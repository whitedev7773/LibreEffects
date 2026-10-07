//! Ordered secondary selectors preserve primary animation and source identity.
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
            name: "Selector stack".into(),
        })
        .unwrap();
    editor.clear_history();
    editor
}
fn add() -> Command {
    Command::AddTextRangeSelector { id: 1 }
}
fn set(selector: TextRangeSelector) -> Command {
    Command::SetTextRangeSelector { id: 1, selector }
}
fn remove(selector: u64) -> Command {
    Command::RemoveTextRangeSelector { id: 1, selector }
}
fn reorder(selector: u64, index: usize) -> Command {
    Command::MoveTextRangeSelector {
        id: 1,
        selector,
        index,
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
    );
    let result = editor.execute(command);
    assert_eq!(result.is_err(), rejects, "{result:?}");
    assert_eq!(editor.current, before.0);
    assert_eq!(editor.undo, before.1);
    assert_eq!(editor.redo, before.2);
    assert_eq!(editor.context_generation(), before.3);
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
fn text_range_selectors_defaults_are_sparse_and_legacy_edits_stay_legacy() {
    assert_eq!(MAX_TEXT_RANGE_SELECTORS, 7);
    assert_eq!(
        TextSelectorMode::ALL.map(TextSelectorMode::label),
        ["Add", "Subtract", "Intersect"]
    );
    for version in [3, 48, 55, 56, 57, 58, 59, 60] {
        let mut editor = scene();
        editor.current.project.version = version;
        with_redo(&mut editor);
        let before = serde_json::to_vec(editor.project()).unwrap();
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .text_range_selectors()
                .is_empty()
        );
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .text_animator_at(0)
                .unwrap()
                .selectors
                .is_empty()
        );
        preserved(
            &mut editor,
            Command::SetTextSelector {
                id: 1,
                selector: TextSelector::default(),
            },
            false,
        );
        assert_eq!(serde_json::to_vec(editor.project()).unwrap(), before);
        let json = editor.project().to_json().unwrap();
        assert!(!json.contains("text_range_selector"));
        assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
        for nontext in [false, true] {
            let mut original = editor.project().clone();
            if nontext {
                original.composition.layers[0].content = Content::Rectangle;
            }
            let mut raw = serde_json::to_value(&original).unwrap();
            raw_layer(&mut raw, false)["text_range_selectors"] = json!([]);
            raw_layer(&mut raw, false)["next_text_range_selector_id"] = json!(1);
            assert_eq!(Project::from_json(&raw.to_string()).unwrap(), original);
        }
    }
}

#[test]
fn text_range_selectors_add_edit_reorder_remove_have_stable_ids_and_atomic_history() {
    let mut editor = scene();
    let original = editor.current.clone();
    editor.execute(add()).unwrap();
    let mut expected = original.clone();
    expected.project.version = 61;
    expected.project.composition.layers[0].text_range_selectors = vec![TextRangeSelector::new(1)];
    expected.project.composition.layers[0].next_text_range_selector_id = 2;
    assert_eq!(editor.current, expected);
    assert_eq!(editor.undo.len(), 1);
    editor.undo();
    assert_eq!(editor.current, original);
    editor.redo();
    assert_eq!(editor.current, expected);
    editor.execute(Command::Batch(vec![add(), add()])).unwrap();
    let mut edited = TextRangeSelector::new(2);
    edited.mode = TextSelectorMode::Subtract;
    edited.start = 70.;
    edited.end = 20.;
    edited.offset = -14.25;
    edited.amount = 37.5;
    edited.selector = TextSelector {
        units: TextSelectorUnits::Words,
        shape: TextSelectorShape::Triangle,
    };
    editor.execute(set(edited.clone())).unwrap();
    editor.execute(reorder(2, 0)).unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().text_range_selectors(),
        &[
            edited.clone(),
            TextRangeSelector::new(1),
            TextRangeSelector::new(3)
        ]
    );
    editor.execute(remove(1)).unwrap();
    editor.execute(add()).unwrap();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .text_range_selectors()
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    let after = editor.current.clone();
    editor.undo();
    editor.redo();
    assert_eq!(editor.current, after);
    with_redo(&mut editor);
    preserved(&mut editor, set(edited.clone()), false);
    preserved(&mut editor, reorder(2, 0), false);
    preserved(
        &mut editor,
        Command::Batch(vec![
            Command::Batch(vec![]),
            set(edited.clone()),
            reorder(2, 0),
        ]),
        false,
    );
    preserved(
        &mut editor,
        Command::Batch(vec![add(), set(TextRangeSelector::new(99))]),
        true,
    );
    assert!(editor.selected_layer().unwrap().text_parameters.is_empty());
    assert_eq!(
        editor.selected_layer().unwrap().text_selector(),
        TextSelector::default()
    );
}

#[test]
fn text_range_selectors_limit_missing_identity_locked_and_overflow_reject_atomically() {
    let mut editor = scene();
    for _ in 0..MAX_TEXT_RANGE_SELECTORS {
        editor.execute(add()).unwrap();
    }
    with_redo(&mut editor);
    preserved(&mut editor, add(), true);
    for id in [0, 8, u64::MAX] {
        preserved(&mut editor, remove(id), true);
        preserved(&mut editor, reorder(id, 0), true);
        preserved(&mut editor, set(TextRangeSelector::new(id)), true);
    }
    for index in [MAX_TEXT_RANGE_SELECTORS, usize::MAX] {
        preserved(&mut editor, reorder(1, index), true);
    }
    for command in [
        add(),
        remove(1),
        reorder(1, 1),
        set(TextRangeSelector::new(1)),
    ] {
        let mut locked = Editor::default();
        locked.current = editor.current.clone();
        locked.undo = editor.undo.clone();
        locked.redo = editor.redo.clone();
        locked.current.project.composition.layers[0].locked = true;
        preserved(&mut locked, command, true);
    }
    let mut overflow = scene();
    overflow.current.project.version = 61;
    overflow.current.project.composition.layers[0].next_text_range_selector_id = u64::MAX - 1;
    with_redo(&mut overflow);
    preserved(&mut overflow, add(), true);
    let mut nontext = scene();
    nontext.current.project.composition.layers[0].content = Content::Rectangle;
    with_redo(&mut nontext);
    preserved(&mut nontext, add(), true);
    preserved(
        &mut editor,
        Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        },
        true,
    );
    preserved(&mut editor, Command::AddTextRangeSelector { id: 999 }, true);
}

#[test]
fn text_range_selectors_numeric_fields_reject_nonfinite_and_out_of_bounds() {
    for (field, values) in [
        (
            0,
            [-0.001, 100.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY],
        ),
        (
            1,
            [-0.001, 100.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY],
        ),
        (
            2,
            [
                -100.001,
                100.001,
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ],
        ),
        (
            3,
            [-0.001, 100.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY],
        ),
    ] {
        let mut editor = scene();
        editor.execute(add()).unwrap();
        with_redo(&mut editor);
        for value in values {
            let mut item = TextRangeSelector::new(1);
            match field {
                0 => item.start = value,
                1 => item.end = value,
                2 => item.offset = value,
                _ => item.amount = value,
            }
            preserved(&mut editor, set(item), true);
        }
    }
}

#[test]
fn text_range_selectors_sampling_is_static_ordered_clipped_and_does_not_skip_adds() {
    let mut editor = scene();
    editor
        .execute(Command::Batch(vec![add(), add(), add()]))
        .unwrap();
    for (id, mode, start, end, offset, amount) in [
        (1, TextSelectorMode::Subtract, 15., 60., -30., 25.),
        (2, TextSelectorMode::Add, 40., 80., 40., 75.),
        (3, TextSelectorMode::Intersect, 80., 20., 0., 100.),
    ] {
        let mut item = TextRangeSelector::new(id);
        item.mode = mode;
        item.start = start;
        item.end = end;
        item.offset = offset;
        item.amount = amount;
        item.selector = TextSelector {
            units: TextSelectorUnits::Lines,
            shape: TextSelectorShape::RampDown,
        };
        editor.execute(set(item)).unwrap();
    }
    for (parameter, value) in [
        (TextParam::AnimatorAmount, 0.),
        (TextParam::AnimatorPositionX, 25.),
    ] {
        editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::Value { frame: 0, value },
            })
            .unwrap();
    }
    let before = editor.project().clone();
    let sample = editor
        .selected_layer()
        .unwrap()
        .text_animator_at(0)
        .unwrap();
    assert_eq!(sample.units, TextSelectorUnits::Graphemes);
    assert_eq!(
        sample
            .selectors
            .iter()
            .map(|s| (s.mode, s.start, s.end, s.amount, s.units, s.shape))
            .collect::<Vec<_>>(),
        vec![
            (
                TextSelectorMode::Subtract,
                0.,
                30.,
                25.,
                TextSelectorUnits::Lines,
                TextSelectorShape::RampDown
            ),
            (
                TextSelectorMode::Add,
                80.,
                100.,
                75.,
                TextSelectorUnits::Lines,
                TextSelectorShape::RampDown
            ),
            (
                TextSelectorMode::Intersect,
                80.,
                20.,
                100.,
                TextSelectorUnits::Lines,
                TextSelectorShape::RampDown
            ),
        ]
    );
    assert!(!sample.is_identity());
    assert_eq!(
        editor.selected_layer().unwrap().text_animator_at(u32::MAX),
        Some(sample.clone())
    );
    assert_eq!(editor.project(), &before);
    for mode in TextSelectorMode::ALL {
        for amount in [0., 100.] {
            for (start, end) in [(0., 100.), (50., 50.), (80., 20.)] {
                let sample = TextAnimatorSample {
                    start: 50.,
                    end: 50.,
                    amount: 0.,
                    position: [1., 0.],
                    selectors: vec![TextRangeSelectorSample {
                        mode,
                        start,
                        end,
                        amount,
                        units: TextSelectorUnits::Words,
                        shape: TextSelectorShape::Square,
                    }],
                    ..Default::default()
                };
                assert_eq!(
                    sample.is_identity(),
                    mode != TextSelectorMode::Add || amount == 0. || start >= end
                );
            }
        }
    }
}

#[test]
fn text_range_selectors_roundtrip_all_modes_and_units_active_and_inactive() {
    for inactive in [false, true] {
        for mode in TextSelectorMode::ALL {
            for units in TextSelectorUnits::ALL {
                for shape in TextSelectorShape::ALL {
                    let mut editor = scene();
                    editor.execute(add()).unwrap();
                    let mut item = TextRangeSelector::new(1);
                    item.mode = mode;
                    item.start = 22.25;
                    item.end = 80.;
                    item.offset = -5.;
                    item.amount = 56.125;
                    item.selector = TextSelector { units, shape };
                    editor.execute(set(item)).unwrap();
                    if inactive {
                        editor.execute(Command::NewComposition).unwrap();
                    }
                    let project = editor.project();
                    assert_eq!(project.version, 61);
                    assert_eq!(
                        Project::from_json(&project.to_json().unwrap()).unwrap(),
                        *project
                    );
                    let view = br#"{"version":2,"pins":[{"version":1,"layer":1,"property":{"Text":"AnimatorAmount"}}]}"#;
                    let native = project_file::encode(project, Some(view)).unwrap();
                    let decoded = project_file::decode(&native).unwrap();
                    assert_eq!(decoded.project, *project);
                    assert_eq!(decoded.view, Some(view.as_slice()));
                    assert_eq!(
                        project_file::encode(&decoded.project, decoded.view).unwrap(),
                        native
                    );
                }
            }
        }
    }
}

#[test]
fn text_range_selectors_invalid_storage_rejects_everywhere_and_before_source_repairs() {
    for inactive in [false, true] {
        let mut editor = scene();
        editor.execute(add()).unwrap();
        if inactive {
            editor.execute(Command::NewComposition).unwrap();
        }
        let base = serde_json::to_value(editor.project()).unwrap();
        let valid = serde_json::to_value(TextRangeSelector::new(1)).unwrap();
        for field in ["id", "mode", "start", "end", "offset", "amount", "selector"] {
            let mut raw = base.clone();
            raw_layer(&mut raw, inactive)["text_range_selectors"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            rejected_wire(raw);
        }
        for (field, values) in [
            (
                "id",
                vec![
                    json!(0),
                    json!(2),
                    json!(u64::MAX),
                    json!(-1),
                    json!(1.5),
                    Value::Null,
                ],
            ),
            ("mode", vec![json!("Multiply"), json!(0), Value::Null]),
            (
                "start",
                vec![json!(-1), json!(101), json!("50"), Value::Null],
            ),
            ("end", vec![json!(-1), json!(101), json!("50"), Value::Null]),
            (
                "offset",
                vec![json!(-101), json!(101), json!("50"), Value::Null],
            ),
            (
                "amount",
                vec![json!(-1), json!(101), json!("50"), Value::Null],
            ),
            (
                "selector",
                vec![
                    json!({"units":"Letters"}),
                    json!({"shape":"Random"}),
                    json!({"units":"Words", "future":true}),
                    Value::Null,
                ],
            ),
            ("future", vec![json!(true)]),
        ] {
            for value in values {
                let mut raw = base.clone();
                raw_layer(&mut raw, inactive)["text_range_selectors"][0][field] = value;
                rejected_wire(raw);
            }
        }
        for value in [
            json!(0),
            json!(1),
            json!(u64::MAX),
            json!("2"),
            json!(2.5),
            Value::Null,
        ] {
            let mut raw = base.clone();
            raw_layer(&mut raw, inactive)["next_text_range_selector_id"] = value;
            rejected_wire(raw);
        }
        for list in [
            json!([valid.clone(), valid.clone()]),
            Value::Null,
            json!({}),
            json!([null]),
        ] {
            let mut raw = base.clone();
            raw_layer(&mut raw, inactive)["text_range_selectors"] = list;
            rejected_wire(raw);
        }
        let mut raw = base.clone();
        raw_layer(&mut raw, inactive)["text_range_selectors"] =
            json!((1..=8).map(TextRangeSelector::new).collect::<Vec<_>>());
        raw_layer(&mut raw, inactive)["next_text_range_selector_id"] = json!(9);
        rejected_wire(raw);
        for version in [3, 48, 59, 60, PROJECT_VERSION + 1] {
            let mut raw = base.clone();
            raw["version"] = json!(version);
            rejected_wire(raw);
        }
        let mut raw = base.clone();
        raw_layer(&mut raw, inactive)["content"] = json!("Rectangle");
        rejected_wire(raw);
        let mut raw = base.clone();
        raw_layer(&mut raw, inactive)
            .as_object_mut()
            .unwrap()
            .remove("next_text_range_selector_id");
        rejected_wire(raw);
        for invalid in 0..6 {
            let mut broken = editor.project().clone();
            let layer = resident_layer(&mut broken, inactive);
            match invalid {
                0 => layer.text_range_selectors[0].start = -1.,
                1 => layer.text_range_selectors[0].offset = 101.,
                2 => layer.next_text_range_selector_id = 0,
                3 => layer.next_text_range_selector_id = u64::MAX,
                4 => layer.text_range_selectors.push(TextRangeSelector::new(1)),
                _ => broken.version = 60,
            }
            assert!(broken.validate().is_err());
            assert!(project_file::encode(&broken, None).is_err());
            let mut edited = scene();
            edited.current.project = broken;
            with_redo(&mut edited);
            for command in [
                add(),
                remove(1),
                reorder(1, 0),
                set(TextRangeSelector::new(1)),
            ] {
                preserved(&mut edited, command, true);
            }
        }
    }
}

#[test]
fn text_range_selectors_removed_ids_survive_save_load_and_schema_stays_required() {
    let mut editor = scene();
    editor.execute(add()).unwrap();
    editor.execute(remove(1)).unwrap();
    let project = editor.project();
    assert_eq!(project.version, 61);
    assert!(
        project.composition.layers[0]
            .text_range_selectors()
            .is_empty()
    );
    assert!(
        !project
            .to_json()
            .unwrap()
            .contains("\"text_range_selectors\"")
    );
    assert!(
        project
            .to_json()
            .unwrap()
            .contains("\"next_text_range_selector_id\": 2")
    );
    let loaded = Project::from_json(&project.to_json().unwrap()).unwrap();
    editor.replace_project(loaded).unwrap();
    editor.execute(add()).unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().text_range_selectors()[0].id,
        2
    );
    editor.execute(remove(2)).unwrap();
    let mut raw = serde_json::to_value(editor.project()).unwrap();
    raw["version"] = json!(60);
    rejected_wire(raw);
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
fn text_range_selectors_keep_primary_tracks_source_text_and_duplicate_layer_identity() {
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
            text: "Changed source\n👩‍💻".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetTextSelector {
            id: 1,
            selector: TextSelector {
                units: TextSelectorUnits::Words,
                shape: TextSelectorShape::RampUp,
            },
        })
        .unwrap();
    for parameter in TextParam::ALL.into_iter().filter(|p| p.is_animator()) {
        editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
    }
    let before = editor.selected_layer().unwrap().clone();
    let before_paths = before.track_paths();
    editor
        .execute(Command::Batch(vec![
            add(),
            Command::RenameLayer {
                id: 1,
                name: "Mixed route".into(),
            },
        ]))
        .unwrap();
    assert_eq!(editor.project().version, 61);
    let after = editor.selected_layer().unwrap();
    assert_eq!(after.text_parameters, before.text_parameters);
    assert_eq!(after.text_selector(), before.text_selector());
    assert_eq!(after.source_text_animation, before.source_text_animation);
    assert_eq!(after.content, before.content);
    assert_eq!(after.track_paths(), before_paths);
    let original = after.clone();
    editor.execute(Command::DuplicateLayers(vec![1])).unwrap();
    let copy = editor.selected_layer().unwrap();
    assert_eq!(copy.text_range_selectors(), original.text_range_selectors());
    assert_eq!(
        copy.next_text_range_selector_id,
        original.next_text_range_selector_id
    );
    assert_eq!(copy.text_parameters, original.text_parameters);
    assert_eq!(copy.source_text_animation, original.source_text_animation);
    editor.execute(Command::DuplicateComposition).unwrap();
    assert!(
        editor
            .project()
            .composition
            .layers
            .iter()
            .all(|layer| layer.text_range_selectors() == original.text_range_selectors())
    );
    assert_eq!(
        text_animation::animator_required_version(editor.project()),
        Some(61)
    );
}
