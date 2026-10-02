use crate::{
    components::TextField,
    editor::{Action, EditorState},
    font_usage::{Group, Replacement},
    ui,
};
use gpui::{Context, Entity, FocusHandle, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Project, TextFont};

pub(crate) struct FontManager {
    state: Entity<EditorState>,
    focus: FocusHandle,
    search: Entity<TextField>,
    revision: Option<u64>,
    project: Project,
    groups: Vec<Group>,
    selected: Option<TextFont>,
    replacement: Option<TextFont>,
    message: String,
}
impl FontManager {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, state, cx| {
            if !state.read(cx).fonts_open {
                this.revision = None;
            }
            cx.notify();
        })
        .detach();
        let search = cx.new(|cx| TextField::new(cx, |_, _, _| {}));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            focus: cx.focus_handle(),
            search,
            revision: None,
            project: Project::default(),
            groups: vec![],
            selected: None,
            replacement: None,
            message: String::new(),
        }
    }
    fn close(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.fonts_open = false;
            cx.notify();
        });
        self.revision = None;
        self.message.clear();
        w.blur();
    }
    fn apply(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let (Some(from), Some(to), Some(revision)) =
            (&self.selected, &self.replacement, self.revision)
        else {
            return;
        };
        let result = Replacement::new(&self.project, revision, from.clone(), to.clone());
        let result = result.and_then(|plan| {
            let state = self.state.read(cx);
            let command = plan.command(state.editor.project(), state.document_revision)?;
            Ok((plan, command))
        });
        match result {
            Ok((plan, command)) => {
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::Edit(command), w, cx));
                let status = &self.state.read(cx).status;
                self.message = if status.starts_with("Edited") {
                    format!(
                        "Replaced {} text layer(s); {} locked layer(s) skipped. One Undo restores the project.",
                        plan.count, plan.locked
                    )
                } else {
                    status.clone()
                };
            }
            Err(e) => self.message = e,
        }
        cx.notify();
    }
}
impl Render for FontManager {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        // Ordinary edits do not advance document_revision (it tracks document
        // switches). Compare the document too, including replacement and Undo.
        if self.revision != Some(state.document_revision) || &self.project != state.editor.project()
        {
            if self.revision.is_none() {
                w.focus(&self.focus);
                self.message.clear();
            }
            self.project = state.editor.project().clone();
            self.revision = Some(state.document_revision);
            self.groups = crate::font_usage::inventory(&self.project);
            self.selected = self
                .groups
                .iter()
                .find(|g| g.warning.is_some())
                .or(self.groups.first())
                .map(|g| g.font.clone());
            self.replacement = None;
        }
        let missing = self.groups.iter().filter(|g| g.warning.is_some()).count();
        let mut root = div()
            .id("project-fonts-dialog")
            .track_focus(&self.focus)
            .w(px(790.0))
            .max_h(px(700.0))
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(ui::BLUE))
            .occlude()
            .on_key_down(|_, _, cx| cx.stop_propagation())
            .capture_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, w, cx| {
                if e.keystroke.key == "escape" {
                    this.close(w, cx);
                    cx.stop_propagation();
                }
            }))
            .child("Project fonts")
            .child(format!(
                "{} font/style reference(s) · {missing} unavailable · all compositions",
                self.groups.len()
            ));
        let mut sources = div()
            .id("project-font-list")
            .w(px(310.0))
            .h(px(255.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for (i, g) in self.groups.iter().enumerate() {
            let font = g.font.clone();
            sources = sources.child(
                ui::text_button(("project-font", i), "")
                    .h_auto()
                    .min_h(px(48.0))
                    .flex_none()
                    .flex_col()
                    .items_start()
                    .when(self.selected.as_ref() == Some(&g.font), |d| {
                        d.bg(rgb(0x164a7b))
                    })
                    .child(g.font.family.clone())
                    .child(div().text_size(px(10.0)).child(format!(
                        "{} · {} layer(s){}",
                        crate::fonts::style_label(&g.font.style()),
                        g.usages.len(),
                        if g.warning.is_some() {
                            " · Unavailable"
                        } else {
                            ""
                        }
                    )))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(font.clone());
                        this.replacement = None;
                        this.message.clear();
                        cx.notify();
                    })),
            );
        }
        if self.groups.is_empty() {
            sources = sources.child("No text layers in this project.");
        }
        let query = self.search.read(cx).value().trim().to_lowercase();
        let mut families = div()
            .id("replacement-font-list")
            .h(px(135.0))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for (i, family) in crate::fonts::families()
            .iter()
            .filter(|f| f.to_lowercase().contains(&query))
            .enumerate()
        {
            let name = family.clone();
            families = families.child(
                ui::text_button(("replacement-family", i), family.clone())
                    .justify_start()
                    .flex_none()
                    .when(
                        self.replacement
                            .as_ref()
                            .is_some_and(|f| f.family == *family),
                        |d| d.text_color(rgb(ui::BLUE)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(from) = &this.selected {
                            let mut style = from.style();
                            style.font_family = name.clone();
                            style.font_face.clear();
                            this.replacement = Some(TextFont::of(&crate::fonts::resolved(&style)));
                            this.message.clear();
                            cx.notify();
                        }
                    })),
            );
        }
        let mut target = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .child("Replacement family (search)")
            .child(self.search.clone())
            .child(families);
        if let Some(to) = self.replacement.clone() {
            let mut variants = div()
                .id("replacement-font-styles")
                .h(px(65.0))
                .overflow_y_scroll()
                .flex()
                .flex_col();
            for (i, v) in crate::fonts::variants(&to.family).into_iter().enumerate() {
                let mut font = to.clone();
                font.face = v.face.clone();
                font.weight = v.weight;
                font.italic = v.italic;
                variants = variants.child(
                    ui::text_button(("replacement-variant", i), v.label())
                        .justify_start()
                        .flex_none()
                        .when(self.replacement.as_ref() == Some(&font), |d| {
                            d.text_color(rgb(ui::BLUE))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.replacement = Some(font.clone());
                            cx.notify();
                        })),
                );
            }
            target = target.child(variants);
        }
        root = root.child(div().flex().gap_3().child(sources).child(target));
        let group = self
            .groups
            .iter()
            .find(|g| Some(&g.font) == self.selected.as_ref());
        if let Some(g) = group {
            root = root
                .child(format!(
                    "Saved: {} · {}",
                    g.font.family,
                    if g.font.face.is_empty() {
                        crate::fonts::label(g.font.weight, g.font.italic)
                    } else {
                        g.font.face.clone()
                    }
                ))
                .child(format!(
                    "Primary preview/output face: {} · {}",
                    g.actual.family, g.actual.face
                ));
            if let Some(warning) = &g.warning {
                root = root.child(div().text_color(rgb(0xffaa88)).child(warning.clone()));
            }
            let usages = div()
                .id("font-usage-layers")
                .h(px(78.0))
                .overflow_y_scroll()
                .children(g.usages.iter().map(|u| {
                    div().child(format!(
                        "{} / {}{}",
                        u.composition,
                        u.layer,
                        if u.locked { " (locked)" } else { "" }
                    ))
                }));
            root = root.child(usages);
        }
        if let Some(to) = &self.replacement {
            root = root.child(format!("Replace with: {} · {}", to.family, to.face));
        }
        let count = group.map_or(0, Group::editable);
        let enabled = count > 0 && self.replacement.is_some() && self.replacement != self.selected;
        root=root.child("Replacement preserves text, spacing, paint, paragraph boxes and animation. Fonts are not embedded.")
            .child(div().text_color(rgb(ui::MUTED)).child("Missing glyphs may use additional fallback faces. Restart the app after installing fonts."));
        if !self.message.is_empty() {
            root = root.child(self.message.clone());
        }
        root.child(
            div()
                .flex()
                .gap_2()
                .child(
                    ui::text_button(
                        "replace-project-font",
                        SharedString::from(format!("Replace in {count} unlocked layer(s)")),
                    )
                    .when(!enabled, |d| d.opacity(0.4))
                    .when(enabled, |d| d.on_click(cx.listener(Self::apply_click))),
                )
                .child(
                    ui::text_button("close-project-fonts", "Close")
                        .on_click(cx.listener(|this, _, w, cx| this.close(w, cx))),
                ),
        )
    }
}
impl FontManager {
    fn apply_click(&mut self, _: &gpui::ClickEvent, w: &mut Window, cx: &mut Context<Self>) {
        self.apply(w, cx);
    }
}
