//! Explicit in-memory media alias groups for private automation transport.
//! These are not saved-project metadata and never deduplicate equal payloads.
use super::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum MediaRef {
    Asset(AssetId),
    Layer(CompositionId, LayerId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaSharing {
    images: Vec<Vec<MediaRef>>,
    sequences: Vec<Vec<MediaRef>>,
}

fn image_handles(project: &Project) -> BTreeMap<MediaRef, Arc<str>> {
    sources(project)
        .filter_map(|(key, content)| match content {
            Content::Image { png } => Some((key, png.clone())),
            _ => None,
        })
        .collect()
}
fn sequence_handles(project: &Project) -> BTreeMap<MediaRef, Arc<Vec<String>>> {
    sources(project)
        .filter_map(|(key, content)| match content {
            Content::ImageSequence { frames, .. } => Some((key, frames.clone())),
            _ => None,
        })
        .collect()
}
fn sources(project: &Project) -> impl Iterator<Item = (MediaRef, &Content)> {
    project
        .asset_library
        .assets
        .iter()
        .map(|(id, asset)| (MediaRef::Asset(*id), &asset.content))
        .chain(
            std::iter::once((project.composition_id, &project.composition))
                .chain(
                    project
                        .other_compositions
                        .iter()
                        .map(|(id, comp)| (*id, comp)),
                )
                .flat_map(|(id, comp)| {
                    comp.layers
                        .iter()
                        .map(move |layer| (MediaRef::Layer(id, layer.id), &layer.content))
                }),
        )
}
fn pointer<T: ?Sized>(value: &Arc<T>) -> usize {
    Arc::as_ptr(value) as *const () as usize
}
fn groups<T: ?Sized>(handles: &BTreeMap<MediaRef, Arc<T>>) -> Vec<Vec<MediaRef>> {
    let mut ids = BTreeMap::new();
    let mut result: Vec<Vec<MediaRef>> = Vec::new();
    for (key, handle) in handles {
        let index = *ids.entry(pointer(handle)).or_insert_with(|| {
            result.push(Vec::new());
            result.len() - 1
        });
        result[index].push(*key);
    }
    result
}
fn restore<T: ?Sized + PartialEq>(
    groups: &[Vec<MediaRef>],
    current: &BTreeMap<MediaRef, Arc<T>>,
    source: &BTreeMap<MediaRef, Arc<T>>,
) -> Result<BTreeMap<MediaRef, Arc<T>>, String> {
    let invalid = || "Invalid or changed automation media sharing".to_string();
    if groups.len() > current.len() {
        return Err(invalid());
    }
    let mut restored = BTreeMap::new();
    let mut original_groups = BTreeMap::new();
    for (index, group) in groups.iter().enumerate() {
        if group.is_empty() || group.len() > current.len() {
            return Err(invalid());
        }
        let first = current.get(&group[0]).ok_or_else(invalid)?;
        let mut original: Option<Arc<T>> = None;
        for key in group {
            let value = current.get(key).ok_or_else(invalid)?;
            if value != first || restored.contains_key(key) {
                return Err(invalid());
            }
            // Only a stable, still-existing reference with an identical payload
            // may supply an original handle. Equal but independent source handles
            // cannot be merged; one original group cannot be split across groups.
            if let Some(handle) = source.get(key) {
                if handle != value
                    || original
                        .as_ref()
                        .is_some_and(|old| !Arc::ptr_eq(old, handle))
                {
                    return Err(invalid());
                }
                if original_groups
                    .insert(pointer(handle), index)
                    .is_some_and(|old| old != index)
                {
                    return Err(invalid());
                }
                original = Some(handle.clone());
            }
            restored.insert(*key, value.clone());
        }
        let handle = original.unwrap_or_else(|| first.clone());
        for key in group {
            restored.insert(*key, handle.clone());
        }
    }
    if restored.len() != current.len() {
        return Err(invalid());
    }
    Ok(restored)
}
impl Project {
    pub fn media_sharing(&self) -> MediaSharing {
        MediaSharing {
            images: groups(&image_handles(self)),
            sequences: groups(&sequence_handles(self)),
        }
    }

    /// Reconstitute explicit aliases lost by serde, without normalizing source,
    /// schema or equal-but-independent assets. Validate all groups before mutation.
    /// A parent may additionally reuse verified handles from its original snapshot.
    pub fn restore_media_sharing(
        &mut self,
        sharing: &MediaSharing,
        source: Option<&Project>,
    ) -> Result<(), String> {
        let mut references = BTreeSet::new();
        if sources(self).any(|(key, _)| !references.insert(key)) {
            return Err("Duplicate automation media reference".into());
        }
        let images = restore(
            &sharing.images,
            &image_handles(self),
            &source.map(image_handles).unwrap_or_default(),
        )?;
        let sequences = restore(
            &sharing.sequences,
            &sequence_handles(self),
            &source.map(sequence_handles).unwrap_or_default(),
        )?;
        let visit = |key, content: &mut Content| match content {
            Content::Image { png } => *png = images[&key].clone(),
            Content::ImageSequence { frames, .. } => *frames = sequences[&key].clone(),
            _ => {}
        };
        for (id, asset) in &mut self.asset_library.assets {
            visit(MediaRef::Asset(*id), &mut asset.content);
        }
        for (id, comp) in std::iter::once((self.composition_id, &mut self.composition)).chain(
            self.other_compositions
                .iter_mut()
                .map(|(id, comp)| (*id, comp)),
        ) {
            for layer in &mut comp.layers {
                visit(MediaRef::Layer(id, layer.id), &mut layer.content);
            }
        }
        Ok(())
    }
}
