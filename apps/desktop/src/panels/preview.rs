use crate::{
    components::TextField,
    editor::{Action, EditorState, Tool},
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
use crate::viewer_tools::{self, Channel, RULER, ViewOption};
use libre_effects_core::{Guide, GuideAxis};
use transform_gesture::{TransformGesture, handles};
#[path = "preview_render.rs"]
mod preview_render;
#[path = "shape_gesture.rs"]
mod shape_gesture;
use preview_render::Request;
use shape_gesture::ShapeGesture;
#[path = "gradient_gesture.rs"]
mod gradient_gesture;
#[path = "text_box.rs"]
mod text_box;
#[path = "text_input.rs"]
mod text_input;

#[derive(Clone)]
struct GuideGesture {
    original: Vec<Guide>,
    index: Option<usize>,
    guide: Guide,
    revision: u64,
    composition: libre_effects_core::CompositionId,
}

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
    snap_points: Vec<[f64; 2]>,
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
    gradient_drag: Option<gradient_gesture::Gesture>,
    gradient_point: usize,
    gradient_focus_target: Option<crate::color_edit::GradientTarget>,
    gradient_focus_watch: Option<[gpui::Subscription; 2]>,
    state: Entity<EditorState>,
    text_dragging: bool,
    text_was_active: bool,
    text_box_drag: Option<text_box::TextBoxDrag>,
    text_resizing: bool,
    text_resize_handle: Rc<Cell<Option<Bounds<Pixels>>>>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    gesture: Option<MoveGesture>,
    drawing: Option<ShapeGesture>,
    pen: super::pen::Pen,
    focus: FocusHandle,
    guide_gesture: Option<GuideGesture>,
    options_open: bool,
    channels_open: bool,
    menu_focus: FocusHandle,
    menu_index: usize,
    raw: Option<std::sync::Arc<image::RgbaImage>>,
    ram: crate::preview_cache::Cache,
    warming: Option<Request>,
    display_channel: Channel,
    renderer: std::sync::Arc<crate::rendering::Renderer>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pending: Option<Request>,
    gradient_render_context: Option<preview_render::GradientContext>,
    decoder_revision: u64,
    ready: Option<(Request, Result<image::RgbaImage, String>)>,
    failed: Option<(libre_effects_core::Project, u32, u32, String)>,
    cached: Option<(
        libre_effects_core::Project,
        u32,
        u32,
        std::sync::Arc<gpui::RenderImage>,
    )>,
}
/// No canvas, ruler, text, guide, or toolbar-menu gesture may run behind either
/// isolated geometry/paint modal, including a late release from an earlier drag.
fn preview_modal_active(state: &EditorState) -> bool {
    state.gradient_editor.is_some() || state.vertex_editor.is_some()
}
/// Outside mouse-up runs during GPUI capture, including when a modal occludes
/// the canvas. A blocked release must remain untouched so the modal receives its
/// own button click. Only invoke the canvas handler when no modal owns input.
fn route_preview_release(modal_active: bool, release: impl FnOnce()) {
    if !modal_active {
        release();
    }
}
fn restore_vertex_return(pen: &mut super::pen::Pen, state: &mut EditorState, active: bool) -> bool {
    // Taking first makes inactive/stale returns terminal, even if a future render
    // restores the same project or the window becomes active again.
    state
        .vertex_return
        .take()
        .is_some_and(|request| active && pen.restore_vertex(&request, state))
}
fn vertex_session(state: &EditorState) -> Option<&super::vertex_editor::Session> {
    state
        .vertex_editor
        .as_ref()
        .filter(|session| session.current(state))
}
fn vertex_overlay(
    state: &EditorState,
) -> Vec<(
    libre_effects_core::VectorPath,
    Affine,
    bool,
    std::collections::BTreeSet<usize>,
)> {
    vertex_session(state)
        .map(|session| {
            let request = session.request();
            vec![(
                session.path().clone(),
                request.world,
                matches!(request.target, libre_effects_core::PathTarget::Mask(_)),
                [request.index].into(),
            )]
        })
        .unwrap_or_default()
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
    rulers: bool,
) -> (f32, Point<Pixels>) {
    let bounds = if rulers {
        Bounds::new(
            bounds.origin + point(px(RULER), px(RULER)),
            size(
                (bounds.size.width - px(RULER)).max(px(1.0)),
                (bounds.size.height - px(RULER)).max(px(1.0)),
            ),
        )
    } else {
        bounds
    };
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
fn pen_view(bounds: Option<Bounds<Pixels>>, state: &EditorState) -> Option<super::pen::View> {
    let bounds = bounds?;
    let comp = state.editor.project().composition();
    let (zoom, origin) = geometry(
        bounds,
        comp.width(),
        comp.height(),
        state.preview_zoom,
        point(px(state.preview_pan[0]), px(state.preview_pan[1])),
        state.viewer.rulers,
    );
    Some(super::pen::View::new(bounds, origin, zoom, state))
}
fn controls_active(
    comp: &libre_effects_core::Composition,
    layer: &libre_effects_core::Layer,
    frame: u32,
    selected: bool,
) -> bool {
    !matches!(layer.content(), libre_effects_core::Content::Audio { .. })
        && (comp.layer_active(layer, frame, true)
            || (selected
                && frame >= layer.in_frame()
                && frame < layer.out_frame(comp.duration())
                && comp
                    .layers()
                    .iter()
                    .any(|l| l.track_matte().is_some_and(|m| m.source == layer.id()))))
}

impl Preview {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        Self::watch_media(cx);
        Self {
            gradient_drag: None,
            gradient_point: 0,
            gradient_focus_target: None,
            gradient_focus_watch: None,
            state,
            text_dragging: false,
            text_was_active: false,
            text_box_drag: None,
            text_resizing: false,
            text_resize_handle: Rc::new(Cell::new(None)),
            bounds: Rc::new(Cell::new(None)),
            gesture: None,
            drawing: None,
            pen: Default::default(),
            focus: cx.focus_handle(),
            guide_gesture: None,
            options_open: false,
            channels_open: false,
            menu_focus: cx.focus_handle(),
            menu_index: 0,
            raw: None,
            ram: Default::default(),
            warming: None,
            display_channel: Channel::Rgb,
            renderer: std::sync::Arc::new(crate::rendering::Renderer::with_cancel(cancel.clone())),
            cancel,
            pending: None,
            gradient_render_context: None,
            decoder_revision: 0,
            ready: None,
            failed: None,
            cached: None,
        }
    }
    fn menu_key(&mut self, event: &gpui::KeyUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if preview_modal_active(self.state.read(cx)) {
            cx.stop_propagation();
            return;
        }
        let count = if self.channels_open {
            Channel::ALL.len()
        } else {
            ViewOption::ALL.len() + 1
        };
        match event.keystroke.key.as_str() {
            "escape" => {
                self.channels_open = false;
                self.options_open = false;
                window.focus(&self.focus);
            }
            "up" => self.menu_index = (self.menu_index + count - 1) % count,
            "down" => self.menu_index = (self.menu_index + 1) % count,
            "enter" | "space" => {
                let action = if self.channels_open {
                    Some(Action::PreviewChannel(Channel::ALL[self.menu_index]))
                } else {
                    Some(
                        ViewOption::ALL
                            .get(self.menu_index)
                            .map_or(Action::ClearGuides, |option| Action::ViewerOption(*option)),
                    )
                };
                if let Some(action) = action {
                    self.state
                        .update(cx, |s, cx| s.dispatch(&action, window, cx));
                }
                if self.channels_open {
                    self.channels_open = false;
                    window.focus(&self.focus);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn cache_pixels(
        &mut self,
        project: libre_effects_core::Project,
        frame: u32,
        dimension: u32,
        pixels: std::sync::Arc<image::RgbaImage>,
        channel: Channel,
        window: &mut Window,
    ) {
        if let Some((_, _, _, old)) = self.cached.take() {
            let _ = window.drop_image(old);
        }
        let display = channel.display(&pixels);
        self.raw = Some(pixels);
        self.display_channel = channel;
        self.cached = Some((
            project,
            frame,
            dimension,
            std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(display)])),
        ));
    }
    fn guide_update(&mut self, position: Point<Pixels>, cx: &Context<Self>) {
        let Some(bounds) = self.bounds.get() else {
            return;
        };
        let state = self.state.read(cx);
        let comp = state.editor.project().composition();
        let (zoom, origin) = geometry(
            bounds,
            comp.width(),
            comp.height(),
            state.preview_zoom,
            point(px(state.preview_pan[0]), px(state.preview_pan[1])),
            state.viewer.rulers,
        );
        if let Some(g) = &mut self.guide_gesture {
            let value = match g.guide.axis {
                GuideAxis::Vertical => f32::from(position.x - origin.x),
                GuideAxis::Horizontal => f32::from(position.y - origin.y),
            } / zoom;
            g.guide.position = f64::from(value).round().clamp(-32768.0, 32768.0);
        }
    }
    fn snap_move(&self, delta: Point<Pixels>, alt: bool, cx: &Context<Self>) -> Point<Pixels> {
        let Some(g) = &self.gesture else {
            return delta;
        };
        if g.layer.is_none() || g.transform.is_some() {
            return delta;
        }
        // Merely selecting a layer near a guide must not move it.
        if !g.moved && f32::from(delta.x).abs() + f32::from(delta.y).abs() <= 1.0 {
            return delta;
        }
        let s = self.state.read(cx);
        let snapped = viewer_tools::snap_delta(
            &g.snap_points,
            [
                f64::from(f32::from(delta.x) / g.zoom),
                f64::from(f32::from(delta.y) / g.zoom),
            ],
            g.zoom,
            s.editor.project().composition().guides(),
            &s.viewer,
            alt,
        );
        point(
            px(snapped[0] as f32 * g.zoom),
            px(snapped[1] as f32 * g.zoom),
        )
    }
    fn sample_pointer(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let info = (|| {
            let bounds = self.bounds.get()?;
            if !bounds.contains(&position) {
                return None;
            }
            let state = self.state.read(cx);
            if state.viewer.rulers
                && (position.x < bounds.left() + px(RULER) || position.y < bounds.top() + px(RULER))
            {
                return None;
            }
            let (project, frame, _, _) = self.cached.as_ref()?;
            if project != state.editor.project() || *frame != state.frame {
                return None;
            }
            let comp = project.composition();
            let (zoom, origin) = geometry(
                bounds,
                comp.width(),
                comp.height(),
                state.preview_zoom,
                point(px(state.preview_pan[0]), px(state.preview_pan[1])),
                state.viewer.rulers,
            );
            let pixel = viewer_tools::sample(
                self.raw.as_ref()?,
                [comp.width(), comp.height()],
                [
                    f64::from(f32::from(position.x - origin.x) / zoom),
                    f64::from(f32::from(position.y - origin.y) / zoom),
                ],
                *frame,
            )?;
            Some((
                state.document_revision,
                project.active_composition_id(),
                pixel,
            ))
        })();
        if self.state.read(cx).pixel_info != info {
            self.state.update(cx, |s, cx| {
                s.pixel_info = info;
                cx.notify();
            });
        }
    }
    // Gradient input retains its existing mapping helper and validity guard.
    // Pen pointer events use the frozen-view adapter instead.
    fn pen_pointer(&self, position: Point<Pixels>, cx: &Context<Self>) -> Option<[f64; 2]> {
        let s = self.state.read(cx);
        let c = s.editor.project().composition();
        let (zoom, origin) = geometry(
            self.bounds.get()?,
            c.width(),
            c.height(),
            s.preview_zoom,
            point(px(s.preview_pan[0]), px(s.preview_pan[1])),
            s.viewer.rulers,
        );
        Some([
            f32::from(position.x - origin.x) as f64 / zoom as f64,
            f32::from(position.y - origin.y) as f64 / zoom as f64,
        ])
    }
    fn down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if preview_modal_active(self.state.read(cx)) {
            cx.stop_propagation();
            return;
        }
        // Even a second down routed to rulers or another canvas editor must
        // discard the old Pen pointer without publishing provisional selection.
        let state = self.state.read(cx);
        self.pen.reset_if_stale(state);
        self.pen.validate_view(pen_view(self.bounds.get(), state));
        self.pen.abandon_pointer();
        if self.state.read(cx).colors.picking() {
            self.sample_pointer(event.position, cx);
            self.state.update(cx, |s, cx| {
                if let Some((revision, composition, pixel)) = &s.pixel_info
                    && *revision == s.document_revision
                    && *composition == s.editor.project().active_composition_id()
                    && pixel.frame == s.frame
                {
                    let rgba = pixel.rgba;
                    s.dispatch(&Action::SampleColor(rgba), window, cx);
                } else {
                    s.status =
                        "Wait for the current frame, then click inside the Composition.".into();
                    cx.notify();
                }
            });
            cx.stop_propagation();
            return;
        }

        let Some(bounds) = self.bounds.get() else {
            self.pen.validate_view(None);
            return;
        };
        crate::components::TextField::commit_active(window, cx);
        window.focus(&self.focus);
        let state = self.state.read(cx);
        let comp = state.editor.project().composition();
        let frame = state.frame;
        let pan = point(px(state.preview_pan[0]), px(state.preview_pan[1]));
        let (zoom, origin) = geometry(
            bounds,
            comp.width(),
            comp.height(),
            state.preview_zoom,
            pan,
            state.viewer.rulers,
        );
        let p = [
            f32::from(event.position.x - origin.x) as f64 / zoom as f64,
            f32::from(event.position.y - origin.y) as f64 / zoom as f64,
        ];
        if state.viewer.rulers
            && (event.position.x < bounds.left() + px(RULER)
                || event.position.y < bounds.top() + px(RULER))
        {
            if !state.viewer.lock_guides {
                let axis = if event.position.y < bounds.top() + px(RULER) {
                    GuideAxis::Horizontal
                } else {
                    GuideAxis::Vertical
                };
                self.guide_gesture = Some(GuideGesture {
                    original: comp.guides().to_vec(),
                    index: None,
                    guide: Guide {
                        axis,
                        position: 0.0,
                    },
                    revision: state.document_revision,
                    composition: state.editor.project().active_composition_id(),
                });
                self.state.update(cx, |s, cx| {
                    s.viewer.guides = true;
                    cx.notify();
                });
                self.guide_update(event.position, cx);
            }
            cx.notify();
            return;
        }
        if state.text_session.is_some() {
            if self
                .text_resize_handle
                .get()
                .is_some_and(|b| b.contains(&event.position))
            {
                self.text_resizing = true;
                cx.stop_propagation();
                return;
            }
            self.text_click(event, cx);
            self.text_dragging = true;
            cx.stop_propagation();
            return;
        }
        if let Some(overlay) = gradient_gesture::Overlay::current(state, state.editor.project())
            && let Some(index) = overlay.hit(p, zoom, self.gradient_point)
        {
            self.gradient_point = index;
            self.gradient_drag = Some(gradient_gesture::Gesture::new(
                overlay, index, p, bounds, state,
            ));
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if state.tool == Tool::Text || (state.tool == Tool::Select && event.click_count == 2) {
            let id = comp
                .layers()
                .iter()
                .find(|l| {
                    let libre_effects_core::Content::Text { text, font_size } = l.content() else {
                        return false;
                    };
                    !l.locked()
                        && comp.layer_active(l, frame, true)
                        && comp
                            .world_transform(l.id(), frame)
                            .and_then(|m| m.inverse())
                            .is_some_and(|m| {
                                if text.is_empty() || l.text_style().paragraph {
                                    let local = m.point(p);
                                    (0.0..=l.width()).contains(&local[0])
                                        && (0.0..=l.height()).contains(&local[1])
                                } else {
                                    crate::text_edit::layout::Layout::shape(
                                        text,
                                        *font_size,
                                        l.width(),
                                        &l.text_style(),
                                    )
                                    .contains(m.point(p))
                                }
                            })
                })
                .map(|l| l.id())
                .filter(|_| !(state.tool == Tool::Text && event.modifiers.shift));
            if id.is_some() || state.tool == Tool::Text {
                if id.is_none() {
                    self.text_box_drag = Some(text_box::TextBoxDrag::new(p, origin, zoom, state));
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                let place_caret = state.tool == Tool::Text && id.is_some();
                crate::components::TextField::commit_active(window, cx);
                self.state.update(cx, |s, cx| {
                    s.dispatch(&Action::BeginText(id, p), window, cx)
                });
                if place_caret {
                    self.text_pointer(event.position, false, cx);
                    self.text_dragging = true;
                }
                cx.stop_propagation();
                return;
            }
        }
        if state.tool == Tool::Pen {
            let command = self.pen.pointer_down(
                state,
                event.position,
                Some(super::pen::View::new(bounds, origin, zoom, state)),
                event.modifiers,
            );
            self.state.update(cx, |s, cx| {
                s.dispatch(&Action::Seek(frame), window, cx);
                if let Some(command) = command {
                    s.dispatch(&Action::Edit(command), window, cx);
                }
            });
            cx.notify();
            return;
        }
        if let Tool::Shape(kind) = state.tool {
            self.drawing = Some(ShapeGesture::new(kind, p, origin, zoom, state));
            self.state
                .update(cx, |s, cx| s.dispatch(&Action::Seek(frame), window, cx));
            cx.notify();
            return;
        }
        if state.tool == Tool::Zoom {
            self.state.update(cx, |s, cx| {
                s.dispatch(
                    &Action::ZoomPreview(if event.modifiers.alt { 0.5 } else { 2.0 }),
                    window,
                    cx,
                )
            });
            return;
        }
        if state.viewer.guides && !state.viewer.lock_guides && state.tool != Tool::Hand {
            if let Some((index, guide)) = comp.guides().iter().enumerate().find(|(_, g)| {
                (g.position - p[if g.axis == GuideAxis::Vertical { 0 } else { 1 }]).abs()
                    * f64::from(zoom)
                    <= 5.0
            }) {
                self.guide_gesture = Some(GuideGesture {
                    original: comp.guides().to_vec(),
                    index: Some(index),
                    guide: *guide,
                    revision: state.document_revision,
                    composition: state.editor.project().active_composition_id(),
                });
                cx.notify();
                return;
            }
        }
        let handle_hit = comp
            .layers()
            .iter()
            .filter(|l| {
                state.selected_layers.contains(&l.id())
                    && !l.locked()
                    && controls_active(comp, l, frame, true)
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
        let hit = handle_hit
            .and_then(|(id, _)| comp.layer(id))
            .or_else(|| {
                comp.layers().iter().find(|l| {
                    state.selected_layers.contains(&l.id())
                        && !l.locked()
                        && !comp.layer_active(l, frame, true)
                        && controls_active(comp, l, frame, true)
                        && comp
                            .corners_at(l.id(), frame)
                            .is_some_and(|corners| point_in_quad(p, corners))
                })
            })
            .or_else(|| {
                comp.layers().iter().find(|layer| {
                    comp.layer_active(layer, frame, true)
                        && !matches!(layer.content(), libre_effects_core::Content::Audio { .. })
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
        if !hand && layer.is_some_and(|id| !state.selected_layers.contains(&id)) {
            return;
        }
        let transform = layer.and_then(|id| match state.tool {
            Tool::Rotate => TransformGesture::rotate(comp, id, frame, p),
            Tool::Anchor => TransformGesture::anchor(comp, id, frame),
            Tool::Select => {
                handle_hit.and_then(|(_, handle)| TransformGesture::scale(comp, id, frame, handle))
            }
            Tool::Text | Tool::Hand | Tool::Zoom | Tool::Shape(_) | Tool::Pen => None,
        });
        let transform = if let Some(transform) = transform {
            match transform.with_selection(
                comp,
                state.selected_layers.iter().copied().collect(),
                frame,
            ) {
                Ok(transform) => Some(transform),
                Err(error) => {
                    self.state.update(cx, |s, cx| {
                        s.status = error;
                        cx.notify();
                    });
                    return;
                }
            }
        } else {
            None
        };
        if !hand && state.tool != Tool::Select && transform.is_none() {
            return;
        }
        let candidates: Vec<_> = comp
            .layers()
            .iter()
            .filter(|l| state.selected_layers.contains(&l.id()) && !l.locked())
            .collect();
        let targets: Vec<_> = candidates
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
        let snap_points = targets
            .iter()
            .filter_map(|(id, _, _)| comp.layer_bounds(*id, frame))
            .flat_map(|[l, t, r, b]| [[l, t], [r, b], [(l + r) / 2.0, (t + b) / 2.0]])
            .collect();
        if hand || layer.is_some() {
            self.gesture = Some(MoveGesture {
                start: event.position,
                delta: point(px(0.0), px(0.0)),
                layer: if hand { None } else { layer },
                targets,
                frame,
                zoom,
                pan,
                pointer: p,
                transform,
                constrained: event.modifiers.shift,
                moved: false,
                snap_points,
            });
            self.state
                .update(cx, |s, cx| s.dispatch(&Action::Seek(frame), window, cx));
        }
        cx.notify();
    }
    fn moving(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if preview_modal_active(self.state.read(cx)) {
            cx.stop_propagation();
            return;
        }
        if self.gradient_drag.is_some() {
            self.update_gradient(
                event.position,
                event.modifiers.shift,
                event.modifiers.alt,
                cx,
            );
            return;
        }
        if self.text_resizing {
            self.resize_text(event.position, cx);
            return;
        }
        if let Some(drag) = &mut self.text_box_drag {
            drag.update(event.position, event.modifiers.alt);
            cx.notify();
            return;
        }
        if self.text_dragging
            && self.state.read(cx).text_session.is_some()
            && event.pressed_button == Some(MouseButton::Left)
        {
            self.text_pointer(event.position, true, cx);
            return;
        }
        self.sample_pointer(event.position, cx);
        if self.state.read(cx).colors.session.is_some()
            || self.state.read(cx).gradient_editor.is_some()
        {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if self.state.read(cx).tool == Tool::Pen {
            let state = self.state.read(cx);
            self.pen.pointer_move(
                state,
                event.position,
                pen_view(self.bounds.get(), state),
                event.modifiers,
            );
            cx.notify();
            return;
        }
        if let Some(drawing) = &mut self.drawing {
            drawing.update(event.position, event.modifiers.shift, event.modifiers.alt);
            cx.notify();
            return;
        }
        if self.guide_gesture.is_some() {
            self.guide_update(event.position, cx);
            cx.notify();
            return;
        }
        let snapped = self
            .gesture
            .as_ref()
            .map(|g| self.snap_move(event.position - g.start, event.modifiers.alt, cx));
        if let Some(gesture) = &mut self.gesture {
            gesture.delta = snapped.unwrap_or(event.position - gesture.start);
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
                let pan = gesture.pan + gesture.delta;
                self.state.update(cx, |s, cx| {
                    s.preview_pan = [f32::from(pan.x), f32::from(pan.y)];
                    cx.notify();
                });
            }
            cx.notify();
        }
    }
    fn cancel_canvas_gestures(&mut self) {
        self.pen.cancel();
        self.gesture = None;
        self.drawing = None;
        self.guide_gesture = None;
        self.gradient_drag = None;
        self.text_box_drag = None;
        self.text_dragging = false;
        self.text_resizing = false;
        self.options_open = false;
        self.channels_open = false;
    }
    fn up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modal_active = preview_modal_active(self.state.read(cx));
        if modal_active {
            // Even a release before the next render abandons earlier held work,
            // without publishing it or consuming the modal's release event.
            self.cancel_canvas_gestures();
            cx.notify();
        }
        route_preview_release(modal_active, || self.finish_pointer_up(event, window, cx));
    }
    fn finish_pointer_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gradient_drag.is_some() {
            self.update_gradient(
                event.position,
                event.modifiers.shift,
                event.modifiers.alt,
                cx,
            );
            if let Some(drag) = self.gradient_drag.take()
                && let Some(command) = drag.command(self.state.read(cx))
            {
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::Edit(command), window, cx));
            }
            cx.notify();
            return;
        }
        if self.text_resizing {
            self.resize_text(event.position, cx);
            self.text_resizing = false;
            return;
        }
        if let Some(mut drag) = self.text_box_drag.take() {
            drag.update(event.position, event.modifiers.alt);
            if drag.valid(self.state.read(cx)) {
                self.state
                    .update(cx, |s, cx| s.dispatch(&drag.action(), window, cx));
            }
            cx.notify();
            return;
        }
        if self.text_dragging {
            self.text_dragging = false;
            return;
        }
        if self.state.read(cx).colors.session.is_some()
            || self.state.read(cx).gradient_editor.is_some()
        {
            return;
        }
        if self.state.read(cx).tool == Tool::Pen {
            let state = self.state.read(cx);
            let command = self.pen.pointer_up(
                state,
                event.position,
                pen_view(self.bounds.get(), state),
                event.modifiers,
            );
            if let Some(command) = command {
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::Edit(command), window, cx));
            }
            cx.notify();
            return;
        }
        if let Some(mut drawing) = self.drawing.take() {
            drawing.update(event.position, event.modifiers.shift, event.modifiers.alt);
            if let Some(command) = drawing.command(self.state.read(cx)) {
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::Edit(command), window, cx));
            }
            cx.notify();
            return;
        }
        if self.guide_gesture.is_some() {
            self.guide_update(event.position, cx);
            let g = self.guide_gesture.take().unwrap();
            let state = self.state.read(cx);
            let comp = state.editor.project().composition();
            if g.revision == state.document_revision
                && g.original == comp.guides()
                && g.composition == state.editor.project().active_composition_id()
                && !state.viewer.lock_guides
            {
                let mut guides = comp.guides().to_vec();
                let keep = self.bounds.get().is_some_and(|b| {
                    b.contains(&event.position)
                        && (!state.viewer.rulers
                            || (event.position.x >= b.left() + px(RULER)
                                && event.position.y >= b.top() + px(RULER)))
                });
                match (g.index, keep) {
                    (Some(i), true) => guides[i] = g.guide,
                    (Some(i), false) => {
                        guides.remove(i);
                    }
                    (None, true) => guides.push(g.guide),
                    _ => {}
                }
                self.state.update(cx, |s, cx| {
                    s.dispatch(&Action::Edit(Command::SetGuides(guides)), window, cx)
                });
            }
            cx.notify();
            return;
        }
        let snapped = self
            .gesture
            .as_ref()
            .map(|g| self.snap_move(event.position - g.start, event.modifiers.alt, cx));
        if let Some(mut gesture) = self.gesture.take() {
            gesture.delta = snapped.unwrap_or(event.position - gesture.start);
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
                let pan = gesture.pan + gesture.delta;
                self.state.update(cx, |s, cx| {
                    s.preview_pan = [f32::from(pan.x), f32::from(pan.y)];
                    cx.notify();
                });
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
        if self.gradient_focus_watch.is_none() {
            self.gradient_focus_watch = Some([
                cx.on_blur(&self.focus.clone(), window, |this, _, cx| {
                    this.gradient_drag = None;
                    this.pen.cancel();
                    cx.notify();
                }),
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.gradient_drag = None;
                        this.pen.cancel();
                        this.state.update(cx, |s, _| {
                            s.vertex_return = None;
                        });
                        cx.notify();
                    }
                }),
            ]);
        }
        if self
            .gradient_drag
            .as_ref()
            .is_some_and(|g| !g.valid(self.state.read(cx)) || self.bounds.get() != Some(g.bounds))
        {
            self.gradient_drag = None;
        }
        let gradient_active = gradient_gesture::Overlay::current(
            self.state.read(cx),
            self.state.read(cx).editor.project(),
        )
        .is_some();
        let gradient_target = if gradient_active {
            self.state.read(cx).gradient_controls
        } else {
            None
        };
        if gradient_active && self.gradient_focus_target != gradient_target {
            window.focus(&self.focus);
        }
        self.gradient_focus_target = gradient_target;
        if self
            .text_box_drag
            .as_ref()
            .is_some_and(|d| !d.valid(self.state.read(cx)))
        {
            self.text_box_drag = None;
        }
        if self
            .state
            .read(cx)
            .text_session
            .as_ref()
            .is_some_and(|session| {
                let state = self.state.read(cx);
                !session.valid(state.editor.project(), state.document_revision, state.frame)
            })
        {
            self.state.update(cx, |s, cx| s.finish_text(false, cx));
            self.text_dragging = false;
        }
        // Ordinary OK/Cancel is the only selection-restoring route. Consume the
        // token even if inactive/stale, so later activation cannot revive it.
        if self.state.read(cx).vertex_return.is_some() {
            if self.state.update(cx, |s, _| {
                restore_vertex_return(&mut self.pen, s, window.is_window_active())
            }) {
                window.focus(&self.focus);
            }
        }
        if preview_modal_active(self.state.read(cx)) {
            self.cancel_canvas_gestures();
        }
        let state = self.state.read(cx);
        self.pen.reset_if_stale(state);
        self.pen.validate_view(pen_view(self.bounds.get(), state));
        if state.text_session.is_some() && !self.text_was_active {
            window.focus(&self.focus);
        }
        self.text_was_active = state.text_session.is_some();
        if !self.text_was_active {
            self.text_resizing = false;
        }
        self.text_resize_handle.set(None);
        if state.welcome() {
            let create = self.state.clone();
            let import = self.state.clone();
            return div()
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(ui::BG))
                .child(ui::panel_header("Composition"))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .child(
                            ui::text_button("welcome-composition", "")
                                .w(px(210.0))
                                .h(px(220.0))
                                .flex_col()
                                .gap_4()
                                .bg(rgb(0x292929))
                                .child(ui::icon("filmstrip").size(px(50.0)))
                                .child("New Composition")
                                .on_click(move |_, _, cx| {
                                    create.update(cx, |s, cx| {
                                        s.new_composition_requested = true;
                                        cx.notify();
                                    })
                                }),
                        )
                        .child(
                            ui::text_button("welcome-footage", "")
                                .w(px(210.0))
                                .h(px(220.0))
                                .flex_col()
                                .gap_4()
                                .bg(rgb(0x292929))
                                .child(ui::icon("folder-open").size(px(50.0)))
                                .child("New Composition From Footage")
                                .on_click(move |_, window, cx| {
                                    import.update(cx, |s, cx| {
                                        s.dispatch(&Action::CompositionFromFootage, window, cx)
                                    })
                                }),
                        ),
                )
                .into_any_element();
        }
        let comp = state.editor.project().composition().clone();
        let frame = state.frame;
        let selected = state.selected_layers.clone();
        let zoom = state.preview_zoom;
        let viewer = state.viewer.clone();
        let channel = viewer.channel;
        let checker = state.checkerboard && channel == Channel::Rgb;
        let mut guides = comp.guides().to_vec();
        if let Some(g) = &self.guide_gesture {
            if g.revision == state.document_revision
                && g.original == comp.guides()
                && g.composition == state.editor.project().active_composition_id()
            {
                if let Some(i) = g.index {
                    guides[i] = g.guide;
                } else {
                    guides.push(g.guide);
                }
            }
        }
        let resolution = state.preview_resolution;
        let revision = state.preview_revision;
        let playing = state.playing;
        let transport = state.transport_generation();
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
        let time = comp.timecode(frame);
        let pan = point(px(state.preview_pan[0]), px(state.preview_pan[1]));
        let gesture = self.gesture.clone();
        let mut render_project = vertex_session(state)
            .map(|session| session.project().clone())
            .or_else(|| {
                state
                    .gradient_editor
                    .as_ref()
                    .and_then(|draft| draft.preview(state))
            })
            .or_else(|| {
                state
                    .gradient_preview
                    .as_ref()
                    .and_then(|draft| draft.preview(state))
            })
            .unwrap_or_else(|| state.text_project());
        let text_session = state.text_session.clone();
        let text_box_rect = self.text_box_drag.as_ref().map(|d| d.rect());
        let text_input = cx.entity();
        let text_resize_handle = self.text_resize_handle.clone();
        let text_focus = self.focus.clone();
        if let Some(g) = &gesture
            && g.layer.is_some()
        {
            let mut temporary = libre_effects_core::Editor::default();
            if temporary.replace_project(render_project.clone()).is_ok()
                && temporary.execute(move_command(g)).is_ok()
            {
                render_project = temporary.project().clone();
            }
        }
        if let Some(command) = self.drawing.as_ref().and_then(|d| d.command(state)) {
            let mut temporary = libre_effects_core::Editor::default();
            if temporary.replace_project(render_project.clone()).is_ok()
                && temporary.execute(command).is_ok()
            {
                render_project = temporary.project().clone();
            }
        }
        if let Some(command) = self.pen.pending(state) {
            let mut temporary = libre_effects_core::Editor::default();
            if temporary.replace_project(render_project.clone()).is_ok()
                && temporary.execute(command).is_ok()
            {
                render_project = temporary.project().clone();
            }
        }
        let pen_overlay = if state.vertex_editor.is_some() {
            vertex_overlay(state)
        } else {
            self.pen.overlay(state)
        };
        let pen_marquee = self.pen.marquee_overlay(state);
        if let Some(command) = self.gradient_drag.as_ref().and_then(|g| g.command(state)) {
            let mut temporary = libre_effects_core::Editor::default();
            if temporary.replace_project(render_project.clone()).is_ok()
                && temporary.execute(command).is_ok()
            {
                render_project = temporary.project().clone();
            }
        }
        let gradient_overlay = gradient_gesture::Overlay::current(state, &render_project);
        let gradient_point = self.gradient_point;
        let pen_active = state.tool == Tool::Pen;
        let pen_order_help = self.pen.order_help(state);
        let vertex_available = self.pen.numeric_vertex_available(state);
        let comp = render_project.composition().clone();
        // Numeric and gradient drafts share the globally unique transient-render
        // generation. Cancel/OK clears the displayed draft before source renders.
        let gradient_gesture = vertex_session(state)
            .map(|session| session.id)
            .or_else(|| {
                state
                    .gradient_editor
                    .as_ref()
                    .filter(|draft| draft.current(state))
                    .map(|draft| draft.id)
            })
            .or_else(|| {
                self.gradient_drag
                    .as_ref()
                    .filter(|g| g.valid(state))
                    .map(|g| g.id)
                    .or_else(|| {
                        state
                            .gradient_preview
                            .as_ref()
                            .filter(|d| d.current(state))
                            .map(|d| d.gesture_id)
                    })
            });
        self.update_render(
            Request {
                project: render_project.clone(),
                frame,
                dimension: max_dimension,
                revision,
                transport,
                gradient_gesture,
            },
            playing,
            channel,
            window,
            cx,
        );
        if channel != self.display_channel {
            if let Some((_, _, _, image)) = &mut self.cached {
                if let Some(raw) = &self.raw {
                    let _ = window.drop_image(image.clone());
                    *image = std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
                        channel.display(raw),
                    )]));
                    self.display_channel = channel;
                }
            }
        }
        let overlay_options = viewer.clone();
        let error = self
            .failed
            .as_ref()
            .filter(|(p, f, _, _)| p == &render_project && *f == frame)
            .map(|(_, _, _, e)| e.clone());
        let rendered = self.cached.as_ref().map(|(_, _, _, image)| image.clone());
        let measured = self.bounds.clone();
        let measured_preview = cx.entity().downgrade();
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
                        if text_session.is_some() { "  ·  Text: Ctrl+Enter finish · Esc cancel" } else if self.state.read(cx).colors.picking() { "  ·  Pick composition color · click to sample · Esc to return" } else if gradient_active { if gradient_point == 0 { "  ·  Gradient Start: drag · Tab switch · arrows move · Alt both · Esc close" } else { "  ·  Gradient End: drag · Tab switch · arrows move · Alt both · Esc close" } } else if pen_active { "  ·  Pen: Shift-click toggle · Shift-drag add box · Ctrl+A path vertices · drag selected · Esc cancel" } else if self.pending.is_some() {
                            "  ·  Rendering…"
                        } else {
                            ""
                        }
                    )),
            )
            .when(pen_active && text_session.is_none() && !gradient_active, |view| {
                view.child(
                    div()
                        .id("pen-path-order-help")
                        .h(px(20.0))
                        .flex_none()
                        .px_3()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .gap_3()
                        .text_size(px(11.0))
                        .text_color(rgb(ui::MUTED))
                        .tooltip(|_, cx| cx.new(|_| ui::Tip("Reorders the base and all animation poses. Curve geometry and key timing stay unchanged. Reverse may change Non-Zero compound fill holes; either action may change stroke dash placement.".into())).into())
                        .child(div().flex_1().min_w_0().overflow_hidden().child(pen_order_help))
                        .child(ui::text_button("pen-edit-vertex", "Edit Vertex…  Shift+V")
                            .h(px(20.0))
                            .flex_none()
                            .when(!vertex_available, |button| button.opacity(0.35))
                            .tooltip(|_, cx| cx.new(|_| ui::Tip("Select exactly one existing Pen vertex. Edit its local anchor and relative tangents; one Undo step on OK.".into())).into())
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                // Prevent the button's default focus before capturing
                                // the live Pen selection; canvas blur still cancels it.
                                window.prevent_default();
                                let request = this.pen.single_vertex_request(this.state.read(cx));
                                if let Some(request) = request {
                                    this.state.update(cx, |s, cx| s.dispatch(&Action::OpenVertex(request), window, cx));
                                }
                                cx.stop_propagation();
                            }))),
                )
            })
            .child(
                div()
                    .id("composition-canvas")
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                        if preview_modal_active(this.state.read(cx)) { cx.stop_propagation(); return; }
                        if this.gradient_key(event, window, cx) { cx.stop_propagation(); return; }
                        if event.keystroke.key=="escape" && this.text_box_drag.take().is_some() {cx.stop_propagation();cx.notify();return;}
                        if this.text_key(event,window,cx) {return;}
                        if this.state.read(cx).colors.session.is_some() || this.state.read(cx).gradient_editor.is_some() { return; }
                        if this.state.read(cx).tool == Tool::Pen {
                            let (handled, request) = this.pen.numeric_vertex_key(
                                event,
                                this.focus.is_focused(window),
                                TextField::is_composing(window, cx),
                                this.state.read(cx),
                            );
                            if handled {
                                if let Some(request) = request { this.state.update(cx, |s, cx| s.dispatch(&Action::OpenVertex(request), window, cx)); }
                                cx.stop_propagation(); cx.notify(); return;
                            }
                            if this.pen.select_all_key(
                                event,
                                this.focus.is_focused(window),
                                TextField::is_composing(window, cx),
                                this.state.read(cx),
                            ) {
                                cx.stop_propagation(); cx.notify(); return;
                            }
                            let (ordered, command) = this.pen.order_key(
                                event,
                                this.focus.is_focused(window),
                                TextField::is_composing(window, cx),
                                this.state.read(cx),
                            );
                            let (handled, command) = if ordered {
                                (true, command)
                            } else {
                                this.pen.key(&event.keystroke.key, this.state.read(cx))
                            };
                            if handled {
                                if let Some(command) = command { this.state.update(cx, |s,cx| s.dispatch(&Action::Edit(command), window, cx)); }
                                cx.stop_propagation(); cx.notify(); return;
                            }
                        }
                        if event.keystroke.key == "escape" && this.drawing.take().is_some() {cx.stop_propagation();cx.notify();return;}
                        if event.keystroke.key == "escape" && this.guide_gesture.take().is_some() {cx.stop_propagation();cx.notify();return;}
                        if event.keystroke.key == "escape"
                            && let Some(gesture) = this.gesture.take()
                        {
                            this.state.update(cx, |s, cx| {
                                s.preview_pan =
                                    [f32::from(gesture.pan.x), f32::from(gesture.pan.y)];
                                cx.notify();
                            });
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
                    .when(!hand && text_session.is_none(), |s| s.cursor_crosshair())
                    .when(text_session.is_some(), |s| s.cursor_text())
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::down))
                    .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {if !preview_modal_active(this.state.read(cx)) { this.state.update(cx,|s,cx|s.finish_text(true,cx)); }}))
                    .on_mouse_move(cx.listener(Self::moving))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::up))
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                if measured.replace(Some(bounds)) != Some(bounds) {
                                    let _ = measured_preview.update(cx, |this, cx| {
                                        // Layout can change fit mapping without a state
                                        // event. Cancel held Pen input before another
                                        // pointer event and repaint its stable source.
                                        this.pen.validate_view(pen_view(Some(bounds), this.state.read(cx)));
                                        cx.notify();
                                    });
                                }
                            },
                            move |bounds, _, window, cx| {
                                let (zoom, origin) =
                                    geometry(bounds, comp.width(), comp.height(), zoom, pan,overlay_options.rulers);
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
                                                    !pen_active && controls_active(
                                                        &comp,
                                                        layer,
                                                        frame,
                                                        selected.contains(&layer.id()),
                                                    )
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
                                    super::pen::paint(&pen_overlay, pen_marquee, origin, zoom, window);
                                    if let Some(overlay) = &gradient_overlay { gradient_gesture::paint(overlay, gradient_point, origin, zoom, window); }
                                    if let Some(r)=text_box_rect {
                                        let b=Bounds::new(origin+point(px(r[0] as f32*zoom),px(r[1] as f32*zoom)),size(px(r[2] as f32*zoom),px(r[3] as f32*zoom)));
                                        window.paint_quad(gpui::outline(b,rgb(ui::BLUE),gpui::BorderStyle::Solid));
                                    }
                                    if let Some(session)=&text_session {text_input::paint(session,origin,zoom,bounds,&text_focus,text_input.clone(),&text_resize_handle,window,cx);}
                                    viewer_tools::paint(&overlay_options,&guides,bounds,stage,zoom,window,cx);
                                });
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(
                div()
                    .relative()
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
                    .child(ui::text_button("viewer-layout-options", "Guides ▾")
                        .on_click(cx.listener(|this,_,w,cx| {if preview_modal_active(this.state.read(cx)) { return; } this.options_open=!this.options_open;this.channels_open=false;this.menu_index=0;w.focus(&this.menu_focus);cx.notify();})))
                    .child(ui::text_button("viewer-channel-options",format!("{} ▾",channel.label()))
                        .on_click(cx.listener(|this,_,w,cx| {if preview_modal_active(this.state.read(cx)) { return; } this.channels_open=!this.channels_open;this.options_open=false;this.menu_index=0;w.focus(&this.menu_focus);cx.notify();})))
                    .when(self.options_open, |toolbar| {
                        let mut menu=div().id("viewer-layout-menu").track_focus(&self.menu_focus).on_key_down(|_,_,cx|cx.stop_propagation()).on_key_up(cx.listener(Self::menu_key)).absolute().bottom(px(32.0)).left(px(164.0)).w(px(250.0)).p_1().bg(rgb(0x2b2b2b)).border_1().border_color(rgb(0x4a4a4a)).shadow_lg().occlude()
                            .on_mouse_down_out(cx.listener(|this,_,_,cx|{this.options_open=false;cx.notify();}));
                        for (index,option) in ViewOption::ALL.into_iter().enumerate() {
                            let label=if option==ViewOption::GridSize {format!("Grid spacing: {:.0} px",viewer.grid_size)} else {format!("{} {}",if viewer.enabled(option) {"✓"} else {"  "},option.label())};
                            menu=menu.child(ui::text_button(("viewer-option",index),label).w_full().justify_start().when(index==self.menu_index,|b|b.bg(rgb(0x164a7b))).on_click(cx.listener(move|this,_,w,cx| {this.state.update(cx,|s,cx|s.dispatch(&Action::ViewerOption(option),w,cx));})));
                        }
                        menu=menu.child(ui::text_button("viewer-clear-guides","Clear guides").w_full().justify_start().when(self.menu_index==8,|b|b.bg(rgb(0x164a7b))).when(viewer.lock_guides,|b|b.opacity(0.35)).on_click(cx.listener(|this,_,w,cx| {this.state.update(cx,|s,cx|s.dispatch(&Action::ClearGuides,w,cx));this.options_open=false;cx.notify();})))
                            .child(div().p_2().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child("Drag from a ruler to add a guide. Drag back to remove. Alt bypasses snapping."));
                        toolbar.child(menu)
                    })
                    .when(self.channels_open, |toolbar| {
                        let mut menu=div().id("viewer-channel-menu").track_focus(&self.menu_focus).on_key_down(|_,_,cx|cx.stop_propagation()).on_key_up(cx.listener(Self::menu_key)).absolute().bottom(px(32.0)).left(px(240.0)).w(px(140.0)).p_1().bg(rgb(0x2b2b2b)).border_1().border_color(rgb(0x4a4a4a)).shadow_lg().occlude()
                            .on_mouse_down_out(cx.listener(|this,_,_,cx|{this.channels_open=false;cx.notify();}));
                        for (index,c) in Channel::ALL.into_iter().enumerate() {
                            menu=menu.child(ui::text_button(("viewer-channel",index),format!("{} {}",if c==channel {"✓"} else {"  "},c.label())).w_full().justify_start().when(index==self.menu_index,|b|b.bg(rgb(0x164a7b))).on_click(cx.listener(move|this,_,w,cx| {this.state.update(cx,|s,cx|s.dispatch(&Action::PreviewChannel(c),w,cx));this.channels_open=false;cx.notify();})));
                        }
                        toolbar.child(menu)
                    })
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(ui::BLUE))
                            .child(time),
                    )
                    .child(div().w(px(8.0))),
            ).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pen_view_snapshot_captures_fit_resize_bounds_origin_and_view_settings() {
        let mut state = EditorState::default();
        let bounds = Bounds::new(point(px(100.), px(200.)), size(px(1200.), px(500.)));
        let original = pen_view(Some(bounds), &state).unwrap();
        assert!(pen_view(None, &state).is_none());
        let mut resized = bounds;
        resized.size.height += px(100.);
        assert!(Some(original) != pen_view(Some(resized), &state));
        let mut moved = bounds;
        moved.origin.x += px(1.);
        assert!(Some(original) != pen_view(Some(moved), &state));
        state.preview_pan = [25., -35.];
        assert!(Some(original) != pen_view(Some(bounds), &state));
        state.preview_pan = [0.; 2];
        state.viewer.rulers = !state.viewer.rulers;
        assert!(Some(original) != pen_view(Some(bounds), &state));
        state.viewer.rulers = !state.viewer.rulers;
        let comp = state.editor.project().composition();
        let (fit, _) = geometry(
            bounds,
            comp.width(),
            comp.height(),
            None,
            point(px(0.), px(0.)),
            state.viewer.rulers,
        );
        state.preview_zoom = Some(fit);
        // An explicit zoom can numerically equal Fit, but its mode is still a
        // different held-gesture context and must not be silently substituted.
        assert!(Some(original) != pen_view(Some(bounds), &state));
    }
    #[test]
    fn fitted_stage_reserves_rulers_and_mapping_survives_zoom_and_pan() {
        let bounds = Bounds::new(point(px(100.0), px(200.0)), size(px(1200.0), px(500.0)));
        let (zoom, origin) = geometry(bounds, 960, 540, None, point(px(0.0), px(0.0)), true);
        assert!(origin.x >= bounds.left() + px(RULER + 24.0));
        assert!(origin.y >= bounds.top() + px(RULER + 24.0));
        assert!(origin.x + px(960.0 * zoom) <= bounds.right() - px(24.0));
        assert!(origin.y + px(540.0 * zoom) <= bounds.bottom() - px(24.0));
        for scale in [0.0625, 0.5, 1.0, 8.0] {
            let (zoom, origin) = geometry(
                bounds,
                960,
                540,
                Some(scale),
                point(px(127.0), px(-54.0)),
                true,
            );
            let p = origin + point(px(321.5 * zoom), px(123.75 * zoom));
            assert!((f32::from(p.x - origin.x) / zoom - 321.5).abs() < 0.001);
            assert!((f32::from(p.y - origin.y) / zoom - 123.75).abs() < 0.001);
        }
    }
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

#[cfg(test)]
#[path = "preview_vertex_tests.rs"]
mod numeric_vertex_tests;
