//! Static point-text runs, shaped as a single compositor chunk per hard line.
//!
//! A tspan carries style. Optional authored origins are admitted only after exact
//! font and shaping verification. The same positioned compositor metadata drives
//! pixels, selection, picking and IME geometry.
use crate::rendering::{append_svg, svg_document};
use libre_effects_core::{AuthoredTextLine, RichText, TextCharacterStyle, TextStyle};
use resvg::usvg;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    ops::Range,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub(crate) struct Cluster {
    pub range: Range<usize>,
    pub x: f64,
    pub end: f64,
}
#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub range: Range<usize>,
    pub terminator: Range<usize>,
    pub y: f64,
    pub size: f64,
    pub clusters: Vec<Cluster>,
    /// Actual authored glyph ink, including enabled strokes. Kept separate
    /// from source caret terminals, which may precede an outline overhang.
    pub paint_bounds: Option<[f64; 4]>,
}
#[derive(Clone, Debug)]
pub(crate) struct Composition {
    pub lines: Vec<Line>,
    fill: String,
    stroke: String,
    stroke_over_fill: bool,
}
#[derive(PartialEq, serde::Serialize)]
struct Key {
    text: String,
    rich: RichText,
    width: f64,
    style: TextStyle,
}
thread_local! {
    static LAST: RefCell<Option<(Arc<Key>, Arc<Composition>)>> = const { RefCell::new(None) };
    static COMPOSITIONS: RefCell<Option<Arc<Mutex<CompositionCache>>>> = const { RefCell::new(None) };
}
#[derive(Default)]
pub(crate) struct CompositionCache {
    entries: VecDeque<(Arc<Key>, Arc<Composition>, usize)>,
    bytes: usize,
}
impl CompositionCache {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}
pub(crate) struct CompositionCacheGuard {
    previous: Option<Arc<Mutex<CompositionCache>>>,
    thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Drop for CompositionCacheGuard {
    fn drop(&mut self) {
        COMPOSITIONS.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}
pub(crate) fn install_composition_cache(
    cache: Arc<Mutex<CompositionCache>>,
) -> CompositionCacheGuard {
    CompositionCacheGuard {
        previous: COMPOSITIONS.with(|slot| slot.replace(Some(cache))),
        thread: std::marker::PhantomData,
    }
}
fn cached_composition(
    text: &str,
    rich: &RichText,
    width: f64,
    style: &TextStyle,
) -> Option<Arc<Composition>> {
    COMPOSITIONS.with(|slot| {
        let cache = slot.borrow().clone()?;
        let mut cache = cache.lock().ok()?;
        let index = cache.entries.iter().position(|(key, _, _)| {
            key.text == text && key.rich == *rich && key.width == width && key.style == *style
        })?;
        let entry = cache.entries.remove(index)?;
        let composition = entry.1.clone();
        cache.entries.push_back(entry);
        Some(composition)
    })
}
fn store_composition(key: Arc<Key>, result: Arc<Composition>) {
    COMPOSITIONS.with(|slot| {
        let Some(cache) = slot.borrow().clone() else {
            return;
        };
        // Count without materializing JSON or an unbounded auxiliary key.
        struct Count(usize);
        impl std::io::Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.saturating_add(bytes.len());
                if self.0 > 8 * 1024 * 1024 {
                    return Err(std::io::Error::other(
                        "Composition key exceeds cache budget",
                    ));
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        if serde_json::to_writer(&mut count, key.as_ref()).is_err() {
            return;
        }
        let bytes = count
            .0
            .saturating_mul(4)
            .saturating_add(1024)
            .saturating_add(result.fill.len())
            .saturating_add(result.stroke.len())
            .saturating_add(result.lines.capacity() * std::mem::size_of::<Line>())
            .saturating_add(
                result
                    .lines
                    .iter()
                    .map(|line| line.clusters.capacity() * std::mem::size_of::<Cluster>())
                    .sum::<usize>(),
            );
        const LIMIT: usize = 32 * 1024 * 1024;
        if bytes > LIMIT {
            return;
        }
        let Ok(mut cache) = cache.lock() else {
            return;
        };
        while !cache.entries.is_empty()
            && (cache.bytes + bytes > LIMIT || cache.entries.len() >= 32)
        {
            cache.bytes -= cache.entries.pop_front().unwrap().2;
        }
        cache.bytes += bytes;
        cache.entries.push_back((key, result, bytes));
    });
}
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn font_style(style: &TextCharacterStyle) -> TextStyle {
    TextStyle {
        font_family: style.font_family.clone(),
        font_face: style.font_face.clone(),
        weight: style.weight,
        italic: style.italic,
        ..Default::default()
    }
}
#[derive(Clone, Copy)]
enum Pass {
    Geometry,
    Fill,
    Stroke,
}
fn line_svg(
    text: &str,
    rich: &RichText,
    range: Range<usize>,
    width: f64,
    baseline: f64,
    style: &TextStyle,
    pass: Pass,
    authored: Option<&AuthoredTextLine>,
) -> Result<String, String> {
    let x = rich.alignment_origin(width, style.align);
    let anchor = match style.align {
        libre_effects_core::TextAlign::Left => "start",
        libre_effects_core::TextAlign::Center => "middle",
        libre_effects_core::TextAlign::Right => "end",
    };
    let mut svg = String::new();
    let variant = if rich.proportional_metrics {
        " font-variant='proportional-width'"
    } else {
        ""
    };
    if let Some(authored) = authored {
        append_svg(&mut svg, format_args!("<text{variant} x='"))?;
        for glyph in &authored.glyphs {
            append_svg(&mut svg, format_args!("{} ", glyph.x))?;
        }
        // Authored origins already contain alignment, kerning and tracking.
        append_svg(
            &mut svg,
            format_args!("' y='{baseline}' text-anchor='start' xml:space='preserve'>"),
        )?;
    } else {
        append_svg(
            &mut svg,
            format_args!(
                "<text{variant} x='{x}' y='{baseline}' text-anchor='{anchor}' xml:space='preserve'>"
            ),
        )?;
    }
    for run in rich
        .runs
        .iter()
        .filter(|run| run.start < range.end && run.end > range.start)
    {
        let start = run.start.max(range.start);
        let end = run.end.min(range.end);
        let s = &run.style;
        let font = crate::fonts::resolved(&font_style(s));
        append_svg(
            &mut svg,
            format_args!(
                "<tspan font-family='{}' font-weight='{}' font-style='{}' font-size='{}' letter-spacing='{}'",
                xml(crate::fonts::svg_family(&font)),
                font.weight,
                if font.italic { "italic" } else { "normal" },
                s.font_size,
                if authored.is_some() {
                    0.0
                } else {
                    s.tracking * s.font_size / 1000.0
                },
            ),
        )?;
        match pass {
            Pass::Geometry => append_svg(&mut svg, format_args!(" fill='white'"))?,
            Pass::Fill if s.fill_enabled => {
                append_svg(&mut svg, format_args!(" fill='#{:06x}'", s.fill_color))?
            }
            Pass::Stroke if s.stroke_enabled && s.stroke_width > 0.0 => {
                let join = match s.stroke_join {
                    libre_effects_core::TextStrokeJoin::Miter => "miter",
                    libre_effects_core::TextStrokeJoin::Round => "round",
                    libre_effects_core::TextStrokeJoin::Bevel => "bevel",
                };
                append_svg(
                    &mut svg,
                    format_args!(
                        " fill='none' stroke='#{:06x}' stroke-width='{}' stroke-linejoin='{join}' stroke-miterlimit='4'",
                        s.stroke_color, s.stroke_width,
                    ),
                )?;
            }
            _ => append_svg(&mut svg, format_args!(" fill='none'"))?,
        }
        // XML normalizes a tab to one space, preserving the original byte map.
        append_svg(
            &mut svg,
            format_args!(">{}</tspan>", xml(&text[start..end])),
        )?;
    }
    append_svg(&mut svg, format_args!("</text>"))?;
    Ok(svg)
}
fn find_text(group: &usvg::Group) -> Option<&usvg::Text> {
    group.children().iter().find_map(|node| match node {
        usvg::Node::Text(text) => Some(text.as_ref()),
        usvg::Node::Group(group) => find_text(group),
        _ => None,
    })
}
fn line_clusters(
    svg: &str,
    range: Range<usize>,
    rich: &RichText,
    width: f64,
    size: f64,
    authored: Option<(&AuthoredTextLine, &crate::fonts::AuthoredFont)>,
) -> Result<Vec<Cluster>, String> {
    if range.is_empty() {
        return Ok(vec![]);
    }
    let svg = svg_document(svg, width.max(1.0), (size * 3.0).max(1.0))?;
    let options = authored.map_or_else(crate::fonts::render_options, |(_, font)| font.options());
    let tree = usvg::Tree::from_str(&svg, &options)
        .map_err(|error| format!("Could not shape rich text: {error}"))?;
    let Some(node) = find_text(tree.root()) else {
        return Err("The compositor could not lay out this rich-text line".into());
    };
    if let Some((authored, font)) = authored {
        verify_glyphs(node, authored, font, true)?;
    }
    let mut source = BTreeMap::<(usize, usize), f64>::new();
    for glyph in node
        .layouted()
        .iter()
        .flat_map(|span| &span.positioned_glyphs)
    {
        let start = range.start + glyph.source_range.start;
        let end = range.start + glyph.source_range.end;
        if start >= end || end > range.end {
            return Err("The compositor returned an invalid rich-text source cluster".into());
        }
        let run = rich
            .runs
            .iter()
            .find(|run| run.start <= start && run.end > start)
            .ok_or("A rich-text source cluster has no style")?;
        // Whole-chunk shaping may form a ligature over a tspan boundary. usvg
        // paints that cluster with its first span. Reject rather than quietly
        // lose the second style, or destroy kerning/bidi by shaping runs apart.
        if end > run.end {
            return Err(format!(
                "Rich-text style boundary at byte {} crosses a shaped cluster ({start}..{end}); move the boundary outside the ligature or joined script cluster",
                run.end,
            ));
        }
        let x = f64::from(glyph.baseline_origin().x);
        source.entry((start, end)).or_insert(x);
    }
    let mut clusters: Vec<_> = source
        .into_iter()
        .map(|((start, end), x)| Cluster {
            range: start..end,
            x,
            end: x,
        })
        .collect();
    // Spans are in logical style order, not visual bidi order. Sort actual
    // compositor positions before taking adjacent origins as cluster advances.
    clusters.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.range.start.cmp(&b.range.start)));
    // A source terminal is a caret boundary, not an ink origin or paint bound.
    // Without independent source pen positions, adjacent verified origins and
    // this terminal define our deterministic caret policy, not AE caret parity.
    let right = authored.map_or_else(
        || f64::from(node.bounding_box().right()),
        |(line, _)| line.end_x,
    );
    for i in 0..clusters.len() {
        clusters[i].end = clusters
            .get(i + 1)
            .map_or(right, |next| next.x)
            .max(clusters[i].x);
    }
    Ok(clusters)
}

fn positioning_error(reason: impl std::fmt::Display) -> String {
    format!("Authored text spacing: {reason}. Reset authored spacing to use native layout.")
}

fn verify_font_and_script(
    text: &str,
    line: &AuthoredTextLine,
) -> Result<&'static crate::fonts::AuthoredFont, String> {
    let source = &text[line.start..line.end];
    if line
        .glyphs
        .windows(2)
        .any(|pair| pair[0].x as f32 >= pair[1].x as f32)
        || line
            .glyphs
            .last()
            .is_some_and(|glyph| line.end_x <= f64::from(glyph.x as f32))
    {
        return Err(positioning_error(
            "saved origins exceed compositor coordinate precision",
        ));
    }
    let bidi = unicode_bidi::BidiInfo::new(source, Some(unicode_bidi::Level::ltr()));
    if bidi.levels.iter().any(|level| level.is_rtl())
        || source.chars().any(|ch| {
            !matches!(
                ch.script(),
                Script::Latin
                    | Script::Greek
                    | Script::Cyrillic
                    | Script::Han
                    | Script::Hiragana
                    | Script::Katakana
                    | Script::Hangul
                    | Script::Common
            )
        })
    {
        return Err(positioning_error(
            "this line requires unsupported complex or RTL shaping",
        ));
    }
    let font = crate::fonts::authored_font(&font_style(&line.style)).map_err(positioning_error)?;
    if font.post_script_name != line.style.font_face || font.index != line.font_index {
        return Err(positioning_error(format!(
            "font face identity does not match {} (index {})",
            line.style.font_face, line.font_index,
        )));
    }
    if !font.sha256.eq_ignore_ascii_case(&line.font_sha256) {
        return Err(positioning_error(format!(
            "font version SHA-256 mismatch for {}",
            line.style.font_face,
        )));
    }
    Ok(font)
}

fn verify_glyphs(
    node: &usvg::Text,
    line: &AuthoredTextLine,
    font: &crate::fonts::AuthoredFont,
    positioned: bool,
) -> Result<(), String> {
    let glyphs: Vec<_> = node
        .layouted()
        .iter()
        .flat_map(|span| &span.positioned_glyphs)
        .collect();
    if glyphs.len() != line.glyphs.len() {
        return Err(positioning_error(
            "shaped glyph count differs from the saved one-to-one mapping",
        ));
    }
    for (actual, expected) in glyphs.into_iter().zip(&line.glyphs) {
        if actual.font != font.id
            || actual.id.0 == 0
            || actual.id.0 != expected.glyph_id
            || actual.source_range != (expected.start - line.start..expected.end - line.start)
        {
            return Err(positioning_error(format!(
                "shaped font, glyph ID or source cluster differs at byte {}",
                expected.start,
            )));
        }
        if positioned && actual.baseline_origin().x != expected.x as f32 {
            return Err(positioning_error(format!(
                "compositor changed the saved glyph origin at byte {}",
                expected.start,
            )));
        }
    }
    Ok(())
}

fn shape_authored(
    svg: &str,
    width: f64,
    size: f64,
    font: &crate::fonts::AuthoredFont,
) -> Result<usvg::Tree, String> {
    let svg = svg_document(svg, width.max(1.0), (size * 3.0).max(1.0))?;
    usvg::Tree::from_str(&svg, &font.options())
        .map_err(|error| positioning_error(format!("could not shape the saved line: {error}")))
}

fn authored_paint(
    svg: &str,
    width: f64,
    size: f64,
    font: &crate::fonts::AuthoredFont,
) -> Result<(String, [f64; 4]), String> {
    let tree = shape_authored(svg, width, size, font)?;
    let bounds = tree.root().stroke_bounding_box();
    let bounds = [bounds.x(), bounds.y(), bounds.width(), bounds.height()].map(f64::from);
    crate::text_animator_render::check_serialization_budget(&tree).map_err(positioning_error)?;
    // Like Text Animator, preserve exact f32 coordinates and give every resource
    // a unique identity. Flatten now against pinned bytes; later paint/export
    // consume these same outlines rather than resolving system fonts again.
    static NEXT_FRAGMENT: AtomicU64 = AtomicU64::new(1);
    let prefix = NEXT_FRAGMENT.fetch_add(1, Ordering::Relaxed);
    let output = tree.to_string_with_unique_resource_ids(&usvg::WriteOptions {
        id_prefix: Some(format!("le-authored-{prefix}-")),
        indent: usvg::Indent::None,
        ..Default::default()
    });
    if output.len() > crate::rendering::SVG_LIMIT {
        return Err(positioning_error("expanded glyph SVG exceeds 64 MiB"));
    }
    // The temporary shaping viewport must not clip point text at negative x/y.
    let start = output.find('>').ok_or("Authored glyph SVG has no root")? + 1;
    let end = output
        .rfind("</svg>")
        .ok_or("Authored glyph SVG has no closing root")?;
    Ok((output[start..end].into(), bounds))
}

pub(crate) fn compose(
    text: &str,
    rich: &RichText,
    width: f64,
    style: &TextStyle,
) -> Result<Arc<Composition>, String> {
    if let Some(result) = cached_composition(text, rich, width, style) {
        return Ok(result);
    }
    LAST.with(|last| {
        if let Some((key, result)) = &*last.borrow() {
            if key.text == text && key.rich == *rich && key.width == width && key.style == *style {
                store_composition(key.clone(), result.clone());
                return Ok(result.clone());
            }
        }
        if style.paragraph {
            return Err("Rich text currently supports point text, not paragraph wrapping".into());
        }
        // Session drafts are not necessarily project-validated yet.
        rich.validate(text)?;
        rich.validate_positioning(text, style)?;
        let boundaries: std::collections::BTreeSet<_> = text
            .grapheme_indices(true)
            .map(|(start, _)| start)
            .chain([text.len()])
            .collect();
        if let Some(run) = rich.runs.iter().find(|run| !boundaries.contains(&run.end)) {
            return Err(format!(
                "Rich-text style boundary at byte {} splits a grapheme or CRLF break",
                run.end
            ));
        }
        for s in std::iter::once(&rich.default_style).chain(rich.runs.iter().map(|run| &run.style))
        {
            if s.stroke_over_fill != rich.default_style.stroke_over_fill {
                return Err("Rich text requires one whole-layer fill/stroke order".into());
            }
            if let Some(warning) = crate::fonts::warning(&font_style(s)) {
                let reason = format!(
                    "Rich-text font unavailable ({} / {}): {warning}",
                    s.font_family, s.font_face
                );
                return Err(if rich.positioning.is_some() {
                    positioning_error(reason)
                } else {
                    reason
                });
            }
        }
        let mut result = Composition {
            lines: vec![],
            fill: String::new(),
            stroke: String::new(),
            stroke_over_fill: rich.default_style.stroke_over_fill,
        };
        for line in rich.line_metrics(text, style)? {
            let authored = rich.positioning.as_ref().and_then(|positioning| {
                positioning
                    .lines
                    .iter()
                    .find(|saved| saved.start == line.range.start)
            });
            let font = authored
                .map(|saved| verify_font_and_script(text, saved))
                .transpose()?;
            let runs: Vec<_> = rich
                .runs
                .iter()
                .filter(|run| run.start < line.range.end && run.end > line.range.start)
                .collect();
            let geometry = line_svg(
                text,
                rich,
                line.range.clone(),
                width,
                line.baseline,
                style,
                Pass::Geometry,
                authored,
            )?;
            if let Some((saved, font)) = authored.zip(font) {
                // Verify ordinary whole-line shaping before explicit x values
                // can split SVG chunks and conceal ligatures or reordering.
                let ordinary = line_svg(
                    text,
                    rich,
                    line.range.clone(),
                    width,
                    line.baseline,
                    style,
                    Pass::Geometry,
                    None,
                )?;
                let tree = shape_authored(&ordinary, width, line.size, font)?;
                let node = find_text(tree.root())
                    .ok_or_else(|| positioning_error("the line did not shape"))?;
                verify_glyphs(node, saved, font, false)?;
            }
            let clusters = line_clusters(
                &geometry,
                line.range.clone(),
                rich,
                width,
                line.size,
                authored.zip(font),
            )?;
            let mut paint_bounds: Option<[f64; 4]> = None;
            let mut paint = |pass| -> Result<String, String> {
                let svg = line_svg(
                    text,
                    rich,
                    line.range.clone(),
                    width,
                    line.baseline,
                    style,
                    pass,
                    authored,
                )?;
                if let Some(font) = font {
                    let (svg, bounds) = authored_paint(&svg, width, line.size, font)?;
                    if bounds[2] > 0.0 && bounds[3] > 0.0 {
                        paint_bounds = Some(paint_bounds.map_or(bounds, |old| {
                            let x = old[0].min(bounds[0]);
                            let y = old[1].min(bounds[1]);
                            [
                                x,
                                y,
                                (old[0] + old[2]).max(bounds[0] + bounds[2]) - x,
                                (old[1] + old[3]).max(bounds[1] + bounds[3]) - y,
                            ]
                        }));
                    }
                    Ok(svg)
                } else {
                    Ok(svg)
                }
            };
            if runs.iter().any(|run| run.style.fill_enabled) {
                append_svg(&mut result.fill, format_args!("{}", paint(Pass::Fill)?))?;
            }
            if runs
                .iter()
                .any(|run| run.style.stroke_enabled && run.style.stroke_width > 0.0)
            {
                append_svg(&mut result.stroke, format_args!("{}", paint(Pass::Stroke)?))?;
            }
            result.lines.push(Line {
                range: line.range,
                terminator: line.terminator,
                y: line.top,
                size: line.size,
                clusters,
                paint_bounds,
            });
        }
        let result = Arc::new(result);
        let key = Arc::new(Key {
            text: text.into(),
            rich: rich.clone(),
            width,
            style: style.clone(),
        });
        store_composition(key.clone(), result.clone());
        *last.borrow_mut() = Some((key, result.clone()));
        Ok(result)
    })
}

pub(crate) fn layer_svg(
    text: &str,
    rich: &RichText,
    width: f64,
    style: &TextStyle,
    opacity: [f64; 2],
    retain_bounds: bool,
) -> Result<(String, Option<String>), String> {
    let composition = compose(text, rich, width, style)?;
    let combine = |fill: &str, stroke: &str| -> Result<String, String> {
        let mut svg = String::new();
        let (first, second) = if composition.stroke_over_fill {
            (fill, stroke)
        } else {
            (stroke, fill)
        };
        append_svg(&mut svg, format_args!("{first}{second}"))?;
        Ok(svg)
    };
    let bounds = if retain_bounds && opacity != [100.0; 2] {
        Some(combine(&composition.fill, &composition.stroke)?)
    } else {
        None
    };
    let attenuate = |pass: &str, opacity: f64| -> Result<String, String> {
        if opacity == 100.0 || pass.is_empty() {
            return Ok(pass.into());
        }
        let mut svg = String::new();
        append_svg(
            &mut svg,
            format_args!("<g opacity='{}'>{pass}</g>", opacity / 100.0),
        )?;
        Ok(svg)
    };
    let fill = attenuate(&composition.fill, opacity[0])?;
    let stroke = attenuate(&composition.stroke, opacity[1])?;
    Ok((combine(&fill, &stroke)?, bounds))
}

/// Paint-independent trees for the explicit font check. Use the renderer's
/// whole-line geometry and immutable verified font bytes for saved positions.
/// Ranges are local to each tree and identify the requested face of each glyph.
pub(crate) fn coverage_lines(
    text: &str,
    rich: &RichText,
    width: f64,
    style: &TextStyle,
) -> Result<Vec<(usvg::Tree, Vec<(Range<usize>, usvg::fontdb::ID)>)>, String> {
    // Retain all renderer admission checks, including exact authored-font and
    // shaping verification; font inspection must not bypass a rejected render.
    compose(text, rich, width, style)?;
    let mut result = Vec::new();
    for line in rich.line_metrics(text, style)? {
        if line.range.is_empty() {
            continue;
        }
        let authored = rich.positioning.as_ref().and_then(|positions| {
            positions
                .lines
                .iter()
                .find(|saved| saved.start == line.range.start)
        });
        let font = authored
            .map(|saved| verify_font_and_script(text, saved))
            .transpose()?;
        let geometry = line_svg(
            text,
            rich,
            line.range.clone(),
            width,
            line.baseline,
            style,
            Pass::Geometry,
            authored,
        )?;
        let options = font.map_or_else(crate::fonts::render_options, |font| font.options());
        let svg = svg_document(&geometry, width.max(1.0), (line.size * 3.0).max(1.0))?;
        let tree = usvg::Tree::from_str(&svg, &options)
            .map_err(|error| format!("Could not inspect rich text: {error}"))?;
        let primary = rich
            .runs
            .iter()
            .filter(|run| run.start < line.range.end && run.end > line.range.start)
            .map(|run| {
                (
                    run.start.max(line.range.start) - line.range.start
                        ..run.end.min(line.range.end) - line.range.start,
                    font.map_or_else(
                        || crate::fonts::matched(&font_style(&run.style)).id,
                        |font| font.id,
                    ),
                )
            })
            .collect();
        result.push((tree, primary));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires installed NotoSansCJKkr-Light for the live AE font-metrics qualification"]
    fn proportional_metrics_match_independent_ae_synthetic_baseline() {
        use super::*;
        use libre_effects_core::{TextAlign, TextStyleRun};
        let text = "あいう";
        let style = TextStyle {
            font_family: "Noto Sans CJK KR".into(),
            font_face: "NotoSansCJKkr-Light".into(),
            weight: 300,
            align: TextAlign::Center,
            ..Default::default()
        };
        assert!(
            crate::fonts::warning(&style).is_none(),
            "The exact reference font is required"
        );
        let character = TextCharacterStyle::from_style(&style, 46.0, 0xffffff);
        let mut rich = RichText::new(
            text,
            character.clone(),
            vec![TextStyleRun {
                start: 0,
                end: text.len(),
                style: character,
            }],
        )
        .unwrap();
        rich.point_origin = true;
        let legacy = compose(text, &rich, 400.0, &style).unwrap();
        rich.proportional_metrics = true;
        let proportional = compose(text, &rich, 400.0, &style).unwrap();
        // Captured independently from a temporary AE text layer using metrics
        // kerning, centered point text and this exact face/size. No source
        // project text or media is part of this synthetic fixture.
        // AE's baselineLocs includes the first glyph's palt placement offset.
        // Our cluster caret deliberately excludes that offset; inspect the
        // same authoritative glyph transform used to paint instead.
        let geometry = line_svg(
            text,
            &rich,
            0..text.len(),
            400.0,
            0.0,
            &style,
            Pass::Geometry,
            None,
        )
        .unwrap();
        let tree = usvg::Tree::from_str(
            &svg_document(&geometry, 400.0, 140.0).unwrap(),
            &crate::fonts::render_options(),
        )
        .unwrap();
        let node = find_text(tree.root()).unwrap();
        let glyph = node
            .layouted()
            .iter()
            .flat_map(|s| &s.positioned_glyphs)
            .next()
            .unwrap();
        let first_origin = f64::from(glyph.transform().tx);
        assert!(
            (first_origin - (-63.802001953125)).abs() < 0.001,
            "actual first glyph origin: {first_origin}"
        );
        assert!((legacy.lines[0].clusters[0].x - proportional.lines[0].clusters[0].x).abs() > 1.0);
        assert_eq!(proportional.lines[0].clusters.len(), 3);
    }
    use super::*;
    use libre_effects_core::{TextAlign, TextStyleRun};

    fn style(size: f64, color: u32, weight: u16) -> TextCharacterStyle {
        TextCharacterStyle::from_style(
            &TextStyle {
                weight,
                ..Default::default()
            },
            size,
            color,
        )
    }
    fn rich(text: &str, spans: &[(Range<usize>, TextCharacterStyle)]) -> RichText {
        RichText::new(
            text,
            spans[0].1.clone(),
            spans
                .iter()
                .map(|(range, style)| TextStyleRun {
                    start: range.start,
                    end: range.end,
                    style: style.clone(),
                })
                .collect(),
        )
        .unwrap()
    }
    fn raster(body: &str) -> Vec<u8> {
        let svg = svg_document(body, 480.0, 240.0).unwrap();
        let tree = usvg::Tree::from_str(&svg, &crate::fonts::render_options()).unwrap();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(480, 240).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        pixmap.data().to_vec()
    }
    fn alias(weight: u16) -> &'static str {
        crate::fonts::svg_family(&TextStyle {
            weight,
            ..Default::default()
        })
    }
    // Synthetic fixtures are independently authored here. Reference SVG strings
    // do not call the rich serializer or recover expected values from its output.
    #[test]
    fn fixture_a_mixed_size_face_fill_and_stroke_match_independent_svg() {
        let a = style(36.0, 0xff3333, 400);
        let mut b = style(60.0, 0x22dd44, 700);
        b.font_face = "WantedSans-Bold".into();
        b.stroke_enabled = true;
        b.stroke_color = 0x2255ee;
        b.stroke_width = 2.0;
        b.stroke_join = libre_effects_core::TextStrokeJoin::Round;
        let mut c = style(48.0, 0xffffff, 900);
        c.font_face = "WantedSans-Black".into();
        c.fill_enabled = false;
        c.stroke_enabled = true;
        c.stroke_color = 0xffdd22;
        c.stroke_width = 3.0;
        let rich = rich("A B C", &[(0..2, a), (2..4, b), (4..5, c)]);
        let (actual, bounds) = layer_svg(
            "A B C",
            &rich,
            480.0,
            &TextStyle::default(),
            [100.0; 2],
            true,
        )
        .unwrap();
        assert!(bounds.is_none());
        let (regular, bold, black) = (alias(400), alias(700), alias(900));
        let reference = format!(
            "<text x='0' y='60' xml:space='preserve'><tspan font-family='{regular}' font-size='36' fill='none'>A </tspan><tspan font-family='{bold}' font-weight='700' font-size='60' fill='none' stroke='#2255ee' stroke-width='2' stroke-linejoin='round'>B </tspan><tspan font-family='{black}' font-weight='900' font-size='48' fill='none' stroke='#ffdd22' stroke-width='3'>C</tspan></text>\
             <text x='0' y='60' xml:space='preserve'><tspan font-family='{regular}' font-size='36' fill='#ff3333'>A </tspan><tspan font-family='{bold}' font-weight='700' font-size='60' fill='#22dd44'>B </tspan><tspan font-family='{black}' font-weight='900' font-size='48' fill='none'>C</tspan></text>"
        );
        let pixels = raster(&actual);
        assert_eq!(pixels, raster(&reference));
        assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
        let layout = crate::text_edit::layout::Layout::shape_rich(
            "A B C",
            &rich,
            480.0,
            &TextStyle::default(),
        )
        .unwrap();
        assert!(layout.cells.iter().all(|cell| cell.height == 72.0));
        assert_eq!(layout.height_at(layout.caret(2)), 72.0);
        assert!(layout.bounds()[2] > 100.0);
    }
    #[test]
    fn fixture_b_mixed_original_breaks_share_pixels_and_source_carets() {
        let text = "A\r\nB\rC\n";
        let rich = rich(
            text,
            &[
                (0..3, style(24.0, 0xff0000, 400)),
                (3..5, style(40.0, 0x00ff00, 700)),
                (5..7, style(32.0, 0x0000ff, 400)),
            ],
        );
        let composition = compose(text, &rich, 480.0, &TextStyle::default()).unwrap();
        assert_eq!(
            composition
                .lines
                .iter()
                .map(|line| (line.range.clone(), line.terminator.clone()))
                .collect::<Vec<_>>(),
            vec![(0..1, 1..3), (3..4, 4..5), (5..6, 6..7), (7..7, 7..7)]
        );
        let (regular, bold) = (alias(400), alias(700));
        let reference = format!(
            "<text x='0' y='24' font-family='{regular}' font-size='24' fill='#ff0000'>A</text><text x='0' y='68.8' font-family='{bold}' font-weight='700' font-size='40' fill='#00ff00'>B</text><text x='0' y='108.8' font-family='{regular}' font-size='32' fill='#0000ff'>C</text>"
        );
        let actual = layer_svg(text, &rich, 480.0, &TextStyle::default(), [100.0; 2], false)
            .unwrap()
            .0;
        assert_eq!(raster(&actual), raster(&reference));
        let layout =
            crate::text_edit::layout::Layout::shape_rich(text, &rich, 480.0, &TextStyle::default())
                .unwrap();
        assert_eq!(layout.caret(1), layout.caret(2));
        assert!(!layout.carets.iter().any(|(byte, _)| *byte == 2));
        for (byte, top, height) in [
            (0, 0.0, 28.8),
            (3, 28.8, 48.0),
            (5, 76.8, 38.4),
            (7, 115.2, 38.4),
        ] {
            let p = layout.caret(byte);
            assert!((p[1] - top).abs() < 0.0001);
            assert!((layout.height_at(p) - height).abs() < 0.0001);
            assert_eq!(layout.hit([p[0], top + height * 0.4]), byte);
        }
    }
    #[test]
    fn fixture_c_cross_run_kerning_uses_whole_chunk_compositor_origins() {
        let text = "AV";
        let rich = rich(
            text,
            &[
                (0..1, style(72.0, 0xff0000, 400)),
                (1..2, style(72.0, 0x0000ff, 400)),
            ],
        );
        let composition = compose(text, &rich, 480.0, &TextStyle::default()).unwrap();
        let reference = format!(
            "<text x='0' y='72' font-family='{}' font-size='72'>AV</text>",
            alias(400)
        );
        let svg = svg_document(&reference, 480.0, 240.0).unwrap();
        let tree = usvg::Tree::from_str(&svg, &crate::fonts::render_options()).unwrap();
        let node = find_text(tree.root()).unwrap();
        let expected: Vec<_> = node
            .layouted()
            .iter()
            .flat_map(|span| &span.positioned_glyphs)
            .map(|glyph| {
                (
                    glyph.source_range.clone(),
                    f64::from(glyph.baseline_origin().x),
                )
            })
            .collect();
        let actual: Vec<_> = composition.lines[0]
            .clusters
            .iter()
            .map(|cluster| (cluster.range.clone(), cluster.x))
            .collect();
        assert_eq!(actual, expected);
        assert!(
            (composition.lines[0].clusters.last().unwrap().end
                - f64::from(node.bounding_box().right()))
            .abs()
                < 0.001
        );
    }
    #[test]
    fn rich_bidi_and_tracking_share_direct_svg_reference() {
        let text = "ABC אבג 123";
        let mut right = style(36.0, 0x33ccff, 700);
        right.tracking = 100.0;
        let rich = rich(
            text,
            &[(0..4, style(48.0, 0xff6633, 400)), (4..text.len(), right)],
        );
        let style = TextStyle {
            align: TextAlign::Center,
            ..Default::default()
        };
        let reference = format!(
            "<text x='240' y='48' text-anchor='middle' xml:space='preserve'><tspan font-family='{}' font-size='48' fill='#ff6633'>ABC </tspan><tspan font-family='{}' font-weight='700' font-size='36' letter-spacing='3.6' fill='#33ccff'>אבג 123</tspan></text>",
            alias(400),
            alias(700)
        );
        let actual = layer_svg(text, &rich, 480.0, &style, [100.0; 2], false)
            .unwrap()
            .0;
        assert_eq!(raster(&actual), raster(&reference));
        let layout =
            crate::text_edit::layout::Layout::shape_rich(text, &rich, 480.0, &style).unwrap();
        assert!(
            layout
                .cells
                .iter()
                .filter(|c| c.range.start >= 4 && c.range.start < 10)
                .all(|c| c.x1 > c.x2)
        );
        assert!(layout.cells.iter().all(|c| text.is_char_boundary(c.range.start) && text.is_char_boundary(c.range.end)));
    }
    #[test]
    fn missing_fonts_and_split_graphemes_fail_visibly() {
        let mut missing = style(36.0, 0xffffff, 400);
        missing.font_family = "LibreEffects Rich QA Missing Face".into();
        assert!(
            compose(
                "A",
                &rich("A", &[(0..1, missing)]),
                480.0,
                &TextStyle::default()
            )
            .unwrap_err()
            .contains("font unavailable")
        );
        for (text, boundary) in [("e\u{301}", 1), ("A\r\nB", 2), ("👩‍💻", 4)] {
            // Deliberately unchecked only to exercise renderer defense after
            // the public constructor has rejected this unsupported boundary.
            let unchecked = RichText {
                point_origin: false,
                proportional_metrics: false,
                positioning: None,
                default_style: style(36.0, 0xff0000, 400),
                runs: vec![
                    TextStyleRun {
                        start: 0,
                        end: boundary,
                        style: style(36.0, 0xff0000, 400),
                    },
                    TextStyleRun {
                        start: boundary,
                        end: text.len(),
                        style: style(36.0, 0x0000ff, 400),
                    },
                ],
            };
            assert!(
                RichText::new(
                    text,
                    unchecked.default_style.clone(),
                    unchecked.runs.clone()
                )
                .unwrap_err()
                .contains("grapheme")
            );
            assert!(
                compose(text, &unchecked, 480.0, &TextStyle::default())
                    .unwrap_err()
                    .contains("grapheme")
            );
        }
    }
    #[test]
    fn style_boundary_inside_authoritative_ligature_is_rejected() {
        // The shipped regular face has a standard liga for this synthetic word.
        // A compositor-owned cluster must never inherit only its first run.
        let text = "wanted_logo";
        let rich = rich(
            text,
            &[
                (0..6, style(36.0, 0xff0000, 400)),
                (6..11, style(36.0, 0x0000ff, 400)),
            ],
        );
        let error = compose(text, &rich, 480.0, &TextStyle::default()).unwrap_err();
        assert!(error.contains("crosses a shaped cluster"), "{error}");
    }
    #[test]
    fn renderer_cache_retains_multiple_texts_and_invalidates_paint_width_and_metrics() {
        let cache = Arc::new(Mutex::new(CompositionCache::default()));
        let _scope = install_composition_cache(cache.clone());
        let base = TextStyle::default();
        let a = rich("ABC", &[(0..3, style(36., 0xff0000, 400))]);
        let b = rich("XYZ", &[(0..3, style(24., 0x0000ff, 700))]);
        let first = compose("ABC", &a, 480., &base).unwrap();
        let second = compose("XYZ", &b, 480., &base).unwrap();
        let again = compose("ABC", &a, 480., &base).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        assert!(Arc::ptr_eq(
            &second,
            &compose("XYZ", &b, 480., &base).unwrap()
        ));
        let mut changed = a.clone();
        changed.runs[0].style.fill_color = 0x00ff00;
        let painted = compose("ABC", &changed, 480., &base).unwrap();
        assert_ne!(first.fill, painted.fill);
        assert!(!Arc::ptr_eq(
            &first,
            &compose("ABC", &a, 240., &base).unwrap()
        ));
        changed.runs[0].style.font_size = 60.;
        assert_ne!(
            painted.lines[0].clusters[1].x,
            compose("ABC", &changed, 480., &base).unwrap().lines[0].clusters[1].x
        );
        assert!(cache.lock().unwrap().bytes <= 32 * 1024 * 1024);
        cache.lock().unwrap().clear();
        assert_eq!(cache.lock().unwrap().bytes, 0);
    }
    #[test]
    fn paint_changes_do_not_change_layout_but_metrics_changes_invalidate_cache() {
        let text = "ABC";
        let a = rich(
            text,
            &[
                (0..1, style(36.0, 0xff0000, 400)),
                (1..3, style(60.0, 0x0000ff, 700)),
            ],
        );
        let first =
            crate::text_edit::layout::Layout::shape_rich(text, &a, 480.0, &TextStyle::default())
                .unwrap();
        let cached =
            crate::text_edit::layout::Layout::shape_rich(text, &a, 480.0, &TextStyle::default())
                .unwrap();
        assert!(Arc::ptr_eq(&first, &cached));
        let mut painted = a.clone();
        painted.runs[1].style.fill_color = 0x00ff00;
        painted.runs[1].style.stroke_enabled = true;
        painted.runs[1].style.stroke_width = 12.0;
        let second = crate::text_edit::layout::Layout::shape_rich(
            text,
            &painted,
            480.0,
            &TextStyle::default(),
        )
        .unwrap();
        assert_eq!(first.carets, second.carets);
        painted.runs[1].style.font_size = 80.0;
        let larger = crate::text_edit::layout::Layout::shape_rich(
            text,
            &painted,
            480.0,
            &TextStyle::default(),
        )
        .unwrap();
        assert_ne!(second.carets, larger.carets);
        assert_ne!(second.bounds(), larger.bounds());
    }
    #[test]
    fn opacity_retains_unattenuated_geometry_and_whole_pass_order() {
        let text = "A B C";
        let mut s = style(72.0, 0xff0000, 700);
        s.stroke_enabled = true;
        s.stroke_width = 9.0;
        let rich = rich(text, &[(0..text.len(), s)]);
        let full = layer_svg(text, &rich, 480.0, &TextStyle::default(), [100.0; 2], true).unwrap();
        let faded =
            layer_svg(text, &rich, 480.0, &TextStyle::default(), [0.0, 50.0], true).unwrap();
        assert_eq!(faded.1.as_deref(), Some(full.0.as_str()));
        assert!(faded.0.starts_with("<g opacity='0.5'>"));
        assert_ne!(raster(&full.0), raster(&faded.0));
    }
    #[test]
    fn empty_rich_source_has_an_insertion_caret_and_no_pixels() {
        let rich = RichText::new("", style(48.0, 0xff0000, 400), vec![]).unwrap();
        let layout =
            crate::text_edit::layout::Layout::shape_rich("", &rich, 480.0, &TextStyle::default())
                .unwrap();
        assert_eq!(layout.carets, vec![(0, [0.0, 0.0])]);
        assert!((layout.height_at([0.0; 2]) - 57.6).abs() < 0.0001);
        assert!(
            layer_svg("", &rich, 480.0, &TextStyle::default(), [100.0; 2], false)
                .unwrap()
                .0
                .is_empty()
        );
    }
    #[test]
    fn point_origin_and_incoming_leading_share_pixels_and_editor_geometry() {
        use libre_effects_core::TextLeading;
        let text = "A\r\nB\n\n";
        let mut first = style(24.0, 0xff0000, 400);
        first.leading = Some(TextLeading::Fixed(42.0));
        let mut second = style(36.0, 0x0000ff, 400);
        second.leading = Some(TextLeading::Auto(1.5));
        let mut rich = rich(text, &[(0..3, first), (3..6, second)]);
        rich.point_origin = true;
        for (align, anchor) in [
            (TextAlign::Left, "start"),
            (TextAlign::Center, "middle"),
            (TextAlign::Right, "end"),
        ] {
            let style = TextStyle {
                align,
                ..Default::default()
            };
            let actual = layer_svg(text, &rich, 480.0, &style, [100.0; 2], false)
                .unwrap()
                .0;
            let reference = format!(
                "<text x='0' y='0' text-anchor='{anchor}' font-family='{}' font-size='24' fill='#ff0000'>A</text><text x='0' y='54' text-anchor='{anchor}' font-family='{}' font-size='36' fill='#0000ff'>B</text>",
                alias(400),
                alias(400),
            );
            let translated = |body: &str| format!("<g transform='translate(240 60)'>{body}</g>");
            let pixels = raster(&translated(&actual));
            assert_eq!(pixels, raster(&translated(&reference)));
            assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
            let layout =
                crate::text_edit::layout::Layout::shape_rich(text, &rich, 480.0, &style).unwrap();
            let wider =
                crate::text_edit::layout::Layout::shape_rich(text, &rich, 960.0, &style).unwrap();
            assert_eq!(layout.carets, wider.carets);
            assert_eq!(layout.caret(1), layout.caret(2));
            assert!(!layout.carets.iter().any(|(byte, _)| *byte == 2));
            for (byte, top, height) in [
                (0, -24.0, 28.8),
                (3, 18.0, 43.2),
                (5, 72.0, 43.2),
                (6, 126.0, 43.2),
            ] {
                let caret = layout.caret(byte);
                assert!((caret[1] - top).abs() < 0.0001);
                assert!((layout.height_at(caret) - height).abs() < 0.0001);
                assert_eq!(layout.caret_rect(caret)[1], top);
                assert_eq!(layout.hit([caret[0], top + height * 0.4]), byte);
            }
            for cell in &layout.cells {
                let center = [(cell.x1 + cell.x2) / 2.0, cell.y + cell.height * 0.4];
                assert!(layout.contains(center));
                assert_eq!(layout.hit_character(center), cell.range.start);
            }
            assert_eq!(layout.caret(5)[0], 0.0);
            assert_eq!(layout.caret(6)[0], 0.0);
            assert_eq!(layout.bounds()[1], -24.0);
        }
    }
    #[test]
    fn point_origin_and_leading_changes_invalidate_both_composition_caches() {
        let text = "A\nB";
        let mut rich = rich(text, &[(0..text.len(), style(24.0, 0xffffff, 400))]);
        let style = TextStyle::default();
        let legacy = compose(text, &rich, 480.0, &style).unwrap();
        let old_layout =
            crate::text_edit::layout::Layout::shape_rich(text, &rich, 480.0, &style).unwrap();
        rich.point_origin = true;
        let point = compose(text, &rich, 480.0, &style).unwrap();
        let point_layout =
            crate::text_edit::layout::Layout::shape_rich(text, &rich, 480.0, &style).unwrap();
        assert!(!Arc::ptr_eq(&legacy, &point));
        assert!(!Arc::ptr_eq(&old_layout, &point_layout));
        assert_eq!(point.lines[0].y, -24.0);
        rich.runs[0].style.leading = Some(libre_effects_core::TextLeading::Fixed(50.0));
        let lead = compose(text, &rich, 480.0, &style).unwrap();
        let lead_layout =
            crate::text_edit::layout::Layout::shape_rich(text, &rich, 480.0, &style).unwrap();
        assert!(!Arc::ptr_eq(&point, &lead));
        assert!(!Arc::ptr_eq(&point_layout, &lead_layout));
        assert_eq!(lead.lines[1].y, 26.0);
        assert_eq!(lead_layout.caret(2)[1], 26.0);
        let cached = compose(text, &rich, 480.0, &style).unwrap();
        assert!(Arc::ptr_eq(&lead, &cached));
    }
}

#[cfg(test)]
#[path = "../tests/authored_spacing_probe/checks.rs"]
mod authored_spacing_checks;
