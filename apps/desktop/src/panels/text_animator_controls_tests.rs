use super::*;
use libre_effects_core::{Editor, TextSelector, TextStyle};

fn scene() -> EditorState {
    let mut state = EditorState::default();
    state.editor = Editor::default();
    state
        .editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "A e\u{301}\n한글 👩‍💻".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 240.,
            name: "Animator controls".into(),
        })
        .unwrap();
    state.selected_layers = [1].into();
    state.editor.clear_history();
    state
}
fn bound(state: &EditorState) -> (Rc<RefCell<AnimatorInput>>, AnimatorTarget) {
    let input = Rc::new(RefCell::new(AnimatorInput::default()));
    input.borrow_mut().observe(state);
    let target = input.borrow().target.clone().unwrap();
    (input, target)
}
fn apply(state: &mut EditorState, command: Command) -> bool {
    state.bulk_test_action(&Action::Edit(command));
    state.status == "Edited"
}
fn set(state: &mut EditorState, index: usize, value: f64, frame: Frame) {
    let command = animator_field_command(
        state.editor.selected_layer().unwrap(),
        frame,
        index,
        &value.to_string(),
    )
    .unwrap()
    .unwrap();
    state.editor.execute(command).unwrap();
}
fn animation(state: &mut EditorState, parameter: TextParam, frame: Frame, edit: AnimatorEdit) {
    let command = animator_command(
        state.editor.selected_layer().unwrap(),
        parameter,
        frame,
        edit,
    )
    .unwrap()
    .unwrap();
    state.editor.execute(command).unwrap();
}

#[test]
fn animator_fields_preserve_precision_defaults_and_literal_bounds() {
    let state = scene();
    let layer = state.editor.selected_layer().unwrap();
    assert_eq!(ANIMATOR_FIELDS, 10);
    assert_eq!(
        ANIMATOR_PARAMETERS,
        [
            TextParam::AnimatorStart,
            TextParam::AnimatorEnd,
            TextParam::AnimatorPositionX,
            TextParam::AnimatorPositionY,
            TextParam::AnimatorOpacity,
            TextParam::AnimatorOffset,
            TextParam::AnimatorAmount,
            TextParam::AnimatorScaleX,
            TextParam::AnimatorScaleY,
            TextParam::AnimatorRotation
        ]
    );
    assert_eq!(
        ANIMATOR_LABELS,
        [
            "Range Start (%)",
            "Range End (%)",
            "Position X (px)",
            "Position Y (px)",
            "Opacity (%)",
            "Range Offset (%)",
            "Amount (%)",
            "Scale X (%)",
            "Scale Y (%)",
            "Rotation (°)"
        ]
    );
    assert_eq!(
        animator_values(layer, 0),
        [0., 100., 0., 0., 100., 0., 100., 100., 100., 0.]
    );
    for (index, parameter) in ANIMATOR_PARAMETERS.into_iter().enumerate() {
        let command = animator_field_command(layer, 17, index, " 12.123456789012345 ")
            .unwrap()
            .unwrap();
        assert!(matches!(command, Command::EditText { id: 1, parameter: p,
            edit: TrackEdit::Value { frame: 17, value } } if p == parameter && value == 12.123456789012345));
        let (min, max) = if index == 2 || index == 3 {
            (-1_000_000., 1_000_000.)
        } else if index == 5 {
            (-100., 100.)
        } else if index == 7 || index == 8 {
            (0., 1000.)
        } else if index == 9 {
            (-3600., 3600.)
        } else {
            (0., 100.)
        };
        for value in [min, max] {
            assert!(animator_field_command(layer, 0, index, &value.to_string()).is_ok());
        }
        for value in [min - 0.001, max + 0.001] {
            assert!(animator_field_command(layer, 0, index, &value.to_string()).is_err());
        }
        for text in ["NaN", "inf", "-inf", "1e400", "", "1 px", "bad"] {
            assert!(animator_field_command(layer, 0, index, text).is_err());
        }
    }
    assert!(animator_field_command(layer, 0, ANIMATOR_FIELDS, "1").is_err());
    assert!(animator_field_command(layer, 0, usize::MAX, "1").is_err());
}

#[test]
fn animator_sparse_noops_retain_redo_and_point_and_paragraph_allow_values() {
    let mut state = scene();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo".into(),
        })
        .unwrap();
    state.editor.undo();
    let original = state.editor.project().clone();
    for (index, value) in [0., 100., 0., 0., 100., 0., 100., 100., 100., 0.]
        .into_iter()
        .enumerate()
    {
        for input in [
            value.to_string(),
            format!("  {value:e}  "),
            format!("{value:.8}"),
        ] {
            assert!(
                animator_field_command(state.editor.selected_layer().unwrap(), 17, index, &input)
                    .unwrap()
                    .is_none()
            );
        }
    }
    for index in [0, 2, 3, 5, 9] {
        assert!(
            animator_field_command(state.editor.selected_layer().unwrap(), 0, index, "-0.0")
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(state.editor.project(), &original);
    assert!(state.editor.can_redo());
    assert!(!state.editor.can_undo());
    for paragraph in [false, true] {
        state
            .editor
            .execute(Command::SetTextStyle {
                id: 1,
                style: TextStyle {
                    paragraph,
                    paragraph_left_indent: 14.,
                    paragraph_first_line_indent: -3.,
                    ..Default::default()
                },
            })
            .unwrap();
        for index in 0..ANIMATOR_FIELDS {
            assert!(
                animator_field_command(state.editor.selected_layer().unwrap(), 0, index, "25")
                    .is_ok()
            );
        }
    }
    state.editor.execute(Command::ToggleLocked(1)).unwrap();
    for index in 0..ANIMATOR_FIELDS {
        assert!(
            animator_field_command(state.editor.selected_layer().unwrap(), 0, index, "25").is_err()
        );
    }
    state.editor.execute(Command::AddSolid).unwrap();
    assert!(animator_field_command(state.editor.selected_layer().unwrap(), 0, 0, "25").is_err());
}

#[test]
fn animator_explicit_actions_are_idempotent_and_preserve_independent_tracks() {
    let mut state = scene();
    let p = TextParam::AnimatorPositionX;
    for edit in [AnimatorEdit::Disable, AnimatorEdit::RemoveKey] {
        assert!(
            animator_command(state.editor.selected_layer().unwrap(), p, 0, edit)
                .unwrap()
                .is_none()
        );
    }
    animation(&mut state, p, 0, AnimatorEdit::Enable);
    let initial_track = state
        .editor
        .selected_layer()
        .unwrap()
        .track(PropertyPath::Text(p))
        .cloned()
        .unwrap();
    assert_eq!(initial_track.keys().len(), 1);
    for edit in [AnimatorEdit::Enable, AnimatorEdit::AddKey] {
        assert!(
            animator_command(state.editor.selected_layer().unwrap(), p, 0, edit)
                .unwrap()
                .is_none()
        );
    }
    set(&mut state, 2, 90., 60);
    animation(
        &mut state,
        TextParam::AnimatorOpacity,
        0,
        AnimatorEdit::Enable,
    );
    set(&mut state, 4, 40., 60);
    let independent = state
        .editor
        .selected_layer()
        .unwrap()
        .track(PropertyPath::Text(TextParam::AnimatorOpacity))
        .cloned();
    let sample = state
        .editor
        .selected_layer()
        .unwrap()
        .text_value_at(p, 30)
        .unwrap();
    animation(&mut state, p, 30, AnimatorEdit::AddKey);
    let keyed = state.editor.project().clone();
    assert_eq!(
        state
            .editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::Text(p))
            .unwrap()
            .keys()
            .len(),
        3
    );
    animation(&mut state, p, 30, AnimatorEdit::RemoveKey);
    assert_eq!(
        state
            .editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::Text(p))
            .unwrap()
            .keys()
            .len(),
        2
    );
    state.editor.undo();
    assert_eq!(state.editor.project(), &keyed);
    state.editor.redo();
    animation(&mut state, p, 30, AnimatorEdit::Disable);
    let disabled = state.editor.project().clone();
    let layer = state.editor.selected_layer().unwrap();
    assert!(
        layer
            .track(PropertyPath::Text(p))
            .unwrap()
            .keys()
            .is_empty()
    );
    assert_eq!(layer.text_value_at(p, 0), Some(sample));
    assert_eq!(layer.text_value_at(p, 60), Some(sample));
    assert_eq!(
        layer.track(PropertyPath::Text(TextParam::AnimatorOpacity)),
        independent.as_ref()
    );
    state.editor.undo();
    assert_eq!(
        state
            .editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::Text(p))
            .unwrap()
            .keys()
            .len(),
        2
    );
    state.editor.redo();
    assert_eq!(state.editor.project(), &disabled);
    assert!(
        animator_command(
            state.editor.selected_layer().unwrap(),
            TextParam::FontSize,
            0,
            AnimatorEdit::Enable
        )
        .is_err()
    );
}

#[test]
fn animator_pending_value_then_add_key_keeps_explicit_intent_and_separate_history() {
    let mut state = scene();
    animation(
        &mut state,
        TextParam::AnimatorPositionX,
        0,
        AnimatorEdit::Enable,
    );
    set(&mut state, 2, 60., 60);
    state.frame = 30;
    let before = state.editor.project().clone();
    let (input, target) = bound(&state);
    assert!(
        input
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(2)))
    );
    assert_eq!(
        submit_animator_field(&input, &target, &mut state, 2, "-12.75", true, apply),
        "-12.75"
    );
    assert!(input.borrow_mut().finish_flush(&target, &state));
    let rebased = input
        .borrow_mut()
        .take_action(&target, &state, true)
        .unwrap();
    assert!(rebased.current(&state));
    assert!(
        input
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_none()
    );
    // The pending animated value already added a key. A frozen Add intent
    // must never become ToggleKey and remove that newly created key.
    assert!(
        animator_command(
            state.editor.selected_layer().unwrap(),
            TextParam::AnimatorPositionX,
            30,
            AnimatorEdit::AddKey
        )
        .unwrap()
        .is_none()
    );
    let after_field = state.editor.project().clone();
    let command = animator_command(
        state.editor.selected_layer().unwrap(),
        TextParam::AnimatorOpacity,
        30,
        AnimatorEdit::Enable,
    )
    .unwrap()
    .unwrap();
    assert!(apply(&mut state, command));
    state.editor.undo();
    assert_eq!(state.editor.project(), &after_field);
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn animator_interpolated_display_is_lossless_and_unchanged_input_never_inserts_key() {
    let mut state = scene();
    for (index, p) in ANIMATOR_PARAMETERS.into_iter().enumerate() {
        animation(&mut state, p, 0, AnimatorEdit::Enable);
        set(&mut state, index, 37.123456789012345, 60);
    }
    state.frame = 17;
    let before = state.editor.project().clone();
    let values = animator_values(state.editor.selected_layer().unwrap(), 17);
    for (index, sample) in values.into_iter().enumerate() {
        let (input, target) = bound(&state);
        assert_eq!(
            target.display(&state, index).parse::<f64>().unwrap(),
            sample
        );
        assert_eq!(
            submit_animator_field(
                &input,
                &target,
                &mut state,
                index,
                &format!("  {sample:e}  "),
                true,
                |_, _| panic!("no-op dispatched")
            ),
            sample.to_string()
        );
        assert!(
            !state
                .editor
                .selected_layer()
                .unwrap()
                .track(PropertyPath::Text(ANIMATOR_PARAMETERS[index]))
                .unwrap()
                .keys()
                .contains_key(&17)
        );
    }
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn guarded_animator_noop_flush_preserves_redo_and_invalid_or_foreign_drafts_block_actions() {
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
        submit_animator_field(
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
    for text in ["NaN", "-1000000.001", "1000000.001", "bad"] {
        let (session, target) = bound(&state);
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(2)))
        );
        assert_eq!(
            submit_animator_field(&session, &target, &mut state, 2, text, true, |_, _| panic!(
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
        submit_animator_field(&session, &target, &mut state, 3, "12", true, |_, _| panic!(
            "wrong field dispatched"
        )),
        "0"
    );
    assert!(!session.borrow_mut().finish_flush(&target, &state));
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn animator_bindings_reject_action_transport_and_selection_round_trips() {
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
        submit_animator_field(&session, &target, &mut state, 2, "18", true, |_, _| {
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
fn animator_bindings_block_source_sessions_marked_text_and_inactive_or_locked_edits() {
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
    assert!(AnimatorTarget::capture(&state).is_none());
    assert!(!session.borrow_mut().prepare(&target, &state, None));
    submit_animator_field(&session, &target, &mut state, 2, "20", true, |_, _| {
        panic!("source session dispatched")
    });
    assert_eq!(state.editor.project(), &before);
    assert!(state.text_session.as_ref().unwrap().buffer.marked.is_some());
    state.text_session = None;
    let (session, target) = bound(&state);
    submit_animator_field(&session, &target, &mut state, 2, "20", false, |_, _| {
        panic!("inactive dispatched")
    });
    assert_eq!(state.editor.project(), &before);
    state.editor.execute(Command::ToggleLocked(1)).unwrap();
    assert!(!target.current(&state));
    assert!(AnimatorTarget::capture(&state).is_none());
    state.editor.execute(Command::ToggleLocked(1)).unwrap();
    state.fonts_open = true;
    assert!(AnimatorTarget::capture(&state).is_none());
    state.fonts_open = false;
    state.editor.execute(Command::AddSolid).unwrap();
    assert!(AnimatorTarget::capture(&state).is_none());
}

#[test]
fn canceled_animator_presses_and_retired_callbacks_cannot_reuse_or_steal_receipts() {
    let mut state = scene();
    let (session, target) = bound(&state);
    assert!(session.borrow_mut().prepare(&target, &state, None));
    assert!(session.borrow_mut().finish_flush(&target, &state));
    assert!(session.borrow().armed.is_none());
    // Release outside: there is no click; later field entry is not prohibited.
    assert_eq!(
        submit_animator_field(&session, &target, &mut state, 2, "20", true, apply),
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
    submit_animator_field(&session, &target, &mut state, 2, "99", true, |_, _| {
        panic!("retired dispatched")
    });
    assert_eq!(state.editor.project(), &before);
    assert_eq!(session.borrow().armed, Some((next.key(), Some(3))));
    assert_eq!(
        submit_animator_field(&session, &next, &mut state, 3, "10", true, apply),
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
fn animator_guarded_receipts_expire_after_equal_source_undo_redo_and_failed_dispatch() {
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
        submit_animator_field(&session, &target, &mut state, 2, "20", true, |_, _| false),
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
        submit_animator_field(&session, &target, &mut state, 2, "20", true, apply),
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
        animator_values(state.editor.selected_layer().unwrap(), state.frame)[2],
        20.
    );
}

#[test]
fn animator_each_property_keeps_value_after_last_key_removal_and_undo_restores_key() {
    for (index, parameter) in ANIMATOR_PARAMETERS.into_iter().enumerate() {
        let mut state = scene();
        set(&mut state, index, 18.123456789012345, 0);
        animation(&mut state, parameter, 17, AnimatorEdit::AddKey);
        let keyed = state.editor.project().clone();
        animation(&mut state, parameter, 17, AnimatorEdit::RemoveKey);
        let layer = state.editor.selected_layer().unwrap();
        assert!(
            layer
                .track(PropertyPath::Text(parameter))
                .unwrap()
                .keys()
                .is_empty()
        );
        for frame in [0, 17, 60] {
            assert_eq!(
                layer.text_value_at(parameter, frame),
                Some(18.123456789012345)
            );
        }
        assert!(
            animator_command(layer, parameter, 17, AnimatorEdit::RemoveKey)
                .unwrap()
                .is_none()
        );
        state.editor.undo();
        assert_eq!(state.editor.project(), &keyed);
    }
}

#[test]
fn timeline_animator_rejects_pending_inspector_draft_before_it_can_create_or_remove_key() {
    let mut state = scene();
    let parameter = TextParam::AnimatorPositionX;
    animation(&mut state, parameter, 0, AnimatorEdit::Enable);
    set(&mut state, 2, 60., 60);
    state.frame = 30;
    let before = state.editor.project().clone();
    let (inspector, inspector_target) = bound(&state);
    let timeline = TimelineAnimator::default();
    timeline.observe(&state);
    let target = timeline.input.borrow().target.clone().unwrap();
    for pending in [
        inspector_target.field_key(2),
        target.field_key(2),
        "foreign-input".into(),
    ] {
        // The actual Timeline guard runs this before commit_active. It cannot
        // acquire a field receipt even if a matching binding is supplied.
        assert!(
            !timeline
                .input
                .borrow_mut()
                .prepare_readonly(&target, &state, Some(pending))
        );
        assert!(!timeline.input.borrow_mut().finish_flush(&target, &state));
        assert!(
            timeline
                .input
                .borrow_mut()
                .take_action(&target, &state, true)
                .is_none()
        );
        assert_eq!(state.editor.project(), &before);
        assert!(
            !state
                .editor
                .selected_layer()
                .unwrap()
                .track(PropertyPath::Text(parameter))
                .unwrap()
                .keys()
                .contains_key(&30)
        );
    }
    // An explicit Enter in Properties is its own transaction. A held old
    // Timeline Add-key control must expire instead of removing the new key.
    assert_eq!(
        submit_animator_field(
            &inspector,
            &inspector_target,
            &mut state,
            2,
            "23.123456789",
            true,
            apply
        ),
        "23.123456789"
    );
    let committed = state.editor.project().clone();
    assert!(!target.current(&state));
    assert!(
        !timeline
            .input
            .borrow_mut()
            .prepare_readonly(&target, &state, None)
    );
    assert!(
        timeline
            .input
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_none()
    );
    assert!(
        animator_command(
            state.editor.selected_layer().unwrap(),
            parameter,
            30,
            AnimatorEdit::AddKey
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(state.editor.project(), &committed);
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn timeline_animator_readonly_receipts_are_one_use_and_round_trips_retire_them() {
    let mut state = scene();
    let timeline = TimelineAnimator::default();
    timeline.observe(&state);
    let target = timeline.input.borrow().target.clone().unwrap();
    assert!(
        timeline
            .input
            .borrow_mut()
            .prepare_readonly(&target, &state, None)
    );
    assert!(timeline.input.borrow_mut().finish_flush(&target, &state));
    let accepted = timeline
        .input
        .borrow_mut()
        .take_action(&target, &state, true)
        .unwrap();
    assert!(accepted.current(&state));
    assert!(
        timeline
            .input
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_none()
    );
    assert!(
        timeline
            .input
            .borrow_mut()
            .prepare_readonly(&target, &state, None)
    );
    assert!(timeline.input.borrow_mut().finish_flush(&target, &state));
    state.bulk_test_action(&Action::Seek(1));
    state.bulk_test_action(&Action::Seek(0));
    assert!(
        timeline
            .input
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_none()
    );
    timeline.observe(&state);
    let fresh = timeline.input.borrow().target.clone().unwrap();
    assert_ne!(fresh.key(), target.key());
    assert!(
        timeline
            .input
            .borrow_mut()
            .prepare_readonly(&fresh, &state, None)
    );
    assert!(timeline.input.borrow_mut().finish_flush(&fresh, &state));
    // Another click carrying pending input must retire an earlier permission.
    assert!(
        !timeline
            .input
            .borrow_mut()
            .prepare_readonly(&fresh, &state, Some("pending".into()))
    );
    assert!(
        timeline
            .input
            .borrow_mut()
            .take_action(&fresh, &state, true)
            .is_none()
    );
}

#[test]
fn timeline_animator_capture_rejects_source_sessions_and_modal_or_locked_owners() {
    let mut state = scene();
    let timeline = TimelineAnimator::default();
    timeline.observe(&state);
    let target = timeline.input.borrow().target.clone().unwrap();
    assert_eq!(target.layer, 1);
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
    assert!(
        !timeline
            .input
            .borrow_mut()
            .prepare_readonly(&target, &state, None)
    );
    timeline.observe(&state);
    assert!(timeline.input.borrow().target.is_none());
    assert!(state.text_session.as_ref().unwrap().buffer.marked.is_some());
    state.text_session = None;
    state.queue_open = true;
    timeline.observe(&state);
    assert!(timeline.input.borrow().target.is_none());
    state.queue_open = false;
    state.editor.execute(Command::ToggleLocked(1)).unwrap();
    timeline.observe(&state);
    assert!(timeline.input.borrow().target.is_none());
}

#[test]
fn animator_offset_pending_value_keeps_one_undo_and_expires_old_timeline_controls() {
    let mut state = scene();
    let parameter = TextParam::AnimatorOffset;
    let index = ANIMATOR_PARAMETERS
        .iter()
        .position(|p| *p == parameter)
        .unwrap();
    animation(&mut state, parameter, 0, AnimatorEdit::Enable);
    set(&mut state, index, 60., 60);
    state.frame = 30;
    state.editor.clear_history();
    let before = state.editor.project().clone();
    let (input, target) = bound(&state);
    let timeline = TimelineAnimator::default();
    timeline.observe(&state);
    let old_timeline = timeline.input.borrow().target.clone().unwrap();
    assert!(
        input
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(index)))
    );
    assert_eq!(
        submit_animator_field(
            &input,
            &target,
            &mut state,
            index,
            "-12.3456789012345",
            true,
            apply
        ),
        "-12.3456789012345"
    );
    assert!(input.borrow_mut().finish_flush(&target, &state));
    assert!(
        input
            .borrow_mut()
            .take_action(&target, &state, true)
            .is_some()
    );
    assert!(
        animator_command(
            state.editor.selected_layer().unwrap(),
            parameter,
            30,
            AnimatorEdit::AddKey
        )
        .unwrap()
        .is_none()
    );
    assert!(
        !timeline
            .input
            .borrow_mut()
            .prepare_readonly(&old_timeline, &state, None)
    );
    let committed = state.editor.project().clone();
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
    let (fresh_input, fresh_target) = bound(&state);
    for invalid in ["-100.001", "100.001", "NaN", "inf"] {
        assert!(fresh_input.borrow_mut().prepare(
            &fresh_target,
            &state,
            Some(fresh_target.field_key(index))
        ));
        assert_eq!(
            submit_animator_field(
                &fresh_input,
                &fresh_target,
                &mut state,
                index,
                invalid,
                true,
                |_, _| panic!("invalid Offset dispatched")
            ),
            "30"
        );
        assert!(!fresh_input.borrow_mut().finish_flush(&fresh_target, &state));
        assert_eq!(state.editor.project(), &before);
        assert!(state.editor.can_redo());
    }
    state.editor.redo();
    assert_eq!(state.editor.project(), &committed);
    submit_animator_field(&input, &target, &mut state, index, "99", true, |_, _| {
        panic!("stale Offset dispatched")
    });
    assert_eq!(state.editor.project(), &committed);
}

#[test]
fn selector_picker_labels_and_commands_preserve_independent_static_settings() {
    assert_eq!(
        TextSelectorUnits::ALL.map(TextSelectorUnits::label),
        ["Characters", "Words", "Lines"]
    );
    assert_eq!(
        TextSelectorShape::ALL.map(TextSelectorShape::label),
        ["Square", "Ramp Up", "Ramp Down", "Triangle"]
    );
    for units in TextSelectorUnits::ALL {
        for shape in TextSelectorShape::ALL {
            let mut state = scene();
            let selector = TextSelector { units, shape };
            state
                .editor
                .execute(Command::SetTextSelector { id: 1, selector })
                .unwrap();
            state.editor.clear_history();
            let layer = state.editor.selected_layer().unwrap();
            for action in [AnimatorAction::Units(units), AnimatorAction::Shape(shape)] {
                assert!(
                    animator_action_command(layer, 23, action)
                        .unwrap()
                        .is_none()
                );
            }
            for next_units in TextSelectorUnits::ALL {
                let command =
                    animator_action_command(layer, 23, AnimatorAction::Units(next_units)).unwrap();
                match command {
                    None => assert_eq!(units, next_units),
                    Some(Command::SetTextSelector {
                        id: 1,
                        selector: next,
                    }) => {
                        assert_eq!(
                            next,
                            TextSelector {
                                units: next_units,
                                shape
                            }
                        );
                    }
                    _ => panic!("Units must change only the static selector"),
                }
            }
            for next_shape in TextSelectorShape::ALL {
                let command =
                    animator_action_command(layer, 23, AnimatorAction::Shape(next_shape)).unwrap();
                match command {
                    None => assert_eq!(shape, next_shape),
                    Some(Command::SetTextSelector {
                        id: 1,
                        selector: next,
                    }) => {
                        assert_eq!(
                            next,
                            TextSelector {
                                units,
                                shape: next_shape
                            }
                        );
                    }
                    _ => panic!("Shape must change only the static selector"),
                }
            }
            assert!(!state.editor.can_undo());
            state.editor.execute(Command::ToggleLocked(1)).unwrap();
            assert!(
                animator_action_command(
                    state.editor.selected_layer().unwrap(),
                    0,
                    AnimatorAction::Units(units)
                )
                .is_err()
            );
            assert!(
                animator_action_command(
                    state.editor.selected_layer().unwrap(),
                    0,
                    AnimatorAction::Shape(shape)
                )
                .is_err()
            );
            state.editor.execute(Command::AddSolid).unwrap();
            assert!(
                animator_action_command(
                    state.editor.selected_layer().unwrap(),
                    0,
                    AnimatorAction::Units(units)
                )
                .is_err()
            );
            assert!(
                animator_action_command(
                    state.editor.selected_layer().unwrap(),
                    0,
                    AnimatorAction::Shape(shape)
                )
                .is_err()
            );
        }
    }
}

#[test]
fn selector_picker_noop_pending_amount_preserves_exact_source_and_redo() {
    let mut state = scene();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo".into(),
        })
        .unwrap();
    state.editor.undo();
    let before = serde_json::to_vec(state.editor.project()).unwrap();
    for action in [
        AnimatorAction::Units(TextSelectorUnits::Graphemes),
        AnimatorAction::Shape(TextSelectorShape::Square),
    ] {
        let (session, target) = bound(&state);
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(6)))
        );
        assert_eq!(
            submit_animator_field(
                &session,
                &target,
                &mut state,
                6,
                " 1e2 ",
                true,
                |_, _| panic!("no-op amount dispatched")
            ),
            "100"
        );
        assert!(session.borrow_mut().finish_flush(&target, &state));
        assert!(submit_animator_action(
            &session,
            &target,
            &mut state,
            true,
            action,
            |_, _| panic!("no-op selector dispatched")
        ));
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            true,
            action,
            |_, _| panic!("receipt reused")
        ));
        assert_eq!(serde_json::to_vec(state.editor.project()).unwrap(), before);
        assert!(!state.editor.can_undo());
        assert!(state.editor.can_redo());
    }
}

#[test]
fn selector_picker_pending_amount_is_precise_and_has_separate_undo_history() {
    for action in [
        AnimatorAction::Units(TextSelectorUnits::Words),
        AnimatorAction::Shape(TextSelectorShape::RampUp),
    ] {
        let mut state = scene();
        let selector = TextSelector {
            units: TextSelectorUnits::Lines,
            shape: TextSelectorShape::Triangle,
        };
        state
            .editor
            .execute(Command::SetTextSelector { id: 1, selector })
            .unwrap();
        animation(
            &mut state,
            TextParam::AnimatorAmount,
            0,
            AnimatorEdit::Enable,
        );
        set(&mut state, 6, 50., 60);
        state.frame = 30;
        state.editor.clear_history();
        let before = state.editor.project().clone();
        let (session, target) = bound(&state);
        let timeline = TimelineAnimator::default();
        timeline.observe(&state);
        let old_timeline = timeline.input.borrow().target.clone().unwrap();
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(6)))
        );
        assert_eq!(
            submit_animator_field(
                &session,
                &target,
                &mut state,
                6,
                "12.123456789012345",
                true,
                apply
            ),
            12.123456789012345_f64.to_string()
        );
        let after_amount = state.editor.project().clone();
        assert_eq!(
            state.editor.selected_layer().unwrap().text_selector(),
            selector
        );
        assert!(session.borrow_mut().finish_flush(&target, &state));
        assert!(submit_animator_action(
            &session, &target, &mut state, true, action, apply
        ));
        let after_selector = state.editor.project().clone();
        let layer = state.editor.selected_layer().unwrap();
        assert_eq!(
            layer.text_value_at(TextParam::AnimatorAmount, 30),
            Some(12.123456789012345)
        );
        assert_eq!(
            layer
                .track(PropertyPath::Text(TextParam::AnimatorAmount))
                .unwrap()
                .keys()
                .len(),
            3
        );
        assert_eq!(layer.source_text_at(30), Some("A e\u{301}\n한글 👩‍💻"));
        let expected = match action {
            AnimatorAction::Units(units) => TextSelector { units, ..selector },
            AnimatorAction::Shape(shape) => TextSelector { shape, ..selector },
            _ => unreachable!(),
        };
        assert_eq!(layer.text_selector(), expected);
        assert!(!old_timeline.current(&state));
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            true,
            action,
            |_, _| panic!("picker receipt reused")
        ));
        state.editor.undo();
        assert_eq!(state.editor.project(), &after_amount);
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
        state.editor.redo();
        state.editor.redo();
        assert_eq!(state.editor.project(), &after_selector);
    }
}

#[test]
fn selector_picker_invalid_or_foreign_pending_input_blocks_every_static_action() {
    for action in [
        AnimatorAction::Units(TextSelectorUnits::Words),
        AnimatorAction::Shape(TextSelectorShape::Triangle),
    ] {
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
        for invalid in ["-0.001", "100.001", "NaN", "inf", ""] {
            let (session, target) = bound(&state);
            assert!(
                session
                    .borrow_mut()
                    .prepare(&target, &state, Some(target.field_key(6)))
            );
            assert_eq!(
                submit_animator_field(
                    &session,
                    &target,
                    &mut state,
                    6,
                    invalid,
                    true,
                    |_, _| panic!("invalid Amount dispatched")
                ),
                "100"
            );
            assert!(!session.borrow_mut().finish_flush(&target, &state));
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                true,
                action,
                |_, _| panic!("selector after invalid input dispatched")
            ));
            assert_eq!(state.editor.project(), &before);
            assert!(state.editor.can_redo());
            assert!(!state.editor.can_undo());
        }
        for pending in ["source-text-pending", "foreign-field"] {
            let (session, target) = bound(&state);
            assert!(
                !session
                    .borrow_mut()
                    .prepare(&target, &state, Some(pending.into()))
            );
            assert!(!session.borrow_mut().finish_flush(&target, &state));
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                true,
                action,
                |_, _| panic!("foreign pending input dispatched")
            ));
            assert_eq!(state.editor.project(), &before);
        }
    }
}

#[test]
fn selector_picker_activation_rejects_inactive_windows_modifiers_field_focus_and_ime() {
    for pointer in [false, true] {
        assert!(animator_activation_allowed(
            pointer, true, false, false, false, false
        ));
        assert!(!animator_activation_allowed(
            pointer, false, false, false, false, false
        ));
        assert!(!animator_activation_allowed(
            pointer, true, true, false, false, false
        ));
        assert!(!animator_activation_allowed(
            pointer, true, false, true, false, false
        ));
        assert_eq!(
            animator_activation_allowed(pointer, true, false, false, true, false),
            pointer
        );
        assert_eq!(
            animator_activation_allowed(pointer, true, false, false, false, true),
            pointer
        );
    }
    let mut state = scene();
    let (session, target) = bound(&state);
    // Keyboard activation has no pointer receipt but still requires the exact
    // rendered source. The event handler independently checks focus and IME.
    assert!(submit_animator_action(
        &session,
        &target,
        &mut state,
        false,
        AnimatorAction::Units(TextSelectorUnits::Words),
        apply
    ));
    assert!(!submit_animator_action(
        &session,
        &target,
        &mut state,
        false,
        AnimatorAction::Shape(TextSelectorShape::Triangle),
        |_, _| panic!("stale keyboard button dispatched")
    ));
    assert_eq!(
        state.editor.selected_layer().unwrap().text_selector(),
        TextSelector {
            units: TextSelectorUnits::Words,
            shape: TextSelectorShape::Square
        }
    );
}

#[test]
fn selector_picker_receipts_expire_after_source_lock_selection_history_and_transport_changes() {
    let interruptions: &[fn(&mut EditorState)] = &[
        |state| {
            state.bulk_test_action(&Action::Seek(1));
            state.bulk_test_action(&Action::Seek(0));
        },
        |state| {
            state.bulk_test_action(&Action::Play);
            state.bulk_test_action(&Action::Play);
        },
        |state| {
            state.bulk_test_action(&Action::Edit(Command::RenameLayer {
                id: 1,
                name: "Changed".into(),
            }));
            state.bulk_test_action(&Action::Undo);
        },
        |state| {
            state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
            state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
        },
        |state| {
            state.selected_layers.insert(999);
        },
        |state| {
            state.document_revision += 1;
        },
        |state| {
            state.bulk_test_action(&Action::Edit(Command::EditSourceText {
                id: 1,
                frame: 0,
                text: "New source".into(),
            }));
        },
    ];
    for interrupt in interruptions {
        for action in [
            AnimatorAction::Units(TextSelectorUnits::Lines),
            AnimatorAction::Shape(TextSelectorShape::RampDown),
        ] {
            let mut state = scene();
            let (session, target) = bound(&state);
            assert!(session.borrow_mut().prepare(&target, &state, None));
            assert!(session.borrow_mut().finish_flush(&target, &state));
            interrupt(&mut state);
            let before = state.editor.project().clone();
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                true,
                action,
                |_, _| panic!("stale picker dispatched")
            ));
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                false,
                action,
                |_, _| panic!("stale keyboard picker dispatched")
            ));
            assert_eq!(state.editor.project(), &before);
        }
    }
}

#[test]
fn selector_picker_source_text_session_keeps_marked_text_and_rejects_stale_clicks() {
    let mut state = scene();
    let (session, target) = bound(&state);
    assert!(session.borrow_mut().prepare(&target, &state, None));
    assert!(session.borrow_mut().finish_flush(&target, &state));
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
    let before = state.editor.project().clone();
    for action in [
        AnimatorAction::Units(TextSelectorUnits::Words),
        AnimatorAction::Shape(TextSelectorShape::Triangle),
    ] {
        assert!(!session.borrow_mut().prepare(&target, &state, None));
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            true,
            action,
            |_, _| panic!("source-text picker dispatched")
        ));
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            false,
            action,
            |_, _| panic!("source-text keyboard picker dispatched")
        ));
        assert!(state.text_session.as_ref().unwrap().buffer.marked.is_some());
        assert_eq!(state.editor.project(), &before);
        assert!(AnimatorTarget::capture(&state).is_none());
    }
}

#[test]
fn animator_transform_display_order_keeps_stable_bindings_after_amount() {
    assert_eq!(
        ANIMATOR_DISPLAY_ORDER.map(|index| ANIMATOR_LABELS[index]),
        [
            "Range Start (%)",
            "Range End (%)",
            "Range Offset (%)",
            "Amount (%)",
            "Scale X (%)",
            "Scale Y (%)",
            "Rotation (°)",
            "Position X (px)",
            "Position Y (px)",
            "Opacity (%)",
        ]
    );
    assert_eq!(
        ANIMATOR_DISPLAY_ORDER
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        (0..ANIMATOR_FIELDS).collect()
    );
    assert_eq!(
        &ANIMATOR_PARAMETERS[7..],
        &[
            TextParam::AnimatorScaleX,
            TextParam::AnimatorScaleY,
            TextParam::AnimatorRotation,
        ]
    );
}

#[test]
fn animator_transform_pending_value_add_key_and_disable_keep_precise_separate_history() {
    for (index, parameter, value) in [
        (7, TextParam::AnimatorScaleX, 125.12345678901235),
        (8, TextParam::AnimatorScaleY, 75.12345678901235),
        (9, TextParam::AnimatorRotation, -450.1234567890123),
    ] {
        let mut state = scene();
        for (other_index, other_parameter) in ANIMATOR_PARAMETERS.into_iter().enumerate().skip(7) {
            animation(&mut state, other_parameter, 0, AnimatorEdit::Enable);
            set(&mut state, other_index, 200., 60);
        }
        state.frame = 30;
        state.editor.clear_history();
        let before = state.editor.project().clone();
        let timeline = TimelineAnimator::default();
        timeline.observe(&state);
        let old_timeline = timeline.input.borrow().target.clone().unwrap();
        let (input, target) = bound(&state);
        assert!(
            input
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(index)))
        );
        assert_eq!(
            submit_animator_field(
                &input,
                &target,
                &mut state,
                index,
                &value.to_string(),
                true,
                apply
            ),
            value.to_string()
        );
        assert!(input.borrow_mut().finish_flush(&target, &state));
        assert!(submit_animator_action(
            &input,
            &target,
            &mut state,
            true,
            AnimatorAction::Animation {
                parameter,
                edit: AnimatorEdit::AddKey
            },
            |_, _| panic!("frozen Add must not remove the pending field's new key")
        ));
        assert!(!old_timeline.current(&state));
        let after_field = state.editor.project().clone();
        let layer = state.editor.selected_layer().unwrap();
        let track = layer.track(PropertyPath::Text(parameter)).unwrap();
        assert_eq!(track.keys().len(), 3);
        assert_eq!(track.keys()[&30].value, value);
        for other_parameter in ANIMATOR_PARAMETERS
            .into_iter()
            .skip(7)
            .filter(|p| *p != parameter)
        {
            assert_eq!(
                layer.track(PropertyPath::Text(other_parameter)),
                before
                    .composition()
                    .layer(1)
                    .unwrap()
                    .track(PropertyPath::Text(other_parameter))
            );
        }
        let (input, target) = bound(&state);
        assert!(input.borrow_mut().prepare(&target, &state, None));
        assert!(input.borrow_mut().finish_flush(&target, &state));
        assert!(submit_animator_action(
            &input,
            &target,
            &mut state,
            true,
            AnimatorAction::Animation {
                parameter,
                edit: AnimatorEdit::Disable
            },
            apply
        ));
        let layer = state.editor.selected_layer().unwrap();
        assert!(
            layer
                .track(PropertyPath::Text(parameter))
                .unwrap()
                .keys()
                .is_empty()
        );
        assert_eq!(layer.text_value_at(parameter, 0), Some(value));
        assert_eq!(layer.text_value_at(parameter, 60), Some(value));
        let disabled = state.editor.project().clone();
        state.editor.undo();
        assert_eq!(state.editor.project(), &after_field);
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
        state.editor.redo();
        state.editor.redo();
        assert_eq!(state.editor.project(), &disabled);
    }
}

#[test]
fn animator_transform_noop_invalid_and_foreign_pending_inputs_preserve_sparse_source_and_redo() {
    for (index, parameter, default, invalid) in [
        (
            7,
            TextParam::AnimatorScaleX,
            "100",
            ["-0.001", "1000.001", "NaN", "inf"],
        ),
        (
            8,
            TextParam::AnimatorScaleY,
            "100",
            ["-0.001", "1000.001", "NaN", "inf"],
        ),
        (
            9,
            TextParam::AnimatorRotation,
            "0",
            ["-3600.001", "3600.001", "NaN", "inf"],
        ),
    ] {
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
        let (input, target) = bound(&state);
        assert!(
            input
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(index)))
        );
        assert_eq!(
            submit_animator_field(
                &input,
                &target,
                &mut state,
                index,
                &format!(" {default}e0 "),
                true,
                |_, _| panic!("neutral value dispatched")
            ),
            default
        );
        assert!(input.borrow_mut().finish_flush(&target, &state));
        assert!(submit_animator_action(
            &input,
            &target,
            &mut state,
            true,
            AnimatorAction::Animation {
                parameter,
                edit: AnimatorEdit::RemoveKey
            },
            |_, _| panic!("missing key removal dispatched")
        ));
        for value in invalid {
            let (input, target) = bound(&state);
            assert!(
                input
                    .borrow_mut()
                    .prepare(&target, &state, Some(target.field_key(index)))
            );
            assert_eq!(
                submit_animator_field(
                    &input,
                    &target,
                    &mut state,
                    index,
                    value,
                    true,
                    |_, _| panic!("invalid transform dispatched")
                ),
                default
            );
            assert!(!input.borrow_mut().finish_flush(&target, &state));
            assert!(!submit_animator_action(
                &input,
                &target,
                &mut state,
                true,
                AnimatorAction::Animation {
                    parameter,
                    edit: AnimatorEdit::Enable
                },
                |_, _| panic!("animation after invalid value dispatched")
            ));
        }
        let (input, target) = bound(&state);
        assert!(
            !input
                .borrow_mut()
                .prepare(&target, &state, Some("foreign-input".into()))
        );
        assert!(!input.borrow_mut().finish_flush(&target, &state));
        assert!(!submit_animator_action(
            &input,
            &target,
            &mut state,
            true,
            AnimatorAction::Animation {
                parameter,
                edit: AnimatorEdit::Enable
            },
            |_, _| panic!("animation after foreign field dispatched")
        ));
        assert_eq!(state.editor.project(), &before);
        assert!(
            state
                .editor
                .selected_layer()
                .unwrap()
                .track(PropertyPath::Text(parameter))
                .is_none()
        );
        assert!(!state.editor.can_undo());
        assert!(state.editor.can_redo());
    }
}

#[test]
fn animator_transform_zero_scale_and_unwrapped_rotation_endpoints_are_undoable() {
    for (index, parameter, values) in [
        (7, TextParam::AnimatorScaleX, [0., 1000.]),
        (8, TextParam::AnimatorScaleY, [0., 1000.]),
        (9, TextParam::AnimatorRotation, [-3600., 3600.]),
    ] {
        for value in values {
            let mut state = scene();
            let before = state.editor.project().clone();
            let (input, target) = bound(&state);
            assert_eq!(
                submit_animator_field(
                    &input,
                    &target,
                    &mut state,
                    index,
                    &value.to_string(),
                    true,
                    apply
                ),
                value.to_string()
            );
            let layer = state.editor.selected_layer().unwrap();
            assert_eq!(layer.text_value_at(parameter, 17), Some(value));
            assert_eq!(layer.source_text_at(0), Some("A e\u{301}\n한글 👩‍💻"));
            assert_eq!(state.frame, 0);
            let after = state.editor.project().clone();
            state.editor.undo();
            assert_eq!(state.editor.project(), &before);
            assert!(!state.editor.can_undo());
            state.editor.redo();
            assert_eq!(state.editor.project(), &after);
        }
    }
}

#[test]
fn animator_transform_fields_and_timeline_reject_pending_locked_and_stale_contexts() {
    let interruptions: [fn(&mut EditorState); 6] = [
        |state| {
            state.bulk_test_action(&Action::Seek(1));
            state.bulk_test_action(&Action::Seek(0));
        },
        |state| {
            state.bulk_test_action(&Action::Play);
            state.bulk_test_action(&Action::Play);
        },
        |state| {
            state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
        },
        |state| {
            state.selected_layers.insert(999);
        },
        |state| {
            state.document_revision += 1;
        },
        |state| {
            state.bulk_test_action(&Action::Edit(Command::EditSourceText {
                id: 1,
                frame: 0,
                text: "Replacement".into(),
            }));
            state.bulk_test_action(&Action::Undo);
        },
    ];
    for (index, parameter) in ANIMATOR_PARAMETERS.into_iter().enumerate().skip(7) {
        for interrupt in interruptions {
            let mut state = scene();
            let (input, target) = bound(&state);
            let timeline = TimelineAnimator::default();
            timeline.observe(&state);
            let timeline_target = timeline.input.borrow().target.clone().unwrap();
            let before = state.editor.project().clone();
            assert!(!timeline.input.borrow_mut().prepare_readonly(
                &timeline_target,
                &state,
                Some(target.field_key(index))
            ));
            assert!(
                !timeline
                    .input
                    .borrow_mut()
                    .finish_flush(&timeline_target, &state)
            );
            assert!(!submit_animator_action(
                &timeline.input,
                &timeline_target,
                &mut state,
                true,
                AnimatorAction::Animation {
                    parameter,
                    edit: AnimatorEdit::AddKey
                },
                |_, _| panic!("Timeline flushed pending transform input")
            ));
            assert_eq!(state.editor.project(), &before);
            assert!(input.borrow_mut().prepare(&target, &state, None));
            assert!(input.borrow_mut().finish_flush(&target, &state));
            assert!(
                timeline
                    .input
                    .borrow_mut()
                    .prepare_readonly(&timeline_target, &state, None)
            );
            assert!(
                timeline
                    .input
                    .borrow_mut()
                    .finish_flush(&timeline_target, &state)
            );
            interrupt(&mut state);
            let interrupted = state.editor.project().clone();
            submit_animator_field(&input, &target, &mut state, index, "25", true, |_, _| {
                panic!("stale transform field dispatched")
            });
            for (session, receipt) in [(&input, &target), (&timeline.input, &timeline_target)] {
                for pointer in [false, true] {
                    assert!(!submit_animator_action(
                        session,
                        receipt,
                        &mut state,
                        pointer,
                        AnimatorAction::Animation {
                            parameter,
                            edit: AnimatorEdit::Enable
                        },
                        |_, _| panic!("stale transform animation dispatched")
                    ));
                }
            }
            assert_eq!(state.editor.project(), &interrupted);
        }
    }
}

fn add_secondary(state: &mut EditorState) -> u64 {
    state
        .editor
        .execute(Command::AddTextRangeSelector { id: 1 })
        .unwrap();
    state
        .editor
        .selected_layer()
        .unwrap()
        .text_range_selectors()
        .last()
        .unwrap()
        .id
}

fn select_secondary(
    state: &mut EditorState,
    session: &Rc<RefCell<AnimatorInput>>,
    target: &AnimatorTarget,
    selector: Option<u64>,
) -> AnimatorTarget {
    assert!(submit_animator_action(
        session,
        target,
        state,
        false,
        AnimatorAction::SelectSelector(selector),
        |_, _| panic!("row selection changed the document")
    ));
    session.borrow().target.clone().unwrap()
}

#[test]
fn secondary_selector_fields_are_precise_static_transactions_and_leave_all_primary_tracks_intact() {
    for index in RANGE_FIELDS {
        let mut state = scene();
        for (i, parameter) in ANIMATOR_PARAMETERS.into_iter().enumerate() {
            animation(&mut state, parameter, 0, AnimatorEdit::Enable);
            set(&mut state, i, 37., 60);
        }
        let id = add_secondary(&mut state);
        state.frame = 30;
        state.editor.clear_history();
        let before = state.editor.project().clone();
        let (session, primary) = bound(&state);
        let target = select_secondary(&mut state, &session, &primary, Some(id));
        assert_eq!(target.selector, Some(id));
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(index)))
        );
        assert_eq!(
            submit_animator_field(
                &session,
                &target,
                &mut state,
                index,
                " 12.123456789012345 ",
                true,
                apply
            ),
            12.123456789012345_f64.to_string()
        );
        assert!(session.borrow_mut().finish_flush(&target, &state));
        let layer = state.editor.selected_layer().unwrap();
        assert_eq!(
            selector_values(layer, 30, Some(id))[index],
            12.123456789012345
        );
        for parameter in ANIMATOR_PARAMETERS {
            assert_eq!(
                layer.track(PropertyPath::Text(parameter)),
                before
                    .composition()
                    .layer(1)
                    .unwrap()
                    .track(PropertyPath::Text(parameter))
            );
        }
        let after = state.editor.project().clone();
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo(), "one source edit must be one undo");
        state.editor.redo();
        assert_eq!(state.editor.project(), &after);
    }
}

#[test]
fn secondary_selector_noops_invalid_values_and_missing_ids_preserve_redo() {
    let mut state = scene();
    let id = add_secondary(&mut state);
    state.editor.clear_history();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo".into(),
        })
        .unwrap();
    state.editor.undo();
    let before = state.editor.project().clone();
    for index in RANGE_FIELDS {
        let value = selector_values(state.editor.selected_layer().unwrap(), 0, Some(id))[index];
        let (session, primary) = bound(&state);
        let target = select_secondary(&mut state, &session, &primary, Some(id));
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(index)))
        );
        assert_eq!(
            submit_animator_field(
                &session,
                &target,
                &mut state,
                index,
                &format!(" {value:e} "),
                true,
                |_, _| panic!("static no-op dispatched")
            ),
            value.to_string()
        );
        assert!(session.borrow_mut().finish_flush(&target, &state));
        assert!(state.editor.can_redo());
        let target = session.borrow().target.clone().unwrap();
        let (min, max) = ANIMATOR_PARAMETERS[index].bounds();
        for invalid in [
            "NaN".to_string(),
            "inf".into(),
            "1e400".into(),
            "bad".into(),
            (min - 0.001).to_string(),
            (max + 0.001).to_string(),
        ] {
            assert!(
                session
                    .borrow_mut()
                    .prepare(&target, &state, Some(target.field_key(index)))
            );
            submit_animator_field(
                &session,
                &target,
                &mut state,
                index,
                &invalid,
                true,
                |_, _| panic!("invalid static value dispatched"),
            );
            assert!(!session.borrow_mut().finish_flush(&target, &state));
            assert_eq!(state.editor.project(), &before);
            assert!(state.editor.can_redo());
        }
        assert!(
            selector_field_command(
                state.editor.selected_layer().unwrap(),
                0,
                Some(u64::MAX),
                index,
                "25"
            )
            .is_err()
        );
    }
    for id in [0, u64::MAX] {
        for index in 0..ANIMATOR_FIELDS {
            assert!(
                selector_field_command(
                    state.editor.selected_layer().unwrap(),
                    0,
                    Some(id),
                    index,
                    "25"
                )
                .is_err()
            );
        }
        for parameter in TextSelectorParam::ALL {
            for edit in [
                AnimatorEdit::Enable,
                AnimatorEdit::Disable,
                AnimatorEdit::AddKey,
                AnimatorEdit::RemoveKey,
            ] {
                assert!(
                    selector_animation_command(
                        state.editor.selected_layer().unwrap(),
                        id,
                        parameter,
                        0,
                        edit
                    )
                    .is_err()
                );
            }
        }
    }
    assert_eq!(state.editor.project(), &before);
    assert!(state.editor.can_redo());
}

#[test]
fn secondary_selector_selection_round_trip_retires_fields_and_held_actions_without_history() {
    let mut state = scene();
    let a = add_secondary(&mut state);
    let b = add_secondary(&mut state);
    state.editor.clear_history();
    let before = state.editor.project().clone();
    let generation = state.input_context_generation();
    let (session, primary) = bound(&state);
    let old_a = select_secondary(&mut state, &session, &primary, Some(a));
    assert!(session.borrow_mut().prepare(&old_a, &state, None));
    assert!(session.borrow_mut().finish_flush(&old_a, &state));
    let target_b = select_secondary(&mut state, &session, &old_a, Some(b));
    let new_a = select_secondary(&mut state, &session, &target_b, Some(a));
    assert_ne!(old_a.key(), new_a.key());
    assert_ne!(primary.key(), new_a.key());
    assert_eq!(state.input_context_generation(), generation);
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
    assert!(
        session
            .borrow_mut()
            .prepare(&new_a, &state, Some(new_a.field_key(5)))
    );
    for old in [&old_a, &target_b, &primary] {
        submit_animator_field(&session, old, &mut state, 5, "99", true, |_, _| {
            panic!("old selector field revived")
        });
        assert!(!session.borrow_mut().prepare(old, &state, None));
        assert!(!session.borrow_mut().finish_flush(old, &state));
        for pointer in [true, false] {
            assert!(!submit_animator_action(
                &session,
                old,
                &mut state,
                pointer,
                AnimatorAction::RemoveSelector(a),
                |_, _| panic!("old selector action revived")
            ));
        }
        assert_eq!(
            session.borrow().armed,
            Some((new_a.key(), Some(5))),
            "old callbacks must not steal the fresh selector receipt"
        );
    }
    assert_eq!(
        submit_animator_field(&session, &new_a, &mut state, 5, "-12", true, apply),
        "-12"
    );
    assert!(session.borrow_mut().finish_flush(&new_a, &state));
}

#[test]
fn secondary_selector_pending_field_reorders_by_id_with_two_atomic_undos() {
    let mut state = scene();
    let a = add_secondary(&mut state);
    let b = add_secondary(&mut state);
    secondary_animation(&mut state, a, 0, 0, AnimatorEdit::Enable);
    set_secondary(&mut state, a, 0, 60., 60);
    state.frame = 30;
    state.editor.clear_history();
    let before = state.editor.project().clone();
    let (session, primary) = bound(&state);
    let target = select_secondary(&mut state, &session, &primary, Some(a));
    assert!(
        session
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(0)))
    );
    submit_animator_field(&session, &target, &mut state, 0, "25", true, apply);
    let after_field = state.editor.project().clone();
    assert!(session.borrow_mut().finish_flush(&target, &state));
    assert!(submit_animator_action(
        &session,
        &target,
        &mut state,
        true,
        AnimatorAction::MoveSelector {
            selector: a,
            index: 1
        },
        apply
    ));
    let layer = state.editor.selected_layer().unwrap();
    assert_eq!(
        layer
            .text_range_selectors()
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![b, a]
    );
    assert_eq!(
        range_selector(layer, a)
            .unwrap()
            .value_at(TextSelectorParam::Start, 30),
        25.
    );
    assert_eq!(range_selector(layer, a).unwrap().start, 0.);
    assert_eq!(range_selector(layer, b).unwrap().start, 0.);
    session.borrow_mut().observe(&state);
    let fresh = session.borrow().target.clone().unwrap();
    assert_eq!(fresh.selector, Some(a));
    assert_ne!(fresh.key(), target.key());
    assert_eq!(fresh.display(&state, 0), "25");
    state.editor.undo();
    assert_eq!(state.editor.project(), &after_field);
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
}

#[test]
fn secondary_selector_remove_readd_and_undo_cannot_inherit_retired_callbacks() {
    let mut state = scene();
    let a = add_secondary(&mut state);
    state.editor.clear_history();
    let (session, primary) = bound(&state);
    let target = select_secondary(&mut state, &session, &primary, Some(a));
    assert!(submit_animator_action(
        &session,
        &target,
        &mut state,
        false,
        AnimatorAction::RemoveSelector(a),
        apply
    ));
    session.borrow_mut().observe(&state);
    let primary = session.borrow().target.clone().unwrap();
    assert_eq!(primary.selector, None);
    assert!(submit_animator_action(
        &session,
        &primary,
        &mut state,
        false,
        AnimatorAction::AddSelector,
        apply
    ));
    let fresh = session.borrow().target.clone().unwrap();
    let b = fresh.selector.unwrap();
    assert_ne!(a, b);
    let after = state.editor.project().clone();
    submit_animator_field(&session, &target, &mut state, 0, "88", true, |_, _| {
        panic!("removed selector callback dispatched")
    });
    assert_eq!(state.editor.project(), &after);
    state.bulk_test_action(&Action::Undo);
    state.bulk_test_action(&Action::Undo);
    session.borrow_mut().observe(&state);
    let current = session.borrow().target.clone().unwrap();
    let restored = select_secondary(&mut state, &session, &current, Some(a));
    assert_ne!(restored.key(), target.key());
    submit_animator_field(&session, &target, &mut state, 0, "77", true, |_, _| {
        panic!("undo restored a retired callback")
    });
    assert_eq!(
        range_selector(state.editor.selected_layer().unwrap(), a)
            .unwrap()
            .start,
        0.
    );
    assert!(state.editor.can_redo());
}

#[test]
fn secondary_selector_discrete_edits_preserve_other_fields_and_primary_grouping_units() {
    assert_eq!(
        TextSelectorMode::ALL.map(TextSelectorMode::label),
        ["Add", "Subtract", "Intersect"]
    );
    let mut state = scene();
    state
        .editor
        .execute(Command::SetTextSelector {
            id: 1,
            selector: TextSelector {
                units: TextSelectorUnits::Words,
                shape: TextSelectorShape::Triangle,
            },
        })
        .unwrap();
    let id = add_secondary(&mut state);
    for index in RANGE_FIELDS {
        secondary_animation(&mut state, id, index, 0, AnimatorEdit::Enable);
        set_secondary(&mut state, id, index, 33., 60);
    }
    let original = range_selector(state.editor.selected_layer().unwrap(), id)
        .unwrap()
        .clone();
    for action in [
        AnimatorAction::SelectorUnits {
            selector: id,
            units: TextSelectorUnits::Lines,
        },
        AnimatorAction::SelectorShape {
            selector: id,
            shape: TextSelectorShape::RampDown,
        },
        AnimatorAction::SelectorMode {
            selector: id,
            mode: TextSelectorMode::Subtract,
        },
    ] {
        let command = animator_action_command(state.editor.selected_layer().unwrap(), 20, action)
            .unwrap()
            .unwrap();
        assert!(matches!(command, Command::SetTextRangeSelector { .. }));
        state.editor.execute(command).unwrap();
        assert!(
            animator_action_command(state.editor.selected_layer().unwrap(), 20, action)
                .unwrap()
                .is_none()
        );
    }
    let layer = state.editor.selected_layer().unwrap();
    let result = range_selector(layer, id).unwrap().clone();
    assert_eq!(
        result,
        TextRangeSelector {
            mode: TextSelectorMode::Subtract,
            selector: TextSelector {
                units: TextSelectorUnits::Lines,
                shape: TextSelectorShape::RampDown,
            },
            ..original
        }
    );
    assert_eq!(layer.text_selector().units, TextSelectorUnits::Words);
    assert_eq!(layer.text_selector().shape, TextSelectorShape::Triangle);
    assert!(
        ANIMATOR_STACK_HELP
            .contains("Primary Units always define scale/rotation groups and pivots")
    );
}

#[test]
fn secondary_selectors_keep_shared_transform_animation_and_reject_primary_range_keys() {
    let mut state = scene();
    let id = add_secondary(&mut state);
    let (session, primary) = bound(&state);
    let target = select_secondary(&mut state, &session, &primary, Some(id));
    for index in RANGE_FIELDS {
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            false,
            AnimatorAction::Animation {
                parameter: ANIMATOR_PARAMETERS[index],
                edit: AnimatorEdit::Enable
            },
            |_, _| panic!("secondary range changed a primary track")
        ));
    }
    assert!(submit_animator_action(
        &session,
        &target,
        &mut state,
        false,
        AnimatorAction::Animation {
            parameter: TextParam::AnimatorScaleX,
            edit: AnimatorEdit::Enable
        },
        apply
    ));
    session.borrow_mut().observe(&state);
    let target = session.borrow().target.clone().unwrap();
    assert_eq!(target.selector, Some(id));
    submit_animator_field(&session, &target, &mut state, 7, "125", true, apply);
    let layer = state.editor.selected_layer().unwrap();
    assert_eq!(
        layer.text_value_at(TextParam::AnimatorScaleX, 0),
        Some(125.)
    );
    assert_eq!(
        range_selector(layer, id).unwrap(),
        &TextRangeSelector::new(id)
    );
    assert_eq!(
        layer
            .track(PropertyPath::Text(TextParam::AnimatorScaleX))
            .unwrap()
            .keys()
            .len(),
        1
    );
}

#[test]
fn secondary_stack_limit_bounds_and_noop_moves_do_not_clear_redo() {
    let mut state = scene();
    for _ in 0..MAX_TEXT_RANGE_SELECTORS {
        add_secondary(&mut state);
    }
    state.editor.clear_history();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo".into(),
        })
        .unwrap();
    state.editor.undo();
    let layer = state.editor.selected_layer().unwrap();
    let id = layer.text_range_selectors()[0].id;
    assert!(
        animator_action_command(layer, 0, AnimatorAction::AddSelector)
            .unwrap()
            .is_none()
    );
    assert!(
        animator_action_command(
            layer,
            0,
            AnimatorAction::MoveSelector {
                selector: id,
                index: 0
            }
        )
        .unwrap()
        .is_none()
    );
    assert!(
        animator_action_command(
            layer,
            0,
            AnimatorAction::MoveSelector {
                selector: id,
                index: MAX_TEXT_RANGE_SELECTORS
            }
        )
        .is_err()
    );
    assert!(animator_action_command(layer, 0, AnimatorAction::RemoveSelector(0)).is_err());
    assert!(state.editor.can_redo());
    assert!(!state.editor.can_undo());
}

#[test]
fn secondary_stack_foreign_pending_and_source_text_ime_never_dispatch_or_finish_source_editing() {
    let mut state = scene();
    let a = add_secondary(&mut state);
    let b = add_secondary(&mut state);
    let (session, primary) = bound(&state);
    let target = select_secondary(&mut state, &session, &primary, Some(a));
    let before = state.editor.project().clone();
    let actions = [
        AnimatorAction::SelectSelector(Some(b)),
        AnimatorAction::AddSelector,
        AnimatorAction::RemoveSelector(a),
        AnimatorAction::MoveSelector {
            selector: a,
            index: 1,
        },
        AnimatorAction::SelectorMode {
            selector: a,
            mode: TextSelectorMode::Intersect,
        },
        AnimatorAction::SelectorAnimation {
            selector: a,
            parameter: TextSelectorParam::Amount,
            edit: AnimatorEdit::Enable,
        },
    ];
    for action in actions {
        assert!(
            !session
                .borrow_mut()
                .prepare(&target, &state, Some("foreign-field".into()))
        );
        assert!(!session.borrow_mut().finish_flush(&target, &state));
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            true,
            action,
            |_, _| panic!("foreign field was flushed")
        ));
    }
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
    for action in actions {
        assert!(!session.borrow_mut().prepare(&target, &state, None));
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            false,
            action,
            |_, _| panic!("Source Text session was finished")
        ));
        submit_animator_field(&session, &target, &mut state, 0, "25", true, |_, _| {
            panic!("Source Text session was finished by numeric field")
        });
        assert!(state.text_session.as_ref().unwrap().buffer.marked.is_some());
        assert_eq!(state.editor.project(), &before);
    }
}

#[test]
fn secondary_stack_invalid_flush_blocks_selection_removal_and_mode_change() {
    let mut state = scene();
    let a = add_secondary(&mut state);
    let b = add_secondary(&mut state);
    for action in [
        AnimatorAction::SelectSelector(Some(b)),
        AnimatorAction::RemoveSelector(a),
        AnimatorAction::SelectorMode {
            selector: a,
            mode: TextSelectorMode::Intersect,
        },
        AnimatorAction::SelectorAnimation {
            selector: a,
            parameter: TextSelectorParam::Amount,
            edit: AnimatorEdit::Enable,
        },
    ] {
        let (session, primary) = bound(&state);
        let target = select_secondary(&mut state, &session, &primary, Some(a));
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(6)))
        );
        submit_animator_field(&session, &target, &mut state, 6, "101", true, |_, _| {
            panic!("invalid Amount dispatched")
        });
        assert!(!session.borrow_mut().finish_flush(&target, &state));
        let before = state.editor.project().clone();
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            true,
            action,
            |_, _| panic!("action survived an invalid static draft")
        ));
        assert_eq!(session.borrow().target.as_ref().unwrap().selector, Some(a));
        assert_eq!(state.editor.project(), &before);
    }
}

fn set_secondary(state: &mut EditorState, selector: u64, index: usize, value: f64, frame: Frame) {
    let command = selector_field_command(
        state.editor.selected_layer().unwrap(),
        frame,
        Some(selector),
        index,
        &value.to_string(),
    )
    .unwrap()
    .unwrap();
    state.editor.execute(command).unwrap();
}

fn secondary_animation(
    state: &mut EditorState,
    selector: u64,
    index: usize,
    frame: Frame,
    edit: AnimatorEdit,
) {
    let command = selector_animation_command(
        state.editor.selected_layer().unwrap(),
        selector,
        selector_parameter(index).unwrap(),
        frame,
        edit,
    )
    .unwrap()
    .unwrap();
    state.editor.execute(command).unwrap();
}

#[test]
fn secondary_animation_uses_stable_id_paths_and_precise_current_frame_noops() {
    let mut state = scene();
    let a = add_secondary(&mut state);
    let b = add_secondary(&mut state);
    for index in 0..ANIMATOR_FIELDS {
        assert_eq!(
            animator_property(None, index),
            PropertyPath::Text(ANIMATOR_PARAMETERS[index])
        );
        if let Some(parameter) = selector_parameter(index) {
            assert_eq!(
                animator_property(Some(a), index),
                PropertyPath::TextSelector {
                    selector: a,
                    parameter
                }
            );
            secondary_animation(&mut state, a, index, 0, AnimatorEdit::Enable);
            set_secondary(&mut state, a, index, 37.123456789012345, 60);
        } else {
            assert_eq!(
                animator_property(Some(a), index),
                PropertyPath::Text(ANIMATOR_PARAMETERS[index])
            );
        }
    }
    state.frame = 17;
    state.editor.clear_history();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo".into(),
        })
        .unwrap();
    state.editor.undo();
    let before = state.editor.project().clone();
    for index in RANGE_FIELDS {
        let layer = state.editor.selected_layer().unwrap();
        let parameter = selector_parameter(index).unwrap();
        let sample = range_selector(layer, a).unwrap().value_at(parameter, 17);
        assert_ne!(
            sample,
            range_selector(layer, a).unwrap().value_at(parameter, 0)
        );
        assert_eq!(selector_values(layer, 17, Some(a))[index], sample);
        let (session, primary) = bound(&state);
        let target = select_secondary(&mut state, &session, &primary, Some(a));
        assert_eq!(
            target.display(&state, index).parse::<f64>().unwrap(),
            sample
        );
        assert_eq!(
            submit_animator_field(
                &session,
                &target,
                &mut state,
                index,
                &format!(" {sample:e} "),
                true,
                |_, _| panic!("equal sample inserted a key")
            ),
            sample.to_string()
        );
        let layer = state.editor.selected_layer().unwrap();
        assert!(
            !layer
                .track(animator_property(Some(a), index))
                .unwrap()
                .keys()
                .contains_key(&17)
        );
        assert!(layer.track(animator_property(Some(b), index)).is_none());
        assert!(
            layer
                .track(PropertyPath::Text(ANIMATOR_PARAMETERS[index]))
                .is_none()
        );
    }
    assert_eq!(state.editor.project(), &before);
    assert!(state.editor.can_redo());
    assert!(!state.editor.can_undo());
    assert!(!ANIMATOR_STACK_HELP.contains("no animation keys"));
}

#[test]
fn secondary_animation_explicit_controls_keep_baselines_unrelated_tracks_and_exact_undo() {
    for index in RANGE_FIELDS {
        let mut state = scene();
        let a = add_secondary(&mut state);
        let b = add_secondary(&mut state);
        secondary_animation(&mut state, b, 6, 0, AnimatorEdit::Enable);
        set_secondary(&mut state, b, 6, 20., 60);
        animation(
            &mut state,
            TextParam::AnimatorScaleX,
            0,
            AnimatorEdit::Enable,
        );
        set(&mut state, 7, 150., 60);
        let original = range_selector(state.editor.selected_layer().unwrap(), a)
            .unwrap()
            .clone();
        let parameter = selector_parameter(index).unwrap();
        for edit in [AnimatorEdit::Disable, AnimatorEdit::RemoveKey] {
            assert!(
                selector_animation_command(
                    state.editor.selected_layer().unwrap(),
                    a,
                    parameter,
                    0,
                    edit
                )
                .unwrap()
                .is_none()
            );
        }
        secondary_animation(&mut state, a, index, 0, AnimatorEdit::Enable);
        set_secondary(&mut state, a, index, 37.125, 60);
        for edit in [AnimatorEdit::Enable, AnimatorEdit::AddKey] {
            assert!(
                selector_animation_command(
                    state.editor.selected_layer().unwrap(),
                    a,
                    parameter,
                    0,
                    edit
                )
                .unwrap()
                .is_none()
            );
        }
        let layer = state.editor.selected_layer().unwrap();
        let source = range_selector(layer, a).unwrap();
        assert_eq!(
            (source.start, source.end, source.offset, source.amount),
            (
                original.start,
                original.end,
                original.offset,
                original.amount
            )
        );
        let sample = source.value_at(parameter, 30);
        let unrelated_selector = range_selector(layer, b).unwrap().clone();
        let shared_track = layer
            .track(PropertyPath::Text(TextParam::AnimatorScaleX))
            .cloned();
        let before = state.editor.project().clone();
        state.editor.clear_history();
        secondary_animation(&mut state, a, index, 30, AnimatorEdit::Disable);
        let disabled = state.editor.project().clone();
        let layer = state.editor.selected_layer().unwrap();
        assert!(
            layer
                .track(animator_property(Some(a), index))
                .unwrap()
                .keys()
                .is_empty()
        );
        for frame in [0, 30, 60] {
            assert_eq!(
                range_selector(layer, a).unwrap().value_at(parameter, frame),
                sample
            );
        }
        assert_eq!(range_selector(layer, b), Some(&unrelated_selector));
        assert_eq!(
            layer.track(PropertyPath::Text(TextParam::AnimatorScaleX)),
            shared_track.as_ref()
        );
        assert!(
            layer
                .track(PropertyPath::Text(ANIMATOR_PARAMETERS[index]))
                .is_none()
        );
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
        state.editor.redo();
        assert_eq!(state.editor.project(), &disabled);
    }
}

#[test]
fn secondary_animation_pending_values_keep_explicit_key_intent_and_separate_history() {
    for index in RANGE_FIELDS {
        for edit in [AnimatorEdit::AddKey, AnimatorEdit::Disable] {
            let mut state = scene();
            let id = add_secondary(&mut state);
            secondary_animation(&mut state, id, index, 0, AnimatorEdit::Enable);
            set_secondary(&mut state, id, index, 60., 60);
            state.frame = 30;
            state.editor.clear_history();
            let before = state.editor.project().clone();
            let (session, primary) = bound(&state);
            let target = select_secondary(&mut state, &session, &primary, Some(id));
            assert!(
                session
                    .borrow_mut()
                    .prepare(&target, &state, Some(target.field_key(index)))
            );
            assert_eq!(
                submit_animator_field(
                    &session,
                    &target,
                    &mut state,
                    index,
                    "12.123456789012345",
                    true,
                    apply
                ),
                12.123456789012345_f64.to_string()
            );
            let after_field = state.editor.project().clone();
            assert!(session.borrow_mut().finish_flush(&target, &state));
            let action = AnimatorAction::SelectorAnimation {
                selector: id,
                parameter: selector_parameter(index).unwrap(),
                edit,
            };
            assert!(submit_animator_action(
                &session, &target, &mut state, true, action, apply
            ));
            let after_action = state.editor.project().clone();
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                true,
                action,
                |_, _| panic!("duplicate click reused receipt")
            ));
            let track = state
                .editor
                .selected_layer()
                .unwrap()
                .track(animator_property(Some(id), index))
                .unwrap();
            if edit == AnimatorEdit::AddKey {
                assert_eq!(track.keys().len(), 3);
                assert!(track.keys().contains_key(&30));
                assert_eq!(after_action, after_field);
            } else {
                assert!(track.keys().is_empty());
                state.editor.undo();
                assert_eq!(state.editor.project(), &after_field);
            }
            state.editor.undo();
            assert_eq!(state.editor.project(), &before);
            assert!(!state.editor.can_undo());
        }
    }
}

#[test]
fn secondary_animation_last_key_removal_keeps_value_and_restores_exact_source() {
    for index in RANGE_FIELDS {
        let mut state = scene();
        let id = add_secondary(&mut state);
        set_secondary(&mut state, id, index, 18.123456789012345, 17);
        secondary_animation(&mut state, id, index, 17, AnimatorEdit::AddKey);
        let keyed = state.editor.project().clone();
        state.frame = 17;
        state.editor.clear_history();
        let (session, primary) = bound(&state);
        let target = select_secondary(&mut state, &session, &primary, Some(id));
        let action = AnimatorAction::SelectorAnimation {
            selector: id,
            parameter: selector_parameter(index).unwrap(),
            edit: AnimatorEdit::RemoveKey,
        };
        assert!(submit_animator_action(
            &session, &target, &mut state, false, action, apply
        ));
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            false,
            action,
            |_, _| panic!("retired callback toggled the last key")
        ));
        let layer = state.editor.selected_layer().unwrap();
        assert!(
            layer
                .track(animator_property(Some(id), index))
                .unwrap()
                .keys()
                .is_empty()
        );
        for frame in [0, 17, 60] {
            assert_eq!(
                selector_values(layer, frame, Some(id))[index],
                18.123456789012345
            );
        }
        state.editor.undo();
        assert_eq!(state.editor.project(), &keyed);
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn secondary_animation_reorder_remove_and_undo_preserve_tracks_and_retire_callbacks() {
    let mut state = scene();
    let a = add_secondary(&mut state);
    let b = add_secondary(&mut state);
    for index in RANGE_FIELDS {
        secondary_animation(&mut state, a, index, 0, AnimatorEdit::Enable);
        set_secondary(&mut state, a, index, 12.125, 60);
    }
    state.frame = 30;
    state.editor.clear_history();
    let before = state.editor.project().clone();
    let source = range_selector(state.editor.selected_layer().unwrap(), a)
        .unwrap()
        .clone();
    let (session, primary) = bound(&state);
    let old = select_secondary(&mut state, &session, &primary, Some(a));
    assert!(submit_animator_action(
        &session,
        &old,
        &mut state,
        false,
        AnimatorAction::MoveSelector {
            selector: a,
            index: 1
        },
        apply
    ));
    session.borrow_mut().observe(&state);
    let moved = session.borrow().target.clone().unwrap();
    assert_eq!(moved.selector, Some(a));
    assert_eq!(
        state
            .editor
            .selected_layer()
            .unwrap()
            .text_range_selectors()
            .iter()
            .map(|range| range.id)
            .collect::<Vec<_>>(),
        vec![b, a]
    );
    assert_eq!(
        range_selector(state.editor.selected_layer().unwrap(), a),
        Some(&source)
    );
    assert!(submit_animator_action(
        &session,
        &moved,
        &mut state,
        false,
        AnimatorAction::RemoveSelector(a),
        apply
    ));
    session.borrow_mut().observe(&state);
    assert_eq!(session.borrow().target.as_ref().unwrap().selector, None);
    state.bulk_test_action(&Action::Undo);
    session.borrow_mut().observe(&state);
    let primary = session.borrow().target.clone().unwrap();
    let restored = select_secondary(&mut state, &session, &primary, Some(a));
    for old in [&old, &moved] {
        for index in RANGE_FIELDS {
            submit_animator_field(&session, old, &mut state, index, "88", true, |_, _| {
                panic!("restored ID revived numeric callback")
            });
            assert!(!submit_animator_action(
                &session,
                old,
                &mut state,
                false,
                AnimatorAction::SelectorAnimation {
                    selector: a,
                    parameter: selector_parameter(index).unwrap(),
                    edit: AnimatorEdit::Disable
                },
                |_, _| panic!("restored ID revived key callback")
            ));
        }
    }
    assert!(restored.current(&state));
    assert_eq!(
        range_selector(state.editor.selected_layer().unwrap(), a),
        Some(&source)
    );
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
}

#[test]
fn secondary_animation_actions_reject_source_frame_history_selection_and_transport_changes() {
    for interruption in 0..7 {
        let mut state = scene();
        let id = add_secondary(&mut state);
        secondary_animation(&mut state, id, 6, 0, AnimatorEdit::Enable);
        set_secondary(&mut state, id, 6, 12., 60);
        let (session, primary) = bound(&state);
        let target = select_secondary(&mut state, &session, &primary, Some(id));
        assert!(session.borrow_mut().prepare(&target, &state, None));
        assert!(session.borrow_mut().finish_flush(&target, &state));
        match interruption {
            0 => {
                state.bulk_test_action(&Action::Seek(1));
                state.bulk_test_action(&Action::Seek(0));
            }
            1 => {
                state.bulk_test_action(&Action::Play);
                state.bulk_test_action(&Action::Play);
            }
            2 => {
                state.bulk_test_action(&Action::Edit(Command::RenameLayer {
                    id: 1,
                    name: "Changed".into(),
                }));
                state.bulk_test_action(&Action::Undo);
            }
            3 => {
                state.document_revision += 1;
            }
            4 => {
                state.selected_layers.insert(99);
            }
            5 => {
                state
                    .editor
                    .execute(Command::EditSourceText {
                        id: 1,
                        frame: 0,
                        text: "Replacement".into(),
                    })
                    .unwrap();
            }
            6 => {
                state.editor.execute(Command::ToggleLocked(1)).unwrap();
            }
            _ => unreachable!(),
        }
        let interrupted = state.editor.project().clone();
        for pointer in [false, true] {
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                pointer,
                AnimatorAction::SelectorAnimation {
                    selector: id,
                    parameter: TextSelectorParam::Amount,
                    edit: AnimatorEdit::Disable
                },
                |_, _| panic!("stale selector animation dispatched")
            ));
        }
        submit_animator_field(&session, &target, &mut state, 6, "44", true, |_, _| {
            panic!("stale selector value dispatched")
        });
        assert_eq!(state.editor.project(), &interrupted);
    }
}

#[test]
fn timeline_secondary_animation_blocks_pending_fields_and_consumes_only_fresh_receipts() {
    let mut state = scene();
    let id = add_secondary(&mut state);
    secondary_animation(&mut state, id, 6, 0, AnimatorEdit::Enable);
    set_secondary(&mut state, id, 6, 12., 60);
    state.frame = 30;
    let timeline = TimelineAnimator::default();
    timeline.observe(&state);
    let old = timeline.input.borrow().target.clone().unwrap();
    let (session, primary) = bound(&state);
    let target = select_secondary(&mut state, &session, &primary, Some(id));
    assert!(
        !timeline
            .input
            .borrow_mut()
            .prepare_readonly(&old, &state, Some(target.field_key(6)))
    );
    assert!(!timeline.input.borrow_mut().finish_flush(&old, &state));
    let action = AnimatorAction::SelectorAnimation {
        selector: id,
        parameter: TextSelectorParam::Amount,
        edit: AnimatorEdit::AddKey,
    };
    assert!(!submit_animator_action(
        &timeline.input,
        &old,
        &mut state,
        true,
        action,
        |_, _| panic!("Timeline flushed Properties input")
    ));
    submit_animator_field(&session, &target, &mut state, 6, "42", true, apply);
    let after_field = state.editor.project().clone();
    assert!(
        !timeline
            .input
            .borrow_mut()
            .prepare_readonly(&old, &state, None)
    );
    assert!(!submit_animator_action(
        &timeline.input,
        &old,
        &mut state,
        false,
        action,
        |_, _| panic!("old Timeline Add key toggled the new key")
    ));
    timeline.observe(&state);
    let fresh = timeline.input.borrow().target.clone().unwrap();
    assert!(
        timeline
            .input
            .borrow_mut()
            .prepare_readonly(&fresh, &state, None)
    );
    assert!(timeline.input.borrow_mut().finish_flush(&fresh, &state));
    assert!(submit_animator_action(
        &timeline.input,
        &fresh,
        &mut state,
        true,
        action,
        |_, _| panic!("explicit Add key removed current key")
    ));
    assert!(!submit_animator_action(
        &timeline.input,
        &fresh,
        &mut state,
        true,
        action,
        |_, _| panic!("Timeline receipt reused")
    ));
    assert_eq!(state.editor.project(), &after_field);
}

fn add_extra(state: &mut EditorState) -> u64 {
    state
        .editor
        .execute(Command::AddTextAnimator { id: 1 })
        .unwrap();
    state
        .editor
        .selected_layer()
        .unwrap()
        .text_animators()
        .last()
        .unwrap()
        .id
}

fn select_extra(
    state: &mut EditorState,
    session: &Rc<RefCell<AnimatorInput>>,
    target: &AnimatorTarget,
    animator: Option<u64>,
) -> AnimatorTarget {
    assert!(submit_animator_action(
        session,
        target,
        state,
        false,
        AnimatorAction::SelectAnimator(animator),
        |_, _| panic!("animator selection changed source")
    ));
    session.borrow().target.clone().unwrap()
}

fn extra_animation(
    state: &mut EditorState,
    animator: u64,
    index: usize,
    frame: Frame,
    edit: AnimatorEdit,
) {
    let command = extra_animation_command(
        state.editor.selected_layer().unwrap(),
        animator,
        ANIMATOR_PARAMETERS[index],
        frame,
        edit,
    )
    .unwrap()
    .unwrap();
    state.editor.execute(command).unwrap();
}

fn set_extra(state: &mut EditorState, animator: u64, index: usize, value: f64, frame: Frame) {
    let command = scoped_field_command(
        state.editor.selected_layer().unwrap(),
        frame,
        Some(animator),
        None,
        index,
        &value.to_string(),
    )
    .unwrap()
    .unwrap();
    state.editor.execute(command).unwrap();
}

#[test]
fn extra_animator_all_ten_fields_are_precise_and_preserve_full_primary_and_other_animators() {
    for index in 0..ANIMATOR_FIELDS {
        let mut state = scene();
        for (i, parameter) in ANIMATOR_PARAMETERS.into_iter().enumerate() {
            animation(&mut state, parameter, 0, AnimatorEdit::Enable);
            set(&mut state, i, 37., 60);
        }
        let secondary = add_secondary(&mut state);
        for i in RANGE_FIELDS {
            secondary_animation(&mut state, secondary, i, 0, AnimatorEdit::Enable);
            set_secondary(&mut state, secondary, i, 73., 60);
        }
        let a = add_extra(&mut state);
        let b = add_extra(&mut state);
        extra_animation(&mut state, b, index, 0, AnimatorEdit::Enable);
        set_extra(&mut state, b, index, 19., 60);
        state.frame = 30;
        state.editor.clear_history();
        let before = state.editor.project().clone();
        let (session, primary) = bound(&state);
        let target = select_extra(&mut state, &session, &primary, Some(a));
        assert_eq!(target.animator, Some(a));
        assert_eq!(target.selector, None);
        assert_eq!(
            scoped_property(Some(a), None, index),
            PropertyPath::TextAnimator {
                animator: a,
                parameter: ANIMATOR_PARAMETERS[index],
            }
        );
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(index)))
        );
        assert_eq!(
            submit_animator_field(
                &session,
                &target,
                &mut state,
                index,
                " 12.123456789012345 ",
                true,
                apply
            ),
            12.123456789012345_f64.to_string()
        );
        assert!(session.borrow_mut().finish_flush(&target, &state));
        let layer = state.editor.selected_layer().unwrap();
        let original = before.composition().layer(1).unwrap();
        assert_eq!(
            scoped_values(layer, 30, Some(a), None)[index],
            12.123456789012345
        );
        assert_eq!(layer.text_selector(), original.text_selector());
        assert_eq!(
            layer.text_range_selectors(),
            original.text_range_selectors()
        );
        assert_eq!(extra_animator(layer, b), extra_animator(original, b));
        for parameter in ANIMATOR_PARAMETERS {
            assert_eq!(
                layer.track(PropertyPath::Text(parameter)),
                original.track(PropertyPath::Text(parameter))
            );
        }
        let after = state.editor.project().clone();
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
        state.editor.redo();
        assert_eq!(state.editor.project(), &after);
    }
}

#[test]
fn extra_animator_sparse_noops_bounds_and_missing_ids_preserve_redo() {
    let mut state = scene();
    let id = add_extra(&mut state);
    state.editor.clear_history();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo".into(),
        })
        .unwrap();
    state.editor.undo();
    let before = state.editor.project().clone();
    for index in 0..ANIMATOR_FIELDS {
        let default =
            scoped_values(state.editor.selected_layer().unwrap(), 17, Some(id), None)[index];
        let (session, primary) = bound(&state);
        let target = select_extra(&mut state, &session, &primary, Some(id));
        assert!(
            session
                .borrow_mut()
                .prepare(&target, &state, Some(target.field_key(index)))
        );
        assert_eq!(
            submit_animator_field(
                &session,
                &target,
                &mut state,
                index,
                &format!("  {default:e} "),
                true,
                |_, _| panic!("sparse no-op dispatched")
            ),
            default.to_string()
        );
        assert!(session.borrow_mut().finish_flush(&target, &state));
        let target = session.borrow().target.clone().unwrap();
        let (min, max) = ANIMATOR_PARAMETERS[index].bounds();
        for invalid in [
            "NaN".into(),
            "inf".into(),
            "1e400".into(),
            "bad".into(),
            (min - 0.001).to_string(),
            (max + 0.001).to_string(),
        ] {
            assert!(
                session
                    .borrow_mut()
                    .prepare(&target, &state, Some(target.field_key(index)))
            );
            submit_animator_field(
                &session,
                &target,
                &mut state,
                index,
                &invalid,
                true,
                |_, _| panic!("invalid extra value dispatched"),
            );
            assert!(!session.borrow_mut().finish_flush(&target, &state));
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                true,
                AnimatorAction::RemoveAnimator(id),
                |_, _| panic!("invalid flush removed animator")
            ));
        }
        for missing in [0, u64::MAX] {
            let layer = state.editor.selected_layer().unwrap();
            assert!(scoped_field_command(layer, 17, Some(missing), None, index, "25").is_err());
            for edit in [
                AnimatorEdit::Enable,
                AnimatorEdit::Disable,
                AnimatorEdit::AddKey,
                AnimatorEdit::RemoveKey,
            ] {
                assert!(
                    extra_animation_command(layer, missing, ANIMATOR_PARAMETERS[index], 17, edit)
                        .is_err()
                );
            }
        }
    }
    assert_eq!(state.editor.project(), &before);
    assert!(state.editor.can_redo());
    assert!(!state.editor.can_undo());
    assert!(
        extra_animator(state.editor.selected_layer().unwrap(), id)
            .unwrap()
            .parameters
            .is_empty()
    );
}

#[test]
fn extra_animator_interpolation_noops_and_explicit_disable_preserve_other_tracks() {
    for index in 0..ANIMATOR_FIELDS {
        let mut state = scene();
        let a = add_extra(&mut state);
        let b = add_extra(&mut state);
        extra_animation(&mut state, a, index, 0, AnimatorEdit::Enable);
        set_extra(&mut state, a, index, 63.12345678901234, 60);
        extra_animation(&mut state, b, index, 0, AnimatorEdit::Enable);
        set_extra(&mut state, b, index, 37., 60);
        state.frame = 17;
        state.editor.clear_history();
        state
            .editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Redo".into(),
            })
            .unwrap();
        state.editor.undo();
        let before = state.editor.project().clone();
        let value = extra_animator(state.editor.selected_layer().unwrap(), a)
            .unwrap()
            .value_at(ANIMATOR_PARAMETERS[index], 17)
            .unwrap();
        let (session, primary) = bound(&state);
        let target = select_extra(&mut state, &session, &primary, Some(a));
        assert_eq!(target.display(&state, index), value.to_string());
        assert_eq!(
            submit_animator_field(
                &session,
                &target,
                &mut state,
                index,
                &value.to_string(),
                true,
                |_, _| panic!("unchanged interpolation inserted key")
            ),
            value.to_string()
        );
        assert_eq!(state.editor.project(), &before);
        assert!(state.editor.can_redo());
        let layer = state.editor.selected_layer().unwrap();
        assert!(
            extra_animation_command(
                layer,
                a,
                ANIMATOR_PARAMETERS[index],
                17,
                AnimatorEdit::Enable
            )
            .unwrap()
            .is_none()
        );
        assert!(
            extra_animation_command(
                layer,
                a,
                ANIMATOR_PARAMETERS[index],
                17,
                AnimatorEdit::RemoveKey
            )
            .unwrap()
            .is_none()
        );
        let fresh = session.borrow().target.clone().unwrap();
        assert!(submit_animator_action(
            &session,
            &fresh,
            &mut state,
            false,
            AnimatorAction::ExtraAnimation {
                animator: a,
                parameter: ANIMATOR_PARAMETERS[index],
                edit: AnimatorEdit::Disable
            },
            apply
        ));
        let layer = state.editor.selected_layer().unwrap();
        assert_eq!(
            extra_animator(layer, b),
            extra_animator(before.composition().layer(1).unwrap(), b)
        );
        let source = extra_animator(layer, a).unwrap();
        for frame in [0, 17, 60] {
            assert_eq!(
                source.value_at(ANIMATOR_PARAMETERS[index], frame),
                Some(value)
            );
        }
        assert!(
            layer
                .track(scoped_property(Some(a), None, index))
                .unwrap()
                .keys()
                .is_empty()
        );
        let after = state.editor.project().clone();
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
        state.editor.redo();
        assert_eq!(state.editor.project(), &after);
    }
}

#[test]
fn extra_animator_pending_values_keep_explicit_key_intent_and_separate_atomic_history() {
    for index in 0..ANIMATOR_FIELDS {
        for edit in [AnimatorEdit::AddKey, AnimatorEdit::Disable] {
            let mut state = scene();
            let id = add_extra(&mut state);
            extra_animation(&mut state, id, index, 0, AnimatorEdit::Enable);
            set_extra(&mut state, id, index, 60., 60);
            state.frame = 30;
            state.editor.clear_history();
            let before = state.editor.project().clone();
            let (session, primary) = bound(&state);
            let target = select_extra(&mut state, &session, &primary, Some(id));
            assert!(
                session
                    .borrow_mut()
                    .prepare(&target, &state, Some(target.field_key(index)))
            );
            assert_eq!(
                submit_animator_field(
                    &session,
                    &target,
                    &mut state,
                    index,
                    "12.123456789012345",
                    true,
                    apply
                ),
                12.123456789012345_f64.to_string()
            );
            let after_field = state.editor.project().clone();
            assert!(session.borrow_mut().finish_flush(&target, &state));
            let action = AnimatorAction::ExtraAnimation {
                animator: id,
                parameter: ANIMATOR_PARAMETERS[index],
                edit,
            };
            assert!(submit_animator_action(
                &session, &target, &mut state, true, action, apply
            ));
            let after_action = state.editor.project().clone();
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                true,
                action,
                |_, _| panic!("duplicate click reused extra animator receipt")
            ));
            let track = state
                .editor
                .selected_layer()
                .unwrap()
                .track(scoped_property(Some(id), None, index))
                .unwrap();
            if edit == AnimatorEdit::AddKey {
                assert_eq!(track.keys().len(), 3);
                assert!(track.keys().contains_key(&30));
                assert_eq!(after_action, after_field);
            } else {
                assert!(track.keys().is_empty());
                state.editor.undo();
                assert_eq!(state.editor.project(), &after_field);
            }
            state.editor.undo();
            assert_eq!(state.editor.project(), &before);
            assert!(!state.editor.can_undo());
        }
    }
}

#[test]
fn extra_animator_last_key_removal_retains_current_value_and_exact_undo() {
    for index in 0..ANIMATOR_FIELDS {
        let mut state = scene();
        let id = add_extra(&mut state);
        set_extra(&mut state, id, index, 18.123456789012345, 17);
        extra_animation(&mut state, id, index, 17, AnimatorEdit::Enable);
        state.frame = 17;
        state.editor.clear_history();
        let before = state.editor.project().clone();
        let (session, primary) = bound(&state);
        let target = select_extra(&mut state, &session, &primary, Some(id));
        assert!(submit_animator_action(
            &session,
            &target,
            &mut state,
            false,
            AnimatorAction::ExtraAnimation {
                animator: id,
                parameter: ANIMATOR_PARAMETERS[index],
                edit: AnimatorEdit::RemoveKey
            },
            apply
        ));
        let layer = state.editor.selected_layer().unwrap();
        assert!(
            layer
                .track(scoped_property(Some(id), None, index))
                .unwrap()
                .keys()
                .is_empty()
        );
        for frame in [0, 17, 30, 60] {
            assert_eq!(
                scoped_values(layer, frame, Some(id), None)[index],
                18.123456789012345
            );
        }
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn extra_animator_pending_reorder_remove_and_undo_preserve_stable_tracks_and_graph_pins() {
    let mut state = scene();
    let a = add_extra(&mut state);
    let b = add_extra(&mut state);
    for index in 0..ANIMATOR_FIELDS {
        extra_animation(&mut state, a, index, 0, AnimatorEdit::Enable);
        set_extra(&mut state, a, index, 60., 60);
    }
    let channel = crate::view_state::GraphChannel {
        id: 1,
        property: scoped_property(Some(a), None, 9),
    };
    state.graph_pin_channel(channel).unwrap();
    state.graph_set_channel_height(channel, false, Some([-90., 180.]));
    state.frame = 30;
    state.editor.clear_history();
    let before = state.editor.project().clone();
    let (session, primary) = bound(&state);
    let target = select_extra(&mut state, &session, &primary, Some(a));
    assert!(
        session
            .borrow_mut()
            .prepare(&target, &state, Some(target.field_key(9)))
    );
    submit_animator_field(
        &session,
        &target,
        &mut state,
        9,
        "25.123456789012345",
        true,
        apply,
    );
    let after_field = state.editor.project().clone();
    let source = extra_animator(state.editor.selected_layer().unwrap(), a)
        .unwrap()
        .clone();
    assert!(session.borrow_mut().finish_flush(&target, &state));
    assert!(submit_animator_action(
        &session,
        &target,
        &mut state,
        true,
        AnimatorAction::MoveAnimator {
            animator: a,
            index: 1
        },
        apply
    ));
    let after_move = state.editor.project().clone();
    assert_eq!(
        state
            .editor
            .selected_layer()
            .unwrap()
            .text_animators()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![b, a]
    );
    assert_eq!(
        extra_animator(state.editor.selected_layer().unwrap(), a),
        Some(&source)
    );
    assert!(state.graph_channels.is_pinned(channel));
    assert!(state.graph_channels.is_available(channel));
    assert_eq!(
        state.graph_channel_height(channel, false),
        Some([-90., 180.])
    );
    session.borrow_mut().observe(&state);
    let moved = session.borrow().target.clone().unwrap();
    assert_eq!(moved.animator, Some(a));
    assert_ne!(moved.key(), target.key());
    assert!(submit_animator_action(
        &session,
        &moved,
        &mut state,
        false,
        AnimatorAction::RemoveAnimator(a),
        apply
    ));
    session.borrow_mut().observe(&state);
    assert_eq!(session.borrow().target.as_ref().unwrap().animator, None);
    assert!(state.graph_channels.is_pinned(channel));
    assert!(!state.graph_channels.is_available(channel));
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &after_move);
    assert!(state.graph_channels.is_pinned(channel));
    assert!(state.graph_channels.is_available(channel));
    assert_eq!(
        state.graph_channel_height(channel, false),
        Some([-90., 180.])
    );
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &after_field);
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
    for old in [&target, &moved] {
        submit_animator_field(&session, old, &mut state, 9, "77", true, |_, _| {
            panic!("restored animator revived old field")
        });
    }
}

#[test]
fn extra_animator_selection_round_trips_retire_held_fields_without_harming_primary_selectors() {
    let mut state = scene();
    let secondary = add_secondary(&mut state);
    let a = add_extra(&mut state);
    let b = add_extra(&mut state);
    state.editor.clear_history();
    let before = state.editor.project().clone();
    let generation = state.input_context_generation();
    let (session, primary) = bound(&state);
    let range = select_secondary(&mut state, &session, &primary, Some(secondary));
    let old_a = select_extra(&mut state, &session, &range, Some(a));
    assert!(session.borrow_mut().prepare(&old_a, &state, None));
    assert!(session.borrow_mut().finish_flush(&old_a, &state));
    let target_b = select_extra(&mut state, &session, &old_a, Some(b));
    let new_a = select_extra(&mut state, &session, &target_b, Some(a));
    assert_ne!(old_a.key(), new_a.key());
    assert_eq!(state.input_context_generation(), generation);
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
    assert!(
        session
            .borrow_mut()
            .prepare(&new_a, &state, Some(new_a.field_key(5)))
    );
    for old in [&primary, &range, &old_a, &target_b] {
        submit_animator_field(&session, old, &mut state, 5, "99", true, |_, _| {
            panic!("old animator field revived")
        });
        assert!(!session.borrow_mut().prepare(old, &state, None));
        assert!(!session.borrow_mut().finish_flush(old, &state));
        for pointer in [true, false] {
            assert!(!submit_animator_action(
                &session,
                old,
                &mut state,
                pointer,
                AnimatorAction::RemoveAnimator(a),
                |_, _| panic!("held animator action revived")
            ));
        }
        assert_eq!(session.borrow().armed, Some((new_a.key(), Some(5))));
    }
    assert_eq!(
        submit_animator_field(&session, &new_a, &mut state, 5, "-12", true, apply),
        "-12"
    );
    assert!(session.borrow_mut().finish_flush(&new_a, &state));
    let current = session.borrow().target.clone().unwrap();
    let primary = select_extra(&mut state, &session, &current, None);
    let returned = select_secondary(&mut state, &session, &primary, Some(secondary));
    assert_ne!(returned.key(), range.key());
    assert_eq!(returned.animator, None);
    assert_eq!(returned.selector, Some(secondary));
    assert!(AnimatorTarget::capture_scope(&state, Some(a), Some(secondary)).is_none());
    assert!(
        scoped_field_command(
            state.editor.selected_layer().unwrap(),
            0,
            Some(a),
            Some(secondary),
            0,
            "25"
        )
        .is_err()
    );
}

#[test]
fn extra_animator_add_remove_nonreuse_capacity_and_noop_moves_preserve_history() {
    let mut state = scene();
    let (session, primary) = bound(&state);
    assert!(submit_animator_action(
        &session,
        &primary,
        &mut state,
        false,
        AnimatorAction::AddAnimator,
        apply
    ));
    let old = session.borrow().target.clone().unwrap();
    let a = old.animator.unwrap();
    assert!(submit_animator_action(
        &session,
        &old,
        &mut state,
        false,
        AnimatorAction::RemoveAnimator(a),
        apply
    ));
    session.borrow_mut().observe(&state);
    let primary = session.borrow().target.clone().unwrap();
    assert_eq!(primary.animator, None);
    assert!(submit_animator_action(
        &session,
        &primary,
        &mut state,
        false,
        AnimatorAction::AddAnimator,
        apply
    ));
    let fresh = session.borrow().target.clone().unwrap();
    let b = fresh.animator.unwrap();
    assert_ne!(a, b);
    submit_animator_field(&session, &old, &mut state, 0, "77", true, |_, _| {
        panic!("readd inherited removed binding")
    });
    state.bulk_test_action(&Action::Undo);
    state.bulk_test_action(&Action::Undo);
    session.borrow_mut().observe(&state);
    let current = session.borrow().target.clone().unwrap();
    let restored = select_extra(&mut state, &session, &current, Some(a));
    assert_ne!(restored.key(), old.key());
    submit_animator_field(&session, &old, &mut state, 0, "88", true, |_, _| {
        panic!("undo revived removed binding")
    });
    add_extra(&mut state);
    add_extra(&mut state);
    assert_eq!(
        state
            .editor
            .selected_layer()
            .unwrap()
            .text_animators()
            .len(),
        MAX_TEXT_ANIMATORS
    );
    state.editor.clear_history();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo".into(),
        })
        .unwrap();
    state.editor.undo();
    let before = state.editor.project().clone();
    let layer = state.editor.selected_layer().unwrap();
    assert!(
        animator_action_command(layer, 0, AnimatorAction::AddAnimator)
            .unwrap()
            .is_none()
    );
    assert!(
        animator_action_command(
            layer,
            0,
            AnimatorAction::MoveAnimator {
                animator: a,
                index: 0
            }
        )
        .unwrap()
        .is_none()
    );
    for action in [
        AnimatorAction::MoveAnimator {
            animator: a,
            index: MAX_TEXT_ANIMATORS,
        },
        AnimatorAction::RemoveAnimator(0),
        AnimatorAction::RemoveAnimator(u64::MAX),
        AnimatorAction::MoveAnimator {
            animator: 0,
            index: 0,
        },
    ] {
        assert!(animator_action_command(layer, 0, action).is_err());
    }
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
    assert!(MULTIPLE_ANIMATORS_HELP.contains("single range"));
    assert!(MULTIPLE_ANIMATORS_HELP.contains("ordered animated secondary selectors"));
    assert!(MULTIPLE_ANIMATORS_HELP.contains("opacity multiplies"));
}

#[test]
fn extra_animator_units_shape_are_independent_and_cannot_mutate_primary_controls() {
    let mut state = scene();
    let secondary = add_secondary(&mut state);
    let id = add_extra(&mut state);
    for index in 0..ANIMATOR_FIELDS {
        extra_animation(&mut state, id, index, 0, AnimatorEdit::Enable);
        set_extra(&mut state, id, index, 37., 60);
    }
    let original = state.editor.selected_layer().unwrap().clone();
    for action in [
        AnimatorAction::AnimatorUnits {
            animator: id,
            units: TextSelectorUnits::Words,
        },
        AnimatorAction::AnimatorShape {
            animator: id,
            shape: TextSelectorShape::Triangle,
        },
    ] {
        let command = animator_action_command(state.editor.selected_layer().unwrap(), 20, action)
            .unwrap()
            .unwrap();
        state.editor.execute(command).unwrap();
        assert!(
            animator_action_command(state.editor.selected_layer().unwrap(), 20, action)
                .unwrap()
                .is_none()
        );
    }
    let result = state.editor.selected_layer().unwrap();
    assert_eq!(
        extra_animator(result, id).unwrap().selector,
        TextSelector {
            units: TextSelectorUnits::Words,
            shape: TextSelectorShape::Triangle
        }
    );
    assert_eq!(
        extra_animator(result, id).unwrap().parameters,
        extra_animator(&original, id).unwrap().parameters
    );
    assert_eq!(result.text_selector(), original.text_selector());
    assert_eq!(
        result.text_range_selectors(),
        original.text_range_selectors()
    );
    let (session, primary) = bound(&state);
    let target = select_extra(&mut state, &session, &primary, Some(id));
    let before = state.editor.project().clone();
    for action in [
        AnimatorAction::Units(TextSelectorUnits::Lines),
        AnimatorAction::Shape(TextSelectorShape::RampDown),
        AnimatorAction::AddSelector,
        AnimatorAction::SelectSelector(Some(secondary)),
        AnimatorAction::RemoveSelector(secondary),
        AnimatorAction::Animation {
            parameter: TextParam::AnimatorScaleX,
            edit: AnimatorEdit::Enable,
        },
        AnimatorAction::SelectorAnimation {
            selector: secondary,
            parameter: TextSelectorParam::Amount,
            edit: AnimatorEdit::Enable,
        },
    ] {
        assert!(!submit_animator_action(
            &session,
            &target,
            &mut state,
            false,
            action,
            |_, _| panic!("extra controls modified Primary")
        ));
    }
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn extra_animator_pending_invalid_foreign_and_source_text_ime_block_all_discrete_actions() {
    for interruption in ["foreign", "invalid", "ime"] {
        let mut state = scene();
        let a = add_extra(&mut state);
        let b = add_extra(&mut state);
        state.editor.clear_history();
        let (session, primary) = bound(&state);
        let target = select_extra(&mut state, &session, &primary, Some(a));
        match interruption {
            "foreign" => assert!(!session.borrow_mut().prepare(
                &target,
                &state,
                Some("foreign-field".into())
            )),
            "invalid" => {
                assert!(
                    session
                        .borrow_mut()
                        .prepare(&target, &state, Some(target.field_key(7)))
                );
                submit_animator_field(&session, &target, &mut state, 7, "-1", true, |_, _| {
                    panic!("negative scale dispatched")
                });
            }
            "ime" => {
                assert!(session.borrow_mut().prepare(&target, &state, None));
                assert!(session.borrow_mut().finish_flush(&target, &state));
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
                assert!(AnimatorTarget::capture_scope(&state, Some(a), None).is_none());
            }
            _ => unreachable!(),
        }
        let before = state.editor.project().clone();
        for action in [
            AnimatorAction::SelectAnimator(Some(b)),
            AnimatorAction::SelectAnimator(None),
            AnimatorAction::AddAnimator,
            AnimatorAction::RemoveAnimator(a),
            AnimatorAction::MoveAnimator {
                animator: a,
                index: 1,
            },
            AnimatorAction::AnimatorUnits {
                animator: a,
                units: TextSelectorUnits::Words,
            },
            AnimatorAction::AnimatorShape {
                animator: a,
                shape: TextSelectorShape::Triangle,
            },
            AnimatorAction::ExtraAnimation {
                animator: a,
                parameter: TextParam::AnimatorScaleX,
                edit: AnimatorEdit::Enable,
            },
        ] {
            assert!(!session.borrow_mut().finish_flush(&target, &state));
            assert!(!submit_animator_action(
                &session,
                &target,
                &mut state,
                true,
                action,
                |_, _| panic!("blocked discrete action dispatched")
            ));
            if interruption == "ime" {
                assert!(!submit_animator_action(
                    &session,
                    &target,
                    &mut state,
                    false,
                    action,
                    |_, _| panic!("keyboard action finished marked Source Text")
                ));
                assert!(state.text_session.as_ref().unwrap().buffer.marked.is_some());
            }
        }
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn extra_animator_receipts_reject_source_frame_history_transport_selection_lock_and_modal_changes()
{
    for interruption in 0..11 {
        let mut state = scene();
        let id = add_extra(&mut state);
        extra_animation(&mut state, id, 9, 0, AnimatorEdit::Enable);
        set_extra(&mut state, id, 9, 60., 60);
        let (session, primary) = bound(&state);
        let target = select_extra(&mut state, &session, &primary, Some(id));
        assert!(session.borrow_mut().prepare(&target, &state, None));
        assert!(session.borrow_mut().finish_flush(&target, &state));
        match interruption {
            0 => {
                state.bulk_test_action(&Action::Seek(1));
                state.bulk_test_action(&Action::Seek(0));
            }
            1 => {
                state.bulk_test_action(&Action::Play);
                state.bulk_test_action(&Action::Play);
            }
            2 => {
                state.bulk_test_action(&Action::Edit(Command::RenameLayer {
                    id: 1,
                    name: "Changed".into(),
                }));
                state.bulk_test_action(&Action::Undo);
            }
            3 => state.document_revision += 1,
            4 => {
                state.selected_layers.insert(99);
            }
            5 => {
                state
                    .editor
                    .execute(Command::EditSourceText {
                        id: 1,
                        frame: 0,
                        text: "Replacement".into(),
                    })
                    .unwrap();
            }
            6 => {
                state.editor.execute(Command::ToggleLocked(1)).unwrap();
            }
            7 => state.media_open = true,
            8 => state.fonts_open = true,
            9 => state.queue_open = true,
            10 => state.new_composition_requested = true,
            _ => unreachable!(),
        }
        let before = state.editor.project().clone();
        for pointer in [true, false] {
            for action in [
                AnimatorAction::RemoveAnimator(id),
                AnimatorAction::AnimatorShape {
                    animator: id,
                    shape: TextSelectorShape::Triangle,
                },
                AnimatorAction::ExtraAnimation {
                    animator: id,
                    parameter: TextParam::AnimatorRotation,
                    edit: AnimatorEdit::Disable,
                },
            ] {
                assert!(!submit_animator_action(
                    &session,
                    &target,
                    &mut state,
                    pointer,
                    action,
                    |_, _| panic!("stale extra animator action dispatched")
                ));
            }
        }
        submit_animator_field(&session, &target, &mut state, 9, "44", true, |_, _| {
            panic!("stale extra animator field dispatched")
        });
        assert_eq!(state.editor.project(), &before);
    }
}

#[test]
fn timeline_extra_animator_blocks_pending_fields_and_consumes_fresh_receipts_once() {
    let mut state = scene();
    let id = add_extra(&mut state);
    extra_animation(&mut state, id, 9, 0, AnimatorEdit::Enable);
    set_extra(&mut state, id, 9, 12., 60);
    state.frame = 30;
    let timeline = TimelineAnimator::default();
    timeline.observe(&state);
    let old = timeline.input.borrow().target.clone().unwrap();
    let (session, primary) = bound(&state);
    let target = select_extra(&mut state, &session, &primary, Some(id));
    assert!(
        !timeline
            .input
            .borrow_mut()
            .prepare_readonly(&old, &state, Some(target.field_key(9)))
    );
    assert!(!timeline.input.borrow_mut().finish_flush(&old, &state));
    let action = AnimatorAction::ExtraAnimation {
        animator: id,
        parameter: TextParam::AnimatorRotation,
        edit: AnimatorEdit::AddKey,
    };
    assert!(!submit_animator_action(
        &timeline.input,
        &old,
        &mut state,
        true,
        action,
        |_, _| panic!("Timeline flushed Properties input")
    ));
    submit_animator_field(&session, &target, &mut state, 9, "42", true, apply);
    let after_field = state.editor.project().clone();
    assert!(
        !timeline
            .input
            .borrow_mut()
            .prepare_readonly(&old, &state, None)
    );
    assert!(!submit_animator_action(
        &timeline.input,
        &old,
        &mut state,
        false,
        action,
        |_, _| panic!("old Timeline Add key toggled new key")
    ));
    timeline.observe(&state);
    let fresh = timeline.input.borrow().target.clone().unwrap();
    assert!(
        timeline
            .input
            .borrow_mut()
            .prepare_readonly(&fresh, &state, None)
    );
    assert!(timeline.input.borrow_mut().finish_flush(&fresh, &state));
    assert!(submit_animator_action(
        &timeline.input,
        &fresh,
        &mut state,
        true,
        action,
        |_, _| panic!("explicit Add key removed current extra animator key")
    ));
    assert!(!submit_animator_action(
        &timeline.input,
        &fresh,
        &mut state,
        true,
        action,
        |_, _| panic!("Timeline extra animator receipt reused")
    ));
    assert_eq!(state.editor.project(), &after_field);
}
