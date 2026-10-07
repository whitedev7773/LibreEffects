//! A static edge policy uses the existing input receipt and one document command.
use super::*;
use crate::color_edit::InputTarget;
use libre_effects_core::{EffectColorSpace, EffectInstance, GaussianEdgeMode, Layer};
use libre_effects_editor_model::automation_ui::FocusedToggleAction;

#[derive(Default)]
pub(super) struct Controls {
    source: Option<InputTarget>,
}
impl Controls {
    pub(super) fn refresh(&mut self, state: &EditorState) {
        if state.editor.selected_layer().is_some_and(|layer| {
            layer
                .effect_stack()
                .iter()
                .any(|effect| effect.kind() == EffectKind::GaussianBlur)
        }) {
            InputTarget::refresh(&mut self.source, state);
        } else {
            self.source = None;
        }
    }
    pub(super) fn render(
        &self,
        state: &Entity<EditorState>,
        layer: &Layer,
        effect: &EffectInstance,
    ) -> gpui::Stateful<gpui::Div> {
        let id = layer.id();
        let effect_id = effect.id();
        let control = format!("gaussian-edge-{id}-{effect_id}");
        let repeat = effect.gaussian_edge_mode() == GaussianEdgeMode::Repeat;
        let available = !layer.locked() && effect.color_space() == EffectColorSpace::Srgb;
        let target = available.then(|| self.source.clone()).flatten();
        let button = ui::text_button(
            SharedString::from(control.clone()),
            if repeat {
                "☑ Repeat Edge Pixels"
            } else {
                "☐ Repeat Edge Pixels"
            },
        )
        .when(!available, |button| button.opacity(0.4));
        let key_state = state.clone();
        let key_target = target.clone();
        let button = button.capture_key_down(move |event, window, cx| {
            if !matches!(event.keystroke.key.as_str(), "enter" | "space") {
                return;
            }
            let target = key_target
                .as_ref()
                .filter(|target| target.current(key_state.read(cx)))
                .cloned();
            let action = crate::modal_keyboard::take_press(event, window, cx).map_or(
                FocusedToggleAction::Consume,
                |press| {
                    press.focused_toggle(
                        event.keystroke.modifiers.modified(),
                        window.is_window_active(),
                        key_state.read(cx).playing,
                        target.is_some(),
                    )
                },
            );
            if action == FocusedToggleAction::Pass {
                return; // First Space while playing belongs to transport.
            }
            cx.stop_propagation();
            window.prevent_default();
            if action == FocusedToggleAction::Activate
                && let Some(target) = target
            {
                toggle(target, id, effect_id, &key_state, window, cx);
            }
        });
        let state = state.clone();
        crate::color_edit::input_pointer_button(button, control.clone(), target.clone()).on_click(
            move |event, window, cx| {
                // GPUI synthesizes these on key-up using current eligibility.
                // The complete press is already owned by key-down or transport.
                if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                    return;
                }
                cx.stop_propagation();
                let Some(target) = crate::color_edit::input_click_target(
                    &control, event, &target, &state, window, cx,
                ) else {
                    return;
                };
                toggle(target, id, effect_id, &state, window, cx);
            },
        )
    }
}

fn toggle(
    target: InputTarget,
    id: LayerId,
    effect_id: EffectId,
    state: &Entity<EditorState>,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    if !target.current(state.read(cx)) {
        return;
    }
    TextField::commit_active(window, cx);
    state.update(cx, |state, cx| {
        if !target.same_context(state) {
            return;
        }
        state.finish_text(true, cx);
        if !target.same_context(state) {
            return;
        }
        let Some(effect) = state
            .editor
            .selected_layer()
            .filter(|layer| layer.id() == id && !layer.locked())
            .and_then(|layer| {
                layer.effect_stack().iter().find(|effect| {
                    effect.id() == effect_id
                        && effect.kind() == EffectKind::GaussianBlur
                        && effect.color_space() == EffectColorSpace::Srgb
                })
            })
        else {
            return;
        };
        let mode = if effect.gaussian_edge_mode() == GaussianEdgeMode::Repeat {
            GaussianEdgeMode::Transparent
        } else {
            GaussianEdgeMode::Repeat
        };
        state.dispatch(
            &Action::Edit(Command::Effect {
                id,
                edit: EffectEdit::SetGaussianEdgeMode {
                    effect: effect_id,
                    mode,
                },
            }),
            window,
            cx,
        );
    });
}
