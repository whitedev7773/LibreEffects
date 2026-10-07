//! Whole-gradient Colors controls with exact outgoing-key interpolation. Each action belongs to the rendered
//! singleton, source snapshot, frame and input generation, including keyboard use.
use super::*;
use crate::panels::timeline::compound_colors::segment_label;
use libre_effects_core::{ContentsNode, Frame, GradientColorsEdit, GradientColorsInterpolation};

const HELP: &str = "Colors keys store complete color and opacity stop rows. Linear and Smoothstep require identical ordered stop IDs; topology changes explicitly hold. Disable clears keys and keeps the current sample. Endpoints animate separately. Finish pending fields with Enter before using buttons.";
const LEGACY_HELP: &str = "Colors animation requires static stop channels. Existing scalar stop animation is preserved; disable those channels before enabling Colors.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Enable,
    Disable,
    Add,
    Remove,
    Mode(GradientColorsInterpolation),
    Previous,
    Next,
    Edit,
}

#[derive(Clone)]
struct Target {
    input: crate::color_edit::InputTarget,
    generation: u64,
    transport: u64,
    serial: u64,
    frame: Frame,
    layer: u64,
    item: u64,
    composition: libre_effects_core::CompositionId,
    tool: crate::editor::Tool,
    gradient_controls: Option<crate::color_edit::GradientTarget>,
    selected_layers: std::collections::BTreeSet<u64>,
}
impl Target {
    fn capture(state: &EditorState, serial: u64, item: u64) -> Option<Self> {
        if tree::blocked(state) || state.selected_layers.len() != 1 {
            return None;
        }
        let layer = state.editor.selected_layer().filter(|l| !l.locked())?;
        let composition = state.editor.project().active_composition_id();
        if state.contents_selection != Some((composition, layer.id(), item)) {
            return None;
        }
        let Content::ShapeContents(contents) = layer.content() else {
            return None;
        };
        contents.node(item)?.kind.gradient()?;
        Some(Self {
            input: crate::color_edit::InputTarget::new(state)?,
            generation: state.input_context_generation(),
            transport: state.transport_generation(),
            serial,
            frame: state.frame,
            layer: layer.id(),
            item,
            composition,
            tool: state.tool,
            gradient_controls: state.gradient_controls,
            selected_layers: state.selected_layers.clone(),
        })
    }
    fn current(&self, state: &EditorState, serial: u64) -> bool {
        self.serial == serial
            && !tree::blocked(state)
            && self.input.current(state)
            && self.generation == state.input_context_generation()
            && self.transport == state.transport_generation()
            && self.frame == state.frame
            && self.tool == state.tool
            && self.gradient_controls == state.gradient_controls
            && self.selected_layers == state.selected_layers
            && state.contents_selection == Some((self.composition, self.layer, self.item))
    }
    fn prepare(&self, state: &EditorState, serial: u64, pending: Option<String>) -> bool {
        pending.is_none() && self.current(state, serial)
    }
    fn action(&self, state: &EditorState, control: Control) -> Option<Action> {
        let Content::ShapeContents(contents) = state.editor.selected_layer()?.content() else {
            return None;
        };
        let node = contents.node(self.item)?;
        let gradient = node.kind.gradient()?;
        let animation = gradient.colors_animation();
        Some(match control {
            Control::Enable | Control::Disable => {
                let enabled = control == Control::Enable;
                if enabled != animation.is_none() || enabled && legacy_animated(node) {
                    return None;
                }
                Action::Edit(Command::Contents {
                    id: self.layer,
                    edit: ContentsEdit::GradientColors {
                        item: self.item,
                        edit: GradientColorsEdit::SetAnimation {
                            frame: self.frame,
                            enabled,
                        },
                    },
                })
            }
            Control::Add | Control::Remove => {
                let exists = animation?.keys().contains_key(&self.frame);
                if (control == Control::Add) == exists {
                    return None;
                }
                Action::Edit(Command::Contents {
                    id: self.layer,
                    edit: ContentsEdit::GradientColors {
                        item: self.item,
                        edit: if control == Control::Remove {
                            GradientColorsEdit::DeleteKey { frame: self.frame }
                        } else {
                            GradientColorsEdit::ToggleKey { frame: self.frame }
                        },
                    },
                })
            }
            Control::Mode(interpolation) => {
                animation?.interpolation(self.frame)?;
                Action::Edit(Command::Contents {
                    id: self.layer,
                    edit: ContentsEdit::GradientColors {
                        item: self.item,
                        edit: GradientColorsEdit::SetInterpolation {
                            frame: self.frame,
                            interpolation,
                        },
                    },
                })
            }
            Control::Previous => {
                Action::Seek(*animation?.keys().range(..self.frame).next_back()?.0)
            }
            Control::Next => Action::Seek(
                *animation?
                    .keys()
                    .range((
                        std::ops::Bound::Excluded(self.frame),
                        std::ops::Bound::Unbounded,
                    ))
                    .next()?
                    .0,
            ),
            Control::Edit => Action::OpenGradient(self.item),
        })
    }
}

fn legacy_animated(node: &ContentsNode) -> bool {
    node.parameters.iter().any(|(p, t)| {
        matches!(p, ContentsParam::Gradient(p) if p.stop().is_some()) && !t.keys().is_empty()
    })
}

impl ContentsControls {
    pub(super) fn compound_color_controls(
        &mut self,
        node: &ContentsNode,
        frame: Frame,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let gradient = node.kind.gradient().expect("gradient controls");
        let animation = gradient.colors_animation();
        let target = Target::capture(self.state.read(cx), self.colors_serial.get(), node.id);
        let mut root = div().flex().flex_col().gap_1().mt_2();
        let count = animation.map_or(0, |a| a.keys().len());
        root = root.child(div().text_size(px(11.)).child(if count == 0 {
            if legacy_animated(node) {
                "Colors · Scalar stop animation"
            } else {
                "Colors · Static"
            }
            .to_string()
        } else {
            format!(
                "Colors · {count} keys · frame {frame} · {}",
                segment_label(animation.unwrap(), frame)
            )
        }));
        let disabled_input = target
            .as_ref()
            .map(|t| t.input.clone())
            .or_else(|| crate::color_edit::InputTarget::new(self.state.read(cx)));
        let mut row = div().flex().flex_wrap().gap_1();
        for (control, label) in [
            (
                if count == 0 {
                    Control::Enable
                } else {
                    Control::Disable
                },
                if count == 0 {
                    "Animate Colors (Hold)"
                } else {
                    "Disable Colors (clear keys)"
                },
            ),
            (
                if animation.is_some_and(|a| a.keys().contains_key(&frame)) {
                    Control::Remove
                } else {
                    Control::Add
                },
                if animation.is_some_and(|a| a.keys().contains_key(&frame)) {
                    "Remove key"
                } else {
                    "Add key"
                },
            ),
            (Control::Previous, "Previous"),
            (Control::Next, "Next"),
            (Control::Edit, "Edit Colors…"),
            (Control::Mode(GradientColorsInterpolation::Hold), "Hold"),
            (Control::Mode(GradientColorsInterpolation::Linear), "Linear"),
            (
                Control::Mode(GradientColorsInterpolation::Smooth),
                "Smoothstep",
            ),
        ] {
            if count == 0 && control != Control::Enable {
                continue;
            }
            let action = target
                .as_ref()
                .and_then(|target| target.action(self.state.read(cx), control));
            let name = format!("contents-colors-{}-{:?}", self.colors_serial.get(), control);
            let button = ui::text_button(gpui::SharedString::from(name.clone()), label)
                .when(matches!(control, Control::Mode(mode) if animation.and_then(|a| a.interpolation(frame)) == Some(mode)), |d| d.text_color(rgb(ui::BLUE)));
            let Some(target) = target.clone().filter(|_| action.is_some()) else {
                row = row.child(crate::color_edit::input_pointer_button_guarded(
                    button.opacity(0.4),
                    name,
                    disabled_input.clone(),
                    |_, _, _| false,
                ));
                continue;
            };
            let serial = self.colors_serial.clone();
            let guard_target = target.clone();
            let state = self.state.clone();
            let input = Some(target.input.clone());
            row = row.child(crate::color_edit::input_pointer_button_guarded(
                button,
                name.clone(),
                input.clone(),
                move |state, cx, _| guard_target.prepare(state, serial.get(), TextField::active_pending_binding(cx)),
            ).on_click({
                let serial = self.colors_serial.clone();
                move |event, window, cx| {
                    cx.stop_propagation();
                    if event.modifiers().modified()
                        || matches!(event, gpui::ClickEvent::Mouse(click) if click.down.modifiers.modified())
                        || TextField::is_composing(window, cx)
                        || (matches!(event, gpui::ClickEvent::Keyboard(_)) && TextField::active_has_focus(window, cx))
                        || !target.prepare(state.read(cx), serial.get(), TextField::active_pending_binding(cx)) {
                        return;
                    }
                    if crate::color_edit::input_click_target(&name, event, &input, &state, window, cx).is_none() {
                        return;
                    }
                    // Pending fields are rejected before blur. The frozen rendered
                    // explicit intent is never recomputed into an opposite action.
                    state.update(cx, |state, cx| {
                        if target.current(state, serial.get()) {
                            if let Some(action) = target.action(state, control) {
                                state.dispatch(&action, window, cx);
                            }
                        }
                    });
                }
            }));
        }
        root = root
            .child(row)
            .child(div().text_size(px(10.)).text_color(rgb(ui::MUTED)).child(
                if count == 0 && legacy_animated(node) {
                    LEGACY_HELP
                } else {
                    HELP
                },
            ));
        if animation.is_some() {
            let sampled = gradient.sampled_node(node, frame);
            let samples = gradient.preview(node, frame, 256);
            let handles = gradient_ramp::handles(&sampled, frame, None);
            root = root.child(canvas(|_, _, _| (), move |bounds, _, window, _| {
                gradient_ramp::paint_ramp(bounds, &samples, &handles, None, window);
            }).w_full().h(px(64.))).child(
                div().text_size(px(10.)).text_color(rgb(ui::MUTED)).child("Edit Colors changes the complete sample at the playhead. Outgoing interpolation buttons affect only an exact current key; select a key to change its mode.")
            );
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> EditorState {
        let mut state = EditorState::default();
        state
            .editor
            .execute(Command::AddContent {
                content: Content::ShapeContents(Default::default()),
                width: 200.,
                height: 120.,
                name: "Colors".into(),
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
        state.contents_selection = Some((state.editor.project().active_composition_id(), 1, 1));
        state.editor.clear_history();
        state
    }
    fn apply(state: &mut EditorState, kind: Control) {
        let target = Target::capture(state, 4, 1).unwrap();
        state.bulk_test_action(&target.action(state, kind).unwrap());
    }
    #[test]
    fn colors_controls_enable_key_and_navigate_exact_hold_frames() {
        let mut state = scene();
        let before = state.editor.project().clone();
        let target = Target::capture(&state, 4, 1).unwrap();
        assert!(target.current(&state, 4));
        for kind in [Control::Add, Control::Previous, Control::Next] {
            assert!(target.action(&state, kind).is_none());
        }
        assert_eq!(state.editor.project(), &before);
        apply(&mut state, Control::Enable);
        state.bulk_test_action(&Action::Seek(30));
        apply(&mut state, Control::Add);
        state.bulk_test_action(&Action::Seek(15));
        let target = Target::capture(&state, 4, 1).unwrap();
        assert!(matches!(
            target.action(&state, Control::Previous),
            Some(Action::Seek(0))
        ));
        assert!(matches!(
            target.action(&state, Control::Next),
            Some(Action::Seek(30))
        ));
        assert!(matches!(
            target.action(&state, Control::Edit),
            Some(Action::OpenGradient(1))
        ));
        apply(&mut state, Control::Disable);
        let target = Target::capture(&state, 4, 1).unwrap();
        assert!(target.action(&state, Control::Next).is_none());
    }
    #[test]
    fn colors_controls_reject_legacy_animated_stop_conversion_without_mutation() {
        let mut state = scene();
        state.bulk_test_action(&Action::Edit(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 1,
                parameter: ContentsParam::Gradient(GradientParam::Red(1)),
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        }));
        let before = state.editor.project().clone();
        let target = Target::capture(&state, 4, 1).unwrap();
        assert!(target.action(&state, Control::Enable).is_none());
        assert_eq!(state.editor.project(), &before);
    }
    #[test]
    fn colors_controls_reject_source_and_transport_equal_return_callbacks() {
        for actions in [
            vec![Action::Seek(3), Action::Seek(0)],
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
        ] {
            let mut state = scene();
            let target = Target::capture(&state, 4, 1).unwrap();
            let source = state.editor.project().clone();
            for action in actions {
                state.bulk_test_action(&action);
            }
            assert_eq!(state.editor.project(), &source);
            assert!(!target.current(&state, 4));
        }
        let state = scene();
        let target = Target::capture(&state, 4, 1).unwrap();
        assert!(
            !target.current(&state, 6),
            "A → B → A selection serial retires the callback"
        );
    }
    #[test]
    fn colors_controls_block_lock_modal_multiselect_and_pending_source_commit() {
        let mut state = scene();
        let target = Target::capture(&state, 4, 1).unwrap();
        state.contents_selection = None;
        assert!(!target.current(&state, 4));
        assert!(Target::capture(&state, 4, 1).is_none());
        state.contents_selection = Some((state.editor.project().active_composition_id(), 1, 1));
        state.queue_open = true;
        assert!(!target.current(&state, 4));
        state.queue_open = false;
        state.bulk_test_action(&Action::Edit(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 1,
                parameter: ContentsParam::Gradient(GradientParam::EndX),
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 150.,
                },
            },
        }));
        assert!(
            !target.current(&state, 4),
            "an ordinary field may commit, but needs a refreshed Colors click"
        );
        let fresh = Target::capture(&state, 4, 1).unwrap();
        assert!(fresh.current(&state, 4));
        state.bulk_test_action(&Action::Edit(Command::ToggleLocked(1)));
        assert!(!fresh.current(&state, 4));
        assert!(Target::capture(&state, 4, 1).is_none());
    }
    #[test]
    fn colors_controls_reject_pending_before_blur_and_keep_explicit_intent() {
        let mut state = scene();
        let target = Target::capture(&state, 4, 1).unwrap();
        for pending in ["invalid", "unchanged", "source-text"] {
            assert!(!target.prepare(&state, 4, Some(pending.into())));
        }
        assert!(target.prepare(&state, 4, None));
        apply(&mut state, Control::Enable);
        assert!(target.action(&state, Control::Enable).is_none());
        assert!(target.action(&state, Control::Add).is_none());
        let fresh = Target::capture(&state, 4, 1).unwrap();
        assert!(
            fresh
                .action(&state, Control::Mode(GradientColorsInterpolation::Linear))
                .is_some()
        );
        state.bulk_test_action(&Action::Seek(15));
        let between = Target::capture(&state, 4, 1).unwrap();
        assert!(
            between
                .action(&state, Control::Mode(GradientColorsInterpolation::Linear))
                .is_none()
        );
        assert!(between.action(&state, Control::Remove).is_none());
    }
}
