//! Search existing workspace commands without introducing a second dispatch path.
use super::{Shell, menu};
use crate::{components::TextField, ui};
use gpui::{Context, KeyDownEvent, Window, div, prelude::*, px, rgb};

impl Shell {
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_return_focus = self.menu_return_focus.take().or_else(|| window.focused(cx));
        TextField::commit_active(window, cx);
        self.state.update(cx, |s, cx| s.finish_text(true, cx));
        self.menu = None;
        self.menu_cursor = None;
        self.search_open = true;
        self.search_cursor = None;
        self.search_query.clear();
        window.blur();
        self.search_field.update(cx, |field, _| {
            field.sync("command-search".into(), String::new(), window);
            field.focus_input(window);
        });
        self.search_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        cx.notify();
    }
    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = false;
        if let Some(focus) = self.search_return_focus.take() {
            window.focus(&focus);
        } else {
            window.focus(&self.focus);
        }
        cx.notify();
    }
    fn search_results(&mut self, cx: &Context<Self>) -> Vec<menu::Match> {
        let query = self.search_field.read(cx).value();
        let results = menu::search(query, self.state.read(cx));
        if self.search_query != query
            || !results
                .iter()
                .any(|r| Some(r.key) == self.search_cursor && r.item.target.is_some())
        {
            self.search_cursor = menu::search_step(&results, None, true);
            self.search_query = query.to_string();
            self.search_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        }
        results
    }
    fn execute_search(&mut self, key: menu::Key, window: &mut Window, cx: &mut Context<Self>) {
        if TextField::is_composing(window, cx) {
            return;
        }
        let results = self.search_results(cx);
        if !results
            .iter()
            .any(|r| r.key == key && r.item.target.is_some())
        {
            return;
        }
        if let Some(target) = menu::resolve(key, self.state.read(cx)) {
            self.search_open = false;
            self.search_return_focus = None;
            self.run_menu(target, window, cx);
        }
    }
    pub(super) fn search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if TextField::is_composing(window, cx) {
            return;
        }
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if m.alt && key == "f4" {
            self.close_search(window, cx);
            return;
        }
        let results = self.search_results(cx);
        match key {
            "escape" => self.close_search(window, cx),
            "p" if m.control && m.shift => self.close_search(window, cx),
            "up" | "down" => {
                self.search_cursor = menu::search_step(&results, self.search_cursor, key == "down")
            }
            "home" | "end" if m.control => {
                self.search_cursor = menu::search_step(&results, None, key == "home")
            }
            "enter" => {
                if let Some(key) = self.search_cursor {
                    self.execute_search(key, window, cx);
                }
            }
            "tab" => self.search_field.read(cx).focus_input(window),
            "s" | "o" | "n" if m.control => {}
            _ => return,
        }
        if let Some(index) = results
            .iter()
            .position(|r| Some(r.key) == self.search_cursor)
        {
            self.search_scroll.scroll_to_item(index);
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }
    pub(super) fn render_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let results = self.search_results(cx);
        let count = results.len();
        let mut list = div()
            .id("command-search-results")
            .max_h(
                (window.viewport_size().height - px(245.0))
                    .max(px(100.0))
                    .min(px(390.0)),
            )
            .overflow_y_scroll()
            .track_scroll(&self.search_scroll);
        for (i, entry) in results.into_iter().enumerate() {
            let enabled = entry.item.target.is_some();
            let key = entry.key;
            list = list.child(
                div()
                    .id(("command-result", i))
                    .px_2()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .tab_index(0)
                    .hover(|d| d.bg(rgb(0x353535)))
                    .focus(|d| d.bg(rgb(0x164a7b)))
                    .h(px(43.0))
                    .w_full()
                    .justify_between()
                    .gap_3()
                    .flex_none()
                    .when(!enabled, |d| d.opacity(0.4))
                    .when(self.search_cursor == Some(key), |d| d.bg(rgb(0x164a7b)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(entry.item.label)
                            .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
                                if enabled {
                                    key.category.to_string()
                                } else {
                                    format!("{} · Unavailable in the current context", key.category)
                                },
                            )),
                    )
                    .child(div().text_color(rgb(ui::MUTED)).child(entry.item.shortcut))
                    .on_click(cx.listener(move |this, _, w, cx| this.execute_search(key, w, cx))),
            );
        }
        if count == 0 {
            list = list.child(
                div()
                    .p_3()
                    .text_color(rgb(ui::MUTED))
                    .child("No matching commands. Try a menu name, command or shortcut."),
            );
        }
        div().absolute().inset_0().flex().justify_center().items_start().pt(px(75.0))
            .bg(gpui::rgba(0x00000080)).occlude()
            .child(div().id("command-search-dialog").w(px(590.0)).max_w_full().p_3()
                .flex().flex_col().gap_2().bg(rgb(ui::PANEL)).border_1().border_color(rgb(ui::BLUE)).shadow_lg().occlude()
                .on_mouse_down_out(cx.listener(|this, _, w, cx| this.close_search(w, cx)))
                .on_key_down(|_, _, cx| cx.stop_propagation())
                .child(div().flex().justify_between().child("Find command").child(ui::text_button("close-command-search", "Close · Esc")
                    .on_click(cx.listener(|this, _, w, cx| this.close_search(w, cx)))))
                .child(self.search_field.clone()).child(list)
                .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(format!("{count} commands · ↑/↓ select · Enter run · Ctrl+Home/End first/last"))))
    }
}
