//! Synthetic source-model coverage; no Adobe documents or compatibility oracle.
use libre_effects_core::{
    Command, Content, Editor, MAX_TEXT_STYLE_RUNS, Project, PropertyPath, RichText,
    TextCharacterStyle, TextFont, TextParam, TextStrokeJoin, TextStyle, TextStyleRun, TrackEdit,
    project_file,
};

fn style(color: u32) -> TextCharacterStyle {
    TextCharacterStyle::from_style(&TextStyle::default(), 32.0, color)
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
            name: "Synthetic rich source".into(),
        })
        .unwrap();
    editor
}
fn rich_editor(text: &str, split: usize) -> Editor {
    let mut editor = editor(text);
    let mut red = style(0xff0000);
    red.font_face = "Synthetic-Regular".into();
    let mut blue = style(0x0000ff);
    blue.font_family = "Synthetic Alternate".into();
    blue.font_face = "SyntheticAlt-Italic".into();
    blue.font_size = 48.0;
    blue.italic = true;
    let runs = if text.is_empty() {
        vec![]
    } else if split == 0 || split == text.len() {
        vec![run(0, text.len(), &red)]
    } else {
        vec![run(0, split, &red), run(split, text.len(), &blue)]
    };
    let rich = RichText::new(text, red, runs).unwrap();
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(rich),
        })
        .unwrap();
    editor
}
fn rich(editor: &Editor) -> &RichText {
    editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .rich_text()
        .unwrap()
}
fn source(editor: &Editor) -> &str {
    editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .source_text_at(0)
        .unwrap()
}
fn version(project: &Project) -> u32 {
    serde_json::to_value(project).unwrap()["version"]
        .as_u64()
        .unwrap() as u32
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
fn rich_text_absence_keeps_legacy_json_and_native_bytes() {
    let mut editor = editor("A\rB\r\n한\n😀");
    let json = editor.project().to_json().unwrap();
    let native = project_file::encode(editor.project(), None).unwrap();
    let before = editor.project().clone();
    let generation = editor.context_generation();
    assert!(!json.contains("rich_text"));
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: None,
        })
        .unwrap();
    assert_eq!(editor.project(), &before);
    assert_eq!(editor.context_generation(), generation);
    assert_eq!(editor.project().to_json().unwrap(), json);
    assert_eq!(
        project_file::encode(editor.project(), None).unwrap(),
        native
    );
    assert_eq!(Project::from_json(&json).unwrap().to_json().unwrap(), json);
}

#[test]
fn rich_text_canonicalizes_only_adjacent_equal_styles() {
    let red = style(0xff0000);
    let blue = style(0x0000ff);
    let rich = RichText::new(
        "ABCD",
        red.clone(),
        vec![
            run(0, 1, &red),
            run(1, 2, &red),
            run(2, 3, &blue),
            run(3, 4, &red),
        ],
    )
    .unwrap();
    assert_eq!(
        rich.runs,
        [run(0, 2, &red), run(2, 3, &blue), run(3, 4, &red)]
    );
    assert_eq!(rich.style_at(2), &blue);
    assert_eq!(rich.style_at(4), &red);
    let noncanonical = RichText {
        point_origin: false,
        positioning: None,
        default_style: red.clone(),
        runs: vec![run(0, 1, &red), run(1, 2, &red)],
    };
    assert!(noncanonical.validate("AB").is_err());
}

#[test]
fn rich_text_validates_exact_utf8_coverage_and_limits() {
    let red = style(0xff0000);
    for runs in [
        vec![],
        vec![run(1, 6, &red)],
        vec![run(0, 5, &red)],
        vec![run(0, 7, &red)],
        vec![run(0, 2, &red), run(2, 6, &red)],
        vec![run(0, 5, &red), run(4, 6, &red)],
        vec![run(0, 1, &red), run(5, 6, &red)],
        vec![run(0, 0, &red), run(0, 6, &red)],
        vec![run(5, 6, &red), run(0, 5, &red)],
    ] {
        assert!(RichText::new("A😀B", red.clone(), runs).is_err());
    }
    assert!(RichText::new("", red.clone(), vec![run(0, 0, &red)]).is_err());
    assert!(
        RichText::new(
            "A",
            red.clone(),
            vec![run(0, 1, &red); MAX_TEXT_STYLE_RUNS + 1]
        )
        .is_err()
    );
    let oversized = "A".repeat(16_385);
    assert!(RichText::new(&oversized, red.clone(), vec![run(0, oversized.len(), &red)]).is_err());
    let maximum = "A".repeat(16_384);
    assert!(RichText::new(&maximum, red.clone(), vec![run(0, maximum.len(), &red)]).is_ok());
    assert!(
        RichText::new(
            "A😀B",
            red.clone(),
            vec![run(0, 1, &red), run(1, 5, &style(0)), run(5, 6, &red)]
        )
        .is_ok()
    );
}

#[test]
fn rich_text_rejects_nonfinite_bounds_unsupported_fields_and_paint_order() {
    let base = style(0xffffff);
    let mut bad = vec![];
    for value in [f64::NAN, f64::INFINITY, -1.0, 0.0, 2049.0] {
        let mut s = base.clone();
        s.font_size = value;
        bad.push(s);
    }
    for value in [f64::NAN, f64::INFINITY, -1001.0, 10001.0] {
        let mut s = base.clone();
        s.tracking = value;
        bad.push(s);
    }
    for value in [f64::NAN, f64::INFINITY, -1.0, 1001.0] {
        let mut s = base.clone();
        s.stroke_width = value;
        bad.push(s);
    }
    for weight in [0, 1001] {
        let mut s = base.clone();
        s.weight = weight;
        bad.push(s);
    }
    for family in ["", "\t", "Font\0Name"] {
        let mut s = base.clone();
        s.font_family = family.into();
        bad.push(s);
    }
    let mut s = base.clone();
    s.font_face = "Bad\nFace".into();
    bad.push(s);
    let mut s = base.clone();
    s.font_face = "F".repeat(257);
    bad.push(s);
    let mut s = base.clone();
    s.fill_color = 0x1000000;
    bad.push(s);
    let mut s = base.clone();
    s.stroke_color = 0x1000000;
    bad.push(s);
    for s in bad {
        assert!(!s.valid());
        assert!(RichText::new("A", base.clone(), vec![run(0, 1, &s)]).is_err());
    }
    let mut ordered = base.clone();
    ordered.stroke_over_fill = true;
    assert!(RichText::new("A", base.clone(), vec![run(0, 1, &ordered)]).is_err());
    let mut json = serde_json::to_value(base).unwrap();
    json["baseline_shift"] = 1.into();
    assert!(serde_json::from_value::<TextCharacterStyle>(json).is_err());
}

#[test]
fn rich_text_utf16_adapter_preserves_crlf_and_rejects_surrogate_splits() {
    let text = "A😀한\r\nZ";
    let red = style(0xff0000);
    let blue = style(0xff);
    let rich =
        RichText::from_utf16_runs(text, red.clone(), vec![run(0, 3, &red), run(3, 7, &blue)])
            .unwrap();
    assert_eq!(rich.runs, [run(0, 5, &red), run(5, 11, &blue)]);
    assert_eq!(&text[rich.runs[1].start..rich.runs[1].end], "한\r\nZ");
    assert!(
        RichText::from_utf16_runs(text, red.clone(), vec![run(0, 2, &red), run(2, 7, &blue)])
            .is_err()
    );
    assert!(RichText::from_utf16_runs(text, red.clone(), vec![run(0, 8, &red)]).is_err());
    assert!(RichText::from_utf16_runs(text, red.clone(), vec![run(0, usize::MAX, &red)]).is_err());
}

#[test]
fn rich_text_native_range_replacement_inherits_and_preserves_suffix_styles() {
    let editor = rich_editor("A😀B\r\n한\n끝\r", 5);
    let original = rich(&editor).clone();
    let red = original.runs[0].style.clone();
    let blue = original.runs[1].style.clone();
    let (text, changed) = original
        .replace_range(source(&editor), 1..5, "你好")
        .unwrap();
    assert_eq!(text, "A你好B\r\n한\n끝\r");
    assert_eq!(changed.runs, [run(0, 7, &red), run(7, text.len(), &blue)]);
    let (text, changed) = original.replace_range(source(&editor), 5..5, "Z").unwrap();
    assert_eq!(text, "A😀ZB\r\n한\n끝\r");
    assert_eq!(changed.runs[0], run(0, 6, &red));
    let (text, changed) = original.replace_range(source(&editor), 0..0, "한").unwrap();
    assert!(text.starts_with("한A😀"));
    assert_eq!(changed.runs[0], run(0, 8, &red));
    let (text, changed) = original
        .replace_range(
            source(&editor),
            source(&editor).len()..source(&editor).len(),
            "Z",
        )
        .unwrap();
    assert_eq!(changed.runs.last().unwrap().style, blue);
    assert_eq!(changed.runs.last().unwrap().end, text.len());
    assert_eq!(rich(&editor), &original);
}

#[test]
fn rich_text_deleting_everything_retains_style_for_retyping() {
    let editor = rich_editor("AB", 1);
    let original = rich(&editor);
    let (text, empty) = original.replace_range("AB", 0..2, "").unwrap();
    assert_eq!(text, "");
    assert!(empty.runs.is_empty());
    assert_eq!(empty.default_style, original.runs[0].style);
    let (text, filled) = empty.replace_range("", 0..0, "😀\r\n한").unwrap();
    assert_eq!(filled.runs, [run(0, text.len(), &empty.default_style)]);
}

#[test]
fn rich_text_range_noops_and_invalid_edits_preserve_exact_data() {
    let editor = rich_editor("A😀B", 5);
    let value = rich(&editor);
    assert_eq!(
        value.replace_range("A😀B", 1..5, "😀").unwrap(),
        ("A😀B".into(), value.clone())
    );
    for range in [2..5, 1..4, 5..1, 0..7, usize::MAX..usize::MAX] {
        assert!(value.replace_range("A😀B", range, "X").is_err());
    }
    assert!(
        value
            .replace_range("A😀B", 0..0, &"X".repeat(16_384))
            .is_err()
    );
    assert!(value.replace_range("stale", 0..0, "X").is_err());
}

#[test]
fn rich_text_commands_are_atomic_and_preserve_exact_line_endings() {
    let mut editor = rich_editor("A😀B\r\n한\n끝\r", 5);
    editor.clear_history();
    let before = editor.project().clone();
    editor
        .execute(Command::ReplaceTextRange {
            id: 1,
            start: 1,
            end: 5,
            text: "你好".into(),
        })
        .unwrap();
    let after = editor.project().clone();
    assert_eq!(source(&editor), "A你好B\r\n한\n끝\r");
    editor.undo();
    assert_eq!(editor.project(), &before);
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(editor.project(), &after);
    assert_rejected_unchanged(
        &mut editor,
        Command::ReplaceTextRange {
            id: 1,
            start: 2,
            end: 4,
            text: "X".into(),
        },
    );
}

#[test]
fn rich_text_whole_source_edit_preserves_common_prefix_suffix_and_undo() {
    let mut editor = rich_editor("A😀B\r\n한\n끝\r", 5);
    let before = editor.project().clone();
    let blue = rich(&editor).runs[1].style.clone();
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 0,
            text: "A你好B\r\n한\n끝\r".into(),
        })
        .unwrap();
    assert_eq!(rich(&editor).runs[1], run(7, source(&editor).len(), &blue));
    let after = editor.project().clone();
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
}

#[test]
fn rich_text_source_and_explicit_draft_style_commit_as_one_batch() {
    let mut editor = rich_editor("AB", 1);
    editor.clear_history();
    let before = editor.project().clone();
    let (text, draft) = rich(&editor).replace_range("AB", 1..1, "한").unwrap();
    editor
        .execute(Command::Batch(vec![
            Command::EditSourceText {
                id: 1,
                frame: 0,
                text: text.clone(),
            },
            Command::SetRichText {
                id: 1,
                rich_text: Some(draft.clone()),
            },
        ]))
        .unwrap();
    assert_eq!(source(&editor), text);
    assert_eq!(rich(&editor), &draft);
    editor.undo();
    assert_eq!(editor.project(), &before);
    assert!(!editor.can_undo());
    let malformed = RichText {
        point_origin: false,
        positioning: None,
        default_style: style(0),
        runs: vec![run(0, 1, &style(0))],
    };
    assert_rejected_unchanged(
        &mut editor,
        Command::Batch(vec![
            Command::EditSourceText {
                id: 1,
                frame: 0,
                text: "changed".into(),
            },
            Command::SetRichText {
                id: 1,
                rich_text: Some(malformed),
            },
        ]),
    );
    assert!(editor.can_redo());
}

#[test]
fn rich_text_native_roundtrip_duplication_and_schema_floor() {
    let mut editor = rich_editor("e\u{301}👩‍💻\r\n日本語\r한국어\n", 3);
    assert_eq!(version(editor.project()), 71);
    let before = editor.project().clone();
    let json = before.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), before);
    let native = project_file::encode(&before, Some(b"{\"synthetic\":true}")).unwrap();
    let decoded = project_file::decode(&native).unwrap();
    assert_eq!(decoded.project, before);
    assert_eq!(decoded.view, Some(b"{\"synthetic\":true}".as_slice()));
    editor.execute(Command::DuplicateLayer(1)).unwrap();
    let copy = editor.selected_layer().unwrap();
    assert_eq!(
        copy.rich_text(),
        before.composition().layer(1).unwrap().rich_text()
    );
    assert_eq!(
        copy.source_text_at(0),
        before.composition().layer(1).unwrap().source_text_at(0)
    );
    let mut historical = serde_json::to_value(&before).unwrap();
    historical["version"] = 65.into();
    assert!(Project::from_json(&historical.to_string()).is_err());
    let mut invalid = serde_json::to_value(&before).unwrap();
    invalid["composition"]["layers"][0]["rich_text"]["runs"][0]["end"] = 2.into();
    assert!(Project::from_json(&invalid.to_string()).is_err());
}

#[test]
fn rich_text_whole_style_size_and_color_changes_update_character_runs() {
    let mut editor = rich_editor("AB", 1);
    let mut text_style = editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .text_style();
    text_style.stroke_enabled = true;
    text_style.stroke_color = 0x00ff00;
    text_style.stroke_width = 3.0;
    text_style.stroke_join = TextStrokeJoin::Round;
    text_style.stroke_over_fill = true;
    text_style.tracking = 25.0;
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: text_style,
        })
        .unwrap();
    for s in std::iter::once(&rich(&editor).default_style)
        .chain(rich(&editor).runs.iter().map(|r| &r.style))
    {
        assert!(s.stroke_enabled && s.stroke_over_fill);
        assert_eq!(s.stroke_color, 0x00ff00);
        assert_eq!(s.stroke_width, 3.0);
        assert_eq!(s.stroke_join, TextStrokeJoin::Round);
        assert_eq!(s.tracking, 25.0);
    }
    assert_ne!(
        rich(&editor).runs[0].style.font_family,
        rich(&editor).runs[1].style.font_family
    );
    editor
        .execute(Command::SetContent {
            id: 1,
            content: Content::Text {
                text: "A한B".into(),
                font_size: 64.0,
            },
        })
        .unwrap();
    assert_eq!(source(&editor), "A한B");
    assert!(rich(&editor).runs.iter().all(|r| r.style.font_size == 64.0));
    editor
        .execute(Command::SetColor {
            id: 1,
            color: 0xaabbcc,
        })
        .unwrap();
    assert!(
        rich(&editor)
            .runs
            .iter()
            .all(|r| r.style.fill_color == 0xaabbcc)
    );
}

#[test]
fn rich_text_font_replacement_finds_runs_even_when_base_font_differs() {
    let mut editor = rich_editor("AB", 1);
    let original = rich(&editor).clone();
    let from = original.runs[1].style.font();
    let to = TextFont {
        family: "Replacement".into(),
        face: "Replacement-Bold".into(),
        weight: 700,
        italic: false,
    };
    editor
        .execute(Command::ReplaceTextFont {
            from,
            to: to.clone(),
        })
        .unwrap();
    assert_eq!(rich(&editor).runs[0], original.runs[0]);
    assert_eq!(rich(&editor).runs[1].style.font(), to);
    assert_eq!(
        rich(&editor).runs[1].style.fill_color,
        original.runs[1].style.fill_color
    );
    assert_eq!(
        rich(&editor).runs[1].style.font_size,
        original.runs[1].style.font_size
    );
}

#[test]
fn rich_text_rejects_unqualified_animation_paragraph_and_content_conversion() {
    let mut editor = rich_editor("AB", 1);
    editor.clear_history();
    assert_rejected_unchanged(
        &mut editor,
        Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
    for parameter in [
        TextParam::FontSize,
        TextParam::FillRed,
        TextParam::Tracking,
        TextParam::AnimatorPositionX,
        TextParam::FillOpacity,
    ] {
        assert_rejected_unchanged(
            &mut editor,
            Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        );
    }
    assert_rejected_unchanged(&mut editor, Command::AddTextAnimator { id: 1 });
    assert_rejected_unchanged(
        &mut editor,
        Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        },
    );
    let mut style = editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .text_style();
    style.paragraph = true;
    assert_rejected_unchanged(&mut editor, Command::SetTextStyle { id: 1, style });
    let payload = rich(&editor).clone();
    let mut animated = super::rich_text_tests::editor("AB");
    animated
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    assert_rejected_unchanged(
        &mut animated,
        Command::SetRichText {
            id: 1,
            rich_text: Some(payload),
        },
    );
}

#[test]
fn rich_text_dormant_paragraph_settings_are_retained() {
    let mut editor = rich_editor("AB", 1);
    let mut style = editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .text_style();
    style.paragraph_first_line_indent = -25.0;
    style.paragraph_space_after = 30.0;
    style.leading = 1.8;
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: style.clone(),
        })
        .unwrap();
    assert_eq!(
        editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .text_style(),
        style
    );
}

#[test]
fn rich_text_locked_changes_and_automation_rollback_are_atomic() {
    let mut editor = rich_editor("AB", 1);
    let payload = rich(&editor).clone();
    editor.execute(Command::ToggleLocked(1)).unwrap();
    assert_rejected_unchanged(
        &mut editor,
        Command::SetRichText {
            id: 1,
            rich_text: None,
        },
    );
    assert_rejected_unchanged(
        &mut editor,
        Command::ReplaceTextRange {
            id: 1,
            start: 0,
            end: 1,
            text: "X".into(),
        },
    );
    let mut project = super::rich_text_tests::editor("AB").project().clone();
    let before = project.clone();
    let composition = project.active_composition_id();
    project
        .apply_automation_command(
            composition,
            Command::SetRichText {
                id: 1,
                rich_text: Some(payload),
            },
        )
        .unwrap();
    assert_eq!(version(&project), 71);
    let styled = project.clone();
    assert!(
        project
            .apply_automation_command(
                composition,
                Command::ReplaceTextRange {
                    id: 1,
                    start: 0,
                    end: 3,
                    text: "X".into()
                }
            )
            .is_err()
    );
    assert_eq!(project, styled);
    let mut target = Editor::default();
    target.replace_project(before).unwrap();
    target.clear_history();
    target.commit_automation_project(project.clone()).unwrap();
    assert_eq!(target.project(), &project);
    target.undo();
    assert!(!target.can_undo());
}

#[test]
fn rich_text_run_count_growth_is_bounded_after_canonicalization() {
    let text = "A".repeat(MAX_TEXT_STYLE_RUNS);
    let red = style(0xff0000);
    let blue = style(0xff);
    let runs = (0..MAX_TEXT_STYLE_RUNS)
        .map(|i| run(i, i + 1, if i % 2 == 0 { &red } else { &blue }))
        .collect();
    let value = RichText::new(&text, red, runs).unwrap();
    assert_eq!(value.runs.len(), MAX_TEXT_STYLE_RUNS);
    // An insertion at a boundary inherits the left run and does not add a run.
    let (_, changed) = value.replace_range(&text, 1..1, "B").unwrap();
    assert_eq!(changed.runs.len(), MAX_TEXT_STYLE_RUNS);
}

#[test]
fn rich_text_rejects_grapheme_and_crlf_style_splits_after_canonicalization() {
    let red = style(0xff0000);
    let blue = style(0xff);
    for (text, split, split_utf16) in [
        ("e\u{301}", 1, 1),
        ("👩‍💻", 4, 2),
        ("\r\n", 1, 1),
        ("👍🏽", 4, 2),
        ("🇰🇷", 4, 2),
    ] {
        let different = vec![run(0, split, &red), run(split, text.len(), &blue)];
        assert!(
            RichText::new(text, red.clone(), different.clone()).is_err(),
            "{text:?}"
        );
        let persisted = RichText {
            point_origin: false,
            positioning: None,
            default_style: red.clone(),
            runs: different,
        };
        assert!(persisted.validate(text).is_err(), "{text:?}");
        let utf16_len = text.encode_utf16().count();
        assert!(
            RichText::from_utf16_runs(
                text,
                red.clone(),
                vec![
                    run(0, split_utf16, &red),
                    run(split_utf16, utf16_len, &blue)
                ]
            )
            .is_err(),
            "{text:?}"
        );
        // These scalar-level source segments have identical complete styles,
        // so canonicalization removes the unsupported boundary before admission.
        let merged = RichText::new(
            text,
            red.clone(),
            vec![run(0, split, &red), run(split, text.len(), &red)],
        )
        .unwrap();
        assert_eq!(merged.runs, [run(0, text.len(), &red)]);
        assert_eq!(
            RichText::from_utf16_runs(
                text,
                red.clone(),
                vec![run(0, split_utf16, &red), run(split_utf16, utf16_len, &red)]
            )
            .unwrap(),
            merged
        );
    }
}

#[test]
fn rich_text_replacement_merges_safe_combining_insertions_and_rejects_cross_style_clusters() {
    let editor = rich_editor("eB", 1);
    let original = rich(&editor);
    let (source, inserted) = original.replace_range("eB", 1..1, "\u{301}").unwrap();
    assert_eq!(source, "e\u{301}B");
    assert_eq!(inserted.runs[0], run(0, 3, &original.runs[0].style));
    assert_eq!(inserted.runs[1], run(3, 4, &original.runs[1].style));

    for (source, split, range, replacement) in [
        ("Ae\u{301}", 1, 1..2, ""),
        ("👩💻", 4, 4..4, "\u{200d}"),
        ("A\n", 1, 1..1, "\r"),
    ] {
        let editor = rich_editor(source, split);
        let original = rich(&editor).clone();
        assert!(
            original.replace_range(source, range, replacement).is_err(),
            "{source:?}"
        );
        assert_eq!(rich(&editor), &original);
    }
}

#[test]
fn rich_text_rejected_cluster_edits_preserve_buffer_ime_and_undo_redo() {
    use crate::text_buffer::Buffer;
    let editor = rich_editor("Ae\u{301}", 1);
    let mut buffer = Buffer::new("Ae\u{301}".into());
    buffer.rich_text = Some(rich(&editor).clone());
    let original_style = buffer.rich_text.clone();
    buffer.anchor = 0;
    buffer.caret = 0;
    buffer.replace(None, "Z", false, None).unwrap();
    buffer.history(false);
    assert_eq!(buffer.text, "Ae\u{301}");
    assert_eq!(buffer.rich_text, original_style);
    // The rejected IME-style replacement would join differently styled A and
    // the surviving combining mark. All draft state and the redo entry survive.
    buffer.anchor = 1;
    buffer.caret = 2;
    buffer.marked = Some(1..2);
    let before = (
        buffer.text.clone(),
        buffer.rich_text.clone(),
        buffer.anchor,
        buffer.caret,
        buffer.marked.clone(),
    );
    assert!(buffer.replace(None, "", true, None).is_err());
    assert_eq!(
        (
            buffer.text.clone(),
            buffer.rich_text.clone(),
            buffer.anchor,
            buffer.caret,
            buffer.marked.clone()
        ),
        before
    );
    buffer.history(true);
    assert_eq!(buffer.text, "ZAe\u{301}");
    buffer.history(false);
    assert_eq!(buffer.text, "Ae\u{301}");
    assert_eq!(buffer.rich_text, original_style);
    // Failed insertion did not add a second Undo snapshot.
    buffer.history(false);
    assert_eq!(buffer.text, "Ae\u{301}");
    assert_eq!(buffer.rich_text, original_style);
}

#[test]
fn rich_text_cluster_rejection_preserves_project_history_and_json_admission() {
    let mut editor = rich_editor("Ae\u{301}", 1);
    editor.clear_history();
    assert_rejected_unchanged(
        &mut editor,
        Command::ReplaceTextRange {
            id: 1,
            start: 1,
            end: 2,
            text: String::new(),
        },
    );
    assert_rejected_unchanged(
        &mut editor,
        Command::EditSourceText {
            id: 1,
            frame: 0,
            text: "A\u{301}".into(),
        },
    );
    let mut persisted = serde_json::to_value(editor.project()).unwrap();
    persisted["composition"]["layers"][0]["content"]["Text"]["text"] = "A\u{301}e".into();
    assert!(Project::from_json(&persisted.to_string()).is_err());
}

// Small test-only IEEE CRC-32 implementation so wire-admission regressions use
// actual native containers without adding another production/test dependency.
fn native_test_crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = u32::MAX;
    for byte in parts.iter().flat_map(|part| part.iter()).copied() {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn native_test_container(metadata: &serde_json::Value) -> Vec<u8> {
    let payload = serde_json::to_vec(metadata).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(project_file::MAGIC);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(52u64 + payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    let header_crc = native_test_crc32(&[&bytes]);
    bytes.extend_from_slice(&header_crc.to_le_bytes());
    let mut chunk = Vec::new();
    chunk.extend_from_slice(b"PROJ");
    chunk.extend_from_slice(&1u16.to_le_bytes());
    chunk.extend_from_slice(&0u16.to_le_bytes());
    chunk.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    let chunk_crc = native_test_crc32(&[&chunk, &payload]);
    bytes.extend_from_slice(&chunk);
    bytes.extend_from_slice(&chunk_crc.to_le_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

#[test]
fn rich_text_reconstructed_schema_71_native_container_fixture_is_valid() {
    assert_eq!(native_test_crc32(&[b"123456789"]), 0xcbf43926);
    let editor = rich_editor("A\r\n한😀", 1);
    let project = editor.project();
    assert_eq!(version(project), 71);
    let metadata = serde_json::to_value(project).unwrap();
    let bytes = native_test_container(&metadata);
    assert_eq!(project_file::decode(&bytes).unwrap().project, *project);
    assert_eq!(project_file::encode(project, None).unwrap(), bytes);
    assert_eq!(Project::from_json(&metadata.to_string()).unwrap(), *project);
}

#[test]
fn rich_text_historical_66_through_70_reject_before_unknown_data_can_disappear() {
    let editor = rich_editor("AB", 1);
    for version in 66..=70 {
        let expected = format!(
            "Unsupported project version {version}; versions 66–70 are not supported by this build"
        );
        for unknown_layout in [false, true] {
            let mut metadata = serde_json::to_value(editor.project()).unwrap();
            metadata["version"] = version.into();
            metadata["historical_rich_metadata"] = serde_json::json!({"unrecovered": true});
            if unknown_layout {
                // A formerly unknown model field and incompatible rich payload
                // must reach the schema diagnostic before serde/assets erase or
                // misinterpret the old source contract.
                metadata["composition"]["layers"][0]["historical_character_styles"] =
                    serde_json::json!({"runs":[{"unrecovered":true}]});
                metadata["composition"]["layers"][0]["rich_text"] =
                    serde_json::json!({"unrecovered_runs": [1, 2]});
                metadata["sequence_assets"] = serde_json::json!(["unrecovered asset layout"]);
            }
            let json = metadata.to_string();
            let bytes = native_test_container(&metadata);
            assert_eq!(Project::from_json(&json).unwrap_err(), expected);
            assert_eq!(project_file::decode(&bytes).unwrap_err(), expected);
            assert_eq!(json, metadata.to_string());
            assert_eq!(bytes, native_test_container(&metadata));
        }
        // Resident-model save, automation and editor installation reject too;
        // read-only wire preflight is not the only enforcement point.
        let mut metadata = serde_json::to_value(Project::default()).unwrap();
        metadata["version"] = version.into();
        let historical: Project = serde_json::from_value(metadata).unwrap();
        assert_eq!(
            historical.validate_automation_project().unwrap_err(),
            expected
        );
        assert_eq!(historical.to_json().unwrap_err(), expected);
        assert_eq!(
            project_file::encode(&historical, None).unwrap_err(),
            expected
        );
        let mut target = Editor::default();
        let before = target.project().clone();
        assert_eq!(target.replace_project(historical).unwrap_err(), expected);
        assert_eq!(target.project(), &before);
        assert!(!target.can_undo());
    }
}

#[test]
fn rich_text_schema_gap_preserves_all_legacy_versions_one_through_65() {
    for version in 1..=65 {
        let mut metadata = serde_json::to_value(Project::default()).unwrap();
        metadata["version"] = version.into();
        let project = Project::from_json(&metadata.to_string()).unwrap();
        let json = project.to_json().unwrap();
        assert!(!json.contains("rich_text"));
        let decoded = Project::from_json(&json).unwrap();
        assert_eq!(decoded, project);
        assert_eq!(decoded.to_json().unwrap(), json);
        let bytes = project_file::encode(&project, None).unwrap();
        let decoded = project_file::decode(&bytes).unwrap();
        assert_eq!(decoded.project, project);
        assert_eq!(
            project_file::encode(&decoded.project, decoded.view).unwrap(),
            bytes
        );
    }
}
