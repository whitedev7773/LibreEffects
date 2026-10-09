use crate::ui;
use gpui::{
    App, Context, FocusHandle, Pixels, Point, Window, anchored, deferred, div, prelude::*, px, rgb,
};
use std::rc::Rc;

/// Explicit, keyboard navigable choices. Opening and changing a choice never
/// propagates Space to the application's playback shortcut.
pub(crate) struct Choice {
    label: &'static str,
    compact: bool,
    anchor: Rc<std::cell::Cell<Option<Point<Pixels>>>>,
    options: Vec<String>,
    selected: usize,
    cursor: usize,
    position: Option<Point<Pixels>>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    select: Rc<dyn Fn(usize, &mut Window, &mut App)>,
}
impl Choice {
    pub fn new(
        label: &'static str,
        options: impl IntoIterator<Item = String>,
        selected: usize,
        cx: &mut Context<Self>,
        select: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label,
            compact: false,
            anchor: Default::default(),
            options: options.into_iter().collect(),
            selected,
            cursor: selected,
            position: None,
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            return_focus: None,
            select: Rc::new(select),
        }
    }
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
    pub fn sync(&mut self, selected: usize) {
        self.selected = selected.min(self.options.len().saturating_sub(1));
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.position = None;
        if let Some(focus) = self.return_focus.take() {
            window.focus(&focus);
        }
        cx.notify();
    }
    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.cursor;
        let select = self.select.clone();
        self.close(window, cx);
        select(index, window, cx);
    }
}
impl Render for Choice {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let anchor = self.anchor.clone();
        let mut root = div()
            .id("choice")
            .relative()
            .child(
                gpui::canvas(
                    move |bounds, _, _| anchor.set(Some(bounds.bottom_left())),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(ui::TEXT))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if event.keystroke.modifiers.modified() {
                    return;
                }
                if this.position.is_some() {
                    match key {
                        "up" => this.cursor = this.cursor.saturating_sub(1),
                        "down" => {
                            this.cursor =
                                (this.cursor + 1).min(this.options.len().saturating_sub(1))
                        }
                        "home" => this.cursor = 0,
                        "end" => this.cursor = this.options.len().saturating_sub(1),
                        "enter" | "space" => this.choose(window, cx),
                        "escape" | "tab" => this.close(window, cx),
                        _ => {}
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                } else if matches!(key, "down" | "up" | "enter" | "space") {
                    this.return_focus = window.focused(cx);
                    window.focus(&this.focus);
                    this.cursor = this.selected;
                    this.position = this.anchor.get();
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .capture_key_up(cx.listener(|_, event: &gpui::KeyUpEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    // Prevent a synthetic button click after key-down handled
                    // the choice, including after focus returns to the toggle.
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .when(!self.compact, |d| {
                d.child(div().text_color(rgb(ui::MUTED)).child(self.label))
            })
            .child(
                ui::text_button(
                    "choice-toggle",
                    self.options.get(self.selected).cloned().unwrap_or_default(),
                )
                .h(px(22.0))
                .w_full()
                .justify_between()
                .border_1()
                .border_color(rgb(ui::BORDER))
                .child(ui::icon("chevron-down"))
                .on_click(cx.listener(
                    |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        if this.position.is_some() {
                            this.close(window, cx);
                        } else {
                            this.return_focus = window.focused(cx);
                            window.focus(&this.focus);
                            this.cursor = this.selected;
                            this.position = Some(event.position());
                            cx.notify();
                        }
                    },
                )),
            );
        if let Some(position) = self.position {
            let mut menu = div()
                .id("choice-menu")
                .occlude()
                .w(px(218.0))
                .max_h(px(240.0))
                .overflow_y_scroll()
                .p_1()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(ui::BLUE))
                .shadow_md()
                .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close(window, cx)));
            for (index, label) in self.options.iter().enumerate() {
                menu = menu.child(
                    ui::text_button(("choice-option", index), label.clone())
                        .w_full()
                        .justify_start()
                        .h(px(24.0))
                        .when(index == self.cursor, |d| d.bg(rgb(0x344455)))
                        .when(index == self.selected, |d| d.text_color(rgb(ui::BLUE)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.cursor = index;
                            this.choose(window, cx);
                            cx.stop_propagation();
                        })),
                );
            }
            root = root.child(
                deferred(
                    anchored()
                        .position(position)
                        .snap_to_window_with_margin(px(8.0))
                        .child(menu),
                )
                .with_priority(2),
            );
        }
        root
    }
}
