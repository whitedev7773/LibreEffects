//! Explicit Project usage navigation changes view/selection, never source history.
use super::*;
use libre_effects_core::ProjectItem;
use libre_effects_editor_model::project_usage;

#[derive(Clone, Copy)]
pub(crate) struct Target {
    pub item: ProjectItem,
    pub composition: CompositionId,
    pub layer: LayerId,
    pub document_revision: u64,
    pub editor_generation: u64,
    pub input_generation: u64,
}

impl EditorState {
    pub(crate) fn project_usage_available(&self) -> bool {
        !self.playing
            && !self.preview_scrub
            && self.text_session.is_none()
            && self.colors.session.is_none()
            && self.gradient_editor.is_none()
            && self.vertex_editor.is_none()
            && self.expression_editor.is_none()
    }

    /// Caller also checks pending text/IME and pointer ownership before focus
    /// moves. The rendered target must still belong to the selected Project item.
    pub(crate) fn show_project_usage(&mut self, target: Target) -> bool {
        if !self.project_usage_available()
            || self.project_item != Some(target.item)
            || self.document_revision != target.document_revision
            || self.editor.context_generation() != target.editor_generation
            || self.input_context_generation() != target.input_generation
        {
            return false;
        }
        let Some(usage) = project_usage::resolve(
            self.editor.project(),
            target.item,
            target.composition,
            target.layer,
        ) else {
            return false;
        };
        if target.composition != self.editor.project().active_composition_id() {
            self.remember_view();
            if self
                .editor
                .activate_composition(target.composition)
                .is_err()
            {
                return false;
            }
            self.composition_changed();
        }
        // Preserve explicit Graph pins/active lanes while selecting the layer.
        self.select_timeline_rows(libre_effects_editor_model::timeline_navigation::Selection {
            active: Some(target.layer),
            layers: [target.layer].into(),
        });
        // Re-selecting the same layer still leaves any key/Contents ownership.
        self.begin_input_action();
        self.retire_colors_clipboard();
        self.selected_keys.clear();
        self.graph_key = None;
        self.colors_key_owned.set(false);
        self.contents_selection = None;
        self.marker_selection = None;
        self.project_usage_reveal = Some((
            self.document_revision,
            target.composition,
            target.layer,
            self.input_context_generation(),
        ));
        let comp = self.editor.project().composition();
        let hidden = comp.hide_shy() && comp.layer(target.layer).is_some_and(|l| l.shy());
        self.status = format!(
            "Selected {} in {}{}",
            usage.layer_name,
            usage.composition_name,
            if hidden {
                ". Turn off Hide Shy to show this layer."
            } else {
                ""
            }
        );
        true
    }
}

#[cfg(test)]
#[path = "editor_project_usage_tests.rs"]
mod tests;
