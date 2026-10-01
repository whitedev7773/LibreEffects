//! Per-instance audio automation; curves use composition time, not source time.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AudioParam {
    LeftLevel,
    RightLevel,
    Pan,
    Fade,
}
impl AudioParam {
    pub const ALL: [Self; 4] = [Self::LeftLevel, Self::RightLevel, Self::Pan, Self::Fade];
    pub fn label(self) -> &'static str {
        match self {
            Self::LeftLevel => "Audio Left (dB)",
            Self::RightLevel => "Audio Right (dB)",
            Self::Pan => "Audio Pan (%)",
            Self::Fade => "Audio Fade (%)",
        }
    }
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::LeftLevel | Self::RightLevel => (-192.0, 12.0),
            Self::Pan => (-100.0, 100.0),
            Self::Fade => (0.0, 100.0),
        }
    }
    fn accepts(self, value: f64) -> bool {
        let (min, max) = self.bounds();
        value.is_finite() && (min..=max).contains(&value)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct AudioControls {
    pub enabled: bool,
    pub parameters: BTreeMap<AudioParam, AnimatedProperty>,
}
impl Default for AudioControls {
    fn default() -> Self {
        Self {
            enabled: true,
            parameters: AudioParam::ALL
                .into_iter()
                .map(|p| {
                    (
                        p,
                        AnimatedProperty::new(if p == AudioParam::Fade { 100.0 } else { 0.0 }),
                    )
                })
                .collect(),
        }
    }
}
impl AudioControls {
    pub fn is_default(&self) -> bool {
        self.enabled
            && self.parameters.len() == AudioParam::ALL.len()
            && AudioParam::ALL.into_iter().all(|p| {
                self.parameters.get(&p).is_some_and(|t| {
                    t.keys.is_empty() && t.value == if p == AudioParam::Fade { 100.0 } else { 0.0 }
                })
            })
    }
}
impl Layer {
    pub fn can_audio(&self) -> bool {
        self.content.audio().is_some() || matches!(self.content, Content::Composition { .. })
    }
    pub fn audio_enabled(&self) -> bool {
        self.audio_controls.enabled
    }
    /// Row-major stereo matrix. Pan moves the opposite channel with sine/cosine
    /// gains; center is exactly identity and full pan sums both channels to one.
    /// Phase cancellation and boosts on correlated stereo are intentional.
    pub fn audio_matrix(&self, frame: f64) -> [f64; 4] {
        if !self.audio_enabled() {
            return [0.0; 4];
        }
        let sample = |p: AudioParam| {
            let (min, max) = p.bounds();
            self.audio_controls.parameters[&p]
                .sample(frame)
                .clamp(min, max)
        };
        let level = |p| {
            let db = sample(p);
            if db <= -192.0 {
                0.0
            } else {
                10.0_f64.powf(db / 20.0)
            }
        };
        let fade = sample(AudioParam::Fade) / 100.0;
        let (l, r) = (
            level(AudioParam::LeftLevel) * fade,
            level(AudioParam::RightLevel) * fade,
        );
        let pan = sample(AudioParam::Pan) / 100.0;
        let (s, c) = (pan.abs() * std::f64::consts::FRAC_PI_2).sin_cos();
        let c = if pan.abs() == 1.0 { 0.0 } else { c };
        if pan >= 0.0 {
            [l * c, 0.0, l * s, r]
        } else {
            [l, r * s, 0.0, r * c]
        }
    }
}
pub(super) fn validate(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    let audio = &layer.audio_controls;
    if !audio.is_default() && (version < 26 || !layer.can_audio()) {
        return Err(
            "Audio controls require an audio/precomposition layer and project version 26".into(),
        );
    }
    if audio.parameters.len() != AudioParam::ALL.len() {
        return Err("Invalid audio controls".into());
    }
    for p in AudioParam::ALL {
        let track = audio.parameters.get(&p).ok_or("Missing audio parameter")?;
        if !p.accepts(track.value)
            || track.keys.len() > 10000
            || track
                .keys
                .iter()
                .any(|(f, k)| *f >= duration || !p.accepts(k.value) || !k.interpolation.valid())
        {
            return Err("Invalid audio parameter or keyframe".into());
        }
    }
    Ok(())
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let id = match command {
        Command::SetAudioEnabled { id, .. }
        | Command::EditAudio { id, .. }
        | Command::FadeAudio { id, .. } => *id,
        _ => return None,
    };
    Some((|| {
        let duration = state.project.composition.duration;
        let layer = editing::editable(state, id)?;
        if !layer.can_audio() {
            return Err("Select a layer with audio or a precomposition".into());
        }
        match command {
            Command::SetAudioEnabled { enabled, .. } => layer.audio_controls.enabled = *enabled,
            Command::EditAudio {
                parameter, edit, ..
            } => time_remap::edit_track(
                layer
                    .audio_controls
                    .parameters
                    .get_mut(parameter)
                    .ok_or("Missing audio parameter")?,
                duration,
                edit,
                |value| parameter.accepts(value),
            )?,
            Command::FadeAudio {
                start,
                end,
                fade_in,
                ..
            } => {
                if *start >= *end || *start < layer.in_frame || *end >= layer.out_frame(duration) {
                    return Err("Fade needs two distinct frames inside the layer".into());
                }
                let track = layer
                    .audio_controls
                    .parameters
                    .get_mut(&AudioParam::Fade)
                    .unwrap();
                track.keys.retain(|f, _| *f < *start || *f > *end);
                for (frame, value) in [
                    (*start, if *fade_in { 0.0 } else { 100.0 }),
                    (*end, if *fade_in { 100.0 } else { 0.0 }),
                ] {
                    track.keys.insert(
                        frame,
                        Keyframe {
                            value,
                            interpolation: Interpolation::Linear,
                        },
                    );
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    })())
}

#[cfg(test)]
#[path = "audio_controls_tests.rs"]
mod tests;
