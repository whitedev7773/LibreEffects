use super::*;
fn image() -> Content {
    Content::Image { png: "YWJj".into() }
}
fn video() -> Content {
    Content::Video {
        path: "old.mp4".into(),
        duration: 2.0,
        source_fps: 30.0,
        start_frame: -5,
        playback: VideoPlayback {
            source_in: 0.5,
            speed: -0.5,
        },
    }
}
fn import(content: Content, frame: Option<Frame>, folder: Option<FolderId>) -> Command {
    Command::ImportAsset {
        content,
        width: 64.0,
        height: 48.0,
        name: "Footage".into(),
        folder,
        frame,
    }
}
#[test]
fn imported_assets_outlive_layers_share_payload_and_reuse_identity_after_reopening() {
    let mut e = Editor::default();
    e.execute(import(image(), None, None)).unwrap();
    assert!(e.project().composition().layers().is_empty());
    e.execute(import(image(), Some(20), None)).unwrap();
    e.execute(Command::AddAssetLayer {
        asset: 1,
        frame: 40,
    })
    .unwrap();
    assert_eq!(e.project().asset_library.assets.len(), 1);
    assert_eq!(e.project().asset_references(1), 2);
    assert_eq!(e.selected_layer().unwrap().in_frame(), 40);
    e.execute(Command::DuplicateComposition).unwrap();
    assert_eq!(e.project().asset_references(1), 4);
    let before = e.project().clone();
    assert!(
        e.execute(Command::DeleteProjectItem(ProjectItem::Asset(1)))
            .is_err()
    );
    assert_eq!(e.project(), &before);
    let json = before.to_json().unwrap();
    assert_eq!(json.matches("YWJj").count(), 1);
    assert_eq!(Project::from_json(&json).unwrap(), before);
    e.execute(Command::RenameProjectItem {
        item: ProjectItem::Asset(1),
        name: "Renamed.png".into(),
    })
    .unwrap();
    assert!(!before.same_document(e.project()));
    assert_eq!(e.selected_layer().unwrap().name(), "Footage");
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project().asset_library.assets[&1].name(), "Renamed.png");

    let mut e = Editor::default();
    e.execute(import(image(), Some(0), None)).unwrap();
    let clipboard = e.copy_layers(&[1]).unwrap();
    e.execute(Command::RemoveLayer(1)).unwrap();
    assert_eq!(e.project().asset_library.assets.len(), 1);
    e.execute(Command::DeleteProjectItem(ProjectItem::Asset(1)))
        .unwrap();
    e.execute(Command::PasteLayers(clipboard)).unwrap();
    assert_eq!(e.selected_layer().unwrap().asset_id(), Some(2));
    assert_eq!(e.project().asset_library.assets.len(), 1);
}
#[test]
fn folders_rename_move_copy_and_undo_without_changing_layer_content() {
    let mut e = Editor::default();
    e.execute(Command::NewProjectFolder {
        name: "Media".into(),
        parent: None,
    })
    .unwrap();
    e.execute(Command::NewProjectFolder {
        name: "Images".into(),
        parent: Some(1),
    })
    .unwrap();
    e.execute(import(image(), Some(0), Some(2))).unwrap();
    e.execute(Command::MoveProjectItem {
        item: ProjectItem::Composition(1),
        folder: Some(2),
    })
    .unwrap();
    let before = e.project().clone();
    for command in [
        Command::MoveProjectItem {
            item: ProjectItem::Folder(1),
            folder: Some(2),
        },
        Command::MoveProjectItem {
            item: ProjectItem::Asset(3),
            folder: Some(99),
        },
        Command::DeleteProjectItem(ProjectItem::Folder(2)),
        Command::RenameProjectItem {
            item: ProjectItem::Asset(3),
            name: "".into(),
        },
    ] {
        assert!(e.execute(command).is_err());
        assert_eq!(e.project(), &before);
    }
    e.execute(Command::DuplicateComposition).unwrap();
    assert_eq!(e.project().asset_library.composition_folder(2), Some(2));
    e.execute(Command::DeleteComposition).unwrap();
    assert_eq!(e.project().asset_library.composition_folder(2), None);
    e.undo();
    assert_eq!(e.project().asset_library.composition_folder(2), Some(2));
    e.undo();
    assert_eq!(e.project(), &before);
    e.execute(Command::MoveProjectItem {
        item: ProjectItem::Asset(3),
        folder: None,
    })
    .unwrap();
    assert_eq!(
        e.selected_layer().unwrap().content(),
        before.composition.layers[0].content()
    );
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
}
#[test]
fn old_projects_migrate_sources_without_changing_trim_speed_or_image_memory() {
    let mut e = Editor::default();
    let mut content = video();
    if let Content::Video { start_frame, .. } = &mut content {
        *start_frame = 0;
    }
    e.execute(Command::AddContent {
        content,
        width: 64.0,
        height: 48.0,
        name: "Movie".into(),
    })
    .unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 5,
        end: 60,
    })
    .unwrap();
    e.execute(Command::ShiftLayer { id: 1, delta: -5 }).unwrap();
    e.execute(Command::DuplicateLayers(vec![1])).unwrap();
    let mut legacy = serde_json::to_value(e.project()).unwrap();
    legacy["version"] = 5.into();
    legacy.as_object_mut().unwrap().remove("asset_library");
    for layer in legacy["composition"]["layers"].as_array_mut().unwrap() {
        layer.as_object_mut().unwrap().remove("asset");
    }
    let loaded = Project::from_json(&legacy.to_string()).unwrap();
    assert_eq!(loaded.version, 22);
    assert_eq!(loaded.asset_library.assets.len(), 1);
    for (a, b) in loaded
        .composition
        .layers
        .iter()
        .zip(e.project().composition.layers.iter())
    {
        assert_eq!(a.content(), b.content());
        for frame in 0..60 {
            assert_eq!(a.video_time(frame, 30), b.video_time(frame, 30));
        }
    }
    assert_eq!(
        Project::from_json(&loaded.to_json().unwrap()).unwrap(),
        loaded
    );
}
#[test]
fn unused_sources_relink_collect_and_undo_with_every_used_instance() {
    let mut e = Editor::default();
    e.execute(import(video(), None, None)).unwrap();
    let before = e.project().clone();
    e.execute(Command::RelinkMedia(vec![MediaReplacement {
        original: "old.mp4".into(),
        path: "new.mp4".into(),
        duration: 3.0,
        fps: 24.0,
        width: 64,
        height: 48,
    }]))
    .unwrap();
    assert!(
        matches!(e.project().asset_library.assets[&1].content(), Content::Video { path, .. } if path == "new.mp4")
    );
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    e.execute(Command::AddAssetLayer {
        asset: 1,
        frame: 15,
    })
    .unwrap();
    e.execute(Command::SetVideoSpeed { id: 1, speed: 0.5 })
        .unwrap();
    let timing = e.selected_layer().unwrap().source_time(30, 30);
    e.execute(Command::DuplicateComposition).unwrap();
    let mut calls = 0;
    let collected = e
        .project()
        .with_video_paths(|path| {
            calls += 1;
            assert_eq!(path, "new.mp4");
            Ok("Media/new.mp4".into())
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(collected.composition.layers[0].source_time(30, 30), timing);
    assert_eq!(
        Project::from_json(&collected.to_json().unwrap()).unwrap(),
        collected
    );
}
#[test]
fn invalid_asset_documents_and_atomic_import_batches_are_rejected() {
    let mut e = Editor::default();
    e.execute(import(image(), Some(0), None)).unwrap();
    let before = e.project().clone();
    assert!(
        e.execute(Command::Batch(vec![
            import(video(), None, None),
            Command::NewProjectFolder {
                name: "Invalid".into(),
                parent: Some(900)
            }
        ]))
        .is_err()
    );
    assert_eq!(e.project(), &before);
    for mode in 0..5 {
        let mut json = serde_json::to_value(&before).unwrap();
        match mode {
            0 => json["version"] = 21.into(),
            1 => json["composition"]["layers"][0]["asset"] = 900.into(),
            2 => json["composition"]["layers"][0]["width"] = 65.into(),
            3 => json["asset_library"]["assets"]["1"]["folder"] = 900.into(),
            _ => json["composition"]["layers"][0]
                .as_object_mut()
                .unwrap()
                .remove("asset")
                .map(|_| ())
                .unwrap(),
        }
        assert!(
            Project::from_json(&json.to_string()).is_err(),
            "mode {mode}"
        );
    }
}
