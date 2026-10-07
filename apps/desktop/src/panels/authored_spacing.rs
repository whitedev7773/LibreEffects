//! Explicit reset for saved point-text spacing, with one owned activation press.
use crate::{
    color_edit::InputTarget,
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Entity, prelude::*};
use libre_effects_core::{Command, LayerId};
use libre_effects_editor_model::automation_ui::FocusedToggleAction;

pub(super) fn reset_button(
    state: &Entity<EditorState>,
    id: LayerId,
    target: Option<InputTarget>,
) -> gpui::Stateful<gpui::Div> {
    const CONTROL: &str = "reset-saved-glyph-spacing";
    let key_state = state.clone();
    let key_target = target.clone();
    let button = ui::text_button(CONTROL, "Reset saved spacing")
        .when(target.is_none(), |button| button.opacity(0.4))
        .capture_key_down(move |event, window, cx| {
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
                return;
            }
            cx.stop_propagation();
            window.prevent_default();
            if action == FocusedToggleAction::Activate
                && let Some(target) = target
            {
                reset(target, id, &key_state, window, cx);
            }
        });
    let state = state.clone();
    crate::color_edit::input_pointer_button_preserving_ime(button, CONTROL.into(), target.clone())
        .on_click(move |event, window, cx| {
            if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                return;
            }
            cx.stop_propagation();
            let Some(target) =
                crate::color_edit::input_click_target(CONTROL, event, &target, &state, window, cx)
            else {
                return;
            };
            reset(target, id, &state, window, cx);
        })
}

fn reset(
    target: InputTarget,
    id: LayerId,
    state: &Entity<EditorState>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    if !target.current(state.read(cx)) || TextField::is_composing(window, cx) {
        return;
    }
    TextField::commit_active(window, cx);
    state.update(cx, |state, cx| {
        if !target.same_context(state) {
            return;
        }
        let Some(mut rich) = state
            .editor
            .selected_layer()
            .filter(|layer| layer.id() == id && !layer.locked())
            .and_then(|layer| layer.rich_text().cloned())
        else {
            return;
        };
        if rich.reset_positioning() {
            state.dispatch(
                &Action::Edit(Command::SetRichText {
                    id,
                    rich_text: Some(rich),
                }),
                window,
                cx,
            );
        }
    });
}
