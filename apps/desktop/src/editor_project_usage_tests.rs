use super::*;
use crate::view_state::GraphChannel;

fn fixture() -> EditorState {
    let mut state = EditorState::default();
    state
        .editor
        .execute(Command::ImportAsset {
            content: Content::Image { png: "YWJj".into() },
            width: 20.0,
            height: 10.0,
            name: "Shared source".into(),
            folder: None,
            frame: Some(0),
        })
        .unwrap();
    state
        .editor
        .execute(Command::AddAssetLayer { asset: 1, frame: 0 })
        .unwrap();
    state.selected_layers = [2].into();
    state.project_item = Some(ProjectItem::Asset(1));
    state.saved = state.editor.project().clone();
    state
}

fn target(state: &EditorState, composition: CompositionId, layer: LayerId) -> Target {
    Target {
        item: state.project_item.unwrap(),
        composition,
        layer,
        document_revision: state.document_revision,
        editor_generation: state.editor.context_generation(),
        input_generation: state.input_context_generation(),
    }
}

#[test]
fn same_comp_jump_preserves_source_redo_view_and_playhead_and_leaves_keys() {
    let mut state = fixture();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 2,
            name: "Temporary".into(),
        })
        .unwrap();
    let renamed = state.editor.project().clone();
    state.editor.undo();
    let source = state.editor.project().clone();
    let source_json = source.to_json().unwrap();
    let channel = GraphChannel {
        id: 2,
        property: Property::Opacity.into(),
    };
    state.graph_channels.explicit = true;
    state.graph_channels.active = Some(channel);
    state.graph_channels.pinned = vec![channel];
    state.frame = 35;
    state.timeline_zoom = 4.0;
    state.timeline_start = 20;
    let view = state.capture_views();
    let key = KeyRef {
        id: 2,
        property: Property::Opacity.into(),
        frame: 0,
    };
    state.selected_keys.insert(key);
    state.graph_key = Some(key);
    state.colors_key_owned.set(true);
    assert!(state.show_project_usage(target(&state, 1, 1)));
    assert_eq!(state.editor.selected(), Some(1));
    assert_eq!(state.selected_layers, [1].into());
    assert!(state.selected_keys.is_empty());
    assert!(state.graph_key.is_none());
    assert!(!state.colors_key_owned.get());
    assert_eq!(state.capture_views(), view);
    assert_eq!(state.editor.project(), &source);
    assert_eq!(state.editor.project().to_json().unwrap(), source_json);
    assert!(!state.dirty());
    assert!(state.editor.can_undo() && state.editor.can_redo());
    state.editor.redo();
    assert_eq!(state.editor.project(), &renamed);
}

#[test]
fn repeated_current_layer_jump_clears_key_selection_without_source_edits() {
    let mut state = fixture();
    let source = state.editor.project().clone();
    for _ in 0..3 {
        state.selected_keys.insert(KeyRef {
            id: 2,
            property: Property::Opacity.into(),
            frame: 0,
        });
        let before = state.input_context_generation();
        assert!(state.show_project_usage(target(&state, 1, 2)));
        assert!(state.selected_keys.is_empty());
        assert!(state.input_context_generation() > before);
        assert_eq!(state.editor.project(), &source);
        assert_eq!(state.editor.selected(), Some(2));
    }
}

#[test]
fn cross_comp_jump_restores_existing_view_without_changing_source_or_history() {
    let mut state = fixture();
    state.frame = 42;
    state.timeline_zoom = 3.0;
    state.remember_view();
    state.editor.execute(Command::NewComposition).unwrap();
    state.composition_changed();
    state
        .editor
        .execute(Command::AddCompositionLayer {
            composition: 1,
            frame: 0,
        })
        .unwrap();
    state.project_item = Some(ProjectItem::Asset(1));
    state.saved = state.editor.project().clone();
    let source = state.editor.project().clone();
    let history = (state.editor.can_undo(), state.editor.can_redo());
    state.frame = 8;
    assert!(state.show_project_usage(target(&state, 1, 1)));
    assert_eq!(state.editor.project().active_composition_id(), 1);
    assert_eq!(state.editor.selected(), Some(1));
    assert_eq!((state.frame, state.timeline_zoom), (42, 3.0));
    assert!(state.editor.project().same_document(&source));
    assert!(!state.dirty());
    assert_eq!((state.editor.can_undo(), state.editor.can_redo()), history);
    assert_eq!(state.composition_views[&2].frame, 8);
    assert_eq!(
        state.project_usage_reveal,
        Some((
            state.document_revision,
            1,
            1,
            state.input_context_generation()
        ))
    );
}

#[test]
fn stale_missing_wrong_item_and_busy_targets_are_noops() {
    let mut state = fixture();
    let valid = target(&state, 1, 1);
    for invalid in [
        Target {
            layer: 999,
            ..valid
        },
        Target {
            composition: 999,
            ..valid
        },
        Target {
            item: ProjectItem::Asset(999),
            ..valid
        },
        Target {
            document_revision: valid.document_revision + 1,
            ..valid
        },
        Target {
            editor_generation: valid.editor_generation + 1,
            ..valid
        },
        Target {
            input_generation: valid.input_generation + 1,
            ..valid
        },
    ] {
        let source = state.editor.project().clone();
        let generation = state.input_context_generation();
        let view = state.capture_views();
        assert!(!state.show_project_usage(invalid));
        assert_eq!(state.editor.project(), &source);
        assert_eq!(state.editor.selected(), Some(2));
        assert_eq!(state.input_context_generation(), generation);
        assert_eq!(state.capture_views(), view);
        assert!(state.project_usage_reveal.is_none());
    }
    state.playing = true;
    assert!(!state.show_project_usage(valid));
    assert!(state.playing);
    state.playing = false;
    state.preview_scrub = true;
    assert!(!state.show_project_usage(valid));
    state.preview_scrub = false;
    state.project_item = Some(ProjectItem::Composition(1));
    assert!(!state.show_project_usage(valid));
}

#[test]
fn hidden_shy_use_is_selected_without_mutating_hide_shy() {
    let mut state = fixture();
    state
        .editor
        .execute(Command::SetLayerSwitch {
            id: 1,
            switch: libre_effects_core::LayerSwitch::Shy,
            enabled: true,
        })
        .unwrap();
    state.editor.execute(Command::SetHideShy(true)).unwrap();
    let source = state.editor.project().clone();
    assert!(state.show_project_usage(target(&state, 1, 1)));
    assert_eq!(state.editor.selected(), Some(1));
    assert!(state.status.contains("Turn off Hide Shy"));
    assert_eq!(state.editor.project(), &source);
}
