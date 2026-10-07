//! Joined XY Position retains planar authored data while sharing the bounded
//! spatial distance sampler. The internal zero-Z adapter is not a 3D layer or
//! camera representation, and never becomes the serialized planar track.

use crate::{
    MAX_SPATIAL_KEYS, SamplingOptions, SpatialEase, SpatialError, SpatialInterpolation,
    SpatialKey3, SpatialPosition3,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialKey2 {
    pub value: [f64; 2],
    pub in_interpolation: SpatialInterpolation,
    pub out_interpolation: SpatialInterpolation,
    pub in_ease: SpatialEase,
    pub out_ease: SpatialEase,
    /// Relative XY offsets, not absolute control points.
    pub in_tangent: [f64; 2],
    pub out_tangent: [f64; 2],
    pub temporal_continuous: bool,
    pub temporal_auto_bezier: bool,
    pub spatial_continuous: bool,
    pub spatial_auto_bezier: bool,
}

impl SpatialKey2 {
    /// Native defaults match joined XYZ: linear sides and manual, independent
    /// metadata with zero relative handles and zero dormant speeds.
    pub fn new(value: [f64; 2]) -> Self {
        Self::from_spatial3(SpatialKey3::new(lift(value)))
    }

    pub fn validate(&self) -> Result<(), SpatialError> {
        self.to_spatial3().validate()
    }

    fn to_spatial3(&self) -> SpatialKey3 {
        SpatialKey3 {
            value: lift(self.value),
            in_interpolation: self.in_interpolation,
            out_interpolation: self.out_interpolation,
            in_ease: self.in_ease,
            out_ease: self.out_ease,
            in_tangent: lift(self.in_tangent),
            out_tangent: lift(self.out_tangent),
            temporal_continuous: self.temporal_continuous,
            temporal_auto_bezier: self.temporal_auto_bezier,
            spatial_continuous: self.spatial_continuous,
            spatial_auto_bezier: self.spatial_auto_bezier,
        }
    }

    fn from_spatial3(key: SpatialKey3) -> Self {
        Self {
            value: planar(key.value),
            in_interpolation: key.in_interpolation,
            out_interpolation: key.out_interpolation,
            in_ease: key.in_ease,
            out_ease: key.out_ease,
            in_tangent: planar(key.in_tangent),
            out_tangent: planar(key.out_tangent),
            temporal_continuous: key.temporal_continuous,
            temporal_auto_bezier: key.temporal_auto_bezier,
            spatial_continuous: key.spatial_continuous,
            spatial_auto_bezier: key.spatial_auto_bezier,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialPosition2 {
    /// Static tracks require a value. Keyed tracks may have no authored base;
    /// absence is retained in the saved representation, including after retime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<[f64; 2]>,
    #[serde(deserialize_with = "deserialize_keys")]
    pub keys: BTreeMap<u32, SpatialKey2>,
}

fn deserialize_keys<'de, D>(deserializer: D) -> Result<BTreeMap<u32, SpatialKey2>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct KeysVisitor;
    impl<'de> serde::de::Visitor<'de> for KeysVisitor {
        type Value = BTreeMap<u32, SpatialKey2>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a map of unique u32 frame keys to planar spatial keys")
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let mut result = BTreeMap::new();
            while let Some((frame, key)) = map.next_entry::<u32, SpatialKey2>()? {
                if result.len() >= MAX_SPATIAL_KEYS {
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

impl SpatialPosition2 {
    pub fn new(value: [f64; 2]) -> Self {
        Self {
            value: Some(value),
            keys: BTreeMap::new(),
        }
    }

    /// Structural validation permits staged metadata restoration. As with XYZ,
    /// active continuity and distance compatibility are checked separately.
    pub fn validate(&self) -> Result<(), SpatialError> {
        if let Some(value) = self.value {
            crate::finite3(lift(value))?;
        } else if self.keys.is_empty() {
            return Err(SpatialError::InvalidMetadata(
                "a planar position needs a static value or at least one key",
            ));
        }
        if self.keys.len() > MAX_SPATIAL_KEYS {
            return Err(SpatialError::KeyBudgetExceeded);
        }
        for key in self.keys.values() {
            key.validate()?;
        }
        Ok(())
    }

    pub fn validate_sampling_metadata(&self) -> Result<(), SpatialError> {
        self.to_spatial3()?.validate_sampling_metadata()
    }

    pub fn validate_sampling(&self, seconds_per_frame: f64) -> Result<(), SpatialError> {
        self.validate_sampling_with_options(seconds_per_frame, SamplingOptions::default())
    }

    pub fn validate_sampling_with_options(
        &self,
        seconds_per_frame: f64,
        options: SamplingOptions,
    ) -> Result<(), SpatialError> {
        self.to_spatial3()?
            .validate_sampling_with_options(seconds_per_frame, options)
    }

    /// Temporal speeds are distance units per second, so the frame duration is
    /// explicit. Equal endpoint values may still describe a nonconstant path.
    pub fn sample(&self, frame: f64, seconds_per_frame: f64) -> Result<[f64; 2], SpatialError> {
        self.sample_with_options(frame, seconds_per_frame, SamplingOptions::default())
    }

    pub fn sample_with_options(
        &self,
        frame: f64,
        seconds_per_frame: f64,
        options: SamplingOptions,
    ) -> Result<[f64; 2], SpatialError> {
        self.to_spatial3()?
            .sample_with_options(frame, seconds_per_frame, options)
            .map(planar)
    }

    /// Use the same exact-frame, positive-scale retiming and speed adjustment
    /// as XYZ. Preserve the authored base's presence and every tangent/flag.
    pub fn retimed(
        &self,
        origin_frame: f64,
        scale: f64,
        offset_frames: f64,
    ) -> Result<Self, SpatialError> {
        let retimed = self
            .to_spatial3()?
            .retimed(origin_frame, scale, offset_frames)?;
        Ok(Self {
            value: self.value,
            keys: retimed
                .keys
                .into_iter()
                .map(|(frame, key)| (frame, SpatialKey2::from_spatial3(key)))
                .collect(),
        })
    }

    fn to_spatial3(&self) -> Result<SpatialPosition3, SpatialError> {
        // Check the key budget before allocating the bounded adapter. A keyed
        // sampler never reads the static base; its first key is only an internal
        // fallback, not a newly authored or serialized value.
        self.validate()?;
        let value = self
            .value
            .or_else(|| self.keys.first_key_value().map(|(_, key)| key.value))
            .expect("validated planar track has a static value or a key");
        Ok(SpatialPosition3 {
            value: lift(value),
            keys: self
                .keys
                .iter()
                .map(|(&frame, key)| (frame, key.to_spatial3()))
                .collect(),
        })
    }
}

fn lift([x, y]: [f64; 2]) -> [f64; 3] {
    [x, y, 0.0]
}

fn planar([x, y, _]: [f64; 3]) -> [f64; 2] {
    [x, y]
}
