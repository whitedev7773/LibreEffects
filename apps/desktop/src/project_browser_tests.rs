use super::*;
use crate::rendering::Renderer;
use libre_effects_core::{Command, Editor};

fn png(path: &std::path::Path, color: [u8; 4]) {
    image::RgbaImage::from_pixel(32, 32, image::Rgba(color))
        .save(path)
        .unwrap();
}
fn configure(e: &mut Editor) {
    e.execute(Command::ConfigureComposition {
        name: "Asset study".into(),
        width: 64,
        height: 64,
        fps: 30,
        duration: 30,
    })
    .unwrap();
}
#[test]
fn multi_import_is_atomic_and_shared_assets_render_after_unused_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("가.png");
    let b = dir.path().join("나.png");
    png(&a, [240, 30, 10, 128]);
    png(&b, [40, 210, 120, 255]);
    let mut e = Editor::default();
    configure(&mut e);
    let before = e.project().clone();
    assert!(
        crate::editor::assets::read_assets(&[a.clone(), dir.path().join("missing.png")], None)
            .is_err()
    );
    assert_eq!(e.project(), &before);
    let commands =
        crate::editor::assets::read_assets(&[a.clone(), b.clone(), a.clone()], None).unwrap();
    e.execute(Command::Batch(commands)).unwrap();
    let imported = e.project().clone();
    assert_eq!(imported.asset_library().assets().len(), 2);
    assert!(imported.composition().layers().is_empty());
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project(), &imported);
    let path = dir.path().join("unused.lfe.json");
    crate::media_io::save(e.project(), &Default::default(), &path).unwrap();
    std::fs::remove_file(a).unwrap();
    std::fs::remove_file(b).unwrap();
    let project = crate::project_io::read_project(&path).unwrap();
    assert_eq!(project, imported);
    e.replace_project(project).unwrap();
    e.execute(Command::AddAssetLayer { asset: 1, frame: 5 })
        .unwrap();
    let r = Renderer::new();
    assert_eq!(
        r.render(e.project(), 0, 64).unwrap().get_pixel(32, 32)[3],
        0
    );
    let pixels = r.render(e.project(), 5, 64).unwrap();
    assert_eq!(pixels.get_pixel(32, 32).0, [239, 30, 10, 128]);
    assert_eq!(r.render_preview(e.project(), 5, 64).unwrap(), pixels);
    let thumb = thumbnail(e.project(), ProjectItem::Asset(1)).unwrap();
    assert_eq!(thumb.get_pixel(16, 16).0, [10, 30, 240, 128]);
    e.execute(Command::NewProjectFolder {
        name: "Sources".into(),
        parent: None,
    })
    .unwrap();
    e.execute(Command::MoveProjectItem {
        item: ProjectItem::Asset(1),
        folder: Some(3),
    })
    .unwrap();
    e.execute(Command::RenameProjectItem {
        item: ProjectItem::Asset(1),
        name: "Hero".into(),
    })
    .unwrap();
    assert_eq!(r.render(e.project(), 5, 64).unwrap(), pixels);
    crate::media_io::save(e.project(), &Default::default(), &path).unwrap();
    let loaded = crate::project_io::read_project(&path).unwrap();
    assert_eq!(r.render(&loaded, 5, 64).unwrap(), pixels);
    let image_path = dir.path().join("out.png");
    pixels.save(&image_path).unwrap();
    assert_eq!(image::open(image_path).unwrap().to_rgba8(), pixels);
}
#[test]
fn tree_search_sort_and_collapsed_folders_keep_stable_item_identity() {
    let mut e = Editor::default();
    for (name, parent) in [("Media", None), ("Nested", Some(1))] {
        e.execute(Command::NewProjectFolder {
            name: name.into(),
            parent,
        })
        .unwrap();
    }
    for (name, folder, png) in [("Zulu", Some(2), "YWJj"), ("Alpha", None, "ZA==")] {
        e.execute(Command::ImportAsset {
            content: Content::Image { png: png.into() },
            width: 20.0,
            height: 10.0,
            name: name.into(),
            folder,
            frame: None,
        })
        .unwrap();
    }
    let visible = rows(e.project(), "", false, false, &Default::default());
    assert_eq!(visible.iter().find(|r| r.name == "Zulu").unwrap().depth, 2);
    assert_eq!(folder_path(e.project(), Some(2)), "Media / Nested");
    assert!(
        !rows(
            e.project(),
            "",
            false,
            false,
            &std::collections::BTreeSet::from([1])
        )
        .iter()
        .any(|r| r.name == "Zulu")
    );
    assert_eq!(
        rows(
            e.project(),
            "zUL",
            true,
            true,
            &std::collections::BTreeSet::from([1])
        )[0]
        .item,
        ProjectItem::Asset(3)
    );
    let images = rows(e.project(), "image", false, true, &Default::default());
    assert_eq!(
        images.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["Zulu", "Alpha"]
    );
}
#[test]
fn unused_sources_are_collected_protected_and_checked_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("clip.mp4");
    std::fs::write(&source, b"source data").unwrap();
    let mut e = Editor::default();
    configure(&mut e);
    e.execute(Command::ImportAsset {
        content: Content::Video {
            path: source.to_string_lossy().into(),
            duration: 1.0,
            source_fps: 30.0,
            start_frame: 0,
            playback: Default::default(),
        },
        width: 64.0,
        height: 64.0,
        name: "Unused".into(),
        folder: None,
        frame: None,
    })
    .unwrap();
    assert_eq!(crate::media_io::entries(e.project())[0].references, 0);
    assert!(crate::project_io::validate_render(e.project(), &source, &(0..5)).is_err());
    let collected =
        crate::media_io::collect(e.project(), &Default::default(), dir.path(), |_, _| Ok(()))
            .unwrap();
    assert_eq!(collected.files, 1);
    let loaded = crate::project_io::read_project(&collected.project_path).unwrap();
    assert!(
        crate::media_io::video_paths(&loaded)
            .first()
            .unwrap()
            .contains("Media")
    );
    e.execute(Command::ImportAsset {
        content: Content::Image { png: "YWJj".into() },
        width: 10.0,
        height: 10.0,
        name: "Broken unused".into(),
        folder: None,
        frame: None,
    })
    .unwrap();
    assert!(crate::rendering::validate_images(e.project()).is_err());
}
#[test]
#[ignore = "requires FFmpeg; verifies mixed footage import, relink, reuse and MP4/MOV asset exports"]
fn mixed_assets_relink_and_export_the_same_preview_pixels() {
    use crate::video_export::{VideoPreset, export_video, ffmpeg_path};
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("image.png");
    png(&still, [220, 50, 90, 128]);
    let mut src = Editor::default();
    configure(&mut src);
    src.execute(Command::AddContent {
        content: Content::Solid,
        width: 32.0,
        height: 32.0,
        name: "Color".into(),
    })
    .unwrap();
    src.execute(Command::SetColor {
        id: 1,
        color: 0x24c684,
    })
    .unwrap();
    let movie = dir.path().join("source.mov");
    export_video(
        src.project(),
        0..30,
        VideoPreset::ProResAlpha,
        &movie,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let mut e = Editor::default();
    configure(&mut e);
    e.execute(Command::Batch(
        crate::editor::assets::read_assets(&[still, movie.clone()], None).unwrap(),
    ))
    .unwrap();
    assert_eq!(e.project().asset_library().assets().len(), 2);
    e.execute(Command::AddAssetLayer { asset: 2, frame: 0 })
        .unwrap();
    e.execute(Command::DuplicateComposition).unwrap();
    let renamed = dir.path().join("relocated.mov");
    std::fs::rename(&movie, &renamed).unwrap();
    e.execute(Command::RelinkMedia(vec![
        crate::media_io::replacement(movie.to_string_lossy().into(), &renamed).unwrap(),
    ]))
    .unwrap();
    e.execute(Command::SetCompositionBackground(0x183048))
        .unwrap();
    let project_path = dir.path().join("assets.lfe.json");
    crate::media_io::save(e.project(), &Default::default(), &project_path).unwrap();
    let loaded = crate::project_io::read_project(&project_path).unwrap();
    let r = Renderer::new();
    let preview = r.render_preview(&loaded, 5, 64).unwrap();
    assert_eq!(preview, r.render(&loaded, 5, 64).unwrap());
    for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
        let path = dir.path().join(format!("out.{}", preset.extension()));
        export_video(
            &loaded,
            0..15,
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
        let decoded = cmd
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(), 64 * 64 * 4 * 15);
        let mut expected = preview.clone();
        if preset == VideoPreset::H264 {
            crate::rendering::composite_background(&mut expected, 0x183048);
        }
        for (x, y) in [(32, 32), (0, 0)] {
            let offset = (y * 64 + x) * 4;
            let actual = &decoded.stdout[offset..offset + 4];
            let reference = expected.get_pixel(x as u32, y as u32).0;
            assert!((i32::from(actual[3]) - i32::from(reference[3])).abs() <= 2);
            if reference[3] > 0 {
                for c in 0..3 {
                    assert!(
                        (i32::from(actual[c]) - i32::from(reference[c])).abs() <= 8,
                        "{actual:?} != {reference:?}"
                    );
                }
            }
        }
    }
}
