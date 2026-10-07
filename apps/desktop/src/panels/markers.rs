use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, MarkerEdit, MarkerTarget};

pub(crate) struct MarkerEditor {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
}
impl MarkerEditor {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let fields = (0..4)
            .map(|index| {
                let state = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        state.update(cx, |s, cx| {
                            let Some((target, marker)) = s.selected_marker() else {
                                return;
                            };
                            let (id, mut frame, mut duration, mut name, mut color) = (
                                marker.id(),
                                marker.frame(),
                                marker.duration(),
                                marker.name().to_string(),
                                marker.color(),
                            );
                            let error = match index {
                                0 => {
                                    name = text.into();
                                    None
                                }
                                1 | 2 => match text.trim().parse::<u32>() {
                                    Ok(n) => {
                                        if index == 1 {
                                            frame = n;
                                        } else {
                                            duration = n;
                                        }
                                        None
                                    }
                                    Err(_) => {
                                        Some("Enter a whole, nonnegative frame number".to_string())
                                    }
                                },
                                _ => match ui::parse_hex_color(text) {
                                    Ok(v) => {
                                        color = v;
                                        None
                                    }
                                    Err(e) => Some(e.to_string()),
                                },
                            };
                            if let Some(error) = error {
                                s.status = error;
                                cx.notify();
                                return;
                            }
                            s.dispatch(
                                &Action::Edit(Command::Marker {
                                    target,
                                    edit: MarkerEdit::Update {
                                        id,
                                        frame,
                                        duration,
                                        name,
                                        color,
                                    },
                                }),
                                window,
                                cx,
                            );
                        });
                    })
                })
            })
            .collect();
        Self { state, fields }
    }
}
impl Render for MarkerEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let Some((target, marker)) = state.selected_marker() else {
            return div().id("no-marker");
        };
        let marker = marker.clone();
        let id = marker.id();
        let composition = state.editor.project().active_composition_id();
        let locked = match target {
            MarkerTarget::Composition => false,
            MarkerTarget::Layer(id) => state
                .editor
                .project()
                .composition()
                .layer(id)
                .is_none_or(|l| l.locked()),
        };
        let title = match target {
            MarkerTarget::Composition => "Composition marker",
            MarkerTarget::Layer(_) => "Layer marker",
        };
        let mut body = div()
            .id("marker-editor")
            .w(px(310.0))
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(ui::BLUE))
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().flex_1().child(title))
                    .child(ui::text_button("close-marker", "Close").on_click({
                        let state = self.state.clone();
                        move |_, _, cx| {
                            state.update(cx, |s, cx| {
                                s.marker_selection = None;
                                cx.notify();
                            })
                        }
                    })),
            );
        for (index, (label, value)) in [
            ("Name", marker.name().to_string()),
            ("Frame", marker.frame().to_string()),
            ("Duration (frames)", marker.duration().to_string()),
            ("Color (RGB hex)", format!("{:06X}", marker.color())),
        ]
        .into_iter()
        .enumerate()
        {
            self.fields[index].update(cx, |f, _| {
                f.sync(
                    format!("{composition}-{target:?}-{id}"),
                    value.clone(),
                    window,
                )
            });
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(120.0)).child(label))
                    .child(
                        div()
                            .flex_1()
                            .when(!locked, |d| d.child(self.fields[index].clone()))
                            .when(locked, |d| d.child(value)),
                    ),
            );
        }
        body.child(
            div()
                .flex()
                .gap_2()
                .child(ui::text_button("marker-previous", "Previous").on_click({
                    let state = self.state.clone();
                    move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(&Action::NavigateMarker(false), w, cx)
                        })
                    }
                }))
                .child(ui::text_button("marker-next", "Next").on_click({
                    let state = self.state.clone();
                    move |_, w, cx| {
                        state.update(cx, |s, cx| s.dispatch(&Action::NavigateMarker(true), w, cx))
                    }
                }))
                .child(div().flex_1())
                .child(
                    ui::text_button("remove-marker", "Remove")
                        .when(locked, |d| d.opacity(0.4))
                        .on_click({
                            let state = self.state.clone();
                            move |_, w, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Marker {
                                            target,
                                            edit: MarkerEdit::Remove(id),
                                        }),
                                        w,
                                        cx,
                                    )
                                })
                            }
                        }),
                ),
        )
    }
}

pub(crate) fn marker_item(
    marker: &libre_effects_core::Marker,
    target: MarkerTarget,
    start: u32,
    visible: u32,
    state: &Entity<EditorState>,
) -> gpui::Stateful<gpui::Div> {
    let id = marker.id();
    let state = state.clone();
    let left = (marker.frame().saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
    let width = (marker.end().saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0) - left;
    div()
        .id(SharedString::from(format!("marker-{target:?}-{id}")))
        .absolute()
        .left(gpui::relative(left))
        .top_0()
        .h(px(12.0))
        .w(gpui::relative(width.max(0.0)))
        .min_w(px(12.0))
        .flex()
        .cursor_pointer()
        .border_l_2()
        .border_color(rgb(marker.color()))
        .child(
            div()
                .flex_none()
                .max_w(px(180.0))
                .overflow_hidden()
                .px_1()
                .whitespace_nowrap()
                .bg(rgb(ui::PANEL))
                .text_size(px(10.0))
                .text_color(rgb(marker.color()))
                .child(marker.name().to_string()),
        )
        .when(marker.duration() > 0, |d| d.border_t_2())
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            state.update(cx, |s, cx| {
                s.dispatch(&Action::ShowMarker(target, id), window, cx)
            });
        })
}
