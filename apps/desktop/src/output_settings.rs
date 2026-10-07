//! Output clocks and channels are independent of the captured composition.
use libre_effects_core::{Composition, FrameRate, FrameRounding};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[cfg(test)]
#[path = "output_settings_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Format {
    Mp4,
    MovAlpha,
    PngAlpha,
    PngBackground,
}
impl Format {
    pub const ALL: [Self; 4] = [
        Self::Mp4,
        Self::MovAlpha,
        Self::PngAlpha,
        Self::PngBackground,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "H.264 MP4",
            Self::MovAlpha => "ProRes 4444 MOV",
            Self::PngAlpha => "PNG sequence · Alpha",
            Self::PngBackground => "PNG sequence · Background",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::MovAlpha => "mov",
            _ => "frames",
        }
    }
    pub fn sequence(self) -> bool {
        matches!(self, Self::PngAlpha | Self::PngBackground)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Channels {
    #[default]
    Auto,
    Rgb,
    Rgba,
    Alpha,
}
impl Channels {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
            Self::Alpha => "alpha",
        }
    }
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "rgb" => Ok(Self::Rgb),
            "rgba" => Ok(Self::Rgba),
            "alpha" => Ok(Self::Alpha),
            _ => Err("Channels: auto, rgb, rgba or alpha".into()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RateControl {
    Crf(u8),
    Bitrate(u32),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AudioOutput {
    #[default]
    Auto,
    Off,
}
impl AudioOutput {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Off => "off",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FontPolicy {
    /// Use the same fallback as preview, and retain an output warning.
    #[default]
    Fallback,
    /// Missing families or substituted faces stop the output before rendering.
    Strict,
}
impl FontPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fallback => "fallback",
            Self::Strict => "strict",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Settings {
    #[serde(default)]
    pub fonts: FontPolicy,
    #[serde(default)]
    pub audio: AudioOutput,
    /// None follows composition size/rate. Size is the exact output raster, stretched.
    pub size: Option<[u32; 2]>,
    pub fps: Option<FrameRate>,
    pub channels: Channels,
    pub rate_control: Option<RateControl>,
    pub encoder_speed: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Spec {
    pub format: Format,
    pub settings: Settings,
}
impl From<Format> for Spec {
    fn from(format: Format) -> Self {
        Self {
            format,
            settings: Settings::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Field {
    Audio,
    Fonts,
    Size,
    Fps,
    Channels,
    Quality,
    Speed,
}
impl Field {
    pub const ALL: [Self; 7] = [
        Self::Size,
        Self::Fps,
        Self::Channels,
        Self::Quality,
        Self::Speed,
        Self::Audio,
        Self::Fonts,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Audio => "Audio",
            Self::Fonts => "Fonts",
            Self::Size => "Size",
            Self::Fps => "FPS",
            Self::Channels => "Channels",
            Self::Quality => "Quality",
            Self::Speed => "Encoder",
        }
    }
}
impl Settings {
    pub fn summary(&self, format: Format) -> String {
        let size = self
            .size
            .map_or("Composition size".into(), |[w, h]| format!("{w} × {h}"));
        let fps = self
            .fps
            .map_or("Composition FPS".into(), |fps| format!("{fps} fps"));
        let channels = match self.channels(format) {
            Channels::Rgb => "RGB + background",
            Channels::Rgba => "RGB + Alpha",
            Channels::Alpha => "Alpha only",
            _ => unreachable!(),
        };
        let encoding = if format == Format::Mp4 {
            let quality = match self.rate_control.unwrap_or(RateControl::Crf(18)) {
                RateControl::Crf(n) => format!("CRF {n}"),
                RateControl::Bitrate(n) => format!("Target {n} kbps"),
            };
            format!(
                " · {quality} · {}",
                self.encoder_speed.as_deref().unwrap_or("medium")
            )
        } else {
            String::new()
        };
        let audio = if format.sequence() || self.audio == AudioOutput::Off {
            " · No audio"
        } else if format == Format::Mp4 {
            " · Auto AAC · 48 kHz stereo"
        } else {
            " · Auto PCM · 48 kHz stereo"
        };
        format!(
            "{size} · {fps} · {channels}{encoding}{audio} · Fonts: {}",
            self.fonts.label()
        )
    }
    pub fn value(&self, field: Field) -> String {
        match field {
            Field::Audio => self.audio.label().into(),
            Field::Fonts => self.fonts.label().into(),
            Field::Size => self.size.map_or("comp".into(), |[w, h]| format!("{w}x{h}")),
            Field::Fps => self.fps.map_or("comp".into(), |f| f.to_string()),
            Field::Channels => self.channels.label().into(),
            Field::Quality => match self.rate_control {
                None => "auto".into(),
                Some(RateControl::Crf(n)) => format!("crf:{n}"),
                Some(RateControl::Bitrate(n)) => format!("kbps:{n}"),
            },
            Field::Speed => self.encoder_speed.clone().unwrap_or("auto".into()),
        }
    }
    pub fn change(&mut self, field: Field, value: &str) -> Result<(), String> {
        let value = value.trim().to_ascii_lowercase();
        match field {
            Field::Fonts => {
                self.fonts = match value.as_str() {
                    "fallback" => FontPolicy::Fallback,
                    "strict" => FontPolicy::Strict,
                    _ => return Err(
                        "Fonts: fallback (warn and render) or strict (require exact family/style)"
                            .into(),
                    ),
                }
            }
            Field::Audio => {
                self.audio = match value.as_str() {
                    "auto" => AudioOutput::Auto,
                    "off" => AudioOutput::Off,
                    _ => {
                        return Err(
                            "Audio: auto or off (AAC in MP4, PCM in MOV; 48 kHz stereo)".into()
                        );
                    }
                }
            }
            Field::Size => {
                self.size = if value == "comp" {
                    None
                } else {
                    let (w, h) = value.split_once('x').ok_or("Size: comp or WIDTHxHEIGHT")?;
                    Some([
                        w.trim().parse().map_err(|_| "Invalid output width")?,
                        h.trim().parse().map_err(|_| "Invalid output height")?,
                    ])
                }
            }
            Field::Fps => {
                self.fps = if value == "comp" {
                    None
                } else {
                    Some(value.parse()?)
                }
            }
            Field::Channels => self.channels = Channels::parse(&value)?,
            Field::Quality => {
                self.rate_control = if value == "auto" {
                    None
                } else if let Some(v) = value.strip_prefix("crf:") {
                    Some(RateControl::Crf(v.parse().map_err(|_| "CRF must be 0–51")?))
                } else if let Some(v) = value.strip_prefix("kbps:") {
                    Some(RateControl::Bitrate(
                        v.parse().map_err(|_| "Bitrate must be 1–1,000,000 kbps")?,
                    ))
                } else {
                    return Err("Quality: auto, crf:18 or kbps:8000".into());
                }
            }
            Field::Speed => self.encoder_speed = if value == "auto" { None } else { Some(value) },
        }
        Ok(())
    }
    pub fn validate(&self, format: Format) -> Result<(), String> {
        if let Some([w, h]) = self.size {
            if w == 0
                || h == 0
                || w > 16384
                || h > 16384
                || u64::from(w) * u64::from(h) > 33_554_432
            {
                return Err(
                    "Output size must be 1–16384 per axis and at most 32 megapixels".into(),
                );
            }
            if format == Format::Mp4 && (w % 2 != 0 || h % 2 != 0) {
                return Err("H.264 requires even output dimensions".into());
            }
        }
        if let Some(fps) = self.fps {
            FrameRate::new(fps.numerator(), fps.denominator())?;
        }
        if self.channels == Channels::Rgba && format == Format::Mp4 {
            return Err("H.264 MP4 cannot preserve alpha; use MOV or PNG".into());
        }
        if format != Format::Mp4 && (self.rate_control.is_some() || self.encoder_speed.is_some()) {
            return Err("CRF, bitrate and encoder speed apply only to H.264 MP4".into());
        }
        if matches!(self.rate_control,Some(RateControl::Crf(n)) if n>51)
            || matches!(self.rate_control,Some(RateControl::Bitrate(n)) if n==0 || n>1_000_000)
        {
            return Err("CRF must be 0–51; bitrate must be 1–1,000,000 kbps".into());
        }
        if self.encoder_speed.as_deref().is_some_and(|s| {
            ![
                "ultrafast",
                "superfast",
                "veryfast",
                "faster",
                "fast",
                "medium",
                "slow",
                "slower",
                "veryslow",
            ]
            .contains(&s)
        }) {
            return Err("Encoder speed: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower or veryslow".into());
        }
        Ok(())
    }
    pub fn channels(&self, format: Format) -> Channels {
        if self.channels != Channels::Auto {
            self.channels
        } else if matches!(format, Format::Mp4 | Format::PngBackground) {
            Channels::Rgb
        } else {
            Channels::Rgba
        }
    }
    pub fn plan(
        &self,
        comp: &Composition,
        range: Range<u32>,
        format: Format,
    ) -> Result<Plan, String> {
        self.validate(format)?;
        if range.is_empty() || range.end > comp.duration() {
            return Err("Output range must be inside the composition".into());
        }
        let [width, height] = self.size.unwrap_or([comp.width(), comp.height()]);
        let fps = self.fps.unwrap_or(comp.fps());
        let frames = comp
            .fps()
            .convert_frames(u64::from(range.end - range.start), fps, FrameRounding::Ceil)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("Output duration overflow")?;
        let timecode_frame = comp
            .fps()
            .convert_frames(
                u64::from(comp.display_start()) + u64::from(range.start),
                fps,
                FrameRounding::Nearest,
            )
            .ok_or("Output timecode overflow")?;
        Ok(Plan {
            width,
            height,
            fps,
            frames,
            source_fps: comp.fps(),
            range,
            timecode: fps.timecode(timecode_frame),
        })
    }
    pub fn apply_channels(&self, pixels: &mut image::RgbaImage, format: Format, background: u32) {
        match self.channels(format) {
            Channels::Rgb => crate::rendering::composite_background(pixels, background),
            Channels::Alpha => {
                for pixel in pixels.pixels_mut() {
                    let a = pixel[3];
                    pixel.0 = [a, a, a, 255];
                }
            }
            _ => {}
        }
    }
    /// Store the selected PNG channels, not an unused opaque alpha plane.
    /// apply_channels must run first so RGB already includes the background.
    pub fn png_bytes(&self, pixels: image::RgbaImage, format: Format) -> Result<Vec<u8>, String> {
        let image = image::DynamicImage::ImageRgba8(pixels);
        let image = match self.channels(format) {
            Channels::Rgb => image::DynamicImage::ImageRgb8(image.to_rgb8()),
            Channels::Alpha => image::DynamicImage::ImageLuma8(image.to_luma8()),
            _ => image,
        };
        let mut data = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut data, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(data.into_inner())
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub frames: u32,
    source_fps: FrameRate,
    pub range: Range<u32>,
    pub timecode: String,
}
impl Plan {
    /// Output sample zero starts exactly at the selected composition frame.
    /// Hold/floor sampling duplicates/drops source frames without changing speed.
    pub fn source_frame(&self, index: u32) -> u32 {
        let offset = self
            .fps
            .convert_frames(u64::from(index), self.source_fps, FrameRounding::Floor)
            .unwrap_or(0);
        (u64::from(self.range.start) + offset).min(u64::from(self.range.end - 1)) as u32
    }
    pub fn sequence_first(&self) -> u32 {
        if self.fps == self.source_fps {
            self.range.start
        } else {
            0
        }
    }
}
