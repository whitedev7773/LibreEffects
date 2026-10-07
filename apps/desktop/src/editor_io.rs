use super::*;
use crate::project_io::{OpenedProject, ProjectFormat};
use crate::rendering::Renderer;
use std::time::Duration;

// A chooser confirms only the selected filename. Never overwrite an existing
// normalized filename that the user did not actually select.
fn selected_native_destination(selected: &Path) -> Result<PathBuf, String> {
    let destination = crate::project_io::native_destination(selected);
    if destination != selected && std::fs::symlink_metadata(&destination).is_ok() {
        return Err(format!(
            "{} already exists. Select that .lep file explicitly to replace it.",
            destination.display()
        ));
    }
    Ok(destination)
}

impl EditorState {
    pub fn prepare_replacement(&mut self, cx: &mut Context<Self>) {
        // Numeric geometry drafts never enter replacement/recovery source or a
        // late field callback. Replacement is not an ordinary modal Cancel.
        self.vertex_editor = None;
        self.expression_editor = None;
        self.vertex_return = None;
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
        let cleanup_warning = if restore {
            self.recovery_session
                .as_ref()
                .ok_or("Recovery session unavailable".to_string())?
                .lock()
                .map_err(|e| e.to_string())?
                .restore(candidate)?
        } else {
            candidate.discard()?;
            None
        };
        self.recovery.take();
        if let Some(editor) = replacement {
            self.vertex_editor = None;
            self.expression_editor = None;
            self.vertex_return = None;
            self.stop();
            self.document_revision = self.document_revision.wrapping_add(1);
            self.editor = editor;
            self.begin_file_operation();
            self.cancel_save(self.file_operation);
            self.clear_clipboard();
            self.load_views(Default::default());
            self.clear_source_provenance();
            self.work_end = self.editor.project().composition().duration();
            self.status = "Recovered checkpoint. Save as to keep it. Other backups remain available at next startup.".into();
            if let Some(warning) = cleanup_warning {
                self.status.push_str(&format!(" {warning}"));
            }
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
        self.open_project(None, cx);
    }
    pub(super) fn open_recent(&mut self, path: &Path, cx: &mut Context<Self>) {
        // Shell validated the exact path before its unsaved-changes prompt.
        // Save and continue can legitimately evict the tenth history entry;
        // retain that already-authorized path instead of revalidating its rank.
        self.open_project(Some(path.to_path_buf()), cx);
    }
    fn open_project(&mut self, selected: Option<PathBuf>, cx: &mut Context<Self>) {
        self.stop();
        let operation = self.begin_file_operation();
        let revision = self.document_revision;
        let previous = self.editor.project().clone();
        let prompt = selected.is_none().then(|| {
            cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Open Libre Effects project".into()),
            })
        });
        cx.spawn(async move |entity, cx| {
            let path = if let Some(path) = selected {
                path
            } else {
                let Ok(Ok(Some(paths))) = prompt.unwrap().await else {
                    return;
                };
                let Some(path) = paths.into_iter().next() else {
                    return;
                };
                path
            };
            let source = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { crate::project_io::read_editor_project(&source) })
                .await;
            let _ = entity.update(cx, |s, cx| {
                if let Err(error) = s.finish_open(operation, revision, &previous, result, path) {
                    s.status = format!("Open failed: {error}");
                }
                s.persist_recent_projects(cx);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn begin_file_operation(&mut self) -> u64 {
        self.pending_svg_import = None;
        self.file_operation = self.file_operation.wrapping_add(1);
        self.file_operation
    }
    fn finish_open(
        &mut self,
        operation: u64,
        revision: u64,
        previous: &Project,
        result: Result<OpenedProject, String>,
        path: PathBuf,
    ) -> Result<(), String> {
        // A newer Open, New, recovery or Save owns the document now. A late
        // callback must not overwrite its baseline, provenance or status.
        if operation != self.file_operation {
            return Ok(());
        }
        if revision != self.document_revision
            || !self.editor.project().same_document(previous)
            || self
                .text_session
                .as_ref()
                .is_some_and(|session| session.changed())
            || self.vertex_editor.is_some()
            || self.expression_editor.is_some()
        {
            return Err("Document changed while opening; open the project again".into());
        }
        self.install_opened_project(result?, path)
    }
    fn install_opened_project(
        &mut self,
        opened: OpenedProject,
        path: PathBuf,
    ) -> Result<(), String> {
        // Keep the current document, path, saved baseline and recovery slot
        // intact unless the resolved project can actually enter the editor.
        let editor = replacement_editor(opened.project)?;
        let path = crate::media_io::clean_absolute(&path)?;
        self.vertex_editor = None;
        self.expression_editor = None;
        self.vertex_return = None;
        self.begin_file_operation();
        self.cancel_save(self.file_operation);
        self.stop();
        self.reset_recovery(false);
        self.document_revision = self.document_revision.wrapping_add(1);
        self.saved = editor.project().clone();
        self.editor = editor;
        self.clear_clipboard();
        self.imported_original = (opened.format == ProjectFormat::LegacyJson
            || crate::project_io::native_destination(&path) != path)
            .then(|| path.clone());
        self.source_format = Some(opened.format);
        if opened.format == ProjectFormat::Lep && self.imported_original.is_none() {
            self.recent_projects.remember(&path);
        }
        self.path = Some(path);
        self.frame = 0;
        self.work_start = 0;
        self.work_end = self.editor.project().composition().duration();
        self.timeline_start = 0;
        self.load_views(opened.views);
        self.selected_layers.clear();
        self.selected_keys.clear();
        let missing = crate::font_usage::missing_count(self.editor.project());
        let message = if self.imported_original.is_some() {
            "Project imported · Save creates a new .lep copy"
        } else {
            "Project opened"
        };
        self.status = if missing == 0 {
            message.into()
        } else {
            format!(
                "{message} · {missing} text layer(s) use unavailable fonts/styles · File → Manage project fonts"
            )
        };
        self.normalize();
        Ok(())
    }
    fn clear_source_provenance(&mut self) {
        self.path = None;
        self.source_format = None;
        self.imported_original = None;
    }
    pub(super) fn install_ae_imported_project(
        &mut self,
        project: Project,
        path: PathBuf,
    ) -> Result<(), String> {
        // Reuse the fully validated document-boundary installer and imported
        // source protection. AE data is never a native .lep save destination.
        self.install_opened_project(
            OpenedProject {
                project,
                views: Default::default(),
                format: ProjectFormat::LegacyJson,
            },
            path,
        )?;
        self.source_format = None;
        self.composition_started = true;
        self.status = self
            .status
            .replacen("Project imported", "AE project data imported", 1);
        Ok(())
    }
    pub(super) fn install_new_project(&mut self) -> Result<(), String> {
        let editor = replacement_editor(Project::default())?;
        self.vertex_editor = None;
        self.expression_editor = None;
        self.vertex_return = None;
        self.begin_file_operation();
        self.cancel_save(self.file_operation);
        self.stop();
        self.reset_recovery(false);
        self.document_revision = self.document_revision.wrapping_add(1);
        self.editor = editor;
        self.saved = self.editor.project().clone();
        self.clear_clipboard();
        self.clear_source_provenance();
        self.composition_started = false;
        self.selected_layers.clear();
        self.selected_keys.clear();
        self.frame = 0;
        self.work_start = 0;
        self.work_end = self.editor.project().composition().duration();
        self.timeline_start = 0;
        self.load_views(Default::default());
        self.status = "New composition".into();
        Ok(())
    }
    fn save_path(&self, choose: bool) -> Option<PathBuf> {
        self.path
            .as_ref()
            .filter(|path| {
                !choose
                    && self.source_format == Some(ProjectFormat::Lep)
                    && crate::project_io::native_destination(path) == **path
            })
            .cloned()
    }
    pub(super) fn should_blur_for_file_action(&self, action: &Action) -> bool {
        match action {
            // Direct saves and already-running saves do not open a chooser.
            // Keep keyboard ownership unless save_project will prompt.
            Action::Save | Action::SaveAs => {
                !self.saving && self.save_path(matches!(action, Action::SaveAs)).is_none()
            }
            Action::Open
            | Action::CollectFiles
            | Action::RelinkSource(_)
            | Action::RelinkMissing
            | Action::ImportImageSequence
            | Action::RelinkSequence(_)
            | Action::ImportImage
            | Action::ImportSvg
            | Action::ImportAeProject
            | Action::ImportVideo
            | Action::RelinkVideo
            | Action::ExportFrame
            | Action::ExportFrameBackground
            | Action::ExportSequence
            | Action::ExportSequenceBackground
            | Action::ExportVideo(_) => true,
            _ => false,
        }
    }
    fn save_directory(&self) -> PathBuf {
        self.path
            .as_ref()
            .and_then(|path| path.parent())
            .unwrap_or(Path::new("."))
            .to_path_buf()
    }
    fn suggested_save_name(&self) -> String {
        let Some(path) = self.path.as_ref() else {
            return "Untitled.lep".into();
        };
        let mut destination = crate::project_io::native_destination(path);
        if self.source_format == Some(ProjectFormat::LegacyJson) && destination == *path {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            destination.set_file_name(format!("{stem}-copy.lep"));
        }
        destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.lep".into())
    }
    fn cancel_save(&mut self, operation: u64) {
        if operation != self.file_operation {
            return;
        }
        self.saving = false;
        self.close_after_save = false;
    }
    fn finish_save(
        &mut self,
        operation: u64,
        snapshot: Project,
        path: PathBuf,
        result: Result<(), String>,
    ) {
        if operation != self.file_operation {
            return;
        }
        self.saving = false;
        match result {
            Ok(()) => {
                self.saved = snapshot;
                self.path = Some(path);
                self.source_format = Some(ProjectFormat::Lep);
                self.status = "Saved".into();
            }
            Err(error) => {
                self.close_after_save = false;
                self.status = format!("Save failed: {error}");
            }
        }
    }
    fn finish_chosen_save(
        &mut self,
        operation: u64,
        snapshot: Project,
        path: PathBuf,
        result: Result<(), String>,
        chose_destination: bool,
    ) {
        let remember = chose_destination && operation == self.file_operation && result.is_ok();
        let recent_path = remember
            .then(|| crate::media_io::clean_absolute(&path).ok())
            .flatten();
        self.finish_save(operation, snapshot, path, result);
        if let Some(path) = recent_path {
            self.recent_projects.remember(&path);
        }
    }
    pub(super) fn clear_recent_projects(&mut self, revision: u64, cx: &mut Context<Self>) {
        if self.recent_projects.clear(revision)
            || (revision == self.recent_projects.revision()
                && self.recent_projects.paths().is_empty()
                && self.recent_history_needs_save())
        {
            self.status = "Recent project list cleared; project files are unchanged".into();
            self.persist_recent_projects(cx);
        }
    }
    pub(crate) fn recent_history_needs_save(&self) -> bool {
        self.recent_projects.revision() != self.recent_projects_persisted_revision
    }
    fn finish_recent_projects_write(&mut self, revision: u64, result: Result<(), String>) -> bool {
        self.recent_projects_writing = false;
        match result {
            Ok(()) => self.recent_projects_persisted_revision = revision,
            Err(error) if revision == self.recent_projects.revision() => {
                self.status
                    .push_str(&format!(" · Recent projects could not be saved: {error}"));
            }
            Err(_) => {}
        }
        // Retry a newer queued snapshot now, but a failed current revision only
        // on the next explicit Open/Save/Clear. Never spin on a broken profile.
        revision != self.recent_projects.revision()
    }
    pub(crate) fn flush_recent_projects_on_close(&self) {
        if !self.recent_history_needs_save() && !self.recent_projects_writing {
            return;
        }
        // Close/replacement is the only synchronous drain. Do not drop a queued
        // Clear while an older write is pending. The revision-aware writer also
        // rejects older background tasks that acquire the lock after this drain.
        let result = (|| {
            let path = crate::recent_projects::path().ok_or("Profile location unavailable")?;
            self.recent_projects_writer
                .lock()
                .map_err(|e| e.to_string())?
                .write(&path, &self.recent_projects)
        })();
        if let Err(error) = result {
            // A settings failure must not prevent quitting or change project
            // data. Routine persistence errors also appear in the status bar.
            eprintln!("Recent projects could not be saved before closing: {error}");
        }
    }
    fn persist_recent_projects(&mut self, cx: &mut Context<Self>) {
        let revision = self.recent_projects.revision();
        if self.recent_projects_writing || revision == self.recent_projects_persisted_revision {
            return;
        }
        let Some(path) = crate::recent_projects::path() else {
            self.status
                .push_str(" · Recent projects could not be saved: profile location unavailable");
            return;
        };
        let history = self.recent_projects.clone();
        let writer = self.recent_projects_writer.clone();
        self.recent_projects_writing = true;
        // A single serial writer keeps a delayed Open/Save write from restoring
        // an older list after Clear. Routine disk sync stays off the UI thread.
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    writer
                        .lock()
                        .map_err(|e| e.to_string())?
                        .write(&path, &history)
                })
                .await;
            let _ = entity.update(cx, |s, cx| {
                if s.finish_recent_projects_write(revision, result) {
                    s.persist_recent_projects(cx);
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
        let previous_operation = self.file_operation;
        let operation = self.begin_file_operation();
        self.advance_ae_import_save(previous_operation, operation);
        let views = self.capture_views();
        let snapshot = self.editor.project().clone();
        let path = self.save_path(choose);
        let chose_destination = path.is_none();
        let suggested_name = self.suggested_save_name();
        let imported_original = self.imported_original.clone();
        let directory = self.save_directory();
        let prompt = path
            .is_none()
            .then(|| cx.prompt_for_new_path(&directory, Some(&suggested_name)));
        self.saving = true;
        cx.spawn(async move |entity, cx| {
            let path = if let Some(prompt) = prompt {
                match prompt.await {
                    Ok(Ok(Some(p))) => p,
                    _ => {
                        let _ = entity.update(cx, |s, cx| {
                            s.cancel_save(operation);
                            cx.notify();
                        });
                        return;
                    }
                }
            } else {
                path.unwrap()
            };
            let path = match selected_native_destination(&path) {
                Ok(path) => path,
                Err(error) => {
                    let _ = entity.update(cx, |s, cx| {
                        s.finish_save(operation, snapshot, path, Err(error));
                        cx.notify();
                    });
                    return;
                }
            };
            let destination = path.clone();
            let copy = snapshot.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::media_io::save_protected(
                        &copy,
                        &views,
                        &destination,
                        imported_original.as_deref(),
                    )
                })
                .await;
            let _ = entity.update(cx, |s, cx| {
                s.finish_chosen_save(operation, snapshot, path, result, chose_destination);
                s.persist_recent_projects(cx);
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
        let imported_original = self.imported_original.clone();
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
            if let Some(source) = &imported_original {
                if let Err(error) = crate::project_io::protect_source(&output, source) {
                    let _ = entity.update(cx, |s, cx| {
                        s.exporting = false;
                        s.status = format!("Render preflight failed: {error}");
                        cx.notify();
                    });
                    return;
                }
            }
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
                let imported_original = imported_original.clone();
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
                        for source in project_path.iter().chain(imported_original.iter()) {
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
