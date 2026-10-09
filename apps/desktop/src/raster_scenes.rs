//! Exact intermediate composites, keyed by rendered inputs rather than frame
//! numbers. Animated geometry, footage, filter domains and dimensions all count.
use resvg::{RepeatEdgeDomain, tiny_skia::Pixmap};
use std::{collections::VecDeque, sync::Arc};

const LIMIT: usize = 128 * 1024 * 1024;
struct Entry {
    svg: String,
    dimensions: [u32; 4],
    domains: Vec<RepeatEdgeDomain>,
    resources: Vec<Arc<Vec<u8>>>,
    pixels: Pixmap,
    bytes: usize,
}
#[derive(Default)]
pub(crate) struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
}
fn same_domains(left: &[RepeatEdgeDomain], right: &[RepeatEdgeDomain]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.filter_id == b.filter_id
                && a.primitive_index == b.primitive_index
                && a.rect == b.rect
                && a.transform == b.transform
        })
}
impl Cache {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub(crate) fn get(
        &mut self,
        svg: &str,
        dimensions: [u32; 4],
        domains: &[RepeatEdgeDomain],
        resources: &[Arc<Vec<u8>>],
    ) -> Option<Pixmap> {
        let index = self.entries.iter().position(|entry| {
            entry.dimensions == dimensions
                && entry.svg == svg
                && same_domains(&entry.domains, domains)
                && entry.resources == resources
        })?;
        let entry = self.entries.remove(index)?;
        let pixels = entry.pixels.clone();
        self.entries.push_back(entry);
        Some(pixels)
    }
    pub(crate) fn insert(
        &mut self,
        svg: &str,
        dimensions: [u32; 4],
        domains: &[RepeatEdgeDomain],
        resources: Vec<Arc<Vec<u8>>>,
        pixels: &Pixmap,
    ) {
        let bytes = pixels
            .data()
            .len()
            .saturating_add(svg.len())
            .saturating_add(resources.iter().map(|r| r.len()).sum::<usize>())
            .saturating_add(
                domains
                    .iter()
                    .map(|d| std::mem::size_of::<RepeatEdgeDomain>() + d.filter_id.len())
                    .sum::<usize>(),
            );
        if bytes > LIMIT || svg.len() > 4 * 1024 * 1024 {
            return;
        }
        while !self.entries.is_empty() && (self.bytes + bytes > LIMIT || self.entries.len() >= 64) {
            self.bytes -= self.entries.pop_front().unwrap().bytes;
        }
        self.bytes += bytes;
        self.entries.push_back(Entry {
            svg: svg.into(),
            dimensions,
            domains: domains.to_vec(),
            resources,
            pixels: pixels.clone(),
            bytes,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_pixels_require_identical_geometry_size_domain_and_resource_bytes() {
        let mut cache = Cache::default();
        let pixels = Pixmap::new(4, 2).unwrap();
        let dimensions = [8, 4, 4, 2];
        let domain = RepeatEdgeDomain {
            filter_id: "blur".into(),
            primitive_index: 0,
            rect: resvg::tiny_skia::Rect::from_xywh(0., 0., 4., 2.).unwrap(),
            transform: resvg::tiny_skia::Transform::identity(),
        };
        let bytes = Arc::new(vec![1, 2, 3]);
        cache.insert(
            "<image href='libre-frame-image:0'/>",
            dimensions,
            &[domain.clone()],
            vec![bytes.clone()],
            &pixels,
        );
        assert_eq!(
            cache
                .get(
                    "<image href='libre-frame-image:0'/>",
                    dimensions,
                    &[domain.clone()],
                    &[bytes.clone()]
                )
                .unwrap()
                .data(),
            pixels.data()
        );
        assert!(
            cache
                .get(
                    "<image href='libre-frame-image:0'/>",
                    dimensions,
                    &[domain.clone()],
                    &[Arc::new(vec![3, 2, 1])]
                )
                .is_none()
        );
        assert!(
            cache
                .get(
                    "other geometry",
                    dimensions,
                    &[domain.clone()],
                    &[bytes.clone()]
                )
                .is_none()
        );
        assert!(
            cache
                .get(
                    "<image href='libre-frame-image:0'/>",
                    [8, 4, 2, 1],
                    &[domain.clone()],
                    &[bytes.clone()]
                )
                .is_none()
        );
        let mut moved = domain;
        moved.transform.tx = 1.;
        assert!(
            cache
                .get(
                    "<image href='libre-frame-image:0'/>",
                    dimensions,
                    &[moved],
                    &[bytes]
                )
                .is_none()
        );
        cache.clear();
        assert_eq!(cache.bytes, 0);
    }
}
