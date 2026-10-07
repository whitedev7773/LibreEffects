//! Caller-triggered, bounded inspection of the renderer's final positioned glyphs.
//!
//! This is paint-independent shaping: hidden layers and disabled Fill/Stroke are
//! examined once at a captured composition-local frame, using the same composed
//! point/paragraph geometry as rendering. This never checks a whole animation.
//! Overflow glyphs are not inspected. A zero glyph-ID-0 count does not guarantee
//! emoji/variation support or of color/bitmap glyph painting. No source ranges or
//! caret positions are inferred from usvg's possibly-empty cluster fragments.
use libre_effects_core::{Content, Frame, Layer, text_paragraphs::paragraphs};
use resvg::usvg::{self, fontdb};
use std::collections::{BTreeMap, HashMap};

pub(crate) const MAX_SOURCE_BYTES: usize = 4096;
pub(crate) const MAX_SOURCE_LINES: usize = 128;
pub(crate) const MAX_GLYPHS: usize = 8192;
pub(crate) const MAX_FACES: usize = 16;
pub(crate) const MAX_SAMPLES: usize = 8;
pub(crate) const MAX_SAMPLE_CHARS: usize = 24;
const MAX_FONT_NAME_CHARS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Face {
    pub family: String,
    pub face: String,
    pub weight: u16,
    pub italic: bool,
}
impl Face {
    fn from_info(info: &fontdb::FaceInfo) -> Self {
        Self {
            family: info
                .families
                .first()
                .map_or_else(String::new, |(name, _)| bounded(name, MAX_FONT_NAME_CHARS)),
            face: bounded(&info.post_script_name, MAX_FONT_NAME_CHARS),
            weight: info.weight.0,
            italic: info.style != fontdb::Style::Normal,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FaceUsage {
    pub font: Face,
    pub glyphs: usize,
    /// Compare IDs inside the parsed tree, not potentially duplicate face names.
    pub is_primary: bool,
    pub is_fallback: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sample {
    /// Final glyph cluster metadata, not an exact source range. May be empty.
    pub text: String,
    pub code_points: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Complete,
    Empty,
    Unsupported,
    Incomplete(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    /// Composition-local frame whose source and typography produced these glyphs.
    pub checked_frame: Frame,
    pub primary: Option<Face>,
    pub faces: Vec<FaceUsage>,
    pub unresolved_glyphs: usize,
    pub samples: Vec<Sample>,
    pub glyphs: usize,
    pub composed_lines: usize,
    pub overflow_lines: usize,
    pub status: Status,
    /// Output examples/faces or the input/inspection budget were bounded.
    pub truncated: bool,
}
impl Report {
    fn new(primary: Option<Face>, checked_frame: Frame) -> Self {
        Self {
            checked_frame,
            primary,
            faces: vec![],
            unresolved_glyphs: 0,
            samples: vec![],
            glyphs: 0,
            composed_lines: 0,
            overflow_lines: 0,
            status: Status::Complete,
            truncated: false,
        }
    }
}
fn bounded(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// Read-only one-layer operation. Callers can cancel between bounded layers.
/// It is intentionally absent from project Open, preflight and frame rendering.
pub(crate) fn analyze(layer: &Layer, frame: Frame) -> Report {
    let primary = matches!(layer.content(), Content::Text { .. })
        .then(|| crate::fonts::matched(&layer.text_style()));
    let mut report = analyze_with_options(layer, frame, &crate::fonts::render_options(), primary);
    report.truncated |= primary.is_some_and(font_name_limited);
    report
}
fn font_name_limited(info: &fontdb::FaceInfo) -> bool {
    info.post_script_name.chars().count() > MAX_FONT_NAME_CHARS
        || info
            .families
            .first()
            .is_some_and(|(name, _)| name.chars().count() > MAX_FONT_NAME_CHARS)
}

fn analyze_with_options(
    layer: &Layer,
    frame: Frame,
    options: &usvg::Options<'_>,
    primary: Option<&fontdb::FaceInfo>,
) -> Report {
    let primary_id = primary.map(|face| face.id);
    let mut report = Report::new(primary.map(Face::from_info), frame);
    if layer.rich_text().is_some() {
        report.status =
            Status::Incomplete("Rich-run font coverage analysis is not yet supported".into());
        return report;
    }
    let Some(text) = layer.source_text_at(frame) else {
        report.status = Status::Unsupported;
        return report;
    };
    // Refuse the whole layer rather than chopping through a cluster/run, which
    // could change both fallback choice and paragraph composition.
    if text.len() > MAX_SOURCE_BYTES
        || paragraphs(text).take(MAX_SOURCE_LINES + 1).count() > MAX_SOURCE_LINES
    {
        report.status = Status::Incomplete("Layer exceeds the text analysis limit".into());
        report.truncated = true;
        return report;
    }
    let mut style = layer.text_style();
    // Sample only into temporary geometry inputs. Font identity and all stored
    // source/style values remain untouched, including existing typography keys.
    let typography = layer
        .text_typography_at(frame)
        .expect("Text content checked");
    typography.apply_to_style(&mut style);
    let font_size = typography.font_size;
    let mut expected = BTreeMap::new();
    let mut expected_nodes = 0;
    let mut add_line = |line: &str| {
        // Match text_svg's hard paragraphs. The SVG parser drops empty text
        // nodes, though those paragraphs still count toward logical line count.
        for paragraph in paragraphs(line) {
            let line = paragraph.text;
            if !line.is_empty() {
                expected_nodes += 1;
            }
            for c in line.chars() {
                *expected.entry(normalized(c)).or_insert(0usize) += 1;
            }
        }
    };
    if style.paragraph {
        let lines = crate::text_flow::lines(text, font_size, layer.width(), &style);
        report.composed_lines = crate::text_flow::composed_count(&lines, layer.height());
        report.overflow_lines = lines.len() - report.composed_lines;
        // Wrapping can create more lines than explicit source newlines.
        if lines.len() > MAX_SOURCE_LINES {
            report.status = Status::Incomplete("Paragraph exceeds the line analysis limit".into());
            report.truncated = true;
            return report;
        }
        for line in lines.iter().take(report.composed_lines) {
            add_line(&text[line.range.start..line.visible_end]);
        }
    } else {
        report.composed_lines = paragraphs(text).count();
        add_line(text);
    }
    // An opaque single fill keeps glyph metadata available regardless of source
    // paint switches. This calls the same helper as the renderer's paint passes.
    let geometry = crate::rendering::text_geometry_svg(
        text,
        font_size,
        "white",
        layer.width(),
        layer.height(),
        &style,
    );
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'>{geometry}</svg>",
        layer.width().max(1.0),
        layer.height().max(1.0),
    );
    inspect_svg(
        &source,
        options,
        expected,
        expected_nodes,
        primary_id,
        report,
    )
}

fn normalized(c: char) -> char {
    // XML/SVG normalizes tabs and embedded CR/newlines to one space even with
    // xml:space=preserve. Line separators are already split by text_svg.
    if matches!(c, '\t' | '\r' | '\n') {
        ' '
    } else {
        c
    }
}

fn inspect_svg(
    source: &str,
    options: &usvg::Options<'_>,
    expected: BTreeMap<char, usize>,
    expected_nodes: usize,
    primary_id: Option<fontdb::ID>,
    mut report: Report,
) -> Report {
    let tree = match usvg::Tree::from_str(source, options) {
        Ok(tree) => tree,
        Err(_) => {
            report.status = Status::Incomplete("Text geometry could not be parsed".into());
            return report;
        }
    };
    struct Inspection {
        fonts: HashMap<fontdb::ID, usize>,
        represented: BTreeMap<char, usize>,
        nodes: usize,
        limited: bool,
    }
    fn visit(group: &usvg::Group, report: &mut Report, inspection: &mut Inspection) {
        for node in group.children() {
            match node {
                usvg::Node::Group(group) => visit(group, report, inspection),
                usvg::Node::Text(text) => {
                    inspection.nodes += 1;
                    for glyph in text
                        .layouted()
                        .iter()
                        .flat_map(|span| &span.positioned_glyphs)
                    {
                        if report.glyphs == MAX_GLYPHS {
                            inspection.limited = true;
                            return;
                        }
                        report.glyphs += 1;
                        *inspection.fonts.entry(glyph.font).or_default() += 1;
                        for c in glyph.text.chars() {
                            *inspection.represented.entry(c).or_default() += 1;
                        }
                        if glyph.id.0 == 0 {
                            report.unresolved_glyphs += 1;
                            let text = bounded(&glyph.text, MAX_SAMPLE_CHARS);
                            report.truncated |= text.len() != glyph.text.len();
                            let sample = Sample {
                                code_points: text.chars().map(u32::from).collect(),
                                text,
                            };
                            if !report.samples.contains(&sample) {
                                if report.samples.len() < MAX_SAMPLES {
                                    report.samples.push(sample);
                                } else {
                                    report.truncated = true;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            if inspection.limited {
                return;
            }
        }
        // Never recurse Text::flattened() or paint subroots: they are outlines
        // of glyphs already inspected, not independent source text.
    }
    let mut inspection = Inspection {
        fonts: HashMap::new(),
        represented: BTreeMap::new(),
        nodes: 0,
        limited: false,
    };
    visit(tree.root(), &mut report, &mut inspection);
    let mut reasons = vec![];
    if inspection.limited {
        report.truncated = true;
        reasons.push("Positioned-glyph analysis limit reached");
    }
    for (id, glyphs) in inspection.fonts {
        // IDs belong to this tree's database. Never resolve these against the
        // initial global catalog: a resolver may have added fonts during parse.
        if let Some(info) = tree.fontdb().face(id) {
            report.truncated |= font_name_limited(info);
            report.faces.push(FaceUsage {
                font: Face::from_info(info),
                glyphs,
                is_primary: primary_id == Some(id),
                is_fallback: primary_id.is_some_and(|primary| primary != id),
            });
        } else if !reasons.contains(&"A positioned font could not be identified") {
            reasons.push("A positioned font could not be identified");
        }
    }
    report.faces.sort_by(|a, b| {
        (&a.font, a.is_primary, a.is_fallback, a.glyphs).cmp(&(
            &b.font,
            b.is_primary,
            b.is_fallback,
            b.glyphs,
        ))
    });
    if report.faces.len() > MAX_FACES {
        report.faces.truncate(MAX_FACES);
        report.truncated = true;
    }
    if report.overflow_lines != 0 {
        reasons.push("Hidden paragraph overflow was not examined");
    }
    if inspection.nodes != expected_nodes || inspection.represented != expected {
        reasons.push("Some source fragments have no final positioned glyph metadata");
    }
    report.status = if !reasons.is_empty() {
        Status::Incomplete(reasons.join("; "))
    } else if expected.is_empty() {
        Status::Empty
    } else {
        Status::Complete
    };
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor, TextParam, TextStyle, TrackEdit};
    use std::sync::Arc;

    const FIXTURE: &[u8] =
        include_bytes!("../assets/fonts/test-fixtures/CoverageFixture-Regular.ttf");
    const WANTED: &[u8] = include_bytes!("../assets/fonts/WantedSans-Regular.ttf");

    fn editor(text: &str, style: TextStyle, width: f64, height: f64) -> Editor {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: text.into(),
                    font_size: 48.0,
                },
                width,
                height,
                name: "Coverage".into(),
            })
            .unwrap();
        editor
            .execute(Command::SetTextStyle {
                id: editor.selected().unwrap(),
                style,
            })
            .unwrap();
        editor
    }
    fn isolated(
        fixture: bool,
        fallback: bool,
    ) -> (usvg::Options<'static>, fontdb::ID, Option<fontdb::ID>) {
        let mut db = fontdb::Database::new();
        db.load_font_data(if fixture { FIXTURE } else { WANTED }.to_vec());
        let primary = db.faces().next().unwrap().id;
        let fallback_id = if fallback {
            db.load_font_data(WANTED.to_vec());
            Some(db.faces().last().unwrap().id)
        } else {
            None
        };
        let options = usvg::Options {
            fontdb: Arc::new(db),
            // text_geometry_svg uses production aliases. This controlled selector
            // intentionally resolves those aliases to the isolated fixture ID.
            font_resolver: usvg::FontResolver {
                select_font: Box::new(move |_, db| {
                    assert!(db.face(primary).is_some());
                    Some(primary)
                }),
                select_fallback: usvg::FontResolver::default_fallback_selector(),
            },
            ..Default::default()
        };
        (options, primary, fallback_id)
    }
    fn inspect(layer: &Layer, options: &usvg::Options<'_>, primary: fontdb::ID) -> Report {
        analyze_with_options(layer, 0, options, options.fontdb.face(primary))
    }
    fn tree_ids(group: &usvg::Group, ids: &mut Vec<(fontdb::ID, u16, String)>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(group) => tree_ids(group, ids),
                usvg::Node::Text(text) => ids.extend(
                    text.layouted()
                        .iter()
                        .flat_map(|span| &span.positioned_glyphs)
                        .map(|glyph| (glyph.font, glyph.id.0, glyph.text.clone())),
                ),
                _ => {}
            }
        }
    }
    fn shape_ids(text: &str, options: &usvg::Options<'_>) -> Vec<(fontdb::ID, u16, String)> {
        let geometry = crate::rendering::text_geometry_svg(
            text,
            48.0,
            "white",
            600.0,
            200.0,
            &TextStyle::default(),
        );
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='600' height='200'>{geometry}</svg>"
        );
        let tree = usvg::Tree::from_str(&source, options).unwrap();
        let mut ids = vec![];
        tree_ids(tree.root(), &mut ids);
        ids
    }

    #[test]
    fn primary_and_successful_whole_run_fallback_use_final_tree_ids() {
        let (options, primary, fallback) = isolated(true, true);
        let fallback = fallback.unwrap();
        assert_ne!(primary, fallback);
        let primary_ids = shape_ids("AB", &options);
        assert_eq!(primary_ids.len(), 2);
        assert!(primary_ids.iter().all(|g| g.0 == primary && g.1 != 0));
        let e = editor("AB", TextStyle::default(), 600.0, 200.0);
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(report.status, Status::Complete);
        assert_eq!(report.unresolved_glyphs, 0);
        assert_eq!(report.faces.len(), 1);
        assert_eq!(report.faces[0].font, report.primary.clone().unwrap());
        assert_eq!(report.faces[0].font.family, "LibreEffects Coverage Fixture");
        assert!(report.faces[0].is_primary && !report.faces[0].is_fallback);

        // A is supported by the primary, but final usvg fallback replaces A as
        // well as the missing Korean glyph. Callback/cmap guesses get this wrong.
        let final_ids = shape_ids("A한", &options);
        assert_eq!(final_ids.len(), 2);
        assert!(final_ids.iter().all(|g| g.0 == fallback && g.1 != 0));
        let e = editor("A한", TextStyle::default(), 600.0, 200.0);
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(report.status, Status::Complete);
        assert_eq!(report.glyphs, 2);
        assert_eq!(report.unresolved_glyphs, 0);
        assert_eq!(report.faces[0].font.family, "Wanted Sans");
        assert_ne!(Some(&report.faces[0].font), report.primary.as_ref());
        assert_eq!(report.faces[0].glyphs, 2);
        assert!(report.faces[0].is_fallback && !report.faces[0].is_primary);
    }

    #[test]
    fn duplicate_face_names_still_distinguish_actual_fallback_ids() {
        let (mut options, primary, fallback) = isolated(true, true);
        let fallback = fallback.unwrap();
        let db = Arc::make_mut(&mut options.fontdb);
        let mut primary_info = db.face(primary).unwrap().clone();
        let fallback_info = db.face(fallback).unwrap().clone();
        // Duplicate installed versions can advertise identical identities while
        // having different coverage. Keep the ASCII-only primary's font bytes.
        primary_info.families = fallback_info.families;
        primary_info.post_script_name = fallback_info.post_script_name;
        db.remove_face(primary);
        let primary = db.push_face_info(primary_info);
        options.font_resolver.select_font = Box::new(move |_, _| Some(primary));
        let ids = shape_ids("A한", &options);
        assert!(ids.iter().all(|glyph| glyph.0 == fallback));
        let e = editor("A한", TextStyle::default(), 600.0, 200.0);
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(report.status, Status::Complete);
        assert_eq!(Some(&report.faces[0].font), report.primary.as_ref());
        assert!(report.faces[0].is_fallback && !report.faces[0].is_primary);
    }

    #[test]
    fn unresolved_counts_are_final_glyph_zero_not_requested_char_counts() {
        let (options, primary, _) = isolated(true, false);
        let e = editor("A한😀\u{10ffff}", TextStyle::default(), 600.0, 200.0);
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(report.status, Status::Complete);
        assert_eq!(report.unresolved_glyphs, 3);
        assert_eq!(
            report
                .samples
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>(),
            vec!["한", "😀", "\u{10ffff}"]
        );
        assert_eq!(report.samples[1].code_points, vec![0x1f600]);
        assert_eq!(report.faces[0].glyphs, 4);
        assert_eq!(report.glyphs, shape_ids("A한😀\u{10ffff}", &options).len());
    }

    #[test]
    fn clusters_ligatures_combining_korean_rtl_and_whitespace_are_honest() {
        let (options, primary, _) = isolated(false, false);
        for source in [
            "office ffi",
            "wanted_logo",
            "a\u{301}\u{308}",
            "한글",
            "אבג 12 مرحبا",
            "A \tB",
            " \t ",
        ] {
            let e = editor(source, TextStyle::default(), 600.0, 200.0);
            let report = inspect(e.selected_layer().unwrap(), &options, primary);
            let final_ids = shape_ids(source, &options);
            assert_eq!(report.glyphs, final_ids.len(), "{source:?}");
            assert_eq!(
                report.unresolved_glyphs,
                final_ids.iter().filter(|g| g.1 == 0).count(),
                "{source:?}"
            );
            assert!(
                !matches!(report.status, Status::Empty | Status::Unsupported),
                "{source:?}: {report:?}"
            );
            // Any parser/shaper metadata loss must be an explicit incomplete
            // state. Do not manufacture byte ranges for multi-glyph clusters.
            let represented: usize = final_ids.iter().map(|g| g.2.chars().count()).sum();
            if represented != source.chars().count() {
                assert!(matches!(report.status, Status::Incomplete(_)), "{source:?}");
            } else {
                assert_eq!(report.status, Status::Complete, "{source:?}");
            }
        }
        let ligatures = shape_ids("wanted_logo", &options);
        assert!(
            ligatures.iter().any(|g| g.2.chars().count() > 1),
            "fixture must exercise real ligature metadata: {ligatures:?}"
        );
        let combining = shape_ids("a\u{301}\u{308}", &options);
        assert!(
            combining.iter().any(|g| g.2.is_empty()),
            "fixture must exercise empty cluster metadata: {combining:?}"
        );
    }

    #[test]
    fn multiline_and_paragraph_overflow_reuse_composed_geometry() {
        let (options, primary, _) = isolated(false, false);
        let e = editor("A\r\nB\n\nC", TextStyle::default(), 600.0, 400.0);
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(report.composed_lines, 4);
        assert_eq!(report.glyphs, 3);
        assert_eq!(report.status, Status::Complete);
        for text in ["A\rB\r\rC\r", "A\r\nB\r\n\r\nC\r\n", "A\rB\r\n\nC\n"] {
            let e = editor(text, TextStyle::default(), 600.0, 400.0);
            let report = inspect(e.selected_layer().unwrap(), &options, primary);
            assert_eq!(report.composed_lines, 5);
            assert_eq!(report.glyphs, 3);
            assert_eq!(report.status, Status::Complete);
        }
        let style = TextStyle {
            paragraph: true,
            ..Default::default()
        };
        let e = editor("one two three four five", style.clone(), 150.0, 65.0);
        let layer = e.selected_layer().unwrap();
        let lines = crate::text_flow::lines("one two three four five", 48.0, 150.0, &style);
        let report = inspect(layer, &options, primary);
        assert_eq!(
            report.composed_lines,
            crate::text_flow::composed_count(&lines, 65.0)
        );
        assert_eq!(report.overflow_lines, lines.len() - report.composed_lines);
        assert!(report.composed_lines > 0 && report.overflow_lines > 0);
        assert!(matches!(report.status, Status::Incomplete(ref s) if s.contains("overflow")));
        let e = editor("W", style, 1.0, 200.0);
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(report.glyphs, 0);
        assert_eq!(report.overflow_lines, 1);
        assert!(matches!(report.status, Status::Incomplete(_)));
    }

    #[test]
    fn animated_typography_drives_composition_overflow_and_actual_glyphs_at_checked_frame() {
        for (parameter, end, source, width, height) in [
            (TextParam::FontSize, 192.0, "AAA AAA AAA", 300.0, 120.0),
            (
                TextParam::Tracking,
                1000.0,
                "one two three four five six",
                220.0,
                200.0,
            ),
            (TextParam::Leading, 4.0, "A\nB\nC", 600.0, 120.0),
        ] {
            let mut e = editor(
                source,
                TextStyle {
                    paragraph: true,
                    ..Default::default()
                },
                width,
                height,
            );
            let id = e.selected().unwrap();
            for edit in [
                TrackEdit::ToggleAnimation { frame: 0 },
                TrackEdit::Value {
                    frame: 40,
                    value: end,
                },
            ] {
                e.execute(Command::EditText {
                    id,
                    parameter,
                    edit,
                })
                .unwrap();
            }
            let original = e.project().clone();
            let layer = e.selected_layer().unwrap();
            let first = analyze(layer, 0);
            let last = analyze(layer, 40);
            assert_eq!(first.checked_frame, 0);
            assert_eq!(last.checked_frame, 40);
            for report in [&first, &last] {
                let typography = layer.text_typography_at(report.checked_frame).unwrap();
                let mut style = layer.text_style();
                typography.apply_to_style(&mut style);
                let lines = crate::text_flow::lines(source, typography.font_size, width, &style);
                assert_eq!(
                    report.composed_lines,
                    crate::text_flow::composed_count(&lines, height)
                );
                assert_eq!(report.overflow_lines, lines.len() - report.composed_lines);
                let geometry = crate::rendering::text_geometry_svg(
                    source,
                    typography.font_size,
                    "white",
                    width,
                    height,
                    &style,
                );
                let svg = format!(
                    "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}'>{geometry}</svg>"
                );
                let tree = usvg::Tree::from_str(&svg, &crate::fonts::render_options()).unwrap();
                let mut ids = vec![];
                tree_ids(tree.root(), &mut ids);
                assert_eq!(report.glyphs, ids.len());
                assert_eq!(
                    report.unresolved_glyphs,
                    ids.iter().filter(|glyph| glyph.1 == 0).count()
                );
            }
            assert_ne!(
                (first.composed_lines, first.overflow_lines),
                (last.composed_lines, last.overflow_lines),
                "{parameter:?}"
            );
            assert_ne!(first.glyphs, last.glyphs, "{parameter:?}");
            assert_eq!(e.project(), &original);
        }
    }

    #[test]
    fn animated_wrapping_preserves_line_limit_and_reports_the_requested_frame() {
        let mut e = editor(
            &"A ".repeat(MAX_SOURCE_LINES + 1),
            TextStyle {
                paragraph: true,
                ..Default::default()
            },
            600.0,
            200.0,
        );
        let id = e.selected().unwrap();
        for edit in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value {
                frame: 40,
                value: 10000.0,
            },
        ] {
            e.execute(Command::EditText {
                id,
                parameter: TextParam::Tracking,
                edit,
            })
            .unwrap();
        }
        let report = analyze(e.selected_layer().unwrap(), 40);
        assert_eq!(report.checked_frame, 40);
        assert_eq!(report.glyphs, 0);
        assert!(report.truncated);
        assert!(
            matches!(report.status, Status::Incomplete(ref reason) if reason.contains("line analysis limit"))
        );
    }

    #[test]
    fn dropped_nodes_negative_tracking_empty_failed_and_unsupported_are_not_clean() {
        let (options, primary, _) = isolated(true, false);
        let e = editor(
            "AAA",
            TextStyle {
                tracking: -1000.0,
                ..Default::default()
            },
            600.0,
            200.0,
        );
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert!(report.glyphs < 3);
        assert!(matches!(report.status, Status::Incomplete(_)));
        let e = editor("", TextStyle::default(), 600.0, 200.0);
        assert_eq!(
            inspect(e.selected_layer().unwrap(), &options, primary).status,
            Status::Empty
        );
        let no_fonts = usvg::Options::default();
        let e = editor("A", TextStyle::default(), 600.0, 200.0);
        let report = analyze_with_options(e.selected_layer().unwrap(), 0, &no_fonts, None);
        assert_eq!(report.glyphs, 0);
        assert!(matches!(report.status, Status::Incomplete(_)));
        let report = inspect_svg(
            "<svg",
            &options,
            BTreeMap::new(),
            0,
            None,
            Report::new(None, 0),
        );
        assert!(matches!(report.status, Status::Incomplete(ref s) if s.contains("parsed")));
        let mut e = e;
        e.execute(Command::SetContent {
            id: e.selected().unwrap(),
            content: Content::Solid,
        })
        .unwrap();
        assert_eq!(
            analyze(e.selected_layer().unwrap(), 0).status,
            Status::Unsupported
        );
    }

    #[test]
    fn report_limits_are_explicit_and_samples_are_bounded() {
        let (options, primary, _) = isolated(true, false);
        for source in [
            "A".repeat(MAX_SOURCE_BYTES + 1),
            "A\n".repeat(MAX_SOURCE_LINES + 1),
            "A\r".repeat(MAX_SOURCE_LINES + 1),
            "A\r\n".repeat(MAX_SOURCE_LINES + 1),
        ] {
            let e = editor(&source, TextStyle::default(), 600.0, 200.0);
            let report = inspect(e.selected_layer().unwrap(), &options, primary);
            assert_eq!(report.glyphs, 0);
            assert!(report.truncated);
            assert!(matches!(report.status, Status::Incomplete(_)));
        }
        let unsupported: String = (0x1f600..0x1f620).filter_map(char::from_u32).collect();
        // This fixture's all-.notdef node has no retained outline and is dropped
        // by usvg. Zero reported glyph IDs here must never claim clean coverage.
        let e = editor(&unsupported, TextStyle::default(), 600.0, 200.0);
        let dropped = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(dropped.glyphs, 0);
        assert!(matches!(dropped.status, Status::Incomplete(_)));
        let e = editor(
            &format!("A{unsupported}"),
            TextStyle::default(),
            600.0,
            200.0,
        );
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(report.unresolved_glyphs, 32);
        assert_eq!(report.samples.len(), MAX_SAMPLES);
        assert!(report.truncated);
        assert!(
            report
                .samples
                .iter()
                .all(|sample| sample.text.chars().count() <= MAX_SAMPLE_CHARS
                    && sample.code_points.len() <= MAX_SAMPLE_CHARS)
        );
    }

    #[test]
    fn added_fallback_font_is_resolved_from_tree_database() {
        let (mut options, primary, _) = isolated(true, false);
        assert_eq!(options.fontdb.faces().count(), 1);
        options.font_resolver.select_fallback = Box::new(|_, used, db| {
            assert_eq!(used.len(), 1);
            Arc::make_mut(db).load_font_data(WANTED.to_vec());
            db.faces().last().map(|face| face.id)
        });
        let e = editor("A한", TextStyle::default(), 600.0, 200.0);
        let report = inspect(e.selected_layer().unwrap(), &options, primary);
        assert_eq!(options.fontdb.faces().count(), 1);
        assert_eq!(report.status, Status::Complete);
        assert_eq!(report.faces[0].font.family, "Wanted Sans");
        assert_eq!(report.faces[0].glyphs, 2);
    }

    #[test]
    fn analysis_preserves_source_history_pixels_layout_and_paint_independence() {
        let mut e = editor("office 한글\nA\tB", TextStyle::default(), 600.0, 200.0);
        let id = e.selected().unwrap();
        let renderer = crate::rendering::Renderer::new();
        let before = e.project().clone();
        let serialized = before.to_json().unwrap();
        let pixels = renderer.render(&before, 0, 480).unwrap();
        let flow = crate::text_flow::lines("office 한글\nA\tB", 48.0, 600.0, &TextStyle::default());
        let history = (e.can_undo(), e.can_redo());
        let report = analyze(e.selected_layer().unwrap(), 0);
        assert_eq!(report.status, Status::Complete);
        assert_eq!(e.project(), &before);
        assert_eq!(e.project().to_json().unwrap(), serialized);
        assert_eq!((e.can_undo(), e.can_redo()), history);
        assert_eq!(renderer.render(e.project(), 0, 480).unwrap(), pixels);
        let after_flow =
            crate::text_flow::lines("office 한글\nA\tB", 48.0, 600.0, &TextStyle::default());
        assert!(Arc::ptr_eq(&flow, &after_flow));
        // Double paint passes and hidden/disabled source are intentionally the
        // same shaping inspection, never double-counting Fill plus Stroke.
        e.execute(Command::SetTextStyle {
            id,
            style: TextStyle {
                stroke_enabled: true,
                stroke_width: 8.0,
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(analyze(e.selected_layer().unwrap(), 0), report);
        e.execute(Command::SetTextStyle {
            id,
            style: TextStyle {
                fill_enabled: false,
                stroke_enabled: false,
                ..Default::default()
            },
        })
        .unwrap();
        e.execute(Command::ToggleVisible(id)).unwrap();
        assert_eq!(analyze(e.selected_layer().unwrap(), 0), report);
        e.undo();
        let redo = e.project().clone();
        let history = (e.can_undo(), e.can_redo());
        analyze(e.selected_layer().unwrap(), 0);
        assert_eq!(e.project(), &redo);
        assert_eq!((e.can_undo(), e.can_redo()), history);
        e.redo();
        assert!(!e.selected_layer().unwrap().visible());
    }

    #[test]
    fn text_opacity_never_hides_shaping_diagnostics_or_changes_source() {
        for paragraph in [false, true] {
            let mut e = editor(
                "office 한글\nA\tB",
                TextStyle {
                    paragraph,
                    stroke_enabled: true,
                    stroke_width: 8.,
                    tracking: -150.,
                    leading: 0.6,
                    ..Default::default()
                },
                180.,
                140.,
            );
            let initial = e.project().clone();
            let id = e.selected().unwrap();
            let baseline = analyze(e.selected_layer().unwrap(), 0);
            assert!(baseline.glyphs > 0);
            for parameter in [TextParam::FillOpacity, TextParam::StrokeOpacity] {
                e.execute(Command::EditText {
                    id,
                    parameter,
                    edit: TrackEdit::ToggleAnimation { frame: 0 },
                })
                .unwrap();
                e.execute(Command::EditText {
                    id,
                    parameter,
                    edit: TrackEdit::Value {
                        frame: 60,
                        value: 0.,
                    },
                })
                .unwrap();
            }
            for disabled in [false, true] {
                if disabled {
                    let mut style = e.selected_layer().unwrap().text_style();
                    style.fill_enabled = false;
                    style.stroke_enabled = false;
                    style.stroke_width = 0.;
                    e.execute(Command::SetTextStyle { id, style }).unwrap();
                }
                let snapshot = e.project().clone();
                let json = snapshot.to_json().unwrap();
                let history = (e.can_undo(), e.can_redo());
                for frame in [0, 15, 30, 45, 60] {
                    let mut expected = baseline.clone();
                    expected.checked_frame = frame;
                    assert_eq!(
                        analyze(e.selected_layer().unwrap(), frame),
                        expected,
                        "paragraph={paragraph} disabled={disabled} frame={frame}"
                    );
                }
                assert_eq!(e.project(), &snapshot);
                assert_eq!(e.project().to_json().unwrap(), json);
                assert_eq!((e.can_undo(), e.can_redo()), history);
                assert_eq!(
                    e.selected_layer().unwrap().content(),
                    initial.composition().layer(id).unwrap().content()
                );
            }
        }
    }
}

#[cfg(test)]
mod source_text_tests {
    use super::*;
    use crate::rendering::source_text_tests::{animated_scene, baked_scene};
    use libre_effects_core::{Command, PropertyPath, TextParam, TrackEdit};

    #[test]
    fn source_text_glyph_reports_match_independently_baked_current_frames() {
        for paragraph in [false, true] {
            let mut e = animated_scene(paragraph);
            let mut style = e.selected_layer().unwrap().text_style();
            style.fill_enabled = false;
            style.stroke_enabled = false;
            e.execute(Command::SetTextStyle { id: 1, style }).unwrap();
            for parameter in [TextParam::FillOpacity, TextParam::StrokeOpacity] {
                e.execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit: TrackEdit::Value {
                        frame: 20,
                        value: 0.,
                    },
                })
                .unwrap();
            }
            let before = e.project().clone();
            let json = before.to_json().unwrap();
            for frame in [0, 9, 10, 19, 20, 29, 30, 39, 40, 60] {
                let static_e = baked_scene(paragraph, frame);
                assert_eq!(
                    analyze(e.project().composition().layer(1).unwrap(), frame),
                    analyze(static_e.selected_layer().unwrap(), frame),
                    "paragraph={paragraph} frame={frame}"
                );
            }
            assert_eq!(e.project(), &before);
            assert_eq!(e.project().to_json().unwrap(), json);
        }
    }

    #[test]
    fn source_text_analysis_limits_apply_only_to_the_current_sample() {
        let mut e = animated_scene(false);
        for (frame, text) in [
            (50, "한".repeat(MAX_SOURCE_BYTES / 3 + 1)),
            (60, "A\n".repeat(MAX_SOURCE_LINES)),
            (70, String::new()),
        ] {
            e.execute(Command::EditSourceText { id: 1, frame, text })
                .unwrap();
        }
        let before = e.project().clone();
        let layer = before.composition().layer(1).unwrap();
        assert!(layer.track(PropertyPath::SourceText).unwrap().keys().len() > 4);
        for frame in [0, 10, 20, 40, 49] {
            let report = analyze(layer, frame);
            assert!(
                report.glyphs > 0,
                "other oversized keys cannot suppress frame {frame}"
            );
            assert_ne!(
                report.status,
                Status::Incomplete("Layer exceeds the text analysis limit".into())
            );
        }
        for frame in [50, 59, 60, 69] {
            let report = analyze(layer, frame);
            assert_eq!(report.checked_frame, frame);
            assert_eq!(
                report.status,
                Status::Incomplete("Layer exceeds the text analysis limit".into())
            );
            assert!(report.truncated);
            assert_eq!(report.glyphs, 0);
        }
        for frame in [30, 39, 70, 89] {
            let report = analyze(layer, frame);
            assert_eq!(report.status, Status::Empty);
            assert_eq!(report.glyphs, 0);
            assert!(!report.truncated);
        }
        assert_eq!(e.project(), &before);
    }
}
