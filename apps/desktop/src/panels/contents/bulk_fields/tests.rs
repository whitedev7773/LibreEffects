//! Headless production callback/session tests. These exercise the same submit
//! function installed on TextField, with real EditorState edit/history helpers.
//! They do not establish native keyboard, pointer focus, IME, or Save/Open.
use super::*;
use libre_effects_core::{Property, ShapeKind, ShapeParam, TrackEdit};

fn scene() -> EditorState {
    let mut state = EditorState::default();
    state
        .editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(Default::default()),
            width: 240.,
            height: 160.,
            name: "Shared fields".into(),
        })
        .unwrap();
    for kind in [
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::Parametric(ShapeKind::Ellipse),
        ContentsKind::Parametric(ShapeKind::Rectangle),
    ] {
        state
            .editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add { parent: 0, kind },
            })
            .unwrap();
    }
    state.editor.clear_history();
    state.bulk_test_action(&Action::Select(1));
    state
}
fn selection() -> Selection {
    Selection {
        parent: Some(0),
        items: [1, 2].into(),
        anchor: Some(2),
        cursor: Some(1),
    }
}
fn contents(state: &EditorState) -> &ShapeContents {
    let Content::ShapeContents(contents) = state
        .editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .content()
    else {
        panic!()
    };
    contents
}
fn bind(state: &EditorState, selected: &Selection) -> Rc<RefCell<Session>> {
    let session = Rc::new(RefCell::new(Session::default()));
    session
        .borrow_mut()
        .observe(state, selected, &BTreeSet::new());
    session
}
fn submit_value(
    session: &Rc<RefCell<Session>>,
    target: &FieldTarget,
    state: &mut EditorState,
    text: &str,
) -> String {
    submit(session, target, state, text, |state, command| {
        state.bulk_test_action(&Action::Edit(command));
        state.status == "Edited"
    })
}
fn track(state: &mut EditorState, item: u64, parameter: ContentsParam, frame: Frame, value: f64) {
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value { frame, value },
        },
    }));
}
fn bytes(state: &EditorState) -> String {
    state.editor.project().to_json().unwrap()
}

#[test]
fn display_and_selection_are_exact_read_only_operations() {
    let state = scene();
    let before = bytes(&state);
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    assert!(session.borrow().current(&target, &state));
    assert_eq!(target.binding.selection, selected);
    assert_eq!(target.binding.frame, 0);
    let shown = display(contents(&state), &selected, ContentsParam::Width, 0).unwrap();
    assert_eq!(
        shown.parse::<f64>().unwrap(),
        contents(&state)
            .node(1)
            .unwrap()
            .value_at(ContentsParam::Width, 0)
    );
    assert_eq!(bytes(&state), before);
    assert!(!state.editor.can_undo());
    assert!(!state.editor.can_redo());
    assert!(state.contents_selection.is_none());
}

#[test]
fn mixed_uses_exact_samples_and_uniform_text_round_trips_full_precision() {
    let mut state = scene();
    let selected = selection();
    let parameter = ContentsParam::Transform(Property::PositionX);
    let value = 123.45678901234567;
    track(&mut state, 1, parameter, 0, value);
    track(&mut state, 2, parameter, 0, value + 0.0000000000001);
    assert_eq!(
        display(contents(&state), &selected, parameter, 0).as_deref(),
        Some("Mixed")
    );
    track(&mut state, 2, parameter, 0, value);
    let shown = display(contents(&state), &selected, parameter, 0).unwrap();
    assert_eq!(shown.parse::<f64>().unwrap().to_bits(), value.to_bits());
    // Geometry insertion is front-to-back: selected source order is [2, 1].
    track(&mut state, 1, parameter, 0, 0.0);
    track(&mut state, 2, parameter, 0, -0.0);
    assert_eq!(
        display(contents(&state), &selected, parameter, 0).as_deref(),
        Some("-0")
    );
    track(&mut state, 2, parameter, 0, 1e-200);
    assert_eq!(
        display(contents(&state), &selected, parameter, 0).as_deref(),
        Some("Mixed")
    );
}

#[test]
fn uniform_signed_zero_representation_follows_source_order() {
    let mut state = scene();
    let parameter = ContentsParam::Transform(Property::PositionX);
    track(&mut state, 1, parameter, 0, 0.0);
    // Ordinary numeric equality deliberately makes +0 → -0 a no-op. Establish
    // the signed source value from a genuinely different value instead.
    track(&mut state, 2, parameter, 0, 1.0);
    track(&mut state, 2, parameter, 0, -0.0);
    assert_eq!(
        contents(&state)
            .node(2)
            .unwrap()
            .value_at(parameter, 0)
            .to_bits(),
        (-0.0f64).to_bits()
    );
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::Move {
            item: 2,
            parent: 0,
            index: 0,
        },
    }));
    assert_eq!(
        display(contents(&state), &selection(), parameter, 0).as_deref(),
        Some("-0")
    );
}

#[test]
fn invalid_absolute_input_restores_source_and_never_dispatches_or_damages_redo() {
    for text in [
        "", " ", "Mixed", "NaN", "inf", "-inf", "1e999", "--1", "0", "32768.1", "1,000",
    ] {
        let mut state = scene();
        track(&mut state, 2, ContentsParam::Width, 0, 314.);
        state.bulk_test_action(&Action::Undo);
        let before = bytes(&state);
        let session = bind(&state, &selection());
        let target = session.borrow().target(ContentsParam::Width).unwrap();
        let source_display = target
            .binding
            .display(state.editor.project(), target.parameter)
            .unwrap();
        let output = submit(&session, &target, &mut state, text, |_, _| {
            panic!("Invalid input must not dispatch")
        });
        assert_eq!(output, source_display, "{text}");
        assert_eq!(bytes(&state), before, "{text}");
        assert!(state.editor.can_redo(), "{text}");
        assert!(!state.editor.can_undo(), "{text}");
        assert!(!state.status.is_empty());
        assert!(!session.borrow().current(&target, &state));
        // The same rejected callback cannot later turn blur into an edit.
        submit(&session, &target, &mut state, "300", |_, _| {
            panic!("Consumed callback")
        });
        assert_eq!(bytes(&state), before);
    }
}

#[test]
fn mixed_invalid_input_never_becomes_zero_and_can_rebind_for_valid_entry() {
    let mut state = scene();
    track(&mut state, 2, ContentsParam::Width, 0, 300.);
    state.editor.clear_history();
    let selected = selection();
    let session = bind(&state, &selected);
    let old = session.borrow().target(ContentsParam::Width).unwrap();
    assert_eq!(submit_value(&session, &old, &mut state, ""), "Mixed");
    assert!(!state.editor.can_undo());
    session
        .borrow_mut()
        .observe(&state, &selected, &BTreeSet::new());
    let fresh = session.borrow().target(ContentsParam::Width).unwrap();
    assert_ne!(fresh.binding_key(), old.binding_key());
    assert_eq!(submit_value(&session, &fresh, &mut state, " 2.5e2 "), "250");
    for item in [1, 2] {
        assert_eq!(
            contents(&state)
                .node(item)
                .unwrap()
                .value_at(ContentsParam::Width, 0),
            250.
        );
    }
}

#[test]
fn accepted_entry_is_one_history_step_and_a_duplicate_callback_is_inert() {
    let mut state = scene();
    let before = bytes(&state);
    let untouched = contents(&state).node(3).unwrap().clone();
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    assert_eq!(
        submit_value(&session, &target, &mut state, "321.125"),
        "321.125"
    );
    let edited = bytes(&state);
    assert_ne!(edited, before);
    assert_eq!(contents(&state).node(3), Some(&untouched));
    assert!(state.contents_selection.is_none());
    assert_eq!(
        submit_value(&session, &target, &mut state, "999"),
        "321.125"
    );
    assert_eq!(bytes(&state), edited);
    state.bulk_test_action(&Action::Undo);
    assert_eq!(bytes(&state), before);
    assert!(!state.editor.can_undo());
    state.bulk_test_action(&Action::Redo);
    assert_eq!(bytes(&state), edited);
    assert!(!state.editor.can_redo());
}

#[test]
fn two_distinct_submissions_rebind_and_retain_exact_selection_and_disclosure() {
    let mut state = scene();
    let selected = selection();
    let collapsed = [77].into(); // Unrelated disclosure state is retained, not normalized away by fields.
    let session = Rc::new(RefCell::new(Session::default()));
    session.borrow_mut().observe(&state, &selected, &collapsed);
    let before = bytes(&state);
    for (parameter, value) in [
        (ContentsParam::Width, "201.25"),
        (ContentsParam::Height, "302.5"),
    ] {
        let target = session.borrow().target(parameter).unwrap();
        assert_eq!(target.binding.selection, selected);
        assert_eq!(target.binding.collapsed, collapsed);
        assert_eq!(submit_value(&session, &target, &mut state, value), value);
        session.borrow_mut().observe(&state, &selected, &collapsed);
    }
    let edited = bytes(&state);
    state.bulk_test_action(&Action::Undo);
    assert_ne!(bytes(&state), before);
    state.bulk_test_action(&Action::Undo);
    assert_eq!(bytes(&state), before);
    state.bulk_test_action(&Action::Redo);
    state.bulk_test_action(&Action::Redo);
    assert_eq!(bytes(&state), edited);
}

#[test]
fn equal_submission_preserves_exact_source_and_redo_after_rebinding() {
    let mut state = scene();
    let parameter = ContentsParam::Width;
    track(&mut state, 1, parameter, 0, 211.);
    state.bulk_test_action(&Action::Undo);
    let before = bytes(&state);
    let session = bind(&state, &selection());
    let target = session.borrow().target(parameter).unwrap();
    let value = target
        .binding
        .display(state.editor.project(), parameter)
        .unwrap();
    assert_ne!(value, "Mixed");
    assert_eq!(
        submit_value(&session, &target, &mut state, &format!("{value}.0")),
        value
    );
    assert_eq!(bytes(&state), before);
    assert!(state.editor.can_redo());
    assert!(!state.editor.can_undo());
}

#[test]
fn selection_aba_cannot_revive_an_old_field_even_with_identical_source() {
    let mut state = scene();
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    let before = bytes(&state);
    let mut other = selected.clone();
    other.items = [2, 3].into();
    // Production publish_tree_selection invalidates on every publication.
    session.borrow_mut().invalidate();
    session
        .borrow_mut()
        .observe(&state, &other, &BTreeSet::new());
    session.borrow_mut().invalidate();
    session
        .borrow_mut()
        .observe(&state, &selected, &BTreeSet::new());
    let fresh = session.borrow().target(ContentsParam::Width).unwrap();
    submit_value(&session, &target, &mut state, "222");
    assert_eq!(bytes(&state), before);
    assert!(session.borrow().current(&fresh, &state));
    assert_eq!(submit_value(&session, &fresh, &mut state, "222"), "222");
}

#[test]
fn source_edit_undo_and_lock_unlock_roundtrips_reject_old_callbacks() {
    for action in [
        Command::RenameLayer {
            id: 1,
            name: "Transient source".into(),
        },
        Command::ToggleLocked(1),
    ] {
        let mut state = scene();
        let session = bind(&state, &selection());
        let target = session.borrow().target(ContentsParam::Width).unwrap();
        let before = bytes(&state);
        state.bulk_test_action(&Action::Edit(action));
        state.bulk_test_action(&Action::Undo);
        assert_eq!(bytes(&state), before);
        submit_value(&session, &target, &mut state, "444");
        assert_eq!(bytes(&state), before);
        assert!(state.editor.can_redo());
        assert_eq!(state.status, STALE);
    }
}

#[test]
fn seek_playback_and_tool_equal_returns_are_guarded_without_a_render() {
    for actions in [
        vec![Action::Seek(9), Action::Seek(0)],
        vec![Action::Play, Action::Play],
        vec![
            Action::SetTool(crate::editor::Tool::Hand),
            Action::SetTool(crate::editor::Tool::Select),
        ],
    ] {
        let mut state = scene();
        let session = bind(&state, &selection());
        let target = session.borrow().target(ContentsParam::Width).unwrap();
        let before = bytes(&state);
        for action in actions {
            state.bulk_test_action(&action);
        }
        submit_value(&session, &target, &mut state, "456");
        assert_eq!(bytes(&state), before);
        assert_eq!(state.status, STALE);
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn layer_selection_equal_return_has_an_independent_input_epoch() {
    let mut state = scene();
    state.editor.execute(Command::AddRectangle).unwrap();
    state.bulk_test_action(&Action::Select(1));
    state.editor.clear_history();
    let session = bind(&state, &selection());
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    let generation = state.transport_generation();
    let before = bytes(&state);
    state.bulk_test_action(&Action::Select(2));
    state.bulk_test_action(&Action::Select(1));
    assert_eq!(generation, state.transport_generation());
    submit_value(&session, &target, &mut state, "456");
    assert_eq!(bytes(&state), before);
    assert_eq!(state.status, STALE);
}

#[test]
fn replacement_reusing_ids_deleted_member_and_parent_change_are_not_retargeted() {
    for mutate in [
        (|state: &mut EditorState| {
            let same = state.editor.project().clone();
            state.editor.replace_project(same).unwrap();
            state.document_revision += 1;
        }) as fn(&mut EditorState),
        |state| {
            state.bulk_test_action(&Action::Edit(Command::Contents {
                id: 1,
                edit: ContentsEdit::Remove(2),
            }))
        },
        |state| {
            state.bulk_test_action(&Action::Edit(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 0,
                    kind: ContentsKind::Group(vec![]),
                },
            }));
            state.bulk_test_action(&Action::Edit(Command::Contents {
                id: 1,
                edit: ContentsEdit::Move {
                    item: 2,
                    parent: 4,
                    index: 0,
                },
            }));
        },
    ] {
        let mut state = scene();
        let session = bind(&state, &selection());
        let target = session.borrow().target(ContentsParam::Width).unwrap();
        mutate(&mut state);
        let before = bytes(&state);
        submit_value(&session, &target, &mut state, "456");
        assert_eq!(bytes(&state), before);
        assert_eq!(state.status, STALE);
    }
}

#[test]
fn modal_and_context_gates_reject_before_dispatch_and_rebind_only_when_eligible() {
    let mutations: &[fn(&mut EditorState)] = &[
        |s| s.playing = true,
        |s| s.media_open = true,
        |s| s.fonts_open = true,
        |s| s.queue_open = true,
        |s| s.new_composition_requested = true,
        |s| s.close_after_save = true,
        |s| s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 1)),
        |s| {
            s.selected_layers.insert(99);
        },
        |s| {
            s.gradient_controls = Some(crate::color_edit::GradientTarget::Contents(
                s.editor.project().active_composition_id(),
                1,
                1,
            ))
        },
        |s| s.editor.clear_selection(),
        |s| {
            s.editor.execute(Command::ToggleLocked(1)).unwrap();
        },
        |s| s.frame = 3,
        |s| s.tool = crate::editor::Tool::Pen,
    ];
    for mutate in mutations {
        let mut state = scene();
        let session = bind(&state, &selection());
        let target = session.borrow().target(ContentsParam::Width).unwrap();
        mutate(&mut state);
        let before = bytes(&state);
        submit(&session, &target, &mut state, "456", |_, _| {
            panic!("Stale context dispatched")
        });
        assert_eq!(bytes(&state), before);
        assert_eq!(state.status, STALE);
    }
}

#[test]
fn observed_modal_roundtrip_retires_serial_before_source_equal_return() {
    let mut state = scene();
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    state.queue_open = true;
    session
        .borrow_mut()
        .observe(&state, &selected, &BTreeSet::new());
    assert!(session.borrow().binding.is_none());
    state.queue_open = false;
    session
        .borrow_mut()
        .observe(&state, &selected, &BTreeSet::new());
    assert!(!session.borrow().current(&target, &state));
    assert!(session.borrow().target(ContentsParam::Width).is_some());
}

#[test]
fn unsupported_channel_is_never_a_field_target() {
    let state = scene();
    let session = bind(&state, &selection());
    assert!(
        session
            .borrow()
            .target(ContentsParam::Shape(ShapeParam::FillOpacity))
            .is_none()
    );
    assert!(
        display(
            contents(&state),
            &selection(),
            ContentsParam::Shape(ShapeParam::FillOpacity),
            0
        )
        .is_none()
    );
}

#[test]
fn bounds_are_inclusive_and_finite_exponents_are_absolute() {
    let state = scene();
    let session = bind(&state, &selection());
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    for value in ["0.001", "32768", " 2e2 "] {
        assert!(target.command(value).is_ok(), "{value}");
    }
    for value in ["0.000999999999", "32768.00000001", "+=1", "1%", "0x10"] {
        assert!(target.command(value).is_err(), "{value}");
    }
}

#[test]
fn tree_press_validates_before_pending_commit_then_replans_same_selection_once() {
    let mut state = scene();
    let selected = selection();
    let collapsed = BTreeSet::new();
    let context = super::super::tree::PressContext::capture(
        &state,
        &selected,
        &collapsed,
        0,
        3,
        Arc::new(state.editor.project().clone()),
    );
    let input = crate::color_edit::InputTarget::new(&state).unwrap();
    let session = bind(&state, &selected);
    let field = session.borrow().target(ContentsParam::Width).unwrap();
    let before = bytes(&state);
    assert!(context.same_context(&state, &selected, &collapsed, false));
    assert!(input.current(&state));
    assert_eq!(
        submit_value(&session, &field, &mut state, "543.125"),
        "543.125"
    );
    assert!(!context.same_context(&state, &selected, &collapsed, false));
    assert!(context.same_context(&state, &selected, &collapsed, true));
    assert!(input.same_context(&state));
    let receipt = crate::color_edit::InputTarget::new(&state).unwrap();
    assert!(receipt.current(&state));
    let mut next = selected.clone();
    next.click(0, &[1, 2, 3], 3, false, false);
    session.borrow_mut().invalidate();
    session.borrow_mut().observe(&state, &next, &collapsed);
    assert!(session.borrow().binding.is_none());
    submit_value(&session, &field, &mut state, "777");
    assert_eq!(
        contents(&state).node(3).unwrap(),
        contents_from_json(&before).node(3).unwrap()
    );
    state.bulk_test_action(&Action::Undo);
    assert_eq!(bytes(&state), before);
    assert!(!state.editor.can_undo());
}

fn contents_from_json(source: &str) -> ShapeContents {
    let project = Project::from_json(source).unwrap();
    let Content::ShapeContents(contents) = project.composition().layer(1).unwrap().content() else {
        panic!()
    };
    contents.clone()
}

#[test]
fn stale_tree_press_is_rejected_before_flush_and_invalid_pending_input_changes_no_source() {
    let mut state = scene();
    let selected = selection();
    let collapsed = BTreeSet::new();
    let old = super::super::tree::PressContext::capture(
        &state,
        &selected,
        &collapsed,
        0,
        3,
        Arc::new(state.editor.project().clone()),
    );
    let session = bind(&state, &selected);
    let field = session.borrow().target(ContentsParam::Width).unwrap();
    state.bulk_test_action(&Action::Seek(1));
    assert!(!old.same_context(&state, &selected, &collapsed, false));
    let before = bytes(&state);
    submit(&session, &field, &mut state, "500", |_, _| {
        panic!("Stale before press")
    });
    assert_eq!(bytes(&state), before);
    session.borrow_mut().observe(&state, &selected, &collapsed);
    let current = super::super::tree::PressContext::capture(
        &state,
        &selected,
        &collapsed,
        0,
        3,
        Arc::new(state.editor.project().clone()),
    );
    let field = session.borrow().target(ContentsParam::Width).unwrap();
    assert!(current.same_context(&state, &selected, &collapsed, false));
    submit(&session, &field, &mut state, "not a number", |_, _| {
        panic!("Invalid input")
    });
    assert!(current.same_context(&state, &selected, &collapsed, true));
    assert_eq!(bytes(&state), before);
    assert!(!state.editor.can_undo());
}

#[test]
fn color_and_text_modal_sessions_cannot_receive_bulk_writes() {
    for color in [true, false] {
        let mut state = scene();
        let selected = selection();
        let session = bind(&state, &selected);
        let target = session.borrow().target(ContentsParam::Width).unwrap();
        if color {
            state.colors.session = Some(
                crate::color_edit::Session::new(
                    crate::color_edit::Target::BackgroundDraft(0x222222),
                    state.editor.project(),
                    state.document_revision,
                    state.frame,
                )
                .unwrap(),
            );
        } else {
            state.text_session = Some(
                crate::text_edit::Session::new_box(
                    state.editor.project(),
                    state.document_revision,
                    state.frame,
                    [0., 0., 100., 100.],
                )
                .unwrap(),
            );
        }
        let before = bytes(&state);
        submit(&session, &target, &mut state, "500", |_, _| {
            panic!("Modal bulk write")
        });
        assert_eq!(bytes(&state), before);
        assert_eq!(state.status, STALE);
    }
}

#[test]
fn real_submit_preserves_graph_pins_ranges_and_available_channels_through_history() {
    use crate::view_state::{GraphChannel, GraphRanges};
    use libre_effects_core::KeyRef;
    let mut state = scene();
    let width = ContentsParam::Width;
    let position = ContentsParam::Transform(Property::PositionX);
    for (item, parameter) in [(2, width), (3, position)] {
        state.bulk_test_action(&Action::Edit(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        }));
    }
    track(&mut state, 2, width, 30, 300.);
    state.bulk_test_action(&Action::Seek(15));
    let keys = [(2, width), (3, position)].map(|(item, parameter)| KeyRef {
        id: 1,
        property: PropertyPath::Contents { item, parameter },
        frame: 0,
    });
    for key in keys {
        let channel = GraphChannel::from(key);
        state.graph_channels.pin(channel).unwrap();
        state.graph_channels.ranges.insert(
            channel,
            GraphRanges {
                value: Some([-75., 325.]),
                speed: Some([-300., 300.]),
            },
        );
    }
    state.graph_channels.activate(keys[1].into());
    state.graph_key = Some(keys[1]);
    state.selected_keys = keys.into();
    state.editor.clear_history();
    let pins = state.graph_channels.clone();
    let selected_keys = state.selected_keys.clone();
    let before = bytes(&state);
    let unaffected = contents(&state).node(3).unwrap().clone();
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(width).unwrap();
    assert_eq!(
        target
            .binding
            .display(state.editor.project(), width)
            .as_deref(),
        Some("Mixed")
    );
    assert_eq!(submit_value(&session, &target, &mut state, "250"), "250");
    assert_eq!(state.graph_channels, pins);
    assert_eq!(state.selected_keys, selected_keys);
    assert_eq!(state.graph_key, Some(keys[1]));
    assert_eq!(contents(&state).node(3), Some(&unaffected));
    let static_track = &contents(&state).node(1).unwrap().parameters[&width];
    assert!(static_track.keys().is_empty());
    assert_eq!(static_track.value_at(15), 250.);
    let animated = &contents(&state).node(2).unwrap().parameters[&width];
    assert_eq!(
        animated.keys().keys().copied().collect::<Vec<_>>(),
        vec![0, 15, 30]
    );
    assert_eq!(animated.keys()[&0].value, 100.);
    assert_eq!(animated.keys()[&15].value, 250.);
    assert_eq!(animated.keys()[&30].value, 300.);
    let after = bytes(&state);
    state.bulk_test_action(&Action::Undo);
    assert_eq!(bytes(&state), before);
    assert_eq!(state.graph_channels, pins);
    assert!(state.selected_keys.is_empty());
    assert_eq!(state.graph_key, None);
    assert!(!state.editor.can_undo());
    state.bulk_test_action(&Action::Redo);
    assert_eq!(bytes(&state), after);
    assert_eq!(state.graph_channels, pins);
    assert!(state.selected_keys.is_empty());
    assert_eq!(state.graph_key, None);
}

fn animate(
    session: &Rc<RefCell<Session>>,
    target: &FieldTarget,
    state: &mut EditorState,
    after_flush: bool,
    action: ContentsAnimationAction,
) -> bool {
    submit_animation(
        session,
        target,
        state,
        after_flush,
        action,
        |state, command| {
            state.bulk_test_action(&Action::Edit(command));
            state.status == "Edited"
        },
    )
}

#[test]
fn animation_summary_exposes_explicit_intent_for_mixed_states() {
    use ContentsAnimationAction::{AddKey, Disable, Enable, RemoveKey};
    let mut state = scene();
    let selected = selection();
    let p = ContentsParam::Width;
    let session = bind(&state, &selected);
    let target = session.borrow().target(p).unwrap();
    let summary = animation_state(contents(&state), &target).unwrap();
    assert_eq!(summary.label(), "Static · No key");
    assert_eq!(
        summary.actions(),
        [(Enable, "Enable animation"), (AddKey, "Add key")]
    );
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item: 2,
            parameter: p,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    }));
    let summary = animation_state(contents(&state), &target).unwrap();
    assert_eq!(summary.label(), "Mixed animation · Mixed keys");
    assert_eq!(
        summary.actions(),
        [
            (Enable, "Enable animation"),
            (Disable, "Disable animation"),
            (AddKey, "Add key"),
            (RemoveKey, "Remove key")
        ]
    );
    assert!(ANIMATION_HELP.contains("removes ALL keys"));
    assert!(ANIMATION_HELP.contains("each item's value"));
    let before = bytes(&state);
    assert_eq!(animation_state(contents(&state), &target).unwrap(), summary);
    assert_eq!(bytes(&state), before);
}

#[test]
fn animation_keyboard_action_is_source_bound_once_and_keeps_singleton_target_empty() {
    let mut state = scene();
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    let before = bytes(&state);
    assert!(animate(
        &session,
        &target,
        &mut state,
        false,
        ContentsAnimationAction::Enable
    ));
    for item in [1, 2] {
        assert!(
            contents(&state).node(item).unwrap().parameters[&ContentsParam::Width]
                .keys()
                .contains_key(&0)
        );
    }
    assert!(state.contents_selection.is_none());
    let after = bytes(&state);
    assert_ne!(before, after);
    assert!(!animate(
        &session,
        &target,
        &mut state,
        false,
        ContentsAnimationAction::Enable
    ));
    assert_eq!(bytes(&state), after);
    state.bulk_test_action(&Action::Undo);
    assert_eq!(bytes(&state), before);
    assert!(!state.editor.can_undo());
    state.bulk_test_action(&Action::Redo);
    assert_eq!(bytes(&state), after);
}

#[test]
fn animation_pointer_rebases_only_from_guarded_pending_scalar_receipt_and_survives_repaint() {
    let mut state = scene();
    let selected = selection();
    let session = bind(&state, &selected);
    let input = session.borrow().target(ContentsParam::Width).unwrap();
    let button = session.borrow().target(ContentsParam::Height).unwrap();
    let action_serial = session.borrow().action_serial;
    let before = bytes(&state);
    assert!(session.borrow_mut().prepare_action(&button, &state));
    assert_eq!(submit_value(&session, &input, &mut state, "250"), "250");
    assert!(!session.borrow().current(&button, &state));
    assert_eq!(session.borrow().action_serial, action_serial);
    let rebound = session.borrow().action_target(&button, &state).unwrap();
    assert_ne!(rebound.binding.serial, button.binding.serial);
    let after_field = bytes(&state);
    session
        .borrow_mut()
        .observe(&state, &selected, &BTreeSet::new());
    assert_eq!(session.borrow().action_serial, action_serial);
    assert!(session.borrow().action_target(&button, &state).is_some());
    assert!(animate(
        &session,
        &button,
        &mut state,
        true,
        ContentsAnimationAction::Enable
    ));
    assert_ne!(session.borrow().action_serial, action_serial);
    for item in [1, 2] {
        let node = contents(&state).node(item).unwrap();
        assert_eq!(node.value_at(ContentsParam::Width, 0), 250.);
        assert!(node.parameters[&ContentsParam::Width].keys().is_empty());
        assert!(
            node.parameters[&ContentsParam::Height]
                .keys()
                .contains_key(&0)
        );
    }
    assert!(state.contents_selection.is_none());
    assert!(!animate(
        &session,
        &button,
        &mut state,
        true,
        ContentsAnimationAction::Disable
    ));
    state.bulk_test_action(&Action::Undo);
    assert_eq!(bytes(&state), after_field);
    state.bulk_test_action(&Action::Undo);
    assert_eq!(bytes(&state), before);
    assert!(!state.editor.can_undo());
}

#[test]
fn ordinary_field_submit_does_not_authorize_a_late_animation_callback() {
    let mut state = scene();
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    let action_serial = session.borrow().action_serial;
    submit_value(&session, &target, &mut state, "250");
    assert!(session.borrow().action_target(&target, &state).is_none());
    assert_ne!(session.borrow().action_serial, action_serial);
    let before = bytes(&state);
    assert!(!animate(
        &session,
        &target,
        &mut state,
        true,
        ContentsAnimationAction::Enable
    ));
    assert_eq!(bytes(&state), before);
}

#[test]
fn invalid_pending_scalar_consumes_animation_arm_without_applying_either_command() {
    for input in ["", "Mixed", "NaN", "inf", "-1", "32769"] {
        let mut state = scene();
        let session = bind(&state, &selection());
        let target = session.borrow().target(ContentsParam::Width).unwrap();
        let before = bytes(&state);
        assert!(session.borrow_mut().prepare_action(&target, &state));
        submit_value(&session, &target, &mut state, input);
        assert!(session.borrow().action_target(&target, &state).is_none());
        assert!(!animate(
            &session,
            &target,
            &mut state,
            true,
            ContentsAnimationAction::Enable
        ));
        assert_eq!(bytes(&state), before);
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn animation_flush_receipt_cannot_survive_source_equal_undo_seek_or_selection_aba() {
    for change in 0..5 {
        let mut state = scene();
        let selected = selection();
        let session = bind(&state, &selected);
        let target = session.borrow().target(ContentsParam::Width).unwrap();
        assert!(session.borrow_mut().prepare_action(&target, &state));
        submit_value(&session, &target, &mut state, "250");
        assert!(session.borrow().action_target(&target, &state).is_some());
        let before = bytes(&state);
        match change {
            0 => {
                track(&mut state, 3, ContentsParam::Width, 0, 123.);
                state.bulk_test_action(&Action::Undo);
            }
            1 => {
                state.bulk_test_action(&Action::Seek(7));
                state.bulk_test_action(&Action::Seek(0));
            }
            2 => {
                session.borrow_mut().invalidate();
                session.borrow_mut().observe(
                    &state,
                    &Selection {
                        items: [2, 3].into(),
                        ..selected.clone()
                    },
                    &BTreeSet::new(),
                );
                session.borrow_mut().invalidate();
                session
                    .borrow_mut()
                    .observe(&state, &selected, &BTreeSet::new());
            }
            3 => {
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
            }
            _ => {
                state.bulk_test_action(&Action::Play);
                state.bulk_test_action(&Action::Play);
            }
        }
        assert_eq!(bytes(&state), before);
        assert!(
            session.borrow().action_target(&target, &state).is_none(),
            "case {change}"
        );
        assert!(!animate(
            &session,
            &target,
            &mut state,
            true,
            ContentsAnimationAction::Disable
        ));
        assert_eq!(bytes(&state), before);
    }
}

#[test]
fn animation_stale_prepress_guard_never_arms_even_if_source_returns_equal() {
    let mut state = scene();
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    let before = bytes(&state);
    track(&mut state, 3, ContentsParam::Height, 0, 123.);
    state.bulk_test_action(&Action::Undo);
    assert_eq!(bytes(&state), before);
    assert!(!session.borrow_mut().prepare_action(&target, &state));
    assert!(session.borrow().armed.is_none());
    assert!(session.borrow().flushed.is_none());
    submit_value(&session, &target, &mut state, "250");
    assert!(!animate(
        &session,
        &target,
        &mut state,
        true,
        ContentsAnimationAction::Enable
    ));
    assert_eq!(bytes(&state), before);
}

#[test]
fn animation_receipt_does_not_authorize_keyboard_rebase_or_a_different_modal_owner() {
    let mut state = scene();
    let selected = selection();
    let session = bind(&state, &selected);
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    assert!(session.borrow_mut().prepare_action(&target, &state));
    submit_value(&session, &target, &mut state, "250");
    let before = bytes(&state);
    assert!(!animate(
        &session,
        &target,
        &mut state,
        false,
        ContentsAnimationAction::Enable
    ));
    state.media_open = true;
    assert!(session.borrow().action_target(&target, &state).is_none());
    assert!(!animate(
        &session,
        &target,
        &mut state,
        true,
        ContentsAnimationAction::Enable
    ));
    assert_eq!(bytes(&state), before);
}

#[test]
fn rejected_core_scalar_growth_cancels_pending_animation_and_preserves_redo() {
    let mut state = scene();
    let mut source = serde_json::to_value(state.editor.project()).unwrap();
    source["composition"]["duration"] = 20001.into();
    let layer = &mut source["composition"]["layers"][0];
    layer["out_frame"] = 20001.into();
    let nodes = layer["content"]["ShapeContents"]["items"]
        .as_array_mut()
        .unwrap();
    let node = nodes.iter_mut().find(|node| node["id"] == 2).unwrap();
    let keys = node["parameters"]["Width"]["keys"].as_object_mut().unwrap();
    let key = serde_json::to_value(libre_effects_core::Keyframe {
        value: 100.,
        interpolation: libre_effects_core::Interpolation::Linear,
        temporal: Default::default(),
    })
    .unwrap();
    for frame in 0..10000 {
        keys.insert((frame * 2).to_string(), key.clone());
    }
    state
        .editor
        .replace_project(Project::from_json(&source.to_string()).unwrap())
        .unwrap();
    state.bulk_test_action(&Action::Select(1));
    state.editor.clear_history();
    track(&mut state, 3, ContentsParam::Height, 0, 456.);
    state.bulk_test_action(&Action::Undo);
    state.bulk_test_action(&Action::Seek(1));
    assert!(state.editor.can_redo());
    let session = bind(&state, &selection());
    let target = session.borrow().target(ContentsParam::Width).unwrap();
    let before = bytes(&state);
    assert!(session.borrow_mut().prepare_action(&target, &state));
    submit_value(&session, &target, &mut state, "250");
    assert_ne!(state.status, "Edited");
    assert_eq!(bytes(&state), before);
    assert!(state.editor.can_redo());
    assert!(!state.editor.can_undo());
    assert!(session.borrow().action_target(&target, &state).is_none());
    assert!(!animate(
        &session,
        &target,
        &mut state,
        true,
        ContentsAnimationAction::Disable
    ));
    assert_eq!(bytes(&state), before);
    assert!(state.editor.can_redo());
}
