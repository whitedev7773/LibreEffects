use super::*;

#[path = "gradient_colors_multikey_tests.rs"]
mod multikey;

fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::ShapeContents(ShapeContents::default()),
        width: 200.,
        height: 100.,
        name: "Gradient".into(),
    })
    .unwrap();
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
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 0,
        end: 100,
    })
    .unwrap();
    e.clear_history();
    e
}
fn node(project: &Project) -> &ContentsNode {
    let Content::ShapeContents(c) = &project.composition.layer(1).unwrap().content else {
        panic!()
    };
    c.node(1).unwrap()
}
fn node_mut(project: &mut Project) -> &mut ContentsNode {
    let Content::ShapeContents(c) = &mut project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == 1)
        .unwrap()
        .content
    else {
        panic!()
    };
    c.node_mut(1).unwrap()
}
fn colors(project: &Project, frame: Frame) -> GradientColors {
    let n = node(project);
    n.kind.gradient().unwrap().colors_at(n, frame)
}
fn command(edit: GradientColorsEdit) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::GradientColors { item: 1, edit },
    }
}
fn edit(e: &mut Editor, edit: GradientColorsEdit) {
    e.execute(command(edit)).unwrap();
}
fn enable(e: &mut Editor, frame: Frame) {
    edit(
        e,
        GradientColorsEdit::SetAnimation {
            frame,
            enabled: true,
        },
    );
}
fn reject(e: &mut Editor, command: Command) {
    let (before, undo, redo) = (e.current.clone(), e.undo.clone(), e.redo.clone());
    assert!(e.execute(command).is_err());
    assert_eq!(e.current, before);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
}
fn gradient_json(project: &Project) -> serde_json::Value {
    serde_json::to_value(node(project).kind.gradient().unwrap()).unwrap()
}

#[test]
fn gradient_colors_legacy_absence_and_noops_are_byte_source_and_history_exact() {
    let mut e = scene();
    let before = e.project().clone();
    let json = before.to_json().unwrap();
    assert!(!json.contains("colors_animation"));
    assert_eq!(
        gradient_json(&before),
        serde_json::json!({"radial":false,"colors":[1,2],"opacities":[3,4],"next_stop":5})
    );
    assert_eq!(before.version, 45);
    enable(&mut e, 30);
    e.undo();
    assert!(e.can_redo());
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    for action in [
        GradientColorsEdit::SetAnimation {
            frame: 30,
            enabled: false,
        },
        GradientColorsEdit::Set {
            frame: 30,
            colors: colors(&before, 30),
        },
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::Red(1),
            value: 0.,
        },
    ] {
        edit(&mut e, action);
    }
    assert_eq!(e.project(), &before);
    assert_eq!(e.project().to_json().unwrap(), json);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
    e.redo();
    assert_eq!(e.project().version, 54);
}

#[test]
fn gradient_colors_hold_snapshots_change_topology_without_touching_legacy_base() {
    let mut e = scene();
    let base = node(e.project()).clone();
    let first = colors(e.project(), 0);
    enable(&mut e, 10);
    edit(
        &mut e,
        GradientColorsEdit::AddStop {
            frame: 30,
            opacity: false,
            position: 25.,
        },
    );
    edit(
        &mut e,
        GradientColorsEdit::Color {
            frame: 30,
            stop: 5,
            color: 0x123456,
        },
    );
    edit(
        &mut e,
        GradientColorsEdit::AddStop {
            frame: 60,
            opacity: true,
            position: 75.,
        },
    );
    edit(
        &mut e,
        GradientColorsEdit::Value {
            frame: 60,
            parameter: GradientParam::Opacity(6),
            value: 40.,
        },
    );
    edit(
        &mut e,
        GradientColorsEdit::RemoveStop { frame: 90, stop: 1 },
    );
    let second = colors(e.project(), 30);
    let third = colors(e.project(), 60);
    let fourth = colors(e.project(), 90);
    for frame in [0, 9, 10, 11, 29] {
        assert_eq!(colors(e.project(), frame), first);
    }
    for frame in [30, 31, 59] {
        assert_eq!(colors(e.project(), frame), second);
    }
    for frame in [60, 61, 89] {
        assert_eq!(colors(e.project(), frame), third);
    }
    for frame in [90, 91, 149] {
        assert_eq!(colors(e.project(), frame), fourth);
    }
    assert_eq!(second.colors.len(), 3);
    assert_eq!(second.opacities.len(), 2);
    assert_eq!(second.color_at(5), Some(0x123456));
    assert_eq!(third.opacities.len(), 3);
    assert_eq!(third.value(GradientParam::Opacity(6)), Some(40.));
    assert_eq!(
        fourth.colors.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![2, 5]
    );
    let n = node(e.project());
    let g = n.kind.gradient().unwrap();
    assert_eq!(n.parameters, base.parameters);
    assert_eq!(g.colors, vec![1, 2]);
    assert_eq!(g.opacities, vec![3, 4]);
    assert_eq!(g.next_stop, 7);
    assert_eq!(g.colors_animation().unwrap().keys().len(), 4);
}

#[test]
fn gradient_colors_disable_and_last_key_remove_bake_sample_with_exact_undo_redo() {
    let mut e = scene();
    enable(&mut e, 10);
    edit(
        &mut e,
        GradientColorsEdit::AddStop {
            frame: 30,
            opacity: false,
            position: 50.,
        },
    );
    edit(
        &mut e,
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::Red(5),
            value: 17.25,
        },
    );
    let before = e.project().clone();
    let sample = colors(&before, 40);
    edit(
        &mut e,
        GradientColorsEdit::SetAnimation {
            frame: 40,
            enabled: false,
        },
    );
    let baked = e.project().clone();
    assert!(
        node(&baked)
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .is_none()
    );
    assert_eq!(baked.version, 54);
    for f in [0, 40, 149] {
        assert_eq!(colors(&baked, f), sample);
    }
    assert!(node(&baked).parameters.values().all(|t| t.keys.is_empty()));
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project(), &baked);
    enable(&mut e, 70);
    edit(&mut e, GradientColorsEdit::ToggleKey { frame: 70 });
    assert_eq!(e.project(), &baked);
}

#[test]
fn gradient_colors_noops_keep_redundant_keys_and_redo_and_allocator_only_drafts() {
    let mut e = scene();
    enable(&mut e, 10);
    edit(&mut e, GradientColorsEdit::ToggleKey { frame: 20 });
    edit(
        &mut e,
        GradientColorsEdit::Color {
            frame: 50,
            stop: 1,
            color: 0xffffff,
        },
    );
    e.undo();
    let (current, undo, redo) = (e.current.clone(), e.undo.clone(), e.redo.clone());
    let sample = colors(e.project(), 30);
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 30,
            colors: sample.clone(),
        },
    );
    edit(&mut e, GradientColorsEdit::MoveKey { from: 20, to: 20 });
    edit(
        &mut e,
        GradientColorsEdit::SetAnimation {
            frame: 30,
            enabled: true,
        },
    );
    let mut draft = node(e.project())
        .kind
        .gradient()
        .unwrap()
        .sampled_node(node(e.project()), 30);
    ShapeGradient::add_stop(&mut draft, false, 50., 30).unwrap();
    ShapeGradient::remove_stop(&mut draft, 5).unwrap();
    assert_eq!(draft.kind.gradient().unwrap().next_stop, 6);
    let from_draft = draft.kind.gradient().unwrap().colors_at(&draft, 30);
    assert_eq!(from_draft, sample);
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 30,
            colors: from_draft,
        },
    );
    assert_eq!(e.current, current);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
}

#[test]
fn gradient_colors_rejects_legacy_animated_promotion_and_all_frozen_scalar_routes() {
    let mut e = scene();
    let p = ContentsParam::Gradient(GradientParam::Red(1));
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item: 1,
            parameter: p,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    })
    .unwrap();
    for action in [
        GradientColorsEdit::SetAnimation {
            frame: 30,
            enabled: true,
        },
        GradientColorsEdit::ToggleKey { frame: 30 },
        GradientColorsEdit::AddStop {
            frame: 30,
            opacity: false,
            position: 50.,
        },
    ] {
        reject(&mut e, command(action));
    }
    e.undo();
    enable(&mut e, 30);
    let path = PropertyPath::Contents {
        item: 1,
        parameter: p,
    };
    let layer = e.project().composition.layer(1).unwrap();
    assert!(layer.track(path).is_none());
    assert!(layer.track_value(path, 30).is_none());
    assert!(!layer.track_paths().contains(&path));
    for command in [
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 1,
                parameter: p,
                edit: TrackEdit::Value {
                    frame: 30,
                    value: 22.,
                },
            },
        },
        Command::EditTrack {
            id: 1,
            property: path,
            edit: TrackEdit::ToggleKey { frame: 30 },
        },
        Command::Contents {
            id: 1,
            edit: ContentsEdit::AddGradientStop {
                item: 1,
                opacity: false,
                position: 50.,
                frame: 30,
            },
        },
        Command::Contents {
            id: 1,
            edit: ContentsEdit::RemoveGradientStop { item: 1, stop: 1 },
        },
    ] {
        reject(&mut e, command);
    }
    let endpoint = ContentsParam::Gradient(GradientParam::EndX);
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item: 1,
            parameter: endpoint,
            edit: TrackEdit::Value {
                frame: 30,
                value: 175.,
            },
        },
    })
    .unwrap();
    assert_eq!(node(e.project()).value_at(endpoint, 0), 175.);
}

#[test]
fn gradient_colors_edit_validation_is_atomic_including_locked_missing_and_batches() {
    let mut e = scene();
    enable(&mut e, 10);
    edit(&mut e, GradientColorsEdit::ToggleKey { frame: 20 });
    for action in [
        GradientColorsEdit::SetAnimation {
            frame: 150,
            enabled: false,
        },
        GradientColorsEdit::ToggleKey { frame: u32::MAX },
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::Red(1),
            value: f64::NAN,
        },
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::Red(1),
            value: 256.,
        },
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::Opacity(3),
            value: -1.,
        },
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::ColorMidpoint(1),
            value: 100.,
        },
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::StartX,
            value: 1.,
        },
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::Red(999),
            value: 1.,
        },
        GradientColorsEdit::Color {
            frame: 30,
            stop: 1,
            color: 0x1000000,
        },
        GradientColorsEdit::AddStop {
            frame: 30,
            opacity: true,
            position: f64::INFINITY,
        },
        GradientColorsEdit::RemoveStop { frame: 30, stop: 1 },
        GradientColorsEdit::RemoveStop {
            frame: 30,
            stop: 999,
        },
        GradientColorsEdit::MoveKey { from: 11, to: 40 },
        GradientColorsEdit::MoveKey { from: 10, to: 20 },
        GradientColorsEdit::MoveKey { from: 10, to: 150 },
    ] {
        reject(&mut e, command(action));
    }
    reject(
        &mut e,
        Command::Batch(vec![
            command(GradientColorsEdit::Color {
                frame: 30,
                stop: 1,
                color: 17,
            }),
            command(GradientColorsEdit::RemoveStop {
                frame: 30,
                stop: 999,
            }),
        ]),
    );
    e.execute(Command::ToggleLocked(1)).unwrap();
    reject(&mut e, command(GradientColorsEdit::ToggleKey { frame: 30 }));
}

#[test]
fn gradient_colors_whole_snapshot_validates_ids_roles_values_and_independent_limits() {
    let mut e = scene();
    enable(&mut e, 0);
    for change in 0..9 {
        let mut c = colors(e.project(), 0);
        match change {
            0 => c.colors[0].id = 0,
            1 => c.colors[0].id = u64::MAX,
            2 => c.colors[0].id = c.colors[1].id,
            3 => {
                c.colors[0].id = 3;
                c.opacities[0].id = 1;
            }
            4 => c.colors[0].red = f64::NAN,
            5 => c.opacities[0].midpoint = 0.,
            6 => {
                c.colors.pop();
            }
            7 => {
                c.opacities.pop();
            }
            _ => {
                c.opacities[0].position = 101.;
            }
        }
        reject(
            &mut e,
            command(GradientColorsEdit::Set {
                frame: 30,
                colors: c,
            }),
        );
    }
    for opacity in [false, true] {
        for _ in 2..ShapeGradient::MAX_STOPS {
            edit(
                &mut e,
                GradientColorsEdit::AddStop {
                    frame: 30,
                    opacity,
                    position: 50.,
                },
            );
        }
        reject(
            &mut e,
            command(GradientColorsEdit::AddStop {
                frame: 30,
                opacity,
                position: 50.,
            }),
        );
    }
    let c = colors(e.project(), 30);
    assert_eq!(c.colors.len(), 32);
    assert_eq!(c.opacities.len(), 32);
}

#[test]
fn gradient_colors_spatial_preview_and_svg_match_literal_static_rows() {
    let mut e = scene();
    enable(&mut e, 10);
    let c = GradientColors {
        colors: vec![
            GradientColorStop {
                id: 1,
                position: 0.,
                midpoint: 25.,
                red: 0.,
                green: 0.,
                blue: 0.,
            },
            GradientColorStop {
                id: 5,
                position: 50.,
                midpoint: 50.,
                red: 0.,
                green: 0.,
                blue: 0.,
            },
            GradientColorStop {
                id: 2,
                position: 50.,
                midpoint: 50.,
                red: 255.,
                green: 255.,
                blue: 255.,
            },
        ],
        opacities: vec![
            GradientOpacityStop {
                id: 3,
                position: 0.,
                midpoint: 50.,
                opacity: 0.,
            },
            GradientOpacityStop {
                id: 4,
                position: 100.,
                midpoint: 50.,
                opacity: 100.,
            },
        ],
    };
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 30,
            colors: c.clone(),
        },
    );
    let n = node(e.project());
    let g = n.kind.gradient().unwrap();
    assert_eq!(
        g.preview(n, 30, 3),
        vec![[0., 0., 0., 0.], [1., 1., 1., 0.5], [1., 1., 1., 1.]]
    );
    for f in [0, 29, 30, 31, 149] {
        let sample = g.sampled_node(n, f);
        let legacy = sample.kind.gradient().unwrap();
        assert!(legacy.colors_animation().is_none());
        assert_eq!(g.preview(n, f, 101), legacy.preview(&sample, f, 101));
        assert_eq!(g.svg(n, f, "oracle"), legacy.svg(&sample, f, "oracle"));
    }
    assert_eq!(
        g.preview_edit(n, 30, 3, GradientParam::Opacity(3), 100.)[0],
        [0., 0., 0., 1.]
    );
}

#[test]
fn gradient_colors_roundtrips_json_native_and_rejects_wrong_versions_and_malformed_metadata() {
    let mut e = scene();
    enable(&mut e, 10);
    edit(
        &mut e,
        GradientColorsEdit::AddStop {
            frame: 30,
            opacity: true,
            position: 55.,
        },
    );
    let p = e.project();
    let json = p.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), *p);
    let view = br#"{"version":2,"unchanged":"endpoint graph"}"#;
    let bytes = project_file::encode(p, Some(view)).unwrap();
    let decoded = project_file::decode(&bytes).unwrap();
    assert_eq!(decoded.project, *p);
    assert_eq!(decoded.view, Some(view.as_slice()));
    assert_eq!(
        project_file::encode(&decoded.project, decoded.view).unwrap(),
        bytes
    );
    for version in [53, PROJECT_VERSION + 1, u32::MAX] {
        let mut bad = p.clone();
        bad.version = version;
        assert!(Project::from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        assert!(project_file::encode(&bad, None).is_err());
    }
    for change in 0..5 {
        let mut bad = p.clone();
        let n = node_mut(&mut bad);
        let mut raw = serde_json::to_value(n.kind.gradient().unwrap()).unwrap();
        match change {
            0 => raw["colors_animation"]["keys"] = serde_json::json!({}),
            1 => raw["next_stop"] = 5.into(),
            2 => raw["colors_animation"]["keys"]["30"]["opacities"][0]["id"] = 1.into(),
            3 => {
                let key = raw["colors_animation"]["keys"]["30"].clone();
                raw["colors_animation"]["keys"]["150"] = key;
            }
            _ => {
                n.parameters
                    .get_mut(&ContentsParam::Gradient(GradientParam::Red(1)))
                    .unwrap()
                    .keys
                    .insert(
                        0,
                        Keyframe {
                            value: 0.,
                            interpolation: Interpolation::Hold,
                            temporal: TemporalHandles::default(),
                        },
                    );
            }
        }
        *n.kind.gradient_mut().unwrap() = serde_json::from_value(raw).unwrap();
        assert!(bad.validate().is_err());
        assert!(project_file::encode(&bad, None).is_err());
    }
}

#[test]
fn gradient_colors_shift_split_duplicate_and_inactive_composition_preserve_all_snapshots() {
    let mut e = scene();
    enable(&mut e, 10);
    edit(
        &mut e,
        GradientColorsEdit::AddStop {
            frame: 90,
            opacity: false,
            position: 45.,
        },
    );
    let original = e.project().clone();
    let original_gradient = node(&original).kind.gradient().unwrap().clone();
    e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
    let g = node(e.project()).kind.gradient().unwrap();
    assert_eq!(
        g.colors_animation()
            .unwrap()
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![20, 100]
    );
    assert_eq!(colors(e.project(), 99), colors(&original, 89));
    assert_eq!(colors(e.project(), 100), colors(&original, 90));
    e.undo();
    assert_eq!(e.project(), &original);
    reject(&mut e, Command::ShiftLayer { id: 1, delta: -11 });
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 50,
    })
    .unwrap();
    for l in e.project().composition.layers() {
        let Content::ShapeContents(c) = l.content() else {
            panic!()
        };
        assert_eq!(c.node(1).unwrap().kind.gradient(), Some(&original_gradient));
    }
    e.undo();
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::Duplicate(1),
    })
    .unwrap();
    let Content::ShapeContents(c) = e.project().composition.layer(1).unwrap().content() else {
        panic!()
    };
    assert_eq!(c.node(2).unwrap().kind.gradient(), Some(&original_gradient));
    e.undo();
    e.execute(Command::NewComposition).unwrap();
    let inactive = e.project().composition_by_id(1).unwrap().clone();
    e.execute(Command::AddNull).unwrap();
    assert_eq!(e.project().version, 54);
    assert_eq!(e.project().composition_by_id(1).unwrap(), &inactive);
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
}

#[test]
fn gradient_colors_layer_clipboard_retimes_snapshots_and_rejects_collisions() {
    let mut e = scene();
    enable(&mut e, 10);
    edit(&mut e, GradientColorsEdit::ToggleKey { frame: 9 });
    let clipboard = e.copy_layers(&[1]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.current.project.composition.fps = FrameRate::new(15, 1).unwrap();
    e.current.project.composition.duration = 150;
    reject(&mut e, Command::PasteLayers(clipboard.clone()));
    e.current.project.composition.fps = FrameRate::new(60, 1).unwrap();
    e.current.project.composition.duration = 300;
    e.execute(Command::PasteLayers(clipboard)).unwrap();
    let Content::ShapeContents(c) = e.selected_layer().unwrap().content() else {
        panic!()
    };
    assert_eq!(
        c.node(1)
            .unwrap()
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap()
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![18, 20]
    );
}

fn install_keys(e: &mut Editor, sample: &GradientColors, count: usize) {
    let keys = (0..count)
        .map(|i| (i.to_string(), serde_json::to_value(sample).unwrap()))
        .collect::<serde_json::Map<_, _>>();
    let g = node_mut(&mut e.current.project)
        .kind
        .gradient_mut()
        .unwrap();
    g.next_stop = sample
        .colors
        .iter()
        .map(|s| s.id)
        .chain(sample.opacities.iter().map(|s| s.id))
        .max()
        .unwrap()
        + 1;
    g.colors_animation = Some(serde_json::from_value(serde_json::json!({"keys":keys})).unwrap());
    e.current.project.version = 54;
    e.current.project.composition.duration = 5000;
    e.current.project.validate().unwrap();
}
#[test]
fn gradient_colors_key_and_stored_stop_budgets_preserve_exact_failure_state() {
    let mut e = scene();
    let c = colors(e.project(), 0);
    install_keys(&mut e, &c, 1000);
    reject(
        &mut e,
        command(GradientColorsEdit::ToggleKey { frame: 1001 }),
    );
    edit(
        &mut e,
        GradientColorsEdit::Color {
            frame: 100,
            stop: 1,
            color: 123,
        },
    );
    let mut c = colors(e.project(), 0);
    for id in 5..35 {
        let mut s = c.colors[0].clone();
        s.id = id;
        c.colors.push(s);
    }
    for id in 35..65 {
        let mut s = c.opacities[0].clone();
        s.id = id;
        c.opacities.push(s);
    }
    install_keys(&mut e, &c, 512);
    reject(
        &mut e,
        command(GradientColorsEdit::ToggleKey { frame: 1001 }),
    );
    edit(
        &mut e,
        GradientColorsEdit::Color {
            frame: 100,
            stop: 1,
            color: 123,
        },
    );
}

#[test]
fn gradient_colors_metadata_budget_is_checked_before_and_after_source_preserving_edits() {
    let mut e = scene();
    let mut text = Editor::default();
    text.execute(Command::AddContent {
        content: Content::Text {
            text: "\u{0001}".repeat(16384),
            font_size: 24.,
        },
        width: 10.,
        height: 10.,
        name: "Text".into(),
    })
    .unwrap();
    let template = text.project().composition.layers[0].clone();
    for id in 2..=181 {
        let mut l = template.clone();
        l.id = id;
        e.current.project.composition.layers.push(l);
    }
    e.current.project.next_layer_id = 182;
    e.current.project.version = 54;
    let size = |p: &Project| serde_json::to_vec(p).unwrap().len();
    let limit = 16 * 1024 * 1024;
    let mut remaining = (size(e.project()) - limit).div_ceil(5);
    for l in &mut e.current.project.composition.layers[1..] {
        let Content::Text { text, .. } = &mut l.content else {
            panic!()
        };
        let count = remaining.min(text.len());
        *text = "x".repeat(count) + &"\u{0001}".repeat(text.len() - count);
        remaining -= count;
    }
    assert_eq!(remaining, 0);
    let pad = limit - size(e.project());
    e.current
        .project
        .composition
        .name
        .push_str(&"x".repeat(pad));
    document::validate_budget(e.project()).unwrap();
    reject(
        &mut e,
        command(GradientColorsEdit::SetAnimation {
            frame: 0,
            enabled: true,
        }),
    );
    e.current.project.composition.name.push('x');
    reject(
        &mut e,
        command(GradientColorsEdit::SetAnimation {
            frame: 0,
            enabled: false,
        }),
    );
    // Reserve enough room for Hold animation, then fill the exact budget again.
    // Sparse interpolation metadata is charged even for a dormant final key.
    e.current.project.composition.name.pop();
    let Content::Text { text, .. } = &mut e.current.project.composition.layers[1].content else {
        panic!()
    };
    text.truncate(text.len().saturating_sub(1024));
    enable(&mut e, 0);
    let pad = limit - size(e.project());
    e.current
        .project
        .composition
        .name
        .push_str(&"x".repeat(pad));
    document::validate_budget(e.project()).unwrap();
    reject(
        &mut e,
        command(GradientColorsEdit::SetInterpolation {
            frame: 0,
            interpolation: GradientColorsInterpolation::Linear,
        }),
    );
    let before = (e.current.clone(), e.undo.clone(), e.redo.clone());
    mode(&mut e, 0, GradientColorsInterpolation::Hold);
    assert_eq!((e.current.clone(), e.undo.clone(), e.redo.clone()), before);
    let sample = colors(e.project(), 0);
    for action in [
        GradientColorsEdit::MoveKeys {
            frames: BTreeSet::from([0]),
            to: 10,
        },
        GradientColorsEdit::SetInterpolations {
            frames: BTreeSet::from([0]),
            interpolation: GradientColorsInterpolation::Smooth,
        },
        GradientColorsEdit::PasteKeys {
            keys: vec![GradientColorsKeyCopy {
                offset: 0,
                colors: sample.clone(),
                interpolation: GradientColorsInterpolation::Hold,
            }],
            frame: 10,
        },
    ] {
        reject(&mut e, command(action));
    }
    e.current.project.composition.name.push('x');
    reject(
        &mut e,
        command(GradientColorsEdit::SetInterpolation {
            frame: 0,
            interpolation: GradientColorsInterpolation::Hold,
        }),
    );
    reject(&mut e, command(GradientColorsEdit::DeleteKey { frame: 0 }));
    // Even reducing or source-neutral commands cannot repair an oversized source.
    for action in [
        GradientColorsEdit::MoveKeys {
            frames: BTreeSet::from([0]),
            to: 0,
        },
        GradientColorsEdit::DeleteKeys {
            frames: BTreeSet::from([0]),
            frame: 0,
        },
        GradientColorsEdit::SetInterpolations {
            frames: BTreeSet::from([0]),
            interpolation: GradientColorsInterpolation::Hold,
        },
        GradientColorsEdit::PasteKeys {
            keys: vec![GradientColorsKeyCopy {
                offset: 0,
                colors: sample.clone(),
                interpolation: GradientColorsInterpolation::Hold,
            }],
            frame: 0,
        },
    ] {
        reject(&mut e, command(action));
    }
}

#[test]
fn gradient_colors_pure_batch_return_and_outside_layer_range_keys_are_explicit() {
    let mut e = scene();
    let original = e.current.clone();
    e.execute(Command::Batch(vec![
        command(GradientColorsEdit::SetAnimation {
            frame: 140,
            enabled: true,
        }),
        command(GradientColorsEdit::SetAnimation {
            frame: 140,
            enabled: false,
        }),
    ]))
    .unwrap();
    assert_eq!(e.current, original);
    assert!(!e.can_undo());
    enable(&mut e, 140);
    assert!(
        node(e.project())
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap()
            .keys()
            .contains_key(&140)
    );
    reject(&mut e, Command::ShiftLayer { id: 1, delta: 10 });
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 50,
    })
    .unwrap();
    for layer in e.project().composition.layers() {
        let Content::ShapeContents(c) = layer.content() else {
            panic!()
        };
        assert!(
            c.node(1)
                .unwrap()
                .kind
                .gradient()
                .unwrap()
                .colors_animation()
                .unwrap()
                .keys()
                .contains_key(&140)
        );
    }
}

#[test]
fn gradient_colors_set_accepts_fresh_ids_without_recycling_or_cross_row_reuse() {
    let mut e = scene();
    enable(&mut e, 0);
    let mut value = colors(e.project(), 0);
    let mut fresh = value.colors[0].clone();
    fresh.id = 500;
    fresh.position = 25.;
    value.colors.push(fresh);
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 30,
            colors: value,
        },
    );
    assert_eq!(node(e.project()).kind.gradient().unwrap().next_stop, 501);
    edit(
        &mut e,
        GradientColorsEdit::RemoveStop {
            frame: 60,
            stop: 500,
        },
    );
    let mut wrong = colors(e.project(), 60);
    let mut old_color_as_opacity = wrong.opacities[0].clone();
    old_color_as_opacity.id = 500;
    wrong.opacities.push(old_color_as_opacity);
    reject(
        &mut e,
        command(GradientColorsEdit::Set {
            frame: 90,
            colors: wrong,
        }),
    );
    edit(
        &mut e,
        GradientColorsEdit::AddStop {
            frame: 90,
            opacity: true,
            position: 50.,
        },
    );
    assert_eq!(colors(e.project(), 90).opacities.last().unwrap().id, 501);
    assert_eq!(node(e.project()).kind.gradient().unwrap().next_stop, 502);
    let mut unknown = serde_json::to_value(
        node(e.project())
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap(),
    )
    .unwrap();
    unknown["interpolation"] = "Linear".into();
    assert!(serde_json::from_value::<GradientColorsAnimation>(unknown).is_err());
}

#[test]
fn gradient_colors_signed_zero_snapshot_changes_are_not_false_render_noops() {
    let mut e = scene();
    enable(&mut e, 0);
    let mut sample = colors(e.project(), 0);
    sample.colors[1].position = 0.;
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 20,
            colors: sample.clone(),
        },
    );
    let before = e.project().clone();
    let n = node(&before);
    let g = n.kind.gradient().unwrap();
    let old_preview = g.preview(n, 20, 3);
    sample.colors[1].position = -0.;
    assert_ne!(sample, colors(&before, 20));
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 20,
            colors: sample,
        },
    );
    let n = node(e.project());
    let g = n.kind.gradient().unwrap();
    assert_ne!(g.preview(n, 20, 3), old_preview);
    assert_eq!(
        colors(e.project(), 20).colors[1].position.to_bits(),
        (-0.0_f64).to_bits()
    );
    e.undo();
    assert_eq!(e.project(), &before);
}

fn animation(project: &Project) -> &GradientColorsAnimation {
    node(project)
        .kind
        .gradient()
        .unwrap()
        .colors_animation()
        .unwrap()
}
fn mode(e: &mut Editor, frame: Frame, interpolation: GradientColorsInterpolation) {
    edit(
        e,
        GradientColorsEdit::SetInterpolation {
            frame,
            interpolation,
        },
    );
}
fn interpolation_scene() -> (Editor, GradientColors, GradientColors) {
    let mut e = scene();
    let start = GradientColors {
        colors: vec![
            GradientColorStop {
                id: 1,
                position: -0.,
                midpoint: 1.,
                red: -0.,
                green: 64.,
                blue: 255.,
            },
            GradientColorStop {
                id: 2,
                position: 100.,
                midpoint: 99.,
                red: 255.,
                green: 192.,
                blue: 0.,
            },
        ],
        opacities: vec![
            GradientOpacityStop {
                id: 3,
                position: 20.,
                midpoint: 10.,
                opacity: 0.,
            },
            GradientOpacityStop {
                id: 4,
                position: 80.,
                midpoint: 90.,
                opacity: 100.,
            },
        ],
    };
    let end = GradientColors {
        colors: vec![
            GradientColorStop {
                id: 1,
                position: 100.,
                midpoint: 99.,
                red: 255.,
                green: 192.,
                blue: 0.,
            },
            GradientColorStop {
                id: 2,
                position: 0.,
                midpoint: 1.,
                red: 0.,
                green: 64.,
                blue: 255.,
            },
        ],
        opacities: vec![
            GradientOpacityStop {
                id: 3,
                position: 80.,
                midpoint: 90.,
                opacity: 100.,
            },
            GradientOpacityStop {
                id: 4,
                position: 20.,
                midpoint: 10.,
                opacity: 0.,
            },
        ],
    };
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 0,
            colors: start.clone(),
        },
    );
    enable(&mut e, 10);
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 50,
            colors: end.clone(),
        },
    );
    e.clear_history();
    (e, start, end)
}

#[test]
fn gradient_colors_interpolation_uses_one_bounded_progress_for_every_number_and_exact_endpoints() {
    use GradientColorsInterpolation::*;
    for interpolation in [Hold, Linear, Smooth] {
        let (mut e, start, end) = interpolation_scene();
        let base = node(e.project()).parameters.clone();
        mode(&mut e, 10, interpolation);
        for frame in [0, 9, 10] {
            assert_eq!(colors(e.project(), frame), start);
        }
        for frame in [50, 51, 149] {
            assert_eq!(colors(e.project(), frame), end);
        }
        for frame in 11..50 {
            let t = f64::from(frame - 10) / 40.;
            let t = match interpolation {
                Hold => 0.,
                Linear => t,
                Smooth => t * t * (3. - 2. * t),
            };
            let mut expected = start.clone();
            let mix = |a: f64, b: f64| a + (b - a) * t;
            if interpolation != Hold {
                for (value, right) in expected.colors.iter_mut().zip(&end.colors) {
                    value.position = mix(value.position, right.position);
                    value.midpoint = mix(value.midpoint, right.midpoint);
                    value.red = mix(value.red, right.red);
                    value.green = mix(value.green, right.green);
                    value.blue = mix(value.blue, right.blue);
                }
                for (value, right) in expected.opacities.iter_mut().zip(&end.opacities) {
                    value.position = mix(value.position, right.position);
                    value.midpoint = mix(value.midpoint, right.midpoint);
                    value.opacity = mix(value.opacity, right.opacity);
                }
            }
            assert_eq!(
                colors(e.project(), frame),
                expected,
                "{interpolation:?} at {frame}"
            );
        }
        assert_eq!(node(e.project()).parameters, base);
        let a = animation(e.project());
        assert_eq!(a.interpolation(10), Some(interpolation));
        assert_eq!(a.interpolation(11), None);
        assert_eq!(a.segment_status(11), None);
        assert_eq!(
            a.segment_at(9).unwrap().hold_reason,
            Some(GradientColorsHoldReason::BeforeFirstKey)
        );
        assert_eq!(
            a.segment_at(20).unwrap(),
            GradientColorsSegmentStatus {
                frame: 10,
                next_frame: Some(50),
                interpolation,
                effective: interpolation,
                hold_reason: None,
            }
        );
        assert_eq!(
            a.segment_at(149).unwrap().hold_reason,
            Some(GradientColorsHoldReason::NoNextKey)
        );
        assert_eq!(a.keys().get(&10), Some(&start));
        assert_eq!(a.keys().get(&50), Some(&end));
    }
}

#[test]
fn gradient_colors_interpolation_topology_is_ordered_and_independent_in_both_rows() {
    use GradientColorsInterpolation::*;
    for change in 0..8 {
        for interpolation in [Linear, Smooth] {
            let (mut e, mut start, mut end) = interpolation_scene();
            match change {
                0 => end.colors.swap(0, 1),
                1 => end.opacities.swap(0, 1),
                2 => {
                    let mut stop = end.colors[0].clone();
                    stop.id = 5;
                    end.colors.push(stop);
                }
                3 => {
                    let mut stop = end.opacities[0].clone();
                    stop.id = 5;
                    end.opacities.push(stop);
                }
                4 => {
                    let mut stop = start.colors[0].clone();
                    stop.id = 5;
                    start.colors.push(stop);
                }
                5 => {
                    let mut stop = start.opacities[0].clone();
                    stop.id = 5;
                    start.opacities.push(stop);
                }
                6 => end.colors[0].id = 5,
                _ => end.opacities[0].id = 5,
            }
            edit(
                &mut e,
                GradientColorsEdit::Set {
                    frame: 10,
                    colors: start.clone(),
                },
            );
            edit(
                &mut e,
                GradientColorsEdit::Set {
                    frame: 50,
                    colors: end.clone(),
                },
            );
            mode(&mut e, 10, interpolation);
            for frame in [0, 10, 11, 30, 49] {
                assert_eq!(colors(e.project(), frame), start);
            }
            assert_eq!(colors(e.project(), 50), end);
            let status = animation(e.project()).segment_status(10).unwrap();
            assert_eq!(status.interpolation, interpolation);
            assert_eq!(status.effective, Hold);
            assert_eq!(
                status.hold_reason,
                Some(GradientColorsHoldReason::IncompatibleTopology)
            );
            mode(&mut e, 10, Hold);
            assert_eq!(
                animation(e.project())
                    .segment_status(10)
                    .unwrap()
                    .hold_reason,
                None
            );
        }
    }
}

#[test]
fn gradient_colors_interpolation_noops_and_pure_batch_return_preserve_schema_bytes_and_redo() {
    use GradientColorsInterpolation::*;
    for version in [54, 55, 56, 57] {
        let (mut e, _, _) = interpolation_scene();
        e.current.project.version = version;
        let before = e.current.clone();
        let json = e.project().to_json().unwrap();
        assert!(!json.contains("outgoing_interpolation"));
        mode(&mut e, 10, Linear);
        assert_eq!(e.project().version, 57);
        e.undo();
        let (undo, redo) = (e.undo.clone(), e.redo.clone());
        mode(&mut e, 10, Hold);
        let sample = colors(e.project(), 20);
        edit(
            &mut e,
            GradientColorsEdit::Set {
                frame: 20,
                colors: sample,
            },
        );
        e.execute(Command::Batch(vec![
            command(GradientColorsEdit::SetInterpolation {
                frame: 10,
                interpolation: Smooth,
            }),
            command(GradientColorsEdit::SetInterpolation {
                frame: 10,
                interpolation: Hold,
            }),
        ]))
        .unwrap();
        assert_eq!(e.current, before);
        assert_eq!(e.project().to_json().unwrap(), json);
        assert_eq!(e.undo, undo);
        assert_eq!(e.redo, redo);
        e.redo();
        let before = (e.current.clone(), e.undo.clone(), e.redo.clone());
        mode(&mut e, 10, Linear);
        assert_eq!((e.current.clone(), e.undo.clone(), e.redo.clone()), before);
        assert_eq!(
            gradient_json(e.project())["colors_animation"]["outgoing_interpolation"],
            serde_json::json!({"10":"Linear"})
        );
    }
}

#[test]
fn gradient_colors_interpolation_snapshot_edits_keep_mode_and_new_keys_default_to_hold() {
    use GradientColorsInterpolation::*;
    let (mut e, _, _) = interpolation_scene();
    mode(&mut e, 10, Smooth);
    for action in [
        GradientColorsEdit::Value {
            frame: 10,
            parameter: GradientParam::Red(1),
            value: 7.,
        },
        GradientColorsEdit::Color {
            frame: 10,
            stop: 2,
            color: 0x123456,
        },
        GradientColorsEdit::AddStop {
            frame: 10,
            opacity: false,
            position: 50.,
        },
        GradientColorsEdit::RemoveStop { frame: 10, stop: 5 },
    ] {
        edit(&mut e, action);
        assert_eq!(animation(e.project()).interpolation(10), Some(Smooth));
    }
    let sample = colors(e.project(), 20);
    edit(&mut e, GradientColorsEdit::ToggleKey { frame: 20 });
    assert_eq!(animation(e.project()).keys().get(&20), Some(&sample));
    assert_eq!(animation(e.project()).interpolation(20), Some(Hold));
    let before = e.project().clone();
    mode(&mut e, 20, Linear);
    edit(&mut e, GradientColorsEdit::ToggleKey { frame: 20 });
    assert_eq!(animation(e.project()).interpolation(20), None);
    assert_eq!(
        gradient_json(e.project())["colors_animation"]["outgoing_interpolation"],
        serde_json::json!({"10":"Smooth"})
    );
    e.undo();
    e.undo();
    assert_eq!(*e.project(), before);
    edit(
        &mut e,
        GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::Opacity(3),
            value: 17.,
        },
    );
    assert_eq!(animation(e.project()).interpolation(30), Some(Hold));
    assert_eq!(animation(e.project()).interpolation(10), Some(Smooth));
}

#[test]
fn gradient_colors_interpolation_move_delete_and_disable_are_atomic_and_preserve_modes() {
    use GradientColorsInterpolation::*;
    let (mut e, start, end) = interpolation_scene();
    mode(&mut e, 10, Linear);
    mode(&mut e, 50, Smooth);
    let original = e.current.clone();
    edit(&mut e, GradientColorsEdit::MoveKey { from: 10, to: 20 });
    assert_eq!(animation(e.project()).interpolation(20), Some(Linear));
    assert_eq!(animation(e.project()).interpolation(10), None);
    assert_eq!(animation(e.project()).keys().get(&20), Some(&start));
    e.undo();
    assert_eq!(e.current, original);
    e.redo();
    for action in [
        GradientColorsEdit::MoveKey { from: 20, to: 50 },
        GradientColorsEdit::MoveKey { from: 21, to: 21 },
        GradientColorsEdit::MoveKey { from: 20, to: 150 },
        GradientColorsEdit::DeleteKey { frame: 21 },
        GradientColorsEdit::DeleteKey { frame: 150 },
        GradientColorsEdit::SetInterpolation {
            frame: 21,
            interpolation: Hold,
        },
        GradientColorsEdit::SetInterpolation {
            frame: 150,
            interpolation: Smooth,
        },
    ] {
        reject(&mut e, command(action));
    }
    let before_delete = e.current.clone();
    edit(&mut e, GradientColorsEdit::DeleteKey { frame: 20 });
    assert_eq!(animation(e.project()).keys().len(), 1);
    assert_eq!(animation(e.project()).interpolation(50), Some(Smooth));
    assert_eq!(colors(e.project(), 0), end);
    edit(&mut e, GradientColorsEdit::DeleteKey { frame: 50 });
    assert!(
        node(e.project())
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .is_none()
    );
    assert_eq!(colors(e.project(), 0), end);
    assert!(
        !gradient_json(e.project())
            .to_string()
            .contains("outgoing_interpolation")
    );
    e.undo();
    e.undo();
    assert_eq!(e.current, before_delete);
    let sample = colors(e.project(), 30);
    edit(
        &mut e,
        GradientColorsEdit::SetAnimation {
            frame: 30,
            enabled: false,
        },
    );
    assert_eq!(colors(e.project(), 0), sample);
    assert_eq!(colors(e.project(), 149), sample);
    assert_eq!(e.project().version, 57);
    reject(&mut e, command(GradientColorsEdit::DeleteKey { frame: 30 }));
    reject(
        &mut e,
        command(GradientColorsEdit::SetInterpolation {
            frame: 30,
            interpolation: Hold,
        }),
    );
}

#[test]
fn gradient_colors_interpolation_final_mode_is_dormant_until_new_segment_exists() {
    use GradientColorsInterpolation::*;
    let mut e = scene();
    enable(&mut e, 10);
    let snapshot = colors(e.project(), 10);
    mode(&mut e, 10, Smooth);
    assert_eq!(e.project().version, 57);
    for frame in [0, 10, 11, 149] {
        assert_eq!(colors(e.project(), frame), snapshot);
    }
    assert_eq!(
        animation(e.project()).segment_status(10).unwrap(),
        GradientColorsSegmentStatus {
            frame: 10,
            next_frame: None,
            interpolation: Smooth,
            effective: Hold,
            hold_reason: Some(GradientColorsHoldReason::NoNextKey),
        }
    );
    edit(
        &mut e,
        GradientColorsEdit::Color {
            frame: 50,
            stop: 1,
            color: 0xffffff,
        },
    );
    assert_eq!(
        animation(e.project()).segment_status(10).unwrap().effective,
        Smooth
    );
    assert_eq!(colors(e.project(), 20).colors[0].red, 255. * 0.15625);
    mode(&mut e, 50, Linear);
    edit(&mut e, GradientColorsEdit::MoveKey { from: 50, to: 0 });
    assert_eq!(animation(e.project()).interpolation(0), Some(Linear));
    assert_eq!(
        animation(e.project()).segment_status(0).unwrap().effective,
        Linear
    );
    assert_eq!(animation(e.project()).interpolation(10), Some(Smooth));
    assert_eq!(colors(e.project(), 5).colors[0].red, 127.5);
}

#[test]
fn gradient_colors_interpolation_shift_split_duplicate_and_clipboard_retime_keep_metadata() {
    use GradientColorsInterpolation::*;
    let (mut e, _, _) = interpolation_scene();
    mode(&mut e, 10, Linear);
    mode(&mut e, 50, Smooth);
    let original = e.current.clone();
    let gradient = node(e.project()).kind.gradient().unwrap().clone();
    e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
    assert_eq!(animation(e.project()).interpolation(20), Some(Linear));
    assert_eq!(animation(e.project()).interpolation(60), Some(Smooth));
    assert_eq!(colors(e.project(), 35), colors(&original.project, 25));
    e.undo();
    assert_eq!(e.current, original);
    reject(&mut e, Command::ShiftLayer { id: 1, delta: -11 });
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 30,
    })
    .unwrap();
    for layer in e.project().composition.layers() {
        let Content::ShapeContents(c) = layer.content() else {
            panic!()
        };
        assert_eq!(c.node(1).unwrap().kind.gradient(), Some(&gradient));
    }
    e.undo();
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::Duplicate(1),
    })
    .unwrap();
    let Content::ShapeContents(c) = e.project().composition.layer(1).unwrap().content() else {
        panic!()
    };
    assert_eq!(c.node(2).unwrap().kind.gradient(), Some(&gradient));
    e.undo();
    let clipboard = e.copy_layers(&[1]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.current.project.composition.fps = FrameRate::new(60, 1).unwrap();
    e.current.project.composition.duration = 300;
    e.execute(Command::PasteLayers(clipboard)).unwrap();
    let Content::ShapeContents(c) = e.selected_layer().unwrap().content() else {
        panic!()
    };
    let n = c.node(1).unwrap();
    let g = n.kind.gradient().unwrap();
    let a = g.colors_animation().unwrap();
    assert_eq!(a.interpolation(20), Some(Linear));
    assert_eq!(a.interpolation(100), Some(Smooth));
    assert_eq!(g.colors_at(n, 50), colors(&original.project, 25));
    assert_eq!(e.project().version, 57);
    e.undo();
    e.current.project.composition.fps = FrameRate::new(1, 1).unwrap();
    e.current.project.composition.duration = 150;
    // Two adjacent source frames map to one destination frame.
    let (mut source, _, _) = interpolation_scene();
    edit(&mut source, GradientColorsEdit::ToggleKey { frame: 11 });
    mode(&mut source, 10, Smooth);
    reject(
        &mut e,
        Command::PasteLayers(source.copy_layers(&[1]).unwrap()),
    );
}

#[test]
fn gradient_colors_interpolation_contents_copy_undo_paste_promotes_only_required_schema() {
    use GradientColorsInterpolation::*;
    let (mut e, _, _) = interpolation_scene();
    let before = e.current.clone();
    mode(&mut e, 50, Smooth);
    let gradient = node(e.project()).kind.gradient().unwrap().clone();
    let clipboard = e.copy_contents(1, 0, &[1]).unwrap();
    e.undo();
    assert_eq!(e.current, before);
    assert_eq!(e.project().version, 54);
    let ids = e.paste_contents(1, 0, 1, &clipboard).unwrap();
    assert_eq!(e.project().version, 57);
    let Content::ShapeContents(contents) = e.project().composition.layer(1).unwrap().content()
    else {
        panic!()
    };
    assert_eq!(
        contents.node(ids[0]).unwrap().kind.gradient(),
        Some(&gradient)
    );
    e.undo();
    assert_eq!(e.current, before);
    e.redo();
    assert_eq!(e.project().version, 57);
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::AddNull).unwrap();
    assert_eq!(e.project().version, 57);
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
}

#[test]
fn gradient_colors_interpolation_json_native_view_roundtrip_and_strict_metadata_validation() {
    use GradientColorsInterpolation::*;
    let (mut e, _, _) = interpolation_scene();
    mode(&mut e, 10, Linear);
    mode(&mut e, 50, Smooth);
    let project = e.project().clone();
    let json = project.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), project);
    let view = br#"{"frame":30,"expanded":[1],"future":{"value":null}}"#;
    let native = project_file::encode(&project, Some(view)).unwrap();
    let decoded = project_file::decode(&native).unwrap();
    assert_eq!(decoded.project, project);
    assert_eq!(decoded.view, Some(view.as_slice()));
    assert_eq!(
        project_file::encode(&decoded.project, decoded.view).unwrap(),
        native
    );
    for version in [54, 55, 56] {
        let mut bad = project.clone();
        bad.version = version;
        assert!(bad.validate().is_err());
        assert!(Project::from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        assert!(project_file::encode(&bad, None).is_err());
    }
    for metadata in [
        serde_json::json!({"10":"Bezier"}),
        serde_json::json!({"10":null}),
        serde_json::json!([]),
        serde_json::json!({"-1":"Linear"}),
        serde_json::json!({"4294967296":"Linear"}),
    ] {
        let mut raw = gradient_json(&project)["colors_animation"].clone();
        raw["outgoing_interpolation"] = metadata;
        assert!(serde_json::from_value::<GradientColorsAnimation>(raw).is_err());
    }
    for metadata in [
        serde_json::json!({"10":"Hold"}),
        serde_json::json!({"11":"Linear"}),
        serde_json::json!({"150":"Smooth"}),
    ] {
        let mut bad = project.clone();
        let mut raw = gradient_json(&bad)["colors_animation"].clone();
        raw["outgoing_interpolation"] = metadata;
        node_mut(&mut bad)
            .kind
            .gradient_mut()
            .unwrap()
            .colors_animation = Some(serde_json::from_value(raw).unwrap());
        assert!(bad.validate().is_err());
        assert!(Project::from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        assert!(project_file::encode(&bad, None).is_err());
        e.current.project = bad;
        // Neither a no-op nor removing/replacing metadata may repair invalid source.
        reject(
            &mut e,
            command(GradientColorsEdit::SetInterpolation {
                frame: 10,
                interpolation: Hold,
            }),
        );
        reject(&mut e, command(GradientColorsEdit::DeleteKey { frame: 10 }));
    }
}

#[test]
fn gradient_colors_interpolation_identical_signed_zero_ties_are_render_constant() {
    use GradientColorsInterpolation::*;
    for interpolation in [Linear, Smooth] {
        let mut e = scene();
        let mut sample = colors(e.project(), 0);
        sample.colors[0].position = 0.;
        sample.colors[1].position = -0.;
        sample.opacities[0].position = 0.;
        sample.opacities[1].position = -0.;
        sample.opacities[0].opacity = 0.;
        edit(
            &mut e,
            GradientColorsEdit::Set {
                frame: 0,
                colors: sample.clone(),
            },
        );
        enable(&mut e, 10);
        edit(&mut e, GradientColorsEdit::ToggleKey { frame: 50 });
        mode(&mut e, 10, interpolation);
        let n = node(e.project());
        let g = n.kind.gradient().unwrap();
        let expected = g.preview(n, 10, 17);
        for frame in 0..=60 {
            assert_eq!(
                g.colors_at(n, frame),
                sample,
                "{interpolation:?} at {frame}"
            );
            assert_eq!(g.preview(n, frame, 17), expected);
        }
    }
}

#[test]
fn gradient_colors_interpolation_and_delete_reject_mixed_or_unbounded_batches_without_history() {
    use GradientColorsInterpolation::*;
    let (mut e, _, _) = interpolation_scene();
    for action in [
        GradientColorsEdit::SetInterpolation {
            frame: 10,
            interpolation: Linear,
        },
        GradientColorsEdit::DeleteKey { frame: 10 },
    ] {
        reject(
            &mut e,
            Command::Batch(vec![command(action.clone()), Command::AddNull]),
        );
        reject(
            &mut e,
            Command::Batch(vec![command(action.clone()), Command::Batch(vec![])]),
        );
        let mut deep = command(action.clone());
        for _ in 0..65 {
            deep = Command::Batch(vec![deep]);
        }
        reject(&mut e, deep);
        reject(&mut e, Command::Batch(vec![command(action); 10000]));
    }
    let original = e.current.clone();
    e.execute(Command::Batch(vec![
        command(GradientColorsEdit::SetInterpolation {
            frame: 10,
            interpolation: Smooth,
        }),
        command(GradientColorsEdit::SetInterpolation {
            frame: 10,
            interpolation: Hold,
        }),
    ]))
    .unwrap();
    assert_eq!(e.current, original);
    assert!(!e.can_undo());
}
