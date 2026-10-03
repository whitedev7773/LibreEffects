//! Exactly one compositor task at a time. Seeking cancels its decoder wait;
//! generations reject its result even if cancellation raced with completion.
use super::*;
use std::sync::atomic::Ordering;

// Only snapshots from the same active gradient gesture may display an intermediate frame.
// Frame, resolution, decoder revision and transport still form a strict cancellation boundary.
pub(super) type GradientContext = (u64, u32, u32, u64, u64);

#[derive(Clone)]
pub(super) struct Request {
    pub project: libre_effects_core::Project,
    pub frame: u32,
    pub dimension: u32,
    pub revision: u64,
    pub transport: u64,
    pub gradient_gesture: Option<u64>,
}
impl Request {
    fn gradient_context(&self) -> Option<GradientContext> {
        self.gradient_gesture.map(|id| {
            (
                id,
                self.frame,
                self.dimension,
                self.revision,
                self.transport,
            )
        })
    }
    fn same_gradient(&self, other: &Self) -> bool {
        self.gradient_context().is_some() && self.gradient_context() == other.gradient_context()
    }
    fn discards_gradient_frame(
        &self,
        previous: Option<GradientContext>,
        displayed: Option<(&libre_effects_core::Project, u32, u32)>,
    ) -> bool {
        previous.is_some()
            && previous != self.gradient_context()
            && displayed.is_some_and(|(p, f, d)| {
                p != &self.project || f != self.frame || d != self.dimension
            })
    }
    fn same_context(&self, other: &Self) -> bool {
        self.project == other.project
            && self.gradient_gesture == other.gradient_gesture
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
        let gradient_context = request.gradient_context();
        let keep_gradient_frame =
            gradient_context.is_some() && gradient_context == self.gradient_render_context;
        // A drag can return to its original value before Escape. RAM then already
        // targets the original project, but the displayed intermediate frame may not.
        let stale_gradient_frame = request.discards_gradient_frame(
            self.gradient_render_context,
            self.cached.as_ref().map(|(p, f, d, _)| (p, *f, *d)),
        );
        self.gradient_render_context = gradient_context;
        if changed || stale_gradient_frame {
            self.failed = None;
            if !keep_gradient_frame {
                self.raw = None;
                if let Some((_, _, _, old)) = self.cached.take() {
                    let _ = window.drop_image(old);
                }
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
            if !prefill && !request.accepts(pending, playing) && !request.same_gradient(pending) {
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
            } else if request.same_gradient(&ready) {
                // Complete one in-flight frame while the pointer moves, then start the latest
                // snapshot. Never insert an intermediate project's pixels into the latest RAM cache.
                if let Ok(pixels) = result {
                    self.cache_pixels(
                        ready.project,
                        ready.frame,
                        ready.dimension,
                        std::sync::Arc::new(pixels),
                        channel,
                        window,
                    );
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
            gradient_gesture: None,
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
    #[test]
    fn live_gradient_coalesces_only_its_own_frame_and_never_crosses_cancellation_boundaries() {
        let current = Request {
            project: Default::default(),
            frame: 30,
            dimension: 1280,
            revision: 1,
            transport: 8,
            gradient_gesture: Some(17),
        };
        let mut intermediate = current.clone();
        let mut editor = libre_effects_core::Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        intermediate.project = editor.project().clone();
        assert!(current.same_gradient(&intermediate));
        assert!(!current.same_context(&intermediate));
        assert!(
            !current.accepts(&intermediate, false),
            "intermediate pixels are display-only, not a current cache hit"
        );
        for mutate in [
            |r: &mut Request| r.gradient_gesture = None,
            |r: &mut Request| r.gradient_gesture = Some(18),
            |r: &mut Request| r.frame += 1,
            |r: &mut Request| r.dimension /= 2,
            |r: &mut Request| r.revision += 1,
            |r: &mut Request| r.transport += 1,
        ] {
            let mut next = current.clone();
            mutate(&mut next);
            assert!(!next.same_gradient(&intermediate));
        }
        let mut ordinary = current.clone();
        ordinary.gradient_gesture = None;
        // Cancel/commit can keep the same latest RAM project while a previous
        // intermediate image is displayed. Drop that image across the boundary.
        assert!(ordinary.discards_gradient_frame(
            current.gradient_context(),
            Some((&intermediate.project, 30, 1280))
        ));
        assert!(!ordinary.discards_gradient_frame(
            current.gradient_context(),
            Some((&ordinary.project, 30, 1280))
        ));
        assert!(ordinary.discards_gradient_frame(
            current.gradient_context(),
            Some((&ordinary.project, 29, 1280))
        ));
        assert!(ordinary.discards_gradient_frame(
            current.gradient_context(),
            Some((&ordinary.project, 30, 640))
        ));
        assert!(!current.discards_gradient_frame(
            current.gradient_context(),
            Some((&intermediate.project, 30, 1280))
        ));
        assert!(!ordinary.same_gradient(&ordinary));
        assert!(ordinary.accepts(&ordinary, false));
    }
}
