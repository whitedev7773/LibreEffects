//! Exact, bounded horizontal point-text origins. Font bytes and native shaping
//! are verified by the compositor before these positions can be rendered.
use super::*;
use unicode_segmentation::UnicodeSegmentation;

/// Saved positions for an ordered subset of complete nonempty hard lines.
/// Unlisted lines keep ordinary layout; source bytes are never normalized.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredTextPositions {
    pub text: String,
    pub align: TextAlign,
    pub lines: Vec<AuthoredTextLine>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredTextLine {
    /// Exact visible hard-line range in original UTF-8 bytes, excluding CR/LF.
    pub start: usize,
    pub end: usize,
    /// SHA-256 of immutable font file bytes, with the collection face index.
    pub font_sha256: String,
    pub font_index: u32,
    /// Exact shaping snapshot. Paint is ignored; leading must contain the
    /// resolved effective value, including inherited TextStyle leading.
    pub style: TextCharacterStyle,
    pub glyphs: Vec<AuthoredTextGlyph>,
    /// Logical terminal boundary in layer pixels; it is not another glyph.
    pub end_x: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredTextGlyph {
    /// One complete, single-scalar grapheme in original UTF-8 bytes.
    pub start: usize,
    pub end: usize,
    pub glyph_id: u16,
    /// Absolute horizontal glyph origin in authored layer-local pixels.
    pub x: f64,
}

impl TextCharacterStyle {
    /// Geometry-affecting character values. Paint changes retain saved spacing.
    pub fn same_shaping_as(&self, other: &Self) -> bool {
        self.same_shaping_except_leading(other) && self.leading == other.leading
    }

    fn same_shaping_except_leading(&self, other: &Self) -> bool {
        self.font_family == other.font_family
            && self.font_face == other.font_face
            && self.weight == other.weight
            && self.italic == other.italic
            && self.font_size == other.font_size
            && self.tracking == other.tracking
    }
}

impl RichText {
    /// Reset only saved horizontal origins. Callers keep this in the same
    /// document or draft history entry as the corresponding typography edit.
    pub fn reset_positioning(&mut self) -> bool {
        self.positioning.take().is_some()
    }

    /// Validate the source and saved shaping snapshot against actual layout
    /// settings. Runtime font identity and glyph admission remain mandatory.
    pub fn validate_positioning(&self, text: &str, style: &TextStyle) -> Result<(), String> {
        self.validate(text)?;
        let Some(positioning) = &self.positioning else {
            return Ok(());
        };
        if !style.valid() || style.paragraph || positioning.align != style.align {
            return Err("Saved text spacing requires matching point-text alignment".into());
        }
        for line in &positioning.lines {
            for run in self.positioned_line_runs(line) {
                let leading = run
                    .style
                    .leading
                    .unwrap_or(TextLeading::Auto(style.leading));
                if line.style.leading != Some(leading) {
                    return Err("Saved text spacing has a mismatched leading snapshot".into());
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_positioning_source(&self, text: &str) -> Result<(), String> {
        let Some(positioning) = &self.positioning else {
            return Ok(());
        };
        if !self.point_origin
            || positioning.text != text
            || positioning.lines.is_empty()
            || positioning.lines.len() > text.len()
        {
            return Err(
                "Saved text spacing requires an exact source snapshot and point origin".into(),
            );
        }
        let coordinate = |x: f64| x.is_finite() && x.abs() <= 1_000_000.0;
        let mut paragraphs =
            text_paragraphs::paragraphs(text).filter(|line| !line.range.is_empty());
        let mut previous_end = 0;
        for line in &positioning.lines {
            if line.start < previous_end || line.start >= line.end {
                return Err("Saved text spacing lines must be ordered and nonoverlapping".into());
            }
            let Some(paragraph) = paragraphs.find(|paragraph| paragraph.range.start >= line.start)
            else {
                return Err("Saved text spacing must map complete nonempty hard lines".into());
            };
            if paragraph.range != (line.start..line.end) {
                return Err("Saved text spacing must map complete nonempty hard lines".into());
            }
            previous_end = line.end;
            if !line.style.valid()
                || line.style.font_face.trim().is_empty()
                || line.style.leading.is_none()
                || line.font_sha256.len() != 64
                || !line
                    .font_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || !coordinate(line.end_x)
                || line.glyphs.is_empty()
                || line.glyphs.len() > paragraph.text.len()
            {
                return Err(
                    "Saved text spacing requires valid exact font, style and geometry snapshots"
                        .into(),
                );
            }
            for run in self.positioned_line_runs(line) {
                if !line.style.same_shaping_except_leading(&run.style)
                    || run
                        .style
                        .leading
                        .is_some_and(|leading| line.style.leading != Some(leading))
                {
                    return Err(
                        "Saved text spacing requires one matching shaping style per line".into(),
                    );
                }
            }
            let mut glyphs = line.glyphs.iter();
            let mut previous_x = None;
            for (offset, grapheme) in paragraph.text.grapheme_indices(true) {
                let Some(glyph) = glyphs.next() else {
                    return Err(
                        "Saved text spacing glyphs must partition their complete hard line".into(),
                    );
                };
                let mut chars = grapheme.chars();
                let scalar = chars.next().expect("graphemes are nonempty");
                if chars.next().is_some()
                    || scalar.is_control()
                    || glyph.start != line.start + offset
                    || glyph.end != glyph.start + grapheme.len()
                    || glyph.glyph_id == 0
                    || !coordinate(glyph.x)
                    || previous_x.is_some_and(|x| glyph.x <= x)
                    || glyph.x >= line.end_x
                {
                    return Err("Saved text spacing requires ordered single-scalar glyph ranges and finite increasing origins".into());
                }
                previous_x = Some(glyph.x);
            }
            if glyphs.next().is_some() {
                return Err(
                    "Saved text spacing glyphs must partition their complete hard line".into(),
                );
            }
        }
        Ok(())
    }

    /// Compare character geometry independently of paint-created run splits.
    /// Callers compare exact source bytes separately.
    pub(crate) fn same_shaping_as(&self, other: &Self) -> bool {
        if self.point_origin != other.point_origin
            || self.proportional_metrics != other.proportional_metrics
            || !self.default_style.same_shaping_as(&other.default_style)
            || self.runs.last().map(|run| run.end) != other.runs.last().map(|run| run.end)
        {
            return false;
        }
        let (mut a, mut b) = (0, 0);
        while let (Some(left), Some(right)) = (self.runs.get(a), other.runs.get(b)) {
            if !left.style.same_shaping_as(&right.style) {
                return false;
            }
            a += usize::from(left.end <= right.end);
            b += usize::from(right.end <= left.end);
        }
        a == self.runs.len() && b == other.runs.len()
    }

    fn positioned_line_runs(&self, line: &AuthoredTextLine) -> &[TextStyleRun] {
        let start = self.runs.partition_point(|run| run.end <= line.start);
        let end = self.runs.partition_point(|run| run.start < line.end);
        &self.runs[start..end]
    }
}

impl Layer {
    pub fn has_authored_text_positions(&self) -> bool {
        self.rich_text
            .as_ref()
            .is_some_and(|rich| rich.positioning.is_some())
    }
}
