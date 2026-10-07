//! Transient layer visibility and Timeline-owned editing scope. Never saved in VIEW.
use super::*;
use crate::timeline_filter::{LayerTypeFilter, matching_layer_ids, selected_visible_click};
use gpui::{App, KeyDownEvent};

impl Timeline {
    pub(crate) fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx) && !self.graph.read(cx).contains_focus(window, cx)
    }

    pub(super) fn visible_layer_ids(&self, cx: &App) -> BTreeSet<LayerId> {
        let state = self.state.read(cx);
        matching_layer_ids(
            state.editor.project().composition(),
            self.search.read(cx).value(),
            self.layer_type,
            self.selected_only,
            &state.selected_layers,
        )
    }

    fn filters_active(&self, cx: &App) -> bool {
        !self.search.read(cx).value().trim().is_empty()
            || self.layer_type != LayerTypeFilter::All
            || self.selected_only
    }

    pub(super) fn scope_blocked(&self, scope: filter_safety::TargetScope, cx: &App) -> bool {
        let state = self.state.read(cx);
        let visible = self.visible_layer_ids(cx);
        filter_safety::blocks_hidden_targets(
            scope,
            &visible,
            &state.selected_layers,
            &state.selected_keys,
            visible.len() != state.editor.project().composition().layers().len(),
        )
    }

    pub(crate) fn selection_action_blocked(&self, action: &Action, cx: &App) -> bool {
        if matches!(action, Action::Edit(Command::MoveLayer { .. })) {
            return !self.layer_reorder_allowed(cx);
        }
        filter_safety::action_scope(action).is_some_and(|scope| self.scope_blocked(scope, cx))
    }

    pub(super) fn layer_reorder_allowed(&self, cx: &App) -> bool {
        !self.filters_active(cx)
            && !self
                .state
                .read(cx)
                .editor
                .project()
                .composition()
                .hide_shy()
    }

    pub(super) fn move_visible_layer(
        &mut self,
        id: LayerId,
        direction: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.layer_reorder_allowed(cx) {
            return;
        }
        self.state.update(cx, |s, cx| {
            let layers = s.editor.project().composition().layers();
            let Some(index) = layers.iter().position(|l| l.id() == id) else {
                return;
            };
            let index =
                (index as i64 + i64::from(direction)).clamp(0, layers.len() as i64 - 1) as usize;
            s.dispatch(&Action::Edit(Command::MoveLayer { id, index }), window, cx);
        });
    }

    pub(super) fn cancel_filtered_gestures(&mut self) {
        self.drag = None;
        self.bar_drag = None;
        self.marquee = None;
        self.scrubbing = false;
        self.snapped_to = None;
        self.parent_open = None;
        self.key_menu = None;
        self.colors.cancel_pointer();
    }

    pub(super) fn prepare_layer_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let context = (
            state.document_revision,
            state.editor.project().active_composition_id(),
        );
        if self.filter_context != Some(context) {
            self.layer_navigation.reset();
            self.reveal_layer = None;
            self.filter_context = Some(context);
            self.layer_type = LayerTypeFilter::All;
            self.selected_only = false;
            self.type_open = false;
            self.type_cursor = 0;
            self.search.update(cx, |field, cx| {
                field.sync(
                    format!("timeline-search:{}:{}", context.0, context.1),
                    String::new(),
                    window,
                );
                cx.notify();
            });
            self.cancel_filtered_gestures();
        }
        let visible = self.visible_layer_ids(cx);
        let signature = (
            self.search.read(cx).value().to_string(),
            self.layer_type,
            self.selected_only,
        );
        if visible != self.visible_layers || signature != self.filter_signature {
            if signature != self.filter_signature || !self.selected_only {
                self.layer_navigation.reset();
            }
            self.cancel_filtered_gestures();
            // A hidden Colors owner must not retain a direct edit path to its keys.
            if self
                .state
                .read(cx)
                .editor
                .selected()
                .is_some_and(|id| !visible.contains(&id))
            {
                self.colors.clear_selection();
            }
            self.visible_layers = visible;
            self.filter_signature = signature;
        }
    }

    pub(super) fn blocked_filter_edit(&self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.status = "Some targets are hidden in the Timeline. Clear the layer filters or select visible rows before editing.".into();
            cx.notify();
        });
    }

    pub(super) fn select_visible_layer(
        &mut self,
        id: LayerId,
        toggle: bool,
        range: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let visible = self.visible_layer_ids(cx);
        if !visible.contains(&id) {
            return;
        }
        self.layer_navigation.reset();
        let selection = {
            let s = self.state.read(cx);
            selected_visible_click(
                s.editor.project().composition(),
                &visible,
                &s.selected_layers,
                s.editor.selected(),
                id,
                toggle,
                range,
            )
        };
        self.colors.clear_selection();
        self.selected_key = None;
        self.state.update(cx, |s, cx| {
            // Let the shared selection action update Graph identity, but never run
            // its full-stack range expansion on filtered rows.
            s.selected_layers.retain(|id| visible.contains(id));
            s.dispatch(&Action::SelectMany(id, toggle && !range, false), window, cx);
            s.selected_layers = selection;
            cx.notify();
        });
    }

    fn clear_layer_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.layer_type = LayerTypeFilter::All;
        self.selected_only = false;
        self.type_open = false;
        self.type_cursor = 0;
        let context = self.filter_context.unwrap_or_default();
        self.search.update(cx, |field, cx| {
            field.sync(
                format!("timeline-search:{}:{}", context.0, context.1),
                String::new(),
                window,
            );
            cx.notify();
        });
        self.cancel_filtered_gestures();
        cx.notify();
    }

    pub(super) fn reveal_project_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((revision, composition, layer, input_generation)) =
            self.state.update(cx, |s, _| s.project_usage_reveal.take())
        else {
            return;
        };
        let state = self.state.read(cx);
        if state.document_revision != revision
            || state.editor.project().active_composition_id() != composition
            || state.editor.selected() != Some(layer)
            || state.input_context_generation() != input_generation
            || !state.project_usage_available()
            || TextField::active_has_pending_source_input(cx)
            || TextField::is_composing(window, cx)
        {
            return;
        }
        // This is an explicit jump. Clear transient filters but never change the
        // saved Hide Shy switch; the editor status explains a hidden shy target.
        self.clear_layer_filters(window, cx);
        self.prepare_layer_filters(window, cx);
        self.layer_navigation.reset();
        self.selected_key = None;
        self.colors.clear_selection();
        self.reveal_layer = Some(layer);
    }

    pub(super) fn layer_filter_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if self.type_focus.is_focused(window) {
            if key == "tab" {
                self.type_open = false;
                cx.notify();
                return false;
            }
            if m.control && !m.alt && !m.shift && !m.platform && key == "f" {
                self.type_open = false;
                self.search.read(cx).focus_input(window);
            } else if !m.modified() && !event.is_held {
                match key {
                    "escape" => {
                        self.type_open = false;
                        window.focus(&self.focus);
                    }
                    "enter" | "space" => {
                        if self.type_open {
                            self.layer_type = LayerTypeFilter::ALL[self.type_cursor];
                            self.type_open = false;
                        } else {
                            self.type_cursor = LayerTypeFilter::ALL
                                .iter()
                                .position(|t| *t == self.layer_type)
                                .unwrap_or(0);
                            self.type_open = true;
                        }
                    }
                    "down" | "up" => {
                        self.type_open = true;
                        let count = LayerTypeFilter::ALL.len();
                        self.type_cursor = if key == "down" {
                            (self.type_cursor + 1) % count
                        } else {
                            (self.type_cursor + count - 1) % count
                        };
                    }
                    "home" => {
                        self.type_open = true;
                        self.type_cursor = 0;
                    }
                    "end" => {
                        self.type_open = true;
                        self.type_cursor = LayerTypeFilter::ALL.len() - 1;
                    }
                    _ => {}
                }
            }
            self.type_scroll.scroll_to_item(self.type_cursor);
            cx.notify();
            return true;
        }
        if self.focus.is_focused(window)
            && m.control
            && !m.alt
            && !m.shift
            && !m.platform
            && key == "f"
        {
            if !event.is_held {
                self.search.read(cx).focus_input(window);
            }
            return true;
        }
        false
    }

    pub(super) fn render_layer_filters(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let state = self.state.read(cx);
        let total = state.editor.project().composition().layers().len();
        let hidden_selected = state
            .selected_layers
            .difference(&self.visible_layers)
            .count();
        let active = self.filters_active(cx);
        let count = format!("{} / {total}", self.visible_layers.len());
        let mut type_menu = div()
            .id("timeline-type-menu")
            .track_scroll(&self.type_scroll)
            .flex()
            .flex_col()
            .max_h(px(220.0))
            .overflow_y_scroll()
            .w(px(145.0))
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(ui::BLUE))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.type_open = false;
                cx.notify();
            }));
        for (index, choice) in LayerTypeFilter::ALL.into_iter().enumerate() {
            type_menu = type_menu.child(
                ui::text_button(("timeline-type-option", index), choice.label())
                    .w_full()
                    .flex_none()
                    .justify_start()
                    .when(index == self.type_cursor, |s| s.bg(rgb(0x344455)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.layer_type = choice;
                        this.type_cursor = index;
                        this.type_open = false;
                        this.cancel_filtered_gestures();
                        window.focus(&this.focus);
                        cx.notify();
                        cx.stop_propagation();
                    })),
            );
        }
        div()
            .flex()
            .items_center()
            .h(px(30.0))
            .flex_none()
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(rgb(ui::BORDER))
            .child(div().text_color(rgb(ui::MUTED)).child("Layers"))
            .child(
                ui::tool(
                    "timeline-search-focus",
                    "magnifier",
                    "Search layer names (Ctrl+F in Timeline)",
                    false,
                )
                .on_click(
                    cx.listener(|this, _, window, cx| this.search.read(cx).focus_input(window)),
                ),
            )
            .child(div().w(px(175.0)).flex_none().child(self.search.clone()))
            .child(
                div()
                    .relative()
                    .w(px(120.0))
                    .flex_none()
                    .child(
                        ui::text_button(
                            "timeline-type-filter",
                            format!("{} ▾", self.layer_type.label()),
                        )
                        .track_focus(&self.type_focus)
                        .w_full()
                        .justify_start()
                        .when(self.layer_type != LayerTypeFilter::All, |s| {
                            s.bg(rgb(0x164a7b))
                        })
                        .on_click(cx.listener(
                            |this, event, window, cx| {
                                if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                                    return;
                                }
                                window.focus(&this.type_focus);
                                this.type_open = !this.type_open;
                                this.type_cursor = LayerTypeFilter::ALL
                                    .iter()
                                    .position(|t| *t == this.layer_type)
                                    .unwrap_or(0);
                                cx.notify();
                                cx.stop_propagation();
                            },
                        )),
                    )
                    .when(self.type_open, |d| {
                        d.child(
                            gpui::deferred(type_menu.absolute().top(px(27.0)).left_0())
                                .with_priority(3),
                        )
                    }),
            )
            .child(
                ui::text_button("timeline-selected-filter", "Selected only")
                    .when(self.selected_only, |d| {
                        d.bg(rgb(0x164a7b)).text_color(rgb(ui::BLUE))
                    })
                    .on_key_down(|event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                            cx.stop_propagation();
                        }
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.selected_only = !this.selected_only;
                        this.cancel_filtered_gestures();
                        window.focus(&this.focus);
                        cx.notify();
                    })),
            )
            .child(
                ui::tool(
                    "timeline-clear-filters",
                    "xmark",
                    "Clear layer search, type and selected-only filters",
                    false,
                )
                .when(!active, |d| d.opacity(0.35))
                .on_key_down(|event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        cx.stop_propagation();
                    }
                })
                .on_click(cx.listener(|this, _, window, cx| this.clear_layer_filters(window, cx))),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(ui::MUTED))
                    .child(count),
            )
            .when(hidden_selected > 0, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        // Give the warning the remaining row width. Width-sensitive
                        // text measurement avoids an empty clipped flex item.
                        .whitespace_normal()
                        .text_ellipsis()
                        .line_clamp(1)
                        .text_size(px(11.0))
                        .text_color(rgb(0xe0ba79))
                        .child(format!(
                            "{hidden_selected} selected hidden · clear filters to edit"
                        )),
                )
            })
            .into_any_element()
    }
}
