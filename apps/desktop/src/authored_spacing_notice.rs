//! Explain an intentional spacing reset without treating deleted layers as edits.
use libre_effects_core::{Content, LayerId, Project};
use std::collections::BTreeSet;

pub(crate) const RETAINED: &str =
    "Saved glyph spacing. Editing text or typography resets it; paint changes retain it.";
pub(crate) const DRAFT_RESET: &str =
    "Saved glyph spacing reset in this draft. Undo restores the saved spacing.";

pub(crate) fn authored_layers(project: &Project) -> BTreeSet<LayerId> {
    project
        .compositions()
        .iter()
        .flat_map(|(_, comp)| comp.layers())
        .filter(|layer| layer.has_authored_text_positions())
        .map(|layer| layer.id())
        .collect()
}

pub(crate) fn reset_count(before: &BTreeSet<LayerId>, project: &Project) -> usize {
    project
        .compositions()
        .iter()
        .flat_map(|(_, comp)| comp.layers())
        .filter(|layer| {
            before.contains(&layer.id())
                && matches!(layer.content(), Content::Text { .. })
                && !layer.has_authored_text_positions()
        })
        .count()
}

pub(crate) fn suffix(count: usize) -> &'static str {
    if count == 0 {
        ""
    } else {
        " · Saved glyph spacing reset"
    }
}
