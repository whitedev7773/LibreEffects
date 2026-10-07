//! Apply weighted source-unit selectors after usvg shaping and paragraph flow.
//! Font choice, bidi, ligatures and every supported glyph paint format remain
//! authoritative in usvg. Source metadata is never recovered by re-shaping.
use crate::text_animator::Selection;
use libre_effects_core::{TextAnimatorSample, TextStyle};
use resvg::usvg;
use std::{collections::HashMap, ops::Range};

fn line_bases(text: &str, base: usize) -> Vec<usize> {
    libre_effects_core::text_paragraphs::paragraphs(text)
        .map(|paragraph| base + paragraph.range.start)
        .collect()
}

/// Produce the exact existing geometry with source-offset IDs on text nodes.
pub(crate) fn source_geometry_svg(
    text: &str,
    size: f64,
    color: &str,
    width: f64,
    height: f64,
    style: &TextStyle,
) -> Result<String, String> {
    let bases = if style.paragraph {
        let lines = crate::text_flow::lines(text, size, width, style);
        lines
            .iter()
            .take(crate::text_flow::composed_count(&lines, height))
            .flat_map(|line| {
                line_bases(&text[line.range.start..line.visible_end], line.range.start)
            })
            .collect()
    } else {
        line_bases(text, 0)
    };
    let original = crate::rendering::text_geometry_svg(text, size, color, width, height, style);
    let mut parts = original.split("<text ");
    let mut tagged = parts.next().unwrap_or_default().to_owned();
    for base in bases {
        let part = parts
            .next()
            .ok_or_else(|| "Text Animator source line count exceeds rendered nodes".to_string())?;
        tagged.push_str(&format!("<text id='le-animator-{base}' "));
        tagged.push_str(part);
    }
    if parts.next().is_some() {
        return Err("Text Animator rendered nodes exceed source line count".into());
    }
    Ok(tagged)
}

fn source_ranges(node: &usvg::Text, source: &str) -> Result<Vec<Range<usize>>, String> {
    let base: usize = node
        .id()
        .strip_prefix("le-animator-")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            "Text Animator encountered a text node without source identity".to_string()
        })?;
    let normalized: String = node.chunks().iter().map(|chunk| chunk.text()).collect();
    let end = base
        .checked_add(normalized.len())
        .ok_or_else(|| "Text Animator source range overflow".to_string())?;
    let expected = source
        .get(base..end)
        .ok_or_else(|| "Text Animator source range is not a Unicode boundary".to_string())?
        .replace(['\t', '\r', '\n'], " ");
    if normalized != expected {
        return Err("Text Animator could not map normalized renderer text to its source".into());
    }
    node.layouted()
        .iter()
        .flat_map(|span| &span.positioned_glyphs)
        .map(|glyph| {
            let range = &glyph.source_range;
            if range.start >= range.end
                || range.end > normalized.len()
                || !normalized.is_char_boundary(range.start)
                || !normalized.is_char_boundary(range.end)
            {
                return Err(
                    "Text Animator encountered an invalid shaping-cluster source range".into(),
                );
            }
            Ok(base + range.start..base + range.end)
        })
        .collect()
}

/// Scale, then rotate around a layer-local baseline origin. Translation is
/// deliberately separate: Position keeps its original protected-cluster weight.
fn unit_transform(sample: &TextAnimatorSample, weight: f64, pivot: [f32; 2]) -> usvg::Transform {
    let sx = 1.0 + weight * (sample.scale[0] / 100.0 - 1.0);
    let sy = 1.0 + weight * (sample.scale[1] / 100.0 - 1.0);
    let degrees = (weight * sample.rotation).rem_euclid(360.0);
    if sx == 1.0 && sy == 1.0 && degrees == 0.0 {
        return usvg::Transform::default();
    }
    let (sin, cos) = match degrees {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        _ => degrees.to_radians().sin_cos(),
    };
    let (a, b, c, d) = (cos * sx, sin * sx, -sin * sy, cos * sy);
    let (x, y) = (f64::from(pivot[0]), f64::from(pivot[1]));
    usvg::Transform::from_row(
        a as f32,
        b as f32,
        c as f32,
        d as f32,
        (x - a * x - c * y) as f32,
        (y - b * x - d * y) as f32,
    )
}

/// Inspect the full shaped pass before changing any glyph. A source Word/Line
/// can occupy several wrapped text nodes. Its first logical rendered cluster's
/// baseline is shared by every fragment, including visually reordered glyphs.
fn transform_plan(
    tree: &usvg::Tree,
    source: &str,
    selection: &Selection,
    sample: &TextAnimatorSample,
) -> Result<HashMap<String, Vec<usvg::Transform>>, String> {
    fn visit<'a>(group: &'a usvg::Group, nodes: &mut Vec<&'a usvg::Text>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(group) => visit(group, nodes),
                usvg::Node::Text(text) => nodes.push(text),
                _ => {}
            }
        }
    }
    let mut nodes = vec![];
    visit(tree.root(), &mut nodes);
    let mut ranges = vec![];
    let mut origins = vec![];
    let mut counts = vec![];
    for node in &nodes {
        let node_ranges = source_ranges(node, source)?;
        counts.push(node_ranges.len());
        ranges.extend(node_ranges);
        for glyph in node
            .layouted()
            .iter()
            .flat_map(|span| &span.positioned_glyphs)
        {
            let mut origin = glyph.baseline_origin();
            node.abs_transform().map_point(&mut origin);
            origins.push([origin.x, origin.y]);
        }
    }
    let protected = selection.protect(&ranges).ok_or_else(|| {
        "Text Animator could not protect full-pass shaping and grapheme ranges".to_string()
    })?;
    let groups = selection
        .transform_groups(&ranges, &protected)
        .ok_or_else(|| "Text Animator could not join authored transform units".to_string())?;
    let mut pivots: HashMap<usize, usize> = HashMap::new();
    for (index, glyph) in groups.iter().enumerate() {
        let first = pivots.entry(glyph.unit).or_insert(index);
        if ranges[index].start < ranges[*first].start {
            *first = index;
        }
    }
    let mut result = HashMap::new();
    let mut offset = 0;
    for (node, count) in nodes.into_iter().zip(counts) {
        let absolute = node.abs_transform();
        let inverse = absolute.invert().ok_or_else(|| {
            "Text Animator encountered a noninvertible layout transform".to_string()
        })?;
        let transforms = groups[offset..offset + count]
            .iter()
            .map(|glyph| {
                let transform = unit_transform(sample, glyph.weight, origins[pivots[&glyph.unit]]);
                if transform.is_identity() {
                    transform
                } else {
                    // Paragraph offsets remain outside the text node. Conjugation
                    // makes one layer-local transform/pivot apply across all wraps.
                    inverse.pre_concat(transform).pre_concat(absolute)
                }
            })
            .collect();
        if result.insert(node.id().to_owned(), transforms).is_some() {
            return Err("Text Animator encountered duplicate source identities".into());
        }
        offset += count;
    }
    Ok(result)
}

/// Materialize a whole paint pass. Its stroke attributes and paragraph clipping
/// must already surround the text. Pass opacity is applied by the caller later.
pub(crate) fn animate_pass(
    pass: &str,
    source: &str,
    selection: &Selection,
    sample: &TextAnimatorSample,
    width: f64,
    height: f64,
    prefix: &str,
    retain_bounds: bool,
) -> Result<(String, Option<String>), String> {
    if pass.is_empty() {
        return Ok((String::new(), None));
    }
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'>{pass}</svg>",
        width.max(1.0),
        height.max(1.0),
    );
    let tree = usvg::Tree::from_str(&svg, &crate::fonts::render_options())
        .map_err(|error| format!("Text Animator shaping failed: {error}"))?;
    // Keep identity Scale/Rotation on the exact preexisting materialization path.
    let transforms = if sample.scale != [100.0, 100.0] || sample.rotation != 0.0 {
        Some(transform_plan(&tree, source, selection, sample)?)
    } else {
        None
    };
    let materialize = |tree: usvg::Tree, opacity: f64| -> Result<String, String> {
        let mut affected = false;
        let tree = tree.with_text_glyph_effects(|node| {
            let ranges = source_ranges(node, source)?;
            let protected = selection.protect(&ranges).ok_or_else(|| {
                "Text Animator could not protect shaping and grapheme ranges".to_string()
            })?;
            let node_transforms = transforms.as_ref().and_then(|plan| plan.get(node.id()));
            if !protected.iter().any(|glyph| glyph.selected)
                && node_transforms
                    .is_none_or(|items| items.iter().all(usvg::Transform::is_identity))
            {
                return Ok(None);
            }
            affected = true;
            Ok(Some(
                protected
                    .into_iter()
                    .enumerate()
                    .map(|(index, glyph)| usvg::GlyphRenderEffect {
                        unit: glyph.unit,
                        transform: node_transforms
                            .map(|items| items[index])
                            .unwrap_or_default(),
                        dx: (sample.position[0] * glyph.weight) as f32,
                        dy: (sample.position[1] * glyph.weight) as f32,
                        opacity: if glyph.weight == 1.0 {
                            (opacity / 100.0) as f32
                        } else {
                            (1.0 + glyph.weight * (opacity / 100.0 - 1.0)) as f32
                        },
                    })
                    .collect(),
            ))
        })?;
        if !affected {
            return untagged_pass(pass);
        }
        serialize_pass(&tree, prefix)
    };
    let bounds = if retain_bounds && sample.opacity != 100.0 {
        Some(materialize(tree.clone(), 100.0)?)
    } else {
        None
    };
    Ok((materialize(tree, sample.opacity)?, bounds))
}

/// Build every animator against the same original layout. In particular, an
/// earlier Position is transformed by a later Scale/Rotation about that later
/// animator's ORIGINAL source-unit pivot, never a transported/recomputed pivot.
fn stack_plan(
    tree: &usvg::Tree,
    source: &str,
    animators: &[(Selection, &TextAnimatorSample)],
) -> Result<HashMap<String, Vec<usvg::GlyphRenderEffect>>, String> {
    fn visit<'a>(group: &'a usvg::Group, nodes: &mut Vec<&'a usvg::Text>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(group) => visit(group, nodes),
                usvg::Node::Text(text) => nodes.push(text),
                _ => {}
            }
        }
    }
    let mut nodes = vec![];
    visit(tree.root(), &mut nodes);
    let mut ranges = vec![];
    let mut origins = vec![];
    let mut counts = vec![];
    for node in &nodes {
        let node_ranges = source_ranges(node, source)?;
        counts.push(node_ranges.len());
        ranges.extend(node_ranges);
        for glyph in node
            .layouted()
            .iter()
            .flat_map(|span| &span.positioned_glyphs)
        {
            let mut origin = glyph.baseline_origin();
            node.abs_transform().map_point(&mut origin);
            origins.push([origin.x, origin.y]);
        }
    }
    let mut transforms = vec![usvg::Transform::default(); ranges.len()];
    let mut opacities = vec![1.0_f64; ranges.len()];
    let mut units = vec![0; ranges.len()];
    for (selection, sample) in animators {
        let protected = selection.protect(&ranges).ok_or_else(|| {
            "Text Animator could not protect full-pass shaping and grapheme ranges".to_string()
        })?;
        let groups = selection
            .transform_groups(&ranges, &protected)
            .ok_or_else(|| "Text Animator could not join authored transform units".to_string())?;
        let mut pivots: HashMap<usize, usize> = HashMap::new();
        for (index, glyph) in groups.iter().enumerate() {
            let first = pivots.entry(glyph.unit).or_insert(index);
            if ranges[index].start < ranges[*first].start {
                *first = index;
            }
        }
        for (index, (glyph, group)) in protected.iter().zip(&groups).enumerate() {
            // Protected-unit identities depend on source ranges, never selector
            // units/weights, and are consequently identical for every animator.
            units[index] = glyph.unit;
            let around_pivot = unit_transform(sample, group.weight, origins[pivots[&group.unit]]);
            let step = usvg::Transform::from_translate(
                (sample.position[0] * glyph.weight) as f32,
                (sample.position[1] * glyph.weight) as f32,
            )
            .pre_concat(around_pivot);
            transforms[index] = step.pre_concat(transforms[index]);
            opacities[index] *= if glyph.weight == 1.0 {
                sample.opacity / 100.0
            } else {
                1.0 + glyph.weight * (sample.opacity / 100.0 - 1.0)
            };
        }
    }
    let mut result = HashMap::new();
    let mut offset = 0;
    for (node, count) in nodes.into_iter().zip(counts) {
        let absolute = node.abs_transform();
        let inverse = absolute.invert().ok_or_else(|| {
            "Text Animator encountered a noninvertible layout transform".to_string()
        })?;
        let effects = (offset..offset + count)
            .map(|index| {
                let mut transform = transforms[index];
                if !transform.is_identity() {
                    // Conjugate only after composing the complete layer-local
                    // product. Paragraph offsets/clips remain in their old place.
                    transform = inverse.pre_concat(transform).pre_concat(absolute);
                }
                let (transform, dx, dy) = if transform.sx == 1.0
                    && transform.sy == 1.0
                    && transform.kx == 0.0
                    && transform.ky == 0.0
                {
                    // Keep translation-only stacks on upstream outline aggregation.
                    (usvg::Transform::default(), transform.tx, transform.ty)
                } else {
                    (transform, 0.0, 0.0)
                };
                usvg::GlyphRenderEffect {
                    unit: units[index],
                    transform,
                    dx,
                    dy,
                    opacity: opacities[index] as f32,
                }
            })
            .collect();
        if result.insert(node.id().to_owned(), effects).is_some() {
            return Err("Text Animator encountered duplicate source identities".into());
        }
        offset += count;
    }
    Ok(result)
}

/// Compose the complete ordered stack on authoritative shaped glyphs and then
/// materialize exactly once. Whole fill/stroke ordering and paragraph clips are
/// retained; the optional bounds pass removes ONLY opacity attenuation.
pub(crate) fn animate_stack_pass(
    pass: &str,
    source: &str,
    animators: &[(Selection, &TextAnimatorSample)],
    width: f64,
    height: f64,
    prefix: &str,
    retain_bounds: bool,
) -> Result<(String, Option<String>), String> {
    if pass.is_empty() {
        return Ok((String::new(), None));
    }
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'>{pass}</svg>",
        width.max(1.0),
        height.max(1.0),
    );
    let tree = usvg::Tree::from_str(&svg, &crate::fonts::render_options())
        .map_err(|error| format!("Text Animator shaping failed: {error}"))?;
    let plan = stack_plan(&tree, source, animators)?;
    let materialize = |tree: usvg::Tree, opaque: bool| -> Result<String, String> {
        let mut affected = false;
        let tree = tree.with_text_glyph_effects(|node| {
            let mut effects = plan
                .get(node.id())
                .ok_or_else(|| "Text Animator stack is missing a shaped node".to_string())?
                .clone();
            if opaque {
                for effect in &mut effects {
                    effect.opacity = 1.0;
                }
            }
            if effects.iter().all(|effect| {
                effect.transform.is_identity()
                    && effect.dx == 0.0
                    && effect.dy == 0.0
                    && effect.opacity == 1.0
            }) {
                return Ok(None);
            }
            affected = true;
            Ok(Some(effects))
        })?;
        if !affected {
            return untagged_pass(pass);
        }
        serialize_pass(&tree, prefix)
    };
    let bounds = if retain_bounds && animators.iter().any(|(_, sample)| sample.opacity != 100.0) {
        Some(materialize(tree.clone(), true)?)
    } else {
        None
    };
    Ok((materialize(tree, false)?, bounds))
}

fn serialize_pass(tree: &usvg::Tree, prefix: &str) -> Result<String, String> {
    check_serialization_budget(tree)?;
    // The opt-in writer preserves exact f32 geometry and resource identity.
    // The document wrapper must not introduce a point-text viewport clip.
    let output = tree.to_string_with_unique_resource_ids(&usvg::WriteOptions {
        id_prefix: Some(prefix.into()),
        indent: usvg::Indent::None,
        ..Default::default()
    });
    if output.len() > crate::rendering::SVG_LIMIT {
        return Err("Text Animator expanded SVG exceeds 64 MiB; simplify the text".into());
    }
    let start = output
        .find('>')
        .ok_or_else(|| "Text Animator SVG has no root".to_string())?
        + 1;
    let end = output
        .rfind("</svg>")
        .ok_or_else(|| "Text Animator SVG has no closing root".to_string())?;
    Ok(output[start..end].to_owned())
}

fn untagged_pass(pass: &str) -> Result<String, String> {
    let mut parts = pass.split("<text id='le-animator-");
    let mut original = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        let (_, rest) = part
            .split_once("' ")
            .ok_or_else(|| "Text Animator source tag is malformed".to_string())?;
        original.push_str("<text ");
        original.push_str(rest);
    }
    Ok(original)
}

#[cfg(test)]
pub(crate) fn source_clusters(
    text: &str,
    size: f64,
    width: f64,
    style: &TextStyle,
) -> Result<Vec<Range<usize>>, String> {
    let content = source_geometry_svg(text, size, "white", width, size * 100.0, style)?;
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'>{content}</svg>",
        width.max(1.0),
        size * 100.0
    );
    let tree =
        usvg::Tree::from_str(&svg, &crate::fonts::render_options()).map_err(|e| e.to_string())?;
    let mut result = vec![];
    tree.with_text_glyph_effects(|node| {
        result.extend(source_ranges(node, text)?);
        Ok(None)
    })?;
    Ok(result)
}

// Preflight before usvg's String serializer can expand embedded bitmap glyphs
// or large outline sets. Deliberately conservative per-node/path overhead keeps
// the new materialization step within the existing 64 MiB SVG resource ceiling.
pub(crate) fn check_serialization_budget(tree: &usvg::Tree) -> Result<(), String> {
    fn add(total: &mut usize, amount: usize) -> Result<(), String> {
        *total = total
            .checked_add(amount)
            .filter(|sum| *sum <= crate::rendering::SVG_LIMIT)
            .ok_or_else(|| {
                "Text Animator expanded geometry exceeds the 64 MiB serialization budget"
                    .to_string()
            })?;
        Ok(())
    }
    fn visit_group(group: &usvg::Group, total: &mut usize) -> Result<(), String> {
        add(total, 1024)?;
        for node in group.children() {
            add(total, 1024)?;
            match node {
                usvg::Node::Group(group) => visit_group(group, total)?,
                usvg::Node::Text(text) => visit_group(text.flattened(), total)?,
                usvg::Node::Path(path) => {
                    add(total, path.data().points().len().saturating_mul(128))?
                }
                usvg::Node::Image(image) => match image.kind() {
                    usvg::ImageKind::JPEG(data)
                    | usvg::ImageKind::PNG(data)
                    | usvg::ImageKind::GIF(data)
                    | usvg::ImageKind::WEBP(data) => {
                        add(
                            total,
                            data.len()
                                .saturating_add(2)
                                .saturating_div(3)
                                .saturating_mul(4),
                        )?;
                    }
                    usvg::ImageKind::SVG(tree) => {
                        let mut embedded = 0;
                        visit_tree(tree, &mut embedded)?;
                        add(
                            total,
                            embedded
                                .saturating_add(2)
                                .saturating_div(3)
                                .saturating_mul(4),
                        )?;
                    }
                },
            }
        }
        Ok(())
    }
    fn visit_tree(tree: &usvg::Tree, total: &mut usize) -> Result<(), String> {
        visit_group(tree.root(), total)?;
        for gradient in tree.linear_gradients() {
            add(total, 1024 + gradient.stops().len().saturating_mul(256))?;
        }
        for gradient in tree.radial_gradients() {
            add(total, 1024 + gradient.stops().len().saturating_mul(256))?;
        }
        for pattern in tree.patterns() {
            visit_group(pattern.root(), total)?;
        }
        for clip in tree.clip_paths() {
            visit_group(clip.root(), total)?;
        }
        for mask in tree.masks() {
            visit_group(mask.root(), total)?;
        }
        for filter in tree.filters() {
            add(total, 1024 + filter.primitives().len().saturating_mul(2048))?;
            for primitive in filter.primitives() {
                if let usvg::filter::Kind::Image(image) = primitive.kind() {
                    visit_group(image.root(), total)?;
                }
            }
        }
        Ok(())
    }
    visit_tree(tree, &mut 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn animator_stack_order_composes_translation_before_later_original_pivot_affine() {
        let source = "A";
        let tree = usvg::Tree::from_str("<svg xmlns='http://www.w3.org/2000/svg' width='200' height='200'><text id='le-animator-0' x='10' y='30' font-size='24'>A</text></svg>", &crate::fonts::render_options()).unwrap();
        let translate = TextAnimatorSample {
            position: [8., 4.],
            ..Default::default()
        };
        let affine = TextAnimatorSample {
            scale: [200., 50.],
            rotation: 90.,
            ..Default::default()
        };
        let plans = [[&translate, &affine], [&affine, &translate]].map(|samples| {
            let active: Vec<_> = samples
                .into_iter()
                .map(|sample| (Selection::new(source, sample), sample))
                .collect();
            stack_plan(&tree, source, &active).unwrap()
        });
        // Independent arithmetic: p=(12,34), pivot=(10,30).
        // T then RS: (20,38) -> (20,34) -> (6,50).
        // RS then T: (14,32) -> (8,34) -> (16,38).
        for (plan, matrix, expected) in [
            (&plans[0], [0., 2., -0.5, 0., 23., 26.], [6., 50.]),
            (&plans[1], [0., 2., -0.5, 0., 33., 14.], [16., 38.]),
        ] {
            let effect = plan["le-animator-0"][0];
            assert_eq!(
                effect.transform,
                usvg::Transform::from_row(
                    matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]
                )
            );
            let mut point = usvg::tiny_skia_path::Point::from_xy(12., 34.);
            effect.transform.map_point(&mut point);
            assert_eq!([point.x, point.y], expected);
            assert_eq!([effect.dx, effect.dy, effect.opacity], [0., 0., 1.]);
        }
    }

    #[test]
    fn animator_stack_original_word_pivots_span_wraps_with_independent_line_weights() {
        use libre_effects_core::{TextSelectorShape, TextSelectorUnits};
        let source = "ABCD\nEF";
        let tree = usvg::Tree::from_str("<svg xmlns='http://www.w3.org/2000/svg' width='300' height='200'><text id='le-animator-0' x='0' y='24' font-size='24'>AB</text><g transform='translate(0 30)'><text id='le-animator-2' x='0' y='24' font-size='24'>CD</text></g><g transform='translate(0 60)'><text id='le-animator-5' x='0' y='24' font-size='24'>EF</text></g></svg>", &crate::fonts::render_options()).unwrap();
        let lines = TextAnimatorSample {
            units: TextSelectorUnits::Lines,
            shape: TextSelectorShape::RampUp,
            position: [16., 0.],
            opacity: 0.,
            ..Default::default()
        };
        let words = TextAnimatorSample {
            units: TextSelectorUnits::Words,
            scale: [200., 50.],
            rotation: 90.,
            opacity: 50.,
            ..Default::default()
        };
        let active: Vec<_> = [&lines, &words]
            .into_iter()
            .map(|sample| (Selection::new(source, sample), sample))
            .collect();
        let plan = stack_plan(&tree, source, &active).unwrap();
        // Hard line weights are 1/4 and 3/4 despite three visual rows.
        // Original word pivots are (0,24) and (0,84), shared across wraps.
        for (id, matrix, opacity) in [
            ("le-animator-0", [0., 2., -0.5, 0., 12., 32.], 0.375),
            ("le-animator-2", [0., 2., -0.5, 0., -3., 2.], 0.375),
            ("le-animator-5", [0., 2., -0.5, 0., 12., 48.], 0.125),
        ] {
            for effect in &plan[id] {
                assert_eq!(
                    effect.transform,
                    usvg::Transform::from_row(
                        matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]
                    ),
                    "{id}"
                );
                assert_eq!(effect.opacity, opacity, "{id}");
            }
        }
    }

    #[test]
    fn animator_unit_affine_weights_scale_and_rotation_around_baseline() {
        let sample = TextAnimatorSample {
            scale: [75.0, 200.0],
            rotation: 180.0,
            position: [100.0, -40.0],
            ..Default::default()
        };
        let transform = unit_transform(&sample, 0.5, [10.0, 20.0]);
        let mut pivot = usvg::tiny_skia_path::Point::from_xy(10.0, 20.0);
        transform.map_point(&mut pivot);
        assert_eq!(pivot, usvg::tiny_skia_path::Point::from_xy(10.0, 20.0));
        let mut point = usvg::tiny_skia_path::Point::from_xy(12.0, 24.0);
        transform.map_point(&mut point);
        assert_eq!(point, usvg::tiny_skia_path::Point::from_xy(4.0, 21.75));
        assert_eq!(
            unit_transform(&sample, 0.0, [10.0, 20.0]),
            usvg::Transform::default()
        );
        let zero = TextAnimatorSample {
            scale: [0.0, 0.0],
            ..Default::default()
        };
        assert_eq!(
            unit_transform(&zero, 1.0, [10.0, 20.0]),
            usvg::Transform::from_row(0., 0., 0., 0., 10., 20.)
        );
        assert_eq!(
            unit_transform(&zero, 0.5, [10.0, 20.0]),
            usvg::Transform::from_row(0.5, 0., 0., 0.5, 5., 10.)
        );
    }

    #[test]
    fn animator_transform_plan_shares_logical_word_pivot_across_reordered_nodes() {
        use libre_effects_core::TextSelectorUnits;
        let source = "ABCD";
        // Deliberately paint the later source fragment first. Each node also
        // has a different paragraph offset, as happens with visual wrapping.
        let tree = usvg::Tree::from_str("<svg xmlns='http://www.w3.org/2000/svg' width='300' height='200'><g transform='translate(60 80)'><text id='le-animator-2' x='10' y='30' font-size='24'>CD</text></g><g transform='translate(20 0)'><text id='le-animator-0' x='10' y='30' font-size='24'>AB</text></g></svg>", &crate::fonts::render_options()).unwrap();
        let sample = TextAnimatorSample {
            units: TextSelectorUnits::Words,
            scale: [200.0, 50.0],
            rotation: 90.0,
            ..Default::default()
        };
        let plan =
            transform_plan(&tree, source, &Selection::new(source, &sample), &sample).unwrap();
        let expected = unit_transform(&sample, 1.0, [30.0, 30.0]);
        for (id, offset) in [
            ("le-animator-0", [20.0, 0.0]),
            ("le-animator-2", [60.0, 80.0]),
        ] {
            let absolute = usvg::Transform::from_translate(offset[0], offset[1]);
            for transform in &plan[id] {
                let global = absolute
                    .pre_concat(*transform)
                    .pre_concat(absolute.invert().unwrap());
                assert_eq!(global, expected);
            }
        }
    }

    #[test]
    fn animator_transform_plan_preserves_authored_lines_across_visual_wraps() {
        use libre_effects_core::{TextSelectorShape, TextSelectorUnits};
        let source = "ABCD\nEF";
        let tree = usvg::Tree::from_str("<svg xmlns='http://www.w3.org/2000/svg' width='300' height='200'><text id='le-animator-0' x='10' y='30' font-size='24'>AB</text><text id='le-animator-2' x='10' y='60' font-size='24'>CD</text><text id='le-animator-5' x='10' y='90' font-size='24'>EF</text></svg>", &crate::fonts::render_options()).unwrap();
        let sample = TextAnimatorSample {
            units: TextSelectorUnits::Lines,
            shape: TextSelectorShape::RampUp,
            scale: [200.0, 200.0],
            ..Default::default()
        };
        let plan =
            transform_plan(&tree, source, &Selection::new(source, &sample), &sample).unwrap();
        for id in ["le-animator-0", "le-animator-2"] {
            assert!(
                plan[id]
                    .iter()
                    .all(|transform| *transform == unit_transform(&sample, 0.25, [10.0, 30.0]))
            );
        }
        assert!(
            plan["le-animator-5"]
                .iter()
                .all(|transform| *transform == unit_transform(&sample, 0.75, [10.0, 90.0]))
        );
    }

    #[test]
    fn animator_source_ranges_retain_crlf_tabs_and_paragraph_trim_offsets() {
        let ranges = source_clusters("A\r\nB\t C", 32.0, 600.0, &TextStyle::default()).unwrap();
        assert_eq!(ranges, [0..1, 3..4, 4..5, 5..6, 6..7]);
        let style = TextStyle {
            paragraph: true,
            ..Default::default()
        };
        let ranges = source_clusters("A  \r\nB", 32.0, 600.0, &style).unwrap();
        assert_eq!(ranges, [0..1, 5..6]);
    }

    #[test]
    fn animator_source_ids_and_clusters_retain_mixed_paragraph_offsets() {
        let text = "A\r日本語\r\n한국어\n\rB\r\n";
        let paragraphs: Vec<_> = libre_effects_core::text_paragraphs::paragraphs(text).collect();
        assert_eq!(
            line_bases(text, 10),
            paragraphs
                .iter()
                .map(|p| 10 + p.range.start)
                .collect::<Vec<_>>()
        );
        for paragraph in [false, true] {
            let style = TextStyle {
                paragraph,
                ..Default::default()
            };
            let svg = source_geometry_svg(text, 24.0, "white", 600.0, 1000.0, &style).unwrap();
            assert_eq!(
                svg.matches("<text id='le-animator-").count(),
                paragraphs.len()
            );
            let clusters = source_clusters(text, 24.0, 600.0, &style).unwrap();
            assert!(!clusters.is_empty());
            for range in clusters {
                assert!(text.is_char_boundary(range.start));
                assert!(text.is_char_boundary(range.end));
                assert!(
                    paragraphs
                        .iter()
                        .any(|p| range.start >= p.range.start && range.end <= p.range.end)
                );
                assert!(!text[range].contains(['\r', '\n']));
            }
        }
    }
    #[test]
    fn animator_source_only_selector_returns_exact_legacy_pass() {
        let text = "A\nB";
        let style = TextStyle::default();
        let original =
            crate::rendering::text_geometry_svg(text, 32.0, "white", 200.0, 120.0, &style);
        let tagged = source_geometry_svg(text, 32.0, "white", 200.0, 120.0, &style).unwrap();
        let sample = TextAnimatorSample {
            start: 40.0,
            end: 60.0,
            position: [100.0, 40.0],
            opacity: 0.0,
            ..Default::default()
        };
        let selection = Selection::new(text, &sample);
        let (paint, bounds) = animate_pass(
            &tagged, text, &selection, &sample, 200.0, 120.0, "qa-", true,
        )
        .unwrap();
        assert_eq!(paint, original);
        assert_eq!(bounds.as_deref(), Some(original.as_str()));
    }
}
