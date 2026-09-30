use crate::ui;
use gpui::{
    AnyView, Bounds, ClickEvent, Context, Entity, IntoElement, MouseButton, MouseMoveEvent, Pixels,
    Render, Window, canvas, div, prelude::*, px, relative, rgb,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}
fn clamp_fraction(fraction: f32, minimum_fraction: f32) -> f32 {
    fraction.clamp(minimum_fraction, 1.0 - minimum_fraction)
}
pub(crate) struct ResizablePanelGroup {
    orientation: Orientation,
    first: AnyView,
    second: AnyView,
    initial_fraction: f32,
    fraction: f32,
    minimum_fraction: f32,
    resizing: bool,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}
impl ResizablePanelGroup {
    pub(crate) fn new<A: Render, B: Render>(
        orientation: Orientation,
        first: Entity<A>,
        second: Entity<B>,
    ) -> Self {
        Self {
            orientation,
            first: first.into(),
            second: second.into(),
            initial_fraction: 0.5,
            fraction: 0.5,
            minimum_fraction: 0.1,
            resizing: false,
            bounds: Rc::new(Cell::new(None)),
        }
    }
    pub(crate) fn initial_fraction(mut self, fraction: f32) -> Self {
        self.fraction = clamp_fraction(fraction, self.minimum_fraction);
        self.initial_fraction = self.fraction;
        self
    }
    pub(crate) fn minimum_fraction(mut self, fraction: f32) -> Self {
        self.minimum_fraction = fraction.clamp(0.0, 0.49);
        self.fraction = clamp_fraction(self.fraction, self.minimum_fraction);
        self.initial_fraction = self.fraction;
        self
    }
    pub(crate) fn reset(&mut self, cx: &mut Context<Self>) {
        self.fraction = self.initial_fraction;
        self.resizing = false;
        cx.notify();
    }
    fn moving(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.resizing || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(bounds) = self.bounds.get() else {
            return;
        };
        let fraction = match self.orientation {
            Orientation::Horizontal => {
                f32::from(event.position.x - bounds.left()) / f32::from(bounds.size.width).max(1.0)
            }
            Orientation::Vertical => {
                f32::from(event.position.y - bounds.top()) / f32::from(bounds.size.height).max(1.0)
            }
        };
        self.fraction = clamp_fraction(fraction, self.minimum_fraction);
        cx.notify();
    }
}
impl Render for ResizablePanelGroup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let orientation = self.orientation;
        let fraction = self.fraction;
        let bounds = self.bounds.clone();
        let handle = div()
            .id("panel-divider")
            .flex_none()
            .bg(rgb(ui::BORDER))
            .when(orientation == Orientation::Horizontal, |s| {
                s.w(px(5.0)).h_full().cursor_col_resize()
            })
            .when(orientation == Orientation::Vertical, |s| {
                s.h(px(5.0)).w_full().cursor_row_resize()
            })
            .hover(|s| s.bg(rgb(ui::BLUE)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.resizing = true;
                    cx.stop_propagation();
                }),
            )
            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                if event.click_count() >= 2 {
                    this.reset(cx);
                }
            }));
        div()
            .id("resizable-panel-group")
            .relative()
            .flex()
            .size_full()
            .overflow_hidden()
            .when(orientation == Orientation::Vertical, |s| s.flex_col())
            .on_mouse_move(cx.listener(Self::moving))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.resizing = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.resizing = false),
            )
            .child(
                canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| ())
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .flex_none()
                    .overflow_hidden()
                    .when(orientation == Orientation::Horizontal, |s| {
                        s.w(relative(fraction)).h_full()
                    })
                    .when(orientation == Orientation::Vertical, |s| {
                        s.h(relative(fraction)).w_full()
                    })
                    .child(self.first.clone()),
            )
            .child(handle)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(self.second.clone()),
            )
    }
}
#[cfg(test)]
mod tests {
    use super::clamp_fraction;
    #[test]
    fn panel_fraction_respects_both_minimums() {
        assert_eq!(clamp_fraction(-1.0, 0.2), 0.2);
        assert_eq!(clamp_fraction(0.45, 0.2), 0.45);
        assert_eq!(clamp_fraction(2.0, 0.2), 0.8);
    }
}
