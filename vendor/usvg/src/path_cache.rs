// Copyright 2026 the Libre Effects Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Opt-in reuse of immutable geometry for identical SVG path data.
use std::{
    cell::RefCell,
    collections::VecDeque,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
};
use tiny_skia_path::Path;

#[derive(Debug)]
struct Entry {
    source: Box<str>,
    path: Arc<Path>,
    bytes: usize,
}
/// A renderer-owned cache. Only the complete `d` value qualifies geometry;
/// paint, CSS, transforms, clipping and filter resolution remain uncached.
#[derive(Debug)]
pub struct PathCache {
    entries: VecDeque<Entry>,
    bytes: usize,
    limit: usize,
    hits: u64,
}
impl PathCache {
    /// Set a retained-payload ceiling; at most 1024 entries are admitted.
    pub fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            limit,
            hits: 0,
        }
    }
    /// Release retained geometry and reset diagnostic hit counts.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.hits = 0;
    }
    /// Number of geometry reuses since the last clear.
    pub fn hits(&self) -> u64 {
        self.hits
    }
    fn get(&mut self, source: &str) -> Option<Arc<Path>> {
        let i = self
            .entries
            .iter()
            .position(|e| e.source.as_ref() == source)?;
        let entry = self.entries.remove(i)?;
        let path = entry.path.clone();
        self.entries.push_back(entry);
        self.hits = self.hits.saturating_add(1);
        Some(path)
    }
    fn insert(&mut self, source: &str, path: Arc<Path>) {
        // PathBuilder grows its point/verb vectors geometrically. Include two
        // payload lengths plus fixed small-vector/header slack in admission.
        let Some(bytes) = path
            .points()
            .len()
            .checked_mul(std::mem::size_of::<tiny_skia_path::Point>())
            .and_then(|n| {
                n.checked_add(path.verbs().len() * std::mem::size_of::<tiny_skia_path::PathVerb>())
            })
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(source.len() + 256))
        else {
            return;
        };
        if source.len() > 8 * 1024 * 1024 || bytes > self.limit {
            return;
        }
        let mut owned = String::new();
        if owned.try_reserve_exact(source.len()).is_err() || self.entries.try_reserve(1).is_err() {
            return;
        }
        owned.push_str(source);
        while !self.entries.is_empty()
            && (self.bytes.saturating_add(bytes) > self.limit || self.entries.len() >= 1024)
        {
            self.bytes -= self.entries.pop_front().unwrap().bytes;
        }
        self.bytes += bytes;
        self.entries.push_back(Entry {
            source: owned.into_boxed_str(),
            path,
            bytes,
        });
    }
}
thread_local! { static CURRENT:RefCell<Option<Arc<Mutex<PathCache>>>>=const {RefCell::new(None)}; }
/// Restores the previous thread-local cache, including nested renders/unwinding.
#[derive(Debug)]
pub struct PathCacheGuard {
    previous: Option<Arc<Mutex<PathCache>>>,
    thread: PhantomData<Rc<()>>,
}
impl Drop for PathCacheGuard {
    fn drop(&mut self) {
        CURRENT.with(|s| *s.borrow_mut() = self.previous.take());
    }
}
/// Select geometry reuse only for this synchronous parse/render scope.
pub fn install_path_cache(cache: Option<Arc<Mutex<PathCache>>>) -> PathCacheGuard {
    PathCacheGuard {
        previous: CURRENT.with(|s| s.replace(cache)),
        thread: PhantomData,
    }
}
pub(crate) fn get(source: &str) -> Option<Arc<Path>> {
    let cache = CURRENT.with(|s| s.borrow().clone())?;
    let result = cache.lock().ok()?.get(source);
    result
}
pub(crate) fn insert(source: &str, path: Arc<Path>) {
    if let Some(cache) = CURRENT.with(|s| s.borrow().clone()) {
        if let Ok(mut cache) = cache.lock() {
            cache.insert(source, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_source_identity_eviction_clear_and_scope_preserve_geometry() {
        let path = Arc::new(tiny_skia_path::PathBuilder::from_rect(
            tiny_skia_path::Rect::from_xywh(0.0, 0.0, 3.0, 4.0).unwrap(),
        ));
        let cache = Arc::new(Mutex::new(PathCache::new(1024)));
        {
            let _outer = install_path_cache(Some(cache.clone()));
            insert("M0 0h3v4z", path.clone());
            assert!(Arc::ptr_eq(&get("M0 0h3v4z").unwrap(), &path));
            assert!(get("M0 0h4v4z").is_none());
            {
                let _inner = install_path_cache(None);
                assert!(get("M0 0h3v4z").is_none());
            }
            assert!(get("M0 0h3v4z").is_some());
            std::thread::spawn(|| assert!(get("M0 0h3v4z").is_none()))
                .join()
                .unwrap();
            insert(&" ".repeat(2048), path.clone());
            assert!(cache.lock().unwrap().bytes <= 1024);
            for i in 0..100 {
                insert(&format!("M{i} 0h3v4z"), path.clone());
            }
            assert!(get("M0 0h3v4z").is_none());
            cache.lock().unwrap().clear();
            assert!(get("M99 0h3v4z").is_none());
        }
        assert!(get("M0 0h3v4z").is_none());
    }
}
