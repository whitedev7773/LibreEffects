// Copyright 2026 the Libre Effects Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit mathematical box profile. Legacy Gaussian blur does not use this.
//! Transparent borders, three horizontal passes then three vertical passes,
//! and byte quantization after every pass. Work is linear in image pixels,
//! independent of kernel width; no radius-sized scratch is allocated.
use super::ImageRefMut;
use crate::{checked, RenderErrorKind};
use rgb::{FromSlice, RGBA8};

pub(super) fn apply(
    radii: [f64; 2],
    source: &mut tiny_skia::Pixmap,
    state: Option<&checked::CheckedState<'_>>,
    vertical_first: bool,
) -> Result<(), RenderErrorKind> {
    if !radii
        .iter()
        .all(|r| r.is_finite() && (0.0..=8192.0).contains(r))
    {
        return Err(RenderErrorKind::InvalidBounds);
    }
    if radii.iter().all(|r| *r <= 0.5) {
        return Ok(());
    }
    let limits = state.map(|s| s.options.limits).unwrap_or_default();
    let bytes = checked::image_bytes(source.width(), source.height(), limits)?;
    let _scratch_live = state.map(|s| s.reserve(bytes)).transpose()?;
    let mut scratch = checked::pixmap(source.width(), source.height(), limits)?;
    for axis in if vertical_first { [1, 0] } else { [0, 1] } {
        let radius = radii[axis];
        if radius <= 0.5 {
            continue;
        }
        for _ in 0..3 {
            pass(
                source.data().as_rgba(),
                ImageRefMut::new(
                    scratch.width(),
                    scratch.height(),
                    scratch.data_mut().as_rgba_mut(),
                ),
                radius,
                axis == 0,
            );
            std::mem::swap(source, &mut scratch);
        }
    }
    Ok(())
}

fn channels(p: RGBA8) -> [u32; 4] {
    [p.r as u32, p.g as u32, p.b as u32, p.a as u32]
}

fn pass(input: &[RGBA8], output: ImageRefMut<'_>, radius: f64, horizontal: bool) {
    let width = output.width as usize;
    let height = output.height as usize;
    let (length, lines, stride) = if horizontal {
        (width, height, 1)
    } else {
        (height, width, width)
    };
    let n = (radius + 0.5).floor() as i64;
    let edge_weight = radius - (n as f64 - 0.5);
    let divisor = 2.0 * radius;
    for line in 0..lines {
        let base = if horizontal { line * width } else { line };
        let get = |index: i64| {
            if index < 0 || index >= length as i64 {
                [0u32; 4]
            } else {
                channels(input[base + index as usize * stride])
            }
        };
        let mut sum = [0u32; 4];
        for i in 0..n.min(length as i64) {
            for (s, value) in sum.iter_mut().zip(get(i)) {
                *s += value;
            }
        }
        for i in 0..length as i64 {
            let left = get(i - n);
            let right = get(i + n);
            let mut out = [0u8; 4];
            for c in 0..4 {
                let value = (sum[c] as f64 + edge_weight * (left[c] + right[c]) as f64) / divisor;
                // Stabilize values within a few f64 ulps below an exact integer.
                // This keeps a mathematically constant byte plane constant.
                out[c] = (value + 32.0 * f64::EPSILON * value.abs())
                    .floor()
                    .clamp(0.0, 255.0) as u8;
            }
            output.data[base + i as usize * stride] = RGBA8::new(out[0], out[1], out[2], out[3]);
            let remove = get(i - n + 1);
            let add = get(i + n);
            for c in 0..4 {
                sum[c] = sum[c] - remove[c] + add[c];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn box3_literal_impulse_zero_axis_and_constant_interior() {
        let mut p = tiny_skia::Pixmap::new(9, 1).unwrap();
        p.data_mut()[16..20].copy_from_slice(&[255; 4]);
        apply([1.0, 0.0], &mut p, None, false).unwrap();
        assert_eq!(
            p.pixels().iter().map(|p| p.alpha()).collect::<Vec<_>>(),
            vec![0, 3, 23, 59, 79, 59, 23, 3, 0]
        );
        assert!(p
            .pixels()
            .iter()
            .all(|p| p.red() == p.alpha() && p.green() == p.alpha() && p.blue() == p.alpha()));
        let before = p.data().to_vec();
        apply([0.0, 0.5], &mut p, None, false).unwrap();
        assert_eq!(p.data(), before);
        let mut constant = tiny_skia::Pixmap::new(40, 40).unwrap();
        constant.fill(tiny_skia::Color::WHITE);
        apply([2.3, 2.3], &mut constant, None, false).unwrap();
        assert_eq!(constant.pixel(20, 20).unwrap().alpha(), 255);
        assert!(constant.pixel(0, 0).unwrap().alpha() < 255);
    }
    #[test]
    fn linear_sliding_windows_equal_independent_direct_convolution() {
        let (w, h) = (7usize, 5usize);
        let input: Vec<_> = (0..w * h)
            .map(|i| {
                let a = (i * 31 % 256) as u8;
                RGBA8::new(a / 2, a / 3, 0, a)
            })
            .collect();
        for radius in [0.6, 1.0, 1.5, 2.32, 8.5, 8192.0] {
            for horizontal in [true, false] {
                let mut output = vec![RGBA8::default(); w * h];
                pass(
                    &input,
                    ImageRefMut::new(w as u32, h as u32, &mut output),
                    radius,
                    horizontal,
                );
                let n = (radius + 0.5_f64).floor() as i64;
                for y in 0..h {
                    for x in 0..w {
                        let mut sums = [0.0; 4];
                        for delta in -n..=n {
                            let (sx, sy) = if horizontal {
                                (x as i64 + delta, y as i64)
                            } else {
                                (x as i64, y as i64 + delta)
                            };
                            if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                                continue;
                            }
                            let weight = if delta.abs() == n {
                                radius - (n as f64 - 0.5)
                            } else {
                                1.0
                            };
                            for (sum, v) in sums
                                .iter_mut()
                                .zip(channels(input[sy as usize * w + sx as usize]))
                            {
                                *sum += weight * v as f64;
                            }
                        }
                        let expected = sums.map(|v| ((v / (2.0 * radius)) + 1e-12).floor() as u8);
                        assert_eq!(
                            channels(output[y * w + x]),
                            expected.map(u32::from),
                            "radius {radius}, horizontal {horizontal}, at {x},{y}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn box3_rejects_invalid_radii_and_checked_scratch_budget() {
        let mut p = tiny_skia::Pixmap::new(10, 10).unwrap();
        p.fill(tiny_skia::Color::WHITE);
        let before = p.data().to_vec();
        for radius in [-1.0, f64::NAN, f64::INFINITY, 8192.01] {
            assert_eq!(
                apply([radius, 1.0], &mut p, None, false),
                Err(RenderErrorKind::InvalidBounds)
            );
            assert_eq!(p.data(), before);
        }
        let options = crate::CheckedRenderOptions {
            limits: crate::RenderLimits {
                max_live_bytes: 799,
                ..Default::default()
            },
            ..Default::default()
        };
        let state = checked::CheckedState::new(&options, 400).unwrap();
        assert_eq!(
            apply([1.0, 1.0], &mut p, Some(&state), false),
            Err(RenderErrorKind::AllocationLimit)
        );
        assert_eq!(p.data(), before);
    }
}
