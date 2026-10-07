//! Sparse text paint, typography and bounded animator tracks with source fallbacks.
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
    FillOpacity,
    StrokeOpacity,
    AnimatorStart,
    AnimatorEnd,
    AnimatorPositionX,
    AnimatorPositionY,
    AnimatorOpacity,
    AnimatorOffset,
    AnimatorAmount,
    AnimatorScaleX,
    AnimatorScaleY,
    AnimatorRotation,
}
impl TextParam {
    pub const ALL: [Self; 22] = [
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
        Self::FillOpacity,
        Self::StrokeOpacity,
        Self::AnimatorStart,
        Self::AnimatorEnd,
        Self::AnimatorPositionX,
        Self::AnimatorPositionY,
        Self::AnimatorOpacity,
        Self::AnimatorOffset,
        Self::AnimatorAmount,
        Self::AnimatorScaleX,
        Self::AnimatorScaleY,
        Self::AnimatorRotation,
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
            Self::FillOpacity => "Fill Opacity",
            Self::StrokeOpacity => "Stroke Opacity",
            Self::AnimatorStart => "Animator · Start",
            Self::AnimatorEnd => "Animator · End",
            Self::AnimatorPositionX => "Animator · Position X",
            Self::AnimatorPositionY => "Animator · Position Y",
            Self::AnimatorOpacity => "Animator · Opacity",
            Self::AnimatorOffset => "Animator · Offset",
            Self::AnimatorAmount => "Animator · Amount",
            Self::AnimatorScaleX => "Animator · Scale X",
            Self::AnimatorScaleY => "Animator · Scale Y",
            Self::AnimatorRotation => "Animator · Rotation",
        }
    }
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::FontSize => (1., 2048.),
            Self::Tracking => (-1000., 10000.),
            Self::Leading => (0.1, 10.),
            Self::StrokeWidth => (0., 1000.),
            Self::FillOpacity
            | Self::StrokeOpacity
            | Self::AnimatorStart
            | Self::AnimatorEnd
            | Self::AnimatorOpacity
            | Self::AnimatorAmount => (0., 100.),
            Self::AnimatorPositionX | Self::AnimatorPositionY => (-1_000_000., 1_000_000.),
            Self::AnimatorOffset => (-100., 100.),
            Self::AnimatorScaleX | Self::AnimatorScaleY => (0., 1000.),
            Self::AnimatorRotation => (-3600., 3600.),
            _ => (0., 255.),
        }
    }
    /// Minimum project schema that can represent this parameter.
    pub const fn required_version(self) -> u32 {
        match self {
            Self::AnimatorScaleX | Self::AnimatorScaleY | Self::AnimatorRotation => 60,
            Self::AnimatorAmount => 59,
            Self::AnimatorOffset => 58,
            Self::AnimatorStart
            | Self::AnimatorEnd
            | Self::AnimatorPositionX
            | Self::AnimatorPositionY
            | Self::AnimatorOpacity => 56,
            Self::FillOpacity | Self::StrokeOpacity => 52,
            Self::FontSize | Self::Tracking | Self::Leading => 49,
            _ => 48,
        }
    }
    /// Parameters of the shared bounded text animator and its pinned primary range.
    pub const fn is_animator(self) -> bool {
        matches!(
            self,
            Self::AnimatorStart
                | Self::AnimatorEnd
                | Self::AnimatorPositionX
                | Self::AnimatorPositionY
                | Self::AnimatorOpacity
                | Self::AnimatorOffset
                | Self::AnimatorAmount
                | Self::AnimatorScaleX
                | Self::AnimatorScaleY
                | Self::AnimatorRotation
        )
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
            Self::FillOpacity
            | Self::StrokeOpacity
            | Self::AnimatorEnd
            | Self::AnimatorOpacity
            | Self::AnimatorAmount
            | Self::AnimatorScaleX
            | Self::AnimatorScaleY => 100.,
            Self::AnimatorStart
            | Self::AnimatorPositionX
            | Self::AnimatorPositionY
            | Self::AnimatorOffset
            | Self::AnimatorRotation => 0.,
        }
    }
}

/// Source units counted by the renderer before visual line wrapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextSelectorUnits {
    #[default]
    Graphemes,
    Words,
    Lines,
}
impl TextSelectorUnits {
    pub const ALL: [Self; 3] = [Self::Graphemes, Self::Words, Self::Lines];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Graphemes => "Characters",
            Self::Words => "Words",
            Self::Lines => "Lines",
        }
    }
}

/// Weight profile inside the selector's effective half-open range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextSelectorShape {
    #[default]
    Square,
    RampUp,
    RampDown,
    Triangle,
}
impl TextSelectorShape {
    pub const ALL: [Self; 4] = [Self::Square, Self::RampUp, Self::RampDown, Self::Triangle];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::RampUp => "Ramp Up",
            Self::RampDown => "Ramp Down",
            Self::Triangle => "Triangle",
        }
    }
}

/// Static unit and shape configuration for a text range selector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextSelector {
    pub units: TextSelectorUnits,
    pub shape: TextSelectorShape,
}
impl TextSelector {
    pub(super) fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Maximum number of additional animators, in addition to the pinned primary.
pub const MAX_TEXT_ANIMATORS: usize = 3;

/// One independent range-selected animator with layer-local stable identity.
/// Only the ten animator TextParam channels are valid in this sparse map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextAnimator {
    pub id: u64,
    #[serde(default, skip_serializing_if = "TextSelector::is_default")]
    pub selector: TextSelector,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<TextParam, AnimatedProperty>,
}
impl TextAnimator {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            selector: TextSelector::default(),
            parameters: BTreeMap::new(),
        }
    }

    /// Evaluate a bounded scalar; non-animator text parameters are unavailable.
    pub fn value_at(&self, parameter: TextParam, frame: Frame) -> Option<f64> {
        let base = animator_base(parameter)?;
        Some(
            self.parameters
                .get(&parameter)
                .map_or(base, |track| track.value_at(frame))
                .clamp(parameter.bounds().0, parameter.bounds().1),
        )
    }

    pub(super) fn track_mut(&mut self, parameter: TextParam) -> Option<&mut AnimatedProperty> {
        let base = animator_base(parameter)?;
        Some(
            self.parameters
                .entry(parameter)
                .or_insert_with(|| AnimatedProperty::new(base)),
        )
    }

    fn sample(&self, frame: Frame) -> TextAnimatorSample {
        let value = |parameter| self.value_at(parameter, frame).expect("Animator channel");
        let offset = value(TextParam::AnimatorOffset);
        TextAnimatorSample {
            start: (value(TextParam::AnimatorStart) + offset).clamp(0., 100.),
            end: (value(TextParam::AnimatorEnd) + offset).clamp(0., 100.),
            position: [
                value(TextParam::AnimatorPositionX),
                value(TextParam::AnimatorPositionY),
            ],
            scale: [
                value(TextParam::AnimatorScaleX),
                value(TextParam::AnimatorScaleY),
            ],
            rotation: value(TextParam::AnimatorRotation),
            opacity: value(TextParam::AnimatorOpacity),
            units: self.selector.units,
            shape: self.selector.shape,
            amount: value(TextParam::AnimatorAmount),
            selectors: Vec::new(),
        }
    }
}

fn animator_base(parameter: TextParam) -> Option<f64> {
    use TextParam::*;
    match parameter {
        AnimatorEnd | AnimatorOpacity | AnimatorAmount | AnimatorScaleX | AnimatorScaleY => {
            Some(100.)
        }
        AnimatorStart | AnimatorPositionX | AnimatorPositionY | AnimatorOffset
        | AnimatorRotation => Some(0.),
        _ => None,
    }
}

/// Maximum number of secondary selectors, in addition to the pinned primary.
pub const MAX_TEXT_RANGE_SELECTORS: usize = 7;

/// Ordered operation on the accumulated per-source-grapheme selector weight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextSelectorMode {
    #[default]
    Add,
    Subtract,
    Intersect,
}
impl TextSelectorMode {
    pub const ALL: [Self; 3] = [Self::Add, Self::Subtract, Self::Intersect];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
        }
    }
}

/// Independently animated scalar channels on a stable secondary selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TextSelectorParam {
    Start,
    End,
    Offset,
    Amount,
}
impl TextSelectorParam {
    pub const ALL: [Self; 4] = [Self::Start, Self::End, Self::Offset, Self::Amount];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::End => "End",
            Self::Offset => "Offset",
            Self::Amount => "Amount",
        }
    }

    pub const fn bounds(self) -> (f64, f64) {
        match self {
            Self::Offset => (-100., 100.),
            _ => (0., 100.),
        }
    }

    fn accepts(self, value: f64) -> bool {
        value.is_finite() && (self.bounds().0..=self.bounds().1).contains(&value)
    }
}

/// A secondary range; identity is stable across edits and reordering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRangeSelector {
    pub id: u64,
    pub mode: TextSelectorMode,
    pub start: f64,
    pub end: f64,
    pub offset: f64,
    pub amount: f64,
    pub selector: TextSelector,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<TextSelectorParam, AnimatedProperty>,
}
impl TextRangeSelector {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            mode: TextSelectorMode::Add,
            start: 0.,
            end: 100.,
            offset: 0.,
            amount: 100.,
            selector: TextSelector::default(),
            parameters: BTreeMap::new(),
        }
    }

    fn valid(&self) -> bool {
        self.id != 0
            && self.id != u64::MAX
            && TextParam::AnimatorStart.accepts(self.start)
            && TextParam::AnimatorEnd.accepts(self.end)
            && TextParam::AnimatorOffset.accepts(self.offset)
            && TextParam::AnimatorAmount.accepts(self.amount)
    }

    fn base(&self, parameter: TextSelectorParam) -> f64 {
        match parameter {
            TextSelectorParam::Start => self.start,
            TextSelectorParam::End => self.end,
            TextSelectorParam::Offset => self.offset,
            TextSelectorParam::Amount => self.amount,
        }
    }

    fn set_base(&mut self, parameter: TextSelectorParam, value: f64) {
        match parameter {
            TextSelectorParam::Start => self.start = value,
            TextSelectorParam::End => self.end = value,
            TextSelectorParam::Offset => self.offset = value,
            TextSelectorParam::Amount => self.amount = value,
        }
    }

    /// Evaluate one channel with a sparse fallback to its authored static value.
    /// Clamp only the visible sample; preserve authored keys and curve handles.
    pub fn value_at(&self, parameter: TextSelectorParam, frame: Frame) -> f64 {
        self.parameters
            .get(&parameter)
            .map_or_else(|| self.base(parameter), |track| track.value_at(frame))
            .clamp(parameter.bounds().0, parameter.bounds().1)
    }

    pub(super) fn track_mut(&mut self, parameter: TextSelectorParam) -> &mut AnimatedProperty {
        let base = self.base(parameter);
        self.parameters
            .entry(parameter)
            .or_insert_with(|| AnimatedProperty::new(base))
    }

    fn sample(&self, frame: Frame) -> TextRangeSelectorSample {
        let offset = self.value_at(TextSelectorParam::Offset, frame);
        TextRangeSelectorSample {
            mode: self.mode,
            start: (self.value_at(TextSelectorParam::Start, frame) + offset).clamp(0., 100.),
            end: (self.value_at(TextSelectorParam::End, frame) + offset).clamp(0., 100.),
            amount: self.value_at(TextSelectorParam::Amount, frame),
            units: self.selector.units,
            shape: self.selector.shape,
        }
    }
}

pub(super) fn first_selector_id() -> u64 {
    1
}
pub(super) fn is_first_selector_id(id: &u64) -> bool {
    *id == first_selector_id()
}

/// Evaluated secondary range. Endpoints include offset and 0–100 clipping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRangeSelectorSample {
    pub mode: TextSelectorMode,
    pub start: f64,
    pub end: f64,
    pub amount: f64,
    pub units: TextSelectorUnits,
    pub shape: TextSelectorShape,
}

/// Evaluated values for one range-selected animator. The renderer owns source
/// unit selection, weighting and cluster mapping; core does not interpret text units.
#[derive(Clone, Debug, PartialEq)]
pub struct TextAnimatorSample {
    /// Effective endpoints after percentage-point Offset and 0–100 clipping.
    pub start: f64,
    pub end: f64,
    pub position: [f64; 2],
    /// Independent nonnegative per-unit scale percentages; zero collapses an axis.
    pub scale: [f64; 2],
    /// Per-unit rotation in degrees, before the layer transform.
    pub rotation: f64,
    pub opacity: f64,
    pub units: TextSelectorUnits,
    pub shape: TextSelectorShape,
    /// Percentage strength applied to the selector's weight, from 0 to 100.
    pub amount: f64,
    /// Secondary ranges, applied in order after the primary above.
    pub selectors: Vec<TextRangeSelectorSample>,
}
impl Default for TextAnimatorSample {
    fn default() -> Self {
        Self {
            start: 0.,
            end: 100.,
            position: [0., 0.],
            scale: [100., 100.],
            rotation: 0.,
            opacity: 100.,
            units: TextSelectorUnits::default(),
            shape: TextSelectorShape::default(),
            amount: 100.,
            selectors: Vec::new(),
        }
    }
}
impl TextAnimatorSample {
    /// Neutral effects or a definitely empty combined selector leave text intact.
    /// An active Add can restore weight even when the primary range is empty.
    pub fn is_identity(&self) -> bool {
        (self.position == [0., 0.]
            && self.scale == [100., 100.]
            && self.rotation == 0.
            && self.opacity == 100.)
            || ((self.amount == 0. || self.start >= self.end)
                && !self.selectors.iter().any(|selector| {
                    selector.mode == TextSelectorMode::Add
                        && selector.amount > 0.
                        && selector.start < selector.end
                }))
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
    /// Independent paint opacity; deliberately excluded from RGB channel helpers.
    pub const fn opacity(self) -> TextParam {
        match self {
            Self::Fill => TextParam::FillOpacity,
            Self::Stroke => TextParam::StrokeOpacity,
        }
    }
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
    pub fn text_selector(&self) -> TextSelector {
        self.text_selector
    }

    /// Additional animators in render order; the primary remains in its legacy fields.
    pub fn text_animators(&self) -> &[TextAnimator] {
        &self.text_animators
    }

    /// Evaluate the pinned primary first, followed by each additional animator.
    pub fn text_animators_at(&self, frame: Frame) -> Option<Vec<TextAnimatorSample>> {
        let primary = self.text_animator_at(frame)?;
        Some(
            std::iter::once(primary)
                .chain(self.text_animators.iter().map(|item| item.sample(frame)))
                .collect(),
        )
    }

    pub fn text_animator_value_command(
        &self,
        animator: u64,
        parameter: TextParam,
        value: f64,
        frame: Frame,
    ) -> Result<Option<Command>, String> {
        if !matches!(self.content, Content::Text { .. }) {
            return Err("Select a text layer".into());
        }
        if self.locked {
            return Err("Unlock the layer before changing its text".into());
        }
        if !parameter.is_animator() || !parameter.accepts(value) {
            return Err("Invalid text animator parameter or value".into());
        }
        let item = self
            .text_animators
            .iter()
            .find(|item| item.id == animator)
            .ok_or("Text animator not found")?;
        if item.value_at(parameter, frame) == Some(value) {
            return Ok(None);
        }
        Ok(Some(Command::EditTrack {
            id: self.id,
            property: PropertyPath::TextAnimator {
                animator,
                parameter,
            },
            edit: TrackEdit::Value { frame, value },
        }))
    }

    pub fn text_range_selectors(&self) -> &[TextRangeSelector] {
        &self.text_range_selectors
    }

    /// Plan a current-frame scalar edit without materializing an unchanged track.
    /// Untracked selectors keep schema-61 static baselines until animation is enabled.
    /// The command validates the composition frame when executed.
    pub fn text_selector_value_command(
        &self,
        selector: u64,
        parameter: TextSelectorParam,
        value: f64,
        frame: Frame,
    ) -> Result<Option<Command>, String> {
        if !matches!(self.content, Content::Text { .. }) {
            return Err("Select a text layer".into());
        }
        if self.locked {
            return Err("Unlock the layer before changing its text".into());
        }
        if !parameter.accepts(value) {
            return Err("Invalid text selector value".into());
        }
        let item = self
            .text_range_selectors
            .iter()
            .find(|item| item.id == selector)
            .ok_or("Text range selector not found")?;
        if item.value_at(parameter, frame) == value {
            return Ok(None);
        }
        Ok(Some(Command::EditTrack {
            id: self.id,
            property: PropertyPath::TextSelector {
                selector,
                parameter,
            },
            edit: TrackEdit::Value { frame, value },
        }))
    }

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
    pub fn text_animator_at(&self, frame: Frame) -> Option<TextAnimatorSample> {
        let offset = self.text_value_at(TextParam::AnimatorOffset, frame)?;
        // Translate the existing hard range without wrapping. Clamp only the
        // evaluated endpoints; keep all three authored tracks independent.
        Some(TextAnimatorSample {
            start: (self.text_value_at(TextParam::AnimatorStart, frame)? + offset).clamp(0., 100.),
            end: (self.text_value_at(TextParam::AnimatorEnd, frame)? + offset).clamp(0., 100.),
            position: [
                self.text_value_at(TextParam::AnimatorPositionX, frame)?,
                self.text_value_at(TextParam::AnimatorPositionY, frame)?,
            ],
            scale: [
                self.text_value_at(TextParam::AnimatorScaleX, frame)?,
                self.text_value_at(TextParam::AnimatorScaleY, frame)?,
            ],
            rotation: self.text_value_at(TextParam::AnimatorRotation, frame)?,
            opacity: self.text_value_at(TextParam::AnimatorOpacity, frame)?,
            units: self.text_selector.units,
            shape: self.text_selector.shape,
            amount: self.text_value_at(TextParam::AnimatorAmount, frame)?,
            selectors: self
                .text_range_selectors
                .iter()
                .map(|selector| selector.sample(frame))
                .collect(),
        })
    }
    pub fn text_typography_at(&self, frame: Frame) -> Option<TextTypography> {
        Some(TextTypography {
            font_size: self.text_value_at(TextParam::FontSize, frame)?,
            tracking: self.text_value_at(TextParam::Tracking, frame)?,
            leading: self.text_value_at(TextParam::Leading, frame)?,
        })
    }
    /// Plan a scalar edit without materializing an unchanged sparse track.
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
    let next = layer.next_text_animator_id;
    if next == 0 || next == u64::MAX || layer.text_animators.len() > MAX_TEXT_ANIMATORS {
        return Err("Invalid text animator limit or next ID".into());
    }
    if (!layer.text_animators.is_empty() || next != first_selector_id())
        && (version < 63 || !matches!(layer.content, Content::Text { .. }))
    {
        return Err(
            "Additional text animators require a text layer and project version 63 or later".into(),
        );
    }
    let mut ids = BTreeSet::new();
    for animator in &layer.text_animators {
        if animator.id == 0 || animator.id >= next || !ids.insert(animator.id) {
            return Err("Invalid text animator or duplicate ID".into());
        }
        for (&parameter, track) in &animator.parameters {
            if !parameter.is_animator()
                || !parameter.accepts(track.value)
                || track.keys.len() > 10_000
                || track.keys.iter().any(|(frame, key)| {
                    *frame >= duration
                        || !parameter.accepts(key.value)
                        || !key.interpolation.valid()
                })
            {
                return Err("Invalid text animator property or keyframe".into());
            }
        }
    }
    let next = layer.next_text_range_selector_id;
    if next == 0 || next == u64::MAX || layer.text_range_selectors.len() > MAX_TEXT_RANGE_SELECTORS
    {
        return Err("Invalid text range selector limit or next ID".into());
    }
    if (!layer.text_range_selectors.is_empty() || next != first_selector_id())
        && (version < 61 || !matches!(layer.content, Content::Text { .. }))
    {
        return Err(
            "Secondary text selectors require a text layer and project version 61 or later".into(),
        );
    }
    let mut ids = BTreeSet::new();
    for selector in &layer.text_range_selectors {
        if !selector.valid() || selector.id >= next || !ids.insert(selector.id) {
            return Err("Invalid text range selector or duplicate ID".into());
        }
        if !selector.parameters.is_empty() && version < 62 {
            return Err("Text selector tracks require project version 62 or later".into());
        }
        for (&parameter, track) in &selector.parameters {
            if !parameter.accepts(track.value)
                || track.keys.len() > 10_000
                || track.keys.iter().any(|(frame, key)| {
                    *frame >= duration
                        || !parameter.accepts(key.value)
                        || !key.interpolation.valid()
                })
            {
                return Err("Invalid text selector property or keyframe".into());
            }
        }
    }
    if !layer.text_selector.is_default()
        && (version < 59 || !matches!(layer.content, Content::Text { .. }))
    {
        return Err("Text selectors require a text layer and project version 59 or later".into());
    }
    if layer.text_parameters.is_empty() {
        return Ok(());
    }
    if version < 48 || !matches!(layer.content, Content::Text { .. }) {
        return Err("Text tracks require a text layer and project version 48 or later".into());
    }
    for (&parameter, track) in &layer.text_parameters {
        if version < parameter.required_version() {
            return Err(format!(
                "{} tracks require project version {}",
                parameter.label(),
                parameter.required_version()
            ));
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
            property:
                PropertyPath::Text(_)
                | PropertyPath::TextSelector { .. }
                | PropertyPath::TextAnimator { .. },
            edit: TrackEdit::Value { .. },
            ..
        } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(value_edits_only),
        _ => false,
    }
}

// Animator-only configuration, scalar, key and temporal commands share one preservation route.
// Empty nested batches are neutral; empty key sets and mixed legacy commands
// retain their historical routes instead of broadening legacy text semantics.
pub(super) fn animator_edits_only(command: &Command) -> bool {
    fn animator_path(property: PropertyPath) -> bool {
        matches!(property, PropertyPath::Text(parameter) if parameter.is_animator())
            || matches!(property, PropertyPath::TextSelector { .. })
            || matches!(property, PropertyPath::TextAnimator { parameter, .. } if parameter.is_animator())
    }
    fn animator_keys(keys: &[KeyRef]) -> bool {
        !keys.is_empty() && keys.iter().all(|key| animator_path(key.property))
    }
    fn classify(command: &Command) -> Option<bool> {
        match command {
            Command::AddTextAnimator { .. }
            | Command::RemoveTextAnimator { .. }
            | Command::MoveTextAnimator { .. }
            | Command::SetTextAnimatorSelector { .. }
            | Command::SetTextSelector { .. }
            | Command::AddTextRangeSelector { .. }
            | Command::RemoveTextRangeSelector { .. }
            | Command::MoveTextRangeSelector { .. }
            | Command::SetTextRangeSelector { .. } => Some(true),
            Command::EditText { parameter, .. } if parameter.is_animator() => Some(true),
            Command::EditTrack { property, .. }
            | Command::SetTemporalHandle { property, .. }
            | Command::SetTemporalMode { property, .. }
                if animator_path(*property) =>
            {
                Some(true)
            }
            Command::MoveKeys { keys, .. }
            | Command::ScaleKeys { keys, .. }
            | Command::ScaleKeyVelocities { keys, .. }
            | Command::DeleteKeys(keys)
                if animator_keys(keys) =>
            {
                Some(true)
            }
            Command::PasteKeys { keys, .. }
                if !keys.is_empty() && keys.iter().all(|copy| animator_path(copy.key.property)) =>
            {
                Some(true)
            }
            Command::Batch(commands) => commands
                .iter()
                .try_fold(false, |found, command| Some(found | classify(command)?)),
            _ => None,
        }
    }
    classify(command) == Some(true)
}

pub(super) fn animator_required_version(project: &Project) -> Option<u32> {
    project
        .compositions()
        .into_iter()
        .flat_map(|(_, composition)| &composition.layers)
        .flat_map(|layer| {
            layer
                .text_parameters
                .keys()
                .filter(|parameter| parameter.is_animator())
                .map(|parameter| parameter.required_version())
                .chain((!layer.text_selector.is_default()).then_some(59))
                .chain(
                    (!layer.text_animators.is_empty()
                        || layer.next_text_animator_id != first_selector_id())
                    .then_some(63),
                )
                .chain(
                    (!layer.text_range_selectors.is_empty()
                        || layer.next_text_range_selector_id != first_selector_id())
                    .then_some(61),
                )
                .chain(
                    layer
                        .text_range_selectors
                        .iter()
                        .any(|selector| !selector.parameters.is_empty())
                        .then_some(62),
                )
        })
        .max()
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    if let Command::EditTrack {
        id,
        property:
            PropertyPath::TextAnimator {
                animator,
                parameter,
            },
        edit,
    } = command
    {
        return Some((|| {
            let duration = state.project.composition.duration;
            let layer = editing::editable(state, *id)?;
            if !matches!(layer.content, Content::Text { .. }) {
                return Err("Select a text layer".into());
            }
            if !parameter.is_animator() {
                return Err("Invalid text animator parameter".into());
            }
            let item = layer
                .text_animators
                .iter_mut()
                .find(|item| item.id == *animator)
                .ok_or("Text animator not found")?;
            if let TrackEdit::Value { frame, value } = edit {
                if *frame >= duration {
                    return Err("Key is outside the composition".into());
                }
                if !parameter.accepts(*value) {
                    return Err("Invalid text animator value".into());
                }
                if item.value_at(*parameter, *frame) == Some(*value) {
                    return Ok(());
                }
            }
            let track = item
                .track_mut(*parameter)
                .ok_or("Invalid text animator parameter")?;
            time_remap::edit_track(track, duration, edit, |value| parameter.accepts(value))?;
            if let TrackEdit::ToggleAnimation { frame } | TrackEdit::ToggleKey { frame } = edit {
                let (min, max) = parameter.bounds();
                track.value = track.value.clamp(min, max);
                if let Some(key) = track.keys.get_mut(frame) {
                    key.value = key.value.clamp(min, max);
                }
            }
            Ok(())
        })());
    }
    if let Command::AddTextAnimator { id }
    | Command::RemoveTextAnimator { id, .. }
    | Command::MoveTextAnimator { id, .. }
    | Command::SetTextAnimatorSelector { id, .. } = command
    {
        return Some((|| {
            let layer = editing::editable(state, *id)?;
            if !matches!(layer.content, Content::Text { .. }) {
                return Err("Select a text layer".into());
            }
            match command {
                Command::AddTextAnimator { .. } => {
                    if layer.text_animators.len() >= MAX_TEXT_ANIMATORS
                        || layer.next_text_animator_id >= u64::MAX - 1
                    {
                        return Err("Text animator limit reached".into());
                    }
                    layer
                        .text_animators
                        .push(TextAnimator::new(layer.next_text_animator_id));
                    layer.next_text_animator_id += 1;
                }
                Command::RemoveTextAnimator { animator, .. } => {
                    let index = layer
                        .text_animators
                        .iter()
                        .position(|item| item.id == *animator)
                        .ok_or("Text animator not found")?;
                    layer.text_animators.remove(index);
                }
                Command::MoveTextAnimator {
                    animator, index, ..
                } => {
                    if *index >= layer.text_animators.len() {
                        return Err("Text animator index out of range".into());
                    }
                    let from = layer
                        .text_animators
                        .iter()
                        .position(|item| item.id == *animator)
                        .ok_or("Text animator not found")?;
                    if from != *index {
                        let item = layer.text_animators.remove(from);
                        layer.text_animators.insert(*index, item);
                    }
                }
                Command::SetTextAnimatorSelector {
                    animator, selector, ..
                } => {
                    let item = layer
                        .text_animators
                        .iter_mut()
                        .find(|item| item.id == *animator)
                        .ok_or("Text animator not found")?;
                    item.selector = *selector;
                }
                _ => unreachable!(),
            }
            Ok(())
        })());
    }
    if let Command::EditTrack {
        id,
        property:
            PropertyPath::TextSelector {
                selector,
                parameter,
            },
        edit,
    } = command
    {
        return Some((|| {
            let duration = state.project.composition.duration;
            let layer = editing::editable(state, *id)?;
            if !matches!(layer.content, Content::Text { .. }) {
                return Err("Select a text layer".into());
            }
            let item = layer
                .text_range_selectors
                .iter_mut()
                .find(|item| item.id == *selector)
                .ok_or("Text range selector not found")?;
            if let TrackEdit::Value { frame, value } = edit {
                if *frame >= duration {
                    return Err("Key is outside the composition".into());
                }
                if !parameter.accepts(*value) {
                    return Err("Invalid text selector value".into());
                }
                if item.value_at(*parameter, *frame) == *value {
                    return Ok(());
                }
                if !item.parameters.contains_key(parameter) {
                    item.set_base(*parameter, *value);
                    return Ok(());
                }
            }
            let track = item.track_mut(*parameter);
            time_remap::edit_track(track, duration, edit, |value| parameter.accepts(value))?;
            if let TrackEdit::ToggleAnimation { frame } | TrackEdit::ToggleKey { frame } = edit {
                let (min, max) = parameter.bounds();
                track.value = track.value.clamp(min, max);
                if let Some(key) = track.keys.get_mut(frame) {
                    key.value = key.value.clamp(min, max);
                }
            }
            Ok(())
        })());
    }
    if let Command::AddTextRangeSelector { id }
    | Command::RemoveTextRangeSelector { id, .. }
    | Command::MoveTextRangeSelector { id, .. }
    | Command::SetTextRangeSelector { id, .. } = command
    {
        return Some((|| {
            let layer = editing::editable(state, *id)?;
            if !matches!(layer.content, Content::Text { .. }) {
                return Err("Select a text layer".into());
            }
            match command {
                Command::AddTextRangeSelector { .. } => {
                    if layer.text_range_selectors.len() >= MAX_TEXT_RANGE_SELECTORS
                        || layer.next_text_range_selector_id >= u64::MAX - 1
                    {
                        return Err("Text range selector limit reached".into());
                    }
                    layer
                        .text_range_selectors
                        .push(TextRangeSelector::new(layer.next_text_range_selector_id));
                    layer.next_text_range_selector_id += 1;
                }
                Command::RemoveTextRangeSelector { selector, .. } => {
                    let index = layer
                        .text_range_selectors
                        .iter()
                        .position(|item| item.id == *selector)
                        .ok_or("Text range selector not found")?;
                    layer.text_range_selectors.remove(index);
                }
                Command::MoveTextRangeSelector {
                    selector, index, ..
                } => {
                    if *index >= layer.text_range_selectors.len() {
                        return Err("Text range selector index out of range".into());
                    }
                    let from = layer
                        .text_range_selectors
                        .iter()
                        .position(|item| item.id == *selector)
                        .ok_or("Text range selector not found")?;
                    if from != *index {
                        let item = layer.text_range_selectors.remove(from);
                        layer.text_range_selectors.insert(*index, item);
                    }
                }
                Command::SetTextRangeSelector { selector, .. } => {
                    if !selector.valid() {
                        return Err("Invalid text range selector".into());
                    }
                    let item = layer
                        .text_range_selectors
                        .iter_mut()
                        .find(|item| item.id == selector.id)
                        .ok_or("Text range selector not found")?;
                    *item = selector.clone();
                }
                _ => unreachable!(),
            }
            Ok(())
        })());
    }
    if let Command::SetTextSelector { id, selector } = command {
        return Some((|| {
            let layer = editing::editable(state, *id)?;
            if !matches!(layer.content, Content::Text { .. }) {
                return Err("Select a text layer".into());
            }
            layer.text_selector = *selector;
            Ok(())
        })());
    }
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
            if parameter.is_animator() && !parameter.accepts(*value) {
                return Err("Invalid text property value".into());
            }
            // A numeric assignment is not an explicit request to add a key.
            // Animator no-ops retain dormant values, key metadata and source bits.
            // Legacy text scalar behavior remains unchanged.
            if (parameter.is_animator() && layer.text_value_at(*parameter, *frame) == Some(*value))
                || (!layer.text_parameters.contains_key(parameter) && *value == base)
            {
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
