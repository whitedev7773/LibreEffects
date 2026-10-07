//! Sparse selector configuration and animated strength keep legacy sources exact.
use super::*;

const AMOUNT: TextParam = TextParam::AnimatorAmount;
const PATH: PropertyPath = PropertyPath::Text(AMOUNT);

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "A e\u{301}\r\n\n한 👩‍💻\n".into(),
                font_size: 40.,
            },
            width: 300.,
            height: 180.,
            name: "Selector".into(),
        })
        .unwrap();
    editor.clear_history();
    editor
}
fn selector(units: TextSelectorUnits, shape: TextSelectorShape) -> TextSelector {
    TextSelector { units, shape }
}
fn config(selector: TextSelector) -> Command {
    Command::SetTextSelector { id: 1, selector }
}
fn edit(edit: TrackEdit, generic: bool) -> Command {
    if generic {
        Command::EditTrack {
            id: 1,
            property: PATH,
            edit,
        }
    } else {
        Command::EditText {
            id: 1,
            parameter: AMOUNT,
            edit,
        }
    }
}
fn value(frame: Frame, value: f64, generic: bool) -> Command {
    edit(TrackEdit::Value { frame, value }, generic)
}
fn with_redo(editor: &mut Editor) {
    let mut future = editor.current.clone();
    future.project.composition.name = "Preserved future".into();
    editor.redo = vec![future];
}
fn preserved(editor: &mut Editor, command: Command, rejects: bool) {
    let before = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
        editor.context_generation(),
    );
    let result = editor.execute(command);
    assert_eq!(result.is_err(), rejects, "{result:?}");
    assert_eq!(editor.current, before.0);
    assert_eq!(editor.undo, before.1);
    assert_eq!(editor.redo, before.2);
    assert_eq!(editor.context_generation(), before.3);
}

#[test]
fn text_selector_defaults_and_amount_are_sparse_and_preserve_legacy_noops() {
    assert_eq!(TextParam::ALL[18], AMOUNT);
    assert_eq!(AMOUNT.bounds(), (0., 100.));
    assert_eq!(AMOUNT.required_version(), 59);
    assert!(AMOUNT.is_animator());
    assert_eq!(TextPaint::from_parameter(AMOUNT), None);
    assert_eq!(
        serde_json::to_string(&AMOUNT).unwrap(),
        "\"AnimatorAmount\""
    );
    assert_eq!(
        TextSelector::default(),
        selector(TextSelectorUnits::Graphemes, TextSelectorShape::Square)
    );
    for version in [3, 48, 49, 52, 53, 54, 55, 56, 57, 58, 59] {
        let mut editor = scene();
        editor.current.project.version = version;
        with_redo(&mut editor);
        let before = serde_json::to_vec(editor.project()).unwrap();
        for frame in [0, 30, u32::MAX] {
            let layer = editor.selected_layer().unwrap();
            assert_eq!(layer.text_selector(), TextSelector::default());
            assert_eq!(layer.text_value_at(AMOUNT, frame), Some(100.));
            assert_eq!(layer.track_value(PATH, frame), Some(100.));
            assert!(layer.track(PATH).is_none());
            assert!(!layer.track_paths().contains(&PATH));
            assert!(
                layer
                    .text_value_command(AMOUNT, 100., frame)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                layer.text_animator_at(frame),
                Some(TextAnimatorSample::default())
            );
        }
        preserved(&mut editor, config(TextSelector::default()), false);
        for generic in [false, true] {
            preserved(&mut editor, value(17, 100., generic), false);
        }
        assert_eq!(serde_json::to_vec(editor.project()).unwrap(), before);
        let json = editor.project().to_json().unwrap();
        assert!(!json.contains("text_selector"));
        assert!(!json.contains("AnimatorAmount"));
        assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
    }
}

#[test]
fn text_selector_every_configuration_changes_only_config_and_is_undoable() {
    for units in TextSelectorUnits::ALL {
        for shape in TextSelectorShape::ALL {
            let selected = selector(units, shape);
            if selected == TextSelector::default() {
                continue;
            }
            let mut editor = scene();
            editor
                .execute(Command::EditText {
                    id: 1,
                    parameter: TextParam::AnimatorOffset,
                    edit: TrackEdit::Value {
                        frame: 0,
                        value: -12.5,
                    },
                })
                .unwrap();
            editor
                .execute(edit(TrackEdit::ToggleAnimation { frame: 7 }, false))
                .unwrap();
            editor.execute(value(45, 37.25, true)).unwrap();
            editor.execute(Command::DuplicateComposition).unwrap();
            editor.activate_composition(1).unwrap();
            editor.select(1);
            editor.clear_history();
            let before = editor.current.clone();
            editor.execute(config(selected)).unwrap();
            let mut expected = before.clone();
            expected.project.version = 59;
            expected.project.composition.layers[0].text_selector = selected;
            assert_eq!(editor.current, expected);
            assert_eq!(editor.undo.len(), 1);
            for frame in [0, 7, 20, 45, 90] {
                let sample = editor
                    .selected_layer()
                    .unwrap()
                    .text_animator_at(frame)
                    .unwrap();
                assert_eq!((sample.units, sample.shape), (units, shape));
                assert_eq!(
                    sample.amount,
                    before.project.composition.layers[0]
                        .text_value_at(AMOUNT, frame)
                        .unwrap()
                );
            }
            editor.undo();
            assert_eq!(editor.current, before);
            editor.redo();
            assert_eq!(editor.current, expected);
            with_redo(&mut editor);
            preserved(&mut editor, config(selected), false);
            editor.execute(config(TextSelector::default())).unwrap();
            assert_eq!(
                editor.selected_layer().unwrap().text_selector(),
                TextSelector::default()
            );
            assert_eq!(editor.project().version, 59);
            let raw = serde_json::to_value(editor.project()).unwrap();
            assert!(
                raw["composition"]["layers"][0]
                    .get("text_selector")
                    .is_none()
            );
            editor.undo();
            assert_eq!(editor.current, expected);
        }
    }
    // A static selector alone raises the schema, without materializing scalar tracks.
    let mut editor = scene();
    let before = editor.current.clone();
    let selected = selector(TextSelectorUnits::Words, TextSelectorShape::RampUp);
    editor.execute(config(selected)).unwrap();
    let mut expected = before.clone();
    expected.project.version = 59;
    expected.project.composition.layers[0].text_selector = selected;
    assert_eq!(editor.current, expected);
    editor.undo();
    assert_eq!(editor.current, before);
    editor.redo();
    assert_eq!(editor.current, expected);
}

#[test]
fn text_selector_default_json_normalizes_on_legacy_and_nontext_layers() {
    for text in [false, true] {
        for version in [3, 48, 58, 59] {
            let mut editor = scene();
            editor.current.project.version = version;
            if !text {
                editor.current.project.composition.layers[0].content = Content::Rectangle;
            }
            let original = editor.project();
            for default in [
                serde_json::json!({}),
                serde_json::json!({"units":"Graphemes"}),
                serde_json::json!({"shape":"Square"}),
                serde_json::json!({"units":"Graphemes","shape":"Square"}),
            ] {
                let mut raw = serde_json::to_value(original).unwrap();
                raw["composition"]["layers"][0]["text_selector"] = default;
                let loaded = Project::from_json(&raw.to_string()).unwrap();
                assert_eq!(&loaded, original);
                assert!(!loaded.to_json().unwrap().contains("text_selector"));
            }
        }
    }
}

#[test]
fn text_selector_native_and_json_roundtrip_all_modes_in_active_or_inactive_compositions() {
    for inactive in [false, true] {
        for units in TextSelectorUnits::ALL {
            for shape in TextSelectorShape::ALL {
                for keyed in [false, true] {
                    let mut editor = scene();
                    editor.execute(config(selector(units, shape))).unwrap();
                    editor
                        .execute(edit(TrackEdit::ToggleKey { frame: 7 }, false))
                        .unwrap();
                    if !keyed {
                        editor
                            .execute(edit(TrackEdit::ToggleKey { frame: 7 }, true))
                            .unwrap();
                    }
                    if inactive {
                        editor.execute(Command::NewComposition).unwrap();
                    }
                    let project = editor.project();
                    assert_eq!(project.version, 59);
                    assert_eq!(
                        Project::from_json(&project.to_json().unwrap()).unwrap(),
                        *project
                    );
                    for view in [br#" { "version":1, "frame":30 } "#.as_slice(), br#" { "version":2, "pins":[{"version":1,"layer":1,"property":{"Text":"AnimatorAmount"}}], "future":true } "#.as_slice()] {
                        let native = project_file::encode(project, Some(view)).unwrap();
                        let decoded = project_file::decode(&native).unwrap();
                        assert_eq!(decoded.project, *project);
                        assert_eq!(decoded.view, Some(view));
                        assert_eq!(project_file::encode(&decoded.project, decoded.view).unwrap(), native);
                    }
                    for version in [3, 48, 55, 56, 57, 58, PROJECT_VERSION + 1] {
                        let mut bad = project.clone();
                        bad.version = version;
                        assert!(bad.validate().is_err());
                        let raw = serde_json::to_value(&bad).unwrap();
                        assert!(Project::from_json(&raw.to_string()).is_err());
                        assert!(document::decode_native(raw, BTreeMap::new()).is_err());
                        assert!(project_file::encode(&bad, None).is_err());
                    }
                }
            }
        }
    }
}

#[test]
fn text_selector_config_alone_requires_schema59_and_valid_text_storage_everywhere() {
    for inactive in [false, true] {
        let mut editor = scene();
        editor
            .execute(config(selector(
                TextSelectorUnits::Lines,
                TextSelectorShape::Triangle,
            )))
            .unwrap();
        if inactive {
            editor.execute(Command::NewComposition).unwrap();
        }
        for version in [3, 48, 56, 57, 58] {
            let mut bad = editor.project().clone();
            bad.version = version;
            assert!(bad.validate().is_err());
            assert!(Project::from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        }
        for invalid in [
            serde_json::json!({"units":"Characters","shape":"Square"}),
            serde_json::json!({"units":"Words","shape":"Wave"}),
            serde_json::json!({"units":null}),
            serde_json::json!({"shape":3}),
            serde_json::json!({"unknown":true}),
            serde_json::Value::Null,
        ] {
            let mut raw = serde_json::to_value(editor.project()).unwrap();
            let layer = if inactive {
                &mut raw["other_compositions"]["1"]["layers"][0]
            } else {
                &mut raw["composition"]["layers"][0]
            };
            layer["text_selector"] = invalid;
            assert!(Project::from_json(&raw.to_string()).is_err());
            assert!(document::decode_native(raw, BTreeMap::new()).is_err());
        }
        let mut bad = editor.project().clone();
        let layer = if inactive {
            &mut bad.other_compositions.get_mut(&1).unwrap().layers[0]
        } else {
            &mut bad.composition.layers[0]
        };
        layer.content = Content::Rectangle;
        assert!(bad.validate().is_err());
    }
}

#[test]
fn text_selector_commands_reject_nontext_locked_missing_and_partial_batches_atomically() {
    let selected = selector(TextSelectorUnits::Words, TextSelectorShape::RampDown);
    for nontext in [false, true] {
        let mut editor = scene();
        if nontext {
            editor.current.project.composition.layers[0].content = Content::Rectangle;
        } else {
            editor.current.project.composition.layers[0].locked = true;
        }
        with_redo(&mut editor);
        for settings in [TextSelector::default(), selected] {
            preserved(&mut editor, config(settings), true);
        }
        for generic in [false, true] {
            preserved(&mut editor, value(0, 100., generic), true);
            preserved(
                &mut editor,
                edit(TrackEdit::ToggleKey { frame: 0 }, generic),
                true,
            );
        }
    }
    let mut editor = scene();
    with_redo(&mut editor);
    preserved(
        &mut editor,
        Command::SetTextSelector {
            id: 999,
            selector: selected,
        },
        true,
    );
    preserved(
        &mut editor,
        Command::Batch(vec![config(selected), value(0, -1., false)]),
        true,
    );
    preserved(
        &mut editor,
        Command::Batch(vec![
            value(0, 25., true),
            Command::SetTextSelector {
                id: 999,
                selector: selected,
            },
        ]),
        true,
    );
    editor.execute(config(selected)).unwrap();
    preserved(
        &mut editor,
        Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        },
        true,
    );
}

#[test]
fn text_selector_source_validation_rejects_repairs_and_noops_before_schema_upgrade() {
    for amount in [false, true] {
        let mut editor = scene();
        let selected = selector(TextSelectorUnits::Words, TextSelectorShape::Square);
        if amount {
            editor
                .execute(edit(TrackEdit::ToggleKey { frame: 7 }, false))
                .unwrap();
        } else {
            editor.execute(config(selected)).unwrap();
        }
        editor.current.project.version = 58;
        with_redo(&mut editor);
        for command in [
            config(TextSelector::default()),
            config(selected),
            value(7, 100., false),
            value(7, 50., true),
        ] {
            preserved(&mut editor, command, true);
        }
    }
}

#[test]
fn text_selector_amount_edits_only_one_track_with_minimal_version_and_full_history() {
    for generic in [false, true] {
        let mut editor = scene();
        editor.execute(Command::DuplicateComposition).unwrap();
        editor.activate_composition(1).unwrap();
        editor.select(1);
        editor.clear_history();
        let before = editor.current.clone();
        editor
            .execute(value(17, 37.12345678912345, generic))
            .unwrap();
        let mut expected = before.clone();
        expected.project.version = 59;
        expected.project.composition.layers[0]
            .text_parameters
            .insert(AMOUNT, AnimatedProperty::new(37.12345678912345));
        assert_eq!(editor.current, expected);
        assert_eq!(editor.undo.len(), 1);
        editor.undo();
        assert_eq!(editor.current, before);
        editor.redo();
        assert_eq!(editor.current, expected);
        with_redo(&mut editor);
        preserved(&mut editor, value(17, 37.12345678912345, generic), false);
    }
}

#[test]
fn text_selector_amount_animation_is_independent_and_zero_is_identity() {
    let mut editor = scene();
    let selected = selector(TextSelectorUnits::Lines, TextSelectorShape::Triangle);
    editor.execute(config(selected)).unwrap();
    for (parameter, value) in [
        (TextParam::AnimatorStart, 20.),
        (TextParam::AnimatorEnd, 60.),
        (TextParam::AnimatorOffset, 10.),
        (TextParam::AnimatorPositionX, 24.),
        (TextParam::AnimatorPositionY, -8.),
        (TextParam::AnimatorOpacity, 0.),
    ] {
        editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::Value { frame: 0, value },
            })
            .unwrap();
    }
    editor.execute(value(0, 0., false)).unwrap();
    editor
        .execute(edit(TrackEdit::ToggleAnimation { frame: 0 }, true))
        .unwrap();
    editor.execute(value(60, 100., true)).unwrap();
    let before = editor.project().clone();
    for (frame, amount) in [(0, 0.), (15, 25.), (30, 50.), (60, 100.), (100, 100.)] {
        let sample = editor
            .selected_layer()
            .unwrap()
            .text_animator_at(frame)
            .unwrap();
        assert_eq!(
            sample,
            TextAnimatorSample {
                start: 30.,
                end: 70.,
                position: [24., -8.],
                opacity: 0.,
                units: selected.units,
                shape: selected.shape,
                amount,
                ..Default::default()
            }
        );
        assert_eq!(sample.is_identity(), amount == 0.);
    }
    assert_eq!(editor.project(), &before);
    with_redo(&mut editor);
    for generic in [false, true] {
        preserved(&mut editor, value(30, 50., generic), false);
    }
    assert!(
        !editor
            .selected_layer()
            .unwrap()
            .track(PATH)
            .unwrap()
            .keys
            .contains_key(&30)
    );
    editor
        .execute(edit(
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Hold,
            },
            false,
        ))
        .unwrap();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .text_animator_at(30)
            .unwrap()
            .amount,
        0.
    );
    editor
        .execute(edit(
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Bezier(Bezier::default()),
            },
            true,
        ))
        .unwrap();
    editor
        .execute(Command::SetTemporalHandle {
            id: 1,
            property: PATH,
            frame: 0,
            incoming: false,
            handle: TemporalHandle {
                slope: 100.,
                influence: 0.8,
            },
        })
        .unwrap();
    let sample = editor
        .selected_layer()
        .unwrap()
        .text_value_at(AMOUNT, 30)
        .unwrap();
    assert!((0.0..=100.0).contains(&sample));
    let before_disable = editor.project().clone();
    editor
        .execute(edit(TrackEdit::ToggleAnimation { frame: 30 }, true))
        .unwrap();
    let layer = editor.selected_layer().unwrap();
    assert_eq!(layer.track(PATH).unwrap().value, sample);
    assert!(layer.track(PATH).unwrap().keys.is_empty());
    assert_eq!(layer.text_selector(), selected);
    for parameter in TextParam::ALL.into_iter().filter(|p| *p != AMOUNT) {
        assert_eq!(
            layer.text_parameters.get(&parameter),
            before.composition.layers[0].text_parameters.get(&parameter)
        );
    }
    editor.undo();
    assert_eq!(editor.project(), &before_disable);
}

#[test]
fn text_selector_amount_invalid_values_frames_and_storage_are_rejected() {
    for generic in [false, true] {
        let mut editor = scene();
        with_redo(&mut editor);
        for bad in [-0.001, 100.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            preserved(&mut editor, value(0, bad, generic), true);
        }
        for frame in [editor.project().composition.duration, u32::MAX] {
            preserved(&mut editor, value(frame, 100., generic), true);
            preserved(
                &mut editor,
                edit(TrackEdit::ToggleKey { frame }, generic),
                true,
            );
        }
        editor
            .execute(edit(TrackEdit::ToggleAnimation { frame: 0 }, generic))
            .unwrap();
        for bad in [-1., 101., f64::INFINITY] {
            preserved(
                &mut editor,
                edit(
                    TrackEdit::Keyframe {
                        from: 0,
                        to: 20,
                        value: bad,
                    },
                    generic,
                ),
                true,
            );
        }
    }
    for inactive in [false, true] {
        for keyed in [false, true] {
            let mut editor = scene();
            editor
                .execute(edit(TrackEdit::ToggleKey { frame: 7 }, false))
                .unwrap();
            if !keyed {
                editor
                    .execute(edit(TrackEdit::ToggleKey { frame: 7 }, true))
                    .unwrap();
            }
            if inactive {
                editor.execute(Command::NewComposition).unwrap();
            }
            for invalid in [
                serde_json::Value::Null,
                serde_json::json!("100"),
                serde_json::json!(-1.),
                serde_json::json!(101.),
            ] {
                let mut raw = serde_json::to_value(editor.project()).unwrap();
                let layer = if inactive {
                    &mut raw["other_compositions"]["1"]["layers"][0]
                } else {
                    &mut raw["composition"]["layers"][0]
                };
                let track = &mut layer["text_parameters"]["AnimatorAmount"];
                if keyed {
                    track["keys"]["7"]["value"] = invalid;
                } else {
                    track["value"] = invalid;
                }
                assert!(Project::from_json(&raw.to_string()).is_err());
                assert!(document::decode_native(raw, BTreeMap::new()).is_err());
            }
            let mut bad = editor.project().clone();
            let layer = if inactive {
                &mut bad.other_compositions.get_mut(&1).unwrap().layers[0]
            } else {
                &mut bad.composition.layers[0]
            };
            layer.content = Content::Rectangle;
            assert!(bad.validate().is_err());
        }
    }
}

#[test]
fn text_selector_mixed_batches_and_duplicate_layers_retain_config_and_schema() {
    let mut editor = scene();
    let selected = selector(TextSelectorUnits::Words, TextSelectorShape::RampUp);
    editor
        .execute(Command::Batch(vec![
            config(selected),
            Command::RenameLayer {
                id: 1,
                name: "Mixed route".into(),
            },
        ]))
        .unwrap();
    assert_eq!(editor.project().version, 59);
    assert_eq!(editor.selected_layer().unwrap().text_selector(), selected);
    editor
        .execute(edit(TrackEdit::ToggleAnimation { frame: 0 }, false))
        .unwrap();
    editor.execute(value(60, 0., true)).unwrap();
    let original = editor.selected_layer().unwrap().clone();
    editor.execute(Command::DuplicateLayers(vec![1])).unwrap();
    let copy = editor.selected_layer().unwrap();
    assert_eq!(copy.text_selector(), selected);
    assert_eq!(copy.text_parameters, original.text_parameters);
    editor.execute(Command::NewComposition).unwrap();
    assert_eq!(editor.project().version, 59);
    assert_eq!(
        text_animation::animator_required_version(editor.project()),
        Some(59)
    );
}
