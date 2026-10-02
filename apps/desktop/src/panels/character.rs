use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, Content, TextAlign};
use std::{cell::Cell, rc::Rc};

pub(crate) struct Character {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
    font_search: Entity<TextField>,
    fonts_open: bool,
    styles_open: bool,
    picker_bounds: [Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>; 2],
    picker_focus: gpui::FocusHandle,
}
impl Character {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let fields = (0..4)
            .map(|i| {
                let state = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, w, cx| {
                        state.update(cx, |s, cx| {
                            let Some(l) = s.editor.selected_layer() else {
                                return;
                            };
                            if l.locked() {
                                return;
                            }
                            let Content::Text {
                                text: content,
                                font_size,
                            } = l.content()
                            else {
                                return;
                            };
                            let id = l.id();
                            let command = (|| -> Result<Command, String> {
                                if i == 3 {
                                    return Ok(Command::SetColor {
                                        id,
                                        color: ui::parse_hex_color(text)?.into(),
                                    });
                                }
                                let v = text
                                    .trim()
                                    .parse::<f64>()
                                    .map_err(|_| "Enter a finite number")?;
                                if i == 0 {
                                    return Ok(Command::SetContent {
                                        id,
                                        content: Content::Text {
                                            text: content.clone(),
                                            font_size: v,
                                        },
                                    });
                                }
                                let mut style = l.text_style();
                                if i == 1 {
                                    style.leading = v / font_size;
                                } else {
                                    style.tracking = v;
                                }
                                Ok(Command::SetTextStyle { id, style })
                            })();
                            match command {
                                Ok(c) => s.dispatch(&Action::Edit(c), w, cx),
                                Err(e) => {
                                    s.status = e;
                                    cx.notify();
                                }
                            }
                        })
                    })
                })
            })
            .collect();
        let font_search = cx.new(|cx| TextField::new(cx, |_, _, _| {}));
        cx.observe(&font_search, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            fields,
            font_search,
            fonts_open: false,
            styles_open: false,
            picker_bounds: std::array::from_fn(|_| Rc::new(Cell::new(None))),
            picker_focus: cx.focus_handle(),
        }
    }
    fn picker(
        &self,
        index: usize,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let position = self.picker_bounds[index]
            .get()
            .map_or(gpui::point(px(0.0), px(0.0)), |b| {
                gpui::point(b.left(), b.bottom())
            });
        gpui::deferred(
            gpui::anchored()
                .position(position)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    div()
                        .id(("character-picker", index))
                        .track_focus(&self.picker_focus)
                        .w(px(260.0))
                        .p_1()
                        .bg(rgb(ui::PANEL))
                        .border_1()
                        .border_color(rgb(ui::BLUE))
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.fonts_open = false;
                            this.styles_open = false;
                            cx.notify();
                        }))
                        .on_key_down(|_, _, cx| cx.stop_propagation())
                        .capture_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, w, cx| {
                            if e.keystroke.key == "escape" {
                                this.fonts_open = false;
                                this.styles_open = false;
                                w.blur();
                                cx.notify();
                                cx.stop_propagation();
                            }
                        }))
                        .child(content),
                ),
        )
        .with_priority(3)
    }
    fn choose_font(
        &mut self,
        family: Option<&str>,
        variant: Option<crate::fonts::Variant>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        TextField::commit_active(w, cx);
        self.state.update(cx, |s, cx| {
            let Some(l) = s
                .editor
                .selected_layer()
                .filter(|l| !l.locked() && matches!(l.content(), Content::Text { .. }))
            else {
                return;
            };
            let id = l.id();
            let mut style = l.text_style();
            if let Some(family) = family {
                style.font_family = family.into();
                style.font_face.clear();
                style = crate::fonts::resolved(&style);
            }
            if let Some(variant) = variant {
                style.weight = variant.weight;
                style.italic = variant.italic;
                style.font_face = variant.face;
            }
            s.dispatch(&Action::Edit(Command::SetTextStyle { id, style }), w, cx);
        });
        self.fonts_open = false;
        self.styles_open = false;
        w.blur();
        cx.notify();
    }
}
impl Render for Character {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div().p_3().flex().flex_col().gap_2();
        let Some(l) = self.state.read(cx).editor.selected_layer().cloned() else {
            return panel.child("Select a text layer.");
        };
        let Content::Text { font_size, .. } = l.content() else {
            return panel.child("Select a text layer.");
        };
        let style = l.text_style();
        let family_bounds = self.picker_bounds[0].clone();
        let style_bounds = self.picker_bounds[1].clone();
        panel = panel.child(
            font_button("text-font-family", format!("{} ▾", style.font_family))
                .relative()
                .child(
                    gpui::canvas(move |b, _, _| family_bounds.set(Some(b)), |_, _, _, _| ())
                        .absolute()
                        .size_full(),
                )
                .justify_start()
                .when(!l.locked(), |b| {
                    b.on_click(cx.listener(|this, _, w, cx| {
                        TextField::commit_active(w, cx);
                        this.fonts_open = !this.fonts_open;
                        this.styles_open = false;
                        w.focus(&this.picker_focus);
                        cx.notify();
                    }))
                }),
        );
        if self.fonts_open && !l.locked() {
            let query = self.font_search.read(cx).value().trim().to_lowercase();

            let mut choices = div()
                .id("font-family-choices")
                .max_h(px(200.0))
                .overflow_y_scroll()
                .flex()
                .flex_col();
            for (i, name) in crate::fonts::families()
                .iter()
                .filter(|n| n.to_lowercase().contains(&query))
                .enumerate()
            {
                let family = name.clone();
                choices = choices.child(
                    font_button(SharedString::from(format!("font-family-{i}")), name.clone())
                        .justify_start()
                        .flex_none()
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.choose_font(Some(&family), None, w, cx)
                        })),
                );
            }
            panel = panel.child(
                self.picker(
                    0,
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(self.font_search.clone())
                        .child(choices),
                    cx,
                ),
            );
        }
        panel = panel.child(
            font_button(
                "text-font-style",
                format!("{} ▾", crate::fonts::style_label(&style)),
            )
            .relative()
            .child(
                gpui::canvas(move |b, _, _| style_bounds.set(Some(b)), |_, _, _, _| ())
                    .absolute()
                    .size_full(),
            )
            .justify_start()
            .when(!l.locked(), |b| {
                b.on_click(cx.listener(|this, _, w, cx| {
                    TextField::commit_active(w, cx);
                    this.styles_open = !this.styles_open;
                    this.fonts_open = false;
                    w.focus(&this.picker_focus);
                    cx.notify();
                }))
            }),
        );
        if self.styles_open && !l.locked() {
            let mut choices = div()
                .id("font-style-choices")
                .max_h(px(200.0))
                .overflow_y_scroll()
                .flex()
                .flex_col();
            for (i, variant) in crate::fonts::variants(&style.font_family)
                .into_iter()
                .enumerate()
            {
                choices = choices.child(
                    font_button(
                        SharedString::from(format!("font-style-{i}")),
                        variant.label(),
                    )
                    .justify_start()
                    .flex_none()
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.choose_font(None, Some(variant.clone()), w, cx)
                    })),
                );
            }
            panel = panel.child(self.picker(1, choices, cx));
        }
        if let Some(warning) = crate::fonts::warning(&style) {
            panel = panel.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(0xffaa88))
                    .child(warning),
            );
        }
        let values = [
            font_size.to_string(),
            format!("{:.2}", l.text_style().leading * font_size),
            l.text_style().tracking.to_string(),
            format!("{:06X}", l.color()),
        ];
        for (i, label) in ["Font size (px)", "Leading (px)", "Tracking", "Fill (hex)"]
            .into_iter()
            .enumerate()
        {
            self.fields[i].update(cx, |f, _| f.sync(l.id().to_string(), values[i].clone(), w));
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .child(div().w(px(108.0)).child(label))
                    .when(i == 3, |d| {
                        d.child(super::color_picker::swatch(
                            "text-fill-color",
                            l.color(),
                            crate::color_edit::Target::Fill(l.id()),
                            l.locked(),
                            &self.state,
                        ))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(!l.locked(), |d| d.child(self.fields[i].clone()))
                            .when(l.locked(), |d| d.child(values[i].clone())),
                    ),
            );
        }
        panel
    }
}
pub(crate) fn paragraph(state: &Entity<EditorState>, cx: &Context<super::Sidebar>) -> gpui::Div {
    let layer = state
        .read(cx)
        .editor
        .selected_layer()
        .filter(|l| matches!(l.content(), Content::Text { .. }))
        .cloned();
    let mut panel = div().p_3().flex().gap_2();
    let Some(l) = layer else {
        return panel.child("Select a text layer.");
    };
    for (align, icon, label) in [
        (TextAlign::Left, "text-align-left", "Align text left"),
        (TextAlign::Center, "text-align-center", "Center text"),
        (TextAlign::Right, "text-align-right", "Align text right"),
    ] {
        let mut style = l.text_style();
        style.align = align;
        let id = l.id();
        let state = state.clone();
        panel = panel.child(
            ui::tool(icon, icon, label, l.text_style().align == align).when(!l.locked(), |b| {
                b.on_click(move |_, w, cx| {
                    state.update(cx, |s, cx| {
                        s.dispatch(
                            &Action::Edit(Command::SetTextStyle {
                                id,
                                style: style.clone(),
                            }),
                            w,
                            cx,
                        )
                    })
                })
            }),
        );
    }
    panel
}

fn font_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    ui::text_button(id, label)
        .truncate()
        .on_key_down(|e, _, cx| {
            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
            }
        })
}
