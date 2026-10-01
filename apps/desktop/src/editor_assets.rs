use super::*;

impl EditorState {
    pub(super) fn relink_sequence(&mut self, asset: u64, cx: &mut Context<Self>) {
        if self.importing_video {
            return;
        }
        let snapshot = self.editor.project().clone();
        let Some(a) = snapshot.asset_library().assets().get(&asset) else {
            return;
        };
        let (content, width, height) = (a.content().clone(), a.width() as u32, a.height() as u32);
        self.stop();
        self.importing_video = true;
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Locate the folder containing the sequence frames".into()),
        });
        cx.spawn(async move |entity, cx| {
            let selected = prompt
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|p| p.into_iter().next());
            let result = if let Some(folder) = selected {
                cx.background_executor()
                    .spawn(async move {
                        crate::image_sequence::relocate(asset, &content, width, height, &folder)
                    })
                    .await
            } else {
                Err("Sequence relink canceled".into())
            };
            let _ = entity.update(cx, |s, cx| {
                s.importing_video = false;
                s.status = match result.and_then(|command| {
                    if !s.editor.project().same_document(&snapshot) {
                        return Err("Project changed during relink; retry".into());
                    }
                    s.editor.execute(command)
                }) {
                    Ok(()) => "Sequence relinked in all compositions".into(),
                    Err(e) => e,
                };
                s.preview_revision = s.preview_revision.wrapping_add(1);
                s.normalize();
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn import_assets(&mut self, sequence: bool, cx: &mut Context<Self>) {
        if self.importing_video {
            return;
        }
        self.stop();
        self.importing_video = true;
        let revision = self.document_revision;
        let fps = self.editor.project().composition().fps();
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
            multiple: !sequence,
            prompt: Some(
                if sequence {
                    "Choose the first numbered PNG/JPEG frame"
                } else {
                    "Import images, video or audio footage"
                }
                .into(),
            ),
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
                .spawn(async move { if sequence { crate::image_sequence::discover(&paths[0], fps, folder).map(|command| vec![command]) } else { read_assets(&paths, folder) } })
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
                        if sequence { "Imported sequence · Interpret footage sets FPS and missing-frame policy".into() } else { format!("Imported {count} file(s) · Add to composition · audio waveform only (playback/export silent)") }
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
                    } else if !crate::audio::probe(path)?.has_video {
                        let audio = crate::audio::probe(path)?
                            .audio
                            .ok_or("No audio or video stream found")?;
                        let path = crate::media_io::path_string(
                            &std::fs::canonicalize(path).map_err(|e| e.to_string())?,
                        )?;
                        crate::audio::decode_chunk(&path, &audio, 0)?;
                        (
                            Content::Audio {
                                path,
                                audio,
                                start_frame: 0,
                                playback: Default::default(),
                            },
                            1,
                            1,
                        )
                    } else {
                        let info = crate::footage::probe(path)?;
                        crate::footage::frame_png(&info.path, 0.0, info.width, info.height, 160)?;
                        (
                            Content::Video {
                                audio: info.audio,
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
