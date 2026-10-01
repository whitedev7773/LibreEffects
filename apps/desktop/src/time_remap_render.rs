//! Retiming must use identical source samples in nested previews, PNG and video exports.
use crate::rendering::Renderer;
use libre_effects_core::{
    Command, Content, Editor, Interpolation, Project, Property, PropertyPath, TrackEdit,
};

fn configure(e: &mut Editor, duration: u32) {
    e.execute(Command::ConfigureComposition {
        name: "Time Remap QA".into(),
        width: 64,
        height: 64,
        fps: 30,
        duration,
    })
    .unwrap();
}
fn source() -> Editor {
    let mut e = Editor::default();
    configure(&mut e, 30);
    for (index, color) in [0xff0000, 0x00ff00, 0x0000ff].into_iter().enumerate() {
        e.execute(Command::AddContent {
            content: Content::Solid,
            width: 48.0,
            height: 48.0,
            name: format!("Color {index}"),
        })
        .unwrap();
        let id = e.selected().unwrap();
        e.execute(Command::SetColor { id, color }).unwrap();
        e.execute(Command::SetValue {
            id,
            property: Property::Opacity,
            frame: 0,
            value: 50.0,
        })
        .unwrap();
        e.execute(Command::SetLayerRange {
            id,
            start: index as u32 * 10,
            end: (index as u32 + 1) * 10,
        })
        .unwrap();
    }
    e
}
fn retime(e: &mut Editor) {
    let id = e.selected().unwrap();
    e.execute(Command::SetLayerRange {
        id,
        start: 0,
        end: 60,
    })
    .unwrap();
    e.execute(Command::SetTimeRemap { id, enabled: true })
        .unwrap();
    for (frame, value) in [(0, 0.0), (15, 0.5), (30, 0.5), (45, 29.0 / 30.0), (59, 0.0)] {
        e.execute(Command::EditTrack {
            id,
            property: PropertyPath::TimeRemap,
            edit: TrackEdit::Value { frame, value },
        })
        .unwrap();
    }
}
fn nested() -> Editor {
    let mut e = source();
    e.execute(Command::NewComposition).unwrap();
    configure(&mut e, 60);
    e.execute(Command::AddCompositionLayer {
        composition: 1,
        frame: 0,
    })
    .unwrap();
    retime(&mut e);
    e
}
const SAMPLES: [(u32, [u8; 4]); 5] = [
    (0, [255, 0, 0, 128]),
    (15, [0, 255, 0, 128]),
    (30, [0, 255, 0, 128]),
    (45, [0, 0, 255, 128]),
    (59, [255, 0, 0, 128]),
];
#[test]
fn nested_preview_png_and_saved_project_use_the_same_retimed_frames() {
    let mut e = nested();
    let r = Renderer::new();
    let project = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for (frame, expected) in SAMPLES {
        let image = r.render(&project, frame, 64).unwrap();
        assert_eq!(image.get_pixel(32, 32).0, expected, "frame {frame}");
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert_eq!(r.render_preview(&project, frame, 64).unwrap(), image);
        let png = dir.path().join(format!("{frame}.png"));
        image.save(&png).unwrap();
        assert_eq!(image::open(&png).unwrap().to_rgba8(), image);
    }
    let id = e.selected().unwrap();
    let before = e.project().clone();
    e.execute(Command::FreezeTimeRemap { id, frame: 45 })
        .unwrap();
    for frame in [0, 15, 30, 45, 59] {
        assert_eq!(
            r.render(e.project(), frame, 64)
                .unwrap()
                .get_pixel(32, 32)
                .0,
            [0, 0, 255, 128]
        );
    }
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(
        r.render(e.project(), 0, 64).unwrap().get_pixel(32, 32).0,
        [0, 0, 255, 128]
    );
}
#[test]
fn outer_animation_uses_composition_time_and_outside_source_is_transparent() {
    let mut e = nested();
    let id = e.selected().unwrap();
    e.execute(Command::FreezeTimeRemap { id, frame: 0 })
        .unwrap();
    e.execute(Command::SetValue {
        id,
        property: Property::Opacity,
        frame: 0,
        value: 0.0,
    })
    .unwrap();
    e.execute(Command::ToggleKeyframe {
        id,
        property: Property::Opacity,
        frame: 0,
    })
    .unwrap();
    e.execute(Command::SetValue {
        id,
        property: Property::Opacity,
        frame: 59,
        value: 100.0,
    })
    .unwrap();
    let r = Renderer::new();
    assert_eq!(
        r.render(e.project(), 0, 64).unwrap().get_pixel(32, 32)[3],
        0
    );
    assert!((63..=66).contains(&r.render(e.project(), 30, 64).unwrap().get_pixel(32, 32)[3]));
    assert_eq!(
        r.render(e.project(), 59, 64).unwrap().get_pixel(32, 32)[3],
        128
    );
    for value in [-1.0, 1.0] {
        e.execute(Command::EditTrack {
            id,
            property: PropertyPath::TimeRemap,
            edit: TrackEdit::Value { frame: 59, value },
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 59, 64).unwrap().get_pixel(32, 32)[3],
            0
        );
    }
    e.execute(Command::SetTimeRemap { id, enabled: false })
        .unwrap();
    assert_eq!(
        r.render(e.project(), 59, 64).unwrap().get_pixel(32, 32)[3],
        0
    );
}
#[test]
fn trimming_and_precomposing_retimed_layers_preserves_rendered_pixels() {
    let mut e = nested();
    let id = e.selected().unwrap();
    let r = Renderer::new();
    let before = e.project().clone();
    e.execute(Command::SplitLayers {
        ids: vec![id],
        frame: 30,
    })
    .unwrap();
    for frame in 0..60 {
        assert_eq!(
            r.render(&before, frame, 64).unwrap(),
            r.render(e.project(), frame, 64).unwrap()
        );
    }
    let ids = e
        .project()
        .composition()
        .layers()
        .iter()
        .map(|l| l.id())
        .collect();
    e.execute(Command::Precompose {
        layers: ids,
        name: "Nested remap".into(),
    })
    .unwrap();
    for (frame, _) in SAMPLES {
        assert_eq!(
            r.render(&before, frame, 64).unwrap(),
            r.render(e.project(), frame, 64).unwrap()
        );
    }
}
#[test]
#[ignore = "requires FFmpeg; tests retimed decoded footage, nested MP4 and alpha MOV"]
fn remapped_footage_and_precompositions_roundtrip_through_video_export() {
    use crate::video_export::{VideoPreset, export_video, ffmpeg_path};
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("source.mov");
    export_video(
        source().project(),
        0..30,
        VideoPreset::ProResAlpha,
        &media,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let mut video = Editor::default();
    configure(&mut video, 60);
    video
        .execute(Command::AddContent {
            content: Content::Video {
                audio: None,
                path: media.to_string_lossy().into_owned(),
                duration: 1.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 64.0,
            height: 64.0,
            name: "Retimed video".into(),
        })
        .unwrap();
    retime(&mut video);
    for (kind, mut e) in [("video", video), ("nested", nested())] {
        // Hold and reverse both need source-time sampling, not a stretched output stream.
        let id = e.selected().unwrap();
        e.execute(Command::EditTrack {
            id,
            property: PropertyPath::TimeRemap,
            edit: TrackEdit::Interpolate {
                frame: 15,
                interpolation: Interpolation::Hold,
            },
        })
        .unwrap();
        e.execute(Command::SetCompositionBackground(0x183048))
            .unwrap();
        let project = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let r = Renderer::new();
        for (frame, expected) in SAMPLES {
            let preview = r.render_preview(&project, frame, 64).unwrap();
            let actual = preview.get_pixel(32, 32).0;
            for c in 0..4 {
                assert!(
                    (i32::from(actual[c]) - i32::from(expected[c])).abs() <= 4,
                    "{kind} frame {frame}: {actual:?}"
                );
            }
        }
        for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
            let output = dir.path().join(format!("{kind}.{}", preset.extension()));
            export_video(
                &project,
                0..60,
                preset,
                &output,
                Default::default(),
                Default::default(),
            )
            .unwrap();
            let mut command = std::process::Command::new(ffmpeg_path());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let decoded = command
                .args(["-v", "error", "-i"])
                .arg(&output)
                .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                .output()
                .unwrap();
            assert!(decoded.status.success());
            assert_eq!(decoded.stdout.len(), 64 * 64 * 4 * 60);
            for (frame, _) in SAMPLES {
                let mut reference = r.render(&project, frame, 64).unwrap();
                if preset == VideoPreset::H264 {
                    crate::rendering::composite_background(&mut reference, 0x183048);
                }
                for (x, y) in [(32, 32), (0, 0)] {
                    let offset = (frame as usize * 64 * 64 + y * 64 + x) * 4;
                    let actual = &decoded.stdout[offset..offset + 4];
                    let expected = reference.get_pixel(x as u32, y as u32).0;
                    assert!((i32::from(actual[3]) - i32::from(expected[3])).abs() <= 2);
                    if expected[3] != 0 {
                        for c in 0..3 {
                            assert!(
                                (i32::from(actual[c]) - i32::from(expected[c])).abs() <= 8,
                                "{kind} {preset:?} {frame}: {actual:?} != {expected:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
