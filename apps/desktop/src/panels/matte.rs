use crate::{
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, Pixels, Point, Window, anchored, canvas, deferred, div,
    point, prelude::*, px, rgb,
};
use libre_effects_core::{Command, Content, LayerId, MatteMode, TrackMatte};
use std::{cell::Cell, rc::Rc};

pub(crate) struct MattePicker {
    source_picker: bool,
    scroll: gpui::ScrollHandle,
    state: Entity<EditorState>,
    id: LayerId,
    open: bool,
    cursor: usize,
    focus: FocusHandle,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}
impl MattePicker {
    pub(crate) fn new(
        state: Entity<EditorState>,
        id: LayerId,
        source_picker: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |this, _, cx| {
            this.open = false;
            cx.notify();
        })
        .detach();
        Self {
            source_picker,
            scroll: Default::default(),
            state,
            id,
            open: false,
            cursor: 0,
            focus: cx.focus_handle(),
            bounds: Default::default(),
        }
    }
    fn choices(&self, cx: &Context<Self>) -> Vec<(Option<TrackMatte>, String)> {
        let comp = self.state.read(cx).editor.project().composition();
        let matte = comp.layer(self.id).and_then(|l| l.track_matte());
        if self.source_picker {
            std::iter::once((None, "None".to_string()))
                .chain(
                    comp.layers()
                        .iter()
                        .enumerate()
                        .filter(|(_, l)| comp.can_track_matte(self.id, l.id()))
                        .map(|(index, l)| {
                            (
                                Some(TrackMatte {
                                    source: l.id(),
                                    mode: matte.map_or(MatteMode::Alpha, |m| m.mode),
                                }),
                                format!("{} · {}", index + 1, l.name()),
                            )
                        }),
                )
                .collect()
        } else {
            MatteMode::ALL
                .into_iter()
                .map(|mode| (matte.map(|m| TrackMatte { mode, ..m }), mode.label().into()))
                .collect()
        }
    }
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((matte, _)) = self.choices(cx).get(index).cloned() else {
            return;
        };
        self.open = false;
        window.focus(&self.focus);
        self.state.update(cx, |s, cx| {
            s.dispatch(
                &Action::Edit(Command::SetTrackMatte { id: self.id, matte }),
                window,
                cx,
            )
        });
        cx.notify();
    }
}
impl Render for MattePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(layer) = self
            .state
            .read(cx)
            .editor
            .project()
            .composition()
            .layer(self.id)
        else {
            return div();
        };
        let matte = layer.track_matte();
        let disabled = layer.locked()
            || matches!(layer.content(), Content::Null)
            || (!self.source_picker && matte.is_none());
        let choices = self.choices(cx);
        let current = choices.iter().position(|(m, _)| *m == matte).unwrap_or(0);
        let label = choices
            .get(current)
            .map_or("None", |(_, label)| label.as_str());
        let count = choices.len();
        let bounds = self.bounds.clone();
        let mut element = div()
            .relative()
            .w_full()
            .h(px(23.0))
            .child(
                ui::text_button("matte-picker", format!("{label} ▾"))
                    .track_focus(&self.focus)
                    .tab_index(0)
                    .w_full()
                    .h_full()
                    .justify_start()
                    .overflow_hidden()
                    .when(disabled, |s| s.opacity(0.4))
                    .on_click(cx.listener(move |this, event, window, cx| {
                        if !disabled {
                            window.focus(&this.focus);
                            if this.open && matches!(event, gpui::ClickEvent::Keyboard(_)) {
                                this.choose(this.cursor, window, cx);
                            } else {
                                this.open = !this.open;
                                this.cursor = current;
                                this.scroll.scroll_to_item(current);
                            }
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if disabled || event.keystroke.modifiers.modified() {
                            return;
                        }
                        match event.keystroke.key.as_str() {
                            "escape" => this.open = false,
                            "enter" | "space" => {
                                // GPUI activates focused buttons on key-up. Consume key-down
                                // so Space does not also start timeline playback.
                            }
                            "down" => {
                                if !this.open {
                                    this.cursor = current;
                                    this.open = true;
                                } else {
                                    this.cursor = (this.cursor + 1) % count;
                                }
                            }
                            "up" => {
                                if !this.open {
                                    this.cursor = current;
                                    this.open = true;
                                } else {
                                    this.cursor = (this.cursor + count - 1) % count;
                                }
                            }
                            _ => return,
                        }
                        this.scroll.scroll_to_item(this.cursor);
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| ())
                    .absolute()
                    .size_full(),
            );
        if self.open && !disabled {
            let position: Point<Pixels> = self
                .bounds
                .get()
                .map_or(point(px(0.0), px(0.0)), |b| point(b.left(), b.bottom()));
            let mut menu = div()
                .id("matte-menu")
                .w(px(if self.source_picker { 235.0 } else { 160.0 }))
                .max_h(px(230.0))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .py_1()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(ui::BLUE))
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.open = false;
                    cx.notify();
                }));
            for (index, (choice, label)) in choices.into_iter().enumerate() {
                menu = menu.child(
                    ui::text_button(
                        ("matte-option", index),
                        format!("{} {}", if choice == matte { "✓" } else { "  " }, label),
                    )
                    .w_full()
                    .flex_none()
                    .overflow_hidden()
                    .justify_start()
                    .when(index == self.cursor, |s| s.bg(rgb(0x344455)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose(index, window, cx);
                        cx.stop_propagation();
                    })),
                );
            }
            element = element.child(
                deferred(
                    anchored()
                        .position(position)
                        .snap_to_window_with_margin(px(8.0))
                        .child(menu),
                )
                .with_priority(3),
            );
        }
        element
    }
}
