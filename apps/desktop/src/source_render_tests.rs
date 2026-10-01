use super::*;
use crate::rendering::Renderer;
use libre_effects_core::{Command, Content, Editor, Project};

fn encoded(pixel: [u8; 4]) -> String {
    let mut bytes = Vec::new();
    let pixels = image::RgbaImage::from_pixel(64, 48, image::Rgba(pixel));
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(pixels.as_raw(), 64, 48, image::ExtendedColorType::Rgba8)
        .unwrap();
    STANDARD.encode(bytes)
}
fn still(e: &mut Editor, png: String) {
    e.execute(Command::ImportAsset {
        content: Content::Image { png: png.into() },
        width: 64.0,
        height: 48.0,
        name: "Matted".into(),
        folder: None,
        frame: None,
    })
    .unwrap();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
}
fn near(actual: [u8; 4], expected: [u8; 4], tolerance: i32) {
    for c in 0..4 {
        assert!(
            (i32::from(actual[c]) - i32::from(expected[c])).abs() <= tolerance,
            "{actual:?} != {expected:?}"
        );
    }
}
#[test]
fn alpha_interpretation_matches_straight_colors_before_effects_and_scaling() {
    let setting = |alpha, invert_alpha| FootageInterpretation {
        alpha,
        invert_alpha,
        fps: None,
    };
    let mut pixels = image::RgbaImage::from_raw(
        3,
        1,
        vec![100, 50, 25, 128, 128, 128, 128, 0, 100, 200, 250, 255],
    )
    .unwrap();
    apply_alpha(
        &mut pixels,
        setting(AlphaInterpretation::Premultiplied { matte: 0 }, false),
    );
    assert_eq!(pixels.get_pixel(0, 0).0, [199, 100, 50, 128]);
    assert_eq!(pixels.get_pixel(1, 0).0, [0, 0, 0, 0]);
    assert_eq!(pixels.get_pixel(2, 0).0, [100, 200, 250, 255]);
    let mut white = image::RgbaImage::from_pixel(1, 1, image::Rgba([227, 177, 152, 128]));
    apply_alpha(
        &mut white,
        setting(AlphaInterpretation::Premultiplied { matte: 0xffffff }, true),
    );
    assert_eq!(white.get_pixel(0, 0).0, [199, 100, 50, 127]);
    let mut hidden = image::RgbaImage::from_pixel(1, 1, image::Rgba([12, 34, 56, 0]));
    apply_alpha(&mut hidden, setting(AlphaInterpretation::Ignore, true));
    assert_eq!(hidden.get_pixel(0, 0).0, [12, 34, 56, 255]);
    let mut e = Editor::default();
    still(&mut e, encoded([100, 50, 25, 128]));
    let renderer = Renderer::new();
    let before = renderer.render(e.project(), 0, 64).unwrap();
    e.execute(Command::InterpretAsset {
        asset: 1,
        interpretation: setting(AlphaInterpretation::Premultiplied { matte: 0 }, false),
    })
    .unwrap();
    let p = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    for size in [32, 64] {
        let out = renderer.render(&p, 0, size).unwrap();
        near(out.get_pixel(5, 5).0, [199, 100, 50, 128], 2);
        assert_eq!(out, renderer.render_preview(&p, 0, size).unwrap());
    }
    e.undo();
    assert_eq!(before, renderer.render(e.project(), 0, 64).unwrap());
    e.redo();
    e.execute(Command::Effect {
        id: 1,
        edit: libre_effects_core::EffectEdit::Add(libre_effects_core::EffectKind::Fill),
    })
    .unwrap();
    let effect = renderer.render(e.project(), 0, 64).unwrap();
    assert_eq!(effect.get_pixel(5, 5)[3], 128);
    let thumb =
        crate::project_browser::thumbnail(&p, libre_effects_core::ProjectItem::Asset(1)).unwrap();
    near(thumb.get_pixel(5, 5).0, [50, 100, 199, 128], 2);
}
#[test]
#[ignore = "requires FFmpeg; verifies conformed source clocks, alpha interpretation and MP4/MOV output"]
fn interpreted_video_matches_preview_saved_project_and_exports() {
    use crate::video_export::{VideoPreset, export_video, ffmpeg_path};
    let dir = tempfile::tempdir().unwrap();
    // Three colors with alpha in the original source clock; raw premultiplied RGB.
    let mut source = Editor::default();
    still(&mut source, encoded([100, 50, 25, 128]));
    source
        .execute(Command::ConfigureComposition {
            name: "Source".into(),
            width: 64,
            height: 48,
            fps: 4,
            duration: 6,
        })
        .unwrap();
    let path = dir.path().join("alpha.mov");
    export_video(
        source.project(),
        0..6,
        VideoPreset::ProResAlpha,
        &path,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let mut e = Editor::default();
    e.execute(Command::Batch(
        crate::editor::assets::read_assets(&[path], None).unwrap(),
    ))
    .unwrap();
    e.execute(Command::InterpretAsset {
        asset: 1,
        interpretation: FootageInterpretation {
            fps: Some(2.into()),
            alpha: AlphaInterpretation::Premultiplied { matte: 0 },
            invert_alpha: false,
        },
    })
    .unwrap();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    assert_eq!(
        (
            e.project().composition().fps(),
            e.project().composition().duration()
        ),
        (2.into(), 6)
    );
    e.execute(Command::SetCompositionBackground(0x204060))
        .unwrap();
    let saved = dir.path().join("interpreted.lfe.json");
    crate::media_io::save(e.project(), &Default::default(), &saved).unwrap();
    let p = crate::project_io::read_project(&saved).unwrap();
    let renderer = Renderer::new();
    for frame in 0..6 {
        let preview = renderer.render_preview(&p, frame, 64).unwrap();
        near(preview.get_pixel(5, 5).0, [199, 100, 50, 128], 6);
        assert_eq!(preview, renderer.render(&p, frame, 64).unwrap());
        assert!(
            (p.composition().layers()[0]
                .video_decode_time(frame, 2)
                .unwrap()
                - f64::from(frame) / 4.0)
                .abs()
                < 1e-9
        );
    }
    for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
        let path = dir.path().join(format!("out.{}", preset.extension()));
        export_video(
            &p,
            0..6,
            preset,
            &path,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let mut decode = std::process::Command::new(ffmpeg_path());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            decode.creation_flags(0x08000000);
        }
        let out = decode
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout.len(), 64 * 48 * 4 * 6);
        for frame in 0..6 {
            let mut expected = renderer.render(&p, frame, 64).unwrap();
            if preset == VideoPreset::H264 {
                crate::rendering::composite_background(&mut expected, 0x204060);
            }
            let offset = frame as usize * 64 * 48 * 4 + (5 * 64 + 5) * 4;
            near(
                out.stdout[offset..offset + 4].try_into().unwrap(),
                expected.get_pixel(5, 5).0,
                8,
            );
        }
    }
    // Lossless CFR source confirms FPS conformance really changes decoded frame selection.
    let movie = dir.path().join("colors.mkv");
    let mut cmd = std::process::Command::new(ffmpeg_path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    assert!(cmd.args(["-v","error","-f","lavfi","-i","color=red:r=4:s=64x48:d=1.5,drawbox=c=lime:t=fill:enable='gte(t,0.5)',drawbox=c=blue:t=fill:enable='gte(t,1)'","-c:v","ffv1"]).arg(&movie).status().unwrap().success());
    let mut e = Editor::default();
    e.execute(Command::Batch(
        crate::editor::assets::read_assets(&[movie], None).unwrap(),
    ))
    .unwrap();
    e.execute(Command::InterpretAsset {
        asset: 1,
        interpretation: FootageInterpretation {
            fps: Some(2.into()),
            ..Default::default()
        },
    })
    .unwrap();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    for (frame, channel) in [(0, 0), (2, 1), (4, 2), (5, 2)] {
        assert!(
            renderer
                .render(e.project(), frame, 64)
                .unwrap()
                .get_pixel(5, 5)[channel]
                > 240
        );
    }
}
