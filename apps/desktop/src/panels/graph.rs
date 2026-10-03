mod selection;
mod snapping;
mod speed;
mod tangent;
mod transform;
mod viewport;
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
    AnimatedProperty, Bezier, Command, Interpolation, LayerId, PropertyPath, TemporalMode,
    TrackEdit,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy)]
struct View {
    start: f64,
    span: f64,
    low: f64,
    high: f64,
}
impl View {
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
#[derive(Clone)]
enum Drag {
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
        start: Point<Pixels>,
        moved: bool,
        space: HandleSpace,
    },
}
pub(crate) struct Graph {
    state: Entity<EditorState>,
    plot: Rc<Cell<Option<Bounds<Pixels>>>>,
    easing: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
    drag: Option<Drag>,
    drag_revision: u64,
    fields: Vec<Entity<TextField>>,
    details: bool,
    transform_box: bool,
    hand: viewport::TemporaryHand,
    focus_watch: Option<[gpui::Subscription; 2]>,
}
fn selected(state: &EditorState) -> Option<(LayerId, u32, PropertyPath)> {
    let (id, frame) = state
        .graph_key
        .filter(|(id, frame)| {
            state.selected_keys.contains(&libre_effects_core::KeyRef {
                id: *id,
                property: state.graph_property,
                frame: *frame,
            })
        })
        .or_else(|| selection::active(state).first().map(|k| (k.id, k.frame)))?;
    (state.editor.selected() == Some(id)
        && state
            .editor
            .project()
            .composition()
            .layer(id)?
            .track(state.graph_property)?
            .keys()
            .contains_key(&frame))
    .then_some((id, frame, state.graph_property))
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
    let from = selected(state).map(|(_, frame, _)| frame);
    let mut keys = state.selected_keys.clone();
    state.dispatch(&Action::Edit(command), window, cx);
    if state.status.starts_with("Edited") {
        state.selected_layers = [id].into();
        if let Some(frame) = from {
            keys.remove(&libre_effects_core::KeyRef {
                id,
                property: state.graph_property,
                frame,
            });
        }
        keys.insert(libre_effects_core::KeyRef {
            id,
            property: state.graph_property,
            frame: to,
        });
        state.selected_keys = keys;
        state.graph_key = Some((id, to));
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
                    Drag::Zoom { id, property, .. }
                    | Drag::Transform { id, property, .. }
                    | Drag::Pan { id, property, .. }
                    | Drag::Key { id, property, .. }
                    | Drag::Handle { id, property, .. }
                    | Drag::Marquee { id, property, .. }
                    | Drag::Tangent { id, property, .. } => (*id, *property),
                };
                let s = this.state.read(cx);
                s.editor.selected() != Some(id)
                    || s.graph_property != p
                    || s.document_revision != this.drag_revision
            }) {
                this.drag = None;
                this.hand.cancel();
            }
            cx.notify();
        })
        .detach();
        let focus = cx.focus_handle();
        let fields = (0..12)
            .map(|index| {
                let edit = state.clone();
                let return_focus = focus.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |state, cx| {
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
                            if index >= 10 {
                                match selection::scale(state, index == 10, value / 100.0) {
                                    Ok((command, moved)) => {
                                        let old = selection::active(state);
                                        let active =
                                            old.iter().position(|k| k.frame == frame).unwrap_or(0);
                                        state.dispatch(&Action::Edit(command), window, cx);
                                        if state.status.starts_with("Edited") {
                                            state.graph_key = Some((id, moved[active].frame));
                                            state.frame = moved[active].frame;
                                            state.selected_keys = moved.into_iter().collect();
                                            cx.notify();
                                        }
                                    }
                                    Err(error) => {
                                        state.status = error;
                                        cx.notify();
                                    }
                                }
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
                                match index {
                                    2 => curve.x1 = value,
                                    3 => curve.y1 = value,
                                    4 => curve.x2 = value,
                                    _ => curve.y2 = value,
                                };
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
            easing: Rc::new(Cell::new(None)),
            focus,
            drag: None,
            drag_revision: 0,
            fields,
            details: false,
            transform_box: false,
            hand: Default::default(),
            focus_watch: None,
        }
    }
    fn cancel_navigation(&mut self, cx: &mut Context<Self>) {
        self.hand.cancel();
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
    fn scroll(&mut self, event: &gpui::ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.drag.is_some() {
            return;
        }
        let Some(bounds) = self.plot.get() else {
            return;
        };
        let state = self.state.read(cx);
        let Some(track) = state
            .editor
            .selected_layer()
            .and_then(|l| l.track(state.graph_property))
        else {
            return;
        };
        let view = viewport::current(state, track);
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
            } else if s.graph_view.height.is_some() {
                s.graph_view.height = Some(viewport::vertical(
                    view,
                    if event.modifiers.control {
                        dy
                    } else {
                        -dy / height
                    },
                    y,
                    event.modifiers.control,
                ));
            }
            cx.notify();
        });
    }
    fn preset(&self, interpolation: Interpolation, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let commands = selection::active(state)
                .into_iter()
                .map(|k| Command::EditTrack {
                    id: k.id,
                    property: k.property,
                    edit: TrackEdit::Interpolate {
                        frame: k.frame,
                        interpolation,
                    },
                })
                .collect::<Vec<_>>();
            if !commands.is_empty() {
                state.dispatch(&Action::Edit(Command::Batch(commands)), window, cx);
            }
        });
    }
    fn ease(&self, incoming: bool, outgoing: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let keys = selection::active(state);
            if keys.is_empty() {
                return;
            }
            match super::key_easing::selected(state.editor.project(), &keys, incoming, outgoing) {
                Ok(Some(command)) => state.dispatch(&Action::Edit(command), window, cx),
                Ok(None) => {}
                Err(error) => {
                    state.status = error;
                    cx.notify();
                }
            }
        });
    }
    fn down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.drag.is_some() {
            return;
        }
        window.focus(&self.focus);
        let Some(bounds) = self.plot.get() else {
            return;
        };
        let state = self.state.read(cx);
        self.drag_revision = state.document_revision;
        let Some(layer) = state.editor.selected_layer() else {
            return;
        };
        let id = layer.id();
        let property = state.graph_property;
        let Some(track) = layer.track(property) else {
            return;
        };
        let fps = state.editor.project().composition().fps().as_f64();
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
        if self.transform_box
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
        let tangents = if self.transform_box && !speed_mode && selection::active(state).len() > 1 {
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
                    s.graph_key = Some((id, tangent.frame));
                    cx.notify();
                });
                cx.notify();
                return;
            }
        }
        if hit.is_none() && tangents.is_empty() && !layer.locked() && !speed_mode {
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
                            start: event.position,
                            moved: false,
                            space,
                        });
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
            let existing = selection::active(state)
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            let toggle = event.modifiers.shift || event.modifiers.control;
            let remove_on_click = !layer.locked() && toggle && existing.contains(&key);
            let chosen = if remove_on_click {
                existing
            } else {
                selection::clicked(existing, key, toggle)
            };
            let samples = selection::snapshot(
                track,
                &chosen.iter().copied().collect::<Vec<_>>(),
                velocity.map(|(side, _, _)| side),
            );
            if !layer.locked() && chosen.contains(&key) {
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
                    Some((id, frame))
                } else {
                    chosen.first().map(|k| (k.id, k.frame))
                };
                s.selected_keys = chosen;
                s.dispatch(&Action::Seek(frame), window, cx);
            });
        } else {
            self.drag = Some(Drag::Marquee {
                id,
                property,
                start: event.position,
                end: event.position,
                additive: event.modifiers.shift || event.modifiers.control,
                view,
                bounds,
            });
        }
        cx.notify();
    }
    fn handle_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let state = self.state.read(cx);
        self.drag_revision = state.document_revision;
        let Some((id, frame, property)) = selected(state) else {
            return;
        };
        if state.editor.selected_layer().is_none_or(|l| l.locked()) {
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
        match &mut self.drag {
            Some(Drag::Transform { transform, .. }) => {
                transform.moving(
                    event.position,
                    event.modifiers.alt,
                    self.state.read(cx).snapping ^ event.modifiers.control,
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
                start,
                moved,
                space,
                ..
            }) => {
                if !*moved
                    && f32::from(event.position.x - start.x).abs()
                        + f32::from(event.position.y - start.y).abs()
                        < 3.0
                {
                    return;
                }
                *moved = true;
                {
                    let (x, y) = space.value(event.position);
                    let (x, y) = (x.clamp(0.0, 1.0), y.clamp(-2.0, 3.0));
                    if *index == 0 {
                        curve.x1 = x;
                        curve.y1 = y;
                    } else {
                        curve.x2 = x;
                        curve.y2 = y;
                    }
                }
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
                    s.graph_key = selection::active(s).first().map(|k| (k.id, k.frame));
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
                Drag::Transform { mut transform, .. } => {
                    // Include the release position when it is outside the canvas.
                    transform.moving(
                        event.position,
                        event.modifiers.alt,
                        state.snapping ^ event.modifiers.control,
                    );
                    if transform.moved {
                        match transform.command() {
                            Ok((command, moved)) => {
                                let active = selected(state)
                                    .and_then(|(id, frame, _)| {
                                        transform
                                            .keys
                                            .iter()
                                            .position(|k| k.id == id && k.frame == frame)
                                    })
                                    .unwrap_or(0);
                                state.dispatch(&Action::Edit(command), window, cx);
                                if state.status.starts_with("Edited") {
                                    state.graph_key = Some((moved[active].id, moved[active].frame));
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
                Drag::Pan { pan, .. } => {
                    // The last move can fall outside this panel's hit area.
                    // Commit the release position even when no move reached us.
                    pan.apply(state, event.position);
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
                    id,
                    property,
                    start,
                    end,
                    additive,
                    view,
                    bounds,
                } => {
                    let track = state
                        .editor
                        .project()
                        .composition()
                        .layer(id)
                        .and_then(|l| l.track(property));
                    let moved =
                        f32::from(end.x - start.x).abs() + f32::from(end.y - start.y).abs() >= 3.0;
                    let mut keys = if additive {
                        selection::active(state)
                            .into_iter()
                            .collect::<std::collections::BTreeSet<_>>()
                    } else {
                        Default::default()
                    };
                    if moved && let Some(track) = track {
                        let fps = state.editor.project().composition().fps().as_f64();
                        for (&frame, k) in track.keys() {
                            let points = if state.graph_view.speed {
                                speed::ends(track, frame, fps)
                                    .into_iter()
                                    .map(|(side, v)| {
                                        let mut p = view.point(bounds, frame as f64, v);
                                        p.x += px(if side { -5.0 } else { 5.0 });
                                        p
                                    })
                                    .collect::<Vec<_>>()
                            } else {
                                vec![view.point(bounds, frame as f64, k.value)]
                            };
                            if points
                                .into_iter()
                                .any(|p| bounds.contains(&p) && selection::inside(start, end, p))
                            {
                                keys.insert(libre_effects_core::KeyRef {
                                    id,
                                    property,
                                    frame,
                                });
                            }
                        }
                    }
                    state.graph_key = keys.first().map(|k| (k.id, k.frame));
                    state.selected_keys = keys;
                    if !moved && !additive {
                        let (frame, _) = view.value(bounds, end);
                        state.dispatch(&Action::Seek(frame.max(0.0).round() as u32), window, cx);
                    }
                    cx.notify();
                }
                Drag::Key {
                    id,
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
                                state.graph_key = Some((id, to));
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
                } => state.dispatch(
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
                ),
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
        let property = state.graph_property;
        if matches!(property, PropertyPath::Path(_)) {
            return div().id("path-graph-help").size_full().p_4().child("Path geometry is edited in the Composition viewer. Move, copy and ease its keyframes in the timeline.")
                .child(ui::action_tool("path-return-timeline", "pen", "Return to path timeline", &self.state, Action::GraphProperty(state.editor.selected().unwrap_or(0),property), false));
        }
        let layer = state.editor.selected_layer().cloned();
        let start = state.timeline_start;
        let span = state.visible_frames();
        let current = state.frame;
        let fps = state.editor.project().composition().fps().as_f64();
        let selection = selected(state);
        let selected_frames = selection::active(state)
            .into_iter()
            .map(|k| k.frame)
            .collect::<std::collections::BTreeSet<_>>();
        let selected_count = selected_frames.len();
        let scale_identity = format!(
            "scale-{:?}-{property:?}-{selected_frames:?}-{}",
            state.editor.selected(),
            state.document_revision
        );
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
                if key == "a" && event.keystroke.modifiers.control && this.focus.is_focused(window)
                {
                    this.drag = None;
                    this.state.update(cx, |s, cx| {
                        if let Some(layer) = s.editor.selected_layer()
                            && let Some(track) = layer.track(s.graph_property)
                        {
                            s.selected_keys = track
                                .keys()
                                .keys()
                                .map(|&frame| libre_effects_core::KeyRef {
                                    id: layer.id(),
                                    property: s.graph_property,
                                    frame,
                                })
                                .collect();
                            s.graph_key = s.selected_keys.first().map(|k| (k.id, k.frame));
                        }
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
                if matches!(key, "delete" | "backspace") {
                    this.state.update(cx, |state, cx| {
                        let keys = selection::active(state);
                        if !keys.is_empty() {
                            state.dispatch(&Action::Edit(Command::DeleteKeys(keys)), window, cx);
                        }
                    });
                    cx.stop_propagation();
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
            .h(px(30.0))
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
                            s.graph_view.height = None;
                        }
                        cx.notify();
                    });
                })),
            );
        }
        if let Some(layer) = &layer {
            toolbar = toolbar.child(ui::action_tool(
                "graph-add-key",
                "diamond",
                "Add / remove key at current frame",
                &self.state,
                Action::Edit(Command::EditTrack {
                    id: layer.id(),
                    property,
                    edit: TrackEdit::ToggleKey { frame: current },
                }),
                layer
                    .track(property)
                    .is_some_and(|t| t.keys().contains_key(&current)),
            ));
        }
        for (label, interpolation) in [
            ("Linear", Interpolation::Linear),
            ("Hold", Interpolation::Hold),
        ] {
            toolbar = toolbar.child(
                ui::text_button(SharedString::from(format!("preset-{label}")), label)
                    .when(selection.is_none() || locked, |s| s.opacity(0.4))
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
                    .when(selection.is_none() || locked, |s| s.opacity(0.4))
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
                    "Transform selected Value Graph keys · Alt: center · Ctrl: toggle snapping",
                    self.transform_box,
                )
                .when(
                    state.graph_view.speed || selected_count < 2 || locked,
                    |s| s.opacity(0.4),
                )
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
                    state.graph_view.height.is_none(),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.drag = None;
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        if s.graph_view.height.is_some() {
                            s.graph_view.height = None;
                        } else if let Some(track) = s
                            .editor
                            .selected_layer()
                            .and_then(|l| l.track(s.graph_property))
                        {
                            let v = viewport::current(s, track);
                            s.graph_view.height = Some([v.low, v.high]);
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
            return root.child(
                div()
                    .p_4()
                    .child("Select a layer in the timeline to edit its animation."),
            );
        };
        let Some(track) = layer.track(property).cloned() else {
            return root.child(div().p_4().child("Select a property in the timeline."));
        };
        let speed_mode = state.graph_view.speed;
        let mut graph_view = viewport::current(state, &track);
        if let Some(
            Drag::Key { view, .. } | Drag::Marquee { view, .. } | Drag::Tangent { view, .. },
        ) = &self.drag
        {
            graph_view = *view;
        }
        if let Some(Drag::Handle { space, .. }) = &self.drag
            && space.inline
        {
            graph_view = space.view;
        }
        if let Some(Drag::Transform { transform, .. }) = &self.drag {
            graph_view = transform.view;
        }
        let measured = self.plot.clone();
        let drag = self.drag.clone();
        let plot_track = if let Some(Drag::Transform { transform, .. }) = &self.drag {
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
        }) = &self.drag
        {
            track
                .preview_temporal_handle(tangent.frame, tangent.incoming, *handle, *split)
                .unwrap_or_else(|_| track.clone())
        } else {
            track.clone()
        };
        let paint_frames = if let Some(Drag::Transform { transform, .. }) = &self.drag {
            transform.frames()
        } else {
            selected_frames.clone()
        };
        let transform_box = (self.transform_box && !speed_mode && !locked)
            .then(|| transform::SelectionBox::new(&plot_track, &paint_frames))
            .flatten();
        let tangents = if transform_box.is_some() {
            vec![]
        } else {
            tangent::for_selection(&plot_track, &paint_frames)
        };
        let selection_mode = [
            TemporalMode::Independent,
            TemporalMode::Continuous,
            TemporalMode::Auto,
        ]
        .into_iter()
        .find(|mode| {
            selected_count > 0
                && selected_frames
                    .iter()
                    .all(|f| track.keys()[f].temporal.mode == *mode)
        });
        let speed_curves = if speed_mode {
            speed::curves(&plot_track, start, span, fps)
        } else {
            vec![]
        };
        let chart = div().flex_1().min_h_0().min_w_0().flex().flex_col().child(
            div()
                .id("value-graph-canvas")
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
                .on_mouse_down(MouseButton::Left, cx.listener(Self::down))
                .on_mouse_down(MouseButton::Middle, cx.listener(Self::down))
                .on_scroll_wheel(cx.listener(Self::scroll))
                .child(
                    canvas(
                        move |bounds, _, _| measured.set(Some(bounds)),
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
                                            0xffc66d,
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
                                        0xffc66d,
                                        1.5,
                                    );
                                }
                                if !speed_mode
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
                                                    0xffc66d
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
                                            0xffc66d
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
                                if let Some(transform_box) = transform_box {
                                    let invalid = matches!(&drag, Some(Drag::Transform { transform, .. }) if transform.preview.is_err());
                                    transform_box.paint(graph_view,bounds,window,invalid);
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
                                let guides = match &drag {
                                    Some(Drag::Key { guides, .. }) => Some(guides),
                                    Some(Drag::Transform { transform, .. }) => Some(&transform.guides),
                                    _ => None,
                                };
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
                                    if let Some(value) = guides.value {
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
        if let Some((id, frame, _)) = selection {
            let key = &track.keys()[&frame];
            if selected_count > 1 {
                easing = easing
                    .child(div().mt_2().child("Scale selected keys"))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child("Time: first key · Value: lowest value"),
                    );
                for (index, label) in [(10, "Time %"), (11, "Value %")] {
                    self.fields[index].update(cx, |field, _| {
                        field.sync(scale_identity.clone(), "100".into(), window)
                    });
                    easing = easing.child(
                        div()
                            .h(px(25.0))
                            .flex()
                            .items_center()
                            .child(div().w(px(65.0)).child(label))
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
                ("Frame", frame.to_string()),
                (
                    "Value",
                    if property == PropertyPath::TimeRemap {
                        format!("{:.12}", key.value)
                    } else {
                        format!("{:.3}", key.value)
                    },
                ),
            ]
            .into_iter()
            .enumerate()
            {
                self.fields[index].update(cx, |field, _| {
                    field.sync(format!("{id}-{property:?}-{frame}"), value, window)
                });
                easing = easing.child(
                    div()
                        .h(px(25.0))
                        .flex()
                        .items_center()
                        .child(div().w(px(65.0)).child(label))
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
                                let commands = selection::active(state)
                                    .into_iter()
                                    .map(|k| Command::SetTemporalMode {
                                        id: k.id,
                                        property: k.property,
                                        frame: k.frame,
                                        mode,
                                    })
                                    .collect::<Vec<_>>();
                                if !commands.is_empty() {
                                    state.dispatch(
                                        &Action::Edit(Command::Batch(commands)),
                                        window,
                                        cx,
                                    );
                                }
                            });
                        }),
                    );
                }
                easing = easing.child(modes).child(
                    div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
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
                    ),
                );
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
                            self.fields[index].update(cx, |field, _| {
                                field.sync(
                                    format!("{id}-{property:?}-{frame}"),
                                    value.map_or_else(|| "—".into(), |v| format!("{v:.6}")),
                                    window,
                                )
                            });
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
                        self.fields[index + 2].update(cx, |field, _| {
                            field.sync(
                                format!("{id}-{property:?}-{frame}"),
                                format!("{value:.3}"),
                                window,
                            )
                        });
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
        root.child(
            div()
                .h(px(23.0))
                .flex_none()
                .px_2()
                .text_color(rgb(ui::MUTED))
                .child(format!(
                    "{} · {selected_count} selected{}",
                    layer.track_label(property).unwrap_or_default(),
                    if speed_mode {
                        " · units/s · diamonds: velocity/influence · Alt: split · Shift: keep velocity"
                    } else if self.transform_box {
                        " · transform handles: time/value scale · Alt: center · Esc: cancel"
                    } else {
                        " · box select · diamonds: tangents · Alt: split · Shift: keep velocity"
                    }
                )),
        )
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
        state.graph_key = Some((1, 0));
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
}
