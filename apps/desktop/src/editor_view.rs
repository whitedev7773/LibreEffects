use super::EditorState;
use crate::view_state::{CompositionView, GraphChannel, GraphRanges, ProjectViews};
use libre_effects_core::{LayerId, PropertyPath};

impl EditorState {
    /// Legacy selection is read-only until the user opts into lanes or lane ranges.
    pub(crate) fn graph_active_channel(&self) -> Option<GraphChannel> {
        let channel = if self.graph_channels.explicit {
            self.graph_channels.active?
        } else {
            GraphChannel {
                id: self.editor.selected()?,
                property: self.graph_property,
            }
        };
        channel
            .available(self.editor.project().composition())
            .then_some(channel)
    }
    pub(crate) fn graph_included_channels(&self) -> Vec<GraphChannel> {
        if self.graph_channels.explicit {
            self.graph_channels
                .included()
                .into_iter()
                .filter(|c| c.available(self.editor.project().composition()))
                .collect()
        } else {
            self.graph_active_channel().into_iter().collect()
        }
    }
    fn graph_enable_channels(&mut self) {
        if self.graph_channels.explicit {
            return;
        }
        let current = self.graph_active_channel();
        self.graph_channels.explicit = true;
        self.graph_channels.active = current;
        // The legacy height belongs only to its current lane AND current mode.
        if let (Some(channel), Some(height)) = (current, self.graph_view.height.take()) {
            let mut ranges = GraphRanges::default();
            if self.graph_view.speed {
                ranges.speed = Some(height);
            } else {
                ranges.value = Some(height);
            }
            ranges.normalize();
            self.graph_channels.ranges.insert(channel, ranges);
        }
    }
    pub(crate) fn graph_activate_channel(
        &mut self,
        channel: GraphChannel,
        preserve_keys: bool,
    ) -> bool {
        if !channel.available(self.editor.project().composition()) {
            return false;
        }
        self.normalize_graph_channels(false);
        self.graph_enable_channels();
        self.graph_focus_channel(channel, preserve_keys)
    }
    /// Existing Timeline/Inspector navigation must remain representable as v1
    /// until the user deliberately opts into lanes, pins or per-channel ranges.
    pub(crate) fn graph_activate_property(
        &mut self,
        channel: GraphChannel,
        preserve_keys: bool,
    ) -> bool {
        // Path rows share Timeline selection and retiming, but their values are
        // opaque pose references and must never become numeric Graph lanes.
        if matches!(channel.property, PropertyPath::Path(_)) {
            if !self
                .editor
                .project()
                .composition()
                .layer(channel.id)
                .is_some_and(|layer| layer.track(channel.property).is_some())
            {
                return false;
            }
            self.normalize_graph_channels(false);
            self.editor.select(channel.id);
            self.selected_layers = [channel.id].into();
            self.graph_property = if self.graph_channels.explicit {
                self.graph_active_channel()
                    .map_or(channel.property, |c| c.property)
            } else {
                channel.property
            };
            if !preserve_keys {
                self.selected_keys.clear();
            }
            self.graph_key = None;
            return true;
        }
        // Whole-layer Text paint rows deliberately expose static defaults even
        // before animation materializes a track. Their labels/components still
        // focus the Inspector. An explicit Graph keeps its existing numeric
        // lane identity and ranges until this Text property has a real track.
        let static_text = matches!(channel.property, PropertyPath::Text(_))
            && self
                .editor
                .project()
                .composition()
                .layer(channel.id)
                .is_some_and(|layer| {
                    layer.track(channel.property).is_none()
                        && layer.track_value(channel.property, self.frame).is_some()
                });
        if static_text {
            self.normalize_graph_channels(false);
            self.editor.select(channel.id);
            self.selected_layers.insert(channel.id);
            self.graph_property = if self.graph_channels.explicit {
                self.graph_active_channel()
                    .map_or(channel.property, |active| active.property)
            } else {
                channel.property
            };
            if !preserve_keys {
                self.selected_keys.clear();
            }
            self.graph_key = None;
            return true;
        }
        if !channel.available(self.editor.project().composition()) {
            return false;
        }
        self.normalize_graph_channels(false);
        self.graph_focus_channel(channel, preserve_keys)
    }
    fn graph_focus_channel(&mut self, channel: GraphChannel, preserve_keys: bool) -> bool {
        if self.graph_channels.explicit {
            self.graph_channels.activate(channel);
        }
        self.editor.select(channel.id);
        self.selected_layers = [channel.id].into();
        self.graph_property = channel.property;
        if !preserve_keys {
            self.selected_keys.clear();
            self.graph_key = None;
        }
        if self.graph_open {
            self.graph_prune_excluded_keys();
        }
        self.graph_key = self
            .graph_key
            .filter(|key| GraphChannel::from(*key) == channel)
            .or_else(|| {
                self.selected_keys
                    .iter()
                    .copied()
                    .find(|key| GraphChannel::from(*key) == channel)
            });
        true
    }
    /// Entering Graph or replacing its transient lane removes invisible keys;
    /// ordinary Timeline-only selection remains free to span other tracks.
    pub(crate) fn graph_prune_excluded_keys(&mut self) {
        let included = self.graph_included_channels();
        self.selected_keys
            .retain(|key| included.contains(&GraphChannel::from(*key)));
        if self
            .graph_key
            .is_some_and(|key| !included.contains(&GraphChannel::from(key)))
        {
            self.graph_key = None;
        }
    }
    pub(crate) fn graph_pin_channel(&mut self, channel: GraphChannel) -> Result<(), String> {
        if !channel.available(self.editor.project().composition()) {
            return Err("This Graph channel is unavailable".into());
        }
        self.normalize_graph_channels(false);
        self.graph_enable_channels();
        self.graph_channels.pin(channel)
    }
    pub(crate) fn graph_unpin_channel(&mut self, channel: GraphChannel) {
        self.graph_channels.unpin(channel);
        if !self.graph_channels.contains(channel) {
            self.selected_keys
                .retain(|key| GraphChannel::from(*key) != channel);
            if self
                .graph_key
                .is_some_and(|key| GraphChannel::from(key) == channel)
            {
                self.graph_key = None;
            }
        }
    }
    pub(crate) fn graph_channel_height(
        &self,
        channel: GraphChannel,
        speed: bool,
    ) -> Option<[f64; 2]> {
        if !self.graph_channels.explicit {
            return (self.graph_active_channel() == Some(channel)
                && self.graph_view.speed == speed)
                .then_some(self.graph_view.height)
                .flatten();
        }
        let ranges = self.graph_channels.ranges.get(&channel)?;
        if speed { ranges.speed } else { ranges.value }
    }
    pub(crate) fn graph_set_channel_height(
        &mut self,
        channel: GraphChannel,
        speed: bool,
        height: Option<[f64; 2]>,
    ) {
        if !self.graph_included_channels().contains(&channel) {
            return;
        }
        if !self.graph_channels.explicit {
            // Ordinary legacy viewport pan/zoom/fit remains representable in
            // VIEW v1. Only an opted-in channel state owns per-lane ranges.
            if self.graph_active_channel() == Some(channel) && self.graph_view.speed == speed {
                self.graph_view.height = height;
                self.graph_view.normalize();
            }
            return;
        }
        let ranges = self.graph_channels.ranges.entry(channel).or_default();
        if speed {
            ranges.speed = height;
        } else {
            ranges.value = height;
        }
        ranges.normalize();
        self.normalize_graph_channels(false);
    }
    /// Explicit external selection updates the active address deliberately. It
    /// never leaves a property's alias silently pointing at a different layer.
    pub(super) fn graph_external_activation(&mut self, id: LayerId, property: PropertyPath) {
        if !self.graph_channels.explicit {
            return;
        }
        let channel = GraphChannel { id, property };
        if channel.available(self.editor.project().composition()) {
            self.graph_channels.activate(channel);
        } else {
            self.graph_channels.active = self
                .graph_channels
                .included()
                .into_iter()
                .find(|c| c.available(self.editor.project().composition()));
        }
        if let Some(active) = self.graph_channels.active {
            self.graph_property = active.property;
        }
        self.graph_key = None;
    }
    pub(super) fn graph_external_layer_selection(&mut self) {
        if !self.graph_channels.explicit {
            self.graph_key = None;
            return;
        }
        if let Some(id) = self.editor.selected() {
            self.graph_external_activation(id, self.graph_property);
        } else {
            self.graph_channels.active = self.graph_channels.included().first().copied();
            self.graph_key = None;
        }
    }
    pub(super) fn normalize_graph_channels(&mut self, history: bool) {
        let project = self.editor.project();
        for (id, view) in &mut self.composition_views {
            view.graph_channels
                .reconcile(project.composition_by_id(*id), history);
        }
        self.graph_channels
            .reconcile(Some(project.composition()), history);
        if let Some(channel) = self.graph_channels.active {
            self.graph_property = channel.property;
        }
    }
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
            graph_view: self.graph_view.clone(),
            expanded: self.expanded,
            graph_channels: self.graph_channels.clone(),
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
        self.graph_view = view.graph_view;
        self.graph_channels = view.graph_channels;
        if let Some(channel) = self.graph_active_channel() {
            self.graph_property = channel.property;
        }
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
        self.gradient_controls = None;
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
        s.graph_view = crate::view_state::GraphView {
            speed: true,
            height: Some([-500.0, 750.0]),
        };
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
        assert!(s.graph_view.speed);
        assert_eq!(s.graph_view.height, Some([-500.0, 750.0]));
        assert!(!s.dirty());
        let views = s.capture_views();
        let json = views.write(s.editor.project()).unwrap();
        let mut loaded = EditorState::default();
        loaded
            .editor
            .replace_project(libre_effects_core::Project::from_json(&json).unwrap())
            .unwrap();
        loaded.load_views(ProjectViews::read(&json, loaded.editor.project()));
        assert_eq!(loaded.graph_view, s.graph_view);
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
        assert_eq!(s.graph_view, Default::default());
        assert_eq!(
            (s.frame, s.timeline_zoom, s.preview_pan),
            (0, 1.0, [0.0; 2])
        );
    }

    fn channel(id: u64, property: libre_effects_core::Property) -> GraphChannel {
        GraphChannel {
            id,
            property: property.into(),
        }
    }
    fn keyed_pair() -> EditorState {
        use libre_effects_core::{Property, TrackEdit};
        let mut s = EditorState::default();
        for _ in 0..2 {
            s.editor.execute(Command::AddRectangle).unwrap();
        }
        for (id, property) in [
            (1, Property::PositionX),
            (1, Property::PositionY),
            (2, Property::Opacity),
        ] {
            s.editor
                .execute(Command::EditTrack {
                    id,
                    property: property.into(),
                    edit: TrackEdit::ToggleKey { frame: 12 },
                })
                .unwrap();
        }
        s.editor.select(1);
        s.normalize();
        s
    }

    #[test]
    fn graph_lane_activation_preserves_keys_and_complete_active_identity_without_source_history() {
        use libre_effects_core::{KeyRef, Property};
        let mut s = keyed_pair();
        s.editor.clear_history();
        s.saved = s.editor.project().clone();
        let source = s.editor.project().clone();
        let x = channel(1, Property::PositionX);
        let y = channel(1, Property::PositionY);
        let a = channel(2, Property::Opacity);
        let keys: std::collections::BTreeSet<_> = [x, y, a]
            .into_iter()
            .map(|c| KeyRef {
                id: c.id,
                property: c.property,
                frame: 12,
            })
            .collect();
        s.selected_keys = keys.clone();
        s.graph_key = Some(KeyRef {
            id: 1,
            property: x.property,
            frame: 12,
        });
        for c in [x, y, a] {
            s.graph_pin_channel(c).unwrap();
        }
        assert!(s.graph_activate_channel(y, true));
        assert_eq!(s.selected_keys, keys);
        assert_eq!(
            s.graph_key,
            Some(KeyRef {
                id: 1,
                property: y.property,
                frame: 12
            })
        );
        assert!(s.graph_activate_channel(a, true));
        assert_eq!(s.editor.selected(), Some(2));
        assert_eq!(s.selected_keys, keys);
        assert_eq!(s.graph_property, a.property);
        assert_eq!(
            s.graph_key,
            Some(KeyRef {
                id: 2,
                property: a.property,
                frame: 12
            })
        );
        s.graph_set_channel_height(x, false, Some([-20., 30.]));
        s.graph_set_channel_height(x, true, Some([-2., 3.]));
        s.graph_unpin_channel(y);
        assert_eq!(s.graph_channel_height(x, false), Some([-20., 30.]));
        assert_eq!(s.graph_channel_height(x, true), Some([-2., 3.]));
        assert_eq!(s.editor.project(), &source);
        assert!(!s.editor.can_undo());
        assert!(!s.editor.can_redo());
        assert!(!s.dirty());
        assert!(
            !s.graph_pin_channel(GraphChannel {
                id: 1,
                property: PropertyPath::TimeRemap
            })
            .is_ok()
        );
        assert_eq!(s.editor.project(), &source);
    }

    #[test]
    fn legacy_height_migrates_to_only_its_original_channel_and_mode() {
        use libre_effects_core::Property;
        let mut s = keyed_pair();
        let x = channel(1, Property::PositionX);
        let a = channel(2, Property::Opacity);
        s.graph_view.speed = true;
        s.graph_view.height = Some([-42., 84.]);
        assert!(!s.graph_channels.explicit);
        s.graph_pin_channel(a).unwrap();
        assert_eq!(s.graph_channel_height(x, true), Some([-42., 84.]));
        assert_eq!(s.graph_channel_height(x, false), None);
        assert_eq!(s.graph_channel_height(a, true), None);
        assert_eq!(s.graph_channel_height(a, false), None);
        assert_eq!(s.graph_view.height, None);
        s.graph_set_channel_height(a, false, Some([0., 100.]));
        assert_eq!(s.graph_channel_height(a, false), Some([0., 100.]));
        assert_eq!(s.graph_channel_height(x, true), Some([-42., 84.]));
    }

    #[test]
    fn delete_save_undo_retains_live_pins_and_ranges_but_prunes_saved_copy() {
        use libre_effects_core::Property;
        let mut s = keyed_pair();
        let x = channel(1, Property::PositionX);
        let a = channel(2, Property::Opacity);
        for c in [x, a] {
            s.graph_pin_channel(c).unwrap();
        }
        s.graph_activate_channel(x, true);
        s.graph_set_channel_height(x, false, Some([-300., 900.]));
        s.graph_set_channel_height(x, true, Some([-40., 40.]));
        s.editor.execute(Command::RemoveLayer(1)).unwrap();
        s.normalize();
        assert_eq!(s.graph_channels.pinned, vec![x, a]);
        assert!(!s.graph_channels.is_available(x));
        assert_eq!(s.graph_active_channel(), Some(a));
        let after_delete = s.editor.project().clone();
        let captured = s.capture_views();
        let bytes = captured.encode_native(s.editor.project()).unwrap();
        let saved = ProjectViews::read_native(&bytes, s.editor.project()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("deleted-channel.lep");
        crate::project_io::write_native_project(&destination, s.editor.project(), Some(&captured))
            .unwrap();
        let reopened = crate::project_io::read_editor_project(&destination).unwrap();
        assert_eq!(reopened.project, after_delete);
        assert_eq!(reopened.views, saved);
        assert_eq!(saved.compositions[&1].graph_channels.pinned, vec![a]);
        assert!(
            !saved.compositions[&1]
                .graph_channels
                .ranges
                .contains_key(&x)
        );
        assert_eq!(s.graph_channels.pinned, vec![x, a]);
        assert_eq!(s.graph_channel_height(x, true), Some([-40., 40.]));
        assert_eq!(s.editor.project(), &after_delete);
        s.step_history(false);
        assert_eq!(s.graph_channels.pinned, vec![x, a]);
        assert!(s.graph_channels.is_available(x));
        assert_eq!(s.graph_active_channel(), Some(a));
        assert_eq!(s.graph_channel_height(x, false), Some([-300., 900.]));
        assert!(s.selected_keys.is_empty());
        assert_eq!(s.graph_key, None);
        s.step_history(true);
        assert!(!s.graph_channels.is_available(x));
        s.editor
            .execute(Command::RenameLayer {
                id: 2,
                name: "Unrelated edit".into(),
            })
            .unwrap();
        s.normalize();
        s.step_history(false);
        assert!(!s.graph_channels.is_available(x));
        s.step_history(false);
        assert!(s.graph_channels.is_available(x));
        assert_eq!(s.graph_active_channel(), Some(a));
    }

    #[test]
    fn explicit_unpin_cannot_be_undone_by_document_history() {
        use libre_effects_core::Property;
        let mut s = keyed_pair();
        let x = channel(1, Property::PositionX);
        s.graph_pin_channel(x).unwrap();
        s.graph_set_channel_height(x, false, Some([0., 100.]));
        s.editor.execute(Command::RemoveLayer(1)).unwrap();
        s.normalize();
        assert_eq!(s.graph_active_channel(), None);
        s.graph_unpin_channel(x);
        assert!(s.graph_channels.pinned.is_empty());
        s.capture_views();
        s.step_history(false);
        assert!(s.graph_channels.pinned.is_empty());
        assert_eq!(s.graph_active_channel(), None);
        assert_eq!(s.graph_channel_height(x, false), None);
    }

    #[test]
    fn reused_layer_and_effect_ids_do_not_revive_old_pin_intent() {
        use libre_effects_core::{EffectEdit, EffectKind, EffectParam, Property};
        let mut s = EditorState::default();
        s.editor.execute(Command::AddRectangle).unwrap();
        let x = channel(1, Property::PositionX);
        s.graph_pin_channel(x).unwrap();
        s.graph_set_channel_height(x, false, Some([0., 100.]));
        s.step_history(false);
        assert!(!s.graph_channels.is_available(x));
        s.editor.execute(Command::AddRectangle).unwrap();
        s.normalize();
        assert!(s.editor.project().composition().layer(1).is_some());
        assert!(!s.graph_channels.is_pinned(x));
        assert_eq!(s.graph_active_channel(), None);
        assert_eq!(s.graph_channel_height(x, false), None);
        s.graph_pin_channel(x).unwrap();
        assert!(s.graph_channels.is_available(x));
        s.editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::GaussianBlur),
            })
            .unwrap();
        let effect = GraphChannel {
            id: 1,
            property: PropertyPath::Effect {
                effect: 1,
                parameter: EffectParam::Radius,
            },
        };
        s.graph_pin_channel(effect).unwrap();
        s.step_history(false);
        assert!(!s.graph_channels.is_available(effect));
        s.editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::GaussianBlur),
            })
            .unwrap();
        s.normalize();
        assert!(!s.graph_channels.is_pinned(effect));
        s.graph_pin_channel(effect).unwrap();
        assert!(s.graph_channels.is_available(effect));
    }

    #[test]
    fn unavailable_pins_still_count_toward_cap_and_one_transient_is_allowed() {
        use libre_effects_core::Property;
        let mut s = EditorState::default();
        for _ in 0..17 {
            s.editor.execute(Command::AddRectangle).unwrap();
        }
        for id in 1..=16 {
            s.graph_pin_channel(channel(id, Property::PositionX))
                .unwrap();
        }
        s.graph_activate_channel(channel(17, Property::PositionX), true);
        assert_eq!(s.graph_included_channels().len(), 17);
        s.editor.execute(Command::RemoveLayer(1)).unwrap();
        s.normalize();
        assert_eq!(s.graph_channels.pinned.len(), 16);
        assert!(
            s.graph_pin_channel(channel(17, Property::PositionX))
                .is_err()
        );
        let _ = s.capture_views().encode_native(s.editor.project()).unwrap();
        assert_eq!(s.graph_channels.pinned.len(), 16);
        s.graph_unpin_channel(channel(1, Property::PositionX));
        s.graph_pin_channel(channel(17, Property::PositionX))
            .unwrap();
        assert_eq!(s.graph_channels.pinned.len(), 16);
    }

    #[test]
    fn inactive_composition_v2_views_survive_open_and_do_not_dirty_project() {
        use libre_effects_core::Property;
        let mut s = keyed_pair();
        let x = channel(1, Property::PositionX);
        s.graph_pin_channel(x).unwrap();
        s.graph_set_channel_height(x, false, Some([-75., 125.]));
        s.remember_view();
        s.editor.execute(Command::NewComposition).unwrap();
        s.composition_changed();
        assert!(!s.graph_channels.explicit);
        let project = s.editor.project().clone();
        let views = s.capture_views();
        let bytes = views.encode_native(&project).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 2);
        assert!(value["compositions"]["1"]["graph_channels"].is_object());
        assert!(value["compositions"]["2"].get("graph_channels").is_none());
        let container = libre_effects_core::project_file::encode(&project, Some(&bytes)).unwrap();
        let opened = crate::project_io::decode_project(&container).unwrap();
        assert_eq!(opened.project, project);
        let mut loaded = EditorState::default();
        loaded.editor.replace_project(opened.project).unwrap();
        loaded.saved = loaded.editor.project().clone();
        loaded.load_views(opened.views);
        loaded.remember_view();
        loaded.editor.activate_composition(1).unwrap();
        loaded.composition_changed();
        assert_eq!(loaded.graph_channels.pinned, vec![x]);
        assert_eq!(loaded.graph_channel_height(x, false), Some([-75., 125.]));
        assert!(!loaded.dirty());
        loaded.load_views(ProjectViews::default());
        assert!(!loaded.graph_channels.explicit);
    }

    #[test]
    fn graph_view_edits_and_native_save_preserve_existing_redo_branch() {
        use libre_effects_core::Property;
        let mut s = keyed_pair();
        s.editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "History witness".into(),
            })
            .unwrap();
        s.step_history(false);
        assert!(s.editor.can_undo());
        assert!(s.editor.can_redo());
        let source = s.editor.project().clone();
        let x = channel(1, Property::PositionX);
        let a = channel(2, Property::Opacity);
        s.graph_pin_channel(x).unwrap();
        s.graph_pin_channel(a).unwrap();
        s.graph_activate_channel(a, true);
        s.graph_set_channel_height(x, true, Some([-50., 50.]));
        s.graph_view.speed = true;
        let views = s.capture_views();
        let view_bytes = views.encode_native(s.editor.project()).unwrap();
        let saved = libre_effects_core::project_file::encode(s.editor.project(), Some(&view_bytes))
            .unwrap();
        assert_eq!(
            libre_effects_core::project_file::decode(&saved)
                .unwrap()
                .project,
            source
        );
        assert_eq!(s.editor.project(), &source);
        assert!(s.editor.can_undo());
        assert!(s.editor.can_redo());
        s.step_history(true);
        assert_eq!(
            s.editor.project().composition().layer(1).unwrap().name(),
            "History witness"
        );
        assert_eq!(s.graph_active_channel(), Some(a));
        assert!(s.selected_keys.is_empty());
    }

    #[test]
    fn ordinary_property_navigation_keeps_legacy_view_schema_and_focuses_correct_lane() {
        use libre_effects_core::Property;
        let mut s = keyed_pair();
        let a = channel(2, Property::Opacity);
        s.graph_view.height = Some([-10., 110.]);
        assert!(s.graph_activate_property(a, false));
        assert_eq!(s.graph_active_channel(), Some(a));
        assert_eq!(s.editor.selected(), Some(2));
        assert_eq!(s.graph_property, a.property);
        assert!(!s.graph_channels.explicit);
        assert_eq!(s.graph_view.height, Some([-10., 110.]));
        let views = s.capture_views();
        let value: serde_json::Value =
            serde_json::from_slice(&views.encode_native(s.editor.project()).unwrap()).unwrap();
        assert_eq!(value["version"], 1);
        assert!(value["compositions"]["1"].get("graph_channels").is_none());
        s.graph_pin_channel(a).unwrap();
        let x = channel(1, Property::PositionX);
        assert!(s.graph_activate_property(x, false));
        assert_eq!(s.graph_channels.active, Some(x));
        assert_eq!(s.graph_channels.pinned, vec![a]);
        assert_eq!(s.graph_property, x.property);
    }

    #[test]
    fn deleted_composition_views_survive_save_and_undo_with_pins_and_ranges() {
        use libre_effects_core::Property;
        let mut s = keyed_pair();
        let x = channel(1, Property::PositionX);
        s.graph_pin_channel(x).unwrap();
        s.graph_set_channel_height(x, true, Some([-70., 70.]));
        s.remember_view();
        s.editor.execute(Command::NewComposition).unwrap();
        s.composition_changed();
        s.remember_view();
        s.editor.activate_composition(1).unwrap();
        s.composition_changed();
        s.remember_view();
        s.editor.execute(Command::DeleteComposition).unwrap();
        s.composition_changed();
        s.normalize();
        assert_eq!(s.editor.project().active_composition_id(), 2);
        assert!(s.composition_views.contains_key(&1));
        let saved = s.capture_views();
        assert!(!saved.compositions.contains_key(&1));
        let bytes = saved.encode_native(s.editor.project()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 1);
        assert!(s.composition_views.contains_key(&1));
        s.step_history(false);
        assert_eq!(s.editor.project().active_composition_id(), 1);
        assert_eq!(s.graph_channels.pinned, vec![x]);
        assert!(s.graph_channels.is_available(x));
        assert_eq!(s.graph_channel_height(x, true), Some([-70., 70.]));
        assert!(s.selected_keys.is_empty());
        assert_eq!(s.graph_key, None);
    }

    #[test]
    fn path_property_navigation_selects_its_layer_without_numeric_channel_opt_in() {
        use libre_effects_core::{
            KeyRef, PathMask, PathTarget, PathVertex, Property, TrackEdit, VectorPath,
        };
        let mut s = keyed_pair();
        let x = channel(1, Property::PositionX);
        s.editor
            .execute(Command::SetPathMasks {
                id: 2,
                masks: vec![PathMask {
                    path: VectorPath {
                        closed: true,
                        vertices: [[0., 0.], [30., 0.], [15., 30.]]
                            .map(PathVertex::corner)
                            .to_vec(),
                    },
                    ..Default::default()
                }],
            })
            .unwrap();
        s.editor
            .execute(Command::AnimatePath {
                id: 2,
                target: PathTarget::Mask(1),
                edit: TrackEdit::ToggleAnimation { frame: 12 },
            })
            .unwrap();
        s.editor.select(1);
        let path = GraphChannel {
            id: 2,
            property: PropertyPath::Path(PathTarget::Mask(1)),
        };
        let keys = [
            KeyRef {
                id: x.id,
                property: x.property,
                frame: 12,
            },
            KeyRef {
                id: path.id,
                property: path.property,
                frame: 12,
            },
        ]
        .into();
        s.selected_keys = keys;
        let keys = s.selected_keys.clone();
        let source = s.editor.project().clone();
        assert!(s.graph_activate_property(path, true));
        assert_eq!(s.editor.selected(), Some(2));
        assert_eq!(s.graph_property, path.property);
        assert_eq!(s.selected_keys, keys);
        assert!(!s.graph_channels.explicit);
        assert!(s.graph_included_channels().is_empty());
        assert!(s.graph_pin_channel(path).is_err());
        s.graph_pin_channel(x).unwrap();
        s.graph_activate_channel(x, true);
        s.graph_open = true;
        assert!(s.graph_activate_property(path, true));
        // A Timeline Path key click follows activation with its full address
        // and Seek normalization even when scalar Graph lanes remain open.
        s.graph_key = Some(KeyRef {
            id: path.id,
            property: path.property,
            frame: 12,
        });
        s.normalize();
        assert_eq!(s.editor.selected(), Some(2));
        assert_eq!(s.graph_active_channel(), Some(x));
        assert_eq!(s.graph_channels.pinned, vec![x]);
        assert_eq!(s.selected_keys, keys);
        assert_eq!(s.editor.project(), &source);
    }

    #[test]
    fn legacy_height_edits_keep_v1_and_reject_inactive_mode_or_channel() {
        use libre_effects_core::Property;
        let mut s = keyed_pair();
        let x = channel(1, Property::PositionX);
        let a = channel(2, Property::Opacity);
        let source = s.editor.project().clone();
        s.graph_set_channel_height(x, false, Some([-10., 80.]));
        assert_eq!(s.graph_view.height, Some([-10., 80.]));
        assert!(!s.graph_channels.explicit);
        s.graph_set_channel_height(x, true, Some([-1., 8.]));
        s.graph_set_channel_height(a, false, Some([0., 100.]));
        assert_eq!(s.graph_view.height, Some([-10., 80.]));
        let views = s.capture_views();
        let value: serde_json::Value =
            serde_json::from_slice(&views.encode_native(s.editor.project()).unwrap()).unwrap();
        assert_eq!(value["version"], 1);
        assert!(value["compositions"]["1"].get("graph_channels").is_none());
        s.graph_set_channel_height(x, false, Some([1., 0.]));
        assert_eq!(s.graph_view.height, None);
        s.graph_view.speed = true;
        s.graph_set_channel_height(x, true, Some([-1., 8.]));
        assert_eq!(s.graph_view.height, Some([-1., 8.]));
        assert!(!s.graph_channels.explicit);
        assert_eq!(s.editor.project(), &source);
        s.graph_pin_channel(x).unwrap();
        assert_eq!(s.graph_channel_height(x, true), Some([-1., 8.]));
        s.graph_set_channel_height(x, false, Some([0., 200.]));
        assert_eq!(s.graph_channel_height(x, false), Some([0., 200.]));
        assert_eq!(s.graph_channel_height(x, true), Some([-1., 8.]));
    }

    #[test]
    fn graph_focus_prunes_replaced_transient_keys_but_preserves_pins_and_timeline_selection() {
        use libre_effects_core::{KeyRef, Property};
        let mut s = keyed_pair();
        let x = channel(1, Property::PositionX);
        let y = channel(1, Property::PositionY);
        let a = channel(2, Property::Opacity);
        let key = |c: GraphChannel| KeyRef {
            id: c.id,
            property: c.property,
            frame: 12,
        };
        let all = [key(x), key(y), key(a)].into();
        s.graph_pin_channel(a).unwrap();
        s.selected_keys = all;
        s.graph_activate_property(y, true);
        assert_eq!(s.selected_keys.len(), 3); // Timeline-only navigation preserves cross-track keys.
        s.graph_open = true;
        s.graph_prune_excluded_keys();
        assert_eq!(s.selected_keys, [key(y), key(a)].into());
        s.graph_activate_channel(x, true);
        assert_eq!(s.selected_keys, [key(a)].into());
        assert_eq!(s.graph_active_channel(), Some(x));
        s.selected_keys.insert(key(x));
        s.graph_activate_channel(a, true);
        assert_eq!(s.selected_keys, [key(a)].into());
        assert_eq!(s.graph_key, Some(key(a)));
        s.graph_pin_channel(x).unwrap();
        s.selected_keys.insert(key(x));
        s.graph_activate_channel(x, true);
        assert_eq!(s.selected_keys, [key(x), key(a)].into());
    }

    fn sparse_text_scene() -> EditorState {
        use libre_effects_core::{Content, KeyRef, Property, TrackEdit};
        let mut s = EditorState::default();
        s.editor.execute(Command::AddRectangle).unwrap();
        s.editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Sparse title".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 100.,
                name: "Title".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::EditTrack {
                id: 1,
                property: Property::PositionX.into(),
                edit: TrackEdit::ToggleKey { frame: 12 },
            })
            .unwrap();
        s.editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Redo witness".into(),
            })
            .unwrap();
        s.editor.undo();
        s.selected_keys = [KeyRef {
            id: 1,
            property: Property::PositionX.into(),
            frame: 12,
        }]
        .into();
        s.saved = s.editor.project().clone();
        s
    }

    #[test]
    fn sparse_text_property_activation_preserves_legacy_focus_v1_and_source_history() {
        use libre_effects_core::{Property, TextParam};
        for parameter in TextParam::ALL {
            for graph_open in [false, true] {
                for rectangle_primary in [false, true] {
                    let mut s = sparse_text_scene();
                    s.editor.select(if rectangle_primary { 1 } else { 2 });
                    s.selected_layers = if rectangle_primary {
                        [1, 2].into()
                    } else {
                        [2].into()
                    };
                    s.graph_property = Property::PositionX.into();
                    s.graph_open = graph_open;
                    s.graph_view.height = Some([-20., 100.]);
                    let property = PropertyPath::Text(parameter);
                    let target = GraphChannel { id: 2, property };
                    let source = s.editor.project().clone();
                    let layers = s.selected_layers.clone();
                    let keys = s.selected_keys.clone();
                    let view = s.graph_view.clone();
                    assert!(
                        s.editor
                            .project()
                            .composition()
                            .layer(2)
                            .unwrap()
                            .track(property)
                            .is_none()
                    );
                    let view_before = s.capture_views();
                    let view_bytes_before = view_before.encode_native(&source).unwrap();
                    let native_before =
                        crate::project_io::encode_native_project(&source, Some(&view_before))
                            .unwrap();
                    assert!(s.graph_activate_property(target, true));
                    assert_eq!(s.editor.selected(), Some(2));
                    assert_eq!(s.selected_layers, layers);
                    assert_eq!(s.selected_keys, keys);
                    assert_eq!(s.graph_property, property);
                    assert_eq!(s.graph_view, view);
                    assert_eq!(s.graph_open, graph_open);
                    assert!(!s.graph_channels.explicit);
                    assert_eq!(s.graph_active_channel(), None);
                    assert!(s.graph_included_channels().is_empty());
                    assert!(s.graph_pin_channel(target).is_err());
                    let views = s.capture_views();
                    let view_bytes = views.encode_native(s.editor.project()).unwrap();
                    assert_eq!(view_bytes, view_bytes_before);
                    let value: serde_json::Value = serde_json::from_slice(&view_bytes).unwrap();
                    let text = std::str::from_utf8(&view_bytes).unwrap();
                    for name in ["FontSize", "Tracking", "Leading"] {
                        assert!(
                            !text.contains(name),
                            "sparse address leaked into VIEW: {name}"
                        );
                    }
                    let native =
                        crate::project_io::encode_native_project(s.editor.project(), Some(&views))
                            .unwrap();
                    assert_eq!(native, native_before);
                    let opened = crate::project_io::decode_project(&native).unwrap();
                    assert_eq!(opened.project, source);
                    assert_eq!(
                        opened.views.encode_native(&source).unwrap(),
                        view_bytes_before
                    );
                    let project_json: serde_json::Value =
                        serde_json::from_str(&source.to_json().unwrap()).unwrap();
                    assert!(project_json["version"].as_u64().unwrap() <= 48);
                    assert_eq!(value["version"], 1);
                    assert!(value["compositions"]["1"].get("graph_channels").is_none());
                    for p in TextParam::ALL {
                        assert!(
                            s.editor
                                .project()
                                .composition()
                                .layer(2)
                                .unwrap()
                                .track(PropertyPath::Text(p))
                                .is_none()
                        );
                    }
                    assert_eq!(s.editor.project(), &source);
                    assert!(!s.dirty());
                    assert!(s.editor.can_undo() && s.editor.can_redo());
                    s.editor.redo();
                    assert_eq!(
                        s.editor.project().composition().layer(1).unwrap().name(),
                        "Redo witness"
                    );
                    s.editor.undo();
                    assert_eq!(s.editor.project(), &source);
                }
            }
        }
    }

    #[test]
    fn sparse_text_focus_keeps_explicit_numeric_graph_identity_and_ranges() {
        use libre_effects_core::{Property, TextParam};
        for parameter in TextParam::ALL {
            for graph_open in [false, true] {
                for rectangle_primary in [false, true] {
                    let mut s = sparse_text_scene();
                    let active = channel(1, Property::PositionX);
                    let second = channel(2, Property::Opacity);
                    s.graph_pin_channel(active).unwrap();
                    s.graph_pin_channel(second).unwrap();
                    s.graph_activate_channel(active, true);
                    s.graph_set_channel_height(active, false, Some([-300., 900.]));
                    s.graph_set_channel_height(active, true, Some([-40., 40.]));
                    s.graph_set_channel_height(second, false, Some([0., 100.]));
                    s.editor.select(if rectangle_primary { 1 } else { 2 });
                    s.selected_layers = if rectangle_primary {
                        [1, 2].into()
                    } else {
                        [2].into()
                    };
                    s.graph_open = graph_open;
                    let source = s.editor.project().clone();
                    let channels = s.graph_channels.clone();
                    let layers = s.selected_layers.clone();
                    let keys = s.selected_keys.clone();
                    let property = PropertyPath::Text(parameter);
                    let target = GraphChannel { id: 2, property };
                    let view_before = s.capture_views();
                    let view_bytes_before = view_before.encode_native(&source).unwrap();
                    let native_before =
                        crate::project_io::encode_native_project(&source, Some(&view_before))
                            .unwrap();
                    assert!(s.graph_activate_property(target, true));
                    assert_eq!(s.editor.selected(), Some(2));
                    assert_eq!(s.selected_layers, layers);
                    assert_eq!(s.selected_keys, keys);
                    assert_eq!(s.graph_channels, channels);
                    assert_eq!(s.graph_active_channel(), Some(active));
                    assert_eq!(s.graph_property, active.property);
                    assert_eq!(s.graph_included_channels(), vec![active, second]);
                    assert_eq!(s.graph_open, graph_open);
                    assert!(!s.graph_activate_channel(target, true));
                    assert!(s.graph_pin_channel(target).is_err());
                    let views = s.capture_views();
                    let view_bytes = views.encode_native(s.editor.project()).unwrap();
                    assert_eq!(view_bytes, view_bytes_before);
                    let value: serde_json::Value = serde_json::from_slice(&view_bytes).unwrap();
                    let text = std::str::from_utf8(&view_bytes).unwrap();
                    for name in ["FontSize", "Tracking", "Leading"] {
                        assert!(
                            !text.contains(name),
                            "sparse address leaked into VIEW: {name}"
                        );
                    }
                    let native =
                        crate::project_io::encode_native_project(s.editor.project(), Some(&views))
                            .unwrap();
                    assert_eq!(native, native_before);
                    let opened = crate::project_io::decode_project(&native).unwrap();
                    assert_eq!(opened.project, source);
                    assert_eq!(
                        opened.views.encode_native(&source).unwrap(),
                        view_bytes_before
                    );
                    let project_json: serde_json::Value =
                        serde_json::from_str(&source.to_json().unwrap()).unwrap();
                    assert!(project_json["version"].as_u64().unwrap() <= 48);
                    assert_eq!(value["version"], 2);
                    assert_eq!(s.graph_channels, channels);
                    for p in TextParam::ALL {
                        assert!(
                            s.editor
                                .project()
                                .composition()
                                .layer(2)
                                .unwrap()
                                .track(PropertyPath::Text(p))
                                .is_none()
                        );
                    }
                    assert_eq!(s.editor.project(), &source);
                    assert!(!s.dirty());
                    assert!(s.editor.can_undo() && s.editor.can_redo());
                }
            }
        }
    }

    #[test]
    fn sparse_text_focus_rejects_nontext_layers_without_changing_selection() {
        use libre_effects_core::{Property, TextParam};
        let mut s = sparse_text_scene();
        s.editor.select(2);
        s.selected_layers = [2].into();
        let source = s.editor.project().clone();
        let property = s.graph_property;
        let keys = s.selected_keys.clone();
        for parameter in TextParam::ALL {
            assert!(!s.graph_activate_property(
                GraphChannel {
                    id: 1,
                    property: PropertyPath::Text(parameter)
                },
                true
            ));
        }
        assert_eq!(s.editor.selected(), Some(2));
        assert_eq!(s.selected_layers, [2].into());
        assert_eq!(s.selected_keys, keys);
        assert_eq!(s.graph_property, property);
        assert_eq!(s.editor.project(), &source);
        assert!(!s.graph_activate_property(channel(999, Property::Opacity), true));
    }
}
