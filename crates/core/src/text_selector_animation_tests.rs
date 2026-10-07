//! Stable secondary-selector animation, sparse source and atomic history contracts.
use super::*;
use serde_json::{Value, json};

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "A e\u{301}\r\n👩‍💻".into(),
                font_size: 40.,
            },
            width: 300.,
            height: 180.,
            name: "Animated selectors".into(),
        })
        .unwrap();
    editor
        .execute(Command::AddTextRangeSelector { id: 1 })
        .unwrap();
    editor.clear_history();
    editor
}
fn path(selector: u64, parameter: TextSelectorParam) -> PropertyPath {
    PropertyPath::TextSelector {
        selector,
        parameter,
    }
}
fn edit(selector: u64, parameter: TextSelectorParam, edit: TrackEdit) -> Command {
    Command::EditTrack {
        id: 1,
        property: path(selector, parameter),
        edit,
    }
}
fn value(parameter: TextSelectorParam, frame: Frame, value: f64) -> Command {
    edit(1, parameter, TrackEdit::Value { frame, value })
}
fn key(value: f64) -> Keyframe {
    Keyframe {
        value,
        interpolation: Interpolation::Linear,
        temporal: TemporalHandles::default(),
    }
}
fn item(editor: &Editor) -> &TextRangeSelector {
    &editor.selected_layer().unwrap().text_range_selectors()[0]
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
    assert_eq!(
        (
            editor.current.clone(),
            editor.undo.clone(),
            editor.redo.clone(),
            editor.context_generation()
        ),
        before
    );
}
fn animate(editor: &mut Editor, parameter: TextSelectorParam, a: f64, b: f64) {
    editor.execute(value(parameter, 0, a)).unwrap();
    editor
        .execute(edit(1, parameter, TrackEdit::ToggleAnimation { frame: 0 }))
        .unwrap();
    editor
        .execute(edit(1, parameter, TrackEdit::ToggleKey { frame: 20 }))
        .unwrap();
    editor.execute(value(parameter, 20, b)).unwrap();
}
fn raw_layer(raw: &mut Value, inactive: bool) -> &mut Value {
    if inactive {
        &mut raw["other_compositions"]["1"]["layers"][0]
    } else {
        &mut raw["composition"]["layers"][0]
    }
}
fn resident_layer(project: &mut Project, inactive: bool) -> &mut Layer {
    if inactive {
        &mut project.other_compositions.get_mut(&1).unwrap().layers[0]
    } else {
        &mut project.composition.layers[0]
    }
}
fn rejected_wire(raw: Value) {
    assert!(
        Project::from_json(&raw.to_string()).is_err(),
        "JSON accepted {raw}"
    );
    assert!(
        document::decode_native(raw.clone(), BTreeMap::new()).is_err(),
        "Native accepted {raw}"
    );
}

#[test]
fn secondary_animation_static_schema61_bytes_and_missing_track_defaults_stay_sparse() {
    let mut editor = scene();
    assert_eq!(
        serde_json::to_value(item(&editor)).unwrap(),
        json!({
            "id":1, "mode":"Add", "start":0.0, "end":100.0, "offset":0.0, "amount":100.0,
            "selector":{"units":"Graphemes", "shape":"Square"}
        })
    );
    for (parameter, expected) in TextSelectorParam::ALL.into_iter().zip([0., 100., 0., 100.]) {
        let layer = editor.selected_layer().unwrap();
        assert_eq!(layer.track_value(path(1, parameter), 37), Some(expected));
        assert_eq!(item(&editor).value_at(parameter, 37), expected);
        assert!(layer.track(path(1, parameter)).is_none());
        assert!(!layer.track_paths().contains(&path(1, parameter)));
        assert_eq!(
            layer.track_label(path(1, parameter)),
            Some(format!("Selector 1 · {}", parameter.label()))
        );
        assert!(
            layer
                .text_selector_value_command(1, parameter, expected, 37)
                .unwrap()
                .is_none()
        );
        with_redo(&mut editor);
        preserved(&mut editor, value(parameter, 37, expected), false);
        let command = editor
            .selected_layer()
            .unwrap()
            .text_selector_value_command(1, parameter, 25., 37)
            .unwrap()
            .unwrap();
        editor.execute(command).unwrap();
        assert!(item(&editor).parameters.is_empty());
        assert_eq!(item(&editor).value_at(parameter, 0), 25.);
        assert_eq!(editor.project().version, 61);
    }
    let json = editor.project().to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
}

#[test]
fn secondary_animation_all_channels_sample_the_current_frame_and_preserve_baselines() {
    let mut editor = scene();
    for (parameter, a, b) in [
        (TextSelectorParam::Start, 10., 30.),
        (TextSelectorParam::End, 50., 90.),
        (TextSelectorParam::Offset, -10., 30.),
        (TextSelectorParam::Amount, 20., 80.),
    ] {
        animate(&mut editor, parameter, a, b);
    }
    let source = item(&editor);
    assert_eq!(
        (source.start, source.end, source.offset, source.amount),
        (10., 50., -10., 20.)
    );
    assert_eq!(editor.project().version, 62);
    assert_eq!(source.parameters.len(), 4);
    let sample = editor
        .selected_layer()
        .unwrap()
        .text_animator_at(10)
        .unwrap();
    assert_eq!(
        (
            sample.selectors[0].start,
            sample.selectors[0].end,
            sample.selectors[0].amount
        ),
        (30., 80., 50.)
    );
    assert_eq!((sample.start, sample.end, sample.amount), (0., 100., 100.));
    assert!(editor.selected_layer().unwrap().text_parameters.is_empty());
    for parameter in TextSelectorParam::ALL {
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .track_paths()
                .contains(&path(1, parameter))
        );
    }
    with_redo(&mut editor);
    preserved(
        &mut editor,
        value(TextSelectorParam::Amount, 10, 50.),
        false,
    );
    assert!(
        !item(&editor).parameters[&TextSelectorParam::Amount]
            .keys()
            .contains_key(&10)
    );
}

#[test]
fn secondary_animation_curve_overshoot_is_clamped_and_disabling_bakes_visible_sample() {
    for parameter in TextSelectorParam::ALL {
        let mut editor = scene();
        animate(&mut editor, parameter, 50., 50.);
        editor
            .execute(Command::SetTemporalHandle {
                id: 1,
                property: path(1, parameter),
                frame: 0,
                incoming: false,
                handle: TemporalHandle {
                    influence: 0.5,
                    slope: 40.,
                },
            })
            .unwrap();
        editor
            .execute(Command::SetTemporalHandle {
                id: 1,
                property: path(1, parameter),
                frame: 20,
                incoming: true,
                handle: TemporalHandle {
                    influence: 0.5,
                    slope: -40.,
                },
            })
            .unwrap();
        assert!(item(&editor).parameters[&parameter].value_at(10) > 100.);
        assert_eq!(item(&editor).value_at(parameter, 10), 100.);
        with_redo(&mut editor);
        preserved(&mut editor, value(parameter, 10, 100.), false);
        editor
            .execute(edit(1, parameter, TrackEdit::ToggleAnimation { frame: 10 }))
            .unwrap();
        let track = &item(&editor).parameters[&parameter];
        assert!(track.keys.is_empty());
        assert_eq!(track.value, 100.);
        let source = item(&editor);
        let baseline = match parameter {
            TextSelectorParam::Start => source.start,
            TextSelectorParam::End => source.end,
            TextSelectorParam::Offset => source.offset,
            TextSelectorParam::Amount => source.amount,
        };
        assert_eq!(baseline, 50.);
        assert_eq!(editor.project().version, 62);
    }
}

#[test]
fn secondary_animation_reorder_remove_undo_and_duplicate_preserve_stable_tracks() {
    let mut editor = scene();
    animate(&mut editor, TextSelectorParam::Amount, 10., 90.);
    editor
        .execute(Command::AddTextRangeSelector { id: 1 })
        .unwrap();
    editor
        .execute(edit(
            2,
            TextSelectorParam::Offset,
            TrackEdit::ToggleKey { frame: 7 },
        ))
        .unwrap();
    let before = editor.current.clone();
    editor
        .execute(Command::MoveTextRangeSelector {
            id: 1,
            selector: 2,
            index: 0,
        })
        .unwrap();
    let layer = editor.selected_layer().unwrap();
    assert_eq!(layer.text_range_selectors()[0].id, 2);
    assert_eq!(
        layer.track_value(path(1, TextSelectorParam::Amount), 10),
        Some(50.)
    );
    assert_eq!(
        layer
            .track(path(2, TextSelectorParam::Offset))
            .unwrap()
            .keys()
            .len(),
        1
    );
    editor.undo();
    assert_eq!(editor.current, before);
    editor.redo();
    let reordered = editor.current.clone();
    editor
        .execute(Command::RemoveTextRangeSelector { id: 1, selector: 1 })
        .unwrap();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .track(path(1, TextSelectorParam::Amount))
            .is_none()
    );
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .track_value(path(1, TextSelectorParam::Amount), 10),
        None
    );
    with_redo(&mut editor);
    preserved(&mut editor, value(TextSelectorParam::Amount, 10, 42.), true);
    // Restore real redo after the rejection-preservation check.
    editor.redo.clear();
    editor.undo();
    assert_eq!(editor.current, reordered);
    editor.execute(Command::DuplicateComposition).unwrap();
    let copy = &editor.project().composition.layers[0];
    assert_eq!(
        copy.text_range_selectors(),
        reordered.project.composition.layers[0].text_range_selectors()
    );
}

#[test]
fn secondary_animation_generic_key_temporal_and_history_commands_preserve_other_source() {
    let mut editor = scene();
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 30,
            text: "Different source".into(),
        })
        .unwrap();
    editor
        .execute(Command::EditText {
            id: 1,
            parameter: TextParam::AnimatorOffset,
            edit: TrackEdit::ToggleKey { frame: 15 },
        })
        .unwrap();
    animate(&mut editor, TextSelectorParam::Amount, 20., 80.);
    let untouched = editor.selected_layer().unwrap().clone();
    let property = path(1, TextSelectorParam::Amount);
    let reference = |frame| KeyRef {
        id: 1,
        property,
        frame,
    };
    let mut states = vec![editor.current.clone()];
    for command in [
        Command::SetTemporalMode {
            id: 1,
            property,
            frame: 0,
            mode: TemporalMode::Auto,
        },
        Command::SetTemporalHandle {
            id: 1,
            property,
            frame: 20,
            incoming: true,
            handle: TemporalHandle {
                influence: 0.25,
                slope: 1.,
            },
        },
        edit(
            1,
            TextSelectorParam::Amount,
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Smooth,
            },
        ),
        Command::MoveKeys {
            keys: vec![reference(20)],
            delta: 10,
        },
        Command::ScaleKeys {
            keys: vec![reference(30)],
            scale: KeyScale {
                time_origin: 0.,
                time_scale: 2.,
                value_origin: 20.,
                value_scale: 0.5,
            },
        },
        Command::ScaleKeyVelocities {
            keys: vec![reference(60)],
            scale: KeyVelocityScale {
                origin: 0.,
                factor: 0.5,
            },
        },
    ] {
        editor.execute(command).unwrap();
        if states.last() != Some(&editor.current) {
            states.push(editor.current.clone());
        }
    }
    let copied = editor
        .selected_layer()
        .unwrap()
        .copy_key(property, 60)
        .unwrap();
    let copied_data = copied.data.clone();
    editor
        .execute(Command::PasteKeys {
            keys: vec![copied],
            frame: 80,
            target: Some(1),
        })
        .unwrap();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .track(property)
            .unwrap()
            .keys()[&80],
        copied_data
    );
    states.push(editor.current.clone());
    editor
        .execute(Command::DeleteKeys(vec![reference(80)]))
        .unwrap();
    states.push(editor.current.clone());
    let mut actual = editor.selected_layer().unwrap().clone();
    actual.text_range_selectors = untouched.text_range_selectors.clone();
    assert_eq!(actual, untouched);
    for expected in states[..states.len() - 1].iter().rev() {
        editor.undo();
        assert_eq!(&editor.current, expected);
    }
    for expected in &states[1..] {
        editor.redo();
        assert_eq!(&editor.current, expected);
    }
    with_redo(&mut editor);
    preserved(
        &mut editor,
        Command::MoveKeys {
            keys: vec![reference(60)],
            delta: 0,
        },
        false,
    );
    preserved(
        &mut editor,
        Command::ScaleKeys {
            keys: vec![reference(60)],
            scale: KeyScale {
                time_origin: 0.,
                time_scale: 1.,
                value_origin: 0.,
                value_scale: 1.,
            },
        },
        false,
    );
    preserved(
        &mut editor,
        Command::ScaleKeyVelocities {
            keys: vec![reference(60)],
            scale: KeyVelocityScale {
                origin: 0.,
                factor: 1.,
            },
        },
        false,
    );
}

#[test]
fn secondary_animation_codec_requires_schema62_for_stored_tracks_only() {
    for inactive in [false, true] {
        for keyed in [false, true] {
            let mut editor = scene();
            for parameter in TextSelectorParam::ALL {
                editor
                    .execute(edit(1, parameter, TrackEdit::ToggleKey { frame: 7 }))
                    .unwrap();
                if !keyed {
                    editor
                        .execute(edit(1, parameter, TrackEdit::ToggleKey { frame: 7 }))
                        .unwrap();
                }
            }
            if inactive {
                editor.execute(Command::NewComposition).unwrap();
            }
            let project = editor.project();
            assert_eq!(project.version, 62);
            let json = project.to_json().unwrap();
            assert_eq!(Project::from_json(&json).unwrap(), *project);
            let view = br#" { "version":2, "pins":[{"version":2,"layer":1,"property":{"TextSelector":{"selector":1,"parameter":"Amount"}}}] } "#;
            let bytes = project_file::encode(project, Some(view)).unwrap();
            let decoded = project_file::decode(&bytes).unwrap();
            assert_eq!(decoded.project, *project);
            assert_eq!(decoded.view, Some(view.as_slice()));
            assert_eq!(
                project_file::encode(&decoded.project, decoded.view).unwrap(),
                bytes
            );
            for version in [3, 59, 60, 61, PROJECT_VERSION + 1] {
                let mut invalid = project.clone();
                invalid.version = version;
                assert!(invalid.validate().is_err());
                assert!(project_file::encode(&invalid, None).is_err());
                rejected_wire(serde_json::to_value(invalid).unwrap());
            }
            let mut raw = serde_json::to_value(project).unwrap();
            raw_layer(&mut raw, inactive)["text_range_selectors"][0]["parameters"] = json!({});
            raw["version"] = json!(61);
            let legacy = Project::from_json(&raw.to_string()).unwrap();
            assert_eq!(legacy.version, 61);
            assert_eq!(text_animation::animator_required_version(&legacy), Some(61));
        }
    }
}

#[test]
fn secondary_animation_invalid_wire_tracks_baselines_and_temporal_metadata_reject() {
    for inactive in [false, true] {
        let mut editor = scene();
        animate(&mut editor, TextSelectorParam::Amount, 10., 90.);
        if inactive {
            editor.execute(Command::NewComposition).unwrap();
        }
        let base = serde_json::to_value(editor.project()).unwrap();
        for invalid in [
            Value::Null,
            json!([]),
            json!({"Bogus":{"value":10,"keys":{}}}),
        ] {
            let mut raw = base.clone();
            raw_layer(&mut raw, inactive)["text_range_selectors"][0]["parameters"] = invalid;
            rejected_wire(raw);
        }
        for parameter in TextSelectorParam::ALL {
            for invalid_value in [
                parameter.bounds().0 - 0.001,
                parameter.bounds().1 + 0.001,
                f64::NAN,
                f64::INFINITY,
            ] {
                for keyed in [false, true] {
                    let mut broken = editor.project().clone();
                    let selector =
                        &mut resident_layer(&mut broken, inactive).text_range_selectors[0];
                    let mut track = AnimatedProperty::new(25.);
                    if keyed {
                        track.keys.insert(7, key(invalid_value));
                    } else {
                        track.value = invalid_value;
                    }
                    selector.parameters.insert(parameter, track);
                    assert!(broken.validate().is_err());
                    assert!(project_file::encode(&broken, None).is_err());
                    rejected_wire(serde_json::to_value(broken).unwrap());
                }
            }
        }
        for invalid in 0..8 {
            let mut broken = editor.project().clone();
            let duration = broken.composition.duration;
            let selector = &mut resident_layer(&mut broken, inactive).text_range_selectors[0];
            let track = selector
                .parameters
                .get_mut(&TextSelectorParam::Amount)
                .unwrap();
            match invalid {
                0 => {
                    track.keys.insert(duration, key(50.));
                }
                1 => {
                    track.keys.get_mut(&0).unwrap().interpolation = Interpolation::Bezier(Bezier {
                        x1: -1.,
                        y1: 0.,
                        x2: 1.,
                        y2: 1.,
                    });
                }
                2 => {
                    track.keys.get_mut(&0).unwrap().temporal.outgoing = Some(TemporalHandle {
                        influence: 0.,
                        slope: 1.,
                    });
                }
                3 => {
                    track.keys.get_mut(&0).unwrap().temporal = TemporalHandles {
                        mode: TemporalMode::Auto,
                        incoming: Some(TemporalHandle {
                            influence: 0.5,
                            slope: 1.,
                        }),
                        outgoing: None,
                    };
                }
                4 => selector.amount = -1.,
                5 => {
                    track.keys = (0..10_001).map(|frame| (frame, key(50.))).collect();
                }
                6 => selector.id = 99,
                _ => resident_layer(&mut broken, inactive).content = Content::Rectangle,
            }
            assert!(broken.validate().is_err(), "accepted case {invalid}");
            assert!(project_file::encode(&broken, None).is_err());
            rejected_wire(serde_json::to_value(broken).unwrap());
        }
    }
}

#[test]
fn secondary_animation_invalid_commands_and_stale_key_batches_are_atomic() {
    for parameter in TextSelectorParam::ALL {
        let mut editor = scene();
        with_redo(&mut editor);
        for bad in [
            parameter.bounds().0 - 0.001,
            parameter.bounds().1 + 0.001,
            f64::NAN,
            f64::INFINITY,
        ] {
            assert!(
                editor
                    .selected_layer()
                    .unwrap()
                    .text_selector_value_command(1, parameter, bad, 0)
                    .is_err()
            );
            preserved(&mut editor, value(parameter, 0, bad), true);
        }
        for frame in [editor.project().composition.duration, u32::MAX] {
            preserved(&mut editor, value(parameter, frame, 25.), true);
            preserved(
                &mut editor,
                edit(1, parameter, TrackEdit::ToggleKey { frame }),
                true,
            );
        }
        preserved(
            &mut editor,
            edit(999, parameter, TrackEdit::ToggleKey { frame: 0 }),
            true,
        );
        preserved(
            &mut editor,
            Command::Batch(vec![
                value(parameter, 0, 25.),
                edit(999, parameter, TrackEdit::ToggleKey { frame: 0 }),
            ]),
            true,
        );
        editor.current.project.composition.layers[0].locked = true;
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .text_selector_value_command(1, parameter, 25., 0)
                .is_err()
        );
        preserved(&mut editor, value(parameter, 0, 25.), true);
        editor.current.project.composition.layers[0].locked = false;
        animate(&mut editor, parameter, 10., 90.);
        with_redo(&mut editor);
        let reference = KeyRef {
            id: 1,
            property: path(1, parameter),
            frame: 20,
        };
        let stale = KeyRef {
            id: 1,
            property: path(999, parameter),
            frame: 20,
        };
        let mut missing_copy = editor
            .selected_layer()
            .unwrap()
            .copy_key(reference.property, 20)
            .unwrap();
        missing_copy.key = stale;
        for command in [
            Command::MoveKeys {
                keys: vec![reference, stale],
                delta: 5,
            },
            Command::DeleteKeys(vec![reference, stale]),
            Command::PasteKeys {
                keys: vec![
                    editor
                        .selected_layer()
                        .unwrap()
                        .copy_key(reference.property, 20)
                        .unwrap(),
                    missing_copy,
                ],
                frame: 50,
                target: None,
            },
            Command::ScaleKeys {
                keys: vec![reference],
                scale: KeyScale {
                    time_origin: 0.,
                    time_scale: 1.,
                    value_origin: 0.,
                    value_scale: 100.,
                },
            },
            edit(
                1,
                parameter,
                TrackEdit::Keyframe {
                    from: 20,
                    to: 0,
                    value: 25.,
                },
            ),
        ] {
            preserved(&mut editor, command, true);
        }
    }
}

#[test]
fn secondary_animation_rejects_invalid_sources_before_noop_or_repair() {
    for inactive in [false, true] {
        let mut editor = scene();
        animate(&mut editor, TextSelectorParam::Amount, 10., 90.);
        if inactive {
            editor.execute(Command::NewComposition).unwrap();
        }
        editor.current.project.version = 61;
        with_redo(&mut editor);
        for command in [
            value(TextSelectorParam::Amount, 0, 10.),
            Command::RemoveTextRangeSelector { id: 1, selector: 1 },
            Command::SetTextRangeSelector {
                id: 1,
                selector: TextRangeSelector::new(1),
            },
            edit(
                1,
                TextSelectorParam::Amount,
                TrackEdit::ToggleAnimation { frame: 0 },
            ),
            Command::DeleteKeys(vec![KeyRef {
                id: 1,
                property: path(1, TextSelectorParam::Amount),
                frame: 0,
            }]),
        ] {
            preserved(&mut editor, command, true);
        }
    }
}

#[test]
fn secondary_animation_paste_materializes_target_baseline_without_rebinding_selector_ids() {
    let mut editor = scene();
    animate(&mut editor, TextSelectorParam::Amount, 10., 90.);
    let property = path(1, TextSelectorParam::Amount);
    let copied = editor
        .selected_layer()
        .unwrap()
        .copy_key(property, 20)
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Target".into(),
                font_size: 20.,
            },
            width: 100.,
            height: 100.,
            name: "Target".into(),
        })
        .unwrap();
    editor
        .execute(Command::AddTextRangeSelector { id: 2 })
        .unwrap();
    let mut target = TextRangeSelector::new(1);
    target.amount = 33.;
    editor
        .execute(Command::SetTextRangeSelector {
            id: 2,
            selector: target,
        })
        .unwrap();
    editor
        .execute(Command::PasteKeys {
            keys: vec![copied.clone()],
            frame: 40,
            target: Some(2),
        })
        .unwrap();
    let target = editor.project().composition.layer(2).unwrap();
    assert_eq!(target.text_range_selectors()[0].amount, 33.);
    assert_eq!(target.track(property).unwrap().value, 33.);
    assert_eq!(target.track(property).unwrap().keys()[&40], copied.data);
    assert_eq!(target.track_value(property, 40), Some(90.));
    editor
        .execute(Command::RemoveTextRangeSelector { id: 2, selector: 1 })
        .unwrap();
    editor
        .execute(Command::AddTextRangeSelector { id: 2 })
        .unwrap();
    assert_eq!(
        editor
            .project()
            .composition
            .layer(2)
            .unwrap()
            .text_range_selectors()[0]
            .id,
        2
    );
    with_redo(&mut editor);
    preserved(
        &mut editor,
        Command::PasteKeys {
            keys: vec![copied],
            frame: 50,
            target: Some(2),
        },
        true,
    );
}

#[test]
fn secondary_animation_layer_shift_and_cross_fps_paste_retime_owned_tracks() {
    let mut editor = scene();
    animate(&mut editor, TextSelectorParam::Offset, -20., 20.);
    let property = path(1, TextSelectorParam::Offset);
    editor
        .execute(Command::SetTemporalHandle {
            id: 1,
            property,
            frame: 0,
            incoming: false,
            handle: TemporalHandle {
                influence: 0.25,
                slope: 2.,
            },
        })
        .unwrap();
    editor.current.project.composition.layers[0].out_frame = Some(100);
    let before = editor.current.clone();
    editor
        .execute(Command::ShiftLayer { id: 1, delta: 10 })
        .unwrap();
    let shifted = editor.current.clone();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .track(property)
            .unwrap()
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![10, 30]
    );
    editor.undo();
    assert_eq!(editor.current, before);
    editor.redo();
    assert_eq!(editor.current, shifted);
    let clipboard = editor.copy_layers(&[1]).unwrap();
    editor.execute(Command::NewComposition).unwrap();
    editor.current.project.composition.fps = FrameRate::from(60);
    editor.current.project.composition.duration = 300;
    editor.execute(Command::PasteLayers(clipboard)).unwrap();
    let track = editor.selected_layer().unwrap().track(property).unwrap();
    assert_eq!(
        track.keys().keys().copied().collect::<Vec<_>>(),
        vec![20, 60]
    );
    assert_eq!(track.keys()[&20].temporal.outgoing.unwrap().slope, 1.);
    assert_eq!(
        editor.selected_layer().unwrap().track_value(property, 40),
        shifted
            .project
            .composition
            .layer(1)
            .unwrap()
            .track_value(property, 20)
    );
}
