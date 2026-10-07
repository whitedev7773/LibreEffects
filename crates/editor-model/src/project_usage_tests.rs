use super::*;
use libre_effects_core::{AssetId, AudioMetadata, Command, Editor, LayerSwitch};
use serde_json::{Value, json};

fn image() -> Content {
    Content::Image { png: "YWJj".into() }
}

fn import(editor: &mut Editor, content: Content) -> AssetId {
    editor
        .execute(Command::ImportAsset {
            content,
            // Equal source paths/payloads may still belong to distinct IDs.
            width: 20.0 + editor.project().asset_library().assets().len() as f64,
            height: 10.0,
            name: "Shared source".into(),
            folder: None,
            frame: None,
        })
        .unwrap();
    *editor
        .project()
        .asset_library()
        .assets()
        .keys()
        .last()
        .unwrap()
}

fn add_asset(editor: &mut Editor, asset: AssetId) -> LayerId {
    editor
        .execute(Command::AddAssetLayer { asset, frame: 0 })
        .unwrap();
    editor.selected().unwrap()
}

fn add_comp(editor: &mut Editor, composition: CompositionId) -> LayerId {
    editor
        .execute(Command::AddCompositionLayer {
            composition,
            frame: 0,
        })
        .unwrap();
    editor.selected().unwrap()
}

fn targets(uses: &[Usage]) -> Vec<(CompositionId, LayerId)> {
    uses.iter()
        .map(|usage| (usage.composition, usage.layer))
        .collect()
}

/// Intentionally bypass document validation for legacy, dangling and cyclic
/// inputs. Product loading and command validation remain unchanged.
fn raw_edit(project: &Project, edit: impl FnOnce(&mut Value)) -> Project {
    let mut raw = serde_json::to_value(project).unwrap();
    edit(&mut raw);
    serde_json::from_value(raw).unwrap()
}

#[test]
fn shared_assets_cover_every_media_form_and_use_exact_ids_in_stack_order() {
    let audio = AudioMetadata {
        stream_index: 0,
        sample_rate: 48_000,
        channels: 2,
        channel_layout: "stereo".into(),
        duration: 5.0,
        start_time: 0.0,
        file_offset: 0.0,
    };
    let contents = [
        image(),
        Content::ImageSequence {
            frames: vec!["frame.png".into()].into(),
            fps: 30.into(),
            missing: Default::default(),
            start_frame: 0,
            playback: Default::default(),
        },
        Content::Video {
            path: "clip.mp4".into(),
            audio: None,
            duration: 5.0,
            source_fps: 30.0,
            start_frame: 0,
            playback: Default::default(),
        },
        Content::Video {
            path: "sound-clip.mp4".into(),
            audio: Some(audio.clone()),
            duration: 5.0,
            source_fps: 30.0,
            start_frame: 0,
            playback: Default::default(),
        },
        Content::Audio {
            path: "sound.wav".into(),
            audio,
            start_frame: 0,
            playback: Default::default(),
        },
    ];
    for content in contents {
        let mut editor = Editor::default();
        let asset = import(&mut editor, content.clone());
        let other = import(&mut editor, content);
        let first = add_asset(&mut editor, asset);
        add_asset(&mut editor, other);
        let second = add_asset(&mut editor, asset);
        editor
            .execute(Command::MoveLayer {
                id: first,
                index: 0,
            })
            .unwrap();
        editor.execute(Command::NewComposition).unwrap();
        let third = add_asset(&mut editor, asset);
        let expected = vec![(1, first), (1, second), (2, third)];
        let uses = direct_uses(editor.project(), ProjectItem::Asset(asset));
        assert_eq!(targets(&uses), expected);
        assert_eq!(uses[0].composition_name, "Composition 01");
        assert_eq!(uses[2].composition_name, "Composition 02");
        assert!(uses.iter().all(|usage| usage.layer_name == "Shared source"));
        editor.activate_composition(1).unwrap();
        assert_eq!(
            direct_uses(editor.project(), ProjectItem::Asset(asset)),
            uses
        );
        for usage in uses {
            assert_eq!(
                resolve(
                    editor.project(),
                    ProjectItem::Asset(asset),
                    usage.composition,
                    usage.layer
                ),
                Some(usage)
            );
        }
    }
}

#[test]
fn nested_compositions_report_direct_edges_without_indirect_multiplication() {
    let mut editor = Editor::default();
    let asset = import(&mut editor, image());
    let media = add_asset(&mut editor, asset);
    editor.execute(Command::NewComposition).unwrap();
    let first = add_comp(&mut editor, 1);
    let second = add_comp(&mut editor, 1);
    editor.execute(Command::NewComposition).unwrap();
    let indirect = add_comp(&mut editor, 2);
    let direct = add_comp(&mut editor, 1);
    let project = editor.project();
    assert_eq!(
        targets(&direct_uses(project, ProjectItem::Asset(asset))),
        [(1, media)]
    );
    assert_eq!(
        targets(&direct_uses(project, ProjectItem::Composition(1))),
        [(2, second), (2, first), (3, direct)]
    );
    assert_eq!(
        targets(&direct_uses(project, ProjectItem::Composition(2))),
        [(3, indirect)]
    );
    assert!(direct_uses(project, ProjectItem::Composition(3)).is_empty());
    assert_eq!(
        resolve(project, ProjectItem::Asset(asset), 3, indirect),
        None
    );
    assert_eq!(
        resolve(project, ProjectItem::Composition(1), 3, indirect),
        None
    );
}

#[test]
fn hidden_locked_shy_and_guide_uses_are_not_visibility_filtered() {
    let mut editor = Editor::default();
    let asset = import(&mut editor, image());
    let layer = add_asset(&mut editor, asset);
    for switch in [LayerSwitch::Shy, LayerSwitch::Guide] {
        editor
            .execute(Command::SetLayerSwitch {
                id: layer,
                switch,
                enabled: true,
            })
            .unwrap();
    }
    editor.execute(Command::SetHideShy(true)).unwrap();
    editor.execute(Command::ToggleVisible(layer)).unwrap();
    editor.execute(Command::ToggleLocked(layer)).unwrap();
    assert_eq!(
        targets(&direct_uses(editor.project(), ProjectItem::Asset(asset))),
        [(1, layer)]
    );
    assert!(resolve(editor.project(), ProjectItem::Asset(asset), 1, layer).is_some());
}

#[test]
fn folders_unused_and_missing_items_have_no_uses_even_with_dangling_references() {
    let mut editor = Editor::default();
    let unused = import(&mut editor, image());
    editor
        .execute(Command::NewProjectFolder {
            name: "Media".into(),
            parent: None,
        })
        .unwrap();
    let folder = *editor
        .project()
        .asset_library()
        .folders()
        .keys()
        .next()
        .unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    let layer = editor.selected().unwrap();
    let project = raw_edit(editor.project(), |raw| {
        raw["composition"]["layers"][0]["asset"] = json!(999);
        raw["composition"]["layers"][0]["content"] = json!({
            "Composition": { "composition": 999, "start_frame": 0 }
        });
    });
    for item in [
        ProjectItem::Asset(unused),
        ProjectItem::Asset(999),
        ProjectItem::Composition(999),
        ProjectItem::Folder(folder),
        ProjectItem::Folder(999),
    ] {
        assert!(direct_uses(&project, item).is_empty());
        assert_eq!(resolve(&project, item, 1, layer), None);
    }
}

#[test]
fn legacy_media_without_an_asset_id_is_never_matched_by_path_or_payload() {
    let mut editor = Editor::default();
    let asset = import(&mut editor, image());
    let layer = add_asset(&mut editor, asset);
    let project = raw_edit(editor.project(), |raw| {
        raw["composition"]["layers"][0]
            .as_object_mut()
            .unwrap()
            .remove("asset");
    });
    assert_eq!(project.composition().layer(layer).unwrap().asset_id(), None);
    assert!(direct_uses(&project, ProjectItem::Asset(asset)).is_empty());
    assert_eq!(resolve(&project, ProjectItem::Asset(asset), 1, layer), None);
}

#[test]
fn cyclic_and_self_references_are_finite_direct_edges() {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    let first = editor.selected().unwrap();
    editor.execute(Command::NewComposition).unwrap();
    let second = add_comp(&mut editor, 1);
    let project = raw_edit(editor.project(), |raw| {
        raw["other_compositions"]["1"]["layers"][0]["content"] = json!({
            "Composition": { "composition": 2, "start_frame": 0 }
        });
    });
    assert_eq!(
        targets(&direct_uses(&project, ProjectItem::Composition(1))),
        [(2, second)]
    );
    assert_eq!(
        targets(&direct_uses(&project, ProjectItem::Composition(2))),
        [(1, first)]
    );
    assert!(resolve(&project, ProjectItem::Composition(2), 1, first).is_some());
    let self_reference = raw_edit(&project, |raw| {
        raw["composition"]["layers"][0]["content"]["Composition"]["composition"] = json!(2);
    });
    assert_eq!(
        targets(&direct_uses(&self_reference, ProjectItem::Composition(2))),
        [(1, first), (2, second)]
    );
}

#[test]
fn resolver_rejects_missing_layer_composition_source_and_repointed_asset() {
    let mut editor = Editor::default();
    let asset = import(&mut editor, image());
    let other = import(&mut editor, image());
    let layer = add_asset(&mut editor, asset);
    let item = ProjectItem::Asset(asset);
    assert_eq!(resolve(editor.project(), item, 999, layer), None);
    assert_eq!(resolve(editor.project(), item, 1, 999), None);
    let repointed = raw_edit(editor.project(), |raw| {
        raw["composition"]["layers"][0]["asset"] = json!(other);
    });
    assert_eq!(resolve(&repointed, item, 1, layer), None);
    assert!(resolve(&repointed, ProjectItem::Asset(other), 1, layer).is_some());
    let removed_source = raw_edit(editor.project(), |raw| {
        raw["asset_library"]["assets"]
            .as_object_mut()
            .unwrap()
            .remove(&asset.to_string());
    });
    assert_eq!(resolve(&removed_source, item, 1, layer), None);
    editor.execute(Command::RemoveLayer(layer)).unwrap();
    assert_eq!(resolve(editor.project(), item, 1, layer), None);
    editor.undo();
    assert!(resolve(editor.project(), item, 1, layer).is_some());
    editor.execute(Command::NewComposition).unwrap();
    let removed_comp = editor.project().active_composition_id();
    let removed_layer = add_asset(&mut editor, asset);
    editor.execute(Command::DeleteComposition).unwrap();
    assert_eq!(
        resolve(editor.project(), item, removed_comp, removed_layer),
        None
    );
}

#[test]
fn resolver_rejects_repointed_composition_and_changed_content_kind() {
    let mut editor = Editor::default();
    editor.execute(Command::NewComposition).unwrap();
    editor.execute(Command::NewComposition).unwrap();
    let layer = add_comp(&mut editor, 1);
    let item = ProjectItem::Composition(1);
    assert!(resolve(editor.project(), item, 3, layer).is_some());
    let repointed = raw_edit(editor.project(), |raw| {
        raw["composition"]["layers"][0]["content"]["Composition"]["composition"] = json!(2);
    });
    assert_eq!(resolve(&repointed, item, 3, layer), None);
    assert!(resolve(&repointed, ProjectItem::Composition(2), 3, layer).is_some());
    let changed = raw_edit(editor.project(), |raw| {
        raw["composition"]["layers"][0]["content"] = json!("Rectangle");
    });
    assert_eq!(resolve(&changed, item, 3, layer), None);
    let removed_source = raw_edit(editor.project(), |raw| {
        raw["other_compositions"]
            .as_object_mut()
            .unwrap()
            .remove("1");
    });
    assert_eq!(resolve(&removed_source, item, 3, layer), None);
}

#[test]
fn renamed_reordered_and_roundtripped_targets_keep_ids_and_live_display_names() {
    let mut editor = Editor::default();
    let asset = import(&mut editor, image());
    let layer = add_asset(&mut editor, asset);
    let other_layer = add_asset(&mut editor, asset);
    let item = ProjectItem::Asset(asset);
    let displayed = direct_uses(editor.project(), item)[1].clone();
    editor
        .execute(Command::RenameLayer {
            id: layer,
            name: "Renamed layer".into(),
        })
        .unwrap();
    editor
        .execute(Command::RenameProjectItem {
            item: ProjectItem::Composition(1),
            name: "Renamed comp".into(),
        })
        .unwrap();
    editor
        .execute(Command::RenameProjectItem {
            item,
            name: "Renamed source".into(),
        })
        .unwrap();
    editor
        .execute(Command::MoveLayer {
            id: layer,
            index: 0,
        })
        .unwrap();
    let project = Project::from_json(&editor.project().to_json().unwrap()).unwrap();
    let resolved = resolve(&project, item, displayed.composition, displayed.layer).unwrap();
    assert_eq!((resolved.composition, resolved.layer), (1, layer));
    assert_eq!(resolved.composition_name, "Renamed comp");
    assert_eq!(resolved.layer_name, "Renamed layer");
    assert_eq!(
        targets(&direct_uses(&project, item)),
        [(1, layer), (1, other_layer)]
    );
}

#[test]
fn repeated_queries_preserve_serialized_source_selection_context_and_history() {
    let mut editor = Editor::default();
    let asset = import(&mut editor, image());
    let layer = add_asset(&mut editor, asset);
    let before_edit = editor.project().clone();
    editor
        .execute(Command::RenameLayer {
            id: layer,
            name: "Edited".into(),
        })
        .unwrap();
    let after_edit = editor.project().clone();
    editor.execute(Command::AddRectangle).unwrap();
    editor.undo();
    let before = editor.project().clone();
    let serialized = before.to_json().unwrap();
    let state = (
        editor.selected(),
        editor.context_generation(),
        editor.can_undo(),
        editor.can_redo(),
    );
    assert!(state.2 && state.3);
    for _ in 0..3 {
        for item in [
            ProjectItem::Asset(asset),
            ProjectItem::Composition(1),
            ProjectItem::Folder(99),
        ] {
            direct_uses(editor.project(), item);
            resolve(editor.project(), item, 1, layer);
        }
    }
    assert_eq!(editor.project(), &before);
    assert!(editor.project().same_document(&before));
    assert_eq!(editor.project().to_json().unwrap(), serialized);
    assert_eq!(
        (
            editor.selected(),
            editor.context_generation(),
            editor.can_undo(),
            editor.can_redo()
        ),
        state
    );
    editor.redo();
    assert_eq!(editor.project().composition().layers().len(), 2);
    editor.undo();
    assert_eq!(editor.project(), &after_edit);
    editor.undo();
    assert_eq!(editor.project(), &before_edit);
}
