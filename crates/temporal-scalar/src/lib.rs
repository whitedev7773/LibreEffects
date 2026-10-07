//! Native, signed scalar timing math. The owning application keeps values and
//! frame keys; this crate keeps only independently authored timing metadata.
//! These defaults and formulas are native Libre Effects behavior, not a claim
//! about another application's default keys or interpolation implementation.
//!
//! Ease speed is signed scalar units per second. A Bezier side contributes a
//! value displacement `speed * (duration * influence / 100)`, never a fraction
//! of the endpoint value difference. Equal endpoints, reversal and overshoot
//! therefore remain meaningful. All stored fields, including dormant endpoint
//! sides, survive unchanged when callers switch interpolation modes.

mod sampling;

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

pub use sampling::{
    MAX_INVERSION_ITERATIONS, SamplingOptions, sample_segment, sample_segment_with_options,
    validate_segment, validate_segment_with_options,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ScalarInterpolation {
    Linear,
    Bezier,
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalarEase {
    /// Signed scalar units per second. Negative and subnormal values are valid.
    pub speed: f64,
    /// Percentage of the adjacent segment duration, in 0.1..=100.
    pub influence: f64,
}

impl Default for ScalarEase {
    fn default() -> Self {
        Self {
            speed: 0.0,
            influence: 100.0 / 3.0,
        }
    }
}

impl ScalarEase {
    pub fn validate(self) -> Result<(), ScalarError> {
        if !self.speed.is_finite() {
            return Err(ScalarError::InvalidMetadata("speed must be finite"));
        }
        if !self.influence.is_finite() || !(0.1..=100.0).contains(&self.influence) {
            return Err(ScalarError::InvalidMetadata(
                "influence must be in 0.1..=100 percent",
            ));
        }
        Ok(())
    }
}

/// Per-key timing only: no duplicate value or frame-key storage.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalarKeyTiming {
    pub in_interpolation: ScalarInterpolation,
    pub out_interpolation: ScalarInterpolation,
    pub in_ease: ScalarEase,
    pub out_ease: ScalarEase,
    pub temporal_continuous: bool,
    pub temporal_auto_bezier: bool,
}

impl Default for ScalarKeyTiming {
    fn default() -> Self {
        Self {
            in_interpolation: ScalarInterpolation::Linear,
            out_interpolation: ScalarInterpolation::Linear,
            in_ease: ScalarEase::default(),
            out_ease: ScalarEase::default(),
            temporal_continuous: false,
            temporal_auto_bezier: false,
        }
    }
}

impl ScalarKeyTiming {
    pub fn new() -> Self {
        Self::default()
    }

    /// Structural validation includes dormant sides. It never canonicalizes
    /// authored speeds, influences or interpolation modes.
    pub fn validate(&self) -> Result<(), ScalarError> {
        self.in_ease.validate()?;
        self.out_ease.validate()?;
        if self.temporal_continuous {
            return Err(ScalarError::UnsupportedMode("temporal continuous"));
        }
        if self.temporal_auto_bezier {
            return Err(ScalarError::UnsupportedMode("temporal auto Bezier"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScalarError {
    NonFinite,
    InvalidMetadata(&'static str),
    UnsupportedMode(&'static str),
    InvalidTime,
    InvalidFrameRange,
    InvalidSamplingOptions,
    IncomingHold,
    NumericRange,
    PrecisionNotEstablished,
    WorkBudgetExceeded,
}

impl fmt::Display for ScalarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => f.write_str("scalar values must be finite"),
            Self::InvalidMetadata(message) => {
                write!(f, "invalid scalar timing metadata: {message}")
            }
            Self::UnsupportedMode(mode) => write!(f, "unsupported scalar timing mode: {mode}"),
            Self::InvalidTime => {
                f.write_str("sample time must be finite and seconds per frame finite and positive")
            }
            Self::InvalidFrameRange => {
                f.write_str("scalar segment frame keys must be strictly increasing")
            }
            Self::InvalidSamplingOptions => {
                f.write_str("invalid scalar sampling tolerance or work ceiling")
            }
            Self::IncomingHold => {
                f.write_str("incoming hold requires outgoing hold on the preceding key")
            }
            Self::NumericRange => {
                f.write_str("scalar timing computation exceeds finite numeric range")
            }
            Self::PrecisionNotEstablished => {
                f.write_str("scalar timing precision could not be established")
            }
            Self::WorkBudgetExceeded => {
                f.write_str("scalar timing inversion work ceiling exceeded")
            }
        }
    }
}
impl Error for ScalarError {}

#[cfg(test)]
mod tests;
