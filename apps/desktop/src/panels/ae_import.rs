//! Native root-composition selection for typed AE project data. Pointer clicks
//! and keyboard activation share the same session-scoped readiness checks.
use crate::{
    editor::{EditorState, ae_import::Stage},
    ui,
};
use gpui::{
    Context, Entity, FocusHandle, KeyDownEvent, KeyUpEvent, Window, div, prelude::*, px, rgb,
};
use libre_effects_editor_model::{ae_import_ui, automation_ui::ActivationKeys};

pub(crate) struct AeImport {
    state: Entity<EditorState>,
    focus: FocusHandle,
    list_focus: FocusHandle,
    cancel_focus: FocusHandle,
    apply_focus: FocusHandle,
    operation: Option<u64>,
    stage: Option<Stage>,
    chooser_returned: bool,
    keys: ActivationKeys,
    scroll: gpui::ScrollHandle,
}
impl AeImport {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            focus: cx.focus_handle(),
            list_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            apply_focus: cx.focus_handle(),
            operation: None,
            stage: None,
            chooser_returned: false,
            keys: crate::modal_keyboard::activation_keys(cx),
            scroll: gpui::ScrollHandle::new(),
        }
    }
    pub(crate) fn contains_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focus.contains_focused(window, cx)
    }
    pub(crate) fn release_key(&mut self, key: &str) {
        self.keys.release(key);
    }
    pub(crate) fn suppress_activation_key(&mut self, key: &str, is_held: bool) {
        self.keys.press(key, is_held);
    }
    fn key_up(&mut self, event: &KeyUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.release_key(event.keystroke.key.as_str());
    }
    fn cancel(&mut self, operation: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self
            .state
            .read(cx)
            .ae_import
            .as_ref()
            .is_some_and(|session| session.operation == operation)
        {
            return;
        }
        self.state.update(cx, |state, cx| {
            state.cancel_ae_import();
            cx.notify();
        });
        window.blur();
    }
    fn focus_index(&self, window: &Window) -> usize {
        if self.apply_focus.is_focused(window) {
            2
        } else if self.cancel_focus.is_focused(window) {
            1
        } else {
            0
        }
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let fresh = self.keys.press(key, event.is_held);
        if modifiers.alt && key == "f4" {
            return;
        }
        // This modal has no text fields. Never let letters, shortcuts or a held
        // activation fall through to the editor or an occluded native button.
        cx.stop_propagation();
        window.prevent_default();
        if modifiers.control || modifiers.alt || modifiers.platform {
            return;
        }
        let Some(session) = self.state.read(cx).ae_import.as_ref() else {
            return;
        };
        let (operation, selected, count, can_apply, ready) = (
            session.operation,
            session.selection,
            session.roots.len(),
            session.can_apply() && !self.state.read(cx).saving,
            session.stage == Stage::Ready,
        );
        let focused = self.focus_index(window);
        if key == "tab" {
            let next = ae_import_ui::next_focus(focused, modifiers.shift, can_apply);
            window.focus(match next {
                1 => &self.cancel_focus,
                2 => &self.apply_focus,
                _ => &self.list_focus,
            });
            return;
        }
        if modifiers.shift {
            return;
        }
        if focused == 0 && ready && matches!(key, "up" | "down" | "left" | "right" | "home" | "end")
        {
            if let Some(index) = ae_import_ui::list_selection(selected, count, key) {
                self.state.update(cx, |state, cx| {
                    state.select_ae_import_root(operation, index);
                    cx.notify();
                });
                self.scroll.scroll_to_item(index);
                window.focus(&self.list_focus);
            }
            return;
        }
        if !fresh {
            return;
        }
        if key == "escape" || (focused == 1 && matches!(key, "enter" | "space")) {
            self.cancel(operation, window, cx);
        } else if can_apply && (key == "enter" || (key == "space" && focused == 2)) {
            self.state
                .update(cx, |state, cx| state.request_ae_import_apply(operation, cx));
        }
    }
}

impl Render for AeImport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(session) = self.state.read(cx).ae_import.as_ref() else {
            return div().into_any_element();
        };
        let (operation, name, roots, selected, stage, error, can_apply) = (
            session.operation,
            session.name.clone(),
            session.roots.clone(),
            session.selection,
            session.stage,
            session.error.clone(),
            session.can_apply() && !self.state.read(cx).saving,
        );
        if self.operation != Some(operation) {
            self.operation = Some(operation);
            self.stage = None;
            self.chooser_returned = false;
            // Do not clear activation ownership here. A held key which opened
            // the chooser must not become a fresh Apply after it closes.
            cx.stop_active_drag(window);
            window.focus(&self.list_focus);
        }
        if !self.chooser_returned && stage != Stage::Choosing {
            self.chooser_returned = true;
            // A native picker may consume key-down but send its release/repeat
            // to this window. Demand a release before activating this modal.
            for key in ["enter", "space", "escape"] {
                self.keys.press(key, true);
            }
        }
        if self.stage == Some(Stage::AwaitingConfirmation) && stage == Stage::Ready {
            window.focus(&self.list_focus);
        }
        self.stage = Some(stage);
        if !can_apply && self.apply_focus.is_focused(window) {
            window.focus(&self.cancel_focus);
        }
        let detail = error.or_else(|| {
            selected
                .and_then(|index| roots.get(index))
                .and_then(|root| root.blocker.clone())
        });
        let stage_text = match stage {
            Stage::Choosing => "Choose one .json or .aep file in the file picker.",
            Stage::Reading => "Reading and validating project data…",
            Stage::Ready if can_apply => {
                "Ready. Apply replaces the open project after the usual save-changes prompt."
            }
            Stage::Ready => "This root cannot be imported. Select a supported root or Cancel.",
            Stage::Converting => "Preparing the selected root without modifying the open project…",
            Stage::AwaitingConfirmation => "Waiting for the save-changes decision…",
            Stage::Failed => "No project was imported. Cancel to return to your project.",
        };
        let mut list = div()
            .id("ae-import-roots")
            .track_focus(&self.list_focus)
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .min_h(px(72.0))
            .max_h(px(
                (f32::from(window.viewport_size().height) - 320.0).clamp(72.0, 300.0)
            ))
            .flex()
            .flex_col()
            .gap_1();
        for (index, root) in roots.iter().enumerate() {
            let selected = selected == Some(index);
            let blocked = root.blocker.is_some();
            let name: String = root
                .name
                .chars()
                .filter(|c| !c.is_control())
                .take(160)
                .collect();
            list =
                list.child(
                    div()
                        .id(("ae-import-root", index))
                        .px_3()
                        .py_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .border_1()
                        .border_color(rgb(if selected { ui::BLUE } else { 0x444444 }))
                        .bg(rgb(if selected { 0x29394e } else { ui::PANEL }))
                        .when(stage == Stage::Ready, |row| {
                            row.cursor_pointer().on_click(cx.listener(
                                move |this, event, window, cx| {
                                    if !pointer_click(event) {
                                        return;
                                    }
                                    this.state.update(cx, |state, cx| {
                                        state.select_ae_import_root(operation, index);
                                        cx.notify();
                                    });
                                    window.focus(&this.list_focus);
                                },
                            ))
                        })
                        .child(format!("{}{}", if selected { "● " } else { "○ " }, name))
                        .child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(
                            format!(
                                "Root {} · {} layer(s) · {}",
                                root.id,
                                root.layers,
                                if blocked { "Blocked" } else { "Ready" }
                            ),
                        )),
                );
        }
        div()
            .id("ae-project-import-dialog")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::key))
            .capture_key_up(cx.listener(Self::key_up))
            .on_key_down(|_, window, cx| { cx.stop_propagation(); window.prevent_default(); })
            .w(px((f32::from(window.viewport_size().width) - 48.0).clamp(260.0, 660.0)))
            .max_h(px((f32::from(window.viewport_size().height) - 36.0).max(200.0)))
            .p_4().flex().flex_col().gap_3()
            .bg(rgb(ui::PANEL)).border_1().border_color(rgb(0x555555)).rounded_md().shadow_lg()
            .child(div().text_size(px(16.0)).child("Import AE project data"))
            .child(div().text_color(rgb(ui::MUTED)).text_size(px(11.0)).child(name))
            .child("Typed interchange JSON is supported. Binary AEP files are inspected only; unsupported payloads are blocked.")
            .children(crate::modal_keyboard::warning(cx).map(|message| {
                div().text_color(rgb(ui::MUTED)).text_size(px(11.0)).child(message)
            }))
            .child(list)
            .when_some(detail, |dialog, detail| dialog.child(
                div().id("ae-import-blocker").max_h(px(96.0)).overflow_y_scroll()
                    .text_color(rgb(0xffc278)).text_size(px(12.0)).child(detail),
            ))
            .child(div().text_size(px(12.0)).child(stage_text))
            .child(div().text_color(rgb(ui::MUTED)).text_size(px(11.0)).child("↑/↓ or Home/End selects a root · Tab changes focus"))
            .child(div().flex().justify_end().gap_2()
                .child(ui::text_button("ae-import-cancel", "Cancel").track_focus(&self.cancel_focus)
                    .on_click(cx.listener(move |this, event, window, cx| {
                        if pointer_click(event) { this.cancel(operation, window, cx); }
                    })))
                .child(ui::text_button("ae-import-apply", "Apply").track_focus(&self.apply_focus)
                    .opacity(if can_apply { 1.0 } else { 0.4 })
                    .when(can_apply, |button| button.on_click(cx.listener(move |this, event, _, cx| {
                        if pointer_click(event) {
                            this.state.update(cx, |state, cx| state.request_ae_import_apply(operation, cx));
                        }
                    }))))
            )
            .into_any_element()
    }
}
fn pointer_click(event: &gpui::ClickEvent) -> bool {
    matches!(event, gpui::ClickEvent::Mouse(click) if !click.down.first_mouse)
}
