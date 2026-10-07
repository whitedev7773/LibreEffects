//! Source-bound numeric expression drafts. This module never executes JavaScript.
//! The desktop evaluates `ExpressionCheck` in its supervised background process.
use libre_effects_core::{
    Command, CompositionId, Editor, EffectKind, ExpressionTarget, Frame, LayerId,
    MAX_EXPRESSION_SOURCE_BYTES, Project, expression_runtime as ae,
};

const EXPIRED: &str = "Expression edit expired. Reopen it from the current layer and frame.";

#[derive(Clone, Debug)]
pub struct ExpressionDraft {
    baseline: Project,
    generation: u64,
    composition: CompositionId,
    layer: LayerId,
    target: ExpressionTarget,
    frame: Frame,
    source: String,
    enabled: bool,
    revision: u64,
    existing: bool,
}

pub enum PreparedExpression {
    /// Disabled drafts need no guest execution; None preserves an unchanged one.
    Ready(Option<Command>),
    Check(ExpressionCheck),
}

pub struct ExpressionCheck {
    pub snapshot: ae::CompositionSnapshot,
    pub roots: Vec<ae::PropertyAddress>,
    candidate: Project,
    composition: CompositionId,
    layer: LayerId,
    target: ExpressionTarget,
    frame: Frame,
    generation: u64,
    revision: u64,
    source: String,
    enabled: bool,
}

fn property(
    project: &Project,
    layer_id: LayerId,
    target: ExpressionTarget,
) -> Result<ae::ExpressionProperty, String> {
    let layer = project
        .composition()
        .layer(layer_id)
        .ok_or("Layer not found")?;
    Ok(match target {
        ExpressionTarget::Position => ae::ExpressionProperty::Position,
        ExpressionTarget::Scale => ae::ExpressionProperty::Scale,
        ExpressionTarget::Opacity => ae::ExpressionProperty::Opacity,
        ExpressionTarget::SourceText => {
            if layer.source_text_at(0).is_none() {
                return Err("Select a text layer".into());
            }
            ae::ExpressionProperty::SourceText
        }
        ExpressionTarget::MaskPath(id) => {
            if !layer.path_masks().iter().any(|mask| mask.id == id) {
                return Err("Select an existing mask".into());
            }
            ae::ExpressionProperty::MaskPath(id)
        }
        ExpressionTarget::Slider(id) => {
            let effect = layer
                .effect_stack()
                .iter()
                .find(|effect| effect.id() == id && effect.kind() == EffectKind::SliderControl)
                .ok_or("Select an existing Slider Control")?;
            // The expression host deliberately resolves the first exact name.
            // Do not check one control and then edit a different same-name one.
            if layer
                .effect_stack()
                .iter()
                .find(|item| {
                    item.kind() == EffectKind::SliderControl && item.name() == effect.name()
                })
                .is_none_or(|first| first.id() != id)
            {
                return Err("Rename this Slider Control before editing its expression; an earlier control has the same name".into());
            }
            ae::ExpressionProperty::Slider(effect.name().into())
        }
    })
}

impl ExpressionDraft {
    pub fn open(
        editor: &Editor,
        layer: LayerId,
        target: ExpressionTarget,
        frame: Frame,
    ) -> Result<Self, String> {
        let project = editor.project();
        if project.evaluated_frame().is_some() {
            return Err("Edit the authored project, not an evaluated frame".into());
        }
        let composition = project.composition();
        if frame >= composition.duration() {
            return Err("Expression frame is outside the composition".into());
        }
        if composition.has_spatial_layers() {
            return Err("Numeric expression editing currently requires a 2D composition".into());
        }
        if editor.selected() != Some(layer) {
            return Err("Select the layer before editing its expression".into());
        }
        let source_layer = composition.layer(layer).ok_or("Layer not found")?;
        if source_layer.locked() {
            return Err("Unlock the layer before editing expressions".into());
        }
        property(project, layer, target)?;
        let existing = source_layer.expression(target);
        Ok(Self {
            baseline: project.clone(),
            generation: editor.context_generation(),
            composition: project.active_composition_id(),
            layer,
            target,
            frame,
            source: existing.map_or_else(|| "value".into(), |program| program.source.clone()),
            enabled: existing.is_none_or(|program| program.enabled),
            revision: 0,
            existing: existing.is_some(),
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn has_existing(&self) -> bool {
        self.existing
    }
    pub fn layer(&self) -> LayerId {
        self.layer
    }
    pub fn target(&self) -> ExpressionTarget {
        self.target
    }
    pub fn frame(&self) -> Frame {
        self.frame
    }
    pub fn composition(&self) -> CompositionId {
        self.composition
    }
    pub fn set_source(&mut self, source: &str) -> Result<bool, String> {
        if source.len() > MAX_EXPRESSION_SOURCE_BYTES || source.contains('\0') {
            return Err("Expression source must be at most 16384 bytes with no NUL".into());
        }
        if self.source == source {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Expression draft revision exhausted")?;
        self.source = source.into();
        self.revision = revision;
        Ok(true)
    }
    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        if self.enabled == enabled {
            return false;
        }
        self.enabled = enabled;
        self.revision = self
            .revision
            .checked_add(1)
            .expect("Expression draft revision exhausted");
        true
    }
    pub fn current(&self, editor: &Editor, frame: Frame) -> bool {
        self.generation == editor.context_generation()
            && self.composition == editor.project().active_composition_id()
            && self.frame == frame
            && editor.selected() == Some(self.layer)
            && self.baseline == *editor.project()
    }
    fn command(&self) -> Command {
        Command::SetExpression {
            id: self.layer,
            target: self.target,
            source: self.source.clone(),
            enabled: self.enabled,
        }
    }
    pub fn prepare(&self, editor: &Editor, frame: Frame) -> Result<PreparedExpression, String> {
        if !self.current(editor, frame) {
            return Err(EXPIRED.into());
        }
        if self.source.is_empty() {
            return Err("Enter an expression, or use Remove expression".into());
        }
        let mut candidate = Editor::default();
        candidate.replace_project(self.baseline.clone())?;
        candidate.execute(self.command())?;
        let candidate = candidate.project().clone();
        if !self.enabled {
            return Ok(PreparedExpression::Ready(
                (candidate != self.baseline).then(|| self.command()),
            ));
        }
        let snapshot = candidate.expression_snapshot(self.composition, frame)?;
        // Core's evaluated-view admission requires the current visible/guide
        // roots. Add the edited property even when a Null/template is hidden.
        let mut roots = candidate.expression_roots(self.composition, frame, true)?;
        let requested = ae::PropertyAddress {
            composition: ae::CompositionId(self.composition),
            layer: ae::LayerId(self.layer),
            property: property(&candidate, self.layer, self.target)?,
        };
        if !roots.contains(&requested) {
            roots.push(requested);
        }
        Ok(PreparedExpression::Check(ExpressionCheck {
            snapshot,
            roots,
            candidate,
            composition: self.composition,
            layer: self.layer,
            target: self.target,
            frame,
            generation: self.generation,
            revision: self.revision,
            source: self.source.clone(),
            enabled: self.enabled,
        }))
    }
    /// Validate a supervised result and return only an authored-source command.
    /// The evaluated frame view is deliberately discarded, never committed.
    pub fn finish(
        &self,
        editor: &Editor,
        frame: Frame,
        check: &ExpressionCheck,
        evaluated: Result<ae::EvaluatedProperties, String>,
    ) -> Result<Option<Command>, String> {
        if !self.current(editor, frame)
            || check.generation != self.generation
            || check.revision != self.revision
            || check.source != self.source
            || check.enabled != self.enabled
            || check.composition != self.composition
            || check.layer != self.layer
            || check.target != self.target
            || check.frame != frame
        {
            return Err(EXPIRED.into());
        }
        let evaluated = evaluated?;
        if check
            .roots
            .iter()
            .any(|root| !evaluated.values.contains_key(root))
        {
            return Err("Expression check did not return every requested property".into());
        }
        check
            .candidate
            .with_evaluated_properties(self.composition, frame, true, &evaluated)?;
        Ok((check.candidate != self.baseline).then(|| self.command()))
    }
    pub fn remove_command(&self, editor: &Editor, frame: Frame) -> Result<Option<Command>, String> {
        if !self.current(editor, frame) {
            return Err(EXPIRED.into());
        }
        Ok(self.existing.then_some(Command::RemoveExpression {
            id: self.layer,
            target: self.target,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{
        EffectEdit, OpacityEase, OpacityEdit, OpacityInterpolation, Property, project_file,
    };
    fn fixture() -> Editor {
        let mut editor = Editor::default();
        editor
            .execute(Command::ConfigureCompositionRate {
                name: "Draft".into(),
                width: 640,
                height: 360,
                fps: 60.into(),
                duration: 300,
                display_start: 0,
            })
            .unwrap();
        editor.execute(Command::AddRectangle).unwrap();
        editor.clear_history();
        editor
    }
    fn prepared_check(draft: &ExpressionDraft, editor: &Editor) -> ExpressionCheck {
        match draft.prepare(editor, draft.frame()).unwrap() {
            PreparedExpression::Check(check) => check,
            _ => panic!("Expected background check"),
        }
    }
    fn evaluate(check: &ExpressionCheck) -> Result<ae::EvaluatedProperties, String> {
        ae::ExpressionEvaluator::default()
            .evaluate(&check.snapshot, &check.roots)
            .map_err(|error| error.to_string())
    }
    fn bytes(editor: &Editor) -> Vec<u8> {
        project_file::encode(editor.project(), None).unwrap()
    }
    #[test]
    fn validated_apply_changes_only_authored_program_in_one_undo() {
        let mut editor = fixture();
        editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                value: 47.,
            })
            .unwrap();
        editor.clear_history();
        let baseline = bytes(&editor);
        let mut draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Position, 12).unwrap();
        draft.set_source("[value[0]+3,value[1]-4]").unwrap();
        let check = prepared_check(&draft, &editor);
        assert_eq!(bytes(&editor), baseline);
        let result = evaluate(&check);
        let command = draft.finish(&editor, 12, &check, result).unwrap().unwrap();
        editor.execute(command).unwrap();
        assert_eq!(
            editor
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .property(Property::PositionX)
                .unwrap()
                .value_at(0),
            47.
        );
        assert_eq!(editor.project().evaluated_frame(), None);
        let applied = bytes(&editor);
        assert_ne!(applied, baseline);
        editor.undo();
        assert_eq!(bytes(&editor), baseline);
        assert!(!editor.can_undo());
        editor.redo();
        assert_eq!(bytes(&editor), applied);
        let loaded = project_file::decode(&applied).unwrap();
        assert_eq!(
            project_file::encode(&loaded.project, loaded.view).unwrap(),
            applied
        );
    }
    #[test]
    fn disabled_source_is_saved_without_a_check_or_execution() {
        let mut editor = fixture();
        let mut draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).unwrap();
        draft.set_source("while(true){}").unwrap();
        draft.set_enabled(false);
        let PreparedExpression::Ready(Some(command)) = draft.prepare(&editor, 0).unwrap() else {
            panic!("Disabled code must not execute")
        };
        editor.execute(command).unwrap();
        let program = editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .expression(ExpressionTarget::Opacity)
            .unwrap();
        assert_eq!(program.source, "while(true){}");
        assert!(!program.enabled);
    }
    #[test]
    fn failures_leave_source_redo_and_the_editable_draft_intact() {
        for source in [
            "(",
            "thisComp.layer('missing').opacity",
            "thisLayer.opacity",
        ] {
            let mut editor = fixture();
            editor
                .execute(Command::RenameLayer {
                    id: 1,
                    name: "Redo".into(),
                })
                .unwrap();
            editor.undo();
            let baseline = bytes(&editor);
            let generation = editor.context_generation();
            let mut draft =
                ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).unwrap();
            draft.set_source(source).unwrap();
            let check = prepared_check(&draft, &editor);
            assert!(draft.finish(&editor, 0, &check, evaluate(&check)).is_err());
            assert_eq!(draft.source(), source);
            assert_eq!(bytes(&editor), baseline);
            assert_eq!(editor.context_generation(), generation);
            assert!(editor.can_redo());
            assert!(!editor.can_undo());
        }
    }
    #[test]
    fn no_op_preserves_crlf_and_the_existing_redo_branch() {
        let mut editor = fixture();
        let source = "// stored\r\nvalue\r\n";
        editor
            .execute(Command::SetExpression {
                id: 1,
                target: ExpressionTarget::Opacity,
                source: source.into(),
                enabled: true,
            })
            .unwrap();
        editor.clear_history();
        editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Redo".into(),
            })
            .unwrap();
        editor.undo();
        let baseline = bytes(&editor);
        let draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).unwrap();
        assert_eq!(draft.source().as_bytes(), source.as_bytes());
        let check = prepared_check(&draft, &editor);
        assert!(
            draft
                .finish(&editor, 0, &check, evaluate(&check))
                .unwrap()
                .is_none()
        );
        assert_eq!(bytes(&editor), baseline);
        assert!(editor.can_redo());
        assert!(!editor.can_undo());
    }
    #[test]
    fn draft_and_editor_aba_frame_selection_and_cross_target_results_expire() {
        let mut editor = fixture();
        editor.execute(Command::AddRectangle).unwrap();
        editor.select(1);
        editor.clear_history();
        let mut draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).unwrap();
        let check = prepared_check(&draft, &editor);
        let result = evaluate(&check).unwrap();
        draft.set_source("value/2").unwrap();
        draft.set_source("value").unwrap();
        assert!(
            draft
                .finish(&editor, 0, &check, Ok(result.clone()))
                .is_err()
        );
        let draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).unwrap();
        assert!(draft.prepare(&editor, 1).is_err());
        let other = ExpressionDraft::open(&editor, 1, ExpressionTarget::Scale, 0).unwrap();
        let other_check = prepared_check(&other, &editor);
        assert!(
            draft
                .finish(&editor, 0, &other_check, evaluate(&other_check))
                .is_err()
        );
        editor.select(2);
        editor.select(1);
        assert!(!draft.current(&editor, 0));
        let draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).unwrap();
        editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "temporary".into(),
            })
            .unwrap();
        editor.undo();
        assert!(!draft.current(&editor, 0));
    }
    #[test]
    fn result_time_dimensions_roots_and_dependency_cycles_are_admitted_again() {
        let editor = fixture();
        let draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Position, 0).unwrap();
        let check = prepared_check(&draft, &editor);
        let good = evaluate(&check).unwrap();
        let mut bad = good.clone();
        bad.time += 1.;
        assert!(draft.finish(&editor, 0, &check, Ok(bad)).is_err());
        let mut bad = good.clone();
        bad.values.clear();
        assert!(draft.finish(&editor, 0, &check, Ok(bad)).is_err());
        let root = check.roots[0].clone();
        let mut bad = good.clone();
        bad.values
            .insert(root.clone(), ae::PropertyValue::Scalar(2.));
        assert!(draft.finish(&editor, 0, &check, Ok(bad)).is_err());
        let mut bad = good;
        bad.dependencies.entry(root.clone()).or_default().push(root);
        assert!(draft.finish(&editor, 0, &check, Ok(bad)).is_err());
    }
    #[test]
    fn hidden_null_slider_is_a_requested_root_and_shadowed_names_reject() {
        let mut editor = fixture();
        editor.execute(Command::AddNull).unwrap();
        editor
            .execute(Command::Effect {
                id: 2,
                edit: EffectEdit::Add(EffectKind::SliderControl),
            })
            .unwrap();
        editor.execute(Command::ToggleVisible(2)).unwrap();
        editor.clear_history();
        let effect = editor
            .project()
            .composition()
            .layer(2)
            .unwrap()
            .effect_stack()[0]
            .id();
        let mut draft =
            ExpressionDraft::open(&editor, 2, ExpressionTarget::Slider(effect), 0).unwrap();
        draft.set_source("value+2").unwrap();
        let check = prepared_check(&draft, &editor);
        assert!(check.roots.iter().any(|root| root.layer == ae::LayerId(2)
            && matches!(root.property, ae::ExpressionProperty::Slider(_))));
        let command = draft
            .finish(&editor, 0, &check, evaluate(&check))
            .unwrap()
            .unwrap();
        editor.execute(command).unwrap();
        editor
            .execute(Command::Effect {
                id: 2,
                edit: EffectEdit::Add(EffectKind::SliderControl),
            })
            .unwrap();
        let second = editor
            .project()
            .composition()
            .layer(2)
            .unwrap()
            .effect_stack()[1]
            .id();
        assert!(
            ExpressionDraft::open(&editor, 2, ExpressionTarget::Slider(second), 0)
                .unwrap_err()
                .contains("same name")
        );
        assert!(ExpressionDraft::open(&editor, 2, ExpressionTarget::Slider(effect), 0).is_ok());
    }
    #[test]
    fn empty_invalid_removed_locked_and_spatial_targets_fail_explicitly() {
        let mut editor = fixture();
        let mut draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).unwrap();
        assert!(
            draft
                .set_source(&"x".repeat(MAX_EXPRESSION_SOURCE_BYTES + 1))
                .is_err()
        );
        assert!(draft.set_source("value\0").is_err());
        assert_eq!(draft.source(), "value");
        assert_eq!(draft.revision(), 0);
        draft.set_source("").unwrap();
        assert!(draft.prepare(&editor, 0).is_err());
        draft.set_enabled(false);
        assert!(draft.prepare(&editor, 0).is_err());
        assert!(draft.remove_command(&editor, 0).unwrap().is_none());
        assert!(ExpressionDraft::open(&editor, 1, ExpressionTarget::Slider(999), 0).is_err());
        editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).is_err());
        editor.execute(Command::ToggleLocked(1)).unwrap();
        editor
            .execute(Command::SetThreeD {
                id: 1,
                enabled: true,
            })
            .unwrap();
        assert!(ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 0).is_err());
    }
    #[test]
    fn native_opacity_timing_is_sampled_but_never_replaced_by_a_check() {
        let mut editor = fixture();
        for frame in [0, 60] {
            editor
                .execute(Command::SetOpacityTiming {
                    id: 1,
                    edit: OpacityEdit::Key { frame, value: 50. },
                })
                .unwrap();
        }
        for frame in [0, 60] {
            editor
                .execute(Command::SetOpacityTiming {
                    id: 1,
                    edit: OpacityEdit::Interpolation {
                        frame,
                        incoming: OpacityInterpolation::Bezier,
                        outgoing: OpacityInterpolation::Bezier,
                    },
                })
                .unwrap();
            editor
                .execute(Command::SetOpacityTiming {
                    id: 1,
                    edit: OpacityEdit::TemporalEase {
                        frame,
                        incoming: OpacityEase {
                            speed: 600.,
                            influence: 100. / 3.,
                        },
                        outgoing: OpacityEase {
                            speed: -600.,
                            influence: 100. / 3.,
                        },
                    },
                })
                .unwrap();
        }
        editor.clear_history();
        let original = editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .opacity_timing()
            .unwrap()
            .clone();
        let mut draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Opacity, 30).unwrap();
        draft.set_source("value+100").unwrap();
        let check = prepared_check(&draft, &editor);
        let command = draft
            .finish(&editor, 30, &check, evaluate(&check))
            .unwrap()
            .unwrap();
        editor.execute(command).unwrap();
        assert_eq!(
            editor
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .opacity_timing(),
            Some(&original)
        );
        assert_eq!(editor.project().evaluated_frame(), None);
    }
    #[test]
    fn explicit_removal_is_one_source_edit_with_unchanged_authored_values() {
        let mut editor = fixture();
        editor
            .execute(Command::SetExpression {
                id: 1,
                target: ExpressionTarget::Scale,
                source: "[90,90]".into(),
                enabled: true,
            })
            .unwrap();
        editor.clear_history();
        let original = bytes(&editor);
        let draft = ExpressionDraft::open(&editor, 1, ExpressionTarget::Scale, 0).unwrap();
        assert!(draft.has_existing());
        editor
            .execute(draft.remove_command(&editor, 0).unwrap().unwrap())
            .unwrap();
        assert!(
            editor
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .expression(ExpressionTarget::Scale)
                .is_none()
        );
        editor.undo();
        assert_eq!(bytes(&editor), original);
        assert!(!editor.can_undo());
    }
}
