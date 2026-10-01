use super::*;
use crate::rendering::Renderer;
use std::time::Duration;

impl EditorState {
    pub fn dirty(&self) -> bool {
        !self.editor.project().same_document(&self.saved)
    }
    pub fn recover(&mut self, restore: bool, cx: &mut Context<Self>) {
        let Some(candidate) = self.recovery.as_ref() else {
            return;
        };
        let result = if restore {
            self.recovery_session
                .as_ref()
                .ok_or("Recovery session unavailable".to_string())
                .and_then(|session| {
                    session
                        .lock()
                        .map_err(|e| e.to_string())?
                        .restore(candidate)
                })
        } else {
            candidate.discard()
        };
        if let Err(error) = result {
            self.status = format!("Recovery failed: {error}");
            cx.notify();
            return;
        }
        let candidate = self.recovery.take().unwrap();
        if restore {
            self.document_revision = self.document_revision.wrapping_add(1);
            let _ = self.editor.replace_project(candidate.project);
            self.clear_clipboard();
            self.editor.clear_history();
            self.load_views(Default::default());
            self.path = None;
            self.work_end = self.editor.project().composition().duration();
            self.status = "Recovered checkpoint. Save as to keep it. Other backups remain available at next startup.".into();
            self.recovery_pending.clear();
        } else {
            self.recovery = self.recovery_pending.pop_front();
        }
        self.recovery_ready = self.recovery.is_none();
        self.normalize();
        cx.notify();
    }
    pub fn next_recovery(&mut self, cx: &mut Context<Self>) {
        if let Some(next) = self.recovery_pending.pop_front() {
            if let Some(current) = self.recovery.replace(next) {
                self.recovery_pending.push_back(current);
            }
        }
        cx.notify();
    }
    pub fn keep_recoveries(&mut self, cx: &mut Context<Self>) {
        self.recovery = None;
        self.recovery_pending.clear();
        self.recovery_ready = true;
        cx.notify();
    }
    pub fn reset_recovery(&self, close: bool) {
        if let Some(session) = &self.recovery_session {
            if let Ok(mut session) = session.lock() {
                let _ = session.reset(close);
            }
        }
    }
    pub fn clear_recovery(&self) {
        self.reset_recovery(true);
    }
    pub fn start_recovery(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_executor()
                .spawn(async { crate::recovery::Session::start() })
                .await;
            let session = match result {
                Ok((session, candidates, warnings)) => {
                    let session = std::sync::Arc::new(std::sync::Mutex::new(session));
                    if entity
                        .update(cx, |s, cx| {
                            s.recovery_session = Some(session.clone());
                            s.recovery_pending = candidates.into();
                            s.recovery = s.recovery_pending.pop_front();
                            s.recovery_ready = s.recovery.is_none();
                            if !warnings.is_empty() {
                                s.status = warnings.join("  ");
                            }
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                    session
                }
                Err(error) => {
                    let _ = entity.update(cx, |s, cx| {
                        s.recovery_ready = true;
                        s.status = format!("Autosave unavailable: {error}");
                        cx.notify();
                    });
                    return;
                }
            };
            let mut previous = None;
            loop {
                gpui::Timer::after(Duration::from_secs(5)).await;
                let Ok(snapshot) = entity.update(cx, |s, _| {
                    let generation = session.lock().ok()?.generation;
                    s.recovery_ready
                        .then(|| (generation, s.dirty().then(|| s.editor.project().clone())))
                }) else {
                    return;
                };
                let Some(snapshot) = snapshot else {
                    continue;
                };
                if Some(&snapshot) == previous.as_ref() {
                    continue;
                }
                previous = Some(snapshot.clone());
                let session = session.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        session
                            .lock()
                            .map_err(|e| e.to_string())?
                            .checkpoint(snapshot.0, snapshot.1.as_ref())
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
                .spawn(async move { crate::project_io::read_editor_project(&source) })
                .await;
            let _ = entity.update(cx, |s, cx| {
                match result {
                    Ok((project, views)) => {
                        s.reset_recovery(false);
                        s.document_revision = s.document_revision.wrapping_add(1);
                        s.saved = project.clone();
                        let _ = s.editor.replace_project(project);
                        s.clear_clipboard();
                        s.editor.clear_history();
                        s.path = Some(path);
                        s.frame = 0;
                        s.work_start = 0;
                        s.work_end = s.editor.project().composition().duration();
                        s.timeline_start = 0;
                        s.load_views(views);
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
        let views = self.capture_views();
        let snapshot = self.editor.project().clone();
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
            let copy = snapshot.clone();
            let result = cx
                .background_executor()
                .spawn(async move { crate::media_io::save(&copy, &views, &destination) })
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
    pub(super) fn export(&mut self, sequence: bool, background: bool, cx: &mut Context<Self>) {
        if self.exporting {
            return;
        }
        self.stop();
        self.video_job = None;
        let project = self.editor.project().clone();
        let project_path = self.path.clone();
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
                let project_path = project_path.clone();
                let destination = if sequence {
                    output.join(format!("frame-{frame:06}.png"))
                } else {
                    output.clone()
                };
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        if let Some(source) = &project_path {
                            crate::project_io::protect_source(&destination, source)?;
                        }
                        crate::project_io::validate_render(&project, &destination, &(frame..frame + 1))?;
                        let mut pixels = renderer.render(&project, frame, u32::MAX)?;
                        if background {
                            crate::rendering::composite_background(&mut pixels, project.composition().background_color());
                        }
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
                    "start_timecode": project.composition().timecode(first_frame),
                    "timecode_format": "non-drop-frame",
                    "width": project.composition().width(),
                    "height": project.composition().height(),
                    "first_frame": first_frame,
                    "rendered_frames": completed,
                    "requested_frames": count,
                    "complete": completed == count && error.is_none(),
                    "alpha": !background,
                    "background_color": if background { Some(format!("#{:06X}", project.composition().background_color())) } else { None },
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
