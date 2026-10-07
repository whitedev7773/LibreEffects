//! Project-local visibility and keyboard selection; never serialized in VIEW.
use super::*;
use libre_effects_editor_model::project_browser::{
    TabDirection, blocks_layer_shortcut, select, shortcut, tab_direction,
};

fn type_filter_row() -> gpui::Div {
    div()
        .flex()
        .flex_wrap()
        .flex_none()
        .min_w_0()
        .min_h(px(27.0))
        .px_2()
        .py(px(1.0))
        .gap_1()
        .items_center()
        .text_size(px(11.0))
}

fn type_filter_button(index: usize, kind: ItemType, selected: bool) -> gpui::Stateful<gpui::Div> {
    ui::text_button(
        ("project-type-filter", index),
        match kind {
            ItemType::All => "All",
            ItemType::Compositions => "Comps",
            ItemType::Footage => "Footage",
            ItemType::Folders => "Folders",
        },
    )
    // Intrinsic, non-shrinking labels wrap as whole controls at the actual
    // available pane width. Keep the same elements and Tab order on every row.
    .flex_none()
    .px_1()
    .whitespace_nowrap()
    .when(selected, |d| d.bg(rgb(0x164a7b)))
    .tooltip(move |_, cx| {
        cx.new(|_| {
            ui::Tip(
                format!(
                    "{}{}",
                    kind.label(),
                    if selected { " · selected" } else { "" }
                )
                .into(),
            )
        })
        .into()
    })
}

impl Browser {
    pub(super) fn filters_active(&self, cx: &gpui::App) -> bool {
        self.item_type != ItemType::All || !self.search.read(cx).value().trim().is_empty()
    }

    pub(super) fn prepare_browsing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let revision = self.state.read(cx).document_revision;
        if self.filter_context == Some(revision) {
            return;
        }
        // The existing document revision also changes on composition activation.
        // Reset conservatively so a replacement cannot inherit stale folder IDs.
        self.filter_context = Some(revision);
        self.item_type = ItemType::All;
        self.collapsed.clear();
        self.move_open = false;
        self.interpretation_open = false;
        self.usage_open = None;
        self.usage_cache = None;
        self.search.update(cx, |field, cx| {
            field.sync(format!("project-search:{revision}"), String::new(), window);
            cx.notify();
        });
        self.row_scroll.scroll_to_item(0);
    }

    pub(super) fn clear_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.item_type = ItemType::All;
        let revision = self.state.read(cx).document_revision;
        self.search.update(cx, |field, cx| {
            field.sync(format!("project-search:{revision}"), String::new(), window);
            cx.notify();
        });
        cx.notify();
    }

    pub(super) fn toggle_folder(&mut self, id: FolderId, cx: &mut Context<Self>) {
        if self.filters_active(cx) {
            self.state.update(cx, |s, cx| {
                s.status = "Clear Project filters to expand or collapse folders.".into();
                cx.notify();
            });
        } else if !self.collapsed.remove(&id) {
            self.collapsed.insert(id);
        }
        cx.notify();
    }

    pub(super) fn type_filters(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        type_filter_row()
            .children(ItemType::ALL.into_iter().enumerate().map(|(index, kind)| {
                type_filter_button(index, kind, kind == self.item_type)
                    .on_key_down(|event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            cx.stop_propagation();
                        }
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.item_type = kind;
                        window.focus(&this.focus);
                        this.row_scroll.scroll_to_item(0);
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }

    pub(super) fn tab_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key != "tab" || !self.focus.contains_focused(window, cx) {
            return;
        }
        // Capture before TextField consumes the event. Tab must never commit a
        // source draft through its blur callback, or leave an active IME/gesture.
        cx.stop_propagation();
        window.prevent_default();
        let m = event.keystroke.modifiers;
        let state = self.state.read(cx);
        let input_busy = TextField::is_composing(window, cx)
            || TextField::active_has_pending_source_input(cx)
            || cx.has_active_drag()
            || state.text_session.is_some()
            || state.colors.session.is_some()
            || state.gradient_editor.is_some()
            || state.vertex_editor.is_some()
            || state.expression_editor.is_some()
            || state.preview_scrub;
        let Some(direction) = tab_direction(
            m.shift,
            m.control || m.alt || m.platform || m.function,
            event.is_held,
            input_busy,
        ) else {
            return;
        };
        let Some(previous) = window.focused(cx) else {
            return;
        };
        match direction {
            TabDirection::Previous => window.focus_prev(),
            TabDirection::Next => window.focus_next(),
        }
        // Keep this local to Project. Clamp at its first/last tab stop instead
        // of handing focus to a panel whose Tab behavior we do not own.
        if !self.focus.contains_focused(window, cx) {
            window.focus(&previous);
        }
    }

    pub(super) fn browsing_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.contains_focused(window, cx) {
            return;
        }
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        // Text entry and IME retain all their keys, including arrow/caret input.
        if TextField::active_has_focus(window, cx) || TextField::is_composing(window, cx) {
            return;
        }
        if key == "space" && !self.focus.is_focused(window) {
            // GPUI activates buttons on key-up. Do not let their key-down also
            // toggle playback in the Shell before that click arrives.
            cx.stop_propagation();
            return;
        }
        if blocks_layer_shortcut(key, m.control, m.alt) {
            // Project selection is not Timeline selection. Unimplemented Project
            // editing keys must not mutate a retained layer/key target elsewhere.
            cx.stop_propagation();
            return;
        }
        let find = key == "f" && m.control && !m.shift && !m.alt && !m.platform && !m.function;
        let navigation = shortcut(key, m.control, m.shift, m.alt, m.platform, m.function);
        let row_key = matches!(key, "up" | "down" | "home" | "end");
        if !find && !row_key && !matches!(key, "enter" | "f2") {
            return;
        }
        // Even unsupported row chords cannot leak into Shell layer nudges/seeks.
        cx.stop_propagation();
        let state = self.state.read(cx);
        if cx.has_active_drag()
            || TextField::active_has_pending_source_input(cx)
            || state.text_session.is_some()
            || state.colors.session.is_some()
            || state.gradient_editor.is_some()
            || state.vertex_editor.is_some()
            || state.expression_editor.is_some()
            || state.preview_scrub
        {
            return;
        }
        if find {
            self.search
                .update(cx, |field, cx| field.focus_select_all(window, cx));
            return;
        }
        // Toolbar buttons retain Enter/Space semantics; only the focused list
        // owns selection and opening. Mouse row selection focuses this handle.
        if !self.focus.is_focused(window) {
            return;
        }
        let rows = project_browser::rows(
            state.editor.project(),
            self.search.read(cx).value(),
            self.item_type,
            self.by_type,
            self.descending,
            &self.collapsed,
        );
        let selected = state.project_item.or(Some(ProjectItem::Composition(
            state.editor.project().active_composition_id(),
        )));
        if key == "f2" {
            if m.modified()
                || event.is_held
                || state.playing
                || self.filter_context != Some(state.document_revision)
            {
                return;
            }
            let Some(row) = rows.iter().find(|row| Some(row.item) == selected) else {
                return;
            };
            // Rebind from the live visible selection before focusing in case a
            // previous selection has not drawn yet. Reuse the details field's
            // existing commit/cancel path; F2 itself never dispatches an edit.
            self.name.update(cx, |field, cx| {
                field.sync(format!("{:?}", row.item), row.name.clone(), window);
                field.focus_select_all(window, cx);
            });
        } else if let Some(navigation) = navigation {
            if let Some(item) = select(&rows, selected, navigation) {
                let index = rows.iter().position(|row| row.item == item).unwrap();
                // No dispatch: source, Undo, layer/key selection, Graph VIEW,
                // active composition, playhead and transport stay untouched.
                self.state.update(cx, |s, cx| {
                    s.project_item = Some(item);
                    cx.notify();
                });
                self.row_scroll.scroll_to_item(index);
                cx.notify();
            }
        } else if key == "enter" && !m.modified() && !event.is_held {
            let Some(item) = selected.filter(|item| rows.iter().any(|row| row.item == *item))
            else {
                return;
            };
            match item {
                ProjectItem::Composition(id) => self.state.update(cx, |s, cx| {
                    s.dispatch(&Action::ActivateComposition(id), window, cx);
                }),
                ProjectItem::Folder(id) => self.toggle_folder(id, cx),
                ProjectItem::Asset(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_filter_row_grows_for_wrapped_controls_without_shrinking() {
        let mut row = type_filter_row();
        let style = row.style();
        assert_eq!(style.flex_wrap, Some(gpui::FlexWrap::Wrap));
        assert_eq!(style.flex_shrink, Some(0.0));
        assert_eq!(style.size.height, None);
        assert_eq!(style.min_size.height, Some(px(27.0).into()));
        assert_eq!(style.min_size.width, Some(px(0.0).into()));
    }

    #[test]
    fn type_filter_labels_keep_intrinsic_single_line_width() {
        for (index, kind) in ItemType::ALL.into_iter().enumerate() {
            let mut button = type_filter_button(index, kind, false);
            let style = button.style();
            assert_eq!(style.flex_grow, Some(0.0));
            assert_eq!(style.flex_shrink, Some(0.0));
            assert_eq!(style.size.width, None);
            assert_eq!(
                style.text.as_ref().unwrap().white_space,
                Some(gpui::WhiteSpace::Nowrap)
            );
        }
    }
}
