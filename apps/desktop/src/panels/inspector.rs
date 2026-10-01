use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, Content, Mask, Property, PropertyPath, TrackEdit};

pub(crate) struct Inspector {
    matte: Option<(
        u64,
        Entity<super::matte::MattePicker>,
        Entity<super::matte::MattePicker>,
    )>,
    blend: Option<(u64, Entity<super::blend::BlendPicker>)>,
    state: Entity<EditorState>,
    name: Entity<TextField>,
    fields: Vec<Entity<TextField>>,
    range: Vec<Entity<TextField>>,
    parent_open: bool,
    parent_owner: Option<u64>,
    extra: Vec<Entity<TextField>>,
    playback: Vec<Entity<TextField>>,
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
        let playback = (0..3)
            .map(|index| {
                let edit = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |s, cx| {
                            let Some(id) = s.editor.selected() else {
                                return;
                            };
                            let Ok(value) = text.trim().parse::<f64>() else {
                                s.status = "Enter a number for video playback".into();
                                cx.notify();
                                return;
                            };
                            let command = if index == 0 {
                                Command::SetVideoSpeed {
                                    id,
                                    speed: value / 100.0,
                                }
                            } else if index == 1 {
                                Command::SetVideoSourceIn { id, seconds: value }
                            } else {
                                Command::EditTrack {
                                    id,
                                    property: PropertyPath::TimeRemap,
                                    edit: TrackEdit::Value {
                                        frame: s.frame,
                                        value,
                                    },
                                }
                            };
                            s.dispatch(&Action::Edit(command), window, cx);
                        });
                    })
                    .numeric()
                })
            })
            .collect();
        let extra = (0..11)
            .map(|index| {
                let edit = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |s, cx| {
                            let Some(l) = s.editor.selected_layer() else {
                                return;
                            };
                            let id = l.id();
                            let number = text.parse::<f64>().ok();
                            let command = match index {
                                0 => {
                                    if let Content::Text { font_size, .. } = l.content() {
                                        Some(Command::SetContent {
                                            id,
                                            content: Content::Text {
                                                text: text.into(),
                                                font_size: *font_size,
                                            },
                                        })
                                    } else {
                                        None
                                    }
                                }
                                1 => {
                                    if let (Content::Text { text, .. }, Some(font_size)) =
                                        (l.content(), number)
                                    {
                                        Some(Command::SetContent {
                                            id,
                                            content: Content::Text {
                                                text: text.clone(),
                                                font_size,
                                            },
                                        })
                                    } else {
                                        None
                                    }
                                }
                                2 => u32::from_str_radix(text.trim().trim_start_matches('#'), 16)
                                    .ok()
                                    .map(|color| Command::SetColor { id, color }),
                                3 | 4 => number.map(|v| {
                                    let mut effects = l.effects();
                                    if index == 3 {
                                        effects.blur = v;
                                    } else {
                                        effects.brightness = v;
                                    }
                                    Command::SetEffects { id, effects }
                                }),
                                9 | 10 => text.trim().parse::<u32>().ok().map(|value| {
                                    Command::ConfigureSolid {
                                        id,
                                        width: if index == 9 { value } else { l.width() as u32 },
                                        height: if index == 10 {
                                            value
                                        } else {
                                            l.height() as u32
                                        },
                                        color: l.color(),
                                    }
                                }),
                                _ => l.mask().zip(number).map(|(mut m, v)| {
                                    match index {
                                        5 => m.x = v,
                                        6 => m.y = v,
                                        7 => m.width = v,
                                        _ => m.height = v,
                                    };
                                    Command::SetMask { id, mask: Some(m) }
                                }),
                            };
                            if let Some(c) = command {
                                s.dispatch(&Action::Edit(c), window, cx);
                            } else {
                                s.status = "Invalid value".into();
                                cx.notify();
                            }
                        })
                    })
                })
            })
            .collect();
        Self {
            blend: None,
            matte: None,
            extra,
            state,
            name,
            fields,
            range,
            parent_open: false,
            parent_owner: None,
            playback,
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
            .bg(rgb(ui::BG));
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
        for (label, indices) in [
            ("Anchor Point", vec![2, 3]),
            ("Position", vec![0, 1]),
            ("Scale", vec![4, 5]),
            ("Rotation", vec![6]),
            ("Opacity", vec![7]),
        ] {
            let properties: Vec<_> = indices.iter().map(|i| Property::ALL[*i]).collect();
            let animated = properties
                .iter()
                .any(|p| !layer.property(*p).keys().is_empty());
            let command = Command::Batch(
                properties
                    .iter()
                    .filter(|p| layer.property(**p).keys().is_empty() == !animated)
                    .map(|p| Command::ToggleAnimation {
                        id,
                        property: *p,
                        frame,
                    })
                    .collect(),
            );
            let mut row = div()
                .flex()
                .items_center()
                .h(px(30.0))
                .gap_1()
                .child(ui::action_tool(
                    gpui::SharedString::from(format!("watch-{label}")),
                    "stopwatch",
                    "Toggle animation",
                    &self.state,
                    Action::Edit(command),
                    animated,
                ))
                .child(div().flex_1().text_size(px(11.0)).child(label));
            for index in indices {
                let property = Property::ALL[index];
                let value = layer.property(property).value_at(frame);
                self.fields[index].update(cx, |field, _| {
                    field.set_numeric();
                    field.sync(format!("{id}-{frame}"), format!("{value:.2}"), window);
                });
                row = row.child(
                    div()
                        .w(px(62.0))
                        .when(!locked, |s| s.child(self.fields[index].clone()))
                        .when(locked, |s| s.child(format!("{value:.2}"))),
                );
            }
            contents = contents.child(row);
        }
        let is_null = matches!(layer.content(), Content::Null);
        if !is_null {
            if self.blend.as_ref().is_none_or(|(owner, _)| *owner != id) {
                self.blend = Some((
                    id,
                    cx.new(|cx| super::blend::BlendPicker::new(self.state.clone(), id, cx)),
                ));
            }
            contents = contents.child(
                div()
                    .flex()
                    .items_center()
                    .mt_1()
                    .child(div().w(px(105.0)).child("Blend mode"))
                    .child(div().flex_1().child(self.blend.as_ref().unwrap().1.clone())),
            );
        }
        if !is_null {
            if self.matte.as_ref().is_none_or(|(owner, _, _)| *owner != id) {
                self.matte = Some((
                    id,
                    cx.new(|cx| super::matte::MattePicker::new(self.state.clone(), id, true, cx)),
                    cx.new(|cx| super::matte::MattePicker::new(self.state.clone(), id, false, cx)),
                ));
            }
            let (_, source, mode) = self.matte.as_ref().unwrap();
            for (label, picker) in [
                ("Track Matte", source.clone()),
                ("Matte mode", mode.clone()),
            ] {
                contents = contents.child(
                    div()
                        .flex()
                        .items_center()
                        .mt_1()
                        .child(div().w(px(105.0)).child(label))
                        .child(div().flex_1().min_w_0().child(picker)),
                );
            }
        }
        let is_adjustment = matches!(layer.content(), Content::Adjustment);
        let mut entries = vec![(2, "Fill (hex)", format!("{:06X}", layer.color()))];
        if let Content::Text { text, font_size } = layer.content() {
            entries.insert(0, (0, "Text", text.clone()));
            entries.insert(1, (1, "Font size", font_size.to_string()));
        }
        if matches!(
            layer.content(),
            Content::Image { .. }
                | Content::Video { .. }
                | Content::Composition { .. }
                | Content::Adjustment
        ) {
            entries.retain(|(index, _, _)| *index != 2);
        }
        if is_null {
            entries.clear();
        }
        if matches!(layer.content(), Content::Solid | Content::Adjustment) {
            entries.push((9, "Source width", layer.width().to_string()));
            entries.push((10, "Source height", layer.height().to_string()));
        }
        contents = contents.child(
            div()
                .mt_3()
                .py_2()
                .border_t_1()
                .border_color(rgb(ui::BORDER))
                .child(if is_null {
                    "Null object · transform controller"
                } else if is_adjustment {
                    "Adjustment · composite below"
                } else if matches!(layer.content(), Content::Solid) {
                    "Solid settings"
                } else {
                    "Content"
                }),
        );
        if let Content::Composition { composition, .. } = layer.content() {
            let source = *composition;
            let state = self.state.clone();
            contents = contents.child(
                ui::text_button("open-source-composition", "Open source composition").on_click(
                    move |_, window, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(&Action::ActivateComposition(source), window, cx)
                        })
                    },
                ),
            );
        }
        if layer.can_time_remap() {
            let remapped = layer.time_remap().is_some();
            contents = contents.child(div().mt_2().child("Time")).child(
                ui::text_button(
                    "time-remap-enable",
                    if remapped {
                        "Disable Time Remapping"
                    } else {
                        "Enable Time Remapping"
                    },
                )
                .when(!locked, |b| {
                    b.on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(&Action::ToggleTimeRemap, window, cx)
                            })
                        }
                    })
                }),
            );
            if remapped {
                let value = format!("{:.12}", layer.source_time(frame, comp.fps()).unwrap());
                self.playback[2].update(cx, |f, _| {
                    f.sync(format!("{id}-{frame}"), value.clone(), window)
                });
                contents = contents
                    .child(
                        div()
                            .flex()
                            .h(px(29.0))
                            .items_center()
                            .child(ui::action_tool(
                                "time-remap-key",
                                "diamond",
                                "Add or remove source time key",
                                &self.state,
                                Action::Edit(Command::EditTrack {
                                    id,
                                    property: PropertyPath::TimeRemap,
                                    edit: TrackEdit::ToggleKey { frame },
                                }),
                                layer.time_remap().unwrap().keys().contains_key(&frame),
                            ))
                            .child(div().w(px(88.0)).child("Source time (s)"))
                            .child(
                                div()
                                    .flex_1()
                                    .when(!locked, |d| d.child(self.playback[2].clone()))
                                    .when(locked, |d| d.child(value)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(ui::text_button("time-remap-graph", "Edit graph").on_click({
                                let state = self.state.clone();
                                move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::GraphProperty(id, PropertyPath::TimeRemap),
                                            window,
                                            cx,
                                        )
                                    })
                                }
                            }))
                            .when(!locked, |d| {
                                d.child(
                                    ui::text_button("time-remap-freeze", "Freeze frame").on_click(
                                        {
                                            let state = self.state.clone();
                                            move |_, window, cx| {
                                                state.update(cx, |s, cx| {
                                                    s.dispatch(&Action::FreezeTimeRemap, window, cx)
                                                })
                                            }
                                        },
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child("Source seconds · out of range is transparent"),
                    );
            }
        }
        if let Content::Video {
            path,
            duration,
            playback,
            ..
        } = layer.content()
        {
            contents = contents
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(ui::MUTED))
                        .child(format!("Linked video · {duration:.2}s · no audio")),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .max_h(px(48.0))
                        .overflow_hidden()
                        .child(path.clone()),
                )
                .child(ui::text_button("relink-video", "Relink source…").on_click({
                    let state = self.state.clone();
                    move |_, window, cx| {
                        state.update(cx, |s, cx| s.dispatch(&Action::RelinkVideo, window, cx));
                    }
                }));
            if layer.time_remap().is_none() {
                for (index, label, value) in [
                    (0, "Speed (%)", format!("{:.2}", playback.speed * 100.0)),
                    (
                        1,
                        "Source In (s)",
                        format!(
                            "{:.6}",
                            layer
                                .content()
                                .video_source_time(layer.in_frame(), comp.fps())
                                .unwrap()
                        ),
                    ),
                ] {
                    self.playback[index].update(cx, |field, _| {
                        field.sync(format!("{id}-{}", layer.in_frame()), value.clone(), window);
                    });
                    contents = contents.child(
                        div()
                            .flex()
                            .h(px(29.0))
                            .items_center()
                            .child(div().w(px(105.0)).child(label))
                            .child(
                                div()
                                    .flex_1()
                                    .when(!locked, |s| s.child(self.playback[index].clone()))
                                    .when(locked, |s| s.child(value)),
                            ),
                    );
                }
                contents = contents
                    .child(div().flex().gap_1().when(!locked, |row| {
                        row.child(ui::text_button("reverse-video", "Reverse").on_click({
                            let state = self.state.clone();
                            move |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::ReverseVideo { id }),
                                        window,
                                        cx,
                                    )
                                })
                            }
                        }))
                        .child(
                            ui::text_button("freeze-video", "Freeze at playhead").on_click({
                                let state = self.state.clone();
                                move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::Edit(Command::FreezeVideo {
                                                id,
                                                frame: s.frame,
                                            }),
                                            window,
                                            cx,
                                        )
                                    })
                                }
                            }),
                        )
                    }))
                    .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
                        "0% freezes · negative speed reverses. Layer range and keys stay fixed.",
                    ));
            }
            let source_status =
                if frame < layer.in_frame() || frame >= layer.out_frame(comp.duration()) {
                    "Playhead is outside the layer".to_string()
                } else {
                    layer
                        .video_time(frame, comp.fps())
                        .map(|seconds| format!("Source now: {seconds:.3}s"))
                        .unwrap_or_else(|| "Outside source · transparent frame".into())
                };
            contents = contents.child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(rgb(ui::MUTED))
                    .child(source_status),
            );
        }
        for (index, label, value) in entries {
            self.extra[index].update(cx, |field, _| {
                if index != 0 && index != 2 {
                    field.set_numeric();
                }
                field.sync(id.to_string(), value.clone(), window);
            });
            contents = contents.child(
                div()
                    .flex()
                    .h(px(29.0))
                    .items_center()
                    .child(div().w(px(105.0)).child(label))
                    .child(
                        div()
                            .flex_1()
                            .when(!locked, |s| s.child(self.extra[index].clone()))
                            .when(locked, |s| s.child(value)),
                    ),
            );
        }
        if matches!(layer.content(), Content::Solid | Content::Adjustment) {
            let state = self.state.clone();
            let command = Command::ConfigureSolid {
                id,
                width: comp.width(),
                height: comp.height(),
                color: layer.color(),
            };
            contents = contents
                .child(
                    ui::text_button("solid-comp-size", "Make comp size").when(!locked, |s| {
                        s.on_click(move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(&Action::Edit(command.clone()), window, cx)
                            });
                        })
                    }),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(ui::MUTED))
                        .child("Size edits keep the layer origin and animation."),
                );
        }
        if !is_null {
            let state = self.state.clone();
            contents = contents.child(
                ui::text_button("open-effects", "Open Effect Controls").on_click(
                    move |_, _, cx| {
                        state.update(cx, |s, cx| {
                            s.effect_controls_open = true;
                            cx.notify();
                        });
                    },
                ),
            );
            let mask = if layer.mask().is_some() {
                None
            } else {
                Some(Mask {
                    x: layer.width() * 0.25,
                    y: layer.height() * 0.25,
                    width: layer.width() * 0.5,
                    height: layer.height() * 0.5,
                    inverted: false,
                })
            };
            contents = contents.child(
                div()
                    .mt_3()
                    .border_t_1()
                    .border_color(rgb(ui::BORDER))
                    .child(
                        ui::text_button(
                            "mask-toggle",
                            if layer.mask().is_some() {
                                "Remove rectangle mask"
                            } else {
                                "Add rectangle mask"
                            },
                        )
                        .on_click({
                            let state = self.state.clone();
                            move |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::SetMask { id, mask }),
                                        window,
                                        cx,
                                    )
                                })
                            }
                        }),
                    ),
            );
            if let Some(mut mask) = layer.mask() {
                for (index, label, value) in [
                    (5, "Mask X", mask.x),
                    (6, "Mask Y", mask.y),
                    (7, "Mask Width", mask.width),
                    (8, "Mask Height", mask.height),
                ] {
                    self.extra[index].update(cx, |f, _| {
                        f.set_numeric();
                        f.sync(id.to_string(), format!("{value:.2}"), window);
                    });
                    contents = contents.child(
                        div()
                            .flex()
                            .h(px(29.0))
                            .items_center()
                            .child(div().flex_1().child(label))
                            .child(
                                div()
                                    .w(px(90.0))
                                    .when(!locked, |s| s.child(self.extra[index].clone())),
                            ),
                    );
                }
                mask.inverted = !mask.inverted;
                contents = contents.child(
                    ui::text_button(
                        "invert-mask",
                        if mask.inverted {
                            "Mask: Add"
                        } else {
                            "Mask: Subtract"
                        },
                    )
                    .on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::SetMask {
                                        id,
                                        mask: Some(mask),
                                    }),
                                    window,
                                    cx,
                                )
                            })
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
