//! Source metadata and timing shared by audio visualization and future mixing.
use super::*;
#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioMetadata {
    pub stream_index: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub channel_layout: String,
    pub duration: f64,
    /// First audio sample relative to the visual source's time zero.
    pub start_time: f64,
    /// Seek offset of the first sample relative to the container's start.
    pub file_offset: f64,
}
impl AudioMetadata {
    pub fn valid(&self) -> bool {
        self.stream_index <= 1024
            && (8000..=384000).contains(&self.sample_rate)
            && (1..=32).contains(&self.channels)
            && self.channel_layout.len() <= 128
            && !self.channel_layout.contains('\0')
            && self.duration.is_finite()
            && self.duration > 0.0
            && self.duration <= 86400.0
            && self.start_time.is_finite()
            && self.start_time.abs() <= 86400.0
            && self.file_offset.is_finite()
            && (0.0..=86400.0).contains(&self.file_offset)
    }
}
impl Content {
    pub fn audio(&self) -> Option<(&str, &AudioMetadata)> {
        match self {
            Self::Audio { path, audio, .. }
            | Self::Video {
                path,
                audio: Some(audio),
                ..
            } => Some((path, audio)),
            _ => None,
        }
    }
}
impl Layer {
    /// Time since the first audio sample. None denotes silence outside the source.
    /// Video frame-rate interpretation scales the attached audio's source clock too.
    pub fn audio_source_time(&self, frame: Frame, fps: impl Into<FrameRate>) -> Option<f64> {
        let time = self.audio_source_seconds(f64::from(frame), fps)?;
        let (_, audio) = self.content.audio()?;
        (time >= 0.0 && time < audio.duration).then_some(time)
    }
    /// Continuous source clock, including out-of-range times (which are silence).
    pub fn audio_source_seconds(&self, frame: f64, fps: impl Into<FrameRate>) -> Option<f64> {
        let (_, audio) = self.content.audio()?;
        let fps = fps.into();
        if !frame.is_finite() || !fps.valid() {
            return None;
        }
        let mut time = if let Some(track) = &self.time_remap {
            track.sample(frame)
        } else {
            let (Content::Audio {
                start_frame,
                playback,
                ..
            }
            | Content::Video {
                start_frame,
                playback,
                ..
            }) = self.content
            else {
                return None;
            };
            playback.source_in + (frame - start_frame as f64) / fps.as_f64() * playback.speed
        };
        if let Content::Video { source_fps, .. } = self.content {
            if let Some(rate) = self.footage_interpretation.fps {
                time *= rate.as_f64() / source_fps;
            }
        }
        time -= audio.start_time;
        if (-1e-9..0.0).contains(&time) {
            time = 0.0;
        }
        Some(time)
    }
}
