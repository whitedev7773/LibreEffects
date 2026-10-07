//! Bounded independent animator stacks, stable tracks and legacy source preservation.
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
            name: "Animator stack".into(),
        })
        .unwrap();
    editor.current.project.version = 3;
    editor.clear_history();
    editor
}
fn stacked() -> Editor {
    let mut editor = scene();
    editor.execute(Command::AddTextAnimator { id: 1 }).unwrap();
    editor.clear_history();
    editor
}
fn parameters() -> impl Iterator<Item = TextParam> {
    TextParam::ALL
        .into_iter()
        .filter(|parameter| parameter.is_animator())
}
fn path(animator: u64, parameter: TextParam) -> PropertyPath {
    PropertyPath::TextAnimator {
        animator,
        parameter,
    }
}
fn edit(animator: u64, parameter: TextParam, edit: TrackEdit) -> Command {
    Command::EditTrack {
        id: 1,
        property: path(animator, parameter),
        edit,
    }
}
fn value(parameter: TextParam, frame: Frame, value: f64) -> Command {
    edit(1, parameter, TrackEdit::Value { frame, value })
}
fn item(editor: &Editor) -> &TextAnimator {
    &editor.selected_layer().unwrap().text_animators()[0]
}
fn animate(editor: &mut Editor, parameter: TextParam, a: f64, b: f64) {
    editor.execute(value(parameter, 0, a)).unwrap();
    editor
        .execute(edit(1, parameter, TrackEdit::ToggleAnimation { frame: 0 }))
        .unwrap();
    editor
        .execute(edit(1, parameter, TrackEdit::ToggleKey { frame: 20 }))
        .unwrap();
    editor.execute(value(parameter, 20, b)).unwrap();
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
fn resident_layer(project: &mut Project, inactive: bool) -> &mut Layer {
    if inactive {
        &mut project.other_compositions.get_mut(&1).unwrap().layers[0]
    } else {
        &mut project.composition.layers[0]
    }
}
fn rejected_wire(raw: Value) {
    assert!(Project::from_json(&raw.to_string()).is_err());
    assert!(document::decode_native(raw, BTreeMap::new()).is_err());
}

#[test]
fn multiple_animators_old_sources_defaults_and_primary_paths_remain_exact() {
    assert_eq!(MAX_TEXT_ANIMATORS, 3);
    assert_eq!(parameters().count(), 10);
    for version in [3, 48, 52, 56, 58, 59, 60, 61, 62] {
        let mut editor = scene();
        editor.current.project.version = version;
        let before = editor.project().to_json().unwrap();
        let layer = editor.selected_layer().unwrap();
        assert!(layer.text_animators().is_empty());
        assert_eq!(
            layer.text_animators_at(0),
            Some(vec![layer.text_animator_at(0).unwrap()])
        );
        assert!(!before.contains("text_animators"));
        assert!(!before.contains("next_text_animator_id"));
        assert_eq!(
            Project::from_json(&before).unwrap().to_json().unwrap(),
            before
        );
        editor.execute(Command::AddTextAnimator { id: 1 }).unwrap();
        let mut expected = serde_json::from_str::<Value>(&before).unwrap();
        expected["version"] = json!(63);
        expected["composition"]["layers"][0]["text_animators"] = json!([{"id":1}]);
        expected["composition"]["layers"][0]["next_text_animator_id"] = json!(2);
        assert_eq!(serde_json::to_value(editor.project()).unwrap(), expected);
        let layer = editor.selected_layer().unwrap();
        assert_eq!(
            layer.text_animators_at(0),
            Some(vec![TextAnimatorSample::default(); 2])
        );
        assert!(item(&editor).parameters.is_empty());
        for parameter in parameters() {
            assert_eq!(
                layer.track_value(path(1, parameter), 20),
                layer.text_value_at(parameter, 20)
            );
            assert!(layer.track(path(1, parameter)).is_none());
            assert!(!layer.track_paths().contains(&path(1, parameter)));
            assert!(
                layer
                    .track_label(path(1, parameter))
                    .unwrap()
                    .starts_with("Animator 1 · ")
            );
        }
        editor.undo();
        assert_eq!(editor.project().to_json().unwrap(), before);
    }
}

#[test]
fn multiple_animators_allocator_survives_last_removal_roundtrip_and_undo() {
    let mut editor = stacked();
    editor
        .execute(Command::RemoveTextAnimator { id: 1, animator: 1 })
        .unwrap();
    assert!(editor.selected_layer().unwrap().text_animators().is_empty());
    let json = editor.project().to_json().unwrap();
    assert!(json.contains("next_text_animator_id"));
    assert!(!json.contains("\"text_animators\""));
    assert_eq!(editor.project().version, 63);
    let restored = Project::from_json(&json).unwrap();
    editor.current.project = restored;
    editor.execute(Command::AddTextAnimator { id: 1 }).unwrap();
    assert_eq!(item(&editor).id, 2);
    editor.undo();
    assert_eq!(editor.project().to_json().unwrap(), json);
    editor.redo();
    assert_eq!(item(&editor).id, 2);
    with_redo(&mut editor);
    for command in [
        Command::RemoveTextAnimator { id: 1, animator: 1 },
        edit(
            1,
            TextParam::AnimatorAmount,
            TrackEdit::ToggleKey { frame: 0 },
        ),
    ] {
        preserved(&mut editor, command, true);
    }
}

#[test]
fn multiple_animators_ten_channels_current_frame_sparse_noops_and_ordered_samples() {
    let mut editor = stacked();
    let untouched = editor.selected_layer().unwrap().clone();
    for parameter in parameters() {
        let default = item(&editor).value_at(parameter, 0).unwrap();
        with_redo(&mut editor);
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .text_animator_value_command(1, parameter, default, 0)
                .unwrap()
                .is_none()
        );
        preserved(&mut editor, value(parameter, 0, default), false);
        animate(&mut editor, parameter, 10., 90.);
        assert_eq!(item(&editor).value_at(parameter, 10), Some(50.));
        assert_eq!(item(&editor).parameters[&parameter].value, 10.);
        with_redo(&mut editor);
        preserved(&mut editor, value(parameter, 10, 50.), false);
        assert_eq!(item(&editor).parameters[&parameter].keys.len(), 2);
    }
    editor
        .execute(Command::SetTextAnimatorSelector {
            id: 1,
            animator: 1,
            selector: TextSelector {
                units: TextSelectorUnits::Words,
                shape: TextSelectorShape::Triangle,
            },
        })
        .unwrap();
    editor.execute(Command::AddTextAnimator { id: 1 }).unwrap();
    editor
        .execute(edit(
            2,
            TextParam::AnimatorPositionX,
            TrackEdit::Value {
                frame: 0,
                value: -123.,
            },
        ))
        .unwrap();
    let samples = editor
        .selected_layer()
        .unwrap()
        .text_animators_at(10)
        .unwrap();
    assert_eq!(samples[0], TextAnimatorSample::default());
    assert_eq!(
        (
            samples[1].start,
            samples[1].end,
            samples[1].position,
            samples[1].scale,
            samples[1].rotation,
            samples[1].opacity,
            samples[1].amount
        ),
        (100., 100., [50., 50.], [50., 50.], 50., 50., 50.)
    );
    assert_eq!(
        (samples[1].units, samples[1].shape),
        (TextSelectorUnits::Words, TextSelectorShape::Triangle)
    );
    assert!(samples[1].selectors.is_empty());
    assert_eq!(samples[2].position, [-123., 0.]);
    editor
        .execute(Command::MoveTextAnimator {
            id: 1,
            animator: 2,
            index: 0,
        })
        .unwrap();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .text_animators_at(10)
            .unwrap(),
        vec![samples[0].clone(), samples[2].clone(), samples[1].clone()]
    );
    let mut actual = editor.selected_layer().unwrap().clone();
    actual.text_animators = untouched.text_animators.clone();
    actual.next_text_animator_id = untouched.next_text_animator_id;
    assert_eq!(actual, untouched);
}

#[test]
fn multiple_animators_limits_stale_controls_nontext_and_invalid_channels_are_atomic() {
    let mut editor = stacked();
    editor
        .execute(Command::Batch(vec![
            Command::AddTextAnimator { id: 1 },
            Command::AddTextAnimator { id: 1 },
        ]))
        .unwrap();
    with_redo(&mut editor);
    preserved(&mut editor, Command::AddTextAnimator { id: 1 }, true);
    for animator in [0, 4, u64::MAX] {
        for command in [
            Command::RemoveTextAnimator { id: 1, animator },
            Command::MoveTextAnimator {
                id: 1,
                animator,
                index: 0,
            },
            Command::SetTextAnimatorSelector {
                id: 1,
                animator,
                selector: TextSelector::default(),
            },
        ] {
            preserved(&mut editor, command, true);
        }
    }
    for index in [3, usize::MAX] {
        preserved(
            &mut editor,
            Command::MoveTextAnimator {
                id: 1,
                animator: 1,
                index,
            },
            true,
        );
    }
    for parameter in TextParam::ALL
        .into_iter()
        .filter(|parameter| !parameter.is_animator())
    {
        let layer = editor.selected_layer().unwrap();
        assert_eq!(item(&editor).value_at(parameter, 0), None);
        assert!(layer.track(path(1, parameter)).is_none());
        assert!(layer.track_value(path(1, parameter), 0).is_none());
        assert!(layer.track_label(path(1, parameter)).is_none());
        assert!(
            layer
                .text_animator_value_command(1, parameter, 1., 0)
                .is_err()
        );
        preserved(&mut editor, value(parameter, 0, 1.), true);
        preserved(
            &mut editor,
            edit(1, parameter, TrackEdit::ToggleKey { frame: 0 }),
            true,
        );
    }
    preserved(
        &mut editor,
        Command::MoveTextAnimator {
            id: 1,
            animator: 1,
            index: 0,
        },
        false,
    );
    preserved(
        &mut editor,
        Command::SetTextAnimatorSelector {
            id: 1,
            animator: 1,
            selector: TextSelector::default(),
        },
        false,
    );
    preserved(
        &mut editor,
        Command::Batch(vec![
            Command::RemoveTextAnimator { id: 1, animator: 1 },
            Command::AddTextAnimator { id: 1 },
            Command::RemoveTextAnimator {
                id: 1,
                animator: 999,
            },
        ]),
        true,
    );
    preserved(
        &mut editor,
        Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        },
        true,
    );
    editor.current.project.composition.layers[0].locked = true;
    for command in [
        Command::AddTextAnimator { id: 1 },
        Command::RemoveTextAnimator { id: 1, animator: 1 },
        Command::SetTextAnimatorSelector {
            id: 1,
            animator: 1,
            selector: TextSelector::default(),
        },
    ] {
        preserved(&mut editor, command, true);
    }
    let mut nontext = scene();
    nontext
        .execute(Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        })
        .unwrap();
    with_redo(&mut nontext);
    preserved(&mut nontext, Command::AddTextAnimator { id: 1 }, true);
    assert!(
        nontext
            .selected_layer()
            .unwrap()
            .text_animators_at(0)
            .is_none()
    );
    let mut overflow = stacked();
    overflow.current.project.composition.layers[0].next_text_animator_id = u64::MAX - 1;
    with_redo(&mut overflow);
    preserved(&mut overflow, Command::AddTextAnimator { id: 1 }, true);
}

#[test]
fn multiple_animators_curve_overshoot_clamps_only_sampling_and_disable_bakes() {
    for parameter in parameters() {
        let mut editor = stacked();
        let max = parameter.bounds().1;
        animate(&mut editor, parameter, max - 10., max - 10.);
        for (frame, incoming, slope) in [(0, false, 40.), (20, true, -40.)] {
            editor
                .execute(Command::SetTemporalHandle {
                    id: 1,
                    property: path(1, parameter),
                    frame,
                    incoming,
                    handle: TemporalHandle {
                        influence: 0.5,
                        slope,
                    },
                })
                .unwrap();
        }
        assert!(item(&editor).parameters[&parameter].value_at(10) > max);
        assert_eq!(item(&editor).value_at(parameter, 10), Some(max));
        with_redo(&mut editor);
        preserved(&mut editor, value(parameter, 10, max), false);
        editor
            .execute(edit(1, parameter, TrackEdit::ToggleAnimation { frame: 10 }))
            .unwrap();
        assert!(item(&editor).parameters[&parameter].keys.is_empty());
        assert_eq!(item(&editor).parameters[&parameter].value, max);
    }
}

#[test]
fn multiple_animators_schema63_roundtrips_active_and_inactive_source_and_view_bytes() {
    for inactive in [false, true] {
        let mut editor = stacked();
        for parameter in parameters() {
            animate(&mut editor, parameter, 10., 90.);
        }
        if inactive {
            editor.execute(Command::NewComposition).unwrap();
        }
        let project = editor.project();
        assert_eq!(project.version, 63);
        let json = project.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap().to_json().unwrap(), json);
        let view=br#" { "version":2, "pins":[{"version":3,"layer":1,"property":{"TextAnimator":{"animator":1,"parameter":"AnimatorAmount"}}}] } "#;
        let bytes = project_file::encode(project, Some(view)).unwrap();
        let decoded = project_file::decode(&bytes).unwrap();
        assert_eq!(decoded.project, *project);
        assert_eq!(decoded.view, Some(view.as_slice()));
        assert_eq!(
            project_file::encode(&decoded.project, decoded.view).unwrap(),
            bytes
        );
        for version in [3, 60, 61, 62, PROJECT_VERSION + 1] {
            let mut invalid = project.clone();
            invalid.version = version;
            assert!(invalid.validate().is_err());
            assert!(project_file::encode(&invalid, None).is_err());
            rejected_wire(serde_json::to_value(invalid).unwrap());
        }
    }
}

#[test]
fn multiple_animators_invalid_sources_reject_before_noop_removal_or_repair() {
    for inactive in [false, true] {
        for invalid in 0..12 {
            let mut editor = stacked();
            animate(&mut editor, TextParam::AnimatorAmount, 10., 90.);
            if inactive {
                editor.execute(Command::NewComposition).unwrap();
            }
            if invalid == 0 {
                editor.current.project.version = 62;
            }
            let layer = resident_layer(&mut editor.current.project, inactive);
            match invalid {
                0 => {}
                1 => layer.next_text_animator_id = 0,
                2 => layer.next_text_animator_id = u64::MAX,
                3 => layer.text_animators[0].id = 0,
                4 => layer.text_animators[0].id = 2,
                5 => layer.text_animators.push(layer.text_animators[0].clone()),
                6 => layer.text_animators = vec![TextAnimator::new(1); 4],
                7 => layer.text_animators[0]
                    .parameters
                    .insert(TextParam::FontSize, AnimatedProperty::new(20.))
                    .map(|_| ())
                    .unwrap_or(()),
                8 => {
                    layer.text_animators[0]
                        .parameters
                        .get_mut(&TextParam::AnimatorAmount)
                        .unwrap()
                        .value = -1.
                }
                9 => {
                    layer.text_animators[0]
                        .parameters
                        .get_mut(&TextParam::AnimatorAmount)
                        .unwrap()
                        .keys
                        .get_mut(&0)
                        .unwrap()
                        .temporal
                        .outgoing = Some(TemporalHandle {
                        influence: 0.,
                        slope: 1.,
                    })
                }
                10 => {
                    layer.text_animators[0]
                        .parameters
                        .get_mut(&TextParam::AnimatorAmount)
                        .unwrap()
                        .keys
                        .get_mut(&0)
                        .unwrap()
                        .value = f64::INFINITY
                }
                _ => layer.content = Content::Rectangle,
            }
            assert!(editor.project().validate().is_err(), "case {invalid}");
            assert!(project_file::encode(editor.project(), None).is_err());
            rejected_wire(serde_json::to_value(editor.project()).unwrap());
            with_redo(&mut editor);
            for command in [
                value(TextParam::AnimatorAmount, 0, 10.),
                Command::RemoveTextAnimator { id: 1, animator: 1 },
                Command::SetTextAnimatorSelector {
                    id: 1,
                    animator: 1,
                    selector: TextSelector::default(),
                },
                edit(
                    1,
                    TextParam::AnimatorAmount,
                    TrackEdit::ToggleAnimation { frame: 0 },
                ),
                Command::DeleteKeys(vec![KeyRef {
                    id: 1,
                    property: path(1, TextParam::AnimatorAmount),
                    frame: 0,
                }]),
            ] {
                preserved(&mut editor, command, true);
            }
        }
    }
    let base = serde_json::to_value(stacked().project()).unwrap();
    for extra in [
        json!({"nested_selectors":[]}),
        json!({"selector":{"units":"Bogus"}}),
        json!({"parameters":{"FontSize":{"value":20,"keys":{}}}}),
        json!({"parameters":null}),
    ] {
        let mut raw = base.clone();
        raw["composition"]["layers"][0]["text_animators"][0]
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        rejected_wire(raw);
    }
}

#[test]
fn multiple_animators_reorder_remove_undo_and_duplicate_preserve_stable_tracks() {
    let mut editor = stacked();
    animate(&mut editor, TextParam::AnimatorAmount, 10., 90.);
    editor.execute(Command::AddTextAnimator { id: 1 }).unwrap();
    editor
        .execute(edit(
            2,
            TextParam::AnimatorOffset,
            TrackEdit::ToggleKey { frame: 7 },
        ))
        .unwrap();
    let before = editor.current.clone();
    editor
        .execute(Command::MoveTextAnimator {
            id: 1,
            animator: 2,
            index: 0,
        })
        .unwrap();
    let layer = editor.selected_layer().unwrap();
    assert_eq!(layer.text_animators()[0].id, 2);
    assert_eq!(
        layer.track_value(path(1, TextParam::AnimatorAmount), 10),
        Some(50.)
    );
    assert_eq!(
        layer
            .track(path(2, TextParam::AnimatorOffset))
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
        .execute(Command::RemoveTextAnimator { id: 1, animator: 1 })
        .unwrap();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .track(path(1, TextParam::AnimatorAmount))
            .is_none()
    );
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .track_value(path(1, TextParam::AnimatorAmount), 10),
        None
    );
    with_redo(&mut editor);
    preserved(&mut editor, value(TextParam::AnimatorAmount, 10, 42.), true);
    // Restore real redo after the rejection-preservation check.
    editor.redo.clear();
    editor.undo();
    assert_eq!(editor.current, reordered);
    editor.execute(Command::DuplicateComposition).unwrap();
    let copy = &editor.project().composition.layers[0];
    assert_eq!(
        copy.text_animators(),
        reordered.project.composition.layers[0].text_animators()
    );
}

#[test]
fn multiple_animators_generic_key_temporal_and_history_commands_preserve_other_source() {
    let mut editor = stacked();
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
    animate(&mut editor, TextParam::AnimatorAmount, 20., 80.);
    let untouched = editor.selected_layer().unwrap().clone();
    let property = path(1, TextParam::AnimatorAmount);
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
            TextParam::AnimatorAmount,
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
    actual.text_animators = untouched.text_animators.clone();
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
fn multiple_animators_invalid_commands_and_stale_key_batches_are_atomic() {
    for parameter in parameters() {
        let mut editor = stacked();
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
                    .text_animator_value_command(1, parameter, bad, 0)
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
                .text_animator_value_command(1, parameter, 25., 0)
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
                    value_scale: parameter.bounds().1 / 90. + 1.,
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
fn multiple_animators_layer_shift_and_cross_fps_paste_retime_owned_tracks() {
    let mut editor = stacked();
    animate(&mut editor, TextParam::AnimatorOffset, -20., 20.);
    let property = path(1, TextParam::AnimatorOffset);
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

#[test]
fn multiple_animators_primary_secondary_selectors_stay_exact_and_duplicates_are_independent() {
    let mut editor = scene();
    for parameter in parameters() {
        editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleKey { frame: 0 },
            })
            .unwrap();
    }
    for _ in 0..MAX_TEXT_RANGE_SELECTORS {
        editor
            .execute(Command::AddTextRangeSelector { id: 1 })
            .unwrap();
    }
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::TextSelector {
                selector: 3,
                parameter: TextSelectorParam::Amount,
            },
            edit: TrackEdit::ToggleKey { frame: 7 },
        })
        .unwrap();
    let before = editor.selected_layer().unwrap().clone();
    let primary = before.text_animator_at(10).unwrap();
    editor
        .execute(Command::Batch(vec![
            Command::AddTextAnimator { id: 1 },
            Command::RenameLayer {
                id: 1,
                name: "Mixed route".into(),
            },
        ]))
        .unwrap();
    animate(&mut editor, TextParam::AnimatorPositionX, 10., 90.);
    let after = editor.selected_layer().unwrap().clone();
    assert_eq!(after.text_animators_at(10).unwrap()[0], primary);
    assert_eq!(after.text_parameters, before.text_parameters);
    assert_eq!(after.text_range_selectors(), before.text_range_selectors());
    assert_eq!(
        after.next_text_range_selector_id,
        before.next_text_range_selector_id
    );
    assert_eq!(after.text_selector(), before.text_selector());
    assert_eq!(after.content, before.content);
    for path in before.track_paths() {
        assert_eq!(after.track(path), before.track(path));
    }
    editor.execute(Command::DuplicateLayers(vec![1])).unwrap();
    let copy = editor.selected_layer().unwrap();
    assert_ne!(copy.id, after.id);
    assert_eq!(copy.text_animators(), after.text_animators());
    assert_eq!(copy.next_text_animator_id, after.next_text_animator_id);
    let copy_id = copy.id;
    editor
        .execute(Command::EditTrack {
            id: copy_id,
            property: path(1, TextParam::AnimatorPositionX),
            edit: TrackEdit::Value {
                frame: 20,
                value: 30.,
            },
        })
        .unwrap();
    assert_eq!(editor.project().composition.layer(1).unwrap(), &after);
    editor
        .execute(Command::RemoveTextAnimator {
            id: copy_id,
            animator: 1,
        })
        .unwrap();
    editor
        .execute(Command::AddTextAnimator { id: copy_id })
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().text_animators()[0].id, 2);
    assert_eq!(
        editor
            .project()
            .composition
            .layer(1)
            .unwrap()
            .text_animators()[0]
            .id,
        1
    );
}

#[test]
fn multiple_animators_paste_preserves_target_sparse_baseline_and_rejects_stale_ids() {
    let mut editor = stacked();
    animate(&mut editor, TextParam::AnimatorAmount, 10., 90.);
    let property = path(1, TextParam::AnimatorAmount);
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
    editor.execute(Command::AddTextAnimator { id: 2 }).unwrap();
    editor
        .execute(Command::PasteKeys {
            keys: vec![copied.clone()],
            frame: 40,
            target: Some(2),
        })
        .unwrap();
    let target = editor.project().composition.layer(2).unwrap();
    assert_eq!(target.track(property).unwrap().value, 100.);
    assert_eq!(target.track(property).unwrap().keys()[&40], copied.data);
    assert_eq!(target.track_value(property, 40), Some(90.));
    for parameter in parameters().filter(|p| *p != TextParam::AnimatorAmount) {
        assert!(
            !target.text_animators()[0]
                .parameters
                .contains_key(&parameter)
        );
    }
    editor
        .execute(Command::RemoveTextAnimator { id: 2, animator: 1 })
        .unwrap();
    editor.execute(Command::AddTextAnimator { id: 2 }).unwrap();
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
