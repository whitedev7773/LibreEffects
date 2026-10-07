//! Read-only direct Project-item references and stable-ID jump targets.

use libre_effects_core::{
    Composition, CompositionId, Content, Layer, LayerId, Project, ProjectItem,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    pub composition: CompositionId,
    pub layer: LayerId,
    pub composition_name: String,
    pub layer_name: String,
}

/// Find direct uses in composition-ID order, then each composition's layer-stack
/// order. Each layer is examined once; nested composition contents are never
/// followed, so indirect uses and cycles cannot multiply the result.
///
/// Assets match only their stable asset ID, never a media path or payload.
/// Folders and missing source items have no uses. This does not change source,
/// selection, history or visibility, and includes hidden/locked/shy layers.
pub fn direct_uses(project: &Project, item: ProjectItem) -> Vec<Usage> {
    if !source_exists(project, item) {
        return Vec::new();
    }
    let mut uses = Vec::new();
    for (composition, comp) in project.compositions() {
        for layer in comp.layers() {
            if references(layer, item) {
                uses.push(usage(composition, comp, layer));
            }
        }
    }
    uses
}

/// Revalidate a displayed target against live IDs before navigating. Renames and
/// reorders retain identity; deleted or repointed targets never fall back to a
/// similarly named item or a different row. Returned display names are current.
pub fn resolve(
    project: &Project,
    item: ProjectItem,
    composition: CompositionId,
    layer: LayerId,
) -> Option<Usage> {
    if !source_exists(project, item) {
        return None;
    }
    let comp = project.composition_by_id(composition)?;
    let layer = comp.layer(layer)?;
    references(layer, item).then(|| usage(composition, comp, layer))
}

fn source_exists(project: &Project, item: ProjectItem) -> bool {
    match item {
        ProjectItem::Asset(id) => project.asset_library().assets().contains_key(&id),
        ProjectItem::Composition(id) => project.composition_by_id(id).is_some(),
        ProjectItem::Folder(_) => false,
    }
}

fn references(layer: &Layer, item: ProjectItem) -> bool {
    match item {
        ProjectItem::Asset(id) => layer.asset_id() == Some(id),
        ProjectItem::Composition(id) => {
            matches!(layer.content(), Content::Composition { composition, .. } if *composition == id)
        }
        ProjectItem::Folder(_) => false,
    }
}

fn usage(composition: CompositionId, comp: &Composition, layer: &Layer) -> Usage {
    Usage {
        composition,
        layer: layer.id(),
        composition_name: comp.name().into(),
        layer_name: layer.name().into(),
    }
}

#[cfg(test)]
#[path = "project_usage_tests.rs"]
mod tests;
