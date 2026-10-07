//! Headless regressions for click-created, content-sized point text. These test
//! session/layout/codec behavior, not native pointer or keyboard event delivery.
use super::*;
use libre_effects_core::{PropertyPath, TextAlign, TextParam, TrackEdit};

fn replace(session: &mut Session, text: &str) {
    session.buffer.all();
    session.buffer.replace(None, text, false, None).unwrap();
}

fn point_editor(text: &str) -> (Editor, LayerId) {
    let mut editor = Editor::default();
    let mut session = Session::new(editor.project(), 0, 0, None, [123.5, 87.25]).unwrap();
    replace(&mut session, text);
    editor.execute(session.command()).unwrap();
    (editor, session.id)
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.001,
        "expected {expected}, got {actual}"
    );
}

fn encloses_logical_geometry(layout: &layout::Layout) {
    let [x, y, width, height] = layout.bounds();
    assert!([x, y, width, height].iter().all(|v| v.is_finite()));
    assert!(width > 0.0 && height > 0.0);
    let enclosed = |px: f64, py: f64| {
        assert!(px >= x - 0.001 && px <= x + width + 0.001);
        assert!(py >= y - 0.001 && py <= y + height + 0.001);
    };
    for (_, point) in &layout.carets {
        // Bounds describe logical caret origins, not the painted caret's
        // one-pixel thickness. Empty/zero-width text still has a usable extent.
        let [cx, cy, _, ch] = layout.caret_rect(*point);
        enclosed(cx, cy);
        enclosed(cx, cy + ch);
    }
    for cell in &layout.cells {
        enclosed(cell.x1, cell.y);
        enclosed(cell.x2, cell.y + layout.line_height());
    }
}

#[test]
fn single_click_point_session_keeps_its_click_anchor_while_content_grows() {
    let editor = Editor::default();
    let before = editor.project().clone();
    let click = [243.5, 118.75];
    let mut session = Session::new(&before, 7, 15, None, click).unwrap();
    let dimensions = [session.width, session.height];
    assert!(!session.style.paragraph);
    assert!(!session.changed());
    assert_eq!(session.world.point([0.0; 2]), click);
    let empty = layout::Layout::new(&session);
    encloses_logical_geometry(&empty);
    assert!(empty.bounds()[2] < dimensions[0]);

    for text in ["Point", "A much longer point-text title\nsecond line", "I"] {
        replace(&mut session, text);
        let draft = session.project().unwrap();
        let layer = draft.composition().layer(session.id).unwrap();
        assert!(!layer.text_style().paragraph);
        assert_eq!([layer.width(), layer.height()], dimensions);
        assert_eq!(
            layer
                .property(Property::AnchorX)
                .expect("2D fixture property")
                .value_at(15),
            0.0
        );
        assert_eq!(
            layer
                .property(Property::AnchorY)
                .expect("2D fixture property")
                .value_at(15),
            0.0
        );
        assert_eq!(
            draft
                .composition()
                .world_transform(session.id, 15)
                .unwrap()
                .point([0.0; 2]),
            click
        );
        assert_eq!(session.world.point([0.0; 2]), click);
        assert_eq!(editor.project(), &before, "draft is not a live edit");
    }
}

#[test]
fn point_text_bounds_grow_and_shrink_without_soft_wrap_or_stored_resize() {
    let mut session = Session::new(&Project::default(), 0, 0, None, [0.0; 2]).unwrap();
    let dimensions = [session.width, session.height];
    replace(&mut session, "W");
    let short = layout::Layout::new(&session).bounds();
    replace(&mut session, &"W".repeat(32));
    let wide_layout = layout::Layout::new(&session);
    let wide = wide_layout.bounds();
    encloses_logical_geometry(&wide_layout);
    assert!(wide[2] > dimensions[0]);
    assert!(wide[2] > short[2]);
    close(wide[3], short[3]);
    assert!(wide_layout.carets.iter().all(|(_, p)| p[1] == 0.0));
    let draft = session.project().unwrap();
    let layer = draft.composition().layer(session.id).unwrap();
    assert_eq!([layer.width(), layer.height()], dimensions);
    assert_eq!(crate::text_flow::layer_lines(layer, 0).unwrap().len(), 1);

    replace(&mut session, "W");
    assert_eq!(layout::Layout::new(&session).bounds(), short);
    replace(&mut session, "");
    let empty = layout::Layout::new(&session);
    encloses_logical_geometry(&empty);
    assert!(empty.bounds()[2] < short[2]);
    close(empty.bounds()[3], short[3]);
    assert!(!session.changed(), "empty new text is not committed");
    assert_eq!([session.width, session.height], dimensions);
}

#[test]
fn point_text_bounds_include_multiline_and_trailing_empty_caret_lines() {
    let mut session = Session::new(&Project::default(), 0, 0, None, [0.0; 2]).unwrap();
    session.style.leading = 1.7;
    replace(&mut session, "WW");
    let first = layout::Layout::new(&session).bounds();
    for (text, lines) in [("WW\nI", 2), ("WW\nI\n\n", 4), ("\n\n", 3), ("", 1)] {
        replace(&mut session, text);
        let layout = layout::Layout::new(&session);
        let bounds = layout.bounds();
        encloses_logical_geometry(&layout);
        close(bounds[1], 0.0);
        close(
            bounds[3],
            (lines - 1) as f64 * session.font_size * session.style.leading + layout.line_height(),
        );
        close(
            layout.caret(session.buffer.text.len())[1],
            (lines - 1) as f64 * session.font_size * session.style.leading,
        );
        if text.starts_with("WW") {
            close(bounds[2], first[2]);
        }
    }
}

#[test]
fn point_text_bounds_enclose_aligned_rtl_spaces_and_combining_graphemes() {
    let mut session = Session::new(&Project::default(), 0, 0, None, [0.0; 2]).unwrap();
    for text in ["ABC", "אבג", "e\u{301} 한글 👩‍💻", "  A  ", "\n"] {
        replace(&mut session, text);
        let mut widths = vec![];
        let mut lefts = vec![];
        for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
            session.style.align = align;
            let layout = layout::Layout::new(&session);
            encloses_logical_geometry(&layout);
            let bounds = layout.bounds();
            widths.push(bounds[2]);
            lefts.push(bounds[0]);
        }
        close(widths[0], widths[1]);
        close(widths[0], widths[2]);
        assert!(lefts[0] < lefts[1] && lefts[1] < lefts[2]);
    }
}

#[test]
fn point_text_bounds_resample_font_size_without_changing_authored_dimensions() {
    let (mut editor, id) = point_editor("WW\nI");
    for edit in [
        TrackEdit::ToggleAnimation { frame: 0 },
        TrackEdit::Value {
            frame: 60,
            value: 144.0,
        },
    ] {
        editor
            .execute(Command::EditText {
                id,
                parameter: TextParam::FontSize,
                edit,
            })
            .unwrap();
    }
    let before = editor.project().clone();
    let native = crate::project_io::encode_native_project(&before, None).unwrap();
    let layer = before.composition().layer(id).unwrap();
    let dimensions = [layer.width(), layer.height()];
    let mut previous = [0.0; 4];
    for frame in [0, 15, 30, 45, 60] {
        let expected_size = 72.0 + 1.2 * f64::from(frame);
        let session = Session::new(&before, 8, frame, Some(id), [0.0; 2]).unwrap();
        close(session.font_size, expected_size);
        let actual = layout::Layout::for_layer(layer, frame).unwrap();
        let expected =
            layout::Layout::shape("WW\nI", expected_size, dimensions[0], &layer.text_style());
        assert_eq!(actual.bounds(), expected.bounds());
        assert_eq!(actual.bounds(), layout::Layout::new(&session).bounds());
        encloses_logical_geometry(&actual);
        let bounds = actual.bounds();
        for _ in 0..3 {
            assert_eq!(
                layout::Layout::for_layer(layer, frame).unwrap().bounds(),
                bounds
            );
        }
        assert!(bounds[2] > previous[2] && bounds[3] > previous[3]);
        previous = bounds;
        assert!(!session.changed());
        assert_eq!([session.width, session.height], dimensions);
    }
    assert_eq!(editor.project(), &before);
    assert_eq!(
        crate::project_io::encode_native_project(editor.project(), None).unwrap(),
        native
    );
}

#[test]
fn point_text_bounds_follow_source_text_hold_keys_including_empty_samples() {
    let (mut editor, id) = point_editor("W");
    editor
        .execute(Command::EditTrack {
            id,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    let wide = "W".repeat(24);
    for (frame, text) in [(10, wide.as_str()), (20, "I\n\n"), (30, "")] {
        editor
            .execute(Command::EditSourceText {
                id,
                frame,
                text: text.into(),
            })
            .unwrap();
    }
    let before = editor.project().clone();
    let native = crate::project_io::encode_native_project(&before, None).unwrap();
    let layer = before.composition().layer(id).unwrap();
    let mut sampled = vec![];
    for frame in [0, 9, 10, 19, 20, 29, 30, 60] {
        let expected_text = match frame {
            0..10 => "W",
            10..20 => wide.as_str(),
            20..30 => "I\n\n",
            _ => "",
        };
        let session = Session::new(&before, 9, frame, Some(id), [0.0; 2]).unwrap();
        assert_eq!(session.buffer.text, expected_text);
        let actual = layout::Layout::for_layer(layer, frame).unwrap();
        let expected =
            layout::Layout::shape(expected_text, 72.0, layer.width(), &layer.text_style());
        assert_eq!(actual.bounds(), expected.bounds());
        assert_eq!(actual.bounds(), layout::Layout::new(&session).bounds());
        encloses_logical_geometry(&actual);
        for _ in 0..3 {
            assert_eq!(
                layout::Layout::for_layer(layer, frame).unwrap().bounds(),
                actual.bounds()
            );
        }
        sampled.push(actual.bounds());
    }
    for pair in sampled.chunks_exact(2) {
        assert_eq!(pair[0], pair[1], "source keys use hold interpolation");
    }
    assert!(sampled[2][2] > sampled[0][2]);
    assert!(sampled[4][2] < sampled[2][2]);
    assert!(sampled[4][3] > sampled[2][3]);
    assert!(sampled[6][2] < sampled[4][2]);
    assert!(sampled[6][3] < sampled[4][3]);
    assert_eq!(editor.project(), &before);
    assert_eq!(
        crate::project_io::encode_native_project(editor.project(), None).unwrap(),
        native
    );
}

#[test]
fn paragraph_text_keeps_drag_authored_box_when_text_or_font_size_changes() {
    let mut editor = Editor::default();
    let mut session = Session::new_box(editor.project(), 0, 0, [20.0, 30.0, 210.0, 165.0]).unwrap();
    for text in ["I", "one two three four five six seven eight\n\n", ""] {
        replace(&mut session, text);
        assert!(session.style.paragraph);
        assert_eq!([session.width, session.height], [210.0, 165.0]);
        let draft = session.project().unwrap();
        let layer = draft.composition().layer(session.id).unwrap();
        assert!(layer.text_style().paragraph);
        assert_eq!([layer.width(), layer.height()], [210.0, 165.0]);
    }
    replace(&mut session, "one two three four five six seven eight");
    editor.execute(session.command()).unwrap();
    let id = session.id;
    let lines_before =
        crate::text_flow::layer_lines(editor.project().composition().layer(id).unwrap(), 0)
            .unwrap()
            .len();
    assert!(
        lines_before > 1,
        "paragraph text still wraps to authored width"
    );
    editor
        .execute(Command::EditText {
            id,
            parameter: TextParam::FontSize,
            edit: TrackEdit::Value {
                frame: 0,
                value: 144.0,
            },
        })
        .unwrap();
    let layer = editor.project().composition().layer(id).unwrap();
    assert!(layer.text_style().paragraph);
    assert_eq!([layer.width(), layer.height()], [210.0, 165.0]);
    assert!(crate::text_flow::layer_lines(layer, 0).unwrap().len() > lines_before);
}

#[test]
fn cancel_point_text_drafts_preserves_document_and_existing_redo_history() {
    let (mut editor, id) = point_editor("Original");
    editor
        .execute(Command::SetPosition {
            id,
            frame: 0,
            x: 300.0,
            y: 200.0,
        })
        .unwrap();
    let redo_target = editor.project().clone();
    editor.undo();
    let before = editor.project().clone();
    let native = crate::project_io::encode_native_project(&before, None).unwrap();
    let selected = editor.selected();
    for target in [None, Some(id)] {
        let mut session = Session::new(&before, 10, 0, target, [50.0, 60.0]).unwrap();
        for text in ["A long abandoned draft\n\n", "I"] {
            replace(&mut session, text);
            assert!(session.changed());
            encloses_logical_geometry(&layout::Layout::new(&session));
            assert_ne!(session.project().unwrap(), before);
            assert_eq!(editor.project(), &before);
        }
        drop(session);
        assert_eq!(editor.selected(), selected);
        assert!(editor.can_redo());
        assert_eq!(
            crate::project_io::encode_native_project(editor.project(), None).unwrap(),
            native
        );
    }
    editor.redo();
    assert_eq!(editor.project(), &redo_target);
}

#[test]
fn point_text_typing_and_dynamic_bounds_commit_as_one_undo_redo_step() {
    for existing in [false, true] {
        let (seed, id) = point_editor("Original");
        let base = if existing {
            seed.project().clone()
        } else {
            Project::default()
        };
        let mut editor = Editor::default();
        editor.replace_project(base.clone()).unwrap();
        editor.clear_history();
        assert!(!editor.can_undo());
        let mut session = Session::new(&base, 11, 0, existing.then_some(id), [50.0, 60.0]).unwrap();
        let dimensions = [session.width, session.height];
        for text in ["W", "A long title\nsecond line\n", "Final\n\n"] {
            replace(&mut session, text);
            encloses_logical_geometry(&layout::Layout::new(&session));
            let _ = session.project().unwrap();
            assert_eq!(editor.project(), &base);
            assert!(!editor.can_undo());
        }
        let expected = session.project().unwrap();
        editor.execute(session.command()).unwrap();
        assert_eq!(editor.project(), &expected);
        let layer = editor.project().composition().layer(session.id).unwrap();
        assert_eq!(layer.source_text_at(0), Some("Final\n\n"));
        assert!(!layer.text_style().paragraph);
        assert_eq!([layer.width(), layer.height()], dimensions);
        editor.undo();
        assert_eq!(editor.project(), &base);
        assert!(
            !editor.can_undo(),
            "typing produced only one document command"
        );
        assert!(editor.can_redo());
        editor.redo();
        assert_eq!(editor.project(), &expected);
        assert!(!editor.can_redo());
    }
}

#[test]
fn point_text_json_and_native_reopen_keep_dynamic_mode_and_anchor() {
    let (editor, id) = point_editor("Saved point text\n\n");
    let before = editor.project().clone();
    let original = before.composition().layer(id).unwrap();
    let dimensions = [original.width(), original.height()];
    let original_bounds = layout::Layout::for_layer(original, 0).unwrap().bounds();
    let json = Project::from_json(&before.to_json().unwrap()).unwrap();
    let native = crate::project_io::decode_project(
        &crate::project_io::encode_native_project(&before, None).unwrap(),
    )
    .unwrap()
    .project;
    for reopened in [json, native] {
        assert_eq!(reopened, before);
        let layer = reopened.composition().layer(id).unwrap();
        assert!(!layer.text_style().paragraph);
        assert_eq!([layer.width(), layer.height()], dimensions);
        assert_eq!(
            layout::Layout::for_layer(layer, 0).unwrap().bounds(),
            original_bounds
        );
        let mut session = Session::new(&reopened, 12, 0, Some(id), [0.0; 2]).unwrap();
        assert_eq!(session.world.point([0.0; 2]), [123.5, 87.25]);
        replace(&mut session, &"W".repeat(32));
        let wide = layout::Layout::new(&session).bounds();
        assert!(wide[2] > dimensions[0]);
        assert!(wide[3] < original_bounds[3]);
        let edited = session.project().unwrap();
        let layer = edited.composition().layer(id).unwrap();
        assert!(!layer.text_style().paragraph);
        assert_eq!([layer.width(), layer.height()], dimensions);
        assert_eq!(crate::text_flow::layer_lines(layer, 0).unwrap().len(), 1);
        assert_eq!(reopened, before, "reopened draft remains transactional");
    }
}
