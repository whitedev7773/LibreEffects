//! Public model contracts with synthetic text/font identities, never private assets.
use crate::text_buffer::Buffer;
use libre_effects_core::*;

fn positioned(text: &str) -> RichText {
    let mut style = TextCharacterStyle::from_style(&TextStyle::default(), 24.0, 0xffffff);
    style.font_family = "Synthetic Sans".into();
    style.font_face = "SyntheticSans-Regular".into();
    let mut rich = RichText::new(
        text,
        style.clone(),
        if text.is_empty() {
            vec![]
        } else {
            vec![TextStyleRun {
                start: 0,
                end: text.len(),
                style: style.clone(),
            }]
        },
    )
    .unwrap();
    rich.point_origin = true;
    style.leading = Some(TextLeading::Auto(1.2));
    rich.positioning = Some(AuthoredTextPositions {
        text: text.into(),
        align: TextAlign::Left,
        lines: text_paragraphs::paragraphs(text)
            .filter(|line| !line.range.is_empty())
            .map(|line| AuthoredTextLine {
                start: line.range.start,
                end: line.range.end,
                font_sha256: "ab".repeat(32),
                font_index: 0,
                style: style.clone(),
                glyphs: line
                    .text
                    .char_indices()
                    .enumerate()
                    .map(|(i, (offset, c))| AuthoredTextGlyph {
                        start: line.range.start + offset,
                        end: line.range.start + offset + c.len_utf8(),
                        glyph_id: 10 + i as u16,
                        x: -10.0 + i as f64 * 17.5,
                    })
                    .collect(),
                end_x: -10.0 + line.text.chars().count() as f64 * 17.5,
            })
            .collect(),
    });
    rich
}

fn editor(text: &str) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: text.into(),
                font_size: 24.0,
            },
            width: 640.0,
            height: 200.0,
            name: "Synthetic saved spacing".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(positioned(text)),
        })
        .unwrap();
    editor.clear_history();
    editor
}

fn saved(editor: &Editor) -> &RichText {
    editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .rich_text()
        .unwrap()
}

fn invalid(mutator: impl FnOnce(&mut RichText)) {
    let text = "猫A\r\n한B";
    let mut rich = positioned(text);
    mutator(&mut rich);
    assert!(
        rich.validate_positioning(text, &TextStyle::default())
            .is_err()
    );
}

#[test]
fn authored_spacing_roundtrip_schema80_and_legacy_serialization() {
    let text = "猫A\r\n\r한B\n";
    let mut editor = editor(text);
    let project = editor.project().clone();
    let json = project.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), project);
    let native = project_file::encode(&project, None).unwrap();
    let reopened = project_file::decode(&native).unwrap().project;
    assert_eq!(reopened, project);
    assert_eq!(project_file::encode(&reopened, None).unwrap(), native);
    let mut value = serde_json::to_value(&project).unwrap();
    assert_eq!(value["version"], 80);
    for version in [71, 74, 79] {
        value["version"] = version.into();
        assert!(Project::from_json(&value.to_string()).is_err());
    }
    let mut legacy = positioned(text);
    legacy.positioning = None;
    legacy.point_origin = false;
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(legacy),
        })
        .unwrap();
    value = serde_json::to_value(editor.project()).unwrap();
    value["version"] = 71.into();
    editor
        .replace_project(Project::from_json(&value.to_string()).unwrap())
        .unwrap();
    let json = editor.project().to_json().unwrap();
    assert!(!json.contains("positioning"));
    let legacy = Project::from_json(&json).unwrap();
    assert_eq!(legacy.to_json().unwrap(), json);
    let original = editor.project().clone();
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(positioned(text)),
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(editor.project()).unwrap()["version"],
        80
    );
    editor.undo();
    assert_eq!(editor.project(), &original);
    editor.redo();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .has_authored_text_positions()
    );
}

#[test]
fn authored_spacing_maps_exact_cjk_crlf_and_allows_only_complete_line_subset() {
    let text = "猫A\r\n\r한B\n";
    let mut rich = positioned(text);
    rich.validate_positioning(text, &TextStyle::default())
        .unwrap();
    let positions = rich.positioning.as_ref().unwrap();
    assert_eq!((positions.lines[0].start, positions.lines[0].end), (0, 4));
    assert_eq!((positions.lines[1].start, positions.lines[1].end), (7, 11));
    assert_eq!(
        (
            positions.lines[0].glyphs[0].start,
            positions.lines[0].glyphs[0].end
        ),
        (0, 3)
    );
    let baselines = rich.line_metrics(text, &TextStyle::default()).unwrap();
    rich.positioning.as_mut().unwrap().lines.remove(0);
    rich.validate_positioning(text, &TextStyle::default())
        .unwrap();
    assert_eq!(
        rich.line_metrics(text, &TextStyle::default()).unwrap(),
        baselines
    );
    assert_eq!(baselines.len(), 4);
    let mut ordinary = rich.clone();
    ordinary.reset_positioning();
    assert_eq!(
        ordinary.line_metrics(text, &TextStyle::default()).unwrap(),
        baselines
    );
    // Unpositioned-line edits still invalidate the complete saved snapshot.
    let next = rich
        .format_range(text, &(0..3), &TextCharacterPatch::FontSize(30.0))
        .unwrap();
    assert!(next.positioning.is_none());
}

#[test]
fn authored_spacing_rejects_bad_source_geometry_font_and_style_snapshots() {
    invalid(|r| r.positioning.as_mut().unwrap().text = "狗A\r\n한B".into());
    invalid(|r| r.point_origin = false);
    invalid(|r| r.positioning.as_mut().unwrap().lines.clear());
    invalid(|r| r.positioning.as_mut().unwrap().lines.reverse());
    invalid(|r| {
        let p = r.positioning.as_mut().unwrap();
        p.lines.push(p.lines[0].clone());
    });
    for mutate in [
        (|l: &mut AuthoredTextLine| l.start = 1) as fn(&mut AuthoredTextLine),
        |l| l.end += 2,
        |l| l.end = usize::MAX,
        |l| l.font_sha256.clear(),
        |l| l.font_sha256 = "z".repeat(64),
        |l| l.style.font_face.clear(),
        |l| l.style.leading = None,
        |l| l.style.leading = Some(TextLeading::Auto(1.5)),
        |l| l.style.font_size += 1.0,
        |l| l.style.tracking += 1.0,
        |l| l.glyphs.clear(),
        |l| {
            l.glyphs.pop();
        },
        |l| l.glyphs.push(l.glyphs[0].clone()),
        |l| l.glyphs[0].end = 1,
        |l| l.glyphs[1].start = 0,
        |l| l.glyphs[0].glyph_id = 0,
        |l| l.glyphs[0].x = f64::NAN,
        |l| l.glyphs[0].x = -1_000_000.1,
        |l| l.glyphs[1].x = l.glyphs[0].x,
        |l| l.end_x = l.glyphs.last().unwrap().x,
        |l| l.end_x = f64::INFINITY,
        |l| l.end_x = 1_000_000.1,
    ] {
        invalid(|r| mutate(&mut r.positioning.as_mut().unwrap().lines[0]));
    }
    for source in ["", "\r\n", "e\u{301}", "👩‍👧", "A\tB"] {
        assert!(positioned(source).validate(source).is_err(), "{source:?}");
    }
    let mut rich = positioned("AB");
    rich.runs = vec![
        TextStyleRun {
            start: 0,
            end: 1,
            style: rich.default_style.clone(),
        },
        TextStyleRun {
            start: 1,
            end: 2,
            style: rich.default_style.clone(),
        },
    ];
    rich.runs[1].style.font_face = "SyntheticSans-Other".into();
    assert!(rich.validate("AB").is_err());
    let mismatch = TextStyle {
        align: TextAlign::Center,
        ..Default::default()
    };
    assert!(
        positioned("AB")
            .validate_positioning("AB", &mismatch)
            .is_err()
    );
    let boxed = TextStyle {
        paragraph: true,
        ..Default::default()
    };
    assert!(positioned("AB").validate_positioning("AB", &boxed).is_err());
}

#[test]
fn authored_spacing_denies_unknown_payload_fields_at_every_level() {
    let original = serde_json::to_value(positioned("AB")).unwrap();
    for pointer in [
        "",
        "/positioning",
        "/positioning/lines/0",
        "/positioning/lines/0/glyphs/0",
    ] {
        let mut value = original.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("future_field".into(), true.into());
        assert!(
            serde_json::from_value::<RichText>(value).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn authored_spacing_typography_edits_drop_cache_and_undo_restores_exact_payload() {
    let text = "AB\r\n猫";
    let original = editor(text);
    let mut changed_run = saved(&original).clone();
    changed_run.runs[0].style.font_size = 30.0;
    let mut moved_origin = saved(&original).clone();
    moved_origin.point_origin = false;
    let mut changed_source = saved(&original).clone();
    // Same byte count still invalidates the source fingerprint.
    let replacement = "CB\r\n猫";
    changed_source.runs[0].end = replacement.len();
    let font = saved(&original).default_style.font();
    let mut replacement_font = font.clone();
    replacement_font.face = "SyntheticSans-Replacement".into();
    let commands = vec![
        Command::ReplaceTextRange {
            id: 1,
            start: 0,
            end: 1,
            text: "C".into(),
        },
        Command::EditSourceText {
            id: 1,
            frame: 0,
            text: replacement.into(),
        },
        Command::SetContent {
            id: 1,
            content: Content::Text {
                text: replacement.into(),
                font_size: 24.0,
            },
        },
        Command::SetContent {
            id: 1,
            content: Content::Text {
                text: text.into(),
                font_size: 30.0,
            },
        },
        Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                align: TextAlign::Center,
                ..Default::default()
            },
        },
        Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                leading: 1.5,
                ..Default::default()
            },
        },
        Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                tracking: 2.0,
                ..Default::default()
            },
        },
        Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                italic: true,
                ..Default::default()
            },
        },
        Command::SetRichText {
            id: 1,
            rich_text: Some(changed_run),
        },
        Command::SetRichText {
            id: 1,
            rich_text: Some(moved_origin),
        },
        Command::SetStyledText {
            id: 1,
            text: replacement.into(),
            rich_text: changed_source,
        },
        Command::ReplaceTextFont {
            from: font,
            to: replacement_font,
        },
    ];
    for command in commands {
        let mut editor = editor(text);
        let before = editor.project().clone();
        editor.execute(command).unwrap();
        assert!(saved(&editor).positioning.is_none());
        let after = editor.project().clone();
        editor.undo();
        assert_eq!(editor.project(), &before);
        editor.redo();
        assert_eq!(editor.project(), &after);
    }
}

#[test]
fn authored_spacing_noops_keep_history_and_paint_transforms_keep_cache() {
    let text = "AB猫";
    let mut editor = editor(text);
    let original = editor.project().clone();
    let positioning = saved(&editor).positioning.clone();
    let generation = editor.context_generation();
    for command in [
        Command::ReplaceTextRange {
            id: 1,
            start: 0,
            end: 1,
            text: "A".into(),
        },
        Command::EditSourceText {
            id: 1,
            frame: 0,
            text: text.into(),
        },
        Command::SetRichText {
            id: 1,
            rich_text: Some(saved(&editor).clone()),
        },
        Command::SetStyledText {
            id: 1,
            text: text.into(),
            rich_text: saved(&editor).clone(),
        },
    ] {
        editor.execute(command).unwrap();
        assert_eq!(editor.project(), &original);
        assert_eq!(editor.context_generation(), generation);
        assert!(!editor.can_undo());
    }
    for command in [
        Command::SetColor {
            id: 1,
            color: 0x123456,
        },
        Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                stroke_enabled: true,
                stroke_width: 4.0,
                stroke_over_fill: true,
                ..Default::default()
            },
        },
        Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame: 0,
            value: 37.0,
        },
        Command::SetValue {
            id: 1,
            property: Property::ScaleX,
            frame: 0,
            value: 105.0,
        },
    ] {
        editor.execute(command).unwrap();
        assert_eq!(saved(&editor).positioning, positioning);
    }
    let rich = saved(&editor);
    let painted = rich
        .format_range(text, &(0..1), &TextCharacterPatch::FillColor(0xabcdef))
        .unwrap();
    assert_eq!(painted.positioning, positioning);
    assert_eq!(painted.runs.len(), 2);
    painted
        .validate_positioning(text, &editor.selected_layer().unwrap().text_style())
        .unwrap();
    let unchanged = painted
        .format_range(text, &(0..text.len()), &TextCharacterPatch::FontSize(24.0))
        .unwrap();
    assert_eq!(unchanged, painted);
}

#[test]
fn authored_spacing_draft_edit_reset_and_formatting_have_atomic_local_history() {
    let text = "猫A\r\n한B";
    let mut buffer = Buffer::new(text.into());
    let rich = positioned(text);
    let base = rich.default_style.clone();
    buffer.rich_text = Some(rich.clone());
    buffer.anchor = 0;
    buffer.caret = 3;
    assert!(
        !buffer
            .format_selection(&base, &TextCharacterPatch::FontSize(24.0))
            .unwrap()
    );
    assert!(
        buffer
            .format_selection(&base, &TextCharacterPatch::FillColor(0x123456))
            .unwrap()
    );
    assert_eq!(
        buffer.rich_text.as_ref().unwrap().positioning,
        rich.positioning
    );
    let painted = buffer.rich_text.clone();
    assert!(
        buffer
            .format_selection(&base, &TextCharacterPatch::FontSize(30.0))
            .unwrap()
    );
    assert!(buffer.rich_text.as_ref().unwrap().positioning.is_none());
    buffer.history(false);
    assert_eq!(buffer.rich_text, painted);
    buffer.history(true);
    assert!(buffer.rich_text.as_ref().unwrap().positioning.is_none());
    buffer.history(false);
    assert!(buffer.reset_positioning());
    let generation = buffer.generation();
    assert!(!buffer.reset_positioning());
    assert_eq!(buffer.generation(), generation);
    buffer.history(false);
    assert_eq!(buffer.rich_text, painted);
    buffer.replace(Some(0..1), "犬", false, None).unwrap();
    assert!(buffer.rich_text.as_ref().unwrap().positioning.is_none());
    buffer.replace(Some(0..1), "猫", false, None).unwrap();
    assert_eq!(buffer.text, text);
    assert!(
        buffer.rich_text.as_ref().unwrap().positioning.is_none(),
        "typing the old text must not revive a stale cache"
    );
    buffer.history(false);
    buffer.history(false);
    assert_eq!(buffer.rich_text, painted);
}

#[test]
fn authored_spacing_invalid_commands_are_atomic_and_keep_redo() {
    let mut editor = editor("AB");
    editor
        .execute(Command::SetColor { id: 1, color: 0 })
        .unwrap();
    editor.undo();
    let before = editor.project().clone();
    let generation = editor.context_generation();
    let mut invalid = saved(&editor).clone();
    invalid.positioning.as_mut().unwrap().lines[0].font_sha256 = "bad".into();
    assert!(
        editor
            .execute(Command::SetRichText {
                id: 1,
                rich_text: Some(invalid)
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    assert_eq!(editor.context_generation(), generation);
    assert!(editor.can_redo());
    editor.redo();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .has_authored_text_positions()
    );
}

#[test]
fn authored_spacing_automation_and_expression_copies_preserve_authored_state() {
    let mut editor = editor("AB");
    let before = editor.project().clone();
    let mut draft = before.clone();
    draft
        .apply_automation_command(
            1,
            Command::EditSourceText {
                id: 1,
                frame: 0,
                text: "CD".into(),
            },
        )
        .unwrap();
    assert!(
        !draft
            .composition()
            .layer(1)
            .unwrap()
            .has_authored_text_positions()
    );
    assert!(editor.commit_automation_project(draft.clone()).unwrap());
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &draft);
    editor.undo();
    for (expression, expect_positions) in [("'AB'", true), ("'CD'", false)] {
        editor
            .execute(Command::SetExpression {
                id: 1,
                target: ExpressionTarget::SourceText,
                source: expression.into(),
                enabled: true,
            })
            .unwrap();
        let authored = editor.project().clone();
        let snapshot = authored.expression_snapshot(1, 0).unwrap();
        let roots = authored.expression_roots(1, 0, false).unwrap();
        let evaluated = expression_runtime::ExpressionEvaluator::default()
            .evaluate(&snapshot, &roots)
            .unwrap();
        let view = authored
            .with_evaluated_properties(1, 0, false, &evaluated)
            .unwrap();
        assert_eq!(
            view.composition()
                .layer(1)
                .unwrap()
                .has_authored_text_positions(),
            expect_positions
        );
        assert!(
            authored
                .composition()
                .layer(1)
                .unwrap()
                .has_authored_text_positions()
        );
        assert_eq!(editor.project(), &authored);
        assert!(view.to_json().is_err());
    }
}
