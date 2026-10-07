//! Render-only Trim Paths geometry and deterministic admission budgets.
//!
//! Every input binary64 control is treated as an exact reference number. All
//! measurement arithmetic is outward rounded, including de Casteljau coordinates,
//! Euclidean distances, sums, and fractional length targets. A leaf's chord and
//! control-polygon bounds are intersected with rigorous Simpson/Boole bounds.
//! For speed f on the leaf's normalized [0,1], Simpson's error is at most
//! sup|f''''|/2880. Interval Taylor coefficients of sqrt(vx²+vy²), obtained from
//! r*r = vx²+vy², bound f''''/24, hence the remainder is sup|r[4]|/120.
//! Boole's five-point rule has error sup|f^(6)|/1935360 = sup|r[6]|/2688.
//! The classical remainder and positive weights are given in Krukowski (2018),
//! section3, https://arxiv.org/pdf/1808.02803 ; Simpson: DLMF 3.5.E6.
//! Interval jets enclose derivatives at EVERY point, not sampled estimates.
//! If speed cannot be bounded away from zero, only geometric bounds are used.
//! This extra enclosure avoids the quadratic convergence cost of polygon bounds
//! on large ordinary contours; it never replaces a bound with an estimate.
//!
//! A balanced sum tree and stable uncertainty heap are shared by both inversions.
//! Candidate residual (including total-length uncertainty) is at most half the
//! cut budget; the other half is reserved for final de Casteljau point rounding.
//! The cut budget is min(2^-10, lower_length * min(span,1-span)/8). Thus two cut
//! errors cannot consume more than a quarter of the smaller retained/removed
//! feature. Exact identities bypass measurement and preserve source formatting.
//! Boundary IDs express topology, not proximity: removed gaps never reconnect.

use crate::{Affine, Frame, VectorPath};
use std::{cmp::Ordering, collections::BinaryHeap, fmt};

pub const TRIM_SOURCE_NODE_LIMIT: usize = 1 << 16;
pub const TRIM_LAYER_WORK_LIMIT: usize = 1 << 20;
pub const TRIM_FRAME_WORK_LIMIT: usize = 1 << 22;
pub const CONTENTS_OUTPUT_BYTE_LIMIT: usize = 64 << 20;
const MAX_PIECES: usize = 1536;
const MAX_RUNS: usize = 513;
const MAX_DEPTH: u8 = 52;
const ABS_CUT_BUDGET: f64 = 1.0 / 1024.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentsRenderErrorKind {
    Precision,
    WorkLimit,
    OutputLimit,
    NonFinite,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentsRenderError {
    pub kind: ContentsRenderErrorKind,
    pub frame: Frame,
    pub operator_id: Option<u64>,
    pub source_id: Option<u64>,
    pub message: &'static str,
}
impl fmt::Display for ContentsRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Contents {:?} at frame {}", self.kind, self.frame)?;
        if let Some(id) = self.operator_id {
            write!(f, ", Trim {id}")?;
        }
        if let Some(id) = self.source_id {
            write!(f, ", source {id}")?;
        }
        write!(f, ": {}", self.message)
    }
}
impl std::error::Error for ContentsRenderError {}

/// Shared by all Contents evaluations in one rendered frame. Limits may be
/// lowered by callers/tests; raising them does not raise the documented hard caps.
/// `begin_layer` resets ONLY layer_work. Output counts include actual copies.
#[derive(Clone, Debug)]
pub struct ContentsRenderBudget {
    pub source_node_limit: usize,
    pub layer_work_limit: usize,
    pub frame_work_limit: usize,
    pub output_byte_limit: usize,
    pub layer_work: usize,
    pub frame_work: usize,
    pub output_bytes: usize,
    pub peak_source_nodes: usize,
    pub subdivision_count: usize,
    pub inversion_count: usize,
}
impl Default for ContentsRenderBudget {
    fn default() -> Self {
        Self {
            source_node_limit: TRIM_SOURCE_NODE_LIMIT,
            layer_work_limit: TRIM_LAYER_WORK_LIMIT,
            frame_work_limit: TRIM_FRAME_WORK_LIMIT,
            output_byte_limit: CONTENTS_OUTPUT_BYTE_LIMIT,
            layer_work: 0,
            frame_work: 0,
            output_bytes: 0,
            peak_source_nodes: 0,
            subdivision_count: 0,
            inversion_count: 0,
        }
    }
}
impl ContentsRenderBudget {
    pub fn begin_layer(&mut self) {
        self.layer_work = 0;
    }
    pub fn check_cancel(
        &self,
        frame: Frame,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<(), ContentsRenderError> {
        if cancel.is_some_and(|c| c()) {
            Err(ContentsRenderError {
                kind: ContentsRenderErrorKind::Cancelled,
                frame,
                operator_id: None,
                source_id: None,
                message: "render cancelled",
            })
        } else {
            Ok(())
        }
    }
    /// Check prospective assembled size before an append/allocation. The limit
    /// applies to each SVG string, preserving the historical final-frame ceiling.
    pub fn check_output_size(&self, bytes: usize, frame: Frame) -> Result<(), ContentsRenderError> {
        if bytes > self.output_byte_limit.min(CONTENTS_OUTPUT_BYTE_LIMIT) {
            return Err(ContentsRenderError {
                kind: ContentsRenderErrorKind::OutputLimit,
                frame,
                operator_id: None,
                source_id: None,
                message: "Contents SVG byte budget exceeded",
            });
        }
        Ok(())
    }
    /// Record actual copy volume for diagnostics; temporary nested copies do not
    /// consume the final-string ceiling. Call check_output_size before appending.
    pub fn charge_output(&mut self, bytes: usize, frame: Frame) -> Result<(), ContentsRenderError> {
        self.check_output_size(bytes, frame)?;
        self.output_bytes = self
            .output_bytes
            .checked_add(bytes)
            .ok_or(ContentsRenderError {
                kind: ContentsRenderErrorKind::OutputLimit,
                frame,
                operator_id: None,
                source_id: None,
                message: "Contents SVG accounting overflow",
            })?;
        Ok(())
    }
    fn work(
        &mut self,
        cx: Context,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<(), ContentsRenderError> {
        if self.frame_work & 127 == 0 {
            self.check_cancel(cx.frame, cancel).map_err(|mut e| {
                e.operator_id = Some(cx.operator);
                e.source_id = Some(cx.source);
                e
            })?;
        }
        if self.layer_work >= self.layer_work_limit.min(TRIM_LAYER_WORK_LIMIT)
            || self.frame_work >= self.frame_work_limit.min(TRIM_FRAME_WORK_LIMIT)
        {
            return Err(cx.error(
                ContentsRenderErrorKind::WorkLimit,
                "Trim measurement work budget exceeded",
            ));
        }
        self.layer_work += 1;
        self.frame_work += 1;
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct Context {
    frame: Frame,
    operator: u64,
    source: u64,
}
impl Context {
    fn error(self, kind: ContentsRenderErrorKind, message: &'static str) -> ContentsRenderError {
        ContentsRenderError {
            kind,
            frame: self.frame,
            operator_id: Some(self.operator),
            source_id: Some(self.source),
            message,
        }
    }
}

type Point = [f64; 2];
type Cubic = [Point; 4];
#[derive(Clone, Debug, PartialEq)]
struct Piece {
    cubic: Cubic,
    start: u64,
    end: u64,
}
#[derive(Clone, Debug, PartialEq)]
enum Geometry {
    Original(VectorPath),
    Pieces(Vec<Piece>),
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RenderContour {
    source_id: u64,
    contour_ordinal: u32,
    geometry: Geometry,
    next_boundary: u64,
}
impl RenderContour {
    pub(crate) fn new(source_id: u64, contour_ordinal: u32, path: VectorPath) -> Self {
        let next_boundary = path.vertices.len() as u64;
        Self {
            source_id,
            contour_ordinal,
            geometry: Geometry::Original(path),
            next_boundary,
        }
    }
    pub(crate) fn source_id(&self) -> u64 {
        self.source_id
    }
    pub(crate) fn is_empty(&self) -> bool {
        match &self.geometry {
            Geometry::Original(p) => p.vertices.is_empty(),
            Geometry::Pieces(p) => p.is_empty(),
        }
    }
    pub(crate) fn transformed(
        mut self,
        transform: Affine,
        frame: Frame,
    ) -> Result<Self, ContentsRenderError> {
        match &mut self.geometry {
            Geometry::Original(path) => {
                for v in &mut path.vertices {
                    // Keep the exact historical transform order for identity output.
                    v.position = transform.point(v.position);
                    v.incoming = transform.vector(v.incoming);
                    v.outgoing = transform.vector(v.outgoing);
                    if !v
                        .position
                        .iter()
                        .chain(&v.incoming)
                        .chain(&v.outgoing)
                        .all(|x| x.is_finite())
                        || (0..2).any(|i| {
                            !(v.position[i] + v.incoming[i]).is_finite()
                                || !(v.position[i] + v.outgoing[i]).is_finite()
                        })
                    {
                        return Err(self.error(
                            frame,
                            ContentsRenderErrorKind::NonFinite,
                            "nonfinite transformed path",
                        ));
                    }
                }
            }
            Geometry::Pieces(pieces) => {
                for piece in pieces {
                    for p in &mut piece.cubic {
                        *p = transform.point(*p);
                        if !p.iter().all(|x| x.is_finite()) {
                            return Err(self.error(
                                frame,
                                ContentsRenderErrorKind::NonFinite,
                                "nonfinite transformed cubic",
                            ));
                        }
                    }
                }
            }
        }
        Ok(self)
    }
    fn error(
        &self,
        frame: Frame,
        kind: ContentsRenderErrorKind,
        message: &'static str,
    ) -> ContentsRenderError {
        ContentsRenderError {
            kind,
            frame,
            operator_id: None,
            source_id: Some(self.source_id),
            message,
        }
    }
    pub(crate) fn svg_data_checked(
        &self,
        frame: Frame,
        budget: &mut ContentsRenderBudget,
    ) -> Result<String, ContentsRenderError> {
        let mut out = String::new();
        let mut append = |part: String| -> Result<(), ContentsRenderError> {
            let size = out.len().checked_add(part.len()).ok_or_else(|| {
                self.error(
                    frame,
                    ContentsRenderErrorKind::OutputLimit,
                    "SVG size overflow",
                )
            })?;
            budget.check_output_size(size, frame).map_err(|mut e| {
                e.source_id = Some(self.source_id);
                e
            })?;
            budget.charge_output(part.len(), frame).map_err(|mut e| {
                e.source_id = Some(self.source_id);
                e
            })?;
            out.push_str(&part);
            Ok(())
        };
        match &self.geometry {
            Geometry::Original(path) => {
                if let Some(first) = path.vertices.first() {
                    append(format!("M{} {}", first.position[0], first.position[1]))?;
                    for i in 1..path.vertices.len() + usize::from(path.closed) {
                        let a = &path.vertices[i - 1];
                        let b = &path.vertices[i % path.vertices.len()];
                        append(format!(
                            " C{} {} {} {} {} {}",
                            a.position[0] + a.outgoing[0],
                            a.position[1] + a.outgoing[1],
                            b.position[0] + b.incoming[0],
                            b.position[1] + b.incoming[1],
                            b.position[0],
                            b.position[1]
                        ))?;
                    }
                    if path.closed {
                        append(" Z".into())?;
                    }
                }
            }
            Geometry::Pieces(pieces) => {
                let mut previous = None;
                for piece in pieces {
                    if previous != Some(piece.start) {
                        let p = piece.cubic[0];
                        append(format!(
                            "{}M{} {}",
                            if previous.is_some() { " " } else { "" },
                            p[0],
                            p[1]
                        ))?;
                    }
                    let [_, a, b, c] = piece.cubic;
                    append(format!(
                        " C{} {} {} {} {} {}",
                        a[0], a[1], b[0], b[1], c[0], c[1]
                    ))?;
                    previous = Some(piece.end);
                }
            }
        }
        Ok(out)
    }
    #[cfg(test)]
    fn svg_data(&self) -> String {
        self.svg_data_checked(0, &mut ContentsRenderBudget::default())
            .unwrap()
    }

    pub(crate) fn trim(
        &mut self,
        start: f64,
        end: f64,
        offset: f64,
        operator_id: u64,
        frame: Frame,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<(), ContentsRenderError> {
        let cx = Context {
            frame,
            operator: operator_id,
            source: self.source_id,
        };
        budget.check_cancel(frame, cancel).map_err(|mut e| {
            e.operator_id = Some(operator_id);
            e.source_id = Some(self.source_id);
            e
        })?;
        if !start.is_finite()
            || !end.is_finite()
            || !offset.is_finite()
            || offset.abs() > 1_000_000.0
            || !(0.0..=100.0).contains(&start)
            || !(0.0..=100.0).contains(&end)
        {
            return Err(cx.error(
                ContentsRenderErrorKind::NonFinite,
                "invalid Trim parameters",
            ));
        }
        // These exact branches must precede conversion, modulo, and measurement.
        if (start == 0.0 && end == 100.0) || (start == 100.0 && end == 0.0) {
            return Ok(());
        }
        if start == end {
            self.geometry = Geometry::Pieces(Vec::new());
            return Ok(());
        }
        if self.is_empty() {
            return Ok(());
        }
        let pieces = self.pieces(cx, budget, cancel)?;
        if pieces.is_empty() {
            self.geometry = Geometry::Pieces(Vec::new());
            return Ok(());
        }
        let span = I::point(end.max(start))
            .sub(I::point(end.min(start)))
            .div_positive(I::point(100.0));
        let smaller = span.min(I::point(1.0).sub(span));
        if smaller.lo <= 0.0 {
            return Err(cx.error(
                ContentsRenderErrorKind::Precision,
                "requested feature is below numerical resolution",
            ));
        }
        // A shared exact numerator avoids losing common arithmetic such as
        // 10/100 + 324/360 == 1. For bounded binary64 offset and integer360,
        // the truncating remainder is representable exactly (it is a dyadic
        // remainder with no more significant bits than the original input).
        let mut begin_numerator = I::point(start.min(end))
            .scale(18.0)
            .add(I::point(offset % 360.0).scale(5.0));
        if begin_numerator.hi < 0.0 {
            begin_numerator = begin_numerator.add(I::point(1800.0));
        } else if begin_numerator.lo >= 1800.0 {
            begin_numerator = begin_numerator.sub(I::point(1800.0));
        }
        if begin_numerator.lo < 0.0 || begin_numerator.hi >= 1800.0 {
            return Err(cx.error(
                ContentsRenderErrorKind::Precision,
                "wrapped start is below numerical resolution",
            ));
        }
        let begin = begin_numerator.div_positive(I::point(1800.0));
        let mut finish_numerator = begin_numerator.add(
            I::point(end.max(start))
                .sub(I::point(end.min(start)))
                .scale(18.0),
        );
        let wraps = finish_numerator.lo > 1800.0;
        if wraps {
            finish_numerator = finish_numerator.sub(I::point(1800.0));
        } else if finish_numerator.hi > 1800.0 {
            return Err(cx.error(
                ContentsRenderErrorKind::Precision,
                "wrapped end is below numerical resolution",
            ));
        }
        let finish = finish_numerator.div_positive(I::point(1800.0));
        let mut table = Table::new(&pieces, cx, budget, cancel)?;
        if table.total().hi == 0.0 {
            self.geometry = Geometry::Pieces(Vec::new());
            return Ok(());
        }
        let cut_budget = loop {
            // Improve weak near-zero chord lower bounds before fixing a relative
            // target: a nearly closed loop can have a large positive arc length.
            let candidate =
                ABS_CUT_BUDGET.min(mul_down(mul_down(table.total().lo, smaller.lo), 0.125));
            if candidate > 0.0
                && candidate.is_finite()
                && table.total().width() <= candidate * 0.125
            {
                break candidate;
            }
            let possible = mul_up(mul_up(table.total().hi, smaller.hi), 0.125);
            if possible <= 0.0 || !possible.is_finite() {
                return Err(cx.error(
                    ContentsRenderErrorKind::Precision,
                    "cut budget is below numerical resolution",
                ));
            }
            table.refine_largest(budget, cancel)?;
        };
        let a = table.invert(begin, begin_numerator, cut_budget, &pieces, budget, cancel)?;
        let b = table.invert(
            finish,
            finish_numerator,
            cut_budget,
            &pieces,
            budget,
            cancel,
        )?;
        let mut output = Vec::new();
        let mut next_boundary = self.next_boundary;
        if wraps {
            extract(
                &pieces,
                a,
                Cut {
                    piece: pieces.len() - 1,
                    t: 1.0,
                },
                &mut output,
                &mut next_boundary,
                cx,
                budget,
                cancel,
            )?;
            extract(
                &pieces,
                Cut { piece: 0, t: 0.0 },
                b,
                &mut output,
                &mut next_boundary,
                cx,
                budget,
                cancel,
            )?;
        } else {
            extract(
                &pieces,
                a,
                b,
                &mut output,
                &mut next_boundary,
                cx,
                budget,
                cancel,
            )?;
        }
        if output.is_empty() {
            return Err(cx.error(
                ContentsRenderErrorKind::Precision,
                "nonempty interval collapsed during extraction",
            ));
        }
        let runs = 1 + output.windows(2).filter(|w| w[0].end != w[1].start).count();
        if output.len() > MAX_PIECES || runs > MAX_RUNS {
            return Err(cx.error(
                ContentsRenderErrorKind::OutputLimit,
                "Trim fragment limit exceeded",
            ));
        }
        self.geometry = Geometry::Pieces(output);
        self.next_boundary = next_boundary;
        Ok(())
    }
    fn pieces(
        &self,
        cx: Context,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<Vec<Piece>, ContentsRenderError> {
        let out = match &self.geometry {
            Geometry::Original(path) => {
                let n = path.vertices.len();
                let count = n.saturating_sub(usize::from(!path.closed));
                if count > MAX_PIECES {
                    return Err(cx.error(
                        ContentsRenderErrorKind::OutputLimit,
                        "too many source segments",
                    ));
                }
                let mut out = Vec::with_capacity(count);
                for i in 0..count {
                    budget.work(cx, cancel)?;
                    let a = &path.vertices[i];
                    let j = (i + 1) % n;
                    let b = &path.vertices[j];
                    let cubic = [
                        a.position,
                        [a.position[0] + a.outgoing[0], a.position[1] + a.outgoing[1]],
                        [b.position[0] + b.incoming[0], b.position[1] + b.incoming[1]],
                        b.position,
                    ];
                    if !finite(cubic) {
                        return Err(
                            cx.error(ContentsRenderErrorKind::NonFinite, "nonfinite source cubic")
                        );
                    }
                    out.push(Piece {
                        cubic,
                        start: i as u64,
                        end: j as u64,
                    });
                }
                out
            }
            Geometry::Pieces(pieces) => {
                for _ in pieces {
                    budget.work(cx, cancel)?;
                }
                pieces.clone()
            }
        };
        Ok(out)
    }
}

// IEEE-754 binary64 directed rounding. Basic operations and sqrt round to
// nearest. Adjacent representables enclose the exact result; error-free TwoSum
// and FMA avoid widening exact arithmetic in analytic boundary fixtures.
fn up(x: f64) -> f64 {
    if x == f64::INFINITY || x.is_nan() {
        x
    } else if x == 0.0 {
        f64::from_bits(1)
    } else {
        f64::from_bits(if x > 0.0 {
            x.to_bits() + 1
        } else {
            x.to_bits() - 1
        })
    }
}
fn down(x: f64) -> f64 {
    -up(-x)
}
fn sum_error(a: f64, b: f64, s: f64) -> f64 {
    let z = s - a;
    (a - (s - z)) + (b - z)
}
fn add_down(a: f64, b: f64) -> f64 {
    let s = a + b;
    if s.is_finite() && sum_error(a, b, s) >= 0.0 {
        s
    } else {
        down(s)
    }
}
fn add_up(a: f64, b: f64) -> f64 {
    let s = a + b;
    if s.is_finite() && sum_error(a, b, s) <= 0.0 {
        s
    } else {
        up(s)
    }
}
fn product_exact(a: f64, b: f64, p: f64) -> bool {
    a == 0.0
        || b == 0.0
        || a == 1.0
        || b == 1.0
        || a == -1.0
        || b == -1.0
        || (p.is_finite() && p.abs() >= f64::from_bits((54_u64) << 52) && a.mul_add(b, -p) == 0.0)
}
fn mul_down(a: f64, b: f64) -> f64 {
    let p = a * b;
    if product_exact(a, b, p) { p } else { down(p) }
}
fn mul_up(a: f64, b: f64) -> f64 {
    let p = a * b;
    if product_exact(a, b, p) { p } else { up(p) }
}
#[derive(Clone, Copy, Debug)]
struct I {
    lo: f64,
    hi: f64,
}
impl I {
    fn point(x: f64) -> Self {
        Self { lo: x, hi: x }
    }
    fn add(self, b: Self) -> Self {
        Self {
            lo: add_down(self.lo, b.lo),
            hi: add_up(self.hi, b.hi),
        }
    }
    fn neg(self) -> Self {
        Self {
            lo: -self.hi,
            hi: -self.lo,
        }
    }
    fn sub(self, b: Self) -> Self {
        self.add(b.neg())
    }
    fn mul(self, b: Self) -> Self {
        let pairs = [
            (self.lo, b.lo),
            (self.lo, b.hi),
            (self.hi, b.lo),
            (self.hi, b.hi),
        ];
        Self {
            lo: pairs
                .iter()
                .map(|&(a, b)| mul_down(a, b))
                .fold(f64::INFINITY, f64::min),
            hi: pairs
                .iter()
                .map(|&(a, b)| mul_up(a, b))
                .fold(f64::NEG_INFINITY, f64::max),
        }
    }
    fn scale(self, x: f64) -> Self {
        self.mul(Self::point(x))
    }
    fn div_positive(self, b: Self) -> Self {
        if b.lo <= 0.0 {
            return Self {
                lo: f64::NEG_INFINITY,
                hi: f64::INFINITY,
            };
        }
        let div = |a: f64, b: f64, lower: bool| {
            let q = a / b;
            let p = q * b;
            if product_exact(q, b, p) && p == a {
                q
            } else if lower {
                down(q)
            } else {
                up(q)
            }
        };
        let pairs = [
            (self.lo, b.lo),
            (self.lo, b.hi),
            (self.hi, b.lo),
            (self.hi, b.hi),
        ];
        Self {
            lo: pairs
                .iter()
                .map(|&(a, b)| div(a, b, true))
                .fold(f64::INFINITY, f64::min),
            hi: pairs
                .iter()
                .map(|&(a, b)| div(a, b, false))
                .fold(f64::NEG_INFINITY, f64::max),
        }
    }
    fn square(self) -> Self {
        let high = self.lo.abs().max(self.hi.abs());
        let low = if self.lo <= 0.0 && self.hi >= 0.0 {
            0.0
        } else {
            self.lo.abs().min(self.hi.abs())
        };
        Self {
            lo: mul_down(low, low).max(0.0),
            hi: mul_up(high, high),
        }
    }
    fn sqrt(self) -> Self {
        let lower = self.lo.max(0.0).sqrt();
        let upper = self.hi.max(0.0).sqrt();
        let exact = |x: f64, y: f64| product_exact(y, y, y * y) && y * y == x;
        Self {
            lo: if exact(self.lo.max(0.0), lower) {
                lower
            } else {
                down(lower).max(0.0)
            },
            hi: if exact(self.hi.max(0.0), upper) {
                upper
            } else {
                up(upper)
            },
        }
    }
    fn intersect(self, b: Self) -> Self {
        Self {
            lo: self.lo.max(b.lo),
            hi: self.hi.min(b.hi),
        }
    }
    fn min(self, b: Self) -> Self {
        Self {
            lo: self.lo.min(b.lo),
            hi: self.hi.min(b.hi),
        }
    }
    fn mid(self) -> f64 {
        self.lo * 0.5 + self.hi * 0.5
    }
    fn width(self) -> f64 {
        add_up(self.hi, -self.lo)
    }
    fn max_abs(self) -> f64 {
        self.lo.abs().max(self.hi.abs())
    }
    fn valid(self) -> bool {
        self.lo.is_finite() && self.hi.is_finite() && self.lo <= self.hi
    }
}
type IP = [I; 2];
type IC = [IP; 4];
fn ip(p: Point) -> IP {
    [I::point(p[0]), I::point(p[1])]
}
fn ic(c: Cubic) -> IC {
    c.map(ip)
}
fn ip_sub(a: IP, b: IP) -> IP {
    [a[0].sub(b[0]), a[1].sub(b[1])]
}
fn ip_add(a: IP, b: IP) -> IP {
    [a[0].add(b[0]), a[1].add(b[1])]
}
fn ip_scale(a: IP, s: f64) -> IP {
    [a[0].scale(s), a[1].scale(s)]
}
fn norm(p: IP) -> I {
    p[0].square().add(p[1].square()).sqrt()
}
fn hull(a: IP, b: IP) -> IP {
    std::array::from_fn(|i| I {
        lo: a[i].lo.min(b[i].lo),
        hi: a[i].hi.max(b[i].hi),
    })
}
fn split_i(c: IC, t: I) -> (IC, IC) {
    let lerp =
        |a: IP, b: IP| std::array::from_fn(|i| a[i].mul(I::point(1.0).sub(t)).add(b[i].mul(t)));
    let a = lerp(c[0], c[1]);
    let b = lerp(c[1], c[2]);
    let d = lerp(c[2], c[3]);
    let e = lerp(a, b);
    let f = lerp(b, d);
    let p = lerp(e, f);
    ([c[0], a, e, p], [p, f, d, c[3]])
}
fn quadrature_bounds(c: IC) -> Option<I> {
    let d = [
        ip_scale(ip_sub(c[1], c[0]), 3.0),
        ip_scale(ip_sub(c[2], c[1]), 3.0),
        ip_scale(ip_sub(c[3], c[2]), 3.0),
    ];
    let v0 = hull(hull(d[0], d[1]), d[2]);
    let v1 = hull(
        ip_scale(ip_sub(d[1], d[0]), 2.0),
        ip_scale(ip_sub(d[2], d[1]), 2.0),
    );
    let v2 = ip_add(ip_sub(d[2], ip_scale(d[1], 2.0)), d[0]);
    let v = [v0, v1, v2];
    let mut s = [I::point(0.0); 7];
    // The zero coefficient uses squares, avoiding the dependency error x*x
    // for an interval crossing zero. Higher coefficients are convolutions.
    s[0] = v0[0].square().add(v0[1].square());
    if s[0].lo <= 0.0 {
        return None;
    }
    for n in 1..7 {
        for i in 0..3 {
            if n >= i && n - i < 3 {
                for axis in 0..2 {
                    s[n] = s[n].add(v[i][axis].mul(v[n - i][axis]));
                }
            }
        }
    }
    let mut r = [I::point(0.0); 7];
    r[0] = s[0].sqrt();
    for n in 1..7 {
        let mut value = s[n];
        for i in 1..n {
            value = value.sub(r[i].mul(r[n - i]));
        }
        r[n] = value.div_positive(r[0].scale(2.0));
    }
    let middle = ip_scale(ip_add(ip_add(d[0], ip_scale(d[1], 2.0)), d[2]), 0.25);
    let estimate = norm(d[0])
        .add(norm(middle).scale(4.0))
        .add(norm(d[2]))
        .div_positive(I::point(6.0));
    let remainder = I::point(r[4].max_abs()).div_positive(I::point(120.0)).hi;
    let result = estimate.add(I {
        lo: -remainder,
        hi: remainder,
    });
    let quarter = ip_scale(
        ip_add(ip_add(ip_scale(d[0], 9.0), ip_scale(d[1], 6.0)), d[2]),
        1.0 / 16.0,
    );
    let three_quarters = ip_scale(
        ip_add(ip_add(d[0], ip_scale(d[1], 6.0)), ip_scale(d[2], 9.0)),
        1.0 / 16.0,
    );
    let boole = norm(d[0])
        .add(norm(d[2]))
        .scale(7.0)
        .add(norm(quarter).add(norm(three_quarters)).scale(32.0))
        .add(norm(middle).scale(12.0))
        .div_positive(I::point(90.0));
    let remainder6 = I::point(r[6].max_abs()).div_positive(I::point(2688.0)).hi;
    let boole = boole.add(I {
        lo: -remainder6,
        hi: remainder6,
    });
    match (result.valid(), boole.valid()) {
        (true, true) => Some(result.intersect(boole)),
        (true, false) => Some(result),
        (false, true) => Some(boole),
        (false, false) => None,
    }
}
fn length(c: IC) -> I {
    let chord = norm(ip_sub(c[3], c[0]));
    let polygon = norm(ip_sub(c[1], c[0]))
        .add(norm(ip_sub(c[2], c[1])))
        .add(norm(ip_sub(c[3], c[2])));
    let geometric = I {
        lo: chord.lo,
        hi: polygon.hi,
    };
    quadrature_bounds(c).map_or(geometric, |s| geometric.intersect(s))
}

#[derive(Clone)]
struct Node {
    first_piece: usize,
    last_piece: usize,
    parent: Option<usize>,
    kind: NodeKind,
    length: I,
}
#[derive(Clone)]
enum NodeKind {
    Leaf {
        cubic: IC,
        piece: usize,
        lo: f64,
        hi: f64,
        depth: u8,
    },
    Branch(usize, usize),
}
#[derive(Clone, Copy, Debug)]
struct HeapEntry {
    uncertainty: f64,
    piece: usize,
    t: f64,
    node: usize,
}
impl PartialEq for HeapEntry {
    fn eq(&self, b: &Self) -> bool {
        self.node == b.node && self.uncertainty == b.uncertainty
    }
}
impl Eq for HeapEntry {}
impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, b: &Self) -> Option<Ordering> {
        Some(self.cmp(b))
    }
}
impl Ord for HeapEntry {
    fn cmp(&self, b: &Self) -> Ordering {
        self.uncertainty
            .total_cmp(&b.uncertainty)
            .then_with(|| b.piece.cmp(&self.piece))
            .then_with(|| b.t.total_cmp(&self.t))
            .then_with(|| b.node.cmp(&self.node))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LengthKey {
    Straight([u64; 2]),
    Translated([u64; 6]),
    Unique(usize),
}
fn length_key(c: Cubic, index: usize) -> LengthKey {
    let rel: [IP; 3] = std::array::from_fn(|i| ip_sub(ip(c[i + 1]), ip(c[0])));
    if rel.iter().flatten().any(|x| x.lo != x.hi) {
        return LengthKey::Unique(index);
    }
    let direction = rel[2];
    let edges = [rel[0], ip_sub(rel[1], rel[0]), ip_sub(rel[2], rel[1])];
    let straight = edges.iter().all(|e| {
        let cross = e[0].mul(direction[1]).sub(e[1].mul(direction[0]));
        let dot = e[0].mul(direction[0]).add(e[1].mul(direction[1]));
        cross.lo == 0.0 && cross.hi == 0.0 && dot.lo >= 0.0
    });
    let bits = |x: f64| if x == 0.0 { 0 } else { x.to_bits() };
    if straight && direction.iter().any(|x| x.lo != 0.0) {
        let mut axis = [bits(direction[0].lo.abs()), bits(direction[1].lo.abs())];
        axis.sort();
        LengthKey::Straight(axis)
    } else {
        LengthKey::Translated(std::array::from_fn(|i| bits(rel[i / 2][i % 2].lo)))
    }
}
struct Table {
    nodes: Vec<Node>,
    heap: BinaryHeap<HeapEntry>,
    root: usize,
    cx: Context,
    length_classes: Vec<Option<usize>>,
    class_totals: Vec<usize>,
}
#[derive(Clone, Copy, Debug)]
struct Cut {
    piece: usize,
    t: f64,
}
impl Table {
    fn new(
        pieces: &[Piece],
        cx: Context,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<Self, ContentsRenderError> {
        let mut table = Self {
            nodes: Vec::new(),
            heap: BinaryHeap::new(),
            root: 0,
            cx,
            length_classes: Vec::new(),
            class_totals: Vec::new(),
        };
        let mut classes = std::collections::BTreeMap::new();
        for (index, piece) in pieces.iter().enumerate() {
            budget.work(cx, cancel)?;
            if piece.cubic.iter().all(|p| *p == piece.cubic[0]) {
                table.length_classes.push(None);
                continue;
            }
            let key = length_key(piece.cubic, index);
            let next = classes.len();
            let class = *classes.entry(key).or_insert(next);
            if class == table.class_totals.len() {
                table.class_totals.push(0);
            }
            table.class_totals[class] += 1;
            table.length_classes.push(Some(class));
        }
        table.root = table.build(pieces, 0, pieces.len(), None, budget, cancel)?;
        Ok(table)
    }
    fn allocate(
        &mut self,
        node: Node,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<usize, ContentsRenderError> {
        budget.work(self.cx, cancel)?;
        if self.nodes.len() >= budget.source_node_limit.min(TRIM_SOURCE_NODE_LIMIT) {
            return Err(self.cx.error(
                ContentsRenderErrorKind::WorkLimit,
                "source-contour measurement node limit exceeded",
            ));
        }
        let id = self.nodes.len();
        self.nodes.push(node);
        budget.peak_source_nodes = budget.peak_source_nodes.max(self.nodes.len());
        Ok(id)
    }
    fn build(
        &mut self,
        pieces: &[Piece],
        lo: usize,
        hi: usize,
        parent: Option<usize>,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<usize, ContentsRenderError> {
        if hi - lo == 1 {
            let cubic = ic(pieces[lo].cubic);
            let length = length(cubic);
            if !length.valid() {
                return Err(self.cx.error(
                    ContentsRenderErrorKind::NonFinite,
                    "invalid arc-length enclosure",
                ));
            }
            let id = self.allocate(
                Node {
                    first_piece: lo,
                    last_piece: lo,
                    parent,
                    kind: NodeKind::Leaf {
                        cubic,
                        piece: lo,
                        lo: 0.0,
                        hi: 1.0,
                        depth: 0,
                    },
                    length,
                },
                budget,
                cancel,
            )?;
            self.heap.push(HeapEntry {
                uncertainty: length.width(),
                piece: lo,
                t: 0.0,
                node: id,
            });
            Ok(id)
        } else {
            let id = self.allocate(
                Node {
                    first_piece: lo,
                    last_piece: hi - 1,
                    parent,
                    kind: NodeKind::Branch(0, 0),
                    length: I::point(0.0),
                },
                budget,
                cancel,
            )?;
            let middle = lo + (hi - lo) / 2;
            let left = self.build(pieces, lo, middle, Some(id), budget, cancel)?;
            let right = self.build(pieces, middle, hi, Some(id), budget, cancel)?;
            self.nodes[id].kind = NodeKind::Branch(left, right);
            self.nodes[id].length = self.nodes[left].length.add(self.nodes[right].length);
            Ok(id)
        }
    }
    fn total(&self) -> I {
        self.nodes[self.root].length
    }
    fn refine_largest(
        &mut self,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<(), ContentsRenderError> {
        loop {
            budget.work(self.cx, cancel)?;
            let entry = self.heap.pop().ok_or_else(|| {
                self.cx.error(
                    ContentsRenderErrorKind::Precision,
                    "no refinable arc-length bounds",
                )
            })?;
            if matches!(self.nodes[entry.node].kind, NodeKind::Leaf { .. }) {
                return self.split(entry.node, budget, cancel);
            }
        }
    }
    fn split(
        &mut self,
        id: usize,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<(), ContentsRenderError> {
        budget.work(self.cx, cancel)?;
        let NodeKind::Leaf {
            cubic,
            piece,
            lo,
            hi,
            depth,
        } = self.nodes[id].kind.clone()
        else {
            unreachable!()
        };
        if depth >= MAX_DEPTH {
            return Err(self.cx.error(
                ContentsRenderErrorKind::Precision,
                "arc-length subdivision depth exceeded",
            ));
        }
        let mid = (lo + hi) * 0.5;
        if mid == lo || mid == hi {
            return Err(self.cx.error(
                ContentsRenderErrorKind::Precision,
                "parameter subdivision made no progress",
            ));
        }
        let (a, b) = split_i(cubic, I::point(0.5));
        let la = length(a);
        let lb = length(b);
        if !la.valid() || !lb.valid() {
            return Err(self.cx.error(
                ContentsRenderErrorKind::NonFinite,
                "invalid subdivided length enclosure",
            ));
        }
        let left = self.allocate(
            Node {
                first_piece: piece,
                last_piece: piece,
                parent: Some(id),
                kind: NodeKind::Leaf {
                    cubic: a,
                    piece,
                    lo,
                    hi: mid,
                    depth: depth + 1,
                },
                length: la,
            },
            budget,
            cancel,
        )?;
        let right = self.allocate(
            Node {
                first_piece: piece,
                last_piece: piece,
                parent: Some(id),
                kind: NodeKind::Leaf {
                    cubic: b,
                    piece,
                    lo: mid,
                    hi,
                    depth: depth + 1,
                },
                length: lb,
            },
            budget,
            cancel,
        )?;
        self.heap.push(HeapEntry {
            uncertainty: la.width(),
            piece,
            t: lo,
            node: left,
        });
        self.heap.push(HeapEntry {
            uncertainty: lb.width(),
            piece,
            t: mid,
            node: right,
        });
        self.nodes[id].kind = NodeKind::Branch(left, right);
        // Intersect the child sum with the previous valid parent enclosure.
        self.nodes[id].length = self.nodes[id].length.intersect(la.add(lb));
        if !self.nodes[id].length.valid() {
            return Err(self.cx.error(
                ContentsRenderErrorKind::Precision,
                "inconsistent child length bounds",
            ));
        }
        let mut parent = self.nodes[id].parent;
        while let Some(p) = parent {
            budget.work(self.cx, cancel)?;
            let NodeKind::Branch(l, r) = self.nodes[p].kind else {
                unreachable!()
            };
            self.nodes[p].length = self.nodes[p]
                .length
                .intersect(self.nodes[l].length.add(self.nodes[r].length));
            if !self.nodes[p].length.valid() {
                return Err(self.cx.error(
                    ContentsRenderErrorKind::Precision,
                    "inconsistent cumulative length bounds",
                ));
            }
            parent = self.nodes[p].parent;
        }
        budget.subdivision_count += 1;
        Ok(())
    }
    // Shared length symbols retain dependencies which interval subtraction
    // intentionally forgets. Equal translated cubics and equal monotone straight
    // chords have provably identical lengths. A rational coefficient identity
    // proves exact gap equality without an epsilon or endless refinement.
    fn common_boundary(
        &self,
        boundary: usize,
        numerator: I,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<bool, ContentsRenderError> {
        if numerator.lo != numerator.hi {
            return Ok(false);
        }
        let mut prefix = vec![0usize; self.class_totals.len()];
        for class in self.length_classes.iter().take(boundary) {
            budget.work(self.cx, cancel)?;
            if let Some(class) = class {
                prefix[*class] += 1;
            }
        }
        for (class, &total) in self.class_totals.iter().enumerate() {
            budget.work(self.cx, cancel)?;
            let difference = I::point(prefix[class] as f64)
                .scale(1800.0)
                .sub(numerator.scale(total as f64));
            if difference.lo != 0.0 || difference.hi != 0.0 {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn invert(
        &mut self,
        q: I,
        numerator: I,
        cut_budget: f64,
        pieces: &[Piece],
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<Cut, ContentsRenderError> {
        if q.lo == 0.0 && q.hi == 0.0 {
            return Ok(Cut { piece: 0, t: 0.0 });
        }
        if q.lo == 1.0 && q.hi == 1.0 {
            return Ok(Cut {
                piece: pieces.len() - 1,
                t: 1.0,
            });
        }
        if !q.valid() || q.lo < 0.0 || q.hi > 1.0 {
            return Err(self.cx.error(
                ContentsRenderErrorKind::Precision,
                "fractional cut is unresolved",
            ));
        }
        'invert: loop {
            budget.work(self.cx, cancel)?;
            budget.inversion_count += 1;
            let target = self.total().mul(q);
            if target.width() > cut_budget * 0.25 {
                self.refine_largest(budget, cancel)?;
                continue;
            }
            let mut node = self.root;
            let mut prefix = I::point(0.0);
            loop {
                budget.work(self.cx, cancel)?;
                match self.nodes[node].kind {
                    NodeKind::Branch(l, r) => {
                        let end = prefix.add(self.nodes[l].length);
                        let left_piece = self.nodes[l].last_piece;
                        let right_piece = self.nodes[r].first_piece;
                        if left_piece != right_piece
                            && pieces[left_piece].end != pieces[right_piece].start
                        {
                            if self.common_boundary(right_piece, numerator, budget, cancel)? {
                                return Ok(Cut {
                                    piece: right_piece,
                                    t: 0.0,
                                });
                            }
                            if target.hi < end.lo {
                                node = l;
                            } else if target.lo > end.hi {
                                prefix = end;
                                node = r;
                            } else {
                                self.refine_largest(budget, cancel)?;
                                continue 'invert;
                            }
                            continue;
                        }
                        if target.mid() <= end.mid() {
                            node = l;
                        } else {
                            prefix = end;
                            node = r;
                        }
                    }
                    NodeKind::Leaf { piece, lo, hi, .. } => {
                        let end = prefix.add(self.nodes[node].length);
                        let candidates =
                            [(Cut { piece, t: lo }, prefix), (Cut { piece, t: hi }, end)];
                        for (cut, cumulative) in candidates {
                            let residual = cumulative.sub(target);
                            let at_gap = if cut.t == 0.0 && piece > 0 {
                                pieces[piece - 1].end != pieces[piece].start
                            } else if cut.t == 1.0 && piece + 1 < pieces.len() {
                                pieces[piece].end != pieces[piece + 1].start
                            } else {
                                false
                            };
                            let boundary = if cut.t == 0.0 { piece } else { piece + 1 };
                            let exact = residual.lo == 0.0 && residual.hi == 0.0;
                            let common = at_gap
                                && self.common_boundary(boundary, numerator, budget, cancel)?;
                            if common {
                                return Ok(cut);
                            }
                            if at_gap && !exact {
                                continue;
                            }
                            if residual.max_abs() <= cut_budget * 0.5
                                && add_up(cumulative.width(), target.width()) <= cut_budget * 0.25
                            {
                                let round = point_rounding(pieces[piece].cubic, cut.t);
                                if round <= cut_budget * 0.5 {
                                    return Ok(cut);
                                }
                                return Err(self.cx.error(
                                    ContentsRenderErrorKind::Precision,
                                    "cut point rounding exceeds feature budget",
                                ));
                            }
                        }
                        // If length uncertainty dominates, refine the global table;
                        // otherwise bisect the leaf containing the desired length.
                        if add_up(prefix.width(), target.width()) > cut_budget * 0.125 {
                            self.refine_largest(budget, cancel)?;
                        } else {
                            self.split(node, budget, cancel)?;
                        }
                        break;
                    }
                }
            }
        }
    }
}
fn finite(c: Cubic) -> bool {
    c.iter().flatten().all(|x| x.is_finite())
}
fn split(c: Cubic, t: f64) -> (Cubic, Cubic) {
    let lerp = |a: Point, b: Point| [a[0] * (1.0 - t) + b[0] * t, a[1] * (1.0 - t) + b[1] * t];
    let a = lerp(c[0], c[1]);
    let b = lerp(c[1], c[2]);
    let d = lerp(c[2], c[3]);
    let e = lerp(a, b);
    let f = lerp(b, d);
    let p = lerp(e, f);
    ([c[0], a, e, p], [p, f, d, c[3]])
}
fn subcubic(c: Cubic, lo: f64, hi: f64) -> Cubic {
    if lo == 0.0 {
        return if hi == 1.0 { c } else { split(c, hi).0 };
    }
    if hi == 1.0 {
        return split(c, lo).1;
    }
    // Cubic blossom: de Casteljau at the two boundary parameters. Evaluate
    // endpoints directly, avoiding a second cut's rounded lo/hi reparameterization.
    let blossom = |s: f64, t: f64, u: f64| {
        let lerp =
            |a: Point, b: Point, q: f64| [a[0] * (1.0 - q) + b[0] * q, a[1] * (1.0 - q) + b[1] * q];
        let a = lerp(c[0], c[1], s);
        let b = lerp(c[1], c[2], s);
        let d = lerp(c[2], c[3], s);
        lerp(lerp(a, b, t), lerp(b, d, t), u)
    };
    [
        split(c, lo).0[3],
        blossom(lo, lo, hi),
        blossom(lo, hi, hi),
        split(c, hi).0[3],
    ]
}
fn point_rounding(c: Cubic, t: f64) -> f64 {
    if t == 0.0 || t == 1.0 {
        return 0.0;
    }
    let nominal = split(c, t).0[3];
    let exact = split_i(ic(c), I::point(t)).0[3];
    norm(std::array::from_fn(|i| exact[i].sub(I::point(nominal[i])))).hi
}
fn extract(
    pieces: &[Piece],
    a: Cut,
    b: Cut,
    out: &mut Vec<Piece>,
    next: &mut u64,
    cx: Context,
    budget: &mut ContentsRenderBudget,
    cancel: Option<&dyn Fn() -> bool>,
) -> Result<(), ContentsRenderError> {
    if a.piece > b.piece || (a.piece == b.piece && a.t > b.t) {
        return Err(cx.error(ContentsRenderErrorKind::Precision, "invalid cut ordering"));
    }
    for (index, piece) in pieces.iter().enumerate().take(b.piece + 1).skip(a.piece) {
        budget.work(cx, cancel)?;
        let lo = if index == a.piece { a.t } else { 0.0 };
        let hi = if index == b.piece { b.t } else { 1.0 };
        if hi == lo {
            continue;
        }
        let mut result = piece.clone();
        result.cubic = subcubic(piece.cubic, lo, hi);
        if hi < 1.0 {
            result.end = *next;
            *next = next.checked_add(1).ok_or_else(|| {
                cx.error(
                    ContentsRenderErrorKind::OutputLimit,
                    "boundary identity exhausted",
                )
            })?;
        }
        if lo > 0.0 {
            result.start = *next;
            *next = next.checked_add(1).ok_or_else(|| {
                cx.error(
                    ContentsRenderErrorKind::OutputLimit,
                    "boundary identity exhausted",
                )
            })?;
        }
        if result.cubic.iter().all(|p| *p == result.cubic[0])
            && !piece.cubic.iter().all(|p| *p == piece.cubic[0])
        {
            return Err(cx.error(
                ContentsRenderErrorKind::Precision,
                "nonconstant cubic collapsed at a cut",
            ));
        }
        if !finite(result.cubic) {
            return Err(cx.error(
                ContentsRenderErrorKind::NonFinite,
                "nonfinite extracted cubic",
            ));
        }
        if out.len() >= MAX_PIECES {
            return Err(cx.error(
                ContentsRenderErrorKind::OutputLimit,
                "Trim cubic piece limit exceeded",
            ));
        }
        out.push(result);
    }
    Ok(())
}

#[cfg(test)]
#[path = "trim_paths_tests.rs"]
mod tests;
