use super::*;

const TYPOGRAPHY: [TextParam; 3] = [TextParam::FontSize, TextParam::Tracking, TextParam::Leading];

fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: "Title 한글\nSecond line".into(),
            font_size: 48.,
        },
        width: 500.,
        height: 200.,
        name: "Typography".into(),
    })
    .unwrap();
    e
}
fn edit(e: &mut Editor, parameter: TextParam, edit: TrackEdit) {
    e.execute(Command::EditText {
        id: 1,
        parameter,
        edit,
    })
    .unwrap();
}
fn set(e: &mut Editor, parameter: TextParam, value: f64, frame: Frame) {
    if let Some(command) = e
        .selected_layer()
        .unwrap()
        .text_value_command(parameter, value, frame)
        .unwrap()
    {
        e.execute(command).unwrap();
    }
}
fn sample(e: &Editor, parameter: TextParam, frame: Frame) -> f64 {
    e.project()
        .composition()
        .layer(1)
        .unwrap()
        .text_value_at(parameter, frame)
        .unwrap()
}
fn assert_rejected(e: &mut Editor, command: Command) {
    let current = e.current.clone();
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    assert!(e.execute(command).is_err());
    assert_eq!(e.current, current);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
}
fn animated_scene() -> Editor {
    let mut e = scene();
    for (parameter, value) in [
        (TextParam::FontSize, 96.),
        (TextParam::Tracking, 120.),
        (TextParam::Leading, 2.4),
    ] {
        edit(&mut e, parameter, TrackEdit::ToggleAnimation { frame: 10 });
        set(&mut e, parameter, value, 40);
        e.execute(Command::SetTemporalHandle {
            id: 1,
            property: PropertyPath::Text(parameter),
            frame: 10,
            incoming: false,
            handle: TemporalHandle {
                slope: 0.25,
                influence: 0.3,
            },
        })
        .unwrap();
        e.execute(Command::SetTemporalMode {
            id: 1,
            property: PropertyPath::Text(parameter),
            frame: 40,
            mode: TemporalMode::Auto,
        })
        .unwrap();
    }
    e
}

#[test]
fn text_typography_sparse_legacy_sampling_and_roundtrip_do_not_materialize_or_upgrade() {
    let mut e = scene();
    let original = e.project().clone();
    assert_eq!(original.version, 3);
    assert_eq!(
        &TextParam::ALL[7..10],
        &TYPOGRAPHY,
        "The original typography positions remain stable"
    );
    for frame in [0, 25, u32::MAX] {
        let layer = e.selected_layer().unwrap();
        assert_eq!(
            layer.text_typography_at(frame),
            Some(TextTypography {
                font_size: 48.,
                tracking: 0.,
                leading: 1.2
            })
        );
        for parameter in TYPOGRAPHY {
            assert!(layer.track(PropertyPath::Text(parameter)).is_none());
            assert!(layer.track_label(PropertyPath::Text(parameter)).is_some());
            assert!(!layer.track_paths().contains(&PropertyPath::Text(parameter)));
        }
    }
    assert_eq!(e.project(), &original);
    let json = original.to_json().unwrap();
    assert!(!json.contains("text_parameters"));
    assert_eq!(Project::from_json(&json).unwrap(), original);
    let bytes = project_file::encode(&original, None).unwrap();
    assert_eq!(&bytes[8..10], &[1, 0]);
    assert_eq!(project_file::decode(&bytes).unwrap().project, original);
    let mut explicit_empty = serde_json::to_value(&original).unwrap();
    explicit_empty["composition"]["layers"][0]["text_parameters"] = serde_json::json!({});
    assert_eq!(
        Project::from_json(&explicit_empty.to_string()).unwrap(),
        original
    );
    e.execute(Command::AddRectangle).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_typography_at(0), None);
}

#[test]
fn text_typography_static_planner_updates_only_base_and_respects_specific_track_presence() {
    let mut e = scene();
    let base = e.selected_layer().unwrap().clone();
    for (parameter, value) in [
        (TextParam::FontSize, 64.),
        (TextParam::Tracking, -25.),
        (TextParam::Leading, 1.8),
    ] {
        let before = e.current.clone();
        let undo = e.undo.len();
        let command = e
            .selected_layer()
            .unwrap()
            .text_value_command(parameter, value, 20)
            .unwrap()
            .unwrap();
        assert!(matches!(
            (&command, parameter),
            (Command::SetContent { .. }, TextParam::FontSize)
                | (
                    Command::SetTextStyle { .. },
                    TextParam::Tracking | TextParam::Leading
                )
        ));
        assert_eq!(e.current, before);
        e.execute(command).unwrap();
        assert_eq!(sample(&e, parameter, 100), value);
        assert!(e.selected_layer().unwrap().text_parameters.is_empty());
        assert!(e.project().version < 48);
        assert_eq!(e.undo.len(), undo + 1);
        let after = e.current.clone();
        e.undo();
        assert_eq!(e.current, before);
        e.redo();
        assert_eq!(e.current, after);
    }
    let layer = e.selected_layer().unwrap();
    assert_eq!(layer.color, base.color);
    let mut expected_style = base.text_style.clone();
    expected_style.tracking = -25.;
    expected_style.leading = 1.8;
    assert_eq!(layer.text_style, expected_style);
    assert_eq!(
        layer.content,
        Content::Text {
            text: "Title 한글\nSecond line".into(),
            font_size: 64.
        }
    );

    // A sibling track does not materialize the edited parameter or leak sampled
    // typography into a new style baseline.
    edit(
        &mut e,
        TextParam::Tracking,
        TrackEdit::ToggleAnimation { frame: 0 },
    );
    set(&mut e, TextParam::Tracking, 200., 40);
    set(&mut e, TextParam::Leading, 2., 40);
    set(&mut e, TextParam::FontSize, 72., 40);
    let layer = e.selected_layer().unwrap();
    assert_eq!(layer.text_parameters.len(), 1);
    assert_eq!(layer.text_style.tracking, -25.);
    assert_eq!(layer.text_style.leading, 2.);
    assert_eq!(layer.text_value_at(TextParam::Tracking, 40), Some(200.));
    assert_eq!(e.project().version, 49);

    // Existing scalar paint semantics remain sparse-track overrides.
    let command = layer
        .text_value_command(TextParam::StrokeWidth, 5., 20)
        .unwrap()
        .unwrap();
    assert!(matches!(
        command,
        Command::EditText {
            parameter: TextParam::StrokeWidth,
            ..
        }
    ));
    e.execute(command).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_style.stroke_width, 1.);
    assert_eq!(sample(&e, TextParam::StrokeWidth, 20), 5.);
}

#[test]
fn text_typography_all_bounds_and_materialized_empty_tracks_preserve_base() {
    for parameter in TYPOGRAPHY {
        let mut e = scene();
        let base = e.selected_layer().unwrap().clone();
        let (min, max) = parameter.bounds();
        for value in [min, max] {
            set(&mut e, parameter, value, 0);
            assert_eq!(sample(&e, parameter, 90), value);
            assert!(e.selected_layer().unwrap().text_parameters.is_empty());
        }
        e.undo();
        e.undo();
        assert_eq!(e.selected_layer().unwrap(), &base);
        edit(
            &mut e,
            parameter,
            TrackEdit::Value {
                frame: 20,
                value: max,
            },
        );
        let layer = e.selected_layer().unwrap();
        assert_eq!(layer.content, base.content);
        assert_eq!(layer.text_style, base.text_style);
        assert!(layer.text_parameters[&parameter].keys.is_empty());
        assert_eq!(e.project().version, 49);
        let command = layer
            .text_value_command(parameter, min, 25)
            .unwrap()
            .unwrap();
        assert!(matches!(command, Command::EditText { .. }));
        e.execute(command).unwrap();
        assert_eq!(sample(&e, parameter, 100), min);
        assert_eq!(e.selected_layer().unwrap().content, base.content);
        assert_eq!(e.selected_layer().unwrap().text_style, base.text_style);
        assert!(
            e.selected_layer().unwrap().text_parameters[&parameter]
                .keys
                .is_empty()
        );
    }
}

#[test]
fn text_typography_planner_unchanged_samples_and_direct_base_noops_preserve_redo() {
    for animated in [false, true] {
        let mut e = if animated { animated_scene() } else { scene() };
        if !animated {
            // A historical schema higher than these static features require must
            // survive the no-op without running unrelated feature migrations.
            let mut imported = e.project().clone();
            imported.version = 47;
            e.replace_project(imported).unwrap();
        }
        e.execute(Command::RenameLayer {
            id: 1,
            name: "Redo me".into(),
        })
        .unwrap();
        e.undo();
        let current = e.current.clone();
        let undo = e.undo.clone();
        let redo = e.redo.clone();
        for parameter in TYPOGRAPHY {
            let value = sample(&e, parameter, 25);
            assert!(
                e.selected_layer()
                    .unwrap()
                    .text_value_command(parameter, value, 25)
                    .unwrap()
                    .is_none()
            );
            if !animated {
                for command in [
                    Command::EditText {
                        id: 1,
                        parameter,
                        edit: TrackEdit::Value { frame: 25, value },
                    },
                    Command::EditTrack {
                        id: 1,
                        property: PropertyPath::Text(parameter),
                        edit: TrackEdit::Value { frame: 25, value },
                    },
                ] {
                    e.execute(Command::Batch(vec![command])).unwrap();
                }
            }
            assert_eq!(e.current, current);
            assert_eq!(e.undo, undo);
            assert_eq!(e.redo, redo);
        }
        e.redo();
        assert_eq!(e.selected_layer().unwrap().name(), "Redo me");
    }
}

#[test]
fn text_typography_key_updates_preserve_metadata_new_keys_are_linear_and_style_is_temporary() {
    let mut e = animated_scene();
    let base_style = e.selected_layer().unwrap().text_style.clone();
    let base_content = e.selected_layer().unwrap().content.clone();
    for parameter in TYPOGRAPHY {
        let before = e.selected_layer().unwrap().text_parameters[&parameter].keys[&10].clone();
        let value = (parameter.bounds().0 + parameter.bounds().1) / 2.;
        set(&mut e, parameter, value, 10);
        let key = &e.selected_layer().unwrap().text_parameters[&parameter].keys[&10];
        assert_eq!(key.interpolation, before.interpolation);
        assert_eq!(key.temporal, before.temporal);
        set(&mut e, parameter, value + 0.25, 25);
        let key = &e.selected_layer().unwrap().text_parameters[&parameter].keys[&25];
        assert_eq!(key.interpolation, Interpolation::Linear);
        assert_eq!(key.temporal, TemporalHandles::default());
    }
    let layer = e.selected_layer().unwrap();
    assert_eq!(layer.text_style, base_style);
    assert_eq!(layer.content, base_content);
    let typography = layer.text_typography_at(25).unwrap();
    assert_eq!(typography.font_size, sample(&e, TextParam::FontSize, 25));
    let mut styled = TextStyle {
        paragraph: true,
        font_family: "Custom".into(),
        weight: 700,
        stroke_enabled: true,
        stroke_color: 0x123456,
        stroke_width: 7.,
        align: TextAlign::Right,
        ..Default::default()
    };
    let mut expected = styled.clone();
    expected.tracking = typography.tracking;
    expected.leading = typography.leading;
    typography.apply_to_style(&mut styled);
    assert_eq!(styled, expected);
    assert_eq!(e.selected_layer().unwrap().text_style, base_style);
}

#[test]
fn text_typography_linear_hold_overshoot_sampling_disable_and_insert_are_bounded() {
    for parameter in TYPOGRAPHY {
        let mut e = scene();
        let base = e.selected_layer().unwrap().clone();
        let (min, max) = parameter.bounds();
        let low = min + (max - min) / 3.;
        let high = min + 2. * (max - min) / 3.;
        edit(
            &mut e,
            parameter,
            TrackEdit::Value {
                frame: 0,
                value: low,
            },
        );
        edit(&mut e, parameter, TrackEdit::ToggleAnimation { frame: 0 });
        set(&mut e, parameter, high, 40);
        assert!((sample(&e, parameter, 20) - (min + max) / 2.).abs() < 1e-9);
        edit(
            &mut e,
            parameter,
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Hold,
            },
        );
        assert_eq!(sample(&e, parameter, 20), low);
        for direction in [-2., 3.] {
            edit(
                &mut e,
                parameter,
                TrackEdit::Interpolate {
                    frame: 0,
                    interpolation: Interpolation::Bezier(Bezier {
                        x1: 1. / 3.,
                        y1: direction,
                        x2: 2. / 3.,
                        y2: direction,
                    }),
                },
            );
            let expected = if direction < 0. { min } else { max };
            assert_eq!(sample(&e, parameter, 20), expected);
            assert!(
                e.selected_layer()
                    .unwrap()
                    .text_value_command(parameter, expected, 20)
                    .unwrap()
                    .is_none()
            );
            let before = e.current.clone();
            for operation in [
                TrackEdit::ToggleKey { frame: 20 },
                TrackEdit::ToggleAnimation { frame: 20 },
            ] {
                edit(&mut e, parameter, operation);
                assert_eq!(
                    e.selected_layer().unwrap().text_parameters[&parameter].value_at(20),
                    expected
                );
                assert_eq!(sample(&e, parameter, 20), expected);
                assert_eq!(e.selected_layer().unwrap().content, base.content);
                assert_eq!(e.selected_layer().unwrap().text_style, base.text_style);
                e.undo();
                assert_eq!(e.current, before);
            }
        }
    }
}

#[test]
fn text_typography_invalid_values_mixed_batches_frames_types_and_locks_are_atomic() {
    let mut e = scene();
    e.execute(Command::RenameLayer {
        id: 1,
        name: "Redo me".into(),
    })
    .unwrap();
    e.undo();
    for parameter in TYPOGRAPHY {
        let (min, max) = parameter.bounds();
        for value in [
            min - 1.,
            max + 1.,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert!(
                e.selected_layer()
                    .unwrap()
                    .text_value_command(parameter, value, 0)
                    .is_err()
            );
            assert_rejected(
                &mut e,
                Command::Batch(vec![
                    Command::EditText {
                        id: 1,
                        parameter: TextParam::FillRed,
                        edit: TrackEdit::Value {
                            frame: 0,
                            value: 20.,
                        },
                    },
                    Command::EditText {
                        id: 1,
                        parameter,
                        edit: TrackEdit::Value { frame: 0, value },
                    },
                ]),
            );
        }
        let value = sample(&e, parameter, 0);
        for edit in [
            TrackEdit::Value { frame: 150, value },
            TrackEdit::ToggleKey { frame: u32::MAX },
            TrackEdit::ToggleAnimation { frame: 150 },
            TrackEdit::Keyframe {
                from: 0,
                to: 150,
                value,
            },
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Hold,
            },
        ] {
            assert_rejected(
                &mut e,
                Command::EditText {
                    id: 1,
                    parameter,
                    edit,
                },
            );
        }
    }
    e.execute(Command::ToggleLocked(1)).unwrap();
    for parameter in TYPOGRAPHY {
        let value = sample(&e, parameter, 0);
        assert!(
            e.selected_layer()
                .unwrap()
                .text_value_command(parameter, value, 0)
                .is_err()
        );
        assert_rejected(
            &mut e,
            Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleKey { frame: 0 },
            },
        );
    }
    e.execute(Command::AddRectangle).unwrap();
    for parameter in TYPOGRAPHY {
        assert_eq!(
            e.selected_layer().unwrap().text_value_at(parameter, 0),
            None
        );
        assert!(
            e.selected_layer()
                .unwrap()
                .text_value_command(parameter, 2., 0)
                .is_err()
        );
        assert_rejected(
            &mut e,
            Command::EditTrack {
                id: 2,
                property: PropertyPath::Text(parameter),
                edit: TrackEdit::ToggleKey { frame: 0 },
            },
        );
    }
}

#[test]
fn text_typography_schema49_inactive_and_empty_tracks_native_and_legacy_validation() {
    for parameter in TYPOGRAPHY {
        for keyed in [false, true] {
            let mut e = scene();
            let base = sample(&e, parameter, 0);
            edit(
                &mut e,
                parameter,
                if keyed {
                    TrackEdit::ToggleKey { frame: 0 }
                } else {
                    TrackEdit::Value {
                        frame: 0,
                        value: base + 1.,
                    }
                },
            );
            assert_eq!(e.project().version, 49);
            e.execute(Command::NewComposition).unwrap();
            let original = e.project().clone();
            assert_eq!(original.version, 49);
            assert_eq!(
                Project::from_json(&original.to_json().unwrap()).unwrap(),
                original
            );
            let bytes = project_file::encode(&original, None).unwrap();
            assert_eq!(&bytes[8..10], &[1, 0]);
            assert_eq!(project_file::decode(&bytes).unwrap().project, original);
            // Typography still requires 49; reject versions newer than the
            // executable rather than hard-coding the former future version.
            for version in [3, 48, PROJECT_VERSION + 1] {
                let mut bad = serde_json::to_value(&original).unwrap();
                bad["version"] = version.into();
                assert!(Project::from_json(&bad.to_string()).is_err());
                assert!(document::decode_native(bad, BTreeMap::new()).is_err());
            }
            let mut disguised = original.clone();
            disguised.version = 48;
            let current = e.current.clone();
            let undo = e.undo.clone();
            let redo = e.redo.clone();
            assert!(e.replace_project(disguised).is_err());
            assert_eq!(e.current, current);
            assert_eq!(e.undo, undo);
            assert_eq!(e.redo, redo);
            let value = serde_json::to_value(&original).unwrap();
            for invalid in ["base", "key", "time", "content"] {
                let mut bad = value.clone();
                let layer = &mut bad["other_compositions"]["1"]["layers"][0];
                let name = serde_json::to_value(parameter)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned();
                match invalid {
                    "base" => {
                        layer["text_parameters"][&name]["value"] =
                            (parameter.bounds().0 - 1.).into()
                    }
                    "key" => {
                        layer["text_parameters"][&name]["keys"]["20"] = serde_json::json!({"value": parameter.bounds().1 + 1., "interpolation":"Linear"})
                    }
                    "time" => {
                        layer["text_parameters"][&name]["keys"]["150"] =
                            serde_json::json!({"value": base, "interpolation":"Linear"})
                    }
                    _ => layer["content"] = "Rectangle".into(),
                }
                assert!(
                    Project::from_json(&bad.to_string()).is_err(),
                    "{parameter:?} {invalid}"
                );
                assert!(document::decode_native(bad, BTreeMap::new()).is_err());
            }
        }
    }
    let mut paint = scene();
    edit(
        &mut paint,
        TextParam::FillRed,
        TrackEdit::ToggleKey { frame: 0 },
    );
    assert_eq!(paint.project().version, 48);
}

#[test]
fn text_typography_generic_copy_move_scale_delete_and_incompatible_replacement() {
    let mut e = animated_scene();
    let original = e.selected_layer().unwrap().text_parameters.clone();
    let refs = |frame| {
        TYPOGRAPHY
            .map(|parameter| KeyRef {
                id: 1,
                property: PropertyPath::Text(parameter),
                frame,
            })
            .to_vec()
    };
    let copies: Vec<_> = TYPOGRAPHY
        .into_iter()
        .map(|p| {
            e.selected_layer()
                .unwrap()
                .copy_key(PropertyPath::Text(p), 10)
                .unwrap()
        })
        .collect();
    e.execute(Command::PasteKeys {
        keys: copies.clone(),
        frame: 60,
        target: None,
    })
    .unwrap();
    e.execute(Command::MoveKeys {
        keys: refs(60),
        delta: 10,
    })
    .unwrap();
    e.execute(Command::ScaleKeys {
        keys: refs(70),
        scale: KeyScale {
            time_origin: 0.,
            time_scale: 1.5,
            value_origin: 0.,
            value_scale: 0.5,
        },
    })
    .unwrap();
    for parameter in TYPOGRAPHY {
        let mut expected = original[&parameter].keys[&10].clone();
        expected.value *= 0.5;
        expected.temporal.outgoing.as_mut().unwrap().slope *= 0.5 / 1.5;
        assert_eq!(
            e.selected_layer().unwrap().text_parameters[&parameter].keys[&105],
            expected
        );
    }
    assert_rejected(
        &mut e,
        Command::ScaleKeys {
            keys: refs(105),
            scale: KeyScale {
                time_origin: 0.,
                time_scale: 1.,
                value_origin: 0.,
                value_scale: 100000.,
            },
        },
    );
    e.execute(Command::DeleteKeys(refs(105))).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, original);
    e.execute(Command::DeleteKeys(refs(10))).unwrap();
    e.execute(Command::DeleteKeys(refs(40))).unwrap();
    for parameter in TYPOGRAPHY {
        assert!(
            e.selected_layer().unwrap().text_parameters[&parameter]
                .keys
                .is_empty()
        );
        assert_eq!(
            sample(&e, parameter, 90),
            original[&parameter].keys[&40].value
        );
    }
    assert_eq!(e.project().version, 49);
    assert_rejected(
        &mut e,
        Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        },
    );
    e.execute(Command::AddRectangle).unwrap();
    assert_rejected(
        &mut e,
        Command::PasteKeys {
            keys: copies,
            frame: 20,
            target: Some(2),
        },
    );
}

#[test]
fn text_typography_duplicate_split_shift_clipboard_fps_and_precompose_preserve_tracks() {
    let mut e = animated_scene();
    let tracks = e.selected_layer().unwrap().text_parameters.clone();
    e.execute(Command::DuplicateLayer(1)).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, tracks);
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 25,
    })
    .unwrap();
    let split = e.selected_layer().unwrap().id();
    assert_eq!(e.selected_layer().unwrap().text_parameters, tracks);
    assert_eq!(
        e.project().composition().layer(1).unwrap().text_parameters,
        tracks
    );
    e.execute(Command::SetLayerRange {
        id: split,
        start: 25,
        end: 100,
    })
    .unwrap();
    e.execute(Command::ShiftLayer {
        id: split,
        delta: 5,
    })
    .unwrap();
    for parameter in TYPOGRAPHY {
        let t = &e.selected_layer().unwrap().text_parameters[&parameter];
        assert_eq!(t.keys[&15], tracks[&parameter].keys[&10]);
        assert_eq!(t.keys[&45], tracks[&parameter].keys[&40]);
    }
    let clipboard = e.copy_layers(&[split]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "60fps".into(),
        width: 1920,
        height: 1080,
        fps: 60,
        duration: 300,
    })
    .unwrap();
    e.execute(Command::PasteLayers(clipboard)).unwrap();
    let pasted = e.selected_layer().unwrap().id();
    let pasted_tracks = e.selected_layer().unwrap().text_parameters.clone();
    for parameter in TYPOGRAPHY {
        let mut expected = tracks[&parameter].keys[&10].clone();
        expected.temporal.outgoing.as_mut().unwrap().slope *= 0.5;
        assert_eq!(pasted_tracks[&parameter].keys[&30], expected);
        assert_eq!(
            pasted_tracks[&parameter].keys[&90],
            tracks[&parameter].keys[&40]
        );
    }
    e.execute(Command::DuplicateComposition).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, pasted_tracks);
    e.activate_composition(2).unwrap();
    e.execute(Command::Precompose {
        layers: vec![pasted],
        name: "Typography precomp".into(),
    })
    .unwrap();
    let Content::Composition { composition, .. } = e.selected_layer().unwrap().content() else {
        panic!("precomp")
    };
    assert_eq!(
        e.project()
            .composition_by_id(*composition)
            .unwrap()
            .layer(pasted)
            .unwrap()
            .text_parameters,
        pasted_tracks
    );
    // Independent origin survives split, shift, FPS conversion and precompose.
    assert_eq!(
        e.project()
            .composition_by_id(*composition)
            .unwrap()
            .layer(pasted)
            .unwrap()
            .start_frame,
        Some(10)
    );
    assert_eq!(e.project().version, 64);
    let bytes = project_file::encode(e.project(), None).unwrap();
    assert_eq!(project_file::decode(&bytes).unwrap().project, *e.project());
}
