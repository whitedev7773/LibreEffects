// Copyright 2026 the Libre Effects Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{box_blur, FilterResult, Image, ImageRefMut};
use crate::checked::{self, CheckedState};
use crate::RenderErrorKind as Failure;
use rgb::FromSlice;
use tiny_skia::{IntRect, Pixmap, Transform};
use usvg::filter::{ColorInterpolation, GaussianBlur, Input};

pub(super) fn apply(
    fe: &GaussianBlur,
    cs: ColorInterpolation,
    ts: Transform,
    region: IntRect,
    output: IntRect,
    source: &Pixmap,
    results: &[FilterResult],
    state: &CheckedState<'_>,
    filter_id: &str,
    primitive_index: usize,
) -> Result<Image, Failure> {
    let domain = state.domain(filter_id, primitive_index)?;
    if cs != ColorInterpolation::SRGB {
        return Err(Failure::UnsupportedColorSpace);
    }
    if !checked::axis_aligned(ts) {
        return Err(Failure::UnsupportedTransform);
    }
    let domain_ts = ts.pre_concat(domain.transform);
    if !checked::axis_aligned(domain_ts) {
        return Err(Failure::UnsupportedTransform);
    }
    let domain = checked::int_rect(
        domain
            .rect
            .transform(domain_ts)
            .ok_or(Failure::InvalidBounds)?,
    )?;
    let source_bounds =
        IntRect::from_xywh(0, 0, source.width(), source.height()).ok_or(Failure::InvalidBounds)?;
    // Upstream filter results have a zero pixel origin. Do not invent an origin
    // when a union of differently sized filter buffers changes that contract.
    if region != source_bounds {
        return Err(Failure::UnsupportedFilterRegion);
    }
    if !contains(region, output) {
        return Err(Failure::ClippedSupport);
    }
    let (input, input_bounds, alpha_only) = match fe.input() {
        Input::SourceGraphic => (source, source_bounds, false),
        Input::SourceAlpha => (source, source_bounds, true),
        Input::Reference(name) => {
            let result = results
                .iter()
                .rev()
                .find(|r| r.name == *name)
                .ok_or(Failure::FilterFailed)?;
            if result.image.color_space != cs {
                return Err(Failure::UnsupportedColorSpace);
            }
            (result.image.as_ref(), result.image.region, false)
        }
    };
    let actual_input =
        IntRect::from_xywh(0, 0, input.width(), input.height()).ok_or(Failure::InvalidBounds)?;
    if !contains(actual_input, domain) || !contains(input_bounds, domain) {
        return Err(Failure::ClippedSupport);
    }
    // tiny-skia's get_scale returns raster-axis scale magnitudes. For an
    // exact quarter turn, swap the authored sigmas BEFORE scaling/cutoff.
    let (dx, dy) = if ts.sx == 0.0 && ts.sy == 0.0 {
        (fe.std_dev_y().get(), fe.std_dev_x().get())
    } else {
        (fe.std_dev_x().get(), fe.std_dev_y().get())
    };
    let (sx, sy, boxes) = super::resolve_std_dev(dx, dy, ts).unwrap_or((0.0, 0.0, false));
    if !sx.is_finite() || !sy.is_finite() {
        return Err(Failure::InvalidBounds);
    }
    let result = blur(input, domain, output, sx, sy, boxes, alpha_only, state)?;
    Ok(Image::from_image(result, cs))
}

fn contains(outer: IntRect, inner: IntRect) -> bool {
    inner.left() >= outer.left()
        && inner.top() >= outer.top()
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

pub(super) fn box_support(sigma: f64) -> Result<u32, Failure> {
    // Guard upstream's float-to-i32 widths and wl + 2 before entering its exact
    // width calculation. Practical memory limits are checked separately.
    let ideal = (12.0f32 * (sigma as f32).powi(2) / 5.0).sqrt() + 1.0;
    if !ideal.is_finite() || ideal >= (i32::MAX - 2) as f32 {
        return Err(Failure::Overflow);
    }
    box_blur::create_box_gauss(sigma as f32)
        .iter()
        .try_fold(0u32, |sum, size| {
            let radius = u32::try_from((size - 1) / 2).map_err(|_| Failure::Overflow)?;
            sum.checked_add(radius).ok_or(Failure::Overflow)
        })
}

fn blur(
    input: &Pixmap,
    domain: IntRect,
    output: IntRect,
    sx: f64,
    sy: f64,
    boxes: bool,
    alpha_only: bool,
    state: &CheckedState<'_>,
) -> Result<Pixmap, Failure> {
    let limits = state.options.limits;
    let output_bytes = checked::image_bytes(input.width(), input.height(), limits)?;
    if sx == 0.0 && sy == 0.0 {
        let _live = state.reserve(output_bytes)?;
        let mut result = checked::pixmap(input.width(), input.height(), limits)?;
        for y in output.top()..output.bottom() {
            for x in output.left()..output.right() {
                let i = ((y as usize) * input.width() as usize + x as usize) * 4;
                result.data_mut()[i..i + 4].copy_from_slice(&input.data()[i..i + 4]);
                if alpha_only {
                    result.data_mut()[i..i + 3].fill(0);
                }
            }
        }
        return Ok(result);
    }
    let (rx, ry) = if boxes {
        (box_support(sx)?, box_support(sy)?)
    } else {
        (fir_radius(sx)?, fir_radius(sy)?)
    };
    let halo = checked::expand_rect(output, rx, ry)?;
    let halo_bytes = checked::image_bytes(halo.width(), halo.height(), limits)?;
    let live_bytes = halo_bytes
        .checked_mul(2)
        .and_then(|n| n.checked_add(output_bytes))
        .ok_or(Failure::Overflow)?;
    let _live = state.reserve(live_bytes)?;
    let mut work = checked::pixmap(halo.width(), halo.height(), limits)?;
    let mut scratch = checked::pixmap(halo.width(), halo.height(), limits)?;
    let mut result = checked::pixmap(input.width(), input.height(), limits)?;

    // Extend the ORIGINAL input once. Intermediate box/FIR passes must never
    // reclamp against D: that would define a different boundary operator.
    for y in 0..halo.height() {
        let source_y = (i64::from(halo.y()) + i64::from(y))
            .clamp(i64::from(domain.top()), i64::from(domain.bottom()) - 1)
            as usize;
        for x in 0..halo.width() {
            let source_x = (i64::from(halo.x()) + i64::from(x))
                .clamp(i64::from(domain.left()), i64::from(domain.right()) - 1)
                as usize;
            let src = (source_y * input.width() as usize + source_x) * 4;
            let dst = (y as usize * halo.width() as usize + x as usize) * 4;
            work.data_mut()[dst..dst + 4].copy_from_slice(&input.data()[src..src + 4]);
            if alpha_only {
                work.data_mut()[dst..dst + 3].fill(0);
            }
        }
    }
    if boxes {
        box_blur::apply_with_scratch(
            sx,
            sy,
            ImageRefMut::new(halo.width(), halo.height(), work.data_mut().as_rgba_mut()),
            ImageRefMut::new(
                halo.width(),
                halo.height(),
                scratch.data_mut().as_rgba_mut(),
            ),
        );
    } else {
        fir_pass(&work, &mut scratch, sy, false)?;
        fir_pass(&scratch, &mut work, sx, true)?;
    }
    let row_bytes = output.width() as usize * 4;
    for y in output.top()..output.bottom() {
        let src = (((i64::from(y) - i64::from(halo.y())) as usize) * halo.width() as usize
            + (i64::from(output.x()) - i64::from(halo.x())) as usize)
            * 4;
        let dst = (y as usize * input.width() as usize + output.x() as usize) * 4;
        result.data_mut()[dst..dst + row_bytes].copy_from_slice(&work.data()[src..src + row_bytes]);
    }
    Ok(result)
}

fn fir_radius(sigma: f64) -> Result<u32, Failure> {
    if !sigma.is_finite() || !(0.0..2.0).contains(&sigma) {
        return Err(Failure::InvalidBounds);
    }
    Ok((sigma * 4.0).ceil() as u32)
}

fn fir_pass(
    input: &Pixmap,
    output: &mut Pixmap,
    sigma: f64,
    horizontal: bool,
) -> Result<(), Failure> {
    let radius = fir_radius(sigma)? as i32;
    if radius == 0 {
        output.data_mut().copy_from_slice(input.data());
        return Ok(());
    }
    let mut weights = [0.0f64; 17];
    let mut sum = 0.0;
    for offset in -radius..=radius {
        let weight = (-(f64::from(offset).powi(2)) / (2.0 * sigma * sigma)).exp();
        weights[(offset + radius) as usize] = weight;
        sum += weight;
    }
    for weight in &mut weights[..(radius * 2 + 1) as usize] {
        *weight /= sum;
    }
    let width = i64::from(input.width());
    let height = i64::from(input.height());
    for y in 0..height {
        for x in 0..width {
            let mut value = [0.0; 4];
            for offset in -radius..=radius {
                let xx = if horizontal { x + i64::from(offset) } else { x };
                let yy = if horizontal { y } else { y + i64::from(offset) };
                if xx < 0 || yy < 0 || xx >= width || yy >= height {
                    continue;
                }
                let i = (yy as usize * width as usize + xx as usize) * 4;
                let weight = weights[(offset + radius) as usize];
                for channel in 0..4 {
                    value[channel] += f64::from(input.data()[i + channel]) * weight;
                }
            }
            let i = (y as usize * width as usize + x as usize) * 4;
            for channel in 0..4 {
                output.data_mut()[i + channel] = value[channel].round() as u8;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckedRenderOptions, RenderLimits};

    fn run(alpha: &[u8], sigma: f64) -> Vec<u8> {
        let mut input = Pixmap::new(alpha.len() as u32, 1).unwrap();
        for (p, a) in input.data_mut().chunks_exact_mut(4).zip(alpha) {
            p[3] = *a;
        }
        let domain = IntRect::from_xywh(0, 0, alpha.len() as u32, 1).unwrap();
        let options = CheckedRenderOptions::default();
        let state = CheckedState::new(&options, input.data().len()).unwrap();
        blur(
            &input,
            domain,
            domain,
            sigma,
            0.0,
            sigma >= 2.0,
            false,
            &state,
        )
        .unwrap()
        .data()
        .chunks_exact(4)
        .map(|p| p[3])
        .collect()
    }

    #[test]
    fn literal_box_boundary_oracles() {
        assert_eq!(run(&[243, 0, 0, 0, 0], 2.0), [147, 96, 51, 21, 6]);
        assert_eq!(run(&[0, 0, 243, 0, 0], 2.0), [30, 45, 51, 45, 30]);
    }
    #[test]
    fn literal_small_sigma_fir_oracle_and_identity() {
        assert_eq!(run(&[0, 255], 1.0), [77, 178]);
        assert_eq!(run(&[13, 200, 49], 0.0), [13, 200, 49]);
    }
    #[test]
    fn constant_premultiplied_edges_and_corners() {
        for (sx, sy) in [(2.0, 2.0), (1.0, 1.0), (2.0, 0.0), (0.0, 1.0)] {
            let mut input = Pixmap::new(3, 2).unwrap();
            for p in input.data_mut().chunks_exact_mut(4) {
                p.copy_from_slice(&[25, 50, 75, 100]);
            }
            let domain = IntRect::from_xywh(0, 0, 3, 2).unwrap();
            let options = CheckedRenderOptions::default();
            let state = CheckedState::new(&options, input.data().len()).unwrap();
            let output = blur(
                &input,
                domain,
                domain,
                sx,
                sy,
                sx >= 2.0 || sy >= 2.0,
                false,
                &state,
            )
            .unwrap();
            assert_eq!(output.data(), input.data(), "{sx}, {sy}");
        }
    }
    #[test]
    fn memory_caps_and_halo_overflow_fail() {
        let input = Pixmap::new(5, 1).unwrap();
        let domain = IntRect::from_xywh(0, 0, 5, 1).unwrap();
        let options = CheckedRenderOptions {
            limits: RenderLimits {
                max_pixels: 10,
                ..RenderLimits::default()
            },
            ..CheckedRenderOptions::default()
        };
        let state = CheckedState::new(&options, 20).unwrap();
        assert_eq!(
            blur(&input, domain, domain, 2.0, 0.0, true, false, &state).unwrap_err(),
            Failure::AllocationLimit
        );
        assert_eq!(box_support(f64::MAX).unwrap_err(), Failure::Overflow);
        assert_eq!(
            checked::expand_rect(IntRect::from_xywh(i32::MAX - 5, 0, 5, 1).unwrap(), 1, 0)
                .unwrap_err(),
            Failure::Overflow
        );
    }
    #[test]
    fn output_crop_has_its_own_full_support_halo() {
        let mut input = Pixmap::new(10, 1).unwrap();
        input.data_mut()[3] = 243;
        let domain = IntRect::from_xywh(0, 0, 10, 1).unwrap();
        let output = IntRect::from_xywh(3, 0, 4, 1).unwrap();
        let options = CheckedRenderOptions::default();
        let state = CheckedState::new(&options, input.data().len()).unwrap();
        let result = blur(&input, domain, output, 2.0, 0.0, true, false, &state).unwrap();
        assert_eq!(
            result
                .data()
                .chunks_exact(4)
                .map(|p| p[3])
                .collect::<Vec<_>>(),
            [0, 0, 0, 21, 6, 1, 0, 0, 0, 0]
        );
    }
}
