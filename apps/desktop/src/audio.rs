//! Linked audio metadata and a bounded, off-thread waveform decoder.
use libre_effects_core::AudioMetadata;
use std::{collections::VecDeque, path::Path, sync::Arc, time::SystemTime};

pub(crate) struct Probe {
    pub audio: Option<AudioMetadata>,
    pub has_video: bool,
}
fn number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| value.as_f64())
        .filter(|n| n.is_finite())
}
pub(crate) fn probe(path: &Path) -> Result<Probe, String> {
    if !path.is_file() {
        return Err("Choose an existing local media file".into());
    }
    let mut cmd = crate::footage::command(&crate::footage::probe_path());
    cmd.args(["-v", "error", "-protocol_whitelist", "file,pipe", "-show_entries", "stream=index,codec_type,sample_rate,channels,channel_layout,duration,start_time:stream_tags=DURATION:stream_disposition=attached_pic:format=duration,start_time", "-of", "json"]).arg(path);
    let data: serde_json::Value =
        serde_json::from_slice(&crate::footage::output(cmd, 1024 * 1024)?)
            .map_err(|e| e.to_string())?;
    parse_probe(&data)
}
fn parse_probe(data: &serde_json::Value) -> Result<Probe, String> {
    let streams = data["streams"].as_array().ok_or("No media streams found")?;
    let video = streams.iter().find(|s| {
        s["codec_type"] == "video" && s["disposition"]["attached_pic"].as_u64() != Some(1)
    });
    let Some(stream) = streams.iter().find(|s| s["codec_type"] == "audio") else {
        return Ok(Probe {
            audio: None,
            has_video: video.is_some(),
        });
    };
    let container_start = number(&data["format"]["start_time"]).unwrap_or(0.0);
    let start = number(&stream["start_time"]).unwrap_or(container_start);
    let reference = video
        .and_then(|s| number(&s["start_time"]))
        .unwrap_or(if video.is_some() {
            container_start
        } else {
            start
        });
    let end_tag = stream["tags"]["DURATION"].as_str().and_then(|s| {
        let parts = s
            .split(':')
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        (parts.len() == 3).then(|| parts[0] * 3600.0 + parts[1] * 60.0 + parts[2])
    });
    let audio = AudioMetadata {
        stream_index: stream["index"]
            .as_u64()
            .and_then(|i| u32::try_from(i).ok())
            .ok_or("Unknown audio stream index")?,
        sample_rate: number(&stream["sample_rate"]).unwrap_or(0.0) as u32,
        channels: stream["channels"].as_u64().unwrap_or(0) as u32,
        channel_layout: stream["channel_layout"]
            .as_str()
            .unwrap_or("unspecified")
            .to_owned(),
        duration: number(&stream["duration"])
            .or_else(|| end_tag.map(|end| end - start))
            .or_else(|| number(&data["format"]["duration"]).map(|d| d - (start - container_start)))
            .unwrap_or(0.0),
        start_time: start - reference,
        file_offset: (start - container_start).max(0.0),
    };
    if !audio.valid() {
        return Err(
            "Audio needs a known duration up to 24 hours, 8–384 kHz and 1–32 channels".into(),
        );
    }
    Ok(Probe {
        audio: Some(audio),
        has_video: video.is_some(),
    })
}

pub(crate) const CHUNK_SECONDS: f64 = 10.0;
pub(crate) const BINS_PER_SECOND: usize = 100;
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Peak {
    pub min: f32,
    pub max: f32,
}
pub(crate) fn decode_chunk(
    path: &str,
    audio: &AudioMetadata,
    chunk: u32,
) -> Result<Arc<Vec<Peak>>, String> {
    if !audio.valid() {
        return Err("Invalid audio metadata".into());
    }
    let start = f64::from(chunk) * CHUNK_SECONDS;
    if start >= audio.duration {
        return Ok(Arc::new(Vec::new()));
    }
    let length = (audio.duration - start).min(CHUNK_SECONDS);
    let rate = audio.sample_rate.min(48000);
    let mut cmd = crate::footage::command(&crate::video_export::ffmpeg_path());
    cmd.args([
        "-v",
        "error",
        "-nostdin",
        "-threads",
        "1",
        "-protocol_whitelist",
        "file,pipe",
        "-ss",
        &format!("{:.9}", start + audio.file_offset),
        "-i",
        path,
        "-map",
        &format!("0:{}", audio.stream_index),
        "-vn",
        "-sn",
        "-dn",
        "-t",
        &format!("{length:.9}"),
        "-af",
        "asetpts=PTS-STARTPTS",
        "-ar",
        &rate.to_string(),
        "-c:a",
        "pcm_f32le",
        "-f",
        "f32le",
        "pipe:1",
    ]);
    let bytes = crate::footage::output(cmd, 64 * 1024 * 1024)?;
    if bytes.is_empty() {
        return Err("Audio decoder returned no samples".into());
    }
    envelope(&bytes, audio.channels as usize, rate)
}
fn envelope(bytes: &[u8], channels: usize, rate: u32) -> Result<Arc<Vec<Peak>>, String> {
    if channels == 0 || bytes.len() % (channels * 4) != 0 {
        return Err("Incomplete audio samples".into());
    }
    let frames = bytes.len() / (channels * 4);
    let bins = (frames as u64 * BINS_PER_SECOND as u64).div_ceil(u64::from(rate)) as usize;
    let mut peaks = vec![Peak::default(); bins];
    for (index, sample) in bytes.chunks_exact(4).enumerate() {
        let sample = f32::from_le_bytes(sample.try_into().unwrap());
        if !sample.is_finite() {
            return Err("Audio contains non-finite samples".into());
        }
        let bin = (index / channels) as u64 * BINS_PER_SECOND as u64 / u64::from(rate);
        peaks[bin as usize].min = peaks[bin as usize].min.min(sample);
        peaks[bin as usize].max = peaks[bin as usize].max.max(sample);
    }
    Ok(Arc::new(peaks))
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Key {
    path: String,
    size: u64,
    modified: Option<SystemTime>,
    metadata: String,
    pub chunk: u32,
}
impl Key {
    pub fn source(path: &str, audio: &AudioMetadata) -> Self {
        let file = std::fs::metadata(path).ok();
        Self {
            path: path.into(),
            size: file.as_ref().map_or(0, |f| f.len()),
            modified: file.and_then(|f| f.modified().ok()),
            metadata: serde_json::to_string(audio).unwrap(),
            chunk: 0,
        }
    }
    pub fn chunk(&self, chunk: u32) -> Self {
        let mut key = self.clone();
        key.chunk = chunk;
        key
    }
}
#[derive(Default)]
pub(crate) struct Waveforms {
    entries: VecDeque<(Key, Result<Arc<Vec<Peak>>, String>)>,
    pub pending: Option<Key>,
}
impl Waveforms {
    pub fn get(&self, key: &Key) -> Option<&Result<Arc<Vec<Peak>>, String>> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn insert(&mut self, key: Key, value: Result<Arc<Vec<Peak>>, String>) {
        self.entries.retain(|(k, _)| k != &key);
        self.entries.push_back((key, value));
        // 4096 × 1000 bins × 8 bytes = 31.25 MiB, plus paths and metadata.
        while self.entries.len() > 4096
            || self
                .entries
                .iter()
                .map(|(key, result)| {
                    key.path.len()
                        + key.metadata.len()
                        + result
                            .as_ref()
                            .map_or_else(|e| e.len(), |v| v.len() * std::mem::size_of::<Peak>())
                })
                .sum::<usize>()
                > 40 * 1024 * 1024
        {
            self.entries.pop_front();
        }
        self.pending = None;
    }
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
