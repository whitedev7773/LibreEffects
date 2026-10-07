//! GPUI-free source paragraph coverage. Fixtures are synthetic multilingual text.
use libre_effects_core::{
    Command, Content, Editor, Project, PropertyPath, TrackEdit,
    text_paragraphs::{paragraph_at, paragraphs},
};

#[test]
fn text_paragraphs_preserve_mixed_original_byte_and_utf16_ranges() {
    let text = "English\r日本語\r\n한국어\n\r끝\r\n";
    let result: Vec<_> = paragraphs(text).collect();
    assert_eq!(
        result.iter().map(|p| p.text).collect::<Vec<_>>(),
        ["English", "日本語", "한국어", "", "끝", ""]
    );
    assert_eq!(
        result.iter().map(|p| p.range.clone()).collect::<Vec<_>>(),
        [0..7, 8..17, 19..28, 29..29, 30..33, 35..35]
    );
    assert_eq!(
        result
            .iter()
            .map(|p| p.terminator.clone())
            .collect::<Vec<_>>(),
        [7..8, 17..19, 28..29, 29..30, 33..35, 35..35]
    );
    let utf16_ranges: Vec<_> = result
        .iter()
        .map(|p| {
            text[..p.range.start].encode_utf16().count()..text[..p.range.end].encode_utf16().count()
        })
        .collect();
    assert_eq!(utf16_ranges, [0..7, 8..11, 13..16, 17..17, 18..19, 21..21]);
    assert_eq!(
        result
            .iter()
            .map(|p| &text[p.source_range()])
            .collect::<String>(),
        text
    );
}

#[test]
fn text_paragraphs_keep_empty_and_trailing_paragraphs_and_crlf_atomic() {
    for (text, expected) in [
        ("", vec![""]),
        ("A", vec!["A"]),
        ("\r", vec!["", ""]),
        ("\n", vec!["", ""]),
        ("\r\n", vec!["", ""]),
        ("\r\n\r\n", vec!["", "", ""]),
        ("\r\r\n\n", vec!["", "", "", ""]),
        ("A\r\n\rB\n", vec!["A", "", "B", ""]),
    ] {
        let mut iter = paragraphs(text);
        assert_eq!(iter.by_ref().map(|p| p.text).collect::<Vec<_>>(), expected);
        assert_eq!(iter.size_hint(), (0, Some(0)));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }
}

#[test]
fn text_paragraphs_keep_unicode_contents_and_find_source_positions() {
    let text = "e\u{301}👩‍💻\r\n日本語\r한국어\n";
    let result: Vec<_> = paragraphs(text).collect();
    assert_eq!(result[0].text, "e\u{301}👩‍💻");
    assert_eq!(result[0].range, 0..14);
    assert_eq!(result[0].terminator, 14..16);
    assert_eq!(text[..result[0].range.end].encode_utf16().count(), 7);
    assert_eq!(text[..result[1].range.start].encode_utf16().count(), 9);
    for p in &result {
        for at in p.source_range() {
            assert_eq!(paragraph_at(text, at), *p);
        }
        assert!(text.is_char_boundary(p.range.start));
        assert!(text.is_char_boundary(p.range.end));
        assert_eq!(&text[p.range.clone()], p.text);
    }
    assert_eq!(paragraph_at(text, 14), result[0]);
    assert_eq!(paragraph_at(text, 15), result[0]);
    assert_eq!(paragraph_at(text, 16), result[1]);
    assert_eq!(paragraph_at(text, usize::MAX), *result.last().unwrap());
    assert_eq!(paragraph_at("", 42).range, 0..0);
    // Unicode soft separators remain within a hard paragraph. Line wrapping
    // interprets their separate UAX #14 semantics without rewriting them.
    assert_eq!(paragraphs("A\u{2028}B\u{2029}C").count(), 1);
}

#[test]
fn text_paragraphs_partition_every_short_break_combination_without_loss() {
    // Exhaust all strings of up to seven bytes over content/CR/LF, including
    // adjacent CRLF and LFCR sequences. This also bounds the iterator output.
    for len in 0..=7 {
        for mut code in 0..3usize.pow(len) {
            let text: String = (0..len)
                .map(|_| {
                    let c = ['A', '\r', '\n'][code % 3];
                    code /= 3;
                    c
                })
                .collect();
            let result: Vec<_> = paragraphs(&text).collect();
            assert!(!result.is_empty() && result.len() <= text.len() + 1);
            let mut cursor = 0;
            for (index, p) in result.iter().enumerate() {
                assert_eq!(p.range.start, cursor);
                assert_eq!(p.range.end, p.terminator.start);
                assert!(!p.text.contains(['\r', '\n']));
                assert!(matches!(
                    &text[p.terminator.clone()],
                    "" | "\r" | "\n" | "\r\n"
                ));
                assert_eq!(p.terminator.is_empty(), index + 1 == result.len());
                cursor = p.terminator.end;
            }
            assert_eq!(cursor, text.len());
            assert_eq!(
                result
                    .iter()
                    .map(|p| &text[p.source_range()])
                    .collect::<String>(),
                text
            );
        }
    }
}

#[test]
fn text_paragraphs_do_not_mutate_authored_or_animated_source_on_save_or_history() {
    let original = "English\r日本語\r한국어\r";
    let changed = "👩‍💻\r\n\r日本語\n한국어\r\n";
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: original.into(),
                font_size: 32.0,
            },
            width: 640.0,
            height: 360.0,
            name: "Synthetic lyric paragraphs".into(),
        })
        .unwrap();
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    let before = editor.project().clone();
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 30,
            text: changed.into(),
        })
        .unwrap();
    let after = editor.project().clone();
    let saved = Project::from_json(&after.to_json().unwrap()).unwrap();
    for project in [&after, &saved] {
        let layer = project.composition().layer(1).unwrap();
        assert_eq!(layer.source_text_at(0), Some(original));
        assert_eq!(layer.source_text_at(30), Some(changed));
        assert_eq!(paragraphs(layer.source_text_at(0).unwrap()).count(), 4);
        assert_eq!(paragraphs(layer.source_text_at(30).unwrap()).count(), 5);
    }
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
}
