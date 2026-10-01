use super::*;
#[cfg(test)]
#[path = "image_sequence_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissingFramePolicy {
    #[default]
    Error,
    Hold,
    Transparent,
}
impl Content {
    /// Physical duration and rate before source interpretation.
    pub fn footage_timing(&self) -> Option<(f64, f64)> {
        match self {
            Self::Video {
                duration,
                source_fps,
                ..
            } => Some((*duration, *source_fps)),
            Self::Audio { audio, .. } => Some((audio.duration, f64::from(audio.sample_rate))),
            Self::ImageSequence { frames, fps, .. } => {
                Some((frames.len() as f64 / fps.as_f64(), fps.as_f64()))
            }
            _ => None,
        }
    }
    pub fn footage_origin(&self) -> Option<i64> {
        match self {
            Self::Video { start_frame, .. }
            | Self::Audio { start_frame, .. }
            | Self::ImageSequence { start_frame, .. } => Some(*start_frame),
            _ => None,
        }
    }
    pub fn linked_paths(&self) -> &[String] {
        match self {
            Self::Video { path, .. } | Self::Audio { path, .. } => std::slice::from_ref(path),
            Self::ImageSequence { frames, .. } => frames,
            _ => &[],
        }
    }
    pub(super) fn linked_paths_mut(&mut self) -> &mut [String] {
        match self {
            Self::Video { path, .. } | Self::Audio { path, .. } => std::slice::from_mut(path),
            Self::ImageSequence { frames, .. } => std::sync::Arc::make_mut(frames).as_mut_slice(),
            _ => &mut [],
        }
    }
}
impl Layer {
    pub fn sequence_frame(&self, frame: Frame, fps: impl Into<FrameRate>) -> Option<usize> {
        let Content::ImageSequence { frames, .. } = &self.content else {
            return None;
        };
        let seconds = self.video_time(frame, fps)?;
        let rate = self.footage_interpretation.frame_rate(&self.content)?;
        let index = (seconds * rate.as_f64() + 1e-7).floor() as usize;
        (index < frames.len()).then_some(index)
    }
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let asset = match command {
        Command::SetSequenceMissing { asset, .. } | Command::RelinkSequence { asset, .. } => *asset,
        _ => return None,
    };
    Some((|| {
        let a = state
            .project
            .asset_library
            .assets
            .get_mut(&asset)
            .ok_or("Asset not found")?;
        let Content::ImageSequence {
            frames, missing, ..
        } = &mut a.content
        else {
            return Err("Select an image sequence".into());
        };
        match command {
            Command::SetSequenceMissing {
                missing: policy, ..
            } => *missing = *policy,
            Command::RelinkSequence { frames: paths, .. } => {
                if paths.len() != frames.len() {
                    return Err("Relink must preserve the sequence frame count".into());
                }
                *frames = paths.clone();
            }
            _ => unreachable!(),
        }
        let (frames, missing) = (frames.clone(), *missing);
        for layer in state.project.compositions_mut().flat_map(|c| &mut c.layers) {
            if layer.asset == Some(asset) {
                let Content::ImageSequence {
                    frames: paths,
                    missing: policy,
                    ..
                } = &mut layer.content
                else {
                    return Err("Invalid sequence instance".into());
                };
                *paths = frames.clone();
                *policy = missing;
            }
        }
        Ok(())
    })())
}

/// Replace shared manifests in a serialization-only copy, before JSON allocation.
pub(super) fn compact(project: &mut Project) -> BTreeMap<String, std::sync::Arc<Vec<String>>> {
    let mut manifests = BTreeMap::new();
    let mut ids = BTreeMap::new();
    let mut visit = |content: &mut Content| {
        if let Content::ImageSequence { frames, .. } = content {
            let id = ids.entry(frames.clone()).or_insert_with(|| {
                let id = format!("sequence-{}", manifests.len() + 1);
                manifests.insert(id.clone(), frames.clone());
                id
            });
            *frames = std::sync::Arc::new(vec![id.clone()]);
        }
    };
    for a in project.asset_library.assets.values_mut() {
        visit(&mut a.content);
    }
    for l in project.compositions_mut().flat_map(|c| &mut c.layers) {
        visit(&mut l.content);
    }
    manifests
}
