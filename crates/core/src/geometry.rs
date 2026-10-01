use serde::{Deserialize, Serialize};

/// 2D affine transform: [a, b, c, d, tx, ty]. Multiplication applies rhs first.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine(pub [f64; 6]);

impl Default for Affine {
    fn default() -> Self {
        Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    }
}
impl Affine {
    pub fn point(self, [x, y]: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, tx, ty] = self.0;
        [a * x + c * y + tx, b * x + d * y + ty]
    }
    pub fn vector(self, [x, y]: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, _, _] = self.0;
        [a * x + c * y, b * x + d * y]
    }
    pub fn compose(self, rhs: Self) -> Self {
        let [a, b, c, d, tx, ty] = self.0;
        let [e, f, g, h, ux, uy] = rhs.0;
        Self([
            a * e + c * f,
            b * e + d * f,
            a * g + c * h,
            b * g + d * h,
            a * ux + c * uy + tx,
            b * ux + d * uy + ty,
        ])
    }
    pub fn inverse(self) -> Option<Self> {
        let [a, b, c, d, tx, ty] = self.0;
        let det = a * d - b * c;
        if !det.is_finite() || det.abs() < 1e-10 {
            return None;
        }
        let result = Self([
            d / det,
            -b / det,
            -c / det,
            a / det,
            (c * ty - d * tx) / det,
            (b * tx - a * ty) / det,
        ]);
        result.valid().then_some(result)
    }
    pub(crate) fn valid(self) -> bool {
        self.0.iter().all(|v| v.is_finite() && v.abs() <= 1e12)
    }
}

/// Normalized temporal cubic Bezier. X is time, Y is progress (may overshoot).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bezier {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}
impl Default for Bezier {
    fn default() -> Self {
        Self {
            x1: 1.0 / 3.0,
            y1: 0.0,
            x2: 2.0 / 3.0,
            y2: 1.0,
        }
    }
}
impl Bezier {
    pub fn valid(self) -> bool {
        [self.x1, self.x2]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && [self.y1, self.y2]
                .iter()
                .all(|v| v.is_finite() && (-2.0..=3.0).contains(v))
    }
    pub fn progress(self, time: f64) -> f64 {
        if time <= 0.0 {
            return 0.0;
        }
        if time >= 1.0 {
            return 1.0;
        }
        fn cubic(t: f64, a: f64, b: f64) -> f64 {
            let u = 1.0 - t;
            3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
        }
        // Bisection remains stable with vertical or coincident tangent handles.
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..40 {
            let t = (low + high) * 0.5;
            if cubic(t, self.x1, self.x2) < time {
                low = t;
            } else {
                high = t;
            }
        }
        cubic((low + high) * 0.5, self.y1, self.y2)
    }
}
