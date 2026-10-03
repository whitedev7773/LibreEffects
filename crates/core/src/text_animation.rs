//! Sparse whole-layer text paint tracks. Typography always remains in TextStyle.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TextParam {
    FillRed,
    FillGreen,
    FillBlue,
    StrokeRed,
    StrokeGreen,
    StrokeBlue,
    StrokeWidth,
}
impl TextParam {
    pub const ALL: [Self; 7] = [
        Self::FillRed,
        Self::FillGreen,
        Self::FillBlue,
        Self::StrokeRed,
        Self::StrokeGreen,
        Self::StrokeBlue,
        Self::StrokeWidth,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::FillRed => "Fill Color · Red",
            Self::FillGreen => "Fill Color · Green",
            Self::FillBlue => "Fill Color · Blue",
            Self::StrokeRed => "Stroke Color · Red",
            Self::StrokeGreen => "Stroke Color · Green",
            Self::StrokeBlue => "Stroke Color · Blue",
            Self::StrokeWidth => "Stroke Width",
        }
    }
    pub fn bounds(self) -> (f64, f64) {
        (
            0.,
            if self == Self::StrokeWidth {
                1000.
            } else {
                255.
            },
        )
    }
    fn accepts(self, value: f64) -> bool {
        value.is_finite() && (self.bounds().0..=self.bounds().1).contains(&value)
    }
    fn base(self, layer: &Layer) -> f64 {
        match self {
            Self::FillRed => ((layer.color >> 16) & 255) as f64,
            Self::FillGreen => ((layer.color >> 8) & 255) as f64,
            Self::FillBlue => (layer.color & 255) as f64,
            Self::StrokeRed => ((layer.text_style.stroke_color >> 16) & 255) as f64,
            Self::StrokeGreen => ((layer.text_style.stroke_color >> 8) & 255) as f64,
            Self::StrokeBlue => (layer.text_style.stroke_color & 255) as f64,
            Self::StrokeWidth => layer.text_style.stroke_width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextPaint {
    Fill,
    Stroke,
}
impl TextPaint {
    pub fn from_parameter(parameter: TextParam) -> Option<Self> {
        [Self::Fill, Self::Stroke]
            .into_iter()
            .find(|paint| paint.channels().contains(&parameter))
    }
    pub fn component_label(parameter: TextParam) -> Option<&'static str> {
        Self::from_parameter(parameter)?
            .channels()
            .iter()
            .position(|p| *p == parameter)
            .map(|i| ["R", "G", "B"][i])
    }
    pub fn channels(self) -> [TextParam; 3] {
        use TextParam::*;
        match self {
            Self::Fill => [FillRed, FillGreen, FillBlue],
            Self::Stroke => [StrokeRed, StrokeGreen, StrokeBlue],
        }
    }
}

impl Layer {
    /// Evaluate paint only; callers must keep the base typography/style for edits.
    pub fn text_value_at(&self, parameter: TextParam, frame: Frame) -> Option<f64> {
        matches!(self.content, Content::Text { .. }).then(|| {
            self.text_parameters.get(&parameter).map_or_else(
                || parameter.base(self),
                |track| {
                    track
                        .value_at(frame)
                        .clamp(parameter.bounds().0, parameter.bounds().1)
                },
            )
        })
    }
    pub fn text_color_at(&self, paint: TextPaint, frame: Frame) -> Option<u32> {
        paint.channels().into_iter().try_fold(0, |rgb, p| {
            Some((rgb << 8) | self.text_value_at(p, frame)?.round() as u32)
        })
    }
    pub fn text_color_animated(&self, paint: TextPaint) -> bool {
        matches!(self.content, Content::Text { .. })
            && paint.channels().iter().any(|p| {
                self.text_parameters
                    .get(p)
                    .is_some_and(|track| !track.keys.is_empty())
            })
    }
    pub(super) fn text_track_mut(&mut self, parameter: TextParam) -> Option<&mut AnimatedProperty> {
        if !matches!(self.content, Content::Text { .. }) {
            return None;
        }
        let base = parameter.base(self);
        Some(
            self.text_parameters
                .entry(parameter)
                .or_insert_with(|| AnimatedProperty::new(base)),
        )
    }
    /// Build a single-Undo RGB edit. Callers suppress unchanged/cancelled dialogs
    /// and reject stale drafts before executing, as with other color commands.
    pub fn text_color_command(
        &self,
        paint: TextPaint,
        color: u32,
        frame: Frame,
    ) -> Result<Command, String> {
        if !matches!(self.content, Content::Text { .. }) {
            return Err("Select a text layer".into());
        }
        if self.locked || color > 0xffffff {
            return Err("Invalid color or locked text layer".into());
        }
        let channels = paint.channels();
        if !channels
            .iter()
            .any(|p| self.text_parameters.contains_key(p))
        {
            return Ok(match paint {
                TextPaint::Fill => Command::SetColor { id: self.id, color },
                TextPaint::Stroke => {
                    let mut style = self.text_style.clone();
                    style.stroke_color = color;
                    Command::SetTextStyle { id: self.id, style }
                }
            });
        }
        let first_key = channels
            .iter()
            .filter_map(|p| self.text_parameters.get(p)?.keys.keys().next().copied())
            .min();
        let mut commands = Vec::new();
        for (i, parameter) in channels.into_iter().enumerate() {
            // A single-channel pasted key must preserve the other channels at
            // the first existing key before inserting a coherent RGB edit.
            if let Some(first) = first_key {
                if self
                    .text_parameters
                    .get(&parameter)
                    .is_none_or(|t| t.keys.is_empty())
                {
                    commands.push(Command::EditText {
                        id: self.id,
                        parameter,
                        edit: TrackEdit::ToggleAnimation { frame: first },
                    });
                }
            }
            commands.push(Command::EditText {
                id: self.id,
                parameter,
                edit: TrackEdit::Value {
                    frame,
                    value: ((color >> (16 - 8 * i)) & 255) as f64,
                },
            });
        }
        Ok(Command::Batch(commands))
    }
    pub fn text_color_animation_command(
        &self,
        paint: TextPaint,
        frame: Frame,
    ) -> Result<Command, String> {
        if !matches!(self.content, Content::Text { .. }) {
            return Err("Select a text layer".into());
        }
        if self.locked {
            return Err("Unlock the layer before changing its color".into());
        }
        let disabling = self.text_color_animated(paint);
        Ok(Command::Batch(
            paint
                .channels()
                .into_iter()
                .filter(|p| {
                    !disabling
                        || self
                            .text_parameters
                            .get(p)
                            .is_some_and(|t| !t.keys.is_empty())
                })
                .map(|parameter| Command::EditText {
                    id: self.id,
                    parameter,
                    edit: TrackEdit::ToggleAnimation { frame },
                })
                .collect(),
        ))
    }
}

pub(super) fn validate(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    if layer.text_parameters.is_empty() {
        return Ok(());
    }
    if version < 48 || !matches!(layer.content, Content::Text { .. }) {
        return Err("Text paint tracks require a text layer and project version 48".into());
    }
    for (&parameter, track) in &layer.text_parameters {
        if !parameter.accepts(track.value)
            || track.keys.len() > 10_000
            || track.keys.iter().any(|(frame, key)| {
                *frame >= duration || !parameter.accepts(key.value) || !key.interpolation.valid()
            })
        {
            return Err("Invalid text paint property or keyframe".into());
        }
    }
    Ok(())
}

// A text-value no-op must not trigger unrelated historical schema migrations.
// Keep this scoped to text; mixed/empty batches retain ordinary command behavior.
pub(super) fn value_edits_only(command: &Command) -> bool {
    match command {
        Command::EditText {
            edit: TrackEdit::Value { .. },
            ..
        }
        | Command::EditTrack {
            property: PropertyPath::Text(_),
            edit: TrackEdit::Value { .. },
            ..
        } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(value_edits_only),
        _ => false,
    }
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::EditText {
        id,
        parameter,
        edit,
    } = command
    else {
        return None;
    };
    Some((|| {
        let duration = state.project.composition.duration;
        let layer = editing::editable(state, *id)?;
        let base = layer
            .text_value_at(*parameter, 0)
            .ok_or("Select a text layer")?;
        if let TrackEdit::Value { frame, value } = edit {
            if *frame >= duration {
                return Err("Key is outside the composition".into());
            }
            if !layer.text_parameters.contains_key(parameter) && *value == base {
                return Ok(());
            }
        }
        let track = layer
            .text_track_mut(*parameter)
            .ok_or("Select a text layer")?;
        time_remap::edit_track(track, duration, edit, |value| parameter.accepts(value))?;
        // Curves may overshoot; disabling or sampling retains visible bounds.
        if let TrackEdit::ToggleAnimation { frame } | TrackEdit::ToggleKey { frame } = edit {
            let (min, max) = parameter.bounds();
            track.value = track.value.clamp(min, max);
            if let Some(key) = track.keys.get_mut(frame) {
                key.value = key.value.clamp(min, max);
            }
        }
        Ok(())
    })())
}
