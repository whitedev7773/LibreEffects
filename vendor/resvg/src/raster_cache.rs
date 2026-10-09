// Copyright 2026 the Libre Effects Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Bounded, opt-in reuse of decoded raster pixels and exact area reductions.
use std::{
    cell::RefCell,
    collections::VecDeque,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
};

struct Entry {
    format: u8,
    source: Arc<Vec<u8>>,
    reductions: Vec<(u32, u32)>,
    pixels: Arc<tiny_skia::Pixmap>,
}
impl Entry {
    fn bytes(&self) -> usize {
        self.source.len() + self.pixels.data().len()
    }
}

/// A cache belongs to a renderer, with no global ownership of project media.
pub struct RasterImageCache {
    entries: VecDeque<Entry>,
    bytes: usize,
    limit: usize,
}
impl RasterImageCache {
    /// Create a cache with a hard retained-byte ceiling (including compressed data).
    pub fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            limit,
        }
    }
    /// Release retained source images and all reduction levels.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}
thread_local! { static CURRENT: RefCell<Option<Arc<Mutex<RasterImageCache>>>> = const { RefCell::new(None) }; }

/// Restores the caller's cache on drop; scopes never transfer between threads.
pub struct RasterImageCacheGuard {
    previous: Option<Arc<Mutex<RasterImageCache>>>,
    thread: PhantomData<Rc<()>>,
}
impl Drop for RasterImageCacheGuard {
    fn drop(&mut self) {
        CURRENT.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}
/// Share decoded images between synchronous renders without copying their pixels.
pub fn install_raster_image_cache(
    cache: Option<Arc<Mutex<RasterImageCache>>>,
) -> RasterImageCacheGuard {
    RasterImageCacheGuard {
        previous: CURRENT.with(|slot| slot.replace(cache)),
        thread: PhantomData,
    }
}
fn source(kind: &usvg::ImageKind) -> Option<(u8, &Arc<Vec<u8>>)> {
    match kind {
        usvg::ImageKind::PNG(data) => Some((0, data)),
        usvg::ImageKind::JPEG(data) => Some((1, data)),
        usvg::ImageKind::GIF(data) => Some((2, data)),
        usvg::ImageKind::WEBP(data) => Some((3, data)),
        usvg::ImageKind::SVG(_) => None,
    }
}
pub(crate) fn get(
    kind: &usvg::ImageKind,
    reductions: &[(u32, u32)],
) -> Option<Arc<tiny_skia::Pixmap>> {
    let (format, source) = source(kind)?;
    CURRENT.with(|slot| {
        let cache = slot.borrow().clone()?;
        let mut cache = cache.lock().ok()?;
        // Full bytes are compared: pointer reuse and hash collisions cannot return stale media.
        let index = cache.entries.iter().position(|entry| {
            entry.format == format && entry.reductions == reductions && entry.source == *source
        })?;
        let entry = cache.entries.remove(index)?;
        let pixels = entry.pixels.clone();
        cache.entries.push_back(entry);
        Some(pixels)
    })
}
pub(crate) fn insert(
    kind: &usvg::ImageKind,
    reductions: &[(u32, u32)],
    pixels: Arc<tiny_skia::Pixmap>,
) {
    let Some((format, source)) = source(kind) else {
        return;
    };
    CURRENT.with(|slot| {
        let Some(cache) = slot.borrow().clone() else {
            return;
        };
        let Ok(mut cache) = cache.lock() else {
            return;
        };
        let bytes = source.len().saturating_add(pixels.data().len());
        if bytes > cache.limit {
            return;
        }
        if cache.entries.iter().any(|entry| {
            entry.format == format && entry.reductions == reductions && entry.source == *source
        }) {
            return;
        }
        while !cache.entries.is_empty()
            && (cache.bytes + bytes > cache.limit || cache.entries.len() >= 128)
        {
            cache.bytes -= cache.entries.pop_front().unwrap().bytes();
        }
        cache.bytes += bytes;
        cache.entries.push_back(Entry {
            format,
            source: source.clone(),
            reductions: reductions.to_vec(),
            pixels,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_geometry_reuse_keeps_current_paint_transforms_clipping_and_pixels() {
        let cache = Arc::new(Mutex::new(usvg::PathCache::new(1024 * 1024)));
        for (d, color, x) in [
            ("M2 3c3 -2 9 2 14 5q-3 9 -10 14z", "#246080", 0),
            ("M2 3c3 -2 9 2 14 5q-3 9 -10 14z", "#804020", 7),
            ("M2 3c3 -2 9 2 14 5q-3 9 -11 14z", "#204080", 3),
            ("M2 3h14v18z invalid", "#206080", 2),
            ("M2 3h14v18z invalid", "#806020", 9),
        ] {
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><defs><clipPath id="c"><path d="M0 0h40v40h-40z"/></clipPath></defs><g clip-path="url(#c)" transform="translate({x} 0)"><path d="{d}" fill="{color}" fill-opacity=".63"/><path d="{d}" transform="translate(25 10)" fill="#732055"/></g></svg>"##
            );
            let mut expected = tiny_skia::Pixmap::new(64, 48).unwrap();
            {
                let _none = usvg::install_path_cache(None);
                let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
                crate::render(
                    &tree,
                    tiny_skia::Transform::identity(),
                    &mut expected.as_mut(),
                );
            }
            let _scope = usvg::install_path_cache(Some(cache.clone()));
            let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
            let mut actual = tiny_skia::Pixmap::new(64, 48).unwrap();
            crate::render(
                &tree,
                tiny_skia::Transform::identity(),
                &mut actual.as_mut(),
            );
            assert_eq!(actual.data(), expected.data());
        }
        assert!(cache.lock().unwrap().hits() >= 5);
    }
    #[test]
    fn content_replacement_format_and_size_do_not_alias_and_eviction_is_bounded() {
        let cache = Arc::new(Mutex::new(RasterImageCache::new(24)));
        let _scope = install_raster_image_cache(Some(cache.clone()));
        let a = usvg::ImageKind::PNG(Arc::new(vec![1; 4]));
        let b = usvg::ImageKind::PNG(Arc::new(vec![2; 4]));
        let pixels = Arc::new(tiny_skia::Pixmap::new(2, 2).unwrap());
        insert(&a, &[], pixels.clone());
        assert!(Arc::ptr_eq(&get(&a, &[]).unwrap(), &pixels));
        assert!(get(&a, &[(1, 1)]).is_none());
        assert!(get(&usvg::ImageKind::JPEG(Arc::new(vec![1; 4])), &[]).is_none());
        assert!(get(&b, &[]).is_none());
        insert(&b, &[], pixels);
        assert!(get(&a, &[]).is_none());
        assert_eq!(cache.lock().unwrap().bytes, 20);
        {
            let _nested = install_raster_image_cache(None);
            assert!(get(&b, &[]).is_none());
        }
        assert!(get(&b, &[]).is_some());
        std::thread::spawn(move || assert!(get(&b, &[]).is_none()))
            .join()
            .unwrap();
        cache.lock().unwrap().clear();
        assert_eq!(cache.lock().unwrap().bytes, 0);
    }
}
