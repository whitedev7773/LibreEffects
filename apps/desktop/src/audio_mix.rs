//! One sample clock for offline output and subsequent device playback.
//! The mixer never quantizes sound to visual frames. Decoding and disk access
//! belong on a worker, not an audio-device callback or the UI thread.
use libre_effects_core::{AudioMetadata, Content, FrameRate, Layer, Project};
use std::{
    collections::VecDeque,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::SystemTime,
};

pub(crate) const SAMPLE_RATE: u32 = 48_000;
const CACHE_CHUNKS: usize = 128;
const CACHE_BYTES: usize = 64 * 1024 * 1024;
const CACHE_SOURCES: usize = 4096;
const MAX_SOURCE_PATH_BYTES: usize = 16 * 1024;
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
    fn position(&self, seconds: f64) -> Result<Option<(f64, f64, [f64; 4])>, String> {
        voice_position(
            self.steps.iter().map(|s| (&s.layer, s.fps, s.duration)),
            seconds,
        )
    }
}
/// Shared continuous audio clock. This intentionally never uses visual sampling,
/// expressions, masks, or effect references.
pub(crate) fn voice_position<'a>(
    steps: impl Iterator<Item = (&'a Layer, FrameRate, u32)>,
    seconds: f64,
) -> Result<Option<(f64, f64, [f64; 4])>, String> {
    let (mut a, mut b) = (seconds, seconds + 1.0 / f64::from(SAMPLE_RATE));
    let mut matrix = [1.0, 0.0, 0.0, 1.0];
    for (layer, rate, duration) in steps {
        let fps = rate.as_f64();
        let (frame, next) = (a * fps, b * fps);
        if !frame.is_finite() || !next.is_finite() {
            return Err("Non-finite audio source clock".into());
        }
        // Preserve the existing mixer's tolerance at exact trim edges.
        if frame + 1e-9 < layer.in_frame_sample()
            || frame + 1e-9 >= layer.out_frame_sample(duration)
            || frame + 1e-9 >= f64::from(duration)
        {
            return Ok(None);
        }
        let m = layer.audio_matrix(frame);
        matrix = [
            matrix[0] * m[0] + matrix[1] * m[2],
            matrix[0] * m[1] + matrix[1] * m[3],
            matrix[2] * m[0] + matrix[3] * m[2],
            matrix[2] * m[1] + matrix[3] * m[3],
        ];
        if matrix.iter().any(|v| !v.is_finite()) {
            return Err("Non-finite audio matrix".into());
        }
        if let Content::Composition { start_frame, .. } = layer.content() {
            if let Some(track) = layer.time_remap() {
                a = track.sample(frame);
                b = track.sample(next);
            } else {
                a = (frame - *start_frame as f64) / fps;
                b = (next - *start_frame as f64) / fps;
            }
        } else {
            a = layer
                .audio_source_seconds(frame, rate)
                .ok_or("Invalid audio source clock")?;
            b = layer
                .audio_source_seconds(next, rate)
                .ok_or("Invalid audio source clock")?;
        }
        if !a.is_finite() || !b.is_finite() {
            return Err("Non-finite audio source clock".into());
        }
    }
    // A held source has no advancing waveform: output silence rather than DC.
    Ok(((b - a).abs() > 1e-12).then_some((a, b - a, matrix)))
}
struct Source {
    path: String,
    audio: AudioMetadata,
    // Retain both the requested path and its resolved identity. Repointing a
    // symlink must invalidate the session just like replacing the file itself.
    canonical: Option<PathBuf>,
    stamp: Option<(u64, SystemTime)>,
}
struct Chunk {
    source: usize,
    second: u32,
    pcm: Vec<[f32; 2]>,
}
#[cfg(test)]
type TestDecoder =
    dyn FnMut(&str, &AudioMetadata, u32, &AtomicBool) -> Result<Vec<[f32; 2]>, String> + Send;

/// Session cache shared by independently constructed selected-layer plans.
/// Keys include the path, full metadata and pinned resolved file stamp. Pins
/// survive PCM eviction; explicit footage refresh starts a new cache/session.
#[derive(Default)]
pub(crate) struct PcmCache {
    sources: Vec<Source>,
    chunks: VecDeque<Chunk>,
    #[cfg(test)]
    decoder: Option<Box<TestDecoder>>,
}
impl PcmCache {
    pub(crate) fn register(&mut self, path: &str, audio: &AudioMetadata) -> Result<usize, String> {
        if !audio.valid() || path.is_empty() || path.len() > MAX_SOURCE_PATH_BYTES {
            return Err("Invalid source audio metadata or path".into());
        }
        if let Some(index) = self
            .sources
            .iter()
            .position(|s| s.path == path && s.audio == *audio)
        {
            return Ok(index);
        }
        if self.sources.len() >= CACHE_SOURCES {
            return Err("Audio cache exceeds 4096 sources".into());
        }
        let mut source_path = String::new();
        source_path
            .try_reserve_exact(path.len())
            .map_err(|_| "Cannot allocate audio source path")?;
        source_path.push_str(path);
        let mut layout = String::new();
        layout
            .try_reserve_exact(audio.channel_layout.len())
            .map_err(|_| "Cannot allocate audio metadata")?;
        layout.push_str(&audio.channel_layout);
        let metadata = AudioMetadata {
            stream_index: audio.stream_index,
            sample_rate: audio.sample_rate,
            channels: audio.channels,
            channel_layout: layout,
            duration: audio.duration,
            start_time: audio.start_time,
            file_offset: audio.file_offset,
        };
        self.sources
            .try_reserve_exact(1)
            .map_err(|_| "Cannot allocate audio source table")?;
        self.make_room(source_path.capacity() + metadata.channel_layout.capacity())?;
        self.sources.push(Source {
            path: source_path,
            audio: metadata,
            canonical: None,
            stamp: None,
        });
        Ok(self.sources.len() - 1)
    }
    fn memory_bytes(&self) -> usize {
        self.sources.capacity() * std::mem::size_of::<Source>()
            + self.chunks.capacity() * std::mem::size_of::<Chunk>()
            + self
                .sources
                .iter()
                .map(|s| {
                    s.path.capacity()
                        + s.audio.channel_layout.capacity()
                        + s.canonical.as_ref().map_or(0, PathBuf::capacity)
                })
                .sum::<usize>()
            + self
                .chunks
                .iter()
                .map(|c| c.pcm.capacity() * std::mem::size_of::<[f32; 2]>())
                .sum::<usize>()
    }
    fn make_room(&mut self, additional: usize) -> Result<(), String> {
        while self
            .memory_bytes()
            .checked_add(additional)
            .is_none_or(|n| n > CACHE_BYTES)
        {
            if self.chunks.pop_front().is_none() {
                return Err("Audio PCM cache exceeds 64 MiB".into());
            }
        }
        Ok(())
    }
    /// Called for every selected request, including cache hits and zero-padded
    /// windows. Offline/changed sources must never become cached silent success.
    pub(crate) fn validate_source(
        &mut self,
        index: usize,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check_cancel(cancel)?;
        let source = self
            .sources
            .get(index)
            .ok_or("Invalid audio source handle")?;
        let canonical = std::fs::canonicalize(&source.path)
            .map_err(|e| format!("Audio offline: {}: {e}", source.path))?;
        if canonical.as_os_str().as_encoded_bytes().len() > MAX_SOURCE_PATH_BYTES {
            return Err("Resolved audio path exceeds limit".into());
        }
        let metadata = std::fs::metadata(&canonical)
            .map_err(|e| format!("Audio offline: {}: {e}", source.path))?;
        if !metadata.is_file() {
            return Err(format!("Audio offline: not a file: {}", source.path));
        }
        let stamp = (
            metadata.len(),
            metadata.modified().map_err(|e| e.to_string())?,
        );
        if self.sources.iter().any(|old| {
            // A second plan/stream or path alias must not repin a file that this
            // session already observed, even when full audio metadata differs.
            (old.path == source.path
                && old
                    .canonical
                    .as_ref()
                    .is_some_and(|resolved| *resolved != canonical))
                || ((old.path == source.path || old.canonical.as_ref() == Some(&canonical))
                    && old.stamp.is_some_and(|pinned| pinned != stamp))
        }) {
            return Err(
                "Audio source changed during rendering; retry with the updated source".into(),
            );
        }
        if source.canonical.is_none() {
            self.make_room(canonical.capacity())?;
            self.sources[index].canonical = Some(canonical);
        }
        self.sources[index].stamp = Some(stamp);
        check_cancel(cancel)
    }
    fn insert_chunk(&mut self, chunk: Chunk, cancel: &AtomicBool) -> Result<(), String> {
        if chunk.pcm.is_empty() || chunk.pcm.len() > SAMPLE_RATE as usize {
            return Err("Audio decoder returned invalid sample count".into());
        }
        for samples in chunk.pcm.chunks(256) {
            check_cancel(cancel)?;
            if samples.iter().flatten().any(|v| !v.is_finite()) {
                return Err("Audio contains non-finite samples".into());
            }
        }
        if self.chunks.len() >= CACHE_CHUNKS {
            self.chunks.pop_front();
        }
        self.chunks
            .try_reserve_exact(1)
            .map_err(|_| "Cannot allocate audio chunk table")?;
        self.make_room(
            chunk
                .pcm
                .capacity()
                .checked_mul(std::mem::size_of::<[f32; 2]>())
                .ok_or("Audio PCM cache size overflow")?,
        )?;
        check_cancel(cancel)?;
        self.chunks.push_back(chunk);
        Ok(())
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
            self.validate_source(source, cancel)?;
            let source_data = &self.sources[source];
            #[cfg(test)]
            let pcm = if let Some(decoder) = &mut self.decoder {
                decoder(&source_data.path, &source_data.audio, second, cancel)?
            } else {
                decode_second(&source_data.path, &source_data.audio, second, cancel)?
            };
            #[cfg(not(test))]
            let pcm = decode_second(&source_data.path, &source_data.audio, second, cancel)?;
            // Never admit partial/canceled chunks or a source changed by decoding.
            self.validate_source(source, cancel)?;
            self.insert_chunk(
                Chunk {
                    source,
                    second,
                    pcm,
                },
                cancel,
            )?;
            self.chunks.len() - 1
        };
        let value = self.chunks[index]
            .pcm
            .get(offset)
            .copied()
            .unwrap_or([0.0; 2]);
        // Preserve the existing cache promotion policy at chunk boundaries.
        if offset == 0 && index + 1 != self.chunks.len() {
            let chunk = self.chunks.remove(index).unwrap();
            self.chunks.push_back(chunk);
        }
        Ok(value)
    }
    /// Existing 48 kHz linear interpolation, including the tiny-fraction cutoff.
    pub(crate) fn interpolated(
        &mut self,
        source: usize,
        time: f64,
        cancel: &AtomicBool,
    ) -> Result<[f64; 2], String> {
        if !time.is_finite() {
            return Err("Non-finite audio sample position".into());
        }
        let audio = &self
            .sources
            .get(source)
            .ok_or("Invalid audio source handle")?
            .audio;
        if time < 0.0 || time >= audio.duration {
            return Ok([0.0; 2]);
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
        if a.iter().chain(&b).any(|v| !v.is_finite()) {
            return Err("Audio contains non-finite samples".into());
        }
        Ok(std::array::from_fn(|c| {
            f64::from(a[c]) + (f64::from(b[c]) - f64::from(a[c])) * fraction
        }))
    }
    #[cfg(test)]
    pub(crate) fn with_decoder(decoder: Box<TestDecoder>) -> Self {
        Self {
            decoder: Some(decoder),
            ..Self::default()
        }
    }
}
pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Audio processing canceled".into())
    } else {
        Ok(())
    }
}
pub(crate) fn silent_window(count: usize) -> Result<Vec<[f32; 2]>, String> {
    if count > SAMPLE_RATE as usize {
        return Err("Audio window exceeds 48000 stereo frames".into());
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| "Cannot allocate audio window")?;
    output.resize(count, [0.0; 2]);
    Ok(output)
}
pub(crate) struct Mixer {
    voices: Vec<Voice>,
    cache: PcmCache,
    pub levels: Levels,
}
impl Mixer {
    pub fn new(project: &Project, include_guides: bool) -> Result<Self, String> {
        let mut this = Self {
            voices: Vec::new(),
            cache: PcmCache::default(),
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
                let source = self.cache.register(path, audio)?;
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
        self.render_speed(origin, start, count, end, 1.0, cancel)
    }
    /// Preview speed changes the source time for every output sample, keeping
    /// audio and rendered video on the same clock (pitch follows speed).
    pub fn render_speed(
        &mut self,
        origin: f64,
        start: u64,
        count: usize,
        end: f64,
        speed: f64,
        cancel: &AtomicBool,
    ) -> Result<Vec<[f32; 2]>, String> {
        if !speed.is_finite() || !(0.25..=2.0).contains(&speed) {
            return Err("Invalid audio preview speed".into());
        }
        if count > SAMPLE_RATE as usize || !origin.is_finite() || !end.is_finite() {
            return Err("Invalid audio block".into());
        }
        let mut output = silent_window(count)?;
        self.levels.frames += count as u64;
        for (offset, value) in output.iter_mut().enumerate() {
            if offset % 256 == 0 && cancel.load(Ordering::Relaxed) {
                return Err("Audio processing canceled".into());
            }
            let seconds = origin + (start + offset as u64) as f64 * speed / f64::from(SAMPLE_RATE);
            if seconds >= end - 1e-12 {
                continue;
            }
            let mut mixed = [0.0_f64; 2];
            for voice in 0..self.voices.len() {
                let Some((time, _rate, matrix)) = self.voices[voice].position(seconds)? else {
                    continue;
                };
                let source = self.voices[voice].source;
                let [l, r] = self.cache.interpolated(source, time, cancel)?;
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
    let mut pcm = Vec::new();
    pcm.try_reserve_exact((bytes.len() / 8).min(SAMPLE_RATE as usize))
        .map_err(|_| "Cannot allocate decoded audio chunk")?;
    for sample in bytes.chunks_exact(8).take(SAMPLE_RATE as usize) {
        check_cancel(cancel)?;
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
