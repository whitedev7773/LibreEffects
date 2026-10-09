use super::{Action, EditorState};
use gpui::{Context, Window};
use libre_effects_core::{CompositionId, LayerId};

impl EditorState {
    pub(crate) fn preview_frame_range(&self) -> std::ops::Range<u32> {
        if self.preview_range == crate::preview_options::PreviewRange::WorkArea {
            return self.work_start
                ..self
                    .work_end
                    .min(self.editor.project().composition().duration());
        }
        crate::preview_options::range(
            self.editor.project().composition(),
            self.preview_range,
            self.frame,
        )
    }
    pub(crate) fn layer_expanded(&self, id: LayerId) -> bool {
        self.layer_tree.as_ref().map_or(
            self.expanded && self.editor.selected() == Some(id),
            |tree| tree.expanded.contains(&id),
        )
    }
    fn ensure_layer_tree(&mut self) -> &mut crate::view_state::LayerTree {
        if self.layer_tree.is_none() {
            let mut tree = crate::view_state::LayerTree::default();
            if self.expanded
                && let Some(id) = self.editor.selected()
            {
                tree.expanded.insert(id);
            }
            self.layer_tree = Some(tree);
        }
        self.layer_tree.as_mut().unwrap()
    }
    pub(crate) fn toggle_layer_expanded(&mut self, id: LayerId) {
        let tree = self.ensure_layer_tree();
        if !tree.expanded.remove(&id) {
            tree.expanded.insert(id);
        }
        self.expanded = self
            .editor
            .selected()
            .is_some_and(|id| self.layer_expanded(id));
    }
    pub(crate) fn reveal_selected_layers(&mut self) {
        let ids = self
            .selected_layers
            .iter()
            .copied()
            .chain(self.editor.selected())
            .collect::<Vec<_>>();
        self.ensure_layer_tree().expanded.extend(ids);
        self.expanded = true;
    }
    pub(crate) fn property_group_open(&self, id: LayerId, group: &str) -> bool {
        // Property shortcuts reveal matching rows even inside collapsed groups.
        self.property_filter.is_some()
            || self
                .layer_tree
                .as_ref()
                .is_none_or(|t| !t.collapsed_groups.contains(&(id, group.into())))
    }
    pub(crate) fn toggle_property_group(&mut self, id: LayerId, group: String) {
        let tree = self.ensure_layer_tree();
        let key = (id, group);
        if !tree.collapsed_groups.remove(&key) {
            tree.collapsed_groups.insert(key);
        }
    }
    pub(super) fn record_navigation(&mut self, previous: CompositionId) {
        if self.navigation_replay {
            return;
        }
        let active = self.editor.project().active_composition_id();
        if self.navigation_history.is_empty() {
            self.navigation_history.push(previous);
        }
        self.navigation_history.truncate(self.navigation_index + 1);
        if self.navigation_history.last() != Some(&active) {
            self.navigation_history.push(active);
        }
        if self.navigation_history.len() > 128 {
            self.navigation_history.remove(0);
        }
        self.navigation_index = self.navigation_history.len() - 1;
    }
    pub(crate) fn history_target(&self, delta: i32) -> Option<(usize, CompositionId)> {
        let mut index = self.navigation_index as i64 + i64::from(delta);
        while index >= 0 && index < self.navigation_history.len() as i64 {
            let id = self.navigation_history[index as usize];
            if self.editor.project().composition_by_id(id).is_some() {
                return Some((index as usize, id));
            }
            index += i64::from(delta.signum());
        }
        None
    }
    pub(super) fn navigate_history(
        &mut self,
        delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if delta == 0 {
            return;
        }
        if let Some((index, id)) = self.history_target(delta) {
            self.navigation_replay = true;
            self.dispatch(&Action::ActivateComposition(id), window, cx);
            self.navigation_replay = false;
            if self.editor.project().active_composition_id() == id {
                self.navigation_index = index;
            }
        }
    }
    /// Actual traversal history is unambiguous even when a precomp has several parents.
    pub(crate) fn composition_path(&self) -> Vec<CompositionId> {
        let current = self.editor.project().active_composition_id();
        let project = self.editor.project();
        let mut path = vec![current];
        for _ in 0..4 {
            let child = *path.last().unwrap();
            let parents = project.compositions().iter().filter_map(|(id, comp)| {
                comp.layers().iter().any(|layer| matches!(layer.content(), libre_effects_core::Content::Composition { composition, .. } if *composition == child)).then_some(*id)
            }).filter(|id| !path.contains(id)).collect::<Vec<_>>();
            let parent = self
                .navigation_history
                .iter()
                .take(self.navigation_index + 1)
                .rev()
                .find(|id| parents.contains(id))
                .copied()
                .or_else(|| {
                    if parents.len() == 1 {
                        parents.first().copied()
                    } else {
                        None
                    }
                });
            let Some(parent) = parent else {
                break;
            };
            path.push(parent);
        }
        path.reverse();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::Command;
    #[test]
    fn independently_expanded_layers_and_groups_survive_view_roundtrip_without_source_edits() {
        let mut s = EditorState::default();
        s.editor.execute(Command::AddSolid).unwrap();
        s.editor.execute(Command::AddSolid).unwrap();
        let ids = s
            .editor
            .project()
            .composition()
            .layers()
            .iter()
            .map(|l| l.id())
            .collect::<Vec<_>>();
        let source = s.editor.project().clone();
        s.toggle_layer_expanded(ids[0]);
        s.toggle_layer_expanded(ids[1]);
        s.toggle_property_group(ids[0], "Transform".into());
        s.editor.select(ids[1]);
        assert!(s.layer_expanded(ids[0]) && s.layer_expanded(ids[1]));
        assert!(!s.property_group_open(ids[0], "Transform"));
        assert!(s.property_group_open(ids[1], "Transform"));
        let views = s.capture_views();
        let bytes = views.encode_native(&source).unwrap();
        let views = crate::view_state::ProjectViews::read_native(&bytes, &source).unwrap();
        s.load_views(views);
        assert!(s.layer_expanded(ids[0]) && s.layer_expanded(ids[1]));
        assert!(!s.property_group_open(ids[0], "Transform"));
        assert_eq!(s.editor.project(), &source);
        s.toggle_layer_expanded(ids[1]);
        assert!(s.layer_expanded(ids[0]) && !s.layer_expanded(ids[1]));
    }
}
