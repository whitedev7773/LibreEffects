//! Current-selection routing for the bounded, source-rectangle layer transforms.
use super::{Action, EditorState};
use libre_effects_core::{Command, Content, LayerTransformOp};

pub(crate) const ENTRIES: [(&str, LayerTransformOp); 5] = [
    (
        "Reset Scale & Rotation",
        LayerTransformOp::ResetScaleRotation,
    ),
    ("Flip Horizontal", LayerTransformOp::FlipHorizontal),
    ("Flip Vertical", LayerTransformOp::FlipVertical),
    (
        "Fit Layer Inside Composition",
        LayerTransformOp::FitInsideComposition,
    ),
    (
        "Center Anchor in Source Bounds",
        LayerTransformOp::CenterAnchorInSourceBounds,
    ),
];

#[derive(Clone, Copy, Default)]
pub(crate) struct Availability {
    local: bool,
    source_bounds: bool,
}
impl Availability {
    pub(crate) fn allows(self, operation: LayerTransformOp) -> bool {
        match operation {
            LayerTransformOp::ResetScaleRotation
            | LayerTransformOp::FlipHorizontal
            | LayerTransformOp::FlipVertical => self.local,
            LayerTransformOp::FitInsideComposition
            | LayerTransformOp::CenterAnchorInSourceBounds => self.source_bounds,
        }
    }
}

impl EditorState {
    /// One inexpensive read-only pass for all five menu/search entries. This is
    /// only structural availability: the atomic core command validates numeric
    /// results at invocation, including singular/collapsed Fit transforms.
    pub(crate) fn layer_transform_availability(&self) -> Availability {
        let composition = self.editor.project().composition();
        // Match other multi-layer commands exactly: no implicit primary fallback.
        let ids: Vec<_> = self.selected_layers.iter().copied().collect();
        if self.frame >= composition.duration() || composition.selection_roots(&ids).is_err() {
            return Availability::default();
        }
        let mut availability = Availability {
            local: true,
            source_bounds: true,
        };
        // Validate every selected member, including descendants carried by a
        // selected ancestor. Checking roots alone would hide a rejected member.
        for id in ids {
            let Some(layer) = composition.layer(id) else {
                return Availability::default();
            };
            // These operations edit independent planar transform channels.
            // Joined spatial geometry is currently authored through scripting.
            if layer.is_three_d() {
                return Availability::default();
            }
            match layer.content() {
                Content::Audio { .. } => return Availability::default(),
                Content::Null => availability.source_bounds = false,
                _ => {}
            }
        }
        availability
    }
}

impl Action {
    /// Rendered menus store only the operation. Commit active field/text edits
    /// and pass modal guards before asking for this current-context command.
    pub(crate) fn layer_transform_command(&self, state: &EditorState) -> Option<Command> {
        let Self::TransformLayers(operation) = self else {
            return None;
        };
        Some(Command::TransformLayers {
            ids: state.selected_layers.iter().copied().collect(),
            frame: state.frame,
            operation: *operation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{AudioMetadata, Frame, KeyRef, LayerId, Property};

    fn rectangles(count: usize) -> EditorState {
        let mut state = EditorState::default();
        for _ in 0..count {
            state.editor.execute(Command::AddRectangle).unwrap();
        }
        state.selected_layers = (1..=count as u64).collect();
        state
    }
    fn set(state: &mut EditorState, id: LayerId, property: Property, value: f64) {
        state
            .editor
            .execute(Command::SetValue {
                id,
                property,
                frame: state.frame,
                value,
            })
            .unwrap();
    }
    fn value(state: &EditorState, id: LayerId, property: Property, frame: Frame) -> f64 {
        state
            .editor
            .project()
            .composition()
            .layer(id)
            .unwrap()
            .property(property)
            .expect("2D test layer has scalar transform tracks")
            .value_at(frame)
    }
    fn invoke(state: &mut EditorState, action: &Action) {
        let command = action.layer_transform_command(state).unwrap();
        state.apply_edit(&command);
        state.normalize();
    }
    fn add_audio(state: &mut EditorState) -> LayerId {
        state
            .editor
            .execute(Command::ImportAsset {
                content: Content::Audio {
                    path: "transform-test.wav".into(),
                    audio: AudioMetadata {
                        stream_index: 0,
                        sample_rate: 48_000,
                        channels: 2,
                        channel_layout: "stereo".into(),
                        duration: 5.0,
                        start_time: 0.0,
                        file_offset: 0.0,
                    },
                    start_frame: 0,
                    playback: Default::default(),
                },
                width: 1.0,
                height: 1.0,
                name: "Audio".into(),
                folder: None,
                frame: Some(0),
            })
            .unwrap();
        state.editor.selected().unwrap()
    }

    #[test]
    fn spatial_member_disables_all_planar_transform_menu_operations() {
        let mut state = rectangles(2);
        state
            .editor
            .execute(Command::SetThreeD {
                id: 2,
                enabled: true,
            })
            .unwrap();
        for (_, operation) in ENTRIES {
            assert!(!state.layer_transform_availability().allows(operation));
        }
        state.selected_layers = [1].into();
        for (_, operation) in ENTRIES {
            assert!(state.layer_transform_availability().allows(operation));
        }
    }

    #[test]
    fn availability_checks_the_exact_complete_selection_once_for_all_operations() {
        let mut state = rectangles(2);
        for (_, operation) in ENTRIES {
            assert!(state.layer_transform_availability().allows(operation));
        }
        state.selected_layers.clear();
        // A retained core primary is not a multi-layer selection.
        assert!(state.editor.selected().is_some());
        for (_, operation) in ENTRIES {
            assert!(!state.layer_transform_availability().allows(operation));
        }
        state.selected_layers = [1, 2].into();
        state
            .editor
            .execute(Command::SetParent {
                id: 2,
                parent: Some(1),
                frame: 0,
            })
            .unwrap();
        state.editor.execute(Command::ToggleLocked(2)).unwrap();
        for (_, operation) in ENTRIES {
            assert!(!state.layer_transform_availability().allows(operation));
        }
        state.editor.execute(Command::ToggleLocked(2)).unwrap();
        state.selected_layers.insert(999);
        for (_, operation) in ENTRIES {
            assert!(!state.layer_transform_availability().allows(operation));
        }
        state.selected_layers = [1].into();
        state.frame = state.editor.project().composition().duration();
        assert!(
            !state
                .layer_transform_availability()
                .allows(LayerTransformOp::FlipHorizontal)
        );
    }

    #[test]
    fn null_and_audio_members_have_operation_specific_availability() {
        let mut state = rectangles(1);
        state.editor.execute(Command::AddNull).unwrap();
        state.selected_layers = [1, 2].into();
        for operation in [
            LayerTransformOp::ResetScaleRotation,
            LayerTransformOp::FlipHorizontal,
            LayerTransformOp::FlipVertical,
        ] {
            assert!(state.layer_transform_availability().allows(operation));
        }
        for operation in [
            LayerTransformOp::FitInsideComposition,
            LayerTransformOp::CenterAnchorInSourceBounds,
        ] {
            assert!(!state.layer_transform_availability().allows(operation));
        }
        let audio = add_audio(&mut state);
        for ids in [vec![audio], vec![1, audio]] {
            state.selected_layers = ids.into_iter().collect();
            for (_, operation) in ENTRIES {
                assert!(!state.layer_transform_availability().allows(operation));
            }
        }
    }

    #[test]
    fn cached_action_plans_current_ids_and_frame_after_field_commit() {
        let mut state = rectangles(2);
        state.selected_layers = [1].into();
        let cached = Action::TransformLayers(LayerTransformOp::FlipHorizontal);
        state.frame = 18;
        state.selected_layers = [2].into();
        // A normal property-field commit is completed before the action planner.
        set(&mut state, 2, Property::ScaleX, 75.0);
        state
            .editor
            .execute(Command::ToggleAnimation {
                id: 2,
                property: Property::ScaleX,
                frame: 0,
            })
            .unwrap();
        let command = cached.layer_transform_command(&state).unwrap();
        assert!(matches!(command, Command::TransformLayers {
            ids, frame: 18, operation: LayerTransformOp::FlipHorizontal
        } if ids == vec![2]));
        invoke(&mut state, &cached);
        assert_eq!(value(&state, 1, Property::ScaleX, 18), 100.0);
        assert_eq!(value(&state, 2, Property::ScaleX, 0), 75.0);
        assert_eq!(value(&state, 2, Property::ScaleX, 18), -75.0);
        assert_eq!(state.selected_layers, [2].into());
    }

    #[test]
    fn transform_action_preserves_selection_and_unrelated_panel_context_and_stops_playback() {
        let mut state = rectangles(2);
        state
            .editor
            .execute(Command::ToggleAnimation {
                id: 2,
                property: Property::Opacity,
                frame: 0,
            })
            .unwrap();
        let key = KeyRef {
            id: 2,
            property: Property::Opacity.into(),
            frame: 0,
        };
        state.graph_property = key.property;
        state.graph_pin_channel(key.into()).unwrap();
        state.graph_external_activation(2, key.property);
        state.graph_key = Some(key);
        state.selected_keys = [key].into();
        let graph = state.graph_channels.clone();
        state.playing = true;
        state.preview_caching = true;
        let generation = state.transport_generation();
        invoke(
            &mut state,
            &Action::TransformLayers(LayerTransformOp::FlipVertical),
        );
        assert_eq!(state.status, "Edited");
        assert_eq!(state.selected_layers, [1, 2].into());
        assert_eq!(state.editor.selected(), Some(2));
        assert_eq!(state.selected_keys, [key].into());
        assert_eq!(state.graph_key, Some(key));
        assert_eq!(state.graph_channels, graph);
        assert!(!state.playing);
        assert!(!state.preview_caching);
        assert!(state.transport_generation() > generation);
    }

    #[test]
    fn invalid_cached_action_rejects_all_members_without_history_or_partial_edits() {
        let action = Action::TransformLayers(LayerTransformOp::FlipHorizontal);
        let mut state = rectangles(2);
        state.editor.execute(Command::ToggleLocked(2)).unwrap();
        let before = state.editor.project().clone();
        let selection = state.selected_layers.clone();
        state.editor.clear_history();
        invoke(&mut state, &action);
        assert_ne!(state.status, "Edited");
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.selected_layers, selection);
        assert!(!state.editor.can_undo());
        assert!(!state.editor.can_redo());
    }

    #[test]
    fn layer_transform_noop_preserves_redo_and_history_keeps_primary_selection_policy() {
        let mut state = rectangles(2);
        state.editor.clear_history();
        let before = state.editor.project().clone();
        invoke(
            &mut state,
            &Action::TransformLayers(LayerTransformOp::FlipHorizontal),
        );
        let after = state.editor.project().clone();
        assert_ne!(before, after);
        assert_eq!(state.selected_layers, [1, 2].into());
        state.step_history(false);
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.selected_layers, [2].into());
        assert!(!state.editor.can_undo());
        assert!(state.editor.can_redo());
        state.selected_layers = [1, 2].into();
        invoke(
            &mut state,
            &Action::TransformLayers(LayerTransformOp::ResetScaleRotation),
        );
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.selected_layers, [1, 2].into());
        assert!(!state.editor.can_undo());
        assert!(state.editor.can_redo());
        state.step_history(true);
        assert_eq!(state.editor.project(), &after);
        assert_eq!(state.selected_layers, [2].into());
    }

    #[test]
    fn transform_keeps_the_explicit_contents_group_target() {
        use libre_effects_core::{ContentsEdit, ContentsKind, ShapeContents};
        let mut state = EditorState::default();
        state
            .editor
            .execute(Command::AddContent {
                content: Content::ShapeContents(ShapeContents::default()),
                width: 500.0,
                height: 300.0,
                name: "Contents".into(),
            })
            .unwrap();
        state
            .editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 0,
                    kind: ContentsKind::Group(vec![]),
                },
            })
            .unwrap();
        state.selected_layers = [1].into();
        let target = (state.editor.project().active_composition_id(), 1, 1);
        state.contents_selection = Some(target);
        invoke(
            &mut state,
            &Action::TransformLayers(LayerTransformOp::CenterAnchorInSourceBounds),
        );
        assert_eq!(state.status, "Edited");
        assert_eq!(state.contents_selection, Some(target));
        assert_eq!(state.selected_layers, [1].into());
    }

    #[test]
    fn structurally_available_fit_still_rejects_numeric_failure_atomically() {
        let mut state = rectangles(2);
        set(&mut state, 2, Property::ScaleX, 0.0);
        assert!(
            state
                .layer_transform_availability()
                .allows(LayerTransformOp::FitInsideComposition)
        );
        let before = state.editor.project().clone();
        state.editor.clear_history();
        invoke(
            &mut state,
            &Action::TransformLayers(LayerTransformOp::FitInsideComposition),
        );
        assert_ne!(state.status, "Edited");
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.selected_layers, [1, 2].into());
        assert!(!state.editor.can_undo());
    }

    #[test]
    fn ordinary_edit_helper_keeps_existing_selection_effect_and_rejection_behavior() {
        use libre_effects_core::{EffectEdit, EffectKind};
        let mut state = rectangles(2);
        state.apply_edit(&Command::SetValue {
            id: 1,
            property: Property::ScaleX,
            frame: 0,
            value: 80.0,
        });
        state.normalize();
        assert_eq!(state.status, "Edited");
        assert_eq!(state.selected_layers, [1, 2].into());
        state.apply_edit(&Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::GaussianBlur),
        });
        assert!(state.effect_controls_open);
        assert_eq!(state.selected_layers, [1, 2].into());
        state.apply_edit(&Command::ToggleLocked(1));
        let before = state.editor.project().clone();
        state.apply_edit(&Command::SetValue {
            id: 1,
            property: Property::ScaleX,
            frame: 0,
            value: 90.0,
        });
        state.normalize();
        assert_ne!(state.status, "Edited");
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.selected_layers, [1, 2].into());
        state.apply_edit(&Command::AddRectangle);
        state.normalize();
        assert_eq!(state.selected_layers, [3].into());
        assert_eq!(state.editor.selected(), Some(3));
        state.apply_edit(&Command::NewComposition);
        state.normalize();
        assert!(state.selected_layers.is_empty());
        assert!(state.selected_keys.is_empty());
        assert_eq!(state.editor.selected(), None);
    }

    #[test]
    fn transform_actions_follow_existing_modal_text_and_file_dispatch_permissions() {
        let state = EditorState::default();
        for (_, operation) in ENTRIES {
            let action = Action::TransformLayers(operation);
            assert!(!action.allowed_in_vertex_editor());
            assert!(!action.allowed_in_gradient_editor());
            assert!(!action.allowed_in_color_editor());
            assert!(action.commits_text_before_dispatch());
            assert!(!state.should_blur_for_file_action(&action));
        }
        assert!(Action::ApplyGradient.allowed_in_gradient_editor());
        assert!(Action::CancelGradient.allowed_in_gradient_editor());
        assert!(Action::PickColor.allowed_in_color_editor());
        assert!(Action::ApplyColor.allowed_in_color_editor());
        assert!(!Action::CommitText.commits_text_before_dispatch());
        assert!(!Action::CancelText.commits_text_before_dispatch());
        assert!(Action::Seek(12).layer_transform_command(&state).is_none());
    }
}
