use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, Content, TextAlign};

pub(crate) struct Character {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
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
        Self { state, fields }
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
        panel = panel.child(
            div()
                .text_color(rgb(ui::TEXT))
                .child("Wanted Sans · Regular"),
        );
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
                        s.dispatch(&Action::Edit(Command::SetTextStyle { id, style }), w, cx)
                    })
                })
            }),
        );
    }
    panel
}
