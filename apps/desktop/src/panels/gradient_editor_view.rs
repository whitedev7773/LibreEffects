use super::Session;
use crate::panels::contents::gradient_ramp::{self as ramp, Handle};
use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, Pixels,
    Window, canvas, div, prelude::*, px, rgb,
};
use libre_effects_core::{GradientParam, ShapeGradient};
use std::{cell::Cell, rc::Rc};

struct Drag {
    handle: Handle,
    offset: f64,
    bounds: Bounds<Pixels>,
}
pub(crate) struct GradientEditor {
    state: Entity<EditorState>,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    fields: Vec<Entity<TextField>>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    drag: Option<Drag>,
    serial: Option<u64>,
    watches: Option<Vec<gpui::Subscription>>,
}
impl GradientEditor {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            this.state.update(cx, |s, cx| {
                if s.invalidate_gradient_editor() {
                    cx.notify();
                }
            });
            if this.state.read(cx).gradient_editor.is_none() {
                this.drag = None;
            }
            cx.notify();
        })
        .detach();
        let focus = cx.focus_handle();
        Self {
            state,
            focus,
            cancel_focus: cx.focus_handle(),
            return_focus: None,
            fields: vec![],
            bounds: Default::default(),
            drag: None,
            serial: None,
            watches: None,
        }
    }
    fn make_fields(&self, serial: u64, cx: &mut Context<Self>) -> Vec<Entity<TextField>> {
        (0..7)
            .map(|index| {
                let state = self.state.clone();
                let focus = self.focus.clone();
                cx.new(|cx| {
                    let field = TextField::new(cx, move |text, _, cx| {
                        state.update(cx, |s, cx| {
                            s.gradient_input(serial, index, text);
                            cx.notify();
                        });
                    })
                    .return_focus(focus);
                    if index == 3 {
                        field
                    } else if index >= 4 {
                        field.integer()
                    } else {
                        field.numeric()
                    }
                })
            })
            .collect()
    }
    fn change(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Session) -> Result<(), String>,
    ) {
        self.state.update(cx, |s, cx| {
            s.invalidate_gradient_editor();
            if let Some(session) = &mut s.gradient_editor
                && let Err(error) = edit(session)
            {
                session.error = error;
            }
            cx.notify();
        });
        cx.notify();
    }
    fn close(&mut self, accept: bool, w: &mut Window, cx: &mut Context<Self>) {
        self.drag = None;
        if accept
            && TextField::is_composing(w, cx)
            && self
                .state
                .read(cx)
                .gradient_editor
                .as_ref()
                .is_some_and(|s| s.compound)
        {
            return;
        }
        if accept {
            TextField::commit_active(w, cx);
        }
        self.state.update(cx, |s, cx| {
            s.dispatch(
                &if accept {
                    Action::ApplyGradient
                } else {
                    Action::CancelGradient
                },
                w,
                cx,
            )
        });
        if self.state.read(cx).gradient_editor.is_none() {
            if let Some(focus) = self.return_focus.take() {
                w.focus(&focus);
            } else {
                w.blur();
            }
        }
        cx.notify();
    }
    fn down(&mut self, e: &MouseDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        TextField::commit_active(w, cx);
        w.focus(&self.focus);
        let Some(bounds) = self.bounds.get() else {
            return;
        };
        let Some(session) = &self.state.read(cx).gradient_editor else {
            return;
        };
        if let Some(handle) = ramp::hit(
            &ramp::handles(session.node(), session.frame, None),
            bounds,
            e.position,
        ) {
            self.drag = Some(Drag {
                handle,
                offset: ramp::position(bounds, e.position.x) - handle.position,
                bounds,
            });
            self.change(cx, |s| s.select(handle.parameter));
        } else {
            let y = f32::from(e.position.y - bounds.top());
            if (0. ..=16.).contains(&y) || (48. ..=64.).contains(&y) {
                self.change(cx, |s| {
                    let gradient = s.node().kind.gradient().unwrap();
                    let count = if y < 20. {
                        gradient.opacities.len()
                    } else {
                        gradient.colors.len()
                    };
                    if count >= ShapeGradient::MAX_STOPS {
                        return Ok(());
                    }
                    s.add(
                        y < 20.,
                        ramp::position(bounds, e.position.x).clamp(0., 100.),
                    )
                });
            }
        }
        cx.stop_propagation();
    }
    fn moving(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(drag) = &self.drag else {
            return;
        };
        if Some(drag.bounds) != self.bounds.get() {
            self.drag = None;
            return;
        }
        let parameter = drag.handle.parameter;
        let value = drag
            .handle
            .value(ramp::position(drag.bounds, x) - drag.offset);
        self.change(cx, |s| s.set_value(parameter, value));
    }
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        if TextField::is_composing(w, cx)
            && (matches!(key, "escape" | "enter")
                || key == "tab"
                    && self
                        .state
                        .read(cx)
                        .gradient_editor
                        .as_ref()
                        .is_some_and(|s| s.compound))
        {
            // Native IME owns confirmation/cancellation while marked text exists.
            // Do not submit the field or dismiss the modal from the same key event.
            cx.stop_propagation();
            return;
        }
        if (e.keystroke.modifiers.control || e.keystroke.modifiers.platform)
            && matches!(key, "s" | "o" | "n")
        {
            // TextField normally blurs for document shortcuts. A modal keeps focus.
            cx.stop_propagation();
            w.prevent_default();
            return;
        }
        let focused_field = self
            .fields
            .iter()
            .position(|field| field.read(cx).has_focus(w));
        if key == "enter"
            && let Some(index) = focused_field
        {
            let pending = self.fields[index].read(cx).has_pending_edit();
            let rejected = self
                .state
                .read(cx)
                .gradient_editor
                .as_ref()
                .is_some_and(|s| s.input_error == Some(index));
            if !pending && !rejected {
                return;
            }
            let text = self.fields[index].read(cx).value().to_owned();
            if let Some(id) = self.serial {
                self.state.update(cx, |s, cx| {
                    s.gradient_input(id, index, &text);
                    cx.notify();
                });
            }
            if self
                .state
                .read(cx)
                .gradient_editor
                .as_ref()
                .is_some_and(|s| s.input_error.is_some())
            {
                // Keep rejected text focused and visible; never submit or accept it.
                cx.stop_propagation();
                w.prevent_default();
            }
            return;
        }
        if key == "escape" {
            if let Some(index) = focused_field {
                let reset = self.state.update(cx, |s, cx| {
                    let reset = self
                        .serial
                        .and_then(|id| s.revert_gradient_field(id, index));
                    cx.notify();
                    reset
                });
                if let Some(value) = reset {
                    self.fields[index].update(cx, |field, _| {
                        field.sync("gradient-field-reverted".into(), value, w)
                    });
                    w.focus(&self.focus);
                    cx.stop_propagation();
                    w.prevent_default();
                }
                // First Escape restores this field; a subsequent Escape from the
                // ramp/dialog cancels the complete gradient transaction.
                return;
            }
            self.close(false, w, cx);
        } else if key == "tab" {
            self.drag = None;
            TextField::commit_active(w, cx);
            if e.keystroke.modifiers.shift {
                w.focus_prev();
            } else {
                w.focus_next();
            }
            // Wrap at the dialog boundary, including fields and disabled-state buttons.
            if !self.focus.contains_focused(w, cx) {
                w.focus(if e.keystroke.modifiers.shift {
                    &self.cancel_focus
                } else {
                    &self.focus
                });
            }
        } else {
            return;
        }
        cx.stop_propagation();
        w.prevent_default();
    }
    fn ramp_key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        self.drag = None;
        if e.keystroke.modifiers.control
            || e.keystroke.modifiers.alt
            || e.keystroke.modifiers.platform
        {
            return;
        }
        let Some(session) = &self.state.read(cx).gradient_editor else {
            return;
        };
        let all = ramp::handles(session.node(), session.frame, None);
        let selected = session.selected;
        let index = all
            .iter()
            .position(|h| h.parameter == selected)
            .unwrap_or(0);
        let step = if e.keystroke.modifiers.shift { 10. } else { 1. };
        if matches!(key, "up" | "down") {
            let next = if key == "up" {
                (index + all.len() - 1) % all.len()
            } else {
                (index + 1) % all.len()
            };
            self.change(cx, |s| s.select(all[next].parameter));
        } else if matches!(key, "left" | "right" | "home" | "end") {
            let (lo, hi) = selected.bounds();
            let value = match key {
                "home" => lo,
                "end" => hi,
                "left" => (session.value(selected) - step).clamp(lo, hi),
                _ => (session.value(selected) + step).clamp(lo, hi),
            };
            self.change(cx, |s| s.set_value(selected, value));
        } else if matches!(key, "delete" | "backspace") && all[index].span.is_none() {
            self.change(cx, |s| {
                let gradient = s.node().kind.gradient().unwrap();
                let count = if s.opacity() {
                    gradient.opacities.len()
                } else {
                    gradient.colors.len()
                };
                if count <= 2 { Ok(()) } else { s.remove() }
            });
        } else {
            return;
        }
        cx.stop_propagation();
    }
}
impl Render for GradientEditor {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.watches.is_none() {
            self.watches = Some(vec![
                cx.observe_window_activation(w, |this, w, cx| {
                    if !w.is_window_active() && this.state.read(cx).gradient_editor.is_some() {
                        this.close(false, w, cx);
                    }
                }),
                cx.on_focus_out(&self.focus.clone(), w, |this, _, w, cx| {
                    if this.state.read(cx).gradient_editor.is_some()
                        && w.is_window_active()
                        && !this.focus.contains_focused(w, cx)
                    {
                        w.focus(&this.focus);
                    }
                }),
            ]);
        }
        let mut root = div()
            .id("gradient-editor-dialog")
            .track_focus(&self.focus)
            .tab_index(0)
            .w(px(590.))
            .max_w_full()
            .max_h((w.viewport_size().height - px(32.)).clamp(px(180.), px(640.)))
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(ui::BLUE))
            .occlude()
            .capture_any_mouse_down(cx.listener(|this, _, w, cx| {
                if TextField::is_composing(w, cx)
                    && this
                        .state
                        .read(cx)
                        .gradient_editor
                        .as_ref()
                        .is_some_and(|s| s.compound)
                {
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .capture_key_down(cx.listener(Self::key))
            .on_key_down(cx.listener(|this, e, w, cx| {
                if this.focus.is_focused(w) {
                    this.ramp_key(e, w, cx);
                }
                cx.stop_propagation();
            }));
        let Some(id) = self.state.read(cx).gradient_editor.as_ref().map(|s| s.id) else {
            self.serial = None;
            return root;
        };
        if self.serial != Some(id) {
            self.serial = Some(id);
            self.return_focus = w.focused(cx);
            self.drag = None;
            self.fields = self.make_fields(id, cx);
            w.focus(&self.focus);
        }
        let session = self.state.read(cx).gradient_editor.as_ref().unwrap();
        let node = session.node();
        let gradient = node.kind.gradient().unwrap();
        let stop = session.selected.stop().unwrap();
        let opacity = session.opacity();
        let all = ramp::handles(node, session.frame, None);
        let selected = session.selected;
        let samples = gradient.preview(node, session.frame, 256);
        let error = session.error.clone();
        let input_error = session.input_error;
        let compound = session.compound;
        let selection_generation = session.selection_generation;
        let serial = session.id;
        let counts = [gradient.colors.len(), gradient.opacities.len()];
        let stops = [gradient.colors.clone(), gradient.opacities.clone()];
        let color = gradient.color_at(node, stop, session.frame);
        let position = session.value(if opacity {
            GradientParam::OpacityPosition(stop)
        } else {
            GradientParam::ColorPosition(stop)
        });
        let midpoint = session.value(if opacity {
            GradientParam::OpacityMidpoint(stop)
        } else {
            GradientParam::ColorMidpoint(stop)
        });
        let alpha = if opacity {
            session.value(GradientParam::Opacity(stop))
        } else {
            100.
        };
        let has_midpoint = all
            .iter()
            .any(|h| h.span.is_some() && h.parameter.stop() == Some(stop));
        let values = [
            session.number_text(position),
            session.number_text(midpoint),
            session.number_text(alpha),
            format!("{:06X}", color.unwrap_or(0)),
            format!("{}", color.unwrap_or(0) >> 16 & 255),
            format!("{}", color.unwrap_or(0) >> 8 & 255),
            format!("{}", color.unwrap_or(0) & 255),
        ];
        root = root
            .child(div().text_size(px(16.)).child("Gradient Editor"))
            .child(div().text_size(px(11.)).text_color(rgb(ui::MUTED)).child(
                "Composition previews this draft. OK applies all changes as one Undo step.",
            ));
        let bounds = self.bounds.clone();
        let owner = cx.entity();
        root = root
            .child(
                div()
                    .id("gradient-editor-ramp")
                    .cursor_crosshair()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::down))
                    .child(
                        canvas(
                            move |b, _, _| bounds.set(Some(b)),
                            move |b, _, w, _| {
                                ramp::paint_ramp(b, &samples, &all, Some(selected), w);
                                let moving = owner.clone();
                                w.on_mouse_event(move |e: &gpui::MouseMoveEvent, phase, _, cx| {
                                    if phase.bubble() && e.pressed_button == Some(MouseButton::Left)
                                    {
                                        moving.update(cx, |this, cx| this.moving(e.position.x, cx));
                                    }
                                });
                                let ending = owner.clone();
                                w.on_mouse_event(move |e: &gpui::MouseUpEvent, phase, _, cx| {
                                    if phase.bubble() && e.button == MouseButton::Left {
                                        ending.update(cx, |this, cx| {
                                            this.moving(e.position.x, cx);
                                            this.drag = None;
                                        });
                                    }
                                });
                            },
                        )
                        .w_full()
                        .h(px(64.)),
                    ),
            )
            .child(div().text_size(px(10.)).text_color(rgb(ui::MUTED)).child(
                "Top: opacity · Bottom: color · ◆ midpoint · Click an empty row to add a stop",
            ));
        for (row, ids) in stops.into_iter().enumerate() {
            let mut buttons =
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(if row == 0 { "Color" } else { "Opacity" });
            for id in ids {
                buttons = buttons.child(
                    ui::text_button(
                        gpui::SharedString::from(format!("modal-stop-{id}")),
                        id.to_string(),
                    )
                    .when(id == stop, |b| b.bg(rgb(0x164a7b)))
                    .on_click(cx.listener(move |this, _, w, cx| {
                        TextField::commit_active(w, cx);
                        this.drag = None;
                        this.change(cx, |s| {
                            s.select(if row == 0 {
                                GradientParam::ColorPosition(id)
                            } else {
                                GradientParam::OpacityPosition(id)
                            })
                        });
                        w.focus(&this.focus);
                    })),
                );
            }
            root = root.child(buttons);
        }
        let mut operations = div().flex().gap_2();
        for row in 0..2 {
            operations = operations.child(
                ui::text_button(
                    ("modal-add-stop", row),
                    if row == 0 {
                        "+ Color stop"
                    } else {
                        "+ Opacity stop"
                    },
                )
                .when(counts[row] >= ShapeGradient::MAX_STOPS, |b| b.opacity(0.4))
                .when(counts[row] < ShapeGradient::MAX_STOPS, |b| {
                    b.on_click(cx.listener(move |this, _, w, cx| {
                        TextField::commit_active(w, cx);
                        this.change(cx, |s| s.add(row == 1, 50.));
                        w.focus(&this.focus);
                    }))
                }),
            );
        }
        operations = operations.child(
            ui::text_button("modal-remove-stop", "Remove stop")
                .when(counts[usize::from(opacity)] <= 2, |b| b.opacity(0.4))
                .when(counts[usize::from(opacity)] > 2, |b| {
                    b.on_click(cx.listener(|this, _, w, cx| {
                        TextField::commit_active(w, cx);
                        this.change(cx, Session::remove);
                        w.focus(&this.focus);
                    }))
                }),
        );
        root = root.child(operations);
        if let Some(color) = color {
            root = root.child(
                div()
                    .h(px(20.))
                    .bg(rgb(color))
                    .border_1()
                    .border_color(rgb(ui::MUTED)),
            );
        }
        let mut fields = div().flex().flex_wrap().gap_2();
        for (index, label) in [
            "Location %",
            "Midpoint %",
            "Opacity %",
            "HEX",
            "Red",
            "Green",
            "Blue",
        ]
        .into_iter()
        .enumerate()
        {
            if (index == 1 && !has_midpoint) || (index == 2 && !opacity) || (index >= 3 && opacity)
            {
                continue;
            }
            if input_error != Some(index) {
                let state = self.state.clone();
                self.fields[index].update(cx, |f, _| {
                    let binding =
                        format!("gradient-{serial}-{stop}-{index}-{selection_generation}");
                    if compound {
                        f.sync_guarded(binding, values[index].clone(), w, move |text, _, cx| {
                            state.update(cx, |s, cx| {
                                let value = s.gradient_compound_input(
                                    serial,
                                    selection_generation,
                                    stop,
                                    index,
                                    text,
                                );
                                cx.notify();
                                value
                            })
                        });
                    } else {
                        f.sync(binding, values[index].clone(), w);
                    }
                });
            }
            fields = fields.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .w(px(if index == 3 { 115. } else { 100. }))
                    .child(label)
                    .child(self.fields[index].clone()),
            );
        }
        root = root.child(fields)
            .child(div().text_size(px(11.)).text_color(rgb(ui::MUTED)).child(
                if compound {
                    "Edits replace the complete Colors snapshot at this frame. Topology holds until the next key; other keys and endpoints stay unchanged. Each row supports 2–32 stops."
                } else {
                    "Existing animated channels get a value at the current frame; unanimated channels remain static. Adding or removing stops changes every frame. Each row supports 2–32 stops."
                }))
            .child(div().text_color(rgb(0xffaa88)).child(error))
            .child(div().flex().gap_2().justify_end()
                .child(ui::text_button("accept-gradient", "OK").on_click(cx.listener(|this, _, w, cx| this.close(true, w, cx))))
                .child(ui::text_button("cancel-gradient", "Cancel").track_focus(&self.cancel_focus)
                    .on_click(cx.listener(|this, _, w, cx| this.close(false, w, cx)))))
            .child(div().text_size(px(10.)).text_color(rgb(ui::MUTED)).child(
                "Ramp focus: Up/Down selects · Left/Right adjusts · Shift: 10 · Home/End · Delete removes · Esc cancels"));
        root
    }
}
