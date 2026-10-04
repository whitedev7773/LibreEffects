use crate::color_edit::InputTarget;
use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{
    Command, Content, Frame, Layer, PropertyPath, TextAlign, TextPaint, TextParam, TrackEdit,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn field_command(
    layer: &Layer,
    frame: Frame,
    index: usize,
    text: &str,
) -> Result<Option<Command>, String> {
    if layer.locked() {
        return Err("Unlock the text layer before editing".into());
    }
    if !matches!(layer.content(), Content::Text { .. }) {
        return Err("Select a text layer".into());
    }
    if index == 3 || index == 5 {
        return crate::color_edit::text_hex_command(
            layer,
            if index == 3 {
                TextPaint::Fill
            } else {
                TextPaint::Stroke
            },
            frame,
            text,
        );
    }
    let parameter = field_parameter(index).ok_or("Unknown text field")?;
    let value = text
        .trim()
        .parse::<f64>()
        .map_err(|_| "Enter a finite number")?;
    layer.text_value_command(parameter, value, frame)
}

fn field_parameter(index: usize) -> Option<TextParam> {
    match index {
        0 => Some(TextParam::FontSize),
        1 => Some(TextParam::Leading),
        2 => Some(TextParam::Tracking),
        4 => Some(TextParam::StrokeWidth),
        _ => None,
    }
}

fn scalar_animation_command(layer: &Layer, parameter: TextParam, frame: Frame) -> Command {
    Command::EditText {
        id: layer.id(),
        parameter,
        edit: TrackEdit::ToggleAnimation { frame },
    }
}

/// Flush pending input before sampling the scalar's independent stopwatch.
/// The frozen target rejects stale controls; only that synchronous field commit
/// may change the document before we replan against the current base and tracks.
pub(super) fn scalar_watch(
    state: &Entity<EditorState>,
    layer: &Layer,
    parameter: TextParam,
    target: Option<InputTarget>,
    scope: &'static str,
) -> impl IntoElement {
    let control = format!("{scope}-{parameter:?}-watch");
    let id = layer.id();
    let state = state.clone();
    let button = ui::tool(
        SharedString::from(control.clone()),
        "stopwatch",
        format!("Toggle {} animation", parameter.label()),
        layer
            .track(PropertyPath::Text(parameter))
            .is_some_and(|t| !t.keys().is_empty()),
    );
    crate::color_edit::input_pointer_button(button, control.clone(), target.clone()).on_click(
        move |event, w, cx| {
            cx.stop_propagation();
            let Some(target) =
                crate::color_edit::input_click_target(&control, event, &target, &state, w, cx)
            else {
                return;
            };
            if state.read(cx).editor.selected() != Some(id) {
                return;
            }
            TextField::commit_active(w, cx);
            state.update(cx, |s, cx| {
                if !target.same_context(s) {
                    return;
                }
                s.finish_text(true, cx);
                if !target.same_context(s) {
                    return;
                }
                let Some(layer) = s
                    .editor
                    .selected_layer()
                    .filter(|l| matches!(l.content(), Content::Text { .. }))
                else {
                    return;
                };
                let command = scalar_animation_command(layer, parameter, s.frame);
                s.dispatch(&Action::Edit(command), w, cx);
            });
        },
    )
}

pub(super) fn color_watch(
    state: &Entity<EditorState>,
    layer: &Layer,
    paint: TextPaint,
    frame: Frame,
) -> impl IntoElement {
    ui::action_tool(
        SharedString::from(format!("text-{paint:?}-color-watch")),
        "stopwatch",
        "Toggle text color animation",
        state,
        Action::Edit(
            layer
                .text_color_animation_command(paint, frame)
                .unwrap_or_else(|_| Command::Batch(vec![])),
        ),
        layer.text_color_animated(paint),
    )
}

pub(crate) struct Character {
    state: Entity<EditorState>,
    input_source: Option<InputTarget>,
    input_targets: Vec<Rc<RefCell<Option<InputTarget>>>>,
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
        let input_targets: Vec<Rc<RefCell<Option<InputTarget>>>> =
            (0..6).map(|_| Default::default()).collect();
        let fields = (0..6)
            .map(|i| {
                let state = state.clone();
                let target = input_targets[i].clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, w, cx| {
                        state.update(cx, |s, cx| {
                            if !target.borrow().as_ref().is_some_and(|t| t.current(s)) {
                                return;
                            }
                            s.finish_text(true, cx);
                            if !target.borrow().as_ref().is_some_and(|t| t.same_context(s)) {
                                return;
                            }
                            let Some(l) = s.editor.selected_layer() else {
                                return;
                            };
                            let command = field_command(l, s.frame, i, text);
                            match command {
                                Ok(Some(c)) => s.dispatch(&Action::Edit(c), w, cx),
                                Ok(None) => {}
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
            input_source: None,
            input_targets,
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
        let frame = self.state.read(cx).frame;
        let mut panel = div().p_3().flex().flex_col().gap_2();
        let Some(l) = self.state.read(cx).editor.selected_layer().cloned() else {
            return panel.child("Select a text layer.");
        };
        let Some(typography) = l.text_typography_at(frame) else {
            return panel.child("Select a text layer.");
        };
        InputTarget::refresh(&mut self.input_source, self.state.read(cx));
        let binding = self
            .input_source
            .as_ref()
            .map(InputTarget::binding)
            .unwrap_or_default();
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
        let fill_color = l.text_color_at(TextPaint::Fill, frame).unwrap();
        let stroke_color = l.text_color_at(TextPaint::Stroke, frame).unwrap();
        let values = [
            typography.font_size.to_string(),
            typography.leading.to_string(),
            typography.tracking.to_string(),
            format!("{fill_color:06X}"),
            l.text_value_at(TextParam::StrokeWidth, frame)
                .unwrap()
                .to_string(),
            format!("{stroke_color:06X}"),
        ];
        for (i, label) in [
            "Font size (px)",
            "Leading (× font size)",
            "Tracking (1/1000 em)",
            "Fill (hex)",
            "Stroke (px)",
            "Stroke (hex)",
        ]
        .into_iter()
        .enumerate()
        {
            *self.input_targets[i].borrow_mut() = self.input_source.clone();
            self.fields[i].update(cx, |f, _| {
                if field_parameter(i).is_some() {
                    f.set_numeric();
                }
                f.sync(binding.clone(), values[i].clone(), w);
            });
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .w(px(155.0))
                            .flex()
                            .items_center()
                            .when(!l.locked() && (i == 3 || i == 5), |d| {
                                d.child(color_watch(
                                    &self.state,
                                    &l,
                                    if i == 3 {
                                        TextPaint::Fill
                                    } else {
                                        TextPaint::Stroke
                                    },
                                    frame,
                                ))
                            })
                            .when(!l.locked() && field_parameter(i).is_some(), |d| {
                                d.child(scalar_watch(
                                    &self.state,
                                    &l,
                                    field_parameter(i).unwrap(),
                                    self.input_source.clone(),
                                    "character",
                                ))
                            })
                            .child(label),
                    )
                    .when(i == 3, |d| {
                        d.child(super::color_picker::swatch(
                            "text-fill-color",
                            fill_color,
                            crate::color_edit::Target::Fill(l.id()),
                            l.locked(),
                            &self.state,
                        ))
                    })
                    .when(i == 5, |d| {
                        d.child(super::color_picker::swatch(
                            "text-stroke-color",
                            stroke_color,
                            crate::color_edit::Target::Stroke(l.id()),
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
        let mut switches = div().flex().gap_2();
        for (fill, enabled, label) in [
            (true, style.fill_enabled, "Fill"),
            (false, style.stroke_enabled, "Stroke"),
        ] {
            let state = self.state.clone();
            switches = switches.child(
                ui::text_button(
                    label,
                    format!("{label}: {}", if enabled { "On" } else { "Off" }),
                )
                .when(enabled, |b| b.text_color(rgb(ui::BLUE)))
                .when(!l.locked(), |b| {
                    b.on_click(move |_, w, cx| {
                        TextField::commit_active(w, cx);
                        state.update(cx, |s, cx| {
                            let Some(l) = s.editor.selected_layer() else {
                                return;
                            };
                            let mut style = l.text_style();
                            if fill {
                                style.fill_enabled = !style.fill_enabled;
                            } else {
                                style.stroke_enabled = !style.stroke_enabled;
                            }
                            s.dispatch(
                                &Action::Edit(Command::SetTextStyle { id: l.id(), style }),
                                w,
                                cx,
                            );
                        });
                    })
                }),
            );
        }
        let state = self.state.clone();
        let order = if style.stroke_over_fill {
            "All strokes over fills"
        } else {
            "All fills over strokes"
        };
        let join_state = self.state.clone();
        panel
            .child(switches)
            .child(
                ui::text_button("text-paint-order", format!("{order} ▾")).when(!l.locked(), |b| {
                    b.on_click(move |_, w, cx| {
                        TextField::commit_active(w, cx);
                        state.update(cx, |s, cx| {
                            let Some(l) = s.editor.selected_layer() else {
                                return;
                            };
                            let mut style = l.text_style();
                            style.stroke_over_fill = !style.stroke_over_fill;
                            s.dispatch(
                                &Action::Edit(Command::SetTextStyle { id: l.id(), style }),
                                w,
                                cx,
                            );
                        });
                    })
                }),
            )
            .child(
                ui::text_button(
                    "text-stroke-join",
                    format!("Join: {:?} ▾", style.stroke_join),
                )
                .when(!l.locked(), |b| {
                    b.on_click(move |_, w, cx| {
                        TextField::commit_active(w, cx);
                        join_state.update(cx, |s, cx| {
                            let Some(l) = s.editor.selected_layer() else {
                                return;
                            };
                            let mut style = l.text_style();
                            use libre_effects_core::TextStrokeJoin::*;
                            style.stroke_join = match style.stroke_join {
                                Miter => Round,
                                Round => Bevel,
                                Bevel => Miter,
                            };
                            s.dispatch(
                                &Action::Edit(Command::SetTextStyle { id: l.id(), style }),
                                w,
                                cx,
                            );
                        });
                    })
                }),
            )
    }
}
#[derive(Clone, Copy)]
enum ParagraphEdit {
    Align(TextAlign),
    Mode(bool),
    FitHeight,
}

fn paragraph_command(
    layer: &Layer,
    frame: Frame,
    edit: ParagraphEdit,
) -> Result<Option<Command>, String> {
    if layer.locked() || !matches!(layer.content(), Content::Text { .. }) {
        return Err("Select an unlocked text layer".into());
    }
    match edit {
        ParagraphEdit::Align(align) => {
            let mut style = layer.text_style();
            if style.align == align {
                return Ok(None);
            }
            style.align = align;
            Ok(Some(Command::SetTextStyle {
                id: layer.id(),
                style,
            }))
        }
        ParagraphEdit::Mode(paragraph) => Ok((layer.text_style().paragraph != paragraph)
            .then(|| crate::text_flow::convert(layer, paragraph, frame))),
        ParagraphEdit::FitHeight => {
            let height = crate::text_flow::fit_height(layer, frame)
                .ok_or("Select paragraph text to fit its box")?
                .ceil();
            if !height.is_finite() || !(1.0..=16384.0).contains(&height) {
                return Err("Fitted text height must be from 1 to 16384 px".into());
            }
            Ok((height != layer.height()).then_some(Command::SetTextBox {
                id: layer.id(),
                width: layer.width(),
                height,
            }))
        }
    }
}

fn dispatch_paragraph(
    state: &Entity<EditorState>,
    target: &Option<InputTarget>,
    edit: ParagraphEdit,
    control: &str,
    event: &gpui::ClickEvent,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let Some(target) =
        crate::color_edit::input_click_target(control, event, target, state, window, cx)
    else {
        return;
    };
    TextField::commit_active(window, cx);
    state.update(cx, |s, cx| {
        if !target.same_context(s) {
            return;
        }
        s.finish_text(true, cx);
        if !target.same_context(s) {
            return;
        }
        let Some(layer) = s.editor.selected_layer() else {
            return;
        };
        match paragraph_command(layer, s.frame, edit) {
            Ok(Some(command)) => s.dispatch(&Action::Edit(command), window, cx),
            Ok(None) => {}
            Err(error) => {
                s.status = error;
                cx.notify();
            }
        }
    });
}

pub(crate) struct Paragraph {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
    input_source: Option<InputTarget>,
    input_targets: Vec<Rc<RefCell<Option<InputTarget>>>>,
}
impl Paragraph {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let input_targets: Vec<Rc<RefCell<Option<InputTarget>>>> =
            (0..2).map(|_| Default::default()).collect();
        let fields = (0..2)
            .map(|index| {
                let state = state.clone();
                let target = input_targets[index].clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, w, cx| {
                        state.update(cx, |s, cx| {
                            if !target.borrow().as_ref().is_some_and(|t| t.current(s)) {
                                return;
                            }
                            s.finish_text(true, cx);
                            if !target.borrow().as_ref().is_some_and(|t| t.same_context(s)) {
                                return;
                            }
                            let Some(layer) = s.editor.selected_layer().filter(|l| {
                                matches!(l.content(), Content::Text { .. })
                                    && l.text_style().paragraph
                            }) else {
                                return;
                            };
                            let Ok(value) = text.trim().parse::<f64>() else {
                                s.status = "Enter a box size in pixels".into();
                                cx.notify();
                                return;
                            };
                            let command = Command::SetTextBox {
                                id: layer.id(),
                                width: if index == 0 { value } else { layer.width() },
                                height: if index == 1 { value } else { layer.height() },
                            };
                            s.dispatch(&Action::Edit(command), w, cx);
                        })
                    })
                    .numeric()
                })
            })
            .collect();
        Self {
            state,
            fields,
            input_source: None,
            input_targets,
        }
    }
}
impl Render for Paragraph {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        InputTarget::refresh(&mut self.input_source, state);
        let binding = self
            .input_source
            .as_ref()
            .map(InputTarget::binding)
            .unwrap_or_default();
        let frame = state.frame;
        let layer = state
            .editor
            .selected_layer()
            .filter(|l| matches!(l.content(), Content::Text { .. }))
            .cloned();
        let mut panel = div().p_3().flex().flex_col().gap_2().text_size(px(11.0));
        let Some(l) = layer else {
            return panel.child("Select a text layer.");
        };
        let mut alignment = div().flex().gap_2();
        for (align, icon, label) in [
            (TextAlign::Left, "text-align-left", "Align text left"),
            (TextAlign::Center, "text-align-center", "Center text"),
            (TextAlign::Right, "text-align-right", "Align text right"),
        ] {
            let state = self.state.clone();
            let target = self.input_source.clone();
            alignment = alignment.child(
                crate::color_edit::input_pointer_button(
                    ui::tool(icon, icon, label, l.text_style().align == align),
                    icon.into(),
                    target.clone(),
                )
                .when(!l.locked(), |b| {
                    b.on_click(move |event, w, cx| {
                        dispatch_paragraph(
                            &state,
                            &target,
                            ParagraphEdit::Align(align),
                            icon,
                            event,
                            w,
                            cx,
                        )
                    })
                }),
            );
        }
        panel = panel.child(alignment);
        let mut modes = div().flex().gap_2();
        for (paragraph, label) in [(false, "Point text"), (true, "Paragraph text")] {
            let state = self.state.clone();
            let target = self.input_source.clone();
            modes = modes.child(
                crate::color_edit::input_pointer_button(
                    ui::text_button(label, label),
                    label.into(),
                    target.clone(),
                )
                .when(l.text_style().paragraph == paragraph, |b| {
                    b.text_color(rgb(ui::BLUE))
                })
                .when(!l.locked(), |b| {
                    b.on_click(move |event, w, cx| {
                        dispatch_paragraph(
                            &state,
                            &target,
                            ParagraphEdit::Mode(paragraph),
                            label,
                            event,
                            w,
                            cx,
                        )
                    })
                }),
            );
        }
        panel = panel.child(modes);
        if l.text_style().paragraph {
            for (i, label, value) in [
                (0, "Box width (px)", l.width()),
                (1, "Box height (px)", l.height()),
            ] {
                *self.input_targets[i].borrow_mut() = self.input_source.clone();
                self.fields[i].update(cx, |f, _| f.sync(binding.clone(), value.to_string(), w));
                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .child(div().w(px(108.0)).child(label))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .when(l.locked(), |d| d.child(value.to_string()))
                                .when(!l.locked(), |d| d.child(self.fields[i].clone())),
                        ),
                );
            }
            if let Some(lines) = crate::text_flow::layer_lines(&l, frame) {
                let needed = crate::text_flow::fit_height(&l, frame).unwrap().ceil();
                if crate::text_flow::composed_count(&lines, l.height()) < lines.len() {
                    let state = self.state.clone();
                    let target = self.input_source.clone();
                    panel = panel
                        .child(
                            div()
                                .text_color(rgb(0xffaa88))
                                .child("Overflow: Point conversion removes hidden text"),
                        )
                        .when(needed > l.height() && needed <= 16384.0, |panel| {
                            panel.child(
                                crate::color_edit::input_pointer_button(
                                    ui::text_button("fit-text-height", "Fit box height"),
                                    "fit-text-height".into(),
                                    target.clone(),
                                )
                                .when(!l.locked(), |b| {
                                    b.on_click(move |event, w, cx| {
                                        dispatch_paragraph(
                                            &state,
                                            &target,
                                            ParagraphEdit::FitHeight,
                                            "fit-text-height",
                                            event,
                                            w,
                                            cx,
                                        )
                                    })
                                }),
                            )
                        });
                }
            }
        }
        panel
    }
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

#[cfg(test)]
mod text_paint_controls_tests {
    use super::*;
    use libre_effects_core::Editor;

    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Text".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 100.,
            name: "Title".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0x102030,
        })
        .unwrap();
        e
    }
    #[test]
    fn text_paint_width_field_is_animated_bounded_and_format_only_input_is_noop() {
        let mut e = scene();
        let before = e.project().clone();
        let width = e
            .selected_layer()
            .unwrap()
            .text_value_at(TextParam::StrokeWidth, 30)
            .unwrap();
        assert!(
            field_command(e.selected_layer().unwrap(), 30, 4, &format!("{width:.4}"))
                .unwrap()
                .is_none()
        );
        assert_eq!(e.project(), &before);
        e.execute(Command::EditText {
            id: 1,
            parameter: TextParam::StrokeWidth,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        let command = field_command(e.selected_layer().unwrap(), 60, 4, "12")
            .unwrap()
            .unwrap();
        e.execute(command).unwrap();
        let mid = (width + 12.) / 2.;
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .text_value_at(TextParam::StrokeWidth, 0),
            Some(width)
        );
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .text_value_at(TextParam::StrokeWidth, 30),
            Some(mid)
        );
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .text_value_at(TextParam::StrokeWidth, 60),
            Some(12.)
        );
        let before = e.project().clone();
        assert!(
            field_command(e.selected_layer().unwrap(), 30, 4, &format!("{mid:.3}"))
                .unwrap()
                .is_none()
        );
        assert_eq!(e.project(), &before);
        for bad in ["-1", "1000.1", "NaN", "inf", "invalid"] {
            assert!(field_command(e.selected_layer().unwrap(), 30, 4, bad).is_err());
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(field_command(e.selected_layer().unwrap(), 30, 4, "4").is_err());
    }
    #[test]
    fn text_paint_character_hex_and_typography_preserve_base_style_and_tracks() {
        let mut e = scene();
        let command = e
            .selected_layer()
            .unwrap()
            .text_color_animation_command(TextPaint::Stroke, 0)
            .unwrap();
        e.execute(command).unwrap();
        let command = field_command(e.selected_layer().unwrap(), 60, 5, "abcdef")
            .unwrap()
            .unwrap();
        e.execute(command).unwrap();
        let before_style = e.selected_layer().unwrap().text_style();
        let before_color = e
            .selected_layer()
            .unwrap()
            .text_color_at(TextPaint::Stroke, 30);
        let tracks: Vec<_> = TextPaint::Stroke
            .channels()
            .into_iter()
            .map(|p| {
                e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(p))
                    .unwrap()
                    .clone()
            })
            .collect();
        for (index, text) in [(0, "60"), (1, "1.3333333333333333"), (2, "20")] {
            let command = field_command(e.selected_layer().unwrap(), 30, index, text)
                .unwrap()
                .unwrap();
            e.execute(command).unwrap();
        }
        let l = e.selected_layer().unwrap();
        assert_eq!(l.text_style().stroke_color, before_style.stroke_color);
        assert_eq!(l.text_style().stroke_width, before_style.stroke_width);
        assert_eq!(l.text_color_at(TextPaint::Stroke, 30), before_color);
        for (p, track) in TextPaint::Stroke.channels().into_iter().zip(tracks) {
            assert_eq!(l.track(PropertyPath::Text(p)), Some(&track));
        }
        assert!(field_command(l, 30, 3, " #102030 ").unwrap().is_none());
        e.execute(Command::AddSolid).unwrap();
        assert!(field_command(e.selected_layer().unwrap(), 30, 3, "abcdef").is_err());
    }
    #[test]
    fn typography_fields_share_sparse_scalar_planning_and_full_precision_noops() {
        let mut e = scene();
        for (index, parameter, value) in [
            (0, TextParam::FontSize, 53.125),
            (1, TextParam::Leading, 1.375),
            (2, TextParam::Tracking, -12.125),
        ] {
            assert_eq!(field_parameter(index), Some(parameter));
            let command = field_command(e.selected_layer().unwrap(), 17, index, &value.to_string())
                .unwrap()
                .unwrap();
            e.execute(command).unwrap();
            assert_eq!(
                e.selected_layer().unwrap().text_value_at(parameter, 17),
                Some(value)
            );
            assert!(
                e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(parameter))
                    .is_none()
            );
        }
        let base = e.project().clone();
        for (index, parameter, end) in [
            (0, TextParam::FontSize, 91.987654321),
            (1, TextParam::Leading, 2.987654321),
            (2, TextParam::Tracking, 134.987654321),
        ] {
            let command = scalar_animation_command(e.selected_layer().unwrap(), parameter, 0);
            e.execute(command).unwrap();
            let command = field_command(e.selected_layer().unwrap(), 60, index, &end.to_string())
                .unwrap()
                .unwrap();
            e.execute(command).unwrap();
            let sample = e
                .selected_layer()
                .unwrap()
                .text_value_at(parameter, 17)
                .unwrap();
            let before = e.project().clone();
            for input in [sample.to_string(), format!("  {sample:e}  ")] {
                assert!(
                    field_command(e.selected_layer().unwrap(), 17, index, &input)
                        .unwrap()
                        .is_none()
                );
            }
            assert_eq!(e.project(), &before);
            assert!(
                !e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(parameter))
                    .unwrap()
                    .keys()
                    .contains_key(&17)
            );
        }
        assert_eq!(
            e.selected_layer().unwrap().content(),
            base.composition().layer(1).unwrap().content()
        );
        assert_eq!(
            e.selected_layer().unwrap().text_style(),
            base.composition().layer(1).unwrap().text_style()
        );
    }

    #[test]
    fn typography_field_ranges_are_specific_finite_and_stopwatches_are_independent() {
        let mut e = scene();
        for (index, parameter, invalid) in [
            (0, TextParam::FontSize, ["0.999", "2048.001"]),
            (1, TextParam::Leading, ["0.0999", "10.001"]),
            (2, TextParam::Tracking, ["-1000.001", "10000.001"]),
        ] {
            for value in invalid.into_iter().chain(["NaN", "inf", "-inf", "bad"]) {
                assert!(field_command(e.selected_layer().unwrap(), 0, index, value).is_err());
            }
            let (min, max) = parameter.bounds();
            for value in [min, max] {
                assert!(
                    field_command(e.selected_layer().unwrap(), 0, index, &value.to_string())
                        .is_ok()
                );
            }
            // Disabling a stopwatch retains its materialized, keyless track.
            // Preserve every other channel exactly, whether sparse or retained.
            let other_tracks: Vec<_> =
                [TextParam::FontSize, TextParam::Tracking, TextParam::Leading]
                    .into_iter()
                    .filter(|p| *p != parameter)
                    .map(|p| {
                        (
                            p,
                            e.selected_layer()
                                .unwrap()
                                .track(PropertyPath::Text(p))
                                .cloned(),
                        )
                    })
                    .collect();
            let sampled = e.selected_layer().unwrap().text_value_at(parameter, 7);
            e.execute(scalar_animation_command(
                e.selected_layer().unwrap(),
                parameter,
                7,
            ))
            .unwrap();
            assert!(
                e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(parameter))
                    .unwrap()
                    .keys()
                    .contains_key(&7)
            );
            for (other, before) in &other_tracks {
                assert_eq!(
                    e.selected_layer()
                        .unwrap()
                        .track(PropertyPath::Text(*other)),
                    before.as_ref()
                );
            }
            e.execute(scalar_animation_command(
                e.selected_layer().unwrap(),
                parameter,
                7,
            ))
            .unwrap();
            assert!(
                e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(parameter))
                    .unwrap()
                    .keys()
                    .is_empty()
            );
            assert_eq!(
                e.selected_layer().unwrap().text_value_at(parameter, 7),
                sampled
            );
            for (other, before) in &other_tracks {
                assert_eq!(
                    e.selected_layer()
                        .unwrap()
                        .track(PropertyPath::Text(*other)),
                    before.as_ref()
                );
            }
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        for index in 0..3 {
            assert!(field_command(e.selected_layer().unwrap(), 0, index, "2").is_err());
        }
    }

    #[test]
    fn paragraph_planning_samples_current_frame_and_rechecks_pending_changes_and_bounds() {
        let mut e = scene();
        let command = paragraph_command(e.selected_layer().unwrap(), 0, ParagraphEdit::Mode(true))
            .unwrap()
            .unwrap();
        e.execute(command).unwrap();
        for (parameter, value) in [
            (TextParam::FontSize, 120.),
            (TextParam::Leading, 2.25),
            (TextParam::Tracking, 20.),
        ] {
            e.execute(scalar_animation_command(
                e.selected_layer().unwrap(),
                parameter,
                0,
            ))
            .unwrap();
            let command = e
                .selected_layer()
                .unwrap()
                .text_value_command(parameter, value, 60)
                .unwrap()
                .unwrap();
            e.execute(command).unwrap();
        }
        let base_style = e.selected_layer().unwrap().text_style();
        let base_content = e.selected_layer().unwrap().content().clone();
        let tracks = [TextParam::FontSize, TextParam::Tracking, TextParam::Leading].map(|p| {
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Text(p))
                .unwrap()
                .clone()
        });
        let at_0 = crate::text_flow::fit_height(e.selected_layer().unwrap(), 0)
            .unwrap()
            .ceil();
        let at_60 = crate::text_flow::fit_height(e.selected_layer().unwrap(), 60)
            .unwrap()
            .ceil();
        assert_ne!(at_0, at_60);
        let command = paragraph_command(e.selected_layer().unwrap(), 60, ParagraphEdit::FitHeight)
            .unwrap()
            .unwrap();
        e.execute(command).unwrap();
        assert_eq!(e.selected_layer().unwrap().height(), at_60);
        assert!(
            paragraph_command(e.selected_layer().unwrap(), 60, ParagraphEdit::FitHeight)
                .unwrap()
                .is_none()
        );
        let command = paragraph_command(
            e.selected_layer().unwrap(),
            60,
            ParagraphEdit::Align(TextAlign::Right),
        )
        .unwrap()
        .unwrap();
        e.execute(command).unwrap();
        assert_eq!(e.selected_layer().unwrap().content(), &base_content);
        assert_eq!(
            e.selected_layer().unwrap().text_style().tracking,
            base_style.tracking
        );
        assert_eq!(
            e.selected_layer().unwrap().text_style().leading,
            base_style.leading
        );
        for (p, track) in [TextParam::FontSize, TextParam::Tracking, TextParam::Leading]
            .into_iter()
            .zip(tracks)
        {
            assert_eq!(
                e.selected_layer().unwrap().track(PropertyPath::Text(p)),
                Some(&track)
            );
        }
        // A pending source edit can make a previously offered Fit invalid.
        e.execute(Command::SetContent {
            id: 1,
            content: Content::Text {
                text: "X\n".repeat(200),
                font_size: 48.,
            },
        })
        .unwrap();
        assert!(
            paragraph_command(e.selected_layer().unwrap(), 60, ParagraphEdit::FitHeight).is_err()
        );
        let before = e.project().clone();
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(
            paragraph_command(e.selected_layer().unwrap(), 60, ParagraphEdit::Mode(false)).is_err()
        );
        assert_eq!(
            before.composition().layer(1).unwrap().height(),
            e.selected_layer().unwrap().height()
        );
    }

    #[test]
    fn typography_and_paragraph_targets_reject_stale_context_but_allow_synchronous_replanning() {
        let mut state = EditorState::default();
        state.editor = scene();
        let target = InputTarget::new(&state).unwrap();
        let command = field_command(state.editor.selected_layer().unwrap(), 0, 0, "72")
            .unwrap()
            .unwrap();
        assert!(target.current(&state));
        state.editor.execute(command).unwrap();
        assert!(!target.current(&state));
        assert!(target.same_context(&state));
        let command = paragraph_command(
            state.editor.selected_layer().unwrap(),
            state.frame,
            ParagraphEdit::Mode(true),
        )
        .unwrap()
        .unwrap();
        state.editor.execute(command).unwrap();
        assert_eq!(
            state
                .editor
                .selected_layer()
                .unwrap()
                .text_typography_at(0)
                .unwrap()
                .font_size,
            72.
        );
        let target = InputTarget::new(&state).unwrap();
        state.frame = 1;
        assert!(!target.current(&state));
        assert!(!target.same_context(&state));
        state.frame = 0;
        state.playing = true;
        assert!(!target.same_context(&state));
        state.playing = false;
        state.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(!target.same_context(&state));
        state.editor.execute(Command::ToggleLocked(1)).unwrap();
        state.editor.execute(Command::AddSolid).unwrap();
        assert!(!target.same_context(&state));
    }
}
