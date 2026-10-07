//! Bounded percentage-point translation of the existing hard text range.
use super::*;

const OFFSET: TextParam = TextParam::AnimatorOffset;
const PATH: PropertyPath = PropertyPath::Text(OFFSET);

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "A e\u{301}\r\n한 👩‍💻".into(),
                font_size: 40.,
            },
            width: 300.,
            height: 180.,
            name: "Offset".into(),
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
fn set(editor: &mut Editor, parameter: TextParam, frame: Frame, value: f64) {
    editor
        .execute(edit(parameter, TrackEdit::Value { frame, value }, false))
        .unwrap();
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
    );
    let result = editor.execute(command);
    assert_eq!(result.is_err(), rejects, "{result:?}");
    assert_eq!(editor.current, before.0);
    assert_eq!(editor.undo, before.1);
    assert_eq!(editor.redo, before.2);
}

#[test]
fn text_animator_offset_is_appended_sparse_neutral_and_keeps_legacy_source_exact() {
    assert_eq!(TextParam::ALL[17], OFFSET);
    assert_eq!(OFFSET.bounds(), (-100., 100.));
    assert_eq!(OFFSET.required_version(), 58);
    assert!(OFFSET.is_animator());
    assert_eq!(
        serde_json::to_string(&OFFSET).unwrap(),
        "\"AnimatorOffset\""
    );
    assert_eq!(TextPaint::from_parameter(OFFSET), None);
    for version in [3, 48, 49, 52, 53, 54, 55, 56, 57, 58] {
        let mut editor = scene();
        editor.current.project.version = version;
        with_redo(&mut editor);
        let before = serde_json::to_vec(editor.project()).unwrap();
        for frame in [0, 30, u32::MAX] {
            let layer = editor.selected_layer().unwrap();
            assert_eq!(layer.text_value_at(OFFSET, frame), Some(0.));
            assert_eq!(layer.track_value(PATH, frame), Some(0.));
            assert!(layer.track(PATH).is_none());
            assert!(!layer.track_paths().contains(&PATH));
            assert!(
                layer
                    .text_value_command(OFFSET, 0., frame)
                    .unwrap()
                    .is_none()
            );
            let sample = layer.text_animator_at(frame).unwrap();
            assert_eq!((sample.start, sample.end), (0., 100.));
            assert!(sample.is_identity());
        }
        for generic in [false, true] {
            preserved(
                &mut editor,
                edit(
                    OFFSET,
                    TrackEdit::Value {
                        frame: 17,
                        value: -0.,
                    },
                    generic,
                ),
                false,
            );
        }
        assert_eq!(serde_json::to_vec(editor.project()).unwrap(), before);
        assert!(
            !editor
                .project()
                .to_json()
                .unwrap()
                .contains("AnimatorOffset")
        );
    }
}

#[test]
fn text_animator_offset_translates_and_clips_endpoints_without_wrapping_or_source_edits() {
    for (start, end, offset, expected) in [
        (20., 40., 30., [50., 70.]),
        (20., 40., -30., [0., 10.]),
        (70., 90., 20., [90., 100.]),
        (0., 100., -100., [0., 0.]),
        (0., 100., 100., [100., 100.]),
        (25., 25., 30., [55., 55.]),
        (80., 20., -30., [50., 0.]),
        (20., 40., 0., [20., 40.]),
    ] {
        let mut editor = scene();
        set(&mut editor, TextParam::AnimatorStart, 0, start);
        set(&mut editor, TextParam::AnimatorEnd, 0, end);
        set(&mut editor, TextParam::AnimatorPositionX, 0, 13.);
        set(&mut editor, OFFSET, 0, offset);
        let before = editor.project().clone();
        let layer = editor.selected_layer().unwrap();
        let sample = layer.text_animator_at(0).unwrap();
        assert_eq!([sample.start, sample.end], expected);
        assert_eq!(sample.is_identity(), expected[0] >= expected[1]);
        assert_eq!(
            layer.text_value_at(TextParam::AnimatorStart, 0),
            Some(start)
        );
        assert_eq!(layer.text_value_at(TextParam::AnimatorEnd, 0), Some(end));
        assert_eq!(layer.text_value_at(OFFSET, 0), Some(offset));
        assert_eq!(editor.project(), &before);
    }
}

#[test]
fn text_animator_offset_samples_three_independent_tracks_before_endpoint_clipping() {
    let mut editor = scene();
    for (parameter, first, last) in [
        (TextParam::AnimatorStart, 20., 40.),
        (TextParam::AnimatorEnd, 40., 60.),
        (OFFSET, -30., 30.),
    ] {
        set(&mut editor, parameter, 0, first);
        editor
            .execute(edit(
                parameter,
                TrackEdit::ToggleAnimation { frame: 0 },
                false,
            ))
            .unwrap();
        set(&mut editor, parameter, 60, last);
    }
    let before = editor.project().clone();
    for (frame, range, offset) in [
        (0, [0., 10.], -30.),
        (30, [30., 50.], 0.),
        (60, [70., 90.], 30.),
        (80, [70., 90.], 30.),
    ] {
        let layer = editor.selected_layer().unwrap();
        let sample = layer.text_animator_at(frame).unwrap();
        assert_eq!([sample.start, sample.end], range);
        assert_eq!(layer.track_value(PATH, frame), Some(offset));
    }
    assert_eq!(editor.project(), &before);
    with_redo(&mut editor);
    for generic in [false, true] {
        preserved(
            &mut editor,
            edit(
                OFFSET,
                TrackEdit::Value {
                    frame: 30,
                    value: 0.,
                },
                generic,
            ),
            false,
        );
    }
    assert!(
        !editor
            .selected_layer()
            .unwrap()
            .track(PATH)
            .unwrap()
            .keys()
            .contains_key(&30)
    );
}

#[test]
fn text_animator_offset_edits_only_its_source_track_and_undo_restores_schema_and_other_compositions()
 {
    for generic in [false, true] {
        let mut editor = scene();
        set(&mut editor, TextParam::FillRed, 0, 117.25);
        editor.execute(Command::DuplicateComposition).unwrap();
        editor.activate_composition(1).unwrap();
        editor.select(1);
        editor.current.project.version = 55;
        editor.clear_history();
        let before = editor.current.clone();
        editor
            .execute(edit(
                OFFSET,
                TrackEdit::Value {
                    frame: 17,
                    value: -12.3456789012345,
                },
                generic,
            ))
            .unwrap();
        let mut expected = before.clone();
        expected.project.version = 58;
        expected.project.composition.layers[0]
            .text_parameters
            .insert(OFFSET, AnimatedProperty::new(-12.3456789012345));
        assert_eq!(editor.current, expected);
        assert_eq!(editor.undo.len(), 1);
        editor.undo();
        assert_eq!(editor.current, before);
        editor.redo();
        assert_eq!(editor.current, expected);
    }
}

#[test]
fn text_animator_offset_rejects_invalid_commands_atomically_and_cannot_repair_source() {
    for generic in [false, true] {
        let mut editor = scene();
        with_redo(&mut editor);
        for value in [
            -100.001,
            100.001,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            preserved(
                &mut editor,
                edit(OFFSET, TrackEdit::Value { frame: 0, value }, generic),
                true,
            );
        }
        preserved(
            &mut editor,
            edit(
                OFFSET,
                TrackEdit::Value {
                    frame: u32::MAX,
                    value: 0.,
                },
                generic,
            ),
            true,
        );
        editor.current.project.composition.layers[0].locked = true;
        preserved(
            &mut editor,
            edit(OFFSET, TrackEdit::ToggleKey { frame: 0 }, generic),
            true,
        );
        editor.current.project.composition.layers[0].locked = false;
        editor
            .execute(edit(OFFSET, TrackEdit::ToggleKey { frame: 0 }, generic))
            .unwrap();
        assert_eq!(editor.project().version, 58);
        editor.current.project.version = 57;
        with_redo(&mut editor);
        preserved(
            &mut editor,
            edit(
                OFFSET,
                TrackEdit::Value {
                    frame: 0,
                    value: 0.,
                },
                generic,
            ),
            true,
        );
    }
}

#[test]
fn text_animator_offset_native_view_and_dormant_storage_roundtrip_with_strict_version_and_bounds() {
    for inactive in [false, true] {
        for keyed in [false, true] {
            let mut editor = scene();
            editor
                .execute(edit(OFFSET, TrackEdit::ToggleKey { frame: 7 }, false))
                .unwrap();
            if !keyed {
                editor
                    .execute(edit(OFFSET, TrackEdit::ToggleKey { frame: 7 }, true))
                    .unwrap();
            }
            if inactive {
                editor.execute(Command::NewComposition).unwrap();
            }
            let project = editor.project();
            assert_eq!(project.version, 58);
            let json = project.to_json().unwrap();
            assert_eq!(Project::from_json(&json).unwrap(), *project);
            let view = br#" { "version":2, "pins":[{"version":1,"layer":1,"property":{"Text":"AnimatorOffset"}}], "future":{"untouched":true} } "#;
            let native = project_file::encode(project, Some(view)).unwrap();
            let decoded = project_file::decode(&native).unwrap();
            assert_eq!(decoded.project, *project);
            assert_eq!(decoded.view, Some(view.as_slice()));
            assert_eq!(
                project_file::encode(&decoded.project, decoded.view).unwrap(),
                native
            );
            for version in [1, 3, 48, 55, 56, 57, PROJECT_VERSION + 1] {
                let mut bad = project.clone();
                bad.version = version;
                assert!(
                    bad.validate().is_err(),
                    "accepted offset in schema {version}"
                );
                assert!(Project::from_json(&serde_json::to_string(&bad).unwrap()).is_err());
                assert!(project_file::encode(&bad, Some(view)).is_err());
            }
            for value in [-100.001, 100.001, f64::NAN, f64::INFINITY] {
                let mut bad = project.clone();
                let layer = if inactive {
                    &mut bad.other_compositions.get_mut(&1).unwrap().layers[0]
                } else {
                    &mut bad.composition.layers[0]
                };
                let track = layer.text_parameters.get_mut(&OFFSET).unwrap();
                if keyed {
                    track.keys.get_mut(&7).unwrap().value = value;
                } else {
                    track.value = value;
                }
                assert!(bad.validate().is_err());
            }
        }
    }
}

#[test]
fn text_animator_offset_disable_bakes_bounded_value_and_preserves_independent_ranges() {
    let mut editor = scene();
    set(&mut editor, TextParam::AnimatorStart, 0, 20.);
    set(&mut editor, TextParam::AnimatorEnd, 0, 60.);
    set(&mut editor, OFFSET, 0, -100.);
    editor
        .execute(edit(OFFSET, TrackEdit::ToggleAnimation { frame: 0 }, false))
        .unwrap();
    set(&mut editor, OFFSET, 60, 100.);
    let layer = &mut editor.current.project.composition.layers[0];
    layer
        .text_parameters
        .get_mut(&OFFSET)
        .unwrap()
        .keys
        .get_mut(&0)
        .unwrap()
        .temporal
        .outgoing = Some(TemporalHandle {
        slope: 100.,
        influence: 0.8,
    });
    editor.project().validate().unwrap();
    let before = editor.project().clone();
    let sample = editor
        .selected_layer()
        .unwrap()
        .text_value_at(OFFSET, 30)
        .unwrap();
    assert!((-100.0..=100.0).contains(&sample));
    editor
        .execute(edit(OFFSET, TrackEdit::ToggleAnimation { frame: 30 }, true))
        .unwrap();
    let layer = editor.selected_layer().unwrap();
    assert_eq!(layer.track(PATH).unwrap().value, sample);
    assert!(layer.track(PATH).unwrap().keys.is_empty());
    for p in [TextParam::AnimatorStart, TextParam::AnimatorEnd] {
        assert_eq!(
            layer.text_parameters[&p],
            before.composition.layers[0].text_parameters[&p]
        );
    }
    editor.undo();
    assert_eq!(editor.project(), &before);
}
