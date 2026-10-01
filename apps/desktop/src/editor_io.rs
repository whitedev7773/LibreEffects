use super::*;
use crate::rendering::Renderer;
use std::time::Duration;

pub(super) fn recovery_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("LibreEffects")
        .join("recovery.lfe.json")
}
impl EditorState {
    pub fn dirty(&self) -> bool {
        self.editor.project() != &self.saved
    }
    pub fn recover(&mut self, restore: bool, cx: &mut Context<Self>) {
        if let Some(project) = self.recovery.take() {
            if restore {
                self.document_revision = self.document_revision.wrapping_add(1);
                let _ = self.editor.replace_project(project);
                self.path = None;
                self.work_end = self.editor.project().composition().duration();
                self.status = "Recovered autosave. Save as to keep it.".into();
            } else {
                let _ = std::fs::remove_file(recovery_path());
            }
        }
        self.recovery_ready = true;
        self.normalize();
        cx.notify();
    }
    pub fn start_recovery(&mut self, cx: &mut Context<Self>) {
        let active = self.recovery_active.clone();
        cx.spawn(async move |entity, cx| {
            let recovered = cx
                .background_executor()
                .spawn(async { read_project(&recovery_path()).ok() })
                .await;
            if entity
                .update(cx, |s, cx| {
                    s.recovery = recovered;
                    s.recovery_ready = s.recovery.is_none();
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            let mut previous = None;
            loop {
                gpui::Timer::after(Duration::from_secs(5)).await;
                let Ok(snapshot) = entity.update(cx, |s, _| {
                    if s.recovery_ready && s.dirty() {
                        Some(s.editor.project().clone())
                    } else {
                        None
                    }
                }) else {
                    return;
                };
                if snapshot == previous {
                    continue;
                }
                previous = snapshot.clone();
                let active = active.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let enabled = active.lock().map_err(|e| e.to_string())?;
                        if !*enabled {
                            return Ok(());
                        }
                        let path = recovery_path();
                        if let Some(project) = snapshot {
                            std::fs::create_dir_all(path.parent().unwrap())
                                .map_err(|e| e.to_string())?;
                            write_project(&path, &project.to_json()?)
                        } else {
                            let _ = std::fs::remove_file(path);
                            Ok(())
                        }
                    })
                    .await;
                if let Err(error) = result {
                    let _ = entity.update(cx, |s, cx| {
                        s.status = format!("Autosave failed: {error}");
                        cx.notify();
                    });
                    previous = None;
                }
            }
        })
        .detach();
    }
    pub fn clear_recovery(&self) {
        let Ok(mut active) = self.recovery_active.lock() else {
            return;
        };
        *active = false;
        let _ = std::fs::remove_file(recovery_path());
    }
    pub(super) fn open(&mut self, cx: &mut Context<Self>) {
        self.stop();
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Libre Effects project".into()),
        });
        cx.spawn(async move |entity, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let source = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { read_project(&source) })
                .await;
            let _ = entity.update(cx, |s, cx| {
                match result {
                    Ok(project) => {
                        s.document_revision = s.document_revision.wrapping_add(1);
                        s.saved = project.clone();
                        let _ = s.editor.replace_project(project);
                        s.editor.clear_history();
                        s.path = Some(path);
                        s.frame = 0;
                        s.work_start = 0;
                        s.work_end = s.editor.project().composition().duration();
                        s.timeline_start = 0;
                        s.selected_layers.clear();
                        s.selected_keys.clear();
                        s.status = "Project opened".into();
                        s.normalize();
                    }
                    Err(e) => s.status = format!("Open failed: {e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        self.save_project(false, cx);
    }
    pub(super) fn save_as(&mut self, cx: &mut Context<Self>) {
        self.save_project(true, cx);
    }
    fn save_project(&mut self, choose: bool, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        self.stop();
        let snapshot = self.editor.project().clone();
        let json = match snapshot.to_json() {
            Ok(s) => s,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let path = if choose { None } else { self.path.clone() };
        let directory = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let prompt = path
            .is_none()
            .then(|| cx.prompt_for_new_path(&directory, Some("Untitled.lfe.json")));
        self.saving = true;
        cx.spawn(async move |entity, cx| {
            let path = if let Some(prompt) = prompt {
                match prompt.await {
                    Ok(Ok(Some(p))) => p,
                    _ => {
                        let _ = entity.update(cx, |s, cx| {
                            s.saving = false;
                            s.close_after_save = false;
                            cx.notify();
                        });
                        return;
                    }
                }
            } else {
                path.unwrap()
            };
            let destination = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { write_project(&destination, &json) })
                .await;
            let _ = entity.update(cx, |s, cx| {
                s.saving = false;
                match result {
                    Ok(()) => {
                        s.saved = snapshot;
                        s.path = Some(path);
                        s.status = "Saved".into();
                    }
                    Err(e) => {
                        s.close_after_save = false;
                        s.status = format!("Save failed: {e}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn import(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import PNG or JPEG image".into()),
        });
        cx.spawn(async move |entity, cx| {
            let Ok(Ok(Some(paths))) = prompt.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let result = cx
                .background_executor()
                .spawn(async move { crate::rendering::import_image(&path) })
                .await;
            let _ = entity.update(cx, |s, cx| {
                s.status = match result.and_then(|(content, w, h)| {
                    s.editor.execute(Command::AddContent {
                        content,
                        width: w as f64,
                        height: h as f64,
                        name,
                    })
                }) {
                    Ok(()) => "Image embedded in project".into(),
                    Err(e) => format!("Import failed: {e}"),
                };
                s.selected_layers.clear();
                s.normalize();
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn export(&mut self, sequence: bool, cx: &mut Context<Self>) {
        if self.exporting {
            return;
        }
        self.stop();
        self.video_job = None;
        let project = self.editor.project().clone();
        let range = if sequence {
            self.work_start..self.work_end
        } else {
            self.frame..self.frame + 1
        };
        let choose_folder = sequence.then(|| {
            cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some("Choose output folder for PNG sequence".into()),
            })
        });
        let choose_file =
            (!sequence).then(|| cx.prompt_for_new_path(Path::new("."), Some("frame.png")));
        self.export_cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let cancel = self.export_cancel.clone();
        self.exporting = true;
        cx.spawn(async move |entity, cx| {
            let selected = if let Some(p) = choose_folder {
                match p.await {
                    Ok(Ok(Some(p))) => p.into_iter().next(),
                    _ => None,
                }
            } else {
                match choose_file.unwrap().await {
                    Ok(Ok(p)) => p,
                    _ => None,
                }
            };
            let Some(path) = selected else {
                let _ = entity.update(cx, |s, cx| {
                    s.exporting = false;
                    cx.notify();
                });
                return;
            };
            let output = if sequence {
                path.join(format!(
                    "LibreEffects-{}-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis(),
                    std::process::id()
                ))
            } else {
                path
            };
            if sequence && let Err(e) = std::fs::create_dir(&output) {
                let _ = entity.update(cx, |s, cx| {
                    s.exporting = false;
                    s.status = e.to_string();
                    cx.notify();
                });
                return;
            }
            let renderer = std::sync::Arc::new(Renderer::new());
            let project = std::sync::Arc::new(project);
            let count = range.len();
            let first_frame = range.start;
            let mut completed = 0;
            let mut error = None;
            for frame in range {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let (renderer, project) = (renderer.clone(), project.clone());
                let destination = if sequence {
                    output.join(format!("frame-{frame:06}.png"))
                } else {
                    output.clone()
                };
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let pixels = renderer.render(&project, frame, u32::MAX)?;
                        let mut data = std::io::Cursor::new(Vec::new());
                        pixels
                            .write_to(&mut data, image::ImageFormat::Png)
                            .map_err(|e| e.to_string())?;
                        crate::project_io::write_bytes(&destination, &data.into_inner())
                    })
                    .await;
                if let Err(e) = result {
                    error = Some(e);
                    break;
                }
                completed += 1;
                if entity
                    .update(cx, |s, cx| {
                        s.status = format!("Rendering {completed}/{count} frames…");
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            if sequence {
                let manifest = serde_json::json!({
                    "composition": project.composition().name(),
                    "fps": project.composition().fps(),
                    "width": project.composition().width(),
                    "height": project.composition().height(),
                    "first_frame": first_frame,
                    "rendered_frames": completed,
                    "requested_frames": count,
                    "complete": completed == count && error.is_none(),
                    "pattern": "frame-%06d.png"
                });
                if let Err(e) = crate::project_io::write_bytes(
                    &output.join("sequence.json"),
                    manifest.to_string().as_bytes(),
                ) {
                    error = Some(e);
                }
            }
            let _ = entity.update(cx, |s, cx| {
                s.exporting = false;
                s.status = if let Some(e) = error {
                    format!("Render failed: {e}")
                } else {
                    format!(
                        "Rendered {completed}/{count} frames to {}",
                        output.display()
                    )
                };
                cx.notify();
            });
        })
        .detach();
    }
}
