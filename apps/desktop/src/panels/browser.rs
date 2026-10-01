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
    effects: Entity<super::effects::EffectControls>,
}
impl Browser {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let search = cx.new(|cx| TextField::new(cx, |_, _, _| {}));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let effects = cx.new(|cx| super::effects::EffectControls::new(state.clone(), cx));
        Self {
            state,
            search,
            effects,
        }
    }
}
impl Render for Browser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let effects_open = self.state.read(cx).effect_controls_open;
        let tabs = div()
            .flex()
            .h(px(27.0))
            .flex_none()
            .border_b_1()
            .border_color(rgb(ui::BORDER))
            .child(
                ui::text_button("project-tab", "Project")
                    .when(!effects_open, |s| s.text_color(rgb(ui::BLUE)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            s.effect_controls_open = false;
                            cx.notify();
                        });
                        cx.notify();
                    })),
            )
            .child(
                ui::text_button("effect-controls-tab", "Effect Controls")
                    .when(effects_open, |s| s.text_color(rgb(ui::BLUE)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            s.effect_controls_open = true;
                            cx.notify();
                        });
                        cx.notify();
                    })),
            );
        if effects_open {
            return div()
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .bg(rgb(ui::BG))
                .child(tabs)
                .child(div().flex_1().min_h_0().child(self.effects.clone()))
                .into_any_element();
        }
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
            .child(tabs)
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
                                "{} fps  •  {:.2} s",
                                comp.fps().label(),
                                comp.fps().seconds(u64::from(comp.duration()))
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
                                .when(id != active, |s| {
                                    s.child(ui::action_tool(
                                        SharedString::from(format!("add-composition-{id}")),
                                        "plus",
                                        "Add to active composition",
                                        &self.state,
                                        Action::AddComposition(id),
                                        false,
                                    ))
                                })
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
            .into_any_element()
    }
}
