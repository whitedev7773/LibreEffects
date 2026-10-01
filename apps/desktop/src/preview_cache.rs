//! Bounded, composition-space RGBA frames. Viewer channels and transport are
//! deliberately outside the key; edits, quality and explicit refresh are not.
use image::RgbaImage;
use libre_effects_core::Project;
use std::{collections::BTreeMap, ops::Range, sync::Arc};

pub(crate) const MIB: usize = 1024 * 1024;
const MAX_FRAMES: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    pub ranges: Vec<Range<u32>>,
    pub bytes: usize,
    pub frames: usize,
    pub hits: u64,
}

#[derive(Default)]
pub(crate) struct Cache {
    context: Option<(Project, u32, u64)>,
    frames: BTreeMap<u32, (Arc<RgbaImage>, u64)>,
    bytes: usize,
    limit: usize,
    clock: u64,
    hits: u64,
}
impl Cache {
    /// Returns true when every previously rendered frame has become invalid.
    pub fn configure(
        &mut self,
        project: &Project,
        dimension: u32,
        revision: u64,
        limit: usize,
    ) -> bool {
        let changed = self
            .context
            .as_ref()
            .is_none_or(|(p, d, r)| p != project || *d != dimension || *r != revision);
        if changed {
            self.frames.clear();
            self.bytes = 0;
            self.hits = 0;
            self.context = Some((project.clone(), dimension, revision));
        }
        self.limit = limit;
        self.evict_to(limit);
        changed
    }
    fn evict_to(&mut self, bytes: usize) {
        while self.bytes > bytes {
            let Some(frame) = self
                .frames
                .iter()
                .min_by_key(|(_, (_, age))| *age)
                .map(|(f, _)| *f)
            else {
                break;
            };
            self.bytes -= self.frames.remove(&frame).unwrap().0.as_raw().len();
        }
    }
    pub fn get(&mut self, frame: u32) -> Option<Arc<RgbaImage>> {
        let (pixels, age) = self.frames.get_mut(&frame)?;
        self.clock = self.clock.wrapping_add(1);
        *age = self.clock;
        self.hits = self.hits.saturating_add(1);
        Some(pixels.clone())
    }
    pub fn insert(&mut self, frame: u32, pixels: Arc<RgbaImage>) {
        let bytes = pixels.as_raw().len();
        if bytes > self.limit || self.limit == 0 {
            return;
        }
        if let Some((old, _)) = self.frames.remove(&frame) {
            self.bytes -= old.as_raw().len();
        }
        if self.frames.len() >= MAX_FRAMES {
            self.evict_to(self.bytes.saturating_sub(1));
        }
        self.evict_to(self.limit - bytes);
        self.clock = self.clock.wrapping_add(1);
        self.bytes += bytes;
        self.frames.insert(frame, (pixels, self.clock));
    }
    pub fn next_missing(&self, range: Range<u32>) -> Option<u32> {
        range
            .into_iter()
            .find(|frame| !self.frames.contains_key(frame))
    }
    /// Pre-rendering must stop at capacity instead of endlessly evicting its
    /// own beginning. On-demand playback/seeking can still use ordinary LRU.
    pub fn can_prefill(&self, frame_bytes: usize) -> bool {
        self.limit > 0
            && self.frames.len() < MAX_FRAMES
            && frame_bytes <= self.limit.saturating_sub(self.bytes)
    }
    pub fn summary(&self) -> Summary {
        let mut ranges: Vec<Range<u32>> = Vec::new();
        for &frame in self.frames.keys() {
            if let Some(last) = ranges.last_mut().filter(|last| last.end == frame) {
                last.end = frame.saturating_add(1);
            } else {
                ranges.push(frame..frame.saturating_add(1));
            }
        }
        Summary {
            ranges,
            bytes: self.bytes,
            frames: self.frames.len(),
            hits: self.hits,
        }
    }
}

/// Filesystem work stays off the UI thread, including large sequence manifests.
/// Refresh Footage remains available for replacements preserving size and mtime.
pub(crate) fn media_stamp(
    project: &Project,
) -> Vec<(String, Option<(u64, std::time::SystemTime)>)> {
    crate::media_io::video_paths(project)
        .into_iter()
        .map(|path| {
            let stamp = std::fs::metadata(&path)
                .ok()
                .and_then(|m| Some((m.len(), m.modified().ok()?)));
            (path, stamp)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixels() -> Arc<RgbaImage> {
        Arc::new(RgbaImage::from_pixel(2, 2, image::Rgba([32, 64, 128, 127])))
    }
    #[test]
    fn linked_file_changes_deletion_and_return_change_the_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.mp4");
        let mut editor = libre_effects_core::Editor::default();
        editor
            .execute(libre_effects_core::Command::AddContent {
                content: libre_effects_core::Content::Video {
                    audio: None,
                    path: path.to_str().unwrap().into(),
                    duration: 1.0,
                    source_fps: 30.0,
                    start_frame: 0,
                    playback: Default::default(),
                },
                width: 20.0,
                height: 20.0,
                name: "Linked source".into(),
            })
            .unwrap();
        let missing = media_stamp(editor.project());
        std::fs::write(&path, b"first").unwrap();
        let first = media_stamp(editor.project());
        assert_ne!(missing, first);
        std::fs::write(&path, b"replacement").unwrap();
        assert_ne!(first, media_stamp(editor.project()));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(missing, media_stamp(editor.project()));
    }
    #[test]
    fn cached_composites_match_direct_frames_after_save_and_reopen() {
        let renderer = crate::rendering::Renderer::new();
        let project =
            Project::from_json(include_str!("../../../examples/adjustment-study.lfe.json"))
                .unwrap();
        let restored = Project::from_json(&project.to_json().unwrap()).unwrap();
        let mut cache = Cache::default();
        cache.configure(&restored, 160, 0, 2 * MIB);
        for frame in 0..30 {
            cache.insert(
                frame,
                Arc::new(renderer.render_preview(&restored, frame, 160).unwrap()),
            );
        }
        for frame in (0..30).rev() {
            assert_eq!(
                *cache.get(frame).unwrap(),
                renderer.render_preview(&restored, frame, 160).unwrap()
            );
        }
        assert_eq!(cache.summary().ranges, vec![0..30]);
        assert_eq!(cache.summary().hits, 30);
    }
    #[test]
    fn tiny_frames_also_have_a_metadata_count_limit() {
        let mut cache = Cache::default();
        cache.configure(&Project::default(), 2, 0, MIB);
        for frame in 0..MAX_FRAMES as u32 + 1 {
            cache.insert(frame, pixels());
        }
        assert_eq!(cache.summary().frames, MAX_FRAMES);
        assert!(cache.get(0).is_none());
        assert!(!cache.can_prefill(16));
    }
    #[test]
    fn lru_is_byte_bounded_and_shares_alpha_pixels() {
        let mut cache = Cache::default();
        cache.configure(&Project::default(), 2, 0, 32);
        let original = pixels();
        cache.insert(0, original.clone());
        cache.insert(1, pixels());
        assert!(Arc::ptr_eq(&cache.get(0).unwrap(), &original));
        cache.insert(2, pixels());
        assert!(cache.get(1).is_none());
        assert_eq!(cache.summary().ranges, vec![0..1, 2..3]);
        assert_eq!(cache.summary().bytes, 32);
        assert_eq!(cache.get(0).unwrap().get_pixel(0, 0).0, [32, 64, 128, 127]);
        cache.configure(&Project::default(), 2, 0, 15);
        cache.insert(3, pixels());
        assert_eq!(cache.summary().bytes, 0);
    }
    #[test]
    fn edits_quality_refresh_and_disabled_cache_invalidate() {
        let mut cache = Cache::default();
        let mut editor = libre_effects_core::Editor::default();
        cache.configure(editor.project(), 2, 0, 64);
        cache.insert(0, pixels());
        assert!(!cache.configure(editor.project(), 2, 0, 64));
        editor
            .execute(libre_effects_core::Command::AddRectangle)
            .unwrap();
        assert!(cache.configure(editor.project(), 2, 0, 64));
        assert!(cache.get(0).is_none());
        cache.insert(0, pixels());
        editor.undo();
        assert!(cache.configure(editor.project(), 2, 0, 64));
        cache.insert(0, pixels());
        assert!(cache.configure(editor.project(), 1, 0, 64));
        cache.insert(0, pixels());
        assert!(cache.configure(editor.project(), 1, 1, 64));
        cache.insert(0, pixels());
        cache.configure(editor.project(), 1, 1, 0);
        cache.insert(1, pixels());
        assert_eq!(cache.summary().frames, 0);
    }
    #[test]
    fn prefill_stops_at_capacity_and_ranges_use_exclusive_ends() {
        let mut cache = Cache::default();
        cache.configure(&Project::default(), 2, 0, 48);
        for f in [11, 10, 13] {
            cache.insert(f, pixels());
        }
        assert_eq!(cache.next_missing(10..15), Some(12));
        assert!(!cache.can_prefill(16));
        assert_eq!(cache.summary().ranges, vec![10..12, 13..14]);
        cache.configure(&Project::default(), 2, 0, 64);
        assert!(cache.can_prefill(16));
        cache.insert(12, pixels());
        assert_eq!(cache.next_missing(10..14), None);
        assert_eq!(cache.summary().ranges, vec![10..14]);
    }
}
