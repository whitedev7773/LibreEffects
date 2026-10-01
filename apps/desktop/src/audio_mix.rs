//! One sample clock for offline output and subsequent device playback.
//! The mixer never quantizes sound to visual frames. Decoding and disk access
//! belong on a worker, not an audio-device callback or the UI thread.
use libre_effects_core::{AudioMetadata, Content, FrameRate, Layer, Project};
use std::{
    collections::VecDeque,
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::SystemTime,
};

pub(crate) const SAMPLE_RATE: u32 = 48_000;
const CACHE_CHUNKS: usize = 128;
const BLOCK: usize = 4096;
// Valid video frame counts are below 2^31 (24 h at up to 240 fps).
const AUDIO_PHASE: u32 = 1 << 31;
pub(crate) fn progress_label(value: u32) -> Option<String> {
    (value & AUDIO_PHASE != 0)
        .then(|| format!("Mixing audio · {}%", (value & !AUDIO_PHASE).min(100)))
}

#[derive(Clone, Default, Debug)]
pub(crate) struct Levels {
    /// Absolute channel peaks before master clipping.
    pub peak: [f32; 2],
    pub clipped_frames: u64,
    pub frames: u64,
    pub sum_squares: [f64; 2],
}
struct Step {
    layer: Layer,
    fps: FrameRate,
    duration: u32,
}
struct Voice {
    source: usize,
    steps: Vec<Step>,
}
impl Voice {
    fn position(&self, seconds: f64) -> Option<(f64, f64, [f64; 4])> {
        let (mut a, mut b) = (seconds, seconds + 1.0 / f64::from(SAMPLE_RATE));
        let mut matrix = [1.0, 0.0, 0.0, 1.0];
        for step in &self.steps {
            let fps = step.fps.as_f64();
            let (frame, next) = (a * fps, b * fps);
            // Small arithmetic roundoff at an exact frame edge must not add/drop
            // a sample. The tolerance is much less than one audio sample.
            if frame + 1e-9 < f64::from(step.layer.in_frame())
                || frame + 1e-9 >= f64::from(step.layer.out_frame(step.duration))
                || frame + 1e-9 >= f64::from(step.duration)
            {
                return None;
            }
            let m = step.layer.audio_matrix(frame);
            matrix = [
                matrix[0] * m[0] + matrix[1] * m[2],
                matrix[0] * m[1] + matrix[1] * m[3],
                matrix[2] * m[0] + matrix[3] * m[2],
                matrix[2] * m[1] + matrix[3] * m[3],
            ];
            if let Content::Composition { start_frame, .. } = step.layer.content() {
                if let Some(track) = step.layer.time_remap() {
                    a = track.sample(frame);
                    b = track.sample(next);
                } else {
                    a = (frame - *start_frame as f64) / fps;
                    b = (next - *start_frame as f64) / fps;
                }
            } else {
                a = step.layer.audio_source_seconds(frame, step.fps)?;
                b = step.layer.audio_source_seconds(next, step.fps)?;
            }
        }
        // A held source has no advancing waveform: output silence rather than DC.
        (a.is_finite() && b.is_finite() && (b - a).abs() > 1e-12).then_some((a, b - a, matrix))
    }
}
struct Source {
    path: String,
    audio: AudioMetadata,
    stamp: Option<(u64, SystemTime)>,
}
struct Chunk {
    source: usize,
    second: u32,
    pcm: Vec<[f32; 2]>,
}
pub(crate) struct Mixer {
    voices: Vec<Voice>,
    sources: Vec<Source>,
    chunks: VecDeque<Chunk>,
    pub levels: Levels,
}
impl Mixer {
    pub fn new(project: &Project, include_guides: bool) -> Result<Self, String> {
        let mut this = Self {
            voices: Vec::new(),
            sources: Vec::new(),
            chunks: VecDeque::new(),
            levels: Levels::default(),
        };
        let mut nodes = 0;
        this.collect(
            project,
            project.active_composition_id(),
            include_guides,
            &mut Vec::new(),
            &mut nodes,
        )?;
        Ok(this)
    }
    fn collect(
        &mut self,
        project: &Project,
        id: u64,
        include_guides: bool,
        ancestors: &mut Vec<Step>,
        nodes: &mut usize,
    ) -> Result<(), String> {
        if ancestors.len() >= 16 {
            return Err("Audio nesting exceeds 16 levels".into());
        }
        let comp = project
            .composition_by_id(id)
            .ok_or("Missing audio composition")?;
        let solo = comp.layers().iter().any(Layer::solo);
        for layer in comp.layers() {
            if !layer.audio_enabled()
                || (!include_guides && layer.guide())
                || (solo && !layer.solo())
            {
                continue;
            }
            // Visibility, alpha, mattes and visual effects do not mute sound.
            if layer.content().audio().is_none()
                && !matches!(layer.content(), Content::Composition { .. })
            {
                continue;
            }
            *nodes += 1;
            if *nodes > 4096 {
                return Err("Audio graph exceeds 4096 layer instances".into());
            }
            ancestors.push(Step {
                layer: layer.clone(),
                fps: comp.fps(),
                duration: comp.duration(),
            });
            if let Content::Composition { composition, .. } = layer.content() {
                self.collect(project, *composition, false, ancestors, nodes)?;
            } else if let Some((path, audio)) = layer.content().audio() {
                if !audio.valid() {
                    return Err("Invalid source audio metadata".into());
                }
                let source = self
                    .sources
                    .iter()
                    .position(|s| s.path == path && s.audio == *audio)
                    .unwrap_or_else(|| {
                        self.sources.push(Source {
                            path: path.into(),
                            audio: audio.clone(),
                            stamp: None,
                        });
                        self.sources.len() - 1
                    });
                self.voices.push(Voice {
                    source,
                    steps: ancestors
                        .iter()
                        .map(|s| Step {
                            layer: s.layer.clone(),
                            fps: s.fps,
                            duration: s.duration,
                        })
                        .collect(),
                });
            }
            ancestors.pop();
        }
        Ok(())
    }
    pub fn has_audio(&self) -> bool {
        !self.voices.is_empty()
    }
    fn sample(
        &mut self,
        source: usize,
        index: i64,
        cancel: &AtomicBool,
    ) -> Result<[f32; 2], String> {
        let audio = &self.sources[source].audio;
        if index < 0 || index as f64 / f64::from(SAMPLE_RATE) >= audio.duration {
            return Ok([0.0; 2]);
        }
        let second = (index as u64 / u64::from(SAMPLE_RATE)) as u32;
        let offset = (index as u64 % u64::from(SAMPLE_RATE)) as usize;
        let found = self
            .chunks
            .iter()
            .position(|c| c.source == source && c.second == second);
        let index = if let Some(index) = found {
            index
        } else {
            let source_data = &mut self.sources[source];
            let metadata = std::fs::metadata(&source_data.path)
                .map_err(|e| format!("Audio offline: {}: {e}", source_data.path))?;
            let stamp = (
                metadata.len(),
                metadata.modified().map_err(|e| e.to_string())?,
            );
            if source_data.stamp.is_some_and(|old| old != stamp) {
                return Err(
                    "Audio source changed during rendering; retry with the updated source".into(),
                );
            }
            source_data.stamp = Some(stamp);
            let pcm = decode_second(&source_data.path, &source_data.audio, second, cancel)?;
            self.chunks.push_back(Chunk {
                source,
                second,
                pcm,
            });
            if self.chunks.len() > CACHE_CHUNKS {
                self.chunks.pop_front();
            }
            self.chunks.len() - 1
        };
        // Promote only at a chunk boundary, avoiding a VecDeque move per sample.
        let value = self.chunks[index]
            .pcm
            .get(offset)
            .copied()
            .unwrap_or([0.0; 2]);
        if offset == 0 && index + 1 != self.chunks.len() {
            let chunk = self.chunks.remove(index).unwrap();
            self.chunks.push_back(chunk);
        }
        Ok(value)
    }
    /// Output sample indexes are absolute relative to origin, so block size cannot
    /// accumulate clock error. End clips the work area; FPS-rounded tails are silent.
    pub fn render(
        &mut self,
        origin: f64,
        start: u64,
        count: usize,
        end: f64,
        cancel: &AtomicBool,
    ) -> Result<Vec<[f32; 2]>, String> {
        if count > SAMPLE_RATE as usize || !origin.is_finite() || !end.is_finite() {
            return Err("Invalid audio block".into());
        }
        let mut output = vec![[0.0_f32; 2]; count];
        self.levels.frames += count as u64;
        for (offset, value) in output.iter_mut().enumerate() {
            if offset % 256 == 0 && cancel.load(Ordering::Relaxed) {
                return Err("Audio processing canceled".into());
            }
            let seconds = origin + (start + offset as u64) as f64 / f64::from(SAMPLE_RATE);
            if seconds >= end - 1e-12 {
                continue;
            }
            let mut mixed = [0.0_f64; 2];
            for voice in 0..self.voices.len() {
                let Some((time, _rate, matrix)) = self.voices[voice].position(seconds) else {
                    continue;
                };
                let source = self.voices[voice].source;
                if time < 0.0 || time >= self.sources[source].audio.duration {
                    continue;
                }
                let sample = time * f64::from(SAMPLE_RATE);
                let index = sample.floor() as i64;
                let fraction = sample - index as f64;
                let a = self.sample(source, index, cancel)?;
                let b = if fraction < 1e-7 {
                    a
                } else {
                    self.sample(source, index + 1, cancel)?
                };
                let l = f64::from(a[0]) + (f64::from(b[0]) - f64::from(a[0])) * fraction;
                let r = f64::from(a[1]) + (f64::from(b[1]) - f64::from(a[1])) * fraction;
                mixed[0] += matrix[0] * l + matrix[1] * r;
                mixed[1] += matrix[2] * l + matrix[3] * r;
            }
            if mixed.iter().any(|v| v.abs() > 1.0) {
                self.levels.clipped_frames += 1;
            }
            for channel in 0..2 {
                self.levels.peak[channel] =
                    self.levels.peak[channel].max(mixed[channel].abs() as f32);
                self.levels.sum_squares[channel] += mixed[channel] * mixed[channel];
                value[channel] = mixed[channel].clamp(-1.0, 1.0) as f32;
            }
        }
        Ok(output)
    }
}
fn decode_second(
    path: &str,
    audio: &AudioMetadata,
    second: u32,
    cancel: &AtomicBool,
) -> Result<Vec<[f32; 2]>, String> {
    // Compressed formats need decoder/resampler history (e.g. MP3 bit reservoir).
    // Start one second early and discard that decoded history on the sample clock.
    let seek = second.saturating_sub(1);
    let discard = (second - seek) * SAMPLE_RATE;
    let mut cmd = crate::footage::command(&crate::video_export::ffmpeg_path());
    cmd.args([
        "-v",
        "error",
        "-nostdin",
        "-threads",
        "1",
        "-protocol_whitelist",
        "file,pipe",
    ]);
    // Seeking to zero would discard AAC's negative-PTS decoder priming packet.
    if seek != 0 || audio.file_offset != 0.0 {
        cmd.args([
            "-ss",
            &format!("{:.9}", f64::from(seek) + audio.file_offset),
        ]);
    }
    cmd.args([
        "-i",
        path,
        "-map",
        &format!("0:{}", audio.stream_index),
        "-vn",
        "-sn",
        "-dn",
        "-t",
        "1",
        "-af",
        &format!("aresample=48000,atrim=start_sample={discard},asetpts=PTS-STARTPTS"),
        "-ar",
        "48000",
        "-ac",
        "2",
        "-c:a",
        "pcm_f32le",
        "-f",
        "f32le",
        "pipe:1",
    ]);
    let bytes = crate::footage::output_cancellable(cmd, u64::from(SAMPLE_RATE) * 8 + 8192, cancel)?;
    if bytes.is_empty() || bytes.len() % 8 != 0 {
        return Err(format!(
            "Audio decoder returned no complete stereo samples: {path}"
        ));
    }
    let mut pcm = Vec::with_capacity(bytes.len() / 8);
    for sample in bytes.chunks_exact(8).take(SAMPLE_RATE as usize) {
        let value = [
            f32::from_le_bytes(sample[..4].try_into().unwrap()),
            f32::from_le_bytes(sample[4..].try_into().unwrap()),
        ];
        if !value.iter().all(|v| v.is_finite()) {
            return Err("Audio contains non-finite samples".into());
        }
        pcm.push(value);
    }
    Ok(pcm)
}
pub(crate) fn sample_count(frames: u32, fps: FrameRate) -> u64 {
    (u128::from(frames) * u128::from(fps.denominator()) * u128::from(SAMPLE_RATE))
        .div_ceil(u128::from(fps.numerator())) as u64
}
pub(crate) fn prepare(
    project: &Project,
    plan: &crate::output_settings::Plan,
    directory: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> Result<Option<tempfile::NamedTempFile>, String> {
    let mut mixer = Mixer::new(project, false)?;
    if !mixer.has_audio() {
        return Ok(None);
    }
    let mut output = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    let comp = project.composition();
    let origin = comp.fps().seconds(u64::from(plan.range.start));
    let end = comp.fps().seconds(u64::from(plan.range.end));
    let samples = sample_count(plan.frames, plan.fps);
    progress.store(AUDIO_PHASE, Ordering::Relaxed);
    for start in (0..samples).step_by(BLOCK) {
        let data = mixer.render(
            origin,
            start,
            (samples - start).min(BLOCK as u64) as usize,
            end,
            cancel,
        )?;
        let bytes: Vec<_> = data
            .into_iter()
            .flat_map(|sample| sample.into_iter().flat_map(f32::to_le_bytes))
            .collect();
        output
            .write_all(&bytes)
            .map_err(|e| format!("Cannot write temporary audio mix: {e}"))?;
        progress.store(
            AUDIO_PHASE | ((start + bytes.len() as u64 / 8) * 100 / samples).min(100) as u32,
            Ordering::Relaxed,
        );
    }
    output.flush().map_err(|e| e.to_string())?;
    progress.store(0, Ordering::Relaxed);
    Ok(Some(output))
}

#[cfg(test)]
#[path = "audio_mix_tests.rs"]
mod tests;
