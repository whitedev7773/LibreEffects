// Copyright 2026 the Libre Effects Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Qualified signed-byte interpolation for opaque source/destination pairs.
//! All other pixels are painted by the original tiny-skia SourceOver path.
use crate::checked::{CheckedState, RenderErrorKind, RenderLimits};
use tiny_skia::{Pixmap, PixmapMut, PixmapPaint, PixmapRef, Transform};

fn channel(source: u8, destination: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return destination;
    }
    if alpha == 255 {
        return source;
    }
    let delta = (i32::from(source) - i32::from(destination)) * i32::from(alpha);
    // Signed division must truncate toward zero. A right shift instead changes
    // dark-over-light results. The product fits i32 for every byte pair.
    (i32::from(destination) + ((delta + 128) * 257) / 65_536) as u8
}

pub(crate) fn draw(
    source: PixmapRef<'_>,
    destination: &mut PixmapMut<'_>,
    x: i32,
    y: i32,
    paint: &PixmapPaint,
    checked: Option<&CheckedState<'_>>,
) -> Result<(), RenderErrorKind> {
    let native = |destination: &mut PixmapMut<'_>| {
        destination.draw_pixmap(x, y, source, paint, Transform::identity(), None);
    };
    if paint.opacity <= 0.0 || paint.opacity >= 1.0 {
        native(destination);
        return Ok(());
    }
    if !paint.opacity.is_finite() || paint.blend_mode != tiny_skia::BlendMode::SourceOver {
        return Err(RenderErrorKind::UnsupportedContext);
    }
    let alpha = (paint.opacity * 255.0).round() as u8;
    let left = i64::from(x).max(0);
    let top = i64::from(y).max(0);
    let right = (i64::from(x) + i64::from(source.width())).min(i64::from(destination.width()));
    let bottom = (i64::from(y) + i64::from(source.height())).min(i64::from(destination.height()));
    if left >= right || top >= bottom {
        return Ok(());
    }
    let width = (right - left) as u32;
    let height = (bottom - top) as u32;
    let limits = checked.map_or_else(RenderLimits::default, |state| state.options.limits);
    let bytes = crate::checked::image_bytes(width, height, limits)?;
    let _live = checked.map(|state| state.reserve(bytes)).transpose()?;
    let mut overrides = crate::checked::pixmap(width, height, limits)?;
    for row in 0..height as usize {
        let si = ((top - i64::from(y)) as usize + row) * source.width() as usize * 4
            + (left - i64::from(x)) as usize * 4;
        let di = ((top as usize + row) * destination.width() as usize + left as usize) * 4;
        let start = row * width as usize * 4;
        for ((s, d), o) in source.data()[si..si + width as usize * 4]
            .chunks_exact(4)
            .zip(destination.data_mut()[di..di + width as usize * 4].chunks_exact(4))
            .zip(overrides.data_mut()[start..start + width as usize * 4].chunks_exact_mut(4))
        {
            if s[3] == 255 && d[3] == 255 {
                for c in 0..3 {
                    o[c] = channel(s[c], d[c], alpha);
                }
                o[3] = 255;
            }
        }
    }
    // Preserve the exact native implementation for unqualified partial-alpha
    // pixels, including its opacity precision and rounding. Overrides are a
    // checked intersection buffer, not a clone of the whole containing canvas.
    native(destination);
    apply(overrides, destination, left as usize, top as usize);
    Ok(())
}

fn apply(overrides: Pixmap, destination: &mut PixmapMut<'_>, x: usize, y: usize) {
    for row in 0..overrides.height() as usize {
        let start = ((y + row) * destination.width() as usize + x) * 4;
        let len = overrides.width() as usize * 4;
        for (d, o) in destination.data_mut()[start..start + len]
            .chunks_exact_mut(4)
            .zip(overrides.data()[row * len..(row + 1) * len].chunks_exact(4))
        {
            if o[3] == 255 {
                d.copy_from_slice(o);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("../tests/fixtures/owned-ae-opaque-opacity-v1.rs");
    #[test]
    fn opacity_only_checked_render_retains_ordinary_gaussian_crop_pixels() {
        let normal = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><defs><filter id="ordinary" filterUnits="userSpaceOnUse" x="-40" y="-40" width="88" height="88" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="3"/></filter></defs><g opacity="1" filter="url(#ordinary)"><rect width="8" height="8" fill="#808080"/></g></svg>"##;
        let private = normal.replace(
            "<g opacity",
            "<g data-libre-effects-compositing=\"opaque-opacity-byte257-v1\" opacity",
        );
        let ordinary = usvg::Tree::from_str(normal, &usvg::Options::default()).unwrap();
        let profiled = usvg::Tree::from_str(&private, &usvg::Options::default()).unwrap();
        let mut expected = Pixmap::new(8, 8).unwrap();
        crate::render(&ordinary, Transform::identity(), &mut expected.as_mut());
        let mut actual = Pixmap::new(8, 8).unwrap();
        crate::render_checked(
            &profiled,
            Transform::identity(),
            &mut actual.as_mut(),
            &Default::default(),
        )
        .unwrap();
        assert_eq!(actual.data(), expected.data());
        let oversized = private.replace("<g data-", "<g transform=\"translate(1e20)\" data-");
        let tree = usvg::Tree::from_str(&oversized, &usvg::Options::default()).unwrap();
        assert!(crate::render_checked(
            &tree,
            Transform::identity(),
            &mut actual.as_mut(),
            &Default::default()
        )
        .is_err());
    }
    #[test]
    fn svg_compositing_matches_measured_opaque_byte_fixture() {
        for &(source, destination, alpha, expected) in OWNED_AE_OPAQUE_CASES {
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#{destination:02x}{destination:02x}{destination:02x}"/><g opacity="{}" data-libre-effects-compositing="opaque-opacity-byte257-v1"><rect width="8" height="8" fill="#{source:02x}{source:02x}{source:02x}"/></g></svg>"##,
                f64::from(alpha) / 255.0
            );
            let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
            let mut output = Pixmap::new(8, 8).unwrap();
            crate::render_checked(
                &tree,
                Transform::identity(),
                &mut output.as_mut(),
                &Default::default(),
            )
            .unwrap();
            for pixel in output.data().chunks_exact(4) {
                assert_eq!(
                    pixel,
                    [expected, expected, expected, 255],
                    "{source}/{destination}/{alpha}"
                );
            }
        }
    }
    #[test]
    fn checked_opacity_profile_rejects_memory_limits_and_conflicting_blend_modes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#808080"/><g opacity="0.2" data-libre-effects-compositing="opaque-opacity-byte257-v1"><rect width="10" height="10"/></g></svg>"##;
        let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).unwrap();
        let mut output = Pixmap::new(10, 10).unwrap();
        let options = crate::CheckedRenderOptions {
            limits: RenderLimits {
                max_live_bytes: 1300,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            crate::render_checked(&tree, Transform::identity(), &mut output.as_mut(), &options)
                .unwrap_err()
                .kind,
            RenderErrorKind::AllocationLimit
        );
        let invalid = svg.replace(
            "opacity=\"0.2\"",
            "opacity=\"0.2\" style=\"mix-blend-mode:multiply\"",
        );
        let tree = usvg::Tree::from_str(&invalid, &usvg::Options::default()).unwrap();
        assert_eq!(
            crate::render_checked(
                &tree,
                Transform::identity(),
                &mut output.as_mut(),
                &Default::default()
            )
            .unwrap_err()
            .kind,
            RenderErrorKind::UnsupportedContext
        );
    }
    #[test]
    fn measured_opaque_byte_cases_and_endpoints() {
        // Literal observed AE 8-bpc results, including the two negative
        // division boundaries that distinguish +127/255 and float +0.5 models.
        for (s, d, a, want) in [
            (0, 191, 2, 191),
            (0, 164, 7, 161),
            (0, 206, 13, 197),
            (0, 255, 51, 205),
            (0, 128, 128, 65),
            (255, 128, 128, 192),
            (32, 224, 51, 187),
            (224, 32, 51, 70),
        ] {
            assert_eq!(channel(s, d, a), want, "{s}/{d}/{a}");
        }
        for s in 0..=255 {
            for d in 0..=255 {
                assert_eq!(channel(s, d, 0), d);
                assert_eq!(channel(s, d, 255), s);
                for a in 1..255 {
                    let c = channel(s, d, a);
                    assert!((s.min(d)..=s.max(d)).contains(&c));
                }
            }
        }
    }
    #[test]
    fn clipped_negative_placement_preserves_native_partial_alpha_pixels() {
        let mut source = Pixmap::new(3, 2).unwrap();
        source.data_mut().copy_from_slice(&[
            0, 0, 0, 255, 0, 0, 0, 255, 64, 32, 0, 128, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 0,
        ]);
        let mut actual = Pixmap::new(3, 2).unwrap();
        actual.data_mut().copy_from_slice(&[
            128, 128, 128, 255, 40, 20, 0, 64, 0, 0, 0, 0, 180, 180, 180, 255, 64, 64, 64, 128, 32,
            32, 32, 255,
        ]);
        let before = actual.clone();
        let mut native = actual.clone();
        let paint = PixmapPaint {
            opacity: 0.2,
            ..Default::default()
        };
        native
            .as_mut()
            .draw_pixmap(-1, 0, source.as_ref(), &paint, Transform::identity(), None);
        draw(source.as_ref(), &mut actual.as_mut(), -1, 0, &paint, None).unwrap();
        assert_eq!(&actual.data()[0..4], &[103, 103, 103, 255]);
        assert_eq!(&actual.data()[12..16], &[145, 145, 145, 255]);
        for i in [1, 2, 4, 5] {
            assert_eq!(
                &actual.data()[i * 4..i * 4 + 4],
                &native.data()[i * 4..i * 4 + 4]
            );
        }
        let mut offscreen = before.clone();
        draw(
            source.as_ref(),
            &mut offscreen.as_mut(),
            i32::MIN,
            i32::MAX,
            &paint,
            None,
        )
        .unwrap();
        assert_eq!(offscreen, before);
    }
}
