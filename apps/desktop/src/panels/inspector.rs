use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, Property};

pub(crate) struct Inspector {
    state: Entity<EditorState>,
    name: Entity<TextField>,
    fields: Vec<Entity<TextField>>,
    range: Vec<Entity<TextField>>,
    parent_open: bool,
    parent_owner: Option<u64>,
}
impl Inspector {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let edit = state.clone();
        let name = cx.new(|cx| {
            TextField::new(cx, move |text, window, cx| {
                edit.update(cx, |state, cx| {
                    if let Some(id) = state.editor.selected() {
                        state.dispatch(
                            &Action::Edit(Command::RenameLayer {
                                id,
                                name: text.into(),
                            }),
                            window,
                            cx,
                        );
                    }
                });
            })
        });
        let fields = Property::ALL
            .into_iter()
            .map(|property| {
                let edit = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |state, cx| {
                            if let Some(id) = state.editor.selected() {
                                match text.trim().parse::<f64>() {
                                    Ok(value) => state.dispatch(
                                        &Action::Edit(Command::SetValue {
                                            id,
                                            property,
                                            frame: state.frame,
                                            value,
                                        }),
                                        window,
                                        cx,
                                    ),
                                    Err(_) => {
                                        state.status =
                                            "Enter a finite number for the property.".into();
                                        cx.notify();
                                    }
                                }
                            }
                        });
                    })
                })
            })
            .collect();
        let range = (0..2)
            .map(|index| {
                let edit = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |state, cx| {
                            if let Some(layer) = state.editor.selected_layer() {
                                let Ok(value) = text.trim().parse::<u32>() else {
                                    state.status = "Enter a whole frame number.".into();
                                    cx.notify();
                                    return;
                                };
                                let duration = state.editor.project().composition().duration();
                                let command = Command::SetLayerRange {
                                    id: layer.id(),
                                    start: if index == 0 { value } else { layer.in_frame() },
                                    end: if index == 1 {
                                        value
                                    } else {
                                        layer.out_frame(duration)
                                    },
                                };
                                state.dispatch(&Action::Edit(command), window, cx);
                            }
                        });
                    })
                })
            })
            .collect();
        Self {
            state,
            name,
            fields,
            range,
            parent_open: false,
            parent_owner: None,
        }
    }
}
impl Render for Inspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let frame = state.frame;
        let duration = state.editor.project().composition().duration();
        let selected = state.editor.selected_layer().cloned();
        let comp = state.editor.project().composition().clone();
        let mut panel = div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .bg(rgb(ui::BG))
            .child(ui::panel_header("Properties"));
        let Some(layer) = selected else {
            return panel.child(
                div()
                    .p_4()
                    .text_color(rgb(ui::MUTED))
                    .child("Select a layer to view its properties."),
            );
        };
        let id = layer.id();
        if self.parent_owner != Some(id) {
            self.parent_open = false;
            self.parent_owner = Some(id);
        }
        let locked = layer.locked();
        self.name.update(cx, |field, _| {
            field.sync(id.to_string(), layer.name().to_string(), window)
        });
        let mut contents = div()
            .id("properties-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_3()
            .py_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_3()
                    .child(ui::icon("square"))
                    .child(
                        div()
                            .flex_1()
                            .when(!locked, |s| s.child(self.name.clone()))
                            .when(locked, |s| s.child(layer.name().to_string())),
                    ),
            )
            .child(
                div()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(0x353535))
                    .child(ui::icon("chevron-down"))
                    .child("Transform"),
            );
        for (index, property) in Property::ALL.into_iter().enumerate() {
            let track = layer.property(property);
            let value = track.value_at(frame);
            let key = track.keys().get(&frame);
            let animated = !track.keys().is_empty();
            self.fields[index].update(cx, |field, _| {
                field.sync(format!("{id}-{frame}"), format!("{value:.2}"), window)
            });
            let mut row = div()
                .flex()
                .items_center()
                .h(px(34.0))
                .gap_1()
                .child(
                    ui::action_tool(
                        gpui::SharedString::from(format!("watch-{property:?}")),
                        "stopwatch",
                        "Enable / disable animation",
                        &self.state,
                        Action::Edit(Command::ToggleAnimation {
                            id,
                            property,
                            frame,
                        }),
                        animated,
                    )
                    .when(locked, |s| s.opacity(0.35)),
                )
                .child(div().flex_1().text_size(px(11.0)).child(property.label()))
                .child(
                    div()
                        .w(px(68.0))
                        .flex_none()
                        .when(!locked, |s| s.child(self.fields[index].clone()))
                        .when(locked, |s| s.child(format!("{value:.2}"))),
                );
            if animated {
                row = row.child(ui::action_tool(
                    gpui::SharedString::from(format!("key-{property:?}")),
                    "diamond",
                    "Add / remove keyframe at current time",
                    &self.state,
                    Action::Edit(Command::ToggleKeyframe {
                        id,
                        property,
                        frame,
                    }),
                    key.is_some(),
                ));
            } else {
                row = row.child(div().w(px(26.0)));
            }
            contents = contents.child(row);
            if let Some(key) = key {
                let label = key.interpolation.label();
                contents = contents.child(
                    ui::text_button(
                        gpui::SharedString::from(format!("interpolation-{property:?}")),
                        format!("Interpolation: {label}"),
                    )
                    .text_size(px(10.0))
                    .text_color(rgb(ui::BLUE))
                    .on_click({
                        let state = self.state.clone();
                        let interpolation = key.interpolation.next();
                        move |_, window, cx| {
                            state.update(cx, |state, cx| {
                                state.dispatch(
                                    &Action::Edit(Command::SetInterpolation {
                                        id,
                                        property,
                                        frame,
                                        interpolation,
                                    }),
                                    window,
                                    cx,
                                )
                            });
                        }
                    }),
                );
            }
        }
        contents = contents.child(
            div()
                .mt_3()
                .py_2()
                .border_t_1()
                .border_color(rgb(0x353535))
                .child("Parent & Link"),
        );
        let parent_label = layer
            .parent()
            .and_then(|id| comp.layer(id))
            .map_or("None".to_string(), |p| format!("{} · {}", p.id(), p.name()));
        contents = contents.child(
            ui::text_button("parent-picker", format!("{parent_label}  ▾"))
                .justify_start()
                .when(locked, |s| s.opacity(0.4))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !locked {
                        this.parent_open = !this.parent_open;
                        cx.notify();
                    }
                })),
        );
        if self.parent_open && !locked {
            let choices = std::iter::once((None, "None".to_string())).chain(
                comp.layers()
                    .iter()
                    .filter(|l| comp.can_parent(id, Some(l.id())))
                    .map(|l| (Some(l.id()), format!("{} · {}", l.id(), l.name()))),
            );
            let mut menu = div()
                .id("parent-options")
                .max_h(px(150.0))
                .overflow_y_scroll()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(0x454545));
            for (parent, label) in choices {
                menu = menu.child(
                    ui::text_button(
                        gpui::SharedString::from(format!("parent-{}", parent.unwrap_or(0))),
                        label,
                    )
                    .justify_start()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.parent_open = false;
                        this.state.update(cx, |state, cx| {
                            state.dispatch(
                                &Action::Edit(Command::SetParent { id, parent, frame }),
                                window,
                                cx,
                            )
                        });
                        cx.notify();
                    })),
                );
            }
            contents = contents.child(menu);
        }
        contents = contents.child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(ui::MUTED))
                .child("Keeps current pose · opacity stays independent"),
        );
        contents = contents.child(
            div()
                .mt_3()
                .py_2()
                .border_t_1()
                .border_color(rgb(0x353535))
                .child("Layer timing"),
        );
        for (index, (label, value)) in [
            ("In (frame)", layer.in_frame()),
            ("Out (exclusive)", layer.out_frame(duration)),
        ]
        .into_iter()
        .enumerate()
        {
            self.range[index].update(cx, |field, _| {
                field.sync(id.to_string(), value.to_string(), window)
            });
            contents = contents.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(32.0))
                    .gap_2()
                    .child(div().flex_1().text_size(px(11.0)).child(label))
                    .child(
                        div()
                            .w(px(82.0))
                            .when(!locked, |s| s.child(self.range[index].clone()))
                            .when(locked, |s| s.child(value.to_string())),
                    ),
            );
        }
        contents = contents.child(div().mt_3().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(if locked { "Layer locked. Unlock it in the timeline to edit." } else { "Enter to apply · Escape to cancel\nStopwatch toggles animation; diamond toggles a key." }));
        panel = panel.child(contents);
        panel
    }
}
