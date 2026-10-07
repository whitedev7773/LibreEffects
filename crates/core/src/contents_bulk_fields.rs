//! Exact, source-preserving scalar assignments and animation actions on siblings.
use super::*;

/// Explicit shared animation intent. Mixed selections never toggle members in
/// opposite directions, and every inserted/static sample belongs to its member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentsAnimationAction {
    /// Static members acquire a current-frame key; animated tracks are untouched.
    Enable,
    /// Remove all keys, retaining each member's own clamped current-frame sample.
    Disable,
    /// Insert only missing current-frame keys; existing key metadata is untouched.
    AddKey,
    /// Remove only current-frame keys. Removing a final key retains its clamped
    /// sample as the static base, matching singleton ToggleKey behavior.
    RemoveKey,
}

impl ShapeContents {
    /// Eligible typed parameters shared by an exact, nonempty sibling selection.
    ///
    /// Call on a validated project. This checks the selection without changing
    /// source or history and preserves the first source sibling's display order,
    /// independently of click order or numeric ID order. Gradient stops have
    /// local identities and never establish correspondence between items.
    pub fn shared_parameters(
        &self,
        parent: u64,
        items: &[u64],
    ) -> Result<Vec<ContentsParam>, String> {
        let nodes = self.shared_siblings(parent, items)?;
        Ok(nodes[0]
            .parameter_order()
            .into_iter()
            .filter(|parameter| {
                nodes.iter().all(|node| {
                    node.parameters.contains_key(parameter)
                        && match parameter {
                            ContentsParam::Gradient(p) if p.stop().is_some() => false,
                            ContentsParam::Gradient(
                                GradientParam::HighlightLength | GradientParam::HighlightAngle,
                            ) => node.kind.gradient().is_some_and(|gradient| gradient.radial),
                            _ => true,
                        }
                })
            })
            .collect())
    }

    fn shared_siblings(&self, parent: u64, items: &[u64]) -> Result<Vec<&ContentsNode>, String> {
        let selected = items.iter().copied().collect::<BTreeSet<_>>();
        if selected.is_empty() || selected.len() != items.len() {
            return Err("Choose nonempty, distinct Contents siblings".into());
        }
        let siblings = if parent == 0 {
            &self.items
        } else {
            match &self
                .node(parent)
                .ok_or("Contents group no longer exists")?
                .kind
            {
                ContentsKind::Group(children) => children,
                _ => return Err("Choose a Contents group".into()),
            }
        };
        let nodes: Vec<_> = siblings
            .iter()
            .filter(|node| selected.contains(&node.id))
            .collect();
        if nodes.len() != items.len() {
            return Err("Choose immediate siblings of the Contents group".into());
        }
        Ok(nodes)
    }

    pub(super) fn set_shared_value(
        &mut self,
        parent: u64,
        items: &[u64],
        parameter: ContentsParam,
        frame: Frame,
        value: f64,
        duration: Frame,
    ) -> Result<(), String> {
        if frame >= duration {
            return Err("Frame is outside the composition".into());
        }
        if !parameter.accepts(value) {
            return Err("Invalid shared Contents property value".into());
        }
        if !self.shared_parameters(parent, items)?.contains(&parameter) {
            return Err("Choose a shared numeric Contents property".into());
        }
        // Plan from the complete original candidate before mutating any member.
        // Equality is exact and uses the rendered/clamped sample, retaining even
        // dormant base values, easing metadata and signed zero on unchanged tracks.
        let mut updates = Vec::new();
        for node in self.shared_siblings(parent, items)? {
            if node.value_at(parameter, frame) == value {
                continue;
            }
            let mut track = node.parameters[&parameter].clone();
            time_remap::edit_track(
                &mut track,
                duration,
                &TrackEdit::Value { frame, value },
                |value| parameter.accepts(value),
            )?;
            updates.push((node.id, track));
        }
        for (item, track) in updates {
            self.node_mut(item)
                .expect("planned sibling remains present")
                .parameters
                .insert(parameter, track);
        }
        Ok(())
    }

    pub(super) fn edit_shared_animation(
        &mut self,
        parent: u64,
        items: &[u64],
        parameter: ContentsParam,
        frame: Frame,
        action: ContentsAnimationAction,
        duration: Frame,
    ) -> Result<(), String> {
        // Validate even intentional no-ops. The editor validates original/final
        // project and metadata budgets around this source-preserving transaction.
        if frame >= duration {
            return Err("Frame is outside the composition".into());
        }
        if !self.shared_parameters(parent, items)?.contains(&parameter) {
            return Err("Choose a shared numeric Contents property".into());
        }
        let mut updates = Vec::new();
        for node in self.shared_siblings(parent, items)? {
            let original = &node.parameters[&parameter];
            let changes = match action {
                ContentsAnimationAction::Enable => original.keys.is_empty(),
                ContentsAnimationAction::Disable => !original.keys.is_empty(),
                ContentsAnimationAction::AddKey => !original.keys.contains_key(&frame),
                ContentsAnimationAction::RemoveKey => original.keys.contains_key(&frame),
            };
            if !changes {
                continue;
            }
            let value = node.value_at(parameter, frame);
            let mut track = original.clone();
            match action {
                ContentsAnimationAction::Enable | ContentsAnimationAction::AddKey => {
                    track.keys.insert(
                        frame,
                        Keyframe {
                            value,
                            interpolation: Interpolation::Linear,
                            temporal: TemporalHandles::default(),
                        },
                    );
                }
                ContentsAnimationAction::Disable => {
                    track.value = value;
                    track.keys.clear();
                }
                ContentsAnimationAction::RemoveKey => {
                    track.keys.remove(&frame);
                    if track.keys.is_empty() {
                        track.value = value;
                    }
                }
            }
            updates.push((node.id, track));
        }
        for (item, track) in updates {
            self.node_mut(item)
                .expect("planned sibling remains present")
                .parameters
                .insert(parameter, track);
        }
        Ok(())
    }
}

/// Only dedicated shared numeric edits and recursively nonempty pure batches bypass
/// unrelated migrations. Ordinary Track writes and mixed/empty batches retain
/// their existing acceptance behavior. Validate metadata at the source and final
/// candidate boundaries, not an oversized temporary state inside a pure batch.
pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::Contents {
            edit: ContentsEdit::SetSharedValue { .. } | ContentsEdit::SharedAnimation { .. },
            ..
        } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}
