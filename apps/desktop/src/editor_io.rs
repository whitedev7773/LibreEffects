use super::*;
use crate::rendering::Renderer;
use std::time::Duration;

impl EditorState {
    pub fn prepare_replacement(&mut self, cx: &mut Context<Self>) {
        self.finish_text(true, cx);
        self.stop();
        self.export_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.collection_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        cx.notify();
    }
    pub fn preserve_replacement(&self) -> Result<(), String> {
        if !self.dirty() {
            if let Some(session) = &self.recovery_session {
                session.lock().map_err(|e| e.to_string())?.reset(true)?;
            }
            return Ok(());
        }
        let session = self.recovery_session.as_ref().ok_or(
            "Recovery storage is not ready; replacement canceled to preserve unsaved edits",
        )?;
        session
            .lock()
            .map_err(|e| e.to_string())?
            .preserve_for_replacement(self.editor.project())
    }
    pub fn dirty(&self) -> bool {
        self.text_session.as_ref().is_some_and(|s| s.changed())
            || !self.editor.project().same_document(&self.saved)
    }
    pub fn recover(&mut self, restore: bool, cx: &mut Context<Self>) {
        if let Err(error) = self.apply_recovery(restore) {
            self.status = format!("Recovery failed: {error}");
        }
        cx.notify();
    }
    fn apply_recovery(&mut self, restore: bool) -> Result<(), String> {
        let Some(candidate) = self.recovery.as_ref() else {
            return Ok(());
        };
        // Validate the exact project that will become live before writing our
        // checkpoint or consuming the abandoned slot. Resolved media paths can
        // exceed the metadata budget even when the stored relative paths fit.
        let replacement = if restore {
            Some(replacement_editor(candidate.project.clone())?)
        } else {
            None
        };
        if restore {
            self.recovery_session
                .as_ref()
                .ok_or("Recovery session unavailable".to_string())?
                .lock()
                .map_err(|e| e.to_string())?
                .restore(candidate)?;
        } else {
            candidate.discard()?;
        }
        self.recovery.take();
        if let Some(editor) = replacement {
            self.stop();
            self.document_revision = self.document_revision.wrapping_add(1);
            self.editor = editor;
            self.clear_clipboard();
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
        Ok(())
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
                        .then(|| (generation, s.dirty().then(|| s.text_project())))
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
                        if let Err(error) = s.install_opened_project(project, views, path) {
                            s.status = format!("Open failed: {error}");
                        }
                    }
                    Err(e) => s.status = format!("Open failed: {e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn install_opened_project(
        &mut self,
        project: Project,
        views: crate::view_state::ProjectViews,
        path: PathBuf,
    ) -> Result<(), String> {
        // Keep the current document, path, saved baseline and recovery slot
        // intact unless the resolved project can actually enter the editor.
        let editor = replacement_editor(project)?;
        self.stop();
        self.reset_recovery(false);
        self.document_revision = self.document_revision.wrapping_add(1);
        self.saved = editor.project().clone();
        self.editor = editor;
        self.clear_clipboard();
        self.path = Some(path);
        self.frame = 0;
        self.work_start = 0;
        self.work_end = self.editor.project().composition().duration();
        self.timeline_start = 0;
        self.load_views(views);
        self.selected_layers.clear();
        self.selected_keys.clear();
        let missing = crate::font_usage::missing_count(self.editor.project());
        self.status = if missing == 0 {
            "Project opened".into()
        } else {
            format!(
                "Project opened · {missing} text layer(s) use unavailable fonts/styles · File → Manage project fonts"
            )
        };
        self.normalize();
        Ok(())
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
            let project = std::sync::Arc::new(project);
            let (preflight_project, preflight_source, preflight_output, preflight_range, preflight_cancel) =
                (project.clone(), project_path.clone(), output.clone(), range.clone(), cancel.clone());
            let preflight = cx.background_executor().spawn(async move {
                crate::output_preflight::check(
                    &preflight_project,
                    preflight_source.as_deref(),
                    preflight_range,
                    if background { crate::output_settings::Format::PngBackground } else { crate::output_settings::Format::PngAlpha },
                    &Default::default(),
                    &preflight_output,
                    if sequence { crate::output_preflight::Destination::NewSequence } else { crate::output_preflight::Destination::File },
                    &crate::video_export::ffmpeg_path(),
                    &preflight_cancel,
                ).map(|prepared| prepared.report)
            }).await;
            let report = match preflight {
                Ok(report) => report,
                Err(report) => {
                    let _ = entity.update(cx, |s, cx| {
                        s.exporting = false;
                        s.status = format!("Render preflight failed: {report}");
                        cx.notify();
                    });
                    return;
                }
            };
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = entity.update(cx, |s, cx| {
                    s.exporting = false;
                    s.status = "Render canceled; destination unchanged".into();
                    cx.notify();
                });
                return;
            }
            if sequence && let Err(e) = std::fs::create_dir(&output) {
                let _ = entity.update(cx, |s, cx| {
                    s.exporting = false;
                    s.status = e.to_string();
                    cx.notify();
                });
                return;
            }
            let renderer = std::sync::Arc::new(Renderer::with_cancel(cancel.clone()));
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
                let frame_cancel = cancel.clone();
                let destination = if sequence {
                    output.join(format!("frame-{frame:06}.png"))
                } else {
                    output.clone()
                };
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        crate::video_decoder::check_cancel(&frame_cancel)?;
                        if let Some(source) = &project_path {
                            crate::project_io::protect_source(&destination, source)?;
                        }
                        crate::project_io::validate_render_with(&project, &destination, &(frame..frame + 1), &frame_cancel, &mut |_, _| {})?;
                        let mut pixels = renderer.render(&project, frame, u32::MAX)?;
                        if background {
                            crate::rendering::composite_background(&mut pixels, project.composition().background_color());
                        }
                        let mut data = std::io::Cursor::new(Vec::new());
                        pixels
                            .write_to(&mut data, image::ImageFormat::Png)
                            .map_err(|e| e.to_string())?;
                        crate::video_decoder::check_cancel(&frame_cancel)?;
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
                        s.status = report.completion(&format!("Rendering {completed}/{count} frames…"));
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
                    report.completion(&format!(
                        "Rendered {completed}/{count} frames to {}",
                        output.display()
                    ))
                };
                cx.notify();
            });
        })
        .detach();
    }
}

/// Construct a document-boundary replacement without changing the live editor.
fn replacement_editor(project: Project) -> Result<Editor, String> {
    let mut editor = Editor::default();
    editor.replace_project(project)?;
    editor.clear_history();
    Ok(editor)
}

#[cfg(test)]
#[path = "editor_io_tests.rs"]
mod tests;
