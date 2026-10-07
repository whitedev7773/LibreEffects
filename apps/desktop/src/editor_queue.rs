use super::*;
use crate::render_queue::{Output, Preset, Queue, Status};
use std::sync::{Arc, Mutex, atomic::Ordering};

#[derive(Clone)]
pub(crate) enum QueueAction {
    Show(bool),
    Add,
    Start,
    Stop,
    Undo,
    Redo,
    Enable(u64),
    Move(u64, i32),
    Remove(u64),
    Retry(u64),
    Range(u64, bool, String),
    Path(u64, usize),
    AddOutput(u64, crate::output_settings::Spec),
    RemoveOutput(u64, usize),
    Settings(u64, usize, crate::output_settings::Field, String),
    ResetSettings(u64, usize),
    Policy,
    SavePreset(u64, String),
    DeletePreset(usize),
}
impl EditorState {
    pub fn load_queue(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |entity, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async { crate::render_queue::default_root().and_then(Queue::load) })
                .await;
            let _ = entity.update(cx, |s, cx| {
                match loaded {
                    Ok(queue) => {
                        s.queue = Some(Arc::new(Mutex::new(queue)));
                        s.queue_message.clear();
                    }
                    Err(error) => s.queue_message = error,
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub fn queue_action(
        &mut self,
        action: &QueueAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let QueueAction::Show(open) = action {
            self.queue_open = *open;
            cx.notify();
            return;
        }
        let Some(queue) = self.queue.clone() else {
            return;
        };
        if matches!(action, QueueAction::Stop) {
            if let Ok(q) = queue.lock() {
                q.cancel.store(true, Ordering::Relaxed);
            }
            return;
        }
        if self.exporting || self.queue_busy {
            self.status = "Wait for the current render or queue change to finish".into();
            cx.notify();
            return;
        }
        if matches!(action, QueueAction::Start) {
            let result = queue
                .lock()
                .map_err(|e| e.to_string())
                .and_then(|mut q| q.begin());
            if let Err(e) = result {
                self.queue_message = e;
                cx.notify();
                return;
            }
            self.stop();
            self.queue_message.clear();
            self.exporting = true;
            self.video_job = None;
            self.export_cancel = queue.lock().unwrap().cancel.clone();
            cx.background_executor()
                .spawn(async move { crate::render_queue::run(queue) })
                .detach();
            cx.spawn(async move |entity, cx| {
                loop {
                    gpui::Timer::after(std::time::Duration::from_millis(100)).await;
                    let Ok(running) = entity.update(cx, |s, cx| {
                        let running = s
                            .queue
                            .as_ref()
                            .and_then(|q| q.try_lock().ok().map(|q| q.running))
                            .unwrap_or(true);
                        if !running {
                            s.exporting = false;
                            s.queue_message = s
                                .queue
                                .as_ref()
                                .and_then(|q| q.lock().ok().map(|q| q.message.clone()))
                                .unwrap_or_default();
                        }
                        cx.notify();
                        running
                    }) else {
                        return;
                    };
                    if !running {
                        return;
                    }
                }
            })
            .detach();
            cx.notify();
            return;
        }
        if matches!(action, QueueAction::Add) {
            window.blur();
            let snapshot = self.editor.project().clone();
            let source = self.path.clone();
            let imported_original = self.imported_original.clone();
            let range = self.work_start..self.work_end;
            let formats = self.queue_formats.clone();
            let prompt = cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some("Choose output folder for this render job".into()),
            });
            self.queue_busy = true;
            self.stop();
            cx.spawn(async move |entity, cx| {
                let folder = prompt
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten()
                    .and_then(|p| p.into_iter().next());
                let result = if let Some(folder) = folder {
                    cx.background_executor()
                        .spawn(async move {
                            queue
                                .lock()
                                .map_err(|e| e.to_string())?
                                .enqueue_protected(
                                    &snapshot,
                                    source,
                                    imported_original,
                                    range,
                                    &formats,
                                    &folder,
                                )
                                .map(|_| ())
                        })
                        .await
                } else {
                    Err("Queue addition canceled".into())
                };
                let _ = entity.update(cx, |s, cx| {
                    s.queue_busy = false;
                    s.queue_open = true;
                    s.queue_message = result.err().unwrap_or_else(|| {
                        "Added a snapshot of the current composition and work area".into()
                    });
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        if let QueueAction::Path(id, _) | QueueAction::AddOutput(id, _) = action {
            window.blur();
            let (id, index, format, initial) = {
                let q = queue.lock().unwrap();
                let Some(job) = q.data.jobs.iter().find(|j| j.id == *id) else {
                    return;
                };
                match action {
                    QueueAction::Path(_, index) => {
                        let Some(o) = job.outputs.get(*index) else {
                            return;
                        };
                        (*id, Some(*index), o.spec.clone(), o.path.clone())
                    }
                    QueueAction::AddOutput(_, format) => (
                        *id,
                        None,
                        format.clone(),
                        job.outputs[0].path.with_file_name(format!(
                            "render-{}-{}.{}",
                            id,
                            job.outputs.len() + 1,
                            format.format.extension()
                        )),
                    ),
                    _ => unreachable!(),
                }
            };
            let prompt = cx.prompt_for_new_path(
                initial.parent().unwrap_or(Path::new(".")),
                initial.file_name().and_then(|s| s.to_str()),
            );
            self.queue_busy = true;
            cx.spawn(async move |entity, cx| {
                let path = prompt.await.ok().and_then(Result::ok).flatten();
                let result = if let Some(path) = path {
                    queue.lock().map_err(|e| e.to_string()).and_then(|mut q| {
                        q.edit(|d| {
                            let job = d
                                .jobs
                                .iter_mut()
                                .find(|j| j.id == id)
                                .ok_or("Job no longer exists")?;
                            if let Some(index) = index {
                                *job.outputs
                                    .get_mut(index)
                                    .ok_or("Output no longer exists")? = Output {
                                    spec: format,
                                    path,
                                    status: Status::Queued,
                                    message: String::new(),
                                };
                            } else {
                                job.outputs.push(Output {
                                    spec: format,
                                    path,
                                    status: Status::Queued,
                                    message: String::new(),
                                });
                            }
                            Ok(())
                        })
                    })
                } else {
                    Err("Output selection canceled".into())
                };
                let _ = entity.update(cx, |s, cx| {
                    s.queue_busy = false;
                    s.queue_message = result.err().unwrap_or_default();
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        let action = action.clone();
        let result = queue.lock().map_err(|e| e.to_string()).and_then(|mut q| {
            if matches!(action, QueueAction::Undo | QueueAction::Redo) {
                return q.history(matches!(action, QueueAction::Redo));
            }
            q.edit(|data| {
                match &action {
                    QueueAction::Settings(id, index, field, value) => {
                        let output = data
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == *id)
                            .and_then(|j| j.outputs.get_mut(*index))
                            .ok_or("Output not found")?;
                        output.spec.settings.change(*field, value)?;
                        output.status = Status::Queued;
                        output.message.clear();
                    }
                    QueueAction::ResetSettings(id, index) => {
                        let output = data
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == *id)
                            .and_then(|j| j.outputs.get_mut(*index))
                            .ok_or("Output not found")?;
                        output.spec.settings = Default::default();
                        output.status = Status::Queued;
                        output.message.clear();
                    }
                    QueueAction::Policy => data.stop_on_error = !data.stop_on_error,
                    QueueAction::DeletePreset(i) => {
                        if *i < data.presets.len() {
                            data.presets.remove(*i);
                        }
                    }
                    QueueAction::SavePreset(id, name) => {
                        let job = data
                            .jobs
                            .iter()
                            .find(|j| j.id == *id)
                            .ok_or("Job not found")?;
                        let preset = Preset {
                            name: name.trim().into(),
                            specs: job.outputs.iter().map(|o| o.spec.clone()).collect(),
                        };
                        if let Some(old) = data.presets.iter_mut().find(|p| p.name == preset.name) {
                            *old = preset;
                        } else {
                            data.presets.push(preset);
                        }
                    }
                    QueueAction::Enable(id) => {
                        let j = data
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == *id)
                            .ok_or("Job not found")?;
                        j.enabled = !j.enabled;
                    }
                    QueueAction::Move(id, delta) => {
                        let i = data
                            .jobs
                            .iter()
                            .position(|j| j.id == *id)
                            .ok_or("Job not found")?;
                        let to = (i as i32 + delta).clamp(0, data.jobs.len() as i32 - 1) as usize;
                        data.jobs.swap(i, to);
                    }
                    QueueAction::Remove(id) => data.jobs.retain(|j| j.id != *id),
                    QueueAction::Retry(id) => {
                        let j = data
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == *id)
                            .ok_or("Job not found")?;
                        for o in &mut j.outputs {
                            if matches!(
                                o.status,
                                Status::Failed | Status::Canceled | Status::Interrupted
                            ) {
                                o.status = Status::Queued;
                                o.message.clear();
                            }
                        }
                    }
                    QueueAction::Range(id, start, text) => {
                        let value = text
                            .trim()
                            .parse::<u32>()
                            .map_err(|_| "Range must use whole composition frames")?;
                        let j = data
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == *id)
                            .ok_or("Job not found")?;
                        if *start {
                            j.range.start = value;
                        } else {
                            j.range.end = value;
                        }
                        for o in &mut j.outputs {
                            o.status = Status::Queued;
                            o.message.clear();
                        }
                    }
                    QueueAction::RemoveOutput(id, index) => {
                        let j = data
                            .jobs
                            .iter_mut()
                            .find(|j| j.id == *id)
                            .ok_or("Job not found")?;
                        if *index < j.outputs.len() {
                            j.outputs.remove(*index);
                        }
                    }
                    _ => {}
                }
                Ok(())
            })
        });
        self.queue_message = result.err().unwrap_or_default();
        cx.notify();
    }
}
