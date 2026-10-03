use crate::{
    components::TextField,
    editor::{Action, EditorState},
    font_usage::{CheckPhase, CoverageSession, Group, Replacement},
    ui,
};
use gpui::{Context, Entity, FocusHandle, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Project, TextFont};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct FontManager {
    state: Entity<EditorState>,
    focus: FocusHandle,
    search: Entity<TextField>,
    revision: Option<u64>,
    inventory_dirty: bool,
    coverage: CoverageSession,
    groups: Vec<Group>,
    selected: Option<TextFont>,
    replacement: Option<TextFont>,
    replacement_origin: Option<Project>,
    message: String,
}
impl FontManager {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, state, cx| {
            let state = state.read(cx);
            this.coverage.validate(
                state.editor.project(),
                state.document_revision,
                this.selected.as_ref(),
                state.fonts_open,
            );
            if !state.fonts_open {
                this.revision = None;
            }
            if this
                .replacement_origin
                .as_ref()
                .is_some_and(|origin| origin != state.editor.project())
            {
                this.replacement = None;
                this.replacement_origin = None;
            }
            this.inventory_dirty = true;
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
            inventory_dirty: true,
            coverage: CoverageSession::default(),
            groups: vec![],
            selected: None,
            replacement: None,
            replacement_origin: None,
            message: String::new(),
        }
    }
    fn close(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.coverage.invalidate();
        self.state.update(cx, |s, cx| {
            s.fonts_open = false;
            cx.notify();
        });
        self.revision = None;
        self.replacement = None;
        self.replacement_origin = None;
        self.message.clear();
        w.blur();
    }
    fn apply(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let (Some(from), Some(to), Some(revision)) =
            (&self.selected, &self.replacement, self.revision)
        else {
            return;
        };
        let result = {
            let state = self.state.read(cx);
            self.replacement_origin
                .as_ref()
                .ok_or_else(|| "The project changed. Choose the replacement again.".to_string())
                .and_then(|origin| Replacement::new(origin, revision, from.clone(), to.clone()))
                .and_then(|plan| {
                    let command = plan.command(state.editor.project(), state.document_revision)?;
                    Ok((plan, command))
                })
        };
        match result {
            Ok((plan, command)) => {
                self.coverage.invalidate();
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
        // Inventory stays cheap; neither Open nor a panel/frame render shapes text.
        // Only explicit Check captures source layers. No full Project render clone.
        self.coverage.validate(
            state.editor.project(),
            state.document_revision,
            self.selected.as_ref(),
            state.fonts_open,
        );
        if self.inventory_dirty || self.revision != Some(state.document_revision) {
            let newly_opened = self.revision.is_none();
            if newly_opened {
                w.focus(&self.focus);
                self.message.clear();
            }
            let groups = crate::font_usage::inventory(state.editor.project());
            let changed = groups != self.groups || self.revision != Some(state.document_revision);
            self.revision = Some(state.document_revision);
            self.inventory_dirty = false;
            self.groups = groups;
            if newly_opened
                || !self
                    .groups
                    .iter()
                    .any(|g| Some(&g.font) == self.selected.as_ref())
            {
                let selected = self
                    .groups
                    .iter()
                    .find(|g| g.warning.is_some())
                    .or(self.groups.first())
                    .map(|g| g.font.clone());
                if selected != self.selected {
                    self.coverage.invalidate();
                }
                self.selected = selected;
            }
            if changed || newly_opened {
                self.replacement = None;
                self.replacement_origin = None;
            }
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
                        if this.selected.as_ref() != Some(&font) {
                            this.coverage.invalidate();
                        }
                        this.selected = Some(font.clone());
                        this.replacement = None;
                        this.replacement_origin = None;
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
                            this.replacement_origin =
                                Some(this.state.read(cx).editor.project().clone());
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
                    "Requested face: {} · {}",
                    g.font.family,
                    if g.font.face.is_empty() {
                        crate::fonts::label(g.font.weight, g.font.italic)
                    } else {
                        g.font.face.clone()
                    }
                ))
                .child(format!(
                    "Primary matched face: {} · {}",
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
                        "{} [composition {}] / {} [layer {}]{}",
                        u.composition,
                        u.composition_id,
                        u.layer,
                        u.layer_id,
                        if u.locked { " (locked)" } else { "" }
                    ))
                }));
            root = root.child(usages);
        }
        if let Some(to) = &self.replacement {
            root = root.child(format!("Replace with: {} · {}", to.family, to.face));
        }
        let count = group.map_or(0, Group::editable);
        root = root.child(self.coverage_panel(cx));
        let enabled = count > 0 && self.replacement.is_some() && self.replacement != self.selected;
        root=root.child("Replacement preserves text, spacing, paint, paragraph boxes and animation. Fonts are not embedded.")
            .child(div().text_color(rgb(ui::MUTED)).child("Glyph checks do not change export policy. Strict export still checks missing families/substituted primary faces. Restart after installing fonts."));
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

impl FontManager {
    fn check_glyphs(&mut self, cx: &mut Context<Self>) {
        let Some(font) = self.selected.clone() else {
            return;
        };
        let state = self.state.read(cx);
        if !state.fonts_open {
            return;
        }
        let job = match self
            .coverage
            .start(state.editor.project(), state.document_revision, &font)
        {
            Ok(job) => job,
            Err(error) => {
                self.message = error;
                cx.notify();
                return;
            }
        };
        self.message.clear();
        cx.notify();
        cx.spawn(async move |entity, cx| {
            // One bounded layer per background task. Cancellation is cooperative
            // between layers, and the worker slot stays held until this loop exits.
            for index in 0..job.snapshot.target() {
                if job.cancelled() {
                    break;
                }
                let pending = job.clone();
                let report = cx
                    .background_executor()
                    .spawn(async move {
                        if pending.cancelled() {
                            return Err("Glyph check cancelled.".into());
                        }
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            crate::font_coverage::analyze(&pending.snapshot.layers[index].layer)
                        }))
                        .map_err(|_| "Glyph analysis failed for this layer.".to_string())
                    })
                    .await;
                let accepted = entity
                    .update(cx, |this, cx| {
                        let state = this.state.read(cx);
                        this.coverage.validate(
                            state.editor.project(),
                            state.document_revision,
                            this.selected.as_ref(),
                            state.fonts_open,
                        );
                        let accepted = this.coverage.accept(job.serial, index, report);
                        cx.notify();
                        accepted
                    })
                    .unwrap_or(false);
                if !accepted {
                    break;
                }
            }
            let _ = entity.update(cx, |this, cx| {
                let state = this.state.read(cx);
                this.coverage.validate(
                    state.editor.project(),
                    state.document_revision,
                    this.selected.as_ref(),
                    state.fonts_open,
                );
                this.coverage.finish(job.serial);
                cx.notify();
            });
        })
        .detach();
    }

    fn coverage_panel(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let busy = self.coverage.busy();
        let enabled = self.selected.is_some() && !busy;
        let mut controls = div().flex().gap_2().items_center().child(
            ui::text_button("check-project-font-glyphs", "Check glyphs")
                .when(!enabled, |d| d.opacity(0.4))
                .when(enabled, |d| {
                    d.on_click(cx.listener(|this, _, _, cx| this.check_glyphs(cx)))
                }),
        );
        if self
            .coverage
            .check
            .as_ref()
            .is_some_and(|check| check.phase == CheckPhase::Checking)
        {
            controls = controls.child(
                ui::text_button("cancel-project-font-glyphs", "Cancel check").on_click(
                    cx.listener(|this, _, _, cx| {
                        this.coverage.cancel();
                        cx.notify();
                    }),
                ),
            );
        }
        let mut panel = div()
            .id("project-font-glyph-check")
            .flex()
            .flex_col()
            .gap_1()
            .border_t_1()
            .border_color(rgb(ui::BORDER))
            .pt_2()
            .child(controls)
            .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(format!(
                "Paint-independent shaping includes hidden, locked and paint-disabled layers. Limits: {} layers; {} source bytes and {} source/composed lines per layer.",
                crate::font_usage::MAX_CHECK_LAYERS,
                crate::font_coverage::MAX_SOURCE_BYTES,
                crate::font_coverage::MAX_SOURCE_LINES,
            )));
        let Some(check) = &self.coverage.check else {
            return panel.child(if busy {
                "Previous glyph check is stopping after its current layer."
            } else {
                "Check this font's current source across all compositions, including locked layers. Nothing is edited."
            });
        };
        let reported = check.reports.len();
        let total = check.snapshot.layers.len();
        let unexamined = total.saturating_sub(reported);
        let target = check.snapshot.target();
        let phase = match &check.phase {
            CheckPhase::Checking => format!(
                "Checking {reported}/{target} scheduled layer(s) · {total} matching layer(s)"
            ),
            CheckPhase::Finished => format!("Finished · {reported}/{total} layer(s) reported"),
            CheckPhase::Cancelled => format!(
                "Cancelled · {reported}/{total} layer(s) reported{}",
                if busy {
                    " · current layer stopping"
                } else {
                    ""
                }
            ),
            CheckPhase::Failed(error) => {
                format!("Check failed · {reported}/{total} layer(s) reported · {error}")
            }
        };
        panel = panel.child(phase);
        let glyphs: usize = check.reports.iter().map(|r| r.glyphs).sum();
        let unresolved: usize = check.reports.iter().map(|r| r.unresolved_glyphs).sum();
        let affected = check
            .reports
            .iter()
            .filter(|r| r.unresolved_glyphs > 0)
            .count();
        let incomplete = check
            .reports
            .iter()
            .filter(|r| !matches!(r.status, crate::font_coverage::Status::Complete))
            .count();
        panel = panel.child(format!(
            "{glyphs} final positioned glyph(s) · {unresolved} unresolved (glyph ID 0) in {affected} layer(s) · {incomplete} empty/incomplete/unsupported layer(s)"
        ));
        if unexamined > 0 {
            panel = panel.child(div().text_color(rgb(0xffaa88)).child(format!(
                "{unexamined} layer(s) unexamined. Each check is limited to {} layers; no whole-group coverage conclusion.", crate::font_usage::MAX_CHECK_LAYERS
            )));
        }
        let mut additional =
            BTreeMap::<crate::font_coverage::Face, (usize, BTreeSet<usize>)>::new();
        for (index, report) in check.reports.iter().enumerate() {
            for usage in &report.faces {
                if usage.is_fallback {
                    let counts = additional.entry(usage.font.clone()).or_default();
                    counts.0 += usage.glyphs;
                    counts.1.insert(index);
                }
            }
        }
        panel = panel.child(if additional.is_empty() {
            "Additional/fallback faces: none recorded in examined glyphs".into()
        } else {
            format!(
                "Additional/fallback faces: {}",
                additional
                    .iter()
                    .map(|(font, (glyphs, layers))| format!(
                        "{} ({glyphs} glyphs, {} layers)",
                        face_label(font),
                        layers.len()
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        });
        let mut rows = div()
            .id("project-font-glyph-results")
            .max_h(px(190.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        for (index, report) in check.reports.iter().enumerate() {
            let usage = &check.snapshot.layers[index].usage;
            let status = report_status(report);
            let primary = report
                .primary
                .as_ref()
                .map(face_label)
                .unwrap_or_else(|| "not resolved".into());
            let actual = if report.faces.is_empty() {
                "none recorded".into()
            } else {
                report
                    .faces
                    .iter()
                    .map(|usage| {
                        format!(
                            "{} ({} glyphs; {})",
                            face_label(&usage.font),
                            usage.glyphs,
                            match (usage.is_primary, usage.is_fallback) {
                                (true, true) => "primary + additional/fallback identities",
                                (true, false) => "primary",
                                (false, true) => "additional/fallback",
                                (false, false) => "identity unavailable",
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            let mut row = div()
                .flex()
                .flex_col()
                .text_size(px(11.0))
                .child(format!(
                    "{} [composition {}] / {} [layer {}]{}",
                    usage.composition,
                    usage.composition_id,
                    usage.layer,
                    usage.layer_id,
                    if usage.locked { " (locked)" } else { "" }
                ))
                .child(format!(
                    "{status} · {} positioned glyph(s) · {} unresolved glyph-ID 0",
                    report.glyphs, report.unresolved_glyphs
                ))
                .child(format!("Primary match: {primary}"))
                .child(format!("Actual positioned-glyph faces: {actual}"));
            if report.overflow_lines > 0 {
                row = row.child(format!(
                    "Paragraph overflow: {} line(s) unexamined",
                    report.overflow_lines
                ));
            }
            for sample in &report.samples {
                let fragment = sample
                    .text
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect::<String>();
                let points = sample
                    .code_points
                    .iter()
                    .map(|c| format!("U+{c:04X}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                row = row.child(if sample.text.is_empty() {
                    "Unresolved glyph: cluster text unavailable".to_string()
                } else {
                    format!("Unresolved source fragment: “{fragment}” · {points}")
                });
            }
            if report.truncated {
                row = row.child("Detail/sample limits reached; this list is not exhaustive.");
            }
            rows = rows.child(row);
        }
        panel.child(rows).child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
            "Reports cover composed source glyph metadata, not caret ranges, overflow text, semantic emoji support or color-font painting. Zero glyph-ID 0 is not a guarantee of complete text support."
        ))
    }
}
fn face_label(face: &crate::font_coverage::Face) -> String {
    format!(
        "{} · {}",
        face.family,
        if face.face.is_empty() {
            crate::fonts::label(face.weight, face.italic)
        } else {
            face.face.clone()
        }
    )
}
fn report_status(report: &crate::font_coverage::Report) -> String {
    match &report.status {
        crate::font_coverage::Status::Complete => "Composed source examined".into(),
        crate::font_coverage::Status::Empty => "Empty: no composed source glyphs to examine".into(),
        crate::font_coverage::Status::Unsupported => "Unsupported source".into(),
        crate::font_coverage::Status::Incomplete(reason) => format!("Incomplete: {reason}"),
    }
}
