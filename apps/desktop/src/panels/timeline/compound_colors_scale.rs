//! Exact duration scaling for one paint's selected compound keys.
//! The first key stays fixed; the entered frame sets the final key's time.
use super::*;

fn scale_command(
    target: &Target,
    state: &EditorState,
    text: &str,
) -> Result<Option<(Command, BTreeSet<Frame>)>, String> {
    let selected = target
        .selection
        .as_ref()
        .filter(|s| s.owns(target))
        .ok_or("Select at least two Colors keys to scale")?;
    let animation = target
        .node(state)
        .and_then(|n| n.kind.gradient())
        .and_then(|g| g.colors_animation())
        .ok_or("Colors keys no longer exist")?;
    let to = text
        .trim()
        .parse::<Frame>()
        .map_err(|_| "Enter a whole frame number")?;
    let mapping = animation.scaled_key_frames(
        &selected.frames,
        to,
        state.editor.project().composition().duration(),
    )?;
    if mapping.iter().all(|(from, to)| from == to) {
        return Ok(None);
    }
    Ok(Some((
        Command::Contents {
            id: selected.layer,
            edit: ContentsEdit::GradientColors {
                item: selected.item,
                edit: GradientColorsEdit::ScaleKeys {
                    frames: selected.frames.clone(),
                    to,
                },
            },
        },
        mapping.into_values().collect(),
    )))
}

fn submit_scale(
    input: &Rc<RefCell<Input>>,
    target: &Target,
    state: &mut EditorState,
    text: &str,
    active: bool,
    apply: impl FnOnce(&mut EditorState, Command) -> bool,
) -> String {
    let end = target
        .selection
        .as_ref()
        .and_then(|s| s.frames.last().copied())
        .unwrap_or(target.owner.frame);
    if !active || !input.borrow().prepare(target, state, None) {
        state.status = "Colors key editing context changed; scale was not applied".into();
        return end.to_string();
    }
    match scale_command(target, state, text) {
        Ok(None) => {}
        Ok(Some((command, frames))) => {
            if apply(state, command) {
                let end = *frames.last().unwrap();
                let mut input = input.borrow_mut();
                input.observe(state);
                input.select_frames(state, target.item, frames);
                return end.to_string();
            }
        }
        Err(error) => state.status = error,
    }
    end.to_string()
}

pub(super) fn scale_control(
    scale_field: &mut Option<Entity<TextField>>,
    input: &Rc<RefCell<Input>>,
    target: &Target,
    selected: &Selection,
    state: &Entity<EditorState>,
    return_focus: gpui::FocusHandle,
    window: &mut Window,
    cx: &mut Context<super::super::Timeline>,
) -> Option<gpui::Div> {
    if selected.frames.len() < 2 {
        return None;
    }
    let input = input.clone();
    let edit_state = state.clone();
    let target = target.clone();
    let field = scale_field
        .get_or_insert_with(|| {
            cx.new(|cx| TextField::new(cx, |_, _, _| {}).return_focus(return_focus))
        })
        .clone();
    field.update(cx, |field, _| {
        field.sync_guarded(
            format!("{}-scale", target.key()),
            selected.frames.last().unwrap().to_string(),
            window,
            move |text, window, cx| {
                edit_state.update(cx, |state, cx| {
                    let display = submit_scale(
                        &input,
                        &target,
                        state,
                        text,
                        window.is_window_active(),
                        |state, command| {
                            state.dispatch(&Action::Edit(command), window, cx);
                            state.status == "Edited"
                        },
                    );
                    cx.notify();
                    display
                })
            },
        );
    });
    Some(
        div()
            .flex()
            .items_center()
            .gap_1()
            .child("Scale end → frame")
            .child(div().w(px(80.)).child(field)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> (EditorState, Rc<RefCell<Input>>, Target) {
        let mut state = EditorState::default();
        state
            .editor
            .execute(Command::AddContent {
                content: Content::ShapeContents(Default::default()),
                width: 200.,
                height: 120.,
                name: "Scale colors".into(),
            })
            .unwrap();
        state
            .editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 0,
                    kind: libre_effects_core::ContentsKind::GradientFill {
                        even_odd: false,
                        gradient: Default::default(),
                    },
                },
            })
            .unwrap();
        state.bulk_test_action(&Action::Select(1));
        state.expanded = true;
        state.bulk_test_action(&Action::Edit(Command::Contents {
            id: 1,
            edit: ContentsEdit::GradientColors {
                item: 1,
                edit: GradientColorsEdit::SetAnimation {
                    frame: 10,
                    enabled: true,
                },
            },
        }));
        for frame in [20, 30, 60] {
            state.bulk_test_action(&Action::Edit(Command::Contents {
                id: 1,
                edit: ContentsEdit::GradientColors {
                    item: 1,
                    edit: GradientColorsEdit::ToggleKey { frame },
                },
            }));
        }
        state.editor.clear_history();
        let input = Rc::new(RefCell::new(Input::default()));
        input.borrow_mut().observe(&state);
        input
            .borrow_mut()
            .select_frames(&state, 1, [10, 20, 30].into());
        let target = input.borrow().target(1).unwrap();
        (state, input, target)
    }

    fn apply(state: &mut EditorState, command: Command) -> bool {
        state.bulk_test_action(&Action::Edit(command));
        state.status == "Edited"
    }

    fn view(state: &EditorState) -> crate::view_state::CompositionView {
        crate::view_state::CompositionView {
            frame: state.frame,
            timeline_start: state.timeline_start,
            timeline_zoom: state.timeline_zoom,
            preview_zoom: state.preview_zoom,
            preview_pan: state.preview_pan,
            preview_resolution: state.preview_resolution,
            checkerboard: state.checkerboard,
            viewer: state.viewer.clone(),
            graph_open: state.graph_open,
            graph_view: state.graph_view.clone(),
            expanded: state.expanded,
            layer_tree: state.layer_tree.clone(),
            graph_channels: state.graph_channels.clone(),
        }
    }

    #[test]
    fn compound_scale_ui_remaps_selection_without_seeking_and_undo_is_atomic() {
        let (mut state, input, target) = scene();
        let before = state.editor.project().clone();
        let before_view = view(&state);
        assert_eq!(
            submit_scale(&input, &target, &mut state, "50", true, apply),
            "50"
        );
        assert_eq!(
            input.borrow().selected.as_ref().unwrap().frames,
            [10, 30, 50].into()
        );
        assert_eq!(view(&state), before_view);
        assert!(!input.borrow().current(&target, &state));
        state.bulk_test_action(&Action::Undo);
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
        state.bulk_test_action(&Action::Redo);
        let animation = target
            .node(&state)
            .unwrap()
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap();
        assert_eq!(
            animation.keys().keys().copied().collect::<Vec<_>>(),
            [10, 30, 50, 60]
        );
    }

    #[test]
    fn compound_scale_ui_invalid_input_noop_and_collision_preserve_redo() {
        let (mut state, input, target) = scene();
        state.bulk_test_action(&Action::Edit(Command::Contents {
            id: 1,
            edit: ContentsEdit::GradientColors {
                item: 1,
                edit: GradientColorsEdit::SetInterpolation {
                    frame: 10,
                    interpolation: GradientColorsInterpolation::Linear,
                },
            },
        }));
        state.bulk_test_action(&Action::Undo);
        input.borrow_mut().observe(&state);
        input
            .borrow_mut()
            .select_frames(&state, 1, [10, 20, 30].into());
        let target = input.borrow().target(target.item).unwrap();
        let before = state.editor.project().clone();
        for text in [
            "-1",
            "1.5",
            "NaN",
            "4294967296",
            "10",
            "11",
            "60",
            "99999",
            " 030 ",
        ] {
            assert_eq!(
                submit_scale(&input, &target, &mut state, text, true, apply),
                "30",
                "{text}"
            );
            assert_eq!(state.editor.project(), &before, "{text}");
            assert!(state.editor.can_redo(), "{text}");
        }
    }

    #[test]
    fn compound_scale_ui_rejects_stale_inactive_pending_and_single_key() {
        let (mut state, input, target) = scene();
        let before = state.editor.project().clone();
        assert_eq!(
            submit_scale(&input, &target, &mut state, "50", false, apply),
            "30"
        );
        assert!(
            !input
                .borrow()
                .prepare(&target, &state, Some("foreign-field".into()))
        );
        input.borrow_mut().select_frames(&state, 1, [10].into());
        assert_eq!(
            submit_scale(&input, &target, &mut state, "50", true, apply),
            "30"
        );
        let single = input.borrow().target(1).unwrap();
        assert!(scale_command(&single, &state, "50").is_err());
        assert_eq!(state.editor.project(), &before);
    }

    #[test]
    fn compound_scale_ui_stale_source_and_selection_round_trips_never_dispatch() {
        for case in 0..8 {
            let (mut state, input, target) = scene();
            match case {
                0 => {
                    state.bulk_test_action(&Action::Seek(1));
                    state.bulk_test_action(&Action::Seek(0));
                }
                1 => {
                    state.bulk_test_action(&Action::Edit(Command::Contents {
                        id: 1,
                        edit: ContentsEdit::GradientColors {
                            item: 1,
                            edit: GradientColorsEdit::SetInterpolation {
                                frame: 10,
                                interpolation: GradientColorsInterpolation::Smooth,
                            },
                        },
                    }));
                    state.bulk_test_action(&Action::Undo);
                }
                2 => {
                    input.borrow_mut().select_frames(&state, 1, [10, 20].into());
                    input
                        .borrow_mut()
                        .select_frames(&state, 1, [10, 20, 30].into());
                }
                3 => {
                    state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
                    state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
                }
                4 => state.retire_colors_context(),
                5 => state.graph_open = true,
                6 => state.expanded = false,
                _ => state.playing = true,
            }
            let source = state.editor.project().clone();
            let before_view = view(&state);
            let history = (state.editor.can_undo(), state.editor.can_redo());
            for observe in [false, true] {
                if observe {
                    input.borrow_mut().observe(&state);
                }
                assert_eq!(
                    submit_scale(&input, &target, &mut state, "50", true, |_, _| {
                        panic!("stale scale callback dispatched, case {case}")
                    }),
                    "30"
                );
            }
            assert_eq!(state.editor.project(), &source, "case {case}");
            assert_eq!(view(&state), before_view, "case {case}");
            assert_eq!((state.editor.can_undo(), state.editor.can_redo()), history);
        }
    }

    #[test]
    fn compound_scale_ui_pointer_ownership_blocks_field_commit() {
        for marquee in [false, true] {
            let (mut state, input, target) = scene();
            let geometry = PointerGeometry {
                bounds: gpui::Bounds::new(
                    gpui::point(px(80.), px(240.)),
                    gpui::size(px(state.visible_frames() as f32 * 4.), px(29.)),
                ),
                start: state.timeline_start,
                visible: state.visible_frames(),
            };
            if marquee {
                assert!(input.borrow_mut().begin_marquee(
                    &target,
                    &state,
                    gpui::point(px(90.), px(250.)),
                    false,
                    geometry,
                ));
            } else {
                assert!(
                    input
                        .borrow_mut()
                        .begin_pointer(&target, &state, 10, 101., false, geometry,)
                );
            }
            let current = input.borrow().target(1).unwrap();
            assert!(input.borrow().current(&current, &state));
            let source = state.editor.project().clone();
            assert_eq!(
                submit_scale(&input, &current, &mut state, "50", true, |_, _| {
                    panic!("scale must not commit during a pointer gesture")
                }),
                "30"
            );
            assert_eq!(state.editor.project(), &source);
            assert!(!state.editor.can_undo());
            assert_eq!(input.borrow().selected_frames(), [10, 20, 30].into());
        }
    }

    #[test]
    fn compound_scale_ui_rejects_mismatched_and_missing_selected_key_ids() {
        let (mut state, input, target) = scene();
        let source = state.editor.project().clone();
        for case in 0..3 {
            let mut foreign = target.clone();
            let selection = foreign.selection.as_mut().unwrap();
            match case {
                0 => selection.layer += 1,
                1 => selection.item += 1,
                _ => {
                    selection.frames.insert(25);
                }
            }
            assert!(scale_command(&foreign, &state, "50").is_err());
        }
        input
            .borrow_mut()
            .select_frames(&state, 1, [10, 20, 25].into());
        let missing = input.borrow().target(1).unwrap();
        assert!(input.borrow().current(&missing, &state));
        assert_eq!(
            submit_scale(&input, &missing, &mut state, "50", true, |_, _| {
                panic!("missing selected key cannot be scaled")
            }),
            "25"
        );
        assert_eq!(state.editor.project(), &source);
        assert!(!state.editor.can_undo());
    }

    #[test]
    fn compound_scale_ui_rejected_apply_preserves_selection_and_duplicate_callback_is_stale() {
        let (mut state, input, target) = scene();
        let source = state.editor.project().clone();
        assert_eq!(
            submit_scale(&input, &target, &mut state, "50", true, |_, _| false),
            "30"
        );
        assert_eq!(state.editor.project(), &source);
        assert!(input.borrow().current(&target, &state));
        assert_eq!(input.borrow().selected_frames(), [10, 20, 30].into());
        assert_eq!(
            submit_scale(&input, &target, &mut state, "50", true, apply),
            "50"
        );
        let scaled = state.editor.project().clone();
        assert_eq!(
            submit_scale(&input, &target, &mut state, "70", true, |_, _| {
                panic!("consumed callback cannot replay")
            }),
            "30"
        );
        assert_eq!(state.editor.project(), &scaled);
        assert_eq!(input.borrow().selected_frames(), [10, 30, 50].into());
        state.bulk_test_action(&Action::Undo);
        assert_eq!(state.editor.project(), &source);
        assert!(!state.editor.can_undo());
    }
}
