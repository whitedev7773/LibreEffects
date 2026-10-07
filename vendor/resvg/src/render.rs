// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::OptionLog;

pub struct Context<'a> {
    pub checked: Option<&'a crate::checked::CheckedState<'a>>,
    pub max_bbox: tiny_skia::IntRect,
}

pub fn render_nodes(
    parent: &usvg::Group,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    for node in parent.children() {
        render_node(node, ctx, transform, pixmap);
    }
}

pub fn render_node(
    node: &usvg::Node,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    if ctx.checked.is_some_and(|state| state.failed()) {
        return;
    }
    match node {
        usvg::Node::Group(ref group) => {
            if render_group(group, ctx, transform, pixmap).is_none() {
                if let Some(state) = ctx.checked {
                    state.fail(crate::RenderError::new(
                        crate::RenderErrorKind::InvalidBounds,
                    ));
                }
            }
        }
        usvg::Node::Path(ref path) => {
            crate::path::render(
                path,
                tiny_skia::BlendMode::SourceOver,
                ctx,
                transform,
                pixmap,
            );
        }
        usvg::Node::Image(ref image) => {
            crate::image::render(image, ctx, transform, pixmap);
        }
        usvg::Node::Text(ref text) => {
            if render_group(text.flattened(), ctx, transform, pixmap).is_none() {
                if let Some(state) = ctx.checked {
                    state.fail(crate::RenderError::new(
                        crate::RenderErrorKind::InvalidBounds,
                    ));
                }
            }
        }
    }
}

fn render_group(
    group: &usvg::Group,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    if group.opaque_opacity_byte257() && group.blend_mode() != usvg::BlendMode::Normal {
        if let Some(state) = ctx.checked {
            state.fail(crate::RenderError::new(
                crate::RenderErrorKind::UnsupportedContext,
            ));
        }
        return None;
    }
    let transform = transform.pre_concat(group.transform());

    if !group.should_isolate() {
        render_nodes(group, ctx, transform, pixmap);
        return Some(());
    }

    let bbox = group.layer_bounding_box().transform(transform)?;

    let legacy_bounds = ctx.checked.is_none_or(|state| state.legacy_group_bounds);
    if let Some(state) = ctx.checked.filter(|_| legacy_bounds) {
        // Validate before using historical integer rounding/cropping below.
        // Its unchecked float casts and +/-2 expansion must not overflow in
        // a checked opacity render, even when the eventual crop is offscreen.
        let bounds = crate::checked::int_rect(bbox.to_rect()).and_then(|rect| {
            if group.filters().is_empty() {
                crate::checked::expand_rect(rect, 2, 2)
            } else {
                Ok(rect)
            }
        });
        if let Err(kind) = bounds {
            state.fail(crate::RenderError::new(kind));
            return None;
        }
    }
    let mut ibbox = if let Some(state) = ctx.checked.filter(|_| !legacy_bounds) {
        let checked_box = (|| {
            let rect = crate::checked::int_rect(bbox.to_rect())?;
            let rect = if group.filters().is_empty() {
                crate::checked::expand_rect(rect, 2, 2)?
            } else {
                rect
            };
            // Checked filters need their complete support even when the
            // viewport is small. The upstream five-viewport crop is only an
            // unchecked allocation shortcut, not the declared filter region.
            // image_bytes/reserve below bound the full allocation instead.
            Ok::<_, crate::RenderErrorKind>(rect)
        })();
        match checked_box {
            Ok(rect) => rect,
            Err(kind) => {
                state.fail(crate::RenderError::new(kind));
                return None;
            }
        }
    } else if group.filters().is_empty() {
        // Preserve upstream expansion and integer conversion for legacy output.
        tiny_skia::IntRect::from_xywh(
            bbox.x().floor() as i32 - 2,
            bbox.y().floor() as i32 - 2,
            bbox.width().ceil() as u32 + 4,
            bbox.height().ceil() as u32 + 4,
        )?
    } else {
        crate::geom::fit_to_rect(bbox.to_int_rect(), ctx.max_bbox)?
    };

    if legacy_bounds && group.filters().is_empty() {
        ibbox = crate::geom::fit_to_rect(ibbox, ctx.max_bbox)?;
    }

    let shift_ts = {
        // Original shift.
        let mut dx = bbox.x();
        let mut dy = bbox.y();

        // Account for subpixel positioned layers.
        dx -= bbox.x() - ibbox.x() as f32;
        dy -= bbox.y() - ibbox.y() as f32;

        tiny_skia::Transform::from_translate(-dx, -dy)
    };

    let transform = shift_ts.pre_concat(transform);

    let _live;
    let mut sub_pixmap = if let Some(state) = ctx.checked {
        let allocation = (|| {
            let bytes =
                crate::checked::image_bytes(ibbox.width(), ibbox.height(), state.options.limits)?;
            let guard = state.reserve(bytes)?;
            let pixmap =
                crate::checked::pixmap(ibbox.width(), ibbox.height(), state.options.limits)?;
            Ok::<_, crate::RenderErrorKind>((pixmap, guard))
        })();
        match allocation {
            Ok((pixmap, guard)) => {
                _live = Some(guard);
                pixmap
            }
            Err(kind) => {
                state.fail(crate::RenderError::new(kind).with_buffer_bounds(ibbox));
                return None;
            }
        }
    } else {
        _live = None;
        tiny_skia::Pixmap::new(ibbox.width(), ibbox.height())
            .log_none(|| log::warn!("Failed to allocate a group layer for: {:?}.", ibbox))?
    };

    render_nodes(group, ctx, transform, &mut sub_pixmap.as_mut());
    if ctx.checked.is_some_and(|state| state.failed()) {
        return None;
    }

    if !group.filters().is_empty() {
        for filter in group.filters() {
            crate::filter::apply(filter, transform, &mut sub_pixmap, ctx.checked);
            if ctx.checked.is_some_and(|state| state.failed()) {
                return None;
            }
        }
    }

    if let Some(clip_path) = group.clip_path() {
        crate::clip::apply(clip_path, transform, &mut sub_pixmap);
    }

    if let Some(mask) = group.mask() {
        crate::mask::apply(mask, ctx, transform, &mut sub_pixmap);
    }

    let paint = tiny_skia::PixmapPaint {
        opacity: group.opacity().get(),
        blend_mode: convert_blend_mode(group.blend_mode()),
        quality: tiny_skia::FilterQuality::Nearest,
    };

    if group.opaque_opacity_byte257() {
        if let Err(kind) = crate::opaque_opacity::draw(
            sub_pixmap.as_ref(),
            pixmap,
            ibbox.x(),
            ibbox.y(),
            &paint,
            ctx.checked,
        ) {
            if let Some(state) = ctx.checked {
                state.fail(crate::RenderError::new(kind));
            }
            return None;
        }
    } else {
        pixmap.draw_pixmap(
            ibbox.x(),
            ibbox.y(),
            sub_pixmap.as_ref(),
            &paint,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    Some(())
}

pub fn convert_blend_mode(mode: usvg::BlendMode) -> tiny_skia::BlendMode {
    match mode {
        usvg::BlendMode::Normal => tiny_skia::BlendMode::SourceOver,
        usvg::BlendMode::Multiply => tiny_skia::BlendMode::Multiply,
        usvg::BlendMode::Screen => tiny_skia::BlendMode::Screen,
        usvg::BlendMode::Overlay => tiny_skia::BlendMode::Overlay,
        usvg::BlendMode::Darken => tiny_skia::BlendMode::Darken,
        usvg::BlendMode::Lighten => tiny_skia::BlendMode::Lighten,
        usvg::BlendMode::ColorDodge => tiny_skia::BlendMode::ColorDodge,
        usvg::BlendMode::ColorBurn => tiny_skia::BlendMode::ColorBurn,
        usvg::BlendMode::HardLight => tiny_skia::BlendMode::HardLight,
        usvg::BlendMode::SoftLight => tiny_skia::BlendMode::SoftLight,
        usvg::BlendMode::Difference => tiny_skia::BlendMode::Difference,
        usvg::BlendMode::Exclusion => tiny_skia::BlendMode::Exclusion,
        usvg::BlendMode::Hue => tiny_skia::BlendMode::Hue,
        usvg::BlendMode::Saturation => tiny_skia::BlendMode::Saturation,
        usvg::BlendMode::Color => tiny_skia::BlendMode::Color,
        usvg::BlendMode::Luminosity => tiny_skia::BlendMode::Luminosity,
    }
}
