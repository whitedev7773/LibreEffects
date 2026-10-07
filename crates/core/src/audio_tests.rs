use super::*;
fn metadata() -> AudioMetadata {
    AudioMetadata {
        stream_index: 0,
        sample_rate: 48000,
        channels: 2,
        channel_layout: "stereo".into(),
        duration: 5.0,
        start_time: 0.0,
        file_offset: 0.0,
    }
}
fn editor() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ImportAsset {
        content: Content::Audio {
            path: "sound.wav".into(),
            audio: metadata(),
            start_frame: 0,
            playback: Default::default(),
        },
        width: 1.0,
        height: 1.0,
        name: "Sound".into(),
        folder: None,
        frame: None,
    })
    .unwrap();
    e.execute(Command::AddAssetLayer {
        asset: 1,
        frame: 12,
    })
    .unwrap();
    e
}
fn time(e: &Editor, id: LayerId, frame: u32) -> f64 {
    e.project()
        .composition()
        .layer(id)
        .unwrap()
        .audio_source_time(frame, e.project().composition().fps())
        .unwrap()
}
#[test]
fn audio_assets_follow_trim_move_split_history_and_serialization() {
    let mut e = editor();
    assert_eq!(e.project().version, 25);
    assert_eq!(time(&e, 1, 42), 1.0);
    e.execute(Command::TrimLayers {
        ids: vec![1],
        frame: 27,
        start: true,
    })
    .unwrap();
    assert_eq!(time(&e, 1, 42), 1.0);
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 27,
        end: 120,
    })
    .unwrap();
    e.execute(Command::ShiftLayer { id: 1, delta: 9 }).unwrap();
    assert_eq!(time(&e, 1, 51), 1.0);
    let before = e.project().clone();
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 66,
    })
    .unwrap();
    assert_eq!(time(&e, 2, 66), 1.5);
    assert_eq!(e.project().asset_references(1), 2);
    let split = e.project().clone();
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project(), &split);
    let json = split.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), split);
    let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
    old["version"] = 24.into();
    assert!(Project::from_json(&old.to_string()).is_err());
    let mut rewrites = 0;
    let portable = split
        .with_video_paths(|p| {
            assert_eq!(p, "sound.wav");
            rewrites += 1;
            Ok("Media/sound.wav".into())
        })
        .unwrap();
    assert_eq!(rewrites, 1);
    assert_eq!(
        portable.composition.layers()[0].content().linked_paths(),
        &["Media/sound.wav"]
    );
}
#[test]
fn audio_relink_updates_all_instances_and_rejects_invalid_metadata_atomically() {
    let mut e = editor();
    e.execute(Command::DuplicateComposition).unwrap();
    let before = e.project().clone();
    let mut audio = metadata();
    audio.sample_rate = 44100;
    audio.channels = 1;
    audio.duration = 2.0;
    let replacement = MediaReplacement {
        original: "sound.wav".into(),
        path: "new.wav".into(),
        width: 1,
        height: 1,
        duration: 2.0,
        fps: 1.0,
        audio: Some(audio.clone()),
    };
    e.execute(Command::RelinkMedia(vec![replacement.clone()]))
        .unwrap();
    for (_, c) in e.project().compositions() {
        assert_eq!(c.layers()[0].content().audio(), Some(("new.wav", &audio)));
    }
    e.undo();
    assert_eq!(e.project(), &before);
    let mut invalid = replacement;
    invalid.audio.as_mut().unwrap().sample_rate = 0;
    assert!(e.execute(Command::RelinkMedia(vec![invalid])).is_err());
    assert_eq!(e.project(), &before);
    e.execute(Command::CompositionFromAsset(1)).unwrap();
    let comp = e.project().composition();
    assert_eq!((comp.width(), comp.height()), (1920, 1080));
    assert_eq!(comp.duration(), 150);
}
#[test]
fn continuous_audio_clock_tracks_rate_interpretation_reverse_and_remap() {
    let mut e = editor();
    e.execute(Command::SetVideoSpeed { id: 1, speed: 2.0 })
        .unwrap();
    let layer = e.project().composition().layer(1).unwrap();
    assert_eq!(layer.audio_source_seconds(12.75, 30), Some(0.05));
    e.execute(Command::SetTimeRemap {
        id: 1,
        enabled: true,
    })
    .unwrap();
    assert!((time(&e, 1, 42) - 2.0).abs() < 1e-9);
    let mut audio = metadata();
    audio.start_time = 0.5;
    audio.file_offset = 0.5;
    e.execute(Command::AddContent {
        content: Content::Video {
            path: "video.mov".into(),
            audio: Some(audio),
            duration: 6.0,
            source_fps: 30.0,
            start_frame: 0,
            playback: Default::default(),
        },
        width: 100.0,
        height: 100.0,
        name: "Video".into(),
    })
    .unwrap();
    assert_eq!(e.selected_layer().unwrap().audio_source_time(0, 30), None);
    assert_eq!(time(&e, 2, 30), 0.5);
    let asset = e.selected_layer().unwrap().asset_id().unwrap();
    e.execute(Command::InterpretAsset {
        asset,
        interpretation: FootageInterpretation {
            fps: Some(15.into()),
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(time(&e, 2, 60), 0.5);
    e.execute(Command::SetLayerRange {
        id: 2,
        start: 30,
        end: 90,
    })
    .unwrap();
    let end = time(&e, 2, 89);
    e.execute(Command::ReverseVideo { id: 2 }).unwrap();
    assert_eq!(time(&e, 2, 30), end);
}

#[test]
fn audio_projects_preserve_embedded_images_and_sequence_manifests() {
    let mut e = editor();
    for content in [
        Content::Image { png: "AA==".into() },
        Content::ImageSequence {
            frames: std::sync::Arc::new(vec!["frame_0001.png".into()]),
            fps: 1.into(),
            missing: MissingFramePolicy::Transparent,
            start_frame: 0,
            playback: Default::default(),
        },
    ] {
        e.execute(Command::AddContent {
            content,
            width: 1.0,
            height: 1.0,
            name: "Image".into(),
        })
        .unwrap();
    }
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
    let original = e.project().clone();
    e.execute(Command::Precompose {
        layers: vec![1],
        name: "Audio source".into(),
    })
    .unwrap();
    let nested = e
        .project()
        .compositions()
        .into_iter()
        .find(|(_, c)| c.name() == "Audio source")
        .unwrap()
        .1;
    assert_eq!(
        nested.layers()[0].content(),
        original.composition().layer(1).unwrap().content()
    );
}

#[test]
fn audio_cannot_supply_or_receive_a_visual_track_matte() {
    let mut e = editor();
    e.execute(Command::AddRectangle).unwrap();
    let original = e.project().clone();
    for (id, source) in [(1, 2), (2, 1)] {
        assert!(!e.project().composition().can_track_matte(id, source));
        assert!(
            e.execute(Command::SetTrackMatte {
                id,
                matte: Some(TrackMatte {
                    source,
                    mode: MatteMode::Alpha
                }),
            })
            .is_err()
        );
        assert_eq!(e.project(), &original);
    }
}
