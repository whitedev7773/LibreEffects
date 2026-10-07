//! Headless frozen-session, source-range and geometry checks for Source Text.
use super::*;
use crate::rendering::source_text_tests::{
    BASE, FIRST, LAST, UNICODE, animated_scene, baked_scene, expected_source,
};
use libre_effects_core::{PropertyPath, TextParam, TextStyle};
use std::sync::Arc;

fn same_geometry(a: &layout::Layout, b: &layout::Layout) {
    assert_eq!(a.size, b.size);
    assert_eq!(a.carets, b.carets);
    let cells = |l: &layout::Layout| {
        l.cells
            .iter()
            .map(|c| (c.range.clone(), c.x1, c.x2, c.y))
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(a), cells(b));
}

fn same_typography_and_paint(before: &Project, after: &Project) {
    let a = before.composition().layer(1).unwrap();
    let b = after.composition().layer(1).unwrap();
    assert_eq!(a.content(), b.content(), "immutable static baseline");
    assert_eq!(a.color(), b.color());
    assert_eq!(a.text_style(), b.text_style());
    for parameter in TextParam::ALL {
        assert_eq!(
            a.track(PropertyPath::Text(parameter)),
            b.track(PropertyPath::Text(parameter)),
            "{parameter:?}"
        );
    }
}

#[test]
fn source_text_caret_selection_hit_wrap_and_fit_share_the_frame_sample() {
    for paragraph in [false, true] {
        let e = animated_scene(paragraph);
        let before = e.project().clone();
        let layer = before.composition().layer(1).unwrap();
        for frame in [0, 9, 10, 19, 20, 29, 30, 39, 40, 60] {
            let s = Session::new(&before, 7, frame, Some(1), [0.; 2]).unwrap();
            assert_eq!(s.buffer.text, expected_source(frame));
            assert!(!s.changed());
            assert!(s.valid(&before, 7, frame));
            assert!(!s.valid(&before, 8, frame));
            assert!(!s.valid(&before, 7, frame + 1));
            let caret = layout::Layout::new(&s);
            let hit = layout::Layout::for_layer(layer, frame).unwrap();
            assert!(Arc::ptr_eq(&caret, &hit));
            let static_e = baked_scene(paragraph, frame);
            let static_layer = static_e.project().composition().layer(1).unwrap();
            let manual = layout::Layout::for_layer(static_layer, frame).unwrap();
            same_geometry(&caret, &manual);
            for (offset, point) in &caret.carets {
                assert!(s.buffer.text.is_char_boundary(*offset));
                assert_eq!(caret.caret(*offset), manual.caret(*offset));
                assert_eq!(caret.hit(*point), manual.hit(*point));
                assert_eq!(caret.hit_character(*point), manual.hit_character(*point));
            }
            for cell in &caret.cells {
                assert!(s.buffer.text.is_char_boundary(cell.range.start));
                assert!(s.buffer.text.is_char_boundary(cell.range.end));
            }
            let actual_flow = crate::text_flow::layer_lines(layer, frame).unwrap();
            let expected_flow = crate::text_flow::layer_lines(static_layer, frame).unwrap();
            let ranges = |flow: &[crate::text_flow::Line]| {
                flow.iter()
                    .map(|line| {
                        (
                            line.range.clone(),
                            line.visible_end,
                            line.bottom,
                            line.fits_width,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(ranges(&actual_flow), ranges(&expected_flow));
            assert_eq!(
                crate::text_flow::fit_height(layer, frame),
                crate::text_flow::fit_height(static_layer, frame)
            );
            let mut selection = s.clone();
            selection.buffer.all();
            assert_eq!(
                &selection.buffer.text[selection.buffer.selection()],
                expected_source(frame)
            );
            selection.line_edge(false, true, false);
            assert_eq!(selection.buffer.caret, 0);
            selection.line_edge(true, true, false);
            assert_eq!(selection.buffer.caret, expected_source(frame).len());
        }
        assert_eq!(e.project(), &before);
    }
}

#[test]
fn source_text_canvas_edits_preview_from_immutable_source_and_commit_only_current_key() {
    for paragraph in [false, true] {
        for frame in [20, 25, 30] {
            let mut e = animated_scene(paragraph);
            let before = e.project().clone();
            let native = crate::project_io::encode_native_project(&before, None).unwrap();
            let mut s = Session::new(&before, 11, frame, Some(1), [0.; 2]).unwrap();
            s.buffer.all();
            s.buffer
                .replace(None, "Edited 한글 👩‍💻\nline", true, Some(0..1))
                .unwrap();
            assert!(s.changed());
            if paragraph {
                s.width = 190.;
                s.height = 145.;
            }
            let draft = s.project().unwrap();
            for n in 0..8 {
                let mut intermediate = s.clone();
                intermediate.buffer.all();
                intermediate
                    .buffer
                    .replace(None, &format!("draft {n} 한글"), false, None)
                    .unwrap();
                let preview = intermediate.project().unwrap();
                assert_ne!(preview, before);
                assert_eq!(e.project(), &before);
            }
            assert_eq!(
                s.project().unwrap(),
                draft,
                "draft interning starts from immutable storage"
            );
            same_typography_and_paint(&before, &draft);
            assert!(!s.valid(&draft, 11, frame));
            let command = s.command();
            drop(s); // Cancel discards only the session and all temporary pools.
            assert_eq!(
                crate::project_io::encode_native_project(e.project(), None).unwrap(),
                native
            );
            e.execute(command).unwrap();
            assert_eq!(e.project(), &draft);
            let layer = e.project().composition().layer(1).unwrap();
            assert_eq!(layer.source_text_at(frame), Some("Edited 한글 👩‍💻\nline"));
            assert_eq!(layer.source_text_at(10), Some(FIRST));
            assert_eq!(layer.source_text_at(40), Some(LAST));
            if frame != 20 {
                assert_eq!(layer.source_text_at(20), Some(UNICODE));
            }
            if frame != 30 {
                assert_eq!(layer.source_text_at(30), Some(""));
            }
            assert!(matches!(layer.content(), Content::Text { text, .. } if text == BASE));
            let saved = crate::project_io::decode_project(
                &crate::project_io::encode_native_project(e.project(), None).unwrap(),
            )
            .unwrap()
            .project;
            let renderer = crate::rendering::Renderer::new();
            assert_eq!(
                renderer.render_preview(&draft, frame, 320).unwrap(),
                renderer.render(&saved, frame, 320).unwrap()
            );
            e.undo();
            assert_eq!(e.project(), &before);
            // A no-change source-only session batch must preserve exact Redo.
            let unchanged = Session::new(e.project(), 12, frame, Some(1), [0.; 2]).unwrap();
            assert!(!unchanged.changed());
            e.execute(unchanged.command()).unwrap();
            assert_eq!(e.project(), &before);
            assert!(e.can_redo());
            e.redo();
            assert_eq!(e.project(), &draft);
        }
    }
}

#[test]
fn source_text_sessions_reject_lock_and_stale_frame_project_or_composition() {
    let mut e = animated_scene(false);
    let s = Session::new(e.project(), 3, 25, Some(1), [0.; 2]).unwrap();
    e.execute(Command::EditSourceText {
        id: 1,
        frame: 40,
        text: "later edit".into(),
    })
    .unwrap();
    assert!(!s.valid(e.project(), 3, 25));
    e.undo();
    assert!(s.valid(e.project(), 3, 25));
    e.execute(Command::DuplicateComposition).unwrap();
    assert!(!s.valid(e.project(), 3, 25));
    e.undo();
    e.execute(Command::ToggleLocked(1)).unwrap();
    assert!(Session::new(e.project(), 3, 25, Some(1), [0.; 2]).is_err());
}

#[test]
fn source_text_fit_and_point_conversion_preserve_other_strings_and_base_typography() {
    let e = animated_scene(true);
    let before = e.project().clone();
    let layer = before.composition().layer(1).unwrap();
    for frame in [20, 25, 30, 60] {
        let static_e = baked_scene(true, frame);
        let static_layer = static_e.project().composition().layer(1).unwrap();
        let flow = crate::text_flow::layer_lines(static_layer, frame).unwrap();
        let source = expected_source(frame);
        let expected = flow
            .iter()
            .take(crate::text_flow::composed_count(
                &flow,
                static_layer.height(),
            ))
            .map(|line| &source[line.range.start..line.visible_end])
            .collect::<Vec<_>>()
            .join("\n");
        let height = crate::text_flow::fit_height(static_layer, frame)
            .unwrap()
            .ceil();
        assert_eq!(
            crate::text_flow::fit_height(layer, frame).unwrap().ceil(),
            height
        );
        let mut edited = Editor::default();
        edited.replace_project(before.clone()).unwrap();
        edited
            .execute(Command::SetTextBox {
                id: 1,
                width: layer.width(),
                height,
            })
            .unwrap();
        same_typography_and_paint(&before, edited.project());
        assert_eq!(
            edited
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .source_text_animation(),
            layer.source_text_animation()
        );
        edited.undo();
        assert_eq!(edited.project(), &before);
        edited
            .execute(crate::text_flow::convert(layer, false, frame))
            .unwrap();
        let point = edited.project().composition().layer(1).unwrap();
        assert_eq!(point.source_text_at(frame), Some(expected.as_str()));
        assert_eq!(point.content(), layer.content());
        assert_eq!(point.source_text_at(10), Some(FIRST));
        if frame != 20 {
            assert_eq!(point.source_text_at(20), Some(UNICODE));
        }
        if frame != 30 {
            assert_eq!(point.source_text_at(30), Some(""));
        }
        if frame < 40 {
            assert_eq!(point.source_text_at(40), Some(LAST));
        }
        let mut expected_style = layer.text_style();
        expected_style.paragraph = false;
        assert_eq!(
            point.text_style(),
            expected_style,
            "only global paragraph mode changes"
        );
        for parameter in TextParam::ALL {
            assert_eq!(
                point.track(PropertyPath::Text(parameter)),
                layer.track(PropertyPath::Text(parameter))
            );
        }
        let after = edited.project().clone();
        edited.undo();
        assert_eq!(edited.project(), &before, "conversion is one transaction");
        edited.redo();
        assert_eq!(edited.project(), &after);
    }
}

#[test]
fn source_text_legacy_inspection_and_unchanged_static_session_keep_sparse_bytes_and_redo() {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: "Legacy 한글".into(),
            font_size: 32.,
        },
        width: 200.,
        height: 100.,
        name: "Legacy".into(),
    })
    .unwrap();
    let before = e.project().clone();
    let native = crate::project_io::encode_native_project(&before, None).unwrap();
    assert!(!before.to_json().unwrap().contains("source_text_animation"));
    e.execute(Command::SetTextStyle {
        id: 1,
        style: TextStyle {
            weight: 700,
            ..Default::default()
        },
    })
    .unwrap();
    let after = e.project().clone();
    e.undo();
    let session = Session::new(e.project(), 1, 15, Some(1), [0.; 2]).unwrap();
    assert!(!session.changed());
    assert_eq!(session.project().unwrap(), before);
    e.execute(session.command()).unwrap();
    assert!(e.can_redo());
    assert_eq!(
        crate::project_io::encode_native_project(e.project(), None).unwrap(),
        native
    );
    e.redo();
    assert_eq!(e.project(), &after);
}
