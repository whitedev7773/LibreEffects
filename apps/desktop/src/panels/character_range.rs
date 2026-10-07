//! Character formatting belongs to a selected source range in the text draft.
use crate::{components::TextField, editor::EditorState, text_edit::SelectionTarget, ui};
use gpui::{Context, Entity, FocusHandle, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{
    TextCharacterPatch, TextFont, TextLeading, TextSelectionStyle, TextStrokeJoin,
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
enum Input {
    Size,
    Leading,
    Tracking,
    Fill,
    Stroke,
    StrokeWidth,
}
impl Input {
    fn key(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Leading => "leading",
            Self::Tracking => "tracking",
            Self::Fill => "fill",
            Self::Stroke => "stroke",
            Self::StrokeWidth => "stroke-width",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Size => "Font size (px)",
            Self::Leading => "Leading (px, Auto ratio, or Inherit)",
            Self::Tracking => "Tracking (1/1000 em)",
            Self::Fill => "Fill (hex)",
            Self::Stroke => "Stroke (hex)",
            Self::StrokeWidth => "Stroke width (px)",
        }
    }
}

pub(super) struct CharacterRange {
    state: Entity<EditorState>,
    focus: FocusHandle,
    fields: [(Input, Entity<TextField>); 6],
    search: Entity<TextField>,
    picker: Option<(SelectionTarget, bool)>,
}
impl CharacterRange {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        let fields = [
            Input::Size,
            Input::Leading,
            Input::Tracking,
            Input::Fill,
            Input::Stroke,
            Input::StrokeWidth,
        ]
        .map(|input| {
            (
                input,
                cx.new(|cx| TextField::new(cx, |_, _, _| {}).return_focus(focus.clone())),
            )
        });
        let search = cx.new(|cx| TextField::new(cx, |_, _, _| {}).return_focus(focus.clone()));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            focus,
            fields,
            search,
            picker: None,
        }
    }
    fn apply(
        &mut self,
        target: &SelectionTarget,
        patch: TextCharacterPatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if TextField::is_composing(window, cx) {
            return;
        }
        apply_patch_to_selection(&self.state, target, patch, cx);
        self.picker = None;
        window.focus(&self.focus);
        cx.notify();
    }
    fn finish(&mut self, serial: u64, commit: bool, window: &mut Window, cx: &mut Context<Self>) {
        if TextField::is_composing(window, cx)
            || self
                .state
                .read(cx)
                .text_session
                .as_ref()
                .is_some_and(|s| s.buffer.marked.is_some())
        {
            return;
        }
        if commit {
            TextField::commit_text_selection_active(window, cx);
        }
        self.state.update(cx, |s, cx| {
            if s.text_session
                .as_ref()
                .is_some_and(|session| session.identity() == serial)
            {
                s.finish_text(commit, cx);
            }
        });
        self.picker = None;
        window.blur();
        cx.notify();
    }
    fn history(&mut self, serial: u64, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if TextField::is_composing(window, cx) {
            return;
        }
        TextField::commit_text_selection_active(window, cx);
        self.state.update(cx, |s, cx| {
            if let Some(session) = s
                .text_session
                .as_mut()
                .filter(|session| session.identity() == serial && session.buffer.marked.is_none())
            {
                session.buffer.history(redo);
                session.preferred_x = None;
                session.caret_hint = None;
                cx.notify();
            }
        });
        self.picker = None;
        window.focus(&self.focus);
        cx.notify();
    }
}

fn apply_patch_to_selection(
    state: &Entity<EditorState>,
    target: &SelectionTarget,
    patch: TextCharacterPatch,
    cx: &mut gpui::App,
) {
    state.update(cx, |state, cx| {
        if !target.current(state) {
            state.status = "Selection changed; choose the character setting again".into();
            cx.notify();
            return;
        }
        match state
            .text_session
            .as_mut()
            .unwrap()
            .format_selection(&patch)
        {
            Ok(true) => {
                state.status = if state
                    .text_session
                    .as_ref()
                    .is_some_and(|session| session.authored_spacing_reset())
                {
                    "Selected characters formatted · Saved glyph spacing reset in this draft".into()
                } else {
                    "Selected characters formatted in the text draft".into()
                };
            }
            Ok(false) => {}
            Err(error) => state.status = error,
        }
        cx.notify();
    });
}
fn summary(state: &EditorState) -> Option<TextSelectionStyle> {
    let session = state.text_session.as_ref()?;
    session.buffer.selection_style(&session.baseline_style).ok()
}
fn display(state: &EditorState, input: Input) -> String {
    let Some(style) = summary(state) else {
        return String::new();
    };
    let value = match input {
        Input::Size => style.font_size.map(|v| v.to_string()),
        Input::Leading => style.leading.map(|leading| match leading {
            None => "Inherit".into(),
            Some(TextLeading::Auto(ratio)) => format!("Auto {ratio}"),
            Some(TextLeading::Fixed(pixels)) => pixels.to_string(),
        }),
        Input::Tracking => style.tracking.map(|v| v.to_string()),
        Input::Fill => style.fill_color.map(|v| format!("{v:06X}")),
        Input::Stroke => style.stroke_color.map(|v| format!("{v:06X}")),
        Input::StrokeWidth => style.stroke_width.map(|v| v.to_string()),
    };
    value.unwrap_or_else(|| "Mixed".into())
}
fn field_patch(text: &str, input: Input) -> Result<TextCharacterPatch, String> {
    let text = text.trim();
    if matches!(input, Input::Fill | Input::Stroke) {
        let hex = text.strip_prefix('#').unwrap_or(text);
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Enter six hexadecimal digits for the selected text color".into());
        }
        let color = u32::from_str_radix(hex, 16).unwrap();
        return Ok(if matches!(input, Input::Fill) {
            TextCharacterPatch::FillColor(color)
        } else {
            TextCharacterPatch::StrokeColor(color)
        });
    }
    if matches!(input, Input::Leading) {
        if text.eq_ignore_ascii_case("inherit") {
            return Ok(TextCharacterPatch::Leading(None));
        }
        let parts: Vec<_> = text.split_whitespace().collect();
        let leading = match parts.as_slice() {
            [mode, ratio] if mode.eq_ignore_ascii_case("auto") => {
                ratio.parse().ok().map(TextLeading::Auto)
            }
            [pixels] => pixels.parse().ok().map(TextLeading::Fixed),
            _ => None,
        };
        return leading
            .filter(|value| value.valid())
            .map(|value| TextCharacterPatch::Leading(Some(value)))
            .ok_or_else(|| "Enter 0.1–20480 pixels, Auto 0.1–10, or Inherit".into());
    }
    let (minimum, maximum, error) = match input {
        Input::Size => (1.0, 2048.0, "Enter a font size from 1 to 2048 pixels"),
        Input::Tracking => (-1000.0, 10000.0, "Enter tracking from -1000 to 10000"),
        Input::StrokeWidth => (0.0, 1000.0, "Enter a stroke width from 0 to 1000 pixels"),
        _ => unreachable!(),
    };
    let value = text.parse::<f64>().map_err(|_| error.to_string())?;
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(error.into());
    }
    Ok(match input {
        Input::Size => TextCharacterPatch::FontSize(value),
        Input::Tracking => TextCharacterPatch::Tracking(value),
        Input::StrokeWidth => TextCharacterPatch::StrokeWidth(value),
        _ => unreachable!(),
    })
}
fn button(
    id: impl Into<gpui::ElementId>,
    text: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    ui::text_button(id, text)
        .w_full()
        .justify_start()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
}
impl Render for CharacterRange {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(session) = self.state.read(cx).text_session.clone() else {
            return div().into_any_element();
        };
        let serial = session.identity();
        let target = session.selection_target();
        let selected = session.buffer.selection();
        let selected_count = session.buffer.text[selected.clone()]
            .graphemes(true)
            .count();
        let source_current = {
            let state = self.state.read(cx);
            session.valid(state.editor.project(), state.document_revision, state.frame)
        };
        let mut panel = div()
            .id("character-selection")
            .track_focus(&self.focus)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_size(px(12.0)).child("Selected characters"))
            .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
                "Formatting stays in this text draft. Done commits typing and styles together.",
            ));
        let spacing_notice = if session
            .buffer
            .rich_text
            .as_ref()
            .is_some_and(|rich| rich.positioning.is_some())
        {
            Some(crate::authored_spacing_notice::RETAINED)
        } else if session.authored_spacing_reset() {
            Some(crate::authored_spacing_notice::DRAFT_RESET)
        } else {
            None
        };
        if let Some(notice) = spacing_notice {
            panel = panel.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child(notice),
            );
        }
        if let Ok(target) = target
            && source_current
        {
            let style = session
                .buffer
                .selection_style(&session.baseline_style)
                .unwrap();
            let family = style.font_family.clone();
            panel = panel.child(
                div()
                    .text_size(px(10.0))
                    .child(format!("{selected_count} selected characters")),
            );
            if self
                .picker
                .as_ref()
                .is_some_and(|(owner, _)| !owner.current(self.state.read(cx)))
            {
                self.picker = None;
            }
            for (input, field) in self.fields.clone() {
                let value = display(self.state.read(cx), input);
                let binding = target.binding(input.key());
                let target = target.clone();
                let state = self.state.clone();
                field.update(cx, |field, _| {
                    field.sync_guarded(binding, value, window, move |text, _, cx| {
                        match field_patch(text, input) {
                            Ok(patch) => apply_patch_to_selection(&state, &target, patch, cx),
                            Err(error) => state.update(cx, |s, cx| {
                                s.status = error;
                                cx.notify();
                            }),
                        }
                        display(state.read(cx), input)
                    })
                });
            }
            let owner = target.clone();
            panel = panel.child(
                button(
                    "selection-family",
                    format!("{} ▾", family.as_deref().unwrap_or("Mixed families")),
                )
                .on_click(cx.listener(move |this, _, w, cx| {
                    if TextField::is_composing(w, cx) || !owner.current(this.state.read(cx)) {
                        return;
                    }
                    this.picker = Some((owner.clone(), false));
                    w.focus(&this.focus);
                    cx.notify();
                })),
            );
            let owner = target.clone();
            let face_label = style
                .font
                .as_ref()
                .map(|font| {
                    if font.face.is_empty() {
                        crate::fonts::label(font.weight, font.italic)
                    } else {
                        font.face.clone()
                    }
                })
                .unwrap_or_else(|| "Mixed faces".into());
            panel = panel.child(
                button("selection-face", format!("{face_label} ▾"))
                    .when(family.is_none(), |b| b.opacity(0.5))
                    .when(family.is_some(), |b| {
                        b.on_click(cx.listener(move |this, _, w, cx| {
                            if TextField::is_composing(w, cx) || !owner.current(this.state.read(cx))
                            {
                                return;
                            }
                            this.picker = Some((owner.clone(), true));
                            w.focus(&this.focus);
                            cx.notify();
                        }))
                    }),
            );
            if let Some((owner, faces)) = self.picker.clone() {
                let mut choices = div()
                    .id("selected-font-choices")
                    .max_h(px(180.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1();
                if faces {
                    if let Some(family) = &family {
                        for (i, variant) in crate::fonts::variants(family).into_iter().enumerate() {
                            let font = TextFont {
                                family: family.clone(),
                                face: variant.face.clone(),
                                weight: variant.weight,
                                italic: variant.italic,
                            };
                            let target = owner.clone();
                            choices = choices.child(
                                button(
                                    SharedString::from(format!("selected-face-{i}")),
                                    variant.label(),
                                )
                                .flex_none()
                                .on_click(cx.listener(
                                    move |this, _, w, cx| {
                                        this.apply(
                                            &target,
                                            TextCharacterPatch::Font(font.clone()),
                                            w,
                                            cx,
                                        )
                                    },
                                )),
                            );
                        }
                    }
                } else {
                    let query = self.search.read(cx).value().trim().to_lowercase();
                    panel = panel
                        .child(div().text_size(px(10.0)).child("Find font family"))
                        .child(self.search.clone());
                    for (i, family) in crate::fonts::families()
                        .iter()
                        .filter(|f| f.to_lowercase().contains(&query))
                        .enumerate()
                    {
                        let family = family.clone();
                        let target = owner.clone();
                        choices = choices.child(
                            button(
                                SharedString::from(format!("selected-family-{i}")),
                                family.clone(),
                            )
                            .flex_none()
                            .on_click(cx.listener(
                                move |this, _, w, cx| {
                                    this.apply(
                                        &target,
                                        TextCharacterPatch::Family(family.clone()),
                                        w,
                                        cx,
                                    )
                                },
                            )),
                        );
                    }
                }
                panel = panel.child(choices);
            }
            for (input, field) in &self.fields {
                panel = panel
                    .child(div().text_size(px(10.0)).child(input.label()))
                    .child(field.clone());
            }
            let owner = target.clone();
            let next = !style.fill_enabled.unwrap_or(false);
            panel = panel.child(
                button(
                    "selection-fill-enabled",
                    match style.fill_enabled {
                        Some(true) => "Fill: On",
                        Some(false) => "Fill: Off",
                        None => "Fill: Mixed",
                    },
                )
                .on_click(cx.listener(move |this, _, w, cx| {
                    this.apply(&owner, TextCharacterPatch::FillEnabled(next), w, cx)
                })),
            );
            let owner = target.clone();
            let next = !style.stroke_enabled.unwrap_or(false);
            panel = panel.child(
                button(
                    "selection-stroke-enabled",
                    match style.stroke_enabled {
                        Some(true) => "Stroke: On",
                        Some(false) => "Stroke: Off",
                        None => "Stroke: Mixed",
                    },
                )
                .on_click(cx.listener(move |this, _, w, cx| {
                    this.apply(&owner, TextCharacterPatch::StrokeEnabled(next), w, cx)
                })),
            );
            let mut joins = div().flex().flex_wrap().gap_1();
            for (key, label, join) in [
                ("selection-join-miter", "Miter", TextStrokeJoin::Miter),
                ("selection-join-round", "Round", TextStrokeJoin::Round),
                ("selection-join-bevel", "Bevel", TextStrokeJoin::Bevel),
            ] {
                let owner = target.clone();
                let label = if style.stroke_join == Some(join) {
                    format!("✓ {label}")
                } else {
                    label.into()
                };
                joins = joins.child(ui::text_button(key, label).on_click(cx.listener(
                    move |this, _, w, cx| {
                        this.apply(&owner, TextCharacterPatch::StrokeJoin(join), w, cx)
                    },
                )));
            }
            panel = panel
                .child(
                    div()
                        .text_size(px(10.0))
                        .child(if style.stroke_join.is_some() {
                            "Stroke join"
                        } else {
                            "Stroke join: Mixed"
                        }),
                )
                .child(joins);
            let fonts: std::collections::BTreeSet<_> = if let Some(rich) = &session.buffer.rich_text
            {
                rich.runs
                    .iter()
                    .filter(|r| r.start < selected.end && r.end > selected.start)
                    .map(|r| r.style.font())
                    .collect()
            } else {
                [session.baseline_style.font()].into_iter().collect()
            };
            for font in fonts {
                if let Some(warning) = crate::fonts::warning(&font.style()) {
                    panel = panel.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(0xffaa88))
                            .child(warning),
                    );
                }
            }
        } else {
            self.picker = None;
            let message = if selected.is_empty() {
                "Select characters in the preview to format them.".into()
            } else {
                session
                    .selection_target()
                    .err()
                    .unwrap_or_else(|| "Text editing context changed".into())
            };
            panel = panel.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(ui::MUTED))
                    .child(message),
            );
        }
        panel = panel
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(ui::text_button("selection-undo", "Undo draft").on_click(
                        cx.listener(move |this, _, w, cx| this.history(serial, false, w, cx)),
                    ))
                    .child(ui::text_button("selection-redo", "Redo draft").on_click(
                        cx.listener(move |this, _, w, cx| this.history(serial, true, w, cx)),
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(ui::text_button("selection-cancel", "Cancel text").on_click(
                        cx.listener(move |this, _, w, cx| this.finish(serial, false, w, cx)),
                    ))
                    .child(ui::text_button("selection-done", "Done").on_click(
                        cx.listener(move |this, _, w, cx| this.finish(serial, true, w, cx)),
                    )),
            )
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, w, cx| {
                if TextField::active_has_focus(w, cx) {
                    return;
                }
                let m = event.keystroke.modifiers;
                let key = event.keystroke.key.as_str();
                if (m.control || m.platform) && matches!(key, "s" | "o" | "n")
                    || (m.alt && key == "f4")
                {
                    this.finish(serial, true, w, cx);
                    return;
                }
                cx.stop_propagation();
                w.prevent_default();
                if event.is_held || m.alt || TextField::is_composing(w, cx) {
                    return;
                }
                if key == "escape" {
                    this.finish(serial, false, w, cx);
                } else if key == "enter" && (m.control || m.platform) {
                    this.finish(serial, true, w, cx);
                } else if matches!(key, "z" | "y") && (m.control || m.platform) {
                    this.history(serial, key == "y" || m.shift, w, cx);
                }
            }));
        crate::color_edit::input_pointer_text_selection_guarded(panel, move |state, _| {
            state.text_session.as_ref().is_some_and(|session| {
                session.identity() == serial
                    && session.valid(state.editor.project(), state.document_revision, state.frame)
            })
        })
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_fields_parse_explicit_leading_tracking_and_stroke_without_coercion() {
        assert!(matches!(
            field_patch("Inherit", Input::Leading).unwrap(),
            TextCharacterPatch::Leading(None)
        ));
        assert!(matches!(
            field_patch(" auto 1.5 ", Input::Leading).unwrap(),
            TextCharacterPatch::Leading(Some(TextLeading::Auto(1.5)))
        ));
        assert!(matches!(
            field_patch("72", Input::Leading).unwrap(),
            TextCharacterPatch::Leading(Some(TextLeading::Fixed(72.0)))
        ));
        assert!(matches!(
            field_patch("-125", Input::Tracking).unwrap(),
            TextCharacterPatch::Tracking(-125.0)
        ));
        assert!(matches!(
            field_patch("0", Input::StrokeWidth).unwrap(),
            TextCharacterPatch::StrokeWidth(0.0)
        ));
        assert!(matches!(
            field_patch(" #12AbCd ", Input::Stroke).unwrap(),
            TextCharacterPatch::StrokeColor(0x12abcd)
        ));
        for (input, values) in [
            (
                Input::Leading,
                vec![
                    "Mixed", "Auto", "Auto 0", "Auto 11", "0", "20481", "NaN", "72 px",
                ],
            ),
            (
                Input::Tracking,
                vec!["Mixed", "-1001", "10001", "NaN", "inf"],
            ),
            (Input::StrokeWidth, vec!["-1", "1001", "NaN"]),
            (Input::Stroke, vec!["#FFF", "1234567", "XX0000"]),
        ] {
            for text in values {
                assert!(field_patch(text, input).is_err(), "{text}");
            }
        }
    }
}
