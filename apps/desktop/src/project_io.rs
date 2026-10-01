use std::{
    io::{Read, Write},
    path::Path,
};

use libre_effects_core::{Content, FrameRounding, Project};

const MAX_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) fn protect_source(destination: &Path, source: &Path) -> Result<(), String> {
    if destination.exists() && source.exists() {
        if same_file::is_same_file(destination, source).map_err(|e| e.to_string())? {
            return Err(format!(
                "Output would replace a source file: {}. Choose another destination.",
                source.display()
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_render(
    project: &Project,
    destination: &Path,
    range: &std::ops::Range<u32>,
) -> Result<(), String> {
    let comp = project.composition();
    if range.is_empty() || range.end > comp.duration() {
        return Err("Choose a non-empty frame range inside the composition".into());
    }
    if comp.width() as u64 * comp.height() as u64 > 33_554_432 {
        return Err("Rendering supports up to 32 megapixels per frame".into());
    }
    for layer in project
        .compositions()
        .into_iter()
        .flat_map(|(_, comp)| comp.layers())
    {
        if let Content::Video { path, .. } = layer.content() {
            protect_source(destination, Path::new(path))?;
        }
    }
    validate_sources(
        project,
        project.active_composition_id(),
        range.clone(),
        &mut Default::default(),
    )
}

fn validate_sources(
    project: &Project,
    id: libre_effects_core::CompositionId,
    range: std::ops::Range<u32>,
    seen: &mut std::collections::BTreeSet<(u64, u32, u32)>,
) -> Result<(), String> {
    if seen.len() >= 4096 {
        return Err(
            "Too many nested time ranges; simplify the composition before rendering".into(),
        );
    }
    if !seen.insert((id, range.start, range.end)) {
        return Ok(());
    }
    let comp = project
        .composition_by_id(id)
        .ok_or("Missing source composition")?;
    for layer in comp
        .layers()
        .iter()
        .filter(|l| comp.layer_enabled(l, false))
    {
        let start = range.start.max(layer.in_frame());
        let end = range.end.min(layer.out_frame(comp.duration()));
        if start >= end {
            continue;
        }
        match layer.content() {
            Content::Video { path, .. } if !Path::new(path).is_file() => {
                return Err(format!(
                    "Footage offline in {}: {path}. Relink before rendering.",
                    comp.name()
                ));
            }
            Content::Composition {
                composition,
                start_frame,
            } => {
                let source = project
                    .composition_by_id(*composition)
                    .ok_or("Missing source composition")?;
                let last = i64::from(end - 1)
                    .checked_sub(*start_frame)
                    .ok_or("Nested source time overflow")?;
                if last < 0 {
                    continue;
                }
                let first = i64::from(start)
                    .checked_sub(*start_frame)
                    .ok_or("Nested source time overflow")?
                    .max(0) as u64;
                let first = comp
                    .fps()
                    .convert_frames(first, source.fps(), FrameRounding::Floor)
                    .ok_or("Nested source time overflow")?;
                let end = comp
                    .fps()
                    .convert_frames(last as u64, source.fps(), FrameRounding::Floor)
                    .ok_or("Nested source time overflow")?
                    .saturating_add(1)
                    .min(u64::from(source.duration()));
                if first < end {
                    validate_sources(project, *composition, first as u32..end as u32, seen)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn read_project(path: &Path) -> Result<Project, String> {
    let json = read_json(path)?;
    parse_project(&json)
}
pub(crate) fn read_editor_project(
    path: &Path,
) -> Result<(Project, crate::view_state::ProjectViews), String> {
    let json = read_json(path)?;
    let project = parse_project(&json)?;
    let views = crate::view_state::ProjectViews::read(&json, &project);
    Ok((project, views))
}
fn parse_project(json: &str) -> Result<Project, String> {
    let project = Project::from_json(json)?;
    crate::rendering::validate_images(&project)?;
    Ok(project)
}
fn read_json(path: &Path) -> Result<String, String> {
    let mut json = String::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_BYTES + 1)
        .read_to_string(&mut json)
        .map_err(|error| error.to_string())?;
    if json.len() as u64 > MAX_BYTES {
        return Err("Project exceeds 256 MiB".into());
    }
    Ok(json)
}

pub(crate) fn write_project(path: &Path, json: &str) -> Result<(), String> {
    validate_project_size(json)?;
    write_bytes(path, json.as_bytes())
}
pub(crate) fn validate_project_size(json: &str) -> Result<(), String> {
    if json.len() as u64 > MAX_BYTES {
        return Err("Project exceeds 256 MiB".into());
    }
    Ok(())
}
pub(crate) fn write_bytes(path: &Path, data: &[u8]) -> Result<(), String> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Complete the temporary file before replacing the destination on the same filesystem.
    let mut temporary =
        tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    temporary
        .write_all(data)
        .map_err(|error| error.to_string())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temporary.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor};

    #[test]
    fn preflight_skips_guide_and_non_solo_media_but_still_protects_their_source() {
        use libre_effects_core::LayerSwitch;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("guide.mp4");
        std::fs::write(&source, b"original").unwrap();
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Video {
                path: source.to_string_lossy().into_owned(),
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 16.0,
            height: 16.0,
            name: "Guide video".into(),
        })
        .unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Guide,
            enabled: true,
        })
        .unwrap();
        assert!(
            validate_render(e.project(), &source, &(0..5))
                .unwrap_err()
                .contains("replace a source")
        );
        std::fs::remove_file(&source).unwrap();
        let output = dir.path().join("output.mp4");
        validate_render(e.project(), &output, &(0..5)).unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Guide,
            enabled: false,
        })
        .unwrap();
        assert!(
            validate_render(e.project(), &output, &(0..5))
                .unwrap_err()
                .contains("offline")
        );
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 2,
            switch: LayerSwitch::Solo,
            enabled: true,
        })
        .unwrap();
        validate_render(e.project(), &output, &(0..5)).unwrap();
    }

    #[test]
    fn render_preflight_protects_sources_including_hardlinks_and_missing_media() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        let alias = dir.path().join("alias.mp4");
        std::fs::write(&source, b"irreplaceable source").unwrap();
        std::fs::hard_link(&source, &alias).unwrap();
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Video {
                path: source.to_string_lossy().into_owned(),
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 16.0,
            height: 16.0,
            name: "Source".into(),
        })
        .unwrap();
        for destination in [&source, &alias] {
            assert!(
                validate_render(e.project(), destination, &(0..5))
                    .unwrap_err()
                    .contains("replace a source")
            );
        }
        assert_eq!(std::fs::read(&source).unwrap(), b"irreplaceable source");
        let output = dir.path().join("output.mp4");
        validate_render(e.project(), &output, &(0..5)).unwrap();
        e.execute(Command::NewComposition).unwrap();
        assert!(
            validate_render(e.project(), &source, &(0..5))
                .unwrap_err()
                .contains("replace a source")
        );
        std::fs::remove_file(&source).unwrap();
        // Missing sources in an unrelated composition do not block this render.
        validate_render(e.project(), &output, &(0..5)).unwrap();
        e.activate_composition(1).unwrap();
        assert!(
            validate_render(e.project(), &output, &(0..5))
                .unwrap_err()
                .contains("offline")
        );
        assert!(!output.exists());
    }

    #[test]
    fn opening_corrupt_embedded_image_reports_error() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: libre_effects_core::Content::Image { png: "YWJj".into() },
                width: 32.0,
                height: 32.0,
                name: "Broken image".into(),
            })
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.lfe.json");
        write_project(&path, &editor.project().to_json().unwrap()).unwrap();
        assert!(read_project(&path).unwrap_err().contains("Invalid image"));
        editor.execute(Command::NewComposition).unwrap();
        write_project(&path, &editor.project().to_json().unwrap()).unwrap();
        assert!(read_project(&path).unwrap_err().contains("Invalid image"));
    }
    #[test]
    fn preflight_checks_nested_footage_only_in_the_requested_time_range() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = Editor::default();
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::AddContent {
            content: Content::Video {
                path: dir
                    .path()
                    .join("missing.mp4")
                    .to_string_lossy()
                    .into_owned(),
                duration: 1.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 16.0,
            height: 16.0,
            name: "Offline".into(),
        })
        .unwrap();
        e.activate_composition(1).unwrap();
        e.execute(Command::AddCompositionLayer {
            composition: 2,
            frame: 60,
        })
        .unwrap();
        let output = dir.path().join("output.mp4");
        validate_render(e.project(), &output, &(0..60)).unwrap();
        assert!(
            validate_render(e.project(), &output, &(60..61))
                .unwrap_err()
                .contains("offline")
        );
        validate_render(e.project(), &output, &(90..150)).unwrap();
    }

    #[test]
    fn save_replaces_existing_project_and_open_recovers_edits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.lfe.json");
        write_project(&path, &Project::default().to_json().unwrap()).unwrap();
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        write_project(&path, &editor.project().to_json().unwrap()).unwrap();
        assert_eq!(&read_project(&path).unwrap(), editor.project());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn oversized_save_leaves_existing_file_intact() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.lfe.json");
        let original = Project::default().to_json().unwrap();
        write_project(&path, &original).unwrap();
        assert!(write_project(&path, &" ".repeat(MAX_BYTES as usize + 1)).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }
}
