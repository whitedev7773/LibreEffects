//! Public-model regression coverage; these tests never link GPUI or open media.
use libre_effects_core::*;
use serde_json::{Value, json};

fn scene(content: Content) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content,
            width: 300.0,
            height: 100.0,
            name: "Timing fixture".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetLayerRange {
            id: 1,
            start: 20,
            end: 100,
        })
        .unwrap();
    editor.clear_history();
    editor
}
fn text_scene() -> Editor {
    scene(Content::Text {
        text: "First 👋".into(),
        font_size: 32.0,
    })
}
fn video(origin: i64) -> Content {
    Content::Video {
        path: "fixture.mp4".into(),
        duration: 20.0,
        source_fps: 30.0,
        start_frame: origin,
        playback: VideoPlayback::default(),
        audio: None,
    }
}
fn layer(editor: &Editor) -> &Layer {
    editor.project().composition().layer(1).unwrap()
}
fn raw(project: &Project) -> Value {
    serde_json::from_str(&project.to_json().unwrap()).unwrap()
}
fn rejected(editor: &mut Editor, command: Command) {
    let before = editor.project().clone();
    let state = (
        editor.selected(),
        editor.can_undo(),
        editor.can_redo(),
        editor.context_generation(),
    );
    assert!(editor.execute(command).is_err());
    assert_eq!(editor.project(), &before);
    assert_eq!(
        (
            editor.selected(),
            editor.can_undo(),
            editor.can_redo(),
            editor.context_generation()
        ),
        state
    );
}
fn set_start(editor: &mut Editor, frame: i64) {
    editor
        .execute(Command::SetLayerStart { id: 1, frame })
        .unwrap();
}
fn set_label(editor: &mut Editor, index: u8) {
    editor
        .execute(Command::SetLayerLabel { id: 1, index })
        .unwrap();
}

#[test]
fn independent_origin_is_never_trim_in_point_and_negative_origins_are_real() {
    let mut e = text_scene();
    assert_eq!((layer(&e).start_frame(), layer(&e).in_frame()), (0, 20));
    set_start(&mut e, -5);
    assert_eq!(
        (
            layer(&e).start_frame(),
            layer(&e).in_frame(),
            layer(&e).out_frame(150)
        ),
        (-5, 15, 95)
    );
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 40,
        end: 80,
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), -5);
    e.execute(Command::TrimLayers {
        ids: vec![1],
        frame: 45,
        start: true,
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), -5);
    set_start(&mut e, 10);
    assert_eq!(
        (
            layer(&e).start_frame(),
            layer(&e).in_frame(),
            layer(&e).out_frame(150)
        ),
        (10, 60, 95)
    );
}

#[test]
fn synthetic_sixty_fps_nonzero_origin_uses_start_delta_before_final_out_point() {
    let mut e = text_scene();
    e.execute(Command::ConfigureComposition {
        name: "60 fps".into(),
        width: 1920,
        height: 1080,
        fps: 60,
        duration: 1800,
    })
    .unwrap();
    let mut value = raw(e.project());
    value["version"] = json!(64);
    let fixture_layer = &mut value["composition"]["layers"][0];
    fixture_layer["start_frame"] = json!(1);
    fixture_layer["in_frame"] = json!(1);
    fixture_layer["out_frame"] = json!(699); // 11.65 seconds.
    e.replace_project(Project::from_json(&value.to_string()).unwrap())
        .unwrap();
    set_start(&mut e, 300);
    assert_eq!(
        (
            layer(&e).start_frame(),
            layer(&e).in_frame(),
            layer(&e).out_frame(1800)
        ),
        (300, 300, 998)
    );
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 300,
        end: 420,
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), 300);
}

#[test]
fn timing_and_labels_commit_once_undo_redo_and_exact_noops_preserve_history() {
    let mut e = text_scene();
    let mut value = raw(e.project());
    value["version"] = json!(63);
    e.replace_project(Project::from_json(&value.to_string()).unwrap())
        .unwrap();
    e.clear_history();
    e.execute(Command::RenameLayer {
        id: 1,
        name: "Redo sentinel".into(),
    })
    .unwrap();
    e.undo();
    let before = e.project().clone();
    let receipt = e.context_generation();
    assert!(e.can_redo());
    set_start(&mut e, 0);
    set_label(&mut e, 0);
    assert_eq!(e.project(), &before);
    assert_eq!(e.context_generation(), receipt);
    assert!(e.can_redo());
    assert!(
        raw(e.project())["composition"]["layers"][0]
            .get("start_frame")
            .is_none()
    );
    assert!(
        raw(e.project())["composition"]["layers"][0]
            .get("label_index")
            .is_none()
    );
    e.execute(Command::Batch(vec![
        Command::SetLayerStart { id: 1, frame: 10 },
        Command::SetLayerLabel { id: 1, index: 9 },
    ]))
    .unwrap();
    let after = e.project().clone();
    assert_eq!(raw(&after)["version"], json!(64));
    e.undo();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
    e.redo();
    assert_eq!(e.project(), &after);
    let receipt = e.context_generation();
    set_start(&mut e, 10);
    set_label(&mut e, 9);
    assert_eq!(e.context_generation(), receipt);
}

#[test]
fn indexed_labels_only_change_label_storage_and_never_fill_or_text_paint() {
    let mut e = text_scene();
    let original = serde_json::to_value(layer(&e)).unwrap();
    let color = layer(&e).color();
    assert_eq!(layer(&e).label_color(), None);
    let mut palette = std::collections::BTreeSet::new();
    for index in 1..=16 {
        set_label(&mut e, index);
        let mut actual = serde_json::to_value(layer(&e)).unwrap();
        assert_eq!(
            actual.as_object_mut().unwrap().remove("label_index"),
            Some(json!(index))
        );
        assert_eq!(actual, original);
        assert_eq!(layer(&e).color(), color);
        assert_eq!(layer(&e).label_index(), index);
        palette.insert(layer(&e).label_color().unwrap());
    }
    assert_eq!(palette.len(), 16);
    set_label(&mut e, 0);
    assert_eq!(layer(&e).label_index(), 0);
    assert!(layer(&e).label_color().is_some());
    rejected(&mut e, Command::SetLayerLabel { id: 1, index: 17 });
    rejected(
        &mut e,
        Command::SetLayerLabel {
            id: 1,
            index: u8::MAX,
        },
    );
}

#[test]
fn origin_moves_transform_effect_text_selector_animator_source_keys_and_markers() {
    let mut e = text_scene();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetParent {
        id: 1,
        parent: Some(2),
        frame: 0,
    })
    .unwrap();
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Add(EffectKind::GaussianBlur),
    })
    .unwrap();
    e.execute(Command::AddTextRangeSelector { id: 1 }).unwrap();
    e.execute(Command::AddTextAnimator { id: 1 }).unwrap();
    let paths = [
        PropertyPath::Transform(Property::PositionX),
        PropertyPath::Transform(Property::Opacity),
        PropertyPath::Effect {
            effect: 1,
            parameter: EffectParam::Radius,
        },
        PropertyPath::Text(TextParam::Tracking),
        PropertyPath::TextSelector {
            selector: 1,
            parameter: TextSelectorParam::Offset,
        },
        PropertyPath::TextAnimator {
            animator: 1,
            parameter: TextParam::AnimatorPositionX,
        },
        PropertyPath::SourceText,
    ];
    for property in paths {
        e.execute(Command::EditTrack {
            id: 1,
            property,
            edit: TrackEdit::ToggleAnimation { frame: 25 },
        })
        .unwrap();
        e.execute(Command::EditTrack {
            id: 1,
            property,
            edit: TrackEdit::ToggleKey { frame: 60 },
        })
        .unwrap();
    }
    e.execute(Command::EditSourceText {
        id: 1,
        frame: 60,
        text: "Second 日本語".into(),
    })
    .unwrap();
    e.execute(Command::Marker {
        target: MarkerTarget::Layer(1),
        edit: MarkerEdit::Add { frame: 40 },
    })
    .unwrap();
    e.execute(Command::Marker {
        target: MarkerTarget::Layer(1),
        edit: MarkerEdit::Update {
            id: 1,
            frame: 40,
            duration: 10,
            name: "Focus".into(),
            color: 0x123456,
        },
    })
    .unwrap();
    let before = layer(&e).clone();
    let parent = e.project().composition().layer(2).unwrap().clone();
    set_start(&mut e, 10);
    let after = layer(&e);
    assert_eq!(after.parent(), Some(2));
    assert_eq!(after.local_transform(35), before.local_transform(25));
    assert_eq!(after.text_style(), before.text_style());
    assert_eq!(e.project().composition().layer(2), Some(&parent));
    for property in paths {
        let expected: std::collections::BTreeMap<_, _> = before
            .track(property)
            .unwrap()
            .keys()
            .iter()
            .map(|(frame, key)| (frame + 10, key.clone()))
            .collect();
        assert_eq!(after.track(property).unwrap().keys(), &expected);
    }
    assert_eq!(after.source_text_at(70), Some("Second 日本語"));
    assert_eq!(
        (after.markers()[0].frame(), after.markers()[0].duration()),
        (50, 10)
    );
}

#[test]
fn origin_moves_complete_gradient_color_poses_and_contents_scalar_tracks() {
    let mut e = scene(Content::ShapeContents(ShapeContents::default()));
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::GradientFill {
                even_odd: false,
                gradient: ShapeGradient::default(),
            },
        },
    })
    .unwrap();
    for edit in [
        GradientColorsEdit::SetAnimation {
            frame: 25,
            enabled: true,
        },
        GradientColorsEdit::Color {
            frame: 60,
            stop: 1,
            color: 0xff0000,
        },
    ] {
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::GradientColors { item: 1, edit },
        })
        .unwrap();
    }
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item: 1,
            parameter: ContentsParam::Gradient(GradientParam::StartX),
            edit: TrackEdit::ToggleAnimation { frame: 30 },
        },
    })
    .unwrap();
    let before = layer(&e).clone();
    set_start(&mut e, 10);
    let (Content::ShapeContents(a), Content::ShapeContents(b)) =
        (before.content(), layer(&e).content())
    else {
        panic!()
    };
    let a = a.node(1).unwrap();
    let b = b.node(1).unwrap();
    let expected: std::collections::BTreeMap<_, _> = a
        .kind
        .gradient()
        .unwrap()
        .colors_animation()
        .unwrap()
        .keys()
        .iter()
        .map(|(frame, colors)| (frame + 10, colors.clone()))
        .collect();
    assert_eq!(
        b.kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap()
            .keys(),
        &expected
    );
    let path = PropertyPath::Contents {
        item: 1,
        parameter: ContentsParam::Gradient(GradientParam::StartX),
    };
    assert!(layer(&e).track(path).unwrap().keys().contains_key(&40));
}

#[test]
fn all_unrepresentable_moves_reject_atomically_including_batch_prefixes() {
    let mut e = text_scene();
    e.execute(Command::EditTrack {
        id: 1,
        property: PropertyPath::Transform(Property::Opacity),
        edit: TrackEdit::ToggleAnimation { frame: 5 },
    })
    .unwrap();
    e.clear_history();
    for frame in [i64::MIN, i64::MAX, -100_000_001, 100_000_001, -6, 51] {
        rejected(&mut e, Command::SetLayerStart { id: 1, frame });
    }
    rejected(
        &mut e,
        Command::ShiftLayer {
            id: 1,
            delta: i64::MAX,
        },
    );
    rejected(
        &mut e,
        Command::Batch(vec![
            Command::SetLayerLabel { id: 1, index: 3 },
            Command::SetLayerStart { id: 1, frame: -6 },
        ]),
    );
    e.execute(Command::Marker {
        target: MarkerTarget::Layer(1),
        edit: MarkerEdit::Add { frame: 145 },
    })
    .unwrap();
    rejected(&mut e, Command::SetLayerStart { id: 1, frame: 5 });
    e.execute(Command::ToggleLocked(1)).unwrap();
    rejected(&mut e, Command::SetLayerStart { id: 1, frame: 1 });
    rejected(&mut e, Command::SetLayerLabel { id: 1, index: 1 });
}

#[test]
fn trims_preserve_image_and_video_sources_and_shift_preserves_local_sampling() {
    let mut still = scene(Content::Image {
        png: "ZmFrZQ==".into(),
    });
    let image = layer(&still).content().clone();
    set_start(&mut still, -5);
    still
        .execute(Command::SetLayerRange {
            id: 1,
            start: 30,
            end: 60,
        })
        .unwrap();
    assert_eq!(layer(&still).content(), &image);
    assert_eq!(layer(&still).start_frame(), -5);

    let mut e = scene(video(3));
    let before = layer(&e).clone();
    assert_eq!(before.start_frame(), 3);
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 40,
        end: 80,
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), 3);
    assert_eq!(layer(&e).content(), before.content());
    assert_eq!(layer(&e).video_time(45, 30), before.video_time(45, 30));
    set_start(&mut e, -2);
    assert_eq!(layer(&e).video_time(40, 30), before.video_time(45, 30));
    assert_eq!(layer(&e).source_time(40, 30), before.source_time(45, 30));
}

#[test]
fn playback_rebases_and_content_replacements_do_not_change_logical_start_time() {
    for command in [
        Command::SetVideoSpeed { id: 1, speed: 2.0 },
        Command::SetVideoSourceIn {
            id: 1,
            seconds: 1.5,
        },
        Command::ReverseVideo { id: 1 },
        Command::FreezeVideo { id: 1, frame: 50 },
    ] {
        let mut e = scene(video(3));
        e.execute(command).unwrap();
        assert_eq!(layer(&e).start_frame(), 3);
        assert!(matches!(
            layer(&e).content(),
            Content::Video {
                start_frame: 20,
                ..
            }
        ));
        assert_eq!(raw(e.project())["version"], json!(64));
        let sampled = layer(&e).source_time(30, 30);
        set_start(&mut e, 8);
        assert_eq!(layer(&e).source_time(35, 30), sampled);
    }
    let mut e = scene(video(3));
    e.execute(Command::SetContent {
        id: 1,
        content: video(8),
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), 3);
    e.execute(Command::SetContent {
        id: 1,
        content: Content::Rectangle,
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), 3);
    let mut e = scene(Content::Rectangle);
    e.execute(Command::SetContent {
        id: 1,
        content: video(8),
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), 0);
}

#[test]
fn native_shift_preserves_sparse_timed_origins_and_updates_static_or_explicit_origins() {
    let mut e = scene(video(3));
    e.execute(Command::ShiftLayer { id: 1, delta: 5 }).unwrap();
    assert_eq!(layer(&e).start_frame(), 8);
    assert!(
        serde_json::to_value(layer(&e))
            .unwrap()
            .get("start_frame")
            .is_none()
    );
    let mut e = text_scene();
    e.execute(Command::ShiftLayer { id: 1, delta: 5 }).unwrap();
    assert_eq!(layer(&e).start_frame(), 5);
    assert_eq!(raw(e.project())["version"], json!(64));
    e.execute(Command::ShiftLayer { id: 1, delta: -10 })
        .unwrap();
    assert_eq!(layer(&e).start_frame(), -5);
}

#[test]
fn duplicate_split_precompose_and_cross_fps_paste_preserve_origin_and_labels() {
    let mut e = text_scene();
    set_start(&mut e, -5);
    set_label(&mut e, 6);
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetParent {
        id: 1,
        parent: Some(2),
        frame: 0,
    })
    .unwrap();
    e.execute(Command::DuplicateLayer(1)).unwrap();
    assert_eq!(e.selected_layer().unwrap().start_frame(), -5);
    assert_eq!(e.selected_layer().unwrap().label_index(), 6);
    assert_eq!(e.selected_layer().unwrap().parent(), Some(2));
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 50,
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), -5);
    assert_eq!(e.selected_layer().unwrap().start_frame(), -5);
    assert_eq!(e.selected_layer().unwrap().in_frame(), 50);
    assert_eq!(e.selected_layer().unwrap().label_index(), 6);
    let clipboard = e.copy_layers(&[1, 2]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "60 fps".into(),
        width: 1920,
        height: 1080,
        fps: 60,
        duration: 400,
    })
    .unwrap();
    e.execute(Command::PasteLayers(clipboard)).unwrap();
    let pasted = e
        .project()
        .composition()
        .layers()
        .iter()
        .find(|l| l.label_index() == 6)
        .unwrap();
    assert_eq!(pasted.start_frame(), -10);
    assert_eq!(pasted.in_frame(), 30);
    assert_eq!(pasted.out_frame(400), 100);
    assert!(
        e.project()
            .composition()
            .layer(pasted.parent().unwrap())
            .is_some()
    );
    let ids: Vec<_> = e
        .project()
        .composition()
        .layers()
        .iter()
        .map(Layer::id)
        .collect();
    let expected = e.project().composition().layers().to_vec();
    e.execute(Command::Precompose {
        layers: ids,
        name: "Nested".into(),
    })
    .unwrap();
    assert_eq!(
        e.project().composition_by_id(3).unwrap().layers(),
        expected.as_slice()
    );
}

#[test]
fn schema_64_roundtrips_active_and_inactive_layers_and_survives_general_edits() {
    for inactive in [false, true] {
        let mut e = text_scene();
        set_start(&mut e, -5);
        set_label(&mut e, 16);
        if inactive {
            e.execute(Command::NewComposition).unwrap();
        }
        e.execute(Command::SetCompositionBackground(0x123456))
            .unwrap();
        assert_eq!(raw(e.project())["version"], json!(64));
        let json = e.project().to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap().to_json().unwrap(), json);
        let view = br#" { "fixture": "view" } "#;
        let bytes = project_file::encode(e.project(), Some(view)).unwrap();
        let decoded = project_file::decode(&bytes).unwrap();
        assert_eq!(&decoded.project, e.project());
        assert_eq!(decoded.view, Some(view.as_slice()));
        assert_eq!(
            project_file::encode(&decoded.project, decoded.view).unwrap(),
            bytes
        );
        for version in [1, 22, 63, u32::MAX] {
            let mut bad = raw(e.project());
            bad["version"] = json!(version);
            assert!(Project::from_json(&bad.to_string()).is_err());
        }
    }
}

#[test]
fn malformed_origins_labels_and_inactive_schema_fields_are_rejected_on_load() {
    let mut e = text_scene();
    e.execute(Command::NewComposition).unwrap();
    for (field, value) in [
        ("start_frame", json!(i64::MIN)),
        ("start_frame", json!(100_000_001)),
        ("start_frame", json!(0.5)),
        ("label_index", json!(17)),
        ("label_index", json!(-1)),
        ("label_index", json!(1.5)),
    ] {
        let mut bad = raw(e.project());
        bad["version"] = json!(64);
        bad["other_compositions"]["1"]["layers"][0][field] = value;
        assert!(Project::from_json(&bad.to_string()).is_err(), "{field}");
    }
}

#[test]
fn automation_commands_are_source_preserving_and_commit_as_one_history_entry() {
    let mut e = text_scene();
    let before = e.project().clone();
    let mut draft = before.clone();
    draft
        .apply_automation_command(1, Command::SetLayerStart { id: 1, frame: 10 })
        .unwrap();
    draft
        .apply_automation_command(1, Command::SetLayerLabel { id: 1, index: 3 })
        .unwrap();
    assert_eq!(e.project(), &before);
    assert!(e.commit_automation_project(draft.clone()).unwrap());
    e.undo();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo());
    e.redo();
    assert_eq!(e.project(), &draft);
    let snapshot = draft.clone();
    assert!(
        draft
            .apply_automation_command(
                1,
                Command::Batch(vec![
                    Command::SetLayerLabel { id: 1, index: 4 },
                    Command::SetLayerStart {
                        id: 1,
                        frame: -1000
                    }
                ])
            )
            .is_err()
    );
    assert_eq!(draft, snapshot);
}

#[test]
fn image_asset_inserted_at_playhead_has_independent_origin_and_legacy_images_stay_sparse() {
    let mut e = Editor::default();
    e.execute(Command::ImportAsset {
        content: Content::Image {
            png: "ZmFrZQ==".into(),
        },
        width: 300.0,
        height: 100.0,
        name: "Still".into(),
        folder: None,
        frame: Some(25),
    })
    .unwrap();
    assert_eq!((layer(&e).start_frame(), layer(&e).in_frame()), (25, 25));
    assert_eq!(raw(e.project())["version"], json!(64));
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 40,
        end: 100,
    })
    .unwrap();
    assert_eq!(layer(&e).start_frame(), 25);
    e.execute(Command::AddAssetLayer { asset: 1, frame: 0 })
        .unwrap();
    assert_eq!(e.selected_layer().unwrap().start_frame(), 0);
    assert!(
        serde_json::to_value(e.selected_layer().unwrap())
            .unwrap()
            .get("start_frame")
            .is_none()
    );
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::AddCompositionLayer {
        composition: 1,
        frame: 12,
    })
    .unwrap();
    assert_eq!(e.selected_layer().unwrap().start_frame(), 12);
    assert!(
        serde_json::to_value(e.selected_layer().unwrap())
            .unwrap()
            .get("start_frame")
            .is_none()
    );
}

#[test]
fn zero_native_shift_preserves_declared_schema_source_and_redo() {
    let mut e = text_scene();
    set_label(&mut e, 4);
    e.undo();
    let before = e.project().clone();
    let receipt = e.context_generation();
    e.execute(Command::ShiftLayer { id: 1, delta: 0 }).unwrap();
    assert_eq!(e.project(), &before);
    assert_eq!(e.context_generation(), receipt);
    assert!(e.can_redo());
}

#[test]
fn uninterned_legacy_media_rejects_schema_raising_edit_without_mutation_or_migration() {
    let e = scene(video(3));
    // Deserialization models an already-resident old document. Normal file
    // loading already canonicalizes these legacy assets before editing.
    let mut value = serde_json::to_value(e.project()).unwrap();
    value["version"] = json!(5);
    value.as_object_mut().unwrap().remove("asset_library");
    value["composition"]["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("asset");
    let legacy: Project = serde_json::from_value(value).unwrap();
    legacy.validate_automation_project().unwrap();
    let mut editor = Editor::default();
    editor.replace_project(legacy.clone()).unwrap();
    editor.clear_history();
    set_start(&mut editor, 3);
    set_label(&mut editor, 0);
    assert_eq!(editor.project(), &legacy);
    rejected(&mut editor, Command::SetLayerStart { id: 1, frame: 8 });
    rejected(&mut editor, Command::SetLayerLabel { id: 1, index: 4 });
    let mut draft = legacy.clone();
    assert!(
        draft
            .apply_automation_command(1, Command::SetLayerLabel { id: 1, index: 4 })
            .is_err()
    );
    assert_eq!(draft, legacy);
    // No-op metadata setters inside an otherwise real automation batch must
    // not raise the floor or normalize legacy sources.
    draft
        .apply_automation_command(
            1,
            Command::Batch(vec![
                Command::RenameLayer {
                    id: 1,
                    name: "Renamed legacy footage".into(),
                },
                Command::SetLayerLabel { id: 1, index: 0 },
                Command::SetLayerStart { id: 1, frame: 3 },
            ]),
        )
        .unwrap();
    assert_eq!(raw(&draft)["version"], json!(5));
    assert_eq!(draft.composition().layer(1).unwrap().asset_id(), None);
    let mut expected = serde_json::to_value(&legacy).unwrap();
    expected["composition"]["layers"][0]["name"] = json!("Renamed legacy footage");
    assert_eq!(serde_json::to_value(&draft).unwrap(), expected);
    // A pure native timed move requires no new field, preserving the old floor.
    editor
        .execute(Command::ShiftLayer { id: 1, delta: 5 })
        .unwrap();
    assert_eq!(layer(&editor).start_frame(), 8);
    assert_eq!(raw(editor.project())["version"], json!(5));
}

#[test]
fn audio_time_remap_mask_channels_and_path_pose_clocks_move_together() {
    let mut content = video(3);
    if let Content::Video { audio, .. } = &mut content {
        *audio = Some(AudioMetadata {
            stream_index: 0,
            sample_rate: 48000,
            channels: 2,
            channel_layout: "stereo".into(),
            duration: 20.0,
            start_time: 0.0,
            file_offset: 0.0,
        });
    }
    let mut e = scene(content);
    let path = VectorPath {
        closed: true,
        vertices: vec![
            PathVertex::corner([0.0, 0.0]),
            PathVertex::corner([100.0, 0.0]),
            PathVertex::corner([50.0, 100.0]),
        ],
    };
    e.execute(Command::SetPathMasks {
        id: 1,
        masks: vec![PathMask {
            path: path.clone(),
            ..Default::default()
        }],
    })
    .unwrap();
    e.execute(Command::SetTimeRemap {
        id: 1,
        enabled: true,
    })
    .unwrap();
    let mut paths = vec![PropertyPath::Path(PathTarget::Mask(1))];
    paths.extend(AudioParam::ALL.into_iter().map(PropertyPath::Audio));
    paths.extend(
        MaskParam::ALL
            .into_iter()
            .map(|parameter| PropertyPath::Mask { mask: 1, parameter }),
    );
    for property in &paths {
        e.execute(Command::EditTrack {
            id: 1,
            property: *property,
            edit: TrackEdit::ToggleAnimation { frame: 25 },
        })
        .unwrap();
        e.execute(Command::EditTrack {
            id: 1,
            property: *property,
            edit: TrackEdit::ToggleKey { frame: 60 },
        })
        .unwrap();
    }
    let mut second_pose = path;
    second_pose.vertices[1].position[0] += 25.0;
    e.execute(Command::EditPath {
        id: 1,
        target: PathTarget::Mask(1),
        frame: 60,
        path: second_pose,
    })
    .unwrap();
    paths.push(PropertyPath::TimeRemap);
    let before = layer(&e).clone();
    set_start(&mut e, 13);
    for property in paths {
        let expected: std::collections::BTreeMap<_, _> = before
            .track(property)
            .unwrap()
            .keys()
            .iter()
            .map(|(frame, key)| (frame + 10, key.clone()))
            .collect();
        assert_eq!(layer(&e).track(property).unwrap().keys(), &expected);
    }
    assert_eq!(
        layer(&e).audio_source_time(60, 30),
        before.audio_source_time(50, 30)
    );
    assert_eq!(layer(&e).source_time(60, 30), before.source_time(50, 30));
    assert_eq!(
        serde_json::to_value(&layer(&e).path_masks()[0].animation).unwrap()["poses"],
        serde_json::to_value(&before.path_masks()[0].animation).unwrap()["poses"]
    );
}
