//! Precision checks for flattening the pinned renderer's radial shaders.
use resvg::tiny_skia;

// tiny-skia 0.11.4 uses different thresholds for radius and focal distance.
const MIN_RADIUS: f32 = 1. / 4096.;
const CONCENTRIC_DISTANCE: f32 = 1. / 32768.;
const FIELD_TOLERANCE: f64 = 1e-6;

#[derive(Clone, Copy)]
pub(super) struct RadialCircle {
    pub(super) center: [f64; 2],
    pub(super) radius: f64,
    pub(super) focus: [f64; 2],
}

pub(super) fn validate_radial_circle(
    center: [f64; 2],
    radius: f64,
    focus: [f64; 2],
) -> Result<(), String> {
    let distance = (focus[0] - center[0]).hypot(focus[1] - center[1]);
    if center.into_iter().chain(focus).any(|v| !v.is_finite())
        || !radius.is_finite()
        || radius as f32 <= MIN_RADIUS
        || !distance.is_finite()
        || distance * 100. / radius > 99.9
    {
        return Err(
            "SVG radial gradient needs a renderer-visible radius and focus within 99.9%".into(),
        );
    }
    Ok(())
}

/// `shader` and `actual` contain values already rounded to the source renderer's
/// f32 lengths and the native emitted SVG's f32 lengths, respectively.
pub(super) fn validate_radial_precision(
    ideal: RadialCircle,
    shader: RadialCircle,
    shader_transform: tiny_skia::Transform,
    actual: RadialCircle,
) -> Result<(), String> {
    validate_radial_circle(ideal.center, ideal.radius, ideal.focus)?;
    let source = ShaderField::new(shader, shader_transform)?;
    let native = ShaderField::new(actual, tiny_skia::Transform::identity())?;
    // In particular, importing the native SVG itself must not reject a focal
    // shader merely because its existing f32 arithmetic differs from ideal math.
    if source == native {
        return Ok(());
    }
    let error = source.relative_error(native)?;
    if !error.is_finite() || error > FIELD_TOLERANCE {
        return Err("SVG radial gradient field loses precision in the editable renderer".into());
    }
    Ok(())
}

// The actual stored f32 shader coefficients, interpreted exactly in f64, can be
// put in the form |M p + q - t(h,0)| = t. Thus every sublevel set is an ellipse,
// with a strictly positive expansion margin 1-h. This also covers the renderer's
// near-concentric branch, whose effective center is the focus, not the center.
#[derive(Clone, Copy, PartialEq)]
struct ShaderField {
    matrix: [f64; 4],
    offset: [f64; 2],
    highlight: f64,
}

impl ShaderField {
    fn new(circle: RadialCircle, transform: tiny_skia::Transform) -> Result<Self, String> {
        let center = circle.center.map(|v| v as f32);
        let focus = circle.focus.map(|v| v as f32);
        let radius = circle.radius as f32;
        if center.into_iter().chain(focus).any(|v| !v.is_finite())
            || !radius.is_finite()
            || radius <= MIN_RADIUS
        {
            return Err("SVG radial gradient is degenerate at renderer precision".into());
        }
        let inverse = transform
            .invert()
            .filter(tiny_skia::Transform::is_finite)
            .ok_or("SVG radial gradient transform collapses at renderer precision")?;
        let delta = [center[0] - focus[0], center[1] - focus[1]];
        let distance = tiny_skia::Point::from_xy(delta[0], delta[1]).length();
        if !distance.is_finite() || distance >= radius {
            return Err(
                "SVG radial gradient focus reaches its circle at renderer precision".into(),
            );
        }
        // Mirror tiny-skia's shader construction, including its f32 operation
        // order and transform inversion/concatenation. Checking only the rounded
        // centers and radius misses cancellation in these stored coefficients.
        let (unit, highlight) = if distance <= CONCENTRIC_DISTANCE {
            (
                tiny_skia::Transform::from_translate(-focus[0], -focus[1])
                    .post_scale(1. / radius, 1. / radius),
                0.,
            )
        } else {
            let from = tiny_skia::Transform::from_row(
                delta[1], -delta[0], delta[0], delta[1], focus[0], focus[1],
            );
            let to = tiny_skia::Transform::from_row(0., -1., 1., 0., 0., 0.);
            let unit = to.pre_concat(
                from.invert()
                    .filter(tiny_skia::Transform::is_finite)
                    .ok_or("SVG radial gradient focus transform collapses at renderer precision")?,
            );
            let r1 = radius / distance;
            if !r1.is_finite() || (1. - r1).abs() <= MIN_RADIUS || r1 <= 1. {
                return Err("SVG radial gradient focus is unstable at renderer precision".into());
            }
            let denominator = r1 * r1 - 1.;
            (
                unit.post_scale(r1 / denominator, 1. / denominator.abs().sqrt()),
                (1. / r1) as f64,
            )
        };
        let unit = inverse.post_concat(unit);
        if !unit.is_finite() {
            return Err("SVG radial gradient shader loses precision".into());
        }
        // For the focal shader t = hypot(x,y) - h*x. Its equivalent
        // circle-family coordinates are ((1-h*h)*x, sqrt(1-h*h)*y).
        let x_scale = 1. - highlight * highlight;
        let y_scale = x_scale.sqrt();
        let result = Self {
            matrix: [
                x_scale * unit.sx as f64,
                y_scale * unit.ky as f64,
                x_scale * unit.kx as f64,
                y_scale * unit.sy as f64,
            ],
            offset: [x_scale * unit.tx as f64, y_scale * unit.ty as f64],
            highlight,
        };
        let [a, b, c, d] = result.matrix;
        let determinant = a * d - b * c;
        if result
            .matrix
            .into_iter()
            .chain(result.offset)
            .any(|v| !v.is_finite())
            || !determinant.is_finite()
            || determinant == 0.
            || !(0. ..1.).contains(&highlight)
        {
            return Err("SVG radial gradient shader collapses at renderer precision".into());
        }
        Ok(result)
    }

    fn relative_error(self, native: Self) -> Result<f64, String> {
        // Express the source field in the native field's normalized coordinates.
        let [a, b, c, d] = native.matrix;
        let determinant = a * d - b * c;
        let inverse = [
            d / determinant,
            -b / determinant,
            -c / determinant,
            a / determinant,
        ];
        let [a, b, c, d] = self.matrix;
        let [e, f, g, h] = inverse;
        let matrix = [a * e + c * f, b * e + d * f, a * g + c * h, b * g + d * h];
        let [a, b, c, d] = matrix;
        let focus = [
            self.offset[0] - a * native.offset[0] - c * native.offset[1],
            self.offset[1] - b * native.offset[0] - d * native.offset[1],
        ];
        let center = [
            focus[0] + a * native.highlight - self.highlight,
            focus[1] + b * native.highlight,
        ];
        // Singular values bound every radius direction; no finite sampling can
        // give this guarantee for all angles, especially with a near-edge focus.
        let maximum = ((a + d).hypot(b - c) + (a - d).hypot(b + c)) / 2.;
        let minimum = (a * d - b * c).abs() / maximum;
        let radius_error = (maximum - 1.).abs().max((minimum - 1.).abs());
        let focus_error = focus[0].hypot(focus[1]);
        let center_error = center[0].hypot(center[1]);
        let error = focus_error.max(center_error + radius_error) / (1. - self.highlight);
        if !error.is_finite() {
            return Err("SVG radial gradient field is unstable at renderer precision".into());
        }
        // On a native t-circle (0 <= t <= 1), the source implicit residual is at
        // most (1-t)*focus_error + t*(center_error+radius_error). Its derivative
        // in t has magnitude >= 1-h, giving the bound above. Both fields are
        // convex gauges; when error < 1/2 the same bound holds after padding to
        // [0,1], including outside the native outer ellipse. Raster per-pixel
        // arithmetic/antialiasing is intentionally not a pixel-equality promise.
        Ok(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(center: [f64; 2], radius: f64, focus: [f64; 2]) -> RadialCircle {
        RadialCircle {
            center,
            radius,
            focus,
        }
    }

    #[test]
    fn radial_radius_cutoff_is_distinct_from_linear_cutoff() {
        assert!(validate_radial_circle([0.; 2], 1. / 4096., [0.; 2]).is_err());
        assert!(validate_radial_circle([0.; 2], 1. / 2048., [0.; 2]).is_ok());
        let shader = circle([0.; 2], 1. / 8192., [0.; 2]);
        let actual = circle([0.; 2], 1., [0.; 2]);
        assert!(
            validate_radial_precision(
                actual,
                shader,
                tiny_skia::Transform::from_scale(8192., 8192.),
                actual,
            )
            .is_err()
        );
    }

    #[test]
    fn identical_near_edge_shader_is_preserved() {
        let ideal = circle([0.; 2], 100., [99.9, 0.]);
        let rounded = circle([0.; 2], 100., [99.9_f32 as f64, 0.]);
        assert!(
            validate_radial_precision(ideal, rounded, tiny_skia::Transform::identity(), rounded,)
                .is_ok()
        );
        assert!(validate_radial_circle([0.; 2], 100., [99.90001, 0.]).is_err());
        // Multiplying .3 by .999 rounds below .2997; compute the same native
        // percentage that will actually be stored instead of rejecting it.
        let fractional = circle([0.; 2], 0.3, [0.2997, 0.]);
        assert!(
            validate_radial_precision(
                fractional,
                fractional,
                tiny_skia::Transform::identity(),
                fractional,
            )
            .is_ok()
        );
    }

    #[test]
    fn rotation_reflection_and_scale_preserve_centered_shader() {
        let shader = circle([0.; 2], 1., [0.; 2]);
        let actual = circle([20., 30.], 20., [20., 30.]);
        for transform in [
            tiny_skia::Transform::from_row(0., 20., -20., 0., 20., 30.),
            tiny_skia::Transform::from_row(-20., 0., 0., 20., 20., 30.),
        ] {
            assert!(validate_radial_precision(actual, shader, transform, actual).is_ok());
        }
    }

    #[test]
    fn materially_changed_near_concentric_branch_rejects() {
        let shader = circle([0.; 2], 1., [0.00001, 0.]);
        let actual = circle([0.; 2], 100000., [1., 0.]);
        assert!(
            validate_radial_precision(
                actual,
                shader,
                tiny_skia::Transform::from_scale(100000., 100000.),
                actual,
            )
            .is_err()
        );
    }

    #[test]
    fn immaterial_near_concentric_branch_change_is_allowed() {
        let shader = circle([0.; 2], 1., [0.00000001, 0.]);
        let actual = circle([0.; 2], 1000000., [0.01, 0.]);
        assert!(
            validate_radial_precision(
                actual,
                shader,
                tiny_skia::Transform::from_scale(1000000., 1000000.),
                actual,
            )
            .is_ok()
        );
    }

    #[test]
    fn native_radius_is_a_scalar_not_f32_endpoint_difference() {
        let actual = circle([100000., 0.], 0.02, [100000., 0.]);
        assert!(
            validate_radial_precision(actual, actual, tiny_skia::Transform::identity(), actual,)
                .is_ok()
        );
    }

    #[test]
    fn focal_conditioning_tightens_the_field_bound() {
        let centered = ShaderField {
            matrix: [1., 0., 0., 1.],
            offset: [0.; 2],
            highlight: 0.,
        };
        let shifted = ShaderField {
            offset: [2e-9, 0.],
            ..centered
        };
        assert!(shifted.relative_error(centered).unwrap() < FIELD_TOLERANCE);
        let near_edge = ShaderField {
            highlight: 0.999,
            ..centered
        };
        let shifted = ShaderField {
            offset: [2e-9, 0.],
            ..near_edge
        };
        assert!(shifted.relative_error(near_edge).unwrap() > FIELD_TOLERANCE);
    }
}
