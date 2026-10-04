//! Independent model and storage coverage for sparse whole-paint opacity.
use super::*;

const OPACITY: [TextParam; 2] = [TextParam::FillOpacity, TextParam::StrokeOpacity];
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a4WQAAAAASUVORK5CYII=";

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Whole paint 한글\nOverlap".into(),
                font_size: 48.,
            },
            width: 500.,
            height: 200.,
            name: "Text opacity".into(),
        })
        .unwrap();
    editor
}
fn edit(editor: &mut Editor, parameter: TextParam, edit: TrackEdit) {
    editor
        .execute(Command::EditText {
            id: 1,
            parameter,
            edit,
        })
        .unwrap();
}
fn set(editor: &mut Editor, parameter: TextParam, value: f64, frame: Frame) {
    if let Some(command) = editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .text_value_command(parameter, value, frame)
        .unwrap()
    {
        editor.execute(command).unwrap();
    }
}
fn sample(editor: &Editor, parameter: TextParam, frame: Frame) -> f64 {
    editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .text_value_at(parameter, frame)
        .unwrap()
}
fn assert_rejected(editor: &mut Editor, command: Command) {
    let current = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    assert!(editor.execute(command).is_err());
    assert_eq!(editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
}
fn animated_scene() -> Editor {
    let mut editor = scene();
    for (parameter, value) in OPACITY.into_iter().zip([20., 60.]) {
        edit(
            &mut editor,
            parameter,
            TrackEdit::ToggleAnimation { frame: 10 },
        );
        set(&mut editor, parameter, value, 40);
        editor
            .execute(Command::SetTemporalHandle {
                id: 1,
                property: PropertyPath::Text(parameter),
                frame: 10,
                incoming: false,
                handle: TemporalHandle {
                    slope: -0.5,
                    influence: 0.3,
                },
            })
            .unwrap();
        editor
            .execute(Command::SetTemporalMode {
                id: 1,
                property: PropertyPath::Text(parameter),
                frame: 40,
                mode: TemporalMode::Auto,
            })
            .unwrap();
    }
    editor
}

#[test]
fn text_opacity_appends_stable_addresses_without_expanding_rgb_helpers() {
    use TextParam::*;
    assert_eq!(
        TextParam::ALL,
        [
            FillRed,
            FillGreen,
            FillBlue,
            StrokeRed,
            StrokeGreen,
            StrokeBlue,
            StrokeWidth,
            FontSize,
            Tracking,
            Leading,
            FillOpacity,
            StrokeOpacity
        ]
    );
    for (index, parameter) in TextParam::ALL.into_iter().enumerate() {
        assert_eq!(
            parameter.required_version(),
            if index < 7 {
                48
            } else if index < 10 {
                49
            } else {
                52
            }
        );
    }
    assert_eq!(TextPaint::Fill.channels(), [FillRed, FillGreen, FillBlue]);
    assert_eq!(
        TextPaint::Stroke.channels(),
        [StrokeRed, StrokeGreen, StrokeBlue]
    );
    for (paint, parameter) in [
        (TextPaint::Fill, FillOpacity),
        (TextPaint::Stroke, StrokeOpacity),
    ] {
        assert_eq!(paint.opacity(), parameter);
        assert_eq!(parameter.bounds(), (0., 100.));
        assert_eq!(TextPaint::from_parameter(parameter), None);
        assert_eq!(TextPaint::component_label(parameter), None);
        assert!(!parameter.is_typography());
        for (component, label) in paint.channels().into_iter().zip(["R", "G", "B"]) {
            assert_eq!(TextPaint::from_parameter(component), Some(paint));
            assert_eq!(TextPaint::component_label(component), Some(label));
        }
    }
}

#[test]
fn text_opacity_absent_and_empty_map_keep_old_source_style_and_native_bytes() {
    let editor = scene();
    let original = editor.project().clone();
    let json = original.to_json().unwrap();
    let native = project_file::encode(&original, None).unwrap();
    assert_eq!(original.version, 3);
    assert!(!json.contains("text_parameters"));
    assert!(!json.contains("text_style"));
    assert_eq!(
        serde_json::to_string(&TextStyle::default()).unwrap(),
        r#"{"font_family":"Wanted Sans","font_face":"","weight":400,"italic":false,"leading":1.2,"tracking":0.0,"align":"Left","paragraph":false,"fill_enabled":true,"stroke_enabled":false,"stroke_color":0,"stroke_width":1.0,"stroke_over_fill":false,"stroke_join":"Miter"}"#
    );
    for frame in [0, 60, u32::MAX] {
        let layer = editor.selected_layer().unwrap();
        for parameter in OPACITY {
            assert_eq!(layer.text_value_at(parameter, frame), Some(100.));
            assert_eq!(
                layer.track_value(PropertyPath::Text(parameter), frame),
                Some(100.)
            );
            assert_eq!(
                layer.track_label(PropertyPath::Text(parameter)).as_deref(),
                Some(parameter.label())
            );
            assert!(layer.track(PropertyPath::Text(parameter)).is_none());
            assert!(!layer.track_paths().contains(&PropertyPath::Text(parameter)));
        }
    }
    assert_eq!(editor.project(), &original);
    assert_eq!(Project::from_json(&json).unwrap().to_json().unwrap(), json);
    let loaded = project_file::decode(&native).unwrap();
    assert_eq!(loaded.project, original);
    assert_eq!(
        project_file::encode(&loaded.project, loaded.view).unwrap(),
        native
    );
    let mut empty = serde_json::to_value(&original).unwrap();
    empty["composition"]["layers"][0]["text_parameters"] = serde_json::json!({});
    let loaded = Project::from_json(&empty.to_string()).unwrap();
    assert_eq!(loaded, original);
    assert_eq!(loaded.to_json().unwrap(), json);
    assert_eq!(project_file::encode(&loaded, None).unwrap(), native);
}

#[test]
fn text_opacity_validated_default_value_routes_preserve_exact_history_redo_assets_and_schema() {
    for historical_assets in [false, true] {
        let mut editor = scene();
        if historical_assets {
            editor
                .execute(Command::AddContent {
                    content: Content::Image { png: PNG.into() },
                    width: 1.,
                    height: 1.,
                    name: "Legacy image".into(),
                })
                .unwrap();
        }
        let mut old = editor.project().clone();
        // Imported pre-asset-library sources must not migrate on a text no-op.
        old.version = if historical_assets { 7 } else { 47 };
        old.asset_library = AssetLibrary::default();
        for layer in &mut old.composition.layers {
            layer.asset = None;
        }
        editor.replace_project(old).unwrap();
        editor.select(1);
        editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Redo retained".into(),
            })
            .unwrap();
        editor.undo();
        let current = editor.current.clone();
        let undo = editor.undo.clone();
        let redo = editor.redo.clone();
        let json = editor.project().to_json().unwrap();
        let view = br#" { "version":1, "frame":20 } "#;
        let native = project_file::encode(editor.project(), Some(view)).unwrap();
        for parameter in OPACITY {
            assert!(
                editor
                    .selected_layer()
                    .unwrap()
                    .text_value_command(parameter, 100., 20)
                    .unwrap()
                    .is_none()
            );
            let direct = Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::Value {
                    frame: 20,
                    value: 100.,
                },
            };
            let generic = Command::EditTrack {
                id: 1,
                property: PropertyPath::Text(parameter),
                edit: TrackEdit::Value {
                    frame: 60,
                    value: 100.,
                },
            };
            for command in [
                direct.clone(),
                generic.clone(),
                Command::Batch(vec![direct, Command::Batch(vec![generic])]),
            ] {
                editor.execute(command).unwrap();
                assert_eq!(editor.current, current);
                assert_eq!(editor.undo, undo);
                assert_eq!(editor.redo, redo);
                assert!(editor.selected_layer().unwrap().text_parameters.is_empty());
                assert_eq!(editor.project().to_json().unwrap(), json);
                assert_eq!(
                    project_file::encode(editor.project(), Some(view)).unwrap(),
                    native
                );
            }
            assert_rejected(
                &mut editor,
                Command::EditText {
                    id: 1,
                    parameter,
                    edit: TrackEdit::Value {
                        frame: 150,
                        value: 100.,
                    },
                },
            );
            assert_rejected(
                &mut editor,
                Command::Batch(vec![Command::EditTrack {
                    id: 1,
                    property: PropertyPath::Text(parameter),
                    edit: TrackEdit::Value {
                        frame: u32::MAX,
                        value: 100.,
                    },
                }]),
            );
        }
        editor.redo();
        assert_eq!(editor.selected_layer().unwrap().name(), "Redo retained");
    }
}

#[test]
fn text_opacity_changed_static_values_are_sparse_keyless_and_explicit_default_is_intentional() {
    for parameter in OPACITY {
        let mut editor = scene();
        editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                value: 43.,
            })
            .unwrap();
        let base = editor.selected_layer().unwrap().clone();
        let current = editor.current.clone();
        let undo = editor.undo.len();
        let value = 37.12345678912345;
        let command = base
            .text_value_command(parameter, value, 20)
            .unwrap()
            .unwrap();
        assert!(
            matches!(command, Command::EditText { parameter: p, edit: TrackEdit::Value { frame: 20, value: v }, .. } if p == parameter && v == value)
        );
        editor.execute(command).unwrap();
        let layer = editor.selected_layer().unwrap();
        assert_eq!(layer.text_parameters.len(), 1);
        assert_eq!(
            layer.text_parameters[&parameter],
            AnimatedProperty::new(value)
        );
        assert_eq!(layer.content, base.content);
        assert_eq!(layer.text_style, base.text_style);
        assert_eq!(layer.properties, base.properties);
        assert_eq!(sample(&editor, parameter, 120), value);
        assert_eq!(editor.project().version, 52);
        assert_eq!(editor.undo.len(), undo + 1);
        editor.undo();
        assert_eq!(editor.current, current);
        editor.redo();
        for value in [0., 100.] {
            set(&mut editor, parameter, value, 20);
            assert_eq!(
                editor.selected_layer().unwrap().text_parameters[&parameter],
                AnimatedProperty::new(value)
            );
            assert_eq!(editor.project().version, 52);
        }
        let mut untouched = scene();
        edit(
            &mut untouched,
            parameter,
            TrackEdit::ToggleKey { frame: 20 },
        );
        assert_eq!(untouched.project().version, 52);
        assert_eq!(
            untouched.selected_layer().unwrap().text_parameters[&parameter].keys[&20].value,
            100.
        );
        edit(
            &mut untouched,
            parameter,
            TrackEdit::ToggleKey { frame: 20 },
        );
        assert_eq!(
            untouched.selected_layer().unwrap().text_parameters[&parameter],
            AnimatedProperty::new(100.)
        );
        assert_eq!(untouched.project().version, 52);
    }
}

#[test]
fn text_opacity_rgb_fill_stroke_and_layer_animation_are_independent() {
    let mut editor = scene();
    editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 50.,
        })
        .unwrap();
    let transform = editor.selected_layer().unwrap().properties.clone();
    for (parameter, value) in OPACITY.into_iter().zip([20., 60.]) {
        edit(
            &mut editor,
            parameter,
            TrackEdit::ToggleAnimation { frame: 0 },
        );
        set(&mut editor, parameter, value, 60);
    }
    assert_eq!(sample(&editor, TextParam::FillOpacity, 30), 60.);
    assert_eq!(sample(&editor, TextParam::StrokeOpacity, 30), 80.);
    let alpha = editor.selected_layer().unwrap().text_parameters.clone();
    for paint in [TextPaint::Fill, TextPaint::Stroke] {
        assert!(!editor.selected_layer().unwrap().text_color_animated(paint));
        for frame in [0, 30] {
            let command = editor
                .selected_layer()
                .unwrap()
                .text_color_animation_command(paint, frame)
                .unwrap();
            editor.execute(command).unwrap();
            for parameter in OPACITY {
                assert_eq!(
                    editor.selected_layer().unwrap().text_parameters[&parameter],
                    alpha[&parameter]
                );
            }
        }
        assert!(!editor.selected_layer().unwrap().text_color_animated(paint));
    }
    let before = editor.current.clone();
    edit(
        &mut editor,
        TextParam::FillOpacity,
        TrackEdit::ToggleAnimation { frame: 30 },
    );
    assert_eq!(
        editor.selected_layer().unwrap().text_parameters[&TextParam::FillOpacity],
        AnimatedProperty::new(60.)
    );
    assert_eq!(
        editor.selected_layer().unwrap().text_parameters[&TextParam::StrokeOpacity],
        alpha[&TextParam::StrokeOpacity]
    );
    assert_eq!(editor.selected_layer().unwrap().properties, transform);
    editor.undo();
    assert_eq!(editor.current, before);
}

#[test]
fn text_opacity_combined_rgb_alpha_batch_preserves_style_and_is_one_undo() {
    for paint in [TextPaint::Fill, TextPaint::Stroke] {
        let mut editor = scene();
        let before = editor.current.clone();
        let undo = editor.undo.len();
        let layer = editor.selected_layer().unwrap();
        let commands = vec![
            layer.text_color_command(paint, 0x123456, 20).unwrap(),
            layer
                .text_value_command(paint.opacity(), 37.5, 20)
                .unwrap()
                .unwrap(),
        ];
        editor.execute(Command::Batch(commands)).unwrap();
        let layer = editor.selected_layer().unwrap();
        assert_eq!(layer.text_color_at(paint, 20), Some(0x123456));
        assert_eq!(layer.text_value_at(paint.opacity(), 20), Some(37.5));
        assert_eq!(layer.text_parameters.len(), 1);
        assert_eq!(editor.undo.len(), undo + 1);
        let after = editor.current.clone();
        editor.undo();
        assert_eq!(editor.current, before);
        editor.redo();
        assert_eq!(editor.current, after);
    }
}

#[test]
fn text_opacity_key_edits_keep_temporal_metadata_and_new_keys_are_linear() {
    let mut editor = animated_scene();
    for parameter in OPACITY {
        let prior = editor.selected_layer().unwrap().text_parameters[&parameter].keys[&10].clone();
        set(&mut editor, parameter, 75., 10);
        let key = &editor.selected_layer().unwrap().text_parameters[&parameter].keys[&10];
        assert_eq!(key.interpolation, prior.interpolation);
        assert_eq!(key.temporal, prior.temporal);
        set(&mut editor, parameter, 55.25, 25);
        let key = &editor.selected_layer().unwrap().text_parameters[&parameter].keys[&25];
        assert_eq!(key.value, 55.25);
        assert_eq!(key.interpolation, Interpolation::Linear);
        assert_eq!(key.temporal, TemporalHandles::default());
    }
}

#[test]
fn text_opacity_linear_hold_smooth_and_bezier_samples_clamp_before_disable_or_insert() {
    for parameter in OPACITY {
        let mut editor = scene();
        set(&mut editor, parameter, 25., 0);
        edit(
            &mut editor,
            parameter,
            TrackEdit::ToggleAnimation { frame: 0 },
        );
        set(&mut editor, parameter, 75., 40);
        assert_eq!(sample(&editor, parameter, 10), 37.5);
        edit(
            &mut editor,
            parameter,
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Hold,
            },
        );
        assert_eq!(sample(&editor, parameter, 39), 25.);
        edit(
            &mut editor,
            parameter,
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Smooth,
            },
        );
        assert_eq!(sample(&editor, parameter, 10), 32.8125);
        for (direction, expected) in [(-2., 0.), (3., 100.)] {
            edit(
                &mut editor,
                parameter,
                TrackEdit::Interpolate {
                    frame: 0,
                    interpolation: Interpolation::Bezier(Bezier {
                        x1: 1. / 3.,
                        y1: direction,
                        x2: 2. / 3.,
                        y2: direction,
                    }),
                },
            );
            let raw = editor.selected_layer().unwrap().text_parameters[&parameter].value_at(20);
            assert!(raw < 0. || raw > 100.);
            assert_eq!(sample(&editor, parameter, 20), expected);
            assert!(
                editor
                    .selected_layer()
                    .unwrap()
                    .text_value_command(parameter, expected, 20)
                    .unwrap()
                    .is_none()
            );
            let before = editor.current.clone();
            for operation in [
                TrackEdit::ToggleKey { frame: 20 },
                TrackEdit::ToggleAnimation { frame: 20 },
            ] {
                edit(&mut editor, parameter, operation);
                let track = &editor.selected_layer().unwrap().text_parameters[&parameter];
                assert_eq!(track.value_at(20), expected);
                assert_eq!(sample(&editor, parameter, 20), expected);
                assert_eq!(editor.project().version, 52);
                editor.undo();
                assert_eq!(editor.current, before);
            }
        }
    }
}

#[test]
fn text_opacity_invalid_values_frames_types_locks_and_mixed_batches_reject_atomically() {
    let mut editor = scene();
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo retained".into(),
        })
        .unwrap();
    editor.undo();
    for parameter in OPACITY {
        for value in [-1., 100.000001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                editor
                    .selected_layer()
                    .unwrap()
                    .text_value_command(parameter, value, 0)
                    .is_err()
            );
            assert_rejected(
                &mut editor,
                Command::Batch(vec![
                    Command::EditText {
                        id: 1,
                        parameter: TextParam::FillRed,
                        edit: TrackEdit::Value {
                            frame: 0,
                            value: 10.,
                        },
                    },
                    Command::EditTrack {
                        id: 1,
                        property: PropertyPath::Text(parameter),
                        edit: TrackEdit::Value { frame: 0, value },
                    },
                ]),
            );
        }
        for edit in [
            TrackEdit::Value {
                frame: 150,
                value: 100.,
            },
            TrackEdit::ToggleKey { frame: u32::MAX },
            TrackEdit::ToggleAnimation { frame: 150 },
            TrackEdit::Keyframe {
                from: 0,
                to: 150,
                value: 50.,
            },
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Hold,
            },
        ] {
            assert_rejected(
                &mut editor,
                Command::EditText {
                    id: 1,
                    parameter,
                    edit,
                },
            );
        }
    }
    editor.execute(Command::ToggleLocked(1)).unwrap();
    for parameter in OPACITY {
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .text_value_command(parameter, 100., 0)
                .is_err()
        );
        for edit in [
            TrackEdit::Value {
                frame: 0,
                value: 100.,
            },
            TrackEdit::ToggleKey { frame: 0 },
        ] {
            assert_rejected(
                &mut editor,
                Command::EditText {
                    id: 1,
                    parameter,
                    edit,
                },
            );
        }
    }
    editor.execute(Command::AddRectangle).unwrap();
    for parameter in OPACITY {
        assert_eq!(
            editor.selected_layer().unwrap().text_value_at(parameter, 0),
            None
        );
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .text_value_command(parameter, 100., 0)
                .is_err()
        );
        assert_rejected(
            &mut editor,
            Command::EditTrack {
                id: 2,
                property: PropertyPath::Text(parameter),
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 100.,
                },
            },
        );
    }
}

#[test]
fn text_opacity_presence_requires_schema52_when_keyless_default_disabled_and_inactive() {
    for parameter in OPACITY {
        for keyless in [false, true] {
            let mut editor = scene();
            let mut style = editor.selected_layer().unwrap().text_style();
            style.fill_enabled = false;
            style.stroke_enabled = false;
            style.stroke_width = 0.;
            editor
                .execute(Command::SetTextStyle { id: 1, style })
                .unwrap();
            edit(
                &mut editor,
                parameter,
                TrackEdit::ToggleAnimation { frame: 20 },
            );
            if keyless {
                edit(
                    &mut editor,
                    parameter,
                    TrackEdit::ToggleAnimation { frame: 20 },
                );
            }
            assert_eq!(editor.project().version, 52);
            editor.execute(Command::NewComposition).unwrap();
            assert_eq!(editor.project().version, 52);
            let original = editor.project().clone();
            assert_eq!(
                Project::from_json(&original.to_json().unwrap()).unwrap(),
                original
            );
            let bytes = project_file::encode(&original, None).unwrap();
            assert_eq!(project_file::decode(&bytes).unwrap().project, original);
            let wire = serde_json::to_value(&original).unwrap();
            for version in [3, 47, 48, 49, 50, 51, PROJECT_VERSION + 1] {
                let mut bad = wire.clone();
                bad["version"] = version.into();
                assert!(
                    Project::from_json(&bad.to_string()).is_err(),
                    "accepted schema {version}"
                );
                assert!(document::decode_native(bad.clone(), BTreeMap::new()).is_err());
                let candidate: Project = serde_json::from_value(bad).unwrap();
                assert!(project_file::encode(&candidate, None).is_err());
                let current = editor.current.clone();
                let undo = editor.undo.clone();
                let redo = editor.redo.clone();
                assert!(editor.replace_project(candidate).is_err());
                assert_eq!(editor.current, current);
                assert_eq!(editor.undo, undo);
                assert_eq!(editor.redo, redo);
            }
        }
    }
}

#[test]
fn text_opacity_malformed_inactive_source_keys_and_temporal_handles_reject() {
    let mut editor = animated_scene();
    editor.execute(Command::NewComposition).unwrap();
    let original = editor.project().clone();
    for parameter in OPACITY {
        let name = serde_json::to_value(parameter)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        for defect in [
            "base_low",
            "base_high",
            "base_null",
            "key",
            "time",
            "interpolation",
            "temporal",
            "type",
            "unknown",
        ] {
            let mut bad = serde_json::to_value(&original).unwrap();
            let layer = &mut bad["other_compositions"]["1"]["layers"][0];
            let track = &mut layer["text_parameters"][&name];
            match defect {
                "base_low" => track["value"] = (-1.).into(),
                "base_high" => track["value"] = 101.into(),
                "base_null" => track["value"] = serde_json::Value::Null,
                "key" => track["keys"]["10"]["value"] = 101.into(),
                "time" => {
                    track["keys"]["150"] = track["keys"]["10"].clone();
                }
                "interpolation" => {
                    track["keys"]["10"]["interpolation"] =
                        serde_json::json!({"Bezier":{"x1":-1.,"y1":0.,"x2":1.,"y2":1.}})
                }
                "temporal" => track["keys"]["10"]["temporal"]["outgoing"]["influence"] = 2.into(),
                "type" => {
                    layer["content"] = "Rectangle".into();
                }
                _ => {
                    layer["text_parameters"]["Opacity"] =
                        serde_json::json!({"value":100.,"keys":{}});
                }
            }
            assert!(
                Project::from_json(&bad.to_string()).is_err(),
                "accepted {parameter:?} {defect}"
            );
            assert!(document::decode_native(bad, BTreeMap::new()).is_err());
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut bad = original.clone();
            bad.other_compositions.get_mut(&1).unwrap().layers[0]
                .text_parameters
                .get_mut(&parameter)
                .unwrap()
                .value = value;
            assert!(bad.validate().is_err());
        }
    }
}

#[test]
fn text_opacity_generic_copy_move_scale_delete_and_incompatible_replacement() {
    let mut e = animated_scene();
    let original = e.selected_layer().unwrap().text_parameters.clone();
    let refs = |frame| {
        OPACITY
            .map(|parameter| KeyRef {
                id: 1,
                property: PropertyPath::Text(parameter),
                frame,
            })
            .to_vec()
    };
    let copies: Vec<_> = OPACITY
        .into_iter()
        .map(|p| {
            e.selected_layer()
                .unwrap()
                .copy_key(PropertyPath::Text(p), 10)
                .unwrap()
        })
        .collect();
    e.execute(Command::PasteKeys {
        keys: copies.clone(),
        frame: 60,
        target: None,
    })
    .unwrap();
    e.execute(Command::MoveKeys {
        keys: refs(60),
        delta: 10,
    })
    .unwrap();
    e.execute(Command::ScaleKeys {
        keys: refs(70),
        scale: KeyScale {
            time_origin: 0.,
            time_scale: 1.5,
            value_origin: 0.,
            value_scale: 0.5,
        },
    })
    .unwrap();
    for parameter in OPACITY {
        let mut expected = original[&parameter].keys[&10].clone();
        expected.value *= 0.5;
        expected.temporal.outgoing.as_mut().unwrap().slope *= 0.5 / 1.5;
        assert_eq!(
            e.selected_layer().unwrap().text_parameters[&parameter].keys[&105],
            expected
        );
    }
    assert_rejected(
        &mut e,
        Command::ScaleKeys {
            keys: refs(105),
            scale: KeyScale {
                time_origin: 0.,
                time_scale: 1.,
                value_origin: 0.,
                value_scale: 100000.,
            },
        },
    );
    e.execute(Command::DeleteKeys(refs(105))).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, original);
    e.execute(Command::DeleteKeys(refs(10))).unwrap();
    e.execute(Command::DeleteKeys(refs(40))).unwrap();
    for parameter in OPACITY {
        assert!(
            e.selected_layer().unwrap().text_parameters[&parameter]
                .keys
                .is_empty()
        );
        assert_eq!(
            sample(&e, parameter, 90),
            original[&parameter].keys[&40].value
        );
    }
    assert_eq!(e.project().version, 52);
    assert_rejected(
        &mut e,
        Command::SetContent {
            id: 1,
            content: Content::Rectangle,
        },
    );
    e.execute(Command::AddRectangle).unwrap();
    assert_rejected(
        &mut e,
        Command::PasteKeys {
            keys: copies,
            frame: 20,
            target: Some(2),
        },
    );
}

#[test]
fn text_opacity_duplicate_split_shift_clipboard_fps_and_precompose_preserve_tracks() {
    let mut e = animated_scene();
    let tracks = e.selected_layer().unwrap().text_parameters.clone();
    e.execute(Command::DuplicateLayer(1)).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, tracks);
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 25,
    })
    .unwrap();
    let split = e.selected_layer().unwrap().id();
    assert_eq!(e.selected_layer().unwrap().text_parameters, tracks);
    assert_eq!(
        e.project().composition().layer(1).unwrap().text_parameters,
        tracks
    );
    e.execute(Command::SetLayerRange {
        id: split,
        start: 25,
        end: 100,
    })
    .unwrap();
    e.execute(Command::ShiftLayer {
        id: split,
        delta: 5,
    })
    .unwrap();
    for parameter in OPACITY {
        let t = &e.selected_layer().unwrap().text_parameters[&parameter];
        assert_eq!(t.keys[&15], tracks[&parameter].keys[&10]);
        assert_eq!(t.keys[&45], tracks[&parameter].keys[&40]);
    }
    let clipboard = e.copy_layers(&[split]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "60fps".into(),
        width: 1920,
        height: 1080,
        fps: 60,
        duration: 300,
    })
    .unwrap();
    e.execute(Command::PasteLayers(clipboard)).unwrap();
    let pasted = e.selected_layer().unwrap().id();
    let pasted_tracks = e.selected_layer().unwrap().text_parameters.clone();
    for parameter in OPACITY {
        let mut expected = tracks[&parameter].keys[&10].clone();
        expected.temporal.outgoing.as_mut().unwrap().slope *= 0.5;
        assert_eq!(pasted_tracks[&parameter].keys[&30], expected);
        assert_eq!(
            pasted_tracks[&parameter].keys[&90],
            tracks[&parameter].keys[&40]
        );
    }
    e.execute(Command::DuplicateComposition).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, pasted_tracks);
    e.activate_composition(2).unwrap();
    e.execute(Command::Precompose {
        layers: vec![pasted],
        name: "Opacity precomp".into(),
    })
    .unwrap();
    let Content::Composition { composition, .. } = e.selected_layer().unwrap().content() else {
        panic!("precomp")
    };
    assert_eq!(
        e.project()
            .composition_by_id(*composition)
            .unwrap()
            .layer(pasted)
            .unwrap()
            .text_parameters,
        pasted_tracks
    );
    assert_eq!(e.project().version, 52);
    let bytes = project_file::encode(e.project(), None).unwrap();
    assert_eq!(project_file::decode(&bytes).unwrap().project, *e.project());
}

#[test]
fn text_opacity_source_font_style_box_and_disabled_paints_preserve_all_tracks() {
    let mut editor = animated_scene();
    let tracks = editor.selected_layer().unwrap().text_parameters.clone();
    let mut style = editor.selected_layer().unwrap().text_style();
    style.font_family = "Missing Font".into();
    style.font_face = "Missing-Bold".into();
    style.weight = 700;
    style.italic = true;
    style.tracking = -100.;
    style.leading = 0.75;
    style.align = TextAlign::Right;
    style.paragraph = true;
    style.fill_enabled = false;
    style.stroke_enabled = false;
    style.stroke_width = 0.;
    style.stroke_over_fill = true;
    style.stroke_join = TextStrokeJoin::Round;
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: style.clone(),
        })
        .unwrap();
    editor
        .execute(Command::SetContent {
            id: 1,
            content: Content::Text {
                text: "Changed source\n字".into(),
                font_size: 72.,
            },
        })
        .unwrap();
    editor
        .execute(Command::SetTextBox {
            id: 1,
            width: 600.,
            height: 300.,
        })
        .unwrap();
    editor
        .execute(Command::ReplaceTextFont {
            from: TextFont::of(&style),
            to: TextFont::of(&TextStyle::default()),
        })
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().text_parameters, tracks);
    assert_eq!(sample(&editor, TextParam::FillOpacity, 40), 20.);
    assert_eq!(sample(&editor, TextParam::StrokeOpacity, 40), 60.);
    let mut enabled = editor.selected_layer().unwrap().text_style();
    enabled.fill_enabled = true;
    enabled.stroke_enabled = true;
    enabled.stroke_width = 12.;
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: enabled,
        })
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().text_parameters, tracks);
    for content in [
        Content::Rectangle,
        Content::Null,
        Content::Shape(Shape::default()),
    ] {
        assert_rejected(&mut editor, Command::SetContent { id: 1, content });
    }
    assert_eq!(
        Project::from_json(&editor.project().to_json().unwrap()).unwrap(),
        *editor.project()
    );
}

#[test]
fn text_opacity_generic_paste_into_sparse_text_preserves_temporal_data_and_sibling_absence() {
    for parameter in OPACITY {
        let mut editor = animated_scene();
        let copied = editor
            .selected_layer()
            .unwrap()
            .copy_key(PropertyPath::Text(parameter), 10)
            .unwrap();
        let key = editor.selected_layer().unwrap().text_parameters[&parameter].keys[&10].clone();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Paste target".into(),
                    font_size: 24.,
                },
                width: 200.,
                height: 100.,
                name: "Target".into(),
            })
            .unwrap();
        editor
            .execute(Command::PasteKeys {
                keys: vec![copied],
                frame: 20,
                target: Some(2),
            })
            .unwrap();
        let layer = editor.selected_layer().unwrap();
        assert_eq!(layer.text_parameters.len(), 1);
        assert_eq!(layer.text_parameters[&parameter].keys[&20], key);
        let sibling = OPACITY.into_iter().find(|p| *p != parameter).unwrap();
        assert!(layer.track(PropertyPath::Text(sibling)).is_none());
        assert_eq!(layer.text_value_at(sibling, 20), Some(100.));
    }
}

#[test]
fn text_opacity_ten_thousand_key_limit_rejects_before_history_commit() {
    for parameter in OPACITY {
        let mut project = scene().project().clone();
        project.version = 52;
        project.composition.duration = 20_000;
        let mut track = AnimatedProperty::new(100.);
        track.keys = (0..10_000)
            .map(|frame| {
                (
                    frame,
                    Keyframe {
                        value: 50.,
                        interpolation: Interpolation::Linear,
                        temporal: TemporalHandles::default(),
                    },
                )
            })
            .collect();
        project.composition.layers[0]
            .text_parameters
            .insert(parameter, track);
        project.validate().unwrap();
        let mut editor = Editor::default();
        editor.replace_project(project).unwrap();
        assert_rejected(
            &mut editor,
            Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleKey { frame: 10_000 },
            },
        );
        set(&mut editor, parameter, 20., 50);
        assert_eq!(
            editor.selected_layer().unwrap().text_parameters[&parameter]
                .keys
                .len(),
            10_000
        );
    }
}

#[test]
fn text_opacity_sparse_entries_count_toward_metadata_budget_atomically() {
    const BUDGET: usize = 16 * 1024 * 1024;
    let mut project = scene().project().clone();
    let mut template = project.composition.layers[0].clone();
    template.content = Content::Text {
        text: "\u{0001}".repeat(16_384),
        font_size: 48.,
    };
    project.composition.layers = (1..=180)
        .map(|id| {
            let mut layer = template.clone();
            layer.id = id;
            layer
        })
        .collect();
    project.next_layer_id = 181;
    let size = |p: &Project| serde_json::to_vec(p).unwrap().len();
    let mut remaining = (size(&project) - BUDGET).div_ceil(5);
    for layer in &mut project.composition.layers {
        let Content::Text { text, .. } = &mut layer.content else {
            unreachable!();
        };
        let count = remaining.min(text.len());
        *text = "x".repeat(count) + &"\u{0001}".repeat(text.len() - count);
        remaining -= count;
    }
    assert_eq!(remaining, 0);
    let padding = BUDGET - size(&project);
    project.composition.name.push_str(&"x".repeat(padding));
    assert_eq!(size(&project), BUDGET);
    let mut editor = Editor::default();
    editor.replace_project(project).unwrap();
    for parameter in OPACITY {
        let current = editor.current.clone();
        let undo = editor.undo.clone();
        let redo = editor.redo.clone();
        let error = editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleKey { frame: 0 },
            })
            .unwrap_err();
        assert!(error.contains("metadata exceeds 16 MiB"), "{error}");
        assert_eq!(editor.current, current);
        assert_eq!(editor.undo, undo);
        assert_eq!(editor.redo, redo);
    }
}

// Read the container independently of the production decoder. Every chunk remains
// v1; VIEW is opaque to core, and its bytes must survive without normalization.
fn chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    assert_eq!(&bytes[..8], b"\x89LEP\r\n\x1a\n");
    assert_eq!(&bytes[8..16], &[1, 0, 32, 0, 0, 0, 0, 0]);
    let mut cursor = 32;
    let mut result = Vec::new();
    while cursor < bytes.len() {
        assert_eq!(&bytes[cursor + 4..cursor + 8], &[1, 0, 0, 0]);
        let length =
            u64::from_le_bytes(bytes[cursor + 8..cursor + 16].try_into().unwrap()) as usize;
        result.push((
            bytes[cursor..cursor + 4].try_into().unwrap(),
            bytes[cursor + 20..cursor + 20 + length].to_vec(),
        ));
        cursor += 20 + length;
    }
    assert_eq!(cursor, bytes.len());
    result
}
fn pack(parts: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut bytes = b"\x89LEP\r\n\x1a\n\x01\x00\x20\x00".to_vec();
    bytes.resize(32, 0);
    for (tag, payload) in parts {
        let start = bytes.len();
        bytes.extend_from_slice(tag);
        bytes.extend_from_slice(&[1, 0, 0, 0]);
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        let mut crc = crc32fast::Hasher::new();
        crc.update(&bytes[start..]);
        crc.update(payload);
        bytes.extend_from_slice(&crc.finalize().to_le_bytes());
        bytes.extend_from_slice(payload);
    }
    let length = bytes.len() as u64;
    bytes[16..24].copy_from_slice(&length.to_le_bytes());
    bytes[24..28].copy_from_slice(&(parts.len() as u32).to_le_bytes());
    let crc = crc32fast::hash(&bytes[..28]);
    bytes[28..32].copy_from_slice(&crc.to_le_bytes());
    bytes
}

#[test]
fn text_opacity_lep1_view1_view2_and_png_or_legacy_image_storage_roundtrip_exactly() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    for view in [br#" { "version":1, "frame":30, "graph_open":true } "#.as_slice(),
        br#" { "version":2, "opaque":{"version":1,"layer":1,"property":{"Text":"FillOpacity"}}, "future_view_field":true } "#.as_slice()] {
        let mut editor = scene();
        for (name, png) in [("PNG", PNG), ("Encoded fallback", "YWJj")] {
            editor.execute(Command::AddContent { content: Content::Image { png: png.into() }, width: 1., height: 1., name: name.into() }).unwrap();
        }
        let before = project_file::encode(editor.project(), Some(view)).unwrap();
        let original_images: Vec<_> = chunks(&before).into_iter().filter(|(tag, _)| tag == b"IMAG").collect();
        assert_eq!(original_images.len(), 2);
        let mut image_data: Vec<_> = original_images.iter().map(|(_, payload)| {
            let id_length = u16::from_le_bytes(payload[..2].try_into().unwrap()) as usize;
            (payload[2], payload[4 + id_length..].to_vec())
        }).collect();
        image_data.sort();
        assert_eq!(image_data, vec![(0, STANDARD.decode(PNG).unwrap()), (1, b"YWJj".to_vec())]);
        set(&mut editor, TextParam::FillOpacity, 37.12345678912345, 0);
        edit(&mut editor, TextParam::StrokeOpacity, TrackEdit::ToggleKey { frame: 0 });
        edit(&mut editor, TextParam::StrokeOpacity, TrackEdit::ToggleKey { frame: 0 });
        editor.execute(Command::NewComposition).unwrap();
        let project = editor.project().clone();
        let json = project.to_json().unwrap();
        let legacy = Project::from_json(&json).unwrap();
        assert_eq!(legacy, project);
        assert_eq!(legacy.to_json().unwrap(), json);
        let encoded = project_file::encode(&project, Some(view)).unwrap();
        let sections = chunks(&encoded);
        assert_eq!(sections.iter().map(|(tag, _)| *tag).collect::<Vec<_>>(), [*b"PROJ", *b"VIEW", *b"IMAG", *b"IMAG"]);
        assert_eq!(sections[1].1, view);
        assert_eq!(sections.iter().filter(|(tag, _)| tag == b"IMAG").cloned().collect::<Vec<_>>(), original_images);
        let metadata: serde_json::Value = serde_json::from_slice(&sections[0].1).unwrap();
        assert_eq!(metadata["version"], 52);
        let alpha = &metadata["other_compositions"]["1"]["layers"].as_array().unwrap().iter().find(|layer| layer["id"] == 1).unwrap()["text_parameters"];
        assert_eq!(alpha["FillOpacity"]["value"], 37.12345678912345);
        assert_eq!(alpha["StrokeOpacity"], serde_json::json!({"value":100.,"keys":{}}));
        assert!(!std::str::from_utf8(&sections[0].1).unwrap().contains("image_assets"));
        let loaded = project_file::decode(&encoded).unwrap();
        assert_eq!(loaded.project, project);
        assert_eq!(loaded.view, Some(view));
        assert_eq!(project_file::encode(&loaded.project, loaded.view).unwrap(), encoded);
        assert_eq!(pack(&sections), encoded);
        for version in [51, PROJECT_VERSION + 1] {
            let mut bad = sections.clone();
            let mut metadata: serde_json::Value = serde_json::from_slice(&bad[0].1).unwrap();
            metadata["version"] = version.into();
            bad[0].1 = serde_json::to_vec(&metadata).unwrap();
            assert!(project_file::decode(&pack(&bad)).is_err());
        }
    }
}
