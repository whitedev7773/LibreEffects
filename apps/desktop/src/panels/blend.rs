use crate::{
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, Pixels, Point, Window, anchored, canvas, deferred, div,
    point, prelude::*, px, rgb,
};
use libre_effects_core::{BlendMode, Command, Content, LayerId};
use std::{cell::Cell, rc::Rc};

pub(crate) struct BlendPicker {
    state: Entity<EditorState>,
    id: LayerId,
    open: bool,
    cursor: usize,
    focus: FocusHandle,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}
impl BlendPicker {
    pub(crate) fn new(state: Entity<EditorState>, id: LayerId, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            this.open = false;
            cx.notify();
        })
        .detach();
        Self {
            state,
            id,
            open: false,
            cursor: 0,
            focus: cx.focus_handle(),
            bounds: Default::default(),
        }
    }
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        window.focus(&self.focus);
        self.state.update(cx, |s, cx| {
            s.dispatch(
                &Action::Edit(Command::SetBlendMode {
                    id: self.id,
                    mode: BlendMode::ALL[index],
                }),
                window,
                cx,
            )
        });
        cx.notify();
    }
}
impl Render for BlendPicker {
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
        let mode = layer.blend_mode();
        let disabled = layer.locked() || matches!(layer.content(), Content::Null);
        let label = if matches!(layer.content(), Content::Null) {
            "—"
        } else {
            mode.label()
        };
        let current = BlendMode::ALL.iter().position(|m| *m == mode).unwrap();
        let bounds = self.bounds.clone();
        let mut element = div()
            .relative()
            .w_full()
            .h(px(23.0))
            .child(
                ui::text_button("blend-picker", format!("{label} ▾"))
                    .track_focus(&self.focus)
                    .tab_index(0)
                    .w_full()
                    .h_full()
                    .justify_start()
                    .when(disabled, |s| s.opacity(0.4))
                    .on_click(cx.listener(move |this, event, window, cx| {
                        if !disabled {
                            window.focus(&this.focus);
                            if this.open && matches!(event, gpui::ClickEvent::Keyboard(_)) {
                                this.choose(this.cursor, window, cx);
                            } else {
                                this.open = !this.open;
                                this.cursor = current;
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
                                    this.cursor = (this.cursor + 1) % BlendMode::ALL.len();
                                }
                            }
                            "up" => {
                                if !this.open {
                                    this.cursor = current;
                                    this.open = true;
                                } else {
                                    this.cursor = (this.cursor + BlendMode::ALL.len() - 1)
                                        % BlendMode::ALL.len();
                                }
                            }
                            _ => return,
                        }
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
                .id("blend-menu")
                .w(px(135.0))
                .py_1()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(ui::BLUE))
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.open = false;
                    cx.notify();
                }));
            for (index, choice) in BlendMode::ALL.into_iter().enumerate() {
                menu = menu.child(
                    ui::text_button(
                        ("blend-option", index),
                        format!(
                            "{} {}",
                            if choice == mode { "✓" } else { "  " },
                            choice.label()
                        ),
                    )
                    .w_full()
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
