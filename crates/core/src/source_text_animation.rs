//! Whole-layer discrete UTF-8 source text. Timing values are opaque pool indices.
use super::*;
use std::sync::Arc;

const MAX_STRING_BYTES: usize = 16_384;
const MAX_POOL_BYTES: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceTextAnimation {
    strings: Vec<Arc<str>>,
    pub(super) timing: AnimatedProperty,
}
impl Default for SourceTextAnimation {
    fn default() -> Self {
        Self {
            strings: Vec::new(),
            timing: AnimatedProperty::new(0.0),
        }
    }
}
impl SourceTextAnimation {
    pub fn is_default(&self) -> bool {
        self.strings.is_empty() && self.timing.keys.is_empty() && self.timing.value == 0.0
    }
    pub fn animated(&self) -> bool {
        !self.timing.keys.is_empty()
    }
    fn index(&self, value: f64) -> Option<usize> {
        (value.is_finite() && value >= 0.0 && value.fract() == 0.0)
            .then_some(value as usize)
            .filter(|index| *index < self.strings.len())
    }
    fn at<'a>(&'a self, base: &'a str, frame: Frame) -> &'a str {
        self.timing
            .keys
            .range(..=frame)
            .next_back()
            .or_else(|| self.timing.keys.first_key_value())
            .and_then(|(_, key)| self.index(key.value))
            .map_or(base, |index| self.strings[index].as_ref())
    }
    fn pool_bytes(&self) -> Option<usize> {
        self.strings
            .iter()
            .try_fold(0usize, |total, text| total.checked_add(text.len()))
    }
    fn intern(&mut self, text: &str) -> Result<f64, String> {
        check_text(text)?;
        if let Some(index) = self.strings.iter().position(|s| s.as_ref() == text) {
            return Ok(index as f64);
        }
        // Build occupancy once: a full pool must not scan every key per slot.
        // Timing's fallback index is refreshed after insertion; only keys own
        // live references, and none of their slots may be changed or renumbered.
        let mut live = vec![false; self.strings.len()];
        for key in self.timing.keys.values() {
            if let Some(index) = self.index(key.value) {
                live[index] = true;
            }
        }
        let free = self
            .strings
            .iter()
            .enumerate()
            .filter(|(index, _)| !live[*index])
            .max_by_key(|(index, text)| (text.len(), std::cmp::Reverse(*index)))
            .map(|(index, _)| index);
        if free.is_none() && self.strings.len() >= MAX_ENTRIES {
            return Err(
                "Source Text supports at most 10000 strings and a 1 MiB string pool".into(),
            );
        }
        let previous_bytes = free.map_or(0, |index| self.strings[index].len());
        let mut bytes = self
            .pool_bytes()
            .and_then(|total| total.checked_sub(previous_bytes))
            .and_then(|total| total.checked_add(text.len()))
            .ok_or("Source Text pool size overflow")?;
        // Dead strings can fragment an otherwise usable byte budget. Plan the
        // minimal deterministic prefix of additional dead slots to empty, then
        // commit only after proving admission. Failed edits retain exact storage.
        let mut reclaim = Vec::new();
        if bytes > MAX_POOL_BYTES {
            for (index, old) in self.strings.iter().enumerate() {
                if !live[index] && Some(index) != free && !old.is_empty() {
                    bytes -= old.len();
                    reclaim.push(index);
                    if bytes <= MAX_POOL_BYTES {
                        break;
                    }
                }
            }
            if bytes > MAX_POOL_BYTES {
                return Err(
                    "Source Text supports at most 10000 strings and a 1 MiB string pool".into(),
                );
            }
        }
        for index in reclaim {
            self.strings[index] = Arc::from("");
        }
        if let Some(index) = free {
            self.strings[index] = Arc::from(text);
            Ok(index as f64)
        } else {
            self.strings.push(Arc::from(text));
            Ok((self.strings.len() - 1) as f64)
        }
    }
    fn add_key(&mut self, frame: Frame, text: &str) -> Result<(), String> {
        if !self.timing.keys.contains_key(&frame) && self.timing.keys.len() >= MAX_ENTRIES {
            return Err("Source Text supports at most 10000 keyframes".into());
        }
        // Replacing the sole reference to a string may reuse its slot even at
        // the pool cap. Other frames stay live and keep their exact indices.
        let previous = self.timing.keys.remove(&frame);
        let value = match self.intern(text) {
            Ok(value) => value,
            Err(error) => {
                if let Some(previous) = previous {
                    self.timing.keys.insert(frame, previous);
                }
                return Err(error);
            }
        };
        self.timing.keys.insert(
            frame,
            Keyframe {
                value,
                interpolation: Interpolation::Hold,
                temporal: TemporalHandles::default(),
            },
        );
        self.refresh_fallback();
        Ok(())
    }
    pub(super) fn refresh_fallback(&mut self) {
        if let Some((_, key)) = self.timing.keys.first_key_value() {
            self.timing.value = key.value;
        }
    }
}
fn check_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_STRING_BYTES {
        return Err("Source Text must be at most 16384 UTF-8 bytes".into());
    }
    Ok(())
}
impl Layer {
    /// Shared discrete sample for editing, layout and rendering, including before
    /// the first key. Static content remains the independent unanimated baseline.
    pub fn source_text_at(&self, frame: Frame) -> Option<&str> {
        let Content::Text { text, .. } = &self.content else {
            return None;
        };
        Some(self.source_text_animation.at(text, frame))
    }
    pub fn source_text_animation(&self) -> &SourceTextAnimation {
        &self.source_text_animation
    }
    pub(super) fn bake_source_text(&mut self, text: String) -> Result<(), String> {
        rich_text::replace_source(self, &text)?;
        let Content::Text { text: base, .. } = &mut self.content else {
            return Err("Source Text requires a text layer".into());
        };
        *base = text;
        self.source_text_animation = SourceTextAnimation::default();
        Ok(())
    }
    pub(super) fn paste_source_text(&mut self, frame: Frame, text: &str) -> Result<(), String> {
        if !matches!(self.content, Content::Text { .. }) {
            return Err("Source Text requires a text layer".into());
        }
        self.source_text_animation.add_key(frame, text)
    }
}

pub(super) fn validate(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    let animation = &layer.source_text_animation;
    if animation.is_default() {
        return Ok(());
    }
    // Keyless nondefault storage is deliberately invalid: off/delete always bake
    // the selected deterministic fallback and remove both timing and the pool.
    if version < 53
        || !matches!(layer.content, Content::Text { .. })
        || animation.strings.is_empty()
        || !animation.animated()
        || animation.strings.len() > MAX_ENTRIES
        || animation
            .strings
            .iter()
            .any(|text| check_text(text).is_err())
        || animation
            .pool_bytes()
            .is_none_or(|bytes| bytes > MAX_POOL_BYTES)
        || animation.index(animation.timing.value).is_none()
        || animation.timing.keys.len() > MAX_ENTRIES
        || animation.timing.keys.iter().any(|(frame, key)| {
            *frame >= duration
                || animation.index(key.value).is_none()
                || key.interpolation != Interpolation::Hold
                || !key.temporal.is_empty()
        })
    {
        return Err(
            "Invalid Source Text animation or unsupported project version (requires 53)".into(),
        );
    }
    Ok(())
}

pub(super) fn value_edits_only(command: &Command) -> bool {
    match command {
        Command::EditSourceText { .. } => true,
        Command::Batch(commands) => {
            !commands.is_empty()
                && commands.iter().all(|command| {
                    value_edits_only(command) || text_animation::value_edits_only(command)
                })
        }
        _ => false,
    }
}

pub(super) fn validate_copy(key: &KeyCopy) -> Result<(), String> {
    if key.key.property == PropertyPath::SourceText {
        let text = key
            .source_text
            .as_deref()
            .ok_or("Missing Source Text clipboard payload")?;
        check_text(text)?;
        if key.path_pose.is_some()
            || key.effect_kind.is_some()
            || !key.data.value.is_finite()
            || key.data.value < 0.0
            || key.data.value.fract() != 0.0
            || key.data.value >= MAX_ENTRIES as f64
            || key.data.interpolation != Interpolation::Hold
            || !key.data.temporal.is_empty()
        {
            return Err("Invalid Source Text clipboard metadata".into());
        }
    } else if key.source_text.is_some() {
        return Err("Source Text clipboard payload requires a Source Text target".into());
    }
    Ok(())
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let (id, frame) = match command {
        Command::EditSourceText { id, frame, .. } => (*id, *frame),
        Command::EditTrack {
            id,
            property: PropertyPath::SourceText,
            edit,
        } => {
            let frame = match edit {
                TrackEdit::ToggleKey { frame } | TrackEdit::ToggleAnimation { frame }
                | TrackEdit::Interpolate { frame, .. } => *frame,
                _ => return Some(Err("Source Text supports Hold keys and time-only edits; edit its string in the text controls".into())),
            };
            (*id, frame)
        }
        _ => return None,
    };
    Some((|| {
        if frame >= state.project.composition.duration {
            return Err("Source Text key is outside the composition".into());
        }
        let layer = editing::editable(state, id)?;
        let sample = layer
            .source_text_at(frame)
            .ok_or("Select a text layer")?
            .to_owned();
        if let Command::EditSourceText { text, .. } = command {
            check_text(text)?;
            if text == &sample {
                return Ok(());
            }
            if layer.source_text_animation.animated() {
                layer.source_text_animation.add_key(frame, text)?;
            } else {
                layer.bake_source_text(text.clone())?;
            }
            return Ok(());
        }
        let Command::EditTrack { edit, .. } = command else {
            unreachable!()
        };
        match edit {
            TrackEdit::ToggleAnimation { .. } if layer.source_text_animation.animated() => {
                layer.bake_source_text(sample)?;
            }
            TrackEdit::ToggleKey { .. }
                if layer.source_text_animation.timing.keys.contains_key(&frame) =>
            {
                layer.source_text_animation.timing.keys.remove(&frame);
                if layer.source_text_animation.animated() {
                    layer.source_text_animation.refresh_fallback();
                } else {
                    layer.bake_source_text(sample)?;
                }
            }
            TrackEdit::ToggleKey { .. } | TrackEdit::ToggleAnimation { .. } => {
                layer.source_text_animation.add_key(frame, &sample)?;
            }
            TrackEdit::Interpolate { interpolation, .. } => {
                if *interpolation != Interpolation::Hold {
                    return Err("Source Text supports Hold interpolation only".into());
                }
                if !layer.source_text_animation.timing.keys.contains_key(&frame) {
                    return Err("Add a Source Text keyframe first".into());
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    })())
}
