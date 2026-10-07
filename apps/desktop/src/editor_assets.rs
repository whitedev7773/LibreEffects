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
                    s.stop();
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
    pub(super) fn import_assets(
        &mut self,
        sequence: bool,
        from_footage: bool,
        cx: &mut Context<Self>,
    ) {
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
            multiple: !sequence && !from_footage,
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
                s.status = match result.and_then(|mut commands| {
                    if revision != s.document_revision {
                        return Err("Document changed during import; import again".into());
                    }
                    s.stop();
                    if from_footage {s.remember_view();}
                    if from_footage {
                        commands = with_composition(s.editor.project(), commands, s.welcome())?;
                    }
                    s.editor.execute(Command::Batch(commands))
                }) {
                    Ok(()) => {
                        if from_footage { s.composition_started = true; s.restore_composition_view(); }
                        if from_footage {"Created composition from footage".into()} else if sequence { "Imported sequence · Interpret footage sets FPS and missing-frame policy".into() } else { format!("Imported {count} file(s) · Add to composition · audio waveform, Preview and output ready") }
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
fn with_composition(
    project: &Project,
    mut commands: Vec<Command>,
    initialize: bool,
) -> Result<Vec<Command>, String> {
    let before = project.asset_library().assets();
    let mut temporary = Editor::default();
    temporary.replace_project(project.clone())?;
    temporary.execute(Command::Batch(commands.clone()))?;
    let asset = temporary
        .project()
        .asset_library()
        .assets()
        .keys()
        .find(|id| !before.contains_key(id))
        .copied()
        .ok_or("No new footage was imported")?;
    if initialize {
        if project.compositions().len() != 1 || !project.composition().layers().is_empty() {
            return Err("Initial composition is not empty".into());
        }
        temporary.execute(Command::CompositionFromAsset(asset))?;
        let comp = temporary.project().composition();
        commands.extend([
            Command::ConfigureCompositionRate {
                name: comp.name().to_string(),
                width: comp.width(),
                height: comp.height(),
                fps: comp.fps(),
                duration: comp.duration(),
                display_start: 0,
            },
            Command::SetCompositionBackground(comp.background_color()),
            Command::AddAssetLayer { asset, frame: 0 },
        ]);
    } else {
        commands.push(Command::CompositionFromAsset(asset));
    }
    Ok(commands)
}

#[cfg(test)]
mod composition_import_tests {
    use super::*;
    #[test]
    fn footage_start_uses_one_composition_and_one_undo_transaction() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.png");
        image::RgbaImage::from_pixel(64, 32, image::Rgba([80, 120, 160, 255]))
            .save(&path)
            .unwrap();
        for initialize in [true, false] {
            let mut e = Editor::default();
            let before = e.project().clone();
            let commands = with_composition(
                &before,
                read_assets(&[path.clone()], None).unwrap(),
                initialize,
            )
            .unwrap();
            e.execute(Command::Batch(commands)).unwrap();
            assert_eq!(
                e.project().compositions().len(),
                if initialize { 1 } else { 2 }
            );
            assert_eq!(e.project().composition().width(), 64);
            assert_eq!(e.project().composition().height(), 32);
            assert_eq!(e.project().composition().layers().len(), 1);
            assert_eq!(e.project().asset_library().assets().len(), 1);
            let saved = e.project().clone();
            e.undo();
            assert_eq!(*e.project(), before);
            e.redo();
            assert_eq!(*e.project(), saved);
        }
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
