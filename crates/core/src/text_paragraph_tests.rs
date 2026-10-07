use super::*;

const FIELDS: [(TextParagraphField, &str); 5] = [
    (TextParagraphField::LeftIndent, "paragraph_left_indent"),
    (TextParagraphField::RightIndent, "paragraph_right_indent"),
    (
        TextParagraphField::FirstLineIndent,
        "paragraph_first_line_indent",
    ),
    (TextParagraphField::SpaceBefore, "paragraph_space_before"),
    (TextParagraphField::SpaceAfter, "paragraph_space_after"),
];

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Base 한글\r\n\nSecond paragraph\tword".into(),
                font_size: 40.0,
            },
            width: 300.0,
            height: 180.0,
            name: "Paragraph".into(),
        })
        .unwrap();
    editor
}

fn command(id: LayerId, field: TextParagraphField, value: f64) -> Command {
    Command::SetTextParagraphValue { id, field, value }
}

fn style_value(style: &mut TextStyle, field: TextParagraphField, value: f64) {
    match field {
        TextParagraphField::LeftIndent => style.paragraph_left_indent = value,
        TextParagraphField::RightIndent => style.paragraph_right_indent = value,
        TextParagraphField::FirstLineIndent => style.paragraph_first_line_indent = value,
        TextParagraphField::SpaceBefore => style.paragraph_space_before = value,
        TextParagraphField::SpaceAfter => style.paragraph_space_after = value,
    }
}

fn with_redo(editor: &mut Editor) {
    editor
        .execute(Command::RenameLayer {
            id: editor.selected().unwrap(),
            name: "Future name".into(),
        })
        .unwrap();
    editor.undo();
    assert!(editor.can_redo());
}

fn rejected(editor: &mut Editor, command: Command) {
    let current = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    assert!(editor.execute(command).is_err());
    assert_eq!(editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
}

fn unchanged(editor: &mut Editor, command: Command) {
    let current = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    let json = editor.project().to_json().unwrap();
    editor.execute(command).unwrap();
    assert_eq!(editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    assert_eq!(editor.project().to_json().unwrap(), json);
}

#[test]
fn text_paragraph_defaults_preserve_legacy_style_shape_and_native_envelope() {
    let old_shape = serde_json::json!({
        "font_family": "Wanted Sans", "font_face": "", "weight": 400,
        "italic": false, "leading": 1.2, "tracking": 0.0, "align": "Left",
        "paragraph": false, "fill_enabled": true, "stroke_enabled": false,
        "stroke_color": 0, "stroke_width": 1.0, "stroke_over_fill": false,
        "stroke_join": "Miter"
    });
    for input in [old_shape.clone(), serde_json::json!({})] {
        let style: TextStyle = serde_json::from_value(input).unwrap();
        assert_eq!(style, TextStyle::default());
        assert!(style.valid());
        assert!(!style.has_paragraph_override());
        assert_eq!(serde_json::to_value(style).unwrap(), old_shape);
    }
    let editor = scene();
    let before = editor.project().clone();
    assert_eq!(before.version, 3);
    let json = before.to_json().unwrap();
    assert!(!json.contains("text_style"));
    let view = br#"{"version":2,"frame":27}"#;
    let bytes = project_file::encode(&before, Some(view)).unwrap();
    assert_eq!(&bytes[8..10], &[1, 0]);
    let decoded = project_file::decode(&bytes).unwrap();
    assert_eq!(decoded.project, before);
    assert_eq!(Project::from_json(&json).unwrap().to_json().unwrap(), json);
    assert_eq!(
        project_file::encode(&decoded.project, Some(view)).unwrap(),
        bytes
    );
    // Explicit new zero fields are ordinary defaults even in older files, and
    // disappear independently when a pre-existing style is serialized again.
    for paragraph in [false, true] {
        let mut original = before.clone();
        if paragraph {
            original.version = 33;
            original.composition.layers[0].text_style.paragraph = true;
        }
        let mut explicit = serde_json::to_value(&original).unwrap();
        explicit["composition"]["layers"][0]["text_style"] =
            serde_json::to_value(&original.composition.layers[0].text_style).unwrap();
        for (_, name) in FIELDS {
            explicit["composition"]["layers"][0]["text_style"][name] = 0.0.into();
        }
        let reopened = Project::from_json(&explicit.to_string()).unwrap();
        assert_eq!(reopened, original);
        assert_eq!(reopened.to_json().unwrap(), original.to_json().unwrap());
        assert_eq!(
            project_file::encode(&reopened, Some(view)).unwrap(),
            project_file::encode(&original, Some(view)).unwrap()
        );
    }
}

#[test]
fn text_paragraph_fields_are_individually_sparse_and_keep_all_legacy_fields() {
    assert_eq!(TextParagraphField::ALL, FIELDS.map(|(field, _)| field));
    let legacy = serde_json::to_value(TextStyle::default()).unwrap();
    for (field, name) in FIELDS {
        let mut style = TextStyle::default();
        style_value(&mut style, field, 3.125);
        let value = serde_json::to_value(&style).unwrap();
        assert_eq!(field.value(&style), 3.125);
        assert!(style.has_paragraph_override());
        for (_, other) in FIELDS {
            assert_eq!(value.get(other).is_some(), other == name);
        }
        let mut expected = legacy.clone();
        expected[name] = 3.125.into();
        assert_eq!(value, expected);
        assert_eq!(serde_json::from_value::<TextStyle>(value).unwrap(), style);
        for zero in [0.0, -0.0] {
            style_value(&mut style, field, zero);
            assert!(style.is_default());
            assert!(!style.has_paragraph_override());
            assert_eq!(serde_json::to_value(&style).unwrap(), legacy);
        }
    }
}

#[test]
fn text_paragraph_all_bounds_and_invalid_commands_are_atomic() {
    for (field, _) in FIELDS {
        let (min, max) = field.bounds();
        assert_eq!(max, 16_384.0);
        assert_eq!(
            min,
            if field == TextParagraphField::FirstLineIndent {
                -16_384.0
            } else {
                0.0
            }
        );
        let mut editor = scene();
        for value in [min, max, 0.0] {
            editor.execute(command(1, field, value)).unwrap();
            assert_eq!(
                field.value(&editor.selected_layer().unwrap().text_style),
                value
            );
            assert!(editor.selected_layer().unwrap().text_style.valid());
        }
        with_redo(&mut editor);
        for value in [
            min - 0.001,
            max + 0.001,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert!(
                editor
                    .selected_layer()
                    .unwrap()
                    .text_paragraph_value_command(field, value)
                    .is_err()
            );
            rejected(&mut editor, command(1, field, value));
            let mut style = editor.selected_layer().unwrap().text_style();
            style_value(&mut style, field, value);
            assert!(!style.valid());
            rejected(&mut editor, Command::SetTextStyle { id: 1, style });
        }
    }
}

#[test]
fn text_paragraph_version_gate_covers_each_active_inactive_and_dormant_field() {
    for (field, name) in FIELDS {
        for paragraph in [false, true] {
            for inactive in [false, true] {
                let mut editor = scene();
                editor.current.project.composition.layers[0]
                    .text_style
                    .paragraph = paragraph;
                // The mode flag has its pre-existing minimum; the field starts sparse.
                editor.current.project.version = if paragraph { 33 } else { 3 };
                editor.execute(command(1, field, 0.25)).unwrap();
                assert_eq!(editor.project().version, 55);
                if inactive {
                    editor.execute(Command::NewComposition).unwrap();
                }
                let project = editor.project();
                let json = project.to_json().unwrap();
                assert_eq!(Project::from_json(&json).unwrap(), *project);
                let bytes = project_file::encode(project, None).unwrap();
                assert_eq!(project_file::decode(&bytes).unwrap().project, *project);
                for version in [54, 33, 28, 3, 1, PROJECT_VERSION + 1] {
                    let mut bad = serde_json::to_value(project).unwrap();
                    bad["version"] = version.into();
                    assert!(
                        Project::from_json(&bad.to_string()).is_err(),
                        "accepted {name} at {version}"
                    );
                    assert!(document::decode_native(bad, BTreeMap::new()).is_err());
                }
                let mut bad = project.clone();
                let layer = if inactive {
                    &mut bad.other_compositions.get_mut(&1).unwrap().layers[0]
                } else {
                    &mut bad.composition.layers[0]
                };
                layer.text_style.paragraph = false;
                layer.content = Content::Rectangle;
                assert!(bad.validate().is_err(), "accepted {name} on non-text");
            }
        }
    }
}

#[test]
fn text_paragraph_malformed_inactive_json_and_nonfinite_styles_reject() {
    let mut editor = scene();
    editor
        .execute(command(1, TextParagraphField::LeftIndent, 1.0))
        .unwrap();
    editor.execute(Command::NewComposition).unwrap();
    let original = editor.project().clone();
    for (field, name) in FIELDS {
        for value in [
            serde_json::Value::Null,
            serde_json::json!("1"),
            serde_json::json!(16_384.01),
            serde_json::json!(-16_384.01),
        ] {
            let mut json = serde_json::to_value(&original).unwrap();
            json["other_compositions"]["1"]["layers"][0]["text_style"][name] = value;
            assert!(Project::from_json(&json.to_string()).is_err());
            assert!(document::decode_native(json, BTreeMap::new()).is_err());
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut bad = original.clone();
            style_value(
                &mut bad.other_compositions.get_mut(&1).unwrap().layers[0].text_style,
                field,
                value,
            );
            assert!(bad.validate().is_err());
        }
    }
}

#[test]
fn text_paragraph_planner_preserves_source_text_typography_paint_and_inactive_compositions() {
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
            frame: 25,
            text: "Animated 👋\r\n\nText".into(),
        })
        .unwrap();
    for (parameter, value) in [
        (TextParam::FontSize, 72.0),
        (TextParam::Tracking, 63.25),
        (TextParam::Leading, 2.375),
        (TextParam::FillOpacity, 37.5),
    ] {
        editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 10 },
            })
            .unwrap();
        editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::Value { frame: 40, value },
            })
            .unwrap();
        editor
            .execute(Command::SetTemporalHandle {
                id: 1,
                property: PropertyPath::Text(parameter),
                frame: 10,
                incoming: false,
                handle: TemporalHandle {
                    slope: 0.375,
                    influence: 0.3,
                },
            })
            .unwrap();
    }
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                paragraph: true,
                align: TextAlign::Right,
                stroke_enabled: true,
                stroke_color: 0xabcdef,
                ..Default::default()
            },
        })
        .unwrap();
    editor.execute(Command::DuplicateComposition).unwrap();
    editor.activate_composition(1).unwrap();
    with_redo(&mut editor);
    for ((field, _), value) in FIELDS.into_iter().zip([13.25, 7.5, -8.75, 11.0, 4.125]) {
        let before = editor.current.clone();
        let previous_undo = editor.undo.len();
        let planned = editor
            .selected_layer()
            .unwrap()
            .text_paragraph_value_command(field, value)
            .unwrap()
            .unwrap();
        assert!(matches!(
            planned,
            Command::SetTextParagraphValue { id: 1, .. }
        ));
        assert_eq!(editor.current, before);
        let mut expected = before.clone();
        expected.project.version = 55;
        style_value(
            &mut expected.project.composition.layers[0].text_style,
            field,
            value,
        );
        editor.execute(planned).unwrap();
        assert_eq!(editor.current, expected);
        assert_eq!(editor.undo.len(), previous_undo + 1);
        assert!(!editor.can_redo());
        editor.undo();
        assert_eq!(editor.current, before);
        editor.redo();
        assert_eq!(editor.current, expected);
        assert_eq!(
            project_file::decode(&project_file::encode(editor.project(), None).unwrap())
                .unwrap()
                .project,
            expected.project
        );
    }
    assert_eq!(
        TextParam::ALL
            .iter()
            .filter(|parameter| !parameter.is_animator())
            .count(),
        12,
        "paragraph fields never become typography tracks"
    );
}

#[test]
fn text_paragraph_locked_missing_nontext_and_invalid_source_reject_even_noops() {
    let mut editor = scene();
    editor.execute(Command::AddRectangle).unwrap();
    with_redo(&mut editor);
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .text_paragraph_value_command(TextParagraphField::LeftIndent, 0.0)
            .is_err()
    );
    for id in [2, 999] {
        rejected(
            &mut editor,
            command(id, TextParagraphField::LeftIndent, 0.0),
        );
    }
    editor.execute(Command::ToggleLocked(1)).unwrap();
    with_redo(&mut editor);
    for value in [0.0, 1.0] {
        assert!(
            editor
                .project()
                .composition
                .layer(1)
                .unwrap()
                .text_paragraph_value_command(TextParagraphField::LeftIndent, value)
                .is_err()
        );
        rejected(
            &mut editor,
            command(1, TextParagraphField::LeftIndent, value),
        );
    }
    let mut editor = scene();
    editor
        .execute(command(1, TextParagraphField::LeftIndent, 1.0))
        .unwrap();
    with_redo(&mut editor);
    editor.current.project.version = 54;
    for value in [0.0, 1.0, 2.0] {
        rejected(
            &mut editor,
            command(1, TextParagraphField::LeftIndent, value),
        );
    }
}

#[test]
fn text_paragraph_exact_format_only_and_batch_return_noops_preserve_redo_and_schema() {
    let mut editor = scene();
    with_redo(&mut editor);
    for (field, _) in FIELDS {
        for value in ["0", "0.0000", "-0.0"] {
            let value = value.parse().unwrap();
            assert!(
                editor
                    .selected_layer()
                    .unwrap()
                    .text_paragraph_value_command(field, value)
                    .unwrap()
                    .is_none()
            );
            unchanged(&mut editor, command(1, field, value));
        }
    }
    assert_eq!(editor.project().version, 3);
    unchanged(
        &mut editor,
        Command::Batch(vec![
            command(1, TextParagraphField::LeftIndent, 12.5),
            Command::Batch(vec![]),
            Command::Batch(vec![command(1, TextParagraphField::LeftIndent, 0.0)]),
        ]),
    );
    assert_eq!(editor.project().version, 3);
    editor
        .execute(command(1, TextParagraphField::SpaceBefore, 3.25))
        .unwrap();
    editor.current.project.composition.layers[0]
        .text_style
        .paragraph_right_indent = -0.0;
    with_redo(&mut editor);
    let same = "3.2500000".parse().unwrap();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .text_paragraph_value_command(TextParagraphField::SpaceBefore, same)
            .unwrap()
            .is_none()
    );
    unchanged(
        &mut editor,
        command(1, TextParagraphField::SpaceBefore, same),
    );
    unchanged(
        &mut editor,
        command(1, TextParagraphField::RightIndent, 0.0),
    );
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .text_style
            .paragraph_right_indent
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(editor.project().version, 55);
}

#[test]
fn text_paragraph_batch_is_one_history_step_and_invalid_member_rolls_back_all() {
    let mut editor = scene();
    with_redo(&mut editor);
    let before = editor.current.clone();
    let old_undo = editor.undo.len();
    rejected(
        &mut editor,
        Command::Batch(vec![
            command(1, TextParagraphField::LeftIndent, 13.5),
            command(1, TextParagraphField::SpaceAfter, -1.0),
        ]),
    );
    rejected(
        &mut editor,
        Command::Batch(vec![
            command(1, TextParagraphField::LeftIndent, 13.5),
            command(999, TextParagraphField::SpaceAfter, 1.0),
        ]),
    );
    editor
        .execute(Command::Batch(vec![
            command(1, TextParagraphField::LeftIndent, 13.5),
            Command::Batch(vec![
                command(1, TextParagraphField::FirstLineIndent, -7.25),
                command(1, TextParagraphField::SpaceAfter, 4.0),
            ]),
        ]))
        .unwrap();
    let after = editor.current.clone();
    assert_eq!(editor.undo.len(), old_undo + 1);
    assert!(!editor.can_redo());
    let mut expected = before.clone();
    expected.project.version = 55;
    let style = &mut expected.project.composition.layers[0].text_style;
    style.paragraph_left_indent = 13.5;
    style.paragraph_first_line_indent = -7.25;
    style.paragraph_space_after = 4.0;
    assert_eq!(after, expected);
    editor.undo();
    assert_eq!(editor.current, before);
    editor.redo();
    assert_eq!(editor.current, after);
}

#[test]
fn text_paragraph_point_mode_retains_fields_and_generic_paths_keep_schema_minimum() {
    let mut editor = scene();
    let style = TextStyle {
        paragraph_left_indent: 25.0,
        paragraph_right_indent: 20.0,
        paragraph_first_line_indent: -10.0,
        paragraph_space_before: 5.0,
        paragraph_space_after: 6.0,
        ..Default::default()
    };
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: style.clone(),
        })
        .unwrap();
    assert_eq!(editor.project().version, 55);
    assert_eq!(editor.selected_layer().unwrap().text_style(), style);
    let mut paragraph = style.clone();
    paragraph.paragraph = true;
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: paragraph.clone(),
        })
        .unwrap();
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: style.clone(),
        })
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().text_style(), style);
    editor.undo();
    assert_eq!(editor.selected_layer().unwrap().text_style(), paragraph);
    editor.redo();
    assert_eq!(editor.selected_layer().unwrap().text_style(), style);
    editor.execute(Command::NewComposition).unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    assert_eq!(
        editor.project().version,
        55,
        "inactive overrides participate in generic minimum recalculation"
    );
    editor.activate_composition(1).unwrap();
    for (field, _) in FIELDS {
        editor.execute(command(1, field, 0.0)).unwrap();
    }
    assert!(
        !editor
            .selected_layer()
            .unwrap()
            .text_style
            .has_paragraph_override()
    );
    assert_eq!(
        editor.project().version,
        55,
        "source-preserving field edits do not lower schema"
    );
}

#[test]
fn text_paragraph_legacy_media_noops_keep_assets_and_materialization_fails_atomically() {
    let mut editor = scene();
    editor
        .execute(Command::AddContent {
            content: Content::Video {
                path: "legacy.mov".into(),
                audio: None,
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 100.0,
            height: 100.0,
            name: "Legacy footage".into(),
        })
        .unwrap();
    // Current media keeps every asset identity and unrelated source byte.
    let before = editor.current.clone();
    let mut expected = before.clone();
    expected.project.version = 55;
    expected
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap()
        .text_style
        .paragraph_left_indent = 8.25;
    editor
        .execute(command(1, TextParagraphField::LeftIndent, 8.25))
        .unwrap();
    assert_eq!(editor.current, expected);
    editor.undo();
    assert_eq!(editor.current, before);
    // Valid old media lacks the asset identities required by all schema >=22 files.
    editor.current.project.asset_library = AssetLibrary::default();
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .for_each(|layer| layer.asset = None);
    editor.current.project.version = 6;
    editor.select(1);
    editor.project().validate().unwrap();
    let mut future = editor.current.clone();
    future.project.composition.name = "Future composition".into();
    editor.redo = vec![future];
    unchanged(&mut editor, command(1, TextParagraphField::LeftIndent, 0.0));
    rejected(&mut editor, command(1, TextParagraphField::LeftIndent, 1.0));
    assert!(editor.project().asset_library.is_default());
    assert_eq!(editor.project().version, 6);
}

fn budget_scene() -> Editor {
    let mut editor = scene();
    editor.current.project.version = 55;
    let mut template = editor.project().composition.layers[0].clone();
    template.text_style.paragraph_left_indent = 1.0;
    template.content = Content::Text {
        text: "\u{0001}".repeat(16_384),
        font_size: 24.0,
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
    let limit = 16 * 1024 * 1024;
    let mut remaining = (size(editor.project()) - limit).div_ceil(5);
    for layer in &mut editor.current.project.composition.layers {
        let Content::Text { text, .. } = &mut layer.content else {
            unreachable!()
        };
        let count = remaining.min(text.len());
        *text = "x".repeat(count) + &"\u{0001}".repeat(text.len() - count);
        remaining -= count;
    }
    assert_eq!(remaining, 0);
    let padding = limit - size(editor.project());
    assert!(padding < 5);
    editor
        .current
        .project
        .composition
        .name
        .push_str(&"x".repeat(padding));
    editor.project().validate().unwrap();
    document::validate_budget(editor.project()).unwrap();
    assert_eq!(size(editor.project()), limit);
    let mut future = editor.current.clone();
    future.project.composition.name = "Future".into();
    editor.redo = vec![future];
    editor
}

#[test]
fn text_paragraph_metadata_budget_checks_original_and_candidate_before_history() {
    let mut editor = budget_scene();
    rejected(
        &mut editor,
        command(1, TextParagraphField::LeftIndent, 10.0),
    );
    unchanged(
        &mut editor,
        Command::Batch(vec![
            command(1, TextParagraphField::LeftIndent, 10.0),
            command(1, TextParagraphField::LeftIndent, 1.0),
        ]),
    );
    editor.current.project.composition.name.push('x');
    editor.project().validate().unwrap();
    for value in [0.0, 1.0, 2.0] {
        rejected(
            &mut editor,
            command(1, TextParagraphField::LeftIndent, value),
        );
    }
}
