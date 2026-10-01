use crate::{
    editor::{Action, EditorState, Tool, timecode},
    ui,
};
use gpui::{
    Bounds, ContentMask, Context, Entity, FocusHandle, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathBuilder, Pixels, Point, Window, canvas, div, fill, point, prelude::*, px,
    rgb, size,
};
use libre_effects_core::{Affine, Command, LayerId, Property};
use std::{cell::Cell, rc::Rc};

#[derive(Clone)]
struct MoveGesture {
    start: Point<Pixels>,
    delta: Point<Pixels>,
    layer: Option<LayerId>,
    position: [f64; 2],
    frame: u32,
    zoom: f32,
    pan: Point<Pixels>,
    inverse_space: Affine,
}
pub(crate) struct Preview {
    state: Entity<EditorState>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pan: Point<Pixels>,
    gesture: Option<MoveGesture>,
    focus: FocusHandle,
}
fn point_in_quad(p: [f64; 2], corners: [[f64; 2]; 4]) -> bool {
    let mut positive = false;
    let mut negative = false;
    let mut area = 0.0;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        positive |= cross > 0.0;
        negative |= cross < 0.0;
        area += a[0] * b[1] - b[0] * a[1];
    }
    area.abs() > 0.001 && !(positive && negative)
}
fn geometry(
    bounds: Bounds<Pixels>,
    width: u32,
    height: u32,
    zoom: Option<f32>,
    pan: Point<Pixels>,
) -> (f32, Point<Pixels>) {
    let fit = ((f32::from(bounds.size.width) - 48.0).max(1.0) / width as f32)
        .min((f32::from(bounds.size.height) - 48.0).max(1.0) / height as f32);
    let zoom = zoom.unwrap_or(fit);
    (
        zoom,
        point(
            bounds.left() + px((f32::from(bounds.size.width) - width as f32 * zoom) / 2.0) + pan.x,
            bounds.top() + px((f32::from(bounds.size.height) - height as f32 * zoom) / 2.0) + pan.y,
        ),
    )
}
impl Preview {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            bounds: Rc::new(Cell::new(None)),
            pan: point(px(0.0), px(0.0)),
            gesture: None,
            focus: cx.focus_handle(),
        }
    }
    fn down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.bounds.get() else {
            return;
        };
        window.focus(&self.focus);
        let state = self.state.read(cx);
        let comp = state.editor.project().composition();
        let frame = state.frame;
        let (zoom, origin) = geometry(
            bounds,
            comp.width(),
            comp.height(),
            state.preview_zoom,
            self.pan,
        );
        let p = [
            f32::from(event.position.x - origin.x) as f64 / zoom as f64,
            f32::from(event.position.y - origin.y) as f64 / zoom as f64,
        ];
        let hit = comp.layers().iter().find(|layer| {
            layer.active_at(frame, comp.duration())
                && !layer.locked()
                && comp
                    .corners_at(layer.id(), frame)
                    .is_some_and(|corners| point_in_quad(p, corners))
        });
        let (layer, position) = hit.map_or((None, [0.0, 0.0]), |layer| {
            (
                Some(layer.id()),
                [
                    layer.property(Property::PositionX).value_at(frame),
                    layer.property(Property::PositionY).value_at(frame),
                ],
            )
        });
        let hand = state.tool == Tool::Hand;
        let inverse_space = layer
            .and_then(|id| comp.position_space(id, frame))
            .and_then(Affine::inverse);
        if !hand && inverse_space.is_none() {
            return;
        }
        if hand || layer.is_some() {
            self.gesture = Some(MoveGesture {
                start: event.position,
                delta: point(px(0.0), px(0.0)),
                layer: if hand { None } else { layer },
                position,
                frame,
                zoom,
                pan: self.pan,
                inverse_space: inverse_space.unwrap_or_default(),
            });
        }
        if !hand && let Some(id) = layer {
            self.state.update(cx, |state, cx| {
                state.dispatch(&Action::Select(id), window, cx)
            });
        }
        cx.notify();
    }
    fn moving(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if let Some(gesture) = &mut self.gesture {
            gesture.delta = event.position - gesture.start;
            if gesture.layer.is_none() {
                self.pan = gesture.pan + gesture.delta;
            }
            cx.notify();
        }
    }
    fn up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(gesture) = self.gesture.take() {
            if let Some(id) = gesture.layer.filter(|_| {
                f32::from(gesture.delta.x).abs() + f32::from(gesture.delta.y).abs() > 1.0
            }) {
                let delta = gesture.inverse_space.vector([
                    f32::from(gesture.delta.x) as f64 / gesture.zoom as f64,
                    f32::from(gesture.delta.y) as f64 / gesture.zoom as f64,
                ]);
                let x = gesture.position[0] + delta[0];
                let y = gesture.position[1] + delta[1];
                self.state.update(cx, |state, cx| {
                    state.dispatch(
                        &Action::Edit(Command::SetPosition {
                            id,
                            frame: gesture.frame,
                            x,
                            y,
                        }),
                        window,
                        cx,
                    )
                });
            }
            cx.notify();
        }
    }
}
impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let comp = state.editor.project().composition().clone();
        let frame = state.frame;
        let selected = state.editor.selected();
        let zoom = state.preview_zoom;
        let checker = state.checkerboard;
        let hand = state.tool == Tool::Hand;
        let title = format!("Composition   {}", comp.name());
        let time = timecode(frame, comp.fps());
        let pan = self.pan;
        let gesture = self.gesture.clone();
        let measured = self.bounds.clone();
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(ui::BG))
            .child(ui::panel_header(title))
            .child(
                div()
                    .h(px(26.0))
                    .flex_none()
                    .px_3()
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .text_color(rgb(ui::MUTED))
                    .child(format!("{}  ›  Active Camera", comp.name())),
            )
            .child(
                div()
                    .id("composition-canvas")
                    .track_focus(&self.focus)
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .when(hand, |s| s.cursor_grab())
                    .when(!hand, |s| s.cursor_crosshair())
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::down))
                    .on_mouse_move(cx.listener(Self::moving))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::up))
                    .child(
                        canvas(
                            move |bounds, _, _| measured.set(Some(bounds)),
                            move |bounds, _, window, _| {
                                let (zoom, origin) =
                                    geometry(bounds, comp.width(), comp.height(), zoom, pan);
                                let stage = Bounds::new(
                                    origin,
                                    size(
                                        px(comp.width() as f32 * zoom),
                                        px(comp.height() as f32 * zoom),
                                    ),
                                );
                                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                    window.paint_quad(fill(stage, rgb(0x000000)));
                                    window.with_content_mask(
                                        Some(ContentMask {
                                            bounds: stage.intersect(&bounds),
                                        }),
                                        |window| {
                                            if checker {
                                                // Draw only visible tiles, even at 800% zoom.
                                                let visible = stage.intersect(&bounds);
                                                let left =
                                                    (f32::from(visible.left() - stage.left())
                                                        / 12.0)
                                                        .floor()
                                                        as i32;
                                                let top = (f32::from(visible.top() - stage.top())
                                                    / 12.0)
                                                    .floor()
                                                    as i32;
                                                let right =
                                                    (f32::from(visible.right() - stage.left())
                                                        / 12.0)
                                                        .ceil()
                                                        as i32;
                                                let bottom =
                                                    (f32::from(visible.bottom() - stage.top())
                                                        / 12.0)
                                                        .ceil()
                                                        as i32;
                                                for y in top..bottom {
                                                    for x in left..right {
                                                        window.paint_quad(fill(
                                                            Bounds::new(
                                                                point(
                                                                    stage.left()
                                                                        + px(x as f32 * 12.0),
                                                                    stage.top()
                                                                        + px(y as f32 * 12.0),
                                                                ),
                                                                size(px(12.0), px(12.0)),
                                                            ),
                                                            rgb(if (x + y) % 2 == 0 {
                                                                0x444444
                                                            } else {
                                                                0x555555
                                                            }),
                                                        ));
                                                    }
                                                }
                                            }
                                            for layer in
                                                comp.layers().iter().rev().filter(|layer| {
                                                    layer.active_at(frame, comp.duration())
                                                })
                                            {
                                                let delta = gesture
                                                    .as_ref()
                                                    // Descendants move with a dragged parent, including hidden parents.
                                                    .filter(|g| {
                                                        g.layer.is_some_and(|id| {
                                                            !comp.can_parent(id, Some(layer.id()))
                                                        })
                                                    })
                                                    .map_or(point(px(0.0), px(0.0)), |g| g.delta);
                                                let corners = comp
                                                    .corners_at(layer.id(), frame)
                                                    .unwrap_or([[0.0; 2]; 4])
                                                    .map(|[x, y]| {
                                                        point(
                                                            origin.x
                                                                + px(x as f32 * zoom)
                                                                + delta.x,
                                                            origin.y
                                                                + px(y as f32 * zoom)
                                                                + delta.y,
                                                        )
                                                    });
                                                let mut shape = PathBuilder::fill();
                                                shape.move_to(corners[0]);
                                                for p in &corners[1..] {
                                                    shape.line_to(*p);
                                                }
                                                shape.close();
                                                let mut color = rgb(layer.color());
                                                color.a = (layer
                                                    .property(Property::Opacity)
                                                    .value_at(frame)
                                                    .clamp(0.0, 100.0)
                                                    / 100.0)
                                                    as f32;
                                                if let Ok(path) = shape.build() {
                                                    window.paint_path(path, color);
                                                }
                                                if selected == Some(layer.id()) {
                                                    let mut outline = PathBuilder::stroke(px(1.0));
                                                    outline.move_to(corners[0]);
                                                    for p in &corners[1..] {
                                                        outline.line_to(*p);
                                                    }
                                                    outline.close();
                                                    if let Ok(path) = outline.build() {
                                                        window.paint_path(path, rgb(ui::BLUE));
                                                    }
                                                    for corner in corners {
                                                        window.paint_quad(fill(
                                                            Bounds::new(
                                                                corner - point(px(2.5), px(2.5)),
                                                                size(px(5.0), px(5.0)),
                                                            ),
                                                            rgb(ui::BLUE),
                                                        ));
                                                    }
                                                    let anchor = comp
                                                        .position_space(layer.id(), frame)
                                                        .unwrap_or_default()
                                                        .point([
                                                            layer
                                                                .property(Property::PositionX)
                                                                .value_at(frame),
                                                            layer
                                                                .property(Property::PositionY)
                                                                .value_at(frame),
                                                        ]);
                                                    let anchor = point(
                                                        origin.x
                                                            + px(anchor[0] as f32 * zoom)
                                                            + delta.x,
                                                        origin.y
                                                            + px(anchor[1] as f32 * zoom)
                                                            + delta.y,
                                                    );
                                                    window.paint_quad(fill(
                                                        Bounds::new(
                                                            anchor - point(px(5.0), px(0.5)),
                                                            size(px(10.0), px(1.0)),
                                                        ),
                                                        rgb(ui::BLUE),
                                                    ));
                                                    window.paint_quad(fill(
                                                        Bounds::new(
                                                            anchor - point(px(0.5), px(5.0)),
                                                            size(px(1.0), px(10.0)),
                                                        ),
                                                        rgb(ui::BLUE),
                                                    ));
                                                }
                                            }
                                        },
                                    );
                                });
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(
                div()
                    .h(px(32.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .border_t_1()
                    .border_color(rgb(ui::BORDER))
                    .child(
                        ui::text_button(
                            "fit-view",
                            zoom.map_or("Fit".into(), |z| format!("{:.0}%", z * 100.0)),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.pan = point(px(0.0), px(0.0));
                            this.state.update(cx, |state, cx| {
                                state.dispatch(&Action::FitPreview, window, cx)
                            });
                        })),
                    )
                    .child(ui::action_tool(
                        "preview-minus",
                        "minus",
                        "Zoom out",
                        &self.state,
                        Action::ZoomPreview(0.5),
                        false,
                    ))
                    .child(ui::action_tool(
                        "preview-plus",
                        "plus",
                        "Zoom in",
                        &self.state,
                        Action::ZoomPreview(2.0),
                        false,
                    ))
                    .child(div().px_2().text_size(px(11.0)).child("Full"))
                    .child(ui::action_tool(
                        "transparency",
                        "square-dashed",
                        "Toggle transparency grid",
                        &self.state,
                        Action::Checkerboard,
                        checker,
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(ui::BLUE))
                            .child(time),
                    )
                    .child(div().w(px(8.0))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hit_test_rejects_outside_and_degenerate_shapes() {
        assert!(point_in_quad(
            [5.0, 5.0],
            [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
        ));
        assert!(!point_in_quad(
            [15.0, 5.0],
            [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
        ));
        assert!(!point_in_quad([5.0, 5.0], [[0.0, 0.0]; 4]));
        assert!(point_in_quad(
            [0.0, 0.0],
            [[0.0, -5.0], [5.0, 0.0], [0.0, 5.0], [-5.0, 0.0]]
        ));
    }
}
