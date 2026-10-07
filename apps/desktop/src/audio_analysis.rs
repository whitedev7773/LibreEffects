//! Selected-source analysis on render workers, with one shared frame budget.
use crate::audio_mix::PcmCache;
use crate::audio_selected::SelectedAudioPlan;
use libre_effects_audio_spectrum::{
    SpectrumAnalysisProfile, SpectrumAnalysisSpec, SpectrumAnalyzer, SpectrumFrame, SpectrumLimits,
};
use libre_effects_core::{
    AudioSpectrumSettings, CompositionId, CompositionSample, CompositionSampleKey, Project,
    SpectrumProfile,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_REQUESTS: usize = 64;
const MAX_PCM_WORK: u64 = 2_000_000;
const MAX_DSP_WORK: u64 = 16_000_000;
const MAX_OUTPUT_BANDS: usize = 65_536;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    profile: u8,
    composition: CompositionId,
    layer: u64,
    time: CompositionSampleKey,
    duration: u64,
    offset: u64,
    start: u64,
    end: u64,
    bands: u16,
}
#[derive(Default)]
pub(crate) struct FrameAnalysis {
    results: BTreeMap<Key, Arc<SpectrumFrame>>,
    requests: usize,
    pcm_work: u64,
    dsp_work: u64,
    output_bands: usize,
}
#[derive(Default)]
pub(crate) struct AudioAnalysis {
    pcm: PcmCache,
    analyzer: SpectrumAnalyzer,
}
impl AudioAnalysis {
    /// None denotes an explicitly unassigned source, not silence/error fallback.
    pub(crate) fn analyze(
        &mut self,
        project: &Project,
        composition: CompositionId,
        sample: CompositionSample,
        settings: &AudioSpectrumSettings,
        budget: &mut FrameAnalysis,
        cancel: &AtomicBool,
    ) -> Result<Option<Arc<SpectrumFrame>>, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Audio Spectrum canceled".into());
        }
        settings.validate()?;
        let profile = match settings.profile {
            SpectrumProfile::NativeV1 => SpectrumAnalysisProfile::NativeV1,
            SpectrumProfile::HammingV1 => SpectrumAnalysisProfile::HammingV1,
        };
        let Some(source) = settings.source else {
            return Ok(None);
        };
        if budget.requests >= MAX_REQUESTS {
            return Err("A frame exceeds 64 selected-audio analysis requests".into());
        }
        budget.requests += 1;
        let plan =
            SelectedAudioPlan::new(project, composition, source.layer, settings.input_scope)?;
        let admission = plan.work_estimate(0)?;
        budget.pcm_work = budget
            .pcm_work
            .checked_add(admission.total_work)
            .filter(|work| *work <= MAX_PCM_WORK)
            .ok_or("Audio Spectrum exceeds the shared frame PCM admission budget")?;
        // This check precedes even a successful coefficient-cache hit.
        plan.validate_sources(&mut self.pcm, cancel)?;
        let key = Key {
            profile: match settings.profile {
                SpectrumProfile::NativeV1 => 0,
                SpectrumProfile::HammingV1 => 1,
            },
            composition,
            layer: source.layer,
            time: sample.key(),
            duration: settings.duration_ms.to_bits(),
            offset: settings.offset_ms.to_bits(),
            start: settings.start_hz.to_bits(),
            end: settings.end_hz.to_bits(),
            bands: settings.bands,
        };
        if let Some(result) = budget.results.get(&key) {
            return Ok(Some(result.clone()));
        }
        let spec = SpectrumAnalysisSpec {
            duration_ms: settings.duration_ms,
            start_hz: settings.start_hz,
            end_hz: settings.end_hz,
            bands: settings.bands,
        };
        let work = spec
            .estimate_profile_work(profile)
            .map_err(|e| e.to_string())?;
        let pcm = plan.work_estimate(work.input_frames)?;
        // render_window has its own pre/post checks, in addition to this
        // service's admission/final checks. Charge all four dependency passes.
        let pcm_work = pcm
            .sample_work
            .checked_add(
                (pcm.dependencies as u64)
                    .checked_mul(2)
                    .ok_or("Audio Spectrum dependency work overflow")?,
            )
            .ok_or("Audio Spectrum PCM work overflow")?;
        let next_pcm = budget
            .pcm_work
            .checked_add(pcm_work)
            .ok_or("Audio Spectrum PCM work overflow")?;
        let next_dsp = budget
            .dsp_work
            .checked_add(work.work_units)
            .ok_or("Audio Spectrum analysis work overflow")?;
        let next_bands = budget
            .output_bands
            .checked_add(work.output_bands)
            .ok_or("Audio Spectrum output count overflow")?;
        if next_pcm > MAX_PCM_WORK || next_dsp > MAX_DSP_WORK || next_bands > MAX_OUTPUT_BANDS {
            return Err("Audio Spectrum exceeds the shared frame analysis budget".into());
        }
        budget.pcm_work = next_pcm;
        budget.dsp_work = next_dsp;
        budget.output_bands = next_bands;
        let origin = sample.seconds() + settings.offset_ms / 1000.0 - settings.duration_ms / 2000.0;
        let pcm = plan.render_window(origin, 0, work.input_frames, &mut self.pcm, cancel)?;
        let result = self
            .analyzer
            .analyze_profile(&pcm, &spec, profile, &SpectrumLimits::default(), cancel)
            .map_err(|e| e.to_string())?;
        // A source may have changed during analysis; never memoize that frame.
        plan.validate_sources(&mut self.pcm, cancel)?;
        if cancel.load(Ordering::Relaxed) {
            return Err("Audio Spectrum canceled".into());
        }
        let result = Arc::new(result);
        budget.results.insert(key, result.clone());
        Ok(Some(result))
    }
}
