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
use transform_gesture::{TransformGesture, layer_corners, layer_handles, layer_snap_points};
#[path = "preview_render.rs"]
mod preview_render;
#[path = "shape_gesture.rs"]
mod shape_gesture;
use preview_render::Request;
use shape_gesture::ShapeGesture;
#[cfg(test)]
#[path = "contents_cross_parent_gesture_tests.rs"]
mod contents_cross_parent_gesture_tests;
#[path = "gradient_gesture.rs"]
mod gradient_gesture;
#[path = "../projected_selection.rs"]
mod projected_selection;
#[path = "text_box.rs"]
mod text_box;
#[path = "text_input.rs"]
mod text_input;
#[cfg(test)]
use projected_selection::point_in_quad;
use projected_selection::{projected_control_order, projected_layer_hit};

/// Point text falls back to the layer box only when the current Hold sample
/// is empty. The static baseline may contain a different string.
fn text_layer_hit(layer: &libre_effects_core::Layer, frame: u32, local: [f64; 2]) -> bool {
    let Some(text) = layer.source_text_at(frame) else {
        return false;
    };
    if text.is_empty() || layer.text_style().paragraph {
        (0.0..=layer.width()).contains(&local[0]) && (0.0..=layer.height()).contains(&local[1])
    } else {
        crate::text_edit::layout::Layout::for_layer(layer, frame)
            .is_some_and(|layout| layout.contains(local))
    }
}

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
    retired_images: super::image_retirement::ImageRetirement,
    ram: crate::preview_cache::Cache,
    warming: Option<Request>,
    display_channel: Channel,
    renderer: std::sync::Arc<crate::rendering::Renderer>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pending: Option<Request>,
    gradient_render_context: Option<preview_render::GradientContext>,
    decoder_revision: u64,
    ready: Option<(Request, Result<crate::rendering::RenderedFrame, String>)>,
    // Only the displayed frame retains its evaluated scene, never every RAM frame.
    displayed: Option<(Request, Option<std::sync::Arc<libre_effects_core::Project>>)>,
    failed: Option<(Request, String)>,
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
    state.gradient_editor.is_some()
        || state.vertex_editor.is_some()
        || state.expression_editor.is_some()
}
/// Canvas edits are authored-coordinate operations. Expression scenes remain
/// selectable through their evaluated bounds, while authoring uses explicit base
/// values in the Inspector until evaluated-coordinate editing is implemented.
fn expression_scene_active(state: &EditorState) -> bool {
    state
        .editor
        .project()
        .expression_roots(
            state.editor.project().active_composition_id(),
            state.frame,
            true,
        )
        .map_or(true, |roots| !roots.is_empty())
}
fn spatial_scene_active(state: &EditorState) -> bool {
    state
        .editor
        .project()
        .composition()
        .layers()
        .iter()
        .any(|layer| layer.has_joined_position())
}
fn canvas_read_only_message(state: &EditorState) -> &'static str {
    if spatial_scene_active(state) {
        "Spatial preview: select layers here; edit geometry through scripting. Hand and Zoom remain available"
    } else {
        EXPRESSION_CANVAS_READ_ONLY
    }
}
const EXPRESSION_CANVAS_READ_ONLY: &str = "Expression-driven preview: select evaluated layers here; edit authored values in the Inspector";

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
                request.indices.clone(),
            )]
        })
        .unwrap_or_default()
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
use libre_effects_editor_model::preview_scene::{
    controls_active, selection_has_expression_transform,
};

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
            retired_images: Default::default(),
            ram: Default::default(),
            warming: None,
            display_channel: Channel::Rgb,
            renderer: std::sync::Arc::new(crate::rendering::Renderer::with_cancel(cancel.clone())),
            cancel,
            pending: None,
            gradient_render_context: None,
            decoder_revision: 0,
            ready: None,
            displayed: None,
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
        request: Request,
        evaluated: Option<std::sync::Arc<libre_effects_core::Project>>,
        pixels: std::sync::Arc<image::RgbaImage>,
        channel: Channel,
        window: &mut Window,
    ) {
        if let Some((_, _, _, old)) = self.cached.take() {
            self.retired_images.retire(old, window);
        }
        let display = channel.display(&pixels);
        self.raw = Some(pixels);
        self.display_channel = channel;
        self.cached = Some((
            request.project.clone(),
            request.frame,
            request.dimension,
            std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(display)])),
        ));
        self.displayed = Some((request, evaluated));
    }
    fn canvas_geometry_read_only(&self, state: &EditorState) -> bool {
        spatial_scene_active(state)
            || expression_scene_active(state)
            || selection_has_expression_transform(
                state.editor.project().composition(),
                state.selected_layers.iter().copied(),
            )
            || self
                .displayed
                .as_ref()
                .is_some_and(|(_, view)| view.is_some())
    }
    fn current_scene<'a>(&'a self, state: &EditorState) -> Option<&'a libre_effects_core::Project> {
        let (shown, evaluated) = self.displayed.as_ref()?;
        let comp = state.editor.project().composition();
        let dimension =
            (comp.width().max(comp.height()).min(1280) / state.preview_resolution).max(1);
        if shown.project != *state.editor.project()
            || shown.frame != state.frame
            || shown.dimension != dimension
            || shown.revision != state.preview_revision
            || shown.document_revision != state.document_revision
            || shown.core_generation != state.editor.context_generation()
            || shown.transport != state.transport_generation()
            || shown.gradient_gesture.is_some()
        {
            return None;
        }
        shown.validate_evaluated_view(evaluated.as_deref()).ok()?;
        Some(evaluated.as_deref().unwrap_or(&shown.project))
    }
    fn select_evaluated(
        &mut self,
        event: &MouseDownEvent,
        point: [f64; 2],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        let Some(scene) = self.current_scene(state).filter(|_| !state.playing) else {
            self.state.update(cx, |state, cx| {
                state.status =
                    "Pause and wait for the current rendered frame before selecting".into();
                cx.notify();
            });
            return;
        };
        let comp = scene.composition();
        let frame = state.frame;
        // Hit-test the very same projection and paint order used for pixels.
        // Selection does not require the legacy 2D position-space inverse.
        let hit = match projected_layer_hit(comp, frame, point, &state.selected_layers, |layer| {
            transform_gesture::layer_bounds(layer, frame)
        }) {
            Ok(hit) => hit,
            Err(error) => {
                self.state.update(cx, |state, cx| {
                    state.status = error;
                    cx.notify();
                });
                return;
            }
        };
        self.state.update(cx, |state, cx| {
            if let Some(id) = hit {
                if event.modifiers.control
                    || event.modifiers.shift
                    || !state.selected_layers.contains(&id)
                {
                    state.dispatch(
                        &Action::SelectMany(id, event.modifiers.control, event.modifiers.shift),
                        window,
                        cx,
                    );
                }
            } else if !event.modifiers.control && !event.modifiers.shift {
                state.editor.clear_selection();
                state.selected_layers.clear();
                state.selected_keys.clear();
            }
            state.status = canvas_read_only_message(state).into();
            cx.notify();
        });
    }
    /// Preserve view-only panning, but abandon every draft that could publish an
    /// authored-coordinate edit after an expression becomes active mid-gesture.
    fn cancel_expression_gestures(&mut self) {
        self.gesture = self
            .gesture
            .take()
            .filter(|gesture| gesture.layer.is_none());
        self.pen.cancel();
        self.drawing = None;
        self.guide_gesture = None;
        self.gradient_drag = None;
        self.text_box_drag = None;
        self.text_dragging = false;
        self.text_resizing = false;
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
            let project = self.current_scene(state)?;
            let frame = state.frame;
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
                frame,
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
        if self.state.read(cx).tool == Tool::Pen
            && !self.state.read(cx).colors.picking()
            && self.state.read(cx).text_session.is_none()
            && !super::pen::pointer_input_allowed(
                self.state.read(cx),
                event.button == MouseButton::Left,
                window.is_window_active(),
                TextField::is_composing(window, cx),
                TextField::active_pending_binding(cx).is_some(),
            )
        {
            self.pen.abandon_pointer();
            if TextField::active_pending_binding(cx).is_some()
                || TextField::is_composing(window, cx)
            {
                self.state.update(cx, |s, cx| {
                    s.status =
                        "Finish or cancel the active text field before editing Pen points".into();
                    cx.notify();
                });
            }
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
            return;
        }
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
        self.text_box_drag = None;
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
        if self.canvas_geometry_read_only(state) {
            let tool = state.tool;
            let selection = tool == Tool::Select && state.text_session.is_none();
            self.cancel_expression_gestures();
            if tool == Tool::Hand {
                self.gesture = Some(MoveGesture {
                    start: event.position,
                    delta: point(px(0.), px(0.)),
                    layer: None,
                    targets: Vec::new(),
                    frame,
                    zoom,
                    pan,
                    pointer: p,
                    transform: None,
                    constrained: false,
                    moved: false,
                    snap_points: Vec::new(),
                });
            } else if tool == Tool::Zoom {
                self.state.update(cx, |state, cx| {
                    state.dispatch(
                        &Action::ZoomPreview(if event.modifiers.alt { 0.5 } else { 2.0 }),
                        window,
                        cx,
                    );
                });
            } else if selection {
                self.select_evaluated(event, p, window, cx);
            } else {
                self.state.update(cx, |state, cx| {
                    state.status = canvas_read_only_message(state).into();
                    cx.notify();
                });
            }
            cx.stop_propagation();
            return;
        }
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
                    !l.locked()
                        && comp.layer_active(l, frame, true)
                        && comp
                            .world_transform(l.id(), frame)
                            .and_then(|m| m.inverse())
                            .is_some_and(|m| text_layer_hit(l, frame, m.point(p)))
                })
                .map(|l| l.id())
                .filter(|_| !(state.tool == Tool::Text && event.modifiers.shift));
            if id.is_some() || state.tool == Tool::Text {
                if id.is_none() {
                    // Freeze the visible frame before capturing insertion guards,
                    // just as editing an existing text layer stops playback.
                    self.state
                        .update(cx, |s, cx| s.dispatch(&Action::Seek(frame), window, cx));
                    self.text_box_drag = Some(text_box::TextBoxDrag::new(
                        p,
                        origin,
                        zoom,
                        bounds,
                        self.state.read(cx),
                    ));
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
            if let Some(command) = command {
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::Edit(command), window, cx));
                self.pen.did_commit(self.state.read(cx));
            }
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
                        l.property(Property::AnchorX)
                            .expect("Anchor remains scalar")
                            .value_at(frame),
                        l.property(Property::AnchorY)
                            .expect("Anchor remains scalar")
                            .value_at(frame),
                    ]]
                } else if state.tool == Tool::Select {
                    layer_handles(l, frame).to_vec()
                } else {
                    Vec::new()
                };
                points.iter().enumerate().find_map(|(index, handle)| {
                    let h = world.point(*handle);
                    ((h[0] - p[0]).hypot(h[1] - p[1]) * zoom as f64 <= 7.0)
                        .then_some((l.id(), index))
                })
            });
        let projected_hit =
            match projected_layer_hit(comp, frame, p, &state.selected_layers, |layer| {
                transform_gesture::layer_bounds(layer, frame)
            }) {
                Ok(hit) => hit,
                Err(error) => {
                    self.state.update(cx, |state, cx| {
                        state.status = error;
                        cx.notify();
                    });
                    return;
                }
            };
        let hit = handle_hit
            .and_then(|(id, _)| comp.layer(id))
            .or_else(|| projected_hit.and_then(|id| comp.layer(id)));
        let layer = hit.map(|layer| layer.id());
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
                        l.property(Property::PositionX)?.value_at(frame),
                        l.property(Property::PositionY)?.value_at(frame),
                    ],
                    comp.position_space(l.id(), frame)?.inverse()?,
                ))
            })
            .collect();
        let snap_points = targets
            .iter()
            .filter_map(|(id, _, _)| layer_snap_points(comp, *id, frame))
            .flatten()
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
    fn moving(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.canvas_geometry_read_only(self.state.read(cx)) {
            self.cancel_expression_gestures();
            self.sample_pointer(event.position, cx);
            if self.gesture.is_none() {
                return;
            }
        }
        if self.state.read(cx).tool == Tool::Pen
            && self.state.read(cx).text_session.is_none()
            && !self.state.read(cx).colors.picking()
            && (!super::pen::pointer_input_allowed(
                self.state.read(cx),
                event.pressed_button == Some(MouseButton::Left),
                window.is_window_active(),
                TextField::is_composing(window, cx),
                TextField::active_pending_binding(cx).is_some(),
            ) || !self.focus.is_focused(window))
        {
            self.pen.abandon_pointer();
            cx.notify();
            return;
        }
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
        if self.text_box_drag.is_some() && event.pressed_button != Some(MouseButton::Left) {
            self.text_box_drag = None;
            cx.notify();
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
        if self.canvas_geometry_read_only(self.state.read(cx)) {
            self.cancel_expression_gestures();
            if self.gesture.is_none() {
                return;
            }
        }
        if self.state.read(cx).tool == Tool::Pen
            && self.state.read(cx).text_session.is_none()
            && !self.state.read(cx).colors.picking()
            && (!super::pen::pointer_input_allowed(
                self.state.read(cx),
                event.button == MouseButton::Left,
                window.is_window_active(),
                TextField::is_composing(window, cx),
                TextField::active_pending_binding(cx).is_some(),
            ) || !self.focus.is_focused(window))
        {
            self.pen.abandon_pointer();
            cx.notify();
            return;
        }
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
            if window.is_window_active()
                && self.focus.is_focused(window)
                && drag.valid(self.state.read(cx), self.bounds.get())
            {
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
                self.pen.did_commit(self.state.read(cx));
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
                    this.text_box_drag = None;
                    this.pen.cancel();
                    cx.notify();
                }),
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.gradient_drag = None;
                        this.text_box_drag = None;
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
            .is_some_and(|d| !d.valid(self.state.read(cx), self.bounds.get()))
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
        } else if self.canvas_geometry_read_only(self.state.read(cx)) {
            self.cancel_expression_gestures();
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
        let document_revision = state.document_revision;
        let core_generation = state.editor.context_generation();
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
        let pen_transform = pen_view(self.bounds.get(), state)
            .and_then(|view| self.pen.transform_overlay(state, view.zoom()));
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
        let spatial_scene = spatial_scene_active(state);
        let expression_scene = self.canvas_geometry_read_only(state);
        let pen_active = state.tool == Tool::Pen && !expression_scene;
        let pen_order_help = self.pen.order_help(state);
        let vertex_available = self.pen.numeric_vertex_available(state);
        let transform_available = self.pen.transform_available(state);
        let transform_enabled = self.pen.transform_enabled();
        let (vertex_caption, vertex_help) = self.pen.numeric_vertex_control_text(state);
        let comp = render_project.composition().clone();
        // Pen affine, numeric and gradient drafts share the globally unique transient-render
        // generation. Cancel/OK clears the displayed draft before source renders.
        let gradient_gesture = vertex_session(state)
            .map(|session| session.id)
            .or_else(|| self.pen.render_generation(state))
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
        let request = Request {
            project: render_project.clone(),
            frame,
            dimension: max_dimension,
            revision,
            document_revision,
            core_generation,
            transport,
            gradient_gesture,
        };
        self.update_render(request.clone(), playing, channel, window, cx);
        if channel != self.display_channel {
            if let Some((_, _, _, image)) = &mut self.cached {
                if let Some(raw) = &self.raw {
                    let replacement =
                        std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
                            channel.display(raw),
                        )]));
                    let old = std::mem::replace(image, replacement);
                    self.retired_images.retire(old, window);
                    self.display_channel = channel;
                }
            }
        }
        let overlay_options = viewer.clone();
        let mut error = self
            .failed
            .as_ref()
            .filter(|(failed, _)| request.accepts(failed, false))
            .map(|(_, e)| e.clone());
        let scene = self
            .displayed
            .as_ref()
            .and_then(|(shown, evaluated)| request.current_geometry(shown, evaluated.as_deref()));
        let geometry_ready = scene.is_some();
        let comp = scene
            .map(|project| project.composition().clone())
            .unwrap_or(comp);
        let control_order = if geometry_ready {
            match projected_control_order(&comp, frame, &selected) {
                Ok(order) => order,
                Err(problem) => {
                    error.get_or_insert(problem);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        let pen_overlay = if expression_scene || !geometry_ready {
            Vec::new()
        } else {
            pen_overlay
        };
        let pen_marquee = if expression_scene || !geometry_ready {
            None
        } else {
            pen_marquee
        };
        let pen_transform = if expression_scene || !geometry_ready {
            None
        } else {
            pen_transform
        };
        let gradient_overlay = if expression_scene || !geometry_ready {
            None
        } else {
            gradient_overlay
        };
        let text_session = if expression_scene || !geometry_ready {
            None
        } else {
            text_session
        };
        let text_box_rect = if expression_scene || !geometry_ready {
            None
        } else {
            text_box_rect
        };
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
                        if spatial_scene { "  ·  Spatial: selection only · Geometry edits through scripting" } else if expression_scene { "  ·  Expressions: selection only · Inspector edits authored values" } else if text_session.is_some() { if text_session.as_ref().is_some_and(|s| s.style.paragraph) { "  ·  Paragraph text: Ctrl+Enter finish · Esc cancel" } else { "  ·  Auto-size text: Ctrl+Enter finish · Esc cancel" } } else if self.state.read(cx).colors.picking() { "  ·  Pick composition color · click to sample · Esc to return" } else if gradient_active { if gradient_point == 0 { "  ·  Gradient Start: drag · Tab switch · arrows move · Alt both · Esc close" } else { "  ·  Gradient End: drag · Tab switch · arrows move · Alt both · Esc close" } } else if pen_active { "  ·  Pen: Shift-click / Shift-drag select · Ctrl+A Contents points · drag selected · Shift+T transform · Esc cancel" } else if self.state.read(cx).tool == Tool::Text {
                            "  ·  Click: auto-size text · Drag: paragraph box"
                        } else if self.pending.is_some() {
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
                        .child(ui::text_button("pen-canvas-transform", if transform_enabled { "Canvas Transform ON · Shift+T" } else { "Canvas Transform · Shift+T" })
                            .h(px(20.0)).flex_none()
                            .when(!transform_available, |button| button.opacity(0.35))
                            .tooltip(|_, cx| cx.new(|_| ui::Tip("Enabled Contents points only, current frame. Corner handles scale in composition axes; top handle rotates around the fixed center. Shift: uniform scale, 15° rotation, axis-constrained move. Numeric editing still supports one path.".into())).into())
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                window.prevent_default();
                                if event.click_count == 1 && event.modifiers == gpui::Modifiers::default()
                                    && this.focus.is_focused(window)
                                    && super::pen::pointer_input_allowed(this.state.read(cx), true, window.is_window_active(), TextField::is_composing(window, cx), TextField::active_pending_binding(cx).is_some())
                                { this.pen.toggle_transform(this.state.read(cx)); }
                                cx.stop_propagation(); cx.notify();
                            })))
                        .child(ui::text_button("pen-edit-vertex", vertex_caption)
                            .h(px(20.0))
                            .flex_none()
                            .when(!vertex_available, |button| button.opacity(0.35))
                            .tooltip(move |_, cx| cx.new(|_| ui::Tip(vertex_help.into())).into())
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                // Prevent the button's default focus before capturing
                                // the live Pen selection; canvas blur still cancels it.
                                window.prevent_default();
                                if !this.focus.is_focused(window)
                                    || !super::pen::pointer_input_allowed(this.state.read(cx), true, window.is_window_active(), TextField::is_composing(window, cx), TextField::active_pending_binding(cx).is_some())
                                { cx.stop_propagation(); return; }
                                let request = this.pen.numeric_vertex_request(this.state.read(cx));
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
                        if this.canvas_geometry_read_only(this.state.read(cx)) {
                            this.cancel_expression_gestures();
                            return;
                        }
                        if this.gradient_key(event, window, cx) { cx.stop_propagation(); return; }
                        if event.keystroke.key=="escape" && this.text_box_drag.take().is_some() {cx.stop_propagation();cx.notify();return;}
                        if this.text_key(event,window,cx) {return;}
                        if this.state.read(cx).colors.session.is_some() || this.state.read(cx).gradient_editor.is_some() { return; }
                        if this.state.read(cx).tool == Tool::Pen {
                            if !this.focus.is_focused(window) || !window.is_window_active()
                                || TextField::is_composing(window, cx) || TextField::active_pending_binding(cx).is_some()
                            { return; }
                            if this.pen.transform_key(event, true, false, this.state.read(cx)) {
                                cx.stop_propagation(); cx.notify(); return;
                            }
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
                            } else if event.keystroke.modifiers == gpui::Modifiers::default() {
                                if event.is_held && matches!(event.keystroke.key.as_str(), "delete" | "backspace" | "enter" | "escape") {
                                    (true, None)
                                } else { this.pen.key(&event.keystroke.key, this.state.read(cx)) }
                            } else { (false, None) };
                            if handled {
                                if let Some(command) = command {
                                    this.state.update(cx, |s,cx| s.dispatch(&Action::Edit(command), window, cx));
                                    this.pen.did_commit(this.state.read(cx));
                                }
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
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, window, cx| {
                        this.text_box_drag = None;
                        if this.state.read(cx).tool == Tool::Pen { this.pen.abandon_pointer(); window.prevent_default(); cx.stop_propagation(); }
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(|this, _, window, cx| {
                        this.text_box_drag = None;
                        if this.state.read(cx).tool == Tool::Pen { this.pen.abandon_pointer(); window.prevent_default(); cx.stop_propagation(); }
                        cx.notify();
                    }))
                    .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.pen.abandon_pointer();
                        this.text_box_drag = None;
                        if !preview_modal_active(this.state.read(cx)) && !crate::color_edit::preserving_text_selection(window, cx) {
                            // Preview's capture precedes the Character field's
                            // outside capture. Commit that field into its owned
                            // draft before closing the text session.
                            if this.state.read(cx).text_session.is_some() {
                                crate::components::TextField::commit_text_selection_active(window, cx);
                            }
                            this.state.update(cx,|s,cx|s.finish_text(true,cx));
                        }
                        cx.notify();
                    }))
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
                                                control_order.iter().filter_map(|id| comp.layer(*id)).filter(|layer| {
                                                    geometry_ready && !pen_active && text_session.is_none() && controls_active(
                                                        &comp,
                                                        layer,
                                                        frame,
                                                        selected.contains(&layer.id()),
                                                    )
                                                })
                                            {
                                                let Some(corners) = layer_corners(&comp, layer.id(), frame) else {
                                                    continue;
                                                };
                                                let corners = corners.map(|[x, y]| {
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
                                                    if !selected.contains(&layer.id()) || expression_scene {
                                                        continue;
                                                    }
                                                    let Ok(projected) = comp.projected_geometry(layer.id(), frame) else {
                                                        continue;
                                                    };
                                                    let world = projected.transform;
                                                    for handle in
                                                        layer_handles(layer, frame)
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
                                                    let anchor = world.point([
                                                        layer.property(Property::AnchorX)
                                                            .expect("Anchor remains scalar").value_at(frame),
                                                        layer.property(Property::AnchorY)
                                                            .expect("Anchor remains scalar").value_at(frame),
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
                                    if let Some(overlay) = pen_transform { super::pen::paint_transform(overlay, origin, zoom, window); }
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

#[cfg(test)]
mod source_text_tests {
    use super::*;
    use libre_effects_core::{Content, Editor, PropertyPath, TrackEdit};

    #[test]
    fn empty_text_hit_fallback_follows_the_sample_in_both_baseline_directions() {
        for baseline in ["", "A"] {
            let mut e = Editor::default();
            e.execute(Command::AddContent {
                content: Content::Text {
                    text: baseline.into(),
                    font_size: 48.0,
                },
                width: 600.0,
                height: 400.0,
                name: "Text".into(),
            })
            .unwrap();
            e.execute(Command::EditTrack {
                id: 1,
                property: PropertyPath::SourceText,
                edit: TrackEdit::ToggleAnimation { frame: 10 },
            })
            .unwrap();
            e.execute(Command::EditSourceText {
                id: 1,
                frame: 30,
                text: if baseline.is_empty() {
                    "A".into()
                } else {
                    "".into()
                },
            })
            .unwrap();
            let before = e.project().clone();
            let layer = e.selected_layer().unwrap();
            for frame in [0, 10, 29, 30, 60] {
                let empty = layer.source_text_at(frame).unwrap().is_empty();
                assert_eq!(text_layer_hit(layer, frame, [590.0, 390.0]), empty);
                assert!(!text_layer_hit(layer, frame, [-1.0, -1.0]));
                assert!(!text_layer_hit(layer, frame, [601.0, 401.0]));
            }
            assert_eq!(e.project(), &before);
        }
        let mut e = Editor::default();
        e.execute(Command::AddSolid).unwrap();
        assert!(!text_layer_hit(e.selected_layer().unwrap(), 0, [1.0, 1.0]));
    }
}
