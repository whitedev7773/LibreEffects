use super::*;
use libre_effects_core::{Editor, TextStyle};

fn scene() -> EditorState {
    let mut state = EditorState::default();
    state.editor = Editor::default();
    state
        .editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Base 한글\nSecond\u{2028}line 👋".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 240.,
            name: "Paragraph controls".into(),
        })
        .unwrap();
    state
        .editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                paragraph: true,
                ..Default::default()
            },
        })
        .unwrap();
    state.selected_layers = [1].into();
    state.editor.clear_history();
    state
}

fn bound(state: &EditorState) -> (Rc<RefCell<ParagraphInput>>, ParagraphTarget) {
    let session = Rc::new(RefCell::new(ParagraphInput::default()));
    session.borrow_mut().observe(state);
    let target = session.borrow().target.clone().unwrap();
    (session, target)
}

fn apply(state: &mut EditorState, command: Command) -> bool {
    state.bulk_test_action(&Action::Edit(command));
    state.status == "Edited"
}

#[test]
fn seven_paragraph_fields_have_precise_static_mapping_and_bounds() {
    let state = scene();
    let layer = state.editor.selected_layer().unwrap();
    assert_eq!(PARAGRAPH_FIELDS, 7);
    assert_eq!(
        PARAGRAPH_LABELS,
        [
            "Box width (px)",
            "Box height (px)",
            "Left indent (px)",
            "Right indent (px)",
            "First-line indent (px)",
            "Space before (px)",
            "Space after (px)",
        ]
    );
    assert_eq!(paragraph_values(layer), [400., 240., 0., 0., 0., 0., 0.]);
    assert_eq!(paragraph_field(0), None);
    assert_eq!(paragraph_field(1), None);
    assert_eq!(paragraph_field(7), None);
    for (index, field) in TextParagraphField::ALL
        .into_iter()
        .enumerate()
        .map(|(i, p)| (i + 2, p))
    {
        assert_eq!(paragraph_field(index), Some(field));
        let command = paragraph_field_command(layer, index, "12.123456789012345")
            .unwrap()
            .unwrap();
        assert!(
            matches!(command, Command::SetTextParagraphValue { id: 1, field: actual, value }
            if actual == field && value == 12.123456789012345)
        );
        let (min, max) = field.bounds();
        for value in [min, max] {
            assert!(paragraph_field_command(layer, index, &value.to_string()).is_ok());
        }
        for value in [min - 0.001, max + 0.001] {
            assert!(paragraph_field_command(layer, index, &value.to_string()).is_err());
        }
    }
    for index in 0..PARAGRAPH_FIELDS {
        for text in ["NaN", "inf", "-inf", "1e400", "", "1 px", "bad"] {
            assert!(
                paragraph_field_command(layer, index, text).is_err(),
                "{index}: {text}"
            );
        }
    }
    for index in 0..2 {
        for value in [1., 16384.] {
            assert!(paragraph_field_command(layer, index, &value.to_string()).is_ok());
        }
        for text in ["0", "-0", "0.999", "16384.001"] {
            assert!(paragraph_field_command(layer, index, text).is_err());
        }
    }
    assert!(paragraph_field_command(layer, 7, "1").is_err());
    assert!(paragraph_field_command(layer, usize::MAX, "1").is_err());
}

#[test]
fn paragraph_noops_keep_redo_and_style_edits_are_single_undo_steps() {
    let mut state = scene();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo target".into(),
        })
        .unwrap();
    state.editor.undo();
    let baseline = state.editor.project().clone();
    for (index, value) in paragraph_values(state.editor.selected_layer().unwrap())
        .into_iter()
        .enumerate()
    {
        for text in [
            value.to_string(),
            format!("  {value:e}  "),
            format!("{value:.8}"),
        ] {
            assert!(
                paragraph_field_command(state.editor.selected_layer().unwrap(), index, &text)
                    .unwrap()
                    .is_none()
            );
        }
    }
    for index in 2..PARAGRAPH_FIELDS {
        assert!(
            paragraph_field_command(state.editor.selected_layer().unwrap(), index, "-0.0")
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(state.editor.project(), &baseline);
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
    for (index, value) in [
        (2, 18.123456789012345),
        (3, 27.5),
        (4, -11.125),
        (5, 7.5),
        (6, 12.75),
    ] {
        let before = state.editor.project().clone();
        let command = paragraph_field_command(
            state.editor.selected_layer().unwrap(),
            index,
            &value.to_string(),
        )
        .unwrap()
        .unwrap();
        state.editor.execute(command).unwrap();
        let after = state.editor.project().clone();
        assert_eq!(
            paragraph_values(state.editor.selected_layer().unwrap())[index],
            value
        );
        assert!(!state.editor.can_redo());
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        state.editor.redo();
        assert_eq!(state.editor.project(), &after);
    }
}

#[test]
fn paragraph_style_commands_preserve_every_other_layer_field_and_all_sampled_tracks() {
    let mut state = scene();
    for (parameter, value) in [
        (TextParam::FontSize, 80.),
        (TextParam::Leading, 1.8),
        (TextParam::Tracking, 20.),
        (TextParam::StrokeWidth, 3.),
        (TextParam::FillOpacity, 40.),
    ] {
        state
            .editor
            .execute(scalar_animation_command(
                state.editor.selected_layer().unwrap(),
                parameter,
                0,
            ))
            .unwrap();
        let command = state
            .editor
            .selected_layer()
            .unwrap()
            .text_value_command(parameter, value, 60)
            .unwrap()
            .unwrap();
        state.editor.execute(command).unwrap();
    }
    state
        .editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    state
        .editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 60,
            text: "Future 👋\nparagraph\u{2029}line".into(),
        })
        .unwrap();
    let samples: Vec<_> = [0, 1, 17, 30, 59, 60, 80]
        .into_iter()
        .map(|frame| {
            let layer = state.editor.selected_layer().unwrap();
            (
                frame,
                layer.source_text_at(frame).unwrap().to_owned(),
                layer.text_typography_at(frame).unwrap(),
            )
        })
        .collect();
    for (index, key, value) in [
        (2, "paragraph_left_indent", 10.5),
        (3, "paragraph_right_indent", 20.25),
        (4, "paragraph_first_line_indent", -5.125),
        (5, "paragraph_space_before", 7.125),
        (6, "paragraph_space_after", 12.75),
    ] {
        let mut expected = serde_json::to_value(state.editor.selected_layer().unwrap()).unwrap();
        expected["text_style"]
            .as_object_mut()
            .unwrap()
            .insert(key.into(), serde_json::json!(value));
        let command = paragraph_field_command(
            state.editor.selected_layer().unwrap(),
            index,
            &value.to_string(),
        )
        .unwrap()
        .unwrap();
        state.editor.execute(command).unwrap();
        let layer = state.editor.selected_layer().unwrap();
        assert_eq!(serde_json::to_value(layer).unwrap(), expected);
        for (frame, text, typography) in &samples {
            assert_eq!(layer.source_text_at(*frame), Some(text.as_str()));
            assert_eq!(layer.text_typography_at(*frame).as_ref(), Some(typography));
        }
    }
}

#[test]
fn paragraph_field_edits_reject_point_locked_and_nontext_without_changes() {
    let mut state = scene();
    let command = paragraph_field_command(state.editor.selected_layer().unwrap(), 2, "25")
        .unwrap()
        .unwrap();
    state.editor.execute(command).unwrap();
    let values = paragraph_values(state.editor.selected_layer().unwrap());
    let mut style = state.editor.selected_layer().unwrap().text_style();
    style.paragraph = false;
    state
        .editor
        .execute(Command::SetTextStyle { id: 1, style })
        .unwrap();
    let point = state.editor.project().clone();
    for index in 0..PARAGRAPH_FIELDS {
        assert!(
            paragraph_field_command(state.editor.selected_layer().unwrap(), index, "30").is_err()
        );
    }
    assert_eq!(state.editor.project(), &point);
    assert_eq!(
        paragraph_values(state.editor.selected_layer().unwrap()),
        values
    );
    let command = paragraph_command(
        state.editor.selected_layer().unwrap(),
        0,
        ParagraphEdit::Mode(true),
    )
    .unwrap()
    .unwrap();
    state.editor.execute(command).unwrap();
    assert_eq!(
        paragraph_values(state.editor.selected_layer().unwrap()),
        values
    );
    state.editor.execute(Command::ToggleLocked(1)).unwrap();
    for index in 0..PARAGRAPH_FIELDS {
        assert!(
            paragraph_field_command(state.editor.selected_layer().unwrap(), index, "30").is_err()
        );
    }
    state.editor.execute(Command::AddSolid).unwrap();
    for index in 0..PARAGRAPH_FIELDS {
        assert!(
            paragraph_field_command(state.editor.selected_layer().unwrap(), index, "30").is_err()
        );
    }
}

#[test]
fn guarded_paragraph_submit_uses_authoritative_values_and_one_use_flush_receipts() {
    let mut state = scene();
    let (session, target) = bound(&state);
    assert!(
        session
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(4)))
    );
    assert_eq!(
        submit_paragraph_field(&session, &target, &mut state, 4, "-12.75", true, apply),
        "-12.75"
    );
    assert!(session.borrow_mut().finish_flush(&target, &state));
    let rebased = session
        .borrow_mut()
        .take_action(&target, &state, true)
        .unwrap();
    assert!(rebased.current(&state));
    assert!(
        session
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_none()
    );
    let command = paragraph_command(
        state.editor.selected_layer().unwrap(),
        0,
        ParagraphEdit::Align(TextAlign::Right),
    )
    .unwrap()
    .unwrap();
    assert!(apply(&mut state, command));
    assert_eq!(
        state.editor.selected_layer().unwrap().text_style().align,
        TextAlign::Right
    );
    state.editor.undo();
    assert_eq!(
        state
            .editor
            .selected_layer()
            .unwrap()
            .text_style()
            .paragraph_first_line_indent,
        -12.75
    );
    state.editor.undo();
    assert_eq!(
        state
            .editor
            .selected_layer()
            .unwrap()
            .text_style()
            .paragraph_first_line_indent,
        0.
    );
}

#[test]
fn guarded_paragraph_noop_flush_preserves_redo_and_invalid_or_foreign_drafts_block_actions() {
    let mut state = scene();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo".into(),
        })
        .unwrap();
    state.editor.undo();
    let before = state.editor.project().clone();
    let (session, target) = bound(&state);
    assert!(
        session
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(2)))
    );
    let generation = state.input_context_generation();
    assert_eq!(
        submit_paragraph_field(
            &session,
            &target,
            &mut state,
            2,
            " 0e0 ",
            true,
            |_, _| panic!("no-op dispatched")
        ),
        "0"
    );
    assert!(session.borrow_mut().finish_flush(&target, &state));
    assert!(
        session
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_some()
    );
    assert_eq!(state.input_context_generation(), generation);
    assert_eq!(state.editor.project(), &before);
    assert!(state.editor.can_redo());
    for text in ["NaN", "-1", "16384.001", "bad"] {
        let (session, target) = bound(&state);
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(2)))
        );
        assert_eq!(
            submit_paragraph_field(&session, &target, &mut state, 2, text, true, |_, _| panic!(
                "invalid dispatched"
            )),
            "0"
        );
        assert!(!session.borrow_mut().finish_flush(&target, &state));
        assert!(
            session
                .borrow_mut()
                .take_action(&target, &state, true)
                .is_none()
        );
        assert_eq!(state.editor.project(), &before);
        assert!(state.editor.can_redo());
    }
    let (session, target) = bound(&state);
    assert!(
        !session
            .borrow_mut()
            .prepare(&target, &state, Some("foreign-field".into()))
    );
    assert!(
        session
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(2)))
    );
    assert_eq!(
        submit_paragraph_field(&session, &target, &mut state, 3, "12", true, |_, _| panic!(
            "wrong field dispatched"
        )),
        "0"
    );
    assert!(!session.borrow_mut().finish_flush(&target, &state));
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn paragraph_bindings_reject_action_transport_and_selection_round_trips() {
    let mut state = scene();
    let original = state.editor.project().clone();
    for actions in [
        vec![Action::Seek(1), Action::Seek(0)],
        vec![Action::Play, Action::Play],
        vec![
            Action::SetTool(crate::editor::Tool::Text),
            Action::SetTool(state.tool),
        ],
        vec![
            Action::Edit(Command::RenameLayer {
                id: 1,
                name: "Changed".into(),
            }),
            Action::Undo,
        ],
    ] {
        let (session, target) = bound(&state);
        for action in actions {
            state.bulk_test_action(&action);
        }
        assert_eq!(state.editor.project(), &original);
        assert!(!target.current(&state));
        assert!(!session.borrow_mut().prepare(&target, &state, None));
        let before = state.editor.project().clone();
        submit_paragraph_field(&session, &target, &mut state, 2, "18", true, |_, _| {
            panic!("stale dispatched")
        });
        assert_eq!(state.editor.project(), &before);
    }
    let (session, target) = bound(&state);
    state.document_revision += 1;
    assert!(!target.current(&state));
    assert!(!session.borrow_mut().prepare(&target, &state, None));
    state.document_revision -= 1;
    state.selected_layers.insert(999);
    assert!(!target.current(&state));
}

#[test]
fn paragraph_bindings_block_source_sessions_marked_text_and_inactive_or_locked_edits() {
    let mut state = scene();
    let (session, target) = bound(&state);
    state.text_session = Some(
        crate::text_edit::Session::new(
            state.editor.project(),
            state.document_revision,
            0,
            Some(1),
            [0., 0.],
        )
        .unwrap(),
    );
    state
        .text_session
        .as_mut()
        .unwrap()
        .buffer
        .replace(None, "marked", true, None)
        .unwrap();
    assert!(state.text_session.as_ref().unwrap().buffer.marked.is_some());
    let before = state.editor.project().clone();
    assert!(ParagraphTarget::capture(&state).is_none());
    assert!(!session.borrow_mut().prepare(&target, &state, None));
    submit_paragraph_field(&session, &target, &mut state, 2, "20", true, |_, _| {
        panic!("source session dispatched")
    });
    assert_eq!(state.editor.project(), &before);
    assert!(state.text_session.as_ref().unwrap().buffer.marked.is_some());
    state.text_session = None;
    let (session, target) = bound(&state);
    submit_paragraph_field(&session, &target, &mut state, 2, "20", false, |_, _| {
        panic!("inactive dispatched")
    });
    assert_eq!(state.editor.project(), &before);
    state.editor.execute(Command::ToggleLocked(1)).unwrap();
    assert!(!target.current(&state));
    assert!(ParagraphTarget::capture(&state).is_none());
    state.editor.execute(Command::ToggleLocked(1)).unwrap();
    state.fonts_open = true;
    assert!(ParagraphTarget::capture(&state).is_none());
    state.fonts_open = false;
    state.editor.execute(Command::AddSolid).unwrap();
    assert!(ParagraphTarget::capture(&state).is_none());
}

#[test]
fn canceled_paragraph_presses_and_retired_callbacks_cannot_reuse_or_steal_receipts() {
    let mut state = scene();
    let (session, target) = bound(&state);
    assert!(session.borrow_mut().prepare(&target, &state, None));
    assert!(session.borrow_mut().finish_flush(&target, &state));
    assert!(session.borrow().armed.is_none());
    // Release outside: there is no click; later field entry is not prohibited.
    assert_eq!(
        submit_paragraph_field(&session, &target, &mut state, 2, "20", true, apply),
        "20"
    );
    assert!(
        session
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_none()
    );
    let next = session.borrow().target.clone().unwrap();
    assert!(
        session
            .borrow_mut()
            .prepare(&next, &state, Some(next.field_key(3)))
    );
    let before = state.editor.project().clone();
    submit_paragraph_field(&session, &target, &mut state, 2, "99", true, |_, _| {
        panic!("retired dispatched")
    });
    assert_eq!(state.editor.project(), &before);
    assert_eq!(session.borrow().armed, Some((next.key(), Some(3))));
    assert_eq!(
        submit_paragraph_field(&session, &next, &mut state, 3, "10", true, apply),
        "10"
    );
    assert!(session.borrow_mut().finish_flush(&next, &state));
    assert!(
        session
            .borrow_mut()
            .take_action(&next, &state, true)
            .is_some()
    );
}

#[test]
fn paragraph_fit_planning_includes_trailing_spacing_and_rechecks_finite_height() {
    let mut state = scene();
    let command = paragraph_field_command(state.editor.selected_layer().unwrap(), 6, "300")
        .unwrap()
        .unwrap();
    state.editor.execute(command).unwrap();
    let layer = state.editor.selected_layer().unwrap();
    let needed = crate::text_flow::fit_height(layer, 0).unwrap().ceil();
    assert!(needed > layer.height());
    let command = paragraph_command(layer, 0, ParagraphEdit::FitHeight)
        .unwrap()
        .unwrap();
    assert!(matches!(command, Command::SetTextBox { width: 400., height, .. } if height == needed));
    state.editor.execute(command).unwrap();
    assert!(
        paragraph_command(
            state.editor.selected_layer().unwrap(),
            0,
            ParagraphEdit::FitHeight
        )
        .unwrap()
        .is_none()
    );
    let command = paragraph_field_command(state.editor.selected_layer().unwrap(), 6, "16384")
        .unwrap()
        .unwrap();
    state.editor.execute(command).unwrap();
    assert!(
        paragraph_command(
            state.editor.selected_layer().unwrap(),
            0,
            ParagraphEdit::FitHeight
        )
        .is_err()
    );
}

#[test]
fn paragraph_guarded_receipts_expire_after_equal_source_undo_redo_and_failed_dispatch() {
    let mut state = scene();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "A changed title".into(),
        })
        .unwrap();
    let before = state.editor.project().clone();
    let (session, target) = bound(&state);
    state.bulk_test_action(&Action::Undo);
    state.bulk_test_action(&Action::Redo);
    assert_eq!(state.editor.project(), &before);
    assert!(!target.current(&state));
    assert!(!session.borrow_mut().prepare(&target, &state, None));
    let (session, target) = bound(&state);
    assert!(
        session
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(2)))
    );
    assert_eq!(
        submit_paragraph_field(&session, &target, &mut state, 2, "20", true, |_, _| false),
        "0"
    );
    assert_eq!(state.editor.project(), &before);
    assert!(!session.borrow_mut().finish_flush(&target, &state));
    let (session, target) = bound(&state);
    assert!(
        session
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(2)))
    );
    assert_eq!(
        submit_paragraph_field(&session, &target, &mut state, 2, "20", true, apply),
        "20"
    );
    assert!(session.borrow_mut().finish_flush(&target, &state));
    state.bulk_test_action(&Action::Seek(0));
    assert!(
        session
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_none()
    );
    assert_eq!(
        paragraph_values(state.editor.selected_layer().unwrap())[2],
        20.
    );
}
