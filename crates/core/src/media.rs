//! Source-path rewrites are document IO operations, independent of layer locking.
use super::*;

#[derive(Clone, Debug)]
pub struct MediaReplacement {
    pub original: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub fps: f64,
}

pub(super) fn relink(
    state: &mut Snapshot,
    replacements: Vec<MediaReplacement>,
) -> Result<(), String> {
    if replacements.is_empty() {
        return Err("No matching media to relink".into());
    }
    let mut sources = BTreeMap::new();
    for replacement in replacements {
        if replacement.original.is_empty()
            || sources
                .insert(replacement.original.clone(), replacement)
                .is_some()
        {
            return Err("Source paths must be unique".into());
        }
    }
    let mut found = BTreeSet::new();
    let mut update = |content: &mut Content, width: f64, height: f64| -> Result<(), String> {
        for path in content.linked_paths() {
            if let Some(source) = sources.get(path) {
                if width != f64::from(source.width) || height != f64::from(source.height) {
                    return Err("Relink requires matching dimensions in every instance".into());
                }
                found.insert(source.original.clone());
            }
        }
        if let Content::Video {
            path,
            duration,
            source_fps,
            ..
        } = content
        {
            if let Some(source) = sources.get(path) {
                *path = source.path.clone();
                *duration = source.duration;
                *source_fps = source.fps;
            }
        } else {
            for path in content.linked_paths_mut() {
                if let Some(source) = sources.get(path) {
                    *path = source.path.clone();
                }
            }
        }
        Ok(())
    };
    for a in state.project.asset_library.assets.values_mut() {
        let (width, height) = (a.width(), a.height());
        update(&mut a.content, width, height)?;
    }
    for l in state.project.compositions_mut().flat_map(|c| &mut c.layers) {
        update(&mut l.content, l.width, l.height)?;
    }
    if found.len() != sources.len() {
        return Err("A source to relink is no longer in this project".into());
    }
    Ok(())
}

impl Project {
    /// Return a validated copy with each distinct linked media path mapped once.
    /// Used by the host to resolve project-relative files or collect dependencies.
    /// The live editor/history must retain absolute paths so Save As is reversible.
    pub fn with_video_paths(
        &self,
        mut map: impl FnMut(&str) -> Result<String, String>,
    ) -> Result<Self, String> {
        let mut copy = self.clone();
        let mut paths: BTreeMap<String, String> = BTreeMap::new();
        let mut manifests = BTreeMap::<usize, std::sync::Arc<Vec<String>>>::new();
        let mut rewrite = |content: &mut Content| -> Result<(), String> {
            if let Content::ImageSequence { frames, .. } = content {
                let pointer = std::sync::Arc::as_ptr(frames) as usize;
                if let Some(mapped) = manifests.get(&pointer) {
                    *frames = mapped.clone();
                    return Ok(());
                }
                let mut mapped = Vec::with_capacity(frames.len());
                for path in frames.iter() {
                    let value = if let Some(value) = paths.get(path) {
                        value.clone()
                    } else {
                        let value = map(path)?;
                        paths.insert(path.clone(), value.clone());
                        value
                    };
                    mapped.push(value);
                }
                let mapped = std::sync::Arc::new(mapped);
                manifests.insert(pointer, mapped.clone());
                *frames = mapped;
                return Ok(());
            }

            for path in content.linked_paths_mut() {
                let value = if let Some(value) = paths.get(path) {
                    value.clone()
                } else {
                    let value = map(path)?;
                    paths.insert(path.clone(), value.clone());
                    value
                };
                *path = value;
            }
            Ok(())
        };
        for asset in copy.asset_library.assets.values_mut() {
            rewrite(&mut asset.content)?;
        }
        for layer in copy.compositions_mut().flat_map(|c| &mut c.layers) {
            rewrite(&mut layer.content)?;
        }
        if paths.iter().any(|(old, new)| old != new) {
            copy.version = copy.version.max(15);
        }
        copy.validate()?;
        Ok(copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_rewrites_cover_inactive_compositions_and_do_not_edit_source_history() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Video {
                path: "C:/source/movie.mp4".into(),
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 20.0,
            height: 20.0,
            name: "Footage".into(),
        })
        .unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        e.execute(Command::ToggleLocked(2)).unwrap();
        let source = e.project().clone();
        let mut count = 0;
        let collected = source
            .with_video_paths(|path| {
                assert_eq!(path, "C:/source/movie.mp4");
                count += 1;
                Ok("Media/movie.mp4".into())
            })
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(collected.version, 22);
        assert_eq!(e.project(), &source);
        for (_, comp) in collected.compositions() {
            assert!(
                matches!(comp.layers()[0].content(), Content::Video { path, .. } if path == "Media/movie.mp4")
            );
        }
        assert_eq!(
            Project::from_json(&collected.to_json().unwrap()).unwrap(),
            collected
        );
        assert!(source.with_video_paths(|_| Ok("".into())).is_err());
        assert!(
            source
                .with_video_paths(|_| Err("Copy failed".into()))
                .is_err()
        );
        assert_eq!(e.project(), &source);
        e.undo();
        assert!(!e.project().composition().layer(2).unwrap().locked());
    }
    #[test]
    fn relink_is_shared_across_compositions_undoable_and_atomic() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Video {
                path: "old.mp4".into(),
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: VideoPlayback {
                    source_in: 1.0,
                    speed: 0.5,
                },
            },
            width: 20.0,
            height: 20.0,
            name: "Movie".into(),
        })
        .unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        e.execute(Command::ToggleLocked(2)).unwrap();
        let before = e.project().clone();
        let replacement = MediaReplacement {
            original: "old.mp4".into(),
            path: "new.mp4".into(),
            width: 20,
            height: 20,
            duration: 5.0,
            fps: 30.0,
        };
        e.execute(Command::RelinkMedia(vec![replacement.clone()]))
            .unwrap();
        let after = e.project().clone();
        for (_, c) in after.compositions() {
            assert!(
                matches!(c.layers()[0].content(), Content::Video { path, playback, .. } if path == "new.mp4" && playback.source_in == 1.0 && playback.speed == 0.5)
            );
        }
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
        e.undo();
        for bad in [
            MediaReplacement {
                width: 21,
                ..replacement.clone()
            },
            MediaReplacement {
                fps: f64::NAN,
                ..replacement.clone()
            },
            MediaReplacement {
                original: "missing.mp4".into(),
                ..replacement.clone()
            },
        ] {
            assert!(e.execute(Command::RelinkMedia(vec![bad])).is_err());
            assert_eq!(e.project(), &before);
        }
        assert!(
            e.execute(Command::RelinkMedia(vec![replacement.clone(), replacement]))
                .is_err()
        );
        assert_eq!(e.project(), &before);
    }
}
