use super::*;
use libre_effects_core::ContentsKind;

fn edit(state: &mut EditorState, item: u64, edit: GradientColorsEdit) {
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::GradientColors { item, edit },
    }));
}
fn scene() -> EditorState {
    let mut state = EditorState::default();
    state
        .editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(Default::default()),
            width: 200.,
            height: 120.,
            name: "Marquee".into(),
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
        for frame in [10, 30, 50] {
            edit(&mut state, item, GradientColorsEdit::ToggleKey { frame });
        }
    }
    state.editor.clear_history();
    state
}
fn geometry(state: &EditorState) -> PointerGeometry {
    PointerGeometry {
        bounds: Bounds::new(
            point(px(80.), px(240.)),
            gpui::size(px(state.visible_frames() as f32 * 4.), px(29.)),
        ),
        start: state.timeline_start,
        visible: state.visible_frames(),
    }
}
fn position(frame: f32, y: f32) -> Point<Pixels> {
    point(px(80. + frame * 4.), px(240. + y))
}
fn bound(state: &EditorState, item: u64, frames: &[Frame]) -> Input {
    let mut input = Input::default();
    input.observe(state);
    input.select_frames(state, item, frames.iter().copied().collect());
    input
}
fn begin(
    input: &mut Input,
    state: &EditorState,
    item: u64,
    shift: bool,
    at: Point<Pixels>,
) -> PointerGeometry {
    let geometry = geometry(state);
    assert!(input.begin_marquee(&input.target(item).unwrap(), state, at, shift, geometry));
    geometry
}

fn view_snapshot(state: &EditorState) -> serde_json::Value {
    serde_json::json!({
        "frame": state.frame,
        "timeline_start": state.timeline_start,
        "timeline_zoom": state.timeline_zoom,
        "preview_zoom": state.preview_zoom,
        "preview_pan": state.preview_pan,
        "preview_resolution": state.preview_resolution,
        "checkerboard": state.checkerboard,
        "viewer": state.viewer,
        "graph_open": state.graph_open,
        "graph_view": state.graph_view,
        "expanded": state.expanded,
        "graph_channels": state.graph_channels,
        "workspace": state.workspace,
        "effect_controls_open": state.effect_controls_open,
        "snapping": state.snapping,
    })
}

#[test]
fn compound_marquee_reverse_and_forward_boxes_select_centers_only_on_pressed_paint() {
    for reverse in [false, true] {
        let state = scene();
        let mut input = bound(&state, 2, &[50]);
        let (a, b) = if reverse {
            (position(35., 28.), position(5., 1.))
        } else {
            (position(5., 1.), position(35., 28.))
        };
        let geometry = begin(&mut input, &state, 1, false, a);
        input.update_marquee(&state, geometry, b);
        assert_eq!(input.selected_frames(), [50].into(), "preview is local");
        assert_eq!(input.marquee_preview(1, &state), Some([10, 30].into()));
        assert_eq!(input.marquee_preview(2, &state), None);
        assert!(input.finish_marquee(&state, geometry, b));
        assert_eq!(input.selected_frames(), [10, 30].into());
        assert_eq!(input.selected.as_ref().unwrap().item, 1);
        assert!(!state.editor.can_undo());
    }
}

#[test]
fn compound_marquee_shift_unions_without_toggling_and_does_not_cross_paints() {
    for previous_item in [1, 2] {
        let state = scene();
        let mut input = bound(&state, previous_item, &[10, 50]);
        let geometry = begin(&mut input, &state, 1, true, position(5., 1.));
        input.finish_marquee(&state, geometry, position(15., 28.));
        let expected = if previous_item == 1 {
            [10, 50].into()
        } else {
            [10].into()
        };
        assert_eq!(input.selected_frames(), expected);
        assert_eq!(input.selected.as_ref().unwrap().item, 1);
    }
}

#[test]
fn compound_marquee_clips_outside_release_to_pressed_lane_and_visible_keys() {
    let mut state = scene();
    state.timeline_start = 5;
    let mut input = bound(&state, 1, &[]);
    let geometry = begin(&mut input, &state, 1, false, position(0., 1.));
    input.finish_marquee(&state, geometry, position(10000., 10000.));
    assert_eq!(input.selected_frames(), [10, 30, 50].into());
    assert_eq!(state.timeline_start, 5);
    assert_eq!(state.frame, 0);
}

#[test]
fn compound_marquee_empty_click_and_box_retain_generic_shortcut_ownership() {
    for action in [
        Action::DeleteSelection,
        Action::CutSelection,
        Action::CopySelection,
        Action::DuplicateSelection,
    ] {
        for end in [position(5., 1.), position(6., 28.), position(55., 10.)] {
            let mut state = scene();
            let source = state.editor.project().clone();
            let mut input = bound(&state, 1, &[10, 30]);
            let geometry = begin(&mut input, &state, 1, false, position(5., 1.));
            assert!(input.domain_owned && state.colors_key_owned.get());
            input.finish_marquee(&state, geometry, end);
            assert!(input.selected_frames().is_empty());
            assert!(input.domain_owned && state.colors_key_owned.get());
            for _ in 0..2 {
                state.bulk_test_action(&action);
            }
            assert_eq!(state.editor.project(), &source);
            assert!(!state.editor.can_undo());
        }
    }
}

#[test]
fn compound_marquee_selection_noop_cancel_and_duplicate_release_preserve_source_view_and_redo() {
    for case in 0..5 {
        let mut state = scene();
        edit(
            &mut state,
            1,
            GradientColorsEdit::SetInterpolation {
                frame: 10,
                interpolation: GradientColorsInterpolation::Smooth,
            },
        );
        let redone = state.editor.project().clone();
        state.bulk_test_action(&Action::Undo);
        let source = state.editor.project().clone();
        let view = view_snapshot(&state);
        let mut input = bound(&state, 1, &[10, 30]);
        let geometry = begin(&mut input, &state, 1, case == 1, position(5., 1.));
        input.update_marquee(&state, geometry, position(35., 28.));
        if case == 2 {
            input.cancel_pointer();
        }
        if case == 3 {
            input.update_marquee(&state, geometry, point(px(f32::NAN), px(250.)));
        }
        let end = if case == 4 {
            position(5., 1.)
        } else {
            position(35., 28.)
        };
        input.finish_marquee(&state, geometry, end);
        assert!(!input.finish_marquee(&state, geometry, position(60., 28.)));
        assert_eq!(state.editor.project(), &source);
        assert_eq!(view_snapshot(&state), view);
        assert!(state.editor.can_redo());
        state.bulk_test_action(&Action::Redo);
        assert_eq!(state.editor.project(), &redone);
    }
}

#[test]
fn compound_marquee_four_pixel_threshold_and_vertical_center_are_exact() {
    let state = scene();
    let mut input = bound(&state, 1, &[]);
    let geometry = begin(&mut input, &state, 1, false, position(9.5, 13.));
    input.update_marquee(&state, geometry, position(10.499, 14.5));
    assert!(!input.marquee.as_ref().unwrap().crossed);
    assert_eq!(input.marquee_preview(1, &state), Some(BTreeSet::new()));
    input.update_marquee(&state, geometry, position(10.5, 14.5));
    assert!(input.marquee.as_ref().unwrap().crossed);
    assert_eq!(input.marquee_preview(1, &state), Some([10].into()));
    input.finish_marquee(&state, geometry, position(10.5, 14.499));
    assert!(input.selected_frames().is_empty());
}

#[test]
fn compound_marquee_geometry_source_and_selection_aba_retire_receipts() {
    for case in 0..15 {
        let mut state = scene();
        let mut input = bound(&state, 1, &[50]);
        let mut geometry = begin(&mut input, &state, 1, false, position(5., 1.));
        match case {
            0 => geometry.bounds.origin.x += px(1.),
            1 => geometry.bounds.origin.y += px(1.),
            2 => geometry.bounds.size.width += px(1.),
            3 => geometry.bounds.size.height += px(1.),
            4 => state.timeline_start += 1,
            5 => state.timeline_zoom *= 2.,
            6 => {
                state.bulk_test_action(&Action::Seek(1));
                state.bulk_test_action(&Action::Seek(0));
            }
            7 => {
                edit(
                    &mut state,
                    1,
                    GradientColorsEdit::SetInterpolation {
                        frame: 10,
                        interpolation: GradientColorsInterpolation::Smooth,
                    },
                );
                state.bulk_test_action(&Action::Undo);
            }
            8 => state.retire_colors_context(),
            9 => {
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
                state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
            }
            10 => state.graph_open = true,
            11 => state.expanded = false,
            12 => input.select_frames(&state, 2, [50].into()),
            13 => state.queue_open = true,
            _ => state.playing = true,
        }
        let source = state.editor.project().clone();
        input.update_marquee(&state, geometry, position(35., 28.));
        assert!(!input.pointer_active(), "case {case}");
        assert!(!input.finish_marquee(&state, geometry, position(60., 28.)));
        assert_eq!(state.editor.project(), &source);
    }
}

#[test]
fn compound_marquee_modifier_aba_cancels_before_release_and_never_accepts_alt() {
    for case in 0..5 {
        let state = scene();
        let mut input = bound(&state, 1, &[50]);
        begin(&mut input, &state, 1, false, position(5., 1.));
        let mut colors = TimelineColors {
            input: Rc::new(RefCell::new(input)),
            ..Default::default()
        };
        let mut modifiers = gpui::Modifiers::default();
        match case {
            0 => modifiers.shift = true,
            1 => modifiers.control = true,
            2 => modifiers.platform = true,
            3 => modifiers.function = true,
            _ => modifiers.alt = true,
        }
        assert!(colors.pointer_modifiers_changed(modifiers));
        assert!(!colors.pointer_modifiers_changed(gpui::Modifiers::default()));
        assert!(!colors.input.borrow().pointer_active());
        assert_eq!(colors.input.borrow().selected_frames(), [50].into());
    }
}

#[test]
fn compound_marquee_pending_active_and_nonfinite_geometry_are_rejected() {
    let state = scene();
    let mut input = bound(&state, 1, &[50]);
    let target = input.target(1).unwrap();
    for binding in ["invalid", "unchanged", "foreign", "colors-scale"] {
        assert!(!input.prepare(&target, &state, Some(binding.into())));
    }
    for case in 0..8 {
        let mut geometry = geometry(&state);
        match case {
            0 => geometry.bounds.origin.x = px(f32::NAN),
            1 => geometry.bounds.origin.y = px(f32::INFINITY),
            2 => geometry.bounds.size.width = px(f32::INFINITY),
            3 => geometry.bounds.size.height = px(f32::NAN),
            4 => geometry.bounds.size.width = px(0.),
            5 => geometry.bounds.size.height = px(0.),
            6 => geometry.bounds.size.height = px(-1.),
            _ => geometry.visible = 0,
        }
        assert!(!input.begin_marquee(&target, &state, position(5., 1.), false, geometry));
    }
    assert!(!input.begin_marquee(&target, &state, position(-5., 1.), false, geometry(&state)));
    begin(&mut input, &state, 1, false, position(5., 1.));
    assert!(!input.prepare(&input.target(1).unwrap(), &state, None));
    assert!(input.cancel_pointer());
    assert!(!input.cancel_pointer());
}

#[test]
fn compound_marquee_observed_context_retirement_keeps_shortcut_tombstone() {
    let mut state = scene();
    let mut input = bound(&state, 1, &[]);
    begin(&mut input, &state, 1, false, position(5., 1.));
    state.retire_colors_context();
    input.observe(&state);
    assert!(!input.pointer_active());
    assert!(input.domain_owned && state.colors_key_owned.get());
}

#[test]
fn compound_marquee_ui_uses_guarded_capture_and_lane_clipped_overlay() {
    let marquee = include_str!("compound_colors_marquee.rs");
    assert!(marquee.contains("input_pointer_key_button_guarded"));
    assert!(marquee.contains("TextField::active_pending_binding(cx)"));
    assert!(marquee.contains("window.prevent_default()"));
    assert!(marquee.contains("this.marquee = None"));
    let capture = include_str!("compound_colors_drag.rs");
    assert!(capture.contains("input.finish_marquee(state, geometry.unwrap(), event.position)"));
    assert!(capture.contains("focus.is_focused(window)"));
    assert!(capture.contains("TextField::is_composing(window, cx)"));
    assert!(capture.contains("input_pointer_generation(window, cx)"));
    let rows = include_str!("compound_colors.rs");
    assert!(rows.contains("input.marquee.as_ref().is_some_and"));
    assert!(rows.contains("self.marquee_overlay(lane, node.id)"));
    assert!(rows.contains("event.keystroke.key != \"alt\" {\n            self.input.borrow_mut().cancel_pointer();\n            cx.notify();"));
}
