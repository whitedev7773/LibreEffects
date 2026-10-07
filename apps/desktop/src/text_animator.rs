//! Weighted logical text units, joined to authoritative shaping cluster ranges.
//!
//! This module never shapes text or estimates glyph placement. Byte ranges must
//! come from the compositor that will paint the glyphs.
use libre_effects_core::{
    MAX_TEXT_RANGE_SELECTORS, TextAnimatorSample, TextSelectorMode, TextSelectorShape,
    TextSelectorUnits,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) struct Selection {
    source_len: usize,
    scalar_boundaries: Vec<usize>,
    graphemes: Vec<Range<usize>>,
    source_units: Vec<Range<usize>>,
    selected: Vec<bool>,
    weights: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ProtectedGlyph {
    /// Equal IDs identify one protected unit in this mapping operation.
    pub unit: usize,
    pub selected: bool,
    /// Maximum influence across the entire connected shaping/grapheme unit.
    pub weight: f64,
}

impl Selection {
    pub(crate) fn new(text: &str, sample: &TextAnimatorSample) -> Self {
        let graphemes: Vec<_> = text
            .grapheme_indices(true)
            .map(|(start, g)| start..start + g.len())
            .collect();
        // Only the primary selector defines authored Scale/Rotation units.
        // Secondary units affect influence, never transform groups or pivots.
        let units = source_units(text, &graphemes, sample.units);
        let mut weights = vec![0.0_f64; graphemes.len()];
        project_weights(&graphemes, &units, &mut weights, |center| {
            selector_weight(sample, center)
        });
        if !sample.selectors.is_empty() {
            let mut secondary = vec![0.0_f64; graphemes.len()];
            // Validated source bounds secondary evaluation to seven passes.
            // Combine at grapheme level BEFORE any shaping-cluster protection.
            debug_assert!(sample.selectors.len() <= MAX_TEXT_RANGE_SELECTORS);
            for selector in &sample.selectors {
                let units = source_units(text, &graphemes, selector.units);
                project_weights(&graphemes, &units, &mut secondary, |center| {
                    range_weight(
                        selector.start,
                        selector.end,
                        selector.amount,
                        selector.shape,
                        center,
                    )
                });
                for (accumulator, value) in weights.iter_mut().zip(&secondary) {
                    *accumulator = match selector.mode {
                        TextSelectorMode::Add => (*accumulator + value).clamp(0.0, 1.0),
                        TextSelectorMode::Subtract => (*accumulator - value).clamp(0.0, 1.0),
                        TextSelectorMode::Intersect => *accumulator * value,
                    };
                }
            }
        }
        let selected = weights.iter().map(|weight| *weight > 0.0).collect();
        Self {
            source_len: text.len(),
            scalar_boundaries: text
                .char_indices()
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect(),
            graphemes,
            source_units: units,
            selected,
            weights,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.selected.iter().any(|selected| *selected)
    }

    /// Join protected clusters by authored selector units, across all text nodes.
    /// These IDs/weights are exclusively for Scale/Rotation. Position and opacity
    /// keep the original protected-cluster weights and opacity paint grouping.
    pub(crate) fn transform_groups(
        &self,
        ranges: &[Range<usize>],
        protected: &[ProtectedGlyph],
    ) -> Option<Vec<ProtectedGlyph>> {
        if ranges.len() != protected.len() {
            return None;
        }
        let count = protected
            .iter()
            .map(|glyph| glyph.unit + 1)
            .max()
            .unwrap_or(0);
        let mut cluster_ranges: Vec<Option<Range<usize>>> = vec![None; count];
        for (range, glyph) in ranges.iter().zip(protected) {
            let start = self.graphemes.partition_point(|g| g.end <= range.start);
            let end = self.graphemes.partition_point(|g| g.start < range.end);
            if start >= end || end > self.graphemes.len() {
                return None;
            }
            let range = self.graphemes[start].start..self.graphemes[end - 1].end;
            if let Some(prior) = &mut cluster_ranges[glyph.unit] {
                prior.start = prior.start.min(range.start);
                prior.end = prior.end.max(range.end);
            } else {
                cluster_ranges[glyph.unit] = Some(range);
            }
        }
        fn root(parents: &mut [usize], index: usize) -> usize {
            let mut current = index;
            while parents[current] != current {
                parents[current] = parents[parents[current]];
                current = parents[current];
            }
            current
        }
        let mut parents: Vec<_> = (0..count + self.source_units.len()).collect();
        for (cluster, range) in cluster_ranges.iter().enumerate() {
            let Some(range) = range else { continue };
            let start = self
                .source_units
                .partition_point(|unit| unit.end <= range.start);
            let end = self
                .source_units
                .partition_point(|unit| unit.start < range.end);
            for unit in start..end {
                let left = root(&mut parents, cluster);
                let right = root(&mut parents, count + unit);
                // The smallest root makes the source-ordered ID deterministic.
                parents[left.max(right)] = left.min(right);
            }
        }
        let mut weights = vec![0.0_f64; parents.len()];
        for glyph in protected {
            let unit = root(&mut parents, glyph.unit);
            weights[unit] = weights[unit].max(glyph.weight);
        }
        Some(
            protected
                .iter()
                .map(|glyph| {
                    let unit = root(&mut parents, glyph.unit);
                    ProtectedGlyph {
                        unit,
                        selected: weights[unit] > 0.0,
                        weight: weights[unit],
                    }
                })
                .collect(),
        )
    }

    /// Assign every glyph to a protected unit, in the input glyph order.
    /// Ranges use original source bytes, not visual order or normalized text.
    /// Invalid/empty intervals fail explicitly instead of guessing a mapping.
    /// Complexity is O((graphemes + glyphs) log(graphemes + glyphs)); source text
    /// already has the project-wide 16,384-byte per-string limit.
    pub(crate) fn protect(&self, ranges: &[Range<usize>]) -> Option<Vec<ProtectedGlyph>> {
        let mut expanded = Vec::with_capacity(ranges.len());
        for (index, range) in ranges.iter().enumerate() {
            if range.start >= range.end
                || range.end > self.source_len
                || self.scalar_boundaries.binary_search(&range.start).is_err()
                || self.scalar_boundaries.binary_search(&range.end).is_err()
            {
                return None;
            }
            let start = self.graphemes.partition_point(|g| g.end <= range.start);
            let end = self.graphemes.partition_point(|g| g.start < range.end);
            if start >= end {
                return None;
            }
            expanded.push((start, end, index));
        }
        expanded.sort_unstable_by_key(|&(start, end, index)| (start, end, index));
        let mut units: Vec<Range<usize>> = Vec::new();
        let mut mapping = vec![0; ranges.len()];
        for (start, end, index) in expanded {
            if let Some(previous) = units.last_mut().filter(|last| start < last.end) {
                previous.end = previous.end.max(end);
            } else {
                units.push(start..end);
            }
            mapping[index] = units.len() - 1;
        }
        let weights: Vec<_> = units
            .iter()
            .map(|range| {
                self.weights[range.clone()]
                    .iter()
                    .copied()
                    .fold(0.0_f64, f64::max)
            })
            .collect();
        Some(
            mapping
                .into_iter()
                .map(|unit| ProtectedGlyph {
                    unit,
                    selected: weights[unit] > 0.0,
                    weight: weights[unit],
                })
                .collect(),
        )
    }
}

// Unit centers are in logical source order. UAX #29 words contain
// alphanumeric characters; punctuation-only spans, spaces and emoji do not
// consume a word index or receive word influence. Hard source lines split on
// CR, LF or CRLF (one break); blank and trailing empty lines count. Auto-wrapped
// visual lines never change these authored indices.
fn source_units(
    text: &str,
    graphemes: &[Range<usize>],
    units: TextSelectorUnits,
) -> Vec<Range<usize>> {
    match units {
        TextSelectorUnits::Graphemes => graphemes.to_vec(),
        TextSelectorUnits::Words => text
            .unicode_word_indices()
            .map(|(start, word)| start..start + word.len())
            .collect(),
        TextSelectorUnits::Lines => libre_effects_core::text_paragraphs::paragraphs(text)
            .map(|paragraph| paragraph.source_range())
            .collect(),
    }
}

fn project_weights(
    graphemes: &[Range<usize>],
    units: &[Range<usize>],
    weights: &mut [f64],
    weight_at: impl Fn(f64) -> f64,
) {
    weights.fill(0.0);
    for (index, range) in units.iter().enumerate() {
        let center = 100.0 * (index as f64 + 0.5) / units.len() as f64;
        let weight = weight_at(center);
        if weight == 0.0 || range.is_empty() {
            continue;
        }
        let start = graphemes.partition_point(|g| g.end <= range.start);
        let end = graphemes.partition_point(|g| g.start < range.end);
        for value in &mut weights[start..end] {
            *value = value.max(weight);
        }
    }
}

fn selector_weight(sample: &TextAnimatorSample, center: f64) -> f64 {
    range_weight(
        sample.start,
        sample.end,
        sample.amount,
        sample.shape,
        center,
    )
}

/// A bounded, half-open selector. Shape ramps are sampled at unit centers,
/// normalized over the effective range after Offset clipping. No wrap/reflow.
fn range_weight(start: f64, end: f64, amount: f64, shape: TextSelectorShape, center: f64) -> f64 {
    if start >= end || center < start || center >= end {
        return 0.0;
    }
    let t = (center - start) / (end - start);
    let shape = match shape {
        TextSelectorShape::Square => 1.0,
        TextSelectorShape::RampUp => t,
        TextSelectorShape::RampDown => 1.0 - t,
        TextSelectorShape::Triangle => 1.0 - (2.0 * t - 1.0).abs(),
    };
    (shape * amount / 100.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secondary(
        mode: TextSelectorMode,
        units: TextSelectorUnits,
        shape: TextSelectorShape,
        start: f64,
        end: f64,
        amount: f64,
    ) -> libre_effects_core::TextRangeSelectorSample {
        libre_effects_core::TextRangeSelectorSample {
            mode,
            units,
            shape,
            start,
            end,
            amount,
        }
    }

    #[test]
    fn ordered_selectors_clamp_each_step_and_do_not_commute() {
        use TextSelectorMode::{Add, Intersect, Subtract};
        let square = |mode, amount| {
            secondary(
                mode,
                TextSelectorUnits::Graphemes,
                TextSelectorShape::Square,
                0.,
                100.,
                amount,
            )
        };
        for (amount, selectors, expected) in [
            (75., vec![square(Add, 75.), square(Subtract, 50.)], 0.5),
            (75., vec![square(Subtract, 50.), square(Add, 75.)], 1.0),
            (0., vec![square(Subtract, 100.), square(Add, 50.)], 0.5),
            (0., vec![square(Add, 50.), square(Subtract, 100.)], 0.0),
            (50., vec![square(Add, 50.), square(Intersect, 50.)], 0.5),
            (50., vec![square(Intersect, 50.), square(Add, 50.)], 0.75),
        ] {
            let sample = TextAnimatorSample {
                amount,
                selectors,
                ..Default::default()
            };
            let before = sample.clone();
            assert_eq!(Selection::new("A", &sample).weights, [expected]);
            assert_eq!(sample, before, "evaluation is read-only");
        }
    }

    #[test]
    fn ordered_selectors_project_mixed_units_shapes_and_amounts_before_combining() {
        let sample = TextAnimatorSample {
            amount: 25.,
            selectors: vec![
                secondary(
                    TextSelectorMode::Add,
                    TextSelectorUnits::Words,
                    TextSelectorShape::Triangle,
                    0.,
                    100.,
                    50.,
                ),
                secondary(
                    TextSelectorMode::Intersect,
                    TextSelectorUnits::Lines,
                    TextSelectorShape::RampDown,
                    0.,
                    100.,
                    100.,
                ),
            ],
            ..Default::default()
        };
        assert_eq!(
            Selection::new("AB CD\n!", &sample).weights,
            [0.375, 0.375, 0.1875, 0.375, 0.375, 0.1875, 0.0625]
        );
        // Intersect must zero the punctuation/space gaps of its own word map;
        // they cannot retain a prior selector's scratch-buffer influence.
        let mut words = sample;
        words.selectors[1].units = TextSelectorUnits::Words;
        words.selectors[1].shape = TextSelectorShape::Square;
        assert_eq!(
            Selection::new("AB CD\n!", &words).weights,
            [0.5, 0.5, 0., 0.5, 0.5, 0., 0.]
        );
    }

    #[test]
    fn ordered_selectors_can_add_to_empty_primary_and_empty_word_domains() {
        for (start, end, amount) in [(50., 50., 100.), (80., 20., 100.), (0., 100., 0.)] {
            let sample = TextAnimatorSample {
                start,
                end,
                amount,
                units: TextSelectorUnits::Words,
                selectors: vec![secondary(
                    TextSelectorMode::Add,
                    TextSelectorUnits::Graphemes,
                    TextSelectorShape::Square,
                    0.,
                    100.,
                    50.,
                )],
                ..Default::default()
            };
            assert_eq!(Selection::new("👩‍💻!", &sample).weights, [0.5, 0.5]);
            assert!(Selection::new("", &sample).is_empty());
        }
    }

    #[test]
    fn ordered_selectors_combine_before_ligature_and_extended_grapheme_protection() {
        let sample = TextAnimatorSample {
            start: 16.,
            end: 34.,
            selectors: vec![secondary(
                TextSelectorMode::Intersect,
                TextSelectorUnits::Graphemes,
                TextSelectorShape::Square,
                50.,
                67.,
                100.,
            )],
            ..Default::default()
        };
        // The primary selects the first f and the secondary selects i. Both
        // touch the ffi cluster, but their per-grapheme intersection is empty.
        let selection = Selection::new("office", &sample);
        assert!(selection.is_empty());
        assert!(
            selection
                .protect(&[5..6, 4..5, 1..4, 0..1])
                .unwrap()
                .iter()
                .all(|glyph| !glyph.selected)
        );
        let mut subtract = sample;
        subtract.selectors[0].mode = TextSelectorMode::Subtract;
        let glyphs = Selection::new("office", &subtract)
            .protect(&[5..6, 4..5, 1..4, 0..1])
            .unwrap();
        assert_eq!(
            glyphs.iter().map(|g| g.weight).collect::<Vec<_>>(),
            [0., 0., 1., 0.]
        );
        let sample = TextAnimatorSample {
            amount: 0.,
            selectors: vec![secondary(
                TextSelectorMode::Add,
                TextSelectorUnits::Graphemes,
                TextSelectorShape::Square,
                25.,
                75.,
                50.,
            )],
            ..Default::default()
        };
        let selection = Selection::new("Ae\u{301}👩‍💻B", &sample);
        let glyphs = selection
            .protect(&[0..1, 1..2, 2..4, 4..8, 8..11, 11..15, 15..16])
            .unwrap();
        assert_eq!(
            glyphs.iter().map(|g| g.weight).collect::<Vec<_>>(),
            [0., 0.5, 0.5, 0.5, 0.5, 0.5, 0.]
        );
        assert_eq!(glyphs[1].unit, glyphs[2].unit);
        assert_eq!(glyphs[3].unit, glyphs[4].unit);
        assert_eq!(glyphs[4].unit, glyphs[5].unit);
    }

    #[test]
    fn ordered_selectors_keep_logical_rtl_newline_and_primary_transform_units() {
        let sample = TextAnimatorSample {
            amount: 0.,
            units: TextSelectorUnits::Lines,
            selectors: vec![secondary(
                TextSelectorMode::Add,
                TextSelectorUnits::Graphemes,
                TextSelectorShape::Square,
                0.,
                20.,
                50.,
            )],
            ..Default::default()
        };
        let selection = Selection::new("אב\r\nC\n", &sample);
        let ranges = [2..4, 0..2, 6..7];
        let glyphs = selection.protect(&ranges).unwrap();
        assert_eq!(
            glyphs.iter().map(|g| g.weight).collect::<Vec<_>>(),
            [0., 0.5, 0.]
        );
        let groups = selection.transform_groups(&ranges, &glyphs).unwrap();
        assert_eq!(
            groups.iter().map(|g| g.weight).collect::<Vec<_>>(),
            [0.5, 0.5, 0.]
        );
        assert_eq!(groups[0].unit, groups[1].unit);
        assert_ne!(groups[1].unit, groups[2].unit);
        let mut newline = sample;
        newline.selectors[0].start = 40.;
        newline.selectors[0].end = 60.;
        let selection = Selection::new("אב\r\nC\n", &newline);
        assert_eq!(selection.weights, [0., 0., 0.5, 0., 0.]);
        assert!(
            selection
                .protect(&ranges)
                .unwrap()
                .iter()
                .all(|g| !g.selected)
        );
    }

    fn selector(text: &str, start: f64, end: f64) -> Selection {
        Selection::new(
            text,
            &TextAnimatorSample {
                start,
                end,
                position: [13.0, -7.0],
                opacity: 50.0,
                ..Default::default()
            },
        )
    }

    #[test]
    fn hard_percent_range_has_literal_half_open_center_boundaries() {
        assert_eq!(
            selector("ABCD", 12.5, 62.5).selected,
            [true, true, false, false]
        );
        assert_eq!(
            selector("ABCD", 12.500001, 62.500001).selected,
            [false, true, true, false]
        );
        assert_eq!(selector("ABCD", 0.0, 100.0).selected, [true; 4]);
        assert!(selector("", 0.0, 100.0).is_empty());
        assert!(selector("ABCD", 50.0, 50.0).is_empty());
        assert!(selector("ABCD", 75.0, 25.0).is_empty());
    }

    #[test]
    fn extended_graphemes_keep_hangul_marks_zwj_flags_and_crlf_whole() {
        let text = "한e\u{301}👩‍💻🇰🇷\r\n \tA";
        let selection = selector(text, 0.0, 100.0);
        let actual: Vec<_> = selection
            .graphemes
            .iter()
            .map(|range| &text[range.clone()])
            .collect();
        assert_eq!(
            actual,
            ["한", "e\u{301}", "👩‍💻", "🇰🇷", "\r\n", " ", "\t", "A"]
        );
        assert_eq!(selection.selected, [true; 8]);
        let only_crlf = selector(text, 56.25, 68.75);
        assert_eq!(
            only_crlf.selected,
            [false, false, false, false, true, false, false, false]
        );
    }

    #[test]
    fn spaces_and_unpainted_newlines_consume_logical_indices() {
        let selection = selector("A \nB", 62.5, 87.5);
        assert_eq!(selection.selected, [false, false, true, false]);
        let glyphs = selection.protect(&[0..1, 3..4]).unwrap();
        assert_eq!(
            glyphs,
            [
                ProtectedGlyph {
                    unit: 0,
                    selected: false,
                    weight: 0.0
                },
                ProtectedGlyph {
                    unit: 1,
                    selected: false,
                    weight: 0.0
                }
            ]
        );
    }

    #[test]
    fn ligatures_expand_selection_and_preserve_visual_input_order() {
        let selection = selector("office", 40.0, 50.0);
        assert_eq!(
            selection.selected,
            [false, false, true, false, false, false]
        );
        let glyphs = selection.protect(&[5..6, 4..5, 1..4, 0..1]).unwrap();
        assert_eq!(
            glyphs,
            [
                ProtectedGlyph {
                    unit: 3,
                    selected: false,
                    weight: 0.0
                },
                ProtectedGlyph {
                    unit: 2,
                    selected: false,
                    weight: 0.0
                },
                ProtectedGlyph {
                    unit: 1,
                    selected: true,
                    weight: 1.0
                },
                ProtectedGlyph {
                    unit: 0,
                    selected: false,
                    weight: 0.0
                }
            ]
        );
    }

    #[test]
    fn fallback_components_share_a_grapheme_unit_and_join_transitively() {
        let selection = selector("A👩‍💻B", 40.0, 60.0);
        let glyphs = selection
            .protect(&[0..1, 1..5, 5..8, 8..12, 12..13])
            .unwrap();
        assert_eq!(
            glyphs,
            [
                ProtectedGlyph {
                    unit: 0,
                    selected: false,
                    weight: 0.0
                },
                ProtectedGlyph {
                    unit: 1,
                    selected: true,
                    weight: 1.0
                },
                ProtectedGlyph {
                    unit: 1,
                    selected: true,
                    weight: 1.0
                },
                ProtectedGlyph {
                    unit: 1,
                    selected: true,
                    weight: 1.0
                },
                ProtectedGlyph {
                    unit: 2,
                    selected: false,
                    weight: 0.0
                }
            ]
        );
        let chain = selector("ABCD", 0.0, 20.0)
            .protect(&[0..2, 1..3, 2..4])
            .unwrap();
        assert_eq!(
            chain,
            [ProtectedGlyph {
                unit: 0,
                selected: true,
                weight: 1.0
            }; 3]
        );
    }

    #[test]
    fn offset_selection_keeps_emoji_fallback_and_ligature_protection_unchanged() {
        use libre_effects_core::{Command, Content, Editor, TextParam, TrackEdit};
        for (text, end, offset, ranges, selected) in [
            (
                "A👩‍💻B",
                1.,
                50.,
                vec![0..1, 1..5, 5..8, 8..12, 12..13],
                vec![false, true, true, true, false],
            ),
            (
                "office",
                10.,
                40.,
                vec![5..6, 4..5, 1..4, 0..1],
                vec![false, false, true, false],
            ),
        ] {
            let mut editor = Editor::default();
            editor
                .execute(Command::AddContent {
                    content: Content::Text {
                        text: text.into(),
                        font_size: 40.,
                    },
                    width: 300.,
                    height: 180.,
                    name: "Offset selection".into(),
                })
                .unwrap();
            for (parameter, value) in [
                (TextParam::AnimatorEnd, end),
                (TextParam::AnimatorOffset, offset),
            ] {
                editor
                    .execute(Command::EditText {
                        id: 1,
                        parameter,
                        edit: TrackEdit::Value { frame: 0, value },
                    })
                    .unwrap();
            }
            let layer = editor.selected_layer().unwrap();
            let sample = layer.text_animator_at(0).unwrap();
            let selection = Selection::new(layer.source_text_at(0).unwrap(), &sample);
            let actual: Vec<_> = selection
                .protect(&ranges)
                .unwrap()
                .into_iter()
                .map(|glyph| glyph.selected)
                .collect();
            assert_eq!(actual, selected);
        }
    }

    #[test]
    fn offset_selects_logical_unicode_graphemes_and_counts_crlf_once() {
        use libre_effects_core::{Command, Content, Editor, TextParam, TrackEdit};
        let text = "한e\u{301}👩‍💻🇰🇷\r\n \tא";
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: text.into(),
                    font_size: 40.,
                },
                width: 300.,
                height: 180.,
                name: "Unicode Offset".into(),
            })
            .unwrap();
        for (parameter, value) in [
            (TextParam::AnimatorStart, 0.),
            (TextParam::AnimatorEnd, 12.5),
            (TextParam::AnimatorOffset, 50.),
        ] {
            editor
                .execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit: TrackEdit::Value { frame: 0, value },
                })
                .unwrap();
        }
        let before = editor.project().clone();
        let layer = editor.selected_layer().unwrap();
        let selection = Selection::new(
            layer.source_text_at(0).unwrap(),
            &layer.text_animator_at(0).unwrap(),
        );
        assert_eq!(
            selection.selected,
            [false, false, false, false, true, false, false, false]
        );
        assert_eq!(&text[selection.graphemes[4].clone()], "\r\n");
        // An unpainted selected CRLF never leaks onto adjacent visible units,
        // even when the authoritative glyph list arrives in visual order.
        let ranges = selection
            .graphemes
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 4)
            .map(|(_, range)| range.clone())
            .rev()
            .collect::<Vec<_>>();
        assert!(
            selection
                .protect(&ranges)
                .unwrap()
                .iter()
                .all(|glyph| !glyph.selected)
        );
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn missing_or_invalid_cluster_mapping_is_never_guessed() {
        let selection = selector("A", 0.0, 100.0);
        assert_eq!(selection.protect(&[]), Some(vec![]));
        assert_eq!(selection.protect(&[0..0]), None);
        assert_eq!(selection.protect(&[0..2]), None);
        assert_eq!(selection.protect(&[1..0]), None);
        assert_eq!(selector("", 0.0, 100.0).protect(&[0..1]), None);
        assert_eq!(selector("한", 0.0, 100.0).protect(&[0..1]), None);
    }

    #[test]
    fn transform_groups_join_whole_words_without_merging_opacity_units() {
        let selection = weighted(
            "ab cd",
            TextSelectorUnits::Words,
            TextSelectorShape::RampUp,
            100.0,
        );
        // Visual order and node boundaries do not define authored words.
        let ranges = [4..5, 1..2, 2..3, 3..4, 0..1];
        let protected = selection.protect(&ranges).unwrap();
        let groups = selection.transform_groups(&ranges, &protected).unwrap();
        assert_eq!(groups[0].unit, groups[3].unit);
        assert_eq!(groups[1].unit, groups[4].unit);
        assert_ne!(groups[1].unit, groups[0].unit);
        assert_eq!(
            groups.iter().map(|glyph| glyph.weight).collect::<Vec<_>>(),
            [0.75, 0.25, 0.0, 0.75, 0.25]
        );
        assert_ne!(protected[0].unit, protected[3].unit);
        assert_ne!(protected[1].unit, protected[4].unit);
    }

    #[test]
    fn transform_group_cluster_closure_merges_words_transitively_at_max_weight() {
        let selection = weighted(
            "ab cd ef",
            TextSelectorUnits::Words,
            TextSelectorShape::RampUp,
            100.0,
        );
        let ranges = [0..1, 1..4, 4..7, 7..8];
        let protected = selection.protect(&ranges).unwrap();
        let groups = selection.transform_groups(&ranges, &protected).unwrap();
        assert!(groups.iter().all(|glyph| glyph.unit == groups[0].unit));
        assert!(
            groups
                .iter()
                .all(|glyph| (glyph.weight - 5.0 / 6.0).abs() < 1e-12)
        );
        assert!(protected[0].weight < protected[3].weight);
        assert_ne!(protected[0].unit, protected[3].unit);
    }

    #[test]
    fn transform_groups_keep_grapheme_protection_and_authored_line_identity() {
        let ranges = [0..1, 1..4, 4..5, 6..7];
        let characters = weighted(
            "Ae\u{301}B\nC",
            TextSelectorUnits::Graphemes,
            TextSelectorShape::Square,
            100.0,
        );
        let protected = characters.protect(&ranges).unwrap();
        assert_eq!(
            characters.transform_groups(&ranges, &protected).unwrap(),
            protected
        );
        let lines = weighted(
            "Ae\u{301}B\nC",
            TextSelectorUnits::Lines,
            TextSelectorShape::Square,
            100.0,
        );
        let protected = lines.protect(&ranges).unwrap();
        let groups = lines.transform_groups(&ranges, &protected).unwrap();
        assert_eq!(groups[0].unit, groups[1].unit);
        assert_eq!(groups[0].unit, groups[2].unit);
        assert_ne!(groups[0].unit, groups[3].unit);
        assert!(lines.transform_groups(&ranges, &protected[..2]).is_none());
    }

    fn weighted(
        text: &str,
        units: TextSelectorUnits,
        shape: TextSelectorShape,
        amount: f64,
    ) -> Selection {
        Selection::new(
            text,
            &TextAnimatorSample {
                units,
                shape,
                amount,
                ..Default::default()
            },
        )
    }

    #[test]
    fn selector_shapes_sample_unit_centers_and_scale_amount() {
        for (shape, expected) in [
            (TextSelectorShape::Square, [1.0, 1.0, 1.0, 1.0]),
            (TextSelectorShape::RampUp, [0.125, 0.375, 0.625, 0.875]),
            (TextSelectorShape::RampDown, [0.875, 0.625, 0.375, 0.125]),
            (TextSelectorShape::Triangle, [0.25, 0.75, 0.75, 0.25]),
        ] {
            assert_eq!(
                weighted("ABCD", TextSelectorUnits::Graphemes, shape, 100.0).weights,
                expected
            );
            for (actual, expected) in weighted("ABCD", TextSelectorUnits::Graphemes, shape, 40.0)
                .weights
                .iter()
                .zip(expected)
            {
                assert!((actual - expected * 0.4).abs() < 1e-12);
            }
            assert!(weighted("ABCD", TextSelectorUnits::Graphemes, shape, 0.0).is_empty());
        }
    }

    #[test]
    fn ramps_use_effective_range_and_remain_half_open() {
        let sample = TextAnimatorSample {
            start: 25.0,
            end: 75.0,
            shape: TextSelectorShape::RampUp,
            ..Default::default()
        };
        assert_eq!(
            Selection::new("ABCD", &sample).weights,
            [0.0, 0.25, 0.75, 0.0]
        );
        assert_eq!(selector_weight(&sample, 25.0), 0.0);
        assert_eq!(selector_weight(&sample, 75.0), 0.0);
        assert_eq!(
            selector_weight(
                &TextAnimatorSample {
                    shape: TextSelectorShape::RampDown,
                    ..sample.clone()
                },
                25.0
            ),
            1.0
        );
        assert!(
            Selection::new(
                "ABCD",
                &TextAnimatorSample {
                    start: 80.0,
                    end: 20.0,
                    ..sample
                }
            )
            .is_empty()
        );
    }

    #[test]
    fn unicode_words_exclude_separators_and_keep_combining_and_korean_text_whole() {
        let text = "e\u{301} 한국어,42👩‍💻";
        let selection = weighted(
            text,
            TextSelectorUnits::Words,
            TextSelectorShape::RampUp,
            100.0,
        );
        let words = text.unicode_words().collect::<Vec<_>>();
        assert_eq!(words, ["e\u{301}", "한국어", "42"]);
        for (g, weight) in selection.graphemes.iter().zip(&selection.weights) {
            let source = &text[g.clone()];
            let expected = match source {
                "e\u{301}" => 1.0 / 6.0,
                "한" | "국" | "어" => 0.5,
                "4" | "2" => 5.0 / 6.0,
                _ => 0.0,
            };
            assert!((weight - expected).abs() < 1e-12, "{source:?}: {weight}");
        }
        assert!(
            weighted(
                " 👩‍💻,!?\n",
                TextSelectorUnits::Words,
                TextSelectorShape::Square,
                100.0
            )
            .is_empty()
        );
    }

    #[test]
    fn word_order_is_logical_and_punctuation_inside_a_unicode_word_is_included() {
        let text = "don't אבג";
        assert_eq!(text.unicode_words().collect::<Vec<_>>(), ["don't", "אבג"]);
        let selection = weighted(
            text,
            TextSelectorUnits::Words,
            TextSelectorShape::RampDown,
            100.0,
        );
        assert_eq!(
            selection.weights,
            [0.75, 0.75, 0.75, 0.75, 0.75, 0.0, 0.25, 0.25, 0.25]
        );
        let glyphs = selection.protect(&[10..12, 8..10, 6..8, 0..5]).unwrap();
        assert_eq!(
            glyphs.iter().map(|g| g.weight).collect::<Vec<_>>(),
            [0.25, 0.25, 0.25, 0.75]
        );
    }

    #[test]
    fn hard_lines_count_crlf_once_and_preserve_blank_and_trailing_indices() {
        let text = "A\r\n\nB\n";
        let selection = weighted(
            text,
            TextSelectorUnits::Lines,
            TextSelectorShape::RampUp,
            100.0,
        );
        assert_eq!(selection.weights, [0.125, 0.125, 0.375, 0.625, 0.625]);
        // A trailing empty fourth line has an index, but no glyph to affect.
        let trailing = Selection::new(
            text,
            &TextAnimatorSample {
                start: 75.0,
                units: TextSelectorUnits::Lines,
                ..Default::default()
            },
        );
        assert!(trailing.is_empty());
        assert_eq!(
            weighted(
                "A\rB\u{2028}C",
                TextSelectorUnits::Lines,
                TextSelectorShape::RampUp,
                100.0
            )
            .weights,
            [0.25, 0.25, 0.75, 0.75, 0.75]
        );
        for units in TextSelectorUnits::ALL {
            assert!(weighted("", units, TextSelectorShape::Triangle, 100.0).is_empty());
        }
    }

    #[test]
    fn mixed_hard_line_selectors_keep_original_unicode_source_units() {
        let text = "A\r日本\r\n한\n\rB\r\n";
        let sample = weighted(
            text,
            TextSelectorUnits::Lines,
            TextSelectorShape::RampUp,
            100.0,
        );
        let graphemes: Vec<_> = text
            .grapheme_indices(true)
            .map(|(at, g)| at..at + g.len())
            .collect();
        let units = source_units(text, &graphemes, TextSelectorUnits::Lines);
        assert_eq!(units, [0..2, 2..10, 10..14, 14..15, 15..18, 18..18]);
        for (index, range) in graphemes.iter().enumerate() {
            let line = units
                .iter()
                .position(|unit| unit.contains(&range.start))
                .unwrap();
            assert!((sample.weights[index] - (line as f64 + 0.5) / 6.0).abs() < 1e-12);
        }
    }

    #[test]
    fn fractional_selection_uses_max_weight_across_ligatures_and_fallback_components() {
        let selection = weighted(
            "office",
            TextSelectorUnits::Graphemes,
            TextSelectorShape::RampUp,
            100.0,
        );
        let glyphs = selection.protect(&[5..6, 4..5, 1..4, 0..1]).unwrap();
        for (glyph, expected) in glyphs
            .iter()
            .zip([11.0 / 12.0, 0.75, 7.0 / 12.0, 1.0 / 12.0])
        {
            assert!((glyph.weight - expected).abs() < 1e-12);
        }
        let emoji = weighted(
            "A👩‍💻B",
            TextSelectorUnits::Graphemes,
            TextSelectorShape::Triangle,
            50.0,
        );
        let glyphs = emoji.protect(&[0..1, 1..5, 5..8, 8..12, 12..13]).unwrap();
        assert_eq!(glyphs[1].weight, 0.5);
        assert_eq!(glyphs[2], glyphs[1]);
        assert_eq!(glyphs[3], glyphs[1]);
        let transitive = weighted(
            "ABCD",
            TextSelectorUnits::Graphemes,
            TextSelectorShape::RampUp,
            100.0,
        )
        .protect(&[0..2, 1..3, 2..4])
        .unwrap();
        assert!(transitive.iter().all(|g| g.weight == 0.875 && g.unit == 0));
    }
}
