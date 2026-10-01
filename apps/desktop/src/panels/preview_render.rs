//! Exactly one compositor task at a time. Seeking cancels its decoder wait;
//! generations reject its result even if cancellation raced with completion.
use super::*;
use std::sync::atomic::Ordering;

#[derive(Clone)]
pub(super) struct Request {
    pub project: libre_effects_core::Project,
    pub frame: u32,
    pub dimension: u32,
    pub revision: u64,
    pub transport: u64,
}
impl Request {
    fn same_context(&self, other: &Self) -> bool {
        self.project == other.project
            && self.dimension == other.dimension
            && self.revision == other.revision
            && self.transport == other.transport
    }
    fn accepts(&self, ready: &Self, playing: bool) -> bool {
        self.same_context(ready)
            && (self.frame == ready.frame || (playing && ready.frame < self.frame))
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Preview {
    pub(super) fn watch_media(cx: &mut Context<Self>) {
        cx.spawn(async move |entity, cx| {
            let mut previous = None;
            loop {
                gpui::Timer::after(std::time::Duration::from_secs(1)).await;
                let Ok((project, revision)) = entity.update(cx, |s, cx| {
                    let state = s.state.read(cx);
                    (state.editor.project().clone(), state.preview_revision)
                }) else {
                    return;
                };
                let snapshot = project.clone();
                let stamp = cx
                    .background_executor()
                    .spawn(async move { crate::preview_cache::media_stamp(&snapshot) })
                    .await;
                let changed = previous
                    .as_ref()
                    .is_some_and(|(p, r, old)| p == &project && *r == revision && old != &stamp);
                if changed {
                    let _ = entity.update(cx, |s, cx| {
                        s.state.update(cx, |state, cx| {
                            if state.editor.project() == &project
                                && state.preview_revision == revision
                            {
                                state.preview_revision = state.preview_revision.wrapping_add(1);
                                cx.notify();
                            }
                        });
                    });
                }
                previous = Some((project, revision, stamp));
            }
        })
        .detach();
    }
    fn finish_warming(&mut self, message: &str, cx: &mut Context<Self>) {
        self.warming = None;
        self.state.update(cx, |s, cx| {
            s.preview_caching = false;
            s.status = message.to_string();
            cx.notify();
        });
    }
    fn publish_cache(&self, cx: &mut Context<Self>) {
        let summary = self.ram.summary();
        if self.state.read(cx).preview_cache != summary {
            self.state.update(cx, |s, cx| {
                s.preview_cache = summary;
                cx.notify();
            });
        }
    }
    pub(super) fn update_render(
        &mut self,
        request: Request,
        playing: bool,
        channel: Channel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        let limit = state.preview_cache_limit;
        let caching = state.preview_caching;
        let work = state.work_start..state.work_end;
        let changed =
            self.ram
                .configure(&request.project, request.dimension, request.revision, limit);
        if changed {
            self.failed = None;
            self.raw = None;
            if let Some((_, _, _, old)) = self.cached.take() {
                let _ = window.drop_image(old);
            }
        }
        if !caching {
            self.warming = None;
        } else if self
            .warming
            .as_ref()
            .is_some_and(|r| !r.same_context(&request))
        {
            self.finish_warming("RAM caching stopped after preview changed", cx);
        } else if self.warming.is_none() {
            self.warming = Some(request.clone());
        }
        if let Some(pending) = &self.pending {
            let prefill = self.warming.is_some() && request.same_context(pending);
            if !prefill && !request.accepts(pending, playing) {
                self.cancel.store(true, Ordering::Release);
            }
        }
        if let Some((ready, result)) = self.ready.take() {
            if request.same_context(&ready) {
                match result {
                    Ok(pixels) => {
                        let pixels = std::sync::Arc::new(pixels);
                        self.ram.insert(ready.frame, pixels.clone());
                        if request.accepts(&ready, playing) {
                            self.cache_pixels(
                                ready.project,
                                ready.frame,
                                ready.dimension,
                                pixels,
                                channel,
                                window,
                            );
                        }
                        self.failed = None;
                    }
                    Err(error) => {
                        if ready.frame == request.frame {
                            self.raw = None;
                            if let Some((_, _, _, old)) = self.cached.take() {
                                let _ = window.drop_image(old);
                            }
                        }
                        if self.warming.is_some() {
                            self.finish_warming(
                                &format!("RAM caching failed at frame {}: {error}", ready.frame),
                                cx,
                            );
                        }
                        self.failed = Some((ready.project, ready.frame, ready.dimension, error));
                    }
                }
            }
        }
        let mut current = self.cached.as_ref().is_some_and(|(p, f, d, _)| {
            p == &request.project && *f == request.frame && *d == request.dimension
        });
        if !current {
            if let Some(pixels) = self.ram.get(request.frame) {
                self.cache_pixels(
                    request.project.clone(),
                    request.frame,
                    request.dimension,
                    pixels,
                    channel,
                    window,
                );
                current = true;
            }
        }
        self.publish_cache(cx);
        if self.pending.is_some() {
            return;
        }
        let failed = self.failed.as_ref().is_some_and(|(p, f, d, _)| {
            p == &request.project && *f == request.frame && *d == request.dimension
        });
        let job = if !current && !failed {
            Some(request)
        } else if self.warming.is_some() {
            match self.ram.next_missing(work) {
                None => {
                    self.finish_warming("Work area cached in RAM", cx);
                    None
                }
                Some(frame) => {
                    let bytes = self.raw.as_ref().map_or(usize::MAX, |p| p.as_raw().len());
                    if self.ram.can_prefill(bytes) {
                        Some(Request { frame, ..request })
                    } else {
                        self.finish_warming(
                            "RAM cache full; reduce preview resolution or increase budget",
                            cx,
                        );
                        None
                    }
                }
            }
        } else {
            None
        };
        if let Some(request) = job {
            self.start_render(request, cx);
        }
    }
    fn start_render(&mut self, request: Request, cx: &mut Context<Self>) {
        self.pending = Some(request.clone());
        self.cancel.store(false, Ordering::Release);
        let renderer = self.renderer.clone();
        let cancel = self.cancel.clone();
        let reset = self.decoder_revision != request.revision;
        self.decoder_revision = request.revision;
        cx.spawn(async move |entity, cx| {
            let job = request.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    if reset {
                        renderer.clear_decoders();
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        renderer.render_preview(&job.project, job.frame, job.dimension)
                    }))
                    .unwrap_or_else(|_| Err("Composition preview failed".into()));
                    if cancel.load(Ordering::Acquire) {
                        renderer.clear_decoders();
                    }
                    result
                })
                .await;
            let _ = entity.update(cx, |s, cx| {
                s.pending = None;
                // A quality/gesture change can return to the original
                // request before cancellation finishes. Never cache a
                // canceled result as that now-valid request's error.
                if !s.cancel.load(Ordering::Acquire) {
                    s.ready = Some((request, result));
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seek_loop_edit_quality_and_refresh_reject_stale_frames() {
        let current = Request {
            project: Default::default(),
            frame: 30,
            dimension: 1280,
            revision: 1,
            transport: 8,
        };
        let mut ready = current.clone();
        ready.frame = 29;
        assert!(current.accepts(&ready, true));
        assert!(!current.accepts(&ready, false));
        ready.transport -= 1;
        assert!(!current.accepts(&ready, true));
        ready = current.clone();
        ready.frame = 100;
        assert!(!current.accepts(&ready, true));
        ready = current.clone();
        ready.dimension = 640;
        assert!(!current.accepts(&ready, true));
        ready = current.clone();
        ready.revision += 1;
        assert!(!current.accepts(&ready, true));
        ready = current.clone();
        let mut editor = libre_effects_core::Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        ready.project = editor.project().clone();
        assert!(!current.accepts(&ready, true));
        assert!(current.accepts(&current, false));
    }
}
