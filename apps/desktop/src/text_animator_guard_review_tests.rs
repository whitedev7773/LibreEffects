//! Independent public-core regressions for animator command routing.
//! Fixtures deserialize the resident model directly so the document loader's
//! deliberate historical migration cannot hide an editing-route mutation.
use libre_effects_core::{
    Command, Content, Editor, Interpolation, KeyRef, KeyScale, Project, PropertyPath, TemporalMode,
    TextParam, TrackEdit,
};

const PARAMETERS: [TextParam; 6] = [
    TextParam::AnimatorStart,
    TextParam::AnimatorEnd,
    TextParam::AnimatorPositionX,
    TextParam::AnimatorPositionY,
    TextParam::AnimatorOpacity,
    TextParam::AnimatorOffset,
];
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a4WQAAAAASUVORK5CYII=";

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Independent animator source".into(),
                font_size: 40.,
            },
            width: 400.,
            height: 180.,
            name: "Text".into(),
        })
        .unwrap();
    editor
}

fn legacy_media(version: u32) -> Project {
    let mut editor = scene();
    editor
        .execute(Command::AddContent {
            content: Content::Image { png: PNG.into() },
            width: 1.,
            height: 1.,
            name: "Unmigrated media".into(),
        })
        .unwrap();
    let mut value = serde_json::to_value(editor.project()).unwrap();
    value["version"] = version.into();
    value.as_object_mut().unwrap().remove("asset_library");
    for layer in value["composition"]["layers"].as_array_mut().unwrap() {
        layer.as_object_mut().unwrap().remove("asset");
    }
    serde_json::from_value(value).unwrap()
}

fn install_with_redo(editor: &mut Editor, project: Project) {
    editor.replace_project(project.clone()).unwrap();
    editor.select(1);
    editor.clear_history();
    let mut future = serde_json::to_value(project).unwrap();
    future["composition"]["name"] = "Unconsumed future".into();
    editor
        .replace_project(serde_json::from_value(future).unwrap())
        .unwrap();
    editor.undo();
    assert!(editor.can_redo());
    assert!(!editor.can_undo());
}

#[test]
fn animator_review_key_paste_cannot_migrate_unrelated_legacy_media() {
    for parameter in PARAMETERS {
        let mut source = scene();
        source
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleKey { frame: 7 },
            })
            .unwrap();
        let copied = source
            .selected_layer()
            .unwrap()
            .copy_key(PropertyPath::Text(parameter), 7)
            .unwrap();
        for version in [7, 21] {
            for nested in [false, true] {
                let mut destination = Editor::default();
                install_with_redo(&mut destination, legacy_media(version));
                let original = serde_json::to_vec(destination.project()).unwrap();
                let paste = Command::PasteKeys {
                    keys: vec![copied.clone()],
                    frame: 23,
                    target: Some(1),
                };
                let command = if nested {
                    Command::Batch(vec![Command::Batch(vec![]), Command::Batch(vec![paste])])
                } else {
                    paste
                };
                assert!(
                    destination.execute(command).is_err(),
                    "animator-only paste migrated schema {version} media for {parameter:?}"
                );
                assert_eq!(serde_json::to_vec(destination.project()).unwrap(), original);
                assert_eq!(destination.selected(), Some(1));
                assert!(destination.can_redo());
                assert!(!destination.can_undo());
                destination.redo();
                assert_eq!(
                    serde_json::to_value(destination.project()).unwrap()["composition"]["name"],
                    "Unconsumed future"
                );
            }
        }
    }
}

#[test]
fn animator_review_explicit_key_noops_retain_exact_authored_source_and_redo() {
    for parameter in PARAMETERS {
        let mut editor = scene();
        for frame in [7, 43] {
            editor
                .execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit: TrackEdit::ToggleKey { frame },
                })
                .unwrap();
        }
        let mut raw = serde_json::to_value(editor.project()).unwrap();
        // Retain negative-zero source bits in an unrelated dormant animator
        // entry. Model equality alone would not detect a normalization here.
        raw["composition"]["layers"][0]["text_parameters"]["AnimatorPositionY"] =
            serde_json::json!({"value": -0.0, "keys": {}});
        if parameter == TextParam::AnimatorPositionY {
            raw["composition"]["layers"][0]["text_parameters"]["AnimatorPositionX"] =
                serde_json::json!({"value": -0.0, "keys": {}});
            raw["composition"]["layers"][0]["text_parameters"]["AnimatorPositionY"] = serde_json::json!({"value": 0.0, "keys": {
                "7": {"value": 0.0, "interpolation": "Linear"},
                "43": {"value": 0.0, "interpolation": "Linear"}
            }});
        }
        install_with_redo(&mut editor, serde_json::from_value(raw).unwrap());
        let original = serde_json::to_vec(editor.project()).unwrap();
        let property = PropertyPath::Text(parameter);
        let key = KeyRef {
            id: 1,
            property,
            frame: 7,
        };
        for command in [
            Command::MoveKeys {
                keys: vec![key],
                delta: 0,
            },
            Command::ScaleKeys {
                keys: vec![key],
                scale: KeyScale {
                    time_origin: -1.0e100,
                    time_scale: 1.,
                    value_origin: 1.0e100,
                    value_scale: 1.,
                },
            },
            Command::SetTemporalMode {
                id: 1,
                property,
                frame: 7,
                mode: TemporalMode::Independent,
            },
            Command::EditTrack {
                id: 1,
                property,
                edit: TrackEdit::Interpolate {
                    frame: 7,
                    interpolation: Interpolation::Linear,
                },
            },
        ] {
            editor
                .execute(Command::Batch(vec![Command::Batch(vec![]), command]))
                .unwrap();
            assert_eq!(serde_json::to_vec(editor.project()).unwrap(), original);
            assert!(editor.can_redo());
            assert!(!editor.can_undo());
        }
    }
}
