// Copyright 2026 the Libre Effects Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Optional exact reuse of filtered groups after their source pixels are painted.
use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt::Write,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
};

struct Entry {
    key: String,
    width: u32,
    height: u32,
    source: Vec<u8>,
    result: Vec<u8>,
}
impl Entry {
    fn bytes(&self) -> usize {
        self.key.len() + self.source.len() + self.result.len()
    }
}
/// Bounded filter results; the input pixels and all evaluated filter parameters
/// must match. Owning the source bytes prevents stale-pointer/cache collisions.
pub struct FilterCache {
    entries: VecDeque<Entry>,
    bytes: usize,
    limit: usize,
    hits: u64,
}
impl FilterCache {
    /// Create a cache with a hard byte limit, accounting for both pixel buffers.
    pub fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            limit,
            hits: 0,
        }
    }
    /// Number of filters whose complete result was reused.
    pub fn hits(&self) -> u64 {
        self.hits
    }
    /// Release retained filter buffers and reset diagnostics.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.hits = 0;
    }
}
thread_local! { static CURRENT: RefCell<Option<Arc<Mutex<FilterCache>>>> = const { RefCell::new(None) }; }
/// Restores the preceding thread-local filter cache, even during unwinding.
pub struct FilterCacheGuard {
    previous: Option<Arc<Mutex<FilterCache>>>,
    thread: PhantomData<Rc<()>>,
}
impl Drop for FilterCacheGuard {
    fn drop(&mut self) {
        CURRENT.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}
/// Enable exact reuse within this synchronous render scope only.
pub fn install_filter_cache(cache: Option<Arc<Mutex<FilterCache>>>) -> FilterCacheGuard {
    FilterCacheGuard {
        previous: CURRENT.with(|slot| slot.replace(cache)),
        thread: PhantomData,
    }
}
pub(crate) fn key(
    filter: &usvg::filter::Filter,
    transform: tiny_skia::Transform,
    checked: Option<&crate::checked::CheckedState<'_>>,
) -> Option<String> {
    if !CURRENT.with(|slot| slot.borrow().is_some()) {
        return None;
    }
    // SVG Tree's Debug intentionally summarizes nodes; it does not identify
    // feImage gradients/patterns. Those external paint inputs must be evaluated.
    if filter
        .primitives()
        .iter()
        .any(|primitive| matches!(primitive.kind(), usvg::filter::Kind::Image(_)))
    {
        return None;
    }
    struct Bounded(String);
    impl std::fmt::Write for Bounded {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            if self.0.len().saturating_add(value.len()) > 16 * 1024 {
                return Err(std::fmt::Error);
            }
            self.0.push_str(value);
            Ok(())
        }
    }
    let mut key = Bounded(String::new());
    write!(&mut key, "{filter:?}/{transform:?}").ok()?;
    if let Some(state) = checked {
        // A hit cannot bypass a different checked allocation/support contract.
        write!(
            &mut key,
            "/{:?}/{:?}/{}/{}",
            state.options.limits,
            state.options.repeat_edge_domains,
            state.legacy_group_bounds,
            state.live_bytes()
        )
        .ok()?;
    } else {
        key.0.push_str("/unchecked");
    }
    Some(key.0)
}
pub(crate) fn get(key: &str, source: &mut tiny_skia::Pixmap) -> bool {
    CURRENT.with(|slot| {
        let Some(cache) = slot.borrow().clone() else {
            return false;
        };
        let Ok(mut cache) = cache.lock() else {
            return false;
        };
        let Some(index) = cache.entries.iter().position(|entry| {
            entry.key == key
                && entry.width == source.width()
                && entry.height == source.height()
                && entry.source == source.data()
        }) else {
            return false;
        };
        let entry = cache.entries.remove(index).unwrap();
        source.data_mut().copy_from_slice(&entry.result);
        cache.entries.push_back(entry);
        cache.hits += 1;
        true
    })
}
pub(crate) fn source_copy(key: &str, source: &tiny_skia::Pixmap) -> Option<Vec<u8>> {
    let bytes = source.data().len().checked_mul(2)?.checked_add(key.len())?;
    CURRENT.with(|slot| {
        let cache = slot.borrow().clone()?;
        if bytes > cache.lock().ok()?.limit {
            return None;
        }
        Some(source.data().to_vec())
    })
}
pub(crate) fn insert(key: String, source: Vec<u8>, result: &tiny_skia::Pixmap) {
    CURRENT.with(|slot| {
        let Some(cache) = slot.borrow().clone() else {
            return;
        };
        let Ok(mut cache) = cache.lock() else {
            return;
        };
        let bytes = key
            .len()
            .saturating_add(source.len())
            .saturating_add(result.data().len());
        if bytes > cache.limit {
            return;
        }
        // Moving footage needs only the latest input for this filter, rather
        // than displacing every reusable static shadow with historical frames.
        if let Some(index) = cache.entries.iter().position(|entry| {
            entry.key == key && entry.width == result.width() && entry.height == result.height()
        }) {
            cache.bytes -= cache.entries.remove(index).unwrap().bytes();
        }
        while !cache.entries.is_empty()
            && (cache.bytes + bytes > cache.limit || cache.entries.len() >= 64)
        {
            cache.bytes -= cache.entries.pop_front().unwrap().bytes();
        }
        cache.bytes += bytes;
        cache.entries.push_back(Entry {
            key,
            width: result.width(),
            height: result.height(),
            source,
            result: result.data().to_vec(),
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tree(fill: &str, deviation: u32, alpha: f32) -> usvg::Tree {
        usvg::Tree::from_str(&format!("<svg xmlns='http://www.w3.org/2000/svg' width='64' height='48'><defs><filter id='shadow' x='-50%' y='-50%' width='200%' height='200%' color-interpolation-filters='sRGB'><feGaussianBlur stdDeviation='{deviation}'/><feOffset dx='2' dy='3'/><feColorMatrix values='0 0 0 0 0.2 0 0 0 0 0.4 0 0 0 0 0.6 0 0 0 0.7 0'/></filter></defs><g opacity='.81' data-libre-effects-compositing='opaque-opacity-byte257-v1'><rect x='12' y='10' width='25' height='17' fill='{fill}' fill-opacity='{alpha}' filter='url(#shadow)' opacity='.7'/></g></svg>"), &usvg::Options::default()).unwrap()
    }
    fn render(tree: &usvg::Tree, checked: bool) -> tiny_skia::Pixmap {
        let mut pixels = tiny_skia::Pixmap::new(64, 48).unwrap();
        if checked {
            crate::render_checked(
                tree,
                tiny_skia::Transform::identity(),
                &mut pixels.as_mut(),
                &crate::CheckedRenderOptions::default(),
            )
            .unwrap();
        } else {
            crate::render(tree, tiny_skia::Transform::identity(), &mut pixels.as_mut());
        }
        pixels
    }
    #[test]
    fn filters_reuse_complete_results_and_reject_changed_source_parameters_and_checked_contracts() {
        let cache = Arc::new(Mutex::new(FilterCache::new(1024 * 1024)));
        for checked in [false, true] {
            for (fill, deviation, alpha) in [
                ("#224466", 3, 1.0),
                ("#224466", 3, 0.5),
                ("#447788", 5, 0.5),
            ] {
                let tree = tree(fill, deviation, alpha);
                let expected = render(&tree, checked);
                let _scope = install_filter_cache(Some(cache.clone()));
                assert_eq!(render(&tree, checked).data(), expected.data());
                let hits = cache.lock().unwrap().hits();
                assert_eq!(render(&tree, checked).data(), expected.data());
                assert!(cache.lock().unwrap().hits() > hits);
            }
        }
        let _scope = install_filter_cache(Some(cache.clone()));
        let mut pixels = tiny_skia::Pixmap::new(64, 48).unwrap();
        let mut options = crate::CheckedRenderOptions::default();
        options.limits.max_live_bytes = pixels.data().len();
        assert!(crate::render_checked(
            &tree("#447788", 5, 0.5),
            tiny_skia::Transform::identity(),
            &mut pixels.as_mut(),
            &options
        )
        .is_err());
        assert!(cache.lock().unwrap().bytes <= 1024 * 1024);
        cache.lock().unwrap().clear();
        assert_eq!(cache.lock().unwrap().bytes, 0);
    }
}
