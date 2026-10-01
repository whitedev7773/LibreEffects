use super::Inspector;
use crate::{
    editor::{Action, EditorState, timecode},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Alignment, Command};

pub(crate) struct Sidebar {
    state: Entity<EditorState>,
    inspector: Entity<Inspector>,
    expanded: [bool; 3],
}
impl Sidebar {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            inspector: cx.new(|cx| Inspector::new(state.clone(), cx)),
            state,
            expanded: [true, false, false],
        }
    }
}
impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let comp = state.editor.project().composition();
        let info = format!(
            "{}\n{} × {} · {} fps\nTime: {}\n{} layers",
            comp.name(),
            comp.width(),
            comp.height(),
            comp.fps(),
            timecode(state.frame, comp.fps()),
            comp.layers().len()
        );
        let playing = state.playing;
        let work = format!("Work area: {}–{}f", state.work_start, state.work_end);
        let mut panel = div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .bg(rgb(ui::BG));
        for (index, label) in ["Properties", "Info", "Preview"].into_iter().enumerate() {
            let active = self.expanded[index];
            panel = panel.child(
                ui::text_button(gpui::SharedString::from(format!("dock-{label}")), label)
                    .h(px(27.0))
                    .flex_none()
                    .justify_start()
                    .border_b_1()
                    .border_color(rgb(ui::BORDER))
                    .when(active, |s| s.text_color(rgb(ui::BLUE)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.expanded[index] = !this.expanded[index];
                        cx.notify();
                    })),
            );
            if active {
                panel = panel.child(match index {
                    0 => div()
                        .flex_1()
                        .min_h_0()
                        .child(self.inspector.clone())
                        .into_any_element(),
                    1 => div()
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_size(px(11.0))
                        .children(info.lines().map(|line| div().child(line.to_string())))
                        .into_any_element(),
                    _ => div()
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .gap_1()
                                .child(ui::action_tool(
                                    "preview-first",
                                    "arrow-left",
                                    "First frame (Home)",
                                    &self.state,
                                    Action::Seek(0),
                                    false,
                                ))
                                .child(ui::action_tool(
                                    "preview-back",
                                    "arrow-left",
                                    "Previous frame",
                                    &self.state,
                                    Action::Step(-1),
                                    false,
                                ))
                                .child(ui::action_tool(
                                    "preview-play",
                                    if playing { "pause" } else { "play" },
                                    "Play / Pause (Space)",
                                    &self.state,
                                    Action::Play,
                                    playing,
                                ))
                                .child(ui::action_tool(
                                    "preview-next",
                                    "arrow-right",
                                    "Next frame",
                                    &self.state,
                                    Action::Step(1),
                                    false,
                                )),
                        )
                        .child("Shortcut: Space")
                        .child(work.clone())
                        .child("Playback loops within the work area.")
                        .into_any_element(),
                });
            }
        }
        panel
    }
}
pub(crate) struct Align {
    state: Entity<EditorState>,
}
impl Align {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}
impl Render for Align {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let selected = state.editor.selected();
        let frame = state.frame;
        let enabled = state.editor.selected_layer().is_some_and(|l| !l.locked());
        let mut controls = div().flex().gap_2();
        for (index, (icon, label, alignment)) in [
            ("object-align-left", "Align left", Alignment::Left),
            (
                "object-align-center-horizontal",
                "Center horizontally",
                Alignment::HorizontalCenter,
            ),
            ("object-align-right", "Align right", Alignment::Right),
            ("object-align-top", "Align top", Alignment::Top),
            (
                "object-align-center-vertical",
                "Center vertically",
                Alignment::VerticalCenter,
            ),
            ("object-align-bottom", "Align bottom", Alignment::Bottom),
        ]
        .into_iter()
        .enumerate()
        {
            controls = controls.child(
                ui::tool(
                    gpui::SharedString::from(format!("align-{index}")),
                    icon,
                    label,
                    false,
                )
                .when(!enabled, |s| s.opacity(0.35))
                .on_click({
                    let state = self.state.clone();
                    move |_, window, cx| {
                        if let Some(id) = selected.filter(|_| enabled) {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::AlignLayer {
                                        id,
                                        frame,
                                        alignment,
                                    }),
                                    window,
                                    cx,
                                )
                            });
                        }
                    }
                }),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(ui::BG))
            .child(ui::panel_header("Align"))
            .child(
                div()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child("Align Layers to:  Composition")
                    .child(controls)
                    .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
                        if enabled {
                            "Align selected layer bounds"
                        } else {
                            "Select an unlocked layer"
                        },
                    )),
            )
    }
}
