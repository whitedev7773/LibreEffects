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
    pub(super) fn update_render(
        &mut self,
        request: Request,
        playing: bool,
        channel: Channel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.revision != request.revision {
            self.revision = request.revision;
            self.failed = None;
            if let Some((_, _, _, old)) = self.cached.take() {
                let _ = window.drop_image(old);
            }
        }
        if let Some(pending) = &self.pending {
            if !request.accepts(pending, playing) {
                self.cancel.store(true, Ordering::Release);
            }
        }
        if let Some((ready, result)) = self.ready.take() {
            if request.accepts(&ready, playing) {
                match result {
                    Ok(pixels) => {
                        self.cache_pixels(
                            ready.project,
                            ready.frame,
                            ready.dimension,
                            pixels,
                            channel,
                            window,
                        );
                        self.failed = None;
                    }
                    Err(error) => {
                        if let Some((_, _, _, old)) = self.cached.take() {
                            let _ = window.drop_image(old);
                        }
                        self.failed = Some((ready.project, ready.frame, ready.dimension, error));
                    }
                }
            }
        }
        let current = self.cached.as_ref().is_some_and(|(p, f, d, _)| {
            p == &request.project && *f == request.frame && *d == request.dimension
        });
        if !current {
            if self
                .cached
                .as_ref()
                .is_some_and(|(p, _, d, _)| p != &request.project || *d != request.dimension)
            {
                if let Some((_, _, _, old)) = self.cached.take() {
                    let _ = window.drop_image(old);
                }
            }
            let failed = self.failed.as_ref().is_some_and(|(p, f, d, _)| {
                p == &request.project && *f == request.frame && *d == request.dimension
            });
            if self.pending.is_none() && !failed {
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
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
