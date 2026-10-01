use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, rgb};
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
        let project = self.state.read(cx).editor.project();
        let comp = project.composition();
        let active = project.active_composition_id();
        let query = self.search.read(cx).value().trim().to_lowercase();
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
            .child(
                div()
                    .id("composition-items")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(
                        project
                            .compositions()
                            .into_iter()
                            .filter(|(_, comp)| comp.name().to_lowercase().contains(&query))
                            .map(|(id, comp)| {
                                let state = self.state.clone();
                                ui::text_button(
                                    SharedString::from(format!("project-comp-{id}")),
                                    "",
                                )
                                .w_full()
                                .h(px(29.0))
                                .gap_2()
                                .justify_start()
                                .px_3()
                                .when(id == active, |s| s.bg(rgb(0x343434)))
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
                                )
                                .on_click(move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(&Action::ActivateComposition(id), window, cx)
                                    })
                                })
                            }),
                    ),
            )
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
                        "project-composition",
                        "plus",
                        "New composition",
                        &self.state,
                        Action::Edit(Command::NewComposition),
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
