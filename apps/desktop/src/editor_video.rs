use super::*;
use crate::video_export::{VideoPreset, export_video};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};

impl EditorState {
    pub(super) fn export_video(&mut self, preset: VideoPreset, cx: &mut Context<Self>) {
        if self.exporting {
            return;
        }
        self.stop();
        let snapshot = self.editor.project().clone();
        let project_path = self.path.clone();
        let range = self.work_start..self.work_end;
        let total = range.len() as u32;
        let name = format!("render.{}", preset.extension());
        let directory = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."));
        let prompt = cx.prompt_for_new_path(directory, Some(&name));
        self.exporting = true;
        self.video_job = None;
        self.export_cancel.store(false, Ordering::Relaxed);
        let cancel = self.export_cancel.clone();
        cx.spawn(async move |entity, cx| {
            let Ok(Ok(Some(path))) = prompt.await else {
                let _ = entity.update(cx, |s, cx| {
                    s.exporting = false;
                    cx.notify();
                });
                return;
            };
            if !path
                .extension()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(preset.extension()))
            {
                let _ = entity.update(cx, |s, cx| {
                    s.exporting = false;
                    s.status = format!(
                        "Use a .{} filename for {}",
                        preset.extension(),
                        preset.label()
                    );
                    cx.notify();
                });
                return;
            }
            let output = path.display().to_string();
            let label = format!(
                "{} · {} × {} · {} fps · frames {}–{} · {}",
                preset.label(),
                snapshot.composition().width(),
                snapshot.composition().height(),
                snapshot.composition().fps(),
                range.start,
                range.end - 1,
                if preset == VideoPreset::H264 {
                    format!(
                        "Background #{:06X}",
                        snapshot.composition().background_color()
                    )
                } else {
                    "Transparent".into()
                }
            );
            let _ = entity.update(cx, |s, cx| {
                s.video_job = Some(VideoJob {
                    label,
                    progress: 0,
                    total,
                    message: output.clone(),
                });
                cx.notify();
            });
            let progress = Arc::new(AtomicU32::new(0));
            let result = Arc::new(Mutex::new(None));
            let (worker_progress, worker_result, worker_cancel) =
                (progress.clone(), result.clone(), cancel.clone());
            cx.background_executor()
                .spawn(async move {
                    let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if let Some(source) = &project_path {
                            crate::project_io::protect_source(&path, source)?;
                        }
                        export_video(
                            &snapshot,
                            range,
                            preset,
                            &path,
                            worker_cancel,
                            worker_progress,
                        )
                    }))
                    .unwrap_or_else(|_| Err("Render worker failed".into()));
                    *worker_result.lock().unwrap() = Some(rendered);
                })
                .detach();
            loop {
                gpui::Timer::after(std::time::Duration::from_millis(100)).await;
                let finished = result.lock().unwrap().take();
                let done = finished.is_some();
                if entity
                    .update(cx, |s, cx| {
                        if let Some(job) = &mut s.video_job {
                            job.progress = progress.load(Ordering::Relaxed);
                            if let Some(result) = finished {
                                job.message = match result {
                                    Ok(()) => format!("Completed · {output}"),
                                    Err(e) => e,
                                };
                                s.status = job.message.clone();
                                s.exporting = false;
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    cancel.store(true, Ordering::Relaxed);
                    return;
                }
                if done {
                    break;
                }
            }
        })
        .detach();
    }
}
