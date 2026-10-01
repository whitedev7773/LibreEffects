use crate::{
    components::TextField,
    editor::{Action, EditorState, PropertyFilter, timecode},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels,
    SharedString, Window, canvas, div, fill, point, prelude::*, px, relative, rgb, size,
};
use libre_effects_core::{Command, KeyRef, LayerId, LayerSwitch, Property};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::{cell::Cell, rc::Rc};

const LEFT: f32 = 560.0;
#[derive(Clone)]
struct KeyDrag {
    from: u32,
    to: u32,
}
pub(crate) struct Timeline {
    left: f32,
    resizing: bool,
    fields: BTreeMap<(LayerId, Property), Entity<TextField>>,
    parent_open: Option<LayerId>,
    bar_drag: Option<(Vec<LayerId>, i32, u32, i64)>,
    marquee: Option<(gpui::Point<Pixels>, gpui::Point<Pixels>)>,
    marquee_additive: bool,
    hit_keys: Rc<RefCell<Vec<(KeyRef, Bounds<Pixels>)>>>,
    hit_layers: Rc<RefCell<Vec<(LayerId, Bounds<Pixels>)>>>,
    search: Entity<TextField>,
    graph: Entity<super::graph::Graph>,
    state: Entity<EditorState>,
    ruler: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
    scrubbing: bool,
    drag: Option<KeyDrag>,
    selected_key: Option<(LayerId, Property, u32)>,
}
fn frame_at(x: f32, left: f32, width: f32, start: u32, visible: u32, duration: u32) -> u32 {
    (start + (((x - left) / width.max(1.0)).clamp(0.0, 1.0) * visible as f32).round() as u32)
        .min(duration - 1)
}
impl Timeline {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let search = cx.new(|cx| TextField::new(cx, |_, _, _| {}));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            left: LEFT,
            resizing: false,
            fields: BTreeMap::new(),
            parent_open: None,
            bar_drag: None,
            marquee: None,
            marquee_additive: false,
            hit_keys: Default::default(),
            hit_layers: Default::default(),
            search,
            graph: cx.new(|cx| super::graph::Graph::new(state.clone(), cx)),
            state,
            ruler: Rc::new(Cell::new(None)),
            focus: cx.focus_handle(),
            scrubbing: false,
            drag: None,
            selected_key: None,
        }
    }
    fn seek_x(&self, x: Pixels, cx: &Context<Self>) -> Option<u32> {
        let bounds = self.ruler.get()?;
        let state = self.state.read(cx);
        Some(frame_at(
            f32::from(x),
            f32::from(bounds.left()),
            f32::from(bounds.size.width),
            state.timeline_start,
            state.visible_frames(),
            state.editor.project().composition().duration(),
        ))
    }
    fn moving(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if self.resizing {
            self.left = f32::from(event.position.x).clamp(380.0, 800.0);
            cx.notify();
            return;
        }
        if let Some((_, end)) = &mut self.marquee {
            *end = event.position;
            cx.notify();
            return;
        }
        let Some(frame) = self.seek_x(event.position.x, cx) else {
            return;
        };
        if let Some((_, _, origin, delta)) = &mut self.bar_drag {
            *delta = frame as i64 - *origin as i64;
            cx.notify();
        }
        if self.scrubbing {
            self.state.update(cx, |state, cx| {
                state.dispatch(&Action::Seek(frame), window, cx)
            });
        }
        if let Some(drag) = &mut self.drag {
            drag.to = frame;
            cx.notify();
        }
    }
    fn up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.scrubbing = false;
        self.resizing = false;
        if let Some((a, b)) = self.marquee.take() {
            let area = Bounds::from_corners(
                point(a.x.min(b.x), a.y.min(b.y)),
                point(a.x.max(b.x), a.y.max(b.y)),
            );
            let intersects = |bounds: Bounds<Pixels>| {
                bounds.right() >= area.left()
                    && bounds.left() <= area.right()
                    && bounds.bottom() >= area.top()
                    && bounds.top() <= area.bottom()
            };
            let keys: Vec<_> = self
                .hit_keys
                .borrow()
                .iter()
                .filter(|(_, b)| intersects(*b))
                .map(|(k, _)| *k)
                .collect();
            let layers: Vec<_> = self
                .hit_layers
                .borrow()
                .iter()
                .filter(|(_, b)| intersects(*b))
                .map(|(id, _)| *id)
                .collect();
            self.state.update(cx, |s, cx| {
                if !self.marquee_additive {
                    s.selected_keys.clear();
                }
                if keys.is_empty() && !self.marquee_additive {
                    s.selected_layers.clear();
                }
                s.selected_layers.extend(keys.iter().map(|k| k.id));
                s.selected_keys.extend(keys);
                s.selected_layers.extend(layers);
                if let Some(id) = s
                    .selected_layers
                    .first()
                    .copied()
                    .or_else(|| s.selected_keys.first().map(|k| k.id))
                {
                    s.editor.select(id);
                } else {
                    s.editor.clear_selection();
                }
                cx.notify();
            });
        }
        if let Some((ids, edge, _, delta)) = self.bar_drag.take()
            && delta != 0
        {
            self.state.update(cx, |s, cx| {
                let comp = s.editor.project().composition();
                let commands = ids
                    .iter()
                    .filter_map(|id| comp.layer(*id))
                    .map(|l| {
                        if edge == 0 {
                            Command::ShiftLayer { id: l.id(), delta }
                        } else {
                            Command::SetLayerRange {
                                id: l.id(),
                                start: if edge < 0 {
                                    (l.in_frame() as i64 + delta)
                                        .clamp(0, comp.duration() as i64 - 1)
                                        as u32
                                } else {
                                    l.in_frame()
                                },
                                end: if edge > 0 {
                                    (l.out_frame(comp.duration()) as i64 + delta)
                                        .clamp(1, comp.duration() as i64)
                                        as u32
                                } else {
                                    l.out_frame(comp.duration())
                                },
                            }
                        }
                    })
                    .collect();
                s.dispatch(&Action::Edit(Command::Batch(commands)), window, cx);
            });
        }
        if let Some(drag) = self.drag.take()
            && drag.to != drag.from
        {
            self.state.update(cx, |s, cx| {
                let keys: Vec<_> = s.selected_keys.iter().copied().collect();
                let delta = drag.to as i64 - drag.from as i64;
                s.dispatch(
                    &Action::Edit(Command::MoveKeys {
                        keys: keys.clone(),
                        delta,
                    }),
                    window,
                    cx,
                );
                if s.status.starts_with("Edited") {
                    s.selected_keys = keys
                        .into_iter()
                        .map(|mut k| {
                            k.frame = (k.frame as i64 + delta) as u32;
                            k
                        })
                        .collect();
                }
            });
        }
        cx.notify();
    }
}
fn grid(start: u32, visible: u32, frame: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for tick in 0..=10 {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px(f32::from(bounds.size.width) * tick as f32 / 10.0),
                            bounds.top(),
                        ),
                        size(px(1.0), bounds.size.height),
                    ),
                    rgb(0x2b2b2b),
                ));
            }
            if frame >= start && frame <= start + visible {
                let x = (frame - start) as f32 / visible as f32;
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px(f32::from(bounds.size.width) * x),
                            bounds.top(),
                        ),
                        size(px(1.0), bounds.size.height),
                    ),
                    rgb(ui::BLUE),
                ));
            }
        },
    )
    .absolute()
    .size_full()
}
impl Render for Timeline {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.hit_keys.borrow_mut().clear();
        self.hit_layers.borrow_mut().clear();
        let left = self.left;
        let state = self.state.read(cx);
        let selected_layers = state.selected_layers.clone();
        let selected_keys = state.selected_keys.clone();
        let comp = state.editor.project().composition().clone();
        let graph_open = state.graph_open;
        let graph_property = state.graph_property;
        let frame = state.frame;
        let start = state.timeline_start;
        let visible = state.visible_frames();
        let selected = state.editor.selected();
        // Changing layers must not leave Delete targeting a key on the old layer.
        if self
            .selected_key
            .is_some_and(|(id, _, _)| selected != Some(id))
        {
            self.selected_key = None;
            self.drag = None;
        }
        let expanded = state.expanded;
        let filter = state.property_filter;
        let zoom = state.timeline_zoom;
        let work_start = state.work_start;
        let work_end = state.work_end;
        let bounds = self.ruler.clone();
        let mut rows = div().flex().flex_col().w_full();
        let query = self.search.read(cx).value().trim().to_lowercase();
        for (index, layer) in comp.layers().iter().enumerate() {
            if !layer.name().to_lowercase().contains(&query) || (comp.hide_shy() && layer.shy()) {
                continue;
            }
            let id = layer.id();
            let selected_row = selected_layers.contains(&id);
            let control_id = |suffix: &str| SharedString::from(format!("layer-{id}-{suffix}"));
            let mut controls = div()
                .flex()
                .items_center()
                .h_full()
                .w(px(left))
                .flex_none()
                .overflow_hidden()
                .child(ui::action_tool(
                    control_id("visible"),
                    if layer.visible() { "eye" } else { "eye-slash" },
                    "Toggle layer visibility",
                    &self.state,
                    Action::Edit(Command::ToggleVisible(id)),
                    false,
                ))
                .child(ui::action_tool(
                    control_id("lock"),
                    if layer.locked() { "lock" } else { "lock-open" },
                    "Toggle layer lock",
                    &self.state,
                    Action::Edit(Command::ToggleLocked(id)),
                    layer.locked(),
                ))
                .children(
                    [
                        (
                            "solo",
                            "target",
                            "Solo: isolate this layer in preview and output",
                            LayerSwitch::Solo,
                            layer.solo(),
                        ),
                        (
                            "shy",
                            "eye-slash",
                            "Shy: hide this row when Hide Shy is enabled",
                            LayerSwitch::Shy,
                            layer.shy(),
                        ),
                        (
                            "guide",
                            "square-dashed",
                            "Guide: preview only, excluded from output and nesting",
                            LayerSwitch::Guide,
                            layer.guide(),
                        ),
                    ]
                    .into_iter()
                    .map(|(key, icon, label, switch, enabled)| {
                        ui::action_tool(
                            control_id(key),
                            icon,
                            label,
                            &self.state,
                            Action::Edit(Command::SetLayerSwitch {
                                id,
                                switch,
                                enabled: !enabled,
                            }),
                            enabled,
                        )
                        .w(px(22.0))
                        .h(px(22.0))
                    }),
                )
                .child(
                    ui::tool(
                        control_id("expand"),
                        if selected_row && expanded {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        "Reveal transform properties",
                        false,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_key = None;
                        this.state.update(cx, |state, cx| {
                            if state.editor.selected() == Some(id) {
                                state.dispatch(&Action::ToggleExpanded, window, cx);
                            } else {
                                state.dispatch(&Action::Select(id), window, cx);
                                state.expanded = true;
                            }
                        });
                    })),
                )
                .child(div().w(px(8.0)).h(px(14.0)).mr_2().bg(rgb(layer.color())))
                .child(
                    div()
                        .w(px(20.0))
                        .text_size(px(10.0))
                        .text_color(rgb(ui::MUTED))
                        .child((index + 1).to_string()),
                )
                .child(
                    ui::text_button(control_id("name"), layer.name().to_string())
                        .flex_1()
                        .min_w_0()
                        .justify_start()
                        .overflow_hidden()
                        .on_click(cx.listener(
                            move |this, event: &gpui::ClickEvent, window, cx| {
                                window.focus(&this.focus);
                                this.selected_key = None;
                                this.state.update(cx, |state, cx| {
                                    state.dispatch(
                                        &Action::SelectMany(
                                            id,
                                            event.modifiers().control,
                                            event.modifiers().shift,
                                        ),
                                        window,
                                        cx,
                                    )
                                });
                            },
                        )),
                );
            let parent_name = layer
                .parent()
                .and_then(|id| comp.layer(id))
                .map_or("None".to_string(), |l| format!("{} · {}", l.id(), l.name()));
            controls = controls.child(
                div()
                    .relative()
                    .w(px(135.0))
                    .flex_none()
                    .child(
                        ui::text_button(control_id("parent"), format!("{parent_name} ▾"))
                            .w_full()
                            .overflow_hidden()
                            .justify_start()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.parent_open = if this.parent_open == Some(id) {
                                    None
                                } else {
                                    Some(id)
                                };
                                cx.notify();
                            })),
                    )
                    .when(self.parent_open == Some(id), |s| {
                        let choices = std::iter::once((None, "None".to_string())).chain(
                            comp.layers()
                                .iter()
                                .filter(|l| comp.can_parent(id, Some(l.id())))
                                .map(|l| (Some(l.id()), l.name().to_string())),
                        );
                        let mut menu = div()
                            .id(control_id("parent-menu"))
                            .w(px(190.0))
                            .max_h(px(180.0))
                            .overflow_y_scroll()
                            .bg(rgb(ui::PANEL))
                            .border_1()
                            .border_color(rgb(ui::BLUE));
                        for (parent, label) in choices {
                            menu = menu.child(
                                ui::text_button(
                                    gpui::SharedString::from(format!("parent-{id}-{parent:?}")),
                                    label,
                                )
                                .w_full()
                                .justify_start()
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.parent_open = None;
                                        this.state.update(cx, |s, cx| {
                                            s.dispatch(
                                                &Action::Edit(Command::SetParent {
                                                    id,
                                                    parent,
                                                    frame,
                                                }),
                                                window,
                                                cx,
                                            )
                                        });
                                    },
                                )),
                            );
                        }
                        s.child(
                            gpui::deferred(menu.absolute().top(px(22.0)).left_0().occlude())
                                .with_priority(2),
                        )
                    }),
            );
            controls = controls
                .child(ui::action_tool(
                    control_id("up"),
                    "arrow-up",
                    "Move layer up",
                    &self.state,
                    Action::Edit(Command::MoveLayer {
                        id,
                        index: index.saturating_sub(1),
                    }),
                    false,
                ))
                .child(ui::action_tool(
                    control_id("down"),
                    "arrow-down",
                    "Move layer down",
                    &self.state,
                    Action::Edit(Command::MoveLayer {
                        id,
                        index: (index + 1).min(comp.layers().len() - 1),
                    }),
                    false,
                ));
            let bar_offset = self
                .bar_drag
                .as_ref()
                .filter(|(ids, _, _, _)| ids.contains(&id));
            let bar_start = bar_offset
                .map_or(layer.in_frame() as i64, |(_, edge, _, d)| {
                    layer.in_frame() as i64 + if *edge <= 0 { *d } else { 0 }
                })
                .max(0) as u32;
            let bar_end = bar_offset
                .map_or(
                    layer.out_frame(comp.duration()) as i64,
                    |(_, edge, _, d)| {
                        layer.out_frame(comp.duration()) as i64 + if *edge >= 0 { *d } else { 0 }
                    },
                )
                .max(0) as u32;
            let bar_left =
                (bar_start.saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
            let right = (bar_end.saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
            let bar_visible =
                layer.out_frame(comp.duration()) > start && layer.in_frame() < start + visible;
            let layer_hits = self.hit_layers.clone();
            let bar_bounds = Rc::new(Cell::new(None));
            let bar_measure = bar_bounds.clone();
            let time_area = div()
                .relative()
                .flex_1()
                .h_full()
                .overflow_hidden()
                .child(grid(start, visible, frame))
                .child(
                    canvas(
                        move |b, _, _| layer_hits.borrow_mut().push((id, b)),
                        |_, _, _, _| (),
                    )
                    .absolute()
                    .size_full(),
                )
                .when(bar_visible, |area| {
                    area.child(
                        div()
                            .id(control_id("bar"))
                            .absolute()
                            .left(relative(bar_left))
                            .w(relative((right - bar_left).max(0.0)))
                            .top(px(3.0))
                            .h(px(17.0))
                            .bg(rgb(layer.color()))
                            .opacity(if layer.visible() { 0.85 } else { 0.25 })
                            .border_1()
                            .border_color(rgb(if selected_row { 0xddd2ff } else { 0x777777 }))
                            .cursor_grab()
                            .child(
                                canvas(move |b, _, _| bar_measure.set(Some(b)), |_, _, _, _| ())
                                    .absolute()
                                    .size_full(),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                        window.focus(&this.focus);
                                        cx.stop_propagation();
                                        let Some(at) = this.seek_x(event.position.x, cx) else {
                                            return;
                                        };
                                        let edge =
                                            bar_bounds.get().map_or(0, |b: Bounds<Pixels>| {
                                                if event.position.x - b.left() < px(7.0) {
                                                    -1
                                                } else if b.right() - event.position.x < px(7.0) {
                                                    1
                                                } else {
                                                    0
                                                }
                                            });
                                        this.state.update(cx, |s, cx| {
                                            if !s.selected_layers.contains(&id) {
                                                s.dispatch(
                                                    &Action::SelectMany(
                                                        id,
                                                        event.modifiers.control,
                                                        event.modifiers.shift,
                                                    ),
                                                    window,
                                                    cx,
                                                );
                                            }
                                            s.selected_keys.clear();
                                        });
                                        this.bar_drag = Some((
                                            this.state
                                                .read(cx)
                                                .selected_layers
                                                .iter()
                                                .copied()
                                                .collect(),
                                            edge,
                                            at,
                                            0,
                                        ));
                                        cx.notify();
                                    },
                                ),
                            ),
                    )
                });
            rows = rows.child(
                div()
                    .flex()
                    .h(px(23.0))
                    .flex_none()
                    .border_b_1()
                    .border_color(rgb(0x151515))
                    .bg(rgb(if selected_row { 0x343434 } else { ui::BG }))
                    .child(controls)
                    .when(!graph_open, |s| s.child(time_area)),
            );
            if selected_row && expanded {
                rows = rows.child(
                    div()
                        .flex()
                        .h(px(24.0))
                        .flex_none()
                        .child(
                            div()
                                .w(px(left))
                                .flex_none()
                                .pl(px(128.0))
                                .flex()
                                .gap_2()
                                .items_center()
                                .text_color(rgb(ui::MUTED))
                                .child(ui::icon("chevron-down"))
                                .child("Transform"),
                        )
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .h_full()
                                .child(grid(start, visible, frame)),
                        ),
                );
                for (label, properties) in [
                    ("Anchor Point", vec![Property::AnchorX, Property::AnchorY]),
                    ("Position", vec![Property::PositionX, Property::PositionY]),
                    ("Scale", vec![Property::ScaleX, Property::ScaleY]),
                    ("Rotation", vec![Property::Rotation]),
                    ("Opacity", vec![Property::Opacity]),
                ] {
                    if filter.is_some_and(|f| {
                        !properties.iter().any(|p| {
                            f.includes(*p)
                                && (f != PropertyFilter::Animated
                                    || !layer.property(*p).keys().is_empty())
                        })
                    }) {
                        continue;
                    }
                    let prop_id =
                        |suffix: &str| SharedString::from(format!("prop-{id}-{label}-{suffix}"));
                    let animated = properties
                        .iter()
                        .any(|p| !layer.property(*p).keys().is_empty());
                    let watch = Command::Batch(
                        properties
                            .iter()
                            .filter(|p| layer.property(**p).keys().is_empty() == !animated)
                            .map(|p| Command::ToggleAnimation {
                                id,
                                property: *p,
                                frame,
                            })
                            .collect(),
                    );
                    let channel = properties[0];
                    let channel_state = self.state.clone();
                    let mut controls = div()
                        .flex()
                        .items_center()
                        .w(px(left))
                        .flex_none()
                        .pl(px(128.0))
                        .child(ui::action_tool(
                            prop_id("watch"),
                            "stopwatch",
                            "Toggle animation",
                            &self.state,
                            Action::Edit(watch),
                            animated,
                        ))
                        .child(
                            ui::text_button(prop_id("label"), label)
                                .flex_1()
                                .justify_start()
                                .text_size(px(11.0))
                                .when(graph_open && properties.contains(&graph_property), |s| {
                                    s.text_color(rgb(ui::BLUE))
                                })
                                .on_click(move |_, _, cx| {
                                    channel_state.update(cx, |s, cx| {
                                        s.editor.select(id);
                                        s.graph_property = channel;
                                        s.graph_key = None;
                                        cx.notify();
                                    });
                                }),
                        );
                    for property in properties.iter().copied() {
                        let input = self
                            .fields
                            .entry((id, property))
                            .or_insert_with(|| {
                                let edit = self.state.clone();
                                cx.new(|cx| {
                                    TextField::new(cx, move |text, window, cx| {
                                        edit.update(cx, |s, cx| {
                                            if let Ok(value) = text.parse::<f64>() {
                                                s.dispatch(
                                                    &Action::Edit(Command::SetValue {
                                                        id,
                                                        property,
                                                        frame: s.frame,
                                                        value,
                                                    }),
                                                    window,
                                                    cx,
                                                );
                                            } else {
                                                s.status = "Enter a finite number".into();
                                                cx.notify();
                                            }
                                        })
                                    })
                                    .numeric()
                                })
                            })
                            .clone();
                        input.update(cx, |field, _| {
                            field.sync(
                                format!("{id}-{frame}"),
                                format!("{:.2}", layer.property(property).value_at(frame)),
                                window,
                            )
                        });
                        controls = controls
                            .child(
                                ui::text_button(
                                    SharedString::from(format!("channel-{id}-{property:?}")),
                                    if properties.len() == 2 {
                                        if matches!(
                                            property,
                                            Property::PositionX
                                                | Property::AnchorX
                                                | Property::ScaleX
                                        ) {
                                            "X"
                                        } else {
                                            "Y"
                                        }
                                    } else {
                                        if property == Property::Opacity {
                                            "%"
                                        } else {
                                            "°"
                                        }
                                    },
                                )
                                .w(px(19.0))
                                .when(graph_open && graph_property == property, |s| {
                                    s.text_color(rgb(ui::BLUE))
                                })
                                .on_click({
                                    let state = self.state.clone();
                                    move |_, _, cx| {
                                        state.update(cx, |s, cx| {
                                            s.editor.select(id);
                                            s.graph_property = property;
                                            s.graph_key = None;
                                            cx.notify();
                                        })
                                    }
                                }),
                            )
                            .child(
                                div()
                                    .w(px(60.0))
                                    .when(!layer.locked(), |s| s.child(input))
                                    .when(layer.locked(), |s| {
                                        s.child(format!(
                                            "{:.1}",
                                            layer.property(property).value_at(frame)
                                        ))
                                    }),
                            );
                    }
                    let at_frame: Vec<_> = properties
                        .iter()
                        .filter(|p| layer.property(**p).keys().contains_key(&frame))
                        .copied()
                        .collect();
                    let toggle = if at_frame.is_empty() {
                        properties.clone()
                    } else {
                        at_frame.clone()
                    };
                    controls = controls.child(ui::action_tool(
                        prop_id("key"),
                        "diamond",
                        "Add / remove keys",
                        &self.state,
                        Action::Edit(Command::Batch(
                            toggle
                                .into_iter()
                                .map(|property| Command::ToggleKeyframe {
                                    id,
                                    property,
                                    frame,
                                })
                                .collect(),
                        )),
                        !at_frame.is_empty(),
                    ));
                    let mut keys = div()
                        .relative()
                        .flex_1()
                        .h_full()
                        .overflow_hidden()
                        .child(grid(start, visible, frame));
                    let frames: BTreeSet<_> = properties
                        .iter()
                        .flat_map(|p| layer.property(*p).keys().keys().copied())
                        .collect();
                    for key_frame in frames {
                        let key_refs: Vec<_> = properties
                            .iter()
                            .filter(|p| layer.property(**p).keys().contains_key(&key_frame))
                            .map(|p| KeyRef {
                                id,
                                property: *p,
                                frame: key_frame,
                            })
                            .collect();
                        let active = key_refs.iter().any(|k| selected_keys.contains(k));
                        let offset = self
                            .drag
                            .as_ref()
                            .filter(|_| active)
                            .map_or(0, |d| d.to as i64 - d.from as i64);
                        let display = (key_frame as i64 + offset).max(0) as u32;
                        if display < start || display > start + visible {
                            continue;
                        }
                        let hits = self.hit_keys.clone();
                        let measured = key_refs.clone();
                        let property = key_refs[0].property;
                        keys = keys.child(
                            div()
                                .id(SharedString::from(format!("key-{id}-{label}-{key_frame}")))
                                .absolute()
                                .left(relative((display - start) as f32 / visible as f32))
                                .ml(px(-6.0))
                                .top(px(4.0))
                                .size(px(13.0))
                                .cursor_pointer()
                                .child(ui::icon("diamond").text_color(rgb(if active {
                                    ui::BLUE
                                } else {
                                    0xc8c8c8
                                })))
                                .child(
                                    canvas(
                                        move |b, _, _| {
                                            for k in &measured {
                                                hits.borrow_mut().push((*k, b));
                                            }
                                        },
                                        |_, _, _, _| (),
                                    )
                                    .absolute()
                                    .size_full(),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            window.focus(&this.focus);
                                            cx.stop_propagation();
                                            this.scrubbing = false;
                                            this.state.update(cx, |s, cx| {
                                                if event.modifiers.control || event.modifiers.shift
                                                {
                                                    for k in &key_refs {
                                                        if !s.selected_keys.remove(k) {
                                                            s.selected_keys.insert(*k);
                                                        }
                                                    }
                                                } else if !key_refs
                                                    .iter()
                                                    .any(|k| s.selected_keys.contains(k))
                                                {
                                                    s.selected_keys =
                                                        key_refs.iter().copied().collect();
                                                }
                                                s.graph_property = property;
                                                s.graph_key = Some((id, key_frame));
                                                s.dispatch(&Action::Seek(key_frame), window, cx);
                                            });
                                            this.drag = Some(KeyDrag {
                                                from: key_frame,
                                                to: key_frame,
                                            });
                                            cx.notify();
                                        },
                                    ),
                                ),
                        );
                    }
                    rows = rows.child(
                        div()
                            .flex()
                            .h(px(25.0))
                            .flex_none()
                            .child(controls)
                            .when(!graph_open, |s| s.child(keys)),
                    );
                }
            }
        }
        if comp.layers().is_empty() {
            rows = rows.child(
                div()
                    .h(px(90.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(ui::MUTED))
                    .child("No layers. Create a rectangle with Ctrl+Y."),
            );
        }
        let work_left = (work_start.saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
        let work_right = (work_end.saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
        div()
            .id("timeline")
            .relative()
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(rgb(ui::BG))
            .on_mouse_move(cx.listener(Self::moving))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::up))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.drag = None;
                    this.bar_drag = None;
                    this.marquee = None;
                    this.resizing = false;
                    this.scrubbing = false;
                    this.parent_open = None;
                    cx.notify();
                }
                if event.keystroke.key == "a" && event.keystroke.modifiers.control {
                    let keys = this.hit_keys.borrow().iter().map(|(k, _)| *k).collect();
                    this.state.update(cx, |s, cx| {
                        if s.selected_keys.is_empty() {
                            s.selected_layers = s
                                .editor
                                .project()
                                .composition()
                                .layers()
                                .iter()
                                .filter(|l| {
                                    !(s.editor.project().composition().hide_shy() && l.shy())
                                })
                                .map(|l| l.id())
                                .collect();
                            if let Some(id) = s.selected_layers.first() {
                                s.editor.select(*id);
                            } else {
                                s.editor.clear_selection();
                            }
                        } else {
                            s.selected_keys = keys;
                        }
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
                if event.keystroke.key == "delete" {
                    this.state
                        .update(cx, |s, cx| s.dispatch(&Action::DeleteSelection, window, cx));
                    cx.stop_propagation();
                }
            }))
            .child(ui::panel_header(comp.name().to_string()))
            .child(
                div()
                    .flex()
                    .h(px(32.0))
                    .flex_none()
                    .items_center()
                    .px_3()
                    .gap_2()
                    .child(
                        div()
                            .w(px(105.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(ui::BLUE))
                                    .child(timecode(frame, comp.fps())),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(ui::MUTED))
                                    .child(format!("{frame:05}  ({} fps)", comp.fps())),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .w(px(165.0))
                            .child(ui::icon("magnifier"))
                            .child(div().flex_1().child(self.search.clone())),
                    )
                    .child(ui::text_button("all-properties", "All").on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |state, cx| {
                                state.dispatch(&Action::Filter(None), window, cx)
                            })
                        }
                    }))
                    .child(
                        ui::text_button("animated-properties", "Animated").on_click({
                            let state = self.state.clone();
                            move |_, window, cx| {
                                state.update(cx, |state, cx| {
                                    state.dispatch(
                                        &Action::Filter(Some(PropertyFilter::Animated)),
                                        window,
                                        cx,
                                    )
                                })
                            }
                        }),
                    )
                    .child(ui::action_tool(
                        "open-graph",
                        "chart-line",
                        "Graph Editor (Shift+F3)",
                        &self.state,
                        Action::ToggleGraph,
                        graph_open,
                    ))
                    .child(
                        ui::text_button("hide-shy-layers", "Hide Shy")
                            .when(comp.hide_shy(), |s| {
                                s.bg(rgb(0x164a7b)).text_color(rgb(ui::BLUE))
                            })
                            .on_click({
                                let state = self.state.clone();
                                let hidden = comp.hide_shy();
                                move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::Edit(Command::SetHideShy(!hidden)),
                                            window,
                                            cx,
                                        )
                                    })
                                }
                            }),
                    )
                    .child(div().flex_1())
                    .child(ui::action_tool(
                        "timeline-minus",
                        "minus",
                        "Zoom out (−)",
                        &self.state,
                        Action::ZoomTimeline(0.5),
                        false,
                    ))
                    .child(div().w(px(38.0)).text_center().child(format!("{zoom:.0}×")))
                    .child(ui::action_tool(
                        "timeline-plus",
                        "plus",
                        "Zoom in (+)",
                        &self.state,
                        Action::ZoomTimeline(2.0),
                        false,
                    )),
            )
            .child(
                div()
                    .flex()
                    .h(px(29.0))
                    .flex_none()
                    .bg(rgb(0x262626))
                    .child(
                        div()
                            .w(px(left))
                            .flex_none()
                            .flex()
                            .items_end()
                            .px_2()
                            .pb_1()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child(div().w(px(178.0)).child("Switches"))
                            .child(div().flex_1().child("Source Name"))
                            .child(div().w(px(181.0)).child("Parent & Link       Order")),
                    )
                    .child(
                        div()
                            .id("time-ruler")
                            .relative()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .cursor_crosshair()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                                    window.focus(&this.focus);
                                    this.selected_key = None;
                                    this.scrubbing = true;
                                    if let Some(frame) = this.seek_x(event.position.x, cx) {
                                        this.state.update(cx, |state, cx| {
                                            state.dispatch(&Action::Seek(frame), window, cx)
                                        });
                                    }
                                }),
                            )
                            .child(
                                canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| ())
                                    .absolute()
                                    .size_full(),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(relative(work_left))
                                    .w(relative((work_right - work_left).max(0.0)))
                                    .top_0()
                                    .h(px(8.0))
                                    .bg(rgb(0x69634e))
                                    .border_l_2()
                                    .border_r_2()
                                    .border_color(rgb(ui::BLUE)),
                            )
                            .children((0..10).map(|tick| {
                                div()
                                    .absolute()
                                    .left(relative(tick as f32 / 10.0))
                                    .top(px(10.0))
                                    .h(px(19.0))
                                    .border_l_1()
                                    .border_color(rgb(0x777777))
                                    .pl_1()
                                    .text_size(px(10.0))
                                    .child(format!(
                                        "{:.2}s",
                                        (start as f32 + visible as f32 * tick as f32 / 10.0)
                                            / comp.fps() as f32
                                    ))
                            }))
                            .when(frame >= start && frame <= start + visible, |s| {
                                s.child(
                                    div()
                                        .absolute()
                                        .top(px(8.0))
                                        .left(relative(
                                            frame.saturating_sub(start) as f32 / visible as f32,
                                        ))
                                        .ml(px(-3.0))
                                        .w(px(7.0))
                                        .h(px(8.0))
                                        .bg(rgb(ui::BLUE)),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .id("timeline-body")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            if this.state.read(cx).graph_open
                                || this.ruler.get().is_none_or(|b| event.position.x < b.left())
                            {
                                return;
                            }
                            window.focus(&this.focus);
                            this.marquee_additive =
                                event.modifiers.shift || event.modifiers.control;
                            this.marquee = Some((event.position, event.position));
                            cx.notify();
                        }),
                    )
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("timeline-rows")
                            .min_h_0()
                            .overflow_y_scroll()
                            .when(graph_open, |s| {
                                s.w(px(left)).flex_none().overflow_x_hidden()
                            })
                            .when(!graph_open, |s| s.flex_1())
                            .child(rows),
                    )
                    .when(graph_open, |s| {
                        s.child(div().flex_1().min_w_0().min_h_0().child(self.graph.clone()))
                    }),
            )
            .child(
                div()
                    .h(px(29.0))
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(ui::BORDER))
                    .flex()
                    .items_center()
                    .px_2()
                    .gap_2()
                    .child(ui::action_tool(
                        "pan-time-left",
                        "arrow-left",
                        "Pan timeline left",
                        &self.state,
                        Action::PanTimeline(-(visible as i32 / 4).max(1)),
                        false,
                    ))
                    .child(ui::action_tool(
                        "pan-time-right",
                        "arrow-right",
                        "Pan timeline right",
                        &self.state,
                        Action::PanTimeline((visible as i32 / 4).max(1)),
                        false,
                    ))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child(format!(
                                "View: {}–{}f",
                                start,
                                (start + visible).min(comp.duration())
                            )),
                    )
                    .child(div().flex_1())
                    .child(ui::text_button("work-start", "Set In (B)").on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |state, cx| {
                                state.dispatch(&Action::WorkStart, window, cx)
                            })
                        }
                    }))
                    .child(ui::text_button("work-end", "Set Out (N)").on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |state, cx| {
                                state.dispatch(&Action::WorkEnd, window, cx)
                            })
                        }
                    }))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child(format!("Work area: {work_start}–{work_end}f")),
                    ),
            )
            .child(
                div()
                    .id("timeline-column-divider")
                    .absolute()
                    .left(px(left - 2.0))
                    .top(px(58.0))
                    .bottom(px(29.0))
                    .w(px(4.0))
                    .cursor_col_resize()
                    .hover(|s| s.bg(rgb(ui::BLUE)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.resizing = true;
                            cx.stop_propagation();
                        }),
                    )
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                        if event.click_count() == 2 {
                            this.left = LEFT;
                            cx.notify();
                        }
                    })),
            )
            .when(self.marquee.is_some(), |s| {
                let (a, b) = self.marquee.unwrap();
                s.child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let rect = Bounds::from_corners(
                                point(a.x.min(b.x), a.y.min(b.y)),
                                point(a.x.max(b.x), a.y.max(b.y)),
                            );
                            window.paint_quad(fill(rect, gpui::rgba(0x529bdf35)));
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .into_any_element()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ruler_mapping_accounts_for_pan_zoom_and_edges() {
        assert_eq!(frame_at(150.0, 100.0, 100.0, 30, 60, 150), 60);
        assert_eq!(frame_at(-50.0, 100.0, 100.0, 30, 60, 150), 30);
        assert_eq!(frame_at(500.0, 100.0, 100.0, 100, 60, 150), 149);
    }
}
