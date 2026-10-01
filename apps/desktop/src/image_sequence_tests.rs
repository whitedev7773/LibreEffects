use super::*;
use crate::{media_io, project_io, rendering::Renderer};
use libre_effects_core::{Editor, Project, TrackEdit};

fn write(path: &Path, pixel: [u8; 4]) {
    image::RgbaImage::from_pixel(64, 48, image::Rgba(pixel))
        .save(path)
        .unwrap();
}
fn scene(folder: &Path) -> Editor {
    write(&folder.join("shot_0001.png"), [200, 40, 20, 128]);
    write(&folder.join("shot_0003.png"), [20, 40, 200, 128]);
    let mut e = Editor::default();
    e.execute(discover(&folder.join("shot_0001.png"), 2.into(), None).unwrap())
        .unwrap();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    e
}
fn pixel(e: &Project, f: u32) -> [u8; 4] {
    Renderer::new()
        .render(e, f, 64)
        .unwrap()
        .get_pixel(10, 10)
        .0
}
fn near(a: [u8; 4], b: [u8; 4], t: i32) {
    for n in 0..4 {
        assert!(
            (i32::from(a[n]) - i32::from(b[n])).abs() <= t,
            "{a:?} != {b:?}"
        );
    }
}
#[test]
fn sequence_discovery_missing_policy_and_changed_sources_are_explicit() {
    let d = tempfile::tempdir().unwrap();
    let mut e = scene(d.path());
    let output = d.path().join("render.png");
    assert_eq!(e.project().composition().duration(), 3);
    near(pixel(e.project(), 0), [200, 40, 20, 128], 2);
    near(pixel(e.project(), 2), [20, 40, 200, 128], 2);
    assert!(Renderer::new().render(e.project(), 1, 64).is_err());
    assert!(project_io::validate_render(e.project(), &output, &(1..2)).is_err());
    assert!(project_io::validate_render(e.project(), &output, &(0..1)).is_ok());
    assert!(
        project_io::validate_render(e.project(), &d.path().join("shot_0002.png"), &(0..1)).is_err()
    );
    assert!(media_io::collect(e.project(), &Default::default(), d.path(), |_, _| Ok(())).is_err());
    e.execute(Command::SetSequenceMissing {
        asset: 1,
        missing: MissingFramePolicy::Hold,
    })
    .unwrap();
    assert_eq!(pixel(e.project(), 1), pixel(e.project(), 0));
    assert!(project_io::validate_render(e.project(), &output, &(0..3)).is_ok());
    e.undo();
    assert!(Renderer::new().render(e.project(), 1, 64).is_err());
    e.redo();
    e.execute(Command::SetSequenceMissing {
        asset: 1,
        missing: MissingFramePolicy::Transparent,
    })
    .unwrap();
    assert_eq!(pixel(e.project(), 1), [0, 0, 0, 0]);
    e.execute(Command::InterpretAsset {
        asset: 1,
        interpretation: FootageInterpretation {
            fps: Some(4.into()),
            ..Default::default()
        },
    })
    .unwrap();
    near(pixel(e.project(), 1), [20, 40, 200, 128], 2); // frame 1 at 2fps samples source frame 2 at 4fps
    assert_eq!(pixel(e.project(), 2), [0, 0, 0, 0]);
    let imported = e.project().asset_library().assets()[&1].content();
    let Content::ImageSequence { frames, .. } = imported else {
        panic!()
    };
    assert_eq!(frames.len(), 3);
    // The selected frame is the beginning, and numbering gaps keep their duration.
    let Command::ImportAsset {
        content: Content::ImageSequence { frames, .. },
        ..
    } = discover(&d.path().join("shot_0003.png"), 2.into(), None).unwrap()
    else {
        panic!()
    };
    assert_eq!(frames.len(), 1);
    write(&d.path().join("other_9999.png"), [0; 4]);
    image::RgbaImage::new(32, 32)
        .save(d.path().join("shot_0004.png"))
        .unwrap();
    assert!(discover(&d.path().join("shot_0001.png"), 2.into(), None).is_err());
    assert!(numbered(&d.path().join("plain.png")).is_err());
    assert!(numbered(&d.path().join("shot_0001.tiff")).is_err());
    std::fs::remove_file(d.path().join("shot_0001.png")).unwrap();
    let content = e.project().asset_library().assets()[&1].content();
    let Content::ImageSequence { frames, .. } = content else {
        panic!()
    };
    assert_eq!(
        resolve_frame(frames, 0, MissingFramePolicy::Hold).unwrap(),
        None
    );
    image::RgbaImage::new(32, 32)
        .save(d.path().join("shot_0003.png"))
        .unwrap();
    assert!(Renderer::new().render(e.project(), 1, 64).is_err());
}
#[test]
fn sequence_relink_collect_move_save_and_source_protection_keep_pixels() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("Original");
    std::fs::create_dir(&src).unwrap();
    let mut e = scene(&src);
    e.execute(Command::SetSequenceMissing {
        asset: 1,
        missing: MissingFramePolicy::Hold,
    })
    .unwrap();
    e.execute(Command::DuplicateComposition).unwrap();
    e.execute(Command::ToggleLocked(2)).unwrap();
    let before = e.project().clone();
    let replacement = root.path().join("Replacement");
    std::fs::create_dir(&replacement).unwrap();
    write(&replacement.join("shot_0001.png"), [0, 200, 100, 128]);
    write(&replacement.join("shot_0003.png"), [240, 100, 20, 128]);
    let a = e.project().asset_library().assets()[&1].clone();
    e.execute(relocate(1, a.content(), 64, 48, &replacement).unwrap())
        .unwrap();
    near(pixel(e.project(), 0), [0, 200, 100, 128], 2);
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    let saved = root.path().join("sequence.lfe.json");
    media_io::save(e.project(), &Default::default(), &saved).unwrap();
    let json = std::fs::read_to_string(&saved).unwrap();
    assert_eq!(json.matches("Replacement/shot_0001.png").count(), 1);
    let loaded = project_io::read_project(&saved).unwrap();
    assert_eq!(pixel(&loaded, 1), pixel(e.project(), 1));
    let source = replacement.join("shot_0001.png");
    assert!(project_io::validate_render(&loaded, &source, &(0..1)).is_err());
    assert!(media_io::save(&loaded, &Default::default(), &source).is_err());
    let alias = root.path().join("alias.png");
    std::fs::hard_link(&source, &alias).unwrap();
    assert!(project_io::validate_render(&loaded, &alias, &(0..1)).is_err());
    let collected =
        media_io::collect(&loaded, &Default::default(), root.path(), |_, _| Ok(())).unwrap();
    assert_eq!(collected.files, 2);
    let moved = root.path().join("Moved");
    std::fs::rename(collected.project_path.parent().unwrap(), &moved).unwrap();
    std::fs::remove_dir_all(&replacement).unwrap();
    std::fs::remove_dir_all(&src).unwrap();
    let p = project_io::read_project(&moved.join("project.lfe.json")).unwrap();
    for f in 0..3 {
        assert_eq!(
            pixel(&p, f),
            if f < 2 {
                [0, 199, 100, 128]
            } else {
                [239, 100, 20, 128]
            }
        );
    }
    assert!(project_io::validate_render(&p, &root.path().join("out.mp4"), &(0..3)).is_ok());
    assert_eq!(
        media_io::entries(&p)
            .iter()
            .filter(|entry| entry.offline)
            .count(),
        1
    );
}
#[test]
fn remapped_precompositions_preflight_only_the_source_intervals_they_sample() {
    let d = tempfile::tempdir().unwrap();
    let mut e = scene(d.path());
    let source = e.project().active_composition_id();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "Parent".into(),
        width: 64,
        height: 48,
        fps: 2,
        duration: 3,
    })
    .unwrap();
    e.execute(Command::AddCompositionLayer {
        composition: source,
        frame: 0,
    })
    .unwrap();
    let id = e.selected().unwrap();
    e.execute(Command::SetTimeRemap { id, enabled: true })
        .unwrap();
    let output = d.path().join("out.mp4");
    assert!(project_io::validate_render(e.project(), &output, &(1..2)).is_err());
    e.execute(Command::FreezeTimeRemap { id, frame: 2 })
        .unwrap();
    assert!(project_io::validate_render(e.project(), &output, &(0..3)).is_ok());
    near(pixel(e.project(), 1), [20, 40, 200, 128], 2);
    e.execute(Command::EditTimeRemap {
        id,
        edit: TrackEdit::Value {
            frame: 2,
            value: 0.5,
        },
    })
    .unwrap();
    assert!(project_io::validate_render(e.project(), &output, &(0..1)).is_err());
    e.undo();
    assert!(project_io::validate_render(e.project(), &output, &(0..3)).is_ok());
}
#[test]
#[ignore = "requires FFmpeg; checks numbered source frames, gaps, alpha and composition background in MP4/MOV"]
fn sequence_preview_and_video_exports_match_after_reopen() {
    use crate::video_export::{VideoPreset, export_video, ffmpeg_path};
    let d = tempfile::tempdir().unwrap();
    let mut e = scene(d.path());
    e.execute(Command::SetSequenceMissing {
        asset: 1,
        missing: MissingFramePolicy::Transparent,
    })
    .unwrap();
    e.execute(Command::SetCompositionBackground(0x204060))
        .unwrap();
    let save = d.path().join("sequence.lfe.json");
    media_io::save(e.project(), &Default::default(), &save).unwrap();
    let p = project_io::read_project(&save).unwrap();
    for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
        let path = d.path().join(format!("out.{}", preset.extension()));
        export_video(
            &p,
            0..3,
            preset,
            &path,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let mut cmd = std::process::Command::new(ffmpeg_path());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let out = cmd
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout.len(), 64 * 48 * 4 * 3);
        for f in 0..3 {
            let mut expected = Renderer::new().render_preview(&p, f, 64).unwrap();
            assert_eq!(expected, Renderer::new().render(&p, f, 64).unwrap());
            if preset == VideoPreset::H264 {
                crate::rendering::composite_background(&mut expected, 0x204060);
            }
            let offset = f as usize * 64 * 48 * 4 + (10 * 64 + 10) * 4;
            near(
                out.stdout[offset..offset + 4].try_into().unwrap(),
                expected.get_pixel(10, 10).0,
                8,
            );
        }
    }
}
