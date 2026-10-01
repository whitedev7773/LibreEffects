use super::*;
fn video(speed: f64) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Video {
            path: "source.mp4".into(),
            duration: 10.0,
            source_fps: 24000.0 / 1001.0,
            start_frame: 0,
            playback: VideoPlayback {
                source_in: 2.0,
                speed,
            },
        },
        width: 128.0,
        height: 72.0,
        name: "Video".into(),
    })
    .unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 12,
        end: 88,
    })
    .unwrap();
    e.execute(Command::ShiftLayer { id: 1, delta: -7 }).unwrap();
    e
}
fn edit(e: &mut Editor, edit: TrackEdit) {
    e.execute(Command::EditTrack {
        id: 1,
        property: PropertyPath::TimeRemap,
        edit,
    })
    .unwrap();
}
fn enable(e: &mut Editor) {
    e.execute(Command::SetTimeRemap {
        id: 1,
        enabled: true,
    })
    .unwrap();
}
#[test]
fn enabling_preserves_shifted_trimmed_speed_reverse_and_frozen_sources() {
    for speed in [0.0, -2.0, -0.5, 0.5, 1.0, 2.0, 100.0] {
        let mut e = video(speed);
        e.current.project.composition.fps = FrameRate::new(30000, 1001).unwrap();
        let before = e.project().clone();
        let original = before.composition().layer(1).unwrap();
        enable(&mut e);
        let l = e.selected_layer().unwrap();
        for f in 5..81 {
            assert_eq!(
                l.video_time(f, before.composition.fps),
                original.content.video_time(f, before.composition.fps),
                "frame={f}, speed={speed}"
            );
        }
        assert_eq!(
            l.time_remap()
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            [5, 80]
        );
        let remapped = e.project().clone();
        assert_eq!(remapped.version, 22);
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &remapped);
        assert_eq!(
            Project::from_json(&remapped.to_json().unwrap()).unwrap(),
            remapped
        );
        e.execute(Command::SetTimeRemap {
            id: 1,
            enabled: false,
        })
        .unwrap();
        assert_eq!(e.selected_layer().unwrap().content(), original.content());
        assert!(e.selected_layer().unwrap().time_remap().is_none());
    }
}
#[test]
fn source_track_supports_ramps_holds_reverse_freeze_and_outside_transparency() {
    let mut e = video(1.0);
    enable(&mut e);
    edit(
        &mut e,
        TrackEdit::Value {
            frame: 5,
            value: 0.0,
        },
    );
    edit(
        &mut e,
        TrackEdit::Value {
            frame: 35,
            value: 2.0,
        },
    );
    edit(
        &mut e,
        TrackEdit::Value {
            frame: 50,
            value: 2.0,
        },
    );
    edit(
        &mut e,
        TrackEdit::Value {
            frame: 80,
            value: 0.0,
        },
    );
    let l = e.selected_layer().unwrap();
    assert_eq!(l.source_time(20, 30), Some(1.0));
    assert_eq!(l.source_time(42, 30), Some(2.0));
    assert_eq!(l.source_time(65, 30), Some(1.0));
    let reverse = e.project().clone();
    let sample = l.video_time(65, 30).unwrap();
    e.execute(Command::FreezeTimeRemap { id: 1, frame: 65 })
        .unwrap();
    let l = e.selected_layer().unwrap();
    for f in 5..81 {
        assert_eq!(l.video_time(f, 30), Some(sample));
    }
    assert_eq!(
        l.time_remap().unwrap().keys()[&65].interpolation,
        Interpolation::Hold
    );
    e.undo();
    assert_eq!(e.project(), &reverse);
    edit(
        &mut e,
        TrackEdit::Interpolate {
            frame: 5,
            interpolation: Interpolation::Hold,
        },
    );
    assert_eq!(e.selected_layer().unwrap().source_time(20, 30), Some(0.0));
    edit(
        &mut e,
        TrackEdit::Interpolate {
            frame: 5,
            interpolation: Interpolation::Bezier(Bezier::default()),
        },
    );
    let value = e.selected_layer().unwrap().source_time(15, 30).unwrap();
    assert!(value > 0.0 && value < 2.0);
    edit(&mut e, TrackEdit::ToggleAnimation { frame: 20 });
    edit(
        &mut e,
        TrackEdit::Value {
            frame: 20,
            value: -1.0,
        },
    );
    assert_eq!(e.selected_layer().unwrap().video_time(20, 30), None);
    edit(
        &mut e,
        TrackEdit::Value {
            frame: 20,
            value: 10.0,
        },
    );
    assert_eq!(e.selected_layer().unwrap().video_time(20, 30), None);
    assert!(
        e.execute(Command::FreezeTimeRemap { id: 1, frame: 20 })
            .is_err()
    );
    edit(
        &mut e,
        TrackEdit::Value {
            frame: 20,
            value: 0.0,
        },
    );
    assert_eq!(e.selected_layer().unwrap().video_time(20, 30), Some(0.0));
}
#[test]
fn nested_fractional_sources_keep_original_sampling_and_can_freeze_and_reverse() {
    let mut e = Editor::default();
    e.execute(Command::ConfigureCompositionRate {
        name: "Source".into(),
        width: 100,
        height: 100,
        fps: FrameRate::new(24000, 1001).unwrap(),
        duration: 48,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureCompositionRate {
        name: "Main".into(),
        width: 100,
        height: 100,
        fps: FrameRate::new(30000, 1001).unwrap(),
        duration: 120,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::AddCompositionLayer {
        composition: 1,
        frame: 10,
    })
    .unwrap();
    let original = e.selected_layer().unwrap().clone();
    let id = original.id();
    e.execute(Command::SetTimeRemap { id, enabled: true })
        .unwrap();
    let source = e.project().composition_by_id(1).unwrap();
    let fps = e.project().composition().fps();
    for f in 10..original.out_frame(120) {
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .composition_frame(f, fps, source),
            original.content().composition_frame(f, fps, source),
            "frame {f}"
        );
    }
    let source_frame = e
        .selected_layer()
        .unwrap()
        .composition_frame(31, fps, source)
        .unwrap();
    e.execute(Command::FreezeTimeRemap { id, frame: 31 })
        .unwrap();
    for f in 10..70 {
        assert_eq!(
            e.selected_layer().unwrap().composition_frame(
                f,
                fps,
                e.project().composition_by_id(1).unwrap()
            ),
            Some(source_frame)
        );
    }
    e.execute(Command::EditTrack {
        id,
        property: PropertyPath::TimeRemap,
        edit: TrackEdit::Value {
            frame: 60,
            value: 0.0,
        },
    })
    .unwrap();
    e.execute(Command::EditTrack {
        id,
        property: PropertyPath::TimeRemap,
        edit: TrackEdit::Interpolate {
            frame: 31,
            interpolation: Interpolation::Linear,
        },
    })
    .unwrap();
    assert!(
        e.selected_layer().unwrap().source_time(45, fps).unwrap()
            < f64::from(source_frame) / (24000.0 / 1001.0)
    );
    e.execute(Command::EditTrack {
        id,
        property: PropertyPath::TimeRemap,
        edit: TrackEdit::Value {
            frame: 90,
            value: 100.0,
        },
    })
    .unwrap();
    assert_eq!(
        e.selected_layer().unwrap().composition_frame(
            90,
            fps,
            e.project().composition_by_id(1).unwrap()
        ),
        None
    );
}
#[test]
fn remap_keys_follow_layer_edits_and_fps_copy_without_changing_seconds() {
    let mut e = video(0.5);
    enable(&mut e);
    edit(
        &mut e,
        TrackEdit::Value {
            frame: 35,
            value: 1.25,
        },
    );
    let original = e.selected_layer().unwrap().clone();
    e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
    let shifted = e.selected_layer().unwrap().clone();
    for f in 5..81 {
        assert_eq!(shifted.source_time(f + 10, 30), original.source_time(f, 30));
    }
    let key = shifted.copy_key(PropertyPath::TimeRemap, 45).unwrap();
    e.execute(Command::PasteKeys {
        keys: vec![key],
        frame: 60,
        target: Some(1),
    })
    .unwrap();
    e.execute(Command::MoveKeys {
        keys: vec![KeyRef {
            id: 1,
            property: PropertyPath::TimeRemap,
            frame: 60,
        }],
        delta: 5,
    })
    .unwrap();
    assert_eq!(e.selected_layer().unwrap().source_time(65, 30), Some(1.25));
    e.execute(Command::DeleteKeys(vec![KeyRef {
        id: 1,
        property: PropertyPath::TimeRemap,
        frame: 65,
    }]))
    .unwrap();
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 50,
    })
    .unwrap();
    assert_eq!(
        e.project().composition().layer(2).unwrap().time_remap(),
        shifted.time_remap()
    );
    let clipboard = e.copy_layers(&[1, 2]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "60 fps".into(),
        width: 1920,
        height: 1080,
        fps: 60,
        duration: 300,
    })
    .unwrap();
    e.execute(Command::PasteLayers(clipboard)).unwrap();
    let pasted = e
        .project()
        .composition()
        .layers()
        .iter()
        .find(|l| l.in_frame() == 30)
        .unwrap();
    for f in 15..50 {
        assert_eq!(pasted.source_time(f * 2, 60), shifted.source_time(f, 30));
    }
    let before = e.project().clone();
    let ids = e
        .project()
        .composition()
        .layers()
        .iter()
        .map(Layer::id)
        .collect();
    e.execute(Command::Precompose {
        layers: ids,
        name: "Retimed".into(),
    })
    .unwrap();
    assert_eq!(
        e.project().composition_by_id(3).unwrap().layers(),
        before.composition().layers()
    );
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
}
#[test]
fn invalid_edits_locked_layers_and_old_versions_are_rejected_atomically() {
    let mut e = video(1.0);
    enable(&mut e);
    let before = e.project().clone();
    for command in [
        Command::EditTrack {
            id: 1,
            property: PropertyPath::TimeRemap,
            edit: TrackEdit::Value {
                frame: 20,
                value: f64::NAN,
            },
        },
        Command::EditTimeRemap {
            id: 1,
            edit: TrackEdit::Value {
                frame: 20,
                value: MAX_SECONDS + 1.0,
            },
        },
        Command::EditTimeRemap {
            id: 1,
            edit: TrackEdit::Value {
                frame: 150,
                value: 2.0,
            },
        },
        Command::EditTimeRemap {
            id: 1,
            edit: TrackEdit::Keyframe {
                from: 5,
                to: 80,
                value: 2.0,
            },
        },
        Command::FreezeTimeRemap { id: 1, frame: 0 },
        Command::SetVideoSpeed { id: 1, speed: 2.0 },
        Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        },
    ] {
        assert!(e.execute(command).is_err());
        assert_eq!(e.project(), &before);
    }
    let mut json: serde_json::Value = serde_json::from_str(&before.to_json().unwrap()).unwrap();
    json["version"] = 20.into();
    assert!(Project::from_json(&json.to_string()).is_err());
    e.execute(Command::ToggleLocked(1)).unwrap();
    let locked = e.project().clone();
    assert!(
        e.execute(Command::SetTimeRemap {
            id: 1,
            enabled: false
        })
        .is_err()
    );
    assert_eq!(e.project(), &locked);
    e.execute(Command::AddRectangle).unwrap();
    let before = e.project().clone();
    assert!(
        e.execute(Command::SetTimeRemap {
            id: 2,
            enabled: true
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
}
