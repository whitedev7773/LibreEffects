//! Grapheme hit regions in layer coordinates, shaped with the selected real face.
use super::Session;
#[path = "text_metrics.rs"]
pub(crate) mod metrics;
use std::ops::Range;
use std::{cell::RefCell, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
#[derive(Clone, Debug)]
pub(crate) struct Cell {
    pub range: Range<usize>,
    pub x1: f64,
    pub x2: f64,
    pub y: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
#[derive(PartialEq)]
struct Key {
    text: String,
    size: f64,
    width: f64,
    style: libre_effects_core::TextStyle,
}
thread_local! { static LAST_LAYOUT: RefCell<Option<(Key,Arc<Layout>)>> = const {RefCell::new(None)}; }
impl Layout {
    pub fn for_layer(
        layer: &libre_effects_core::Layer,
        frame: libre_effects_core::Frame,
    ) -> Option<Arc<Self>> {
        let libre_effects_core::Content::Text { text, .. } = layer.content() else {
            return None;
        };
        let typography = layer.text_typography_at(frame)?;
        let mut style = layer.text_style();
        typography.apply_to_style(&mut style);
        Some(Self::shape(
            text,
            typography.font_size,
            layer.width(),
            &style,
        ))
    }
    pub fn new(session: &Session) -> Arc<Self> {
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
        for (line_index, flow) in lines.iter().enumerate() {
            let base = flow.range.start;
            let line = &text[flow.range.clone()];
            let drawn = &text[base..flow.visible_end];
            let y = line_index as f64 * font_size * style.leading;
            let clusters = metrics::clusters(drawn, font_size, width, style).unwrap_or_default();
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
                    let a = cluster.x + (cluster.end - cluster.x) * position / count;
                    let b = cluster.x + (cluster.end - cluster.x) * (position + 1.0) / count;
                    let (old_a, old_b) = regions[index].unwrap_or((a.min(b), a.max(b)));
                    regions[index] = Some((old_a.min(a.min(b)), old_b.max(a.max(b))));
                }
            }
            let mut last = match style.align {
                libre_effects_core::TextAlign::Left => 0.0,
                libre_effects_core::TextAlign::Center => width / 2.0,
                libre_effects_core::TextAlign::Right => width,
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
                });
                last = x2;
            }
        }
        result
    }
    pub fn contains(&self, p: [f64; 2]) -> bool {
        self.cells.iter().any(|c| {
            p[0] >= c.x1.min(c.x2)
                && p[0] <= c.x1.max(c.x2)
                && p[1] >= c.y
                && p[1] <= c.y + self.line_height()
        })
    }
    /// Shared logical height for hit regions, selections, carets and IME anchors.
    pub fn line_height(&self) -> f64 {
        self.size * 1.2
    }
    pub fn caret_rect(&self, at: [f64; 2]) -> [f64; 4] {
        [at[0], at[1], 1.0, self.line_height()]
    }
    pub fn hit_character(&self, p: [f64; 2]) -> usize {
        self.cells
            .iter()
            .filter(|c| {
                p[0] >= c.x1.min(c.x2)
                    && p[0] <= c.x1.max(c.x2)
                    && p[1] >= c.y
                    && p[1] <= c.y + self.line_height()
            })
            .min_by(|a, b| {
                (a.y + self.size * 0.5 - p[1])
                    .abs()
                    .total_cmp(&(b.y + self.size * 0.5 - p[1]).abs())
            })
            .map_or_else(|| self.hit(p), |c| c.range.start)
    }
    pub fn caret(&self, at: usize) -> [f64; 2] {
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
                    ((q[1] + self.size * 0.5 - p[1]).abs() * 10000.0) + (q[0] - p[0]).abs()
                };
                d(a).total_cmp(&d(b))
            })
            .copied()
            .unwrap_or((0, [0.0; 2]))
    }
}
