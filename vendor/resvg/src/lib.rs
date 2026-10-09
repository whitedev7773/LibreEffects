// Copyright 2017 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/*!
[resvg](https://github.com/linebender/resvg) is an SVG rendering library.
*/

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::identity_op)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::wrong_self_convention)]

pub use tiny_skia;
pub use usvg;

mod acceleration;
mod checked;
pub use acceleration::{install_box_blur_accelerator, BoxBlurAccelerator, BoxBlurAcceleratorGuard};
mod clip;
mod opaque_opacity;

pub use checked::{
    CheckedRenderOptions, RenderError, RenderErrorKind, RenderLimits, RepeatEdgeDomain,
};
mod filter;
mod filter_cache;
pub use filter_cache::{install_filter_cache, FilterCache, FilterCacheGuard};
mod geom;
mod image;
mod raster_cache;
pub use raster_cache::{install_raster_image_cache, RasterImageCache, RasterImageCacheGuard};
mod mask;
mod path;
mod profile;
mod render;
pub use profile::{render_profile, reset_render_profile};

/// Renders a tree onto the pixmap.
///
/// `transform` will be used as a root transform.
/// Can be used to position SVG inside the `pixmap`.
///
/// The produced content is in the sRGB color space.
pub fn render(
    tree: &usvg::Tree,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    let target_size = tiny_skia::IntSize::from_wh(pixmap.width(), pixmap.height()).unwrap();
    let max_bbox = tiny_skia::IntRect::from_xywh(
        -(target_size.width() as i32) * 2,
        -(target_size.height() as i32) * 2,
        target_size.width() * 5,
        target_size.height() * 5,
    )
    .unwrap();

    let ctx = render::Context {
        max_bbox,
        checked: None,
    };
    render::render_nodes(tree.root(), &ctx, transform, pixmap);
}

/// Renders with explicit errors for private blur and opacity-compositing profiles.
///
/// Legacy SVGs with no non-default mathematical profile use [`render`] unchanged.
/// On failure the destination may be partially painted and must be discarded.
/// Repeat mode supports sRGB, axis-aligned transformed source domains, and an
/// unclipped filter buffer; unsupported inputs fail instead of changing pixels.
/// Explicit fractional-box blur needs no repeat domain and preserves authored
/// directional pass order under exact quarter turns. Its buffers share the
/// checked allocation limits; unsupported transforms fail explicitly.
/// Opaque byte-opacity interpolation also shares checked temporary limits.
/// Without explicit box/repeat blur, ordinary group crop pixels remain legacy.
pub fn render_checked(
    tree: &usvg::Tree,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    options: &CheckedRenderOptions<'_>,
) -> Result<(), RenderError> {
    let mut needs_checked_blur = false;
    for filter in tree.filters() {
        for (index, primitive) in filter.primitives().iter().enumerate() {
            if let usvg::filter::Kind::GaussianBlur(fe) = primitive.kind() {
                if fe.edge_mode() == usvg::filter::EdgeMode::Wrap {
                    return Err(RenderError::primitive(
                        filter.id(),
                        index,
                        RenderErrorKind::UnsupportedEdgeMode,
                    ));
                }
                if fe.edge_mode() == usvg::filter::EdgeMode::Duplicate || fe.box3_radius().is_some()
                {
                    needs_checked_blur = true;
                }
            }
        }
    }
    if !needs_checked_blur && !tree.has_opaque_opacity_byte257() {
        render(tree, transform, pixmap);
        return Ok(());
    }
    let bytes = checked::image_bytes(pixmap.width(), pixmap.height(), options.limits)
        .map_err(RenderError::new)?;
    let mut state = checked::CheckedState::new(options, bytes)?;
    state.legacy_group_bounds = !needs_checked_blur;
    let w =
        i32::try_from(pixmap.width()).map_err(|_| RenderError::new(RenderErrorKind::Overflow))?;
    let h =
        i32::try_from(pixmap.height()).map_err(|_| RenderError::new(RenderErrorKind::Overflow))?;
    let max_bbox = tiny_skia::IntRect::from_xywh(
        w.checked_mul(-2)
            .ok_or_else(|| RenderError::new(RenderErrorKind::Overflow))?,
        h.checked_mul(-2)
            .ok_or_else(|| RenderError::new(RenderErrorKind::Overflow))?,
        pixmap
            .width()
            .checked_mul(5)
            .ok_or_else(|| RenderError::new(RenderErrorKind::Overflow))?,
        pixmap
            .height()
            .checked_mul(5)
            .ok_or_else(|| RenderError::new(RenderErrorKind::Overflow))?,
    )
    .ok_or_else(|| RenderError::new(RenderErrorKind::Overflow))?;
    let ctx = render::Context {
        max_bbox,
        checked: Some(&state),
    };
    render::render_nodes(tree.root(), &ctx, transform, pixmap);
    state.finish()
}

/// Renders a node onto the pixmap.
///
/// `transform` will be used as a root transform.
/// Can be used to position SVG inside the `pixmap`.
///
/// The expected pixmap size can be retrieved from `usvg::Node::abs_layer_bounding_box()`.
///
/// Returns `None` when `node` has a zero size.
///
/// The produced content is in the sRGB color space.
pub fn render_node(
    node: &usvg::Node,
    mut transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    let bbox = node.abs_layer_bounding_box()?;

    let target_size = tiny_skia::IntSize::from_wh(pixmap.width(), pixmap.height()).unwrap();
    let max_bbox = tiny_skia::IntRect::from_xywh(
        -(target_size.width() as i32) * 2,
        -(target_size.height() as i32) * 2,
        target_size.width() * 5,
        target_size.height() * 5,
    )
    .unwrap();

    transform = transform.pre_translate(-bbox.x(), -bbox.y());

    let ctx = render::Context {
        max_bbox,
        checked: None,
    };
    render::render_node(node, &ctx, transform, pixmap);

    Some(())
}

pub(crate) trait OptionLog {
    fn log_none<F: FnOnce()>(self, f: F) -> Self;
}

impl<T> OptionLog for Option<T> {
    #[inline]
    fn log_none<F: FnOnce()>(self, f: F) -> Self {
        self.or_else(|| {
            f();
            None
        })
    }
}
