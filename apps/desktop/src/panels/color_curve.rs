use crate::{
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathBuilder, Pixels, Point, Window, canvas, div, fill, point, prelude::*, px,
    rgb, size,
};
use libre_effects_core::{
    Command, CurveChannel, EffectEdit, EffectId, EffectKind, LayerId, sample_color_curve,
};
use std::{cell::Cell, rc::Rc};

pub(crate) struct ColorCurve {
    state: Entity<EditorState>,
    layer: LayerId,
    effect: EffectId,
    pub(crate) channel: CurveChannel,
    selected: usize,
    drag: Option<(usize, f64, u32)>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
}
impl ColorCurve {
    pub(crate) fn new(
        state: Entity<EditorState>,
        layer: LayerId,
        effect: EffectId,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |this, _, cx| {
            this.drag = None;
            cx.notify();
        })
        .detach();
        Self {
            state,
            layer,
            effect,
            channel: CurveChannel::Rgb,
            selected: 2,
            drag: None,
            bounds: Default::default(),
            focus: cx.focus_handle(),
        }
    }
    fn editable(&self, cx: &Context<Self>) -> bool {
        self.state
            .read(cx)
            .editor
            .project()
            .composition()
            .layer(self.layer)
            .is_some_and(|l| {
                !l.locked()
                    && l.effect_stack()
                        .iter()
                        .any(|e| e.id() == self.effect && e.kind() == EffectKind::Curves)
            })
    }
    fn commit(
        &self,
        index: usize,
        value: f64,
        frame: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |s, cx| {
            s.dispatch(
                &Action::Edit(Command::Effect {
                    id: self.layer,
                    edit: EffectEdit::SetValue {
                        effect: self.effect,
                        parameter: self.channel.parameters()[index],
                        frame,
                        value: value.clamp(0.0, 255.0),
                    },
                }),
                window,
                cx,
            )
        });
    }
    fn value(&self, p: Point<Pixels>) -> Option<(usize, f64)> {
        let b = self.bounds.get()?;
        let x = (f32::from(p.x - b.left()) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0);
        let y = (f32::from(b.bottom() - p.y) / f32::from(b.size.height).max(1.0)).clamp(0.0, 1.0);
        Some(((x * 4.0).round() as usize, y as f64 * 255.0))
    }
    fn down(&mut self, e: &MouseDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        if let Some((index, value)) = self.value(e.position) {
            w.focus(&self.focus);
            self.selected = index;
            self.drag = Some((index, value, self.state.read(cx).frame));
            cx.stop_propagation();
            cx.notify();
        }
    }
    fn moving(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some((index, _, frame)) = self.drag
            && e.pressed_button == Some(MouseButton::Left)
        {
            if let Some((_, value)) = self.value(e.position) {
                self.drag = Some((index, value, frame));
                cx.notify();
            }
            cx.stop_propagation();
        }
    }
    fn up(&mut self, _: &MouseUpEvent, w: &mut Window, cx: &mut Context<Self>) {
        if let Some((index, value, frame)) = self.drag.take() {
            self.commit(index, value, frame, w, cx);
            cx.stop_propagation();
            cx.notify();
        }
    }
}
impl Render for ColorCurve {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let frame = state.frame;
        let Some(effect) = state
            .editor
            .project()
            .composition()
            .layer(self.layer)
            .and_then(|l| l.effect_stack().iter().find(|e| e.id() == self.effect))
        else {
            return div();
        };
        if effect.kind() != EffectKind::Curves {
            return div();
        }
        let mut values = effect.curve_values(self.channel, frame);
        if let Some((index, value, _)) = self.drag {
            values[index] = value / 255.0;
        }
        let color = match self.channel {
            CurveChannel::Rgb => ui::TEXT,
            CurveChannel::Red => 0xff7373,
            CurveChannel::Green => 0x70db8a,
            CurveChannel::Blue => 0x62aaff,
        };
        let selected = self.selected;
        let bounds = self.bounds.clone();
        let mut channels = div().flex().items_center().gap_1();
        for (index, channel) in CurveChannel::ALL.into_iter().enumerate() {
            channels = channels.child(
                ui::text_button(("curve-channel", index), channel.label())
                    .when(channel == self.channel, |s| {
                        s.bg(rgb(0x164a7b)).text_color(rgb(ui::BLUE))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.channel = channel;
                        this.drag = None;
                        cx.notify();
                        cx.stop_propagation();
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(channels)
            .child(
                div()
                    .id("color-curve-plot")
                    .track_focus(&self.focus)
                    .tab_index(0)
                    .bg(rgb(0x171717))
                    .p_2()
                    .cursor_crosshair()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::down))
                    .on_mouse_move(cx.listener(Self::moving))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::up))
                    .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, w, cx| {
                        if e.keystroke.modifiers.control
                            || e.keystroke.modifiers.alt
                            || e.keystroke.modifiers.platform
                        {
                            return;
                        }
                        match e.keystroke.key.as_str() {
                            "escape" => this.drag = None,
                            "left" => this.selected = this.selected.saturating_sub(1),
                            "right" => this.selected = (this.selected + 1).min(4),
                            "up" | "down" => {
                                if !this.editable(cx) {
                                    return;
                                }
                                let state = this.state.read(cx);
                                let frame = state.frame;
                                let Some(fx) = state
                                    .editor
                                    .project()
                                    .composition()
                                    .layer(this.layer)
                                    .and_then(|l| {
                                        l.effect_stack().iter().find(|fx| fx.id() == this.effect)
                                    })
                                else {
                                    return;
                                };
                                let step = if e.keystroke.modifiers.shift {
                                    10.0
                                } else {
                                    1.0
                                };
                                let value = fx
                                    .value_at(this.channel.parameters()[this.selected], frame)
                                    + if e.keystroke.key == "up" { step } else { -step };
                                this.commit(this.selected, value, frame, w, cx);
                            }
                            _ => return,
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(
                        canvas(
                            move |b, _, _| bounds.set(Some(b)),
                            move |b, _, w, _| {
                                let at = |x: f64, y: f64| {
                                    point(
                                        b.left() + b.size.width * x as f32,
                                        b.bottom() - b.size.height * y as f32,
                                    )
                                };
                                for i in 0..=4 {
                                    let t = i as f64 / 4.0;
                                    for points in
                                        [[at(t, 0.0), at(t, 1.0)], [at(0.0, t), at(1.0, t)]]
                                    {
                                        let mut path = PathBuilder::stroke(px(1.0));
                                        path.move_to(points[0]);
                                        path.line_to(points[1]);
                                        if let Ok(path) = path.build() {
                                            w.paint_path(path, rgb(0x353535));
                                        }
                                    }
                                }
                                let mut path = PathBuilder::stroke(px(1.5));
                                for i in 0..=128 {
                                    let x = i as f64 / 128.0;
                                    let p = at(x, sample_color_curve(values, x));
                                    if i == 0 {
                                        path.move_to(p);
                                    } else {
                                        path.line_to(p);
                                    }
                                }
                                if let Ok(path) = path.build() {
                                    w.paint_path(path, rgb(color));
                                }
                                for (i, value) in values.into_iter().enumerate() {
                                    let p = at(i as f64 / 4.0, value);
                                    w.paint_quad(fill(
                                        Bounds::new(
                                            p - point(px(3.0), px(3.0)),
                                            size(px(6.0), px(6.0)),
                                        ),
                                        rgb(if i == selected { ui::BLUE } else { color }),
                                    ));
                                }
                            },
                        )
                        .w_full()
                        .h(px(135.0)),
                    ),
            )
            .child(
                div()
                    .text_color(rgb(ui::MUTED))
                    .text_size(px(10.0))
                    .child("5 points · drag vertically · arrows adjust"),
            )
    }
}
