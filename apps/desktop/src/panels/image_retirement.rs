//! Retire panel images safely for the pinned GPUI 0.2.2 Linux Blade renderer.
//!
//! GPUI 0.2.2's `on_next_frame` callbacks run *before* draw/present despite the
//! method's doc comment. `defer` is only an effect-cycle boundary. Neither alone
//! is an atlas/GPU fence: `BladeAtlas::remove` immediately destroys an unused
//! texture, while pending uploads still name its recyclable texture slot.
//!
//! We use three callback generations, forcing a full refresh at the first two:
//! 1. Rebuild the scene without the retired image and flush any queued uploads.
//! 2. Present again; BladeRenderer waits for the previous submission to finish.
//! 3. Remove the image before drawing. No pending upload, scene or Blade GPU
//!    submission can now reference it. Only then may the atlas recycle its slot.
//!
//! Source ordering: gpui 0.2.2 window.rs (`on_request_frame`, `refresh`, `draw`),
//! view.rs (`AnyView::prepaint`, `paint`), blade_renderer.rs (`draw`). A refresh
//! disables cached view/prepaint reuse, so cached sprites cannot survive step 1.
//! This also handles the initial `Window::draw` that has no matching present.
//! Callback counts are not a public cross-platform GPU fence: recheck this
//! ordering on GPUI upgrades. Other platforms retain their previous immediate
//! image-drop policy until their backend lifetimes are separately verified.
use gpui::{RenderImage, Window};
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::{cell::RefCell, rc::Rc};

/// One queue per panel/window. Remove the image from the panel's render source
/// before retiring it, and never display that image again. All callers retire
/// during render, so a hidden/stalled window also stops producing new batches.
/// On Linux, CPU images are retained for at most three callback generations,
/// with one callback per queue. Window teardown drops callbacks/CPU images;
/// Blade destroys its atlas after waiting for the renderer, without invoking
/// this helper's `drop_image` path.
#[derive(Default)]
pub(super) struct ImageRetirement {
    #[cfg(target_os = "linux")]
    queue: Rc<RefCell<RetirementQueue<Arc<RenderImage>>>>,
}

impl ImageRetirement {
    pub(super) fn retire(&self, image: Arc<RenderImage>, window: &mut Window) {
        #[cfg(target_os = "linux")]
        if self.queue.borrow_mut().retire(image) {
            schedule(self.queue.clone(), window);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = window.drop_image(image);
        }
    }
}

#[cfg(target_os = "linux")]
fn schedule(queue: Rc<RefCell<RetirementQueue<Arc<RenderImage>>>>, window: &Window) {
    window.on_next_frame(move |window, _| {
        let (ready, needs_presentation) = queue.borrow_mut().before_frame();
        for image in ready {
            let _ = window.drop_image(image);
        }
        if needs_presentation {
            // on_next_frame alone does not dirty the scene. Force both a scene
            // rebuild and presentation, even if input/playback has stopped.
            window.refresh();
            schedule(queue, window);
        }
    });
}

/// The same state machine is exercised by the atlas/renderer model below.
/// Each batch advances only at a platform frame callback, never at a CPU draw,
/// cache replacement, timer, or effect-cycle boundary.
#[cfg(any(target_os = "linux", test))]
struct RetirementQueue<T> {
    batches: [Vec<T>; 3],
}

#[cfg(any(target_os = "linux", test))]
impl<T> Default for RetirementQueue<T> {
    fn default() -> Self {
        Self {
            batches: std::array::from_fn(|_| Vec::new()),
        }
    }
}

#[cfg(any(target_os = "linux", test))]
impl<T: PartialEq> RetirementQueue<T> {
    /// Returns whether to schedule the queue's first callback. Later retires
    /// coalesce into it, and duplicate image IDs do not restart their barrier.
    fn retire(&mut self, image: T) -> bool {
        if self.batches.iter().any(|batch| batch.contains(&image)) {
            return false;
        }
        let was_empty = self.batches.iter().all(Vec::is_empty);
        self.batches[0].push(image);
        was_empty
    }

    fn before_frame(&mut self) -> (Vec<T>, bool) {
        let ready = std::mem::take(&mut self.batches[2]);
        self.batches.swap(1, 2);
        self.batches.swap(0, 1);
        let needs_presentation = self.batches.iter().any(|batch| !batch.is_empty());
        (ready, needs_presentation)
    }
}

#[cfg(test)]
mod tests {
    use super::RetirementQueue;
    use std::collections::BTreeMap;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Texture {
        slot: usize,
        generation: usize,
    }

    /// Models the relevant pinned GPUI ordering, including cached scene reuse,
    /// uploads from CPU draws without present, immediate atlas slot recycling,
    /// and the newest GPU submission remaining in flight until the next present.
    #[derive(Default)]
    struct Renderer {
        queue: RetirementQueue<usize>,
        source: Option<usize>,
        slots: Vec<Option<Texture>>,
        free: Vec<usize>,
        keys: BTreeMap<usize, Texture>,
        next_generation: usize,
        initializations: Vec<Texture>,
        uploads: Vec<Texture>,
        scene: Vec<Texture>,
        gpu: Vec<Texture>,
        dropped: Vec<usize>,
        presentations: usize,
        refresh: bool,
    }

    impl Renderer {
        fn replace(&mut self, image: Option<usize>) {
            if let Some(old) = self.source.take() {
                self.queue.retire(old);
            }
            self.source = image;
        }

        fn draw(&mut self) {
            self.scene.clear();
            if let Some(image) = self.source {
                let texture = *self.keys.entry(image).or_insert_with(|| {
                    let slot = self.free.pop().unwrap_or(self.slots.len());
                    self.next_generation += 1;
                    let texture = Texture {
                        slot,
                        generation: self.next_generation,
                    };
                    if slot == self.slots.len() {
                        self.slots.push(Some(texture));
                    } else {
                        self.slots[slot] = Some(texture);
                    }
                    self.initializations.push(texture);
                    self.uploads.push(texture);
                    texture
                });
                self.scene.push(texture);
            }
        }

        fn remove(&mut self, image: usize) {
            if let Some(texture) = self.keys.remove(&image) {
                assert!(!self.initializations.contains(&texture), "pending init");
                assert!(!self.uploads.contains(&texture), "pending upload");
                assert!(!self.scene.contains(&texture), "cached scene reference");
                assert!(!self.gpu.contains(&texture), "GPU still in flight");
                assert_eq!(self.slots[texture.slot].take(), Some(texture));
                self.free.push(texture.slot);
            }
            self.dropped.push(image);
        }

        fn callback(&mut self) -> bool {
            let (ready, needs_presentation) = self.queue.before_frame();
            for image in ready {
                self.remove(image);
            }
            self.refresh |= needs_presentation;
            needs_presentation
        }

        fn present(&mut self) {
            if std::mem::take(&mut self.refresh) {
                self.draw();
            }
            for texture in self
                .initializations
                .iter()
                .chain(&self.uploads)
                .chain(&self.scene)
                .chain(&self.gpu)
            {
                assert_eq!(
                    self.slots[texture.slot],
                    Some(*texture),
                    "destroyed or recycled texture slot"
                );
            }
            self.initializations.clear();
            // submit(new); wait(previous); remember(new)
            self.gpu = std::mem::take(&mut self.uploads);
            self.gpu.extend(self.scene.iter().copied());
            self.presentations += 1;
        }

        fn tick(&mut self) -> bool {
            let again = self.callback();
            if self.refresh {
                self.present();
            }
            again
        }

        fn retained(&self) -> usize {
            self.queue.batches.iter().map(Vec::len).sum()
        }
    }

    #[test]
    fn initial_draw_without_present_needs_two_presentations_before_release() {
        let mut renderer = Renderer::default();
        renderer.replace(Some(1));
        renderer.draw(); // Window::open_window draws before its first present.
        renderer.replace(None);
        assert!(renderer.tick());
        assert!(renderer.dropped.is_empty());
        assert_eq!(renderer.presentations, 1);
        assert!(!renderer.gpu.is_empty()); // Old upload was just submitted.
        assert!(renderer.tick());
        assert!(renderer.dropped.is_empty());
        assert_eq!(renderer.presentations, 2);
        assert!(renderer.gpu.is_empty());
        assert!(!renderer.tick());
        assert_eq!(renderer.dropped, [1]);
        assert_eq!(renderer.presentations, 2); // No perpetual redraw loop.
    }

    #[test]
    fn full_refresh_replaces_cached_sprites_before_release() {
        let mut renderer = Renderer::default();
        renderer.replace(Some(1));
        renderer.draw();
        renderer.present();
        renderer.replace(Some(2));
        let old_scene = renderer.scene.clone();
        renderer.tick();
        assert_ne!(renderer.scene, old_scene);
        assert_eq!(renderer.source, Some(2));
        renderer.tick();
        renderer.tick();
        assert_eq!(renderer.dropped, [1]);
        assert!(renderer.keys.contains_key(&2));
    }

    #[test]
    fn same_frame_replacements_and_duplicate_retires_coalesce() {
        let mut queue = RetirementQueue::default();
        assert!(queue.retire(1));
        for id in 1..100 {
            assert!(!queue.retire(id));
        }
        assert_eq!(queue.before_frame(), (Vec::new(), true));
        assert!(!queue.retire(1)); // A duplicate does not reset its age.
        assert_eq!(queue.before_frame(), (Vec::new(), true));
        assert_eq!(queue.before_frame(), ((1..100).collect(), false));
        assert_eq!(queue.before_frame(), (Vec::new(), false));
        assert!(queue.retire(100)); // An idle queue can be restarted.
    }

    #[test]
    fn repeated_draws_before_present_keep_every_pending_upload_alive() {
        let mut renderer = Renderer::default();
        for image in 1..20 {
            renderer.replace(Some(image));
            renderer.draw();
        }
        renderer.replace(None); // Cache clearing also retires the latest image.
        assert_eq!(renderer.uploads.len(), 19);
        assert_eq!(renderer.retained(), 19);
        renderer.tick();
        renderer.tick();
        renderer.tick();
        assert_eq!(renderer.dropped, (1..20).collect::<Vec<_>>());
        assert!(renderer.slots.iter().all(Option::is_none));
    }

    #[test]
    fn texture_id_is_recycled_only_after_uploads_and_gpu_use_complete() {
        let mut renderer = Renderer::default();
        renderer.replace(Some(1));
        renderer.draw();
        let old = renderer.keys[&1];
        renderer.replace(Some(2));
        renderer.tick();
        assert_ne!(renderer.keys[&2].slot, old.slot);
        renderer.tick();
        renderer.tick();
        renderer.replace(Some(3));
        renderer.draw();
        let reused = renderer.keys[&3];
        assert_eq!(reused.slot, old.slot);
        assert_ne!(reused.generation, old.generation);
        renderer.tick();
        renderer.tick();
        renderer.tick();
        assert_eq!(renderer.dropped, [1, 2]);
    }

    #[test]
    fn sustained_updates_keep_only_three_frame_generations() {
        let mut renderer = Renderer::default();
        for frame in 0..200 {
            // A render can replace pixels, then replace the channel display.
            for change in 0..2 {
                renderer.replace(Some(frame * 2 + change));
                renderer.draw();
            }
            assert!(renderer.retained() <= 6);
            renderer.tick();
        }
        renderer.replace(None);
        renderer.tick();
        renderer.tick();
        assert!(!renderer.tick());
        assert_eq!(renderer.retained(), 0);
        assert_eq!(renderer.dropped.len(), 400);
    }

    #[test]
    fn replacements_during_barrier_draws_keep_their_own_full_lifetime() {
        let mut renderer = Renderer::default();
        renderer.replace(Some(1));
        renderer.draw();
        renderer.replace(Some(2));
        assert!(renderer.callback());
        // The refresh-triggered render may itself accept fresh pixels or change
        // display channels. Its new retirees must not inherit the older age.
        renderer.replace(Some(3));
        renderer.present();
        assert!(renderer.callback());
        renderer.replace(Some(4));
        renderer.present();
        assert!(renderer.tick());
        assert_eq!(renderer.dropped, [1]);
        assert!(renderer.tick());
        assert_eq!(renderer.dropped, [1, 2]);
        assert!(!renderer.callback());
        assert_eq!(renderer.dropped, [1, 2, 3]);
        // After the draining callback, an independently dirty frame can render
        // and retire another image. An empty queue must schedule a new chain.
        let old = renderer.source.take().unwrap();
        assert!(renderer.queue.retire(old));
        renderer.source = Some(5);
        renderer.draw();
        renderer.present();
        assert!(renderer.tick());
        assert!(renderer.tick());
        assert!(!renderer.tick());
        assert_eq!(renderer.dropped, [1, 2, 3, 4]);
        assert!(renderer.keys.contains_key(&5));
    }

    #[test]
    fn stalled_window_does_not_advance_or_release_and_close_drops_cpu_owners() {
        use std::rc::Rc;
        let image = Rc::new(1);
        let weak = Rc::downgrade(&image);
        let queue = Rc::new(std::cell::RefCell::new(RetirementQueue::default()));
        queue.borrow_mut().retire(image);
        let callback_owner = queue.clone();
        drop(queue); // Panel owner can disappear before the callback runs.
        assert!(weak.upgrade().is_some());
        assert_eq!(callback_owner.borrow().batches[0].len(), 1);
        // Closing the window destroys its pending callback. There is no timer,
        // self-cycle or destructor that calls into a dead window/atlas.
        drop(callback_owner);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    #[should_panic(expected = "pending init")]
    fn model_detects_the_original_immediate_drop_bug() {
        let mut renderer = Renderer::default();
        renderer.replace(Some(1));
        renderer.draw();
        renderer.remove(1);
    }

    #[test]
    #[should_panic(expected = "GPU still in flight")]
    fn model_rejects_a_drop_after_only_one_presentation() {
        let mut renderer = Renderer::default();
        renderer.replace(Some(1));
        renderer.draw();
        renderer.replace(None);
        renderer.tick();
        renderer.remove(1);
    }
}
