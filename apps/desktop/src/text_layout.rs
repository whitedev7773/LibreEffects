//! Grapheme hit regions in layer coordinates, shaped with the selected real face.
use super::Session;
pub(crate) use crate::text_metrics as metrics;
use std::ops::Range;
use std::{cell::RefCell, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
#[derive(Clone, Debug)]
pub(crate) struct Cell {
    pub range: Range<usize>,
    pub x1: f64,
    pub x2: f64,
    pub y: f64,
    pub height: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_paragraph_carets_use_original_bytes_and_keep_crlf_atomic() {
        use libre_effects_core::{TextStyle, text_paragraphs::paragraphs};
        let text = "English e\u{301}\r日本語👩‍💻\r\n한국어\n\r끝\r\n";
        let source: Vec<_> = paragraphs(text).collect();
        let canonical = source.iter().map(|p| p.text).collect::<Vec<_>>().join("\n");
        let normalized: Vec<_> = paragraphs(&canonical).collect();
        for paragraph in [false, true] {
            let style = TextStyle {
                paragraph,
                ..Default::default()
            };
            let actual = Layout::shape(text, 24.0, 90.0, &style);
            let expected = Layout::shape(&canonical, 24.0, 90.0, &style);
            assert_eq!(actual.bounds(), expected.bounds());
            for (source, normalized) in source.iter().zip(&normalized) {
                for relative in source
                    .text
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain([source.text.len()])
                {
                    assert_eq!(
                        actual.caret(source.range.start + relative),
                        expected.caret(normalized.range.start + relative)
                    );
                }
                for at in source.terminator.clone() {
                    assert_eq!(actual.caret(at), actual.caret(source.range.end));
                }
            }
            for (at, _) in &actual.carets {
                assert!(text.is_char_boundary(*at));
                assert!(
                    !source
                        .iter()
                        .any(|p| *at > p.terminator.start && *at < p.terminator.end)
                );
            }
            for cell in &actual.cells {
                assert!(!text[cell.range.clone()].contains(['\r', '\n']));
            }
            assert_eq!(actual.caret(text.len()), expected.caret(canonical.len()));
        }
    }

    #[test]
    fn multiline_graphemes_alignment_and_rtl_have_stable_hit_regions() {
        let mut s = Session::new(
            &libre_effects_core::Project::default(),
            0,
            0,
            None,
            [0.0; 2],
        )
        .unwrap();
        s.buffer.text = "한글 office e\u{301}\n\n".into();
        let l = Layout::new(&s);
        for (i, p) in &l.carets {
            assert!(s.buffer.text.is_char_boundary(*i));
            assert!(p.iter().all(|v| v.is_finite()));
            assert_eq!(l.hit([p[0], p[1] + s.font_size * 0.5]), *i);
        }
        let end = s.buffer.text.len();
        assert_eq!(l.caret(end), [0.0, 2.0 * s.font_size * s.style.leading]);
        s.buffer.text = "ABC".into();
        let left = Layout::new(&s);
        let width = left.caret(3)[0];
        s.style.align = libre_effects_core::TextAlign::Center;
        let center = Layout::new(&s);
        assert!((center.caret(0)[0] - (s.width - width) / 2.0).abs() < 1e-6);
        s.style.align = libre_effects_core::TextAlign::Right;
        assert!((Layout::new(&s).caret(3)[0] - s.width).abs() < 1e-6);
        s.buffer.text = "אבג".into();
        let rtl = Layout::new(&s);
        assert_eq!(rtl.cells.len(), 3);
        assert!(rtl.cells.iter().all(|c| c.x1 > c.x2));
        assert!(rtl.caret(0)[0] > rtl.caret(s.buffer.text.len())[0]);
        s.style.align = libre_effects_core::TextAlign::Left;
        s.buffer.text = "First\nSecond\nThird".into();
        let l = Layout::new(&s);
        assert!(l.contains([20.0, 2.0 * s.font_size * s.style.leading + 20.0]));
        assert!(!l.contains([-20.0, -20.0]));
    }
    #[test]
    fn layout_cache_and_tracking_use_rendered_endpoints_without_trailing_space() {
        let style = libre_effects_core::TextStyle {
            tracking: 200.0,
            ..Default::default()
        };
        let a = Layout::shape("AA", 72.0, 640.0, &style);
        let b = Layout::shape("AA", 72.0, 640.0, &style);
        assert!(Arc::ptr_eq(&a, &b));
        let plain = Layout::shape("AA", 72.0, 640.0, &Default::default());
        assert!(!Arc::ptr_eq(&a, &plain));
        assert!((a.caret(2)[0] - plain.caret(2)[0] - 14.4).abs() < 0.001);
        let center = Layout::shape(
            "AA",
            72.0,
            640.0,
            &libre_effects_core::TextStyle {
                align: libre_effects_core::TextAlign::Center,
                ..style
            },
        );
        assert!((center.caret(0)[0] + center.caret(2)[0] - 640.0).abs() < 0.001);
        let emoji = Layout::shape("A👩‍💻B", 72.0, 640.0, &Default::default());
        assert_eq!(emoji.cells.len(), 3);
        assert_eq!(emoji.cells[1].range, 1..12);
        assert_eq!(emoji.caret(5), emoji.caret(1));
        let word = Layout::shape("word ", 72.0, 640.0, &Default::default());
        let end = word.caret(4);
        assert_eq!(word.hit([end[0] - 0.1, 30.0]), 4);
        assert_eq!(word.hit_character([end[0] - 0.1, 30.0]), 3);
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Layout {
    pub cells: Vec<Cell>,
    pub carets: Vec<(usize, [f64; 2])>,
    pub size: f64,
    hard_breaks: Vec<Range<usize>>,
    /// Rich lines have independent maximum font sizes; legacy geometry is unchanged.
    line_sizes: Vec<(f64, f64)>,
    /// Authored glyph ink can overhang its independent logical caret terminal.
    /// Native/legacy text keeps its existing logical-only geometry.
    paint_lines: Vec<(Range<usize>, [f64; 4])>,
    /// Failed rich composition never falls back to unstyled caret geometry.
    pub error: Option<String>,
}
#[derive(PartialEq)]
struct Key {
    text: String,
    size: f64,
    width: f64,
    style: libre_effects_core::TextStyle,
}
#[derive(PartialEq)]
struct RichKey {
    text: String,
    rich: libre_effects_core::RichText,
    width: f64,
    style: libre_effects_core::TextStyle,
}
thread_local! { static LAST_RICH_LAYOUT: RefCell<Option<(RichKey,Arc<Layout>)>> = const {RefCell::new(None)}; }
thread_local! { static LAST_LAYOUT: RefCell<Option<(Key,Arc<Layout>)>> = const {RefCell::new(None)}; }
impl Layout {
    pub fn for_layer(
        layer: &libre_effects_core::Layer,
        frame: libre_effects_core::Frame,
    ) -> Option<Arc<Self>> {
        let text = layer.source_text_at(frame)?;
        let typography = layer.text_typography_at(frame)?;
        let mut style = layer.text_style();
        typography.apply_to_style(&mut style);
        if let Some(rich) = layer.rich_text() {
            return Self::shape_rich(text, rich, layer.width(), &style).ok();
        }
        Some(Self::shape(
            text,
            typography.font_size,
            layer.width(),
            &style,
        ))
    }
    pub fn new(session: &Session) -> Arc<Self> {
        if let Some(rich) = &session.buffer.rich_text {
            return Self::shape_rich(&session.buffer.text, rich, session.width, &session.style)
                .unwrap_or_else(|error| {
                    Arc::new(Self {
                        size: session.font_size,
                        error: Some(error),
                        ..Default::default()
                    })
                });
        }
        Self::shape(
            &session.buffer.text,
            session.font_size,
            session.width,
            &session.style,
        )
    }
    pub fn shape(
        text: &str,
        font_size: f64,
        width: f64,
        style: &libre_effects_core::TextStyle,
    ) -> Arc<Self> {
        LAST_LAYOUT.with(|last| {
            if let Some((key, layout)) = &*last.borrow() {
                if key.text == text
                    && key.size == font_size
                    && key.width == width
                    && &key.style == style
                {
                    return layout.clone();
                }
            }
            let layout = Arc::new(Self::compute(text, font_size, width, style));
            *last.borrow_mut() = Some((
                Key {
                    text: text.into(),
                    size: font_size,
                    width,
                    style: style.clone(),
                },
                layout.clone(),
            ));
            layout
        })
    }
    pub fn shape_rich(
        text: &str,
        rich: &libre_effects_core::RichText,
        width: f64,
        style: &libre_effects_core::TextStyle,
    ) -> Result<Arc<Self>, String> {
        LAST_RICH_LAYOUT.with(|last| {
            if let Some((key, layout)) = &*last.borrow() {
                if key.text == text
                    && key.rich == *rich
                    && key.width == width
                    && key.style == *style
                {
                    return Ok(layout.clone());
                }
            }
            let composition = crate::rich_text_render::compose(text, rich, width, style)?;
            let mut result = Self {
                size: rich.default_style.font_size,
                ..Default::default()
            };
            for line in &composition.lines {
                result.line_sizes.push((line.y, line.size));
                if let Some(bounds) = line.paint_bounds {
                    result.paint_lines.push((line.range.clone(), bounds));
                }
                if !line.terminator.is_empty() {
                    result.hard_breaks.push(line.terminator.clone());
                }
                let base = line.range.start;
                let source = &text[line.range.clone()];
                let bidi = unicode_bidi::BidiInfo::new(source, Some(unicode_bidi::Level::ltr()));
                let graphemes: Vec<_> = source
                    .grapheme_indices(true)
                    .map(|(i, g)| base + i..base + i + g.len())
                    .collect();
                let mut regions = vec![None::<(f64, f64)>; graphemes.len()];
                for cluster in &line.clusters {
                    let start = graphemes.partition_point(|g| g.end <= cluster.range.start);
                    let end = graphemes.partition_point(|g| g.start < cluster.range.end);
                    let count = (end - start).max(1) as f64;
                    let rtl = bidi
                        .levels
                        .get(cluster.range.start - base)
                        .is_some_and(|l| l.is_rtl());
                    for (j, index) in (start..end).enumerate() {
                        let position = if rtl {
                            count - j as f64 - 1.0
                        } else {
                            j as f64
                        };
                        let a = cluster.x + (cluster.end - cluster.x) * position / count;
                        let b = cluster.x + (cluster.end - cluster.x) * (position + 1.0) / count;
                        let (old_a, old_b) = regions[index].unwrap_or((a.min(b), a.max(b)));
                        regions[index] = Some((old_a.min(a.min(b)), old_b.max(a.max(b))));
                    }
                }
                let mut last = rich.alignment_origin(width, style.align);
                if graphemes.is_empty() {
                    result.carets.push((base, [last, line.y]));
                }
                for (range, region) in graphemes.into_iter().zip(regions) {
                    let (a, b) = region.unwrap_or((last, last));
                    let (x1, x2) = if bidi
                        .levels
                        .get(range.start - base)
                        .is_some_and(|l| l.is_rtl())
                    {
                        (b, a)
                    } else {
                        (a, b)
                    };
                    result.carets.push((range.start, [x1, line.y]));
                    result.carets.push((range.end, [x2, line.y]));
                    result.cells.push(Cell {
                        range,
                        x1,
                        x2,
                        y: line.y,
                        height: line.size * 1.2,
                    });
                    last = x2;
                }
            }
            let layout = Arc::new(result);
            *last.borrow_mut() = Some((
                RichKey {
                    text: text.into(),
                    rich: rich.clone(),
                    width,
                    style: style.clone(),
                },
                layout.clone(),
            ));
            Ok(layout)
        })
    }
    fn compute(
        text: &str,
        font_size: f64,
        width: f64,
        style: &libre_effects_core::TextStyle,
    ) -> Self {
        let mut result = Self {
            size: font_size,
            ..Default::default()
        };
        let lines = crate::text_flow::lines(text, font_size, width, style);
        for flow in lines.iter() {
            if !flow.terminator.is_empty() {
                result.hard_breaks.push(flow.terminator.clone());
            }
            let base = flow.range.start;
            let line = &text[flow.range.clone()];
            let drawn = &text[base..flow.visible_end];
            let y = flow.y;
            let clusters =
                metrics::clusters(drawn, font_size, flow.width, style).unwrap_or_default();
            let bidi = unicode_bidi::BidiInfo::new(line, Some(unicode_bidi::Level::ltr()));
            let graphemes: Vec<_> = line
                .grapheme_indices(true)
                .map(|(i, g)| i..i + g.len())
                .collect();
            let mut regions = vec![None::<(f64, f64)>; graphemes.len()];
            for cluster in &clusters {
                let start = graphemes.partition_point(|g| g.end <= cluster.range.start);
                let end = graphemes.partition_point(|g| g.start < cluster.range.end);
                let indexes = start..end;
                let count = indexes.len().max(1) as f64;
                let rtl = bidi
                    .levels
                    .get(cluster.range.start)
                    .is_some_and(|l| l.is_rtl());
                for (j, index) in indexes.into_iter().enumerate() {
                    let position = if rtl {
                        count - j as f64 - 1.0
                    } else {
                        j as f64
                    };
                    let a = flow.x + cluster.x + (cluster.end - cluster.x) * position / count;
                    let b =
                        flow.x + cluster.x + (cluster.end - cluster.x) * (position + 1.0) / count;
                    let (old_a, old_b) = regions[index].unwrap_or((a.min(b), a.max(b)));
                    regions[index] = Some((old_a.min(a.min(b)), old_b.max(a.max(b))));
                }
            }
            let mut last = flow.x
                + match style.align {
                    libre_effects_core::TextAlign::Left => 0.0,
                    libre_effects_core::TextAlign::Center => flow.width / 2.0,
                    libre_effects_core::TextAlign::Right => flow.width,
                };
            if graphemes.is_empty() {
                result.carets.push((base, [last, y]));
            }
            for (range, region) in graphemes.into_iter().zip(regions) {
                let (a, b) = region.unwrap_or((last, last));
                let (x1, x2) = if bidi.levels.get(range.start).is_some_and(|l| l.is_rtl()) {
                    (b, a)
                } else {
                    (a, b)
                };
                result.carets.push((base + range.start, [x1, y]));
                result.carets.push((base + range.end, [x2, y]));
                result.cells.push(Cell {
                    range: base + range.start..base + range.end,
                    x1,
                    x2,
                    y,
                    height: font_size * 1.2,
                });
                last = x2;
            }
        }
        result
    }
    /// Logical point-text box, including spaces and empty/trailing lines.
    /// This is evaluated geometry, never a mutation of authored layer dimensions.
    pub fn bounds(&self) -> [f64; 4] {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for (_, [x, y]) in &self.carets {
            min[0] = min[0].min(*x);
            min[1] = min[1].min(*y);
            max[0] = max[0].max(*x);
            max[1] = max[1].max(*y + self.height_at([*x, *y]));
        }
        for (_, bounds) in &self.paint_lines {
            min[0] = min[0].min(bounds[0]);
            min[1] = min[1].min(bounds[1]);
            max[0] = max[0].max(bounds[0] + bounds[2]);
            max[1] = max[1].max(bounds[1] + bounds[3]);
        }
        if self.carets.is_empty() {
            return [0.0, 0.0, 1.0, self.line_height()];
        }
        [min[0], min[1], (max[0] - min[0]).max(1.0), max[1] - min[1]]
    }
    pub fn contains(&self, p: [f64; 2]) -> bool {
        self.cells.iter().any(|c| {
            p[0] >= c.x1.min(c.x2)
                && p[0] <= c.x1.max(c.x2)
                && p[1] >= c.y
                && p[1] <= c.y + c.height
        }) || self
            .paint_lines
            .iter()
            .any(|(_, bounds)| in_bounds(p, *bounds))
    }
    /// Shared logical height for hit regions, selections, carets and IME anchors.
    pub fn line_height(&self) -> f64 {
        self.size * 1.2
    }
    fn size_at(&self, y: f64) -> f64 {
        self.line_sizes
            .iter()
            .find(|(top, _)| (*top - y).abs() < 0.001)
            .map_or(self.size, |(_, size)| *size)
    }
    pub fn height_at(&self, at: [f64; 2]) -> f64 {
        self.size_at(at[1]) * 1.2
    }
    pub fn caret_rect(&self, at: [f64; 2]) -> [f64; 4] {
        [at[0], at[1], 1.0, self.height_at(at)]
    }
    pub fn hit_character(&self, p: [f64; 2]) -> usize {
        self.cells
            .iter()
            .filter(|c| {
                p[0] >= c.x1.min(c.x2)
                    && p[0] <= c.x1.max(c.x2)
                    && p[1] >= c.y
                    && p[1] <= c.y + c.height
            })
            .min_by(|a, b| {
                (a.y + self.size_at(a.y) * 0.5 - p[1])
                    .abs()
                    .total_cmp(&(b.y + self.size_at(b.y) * 0.5 - p[1]).abs())
            })
            .map_or_else(
                || {
                    // Outside logical cells but inside authored ink, select the
                    // nearest character on that painted line, never its terminal
                    // source offset. Caret placement itself remains unchanged.
                    self.paint_lines
                        .iter()
                        .filter(|(_, bounds)| in_bounds(p, *bounds))
                        .flat_map(|(range, _)| {
                            self.cells
                                .iter()
                                .filter(move |cell| range.contains(&cell.range.start))
                        })
                        .min_by(|a, b| {
                            let distance = |c: &Cell| {
                                let nearest = p[0].clamp(c.x1.min(c.x2), c.x1.max(c.x2));
                                (p[0] - nearest).abs()
                            };
                            distance(a).total_cmp(&distance(b))
                        })
                        .map_or_else(|| self.hit(p), |cell| cell.range.start)
                },
                |c| c.range.start,
            )
    }
    pub fn caret(&self, at: usize) -> [f64; 2] {
        // CRLF is one grapheme and one hard break. IME/source UTF-16 offsets
        // can still address its middle byte; display that at the preceding
        // paragraph's end without inventing a caret between CR and LF.
        let at = self
            .hard_breaks
            .iter()
            .find(|range| range.contains(&at))
            .map_or(at, |range| range.start);
        self.carets
            .iter()
            .rev()
            .min_by_key(|(i, _)| i.abs_diff(at))
            .map_or([0.0; 2], |(_, p)| *p)
    }
    pub fn hit(&self, p: [f64; 2]) -> usize {
        self.hit_caret(p).0
    }
    pub fn hit_caret(&self, p: [f64; 2]) -> (usize, [f64; 2]) {
        self.carets
            .iter()
            .min_by(|(_, a), (_, b)| {
                let d = |q: &[f64; 2]| {
                    ((q[1] + self.size_at(q[1]) * 0.5 - p[1]).abs() * 10000.0) + (q[0] - p[0]).abs()
                };
                d(a).total_cmp(&d(b))
            })
            .copied()
            .unwrap_or((0, [0.0; 2]))
    }
}

fn in_bounds(point: [f64; 2], bounds: [f64; 4]) -> bool {
    point[0] >= bounds[0]
        && point[0] <= bounds[0] + bounds[2]
        && point[1] >= bounds[1]
        && point[1] <= bounds[1] + bounds[3]
}
