//! Numeric editing stays in a private, serial-bound path transaction.
use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, FocusHandle, KeyDownEvent, Window, div, prelude::*, px, rgb};
use libre_effects_core::PathTarget;

pub(crate) struct VertexEditor {
    state: Entity<EditorState>,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    fields: Vec<Entity<TextField>>,
    serial: Option<u64>,
    watches: Option<Vec<gpui::Subscription>>,
}
impl VertexEditor {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            this.state.update(cx, |s, cx| {
                if s.invalidate_vertex_editor() {
                    cx.notify();
                }
            });
            cx.notify();
        })
        .detach();
        Self {
            state,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            fields: vec![],
            serial: None,
            watches: None,
        }
    }

    fn make_fields(&self, serial: u64, cx: &mut Context<Self>) -> Vec<Entity<TextField>> {
        (0..6)
            .map(|index| {
                let state = self.state.clone();
                let focus = self.focus.clone();
                cx.new(|cx| {
                    // Deliberately use plain entry. The general numeric scrub rounds
                    // to two decimal places, which cannot preserve arbitrary vertices.
                    TextField::new(cx, move |text, _, cx| {
                        state.update(cx, |s, cx| {
                            s.vertex_input(serial, index, text);
                            cx.notify();
                        });
                    })
                    .return_focus(focus)
                })
            })
            .collect()
    }

    fn close(&mut self, serial: u64, accept: bool, w: &mut Window, cx: &mut Context<Self>) {
        if !matching_session(self.state.read(cx), serial) {
            return;
        }
        if accept {
            if TextField::is_composing(w, cx) {
                return;
            }
            if self.fields.iter().any(|field| field.read(cx).has_focus(w)) {
                TextField::commit_active(w, cx);
            }
        }
        // Commit/blur callbacks may have invalidated or replaced the session.
        if !matching_session(self.state.read(cx), serial) {
            return;
        }
        self.state.update(cx, |s, cx| {
            s.dispatch(
                &if accept {
                    Action::ApplyVertex
                } else {
                    Action::CancelVertex
                },
                w,
                cx,
            );
        });
        if self.state.read(cx).vertex_editor.is_none() {
            // Preview owns the only allowed return: a validated one-shot request.
            // Never resurrect an old canvas selection with an arbitrary focus handle.
            w.blur();
        }
        cx.notify();
    }

    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        if matches!(key, "escape" | "enter" | "tab") && TextField::is_composing(w, cx) {
            cx.stop_propagation();
            return;
        }
        if (e.keystroke.modifiers.control || e.keystroke.modifiers.platform)
            && matches!(key, "s" | "o" | "n")
        {
            // TextField normally blurs for document shortcuts; retain modal focus.
            cx.stop_propagation();
            w.prevent_default();
            return;
        }
        let Some(serial) = self.serial else {
            return;
        };
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
                .vertex_editor
                .as_ref()
                .is_some_and(|s| s.has_input_error(index));
            if !pending && !rejected {
                return;
            }
            let text = self.fields[index].read(cx).value().to_owned();
            self.state.update(cx, |s, cx| {
                s.vertex_input(serial, index, &text);
                cx.notify();
            });
            if self
                .state
                .read(cx)
                .vertex_editor
                .as_ref()
                .is_some_and(|s| s.has_input_error(index))
            {
                // Keep rejected text visible and focused. Another invalid field
                // remains invalid when this one is corrected or reverted.
                cx.stop_propagation();
                w.prevent_default();
            }
            return;
        }
        if key == "escape" {
            if let Some(index) = focused_field {
                let reset = self.state.update(cx, |s, cx| {
                    let reset = s.revert_vertex_field(serial, index);
                    cx.notify();
                    reset
                });
                if let Some(value) = reset {
                    self.fields[index].update(cx, |field, _| {
                        field.sync(format!("vertex-reverted-{serial}-{index}"), value, w)
                    });
                    w.focus(&self.focus);
                    cx.stop_propagation();
                    w.prevent_default();
                }
                // Otherwise TextField restores its unsubmitted text. A subsequent
                // Escape from dialog focus cancels the complete transaction.
                return;
            }
            self.close(serial, false, w, cx);
        } else if key == "tab" {
            if focused_field.is_some() {
                TextField::commit_active(w, cx);
            }
            if e.keystroke.modifiers.shift {
                w.focus_prev();
            } else {
                w.focus_next();
            }
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
}

fn matching_session(state: &EditorState, serial: u64) -> bool {
    state
        .vertex_editor
        .as_ref()
        .is_some_and(|s| s.id == serial && s.current(state))
}

fn path_label(target: PathTarget) -> String {
    match target {
        PathTarget::Shape => "Shape path".into(),
        PathTarget::Contents(item) => format!("Contents path {item}"),
        PathTarget::Mask(mask) => format!("Mask path {mask}"),
    }
}

impl Render for VertexEditor {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.watches.is_none() {
            self.watches = Some(vec![
                cx.observe_window_activation(w, |this, w, cx| {
                    if !w.is_window_active() {
                        this.state.update(cx, |s, cx| {
                            s.discard_vertex_editor();
                            cx.notify();
                        });
                        this.serial = None;
                    }
                }),
                cx.on_focus_out(&self.focus.clone(), w, |this, _, w, cx| {
                    if this.state.read(cx).vertex_editor.is_some()
                        && w.is_window_active()
                        && !this.focus.contains_focused(w, cx)
                    {
                        w.focus(&this.focus);
                    }
                }),
            ]);
        }
        let mut root = div()
            .id("vertex-editor-dialog")
            .track_focus(&self.focus)
            .tab_index(0)
            .w(px(540.))
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
            .capture_key_down(cx.listener(Self::key))
            .on_key_down(cx.listener(|_, _, _, cx| cx.stop_propagation()));
        let Some(id) = self.state.read(cx).vertex_editor.as_ref().map(|s| s.id) else {
            self.serial = None;
            return root;
        };
        if self.serial != Some(id) {
            self.serial = Some(id);
            self.fields = self.make_fields(id, cx);
            w.focus(&self.focus);
        }
        let session = self.state.read(cx).vertex_editor.as_ref().unwrap();
        let descriptor = format!(
            "{} · Layer {} · Vertex {} · Frame {}",
            path_label(session.target),
            session.layer,
            session.index + 1,
            session.frame
        );
        let values: Vec<_> = (0..6).map(|i| session.field_value(i).unwrap()).collect();
        let rejected: Vec<_> = (0..6).map(|i| session.has_input_error(i)).collect();
        let error = session.error.clone();
        root = root
            .child(div().text_size(px(16.)).child("Edit Vertex"))
            .child(div().text_size(px(11.)).text_color(rgb(ui::MUTED)).child(descriptor))
            .child(div().text_size(px(11.)).text_color(rgb(ui::MUTED)).child(
                "Path-local pixels (px). Incoming and outgoing offsets are relative to the anchor; handles edit independently.",
            ));
        for (row, label) in ["Anchor", "Incoming offset", "Outgoing offset"]
            .into_iter()
            .enumerate()
        {
            let mut controls = div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w(px(118.)).child(label));
            for axis in 0..2 {
                let index = row * 2 + axis;
                if !rejected[index] {
                    self.fields[index].update(cx, |field, _| {
                        field.sync(format!("vertex-{id}-{index}"), values[index].clone(), w)
                    });
                }
                controls = controls.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .child(if axis == 0 {
                            "X (local px)"
                        } else {
                            "Y (local px)"
                        })
                        .child(self.fields[index].clone()),
                );
            }
            root = root.child(controls);
        }
        root.child(div().text_size(px(11.)).text_color(rgb(ui::MUTED)).child(
                "Enter or Tab previews each value. Animated paths update at this frame; static paths stay static. Values must be finite and between −1,000,000 and 1,000,000.",
            ))
            .child(div().text_color(rgb(0xffaa88)).child(error))
            .child(div().flex().gap_2().justify_end()
                .child(ui::text_button("accept-vertex", "OK")
                    .on_click(cx.listener(move |this, _, w, cx| this.close(id, true, w, cx))))
                .child(ui::text_button("cancel-vertex", "Cancel")
                    .track_focus(&self.cancel_focus)
                    .on_click(cx.listener(move |this, _, w, cx| this.close(id, false, w, cx)))))
            .child(div().text_size(px(10.)).text_color(rgb(ui::MUTED)).child(
                "OK applies one Undo step. Esc reverts the focused field; Esc again cancels the draft.",
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Tool;
    use crate::panels::vertex_editor::{Request, Session};
    use libre_effects_core::{Command, Content, PathVertex, Shape, VectorPath};

    fn opened() -> EditorState {
        let mut state = EditorState::default();
        state.tool = Tool::Pen;
        let path = VectorPath {
            closed: false,
            vertices: vec![
                PathVertex::corner([0.123456789012345, 20.]),
                PathVertex::corner([120., 40.]),
            ],
        };
        state
            .editor
            .execute(Command::AddContent {
                content: Content::Shape(Shape {
                    path: Some(path.clone()),
                    fill: false,
                    ..Default::default()
                }),
                width: 256.,
                height: 256.,
                name: "Vertex routing".into(),
            })
            .unwrap();
        state.editor.clear_history();
        let world = state
            .editor
            .project()
            .composition()
            .world_transform(1, state.frame)
            .unwrap();
        let request = Request::new(&state, 1, PathTarget::Shape, 0, path, world).unwrap();
        state.vertex_editor = Some(Session::new(&state, request).unwrap());
        state
    }

    #[test]
    fn vertex_dialog_closures_require_the_exact_current_session() {
        let mut state = opened();
        let first = state.vertex_editor.as_ref().unwrap().id;
        assert!(matching_session(&state, first));
        assert!(!matching_session(&state, first.wrapping_add(1)));
        state.cancel_vertex_editor();
        assert!(!matching_session(&state, first));
        let request = state.vertex_return.take().unwrap();
        state.vertex_editor = Some(Session::new(&state, request).unwrap());
        let second = state.vertex_editor.as_ref().unwrap().id;
        assert_ne!(first, second);
        assert!(!matching_session(&state, first));
        assert!(matching_session(&state, second));
        // A queued callback from the old field cannot alter the reopened draft.
        let before = state.vertex_editor.as_ref().unwrap().project().clone();
        state.vertex_input(first, 0, "300");
        assert_eq!(state.vertex_editor.as_ref().unwrap().project(), &before);
        state.frame += 1;
        assert!(!matching_session(&state, second));
    }

    #[test]
    fn vertex_window_close_discards_before_late_field_submit_without_restoring() {
        let mut state = opened();
        let source = state.editor.project().clone();
        let serial = state.vertex_editor.as_ref().unwrap().id;
        state.vertex_input(serial, 0, "300");
        state.vertex_return = Some(state.vertex_editor.as_ref().unwrap().request().clone());
        state.discard_vertex_editor();
        assert!(state.vertex_editor.is_none());
        assert!(state.vertex_return.is_none());
        assert!(!matching_session(&state, serial));
        state.vertex_input(serial, 0, "500");
        state.accept_vertex_editor();
        state.cancel_vertex_editor();
        assert_eq!(state.editor.project(), &source);
        assert!(state.vertex_return.is_none());
        assert!(!state.editor.can_undo());
        assert!(!state.editor.can_redo());
    }

    #[test]
    fn vertex_dialog_describes_each_explicit_path_kind() {
        assert_eq!(path_label(PathTarget::Shape), "Shape path");
        assert_eq!(path_label(PathTarget::Contents(12)), "Contents path 12");
        assert_eq!(path_label(PathTarget::Mask(3)), "Mask path 3");
    }
}
