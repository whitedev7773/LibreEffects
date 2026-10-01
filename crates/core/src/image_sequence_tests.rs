use super::*;
use std::sync::Arc;
fn sequence() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ImportAsset {
        content: Content::ImageSequence {
            frames: Arc::new((1..=6).map(|n| format!("shot_{n:04}.png")).collect()),
            fps: 2.into(),
            missing: MissingFramePolicy::Error,
            start_frame: 0,
            playback: Default::default(),
        },
        width: 64.0,
        height: 48.0,
        name: "Shot".into(),
        folder: None,
        frame: None,
    })
    .unwrap();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    e
}
#[test]
fn sequence_sampling_retime_split_shift_copy_and_shared_interpretation() {
    let mut e = sequence();
    assert_eq!(e.project().version, 24);
    assert_eq!(
        (
            e.project().composition().duration(),
            e.project().composition().fps()
        ),
        (6, 2.into())
    );
    for f in 0..6 {
        assert_eq!(
            e.selected_layer().unwrap().sequence_frame(f, 2),
            Some(f as usize)
        );
    }
    e.execute(Command::ReverseVideo { id: 1 }).unwrap();
    for f in 0..6 {
        assert_eq!(
            e.selected_layer().unwrap().sequence_frame(f, 2),
            Some(5 - f as usize)
        );
    }
    let reversed = e.project().clone();
    e.execute(Command::SetTimeRemap {
        id: 1,
        enabled: true,
    })
    .unwrap();
    for f in 0..6 {
        assert_eq!(
            e.selected_layer().unwrap().sequence_frame(f, 2),
            Some(5 - f as usize)
        );
    }
    e.execute(Command::FreezeTimeRemap { id: 1, frame: 2 })
        .unwrap();
    for f in 0..6 {
        assert_eq!(e.selected_layer().unwrap().sequence_frame(f, 2), Some(3));
    }
    e.undo();
    e.undo();
    assert_eq!(e.project(), &reversed);
    e.execute(Command::DuplicateComposition).unwrap();
    e.execute(Command::ToggleLocked(2)).unwrap();
    let before = e.project().clone();
    e.execute(Command::InterpretAsset {
        asset: 1,
        interpretation: FootageInterpretation {
            fps: Some(4.into()),
            ..Default::default()
        },
    })
    .unwrap();
    for (_, c) in e.project().compositions() {
        for l in c.layers() {
            assert_eq!(l.footage_interpretation().fps, Some(4.into()));
        }
    }
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    assert_eq!(
        (
            e.project().composition().duration(),
            e.project().composition().fps()
        ),
        (6, 4.into())
    );
}
#[test]
fn manifests_are_shared_portable_versioned_and_relink_is_atomic() {
    let mut e = sequence();
    e.execute(Command::DuplicateComposition).unwrap();
    e.execute(Command::ToggleLocked(2)).unwrap();
    e.execute(Command::SetSequenceMissing {
        asset: 1,
        missing: MissingFramePolicy::Hold,
    })
    .unwrap();
    let before = e.project().clone();
    let paths = Arc::new((1..=6).map(|n| format!("moved/shot_{n:04}.png")).collect());
    e.execute(Command::RelinkSequence {
        asset: 1,
        frames: paths,
    })
    .unwrap();
    let after = e.project().clone();
    let json = after.to_json().unwrap();
    assert_eq!(json.matches("moved/shot_0001.png").count(), 1);
    let p = Project::from_json(&json).unwrap();
    assert_eq!(p, after);
    let Content::ImageSequence { frames: source, .. } = p.asset_library.assets[&1].content() else {
        panic!()
    };
    for (_, c) in p.compositions() {
        for l in c.layers() {
            let Content::ImageSequence {
                frames, missing, ..
            } = l.content()
            else {
                panic!()
            };
            assert!(Arc::ptr_eq(source, frames));
            assert_eq!(*missing, MissingFramePolicy::Hold);
        }
    }
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project(), &after);
    assert!(
        e.execute(Command::RelinkSequence {
            asset: 1,
            frames: Arc::new(vec!["bad.png".into()])
        })
        .is_err()
    );
    assert_eq!(e.project(), &after);
    let mut calls = 0;
    let mapped = p
        .with_video_paths(|v| {
            calls += 1;
            Ok(format!("root/{v}"))
        })
        .unwrap();
    assert_eq!(calls, 6);
    assert_eq!(mapped.version, 24);
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["version"] = 23.into();
    assert!(Project::from_json(&value.to_string()).is_err());
    value["version"] = 24.into();
    value["sequence_assets"] = serde_json::json!({});
    assert!(Project::from_json(&value.to_string()).is_err());
}
#[test]
fn sequence_split_shift_and_cross_fps_clipboard_keep_source_samples() {
    let mut e = sequence();
    e.execute(Command::ConfigureComposition {
        name: "Edit".into(),
        width: 64,
        height: 48,
        fps: 4,
        duration: 24,
    })
    .unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 0,
        end: 12,
    })
    .unwrap();
    let original = e.selected_layer().unwrap().clone();
    e.execute(Command::ShiftLayer { id: 1, delta: 3 }).unwrap();
    let shifted = e.selected_layer().unwrap().clone();
    for f in 0..12 {
        assert_eq!(
            shifted.sequence_frame(f + 3, 4),
            original.sequence_frame(f, 4)
        );
    }
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 7,
    })
    .unwrap();
    let head = e
        .project()
        .composition()
        .layers()
        .iter()
        .find(|l| l.in_frame() == 7)
        .unwrap();
    for f in 7..15 {
        assert_eq!(head.sequence_frame(f, 4), shifted.sequence_frame(f, 4));
    }
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
    let head = head.clone();
    let clip = e.copy_layers(&[head.id()]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "8fps".into(),
        width: 64,
        height: 48,
        fps: 8,
        duration: 48,
    })
    .unwrap();
    e.execute(Command::PasteLayers(clip)).unwrap();
    for f in 7..15 {
        assert_eq!(
            e.selected_layer().unwrap().sequence_frame(f * 2, 8),
            head.sequence_frame(f, 4)
        );
    }
}
