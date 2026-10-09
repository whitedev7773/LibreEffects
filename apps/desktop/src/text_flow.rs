//! Source ranges for point text and Unicode-aware paragraph wrapping.
use crate::text_metrics as metrics;
use libre_effects_core::TextStyle;
use std::{cell::RefCell, ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub(crate) struct Line {
    /// Original content bytes, excluding the hard paragraph terminator.
    pub range: Range<usize>,
    /// Complete original hard break on the final visual line; empty otherwise.
    pub terminator: Range<usize>,
    pub visible_end: usize,
    /// Line origin and available width in layer-local pixels.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub bottom: f64,
    /// Space after this hard-newline paragraph, used by Fit, not ink clipping.
    pub after: f64,
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
    frame: libre_effects_core::Frame,
) -> libre_effects_core::Command {
    use libre_effects_core::Command;
    let mut style = layer.text_style();
    let mut commands = vec![];
    if style.paragraph && !paragraph {
        if let Some(text) = layer.source_text_at(frame) {
            let flow = layer_lines(layer, frame).unwrap();
            let text = flow
                .iter()
                .take(composed_count(&flow, layer.height()))
                .map(|l| &text[l.range.start..l.visible_end])
                .collect::<Vec<_>>()
                .join("\n");
            commands.push(Command::EditSourceText {
                id: layer.id(),
                frame,
                text,
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
/// Shape the current frame without turning evaluated typography into edit data.
pub(crate) fn layer_lines(
    layer: &libre_effects_core::Layer,
    frame: libre_effects_core::Frame,
) -> Option<Arc<Vec<Line>>> {
    let text = layer.source_text_at(frame)?;
    let typography = layer.text_typography_at(frame)?;
    let mut style = layer.text_style();
    typography.apply_to_style(&mut style);
    if let Some(rich) = layer.rich_text() {
        let composed = crate::rich_text_render::compose(text, rich, layer.width(), &style).ok()?;
        return Some(Arc::new(
            composed
                .lines
                .iter()
                .map(|line| Line {
                    range: line.range.clone(),
                    terminator: line.terminator.clone(),
                    visible_end: line.range.end,
                    x: 0.0,
                    y: line.y,
                    width: layer.width(),
                    bottom: line.y + line.size * 1.2,
                    after: 0.0,
                    fits_width: true,
                })
                .collect(),
        ));
    }
    Some(lines(text, typography.font_size, layer.width(), &style))
}
pub(crate) fn fit_height(
    layer: &libre_effects_core::Layer,
    frame: libre_effects_core::Frame,
) -> Option<f64> {
    layer.text_style().paragraph.then_some(())?;
    Some(
        layer_lines(layer, frame)?
            .iter()
            .map(|line| line.bottom + line.after)
            .fold(1.0, f64::max),
    )
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
    text.trim_end_matches([' ', '\t', '\u{2028}', '\u{2029}'])
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
        let mut paragraph_spacing = 0.0;
        for source in libre_effects_core::text_paragraphs::paragraphs(text) {
            let base = source.range.start;
            let paragraph = source.text;
            let line_geometry = |first: bool| {
                if style.paragraph {
                    let x = style.paragraph_left_indent
                        + if first {
                            style.paragraph_first_line_indent
                        } else {
                            0.0
                        };
                    (x, width - style.paragraph_right_indent - x)
                } else {
                    (0.0, width)
                }
            };
            if style.paragraph {
                paragraph_spacing += style.paragraph_space_before;
            }
            let (x, available) = line_geometry(true);
            // An exhausted first-line interval is overflow, not a tiny forced
            // line. Keep its full source range for editing and conversion.
            if !style.paragraph || paragraph.is_empty() || available <= 0.0 {
                result.push(Line {
                    range: source.range.clone(),
                    terminator: source.terminator.clone(),
                    visible_end: base + paragraph.len(),
                    x,
                    y: result.len() as f64 * size * style.leading + paragraph_spacing,
                    width: available,
                    bottom: 0.0,
                    after: if style.paragraph {
                        style.paragraph_space_after
                    } else {
                        0.0
                    },
                    fits_width: !style.paragraph || available > 0.0,
                });
                if style.paragraph {
                    paragraph_spacing += style.paragraph_space_after;
                }
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
                let (x, available) = line_geometry(start == 0);
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
                    .take_while(|end| estimate(*end) <= available)
                    .last();
                if end.is_none() {
                    end = ends
                        .iter()
                        .copied()
                        .skip(si + 1)
                        .take_while(|end| *end <= limit && estimate(*end) <= available)
                        .last();
                }
                let mut end = end.unwrap_or(ends[si + 1]);
                // Shaping a line on its own may choose a different fallback face
                // or boundary ligature. Verify the candidate with the renderer.
                while end > ends[si + 1]
                    && measured(
                        &paragraph[start..start + visible(&paragraph[start..end])],
                        size,
                        available,
                        style,
                    ) > available + 0.001
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
                        available,
                        style,
                    ) > available + 0.001
                    {
                        break;
                    }
                    end = next;
                }
                result.push(Line {
                    range: base + start..base + end,
                    terminator: base + end..base + end,
                    visible_end: base + start + visible(&paragraph[start..end]),
                    x,
                    y: result.len() as f64 * size * style.leading + paragraph_spacing,
                    width: available,
                    bottom: 0.0,
                    after: 0.0,
                    fits_width: available > 0.0,
                });
                start = end;
            }
            if let Some(last) = result.last_mut() {
                last.terminator = source.terminator;
                last.after = style.paragraph_space_after;
            }
            paragraph_spacing += style.paragraph_space_after;
        }
        for line in &mut result {
            if style.paragraph {
                let content = &text[line.range.start..line.visible_end];
                line.bottom = line.y + metrics::line_bottom(content, size, line.width, style);
                line.fits_width &= measured(content, size, line.width, style) <= line.width + 0.001;
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
    fn mixed_hard_paragraphs_share_original_ranges_and_svg_nodes() {
        use libre_effects_core::text_paragraphs::paragraphs;
        for text in ["English\r日本語\r\n한국어\n\r끝\r\n", "\r\r\n\n", ""] {
            for paragraph in [false, true] {
                let style = TextStyle {
                    paragraph,
                    ..Default::default()
                };
                let flow = lines(text, 24.0, 1000.0, &style);
                let source: Vec<_> = paragraphs(text).collect();
                assert_eq!(flow.len(), source.len());
                for (index, (line, source)) in flow.iter().zip(&source).enumerate() {
                    assert_eq!(line.range, source.range);
                    assert_eq!(line.terminator, source.terminator);
                    assert_eq!(line.visible_end, source.range.end);
                    assert_eq!(line.y, index as f64 * 24.0 * style.leading);
                }
                let svg = crate::rendering::text_geometry_svg(
                    text, 24.0, "white", 1000.0, 10000.0, &style,
                );
                assert_eq!(svg.matches("<text ").count(), flow.len());
                let tagged = crate::text_animator_render::source_geometry_svg(
                    text, 24.0, "white", 1000.0, 10000.0, &style,
                )
                .unwrap();
                assert_eq!(tagged.matches("<text ").count(), flow.len());
                for source in &source {
                    assert!(tagged.contains(&format!("id='le-animator-{}'", source.range.start)));
                }
                let canonical = source.iter().map(|p| p.text).collect::<Vec<_>>().join("\n");
                assert_eq!(
                    svg,
                    crate::rendering::text_geometry_svg(
                        &canonical, 24.0, "white", 1000.0, 10000.0, &style,
                    )
                );
            }
        }
    }

    #[test]
    fn wrapped_mixed_paragraphs_partition_source_without_normalizing_breaks() {
        let text = "English words wrap\r日本語の段落\r\n한국어 문단\n\r끝\r\n";
        let style = TextStyle {
            paragraph: true,
            ..Default::default()
        };
        let flow = lines(text, 24.0, 65.0, &style);
        let mut source = String::new();
        let boundaries: Vec<_> = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        for line in flow.iter() {
            assert!(boundaries.contains(&line.range.start));
            assert!(boundaries.contains(&line.range.end));
            assert!(boundaries.contains(&line.terminator.end));
            assert_eq!(line.range.end, line.terminator.start);
            assert!(line.visible_end <= line.range.end);
            source.push_str(&text[line.range.clone()]);
            source.push_str(&text[line.terminator.clone()]);
        }
        assert_eq!(source, text);
        assert_eq!(
            flow.iter()
                .filter(|line| !line.terminator.is_empty())
                .count(),
            5
        );
        assert_eq!(flow.last().unwrap().range, text.len()..text.len());
        let svg = crate::rendering::text_geometry_svg(text, 24.0, "white", 65.0, 10000.0, &style);
        assert_eq!(svg.matches("<text ").count(), flow.len());
    }

    #[test]
    fn paragraph_offsets_blank_lines_and_trailing_space_share_one_geometry() {
        let style = TextStyle {
            paragraph: true,
            leading: 1.5,
            paragraph_left_indent: 20.0,
            paragraph_right_indent: 20.0,
            paragraph_first_line_indent: 9.0,
            paragraph_space_before: 7.0,
            paragraph_space_after: 11.0,
            ..Default::default()
        };
        let flow = lines("A\n\nB\n", 36.0, 220.0, &style);
        assert_eq!(flow.len(), 4);
        for (line, y) in flow.iter().zip([7.0, 79.0, 151.0, 223.0]) {
            assert_eq!(
                (line.x, line.width, line.y, line.after),
                (29.0, 171.0, y, 11.0)
            );
            assert!(line.fits_width);
        }
        let impossible = lines("source remains\nnext", 36.0, 20.0, &style);
        assert_eq!(impossible[0].range, 0..14);
        assert!(!impossible[0].fits_width);
        assert_eq!(composed_count(&impossible, 10000.0), 0);
        let point = TextStyle {
            paragraph: false,
            ..style
        };
        let dormant = lines("A\nB", 36.0, 220.0, &point);
        assert_eq!(
            (
                dormant[0].x,
                dormant[0].y,
                dormant[0].width,
                dormant[0].after
            ),
            (0.0, 0.0, 220.0, 0.0)
        );
        assert_eq!(dormant[1].y, 54.0);
    }
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
        e.execute(convert(layer, false, 0)).unwrap();
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
