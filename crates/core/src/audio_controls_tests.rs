use super::*;
fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Audio {
            path: "voice.wav".into(),
            audio: AudioMetadata {
                stream_index: 0,
                sample_rate: 48000,
                channels: 2,
                channel_layout: "stereo".into(),
                duration: 5.0,
                start_time: 0.0,
                file_offset: 0.0,
            },
            start_frame: 0,
            playback: Default::default(),
        },
        width: 1.0,
        height: 1.0,
        name: "Voice".into(),
    })
    .unwrap();
    e
}
fn edit(e: &mut Editor, p: AudioParam, edit: TrackEdit) {
    e.execute(Command::EditTrack {
        id: 1,
        property: PropertyPath::Audio(p),
        edit,
    })
    .unwrap();
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
#[test]
fn audio_levels_pan_fade_and_mute_follow_history_and_document_versions() {
    let mut e = scene();
    assert_eq!(e.project().version, 25);
    assert_eq!(
        e.selected_layer().unwrap().audio_matrix(0.0),
        [1.0, 0.0, 0.0, 1.0]
    );
    let before = e.project().clone();
    edit(
        &mut e,
        AudioParam::LeftLevel,
        TrackEdit::Value {
            frame: 0,
            value: -6.020599913279624,
        },
    );
    let changed = e.project().clone();
    assert_eq!(changed.version, 26);
    near(
        changed.composition().layer(1).unwrap().audio_matrix(0.0)[0],
        0.5,
    );
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project(), &changed);
    edit(
        &mut e,
        AudioParam::Pan,
        TrackEdit::Value {
            frame: 0,
            value: 100.0,
        },
    );
    let m = e.selected_layer().unwrap().audio_matrix(0.0);
    assert_eq!(m[..2], [0.0, 0.0]);
    near(m[2], 0.5);
    near(m[3], 1.0);
    e.execute(Command::SetAudioEnabled {
        id: 1,
        enabled: false,
    })
    .unwrap();
    assert_eq!(e.selected_layer().unwrap().audio_matrix(0.0), [0.0; 4]);
    let json = e.project().to_json().unwrap();
    let restored = Project::from_json(&json).unwrap();
    assert_eq!(&restored, e.project());
    let mut invalid: serde_json::Value = serde_json::from_str(&json).unwrap();
    invalid["version"] = 25.into();
    assert!(Project::from_json(&invalid.to_string()).is_err());
    // Older projects have an identity mix and do not acquire new serialized fields.
    assert!(!before.to_json().unwrap().contains("audio_controls"));
}
#[test]
fn audio_keyframes_split_move_copy_and_fades_preserve_automation() {
    let mut e = scene();
    e.execute(Command::FadeAudio {
        id: 1,
        start: 0,
        end: 30,
        fade_in: true,
    })
    .unwrap();
    e.execute(Command::FadeAudio {
        id: 1,
        start: 120,
        end: 149,
        fade_in: false,
    })
    .unwrap();
    near(e.selected_layer().unwrap().audio_matrix(15.0)[0], 0.5);
    assert_eq!(e.selected_layer().unwrap().audio_matrix(149.0), [0.0; 4]);
    edit(
        &mut e,
        AudioParam::Pan,
        TrackEdit::ToggleAnimation { frame: 0 },
    );
    edit(
        &mut e,
        AudioParam::Pan,
        TrackEdit::Value {
            frame: 30,
            value: 100.0,
        },
    );
    let copy = e
        .selected_layer()
        .unwrap()
        .copy_key(PropertyPath::Audio(AudioParam::Pan), 30)
        .unwrap();
    e.execute(Command::PasteKeys {
        keys: vec![copy],
        frame: 60,
        target: Some(1),
    })
    .unwrap();
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 75,
    })
    .unwrap();
    let p = e.project();
    for f in [0.0, 15.0, 30.0, 120.0, 149.0] {
        assert_eq!(
            p.composition().layer(1).unwrap().audio_matrix(f),
            p.composition().layer(2).unwrap().audio_matrix(f)
        );
    }
    // Remove late fade keys before shifting, which correctly refuses keys past duration.
    let refs = [120, 149].map(|frame| KeyRef {
        id: 1,
        property: PropertyPath::Audio(AudioParam::Fade),
        frame,
    });
    e.execute(Command::DeleteKeys(refs.into())).unwrap();
    e.execute(Command::ShiftLayer { id: 1, delta: 5 }).unwrap();
    let layer = e.project().composition().layer(1).unwrap();
    assert!(
        layer
            .track(PropertyPath::Audio(AudioParam::Pan))
            .unwrap()
            .keys()
            .contains_key(&65)
    );
    assert!(
        layer
            .track(PropertyPath::Audio(AudioParam::Fade))
            .unwrap()
            .keys()
            .contains_key(&35)
    );
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
}
#[test]
fn invalid_audio_edits_do_not_mutate_document_and_locked_layers_are_protected() {
    let mut e = scene();
    for (p, v) in [
        (AudioParam::LeftLevel, 12.01),
        (AudioParam::RightLevel, -193.0),
        (AudioParam::Pan, 101.0),
        (AudioParam::Fade, f64::NAN),
    ] {
        let before = e.project().clone();
        assert!(
            e.execute(Command::EditTrack {
                id: 1,
                property: PropertyPath::Audio(p),
                edit: TrackEdit::Value { frame: 0, value: v }
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }
    let before = e.project().clone();
    assert!(
        e.execute(Command::FadeAudio {
            id: 1,
            start: 30,
            end: 150,
            fade_in: false
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
    e.execute(Command::ToggleLocked(1)).unwrap();
    assert!(
        e.execute(Command::SetAudioEnabled {
            id: 1,
            enabled: false
        })
        .is_err()
    );
    e.execute(Command::AddRectangle).unwrap();
    assert!(
        e.execute(Command::SetAudioEnabled {
            id: 2,
            enabled: false
        })
        .is_err()
    );
}

#[test]
fn audio_automation_resamples_with_clipboard_and_roundtrips_mixed_media() {
    let mut e = scene();
    e.execute(Command::FadeAudio {
        id: 1,
        start: 0,
        end: 30,
        fade_in: true,
    })
    .unwrap();
    let clipboard = e.copy_layers(&[1]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "60 fps".into(),
        width: 64,
        height: 48,
        fps: 60,
        duration: 300,
    })
    .unwrap();
    e.execute(Command::PasteLayers(clipboard)).unwrap();
    let layer = e.selected_layer().unwrap();
    assert_eq!(
        layer
            .track(PropertyPath::Audio(AudioParam::Fade))
            .unwrap()
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![0, 60]
    );
    near(layer.audio_matrix(30.0)[0], 0.5);
    e.execute(Command::AddContent {
        content: Content::Image { png: "YWJj".into() },
        width: 1.0,
        height: 1.0,
        name: "Image".into(),
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::ImageSequence {
            frames: std::sync::Arc::new(vec!["frame1.png".into(), "frame2.png".into()]),
            fps: 30.into(),
            missing: Default::default(),
            start_frame: 0,
            playback: Default::default(),
        },
        width: 1.0,
        height: 1.0,
        name: "Sequence".into(),
    })
    .unwrap();
    let saved = e.project().to_json().unwrap();
    assert!(saved.contains("image_assets") && saved.contains("sequence_assets"));
    assert_eq!(Project::from_json(&saved).unwrap(), *e.project());
}
