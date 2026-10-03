use crate::editor::presets::PresetAction;
use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{
    Command, Content, EffectEdit, EffectId, EffectKind, EffectParam, Effects, LayerId,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct EffectControls {
    curves: BTreeMap<(LayerId, EffectId), Entity<super::color_curve::ColorCurve>>,
    state: Entity<EditorState>,
    fields: BTreeMap<(LayerId, EffectId, EffectParam), Entity<TextField>>,
    names: BTreeMap<(LayerId, EffectId), Entity<TextField>>,
}
impl EffectControls {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            fields: BTreeMap::new(),
            curves: BTreeMap::new(),
            names: BTreeMap::new(),
        }
    }
}
impl Render for EffectControls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let layer = state.editor.selected_layer().cloned();
        let frame = state.frame;
        let composition = state.editor.project().active_composition_id();
        let gradient_controls = state.gradient_controls;
        let mut body = div()
            .id("effect-controls-scroll")
            .size_full()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(11.0));
        let Some(layer) = layer else {
            self.curves.clear();
            self.fields.clear();
            self.names.clear();
            return body.child("Select a layer to edit effects");
        };
        if matches!(layer.content(), Content::Null) {
            return body.child("Null objects have no rendered pixels.");
        }
        self.curves.retain(|(owner, effect), _| {
            *owner == layer.id()
                && layer
                    .effect_stack()
                    .iter()
                    .any(|e| e.id() == *effect && e.kind() == EffectKind::Curves)
        });
        let id = layer.id();
        let locked = layer.locked();
        let state_entity = self.state.clone();
        let tool = |name: String, icon, label, edit: EffectEdit, active| {
            ui::action_tool(
                SharedString::from(name),
                icon,
                label,
                &state_entity,
                Action::Edit(Command::Effect { id, edit }),
                active,
            )
            .w(px(22.0))
            .h(px(22.0))
            .when(locked, |s| s.opacity(0.4))
        };
        body = body.child(
            div()
                .py_1()
                .text_color(rgb(ui::BLUE))
                .child(layer.name().to_string()),
        );
        if !layer.effect_stack().is_empty() || layer.effects() != Effects::default() {
            body = body.child(preset_button(
                "save-effect-stack",
                "Save effect preset…",
                &self.state,
                PresetAction::Save(None),
            ));
        }
        if locked {
            body = body.child("Unlock this layer to edit its effects.");
        }
        if layer.effects() != Effects::default() {
            let state = self.state.clone();
            body = body.child(
                div()
                    .py_2()
                    .child(format!("Existing effects: Blur {:.2} · Brightness {:.2} · Grayscale {}. Applied before this stack.",layer.effects().blur,layer.effects().brightness,if layer.effects().grayscale {"On"}else{"Off"}))
                    .child(
                        ui::text_button("convert-effects", "Edit as ordered effects")
                            .on_click(move |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Effect {
                                            id,
                                            edit: EffectEdit::ConvertLegacy,
                                        }),
                                        window,
                                        cx,
                                    )
                                });
                            }),
                    ),
            );
        }
        let mut keep_fields = BTreeSet::new();
        let mut keep_names = BTreeSet::new();
        for (index, effect) in layer.effect_stack().iter().enumerate() {
            let effect_id = effect.id();
            let key = (id, effect_id);
            keep_names.insert(key);
            if !self.names.contains_key(&key) {
                let state = self.state.clone();
                self.names.insert(
                    key,
                    cx.new(|cx| {
                        TextField::new(cx, move |text, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::Effect {
                                        id,
                                        edit: EffectEdit::Rename {
                                            effect: effect_id,
                                            name: text.into(),
                                        },
                                    }),
                                    window,
                                    cx,
                                )
                            });
                        })
                    }),
                );
            }
            let name = self.names[&key].clone();
            name.update(cx, |f, _| {
                f.sync(
                    format!("{id}-{effect_id}"),
                    effect.name().to_string(),
                    window,
                )
            });
            let prefix = format!("effect-{id}-{effect_id}");
            let mut section = div()
                .border_t_1()
                .border_color(rgb(ui::BORDER))
                .py_2()
                .flex()
                .flex_col()
                .gap_1();
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(tool(
                        format!("{prefix}-enabled"),
                        "eye",
                        "Enable / bypass effect",
                        EffectEdit::Bypass {
                            effect: effect_id,
                            bypassed: !effect.bypassed(),
                        },
                        !effect.bypassed(),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(!locked, |s| s.child(name))
                            .when(locked, |s| s.child(effect.name().to_string())),
                    ),
            );
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(ui::MUTED))
                            .child(effect.kind().label()),
                    )
                    .child(tool(
                        format!("{prefix}-up"),
                        "arrow-up",
                        "Move effect up",
                        EffectEdit::Move {
                            effect: effect_id,
                            index: index.saturating_sub(1),
                        },
                        false,
                    ))
                    .child(tool(
                        format!("{prefix}-down"),
                        "arrow-down",
                        "Move effect down",
                        EffectEdit::Move {
                            effect: effect_id,
                            index: (index + 1).min(layer.effect_stack().len() - 1),
                        },
                        false,
                    ))
                    .child(tool(
                        format!("{prefix}-copy"),
                        "copy",
                        "Duplicate effect",
                        EffectEdit::Duplicate(effect_id),
                        false,
                    ))
                    .child(tool(
                        format!("{prefix}-reset"),
                        "arrow-rotate-left",
                        "Reset effect and remove its keys",
                        EffectEdit::Reset(effect_id),
                        false,
                    ))
                    .child(tool(
                        format!("{prefix}-remove"),
                        "trash-bin",
                        "Remove effect",
                        EffectEdit::Remove(effect_id),
                        false,
                    )),
            );
            section = section.child(preset_button(
                SharedString::from(format!("{prefix}-save-preset")),
                "Save this effect…",
                &self.state,
                PresetAction::Save(Some(effect_id)),
            ));
            if matches!(
                effect.kind(),
                EffectKind::LinearGradient | EffectKind::RadialGradient
            ) {
                let target = crate::color_edit::GradientTarget::Effect(composition, id, effect_id);
                let active = gradient_controls == Some(target);
                let state = self.state.clone();
                section = section.child(
                    ui::text_button(
                        SharedString::from(format!("{prefix}-points")),
                        "Edit gradient in Composition",
                    )
                    .when(active, |b| b.bg(rgb(0x164a7b)))
                    .when(locked || effect.bypassed(), |b| b.opacity(0.4))
                    .on_click(move |_, window, cx| {
                        TextField::commit_active(window, cx);
                        state.update(cx, |s, cx| {
                            if s.editor.project().composition().layer(id).is_some_and(|l| {
                                !l.locked()
                                    && l.effect_stack()
                                        .iter()
                                        .any(|e| e.id() == effect_id && !e.bypassed())
                            }) {
                                s.dispatch(&Action::Seek(s.frame), window, cx);
                                s.tool = crate::editor::Tool::Select;
                                s.gradient_controls = if s.gradient_controls == Some(target) {
                                    None
                                } else {
                                    Some(target)
                                };
                                cx.notify();
                            }
                        });
                    }),
                );
            }
            let mut shown_curve = None;
            if effect.kind() == EffectKind::Curves {
                if !self.curves.contains_key(&key) {
                    let curve = cx.new(|cx| {
                        super::color_curve::ColorCurve::new(self.state.clone(), id, effect_id, cx)
                    });
                    cx.observe(&curve, |_, _, cx| cx.notify()).detach();
                    self.curves.insert(key, curve);
                }
                let curve = self.curves[&key].clone();
                shown_curve = Some(curve.read(cx).channel.parameters());
                section = section.child(curve);
            }
            for spec in effect
                .kind()
                .parameters()
                .into_iter()
                .filter(|s| shown_curve.is_none_or(|params| params.contains(&s.parameter)))
            {
                let parameter = spec.parameter;
                let key = (id, effect_id, parameter);
                keep_fields.insert(key);
                if !self.fields.contains_key(&key) {
                    let state = self.state.clone();
                    self.fields.insert(
                        key,
                        cx.new(|cx| {
                            TextField::new(cx, move |text, window, cx| {
                                state.update(cx, |s, cx| match text.trim().parse::<f64>() {
                                    Ok(value) => s.dispatch(
                                        &Action::Edit(Command::Effect {
                                            id,
                                            edit: EffectEdit::SetValue {
                                                effect: effect_id,
                                                parameter,
                                                frame: s.frame,
                                                value,
                                            },
                                        }),
                                        window,
                                        cx,
                                    ),
                                    Err(_) => {
                                        s.status = "Enter a finite effect parameter value".into();
                                        cx.notify();
                                    }
                                });
                            })
                            .numeric()
                        }),
                    );
                }
                let field = self.fields[&key].clone();
                let value = effect.value_at(parameter, frame);
                field.update(cx, |f, _| {
                    f.sync(
                        format!("{id}-{effect_id}-{frame}"),
                        format!("{value:.2}"),
                        window,
                    )
                });
                let track = effect.parameter(parameter).unwrap();
                let animated = !track.keys().is_empty();
                let keyed = track.keys().contains_key(&frame);
                let param_prefix = format!("{prefix}-{parameter:?}");
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(tool(
                            format!("{param_prefix}-watch"),
                            "stopwatch",
                            "Enable animation / remove all parameter keys",
                            EffectEdit::ToggleAnimation {
                                effect: effect_id,
                                parameter,
                                frame,
                            },
                            animated,
                        ))
                        .child(
                            ui::text_button(
                                SharedString::from(format!("{param_prefix}-graph")),
                                spec.label,
                            )
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .on_click({
                                let state = self.state.clone();
                                move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::GraphProperty(
                                                id,
                                                libre_effects_core::PropertyPath::Effect {
                                                    effect: effect_id,
                                                    parameter,
                                                },
                                            ),
                                            window,
                                            cx,
                                        )
                                    })
                                }
                            }),
                        )
                        .child(
                            div()
                                .w(px(63.0))
                                .when(!locked, |s| s.child(field))
                                .when(locked, |s| s.child(format!("{value:.2}"))),
                        )
                        .child(tool(
                            format!("{param_prefix}-key"),
                            "diamond",
                            "Add / remove key at playhead",
                            EffectEdit::ToggleKey {
                                effect: effect_id,
                                parameter,
                                frame,
                            },
                            keyed,
                        )),
                );
                if animated {
                    let mut keys = div().flex().items_center().gap_1().pl(px(24.0));
                    for (label, icon, at) in [
                        (
                            "Previous parameter key",
                            "arrow-left",
                            track.keys().range(..frame).next_back().map(|(f, _)| *f),
                        ),
                        (
                            "Next parameter key",
                            "arrow-right",
                            track
                                .keys()
                                .range(frame.saturating_add(1)..)
                                .next()
                                .map(|(f, _)| *f),
                        ),
                    ] {
                        if let Some(at) = at {
                            keys = keys.child(
                                ui::action_tool(
                                    SharedString::from(format!("{param_prefix}-{icon}")),
                                    icon,
                                    label,
                                    &self.state,
                                    Action::Seek(at),
                                    false,
                                )
                                .w(px(22.0)),
                            );
                        }
                    }
                    keys = keys.child(
                        div()
                            .flex_1()
                            .text_color(rgb(ui::MUTED))
                            .child(format!("{} keys", track.keys().len())),
                    );
                    if let Some(key) = track.keys().get(&frame) {
                        let state = self.state.clone();
                        let interpolation = key.interpolation.next();
                        keys = keys.child(
                            ui::text_button(
                                SharedString::from(format!("{param_prefix}-interpolation")),
                                key.interpolation.label(),
                            )
                            .on_click(move |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Effect {
                                            id,
                                            edit: EffectEdit::Interpolate {
                                                effect: effect_id,
                                                parameter,
                                                frame,
                                                interpolation,
                                            },
                                        }),
                                        window,
                                        cx,
                                    )
                                });
                            }),
                        );
                    }
                    section = section.child(keys);
                }
            }
            body = body.child(section);
        }
        self.fields.retain(|k, _| keep_fields.contains(k));
        self.names.retain(|k, _| keep_names.contains(k));
        if layer.effect_stack().is_empty() {
            body = body.child("Add an effect from Effects & Presets.");
        }
        body
    }
}

pub(crate) struct EffectCatalog {
    state: Entity<EditorState>,
    search: Entity<TextField>,
    collapsed: BTreeSet<&'static str>,
}
impl EffectCatalog {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let search = cx.new(|cx| TextField::new(cx, |_, _, _| {}));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            search,
            collapsed: [
                "Blur & Sharpen",
                "Color Correction",
                "Generate",
                "Perspective",
                "Stylize",
            ]
            .into_iter()
            .collect(),
        }
    }
}
impl Render for EffectCatalog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self
            .state
            .read(cx)
            .editor
            .selected_layer()
            .filter(|l| !l.locked() && !matches!(l.content(), Content::Null))
            .map(|l| l.id());
        let query = self.search.read(cx).value().to_lowercase();
        let mut list = div()
            .id("effects-catalog")
            .p_2()
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().flex_none().child(self.search.clone()));
        if selected.is_none() {
            list = list.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child("Select an unlocked image, text, shape or composition layer."),
            );
        }
        list = list.child(
            div()
                .flex()
                .gap_1()
                .child(preset_button(
                    "import-effect-preset",
                    "Import preset…",
                    &self.state,
                    PresetAction::Import,
                ))
                .child(preset_button(
                    "refresh-effect-presets",
                    "Refresh",
                    &self.state,
                    PresetAction::Reload,
                )),
        );
        let library = &self.state.read(cx).presets;
        if library.busy {
            list = list.child("Loading presets…");
        }
        if !library.message.is_empty() {
            list = list.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(0xffaa88))
                    .child(library.message.clone()),
            );
        }
        let entries: Vec<_> = library
            .entries
            .iter()
            .filter(|e| {
                query.is_empty()
                    || e.preset.name().to_lowercase().contains(&query)
                    || e.preset
                        .effects()
                        .iter()
                        .any(|fx| fx.kind().label().to_lowercase().contains(&query))
            })
            .cloned()
            .collect();
        let mut count = entries.len();
        if !entries.is_empty() {
            list = list.child(div().py_1().child("User Presets"));
            for (i, entry) in entries.into_iter().enumerate() {
                let label = entry.preset.name().to_string();
                let detail = format!(
                    "{} effects · {} keys · {} fps. First key starts at the playhead. Appends to selected layers in one Undo.",
                    entry.preset.effects().len(),
                    entry.preset.key_count(),
                    entry.preset.fps()
                );
                list = list.child(
                    preset_button(
                        SharedString::from(format!("user-preset-{i}")),
                        label,
                        &self.state,
                        PresetAction::Apply(entry.id),
                    )
                    .justify_start()
                    .h(px(24.0))
                    .flex_none()
                    .ml_3()
                    .when(selected.is_none(), |d| d.opacity(0.5))
                    .tooltip(move |_, cx| cx.new(|_| ui::Tip(detail.clone().into())).into()),
                );
            }
        }

        for (category, kinds) in [
            ("Blur & Sharpen", vec![EffectKind::GaussianBlur]),
            (
                "Color Correction",
                vec![
                    EffectKind::Brightness,
                    EffectKind::Grayscale,
                    EffectKind::Tint,
                    EffectKind::HueSaturation,
                    EffectKind::Levels,
                    EffectKind::Curves,
                ],
            ),
            (
                "Generate",
                vec![
                    EffectKind::Fill,
                    EffectKind::LinearGradient,
                    EffectKind::RadialGradient,
                ],
            ),
            ("Perspective", vec![EffectKind::DropShadow]),
            ("Stylize", vec![EffectKind::Glow]),
        ] {
            let kinds: Vec<_> = kinds
                .into_iter()
                .filter(|k| {
                    query.is_empty()
                        || category.to_lowercase().contains(&query)
                        || k.label().to_lowercase().contains(&query)
                })
                .collect();
            if kinds.is_empty() {
                continue;
            }
            count += kinds.len();
            let expanded = !query.is_empty() || !self.collapsed.contains(category);
            list = list.child(
                ui::text_button(
                    SharedString::from(format!("effect-group-{category}")),
                    format!("{}  {category}", if expanded { "▾" } else { "▸" }),
                )
                .h(px(23.0))
                .flex_none()
                .justify_start()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.collapsed.remove(category) {
                        this.collapsed.insert(category);
                    }
                    cx.notify();
                })),
            );
            if !expanded {
                continue;
            }
            for kind in kinds {
                let state = self.state.clone();
                list = list.child(
                    ui::text_button(
                        SharedString::from(format!("add-{kind:?}")),
                        format!("ƒx  {}", kind.label()),
                    )
                    .h(px(23.0))
                    .flex_none()
                    .ml_3()
                    .justify_start()
                    .when(selected.is_none(), |s| s.opacity(0.5))
                    .on_click(move |_, window, cx| {
                        if let Some(id) = selected {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::Effect {
                                        id,
                                        edit: EffectEdit::Add(kind),
                                    }),
                                    window,
                                    cx,
                                );
                                s.effect_controls_open = true;
                                cx.notify();
                            });
                        }
                    }),
                );
            }
        }
        if count == 0 {
            list = list.child(
                div()
                    .p_2()
                    .text_color(rgb(ui::MUTED))
                    .child("No matching effects"),
            );
        }
        list
    }
}

fn preset_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    state: &Entity<EditorState>,
    action: PresetAction,
) -> gpui::Stateful<gpui::Div> {
    let state = state.clone();
    ui::text_button(id, label)
        .on_click(move |_, w, cx| {
            TextField::commit_active(w, cx);
            state.update(cx, |s, cx| {
                s.dispatch(&Action::Preset(action.clone()), w, cx)
            });
        })
        .on_key_down(|e, _, cx| {
            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
            }
        })
}
