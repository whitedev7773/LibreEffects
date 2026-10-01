//! Shared source interpretation; layer timing and animation stay independent.
use super::*;
#[cfg(test)]
#[path = "footage_interpretation_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlphaInterpretation {
    #[default]
    Straight,
    Ignore,
    Premultiplied {
        matte: u32,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootageInterpretation {
    pub fps: Option<FrameRate>,
    pub alpha: AlphaInterpretation,
    pub invert_alpha: bool,
}
impl FootageInterpretation {
    pub(super) fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub(super) fn validate(self, content: &Content) -> Result<(), String> {
        if self
            .fps
            .is_some_and(|fps| !fps.valid() || content.footage_timing().is_none())
            || matches!(self.alpha, AlphaInterpretation::Premultiplied { matte } if matte > 0xffffff)
            || (!matches!(
                content,
                Content::Image { .. } | Content::Video { .. } | Content::ImageSequence { .. }
            ) && !self.is_default())
        {
            return Err("Invalid footage interpretation".into());
        }
        if self.duration(content).is_some_and(|s| s > 86400.0) {
            return Err("Interpreted footage cannot exceed 24 hours".into());
        }
        Ok(())
    }
    pub fn frame_rate(self, content: &Content) -> Option<FrameRate> {
        match content {
            Content::Video { source_fps, .. } => self.fps.or_else(|| native_rate(*source_fps)),
            Content::ImageSequence { fps, .. } => Some(self.fps.unwrap_or(*fps)),
            _ => None,
        }
    }
    pub fn duration(self, content: &Content) -> Option<f64> {
        let (duration, source_fps) = content.footage_timing()?;
        Some(match self.fps {
            Some(fps) => duration * source_fps / fps.as_f64(),
            None => duration,
        })
    }
}
fn native_rate(value: f64) -> Option<FrameRate> {
    for n in [24_000, 30_000, 60_000, 120_000, 240_000] {
        if (value - f64::from(n) / 1001.0).abs() < 0.0000001 {
            return FrameRate::new(n, 1001).ok();
        }
    }
    if !value.is_finite() || !(1.0..=240.0).contains(&value) {
        return None;
    }
    FrameRate::new((value * 1_000_000.0).round() as u32, 1_000_000).ok()
}
impl Layer {
    pub fn footage_interpretation(&self) -> FootageInterpretation {
        self.footage_interpretation
    }
    /// Sample time in the original file's clock, for decoding only.
    pub fn video_decode_time(&self, frame: Frame, fps: impl Into<FrameRate>) -> Option<f64> {
        let seconds = self.video_time(frame, fps)?;
        let Content::Video { source_fps, .. } = self.content else {
            return None;
        };
        Some(match self.footage_interpretation.fps {
            Some(fps) => seconds * fps.as_f64() / source_fps,
            None => seconds,
        })
    }
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    Some(match command {
        Command::InterpretAsset {
            asset,
            interpretation,
        } => (|| {
            let a = state
                .project
                .asset_library
                .assets
                .get_mut(asset)
                .ok_or("Asset not found")?;
            interpretation.validate(a.content())?;
            a.interpretation = *interpretation;
            for layer in state.project.compositions_mut().flat_map(|c| &mut c.layers) {
                if layer.asset == Some(*asset) {
                    layer.footage_interpretation = *interpretation;
                }
            }
            Ok(())
        })(),
        Command::CompositionFromAsset(asset) => (|| {
            let a = state
                .project
                .asset_library
                .assets
                .get(asset)
                .ok_or("Asset not found")?
                .clone();
            if a.width().fract() != 0.0 || a.height().fract() != 0.0 {
                return Err("Source composition requires whole-pixel source dimensions".into());
            }
            let fps = a
                .interpretation
                .frame_rate(a.content())
                .unwrap_or(state.project.composition.fps);
            let audio_only = matches!(a.content(), Content::Audio { .. });
            let (width, height) = if audio_only {
                (
                    state.project.composition.width,
                    state.project.composition.height,
                )
            } else {
                (a.width() as u32, a.height() as u32)
            };
            // A still uses the current composition's elapsed duration, at its FPS.
            let duration = a
                .interpretation
                .duration(a.content())
                .map(|s| (s * fps.as_f64() - 1e-7).ceil().max(1.0) as Frame)
                .unwrap_or(state.project.composition.duration);
            super::apply(state, Command::NewComposition)?;
            super::apply(
                state,
                Command::ConfigureCompositionRate {
                    name: a.name().to_owned(),
                    width,
                    height,
                    fps,
                    duration,
                    display_start: 0,
                },
            )?;
            state.project.composition.background_color = 0x000000;
            state.project.composition.hide_shy = false;
            super::apply(
                state,
                Command::MoveProjectItem {
                    item: ProjectItem::Composition(state.project.composition_id),
                    folder: a.folder(),
                },
            )?;
            super::apply(
                state,
                Command::AddAssetLayer {
                    asset: *asset,
                    frame: 0,
                },
            )
        })(),
        _ => return None,
    })
}
