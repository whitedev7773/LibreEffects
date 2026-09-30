use std::{cell::Cell, rc::Rc};

use gpui::{
    Bounds, Context, Entity, MouseButton, Pixels, SharedString, Window, canvas, div, prelude::*,
    px, relative, rgb,
};
use libre_effects_core::{Command, Property};

use crate::{
    editor::{Action, EditorState, action_button, timecode},
    theme::ActiveTheme,
};

pub(crate) struct Timeline {
    state: Entity<EditorState>,
    ruler_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Timeline {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            ruler_bounds: Rc::new(Cell::new(None)),
        }
    }
}

impl Render for Timeline {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = window.theme().colors;
        let state = self.state.read(cx);
        let comp = state.editor.project().composition();
        let duration = comp.duration();
        let frame = state.frame;
        let selected = state.editor.selected();
        let left_width = px(360.0);
        let mut contents = div().flex().flex_col().min_w(px(620.0)).child(
            div()
                .flex()
                .h_8()
                .items_center()
                .child(
                    div()
                        .w(left_width)
                        .flex_none()
                        .px_3()
                        .text_xs()
                        .child("Layers  /  top layer renders in front"),
                )
                .child({
                    let hit_bounds = self.ruler_bounds.clone();
                    let paint_bounds = self.ruler_bounds.clone();
                    let state = self.state.clone();
                    let keyboard_state = self.state.clone();
                    div()
                        .id("time-ruler")
                        .relative()
                        .flex_1()
                        .h_full()
                        .cursor_pointer()
                        .bg(colors.secondary)
                        .tab_index(0)
                        .focus(|style| style.bg(colors.accent))
                        .on_key_down(move |event, window, cx| {
                            let action = match event.keystroke.key.as_str() {
                                "left" => Action::Step(-1),
                                "right" => Action::Step(1),
                                "home" => Action::Seek(0),
                                "end" => Action::Seek(duration - 1),
                                "space" => Action::Play,
                                _ => return,
                            };
                            cx.stop_propagation();
                            keyboard_state
                                .update(cx, |state, cx| state.dispatch(&action, window, cx));
                        })
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            if let Some(bounds) = hit_bounds.get() {
                                let fraction = f32::from(event.position.x - bounds.left())
                                    / f32::from(bounds.size.width).max(1.0);
                                let target = (fraction.clamp(0.0, 1.0) * duration as f32) as u32;
                                state.update(cx, |state, cx| {
                                    state.dispatch(&Action::Seek(target), window, cx)
                                });
                            }
                        })
                        .child(
                            canvas(
                                move |bounds, _, _| paint_bounds.set(Some(bounds)),
                                |_, _, _, _| (),
                            )
                            .absolute()
                            .size_full(),
                        )
                        .children((0..5).map(|tick| {
                            div()
                                .absolute()
                                .left(relative(tick as f32 / 5.0))
                                .top(px(6.0))
                                .pl_1()
                                .border_l_1()
                                .border_color(colors.muted_foreground)
                                .text_xs()
                                .child(format!(
                                    "{:.1}s",
                                    duration as f64 * tick as f64 / 5.0 / comp.fps() as f64
                                ))
                        }))
                }),
        );
        for (index, layer) in comp.layers().iter().enumerate() {
            let id = layer.id();
            let control_id = |suffix| SharedString::from(format!("layer-{id}-{suffix}"));
            let mut row =
                div().flex().flex_col().flex_none().child(
                    div()
                        .flex()
                        .h_8()
                        .items_center()
                        .border_b_1()
                        .border_color(colors.border)
                        .when(selected == Some(id), |row| row.bg(colors.secondary))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .w(left_width)
                                .flex_none()
                                .gap_1()
                                .px_1()
                                .child(
                                    action_button(
                                        control_id("visible"),
                                        if layer.visible() { "On" } else { "Off" },
                                        &self.state,
                                        Action::Edit(Command::ToggleVisible(id)),
                                    )
                                    .disabled(layer.locked()),
                                )
                                .child(action_button(
                                    control_id("lock"),
                                    if layer.locked() { "Unlock" } else { "Lock" },
                                    &self.state,
                                    Action::Edit(Command::ToggleLocked(id)),
                                ))
                                .child(div().flex_1().min_w_0().overflow_hidden().child(
                                    action_button(
                                        control_id("select"),
                                        layer.name().to_string(),
                                        &self.state,
                                        Action::Select(id),
                                    ),
                                ))
                                .child(
                                    action_button(
                                        control_id("up"),
                                        "↑",
                                        &self.state,
                                        Action::Edit(Command::MoveLayer {
                                            id,
                                            index: index.saturating_sub(1),
                                        }),
                                    )
                                    .disabled(layer.locked() || index == 0),
                                )
                                .child(
                                    action_button(
                                        control_id("down"),
                                        "↓",
                                        &self.state,
                                        Action::Edit(Command::MoveLayer {
                                            id,
                                            index: index + 1,
                                        }),
                                    )
                                    .disabled(layer.locked() || index + 1 == comp.layers().len()),
                                )
                                .child(
                                    action_button(
                                        control_id("delete"),
                                        "Del",
                                        &self.state,
                                        Action::Edit(Command::RemoveLayer(id)),
                                    )
                                    .disabled(layer.locked()),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h_4()
                                .mx_1()
                                .rounded_sm()
                                .bg(rgb(layer.color()))
                                .opacity(if layer.visible() { 0.75 } else { 0.2 }),
                        ),
                );
            if selected == Some(id) {
                for property in Property::ALL {
                    let track = layer.property(property);
                    if track.keys().is_empty() {
                        continue;
                    }
                    row = row.child(
                        div()
                            .flex()
                            .h_6()
                            .items_center()
                            .child(
                                div()
                                    .w(left_width)
                                    .flex_none()
                                    .pl_8()
                                    .text_xs()
                                    .text_color(colors.muted_foreground)
                                    .child(property.label()),
                            )
                            .child(div().relative().flex_1().h_full().children(
                                track.keys().keys().map(|key_frame| {
                                    div()
                                        .absolute()
                                        .left(relative(*key_frame as f32 / duration as f32))
                                        .ml(px(-10.0))
                                        .child(
                                            action_button(
                                                SharedString::from(format!(
                                                    "key-{id}-{property:?}-{key_frame}"
                                                )),
                                                "◆",
                                                &self.state,
                                                Action::Seek(*key_frame),
                                            )
                                            .size(crate::components::ButtonSize::IconXSmall),
                                        )
                                }),
                            )),
                    );
                }
            }
            contents = contents.child(row);
        }
        if comp.layers().is_empty() {
            contents = contents.child(
                div()
                    .p_6()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child("No layers yet. Click + Rectangle in the Project panel."),
            );
        }
        div()
            .flex()
            .flex_col()
            .h(px(270.0))
            .min_h(px(180.0))
            .flex_none()
            .bg(colors.card)
            .border_t_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex()
                    .h_9()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .child(div().text_sm().mr_3().child("Timeline"))
                    .child(action_button(
                        "first-frame",
                        "Start",
                        &self.state,
                        Action::Seek(0),
                    ))
                    .child(action_button(
                        "previous-frame",
                        "−1f",
                        &self.state,
                        Action::Step(-1),
                    ))
                    .child(action_button(
                        "play",
                        if state.playing { "Pause" } else { "Play" },
                        &self.state,
                        Action::Play,
                    ))
                    .child(action_button(
                        "next-frame",
                        "+1f",
                        &self.state,
                        Action::Step(1),
                    ))
                    .child(action_button(
                        "last-frame",
                        "End",
                        &self.state,
                        Action::Seek(duration - 1),
                    ))
                    .child(div().text_xs().child(timecode(frame, comp.fps())))
                    .child(
                        div()
                            .text_xs()
                            .text_color(colors.muted_foreground)
                            .child(format!("Frame {frame} / {}", duration - 1)),
                    ),
            )
            .child(
                div()
                    .id("timeline-scroll")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .overflow_x_scroll()
                    .child(contents)
                    .child(
                        div()
                            .absolute()
                            .left(left_width)
                            .right_0()
                            .top_0()
                            .h_full()
                            .child(
                                div()
                                    .relative()
                                    .left(relative(frame as f32 / duration as f32))
                                    .w(px(1.0))
                                    .h_full()
                                    .bg(rgb(0xb9acff)),
                            ),
                    ),
            )
    }
}
