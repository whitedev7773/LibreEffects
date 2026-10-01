use super::EditorState;
use crate::view_state::{CompositionView, ProjectViews};

impl EditorState {
    fn current_view(&self) -> CompositionView {
        CompositionView {
            frame: self.frame,
            timeline_start: self.timeline_start,
            timeline_zoom: self.timeline_zoom,
            preview_zoom: self.preview_zoom,
            preview_pan: self.preview_pan,
            preview_resolution: self.preview_resolution,
            checkerboard: self.checkerboard,
            viewer: self.viewer.clone(),
            graph_open: self.graph_open,
            expanded: self.expanded,
        }
    }
    pub(super) fn remember_view(&mut self) {
        // Retain recently deleted compositions for Undo, while bounding session memory.
        if self.composition_views.len() >= 1000 {
            self.composition_views
                .retain(|id, _| self.editor.project().composition_by_id(*id).is_some());
        }
        self.composition_views.insert(
            self.editor.project().active_composition_id(),
            self.current_view(),
        );
    }
    pub(super) fn restore_composition_view(&mut self) {
        let mut view = self
            .composition_views
            .get(&self.editor.project().active_composition_id())
            .cloned()
            .unwrap_or_default();
        view.normalize(self.editor.project().composition().duration());
        self.frame = view.frame;
        self.timeline_start = view.timeline_start;
        self.timeline_zoom = view.timeline_zoom;
        self.preview_zoom = view.preview_zoom;
        self.preview_pan = view.preview_pan;
        self.preview_resolution = view.preview_resolution;
        self.checkerboard = view.checkerboard;
        self.viewer = view.viewer;
        self.pixel_info = None;
        self.graph_open = view.graph_open;
        self.expanded = view.expanded;
    }
    pub(super) fn capture_views(&mut self) -> ProjectViews {
        self.remember_view();
        let mut views = ProjectViews::default();
        views.compositions = self.composition_views.clone();
        views.workspace = self.workspace.clone();
        views.workspace.effect_controls_open = self.effect_controls_open;
        views.workspace.snapping = self.snapping;
        views.normalize(self.editor.project());
        views
    }
    pub(super) fn load_views(&mut self, mut views: ProjectViews) {
        self.project_item = None;
        views.normalize(self.editor.project());
        self.composition_views = views.compositions;
        self.workspace = views.workspace;
        self.snapping = self.workspace.snapping;
        self.effect_controls_open = self.workspace.effect_controls_open;
        self.property_filter = None;
        self.graph_property = libre_effects_core::Property::PositionX.into();
        self.graph_key = None;
        self.restore_composition_view();
        self.preview_revision = self.preview_revision.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::Command;
    #[test]
    fn composition_views_survive_switches_history_and_save_without_document_edits() {
        let mut s = EditorState::default();
        s.editor.execute(Command::NewComposition).unwrap();
        s.saved = s.editor.project().clone();
        s.frame = 80;
        s.timeline_zoom = 8.0;
        s.timeline_start = 60;
        s.preview_pan = [25.0, -30.0];
        s.preview_zoom = Some(2.0);
        s.remember_view();
        s.editor.activate_composition(1).unwrap();
        s.composition_changed();
        assert_eq!(
            (s.frame, s.timeline_zoom, s.preview_pan),
            (0, 1.0, [0.0; 2])
        );
        s.frame = 25;
        s.remember_view();
        s.editor.activate_composition(2).unwrap();
        s.composition_changed();
        assert_eq!((s.frame, s.timeline_zoom, s.timeline_start), (80, 8.0, 60));
        assert_eq!((s.preview_zoom, s.preview_pan), (Some(2.0), [25.0, -30.0]));
        assert!(!s.dirty());
        let views = s.capture_views();
        let json = views.write(s.editor.project()).unwrap();
        let mut loaded = EditorState::default();
        loaded
            .editor
            .replace_project(libre_effects_core::Project::from_json(&json).unwrap())
            .unwrap();
        loaded.load_views(ProjectViews::read(&json, loaded.editor.project()));
        assert_eq!(
            (loaded.frame, loaded.timeline_start, loaded.preview_zoom),
            (80, 60, Some(2.0))
        );
        s.remember_view();
        s.editor.execute(Command::DeleteComposition).unwrap();
        s.composition_changed();
        assert_eq!(s.frame, 25);
        s.step_history(false);
        assert_eq!(s.frame, 80);
        assert!(!s.dirty());
        s.step_history(true);
        assert_eq!(s.frame, 25);
        s.editor
            .execute(Command::ConfigureComposition {
                name: "Short".into(),
                width: 100,
                height: 100,
                fps: 30,
                duration: 10,
            })
            .unwrap();
        s.normalize();
        assert_eq!(s.frame, 9);
        s.load_views(Default::default());
        assert_eq!(
            (s.frame, s.timeline_zoom, s.preview_pan),
            (0, 1.0, [0.0; 2])
        );
    }
}
