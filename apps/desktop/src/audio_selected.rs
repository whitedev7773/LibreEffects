//! Native SelectedLayerOutput analysis, independent of the active master mix.
//!
//! Plans borrow the immutable authored project: no sampled visual views, visual
//! effects, or expressions are traversed. The chosen layer contributes its own
//! trim, source clock, remap and audio matrix. Siblings and ancestors outside that
//! selected subtree never contribute. Descendants use output audio/guide/solo
//! rules; the selected layer's explicit selection ignores its owner's solo/guide
//! routing, while its own audio switch is still honored.
use crate::audio_mix::{PcmCache, SAMPLE_RATE, check_cancel, silent_window, voice_position};
use libre_effects_core::{
    AudioMetadata, CompositionId, Content, FrameRate, Layer, LayerId, Project, SpectrumInputScope,
};
use std::sync::atomic::AtomicBool;

const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 4096;
// Two adjacent samples in each of two channels. This is a deterministic work
// weight for interpolation, not a prediction of CPU time or decoder I/O.
const INTERPOLATION_WORK: u64 = 4;

/// Checked work units for admission to the renderer's shared per-frame budget.
/// Silent intervals cannot discount an otherwise audible authored voice: its
/// trim/remap must still be sampled to discover that silence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PcmWorkEstimate {
    pub sample_work: u64,
    /// Layer instances inspected while collecting the structural subtree,
    /// including silent and non-audio leaves, without visual-property traversal.
    pub graph_nodes: usize,
    /// Unique selected path/metadata dependencies, including muted routes.
    pub dependencies: usize,
    /// Sample work plus graph nodes and two dependency validation passes.
    pub total_work: u64,
}

#[derive(Clone, Copy)]
struct Step<'a> {
    layer: &'a Layer,
    fps: FrameRate,
    duration: u32,
}
struct Voice<'a> {
    source: usize,
    steps: Vec<Step<'a>>,
}
/// A source plan is immutable and bound to a specific authored project snapshot.
/// Its PCM cache may be reused with other plans from that snapshot/session.
pub(crate) struct SelectedAudioPlan<'a> {
    sources: Vec<(&'a str, &'a AudioMetadata)>,
    voices: Vec<Voice<'a>>,
    graph_nodes: usize,
}
impl<'a> SelectedAudioPlan<'a> {
    pub(crate) fn new(
        project: &'a Project,
        owner: CompositionId,
        selected: LayerId,
        scope: SpectrumInputScope,
    ) -> Result<Self, String> {
        match scope {
            SpectrumInputScope::SelectedLayerOutput => {}
        }
        // Authored data is mandatory even when a sampled view happens to retain
        // some audio tracks today. Reject that accidental dependency explicitly.
        if project.render_sample_receipt().is_some() {
            return Err("Selected audio requires the authored project snapshot".into());
        }
        let comp = project
            .composition_by_id(owner)
            .ok_or("Missing selected audio composition")?;
        let layer = comp
            .layer(selected)
            .ok_or("Missing selected audio layer in owning composition")?;
        if !layer.can_audio() {
            return Err("Selected layer has no audio source".into());
        }
        let mut plan = Self {
            sources: Vec::new(),
            voices: Vec::new(),
            graph_nodes: 0,
        };
        let mut steps = Vec::new();
        steps
            .try_reserve_exact(MAX_DEPTH)
            .map_err(|_| "Cannot allocate selected audio graph")?;
        let mut nodes = 0;
        plan.collect_layer(
            project,
            layer,
            comp.fps(),
            comp.duration(),
            true,
            &mut steps,
            &mut nodes,
        )?;
        if plan.sources.is_empty() {
            return Err("Selected layer contains no audio media".into());
        }
        Ok(plan)
    }
    #[allow(clippy::too_many_arguments)]
    fn collect_layer(
        &mut self,
        project: &'a Project,
        layer: &'a Layer,
        fps: FrameRate,
        duration: u32,
        enabled: bool,
        ancestors: &mut Vec<Step<'a>>,
        nodes: &mut usize,
    ) -> Result<(), String> {
        self.graph_nodes = self
            .graph_nodes
            .checked_add(1)
            .ok_or("Selected audio graph work overflow")?;
        if !layer.can_audio() {
            return Ok(());
        }
        if ancestors.len() >= MAX_DEPTH {
            return Err("Audio nesting exceeds 16 levels".into());
        }
        *nodes += 1;
        if *nodes > MAX_NODES {
            return Err("Audio graph exceeds 4096 layer instances".into());
        }
        ancestors.push(Step {
            layer,
            fps,
            duration,
        });
        let enabled = enabled && layer.audio_enabled();
        if let Content::Composition { composition, .. } = layer.content() {
            let child = project
                .composition_by_id(*composition)
                .ok_or("Missing selected audio descendant composition")?;
            let solo = child.layers().iter().any(Layer::solo);
            for layer in child.layers() {
                // Walk the structural dependency even when it is currently
                // silent. A mute must not hide offline/changed selected media.
                self.collect_layer(
                    project,
                    layer,
                    child.fps(),
                    child.duration(),
                    enabled && !layer.guide() && (!solo || layer.solo()),
                    ancestors,
                    nodes,
                )?;
            }
        } else if let Some((path, audio)) = layer.content().audio() {
            if !audio.valid() {
                return Err("Invalid selected source audio metadata".into());
            }
            let source = if let Some(index) = self
                .sources
                .iter()
                .position(|(p, a)| *p == path && **a == *audio)
            {
                index
            } else {
                self.sources
                    .try_reserve_exact(1)
                    .map_err(|_| "Cannot allocate selected audio sources")?;
                self.sources.push((path, audio));
                self.sources.len() - 1
            };
            if enabled {
                let mut steps = Vec::new();
                steps
                    .try_reserve_exact(ancestors.len())
                    .map_err(|_| "Cannot allocate selected audio voice")?;
                steps.extend_from_slice(ancestors);
                self.voices
                    .try_reserve_exact(1)
                    .map_err(|_| "Cannot allocate selected audio voices")?;
                self.voices.push(Voice { source, steps });
            }
        }
        ancestors.pop();
        Ok(())
    }
    /// No allocation, decoding or media access. The renderer must reserve
    /// `total_work` before rendering this window; exceeding its shared budget
    /// rejects the request instead of reducing voices, duration or resolution.
    pub(crate) fn work_estimate(&self, count: usize) -> Result<PcmWorkEstimate, String> {
        if count > SAMPLE_RATE as usize {
            return Err("Audio window exceeds 48000 stereo frames".into());
        }
        let overflow = || "Selected audio PCM work overflow".to_string();
        let per_sample = self.voices.iter().try_fold(0_u64, |total, voice| {
            let steps = u64::try_from(voice.steps.len()).map_err(|_| overflow())?;
            total
                .checked_add(steps.checked_add(INTERPOLATION_WORK).ok_or_else(overflow)?)
                .ok_or_else(overflow)
        })?;
        let sample_work = (count as u64)
            .checked_mul(per_sample)
            .ok_or_else(overflow)?;
        let graph_work = u64::try_from(self.graph_nodes).map_err(|_| overflow())?;
        let dependency_work = u64::try_from(self.sources.len())
            .map_err(|_| overflow())?
            .checked_mul(2)
            .ok_or_else(overflow)?;
        let total_work = sample_work
            .checked_add(graph_work)
            .and_then(|work| work.checked_add(dependency_work))
            .ok_or_else(overflow)?;
        Ok(PcmWorkEstimate {
            sample_work,
            graph_nodes: self.graph_nodes,
            dependencies: self.sources.len(),
            total_work,
        })
    }
    /// Structural selected media, including muted/guide/non-solo descendants.
    /// Paths can repeat when the same file has distinct authored audio metadata.
    pub(crate) fn dependencies(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.sources.iter().map(|(path, _)| *path)
    }
    /// Admit even a memoized spectral request through this check. The renderer
    /// must not serve its own coefficient cache before checking selected media.
    pub(crate) fn validate_sources(
        &self,
        cache: &mut PcmCache,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        check_cancel(cancel)?;
        for (path, audio) in &self.sources {
            let source = cache.register(path, audio)?;
            cache.validate_source(source, cancel)?;
        }
        Ok(())
    }
    /// Unclipped, unmetered stereo samples on the absolute 48 kHz owner clock.
    /// `start` is an offset from the original origin, allowing block partitioning
    /// without repeated origin additions. Source/trim boundaries are zero padded.
    pub(crate) fn render_window(
        &self,
        origin: f64,
        start: u64,
        count: usize,
        cache: &mut PcmCache,
        cancel: &AtomicBool,
    ) -> Result<Vec<[f32; 2]>, String> {
        check_cancel(cancel)?;
        // Beyond 2^53 an integer sample offset cannot be represented exactly.
        // Normal authored timelines are bounded to 24 h, far below this ceiling.
        let end = start
            .checked_add(count as u64)
            .ok_or("Invalid selected audio window")?;
        if !origin.is_finite()
            || end > (1_u64 << 53)
            || !(origin + end as f64 / f64::from(SAMPLE_RATE)).is_finite()
        {
            return Err("Invalid selected audio window".into());
        }
        let mut output = silent_window(count)?;
        let mut handles = Vec::new();
        handles
            .try_reserve_exact(self.sources.len())
            .map_err(|_| "Cannot allocate selected audio handles")?;
        for (path, audio) in &self.sources {
            let source = cache.register(path, audio)?;
            cache.validate_source(source, cancel)?;
            handles.push(source);
        }
        for (offset, value) in output.iter_mut().enumerate() {
            if offset % 256 == 0 {
                check_cancel(cancel)?;
            }
            let seconds = origin + (start + offset as u64) as f64 / f64::from(SAMPLE_RATE);
            let mut mixed = [0.0_f64; 2];
            for (index, voice) in self.voices.iter().enumerate() {
                if index % 256 == 0 {
                    check_cancel(cancel)?;
                }
                let Some((time, _, matrix)) = voice_position(
                    voice.steps.iter().map(|s| (s.layer, s.fps, s.duration)),
                    seconds,
                )?
                else {
                    continue;
                };
                let [l, r] = cache.interpolated(handles[voice.source], time, cancel)?;
                mixed[0] += matrix[0] * l + matrix[1] * r;
                mixed[1] += matrix[2] * l + matrix[3] * r;
            }
            for channel in 0..2 {
                value[channel] = mixed[channel] as f32;
                if !value[channel].is_finite() {
                    return Err("Non-finite selected audio sum".into());
                }
            }
        }
        // Catch mutations during an entirely cached request as well as decode.
        self.validate_sources(cache, cancel)?;
        Ok(output)
    }
}

#[cfg(test)]
#[path = "audio_selected_tests.rs"]
mod tests;
