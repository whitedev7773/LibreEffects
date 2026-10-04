//! Sparse whole-layer text paint and typography tracks with static source fallbacks.
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
    FontSize,
    Tracking,
    Leading,
}
impl TextParam {
    pub const ALL: [Self; 10] = [
        Self::FillRed,
        Self::FillGreen,
        Self::FillBlue,
        Self::StrokeRed,
        Self::StrokeGreen,
        Self::StrokeBlue,
        Self::StrokeWidth,
        Self::FontSize,
        Self::Tracking,
        Self::Leading,
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
            Self::FontSize => "Font Size",
            Self::Tracking => "Tracking",
            Self::Leading => "Leading",
        }
    }
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::FontSize => (1., 2048.),
            Self::Tracking => (-1000., 10000.),
            Self::Leading => (0.1, 10.),
            Self::StrokeWidth => (0., 1000.),
            _ => (0., 255.),
        }
    }
    pub(super) fn is_typography(self) -> bool {
        matches!(self, Self::FontSize | Self::Tracking | Self::Leading)
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
            Self::FontSize => match layer.content {
                Content::Text { font_size, .. } => font_size,
                _ => unreachable!("Text parameter base requires text content"),
            },
            Self::Tracking => layer.text_style.tracking,
            Self::Leading => layer.text_style.leading,
        }
    }
}

/// Evaluated layout values. Font size lives separately in `Content::Text`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextTypography {
    pub font_size: f64,
    pub tracking: f64,
    pub leading: f64,
}
impl TextTypography {
    /// Apply spacing to a temporary layout style without changing its identity,
    /// paragraph settings or paint. Never use the result as an editing baseline.
    pub fn apply_to_style(&self, style: &mut TextStyle) {
        style.tracking = self.tracking;
        style.leading = self.leading;
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
    /// Evaluate a text scalar; callers must retain the static source/style for edits.
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
    pub fn text_typography_at(&self, frame: Frame) -> Option<TextTypography> {
        Some(TextTypography {
            font_size: self.text_value_at(TextParam::FontSize, frame)?,
            tracking: self.text_value_at(TextParam::Tracking, frame)?,
            leading: self.text_value_at(TextParam::Leading, frame)?,
        })
    }
    /// Plan a scalar edit without materializing an untouched typography track.
    /// Existing tracks keep their source baseline, and explicit track/key commands
    /// remain available when the caller intends to enable animation.
    ///
    /// The caller must supply a current, valid composition frame: a layer alone
    /// does not know its composition duration. Static baseline commands are
    /// frame-independent; emitted track edits validate the frame when executed.
    pub fn text_value_command(
        &self,
        parameter: TextParam,
        value: f64,
        frame: Frame,
    ) -> Result<Option<Command>, String> {
        let sampled = self
            .text_value_at(parameter, frame)
            .ok_or("Select a text layer")?;
        if self.locked {
            return Err("Unlock the layer before changing its text".into());
        }
        if !parameter.accepts(value) {
            return Err("Invalid text property value".into());
        }
        if value == sampled {
            return Ok(None);
        }
        let command = if parameter.is_typography() && !self.text_parameters.contains_key(&parameter)
        {
            match parameter {
                TextParam::FontSize => {
                    let mut content = self.content.clone();
                    let Content::Text { font_size, .. } = &mut content else {
                        unreachable!();
                    };
                    *font_size = value;
                    Command::SetContent {
                        id: self.id,
                        content,
                    }
                }
                TextParam::Tracking | TextParam::Leading => {
                    let mut style = self.text_style.clone();
                    if parameter == TextParam::Tracking {
                        style.tracking = value;
                    } else {
                        style.leading = value;
                    }
                    Command::SetTextStyle { id: self.id, style }
                }
                _ => unreachable!(),
            }
        } else {
            Command::EditText {
                id: self.id,
                parameter,
                edit: TrackEdit::Value { frame, value },
            }
        };
        Ok(Some(command))
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
        return Err("Text tracks require a text layer and project version 48 or later".into());
    }
    for (&parameter, track) in &layer.text_parameters {
        if parameter.is_typography() && version < 49 {
            return Err("Text typography tracks require project version 49".into());
        }
        if !parameter.accepts(track.value)
            || track.keys.len() > 10_000
            || track.keys.iter().any(|(frame, key)| {
                *frame >= duration || !parameter.accepts(key.value) || !key.interpolation.valid()
            })
        {
            return Err("Invalid text property or keyframe".into());
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
