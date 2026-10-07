use super::*;
use libre_effects_core::{ContentsKind, GradientParam};

fn scene() -> EditorState {
    let mut state = EditorState::default();
    state
        .editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(Default::default()),
            width: 200.,
            height: 120.,
            name: "Colors lanes".into(),
        })
        .unwrap();
    for _ in 0..2 {
        state
            .editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 0,
                    kind: ContentsKind::GradientFill {
                        even_odd: false,
                        gradient: Default::default(),
                    },
                },
            })
            .unwrap();
    }
    state.bulk_test_action(&Action::Select(1));
    state.expanded = true;
    for item in 1..=2 {
        edit(
            &mut state,
            item,
            GradientColorsEdit::SetAnimation {
                frame: 0,
                enabled: true,
            },
        );
        edit(
            &mut state,
            item,
            GradientColorsEdit::ToggleKey { frame: 30 },
        );
    }
    state.editor.clear_history();
    state
}
fn edit(state: &mut EditorState, item: u64, edit: GradientColorsEdit) {
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::GradientColors { item, edit },
    }));
}
fn bound(state: &EditorState, selected: bool) -> (Rc<RefCell<Input>>, Target) {
    let input = Rc::new(RefCell::new(Input::default()));
    input.borrow_mut().observe(state);
    if selected {
        input.borrow_mut().select_frame(state, 1, state.frame);
    }
    let target = input.borrow().target(1).unwrap();
    (input, target)
}
fn animation(state: &EditorState, item: u64) -> &GradientColorsAnimation {
    let Content::ShapeContents(c) = state.editor.selected_layer().unwrap().content() else {
        panic!()
    };
    c.node(item)
        .unwrap()
        .kind
        .gradient()
        .unwrap()
        .colors_animation()
        .unwrap()
}
fn apply_move(state: &mut EditorState, command: Command, frame: Frame) -> bool {
    state.bulk_test_action(&Action::Edit(command));
    if state.status != "Edited" {
        return false;
    }
    state.bulk_test_action(&Action::Seek(frame));
    true
}

#[test]
fn compound_timeline_rows_include_each_gradient_and_honor_animated_filter() {
    let mut state = scene();
    edit(
        &mut state,
        2,
        GradientColorsEdit::SetAnimation {
            frame: 0,
            enabled: false,
        },
    );
    let layer = state.editor.selected_layer().unwrap();
    assert_eq!(
        visible_nodes(layer, None)
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(
        visible_nodes(layer, Some(PropertyFilter::Animated))
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        [1]
    );
    assert!(visible_nodes(layer, Some(PropertyFilter::Position)).is_empty());
}

#[test]
fn compound_timeline_exact_selection_owns_mode_delete_and_move() {
    let mut state = scene();
    let (input, target) = bound(&state, false);
    assert!(target.command(&state, Control::Remove).is_none());
    assert!(
        target
            .command(&state, Control::Mode(GradientColorsInterpolation::Linear))
            .is_none()
    );
    assert!(move_command(&target, &state, "10").is_err());
    input.borrow_mut().select_frame(&state, 1, 0);
    assert!(!input.borrow().current(&target, &state));
    let selected = input.borrow().target(1).unwrap();
    let other = input.borrow().target(2).unwrap();
    assert!(selected.command(&state, Control::Remove).is_some());
    assert!(other.command(&state, Control::Remove).is_none());
    assert!(
        other
            .command(&state, Control::Mode(GradientColorsInterpolation::Smooth))
            .is_none()
    );
    state.bulk_test_action(&Action::Seek(15));
    input.borrow_mut().observe(&state);
    assert_eq!(input.borrow().selected.as_ref().unwrap().frames, [0].into());
    assert!(!input.borrow().current(&selected, &state));
    let between = input.borrow().target(1).unwrap();
    assert!(
        between
            .command(&state, Control::Mode(GradientColorsInterpolation::Linear))
            .is_some()
    );
    assert!(between.command(&state, Control::Add).is_some());
    assert!(matches!(
        between.command(&state, Control::Previous),
        Some(Action::Seek(0))
    ));
    assert!(matches!(
        between.command(&state, Control::Next),
        Some(Action::Seek(30))
    ));
}

#[test]
fn compound_timeline_rejects_invalid_unchanged_and_foreign_pending_before_blur() {
    let state = scene();
    let (input, target) = bound(&state, true);
    let before = state.editor.project().clone();
    assert!(input.borrow().prepare(&target, &state, None));
    for binding in [
        "invalid-opacity",
        "unchanged-source-text",
        "foreign-properties",
        "colors-move",
    ] {
        assert!(
            !input
                .borrow()
                .prepare(&target, &state, Some(binding.into()))
        );
    }
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn compound_timeline_retires_source_transport_and_selection_round_trips() {
    for actions in [
        vec![Action::Seek(9), Action::Seek(0)],
        vec![Action::Play, Action::Play],
        vec![
            Action::Edit(Command::Contents {
                id: 1,
                edit: ContentsEdit::Enabled {
                    item: 1,
                    enabled: false,
                },
            }),
            Action::Undo,
        ],
        vec![Action::Select(2), Action::Select(1)],
    ] {
        let mut state = scene();
        let (input, target) = bound(&state, true);
        for action in actions {
            state.bulk_test_action(&action);
        }
        assert!(!input.borrow().current(&target, &state));
        input.borrow_mut().observe(&state);
        assert!(!input.borrow().current(&target, &state));
    }
    let state = scene();
    let (input, target) = bound(&state, true);
    input.borrow_mut().select_frame(&state, 2, 0);
    input.borrow_mut().select_frame(&state, 1, 0);
    assert!(!input.borrow().current(&target, &state));
}

#[test]
fn compound_timeline_rejects_blocked_owner_and_hidden_or_changed_selection() {
    for change in 0..10 {
        let mut state = scene();
        let (input, target) = bound(&state, true);
        match change {
            0 => {
                state.playing = true;
                false
            }
            1 => {
                state.queue_open = true;
                false
            }
            2 => state.selected_layers.insert(2),
            3 => {
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
                false
            }
            4 => {
                state.expanded = false;
                false
            }
            5 => {
                state.property_filter = Some(PropertyFilter::Animated);
                false
            }
            6 => {
                state.graph_open = true;
                false
            }
            7 => {
                state.contents_selection =
                    Some((state.editor.project().active_composition_id(), 1, 2));
                false
            }
            8 => {
                state.tool = crate::editor::Tool::Pen;
                false
            }
            _ => {
                state.selected_keys.insert(libre_effects_core::KeyRef {
                    id: 1,
                    property: libre_effects_core::Property::Opacity.into(),
                    frame: 0,
                });
                false
            }
        };
        assert!(!input.borrow().current(&target, &state), "case {change}");
    }
}

#[test]
fn compound_timeline_move_validates_whole_frames_collision_bounds_and_noop_redo() {
    let mut state = scene();
    edit(
        &mut state,
        1,
        GradientColorsEdit::SetInterpolation {
            frame: 0,
            interpolation: GradientColorsInterpolation::Linear,
        },
    );
    state.bulk_test_action(&Action::Undo);
    assert!(state.editor.can_redo());
    let (input, target) = bound(&state, true);
    let before = state.editor.project().clone();
    for text in ["-1", "1.5", "NaN", "4294967296", "30", "99999"] {
        assert_eq!(
            submit_move(&input, &target, &mut state, text, true, apply_move),
            "0"
        );
        assert_eq!(state.editor.project(), &before);
        assert!(state.editor.can_redo());
    }
    assert_eq!(
        submit_move(&input, &target, &mut state, " 000 ", true, apply_move),
        "0"
    );
    assert!(state.editor.can_redo());
    assert_eq!(
        submit_move(&input, &target, &mut state, "12", true, apply_move),
        "12"
    );
    assert_eq!(state.frame, 12);
    assert_eq!(
        animation(&state, 1)
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [12, 30]
    );
    assert_eq!(
        animation(&state, 2)
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [0, 30]
    );
    assert_eq!(
        input.borrow().selected.as_ref().unwrap().frames,
        [12].into()
    );
    assert!(!input.borrow().current(&target, &state));
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn compound_timeline_delayed_or_removed_key_move_never_applies() {
    let mut state = scene();
    let (input, target) = bound(&state, true);
    edit(&mut state, 1, GradientColorsEdit::DeleteKey { frame: 0 });
    input.borrow_mut().observe(&state);
    let before = state.editor.project().clone();
    assert!(input.borrow().selected.is_none());
    assert_eq!(
        submit_move(&input, &target, &mut state, "12", true, apply_move),
        "0"
    );
    assert_eq!(state.editor.project(), &before);
    let (input, target) = bound(&state, false);
    assert_eq!(
        submit_move(&input, &target, &mut state, "12", false, apply_move),
        "0"
    );
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn compound_timeline_status_exposes_topology_fallback_and_boundary_holds() {
    let mut state = scene();
    edit(
        &mut state,
        1,
        GradientColorsEdit::SetInterpolation {
            frame: 0,
            interpolation: GradientColorsInterpolation::Smooth,
        },
    );
    assert!(segment_label(animation(&state, 1), 15).contains("Smoothstep"));
    edit(
        &mut state,
        1,
        GradientColorsEdit::AddStop {
            frame: 30,
            opacity: true,
            position: 0.5,
        },
    );
    assert!(
        segment_label(animation(&state, 1), 15).contains("Hold fallback: stop IDs/order differ")
    );
    assert!(segment_label(animation(&state, 1), 30).contains("last key holds"));
    edit(
        &mut state,
        1,
        GradientColorsEdit::MoveKey { from: 0, to: 10 },
    );
    assert!(segment_label(animation(&state, 1), 0).contains("before first key"));
}

#[test]
fn compound_timeline_static_legacy_stop_animation_cannot_enable_colors() {
    let mut state = scene();
    edit(
        &mut state,
        1,
        GradientColorsEdit::SetAnimation {
            frame: 0,
            enabled: false,
        },
    );
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item: 1,
            parameter: libre_effects_core::ContentsParam::Gradient(GradientParam::Red(1)),
            edit: libre_effects_core::TrackEdit::ToggleAnimation { frame: 0 },
        },
    }));
    let (_, target) = bound(&state, false);
    assert!(target.command(&state, Control::Enable).is_none());
}

#[test]
fn compound_timeline_wiring_keeps_colors_outside_scalar_graph_and_drags() {
    let source = include_str!("../timeline.rs");
    assert!(source.contains("rows = rows.child(self.colors.render_rows("));
    assert!(source.contains("self.colors.observe(state)"));
    assert!(source.contains("this.colors.clear_selection()"));
    assert!(source.contains("this.colors.key_down(event, &this.state, window, cx)"));
    let module = include_str!("compound_colors.rs");
    assert!(module.contains("TextField::active_pending_binding(cx)"));
    assert!(module.contains("TextField::is_composing(window, cx)"));
    assert!(module.contains("if event.is_held"));
    assert!(module.contains("field.sync_guarded("));
    assert!(!module.contains("PropertyPath::"));
    assert!(
        module
            .contains(".on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())")
    );
    assert!(!module.contains("KeyDrag"));
}

#[test]
fn compound_timeline_edit_requires_exact_existing_contents_owner() {
    let mut state = scene();
    let composition = state.editor.project().active_composition_id();
    for previous in [None, Some((composition, 1, 2))] {
        state.contents_selection = previous;
        let (_, target) = bound(&state, false);
        assert!(target.command(&state, Control::Edit).is_none());
        assert_eq!(state.contents_selection, previous);
    }
    state.contents_selection = Some((composition, 1, 1));
    let (_, target) = bound(&state, false);
    assert!(matches!(
        target.command(&state, Control::Edit),
        Some(Action::OpenGradient(1))
    ));
    let session = crate::panels::gradient_editor::Session::new(&state, 1).unwrap();
    assert!(session.current(&state));
}

#[test]
fn compound_timeline_shared_ownership_retires_on_explicit_domain_switch() {
    let state = scene();
    let (input, target) = bound(&state, true);
    assert!(state.colors_key_owned.get());
    state.colors_key_owned.set(false);
    assert!(!input.borrow().current(&target, &state));
    input.borrow_mut().observe(&state);
    assert!(input.borrow().selected.is_none());
    assert!(!state.colors_key_owned.get());
    input.borrow_mut().select_frame(&state, 1, 0);
    assert!(state.colors_key_owned.get());
    input.borrow_mut().select(None);
    assert!(!state.colors_key_owned.get());
}

#[test]
fn compound_timeline_move_returns_enter_and_escape_to_timeline_focus() {
    let timeline = include_str!("../timeline.rs");
    let colors = include_str!("compound_colors.rs");
    let field = include_str!("../../components/text_field.rs");
    assert!(
        timeline.contains("self.input_source.clone(),\n                    self.focus.clone(),")
    );
    assert!(colors.contains(".return_focus(return_focus.clone())"));
    assert!(
        field.contains(
            "if let Some(focus) = &self.return_focus {\n            window.focus(focus);"
        )
    );
    let enter = field
        .split("\"enter\" => {")
        .nth(1)
        .unwrap()
        .split("\"escape\" => {")
        .next()
        .unwrap();
    let escape = field
        .split("\"escape\" => {")
        .nth(1)
        .unwrap()
        .split("\"backspace\" => {")
        .next()
        .unwrap();
    assert!(enter.contains("self.submit(window, cx)"));
    assert!(enter.contains("self.finish_input(window)"));
    assert!(escape.contains("self.finish_input(window)"));
}

#[test]
fn compound_timeline_shift_toggles_same_paint_and_replaces_other_paints() {
    let state = scene();
    let (input, old) = bound(&state, true);
    input.borrow_mut().select_key(&state, 1, 30, true);
    assert_eq!(
        input.borrow().selected.as_ref().unwrap().frames,
        [0, 30].into()
    );
    assert!(!input.borrow().current(&old, &state));
    input.borrow_mut().select_key(&state, 1, 0, true);
    assert_eq!(
        input.borrow().selected.as_ref().unwrap().frames,
        [30].into()
    );
    input.borrow_mut().select_key(&state, 2, 0, true);
    let selected = input.borrow().selected.clone().unwrap();
    assert_eq!(selected.item, 2);
    assert_eq!(selected.frames, [0].into());
    input.borrow_mut().select_key(&state, 2, 30, false);
    assert_eq!(
        input.borrow().selected.as_ref().unwrap().frames,
        [30].into()
    );
    input.borrow_mut().select_key(&state, 2, 30, true);
    assert!(input.borrow().selected.is_none());
    assert!(!state.colors_key_owned.get());
}

#[test]
fn compound_timeline_group_move_anchors_earliest_even_away_from_playhead() {
    let mut state = scene();
    let (input, _) = bound(&state, true);
    input.borrow_mut().select_key(&state, 1, 30, true);
    state.bulk_test_action(&Action::Seek(15));
    input.borrow_mut().observe(&state);
    let target = input.borrow().target(1).unwrap();
    let before = state.editor.project().clone();
    assert_eq!(
        submit_move(&input, &target, &mut state, "10", true, apply_move),
        "10"
    );
    assert_eq!(
        animation(&state, 1)
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [10, 40]
    );
    assert_eq!(
        input.borrow().selected.as_ref().unwrap().frames,
        [10, 40].into()
    );
    assert_eq!(state.frame, 10);
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn compound_timeline_group_move_allows_selected_overlap_and_rejects_outside_bounds() {
    let mut state = scene();
    edit(&mut state, 1, GradientColorsEdit::ToggleKey { frame: 60 });
    let (input, _) = bound(&state, true);
    input.borrow_mut().select_key(&state, 1, 30, true);
    let target = input.borrow().target(1).unwrap();
    assert!(move_command(&target, &state, "30").is_err()); // unselected 60
    let duration = state.editor.project().composition().duration();
    assert!(move_command(&target, &state, &(duration - 1).to_string()).is_err());
    assert!(move_command(&target, &state, &Frame::MAX.to_string()).is_err());
    edit(&mut state, 1, GradientColorsEdit::DeleteKey { frame: 60 });
    input.borrow_mut().observe(&state);
    input.borrow_mut().select_frames(&state, 1, [0, 30].into());
    let target = input.borrow().target(1).unwrap();
    assert!(move_command(&target, &state, "30").unwrap().is_some());
    assert_eq!(
        submit_move(&input, &target, &mut state, "30", true, apply_move),
        "30"
    );
    assert_eq!(
        animation(&state, 1)
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [30, 60]
    );
}

#[test]
fn compound_timeline_group_modes_and_delete_use_selection_and_current_playhead() {
    let mut state = scene();
    let (input, _) = bound(&state, true);
    input.borrow_mut().select_key(&state, 1, 30, true);
    state.bulk_test_action(&Action::Seek(17));
    input.borrow_mut().observe(&state);
    let target = input.borrow().target(1).unwrap();
    let action = target
        .command(&state, Control::Mode(GradientColorsInterpolation::Smooth))
        .unwrap();
    state.bulk_test_action(&action);
    assert_eq!(
        animation(&state, 1).interpolation(0),
        Some(GradientColorsInterpolation::Smooth)
    );
    assert_eq!(
        animation(&state, 1).interpolation(30),
        Some(GradientColorsInterpolation::Smooth)
    );
    input
        .borrow_mut()
        .finish_mode(&state, target.selection.clone().unwrap());
    let target = input.borrow().target(1).unwrap();
    assert!(
        matches!(target.command(&state, Control::Remove), Some(Action::Edit(Command::Contents {
        edit: ContentsEdit::GradientColors { edit: GradientColorsEdit::DeleteKeys { frames, frame: 17 }, .. }, ..
    })) if frames == [0, 30].into())
    );
}

fn copy_group(state: &EditorState) -> Rc<RefCell<Input>> {
    let (input, _) = bound(state, true);
    input.borrow_mut().select_key(state, 1, 30, true);
    let target = input.borrow().target(1).unwrap();
    assert!(input.borrow_mut().copy_selected(&target, state));
    assert!(!input.borrow().current(&target, state));
    input
}
fn paste(input: &Rc<RefCell<Input>>, state: &mut EditorState) -> bool {
    let target = input.borrow().target(1).unwrap();
    let Some(action) = input.borrow().paste_command(&target, state) else {
        return false;
    };
    let clipboard = input.borrow().clipboard.clone().unwrap();
    let frame = state.frame;
    state.bulk_test_action(&action);
    if state.status == "Edited" {
        input.borrow_mut().finish_paste(state, clipboard, frame);
        true
    } else {
        input.borrow_mut().observe(state);
        false
    }
}

#[test]
fn compound_timeline_clipboard_preserves_snapshot_offsets_modes_and_repeats_after_own_paste() {
    let mut state = scene();
    edit(
        &mut state,
        1,
        GradientColorsEdit::SetInterpolation {
            frame: 0,
            interpolation: GradientColorsInterpolation::Linear,
        },
    );
    edit(
        &mut state,
        1,
        GradientColorsEdit::SetInterpolation {
            frame: 30,
            interpolation: GradientColorsInterpolation::Smooth,
        },
    );
    edit(
        &mut state,
        1,
        GradientColorsEdit::AddStop {
            frame: 30,
            opacity: true,
            position: 0.3,
        },
    );
    let originals = animation(&state, 1).keys().clone();
    let input = copy_group(&state);
    let stale = input.borrow().target(1).unwrap();
    state.bulk_test_action(&Action::Seek(50));
    input.borrow_mut().observe(&state);
    assert!(!input.borrow().current(&stale, &state));
    assert!(input.borrow().clipboard.is_some());
    assert!(state.colors_key_owned.get());
    assert!(paste(&input, &mut state));
    assert_eq!(
        input.borrow().selected.as_ref().unwrap().frames,
        [50, 80].into()
    );
    for start in [50, 100] {
        if start == 100 {
            state.bulk_test_action(&Action::Seek(start));
            input.borrow_mut().observe(&state);
            assert!(paste(&input, &mut state));
        }
        let keys = animation(&state, 1);
        assert_eq!(keys.keys().get(&start), originals.get(&0));
        assert_eq!(keys.keys().get(&(start + 30)), originals.get(&30));
        assert_eq!(
            keys.interpolation(start),
            Some(GradientColorsInterpolation::Linear)
        );
        assert_eq!(
            keys.interpolation(start + 30),
            Some(GradientColorsInterpolation::Smooth)
        );
    }
    assert!(input.borrow().clipboard.is_some());
    assert_eq!(animation(&state, 2).keys().len(), 2);
}

#[test]
fn compound_timeline_clipboard_never_revives_after_source_history_or_domain_aba() {
    for case in 0..8 {
        let mut state = scene();
        let input = copy_group(&state);
        let before = state.editor.project().clone();
        match case {
            0 => {
                edit(
                    &mut state,
                    1,
                    GradientColorsEdit::SetInterpolation {
                        frame: 0,
                        interpolation: GradientColorsInterpolation::Linear,
                    },
                );
                state.bulk_test_action(&Action::Undo);
            }
            1 => {
                state.bulk_test_action(&Action::Undo);
            } // even empty history
            2 => {
                state.bulk_test_action(&Action::Redo);
            }
            3 => {
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
            }
            4 => {
                state.bulk_test_action(&Action::Select(2));
                state.bulk_test_action(&Action::Select(1));
            }
            5 => {
                input.borrow_mut().select_frame(&state, 2, 0);
                input.borrow_mut().select_frame(&state, 1, 0);
            }
            6 => {
                state.retire_colors_clipboard(); // direct Contents/shell domain switch
                state.contents_selection =
                    Some((state.editor.project().active_composition_id(), 1, 2));
                state.contents_selection = None;
            }
            _ => {
                state = scene();
            } // source-equal project replacement, different latch
        }
        assert_eq!(state.editor.project(), &before);
        if let Some(target) = input.borrow().target(1) {
            assert!(
                input.borrow().paste_command(&target, &state).is_none(),
                "case {case}"
            );
        }
        input.borrow_mut().observe(&state);
        assert!(input.borrow().clipboard.is_none(), "case {case}");
        input.borrow_mut().select_frame(&state, 1, 0);
        let target = input.borrow().target(1).unwrap();
        assert!(
            input.borrow().paste_command(&target, &state).is_none(),
            "case {case}"
        );
    }
}

#[test]
fn compound_timeline_clipboard_requires_explicit_same_paint_selection_and_rejects_pending() {
    let mut state = scene();
    let input = copy_group(&state);
    let target = input.borrow().target(1).unwrap();
    let other = input.borrow().target(2).unwrap();
    assert!(input.borrow().paste_command(&target, &state).is_some());
    assert!(input.borrow().paste_command(&other, &state).is_none());
    assert!(!input.borrow().prepare(
        &target,
        &state,
        Some("invalid or unchanged pending field".into())
    ));
    state.queue_open = true;
    input.borrow_mut().observe(&state);
    state.queue_open = false;
    input.borrow_mut().observe(&state);
    assert!(input.borrow().clipboard.is_none());
    let input = copy_group(&state);
    input.borrow_mut().select(None);
    assert!(input.borrow().clipboard.is_none());
    assert!(!state.colors_key_owned.get());
}

#[test]
fn compound_timeline_clipboard_collisions_reject_without_source_or_history_change() {
    let mut state = scene();
    let input = copy_group(&state);
    state.bulk_test_action(&Action::Seek(30));
    input.borrow_mut().observe(&state);
    let before = state.editor.project().clone();
    assert!(!paste(&input, &mut state)); // existing 30 plus absent 60 is partial collision
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
    assert!(input.borrow().clipboard.is_none()); // rejected edit also retires receipt
}

#[test]
fn compound_timeline_copy_and_exact_paste_preserve_redo() {
    let mut state = scene();
    edit(
        &mut state,
        1,
        GradientColorsEdit::SetInterpolation {
            frame: 0,
            interpolation: GradientColorsInterpolation::Linear,
        },
    );
    state.bulk_test_action(&Action::Undo);
    let before = state.editor.project().clone();
    let input = copy_group(&state);
    assert!(state.editor.can_redo());
    assert!(paste(&input, &mut state));
    assert_eq!(state.editor.project(), &before);
    assert!(state.editor.can_redo());
    assert!(input.borrow().clipboard.is_some());
}

#[test]
fn compound_timeline_only_key_selection_accepts_shift_and_rejects_other_modifiers() {
    let shift = gpui::Modifiers {
        shift: true,
        ..Default::default()
    };
    assert!(modifiers_allowed(shift, Control::Select(0)));
    for control in [
        Control::Copy,
        Control::Paste,
        Control::Remove,
        Control::Mode(GradientColorsInterpolation::Hold),
    ] {
        assert!(!modifiers_allowed(shift, control));
    }
    for modifier in 0..4 {
        let mut flags = shift;
        match modifier {
            0 => flags.control = true,
            1 => flags.alt = true,
            2 => flags.platform = true,
            _ => flags.function = true,
        }
        assert!(!modifiers_allowed(flags, Control::Select(0)));
    }
}

#[test]
fn compound_timeline_selection_retires_source_aba_but_keeps_destructive_domain_tombstone() {
    for case in 0..3 {
        let mut state = scene();
        let (input, old) = bound(&state, true);
        let before = state.editor.project().clone();
        match case {
            0 => {
                edit(&mut state, 1, GradientColorsEdit::DeleteKey { frame: 0 });
                edit(&mut state, 1, GradientColorsEdit::ToggleKey { frame: 0 });
            }
            1 => {
                edit(
                    &mut state,
                    1,
                    GradientColorsEdit::SetInterpolation {
                        frame: 0,
                        interpolation: GradientColorsInterpolation::Linear,
                    },
                );
                state.bulk_test_action(&Action::Undo);
            }
            _ => {
                edit(
                    &mut state,
                    1,
                    GradientColorsEdit::SetInterpolation {
                        frame: 0,
                        interpolation: GradientColorsInterpolation::Linear,
                    },
                );
                state.bulk_test_action(&Action::Undo);
                let target = input.borrow().target(1).unwrap();
                assert!(!input.borrow().current(&target, &state));
                state.bulk_test_action(&Action::Redo);
            }
        }
        if case != 2 {
            assert_eq!(state.editor.project(), &before);
        }
        assert!(!input.borrow().current(&old, &state));
        input.borrow_mut().observe(&state);
        assert!(input.borrow().selected.is_none());
        assert!(input.borrow().domain_owned);
        assert!(state.colors_key_owned.get());
        let fresh = input.borrow().target(1).unwrap();
        assert!(fresh.command(&state, Control::Remove).is_none());
        let source = state.editor.project().clone();
        for _ in 0..3 {
            state.bulk_test_action(&Action::DeleteSelection);
        }
        assert_eq!(state.editor.project(), &source);
        assert_eq!(state.editor.project().composition().layers().len(), 1);
    }
}

#[test]
fn compound_timeline_delete_does_not_allow_repeated_delete_to_remove_layer() {
    let mut state = scene();
    let (input, target) = bound(&state, true);
    let action = target.command(&state, Control::Remove).unwrap();
    state.bulk_test_action(&action);
    input.borrow_mut().observe(&state);
    assert!(input.borrow().selected.is_none());
    assert!(input.borrow().domain_owned);
    assert!(state.colors_key_owned.get());
    let after = state.editor.project().clone();
    for action in [
        Action::DeleteSelection,
        Action::CutSelection,
        Action::PasteSelection,
        Action::DuplicateSelection,
    ] {
        state.bulk_test_action(&action);
        assert_eq!(state.editor.project(), &after);
    }
    input.borrow_mut().select(None);
    assert!(!state.colors_key_owned.get());
    assert!(!input.borrow().domain_owned);
}

#[test]
fn compound_timeline_shell_modal_retires_before_blur_and_cannot_revive_after_close() {
    let mut state = scene();
    let input = copy_group(&state);
    let target = input.borrow().target(1).unwrap();
    let before = state.editor.project().clone();
    state.retire_colors_context();
    assert_eq!(
        submit_move(&input, &target, &mut state, "12", true, apply_move),
        "0"
    );
    assert_eq!(state.editor.project(), &before);
    input.borrow_mut().observe(&state);
    assert!(input.borrow().selected.is_none());
    assert!(input.borrow().clipboard.is_none());
    input.borrow_mut().select_frame(&state, 1, 0);
    let target = input.borrow().target(1).unwrap();
    assert!(input.borrow().paste_command(&target, &state).is_none());
    let shell = include_str!("../../shell.rs");
    for (start, end) in [
        ("fn open_settings", "fn new_composition"),
        ("fn open_about", "fn close_about"),
        ("fn run_menu", "fn open_about"),
    ] {
        let body = shell
            .split(start)
            .nth(1)
            .unwrap()
            .split(end)
            .next()
            .unwrap();
        assert!(
            body.find("retire_colors_context()").unwrap() < body.find("window.focus(").unwrap()
        );
    }
}

#[test]
fn compound_timeline_direct_domain_epoch_retires_equal_callbacks_and_selection() {
    let mut state = scene();
    let (input, target) = bound(&state, true);
    state.retire_colors_clipboard();
    assert!(!target.owner.current(&state));
    assert!(!input.borrow().current(&target, &state));
    input.borrow_mut().observe(&state);
    assert!(input.borrow().selected.is_none());
    assert!(state.colors_key_owned.get());
}

#[test]
fn compound_timeline_selected_details_cannot_expand_the_ruler_coordinate_space() {
    let mut row = bounded_row();
    assert_eq!(row.style().size.width, Some(relative(1.).into()));
    assert_eq!(row.style().max_size.width, Some(relative(1.).into()));
    assert_eq!(row.style().min_size.width, Some(px(0.).into()));
    let mut help = note(
        "Long selected-key help must wrap within the viewport, leaving the lane and ruler widths unchanged.",
    );
    assert_eq!(help.style().size.width, Some(relative(1.).into()));
    assert_eq!(help.style().max_size.width, Some(relative(1.).into()));
    assert_eq!(help.style().min_size.width, Some(px(0.).into()));
    assert_eq!(
        help.text_style().as_ref().unwrap().white_space,
        Some(gpui::WhiteSpace::Normal)
    );
    let source = include_str!("compound_colors.rs");
    assert!(source.contains("let mut rows = bounded_row().flex().flex_col()"));
    assert!(source.contains("let mut detail = bounded_row()"));
    assert!(source.contains("rows = rows.child(note(summary))"));
    assert!(source.contains("rows = rows.child(detail).child(note("));
    let timeline = include_str!("../timeline.rs");
    assert!(timeline.contains(".id(\"timeline-rows\")\n                            .min_w_0()"));
}

fn pointer_geometry(state: &EditorState) -> PointerGeometry {
    PointerGeometry {
        bounds: gpui::Bounds::new(
            gpui::point(px(80.), px(240.)),
            gpui::size(px(state.visible_frames() as f32 * 4.), px(29.)),
        ),
        start: state.timeline_start,
        visible: state.visible_frames(),
    }
}
fn pointer_press(
    state: &EditorState,
    frames: &[Frame],
    key: Frame,
    shift: bool,
) -> (Input, PointerGeometry) {
    let mut input = Input::default();
    input.observe(state);
    input.select_frames(state, 1, frames.iter().copied().collect());
    let target = input.target(1).unwrap();
    let geometry = pointer_geometry(state);
    assert!(input.begin_pointer(&target, state, key, 101., shift, geometry));
    (input, geometry)
}
fn pointer_apply(state: &mut EditorState, action: Action) -> bool {
    let edit = matches!(action, Action::Edit(_));
    state.bulk_test_action(&action);
    !edit || state.status == "Edited"
}

#[test]
fn compound_pointer_press_selected_group_waits_until_click_release_to_collapse() {
    let mut state = scene();
    let before = state.editor.project().clone();
    let (mut input, geometry) = pointer_press(&state, &[0, 30], 30, false);
    assert_eq!(input.selected_frames(), [0, 30].into());
    assert_eq!(state.frame, 0);
    assert_eq!(state.editor.project(), &before);
    let mut calls = 0;
    assert!(
        input.finish_pointer(&mut state, geometry, 101., false, true, |state, action| {
            calls += 1;
            assert!(matches!(action, Action::Seek(30)));
            pointer_apply(state, action)
        })
    );
    assert_eq!(calls, 1);
    assert_eq!(state.frame, 30);
    assert_eq!(input.selected_frames(), [30].into());
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
}

#[test]
fn compound_pointer_horizontal_threshold_is_inclusive_and_off_center_press_has_no_jump() {
    let mut state = scene();
    state.snapping = false;
    let (mut input, geometry) = pointer_press(&state, &[0, 30], 30, false);
    let before = state.editor.project().clone();
    input.update_pointer(&state, geometry, 104.999, false);
    assert!(!input.pointer.as_ref().unwrap().crossed);
    assert_eq!(input.pointer.as_ref().unwrap().delta, 0);
    input.update_pointer(&state, geometry, 105., false);
    assert!(input.pointer.as_ref().unwrap().crossed);
    assert_eq!(input.pointer.as_ref().unwrap().delta, 1);
    assert_eq!(state.editor.project(), &before);
    assert_eq!(state.frame, 0);
    assert!(!state.editor.can_undo());
    let mut calls = 0;
    input.finish_pointer(&mut state, geometry, 105., false, false, |state, action| {
        calls += 1;
        assert!(matches!(
            action,
            Action::Edit(Command::Contents {
                edit: ContentsEdit::GradientColors {
                    edit: GradientColorsEdit::MoveKeys { to: 1, .. },
                    ..
                },
                ..
            })
        ));
        pointer_apply(state, action)
    });
    assert_eq!(calls, 1);
    assert_eq!(input.selected_frames(), [1, 31].into());
    assert_eq!(state.frame, 0, "drag must not seek");
    state.bulk_test_action(&Action::Undo);
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
}

#[test]
fn compound_pointer_shift_is_selection_only_and_never_retimes_after_threshold() {
    let mut state = scene();
    let before = state.editor.project().clone();
    let (mut input, geometry) = pointer_press(&state, &[0], 30, true);
    assert_eq!(input.selected_frames(), [0].into());
    input.update_pointer(&state, geometry, 181., false);
    assert_eq!(input.pointer.as_ref().unwrap().delta, 0);
    input.finish_pointer(&mut state, geometry, 101., false, true, |_, _| {
        panic!("Shift drag must not become click or edit")
    });
    assert_eq!(input.selected_frames(), [0].into());
    assert_eq!(state.editor.project(), &before);
    let target = input.target(1).unwrap();
    assert!(input.begin_pointer(&target, &state, 30, 101., true, geometry));
    input.finish_pointer(&mut state, geometry, 101., false, true, pointer_apply);
    assert_eq!(input.selected_frames(), [0, 30].into());
    let target = input.target(1).unwrap();
    assert!(input.begin_pointer(&target, &state, 30, 101., true, geometry));
    input.finish_pointer(&mut state, geometry, 101., false, true, pointer_apply);
    assert_eq!(input.selected_frames(), [0].into());
    assert_eq!(state.editor.project(), &before);
}

#[test]
fn compound_pointer_no_op_cancel_and_duplicate_release_preserve_redo_and_group() {
    for case in 0..4 {
        let mut state = scene();
        edit(
            &mut state,
            1,
            GradientColorsEdit::SetInterpolation {
                frame: 0,
                interpolation: GradientColorsInterpolation::Smooth,
            },
        );
        let redone = state.editor.project().clone();
        state.bulk_test_action(&Action::Undo);
        let before = state.editor.project().clone();
        let (mut input, geometry) = pointer_press(&state, &[0, 30], 30, false);
        if case != 0 {
            input.update_pointer(&state, geometry, 181., true);
        }
        if case == 2 {
            assert!(input.cancel_pointer());
        }
        if case == 3 {
            input.update_pointer(&state, geometry, f32::NAN, true);
        }
        input.finish_pointer(&mut state, geometry, 101., true, false, |_, _| {
            panic!("no-op/cancel cannot dispatch")
        });
        assert!(
            !input.finish_pointer(&mut state, geometry, 201., true, false, |_, _| panic!(
                "duplicate release"
            ))
        );
        assert_eq!(input.selected_frames(), [0, 30].into());
        assert_eq!(state.editor.project(), &before);
        assert!(state.editor.can_redo());
        state.bulk_test_action(&Action::Redo);
        assert_eq!(state.editor.project(), &redone);
    }
}

#[test]
fn compound_pointer_snaps_to_same_paint_and_playhead_with_alt_bypass() {
    let mut state = scene();
    state.snapping = true;
    state.bulk_test_action(&Action::Seek(10));
    let (mut input, geometry) = pointer_press(&state, &[0, 30], 0, false);
    input.update_pointer(&state, geometry, 137., false); // raw +9, playhead +10 within 4px
    assert_eq!(input.pointer.as_ref().unwrap().delta, 10);
    assert_eq!(input.pointer.as_ref().unwrap().snapped, Some(10));
    input.update_pointer(&state, geometry, 137., true);
    assert_eq!(input.pointer.as_ref().unwrap().delta, 9);
    assert_eq!(input.pointer.as_ref().unwrap().snapped, None);
    input.update_pointer(&state, geometry, 131., false); // +7.5 -> distance10px, outside8px
    assert_eq!(input.pointer.as_ref().unwrap().delta, 8);
    assert_eq!(input.pointer.as_ref().unwrap().snapped, None);
    input.cancel_pointer();
    state.bulk_test_action(&Action::Seek(0));
    let (mut input, geometry) = pointer_press(&state, &[0], 0, false);
    input.update_pointer(&state, geometry, 213., false); // +28 -> unselected30 at8px
    assert_eq!(input.pointer.as_ref().unwrap().delta, 30);
    assert_eq!(input.pointer.as_ref().unwrap().snapped, Some(30));
    assert!(input.pointer.as_ref().unwrap().invalid);
    input.finish_pointer(&mut state, geometry, 213., false, false, |_, _| {
        panic!("snap collision rejects atomically")
    });
    assert!(!state.editor.can_undo());
    // Another paint's key at 18 must not be a snap target.
    edit(&mut state, 2, GradientColorsEdit::ToggleKey { frame: 18 });
    let (mut input, geometry) = pointer_press(&state, &[0], 0, false);
    input.update_pointer(&state, geometry, 169., false);
    assert_eq!(input.pointer.as_ref().unwrap().delta, 17);
    assert_eq!(input.pointer.as_ref().unwrap().snapped, None);
}

#[test]
fn compound_pointer_common_bounds_clamp_without_compressing_gaps() {
    let mut state = scene();
    state.snapping = false;
    let duration = state.editor.project().composition().duration();
    let (mut input, geometry) = pointer_press(&state, &[0, 30], 0, false);
    input.update_pointer(&state, geometry, -10000., true);
    assert_eq!(input.pointer.as_ref().unwrap().delta, 0);
    input.update_pointer(&state, geometry, 10000., true);
    assert_eq!(
        input.pointer.as_ref().unwrap().delta,
        i64::from(duration - 31)
    );
    input.finish_pointer(&mut state, geometry, 10000., true, false, pointer_apply);
    assert_eq!(
        input.selected_frames(),
        [duration - 31, duration - 1].into()
    );
    assert_eq!(
        animation(&state, 1)
            .keys()
            .keys()
            .copied()
            .collect::<BTreeSet<_>>(),
        input.selected_frames()
    );
}

#[test]
fn compound_pointer_frozen_geometry_and_context_cannot_reattach_after_aba() {
    for case in 0..14 {
        let mut state = scene();
        let (mut input, mut geometry) = pointer_press(&state, &[0, 30], 0, false);
        let original = state.editor.project().clone();
        match case {
            0 => geometry.bounds.origin.x += px(1.),
            1 => geometry.bounds.origin.y += px(1.),
            2 => geometry.bounds.size.width += px(1.),
            3 => geometry.bounds.size.height += px(1.),
            4 => state.timeline_start += 1,
            5 => state.timeline_zoom *= 2.,
            6 => state.snapping = !state.snapping,
            7 => {
                state.bulk_test_action(&Action::Seek(1));
                state.bulk_test_action(&Action::Seek(0));
            }
            8 => {
                edit(
                    &mut state,
                    1,
                    GradientColorsEdit::SetInterpolation {
                        frame: 0,
                        interpolation: GradientColorsInterpolation::Smooth,
                    },
                );
                state.bulk_test_action(&Action::Undo);
            }
            9 => {
                state.retire_colors_context();
            }
            10 => {
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
            }
            11 => state.graph_open = true,
            12 => state.expanded = false,
            _ => input.select_frame(&state, 2, 0),
        }
        let before = state.editor.project().clone();
        input.update_pointer(&state, geometry, 141., true);
        assert!(input.pointer.is_none(), "case {case}");
        // Restore geometry/context bytes. The retired receipt must stay retired.
        let restored = pointer_geometry(&state);
        input.finish_pointer(&mut state, restored, 141., true, false, |_, _| {
            panic!("retired pointer cannot apply")
        });
        assert_eq!(state.editor.project(), &before);
        if case != 13 {
            assert_eq!(state.editor.project(), &original);
        }
    }
}

#[test]
fn compound_pointer_pending_input_and_source_retirement_keep_domain_tombstone() {
    let mut state = scene();
    let (mut input, geometry) = pointer_press(&state, &[0, 30], 0, false);
    let target = input.target(1).unwrap();
    assert!(
        !input.prepare(&target, &state, None),
        "other controls cannot race active pointer"
    );
    state.retire_colors_context();
    input.observe(&state);
    assert!(input.pointer.is_none());
    assert!(input.selected.is_none());
    assert!(input.domain_owned);
    assert!(state.colors_key_owned.get());
    assert!(
        !input.finish_pointer(&mut state, geometry, 141., false, false, |_, _| panic!(
            "retired context"
        ))
    );
}

#[test]
fn compound_pointer_capture_and_ghosts_are_present_before_first_press() {
    let timeline = include_str!("../timeline.rs");
    let root = timeline.split(".id(\"timeline\")").nth(1).unwrap();
    assert!(root.contains("window.on_mouse_event(move |event: &MouseMoveEvent, phase"));
    assert!(root.contains("window.on_mouse_event(move |event: &MouseUpEvent, phase"));
    assert!(root.contains("this.colors.pointer_up("));
    assert!(root.contains(".on_modifiers_changed("));
    let pointer = include_str!("compound_colors_drag.rs");
    let key_button = pointer.split("fn key_button(").nth(1).unwrap();
    assert!(key_button.contains("if !matches!(event, gpui::ClickEvent::Keyboard(_))"));
    assert!(pointer.contains("input_pointer_generation(window, cx)"));
    assert!(pointer.contains("TextField::active_pending_binding(cx).is_none()"));
    let colors = include_str!("compound_colors.rs");
    assert!(colors.contains("p.previous_selection.clone()"));
    assert!(colors.contains("p.geometry.bounds != bounds"));
}

#[test]
fn compound_pointer_shift_empty_selection_owns_generic_delete_and_clipboard_domain() {
    for action in [
        Action::DeleteSelection,
        Action::CutSelection,
        Action::DuplicateSelection,
        Action::CopySelection,
    ] {
        let mut state = scene();
        let before = state.editor.project().clone();
        let mut input = Input::default();
        input.observe(&state);
        let target = input.target(1).unwrap();
        let geometry = pointer_geometry(&state);
        assert!(input.begin_pointer(&target, &state, 30, 101., true, geometry));
        assert!(input.domain_owned);
        assert!(state.colors_key_owned.get());
        assert!(input.selected.is_none());
        state.bulk_test_action(&action);
        assert_eq!(state.editor.project(), &before);
        input.cancel_pointer();
        state.bulk_test_action(&Action::DeleteSelection);
        assert_eq!(state.editor.project(), &before);
        assert!(state.colors_key_owned.get());
    }
}

#[test]
fn compound_pointer_modifier_aba_cancels_without_waiting_for_mouse_motion() {
    for case in 0..4 {
        let state = scene();
        let (input, _) = pointer_press(&state, &[0, 30], 0, case == 3);
        let mut colors = TimelineColors {
            input: Rc::new(RefCell::new(input)),
            ..Default::default()
        };
        let mut modifiers = gpui::Modifiers::default();
        match case {
            0 => modifiers.control = true,
            1 => modifiers.platform = true,
            2 => modifiers.shift = true,
            _ => {} // Shift was held at press, released now.
        }
        assert!(colors.pointer_modifiers_changed(modifiers));
        assert!(!colors.pointer_modifiers_changed(gpui::Modifiers::default()));
        assert!(colors.input.borrow().pointer.is_none());
        assert!(colors.input.borrow().domain_owned);
    }
    let state = scene();
    let (input, _) = pointer_press(&state, &[0, 30], 0, false);
    let mut colors = TimelineColors {
        input: Rc::new(RefCell::new(input)),
        ..Default::default()
    };
    let mut alt = gpui::Modifiers::default();
    alt.alt = true;
    assert!(!colors.pointer_modifiers_changed(alt));
    assert!(!colors.pointer_modifiers_changed(gpui::Modifiers::default()));
    assert!(colors.input.borrow().pointer.is_some());
}

#[test]
fn compound_pointer_equal_distance_snap_chooses_ascending_target_deterministically() {
    let mut state = scene();
    edit(&mut state, 1, GradientColorsEdit::ToggleKey { frame: 20 });
    state.bulk_test_action(&Action::Seek(10));
    state.snapping = true;
    let (mut input, mut geometry) = pointer_press(&state, &[0], 0, false);
    input.cancel_pointer();
    // One px per frame puts both equidistant targets in the eight-px window.
    geometry.bounds.size.width = px(state.visible_frames() as f32);
    let target = input.target(1).unwrap();
    assert!(input.begin_pointer(&target, &state, 0, 101., false, geometry));
    input.update_pointer(&state, geometry, 116., false);
    assert_eq!(input.pointer.as_ref().unwrap().delta, 10);
    assert_eq!(input.pointer.as_ref().unwrap().snapped, Some(10));
    assert!(!input.pointer.as_ref().unwrap().invalid);
}
