//! Native joined XYZ animation. These defaults are Libre Effects defaults, not
//! a claim about another application's key creation behavior.
//!
//! Structural validation deliberately does not inspect adjacent segments. A
//! caller may set values first and metadata second, then validate sampling at a
//! transaction boundary. No endpoint/dormant metadata is canonicalized away.
mod planar;
mod sampling;

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt};

pub use planar::{SpatialKey2, SpatialPosition2};
pub use sampling::{
    MAX_INVERSION_ITERATIONS, MAX_SUBDIVISION_DEPTH, MAX_SUBDIVISIONS, SamplingOptions,
};

/// An allocation/iteration ceiling, independent of a document's stricter budget.
pub const MAX_SPATIAL_KEYS: usize = 100_000;
pub const MAX_KEYS: usize = MAX_SPATIAL_KEYS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SpatialInterpolation {
    Linear,
    Bezier,
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialEase {
    /// Nonnegative spatial distance units per second, independent of XYZ axes.
    pub speed: f64,
    /// Percent of the adjacent segment's duration, in 0.1..=100.
    pub influence: f64,
}

impl Default for SpatialEase {
    fn default() -> Self {
        Self {
            speed: 0.0,
            influence: 100.0 / 3.0,
        }
    }
}

impl SpatialEase {
    pub fn validate(self) -> Result<(), SpatialError> {
        if !self.speed.is_finite() || self.speed < 0.0 {
            return Err(SpatialError::InvalidMetadata(
                "speed must be finite and nonnegative",
            ));
        }
        if !self.influence.is_finite() || !(0.1..=100.0).contains(&self.influence) {
            return Err(SpatialError::InvalidMetadata(
                "influence must be in 0.1..=100 percent",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialKey3 {
    pub value: [f64; 3],
    pub in_interpolation: SpatialInterpolation,
    pub out_interpolation: SpatialInterpolation,
    pub in_ease: SpatialEase,
    pub out_ease: SpatialEase,
    /// Relative XYZ offsets; they are not absolute control-point coordinates.
    pub in_tangent: [f64; 3],
    pub out_tangent: [f64; 3],
    pub temporal_continuous: bool,
    pub temporal_auto_bezier: bool,
    pub spatial_continuous: bool,
    pub spatial_auto_bezier: bool,
}

impl SpatialKey3 {
    /// Explicit native defaults: linear sides, zero relative spatial handles,
    /// zero dormant speeds, and manual independent temporal/spatial metadata.
    pub fn new(value: [f64; 3]) -> Self {
        Self {
            value,
            in_interpolation: SpatialInterpolation::Linear,
            out_interpolation: SpatialInterpolation::Linear,
            in_ease: SpatialEase::default(),
            out_ease: SpatialEase::default(),
            in_tangent: [0.0; 3],
            out_tangent: [0.0; 3],
            temporal_continuous: false,
            temporal_auto_bezier: false,
            spatial_continuous: false,
            spatial_auto_bezier: false,
        }
    }

    pub fn validate(&self) -> Result<(), SpatialError> {
        finite3(self.value)?;
        finite3(self.in_tangent)?;
        finite3(self.out_tangent)?;
        self.in_ease.validate()?;
        self.out_ease.validate()?;
        if self.temporal_continuous {
            return Err(SpatialError::UnsupportedMode("temporal continuous"));
        }
        if self.temporal_auto_bezier {
            return Err(SpatialError::UnsupportedMode("temporal auto Bezier"));
        }
        if self.spatial_auto_bezier {
            return Err(SpatialError::UnsupportedMode("spatial auto Bezier"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialPosition3 {
    pub value: [f64; 3],
    #[serde(deserialize_with = "deserialize_keys")]
    pub keys: BTreeMap<u32, SpatialKey3>,
}

fn deserialize_keys<'de, D>(deserializer: D) -> Result<BTreeMap<u32, SpatialKey3>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct KeysVisitor;
    impl<'de> serde::de::Visitor<'de> for KeysVisitor {
        type Value = BTreeMap<u32, SpatialKey3>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a map of unique u32 frame keys to spatial keys")
        }
        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let mut result = BTreeMap::new();
            while let Some((frame, key)) = map.next_entry::<u32, SpatialKey3>()? {
                if result.len() >= MAX_KEYS {
                    return Err(serde::de::Error::custom("spatial key budget exceeded"));
                }
                if result.insert(frame, key).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate numeric spatial frame key",
                    ));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(KeysVisitor)
}

impl SpatialPosition3 {
    pub fn new(value: [f64; 3]) -> Self {
        Self {
            value,
            keys: BTreeMap::new(),
        }
    }

    /// Cheap structural validation, including dormant metadata and modes.
    /// Active temporal distance/path compatibility is checked separately.
    pub fn validate(&self) -> Result<(), SpatialError> {
        finite3(self.value)?;
        if self.keys.len() > MAX_SPATIAL_KEYS {
            return Err(SpatialError::KeyBudgetExceeded);
        }
        for key in self.keys.values() {
            key.validate()?;
        }
        Ok(())
    }

    /// Cheap structural and active-metadata validation without arc integration.
    /// Manual continuity relates two active sides of an interior key. Endpoint
    /// sides and sides of outgoing-hold segments remain authored dormant data.
    /// Keeping this separate from `validate` permits metadata restoration in
    /// multiple operations before the track is sampled or finalized.
    pub fn validate_sampling_metadata(&self) -> Result<(), SpatialError> {
        self.validate()?;
        let mut keys = self.keys.values();
        let Some(mut previous) = keys.next() else {
            return Ok(());
        };
        let Some(mut current) = keys.next() else {
            return Ok(());
        };
        for next in keys {
            let incoming_active = previous.out_interpolation != SpatialInterpolation::Hold;
            let outgoing_active = current.out_interpolation != SpatialInterpolation::Hold;
            if current.spatial_continuous
                && incoming_active
                && outgoing_active
                && !antiparallel_or_zero(current.in_tangent, current.out_tangent)
            {
                return Err(SpatialError::InvalidMetadata(
                    "active continuous spatial handles must be antiparallel, or either must be zero",
                ));
            }
            previous = current;
            current = next;
        }
        Ok(())
    }

    pub fn sample(&self, frame: f64, seconds_per_frame: f64) -> Result<[f64; 3], SpatialError> {
        self.sample_with_options(frame, seconds_per_frame, SamplingOptions::default())
    }

    pub fn sample_with_options(
        &self,
        frame: f64,
        seconds_per_frame: f64,
        options: SamplingOptions,
    ) -> Result<[f64; 3], SpatialError> {
        self.validate_sampling_metadata()?;
        sampling::validate_time(frame, seconds_per_frame)?;
        options.validate()?;
        let Some((&first_frame, first)) = self.keys.first_key_value() else {
            return Ok(self.value);
        };
        if frame <= f64::from(first_frame) {
            return Ok(first.value);
        }
        let (&last_frame, last) = self.keys.last_key_value().unwrap();
        if frame >= f64::from(last_frame) {
            return Ok(last.value);
        }
        // Frame is now strictly within the u32 key range. Exact key samples
        // return source values without evaluating either neighboring segment.
        let floor = frame.floor() as u32;
        let (&a_frame, a) = self.keys.range(..=floor).next_back().unwrap();
        if frame == f64::from(a_frame) {
            return Ok(a.value);
        }
        let (&b_frame, b) = self.keys.range((a_frame + 1)..).next().unwrap();
        sampling::sample_segment(a_frame, a, b_frame, b, frame, seconds_per_frame, options)
    }

    /// Validate all active segments. Outgoing hold bypasses dormant ease and
    /// spatial path compatibility; incoming hold alone is unsupported.
    pub fn validate_sampling(&self, seconds_per_frame: f64) -> Result<(), SpatialError> {
        self.validate_sampling_with_options(seconds_per_frame, SamplingOptions::default())
    }

    pub fn validate_sampling_with_options(
        &self,
        seconds_per_frame: f64,
        options: SamplingOptions,
    ) -> Result<(), SpatialError> {
        self.validate_sampling_metadata()?;
        sampling::validate_time(0.0, seconds_per_frame)?;
        options.validate()?;
        let mut previous = None;
        for (&frame, key) in &self.keys {
            if let Some((prior_frame, prior_key)) = previous {
                sampling::validate_segment(
                    prior_frame,
                    prior_key,
                    frame,
                    key,
                    seconds_per_frame,
                    options,
                )?;
            }
            previous = Some((frame, key));
        }
        Ok(())
    }

    /// Return a translated copy. Relative handles and all temporal metadata
    /// remain byte-for-byte unchanged; overflow rejects the entire operation.
    pub fn translated(&self, offset: [f64; 3]) -> Result<Self, SpatialError> {
        self.validate()?;
        finite3(offset)?;
        let mut result = self.clone();
        for value in
            std::iter::once(&mut result.value).chain(result.keys.values_mut().map(|k| &mut k.value))
        {
            for i in 0..3 {
                value[i] += offset[i];
            }
            finite3(*value)?;
        }
        Ok(result)
    }

    /// Map keys to origin + (frame - origin) * scale + offset_frames.
    /// The positive scale changes physical duration at an unchanged frame rate,
    /// so every speed (including dormant speeds) is divided by scale. Exact
    /// integer frames are required: no implicit rounding or colliding keys.
    pub fn retimed(
        &self,
        origin_frame: f64,
        scale: f64,
        offset_frames: f64,
    ) -> Result<Self, SpatialError> {
        self.validate()?;
        if !origin_frame.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
            || !offset_frames.is_finite()
        {
            return Err(SpatialError::InvalidRetime);
        }
        let mut keys = BTreeMap::new();
        for (&frame, key) in &self.keys {
            let mapped = (f64::from(frame) - origin_frame) * scale + origin_frame + offset_frames;
            if !mapped.is_finite()
                || !(0.0..=f64::from(u32::MAX)).contains(&mapped)
                || mapped.fract() != 0.0
            {
                return Err(SpatialError::InvalidRetime);
            }
            let mut key = key.clone();
            for ease in [&mut key.in_ease, &mut key.out_ease] {
                let old = ease.speed;
                ease.speed /= scale;
                if !ease.speed.is_finite() || (old != 0.0 && ease.speed == 0.0) {
                    return Err(SpatialError::InvalidRetime);
                }
            }
            if keys.insert(mapped as u32, key).is_some() {
                return Err(SpatialError::RetimeCollision);
            }
        }
        Ok(Self {
            value: self.value,
            keys,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpatialError {
    NonFinite,
    InvalidMetadata(&'static str),
    UnsupportedMode(&'static str),
    KeyBudgetExceeded,
    InvalidTime,
    InvalidSamplingOptions,
    IncomingHold,
    NonzeroSpeedOnConstantPath,
    TemporalDistanceReversal,
    NumericRange,
    PrecisionNotEstablished,
    WorkBudgetExceeded,
    InvalidRetime,
    RetimeCollision,
}

impl fmt::Display for SpatialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => f.write_str("spatial position and tangents must be finite XYZ triples"),
            Self::InvalidMetadata(message) => write!(f, "invalid spatial metadata: {message}"),
            Self::UnsupportedMode(mode) => write!(f, "unsupported spatial mode: {mode}"),
            Self::KeyBudgetExceeded => f.write_str("spatial key budget exceeded"),
            Self::InvalidTime => f.write_str("sample time must be finite and seconds per frame finite and positive"),
            Self::InvalidSamplingOptions => f.write_str("invalid spatial sampling tolerance or work ceiling"),
            Self::IncomingHold => f.write_str("incoming hold requires outgoing hold on the preceding key"),
            Self::NonzeroSpeedOnConstantPath => f.write_str("nonzero active spatial speed on a constant path"),
            Self::TemporalDistanceReversal => f.write_str("temporal distance reversal or overshoot is unsupported"),
            Self::NumericRange => f.write_str("spatial computation exceeds finite numeric range"),
            Self::PrecisionNotEstablished => f.write_str("spatial sampling precision could not be established"),
            Self::WorkBudgetExceeded => f.write_str("spatial subdivision or inversion work ceiling exceeded"),
            Self::InvalidRetime => f.write_str("retiming requires positive finite scale, exact in-range frame keys and representable speeds"),
            Self::RetimeCollision => f.write_str("retiming would merge distinct spatial keys"),
        }
    }
}
impl Error for SpatialError {}

fn finite3(value: [f64; 3]) -> Result<(), SpatialError> {
    if value.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(SpatialError::NonFinite)
    }
}

fn antiparallel_or_zero(a: [f64; 3], b: [f64; 3]) -> bool {
    let a_scale = a.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let b_scale = b.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    if a_scale == 0.0 || b_scale == 0.0 {
        return true;
    }
    let a = a.map(|v| v / a_scale);
    let b = b.map(|v| v / b_scale);
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    // Direction-only comparison, with a small roundoff allowance. Independent
    // lengths are intentional; normalize first to preserve subnormal handles.
    a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>() < 0.0
        && cross.iter().all(|v| v.abs() <= 64.0 * f64::EPSILON)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod planar_tests;
