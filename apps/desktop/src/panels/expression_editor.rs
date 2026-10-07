//! Local numeric-expression authoring. Candidate programs are checked by the
//! supervised child process; only the authored command can enter source history.
use crate::{
    components::ScriptTextInput,
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Context, Entity, FocusHandle, KeyDownEvent, KeyUpEvent, Window, div, prelude::*, px, rgb,
};
use libre_effects_core::{Command, ExpressionTarget, Layer, LayerId};
use libre_effects_editor_model::{
    automation_ui::ActivationKeys,
    expression_edit::{ExpressionCheck, ExpressionDraft, PreparedExpression},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

struct PendingCheck {
    revision: u64,
    cancel: Arc<AtomicBool>,
}
impl Drop for PendingCheck {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub(crate) struct Session {
    serial: u64,
    document: u64,
    input: u64,
    transport: u64,
    selected_layers: std::collections::BTreeSet<LayerId>,
    draft: ExpressionDraft,
    title: String,
    error: Option<String>,
    pending: Option<PendingCheck>,
}
impl Session {
    pub(crate) fn new(
        state: &EditorState,
        layer: LayerId,
        target: ExpressionTarget,
    ) -> Result<Self, String> {
        let draft = ExpressionDraft::open(&state.editor, layer, target, state.frame)?;
        let owner = state.editor.project().composition().layer(layer).unwrap();
        let title = format!("{} · {}", owner.name(), target_label(owner, target));
        Ok(Self {
            serial: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            document: state.document_revision,
            input: state.input_context_generation(),
            transport: state.transport_generation(),
            selected_layers: state.selected_layers.clone(),
            draft,
            title,
            error: None,
            pending: None,
        })
    }

    fn current(&self, state: &EditorState) -> bool {
        self.document == state.document_revision
            && self.input == state.input_context_generation()
            && self.transport == state.transport_generation()
            && self.selected_layers == state.selected_layers
            && self.draft.current(&state.editor, state.frame)
            && !state.playing
            && !state.preview_caching
            && state.automation.is_none()
            && state.ae_import.is_none()
            && state.recovery.is_none()
            && state.text_session.is_none()
            && state.vertex_editor.is_none()
            && state.gradient_editor.is_none()
            && state.colors.session.is_none()
    }

    fn owns_check(&self, serial: u64, revision: u64, cancel: &Arc<AtomicBool>) -> bool {
        self.serial == serial
            && self.draft.revision() == revision
            && self.pending.as_ref().is_some_and(|pending| {
                pending.revision == revision
                    && Arc::ptr_eq(&pending.cancel, cancel)
                    && !cancel.load(Ordering::Relaxed)
            })
    }
}

impl EditorState {
    pub(crate) fn invalidate_expression_editor(&mut self) -> bool {
        if self
            .expression_editor
            .as_ref()
            .is_some_and(|session| !session.current(self))
        {
            self.expression_editor = None;
            self.status = "Expression draft canceled because the editing context changed".into();
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel_expression_editor(&mut self) {
        if self.expression_editor.take().is_some() {
            self.status = "Expression edit canceled; project unchanged".into();
        }
    }

    fn expression_input(&mut self, serial: u64, value: &str) {
        if self.invalidate_expression_editor() {
            return;
        }
        let Some(session) = self
            .expression_editor
            .as_mut()
            .filter(|s| s.serial == serial)
        else {
            return;
        };
        match session.draft.set_source(value) {
            Ok(true) => {
                session.pending = None;
                session.error = None;
            }
            Ok(false) => {}
            Err(error) => session.error = Some(error),
        }
    }

    fn expression_edit_activity(&mut self, serial: u64) {
        if let Some(session) = self
            .expression_editor
            .as_mut()
            .filter(|s| s.serial == serial)
        {
            // Marked input is deliberately absent from the committed source
            // callback. It must still invalidate a check of the prior draft.
            session.pending = None;
        }
    }

    fn toggle_expression_enabled(&mut self, serial: u64) {
        if self.invalidate_expression_editor() {
            return;
        }
        let Some(session) = self
            .expression_editor
            .as_mut()
            .filter(|s| s.serial == serial)
        else {
            return;
        };
        session.draft.set_enabled(!session.draft.enabled());
        session.pending = None;
        session.error = None;
    }

    fn commit_expression_command(&mut self, mut session: Session, command: Option<Command>) {
        let Some(command) = command else {
            self.status = "Expression unchanged".into();
            return;
        };
        match self.editor.execute(command) {
            Ok(()) => self.status = "Expression applied".into(),
            Err(error) => {
                session.error = Some(error);
                self.expression_editor = Some(session);
            }
        }
    }

    pub(crate) fn remove_expression_editor(&mut self) {
        if self.invalidate_expression_editor() {
            return;
        }
        let Some(mut session) = self.expression_editor.take() else {
            return;
        };
        session.pending = None;
        match session.draft.remove_command(&self.editor, self.frame) {
            Ok(command) => self.commit_expression_command(session, command),
            Err(error) => {
                session.error = Some(error);
                self.expression_editor = Some(session);
            }
        }
    }

    pub(crate) fn apply_expression_editor(&mut self, cx: &mut Context<Self>) {
        if self.invalidate_expression_editor() {
            return;
        }
        let Some(mut session) = self.expression_editor.take() else {
            return;
        };
        if session.pending.is_some() {
            self.expression_editor = Some(session);
            return;
        }
        let check = match session.draft.prepare(&self.editor, self.frame) {
            Ok(PreparedExpression::Ready(command)) => {
                self.commit_expression_command(session, command);
                return;
            }
            Ok(PreparedExpression::Check(check)) => check,
            Err(error) => {
                session.error = Some(error);
                self.expression_editor = Some(session);
                return;
            }
        };
        let serial = session.serial;
        let revision = session.draft.revision();
        let cancel = Arc::new(AtomicBool::new(false));
        session.pending = Some(PendingCheck {
            revision,
            cancel: cancel.clone(),
        });
        session.error = None;
        self.expression_editor = Some(session);
        let (sender, receiver) = mpsc::channel();
        let worker_cancel = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("expression-authoring-check".into())
            .stack_size(4 * 1024 * 1024)
            .spawn(move || {
                let result = crate::automation_process::evaluate_expressions(
                    &check.snapshot,
                    &check.roots,
                    worker_cancel,
                )
                .map_err(|error| error.to_string());
                let _ = sender.send((check, result));
            });
        if let Err(error) = worker {
            let session = self.expression_editor.as_mut().unwrap();
            session.pending = None;
            session.error = Some(format!("Cannot start expression check: {error}"));
            return;
        }
        cx.spawn(async move |entity, cx| {
            loop {
                let current = entity.update(cx, |state, cx| {
                    if state.invalidate_expression_editor() {
                        cx.notify();
                    }
                    state
                        .expression_editor
                        .as_ref()
                        .is_some_and(|session| session.owns_check(serial, revision, &cancel))
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
                match receiver.try_recv() {
                    Ok((check, result)) => {
                        let _ = entity.update(cx, |state, cx| {
                            state
                                .finish_expression_check(serial, revision, &cancel, &check, result);
                            cx.notify();
                        });
                        return;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        let _ = entity.update(cx, |state, cx| {
                            if let Some(session) = state
                                .expression_editor
                                .as_mut()
                                .filter(|s| s.owns_check(serial, revision, &cancel))
                            {
                                session.pending = None;
                                session.error =
                                    Some("Expression check stopped unexpectedly".into());
                            }
                            cx.notify();
                        });
                        return;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
            }
        })
        .detach();
    }

    fn finish_expression_check(
        &mut self,
        serial: u64,
        revision: u64,
        cancel: &Arc<AtomicBool>,
        check: &ExpressionCheck,
        result: Result<libre_effects_ae_expressions::EvaluatedProperties, String>,
    ) {
        if self.invalidate_expression_editor() {
            return;
        }
        if !self
            .expression_editor
            .as_ref()
            .is_some_and(|session| session.owns_check(serial, revision, cancel))
        {
            return;
        }
        let mut session = self.expression_editor.take().unwrap();
        session.pending = None;
        match session
            .draft
            .finish(&self.editor, self.frame, check, result)
        {
            Ok(command) => self.commit_expression_command(session, command),
            Err(error) => {
                session.error = Some(error);
                self.expression_editor = Some(session);
            }
        }
    }
}

fn target_label(layer: &Layer, target: ExpressionTarget) -> String {
    match target {
        ExpressionTarget::Position => "Position".into(),
        ExpressionTarget::Scale => "Scale".into(),
        ExpressionTarget::Opacity => "Opacity".into(),
        ExpressionTarget::SourceText => "Source Text".into(),
        ExpressionTarget::MaskPath(id) => format!("Mask {id} Path"),
        ExpressionTarget::Slider(id) => layer
            .effect_stack()
            .iter()
            .find(|effect| effect.id() == id)
            .map(|effect| format!("{} · Slider", effect.name()))
            .unwrap_or_else(|| "Slider".into()),
    }
}

/// The same native entrypoint is shared by transform and Slider properties.
pub(super) fn entry_button(
    state: &Entity<EditorState>,
    layer: &Layer,
    target: ExpressionTarget,
) -> gpui::Stateful<gpui::Div> {
    let state = state.clone();
    let id = layer.id();
    let locked = layer.locked();
    let program = layer.expression(target);
    let enabled = program.is_some_and(|program| program.enabled);
    let label = if program.is_some() { "fx" } else { "fx+" };
    let tip: gpui::SharedString = format!(
        "{} {} expression{}",
        if program.is_some() { "Edit" } else { "Add" },
        target_label(layer, target),
        if locked {
            " · unlock layer first"
        } else if enabled {
            " · enabled"
        } else if program.is_some() {
            " · disabled"
        } else {
            ""
        },
    )
    .into();
    let button = ui::text_button(
        gpui::SharedString::from(format!("expression-{id}-{target:?}")),
        label,
    )
    .flex_none()
    .text_size(px(10.0))
    .text_color(rgb(if enabled { ui::BLUE } else { ui::MUTED }))
    .when(locked, |button| button.opacity(0.45))
    .tooltip(move |_, cx| cx.new(|_| ui::Tip(tip.clone())).into())
    .on_click(move |_, window, cx| {
        cx.stop_propagation();
        if !locked {
            state.update(cx, |state, cx| {
                state.dispatch(&Action::OpenExpression(id, target), window, cx)
            });
        }
    });
    // Register with the workspace ancestor, which runs before sibling input
    // outside-down handlers. A pending base/name field or IME stays untouched.
    crate::color_edit::input_pointer_navigation_guarded(button, move |state, _| {
        state.svg_import_available() && state.editor.selected() == Some(id) && !locked
    })
}

pub(crate) struct ExpressionEditor {
    state: Entity<EditorState>,
    focus: FocusHandle,
    enabled_focus: FocusHandle,
    remove_focus: FocusHandle,
    cancel_focus: FocusHandle,
    apply_focus: FocusHandle,
    field: Option<(u64, Entity<ScriptTextInput>)>,
    keys: ActivationKeys,
}
impl ExpressionEditor {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            focus: cx.focus_handle(),
            enabled_focus: cx.focus_handle(),
            remove_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            apply_focus: cx.focus_handle(),
            field: None,
            keys: crate::modal_keyboard::activation_keys(cx),
        }
    }

    pub(crate) fn contains_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focus.contains_focused(window, cx)
    }

    pub(crate) fn suppress_activation_key(&mut self, key: &str, held: bool) {
        self.keys.press(key, held);
    }

    pub(crate) fn release_key(&mut self, key: &str) {
        self.keys.release(key);
    }

    fn key_up(&mut self, event: &KeyUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.release_key(event.keystroke.key.as_str());
    }

    fn action(&mut self, serial: u64, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if !self
            .state
            .read(cx)
            .expression_editor
            .as_ref()
            .is_some_and(|s| s.serial == serial)
        {
            return;
        }
        if !matches!(action, Action::CancelExpression)
            && self
                .field
                .as_ref()
                .is_some_and(|(_, field)| field.read(cx).is_composing())
        {
            return;
        }
        self.state
            .update(cx, |state, cx| state.dispatch(&action, window, cx));
        if self.state.read(cx).expression_editor.is_none() {
            window.blur();
        }
        cx.notify();
    }

    fn toggle(&mut self, serial: u64, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            state.toggle_expression_enabled(serial);
            cx.notify();
        });
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let fresh = self.keys.press(key, event.is_held);
        let Some(session) = self.state.read(cx).expression_editor.as_ref() else {
            cx.stop_propagation();
            window.prevent_default();
            return;
        };
        let (serial, busy, existing) = (
            session.serial,
            session.pending.is_some(),
            session.draft.has_existing(),
        );
        let Some((_, field)) = self.field.as_ref() else {
            return;
        };
        let typing = field.read(cx).has_focus(window);
        if field.read(cx).is_composing() {
            // Native composition owns its Enter/Escape and commit sequence.
            if matches!(key, "enter" | "escape" | "tab") {
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        let command = modifiers.control || modifiers.platform;
        if key == "tab" && !command && !modifiers.alt {
            cx.stop_propagation();
            window.prevent_default();
            let mut order = vec![field.read(cx).focus_handle(), self.enabled_focus.clone()];
            if existing {
                order.push(self.remove_focus.clone());
            }
            order.push(self.cancel_focus.clone());
            if !busy {
                order.push(self.apply_focus.clone());
            }
            let at = order
                .iter()
                .position(|focus| focus.is_focused(window))
                .unwrap_or(0);
            let next = if modifiers.shift {
                (at + order.len() - 1) % order.len()
            } else {
                (at + 1) % order.len()
            };
            window.focus(&order[next]);
            return;
        }
        if key == "escape" || (key == "enter" && command && !modifiers.alt) {
            cx.stop_propagation();
            window.prevent_default();
            if fresh && !modifiers.shift {
                if key == "escape" && !command && !modifiers.alt {
                    self.action(serial, Action::CancelExpression, window, cx);
                } else if key == "enter" && !busy {
                    self.action(serial, Action::ApplyExpression, window, cx);
                }
            }
            return;
        }
        if typing {
            return;
        }
        if modifiers.alt && key == "f4" {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        if fresh
            && !command
            && !modifiers.alt
            && !modifiers.shift
            && matches!(key, "enter" | "space")
        {
            if self.enabled_focus.is_focused(window) {
                self.toggle(serial, cx);
            } else if self.remove_focus.is_focused(window) && existing {
                self.action(serial, Action::RemoveExpression, window, cx);
            } else if self.cancel_focus.is_focused(window) {
                self.action(serial, Action::CancelExpression, window, cx);
            } else if self.apply_focus.is_focused(window) && !busy {
                self.action(serial, Action::ApplyExpression, window, cx);
            }
        }
    }
}

impl Render for ExpressionEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(session) = self.state.read(cx).expression_editor.as_ref() else {
            self.field = None;
            return div().into_any_element();
        };
        let (serial, title, source, enabled, existing, busy, error, frame, target, locals) = (
            session.serial,
            session.title.clone(),
            session.draft.source().to_string(),
            session.draft.enabled(),
            session.draft.has_existing(),
            session.pending.is_some(),
            session.error.clone(),
            session.draft.frame(),
            session.draft.target(),
            self.state
                .read(cx)
                .editor
                .project()
                .composition()
                .layer(session.draft.layer())
                .and_then(|layer| layer.expression(session.draft.target()))
                .map_or_else(Vec::new, |program| program.local_bindings.clone()),
        );
        if self.field.as_ref().is_none_or(|(id, _)| *id != serial) {
            let state = self.state.clone();
            let activity_state = self.state.clone();
            let field = cx.new(|cx| {
                ScriptTextInput::new(cx, true, move |text, _, cx| {
                    state.update(cx, |state, cx| {
                        state.expression_input(serial, text);
                        cx.notify();
                    });
                })
            });
            field.update(cx, |field, _| {
                field.set_edit_activity(move |_, cx| {
                    activity_state.update(cx, |state, cx| {
                        state.expression_edit_activity(serial);
                        cx.notify();
                    });
                });
                // Sync is display-only. The model retains exact stored bytes,
                // including CRLF, until the first actual source edit callback.
                field.sync(format!("expression-{serial}"), source.clone(), window);
            });
            cx.stop_active_drag(window);
            field.read(cx).focus_input(window);
            self.field = Some((serial, field));
        }
        let field = self.field.as_ref().unwrap().1.clone();
        let keyboard_warning = crate::modal_keyboard::warning(cx);
        // Leave the error and action row visible when the window is short.
        let reserved = if error.is_some() { 460.0 } else { 360.0 }
            + if keyboard_warning.is_some() {
                60.0
            } else {
                0.0
            };
        let height = (f32::from(window.viewport_size().height) - reserved).clamp(72.0, 340.0);
        field.update(cx, |field, _| field.set_height(height));
        if !self.contains_focus(window, cx) {
            field.read(cx).focus_input(window);
        }
        if busy && self.apply_focus.is_focused(window) {
            window.focus(&self.cancel_focus);
        }
        let mut footer = div().flex_none().flex().items_center().gap_2();
        if existing {
            footer = footer.child(
                ui::text_button("expression-remove", "Remove expression")
                    .track_focus(&self.remove_focus)
                    .on_click(cx.listener(move |this, event, window, cx| {
                        if pointer_click(event) {
                            this.action(serial, Action::RemoveExpression, window, cx);
                        }
                    })),
            );
        }
        footer = footer
            .child(div().flex_1())
            .child(
                ui::text_button("expression-cancel", "Cancel")
                    .track_focus(&self.cancel_focus)
                    .on_click(cx.listener(move |this, event, window, cx| {
                        if pointer_click(event) {
                            this.action(serial, Action::CancelExpression, window, cx);
                        }
                    })),
            )
            .child(
                ui::text_button(
                    "expression-apply",
                    if busy {
                        "Checking…"
                    } else if enabled {
                        "Apply"
                    } else {
                        "Save disabled"
                    },
                )
                .track_focus(&self.apply_focus)
                .bg(rgb(0x164a7b))
                .when(busy, |button| button.opacity(0.5))
                .on_click(cx.listener(move |this, event, window, cx| {
                    if pointer_click(event) && !busy {
                        this.action(serial, Action::ApplyExpression, window, cx);
                    }
                })),
            );
        div()
            .id("expression-editor-dialog")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::key))
            .capture_key_up(cx.listener(Self::key_up))
            .on_key_down(|_, window, cx| {
                if !ScriptTextInput::active_has_focus(window, cx) { cx.stop_propagation(); }
            })
            .w(px((f32::from(window.viewport_size().width) - 40.0).clamp(280.0, 760.0)))
            .max_h(window.viewport_size().height - px(24.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(0x555555))
            .rounded_md()
            .shadow_lg()
            .child(div().flex_none().text_size(px(16.0)).child("Edit expression"))
            .child(div().flex_none().text_size(px(12.0)).child(title))
            .child(div().flex_none().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(
                match target {
                    ExpressionTarget::Position | ExpressionTarget::Scale => "Return [x, y]. value is the authored base pair; time is seconds.",
                    ExpressionTarget::SourceText => "Return a string. Uniform character styling is preserved; time is seconds.",
                    ExpressionTarget::MaskPath(_) => "Return createPath(points, inTangents, outTangents, true). Tangents are relative offsets.",
                    _ => "Return a number. value is the authored base value; time is seconds.",
                },
            ))
            .when(!locals.is_empty(), |element| element.child(div().flex_none().text_size(px(11.0)).text_color(rgb(ui::MUTED))
                .child(format!("Explicit local bindings: {}", locals.join(", ")))))
            .child(div().flex_none().child(field))
            .child(div().flex_none().flex().items_center().gap_2().child(
                ui::text_button("expression-enabled", if enabled { "Enabled: On" } else { "Enabled: Off" })
                    .track_focus(&self.enabled_focus)
                    .on_click(cx.listener(move |this, event, _, cx| {
                        if pointer_click(event) { this.toggle(serial, cx); }
                    })),
            ).child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(format!("{} / {} bytes", source.len(), libre_effects_core::MAX_EXPRESSION_SOURCE_BYTES))))
            .child(div().flex_none().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(
                if busy { format!("Checking at frame {frame}… Editing or Cancel discards this check.") } else if enabled { format!("Apply checks frame {frame}. Preview and export evaluate other frames when used. Ctrl/Cmd+Enter applies; Escape cancels.") } else { "Disabled source saves without evaluation. Ctrl/Cmd+Enter saves; Escape cancels.".into() },
            ))
            .children(error.map(|error| div().id("expression-error").flex_none().max_h(px(96.0)).overflow_y_scroll().text_size(px(12.0)).text_color(rgb(0xffaa88)).child(error)))
            .children(keyboard_warning.map(|warning| div().flex_none().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(warning)))
            .child(footer)
            .into_any_element()
    }
}

fn pointer_click(event: &gpui::ClickEvent) -> bool {
    matches!(event, gpui::ClickEvent::Mouse(click) if !click.down.first_mouse)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> EditorState {
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state.selected_layers = [1].into();
        state
    }

    fn open(state: &mut EditorState) -> u64 {
        let session = Session::new(state, 1, ExpressionTarget::Opacity).unwrap();
        let serial = session.serial;
        state.expression_editor = Some(session);
        serial
    }

    #[test]
    fn cancel_keeps_authored_source_and_redo_and_cancels_the_pending_check() {
        let mut state = state();
        state
            .editor
            .execute(Command::SetExpression {
                id: 1,
                target: ExpressionTarget::Opacity,
                source: "value\r\n".into(),
                enabled: true,
            })
            .unwrap();
        state
            .editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Temporary".into(),
            })
            .unwrap();
        state.editor.undo();
        let source = state.editor.project().clone();
        let serial = open(&mut state);
        assert_eq!(
            state.expression_editor.as_ref().unwrap().draft.source(),
            "value\r\n"
        );
        state.expression_input(serial, "value + 20");
        state.toggle_expression_enabled(serial);
        let cancel = Arc::new(AtomicBool::new(false));
        let session = state.expression_editor.as_mut().unwrap();
        session.pending = Some(PendingCheck {
            revision: session.draft.revision(),
            cancel: cancel.clone(),
        });
        state.cancel_expression_editor();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(state.expression_editor.is_none());
        assert_eq!(state.editor.project(), &source);
        assert!(state.editor.can_redo());
    }

    #[test]
    fn old_callbacks_cannot_edit_a_reopened_draft_and_typing_cancels_its_check() {
        let mut state = state();
        let first = open(&mut state);
        state.cancel_expression_editor();
        let second = open(&mut state);
        state.expression_input(first, "99");
        assert_eq!(
            state.expression_editor.as_ref().unwrap().draft.source(),
            "value"
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let session = state.expression_editor.as_mut().unwrap();
        session.pending = Some(PendingCheck {
            revision: session.draft.revision(),
            cancel: cancel.clone(),
        });
        state.expression_input(second, "42");
        assert!(cancel.load(Ordering::Relaxed));
        assert!(state.expression_editor.as_ref().unwrap().pending.is_none());
        assert_eq!(
            state.expression_editor.as_ref().unwrap().draft.source(),
            "42"
        );
        assert!(
            state
                .editor
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .expressions()
                .is_empty()
        );
    }

    #[test]
    fn marked_or_unchanged_edit_activity_retires_check_without_publishing_source() {
        let mut state = state();
        let serial = open(&mut state);
        let cancel = Arc::new(AtomicBool::new(false));
        let session = state.expression_editor.as_mut().unwrap();
        let revision = session.draft.revision();
        let source = session.draft.source().to_string();
        session.pending = Some(PendingCheck {
            revision,
            cancel: cancel.clone(),
        });
        // This hook runs before marked replacement, even when the text equals
        // the existing buffer and the committed-source callback is suppressed.
        state.expression_edit_activity(serial);
        assert!(cancel.load(Ordering::Relaxed));
        let session = state.expression_editor.as_ref().unwrap();
        assert!(session.pending.is_none());
        assert_eq!(session.draft.revision(), revision);
        assert_eq!(session.draft.source(), source);
        assert!(
            state
                .editor
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .expressions()
                .is_empty()
        );
    }

    #[test]
    fn canceled_attempt_cannot_own_a_new_check_of_the_same_source_revision() {
        let mut state = state();
        let serial = open(&mut state);
        let old_cancel = Arc::new(AtomicBool::new(false));
        let session = state.expression_editor.as_mut().unwrap();
        let revision = session.draft.revision();
        session.pending = Some(PendingCheck {
            revision,
            cancel: old_cancel.clone(),
        });
        assert!(session.owns_check(serial, revision, &old_cancel));
        state.expression_edit_activity(serial);
        let new_cancel = Arc::new(AtomicBool::new(false));
        let session = state.expression_editor.as_mut().unwrap();
        session.pending = Some(PendingCheck {
            revision,
            cancel: new_cancel.clone(),
        });
        assert_eq!(session.draft.revision(), revision);
        assert!(!session.owns_check(serial, revision, &old_cancel));
        assert!(session.owns_check(serial, revision, &new_cancel));
        assert!(!new_cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn frame_or_document_or_selection_changes_retire_the_local_draft() {
        for change in 0..3 {
            let mut state = state();
            let source = state.editor.project().clone();
            let serial = open(&mut state);
            state.expression_input(serial, "42");
            match change {
                0 => state.frame += 1,
                1 => state.document_revision += 1,
                _ => state.selected_layers.clear(),
            }
            assert!(state.invalidate_expression_editor());
            assert!(state.expression_editor.is_none());
            assert_eq!(state.editor.project(), &source);
        }
    }
}
