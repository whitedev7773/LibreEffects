//! Recover logical text clusters from the compositor's positioned glyphs.
//! usvg exposes actual fallback faces and transforms, but not byte offsets. Shape
//! each used face only to recover those offsets; never use its guessed placement.
use libre_effects_core::TextStyle;
use resvg::usvg::{self, fontdb};
use std::{collections::HashMap, ops::Range};
use unicode_script::{Script, UnicodeScript};

// Match usvg 0.45's cursive-tracking policy when negative tracking hides a
// cluster entirely. The actual glyph positions still come from usvg.
fn spaced(c: char) -> bool {
    !matches!(
        c.script(),
        Script::Arabic
            | Script::Syriac
            | Script::Nko
            | Script::Manichaean
            | Script::Psalter_Pahlavi
            | Script::Mandaic
            | Script::Mongolian
            | Script::Phags_Pa
            | Script::Devanagari
            | Script::Bengali
            | Script::Gurmukhi
            | Script::Modi
            | Script::Sharada
            | Script::Syloti_Nagri
            | Script::Tirhuta
            | Script::Ogham
    )
}

pub(super) struct Cluster {
    pub range: Range<usize>,
    pub x: f64,
    pub end: f64,
}
struct Marker {
    id: u16,
    text: String,
    range: Range<usize>,
    offset: f32,
    width: f32,
}
fn markers(text: &str, font: fontdb::ID, size: f32, db: &fontdb::Database) -> Vec<Marker> {
    db.with_face_data(font, |data, index| {
        let Some(face) = rustybuzz::Face::from_slice(data, index) else {
            return vec![];
        };
        let scale = size / face.units_per_em() as f32;
        // This is the compositor's base direction, including paragraphs that
        // start with a RTL character but contain Latin numbers or punctuation.
        let bidi = unicode_bidi::BidiInfo::new(text, Some(unicode_bidi::Level::ltr()));
        let Some(para) = bidi.paragraphs.first() else {
            return vec![];
        };
        let (levels, runs) = bidi.visual_runs(para, para.range.clone());
        let mut result = vec![];
        for run in runs {
            let source = &text[run.clone()];
            let ltr = levels[run.start].is_ltr();
            let mut input = rustybuzz::UnicodeBuffer::new();
            input.push_str(source);
            input.set_direction(if ltr {
                rustybuzz::Direction::LeftToRight
            } else {
                rustybuzz::Direction::RightToLeft
            });
            let shaped = rustybuzz::shape(&face, &[], input);
            let infos = shaped.glyph_infos();
            let positions = shaped.glyph_positions();
            let mut starts: Vec<_> = infos.iter().map(|g| g.cluster as usize).collect();
            starts.push(source.len());
            starts.sort_unstable();
            starts.dedup();
            let mut previous = None;
            let mut offset = 0.0;
            for (i, (info, pos)) in infos.iter().zip(positions).enumerate() {
                let start = info.cluster as usize;
                if previous != Some(start) {
                    offset = 0.0;
                    previous = Some(start);
                }
                let end = starts
                    .get(starts.partition_point(|n| *n <= start))
                    .copied()
                    .unwrap_or(source.len());
                // PositionedGlyph.text is empty for intermediate glyphs in a
                // multi-glyph cluster. Match this metadata without splitting a
                // combining sequence into separate selection cells.
                let neighbor = if ltr {
                    i.checked_add(1)
                } else {
                    i.checked_sub(1)
                };
                let text_end = neighbor
                    .and_then(|i| infos.get(i))
                    .map_or(source.len(), |g| g.cluster as usize);
                result.push(Marker {
                    id: info.glyph_id as u16,
                    text: source.get(start..text_end).unwrap_or("").into(),
                    range: run.start + start..run.start + end,
                    offset: (offset + pos.x_offset as f32) * scale,
                    width: pos.x_advance as f32 * scale,
                });
                offset += pos.x_advance as f32;
            }
        }
        result
    })
    .unwrap_or_default()
}
fn find_text(group: &usvg::Group) -> Option<&usvg::Text> {
    group.children().iter().find_map(|node| match node {
        usvg::Node::Text(t) => Some(t.as_ref()),
        usvg::Node::Group(g) => find_text(g),
        _ => None,
    })
}
pub(super) fn clusters(
    text: &str,
    size: f64,
    width: f64,
    style: &TextStyle,
) -> Option<Vec<Cluster>> {
    if text.is_empty() {
        return Some(vec![]);
    }
    // SVG preserves spaces but normalizes a tab to one space. Both occupy one
    // UTF-8 byte, so original text offsets remain valid.
    let normalized = text.replace('\t', " ");
    let mut hidden = std::collections::HashSet::new();
    if style.tracking < 0.0 {
        let untracked = TextStyle {
            tracking: 0.0,
            ..style.clone()
        };
        let base = clusters(text, size, width, &untracked)?;
        for c in base.iter().take(base.len().saturating_sub(1)) {
            if normalized[c.range.start..]
                .chars()
                .next()
                .is_some_and(spaced)
                && c.end - c.x + style.tracking * size / 1000.0 <= 0.0
            {
                hidden.insert(c.range.start);
            }
        }
    }
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'>{}</svg>",
        width.max(1.0),
        (size * 2.0).max(1.0),
        crate::rendering::text_svg(&normalized, size, "white", width, style.clone())
    );
    let tree = usvg::Tree::from_str(&svg, &crate::fonts::render_options()).ok()?;
    let Some(node) = find_text(tree.root()) else {
        return Some(vec![]);
    };
    let mut fonts = HashMap::new();
    let mut cursor = 0;
    let mut result: Vec<Cluster> = vec![];
    for glyph in node.layouted().iter().flat_map(|s| &s.positioned_glyphs) {
        let candidates = fonts
            .entry(glyph.font)
            .or_insert_with(|| markers(&normalized, glyph.font, size as f32, tree.fontdb()));
        let (index, marker) = candidates.iter().enumerate().skip(cursor).find(|(_, m)| {
            !hidden.contains(&m.range.start) && m.id == glyph.id.0 && m.text == glyph.text
        })?;
        cursor = index + 1;
        let x = f64::from(glyph.transform().tx - marker.offset);
        if let Some(last) = result
            .last_mut()
            .filter(|c| c.range.start == marker.range.start)
        {
            last.range.end = last.range.end.max(marker.range.end);
            last.end = last.end.max(x + f64::from(marker.width));
        } else {
            result.push(Cluster {
                range: marker.range.clone(),
                x,
                end: x + f64::from(marker.width),
            });
        }
    }
    // Actual subsequent origins include kerning and script-sensitive tracking.
    // The last cluster has no trailing letter spacing in the SVG compositor.
    for i in 0..result.len().saturating_sub(1) {
        result[i].end = result[i + 1].x;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clusters_follow_actual_rendered_width_with_fallback_tracking_and_rtl() {
        let cases = [
            "AV 한글",
            "世界 title",
            "office ffi e\u{301}",
            "אבג 123 abc",
            "مرحبا بالعالم",
            "नमस्ते दुनिया",
            "👩‍💻 🇰🇷 🦀",
            "A\t  B",
            "A\u{200f}B",
        ];
        for family in ["Wanted Sans", "Arial"] {
            for tracking in [0.0, 150.0, -50.0] {
                for text in cases {
                    let style = TextStyle {
                        font_family: family.into(),
                        tracking,
                        ..Default::default()
                    };
                    let result = clusters(text, 72.0, 640.0, &style).unwrap_or_else(|| {
                        panic!("Unmapped glyphs: {family}, {tracking}, {text:?}")
                    });
                    assert!(!result.is_empty(), "{text:?}");
                    let svg = format!(
                        "<svg xmlns='http://www.w3.org/2000/svg' width='640' height='144'>{}</svg>",
                        crate::rendering::text_svg(text, 72.0, "white", 640.0, style)
                    );
                    let tree = usvg::Tree::from_str(&svg, &crate::fonts::render_options()).unwrap();
                    let node = find_text(tree.root()).unwrap();
                    let right = result
                        .iter()
                        .map(|c| c.end)
                        .fold(f64::NEG_INFINITY, f64::max);
                    assert!(
                        (right - f64::from(node.bounding_box().right())).abs() < 0.05,
                        "{family}, {tracking}, {text:?}: {right} vs {:?}",
                        node.bounding_box()
                    );
                    for c in result {
                        assert!(
                            text.is_char_boundary(c.range.start)
                                && text.is_char_boundary(c.range.end)
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn negative_tracking_maps_remaining_repeated_glyph_to_its_real_index() {
        let style = TextStyle {
            tracking: -1000.0,
            ..Default::default()
        };
        let result = clusters("AAAA", 72.0, 640.0, &style).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range, 3..4);
    }
}
