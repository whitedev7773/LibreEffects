//! Native controls for the bounded ScriptUI dialog protocol. The VM retains the
//! callback closures; this view only forwards validated, session-scoped events.
use crate::{components::ScriptTextInput, editor::EditorState, ui};
use gpui::{
    Bounds, Context, Entity, FocusHandle, KeyDownEvent, KeyUpEvent, Pixels, ScrollHandle,
    SharedString, Window, div, prelude::*, px, rgb,
};
use libre_effects_editor_model::automation::{UiNode, UiRequest, UiResponse};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

mod policy;
use policy::{ControlSize, DialogSize, EditWidth, FocusRequests, reveal_delta};

const CANCEL_SCRIPT: u64 = u64::MAX;
const DIALOG_YES: u64 = u64::MAX - 1;
const DIALOG_NO: u64 = u64::MAX - 2;

pub(crate) struct ScriptUi {
    state: Entity<EditorState>,
    focus: FocusHandle,
    fields: BTreeMap<u64, Entity<ScriptTextInput>>,
    buttons: BTreeMap<u64, FocusHandle>,
    operation: Option<u64>,
    dialog_key: Option<(u64, u8)>,
    tab_order: Vec<(u64, bool)>,
    dialog_focus: BTreeMap<u64, (u64, bool)>,
    focus_requests: BTreeMap<u64, FocusRequests>,
    body_scroll: ScrollHandle,
    control_bounds: Rc<RefCell<BTreeMap<u64, Bounds<Pixels>>>>,
    reveal_serial: Rc<Cell<u64>>,
    control_space: [f32; 2],
    sync_revision: u64,
    viewport: Option<(f32, f32)>,
    activation_keys: libre_effects_editor_model::automation_ui::ActivationKeys,
}
impl ScriptUi {
    pub(crate) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            focus: cx.focus_handle(),
            fields: BTreeMap::new(),
            buttons: BTreeMap::new(),
            operation: None,
            dialog_key: None,
            tab_order: vec![],
            dialog_focus: BTreeMap::new(),
            focus_requests: BTreeMap::new(),
            body_scroll: ScrollHandle::new(),
            control_bounds: Rc::default(),
            reveal_serial: Rc::default(),
            control_space: [800.0, 600.0],
            sync_revision: 0,
            viewport: None,
            activation_keys: crate::modal_keyboard::activation_keys(cx),
        }
    }
    pub(crate) fn contains_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focus.contains_focused(window, cx)
    }
    fn focused_control(&self, window: &Window, cx: &gpui::App) -> Option<(u64, bool)> {
        self.tab_order.iter().copied().find(|(id, field)| {
            if *field {
                self.fields
                    .get(id)
                    .is_some_and(|f| f.read(cx).has_focus(window))
            } else {
                self.buttons.get(id).is_some_and(|f| f.is_focused(window))
            }
        })
    }
    fn focus_control(&self, id: u64, field: bool, window: &mut Window, cx: &mut Context<Self>) {
        if field {
            self.fields[&id].read(cx).focus_input(window);
        } else {
            window.focus(&self.buttons[&id]);
        }
        self.reveal_control(id, window, cx);
        cx.notify();
    }
    fn reveal_control(&self, id: u64, window: &mut Window, cx: &gpui::App) {
        let Some(focus) = self
            .fields
            .get(&id)
            .map(|field| field.read(cx).focus_handle())
            .or_else(|| self.buttons.get(&id).cloned())
        else {
            return;
        };
        let controls = self.control_bounds.clone();
        let scroll = self.body_scroll.clone();
        let pending = self.reveal_serial.clone();
        let serial = pending.get().wrapping_add(1);
        pending.set(serial);
        window.on_next_frame(move |window, _| {
            // Fast Tab or several requests in one frame must not each add an
            // offset computed from the same old bounds, or chase moved focus.
            if pending.get() != serial || !focus.is_focused(window) {
                return;
            }
            let Some(control) = controls.borrow().get(&id).copied() else {
                return;
            };
            let viewport = scroll.bounds();
            let delta = reveal_delta(
                f32::from(viewport.top()),
                f32::from(viewport.bottom()),
                f32::from(control.top()),
                f32::from(control.bottom()),
            );
            if delta != 0.0 {
                let mut offset = scroll.offset();
                offset.y += px(delta);
                scroll.set_offset(offset);
                window.refresh();
            }
        });
    }
    fn measure_controls(
        &self,
        ids: Vec<u64>,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut gpui::App) + 'static + use<> {
        let controls = self.control_bounds.clone();
        move |bounds, _, _| {
            let mut controls = controls.borrow_mut();
            for (id, bounds) in ids.iter().copied().zip(bounds) {
                if id != 0 {
                    controls.insert(id, bounds);
                }
            }
        }
    }
    fn respond(&mut self, response: UiResponse, cx: &mut Context<Self>) {
        let Some(session) = self.state.read(cx).automation.as_ref() else {
            return;
        };
        let (operation, revision) = (session.operation, session.revision);
        self.state.update(cx, |s, cx| {
            s.automation_response(operation, revision, response, cx)
        });
    }
    fn key_up(&mut self, event: &KeyUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.activation_keys.release(event.keystroke.key.as_str());
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let fresh_activation = self.activation_keys.press(key, event.is_held);
        if modifiers.alt && key == "f4" {
            return;
        }
        if ScriptTextInput::active_is_composing(window, cx) {
            return;
        }
        if key == "tab" && !modifiers.control && !modifiers.alt && !modifiers.platform {
            cx.stop_propagation();
            window.prevent_default();
            let focused = self
                .focused_control(window, cx)
                .and_then(|control| self.tab_order.iter().position(|item| *item == control));
            if !self.tab_order.is_empty() {
                let count = self.tab_order.len();
                let next = match focused {
                    Some(i) if modifiers.shift => (i + count - 1) % count,
                    Some(i) => (i + 1) % count,
                    None if modifiers.shift => count - 1,
                    None => 0,
                };
                let (id, field) = self.tab_order[next];
                self.focus_control(id, field, window, cx);
            }
            return;
        }
        if modifiers.control || modifiers.platform || modifiers.alt {
            return;
        }
        if !fresh_activation && key == "escape" {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if matches!(key, "enter" | "space")
            && self
                .buttons
                .get(&CANCEL_SCRIPT)
                .is_some_and(|focus| focus.is_focused(window))
        {
            cx.stop_propagation();
            window.prevent_default();
            if fresh_activation {
                self.state.update(cx, |s, cx| {
                    s.cancel_automation();
                    cx.notify();
                });
            }
            return;
        }
        let request = self
            .state
            .read(cx)
            .automation
            .as_ref()
            .and_then(|s| s.request.clone());
        let response = match (request, key) {
            (
                Some(UiRequest::Dialog {
                    id,
                    cancel_element,
                    root,
                    ..
                }),
                "escape",
            ) => match cancel_element {
                Some(control_id) => {
                    libre_effects_editor_model::automation_ui::active_control(&root, control_id)
                        .map(|_| UiResponse::Click {
                            dialog_id: id,
                            control_id,
                        })
                }
                None => Some(UiResponse::Close { dialog_id: id }),
            },
            (
                Some(UiRequest::Dialog {
                    id,
                    default_element,
                    root,
                    ..
                }),
                "enter" | "space",
            ) => {
                let button = self
                    .buttons
                    .iter()
                    .find(|(id, focus)| {
                        focus.is_focused(window)
                            && libre_effects_editor_model::automation_ui::active_control(
                                &root, **id,
                            )
                            .is_some()
                    })
                    .map(|(id, _)| *id);
                let default_element = default_element.filter(|id| {
                    libre_effects_editor_model::automation_ui::active_control(&root, *id).is_some()
                });
                let multiline = self.tab_order.iter().any(|(id, field)| {
                    *field
                        && self
                            .fields
                            .get(id)
                            .is_some_and(|f| f.read(cx).has_focus(window) && f.read(cx).multiline())
                });
                let text_focused = self.tab_order.iter().any(|(id, field)| {
                    *field
                        && self
                            .fields
                            .get(id)
                            .is_some_and(|f| f.read(cx).has_focus(window))
                });
                if key == "enter" && !multiline {
                    button
                        .or(default_element)
                        .map(|control_id| UiResponse::Click {
                            dialog_id: id,
                            control_id,
                        })
                } else if key == "space" && !text_focused {
                    button.map(|control_id| UiResponse::Click {
                        dialog_id: id,
                        control_id,
                    })
                } else {
                    None
                }
            }
            (Some(UiRequest::Alert { .. }), "enter" | "space" | "escape") => {
                Some(UiResponse::AlertDismissed)
            }
            (Some(UiRequest::Confirm { .. }), "enter" | "space") => Some(UiResponse::Confirm {
                value: !self
                    .buttons
                    .get(&DIALOG_NO)
                    .is_some_and(|focus| focus.is_focused(window)),
            }),
            (Some(UiRequest::Confirm { .. }), "escape") => {
                Some(UiResponse::Confirm { value: false })
            }
            (None, "escape") => {
                self.state.update(cx, |s, cx| {
                    s.cancel_automation();
                    cx.notify();
                });
                None
            }
            _ => None,
        };
        if let Some(response) = response {
            cx.stop_propagation();
            window.prevent_default();
            if fresh_activation {
                self.respond(response, cx);
            }
        }
    }
    fn node(
        &mut self,
        node: &UiNode,
        dialog: u64,
        operation: u64,
        revision: u64,
        enabled: bool,
        parent_row: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if !node.visible {
            return div().into_any_element();
        }
        let enabled = enabled && node.enabled;
        let id = node.id;
        match node.kind.as_str() {
            "edittext" => {
                let size = ControlSize::new(
                    node.minimum_size,
                    node.preferred_size,
                    if node.multiline { 112.0 } else { 24.0 },
                    self.control_space,
                );
                let wrapper = div().min_w_0().max_w_full();
                let wrapper = match size.edit_width(parent_row, self.control_space[0]) {
                    EditWidth::Fill => wrapper.flex_none().w_full(),
                    EditWidth::Flexible { minimum } => wrapper.flex_1().min_w(px(minimum)),
                    EditWidth::Requested(width) => wrapper.flex_none().w(px(width)),
                };
                if !enabled {
                    if let Some(field) = self.fields.get(&id) {
                        if field.read(cx).has_focus(window) {
                            window.focus(&self.focus);
                        }
                    }
                    return wrapper
                        .h(px(size.height))
                        .overflow_hidden()
                        .p_1()
                        .bg(rgb(0x181818))
                        .opacity(0.5)
                        .child(node.text.clone())
                        .into_any_element();
                }
                if !self.fields.contains_key(&id) {
                    let state = self.state.clone();
                    let field = cx.new(|cx| {
                        ScriptTextInput::new(cx, node.multiline, move |text, _, cx| {
                            let state = state.clone();
                            let text = text.to_string();
                            state.update(cx, |s, cx| {
                                if let Some(session) =
                                    s.automation.as_ref().filter(|s| s.operation == operation)
                                {
                                    let revision = session.revision;
                                    s.automation_response(
                                        operation,
                                        revision,
                                        UiResponse::Change {
                                            dialog_id: dialog,
                                            control_id: id,
                                            text,
                                        },
                                        cx,
                                    );
                                }
                            });
                        })
                    });
                    self.fields.insert(id, field);
                }
                let field = self.fields[&id].clone();
                field.update(cx, |field, _| {
                    field.set_height(size.height);
                    field.sync_revision(
                        format!("script-{operation}-{dialog}-{id}"),
                        node.text.clone(),
                        self.sync_revision,
                        window,
                    )
                });
                if enabled {
                    self.tab_order.push((id, true));
                }
                wrapper.child(field).into_any_element()
            }
            "button" => {
                let size = ControlSize::new(
                    node.minimum_size,
                    node.preferred_size,
                    25.0,
                    self.control_space,
                );
                let focus = self
                    .buttons
                    .entry(id)
                    .or_insert_with(|| cx.focus_handle())
                    .clone();
                if enabled {
                    self.tab_order.push((id, false));
                }
                let state = self.state.clone();
                let pending = self
                    .state
                    .read(cx)
                    .automation
                    .as_ref()
                    .is_some_and(|s| s.action_pending);
                ui::text_button(
                    SharedString::from(format!("script-button-{id}")),
                    node.text.clone(),
                )
                .track_focus(&focus)
                .flex_none()
                .min_w(px(65.0_f32.min(self.control_space[0])))
                .max_w_full()
                .when_some(size.width, |d, width| d.min_w_0().w(px(width)))
                .h(px(size.height))
                .overflow_hidden()
                .border_1()
                .border_color(rgb(0x555555))
                .rounded_sm()
                .px_3()
                .py_1()
                .when(!enabled || pending, |d| d.opacity(0.45))
                .on_click(move |event, window, cx| {
                    if !pointer_click(event) {
                        return;
                    }
                    if !enabled || pending || ScriptTextInput::active_is_composing(window, cx) {
                        return;
                    }
                    state.update(cx, |s, cx| {
                        s.automation_response(
                            operation,
                            revision,
                            UiResponse::Click {
                                dialog_id: dialog,
                                control_id: id,
                            },
                            cx,
                        )
                    });
                })
                .into_any_element()
            }
            "statictext" => div()
                .when(!parent_row, |d| d.flex_none())
                .min_w_0()
                .text_size(px(12.0))
                .child(node.text.clone())
                .into_any_element(),
            _ => {
                let panel = node.kind == "panel";
                let title = panel && !node.text.is_empty();
                let ids = title
                    .then_some(0)
                    .into_iter()
                    .chain(node.children.iter().map(|child| child.id))
                    .collect();
                let mut group = div()
                    .on_children_prepainted(self.measure_controls(ids))
                    .when(!parent_row, |d| d.flex_none())
                    .max_w_full()
                    .min_w_0()
                    .flex()
                    .gap_2()
                    .when(node.orientation != "row", |d| d.flex_col())
                    .when(node.orientation == "row", |d| d.flex_wrap())
                    .when(panel, |d| {
                        d.border_1().border_color(rgb(0x494949)).rounded_sm().p_2()
                    });
                if title {
                    group = group.child(div().text_color(rgb(ui::BLUE)).child(node.text.clone()));
                }
                for child in &node.children {
                    group = group.child(self.node(
                        child,
                        dialog,
                        operation,
                        revision,
                        enabled,
                        node.orientation == "row",
                        window,
                        cx,
                    ));
                }
                group.into_any_element()
            }
        }
    }
}
impl Render for ScriptUi {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(session) = self.state.read(cx).automation.as_ref() else {
            return div().into_any_element();
        };
        let (operation, revision, name, request, acknowledged, pending) = (
            session.operation,
            session.revision,
            session.name.clone(),
            session.request.clone(),
            session.inputs_acknowledged(),
            session.action_pending,
        );
        if self.operation != Some(operation) {
            self.operation = Some(operation);
            self.dialog_key = None;
            self.viewport = None;
            self.activation_keys.clear();
            self.fields.clear();
            self.buttons.clear();
            self.tab_order.clear();
            self.dialog_focus.clear();
            self.focus_requests.clear();
            self.body_scroll = ScrollHandle::new();
            self.control_bounds = Rc::default();
            self.reveal_serial = Rc::default();
        }
        if acknowledged {
            self.sync_revision = revision;
        }
        let viewport = (
            f32::from(window.viewport_size().width),
            f32::from(window.viewport_size().height),
        );
        let resized = self.viewport.is_some_and(|previous| previous != viewport);
        if resized {
            if let Some(UiRequest::Dialog {
                id,
                resizable: true,
                ..
            }) = &request
            {
                if acknowledged && !pending {
                    self.respond(UiResponse::Resize { dialog_id: *id }, cx);
                }
            }
        }
        self.viewport = Some(viewport);
        let (minimum, preferred) = match &request {
            Some(UiRequest::Dialog { root, .. }) => (root.minimum_size, root.preferred_size),
            _ => (None, None),
        };
        let mut size = DialogSize::new(minimum, preferred, [viewport.0, viewport.1]);
        let keyboard_warning = crate::modal_keyboard::warning(cx);
        if let Some(message) = keyboard_warning {
            let notice_height = window
                .text_system()
                .shape_text(
                    message.into(),
                    px(11.0),
                    &[window.text_style().to_run(message.len())],
                    Some(px((size.width - 34.0).max(1.0))),
                    None,
                )
                .map(|lines| {
                    lines
                        .iter()
                        .map(|line| (line.wrap_boundaries.len() + 1) as f32 * 16.0)
                        .sum()
                })
                .unwrap_or(size.maximum_height);
            size.reserve_notice(notice_height + 12.0);
        }
        self.control_space = size.controls;
        let key = match &request {
            Some(UiRequest::Dialog { id, .. }) => (*id, 0),
            Some(UiRequest::Alert { .. }) => (revision, 1),
            Some(UiRequest::Confirm { .. }) => (revision, 2),
            None => (0, 3),
        };
        let new_dialog = self.dialog_key != Some(key);
        if new_dialog {
            // A nested alert/confirm suspends its parent's focus. Restoring it
            // must not replay a persistent active=true from an older callback.
            if let Some((dialog, 0)) = self.dialog_key {
                if let Some(control) = self.focused_control(window, cx) {
                    self.dialog_focus.insert(dialog, control);
                }
            }
            self.dialog_key = Some(key);
            cx.stop_active_drag(window);
        }
        if new_dialog || !self.focus.contains_focused(window, cx) {
            window.focus(&self.focus);
        }
        self.tab_order.clear();
        let mut title = "Running script…".to_string();
        let mut body = div()
            .id("scriptui-scroll")
            .track_scroll(&self.body_scroll)
            .overflow_y_scroll()
            .min_h_0()
            .max_h(px(size.body_height))
            .flex_shrink()
            .flex_grow()
            .flex()
            .flex_col()
            .gap_3()
            .p_1();
        let mut requested_focus = None;
        match request {
            Some(UiRequest::Dialog {
                id,
                title: dialog_title,
                root,
                ..
            }) => {
                title = dialog_title;
                if acknowledged {
                    let mut requests = Vec::new();
                    collect_focus_requests(&root, true, &mut requests);
                    requested_focus = self.focus_requests.entry(id).or_default().take(requests);
                }
                body =
                    body.child(self.node(&root, id, operation, revision, true, false, window, cx));
                let mut live = BTreeSet::new();
                collect_ids(&root, &mut live);
                self.fields.retain(|id, _| live.contains(id));
                self.buttons.retain(|id, _| live.contains(id));
                self.control_bounds
                    .borrow_mut()
                    .retain(|id, _| live.contains(id));
            }
            Some(UiRequest::Alert { message }) => {
                let focus = self
                    .buttons
                    .entry(DIALOG_YES)
                    .or_insert_with(|| cx.focus_handle())
                    .clone();
                self.tab_order.push((DIALOG_YES, false));
                title = "Script message".into();
                body = body.child(message).child(
                    div()
                        .on_children_prepainted(self.measure_controls(vec![DIALOG_YES]))
                        .flex_none()
                        .child(
                            ui::text_button("script-alert-ok", "OK")
                                .track_focus(&focus)
                                .when(pending, |d| d.opacity(0.5))
                                .on_click(cx.listener(|this, event, _, cx| {
                                    if pointer_click(event) {
                                        this.respond(UiResponse::AlertDismissed, cx);
                                    }
                                })),
                        ),
                );
            }
            Some(UiRequest::Confirm { message }) => {
                let yes = self
                    .buttons
                    .entry(DIALOG_YES)
                    .or_insert_with(|| cx.focus_handle())
                    .clone();
                let no = self
                    .buttons
                    .entry(DIALOG_NO)
                    .or_insert_with(|| cx.focus_handle())
                    .clone();
                self.tab_order
                    .extend([(DIALOG_YES, false), (DIALOG_NO, false)]);
                title = "Script confirmation".into();
                body = body.child(message).child(
                    div()
                        .on_children_prepainted(self.measure_controls(vec![DIALOG_YES, DIALOG_NO]))
                        .flex_none()
                        .flex()
                        .gap_3()
                        .child(
                            ui::text_button("script-confirm-yes", "Yes")
                                .track_focus(&yes)
                                .on_click(cx.listener(|this, event, _, cx| {
                                    if pointer_click(event) {
                                        this.respond(UiResponse::Confirm { value: true }, cx);
                                    }
                                })),
                        )
                        .child(
                            ui::text_button("script-confirm-no", "No")
                                .track_focus(&no)
                                .on_click(cx.listener(|this, event, _, cx| {
                                    if pointer_click(event) {
                                        this.respond(UiResponse::Confirm { value: false }, cx);
                                    }
                                })),
                        ),
                );
            }
            None => {
                body = body.child("Project changes are isolated until the script succeeds.");
            }
        }
        let cancel_focus = self
            .buttons
            .entry(CANCEL_SCRIPT)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        self.tab_order.push((CANCEL_SCRIPT, false));
        // Removed, hidden or disabled descendants cannot retain a native IME
        // target behind the modal or strand the keyboard outside its focus tree.
        let retired_field = self.fields.iter().any(|(id, field)| {
            field.read(cx).has_focus(window) && !self.tab_order.contains(&(*id, true))
        });
        let retired_button = self
            .buttons
            .iter()
            .any(|(id, focus)| focus.is_focused(window) && !self.tab_order.contains(&(*id, false)));
        if retired_field || retired_button {
            window.focus(&self.focus);
        }
        let requested_focus = requested_focus.and_then(|requested| {
            self.tab_order
                .iter()
                .copied()
                .find(|(id, _)| *id == requested)
        });
        let restored_focus = (new_dialog && key.1 == 0)
            .then(|| self.dialog_focus.get(&key.0).copied())
            .flatten()
            .filter(|control| self.tab_order.contains(control));
        let initial_focus = new_dialog
            .then(|| self.tab_order.iter().copied().find(|(_, field)| *field))
            .flatten();
        if let Some((id, field)) = requested_focus.or(restored_focus).or(initial_focus) {
            self.focus_control(id, field, window, cx);
        } else if resized {
            if let Some((id, _)) = self.focused_control(window, cx) {
                self.reveal_control(id, window, cx);
            }
        }
        div()
            .id("scriptui-dialog")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::key))
            .capture_key_up(cx.listener(Self::key_up))
            .on_key_down(|_, window, cx| {
                if !ScriptTextInput::active_has_focus(window, cx) {
                    cx.stop_propagation();
                }
            })
            .w(px(size.width))
            .min_h(px(size.minimum_height))
            .when_some(size.height, |d, height| d.h(px(height)))
            .max_h(px(size.maximum_height))
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(0x555555))
            .rounded_md()
            .shadow_lg()
            .child(div().flex_none().text_size(px(16.0)).child(title))
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(ui::MUTED))
                    .text_size(px(11.0))
                    .child(format!(
                        "{name} · ScriptUI / bounded native scripting preview"
                    )),
            )
            .children(keyboard_warning.map(|message| {
                div()
                    .flex_none()
                    .text_color(rgb(ui::MUTED))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .child(message)
            }))
            .child(body)
            .child(
                div().flex_none().flex().justify_end().child(
                    ui::text_button("script-cancel-all", "Cancel script")
                        .track_focus(&cancel_focus)
                        .on_click(cx.listener(|this, event, window, cx| {
                            if !pointer_click(event) {
                                return;
                            }
                            this.state.update(cx, |s, cx| {
                                s.cancel_automation();
                                cx.notify();
                            });
                            window.blur();
                        })),
                ),
            )
            .into_any_element()
    }
}
fn collect_ids(node: &UiNode, ids: &mut BTreeSet<u64>) {
    ids.insert(node.id);
    for child in &node.children {
        collect_ids(child, ids);
    }
}
fn collect_focus_requests(node: &UiNode, parent: bool, requests: &mut Vec<(u64, u64, bool)>) {
    let enabled = parent && node.visible && node.enabled;
    requests.push((
        node.focus_request,
        node.id,
        enabled && node.active && matches!(node.kind.as_str(), "edittext" | "button"),
    ));
    for child in &node.children {
        collect_focus_requests(child, enabled, requests);
    }
}

fn pointer_click(event: &gpui::ClickEvent) -> bool {
    matches!(event, gpui::ClickEvent::Mouse(click) if !click.down.first_mouse)
}
