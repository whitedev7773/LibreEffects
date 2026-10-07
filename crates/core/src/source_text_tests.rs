use super::*;

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Base".into(),
                font_size: 40.0,
            },
            width: 300.0,
            height: 100.0,
            name: "Source Text".into(),
        })
        .unwrap();
    editor
}
fn edit(editor: &mut Editor, frame: Frame, text: &str) {
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame,
            text: text.into(),
        })
        .unwrap();
}
fn track_edit(editor: &mut Editor, edit: TrackEdit) {
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit,
        })
        .unwrap();
}
fn animated() -> Editor {
    let mut editor = scene();
    track_edit(&mut editor, TrackEdit::ToggleAnimation { frame: 10 });
    edit(&mut editor, 20, "Second 👋\r\nline");
    edit(&mut editor, 40, "");
    editor
}
fn key(frame: Frame) -> KeyRef {
    KeyRef {
        id: 1,
        property: PropertyPath::SourceText,
        frame,
    }
}
#[test]
fn source_text_hold_sampling_is_discrete_and_keeps_the_baseline() {
    let editor = animated();
    let layer = editor.selected_layer().unwrap();
    for (frame, text) in [
        (0, "Base"),
        (10, "Base"),
        (19, "Base"),
        (20, "Second 👋\r\nline"),
        (39, "Second 👋\r\nline"),
        (40, ""),
        (149, ""),
    ] {
        assert_eq!(layer.source_text_at(frame), Some(text));
    }
    assert_eq!(
        layer.content(),
        &Content::Text {
            text: "Base".into(),
            font_size: 40.0
        }
    );
    assert_eq!(layer.track_value(PropertyPath::SourceText, 20), None);
    assert!(layer.source_text_animation().animated());
    assert_eq!(editor.project().version, 53);
}

fn assert_rejected(editor: &mut Editor, command: Command) {
    let before = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    assert!(editor.execute(command).is_err());
    assert_eq!(editor.current, before);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
}
fn animation_json(editor: &Editor) -> serde_json::Value {
    serde_json::to_value(
        editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .source_text_animation(),
    )
    .unwrap()
}
fn source_command(frame: Frame, text: &str) -> Command {
    Command::EditSourceText {
        id: 1,
        frame,
        text: text.into(),
    }
}
fn generic(edit: TrackEdit) -> Command {
    Command::EditTrack {
        id: 1,
        property: PropertyPath::SourceText,
        edit,
    }
}
#[test]
fn source_text_default_is_sparse_and_preserves_legacy_json_and_lep_bytes() {
    let editor = scene();
    let original = editor.project().clone();
    assert_eq!(original.version, 3);
    let json = original.to_json().unwrap();
    let bytes = project_file::encode(&original, Some(br#"{"version":2,"frame":12}"#)).unwrap();
    assert!(!json.contains("source_text_animation"));
    for frame in [0, 40, u32::MAX] {
        let layer = editor.selected_layer().unwrap();
        assert_eq!(layer.source_text_at(frame), Some("Base"));
        assert!(layer.source_text_animation().is_default());
        assert!(
            layer
                .track(PropertyPath::SourceText)
                .unwrap()
                .keys()
                .is_empty()
        );
        assert_eq!(
            layer.track_label(PropertyPath::SourceText).as_deref(),
            Some("Source Text")
        );
    }
    let mut explicit = serde_json::to_value(&original).unwrap();
    explicit["composition"]["layers"][0]["source_text_animation"] =
        serde_json::to_value(SourceTextAnimation::default()).unwrap();
    let loaded = Project::from_json(&explicit.to_string()).unwrap();
    assert_eq!(loaded, original);
    assert_eq!(loaded.to_json().unwrap(), json);
    let decoded = project_file::decode(&bytes).unwrap();
    assert_eq!(
        project_file::encode(&decoded.project, decoded.view).unwrap(),
        bytes
    );
}
#[test]
fn source_text_static_edits_do_not_enable_animation_or_change_typography() {
    let mut editor = scene();
    let before = editor.project().clone();
    edit(&mut editor, 75, "👩‍💻 e\u{301}\r\n終わり");
    let after = editor.project().clone();
    let layer = editor.selected_layer().unwrap();
    assert!(layer.source_text_animation().is_default());
    assert_eq!(after.version, 3);
    assert_eq!(layer.text_typography_at(75).unwrap().font_size, 40.0);
    assert_eq!(layer.source_text_at(0), Some("👩‍💻 e\u{301}\r\n終わり"));
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
    edit(&mut editor, 0, "");
    assert_eq!(
        editor.selected_layer().unwrap().source_text_at(90),
        Some("")
    );
}
#[test]
fn source_text_noops_preserve_exact_assets_history_redo_and_legacy_schema() {
    for historical_image in [false, true] {
        let mut editor = scene();
        if historical_image {
            editor.execute(Command::AddContent {
                content: Content::Image { png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a4WQAAAAASUVORK5CYII=".into() },
                width: 1.0, height: 1.0, name: "Legacy image".into(),
            }).unwrap();
        }
        let mut project = editor.project().clone();
        project.version = if historical_image { 7 } else { 47 };
        project.asset_library = AssetLibrary::default();
        for layer in &mut project.composition.layers {
            layer.asset = None;
        }
        editor.replace_project(project).unwrap();
        editor.select(1);
        editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Retain redo".into(),
            })
            .unwrap();
        editor.undo();
        let before = editor.current.clone();
        let undo = editor.undo.clone();
        let redo = editor.redo.clone();
        let bytes = project_file::encode(editor.project(), None).unwrap();
        for command in [
            source_command(20, "Base"),
            Command::Batch(vec![
                source_command(40, "Base"),
                Command::Batch(vec![source_command(70, "Base")]),
            ]),
        ] {
            editor.execute(command).unwrap();
            assert_eq!(editor.current, before);
            assert_eq!(editor.undo, undo);
            assert_eq!(editor.redo, redo);
            assert_eq!(project_file::encode(editor.project(), None).unwrap(), bytes);
        }
        assert_rejected(&mut editor, source_command(150, "Base"));
        assert_rejected(&mut editor, source_command(0, &"é".repeat(8193)));
    }
}
#[test]
fn source_text_animated_equivalent_edit_is_noop_but_explicit_key_is_retained() {
    let mut editor = animated();
    edit(&mut editor, 50, "redo");
    editor.undo();
    let before = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    let pool = animation_json(&editor);
    for frame in [20, 21, 39] {
        edit(&mut editor, frame, "Second 👋\r\nline");
    }
    assert_eq!(editor.current, before);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    assert_eq!(animation_json(&editor), pool);
    track_edit(&mut editor, TrackEdit::ToggleKey { frame: 30 });
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::SourceText)
            .unwrap()
            .keys()
            .len(),
        4
    );
    assert_eq!(animation_json(&editor)["strings"], pool["strings"]);
}
#[test]
fn source_text_stopwatch_and_last_key_bake_current_or_last_removed_sample() {
    for (frame, expected) in [(0, "Base"), (25, "Second 👋\r\nline"), (90, "")] {
        let mut editor = animated();
        let before = editor.project().clone();
        track_edit(&mut editor, TrackEdit::ToggleAnimation { frame });
        let layer = editor.selected_layer().unwrap();
        assert!(layer.source_text_animation().is_default());
        assert_eq!(layer.source_text_at(0), Some(expected));
        assert_eq!(layer.source_text_at(149), Some(expected));
        editor.undo();
        assert_eq!(editor.project(), &before);
    }
    let mut editor = scene();
    track_edit(&mut editor, TrackEdit::ToggleKey { frame: 50 });
    edit(&mut editor, 50, "Freeze this key");
    track_edit(&mut editor, TrackEdit::ToggleKey { frame: 50 });
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .source_text_animation()
            .is_default()
    );
    assert_eq!(
        editor.selected_layer().unwrap().source_text_at(0),
        Some("Freeze this key")
    );
    let mut editor = animated();
    // DeleteKeys has no playhead: its existing sorted-key traversal determines
    // the final removed key, independently of clipboard/selection order.
    editor
        .execute(Command::DeleteKeys(vec![
            key(40),
            key(10),
            key(20),
            key(20),
        ]))
        .unwrap();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .source_text_animation()
            .is_default()
    );
    assert_eq!(editor.selected_layer().unwrap().source_text_at(0), Some(""));
}
#[test]
fn source_text_rejects_numeric_easing_handles_modes_and_velocity_atomically() {
    let mut editor = animated();
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: Property::Rotation.into(),
            edit: TrackEdit::ToggleKey { frame: 10 },
        })
        .unwrap();
    let numeric = KeyRef {
        id: 1,
        property: Property::Rotation.into(),
        frame: 10,
    };
    for command in [
        generic(TrackEdit::Value {
            frame: 10,
            value: 1.0,
        }),
        generic(TrackEdit::Keyframe {
            from: 10,
            to: 11,
            value: 0.0,
        }),
        generic(TrackEdit::Interpolate {
            frame: 10,
            interpolation: Interpolation::Linear,
        }),
        generic(TrackEdit::Interpolate {
            frame: 10,
            interpolation: Interpolation::Smooth,
        }),
        generic(TrackEdit::Interpolate {
            frame: 10,
            interpolation: Interpolation::Bezier(Bezier::default()),
        }),
        Command::SetTemporalHandle {
            id: 1,
            property: PropertyPath::SourceText,
            frame: 10,
            incoming: false,
            handle: TemporalHandle {
                influence: 0.5,
                slope: 1.0,
            },
        },
        Command::SetTemporalMode {
            id: 1,
            property: PropertyPath::SourceText,
            frame: 10,
            mode: TemporalMode::Independent,
        },
        Command::SetTemporalMode {
            id: 1,
            property: PropertyPath::SourceText,
            frame: 10,
            mode: TemporalMode::Auto,
        },
        Command::ScaleKeyVelocities {
            keys: vec![numeric, key(10)],
            scale: KeyVelocityScale {
                origin: 0.0,
                factor: 2.0,
            },
        },
        Command::ScaleKeys {
            keys: vec![numeric, key(10)],
            scale: KeyScale {
                time_origin: 0.0,
                time_scale: 1.5,
                value_origin: 0.0,
                value_scale: 2.0,
            },
        },
        Command::Batch(vec![
            source_command(50, "Must roll back"),
            generic(TrackEdit::Interpolate {
                frame: 10,
                interpolation: Interpolation::Smooth,
            }),
        ]),
    ] {
        assert_rejected(&mut editor, command);
    }
    track_edit(
        &mut editor,
        TrackEdit::Interpolate {
            frame: 10,
            interpolation: Interpolation::Hold,
        },
    );
    assert_rejected(
        &mut editor,
        generic(TrackEdit::Interpolate {
            frame: 11,
            interpolation: Interpolation::Hold,
        }),
    );
}
#[test]
fn source_text_requires_valid_unlocked_text_target_and_composition_frame() {
    let mut editor = scene();
    for command in [
        source_command(150, "bad"),
        source_command(u32::MAX, "bad"),
        generic(TrackEdit::ToggleKey { frame: 150 }),
        Command::EditSourceText {
            id: 900,
            frame: 0,
            text: "bad".into(),
        },
    ] {
        assert_rejected(&mut editor, command);
    }
    editor.execute(Command::ToggleLocked(1)).unwrap();
    assert_rejected(&mut editor, source_command(0, "Base"));
    assert_rejected(&mut editor, generic(TrackEdit::ToggleKey { frame: 0 }));
    editor.execute(Command::AddRectangle).unwrap();
    let layer = editor.selected_layer().unwrap();
    assert!(layer.source_text_at(0).is_none());
    assert!(layer.track(PropertyPath::SourceText).is_none());
    assert!(layer.track_label(PropertyPath::SourceText).is_none());
    assert_rejected(
        &mut editor,
        Command::EditSourceText {
            id: 2,
            frame: 0,
            text: "bad".into(),
        },
    );
    assert_rejected(
        &mut editor,
        Command::EditTrack {
            id: 2,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
}
#[test]
fn source_text_set_content_guard_allows_only_unchanged_baseline_with_font_size() {
    let mut editor = animated();
    let animation = editor
        .selected_layer()
        .unwrap()
        .source_text_animation()
        .clone();
    for content in [
        Content::Rectangle,
        Content::Text {
            text: "Changed baseline".into(),
            font_size: 40.0,
        },
        Content::Text {
            text: "Second 👋\r\nline".into(),
            font_size: 40.0,
        },
    ] {
        assert_rejected(&mut editor, Command::SetContent { id: 1, content });
    }
    editor
        .execute(Command::SetContent {
            id: 1,
            content: Content::Text {
                text: "Base".into(),
                font_size: 75.0,
            },
        })
        .unwrap();
    editor
        .execute(Command::SetColor {
            id: 1,
            color: 0x123456,
        })
        .unwrap();
    let mut style = editor.selected_layer().unwrap().text_style().clone();
    style.tracking = 50.0;
    editor
        .execute(Command::SetTextStyle { id: 1, style })
        .unwrap();
    editor
        .execute(Command::EditText {
            id: 1,
            parameter: TextParam::FillOpacity,
            edit: TrackEdit::Value {
                frame: 0,
                value: 65.0,
            },
        })
        .unwrap();
    let layer = editor.selected_layer().unwrap();
    assert_eq!(layer.source_text_animation(), &animation);
    assert_eq!(layer.text_typography_at(25).unwrap().font_size, 75.0);
    assert_eq!(layer.source_text_at(25), Some("Second 👋\r\nline"));
}
#[test]
fn source_text_clipboard_maps_actual_strings_between_independent_pools() {
    let mut editor = animated();
    let copied = editor
        .selected_layer()
        .unwrap()
        .copy_key(PropertyPath::SourceText, 20)
        .unwrap();
    assert_eq!(copied.source_text.as_deref(), Some("Second 👋\r\nline"));
    assert!(copied.path_pose.is_none());
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Other pool".into(),
                font_size: 30.0,
            },
            width: 200.0,
            height: 80.0,
            name: "Destination".into(),
        })
        .unwrap();
    editor
        .execute(Command::EditTrack {
            id: 2,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleKey { frame: 0 },
        })
        .unwrap();
    editor
        .execute(Command::EditSourceText {
            id: 2,
            frame: 10,
            text: "Different index one".into(),
        })
        .unwrap();
    editor
        .execute(Command::PasteKeys {
            keys: vec![copied.clone()],
            frame: 60,
            target: Some(2),
        })
        .unwrap();
    let layer = editor.selected_layer().unwrap();
    assert_eq!(layer.source_text_at(59), Some("Different index one"));
    assert_eq!(layer.source_text_at(60), Some("Second 👋\r\nline"));
    assert_ne!(
        layer.track(PropertyPath::SourceText).unwrap().keys()[&60].value,
        copied.data.value
    );
    assert_rejected(
        &mut editor,
        Command::PasteKeys {
            keys: vec![copied],
            frame: 60,
            target: Some(2),
        },
    );
}
#[test]
fn source_text_clipboard_rejects_missing_mismatched_and_temporal_payloads_atomically() {
    let mut editor = animated();
    let copied = editor
        .selected_layer()
        .unwrap()
        .copy_key(PropertyPath::SourceText, 20)
        .unwrap();
    let mut invalid = Vec::new();
    let mut bad = copied.clone();
    bad.source_text = None;
    invalid.push(bad);
    let mut bad = copied.clone();
    bad.source_text = Some("a".repeat(16385));
    invalid.push(bad);
    let mut bad = copied.clone();
    bad.key.property = Property::Rotation.into();
    invalid.push(bad);
    let mut bad = copied.clone();
    bad.effect_kind = Some(EffectKind::GaussianBlur);
    invalid.push(bad);
    let mut bad = copied.clone();
    bad.data.interpolation = Interpolation::Linear;
    invalid.push(bad);
    let mut bad = copied.clone();
    bad.data.temporal.mode = TemporalMode::Auto;
    invalid.push(bad);
    let mut bad = copied.clone();
    bad.data.temporal.incoming = Some(TemporalHandle {
        slope: 1.0,
        influence: 0.3,
    });
    invalid.push(bad);
    for value in [-1.0, 0.5, 10_000.0, f64::NAN, f64::INFINITY] {
        let mut bad = copied.clone();
        bad.data.value = value;
        invalid.push(bad);
    }
    for bad in invalid {
        assert_rejected(
            &mut editor,
            Command::Batch(vec![
                source_command(50, "rollback"),
                Command::PasteKeys {
                    keys: vec![bad],
                    frame: 60,
                    target: Some(1),
                },
            ]),
        );
    }
    assert_rejected(
        &mut editor,
        Command::PasteKeys {
            keys: vec![copied.clone()],
            frame: 150,
            target: Some(1),
        },
    );
    editor.execute(Command::AddRectangle).unwrap();
    assert_rejected(
        &mut editor,
        Command::PasteKeys {
            keys: vec![copied.clone()],
            frame: 60,
            target: Some(2),
        },
    );
    editor.execute(Command::ToggleLocked(1)).unwrap();
    assert_rejected(
        &mut editor,
        Command::PasteKeys {
            keys: vec![copied],
            frame: 60,
            target: Some(1),
        },
    );
}
#[test]
fn source_text_move_time_scale_and_delete_preserve_strings_and_reject_collisions() {
    let mut editor = animated();
    let strings = animation_json(&editor)["strings"].clone();
    editor
        .execute(Command::MoveKeys {
            keys: vec![key(20)],
            delta: 5,
        })
        .unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().source_text_at(24),
        Some("Base")
    );
    assert_eq!(
        editor.selected_layer().unwrap().source_text_at(25),
        Some("Second 👋\r\nline")
    );
    assert_rejected(
        &mut editor,
        Command::MoveKeys {
            keys: vec![key(25)],
            delta: 15,
        },
    );
    editor
        .execute(Command::ScaleKeys {
            keys: vec![key(10), key(25), key(40)],
            scale: KeyScale {
                time_origin: 0.0,
                time_scale: 2.0,
                value_origin: 0.0,
                value_scale: 1.0,
            },
        })
        .unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().source_text_at(49),
        Some("Base")
    );
    assert_eq!(
        editor.selected_layer().unwrap().source_text_at(50),
        Some("Second 👋\r\nline")
    );
    assert_eq!(animation_json(&editor)["strings"], strings);
    assert_rejected(
        &mut editor,
        Command::ScaleKeys {
            keys: vec![key(20), key(50)],
            scale: KeyScale {
                time_origin: 0.0,
                time_scale: 0.001,
                value_origin: 0.0,
                value_scale: 1.0,
            },
        },
    );
    assert_rejected(&mut editor, Command::DeleteKeys(vec![key(20), key(12)]));
}
#[test]
fn source_text_roundtrips_unicode_empty_and_unchanged_view_without_new_container_version() {
    let editor = animated();
    for view in [
        br#"{"version":1,"frame":30}"#.as_slice(),
        br#" { "version":2, "frame":45, "pins":[] } "#.as_slice(),
    ] {
        let json = editor.project().to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
        let bytes = project_file::encode(editor.project(), Some(view)).unwrap();
        assert_eq!(&bytes[..8], project_file::MAGIC);
        assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
        let decoded = project_file::decode(&bytes).unwrap();
        assert_eq!(decoded.project, *editor.project());
        assert_eq!(decoded.view, Some(view));
        assert_eq!(
            project_file::encode(&decoded.project, decoded.view).unwrap(),
            bytes
        );
    }
}
#[test]
fn source_text_storage_rejects_schema_disguise_indices_nonhold_and_keyless_pools() {
    let mut editor = animated();
    editor.execute(Command::NewComposition).unwrap();
    let original = editor.project().clone();
    let value = serde_json::to_value(&original).unwrap();
    let mut bad_values = Vec::new();
    let mut bad = value.clone();
    bad["version"] = 52.into();
    bad_values.push(bad);
    fn path(v: &mut serde_json::Value) -> &mut serde_json::Value {
        &mut v["other_compositions"]["1"]["layers"][0]["source_text_animation"]
    }
    for index in [
        serde_json::json!(-1),
        serde_json::json!(0.5),
        serde_json::json!(99),
        serde_json::json!(null),
    ] {
        let mut bad = value.clone();
        path(&mut bad)["timing"]["keys"]["20"]["value"] = index;
        bad_values.push(bad);
    }
    for interpolation in [
        serde_json::json!("Linear"),
        serde_json::json!("Smooth"),
        serde_json::json!({"Bezier":{"x1":0.2,"y1":0.3,"x2":0.7,"y2":0.8}}),
    ] {
        let mut bad = value.clone();
        path(&mut bad)["timing"]["keys"]["20"]["interpolation"] = interpolation;
        bad_values.push(bad);
    }
    let mut bad = value.clone();
    path(&mut bad)["timing"]["keys"]["20"]["temporal"] = serde_json::json!({"mode":"Auto"});
    bad_values.push(bad);
    let mut bad = value.clone();
    path(&mut bad)["timing"]["keys"]["20"]["temporal"] =
        serde_json::json!({"incoming":{"slope":1.0,"influence":0.5}});
    bad_values.push(bad);
    let mut bad = value.clone();
    path(&mut bad)["timing"]["value"] = 0.25.into();
    bad_values.push(bad);
    let mut bad = value.clone();
    let data = path(&mut bad)["timing"]["keys"]["20"].clone();
    path(&mut bad)["timing"]["keys"]["150"] = data;
    bad_values.push(bad);
    let mut bad = value.clone();
    path(&mut bad)["strings"] = serde_json::json!([]);
    bad_values.push(bad);
    let mut bad = value.clone();
    path(&mut bad)["timing"]["keys"] = serde_json::json!({});
    bad_values.push(bad);
    let mut bad = value.clone();
    path(&mut bad)["strings"][0] = "x".repeat(16385).into();
    bad_values.push(bad);
    let mut bad = value.clone();
    bad["other_compositions"]["1"]["layers"][0]["content"] = "Rectangle".into();
    bad_values.push(bad);
    for bad in bad_values {
        assert!(Project::from_json(&bad.to_string()).is_err());
        assert!(document::decode_native(bad.clone(), BTreeMap::new()).is_err());
        if let Ok(candidate) = serde_json::from_value::<Project>(bad) {
            assert!(candidate.validate().is_err());
            assert!(project_file::encode(&candidate, None).is_err());
            let before = editor.current.clone();
            assert!(editor.replace_project(candidate).is_err());
            assert_eq!(editor.current, before);
        }
    }
}
#[test]
fn source_text_interning_reuses_dead_slots_without_changing_other_live_indices() {
    let mut editor = animated();
    let initial = animation_json(&editor);
    let left = editor
        .selected_layer()
        .unwrap()
        .track(PropertyPath::SourceText)
        .unwrap()
        .keys()[&10]
        .value;
    let right = editor
        .selected_layer()
        .unwrap()
        .track(PropertyPath::SourceText)
        .unwrap()
        .keys()[&40]
        .value;
    for n in 0..50 {
        edit(&mut editor, 20, &format!("replacement {n}"));
    }
    let final_pool = animation_json(&editor);
    assert_eq!(
        final_pool["strings"].as_array().unwrap().len(),
        initial["strings"].as_array().unwrap().len()
    );
    let layer = editor.selected_layer().unwrap();
    let track = layer.track(PropertyPath::SourceText).unwrap();
    assert_eq!(track.keys()[&10].value, left);
    assert_eq!(track.keys()[&40].value, right);
    assert_eq!(layer.source_text_at(10), Some("Base"));
    assert_eq!(layer.source_text_at(40), Some(""));
    edit(&mut editor, 20, "Base");
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::SourceText)
            .unwrap()
            .keys()[&20]
            .value,
        left
    );
}

fn editor_with_pool(strings: Vec<String>, frames: Vec<(Frame, usize)>) -> Editor {
    let mut project = scene().project().clone();
    project.version = 53;
    project.composition.duration = 20_000;
    let keys: BTreeMap<_, _> = frames
        .into_iter()
        .map(|(frame, index)| {
            (
                frame,
                Keyframe {
                    value: index as f64,
                    interpolation: Interpolation::Hold,
                    temporal: TemporalHandles::default(),
                },
            )
        })
        .collect();
    project.composition.layers[0].source_text_animation =
        serde_json::from_value(serde_json::json!({
            "strings": strings, "timing": { "value": 0.0, "keys": keys }
        }))
        .unwrap();
    let mut editor = Editor::default();
    editor.replace_project(project).unwrap();
    editor.select(1);
    editor
}
fn full_pool(character: char) -> Vec<String> {
    (0..64)
        .map(|index| format!("{:04}{}", index, character.to_string().repeat(16_380)))
        .collect()
}
#[test]
fn source_text_string_byte_bound_is_utf8_exact_and_failures_preserve_redo() {
    let mut editor = scene();
    let exact = "é".repeat(8192);
    edit(&mut editor, 0, &exact);
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .source_text_at(0)
            .unwrap()
            .len(),
        16_384
    );
    track_edit(&mut editor, TrackEdit::ToggleKey { frame: 0 });
    edit(&mut editor, 20, "redo");
    editor.undo();
    assert_rejected(&mut editor, source_command(30, &format!("{exact}x")));
    assert_rejected(&mut editor, source_command(0, &"💡".repeat(4097)));
    edit(&mut editor, 30, &"💡".repeat(4096));
}
#[test]
fn source_text_exact_pool_cap_reuses_sole_reference_but_rejects_shared_reference_growth() {
    let mut editor = editor_with_pool(
        full_pool('x'),
        (0..64).map(|index| (index, index as usize)).collect(),
    );
    let pool = animation_json(&editor);
    assert_eq!(
        pool["strings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().len())
            .sum::<usize>(),
        1024 * 1024
    );
    let replacement = format!("new!{}", "y".repeat(16_380));
    edit(&mut editor, 0, &replacement);
    let after = animation_json(&editor);
    assert_eq!(after["strings"].as_array().unwrap().len(), 64);
    for frame in 1..64 {
        assert_eq!(
            after["timing"]["keys"][frame.to_string()]["value"],
            pool["timing"]["keys"][frame.to_string()]["value"]
        );
    }
    edit(&mut editor, 100, &replacement);
    assert_rejected(
        &mut editor,
        source_command(0, &format!("next{}", "z".repeat(16_380))),
    );
    assert_rejected(
        &mut editor,
        source_command(101, "new string beyond full pool"),
    );
    editor.execute(Command::DeleteKeys(vec![key(100)])).unwrap();
    edit(&mut editor, 0, &format!("next{}", "z".repeat(16_380)));
}
#[test]
fn source_text_ten_thousand_keys_and_strings_are_bounded_without_blocking_replacements() {
    let strings: Vec<_> = (0..10_000).map(|index| format!("text {index}")).collect();
    let mut editor = editor_with_pool(
        strings,
        (0..10_000).map(|index| (index, index as usize)).collect(),
    );
    assert_rejected(&mut editor, generic(TrackEdit::ToggleKey { frame: 10_000 }));
    assert_rejected(&mut editor, source_command(10_000, "extra"));
    edit(&mut editor, 0, "replacement at cap");
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::SourceText)
            .unwrap()
            .keys()
            .len(),
        10_000
    );
    assert_eq!(
        animation_json(&editor)["strings"].as_array().unwrap().len(),
        10_000
    );
    let value = serde_json::to_value(editor.project()).unwrap();
    let mut too_many_strings = value.clone();
    too_many_strings["composition"]["layers"][0]["source_text_animation"]["strings"]
        .as_array_mut()
        .unwrap()
        .push("extra".into());
    assert!(Project::from_json(&too_many_strings.to_string()).is_err());
    let mut too_many_keys = value;
    let first =
        too_many_keys["composition"]["layers"][0]["source_text_animation"]["timing"]["keys"]["0"]
            .clone();
    too_many_keys["composition"]["layers"][0]["source_text_animation"]["timing"]["keys"]["10000"] =
        first;
    assert!(Project::from_json(&too_many_keys.to_string()).is_err());
}
#[test]
fn source_text_nonfinite_indices_and_inconsistent_default_are_rejected_in_memory() {
    let mut editor = animated();
    let before = editor.current.clone();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for base in [true, false] {
            let mut project = editor.project().clone();
            let timing = &mut project.composition.layers[0].source_text_animation.timing;
            if base {
                timing.value = value;
            } else {
                timing.keys.get_mut(&20).unwrap().value = value;
            }
            assert!(project.validate().is_err());
            assert!(project_file::encode(&project, None).is_err());
            assert!(editor.replace_project(project).is_err());
            assert_eq!(editor.current, before);
        }
    }
    let mut project = scene().project().clone();
    project.composition.layers[0]
        .source_text_animation
        .timing
        .value = 1.0;
    assert!(project.validate().is_err());
    let mut pool_over = animation_json(&editor);
    pool_over["strings"] = serde_json::json!(full_pool('x'));
    pool_over["strings"]
        .as_array_mut()
        .unwrap()
        .push("x".into());
    let mut project = editor.project().clone();
    project.composition.layers[0].source_text_animation =
        serde_json::from_value(pool_over).unwrap();
    assert!(project.validate().is_err());
}
#[test]
fn source_text_escaped_pools_count_against_existing_metadata_budget_atomically() {
    let editor = editor_with_pool(
        full_pool('\u{0001}'),
        (0..64).map(|index| (index, index as usize)).collect(),
    );
    let mut project = editor.project().clone();
    let mut duplicate = project.composition.layers[0].clone();
    duplicate.id = 2;
    project.composition.layers.push(duplicate);
    project.next_layer_id = 3;
    let mut editor = Editor::default();
    editor.replace_project(project).unwrap();
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo retained".into(),
        })
        .unwrap();
    editor.undo();
    let before = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    let error = editor.execute(Command::DuplicateLayer(1)).unwrap_err();
    assert!(error.contains("metadata exceeds 16 MiB"), "{error}");
    assert_eq!(editor.current, before);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    let mut oversized = editor.project().clone();
    let mut duplicate = oversized.composition.layers[0].clone();
    duplicate.id = 3;
    oversized.composition.layers.push(duplicate);
    oversized.next_layer_id = 4;
    oversized.validate().unwrap();
    assert!(oversized.to_json().is_err());
    assert!(project_file::encode(&oversized, None).is_err());
    assert!(editor.replace_project(oversized).is_err());
    assert_eq!(editor.current, before);
}
#[test]
fn source_text_duplicate_split_shift_cross_fps_and_precompose_keep_full_state() {
    let mut editor = animated();
    let animation = editor
        .selected_layer()
        .unwrap()
        .source_text_animation()
        .clone();
    editor.execute(Command::DuplicateLayer(1)).unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().source_text_animation(),
        &animation
    );
    editor
        .execute(Command::SplitLayers {
            ids: vec![1],
            frame: 25,
        })
        .unwrap();
    let split = editor.selected_layer().unwrap().id();
    assert_eq!(
        editor.selected_layer().unwrap().source_text_animation(),
        &animation
    );
    assert_eq!(
        editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .source_text_animation(),
        &animation
    );
    editor
        .execute(Command::SetLayerRange {
            id: split,
            start: 25,
            end: 100,
        })
        .unwrap();
    editor
        .execute(Command::ShiftLayer {
            id: split,
            delta: 5,
        })
        .unwrap();
    let shifted = editor.project().composition().layer(split).unwrap();
    for (old, new) in [(10, 15), (20, 25), (40, 45)] {
        assert_eq!(
            shifted.track(PropertyPath::SourceText).unwrap().keys()[&new],
            animation.timing.keys[&old]
        );
    }
    assert_eq!(shifted.source_text_at(25), Some("Second 👋\r\nline"));
    let clipboard = editor.copy_layers(&[split]).unwrap();
    editor.execute(Command::NewComposition).unwrap();
    editor
        .execute(Command::ConfigureComposition {
            name: "60fps".into(),
            width: 1920,
            height: 1080,
            fps: 60,
            duration: 300,
        })
        .unwrap();
    editor.execute(Command::PasteLayers(clipboard)).unwrap();
    let pasted = editor.selected_layer().unwrap();
    let pasted_id = pasted.id();
    for (old, new) in [(10, 30), (20, 50), (40, 90)] {
        assert_eq!(
            pasted.track(PropertyPath::SourceText).unwrap().keys()[&new],
            animation.timing.keys[&old]
        );
    }
    let pasted_animation = pasted.source_text_animation().clone();
    editor
        .execute(Command::Precompose {
            layers: vec![pasted_id],
            name: "Animated words".into(),
        })
        .unwrap();
    let Content::Composition { composition, .. } = editor.selected_layer().unwrap().content()
    else {
        panic!("expected precomposition");
    };
    let child = editor
        .project()
        .composition_by_id(*composition)
        .unwrap()
        .layer(pasted_id)
        .unwrap();
    assert_eq!(child.source_text_animation(), &pasted_animation);
    assert_eq!(child.source_text_at(50), Some("Second 👋\r\nline"));
    assert_eq!(
        Project::from_json(&editor.project().to_json().unwrap()).unwrap(),
        *editor.project()
    );
}
#[test]
fn source_text_duration_and_cross_fps_collisions_reject_atomically_while_rate_keeps_frames() {
    let mut editor = animated();
    assert_rejected(
        &mut editor,
        Command::ConfigureComposition {
            name: "Too short".into(),
            width: 1920,
            height: 1080,
            fps: 30,
            duration: 40,
        },
    );
    let animation = editor
        .selected_layer()
        .unwrap()
        .source_text_animation()
        .clone();
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "Rate change".into(),
            width: 1920,
            height: 1080,
            fps: 60.into(),
            duration: 150,
            display_start: 0,
        })
        .unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().source_text_animation(),
        &animation
    );
    track_edit(&mut editor, TrackEdit::ToggleKey { frame: 11 });
    let clipboard = editor.copy_layers(&[1]).unwrap();
    editor.execute(Command::NewComposition).unwrap();
    editor
        .execute(Command::ConfigureComposition {
            name: "Coarse rate".into(),
            width: 1920,
            height: 1080,
            fps: 1,
            duration: 150,
        })
        .unwrap();
    assert_rejected(&mut editor, Command::PasteLayers(clipboard));
}

#[test]
fn source_text_fragmented_dead_slots_reclaim_only_unreferenced_strings() {
    // 128 eight-KiB slots exactly fill the pool. Three dead slots can admit one
    // sixteen-KiB string even though no single dead slot is large enough.
    let strings: Vec<_> = (0..128)
        .map(|index| format!("{index:04}{}", "x".repeat(8188)))
        .collect();
    let mut editor = editor_with_pool(
        strings,
        (3..128).map(|index| (index, index as usize)).collect(),
    );
    let before = animation_json(&editor);
    let replacement = format!("new!{}", "y".repeat(16_380));
    edit(&mut editor, 0, &replacement);
    let after = animation_json(&editor);
    assert_eq!(after["strings"].as_array().unwrap().len(), 128);
    assert_eq!(after["strings"][0].as_str(), Some(replacement.as_str()));
    assert_eq!(after["strings"][1].as_str(), Some(""));
    assert_eq!(after["strings"][2], before["strings"][2]);
    for index in 3..128 {
        assert_eq!(after["strings"][index], before["strings"][index]);
        assert_eq!(
            after["timing"]["keys"][index.to_string()],
            before["timing"]["keys"][index.to_string()]
        );
    }
    assert_eq!(
        after["strings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().len())
            .sum::<usize>(),
        1024 * 1024
    );
    // Reusing an empty dead slot must not alter the still-live string in slot0.
    edit(&mut editor, 1, "");
    assert_eq!(
        editor.selected_layer().unwrap().source_text_at(0),
        Some(replacement.as_str())
    );
    assert_eq!(editor.selected_layer().unwrap().source_text_at(1), Some(""));
}
#[test]
fn source_text_fragmented_admission_failure_does_not_clear_dead_slots() {
    let strings: Vec<_> = (0..128)
        .map(|index| format!("{index:04}{}", "x".repeat(8188)))
        .collect();
    // Only one eight-KiB slot is dead, so sixteen KiB does not fit.
    let mut editor = editor_with_pool(
        strings,
        (1..128).map(|index| (index, index as usize)).collect(),
    );
    let before = animation_json(&editor);
    assert_rejected(&mut editor, source_command(0, &"z".repeat(16_384)));
    assert_eq!(animation_json(&editor), before);
}
#[test]
fn source_text_reuse_prefers_large_dead_slot_without_unnecessary_reclamation() {
    let mut strings = full_pool('x');
    // Slot0 is tiny and dead, slot1 is large and dead. Live strings fill the rest.
    strings[0] = "x".into();
    strings.push("q".repeat(16_383));
    let mut editor = editor_with_pool(
        strings,
        (2..65).map(|index| (index, index as usize)).collect(),
    );
    let before = animation_json(&editor);
    let replacement = "y".repeat(16_384);
    edit(&mut editor, 0, &replacement);
    let after = animation_json(&editor);
    assert_eq!(after["strings"][0], before["strings"][0]);
    assert_eq!(after["strings"][1].as_str(), Some(replacement.as_str()));
    for index in 2..65 {
        assert_eq!(after["strings"][index], before["strings"][index]);
    }
}
#[test]
fn source_text_ten_thousand_slot_occupancy_reuses_a_high_dead_index() {
    let strings: Vec<_> = (0..10_000).map(|index| format!("text {index}")).collect();
    let mut editor = editor_with_pool(
        strings,
        (0..9999).map(|index| (index, index as usize)).collect(),
    );
    let before = animation_json(&editor);
    edit(&mut editor, 10_000, "one linear live-slot scan");
    let after = animation_json(&editor);
    assert_eq!(after["strings"].as_array().unwrap().len(), 10_000);
    assert_eq!(
        after["strings"][9999].as_str(),
        Some("one linear live-slot scan")
    );
    assert_eq!(
        after["timing"]["keys"]["10000"]["value"],
        serde_json::json!(9999.0)
    );
    for frame in [0, 1000, 5000, 9998] {
        assert_eq!(
            after["timing"]["keys"][frame.to_string()],
            before["timing"]["keys"][frame.to_string()]
        );
        assert_eq!(after["strings"][frame], before["strings"][frame]);
    }
}
