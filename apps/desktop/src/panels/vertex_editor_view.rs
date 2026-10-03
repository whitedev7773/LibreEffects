//! Numeric editing stays in a private, serial-bound path transaction.
use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, MouseUpEvent,
    Pixels, Point, Window, div, prelude::*, px, rgb,
};
use libre_effects_core::PathTarget;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy)]
struct ResetBounds {
    serial: u64,
    bounds: Bounds<Pixels>,
}
impl ResetBounds {
    fn hit(self, serial: u64, position: Point<Pixels>) -> bool {
        self.serial == serial && self.bounds.contains(&position)
    }
}

fn release_reset_press(
    press: &mut Option<u64>,
    serial: u64,
    bounds: Option<ResetBounds>,
    position: Point<Pixels>,
) -> Option<bool> {
    let pressed = press.take()?;
    Some(pressed == serial && bounds.is_some_and(|bounds| bounds.hit(serial, position)))
}

struct FieldRow {
    label: &'static str,
    fields: &'static [(usize, &'static str)],
}

const VERTEX_ROWS: &[FieldRow] = &[
    FieldRow {
        label: "Anchor",
        fields: &[(0, "X (local px)"), (1, "Y (local px)")],
    },
    FieldRow {
        label: "Incoming offset",
        fields: &[(2, "X (local px)"), (3, "Y (local px)")],
    },
    FieldRow {
        label: "Outgoing offset",
        fields: &[(4, "X (local px)"), (5, "Y (local px)")],
    },
];
const TRANSFORM_ROWS: &[FieldRow] = &[
    FieldRow {
        label: "Translation",
        fields: &[(0, "Delta X (local px)"), (1, "Delta Y (local px)")],
    },
    FieldRow {
        label: "Rotation",
        fields: &[(2, "Rotation (°)")],
    },
    FieldRow {
        label: "Scale",
        fields: &[(3, "Scale X (%)"), (4, "Scale Y (%)")],
    },
    FieldRow {
        label: "Pivot",
        fields: &[(5, "Pivot X (local px)"), (6, "Pivot Y (local px)")],
    },
];

fn field_rows(transform: bool) -> &'static [FieldRow] {
    if transform {
        TRANSFORM_ROWS
    } else {
        VERTEX_ROWS
    }
}

fn dialog_title(transform: bool) -> &'static str {
    if transform {
        "Transform Vertices"
    } else {
        "Edit Vertex"
    }
}

pub(crate) struct VertexEditor {
    state: Entity<EditorState>,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    fields: Vec<Entity<TextField>>,
    serial: Option<u64>,
    reset_bounds: Rc<Cell<Option<ResetBounds>>>,
    reset_press: Option<u64>,
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
            reset_bounds: Default::default(),
            reset_press: None,
            watches: None,
        }
    }

    fn make_fields(
        &self,
        serial: u64,
        count: usize,
        cx: &mut Context<Self>,
    ) -> Vec<Entity<TextField>> {
        (0..count)
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

    fn rebind_fields(&mut self, serial: u64, w: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self
            .state
            .read(cx)
            .vertex_editor
            .as_ref()
            .filter(|s| s.id == serial)
        else {
            return;
        };
        let values: Vec<_> = (0..session.field_count())
            .map(|index| session.field_value(index).unwrap())
            .collect();
        self.serial = Some(serial);
        self.reset_bounds.set(None);
        self.reset_press = None;
        self.fields = self.make_fields(serial, values.len(), cx);
        for (index, value) in values.into_iter().enumerate() {
            self.fields[index].update(cx, |field, _| {
                field.sync(format!("vertex-{serial}-{index}"), value, w)
            });
        }
        // The old callbacks are already invalid before this focus change can
        // submit the old focused field, including a queued blur after Reset.
        w.focus(&self.focus);
    }

    fn reset(&mut self, serial: u64, w: &mut Window, cx: &mut Context<Self>) {
        if !matching_session(self.state.read(cx), serial) {
            return;
        }
        let reset = self.state.update(cx, |state, cx| {
            let reset = state.reset_vertex_editor(serial);
            cx.notify();
            reset
        });
        if reset {
            let next = self.state.read(cx).vertex_editor.as_ref().unwrap().id;
            self.rebind_fields(next, w, cx);
            cx.notify();
        }
    }

    fn pointer_down(
        &mut self,
        serial: u64,
        e: &MouseDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if e.button != MouseButton::Left
            || !self
                .reset_bounds
                .get()
                .is_some_and(|bounds| bounds.hit(serial, e.position))
        {
            return;
        }
        // TextField submits outside mouse-down during capture, before normal
        // button handlers. Consume only a Reset press before those callbacks;
        // keep the focused field and draft untouched until a matching release.
        self.reset_press = Some(serial);
        cx.stop_propagation();
        w.prevent_default();
    }

    fn pointer_up(
        &mut self,
        serial: u64,
        e: &MouseUpEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if e.button != MouseButton::Left {
            return;
        }
        let Some(apply) = release_reset_press(
            &mut self.reset_press,
            serial,
            self.reset_bounds.get(),
            e.position,
        ) else {
            return;
        };
        if apply {
            self.reset(serial, w, cx);
        }
        // Release outside Reset cancels the press, including outside the dialog.
        // The consumed mouse-down cannot produce an additional native click.
        cx.stop_propagation();
        w.prevent_default();
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

    fn key(&mut self, serial: u64, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        if matches!(key, "escape" | "enter" | "tab") && TextField::is_composing(w, cx) {
            cx.stop_propagation();
            return;
        }
        if ((e.keystroke.modifiers.control || e.keystroke.modifiers.platform)
            && matches!(key, "s" | "o" | "n"))
            || (e.keystroke.modifiers.alt && key == "f4")
        {
            // TextField normally blurs for document shortcuts; retain modal focus.
            cx.stop_propagation();
            w.prevent_default();
            return;
        }
        if !matching_session(self.state.read(cx), serial) {
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
                        this.reset_press = None;
                        this.reset_bounds.set(None);
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
            .on_key_down(cx.listener(|_, _, _, cx| cx.stop_propagation()));
        let Some(id) = self.state.read(cx).vertex_editor.as_ref().map(|s| s.id) else {
            self.serial = None;
            self.reset_press = None;
            self.reset_bounds.set(None);
            return root;
        };
        if self.serial != Some(id) {
            self.rebind_fields(id, w, cx);
        }
        root = root
            .capture_key_down(cx.listener(move |this, e, w, cx| this.key(id, e, w, cx)))
            .capture_any_mouse_down(
                cx.listener(move |this, e, w, cx| this.pointer_down(id, e, w, cx)),
            )
            .capture_any_mouse_up(cx.listener(move |this, e, w, cx| this.pointer_up(id, e, w, cx)))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, e, w, cx| this.pointer_up(id, e, w, cx)),
            );
        let session = self.state.read(cx).vertex_editor.as_ref().unwrap();
        let transform = session.is_transform();
        let selection = if transform {
            format!("{} selected vertices", session.request().indices.len())
        } else {
            format!("Vertex {}", session.index + 1)
        };
        let descriptor = format!(
            "{} · Layer {} · {} · Frame {}",
            path_label(session.target),
            session.layer,
            selection,
            session.frame
        );
        let values: Vec<_> = (0..session.field_count())
            .map(|i| session.field_value(i).unwrap())
            .collect();
        let errors: Vec<_> = (0..session.field_count())
            .map(|i| session.field_error(i).map(str::to_owned))
            .collect();
        let error = session.error.clone();
        root = root
            .child(div().text_size(px(16.)).child(dialog_title(transform)))
            .child(div().text_size(px(11.)).text_color(rgb(ui::MUTED)).child(descriptor))
            .child(div().text_size(px(11.)).text_color(rgb(ui::MUTED)).child(if transform {
                "Path-local scale → rotate → translate. Pivot starts at the selected anchors’ bounds center and stays fixed until changed."
            } else {
                "Path-local pixels (px). Incoming and outgoing offsets are relative to the anchor; handles edit independently."
            }));
        for row in field_rows(transform) {
            let mut controls = div()
                .flex()
                .flex_none()
                .items_center()
                .gap_2()
                .child(div().w(px(118.)).flex_none().child(row.label));
            for &(index, label) in row.fields {
                if errors[index].is_none() {
                    self.fields[index].update(cx, |field, _| {
                        field.sync(format!("vertex-{id}-{index}"), values[index].clone(), w)
                    });
                }
                controls = controls.child(
                    div()
                        .id(("vertex-editor-field", index))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .child(label)
                        .child(self.fields[index].clone())
                        .when_some(errors[index].clone(), |field, error| {
                            field.child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(0xffaa88))
                                    .child(error),
                            )
                        }),
                );
            }
            root = root.child(controls);
        }
        let reset_bounds = self.reset_bounds.clone();
        root.child(div().text_size(px(11.)).text_color(rgb(ui::MUTED)).child(if transform {
                "Enter or Tab previews. Negative and zero scale are allowed; resulting geometry must be finite within ±1,000,000. Animated paths update at this frame only; static paths stay static."
            } else {
                "Enter or Tab previews each value. Animated paths update at this frame; static paths stay static. Values must be finite and between −1,000,000 and 1,000,000."
            }))
            .when(errors.iter().all(Option::is_none) && !error.is_empty(), |root| root.child(div().text_color(rgb(0xffaa88)).child(error)))
            .child(div()
                .on_children_prepainted(move |bounds, _, _| {
                    reset_bounds.set(if transform {
                        bounds.first().copied().map(|bounds| ResetBounds { serial: id, bounds })
                    } else {
                        None
                    });
                })
                .flex().flex_none().gap_2().justify_end()
                .when(transform, |buttons| buttons.child(ui::text_button("reset-vertex", "Reset")
                    .on_click(cx.listener(move |this, _, w, cx| this.reset(id, w, cx)))))
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

    fn opened_selection(indices: &[usize]) -> EditorState {
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
        let request = Request::for_selection(
            &state,
            1,
            PathTarget::Shape,
            indices.iter().copied().collect(),
            path,
            world,
        )
        .unwrap();
        state.vertex_editor = Some(Session::new(&state, request).unwrap());
        state
    }

    fn opened() -> EditorState {
        opened_selection(&[0])
    }

    #[test]
    fn vertex_dialog_mode_controls_field_order_labels_and_singleton_compatibility() {
        for (indices, title, expected_count) in [
            (&[0][..], "Edit Vertex", 6),
            (&[0, 1][..], "Transform Vertices", 7),
        ] {
            let state = opened_selection(indices);
            let session = state.vertex_editor.as_ref().unwrap();
            assert_eq!(dialog_title(session.is_transform()), title);
            assert_eq!(session.field_count(), expected_count);
            let fields: Vec<_> = field_rows(session.is_transform())
                .iter()
                .flat_map(|row| row.fields)
                .copied()
                .collect();
            assert_eq!(
                fields.iter().map(|&(index, _)| index).collect::<Vec<_>>(),
                (0..expected_count).collect::<Vec<_>>()
            );
            for (index, _) in fields {
                assert_eq!(
                    session.field_value(index).unwrap().parse::<f64>().unwrap(),
                    session.value(index).unwrap()
                );
            }
        }
        assert_eq!(
            VERTEX_ROWS.iter().map(|row| row.label).collect::<Vec<_>>(),
            ["Anchor", "Incoming offset", "Outgoing offset"]
        );
        assert_eq!(
            TRANSFORM_ROWS
                .iter()
                .flat_map(|row| row.fields)
                .map(|&(_, label)| label)
                .collect::<Vec<_>>(),
            [
                "Delta X (local px)",
                "Delta Y (local px)",
                "Rotation (°)",
                "Scale X (%)",
                "Scale Y (%)",
                "Pivot X (local px)",
                "Pivot Y (local px)"
            ]
        );
    }

    #[test]
    fn vertex_transform_reset_rebind_rejects_old_fields_reset_and_reopen_callbacks() {
        let mut state = opened_selection(&[0, 1]);
        let source = state.editor.project().clone();
        let first = state.vertex_editor.as_ref().unwrap().id;
        let initial: Vec<_> = (0..7)
            .map(|index| {
                state
                    .vertex_editor
                    .as_ref()
                    .unwrap()
                    .field_value(index)
                    .unwrap()
            })
            .collect();
        state.vertex_input(first, 0, "25.123456789012345");
        state.vertex_input(first, 3, "-50");
        state.vertex_input(first, 1, "invalid");
        state.vertex_input(first, 6, "NaN");
        assert!(state.reset_vertex_editor(first));
        let next = state.vertex_editor.as_ref().unwrap().id;
        assert_ne!(first, next);
        assert!(!matching_session(&state, first));
        assert!(matching_session(&state, next));
        let reset = state.vertex_editor.as_ref().unwrap();
        assert_eq!(reset.project(), &source);
        assert_eq!(
            (0..7)
                .map(|index| reset.field_value(index).unwrap())
                .collect::<Vec<_>>(),
            initial
        );
        assert!((0..7).all(|index| !reset.has_input_error(index)));
        assert!(reset.error.is_empty());
        for index in 0..7 {
            state.vertex_input(first, index, "500");
            assert!(state.revert_vertex_field(first, index).is_none());
        }
        assert!(!state.reset_vertex_editor(first));
        assert_eq!(state.vertex_editor.as_ref().unwrap().id, next);
        assert_eq!(state.vertex_editor.as_ref().unwrap().project(), &source);
        state.cancel_vertex_editor();
        let request = state.vertex_return.take().unwrap();
        state.vertex_editor = Some(Session::new(&state, request).unwrap());
        let reopened = state.vertex_editor.as_ref().unwrap().id;
        for stale in [first, next] {
            state.vertex_input(stale, 0, "900");
            assert!(!matching_session(&state, stale));
            assert!(!state.reset_vertex_editor(stale));
        }
        assert!(matching_session(&state, reopened));
        assert_eq!(state.vertex_editor.as_ref().unwrap().project(), &source);
        state.cancel_vertex_editor();
        state.vertex_input(reopened, 0, "400");
        assert!(!state.reset_vertex_editor(reopened));
        assert_eq!(state.editor.project(), &source);
        assert!(!state.editor.can_undo());
        assert!(!state.editor.can_redo());
    }

    #[test]
    fn vertex_transform_field_escape_clears_only_its_rejected_value() {
        let mut state = opened_selection(&[0, 1]);
        let serial = state.vertex_editor.as_ref().unwrap().id;
        state.vertex_input(serial, 0, "17.123456789012345");
        let accepted = state
            .vertex_editor
            .as_ref()
            .unwrap()
            .field_value(0)
            .unwrap();
        let preview = state.vertex_editor.as_ref().unwrap().project().clone();
        state.vertex_input(serial, 0, "not numeric");
        state.vertex_input(serial, 2, "NaN");
        assert!(
            state
                .vertex_editor
                .as_ref()
                .unwrap()
                .field_error(0)
                .is_some()
        );
        assert!(
            state
                .vertex_editor
                .as_ref()
                .unwrap()
                .field_error(2)
                .is_some()
        );
        assert_eq!(state.revert_vertex_field(serial, 0), Some(accepted));
        let session = state.vertex_editor.as_ref().unwrap();
        assert!(!session.has_input_error(0));
        assert!(session.has_input_error(2));
        assert!(session.command().is_err());
        assert_eq!(session.project(), &preview);
        assert_eq!(state.revert_vertex_field(serial, 2), Some("0".into()));
        assert!(state.revert_vertex_field(serial, 2).is_none());
        assert!(state.vertex_editor.as_ref().unwrap().command().is_ok());
    }

    #[test]
    fn vertex_reset_pointer_release_is_serial_bound_and_drag_out_cancels() {
        let inside = gpui::point(px(20.), px(15.));
        let outside = gpui::point(px(300.), px(150.));
        let bounds = ResetBounds {
            serial: 4,
            bounds: Bounds::new(gpui::point(px(10.), px(10.)), gpui::size(px(50.), px(25.))),
        };
        assert!(bounds.hit(4, inside));
        assert!(!bounds.hit(5, inside));
        assert!(!bounds.hit(4, outside));
        let mut press = Some(4);
        assert_eq!(
            release_reset_press(&mut press, 4, Some(bounds), outside),
            Some(false)
        );
        assert!(press.is_none());
        assert_eq!(
            release_reset_press(&mut press, 4, Some(bounds), inside),
            None
        );
        press = Some(4);
        assert_eq!(
            release_reset_press(&mut press, 5, Some(bounds), inside),
            Some(false)
        );
        press = Some(4);
        assert_eq!(
            release_reset_press(&mut press, 4, None, inside),
            Some(false)
        );
        press = Some(4);
        assert_eq!(
            release_reset_press(&mut press, 4, Some(bounds), inside),
            Some(true)
        );
        assert!(press.is_none());
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
