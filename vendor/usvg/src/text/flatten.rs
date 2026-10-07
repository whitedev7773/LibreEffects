// Copyright 2022 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::mem;
use std::sync::Arc;

use fontdb::{Database, ID};
use rustybuzz::ttf_parser;
use rustybuzz::ttf_parser::{GlyphId, RasterImageFormat, RgbaColor};
use tiny_skia_path::{NonZeroRect, Size, Transform};
use xmlwriter::XmlWriter;

use crate::text::colr::GlyphPainter;
use crate::*;

fn resolve_rendering_mode(text: &Text) -> ShapeRendering {
    match text.rendering_mode {
        TextRendering::OptimizeSpeed => ShapeRendering::CrispEdges,
        TextRendering::OptimizeLegibility => ShapeRendering::GeometricPrecision,
        TextRendering::GeometricPrecision => ShapeRendering::GeometricPrecision,
    }
}

fn push_outline_paths(
    span: &layout::Span,
    builder: &mut tiny_skia_path::PathBuilder,
    new_children: &mut Vec<Node>,
    rendering_mode: ShapeRendering,
) {
    let builder = mem::replace(builder, tiny_skia_path::PathBuilder::new());

    if let Some(path) = builder.finish().and_then(|p| {
        Path::new(
            String::new(),
            span.visible,
            span.fill.clone(),
            span.stroke.clone(),
            span.paint_order,
            rendering_mode,
            Arc::new(p),
            Transform::default(),
        )
    }) {
        new_children.push(Node::Path(Box::new(path)));
    }
}

pub(crate) fn flatten(text: &mut Text, fontdb: &fontdb::Database) -> Option<(Group, NonZeroRect)> {
    flatten_spans(
        &text.layouted,
        &text.id,
        resolve_rendering_mode(text),
        fontdb,
    )
}

fn flatten_spans(
    spans: &[layout::Span],
    id: &str,
    rendering_mode: ShapeRendering,
    fontdb: &fontdb::Database,
) -> Option<(Group, NonZeroRect)> {
    let mut new_children = vec![];

    for span in spans {
        if let Some(path) = span.overline.as_ref() {
            let mut path = path.clone();
            path.rendering_mode = rendering_mode;
            new_children.push(Node::Path(Box::new(path)));
        }

        if let Some(path) = span.underline.as_ref() {
            let mut path = path.clone();
            path.rendering_mode = rendering_mode;
            new_children.push(Node::Path(Box::new(path)));
        }

        // Instead of always processing each glyph separately, we always collect
        // as many outline glyphs as possible by pushing them into the span_builder
        // and only if we encounter a different glyph, or we reach the very end of the
        // span to we push the actual outline paths into new_children. This way, we don't need
        // to create a new path for every glyph if we have many consecutive glyphs
        // with just outlines (which is the most common case).
        let mut span_builder = tiny_skia_path::PathBuilder::new();

        for glyph in &span.positioned_glyphs {
            // A (best-effort conversion of a) COLR glyph.
            if let Some(tree) = fontdb.colr(glyph.font, glyph.id) {
                let mut group = Group {
                    transform: glyph.colr_transform(),
                    ..Group::empty()
                };
                // TODO: Probably need to update abs_transform of children?
                group.children.push(Node::Group(Box::new(tree.root)));
                group.calculate_bounding_boxes();

                new_children.push(Node::Group(Box::new(group)));
            }
            // An SVG glyph. Will return the usvg node containing the glyph descriptions.
            else if let Some(node) = fontdb.svg(glyph.font, glyph.id) {
                push_outline_paths(span, &mut span_builder, &mut new_children, rendering_mode);

                let mut group = Group {
                    transform: glyph.svg_transform(),
                    ..Group::empty()
                };
                // TODO: Probably need to update abs_transform of children?
                group.children.push(node);
                group.calculate_bounding_boxes();

                new_children.push(Node::Group(Box::new(group)));
            }
            // A bitmap glyph.
            else if let Some(img) = fontdb.raster(glyph.font, glyph.id) {
                push_outline_paths(span, &mut span_builder, &mut new_children, rendering_mode);

                let transform = if img.is_sbix {
                    glyph.sbix_transform(
                        img.x as f32,
                        img.y as f32,
                        img.glyph_bbox.map(|bbox| bbox.x_min).unwrap_or(0) as f32,
                        img.glyph_bbox.map(|bbox| bbox.y_min).unwrap_or(0) as f32,
                        img.pixels_per_em as f32,
                        img.image.size.height(),
                    )
                } else {
                    glyph.cbdt_transform(
                        img.x as f32,
                        img.y as f32,
                        img.pixels_per_em as f32,
                        img.image.size.height(),
                    )
                };

                let mut group = Group {
                    transform,
                    ..Group::empty()
                };
                group.children.push(Node::Image(Box::new(img.image)));
                group.calculate_bounding_boxes();

                new_children.push(Node::Group(Box::new(group)));
            } else if let Some(outline) = fontdb
                .outline(glyph.font, glyph.id)
                .and_then(|p| p.transform(glyph.outline_transform()))
            {
                span_builder.push_path(&outline);
            }
        }

        push_outline_paths(span, &mut span_builder, &mut new_children, rendering_mode);

        if let Some(path) = span.line_through.as_ref() {
            let mut path = path.clone();
            path.rendering_mode = rendering_mode;
            new_children.push(Node::Path(Box::new(path)));
        }
    }

    let mut group = Group {
        id: id.to_owned(),
        ..Group::empty()
    };

    for child in new_children {
        group.children.push(child);
    }

    group.calculate_bounding_boxes();
    let stroke_bbox = group.stroke_bounding_box().to_non_zero_rect()?;
    Some((group, stroke_bbox))
}

struct PathBuilder {
    builder: tiny_skia_path::PathBuilder,
}

impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

pub(crate) trait DatabaseExt {
    fn outline(&self, id: ID, glyph_id: GlyphId) -> Option<tiny_skia_path::Path>;
    fn raster(&self, id: ID, glyph_id: GlyphId) -> Option<BitmapImage>;
    fn svg(&self, id: ID, glyph_id: GlyphId) -> Option<Node>;
    fn colr(&self, id: ID, glyph_id: GlyphId) -> Option<Tree>;
}

pub(crate) struct BitmapImage {
    image: Image,
    x: i16,
    y: i16,
    pixels_per_em: u16,
    glyph_bbox: Option<ttf_parser::Rect>,
    is_sbix: bool,
}

impl DatabaseExt for Database {
    #[inline(never)]
    fn outline(&self, id: ID, glyph_id: GlyphId) -> Option<tiny_skia_path::Path> {
        self.with_face_data(id, |data, face_index| -> Option<tiny_skia_path::Path> {
            let font = ttf_parser::Face::parse(data, face_index).ok()?;

            let mut builder = PathBuilder {
                builder: tiny_skia_path::PathBuilder::new(),
            };

            font.outline_glyph(glyph_id, &mut builder)?;
            builder.builder.finish()
        })?
    }

    fn raster(&self, id: ID, glyph_id: GlyphId) -> Option<BitmapImage> {
        self.with_face_data(id, |data, face_index| -> Option<BitmapImage> {
            let font = ttf_parser::Face::parse(data, face_index).ok()?;
            let image = font.glyph_raster_image(glyph_id, u16::MAX)?;

            if image.format == RasterImageFormat::PNG {
                let bitmap_image = BitmapImage {
                    image: Image {
                        id: String::new(),
                        visible: true,
                        size: Size::from_wh(image.width as f32, image.height as f32)?,
                        rendering_mode: ImageRendering::OptimizeQuality,
                        kind: ImageKind::PNG(Arc::new(image.data.into())),
                        abs_transform: Transform::default(),
                        abs_bounding_box: NonZeroRect::from_xywh(
                            0.0,
                            0.0,
                            image.width as f32,
                            image.height as f32,
                        )?,
                    },
                    x: image.x,
                    y: image.y,
                    pixels_per_em: image.pixels_per_em,
                    glyph_bbox: font.glyph_bounding_box(glyph_id),
                    // ttf-parser always checks sbix first, so if this table exists, it was used.
                    is_sbix: font.tables().sbix.is_some(),
                };

                return Some(bitmap_image);
            }

            None
        })?
    }

    fn svg(&self, id: ID, glyph_id: GlyphId) -> Option<Node> {
        // TODO: Technically not 100% accurate because the SVG format in a OTF font
        // is actually a subset/superset of a normal SVG, but it seems to work fine
        // for Twitter Color Emoji, so might as well use what we already have.

        // TODO: Glyph records can contain the data for multiple glyphs. We should
        // add a cache so we don't need to reparse the data every time.
        self.with_face_data(id, |data, face_index| -> Option<Node> {
            let font = ttf_parser::Face::parse(data, face_index).ok()?;
            let image = font.glyph_svg_image(glyph_id)?;
            let tree = Tree::from_data(image.data, &Options::default()).ok()?;

            // Twitter Color Emoji seems to always have one SVG record per glyph,
            // while Noto Color Emoji sometimes contains multiple ones. It's kind of hacky,
            // but the best we have for now.
            let node = if image.start_glyph_id == image.end_glyph_id {
                Node::Group(Box::new(tree.root))
            } else {
                tree.node_by_id(&format!("glyph{}", glyph_id.0))
                    .log_none(|| {
                        log::warn!("Failed to find SVG glyph node for glyph {}", glyph_id.0)
                    })
                    .cloned()?
            };

            Some(node)
        })?
    }

    fn colr(&self, id: ID, glyph_id: GlyphId) -> Option<Tree> {
        self.with_face_data(id, |data, face_index| -> Option<Tree> {
            let face = ttf_parser::Face::parse(data, face_index).ok()?;

            let mut svg = XmlWriter::new(xmlwriter::Options::default());

            svg.start_element("svg");
            svg.write_attribute("xmlns", "http://www.w3.org/2000/svg");
            svg.write_attribute("xmlns:xlink", "http://www.w3.org/1999/xlink");

            let mut path_buf = String::with_capacity(256);
            let gradient_index = 1;
            let clip_path_index = 1;

            svg.start_element("g");

            let mut glyph_painter = GlyphPainter {
                face: &face,
                svg: &mut svg,
                path_buf: &mut path_buf,
                gradient_index,
                clip_path_index,
                palette_index: 0,
                transform: ttf_parser::Transform::default(),
                outline_transform: ttf_parser::Transform::default(),
                transforms_stack: vec![ttf_parser::Transform::default()],
            };

            face.paint_color_glyph(
                glyph_id,
                0,
                RgbaColor::new(0, 0, 0, 255),
                &mut glyph_painter,
            )?;
            svg.end_element();

            Tree::from_data(svg.end_document().as_bytes(), &Options::default()).ok()
        })?
    }
}

/// Apply post-layout effects without choosing fonts or shaping text again.
pub(crate) fn flatten_with_effects(
    text: &Text,
    effects: &[crate::GlyphRenderEffect],
    fontdb: &fontdb::Database,
) -> Result<(Group, Rect), String> {
    let count: usize = text
        .layouted
        .iter()
        .map(|span| span.positioned_glyphs.len())
        .sum();
    if effects.len() != count {
        return Err("Text glyph effect count does not match the shaped layout".into());
    }
    if effects.iter().any(|effect| {
        !effect.transform.is_finite()
            || !effect.dx.is_finite()
            || !effect.dy.is_finite()
            || !effect.opacity.is_finite()
            || !(0.0..=1.0).contains(&effect.opacity)
    }) {
        return Err(
            "Text glyph effect has an invalid affine transform, translation or opacity".into(),
        );
    }
    // A protected unit receives one opacity operation. If a caller's unit
    // recurs across a different span or a disjoint visual run, reject it rather
    // than silently applying opacity twice or changing paint order.
    let mut seen =
        std::collections::HashMap::<usize, (crate::GlyphRenderEffect, usize, usize)>::new();
    let mut glyph_index = 0;
    for (span_index, span) in text.layouted.iter().enumerate() {
        for _ in &span.positioned_glyphs {
            let effect = effects[glyph_index];
            if let Some((prior, prior_span, prior_index)) = seen.get(&effect.unit) {
                if prior.transform != effect.transform
                    || prior.dx != effect.dx
                    || prior.dy != effect.dy
                    || prior.opacity != effect.opacity
                {
                    return Err("A protected text unit has inconsistent glyph effects".into());
                }
                if effect.opacity < 1.0
                    && (*prior_span != span_index || *prior_index + 1 != glyph_index)
                {
                    return Err(
                        "An attenuated protected text unit is not one contiguous paint run".into(),
                    );
                }
            }
            seen.insert(effect.unit, (effect, span_index, glyph_index));
            glyph_index += 1;
        }
    }
    if let Some(first) = effects.first().filter(|first| {
        effects.iter().all(|effect| {
            effect.transform == first.transform
                && effect.dx == first.dx
                && effect.dy == first.dy
                && effect.opacity == first.opacity
                && (effect.opacity == 1.0 || effect.unit == first.unit)
        })
    }) {
        // A uniform opaque node, or one complete protected unit, can retain
        // the exact original outlines/paint. Transform the finished group so
        // floating-point reassociation cannot alter a full-selection result.
        let mut group = text.flattened.as_ref().clone();
        let transform = Transform::from_translate(first.dx, first.dy).pre_concat(first.transform);
        if collapsed(transform) {
            return Ok(empty_effect_geometry(&text.id));
        }
        group.transform = transform.pre_concat(group.transform);
        group.opacity = Opacity::new_clamped(group.opacity.get() * first.opacity);
        let bounds = text
            .stroke_bounding_box
            .to_non_zero_rect()
            .and_then(|bounds| bounds.transform(transform))
            .ok_or_else(|| "Text glyph effects produced invalid transformed bounds".to_string())?;
        return Ok((group, bounds.to_rect()));
    }
    if effects.iter().any(|effect| !effect.transform.is_identity()) {
        return flatten_affine_effects(text, effects, fontdb);
    }
    let mut spans = text.layouted.clone();
    for (glyph, effect) in spans
        .iter_mut()
        .flat_map(|span| &mut span.positioned_glyphs)
        .zip(effects)
    {
        glyph.translate(effect.dx, effect.dy);
    }
    let rendering_mode = resolve_rendering_mode(text);
    // The ordinary opaque path keeps upstream path aggregation, even when
    // neighboring glyphs have different translations.
    if effects.iter().all(|effect| effect.opacity == 1.0) {
        return flatten_spans(&spans, &text.id, rendering_mode, fontdb)
            .map(|(group, bounds)| (group, bounds.to_rect()))
            .ok_or_else(|| "Text glyph effects produced invalid geometry".into());
    }
    let mut root = Group {
        id: text.id.clone(),
        ..Group::empty()
    };
    let mut offset = 0;
    for span in &spans {
        let count = span.positioned_glyphs.len();
        let span_effects = &effects[offset..offset + count];
        offset += count;
        let mut start = 0;
        while start < count {
            let effect = span_effects[start];
            let mut end = start + 1;
            while end < count
                && span_effects[end].opacity == effect.opacity
                && (effect.opacity == 1.0 || span_effects[end].unit == effect.unit)
            {
                end += 1;
            }
            // Do not clone the complete span for every unit: source text can
            // contain thousands of independently attenuated glyphs.
            let piece = layout::Span {
                fill: span.fill.clone(),
                stroke: span.stroke.clone(),
                paint_order: span.paint_order,
                font_size: span.font_size,
                visible: span.visible,
                positioned_glyphs: span.positioned_glyphs[start..end].to_vec(),
                overline: (start == 0).then(|| span.overline.clone()).flatten(),
                underline: (start == 0).then(|| span.underline.clone()).flatten(),
                line_through: (end == count).then(|| span.line_through.clone()).flatten(),
            };
            if let Some((mut group, _)) = flatten_spans(&[piece], "", rendering_mode, fontdb) {
                group.opacity = Opacity::new_clamped(effect.opacity);
                root.children.push(Node::Group(Box::new(group)));
            }
            start = end;
        }
    }
    root.calculate_bounding_boxes();
    let bounds = root
        .stroke_bounding_box()
        .to_non_zero_rect()
        .ok_or_else(|| "Text glyph effects produced invalid geometry".to_string())?;
    Ok((root, bounds.to_rect()))
}

/// A singular affine collapses all ink, including its stroke. Keep this explicit
/// rather than passing degenerate bounds to upstream nonzero-rectangle APIs.
fn collapsed(transform: Transform) -> bool {
    f64::from(transform.sx) * f64::from(transform.sy)
        - f64::from(transform.kx) * f64::from(transform.ky)
        == 0.0
}

fn empty_effect_geometry(id: &str) -> (Group, Rect) {
    let group = Group {
        id: id.to_owned(),
        ..Group::empty()
    };
    let bounds = group.stroke_bounding_box();
    (group, bounds)
}

/// Transform complete, original glyph paint rather than just its outline
/// coordinates. This also scales authored strokes, bitmap glyphs and SVG/COLR
/// resources consistently. Only adjacent identical affines can share a piece;
/// opacity continues to use the caller's independent protected-cluster IDs.
fn flatten_affine_effects(
    text: &Text,
    effects: &[crate::GlyphRenderEffect],
    fontdb: &fontdb::Database,
) -> Result<(Group, Rect), String> {
    let mut root = Group {
        id: text.id.clone(),
        ..Group::empty()
    };
    let rendering_mode = resolve_rendering_mode(text);
    let mut offset = 0;
    for span in &text.layouted {
        let count = span.positioned_glyphs.len();
        let span_effects = &effects[offset..offset + count];
        offset += count;
        let mut start = 0;
        while start < count {
            let effect = span_effects[start];
            let mut end = start + 1;
            while end < count
                && span_effects[end].transform == effect.transform
                && span_effects[end].dx == effect.dx
                && span_effects[end].dy == effect.dy
                && span_effects[end].opacity == effect.opacity
                && (effect.opacity == 1.0 || span_effects[end].unit == effect.unit)
            {
                end += 1;
            }
            let transform =
                Transform::from_translate(effect.dx, effect.dy).pre_concat(effect.transform);
            if !collapsed(transform) {
                let piece = layout::Span {
                    fill: span.fill.clone(),
                    stroke: span.stroke.clone(),
                    paint_order: span.paint_order,
                    font_size: span.font_size,
                    visible: span.visible,
                    positioned_glyphs: span.positioned_glyphs[start..end].to_vec(),
                    overline: (start == 0).then(|| span.overline.clone()).flatten(),
                    underline: (start == 0).then(|| span.underline.clone()).flatten(),
                    line_through: (end == count).then(|| span.line_through.clone()).flatten(),
                };
                if let Some((mut group, _)) = flatten_spans(&[piece], "", rendering_mode, fontdb) {
                    group
                        .stroke_bounding_box()
                        .transform(transform)
                        .ok_or_else(|| {
                            "Text glyph effects produced invalid transformed bounds".to_string()
                        })?;
                    group.transform = transform.pre_concat(group.transform);
                    group.opacity = Opacity::new_clamped(effect.opacity);
                    root.children.push(Node::Group(Box::new(group)));
                }
            }
            start = end;
        }
    }
    root.calculate_bounding_boxes();
    let bounds = root.stroke_bounding_box();
    Ok((root, bounds))
}
