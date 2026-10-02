//! Grapheme hit regions in layer coordinates, shaped with the selected real face.
use super::Session;
use std::ops::Range;
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
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Layout {
    pub cells: Vec<Cell>,
    pub carets: Vec<(usize, [f64; 2])>,
    pub size: f64,
}
impl Layout {
    pub fn new(session: &Session) -> Self {
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
    ) -> Self {
        let db = crate::fonts::database();
        let selected = crate::fonts::matched(style);
        db.with_face_data(selected.id, |data, index| {
            let Some(face) = rustybuzz::Face::from_slice(data, index) else {
                return Self::default();
            };
            let scale = font_size / f64::from(face.units_per_em());
            let spacing = style.tracking * font_size / 1000.0;
            let mut result = Self {
                size: font_size,
                ..Default::default()
            };
            let mut base = 0;
            for (line_index, line) in text.split('\n').enumerate() {
                let y = line_index as f64 * font_size * style.leading;
                let bidi = unicode_bidi::BidiInfo::new(line, None);
                let mut cells = vec![];
                let mut x = 0.0;
                if let Some(para) = bidi.paragraphs.first() {
                    let (levels, runs) = bidi.visual_runs(para, 0..line.len());
                    for run in runs {
                        let rtl = levels[run.start].is_rtl();
                        let mut input = rustybuzz::UnicodeBuffer::new();
                        input.push_str(&line[run.clone()]);
                        input.guess_segment_properties();
                        input.set_direction(if rtl {
                            rustybuzz::Direction::RightToLeft
                        } else {
                            rustybuzz::Direction::LeftToRight
                        });
                        let shaped = rustybuzz::shape(&face, &[], input);
                        let mut starts: Vec<_> = shaped
                            .glyph_infos()
                            .iter()
                            .map(|g| g.cluster as usize)
                            .collect();
                        starts.push(run.len());
                        starts.sort_unstable();
                        starts.dedup();
                        let mut i = 0;
                        while i < shaped.len() {
                            let start = shaped.glyph_infos()[i].cluster as usize;
                            let end = starts
                                .iter()
                                .copied()
                                .find(|n| *n > start)
                                .unwrap_or(run.len());
                            let mut advance = 0.0;
                            while i < shaped.len()
                                && shaped.glyph_infos()[i].cluster as usize == start
                            {
                                advance += f64::from(shaped.glyph_positions()[i].x_advance) * scale;
                                i += 1;
                            }
                            let graphemes: Vec<_> = line[run.start + start..run.start + end]
                                .grapheme_indices(true)
                                .collect();
                            let count = graphemes.len().max(1);
                            advance += spacing;
                            for (j, (at, g)) in graphemes.iter().enumerate() {
                                let (a, b) = if rtl {
                                    ((count - j) as f64, (count - j - 1) as f64)
                                } else {
                                    (j as f64, (j + 1) as f64)
                                };
                                cells.push(Cell {
                                    range: base + run.start + start + at
                                        ..base + run.start + start + at + g.len(),
                                    x1: x + advance * a / count as f64,
                                    x2: x + advance * b / count as f64,
                                    y,
                                });
                            }
                            x += advance;
                        }
                    }
                }
                let offset = match style.align {
                    libre_effects_core::TextAlign::Left => 0.0,
                    libre_effects_core::TextAlign::Center => (width - x) / 2.0,
                    libre_effects_core::TextAlign::Right => width - x,
                };
                if cells.is_empty() {
                    result.carets.push((base, [offset, y]));
                }
                for mut cell in cells {
                    cell.x1 += offset;
                    cell.x2 += offset;
                    result.carets.push((cell.range.start, [cell.x1, y]));
                    result.carets.push((cell.range.end, [cell.x2, y]));
                    result.cells.push(cell);
                }
                base += line.len() + 1;
            }
            result
        })
        .unwrap_or_default()
    }
    pub fn contains(&self, p: [f64; 2]) -> bool {
        self.cells.iter().any(|c| {
            p[0] >= c.x1.min(c.x2)
                && p[0] <= c.x1.max(c.x2)
                && p[1] >= c.y
                && p[1] <= c.y + self.size * 1.2
        })
    }
    pub fn caret(&self, at: usize) -> [f64; 2] {
        self.carets
            .iter()
            .find(|(i, _)| *i == at)
            .map_or([0.0; 2], |(_, p)| *p)
    }
    pub fn hit(&self, p: [f64; 2]) -> usize {
        self.carets
            .iter()
            .min_by(|(_, a), (_, b)| {
                let d = |q: &[f64; 2]| {
                    ((q[1] + self.size * 0.5 - p[1]).abs() * 10000.0) + (q[0] - p[0]).abs()
                };
                d(a).total_cmp(&d(b))
            })
            .map_or(0, |(i, _)| *i)
    }
}
