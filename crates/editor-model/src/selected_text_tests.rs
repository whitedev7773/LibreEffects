//! Synthetic native selected-character contracts, without installed fonts or AE data.
use libre_effects_core::{
    Command, Content, Editor, MAX_TEXT_STYLE_RUNS, Project, PropertyPath, RichText,
    TextCharacterPatch, TextCharacterStyle, TextFont, TextLeading, TextParam, TextStrokeJoin,
    TextStyle, TextStyleRun, TrackEdit, project_file,
};

fn style(color: u32) -> TextCharacterStyle {
    let mut value = TextCharacterStyle::from_style(&TextStyle::default(), 32.0, color);
    value.font_family = "Synthetic Family".into();
    value.font_face = "Synthetic-Regular".into();
    value.tracking = 25.0;
    value.stroke_enabled = true;
    value.stroke_width = 3.0;
    value.stroke_color = 0x123456;
    value.stroke_join = TextStrokeJoin::Round;
    value
}

fn run(start: usize, end: usize, style: &TextCharacterStyle) -> TextStyleRun {
    TextStyleRun {
        start,
        end,
        style: style.clone(),
    }
}

fn editor(text: &str) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: text.into(),
                font_size: 32.0,
            },
            width: 640.0,
            height: 360.0,
            name: "Synthetic selected source".into(),
        })
        .unwrap();
    editor
}

fn assert_rejected_unchanged(editor: &mut Editor, command: Command) {
    let before = editor.project().clone();
    let generation = editor.context_generation();
    let selection = editor.selected();
    let history = (editor.can_undo(), editor.can_redo());
    assert!(editor.execute(command).is_err());
    assert_eq!(editor.project(), &before);
    assert_eq!(editor.context_generation(), generation);
    assert_eq!(editor.selected(), selection);
    assert_eq!((editor.can_undo(), editor.can_redo()), history);
}

#[test]
fn selected_patch_splits_only_selection_and_preserves_unrelated_character_fields() {
    let text = "AβC\r\n한Z";
    let red = style(0xff0000);
    let mut blue = style(0x0000ff);
    blue.weight = 700;
    blue.italic = true;
    blue.font_face = "Synthetic-BoldItalic".into();
    blue.font_size = 48.0;
    blue.tracking = -10.0;
    blue.fill_enabled = false;
    let rich = RichText::new(
        text,
        red.clone(),
        vec![run(0, 4, &red), run(4, text.len(), &blue)],
    )
    .unwrap();
    for patch in [
        TextCharacterPatch::Family("Synthetic Other".into()),
        TextCharacterPatch::Font(TextFont {
            family: "Synthetic Other".into(),
            face: "SyntheticOther-Light".into(),
            weight: 300,
            italic: false,
        }),
        TextCharacterPatch::FontSize(64.0),
        TextCharacterPatch::Leading(Some(TextLeading::Fixed(72.0))),
        TextCharacterPatch::Tracking(125.0),
        TextCharacterPatch::FillColor(0x00ff00),
        TextCharacterPatch::FillEnabled(true),
        TextCharacterPatch::StrokeColor(0xff9900),
        TextCharacterPatch::StrokeEnabled(false),
        TextCharacterPatch::StrokeWidth(5.0),
        TextCharacterPatch::StrokeJoin(TextStrokeJoin::Bevel),
    ] {
        let changed = rich.format_range(text, &(1..9), &patch).unwrap();
        assert_eq!(changed.default_style, red);
        assert_eq!(changed.style_at(0), &red);
        assert_eq!(changed.style_at(9), &blue);
        for (at, original) in [(1, &red), (4, &blue)] {
            let mut expected = original.clone();
            match &patch {
                TextCharacterPatch::Family(family) => {
                    expected.font_family = family.clone();
                    expected.font_face.clear();
                }
                TextCharacterPatch::Font(font) => {
                    expected.font_family = font.family.clone();
                    expected.font_face = font.face.clone();
                    expected.weight = font.weight;
                    expected.italic = font.italic;
                }
                TextCharacterPatch::FontSize(value) => expected.font_size = *value,
                TextCharacterPatch::Leading(value) => expected.leading = *value,
                TextCharacterPatch::Tracking(value) => expected.tracking = *value,
                TextCharacterPatch::FillColor(value) => expected.fill_color = *value,
                TextCharacterPatch::FillEnabled(value) => expected.fill_enabled = *value,
                TextCharacterPatch::StrokeColor(value) => expected.stroke_color = *value,
                TextCharacterPatch::StrokeEnabled(value) => expected.stroke_enabled = *value,
                TextCharacterPatch::StrokeWidth(value) => expected.stroke_width = *value,
                TextCharacterPatch::StrokeJoin(value) => expected.stroke_join = *value,
            }
            assert_eq!(changed.style_at(at), &expected);
        }
        changed.validate(text).unwrap();
    }
    assert_eq!(rich.runs, [run(0, 4, &red), run(4, text.len(), &blue)]);
}

#[test]
fn selected_summary_reports_mixed_per_field_and_respects_exclusive_end() {
    let regular = style(0xff0000);
    let mut bold = regular.clone();
    bold.weight = 700;
    bold.font_face = "Synthetic-Bold".into();
    let mut other = bold.clone();
    other.font_family = "Synthetic Alternate".into();
    other.font_size = 64.0;
    other.fill_color = 0x0000ff;
    other.fill_enabled = false;
    let rich = RichText::new(
        "ABCD",
        regular.clone(),
        vec![run(0, 1, &regular), run(1, 3, &bold), run(3, 4, &other)],
    )
    .unwrap();
    let summary = rich.selection_style("ABCD", &(0..3)).unwrap();
    assert_eq!(summary.font_family, Some(regular.font_family.clone()));
    assert_eq!(summary.font, None);
    assert_eq!(summary.font_size, Some(32.0));
    assert_eq!(summary.fill_color, Some(0xff0000));
    assert_eq!(summary.fill_enabled, Some(true));
    assert_eq!(
        rich.selection_style("ABCD", &(1..3)).unwrap().font,
        Some(bold.font())
    );
    let all = rich.selection_style("ABCD", &(0..4)).unwrap();
    assert!(all.font_family.is_none() && all.font.is_none() && all.font_size.is_none());
    assert!(all.fill_color.is_none() && all.fill_enabled.is_none());
}

#[test]
fn selected_tracking_leading_and_stroke_summary_distinguishes_mixed_from_inherit() {
    let first = style(0xff0000);
    let mut second = first.clone();
    second.leading = Some(TextLeading::Auto(1.5));
    second.tracking = 150.0;
    second.stroke_color = 0x00ff00;
    second.stroke_enabled = false;
    second.stroke_width = 0.0;
    second.stroke_join = TextStrokeJoin::Bevel;
    let rich = RichText::new(
        "AB",
        first.clone(),
        vec![run(0, 1, &first), run(1, 2, &second)],
    )
    .unwrap();
    let only_first = rich.selection_style("AB", &(0..1)).unwrap();
    assert_eq!(only_first.leading, Some(None));
    assert_eq!(only_first.tracking, Some(25.0));
    assert_eq!(only_first.stroke_color, Some(0x123456));
    assert_eq!(only_first.stroke_enabled, Some(true));
    assert_eq!(only_first.stroke_width, Some(3.0));
    assert_eq!(only_first.stroke_join, Some(TextStrokeJoin::Round));
    let both = rich.selection_style("AB", &(0..2)).unwrap();
    assert_eq!(both.leading, None);
    assert_eq!(both.tracking, None);
    assert_eq!(both.stroke_color, None);
    assert_eq!(both.stroke_enabled, None);
    assert_eq!(both.stroke_width, None);
    assert_eq!(both.stroke_join, None);
    let inherited = rich
        .format_range("AB", &(0..2), &TextCharacterPatch::Leading(None))
        .unwrap();
    assert_eq!(
        inherited.selection_style("AB", &(0..2)).unwrap().leading,
        Some(None)
    );
    assert_eq!(inherited.style_at(1).stroke_join, TextStrokeJoin::Bevel);
}

#[test]
fn selected_line_leading_changes_baselines_and_roundtrips_as_one_editor_edit() {
    let source = "A\r\nB";
    let mut editor = editor(source);
    let original = editor.project().clone();
    let base = editor
        .selected_layer()
        .unwrap()
        .base_character_style()
        .unwrap();
    let rich = RichText::new(source, base.clone(), vec![run(0, source.len(), &base)]).unwrap();
    let changed = rich
        .format_range(
            source,
            &(3..4),
            &TextCharacterPatch::Leading(Some(TextLeading::Fixed(72.0))),
        )
        .unwrap()
        .format_range(source, &(3..4), &TextCharacterPatch::Tracking(200.0))
        .unwrap()
        .format_range(source, &(3..4), &TextCharacterPatch::StrokeWidth(6.0))
        .unwrap();
    let lines = changed.line_metrics(source, &TextStyle::default()).unwrap();
    assert_eq!(lines[1].baseline - lines[0].baseline, 72.0);
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(changed.clone()),
        })
        .unwrap();
    let expected = editor.project().clone();
    let bytes = project_file::encode(&expected, None).unwrap();
    assert_eq!(project_file::decode(&bytes).unwrap().project, expected);
    editor.undo();
    assert_eq!(editor.project(), &original);
    editor.redo();
    assert_eq!(editor.project(), &expected);
    let generation = editor.context_generation();
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(changed),
        })
        .unwrap();
    assert_eq!(editor.context_generation(), generation);
}

#[test]
fn selected_same_family_preserves_exact_faces_and_local_redo() {
    let regular = style(0xff0000);
    let mut bold = regular.clone();
    bold.font_face = "Synthetic-BoldItalic".into();
    bold.weight = 700;
    bold.italic = true;
    let rich = RichText::new(
        "AB",
        regular.clone(),
        vec![run(0, 1, &regular), run(1, 2, &bold)],
    )
    .unwrap();
    let mut buffer = crate::text_buffer::Buffer::new("AB".into());
    buffer.rich_text = Some(rich.clone());
    buffer.all();
    buffer
        .format_selection(&regular, &TextCharacterPatch::FontSize(64.0))
        .unwrap();
    let resized = buffer.rich_text.clone();
    buffer.history(false);
    let generation = buffer.generation();
    assert!(
        !buffer
            .format_selection(
                &regular,
                &TextCharacterPatch::Family(regular.font_family.clone()),
            )
            .unwrap()
    );
    assert_eq!(buffer.rich_text, Some(rich));
    assert_eq!(buffer.generation(), generation);
    assert_eq!((buffer.anchor, buffer.caret), (0, 2));
    buffer.history(true);
    assert_eq!(buffer.rich_text, resized);
    assert_eq!(
        buffer.rich_text.as_ref().unwrap().style_at(0).font(),
        regular.font()
    );
    assert_eq!(
        buffer.rich_text.as_ref().unwrap().style_at(1).font(),
        bold.font()
    );
}

#[test]
fn selected_family_clears_exact_face_only_on_runs_that_change_family() {
    let regular = style(0xff0000);
    let mut other = regular.clone();
    other.font_family = "Synthetic Other".into();
    other.font_face = "SyntheticOther-BoldItalic".into();
    other.weight = 700;
    other.italic = true;
    let rich = RichText::new(
        "AB",
        regular.clone(),
        vec![run(0, 1, &regular), run(1, 2, &other)],
    )
    .unwrap();
    let changed = rich
        .format_range(
            "AB",
            &(0..2),
            &TextCharacterPatch::Family(regular.font_family.clone()),
        )
        .unwrap();
    assert_eq!(changed.style_at(0), &regular);
    other.font_family = regular.font_family.clone();
    other.font_face.clear();
    assert_eq!(changed.style_at(1), &other);
    assert_eq!(changed.default_style, regular);
}

#[test]
fn selected_format_rejects_scalar_and_grapheme_splits_even_for_noop() {
    let base = style(0xff0000);
    for (text, split) in [
        ("e\u{301}", 1),
        ("👩‍💻", 4),
        ("👍🏽", 4),
        ("🇰🇷", 4),
        ("\r\n", 1),
        ("😀", 2),
    ] {
        let rich = RichText::new(text, base.clone(), vec![run(0, text.len(), &base)]).unwrap();
        for range in [
            0..split,
            split..text.len(),
            0..0,
            text.len()..text.len(),
            0..text.len() + 1,
            usize::MAX..usize::MAX,
        ] {
            assert!(
                rich.format_range(
                    text,
                    &range,
                    &TextCharacterPatch::FillColor(base.fill_color)
                )
                .is_err(),
                "{text:?} {range:?}"
            );
            assert!(rich.selection_style(text, &range).is_err());
        }
        assert_eq!(
            rich.format_range(
                text,
                &(0..text.len()),
                &TextCharacterPatch::FillColor(base.fill_color)
            )
            .unwrap(),
            rich
        );
        assert!(
            rich.format_range(text, &(0..text.len()), &TextCharacterPatch::FillColor(0))
                .is_ok()
        );
    }
}

#[test]
fn selected_format_rejects_invalid_attribute_values_without_mutation() {
    let base = style(0);
    let rich = RichText::new("AB", base.clone(), vec![run(0, 2, &base)]).unwrap();
    let before = rich.clone();
    for patch in [
        TextCharacterPatch::Family("".into()),
        TextCharacterPatch::Family("Bad\nFamily".into()),
        TextCharacterPatch::Family("F".repeat(257)),
        TextCharacterPatch::Font(TextFont {
            family: "Synthetic".into(),
            face: "Bad\nFace".into(),
            weight: 400,
            italic: false,
        }),
        TextCharacterPatch::Font(TextFont {
            family: "Synthetic".into(),
            face: "Synthetic-Regular".into(),
            weight: 0,
            italic: false,
        }),
        TextCharacterPatch::FontSize(f64::NAN),
        TextCharacterPatch::FontSize(f64::INFINITY),
        TextCharacterPatch::FontSize(0.0),
        TextCharacterPatch::FontSize(2049.0),
        TextCharacterPatch::FillColor(0x1000000),
        TextCharacterPatch::Leading(Some(TextLeading::Auto(f64::NAN))),
        TextCharacterPatch::Leading(Some(TextLeading::Auto(0.0))),
        TextCharacterPatch::Leading(Some(TextLeading::Fixed(20481.0))),
        TextCharacterPatch::Tracking(f64::INFINITY),
        TextCharacterPatch::Tracking(-1001.0),
        TextCharacterPatch::Tracking(10001.0),
        TextCharacterPatch::StrokeColor(0x1000000),
        TextCharacterPatch::StrokeWidth(-1.0),
        TextCharacterPatch::StrokeWidth(f64::NAN),
        TextCharacterPatch::StrokeWidth(1001.0),
    ] {
        assert!(rich.format_range("AB", &(0..1), &patch).is_err());
        assert_eq!(rich, before);
    }
    for size in [1.0, 2048.0] {
        assert_eq!(
            rich.format_range("AB", &(0..1), &TextCharacterPatch::FontSize(size))
                .unwrap()
                .style_at(0)
                .font_size,
            size
        );
    }
}

#[test]
fn selected_format_checks_run_limit_after_merging() {
    let text = "AB".repeat(MAX_TEXT_STYLE_RUNS);
    let red = style(0xff0000);
    let blue = style(0x0000ff);
    let runs = (0..MAX_TEXT_STYLE_RUNS)
        .map(|i| run(i * 2, i * 2 + 2, if i % 2 == 0 { &red } else { &blue }))
        .collect();
    let rich = RichText::new(&text, red.clone(), runs).unwrap();
    assert!(
        rich.format_range(&text, &(1..2), &TextCharacterPatch::FillColor(0x00ff00))
            .is_err()
    );
    assert_eq!(
        rich.format_range(&text, &(1..3), &TextCharacterPatch::FillColor(0xff0000))
            .unwrap()
            .runs
            .len(),
        MAX_TEXT_STYLE_RUNS
    );
    assert_eq!(
        rich.format_range(&text, &(1..2), &TextCharacterPatch::FillColor(0xff0000))
            .unwrap(),
        rich
    );
    let uniform = rich
        .format_range(
            &text,
            &(0..text.len()),
            &TextCharacterPatch::FillColor(0xff0000),
        )
        .unwrap();
    assert_eq!(uniform.runs, [run(0, text.len(), &red)]);
}

#[test]
fn exact_styled_source_commits_valid_final_grapheme_after_prior_style_change() {
    let source = "aX\u{301}\r\n끝";
    let mut editor = editor(source);
    let red = style(0xff0000);
    let blue = style(0x0000ff);
    let original = RichText::new(
        source,
        red.clone(),
        vec![run(0, 1, &red), run(1, source.len(), &blue)],
    )
    .unwrap();
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(original.clone()),
        })
        .unwrap();
    let before = editor.project().clone();
    let styled = original
        .format_range(source, &(0..1), &TextCharacterPatch::FillColor(0x0000ff))
        .unwrap();
    let (text, rich_text) = styled.replace_range(source, 1..2, "").unwrap();
    assert_eq!(text, "a\u{301}\r\n끝");
    // The previous two-command path fails while trying to join differently
    // styled scalars. The exact transaction validates only the final draft.
    assert_rejected_unchanged(
        &mut editor,
        Command::Batch(vec![
            Command::EditSourceText {
                id: 1,
                frame: 0,
                text: text.clone(),
            },
            Command::SetRichText {
                id: 1,
                rich_text: Some(rich_text.clone()),
            },
        ]),
    );
    editor.clear_history();
    editor
        .execute(Command::SetStyledText {
            id: 1,
            text: text.clone(),
            rich_text: rich_text.clone(),
        })
        .unwrap();
    let after = editor.project().clone();
    let layer = after.composition().layer(1).unwrap();
    assert_eq!(layer.source_text_at(0), Some(text.as_str()));
    assert_eq!(layer.rich_text(), Some(&rich_text));
    let mut expected_layer = serde_json::to_value(before.composition().layer(1).unwrap()).unwrap();
    expected_layer["content"]["Text"]["text"] = text.into();
    expected_layer["rich_text"] = serde_json::to_value(rich_text).unwrap();
    assert_eq!(serde_json::to_value(layer).unwrap(), expected_layer);
    editor.undo();
    assert_eq!(editor.project(), &before);
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(editor.project(), &after);
    let bytes = project_file::encode(&after, None).unwrap();
    assert_eq!(project_file::decode(&bytes).unwrap().project, after);
    assert_eq!(
        Project::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
}

#[test]
fn exact_styled_source_noop_and_invalid_payload_preserve_history_and_project() {
    let mut editor = editor("AB");
    let base = style(0);
    let rich = RichText::new("AB", base.clone(), vec![run(0, 2, &base)]).unwrap();
    editor
        .execute(Command::SetStyledText {
            id: 1,
            text: "AB".into(),
            rich_text: rich.clone(),
        })
        .unwrap();
    let styled = editor.project().clone();
    editor
        .execute(Command::SetColor {
            id: 1,
            color: 0x123456,
        })
        .unwrap();
    editor.undo();
    let generation = editor.context_generation();
    editor
        .execute(Command::SetStyledText {
            id: 1,
            text: "AB".into(),
            rich_text: rich.clone(),
        })
        .unwrap();
    assert_eq!(editor.project(), &styled);
    assert_eq!(editor.context_generation(), generation);
    assert!(editor.can_redo());
    assert_rejected_unchanged(
        &mut editor,
        Command::SetStyledText {
            id: 1,
            text: "ABCDE".into(),
            rich_text: rich.clone(),
        },
    );
    let malformed = RichText {
        point_origin: false,
        proportional_metrics: false,
        positioning: None,
        default_style: base.clone(),
        runs: vec![run(0, 1, &base), run(1, 2, &style(1))],
    };
    assert_rejected_unchanged(
        &mut editor,
        Command::SetStyledText {
            id: 1,
            text: "\r\n".into(),
            rich_text: malformed,
        },
    );
    editor.execute(Command::ToggleLocked(1)).unwrap();
    assert_rejected_unchanged(
        &mut editor,
        Command::SetStyledText {
            id: 1,
            text: "AB".into(),
            rich_text: rich,
        },
    );
}

#[test]
fn exact_styled_source_reuses_plain_layer_eligibility_gate() {
    let base = style(0);
    let rich = RichText::new("AB", base.clone(), vec![run(0, 2, &base)]).unwrap();
    for disqualify in [
        Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
        Command::EditText {
            id: 1,
            parameter: TextParam::FontSize,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
        Command::AddTextAnimator { id: 1 },
        Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                paragraph: true,
                ..TextStyle::default()
            },
        },
        Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        },
    ] {
        let mut editor = editor("AB");
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .rich_text_eligibility()
                .is_ok()
        );
        editor.execute(disqualify).unwrap();
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .rich_text_eligibility()
                .is_err()
        );
        assert_rejected_unchanged(
            &mut editor,
            Command::SetStyledText {
                id: 1,
                text: "AB".into(),
                rich_text: rich.clone(),
            },
        );
    }
}
