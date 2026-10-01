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
#[path = "transform_gesture.rs"]
mod transform_gesture;
use transform_gesture::{TransformGesture, handles};

#[derive(Clone)]
struct MoveGesture {
    start: Point<Pixels>,
    delta: Point<Pixels>,
    layer: Option<LayerId>,
    targets: Vec<(LayerId, [f64; 2], Affine)>,
    frame: u32,
    zoom: f32,
    pan: Point<Pixels>,
    pointer: [f64; 2],
    transform: Option<TransformGesture>,
    constrained: bool,
    moved: bool,
}
fn move_command(g: &MoveGesture) -> Command {
    if let Some(transform) = &g.transform {
        return transform.command(
            g.frame,
            [
                f32::from(g.delta.x) as f64 / g.zoom as f64,
                f32::from(g.delta.y) as f64 / g.zoom as f64,
            ],
            g.constrained,
        );
    }
    Command::Batch(
        g.targets
            .iter()
            .map(|(id, position, space)| {
                let d = space.vector([
                    f32::from(g.delta.x) as f64 / g.zoom as f64,
                    f32::from(g.delta.y) as f64 / g.zoom as f64,
                ]);
                Command::SetPosition {
                    id: *id,
                    frame: g.frame,
                    x: position[0] + d[0],
                    y: position[1] + d[1],
                }
            })
            .collect(),
    )
}
pub(crate) struct Preview {
    state: Entity<EditorState>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pan: Point<Pixels>,
    gesture: Option<MoveGesture>,
    focus: FocusHandle,
    renderer: crate::rendering::Renderer,
    pending: bool,
    revision: u64,
    ready: Option<(
        libre_effects_core::Project,
        u32,
        u32,
        u64,
        Result<image::RgbaImage, String>,
    )>,
    failed: Option<(libre_effects_core::Project, u32, u32, String)>,
    cached: Option<(
        libre_effects_core::Project,
        u32,
        u32,
        std::sync::Arc<gpui::RenderImage>,
    )>,
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
            renderer: crate::rendering::Renderer::new(),
            pending: false,
            revision: 0,
            ready: None,
            failed: None,
            cached: None,
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
        let handle_hit = comp
            .layers()
            .iter()
            .filter(|l| {
                state.selected_layers.contains(&l.id())
                    && !l.locked()
                    && comp.layer_active(l, frame, true)
            })
            .find_map(|l| {
                let world = comp.world_transform(l.id(), frame)?;
                let points = if state.tool == Tool::Anchor {
                    vec![[
                        l.property(Property::AnchorX).value_at(frame),
                        l.property(Property::AnchorY).value_at(frame),
                    ]]
                } else if state.tool == Tool::Select {
                    handles(l.width(), l.height()).to_vec()
                } else {
                    Vec::new()
                };
                points.iter().enumerate().find_map(|(index, handle)| {
                    let h = world.point(*handle);
                    ((h[0] - p[0]).hypot(h[1] - p[1]) * zoom as f64 <= 7.0)
                        .then_some((l.id(), index))
                })
            });
        let hit = handle_hit.and_then(|(id, _)| comp.layer(id)).or_else(|| {
            comp.layers().iter().find(|layer| {
                comp.layer_active(layer, frame, true)
                    && !layer.locked()
                    && comp
                        .corners_at(layer.id(), frame)
                        .is_some_and(|corners| point_in_quad(p, corners))
            })
        });
        let (layer, _) = hit.map_or((None, [0.0, 0.0]), |layer| {
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
            if layer.is_none() && !event.modifiers.control && !event.modifiers.shift {
                self.state.update(cx, |s, cx| {
                    s.editor.clear_selection();
                    s.selected_layers.clear();
                    s.selected_keys.clear();
                    cx.notify();
                });
            }
            return;
        }
        if !hand && let Some(id) = layer {
            self.state.update(cx, |s, cx| {
                if event.modifiers.control
                    || event.modifiers.shift
                    || !s.selected_layers.contains(&id)
                {
                    s.dispatch(
                        &Action::SelectMany(id, event.modifiers.control, event.modifiers.shift),
                        window,
                        cx,
                    );
                }
            });
        }
        let state = self.state.read(cx);
        let comp = state.editor.project().composition();
        let transform = layer.and_then(|id| match state.tool {
            Tool::Rotate => TransformGesture::rotate(comp, id, frame, p),
            Tool::Anchor => TransformGesture::anchor(comp, id, frame),
            Tool::Select => {
                handle_hit.and_then(|(_, handle)| TransformGesture::scale(comp, id, frame, handle))
            }
            Tool::Hand => None,
        });
        if !hand && state.tool != Tool::Select && transform.is_none() {
            return;
        }
        let candidates: Vec<_> = comp
            .layers()
            .iter()
            .filter(|l| state.selected_layers.contains(&l.id()) && !l.locked())
            .collect();
        let targets = candidates
            .iter()
            .filter(|l| {
                !candidates.iter().any(|parent| {
                    parent.id() != l.id() && !comp.can_parent(parent.id(), Some(l.id()))
                })
            })
            .filter_map(|l| {
                Some((
                    l.id(),
                    [
                        l.property(Property::PositionX).value_at(frame),
                        l.property(Property::PositionY).value_at(frame),
                    ],
                    comp.position_space(l.id(), frame)?.inverse()?,
                ))
            })
            .collect();
        if hand || layer.is_some() {
            self.gesture = Some(MoveGesture {
                start: event.position,
                delta: point(px(0.0), px(0.0)),
                layer: if hand { None } else { layer },
                targets,
                frame,
                zoom,
                pan: self.pan,
                pointer: p,
                transform,
                constrained: event.modifiers.shift,
                moved: false,
            });
            self.state
                .update(cx, |s, cx| s.dispatch(&Action::Seek(frame), window, cx));
        }
        cx.notify();
    }
    fn moving(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if let Some(gesture) = &mut self.gesture {
            gesture.delta = event.position - gesture.start;
            gesture.moved |=
                f32::from(gesture.delta.x).abs() + f32::from(gesture.delta.y).abs() > 1.0;
            gesture.constrained = event.modifiers.shift;
            if let Some(transform) = &mut gesture.transform {
                transform.update([
                    gesture.pointer[0] + f32::from(gesture.delta.x) as f64 / gesture.zoom as f64,
                    gesture.pointer[1] + f32::from(gesture.delta.y) as f64 / gesture.zoom as f64,
                ]);
            }
            if gesture.layer.is_none() {
                self.pan = gesture.pan + gesture.delta;
            }
            cx.notify();
        }
    }
    fn up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(mut gesture) = self.gesture.take() {
            gesture.delta = event.position - gesture.start;
            gesture.moved |=
                f32::from(gesture.delta.x).abs() + f32::from(gesture.delta.y).abs() > 1.0;
            gesture.constrained = event.modifiers.shift;
            if let Some(transform) = &mut gesture.transform {
                transform.update([
                    gesture.pointer[0] + f32::from(gesture.delta.x) as f64 / gesture.zoom as f64,
                    gesture.pointer[1] + f32::from(gesture.delta.y) as f64 / gesture.zoom as f64,
                ]);
            }
            if gesture.layer.is_none() {
                self.pan = gesture.pan + gesture.delta;
            }
            if gesture.layer.is_some() && gesture.moved {
                let command = move_command(&gesture);
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::Edit(command), window, cx));
            }
            cx.notify();
        }
    }
}
impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let comp = state.editor.project().composition().clone();
        let frame = state.frame;
        let selected = state.selected_layers.clone();
        let zoom = state.preview_zoom;
        let checker = state.checkerboard;
        let resolution = state.preview_resolution;
        let revision = state.preview_revision;
        let playing = state.playing;
        let max_dimension = (comp.width().max(comp.height()).min(1280) / resolution).max(1);
        let hand = state.tool == Tool::Hand;
        let active_composition = state.editor.project().active_composition_id();
        let tabs: Vec<_> = state
            .editor
            .project()
            .compositions()
            .into_iter()
            .map(|(id, comp)| (id, comp.name().to_string()))
            .collect();
        let time = timecode(frame, comp.fps());
        let pan = self.pan;
        let gesture = self.gesture.clone();
        let mut render_project = state.editor.project().clone();
        if let Some(g) = &gesture
            && g.layer.is_some()
        {
            let mut temporary = libre_effects_core::Editor::default();
            let _ = temporary.replace_project(render_project.clone());
            let _ = temporary.execute(move_command(g));
            render_project = temporary.project().clone();
        }
        let comp = render_project.composition().clone();
        if self.revision != revision {
            self.revision = revision;
            self.failed = None;
            if let Some((_, _, _, old)) = self.cached.take() {
                let _ = window.drop_image(old);
            }
        }
        if let Some((project, ready_frame, dimension, generation, result)) = self.ready.take() {
            if project == render_project
                && dimension == max_dimension
                && generation == revision
                && (playing || ready_frame == frame)
            {
                if let Some((_, _, _, old)) = self.cached.take() {
                    let _ = window.drop_image(old);
                }
                match result {
                    Ok(mut pixels) => {
                        for p in pixels.pixels_mut() {
                            p.0.swap(0, 2);
                        }
                        self.cached = Some((
                            project,
                            ready_frame,
                            dimension,
                            std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
                                pixels,
                            )])),
                        ));
                        self.failed = None;
                    }
                    Err(e) => self.failed = Some((project, ready_frame, dimension, e)),
                }
            }
        }
        if self.cached.as_ref().is_none_or(|(p, f, dimension, _)| {
            p != &render_project || *f != frame || *dimension != max_dimension
        }) {
            let has_video = comp.layers().iter().any(|l| {
                matches!(
                    l.content(),
                    libre_effects_core::Content::Video { .. }
                        | libre_effects_core::Content::Composition { .. }
                )
            });
            if has_video {
                if self
                    .cached
                    .as_ref()
                    .is_some_and(|(p, _, _, _)| p != &render_project)
                {
                    if let Some((_, _, _, old)) = self.cached.take() {
                        let _ = window.drop_image(old);
                    }
                }
                let failed = self.failed.as_ref().is_some_and(|(p, f, d, _)| {
                    p == &render_project && *f == frame && *d == max_dimension
                });
                if !self.pending && !failed {
                    self.pending = true;
                    let project = render_project.clone();
                    cx.spawn(async move |entity, cx| {
                        let worker_project = project.clone();
                        let result = cx
                            .background_executor()
                            .spawn(async move {
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    crate::rendering::Renderer::new().render_preview(
                                        &worker_project,
                                        frame,
                                        max_dimension,
                                    )
                                }))
                                .unwrap_or_else(|_| Err("Video preview failed".into()))
                            })
                            .await;
                        let _ = entity.update(cx, |s, cx| {
                            s.pending = false;
                            s.ready = Some((project, frame, max_dimension, revision, result));
                            cx.notify();
                        });
                    })
                    .detach();
                }
            } else {
                self.failed = None;
                if let Some((_, _, _, old)) = self.cached.take() {
                    let _ = window.drop_image(old);
                }
                match self
                    .renderer
                    .render_preview(&render_project, frame, max_dimension)
                {
                    Ok(mut pixels) => {
                        for pixel in pixels.pixels_mut() {
                            pixel.0.swap(0, 2);
                        }
                        self.cached = Some((
                            render_project.clone(),
                            frame,
                            max_dimension,
                            std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
                                pixels,
                            )])),
                        ));
                    }
                    Err(error) => {
                        self.cached = None;
                        self.failed = Some((render_project.clone(), frame, max_dimension, error));
                    }
                }
            }
        }
        let error = self
            .failed
            .as_ref()
            .filter(|(p, f, _, _)| p == &render_project && *f == frame)
            .map(|(_, _, _, e)| e.clone());
        let rendered = self.cached.as_ref().map(|(_, _, _, image)| image.clone());
        let measured = self.bounds.clone();
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(ui::BG))
            .child(
                div()
                    .id("composition-tabs")
                    .flex()
                    .flex_none()
                    .h(px(27.0))
                    .overflow_x_scroll()
                    .border_b_1()
                    .border_color(rgb(ui::BORDER))
                    .children(tabs.into_iter().map(|(id, name)| {
                        let state = self.state.clone();
                        ui::text_button(
                            gpui::SharedString::from(format!("composition-tab-{id}")),
                            format!("Composition   {name}"),
                        )
                        .flex_none()
                        .text_size(px(11.0))
                        .px_3()
                        .when(id == active_composition, |s| {
                            s.bg(rgb(0x343434)).border_b_1().border_color(rgb(ui::BLUE))
                        })
                        .on_click(move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(&Action::ActivateComposition(id), window, cx)
                            })
                        })
                    })),
            )
            .when_some(error, |s, error| {
                s.child(
                    div()
                        .px_3()
                        .py_2()
                        .max_h(px(74.0))
                        .overflow_hidden()
                        .text_color(rgb(0xf0b5b5))
                        .child(error),
                )
            })
            .child(
                div()
                    .h(px(26.0))
                    .flex_none()
                    .px_3()
                    .flex()
                    .items_center()
                    .text_size(px(11.0))
                    .text_color(rgb(ui::MUTED))
                    .child(format!(
                        "{}  ›  Active Camera{}",
                        comp.name(),
                        if self.pending {
                            "  ·  Decoding footage…"
                        } else {
                            ""
                        }
                    )),
            )
            .child(
                div()
                    .id("composition-canvas")
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "escape"
                            && let Some(gesture) = this.gesture.take()
                        {
                            this.pan = gesture.pan;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
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
                                    window.paint_quad(fill(stage, rgb(comp.background_color())));
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
                                            if let Some(image) = rendered.clone() {
                                                let _ = window.paint_image(
                                                    stage,
                                                    Default::default(),
                                                    image,
                                                    0,
                                                    false,
                                                );
                                            }
                                            for layer in
                                                comp.layers().iter().rev().filter(|layer| {
                                                    comp.layer_active(layer, frame, true)
                                                })
                                            {
                                                let corners = comp
                                                    .corners_at(layer.id(), frame)
                                                    .unwrap_or([[0.0; 2]; 4])
                                                    .map(|[x, y]| {
                                                        point(
                                                            origin.x + px(x as f32 * zoom),
                                                            origin.y + px(y as f32 * zoom),
                                                        )
                                                    });
                                                if selected.contains(&layer.id())
                                                    || matches!(
                                                        layer.content(),
                                                        libre_effects_core::Content::Null
                                                    )
                                                {
                                                    let mut outline = PathBuilder::stroke(px(1.0));
                                                    outline.move_to(corners[0]);
                                                    for p in &corners[1..] {
                                                        outline.line_to(*p);
                                                    }
                                                    outline.close();
                                                    if let Ok(path) = outline.build() {
                                                        window.paint_path(
                                                            path,
                                                            rgb(
                                                                if selected.contains(&layer.id()) {
                                                                    ui::BLUE
                                                                } else {
                                                                    layer.color()
                                                                },
                                                            ),
                                                        );
                                                    }
                                                    if !selected.contains(&layer.id()) {
                                                        continue;
                                                    }
                                                    let world = comp
                                                        .world_transform(layer.id(), frame)
                                                        .unwrap_or_default();
                                                    for handle in
                                                        handles(layer.width(), layer.height())
                                                    {
                                                        let [x, y] = world.point(handle);
                                                        let corner = point(
                                                            origin.x + px(x as f32 * zoom),
                                                            origin.y + px(y as f32 * zoom),
                                                        );
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
                                                        origin.x + px(anchor[0] as f32 * zoom),
                                                        origin.y + px(anchor[1] as f32 * zoom),
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
                    .child(
                        ui::text_button(
                            "preview-resolution",
                            match resolution {
                                2 => "Half ▾",
                                4 => "Quarter ▾",
                                _ => "Full ▾",
                            },
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.state.update(cx, |s, cx| {
                                s.dispatch(&Action::CyclePreviewResolution, window, cx)
                            })
                        })),
                    )
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
