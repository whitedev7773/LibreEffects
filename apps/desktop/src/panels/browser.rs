use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::Command;

pub(crate) struct Browser {
    state: Entity<EditorState>,
    search: Entity<TextField>,
}
impl Browser {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let search = cx.new(|cx| TextField::new(cx, |_, _, _| {}));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self { state, search }
    }
}
impl Render for Browser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let comp = self.state.read(cx).editor.project().composition();
        let matches = comp
            .name()
            .to_lowercase()
            .contains(&self.search.read(cx).value().trim().to_lowercase());
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .bg(rgb(ui::BG))
            .overflow_hidden()
            .child(ui::panel_header("Project"))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .p_3()
                    .h(px(110.0))
                    .flex_none()
                    .child(
                        div()
                            .w(px(72.0))
                            .h(px(45.0))
                            .bg(rgb(0x080808))
                            .border_1()
                            .border_color(rgb(0x4b4b4b))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(ui::icon("filmstrip")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .text_size(px(11.0))
                            .child(comp.name().to_string())
                            .child(div().text_color(rgb(ui::MUTED)).child(format!(
                                "{} × {} (1.00)",
                                comp.width(),
                                comp.height()
                            )))
                            .child(div().text_color(rgb(ui::MUTED)).child(format!(
                                "{:.2} fps  •  {:.2} s",
                                comp.fps(),
                                comp.duration() as f32 / comp.fps() as f32
                            )))
                            .child(
                                div()
                                    .text_color(rgb(ui::MUTED))
                                    .child(format!("{} layers", comp.layers().len())),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .mx_2()
                    .gap_1()
                    .child(ui::icon("magnifier"))
                    .child(div().flex_1().child(self.search.clone())),
            )
            .child(
                div()
                    .flex()
                    .px_3()
                    .h(px(25.0))
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(0x3a3a3a))
                    .text_size(px(11.0))
                    .text_color(rgb(ui::MUTED))
                    .child(div().flex_1().child("Name"))
                    .child("Type"),
            )
            .when(matches, |s| {
                s.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .h(px(29.0))
                        .px_3()
                        .bg(rgb(0x343434))
                        .child(ui::icon("filmstrip"))
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .child(comp.name().to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(ui::MUTED))
                                .child("Comp"),
                        ),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .h(px(31.0))
                    .flex_none()
                    .flex()
                    .gap_1()
                    .px_2()
                    .items_center()
                    .border_t_1()
                    .border_color(rgb(ui::BORDER))
                    .child(ui::action_tool(
                        "project-open",
                        "folder-open",
                        "Open project",
                        &self.state,
                        Action::RequestOpen,
                        false,
                    ))
                    .child(ui::action_tool(
                        "project-rectangle",
                        "plus",
                        "New rectangle layer",
                        &self.state,
                        Action::Edit(Command::AddRectangle),
                        false,
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child("RGBA"),
                    ),
            )
    }
}
