//! Frozen editing geometry must never become persisted source typography.
use super::*;
use libre_effects_core::{PropertyPath, TemporalHandle, TextParam, TextStyle, TrackEdit};
use std::sync::Arc;

const TYPOGRAPHY: [TextParam; 3] = [TextParam::FontSize, TextParam::Tracking, TextParam::Leading];

fn scene(paragraph: bool) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Typography sessions".into(),
            width: 640,
            height: 480,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "AA BB CC DD EE FF\n한글 e\u{301} 👩‍💻".into(),
                font_size: 48.,
            },
            width: 230.,
            height: 125.,
            name: "Text".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                paragraph,
                tracking: 0.,
                leading: 1.2,
                ..Default::default()
            },
        })
        .unwrap();
    for (parameter, value) in [
        (TextParam::FontSize, 96.),
        (TextParam::Tracking, 300.),
        (TextParam::Leading, 2.),
        (TextParam::FillRed, 96.),
        (TextParam::StrokeWidth, 16.),
    ] {
        for edit in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value { frame: 60, value },
        ] {
            editor
                .execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit,
                })
                .unwrap();
        }
    }
    editor
}

fn unchanged_typography(before: &Project, after: &Project) {
    let a = before.composition().layer(1).unwrap();
    let b = after.composition().layer(1).unwrap();
    assert!(matches!(b.content(), Content::Text { font_size, .. } if *font_size == 48.));
    assert_eq!(a.text_style(), b.text_style());
    assert_eq!(a.color(), b.color());
    for parameter in TextParam::ALL {
        assert_eq!(
            a.track(PropertyPath::Text(parameter)),
            b.track(PropertyPath::Text(parameter)),
            "{parameter:?}"
        );
        for frame in [0, 15, 30, 45, 60] {
            assert_eq!(
                a.text_value_at(parameter, frame),
                b.text_value_at(parameter, frame)
            );
        }
    }
}

fn assert_geometry(a: &layout::Layout, b: &layout::Layout) {
    assert_eq!(a.size, b.size);
    assert_eq!(a.carets, b.carets);
    let cells = |layout: &layout::Layout| {
        layout
            .cells
            .iter()
            .map(|cell| (cell.range.clone(), cell.x1, cell.x2, cell.y))
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(a), cells(b));
}

#[test]
fn typography_source_commit_cancel_and_undo_never_bake_midframe_or_existing_keys() {
    for paragraph in [false, true] {
        for frame in [30, 60] {
            let mut editor = scene(paragraph);
            editor
                .execute(Command::SetTemporalHandle {
                    id: 1,
                    property: PropertyPath::Text(TextParam::Tracking),
                    frame: 0,
                    incoming: false,
                    handle: TemporalHandle {
                        slope: 8.,
                        influence: 0.4,
                    },
                })
                .unwrap();
            let before = editor.project().clone();
            let native = crate::project_io::encode_native_project(&before, None).unwrap();
            let mut session = Session::new(&before, 7, frame, Some(1), [0.; 2]).unwrap();
            let sampled = before
                .composition()
                .layer(1)
                .unwrap()
                .text_typography_at(frame)
                .unwrap();
            assert_eq!(session.font_size, sampled.font_size);
            assert_eq!(session.style.tracking, sampled.tracking);
            assert_eq!(session.style.leading, sampled.leading);
            assert_ne!(session.font_size, 48.);
            assert!(session.valid(&before, 7, frame));
            assert!(!session.valid(&before, 8, frame));
            assert!(!session.valid(&before, 7, frame + 1));
            assert!(!session.changed());
            session.buffer.all();
            session
                .buffer
                .replace(
                    None,
                    "Changed 한글 e\u{301} 👩‍💻\nsecond line",
                    true,
                    Some(0..1),
                )
                .unwrap();
            if paragraph {
                session.width = 260.;
                session.height = 180.;
            }
            assert!(session.changed());
            let draft = session.project().unwrap();
            assert!(!session.valid(&draft, 7, frame));
            unchanged_typography(&before, &draft);
            assert_eq!(editor.project(), &before);
            let command = session.command();
            drop(session); // Escape/cancel only discards the frozen session.
            assert_eq!(
                crate::project_io::encode_native_project(editor.project(), None).unwrap(),
                native
            );
            editor.execute(command).unwrap();
            assert_eq!(editor.project(), &draft);
            unchanged_typography(&before, editor.project());
            let saved = crate::project_io::decode_project(
                &crate::project_io::encode_native_project(editor.project(), None).unwrap(),
            )
            .unwrap()
            .project;
            assert_eq!(saved, draft);
            // Seek, Undo and seek again must re-sample restored data, not the abandoned draft.
            for seek in [60, 0, 30] {
                let reopened = Session::new(editor.project(), 8, seek, Some(1), [0.; 2]).unwrap();
                assert_eq!(
                    reopened.font_size,
                    before
                        .composition()
                        .layer(1)
                        .unwrap()
                        .text_typography_at(seek)
                        .unwrap()
                        .font_size
                );
            }
            editor.undo();
            assert_eq!(editor.project(), &before);
            let restored = Session::new(editor.project(), 9, frame, Some(1), [0.; 2]).unwrap();
            assert_geometry(
                &layout::Layout::new(&restored),
                &layout::Layout::for_layer(before.composition().layer(1).unwrap(), frame).unwrap(),
            );
            editor.redo();
            assert_eq!(editor.project(), &draft);
        }
    }
}

#[test]
fn playhead_hit_caret_graphemes_and_layout_cache_share_sampled_typography() {
    for paragraph in [false, true] {
        let editor = scene(paragraph);
        let layer = editor.project().composition().layer(1).unwrap();
        let base_layout = layout::Layout::for_layer(layer, 0).unwrap();
        for frame in [0, 30, 60] {
            let session = Session::new(editor.project(), 0, frame, Some(1), [0.; 2]).unwrap();
            let hit_layout = layout::Layout::for_layer(layer, frame).unwrap();
            let caret_layout = layout::Layout::new(&session);
            assert!(Arc::ptr_eq(&hit_layout, &caret_layout));
            assert_geometry(&hit_layout, &caret_layout);
            let expected_size = 48. + frame as f64 * 0.8;
            let expected_style = TextStyle {
                paragraph,
                tracking: frame as f64 * 5.,
                leading: 1.2 + frame as f64 * 0.8 / 60.,
                ..Default::default()
            };
            let manual = layout::Layout::shape(
                &session.buffer.text,
                expected_size,
                session.width,
                &expected_style,
            );
            assert_geometry(&caret_layout, &manual);
            assert_eq!(caret_layout.size, expected_size);
            let caret = session.caret_position(&caret_layout);
            assert_eq!(
                caret_layout.caret_rect(caret),
                [caret[0], caret[1], 1., expected_size * 1.2]
            );
            assert_eq!(caret_layout.line_height(), expected_size * 1.2);
            let boundaries: Vec<_> = session
                .buffer
                .text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([session.buffer.text.len()])
                .collect();
            for cell in &caret_layout.cells {
                assert!(
                    boundaries.contains(&cell.range.start) && boundaries.contains(&cell.range.end)
                );
                let point = [(cell.x1 + cell.x2) / 2., cell.y + expected_size * 0.5];
                assert!(caret_layout.contains(point));
                assert!(boundaries.contains(&caret_layout.hit(point)));
                // Selection/IME height is sampled font size, with the same legacy 1.2 factor.
                assert!(caret_layout.contains([point[0], cell.y + expected_size * 1.19]));
            }
            if frame > 0 {
                assert_ne!(caret_layout.carets, base_layout.carets);
            }
        }
    }
}

#[test]
fn size_times_tracking_and_leading_drive_point_hit_and_vertical_navigation() {
    let mut editor = scene(false);
    editor
        .execute(Command::SetContent {
            id: 1,
            content: Content::Text {
                text: "AAAA\nAAAA\nAAAA".into(),
                font_size: 48.,
            },
        })
        .unwrap();
    let layer = editor.project().composition().layer(1).unwrap();
    let base = layout::Layout::for_layer(layer, 0).unwrap();
    for (frame, size, tracking, leading) in
        [(0, 48., 0., 1.2), (30, 72., 150., 1.6), (60, 96., 300., 2.)]
    {
        let mut session = Session::new(editor.project(), 0, frame, Some(1), [0.; 2]).unwrap();
        let actual = layout::Layout::new(&session);
        let untracked = layout::Layout::shape(
            "AAAA\nAAAA\nAAAA",
            size,
            session.width,
            &TextStyle {
                leading,
                ..Default::default()
            },
        );
        assert!(
            (actual.caret(4)[0] - untracked.caret(4)[0] - 3. * tracking * size / 1000.).abs()
                < 0.001
        );
        assert!((actual.caret(5)[1] - size * leading).abs() < 0.001);
        assert!((actual.caret(10)[1] - 2. * size * leading).abs() < 0.001);
        if frame > 0 {
            let point = [(base.caret(4)[0] + actual.caret(4)[0]) / 2., size * 0.5];
            assert!(!base.contains(point));
            assert!(actual.contains(point));
        }
        session.buffer.select(2, false);
        session.vertical(true, false);
        assert_eq!(session.buffer.caret, 7);
        session.vertical(true, true);
        assert_eq!(session.buffer.selection(), 7..12);
        session.vertical(false, false);
        assert_eq!(session.buffer.caret, 7);
    }
}

#[test]
fn sampled_paragraph_conversion_and_fit_preserve_static_style_and_all_tracks() {
    let editor = scene(true);
    let before = editor.project().clone();
    let layer = before.composition().layer(1).unwrap();
    let base = crate::text_flow::layer_lines(layer, 0).unwrap();
    let large = crate::text_flow::layer_lines(layer, 60).unwrap();
    assert!(large.len() > base.len());
    assert!(crate::text_flow::composed_count(&large, layer.height()) < large.len());
    assert!(
        crate::text_flow::fit_height(layer, 60).unwrap()
            > crate::text_flow::fit_height(layer, 0).unwrap()
    );
    for frame in [0, 30, 60] {
        let sample = layer.text_typography_at(frame).unwrap();
        let mut sampled_style = layer.text_style();
        sample.apply_to_style(&mut sampled_style);
        let Content::Text { text, .. } = layer.content() else {
            unreachable!()
        };
        let manual = crate::text_flow::lines(text, sample.font_size, layer.width(), &sampled_style);
        let expected = manual
            .iter()
            .take(crate::text_flow::composed_count(&manual, layer.height()))
            .map(|line| &text[line.range.start..line.visible_end])
            .collect::<Vec<_>>()
            .join("\n");
        let height = crate::text_flow::fit_height(layer, frame).unwrap().ceil();
        assert_eq!(
            height,
            manual
                .iter()
                .map(|line| line.bottom)
                .fold(1., f64::max)
                .ceil()
        );
        let mut fitted = Editor::default();
        fitted.replace_project(before.clone()).unwrap();
        fitted
            .execute(Command::SetTextBox {
                id: 1,
                width: layer.width(),
                height,
            })
            .unwrap();
        unchanged_typography(&before, fitted.project());
        let fitted_layer = fitted.project().composition().layer(1).unwrap();
        let fit_lines = crate::text_flow::layer_lines(fitted_layer, frame).unwrap();
        assert_eq!(
            crate::text_flow::composed_count(&fit_lines, height),
            fit_lines.len()
        );
        fitted.undo();
        assert_eq!(fitted.project(), &before);
        fitted
            .execute(crate::text_flow::convert(layer, false, frame))
            .unwrap();
        let point = fitted.project().composition().layer(1).unwrap();
        assert!(
            matches!(point.content(), Content::Text { text, font_size } if text == &expected && *font_size == 48.)
        );
        let mut expected_style = layer.text_style();
        expected_style.paragraph = false;
        assert_eq!(point.text_style(), expected_style);
        for parameter in TextParam::ALL {
            assert_eq!(
                point.track(PropertyPath::Text(parameter)),
                layer.track(PropertyPath::Text(parameter))
            );
        }
        assert!(crate::text_flow::fit_height(point, frame).is_none());
        fitted.undo();
        assert_eq!(fitted.project(), &before);
    }
    for parameter in TYPOGRAPHY {
        assert!(layer.track(PropertyPath::Text(parameter)).is_some());
    }
}
