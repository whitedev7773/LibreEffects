use crate::{
    color_edit::{Target, from_hsv},
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, MouseButton, MouseMoveEvent, Pixels, Point, Window,
    canvas, div, fill, point, prelude::*, px, rgb, size,
};
use std::{cell::Cell, rc::Rc};

pub(crate) fn swatch(
    id: impl Into<gpui::ElementId>,
    color: u32,
    target: Target,
    locked: bool,
    state: &Entity<EditorState>,
) -> gpui::Stateful<gpui::Div> {
    let state = state.clone();
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
        .when(!locked, |d| {
            d.on_click(move |_, w, cx| {
                TextField::commit_active(w, cx);
                state.update(cx, |s, cx| s.dispatch(&Action::OpenColor(target), w, cx));
            })
        })
}

pub(crate) struct ColorPicker {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
    bounds: [Rc<Cell<Option<Bounds<Pixels>>>>; 2],
    drag: Option<usize>,
    focus: FocusHandle,
    serial: u64,
}
impl ColorPicker {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let fields = (0..5)
            .map(|index| {
                let state = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, _, cx| {
                        state.update(cx, |s, cx| {
                            if let Some(session) = &mut s.colors.session
                                && let Err(error) = session.input(index, text)
                            {
                                session.error = error;
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
            bounds: Default::default(),
            drag: None,
            focus: cx.focus_handle(),
            serial: 0,
        }
    }
    fn pointer(&self, index: usize, p: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(b) = self.bounds[index].get() else {
            return;
        };
        let x =
            (f32::from(p.x - b.left()) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0) as f64;
        let y =
            (f32::from(p.y - b.top()) / f32::from(b.size.height).max(1.0)).clamp(0.0, 1.0) as f64;
        self.state.update(cx, |s, cx| {
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
            && let Some(index) = self.drag
        {
            self.pointer(index, e.position, cx);
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
            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, _, cx| {
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
                            TextField::commit_active(w, cx);
                            w.focus(&this.focus);
                            this.drag = Some(index);
                            this.pointer(index, e.position, cx);
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
            format!("{:.2}", color.opacity),
        ];
        for (index, label) in [
            "HEX",
            "Red",
            "Green",
            "Blue",
            if matches!(target, crate::color_edit::Target::Fill(_)) {
                "Layer opacity %"
            } else {
                "Opacity %"
            },
        ]
        .into_iter()
        .enumerate()
        {
            if index == 4 && !target.alpha() {
                continue;
            }
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
            if matches!(
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
                        TextField::commit_active(w, cx);
                        w.blur();
                        state.update(cx, |s, cx| {
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
                if !matches!(action, Action::CancelColor) {
                    TextField::commit_active(w, cx);
                }
                w.blur();
                state.update(cx, |s, cx| s.dispatch(&action, w, cx));
            }));
        }
        root.child(colors).child(div().text_color(rgb(0xffaa88)).child(error)).child(buttons)
            .child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child("Color area: arrows change saturation / brightness; Page Up/Down change hue. Shift: larger steps."))
    }
}
