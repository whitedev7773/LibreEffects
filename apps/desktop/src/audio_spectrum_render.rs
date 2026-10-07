//! NativeV1 spectrum geometry, independent of audio analysis and preview scale.
//!
//! Samples include both baseline endpoints when there are at least two bands;
//! one band is placed at `start`. Above uses `(dy, -dx) / baseline_length` in
//! SVG's downward-positive Y coordinates. Amplitudes are clamped to one only
//! when calculating height. Line joins samples with round joins/caps; a
//! one-band Line is a circle. Bars use round-capped baseline-to-height strokes
//! (a silent bar is a round dot). Points are circles of radius thickness / 2.
//! Silent Line samples remain on the baseline. Both mirrors the geometry,
//! omits coincident zero-height geometry, and applies color alpha once to the
//! opaque geometry group, including where strokes or circles overlap.
//!
//! A user-space clip always limits painted geometry, including stroke extents,
//! to the nominal layer rectangle. This fragment never paints the original
//! rectangle: the caller composes it first when `composite_original` is set.

use libre_effects_core::{AudioSpectrumSettings, SpectrumDisplay, SpectrumSide};
use std::{
    fmt,
    sync::atomic::{AtomicBool, Ordering},
};

const SVG_LIMIT: usize = 8 * 1024 * 1024;
const PREFIX_LIMIT: usize = 1024;
const CANCEL_BATCH: usize = 64;

/// Generate only the spectrum, in unscaled layer coordinates. `prefix` must
/// be unique in the containing SVG and contain 1..=1024 ASCII letters, digits,
/// hyphens, underscores or periods. Layer dimensions follow core's 1..=16384
/// domain. No result is returned on invalid input, cancellation or overflow.
pub(crate) fn spectrum_svg(
    settings: &AudioSpectrumSettings,
    amplitudes: &[f64],
    width: f64,
    height: f64,
    prefix: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    spectrum_svg_with_cancel(settings, amplitudes, width, height, prefix, &|| {
        cancel.load(Ordering::Relaxed)
    })
}

fn spectrum_svg_with_cancel(
    settings: &AudioSpectrumSettings,
    amplitudes: &[f64],
    width: f64,
    height: f64,
    prefix: &str,
    cancel: &dyn Fn() -> bool,
) -> Result<String, String> {
    check_cancel(cancel)?;
    settings.validate()?;
    if [width, height]
        .iter()
        .any(|value| !value.is_finite() || !(1.0..=16_384.0).contains(value))
    {
        return Err("Invalid native Audio Spectrum layer dimensions".into());
    }
    if prefix.is_empty()
        || prefix.len() > PREFIX_LIMIT
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("Invalid native Audio Spectrum SVG prefix".into());
    }
    if amplitudes.len() != usize::from(settings.bands) {
        return Err("Native Audio Spectrum amplitude count does not match bands".into());
    }
    for (index, amplitude) in amplitudes.iter().enumerate() {
        poll_cancel(index, cancel)?;
        if !amplitude.is_finite() || *amplitude < 0.0 {
            return Err("Native Audio Spectrum amplitudes must be finite and nonnegative".into());
        }
    }
    check_cancel(cancel)?;

    let delta = [
        settings.end[0] - settings.start[0],
        settings.end[1] - settings.start[1],
    ];
    let length = delta[0].hypot(delta[1]);
    let geometry = Geometry {
        settings,
        amplitudes,
        normal: [delta[1] / length, -delta[0] / length],
    };
    let mut output = BoundedSvg::new(SVG_LIMIT);
    let [red, green, blue, alpha] = settings.color;
    let opacity = f64::from(alpha) / 255.0;
    output.append(format_args!(
        "<defs><clipPath id='spectrum-{prefix}-clip' clipPathUnits='userSpaceOnUse'><rect x='0' y='0' width='{width}' height='{height}'/></clipPath></defs><g clip-path='url(#spectrum-{prefix}-clip)' opacity='{opacity}'>"
    ))?;
    match settings.display {
        SpectrumDisplay::Line if amplitudes.len() > 1 => {
            stroke_start(&mut output, settings, red, green, blue)?;
            let side = first_side(settings.side);
            for index in 0..amplitudes.len() {
                poll_cancel(index, cancel)?;
                let [x, y] = geometry.point(index, side);
                let command = if index == 0 { 'M' } else { 'L' };
                output.append(format_args!("{command}{x} {y} "))?;
            }
            if settings.side == SpectrumSide::Both {
                // The mirrored path omits baseline segments already present
                // in the primary path. Restart after each omitted segment.
                let mut connected = false;
                for index in 1..amplitudes.len() {
                    poll_cancel(index - 1, cancel)?;
                    if geometry.height(index - 1) == 0.0 && geometry.height(index) == 0.0 {
                        connected = false;
                        continue;
                    }
                    if !connected {
                        let [x, y] = geometry.point(index - 1, -1.0);
                        output.append(format_args!("M{x} {y} "))?;
                    }
                    let [x, y] = geometry.point(index, -1.0);
                    output.append(format_args!("L{x} {y} "))?;
                    connected = true;
                }
            }
            output.append(format_args!("'/>",))?;
        }
        SpectrumDisplay::Bars => {
            stroke_start(&mut output, settings, red, green, blue)?;
            for index in 0..amplitudes.len() {
                poll_cancel(index, cancel)?;
                let (start, end) = if settings.side == SpectrumSide::Both {
                    // A single above-to-below stroke is the union of the two
                    // baseline-to-height bars, with no duplicate baseline cap.
                    (geometry.point(index, 1.0), geometry.point(index, -1.0))
                } else {
                    (
                        geometry.baseline(index),
                        geometry.point(index, first_side(settings.side)),
                    )
                };
                output.append(format_args!(
                    "M{} {} L{} {} ",
                    start[0], start[1], end[0], end[1]
                ))?;
            }
            output.append(format_args!("'/>",))?;
        }
        SpectrumDisplay::Line | SpectrumDisplay::Points => {
            // The Line arm here contains exactly one sample. Use an explicit
            // circle instead of a move-only path, which paints nothing.
            output.append(format_args!("<g fill='rgb({red},{green},{blue})'>"))?;
            let radius = settings.thickness / 2.0;
            for index in 0..amplitudes.len() {
                poll_cancel(index, cancel)?;
                circle(
                    &mut output,
                    geometry.point(index, first_side(settings.side)),
                    radius,
                )?;
                if settings.side == SpectrumSide::Both && geometry.height(index) != 0.0 {
                    circle(&mut output, geometry.point(index, -1.0), radius)?;
                }
            }
            output.append(format_args!("</g>"))?;
        }
    }
    output.append(format_args!("</g>"))?;
    check_cancel(cancel)?;
    Ok(output.value)
}

fn first_side(side: SpectrumSide) -> f64 {
    match side {
        SpectrumSide::Above | SpectrumSide::Both => 1.0,
        SpectrumSide::Below => -1.0,
    }
}

struct Geometry<'a> {
    settings: &'a AudioSpectrumSettings,
    amplitudes: &'a [f64],
    normal: [f64; 2],
}

impl Geometry<'_> {
    fn baseline(&self, index: usize) -> [f64; 2] {
        if index == 0 {
            return self.settings.start;
        }
        if index + 1 == self.amplitudes.len() {
            return self.settings.end;
        }
        let fraction = index as f64 / (self.amplitudes.len() - 1) as f64;
        std::array::from_fn(|axis| {
            self.settings.start[axis]
                + (self.settings.end[axis] - self.settings.start[axis]) * fraction
        })
    }

    fn height(&self, index: usize) -> f64 {
        self.amplitudes[index].min(1.0) * self.settings.maximum_height
    }

    fn point(&self, index: usize, side: f64) -> [f64; 2] {
        let baseline = self.baseline(index);
        let height = self.height(index) * side;
        std::array::from_fn(|axis| baseline[axis] + self.normal[axis] * height)
    }
}

fn stroke_start(
    output: &mut BoundedSvg,
    settings: &AudioSpectrumSettings,
    red: u8,
    green: u8,
    blue: u8,
) -> Result<(), String> {
    output.append(format_args!(
        "<path fill='none' stroke='rgb({red},{green},{blue})' stroke-width='{}' stroke-linecap='round' stroke-linejoin='round' d='",
        settings.thickness
    ))
}

fn circle(output: &mut BoundedSvg, [x, y]: [f64; 2], radius: f64) -> Result<(), String> {
    output.append(format_args!("<circle cx='{x}' cy='{y}' r='{radius}'/>"))
}

fn poll_cancel(index: usize, cancel: &dyn Fn() -> bool) -> Result<(), String> {
    if index % CANCEL_BATCH == 0 {
        check_cancel(cancel)?;
    }
    Ok(())
}

fn check_cancel(cancel: &dyn Fn() -> bool) -> Result<(), String> {
    if cancel() {
        return Err("Native Audio Spectrum rendering cancelled".into());
    }
    Ok(())
}

/// Count before reserving/formatting so even a single oversized append is
/// rejected before it can allocate or leave a partially appended fragment.
struct BoundedSvg {
    value: String,
    limit: usize,
}

impl BoundedSvg {
    fn new(limit: usize) -> Self {
        Self {
            value: String::new(),
            limit,
        }
    }

    fn append(&mut self, args: fmt::Arguments<'_>) -> Result<(), String> {
        struct Count {
            bytes: usize,
            limit: usize,
        }
        impl fmt::Write for Count {
            fn write_str(&mut self, value: &str) -> fmt::Result {
                self.bytes = self.bytes.checked_add(value.len()).ok_or(fmt::Error)?;
                if self.bytes > self.limit {
                    return Err(fmt::Error);
                }
                Ok(())
            }
        }
        let mut count = Count {
            bytes: self.value.len(),
            limit: self.limit,
        };
        fmt::write(&mut count, args)
            .map_err(|_| "Native Audio Spectrum SVG exceeds its 8 MiB limit".to_string())?;
        self.value
            .try_reserve_exact(count.bytes - self.value.len())
            .map_err(|_| "Could not allocate bounded native Audio Spectrum SVG".to_string())?;
        fmt::write(&mut self.value, args)
            .map_err(|_| "Could not format native Audio Spectrum SVG".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn settings(display: SpectrumDisplay, side: SpectrumSide) -> AudioSpectrumSettings {
        AudioSpectrumSettings {
            bands: 3,
            start: [10.0, 50.0],
            end: [30.0, 50.0],
            maximum_height: 20.0,
            thickness: 4.0,
            color: [18, 52, 86, 128],
            display,
            side,
            ..Default::default()
        }
    }

    fn render(settings: &AudioSpectrumSettings, amplitudes: &[f64]) -> String {
        spectrum_svg(
            settings,
            amplitudes,
            100.0,
            100.0,
            "test-1",
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    fn path(svg: &str) -> &str {
        svg.split(" d='")
            .nth(1)
            .unwrap()
            .split('\'')
            .next()
            .unwrap()
    }

    #[test]
    fn horizontal_line_has_literal_positions_and_both_side_signs() {
        let mut settings = settings(SpectrumDisplay::Line, SpectrumSide::Above);
        assert_eq!(
            path(&render(&settings, &[0.0, 0.5, 1.0])),
            "M10 50 L20 40 L30 30 "
        );
        settings.side = SpectrumSide::Below;
        assert_eq!(
            path(&render(&settings, &[0.0, 0.5, 1.0])),
            "M10 50 L20 60 L30 70 "
        );
        settings.side = SpectrumSide::Both;
        assert_eq!(
            path(&render(&settings, &[0.0, 0.5, 1.0])),
            "M10 50 L20 40 L30 30 M10 50 L20 60 L30 70 "
        );
    }

    #[test]
    fn vertical_reversed_and_diagonal_baselines_use_oriented_normal() {
        let mut settings = settings(SpectrumDisplay::Line, SpectrumSide::Above);
        settings.bands = 2;
        settings.start = [20.0, 10.0];
        settings.end = [20.0, 30.0];
        assert_eq!(path(&render(&settings, &[0.5, 1.0])), "M30 10 L40 30 ");
        settings.start = [30.0, 50.0];
        settings.end = [10.0, 50.0];
        assert_eq!(path(&render(&settings, &[0.5, 1.0])), "M30 60 L10 70 ");
        settings.start = [10.0, 20.0];
        settings.end = [40.0, 60.0];
        settings.maximum_height = 10.0;
        assert_eq!(path(&render(&settings, &[1.0, 1.0])), "M18 14 L48 54 ");
    }

    #[test]
    fn bars_cover_all_sides_and_keep_silent_sample_once() {
        let mut settings = settings(SpectrumDisplay::Bars, SpectrumSide::Above);
        assert_eq!(
            path(&render(&settings, &[0.0, 0.5, 1.0])),
            "M10 50 L10 50 M20 50 L20 40 M30 50 L30 30 "
        );
        settings.side = SpectrumSide::Below;
        assert_eq!(
            path(&render(&settings, &[0.0, 0.5, 1.0])),
            "M10 50 L10 50 M20 50 L20 60 M30 50 L30 70 "
        );
        settings.side = SpectrumSide::Both;
        assert_eq!(
            path(&render(&settings, &[0.0, 0.5, 1.0])),
            "M10 50 L10 50 M20 40 L20 60 M30 30 L30 70 "
        );
    }

    #[test]
    fn points_cover_all_sides_and_omit_coincident_mirror() {
        for (side, y_values) in [
            (SpectrumSide::Above, &[50, 40, 30][..]),
            (SpectrumSide::Below, &[50, 60, 70][..]),
            (SpectrumSide::Both, &[50, 40, 60, 30, 70][..]),
        ] {
            let svg = render(&settings(SpectrumDisplay::Points, side), &[0.0, 0.5, 1.0]);
            assert_eq!(svg.matches("<circle ").count(), y_values.len());
            let actual: Vec<_> = svg
                .split(" cy='")
                .skip(1)
                .map(|value| value.split('\'').next().unwrap().parse::<u32>().unwrap())
                .collect();
            assert_eq!(actual, y_values);
            assert_eq!(svg.matches(" r='2'").count(), y_values.len());
            assert!(!svg.contains("<path"));
        }
    }

    #[test]
    fn one_band_is_placed_at_start_for_every_display_and_side() {
        for display in [
            SpectrumDisplay::Line,
            SpectrumDisplay::Bars,
            SpectrumDisplay::Points,
        ] {
            for side in [SpectrumSide::Above, SpectrumSide::Below, SpectrumSide::Both] {
                let mut settings = settings(display, side);
                settings.bands = 1;
                let svg = render(&settings, &[0.5]);
                if display == SpectrumDisplay::Bars {
                    assert_eq!(
                        path(&svg),
                        match side {
                            SpectrumSide::Above => "M10 50 L10 40 ",
                            SpectrumSide::Below => "M10 50 L10 60 ",
                            SpectrumSide::Both => "M10 40 L10 60 ",
                        }
                    );
                } else {
                    let count = if side == SpectrumSide::Both { 2 } else { 1 };
                    assert_eq!(svg.matches("<circle cx='10'").count(), count);
                    if side != SpectrumSide::Below {
                        assert!(svg.contains("<circle cx='10' cy='40' r='2'/>"));
                    }
                    if side != SpectrumSide::Above {
                        assert!(svg.contains("<circle cx='10' cy='60' r='2'/>"));
                    }
                }
            }
        }
    }

    #[test]
    fn silence_and_zero_maximum_height_do_not_duplicate_mirrors() {
        for display in [
            SpectrumDisplay::Line,
            SpectrumDisplay::Bars,
            SpectrumDisplay::Points,
        ] {
            let mut settings = settings(display, SpectrumSide::Both);
            let silent = render(&settings, &[0.0; 3]);
            settings.maximum_height = 0.0;
            assert_eq!(render(&settings, &[1.0; 3]), silent);
            match display {
                SpectrumDisplay::Line => assert_eq!(path(&silent), "M10 50 L20 50 L30 50 "),
                SpectrumDisplay::Bars => {
                    assert_eq!(path(&silent), "M10 50 L10 50 M20 50 L20 50 M30 50 L30 50 ")
                }
                SpectrumDisplay::Points => assert_eq!(silent.matches("<circle ").count(), 3),
            }
            settings.bands = 1;
            let single = render(&settings, &[0.0]);
            if display != SpectrumDisplay::Bars {
                assert_eq!(single.matches("<circle ").count(), 1);
                assert!(single.contains("<circle cx='10' cy='50' r='2'/>"));
            }
        }
    }

    #[test]
    fn mirrored_line_omits_baseline_runs_and_restarts_after_gaps() {
        let mut settings = settings(SpectrumDisplay::Line, SpectrumSide::Both);
        settings.bands = 6;
        settings.end = [60.0, 50.0];
        assert_eq!(
            path(&render(&settings, &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0])),
            "M10 50 L20 50 L30 30 L40 50 L50 50 L60 30 M20 50 L30 70 L40 50 M50 50 L60 70 "
        );
    }

    #[test]
    fn cap_applies_to_height_without_mutating_amplitudes() {
        let settings = settings(SpectrumDisplay::Line, SpectrumSide::Above);
        let amplitudes = [1.0, 2.0, f64::MAX];
        assert_eq!(
            path(&render(&settings, &amplitudes)),
            "M10 30 L20 30 L30 30 "
        );
        assert_eq!(amplitudes, [1.0, 2.0, f64::MAX]);
    }

    #[test]
    fn color_alpha_and_round_geometry_are_explicit() {
        for display in [
            SpectrumDisplay::Line,
            SpectrumDisplay::Bars,
            SpectrumDisplay::Points,
        ] {
            let svg = render(&settings(display, SpectrumSide::Both), &[0.0, 0.5, 1.0]);
            assert_eq!(svg.matches("opacity='").count(), 1);
            assert!(svg.contains("opacity='0.5019607843137255'"));
            assert_eq!(svg.matches("rgb(18,52,86)").count(), 1);
            if display != SpectrumDisplay::Points {
                assert!(
                    svg.contains("stroke-width='4' stroke-linecap='round' stroke-linejoin='round'")
                );
            }
        }
        let mut settings = settings(SpectrumDisplay::Line, SpectrumSide::Both);
        settings.color[3] = 0;
        assert!(render(&settings, &[0.0; 3]).contains("opacity='0'"));
        settings.color[3] = 255;
        assert!(render(&settings, &[0.0; 3]).contains("opacity='1'"));
    }

    #[test]
    fn nominal_clip_wraps_all_geometry_and_original_is_callers_responsibility() {
        let mut settings = settings(SpectrumDisplay::Line, SpectrumSide::Both);
        settings.start = [-10.0, 0.0];
        settings.end = [110.0, 0.0];
        let svg = render(&settings, &[1.0; 3]);
        assert!(svg.starts_with("<defs><clipPath id='spectrum-test-1-clip' clipPathUnits='userSpaceOnUse'><rect x='0' y='0' width='100' height='100'/></clipPath></defs><g clip-path='url(#spectrum-test-1-clip)'"));
        assert!(svg.ends_with("'/></g>"));
        assert!(path(&svg).contains("M-10 -20"));
        assert_eq!(svg.matches("<rect ").count(), 1);
        settings.composite_original = true;
        assert_eq!(render(&settings, &[1.0; 3]), svg);
    }

    #[test]
    fn full_profile_preserves_all_1920_samples_and_endpoints() {
        let settings = AudioSpectrumSettings {
            bands: 1920,
            start: [0.0, 960.0],
            end: [1920.0, 960.0],
            maximum_height: 480.0,
            thickness: 4.0,
            color: [255; 4],
            ..Default::default()
        };
        let svg = spectrum_svg(
            &settings,
            &[1.0; 1920],
            1920.0,
            1080.0,
            "profile",
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(path(&svg).starts_with("M0 480 L"));
        assert!(path(&svg).ends_with("L1920 480 "));
        assert_eq!(path(&svg).matches('L').count(), 1919);
        assert!(svg.contains("stroke='rgb(255,255,255)' stroke-width='4'"));
        assert!(svg.len() < SVG_LIMIT);
    }

    #[test]
    fn admitted_band_counts_remain_independent_of_layer_dimensions() {
        for count in [1, 1920, 4096] {
            for display in [
                SpectrumDisplay::Line,
                SpectrumDisplay::Bars,
                SpectrumDisplay::Points,
            ] {
                let mut settings = settings(display, SpectrumSide::Above);
                settings.bands = count;
                let amplitudes = vec![0.5; usize::from(count)];
                let svg = spectrum_svg(
                    &settings,
                    &amplitudes,
                    1.0,
                    1.0,
                    "small",
                    &AtomicBool::new(false),
                )
                .unwrap();
                match display {
                    SpectrumDisplay::Line if count > 1 => {
                        assert_eq!(path(&svg).matches('L').count(), usize::from(count) - 1)
                    }
                    SpectrumDisplay::Bars => {
                        assert_eq!(path(&svg).matches('M').count(), usize::from(count))
                    }
                    _ => assert_eq!(svg.matches("<circle ").count(), usize::from(count)),
                }
                assert!(svg.len() < SVG_LIMIT);
            }
        }
    }

    #[test]
    fn rejects_mismatched_nonfinite_and_negative_amplitudes() {
        let settings = settings(SpectrumDisplay::Line, SpectrumSide::Above);
        let cancel = AtomicBool::new(false);
        for amplitudes in [
            &[][..],
            &[0.0, 0.0],
            &[0.0; 4],
            &[0.0, -0.1, 0.0],
            &[0.0, f64::NAN, 0.0],
            &[f64::INFINITY, 0.0, 0.0],
            &[0.0, 0.0, f64::NEG_INFINITY],
        ] {
            assert!(spectrum_svg(&settings, amplitudes, 100.0, 100.0, "test", &cancel).is_err());
        }
    }

    #[test]
    fn rejects_invalid_dimensions_settings_and_unsafe_prefixes() {
        let mut settings = settings(SpectrumDisplay::Line, SpectrumSide::Above);
        let cancel = AtomicBool::new(false);
        for bad in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.0,
            0.0,
            0.5,
            16_385.0,
        ] {
            for (width, height) in [(bad, 100.0), (100.0, bad)] {
                assert!(
                    spectrum_svg(&settings, &[0.0; 3], width, height, "test", &cancel).is_err()
                );
            }
        }
        for prefix in [
            "",
            "a b",
            "a'b",
            "<tag>",
            "é",
            "a#b",
            "a/b",
            "a\0b",
            &"a".repeat(PREFIX_LIMIT + 1),
        ] {
            assert!(spectrum_svg(&settings, &[0.0; 3], 100.0, 100.0, prefix, &cancel).is_err());
        }
        assert!(spectrum_svg(&settings, &[0.0; 3], 100.0, 100.0, "a-Z_9.0", &cancel).is_ok());
        settings.start = settings.end;
        assert!(spectrum_svg(&settings, &[0.0; 3], 100.0, 100.0, "test", &cancel).is_err());
        settings.start = [0.0, f64::NAN];
        assert!(spectrum_svg(&settings, &[0.0; 3], 100.0, 100.0, "test", &cancel).is_err());
        settings.start = [10.0, 50.0];
        settings.bands = 4097;
        assert!(spectrum_svg(&settings, &[0.0; 3], 100.0, 100.0, "test", &cancel).is_err());
        settings.bands = 0;
        assert!(spectrum_svg(&settings, &[], 100.0, 100.0, "test", &cancel).is_err());
        settings.bands = 3;
        settings.thickness = 0.0;
        assert!(spectrum_svg(&settings, &[0.0; 3], 100.0, 100.0, "test", &cancel).is_err());
        settings.thickness = 4.0;
        settings.maximum_height = -1.0;
        assert!(spectrum_svg(&settings, &[0.0; 3], 100.0, 100.0, "test", &cancel).is_err());
        settings.maximum_height = 20.0;
        settings.duration_ms = 0.0;
        assert!(spectrum_svg(&settings, &[0.0; 3], 100.0, 100.0, "test", &cancel).is_err());
    }

    #[test]
    fn cancellation_is_checked_before_work_and_in_generation_batches() {
        let mut settings = settings(SpectrumDisplay::Points, SpectrumSide::Both);
        settings.bands = 4096;
        assert!(
            spectrum_svg(
                &settings,
                &[0.5; 4096],
                100.0,
                100.0,
                "test",
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .contains("cancelled")
        );
        for display in [
            SpectrumDisplay::Line,
            SpectrumDisplay::Bars,
            SpectrumDisplay::Points,
        ] {
            settings.display = display;
            let polls = Cell::new(0);
            let validation_polls = 2 + 4096 / CANCEL_BATCH;
            let result =
                spectrum_svg_with_cancel(&settings, &[0.5; 4096], 100.0, 100.0, "test", &|| {
                    polls.set(polls.get() + 1);
                    polls.get() == validation_polls + 3
                });
            assert!(result.unwrap_err().contains("cancelled"));
            assert_eq!(polls.get(), validation_polls + 3);
        }
    }

    #[test]
    fn bounded_writer_rejects_before_partial_append() {
        let mut output = BoundedSvg::new(5);
        output.append(format_args!("abc")).unwrap();
        assert!(output.append(format_args!("{}", "def")).is_err());
        assert_eq!(output.value, "abc");
        output.append(format_args!("de")).unwrap();
        assert_eq!(output.value, "abcde");
    }
}
