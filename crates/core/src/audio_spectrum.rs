//! Explicit native spectrum settings. These do not claim AE DSP equivalence.
use super::*;
#[cfg(test)]
#[path = "audio_spectrum_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpectrumProfile {
    NativeV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpectrumInputScope {
    SelectedLayerOutput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpectrumDisplay {
    Line,
    Bars,
    Points,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpectrumSide {
    Above,
    Below,
    Both,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectrumSource {
    /// Stable layer identity in the effect owner's composition.
    pub layer: LayerId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSpectrumSettings {
    pub profile: SpectrumProfile,
    pub input_scope: SpectrumInputScope,
    pub source: Option<SpectrumSource>,
    pub bands: u16,
    pub start_hz: f64,
    pub end_hz: f64,
    pub duration_ms: f64,
    pub offset_ms: f64,
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub maximum_height: f64,
    pub thickness: f64,
    pub color: [u8; 4],
    pub display: SpectrumDisplay,
    pub side: SpectrumSide,
    pub composite_original: bool,
}
impl Default for AudioSpectrumSettings {
    fn default() -> Self {
        Self {
            profile: SpectrumProfile::NativeV1,
            input_scope: SpectrumInputScope::SelectedLayerOutput,
            source: None,
            bands: 64,
            start_hz: 20.0,
            end_hz: 800.0,
            duration_ms: 90.0,
            offset_ms: 0.0,
            start: [0.0, 0.0],
            end: [640.0, 0.0],
            maximum_height: 480.0,
            thickness: 4.0,
            color: [255; 4],
            display: SpectrumDisplay::Line,
            side: SpectrumSide::Above,
            composite_original: false,
        }
    }
}
impl AudioSpectrumSettings {
    pub fn validate(&self) -> Result<(), String> {
        let within =
            |value: f64, min: f64, max: f64| value.is_finite() && (min..=max).contains(&value);
        if !(1..=4096).contains(&self.bands)
            || !within(self.start_hz, 0.0, 24_000.0)
            || !within(self.end_hz, self.start_hz, 24_000.0)
            || !within(self.duration_ms, 1.0, 1000.0)
            || !within(self.offset_ms, -86_400_000.0, 86_400_000.0)
            || !within(self.maximum_height, 0.0, 32_768.0)
            || !within(self.thickness, 0.1, 256.0)
            || self
                .start
                .iter()
                .chain(&self.end)
                .any(|v| !within(*v, -32_768.0, 32_768.0))
            || self.start == self.end
            || self.source.is_some_and(|source| source.layer == 0)
        {
            return Err("Invalid native Audio Spectrum settings".into());
        }
        Ok(())
    }
    pub(crate) fn fit_layer(&mut self, width: f64, height: f64) {
        self.start = [0.0, height];
        self.end = [width, height];
        self.maximum_height = self.maximum_height.min(height);
    }
}

pub(super) fn validate_owner(layer: &Layer) -> Result<(), String> {
    if !matches!(layer.content, Content::Rectangle | Content::Solid) || layer.is_three_d() {
        return Err("Native Audio Spectrum requires a 2D Rectangle or Solid".into());
    }
    Ok(())
}
pub(super) fn validate_target(layer: &Layer, stack: &[EffectInstance]) -> Result<(), String> {
    let mut pixels_before = false;
    let mut spectrum = false;
    for effect in stack {
        if effect.kind() == EffectKind::AudioSpectrum {
            validate_owner(layer)?;
        }
        if effect.bypassed() || effect.kind() == EffectKind::SliderControl {
            continue;
        }
        if effect.kind() == EffectKind::AudioSpectrum {
            if spectrum || pixels_before {
                return Err("Native Audio Spectrum must be the first enabled pixel effect, with one active instance".into());
            }
            if layer.effects != Effects::default()
                || layer.mask.is_some()
                || !layer.path_masks.is_empty()
                || layer.track_matte.is_some()
            {
                return Err("Native Audio Spectrum currently requires no legacy effects, masks or track matte".into());
            }
            spectrum = true;
        }
        pixels_before = true;
    }
    Ok(())
}

fn audio_source(
    project: &Project,
    layer: &Layer,
    known: &mut BTreeMap<CompositionId, bool>,
    path: &mut BTreeSet<CompositionId>,
) -> Result<bool, String> {
    if let Some((_, metadata)) = layer.content.audio() {
        return if metadata.valid() {
            Ok(true)
        } else {
            Err("Invalid selected source audio metadata".into())
        };
    }
    let Content::Composition { composition, .. } = layer.content else {
        return Ok(false);
    };
    if let Some(result) = known.get(&composition) {
        return Ok(*result);
    }
    if path.len() >= 16 || !path.insert(composition) {
        return Err("Invalid selected audio composition graph".into());
    }
    let comp = project
        .composition_by_id(composition)
        .ok_or("Missing selected audio composition")?;
    let mut found = false;
    for child in &comp.layers {
        found |= audio_source(project, child, known, path)?;
    }
    path.remove(&composition);
    known.insert(composition, found);
    Ok(found)
}
impl Project {
    /// Structurally valid source choices; output switches may legitimately make
    /// a chosen source silent without erasing its dependency identity.
    pub fn spectrum_source_layers(
        &self,
        composition: CompositionId,
    ) -> Result<Vec<LayerId>, String> {
        let comp = self
            .composition_by_id(composition)
            .ok_or("Composition not found")?;
        let mut known = BTreeMap::new();
        let mut result = Vec::new();
        for layer in &comp.layers {
            if audio_source(self, layer, &mut known, &mut BTreeSet::new())? {
                result.push(layer.id);
            }
        }
        Ok(result)
    }
}
pub(super) fn validate_project(project: &Project) -> Result<(), String> {
    let mut known = BTreeMap::new();
    for (comp_id, comp) in project.compositions() {
        for layer in &comp.layers {
            for effect in &layer.effect_stack {
                let Some(source) = effect.audio_spectrum().and_then(|s| s.source) else {
                    continue;
                };
                let selected = comp.layer(source.layer).ok_or_else(|| format!(
                    "Audio Spectrum in composition {comp_id}, layer {}, effect {} refers to missing source layer {}",
                    layer.id, effect.id(), source.layer
                ))?;
                if !audio_source(project, selected, &mut known, &mut BTreeSet::new())? {
                    return Err(format!(
                        "Audio Spectrum source layer {} has no audio dependency",
                        source.layer
                    ));
                }
            }
        }
    }
    Ok(())
}
impl Layer {
    pub fn spectrum_sources(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.effect_stack.iter().filter_map(|effect| {
            effect
                .audio_spectrum()
                .and_then(|settings| settings.source)
                .map(|source| source.layer)
        })
    }
    pub(super) fn remap_spectrum_sources(&mut self, mapping: &BTreeMap<LayerId, LayerId>) {
        for effect in &mut self.effect_stack {
            if let Some(source) = effect.audio_spectrum_mut().and_then(|s| s.source.as_mut()) {
                source.layer = mapping.get(&source.layer).copied().unwrap_or(source.layer);
            }
        }
    }
}
pub(super) fn materialized(project: &Project) -> bool {
    project.compositions().iter().any(|(_, comp)| {
        comp.layers.iter().any(|layer| {
            layer.blend_mode == BlendMode::Difference
                || layer
                    .effect_stack
                    .iter()
                    .any(|effect| effect.audio_spectrum().is_some())
        })
    })
}
pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::Effect {
            edit: EffectEdit::SetAudioSpectrum { .. },
            ..
        } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}
