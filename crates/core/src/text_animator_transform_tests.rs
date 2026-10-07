//! Bounded per-unit transforms preserve sparse source, history and selector tracks.
use super::*;

const PARAMETERS: [TextParam; 3] = [
    TextParam::AnimatorScaleX,
    TextParam::AnimatorScaleY,
    TextParam::AnimatorRotation,
];
const DEFAULTS: [f64; 3] = [100., 100., 0.];

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
            name: "Unit transforms".into(),
        })
        .unwrap();
    editor.clear_history();
    editor
}
fn edit(parameter: TextParam, edit: TrackEdit, generic: bool) -> Command {
    if generic {
        Command::EditTrack {
            id: 1,
            property: PropertyPath::Text(parameter),
            edit,
        }
    } else {
        Command::EditText {
            id: 1,
            parameter,
            edit,
        }
    }
}
fn value(parameter: TextParam, frame: Frame, value: f64, generic: bool) -> Command {
    edit(parameter, TrackEdit::Value { frame, value }, generic)
}
fn key(value: f64, interpolation: Interpolation) -> Keyframe {
    Keyframe {
        value,
        interpolation,
        temporal: TemporalHandles::default(),
    }
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
        serde_json::to_vec(editor.project()).unwrap(),
    );
    let result = editor.execute(command);
    assert_eq!(result.is_err(), rejects, "{result:?}");
    assert_eq!(editor.current, before.0);
    assert_eq!(editor.undo, before.1);
    assert_eq!(editor.redo, before.2);
    assert_eq!(editor.context_generation(), before.3);
    assert_eq!(serde_json::to_vec(editor.project()).unwrap(), before.4);
}
fn rich_scene() -> Editor {
    let mut editor = scene();
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 10 },
        })
        .unwrap();
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 40,
            text: "Changed 👨‍👩‍👧\nsource".into(),
        })
        .unwrap();
    let layer = &mut editor.current.project.composition.layers[0];
    layer.text_style.paragraph = true;
    layer.text_style.paragraph_first_line_indent = -3.25;
    layer.text_style.stroke_enabled = true;
    layer.text_style.stroke_width = 7.5;
    layer.text_selector = TextSelector {
        units: TextSelectorUnits::Words,
        shape: TextSelectorShape::Triangle,
    };
    for parameter in TextParam::ALL[..19].iter().copied() {
        let (min, max) = parameter.bounds();
        let mut track = AnimatedProperty::new(min + (max - min) * 0.25);
        let mut first = key(min + (max - min) * 0.5, Interpolation::Smooth);
        first.temporal.outgoing = Some(TemporalHandle {
            slope: 0.375,
            influence: 0.3,
        });
        track.keys.insert(10, first);
        track
            .keys
            .insert(40, key(min + (max - min) * 0.75, Interpolation::Hold));
        layer.text_parameters.insert(parameter, track);
    }
    editor.current.project.version = 59;
    editor.execute(Command::DuplicateComposition).unwrap();
    editor.activate_composition(1).unwrap();
    editor.select(1);
    editor.project().validate().unwrap();
    editor.clear_history();
    with_redo(&mut editor);
    editor
}
fn changed_track(
    editor: &mut Editor,
    parameter: TextParam,
    command: Command,
    expected_track: AnimatedProperty,
) {
    let before = editor.current.clone();
    let mut expected = before.clone();
    expected.project.version = 60;
    expected.project.composition.layers[0]
        .text_parameters
        .insert(parameter, expected_track);
    let undo_count = editor.undo.len();
    assert!(text_animation::animator_edits_only(&command));
    editor.execute(command).unwrap();
    assert_eq!(editor.current, expected);
    assert_eq!(
        serde_json::to_vec(editor.project()).unwrap(),
        serde_json::to_vec(&expected.project).unwrap()
    );
    assert_eq!(editor.undo.len(), undo_count + 1);
    assert!(!editor.can_redo());
    editor.undo();
    assert_eq!(editor.current, before);
    editor.redo();
    assert_eq!(editor.current, expected);
}

#[test]
fn text_animator_transforms_append_sparse_defaults_and_leave_legacy_versions_exact() {
    assert_eq!(TextParam::ALL[19..], PARAMETERS);
    for (parameter, default, name, bounds) in [
        (PARAMETERS[0], 100., "AnimatorScaleX", (0., 1000.)),
        (PARAMETERS[1], 100., "AnimatorScaleY", (0., 1000.)),
        (PARAMETERS[2], 0., "AnimatorRotation", (-3600., 3600.)),
    ] {
        assert_eq!(parameter.bounds(), bounds);
        assert_eq!(parameter.required_version(), 60);
        assert!(parameter.is_animator());
        assert!(!parameter.is_typography());
        assert_eq!(TextPaint::from_parameter(parameter), None);
        let serialized = serde_json::to_string(&parameter).unwrap();
        assert_eq!(serialized, format!("\"{name}\""));
        assert_eq!(
            serde_json::from_str::<TextParam>(&serialized).unwrap(),
            parameter
        );
        for version in [3, 48, 49, 52, 53, 54, 55, 56, 57, 58, 59, 60] {
            let mut editor = scene();
            editor.current.project.version = version;
            with_redo(&mut editor);
            let before = serde_json::to_vec(editor.project()).unwrap();
            let path = PropertyPath::Text(parameter);
            for frame in [0, 30, u32::MAX] {
                let layer = editor.selected_layer().unwrap();
                assert_eq!(layer.text_value_at(parameter, frame), Some(default));
                assert_eq!(layer.track_value(path, frame), Some(default));
                assert_eq!(layer.track_label(path), Some(parameter.label().into()));
                assert!(layer.track(path).is_none());
                assert!(!layer.track_paths().contains(&path));
                assert!(
                    layer
                        .text_value_command(parameter, default, frame)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(
                    layer.text_animator_at(frame),
                    Some(TextAnimatorSample::default())
                );
            }
            for generic in [false, true] {
                preserved(&mut editor, value(parameter, 17, default, generic), false);
            }
            assert_eq!(serde_json::to_vec(editor.project()).unwrap(), before);
            let json = editor.project().to_json().unwrap();
            assert!(!json.contains(name));
            assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
        }
    }
    for (parameter, value, version) in [
        (TextParam::FillRed, 17., 48),
        (TextParam::FillOpacity, 25., 52),
        (TextParam::AnimatorPositionX, 12., 56),
        (TextParam::AnimatorOffset, 20., 58),
        (TextParam::AnimatorAmount, 50., 59),
    ] {
        let mut editor = scene();
        editor
            .execute(self::value(parameter, 0, value, false))
            .unwrap();
        assert_eq!(editor.project().version, version);
    }
}

#[test]
fn text_animator_transforms_identity_includes_scale_and_rotation() {
    let neutral = TextAnimatorSample::default();
    assert_eq!(neutral.scale, [100., 100.]);
    assert_eq!(neutral.rotation, 0.);
    assert!(neutral.is_identity());
    for sample in [
        TextAnimatorSample {
            scale: [0., 100.],
            ..neutral.clone()
        },
        TextAnimatorSample {
            scale: [100., 0.],
            ..neutral.clone()
        },
        TextAnimatorSample {
            scale: [125., 75.],
            ..neutral.clone()
        },
        TextAnimatorSample {
            rotation: -3600.,
            ..neutral.clone()
        },
        TextAnimatorSample {
            rotation: 360.,
            ..neutral.clone()
        },
        TextAnimatorSample {
            rotation: 0.001,
            ..neutral.clone()
        },
    ] {
        assert!(!sample.is_identity());
        assert!(
            TextAnimatorSample {
                amount: 0.,
                ..sample.clone()
            }
            .is_identity()
        );
        assert!(
            TextAnimatorSample {
                start: 50.,
                end: 50.,
                ..sample.clone()
            }
            .is_identity()
        );
        assert!(
            TextAnimatorSample {
                start: 80.,
                end: 20.,
                ..sample.clone()
            }
            .is_identity()
        );
    }
}

#[test]
fn text_animator_transforms_scalar_edits_and_animation_preserve_full_source_and_history() {
    for (parameter, default) in PARAMETERS.into_iter().zip(DEFAULTS) {
        for generic in [false, true] {
            let mut editor = rich_scene();
            changed_track(
                &mut editor,
                parameter,
                value(parameter, 17, 37.12345678912345, generic),
                AnimatedProperty::new(37.12345678912345),
            );
            changed_track(
                &mut editor,
                parameter,
                value(parameter, 17, default, generic),
                AnimatedProperty::new(default),
            );
            let mut keyed = AnimatedProperty::new(default);
            keyed.keys.insert(10, key(default, Interpolation::Linear));
            changed_track(
                &mut editor,
                parameter,
                edit(parameter, TrackEdit::ToggleAnimation { frame: 10 }, generic),
                keyed.clone(),
            );
            keyed.keys.insert(40, key(80., Interpolation::Linear));
            changed_track(
                &mut editor,
                parameter,
                value(parameter, 40, 80., generic),
                keyed,
            );
            let sample = editor
                .selected_layer()
                .unwrap()
                .text_value_at(parameter, 25)
                .unwrap();
            with_redo(&mut editor);
            preserved(&mut editor, value(parameter, 25, sample, generic), false);
            changed_track(
                &mut editor,
                parameter,
                edit(parameter, TrackEdit::ToggleAnimation { frame: 25 }, generic),
                AnimatedProperty::new(sample),
            );
            assert_eq!(
                editor.selected_layer().unwrap().source_text_at(40),
                Some("Changed 👨‍👩‍👧\nsource")
            );
        }
    }
}

#[test]
fn text_animator_transforms_sample_independently_with_source_text_and_selector_animation() {
    let mut editor = rich_scene();
    for (parameter, first, last) in [
        (PARAMETERS[0], 0., 1000.),
        (PARAMETERS[1], 200., 0.),
        (PARAMETERS[2], -3600., 3600.),
    ] {
        editor.execute(value(parameter, 0, first, false)).unwrap();
        editor
            .execute(edit(
                parameter,
                TrackEdit::ToggleAnimation { frame: 0 },
                false,
            ))
            .unwrap();
        editor.execute(value(parameter, 60, last, true)).unwrap();
    }
    let before = editor.current.clone();
    for (frame, scale, rotation) in [
        (0, [0., 200.], -3600.),
        (15, [250., 150.], -1800.),
        (30, [500., 100.], 0.),
        (60, [1000., 0.], 3600.),
        (u32::MAX, [1000., 0.], 3600.),
    ] {
        let layer = editor.selected_layer().unwrap();
        let sample = layer.text_animator_at(frame).unwrap();
        assert_eq!(sample.scale, scale);
        assert_eq!(sample.rotation, rotation);
        assert_eq!(sample.units, TextSelectorUnits::Words);
        assert_eq!(sample.shape, TextSelectorShape::Triangle);
        assert_eq!(
            sample.amount,
            layer
                .text_value_at(TextParam::AnimatorAmount, frame)
                .unwrap()
        );
        assert_eq!(
            sample.position,
            [
                layer
                    .text_value_at(TextParam::AnimatorPositionX, frame)
                    .unwrap(),
                layer
                    .text_value_at(TextParam::AnimatorPositionY, frame)
                    .unwrap()
            ]
        );
        let offset = layer
            .text_value_at(TextParam::AnimatorOffset, frame)
            .unwrap();
        assert_eq!(
            sample.start,
            (layer
                .text_value_at(TextParam::AnimatorStart, frame)
                .unwrap()
                + offset)
                .clamp(0., 100.)
        );
        assert_eq!(
            sample.end,
            (layer.text_value_at(TextParam::AnimatorEnd, frame).unwrap() + offset).clamp(0., 100.)
        );
    }
    assert_eq!(editor.current, before);
}

#[test]
fn text_animator_transforms_dormant_and_keyed_storage_require_schema60_in_every_composition() {
    for parameter in PARAMETERS {
        for inactive in [false, true] {
            for keyed in [false, true] {
                let mut editor = scene();
                editor
                    .execute(edit(parameter, TrackEdit::ToggleKey { frame: 7 }, false))
                    .unwrap();
                if !keyed {
                    editor
                        .execute(edit(parameter, TrackEdit::ToggleKey { frame: 7 }, true))
                        .unwrap();
                }
                if inactive {
                    editor.execute(Command::NewComposition).unwrap();
                }
                let project = editor.project();
                assert_eq!(project.version, 60);
                assert_eq!(text_animation::animator_required_version(project), Some(60));
                assert_eq!(
                    Project::from_json(&project.to_json().unwrap()).unwrap(),
                    *project
                );
                let view = format!(
                    " {{ \"version\":2, \"pins\":[{{\"version\":1,\"layer\":1,\"property\":{{\"Text\":{}}}}}], \"future\":true }} ",
                    serde_json::to_string(&parameter).unwrap()
                );
                let native = project_file::encode(project, Some(view.as_bytes())).unwrap();
                let decoded = project_file::decode(&native).unwrap();
                assert_eq!(decoded.project, *project);
                assert_eq!(decoded.view, Some(view.as_bytes()));
                assert_eq!(
                    project_file::encode(&decoded.project, decoded.view).unwrap(),
                    native
                );
                for version in [3, 48, 55, 56, 57, 58, 59, PROJECT_VERSION + 1] {
                    let mut bad = project.clone();
                    bad.version = version;
                    assert!(
                        bad.validate().is_err(),
                        "accepted {parameter:?} in schema {version}"
                    );
                    let raw = serde_json::to_value(&bad).unwrap();
                    assert!(Project::from_json(&raw.to_string()).is_err());
                    assert!(document::decode_native(raw, BTreeMap::new()).is_err());
                    assert!(project_file::encode(&bad, None).is_err());
                }
                for bad_value in [
                    parameter.bounds().0 - 0.001,
                    parameter.bounds().1 + 0.001,
                    f64::NAN,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                ] {
                    let mut bad = project.clone();
                    let layer = if inactive {
                        &mut bad.other_compositions.get_mut(&1).unwrap().layers[0]
                    } else {
                        &mut bad.composition.layers[0]
                    };
                    let track = layer.text_parameters.get_mut(&parameter).unwrap();
                    if keyed {
                        track.keys.get_mut(&7).unwrap().value = bad_value;
                    } else {
                        track.value = bad_value;
                    }
                    assert!(bad.validate().is_err());
                    assert!(project_file::encode(&bad, None).is_err());
                }
                let mut bad = project.clone();
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
}

#[test]
fn text_animator_transforms_reject_invalid_commands_atomically_and_cannot_repair_sources() {
    for (parameter, default) in PARAMETERS.into_iter().zip(DEFAULTS) {
        for generic in [false, true] {
            let mut editor = scene();
            with_redo(&mut editor);
            for bad in [
                parameter.bounds().0 - 0.001,
                parameter.bounds().1 + 0.001,
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ] {
                assert!(
                    editor
                        .selected_layer()
                        .unwrap()
                        .text_value_command(parameter, bad, 0)
                        .is_err()
                );
                preserved(&mut editor, value(parameter, 0, bad, generic), true);
            }
            for frame in [editor.project().composition.duration, u32::MAX] {
                preserved(&mut editor, value(parameter, frame, default, generic), true);
                preserved(
                    &mut editor,
                    edit(parameter, TrackEdit::ToggleKey { frame }, generic),
                    true,
                );
            }
            editor.current.project.composition.layers[0].locked = true;
            for command in [
                value(parameter, 0, default, generic),
                value(parameter, 0, 25., generic),
                edit(parameter, TrackEdit::ToggleAnimation { frame: 0 }, generic),
            ] {
                preserved(&mut editor, command, true);
            }
            editor.current.project.composition.layers[0].locked = false;
            preserved(
                &mut editor,
                Command::Batch(vec![
                    value(parameter, 0, 25., generic),
                    value(parameter, 1, parameter.bounds().1 + 1., generic),
                ]),
                true,
            );
            editor.current.project.composition.layers[0].content = Content::Rectangle;
            assert_eq!(editor.selected_layer().unwrap().text_animator_at(0), None);
            preserved(&mut editor, value(parameter, 0, 25., generic), true);
            let mut editor = scene();
            editor
                .execute(edit(parameter, TrackEdit::ToggleKey { frame: 0 }, generic))
                .unwrap();
            editor.current.project.version = 59;
            with_redo(&mut editor);
            preserved(&mut editor, value(parameter, 0, default, generic), true);
            preserved(
                &mut editor,
                edit(parameter, TrackEdit::ToggleAnimation { frame: 0 }, generic),
                true,
            );
            editor.current.project.version = 60;
            editor.current.project.composition.layers[0]
                .text_parameters
                .get_mut(&parameter)
                .unwrap()
                .value = parameter.bounds().1 + 1.;
            preserved(&mut editor, value(parameter, 0, default, generic), true);
            preserved(
                &mut editor,
                edit(parameter, TrackEdit::ToggleAnimation { frame: 0 }, generic),
                true,
            );
        }
    }
}

fn keyed_scene(parameter: TextParam) -> Editor {
    let mut editor = rich_scene();
    let mut track = AnimatedProperty::new(73.125);
    let mut first = key(20., Interpolation::Bezier(Bezier::default()));
    first.temporal = TemporalHandles {
        incoming: Some(TemporalHandle {
            slope: -3.,
            influence: 0.2,
        }),
        outgoing: Some(TemporalHandle {
            slope: 2.,
            influence: 0.4,
        }),
        ..Default::default()
    };
    track.keys.insert(10, first);
    track.keys.insert(40, key(80., Interpolation::Hold));
    editor.current.project.composition.layers[0]
        .text_parameters
        .insert(parameter, track);
    editor.current.project.version = 60;
    editor.project().validate().unwrap();
    with_redo(&mut editor);
    editor
}

#[test]
fn text_animator_transforms_key_and_temporal_commands_preserve_full_source_and_history() {
    for parameter in PARAMETERS {
        let source = keyed_scene(parameter);
        let path = PropertyPath::Text(parameter);
        let reference = KeyRef {
            id: 1,
            property: path,
            frame: 10,
        };
        let original = source.selected_layer().unwrap().text_parameters[&parameter].clone();
        let mut cases = Vec::new();
        let mut expected = original.clone();
        let first = expected.keys.remove(&10).unwrap();
        expected.keys.insert(15, first);
        cases.push((
            Command::MoveKeys {
                keys: vec![reference],
                delta: 5,
            },
            expected,
        ));
        let mut expected = original.clone();
        expected.keys.remove(&10);
        cases.push((Command::DeleteKeys(vec![reference]), expected));
        let mut expected = original.clone();
        let mut first = expected.keys.remove(&10).unwrap();
        first.value = 40.;
        expected.keys.insert(20, first);
        cases.push((
            Command::ScaleKeys {
                keys: vec![reference],
                scale: KeyScale {
                    time_origin: 0.,
                    time_scale: 2.,
                    value_origin: 0.,
                    value_scale: 2.,
                },
            },
            expected,
        ));
        let mut expected = original.clone();
        expected
            .keys
            .get_mut(&10)
            .unwrap()
            .temporal
            .outgoing
            .as_mut()
            .unwrap()
            .slope = 4.;
        cases.push((
            Command::ScaleKeyVelocities {
                keys: vec![reference],
                scale: KeyVelocityScale {
                    origin: 0.,
                    factor: 2.,
                },
            },
            expected,
        ));
        let mut expected = original.clone();
        expected.keys.insert(70, original.keys[&10].clone());
        let copy = source.selected_layer().unwrap().copy_key(path, 10).unwrap();
        cases.push((
            Command::PasteKeys {
                keys: vec![copy],
                frame: 70,
                target: Some(1),
            },
            expected,
        ));
        let mut expected = original.clone();
        let handle = TemporalHandle {
            slope: 5.,
            influence: 0.25,
        };
        expected.keys.get_mut(&10).unwrap().temporal.outgoing = Some(handle);
        cases.push((
            Command::SetTemporalHandle {
                id: 1,
                property: path,
                frame: 10,
                incoming: false,
                handle,
            },
            expected,
        ));
        let mut expected = original.clone();
        expected.keys.get_mut(&10).unwrap().temporal = TemporalHandles {
            mode: TemporalMode::Auto,
            ..Default::default()
        };
        cases.push((
            Command::SetTemporalMode {
                id: 1,
                property: path,
                frame: 10,
                mode: TemporalMode::Auto,
            },
            expected,
        ));
        let mut expected = original.clone();
        expected.keys.get_mut(&10).unwrap().interpolation = Interpolation::Hold;
        expected.keys.get_mut(&10).unwrap().temporal.outgoing = None;
        cases.push((
            edit(
                parameter,
                TrackEdit::Interpolate {
                    frame: 10,
                    interpolation: Interpolation::Hold,
                },
                false,
            ),
            expected,
        ));
        let mut expected = original.clone();
        let mut first = expected.keys.remove(&10).unwrap();
        first.value = 25.;
        expected.keys.insert(15, first);
        cases.push((
            edit(
                parameter,
                TrackEdit::Keyframe {
                    from: 10,
                    to: 15,
                    value: 25.,
                },
                true,
            ),
            expected,
        ));
        for (command, expected) in cases {
            let mut editor = keyed_scene(parameter);
            changed_track(
                &mut editor,
                parameter,
                Command::Batch(vec![Command::Batch(vec![]), command]),
                expected,
            );
        }
        let mut editor = keyed_scene(parameter);
        for command in [
            Command::MoveKeys {
                keys: vec![reference],
                delta: 0,
            },
            Command::ScaleKeys {
                keys: vec![reference],
                scale: KeyScale {
                    time_origin: 0.,
                    time_scale: 1.,
                    value_origin: 0.,
                    value_scale: 1.,
                },
            },
            Command::ScaleKeyVelocities {
                keys: vec![reference],
                scale: KeyVelocityScale {
                    origin: 0.,
                    factor: 1.,
                },
            },
            Command::SetTemporalHandle {
                id: 1,
                property: path,
                frame: 10,
                incoming: false,
                handle: TemporalHandle {
                    slope: 2.,
                    influence: 0.4,
                },
            },
            Command::SetTemporalMode {
                id: 1,
                property: path,
                frame: 10,
                mode: TemporalMode::Independent,
            },
        ] {
            preserved(&mut editor, command, false);
        }
        for command in [
            edit(
                parameter,
                TrackEdit::Keyframe {
                    from: 10,
                    to: 40,
                    value: 25.,
                },
                true,
            ),
            edit(
                parameter,
                TrackEdit::Keyframe {
                    from: 10,
                    to: 20,
                    value: parameter.bounds().0 - 1.,
                },
                false,
            ),
            Command::MoveKeys {
                keys: vec![reference],
                delta: 30,
            },
            Command::ScaleKeys {
                keys: vec![reference],
                scale: KeyScale {
                    time_origin: 0.,
                    time_scale: 1.,
                    value_origin: 0.,
                    value_scale: 1000.,
                },
            },
            Command::SetTemporalHandle {
                id: 1,
                property: path,
                frame: 10,
                incoming: false,
                handle: TemporalHandle {
                    slope: 2.,
                    influence: 0.,
                },
            },
        ] {
            preserved(&mut editor, command, true);
        }
    }
}

#[test]
fn text_animator_transforms_sampling_clamps_curve_overshoot_before_baking() {
    for parameter in PARAMETERS {
        for high in [false, true] {
            let mut editor = keyed_scene(parameter);
            let bound = if high {
                parameter.bounds().1
            } else {
                parameter.bounds().0
            };
            let mut track = AnimatedProperty::new(73.125);
            let mut first = key(bound, Interpolation::Linear);
            first.temporal.outgoing = Some(TemporalHandle {
                slope: if high { 1_000_000. } else { -1_000_000. },
                influence: 0.8,
            });
            track.keys.insert(0, first);
            track.keys.insert(60, key(bound, Interpolation::Linear));
            assert!(if high {
                track.value_at(30) > bound
            } else {
                track.value_at(30) < bound
            });
            editor.current.project.composition.layers[0]
                .text_parameters
                .insert(parameter, track.clone());
            editor.project().validate().unwrap();
            let path = PropertyPath::Text(parameter);
            assert_eq!(
                editor
                    .selected_layer()
                    .unwrap()
                    .text_value_at(parameter, 30),
                Some(bound)
            );
            assert_eq!(
                editor.selected_layer().unwrap().track_value(path, 30),
                Some(bound)
            );
            let before = editor.current.clone();
            with_redo(&mut editor);
            preserved(&mut editor, value(parameter, 30, bound, false), false);
            changed_track(
                &mut editor,
                parameter,
                edit(parameter, TrackEdit::ToggleAnimation { frame: 30 }, true),
                AnimatedProperty::new(bound),
            );
            editor.undo();
            assert_eq!(editor.current, before);
            let mut expected = track;
            expected.keys.insert(30, key(bound, Interpolation::Linear));
            changed_track(
                &mut editor,
                parameter,
                edit(parameter, TrackEdit::ToggleKey { frame: 30 }, false),
                expected,
            );
        }
    }
}

#[test]
fn text_animator_transforms_copy_layers_rescales_keys_and_preserves_dormant_values() {
    let mut editor = scene();
    for parameter in PARAMETERS {
        let mut track = AnimatedProperty::new(73.125);
        let mut first = key(20., Interpolation::Bezier(Bezier::default()));
        first.temporal.outgoing = Some(TemporalHandle {
            slope: 2.,
            influence: 0.4,
        });
        track.keys.insert(10, first);
        track.keys.insert(40, key(80., Interpolation::Hold));
        editor.current.project.composition.layers[0]
            .text_parameters
            .insert(parameter, track);
    }
    editor.current.project.version = 60;
    editor.project().validate().unwrap();
    let original = editor.selected_layer().unwrap().text_parameters.clone();
    let clipboard = editor.copy_layers(&[1]).unwrap();
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
    let mut expected = original.clone();
    for track in expected.values_mut() {
        let mut first = track.keys.remove(&10).unwrap();
        let last = track.keys.remove(&40).unwrap();
        first.temporal.outgoing.as_mut().unwrap().slope = 1.;
        track.keys.insert(20, first);
        track.keys.insert(80, last);
    }
    assert_eq!(editor.selected_layer().unwrap().text_parameters, expected);
    editor
        .execute(Command::DuplicateLayer(editor.selected().unwrap()))
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().text_parameters, expected);
    assert_eq!(
        editor.project().other_compositions[&1].layers[0].text_parameters,
        original
    );
    assert_eq!(editor.project().version, 60);
    let native = project_file::encode(editor.project(), None).unwrap();
    assert_eq!(
        project_file::decode(&native).unwrap().project,
        *editor.project()
    );
}
