//! Affine scaling of selected scalar endpoint velocities without moving keys.
use super::*;

/// Signed endpoint velocity transform, in property units per frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyVelocityScale {
    pub origin: f64,
    pub factor: f64,
}
impl KeyVelocityScale {
    fn validate(self) -> Result<(), String> {
        if !self.origin.is_finite() || !self.factor.is_finite() {
            return Err("Velocity scaling requires a finite origin and factor".into());
        }
        Ok(())
    }
    fn handle(self, handle: TemporalHandle) -> Result<TemporalHandle, String> {
        // Preserve exact identities, including a large origin or factor. A zero
        // factor does not need a potentially overflowing intermediate product.
        let slope = if self.factor == 1.0 || handle.slope == self.origin {
            handle.slope
        } else if self.factor == 0.0 {
            self.origin
        } else if (0.5..=1.5).contains(&self.factor) {
            // Around identity, adding a small delta avoids cancellation between
            // a large origin and its nearly equal scaled displacement.
            (self.factor - 1.0).mul_add(handle.slope - self.origin, handle.slope)
        } else {
            self.factor.mul_add(handle.slope - self.origin, self.origin)
        };
        let transformed = TemporalHandle { slope, ..handle };
        if !transformed.valid() {
            return Err("Scaled key velocity is outside the supported range".into());
        }
        Ok(transformed)
    }
}

// These edits need only the existing temporal schema minimums. In particular,
// even nested no-op batches must not migrate assets or downgrade imported schemas.
pub(super) fn edits_only(command: &Command) -> bool {
    fn classify(command: &Command) -> Option<bool> {
        match command {
            Command::ScaleKeyVelocities { .. } => Some(true),
            Command::Batch(commands) => {
                let mut has_scale = false;
                for command in commands {
                    has_scale |= classify(command)?;
                }
                Some(has_scale)
            }
            _ => None,
        }
    }
    // Empty nested batches are harmless within a velocity edit; a wholly empty
    // batch still follows the editor's pre-existing migration behavior.
    classify(command) == Some(true)
}

pub(super) fn apply(
    state: &mut Snapshot,
    keys: &[KeyRef],
    scale: KeyVelocityScale,
) -> Result<(), String> {
    scale.validate()?;
    if keys.is_empty() {
        return Err("Select keyframes to scale velocities".into());
    }
    let mut tracks = BTreeMap::<_, Vec<_>>::new();
    for key in keys {
        if matches!(key.property, PropertyPath::Path(_)) {
            return Err("Geometry path keys do not support scalar velocity scaling".into());
        }
        tracks
            .entry((key.id, key.property))
            .or_default()
            .push(key.frame);
    }
    let mut version = state.project.version;
    for ((id, property), frames) in tracks {
        let track = editing::editable(state, id)?.track_mut(property)?;
        let preview = track.preview_key_velocity_scale(&frames, scale)?;
        if preview != *track {
            version = version.max(
                if preview
                    .keys
                    .values()
                    .any(|k| !k.temporal.mode.is_independent())
                {
                    36
                } else {
                    35
                },
            );
            *track = preview;
        }
    }
    state.project.version = version;
    Ok(())
}

impl AnimatedProperty {
    /// Return the incoming and outgoing handles for actual neighboring segments.
    /// Missing endpoint sides are None, even when dormant metadata is stored.
    /// Hold, singular, and nonrepresentable endpoint handles reject the selection.
    pub fn key_velocity_handles(
        &self,
        frame: Frame,
    ) -> Result<[Option<TemporalHandle>; 2], String> {
        let key = self
            .keys
            .get(&frame)
            .ok_or("Selected key no longer exists")?;
        if !key.temporal.valid() {
            return Err("Invalid scalar temporal handles".into());
        }
        let previous = self.keys.range(..frame).next_back();
        let next = self
            .keys
            .range((std::ops::Bound::Excluded(frame), std::ops::Bound::Unbounded))
            .next();
        let resolved = self.resolved_key(frame).temporal;
        let stored = [resolved.incoming, resolved.outgoing];
        let segments = [
            previous.map(|(&f, k)| (f, k, frame, key)),
            next.map(|(&f, k)| (frame, key, f, k)),
        ];
        let mut handles = [None, None];
        for (index, segment) in segments.into_iter().enumerate() {
            let Some((start, a, end, b)) = segment else {
                continue;
            };
            // Stored metadata can exist on Hold or absent segments. Check the
            // actual segment before resolving any stored or automatic handle.
            if a.interpolation == Interpolation::Hold {
                return Err("Hold segments do not support endpoint velocity scaling".into());
            }
            if !a.interpolation.valid() || !a.value.is_finite() || !b.value.is_finite() {
                return Err("Invalid scalar segment".into());
            }
            let handle = if let Some(handle) = stored[index] {
                handle
            } else {
                // Derive legacy slopes in normalized coordinates. Subtracting
                // rounded absolute control values can turn an exact secant or
                // zero Smooth endpoint into a tiny unintended edit.
                let secant = (b.value - a.value) / (end - start) as f64;
                let curve = match a.interpolation {
                    Interpolation::Bezier(curve) => curve,
                    _ => Bezier::default(),
                };
                let influence = if index == 0 { 1.0 - curve.x2 } else { curve.x1 };
                let slope = match a.interpolation {
                    Interpolation::Linear => secant,
                    Interpolation::Smooth => 0.0,
                    Interpolation::Bezier(_) => {
                        let delta = if index == 0 { 1.0 - curve.y2 } else { curve.y1 };
                        secant * (delta / influence)
                    }
                    Interpolation::Hold => unreachable!(),
                };
                TemporalHandle { slope, influence }
            };
            if !handle.valid() {
                return Err(
                    "Endpoint velocity has no finite, representable temporal handle".into(),
                );
            }
            handles[index] = Some(handle);
        }
        Ok(handles)
    }

    /// Detached preview shared with ScaleKeyVelocities. Every selected real side
    /// is validated, including identity edits; source values/times/interpolation
    /// and influences remain fixed. Layer locks are checked by the command.
    pub fn preview_key_velocity_scale(
        &self,
        frames: &[Frame],
        scale: KeyVelocityScale,
    ) -> Result<Self, String> {
        scale.validate()?;
        if frames.is_empty() {
            return Err("Select keyframes to scale velocities".into());
        }
        let mut next = self.clone();
        for frame in frames.iter().copied().collect::<BTreeSet<_>>() {
            // Resolve all sides from the original track, never the partially
            // edited preview, and never through the side-coupled manual setters.
            let original = self.key_velocity_handles(frame)?;
            let transformed = [
                original[0].map(|h| scale.handle(h)).transpose()?,
                original[1].map(|h| scale.handle(h)).transpose()?,
            ];
            if original == transformed {
                continue;
            }
            let key = next.keys.get_mut(&frame).unwrap();
            if key.temporal.mode == TemporalMode::Independent {
                for (index, side) in [&mut key.temporal.incoming, &mut key.temporal.outgoing]
                    .into_iter()
                    .enumerate()
                {
                    if original[index] != transformed[index] {
                        *side = transformed[index];
                    }
                }
            } else {
                // Continuous and Auto share one slope, including dormant endpoint
                // metadata. Freeze Auto only when an actual endpoint changes.
                let mut resolved = self.resolved_key(frame).temporal;
                resolved.mode = TemporalMode::Continuous;
                resolved.incoming = resolved.incoming.map(|h| scale.handle(h)).transpose()?;
                resolved.outgoing = resolved.outgoing.map(|h| scale.handle(h)).transpose()?;
                key.temporal = resolved;
            }
        }
        Ok(next)
    }
}

#[cfg(test)]
#[path = "key_velocity_scale_tests.rs"]
mod tests;
