use super::*;

// These schema48 regressions intentionally exercise the original seven paint tracks.
const PAINT_PARAMETERS: [TextParam; 7] = [
    TextParam::FillRed,
    TextParam::FillGreen,
    TextParam::FillBlue,
    TextParam::StrokeRed,
    TextParam::StrokeGreen,
    TextParam::StrokeBlue,
    TextParam::StrokeWidth,
];

fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: "Title 한글\nSecond line".into(),
            font_size: 48.,
        },
        width: 500.,
        height: 200.,
        name: "Text".into(),
    })
    .unwrap();
    e.execute(Command::SetColor {
        id: 1,
        color: 0x102030,
    })
    .unwrap();
    e.execute(Command::SetTextStyle {
        id: 1,
        style: TextStyle {
            stroke_color: 0x204060,
            stroke_width: 12.,
            stroke_enabled: true,
            paragraph: true,
            ..Default::default()
        },
    })
    .unwrap();
    e
}
fn edit(e: &mut Editor, parameter: TextParam, edit: TrackEdit) {
    e.execute(Command::EditTrack {
        id: 1,
        property: PropertyPath::Text(parameter),
        edit,
    })
    .unwrap();
}
fn color(e: &Editor, paint: TextPaint, frame: Frame) -> u32 {
    e.project()
        .composition()
        .layer(1)
        .unwrap()
        .text_color_at(paint, frame)
        .unwrap()
}
fn toggle(e: &mut Editor, paint: TextPaint, frame: Frame) {
    let command = e
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .text_color_animation_command(paint, frame)
        .unwrap();
    e.execute(command).unwrap();
}
fn set(e: &mut Editor, paint: TextPaint, rgb: u32, frame: Frame) {
    let command = e
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .text_color_command(paint, rgb, frame)
        .unwrap();
    e.execute(command).unwrap();
}
fn assert_rejected(e: &mut Editor, command: Command) {
    let current = e.current.clone();
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    assert!(e.execute(command).is_err());
    assert_eq!(e.current, current);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
}

#[test]
fn text_animation_static_legacy_and_sparse_defaults_do_not_upgrade_on_load() {
    let mut legacy = Editor::default();
    legacy
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Legacy".into(),
                font_size: 24.,
            },
            width: 200.,
            height: 100.,
            name: "Legacy".into(),
        })
        .unwrap();
    assert_eq!(legacy.project().version, 3);
    let legacy_json = legacy.project().to_json().unwrap();
    assert!(!legacy_json.contains("text_parameters"));
    assert!(!legacy_json.contains("text_style"));
    assert_eq!(Project::from_json(&legacy_json).unwrap(), *legacy.project());
    assert_eq!(
        project_file::decode(&project_file::encode(legacy.project(), None).unwrap())
            .unwrap()
            .project,
        *legacy.project()
    );
    let mut e = scene();
    let old = e.project().clone();
    assert!(old.version < 48);
    let json = old.to_json().unwrap();
    assert!(!json.contains("text_parameters"));
    assert_eq!(Project::from_json(&json).unwrap(), old);
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["composition"]["layers"][0]["text_parameters"] = serde_json::json!({});
    assert_eq!(Project::from_json(&value.to_string()).unwrap(), old);
    assert_eq!(color(&e, TextPaint::Fill, 40), 0x102030);
    assert_eq!(color(&e, TextPaint::Stroke, 40), 0x204060);
    set(&mut e, TextPaint::Fill, 0x556677, 40);
    set(&mut e, TextPaint::Stroke, 0x123456, 40);
    assert!(e.selected_layer().unwrap().text_parameters.is_empty());
    assert!(e.project().version < 48);
    let current = e.current.clone();
    let history = e.undo.clone();
    set(&mut e, TextPaint::Fill, 0x556677, 40);
    set(&mut e, TextPaint::Stroke, 0x123456, 40);
    assert_eq!(e.current, current);
    assert_eq!(e.undo, history);
}

#[test]
fn text_animation_rgb_groups_have_one_undo_preserve_base_and_roundtrip_native_v1() {
    let mut e = scene();
    let style = e.selected_layer().unwrap().text_style();
    for (paint, base, end, middle) in [
        (TextPaint::Fill, 0x102030, 0x90a0b0, 0x506070),
        (TextPaint::Stroke, 0x204060, 0x6080a0, 0x406080),
    ] {
        let before = e.project().clone();
        let undo = e.undo.len();
        toggle(&mut e, paint, 0);
        assert_eq!(e.undo.len(), undo + 1);
        let enabled = e.project().clone();
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &enabled);
        set(&mut e, paint, end, 40);
        assert_eq!(color(&e, paint, 0), base);
        assert_eq!(color(&e, paint, 20), middle);
        assert_eq!(color(&e, paint, 40), end);
        assert_eq!(e.selected_layer().unwrap().text_style(), style);
        assert_eq!(e.selected_layer().unwrap().color(), 0x102030);
        assert_eq!(e.project().version, 48);
        let edited = e.project().clone();
        assert_eq!(
            Project::from_json(&edited.to_json().unwrap()).unwrap(),
            edited
        );
        let bytes = project_file::encode(&edited, None).unwrap();
        assert_eq!(&bytes[8..10], &[1, 0]);
        assert_eq!(project_file::decode(&bytes).unwrap().project, edited);
        e.undo();
        assert_eq!(e.project(), &enabled);
        e.redo();
        assert_eq!(e.project(), &edited);
        toggle(&mut e, paint, 20);
        assert!(!e.selected_layer().unwrap().text_color_animated(paint));
        assert_eq!(color(&e, paint, 200), middle);
        e.undo();
        assert_eq!(e.project(), &edited);
    }
}

#[test]
fn text_animation_all_seven_tracks_linear_hold_and_bezier_overshoot_are_bounded() {
    for parameter in PAINT_PARAMETERS {
        let mut e = scene();
        let path = PropertyPath::Text(parameter);
        let (min, max) = parameter.bounds();
        edit(
            &mut e,
            parameter,
            TrackEdit::Value {
                frame: 0,
                value: max / 3.,
            },
        );
        edit(&mut e, parameter, TrackEdit::ToggleAnimation { frame: 0 });
        edit(
            &mut e,
            parameter,
            TrackEdit::Value {
                frame: 40,
                value: 2. * max / 3.,
            },
        );
        assert_eq!(
            e.selected_layer().unwrap().track_value(path, 20),
            Some(max / 2.)
        );
        edit(
            &mut e,
            parameter,
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Hold,
            },
        );
        assert_eq!(
            e.selected_layer().unwrap().track_value(path, 20),
            Some(max / 3.)
        );
        for direction in [-2., 3.] {
            edit(
                &mut e,
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
            let expected = if direction < 0. { min } else { max };
            assert_eq!(
                e.selected_layer().unwrap().track_value(path, 20),
                Some(expected)
            );
            let before = e.project().clone();
            for operation in [
                TrackEdit::ToggleKey { frame: 20 },
                TrackEdit::ToggleAnimation { frame: 20 },
            ] {
                edit(&mut e, parameter, operation);
                assert_eq!(
                    e.selected_layer()
                        .unwrap()
                        .track(path)
                        .unwrap()
                        .value_at(20),
                    expected
                );
                e.undo();
                assert_eq!(e.project(), &before);
            }
        }
    }
}

#[test]
fn text_animation_partial_channel_keys_seed_rgb_baselines_and_toggle_only_existing_keys() {
    for paint in [TextPaint::Fill, TextPaint::Stroke] {
        let mut e = scene();
        let base = color(&e, paint, 0);
        let red = paint.channels()[0];
        edit(&mut e, red, TrackEdit::ToggleKey { frame: 0 });
        let before = e.project().clone();
        toggle(&mut e, paint, 20);
        assert_eq!(e.selected_layer().unwrap().text_parameters.len(), 1);
        assert_eq!(color(&e, paint, 20), base);
        e.undo();
        assert_eq!(e.project(), &before);
        set(&mut e, paint, 0x90a0b0, 40);
        assert_eq!(color(&e, paint, 0), base);
        for parameter in paint.channels() {
            let track = e
                .selected_layer()
                .unwrap()
                .track(PropertyPath::Text(parameter))
                .unwrap();
            assert_eq!(track.keys.len(), 2);
            assert!(track.keys.contains_key(&0));
            assert!(track.keys.contains_key(&40));
        }
        e.undo();
        assert_eq!(e.project(), &before);
    }
}

#[test]
fn text_animation_invalid_bounds_frames_and_batches_are_atomic() {
    let mut e = scene();
    for parameter in PAINT_PARAMETERS {
        for value in [-1., parameter.bounds().1 + 1., f64::NAN, f64::INFINITY] {
            assert_rejected(
                &mut e,
                Command::Batch(vec![
                    Command::RenameLayer {
                        id: 1,
                        name: "Must not change".into(),
                    },
                    Command::EditText {
                        id: 1,
                        parameter,
                        edit: TrackEdit::Value { frame: 0, value },
                    },
                ]),
            );
        }
        for edit in [
            TrackEdit::ToggleAnimation { frame: u32::MAX },
            TrackEdit::Keyframe {
                from: 20,
                to: 30,
                value: 1.,
            },
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Hold,
            },
        ] {
            assert_rejected(
                &mut e,
                Command::EditText {
                    id: 1,
                    parameter,
                    edit,
                },
            );
        }
    }
    e.execute(Command::ToggleLocked(1)).unwrap();
    assert!(
        e.selected_layer()
            .unwrap()
            .text_color_command(TextPaint::Fill, 0, 0)
            .is_err()
    );
    assert!(
        e.selected_layer()
            .unwrap()
            .text_color_animation_command(TextPaint::Stroke, 0)
            .is_err()
    );
    assert_rejected(
        &mut e,
        Command::EditText {
            id: 1,
            parameter: TextParam::FillRed,
            edit: TrackEdit::ToggleKey { frame: 0 },
        },
    );
    e.execute(Command::AddRectangle).unwrap();
    let layer = e.selected_layer().unwrap();
    assert_eq!(layer.text_value_at(TextParam::StrokeWidth, 0), None);
    assert_eq!(layer.text_color_at(TextPaint::Fill, 0), None);
    assert_eq!(
        layer.track_value(PropertyPath::Text(TextParam::FillRed), 0),
        None
    );
    assert!(layer.text_color_command(TextPaint::Fill, 0, 0).is_err());
    assert_rejected(
        &mut e,
        Command::EditText {
            id: 2,
            parameter: TextParam::FillRed,
            edit: TrackEdit::ToggleKey { frame: 0 },
        },
    );
}

fn animated_scene() -> Editor {
    let mut e = scene();
    for parameter in PAINT_PARAMETERS {
        edit(&mut e, parameter, TrackEdit::ToggleKey { frame: 10 });
        edit(
            &mut e,
            parameter,
            TrackEdit::Value {
                frame: 40,
                value: 120.,
            },
        );
        e.execute(Command::SetTemporalHandle {
            id: 1,
            property: PropertyPath::Text(parameter),
            frame: 10,
            incoming: false,
            handle: TemporalHandle {
                slope: 2.,
                influence: 0.3,
            },
        })
        .unwrap();
        e.execute(Command::SetTemporalMode {
            id: 1,
            property: PropertyPath::Text(parameter),
            frame: 40,
            mode: TemporalMode::Auto,
        })
        .unwrap();
    }
    e
}

#[test]
fn text_animation_generic_copy_move_delete_scale_and_temporal_metadata() {
    let mut e = animated_scene();
    let paths: Vec<_> = PAINT_PARAMETERS
        .into_iter()
        .map(PropertyPath::Text)
        .collect();
    let refs = |frame| {
        paths
            .iter()
            .map(|&property| KeyRef {
                id: 1,
                property,
                frame,
            })
            .collect::<Vec<_>>()
    };
    let original = e.selected_layer().unwrap().text_parameters.clone();
    let copies: Vec<_> = paths
        .iter()
        .map(|&p| e.selected_layer().unwrap().copy_key(p, 10).unwrap())
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
    for parameter in PAINT_PARAMETERS {
        let t = &e.selected_layer().unwrap().text_parameters[&parameter];
        assert_eq!(t.keys[&70], original[&parameter].keys[&10]);
    }
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
    for parameter in PAINT_PARAMETERS {
        let t = &e.selected_layer().unwrap().text_parameters[&parameter];
        let mut expected = original[&parameter].keys[&10].clone();
        expected.value *= 0.5;
        expected.temporal.outgoing.as_mut().unwrap().slope *= 0.5 / 1.5;
        assert_eq!(t.keys[&105], expected);
        assert_eq!(t.keys[&40], original[&parameter].keys[&40]);
    }
    let before = e.project().clone();
    assert_rejected(
        &mut e,
        Command::ScaleKeys {
            keys: refs(105),
            scale: KeyScale {
                time_origin: 0.,
                time_scale: 1.,
                value_origin: 0.,
                value_scale: 1000.,
            },
        },
    );
    assert_eq!(e.project(), &before);
    assert_rejected(
        &mut e,
        Command::MoveKeys {
            keys: vec![
                refs(10)[0],
                KeyRef {
                    frame: 99,
                    ..refs(10)[1]
                },
            ],
            delta: 2,
        },
    );
    e.execute(Command::DeleteKeys(refs(105))).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, original);
    let value = e
        .selected_layer()
        .unwrap()
        .text_value_at(TextParam::FillRed, 40)
        .unwrap();
    e.execute(Command::DeleteKeys(refs(10))).unwrap();
    e.execute(Command::DeleteKeys(refs(40))).unwrap();
    assert_eq!(
        e.selected_layer()
            .unwrap()
            .text_value_at(TextParam::FillRed, 90),
        Some(value)
    );
    assert!(
        e.selected_layer()
            .unwrap()
            .text_parameters
            .values()
            .all(|t| t.keys.is_empty())
    );
    e.execute(Command::AddContent {
        content: Content::Text {
            text: "Target".into(),
            font_size: 24.,
        },
        width: 100.,
        height: 100.,
        name: "Target".into(),
    })
    .unwrap();
    e.execute(Command::PasteKeys {
        keys: copies.clone(),
        frame: 20,
        target: Some(2),
    })
    .unwrap();
    for parameter in PAINT_PARAMETERS {
        assert_eq!(
            e.selected_layer().unwrap().text_parameters[&parameter].keys[&20],
            original[&parameter].keys[&10]
        );
    }
    e.execute(Command::AddRectangle).unwrap();
    assert_rejected(
        &mut e,
        Command::PasteKeys {
            keys: copies,
            frame: 20,
            target: Some(3),
        },
    );
}

#[test]
fn text_animation_source_box_style_font_edits_preserve_tracks_and_incompatible_content_rejects() {
    let mut e = animated_scene();
    let tracks = e.selected_layer().unwrap().text_parameters.clone();
    let mut style = e.selected_layer().unwrap().text_style();
    style.font_family = "Missing Font".into();
    style.font_face = "Missing-Bold".into();
    style.weight = 700;
    style.italic = true;
    style.leading = 1.8;
    style.tracking = 55.;
    style.align = TextAlign::Center;
    style.fill_enabled = false;
    style.stroke_enabled = false;
    style.stroke_color = 0xffffff;
    style.stroke_width = 100.;
    style.stroke_over_fill = true;
    style.stroke_join = TextStrokeJoin::Round;
    e.execute(Command::SetTextStyle {
        id: 1,
        style: style.clone(),
    })
    .unwrap();
    e.execute(Command::SetContent {
        id: 1,
        content: Content::Text {
            text: "Changed source\n字".into(),
            font_size: 72.,
        },
    })
    .unwrap();
    e.execute(Command::SetTextBox {
        id: 1,
        width: 600.,
        height: 300.,
    })
    .unwrap();
    let from = TextFont::of(&style);
    let to = TextFont::of(&TextStyle::default());
    e.execute(Command::ReplaceTextFont { from, to }).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, tracks);
    for content in [
        Content::Rectangle,
        Content::Null,
        Content::Shape(Shape::default()),
    ] {
        assert_rejected(&mut e, Command::SetContent { id: 1, content });
    }
    let after = e.project().clone();
    assert_eq!(
        Project::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
}

#[test]
fn text_animation_duplicate_split_shift_paste_across_fps_and_precompose_preserve_tracks() {
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
    for parameter in PAINT_PARAMETERS {
        let t = &e
            .project()
            .composition()
            .layer(split)
            .unwrap()
            .text_parameters[&parameter];
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
    for parameter in PAINT_PARAMETERS {
        let t = &pasted_tracks[&parameter];
        let mut expected = tracks[&parameter].keys[&10].clone();
        expected.temporal.outgoing.as_mut().unwrap().slope *= 0.5;
        assert_eq!(t.keys[&30], expected);
        assert_eq!(t.keys[&90], tracks[&parameter].keys[&40]);
    }
    e.execute(Command::DuplicateComposition).unwrap();
    assert_eq!(e.selected_layer().unwrap().text_parameters, pasted_tracks);
    e.activate_composition(2).unwrap();
    e.execute(Command::Precompose {
        layers: vec![pasted],
        name: "Text precomp".into(),
    })
    .unwrap();
    let Content::Composition { composition, .. } = e.selected_layer().unwrap().content() else {
        panic!("precomp");
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
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
}

#[test]
fn text_animation_schema_disguise_inactive_compositions_invalid_keys_and_replacement_are_rejected()
{
    let mut e = animated_scene();
    e.execute(Command::NewComposition).unwrap();
    let original = e.project().clone();
    assert_eq!(original.version, 48);
    let value = serde_json::to_value(&original).unwrap();
    let mut malformed = Vec::new();
    let mut old = value.clone();
    old["version"] = 47.into();
    malformed.push(old);
    let mut nontext = value.clone();
    nontext["other_compositions"]["1"]["layers"][0]["content"] = "Rectangle".into();
    nontext["other_compositions"]["1"]["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("text_style");
    malformed.push(nontext);
    let mut unknown = value.clone();
    unknown["other_compositions"]["1"]["layers"][0]["text_parameters"]["UnknownTextParameter"] =
        serde_json::json!({"value": 30., "keys": {}});
    malformed.push(unknown);
    for field in ["value", "key", "time", "interpolation", "temporal"] {
        let mut bad = value.clone();
        let track = &mut bad["other_compositions"]["1"]["layers"][0]["text_parameters"]["FillRed"];
        match field {
            "value" => track["value"] = (-1.).into(),
            "key" => track["keys"]["10"]["value"] = 256.into(),
            "time" => {
                let key = track["keys"]["10"].clone();
                track["keys"]["150"] = key;
            }
            "interpolation" => {
                track["keys"]["10"]["interpolation"] =
                    serde_json::json!({"Bezier":{"x1":-1.,"y1":0.,"x2":1.,"y2":1.}})
            }
            _ => track["keys"]["10"]["temporal"]["outgoing"]["influence"] = 2.into(),
        }
        malformed.push(bad);
    }
    for bad in malformed {
        assert!(Project::from_json(&bad.to_string()).is_err());
        assert!(document::decode_native(bad.clone(), BTreeMap::new()).is_err());
        if let Ok(candidate) = serde_json::from_value::<Project>(bad) {
            let current = e.current.clone();
            let undo = e.undo.clone();
            let redo = e.redo.clone();
            assert!(e.replace_project(candidate).is_err());
            assert_eq!(e.current, current);
            assert_eq!(e.undo, undo);
            assert_eq!(e.redo, redo);
        }
    }
    assert_eq!(e.project(), &original);
    let native = project_file::encode(&original, None).unwrap();
    assert_eq!(project_file::decode(&native).unwrap().project, original);
}

#[test]
fn text_animation_ten_thousand_key_limit_is_checked_before_history_commit() {
    let mut project = scene().project().clone();
    project.version = 48;
    project.composition.duration = 20_000;
    let mut track = AnimatedProperty::new(10.);
    track.keys = (0..10_000)
        .map(|frame| {
            (
                frame,
                Keyframe {
                    value: 10.,
                    interpolation: Interpolation::Linear,
                    temporal: TemporalHandles::default(),
                },
            )
        })
        .collect();
    project.composition.layers[0]
        .text_parameters
        .insert(TextParam::FillRed, track);
    project.validate().unwrap();
    let mut e = Editor::default();
    e.replace_project(project).unwrap();
    assert_rejected(
        &mut e,
        Command::EditText {
            id: 1,
            parameter: TextParam::FillRed,
            edit: TrackEdit::ToggleKey { frame: 10_000 },
        },
    );
    edit(
        &mut e,
        TextParam::FillRed,
        TrackEdit::Value {
            frame: 50,
            value: 20.,
        },
    );
    assert_eq!(
        e.selected_layer().unwrap().text_parameters[&TextParam::FillRed]
            .keys
            .len(),
        10_000
    );
}

#[test]
fn text_animation_metadata_budget_accounts_for_sparse_tracks_atomically() {
    const BUDGET: usize = 16 * 1024 * 1024;
    let mut project = scene().project().clone();
    let mut template = project.composition.layers[0].clone();
    template.content = Content::Text {
        text: "\u{0001}".repeat(16_384),
        font_size: 48.,
    };
    project.composition.layers = (1..=180)
        .map(|id| {
            let mut l = template.clone();
            l.id = id;
            l
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
    let mut e = Editor::default();
    e.replace_project(project).unwrap();
    let current = e.current.clone();
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    let error = e
        .execute(Command::EditText {
            id: 1,
            parameter: TextParam::FillRed,
            edit: TrackEdit::ToggleKey { frame: 0 },
        })
        .unwrap_err();
    assert!(error.contains("metadata exceeds 16 MiB"), "{error}");
    assert_eq!(e.current, current);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
}

#[test]
fn text_animation_color_draft_helpers_are_pure_and_static_override_baselines_are_preserved() {
    let mut e = scene();
    edit(
        &mut e,
        TextParam::FillGreen,
        TrackEdit::Value {
            frame: 0,
            value: 80.,
        },
    );
    edit(
        &mut e,
        TextParam::FillRed,
        TrackEdit::ToggleKey { frame: 10 },
    );
    let current = e.current.clone();
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    let command = e
        .selected_layer()
        .unwrap()
        .text_color_command(TextPaint::Fill, 0x90a0b0, 40)
        .unwrap();
    let _cancelled = e
        .selected_layer()
        .unwrap()
        .text_color_animation_command(TextPaint::Fill, 20)
        .unwrap();
    assert_eq!(e.current, current);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
    assert!(
        e.selected_layer()
            .unwrap()
            .text_color_command(TextPaint::Fill, 0x1000000, 0)
            .is_err()
    );
    e.execute(command).unwrap();
    assert_eq!(color(&e, TextPaint::Fill, 10), 0x105030);
    assert_eq!(color(&e, TextPaint::Fill, 25), 0x507870);
    assert_eq!(e.selected_layer().unwrap().color(), 0x102030);
    assert_eq!(e.undo.len(), undo.len() + 1);
    e.undo();
    assert_eq!(e.current, current);
}

#[test]
fn text_animation_untouched_base_value_edits_are_exact_noops_without_materializing_tracks() {
    for parameter in PAINT_PARAMETERS {
        let mut e = scene();
        // Historical higher schemas must survive semantic no-ops unchanged.
        let mut imported = e.project().clone();
        imported.version = 47;
        e.replace_project(imported).unwrap();
        e.execute(Command::RenameLayer {
            id: 1,
            name: "Redo me".into(),
        })
        .unwrap();
        e.undo();
        let base = e
            .selected_layer()
            .unwrap()
            .text_value_at(parameter, 0)
            .unwrap();
        let current = e.current.clone();
        let undo = e.undo.clone();
        let redo = e.redo.clone();
        let direct = Command::EditText {
            id: 1,
            parameter,
            edit: TrackEdit::Value {
                frame: 20,
                value: base,
            },
        };
        let generic = Command::EditTrack {
            id: 1,
            property: PropertyPath::Text(parameter),
            edit: TrackEdit::Value {
                frame: 40,
                value: base,
            },
        };
        for command in [
            direct.clone(),
            generic.clone(),
            Command::Batch(vec![direct, Command::Batch(vec![generic])]),
        ] {
            e.execute(command).unwrap();
            assert_eq!(e.current, current);
            assert_eq!(e.undo, undo);
            assert_eq!(e.redo, redo);
            assert!(e.selected_layer().unwrap().text_parameters.is_empty());
        }
        assert_rejected(
            &mut e,
            Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::Value {
                    frame: u32::MAX,
                    value: base,
                },
            },
        );
        edit(
            &mut e,
            parameter,
            TrackEdit::Value {
                frame: 20,
                value: base + 1.,
            },
        );
        assert_eq!(e.selected_layer().unwrap().text_parameters.len(), 1);
        assert_eq!(e.project().version, 48);
        assert!(e.redo.is_empty());
        e.undo();
        assert_eq!(e.current, current);
        edit(&mut e, parameter, TrackEdit::ToggleAnimation { frame: 20 });
        assert_eq!(
            e.selected_layer().unwrap().text_parameters[&parameter].keys[&20].value,
            base
        );
        assert_eq!(e.project().version, 48);
    }
}
