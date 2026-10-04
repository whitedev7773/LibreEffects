use crate::{
    color_edit::{InputTarget, Target, from_hsv},
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, MouseButton, MouseMoveEvent, Pixels, Point, Window,
    canvas, div, fill, point, prelude::*, px, rgb, size,
};
use std::{cell::Cell, rc::Rc};

fn swatch_button(
    id: impl Into<gpui::ElementId>,
    color: u32,
    target: Target,
) -> gpui::Stateful<gpui::Div> {
    ui::text_button(id, "")
        .w(px(26.0))
        .h(px(20.0))
        .mr_1()
        .bg(rgb(color))
        .border_1()
        .border_color(rgb(ui::MUTED))
        .tooltip(move |_, cx| {
            cx.new(|_| ui::Tip(format!("{} · #{color:06X}", target.title()).into()))
                .into()
        })
}

pub(crate) fn swatch(
    id: impl Into<gpui::ElementId>,
    color: u32,
    target: Target,
    locked: bool,
    state: &Entity<EditorState>,
) -> gpui::Stateful<gpui::Div> {
    let state = state.clone();
    swatch_button(id, color, target).when(!locked, |d| {
        d.on_click(move |_, w, cx| {
            TextField::commit_active(w, cx);
            state.update(cx, |s, cx| s.dispatch(&Action::OpenColor(target), w, cx));
        })
    })
}

/// A text swatch uses the same pointer-before-blur receipt as its scalar watch.
/// Pending input is committed before opening the dialog from the refreshed source.
pub(crate) fn text_swatch(
    id: &'static str,
    color: u32,
    layer: libre_effects_core::LayerId,
    paint: libre_effects_core::TextPaint,
    rendered: Option<InputTarget>,
    state: &Entity<EditorState>,
) -> gpui::Stateful<gpui::Div> {
    let state = state.clone();
    let target = Target::Text(layer, paint);
    let control = id.to_owned();
    crate::color_edit::input_pointer_button(
        swatch_button(id, color, target),
        control.clone(),
        rendered.clone(),
    )
    .on_click(move |event, w, cx| {
        cx.stop_propagation();
        let Some(binding) =
            crate::color_edit::input_click_target(&control, event, &rendered, &state, w, cx)
        else {
            return;
        };
        if state.read(cx).editor.selected() != Some(layer) {
            return;
        }
        TextField::commit_active(w, cx);
        state.update(cx, |s, cx| {
            if !binding.same_context(s) {
                return;
            }
            s.finish_text(true, cx);
            if binding.same_context(s) {
                s.dispatch(&Action::OpenColor(target), w, cx);
            }
        });
    })
}

fn session_visible(state: &EditorState, serial: u64) -> bool {
    state.colors.serial == serial && state.colors.session.as_ref().is_some_and(|s| !s.picking)
}

fn session_current(state: &EditorState, serial: u64) -> bool {
    session_visible(state, serial)
        && state.colors.session.as_ref().is_some_and(|session| {
            session
                .validate_context(
                    state.editor.project(),
                    state.document_revision,
                    state.frame,
                    state.editor.selected(),
                    state.playing,
                )
                .is_ok()
        })
}

fn commit_field(state: &mut EditorState, serial: Option<u64>, index: usize, text: &str) -> bool {
    if !serial.is_some_and(|serial| session_current(state, serial)) {
        return false;
    }
    if let Some(session) = &mut state.colors.session
        && let Err(error) = session.input(index, text)
    {
        session.error = error;
    }
    true
}

pub(crate) struct ColorPicker {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
    field_serials: Vec<Rc<Cell<Option<u64>>>>,
    bounds: [Rc<Cell<Option<Bounds<Pixels>>>>; 2],
    drag: Option<(u64, usize)>,
    focus: FocusHandle,
    serial: u64,
}
impl ColorPicker {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let field_serials: Vec<Rc<Cell<Option<u64>>>> =
            (0..5).map(|_| Default::default()).collect();
        let fields = (0..5)
            .map(|index| {
                let state = state.clone();
                let serial = field_serials[index].clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, _, cx| {
                        state.update(cx, |s, cx| {
                            if !commit_field(s, serial.get(), index, text) {
                                return;
                            }
                            cx.notify();
                        })
                    })
                })
            })
            .collect();
        Self {
            state,
            fields,
            field_serials,
            bounds: Default::default(),
            drag: None,
            focus: cx.focus_handle(),
            serial: 0,
        }
    }
    fn pointer(&self, serial: u64, index: usize, p: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(b) = self.bounds[index].get() else {
            return;
        };
        let x =
            (f32::from(p.x - b.left()) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0) as f64;
        let y =
            (f32::from(p.y - b.top()) / f32::from(b.size.height).max(1.0)).clamp(0.0, 1.0) as f64;
        self.state.update(cx, |s, cx| {
            if !session_current(s, serial) {
                return;
            }
            if let Some(session) = &mut s.colors.session {
                let mut hsv = session.hsv;
                if index == 0 {
                    hsv[1] = x;
                    hsv[2] = 1.0 - y;
                } else {
                    hsv[0] = y * 360.0;
                }
                session.set_hsv(hsv);
            }
            cx.notify();
        });
    }
    fn moving(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if e.pressed_button == Some(MouseButton::Left)
            && let Some((serial, index)) = self.drag
        {
            self.pointer(serial, index, e.position, cx);
        }
    }
}
impl Render for ColorPicker {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("color-dialog")
            .track_focus(&self.focus)
            .p_4()
            .w(px(560.0))
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(ui::MUTED))
            .flex()
            .flex_col()
            .gap_3();
        let s = self.state.read(cx);
        let Some(session) = &s.colors.session else {
            return root;
        };
        let (serial, target, color, original, hsv, error, recent) = (
            s.colors.serial,
            session.target,
            session.color,
            session.original,
            session.hsv,
            session.error.clone(),
            s.colors.recent.clone(),
        );
        if self.serial != serial {
            self.serial = serial;
            self.drag = None;
            w.focus(&self.focus);
        }
        root = root
            .child(div().text_size(px(15.0)).child(target.title()))
            .on_mouse_move(cx.listener(Self::moving))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag = None),
            )
            .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                let key = e.keystroke.key.as_str();
                if !matches!(
                    key,
                    "left" | "right" | "up" | "down" | "pageup" | "pagedown"
                ) {
                    return;
                }
                let step = if e.keystroke.modifiers.shift {
                    0.1
                } else {
                    0.01
                };
                this.state.update(cx, |s, cx| {
                    if !session_current(s, serial) {
                        return;
                    }
                    if let Some(session) = &mut s.colors.session {
                        let mut hsv = session.hsv;
                        match key {
                            "left" => hsv[1] -= step,
                            "right" => hsv[1] += step,
                            "up" => hsv[2] += step,
                            "down" => hsv[2] -= step,
                            "pageup" => hsv[0] += step * 100.0,
                            _ => hsv[0] -= step * 100.0,
                        }
                        session.set_hsv(hsv);
                    }
                    cx.notify();
                });
                cx.stop_propagation();
            }));
        let mut maps = div().flex().gap_2();
        for index in 0..2 {
            let bounds = self.bounds[index].clone();
            maps = maps.child(
                div()
                    .id(("color-map", index))
                    .w(px(if index == 0 { 240.0 } else { 24.0 }))
                    .h(px(240.0))
                    .cursor_crosshair()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, w, cx| {
                            if !session_current(this.state.read(cx), serial) {
                                return;
                            }
                            TextField::commit_active(w, cx);
                            w.focus(&this.focus);
                            this.drag = Some((serial, index));
                            this.pointer(serial, index, e.position, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        canvas(
                            move |b, _, _| bounds.set(Some(b)),
                            move |b, _, w, _| {
                                let cols = if index == 0 { 64 } else { 1 };
                                let rows = 64;
                                for y in 0..rows {
                                    for x in 0..cols {
                                        let color = if index == 0 {
                                            from_hsv([
                                                hsv[0],
                                                x as f64 / (cols - 1) as f64,
                                                1.0 - y as f64 / (rows - 1) as f64,
                                            ])
                                        } else {
                                            from_hsv([
                                                y as f64 / (rows - 1) as f64 * 360.0,
                                                1.0,
                                                1.0,
                                            ])
                                        };
                                        w.paint_quad(fill(
                                            Bounds::new(
                                                point(
                                                    b.left()
                                                        + b.size.width * x as f32 / cols as f32,
                                                    b.top()
                                                        + b.size.height * y as f32 / rows as f32,
                                                ),
                                                size(
                                                    b.size.width / cols as f32 + px(0.5),
                                                    b.size.height / rows as f32 + px(0.5),
                                                ),
                                            ),
                                            rgb(color),
                                        ));
                                    }
                                }
                                let p = if index == 0 {
                                    point(
                                        b.left() + b.size.width * hsv[1] as f32,
                                        b.top() + b.size.height * (1.0 - hsv[2]) as f32,
                                    )
                                } else {
                                    point(
                                        b.left() + b.size.width / 2.0,
                                        b.top() + b.size.height * (hsv[0] / 360.0) as f32,
                                    )
                                };
                                for (width, color) in [(9.0, 0x000000), (5.0, 0xffffff)] {
                                    w.paint_quad(fill(
                                        Bounds::new(
                                            p - point(px(width / 2.0), px(width / 2.0)),
                                            size(px(width), px(width)),
                                        ),
                                        rgb(color),
                                    ));
                                }
                            },
                        )
                        .size_full(),
                    ),
            );
        }
        let mut inputs = div().flex().flex_col().gap_2().flex_1();
        let values = [
            format!("{:06X}", color.rgb),
            ((color.rgb >> 16) & 255).to_string(),
            ((color.rgb >> 8) & 255).to_string(),
            (color.rgb & 255).to_string(),
            if matches!(target, Target::Text(..)) {
                color.opacity.to_string()
            } else {
                format!("{:.2}", color.opacity)
            },
        ];
        for (index, label) in ["HEX", "Red", "Green", "Blue", target.opacity_label()]
            .into_iter()
            .enumerate()
        {
            if index == 4 && !target.alpha() {
                continue;
            }
            self.field_serials[index].set(Some(serial));
            self.fields[index].update(cx, |f, _| {
                f.sync(serial.to_string(), values[index].clone(), w)
            });
            inputs = inputs.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(105.0)).child(label))
                    .child(div().flex_1().child(self.fields[index].clone())),
            );
        }
        inputs = inputs.child(
            div().flex().gap_2().children(
                [(original, "Original"), (color, "New")]
                    .into_iter()
                    .map(|(c, label)| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .w(px(80.0))
                                    .h(px(26.0))
                                    .bg(rgb(c.rgb))
                                    .border_1()
                                    .border_color(rgb(ui::MUTED)),
                            )
                            .child(label)
                    }),
            ),
        );
        root = root.child(div().flex().gap_3().child(maps).child(inputs));
        root = root.child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(
            if matches!(target, Target::Text(_, libre_effects_core::TextPaint::Fill)) {
                "Opacity changes only Text Fill at the current frame. Six-digit HEX keeps opacity; RRGGBBAA includes it."
            } else if matches!(target, Target::Text(_, libre_effects_core::TextPaint::Stroke)) {
                "Opacity changes only Text Stroke at the current frame. Six-digit HEX keeps opacity; RRGGBBAA includes it."
            } else if matches!(
                target,
                crate::color_edit::Target::Shape(_, _) | crate::color_edit::Target::Contents(_, _)
            ) {
                "Opacity changes this fill or stroke at the current frame. HEX accepts RRGGBBAA."
            } else if matches!(target, crate::color_edit::Target::GradientStop(..)) {
                "This changes the selected color stop. Transparency uses separate opacity stops."
            } else if target.alpha() {
                "Opacity changes the entire layer at the current frame. HEX accepts RRGGBBAA."
            } else {
                "RGB color · background is opaque; stroke keeps the layer opacity."
            },
        ));
        let mut colors = div().flex().items_center().gap_1().child("Recent");
        for (i, color) in recent.into_iter().enumerate() {
            let state = self.state.clone();
            colors = colors.child(
                ui::text_button(("recent-color", i), "")
                    .w(px(24.0))
                    .bg(rgb(color.rgb))
                    .border_1()
                    .border_color(rgb(ui::MUTED))
                    .on_click(move |_, w, cx| {
                        if !session_current(state.read(cx), serial) {
                            return;
                        }
                        TextField::commit_active(w, cx);
                        w.blur();
                        state.update(cx, |s, cx| {
                            if !session_current(s, serial) {
                                return;
                            }
                            if let Some(session) = &mut s.colors.session {
                                session.set_color(color);
                            }
                            cx.notify();
                        });
                    }),
            );
        }
        let mut buttons = div().flex().gap_2();
        for (id, label, action) in [
            ("sample-color", "Pick from Composition", Action::PickColor),
            ("accept-color", "OK", Action::ApplyColor),
            ("cancel-color", "Cancel", Action::CancelColor),
        ] {
            let state = self.state.clone();
            buttons = buttons.child(ui::text_button(id, label).on_click(move |_, w, cx| {
                if !session_visible(state.read(cx), serial)
                    || matches!(action, Action::PickColor)
                        && !session_current(state.read(cx), serial)
                {
                    return;
                }
                if !matches!(action, Action::CancelColor) {
                    TextField::commit_active(w, cx);
                }
                w.blur();
                state.update(cx, |s, cx| {
                    if session_visible(s, serial) {
                        s.dispatch(&action, w, cx);
                    }
                });
            }));
        }
        root.child(colors).child(div().text_color(rgb(0xffaa88)).child(error)).child(buttons)
            .child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child("Color area: arrows change saturation / brightness; Page Up/Down change hue. Shift: larger steps."))
    }
}

#[cfg(test)]
mod text_picker_tests {
    use super::*;
    use crate::color_edit::Session;
    use libre_effects_core::{Command, Content, TextPaint};

    fn scene(paint: TextPaint) -> EditorState {
        let mut state = EditorState::default();
        state
            .editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Text".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Text".into(),
            })
            .unwrap();
        state.frame = 17;
        state.colors.serial = 3;
        state.colors.session = Some(
            Session::new(
                Target::Text(1, paint),
                state.editor.project(),
                state.document_revision,
                state.frame,
            )
            .unwrap(),
        );
        state.editor.clear_history();
        state
    }

    #[test]
    fn text_picker_field_commit_is_draft_only_precise_and_uses_frozen_serial() {
        for paint in [TextPaint::Fill, TextPaint::Stroke] {
            let mut state = scene(paint);
            let source = state.editor.project().clone();
            assert!(commit_field(&mut state, Some(3), 4, "27.123456789012345"));
            let expected = state.colors.session.as_ref().unwrap().color;
            assert_eq!(expected.opacity, 27.123456789012345);
            assert!(commit_field(&mut state, Some(3), 0, "abcdef"));
            assert_eq!(
                state.colors.session.as_ref().unwrap().color.opacity,
                expected.opacity
            );
            let draft = state.colors.session.as_ref().unwrap().color;
            assert!(!commit_field(&mut state, None, 4, "1"));
            assert!(!commit_field(&mut state, Some(2), 4, "1"));
            assert_eq!(state.colors.session.as_ref().unwrap().color, draft);
            state.colors.serial = 4; // Same source, a newly opened dialog.
            assert!(!commit_field(&mut state, Some(3), 4, "1"));
            assert_eq!(state.colors.session.as_ref().unwrap().color, draft);
            assert_eq!(state.editor.project(), &source);
            assert!(!state.editor.can_undo());
        }
    }

    #[test]
    fn text_picker_hidden_or_stale_callbacks_cannot_mutate_current_draft() {
        // These are model/helper calls, not evidence of native key or mouse delivery.
        for paint in [TextPaint::Fill, TextPaint::Stroke] {
            for change in 0..9 {
                let mut state = scene(paint);
                let original = state.colors.session.as_ref().unwrap().color;
                match change {
                    0 => state.colors.session.as_mut().unwrap().picking = true,
                    1 => state.colors.serial += 1,
                    2 => state.frame += 1,
                    3 => state.document_revision += 1,
                    4 => state.editor.clear_selection(),
                    5 => state.playing = true,
                    6 => {
                        state.editor.execute(Command::ToggleLocked(1)).unwrap();
                    }
                    7 => {
                        state
                            .editor
                            .execute(Command::SetContent {
                                id: 1,
                                content: Content::Text {
                                    text: "Changed source".into(),
                                    font_size: 48.,
                                },
                            })
                            .unwrap();
                    }
                    _ => {
                        state.editor.execute(Command::NewComposition).unwrap();
                    }
                }
                assert!(!session_current(&state, 3));
                assert!(!commit_field(&mut state, Some(3), 4, "2.5"));
                assert_eq!(state.colors.session.as_ref().unwrap().color, original);
            }
            let mut state = scene(paint);
            state.colors.session = None;
            assert!(!commit_field(&mut state, Some(3), 4, "2.5"));
        }
    }
    #[test]
    fn generic_picker_shared_stale_gate_keeps_selection_playback_and_alpha_policy() {
        // Shared callback guards intentionally strengthen old generic dialogs;
        // generic target color/alpha and selection/playback policy is unchanged.
        for target in [
            Target::Fill(1),
            Target::Stroke(1),
            Target::BackgroundDraft(0x102030),
        ] {
            let generic_scene = || {
                let mut state = scene(TextPaint::Fill);
                state.colors.session = Some(
                    Session::new(
                        target,
                        state.editor.project(),
                        state.document_revision,
                        state.frame,
                    )
                    .unwrap(),
                );
                state
            };
            let mut state = generic_scene();
            state.editor.clear_selection();
            state.playing = true;
            assert!(session_current(&state, 3));
            assert!(commit_field(&mut state, Some(3), 0, "abcdef"));
            assert_eq!(state.colors.session.as_ref().unwrap().color.rgb, 0xabcdef);
            assert!(commit_field(&mut state, Some(3), 4, "27.123456789012345"));
            let session = state.colors.session.as_ref().unwrap();
            if target.alpha() {
                assert!(session.error.is_empty());
                assert_eq!(session.color.opacity, 27.123456789012345);
                state.editor.execute(session.command().unwrap()).unwrap();
                let layer = state.editor.project().composition().layer(1).unwrap();
                assert_eq!(
                    layer
                        .property(libre_effects_core::Property::Opacity)
                        .value_at(17),
                    27.123456789012345
                );
                for paint in [TextPaint::Fill, TextPaint::Stroke] {
                    assert!(
                        layer
                            .track(libre_effects_core::PropertyPath::Text(paint.opacity()))
                            .is_none()
                    );
                }
            } else {
                assert!(!session.error.is_empty());
                assert_eq!(session.color.opacity, 100.);
                assert!(commit_field(&mut state, Some(3), 0, "11223380"));
                assert_eq!(state.colors.session.as_ref().unwrap().color.rgb, 0xabcdef);
            }
            for change in 0..6 {
                let mut state = generic_scene();
                let original = state.colors.session.as_ref().unwrap().color;
                match change {
                    0 => state.colors.serial += 1,
                    1 => state.colors.session.as_mut().unwrap().picking = true,
                    2 => state.frame += 1,
                    3 => state.document_revision += 1,
                    4 => {
                        state
                            .editor
                            .execute(Command::SetColor {
                                id: 1,
                                color: 0x123456,
                            })
                            .unwrap();
                    }
                    _ => state.colors.session = None,
                }
                assert!(!session_current(&state, 3));
                assert!(!commit_field(&mut state, Some(3), 0, "abcdef"));
                if let Some(session) = &state.colors.session {
                    assert_eq!(session.color, original);
                }
            }
        }
    }
}
