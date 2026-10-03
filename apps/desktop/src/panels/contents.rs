use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::{
    Command, Content, ContentsEdit, ContentsKind, ContentsParam, PathTarget, PropertyPath,
    ShapeKind, ShapeStroke, StrokeCap, StrokeJoin, TrackEdit,
};

pub(crate) struct ContentsControls {
    state: Entity<EditorState>,
    owner: Option<u64>,
    selected: Option<u64>,
    fields: Vec<(ContentsParam, Entity<TextField>)>,
    name: Option<Entity<TextField>>,
    add_open: bool,
    collapsed: std::collections::BTreeSet<u64>,
}
impl ContentsControls {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            owner: None,
            selected: None,
            fields: vec![],
            name: None,
            add_open: false,
            collapsed: Default::default(),
        }
    }
    fn select(&mut self, layer: u64, item: u64, cx: &mut Context<Self>) {
        self.owner = Some(layer);
        self.selected = Some(item);
        self.add_open = false;
        let params = self
            .state
            .read(cx)
            .editor
            .selected_layer()
            .and_then(|l| match l.content() {
                Content::ShapeContents(c) => c
                    .node(item)
                    .map(|n| n.parameters.keys().copied().collect::<Vec<_>>()),
                _ => None,
            })
            .unwrap_or_default();
        self.fields = params
            .into_iter()
            .map(|p| {
                let state = self.state.clone();
                (
                    p,
                    cx.new(|cx| {
                        TextField::new(cx, move |text, w, cx| {
                            state.update(cx, |s, cx| {
                                if s.editor.selected() != Some(layer) {
                                    return;
                                }
                                match text.trim().parse::<f64>() {
                                    Ok(value) => s.dispatch(
                                        &Action::Edit(Command::Contents {
                                            id: layer,
                                            edit: ContentsEdit::Track {
                                                item,
                                                parameter: p,
                                                edit: TrackEdit::Value {
                                                    frame: s.frame,
                                                    value,
                                                },
                                            },
                                        }),
                                        w,
                                        cx,
                                    ),
                                    Err(_) => {
                                        s.status = "Enter a finite Contents value".into();
                                        cx.notify();
                                    }
                                }
                            })
                        })
                    }),
                )
            })
            .collect();
        let state = self.state.clone();
        self.name = Some(cx.new(|cx| {
            TextField::new(cx, move |text, w, cx| {
                state.update(cx, |s, cx| {
                    if s.editor.selected() == Some(layer) {
                        s.dispatch(
                            &Action::Edit(Command::Contents {
                                id: layer,
                                edit: ContentsEdit::Rename {
                                    item,
                                    name: text.into(),
                                },
                            }),
                            w,
                            cx,
                        );
                    }
                })
            })
        }));
        cx.notify();
    }
}
impl Render for ContentsControls {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().flex().flex_col().gap_1();
        let Some(layer) = self.state.read(cx).editor.selected_layer().cloned() else {
            return root;
        };
        let id = layer.id();
        let locked = layer.locked();
        let frame = self.state.read(cx).frame;
        if matches!(layer.content(), Content::Shape(_)) {
            let state = self.state.clone();
            return root.child(ui::text_button("organize-contents","Create Contents Group")
                .tooltip(|_,cx|cx.new(|_|ui::Tip("Organize this shape into separate Path, Stroke and Fill items. Existing animation is retained. Undo restores the original.".into())).into())
                .when(!locked,|b|b.on_click(move|_,w,cx|state.update(cx,|s,cx|s.dispatch(&Action::Edit(Command::Contents{id,edit:ContentsEdit::Promote}),w,cx)))));
        }
        let Content::ShapeContents(contents) = layer.content() else {
            return root;
        };
        if self.owner != Some(id) || self.selected.is_some_and(|n| contents.node(n).is_none()) {
            self.owner = Some(id);
            self.selected = None;
            self.fields.clear();
            self.name = None;
            self.add_open = false;
            self.collapsed.clear();
        }
        let parent = self
            .selected
            .and_then(|item| {
                contents
                    .rows()
                    .into_iter()
                    .find(|(_, _, n)| n.id == item)
                    .map(|(_, p, n)| {
                        if matches!(n.kind, ContentsKind::Group(_)) {
                            item
                        } else {
                            p
                        }
                    })
            })
            .unwrap_or(0);
        root = root.child(ui::text_button("contents-add", "Add ▾").when(!locked, |b| {
            b.on_click(cx.listener(|this, _, _, cx| {
                this.add_open = !this.add_open;
                cx.notify();
            }))
        }));
        if self.add_open {
            for (index, kind) in [
                ContentsKind::Group(vec![]),
                ContentsKind::Parametric(ShapeKind::Rectangle),
                ContentsKind::Parametric(ShapeKind::RoundedRectangle),
                ContentsKind::Parametric(ShapeKind::Ellipse),
                ContentsKind::Parametric(ShapeKind::Polygon),
                ContentsKind::Parametric(ShapeKind::Star),
                ContentsKind::Fill { even_odd: false },
                ContentsKind::Stroke(Default::default()),
            ]
            .into_iter()
            .enumerate()
            {
                let state = self.state.clone();
                let label = kind.label();
                root = root.child(
                    ui::text_button(("contents-add-kind", index), label).on_click(cx.listener(
                        move |this, _, w, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::Contents {
                                        id,
                                        edit: ContentsEdit::Add {
                                            parent,
                                            kind: kind.clone(),
                                        },
                                    }),
                                    w,
                                    cx,
                                )
                            });
                            this.add_open = false;
                            cx.notify();
                        },
                    )),
                );
            }
        }
        let mut hidden_depth = None;
        for (depth, _, node) in contents.rows() {
            if hidden_depth.is_some_and(|d| depth > d) {
                continue;
            }
            hidden_depth = None;
            let is_group = matches!(node.kind, ContentsKind::Group(_));
            let collapsed = self.collapsed.contains(&node.id);
            if is_group && collapsed {
                hidden_depth = Some(depth);
            }
            let item = node.id;
            let state = self.state.clone();
            let enabled = node.enabled;
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .pl(px(depth as f32 * 10.))
                    .h(px(26.))
                    .when(is_group, |d| {
                        d.child(
                            ui::tool(
                                ("contents-expand", item),
                                if collapsed {
                                    "chevron-right"
                                } else {
                                    "chevron-down"
                                },
                                "Expand or collapse group",
                                false,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if !this.collapsed.remove(&item) {
                                        this.collapsed.insert(item);
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                    })
                    .child(
                        ui::tool(
                            ("contents-visible", item),
                            "eye",
                            "Toggle item visibility",
                            enabled,
                        )
                        .when(!locked, |b| {
                            b.on_click(move |_, w, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Contents {
                                            id,
                                            edit: ContentsEdit::Enabled {
                                                item,
                                                enabled: !enabled,
                                            },
                                        }),
                                        w,
                                        cx,
                                    )
                                })
                            })
                        }),
                    )
                    .child(
                        ui::text_button(("contents-item", item), node.name.clone())
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .when(self.selected == Some(item), |b| b.bg(rgb(0x164a7b)))
                            .on_click(cx.listener(move |this, _, _, cx| this.select(id, item, cx))),
                    ),
            );
        }
        let Some(item) = self.selected else {
            return root.child(
                div()
                    .text_size(px(11.))
                    .child("Select a Contents item to edit"),
            );
        };
        let Some(node) = contents.node(item) else {
            return root;
        };
        if !self
            .fields
            .iter()
            .map(|(p, _)| p)
            .eq(node.parameters.keys())
        {
            self.select(id, item, cx);
        }
        if let Some(name) = &self.name {
            name.update(cx, |f, _| {
                f.sync(format!("contents-name-{id}-{item}"), node.name.clone(), w)
            });
            root = root.child(
                div()
                    .mt_2()
                    .when(!locked, |d| d.child(name.clone()))
                    .when(locked, |d| d.child(node.name.clone())),
            );
        }
        let (_, parent, _) = contents
            .rows()
            .into_iter()
            .find(|(_, _, n)| n.id == item)
            .unwrap();
        let siblings = if parent == 0 {
            &contents.items
        } else {
            let ContentsKind::Group(v) = &contents.node(parent).unwrap().kind else {
                unreachable!()
            };
            v
        };
        let index = siblings.iter().position(|n| n.id == item).unwrap();
        let mut actions = div().flex().gap_1();
        for (key, icon, label, edit, allowed) in [
            (
                0usize,
                "arrow-up",
                "Move earlier",
                ContentsEdit::Move {
                    item,
                    parent,
                    index: index.saturating_sub(1),
                },
                index > 0,
            ),
            (
                1,
                "arrow-down",
                "Move later",
                ContentsEdit::Move {
                    item,
                    parent,
                    index: index + 1,
                },
                index + 1 < siblings.len(),
            ),
            (
                2,
                "copy",
                "Duplicate item",
                ContentsEdit::Duplicate(item),
                true,
            ),
            (
                3,
                "trash-bin",
                "Delete item",
                ContentsEdit::Remove(item),
                true,
            ),
        ] {
            let state = self.state.clone();
            actions = actions.child(ui::tool(("contents-action", key), icon, label, false).when(
                !locked && allowed,
                |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::Contents {
                                    id,
                                    edit: edit.clone(),
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                },
            ));
        }
        root = root.child(actions);
        let mut parenting = div().flex().flex_col().gap_1();
        if parent != 0 {
            let grand = contents
                .rows()
                .into_iter()
                .find(|(_, _, n)| n.id == parent)
                .map(|(_, p, _)| p)
                .unwrap();
            let group = if grand == 0 {
                &contents.items
            } else {
                let ContentsKind::Group(v) = &contents.node(grand).unwrap().kind else {
                    unreachable!()
                };
                v
            };
            let to = group.iter().position(|n| n.id == parent).unwrap() + 1;
            let state = self.state.clone();
            parenting = parenting.child(
                ui::text_button("contents-outdent", "Move out of group").when(!locked, |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::Contents {
                                    id,
                                    edit: ContentsEdit::Move {
                                        item,
                                        parent: grand,
                                        index: to,
                                    },
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                }),
            );
        }
        if index > 0
            && let ContentsKind::Group(v) = &siblings[index - 1].kind
        {
            let target = siblings[index - 1].id;
            let to = v.len();
            let state = self.state.clone();
            parenting = parenting.child(
                ui::text_button("contents-indent", "Move into group above").when(!locked, |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::Contents {
                                    id,
                                    edit: ContentsEdit::Move {
                                        item,
                                        parent: target,
                                        index: to,
                                    },
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                }),
            );
        }
        root = root.child(parenting);
        if matches!(node.kind, ContentsKind::Parametric(_)) {
            let state = self.state.clone();
            root = root.child(
                ui::text_button("contents-to-path", "Convert To Bezier Path").when(!locked, |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::Contents {
                                    id,
                                    edit: ContentsEdit::ConvertPath {
                                        item,
                                        frame: s.frame,
                                    },
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                }),
            );
        }
        if matches!(node.kind, ContentsKind::Path { .. }) {
            root = root.child(super::path_controls::row(
                &self.state,
                &layer,
                PathTarget::Contents(item),
                frame,
            ));
        }
        if let ContentsKind::Fill { even_odd } = node.kind {
            let state = self.state.clone();
            root = root.child(
                ui::text_button(
                    "contents-fill-rule",
                    if even_odd {
                        "Fill rule: Even-Odd"
                    } else {
                        "Fill rule: Non-Zero"
                    },
                )
                .when(!locked, |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::Contents {
                                    id,
                                    edit: ContentsEdit::FillRule {
                                        item,
                                        even_odd: !even_odd,
                                    },
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                }),
            );
        }
        if let Some(color) = node.paint_color_at(frame) {
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child("Color")
                    .child(super::color_picker::swatch(
                        "contents-paint-color",
                        color,
                        crate::color_edit::Target::Contents(id, item),
                        locked,
                        &self.state,
                    )),
            );
        }
        if let ContentsKind::Stroke(style) = &node.kind {
            for (cap_row, label) in [(true, "Line Cap"), (false, "Line Join")] {
                let mut options = div().flex().gap_1();
                for index in 0..3 {
                    let (name, active, edit) = if cap_row {
                        let cap = StrokeCap::ALL[index];
                        (
                            cap.label(),
                            style.cap == cap,
                            ContentsEdit::StrokeCap { item, cap },
                        )
                    } else {
                        let join = StrokeJoin::ALL[index];
                        (
                            join.label(),
                            style.join == join,
                            ContentsEdit::StrokeJoin { item, join },
                        )
                    };
                    let state = self.state.clone();
                    options = options.child(
                        ui::text_button(
                            (
                                if cap_row {
                                    "contents-cap"
                                } else {
                                    "contents-join"
                                },
                                index,
                            ),
                            name,
                        )
                        .when(active, |b| b.bg(rgb(0x164a7b)))
                        .when(!locked, |b| {
                            b.on_click(move |_, w, cx| {
                                TextField::commit_active(w, cx);
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Contents {
                                            id,
                                            edit: edit.clone(),
                                        }),
                                        w,
                                        cx,
                                    )
                                });
                            })
                        }),
                    );
                }
                root = root.child(div().flex().flex_col().gap_1().child(label).child(options));
            }
            let mut dashes = div().flex().items_center().gap_1().child("Dashes");
            for (key, icon, label, edit, enabled) in [
                (
                    0usize,
                    "plus",
                    "Add dash or gap",
                    ContentsEdit::AddDash(item),
                    style.dashes.len() < ShapeStroke::MAX_DASHES,
                ),
                (
                    1,
                    "minus",
                    "Remove last dash or gap",
                    ContentsEdit::RemoveDash(item),
                    !style.dashes.is_empty(),
                ),
            ] {
                let state = self.state.clone();
                dashes = dashes.child(
                    ui::tool(("contents-dashes", key), icon, label, false)
                        .when(locked || !enabled, |b| b.opacity(0.4))
                        .when(!locked && enabled, |b| {
                            b.on_click(move |_, w, cx| {
                                TextField::commit_active(w, cx);
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Contents {
                                            id,
                                            edit: edit.clone(),
                                        }),
                                        w,
                                        cx,
                                    )
                                });
                            })
                        }),
                );
            }
            root = root.child(dashes);
        }
        for (p, field) in &self.fields {
            let Some(track) = node.parameters.get(p) else {
                continue;
            };
            let property = PropertyPath::Contents {
                item,
                parameter: *p,
            };
            let value = node.value_at(*p, frame).to_string();
            field.update(cx, |f, _| {
                f.sync(
                    format!("contents-{id}-{item}-{p:?}-{frame}"),
                    value.clone(),
                    w,
                )
            });
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(27.))
                    .child(ui::action_tool(
                        gpui::SharedString::from(format!("contents-watch-{p:?}")),
                        "stopwatch",
                        "Toggle animation",
                        &self.state,
                        Action::Edit(Command::EditTrack {
                            id,
                            property,
                            edit: TrackEdit::ToggleAnimation { frame },
                        }),
                        !track.keys().is_empty(),
                    ))
                    .child(
                        ui::text_button(
                            gpui::SharedString::from(format!("contents-label-{p:?}")),
                            p.label(),
                        )
                        .flex_1()
                        .min_w_0()
                        .justify_start()
                        .on_click({
                            let state = self.state.clone();
                            move |_, w, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(&Action::GraphProperty(id, property), w, cx)
                                })
                            }
                        }),
                    )
                    .child(
                        div()
                            .w(px(72.))
                            .when(!locked, |d| d.child(field.clone()))
                            .when(locked, |d| d.child(value)),
                    ),
            );
        }
        root
    }
}
