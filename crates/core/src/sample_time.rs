//! Transient visual sample clocks. Authored keys remain integer frame records.
use super::{Frame, FrameRate};

// Matches the admitted Time Remap source-seconds range. Intermediate rational
// arithmetic is independently checked; an unrepresentable clock never rounds.
const MAX_SECONDS: f64 = 20_000_000_000.0;
const OVERFLOW: &str = "Composition sample clock overflow";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Clock {
    Rational { numerator: i128, denominator: u128 },
    Floating(u64),
}

impl Clock {
    fn rational(numerator: i128, denominator: u128) -> Result<Self, String> {
        if denominator == 0 || denominator > i128::MAX as u128 || numerator == i128::MIN {
            return Err(OVERFLOW.into());
        }
        let divisor = gcd(numerator.unsigned_abs(), denominator);
        Ok(Self::Rational {
            numerator: numerator / divisor as i128,
            denominator: denominator / divisor,
        })
    }

    fn floating(value: f64) -> Result<Self, String> {
        if !value.is_finite() {
            return Err("Composition sample must be finite".into());
        }
        Ok(Self::Floating(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    fn value(self) -> f64 {
        match self {
            Self::Rational {
                numerator,
                denominator,
            } => numerator as f64 / denominator as f64,
            Self::Floating(bits) => f64::from_bits(bits),
        }
    }

    fn scaled(self, numerator: u32, denominator: u32) -> Result<Self, String> {
        match self {
            Self::Rational {
                numerator: n,
                denominator: d,
            } => {
                let a = gcd(n.unsigned_abs(), u128::from(denominator));
                let b = gcd(d, u128::from(numerator));
                let n = (n / a as i128)
                    .checked_mul(i128::from(numerator) / b as i128)
                    .ok_or(OVERFLOW)?;
                let d = (d / b)
                    .checked_mul(u128::from(denominator) / a)
                    .ok_or(OVERFLOW)?;
                Self::rational(n, d)
            }
            Self::Floating(bits) => {
                Self::floating(f64::from_bits(bits) * f64::from(numerator) / f64::from(denominator))
            }
        }
    }

    fn subtract(self, other: Self) -> Result<Self, String> {
        match (self, other) {
            (
                Self::Rational {
                    numerator: a,
                    denominator: b,
                },
                Self::Rational {
                    numerator: c,
                    denominator: d,
                },
            ) => {
                let common = gcd(b, d);
                let numerator = a
                    .checked_mul((d / common) as i128)
                    .and_then(|left| {
                        c.checked_mul((b / common) as i128)
                            .and_then(|right| left.checked_sub(right))
                    })
                    .ok_or(OVERFLOW)?;
                let denominator = b.checked_mul(d / common).ok_or(OVERFLOW)?;
                Self::rational(numerator, denominator)
            }
            _ => Self::floating(self.value() - other.value()),
        }
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Exact cache identity. Rational clocks retain their reduced lineage; arbitrary
/// remapped clocks retain every finite f64 bit, except canonical negative zero.
/// No two samples are merged merely because they have the same floor frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CompositionSampleKey {
    seconds: Clock,
    rate_numerator: u32,
    rate_denominator: u32,
}

/// A checked transient clock in one composition's authored timebase.
/// Negative/out-of-duration times are retained for explicit caller range checks.
/// This type is intentionally not serializable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionSample {
    seconds: Clock,
    frame: Clock,
    fps: FrameRate,
}

impl CompositionSample {
    fn new(seconds: Clock, fps: FrameRate) -> Result<Self, String> {
        if !fps.valid() {
            return Err("Invalid composition sample frame rate".into());
        }
        let fps = FrameRate::new(fps.numerator(), fps.denominator())?;
        if seconds.value().abs() > MAX_SECONDS {
            return Err("Composition sample exceeds the source-seconds limit".into());
        }
        let frame = seconds.scaled(fps.numerator(), fps.denominator())?;
        Ok(Self {
            seconds,
            frame,
            fps,
        })
    }

    pub fn from_frame(frame: Frame, fps: FrameRate) -> Result<Self, String> {
        let seconds = Clock::rational(
            i128::from(frame) * i128::from(fps.denominator()),
            u128::from(fps.numerator()),
        )?;
        Self::new(seconds, fps)
    }

    pub fn from_seconds(seconds: f64, fps: FrameRate) -> Result<Self, String> {
        Self::new(Clock::floating(seconds)?, fps)
    }

    /// Change only the coordinate timebase, never the sample's source time.
    pub fn in_rate(self, fps: FrameRate) -> Result<Self, String> {
        Self::new(self.seconds, fps)
    }

    /// Source origins are expressed in the containing composition's frames.
    /// This is independent of visible trim and expression startTime metadata.
    pub fn subtract_frames(self, frames: i64, parent_fps: FrameRate) -> Result<Self, String> {
        if !parent_fps.valid() {
            return Err("Invalid composition sample parent frame rate".into());
        }
        if frames == 0 {
            return Ok(self);
        }
        let offset = Clock::rational(
            i128::from(frames)
                .checked_mul(i128::from(parent_fps.denominator()))
                .ok_or(OVERFLOW)?,
            u128::from(parent_fps.numerator()),
        )?;
        Self::new(self.seconds.subtract(offset)?, self.fps)
    }

    /// Explicit preceding-frame sampling for a preserved source-frame grid.
    /// There is no epsilon or near-integer rounding in this operation.
    pub fn quantized(self, fps: FrameRate) -> Result<Self, String> {
        Self::from_frame(self.in_rate(fps)?.floor_frame()?, fps)
    }

    pub fn seconds(self) -> f64 {
        self.seconds.value()
    }

    pub fn frame(self) -> f64 {
        self.frame.value()
    }

    /// Obtain a safe integer surrogate only after checking the source range.
    pub fn floor_frame(self) -> Result<Frame, String> {
        match self.frame {
            Clock::Rational {
                numerator,
                denominator,
            } => Frame::try_from(numerator.div_euclid(denominator as i128))
                .map_err(|_| "Composition sample is outside the integer frame range".into()),
            Clock::Floating(bits) => {
                let frame = f64::from_bits(bits).floor();
                if frame < 0.0 || frame > f64::from(Frame::MAX) {
                    return Err("Composition sample is outside the integer frame range".into());
                }
                Ok(frame as Frame)
            }
        }
    }

    pub fn is_fractional(self) -> bool {
        match self.frame {
            Clock::Rational { denominator, .. } => denominator != 1,
            Clock::Floating(bits) => f64::from_bits(bits).fract() != 0.0,
        }
    }

    pub fn key(self) -> CompositionSampleKey {
        CompositionSampleKey {
            seconds: self.seconds,
            rate_numerator: self.fps.numerator(),
            rate_denominator: self.fps.denominator(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_player_clocks_recover_exact_integer_boundaries() {
        for frame in [1, 4359, 15768] {
            let root = CompositionSample::from_frame(frame, 60.into()).unwrap();
            let player = root.in_rate(23.into()).unwrap();
            assert!(player.is_fractional());
            let child = player
                .subtract_frames(0, 23.into())
                .unwrap()
                .in_rate(60.into())
                .unwrap();
            assert_eq!(child.frame(), f64::from(frame));
            assert_eq!(child.floor_frame().unwrap(), frame);
            assert!(!child.is_fractional());
            assert_eq!(child.key(), root.key());
        }
        let standalone = CompositionSample::from_frame(1, 23.into())
            .unwrap()
            .in_rate(60.into())
            .unwrap();
        assert_eq!(standalone.frame(), 60.0 / 23.0);
        assert_eq!(standalone.floor_frame().unwrap(), 2);
        assert!(standalone.is_fractional());
    }

    #[test]
    fn signed_origins_and_fractional_rates_retain_exact_lineage() {
        let rate = FrameRate::new(30000, 1001).unwrap();
        let sample = CompositionSample::from_frame(4359, rate).unwrap();
        let shifted = sample.subtract_frames(59, rate).unwrap();
        assert_eq!(shifted.floor_frame().unwrap(), 4300);
        assert_eq!(
            shifted.subtract_frames(-59, rate).unwrap().key(),
            sample.key()
        );
        let negative = CompositionSample::from_frame(0, rate)
            .unwrap()
            .subtract_frames(1, rate)
            .unwrap();
        assert!(negative.seconds() < 0.0);
        assert!(negative.floor_frame().is_err());
        assert!(negative.quantized(rate).is_err());
        assert_eq!(negative.subtract_frames(-1, rate).unwrap().seconds(), 0.0);
    }

    #[test]
    fn preserved_grid_is_explicit_and_does_not_alias_continuous_samples() {
        let root = CompositionSample::from_frame(4359, 60.into()).unwrap();
        let grid = root
            .quantized(23.into())
            .unwrap()
            .in_rate(60.into())
            .unwrap();
        assert_eq!(grid.floor_frame().unwrap(), 4356);
        let a = CompositionSample::from_seconds(0.021, 23.into()).unwrap();
        let b = CompositionSample::from_seconds(0.022, 23.into()).unwrap();
        assert_eq!(a.floor_frame().unwrap(), b.floor_frame().unwrap());
        assert_ne!(a.key(), b.key());
        let near = CompositionSample::from_seconds(f64::from_bits(1.0_f64.to_bits() - 1), 1.into())
            .unwrap();
        assert_eq!(near.floor_frame().unwrap(), 0);
        assert_ne!(
            near.key(),
            CompositionSample::from_seconds(1.0, 1.into())
                .unwrap()
                .key()
        );
        assert_eq!(
            CompositionSample::from_seconds(-0.0, 23.into())
                .unwrap()
                .key(),
            CompositionSample::from_seconds(0.0, 23.into())
                .unwrap()
                .key()
        );
    }

    #[test]
    fn invalid_clocks_and_checked_overflow_are_errors() {
        for seconds in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            MAX_SECONDS + 1.0,
        ] {
            assert!(CompositionSample::from_seconds(seconds, 60.into()).is_err());
        }
        assert!(CompositionSample::from_frame(0, 0.into()).is_err());
        let sample = CompositionSample::from_frame(Frame::MAX, 1.into()).unwrap();
        assert_eq!(sample.floor_frame().unwrap(), Frame::MAX);
        assert!(sample.in_rate(240.into()).unwrap().floor_frame().is_err());
        assert!(sample.subtract_frames(i64::MIN, 1.into()).is_err());
        let huge = Clock::rational(i128::MAX, 1).unwrap();
        assert!(huge.scaled(2, 1).is_err());
        assert!(huge.subtract(Clock::rational(-1, 1).unwrap()).is_err());
        assert!(
            Clock::rational(1, i128::MAX as u128)
                .unwrap()
                .scaled(1, 2)
                .is_err()
        );
    }
}
