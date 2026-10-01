use super::*;

impl EditorState {
    pub(super) fn import_assets(&mut self, cx: &mut Context<Self>) {
        if self.importing_video {
            return;
        }
        self.stop();
        self.importing_video = true;
        let revision = self.document_revision;
        let folder = match self.project_item {
            Some(libre_effects_core::ProjectItem::Folder(id)) => Some(id),
            Some(libre_effects_core::ProjectItem::Asset(id)) => self
                .editor
                .project()
                .asset_library()
                .assets()
                .get(&id)
                .and_then(|a| a.folder()),
            _ => None,
        };
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import images or video footage".into()),
        });
        self.status = "Choose footage for the Project panel".into();
        cx.spawn(async move |entity, cx| {
            let paths = prompt.await.ok().and_then(Result::ok).flatten();
            let Some(paths) = paths.filter(|p| !p.is_empty()) else {
                let _ = entity.update(cx, |s, cx| {
                    s.importing_video = false;
                    s.status = "Import canceled".into();
                    cx.notify();
                });
                return;
            };
            let count = paths.len();
            let result = cx
                .background_executor()
                .spawn(async move { read_assets(&paths, folder) })
                .await;
            let _ = entity.update(cx, |s, cx| {
                s.importing_video = false;
                s.status = match result.and_then(|commands| {
                    if revision != s.document_revision {
                        return Err("Document changed during import; import again".into());
                    }
                    s.editor.execute(Command::Batch(commands))
                }) {
                    Ok(()) => {
                        format!("Imported {count} file(s) · select footage and Add to composition")
                    }
                    Err(error) => format!("Import failed; no files added: {error}"),
                };
                s.normalize();
                cx.notify();
            });
        })
        .detach();
    }
}
pub(crate) fn read_assets(
    paths: &[PathBuf],
    folder: Option<libre_effects_core::FolderId>,
) -> Result<Vec<Command>, String> {
    if paths.len() > 1000 {
        return Err("Import at most 1000 files at once".into());
    }
    let mut images = std::collections::BTreeSet::<std::sync::Arc<str>>::new();
    let mut bytes = 0usize;
    paths
        .iter()
        .map(|path| {
            let result = (|| {
                let extension = path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                let (mut content, width, height) =
                    if matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
                        crate::rendering::import_image(path)?
                    } else {
                        let info = crate::footage::probe(path)?;
                        crate::footage::frame_png(&info.path, 0.0, info.width, info.height, 160)?;
                        (
                            Content::Video {
                                path: info.path,
                                duration: info.duration,
                                source_fps: info.source_fps,
                                start_frame: 0,
                                playback: Default::default(),
                            },
                            info.width,
                            info.height,
                        )
                    };
                if let Content::Image { png } = &mut content {
                    if let Some(shared) = images.get(png) {
                        *png = shared.clone();
                    } else {
                        bytes = bytes.saturating_add(png.len());
                        if bytes > 128 * 1024 * 1024 {
                            return Err(
                                "Selected images exceed the 128 MiB project image limit".into()
                            );
                        }
                        images.insert(png.clone());
                    }
                }
                Ok(Command::ImportAsset {
                    content,
                    width: f64::from(width),
                    height: f64::from(height),
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    folder,
                    frame: None,
                })
            })();
            result.map_err(|error: String| format!("{}: {error}", path.display()))
        })
        .collect()
}
