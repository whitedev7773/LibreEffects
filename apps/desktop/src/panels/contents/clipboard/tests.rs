use super::*;
use libre_effects_core::Property;
fn edit(state: &mut EditorState, edit: ContentsEdit) {
    state.bulk_test_action(&Action::Edit(Command::Contents { id: 1, edit }));
    assert_eq!(state.status, "Edited");
}
fn scene() -> EditorState {
    let mut state = EditorState::default();
    state
        .editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(Default::default()),
            width: 100.,
            height: 100.,
            name: "Clipboard".into(),
        })
        .unwrap();
    for parent in [0, 0, 1, 0] {
        edit(
            &mut state,
            ContentsEdit::Add {
                parent,
                kind: ContentsKind::Group(vec![]),
            },
        );
    }
    state.selected_layers = [1].into();
    state.editor.clear_history();
    state
}
fn contents(state: &EditorState) -> &ShapeContents {
    let Content::ShapeContents(contents) = state.editor.selected_layer().unwrap().content() else {
        panic!()
    };
    contents
}
fn select(state: &mut EditorState, parent: u64, items: &[u64]) -> Selection {
    let mut selection = Selection::default();
    selection.all(parent, items);
    state.contents_selection = selection
        .singleton()
        .map(|id| (state.editor.project().active_composition_id(), 1, id));
    selection
}
fn binding(state: &EditorState, selection: &Selection) -> Binding {
    Binding::capture(state, selection, &BTreeSet::new(), 0).unwrap()
}
fn apply(state: &mut EditorState, command: Command) -> bool {
    state.bulk_test_action(&Action::Edit(command));
    state.status == "Edited"
}
fn session(state: &EditorState, selection: &Selection) -> (Rc<RefCell<Session>>, Binding) {
    let session = Rc::new(RefCell::new(Session::default()));
    session
        .borrow_mut()
        .observe(state, selection, &BTreeSet::new());
    let target = session.borrow().binding.clone().unwrap();
    (session, target)
}
fn field_key(target: &Binding, parameter: Option<ContentsParam>) -> String {
    format!("contents-single-{}-{parameter:?}", target.serial)
}

#[test]
fn destination_resolves_root_group_leaf_and_noncontiguous_source_order() {
    let mut state = scene();
    let cases = [
        (0, vec![], 0, 3),
        (0, vec![1], 1, 1),
        (0, vec![4, 1], 0, 3),
        (1, vec![3], 3, 0),
    ];
    for (parent, items, expected_parent, index) in cases {
        let selection = select(&mut state, parent, &items);
        assert_eq!(
            destination(contents(&state), &selection),
            Some(Destination {
                parent: expected_parent,
                index
            })
        );
    }
    edit(
        &mut state,
        ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::Fill { even_odd: false },
        },
    );
    let selection = select(&mut state, 0, &[5]);
    assert_eq!(
        destination(contents(&state), &selection),
        Some(Destination {
            parent: 0,
            index: 4
        })
    );
    for ids in [vec![3, 4], vec![99]] {
        let selection = select(&mut state, 0, &ids);
        assert!(destination(contents(&state), &selection).is_none());
    }
}

#[test]
fn copy_no_history_retains_redo_and_snapshot_survives_later_source_edits() {
    let mut state = scene();
    edit(
        &mut state,
        ContentsEdit::Rename {
            item: 1,
            name: "Future".into(),
        },
    );
    state.bulk_test_action(&Action::Undo);
    let source = state.editor.project().clone();
    let selection = select(&mut state, 0, &[1, 4]);
    let target = binding(&state, &selection);
    assert!(
        execute(&mut state, &target, Operation::Copy, |_, _| panic!(
            "Copy must not edit"
        ))
        .is_some()
    );
    let snapshot = state.contents_clipboard().unwrap().clone();
    assert_eq!(state.editor.project(), &source);
    state.bulk_test_action(&Action::Redo);
    assert_eq!(contents(&state).node(1).unwrap().name, "Future");
    assert_eq!(state.contents_clipboard(), Some(&snapshot));
    let empty = select(&mut state, 0, &[]);
    let target = binding(&state, &empty);
    let result = execute(&mut state, &target, Operation::Paste, apply).unwrap();
    assert_eq!(result.parent, 0);
    assert_eq!(result.items.len(), 2);
    assert_ne!(
        contents(&state).node(result.items[0]).unwrap().name,
        "Future"
    );
}

#[test]
fn cut_is_one_transaction_and_failed_cut_preserves_previous_snapshot_and_history() {
    let mut state = scene();
    let selection = select(&mut state, 0, &[2]);
    let target = binding(&state, &selection);
    execute(&mut state, &target, Operation::Copy, apply);
    let old = state.contents_clipboard().cloned();
    let selection = select(&mut state, 0, &[1, 4]);
    let target = binding(&state, &selection);
    let before = state.editor.project().clone();
    assert!(
        execute(&mut state, &target, Operation::Cut, |state, _| {
            state.status = "Rejected transaction".into();
            false
        })
        .is_none()
    );
    assert_eq!(state.editor.project(), &before);
    assert_eq!(state.contents_clipboard(), old.as_ref());
    let result = execute(&mut state, &target, Operation::Cut, apply).unwrap();
    assert!(result.items.is_empty());
    assert_eq!(
        contents(&state)
            .items
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        vec![2]
    );
    let snapshot = state.contents_clipboard().cloned();
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &before);
    assert_eq!(state.contents_clipboard(), snapshot.as_ref());
    state.bulk_test_action(&Action::Redo);
    assert_eq!(contents(&state).items.len(), 1);
}

#[test]
fn paste_selects_only_inserted_roots_and_reveals_all_destination_ancestors() {
    let mut state = scene();
    let selection = select(&mut state, 0, &[4, 2]);
    let target = binding(&state, &selection);
    execute(&mut state, &target, Operation::Copy, apply).unwrap();
    let destination_selection = select(&mut state, 1, &[3]);
    let target = binding(&state, &destination_selection);
    let before = state.editor.project().clone();
    let outcome = execute(&mut state, &target, Operation::Paste, apply).unwrap();
    assert_eq!(outcome.parent, 3);
    assert_eq!(outcome.items.len(), 2);
    assert!(outcome.items.iter().all(|id| *id > 4));
    let mut selection = Selection::default();
    selection.all(outcome.parent, &outcome.items);
    let mut collapsed: BTreeSet<_> = [1, 3, 4].into();
    reveal(contents(&state), outcome.parent, &mut collapsed);
    selection.reconcile(contents(&state), &collapsed);
    assert_eq!(selection.items, outcome.items.into_iter().collect());
    assert_eq!(collapsed, [4].into());
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn binding_rejects_locked_playing_modals_invalid_selection_and_equal_return_actions() {
    let mut state = scene();
    let selection = select(&mut state, 0, &[1]);
    let target = binding(&state, &selection);
    for action in [
        Action::Seek(8),
        Action::Seek(0),
        Action::Select(1),
        Action::Edit(Command::ToggleLocked(1)),
        Action::Undo,
    ] {
        state.bulk_test_action(&action);
        assert!(!target.current(&state, &selection, &BTreeSet::new(), 0));
    }
    state.playing = true;
    assert!(Binding::capture(&state, &selection, &BTreeSet::new(), 0).is_none());
    state.playing = false;
    state.media_open = true;
    assert!(Binding::capture(&state, &selection, &BTreeSet::new(), 0).is_none());
    state.media_open = false;
    let mut invalid = selection.clone();
    invalid.items.insert(99);
    assert!(Binding::capture(&state, &invalid, &BTreeSet::new(), 0).is_none());
}

#[test]
fn pending_singleton_receipt_accepts_one_semantic_noop_and_repaint_then_expires() {
    let mut state = scene();
    let selection = select(&mut state, 0, &[1]);
    let (session, target) = session(&state, &selection);
    let parameter = Some(ContentsParam::Transform(Property::PositionX));
    assert!(session.borrow_mut().prepare(
        &target,
        &state,
        &selection,
        &BTreeSet::new(),
        Some(field_key(&target, parameter))
    ));
    let before = state.editor.project().clone();
    assert_eq!(
        submit_singleton(&session, &target, parameter, &mut state, "0.0", apply),
        "0"
    );
    assert_eq!(state.editor.project(), &before);
    let next = session
        .borrow()
        .action_target(&target, &state, &selection, &BTreeSet::new(), true)
        .unwrap();
    assert!(
        session
            .borrow()
            .action_target(&target, &state, &selection, &BTreeSet::new(), false)
            .is_none()
    );
    session
        .borrow_mut()
        .observe(&state, &selection, &BTreeSet::new());
    assert!(
        session
            .borrow()
            .action_target(&next, &state, &selection, &BTreeSet::new(), true)
            .is_some()
    );
    session.borrow_mut().invalidate();
    assert!(
        session
            .borrow()
            .action_target(&next, &state, &selection, &BTreeSet::new(), true)
            .is_none()
    );
}

#[test]
fn pending_singleton_invalid_and_failed_core_drafts_never_grant_receipt() {
    for (text, reject_core) in [("garbage", false), ("NaN", false), ("17", true)] {
        let mut state = scene();
        let selection = select(&mut state, 0, &[1]);
        let (session, target) = session(&state, &selection);
        let parameter = Some(ContentsParam::Transform(Property::PositionX));
        session.borrow_mut().prepare(
            &target,
            &state,
            &selection,
            &BTreeSet::new(),
            Some(field_key(&target, parameter)),
        );
        let before = state.editor.project().clone();
        submit_singleton(
            &session,
            &target,
            parameter,
            &mut state,
            text,
            |state, command| {
                if reject_core {
                    false
                } else {
                    apply(state, command)
                }
            },
        );
        assert_eq!(state.editor.project(), &before);
        assert!(
            session
                .borrow()
                .action_target(&target, &state, &selection, &BTreeSet::new(), true)
                .is_none()
        );
    }
}

#[test]
fn pending_singleton_receipt_rejects_wrong_field_and_intervening_equal_return_actions() {
    for mode in 0..4 {
        let mut state = scene();
        let selection = select(&mut state, 0, &[1]);
        let (session, target) = session(&state, &selection);
        let parameter = Some(ContentsParam::Transform(Property::PositionX));
        let key = if mode == 0 {
            "different-field".into()
        } else {
            field_key(&target, parameter)
        };
        session
            .borrow_mut()
            .prepare(&target, &state, &selection, &BTreeSet::new(), Some(key));
        match mode {
            1 => {
                state.bulk_test_action(&Action::Seek(1));
                state.bulk_test_action(&Action::Seek(0));
            }
            2 => {
                edit(
                    &mut state,
                    ContentsEdit::Rename {
                        item: 1,
                        name: "Temp".into(),
                    },
                );
                state.bulk_test_action(&Action::Undo);
            }
            3 => {
                session.borrow_mut().invalidate();
            }
            _ => {}
        }
        let before = state.editor.project().clone();
        let history = (state.editor.can_undo(), state.editor.can_redo());
        submit_singleton(&session, &target, parameter, &mut state, "12", apply);
        assert_eq!(state.editor.project(), &before);
        assert_eq!((state.editor.can_undo(), state.editor.can_redo()), history);
        assert!(
            session
                .borrow()
                .action_target(&target, &state, &selection, &BTreeSet::new(), true)
                .is_none()
        );
    }
}

#[test]
fn direct_receipt_cannot_hide_extra_generations_or_transport_changes() {
    let mut state = scene();
    let selection = select(&mut state, 0, &[1]);
    let (session, target) = session(&state, &selection);
    session.borrow_mut().prepare(
        &target,
        &state,
        &selection,
        &BTreeSet::new(),
        Some("field".into()),
    );
    state.bulk_test_action(&Action::Seek(3));
    state.bulk_test_action(&Action::Seek(0));
    let generation = state.input_context_generation();
    let transport = state.transport_generation();
    edit(
        &mut state,
        ContentsEdit::Rename {
            item: 1,
            name: "Accepted unrelated callback".into(),
        },
    );
    session
        .borrow_mut()
        .field_submitted(&state, true, "field", generation, transport);
    assert!(
        session
            .borrow()
            .action_target(&target, &state, &selection, &BTreeSet::new(), true)
            .is_none()
    );
}

#[test]
fn consecutive_singleton_fields_rebind_after_edit_and_copy() {
    let mut state = scene();
    let selection = select(&mut state, 0, &[1]);
    let (session, first) = session(&state, &selection);
    let parameter = Some(ContentsParam::Transform(Property::PositionX));
    assert_eq!(
        submit_singleton(&session, &first, parameter, &mut state, "14", apply),
        "14"
    );
    session
        .borrow_mut()
        .observe(&state, &selection, &BTreeSet::new());
    let second = session.borrow().binding.clone().unwrap();
    assert_ne!(first.serial, second.serial);
    assert_eq!(
        submit_singleton(&session, &second, None, &mut state, "Renamed", apply),
        "Renamed"
    );
    session
        .borrow_mut()
        .observe(&state, &selection, &BTreeSet::new());
    let third = session.borrow().binding.clone().unwrap();
    execute(&mut state, &third, Operation::Copy, apply).unwrap();
    session
        .borrow_mut()
        .observe(&state, &selection, &BTreeSet::new());
    let fourth = session.borrow().binding.clone().unwrap();
    assert_eq!(
        submit_singleton(&session, &fourth, parameter, &mut state, "21", apply),
        "21"
    );
    assert_eq!(contents(&state).node(1).unwrap().name, "Renamed");
}

#[test]
fn shared_field_callback_grants_only_successful_matching_receipt() {
    let parameter = ContentsParam::Transform(Property::PositionX);
    for text in ["17", "0.0", "invalid", "NaN"] {
        let mut state = scene();
        let selection = select(&mut state, 0, &[1, 2]);
        let (session, target) = session(&state, &selection);
        session.borrow_mut().prepare(
            &target,
            &state,
            &selection,
            &BTreeSet::new(),
            Some(format!("contents-shared-0-{parameter:?}")),
        );
        super::super::bulk_fields::submit_clipboard_test(
            &session, &mut state, &selection, parameter, text,
        );
        assert_eq!(
            session
                .borrow()
                .action_target(&target, &state, &selection, &BTreeSet::new(), true)
                .is_some(),
            matches!(text, "17" | "0.0")
        );
    }
}

#[test]
fn unsupported_pending_field_cannot_grant_source_equal_clipboard_action() {
    let mut state = scene();
    let selection = select(&mut state, 0, &[1]);
    let (session, target) = session(&state, &selection);
    assert!(session.borrow_mut().prepare(
        &target,
        &state,
        &selection,
        &BTreeSet::new(),
        Some("inspector-other-field".into())
    ));
    assert!(
        session
            .borrow()
            .action_target(&target, &state, &selection, &BTreeSet::new(), true)
            .is_none()
    );
    assert!(
        session
            .borrow_mut()
            .prepare(&target, &state, &selection, &BTreeSet::new(), None)
    );
    assert!(
        session
            .borrow()
            .action_target(&target, &state, &selection, &BTreeSet::new(), true)
            .is_some()
    );
}

#[test]
fn exact_clipboard_chords_handle_and_modified_held_text_or_ime_do_not_escape_tree() {
    for (key, expected) in [
        ("c", TreeKey::Copy),
        ("x", TreeKey::Cut),
        ("v", TreeKey::Paste),
    ] {
        assert_eq!(
            tree_key(key, true, false, false, false, true, false),
            Some(expected)
        );
        for (shift, alt, other) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (true, true, true),
        ] {
            assert_eq!(
                tree_key(key, true, shift, alt, other, true, false),
                Some(TreeKey::Consume)
            );
        }
        assert_eq!(tree_key(key, true, false, false, false, false, false), None);
        assert_eq!(tree_key(key, true, false, false, false, true, true), None);
    }
}

#[test]
fn wrong_pending_bulk_callback_rejects_before_mutating_source_or_history() {
    let mut state = scene();
    let selection = select(&mut state, 0, &[1, 2]);
    let (session, target) = session(&state, &selection);
    session.borrow_mut().prepare(
        &target,
        &state,
        &selection,
        &BTreeSet::new(),
        Some("unrelated-field".into()),
    );
    let before = state.editor.project().clone();
    super::super::bulk_fields::submit_clipboard_test(
        &session,
        &mut state,
        &selection,
        ContentsParam::Transform(Property::PositionX),
        "19",
    );
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
    assert!(
        session
            .borrow()
            .action_target(&target, &state, &selection, &BTreeSet::new(), true)
            .is_none()
    );
}
#[test]
fn different_composition_or_fps_paste_failure_preserves_clipboard_source_and_redo() {
    for different_composition in [false, true] {
        let mut state = scene();
        let selection = select(&mut state, 0, &[1]);
        let target = binding(&state, &selection);
        execute(&mut state, &target, Operation::Copy, apply).unwrap();
        if different_composition {
            state.editor.execute(Command::NewComposition).unwrap();
            state
                .editor
                .execute(Command::AddContent {
                    content: Content::ShapeContents(Default::default()),
                    width: 100.,
                    height: 100.,
                    name: "Other".into(),
                })
                .unwrap();
            state.selected_layers = state.editor.selected().into_iter().collect();
            state.contents_selection = None;
        } else {
            state.bulk_test_action(&Action::Edit(Command::ConfigureCompositionRate {
                name: "Changed FPS".into(),
                width: 1920,
                height: 1080,
                fps: 24.into(),
                duration: 150,
                display_start: 0,
            }));
        }
        state
            .editor
            .execute(Command::RenameLayer {
                id: state.editor.selected().unwrap(),
                name: "Redo survives".into(),
            })
            .unwrap();
        state.editor.undo();
        let before = state.editor.project().clone();
        let clipboard = state.contents_clipboard().cloned();
        let selection = Selection::default();
        state.contents_selection = None;
        let target = binding(&state, &selection);
        assert!(execute(&mut state, &target, Operation::Paste, apply).is_none());
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.contents_clipboard(), clipboard.as_ref());
        assert!(state.editor.can_redo());
    }
}

#[test]
fn canceled_pointer_flush_does_not_strand_the_next_ordinary_field_commit() {
    for pending in [None, Some("unsupported-field".to_string())] {
        let mut state = scene();
        let selection = select(&mut state, 0, &[1]);
        let (session, target) = session(&state, &selection);
        session.borrow_mut().prepare(
            &target,
            &state,
            &selection,
            &BTreeSet::new(),
            pending.clone(),
        );
        assert_eq!(
            session
                .borrow_mut()
                .finish_flush(&target, &state, &selection, &BTreeSet::new()),
            pending.is_none()
        );
        // Simulate release outside: native InputPress is discarded, no execute.
        session
            .borrow_mut()
            .observe(&state, &selection, &BTreeSet::new());
        let fresh = session.borrow().binding.clone().unwrap();
        let parameter = Some(ContentsParam::Transform(Property::PositionX));
        assert_eq!(
            submit_singleton(&session, &fresh, parameter, &mut state, "23", apply),
            "23"
        );
        assert_eq!(
            contents(&state)
                .node(1)
                .unwrap()
                .value_at(parameter.unwrap(), 0),
            23.
        );
    }
}
