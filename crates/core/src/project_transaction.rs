//! Validated, source-preserving drafts for bounded automation hosts.
use super::*;

fn required_version(command: &Command, depth: usize) -> Result<u32, String> {
    if depth > 8 {
        return Err("Automation command nesting limit exceeded".into());
    }
    Ok(match command {
        Command::Batch(commands) if commands.len() <= 10_000 => commands
            .iter()
            .map(|command| required_version(command, depth + 1))
            .try_fold(1, |version, next| next.map(|next| version.max(next)))?,
        Command::SetOpacityTiming { .. } => 1,
        Command::SetMaskFeatherKernel { .. } => 83,
        Command::SetCompositingProfile { .. } => 84,
        Command::SetRichText { .. } => 71,
        Command::SetLayerRangeSamples { .. } => 81,
        Command::ReplaceTextRange { .. } => 1,
        Command::DuplicateLayer(_)
        | Command::RenameLayer { .. }
        | Command::RemoveLayer(_)
        | Command::MoveLayer { .. }
        | Command::ToggleVisible(_)
        | Command::SetLayerRange { .. }
        | Command::SetLayerStart { .. }
        | Command::SetLayerLabel { .. }
        | Command::SetValue { .. }
        | Command::ToggleKeyframe { .. }
        | Command::EditSourceText { .. } => 1,
        Command::SetInterpolation { interpolation, .. } => {
            if matches!(interpolation, Interpolation::Bezier(_)) {
                2
            } else {
                1
            }
        }
        Command::SetThreeD { .. }
        | Command::SetSpatialPosition { .. }
        | Command::SetSpatialParent { .. }
        | Command::SetCamera { .. } => 72,
        Command::SetPlanarPosition { .. }
        | Command::EditPlanarPosition { .. }
        | Command::SetPlanarParent { .. } => 75,
        Command::SetExpression { .. }
        | Command::SetExpressionEnabled { .. }
        | Command::RemoveExpression { .. } => 1,
        Command::Effect { .. } => 12,
        Command::Marker { .. } => 13,
        Command::SetTemporalHandle { .. } => 35,
        Command::SetTemporalMode { mode, .. } => {
            if mode.is_independent() {
                35
            } else {
                36
            }
        }
        _ => return Err("This command is outside the bounded automation API".into()),
    })
}

impl Project {
    /// Validate an automation input without normalizing its schema, assets or sources.
    pub fn validate_automation_project(&self) -> Result<(), String> {
        self.validate()?;
        document::validate_budget(self)
    }

    /// Apply one admitted command atomically to a detached project. The active
    /// composition, unrelated source data and schema floor remain unchanged.
    /// This deliberately bypasses the general-edit asset/schema migration route.
    pub fn apply_automation_command(
        &mut self,
        composition: CompositionId,
        command: Command,
    ) -> Result<(), String> {
        self.validate_automation_project()?;
        let minimum_version = required_version(&command, 0)?;
        let active = self.active_composition_id();
        let mut next = Snapshot {
            project: self.clone(),
            selected: None,
        };
        next.project.activate_composition(composition)?;
        apply(&mut next, command)?;
        next.project.activate_composition(active)?;
        if next.project != *self {
            next.project.version = next.project.version.max(minimum_version);
            if expressions::materialized(&next.project) {
                next.project.version = next.project.version.max(65);
            }
            if layer_timing::materialized(&next.project) {
                next.project.version = next.project.version.max(64);
            }
            if let Some(version) = rich_text::required_version(&next.project) {
                next.project.version = next.project.version.max(version);
            }
            if spatial::materialized(&next.project) {
                next.project.version = next.project.version.max(72);
            }
            if opacity_timing::materialized(&next.project) {
                next.project.version = next.project.version.max(73);
            }
            if planar::materialized(&next.project) {
                next.project.version = next.project.version.max(75);
            }
            if let Some(version) = audio_spectrum::required_version(&next.project) {
                next.project.version = next.project.version.max(version);
            }
            next.project.validate_automation_project()?;
            *self = next.project;
        }
        Ok(())
    }
}

impl Editor {
    /// Accept a completed automation draft as exactly one undoable source edit.
    /// Invalid drafts and exact no-ops preserve the project, selection, both
    /// history stacks and context receipt. Existing valid selection is retained.
    /// Callers must check their captured context receipt before committing an
    /// asynchronously evaluated draft.
    pub fn commit_automation_project(&mut self, mut project: Project) -> Result<bool, String> {
        self.current.project.validate_automation_project()?;
        project.validate_automation_project()?;
        project.validate_spatial_animation()?;
        project.validate_planar_animation()?;
        project.validate_opacity_animation()?;
        project.activate_composition(self.current.project.active_composition_id())?;
        if project == self.current.project {
            return Ok(false);
        }
        let selected = self
            .current
            .selected
            .filter(|id| project.composition.layer(*id).is_some());
        self.accept_candidate(Snapshot { project, selected })?;
        Ok(true)
    }
}
