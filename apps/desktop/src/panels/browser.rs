mod browsing;
mod usage;

use crate::{
    components::TextField,
    editor::{Action, EditorState},
    project_browser::{self, ItemType, Row},
    ui,
};
use gpui::{
    Context, Entity, FocusHandle, MouseButton, SharedString, Window, div, img, prelude::*, px, rgb,
};
use libre_effects_core::{Command, Content, FolderId, Project, ProjectItem};
use std::{collections::BTreeSet, sync::Arc};

fn item_name(item: ProjectItem, name: String) -> gpui::Stateful<gpui::Div> {
    let name = SharedString::from(name);
    div()
        .id(SharedString::from(format!("project-name-{item:?}")))
        .flex_1()
        .min_w_0()
        // GPUI 0.2.2 caches text measurements by wrap width, not truncation width.
        // `truncate()` sets nowrap, reusing the intrinsic-width line after flex
        // shrink and hiding its ellipsis. Keep width-sensitive measurement, then
        // clamp to one line so the final available width gets a visible ellipsis.
        .whitespace_normal()
        .text_ellipsis()
        .line_clamp(1)
        // Keep the complete name available without changing the model or row actions.
        .tooltip({
            let name = name.clone();
            move |_, cx| cx.new(|_| ui::Tip(name.clone())).into()
        })
        .child(name)
}

pub(crate) struct Browser {
    state: Entity<EditorState>,
    search: Entity<TextField>,
    focus: FocusHandle,
    item_type: ItemType,
    filter_context: Option<u64>,
    row_scroll: gpui::ScrollHandle,
    name: Entity<TextField>,
    effects: Entity<super::effects::EffectControls>,
    collapsed: BTreeSet<FolderId>,
    by_type: bool,
    descending: bool,
    move_open: bool,
    details_open: bool,
    filters_open: bool,
    menu_position: Option<gpui::Point<gpui::Pixels>>,
    type_width: f32,
    column_drag: Option<(f32, f32)>,
    interpretation_open: bool,
    usage_open: Option<ProjectItem>,
    usage_cache: Option<usage::Cache>,
    interpretation: Option<(u64, Entity<super::footage_interpretation::Interpretation>)>,
    thumbnail: Option<Arc<gpui::RenderImage>>,
    retired_images: super::image_retirement::ImageRetirement,
    thumbnail_key: Option<(ProjectItem, Project, u64)>,
    thumbnail_pending: bool,
    thumbnail_error: Option<String>,
}
impl Browser {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle().tab_index(0).tab_stop(true);
        let search = cx.new(|cx| {
            TextField::new(cx, |_, _, _| {})
                .tab_stop()
                .return_focus(focus.clone())
        });
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let edit = state.clone();
        let name = cx.new(|cx| {
            TextField::new(cx, move |value, window, cx| {
                edit.update(cx, |s, cx| {
                    if let Some(item) = s.project_item {
                        s.dispatch(
                            &Action::Edit(Command::RenameProjectItem {
                                item,
                                name: value.into(),
                            }),
                            window,
                            cx,
                        );
                    }
                });
            })
            .tab_stop()
        });
        let effects = cx.new(|cx| super::effects::EffectControls::new(state.clone(), cx));
        Self {
            state,
            search,
            focus,
            item_type: ItemType::All,
            filter_context: None,
            row_scroll: gpui::ScrollHandle::new(),
            name,
            effects,
            collapsed: BTreeSet::new(),
            by_type: false,
            descending: false,
            move_open: false,
            details_open: false,
            filters_open: false,
            menu_position: None,
            type_width: 58.0,
            column_drag: None,
            interpretation_open: false,
            usage_open: None,
            usage_cache: None,
            interpretation: None,
            thumbnail: None,
            retired_images: Default::default(),
            thumbnail_key: None,
            thumbnail_pending: false,
            thumbnail_error: None,
        }
    }
    fn load_thumbnail(
        &mut self,
        project: &Project,
        item: ProjectItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = (item, project.clone(), self.state.read(cx).preview_revision);
        if self.thumbnail_key.as_ref() == Some(&key) || self.thumbnail_pending {
            return;
        }
        if let Some(image) = self.thumbnail.take() {
            self.retired_images.retire(image, window);
        }
        self.thumbnail_key = Some(key.clone());
        self.thumbnail_error = None;
        if matches!(item, ProjectItem::Folder(_)) {
            return;
        }
        self.thumbnail_pending = true;
        cx.spawn(async move |entity, cx| {
            let snapshot = key.1.clone();
            let result = cx
                .background_executor()
                .spawn(async move { project_browser::thumbnail(&snapshot, item) })
                .await;
            let _ = entity.update(cx, |this, cx| {
                this.thumbnail_pending = false;
                if this.thumbnail_key.as_ref() == Some(&key) {
                    match result {
                        Ok(pixels) => {
                            this.thumbnail =
                                Some(Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
                                    pixels,
                                )])))
                        }
                        Err(error) => this.thumbnail_error = Some(error),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn item_row(
        &self,
        row: Row,
        selected: ProjectItem,
        active: u64,
        frame: u32,
        parent_path: Option<String>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let Row {
            item,
            name,
            kind,
            depth,
            ..
        } = row;
        let icon = match item {
            ProjectItem::Folder(_) => "folder-open",
            _ if kind == "Image" => "square",
            _ => "filmstrip",
        };
        let mut label = item_name(item, name.clone());
        if let Some(path) = parent_path {
            label = label.tooltip(move |_, cx| {
                cx.new(|_| ui::Tip(format!("{name} · {path}").into()))
                    .into()
            });
        }
        let mut element = ui::text_button(SharedString::from(format!("project-{item:?}")), "")
            .w_full()
            .min_w_0()
            .h(px(22.0))
            .flex_none()
            .gap_1()
            .justify_start()
            .pl(px(12.0 + depth as f32 * 12.0))
            .pr_2()
            .when(item == selected, |s| s.bg(rgb(0x343434)))
            .child(div().w(px(18.0)).flex_none().when_some(
                match item {
                    ProjectItem::Folder(id) => Some(id),
                    _ => None,
                },
                |d, id| {
                    d.child(
                        ui::tool(
                            SharedString::from(format!("folder-toggle-{id}")),
                            if self.collapsed.contains(&id) {
                                "chevron-right"
                            } else {
                                "chevron-down"
                            },
                            "Expand or collapse folder",
                            false,
                        )
                        .size(px(18.0))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_folder(id, cx);
                            cx.notify();
                        })),
                    )
                },
            ))
            .child(ui::icon(icon))
            .child(label)
            .child(
                div()
                    .flex_none()
                    .w(px(self.type_width + 5.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child(kind),
            )
            .on_key_down(|event: &gpui::KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                }
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.state.update(cx, |s, cx| {
                        s.project_item = Some(item);
                        cx.notify();
                    });
                    this.menu_position = Some(event.position);
                    window.focus(&this.focus);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    window.focus(&this.focus);
                    this.state.update(cx, |s, cx| {
                        s.project_item = Some(item);
                        if event.click_count() == 2 {
                            if let ProjectItem::Composition(id) = item {
                                s.dispatch(&Action::ActivateComposition(id), window, cx);
                            }
                        }
                        cx.notify();
                    });
                }),
            );
        let mut actions = div().w(px(36.0)).flex_none().flex().items_center();
        match item {
            ProjectItem::Folder(_) => {}
            ProjectItem::Asset(id) => {
                actions = actions.child(
                    ui::action_tool(
                        SharedString::from(format!("asset-add-{id}")),
                        "plus",
                        "Add footage at playhead",
                        &self.state,
                        Action::Edit(Command::AddAssetLayer { asset: id, frame }),
                        false,
                    )
                    .size(px(18.0)),
                );
            }
            ProjectItem::Composition(id) => {
                actions = actions.child(
                    ui::action_tool(
                        SharedString::from(format!("comp-open-{id}")),
                        "arrow-right",
                        "Open composition",
                        &self.state,
                        Action::ActivateComposition(id),
                        false,
                    )
                    .size(px(18.0)),
                );
                if id != active {
                    actions = actions.child(
                        ui::action_tool(
                            SharedString::from(format!("comp-add-{id}")),
                            "plus",
                            "Add to active composition",
                            &self.state,
                            Action::AddComposition(id),
                            false,
                        )
                        .size(px(18.0)),
                    );
                }
            }
        }
        element = element.child(actions);
        element
    }
}

#[cfg(test)]
mod row_label_tests {
    use super::*;
    use gpui::{Overflow, TextOverflow, WhiteSpace};

    #[test]
    fn project_name_remeasures_at_flex_width_and_clamps_to_one_ellipsized_line() {
        let mut label = item_name(
            ProjectItem::Composition(1),
            "A very long composition name with spaces and Unicode — 日本語".into(),
        );
        let style = label.style();
        assert_eq!(style.min_size.width, Some(px(0.0).into()));
        assert_eq!(style.flex_grow, Some(1.0));
        assert_eq!(style.flex_shrink, Some(1.0));
        assert_eq!(style.overflow.x, Some(Overflow::Hidden));
        assert_eq!(style.overflow.y, Some(Overflow::Hidden));
        let text = style.text.as_ref().expect("project name text style");
        // Nowrap leaves GPUI's wrap-width cache key at None, so flex shrink
        // would reuse the untruncated intrinsic measurement and only clip it.
        assert_eq!(text.white_space, Some(WhiteSpace::Normal));
        assert_eq!(text.line_clamp, Some(1));
        assert_eq!(text.text_overflow, Some(TextOverflow::Truncate("…".into())));
    }
}
impl Render for Browser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare_browsing(window, cx);
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
        let state = self.state.read(cx);
        if state.welcome() {
            let create = self.state.clone();
            return div()
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(ui::BG))
                .child(tabs)
                .child(div().flex_1().min_h_0())
                .child(
                    div()
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .border_t_1()
                        .border_color(rgb(ui::BORDER))
                        .child(ui::action_tool(
                            "empty-project-import",
                            "folder-open",
                            "Import footage (Ctrl+I)",
                            &self.state,
                            Action::ImportImage,
                            false,
                        ))
                        .child(
                            ui::tool(
                                "empty-project-composition",
                                "filmstrip",
                                "New composition (Ctrl+N)",
                                false,
                            )
                            .on_click(move |_, _, cx| {
                                create.update(cx, |s, cx| {
                                    s.new_composition_requested = true;
                                    cx.notify();
                                })
                            }),
                        ),
                )
                .into_any_element();
        }
        let project = state.editor.project().clone();
        let active = project.active_composition_id();
        let frame = state.frame;
        let all =
            project_browser::rows(&project, "", ItemType::All, false, false, &BTreeSet::new());
        let selected = state
            .project_item
            .filter(|item| all.iter().any(|r| r.item == *item))
            .unwrap_or(ProjectItem::Composition(active));
        let row = all.iter().find(|r| r.item == selected).unwrap();
        let folder = match selected {
            ProjectItem::Folder(id) => Some(id),
            _ => row.folder,
        };
        let name = row.name.clone();
        let path = project_browser::folder_path(&project, row.folder);
        let details: Vec<String> = match selected {
            ProjectItem::Composition(id) => {
                let c = project.composition_by_id(id).unwrap();
                vec![
                    format!("{} × {} (1.00)", c.width(), c.height()),
                    format!(
                        "{} fps · {:.2} s",
                        c.fps().label(),
                        c.fps().seconds(u64::from(c.duration()))
                    ),
                    format!("{} layers", c.layers().len()),
                ]
            }
            ProjectItem::Asset(id) => {
                let a = &project.asset_library().assets()[&id];
                let source = match a.content() {
                    Content::Video { .. } | Content::ImageSequence { .. } => format!(
                        "{} fps · {:.2} s",
                        a.interpretation().frame_rate(a.content()).unwrap().label(),
                        a.interpretation().duration(a.content()).unwrap()
                    ),
                    Content::Audio { audio, .. } => format!("Audio · {:.2} s", audio.duration),
                    _ => "Still image · embedded".into(),
                };
                vec![
                    a.content()
                        .audio()
                        .map(|(_, audio)| {
                            format!(
                                "{} Hz · {} ch · {}",
                                audio.sample_rate, audio.channels, audio.channel_layout
                            )
                        })
                        .unwrap_or_else(|| format!("{} × {}", a.width(), a.height())),
                    source,
                ]
            }
            ProjectItem::Folder(id) => vec![
                format!(
                    "{} direct item(s)",
                    all.iter().filter(|r| r.folder == Some(id)).count()
                ),
                path.clone(),
            ],
        };
        self.state
            .update(cx, |s, _| s.project_item = Some(selected));
        self.name.update(cx, |field, _| {
            field.sync(format!("{selected:?}"), name, window)
        });
        if self.details_open {
            self.load_thumbnail(&project, selected, window, cx);
        }
        let mut thumb = div()
            .w(px(72.0))
            .h(px(45.0))
            .flex_none()
            .bg(rgb(0x080808))
            .border_1()
            .border_color(rgb(0x4b4b4b))
            .flex()
            .items_center()
            .justify_center();
        if let Some(image) = self.thumbnail.as_ref().filter(|_| {
            self.thumbnail_key
                .as_ref()
                .is_some_and(|(item, p, revision)| {
                    *item == selected
                        && p == &project
                        && *revision == self.state.read(cx).preview_revision
                })
        }) {
            thumb = thumb.child(
                img(image.clone())
                    .size_full()
                    .object_fit(gpui::ObjectFit::Contain),
            );
        } else {
            thumb = thumb.child(ui::icon(if matches!(selected, ProjectItem::Folder(_)) {
                "folder-open"
            } else {
                "filmstrip"
            }));
        }
        let query = self.search.read(cx).value().to_owned();
        let rows = project_browser::rows(
            &project,
            &query,
            self.item_type,
            self.by_type,
            self.descending,
            &self.collapsed,
        );
        let filtering = self.filters_active(cx);
        let no_results = rows.is_empty();
        let count = format!(
            "{} / {} items{}",
            rows.len(),
            all.len(),
            if rows.iter().any(|row| row.item == selected) {
                ""
            } else {
                " · selected hidden"
            }
        );
        let mut panel = div()
            .id("project-browser")
            .border_1()
            .border_color(rgb(ui::BORDER))
            .focus(|s| s.border_color(rgb(ui::BLUE)))
            .track_focus(&self.focus)
            .tab_index(0)
            .capture_key_down(cx.listener(Self::tab_key))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.menu_position.is_some() {
                    if event.keystroke.key == "escape" {
                        this.menu_position = None;
                        cx.notify();
                    }
                    cx.stop_propagation();
                } else {
                    this.browsing_key(event, window, cx);
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .bg(rgb(ui::BG))
            .overflow_hidden()
            .child(tabs)
            .when(self.details_open, |d| {
                d.child(
                    div()
                        .flex()
                        .gap_3()
                        .p_3()
                        .h(px(96.0))
                        .flex_none()
                        .child(thumb)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .text_size(px(11.0))
                                .child(self.name.clone())
                                .children(details.into_iter().map(|line| {
                                    div()
                                        .overflow_hidden()
                                        .text_color(rgb(ui::MUTED))
                                        .child(line)
                                })),
                        ),
                )
            })
            .when(self.usage_open == Some(selected), |d| {
                d.child(self.usage_details(&project, selected, cx))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .mx_2()
                    .gap_1()
                    .child(
                        ui::tool(
                            "project-search-focus",
                            "magnifier",
                            "Search Project names and types (Ctrl+F)",
                            false,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.search.read(cx).focus_input(window)
                        })),
                    )
                    .child(div().flex_1().min_w_0().child(self.search.clone()))
                    .child(
                        ui::tool(
                            "project-filters",
                            "magnifier",
                            "Show Project type filters",
                            self.filters_open,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.filters_open = !this.filters_open;
                            cx.notify();
                            cx.stop_propagation();
                        })),
                    )
                    .child(
                        ui::tool(
                            "project-actions",
                            "chevron-down",
                            "Project item actions (also right-click an item)",
                            false,
                        )
                        .on_key_down(|e, _, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                            }
                        })
                        .on_click(cx.listener(
                            |this, event: &gpui::ClickEvent, window, cx| {
                                this.menu_position = Some(event.position());
                                window.focus(&this.focus);
                                cx.notify();
                                cx.stop_propagation();
                            },
                        )),
                    )
                    .child(
                        ui::tool(
                            "project-clear-filters",
                            "xmark",
                            "Clear Project search and type filter",
                            false,
                        )
                        .when(!filtering, |d| d.opacity(0.35))
                        .on_key_down(|event: &gpui::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                                cx.stop_propagation();
                            }
                        })
                        .on_click(
                            cx.listener(|this, _, window, cx| this.clear_filters(window, cx)),
                        ),
                    ),
            )
            .when(self.filters_open || self.item_type != ItemType::All, |d| {
                d.child(self.type_filters(cx))
            })
            .when(self.move_open, |d| {
                d.child(
                    div()
                        .flex()
                        .h(px(25.0))
                        .px_2()
                        .items_center()
                        .child(
                            ui::text_button("project-move", format!("Move to… · {path}"))
                                .flex_1()
                                .overflow_hidden()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.move_open = !this.move_open;
                                    cx.notify();
                                })),
                        )
                        .when(matches!(selected, ProjectItem::Asset(_)), |d| {
                            d.child(ui::action_tool(
                                "delete-project-asset",
                                "trash-bin",
                                "Delete unused source",
                                &self.state,
                                Action::Edit(Command::DeleteProjectItem(selected)),
                                false,
                            ))
                        })
                        .when(matches!(selected, ProjectItem::Folder(_)), |d| {
                            d.child(ui::action_tool(
                                "delete-project-folder",
                                "trash-bin",
                                "Delete empty folder",
                                &self.state,
                                Action::Edit(Command::DeleteProjectItem(selected)),
                                false,
                            ))
                        }),
                )
            });
        if self.move_open {
            let mut destinations = vec![(None, "Project root".to_owned())];
            destinations.extend(
                project
                    .asset_library()
                    .folders()
                    .keys()
                    .map(|id| (Some(*id), project_browser::folder_path(&project, Some(*id)))),
            );
            panel = panel.child(
                div()
                    .id("project-move-destinations")
                    .max_h(px(160.0))
                    .overflow_y_scroll()
                    .children(destinations.into_iter().map(|(folder, label)| {
                        ui::text_button(
                            SharedString::from(format!("move-destination-{folder:?}")),
                            label,
                        )
                        .w_full()
                        .justify_start()
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::MoveProjectItem {
                                            item: selected,
                                            folder,
                                        }),
                                        window,
                                        cx,
                                    )
                                });
                                this.move_open = false;
                                cx.notify();
                            },
                        ))
                    })),
            );
        }
        if let Some(error) = &self.thumbnail_error {
            panel = panel.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(0xf0a070))
                    .h(px(20.0))
                    .px_2()
                    .overflow_hidden()
                    .child(error.clone()),
            );
        }
        if let ProjectItem::Asset(id) = selected {
            panel = panel.when(self.interpretation_open, |d| {
                d.child(
                    div()
                        .flex()
                        .px_2()
                        .gap_1()
                        .when(
                            !matches!(
                                project.asset_library().assets()[&id].content(),
                                Content::Audio { .. }
                            ),
                            |d| {
                                d.child(
                                    ui::text_button("interpret-footage", "Interpret footage…")
                                        .flex_1()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.interpretation_open = !this.interpretation_open;
                                            cx.notify();
                                        })),
                                )
                            },
                        )
                        .child(
                            ui::text_button("comp-from-footage", "New comp from source").on_click(
                                {
                                    let state = self.state.clone();
                                    move |_, window, cx| {
                                        state.update(cx, |s, cx| {
                                            s.dispatch(
                                                &Action::Edit(Command::CompositionFromAsset(id)),
                                                window,
                                                cx,
                                            )
                                        });
                                    }
                                },
                            ),
                        ),
                )
            });
            if self.interpretation_open
                && !matches!(
                    project.asset_library().assets()[&id].content(),
                    Content::Audio { .. }
                )
            {
                if self
                    .interpretation
                    .as_ref()
                    .is_none_or(|(owner, _)| *owner != id)
                {
                    self.interpretation = Some((
                        id,
                        cx.new(|cx| {
                            super::footage_interpretation::Interpretation::new(
                                self.state.clone(),
                                id,
                                cx,
                            )
                        }),
                    ));
                }
                panel = panel.child(self.interpretation.as_ref().unwrap().1.clone());
            }
            if matches!(
                project.asset_library().assets()[&id].content(),
                Content::ImageSequence { .. }
            ) {
                panel = panel.child(
                    ui::text_button("relink-sequence", "Relink sequence folder…").on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(&Action::RelinkSequence(id), window, cx)
                            });
                        }
                    }),
                );
            }
            if let Content::Video { path, .. } | Content::Audio { path, .. } =
                project.asset_library().assets()[&id].content()
            {
                panel = panel.child(
                    ui::text_button("project-relink", "Relink source…").on_click({
                        let state = self.state.clone();
                        let path = path.clone();
                        move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(&Action::RelinkSource(path.clone()), window, cx)
                            })
                        }
                    }),
                );
            }
        }
        panel = panel
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if let Some((x, width)) = this.column_drag {
                    this.type_width = (width + x - f32::from(event.position.x)).clamp(24.0, 150.0);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.column_drag = None;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.column_drag = None;
                    cx.notify();
                }),
            );
        if let Some(position) = self.menu_position {
            let mut menu = div()
                .id("project-actions-menu")
                .w(px(225.0))
                .p_1()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(ui::BLUE))
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.menu_position = None;
                    cx.notify();
                }));
            for (index, label) in [
                "Item details",
                "Show uses",
                "Move to folder…",
                "Interpret footage…",
                "New composition from source",
                "Delete unused item",
            ]
            .into_iter()
            .enumerate()
            {
                let disabled = match index {
                    1 => matches!(selected, ProjectItem::Folder(_)),
                    3 | 4 => !matches!(selected, ProjectItem::Asset(_)),
                    _ => false,
                };
                menu = menu.child(
                    ui::text_button(("project-action", index), label)
                        .when(disabled, |d| d.opacity(0.35))
                        .w_full()
                        .justify_start()
                        .on_key_down(|e, _, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                            }
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.menu_position = None;
                            match index {
                                0 => this.details_open = !this.details_open,
                                1 => {
                                    this.usage_open = if this.usage_open == Some(selected) {
                                        None
                                    } else {
                                        Some(selected)
                                    }
                                }
                                2 => this.move_open = !this.move_open,
                                3 => this.interpretation_open = !this.interpretation_open,
                                4 => {
                                    if let ProjectItem::Asset(id) = selected {
                                        this.state.update(cx, |s, cx| {
                                            s.dispatch(
                                                &Action::Edit(Command::CompositionFromAsset(id)),
                                                window,
                                                cx,
                                            )
                                        });
                                    }
                                }
                                _ => {
                                    this.state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::Edit(Command::DeleteProjectItem(selected)),
                                            window,
                                            cx,
                                        )
                                    });
                                }
                            }
                            cx.notify();
                            cx.stop_propagation();
                        })),
                );
            }
            panel = panel.child(
                gpui::deferred(
                    gpui::anchored()
                        .position(position)
                        .snap_to_window_with_margin(px(8.0))
                        .child(menu),
                )
                .with_priority(2),
            );
        }
        panel
            .child(
                div()
                    .flex()
                    .px_2()
                    .h(px(25.0))
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(0x3a3a3a))
                    .text_size(px(11.0))
                    .text_color(rgb(ui::MUTED))
                    .child(
                        ui::text_button(
                            "project-sort-name",
                            if !self.by_type && self.descending {
                                "Name ↓"
                            } else {
                                "Name ↑"
                            },
                        )
                        .flex_1()
                        .justify_start()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.descending = !this.by_type && !this.descending;
                            this.by_type = false;
                            cx.notify();
                        })),
                    )
                    .child(
                        div().id("project-column-divider").w(px(5.0)).h_full().cursor_col_resize().hover(|d| d.bg(rgb(ui::BLUE)))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                                this.column_drag = Some((f32::from(event.position.x), this.type_width)); window.prevent_default(); cx.stop_propagation();
                            }))
                    )
                    .child(
                        ui::text_button("project-sort-type", "Type").w(px(self.type_width)).flex_none().px_0().justify_start().on_click(cx.listener(
                            |this, _, _, cx| {
                                this.descending = this.by_type && !this.descending;
                                this.by_type = true;
                                cx.notify();
                            },
                        )),
                    )
                    .child(div().w(px(36.0)).flex_none()),
            )
            .child(
                div()
                    .id("project-items")
                    .track_scroll(&self.row_scroll)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, _| {
                        if !window.default_prevented() {
                            window.focus(&this.focus);
                        }
                    }))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(no_results, |d| d.child(div().px_3().py_2().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child("No matching Project items. Clear the filters to see all items.")))
                    .children(rows.into_iter().map(|row| {
                        let path = filtering.then(|| project_browser::folder_path(&project, row.folder));
                        self.item_row(row, selected, active, frame, path, cx)
                    })),
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
                        "project-import",
                        "folder-open",
                        "Import footage (Ctrl+I)",
                        &self.state,
                        Action::ImportImage,
                        false,
                    ))
                    .child(ui::action_tool(
                        "project-new-folder",
                        "plus",
                        "New folder",
                        &self.state,
                        Action::Edit(Command::NewProjectFolder {
                            name: "Untitled Folder".into(),
                            parent: folder,
                        }),
                        false,
                    ))
                    .child(ui::action_tool(
                        "project-composition",
                        "filmstrip",
                        "New composition",
                        &self.state,
                        Action::Edit(Command::NewComposition),
                        false,
                    ))
                    .child(
                        div()
                            .id("project-match-count")
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .text_ellipsis()
                            .line_clamp(1)
                            .tooltip({ let count = count.clone(); move |_, cx| cx.new(|_| ui::Tip(count.clone().into())).into() })
                            .child(count),
                    ),
            )
            .into_any_element()
    }
}
