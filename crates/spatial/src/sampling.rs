use crate::{SpatialError, SpatialInterpolation, SpatialKey3};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BinaryHeap};

/// Hard ceilings apply even when callers supply their own options.
pub const MAX_SUBDIVISIONS: usize = 262_144;
pub const MAX_SUBDIVISION_DEPTH: u32 = 48;
pub const MAX_INVERSION_ITERATIONS: u32 = 80;

/// Error is measured in spatial distance units, not cubic parameter units.
/// The requested error is max(absolute_tolerance, relative_tolerance * the
/// initial control-polygon length). Defaults retain relative precision for
/// arbitrarily small finite paths, instead of treating them as constant.
///
/// Arc length is enclosed between subdivided chord and control-polygon sums.
/// Each enclosure has a floating-point roundoff allowance. A deterministic
/// priority queue refines the leaf with the largest enclosure width, using
/// one global segment error budget. This avoids forcing sub-ulp local budgets
/// onto nearly straight portions of ordinary curves. Subdivision stops
/// when the total enclosure width is <= requested error / 16. Inversion uses
/// these same enclosures; its returned point differs from the requested arc
/// distance by at most requested error, subject to the floating-point guard.
/// Temporal inversion uses compensated arithmetic and bisection, including
/// zero-derivative times. A conservative 1e-9 * polygon-length numeric floor
/// applies to Bezier temporal inversion. Requests below an applicable numeric
/// floor return an error; they never silently relax the requested tolerance.
///
/// Work is bounded per sampled/validated segment by max_subdivisions de
/// Casteljau splits, max_depth recursive levels, and max_inversion_iterations
/// temporal/arc inversion steps each. All exhausted ceilings return errors.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamplingOptions {
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
    pub max_subdivisions: usize,
    pub max_depth: u32,
    pub max_inversion_iterations: u32,
}

impl Default for SamplingOptions {
    fn default() -> Self {
        Self {
            absolute_tolerance: 0.0,
            relative_tolerance: 1e-7,
            max_subdivisions: 65_536,
            max_depth: 40,
            max_inversion_iterations: 64,
        }
    }
}

impl SamplingOptions {
    pub fn validate(self) -> Result<(), SpatialError> {
        if !self.absolute_tolerance.is_finite()
            || self.absolute_tolerance < 0.0
            || !self.relative_tolerance.is_finite()
            || self.relative_tolerance < 0.0
            || (self.absolute_tolerance == 0.0 && self.relative_tolerance == 0.0)
            || self.max_subdivisions == 0
            || self.max_subdivisions > MAX_SUBDIVISIONS
            || self.max_depth == 0
            || self.max_depth > MAX_SUBDIVISION_DEPTH
            || self.max_inversion_iterations == 0
            || self.max_inversion_iterations > MAX_INVERSION_ITERATIONS
        {
            return Err(SpatialError::InvalidSamplingOptions);
        }
        Ok(())
    }
}

pub(crate) fn validate_time(frame: f64, seconds_per_frame: f64) -> Result<(), SpatialError> {
    if !frame.is_finite() || !seconds_per_frame.is_finite() || seconds_per_frame <= 0.0 {
        Err(SpatialError::InvalidTime)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Curve([[f64; 3]; 4]);

impl Curve {
    fn split(self) -> (Self, Self) {
        let [a, b, c, d] = self.0;
        let ab = midpoint(a, b);
        let bc = midpoint(b, c);
        let cd = midpoint(c, d);
        let abc = midpoint(ab, bc);
        let bcd = midpoint(bc, cd);
        let middle = midpoint(abc, bcd);
        (Self([a, ab, abc, middle]), Self([middle, bcd, cd, d]))
    }

    fn bounds(self) -> Result<Bounds, SpatialError> {
        if let Some(length) = self.exact_axis_line_length() {
            return Ok(Bounds {
                low: length,
                high: length,
            });
        }
        let chord = distance(self.0[0], self.0[3]);
        let polygon = self.polygon_length();
        if !chord.is_finite() || !polygon.is_finite() {
            return Err(SpatialError::NumericRange);
        }
        if polygon == 0.0 {
            return Ok(Bounds {
                low: 0.0,
                high: 0.0,
            });
        }
        let coordinate_scale = self.0.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
        let roundoff = coordinate_scale * (64.0 * f64::EPSILON);
        let high = polygon + roundoff;
        if !high.is_finite() {
            return Err(SpatialError::NumericRange);
        }
        Ok(Bounds {
            low: (chord - roundoff).max(0.0),
            high,
        })
    }

    fn polygon_length(self) -> f64 {
        distance(self.0[0], self.0[1])
            + distance(self.0[1], self.0[2])
            + distance(self.0[2], self.0[3])
    }

    fn exact_axis_line_length(self) -> Option<f64> {
        let mut varying = None;
        for axis in 0..3 {
            if self.0.iter().any(|p| p[axis] != self.0[0][axis]) {
                if varying.is_some() {
                    return None;
                }
                varying = Some(axis);
            }
        }
        let Some(axis) = varying else {
            return Some(0.0);
        };
        let increasing = self.0.windows(2).all(|pair| pair[0][axis] <= pair[1][axis]);
        let decreasing = self.0.windows(2).all(|pair| pair[0][axis] >= pair[1][axis]);
        if !increasing && !decreasing {
            return None;
        }
        let difference = Double(self.0[3][axis], 0.0).add(Double(-self.0[0][axis], 0.0));
        (difference.1 == 0.0 && difference.0.is_finite()).then_some(difference.0.abs())
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    low: f64,
    high: f64,
}

impl Bounds {
    fn plus(self, other: Self) -> Result<Self, SpatialError> {
        let low = self.low + other.low;
        let high = self.high + other.high;
        if !high.is_finite() {
            return Err(SpatialError::NumericRange);
        }
        // Summation roundoff is included in the accumulated enclosure.
        Ok(Self {
            low: low * (1.0 - 2.0 * f64::EPSILON),
            high: high * (1.0 + 2.0 * f64::EPSILON),
        })
    }
}

struct ArcNode {
    curve: Curve,
    bounds: Bounds,
    children: Option<Box<[ArcNode; 2]>>,
}

struct Work {
    options: SamplingOptions,
    splits: usize,
}

impl Work {
    fn split(&mut self, depth: u32) -> Result<(), SpatialError> {
        if self.splits >= self.options.max_subdivisions || depth >= self.options.max_depth {
            return Err(SpatialError::WorkBudgetExceeded);
        }
        self.splits += 1;
        Ok(())
    }
}

struct PendingArcNode {
    curve: Curve,
    bounds: Bounds,
    depth: u32,
    children: Option<[usize; 2]>,
}

#[derive(Clone, Copy)]
struct WidestLeaf {
    width: f64,
    index: usize,
}
impl PartialEq for WidestLeaf {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.index == other.index
    }
}
impl Eq for WidestLeaf {}
impl PartialOrd for WidestLeaf {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for WidestLeaf {
    fn cmp(&self, other: &Self) -> Ordering {
        self.width
            .total_cmp(&other.width)
            .then_with(|| other.index.cmp(&self.index))
    }
}

impl ArcNode {
    fn build(
        curve: Curve,
        tolerance: f64,
        depth: u32,
        work: &mut Work,
    ) -> Result<Self, SpatialError> {
        let bounds = curve.bounds()?;
        if bounds.high - bounds.low <= tolerance {
            return Ok(Self {
                curve,
                bounds,
                children: None,
            });
        }
        if tolerance == 0.0 {
            return Err(SpatialError::PrecisionNotEstablished);
        }
        let mut nodes = vec![PendingArcNode {
            curve,
            bounds,
            depth,
            children: None,
        }];
        let mut leaves = BinaryHeap::from([WidestLeaf {
            width: bounds.high - bounds.low,
            index: 0,
        }]);
        let mut width = Double(bounds.high - bounds.low, 0.0);
        // The remaining five percent covers enclosure summation rounding;
        // the reconstructed root is checked against the original tolerance.
        while width.0 + width.1 > tolerance * 0.95 {
            let Some(leaf) = leaves.pop() else {
                return Err(SpatialError::PrecisionNotEstablished);
            };
            let node = &nodes[leaf.index];
            work.split(node.depth)?;
            let (left, right) = node.curve.split();
            let left_bounds = left.bounds()?;
            let right_bounds = right.bounds()?;
            let left_width = left_bounds.high - left_bounds.low;
            let right_width = right_bounds.high - right_bounds.low;
            // Subdividing a roundoff-limited leaf can widen its enclosure.
            // Keep that certified leaf and spend work on other uncertainty.
            if left_width + right_width >= leaf.width
                || left.0 == node.curve.0
                || right.0 == node.curve.0
            {
                continue;
            }
            let child_depth = node.depth + 1;
            let first_child = nodes.len();
            nodes[leaf.index].children = Some([first_child, first_child + 1]);
            nodes.push(PendingArcNode {
                curve: left,
                bounds: left_bounds,
                depth: child_depth,
                children: None,
            });
            nodes.push(PendingArcNode {
                curve: right,
                bounds: right_bounds,
                depth: child_depth,
                children: None,
            });
            leaves.push(WidestLeaf {
                width: left_width,
                index: first_child,
            });
            leaves.push(WidestLeaf {
                width: right_width,
                index: first_child + 1,
            });
            width = width
                .add(Double(-leaf.width, 0.0))
                .add(Double(left_width, 0.0))
                .add(Double(right_width, 0.0));
        }
        let result = Self::from_pending(&nodes, 0)?;
        if result.bounds.high - result.bounds.low > tolerance {
            return Err(SpatialError::PrecisionNotEstablished);
        }
        Ok(result)
    }

    fn from_pending(nodes: &[PendingArcNode], index: usize) -> Result<Self, SpatialError> {
        let node = &nodes[index];
        if let Some([left, right]) = node.children {
            let left = Self::from_pending(nodes, left)?;
            let right = Self::from_pending(nodes, right)?;
            Ok(Self {
                curve: node.curve,
                bounds: left.bounds.plus(right.bounds)?,
                children: Some(Box::new([left, right])),
            })
        } else {
            Ok(Self {
                curve: node.curve,
                bounds: node.bounds,
                children: None,
            })
        }
    }

    fn invert(
        &mut self,
        target: Bounds,
        tolerance: f64,
        depth: u32,
        iteration: u32,
        work: &mut Work,
    ) -> Result<[f64; 3], SpatialError> {
        if iteration >= work.options.max_inversion_iterations {
            return Err(SpatialError::WorkBudgetExceeded);
        }
        if self.bounds.high <= tolerance {
            return Ok(self.curve.split().0.0[3]);
        }
        if self.children.is_none() {
            work.split(depth)?;
            let (left, right) = self.curve.split();
            // Every new leaf is a subdivision of an already certified leaf.
            // Tighter local bounds improve, rather than reset, the global bound.
            let left = Self::build(left, tolerance / 64.0, depth + 1, work)?;
            let right = Self::build(right, tolerance / 64.0, depth + 1, work)?;
            self.children = Some(Box::new([left, right]));
        }
        let [left, right] = self.children.as_mut().unwrap().as_mut();
        let split = left.bounds;
        if target.high < split.low {
            left.invert(target, tolerance, depth + 1, iteration + 1, work)
        } else if target.low > split.high {
            right.invert(
                Bounds {
                    low: target.low - split.high,
                    high: target.high - split.low,
                },
                tolerance,
                depth + 1,
                iteration + 1,
                work,
            )
        } else {
            // Target and split enclosures overlap. Any true target differs
            // from this point by no more than this arc-distance interval.
            let error = (target.high - split.low).max(split.high - target.low);
            if error <= tolerance {
                Ok(left.curve.0[3])
            } else {
                Err(SpatialError::PrecisionNotEstablished)
            }
        }
    }
}

struct Segment {
    origin: [f64; 3],
    arc: ArcNode,
    duration: f64,
    tolerance: f64,
    work: Work,
}

fn hold(a: &SpatialKey3, b: &SpatialKey3) -> Result<bool, SpatialError> {
    if a.out_interpolation == SpatialInterpolation::Hold {
        return Ok(true);
    }
    if b.in_interpolation == SpatialInterpolation::Hold {
        return Err(SpatialError::IncomingHold);
    }
    Ok(false)
}

fn segment(
    a_frame: u32,
    a: &SpatialKey3,
    b_frame: u32,
    b: &SpatialKey3,
    seconds_per_frame: f64,
    options: SamplingOptions,
) -> Result<Segment, SpatialError> {
    let duration = f64::from(b_frame - a_frame) * seconds_per_frame;
    if !duration.is_finite() || duration <= 0.0 {
        return Err(SpatialError::NumericRange);
    }
    let end = std::array::from_fn(|i| b.value[i] - a.value[i]);
    let incoming = std::array::from_fn(|i| end[i] + b.in_tangent[i]);
    let curve = Curve([[0.0; 3], a.out_tangent, incoming, end]);
    if !curve.0.iter().flatten().all(|v| v.is_finite()) {
        return Err(SpatialError::NumericRange);
    }
    let length = curve.polygon_length();
    if !length.is_finite() {
        return Err(SpatialError::NumericRange);
    }
    let tolerance = options
        .absolute_tolerance
        .max(options.relative_tolerance * length);
    if !tolerance.is_finite() {
        return Err(SpatialError::NumericRange);
    }
    if length != 0.0 && tolerance == 0.0 {
        return Err(SpatialError::PrecisionNotEstablished);
    }
    let mut work = Work { options, splits: 0 };
    let arc = ArcNode::build(curve, tolerance / 16.0, 0, &mut work)?;
    if length == 0.0 {
        if (a.out_interpolation == SpatialInterpolation::Bezier && a.out_ease.speed != 0.0)
            || (b.in_interpolation == SpatialInterpolation::Bezier && b.in_ease.speed != 0.0)
        {
            return Err(SpatialError::NonzeroSpeedOnConstantPath);
        }
    } else {
        let low = distance_controls(a, b, duration, arc.bounds.low)?;
        let high = distance_controls(a, b, duration, arc.bounds.high)?;
        match (monotone(low), monotone(high)) {
            (false, false) => return Err(SpatialError::TemporalDistanceReversal),
            (false, true) | (true, false) => return Err(SpatialError::PrecisionNotEstablished),
            (true, true) => {}
        }
    }
    Ok(Segment {
        origin: a.value,
        arc,
        duration,
        tolerance,
        work,
    })
}

pub(crate) fn validate_segment(
    a_frame: u32,
    a: &SpatialKey3,
    b_frame: u32,
    b: &SpatialKey3,
    seconds_per_frame: f64,
    options: SamplingOptions,
) -> Result<(), SpatialError> {
    if !hold(a, b)? {
        segment(a_frame, a, b_frame, b, seconds_per_frame, options)?;
    }
    Ok(())
}

pub(crate) fn sample_segment(
    a_frame: u32,
    a: &SpatialKey3,
    b_frame: u32,
    b: &SpatialKey3,
    frame: f64,
    seconds_per_frame: f64,
    options: SamplingOptions,
) -> Result<[f64; 3], SpatialError> {
    if hold(a, b)? {
        return Ok(a.value);
    }
    let mut segment = segment(a_frame, a, b_frame, b, seconds_per_frame, options)?;
    if segment.arc.bounds.high == 0.0 {
        return Ok(a.value);
    }
    let elapsed = (frame - f64::from(a_frame)) / f64::from(b_frame - a_frame);
    let target = temporal_target(a, b, elapsed, &segment)?;
    let local = segment
        .arc
        .invert(target, segment.tolerance * 0.5, 0, 0, &mut segment.work)?;
    let result = std::array::from_fn(|i| segment.origin[i] + local[i]);
    if !result.iter().all(|v| v.is_finite()) {
        return Err(SpatialError::NumericRange);
    }
    let residual: [f64; 3] = std::array::from_fn(|i| {
        let local_part = result[i] - segment.origin[i];
        (segment.origin[i] - (result[i] - local_part)) + (local[i] - local_part)
    });
    if distance(residual, [0.0; 3]) > segment.tolerance * 0.25 {
        return Err(SpatialError::PrecisionNotEstablished);
    }
    Ok(result)
}

fn active_distance(speed: f64, influence: f64, duration: f64) -> Result<f64, SpatialError> {
    let time = duration * (influence / 100.0);
    let distance = speed * time;
    if !distance.is_finite() {
        return Err(SpatialError::NumericRange);
    }
    if speed != 0.0 && distance == 0.0 {
        return Err(SpatialError::PrecisionNotEstablished);
    }
    Ok(distance)
}

fn distance_controls(
    a: &SpatialKey3,
    b: &SpatialKey3,
    duration: f64,
    length: f64,
) -> Result<[f64; 4], SpatialError> {
    let outgoing = if a.out_interpolation == SpatialInterpolation::Bezier {
        active_distance(a.out_ease.speed, a.out_ease.influence, duration)?
    } else {
        length / 3.0
    };
    let incoming = if b.in_interpolation == SpatialInterpolation::Bezier {
        length - active_distance(b.in_ease.speed, b.in_ease.influence, duration)?
    } else {
        length * (2.0 / 3.0)
    };
    let controls = [0.0, outgoing, incoming, length];
    if controls.iter().all(|v| v.is_finite()) {
        Ok(controls)
    } else {
        Err(SpatialError::NumericRange)
    }
}

fn monotone(controls: [f64; 4]) -> bool {
    let scale = controls.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    if scale == 0.0 {
        return true;
    }
    let [a, b, c, d] = controls.map(|v| v / scale);
    let (first, middle, last) = (b - a, c - b, d - c);
    // The derivative is a quadratic Bernstein polynomial. Nonnegative end
    // coefficients and middle >= -sqrt(first*last) are necessary/sufficient.
    first >= 0.0 && last >= 0.0 && (middle >= 0.0 || -middle <= first.sqrt() * last.sqrt())
}

fn temporal_target(
    a: &SpatialKey3,
    b: &SpatialKey3,
    elapsed: f64,
    segment: &Segment,
) -> Result<Bounds, SpatialError> {
    let length = segment.arc.bounds;
    if a.out_interpolation == SpatialInterpolation::Linear
        && b.in_interpolation == SpatialInterpolation::Linear
    {
        return Ok(Bounds {
            low: length.low * elapsed,
            high: length.high * elapsed,
        });
    }
    if segment.tolerance < segment.arc.curve.polygon_length() * 1e-9 {
        return Err(SpatialError::PrecisionNotEstablished);
    }
    let x1 = if a.out_interpolation == SpatialInterpolation::Bezier {
        a.out_ease.influence / 100.0
    } else {
        1.0 / 3.0
    };
    let x2 = if b.in_interpolation == SpatialInterpolation::Bezier {
        1.0 - b.in_ease.influence / 100.0
    } else {
        2.0 / 3.0
    };
    let low_controls = distance_controls(a, b, segment.duration, length.low)?;
    let high_controls = distance_controls(a, b, segment.duration, length.high)?;
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..segment.work.options.max_inversion_iterations {
        let middle = (low + high) * 0.5;
        let residual = compensated_cubic_difference([0.0, x1, x2, 1.0], middle, elapsed);
        if residual < 0.0 {
            low = middle;
        } else if residual > 0.0 {
            high = middle;
        } else {
            low = middle;
            high = middle;
        }
        let result = Bounds {
            low: cubic(low_controls, low),
            high: cubic(high_controls, high),
        };
        if result.high - result.low <= segment.tolerance / 4.0 {
            return Ok(result);
        }
    }
    Err(SpatialError::WorkBudgetExceeded)
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    // Half first to avoid overflow in the sum of large, finite coordinates.
    std::array::from_fn(|i| a[i] * 0.5 + b[i] * 0.5)
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}
fn cubic([a, b, c, d]: [f64; 4], u: f64) -> f64 {
    let lerp = |a: f64, b: f64| a * (1.0 - u) + b * u;
    lerp(lerp(lerp(a, b), lerp(b, c)), lerp(lerp(b, c), lerp(c, d)))
}

// Two-component arithmetic keeps x(u)-time meaningful near a zero time
// derivative. Ordinary floating-point cubic evaluation can create a broad
// false equality plateau at x=1/2 when the influences are both 100 percent.
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
}
fn compensated_cubic_difference(controls: [f64; 4], u: f64, target: f64) -> f64 {
    let u = Double(u, 0.0);
    let v = Double(1.0, 0.0).add(Double(-u.0, 0.0));
    let mut values = controls.map(|x| Double(x, 0.0).add(Double(-target, 0.0)));
    for width in (1..=3).rev() {
        for i in 0..width {
            values[i] = values[i].mul(v).add(values[i + 1].mul(u));
        }
    }
    values[0].0 + values[0].1
}

#[cfg(test)]
pub(crate) fn test_arc_length(
    a: &SpatialKey3,
    b: &SpatialKey3,
) -> Result<(f64, f64), SpatialError> {
    let segment = segment(0, a, 1, b, 1.0, SamplingOptions::default())?;
    Ok((segment.arc.bounds.low, segment.arc.bounds.high))
}

#[cfg(test)]
pub(crate) fn test_arc_work(
    a: &SpatialKey3,
    b: &SpatialKey3,
) -> Result<(usize, u32), SpatialError> {
    fn depth(node: &ArcNode) -> u32 {
        node.children.as_ref().map_or(0, |children| {
            1 + depth(&children[0]).max(depth(&children[1]))
        })
    }
    let segment = segment(0, a, 1, b, 1.0, SamplingOptions::default())?;
    Ok((segment.work.splits, depth(&segment.arc)))
}
