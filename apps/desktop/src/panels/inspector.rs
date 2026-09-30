use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px};
use libre_effects_core::{Command, Interpolation, Property};

use crate::{
    editor::{Action, EditorState, action_button},
    theme::ActiveTheme,
};

pub(crate) struct Inspector {
    state: Entity<EditorState>,
}

impl Inspector {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for Inspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = window.theme().colors;
        let state = self.state.read(cx);
        let frame = state.frame;
        let mut panel = div()
            .id("properties")
            .flex()
            .flex_col()
            .w(px(320.0))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .p_3()
            .gap_2()
            .border_l_1()
            .border_color(colors.border)
            .bg(colors.card)
            .child(div().text_sm().child("Transform"));
        let Some(layer) = state.editor.selected_layer() else {
            return panel.child(
                div()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child("Select a layer in the timeline."),
            );
        };
        panel = panel.child(
            div()
                .text_xs()
                .text_color(colors.muted_foreground)
                .child(layer.name().to_string()),
        );
        let id = layer.id();
        for property in Property::ALL {
            let track = layer.property(property);
            let value = track.value_at(frame);
            let key = track.keys().get(&frame);
            let interpolation = key.map_or(Interpolation::Linear, |key| key.interpolation);
            let step = match property {
                Property::PositionX
                | Property::PositionY
                | Property::AnchorX
                | Property::AnchorY => 10.0,
                _ => 5.0,
            };
            let down = if property == Property::Opacity {
                (value - step).max(0.0)
            } else {
                value - step
            };
            let up = if property == Property::Opacity {
                (value + step).min(100.0)
            } else {
                value + step
            };
            let control_id = |suffix| SharedString::from(format!("{property:?}-{suffix}"));
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap_1()
                    .py_2()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(div().flex_1().text_xs().child(property.label()))
                            .child(
                                action_button(
                                    control_id("minus"),
                                    "−",
                                    &self.state,
                                    Action::Edit(Command::SetValue {
                                        id,
                                        property,
                                        frame,
                                        value: down,
                                    }),
                                )
                                .disabled(layer.locked()),
                            )
                            .child(
                                div()
                                    .w(px(56.0))
                                    .text_xs()
                                    .text_center()
                                    .child(format!("{value:.1}")),
                            )
                            .child(
                                action_button(
                                    control_id("plus"),
                                    "+",
                                    &self.state,
                                    Action::Edit(Command::SetValue {
                                        id,
                                        property,
                                        frame,
                                        value: up,
                                    }),
                                )
                                .disabled(layer.locked()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                action_button(
                                    control_id("key"),
                                    if key.is_some() {
                                        "◆ Remove key"
                                    } else {
                                        "◇ Add key"
                                    },
                                    &self.state,
                                    Action::Edit(Command::ToggleKeyframe {
                                        id,
                                        property,
                                        frame,
                                    }),
                                )
                                .disabled(layer.locked()),
                            )
                            .child(
                                action_button(
                                    control_id("interpolation"),
                                    format!("{interpolation:?}"),
                                    &self.state,
                                    Action::Edit(Command::SetInterpolation {
                                        id,
                                        property,
                                        frame,
                                        interpolation: interpolation.next(),
                                    }),
                                )
                                .disabled(layer.locked() || key.is_none()),
                            ),
                    ),
            );
        }
        panel.child(div().text_xs().text_color(colors.muted_foreground)
            .child("Add the first key, move the playhead, then change a value to add another key. Interpolation applies after the key."))
    }
}
