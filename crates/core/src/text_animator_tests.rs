//! Independent source, history and portable storage coverage for the bounded animator.
use super::*;

const PARAMETERS: [TextParam; 5] = [
    TextParam::AnimatorStart,
    TextParam::AnimatorEnd,
    TextParam::AnimatorPositionX,
    TextParam::AnimatorPositionY,
    TextParam::AnimatorOpacity,
];
const DEFAULTS: [f64; 5] = [0., 100., 0., 0., 100.];
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a4WQAAAAASUVORK5CYII=";

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "A\u{0301} 한글\r\n\n👩‍👩‍👧‍👦\tAB".into(),
                font_size: 40.,
            },
            width: 300.,
            height: 180.,
            name: "Animator".into(),
        })
        .unwrap();
    editor
}
fn command(parameter: TextParam, edit: TrackEdit, generic: bool) -> Command {
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
    command(parameter, TrackEdit::Value { frame, value }, generic)
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
    future.project.composition.name = "Future composition".into();
    editor.redo = vec![future];
}
fn rejected(editor: &mut Editor, command: Command) {
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    assert!(editor.execute(command).is_err());
    assert_eq!(editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
}
fn unchanged(editor: &mut Editor, command: Command) {
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    let json = serde_json::to_vec(&current.project).unwrap();
    editor.execute(command).unwrap();
    assert_eq!(editor.current, current);
    assert_eq!(serde_json::to_vec(editor.project()).unwrap(), json);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
}
fn rich_scene() -> Editor {
    let mut editor = scene();
    editor
        .execute(Command::AddContent {
            content: Content::Image { png: PNG.into() },
            width: 1.,
            height: 1.,
            name: "Asset".into(),
        })
        .unwrap();
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
            text: "Different 👨‍👩‍👧\nsource".into(),
        })
        .unwrap();
    let layer = editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap();
    layer.text_style.paragraph = true;
    layer.text_style.paragraph_first_line_indent = -3.25;
    layer.text_style.stroke_enabled = true;
    layer.text_style.stroke_width = 7.5;
    layer.text_style.fill_enabled = false;
    // Every older scalar has distinct base and authored metadata. Animator edits
    // must preserve these exact objects, including dormant baseline and handles.
    for parameter in TextParam::ALL[..12].iter().copied() {
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
    editor.current.project.version = 55;
    editor.execute(Command::DuplicateComposition).unwrap();
    editor.activate_composition(1).unwrap();
    editor.select(1);
    editor.project().validate().unwrap();
    with_redo(&mut editor);
    editor
}

#[test]
fn text_animator_appends_addresses_defaults_and_identity_without_materializing_source() {
    assert_eq!(TextParam::ALL[12..17], PARAMETERS);
    let editor = scene();
    let before = serde_json::to_vec(editor.project()).unwrap();
    for (parameter, default) in PARAMETERS.into_iter().zip(DEFAULTS) {
        assert!(parameter.is_animator());
        assert!(!parameter.is_typography());
        assert_eq!(parameter.required_version(), 56);
        assert_eq!(TextPaint::from_parameter(parameter), None);
        let bounds = if matches!(
            parameter,
            TextParam::AnimatorPositionX | TextParam::AnimatorPositionY
        ) {
            (-1_000_000., 1_000_000.)
        } else {
            (0., 100.)
        };
        assert_eq!(parameter.bounds(), bounds);
        for frame in [0, 30, u32::MAX] {
            let layer = editor.selected_layer().unwrap();
            assert_eq!(layer.text_value_at(parameter, frame), Some(default));
            assert_eq!(
                layer.track_value(PropertyPath::Text(parameter), frame),
                Some(default)
            );
            assert!(layer.track(PropertyPath::Text(parameter)).is_none());
            assert!(!layer.track_paths().contains(&PropertyPath::Text(parameter)));
            assert!(
                layer
                    .text_value_command(parameter, default, frame)
                    .unwrap()
                    .is_none()
            );
        }
    }
    let neutral = TextAnimatorSample {
        start: 0.,
        end: 100.,
        position: [0., 0.],
        opacity: 100.,
        ..Default::default()
    };
    assert_eq!(
        editor.selected_layer().unwrap().text_animator_at(0),
        Some(neutral.clone())
    );
    assert!(neutral.is_identity());
    assert!(
        !TextAnimatorSample {
            position: [1., 0.],
            ..neutral.clone()
        }
        .is_identity()
    );
    assert!(
        !TextAnimatorSample {
            opacity: 99.,
            ..neutral.clone()
        }
        .is_identity()
    );
    for (start, end) in [(50., 50.), (80., 20.)] {
        assert!(
            TextAnimatorSample {
                start,
                end,
                position: [100., -50.],
                opacity: 0.,
                ..Default::default()
            }
            .is_identity()
        );
    }
    assert_eq!(serde_json::to_vec(editor.project()).unwrap(), before);
    assert_eq!(editor.project().version, 3);
}

#[test]
fn text_animator_finite_endpoints_and_old_omitted_storage_remain_exact() {
    for generic in [false, true] {
        for p in PARAMETERS {
            let mut editor = scene();
            for assigned in [p.bounds().0, p.bounds().1] {
                editor.execute(value(p, 0, assigned, generic)).unwrap();
                assert_eq!(
                    editor.selected_layer().unwrap().text_value_at(p, 0),
                    Some(assigned)
                );
                editor
                    .execute(command(p, TrackEdit::ToggleKey { frame: 10 }, generic))
                    .unwrap();
                assert_eq!(
                    editor.selected_layer().unwrap().text_parameters[&p].keys[&10].value,
                    assigned
                );
                editor
                    .execute(command(
                        p,
                        TrackEdit::ToggleAnimation { frame: 10 },
                        generic,
                    ))
                    .unwrap();
            }
        }
    }
    let mut project = scene().project().clone();
    let view = br#" {"version":2,"frame":10} "#;
    for version in [3, 7, 22, 48, 49, 52, 53, 54, 55, 56] {
        project.version = version;
        let json = project.to_json().unwrap();
        assert!(!json.contains("text_parameters"));
        assert_eq!(Project::from_json(&json).unwrap().to_json().unwrap(), json);
        let native = project_file::encode(&project, Some(view)).unwrap();
        let decoded = project_file::decode(&native).unwrap();
        assert_eq!(decoded.project, project);
        assert_eq!(
            project_file::encode(&decoded.project, decoded.view).unwrap(),
            native
        );
    }
}

#[test]
fn text_animator_every_static_field_preserves_exact_unrelated_source_assets_and_history() {
    for generic in [false, true] {
        for (parameter, assigned) in PARAMETERS
            .into_iter()
            .zip([20., 80., -321.25, 51.125, 36.5])
        {
            let mut editor = rich_scene();
            let before = editor.current.clone();
            let mut expected = before.clone();
            expected.project.version = 56;
            expected
                .project
                .composition
                .layers
                .iter_mut()
                .find(|layer| layer.id == 1)
                .unwrap()
                .text_parameters
                .insert(parameter, AnimatedProperty::new(assigned));
            let undo_count = editor.undo.len();
            editor
                .execute(command(
                    parameter,
                    TrackEdit::Value {
                        frame: 25,
                        value: assigned,
                    },
                    generic,
                ))
                .unwrap();
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
    }
}

#[test]
fn text_animator_all_track_edits_preserve_baselines_keys_and_metadata() {
    for generic in [false, true] {
        let mut editor = rich_scene();
        let p = TextParam::AnimatorPositionX;
        let mut expected = editor.current.clone();
        expected.project.version = 56;
        let mut expected_track = AnimatedProperty::new(0.);
        expected_track
            .keys
            .insert(10, key(0., Interpolation::Linear));
        let steps = [
            TrackEdit::ToggleAnimation { frame: 10 },
            TrackEdit::Value {
                frame: 40,
                value: 60.,
            },
            TrackEdit::Interpolate {
                frame: 10,
                interpolation: Interpolation::Hold,
            },
            TrackEdit::Keyframe {
                from: 40,
                to: 50,
                value: 80.,
            },
            TrackEdit::ToggleKey { frame: 30 },
            TrackEdit::ToggleKey { frame: 30 },
            TrackEdit::ToggleAnimation { frame: 50 },
        ];
        for (index, edit) in steps.into_iter().enumerate() {
            match index {
                0 => {}
                1 => {
                    expected_track
                        .keys
                        .insert(40, key(60., Interpolation::Linear));
                }
                2 => {
                    expected_track.keys.get_mut(&10).unwrap().interpolation = Interpolation::Hold;
                }
                3 => {
                    let mut moved = expected_track.keys.remove(&40).unwrap();
                    moved.value = 80.;
                    expected_track.keys.insert(50, moved);
                }
                4 => {
                    expected_track
                        .keys
                        .insert(30, key(0., Interpolation::Linear));
                }
                5 => {
                    expected_track.keys.remove(&30);
                }
                6 => {
                    expected_track.value = 80.;
                    expected_track.keys.clear();
                }
                _ => unreachable!(),
            }
            expected
                .project
                .composition
                .layers
                .iter_mut()
                .find(|layer| layer.id == 1)
                .unwrap()
                .text_parameters
                .insert(p, expected_track.clone());
            editor.execute(command(p, edit, generic)).unwrap();
            assert_eq!(editor.current, expected, "step {index}");
        }
        // Keyframe and Value preserve independently-authored dormant key handles.
        let mut track = AnimatedProperty::new(-123.);
        let mut authored = key(20., Interpolation::Bezier(Bezier::default()));
        authored.temporal.outgoing = Some(TemporalHandle {
            slope: 2.,
            influence: 0.4,
        });
        track.keys.insert(10, authored.clone());
        track.keys.insert(40, key(80., Interpolation::Hold));
        editor
            .current
            .project
            .composition
            .layers
            .iter_mut()
            .find(|layer| layer.id == 1)
            .unwrap()
            .text_parameters
            .insert(p, track);
        let before = editor.current.clone();
        let mut expected = before.clone();
        let track = expected
            .project
            .composition
            .layers
            .iter_mut()
            .find(|layer| layer.id == 1)
            .unwrap()
            .text_parameters
            .get_mut(&p)
            .unwrap();
        let mut moved = track.keys.remove(&10).unwrap();
        moved.value = 30.;
        track.keys.insert(15, moved);
        editor
            .execute(command(
                p,
                TrackEdit::Keyframe {
                    from: 10,
                    to: 15,
                    value: 30.,
                },
                generic,
            ))
            .unwrap();
        assert_eq!(editor.current, expected);
        expected
            .project
            .composition
            .layers
            .iter_mut()
            .find(|layer| layer.id == 1)
            .unwrap()
            .text_parameters
            .get_mut(&p)
            .unwrap()
            .keys
            .get_mut(&15)
            .unwrap()
            .value = 35.;
        editor.execute(value(p, 15, 35., generic)).unwrap();
        assert_eq!(editor.current, expected);
        assert_eq!(
            editor.selected_layer().unwrap().text_parameters[&p].value,
            -123.
        );
    }
}

#[test]
fn text_animator_exact_value_noops_preserve_dormant_entries_samples_redo_and_schema() {
    for generic in [false, true] {
        let mut editor = scene();
        with_redo(&mut editor);
        for (p, default) in PARAMETERS.into_iter().zip(DEFAULTS) {
            unchanged(&mut editor, value(p, 27, default, generic));
        }
        assert_eq!(editor.project().version, 3);
        let p = TextParam::AnimatorPositionX;
        editor.current.project.version = 56;
        let mut track = AnimatedProperty::new(-0.0);
        track.keys.insert(10, key(0., Interpolation::Linear));
        track.keys.insert(40, key(60., Interpolation::Hold));
        editor.current.project.composition.layers[0]
            .text_parameters
            .insert(p, track);
        with_redo(&mut editor);
        for (frame, sampled) in [(0, 0.), (10, 0.), (25, 30.), (40, 60.), (149, 60.)] {
            unchanged(&mut editor, value(p, frame, sampled, generic));
        }
        assert_eq!(
            editor.selected_layer().unwrap().text_parameters[&p]
                .value
                .to_bits(),
            (-0.0f64).to_bits()
        );
        unchanged(
            &mut editor,
            command(
                p,
                TrackEdit::Keyframe {
                    from: 40,
                    to: 40,
                    value: 60.,
                },
                generic,
            ),
        );
        unchanged(
            &mut editor,
            command(
                p,
                TrackEdit::Interpolate {
                    frame: 40,
                    interpolation: Interpolation::Hold,
                },
                generic,
            ),
        );
        unchanged(
            &mut editor,
            Command::Batch(vec![
                Command::Batch(vec![]),
                value(p, 40, 50., generic),
                Command::Batch(vec![value(p, 40, 60., generic)]),
            ]),
        );
    }
}

#[test]
fn text_animator_sampling_supports_hold_linear_bezier_temporal_and_bounds() {
    for (p, default) in PARAMETERS.into_iter().zip(DEFAULTS) {
        let mut editor = scene();
        editor
            .execute(command(p, TrackEdit::ToggleAnimation { frame: 10 }, false))
            .unwrap();
        let target = if default == 100. { 20. } else { 80. };
        editor.execute(value(p, 50, target, true)).unwrap();
        let layer = editor.selected_layer().unwrap();
        assert_eq!(layer.text_value_at(p, 0), Some(default));
        assert_eq!(layer.text_value_at(p, 30), Some((default + target) * 0.5));
        assert_eq!(layer.text_value_at(p, 70), Some(target));
        for interpolation in [
            Interpolation::Hold,
            Interpolation::Smooth,
            Interpolation::Bezier(Bezier::default()),
        ] {
            editor
                .execute(command(
                    p,
                    TrackEdit::Interpolate {
                        frame: 10,
                        interpolation,
                    },
                    true,
                ))
                .unwrap();
            let sample = editor
                .selected_layer()
                .unwrap()
                .text_value_at(p, 30)
                .unwrap();
            if interpolation == Interpolation::Hold {
                assert_eq!(sample, default);
            } else {
                assert!((sample - (default + target) * 0.5).abs() < 1e-8);
            }
        }
        let track = editor.current.project.composition.layers[0]
            .text_parameters
            .get_mut(&p)
            .unwrap();
        track.keys.get_mut(&10).unwrap().temporal.outgoing = Some(TemporalHandle {
            slope: 1e9,
            influence: 1.,
        });
        editor.project().validate().unwrap();
        assert_eq!(
            editor.selected_layer().unwrap().text_value_at(p, 30),
            Some(p.bounds().1)
        );
        editor
            .execute(command(p, TrackEdit::ToggleAnimation { frame: 30 }, false))
            .unwrap();
        assert_eq!(
            editor.selected_layer().unwrap().text_parameters[&p].value,
            p.bounds().1
        );
        assert!(
            editor.selected_layer().unwrap().text_parameters[&p]
                .keys
                .is_empty()
        );
    }
}

#[test]
fn text_animator_invalid_values_frames_targets_and_batches_are_atomic() {
    for p in PARAMETERS {
        for generic in [false, true] {
            let mut editor = scene();
            with_redo(&mut editor);
            let (min, max) = p.bounds();
            for invalid in [
                min - 0.1,
                max + 0.1,
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ] {
                rejected(&mut editor, value(p, 0, invalid, generic));
                rejected(
                    &mut editor,
                    command(
                        p,
                        TrackEdit::Keyframe {
                            from: 0,
                            to: 10,
                            value: invalid,
                        },
                        generic,
                    ),
                );
            }
            for edit in [
                TrackEdit::Value {
                    frame: 150,
                    value: min,
                },
                TrackEdit::ToggleAnimation { frame: 150 },
                TrackEdit::ToggleKey { frame: u32::MAX },
                TrackEdit::Keyframe {
                    from: 0,
                    to: 150,
                    value: min,
                },
                TrackEdit::Interpolate {
                    frame: 0,
                    interpolation: Interpolation::Hold,
                },
            ] {
                rejected(&mut editor, command(p, edit, generic));
            }
            rejected(
                &mut editor,
                Command::Batch(vec![
                    value(p, 0, 35., generic),
                    value(p, 0, max + 1., generic),
                ]),
            );
            editor.current.project.composition.layers[0].locked = true;
            rejected(&mut editor, value(p, 0, min, generic));
            editor.current.project.composition.layers[0].locked = false;
            editor.current.project.composition.layers[0].content = Content::Rectangle;
            assert_eq!(editor.selected_layer().unwrap().text_animator_at(0), None);
            rejected(&mut editor, value(p, 0, min, generic));
            rejected(
                &mut editor,
                Command::EditText {
                    id: 999,
                    parameter: p,
                    edit: TrackEdit::ToggleKey { frame: 0 },
                },
            );
        }
    }
}

#[test]
fn text_animator_source_validation_prevents_repairs_even_for_noops() {
    for generic in [false, true] {
        let mut editor = scene();
        editor
            .execute(command(
                TextParam::AnimatorStart,
                TrackEdit::ToggleKey { frame: 10 },
                false,
            ))
            .unwrap();
        with_redo(&mut editor);
        editor.current.project.version = 55;
        rejected(
            &mut editor,
            value(TextParam::AnimatorStart, 10, 0., generic),
        );
        rejected(
            &mut editor,
            command(
                TextParam::AnimatorStart,
                TrackEdit::ToggleKey { frame: 10 },
                generic,
            ),
        );
        editor.current.project.version = 56;
        editor.current.project.composition.layers[0]
            .text_parameters
            .get_mut(&TextParam::AnimatorStart)
            .unwrap()
            .keys
            .get_mut(&10)
            .unwrap()
            .temporal
            .incoming = Some(TemporalHandle {
            slope: 0.,
            influence: 0.,
        });
        rejected(
            &mut editor,
            command(
                TextParam::AnimatorStart,
                TrackEdit::ToggleAnimation { frame: 10 },
                generic,
            ),
        );
    }
}

#[test]
fn text_animator_legacy_media_noops_preserve_assets_and_materialization_rejects() {
    for generic in [false, true] {
        let mut editor = scene();
        editor
            .execute(Command::AddContent {
                content: Content::Image { png: PNG.into() },
                width: 1.,
                height: 1.,
                name: "Legacy image".into(),
            })
            .unwrap();
        editor.current.project.version = 7;
        editor.current.project.asset_library = AssetLibrary::default();
        editor
            .current
            .project
            .composition
            .layers
            .iter_mut()
            .for_each(|layer| layer.asset = None);
        editor.project().validate().unwrap();
        with_redo(&mut editor);
        for (p, default) in PARAMETERS.into_iter().zip(DEFAULTS) {
            unchanged(&mut editor, value(p, 20, default, generic));
            rejected(&mut editor, value(p, 20, 37., generic));
            rejected(
                &mut editor,
                command(p, TrackEdit::ToggleAnimation { frame: 20 }, generic),
            );
            rejected(
                &mut editor,
                command(p, TrackEdit::ToggleKey { frame: 20 }, generic),
            );
        }
        assert_eq!(editor.project().version, 7);
        assert!(editor.project().asset_library.is_default());
    }
}

// Read and rebuild LEP independently so unsupported schema tests exercise the
// actual native reader after CRC validation, not just its JSON helper.
fn chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    assert_eq!(&bytes[..16], b"\x89LEP\r\n\x1a\n\x01\x00\x20\x00\0\0\0\0");
    let mut cursor = 32;
    let mut result = Vec::new();
    while cursor < bytes.len() {
        assert_eq!(&bytes[cursor + 4..cursor + 8], &[1, 0, 0, 0]);
        let length =
            u64::from_le_bytes(bytes[cursor + 8..cursor + 16].try_into().unwrap()) as usize;
        result.push((
            bytes[cursor..cursor + 4].try_into().unwrap(),
            bytes[cursor + 20..cursor + 20 + length].to_vec(),
        ));
        cursor += 20 + length;
    }
    assert_eq!(cursor, bytes.len());
    result
}
fn pack(parts: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut bytes = b"\x89LEP\r\n\x1a\n\x01\x00\x20\x00".to_vec();
    bytes.resize(32, 0);
    for (tag, payload) in parts {
        let start = bytes.len();
        bytes.extend_from_slice(tag);
        bytes.extend_from_slice(&[1, 0, 0, 0]);
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        let mut crc = crc32fast::Hasher::new();
        crc.update(&bytes[start..]);
        crc.update(payload);
        bytes.extend_from_slice(&crc.finalize().to_le_bytes());
        bytes.extend_from_slice(payload);
    }
    let length = bytes.len() as u64;
    bytes[16..24].copy_from_slice(&length.to_le_bytes());
    bytes[24..28].copy_from_slice(&(parts.len() as u32).to_le_bytes());
    let crc = crc32fast::hash(&bytes[..28]);
    bytes[28..32].copy_from_slice(&crc.to_le_bytes());
    bytes
}

#[test]
fn text_animator_json_lep1_view1_view2_roundtrip_materialized_defaults_and_inactive_tracks() {
    for inactive in [false, true] {
        for view in [br#" { "version":1, "frame":30 } "#.as_slice(), br#" { "version":2, "pins":[{"version":1,"layer":1,"property":{"Text":"AnimatorOpacity"}}], "future":true } "#.as_slice()] {
            let mut editor = scene();
            for p in PARAMETERS {
                editor.execute(command(p, TrackEdit::ToggleKey { frame: 10 }, false)).unwrap();
                editor.execute(command(p, TrackEdit::ToggleKey { frame: 10 }, true)).unwrap();
            }
            if inactive { editor.execute(Command::NewComposition).unwrap(); }
            assert_eq!(editor.project().version, 56);
            let project = editor.project();
            let raw = serde_json::to_value(project).unwrap();
            let map = if inactive { &raw["other_compositions"]["1"]["layers"][0]["text_parameters"] } else { &raw["composition"]["layers"][0]["text_parameters"] };
            for ((p, default), name) in PARAMETERS.into_iter().zip(DEFAULTS).zip(["AnimatorStart", "AnimatorEnd", "AnimatorPositionX", "AnimatorPositionY", "AnimatorOpacity"]) {
                assert_eq!(serde_json::to_value(p).unwrap(), serde_json::json!(name));
                assert_eq!(map[name], serde_json::json!({"value":default,"keys":{}}));
            }
            let json = project.to_json().unwrap();
            assert_eq!(Project::from_json(&json).unwrap(), *project);
            let native = project_file::encode(project, Some(view)).unwrap();
            let parts = chunks(&native);
            assert_eq!(parts.iter().find(|(tag, _)| tag == b"VIEW").unwrap().1, view);
            let decoded = project_file::decode(&native).unwrap();
            assert_eq!(decoded.project, *project); assert_eq!(decoded.view, Some(view));
            assert_eq!(project_file::encode(&decoded.project, decoded.view).unwrap(), native);
            for version in [1, 3, 48, 49, 52, 53, 54, 55, PROJECT_VERSION + 1] {
                let mut bad = raw.clone(); bad["version"] = version.into();
                assert!(Project::from_json(&bad.to_string()).is_err(), "accepted schema {version}");
                let mut bad_parts = parts.clone();
                let metadata = &mut bad_parts.iter_mut().find(|(tag, _)| tag == b"PROJ").unwrap().1;
                let mut bad: serde_json::Value = serde_json::from_slice(metadata).unwrap(); bad["version"] = version.into();
                *metadata = serde_json::to_vec(&bad).unwrap();
                assert!(project_file::decode(&pack(&bad_parts)).is_err(), "native accepted schema {version}");
            }
        }
    }
}

#[test]
fn text_animator_invalid_storage_in_any_composition_and_default_on_nontext_rejects() {
    for inactive in [false, true] {
        let mut editor = scene();
        editor
            .execute(command(
                TextParam::AnimatorStart,
                TrackEdit::ToggleAnimation { frame: 10 },
                false,
            ))
            .unwrap();
        if inactive {
            editor.execute(Command::NewComposition).unwrap();
        }
        for p in PARAMETERS {
            let name = serde_json::to_value(p)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            for invalid in [
                serde_json::Value::Null,
                serde_json::json!("0"),
                serde_json::json!(p.bounds().0 - 1.),
                serde_json::json!(p.bounds().1 + 1.),
            ] {
                let mut raw = serde_json::to_value(editor.project()).unwrap();
                let layer = if inactive {
                    &mut raw["other_compositions"]["1"]["layers"][0]
                } else {
                    &mut raw["composition"]["layers"][0]
                };
                layer["text_parameters"][&name] = serde_json::json!({"value":invalid,"keys":{}});
                assert!(Project::from_json(&raw.to_string()).is_err());
                assert!(document::decode_native(raw, BTreeMap::new()).is_err());
            }
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
fn text_animator_key_clipboard_move_delete_scale_and_fps_layer_clipboard_preserve_tracks() {
    let mut editor = scene();
    for (p, default) in PARAMETERS.into_iter().zip(DEFAULTS) {
        editor
            .execute(command(p, TrackEdit::ToggleAnimation { frame: 10 }, false))
            .unwrap();
        editor
            .execute(value(p, 40, if default == 100. { 25. } else { 75. }, true))
            .unwrap();
        editor
            .execute(Command::SetTemporalHandle {
                id: 1,
                property: PropertyPath::Text(p),
                frame: 10,
                incoming: false,
                handle: TemporalHandle {
                    slope: 0.375,
                    influence: 0.3,
                },
            })
            .unwrap();
    }
    let original = editor.selected_layer().unwrap().text_parameters.clone();
    let copied: Vec<_> = PARAMETERS
        .into_iter()
        .map(|p| {
            editor
                .selected_layer()
                .unwrap()
                .copy_key(PropertyPath::Text(p), 10)
                .unwrap()
        })
        .collect();
    let refs: Vec<_> = PARAMETERS
        .into_iter()
        .map(|p| KeyRef {
            id: 1,
            property: PropertyPath::Text(p),
            frame: 40,
        })
        .collect();
    editor
        .execute(Command::MoveKeys {
            keys: refs,
            delta: 10,
        })
        .unwrap();
    for p in PARAMETERS {
        assert_eq!(
            editor.selected_layer().unwrap().text_parameters[&p].keys[&50],
            original[&p].keys[&40]
        );
    }
    editor
        .execute(Command::PasteKeys {
            keys: copied,
            frame: 70,
            target: Some(1),
        })
        .unwrap();
    for p in PARAMETERS {
        assert_eq!(
            editor.selected_layer().unwrap().text_parameters[&p].keys[&70],
            original[&p].keys[&10]
        );
    }
    editor
        .execute(Command::DeleteKeys(
            PARAMETERS
                .into_iter()
                .map(|p| KeyRef {
                    id: 1,
                    property: PropertyPath::Text(p),
                    frame: 70,
                })
                .collect(),
        ))
        .unwrap();
    editor
        .execute(Command::ScaleKeys {
            keys: PARAMETERS
                .into_iter()
                .map(|p| KeyRef {
                    id: 1,
                    property: PropertyPath::Text(p),
                    frame: 50,
                })
                .collect(),
            scale: KeyScale {
                time_origin: 0.,
                time_scale: 0.8,
                value_origin: 0.,
                value_scale: 1.,
            },
        })
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().text_parameters, original);
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
        let first = track.keys.remove(&10).unwrap();
        let last = track.keys.remove(&40).unwrap();
        let mut first = first;
        first.temporal.outgoing.as_mut().unwrap().slope *= 0.5;
        track.keys.insert(20, first);
        track.keys.insert(80, last);
    }
    assert_eq!(editor.selected_layer().unwrap().text_parameters, expected);
    editor
        .execute(Command::DuplicateLayer(editor.selected().unwrap()))
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().text_parameters, expected);
    assert_eq!(editor.project().version, 56);
    let native = project_file::encode(editor.project(), None).unwrap();
    assert_eq!(
        project_file::decode(&native).unwrap().project,
        *editor.project()
    );
}

#[test]
fn text_animator_ten_thousand_key_limit_checks_candidate_and_original() {
    for p in PARAMETERS {
        let mut editor = scene();
        editor.current.project.version = 56;
        editor.current.project.composition.duration = 20_000;
        let mut track = AnimatedProperty::new(50.);
        track.keys = (0..10_000)
            .map(|frame| (frame, key(50., Interpolation::Linear)))
            .collect();
        editor.current.project.composition.layers[0]
            .text_parameters
            .insert(p, track);
        editor.project().validate().unwrap();
        with_redo(&mut editor);
        rejected(
            &mut editor,
            command(p, TrackEdit::ToggleKey { frame: 10_000 }, false),
        );
        editor.execute(value(p, 50, 25., true)).unwrap();
        assert_eq!(
            editor.selected_layer().unwrap().text_parameters[&p]
                .keys
                .len(),
            10_000
        );
        editor.current.project.composition.layers[0]
            .text_parameters
            .get_mut(&p)
            .unwrap()
            .keys
            .insert(10_000, key(50., Interpolation::Linear));
        rejected(
            &mut editor,
            command(p, TrackEdit::ToggleKey { frame: 10_000 }, true),
        );
        rejected(
            &mut editor,
            command(p, TrackEdit::ToggleAnimation { frame: 10 }, true),
        );
    }
}

#[test]
fn text_animator_metadata_budget_checks_original_and_candidate() {
    const LIMIT: usize = 16 * 1024 * 1024;
    let mut editor = scene();
    let mut template = editor.project().composition.layers[0].clone();
    template.content = Content::Text {
        text: "\u{0001}".repeat(16_384),
        font_size: 24.,
    };
    editor.current.project.composition.layers = (1..=180)
        .map(|id| {
            let mut layer = template.clone();
            layer.id = id;
            layer
        })
        .collect();
    editor.current.project.next_layer_id = 181;
    let size = |project: &Project| serde_json::to_vec(project).unwrap().len();
    let mut remaining = (size(editor.project()) - LIMIT).div_ceil(5);
    for layer in &mut editor.current.project.composition.layers {
        let Content::Text { text, .. } = &mut layer.content else {
            unreachable!()
        };
        let count = remaining.min(text.len());
        *text = "x".repeat(count) + &"\u{0001}".repeat(text.len() - count);
        remaining -= count;
    }
    assert_eq!(remaining, 0);
    let padding = LIMIT - size(editor.project());
    editor
        .current
        .project
        .composition
        .name
        .push_str(&"x".repeat(padding));
    editor.project().validate().unwrap();
    document::validate_budget(editor.project()).unwrap();
    assert_eq!(size(editor.project()), LIMIT);
    with_redo(&mut editor);
    rejected(
        &mut editor,
        value(TextParam::AnimatorPositionX, 0, 1., false),
    );
    unchanged(
        &mut editor,
        value(TextParam::AnimatorPositionX, 0, 0., true),
    );
    editor.current.project.composition.name.push('x');
    rejected(
        &mut editor,
        value(TextParam::AnimatorPositionX, 0, 0., false),
    );
    rejected(
        &mut editor,
        command(
            TextParam::AnimatorPositionX,
            TrackEdit::ToggleKey { frame: 0 },
            true,
        ),
    );
}

fn animator_key_scene() -> Editor {
    let mut editor = rich_scene();
    editor.current.project.version = 56;
    let mut track = AnimatedProperty::new(-123.);
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
    track.keys.insert(40, key(80., Interpolation::Linear));
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap()
        .text_parameters
        .insert(TextParam::AnimatorPositionX, track);
    editor.project().validate().unwrap();
    with_redo(&mut editor);
    editor
}

fn animator_key_cases(editor: &Editor) -> Vec<(&'static str, Command, AnimatedProperty)> {
    let property = PropertyPath::Text(TextParam::AnimatorPositionX);
    let reference = KeyRef {
        id: 1,
        property,
        frame: 10,
    };
    let original = editor
        .project()
        .composition
        .layer(1)
        .unwrap()
        .text_parameters[&TextParam::AnimatorPositionX]
        .clone();
    let mut cases = Vec::new();
    let mut expected = original.clone();
    let first = expected.keys.remove(&10).unwrap();
    expected.keys.insert(15, first);
    cases.push((
        "move",
        Command::MoveKeys {
            keys: vec![reference],
            delta: 5,
        },
        expected,
    ));
    let mut expected = original.clone();
    expected.keys.remove(&10);
    cases.push(("delete", Command::DeleteKeys(vec![reference]), expected));
    let mut expected = original.clone();
    let mut first = expected.keys.remove(&10).unwrap();
    first.value = 40.;
    expected.keys.insert(20, first);
    cases.push((
        "scale",
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
        "velocity",
        Command::ScaleKeyVelocities {
            keys: vec![reference],
            scale: KeyVelocityScale {
                origin: 0.,
                factor: 2.,
            },
        },
        expected,
    ));
    let copy = editor
        .project()
        .composition
        .layer(1)
        .unwrap()
        .copy_key(property, 10)
        .unwrap();
    let mut expected = original.clone();
    expected.keys.insert(70, original.keys[&10].clone());
    cases.push((
        "paste",
        Command::PasteKeys {
            keys: vec![copy],
            frame: 70,
            target: Some(1),
        },
        expected,
    ));
    let handle = TemporalHandle {
        slope: 5.,
        influence: 0.25,
    };
    let mut expected = original.clone();
    expected.keys.get_mut(&10).unwrap().temporal.outgoing = Some(handle);
    cases.push((
        "handle",
        Command::SetTemporalHandle {
            id: 1,
            property,
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
        "mode",
        Command::SetTemporalMode {
            id: 1,
            property,
            frame: 10,
            mode: TemporalMode::Auto,
        },
        expected,
    ));
    cases
}

#[test]
fn text_animator_key_and_temporal_commands_preserve_literal_full_source_and_history() {
    let source = animator_key_scene();
    for (name, command, expected_track) in animator_key_cases(&source) {
        let mut editor = Editor {
            current: source.current.clone(),
            undo: source.undo.clone(),
            redo: source.redo.clone(),
            context_generation: source.context_generation,
        };
        let before = editor.current.clone();
        let mut expected = before.clone();
        expected
            .project
            .composition
            .layers
            .iter_mut()
            .find(|layer| layer.id == 1)
            .unwrap()
            .text_parameters
            .insert(TextParam::AnimatorPositionX, expected_track);
        let undo_count = editor.undo.len();
        assert!(
            text_animation::animator_edits_only(&command),
            "route {name}"
        );
        editor
            .execute(Command::Batch(vec![Command::Batch(vec![]), command]))
            .unwrap();
        assert_eq!(editor.current, expected, "{name}");
        assert_eq!(
            serde_json::to_vec(editor.project()).unwrap(),
            serde_json::to_vec(&expected.project).unwrap(),
            "{name}"
        );
        assert_eq!(editor.undo.len(), undo_count + 1, "{name}");
        assert!(!editor.can_redo(), "{name}");
        editor.undo();
        assert_eq!(editor.current, before, "{name}");
        editor.redo();
        assert_eq!(editor.current, expected, "{name}");
    }
    let mut editor = animator_key_scene();
    let property = PropertyPath::Text(TextParam::AnimatorPositionX);
    let reference = KeyRef {
        id: 1,
        property,
        frame: 10,
    };
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
            property,
            frame: 10,
            incoming: false,
            handle: TemporalHandle {
                slope: 2.,
                influence: 0.4,
            },
        },
        Command::SetTemporalMode {
            id: 1,
            property,
            frame: 10,
            mode: TemporalMode::Independent,
        },
    ] {
        unchanged(&mut editor, command);
    }
}

#[test]
fn text_animator_key_paste_into_legacy_media_rejects_without_migration() {
    let mut editor = scene();
    editor
        .execute(Command::AddContent {
            content: Content::Image { png: PNG.into() },
            width: 1.,
            height: 1.,
            name: "Legacy image".into(),
        })
        .unwrap();
    editor.current.project.version = 7;
    editor.current.project.asset_library = AssetLibrary::default();
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .for_each(|layer| layer.asset = None);
    // Exercise the actual old source load boundary, retaining its declared schema.
    let literal = serde_json::to_vec(editor.project()).unwrap();
    editor.current.project = serde_json::from_slice(&literal).unwrap();
    editor.project().validate().unwrap();
    document::validate_budget(editor.project()).unwrap();
    with_redo(&mut editor);
    for p in PARAMETERS {
        let mut data = key(37., Interpolation::Bezier(Bezier::default()));
        data.temporal.outgoing = Some(TemporalHandle {
            slope: 0.375,
            influence: 0.3,
        });
        let copy = KeyCopy {
            key: KeyRef {
                id: 1,
                property: PropertyPath::Text(p),
                frame: 10,
            },
            data,
            source_text: None,
            path_pose: None,
            effect_kind: None,
        };
        rejected(
            &mut editor,
            Command::PasteKeys {
                keys: vec![copy],
                frame: 40,
                target: Some(1),
            },
        );
        assert_eq!(serde_json::to_vec(editor.project()).unwrap(), literal);
    }
    assert!(editor.project().asset_library.is_default());
    assert_eq!(editor.project().version, 7);
}

#[test]
fn text_animator_key_commands_validate_original_schema_and_metadata_before_repairs() {
    let source = animator_key_scene();
    for (name, command, _) in animator_key_cases(&source) {
        let mut editor = Editor {
            current: source.current.clone(),
            undo: source.undo.clone(),
            redo: source.redo.clone(),
            context_generation: source.context_generation,
        };
        editor.current.project.version = 55;
        assert!(text_animation::animator_edits_only(&command), "{name}");
        rejected(&mut editor, command);
    }
    let mut oversized = animator_key_scene();
    let mut template = oversized.project().composition.layer(1).unwrap().clone();
    template.content = Content::Text {
        text: "\u{0001}".repeat(16_384),
        font_size: 40.,
    };
    // Keep all IDs disjoint from the inactive composition in the source fixture.
    oversized.current.project.composition.layers = (100..280)
        .map(|id| {
            let mut layer = template.clone();
            layer.id = id;
            layer
        })
        .collect();
    oversized.current.project.composition.layers[0].id = 1;
    oversized.current.project.next_layer_id = 280;
    oversized.project().validate().unwrap();
    assert!(document::validate_budget(oversized.project()).is_err());
    for (_, command, _) in animator_key_cases(&oversized) {
        rejected(&mut oversized, command);
    }
}

#[test]
fn text_animator_preservation_classifier_keeps_empty_and_mixed_legacy_routes() {
    let source = animator_key_scene();
    let animator = KeyRef {
        id: 1,
        property: PropertyPath::Text(TextParam::AnimatorPositionX),
        frame: 10,
    };
    let legacy = KeyRef {
        property: PropertyPath::Text(TextParam::FillOpacity),
        ..animator
    };
    for keys in [vec![], vec![animator, legacy]] {
        for command in [
            Command::MoveKeys {
                keys: keys.clone(),
                delta: 1,
            },
            Command::DeleteKeys(keys.clone()),
            Command::ScaleKeys {
                keys: keys.clone(),
                scale: KeyScale {
                    time_origin: 0.,
                    time_scale: 1.,
                    value_origin: 0.,
                    value_scale: 1.,
                },
            },
            Command::ScaleKeyVelocities {
                keys: keys.clone(),
                scale: KeyVelocityScale {
                    origin: 0.,
                    factor: 1.,
                },
            },
        ] {
            assert!(!text_animation::animator_edits_only(&command));
        }
    }
    let copies = [animator, legacy]
        .into_iter()
        .map(|reference| {
            source
                .selected_layer()
                .unwrap()
                .copy_key(reference.property, reference.frame)
                .unwrap()
        })
        .collect();
    for keys in [vec![], copies] {
        assert!(!text_animation::animator_edits_only(&Command::PasteKeys {
            keys,
            frame: 80,
            target: Some(1)
        }));
    }
    assert!(!text_animation::animator_edits_only(&Command::Batch(
        vec![]
    )));
    assert!(!text_animation::animator_edits_only(&Command::Batch(vec![
        value(TextParam::AnimatorPositionX, 0, 1., true),
        value(TextParam::FillOpacity, 0, 50., true),
    ])));
    assert!(!text_animation::animator_edits_only(&Command::ShiftLayer {
        id: 1,
        delta: 1
    }));
}
