// Copyright 2026 the Libre Effects Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::cell::{Cell, RefCell};
use tiny_skia::{IntRect, Pixmap, Rect, Transform};

/// The nominal input domain for one Gaussian primitive in this frame.
///
/// `transform` maps `rect` into that filter's user space. The renderer composes
/// this with its actual group/root transform, including temporary layer shifts.
/// This is neither a primitive output region nor an allocation/padding rectangle.
#[derive(Clone, Debug)]
pub struct RepeatEdgeDomain {
    /// Parsed filter ID; resource IDs must be unique in the rendered tree.
    pub filter_id: String,
    /// Zero-based index in the parsed filter's primitive list.
    pub primitive_index: usize,
    /// Nominal source rectangle, including transparent source pixels.
    pub rect: Rect,
    /// Mapping from the rectangle's coordinates into filter user space.
    pub transform: Transform,
}

/// Bounds on checked temporary pixel allocations.
#[derive(Clone, Copy, Debug)]
pub struct RenderLimits {
    /// Maximum pixels in any one checked image.
    pub max_pixels: usize,
    /// Maximum bytes in any one checked image.
    pub max_bytes: usize,
    /// Maximum simultaneously live checked buffers, including the target,
    /// containing layers, retained filter results, output, halo and scratch.
    pub max_live_bytes: usize,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_pixels: 64 * 1024 * 1024,
            max_bytes: 256 * 1024 * 1024,
            max_live_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Frame-local inputs for [`crate::render_checked`].
#[derive(Clone, Copy, Debug, Default)]
pub struct CheckedRenderOptions<'a> {
    /// One authoritative source domain for each duplicate-edge Gaussian.
    pub repeat_edge_domains: &'a [RepeatEdgeDomain],
    /// Temporary memory bounds. These do not change the legacy renderer.
    pub limits: RenderLimits,
}

/// A checked render failure. Discard the destination after any failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError {
    /// The affected filter, when failure occurred within one.
    pub filter_id: Option<String>,
    /// The affected zero-based primitive, when known.
    pub primitive_index: Option<usize>,
    /// Machine-readable reason.
    pub kind: RenderErrorKind,
}

/// Failures are explicit; checked repeat rendering never silently disables blur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum RenderErrorKind {
    MissingDomain,
    DuplicateDomain,
    InvalidBounds,
    UnsupportedTransform,
    UnsupportedEdgeMode,
    UnsupportedColorSpace,
    UnsupportedFilterRegion,
    UnsupportedContext,
    ClippedSupport,
    Overflow,
    AllocationLimit,
    AllocationFailed,
    FilterFailed,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "checked SVG rendering failed: {:?}", self.kind)?;
        if let Some(id) = &self.filter_id {
            write!(f, " in filter {id:?}")?;
        }
        if let Some(index) = self.primitive_index {
            write!(f, " primitive {index}")?;
        }
        Ok(())
    }
}
impl std::error::Error for RenderError {}

impl RenderError {
    pub(crate) fn new(kind: RenderErrorKind) -> Self {
        Self {
            filter_id: None,
            primitive_index: None,
            kind,
        }
    }
    pub(crate) fn primitive(id: &str, index: usize, kind: RenderErrorKind) -> Self {
        Self {
            filter_id: Some(id.to_owned()),
            primitive_index: Some(index),
            kind,
        }
    }
}

pub(crate) struct CheckedState<'a> {
    pub options: &'a CheckedRenderOptions<'a>,
    pub error: RefCell<Option<RenderError>>,
    live: Cell<usize>,
}

impl<'a> CheckedState<'a> {
    pub fn new(
        options: &'a CheckedRenderOptions<'a>,
        target_bytes: usize,
    ) -> Result<Self, RenderError> {
        if target_bytes > options.limits.max_live_bytes {
            return Err(RenderError::new(RenderErrorKind::AllocationLimit));
        }
        Ok(Self {
            options,
            error: RefCell::new(None),
            live: Cell::new(target_bytes),
        })
    }
    pub fn fail(&self, error: RenderError) {
        let mut slot = self.error.borrow_mut();
        if slot.is_none() {
            *slot = Some(error);
        }
    }
    pub fn failed(&self) -> bool {
        self.error.borrow().is_some()
    }
    pub fn domain(&self, id: &str, index: usize) -> Result<&RepeatEdgeDomain, RenderErrorKind> {
        let mut found = None;
        for (i, domain) in self.options.repeat_edge_domains.iter().enumerate() {
            if domain.filter_id == id && domain.primitive_index == index {
                if found.is_some() {
                    return Err(RenderErrorKind::DuplicateDomain);
                }
                found = Some(i);
            }
        }
        let i = found.ok_or(RenderErrorKind::MissingDomain)?;
        Ok(&self.options.repeat_edge_domains[i])
    }
    pub fn finish(&self) -> Result<(), RenderError> {
        if let Some(error) = self.error.borrow_mut().take() {
            return Err(error);
        }
        Ok(())
    }
    pub fn reserve(&self, bytes: usize) -> Result<LiveGuard<'_>, RenderErrorKind> {
        let total = self
            .live
            .get()
            .checked_add(bytes)
            .ok_or(RenderErrorKind::Overflow)?;
        if total > self.options.limits.max_live_bytes {
            return Err(RenderErrorKind::AllocationLimit);
        }
        self.live.set(total);
        Ok(LiveGuard {
            live: &self.live,
            bytes,
        })
    }
}

pub(crate) struct LiveGuard<'a> {
    live: &'a Cell<usize>,
    bytes: usize,
}
impl Drop for LiveGuard<'_> {
    fn drop(&mut self) {
        self.live.set(self.live.get() - self.bytes);
    }
}

pub(crate) fn image_bytes(
    width: u32,
    height: u32,
    limits: RenderLimits,
) -> Result<usize, RenderErrorKind> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(RenderErrorKind::InvalidBounds);
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(RenderErrorKind::Overflow)?;
    let bytes = pixels.checked_mul(4).ok_or(RenderErrorKind::Overflow)?;
    if bytes > isize::MAX as usize {
        return Err(RenderErrorKind::Overflow);
    }
    if pixels > limits.max_pixels || bytes > limits.max_bytes {
        return Err(RenderErrorKind::AllocationLimit);
    }
    Ok(bytes)
}

pub(crate) fn pixmap(
    width: u32,
    height: u32,
    limits: RenderLimits,
) -> Result<Pixmap, RenderErrorKind> {
    let bytes = image_bytes(width, height, limits)?;
    let mut data = Vec::new();
    data.try_reserve_exact(bytes)
        .map_err(|_| RenderErrorKind::AllocationFailed)?;
    data.resize(bytes, 0);
    let size = tiny_skia::IntSize::from_wh(width, height).ok_or(RenderErrorKind::InvalidBounds)?;
    Pixmap::from_vec(data, size).ok_or(RenderErrorKind::InvalidBounds)
}

/// Only diagonal/anti-diagonal transforms preserve axis-aligned rectangles.
pub(crate) fn axis_aligned(ts: Transform) -> bool {
    let finite = [ts.sx, ts.kx, ts.ky, ts.sy, ts.tx, ts.ty]
        .iter()
        .all(|v| v.is_finite());
    finite
        && ((ts.kx == 0.0 && ts.ky == 0.0 && ts.sx != 0.0 && ts.sy != 0.0)
            || (ts.sx == 0.0 && ts.sy == 0.0 && ts.kx != 0.0 && ts.ky != 0.0))
}

pub(crate) fn int_rect(rect: Rect) -> Result<IntRect, RenderErrorKind> {
    let x = (rect.left() as f64).floor();
    let y = (rect.top() as f64).floor();
    let right = (rect.right() as f64).ceil();
    let bottom = (rect.bottom() as f64).ceil();
    if ![x, y, right, bottom].iter().all(|v| v.is_finite()) || right <= x || bottom <= y {
        return Err(RenderErrorKind::InvalidBounds);
    }
    if x < i32::MIN as f64
        || y < i32::MIN as f64
        || right > i32::MAX as f64
        || bottom > i32::MAX as f64
    {
        return Err(RenderErrorKind::Overflow);
    }
    IntRect::from_xywh(x as i32, y as i32, (right - x) as u32, (bottom - y) as u32)
        .ok_or(RenderErrorKind::Overflow)
}

pub(crate) fn expand_rect(rect: IntRect, x: u32, y: u32) -> Result<IntRect, RenderErrorKind> {
    let left = i64::from(rect.left()) - i64::from(x);
    let top = i64::from(rect.top()) - i64::from(y);
    let right = i64::from(rect.right()) + i64::from(x);
    let bottom = i64::from(rect.bottom()) + i64::from(y);
    if left < i64::from(i32::MIN)
        || top < i64::from(i32::MIN)
        || right > i64::from(i32::MAX)
        || bottom > i64::from(i32::MAX)
    {
        return Err(RenderErrorKind::Overflow);
    }
    IntRect::from_xywh(
        left as i32,
        top as i32,
        (right - left) as u32,
        (bottom - top) as u32,
    )
    .ok_or(RenderErrorKind::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(edge: &str, sigma: &str, color: &str) -> usvg::Tree {
        usvg::Tree::from_str(&format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="5" height="3"><defs><filter id="blur" filterUnits="userSpaceOnUse" x="0" y="0" width="5" height="3" color-interpolation-filters="{color}"><feGaussianBlur stdDeviation="{sigma}" {edge}/></filter></defs><g filter="url(#blur)"><rect width="5" height="3" fill="#204060"/></g></svg>"##), &usvg::Options::default()).unwrap()
    }
    fn domain() -> RepeatEdgeDomain {
        RepeatEdgeDomain {
            filter_id: "blur".into(),
            primitive_index: 0,
            rect: Rect::from_xywh(0.0, 0.0, 5.0, 3.0).unwrap(),
            transform: Transform::identity(),
        }
    }
    fn render(
        tree: &usvg::Tree,
        domains: &[RepeatEdgeDomain],
        transform: Transform,
    ) -> Result<Pixmap, RenderError> {
        let mut pixmap = Pixmap::new(5, 3).unwrap();
        crate::render_checked(
            tree,
            transform,
            &mut pixmap.as_mut(),
            &CheckedRenderOptions {
                repeat_edge_domains: domains,
                ..CheckedRenderOptions::default()
            },
        )?;
        Ok(pixmap)
    }
    #[test]
    fn absent_mode_preserves_legacy_pixels_exactly() {
        let tree = tree("", "2 1", "sRGB");
        let mut legacy = Pixmap::new(5, 3).unwrap();
        crate::render(&tree, Transform::identity(), &mut legacy.as_mut());
        let checked = render(&tree, &[], Transform::identity()).unwrap();
        assert_eq!(legacy.data(), checked.data());
    }
    #[test]
    fn checked_duplicate_retains_constant_boundary() {
        for sigma in ["2 0", "1 1", "0 0", "0.049 0.049"] {
            let tree = tree(r#"edgeMode="duplicate""#, sigma, "sRGB");
            let output = render(&tree, &[domain()], Transform::identity()).unwrap();
            for pixel in output.data().chunks_exact(4) {
                assert_eq!(pixel, [32, 64, 96, 255]);
            }
        }
    }
    #[test]
    fn checked_failures_never_silently_disable_repeat() {
        let svg = tree(r#"edgeMode="duplicate""#, "2 0", "sRGB");
        assert_eq!(
            render(&svg, &[], Transform::identity()).unwrap_err().kind,
            RenderErrorKind::MissingDomain
        );
        assert_eq!(
            render(&svg, &[domain(), domain()], Transform::identity())
                .unwrap_err()
                .kind,
            RenderErrorKind::DuplicateDomain
        );
        let mut bad = domain();
        bad.transform = Transform::from_row(1.0, 0.0, 0.2, 1.0, 0.0, 0.0);
        assert_eq!(
            render(&svg, &[bad], Transform::identity())
                .unwrap_err()
                .kind,
            RenderErrorKind::UnsupportedTransform
        );
        let mut bad = domain();
        bad.rect = Rect::from_xywh(-1.0, 0.0, 6.0, 3.0).unwrap();
        assert_eq!(
            render(&svg, &[bad], Transform::identity())
                .unwrap_err()
                .kind,
            RenderErrorKind::ClippedSupport
        );
        let linear = tree(r#"edgeMode="duplicate""#, "2 0", "linearRGB");
        assert_eq!(
            render(&linear, &[domain()], Transform::identity())
                .unwrap_err()
                .kind,
            RenderErrorKind::UnsupportedColorSpace
        );
    }
    #[test]
    fn live_cap_accounts_for_input_output_halo_and_scratch() {
        let svg = tree(r#"edgeMode="duplicate""#, "2 0", "sRGB");
        let domains = [domain()];
        let mut output = Pixmap::new(5, 3).unwrap();
        let options = CheckedRenderOptions {
            repeat_edge_domains: &domains,
            limits: RenderLimits {
                max_live_bytes: 200,
                ..RenderLimits::default()
            },
        };
        assert_eq!(
            crate::render_checked(&svg, Transform::identity(), &mut output.as_mut(), &options)
                .unwrap_err()
                .kind,
            RenderErrorKind::AllocationLimit
        );
    }
    #[test]
    fn exact_quarter_turn_swaps_authored_blur_axes() {
        let tree = usvg::Tree::from_str(r##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="5"><defs><filter id="blur" filterUnits="userSpaceOnUse" x="0" y="0" width="5" height="1" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="2 0" edgeMode="duplicate"/></filter></defs><g transform="matrix(0 1 -1 0 1 0)" filter="url(#blur)"><rect width="1" height="1" opacity="0.9529411764705882"/></g></svg>"##, &usvg::Options::default()).unwrap();
        let mut output = Pixmap::new(1, 5).unwrap();
        let domains = [RepeatEdgeDomain {
            rect: Rect::from_xywh(0.0, 0.0, 5.0, 1.0).unwrap(),
            ..domain()
        }];
        crate::render_checked(
            &tree,
            Transform::identity(),
            &mut output.as_mut(),
            &CheckedRenderOptions {
                repeat_edge_domains: &domains,
                ..CheckedRenderOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            output
                .data()
                .chunks_exact(4)
                .map(|p| p[3])
                .collect::<Vec<_>>(),
            [147, 96, 51, 21, 6]
        );
    }

    #[test]
    fn nominal_domain_survives_inverse_then_forward_adjustment_mapping() {
        let tree = usvg::Tree::from_str(r##"<svg xmlns="http://www.w3.org/2000/svg" width="5" height="3"><defs><filter id="blur" filterUnits="userSpaceOnUse" x="-10" y="-20" width="5" height="3" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="2 0" edgeMode="duplicate"/></filter></defs><g transform="translate(10 20)" filter="url(#blur)"><g transform="translate(-10 -20)"><rect width="5" height="3" fill="#204060"/></g></g></svg>"##, &usvg::Options::default()).unwrap();
        let domains = [RepeatEdgeDomain {
            transform: Transform::from_translate(-10.0, -20.0),
            ..domain()
        }];
        let output = render(&tree, &domains, Transform::identity()).unwrap();
        for pixel in output.data().chunks_exact(4) {
            assert_eq!(pixel, [32, 64, 96, 255]);
        }
    }

    #[test]
    fn nested_ordered_stages_use_the_previous_declared_output_domain() {
        let tree = usvg::Tree::from_str(r##"<svg xmlns="http://www.w3.org/2000/svg" width="5" height="3"><defs><filter id="inner" filterUnits="userSpaceOnUse" x="-2" y="-2" width="9" height="7" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="1 0" edgeMode="duplicate"/></filter><filter id="outer" filterUnits="userSpaceOnUse" x="-4" y="-4" width="13" height="11" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="2 0" edgeMode="duplicate"/></filter></defs><g filter="url(#outer)"><g filter="url(#inner)"><rect width="5" height="3" fill="#204060"/></g></g></svg>"##, &usvg::Options::default()).unwrap();
        let domains = [
            RepeatEdgeDomain {
                filter_id: "inner".into(),
                ..domain()
            },
            RepeatEdgeDomain {
                filter_id: "outer".into(),
                rect: Rect::from_xywh(-2.0, -2.0, 9.0, 7.0).unwrap(),
                ..domain()
            },
        ];
        let output = render(&tree, &domains, Transform::identity()).unwrap();
        for pixel in output.data().chunks_exact(4) {
            assert_eq!(pixel, [32, 64, 96, 255]);
        }
    }

    #[test]
    fn fractional_axis_aligned_domain_keeps_its_full_raster_coverage() {
        let tree = tree(r#"edgeMode="duplicate""#, "1 0", "sRGB");
        let output = render(&tree, &[domain()], Transform::from_translate(0.25, 0.0)).unwrap();
        assert!(output.data().chunks_exact(4).all(|p| p[3] > 0));
        assert!(output.data().chunks_exact(4).any(|p| p[3] > 245));
    }

    #[test]
    fn transformed_bounds_and_required_support_fail_explicitly() {
        let tree = tree(r#"edgeMode="duplicate""#, "2 0", "sRGB");
        assert_eq!(
            render(&tree, &[domain()], Transform::from_scale(100.0, 100.0))
                .unwrap_err()
                .kind,
            RenderErrorKind::ClippedSupport
        );
        let mut malformed = domain();
        malformed.transform.tx = f32::INFINITY;
        assert_eq!(
            render(&tree, &[malformed], Transform::identity())
                .unwrap_err()
                .kind,
            RenderErrorKind::UnsupportedTransform
        );
        assert_eq!(
            int_rect(Rect::from_xywh(1.0e20, 0.0, 1.0e20, 1.0).unwrap()).unwrap_err(),
            RenderErrorKind::Overflow
        );
    }
    #[test]
    fn frame_registry_may_include_unrendered_or_already_baked_stages() {
        let svg = tree(r#"edgeMode="duplicate""#, "1 0", "sRGB");
        let mut unrelated = domain();
        unrelated.filter_id = "already-baked-stage".into();
        render(&svg, &[domain(), unrelated.clone()], Transform::identity()).unwrap();
        let legacy = tree("", "1 0", "sRGB");
        let with_registry = render(&legacy, &[unrelated], Transform::identity()).unwrap();
        let without_registry = render(&legacy, &[], Transform::identity()).unwrap();
        assert_eq!(with_registry.data(), without_registry.data());
    }
    #[test]
    fn embedded_svg_repeat_uses_the_same_checked_context() {
        let inner = r##"<svg xmlns="http://www.w3.org/2000/svg" width="5" height="3"><defs><filter id="blur" filterUnits="userSpaceOnUse" x="0" y="0" width="5" height="3" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="2 0" edgeMode="duplicate"/></filter></defs><g filter="url(#blur)"><rect width="5" height="3" fill="#204060"/></g></svg>"##;
        let encoded: String = inner.bytes().map(|b| format!("%{b:02X}")).collect();
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="5" height="3"><image width="5" height="3" href="data:image/svg+xml,{encoded}"/></svg>"##
        );
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        assert_eq!(
            render(&tree, &[], Transform::identity()).unwrap_err().kind,
            RenderErrorKind::MissingDomain
        );
        let output = render(&tree, &[domain()], Transform::identity()).unwrap();
        for pixel in output.data().chunks_exact(4) {
            assert_eq!(pixel, [32, 64, 96, 255]);
        }
    }
}
