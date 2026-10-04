mod channels;
mod planning;
mod selection;
mod snapping;
mod speed;
mod tangent;
mod transform;
mod viewport;
use crate::view_state::GraphChannel;
use crate::{
    components::TextField,
    editor::{Action, EditorState, Tool},
    ui,
};
use gpui::{
    Bounds, ContentMask, Context, Entity, FocusHandle, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathBuilder, Pixels, Point, SharedString, Window, canvas, div, fill, point,
    prelude::*, px, relative, rgb, size,
};
use libre_effects_core::{
    AnimatedProperty, Bezier, Command, Interpolation, KeyRef, LayerId, PropertyPath, TemporalMode,
    TextParam, TrackEdit,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

#[cfg(test)]
fn graph_units(property: PropertyPath, speed: bool) -> &'static str {
    match (property, speed) {
        (PropertyPath::Text(parameter), speed) => channels::text_unit(parameter).label(speed),
        (_, true) => "units/s",
        _ => "",
    }
}

#[derive(Clone, Copy)]
struct View {
    start: f64,
    span: f64,
    low: f64,
    high: f64,
}
impl View {
    fn speed_curves(self, track: &AnimatedProperty, fps: f64) -> Vec<Vec<(f64, f64)>> {
        // Sampling and painting must share the selected (possibly frozen) view.
        speed::curves(track, self.start as u32, self.span.ceil() as u32, fps)
    }
    fn point(self, bounds: Bounds<Pixels>, frame: f64, value: f64) -> Point<Pixels> {
        point(
            bounds.left() + bounds.size.width * ((frame - self.start) / self.span) as f32,
            bounds.bottom()
                - bounds.size.height * ((value - self.low) / (self.high - self.low)) as f32,
        )
    }
    fn value(self, bounds: Bounds<Pixels>, p: Point<Pixels>) -> (f64, f64) {
        (
            self.start
                + f32::from(p.x - bounds.left()) as f64
                    / f32::from(bounds.size.width).max(1.0) as f64
                    * self.span,
            self.low
                + f32::from(bounds.bottom() - p.y) as f64
                    / f32::from(bounds.size.height).max(1.0) as f64
                    * (self.high - self.low),
        )
    }
}
fn view(track: &AnimatedProperty, start: u32, span: u32, speed_mode: bool, fps: f64) -> View {
    if speed_mode {
        let (mut low, mut high) = (0.0_f64, 0.0_f64);
        for (_, v) in speed::curves(track, start, span, fps).into_iter().flatten() {
            low = low.min(v);
            high = high.max(v);
        }
        let padding = ((high - low) * 0.18).max(1.0);
        return View {
            start: start as f64,
            span: span.max(1) as f64,
            low: low - padding,
            high: high + padding,
        };
    }
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for i in 0..=512 {
        let v = track.sample(start as f64 + span as f64 * i as f64 / 512.0);
        low = low.min(v);
        high = high.max(v);
    }
    for (_, key) in track.keys().range(start..=start.saturating_add(span)) {
        low = low.min(key.value);
        high = high.max(key.value);
    }
    let padding = ((high - low) * 0.18).max(1.0);
    View {
        start: start as f64,
        span: span.max(1) as f64,
        low: low - padding,
        high: high + padding,
    }
}
const EASE_VIEW: View = View {
    start: 0.0,
    span: 1.0,
    low: -0.3,
    high: 1.3,
};
#[derive(Clone, Copy)]
struct HandleSpace {
    view: View,
    bounds: Bounds<Pixels>,
    from: f64,
    span: f64,
    low: f64,
    delta: f64,
    inline: bool,
}
impl HandleSpace {
    fn point(self, x: f64, y: f64) -> Point<Pixels> {
        self.view.point(
            self.bounds,
            self.from + self.span * x,
            self.low + self.delta * y,
        )
    }
    fn value(self, p: Point<Pixels>) -> (f64, f64) {
        let (f, v) = self.view.value(self.bounds, p);
        ((f - self.from) / self.span, (v - self.low) / self.delta)
    }
    fn segment(
        view: View,
        bounds: Bounds<Pixels>,
        track: &AnimatedProperty,
        frame: u32,
    ) -> Option<Self> {
        let a = track.keys().get(&frame)?.value;
        let (&end, b) = track.keys().range(frame + 1..).next()?;
        ((b.value - a).abs() > 1e-9).then_some(Self {
            view,
            bounds,
            from: frame as f64,
            span: (end - frame) as f64,
            low: a,
            delta: b.value - a,
            inline: true,
        })
    }
}
fn handle_draft(
    space: HandleSpace,
    original: Bezier,
    index: usize,
    start: Point<Pixels>,
    end: Point<Pixels>,
) -> Bezier {
    let delta = end - start;
    if f32::from(delta.x).abs() + f32::from(delta.y).abs() < 3.0 {
        return original;
    }
    let (x, y) = if index == 0 {
        (original.x1, original.y1)
    } else {
        (original.x2, original.y2)
    };
    let (x, y) = space.value(space.point(x, y) + delta);
    let mut curve = original;
    if index == 0 {
        curve.x1 = x.clamp(0.0, 1.0);
        curve.y1 = y.clamp(-2.0, 3.0);
    } else {
        curve.x2 = x.clamp(0.0, 1.0);
        curve.y2 = y.clamp(-2.0, 3.0);
    }
    curve
}
#[derive(Clone, Copy)]
struct LaneGeometry {
    channel: GraphChannel,
    bounds: Bounds<Pixels>,
    view: View,
}
#[derive(Clone)]
enum Drag {
    Time {
        id: LayerId,
        property: PropertyPath,
        gesture: planning::TimeGesture,
        bounds: Bounds<Pixels>,
        remove_on_click: Option<KeyRef>,
    },
    Transform {
        id: LayerId,
        property: PropertyPath,
        transform: transform::Transform,
    },
    Zoom {
        id: LayerId,
        property: PropertyPath,
        zoom: viewport::Zoom,
        end: Point<Pixels>,
    },
    Pan {
        id: LayerId,
        property: PropertyPath,
        pan: viewport::Pan,
    },
    Tangent {
        id: LayerId,
        property: PropertyPath,
        tangent: tangent::Tangent,
        handle: libre_effects_core::TemporalHandle,
        split: bool,
        start: Point<Pixels>,
        moved: bool,
        view: View,
        bounds: Bounds<Pixels>,
        fps: f64,
        speed: bool,
    },
    Marquee {
        id: LayerId,
        property: PropertyPath,
        start: Point<Pixels>,
        end: Point<Pixels>,
        additive: bool,
        view: View,
        bounds: Bounds<Pixels>,
        lanes: Vec<LaneGeometry>,
    },
    Key {
        id: LayerId,
        property: PropertyPath,
        from: u32,
        to: u32,
        value: f64,
        velocity: Option<(bool, libre_effects_core::TemporalHandle, f64)>,
        keys: Vec<selection::Sample>,
        snapping: snapping::Targets,
        guides: snapping::Guides,
        origin: f64,
        remove_on_click: bool,
        view: View,
        bounds: Bounds<Pixels>,
        start: Point<Pixels>,
        moved: bool,
    },
    Handle {
        id: LayerId,
        property: PropertyPath,
        frame: u32,
        index: usize,
        curve: Bezier,
        original: Bezier,
        start: Point<Pixels>,
        moved: bool,
        space: HandleSpace,
    },
}
/// Numeric drafts bind to the full active channel, independently of Inspector focus.
#[derive(Clone)]
struct GraphInputSource {
    identity: u64,
    source: std::sync::Arc<libre_effects_core::Project>,
    revision: u64,
    frame: u32,
    channel: GraphChannel,
    tool: Tool,
    transport: u64,
    graph_open: bool,
}
impl GraphInputSource {
    fn eligible(state: &EditorState) -> Option<GraphChannel> {
        let channel = state.graph_active_channel()?;
        (graph_editable(state)
            && state
                .editor
                .project()
                .composition()
                .layer(channel.id)
                .is_some_and(|layer| !layer.locked()))
        .then_some(channel)
    }
    fn new(state: &EditorState) -> Option<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let channel = Self::eligible(state)?;
        Some(Self {
            identity: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            source: std::sync::Arc::new(state.editor.project().clone()),
            revision: state.document_revision,
            frame: state.frame,
            channel,
            tool: state.tool,
            transport: state.transport_generation(),
            graph_open: state.graph_open,
        })
    }
    fn current(&self, state: &EditorState) -> bool {
        Self::eligible(state) == Some(self.channel)
            && self.revision == state.document_revision
            && self.frame == state.frame
            && self.tool == state.tool
            && self.transport == state.transport_generation()
            && self.graph_open == state.graph_open
            && self.source.as_ref() == state.editor.project()
    }
    fn refresh(previous: &mut Option<Self>, state: &EditorState) {
        let Some(channel) = Self::eligible(state) else {
            *previous = None;
            return;
        };
        if let Some(previous) = previous.as_mut()
            && previous.source.as_ref() == state.editor.project()
        {
            previous.channel = channel;
            previous.revision = state.document_revision;
            previous.frame = state.frame;
            previous.tool = state.tool;
            previous.transport = state.transport_generation();
            previous.graph_open = state.graph_open;
            return;
        }
        *previous = Self::new(state);
    }
    fn binding(&self) -> String {
        format!(
            "{}-{}-{}-{:?}-{:?}-{}-{}",
            self.identity,
            self.revision,
            self.frame,
            self.channel,
            match self.tool {
                Tool::Shape(shape) => format!("shape:{shape:?}"),
                tool => format!("{:?}", std::mem::discriminant(&tool)),
            },
            self.transport,
            self.graph_open
        )
    }
}
#[derive(Clone)]
struct KeyInputTarget {
    document: GraphInputSource,
    selected: (LayerId, u32, PropertyPath),
    keys: Vec<libre_effects_core::KeyRef>,
    displayed_number: Option<f64>,
}
impl KeyInputTarget {
    #[cfg(test)]
    fn new(state: &EditorState) -> Option<Self> {
        let selected = selected(state)?;
        Some(Self {
            document: GraphInputSource::new(state)?,
            selected,
            keys: selection::included(state),
            displayed_number: None,
        })
    }
    fn same_number(&self, value: f64) -> bool {
        self.displayed_number == Some(value)
    }
    fn current(&self, state: &EditorState) -> bool {
        self.document.current(state)
            && selected(state) == Some(self.selected)
            && selection::included(state) == self.keys
    }
    fn binding(&self) -> String {
        format!(
            "{}-{:?}-{:?}",
            self.document.binding(),
            self.selected,
            self.keys
        )
    }
}

pub(crate) struct Graph {
    state: Entity<EditorState>,
    plot: Rc<Cell<Option<Bounds<Pixels>>>>,
    lanes: Rc<RefCell<BTreeMap<GraphChannel, LaneGeometry>>>,
    lane_clip: Rc<Cell<Option<Bounds<Pixels>>>>,
    easing: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
    drag: Option<Drag>,
    drag_revision: u64,
    drag_tool: Tool,
    frozen: Option<planning::FrozenContext>,
    fields: Vec<Entity<TextField>>,
    input_targets: Vec<Rc<RefCell<Option<KeyInputTarget>>>>,
    input_source: Option<GraphInputSource>,
    details: bool,
    transform_box: bool,
    hand: viewport::TemporaryHand,
    focus_watch: Option<[gpui::Subscription; 2]>,
}
fn selected(state: &EditorState) -> Option<(LayerId, u32, PropertyPath)> {
    let channel = state.graph_active_channel()?;
    let keys = selection::active(state);
    let key = state
        .graph_key
        .filter(|key| keys.contains(key))
        .or_else(|| keys.first().copied())?;
    (key.id == channel.id && key.property == channel.property).then_some((
        key.id,
        key.frame,
        key.property,
    ))
}
fn channel_color(channel: GraphChannel) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    format!("{}:{:?}", channel.id, channel.property).hash(&mut hash);
    [0xffc66d, 0x7bd8a5, 0xd3a3ff, 0x72c8ff, 0xff92b0, 0x9de3de][hash.finish() as usize % 6]
}
fn marquee_keys(
    state: &EditorState,
    lanes: &[LaneGeometry],
    start: Point<Pixels>,
    end: Point<Pixels>,
    clip: Option<Bounds<Pixels>>,
) -> BTreeSet<KeyRef> {
    let mut keys = BTreeSet::new();
    let fps = state.editor.project().composition().fps().as_f64();
    for lane in lanes {
        let Some(track) = state
            .editor
            .project()
            .composition()
            .layer(lane.channel.id)
            .and_then(|layer| layer.track(lane.channel.property))
        else {
            continue;
        };
        for (&frame, key) in track.keys() {
            let points = if state.graph_view.speed {
                speed::ends(track, frame, fps)
                    .into_iter()
                    .map(|(side, value)| {
                        let mut p = lane.view.point(lane.bounds, frame as f64, value);
                        p.x += px(if side { -5.0 } else { 5.0 });
                        p
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![lane.view.point(lane.bounds, frame as f64, key.value)]
            };
            if points.into_iter().any(|p| {
                lane.bounds.contains(&p)
                    && clip.is_none_or(|bounds| bounds.contains(&p))
                    && selection::inside(start, end, p)
            }) {
                keys.insert(KeyRef {
                    id: lane.channel.id,
                    property: lane.channel.property,
                    frame,
                });
            }
        }
    }
    keys
}
fn graph_editable(state: &EditorState) -> bool {
    !state.playing
        && !state.fonts_open
        && !state.media_open
        && !state.queue_open
        && state.colors.session.is_none()
        && state.gradient_editor.is_none()
        && state.vertex_editor.is_none()
        && state.text_session.is_none()
        && !state.new_composition_requested
}
fn numeric_handle_unchanged(
    track: &AnimatedProperty,
    frame: u32,
    incoming: bool,
    handle: libre_effects_core::TemporalHandle,
) -> bool {
    let segment = if incoming {
        track.keys().range(..frame).next_back().map(|(_, key)| key)
    } else {
        track
            .keys()
            .range(frame + 1..)
            .next()
            .and_then(|_| track.keys().get(&frame))
    };
    if segment.is_none_or(|key| key.interpolation == Interpolation::Hold) {
        return false;
    }
    let original = track
        .key_velocity_handles(frame)
        .ok()
        .and_then(|handles| handles[if incoming { 0 } else { 1 }])
        .or_else(|| track.temporal_handle(frame, incoming));
    original.is_some_and(|original| {
        let same =
            |a: f64, b: f64| (a - b).abs() <= 16.0 * f64::EPSILON * a.abs().max(b.abs()).max(1.0);
        same(original.slope, handle.slope) && same(original.influence, handle.influence)
    })
}
fn graph_owned_key(key: &str, m: gpui::Modifiers) -> bool {
    matches!(
        key,
        "delete" | "backspace" | "f9" | "left" | "right" | "up" | "down"
    ) || (m.control && matches!(key, "a" | "d" | "x" | "c" | "v" | "z"))
        || (m.alt && matches!(key, "[" | "]"))
}
fn graph_shortcut(key: &str, modifiers: gpui::Modifiers) -> bool {
    match key {
        "delete" | "backspace" => {
            !modifiers.control && !modifiers.alt && !modifiers.platform && !modifiers.shift
        }
        "a" => modifiers.control && !modifiers.alt && !modifiers.platform && !modifiers.shift,
        _ => false,
    }
}
fn chosen_contains_key(state: &EditorState, key: KeyRef) -> bool {
    state.selected_keys.contains(&key)
}
fn mixed_selection(keys: &[KeyRef]) -> bool {
    keys.first().is_some_and(|first| {
        keys.iter()
            .any(|key| key.id != first.id || key.property != first.property)
    })
}
fn apply_plan(
    state: &mut EditorState,
    result: Result<planning::EditPlan, String>,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    if !graph_editable(state) {
        return;
    }
    match result {
        Ok(plan) => {
            let prior_active = state.graph_key;
            if let Some(command) = plan.command {
                state.dispatch(&Action::Edit(command), window, cx);
                if !state.status.starts_with("Edited") {
                    return;
                }
            }
            state.selected_keys = plan.keys.into_iter().collect();
            state.graph_key = plan
                .active
                .or_else(|| prior_active.filter(|key| state.selected_keys.contains(key)));
            if let Some(key) = plan.active {
                state.frame = key.frame;
            }
        }
        Err(error) => state.status = error,
    }
    cx.notify();
}

fn curve_at(state: &EditorState) -> Option<Bezier> {
    if selection::active(state).len() > 1 {
        return None;
    }
    let (id, frame, property) = selected(state)?;
    let track = state
        .editor
        .project()
        .composition()
        .layer(id)?
        .track(property)?;
    let (_, next) = track.keys().range(frame + 1..).next()?;
    if track.keys()[&frame].temporal.outgoing.is_some()
        || next.temporal.incoming.is_some()
        || !track.keys()[&frame].temporal.mode.is_independent()
        || !next.temporal.mode.is_independent()
    {
        return None;
    }
    Some(match track.keys()[&frame].interpolation {
        Interpolation::Bezier(b) => b,
        Interpolation::Linear => Bezier {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
        },
        Interpolation::Smooth => Bezier::default(),
        Interpolation::Hold => return None,
    })
}
fn dispatch_key(
    state: &mut EditorState,
    command: Command,
    id: LayerId,
    to: u32,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    let selected = selected(state);
    let property = selected.map_or(state.graph_property, |(_, _, p)| p);
    let from = selected.map(|(_, frame, _)| frame);
    let mut keys = state.selected_keys.clone();
    state.dispatch(&Action::Edit(command), window, cx);
    if state.status.starts_with("Edited") {
        state.selected_layers = [id].into();
        if let Some(frame) = from {
            keys.remove(&libre_effects_core::KeyRef {
                id,
                property,
                frame,
            });
        }
        keys.insert(libre_effects_core::KeyRef {
            id,
            property,
            frame: to,
        });
        state.selected_keys = keys;
        state.graph_key = Some(KeyRef {
            id,
            property,
            frame: to,
        });
        state.frame = to;
        cx.notify();
    }
}
impl Graph {
    pub(super) fn focus(&self, window: &mut Window) {
        window.focus(&self.focus);
    }
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            if this.drag.as_ref().is_some_and(|drag| {
                let (id, p) = match drag {
                    Drag::Time { id, property, .. }
                    | Drag::Zoom { id, property, .. }
                    | Drag::Transform { id, property, .. }
                    | Drag::Pan { id, property, .. }
                    | Drag::Key { id, property, .. }
                    | Drag::Handle { id, property, .. }
                    | Drag::Marquee { id, property, .. }
                    | Drag::Tangent { id, property, .. } => (*id, *property),
                };
                let s = this.state.read(cx);
                s.graph_active_channel() != Some(GraphChannel { id, property: p })
                    || s.document_revision != this.drag_revision || s.tool != this.drag_tool
                    || !graph_editable(s) || !s.graph_open
                    || matches!(drag, Drag::Transform { transform, .. } if !transform.is_current(s))
                    || matches!(drag, Drag::Time { gesture, bounds, .. } if !gesture.is_current(s, Some(*bounds)))
                    || (matches!(drag, Drag::Key { .. } | Drag::Tangent { .. } | Drag::Handle { .. } | Drag::Marquee { .. }) && this.frozen.as_ref().is_none_or(|context| !context.current(s,this.plot.get())))
            }) {
                this.drag = None;
                this.hand.cancel();
            }
            cx.notify();
        })
        .detach();
        let focus = cx.focus_handle();
        let input_targets: Vec<Rc<RefCell<Option<KeyInputTarget>>>> =
            (0..13).map(|_| Default::default()).collect();
        let fields = (0..13)
            .map(|index| {
                let edit = state.clone();
                let return_focus = focus.clone();
                let target = input_targets[index].clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |state, cx| {
                            if !target.borrow().as_ref().is_some_and(|t| t.current(state)) {
                                return;
                            }
                            let Some((id, frame, property)) = selected(state) else {
                                return;
                            };
                            let Ok(value) = text.trim().parse::<f64>() else {
                                state.status = "Enter a finite number.".into();
                                cx.notify();
                                return;
                            };
                            if !value.is_finite() {
                                state.status = "Enter a finite number.".into();
                                cx.notify();
                                return;
                            }
                            // Reformatting the displayed number must not round its
                            // full-precision source or migrate a legacy document.
                            if target
                                .borrow()
                                .as_ref()
                                .is_some_and(|target| target.same_number(value))
                            {
                                return;
                            }
                            if index == 10 || index == 12 {
                                let keys = selection::included(state);
                                let result = if index == 12 {
                                    if value.fract() != 0.0 || value.abs() > i64::MAX as f64 {
                                        state.status =
                                            "Time offset must be a whole number of frames.".into();
                                        cx.notify();
                                        return;
                                    }
                                    planning::EditPlan::translate(
                                        state.editor.project(),
                                        &keys,
                                        value as i64,
                                        state.graph_key,
                                    )
                                } else {
                                    let origin =
                                        keys.iter().map(|key| key.frame).min().unwrap_or(0) as f64;
                                    planning::EditPlan::scale_time(
                                        state.editor.project(),
                                        &keys,
                                        origin,
                                        value / 100.0,
                                        state.graph_key,
                                    )
                                };
                                apply_plan(state, result, window, cx);
                            } else if index == 11 {
                                let result = planning::EditPlan::scale_value(
                                    state.editor.project(),
                                    &selection::included(state),
                                    value / 100.0,
                                    state.graph_key,
                                );
                                apply_plan(state, result, window, cx);
                            } else if index < 2 {
                                if index == 0
                                    && (value < 0.0
                                        || value.fract() != 0.0
                                        || value
                                            >= state.editor.project().composition().duration()
                                                as f64)
                                {
                                    state.status =
                                        "Key time must be a whole frame inside the composition."
                                            .into();
                                    cx.notify();
                                    return;
                                }
                                let track = state
                                    .editor
                                    .project()
                                    .composition()
                                    .layer(id)
                                    .unwrap()
                                    .track(property)
                                    .expect("selected graph track");
                                let to = if index == 0 { value as u32 } else { frame };
                                let value = if index == 1 {
                                    value
                                } else {
                                    track.keys()[&frame].value
                                };
                                if to == frame && value == track.keys()[&frame].value {
                                    return;
                                }
                                dispatch_key(
                                    state,
                                    Command::EditTrack {
                                        id,
                                        property,
                                        edit: TrackEdit::Keyframe {
                                            from: frame,
                                            to,
                                            value,
                                        },
                                    },
                                    id,
                                    to,
                                    window,
                                    cx,
                                );
                            } else if index >= 6 {
                                let incoming = index < 8;
                                let comp = state.editor.project().composition();
                                let track = comp.layer(id).unwrap().track(property).unwrap();
                                let mut handle = track.temporal_handle(frame, incoming).unwrap_or(
                                    libre_effects_core::TemporalHandle {
                                        slope: 0.0,
                                        influence: 1.0 / 3.0,
                                    },
                                );
                                if index % 2 == 0 {
                                    handle.slope = value / comp.fps().as_f64();
                                } else {
                                    handle.influence = value / 100.0;
                                }
                                if numeric_handle_unchanged(track, frame, incoming, handle) {
                                    return;
                                }
                                state.dispatch(
                                    &Action::Edit(Command::SetTemporalHandle {
                                        id,
                                        property,
                                        frame,
                                        incoming,
                                        handle,
                                    }),
                                    window,
                                    cx,
                                );
                            } else if let Some(mut curve) = curve_at(state) {
                                let original = curve;
                                match index {
                                    2 => curve.x1 = value,
                                    3 => curve.y1 = value,
                                    4 => curve.x2 = value,
                                    _ => curve.y2 = value,
                                };
                                if curve == original {
                                    return;
                                }
                                state.dispatch(
                                    &Action::Edit(Command::EditTrack {
                                        id,
                                        property,
                                        edit: TrackEdit::Interpolate {
                                            frame,
                                            interpolation: Interpolation::Bezier(curve),
                                        },
                                    }),
                                    window,
                                    cx,
                                );
                            }
                        });
                    })
                    .return_focus(return_focus)
                })
            })
            .collect();
        Self {
            state,
            plot: Rc::new(Cell::new(None)),
            lanes: Default::default(),
            lane_clip: Default::default(),
            easing: Rc::new(Cell::new(None)),
            focus,
            drag: None,
            drag_revision: 0,
            drag_tool: Tool::Select,
            frozen: None,
            fields,
            input_targets,
            input_source: None,
            details: false,
            transform_box: false,
            hand: Default::default(),
            focus_watch: None,
        }
    }
    fn sync_input(
        &mut self,
        index: usize,
        target: &Option<KeyInputTarget>,
        value: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // Each field retains its own displayed context; hidden controls cannot
        // inherit a fresh target while still carrying an old pending draft.
        *self.input_targets[index].borrow_mut() = target.clone().map(|mut target| {
            target.displayed_number = value.parse().ok();
            target
        });
        let binding = target
            .as_ref()
            .map(KeyInputTarget::binding)
            .unwrap_or_default();
        self.fields[index].update(cx, |field, _| field.sync(binding, value, window));
    }
    fn freeze_drag(&mut self, cx: &Context<Self>) {
        self.frozen = None;
        if matches!(
            self.drag,
            Some(
                Drag::Key { .. }
                    | Drag::Tangent { .. }
                    | Drag::Handle { .. }
                    | Drag::Marquee { .. }
            )
        ) {
            self.frozen = self
                .plot
                .get()
                .and_then(|bounds| planning::FrozenContext::new(self.state.read(cx), bounds).ok());
            if self.frozen.is_none() {
                self.drag = None;
            }
        }
    }
    fn cancel_navigation(&mut self, cx: &mut Context<Self>) {
        self.hand.cancel();
        self.frozen = None;
        match self.drag.take() {
            Some(Drag::Pan { pan, .. }) => self.state.update(cx, |s, cx| {
                pan.restore(s);
                cx.notify();
            }),
            Some(Drag::Zoom { zoom, .. }) => self.state.update(cx, |s, cx| {
                zoom.restore(s);
                cx.notify();
            }),
            _ => {}
        }
        cx.notify();
    }
    fn fit(&mut self, selected_only: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.drag = None;
        window.focus(&self.focus);
        self.state.update(cx, |s, cx| {
            viewport::fit(s, selected_only);
            cx.notify();
        });
    }
    fn scroll(
        &mut self,
        channel: GraphChannel,
        event: &gpui::ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_some() {
            cx.stop_propagation();
            return;
        }
        let state = self.state.read(cx);
        if state.graph_included_channels().len() > 1
            && !event.modifiers.alt
            && !event.modifiers.shift
            && !event.modifiers.control
        {
            return; // Ordinary wheel scrolls the lane list, including offscreen pins.
        }
        cx.stop_propagation();
        let Some(lane) = self.lanes.borrow().get(&channel).copied() else {
            return;
        };
        let bounds = lane.bounds;
        let view = lane.view;
        let delta = event.delta.pixel_delta(px(20.0));
        let dx = f32::from(delta.x) as f64;
        let dy = f32::from(delta.y) as f64;
        let width = f32::from(bounds.size.width).max(1.0) as f64;
        let height = f32::from(bounds.size.height).max(1.0) as f64;
        let x = f32::from(event.position.x - bounds.left()) as f64 / width;
        let y = f32::from(bounds.bottom() - event.position.y) as f64 / height;
        self.state.update(cx, |s, cx| {
            if event.modifiers.alt {
                viewport::horizontal(s, dy, x, true);
            } else if event.modifiers.shift || dx.abs() > dy.abs() {
                viewport::horizontal(
                    s,
                    if dx.abs() > dy.abs() { dx } else { dy } / width,
                    x,
                    false,
                );
            } else if event.modifiers.control
                || s.graph_channel_height(channel, s.graph_view.speed)
                    .is_some()
            {
                let height = viewport::vertical(
                    view,
                    if event.modifiers.control {
                        dy
                    } else {
                        -dy / height
                    },
                    y,
                    event.modifiers.control,
                );
                s.graph_set_channel_height(channel, s.graph_view.speed, Some(height));
            }
            cx.notify();
        });
    }
    fn preset(&self, interpolation: Interpolation, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let result = planning::EditPlan::interpolation(
                state.editor.project(),
                &selection::included(state),
                interpolation,
            );
            apply_plan(state, result, window, cx);
        });
    }
    fn ease(&self, incoming: bool, outgoing: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let result = planning::EditPlan::ease(
                state.editor.project(),
                &selection::included(state),
                incoming,
                outgoing,
            );
            apply_plan(state, result, window, cx);
        });
    }
    fn down(
        &mut self,
        channel: GraphChannel,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_some() {
            return;
        }
        self.frozen = None;
        window.focus(&self.focus);
        cx.stop_propagation();
        let Some(lane) = self.lanes.borrow().get(&channel).copied() else {
            return;
        };
        let bounds = lane.bounds;
        if !graph_editable(self.state.read(cx)) {
            return;
        }
        self.state.update(cx, |s, cx| {
            s.graph_activate_property(channel, true);
            cx.notify();
        });
        self.plot.set(Some(bounds));
        let state = self.state.read(cx);
        self.drag_revision = state.document_revision;
        self.drag_tool = state.tool;
        let Some(layer) = state.editor.project().composition().layer(channel.id) else {
            return;
        };
        let id = channel.id;
        let property = channel.property;
        let Some(track) = layer.track(property) else {
            return;
        };
        let fps = state.editor.project().composition().fps().as_f64();
        let snap_playhead = state.frame;
        let view = viewport::current(state, track);
        if event.button == MouseButton::Middle || state.tool == Tool::Hand || self.hand.held {
            self.hand.consume();
            self.drag = Some(Drag::Pan {
                id,
                property,
                pan: viewport::Pan::new(state, view, bounds, event),
            });
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if state.tool == Tool::Zoom {
            self.drag = Some(Drag::Zoom {
                id,
                property,
                zoom: viewport::Zoom::new(state, view, bounds, event),
                end: event.position,
            });
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let speed_mode = state.graph_view.speed;
        let included_selection = selection::included(state);
        let mixed = mixed_selection(&included_selection);
        if self.transform_box && mixed {
            if let Some((first, last)) =
                planning::time_bounds(&included_selection).filter(|(a, b)| a < b)
            {
                let x = event.position.x;
                let edge = if (f32::from(x - view.point(bounds, first as f64, 0.0).x)).abs() < 8.0 {
                    Some(-1)
                } else if (f32::from(x - view.point(bounds, last as f64, 0.0).x)).abs() < 8.0 {
                    Some(1)
                } else {
                    None
                };
                if let Some(edge) = edge {
                    if let Ok(gesture) =
                        planning::TimeGesture::new(state, view, bounds, event.position, Some(edge))
                    {
                        self.drag = Some(Drag::Time {
                            id,
                            property,
                            gesture,
                            bounds,
                            remove_on_click: None,
                        });
                        cx.notify();
                        return;
                    }
                }
            }
        }
        if self.transform_box
            && !mixed
            && let Some(transform) = transform::Transform::new(state, view, bounds, event.position)
        {
            self.drag = Some(Drag::Transform {
                id,
                property,
                transform,
            });
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let tangents = if mixed || (self.transform_box && selection::active(state).len() > 1) {
            vec![]
        } else {
            tangent::for_selection(
                track,
                &selection::active(state).iter().map(|k| k.frame).collect(),
            )
        };
        let hit = track
            .keys()
            .iter()
            .filter(|(f, _)| {
                **f >= state.timeline_start && **f <= state.timeline_start + state.visible_frames()
            })
            .find_map(|(&f, k)| {
                let near = |v: f64, offset: f32| {
                    let p = view.point(bounds, f as f64, v);
                    f32::from(p.x - event.position.x).abs() < 9.0 + offset.abs()
                        && (f32::from(p.x - event.position.x) + offset).abs() < 7.0
                        && f32::from(p.y - event.position.y).abs() < 9.0
                };
                if speed_mode {
                    speed::ends(track, f, fps)
                        .into_iter()
                        .find(|(incoming, v)| near(*v, if *incoming { -5.0 } else { 5.0 }))
                        .map(|(incoming, _)| {
                            (
                                f,
                                k.value,
                                Some((
                                    incoming,
                                    track.temporal_handle(f, incoming).unwrap_or(
                                        libre_effects_core::TemporalHandle {
                                            slope: 0.0,
                                            influence: 1.0 / 3.0,
                                        },
                                    ),
                                    fps,
                                )),
                            )
                        })
                } else {
                    near(k.value, 0.0).then_some((f, k.value, None))
                }
            });
        if hit.is_none() && !layer.locked() {
            if let Some(tangent) = tangents
                .iter()
                .find(|t| {
                    let (_, p) = t.points(view, bounds, speed_mode, fps);
                    bounds.contains(&p)
                        && f32::from(p.x - event.position.x).abs() < 8.0
                        && f32::from(p.y - event.position.y).abs() < 8.0
                })
                .copied()
            {
                self.drag = Some(Drag::Tangent {
                    id,
                    property,
                    tangent,
                    handle: tangent.handle,
                    split: event.modifiers.alt,
                    start: event.position,
                    moved: false,
                    view,
                    bounds,
                    fps,
                    speed: speed_mode,
                });
                self.state.update(cx, |s, cx| {
                    s.graph_key = Some(KeyRef {
                        id,
                        property,
                        frame: tangent.frame,
                    });
                    cx.notify();
                });
                self.freeze_drag(cx);
                cx.notify();
                return;
            }
        }
        if hit.is_none() && !mixed && tangents.is_empty() && !layer.locked() && !speed_mode {
            if let Some((_, frame, _)) = selected(state)
                && let Some(curve) = curve_at(state)
                && let Some(space) = HandleSpace::segment(view, bounds, track, frame)
            {
                for (index, (x, y)) in [(curve.x1, curve.y1), (curve.x2, curve.y2)]
                    .into_iter()
                    .enumerate()
                {
                    let p = space.point(x, y);
                    if f32::from(p.x - event.position.x).abs() < 10.0
                        && f32::from(p.y - event.position.y).abs() < 10.0
                    {
                        self.drag = Some(Drag::Handle {
                            id,
                            property,
                            frame,
                            index,
                            curve,
                            original: curve,
                            start: event.position,
                            moved: false,
                            space,
                        });
                        self.freeze_drag(cx);
                        cx.notify();
                        return;
                    }
                }
            }
        }
        if let Some((frame, value, velocity)) = hit {
            let key = libre_effects_core::KeyRef {
                id,
                property,
                frame,
            };
            let existing = selection::included(state)
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            let toggle = event.modifiers.shift || event.modifiers.control;
            let all_unlocked = existing.iter().all(|key| {
                state
                    .editor
                    .project()
                    .composition()
                    .layer(key.id)
                    .is_some_and(|layer| !layer.locked())
            });
            let remove_on_click =
                !layer.locked() && all_unlocked && toggle && existing.contains(&key);
            let chosen = if remove_on_click {
                existing
            } else {
                selection::clicked(existing, key, toggle)
            };
            let samples = selection::snapshot(
                track,
                &chosen
                    .iter()
                    .filter(|key| key.id == id && key.property == property)
                    .copied()
                    .collect::<Vec<_>>(),
                velocity.map(|(side, _, _)| side),
            );
            let mixed = mixed_selection(&chosen.iter().copied().collect::<Vec<_>>());
            if !mixed && !layer.locked() && chosen.contains(&key) {
                let snapping = snapping::Targets::new(
                    state,
                    track,
                    &samples,
                    velocity.map(|(side, _, _)| side),
                );
                self.drag = Some(Drag::Key {
                    id,
                    property,
                    from: frame,
                    to: frame,
                    value,
                    velocity,
                    keys: samples,
                    snapping,
                    guides: Default::default(),
                    origin: velocity.map_or(value, |(_, h, _)| h.slope),
                    remove_on_click,
                    view,
                    bounds,
                    start: event.position,
                    moved: false,
                });
            }
            self.state.update(cx, |s, cx| {
                s.graph_key = if chosen.contains(&key) {
                    Some(key)
                } else {
                    chosen.first().copied()
                };
                s.selected_keys = chosen;
                s.dispatch(&Action::Seek(frame), window, cx);
            });
            if mixed && chosen_contains_key(self.state.read(cx), key) {
                match planning::TimeGesture::new(
                    self.state.read(cx),
                    view,
                    bounds,
                    event.position,
                    None,
                ) {
                    Ok(mut gesture) => {
                        gesture.set_snap_playhead(snap_playhead);
                        self.drag = Some(Drag::Time {
                            id,
                            property,
                            gesture,
                            bounds,
                            remove_on_click: remove_on_click.then_some(key),
                        })
                    }
                    Err(error) => self.state.update(cx, |s, cx| {
                        s.status = error;
                        cx.notify();
                    }),
                }
            }
        } else {
            self.drag = Some(Drag::Marquee {
                id,
                property,
                start: event.position,
                end: event.position,
                additive: event.modifiers.shift || event.modifiers.control,
                view,
                bounds,
                lanes: self
                    .lanes
                    .borrow()
                    .values()
                    .copied()
                    .filter(|lane| {
                        self.lane_clip
                            .get()
                            .is_none_or(|clip| lane.bounds.intersects(&clip))
                    })
                    .collect(),
            });
        }
        self.freeze_drag(cx);
        cx.notify();
    }
    fn handle_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let state = self.state.read(cx);
        self.drag_revision = state.document_revision;
        self.drag_tool = state.tool;
        let Some((id, frame, property)) = selected(state) else {
            return;
        };
        if !graph_editable(state)
            || state
                .editor
                .project()
                .composition()
                .layer(id)
                .is_none_or(|l| l.locked())
        {
            return;
        }
        let Some(curve) = curve_at(state) else {
            return;
        };
        let Some(bounds) = self.easing.get() else {
            return;
        };
        for (index, (x, y)) in [(curve.x1, curve.y1), (curve.x2, curve.y2)]
            .into_iter()
            .enumerate()
        {
            let p = EASE_VIEW.point(bounds, x, y);
            if f32::from(p.x - event.position.x).abs() < 12.0
                && f32::from(p.y - event.position.y).abs() < 12.0
            {
                self.drag = Some(Drag::Handle {
                    id,
                    property,
                    frame,
                    index,
                    curve,
                    original: curve,
                    start: event.position,
                    moved: false,
                    space: HandleSpace {
                        view: EASE_VIEW,
                        bounds,
                        from: 0.0,
                        span: 1.0,
                        low: 0.0,
                        delta: 1.0,
                        inline: false,
                    },
                });
                break;
            }
        }
        self.freeze_drag(cx);
        cx.notify();
    }
    fn moving(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let button = match &self.drag {
            Some(Drag::Pan { pan, .. }) => pan.button,
            _ => MouseButton::Left,
        };
        if event.pressed_button != Some(button) {
            return;
        }
        if matches!(
            self.drag,
            Some(
                Drag::Key { .. }
                    | Drag::Tangent { .. }
                    | Drag::Handle { .. }
                    | Drag::Marquee { .. }
            )
        ) && self
            .frozen
            .as_ref()
            .is_none_or(|context| !context.current(self.state.read(cx), self.plot.get()))
        {
            self.drag = None;
            self.frozen = None;
            cx.notify();
            return;
        }
        match &mut self.drag {
            Some(Drag::Time {
                gesture,
                bounds,
                id,
                property,
                ..
            }) => {
                let current = self
                    .lanes
                    .borrow()
                    .get(&GraphChannel {
                        id: *id,
                        property: *property,
                    })
                    .map(|lane| lane.bounds);
                if current != Some(*bounds) || !gesture.is_current(self.state.read(cx), current) {
                    self.drag = None;
                    cx.notify();
                    return;
                }
                gesture.update_pointer(
                    event.position,
                    event.modifiers.alt,
                    event.modifiers.control,
                );
                cx.stop_propagation();
            }
            Some(Drag::Transform { transform, .. }) => {
                if !transform.is_current(self.state.read(cx))
                    || !transform.geometry_current(self.plot.get())
                {
                    self.drag = None;
                    cx.notify();
                    return;
                }
                transform.update_pointer(
                    event.position,
                    event.modifiers.alt,
                    event.modifiers.control,
                );
                cx.stop_propagation();
            }
            Some(Drag::Zoom { zoom, end, .. }) => {
                *end = event.position;
                self.state.update(cx, |s, cx| {
                    zoom.moving(s, event.position);
                    cx.notify();
                });
                cx.stop_propagation();
            }
            Some(Drag::Pan { pan, .. }) => {
                self.state.update(cx, |s, cx| {
                    pan.apply(s, event.position);
                    cx.notify();
                });
                cx.stop_propagation();
            }
            Some(Drag::Tangent {
                tangent,
                handle,
                split,
                start,
                moved,
                view,
                bounds,
                fps,
                speed,
                ..
            }) => {
                let delta = event.position - *start;
                if !*moved && f32::from(delta.x).abs() + f32::from(delta.y).abs() < 3.0 {
                    return;
                }
                *moved = true;
                *split |= event.modifiers.alt;
                if delta.x == px(0.0) && delta.y == px(0.0) {
                    *handle = tangent.handle;
                    cx.notify();
                    return;
                }
                *handle =
                    tangent.dragged(*view, *bounds, *speed, *fps, delta, event.modifiers.shift);
            }
            Some(Drag::Marquee { end, .. }) => {
                *end = event.position;
            }
            Some(Drag::Key {
                from,
                to,
                value,
                velocity,
                keys,
                snapping,
                guides,
                origin,
                view,
                bounds,
                start,
                moved,
                ..
            }) => {
                let mut dx = f32::from(event.position.x - start.x);
                let mut dy = f32::from(event.position.y - start.y);
                if !*moved && dx.abs() + dy.abs() < 3.0 {
                    return;
                }
                *moved = true;
                if event.modifiers.shift {
                    if dx.abs() > dy.abs() {
                        dy = 0.0;
                    } else {
                        dx = 0.0;
                    }
                }
                let raw_delta =
                    dx as f64 / f32::from(bounds.size.width).max(1.0) as f64 * view.span;
                let state = self.state.read(cx);
                let duration = state.editor.project().composition().duration();
                let raw_amount = -(dy as f64) / f32::from(bounds.size.height).max(1.0) as f64
                    * (view.high - view.low)
                    / velocity.map_or(1.0, |(_, _, fps)| fps);
                let (delta, amount, matched) = snapping.apply(
                    keys,
                    raw_delta,
                    raw_amount,
                    *view,
                    *bounds,
                    duration,
                    velocity.map(|(side, _, _)| side),
                    snapping::enabled(state.snapping, event.modifiers.control, event.modifiers.alt),
                    dx != 0.0,
                    dy != 0.0,
                );
                *guides = matched;
                *to = (*from as i64 + delta) as u32;
                if let Some((_, handle, _)) = velocity {
                    handle.slope = *origin + amount;
                } else {
                    *value = *origin + amount;
                }
            }
            Some(Drag::Handle {
                index,
                curve,
                original,
                start,
                moved,
                space,
                ..
            }) => {
                if f32::from(event.position.x - start.x).abs()
                    + f32::from(event.position.y - start.y).abs()
                    < 3.0
                {
                    *curve = *original;
                    cx.notify();
                    return;
                }
                *moved = true;
                *curve = handle_draft(*space, *original, *index, *start, event.position);
            }
            None => return,
        }
        cx.notify();
    }
    fn up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let button = match &self.drag {
            Some(Drag::Pan { pan, .. }) => pan.button,
            _ => MouseButton::Left,
        };
        if event.button != button {
            return;
        }
        // A release may arrive outside the lane without a final move event.
        self.moving(
            &MouseMoveEvent {
                position: event.position,
                pressed_button: Some(button),
                modifiers: event.modifiers,
            },
            window,
            cx,
        );
        if let Some(drag) = self.drag.take() {
            if let Drag::Key {
                id,
                property,
                from,
                moved: false,
                remove_on_click: true,
                ..
            } = &drag
            {
                self.state.update(cx, |s, cx| {
                    s.selected_keys.remove(&libre_effects_core::KeyRef {
                        id: *id,
                        property: *property,
                        frame: *from,
                    });
                    s.graph_key = selection::active(s).first().copied();
                    cx.notify();
                });
            }
            if matches!(
                &drag,
                Drag::Key { moved: false, .. }
                    | Drag::Handle { moved: false, .. }
                    | Drag::Tangent { moved: false, .. }
            ) {
                cx.notify();
                return;
            }
            self.state.update(cx, |state, cx| match drag {
                Drag::Time {
                    gesture,
                    bounds,
                    remove_on_click,
                    ..
                } => {
                    if !gesture.is_current(state, Some(bounds)) {
                        return;
                    }
                    if !gesture.moved {
                        if let Some(key) = remove_on_click {
                            state.selected_keys.remove(&key);
                            state.graph_key = selection::active(state).first().copied();
                        }
                        cx.notify();
                    } else {
                        apply_plan(state, gesture.preview, window, cx);
                    }
                }
                Drag::Transform { transform, .. } => {
                    if !transform.is_current(state) || !transform.geometry_current(self.plot.get())
                    {
                        return;
                    }
                    if transform.moved && (transform.has_changes() || transform.preview.is_err()) {
                        match transform.command() {
                            Ok((command, moved)) => {
                                let active = transform.active;
                                state.dispatch(&Action::Edit(command), window, cx);
                                if state.status.starts_with("Edited") {
                                    state.graph_key = Some(moved[active]);
                                    state.frame = moved[active].frame;
                                    state.selected_keys = moved.into_iter().collect();
                                }
                            }
                            Err(error) => state.status = error,
                        }
                        cx.notify();
                    }
                }
                Drag::Zoom { zoom, .. } => {
                    zoom.finish(state, event.position);
                    cx.notify();
                }
                Drag::Pan { .. } => {
                    cx.notify();
                }
                Drag::Tangent {
                    id,
                    property,
                    tangent,
                    handle,
                    split,
                    ..
                } => {
                    if handle != tangent.handle || split {
                        state.dispatch(
                            &Action::Edit(tangent.command(id, property, handle, split)),
                            window,
                            cx,
                        );
                    }
                }
                Drag::Marquee {
                    start,
                    additive,
                    view,
                    bounds,
                    lanes,
                    ..
                } => {
                    let end = event.position;
                    if lanes.iter().any(|lane| {
                        self.lanes
                            .borrow()
                            .get(&lane.channel)
                            .is_none_or(|current| current.bounds != lane.bounds)
                    }) {
                        return;
                    }
                    let moved =
                        f32::from(end.x - start.x).abs() + f32::from(end.y - start.y).abs() >= 3.0;
                    let mut keys = if additive {
                        selection::included(state)
                            .into_iter()
                            .collect::<BTreeSet<_>>()
                    } else {
                        BTreeSet::new()
                    };
                    if moved {
                        keys.extend(marquee_keys(
                            state,
                            &lanes,
                            start,
                            end,
                            self.lane_clip.get(),
                        ));
                    }
                    state.selected_keys = keys;
                    state.graph_key = selection::active(state).first().copied();
                    if !moved && !additive {
                        let (frame, _) = view.value(bounds, end);
                        state.dispatch(&Action::Seek(frame.max(0.0).round() as u32), window, cx);
                    }
                    cx.notify();
                }
                Drag::Key {
                    id,
                    property,
                    from,
                    to,
                    value,
                    velocity,
                    keys,
                    origin,
                    ..
                } => {
                    let delta = to as i64 - from as i64;
                    let amount = velocity.map_or(value - origin, |(_, h, _)| h.slope - origin);
                    if delta == 0 && amount == 0.0 {
                        return;
                    }
                    match selection::translate(
                        &keys,
                        delta,
                        amount,
                        velocity.map(|(side, _, _)| side),
                    ) {
                        Ok(command) => {
                            state.dispatch(&Action::Edit(command), window, cx);
                            if state.status.starts_with("Edited") {
                                state.selected_keys = keys
                                    .into_iter()
                                    .map(|s| {
                                        let mut k = s.key;
                                        k.frame = (k.frame as i64 + delta) as u32;
                                        k
                                    })
                                    .collect();
                                state.graph_key = Some(KeyRef {
                                    id,
                                    property,
                                    frame: to,
                                });
                                state.frame = to;
                                cx.notify();
                            }
                        }
                        Err(error) => {
                            state.status = error;
                            cx.notify();
                        }
                    }
                }
                Drag::Handle {
                    id,
                    property,
                    frame,
                    curve,
                    ..
                } => {
                    if curve_at(state) == Some(curve) {
                        return;
                    }
                    state.dispatch(
                        &Action::Edit(Command::EditTrack {
                            id,
                            property,
                            edit: TrackEdit::Interpolate {
                                frame,
                                interpolation: Interpolation::Bezier(curve),
                            },
                        }),
                        window,
                        cx,
                    );
                }
            });
        }
        cx.notify();
    }
}
fn stroke(
    window: &mut Window,
    points: impl IntoIterator<Item = Point<Pixels>>,
    color: u32,
    width: f32,
) {
    let mut path = PathBuilder::stroke(px(width));
    let mut points = points.into_iter();
    if let Some(p) = points.next() {
        path.move_to(p);
    }
    for p in points {
        path.line_to(p);
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, rgb(color));
    }
}
fn dot(window: &mut Window, p: Point<Pixels>, color: u32) {
    window.paint_quad(fill(
        Bounds::new(p - point(px(4.0), px(4.0)), size(px(8.0), px(8.0))),
        rgb(color),
    ));
}
impl Graph {
    fn channel_list(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let state = self.state.read(cx);
        let mut channels = state.graph_channels.pinned.clone();
        if let Some(active) = state.graph_active_channel() {
            if !channels.contains(&active) {
                channels.push(active);
            }
        }
        let mut list = div()
            .id("graph-channel-list")
            .flex_none()
            .max_h(px(78.0))
            .overflow_y_scroll()
            .flex()
            .flex_wrap()
            .gap_1()
            .px_1()
            .py_1()
            .bg(rgb(ui::PANEL))
            .child(div().text_size(px(10.0)).child(format!(
                "Channels · {}/16 pinned",
                state.graph_channels.pinned.len()
            )));
        for channel in channels {
            let available = channel.available(state.editor.project().composition());
            let pinned = state.graph_channels.is_pinned(channel);
            let label = channels::describe(state.editor.project(), channel)
                .map(|d| d.label)
                .unwrap_or_else(|| {
                    format!(
                        "Unavailable · Layer #{} · {:?}",
                        channel.id, channel.property
                    )
                });
            let caption = if available {
                format!(
                    "{} #{} {}",
                    if pinned { "◆" } else { "◇" },
                    channel.id,
                    state
                        .editor
                        .project()
                        .composition()
                        .layer(channel.id)
                        .and_then(|layer| layer.track_label(channel.property))
                        .unwrap_or_default()
                )
            } else {
                label.clone()
            };
            let tooltip = label.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        ui::text_button(
                            SharedString::from(format!(
                                "graph-channel-{}-{:?}",
                                channel.id, channel.property
                            )),
                            caption,
                        )
                        .text_size(px(10.0))
                        .text_color(rgb(if available {
                            channel_color(channel)
                        } else {
                            ui::MUTED
                        }))
                        .when(state.graph_active_channel() == Some(channel), |s| {
                            s.bg(rgb(0x34495c))
                        })
                        .tooltip(move |_, cx| cx.new(|_| ui::Tip(tooltip.clone().into())).into())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                if !available {
                                    return;
                                }
                                this.drag = None;
                                window.focus(&this.focus);
                                this.state.update(cx, |s, cx| {
                                    s.graph_activate_channel(channel, true);
                                    cx.notify();
                                });
                            },
                        )),
                    )
                    .when(pinned, |s| {
                        s.child(
                            ui::text_button(
                                SharedString::from(format!(
                                    "remove-channel-{}-{:?}",
                                    channel.id, channel.property
                                )),
                                "×",
                            )
                            .tooltip(|_, cx| cx.new(|_| ui::Tip("Unpin channel".into())).into())
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.drag = None;
                                    this.state.update(cx, |s, cx| {
                                        s.graph_unpin_channel(channel);
                                        cx.notify();
                                    });
                                },
                            )),
                        )
                    }),
            );
        }
        list.into_any_element()
    }
    fn lane(
        &self,
        channel: GraphChannel,
        multiple: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let state = self.state.read(cx);
        let layer = state
            .editor
            .project()
            .composition()
            .layer(channel.id)
            .expect("included Graph layer");
        let track = layer
            .track(channel.property)
            .expect("included Graph channel")
            .clone();
        let active = state.graph_active_channel() == Some(channel);
        let property = channel.property;
        let current = state.frame;
        let fps = state.editor.project().composition().fps().as_f64();
        let all_selected = selection::included(state);
        let mixed = mixed_selection(&all_selected);
        let selected_frames = all_selected
            .iter()
            .filter(|key| GraphChannel::from(**key) == channel)
            .map(|key| key.frame)
            .collect::<BTreeSet<_>>();
        let selection = selected(state)
            .filter(|(id, _, property)| *id == channel.id && *property == channel.property);
        let curve = active.then(|| curve_at(state)).flatten();
        let locked = layer.locked();
        let pinned = state.graph_channels.is_pinned(channel);
        let descriptor = channels::describe(state.editor.project(), channel);
        let label = descriptor
            .as_ref()
            .map(|d| d.label.clone())
            .unwrap_or_else(|| format!("Layer #{} · {:?}", channel.id, property));
        let units = descriptor
            .as_ref()
            .map_or("", |d| d.units.label(state.graph_view.speed));
        let color = channel_color(channel);
        let lane_drag = self
            .drag
            .as_ref()
            .filter(|drag| match drag {
                Drag::Time { .. } | Drag::Marquee { .. } => true,
                Drag::Zoom { id, property, .. }
                | Drag::Transform { id, property, .. }
                | Drag::Pan { id, property, .. }
                | Drag::Key { id, property, .. }
                | Drag::Handle { id, property, .. }
                | Drag::Tangent { id, property, .. } => {
                    *id == channel.id && *property == channel.property
                }
            })
            .cloned();
        let shared_guides = match &self.drag {
            Some(Drag::Time { gesture, .. }) => Some(gesture.guides.clone()),
            Some(Drag::Key { guides, .. }) => Some(guides.clone()),
            Some(Drag::Transform { transform, .. }) => Some(transform.guides.clone()),
            _ => None,
        };
        let mixed_bounds = (mixed && self.transform_box)
            .then(|| planning::time_bounds(&all_selected))
            .flatten()
            .filter(|(a, b)| a < b);
        let speed_mode = state.graph_view.speed;
        let mut graph_view = viewport::current_channel(state, channel, &track);
        if let Some(Drag::Key { view, .. } | Drag::Tangent { view, .. }) = &lane_drag {
            graph_view = *view;
        }
        if let Some(Drag::Marquee { lanes, .. }) = &lane_drag {
            if let Some(lane) = lanes.iter().find(|lane| lane.channel == channel) {
                graph_view = lane.view;
            }
        }
        if let Some(Drag::Handle { space, .. }) = &lane_drag
            && space.inline
        {
            graph_view = space.view;
        }
        if let Some(Drag::Transform { transform, .. }) = &lane_drag {
            graph_view = transform.view;
        }
        let measured = self.lanes.clone();
        let active_plot = self.plot.clone();
        let drag = lane_drag.clone();
        let plot_track = if let Some(Drag::Time { gesture, .. }) = &lane_drag {
            gesture
                .preview
                .as_ref()
                .ok()
                .and_then(|plan| plan.tracks.get(&channel))
                .cloned()
                .unwrap_or_else(|| track.clone())
        } else if let Some(Drag::Transform { transform, .. }) = &lane_drag {
            transform
                .preview
                .as_ref()
                .map(|(_, track)| track.clone())
                .unwrap_or_else(|_| track.clone())
        } else if let Some(Drag::Tangent {
            tangent,
            handle,
            split,
            moved: true,
            ..
        }) = &lane_drag
        {
            track
                .preview_temporal_handle(tangent.frame, tangent.incoming, *handle, *split)
                .unwrap_or_else(|_| track.clone())
        } else {
            track.clone()
        };
        let paint_frames = if let Some(Drag::Time { gesture, .. }) = &lane_drag {
            gesture
                .preview
                .as_ref()
                .map(|plan| {
                    plan.keys
                        .iter()
                        .filter(|key| GraphChannel::from(**key) == channel)
                        .map(|key| key.frame)
                        .collect()
                })
                .unwrap_or_else(|_| selected_frames.clone())
        } else if let Some(Drag::Transform { transform, .. }) = &lane_drag {
            transform.frames()
        } else {
            selected_frames.clone()
        };
        let transform_box = (active && !mixed && self.transform_box && !locked)
            .then(|| transform::SelectionBox::new(&plot_track, &paint_frames, speed_mode, fps))
            .flatten();
        let speed_hint = transform_box
            .as_ref()
            .and_then(|area| area.velocity_disabled_reason())
            .map(|reason| format!("Time handles only · {reason}"));
        let tangents = if !active || mixed || transform_box.is_some() {
            vec![]
        } else {
            tangent::for_selection(&plot_track, &paint_frames)
        };
        let speed_curves = if speed_mode {
            graph_view.speed_curves(&plot_track, fps)
        } else {
            vec![]
        };
        let chart = div().flex_1().min_h_0().min_w_0().flex().flex_col().child(
            div()
                .id(SharedString::from(format!("graph-lane-{}-{:?}", channel.id, channel.property)))
                .flex_1()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .bg(rgb(0x262626))
                .cursor_crosshair()
                .when(
                    state.tool == Tool::Hand
                        || self.hand.held
                        || matches!(self.drag, Some(Drag::Pan { .. })),
                    |s| s.cursor_grab(),
                )
                .on_mouse_down(MouseButton::Left, cx.listener(move |this,event,window,cx| this.down(channel,event,window,cx)))
                .on_mouse_down(MouseButton::Middle, cx.listener(move |this,event,window,cx| this.down(channel,event,window,cx)))
                .on_scroll_wheel(cx.listener(move |this,event,window,cx| this.scroll(channel,event,window,cx)))
                .child(
                    canvas(
                        move |bounds, _, _| {
                            measured.borrow_mut().insert(channel, LaneGeometry { channel, bounds, view: graph_view });
                            if active { active_plot.set(Some(bounds)); }
                        },
                        move |bounds, _, window, _| {
                            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                for i in 0..=10 {
                                    let x = bounds.left() + bounds.size.width * (i as f32 / 10.0);
                                    stroke(
                                        window,
                                        [point(x, bounds.top()), point(x, bounds.bottom())],
                                        0x343434,
                                        1.0,
                                    );
                                }
                                for i in 0..=4 {
                                    let y = bounds.top() + bounds.size.height * (i as f32 / 4.0);
                                    stroke(
                                        window,
                                        [point(bounds.left(), y), point(bounds.right(), y)],
                                        0x343434,
                                        1.0,
                                    );
                                }
                                let zero = graph_view.point(bounds, graph_view.start, 0.0).y;
                                stroke(
                                    window,
                                    [point(bounds.left(), zero), point(bounds.right(), zero)],
                                    0x555555,
                                    1.0,
                                );
                                let evaluate = |frame: f64| {
                                    if let Some(Drag::Handle {
                                        frame: from, curve, ..
                                    }) = &drag
                                    {
                                        if let Some((&end, b)) =
                                            plot_track.keys().range(from + 1..).next()
                                        {
                                            if frame >= *from as f64 && frame <= end as f64 {
                                                let a = plot_track.keys()[from].value;
                                                return a
                                                    + (b.value - a)
                                                        * curve.progress(
                                                            (frame - *from as f64)
                                                                / (end - *from) as f64,
                                                        );
                                            }
                                        }
                                    }
                                    plot_track.sample(frame)
                                };
                                if speed_mode {
                                    for line in &speed_curves {
                                        stroke(
                                            window,
                                            line.iter()
                                                .map(|(f, v)| graph_view.point(bounds, *f, *v)),
                                            color,
                                            1.5,
                                        );
                                    }
                                } else {
                                    stroke(
                                        window,
                                        (0..=600).map(|i| {
                                            let f = graph_view.start
                                                + graph_view.span * i as f64 / 600.0;
                                            graph_view.point(bounds, f, evaluate(f))
                                        }),
                                        color,
                                        1.5,
                                    );
                                }
                                if active && !mixed && !speed_mode
                                    && tangents.is_empty()
                                    && let Some((_, frame, _)) = selection
                                    && let Some(mut curve) = curve
                                    && let Some(space) =
                                        HandleSpace::segment(graph_view, bounds, &plot_track, frame)
                                {
                                    if let Some(Drag::Handle { curve: preview, .. }) = &drag {
                                        curve = *preview;
                                    }
                                    stroke(
                                        window,
                                        [space.point(0.0, 0.0), space.point(curve.x1, curve.y1)],
                                        ui::BLUE,
                                        1.0,
                                    );
                                    stroke(
                                        window,
                                        [space.point(1.0, 1.0), space.point(curve.x2, curve.y2)],
                                        ui::BLUE,
                                        1.0,
                                    );
                                    dot(window, space.point(curve.x1, curve.y1), ui::BLUE);
                                    dot(window, space.point(curve.x2, curve.y2), ui::BLUE);
                                }
                                if !locked {
                                    for &t in &tangents {
                                        tangent::paint(
                                            window, t, graph_view, bounds, speed_mode, fps,
                                        );
                                    }
                                }
                                for (&f, k) in plot_track.keys() {
                                    if speed_mode {
                                        for (incoming, v) in speed::ends(&plot_track, f, fps) {
                                            let mut p = graph_view.point(bounds, f as f64, v);
                                            p.x += px(if incoming { -5.0 } else { 5.0 });
                                            dot(
                                                window,
                                                p,
                                                if paint_frames.contains(&f) {
                                                    ui::BLUE
                                                } else {
                                                    color
                                                },
                                            );
                                        }
                                        continue;
                                    }
                                    dot(
                                        window,
                                        graph_view.point(bounds, f as f64, k.value),
                                        if paint_frames.contains(&f) {
                                            ui::BLUE
                                        } else {
                                            color
                                        },
                                    );
                                }
                                if let Some(Drag::Key {
                                    from,
                                    to,
                                    value,
                                    velocity,
                                    keys,
                                    origin,
                                    ..
                                }) = &drag
                                {
                                    let delta = *to as i64 - *from as i64;
                                    let amount = velocity
                                        .map_or(*value - *origin, |(_, h, _)| h.slope - *origin);
                                    for sample in keys {
                                        let val = if let Some((_, _, fps)) = velocity {
                                            sample.handle.map(|h| (h.slope + amount) * fps)
                                        } else {
                                            Some(sample.value + amount)
                                        };
                                        if let Some(val) = val {
                                            dot(
                                                window,
                                                graph_view.point(
                                                    bounds,
                                                    (sample.key.frame as i64 + delta) as f64,
                                                    val,
                                                ),
                                                0xffffff,
                                            );
                                        }
                                    }
                                }
                                if let Some(transform_box) = &transform_box {
                                    let invalid = matches!(&drag, Some(Drag::Transform { transform, .. }) if transform.preview.is_err());
                                    transform_box.paint(graph_view,bounds,window,invalid);
                                }
                                if let Some((first,last)) = mixed_bounds {
                                    for frame in [first,last] {
                                        let x = graph_view.point(bounds, frame as f64, 0.0).x;
                                        stroke(window, [point(x,bounds.top()), point(x,bounds.bottom())], ui::BLUE, 1.0);
                                        dot(window, point(x,bounds.center().y), ui::BLUE);
                                    }
                                }
                                let area = match &drag {
                                    Some(Drag::Zoom { zoom, end, .. }) => zoom.area(*end),
                                    Some(Drag::Marquee { start, end, .. }) => {
                                        Some(Bounds::from_corners(
                                            point(start.x.min(end.x), start.y.min(end.y)),
                                            point(start.x.max(end.x), start.y.max(end.y)),
                                        ))
                                    }
                                    _ => None,
                                };
                                if let Some(area) = area {
                                    window.paint_quad(fill(area, gpui::rgba(0x4ba6ff22)));
                                    stroke(
                                        window,
                                        [
                                            area.origin,
                                            point(area.right(), area.top()),
                                            point(area.right(), area.bottom()),
                                            point(area.left(), area.bottom()),
                                            area.origin,
                                        ],
                                        ui::BLUE,
                                        1.0,
                                    );
                                }
                                let x = graph_view.point(bounds, current as f64, 0.0).x;
                                stroke(
                                    window,
                                    [point(x, bounds.top()), point(x, bounds.bottom())],
                                    ui::BLUE,
                                    1.0,
                                );
                                let guides = shared_guides.as_ref();
                                if let Some(guides) = guides {
                                    if let Some(frame) = guides.frame {
                                        let x = graph_view.point(bounds, frame as f64, 0.0).x;
                                        stroke(
                                            window,
                                            [point(x, bounds.top()), point(x, bounds.bottom())],
                                            0xff9c42,
                                            1.5,
                                        );
                                    }
                                    if active && let Some(value) = guides.value {
                                        let y = graph_view.point(bounds, graph_view.start, value).y;
                                        stroke(
                                            window,
                                            [point(bounds.left(), y), point(bounds.right(), y)],
                                            0xff9c42,
                                            1.5,
                                        );
                                    }
                                }
                            });
                        },
                    )
                    .size_full(),
                )
                .children((1..4).map(|i| {
                    let value =
                        graph_view.high - (graph_view.high - graph_view.low) * i as f64 / 4.0;
                    div()
                        .absolute()
                        .left(px(5.0))
                        .top(relative(i as f32 / 4.0))
                        .text_size(px(10.0))
                        .text_color(rgb(ui::MUTED))
                        .child(format!("{value:.1}"))
                })),
        );
        let header = div()
            .h(px(26.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px_1()
            .bg(rgb(if active { 0x293849 } else { ui::PANEL }))
            .child(div().w(px(4.0)).h(px(16.0)).bg(rgb(color)))
            .child(
                ui::text_button(
                    SharedString::from(format!("lane-activate-{}-{:?}", channel.id, property)),
                    format!("{}{}", if active { "● " } else { "" }, label),
                )
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .justify_start()
                .text_size(px(10.0))
                .tooltip(move |_, cx| {
                    cx.new(|_| {
                        ui::Tip(
                            format!("{label} · {units}{}", if locked { " · Locked" } else { "" })
                                .into(),
                        )
                    })
                    .into()
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.drag = None;
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        s.graph_activate_channel(channel, true);
                        cx.notify();
                    });
                })),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child(units),
            )
            .when(locked, |s| {
                s.child(div().flex_none().text_size(px(10.0)).child("Locked"))
            })
            .child(
                ui::text_button(
                    SharedString::from(format!("lane-pin-{}-{:?}", channel.id, property)),
                    if pinned { "Unpin" } else { "Pin" },
                )
                .text_size(px(10.0))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.drag = None;
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        if s.graph_channels.is_pinned(channel) {
                            s.graph_unpin_channel(channel);
                        } else if let Err(error) = s.graph_pin_channel(channel) {
                            s.status = error;
                        }
                        cx.notify();
                    });
                })),
            )
            .child(
                ui::text_button(
                    SharedString::from(format!("lane-select-{}-{:?}", channel.id, property)),
                    "Select all",
                )
                .text_size(px(10.0))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.drag = None;
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        s.graph_activate_channel(channel, true);
                        s.selected_keys
                            .extend(channels::all_keys(s.editor.project(), &[channel]));
                        s.graph_key = selection::active(s).first().copied();
                        cx.notify();
                    });
                })),
            )
            .child(
                ui::text_button(
                    SharedString::from(format!("lane-height-{}-{:?}", channel.id, property)),
                    "Fit Y",
                )
                .text_size(px(10.0))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.drag = None;
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        s.graph_set_channel_height(channel, s.graph_view.speed, None);
                        cx.notify();
                    });
                })),
            );
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .border_b_1()
            .border_color(rgb(ui::BORDER))
            .when(multiple, |s| s.h(px(180.0)).flex_none())
            .when(!multiple, |s| s.flex_1())
            .child(header)
            .child(chart)
            .when_some(speed_hint, |s, hint| {
                s.child(
                    div()
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(rgb(ui::MUTED))
                        .child(hint),
                )
            })
            .into_any_element()
    }
}
impl Render for Graph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_watch.is_none() {
            self.focus_watch = Some([
                cx.on_blur(&self.focus.clone(), window, |this, _, cx| {
                    this.cancel_navigation(cx)
                }),
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.cancel_navigation(cx);
                    }
                }),
            ]);
        }
        let state = self.state.read(cx);
        let property = state
            .graph_active_channel()
            .map_or(state.graph_property, |channel| channel.property);
        if matches!(property, PropertyPath::Path(_)) {
            return div().id("path-graph-help").size_full().p_4().child("Path geometry is edited in the Composition viewer. Move, copy and ease its keyframes in the timeline.")
                .child(ui::action_tool("path-return-timeline", "pen", "Return to path timeline", &self.state, Action::GraphProperty(state.editor.selected().unwrap_or(0),property), false));
        }
        let active_channel = state.graph_active_channel();
        let layer = active_channel
            .and_then(|channel| state.editor.project().composition().layer(channel.id))
            .cloned();
        let current = state.frame;
        let fps = state.editor.project().composition().fps().as_f64();
        let selection = selected(state);
        let included_keys = selection::included(state);
        let selected_count = included_keys.len();
        let mixed = mixed_selection(&included_keys);
        let input_target = if self.details
            && let Some(selected) = selection
        {
            GraphInputSource::refresh(&mut self.input_source, state);
            self.input_source.clone().map(|document| KeyInputTarget {
                document,
                selected,
                keys: selection::included(state),
                displayed_number: None,
            })
        } else {
            self.input_source = None;
            None
        };
        let curve = curve_at(state);
        let locked = layer.as_ref().is_none_or(|l| l.locked());
        let root = div()
            .id("graph-editor")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(ui::BG))
            .on_mouse_move(cx.listener(Self::moving))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::up))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::up))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let m = event.keystroke.modifiers;
                if !this.focus.is_focused(window) {
                    if this.focus.contains_focused(window, cx) && graph_owned_key(key, m) {
                        cx.stop_propagation();
                    }
                    return;
                }
                if !graph_editable(this.state.read(cx)) {
                    if graph_owned_key(key, m) {
                        cx.stop_propagation();
                    }
                    return;
                }
                if event.is_held && (graph_shortcut(key, m) || key == "f9") {
                    cx.stop_propagation();
                    return;
                }
                if matches!(key, "left" | "right" | "up" | "down") {
                    cx.stop_propagation();
                    if this.drag.is_none()
                        && !m.control
                        && !m.alt
                        && !m.platform
                        && matches!(key, "left" | "right")
                    {
                        this.state.update(cx, |s, cx| {
                            let delta =
                                if key == "left" { -1 } else { 1 } * if m.shift { 10 } else { 1 };
                            let keys = selection::included(s);
                            if keys.is_empty() {
                                s.dispatch(&Action::Step(delta), window, cx);
                            } else {
                                let result = planning::EditPlan::translate(
                                    s.editor.project(),
                                    &keys,
                                    delta as i64,
                                    s.graph_key,
                                );
                                apply_plan(s, result, window, cx);
                            }
                        });
                    }
                    return;
                }
                if m.control && matches!(key, "c" | "x" | "v" | "d") {
                    cx.stop_propagation();
                    if m.alt || m.shift || m.platform || this.drag.is_some() || event.is_held {
                        return;
                    }
                    this.state.update(cx, |s, cx| {
                        let keys = selection::included(s);
                        if key == "v" {
                            let result = planning::EditPlan::paste(s);
                            let active = result
                                .as_ref()
                                .ok()
                                .filter(|plan| plan.command.is_some())
                                .and_then(|plan| plan.active);
                            apply_plan(s, result, window, cx);
                            if let Some(active) = active
                                && s.graph_key == Some(active)
                                && s.selected_keys.contains(&active)
                            {
                                s.graph_activate_property(GraphChannel::from(active), true);
                                cx.notify();
                            }
                            return;
                        }
                        if key == "d" {
                            s.status =
                                "Use the Timeline to duplicate keys with an explicit destination."
                                    .into();
                            cx.notify();
                            return;
                        }
                        if keys.is_empty() {
                            return;
                        }
                        let result = (key == "x")
                            .then(|| planning::EditPlan::delete(s.editor.project(), &keys));
                        if result.as_ref().is_some_and(|result| result.is_err()) {
                            apply_plan(s, result.unwrap(), window, cx);
                            return;
                        }
                        let selected =
                            std::mem::replace(&mut s.selected_keys, keys.into_iter().collect());
                        s.dispatch(&Action::CopyKeys, window, cx);
                        s.selected_keys = selected;
                        if let Some(result) = result {
                            apply_plan(s, result, window, cx);
                        }
                        cx.notify();
                    });
                    return;
                }
                if m.alt && matches!(key, "[" | "]") {
                    cx.stop_propagation();
                    return;
                }
                if key == "space"
                    && this.focus.is_focused(window)
                    && !m.control
                    && !m.alt
                    && !m.platform
                    && !m.shift
                {
                    this.hand.press(event.is_held, this.drag.is_some());
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                this.hand.consume();
                if key == "f"
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                    && this.focus.is_focused(window)
                {
                    this.fit(event.keystroke.modifiers.shift, window, cx);
                    cx.stop_propagation();
                    return;
                }
                if key == "z"
                    && event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                    && !this.state.read(cx).queue_open
                {
                    this.drag = None;
                    this.state.update(cx, |s, cx| {
                        s.dispatch(
                            &if event.keystroke.modifiers.shift {
                                Action::Redo
                            } else {
                                Action::Undo
                            },
                            window,
                            cx,
                        )
                    });
                    window.focus(&this.focus);
                    cx.stop_propagation();
                    return;
                }
                if key == "escape" {
                    this.cancel_navigation(cx);
                    this.details = false;
                    cx.stop_propagation();
                    cx.notify();
                }
                if let Some((incoming, outgoing)) = super::key_easing::shortcut(event)
                    && this.focus.is_focused(window)
                    && this.drag.is_none()
                {
                    this.ease(incoming, outgoing, window, cx);
                    cx.stop_propagation();
                    return;
                }
                if key == "a" && graph_shortcut(key, m) {
                    this.drag = None;
                    this.state.update(cx, |s, cx| {
                        s.selected_keys =
                            channels::all_keys(s.editor.project(), &s.graph_included_channels());
                        s.graph_key = selection::active(s).first().copied();
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
                if matches!(key, "delete" | "backspace") {
                    cx.stop_propagation(); // Never let Graph deletion fall through to layer deletion.
                    if !graph_shortcut(key, m) || this.drag.is_some() {
                        return;
                    }
                    this.state.update(cx, |s, cx| {
                        let keys = selection::included(s);
                        if keys.is_empty() {
                            return;
                        }
                        let result = planning::EditPlan::delete(s.editor.project(), &keys);
                        apply_plan(s, result, window, cx);
                    });
                }
            }))
            .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, window, cx| {
                if event.keystroke.key == "space" && this.hand.held {
                    if this.hand.release() && this.focus.is_focused(window) {
                        this.state
                            .update(cx, |s, cx| s.dispatch(&Action::Play, window, cx));
                    }
                    cx.stop_propagation();
                    cx.notify();
                }
            }));
        let mut toolbar = div()
            .min_h(px(30.0))
            .flex_wrap()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.0))
            .px_1();
        for (label, speed_mode) in [("Value", false), ("Speed", true)] {
            toolbar = toolbar.child(
                ui::text_button(
                    SharedString::from(format!("graph-type-{speed_mode}")),
                    label,
                )
                .when(state.graph_view.speed == speed_mode, |s| {
                    s.bg(rgb(0x34495c))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.drag = None;
                    this.state.update(cx, |s, cx| {
                        if s.graph_view.speed != speed_mode {
                            s.graph_view.speed = speed_mode;
                            if !s.graph_channels.explicit {
                                s.graph_view.height = None;
                            }
                        }
                        cx.notify();
                    });
                })),
            );
        }
        if let Some(layer) = &layer {
            let id = layer.id();
            toolbar = toolbar.child(
                ui::tool(
                    "graph-add-key",
                    "diamond",
                    "Add / remove key at current frame",
                    layer
                        .track(property)
                        .is_some_and(|track| track.keys().contains_key(&current)),
                )
                .when(locked, |s| s.opacity(0.4))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.drag = None;
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        if graph_editable(s)
                            && s.editor
                                .project()
                                .composition()
                                .layer(id)
                                .is_some_and(|layer| !layer.locked())
                        {
                            s.dispatch(
                                &Action::Edit(Command::EditTrack {
                                    id,
                                    property,
                                    edit: TrackEdit::ToggleKey { frame: s.frame },
                                }),
                                window,
                                cx,
                            );
                        }
                    });
                })),
            );
        }
        for (label, interpolation) in [
            ("Linear", Interpolation::Linear),
            ("Hold", Interpolation::Hold),
        ] {
            toolbar = toolbar.child(
                ui::text_button(SharedString::from(format!("preset-{label}")), label)
                    .when(selected_count == 0, |s| s.opacity(0.4))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.preset(interpolation, window, cx)
                    })),
            );
        }
        for (label, incoming, outgoing, shortcut) in [
            ("Ease", true, true, "Easy Ease (F9)"),
            ("Ease In", true, false, "Easy Ease In (Shift+F9)"),
            ("Ease Out", false, true, "Easy Ease Out (Ctrl+Shift+F9)"),
        ] {
            toolbar = toolbar.child(
                ui::text_button(SharedString::from(format!("ease-{label}")), label)
                    .tooltip(move |_, cx| cx.new(|_| ui::Tip(shortcut.into())).into())
                    .when(selected_count == 0, |s| s.opacity(0.4))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ease(incoming, outgoing, window, cx)
                    })),
            );
        }
        toolbar = toolbar.child(
            ui::text_button("graph-snap", "Snap")
                .when(state.snapping, |s| s.bg(rgb(0x34495c)))
                .tooltip(|_, cx| {
                    cx.new(|_| {
                        ui::Tip(
                            "Snap time/value · Ctrl toggles · Alt: bypass key moves, center box scaling"
                                .into(),
                        )
                    })
                    .into()
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        s.snapping = !s.snapping;
                        cx.notify();
                    });
                })),
        );
        toolbar = toolbar
            .child(
                ui::tool(
                    "graph-transform-box",
                    "square",
                    if state.graph_view.speed {
                        "Scale key times or endpoint velocities · Alt: center · Ctrl: toggle snapping"
                    } else {
                        "Transform selected Value Graph keys · Alt: center · Ctrl: toggle snapping"
                    },
                    self.transform_box,
                )
                .when(selected_count < 2 || locked, |s| s.opacity(0.4))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.drag = None;
                    this.transform_box = !this.transform_box;
                    window.focus(&this.focus);
                    cx.notify();
                })),
            )
            .child(
                ui::tool(
                    "graph-auto-height",
                    "chart-line",
                    "Auto Zoom Height · disable to pan/zoom vertically",
                    active_channel.is_none_or(|channel| state.graph_channel_height(channel,state.graph_view.speed).is_none()),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.drag = None;
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        if let Some(channel) = s.graph_active_channel() {
                            let height = if s.graph_channel_height(channel,s.graph_view.speed).is_some() { None }
                                else { s.editor.project().composition().layer(channel.id).and_then(|layer|layer.track(channel.property)).map(|track| {
                                    let view=viewport::current_channel(s,channel,track); [view.low,view.high]
                                }) };
                            s.graph_set_channel_height(channel,s.graph_view.speed,height);
                        }
                        cx.notify();
                    });
                })),
            )
            .child(
                ui::tool(
                    "graph-fit-selection",
                    "target",
                    "Fit Selection · Shift+F",
                    false,
                )
                .when(selected_count == 0, |s| s.opacity(0.4))
                .on_click(cx.listener(|this, _, window, cx| this.fit(true, window, cx))),
            )
            .child(
                ui::tool("graph-fit-all", "square-dashed", "Fit All · F", false)
                    .on_click(cx.listener(|this, _, window, cx| this.fit(false, window, cx))),
            );
        toolbar = toolbar.child(div().flex_1()).child(
            ui::text_button("keyframe-details", "Keyframe...").on_click(cx.listener(
                |this, _, window, cx| {
                    window.focus(&this.focus);
                    this.details = !this.details;
                    cx.notify();
                },
            )),
        );
        let Some(layer) = layer else {
            let included = self.state.read(cx).graph_included_channels();
            let mut body = div()
                .id("inactive-graph-lanes")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col();
            for channel in &included {
                body = body.child(self.lane(*channel, included.len() > 1, cx));
            }
            return root
                .child(self.channel_list(cx))
                .child(body)
                .child(div().p_2().child("Select a channel to edit its keys."))
                .child(toolbar);
        };
        let Some(track) = layer.track(property).cloned() else {
            return root.child(div().p_4().child(if matches!(property, PropertyPath::Text(_)) {
                "Enable this text property's stopwatch or add a key in the timeline to edit its graph."
            } else { "Select a property in the timeline." }));
        };
        let speed_mode = state.graph_view.speed;
        let selection_mode = [
            TemporalMode::Independent,
            TemporalMode::Continuous,
            TemporalMode::Auto,
        ]
        .into_iter()
        .find(|mode| {
            !included_keys.is_empty()
                && included_keys.iter().all(|key| {
                    state
                        .editor
                        .project()
                        .composition()
                        .layer(key.id)
                        .and_then(|layer| layer.track(key.property))
                        .and_then(|track| track.keys().get(&key.frame))
                        .is_some_and(|key| key.temporal.mode == *mode)
                })
        });
        let included_channels = state.graph_included_channels();
        let time_start = state.timeline_start;
        let time_span = state.visible_frames();
        self.lanes
            .borrow_mut()
            .retain(|channel, _| included_channels.contains(channel));
        let channel_list = self.channel_list(cx);
        let clip = self.lane_clip.clone();
        let mut chart = div()
            .id("graph-lanes")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .relative()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(
                canvas(move |bounds, _, _| clip.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            );
        for channel in &included_channels {
            chart = chart.child(self.lane(*channel, included_channels.len() > 1, cx));
        }
        let mut easing = div()
            .id("easing-controls")
            .w(px(280.0))
            .h(px(if selected_count > 1 { 450.0 } else { 420.0 }))
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(0x555555))
            .flex_none()
            .overflow_y_scroll()
            .border_l_1()
            .border_color(rgb(ui::BORDER))
            .px_2()
            .child(div().h(px(20.0)).child("Keyframe timing and velocity"))
            .when(selected_count > 1, |s| {
                s.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(ui::MUTED))
                        .child(format!("{selected_count} keys selected")),
                )
            });
        if let Some((_, frame, _)) = selection {
            let key = &track.keys()[&frame];
            if selected_count > 1 {
                easing = easing.child(div().mt_2().child("All selected keys")).child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(ui::MUTED))
                        .child(if mixed {
                            "Mixed channels: time only · pivot: first selected key"
                        } else {
                            "Time: first key · Value: lowest value"
                        }),
                );
                for (index, label) in [(12, "Offset frames"), (10, "Time %"), (11, "Value %")] {
                    if index == 11 && mixed {
                        continue;
                    }
                    self.sync_input(
                        index,
                        &input_target,
                        if index == 12 { "0" } else { "100" }.into(),
                        window,
                        cx,
                    );
                    easing = easing.child(
                        div()
                            .h(px(25.0))
                            .flex()
                            .items_center()
                            .child(div().w(px(90.0)).child(label))
                            .child(
                                div()
                                    .flex_1()
                                    .when(!locked, |s| s.child(self.fields[index].clone()))
                                    .when(locked, |s| s.child("Locked")),
                            ),
                    );
                }
            }
            for (index, (label, value)) in [
                ("Active frame", frame.to_string()),
                (
                    "Active value",
                    if matches!(property, PropertyPath::Text(_)) {
                        key.value.to_string()
                    } else if property == PropertyPath::TimeRemap {
                        format!("{:.12}", key.value)
                    } else {
                        format!("{:.3}", key.value)
                    },
                ),
            ]
            .into_iter()
            .enumerate()
            {
                self.sync_input(index, &input_target, value, window, cx);
                easing = easing.child(
                    div()
                        .h(px(25.0))
                        .flex()
                        .items_center()
                        .child(div().w(px(90.0)).child(label))
                        .child(
                            div()
                                .flex_1()
                                .when(!locked, |s| s.child(self.fields[index].clone()))
                                .when(locked, |s| s.child("Locked")),
                        ),
                );
            }
            if !matches!(property, PropertyPath::Path(_)) {
                let mut modes = div().flex().mt_1();
                for mode in [
                    TemporalMode::Independent,
                    TemporalMode::Continuous,
                    TemporalMode::Auto,
                ] {
                    let state = self.state.clone();
                    modes = modes.child(
                        ui::text_button(
                            SharedString::from(format!("temporal-{mode:?}")),
                            mode.label(),
                        )
                        .text_size(px(10.0))
                        .px_1()
                        .when(selection_mode == Some(mode), |s| s.bg(rgb(0x34495c)))
                        .when(locked, |s| s.opacity(0.4))
                        .on_click(move |_, window, cx| {
                            state.update(cx, |state, cx| {
                                let result = planning::EditPlan::temporal_mode(
                                    state.editor.project(),
                                    &selection::included(state),
                                    mode,
                                );
                                apply_plan(state, result, window, cx);
                            });
                        }),
                    );
                }
                easing = easing
                    .child(
                        div()
                            .mt_2()
                            .text_size(px(10.0))
                            .child("Selected keys: temporal mode"),
                    )
                    .child(modes)
                    .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
                        match key.temporal.mode {
                            TemporalMode::Auto => {
                                "Auto follows neighbors; edits switch to Continuous."
                            }
                            TemporalMode::Continuous => {
                                "Velocities are linked; influences stay independent."
                            }
                            TemporalMode::Independent => {
                                "Incoming and outgoing velocities are independent."
                            }
                        },
                    ));
                for incoming in [true, false] {
                    let exists = if incoming {
                        track.keys().range(..frame).next_back().is_some()
                    } else {
                        track.keys().range(frame + 1..).next().is_some()
                    };
                    easing =
                        easing.child(div().mt_2().text_color(rgb(ui::MUTED)).child(if incoming {
                            "Incoming"
                        } else {
                            "Outgoing"
                        }));
                    if exists {
                        let handle = track.temporal_handle(frame, incoming);
                        for (offset, label) in
                            ["Velocity /s", "Influence %"].into_iter().enumerate()
                        {
                            let index = if incoming { 6 } else { 8 } + offset;
                            let value = handle.map(|h| {
                                if offset == 0 {
                                    h.slope * fps
                                } else {
                                    h.influence * 100.0
                                }
                            });
                            self.sync_input(
                                index,
                                &input_target,
                                value.map_or_else(|| "—".into(), |v| format!("{v:.6}")),
                                window,
                                cx,
                            );
                            easing = easing.child(
                                div()
                                    .h(px(25.0))
                                    .flex()
                                    .items_center()
                                    .child(div().w(px(85.0)).child(label))
                                    .child(
                                        div()
                                            .flex_1()
                                            .when(!locked, |s| s.child(self.fields[index].clone()))
                                            .when(locked, |s| s.child("Locked")),
                                    ),
                            );
                        }
                    } else {
                        easing = easing.child(
                            div()
                                .text_color(rgb(ui::MUTED))
                                .child("No adjacent segment"),
                        );
                    }
                }
                easing = easing.child(div().mt_2().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child("Velocity is signed; influence is 0.1–100%. For Hold or vertical tangents (—), editing starts from 0 velocity / 33.33%."));
            }
            if let Some(mut curve) = curve {
                if let Some(Drag::Handle { curve: preview, .. }) = &self.drag {
                    curve = *preview;
                }
                let measured = self.easing.clone();
                easing = easing.child(
                    div()
                        .id("bezier-handles")
                        .h(px(110.0))
                        .mx_3()
                        .overflow_hidden()
                        .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_down))
                        .child(
                            canvas(
                                move |b, _, _| measured.set(Some(b)),
                                move |bounds, _, window, _| {
                                    window.with_content_mask(
                                        Some(ContentMask { bounds }),
                                        |window| {
                                            let p = |x, y| EASE_VIEW.point(bounds, x, y);
                                            stroke(
                                                window,
                                                [
                                                    p(0.0, 0.0),
                                                    p(1.0, 0.0),
                                                    p(1.0, 1.0),
                                                    p(0.0, 1.0),
                                                    p(0.0, 0.0),
                                                ],
                                                0x454545,
                                                1.0,
                                            );
                                            stroke(
                                                window,
                                                [p(0.0, 0.0), p(curve.x1, curve.y1)],
                                                ui::BLUE,
                                                1.0,
                                            );
                                            stroke(
                                                window,
                                                [p(1.0, 1.0), p(curve.x2, curve.y2)],
                                                ui::BLUE,
                                                1.0,
                                            );
                                            stroke(
                                                window,
                                                (0..=100).map(|i| {
                                                    let x = i as f64 / 100.0;
                                                    p(x, curve.progress(x))
                                                }),
                                                0xffc66d,
                                                1.5,
                                            );
                                            dot(window, p(curve.x1, curve.y1), ui::BLUE);
                                            dot(window, p(curve.x2, curve.y2), ui::BLUE);
                                        },
                                    );
                                },
                            )
                            .size_full(),
                        ),
                );
                for row in 0..2 {
                    let mut fields = div().flex().h(px(25.0)).gap_1();
                    for column in 0..2 {
                        let index = row * 2 + column;
                        let value = [curve.x1, curve.y1, curve.x2, curve.y2][index];
                        self.sync_input(
                            index + 2,
                            &input_target,
                            format!("{value:.3}"),
                            window,
                            cx,
                        );
                        fields = fields
                            .child(div().w(px(22.0)).child(["X1", "Y1", "X2", "Y2"][index]))
                            .child(
                                div()
                                    .flex_1()
                                    .when(!locked, |s| s.child(self.fields[index + 2].clone())),
                            );
                    }
                    easing = easing.child(fields);
                }
                easing = easing.child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
                    format!(
                        "{} · drag handles or type coordinates",
                        key.interpolation.label()
                    ),
                ));
            } else {
                easing = easing.child(div().py_3().text_color(rgb(ui::MUTED)).child(
                    if track.keys().range(frame + 1..).next().is_none() {
                        "Last key: no outgoing segment."
                    } else {
                        "Use the velocity fields above. Linear or Hold resets the outgoing segment."
                    },
                ));
            }
        } else {
            easing = easing.child(
                div()
                    .py_3()
                    .text_color(rgb(ui::MUTED))
                    .child("Click a graph key to edit its time, value and outgoing curve."),
            );
        }
        easing = easing.child(
            ui::text_button("close-key-details", "Close").on_click(cx.listener(
                |this, _, _, cx| {
                    this.details = false;
                    cx.notify();
                },
            )),
        );
        root.child(channel_list).child(
            div()
                .h(px(23.0))
                .flex_none()
                .px_2()
                .text_color(rgb(ui::MUTED))
                .child(format!("{selected_count} keys selected · {} · Shift/Ctrl-click: toggle · Ctrl+A: all lanes", if mixed { "time-only mixed selection" } else if speed_mode { "Speed Graph" } else { "Value Graph" })),
        )
        .child(div().h(px(20.0)).flex_none().relative().overflow_hidden().bg(rgb(ui::PANEL))
            .children((0..=10).map(|i| div().absolute().left(relative(i as f32/10.0)).top(px(2.0)).text_size(px(10.0)).text_color(rgb(ui::MUTED))
                .child(format!("{}f",time_start as u64 + time_span as u64*i/10)))))
        .child(chart)
        .child(toolbar)
        .when(self.details, |s| {
            s.child(gpui::deferred(
                easing.absolute().right_0().bottom(px(30.0)).occlude(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn text_paint_graph_input_context_rejects_stale_key_property_frame_document_and_selection() {
        use libre_effects_core::{
            Command, Content, KeyRef, Property, PropertyPath, TextParam, TrackEdit,
        };
        for property in [
            PropertyPath::Text(TextParam::FillRed),
            Property::PositionX.into(),
        ] {
            let mut state = super::EditorState::default();
            state
                .editor
                .execute(Command::AddContent {
                    content: Content::Text {
                        text: "Title".into(),
                        font_size: 48.,
                    },
                    width: 400.,
                    height: 100.,
                    name: "Text".into(),
                })
                .unwrap();
            for frame in [0, 60] {
                state
                    .editor
                    .execute(Command::EditTrack {
                        id: 1,
                        property,
                        edit: TrackEdit::ToggleKey { frame },
                    })
                    .unwrap();
            }
            let key = KeyRef {
                id: 1,
                property,
                frame: 0,
            };
            let end = KeyRef { frame: 60, ..key };
            state.graph_property = property;
            state.graph_key = Some(KeyRef {
                id: 1,
                property: state.graph_property,
                frame: 0,
            });
            state.selected_keys = [key].into();
            let target = super::KeyInputTarget::new(&state).unwrap();
            assert!(target.current(&state));
            state.frame = 30;
            assert!(!target.current(&state));
            assert_ne!(
                target.binding(),
                super::KeyInputTarget::new(&state).unwrap().binding()
            );
            state.frame = 0;
            state.selected_keys.insert(end);
            assert!(!target.current(&state));
            assert_ne!(
                target.binding(),
                super::KeyInputTarget::new(&state).unwrap().binding()
            );
            state.graph_key = Some(KeyRef {
                id: 1,
                property: state.graph_property,
                frame: 60,
            });
            assert!(!target.current(&state));
            state.selected_keys = [key].into();
            state.graph_key = Some(KeyRef {
                id: 1,
                property: state.graph_property,
                frame: 0,
            });
            assert!(target.current(&state));
            state.graph_property = Property::Rotation.into();
            assert!(!target.current(&state));
            state.graph_property = property;
            state.document_revision += 1;
            assert!(!target.current(&state));
            state.document_revision -= 1;
            state
                .editor
                .execute(Command::RenameLayer {
                    id: 1,
                    name: "Other".into(),
                })
                .unwrap();
            assert!(!target.current(&state));
            state.editor.undo();
            assert!(target.current(&state));
            state.selected_keys.clear();
            assert!(!target.current(&state));
            assert!(super::KeyInputTarget::new(&state).is_none());
        }
    }

    #[test]
    fn text_paint_graph_units_are_channel_and_mode_specific() {
        for p in libre_effects_core::TextParam::ALL {
            let path = libre_effects_core::PropertyPath::Text(p);
            let expected = match p {
                TextParam::FontSize | TextParam::StrokeWidth => ("px", "px/s"),
                TextParam::Tracking => ("1/1000 em", "(1/1000 em)/s"),
                TextParam::Leading => ("ratio", "ratio/s"),
                TextParam::FillOpacity | TextParam::StrokeOpacity => ("%", "%/s"),
                _ => ("RGB 0–255", "RGB units/s"),
            };
            assert_eq!(super::graph_units(path, false), expected.0);
            assert_eq!(super::graph_units(path, true), expected.1);
        }
        assert_eq!(
            super::graph_units(libre_effects_core::Property::Opacity.into(), true),
            "units/s"
        );
    }

    use super::*;
    #[test]
    fn key_details_require_a_current_selection_after_undo_or_deselect() {
        use libre_effects_core::{KeyRef, Property};
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state
            .editor
            .execute(Command::ToggleKeyframe {
                id: 1,
                property: Property::PositionX,
                frame: 0,
            })
            .unwrap();
        state.graph_property = Property::PositionX.into();
        state.graph_key = Some(KeyRef {
            id: 1,
            property: state.graph_property,
            frame: 0,
        });
        assert!(selected(&state).is_none());
        state.selected_keys.insert(KeyRef {
            id: 1,
            property: state.graph_property,
            frame: 0,
        });
        assert_eq!(selected(&state), Some((1, 0, state.graph_property)));
        state.graph_key = None;
        assert_eq!(selected(&state), Some((1, 0, state.graph_property)));
        state.selected_keys.clear();
        assert!(selected(&state).is_none());
    }
    #[test]
    fn inline_handle_coordinates_support_descending_segments() {
        let bounds = Bounds::new(point(px(400.0), px(100.0)), size(px(600.0), px(200.0)));
        let space = HandleSpace {
            view: View {
                start: 30.0,
                span: 120.0,
                low: -100.0,
                high: 500.0,
            },
            bounds,
            from: 60.0,
            span: 60.0,
            low: 400.0,
            delta: -300.0,
            inline: true,
        };
        let (x, y) = space.value(space.point(0.3, 1.25));
        assert!((x - 0.3).abs() < 1e-5);
        assert!((y - 1.25).abs() < 1e-5);
    }
    #[test]
    fn graph_coordinates_round_trip_with_negative_values_and_zoom() {
        let view = View {
            start: 40.0,
            span: 30.0,
            low: -50.0,
            high: 150.0,
        };
        let bounds = Bounds::new(point(px(20.0), px(30.0)), size(px(600.0), px(200.0)));
        let (f, v) = view.value(bounds, view.point(bounds, 55.0, -20.0));
        assert!((f - 55.0).abs() < 1e-5);
        assert!((v + 20.0).abs() < 1e-5);
    }
    fn lane_fixture() -> (EditorState, [GraphChannel; 2]) {
        use libre_effects_core::Property;
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state.editor.execute(Command::AddRectangle).unwrap();
        let channels = [
            GraphChannel {
                id: 1,
                property: Property::PositionX.into(),
            },
            GraphChannel {
                id: 2,
                property: Property::Opacity.into(),
            },
        ];
        for channel in channels {
            for frame in [10, 20] {
                state
                    .editor
                    .execute(Command::EditTrack {
                        id: channel.id,
                        property: channel.property,
                        edit: TrackEdit::ToggleKey { frame },
                    })
                    .unwrap();
                state
                    .editor
                    .execute(Command::EditTrack {
                        id: channel.id,
                        property: channel.property,
                        edit: TrackEdit::Keyframe {
                            from: frame,
                            to: frame,
                            value: 50.0,
                        },
                    })
                    .unwrap();
            }
            state.graph_pin_channel(channel).unwrap();
        }
        state.graph_activate_channel(channels[0], true);
        state.selected_keys = channels
            .iter()
            .map(|channel| KeyRef {
                id: channel.id,
                property: channel.property,
                frame: 10,
            })
            .collect();
        state.graph_key = state.selected_keys.first().copied();
        (state, channels)
    }
    #[test]
    fn graph_active_key_keeps_identity_when_inspector_targets_another_lane() {
        let (mut state, channels) = lane_fixture();
        state.editor.select(channels[1].id);
        assert_eq!(selected(&state), Some((1, 10, channels[0].property)));
        let target = KeyInputTarget::new(&state).unwrap();
        assert!(target.current(&state));
        state.tool = Tool::Hand;
        assert!(!target.current(&state));
        state.tool = Tool::Select;
        state.graph_open = !state.graph_open;
        assert!(!target.current(&state));
        state.graph_open = !state.graph_open;
        assert!(target.current(&state));
        state.graph_activate_channel(channels[1], true);
        assert_eq!(state.selected_keys.len(), 2);
        assert_eq!(selected(&state), Some((2, 10, channels[1].property)));
        assert!(!target.current(&state));
    }
    #[test]
    fn marquee_uses_each_lane_range_and_keeps_equal_time_keys_distinct() {
        let (state, channels) = lane_fixture();
        let lanes = [
            LaneGeometry {
                channel: channels[0],
                bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(300.0), px(100.0))),
                view: View {
                    start: 0.0,
                    span: 30.0,
                    low: 0.0,
                    high: 100.0,
                },
            },
            LaneGeometry {
                channel: channels[1],
                bounds: Bounds::new(point(px(0.0), px(130.0)), size(px(300.0), px(100.0))),
                view: View {
                    start: 0.0,
                    span: 30.0,
                    low: 0.0,
                    high: 200.0,
                },
            },
        ];
        let selected = marquee_keys(
            &state,
            &lanes,
            point(px(90.0), px(30.0)),
            point(px(110.0), px(220.0)),
            None,
        );
        assert_eq!(selected.len(), 2);
        assert!(mixed_selection(
            &selected.iter().copied().collect::<Vec<_>>()
        ));
        let clipped = marquee_keys(
            &state,
            &lanes,
            point(px(90.0), px(30.0)),
            point(px(110.0), px(220.0)),
            Some(lanes[0].bounds),
        );
        assert_eq!(clipped.len(), 1);
        assert_eq!(clipped.first().unwrap().id, 1);
        assert_eq!(
            channels::all_keys(state.editor.project(), &state.graph_included_channels()).len(),
            4
        );
    }
    #[test]
    fn graph_shortcuts_reject_modifier_leaks_and_busy_editor_contexts() {
        let control = gpui::Modifiers {
            control: true,
            ..Default::default()
        };
        assert!(graph_shortcut("a", control));
        assert!(!graph_shortcut(
            "a",
            gpui::Modifiers {
                shift: true,
                ..control
            }
        ));
        assert!(!graph_shortcut("delete", control));
        assert!(graph_shortcut("delete", Default::default()));
        for key in ["left", "right", "up", "down", "delete", "backspace", "f9"] {
            assert!(graph_owned_key(key, Default::default()));
        }
        for key in ["a", "c", "x", "v", "d", "z"] {
            assert!(graph_owned_key(key, control));
        }

        let (mut state, _) = lane_fixture();
        assert!(graph_editable(&state));
        state.playing = true;
        assert!(!graph_editable(&state));
        state.playing = false;
        state.queue_open = true;
        assert!(!graph_editable(&state));
        state.queue_open = false;
        state.media_open = true;
        assert!(!graph_editable(&state));
    }
    #[test]
    fn ordinary_graph_activation_retains_legacy_view_until_pin() {
        use libre_effects_core::Property;
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        let channel = GraphChannel {
            id: 1,
            property: Property::PositionX.into(),
        };
        assert!(state.graph_activate_property(channel, true));
        assert!(!state.graph_channels.explicit);
        state.graph_pin_channel(channel).unwrap();
        assert!(state.graph_channels.explicit);
    }
    #[test]
    fn numeric_velocity_noop_preserves_linear_precision_but_not_hold_initialization() {
        let (mut state, channels) = lane_fixture();
        let c = channels[0];
        state
            .editor
            .execute(Command::EditTrack {
                id: c.id,
                property: c.property,
                edit: TrackEdit::Keyframe {
                    from: 20,
                    to: 20,
                    value: 100.0,
                },
            })
            .unwrap();
        let track = state
            .editor
            .project()
            .composition()
            .layer(c.id)
            .unwrap()
            .track(c.property)
            .unwrap();
        let mut handle = track.key_velocity_handles(10).unwrap()[1].unwrap();
        handle.slope += f64::EPSILON * handle.slope;
        assert!(numeric_handle_unchanged(track, 10, false, handle));
        assert!(!numeric_handle_unchanged(track, 10, true, handle));
        handle.slope += 0.01;
        assert!(!numeric_handle_unchanged(track, 10, false, handle));
        state
            .editor
            .execute(Command::EditTrack {
                id: c.id,
                property: c.property,
                edit: TrackEdit::Interpolate {
                    frame: 10,
                    interpolation: Interpolation::Hold,
                },
            })
            .unwrap();
        let track = state
            .editor
            .project()
            .composition()
            .layer(c.id)
            .unwrap()
            .track(c.property)
            .unwrap();
        assert!(!numeric_handle_unchanged(
            track,
            10,
            false,
            libre_effects_core::TemporalHandle {
                slope: 0.0,
                influence: 1.0 / 3.0
            }
        ));
    }
    #[test]
    fn normalized_handle_return_preserves_exact_curve_for_off_center_grabs() {
        let space = HandleSpace {
            view: EASE_VIEW,
            bounds: Bounds::new(point(px(20.0), px(40.0)), size(px(251.0), px(129.0))),
            from: 0.0,
            span: 1.0,
            low: 0.0,
            delta: 1.0,
            inline: false,
        };
        let original = Bezier::default();
        let start = space.point(original.x1, original.y1) + point(px(4.0), px(-3.0));
        assert_ne!(
            handle_draft(space, original, 0, start, start + point(px(30.0), px(20.0))),
            original
        );
        assert_eq!(handle_draft(space, original, 0, start, start), original);
        assert_eq!(
            handle_draft(space, original, 0, start, start + point(px(0.1), px(0.1))),
            original
        );
    }
    #[test]
    fn numeric_text_reformatting_does_not_round_full_precision_values() {
        let (state, _) = lane_fixture();
        let mut target = KeyInputTarget::new(&state).unwrap();
        target.displayed_number = Some(33.333333);
        assert!(target.same_number("33.3333330".parse().unwrap()));
        assert!(!target.same_number(33.333334));
        target.displayed_number = None; // Hold or missing side displays an em dash.
        assert!(!target.same_number(0.0));
    }
}
