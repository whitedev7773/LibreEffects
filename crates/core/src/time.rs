//! Exact frame clocks. Timecodes are non-drop-frame labels; seconds are elapsed time.
use super::*;
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum FrameRounding {
    Floor,
    Ceil,
    Nearest,
}

impl FrameRate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, String> {
        if denominator == 0 || numerator == 0 {
            return Err("Frame rate must be between 1 and 240 fps".into());
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let rate = Self {
            numerator: numerator / a,
            denominator: denominator / a,
        };
        if !rate.valid() {
            return Err("Frame rate must be 1–240 fps with a denominator at most 1,000,000".into());
        }
        Ok(rate)
    }
    pub fn numerator(self) -> u32 {
        self.numerator
    }
    pub fn denominator(self) -> u32 {
        self.denominator
    }
    pub fn as_f64(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }
    /// Compact UI label. Use Display for exact rate entry and encoder arguments.
    pub fn label(self) -> String {
        format!("{:.3}", self.as_f64())
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
    pub fn seconds(self, frames: u64) -> f64 {
        frames as f64 * f64::from(self.denominator) / f64::from(self.numerator)
    }
    pub(super) fn valid(self) -> bool {
        self.denominator > 0
            && self.denominator <= 1_000_000
            && self.numerator >= self.denominator
            && u64::from(self.numerator) <= u64::from(self.denominator) * 240
    }
    pub fn max_duration(self) -> Frame {
        (u64::from(self.numerator) * 86_400 / u64::from(self.denominator)) as Frame
    }
    pub fn nominal(self) -> u32 {
        (self.numerator + self.denominator / 2) / self.denominator
    }
    /// Map elapsed frames to another clock without a floating-point intermediate.
    pub fn convert_frames(self, frames: u64, to: Self, rounding: FrameRounding) -> Option<u64> {
        if !self.valid() || !to.valid() {
            return None;
        }
        let value = u128::from(frames) * u128::from(to.numerator) * u128::from(self.denominator);
        let divisor = u128::from(self.numerator) * u128::from(to.denominator);
        let result = match rounding {
            FrameRounding::Floor => value / divisor,
            FrameRounding::Ceil => value.div_ceil(divisor),
            FrameRounding::Nearest => (value + divisor / 2) / divisor,
        };
        u64::try_from(result).ok()
    }
    pub fn convert_origin(self, frame: i64, to: Self) -> Option<i64> {
        let magnitude = self.convert_frames(frame.unsigned_abs(), to, FrameRounding::Nearest)?;
        i64::try_from(i128::from(magnitude) * i128::from(frame.signum())).ok()
    }
    pub fn timecode(self, frame: u64) -> String {
        let fps = u64::from(self.nominal());
        let seconds = frame / fps;
        format!(
            "{:02}:{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            frame % fps
        )
    }
    pub fn parse_timecode(self, value: &str) -> Result<Frame, String> {
        let fields: Vec<_> = value.trim().split(':').collect();
        if fields.len() != 4 {
            return Err("Use non-drop-frame HH:MM:SS:FF".into());
        }
        let numbers: Vec<u64> = fields.iter().map(|s| digits(s)).collect::<Result<_, _>>()?;
        if numbers[0] > 99
            || numbers[1] >= 60
            || numbers[2] >= 60
            || numbers[3] >= u64::from(self.nominal())
        {
            return Err("Timecode has an out-of-range field".into());
        }
        u32::try_from(
            ((numbers[0] * 60 + numbers[1]) * 60 + numbers[2]) * u64::from(self.nominal())
                + numbers[3],
        )
        .map_err(|_| "Timecode exceeds supported range".into())
    }
    /// Bare numbers / `f` are frames; `s` is elapsed seconds, rounded to nearest frame.
    pub fn parse_duration(self, value: &str) -> Result<Frame, String> {
        let value = value.trim();
        if value.contains(':') {
            return self.parse_timecode(value);
        }
        if let Some(seconds) = value.strip_suffix('s') {
            let (n, d) = decimal(seconds.trim(), 9)?;
            let value = u128::from(n) * u128::from(self.numerator);
            let divisor = u128::from(d) * u128::from(self.denominator);
            return u32::try_from((value + divisor / 2) / divisor)
                .map_err(|_| "Duration exceeds supported range".into());
        }
        u32::try_from(digits(value.strip_suffix('f').unwrap_or(value).trim())?)
            .map_err(|_| "Duration exceeds supported range".into())
    }
}

impl From<u32> for FrameRate {
    fn from(value: u32) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }
}
impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            self.numerator.fmt(f)
        } else {
            write!(f, "{}/{}", self.numerator, self.denominator)
        }
    }
}
impl FromStr for FrameRate {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        let value = value.trim();
        let (n, d) = match value {
            "23.976" => (24_000, 1_001),
            "29.97" => (30_000, 1_001),
            "59.94" => (60_000, 1_001),
            "119.88" => (120_000, 1_001),
            _ => match value.split_once('/') {
                Some((n, d)) => (digits(n.trim())?, digits(d.trim())?),
                None => decimal(value, 6)?,
            },
        };
        Self::new(
            u32::try_from(n).map_err(|_| "Frame rate numerator is too large")?,
            u32::try_from(d).map_err(|_| "Frame rate denominator is too large")?,
        )
    }
}
fn digits(value: &str) -> Result<u64, String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Enter a non-negative number".into());
    }
    value
        .parse()
        .map_err(|_| "Number exceeds supported range".into())
}
fn decimal(value: &str, precision: usize) -> Result<(u64, u64), String> {
    if let Some((whole, fraction)) = value.split_once('.') {
        if fraction.len() > precision {
            return Err(format!("Use at most {precision} decimal places"));
        }
        let scale = 10u64.pow(fraction.len() as u32);
        let number = digits(whole)?
            .checked_mul(scale)
            .and_then(|n| n.checked_add(digits(fraction).ok()?))
            .ok_or("Invalid decimal number")?;
        Ok((number, scale))
    } else {
        Ok((digits(value)?, 1))
    }
}
impl Serialize for FrameRate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.denominator == 1 {
            self.numerator.serialize(serializer)
        } else {
            #[derive(Serialize)]
            struct Ratio {
                numerator: u32,
                denominator: u32,
            }
            Ratio {
                numerator: self.numerator,
                denominator: self.denominator,
            }
            .serialize(serializer)
        }
    }
}
impl<'de> Deserialize<'de> for FrameRate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Integer(u32),
            Ratio { numerator: u32, denominator: u32 },
        }
        let (n, d) = match Value::deserialize(deserializer)? {
            Value::Integer(n) => (n, 1),
            Value::Ratio {
                numerator,
                denominator,
            } => (numerator, denominator),
        };
        Self::new(n, d).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn configure(
        e: &mut Editor,
        rate: &str,
        duration: Frame,
        display_start: Frame,
    ) -> Result<(), String> {
        e.execute(Command::ConfigureCompositionRate {
            name: "Exact clock".into(),
            width: 1920,
            height: 1080,
            fps: rate.parse().unwrap(),
            duration,
            display_start,
        })
    }
    #[test]
    fn rates_parse_reduce_and_reject_invalid_clocks() {
        for (input, n, d) in [
            ("23.976", 24000, 1001),
            ("29.97", 30000, 1001),
            ("59.94", 60000, 1001),
            ("48/2", 24, 1),
            ("12.5", 25, 2),
        ] {
            let rate: FrameRate = input.parse().unwrap();
            assert_eq!((rate.numerator(), rate.denominator()), (n, d));
            assert_eq!(rate.to_string().parse::<FrameRate>().unwrap(), rate);
            assert_eq!(
                serde_json::from_str::<FrameRate>(&serde_json::to_string(&rate).unwrap()).unwrap(),
                rate
            );
        }
        for input in [
            "0",
            "0/1",
            "24/0",
            "NaN",
            "-30",
            "0.5",
            "241",
            "240000001/1000000",
            "24/1/2",
            "1e2",
            "12.1234567",
            "18446744073709551615.9",
        ] {
            assert!(input.parse::<FrameRate>().is_err(), "{input}");
        }
        assert_eq!(serde_json::to_string(&FrameRate::from(24)).unwrap(), "24");
        for json in ["0", "24.0", r#"{"numerator":24,"denominator":0}"#] {
            assert!(serde_json::from_str::<FrameRate>(json).is_err());
        }
    }
    #[test]
    fn exact_clocks_do_not_drift_at_hour_boundaries_or_large_origins() {
        let film: FrameRate = "24000/1001".parse().unwrap();
        let ntsc: FrameRate = "30000/1001".parse().unwrap();
        assert_eq!(
            film.convert_frames(86_400, ntsc, FrameRounding::Floor),
            Some(108_000)
        );
        assert_eq!(film.seconds(86_400), 3603.6);
        assert_eq!(film.convert_frames(1, ntsc, FrameRounding::Floor), Some(1));
        assert_eq!(film.convert_frames(1, ntsc, FrameRounding::Ceil), Some(2));
        assert_eq!(
            film.convert_frames(2, ntsc, FrameRounding::Nearest),
            Some(3)
        );
        assert_eq!(film.convert_origin(-2, ntsc), Some(-3));
        assert_eq!(film.convert_origin(i64::MIN, film), Some(i64::MIN));
        assert_eq!(
            FrameRate::from(1).convert_frames(u64::MAX, 240.into(), FrameRounding::Floor),
            None
        );
        assert_eq!(
            FrameRate::from(0).convert_frames(0, ntsc, FrameRounding::Floor),
            None
        );
        assert!(ntsc.seconds(u64::from(ntsc.max_duration())) <= 86_400.0);
        assert!(ntsc.seconds(u64::from(ntsc.max_duration()) + 1) > 86_400.0);
    }
    #[test]
    fn duration_units_and_ndf_timecode_are_distinct_and_checked() {
        let fps: FrameRate = "29.97".parse().unwrap();
        for (value, expected) in [
            ("300", 300),
            ("300f", 300),
            ("10s", 300),
            ("3600s", 107892),
            ("01:00:00:00", 108000),
            ("0.05s", 1),
        ] {
            assert_eq!(fps.parse_duration(value).unwrap(), expected);
        }
        assert_eq!(fps.timecode(108000), "01:00:00:00");
        assert_eq!(fps.timecode(108031), "01:00:01:01");
        for value in [
            "01:00:00;00",
            "00:00:00:30",
            "00:60:00:00",
            "-1",
            "1.2f",
            "100000000000000000000s",
            "1.1234567891s",
        ] {
            assert!(fps.parse_duration(value).is_err(), "{value}");
        }
    }
    #[test]
    fn fractional_settings_roundtrip_history_and_legacy_migration_are_atomic() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        let before = e.project().clone();
        configure(&mut e, "23.976", 240, 86400).unwrap();
        let after = e.project().clone();
        assert_eq!(after.version, 14);
        assert_eq!(after.composition().timecode(24), "01:00:01:00");
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        assert!(configure(&mut e, "23.976", 240, 24 * 86400).is_err());
        assert!(configure(&mut e, "23.976", 3_000_000, 0).is_err());
        assert_eq!(e.project(), &after);
        let mut legacy = serde_json::to_value(&before).unwrap();
        legacy["composition"]
            .as_object_mut()
            .unwrap()
            .remove("display_start");
        assert_eq!(Project::from_json(&legacy.to_string()).unwrap(), before);
        let mut too_old = serde_json::to_value(&after).unwrap();
        too_old["version"] = 13.into();
        assert!(Project::from_json(&too_old.to_string()).is_err());
        e.execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::Rotation,
            frame: 200,
        })
        .unwrap();
        let before = e.project().clone();
        assert!(configure(&mut e, "29.97", 100, 0).is_err());
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn nested_and_pasted_animation_markers_and_video_use_the_same_clock() {
        let mut e = Editor::default();
        configure(&mut e, "23.976", 240, 86400).unwrap();
        e.execute(Command::AddContent {
            content: Content::Video {
                path: "clock.mp4".into(),
                duration: 20.0,
                source_fps: 24.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 20.0,
            height: 20.0,
            name: "Clock".into(),
        })
        .unwrap();
        e.execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::Rotation,
            frame: 24,
        })
        .unwrap();
        e.execute(Command::Marker {
            target: MarkerTarget::Layer(1),
            edit: MarkerEdit::Add { frame: 24 },
        })
        .unwrap();
        e.execute(Command::Marker {
            target: MarkerTarget::Composition,
            edit: MarkerEdit::Add { frame: 1 },
        })
        .unwrap();
        let clip = e.copy_layers(&[1]).unwrap();
        e.execute(Command::NewComposition).unwrap();
        assert!(e.project().composition().markers().is_empty());
        configure(&mut e, "29.97", 600, 0).unwrap();
        e.execute(Command::PasteLayers(clip)).unwrap();
        let copied = e.selected_layer().unwrap();
        assert!(copied.property(Property::Rotation).keys().contains_key(&30));
        assert_eq!(copied.markers()[0].frame(), 30);
        assert_eq!(copied.out_frame(600), 300);
        assert!(
            (copied
                .content()
                .video_source_time(30, e.project().composition().fps())
                .unwrap()
                - 1.001)
                .abs()
                < 1e-12
        );
        e.execute(Command::AddCompositionLayer {
            composition: 1,
            frame: 5,
        })
        .unwrap();
        let nested = e.selected_layer().unwrap();
        assert_eq!(nested.out_frame(600), 305);
        let source = e.project().composition_by_id(1).unwrap();
        let rate = e.project().composition().fps();
        assert_eq!(
            nested.content().composition_frame(35, rate, source),
            Some(24)
        );
        assert_eq!(
            nested.content().composition_frame(304, rate, source),
            Some(239)
        );
        assert_eq!(nested.content().composition_frame(305, rate, source), None);
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
    }
}
