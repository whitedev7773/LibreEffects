//! Model, animation, persistence and no-op regressions for Luma Key.
use super::*;

const PARAMETERS: [EffectParam; 2] = [EffectParam::LumaThreshold, EffectParam::LumaSoftness];

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    edit(&mut editor, EffectEdit::Add(EffectKind::LumaKey));
    editor
}
fn edit(editor: &mut Editor, edit: EffectEdit) {
    editor.execute(Command::Effect { id: 1, edit }).unwrap();
}
fn value(parameter: EffectParam, frame: Frame, value: f64) -> Command {
    Command::Effect {
        id: 1,
        edit: EffectEdit::SetValue {
            effect: 1,
            parameter,
            frame,
            value,
        },
    }
}
fn mode(mode: LumaKeyMode) -> Command {
    Command::Effect {
        id: 1,
        edit: EffectEdit::SetLumaKeyMode { effect: 1, mode },
    }
}
fn path(parameter: EffectParam) -> PropertyPath {
    PropertyPath::Effect {
        effect: 1,
        parameter,
    }
}
fn effect(editor: &Editor) -> &EffectInstance {
    &editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .effect_stack()[0]
}
fn assert_unchanged(editor: &mut Editor, command: Command, rejected: bool) {
    let current = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    let json = editor.project().to_json().unwrap();
    let native = project_file::encode(editor.project(), Some(b"{\"version\":1}")).unwrap();
    let result = editor.execute(command);
    assert_eq!(result.is_err(), rejected, "{result:?}");
    assert_eq!(editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    assert_eq!(editor.project().to_json().unwrap(), json);
    assert_eq!(
        project_file::encode(editor.project(), Some(b"{\"version\":1}")).unwrap(),
        native
    );
}
fn animated_scene() -> Editor {
    let mut editor = scene();
    for (parameter, a, b) in [
        (EffectParam::LumaThreshold, 0.0, 240.0),
        (EffectParam::LumaSoftness, 10.0, 70.0),
    ] {
        editor.execute(value(parameter, 10, a)).unwrap();
        edit(
            &mut editor,
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter,
                frame: 10,
            },
        );
        editor.execute(value(parameter, 40, b)).unwrap();
    }
    editor.execute(mode(LumaKeyMode::KeepDarker)).unwrap();
    editor
}
fn audio() -> Content {
    Content::Audio {
        path: "luma-test.wav".into(),
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
    }
}

#[test]
fn luma_defaults_specs_mode_and_legacy_wire_shape_are_explicit() {
    assert_eq!(EffectKind::ALL.len(), 14);
    assert_eq!(
        EffectKind::ALL
            .iter()
            .filter(|k| **k == EffectKind::LumaKey)
            .count(),
        1
    );
    assert_eq!(EffectKind::LumaKey.label(), "Luma Key");
    assert_eq!(LumaKeyMode::default(), LumaKeyMode::KeepBrighter);
    assert_eq!(
        LumaKeyMode::ALL,
        [LumaKeyMode::KeepBrighter, LumaKeyMode::KeepDarker]
    );
    let specs = EffectKind::LumaKey.parameters();
    assert_eq!(
        specs
            .iter()
            .map(|s| (s.parameter, s.label, s.min, s.max, s.default))
            .collect::<Vec<_>>(),
        vec![
            (EffectParam::LumaThreshold, "Threshold", 0.0, 255.0, 128.0),
            (EffectParam::LumaSoftness, "Softness", 0.0, 255.0, 0.0),
        ]
    );
    let editor = scene();
    assert_eq!(editor.project().version, 51);
    let instance = effect(&editor);
    assert_eq!(instance.name(), "Luma Key");
    assert!(!instance.bypassed());
    assert_eq!(instance.color_space(), EffectColorSpace::Srgb);
    assert_eq!(instance.luma_key_mode(), Some(LumaKeyMode::KeepBrighter));
    assert_eq!(instance.value_at(EffectParam::LumaThreshold, 99), 128.0);
    assert_eq!(instance.value_at(EffectParam::LumaSoftness, 99), 0.0);
    for parameter in PARAMETERS {
        assert!(instance.parameter(parameter).unwrap().keys().is_empty());
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .track_paths()
                .contains(&path(parameter))
        );
    }
    assert_eq!(
        serde_json::to_string(instance).unwrap(),
        r#"{"id":1,"kind":"LumaKey","name":"Luma Key","bypassed":false,"color_space":"Srgb","luma_key_mode":"KeepBrighter","parameters":{"LumaThreshold":{"value":128.0,"keys":{}},"LumaSoftness":{"value":0.0,"keys":{}}}}"#
    );
    let mut old = Editor::default();
    old.execute(Command::AddRectangle).unwrap();
    edit(&mut old, EffectEdit::Add(EffectKind::Brightness));
    assert_eq!(
        serde_json::to_string(effect(&old)).unwrap(),
        r#"{"id":1,"kind":"Brightness","name":"Brightness","bypassed":false,"color_space":"Srgb","parameters":{"Amount":{"value":1.0,"keys":{}}}}"#
    );
    assert_eq!(old.project().version, 12);
    assert_eq!(effect(&old).luma_key_mode(), None);
}

#[test]
fn luma_values_reject_nonfinite_out_of_range_wrong_targets_and_frames_atomically() {
    let mut editor = scene();
    for parameter in PARAMETERS {
        for invalid in [
            -f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            f64::INFINITY,
            -0.001,
            255.001,
        ] {
            assert_unchanged(&mut editor, value(parameter, 0, invalid), true);
        }
        for valid in [0.0, 0.125, 127.5, 254.875, 255.0] {
            editor.execute(value(parameter, 0, valid)).unwrap();
            assert_eq!(effect(&editor).value_at(parameter, 0), valid);
        }
        assert_unchanged(&mut editor, value(parameter, 150, 255.0), true);
        assert_unchanged(
            &mut editor,
            Command::EditTrack {
                id: 1,
                property: path(parameter),
                edit: TrackEdit::Value {
                    frame: u32::MAX,
                    value: 255.0,
                },
            },
            true,
        );
    }
    assert_unchanged(&mut editor, value(EffectParam::Radius, 0, 10.0), true);
    for command in [
        Command::Effect {
            id: 999,
            edit: EffectEdit::SetLumaKeyMode {
                effect: 1,
                mode: LumaKeyMode::KeepBrighter,
            },
        },
        Command::Effect {
            id: 1,
            edit: EffectEdit::SetLumaKeyMode {
                effect: 999,
                mode: LumaKeyMode::KeepBrighter,
            },
        },
        Command::Effect {
            id: 1,
            edit: EffectEdit::SetValue {
                effect: 999,
                parameter: EffectParam::LumaThreshold,
                frame: 0,
                value: 128.0,
            },
        },
    ] {
        assert_unchanged(&mut editor, command, true);
    }
    edit(&mut editor, EffectEdit::Add(EffectKind::Brightness));
    assert_unchanged(
        &mut editor,
        Command::Effect {
            id: 1,
            edit: EffectEdit::SetLumaKeyMode {
                effect: 2,
                mode: LumaKeyMode::KeepDarker,
            },
        },
        true,
    );
    assert_unchanged(
        &mut editor,
        Command::Effect {
            id: 1,
            edit: EffectEdit::SetValue {
                effect: 2,
                parameter: EffectParam::LumaThreshold,
                frame: 0,
                value: 128.0,
            },
        },
        true,
    );
    editor.execute(Command::ToggleLocked(1)).unwrap();
    for command in [
        value(EffectParam::LumaThreshold, 0, 255.0),
        mode(LumaKeyMode::KeepBrighter),
        Command::Effect {
            id: 1,
            edit: EffectEdit::Reset(1),
        },
    ] {
        assert_unchanged(&mut editor, command, true);
    }
}

#[test]
fn luma_noops_preserve_interpolated_keys_history_redo_assets_and_bytes() {
    let mut editor = animated_scene();
    editor
        .execute(Command::AddContent {
            content: Content::Image { png: "YWJj".into() },
            width: 16.0,
            height: 16.0,
            name: "Shared bytes".into(),
        })
        .unwrap();
    editor.execute(Command::DuplicateLayer(2)).unwrap();
    editor
        .execute(Command::RenameLayer {
            id: 2,
            name: "Changed".into(),
        })
        .unwrap();
    editor.undo();
    assert!(editor.can_redo());
    let before_assets = editor.project().asset_library.clone();
    let before_pointers: Vec<_> = editor
        .project()
        .composition
        .layers
        .iter()
        .filter_map(|l| match &l.content {
            Content::Image { png } => Some(png.as_ptr()),
            _ => None,
        })
        .collect();
    let noops = || {
        vec![
            value(EffectParam::LumaThreshold, 25, 120.0),
            Command::EditTrack {
                id: 1,
                property: path(EffectParam::LumaSoftness),
                edit: TrackEdit::Value {
                    frame: 25,
                    value: 40.0,
                },
            },
            mode(LumaKeyMode::KeepDarker),
            Command::EditTrack {
                id: 1,
                property: path(EffectParam::LumaThreshold),
                edit: TrackEdit::Keyframe {
                    from: 40,
                    to: 40,
                    value: 240.0,
                },
            },
        ]
    };
    for command in noops() {
        assert_unchanged(&mut editor, command, false);
    }
    assert_unchanged(
        &mut editor,
        Command::Batch(vec![
            Command::Batch(noops()),
            Command::Batch(vec![Command::Batch(noops())]),
        ]),
        false,
    );
    assert_eq!(
        effect(&editor)
            .parameter(EffectParam::LumaThreshold)
            .unwrap()
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [10, 40]
    );
    assert_eq!(editor.project().asset_library, before_assets);
    assert_eq!(
        editor
            .project()
            .composition
            .layers
            .iter()
            .filter_map(|l| match &l.content {
                Content::Image { png } => Some(png.as_ptr()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        before_pointers
    );
    assert!(editor.can_redo());
    editor.redo();
    assert_eq!(
        editor.project().composition().layer(2).unwrap().name(),
        "Changed"
    );
}

#[test]
fn luma_noops_validate_locks_and_nested_batches_and_watch_is_intentional() {
    let mut editor = scene();
    editor.clear_history();
    assert_unchanged(
        &mut editor,
        value(EffectParam::LumaThreshold, 0, 128.0),
        false,
    );
    assert_unchanged(&mut editor, mode(LumaKeyMode::KeepBrighter), false);
    assert!(!editor.can_undo());
    let invalid = Command::Batch(vec![
        value(EffectParam::LumaThreshold, 0, 140.0),
        Command::Batch(vec![
            mode(LumaKeyMode::KeepDarker),
            value(EffectParam::LumaSoftness, 150, 0.0),
        ]),
    ]);
    assert_unchanged(&mut editor, invalid, true);
    edit(
        &mut editor,
        EffectEdit::ToggleAnimation {
            effect: 1,
            parameter: EffectParam::LumaThreshold,
            frame: 0,
        },
    );
    assert!(editor.can_undo());
    assert_eq!(
        effect(&editor)
            .parameter(EffectParam::LumaThreshold)
            .unwrap()
            .keys()
            .len(),
        1
    );
    edit(
        &mut editor,
        EffectEdit::ToggleKey {
            effect: 1,
            parameter: EffectParam::LumaThreshold,
            frame: 25,
        },
    );
    assert_eq!(
        effect(&editor)
            .parameter(EffectParam::LumaThreshold)
            .unwrap()
            .keys()
            .len(),
        2
    );
    editor.undo();
    assert!(editor.can_redo());
    assert_unchanged(
        &mut editor,
        value(EffectParam::LumaThreshold, 25, 128.0),
        false,
    );
    editor.redo();
    assert_eq!(
        effect(&editor)
            .parameter(EffectParam::LumaThreshold)
            .unwrap()
            .keys()
            .len(),
        2
    );
    editor.execute(Command::ToggleLocked(1)).unwrap();
    assert_unchanged(
        &mut editor,
        Command::Batch(vec![
            mode(LumaKeyMode::KeepBrighter),
            value(EffectParam::LumaThreshold, 0, 128.0),
        ]),
        true,
    );
}

#[test]
fn luma_reset_duplicate_order_bypass_and_undo_redo_preserve_typed_state() {
    let mut editor = animated_scene();
    edit(
        &mut editor,
        EffectEdit::Rename {
            effect: 1,
            name: "Highlights".into(),
        },
    );
    edit(
        &mut editor,
        EffectEdit::Bypass {
            effect: 1,
            bypassed: true,
        },
    );
    let original = effect(&editor).clone();
    edit(&mut editor, EffectEdit::Duplicate(1));
    let stack = editor.selected_layer().unwrap().effect_stack();
    assert_eq!(
        stack.iter().map(EffectInstance::id).collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(stack[1].name(), "Highlights");
    assert!(stack[1].bypassed());
    assert_eq!(stack[1].luma_key_mode(), Some(LumaKeyMode::KeepDarker));
    for parameter in PARAMETERS {
        assert_eq!(stack[1].parameter(parameter), original.parameter(parameter));
    }
    edit(
        &mut editor,
        EffectEdit::Move {
            effect: 2,
            index: 0,
        },
    );
    assert_eq!(effect(&editor).id(), 2);
    let before = editor.project().clone();
    edit(&mut editor, EffectEdit::Reset(2));
    assert_eq!(effect(&editor).name(), "Highlights");
    assert!(!effect(&editor).bypassed());
    assert_eq!(
        effect(&editor).luma_key_mode(),
        Some(LumaKeyMode::KeepBrighter)
    );
    assert_eq!(
        effect(&editor).value_at(EffectParam::LumaThreshold, 25),
        128.0
    );
    assert_eq!(effect(&editor).value_at(EffectParam::LumaSoftness, 25), 0.0);
    for parameter in PARAMETERS {
        assert!(
            effect(&editor)
                .parameter(parameter)
                .unwrap()
                .keys()
                .is_empty()
        );
    }
    let after = editor.project().clone();
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
    edit(&mut editor, EffectEdit::Remove(2));
    assert_eq!(effect(&editor), &original);
    assert_eq!(
        Project::from_json(&editor.project().to_json().unwrap()).unwrap(),
        *editor.project()
    );
}

#[test]
fn luma_nonpixel_targets_reject_add_preset_load_and_content_conversion() {
    let source = scene();
    let preset =
        EffectPreset::capture(source.selected_layer().unwrap(), None, 30.into(), "Luma").unwrap();
    for content in [Content::Null, audio()] {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: content.clone(),
                width: 16.0,
                height: 16.0,
                name: "Nonpixel".into(),
            })
            .unwrap();
        assert_unchanged(
            &mut editor,
            Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::LumaKey),
            },
            true,
        );
        assert_unchanged(
            &mut editor,
            Command::Effect {
                id: 1,
                edit: EffectEdit::ApplyPreset {
                    preset: preset.clone(),
                    frame: 0,
                },
            },
            true,
        );
        let mut malformed = editor.project().clone();
        malformed.version = 51;
        malformed.composition.layers[0].effect_stack =
            source.selected_layer().unwrap().effect_stack().to_vec();
        malformed.composition.layers[0].next_effect_id = 2;
        assert!(Project::from_json(&serde_json::to_string(&malformed).unwrap()).is_err());
        assert!(editor.replace_project(malformed.clone()).is_err());
        assert!(
            EffectPreset::capture(
                &malformed.composition.layers[0],
                None,
                30.into(),
                "Bad target"
            )
            .is_err()
        );
        // The command validator also rejects an invalid target before a sampled no-op.
        let mut snapshot = Snapshot {
            project: malformed,
            selected: Some(1),
        };
        for command in [
            mode(LumaKeyMode::KeepBrighter),
            value(EffectParam::LumaThreshold, 0, 128.0),
        ] {
            let before = snapshot.clone();
            assert!(apply(&mut snapshot, command).is_err());
            assert_eq!(snapshot, before);
        }
        let mut editor = scene();
        assert_unchanged(&mut editor, Command::SetContent { id: 1, content }, true);
    }
}

#[test]
fn luma_presence_requires_schema51_inactive_bypassed_nested_and_unanimated() {
    let mut editor = scene();
    edit(
        &mut editor,
        EffectEdit::Bypass {
            effect: 1,
            bypassed: true,
        },
    );
    editor.execute(Command::ToggleVisible(1)).unwrap();
    editor
        .execute(Command::Precompose {
            layers: vec![1],
            name: "Nested luma".into(),
        })
        .unwrap();
    editor.execute(Command::NewComposition).unwrap();
    assert!(editor.project().composition().layers().is_empty());
    assert_eq!(editor.project().version, 51);
    let original = editor.project().clone();
    for version in [1, 12, 19, 35, 36, 49, 50] {
        let mut bad = original.clone();
        bad.version = version;
        assert!(Project::from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        assert!(project_file::encode(&bad, None).is_err());
        assert!(editor.replace_project(bad).is_err());
        assert_eq!(editor.project(), &original);
    }
    assert_eq!(
        Project::from_json(&original.to_json().unwrap()).unwrap(),
        original
    );
    editor.execute(Command::AddRectangle).unwrap();
    assert_eq!(editor.project().version, 51);
    let json = editor.project().to_json().unwrap();
    assert!(json.contains("KeepBrighter"));
}

#[test]
fn luma_loaded_mode_color_parameter_and_animation_validation_is_strict() {
    let editor = animated_scene();
    let source = serde_json::to_value(editor.project()).unwrap();
    let mut cases = Vec::new();
    for mode in [
        serde_json::Value::Null,
        serde_json::json!("Unknown"),
        serde_json::json!(0),
    ] {
        let mut bad = source.clone();
        bad["composition"]["layers"][0]["effect_stack"][0]["luma_key_mode"] = mode;
        cases.push(bad);
    }
    let mut absent = source.clone();
    absent["composition"]["layers"][0]["effect_stack"][0]
        .as_object_mut()
        .unwrap()
        .remove("luma_key_mode");
    cases.push(absent);
    let mut linear = source.clone();
    linear["composition"]["layers"][0]["effect_stack"][0]["color_space"] = "LinearRgb".into();
    cases.push(linear);
    for field in ["base", "key", "frame", "missing", "extra", "temporal"] {
        let mut bad = source.clone();
        let parameters = &mut bad["composition"]["layers"][0]["effect_stack"][0]["parameters"];
        match field {
            "base" => parameters["LumaThreshold"]["value"] = (-0.01).into(),
            "key" => parameters["LumaThreshold"]["keys"]["40"]["value"] = 256.into(),
            "frame" => {
                parameters["LumaThreshold"]["keys"]["150"] =
                    parameters["LumaThreshold"]["keys"]["40"].clone();
            }
            "missing" => {
                parameters.as_object_mut().unwrap().remove("LumaSoftness");
            }
            "extra" => parameters["Radius"] = serde_json::json!({"value": 0.0, "keys": {}}),
            _ => {
                parameters["LumaThreshold"]["keys"]["10"]["temporal"] =
                    serde_json::json!({"outgoing": {"slope": 1.0, "influence": 2.0}})
            }
        }
        cases.push(bad);
    }
    let mut old = Editor::default();
    old.execute(Command::AddRectangle).unwrap();
    edit(&mut old, EffectEdit::Add(EffectKind::Brightness));
    let mut foreign = serde_json::to_value(old.project()).unwrap();
    foreign["composition"]["layers"][0]["effect_stack"][0]["luma_key_mode"] = "KeepBrighter".into();
    cases.push(foreign);
    for bad in cases {
        assert!(
            Project::from_json(&bad.to_string()).is_err(),
            "accepted malformed project: {bad}"
        );
        assert!(document::decode_native(bad.clone(), BTreeMap::new()).is_err());
        if let Ok(project) = serde_json::from_value::<Project>(bad) {
            assert!(project.validate().is_err());
        }
    }
}

#[test]
fn luma_clamps_temporal_overshoot_and_noop_preserves_the_unclamped_curve() {
    let mut editor = scene();
    for parameter in PARAMETERS {
        editor.execute(value(parameter, 0, 128.0)).unwrap();
        edit(
            &mut editor,
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter,
                frame: 0,
            },
        );
        edit(
            &mut editor,
            EffectEdit::ToggleKey {
                effect: 1,
                parameter,
                frame: 20,
            },
        );
        for (frame, incoming) in [(0, false), (20, true)] {
            editor
                .execute(Command::SetTemporalHandle {
                    id: 1,
                    property: path(parameter),
                    frame,
                    incoming,
                    handle: TemporalHandle {
                        slope: if incoming { -100.0 } else { 100.0 },
                        influence: 0.5,
                    },
                })
                .unwrap();
        }
        assert!(effect(&editor).parameter(parameter).unwrap().value_at(10) > 255.0);
        assert_eq!(effect(&editor).value_at(parameter, 10), 255.0);
        assert_unchanged(&mut editor, value(parameter, 10, 255.0), false);
        for (frame, incoming) in [(0, false), (20, true)] {
            editor
                .execute(Command::SetTemporalHandle {
                    id: 1,
                    property: path(parameter),
                    frame,
                    incoming,
                    handle: TemporalHandle {
                        slope: if incoming { 100.0 } else { -100.0 },
                        influence: 0.5,
                    },
                })
                .unwrap();
        }
        assert!(effect(&editor).parameter(parameter).unwrap().value_at(10) < 0.0);
        assert_eq!(effect(&editor).value_at(parameter, 10), 0.0);
        assert_unchanged(&mut editor, value(parameter, 10, 0.0), false);
    }
}

#[test]
fn luma_presets_version4_roundtrip_fresh_ids_mode_temporal_and_fps_conversion() {
    let mut editor = animated_scene();
    editor
        .execute(Command::SetTemporalHandle {
            id: 1,
            property: path(EffectParam::LumaThreshold),
            frame: 10,
            incoming: false,
            handle: TemporalHandle {
                slope: 6.0,
                influence: 0.4,
            },
        })
        .unwrap();
    let preset = EffectPreset::capture(
        editor.selected_layer().unwrap(),
        Some(1),
        30.into(),
        "Luma look",
    )
    .unwrap();
    let text = preset.to_json().unwrap();
    let wire: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(wire["version"], 4);
    assert_eq!(wire["effects"][0]["luma_key_mode"], "KeepDarker");
    assert_eq!(preset.key_count(), 4);
    assert_eq!(
        preset.effects()[0]
            .parameter(EffectParam::LumaThreshold)
            .unwrap()
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [0, 30]
    );
    for version in [1, 2, 3, 5] {
        let mut bad = wire.clone();
        bad["version"] = version.into();
        assert!(EffectPreset::from_json(&bad.to_string()).is_err());
    }
    for field in ["luma_key_mode", "color_space"] {
        let mut bad = wire.clone();
        bad["effects"][0][field] = if field == "color_space" {
            "LinearRgb".into()
        } else {
            serde_json::Value::Null
        };
        assert!(EffectPreset::from_json(&bad.to_string()).is_err());
    }
    let preset = EffectPreset::from_json(&text).unwrap();
    assert_eq!(preset.to_json().unwrap(), text);
    editor
        .execute(Command::ConfigureComposition {
            name: "60fps".into(),
            width: 1920,
            height: 1080,
            fps: 60,
            duration: 300,
        })
        .unwrap();
    let before = editor.project().clone();
    edit(&mut editor, EffectEdit::ApplyPreset { preset, frame: 25 });
    let appended = &editor.selected_layer().unwrap().effect_stack()[1];
    assert_eq!(appended.id(), 2);
    assert_eq!(appended.luma_key_mode(), Some(LumaKeyMode::KeepDarker));
    for parameter in PARAMETERS {
        let track = appended.parameter(parameter).unwrap();
        assert_eq!(track.keys().keys().copied().collect::<Vec<_>>(), [25, 85]);
    }
    assert_eq!(
        appended
            .parameter(EffectParam::LumaThreshold)
            .unwrap()
            .keys()[&25]
            .temporal
            .outgoing,
        Some(TemporalHandle {
            slope: 3.0,
            influence: 0.4
        })
    );
    assert_eq!(appended.value_at(EffectParam::LumaSoftness, 55), 40.0);
    let after = editor.project().clone();
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
}

#[test]
fn luma_preset_collisions_and_duration_reject_atomically_and_legacy_versions_stay_old() {
    let mut editor = animated_scene();
    let preset =
        EffectPreset::capture(editor.selected_layer().unwrap(), None, 240.into(), "Rapid").unwrap();
    assert_unchanged(
        &mut editor,
        Command::Effect {
            id: 1,
            edit: EffectEdit::ApplyPreset {
                preset: preset.clone(),
                frame: 150,
            },
        },
        true,
    );
    let mut wire: serde_json::Value = serde_json::from_str(&preset.to_json().unwrap()).unwrap();
    let key = wire["effects"][0]["parameters"]["LumaThreshold"]["keys"]["0"].clone();
    wire["effects"][0]["parameters"]["LumaThreshold"]["keys"]["1"] = key;
    let preset = EffectPreset::from_json(&wire.to_string()).unwrap();
    editor
        .execute(Command::ConfigureComposition {
            name: "1fps".into(),
            width: 1920,
            height: 1080,
            fps: 1,
            duration: 150,
        })
        .unwrap();
    assert_unchanged(
        &mut editor,
        Command::Effect {
            id: 1,
            edit: EffectEdit::ApplyPreset { preset, frame: 0 },
        },
        true,
    );
    for kind in EffectKind::ALL
        .into_iter()
        // Slider Control has its own schema-65 coverage; this loop checks legacy effects.
        .filter(|k| !matches!(k, EffectKind::LumaKey | EffectKind::SliderControl))
    {
        let mut old = Editor::default();
        old.execute(Command::AddRectangle).unwrap();
        edit(&mut old, EffectEdit::Add(kind));
        let preset =
            EffectPreset::capture(old.selected_layer().unwrap(), None, 30.into(), "Legacy")
                .unwrap();
        let text = preset.to_json().unwrap();
        assert!(!text.contains("luma_key_mode"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap()["version"],
            1
        );
        assert_eq!(
            EffectPreset::from_json(&text).unwrap().to_json().unwrap(),
            text
        );
        assert_eq!(
            old.project().version,
            if matches!(
                kind,
                EffectKind::Curves | EffectKind::LinearGradient | EffectKind::RadialGradient
            ) {
                19
            } else {
                12
            }
        );
    }
}

#[test]
fn luma_duplicate_split_shift_clipboard_fps_and_key_copy_preserve_animation() {
    let mut editor = animated_scene();
    editor
        .execute(Command::SetTemporalHandle {
            id: 1,
            property: path(EffectParam::LumaThreshold),
            frame: 10,
            incoming: false,
            handle: TemporalHandle {
                slope: 6.0,
                influence: 0.4,
            },
        })
        .unwrap();
    let original = effect(&editor).clone();
    let keys: Vec<_> = PARAMETERS
        .into_iter()
        .map(|p| {
            editor
                .selected_layer()
                .unwrap()
                .copy_key(path(p), 10)
                .unwrap()
        })
        .collect();
    editor.execute(Command::DuplicateLayer(1)).unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().effect_stack(),
        &[original.clone()]
    );
    editor
        .execute(Command::PasteKeys {
            keys,
            frame: 60,
            target: Some(2),
        })
        .unwrap();
    for parameter in PARAMETERS {
        assert_eq!(
            editor.selected_layer().unwrap().effect_stack()[0]
                .parameter(parameter)
                .unwrap()
                .keys()[&60],
            original.parameter(parameter).unwrap().keys()[&10]
        );
    }
    editor
        .execute(Command::SplitLayers {
            ids: vec![1],
            frame: 25,
        })
        .unwrap();
    let split = editor.selected().unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().effect_stack(),
        &[original.clone()]
    );
    editor
        .execute(Command::SetLayerRange {
            id: split,
            start: 25,
            end: 100,
        })
        .unwrap();
    editor
        .execute(Command::ShiftLayer {
            id: split,
            delta: 5,
        })
        .unwrap();
    let clipboard = editor.copy_layers(&[split]).unwrap();
    editor.execute(Command::NewComposition).unwrap();
    editor
        .execute(Command::ConfigureComposition {
            name: "60fps".into(),
            width: 1920,
            height: 1080,
            fps: 60,
            duration: 300,
        })
        .unwrap();
    editor.execute(Command::PasteLayers(clipboard)).unwrap();
    let pasted = &editor.selected_layer().unwrap().effect_stack()[0];
    assert_eq!(pasted.luma_key_mode(), Some(LumaKeyMode::KeepDarker));
    for parameter in PARAMETERS {
        assert_eq!(
            pasted
                .parameter(parameter)
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            [30, 90]
        );
    }
    assert_eq!(
        pasted.parameter(EffectParam::LumaThreshold).unwrap().keys()[&30]
            .temporal
            .outgoing,
        Some(TemporalHandle {
            slope: 3.0,
            influence: 0.4
        })
    );
    // Five source frames of origin movement resample to ten at 60 fps.
    assert_eq!(editor.selected_layer().unwrap().start_frame, Some(10));
    assert_eq!(editor.project().version, 64);
    assert_eq!(
        Project::from_json(&editor.project().to_json().unwrap()).unwrap(),
        *editor.project()
    );
}

#[test]
fn luma_stack_and_key_limits_preserve_atomicity_and_exact_noops() {
    let mut editor = scene();
    let additions: Vec<_> = (1..64)
        .map(|_| Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::LumaKey),
        })
        .collect();
    editor.execute(Command::Batch(additions)).unwrap();
    assert_eq!(editor.selected_layer().unwrap().effect_stack().len(), 64);
    for edit in [
        EffectEdit::Add(EffectKind::LumaKey),
        EffectEdit::Duplicate(1),
    ] {
        assert_unchanged(&mut editor, Command::Effect { id: 1, edit }, true);
    }
    let mut editor = scene();
    let mut project = editor.project().clone();
    project.composition.duration = 20_000;
    let track = project.composition.layers[0]
        .track_mut(path(EffectParam::LumaThreshold))
        .unwrap();
    track.keys = (0..10_000)
        .map(|frame| {
            (
                frame,
                Keyframe {
                    value: 128.0,
                    interpolation: Interpolation::Linear,
                    temporal: Default::default(),
                },
            )
        })
        .collect();
    editor.replace_project(project).unwrap();
    assert_unchanged(
        &mut editor,
        value(EffectParam::LumaThreshold, 15_000, 128.0),
        false,
    );
    assert_unchanged(
        &mut editor,
        value(EffectParam::LumaThreshold, 15_000, 129.0),
        true,
    );
    assert_unchanged(
        &mut editor,
        Command::Effect {
            id: 1,
            edit: EffectEdit::ToggleKey {
                effect: 1,
                parameter: EffectParam::LumaThreshold,
                frame: 15_000,
            },
        },
        true,
    );
}
