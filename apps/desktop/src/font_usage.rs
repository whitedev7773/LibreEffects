//! Project-wide font inventory and transactional replacement planning.
use libre_effects_core::{
    Command, CompositionId, Content, Frame, Layer, LayerId, Project, TextFont,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Usage {
    pub composition_id: CompositionId,
    pub layer_id: LayerId,
    pub composition: String,
    pub layer: String,
    pub locked: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Group {
    pub font: TextFont,
    pub actual: TextFont,
    pub warning: Option<String>,
    pub usages: Vec<Usage>,
}
impl Group {
    pub fn editable(&self) -> usize {
        self.usages.iter().filter(|u| !u.locked).count()
    }
}
pub(crate) fn inventory(project: &Project) -> Vec<Group> {
    let mut groups = BTreeMap::<TextFont, Vec<Usage>>::new();
    for (composition_id, comp) in project.compositions() {
        for layer in comp.layers() {
            if matches!(layer.content(), Content::Text { .. }) {
                groups
                    .entry(TextFont::of(&layer.text_style()))
                    .or_default()
                    .push(Usage {
                        composition_id,
                        layer_id: layer.id(),
                        composition: comp.name().into(),
                        layer: layer.name().into(),
                        locked: layer.locked(),
                    });
            }
        }
    }
    groups
        .into_iter()
        .map(|(font, usages)| {
            let style = font.style();
            Group {
                actual: TextFont::of(&crate::fonts::resolved(&style)),
                warning: crate::fonts::warning(&style),
                font,
                usages,
            }
        })
        .collect()
}
pub(crate) fn missing_count(project: &Project) -> usize {
    inventory(project)
        .iter()
        .filter(|g| g.warning.is_some())
        .map(|g| g.usages.len())
        .sum()
}
// Deliberately independent of the cheap inventory and export missing_count.
// A check freezes only selected text layers, once, after an explicit UI action.
pub(crate) const MAX_CHECK_LAYERS: usize = 256;
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FrozenLayer {
    pub usage: Usage,
    pub layer: Layer,
    /// Captured composition-local frame; inactive compositions use frame zero.
    pub checked_frame: Frame,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CoverageSnapshot {
    revision: u64,
    font: TextFont,
    pub active_composition_id: CompositionId,
    pub active_frame: Frame,
    pub layers: Vec<FrozenLayer>,
}
impl CoverageSnapshot {
    fn capture(project: &Project, revision: u64, font: &TextFont, active_frame: Frame) -> Self {
        let active_composition_id = project.active_composition_id();
        let mut layers = Vec::new();
        for (composition_id, comp) in project.compositions() {
            for layer in comp.layers() {
                if matches!(layer.content(), Content::Text { .. })
                    && TextFont::of(&layer.text_style()) == *font
                {
                    layers.push(FrozenLayer {
                        usage: Usage {
                            composition_id,
                            layer_id: layer.id(),
                            composition: comp.name().into(),
                            layer: layer.name().into(),
                            locked: layer.locked(),
                        },
                        layer: layer.clone(),
                        checked_frame: if composition_id == active_composition_id {
                            active_frame
                        } else {
                            0
                        },
                    });
                }
            }
        }
        Self {
            revision,
            font: font.clone(),
            active_composition_id,
            active_frame,
            layers,
        }
    }
    pub fn target(&self) -> usize {
        self.layers.len().min(MAX_CHECK_LAYERS)
    }
    fn matches(
        &self,
        project: &Project,
        revision: u64,
        selected: Option<&TextFont>,
        active_frame: Frame,
    ) -> bool {
        if self.revision != revision
            || selected != Some(&self.font)
            || self.active_composition_id != project.active_composition_id()
            || self.active_frame != active_frame
        {
            return false;
        }
        // Count membership too: an added matching layer must invalidate a report.
        let mut index = 0;
        for (composition_id, comp) in project.compositions() {
            for layer in comp.layers() {
                if matches!(layer.content(), Content::Text { .. })
                    && TextFont::of(&layer.text_style()) == self.font
                {
                    let Some(frozen) = self.layers.get(index) else {
                        return false;
                    };
                    if frozen.usage.composition_id != composition_id
                        || frozen.usage.layer_id != layer.id()
                        || frozen.usage.composition != comp.name()
                        || &frozen.layer != layer
                    {
                        return false;
                    }
                    index += 1;
                }
            }
        }
        index == self.layers.len()
    }
}
#[derive(Clone)]
pub(crate) struct CoverageJob {
    pub serial: u64,
    pub snapshot: Arc<CoverageSnapshot>,
    pub cancel: Arc<AtomicBool>,
}
impl CoverageJob {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckPhase {
    Checking,
    Finished,
    Cancelled,
    Failed(String),
}
pub(crate) struct CoverageCheck {
    serial: u64,
    pub snapshot: Arc<CoverageSnapshot>,
    pub reports: Vec<crate::font_coverage::Report>,
    pub phase: CheckPhase,
}
/// Pure lifecycle state, shared by the dialog and deterministic async-race tests.
/// Cancelling never frees the worker slot until that worker has actually settled.
#[derive(Default)]
pub(crate) struct CoverageSession {
    serial: u64,
    worker: Option<u64>,
    cancel: Option<Arc<AtomicBool>>,
    pub check: Option<CoverageCheck>,
}
impl CoverageSession {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    pub fn invalidate(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
        self.serial = self.serial.wrapping_add(1);
        self.check = None;
    }
    pub fn validate(
        &mut self,
        project: &Project,
        revision: u64,
        selected: Option<&TextFont>,
        open: bool,
        active_frame: Frame,
    ) {
        if self.check.as_ref().is_some_and(|check| {
            !open
                || !check
                    .snapshot
                    .matches(project, revision, selected, active_frame)
        }) {
            self.invalidate();
        }
    }
    pub fn start(
        &mut self,
        project: &Project,
        revision: u64,
        font: &TextFont,
        active_frame: Frame,
    ) -> Result<CoverageJob, String> {
        if self.busy() {
            return Err("The previous glyph check is still stopping.".into());
        }
        let snapshot = Arc::new(CoverageSnapshot::capture(
            project,
            revision,
            font,
            active_frame,
        ));
        if snapshot.layers.is_empty() {
            return Err("This font is no longer used.".into());
        }
        self.serial = self.serial.wrapping_add(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let job = CoverageJob {
            serial: self.serial,
            snapshot: snapshot.clone(),
            cancel: cancel.clone(),
        };
        self.worker = Some(job.serial);
        self.cancel = Some(cancel);
        self.check = Some(CoverageCheck {
            serial: job.serial,
            snapshot,
            reports: Vec::new(),
            phase: CheckPhase::Checking,
        });
        Ok(job)
    }
    pub fn cancel(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(check) = &mut self.check {
            if check.phase == CheckPhase::Checking {
                check.phase = CheckPhase::Cancelled;
            }
        }
    }
    // The caller validates the live source immediately before accepting each result.
    pub fn accept(
        &mut self,
        serial: u64,
        index: usize,
        result: Result<crate::font_coverage::Report, String>,
    ) -> bool {
        let Some(check) = &mut self.check else {
            return false;
        };
        if check.serial != serial
            || check.phase != CheckPhase::Checking
            || index != check.reports.len()
            || index >= check.snapshot.target()
        {
            return false;
        }
        match result {
            Ok(report) => {
                if report.checked_frame != check.snapshot.layers[index].checked_frame {
                    check.phase = CheckPhase::Failed(
                        "The glyph result did not match its captured local frame.".into(),
                    );
                    if let Some(cancel) = &self.cancel {
                        cancel.store(true, Ordering::Release);
                    }
                    return false;
                }
                check.reports.push(report);
                true
            }
            Err(error) => {
                check.phase = CheckPhase::Failed(error);
                if let Some(cancel) = &self.cancel {
                    cancel.store(true, Ordering::Release);
                }
                false
            }
        }
    }
    pub fn finish(&mut self, serial: u64) {
        if self.worker != Some(serial) {
            return;
        }
        self.worker = None;
        self.cancel = None;
        if let Some(check) = &mut self.check {
            if check.serial == serial && check.phase == CheckPhase::Checking {
                check.phase = if check.reports.len() == check.snapshot.target() {
                    CheckPhase::Finished
                } else {
                    CheckPhase::Failed(
                        "The worker stopped before all scheduled layers were examined.".into(),
                    )
                };
            }
        }
    }
}
impl Drop for CoverageSession {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
    }
}

pub(crate) struct Replacement {
    origin: Project,
    revision: u64,
    pub from: TextFont,
    pub to: TextFont,
    pub count: usize,
    pub locked: usize,
}
impl Replacement {
    pub fn new(
        project: &Project,
        revision: u64,
        from: TextFont,
        to: TextFont,
    ) -> Result<Self, String> {
        if from == to {
            return Err("Choose a different font or style".into());
        }
        if crate::fonts::warning(&to.style()).is_some() {
            return Err("Choose an installed replacement face".into());
        }
        let group = inventory(project)
            .into_iter()
            .find(|g| g.font == from)
            .ok_or("This font is no longer used")?;
        let count = group.editable();
        if count == 0 {
            return Err("All matching text layers are locked".into());
        }
        Ok(Self {
            origin: project.clone(),
            revision,
            from,
            to,
            count,
            locked: group.usages.len() - count,
        })
    }
    pub fn command(&self, project: &Project, revision: u64) -> Result<Command, String> {
        if project != &self.origin || revision != self.revision {
            return Err("The project changed. Choose the replacement again.".into());
        }
        Ok(Command::ReplaceTextFont {
            from: self.from.clone(),
            to: self.to.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, TextStyle};
    #[test]
    fn project_font_report_replacement_history_and_rendering() {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Fonts QA".into(),
            width: 480,
            height: 200,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Font 한글".into(),
                font_size: 64.0,
            },
            width: 400.0,
            height: 100.0,
            name: "Title".into(),
        })
        .unwrap();
        let style = TextStyle {
            font_family: "LibreEffects missing font QA".into(),
            weight: 700,
            ..Default::default()
        };
        e.execute(Command::SetTextStyle {
            id: 1,
            style: style.clone(),
        })
        .unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        e.execute(Command::ToggleLocked(2)).unwrap();
        let before = e.project().clone();
        let groups = inventory(&before);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].editable(), 1);
        assert_eq!(groups[0].usages.len(), 2);
        assert!(groups[0].warning.as_ref().unwrap().contains("Missing font"));
        assert_eq!(groups[0].actual.family, "Wanted Sans");
        assert_eq!(missing_count(&before), 2);
        let plan =
            Replacement::new(&before, 7, groups[0].font.clone(), groups[0].actual.clone()).unwrap();
        assert_eq!((plan.count, plan.locked), (1, 1));
        assert!(plan.command(&before, 8).is_err());
        let mut changed = before.clone();
        changed.activate_composition(1).unwrap();
        assert!(plan.command(&changed, 7).is_err());
        let renderer = crate::rendering::Renderer::new();
        let expected = renderer.render(&changed, 0, 480).unwrap();
        e.execute(plan.command(&before, 7).unwrap()).unwrap();
        let after = e.project().clone();
        assert_eq!(missing_count(&after), 1);
        let mut saved = Project::from_json(&after.to_json().unwrap()).unwrap();
        saved.activate_composition(1).unwrap();
        assert_eq!(renderer.render_preview(&saved, 0, 480).unwrap(), expected);
        assert_eq!(
            renderer.render_output(&saved, 0, 480, 200).unwrap(),
            expected
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        assert!(
            Replacement::new(&after, 8, groups[0].font.clone(), groups[0].actual.clone()).is_err()
        );
        assert!(
            Replacement::new(&before, 7, groups[0].font.clone(), groups[0].font.clone()).is_err()
        );
        let unavailable = TextFont {
            family: "Missing replacement QA".into(),
            ..groups[0].actual.clone()
        };
        assert!(Replacement::new(&before, 7, groups[0].font.clone(), unavailable).is_err());
    }
    #[test]
    fn unavailable_face_and_slant_are_reported_separately_from_installed_fonts() {
        let mut e = Editor::default();
        for (i, style) in [
            TextStyle::default(),
            TextStyle {
                font_face: "WantedSans-Missing".into(),
                ..Default::default()
            },
            TextStyle {
                italic: true,
                ..Default::default()
            },
        ]
        .into_iter()
        .enumerate()
        {
            e.execute(Command::AddContent {
                content: Content::Text {
                    text: "Title".into(),
                    font_size: 48.0,
                },
                width: 200.0,
                height: 100.0,
                name: format!("Title {i}"),
            })
            .unwrap();
            e.execute(Command::SetTextStyle {
                id: e.selected().unwrap(),
                style,
            })
            .unwrap();
        }
        let groups = inventory(e.project());
        assert_eq!(groups.len(), 3);
        assert_eq!(missing_count(e.project()), 2);
        assert_eq!(groups.iter().filter(|g| g.warning.is_none()).count(), 1);
        assert!(groups.iter().all(|g| g.actual.family == "Wanted Sans"));
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use crate::font_coverage::{Report, Status};
    use libre_effects_core::{Editor, PropertyPath, TextParam, TextStyle, TrackEdit};

    fn add_text(editor: &mut Editor, text: &str) {
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: text.into(),
                    font_size: 32.0,
                },
                width: 250.0,
                height: 100.0,
                name: "Same layer".into(),
            })
            .unwrap();
    }
    fn scene() -> (Editor, TextFont) {
        let mut editor = Editor::default();
        add_text(&mut editor, "First");
        add_text(&mut editor, "Second");
        let font = TextFont::of(&TextStyle::default());
        (editor, font)
    }
    fn report(glyphs: usize) -> Report {
        Report {
            checked_frame: 0,
            primary: None,
            faces: vec![],
            unresolved_glyphs: 0,
            samples: vec![],
            glyphs,
            composed_lines: 1,
            overflow_lines: 0,
            status: Status::Complete,
            truncated: false,
        }
    }
    fn assert_invalidated(mut editor: Editor, font: &TextFont, edit: Command) {
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, font, 0).unwrap();
        editor.execute(edit).unwrap();
        session.validate(editor.project(), 7, Some(font), true, 0);
        assert!(session.check.is_none());
        assert!(job.cancelled());
        assert!(!session.accept(job.serial, 0, Ok(report(1))));
        session.finish(job.serial);
        assert!(!session.busy());
    }

    fn report_at(glyphs: usize, frame: Frame) -> Report {
        Report {
            checked_frame: frame,
            ..report(glyphs)
        }
    }
    fn animate(editor: &mut Editor, id: LayerId, parameter: TextParam, value: f64) {
        for edit in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value { frame: 40, value },
        ] {
            editor
                .execute(Command::EditText {
                    id,
                    parameter,
                    edit,
                })
                .unwrap();
        }
    }

    #[test]
    fn glyph_check_samples_active_local_frame_and_inactive_zero_including_disabled_layers() {
        let mut editor = Editor::default();
        add_text(&mut editor, "one two three four five");
        let id = editor.selected().unwrap();
        editor
            .execute(Command::SetTextStyle {
                id,
                style: TextStyle {
                    paragraph: true,
                    fill_enabled: false,
                    stroke_enabled: false,
                    ..Default::default()
                },
            })
            .unwrap();
        animate(&mut editor, id, TextParam::FontSize, 192.0);
        animate(&mut editor, id, TextParam::Tracking, 250.0);
        animate(&mut editor, id, TextParam::Leading, 3.0);
        editor.execute(Command::ToggleVisible(id)).unwrap();
        editor.execute(Command::ToggleLocked(id)).unwrap();
        editor.execute(Command::DuplicateComposition).unwrap();
        let source = editor.project().clone();
        let font = TextFont::of(&TextStyle::default());
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 40).unwrap();
        assert_eq!(
            job.snapshot.active_composition_id,
            source.active_composition_id()
        );
        assert_eq!(job.snapshot.active_frame, 40);
        assert_eq!(job.snapshot.layers.len(), 2);
        let mut counts = BTreeMap::new();
        for (index, frozen) in job.snapshot.layers.iter().enumerate() {
            let frame = if frozen.usage.composition_id == source.active_composition_id() {
                40
            } else {
                0
            };
            assert_eq!(frozen.checked_frame, frame);
            assert!(!frozen.layer.visible());
            assert!(frozen.usage.locked);
            assert!(!frozen.layer.text_style().fill_enabled);
            assert!(!frozen.layer.text_style().stroke_enabled);
            let sampled = frozen.layer.text_typography_at(frame).unwrap();
            assert_eq!(sampled.font_size, if frame == 0 { 32.0 } else { 192.0 });
            let report = crate::font_coverage::analyze(&frozen.layer, frozen.checked_frame);
            assert_eq!(report.checked_frame, frame);
            counts.insert(
                frame,
                (report.composed_lines, report.overflow_lines, report.glyphs),
            );
            assert!(session.accept(job.serial, index, Ok(report)));
        }
        session.finish(job.serial);
        assert_eq!(session.check.as_ref().unwrap().phase, CheckPhase::Finished);
        assert!(counts[&0].0 > counts[&40].0);
        assert!(counts[&0].2 > counts[&40].2);
        assert!(counts[&40].1 > 0);
        assert_eq!(editor.project(), &source);
        assert_eq!(inventory(editor.project()).len(), 1);
        assert_eq!(missing_count(editor.project()), 0);
    }

    #[test]
    fn glyph_check_seek_invalidates_partial_results_and_rejects_late_worker_races() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let old = session.start(editor.project(), 7, &font, 42).unwrap();
        assert!(session.accept(old.serial, 0, Ok(report_at(1, 42))));
        session.validate(editor.project(), 7, Some(&font), true, 43);
        assert!(session.check.is_none());
        assert!(old.cancelled());
        assert!(session.busy());
        assert!(session.start(editor.project(), 7, &font, 43).is_err());
        assert!(!session.accept(old.serial, 1, Ok(report_at(2, 42))));
        session.finish(old.serial);
        let new = session.start(editor.project(), 7, &font, 43).unwrap();
        assert_ne!(old.serial, new.serial);
        assert!(!session.accept(old.serial, 0, Ok(report_at(9, 42))));
        session.finish(old.serial);
        assert!(session.busy());
        assert!(session.accept(new.serial, 0, Ok(report_at(2, 43))));
        assert_eq!(session.check.as_ref().unwrap().reports[0].checked_frame, 43);
        session.cancel();
        session.finish(new.serial);
        assert!(!session.busy());
    }

    #[test]
    fn glyph_check_composition_switch_invalidates_even_with_identical_layers_and_frame() {
        let (mut editor, font) = scene();
        let original_id = editor.project().active_composition_id();
        editor.execute(Command::DuplicateComposition).unwrap();
        let duplicate_id = editor.project().active_composition_id();
        let mut session = CoverageSession::default();
        let old = session.start(editor.project(), 7, &font, 42).unwrap();
        editor.activate_composition(original_id).unwrap();
        session.validate(editor.project(), 7, Some(&font), true, 42);
        assert!(session.check.is_none());
        assert!(old.cancelled());
        editor.activate_composition(duplicate_id).unwrap();
        session.validate(editor.project(), 7, Some(&font), true, 42);
        assert!(!session.accept(old.serial, 0, Ok(report_at(1, 0))));
        session.finish(old.serial);
        assert!(!session.busy());
    }

    #[test]
    fn glyph_check_mismatched_result_frame_fails_and_cancels_without_mislabeling() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 24).unwrap();
        assert!(!session.accept(job.serial, 0, Ok(report_at(1, 0))));
        assert!(job.cancelled());
        assert!(session.check.as_ref().unwrap().reports.is_empty());
        assert!(matches!(
            session.check.as_ref().unwrap().phase,
            CheckPhase::Failed(_)
        ));
        session.finish(job.serial);
        assert!(!session.busy());
    }

    #[test]
    fn glyph_check_typography_key_edits_invalidate_without_revision_change() {
        let (mut editor, font) = scene();
        animate(&mut editor, 1, TextParam::FontSize, 96.0);
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 20).unwrap();
        editor
            .execute(Command::EditText {
                id: 1,
                parameter: TextParam::FontSize,
                edit: TrackEdit::Value {
                    frame: 40,
                    value: 128.0,
                },
            })
            .unwrap();
        session.validate(editor.project(), 7, Some(&font), true, 20);
        assert!(job.cancelled());
        assert!(session.check.is_none());
        assert!(!session.accept(job.serial, 0, Ok(report_at(1, 20))));
        session.finish(job.serial);
    }

    #[test]
    fn font_replacement_preserves_static_typography_and_every_animation_track() {
        let mut editor = Editor::default();
        add_text(&mut editor, "Animated title");
        let id = editor.selected().unwrap();
        let style = TextStyle {
            font_family: "Missing animated-font QA".into(),
            tracking: 125.0,
            leading: 1.75,
            paragraph: true,
            ..Default::default()
        };
        editor
            .execute(Command::SetTextStyle {
                id,
                style: style.clone(),
            })
            .unwrap();
        for (parameter, value) in [
            (TextParam::FontSize, 96.0),
            (TextParam::Tracking, 500.0),
            (TextParam::Leading, 3.0),
            (TextParam::FillRed, 120.0),
        ] {
            animate(&mut editor, id, parameter, value);
        }
        let before = editor.project().clone();
        let original = before.composition().layer(id).unwrap();
        let from = TextFont::of(&style);
        let to = TextFont::of(&crate::fonts::resolved(&style));
        let plan = Replacement::new(&before, 7, from, to.clone()).unwrap();
        editor
            .execute(plan.command(editor.project(), 7).unwrap())
            .unwrap();
        let replaced = editor.selected_layer().unwrap();
        let mut expected_style = style;
        to.apply(&mut expected_style);
        assert_eq!(replaced.text_style(), expected_style);
        assert_eq!(replaced.content(), original.content());
        assert_eq!(
            (replaced.width(), replaced.height()),
            (original.width(), original.height())
        );
        for parameter in TextParam::ALL {
            assert_eq!(
                replaced.track(PropertyPath::Text(parameter)),
                original.track(PropertyPath::Text(parameter))
            );
        }
        for frame in [0, 20, 40, 100] {
            assert_eq!(
                replaced.text_typography_at(frame),
                original.text_typography_at(frame)
            );
        }
        assert_eq!(missing_count(editor.project()), 0);
        let after = editor.project().clone();
        editor.undo();
        assert_eq!(editor.project(), &before);
        editor.redo();
        assert_eq!(editor.project(), &after);
    }

    #[test]
    fn glyph_check_uses_ids_when_composition_and_layer_names_are_identical() {
        let (mut editor, font) = scene();
        editor.execute(Command::DuplicateComposition).unwrap();
        editor
            .execute(Command::ConfigureComposition {
                name: "Composition 01".into(),
                width: 1920,
                height: 1080,
                fps: 30,
                duration: 300,
            })
            .unwrap();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        assert_eq!(job.snapshot.layers.len(), 4);
        let ids: std::collections::BTreeSet<_> = job
            .snapshot
            .layers
            .iter()
            .map(|f| (f.usage.composition_id, f.usage.layer_id))
            .collect();
        assert_eq!(ids.len(), 4);
        assert!(
            job.snapshot
                .layers
                .iter()
                .all(|f| f.usage.layer == "Same layer")
        );
        assert!(
            job.snapshot
                .layers
                .iter()
                .all(|f| f.usage.composition == "Composition 01")
        );
        for index in 0..4 {
            assert!(session.accept(job.serial, index, Ok(report(index + 1))));
        }
        session.finish(job.serial);
        let check = session.check.as_ref().unwrap();
        assert_eq!(check.phase, CheckPhase::Finished);
        for (i, actual) in check.reports.iter().enumerate() {
            assert_eq!(actual.glyphs, i + 1);
            assert_eq!(
                check.snapshot.layers[i].usage.layer_id,
                job.snapshot.layers[i].layer.id()
            );
        }
        let group = inventory(editor.project()).remove(0);
        assert_eq!(
            group
                .usages
                .iter()
                .map(|u| (u.composition_id, u.layer_id))
                .collect::<std::collections::BTreeSet<_>>(),
            ids
        );
    }

    #[test]
    fn glyph_check_source_edits_invalidate_without_a_revision_change() {
        let (editor, font) = scene();
        assert_invalidated(
            editor,
            &font,
            Command::SetContent {
                id: 1,
                content: Content::Text {
                    text: "Changed".into(),
                    font_size: 32.0,
                },
            },
        );
    }
    #[test]
    fn glyph_check_font_size_edits_invalidate_without_a_revision_change() {
        let (editor, font) = scene();
        assert_invalidated(
            editor,
            &font,
            Command::SetContent {
                id: 1,
                content: Content::Text {
                    text: "First".into(),
                    font_size: 48.0,
                },
            },
        );
    }
    #[test]
    fn glyph_check_style_edits_invalidate_without_a_revision_change() {
        let (editor, font) = scene();
        assert_invalidated(
            editor,
            &font,
            Command::SetTextStyle {
                id: 1,
                style: TextStyle {
                    tracking: 100.0,
                    ..Default::default()
                },
            },
        );
    }
    #[test]
    fn glyph_check_paragraph_boxes_invalidate_without_a_revision_change() {
        let (mut editor, font) = scene();
        editor
            .execute(Command::SetTextStyle {
                id: 1,
                style: TextStyle {
                    paragraph: true,
                    ..Default::default()
                },
            })
            .unwrap();
        assert_invalidated(
            editor,
            &font,
            Command::SetTextBox {
                id: 1,
                width: 80.0,
                height: 40.0,
            },
        );
    }
    #[test]
    fn glyph_check_new_matching_layer_invalidates_even_when_prior_layers_match() {
        let (mut editor, font) = scene();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        add_text(&mut editor, "Third");
        session.validate(editor.project(), 7, Some(&font), true, 0);
        assert!(session.check.is_none());
        assert!(job.cancelled());
    }
    #[test]
    fn glyph_check_document_replacement_invalidates_identical_content() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        session.validate(editor.project(), 8, Some(&font), true, 0);
        assert!(session.check.is_none());
        assert!(job.cancelled());
    }
    #[test]
    fn glyph_check_selection_change_discards_and_cancels_the_job() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        let other = TextFont {
            weight: 700,
            ..font.clone()
        };
        session.validate(editor.project(), 7, Some(&other), true, 0);
        assert!(session.check.is_none());
        assert!(job.cancelled());
        session.validate(editor.project(), 7, Some(&font), true, 0);
        assert!(!session.accept(job.serial, 0, Ok(report(1))));
    }
    #[test]
    fn glyph_check_close_reopen_rejects_late_results_and_preserves_one_worker() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let old = session.start(editor.project(), 7, &font, 0).unwrap();
        session.validate(editor.project(), 7, Some(&font), false, 0);
        session.validate(editor.project(), 7, Some(&font), true, 0);
        assert!(session.start(editor.project(), 7, &font, 0).is_err());
        assert!(!session.accept(old.serial, 0, Ok(report(1))));
        session.finish(old.serial);
        assert!(session.check.is_none());
        let new = session.start(editor.project(), 7, &font, 0).unwrap();
        assert_ne!(old.serial, new.serial);
        assert!(!session.accept(old.serial, 0, Ok(report(100))));
        session.finish(old.serial);
        assert!(session.busy());
        assert!(session.accept(new.serial, 0, Ok(report(2))));
        assert_eq!(session.check.as_ref().unwrap().reports[0].glyphs, 2);
    }
    #[test]
    fn glyph_check_cancel_keeps_only_finished_layer_reports() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        assert!(session.accept(job.serial, 0, Ok(report(1))));
        session.cancel();
        assert!(job.cancelled());
        assert!(!session.accept(job.serial, 1, Ok(report(2))));
        assert!(session.start(editor.project(), 7, &font, 0).is_err());
        session.finish(job.serial);
        let check = session.check.as_ref().unwrap();
        assert_eq!(check.phase, CheckPhase::Cancelled);
        assert_eq!(check.reports.len(), 1);
        assert_eq!(check.snapshot.layers.len(), 2);
        assert!(!session.busy());
    }
    #[test]
    fn glyph_check_duplicate_out_of_order_and_failed_results_are_not_success() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        assert!(!session.accept(job.serial, 1, Ok(report(2))));
        assert!(session.accept(job.serial, 0, Ok(report(1))));
        assert!(!session.accept(job.serial, 0, Ok(report(3))));
        assert!(!session.accept(job.serial, 1, Err("Worker failure".into())));
        session.finish(job.serial);
        assert_eq!(
            session.check.as_ref().unwrap().phase,
            CheckPhase::Failed("Worker failure".into())
        );
        assert_eq!(session.check.as_ref().unwrap().reports.len(), 1);
    }
    #[test]
    fn glyph_check_dropped_worker_and_empty_reports_do_not_become_success() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        session.finish(job.serial);
        assert!(matches!(
            session.check.as_ref().unwrap().phase,
            CheckPhase::Failed(_)
        ));
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        let mut empty = report(0);
        empty.status = Status::Empty;
        assert!(session.accept(job.serial, 0, Ok(empty)));
        session.cancel();
        session.finish(job.serial);
        assert_eq!(
            session.check.as_ref().unwrap().reports[0].status,
            Status::Empty
        );
    }
    #[test]
    fn glyph_check_does_not_touch_source_selection_undo_or_redo() {
        let (mut editor, font) = scene();
        let expected_redo = editor.project().clone();
        editor.undo();
        let source = editor.project().clone();
        let selected = editor.selected();
        let history = (editor.can_undo(), editor.can_redo());
        let missing = missing_count(&source);
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        assert!(session.accept(
            job.serial,
            0,
            Ok(crate::font_coverage::analyze(
                &job.snapshot.layers[0].layer,
                job.snapshot.layers[0].checked_frame,
            ))
        ));
        session.finish(job.serial);
        session.cancel();
        session.invalidate();
        assert_eq!(editor.project(), &source);
        assert_eq!(editor.selected(), selected);
        assert_eq!((editor.can_undo(), editor.can_redo()), history);
        assert_eq!(missing_count(editor.project()), missing);
        editor.redo();
        assert_eq!(editor.project(), &expected_redo);
    }
    #[test]
    fn glyph_check_replacement_and_undo_cannot_restore_an_old_report() {
        let (mut editor, _) = scene();
        let style = TextStyle {
            font_family: "Missing glyph-check test".into(),
            ..Default::default()
        };
        let font = TextFont::of(&style);
        for id in [1, 2] {
            editor
                .execute(Command::SetTextStyle {
                    id,
                    style: style.clone(),
                })
                .unwrap();
        }
        editor.execute(Command::ToggleLocked(2)).unwrap();
        let original = editor.project().clone();
        let mut session = CoverageSession::default();
        let old = session.start(editor.project(), 7, &font, 0).unwrap();
        assert_eq!(old.snapshot.layers.len(), 2);
        assert!(
            old.snapshot
                .layers
                .iter()
                .find(|f| f.usage.layer_id == 2)
                .unwrap()
                .usage
                .locked
        );
        let replacement = TextFont::of(&crate::fonts::resolved(&style));
        let plan =
            Replacement::new(editor.project(), 7, font.clone(), replacement.clone()).unwrap();
        assert_eq!((plan.count, plan.locked), (1, 1));
        editor
            .execute(plan.command(editor.project(), 7).unwrap())
            .unwrap();
        let changed = editor.project().clone();
        session.validate(editor.project(), 7, Some(&font), true, 0);
        assert!(old.cancelled());
        assert!(session.check.is_none());
        session.finish(old.serial);
        let current = session.start(editor.project(), 7, &replacement, 0).unwrap();
        editor.undo();
        assert_eq!(editor.project(), &original);
        session.validate(editor.project(), 7, Some(&replacement), true, 0);
        assert!(current.cancelled());
        assert!(!session.accept(old.serial, 0, Ok(report(1))));
        assert!(!session.accept(current.serial, 0, Ok(report(1))));
        editor.redo();
        assert_eq!(editor.project(), &changed);
        assert!(session.check.is_none());
    }
    #[test]
    fn glyph_check_bounded_job_retains_explicit_unexamined_layer_count() {
        let (editor, font) = scene();
        let mut snapshot = CoverageSnapshot::capture(editor.project(), 7, &font, 0);
        snapshot
            .layers
            .resize(MAX_CHECK_LAYERS + 1, snapshot.layers[0].clone());
        assert_eq!(snapshot.target(), MAX_CHECK_LAYERS);
        assert_eq!(snapshot.layers.len() - snapshot.target(), 1);
    }
    #[test]
    fn glyph_check_drop_signals_cancellation() {
        let (editor, font) = scene();
        let mut session = CoverageSession::default();
        let job = session.start(editor.project(), 7, &font, 0).unwrap();
        drop(session);
        assert!(job.cancelled());
    }
}
