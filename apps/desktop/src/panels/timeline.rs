use crate::{
    components::TextField,
    editor::{Action, EditorState, PropertyFilter, timecode},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels,
    SharedString, Window, canvas, div, fill, point, prelude::*, px, relative, rgb, size,
};
use libre_effects_core::{Command, LayerId, Property};
use std::{cell::Cell, rc::Rc};

pub(super) const LEFT: f32 = 560.0;
#[derive(Clone)]
struct KeyDrag {
    id: LayerId,
    property: Property,
    from: u32,
    to: u32,
}
pub(crate) struct Timeline {
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
        let Some(frame) = self.seek_x(event.position.x, cx) else {
            return;
        };
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
        if let Some(drag) = self.drag.take() {
            if drag.to != drag.from {
                self.state.update(cx, |state, cx| {
                    state.dispatch(
                        &Action::Edit(Command::MoveKeyframe {
                            id: drag.id,
                            property: drag.property,
                            from: drag.from,
                            to: drag.to,
                        }),
                        window,
                        cx,
                    )
                });
                let exists = self
                    .state
                    .read(cx)
                    .editor
                    .project()
                    .composition()
                    .layer(drag.id)
                    .is_some_and(|layer| {
                        layer.property(drag.property).keys().contains_key(&drag.to)
                    });
                if exists && self.state.read(cx).status.starts_with("Edited") {
                    self.selected_key = Some((drag.id, drag.property, drag.to));
                }
            }
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
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
            if !layer.name().to_lowercase().contains(&query) {
                continue;
            }
            let id = layer.id();
            let selected_row = selected == Some(id);
            let control_id = |suffix: &str| SharedString::from(format!("layer-{id}-{suffix}"));
            let mut controls = div()
                .flex()
                .items_center()
                .h_full()
                .w(px(LEFT))
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
                        .on_click(cx.listener(move |this, _, window, cx| {
                            window.focus(&this.focus);
                            this.selected_key = None;
                            this.state.update(cx, |state, cx| {
                                state.dispatch(&Action::Select(id), window, cx)
                            });
                        })),
                );
            if let Some(parent) = layer.parent() {
                controls = controls.child(
                    ui::text_button(control_id("parent"), format!("↳ {parent}"))
                        .text_size(px(10.0))
                        .text_color(rgb(ui::BLUE))
                        .on_click({
                            let state = self.state.clone();
                            move |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(&Action::Select(parent), window, cx)
                                })
                            }
                        }),
                );
            }
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
            let left =
                (layer.in_frame().saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
            let right = (layer.out_frame(comp.duration()).saturating_sub(start) as f32
                / visible as f32)
                .clamp(0.0, 1.0);
            let bar_visible =
                layer.out_frame(comp.duration()) > start && layer.in_frame() < start + visible;
            let time_area = div()
                .relative()
                .flex_1()
                .h_full()
                .overflow_hidden()
                .child(grid(start, visible, frame))
                .when(bar_visible, |area| {
                    area.child(
                        div()
                            .id(control_id("bar"))
                            .absolute()
                            .left(relative(left))
                            .w(relative((right - left).max(0.0)))
                            .top(px(5.0))
                            .h(px(17.0))
                            .bg(rgb(layer.color()))
                            .opacity(if layer.visible() { 0.85 } else { 0.25 })
                            .border_1()
                            .border_color(rgb(if selected_row { 0xddd2ff } else { 0x777777 }))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&this.focus);
                                this.selected_key = None;
                                this.state.update(cx, |state, cx| {
                                    state.dispatch(&Action::Select(id), window, cx)
                                });
                            })),
                    )
                });
            rows = rows.child(
                div()
                    .flex()
                    .h(px(27.0))
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
                                .w(px(LEFT))
                                .flex_none()
                                .pl(px(62.0))
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
                for property in Property::ALL {
                    let track = layer.property(property);
                    if let Some(filter) = filter {
                        if !filter.includes(property)
                            || (filter == PropertyFilter::Animated && track.keys().is_empty())
                        {
                            continue;
                        }
                    }
                    let prop_id = |suffix: &str| {
                        SharedString::from(format!("prop-{id}-{property:?}-{suffix}"))
                    };
                    let mut keys = div()
                        .relative()
                        .flex_1()
                        .h_full()
                        .overflow_hidden()
                        .child(grid(start, visible, frame));
                    for key_frame in track.keys().keys().copied() {
                        let drag = self.drag.as_ref().filter(|d| {
                            d.id == id && d.property == property && d.from == key_frame
                        });
                        let display_frame = drag.map_or(key_frame, |d| d.to);
                        if display_frame < start || display_frame > start + visible {
                            continue;
                        }
                        let active = self.selected_key == Some((id, property, key_frame));
                        keys = keys.child(
                            div()
                                .id(SharedString::from(format!(
                                    "diamond-{id}-{property:?}-{key_frame}"
                                )))
                                .absolute()
                                .left(relative((display_frame - start) as f32 / visible as f32))
                                .ml(px(-6.0))
                                .top(px(4.0))
                                .size(px(13.0))
                                .text_color(rgb(if active { ui::BLUE } else { 0xc8c8c8 }))
                                .cursor_pointer()
                                .child(ui::icon("diamond").text_color(rgb(if active {
                                    ui::BLUE
                                } else {
                                    0xc8c8c8
                                })))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, window, cx| {
                                        window.focus(&this.focus);
                                        this.selected_key = Some((id, property, key_frame));
                                        this.scrubbing = false;
                                        let locked = this
                                            .state
                                            .read(cx)
                                            .editor
                                            .project()
                                            .composition()
                                            .layer(id)
                                            .is_none_or(|l| l.locked());
                                        if !locked {
                                            this.drag = Some(KeyDrag {
                                                id,
                                                property,
                                                from: key_frame,
                                                to: key_frame,
                                            });
                                        }
                                        this.state.update(cx, |state, cx| {
                                            state.graph_property = property;
                                            state.graph_key = Some((id, key_frame));
                                            state.dispatch(&Action::Seek(key_frame), window, cx)
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        );
                    }
                    rows = rows.child(
                        div()
                            .flex()
                            .h(px(25.0))
                            .flex_none()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .w(px(LEFT))
                                    .flex_none()
                                    .pl(px(80.0))
                                    .child(ui::action_tool(
                                        prop_id("watch"),
                                        "stopwatch",
                                        "Toggle property animation",
                                        &self.state,
                                        Action::Edit(Command::ToggleAnimation {
                                            id,
                                            property,
                                            frame,
                                        }),
                                        !track.keys().is_empty(),
                                    ))
                                    .child(
                                        ui::text_button(
                                            prop_id("graph-property"),
                                            property.label(),
                                        )
                                        .flex_1()
                                        .justify_start()
                                        .text_size(px(11.0))
                                        .when(graph_open && graph_property == property, |s| {
                                            s.text_color(rgb(ui::BLUE))
                                        })
                                        .on_click({
                                            let state = self.state.clone();
                                            move |_, _, cx| {
                                                state.update(cx, |s, cx| {
                                                    s.graph_property = property;
                                                    s.graph_key = None;
                                                    cx.notify();
                                                })
                                            }
                                        }),
                                    )
                                    .child(
                                        div()
                                            .w(px(65.0))
                                            .text_color(rgb(ui::BLUE))
                                            .text_size(px(11.0))
                                            .child(format!("{:.2}", track.value_at(frame))),
                                    )
                                    .child(ui::action_tool(
                                        prop_id("key"),
                                        "diamond",
                                        "Add / remove keyframe",
                                        &self.state,
                                        Action::Edit(Command::ToggleKeyframe {
                                            id,
                                            property,
                                            frame,
                                        }),
                                        track.keys().contains_key(&frame),
                                    )),
                            )
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
                    this.scrubbing = false;
                    this.selected_key = None;
                    cx.notify();
                }
                if event.keystroke.key == "delete"
                    && let Some((id, property, frame)) = this.selected_key.take()
                {
                    let exists = this
                        .state
                        .read(cx)
                        .editor
                        .project()
                        .composition()
                        .layer(id)
                        .is_some_and(|l| l.property(property).keys().contains_key(&frame));
                    if exists {
                        this.state.update(cx, |state, cx| {
                            state.dispatch(
                                &Action::Edit(Command::ToggleKeyframe {
                                    id,
                                    property,
                                    frame,
                                }),
                                window,
                                cx,
                            )
                        });
                    }
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
                            .w(px(LEFT))
                            .flex_none()
                            .flex()
                            .items_end()
                            .px_2()
                            .pb_1()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child(div().w(px(80.0)).child("Switches"))
                            .child(div().flex_1().child("Source Name"))
                            .child("Order"),
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
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("timeline-rows")
                            .min_h_0()
                            .overflow_y_scroll()
                            .when(graph_open, |s| {
                                s.w(px(LEFT)).flex_none().overflow_x_hidden()
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
