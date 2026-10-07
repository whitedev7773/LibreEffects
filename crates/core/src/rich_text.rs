//! Bounded static character styles. All persisted ranges are UTF-8 byte offsets.
//! Native replacement inherits the character before the edit, or the character
//! after a leading edit. This is a native contract, not AE TextDocument parity.
use super::*;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_TEXT_STYLE_RUNS: usize = 4096;
const MAX_TEXT_BYTES: usize = 16_384;

/// Requested baseline advance for an incoming hard line. Auto is a font-size
/// multiplier; Fixed is in layer pixels. This is a bounded native contract.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TextLeading {
    Auto(f64),
    Fixed(f64),
}
impl TextLeading {
    pub fn valid(self) -> bool {
        match self {
            Self::Auto(value) => value.is_finite() && (0.1..=10.0).contains(&value),
            Self::Fixed(value) => value.is_finite() && (0.1..=20480.0).contains(&value),
        }
    }
    fn pixels(self, font_size: f64) -> f64 {
        match self {
            Self::Auto(ratio) => font_size * ratio,
            Self::Fixed(pixels) => pixels,
        }
    }
}

/// Fully resolved character values, with optional native hard-line leading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextCharacterStyle {
    pub font_family: String,
    /// Exact PostScript identity, or empty to resolve family/weight/slant.
    pub font_face: String,
    pub weight: u16,
    pub italic: bool,
    pub font_size: f64,
    /// Absent retains legacy layout; in explicit layout it inherits TextStyle.leading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leading: Option<TextLeading>,
    pub tracking: f64,
    pub fill_color: u32,
    pub fill_enabled: bool,
    pub stroke_color: u32,
    pub stroke_enabled: bool,
    pub stroke_width: f64,
    pub stroke_over_fill: bool,
    pub stroke_join: TextStrokeJoin,
}

/// A bounded native character edit. Each patch changes only the named value;
/// changing a family also clears an exact face while retaining weight/slant.
#[derive(Clone, Debug, PartialEq)]
pub enum TextCharacterPatch {
    Family(String),
    Font(TextFont),
    FontSize(f64),
    FillColor(u32),
    FillEnabled(bool),
}

impl TextCharacterPatch {
    fn apply(&self, style: &mut TextCharacterStyle) {
        match self {
            Self::Family(family) => {
                if style.font_family != *family {
                    style.font_family = family.clone();
                    style.font_face.clear();
                }
            }
            Self::Font(font) => {
                style.font_family = font.family.clone();
                style.font_face = font.face.clone();
                style.weight = font.weight;
                style.italic = font.italic;
            }
            Self::FontSize(value) => style.font_size = *value,
            Self::FillColor(value) => style.fill_color = *value,
            Self::FillEnabled(value) => style.fill_enabled = *value,
        }
    }
}

/// Uniform values across a nonempty selection. `None` means that field is mixed.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSelectionStyle {
    pub font_family: Option<String>,
    pub font: Option<TextFont>,
    pub font_size: Option<f64>,
    pub fill_color: Option<u32>,
    pub fill_enabled: Option<bool>,
}
impl TextCharacterStyle {
    pub fn from_style(style: &TextStyle, font_size: f64, fill_color: u32) -> Self {
        Self {
            font_family: style.font_family.clone(),
            font_face: style.font_face.clone(),
            weight: style.weight,
            italic: style.italic,
            font_size,
            leading: None,
            tracking: style.tracking,
            fill_color,
            fill_enabled: style.fill_enabled,
            stroke_color: style.stroke_color,
            stroke_enabled: style.stroke_enabled,
            stroke_width: style.stroke_width,
            stroke_over_fill: style.stroke_over_fill,
            stroke_join: style.stroke_join,
        }
    }
    /// Retain paragraph and line settings while applying character attributes.
    pub fn apply_to_text_style(&self, style: &mut TextStyle) {
        style.font_family = self.font_family.clone();
        style.font_face = self.font_face.clone();
        style.weight = self.weight;
        style.italic = self.italic;
        style.tracking = self.tracking;
        style.fill_enabled = self.fill_enabled;
        style.stroke_enabled = self.stroke_enabled;
        style.stroke_color = self.stroke_color;
        style.stroke_width = self.stroke_width;
        style.stroke_over_fill = self.stroke_over_fill;
        style.stroke_join = self.stroke_join;
    }
    pub fn valid(&self) -> bool {
        let mut style = TextStyle::default();
        self.apply_to_text_style(&mut style);
        style.valid()
            && self.font_size.is_finite()
            && (1.0..=2048.0).contains(&self.font_size)
            && self.leading.is_none_or(TextLeading::valid)
            && self.fill_color <= 0xffffff
    }
    pub fn font(&self) -> TextFont {
        TextFont {
            family: self.font_family.clone(),
            face: self.font_face.clone(),
            weight: self.weight,
            italic: self.italic,
        }
    }
    pub(crate) fn replace_font(&mut self, from: &TextFont, to: &TextFont) -> bool {
        if self.font() != *from {
            return false;
        }
        self.font_family = to.family.clone();
        self.font_face = to.face.clone();
        self.weight = to.weight;
        self.italic = to.italic;
        true
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextStyleRun {
    /// Inclusive source UTF-8 byte offset (not glyph, scalar, or UTF-16 index).
    pub start: usize,
    /// Exclusive source UTF-8 byte offset.
    pub end: usize,
    pub style: TextCharacterStyle,
}

/// Optional layer payload. Runs partition every source byte including CR/LF;
/// adjacent identical styles are merged before checking extended grapheme
/// boundaries (including CRLF). Empty text has no runs and retains its
/// insertion style in default_style. No source whitespace is normalized.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RichText {
    pub default_style: TextCharacterStyle,
    pub runs: Vec<TextStyleRun>,
    /// Align around x=0 and place the first baseline at y=0. False retains the
    /// legacy width-based horizontal anchor and first-line top at y=0.
    #[serde(default, skip_serializing_if = "is_false")]
    pub point_origin: bool,
    /// Exact saved horizontal point-text positions (schema 80). Absent retains
    /// the legacy serialized representation and ordinary native shaping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positioning: Option<AuthoredTextPositions>,
}

/// Source-preserving hard-line geometry shared by rendering and text editing.
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextLineMetrics {
    pub range: Range<usize>,
    pub terminator: Range<usize>,
    /// Rendered baseline in authored layer coordinates.
    pub baseline: f64,
    /// Logical em-box top used by selection, picking, carets and IME geometry.
    pub top: f64,
    /// Maximum visible run size, or the insertion size for an empty line.
    pub size: f64,
}
impl RichText {
    pub fn new(
        text: &str,
        default_style: TextCharacterStyle,
        runs: Vec<TextStyleRun>,
    ) -> Result<Self, String> {
        let mut value = Self {
            default_style,
            runs,
            point_origin: false,
            positioning: None,
        };
        value.validate_ranges(text, false)?;
        value.canonicalize();
        value.validate(text)?;
        Ok(value)
    }
    /// Import adapter: only this method interprets run start/end as UTF-16 code
    /// units. A boundary inside a surrogate pair is an error, never rounded.
    pub fn from_utf16_runs(
        text: &str,
        default_style: TextCharacterStyle,
        mut runs: Vec<TextStyleRun>,
    ) -> Result<Self, String> {
        if text.len() > MAX_TEXT_BYTES || runs.len() > MAX_TEXT_STYLE_RUNS {
            return Err("Rich text exceeds its source or run limit".into());
        }
        let mut boundaries = BTreeMap::new();
        let mut utf16 = 0;
        boundaries.insert(0, 0);
        for (byte, ch) in text.char_indices() {
            utf16 += ch.len_utf16();
            boundaries.insert(utf16, byte + ch.len_utf8());
        }
        for run in &mut runs {
            run.start = *boundaries
                .get(&run.start)
                .ok_or("Invalid UTF-16 run start boundary")?;
            run.end = *boundaries
                .get(&run.end)
                .ok_or("Invalid UTF-16 run end boundary")?;
        }
        Self::new(text, default_style, runs)
    }
    pub fn validate(&self, text: &str) -> Result<(), String> {
        self.validate_ranges(text, true)?;
        self.validate_positioning_source(text)
    }
    fn validate_ranges(&self, text: &str, canonical: bool) -> Result<(), String> {
        if text.len() > MAX_TEXT_BYTES
            || self.runs.len() > MAX_TEXT_STYLE_RUNS
            || !self.default_style.valid()
        {
            return Err("Invalid rich text default style or source/run limit".into());
        }
        let mut end = 0;
        let mut previous = None;
        for run in &self.runs {
            if run.start != end
                || run.start >= run.end
                || run.end > text.len()
                || !text.is_char_boundary(run.start)
                || !text.is_char_boundary(run.end)
                || !run.style.valid()
                || run.style.stroke_over_fill != self.default_style.stroke_over_fill
                || (canonical && previous == Some(&run.style))
            {
                return Err("Rich text requires contiguous canonical UTF-8 character runs and uniform paint order".into());
            }
            end = run.end;
            previous = Some(&run.style);
        }
        if end != text.len() {
            return Err("Rich text runs must cover the exact source text".into());
        }
        if canonical {
            // Check after equal-style merging: importer scalar ranges may split
            // a grapheme harmlessly when their complete styles are identical.
            // Persisted character-style boundaries must never do so. UAX #29
            // also keeps a CRLF terminator atomic without rewriting its bytes.
            let mut boundaries = vec![false; text.len() + 1];
            boundaries[text.len()] = true;
            for (byte, _) in text.grapheme_indices(true) {
                boundaries[byte] = true;
            }
            if self.runs.iter().any(|run| !boundaries[run.end]) {
                return Err("Rich character styles must not split a grapheme or CRLF".into());
            }
        }
        Ok(())
    }
    fn canonicalize(&mut self) {
        let mut canonical: Vec<TextStyleRun> = Vec::with_capacity(self.runs.len());
        for run in std::mem::take(&mut self.runs) {
            if let Some(previous) = canonical.last_mut()
                && previous.end == run.start
                && previous.style == run.style
            {
                previous.end = run.end;
            } else {
                canonical.push(run);
            }
        }
        self.runs = canonical;
    }
    /// Returns the style owning this byte. End-of-source uses the final style;
    /// empty source uses the retained insertion style.
    pub fn style_at(&self, byte: usize) -> &TextCharacterStyle {
        self.runs
            .iter()
            .find(|run| byte < run.end)
            .or_else(|| self.runs.last())
            .map_or(&self.default_style, |run| &run.style)
    }
    /// Whether this payload opts into baseline-based incoming-line advances.
    pub fn has_explicit_line_metrics(&self) -> bool {
        self.point_origin
            || self.default_style.leading.is_some()
            || self.runs.iter().any(|run| run.style.leading.is_some())
    }
    /// The anchor, not the left edge of the shaped line. Point text keeps its
    /// authored layer origin independently of alignment and layer dimensions.
    pub fn alignment_origin(&self, width: f64, align: TextAlign) -> f64 {
        if self.point_origin {
            return 0.0;
        }
        match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => width / 2.0,
            TextAlign::Right => width,
        }
    }
    /// Resolve hard-line baselines without fonts or a compositor. Explicit
    /// layout advances by the maximum requested leading on the incoming line;
    /// nonempty lines exclude their terminators. Blank lines use the character
    /// at their start, and a final blank line inherits the final source style.
    /// With no new fields enabled the legacy top/size arithmetic is unchanged.
    pub fn line_metrics(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> Result<Vec<RichTextLineMetrics>, String> {
        self.validate_positioning(text, style)?;
        if !style.valid() || style.paragraph {
            return Err("Rich line metrics require valid point-text settings".into());
        }
        let explicit = self.has_explicit_line_metrics();
        let mut lines = Vec::new();
        let mut legacy_top = 0.0;
        let mut baseline = 0.0;
        for paragraph in text_paragraphs::paragraphs(text) {
            let first_run = self
                .runs
                .partition_point(|run| run.end <= paragraph.range.start);
            let end_run = if paragraph.range.is_empty() {
                first_run
            } else {
                self.runs
                    .partition_point(|run| run.start < paragraph.range.end)
            };
            let runs = &self.runs[first_run..end_run];
            let insertion = self
                .runs
                .get(first_run)
                .or_else(|| self.runs.last())
                .map_or(&self.default_style, |run| &run.style);
            let size = runs
                .iter()
                .map(|run| run.style.font_size)
                .reduce(f64::max)
                .unwrap_or(insertion.font_size);
            let top = if explicit {
                let advance = |s: &TextCharacterStyle| {
                    s.leading
                        .unwrap_or(TextLeading::Auto(style.leading))
                        .pixels(s.font_size)
                };
                if lines.is_empty() {
                    baseline = if self.point_origin { 0.0 } else { size };
                } else {
                    baseline += runs
                        .iter()
                        .map(|run| advance(&run.style))
                        .reduce(f64::max)
                        .unwrap_or_else(|| advance(insertion));
                }
                baseline - size
            } else {
                baseline = legacy_top + size;
                legacy_top
            };
            lines.push(RichTextLineMetrics {
                range: paragraph.range,
                terminator: paragraph.terminator,
                baseline,
                top,
                size,
            });
            if !explicit {
                legacy_top += size * style.leading;
            }
        }
        Ok(lines)
    }
    /// Pure selected-character formatting. Both boundaries must be extended
    /// grapheme boundaries even when the requested change would be a no-op.
    /// Source bytes, the default style, and unselected attributes are retained.
    pub fn format_range(
        &self,
        source: &str,
        range: &Range<usize>,
        patch: &TextCharacterPatch,
    ) -> Result<Self, String> {
        self.validate(source)?;
        check_selection_range(source, range)?;
        let mut runs = Vec::with_capacity(self.runs.len() + 2);
        for run in &self.runs {
            if run.end <= range.start || run.start >= range.end {
                runs.push(run.clone());
                continue;
            }
            let start = run.start.max(range.start);
            let end = run.end.min(range.end);
            if run.start < start {
                runs.push(TextStyleRun {
                    start: run.start,
                    end: start,
                    style: run.style.clone(),
                });
            }
            let mut style = run.style.clone();
            patch.apply(&mut style);
            runs.push(TextStyleRun { start, end, style });
            if end < run.end {
                runs.push(TextStyleRun {
                    start: end,
                    end: run.end,
                    style: run.style.clone(),
                });
            }
        }
        let mut next = Self {
            default_style: self.default_style.clone(),
            runs,
            point_origin: self.point_origin,
            positioning: self.positioning.clone(),
        };
        // A temporary split may exceed the limit and then merge back within it.
        next.canonicalize();
        if !self.same_shaping_as(&next) {
            next.reset_positioning();
        }
        next.validate(source)?;
        Ok(next)
    }
    pub fn selection_style(
        &self,
        source: &str,
        range: &Range<usize>,
    ) -> Result<TextSelectionStyle, String> {
        self.validate(source)?;
        check_selection_range(source, range)?;
        let first = self.style_at(range.start);
        let mut summary = TextSelectionStyle {
            font_family: Some(first.font_family.clone()),
            font: Some(first.font()),
            font_size: Some(first.font_size),
            fill_color: Some(first.fill_color),
            fill_enabled: Some(first.fill_enabled),
        };
        for run in self
            .runs
            .iter()
            .filter(|run| run.start < range.end && run.end > range.start)
        {
            let style = &run.style;
            if style.font_family != first.font_family {
                summary.font_family = None;
            }
            if style.font() != first.font() {
                summary.font = None;
            }
            if style.font_size != first.font_size {
                summary.font_size = None;
            }
            if style.fill_color != first.fill_color {
                summary.fill_color = None;
            }
            if style.fill_enabled != first.fill_enabled {
                summary.fill_enabled = None;
            }
        }
        Ok(summary)
    }
    /// Pure, atomic native buffer edit. Invalid ranges leave both inputs intact.
    /// Insertions inherit the preceding character's style, or the following
    /// character at source start. Full deletion retains that insertion style.
    pub fn replace_range(
        &self,
        source: &str,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(String, Self), String> {
        self.validate(source)?;
        check_range(source, &range)?;
        let size = source.len() - (range.end - range.start);
        let size = size
            .checked_add(replacement.len())
            .ok_or("Rich text source size overflow")?;
        if size > MAX_TEXT_BYTES {
            return Err("Source Text must be at most 16384 UTF-8 bytes".into());
        }
        if &source[range.clone()] == replacement {
            return Ok((source.into(), self.clone()));
        }
        let inherited = self.style_at(range.start.saturating_sub(1)).clone();
        let mut text = String::with_capacity(size);
        text.push_str(&source[..range.start]);
        text.push_str(replacement);
        text.push_str(&source[range.end..]);
        let mut runs = Vec::with_capacity(self.runs.len() + 2);
        for run in &self.runs {
            if run.start < range.start {
                runs.push(TextStyleRun {
                    start: run.start,
                    end: run.end.min(range.start),
                    style: run.style.clone(),
                });
            }
        }
        if !replacement.is_empty() {
            runs.push(TextStyleRun {
                start: range.start,
                end: range.start + replacement.len(),
                style: inherited.clone(),
            });
        }
        let suffix_start = range.start + replacement.len();
        for run in &self.runs {
            if run.end > range.end {
                runs.push(TextStyleRun {
                    start: suffix_start + run.start.max(range.end) - range.end,
                    end: suffix_start + run.end - range.end,
                    style: run.style.clone(),
                });
            }
        }
        let mut next = Self {
            default_style: if text.is_empty() {
                inherited
            } else {
                self.default_style.clone()
            },
            runs,
            point_origin: self.point_origin,
            positioning: None,
        };
        next.canonicalize();
        next.validate(&text)?;
        Ok((text, next))
    }
    pub(crate) fn for_each_style(&mut self, mut update: impl FnMut(&mut TextCharacterStyle)) {
        let original = self.default_style.clone();
        update(&mut self.default_style);
        let mut changed_shaping = !original.same_shaping_as(&self.default_style);
        for run in &mut self.runs {
            let original = run.style.clone();
            update(&mut run.style);
            changed_shaping |= !original.same_shaping_as(&run.style);
        }
        if changed_shaping {
            self.reset_positioning();
        }
        self.canonicalize();
    }
}

fn check_range(text: &str, range: &Range<usize>) -> Result<(), String> {
    if range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return Err("Text replacement must use valid UTF-8 character boundaries".into());
    }
    Ok(())
}

fn check_selection_range(text: &str, range: &Range<usize>) -> Result<(), String> {
    check_range(text, range)?;
    if range.is_empty() {
        return Err("Select one or more characters to format".into());
    }
    let boundary =
        |at| at == text.len() || text.grapheme_indices(true).any(|(start, _)| start == at);
    if !boundary(range.start) || !boundary(range.end) {
        return Err("Character formatting must not split a grapheme or CRLF".into());
    }
    Ok(())
}

impl Layer {
    pub fn rich_text(&self) -> Option<&RichText> {
        self.rich_text.as_ref()
    }
    pub fn base_character_style(&self) -> Option<TextCharacterStyle> {
        let Content::Text { font_size, .. } = self.content else {
            return None;
        };
        Some(TextCharacterStyle::from_style(
            &self.text_style,
            font_size,
            self.color,
        ))
    }
    /// Shared eligibility for a native rich draft, including plain-text layers.
    pub fn rich_text_eligibility(&self) -> Result<(), String> {
        if !matches!(self.content, Content::Text { .. }) {
            return Err("Rich character styles require text content".into());
        }
        if self.source_text_animation.animated()
            || !self.text_parameters.is_empty()
            || !self.text_animators.is_empty()
            || self.text_style.paragraph
        {
            return Err(
                "Rich character styles currently require static point text without text animation"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn character_style_at(&self, byte: usize) -> Option<TextCharacterStyle> {
        self.rich_text
            .as_ref()
            .map(|rich| rich.style_at(byte).clone())
            .or_else(|| self.base_character_style())
    }
}

pub(super) fn validate(layer: &Layer, version: u32) -> Result<(), String> {
    let Some(rich) = &layer.rich_text else {
        return Ok(());
    };
    let Content::Text { text, .. } = &layer.content else {
        return Err("Rich character styles require text content".into());
    };
    if version < 71 {
        return Err("Rich character styles require project version 71".into());
    }
    if version < 74 && rich.has_explicit_line_metrics() {
        return Err("Point-text origin and character leading require project version 74".into());
    }
    if version < 80 && rich.positioning.is_some() {
        return Err("Saved text spacing requires project version 80".into());
    }
    layer.rich_text_eligibility()?;
    rich.validate_positioning(text, &layer.text_style)
}

pub(super) fn required_version(project: &Project) -> Option<u32> {
    project
        .compositions()
        .iter()
        .flat_map(|(_, comp)| &comp.layers)
        .filter_map(|layer| layer.rich_text.as_ref())
        .map(|rich| {
            if rich.positioning.is_some() {
                80
            } else if rich.has_explicit_line_metrics() {
                74
            } else {
                71
            }
        })
        .max()
}

pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::SetRichText { .. }
        | Command::SetStyledText { .. }
        | Command::ReplaceTextRange { .. } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}

/// Preserve styles for full-string native edits by isolating one changed scalar
/// region. Unchanged common prefix/suffix retain their exact run ownership.
pub(super) fn replace_source(layer: &mut Layer, next: &str) -> Result<(), String> {
    let Some(rich) = &layer.rich_text else {
        return Ok(());
    };
    let Content::Text { text, .. } = &layer.content else {
        return Err("Select a text layer".into());
    };
    if text == next {
        return Ok(());
    }
    let prefix: usize = text
        .chars()
        .zip(next.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let suffix: usize = text[prefix..]
        .chars()
        .rev()
        .zip(next[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let (_, replacement) = rich.replace_range(
        text,
        prefix..text.len() - suffix,
        &next[prefix..next.len() - suffix],
    )?;
    layer.rich_text = Some(replacement);
    Ok(())
}

/// Update dependent style storage before legacy commands change their baselines.
/// Rejection is safe because all callers mutate an isolated project candidate.
pub(super) fn prepare(state: &mut Snapshot, command: &Command) -> Result<(), String> {
    match command {
        Command::SetTextStyle { id, style } => {
            let layer = editing::editable(state, *id)?;
            if let Some(rich) = &mut layer.rich_text {
                if !style.valid() {
                    return Err("Invalid text style".into());
                }
                let old = &layer.text_style;
                if old.align != style.align
                    || old.leading != style.leading
                    || old.paragraph != style.paragraph
                {
                    rich.reset_positioning();
                }
                rich.for_each_style(|target| {
                    macro_rules! changed { ($($field:ident),*) => { $(if old.$field != style.$field { target.$field = style.$field.clone(); })* }; }
                    changed!(font_family, font_face, weight, italic, tracking, fill_enabled, stroke_enabled, stroke_color, stroke_width, stroke_over_fill, stroke_join);
                });
            }
        }
        Command::SetContent { id, content } => {
            let layer = editing::editable(state, *id)?;
            if layer.rich_text.is_some() {
                let Content::Text { text, font_size } = content else {
                    return Err("Clear rich character styles before changing content type".into());
                };
                replace_source(layer, text)?;
                let Content::Text { font_size: old, .. } = layer.content else {
                    return Err("Select a text layer".into());
                };
                if old != *font_size {
                    layer
                        .rich_text
                        .as_mut()
                        .unwrap()
                        .for_each_style(|style| style.font_size = *font_size);
                }
            }
        }
        Command::SetColor { id, color } => {
            let layer = editing::editable(state, *id)?;
            if layer.color != *color
                && let Some(rich) = &mut layer.rich_text
            {
                rich.for_each_style(|style| style.fill_color = *color);
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    match command {
        Command::SetStyledText {
            id,
            text,
            rich_text,
        } => Some((|| {
            let layer = editing::editable(state, *id)?;
            layer.rich_text_eligibility()?;
            let rich_text = replacement_rich_text(layer, text, rich_text);
            rich_text.validate(text)?;
            // Install the final source and runs together. Re-diffing against
            // the old styles could reject a now-valid grapheme join.
            if let Content::Text { text: source, .. } = &mut layer.content {
                *source = text.clone();
            }
            layer.rich_text = Some(rich_text);
            validate(layer, 80)
        })()),
        Command::SetRichText { id, rich_text } => Some((|| {
            let layer = editing::editable(state, *id)?;
            let Content::Text { text, .. } = &layer.content else {
                return Err("Select a text layer".into());
            };
            let rich_text = rich_text
                .as_ref()
                .map(|rich| replacement_rich_text(layer, text, rich));
            if let Some(rich) = &rich_text {
                rich.validate(text)?;
            }
            layer.rich_text = rich_text;
            // Feature validation uses the future floor; the transaction handles
            // the actual schema upgrade only if this edit changes stored data.
            validate(layer, 80)
        })()),
        Command::ReplaceTextRange {
            id,
            start,
            end,
            text,
        } => Some((|| {
            let layer = editing::editable(state, *id)?;
            if layer.source_text_animation.animated() {
                return Err("Range replacement requires static Source Text".into());
            }
            let Content::Text { text: source, .. } = &layer.content else {
                return Err("Select a text layer".into());
            };
            let range = *start..*end;
            check_range(source, &range)?;
            let replacement = if let Some(rich) = &layer.rich_text {
                let (source, rich) = rich.replace_range(source, range, text)?;
                layer.rich_text = Some(rich);
                source
            } else {
                let len = source.len() - (end - start);
                if text.len() > MAX_TEXT_BYTES || len > MAX_TEXT_BYTES - text.len() {
                    return Err("Source Text must be at most 16384 UTF-8 bytes".into());
                }
                let mut next = source.clone();
                next.replace_range(range, text);
                next
            };
            if let Content::Text { text: source, .. } = &mut layer.content {
                *source = replacement;
            }
            Ok(())
        })()),
        _ => None,
    }
}

// A native formatting/draft caller may carry the old cache through a source or
// typography edit. Reset that cache atomically; newly authored payloads still
// receive full validation and are never silently repaired.
fn replacement_rich_text(layer: &Layer, text: &str, rich: &RichText) -> RichText {
    let mut next = rich.clone();
    if let Some(old) = &layer.rich_text
        && old.positioning.is_some()
        && old.positioning == next.positioning
        && (!matches!(&layer.content, Content::Text { text: old, .. } if old == text)
            || !old.same_shaping_as(&next))
    {
        next.reset_positioning();
    }
    next
}
