//! Native effect policy and storage contracts; no private reference data.
use libre_effects_core::*;

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor.execute(Command::AddAdjustment).unwrap();
    editor
        .execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::GaussianBlur),
        })
        .unwrap();
    editor
}
fn mode(editor: &mut Editor, mode: GaussianEdgeMode) -> Result<(), String> {
    editor.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::SetGaussianEdgeMode { effect: 1, mode },
    })
}
fn effect(editor: &Editor) -> &EffectInstance {
    &editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .effect_stack()[0]
}
#[test]
fn gaussian_edge_default_noop_history_and_schema_are_exact() {
    let mut editor = scene();
    let old = editor.project().to_json().unwrap();
    assert!(!old.contains("gaussian_edge_mode"));
    mode(&mut editor, GaussianEdgeMode::Repeat).unwrap();
    let repeat = editor.project().to_json().unwrap();
    assert_eq!(
        effect(&editor).gaussian_edge_mode(),
        GaussianEdgeMode::Repeat
    );
    let raw: serde_json::Value = serde_json::from_str(&repeat).unwrap();
    assert_eq!(raw["version"], 78);
    let mut bad = raw.clone();
    bad["version"] = 77.into();
    assert!(Project::from_json(&bad.to_string()).is_err());
    assert_eq!(
        Project::from_json(&repeat).unwrap().to_json().unwrap(),
        repeat
    );
    editor.undo();
    assert_eq!(editor.project().to_json().unwrap(), old);
    // An unchanged Off action must leave the pending Redo branch intact.
    mode(&mut editor, GaussianEdgeMode::Transparent).unwrap();
    assert_eq!(editor.project().to_json().unwrap(), old);
    editor.redo();
    assert_eq!(editor.project().to_json().unwrap(), repeat);
    mode(&mut editor, GaussianEdgeMode::Transparent).unwrap();
    assert!(
        !editor
            .project()
            .to_json()
            .unwrap()
            .contains("gaussian_edge_mode")
    );
    editor.undo();
    assert_eq!(editor.project().to_json().unwrap(), repeat);
}
#[test]
fn gaussian_edge_duplicate_reset_preset_and_unrelated_edits_preserve_policy() {
    let mut editor = scene();
    mode(&mut editor, GaussianEdgeMode::Repeat).unwrap();
    editor
        .execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Duplicate(1),
        })
        .unwrap();
    let layer = editor.project().composition().layer(1).unwrap();
    assert!(
        layer
            .effect_stack()
            .iter()
            .all(|e| e.gaussian_edge_mode() == GaussianEdgeMode::Repeat)
    );
    let preset = EffectPreset::capture(layer, None, 30.into(), "Edge policy").unwrap();
    let serialized = preset.to_json().unwrap();
    let mut raw: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(raw["version"], 5);
    raw["version"] = 4.into();
    assert!(EffectPreset::from_json(&raw.to_string()).is_err());
    editor
        .execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Reset(1),
        })
        .unwrap();
    assert_eq!(
        effect(&editor).gaussian_edge_mode(),
        GaussianEdgeMode::Transparent
    );
    assert_eq!(
        editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .effect_stack()[1]
            .gaussian_edge_mode(),
        GaussianEdgeMode::Repeat
    );
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Still repeated".into(),
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(editor.project()).unwrap()["version"],
        78
    );
    let mut destination = Editor::default();
    destination.execute(Command::AddRectangle).unwrap();
    destination
        .execute(Command::Effect {
            id: 1,
            edit: EffectEdit::ApplyPreset {
                preset: EffectPreset::from_json(&serialized).unwrap(),
                frame: 0,
            },
        })
        .unwrap();
    assert!(
        destination
            .selected_layer()
            .unwrap()
            .effect_stack()
            .iter()
            .all(|e| e.gaussian_edge_mode() == GaussianEdgeMode::Repeat)
    );
    assert_eq!(
        serde_json::to_value(destination.project()).unwrap()["version"],
        78
    );
}
#[test]
fn gaussian_edge_invalid_owner_legacy_mode_and_locked_edits_roll_back() {
    let mut editor = scene();
    editor
        .execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::Fill),
        })
        .unwrap();
    let before = editor.project().clone();
    assert!(
        editor
            .execute(Command::Batch(vec![
                Command::RenameLayer {
                    id: 1,
                    name: "Must roll back".into()
                },
                Command::Effect {
                    id: 1,
                    edit: EffectEdit::SetGaussianEdgeMode {
                        effect: 2,
                        mode: GaussianEdgeMode::Repeat
                    }
                }
            ]))
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    let mut raw = serde_json::to_value(&before).unwrap();
    raw["composition"]["layers"][0]["effect_stack"][0]["color_space"] = "LinearRgb".into();
    editor
        .replace_project(Project::from_json(&raw.to_string()).unwrap())
        .unwrap();
    let before = editor.project().clone();
    assert!(mode(&mut editor, GaussianEdgeMode::Repeat).is_err());
    assert_eq!(editor.project(), &before);
    editor.execute(Command::ToggleLocked(1)).unwrap();
    let before = editor.project().clone();
    assert!(mode(&mut editor, GaussianEdgeMode::Transparent).is_err());
    assert_eq!(editor.project(), &before);
    // Forged Repeat metadata on a non-Gaussian effect is invalid even in schema78.
    let mut raw = serde_json::to_value(&before).unwrap();
    raw["version"] = 78.into();
    raw["composition"]["layers"][0]["effect_stack"][1]["gaussian_edge_mode"] = "Repeat".into();
    assert!(Project::from_json(&raw.to_string()).is_err());
}
