use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

use libre_effects_core::{Content, FrameRounding, Project};

const MAX_BYTES: u64 = libre_effects_core::project_file::MAX_FILE_BYTES as u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectFormat {
    Lep,
    LegacyJson,
}

#[derive(Debug)]
pub(crate) struct OpenedProject {
    pub project: Project,
    pub views: crate::view_state::ProjectViews,
    pub format: ProjectFormat,
}

pub(crate) fn protect_source(destination: &Path, source: &Path) -> Result<(), String> {
    // Expected-but-missing sequence frames are sources too: exporting there would
    // silently replace a gap and change subsequent previews or renders.
    fn identity(path: &Path) -> Result<String, String> {
        let absolute = crate::media_io::clean_absolute(path)?;
        let normalized = std::fs::canonicalize(&absolute)
            .or_else(|_| {
                std::fs::canonicalize(absolute.parent().unwrap_or(Path::new(".")))
                    .map(|parent| parent.join(absolute.file_name().unwrap_or_default()))
            })
            .unwrap_or(absolute);
        let value = crate::media_io::path_string(&normalized)?;
        #[cfg(windows)]
        let value = value.to_lowercase();
        Ok(value)
    }
    if identity(destination)? == identity(source)? {
        return Err(format!(
            "Output would replace a source path: {}. Choose another destination.",
            source.display()
        ));
    }
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
    validate_render_with(
        project,
        destination,
        range,
        &Default::default(),
        &mut |_, _| {},
    )
}

/// Visits only output-visible layers (including required mattes and nested ranges).
/// Font diagnostics and source validation deliberately share this traversal.
pub(crate) fn validate_render_with(
    project: &Project,
    destination: &Path,
    range: &std::ops::Range<u32>,
    cancel: &std::sync::atomic::AtomicBool,
    visit: &mut impl FnMut(&libre_effects_core::Composition, &libre_effects_core::Layer),
) -> Result<(), String> {
    crate::video_decoder::check_cancel(cancel)?;
    let comp = project.composition();
    if range.is_empty() || range.end > comp.duration() {
        return Err("Choose a non-empty frame range inside the composition".into());
    }
    if comp.width() as u64 * comp.height() as u64 > 33_554_432 {
        return Err("Rendering supports up to 32 megapixels per frame".into());
    }
    for path in crate::media_io::video_paths(project) {
        crate::video_decoder::check_cancel(cancel)?;
        protect_source(destination, Path::new(&path))?;
    }
    validate_sources(
        project,
        project.active_composition_id(),
        range.clone(),
        &mut Default::default(),
        cancel,
        visit,
    )
}

fn validate_sources(
    project: &Project,
    id: libre_effects_core::CompositionId,
    range: std::ops::Range<u32>,
    seen: &mut std::collections::BTreeSet<(u64, u32, u32)>,
    cancel: &std::sync::atomic::AtomicBool,
    visit: &mut impl FnMut(&libre_effects_core::Composition, &libre_effects_core::Layer),
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
    let mut pending: Vec<_> = comp
        .layers()
        .iter()
        .filter(|l| comp.layer_enabled(l, false))
        .map(|l| (l.id(), range.clone()))
        .collect();
    let mut checked = std::collections::BTreeSet::new();
    while let Some((layer_id, range)) = pending.pop() {
        crate::video_decoder::check_cancel(cancel)?;
        let layer = comp.layer(layer_id).ok_or("Missing matte source")?;
        let start = range.start.max(layer.in_frame());
        let end = range.end.min(layer.out_frame(comp.duration()));
        if start >= end || !checked.insert((layer_id, start, end)) {
            continue;
        }
        if checked.len() > 4096 {
            return Err(
                "Too many matte time ranges; simplify the composition before rendering".into(),
            );
        }
        visit(comp, layer);
        if let Some(matte) = layer.track_matte() {
            pending.push((matte.source, start..end));
        }
        match layer.content() {
            Content::Video { path, .. } if !Path::new(path).is_file() => {
                return Err(format!(
                    "Footage offline in {}: {path}. Relink before rendering.",
                    comp.name()
                ));
            }
            Content::ImageSequence {
                frames, missing, ..
            } => {
                let mut sampled = std::collections::BTreeSet::new();
                for frame in start..end {
                    crate::video_decoder::check_cancel(cancel)?;
                    if let Some(index) = layer.sequence_frame(frame, comp.fps()) {
                        if sampled.insert(index) {
                            crate::image_sequence::resolve_frame(frames, index, *missing)?;
                        }
                    }
                }
            }
            Content::Composition {
                composition,
                start_frame,
            } => {
                let source = project
                    .composition_by_id(*composition)
                    .ok_or("Missing source composition")?;
                if layer.time_remap().is_some() {
                    let mut sampled = std::collections::BTreeSet::new();
                    for frame in start..end {
                        crate::video_decoder::check_cancel(cancel)?;
                        if let Some(frame) = layer.composition_frame(frame, comp.fps(), source) {
                            sampled.insert(frame);
                        }
                    }
                    let mut interval: Option<std::ops::Range<u32>> = None;
                    for frame in sampled {
                        match &mut interval {
                            Some(r) if r.end == frame => r.end += 1,
                            _ => {
                                if let Some(r) = interval.take() {
                                    validate_sources(
                                        project,
                                        *composition,
                                        r,
                                        seen,
                                        cancel,
                                        visit,
                                    )?;
                                }
                                interval = Some(frame..frame + 1);
                            }
                        }
                    }
                    if let Some(r) = interval {
                        validate_sources(project, *composition, r, seen, cancel, visit)?;
                    }
                    continue;
                }
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
                    validate_sources(
                        project,
                        *composition,
                        first as u32..end as u32,
                        seen,
                        cancel,
                        visit,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The extension is a save-dialog convention, never a decoder selector.
pub(crate) fn native_destination(path: &Path) -> PathBuf {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        let mut name = path.as_os_str().to_os_string();
        name.push(".lep");
        return PathBuf::from(name);
    };
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".lep") {
        return path.to_path_buf();
    }
    let keep = if lower.ends_with(".lfe.json") {
        name.len() - ".lfe.json".len()
    } else if lower.ends_with(".json") {
        name.len() - ".json".len()
    } else {
        name.len()
    };
    path.with_file_name(format!("{}.lep", &name[..keep]))
}

pub(crate) fn read_project(path: &Path) -> Result<Project, String> {
    Ok(read_editor_project(path)?.project)
}
pub(crate) fn read_editor_project(path: &Path) -> Result<OpenedProject, String> {
    let mut opened = decode_project(&read_bytes(path)?)?;
    opened.project = crate::media_io::resolve(&opened.project, path)?;
    Ok(opened)
}

/// Decode and validate before rebasing any linked media against the outer file.
pub(crate) fn decode_project(bytes: &[u8]) -> Result<OpenedProject, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Project exceeds 256 MiB".into());
    }
    let (project, views, format) = if bytes.starts_with(libre_effects_core::project_file::MAGIC) {
        let decoded = libre_effects_core::project_file::decode(bytes)?;
        crate::rendering::validate_images(&decoded.project)?;
        let views = match decoded.view {
            Some(view) => crate::view_state::ProjectViews::read_native(view, &decoded.project)?,
            None => Default::default(),
        };
        (decoded.project, views, ProjectFormat::Lep)
    } else {
        // Only JSON objects are supported legacy documents. A damaged native
        // header or another binary format must never be mistaken for JSON.
        if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
            return Err(
                "Unrecognized project format; expected a Libre Effects Project or legacy JSON"
                    .into(),
            );
        }
        let json = std::str::from_utf8(bytes).map_err(|_| "Legacy project is not valid UTF-8")?;
        let project = Project::from_json(json)?;
        crate::rendering::validate_images(&project)?;
        let views = crate::view_state::ProjectViews::read(json, &project);
        (project, views, ProjectFormat::LegacyJson)
    };
    Ok(OpenedProject {
        project,
        views,
        format,
    })
}

/// Bound both existing lengths and files that grow after the metadata check.
pub(crate) fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_BYTES {
        return Err("Project exceeds 256 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Project exceeds 256 MiB".into());
    }
    Ok(bytes)
}

pub(crate) fn encode_native_project(
    project: &Project,
    views: Option<&crate::view_state::ProjectViews>,
) -> Result<Vec<u8>, String> {
    crate::rendering::validate_images(project)?;
    let view = views
        .map(|views| views.encode_native(project))
        .transpose()?;
    libre_effects_core::project_file::encode(project, view.as_deref())
}
pub(crate) fn write_native_project(
    path: &Path,
    project: &Project,
    views: Option<&crate::view_state::ProjectViews>,
) -> Result<(), String> {
    // Finish validation and encoding before creating or replacing any file.
    write_bytes(path, &encode_native_project(project, views)?)
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
    fn formats_are_detected_from_bytes_even_with_misleading_extensions() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let project = editor.project();
        let legacy = project.to_json().unwrap();
        let native = encode_native_project(project, None).unwrap();
        for name in [
            "project.lep",
            "project.json",
            "session.previous",
            "no-extension",
        ] {
            let path = dir.path().join(name);
            write_bytes(&path, legacy.as_bytes()).unwrap();
            let opened = read_editor_project(&path).unwrap();
            assert_eq!(opened.format, ProjectFormat::LegacyJson);
            assert_eq!(&opened.project, project);
            assert_eq!(std::fs::read(&path).unwrap(), legacy.as_bytes());
            write_bytes(&path, &native).unwrap();
            let opened = read_editor_project(&path).unwrap();
            assert_eq!(opened.format, ProjectFormat::Lep);
            assert_eq!(&opened.project, project);
        }
        for bytes in [
            b"\x89LEP".as_slice(),
            b"\x89LFE\r\n\x1a\n",
            b"PK\x03\x04",
            b"[]",
            b"",
        ] {
            assert!(decode_project(bytes).is_err());
        }
        let mut corrupt = native;
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(decode_project(&corrupt).is_err());
    }

    #[test]
    fn native_destination_keeps_unicode_names_and_normalizes_legacy_suffixes() {
        for (input, expected) in [
            ("folder/장면.lfe.json", "folder/장면.lep"),
            ("Scene.JSON", "Scene.lep"),
            ("Scene.LEP", "Scene.LEP"),
            ("scene.v2", "scene.v2.lep"),
            ("scene", "scene.lep"),
        ] {
            assert_eq!(native_destination(Path::new(input)), Path::new(expected));
        }
    }

    #[test]
    fn native_image_validation_and_failed_writes_preserve_existing_data() {
        use base64::Engine;
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("sentinel.lep");
        std::fs::write(&destination, b"original project").unwrap();
        let mut oversized = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(4097, 1)
            .write_to(&mut oversized, image::ImageFormat::Png)
            .unwrap();
        for png in [
            "YWJj".to_owned(),
            base64::engine::general_purpose::STANDARD.encode(oversized.into_inner()),
        ] {
            let mut editor = Editor::default();
            editor
                .execute(Command::AddContent {
                    content: Content::Image { png: png.into() },
                    width: 32.0,
                    height: 32.0,
                    name: "Invalid image".into(),
                })
                .unwrap();
            // Core preserves accepted image strings; the desktop decoder must
            // still apply the full image decoder and dimension/allocation limits.
            let bytes = libre_effects_core::project_file::encode(editor.project(), None).unwrap();
            assert!(
                decode_project(&bytes)
                    .unwrap_err()
                    .contains("Invalid image")
            );
            assert!(write_native_project(&destination, editor.project(), None).is_err());
            assert_eq!(std::fs::read(&destination).unwrap(), b"original project");
        }
        let blocked = dir.path().join("blocked.lep");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("sentinel"), b"keep me").unwrap();
        assert!(write_native_project(&blocked, &Project::default(), None).is_err());
        assert_eq!(std::fs::read(blocked.join("sentinel")).unwrap(), b"keep me");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn native_images_roundtrip_with_shared_assets_and_identical_pixels() {
        use base64::Engine;
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 70, 220, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Image {
                    png: base64::engine::general_purpose::STANDARD
                        .encode(png.into_inner())
                        .into(),
                },
                width: 20.0,
                height: 20.0,
                name: "Shared PNG".into(),
            })
            .unwrap();
        editor.execute(Command::DuplicateComposition).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("이미지.lep");
        write_native_project(&path, editor.project(), None).unwrap();
        let opened = read_project(&path).unwrap();
        assert_eq!(&opened, editor.project());
        let images: Vec<_> = opened
            .compositions()
            .into_iter()
            .flat_map(|(_, composition)| composition.layers())
            .map(|layer| match layer.content() {
                Content::Image { png } => png,
                _ => panic!(),
            })
            .collect();
        assert_eq!(images.len(), 2);
        assert!(std::sync::Arc::ptr_eq(images[0], images[1]));
        let Content::Image { png: asset } = opened.asset_library().assets()[&1].content() else {
            panic!()
        };
        assert!(std::sync::Arc::ptr_eq(images[0], asset));
        let renderer = crate::rendering::Renderer::new();
        assert_eq!(
            renderer.render(&opened, 0, 64).unwrap(),
            renderer.render(editor.project(), 0, 64).unwrap()
        );
    }

    #[test]
    fn oversized_disk_files_are_rejected_before_reading_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.lep");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert!(read_bytes(&path).unwrap_err().contains("256 MiB"));
        assert!(read_project(&path).unwrap_err().contains("256 MiB"));
    }

    #[test]
    fn preflight_skips_guide_and_non_solo_media_but_still_protects_their_source() {
        use libre_effects_core::LayerSwitch;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("guide.mp4");
        std::fs::write(&source, b"original").unwrap();
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Video {
                audio: None,
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
                audio: None,
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
                audio: None,
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
