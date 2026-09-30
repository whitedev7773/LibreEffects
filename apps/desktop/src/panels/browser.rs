use gpui::{Context, Entity, Window, div, prelude::*, px};
use libre_effects_core::Command;

use crate::{
    editor::{Action, EditorState, action_button},
    theme::ActiveTheme,
};

pub(crate) struct Browser {
    state: Entity<EditorState>,
}

impl Browser {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for Browser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = window.theme().colors;
        let comp = self.state.read(cx).editor.project().composition();
        div()
            .flex()
            .flex_col()
            .w(px(200.0))
            .flex_none()
            .h_full()
            .p_3()
            .gap_3()
            .border_r_1()
            .border_color(colors.border)
            .bg(colors.sidebar)
            .child(div().text_sm().child("Project"))
            .child(
                div()
                    .p_2()
                    .bg(colors.secondary)
                    .rounded_md()
                    .text_xs()
                    .child(comp.name().to_string())
                    .child(
                        div()
                            .mt_2()
                            .text_color(colors.muted_foreground)
                            .child(format!(
                                "{} x {}  /  {} fps",
                                comp.width(),
                                comp.height(),
                                comp.fps()
                            )),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_color(colors.muted_foreground)
                            .child(format!(
                                "{:.1} seconds",
                                comp.duration() as f64 / comp.fps() as f64
                            )),
                    ),
            )
            .child(
                action_button(
                    "add-rectangle",
                    "+ Rectangle",
                    &self.state,
                    Action::Edit(Command::AddRectangle),
                )
                .full_width(),
            )
            .child(
                div().text_xs().text_color(colors.muted_foreground).child(
                    "Create layers here. Select a layer in the timeline to edit its transform.",
                ),
            )
    }
}
