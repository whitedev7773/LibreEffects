//! Portable project IO. Live documents use absolute paths; saved copies may be relative.
use crate::{project_io, view_state::ProjectViews};
use libre_effects_core::{Content, Project};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub(crate) fn video_paths(project: &Project) -> BTreeSet<String> {
    project
        .compositions()
        .into_iter()
        .flat_map(|(_, c)| c.layers())
        .filter_map(|l| {
            if let Content::Video { path, .. } = l.content() {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn path_string(path: &Path) -> Result<String, String> {
    let value = path.to_str().ok_or("Media paths must be Unicode")?;
    // Canonical Windows paths have a verbatim prefix; normal drive/UNC paths
    // remain readable by FFmpeg and comparable with imported source paths.
    if let Some(unc) = value.strip_prefix("\\\\?\\UNC\\") {
        return Ok(format!("\\\\{unc}"));
    }
    Ok(value.strip_prefix("\\\\?\\").unwrap_or(value).to_owned())
}

fn directory(project_path: &Path) -> Result<PathBuf, String> {
    let parent = project_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let canonical = std::fs::canonicalize(parent)
        .map_err(|e| format!("Cannot open project folder {}: {e}", parent.display()))?;
    Ok(PathBuf::from(path_string(&canonical)?))
}

fn clean_absolute(path: &Path) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    let mut clean = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            _ => clean.push(part.as_os_str()),
        }
    }
    Ok(clean)
}

pub(crate) fn resolve(project: &Project, path: &Path) -> Result<Project, String> {
    let base = directory(path)?;
    project.with_video_paths(|source| {
        let source = PathBuf::from(source);
        // Do not require media to exist: offline projects must remain editable.
        let absolute = if source.is_absolute() {
            source
        } else {
            base.join(source)
        };
        path_string(&clean_absolute(&absolute)?)
    })
}

pub(crate) fn portable(project: &Project, path: &Path) -> Result<Project, String> {
    let base = directory(path)?;
    project.with_video_paths(|source| {
        let source = clean_absolute(Path::new(source))?;
        match source.strip_prefix(&base) {
            Ok(relative) if !relative.as_os_str().is_empty() => {
                Ok(path_string(relative)?.replace('\\', "/"))
            }
            _ => path_string(&source),
        }
    })
}

pub(crate) fn save(
    project: &Project,
    views: &ProjectViews,
    destination: &Path,
) -> Result<(), String> {
    for source in video_paths(project) {
        project_io::protect_source(destination, Path::new(&source))?;
    }
    let copy = portable(project, destination)?;
    project_io::write_project(destination, &views.write(&copy)?)
}

#[derive(Clone)]
pub(crate) struct MediaEntry {
    pub path: String,
    pub references: usize,
    pub offline: bool,
}
pub(crate) fn entries(project: &Project) -> Vec<MediaEntry> {
    let mut entries = std::collections::BTreeMap::<String, usize>::new();
    for (_, comp) in project.compositions() {
        for layer in comp.layers() {
            if let Content::Video { path, .. } = layer.content() {
                *entries.entry(path.clone()).or_default() += 1;
            }
        }
    }
    entries
        .into_iter()
        .map(|(path, references)| MediaEntry {
            offline: !Path::new(&path).is_file(),
            path,
            references,
        })
        .collect()
}

pub(crate) fn replacement(
    original: String,
    path: &Path,
) -> Result<libre_effects_core::MediaReplacement, String> {
    let info = crate::footage::probe(path)?;
    crate::footage::frame_png(&info.path, 0.0, info.width, info.height, 320)?;
    Ok(libre_effects_core::MediaReplacement {
        original,
        path: info.path,
        width: info.width,
        height: info.height,
        duration: info.duration,
        fps: info.source_fps,
    })
}

fn filename_key(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    #[cfg(windows)]
    {
        Some(name.to_lowercase())
    }
    #[cfg(not(windows))]
    {
        Some(name.to_owned())
    }
}

/// Bounded, non-symlink traversal. A filename match is only a candidate;
/// ambiguous names are left for explicit user selection.
pub(crate) fn missing_candidates(
    project: &Project,
    folder: &Path,
) -> Result<(Vec<(String, PathBuf)>, Vec<String>), String> {
    let missing: Vec<_> = entries(project).into_iter().filter(|e| e.offline).collect();
    if missing.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for entry in &missing {
        if let Some(name) = filename_key(Path::new(&entry.path)) {
            *counts.entry(name).or_default() += 1;
        }
    }
    let mut files = std::collections::BTreeMap::<String, Vec<PathBuf>>::new();
    let mut folders = vec![(folder.to_path_buf(), 0)];
    let mut scanned = 0;
    while let Some((directory, depth)) = folders.pop() {
        for item in std::fs::read_dir(&directory)
            .map_err(|e| format!("Cannot search {}: {e}", directory.display()))?
        {
            let item = item.map_err(|e| e.to_string())?;
            scanned += 1;
            if scanned > 10_000 {
                return Err("Folder search exceeds 10,000 entries; choose a smaller folder".into());
            }
            let kind = item.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                if depth >= 16 {
                    return Err("Folder search exceeds 16 levels; choose a smaller folder".into());
                }
                folders.push((item.path(), depth + 1));
            } else if kind.is_file() {
                if let Some(name) = filename_key(&item.path()) {
                    if counts.contains_key(&name) {
                        files.entry(name).or_default().push(item.path());
                    }
                }
            }
        }
    }
    let mut found = Vec::new();
    let mut unresolved = Vec::new();
    for entry in missing {
        let Some(name) = filename_key(Path::new(&entry.path)) else {
            unresolved.push(format!("Invalid name: {}", entry.path));
            continue;
        };
        match files.get(&name) {
            Some(paths) if paths.len() == 1 && counts[&name] == 1 => {
                found.push((entry.path, paths[0].clone()))
            }
            Some(_) => unresolved.push(format!("Ambiguous name; locate manually: {}", entry.path)),
            None => unresolved.push(format!("Not found: {}", entry.path)),
        }
    }
    Ok((found, unresolved))
}

#[derive(Debug)]
pub(crate) struct Collection {
    pub project_path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

pub(crate) fn collect(
    project: &Project,
    views: &ProjectViews,
    parent: &Path,
    mut progress: impl FnMut(usize, usize) -> Result<(), String>,
) -> Result<Collection, String> {
    let sources = video_paths(project);
    progress(0, sources.len())?;
    // The new folder belongs only to this operation. TempDir rolls it back on
    // any error, including a failed copy or project write; existing files stay intact.
    let staging = tempfile::Builder::new()
        .prefix("LibreEffects-collected-")
        .tempdir_in(parent)
        .map_err(|e| format!("Cannot create collection in {}: {e}", parent.display()))?;
    let media = staging.path().join("Media");
    std::fs::create_dir(&media).map_err(|e| e.to_string())?;
    let mut mappings = std::collections::BTreeMap::new();
    let mut copied = std::collections::BTreeMap::<PathBuf, String>::new();
    let mut total_bytes = 0u64;
    for (index, source) in sources.iter().enumerate() {
        let canonical =
            std::fs::canonicalize(source).map_err(|e| format!("Cannot collect {source}: {e}"))?;
        let relative = if let Some(existing) = copied.get(&canonical) {
            existing.clone()
        } else {
            let file = Path::new(source)
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("Invalid media filename: {source}"))?;
            let name = format!("{:04}-{file}", copied.len() + 1);
            let mut input = std::fs::File::open(&canonical)
                .map_err(|e| format!("Cannot read {source}: {e}"))?;
            let before = input.metadata().map_err(|e| e.to_string())?;
            if !before.is_file() {
                return Err(format!("Media is not a regular file: {source}"));
            }
            let destination = media.join(&name);
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map_err(|e| format!("Cannot collect {source}: {e}"))?;
            let mut bytes = 0u64;
            let mut buffer = vec![0; 1024 * 1024];
            loop {
                progress(index, sources.len())?;
                let count = input
                    .read(&mut buffer)
                    .map_err(|e| format!("Cannot read {source}: {e}"))?;
                if count == 0 {
                    break;
                }
                output
                    .write_all(&buffer[..count])
                    .map_err(|e| format!("Cannot copy {source}: {e}"))?;
                bytes += count as u64;
            }
            output
                .sync_all()
                .map_err(|e| format!("Cannot finish {source}: {e}"))?;
            let after = input.metadata().map_err(|e| e.to_string())?;
            if bytes != before.len()
                || before.len() != after.len()
                || before.modified().ok() != after.modified().ok()
            {
                return Err(format!(
                    "Source changed during collection: {source}. Retry after the file is stable."
                ));
            }
            total_bytes = total_bytes
                .checked_add(bytes)
                .ok_or("Collection size overflow")?;
            let relative = format!("Media/{name}");
            copied.insert(canonical, relative.clone());
            relative
        };
        mappings.insert(source.clone(), relative);
        progress(index + 1, sources.len())?;
    }
    let collected = project.with_video_paths(|source| Ok(mappings[source].clone()))?;
    let project_path = staging.path().join("project.lfe.json");
    let json = views.write(&collected)?;
    progress(sources.len(), sources.len())?;
    project_io::write_project(&project_path, &json)?;
    let folder = staging.keep();
    Ok(Collection {
        project_path: folder.join("project.lfe.json"),
        files: copied.len(),
        bytes: total_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor};
    fn scene(source: &Path) -> Project {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Video {
                path: source.to_str().unwrap().into(),
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 20.0,
            height: 20.0,
            name: "Shared footage".into(),
        })
        .unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        e.project().clone()
    }
    #[test]
    fn portable_save_resolves_after_folder_move_and_save_as_preserves_live_paths() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("원본");
        std::fs::create_dir(&original).unwrap();
        let source = original.join("동영상.mp4");
        std::fs::write(&source, b"source bytes").unwrap();
        let p = scene(&source);
        let file = original.join("project.lfe.json");
        save(&p, &Default::default(), &file).unwrap();
        let json = std::fs::read_to_string(&file).unwrap();
        assert!(json.contains("동영상.mp4"));
        assert!(!json.contains("원본"));
        let read = project_io::read_project(&file).unwrap();
        assert_eq!(video_paths(&read), video_paths(&p));
        let other = root.path().join("another.lfe.json");
        save(&read, &Default::default(), &other).unwrap();
        assert_eq!(
            video_paths(&project_io::read_project(&other).unwrap()),
            video_paths(&p)
        );
        let moved = root.path().join("이동");
        std::fs::rename(&original, &moved).unwrap();
        let read = project_io::read_project(&moved.join("project.lfe.json")).unwrap();
        assert_eq!(
            video_paths(&read),
            BTreeSet::from([moved.join("동영상.mp4").to_str().unwrap().into()])
        );
        assert_eq!(
            std::fs::read(video_paths(&read).first().unwrap()).unwrap(),
            b"source bytes"
        );
        assert_eq!(
            video_paths(&p),
            BTreeSet::from([source.to_str().unwrap().into()])
        );
        // An offline project still opens, without resolving against the process CWD.
        std::fs::remove_file(moved.join("동영상.mp4")).unwrap();
        assert!(project_io::read_project(&moved.join("project.lfe.json")).is_ok());
    }
    #[test]
    fn collection_deduplicates_shared_sources_and_rolls_back_failed_copies() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("same.mp4");
        std::fs::write(&source, b"original footage").unwrap();
        let p = scene(&source);
        let result = collect(&p, &Default::default(), root.path(), |_, _| Ok(())).unwrap();
        assert_eq!((result.files, result.bytes), (1, 16));
        let restored = project_io::read_project(&result.project_path).unwrap();
        assert_eq!(video_paths(&restored).len(), 1);
        assert_eq!(
            std::fs::read(video_paths(&restored).first().unwrap()).unwrap(),
            b"original footage"
        );
        assert!(video_paths(&restored).first().unwrap().contains("Media"));
        let missing = p
            .with_video_paths(|_| Ok(root.path().join("missing.mp4").to_str().unwrap().into()))
            .unwrap();
        let before = std::fs::read_dir(root.path()).unwrap().count();
        assert!(
            collect(&missing, &Default::default(), root.path(), |_, _| Ok(()))
                .unwrap_err()
                .contains("missing.mp4")
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), before);
        assert_eq!(std::fs::read(&source).unwrap(), b"original footage");
        assert!(save(&p, &Default::default(), &source).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"original footage");
    }
    #[test]
    fn collection_cancellation_and_duplicate_filenames_preserve_every_source() {
        let root = tempfile::tempdir().unwrap();
        for name in ["first", "second"] {
            std::fs::create_dir(root.path().join(name)).unwrap();
        }
        let first = root.path().join("first/clip.mp4");
        let second = root.path().join("second/clip.mp4");
        std::fs::write(&first, vec![1; 3 * 1024 * 1024]).unwrap();
        std::fs::write(&second, b"different file").unwrap();
        let mut e = Editor::default();
        e.replace_project(scene(&first)).unwrap();
        e.execute(Command::SetContent {
            id: 2,
            content: Content::Video {
                path: second.to_str().unwrap().into(),
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
        })
        .unwrap();
        let before = std::fs::read_dir(root.path()).unwrap().count();
        let mut ticks = 0;
        assert!(
            collect(e.project(), &Default::default(), root.path(), |_, _| {
                ticks += 1;
                if ticks == 3 {
                    Err("Canceled".into())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), before);
        let result = collect(e.project(), &Default::default(), root.path(), |_, _| Ok(())).unwrap();
        assert_eq!(result.files, 2);
        let collected = project_io::read_project(&result.project_path).unwrap();
        let contents: BTreeSet<_> = video_paths(&collected)
            .iter()
            .map(|p| std::fs::read(p).unwrap())
            .collect();
        assert_eq!(
            contents,
            BTreeSet::from([
                std::fs::read(first).unwrap(),
                std::fs::read(second).unwrap()
            ])
        );
    }
    #[test]
    fn missing_search_never_guesses_duplicate_names_and_skips_online_sources() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("one")).unwrap();
        std::fs::create_dir(root.path().join("two")).unwrap();
        let p = scene(&root.path().join("offline/clip.mp4"));
        std::fs::write(root.path().join("one/clip.mp4"), b"one").unwrap();
        let (found, issues) = missing_candidates(&p, root.path()).unwrap();
        assert_eq!((found.len(), issues.len()), (1, 0));
        std::fs::write(root.path().join("two/clip.mp4"), b"two").unwrap();
        let (found, issues) = missing_candidates(&p, root.path()).unwrap();
        assert!(found.is_empty());
        assert!(issues[0].contains("Ambiguous"));
        let online = scene(&root.path().join("one/clip.mp4"));
        assert!(
            missing_candidates(&online, root.path())
                .unwrap()
                .0
                .is_empty()
        );
    }
    #[test]
    #[ignore = "requires FFmpeg; verifies collected and relocated footage renders identically"]
    fn collected_moved_project_renders_without_the_original_footage() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("clip.mp4");
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Source".into(),
            width: 64,
            height: 64,
            fps: 24,
            duration: 8,
        })
        .unwrap();
        e.execute(Command::AddRectangle).unwrap();
        crate::video_export::export_video(
            e.project(),
            0..8,
            crate::video_export::VideoPreset::H264,
            &source,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let info = crate::footage::probe(&source).unwrap();
        let mut e = Editor::default();
        e.execute(Command::ConfigureCompositionRate {
            name: "Master".into(),
            width: 64,
            height: 64,
            fps: "30000/1001".parse().unwrap(),
            duration: 10,
            display_start: 0,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Video {
                path: info.path,
                duration: info.duration,
                source_fps: info.source_fps,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 64.0,
            height: 64.0,
            name: "Footage".into(),
        })
        .unwrap();
        e.execute(Command::Precompose {
            layers: vec![1],
            name: "Nested".into(),
        })
        .unwrap();
        let renderer = crate::rendering::Renderer::new();
        let before = renderer.render(e.project(), 3, 64).unwrap();
        let result = collect(e.project(), &Default::default(), root.path(), |_, _| Ok(())).unwrap();
        let moved = root.path().join("moved package");
        std::fs::rename(result.project_path.parent().unwrap(), &moved).unwrap();
        std::fs::remove_file(source).unwrap();
        let restored = project_io::read_project(&moved.join("project.lfe.json")).unwrap();
        crate::footage::clear_cache();
        assert_eq!(renderer.render(&restored, 3, 64).unwrap(), before);
        let output = root.path().join("render.mp4");
        crate::video_export::export_video(
            &restored,
            0..5,
            crate::video_export::VideoPreset::H264,
            &output,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        assert!(output.metadata().unwrap().len() > 0);
    }
}
