use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};

impl EditorState {
    pub(super) fn refresh_media(&mut self, cx: &mut Context<Self>) {
        if self.scanning_media {
            return;
        }
        self.stop();
        self.scanning_media = true;
        let snapshot = self.editor.project().clone();
        cx.spawn(async move |entity, cx| {
            let copy = snapshot.clone();
            let entries = cx
                .background_executor()
                .spawn(async move { crate::media_io::entries(&copy) })
                .await;
            let _ = entity.update(cx, |s, cx| {
                s.scanning_media = false;
                if s.editor.project().same_document(&snapshot) {
                    s.media_entries = entries;
                } else if s.media_open {
                    s.refresh_media(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn relink_media(&mut self, source: Option<String>, cx: &mut Context<Self>) {
        if self.importing_video {
            return;
        }
        self.stop();
        self.importing_video = true;
        let snapshot = self.editor.project().clone();
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: source.is_some(),
            directories: source.is_none(),
            multiple: false,
            prompt: Some(
                if source.is_some() {
                    "Locate the replacement media file"
                } else {
                    "Find missing footage in this folder"
                }
                .into(),
            ),
        });
        self.media_message = "Choose replacement media".into();
        cx.spawn(async move |entity, cx| {
            let selected = prompt
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|p| p.into_iter().next());
            let Some(path) = selected else {
                let _ = entity.update(cx, |s, cx| {
                    s.importing_video = false;
                    s.media_message = "Relink canceled".into();
                    cx.notify();
                });
                return;
            };
            let copy = snapshot.clone();
            let _ = entity.update(cx, |s, cx| {
                s.media_message = "Checking replacement footage…".into();
                cx.notify();
            });
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (candidates, mut issues) = if let Some(source) = source {
                        (vec![(source, path)], Vec::new())
                    } else {
                        crate::media_io::missing_candidates(&copy, &path)?
                    };
                    let mut replacements = Vec::new();
                    for (source, path) in candidates {
                        match crate::media_io::replacement_for_project(&copy, source.clone(), &path)
                        {
                            Ok(replacement) => replacements.push(replacement),
                            Err(error) => issues.push(format!("{source}: {error}")),
                        }
                    }
                    Ok::<_, String>((replacements, issues))
                })
                .await;
            let _ = entity.update(cx, |s, cx| {
                s.importing_video = false;
                s.media_message = match result.and_then(|(replacements, issues)| {
                    if !s.editor.project().same_document(&snapshot) {
                        return Err(
                            "Project changed while locating footage; retry the relink".into()
                        );
                    }
                    let count = replacements.len();
                    if count > 0 {
                        s.editor.execute(Command::RelinkMedia(replacements))?;
                    }
                    Ok(format!(
                        "Relinked {count} source(s) in all compositions.{}",
                        if issues.is_empty() {
                            String::new()
                        } else {
                            format!("\n{}", issues.join("\n"))
                        }
                    ))
                }) {
                    Ok(message) => {
                        crate::footage::clear_cache();
                        s.preview_revision = s.preview_revision.wrapping_add(1);
                        message
                    }
                    Err(error) => format!("Relink failed: {error}"),
                };
                s.status = s.media_message.lines().next().unwrap_or_default().into();
                s.normalize();
                s.refresh_media(cx);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn collect_files(&mut self, cx: &mut Context<Self>) {
        if self.collecting {
            return;
        }
        self.stop();
        let snapshot = self.editor.project().clone();
        let views = self.capture_views();
        let total = crate::media_io::video_paths(&snapshot).len();
        let cancel = Arc::new(AtomicBool::new(false));
        self.collection_cancel = cancel.clone();
        self.collecting = true;
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a parent folder for a new collected project".into()),
        });
        cx.spawn(async move |entity, cx| {
            let folder = prompt.await.ok().and_then(Result::ok).flatten().and_then(|p| p.into_iter().next());
            let Some(folder) = folder else {
                let _ = entity.update(cx, |s, cx| { s.collecting = false; s.status = "File collection canceled".into(); cx.notify(); });
                return;
            };
            let progress = Arc::new(AtomicUsize::new(0));
            let count = progress.clone();
            let canceled = cancel.clone();
            let (send, receive) = mpsc::channel();
            cx.background_executor().spawn(async move {
                let result = crate::media_io::collect(&snapshot, &views, &folder, |done, _| {
                    count.store(done, Ordering::Relaxed);
                    if canceled.load(Ordering::Relaxed) { Err("File collection canceled; source project unchanged".into()) } else { Ok(()) }
                });
                let _ = send.send(result);
            }).detach();
            loop {
                let result = match receive.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => Some(Err("File collection worker stopped".into())),
                };
                let finished = result.is_some();
                if entity.update(cx, |s, cx| {
                    s.status = match result {
                        Some(Ok(result)) => format!("Collected {} files ({:.1} MiB): {}", result.files, result.bytes as f64 / 1048576.0, result.project_path.display()),
                        Some(Err(error)) => format!("Collection failed: {error}"),
                        None => format!("Collecting {}/{total} media files… File > Cancel file collection to stop", progress.load(Ordering::Relaxed)),
                    };
                    s.collecting = !finished;
                    s.media_message = s.status.clone();
                    cx.notify();
                }).is_err() {
                    cancel.store(true, Ordering::Relaxed);
                    return;
                }
                if finished { break; }
                gpui::Timer::after(std::time::Duration::from_millis(200)).await;
            }
        }).detach();
    }
}
