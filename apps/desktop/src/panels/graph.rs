use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, ContentMask, Context, Entity, FocusHandle, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathBuilder, Pixels, Point, SharedString, Window, canvas, div, fill, point,
    prelude::*, px, relative, rgb, size,
};
use libre_effects_core::{
    AnimatedProperty, Bezier, Command, Interpolation, LayerId, PropertyPath, TrackEdit,
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
fn view(track: &AnimatedProperty, start: u32, span: u32) -> View {
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
        span: span as f64,
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
    Key {
        id: LayerId,
        property: PropertyPath,
        from: u32,
        to: u32,
        value: f64,
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
    fields: Vec<Entity<TextField>>,
    details: bool,
}
fn selected(state: &EditorState) -> Option<(LayerId, u32, PropertyPath)> {
    let (id, frame) = state.graph_key?;
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
    let (id, frame, property) = selected(state)?;
    let track = state
        .editor
        .project()
        .composition()
        .layer(id)?
        .track(property)?;
    track.keys().range(frame + 1..).next()?;
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
    state.dispatch(&Action::Edit(command), window, cx);
    if state.status.starts_with("Edited") {
        state.selected_layers = [id].into();
        state.selected_keys = [libre_effects_core::KeyRef {
            id,
            property: state.graph_property,
            frame: to,
        }]
        .into();
        state.graph_key = Some((id, to));
        state.frame = to;
        cx.notify();
    }
}
impl Graph {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            if this.drag.as_ref().is_some_and(|drag| {
                let (id, p) = match drag {
                    Drag::Key { id, property, .. } | Drag::Handle { id, property, .. } => {
                        (*id, *property)
                    }
                };
                let s = this.state.read(cx);
                s.editor.selected() != Some(id) || s.graph_property != p
            }) {
                this.drag = None;
            }
            cx.notify();
        })
        .detach();
        let fields = (0..6)
            .map(|index| {
                let edit = state.clone();
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
                            if index < 2 {
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
                })
            })
            .collect();
        Self {
            state,
            plot: Rc::new(Cell::new(None)),
            easing: Rc::new(Cell::new(None)),
            focus: cx.focus_handle(),
            drag: None,
            fields,
            details: false,
        }
    }
    fn preset(&self, interpolation: Interpolation, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            if let Some((id, frame, property)) = selected(state) {
                state.dispatch(
                    &Action::Edit(Command::EditTrack {
                        id,
                        property,
                        edit: TrackEdit::Interpolate {
                            frame,
                            interpolation,
                        },
                    }),
                    window,
                    cx,
                );
            }
        });
    }
    fn down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let Some(bounds) = self.plot.get() else {
            return;
        };
        let state = self.state.read(cx);
        let Some(layer) = state.editor.selected_layer() else {
            return;
        };
        let id = layer.id();
        let property = state.graph_property;
        let Some(track) = layer.track(property) else {
            return;
        };
        let view = view(track, state.timeline_start, state.visible_frames());
        let hit = track
            .keys()
            .iter()
            .filter(|(f, _)| {
                **f >= state.timeline_start && **f <= state.timeline_start + state.visible_frames()
            })
            .find(|(f, k)| {
                let p = view.point(bounds, **f as f64, k.value);
                f32::from(p.x - event.position.x).abs() < 9.0
                    && f32::from(p.y - event.position.y).abs() < 9.0
            })
            .map(|(f, k)| (*f, k.value));
        if hit.is_none() && !layer.locked() {
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
        if let Some((frame, value)) = hit {
            if !layer.locked() {
                self.drag = Some(Drag::Key {
                    id,
                    property,
                    from: frame,
                    to: frame,
                    value,
                    view,
                    bounds,
                    start: event.position,
                    moved: false,
                });
            }
            self.state.update(cx, |s, cx| {
                s.graph_key = Some((id, frame));
                s.selected_keys = std::iter::once(libre_effects_core::KeyRef {
                    id,
                    property,
                    frame,
                })
                .collect();
                s.dispatch(&Action::Seek(frame), window, cx);
            });
        } else {
            let (frame, _) = view.value(bounds, event.position);
            self.state.update(cx, |s, cx| {
                s.graph_key = None;
                s.selected_keys.clear();
                s.dispatch(&Action::Seek(frame.max(0.0).round() as u32), window, cx);
            });
        }
        cx.notify();
    }
    fn handle_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let state = self.state.read(cx);
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
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        match &mut self.drag {
            Some(Drag::Key {
                to,
                value,
                view,
                bounds,
                start,
                moved,
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
                let (f, v) = view.value(*bounds, event.position);
                *to = f.round().clamp(
                    0.0,
                    (self
                        .state
                        .read(cx)
                        .editor
                        .project()
                        .composition()
                        .duration()
                        - 1) as f64,
                ) as u32;
                *value = v;
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
    fn up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag.take() {
            if matches!(
                &drag,
                Drag::Key { moved: false, .. } | Drag::Handle { moved: false, .. }
            ) {
                cx.notify();
                return;
            }
            self.state.update(cx, |state, cx| match drag {
                Drag::Key {
                    id,
                    property,
                    from,
                    to,
                    value,
                    ..
                } => dispatch_key(
                    state,
                    Command::EditTrack {
                        id,
                        property,
                        edit: TrackEdit::Keyframe { from, to, value },
                    },
                    id,
                    to,
                    window,
                    cx,
                ),
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
        let state = self.state.read(cx);
        let property = state.graph_property;
        let layer = state.editor.selected_layer().cloned();
        let start = state.timeline_start;
        let span = state.visible_frames();
        let current = state.frame;
        let selection = selected(state);
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
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if key == "escape" {
                    this.drag = None;
                    this.details = false;
                    cx.stop_propagation();
                    cx.notify();
                }
                if key == "f9" {
                    this.preset(Interpolation::Bezier(Bezier::default()), window, cx);
                    cx.stop_propagation();
                }
                if matches!(key, "delete" | "backspace") {
                    this.state.update(cx, |state, cx| {
                        if let Some((id, frame, property)) = selected(state) {
                            state.dispatch(
                                &Action::Edit(Command::EditTrack {
                                    id,
                                    property,
                                    edit: TrackEdit::ToggleKey { frame },
                                }),
                                window,
                                cx,
                            );
                        }
                    });
                    cx.stop_propagation();
                }
            }));
        let mut toolbar = div()
            .h(px(30.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px_2();
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
            ("Ease (F9)", Interpolation::Bezier(Bezier::default())),
            (
                "Ease In",
                Interpolation::Bezier(Bezier {
                    x1: 0.42,
                    y1: 0.0,
                    x2: 1.0,
                    y2: 1.0,
                }),
            ),
            (
                "Ease Out",
                Interpolation::Bezier(Bezier {
                    x1: 0.0,
                    y1: 0.0,
                    x2: 0.58,
                    y2: 1.0,
                }),
            ),
        ] {
            toolbar = toolbar.child(
                ui::text_button(SharedString::from(format!("preset-{label}")), label)
                    .when(selection.is_none() || locked, |s| s.opacity(0.4))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.preset(interpolation, window, cx)
                    })),
            );
        }
        toolbar = toolbar.child(div().flex_1()).child(
            ui::text_button("keyframe-details", "Keyframe...").on_click(cx.listener(
                |this, _, _, cx| {
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
        let mut graph_view = view(&track, start, span);
        if let Some(Drag::Key { view, .. }) = &self.drag {
            graph_view = *view;
        }
        if let Some(Drag::Handle { space, .. }) = &self.drag
            && space.inline
        {
            graph_view = space.view;
        }
        let measured = self.plot.clone();
        let drag = self.drag.clone();
        let plot_track = track.clone();
        let chart = div().flex_1().min_h_0().min_w_0().flex().flex_col().child(
            div()
                .id("value-graph-canvas")
                .flex_1()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .bg(rgb(0x262626))
                .cursor_crosshair()
                .on_mouse_down(MouseButton::Left, cx.listener(Self::down))
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
                                stroke(
                                    window,
                                    (0..=600).map(|i| {
                                        let f =
                                            graph_view.start + graph_view.span * i as f64 / 600.0;
                                        graph_view.point(bounds, f, evaluate(f))
                                    }),
                                    0xffc66d,
                                    1.5,
                                );
                                if let Some((_, frame, _)) = selection
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
                                for (&f, k) in plot_track.keys() {
                                    dot(
                                        window,
                                        graph_view.point(bounds, f as f64, k.value),
                                        if selection.is_some_and(|(_, frame, _)| frame == f) {
                                            ui::BLUE
                                        } else {
                                            0xffc66d
                                        },
                                    );
                                }
                                if let Some(Drag::Key { to, value, .. }) = &drag {
                                    dot(
                                        window,
                                        graph_view.point(bounds, *to as f64, *value),
                                        0xffffff,
                                    );
                                }
                                let x = graph_view.point(bounds, current as f64, 0.0).x;
                                stroke(
                                    window,
                                    [point(x, bounds.top()), point(x, bounds.bottom())],
                                    ui::BLUE,
                                    1.0,
                                );
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
            .h(px(290.0))
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(0x555555))
            .flex_none()
            .overflow_y_scroll()
            .border_l_1()
            .border_color(rgb(ui::BORDER))
            .px_2()
            .child(
                div()
                    .h(px(20.0))
                    .child("Outgoing segment · temporal Bezier"),
            );
        if let Some((id, frame, _)) = selection {
            let key = &track.keys()[&frame];
            for (index, (label, value)) in [
                ("Frame", frame.to_string()),
                ("Value", format!("{:.3}", key.value)),
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
                        "Hold segment: choose an easing preset to enable handles."
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
                .child(layer.track_label(property).unwrap_or_default()),
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
