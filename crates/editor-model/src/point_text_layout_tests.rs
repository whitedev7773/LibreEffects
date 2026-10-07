//! Synthetic native point-text contracts; no private reference text or assets.
use libre_effects_core::{
    Command, Content, Editor, Project, RichText, TextAlign, TextCharacterPatch, TextCharacterStyle,
    TextLeading, TextStyle, TextStyleRun, project_file,
};
use std::ops::Range;

fn character(size: f64, leading: Option<TextLeading>) -> TextCharacterStyle {
    let mut style = TextCharacterStyle::from_style(&TextStyle::default(), size, 0xffffff);
    style.leading = leading;
    style
}
fn rich(text: &str, spans: &[(Range<usize>, TextCharacterStyle)]) -> RichText {
    RichText::new(
        text,
        spans[0].1.clone(),
        spans
            .iter()
            .map(|(range, style)| TextStyleRun {
                start: range.start,
                end: range.end,
                style: style.clone(),
            })
            .collect(),
    )
    .unwrap()
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
            height: 360.0,
            name: "Synthetic point text".into(),
        })
        .unwrap();
    editor
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn point_text_uses_maximum_incoming_run_leading_and_size() {
    let text = "AB\nCD\r\nE\rF\n";
    let mut rich = rich(
        text,
        &[
            (0..3, character(90.0, Some(TextLeading::Fixed(120.0)))),
            (3..4, character(22.0, Some(TextLeading::Fixed(48.0)))),
            (4..5, character(30.0, Some(TextLeading::Auto(2.0)))),
            // A nonempty line's terminator cannot enlarge its size/advance.
            (5..7, character(96.0, Some(TextLeading::Fixed(500.0)))),
            (7..9, character(20.0, Some(TextLeading::Auto(1.5)))),
            (9..11, character(50.0, None)),
        ],
    );
    rich.point_origin = true;
    let lines = rich.line_metrics(text, &TextStyle::default()).unwrap();
    for (line, (baseline, top, size)) in lines.iter().zip([
        (0.0, -90.0, 90.0),
        (60.0, 30.0, 30.0),
        (90.0, 70.0, 20.0),
        (150.0, 100.0, 50.0),
        (210.0, 160.0, 50.0),
    ]) {
        close(line.baseline, baseline);
        close(line.top, top);
        close(line.size, size);
    }
    assert_eq!(lines.len(), 5);
    rich.point_origin = false;
    let shifted = rich.line_metrics(text, &TextStyle::default()).unwrap();
    for (point, legacy_origin) in lines.iter().zip(shifted) {
        close(legacy_origin.baseline, point.baseline + 90.0);
        close(legacy_origin.top, point.top + 90.0);
    }
}

#[test]
fn blank_lines_keep_break_bytes_and_use_their_own_insertion_style() {
    let text = "A\r\n\rB\n\nC\r";
    let mut rich = rich(
        text,
        &[
            (0..3, character(12.0, Some(TextLeading::Fixed(20.0)))),
            (3..4, character(18.0, Some(TextLeading::Fixed(27.0)))),
            (4..6, character(24.0, Some(TextLeading::Auto(1.5)))),
            (6..7, character(30.0, Some(TextLeading::Fixed(35.0)))),
            (7..9, character(14.0, Some(TextLeading::Auto(2.0)))),
        ],
    );
    rich.point_origin = true;
    let lines = rich.line_metrics(text, &TextStyle::default()).unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.range.clone(), line.terminator.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0..1, 1..3),
            (3..3, 3..4),
            (4..5, 5..6),
            (6..6, 6..7),
            (7..8, 8..9),
            (9..9, 9..9),
        ]
    );
    for (line, expected) in lines.iter().zip([0.0, 27.0, 63.0, 98.0, 126.0, 154.0]) {
        close(line.baseline, expected);
    }
    let mut empty =
        RichText::new("", character(18.0, Some(TextLeading::Fixed(27.0))), vec![]).unwrap();
    empty.point_origin = true;
    let line = empty.line_metrics("", &TextStyle::default()).unwrap();
    assert_eq!(line.len(), 1);
    assert_eq!((line[0].baseline, line[0].top), (0.0, -18.0));
}

#[test]
fn point_origin_without_leading_uses_incoming_size_and_alignment_around_zero() {
    let mut rich = rich(
        "A\nB",
        &[(0..2, character(50.0, None)), (2..3, character(20.0, None))],
    );
    for (align, anchor) in [
        (TextAlign::Left, 0.0),
        (TextAlign::Center, 320.0),
        (TextAlign::Right, 640.0),
    ] {
        assert_eq!(rich.alignment_origin(640.0, align), anchor);
    }
    rich.point_origin = true;
    let style = TextStyle {
        leading: 1.5,
        ..Default::default()
    };
    let lines = rich.line_metrics("A\nB", &style).unwrap();
    assert_eq!((lines[0].baseline, lines[1].baseline), (0.0, 30.0));
    for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        assert_eq!(rich.alignment_origin(640.0, align), 0.0);
        assert_eq!(rich.alignment_origin(123.0, align), 0.0);
    }
}

#[test]
fn utf16_style_conversion_retains_original_multibyte_line_ranges() {
    let text = "猫\r\n😀e\u{301}\n";
    let first = character(20.0, Some(TextLeading::Fixed(28.0)));
    let second = character(30.0, Some(TextLeading::Auto(1.5)));
    let mut rich = RichText::from_utf16_runs(
        text,
        first.clone(),
        vec![
            TextStyleRun {
                start: 0,
                end: 3,
                style: first,
            },
            TextStyleRun {
                start: 3,
                end: 8,
                style: second,
            },
        ],
    )
    .unwrap();
    rich.point_origin = true;
    let lines = rich.line_metrics(text, &TextStyle::default()).unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.range.clone(), line.terminator.clone(), line.baseline))
            .collect::<Vec<_>>(),
        vec![
            (0..3, 3..5, 0.0),
            (5..12, 12..13, 45.0),
            (13..13, 13..13, 90.0)
        ],
    );
    assert_eq!(&text[lines[1].range.clone()], "😀e\u{301}");
}

#[test]
fn absent_fields_keep_exact_legacy_json_and_line_arithmetic() {
    let json = r#"{"default_style":{"font_family":"Wanted Sans","font_face":"","weight":400,"italic":false,"font_size":24.0,"tracking":0.0,"fill_color":16777215,"fill_enabled":true,"stroke_color":0,"stroke_enabled":false,"stroke_width":1.0,"stroke_over_fill":false,"stroke_join":"Miter"},"runs":[{"start":0,"end":1,"style":{"font_family":"Wanted Sans","font_face":"","weight":400,"italic":false,"font_size":24.0,"tracking":0.0,"fill_color":16777215,"fill_enabled":true,"stroke_color":0,"stroke_enabled":false,"stroke_width":1.0,"stroke_over_fill":false,"stroke_join":"Miter"}}]}"#;
    let restored: RichText = serde_json::from_str(json).unwrap();
    assert!(!restored.has_explicit_line_metrics());
    assert_eq!(serde_json::to_string(&restored).unwrap(), json);
    assert_eq!(restored, rich("A", &[(0..1, character(24.0, None))]));
    let text = "A\r\nB\rC\n";
    let rich = rich(
        text,
        &[
            (0..3, character(24.0, None)),
            (3..5, character(40.0, None)),
            (5..7, character(32.0, None)),
        ],
    );
    let lines = rich.line_metrics(text, &TextStyle::default()).unwrap();
    let mut expected_top = 0.0;
    for (line, size) in lines.iter().zip([24.0, 40.0, 32.0, 32.0]) {
        assert_eq!(line.top, expected_top);
        assert_eq!(line.baseline, expected_top + size);
        expected_top += size * 1.2;
    }
}

#[test]
fn invalid_leading_is_rejected_without_normalizing_values_or_breaks() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, 0.099] {
        for leading in [TextLeading::Auto(invalid), TextLeading::Fixed(invalid)] {
            assert!(!leading.valid());
            let bad = character(24.0, Some(leading));
            assert!(RichText::new("", bad.clone(), vec![]).is_err());
            assert!(
                RichText::new(
                    "A",
                    character(24.0, None),
                    vec![TextStyleRun {
                        start: 0,
                        end: 1,
                        style: bad
                    }],
                )
                .is_err()
            );
        }
    }
    assert!(!TextLeading::Auto(10.001).valid());
    assert!(!TextLeading::Fixed(20480.001).valid());
    for valid in [
        TextLeading::Auto(0.1),
        TextLeading::Auto(10.0),
        TextLeading::Fixed(0.1),
        TextLeading::Fixed(20480.0),
    ] {
        assert!(valid.valid());
    }
    let a = character(24.0, Some(TextLeading::Fixed(36.0)));
    let b = character(24.0, Some(TextLeading::Auto(1.5)));
    assert!(
        RichText::new(
            "\r\n",
            a.clone(),
            vec![
                TextStyleRun {
                    start: 0,
                    end: 1,
                    style: a
                },
                TextStyleRun {
                    start: 1,
                    end: 2,
                    style: b
                },
            ],
        )
        .is_err()
    );
    assert!(serde_json::from_str::<TextLeading>(r#"{"Unknown":1.2}"#).is_err());
}

#[test]
fn text_edits_preserve_point_origin_and_character_leading() {
    let text = "AB\r\nCD";
    let mut rich = rich(
        text,
        &[
            (0..4, character(24.0, Some(TextLeading::Fixed(36.0)))),
            (4..6, character(30.0, Some(TextLeading::Auto(1.5)))),
        ],
    );
    rich.point_origin = true;
    let formatted = rich
        .format_range(text, &(4..6), &TextCharacterPatch::FontSize(40.0))
        .unwrap();
    assert!(formatted.point_origin);
    assert_eq!(formatted.style_at(4).leading, Some(TextLeading::Auto(1.5)));
    assert_eq!(
        formatted.line_metrics(text, &TextStyle::default()).unwrap()[1].baseline,
        60.0
    );
    let (next, replaced) = formatted.replace_range(text, 5..6, "XY").unwrap();
    assert_eq!(next, "AB\r\nCXY");
    assert!(replaced.point_origin);
    assert_eq!(replaced.style_at(6).leading, Some(TextLeading::Auto(1.5)));
    let (_, empty) = replaced.replace_range(&next, 0..next.len(), "").unwrap();
    assert!(empty.point_origin);
    assert_eq!(empty.default_style.leading, Some(TextLeading::Fixed(36.0)));
}

#[test]
fn schema74_roundtrip_undo_and_native_bytes_preserve_explicit_metrics() {
    let text = "A\r\nB";
    let legacy = rich(text, &[(0..text.len(), character(24.0, None))]);
    let mut editor = editor(text);
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(legacy.clone()),
        })
        .unwrap();
    let mut old_json = serde_json::to_value(editor.project()).unwrap();
    old_json["version"] = 71.into();
    let old = Project::from_json(&old_json.to_string()).unwrap();
    editor.replace_project(old).unwrap();
    let old_json = editor.project().to_json().unwrap();
    let old_native = project_file::encode(editor.project(), None).unwrap();
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(legacy.clone()),
        })
        .unwrap();
    assert_eq!(editor.project().to_json().unwrap(), old_json);
    assert_eq!(
        project_file::encode(editor.project(), None).unwrap(),
        old_native
    );
    let mut old_layer =
        serde_json::to_value(editor.project().composition().layer(1).unwrap()).unwrap();
    let mut explicit = legacy;
    explicit.point_origin = true;
    explicit.default_style.leading = Some(TextLeading::Fixed(35.0));
    explicit.runs[0].style.leading = Some(TextLeading::Auto(1.5));
    editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(explicit.clone()),
        })
        .unwrap();
    let project = editor.project().clone();
    let mut json = serde_json::to_value(&project).unwrap();
    assert_eq!(json["version"], 74);
    let mut new_layer = json["composition"]["layers"][0].clone();
    old_layer.as_object_mut().unwrap().remove("rich_text");
    new_layer.as_object_mut().unwrap().remove("rich_text");
    assert_eq!(
        new_layer, old_layer,
        "authored layer frame must stay intact"
    );
    let native = project_file::encode(&project, None).unwrap();
    let decoded = project_file::decode(&native).unwrap().project;
    assert_eq!(decoded, project);
    assert_eq!(project_file::encode(&decoded, None).unwrap(), native);
    assert_eq!(
        decoded.composition().layer(1).unwrap().rich_text(),
        Some(&explicit)
    );
    for old_version in [71, 72, 73] {
        json["version"] = old_version.into();
        assert!(
            Project::from_json(&json.to_string())
                .unwrap_err()
                .contains("version 74")
        );
    }
    editor.undo();
    assert_eq!(editor.project().to_json().unwrap(), old_json);
    editor.redo();
    assert_eq!(editor.project(), &project);
}

#[test]
fn each_explicit_field_requires_schema74_and_invalid_changes_are_atomic() {
    for mode in 0..3 {
        let mut rich = rich("A", &[(0..1, character(24.0, None))]);
        match mode {
            0 => rich.point_origin = true,
            1 => rich.default_style.leading = Some(TextLeading::Fixed(35.0)),
            _ => rich.runs[0].style.leading = Some(TextLeading::Auto(1.5)),
        }
        let mut editor = editor("A");
        editor
            .execute(Command::SetStyledText {
                id: 1,
                text: "A".into(),
                rich_text: rich.clone(),
            })
            .unwrap();
        let mut json = serde_json::to_value(editor.project()).unwrap();
        assert_eq!(json["version"], 74);
        json["version"] = 73.into();
        assert!(Project::from_json(&json.to_string()).is_err());
        let before = editor.project().clone();
        let generation = editor.context_generation();
        rich.runs[0].style.leading = Some(TextLeading::Fixed(-1.0));
        assert!(
            editor
                .execute(Command::SetRichText {
                    id: 1,
                    rich_text: Some(rich)
                })
                .is_err()
        );
        assert_eq!(editor.project(), &before);
        assert_eq!(editor.context_generation(), generation);
    }
}
