use crate::{ScalarError, ScalarInterpolation, ScalarKeyTiming};
use serde::{Deserialize, Serialize};

pub const MAX_INVERSION_ITERATIONS: u32 = 80;
const ROUNDOFF_FLOOR: f64 = 128.0 * f64::EPSILON;

/// Simultaneous absolute and relative value-error ceilings: the smaller bound
/// wins. In particular, large signed control values never relax the absolute
/// ceiling for an otherwise small scalar sample. Relative error is measured
/// against the maximum magnitude of the four value controls, not endpoint
/// delta or the sampled value (both can be zero on a nonconstant curve).
///
/// A normalized floating-point floor of 128 machine epsilons applies. Requests
/// below that floor fail explicitly, as do unrepresentable active displacements
/// and output rounding exceeding the requested ceiling. Bisection has a fixed
/// hard work bound and uses compensated centered time-polynomial arithmetic,
/// including when both influences are 100% and the derivative vanishes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamplingOptions {
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
    pub max_inversion_iterations: u32,
}

impl Default for SamplingOptions {
    fn default() -> Self {
        Self {
            absolute_tolerance: 1e-9,
            relative_tolerance: 1e-12,
            max_inversion_iterations: 64,
        }
    }
}

impl SamplingOptions {
    pub fn validate(self) -> Result<(), ScalarError> {
        if !self.absolute_tolerance.is_finite()
            || self.absolute_tolerance <= 0.0
            || !self.relative_tolerance.is_finite()
            || self.relative_tolerance <= 0.0
            || self.max_inversion_iterations == 0
            || self.max_inversion_iterations > MAX_INVERSION_ITERATIONS
        {
            return Err(ScalarError::InvalidSamplingOptions);
        }
        Ok(())
    }
}

/// Sample adjacent key data; outside the segment, return its exact endpoint.
/// Exact endpoints do not evaluate active controls or either neighboring side.
#[allow(clippy::too_many_arguments)]
pub fn sample_segment(
    a_frame: u32,
    a_value: f64,
    a: &ScalarKeyTiming,
    b_frame: u32,
    b_value: f64,
    b: &ScalarKeyTiming,
    frame: f64,
    seconds_per_frame: f64,
) -> Result<f64, ScalarError> {
    sample_segment_with_options(
        a_frame,
        a_value,
        a,
        b_frame,
        b_value,
        b,
        frame,
        seconds_per_frame,
        SamplingOptions::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn sample_segment_with_options(
    a_frame: u32,
    a_value: f64,
    a: &ScalarKeyTiming,
    b_frame: u32,
    b_value: f64,
    b: &ScalarKeyTiming,
    frame: f64,
    seconds_per_frame: f64,
    options: SamplingOptions,
) -> Result<f64, ScalarError> {
    validate_inputs(
        a_frame,
        a_value,
        a,
        b_frame,
        b_value,
        b,
        seconds_per_frame,
        options,
    )?;
    if !frame.is_finite() {
        return Err(ScalarError::InvalidTime);
    }
    if frame <= f64::from(a_frame) {
        return Ok(a_value);
    }
    if frame >= f64::from(b_frame) {
        return Ok(b_value);
    }
    if hold(a, b)? {
        return Ok(a_value);
    }
    let curve = Curve::new(
        a_frame,
        a_value,
        a,
        b_frame,
        b_value,
        b,
        seconds_per_frame,
        options,
    )?;
    if curve.constant {
        return Ok(a_value);
    }
    // Keep the frame subtraction and division remainder: rounding elapsed
    // time to one float is observable near a stationary time derivative.
    let elapsed = Double(frame, 0.0)
        .add(Double(-f64::from(a_frame), 0.0))
        .div(f64::from(b_frame - a_frame));
    let u = if a.out_interpolation == ScalarInterpolation::Linear
        && b.in_interpolation == ScalarInterpolation::Linear
    {
        elapsed.0 + elapsed.1
    } else {
        curve.invert_time(elapsed, options.max_inversion_iterations)?
    };
    curve.value(u)
}

/// Check active controls and numerical precision without keeping a key map or
/// sampling intermediate frames. Outgoing Hold makes both segment sides dormant.
#[allow(clippy::too_many_arguments)]
pub fn validate_segment(
    a_frame: u32,
    a_value: f64,
    a: &ScalarKeyTiming,
    b_frame: u32,
    b_value: f64,
    b: &ScalarKeyTiming,
    seconds_per_frame: f64,
) -> Result<(), ScalarError> {
    validate_segment_with_options(
        a_frame,
        a_value,
        a,
        b_frame,
        b_value,
        b,
        seconds_per_frame,
        SamplingOptions::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn validate_segment_with_options(
    a_frame: u32,
    a_value: f64,
    a: &ScalarKeyTiming,
    b_frame: u32,
    b_value: f64,
    b: &ScalarKeyTiming,
    seconds_per_frame: f64,
    options: SamplingOptions,
) -> Result<(), ScalarError> {
    validate_inputs(
        a_frame,
        a_value,
        a,
        b_frame,
        b_value,
        b,
        seconds_per_frame,
        options,
    )?;
    if !hold(a, b)? {
        Curve::new(
            a_frame,
            a_value,
            a,
            b_frame,
            b_value,
            b,
            seconds_per_frame,
            options,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_inputs(
    a_frame: u32,
    a_value: f64,
    a: &ScalarKeyTiming,
    b_frame: u32,
    b_value: f64,
    b: &ScalarKeyTiming,
    seconds_per_frame: f64,
    options: SamplingOptions,
) -> Result<(), ScalarError> {
    if a_frame >= b_frame {
        return Err(ScalarError::InvalidFrameRange);
    }
    if !seconds_per_frame.is_finite() || seconds_per_frame <= 0.0 {
        return Err(ScalarError::InvalidTime);
    }
    if !a_value.is_finite() || !b_value.is_finite() {
        return Err(ScalarError::NonFinite);
    }
    a.validate()?;
    b.validate()?;
    options.validate()
}

fn hold(a: &ScalarKeyTiming, b: &ScalarKeyTiming) -> Result<bool, ScalarError> {
    if a.out_interpolation == ScalarInterpolation::Hold {
        Ok(true)
    } else if b.in_interpolation == ScalarInterpolation::Hold {
        Err(ScalarError::IncomingHold)
    } else {
        Ok(false)
    }
}

struct Curve {
    controls: [f64; 4],
    scale: f64,
    tolerance: f64,
    derivative_bound: f64,
    outgoing_fraction: f64,
    incoming_fraction: f64,
    constant: bool,
}

impl Curve {
    #[allow(clippy::too_many_arguments)]
    fn new(
        a_frame: u32,
        a_value: f64,
        a: &ScalarKeyTiming,
        b_frame: u32,
        b_value: f64,
        b: &ScalarKeyTiming,
        seconds_per_frame: f64,
        options: SamplingOptions,
    ) -> Result<Self, ScalarError> {
        let duration = f64::from(b_frame - a_frame) * seconds_per_frame;
        if !duration.is_finite() || duration <= 0.0 {
            return Err(ScalarError::NumericRange);
        }
        let outgoing_bezier = a.out_interpolation == ScalarInterpolation::Bezier;
        let incoming_bezier = b.in_interpolation == ScalarInterpolation::Bezier;
        let outgoing_delta = if outgoing_bezier {
            displacement(a.out_ease.speed, a.out_ease.influence, duration)?
        } else {
            0.0
        };
        let incoming_delta = if incoming_bezier {
            displacement(b.in_ease.speed, b.in_ease.influence, duration)?
        } else {
            0.0
        };
        // A constant is identified from the authored active speeds, never from
        // rounded controls or an epsilon comparison that could erase motion.
        let constant = a_value == b_value
            && (!outgoing_bezier || a.out_ease.speed == 0.0)
            && (!incoming_bezier || b.in_ease.speed == 0.0);
        let scale = [a_value, b_value, outgoing_delta, incoming_delta]
            .iter()
            .fold(0.0_f64, |m, value| m.max(value.abs()));
        let normalize = |value: f64| if scale == 0.0 { value } else { value / scale };
        let first = normalize(a_value);
        let last = normalize(b_value);
        // Normalize before constructing controls so subnormal values do not
        // lose a third of a secant to intermediate underflow. No endpoint
        // difference is used as a divisor.
        let outgoing = if outgoing_bezier {
            first + normalize(outgoing_delta)
        } else {
            first * (2.0 / 3.0) + last * (1.0 / 3.0)
        };
        let incoming = if incoming_bezier {
            last - normalize(incoming_delta)
        } else {
            first * (1.0 / 3.0) + last * (2.0 / 3.0)
        };
        let controls = [first, outgoing, incoming, last];
        if controls.iter().any(|value| !(value * scale).is_finite()) {
            return Err(ScalarError::NumericRange);
        }
        let control_magnitude = controls.iter().fold(0.0_f64, |m, value| m.max(value.abs()));
        let tolerance = if scale == 0.0 {
            options.relative_tolerance
        } else {
            (options.relative_tolerance * control_magnitude).min(options.absolute_tolerance / scale)
        };
        if !constant && tolerance < ROUNDOFF_FLOOR {
            return Err(ScalarError::PrecisionNotEstablished);
        }
        let outgoing_fraction = if outgoing_bezier {
            a.out_ease.influence / 100.0
        } else {
            1.0 / 3.0
        };
        let incoming_fraction = if incoming_bezier {
            b.in_ease.influence / 100.0
        } else {
            1.0 / 3.0
        };
        let derivative_bound = 3.0
            * controls
                .windows(2)
                .map(|v| (v[1] - v[0]).abs())
                .fold(0.0, f64::max);
        Ok(Self {
            controls,
            scale,
            tolerance,
            derivative_bound,
            outgoing_fraction,
            incoming_fraction,
            constant,
        })
    }

    fn invert_time(&self, elapsed: Double, iterations: u32) -> Result<f64, ScalarError> {
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..iterations {
            let middle = low + (high - low) * 0.5;
            if middle == low || middle == high {
                return Err(ScalarError::PrecisionNotEstablished);
            }
            let residual = centered_time_difference(
                self.outgoing_fraction,
                self.incoming_fraction,
                middle,
                elapsed,
            );
            if residual < 0.0 {
                low = middle;
            } else if residual > 0.0 {
                high = middle;
            } else {
                // Centered compensated evaluation retains its small terms at
                // the stationary point. The floating-point allowance below
                // already reserves more than the parameter roundoff here.
                return Ok(middle);
            }
            // A derivative bound on the VALUE cubic certifies error even if
            // it reverses direction, has equal endpoints or overshoots.
            if (high - low) * self.derivative_bound <= self.tolerance * 0.25 {
                return Ok(low + (high - low) * 0.5);
            }
        }
        Err(ScalarError::WorkBudgetExceeded)
    }

    fn value(&self, u: f64) -> Result<f64, ScalarError> {
        let mut values = self.controls;
        for width in (1..=3).rev() {
            for i in 0..width {
                values[i] = values[i] * (1.0 - u) + values[i + 1] * u;
            }
        }
        let normalized = values[0];
        let value = normalized * self.scale;
        if !value.is_finite() {
            return Err(ScalarError::NumericRange);
        }
        if normalized != 0.0 && value == 0.0 {
            return Err(ScalarError::PrecisionNotEstablished);
        }
        // Subnormal output has a fixed quantization step. Compare its return
        // to normalized units rather than letting an underflowed error bound
        // accidentally certify zero. Exact representable excursions still pass.
        if value != 0.0 && value.abs() < f64::MIN_POSITIVE {
            let quantization = (value / self.scale - normalized).abs();
            if quantization + ROUNDOFF_FLOOR > self.tolerance * 0.5 {
                return Err(ScalarError::PrecisionNotEstablished);
            }
        }
        Ok(value)
    }
}

fn displacement(speed: f64, influence: f64, duration: f64) -> Result<f64, ScalarError> {
    // Group time first. Dividing a subnormal speed by FPS would erase authored
    // motion which can be representable over a long adjacent-key duration.
    let time = duration * (influence / 100.0);
    let displacement = speed * time;
    if !displacement.is_finite() {
        return Err(ScalarError::NumericRange);
    }
    if speed != 0.0 && displacement == 0.0 {
        return Err(ScalarError::PrecisionNotEstablished);
    }
    if speed != 0.0 {
        // Subnormal multiplication can have large relative rounding error.
        // Recover its ratio in normal units, reserving a small part of the
        // normalized roundoff allowance instead of accepting a rounded handle
        // whose error is larger than the requested value precision.
        let fraction = influence / 100.0;
        if time < f64::MIN_POSITIVE
            && (time / duration - fraction).abs() > fraction * (16.0 * f64::EPSILON)
        {
            return Err(ScalarError::PrecisionNotEstablished);
        }
        if displacement.abs() < f64::MIN_POSITIVE
            && (displacement / speed - time).abs() > time * (16.0 * f64::EPSILON)
        {
            return Err(ScalarError::PrecisionNotEstablished);
        }
    }
    Ok(displacement)
}

#[derive(Clone, Copy)]
struct Double(f64, f64);
impl Double {
    fn add(self, rhs: Self) -> Self {
        let sum = self.0 + rhs.0;
        let rhs_part = sum - self.0;
        let error = (self.0 - (sum - rhs_part)) + (rhs.0 - rhs_part) + self.1 + rhs.1;
        let high = sum + error;
        Self(high, error - (high - sum))
    }
    fn mul(self, rhs: Self) -> Self {
        let product = self.0 * rhs.0;
        let error =
            self.0.mul_add(rhs.0, -product) + self.0 * rhs.1 + self.1 * rhs.0 + self.1 * rhs.1;
        let high = product + error;
        Self(high, error - (high - product))
    }
    fn div(self, rhs: f64) -> Self {
        let quotient = self.0 / rhs;
        let residual = self.add(Double(-quotient, 0.0).mul(Double(rhs, 0.0)));
        Double(quotient, 0.0).add(Double((residual.0 + residual.1) / rhs, 0.0))
    }
}

fn centered_time_difference(p: f64, q: f64, u: f64, target: Double) -> f64 {
    // x(u)-target, centered at v=u-1/2. For p=q=1 this is
    // 4*v^3 + (1/2-target). No subtraction of two ~1/2 cubic values
    // can manufacture a false equality plateau near the stationary derivative.
    let p = Double(p, 0.0);
    let q = Double(q, 0.0);
    let v = Double(u, 0.0).add(Double(-0.5, 0.0));
    let cubic = p.add(q).mul(Double(3.0, 0.0)).add(Double(-2.0, 0.0));
    let quadratic = q.add(Double(-p.0, 0.0)).mul(Double(1.5, 0.0));
    let linear = Double(1.0, 0.0)
        .add(Double(-p.0, 0.0))
        .add(Double(1.0, 0.0).add(Double(-q.0, 0.0)))
        .mul(Double(0.75, 0.0));
    let constant = p
        .add(Double(-q.0, 0.0))
        .mul(Double(0.375, 0.0))
        .add(Double(0.5, 0.0).add(Double(-target.0, -target.1)));
    let result = cubic
        .mul(v)
        .add(quadratic)
        .mul(v)
        .add(linear)
        .mul(v)
        .add(constant);
    result.0 + result.1
}
