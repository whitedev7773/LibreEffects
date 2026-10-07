//! Independent event-model regressions. These use EditorState's dispatch-equivalent
//! action/normalization harness; they do not claim native pointer or IME delivery.
use super::*;
use crate::editor::Action;
use libre_effects_core::{Property, ShapeContents};

fn scene() -> EditorState {
    let mut state = EditorState::default();
    state.tool = Tool::Pen;
    state.composition_started = true;
    state
        .editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents::default()),
            width: 400.,
            height: 300.,
            name: "Independent cross-path guards".into(),
        })
        .unwrap();
    for _ in 0..2 {
        state
            .editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 0,
                    kind: ContentsKind::Group(vec![]),
                },
            })
            .unwrap();
    }
    for (parent, x) in [(1, 40.), (2, 220.)] {
        state
            .editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent,
                    kind: ContentsKind::Path {
                        path: VectorPath {
                            vertices: [[x, 40.], [x + 80., 40.], [x + 80., 160.], [x, 160.]]
                                .map(PathVertex::corner)
                                .into(),
                            closed: true,
                        },
                        animation: Default::default(),
                    },
                },
            })
            .unwrap();
    }
    state.selected_layers.insert(1);
    state.editor.clear_history();
    state
}

fn view(state: &EditorState) -> View {
    View::new(
        Bounds::new(point(px(0.), px(0.)), size(px(2000.), px(1400.))),
        point(px(0.), px(0.)),
        1.,
        state,
    )
}

fn pointer(position: [f64; 2]) -> Point<Pixels> {
    point(px(position[0] as f32), px(position[1] as f32))
}

fn anchor(state: &EditorState, item: u64, index: usize) -> [f64; 2] {
    let (_, path, world) = paths(state)
        .into_iter()
        .find(|(target, _, _)| *target == Target::Contents(1, item))
        .unwrap();
    world.point(path.vertices[index].position)
}

fn selected_count(pen: &Pen, state: &EditorState) -> usize {
    pen.overlay(state)
        .iter()
        .map(|(_, _, _, indices)| indices.len())
        .sum()
}

fn select_pair(pen: &mut Pen, state: &EditorState) {
    for (item, shift) in [(3, false), (4, true)] {
        let modifiers = Modifiers {
            shift,
            ..Default::default()
        };
        let p = pointer(anchor(state, item, 0));
        assert!(
            pen.pointer_down(state, p, Some(view(state)), modifiers)
                .is_none()
        );
        assert!(
            pen.pointer_up(state, p, Some(view(state)), modifiers)
                .is_none()
        );
    }
    assert_eq!(selected_count(pen, state), 2);
}

fn held_move(pen: &mut Pen, state: &EditorState) -> ([f64; 2], [f64; 2]) {
    held_gesture(pen, state, 0)
}

fn held_gesture(pen: &mut Pen, state: &EditorState, kind: usize) -> ([f64; 2], [f64; 2]) {
    select_pair(pen, state);
    let (start, finish) = if kind == 0 {
        let start = anchor(state, 3, 0);
        (start, [start[0] + 25., start[1] - 12.])
    } else {
        pen.toggle_transform(state);
        let overlay = pen.transform_overlay(state, 1.).unwrap();
        if kind == 1 {
            let start = overlay.corners[2];
            (start, [start[0] + 35., start[1] + 18.])
        } else {
            let start = overlay.rotate;
            let delta = sub(start, overlay.pivot);
            (
                start,
                [overlay.pivot[0] - delta[1], overlay.pivot[1] + delta[0]],
            )
        }
    };
    assert!(
        pen.pointer_down(state, pointer(start), Some(view(state)), Default::default())
            .is_none()
    );
    pen.pointer_move(
        state,
        pointer(finish),
        Some(view(state)),
        Default::default(),
    );
    assert!(pen.pending(state).is_some());
    (start, finish)
}

#[test]
fn cross_path_guard_equal_return_editor_actions_cannot_revive_a_held_transform() {
    for case in 0..18 {
        let interruption = case % 6;
        let mut state = scene();
        let mut pen = Pen::default();
        let (start, finish) = held_gesture(&mut pen, &state, case / 6);
        let source = state.editor.project().clone();
        let before_transport = state.transport_generation();
        let before_input = state.input_context_generation();
        match interruption {
            0 => state.bulk_test_action(&Action::Seek(state.frame)),
            1 => {
                state.bulk_test_action(&Action::Seek(7));
                state.bulk_test_action(&Action::Seek(0));
            }
            2 => {
                state.bulk_test_action(&Action::Play);
                state.bulk_test_action(&Action::Play);
            }
            3 => {
                state.bulk_test_action(&Action::SetTool(Tool::Select));
                state.bulk_test_action(&Action::SetTool(Tool::Pen));
            }
            4 => state.bulk_test_action(&Action::Select(1)),
            _ => {
                state.bulk_test_action(&Action::Edit(Command::SetValue {
                    id: 1,
                    property: Property::Rotation,
                    frame: 0,
                    value: 19.,
                }));
                state.bulk_test_action(&Action::Undo);
            }
        }
        assert_eq!(state.editor.project(), &source);
        assert!(state.input_context_generation() > before_input);
        if matches!(interruption, 0 | 1 | 2 | 5) {
            assert!(state.transport_generation() > before_transport);
        }
        assert!(pen.pending(&state).is_none(), "interruption {interruption}");
        assert!(
            pen.pointer_up(
                &state,
                pointer(finish),
                Some(view(&state)),
                Default::default()
            )
            .is_none()
        );
        pen.pointer_move(
            &state,
            pointer(start),
            Some(view(&state)),
            Default::default(),
        );
        assert!(
            pen.pointer_up(
                &state,
                pointer(finish),
                Some(view(&state)),
                Default::default()
            )
            .is_none()
        );
        assert_eq!(selected_count(&pen, &state), 0);
        assert_eq!(state.editor.project(), &source);
    }
}

#[test]
fn cross_path_guard_view_cancellation_is_terminal_and_does_not_commit_or_clear_idle_selection() {
    for case in 0..21 {
        let field = case % 7;
        let state = scene();
        let mut pen = Pen::default();
        let (start, finish) = held_gesture(&mut pen, &state, case / 7);
        let original_view = view(&state);
        let mut changed = original_view;
        match field {
            0 => changed.zoom = 1.25,
            1 => changed.zoom_setting = Some(1.25),
            2 => changed.pan[0] = 3.,
            3 => changed.rulers = !changed.rulers,
            4 => changed.origin.y += px(1.),
            5 => changed.bounds.size.width -= px(1.),
            _ => changed.bounds.origin.x += px(1.),
        }
        assert!(!pen.validate_view(Some(changed)));
        assert!(pen.pending(&state).is_none());
        assert_eq!(selected_count(&pen, &state), 2);
        pen.pointer_move(
            &state,
            pointer(start),
            Some(original_view),
            Default::default(),
        );
        assert!(
            pen.pointer_up(
                &state,
                pointer(finish),
                Some(original_view),
                Default::default()
            )
            .is_none()
        );
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn cross_path_guard_removing_primary_selection_keeps_remaining_path_numerically_editable() {
    let state = scene();
    let mut pen = Pen::default();
    select_pair(&mut pen, &state);
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let first = pointer(anchor(&state, 3, 0));
    assert!(
        pen.pointer_down(&state, first, Some(view(&state)), shift)
            .is_none()
    );
    assert!(
        pen.pointer_up(&state, first, Some(view(&state)), shift)
            .is_none()
    );
    assert_eq!(selected_count(&pen, &state), 1);
    let request = pen
        .numeric_vertex_request(&state)
        .expect("remaining singleton path stays editable");
    assert_eq!(request.target, PathTarget::Contents(4));
    assert_eq!(request.indices, [0].into());
    let (handled, command) = pen.key("delete", &state);
    assert!(handled);
    assert!(matches!(
        command,
        Some(Command::EditPath {
            id: 1,
            target: PathTarget::Contents(4),
            ..
        })
    ));
}

#[test]
fn cross_path_guard_successive_release_only_edits_survive_real_editor_normalization() {
    let mut state = scene();
    let mut pen = Pen::default();
    select_pair(&mut pen, &state);
    let original = state.editor.project().clone();
    let original_anchors = [anchor(&state, 3, 0), anchor(&state, 4, 0)];
    for iteration in 1..=2 {
        let start = anchor(&state, 3, 0);
        let finish = [start[0] + 24., start[1] - 16.];
        assert!(
            pen.pointer_down(
                &state,
                pointer(start),
                Some(view(&state)),
                Default::default()
            )
            .is_none()
        );
        // Deliberately skip pointer_move: release must evaluate the final pointer.
        let command = pen
            .pointer_up(
                &state,
                pointer(finish),
                Some(view(&state)),
                Default::default(),
            )
            .unwrap();
        assert!(matches!(command, Command::TransformContentsPoints { .. }));
        let before_input = state.input_context_generation();
        let before_transport = state.transport_generation();
        state.bulk_test_action(&Action::Edit(command));
        assert_eq!(state.status, "Edited");
        assert!(state.input_context_generation() > before_input);
        assert!(state.transport_generation() > before_transport);
        pen.did_commit(&state);
        pen.reset_if_stale(&state);
        assert_eq!(selected_count(&pen, &state), 2, "edit {iteration}");
        for (item, start) in [3, 4].into_iter().zip(original_anchors) {
            assert_eq!(
                anchor(&state, item, 0),
                [
                    start[0] + 24. * iteration as f64,
                    start[1] - 16. * iteration as f64
                ]
            );
        }
    }
    state.bulk_test_action(&Action::Undo);
    assert!(state.editor.can_undo());
    state.bulk_test_action(&Action::Undo);
    assert!(!state.editor.can_undo());
    assert_eq!(state.editor.project(), &original);
    pen.reset_if_stale(&state);
    assert_eq!(selected_count(&pen, &state), 0);
}

#[test]
fn cross_path_guard_invalid_final_pointer_cannot_publish_last_valid_draft() {
    for final_position in [[2_000_000., 2_000_000.], [f64::NAN, 20.]] {
        let state = scene();
        let mut pen = Pen::default();
        let (_, finish) = held_move(&mut pen, &state);
        let before = state.editor.project().clone();
        assert!(
            pen.pointer_up(
                &state,
                pointer(final_position),
                Some(view(&state)),
                Default::default()
            )
            .is_none()
        );
        assert!(pen.pending(&state).is_none());
        assert!(
            pen.pointer_up(
                &state,
                pointer(finish),
                Some(view(&state)),
                Default::default()
            )
            .is_none()
        );
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn cross_path_guard_unexpected_modifiers_cancel_before_late_release() {
    for at_release in [false, true] {
        for excluded in 0..4 {
            let state = scene();
            let mut pen = Pen::default();
            let (_, finish) = held_move(&mut pen, &state);
            let mut modifiers = Modifiers::default();
            match excluded {
                0 => modifiers.alt = true,
                1 => modifiers.control = true,
                2 => modifiers.platform = true,
                _ => modifiers.function = true,
            }
            if at_release {
                assert!(
                    pen.pointer_up(&state, pointer(finish), Some(view(&state)), modifiers)
                        .is_none()
                );
            } else {
                pen.pointer_move(&state, pointer(finish), Some(view(&state)), modifiers);
            }
            assert!(pen.pending(&state).is_none());
            assert!(
                pen.pointer_up(
                    &state,
                    pointer(finish),
                    Some(view(&state)),
                    Default::default()
                )
                .is_none()
            );
            assert_eq!(selected_count(&pen, &state), 2);
            assert!(!state.editor.can_undo());
        }
    }
}

#[test]
fn cross_path_guard_pointer_policy_and_interrupted_callback_retire_held_draft() {
    let combinations = [
        (false, true, false, false), // A non-left press or move reporting another button.
        (true, false, false, false), // Window deactivation before callback.
        (true, true, true, false),   // Marked composition begins before release.
        (true, true, false, true),   // Another field owns pending input.
    ];
    for (left, active, composing, pending) in combinations {
        let state = scene();
        let mut pen = Pen::default();
        let (_, finish) = held_move(&mut pen, &state);
        assert!(!pointer_input_allowed(
            &state, left, active, composing, pending
        ));
        // This is the same retirement used by Preview's guarded callbacks.
        pen.abandon_pointer();
        assert!(pointer_input_allowed(&state, true, true, false, false));
        assert!(pen.pending(&state).is_none());
        assert!(
            pen.pointer_up(
                &state,
                pointer(finish),
                Some(view(&state)),
                Default::default()
            )
            .is_none()
        );
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn cross_path_guard_return_to_start_is_source_and_redo_preserving_for_every_handle() {
    for kind in 0..3 {
        let mut state = scene();
        state.bulk_test_action(&Action::Edit(Command::SetValue {
            id: 1,
            property: Property::Rotation,
            frame: 0,
            value: 19.,
        }));
        let redo = state.editor.project().clone();
        state.bulk_test_action(&Action::Undo);
        let original = state.editor.project().clone();
        let input_epoch = state.input_context_generation();
        let transport_epoch = state.transport_generation();
        let mut pen = Pen::default();
        let (start, _) = held_gesture(&mut pen, &state, kind);
        assert!(
            pen.pointer_up(
                &state,
                pointer(start),
                Some(view(&state)),
                Default::default()
            )
            .is_none()
        );
        assert_eq!(state.editor.project(), &original);
        assert!(!state.editor.can_undo());
        assert!(state.editor.can_redo());
        assert_eq!(state.input_context_generation(), input_epoch);
        assert_eq!(state.transport_generation(), transport_epoch);
        assert_eq!(selected_count(&pen, &state), 2);
        state.bulk_test_action(&Action::Redo);
        assert_eq!(state.editor.project(), &redo);
    }
}

#[test]
fn cross_path_guard_commit_acknowledgment_is_one_use_and_rejects_equal_return_interruption() {
    for duplicate_acknowledgment in [false, true] {
        let mut state = scene();
        let mut pen = Pen::default();
        let (_, finish) = held_move(&mut pen, &state);
        let command = pen
            .pointer_up(
                &state,
                pointer(finish),
                Some(view(&state)),
                Default::default(),
            )
            .unwrap();
        state.bulk_test_action(&Action::Edit(command));
        let committed = state.editor.project().clone();
        if duplicate_acknowledgment {
            pen.did_commit(&state);
            assert_eq!(selected_count(&pen, &state), 2);
            state.bulk_test_action(&Action::Seek(state.frame));
        } else {
            state.bulk_test_action(&Action::Undo);
            state.bulk_test_action(&Action::Redo);
        }
        assert_eq!(state.editor.project(), &committed);
        pen.did_commit(&state);
        pen.reset_if_stale(&state);
        assert_eq!(selected_count(&pen, &state), 0);
        assert!(pen.pending(&state).is_none());
        assert!(
            pen.pointer_up(
                &state,
                pointer(finish),
                Some(view(&state)),
                Default::default()
            )
            .is_none()
        );
        assert_eq!(state.editor.project(), &committed);
    }
}
