use super::*;
fn video(e: &mut Editor) {
    e.execute(Command::ImportAsset {
        content: Content::Video {
            audio: None,
            path: "source.mov".into(),
            duration: 1.5,
            source_fps: 4.0,
            start_frame: 0,
            playback: Default::default(),
        },
        name: "Source".into(),
        width: 64.0,
        height: 48.0,
        folder: None,
        frame: Some(0),
    })
    .unwrap();
}
fn conform(e: &mut Editor, fps: u32) {
    e.execute(Command::InterpretAsset {
        asset: 1,
        interpretation: FootageInterpretation {
            fps: Some(fps.into()),
            ..Default::default()
        },
    })
    .unwrap();
}
#[test]
fn interpretation_changes_shared_sampling_but_preserves_timeline_and_history() {
    let mut e = Editor::default();
    video(&mut e);
    e.execute(Command::TrimLayers {
        ids: vec![1],
        frame: 30,
        start: false,
    })
    .unwrap();
    e.execute(Command::SetValue {
        id: 1,
        property: Property::Rotation,
        frame: 15,
        value: 42.0,
    })
    .unwrap();
    e.execute(Command::DuplicateComposition).unwrap();
    e.execute(Command::ToggleLocked(2)).unwrap();
    let before = e.project().clone();
    conform(&mut e, 2);
    for (_, comp) in e.project().compositions() {
        let l = &comp.layers[0];
        assert_eq!(l.video_time(30, 30), Some(1.0));
        assert_eq!(l.video_decode_time(30, 30), Some(0.5));
        assert_eq!(l.video_decode_time(89, 30), Some(1.25));
        assert_eq!(l.video_decode_time(90, 30), None);
        let old = before
            .composition_by_id(if l.id == 1 { 1 } else { 2 })
            .unwrap();
        assert_eq!(l.properties, old.layers[0].properties);
        assert_eq!(
            (l.in_frame, l.out_frame),
            (old.layers[0].in_frame, old.layers[0].out_frame)
        );
    }
    let interpreted = e.project().clone();
    assert_eq!(interpreted.version, 23);
    assert_eq!(
        Project::from_json(&interpreted.to_json().unwrap()).unwrap(),
        interpreted
    );
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project(), &interpreted);
    e.execute(Command::RelinkMedia(vec![MediaReplacement {
        audio: None,
        original: "source.mov".into(),
        path: "new.mov".into(),
        width: 64,
        height: 48,
        duration: 2.0,
        fps: 8.0,
    }]))
    .unwrap();
    for (_, comp) in e.project().compositions() {
        assert_eq!(comp.layers[0].video_decode_time(30, 30), Some(0.25));
        assert_eq!(comp.layers[0].footage_interpretation.fps, Some(2.into()));
    }
}
#[test]
fn conformed_reverse_freeze_and_time_remap_use_interpreted_seconds() {
    let mut e = Editor::default();
    video(&mut e);
    conform(&mut e, 2);
    e.execute(Command::TrimLayers {
        ids: vec![1],
        frame: 89,
        start: false,
    })
    .unwrap();
    let before = e.selected_layer().unwrap().clone();
    e.execute(Command::ReverseVideo { id: 1 }).unwrap();
    for frame in 0..90 {
        assert_eq!(
            e.selected_layer().unwrap().video_decode_time(frame, 30),
            before.video_decode_time(89 - frame, 30)
        );
    }
    e.undo();
    for command in [
        Command::FreezeVideo { id: 1, frame: 40 },
        Command::FreezeTimeRemap { id: 1, frame: 40 },
    ] {
        e.execute(command).unwrap();
        for frame in 0..90 {
            assert_eq!(
                e.selected_layer().unwrap().video_decode_time(frame, 30),
                Some(0.5)
            );
        }
        e.undo();
    }
    e.execute(Command::SetTimeRemap {
        id: 1,
        enabled: true,
    })
    .unwrap();
    for frame in 0..90 {
        assert_eq!(
            e.selected_layer().unwrap().video_decode_time(frame, 30),
            before.video_decode_time(frame, 30)
        );
    }
    e.execute(Command::EditTimeRemap {
        id: 1,
        edit: TrackEdit::Value {
            frame: 30,
            value: 2.0,
        },
    })
    .unwrap();
    assert_eq!(
        e.selected_layer().unwrap().video_decode_time(30, 30),
        Some(1.0)
    );
    let copy = e.copy_layers(&[1]).unwrap();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "Paste".into(),
        width: 64,
        height: 48,
        fps: 60,
        duration: 300,
    })
    .unwrap();
    e.execute(Command::PasteLayers(copy)).unwrap();
    assert_eq!(
        e.selected_layer().unwrap().video_decode_time(60, 60),
        Some(1.0)
    );
}
#[test]
fn composition_from_source_matches_interpretation_and_folder_in_one_undo() {
    let mut e = Editor::default();
    video(&mut e);
    conform(&mut e, 2);
    e.execute(Command::NewProjectFolder {
        name: "Footage".into(),
        parent: None,
    })
    .unwrap();
    e.execute(Command::MoveProjectItem {
        item: ProjectItem::Asset(1),
        folder: Some(2),
    })
    .unwrap();
    let before = e.project().clone();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    let p = e.project();
    let c = p.composition();
    assert_eq!(
        (c.width(), c.height(), c.fps(), c.duration()),
        (64, 48, 2.into(), 6)
    );
    assert_eq!(c.name(), "Source");
    assert_eq!(
        p.asset_library.composition_folder(p.composition_id),
        Some(2)
    );
    assert_eq!(c.layers[0].asset_id(), Some(1));
    assert_eq!(c.layers[0].video_decode_time(5, c.fps()), Some(1.25));
    assert!(c.layers[0].active_at(5, c.duration()));
    let new = p.clone();
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project(), &new);
    for fps in [24_000, 30_000, 60_000] {
        let mut e = Editor::default();
        e.execute(Command::ImportAsset {
            content: Content::Video {
                audio: None,
                path: "ntsc.mov".into(),
                duration: 1001.0 / f64::from(fps) * 120.0,
                source_fps: f64::from(fps) / 1001.0,
                start_frame: 0,
                playback: Default::default(),
            },
            name: "NTSC".into(),
            width: 64.0,
            height: 48.0,
            folder: None,
            frame: None,
        })
        .unwrap();
        e.execute(Command::CompositionFromAsset(1)).unwrap();
        assert_eq!(
            e.project().composition().fps(),
            FrameRate::new(fps, 1001).unwrap()
        );
        assert_eq!(e.project().composition().duration(), 120);
    }
}
#[test]
fn still_alpha_interpretation_survives_source_reuse_and_rejects_invalid_data() {
    let mut e = Editor::default();
    e.execute(Command::ImportAsset {
        content: Content::Image { png: "YWJj".into() },
        width: 32.0,
        height: 24.0,
        name: "Still".into(),
        folder: None,
        frame: None,
    })
    .unwrap();
    let setting = FootageInterpretation {
        alpha: AlphaInterpretation::Premultiplied { matte: 0x204060 },
        invert_alpha: true,
        fps: None,
    };
    e.execute(Command::InterpretAsset {
        asset: 1,
        interpretation: setting,
    })
    .unwrap();
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    assert_eq!(
        e.selected_layer().unwrap().footage_interpretation(),
        setting
    );
    let saved = e.project().clone();
    for setting in [
        FootageInterpretation {
            fps: Some(24.into()),
            ..setting
        },
        FootageInterpretation {
            alpha: AlphaInterpretation::Premultiplied { matte: 0x1ffffff },
            ..setting
        },
    ] {
        assert!(
            e.execute(Command::InterpretAsset {
                asset: 1,
                interpretation: setting
            })
            .is_err()
        );
        assert_eq!(e.project(), &saved);
    }
    for mode in 0..3 {
        let mut json = serde_json::to_value(&saved).unwrap();
        match mode {
            0 => json["version"] = 22.into(),
            1 => json["composition"]["layers"][0]
                .as_object_mut()
                .unwrap()
                .remove("footage_interpretation")
                .map(|_| ())
                .unwrap(),
            _ => json["version"] = u32::MAX.into(),
        }
        assert!(Project::from_json(&json.to_string()).is_err());
    }
    e.execute(Command::SetContent {
        id: e.selected_layer().unwrap().id(),
        content: Content::Rectangle,
    })
    .unwrap();
    assert_eq!(
        e.selected_layer().unwrap().footage_interpretation(),
        FootageInterpretation::default()
    );
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
}
