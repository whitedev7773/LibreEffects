//! Source seconds are independent of composition frames and of transform/effect time.
use super::*;
#[cfg(test)]
#[path = "time_remap_tests.rs"]
mod tests;

// Includes every source time reachable by the existing source-in/speed/origin limits.
const MAX_SECONDS: f64 = 20_000_000_000.0;
fn accepts(value: f64) -> bool {
    value.is_finite() && value.abs() <= MAX_SECONDS
}

impl Layer {
    pub fn can_time_remap(&self) -> bool {
        matches!(
            self.content,
            Content::Video { .. }
                | Content::Audio { .. }
                | Content::ImageSequence { .. }
                | Content::Composition { .. }
        )
    }
    pub fn time_remap(&self) -> Option<&AnimatedProperty> {
        self.time_remap.as_ref()
    }
    /// Unquantized source seconds. Values outside the source remain editable.
    pub fn source_time(&self, frame: Frame, fps: impl Into<FrameRate>) -> Option<f64> {
        let fps = fps.into();
        if !self.can_time_remap() || !fps.valid() {
            return None;
        }
        if let Some(track) = &self.time_remap {
            return Some(track.value_at(frame));
        }
        match self.content {
            Content::Composition { start_frame, .. } => {
                Some((f64::from(frame) - start_frame as f64) / fps.as_f64())
            }
            _ => self.content.video_source_time(frame, fps),
        }
    }
    pub fn video_time(&self, frame: Frame, fps: impl Into<FrameRate>) -> Option<f64> {
        let (duration, source_fps) = self.content.footage_timing()?;
        let seconds = nonnegative(self.source_time(frame, fps)?);
        let (duration, source_fps) = match self.footage_interpretation.fps {
            Some(fps) => (duration * source_fps / fps.as_f64(), fps.as_f64()),
            None => (duration, source_fps),
        };
        (seconds >= 0.0 && seconds < duration)
            .then(|| ((seconds * source_fps + 1e-7).floor() / source_fps).max(0.0))
    }
    pub fn composition_frame(
        &self,
        frame: Frame,
        parent_fps: impl Into<FrameRate>,
        source: &Composition,
    ) -> Option<Frame> {
        if self.time_remap.is_none() {
            // Preserve the exact integer conversion of all existing projects.
            return self.content.composition_frame(frame, parent_fps, source);
        }
        if !matches!(self.content, Content::Composition { .. }) {
            return None;
        }
        let seconds = nonnegative(self.source_time(frame, parent_fps)?);
        let value = seconds * source.fps.as_f64();
        if !value.is_finite() || value < 0.0 || value >= f64::from(source.duration) {
            return None;
        }
        let frame = (value + 1e-7).floor() as Frame;
        (frame < source.duration).then_some(frame)
    }
}
fn nonnegative(seconds: f64) -> f64 {
    if (-1e-9..0.0).contains(&seconds) {
        0.0
    } else {
        seconds
    }
}

pub(super) fn validate(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    let Some(track) = &layer.time_remap else {
        return Ok(());
    };
    if version < 21 {
        return Err("Time Remap requires project version 21".into());
    }
    if !layer.can_time_remap()
        || !accepts(track.value)
        || track.keys.len() > 10000
        || track.keys.iter().any(|(frame, key)| {
            *frame >= duration || !accepts(key.value) || !key.interpolation.valid()
        })
    {
        return Err("Invalid Time Remap source or animation".into());
    }
    Ok(())
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let id = match command {
        Command::SetTimeRemap { id, .. }
        | Command::FreezeTimeRemap { id, .. }
        | Command::EditTimeRemap { id, .. } => *id,
        _ => return None,
    };
    Some((|| {
        let comp = &state.project.composition;
        let (fps, duration) = (comp.fps, comp.duration);
        let layer = comp.layer(id).ok_or("Layer not found")?;
        if layer.locked || !layer.can_time_remap() {
            return Err("Select an unlocked footage or precomposition layer".into());
        }
        match command {
            Command::SetTimeRemap { enabled, .. } => {
                if *enabled == layer.time_remap.is_some() {
                    return Ok(());
                }
                let track = if *enabled {
                    let first = layer.in_frame;
                    let last = layer.out_frame(duration) - 1;
                    let mut track = AnimatedProperty::new(layer.source_time(first, fps).unwrap());
                    // Use the first/last displayed frames so enabling preserves
                    // trimming, shifted origins and existing speed/reverse/freeze.
                    for frame in [first, last] {
                        track.keys.insert(
                            frame,
                            Keyframe {
                                temporal: TemporalHandles::default(),
                                value: layer.source_time(frame, fps).unwrap(),
                                interpolation: Interpolation::Linear,
                            },
                        );
                    }
                    Some(track)
                } else {
                    None
                };
                editing::editable(state, id)?.time_remap = track;
            }
            Command::FreezeTimeRemap { frame, .. } => {
                if *frame < layer.in_frame || *frame >= layer.out_frame(duration) {
                    return Err("Place the playhead inside the layer to freeze".into());
                }
                let seconds = match layer.content {
                    Content::Video { .. }
                    | Content::Audio { .. }
                    | Content::ImageSequence { .. } => layer.video_time(*frame, fps),
                    Content::Composition { composition, .. } => {
                        let source = state
                            .project
                            .composition_by_id(composition)
                            .ok_or("Missing source composition")?;
                        layer
                            .composition_frame(*frame, fps, source)
                            .map(|f| f64::from(f) / source.fps.as_f64())
                    }
                    _ => None,
                }
                .ok_or("There is no source frame at the playhead")?;
                let mut track = AnimatedProperty::new(seconds);
                track.keys.insert(
                    *frame,
                    Keyframe {
                        temporal: TemporalHandles::default(),
                        value: seconds,
                        interpolation: Interpolation::Hold,
                    },
                );
                editing::editable(state, id)?.time_remap = Some(track);
            }
            Command::EditTimeRemap { edit, .. } => edit_track(
                editing::editable(state, id)?
                    .time_remap
                    .as_mut()
                    .ok_or("Enable Time Remap first")?,
                duration,
                edit,
                accepts,
            )?,
            _ => unreachable!(),
        }
        Ok(())
    })())
}

pub(super) fn edit_track(
    track: &mut AnimatedProperty,
    duration: Frame,
    edit: &TrackEdit,
    accepts: impl Fn(f64) -> bool,
) -> Result<(), String> {
    let frame = match *edit {
        TrackEdit::Value { frame, .. }
        | TrackEdit::ToggleKey { frame }
        | TrackEdit::ToggleAnimation { frame }
        | TrackEdit::Interpolate { frame, .. } => frame,
        TrackEdit::Keyframe { to, .. } => to,
    };
    if frame >= duration {
        return Err("Key is outside the composition".into());
    }
    match *edit {
        TrackEdit::Value { value, .. } => {
            if !accepts(value) {
                return Err("Invalid animated property value".into());
            }
            if track.keys.is_empty() {
                track.value = value;
            } else {
                let interpolation = track
                    .keys
                    .get(&frame)
                    .map_or(Interpolation::Linear, |k| k.interpolation);
                track.keys.insert(
                    frame,
                    Keyframe {
                        temporal: track
                            .keys
                            .get(&frame)
                            .map_or(TemporalHandles::default(), |k| k.temporal),
                        value,
                        interpolation,
                    },
                );
            }
        }
        TrackEdit::Keyframe { from, value, .. } => {
            if !accepts(value) {
                return Err("Invalid animated property value".into());
            }
            if from != frame && track.keys.contains_key(&frame) {
                return Err("Destination already contains a key".into());
            }
            let mut key = track
                .keys
                .remove(&from)
                .ok_or("Selected key no longer exists")?;
            key.value = value;
            track.keys.insert(frame, key);
        }
        TrackEdit::ToggleKey { .. } => {
            let value = track.value_at(frame);
            if track.keys.remove(&frame).is_none() {
                track.keys.insert(
                    frame,
                    Keyframe {
                        temporal: TemporalHandles::default(),
                        value,
                        interpolation: Interpolation::Linear,
                    },
                );
            } else if track.keys.is_empty() {
                track.value = value;
            }
        }
        TrackEdit::ToggleAnimation { .. } => {
            if track.keys.is_empty() {
                track.keys.insert(
                    frame,
                    Keyframe {
                        temporal: TemporalHandles::default(),
                        value: track.value,
                        interpolation: Interpolation::Linear,
                    },
                );
            } else {
                track.value = track.value_at(frame);
                track.keys.clear();
            }
        }
        TrackEdit::Interpolate { interpolation, .. } => {
            if !interpolation.valid() {
                return Err("Invalid interpolation".into());
            }
            track.set_interpolation(frame, interpolation)?;
        }
    }
    Ok(())
}
