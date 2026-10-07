//! Exactly one compositor task at a time. Seeking cancels its decoder wait;
//! generations reject its result even if cancellation raced with completion.
use super::*;
use std::sync::atomic::Ordering;

pub(super) use libre_effects_editor_model::preview_scene::{
    GradientContext, PreviewRequest as Request,
};
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
        let stale_receipt = self.displayed.as_ref().is_some_and(|(shown, _)| {
            shown.document_revision != request.document_revision
                || shown.core_generation != request.core_generation
                || shown.transport != request.transport
        });
        if changed || stale_gradient_frame || stale_receipt {
            self.failed = None;
            if !keep_gradient_frame {
                self.raw = None;
                self.displayed = None;
                if let Some((_, _, _, old)) = self.cached.take() {
                    self.retired_images.retire(old, window);
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
            // A successful pixel render cannot erase a missing/stale geometry failure.
            let result = result.and_then(|rendered| {
                ready.validate_evaluated_view(rendered.evaluated.as_deref())?;
                Ok(rendered)
            });
            if request.same_context(&ready) {
                match result {
                    Ok(rendered) => {
                        let pixels = std::sync::Arc::new(rendered.pixels);
                        self.ram.insert(ready.frame, pixels.clone());
                        if request.accepts(&ready, playing) {
                            self.cache_pixels(ready, rendered.evaluated, pixels, channel, window);
                        }
                        self.failed = None;
                    }
                    Err(error) => {
                        if ready.frame == request.frame {
                            self.raw = None;
                            self.displayed = None;
                            if let Some((_, _, _, old)) = self.cached.take() {
                                self.retired_images.retire(old, window);
                            }
                        }
                        if self.warming.is_some() {
                            self.finish_warming(
                                &format!("RAM caching failed at frame {}: {error}", ready.frame),
                                cx,
                            );
                        }
                        self.failed = Some((ready, error));
                    }
                }
            } else if request.same_gradient(&ready) {
                // Complete one in-flight frame while the pointer moves, then start the latest
                // snapshot. Never insert an intermediate project's pixels into the latest RAM cache.
                if let Ok(rendered) = result {
                    self.cache_pixels(
                        ready,
                        rendered.evaluated,
                        std::sync::Arc::new(rendered.pixels),
                        channel,
                        window,
                    );
                }
            }
        }
        let mut current = self.displayed.as_ref().is_some_and(|(shown, view)| {
            request.current_geometry(shown, view.as_deref()).is_some()
        });
        // RAM stores only pixels. Expression scenes must produce a matching current
        // view again rather than retaining an entire evaluated project per cached frame.
        if !current && !request.needs_evaluated_view() {
            if let Some(pixels) = self.ram.get(request.frame) {
                self.cache_pixels(request.clone(), None, pixels, channel, window);
                current = true;
            }
        }
        self.publish_cache(cx);
        if self.pending.is_some() {
            return;
        }
        let failed = self
            .failed
            .as_ref()
            .is_some_and(|(failed, _)| request.accepts(failed, false));
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
                        renderer.render_preview_with_view(&job.project, job.frame, job.dimension)
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
    fn trim_failure_survives_preview_worker_transfer_and_stale_request_is_rejected() {
        let e = crate::rendering::trim_tests::partial_scene();
        let request = Request {
            project: e.project().clone(),
            frame: 0,
            dimension: 200,
            revision: 4,
            document_revision: 2,
            core_generation: 1,
            transport: 7,
            gradient_gesture: None,
        };
        let renderer = crate::rendering::with_test_contents_budget(
            libre_effects_core::ContentsRenderBudget {
                frame_work_limit: 0,
                ..Default::default()
            },
            crate::rendering::Renderer::new,
        );
        let job = request.clone();
        let result = std::thread::spawn(move || {
            renderer.render_preview(&job.project, job.frame, job.dimension)
        })
        .join()
        .unwrap();
        let error = result.unwrap_err();
        assert!(
            error.contains("WorkLimit") && error.contains("Trim line"),
            "{error}"
        );
        assert!(request.accepts(&request, false));
        let mut superseded = request.clone();
        superseded.revision += 1;
        assert!(!superseded.accepts(&request, false));
        superseded = request.clone();
        superseded.transport += 1;
        assert!(!superseded.accepts(&request, false));
        // A cancelled render remains an error across the same worker boundary,
        // allowing start_render's cancellation guard to discard it, never cache it.
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let renderer = crate::rendering::Renderer::with_cancel(cancel);
        let error = std::thread::spawn(move || {
            renderer.render_preview(&request.project, request.frame, request.dimension)
        })
        .join()
        .unwrap()
        .unwrap_err();
        assert!(error.to_lowercase().contains("cancel"), "{error}");
    }
    #[test]
    fn seek_loop_edit_quality_and_refresh_reject_stale_frames() {
        let current = Request {
            project: Default::default(),
            frame: 30,
            dimension: 1280,
            revision: 1,
            document_revision: 2,
            core_generation: 1,
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
            document_revision: 2,
            core_generation: 1,
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
            |r: &mut Request| r.document_revision += 1,
            |r: &mut Request| r.core_generation += 1,
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
