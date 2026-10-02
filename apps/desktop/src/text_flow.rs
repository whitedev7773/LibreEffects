//! Source ranges for point text and Unicode-aware paragraph wrapping.
use crate::text_edit::layout::metrics;
use libre_effects_core::TextStyle;
use std::{cell::RefCell, ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub range: Range<usize>,
    pub visible_end: usize,
    pub bottom: f64,
    pub fits_width: bool,
}
pub(crate) fn composed_count(lines: &[Line], height: f64) -> usize {
    lines
        .iter()
        .take_while(|l| l.fits_width && l.bottom <= height + 0.001)
        .count()
}
pub(crate) fn convert(
    layer: &libre_effects_core::Layer,
    paragraph: bool,
) -> libre_effects_core::Command {
    use libre_effects_core::{Command, Content};
    let mut style = layer.text_style();
    let mut commands = vec![];
    if style.paragraph && !paragraph {
        if let Content::Text { text, font_size } = layer.content() {
            let flow = lines(text, *font_size, layer.width(), &style);
            let text = flow
                .iter()
                .take(composed_count(&flow, layer.height()))
                .map(|l| &text[l.range.start..l.visible_end])
                .collect::<Vec<_>>()
                .join("\n");
            commands.push(Command::SetContent {
                id: layer.id(),
                content: Content::Text {
                    text,
                    font_size: *font_size,
                },
            });
        }
    }
    style.paragraph = paragraph;
    commands.push(Command::SetTextStyle {
        id: layer.id(),
        style,
    });
    Command::Batch(commands)
}
struct Cache {
    text: String,
    size: f64,
    width: f64,
    style: TextStyle,
    lines: Arc<Vec<Line>>,
}
thread_local! {static CACHE:RefCell<Option<Cache>>=const {RefCell::new(None)};}
fn visible(text: &str) -> usize {
    text.trim_end_matches([' ', '\t', '\r', '\u{2028}', '\u{2029}'])
        .len()
}
fn measured(text: &str, size: f64, width: f64, style: &TextStyle) -> f64 {
    let left = TextStyle {
        align: libre_effects_core::TextAlign::Left,
        ..style.clone()
    };
    metrics::clusters(text, size, width, &left).map_or(f64::INFINITY, |c| {
        c.iter().map(|c| c.end).fold(0.0, f64::max) - c.iter().map(|c| c.x).fold(0.0, f64::min)
    })
}
pub(crate) fn lines(text: &str, size: f64, width: f64, style: &TextStyle) -> Arc<Vec<Line>> {
    CACHE.with(|cache| {
        if let Some(c) = &*cache.borrow() {
            if c.text == text && c.size == size && c.width == width && c.style == *style {
                return c.lines.clone();
            }
        }
        let mut result = vec![];
        let mut base = 0;
        for raw in text.split('\n') {
            let paragraph = raw.strip_suffix('\r').unwrap_or(raw);
            if !style.paragraph || paragraph.is_empty() {
                result.push(Line {
                    range: base..base + raw.len(),
                    visible_end: base + paragraph.len(),
                    bottom: 0.0,
                    fits_width: true,
                });
                base += raw.len() + 1;
                continue;
            }
            let glyphs = metrics::clusters(paragraph, size, width, style).unwrap_or_default();
            let mut ends: Vec<_> = paragraph
                .grapheme_indices(true)
                .map(|(i, g)| i + g.len())
                .collect();
            ends.insert(0, 0);
            let mut advances = vec![0.0; ends.len()];
            for c in glyphs {
                let a = ends
                    .partition_point(|n| *n <= c.range.start)
                    .saturating_sub(1);
                let b = ends.partition_point(|n| *n < c.range.end);
                let n = (b - a).max(1) as f64;
                for w in &mut advances[a + 1..=b] {
                    *w += (c.end - c.x).max(0.0) / n;
                }
            }
            for i in 1..advances.len() {
                advances[i] += advances[i - 1];
            }
            let breaks: Vec<_> = unicode_linebreak::linebreaks(paragraph)
                .filter(|(i, _)| ends.binary_search(i).is_ok())
                .collect();
            let mut start = 0;
            while start < paragraph.len() {
                let si = ends.binary_search(&start).unwrap();
                let limit = breaks
                    .iter()
                    .find(|(i, kind)| {
                        *i > start && *kind == unicode_linebreak::BreakOpportunity::Mandatory
                    })
                    .map_or(paragraph.len(), |(i, _)| *i);
                let opportunities: Vec<_> = breaks
                    .iter()
                    .map(|(i, _)| *i)
                    .filter(|i| *i > start && *i <= limit)
                    .collect();
                let estimate = |end: usize| {
                    let v = start + visible(&paragraph[start..end]);
                    let vi = ends.partition_point(|i| *i <= v).saturating_sub(1);
                    advances[vi] - advances[si]
                };
                let mut end = opportunities
                    .iter()
                    .copied()
                    .take_while(|end| estimate(*end) <= width)
                    .last();
                if end.is_none() {
                    end = ends
                        .iter()
                        .copied()
                        .skip(si + 1)
                        .take_while(|end| *end <= limit && estimate(*end) <= width)
                        .last();
                }
                let mut end = end.unwrap_or(ends[si + 1]);
                // Shaping a line on its own may choose a different fallback face
                // or boundary ligature. Verify the candidate with the renderer.
                while end > ends[si + 1]
                    && measured(
                        &paragraph[start..start + visible(&paragraph[start..end])],
                        size,
                        width,
                        style,
                    ) > width + 0.001
                {
                    end = opportunities
                        .iter()
                        .copied()
                        .filter(|n| *n < end)
                        .last()
                        .unwrap_or_else(|| ends[ends.binary_search(&end).unwrap() - 1]);
                }
                // Recover space when the line's actual font is narrower than the
                // estimate. Keep at least one grapheme for an over-wide glyph.
                for next in opportunities
                    .iter()
                    .copied()
                    .filter(|i| *i > end)
                    .collect::<Vec<_>>()
                {
                    if measured(
                        &paragraph[start..start + visible(&paragraph[start..next])],
                        size,
                        width,
                        style,
                    ) > width + 0.001
                    {
                        break;
                    }
                    end = next;
                }
                result.push(Line {
                    range: base + start..base + end,
                    visible_end: base + start + visible(&paragraph[start..end]),
                    bottom: 0.0,
                    fits_width: true,
                });
                start = end;
            }
            if let Some(last) = result.last_mut() {
                last.range.end = base + raw.len();
            }
            base += raw.len() + 1;
        }
        for (i, line) in result.iter_mut().enumerate() {
            if style.paragraph {
                let content = &text[line.range.start..line.visible_end];
                line.bottom = i as f64 * size * style.leading
                    + metrics::line_bottom(content, size, width, style);
                line.fits_width = measured(content, size, width, style) <= width + 0.001;
            }
        }
        let lines = Arc::new(result);
        *cache.borrow_mut() = Some(Cache {
            text: text.into(),
            size,
            width,
            style: style.clone(),
            lines: lines.clone(),
        });
        lines
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn point_conversion_keeps_composed_lines_and_undo_restores_overflow() {
        use libre_effects_core::{Content, Editor};
        let mut e = Editor::default();
        let mut draft =
            crate::text_edit::Session::new_box(e.project(), 0, 0, [20.0, 20.0, 200.0, 90.0])
                .unwrap();
        draft
            .buffer
            .replace(None, "one two three four five", false, None)
            .unwrap();
        e.execute(draft.command()).unwrap();
        let before = e.project().clone();
        let layer = e.selected_layer().unwrap();
        let flow = lines(&draft.buffer.text, 72.0, 200.0, &draft.style);
        let count = composed_count(&flow, 90.0);
        assert!(count > 0 && count < flow.len());
        let expected = flow
            .iter()
            .take(count)
            .map(|l| &draft.buffer.text[l.range.start..l.visible_end])
            .collect::<Vec<_>>()
            .join("\n");
        e.execute(convert(layer, false)).unwrap();
        assert!(!e.selected_layer().unwrap().text_style().paragraph);
        assert!(
            matches!(e.selected_layer().unwrap().content(),Content::Text{text,..} if text==&expected)
        );
        e.undo();
        assert_eq!(e.project(), &before);
        let too_wide = lines("W", 72.0, 1.0, &draft.style);
        assert_eq!(composed_count(&too_wide, 1000.0), 0);
    }
    #[test]
    fn paragraph_preview_output_and_saved_pixels_match_reflow_and_clip() {
        use libre_effects_core::{Command, Content, Editor, Project};
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Paragraph QA".into(),
            width: 480,
            height: 300,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        let mut draft =
            crate::text_edit::Session::new_box(e.project(), 0, 0, [20.0, 20.0, 180.0, 80.0])
                .unwrap();
        draft.font_size = 32.0;
        draft
            .buffer
            .replace(
                None,
                "한글 paragraph text wraps into a box across several lines",
                false,
                None,
            )
            .unwrap();
        e.execute(draft.command()).unwrap();
        let original = e.project().clone();
        let renderer = crate::rendering::Renderer::new();
        let pixels = renderer.render_preview(&original, 0, 480).unwrap();
        assert!(pixels.pixels().any(|p| p[3] > 0));
        for (x, y, p) in pixels.enumerate_pixels() {
            if x < 20 || x >= 200 || y < 20 || y >= 100 {
                assert_eq!(p[3], 0, "{x},{y}");
            }
        }
        let saved = Project::from_json(&original.to_json().unwrap()).unwrap();
        assert_eq!(renderer.render(&saved, 0, 480).unwrap(), pixels);
        let wrapped = lines(&draft.buffer.text, 32.0, 180.0, &draft.style)
            .iter()
            .map(|l| &draft.buffer.text[l.range.start..l.visible_end])
            .collect::<Vec<_>>()
            .join("\n");
        e.execute(Command::SetTextStyle {
            id: draft.id,
            style: TextStyle::default(),
        })
        .unwrap();
        e.execute(Command::SetContent {
            id: draft.id,
            content: Content::Text {
                text: wrapped,
                font_size: 32.0,
            },
        })
        .unwrap();
        let explicit = renderer.render(e.project(), 0, 480).unwrap();
        for y in 20..100 {
            for x in 20..200 {
                assert_eq!(pixels.get_pixel(x, y), explicit.get_pixel(x, y));
            }
        }
        e.replace_project(original.clone()).unwrap();
        e.execute(Command::SetTextBox {
            id: draft.id,
            width: 300.0,
            height: 240.0,
        })
        .unwrap();
        assert_ne!(renderer.render(e.project(), 0, 480).unwrap(), pixels);
        e.undo();
        assert_eq!(e.project(), &original);
    }
    #[test]
    fn paragraph_wraps_source_ranges_without_splitting_graphemes_or_losing_text() {
        let style = TextStyle {
            paragraph: true,
            ..Default::default()
        };
        for text in [
            "one two three four",
            "한글 문단 자동 줄바꿈",
            "👩‍💻👩‍💻 e\u{301}e\u{301}",
            "supercalifragilisticexpialidocious",
            "A  B\n\nC",
            "A\u{a0}B C",
            "A\u{2028}B",
        ] {
            let result = lines(text, 36.0, 100.0, &style);
            assert!(!result.is_empty());
            let boundaries: Vec<_> = text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect();
            for line in result.iter() {
                assert!(
                    boundaries.contains(&line.range.start) && boundaries.contains(&line.range.end)
                );
                let shown = &text[line.range.start..line.visible_end];
                assert!(
                    measured(shown, 36.0, 100.0, &style) <= 100.001
                        || shown.graphemes(true).count() <= 1,
                    "{shown:?}"
                );
            }
            let joined = result
                .iter()
                .map(|l| &text[l.range.clone()])
                .collect::<String>();
            assert_eq!(joined, text.replace('\n', ""));
            assert!(lines(text, 36.0, 60.0, &style).len() >= result.len());
        }
        let hard = lines("A\u{2028}B", 36.0, 1000.0, &style);
        assert_eq!(hard.len(), 2);
        assert_eq!(
            lines("one two three", 36.0, 40.0, &TextStyle::default()).len(),
            1
        );
        let tiny = lines("👩‍💻", 72.0, 1.0, &style);
        assert_eq!(tiny.len(), 1);
        assert_eq!(tiny[0].range, 0..11);
    }
}
