use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, LayerId, MaskParam, PropertyPath, TrackEdit};
use std::collections::BTreeMap;

pub(crate) struct MaskValues {
    state: Entity<EditorState>,
    fields: BTreeMap<(LayerId, u64, MaskParam), Entity<TextField>>,
}
impl MaskValues {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            fields: BTreeMap::new(),
        }
    }
}
impl Render for MaskValues {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let Some(layer) = s.editor.selected_layer().cloned() else {
            self.fields.clear();
            return div();
        };
        let (id, frame, locked) = (layer.id(), s.frame, layer.locked());
        self.fields.retain(|(owner, mask, _), _| {
            *owner == id && layer.path_masks().iter().any(|m| m.id == *mask)
        });
        let mut root = div().flex().flex_col().gap_1();
        if !layer.path_masks().is_empty() {
            root = root.child(
                div()
                    .mt_3()
                    .py_2()
                    .border_t_1()
                    .border_color(rgb(ui::BORDER))
                    .child("Masks"),
            );
        }
        for (index, mask) in layer.path_masks().iter().enumerate() {
            root = root.child(super::path_masks::row(&self.state, &layer, index));
            root = root.child(super::path_controls::row(
                &self.state,
                &layer,
                libre_effects_core::PathTarget::Mask(mask.id),
                frame,
            ));
            for parameter in MaskParam::ALL {
                let mask_id = mask.id;
                let path = PropertyPath::Mask {
                    mask: mask_id,
                    parameter,
                };
                let field = self
                    .fields
                    .entry((id, mask_id, parameter))
                    .or_insert_with(|| {
                        let state = self.state.clone();
                        cx.new(|cx| {
                            TextField::new(cx, move |text, w, cx| {
                                state.update(cx, |s, cx| {
                                    if s.editor.selected() != Some(id) {
                                        return;
                                    }
                                    match text.trim().parse::<f64>() {
                                        Ok(value) => s.dispatch(
                                            &Action::Edit(Command::EditTrack {
                                                id,
                                                property: path,
                                                edit: TrackEdit::Value {
                                                    frame: s.frame,
                                                    value,
                                                },
                                            }),
                                            w,
                                            cx,
                                        ),
                                        Err(_) => {
                                            s.status = "Enter a finite mask value".into();
                                            cx.notify();
                                        }
                                    }
                                });
                            })
                            .numeric()
                        })
                    })
                    .clone();
                let value = layer.track_value(path, frame).unwrap();
                field.update(cx, |f, _| {
                    f.sync(
                        format!("{id}-{mask_id}-{frame}"),
                        format!("{value:.2}"),
                        window,
                    )
                });
                let key = |suffix: &str| {
                    SharedString::from(format!("mask-{mask_id}-{parameter:?}-{suffix}"))
                };
                let track = layer.track(path).unwrap();
                root = root.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(27.0))
                        .child(ui::action_tool(
                            key("watch"),
                            "stopwatch",
                            "Toggle mask animation",
                            &self.state,
                            Action::Edit(Command::EditTrack {
                                id,
                                property: path,
                                edit: TrackEdit::ToggleAnimation { frame },
                            }),
                            !track.keys().is_empty(),
                        ))
                        .child(ui::action_tool(
                            key("key"),
                            "diamond",
                            "Add or remove mask key",
                            &self.state,
                            Action::Edit(Command::EditTrack {
                                id,
                                property: path,
                                edit: TrackEdit::ToggleKey { frame },
                            }),
                            track.keys().contains_key(&frame),
                        ))
                        .child(
                            div()
                                .w(px(88.0))
                                .text_size(px(11.0))
                                .child(parameter.label().trim_start_matches("Mask ")),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .when(!locked, |d| d.child(field))
                                .when(locked, |d| d.child(format!("{value:.2}"))),
                        )
                        .child(ui::action_tool(
                            key("graph"),
                            "chart-line",
                            "Edit mask value graph",
                            &self.state,
                            Action::GraphProperty(id, path),
                            false,
                        )),
                );
            }
        }
        root
    }
}
