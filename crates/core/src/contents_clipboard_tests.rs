use super::*;

fn node(id: u64, kind: ContentsKind) -> ContentsNode {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents::default()),
            width: 100.,
            height: 100.,
            name: "Fixture".into(),
        })
        .unwrap();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add {
                parent: 0,
                kind: if matches!(kind, ContentsKind::Group(_)) {
                    ContentsKind::Group(vec![])
                } else {
                    kind.clone()
                },
            },
        })
        .unwrap();
    let mut node = contents(editor.project(), 1).items[0].clone();
    node.id = id;
    node.name = format!("Authored {} {id}", kind.label());
    node.kind = kind;
    node
}
fn contents(project: &Project, id: LayerId) -> &ShapeContents {
    let Content::ShapeContents(contents) = &project.composition.layer(id).unwrap().content else {
        panic!()
    };
    contents
}
fn contents_mut(project: &mut Project, id: LayerId) -> &mut ShapeContents {
    let Content::ShapeContents(contents) = &mut project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == id)
        .unwrap()
        .content
    else {
        panic!()
    };
    contents
}
fn scene(nodes: Vec<ContentsNode>, next_id: u64, version: u32) -> Editor {
    let mut editor = Editor::default();
    for name in ["Source", "Destination"] {
        editor
            .execute(Command::AddContent {
                content: Content::ShapeContents(ShapeContents::default()),
                width: 100.,
                height: 100.,
                name: name.into(),
            })
            .unwrap();
    }
    *contents_mut(&mut editor.current.project, 1) = ShapeContents {
        items: nodes,
        next_id,
    };
    editor.current.project.version = version;
    editor.current.project.validate().unwrap();
    editor.select(1);
    editor.clear_history();
    // Both histories deliberately contain complete, distinct valid snapshots.
    let mut older = editor.current.clone();
    older.project.composition.name = "Older".into();
    editor.undo.push(older);
    let mut newer = editor.current.clone();
    newer.project.composition.name = "Redo".into();
    editor.redo.push(newer);
    editor
}
fn fixture() -> Editor {
    let mut inner = node(
        6,
        ContentsKind::Group(vec![
            node(19, ContentsKind::Parametric(ShapeKind::Star)),
            node(4, ContentsKind::Fill { even_odd: true }),
        ]),
    );
    inner.enabled = false;
    let group = node(
        40,
        ContentsKind::Group(vec![inner, node(12, ContentsKind::TrimPaths)]),
    );
    scene(
        vec![
            node(73, ContentsKind::Fill { even_odd: false }),
            group,
            node(9, ContentsKind::Parametric(ShapeKind::Ellipse)),
            node(25, ContentsKind::Stroke(ShapeStroke::default())),
        ],
        100,
        53,
    )
}
fn remove(id: LayerId, parent: u64, items: &[u64]) -> Command {
    Command::Contents {
        id,
        edit: ContentsEdit::RemoveSiblings {
            parent,
            items: items.to_vec(),
        },
    }
}
fn paste(id: LayerId, parent: u64, index: usize, clipboard: &ContentsClipboard) -> Command {
    Command::Contents {
        id,
        edit: ContentsEdit::Paste {
            parent,
            index,
            clipboard: clipboard.clone(),
        },
    }
}
fn assert_unchanged(editor: &Editor, current: &Snapshot, undo: &[Snapshot], redo: &[Snapshot]) {
    assert_eq!(&editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    assert_eq!(
        serde_json::to_vec(&editor.current.project).unwrap(),
        serde_json::to_vec(&current.project).unwrap()
    );
}
fn reject(editor: &mut Editor, command: Command) {
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    assert!(editor.execute(command).is_err());
    assert_unchanged(editor, &current, &undo, &redo);
}
fn ids(contents: &ShapeContents) -> Vec<u64> {
    contents.rows().iter().map(|(_, _, n)| n.id).collect()
}

#[test]
fn contents_clipboard_copy_is_source_ordered_immutable_and_history_free() {
    let mut editor = fixture();
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    let clipboard = editor.copy_contents(1, 0, &[25, 40]).unwrap();
    assert_eq!(clipboard.len(), 2);
    assert!(!clipboard.is_empty());
    assert_eq!(
        clipboard.contents.items,
        vec![
            contents(editor.project(), 1).items[1].clone(),
            contents(editor.project(), 1).items[3].clone()
        ]
    );
    assert_unchanged(&editor, &current, &undo, &redo);
    let captured = clipboard.clone();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Rename {
                item: 19,
                name: "Changed after copy".into(),
            },
        })
        .unwrap();
    editor.execute(remove(1, 0, &[40, 25])).unwrap();
    assert_eq!(clipboard, captured);
    assert_eq!(
        editor.paste_contents(2, 0, 0, &clipboard).unwrap(),
        vec![1, 6]
    );
    assert_eq!(ids(contents(editor.project(), 2)), vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(
        contents(editor.project(), 2).node(3).unwrap().name,
        "Authored Star 19"
    );
    assert!(!contents(editor.project(), 2).node(2).unwrap().enabled);
}

#[test]
fn contents_clipboard_cut_removes_exact_siblings_and_retains_empty_parent_one_history() {
    let mut editor = fixture();
    let before = editor.current.clone();
    let undo = editor.undo.clone();
    let clipboard = editor.copy_contents(1, 40, &[12, 6]).unwrap();
    editor.execute(remove(1, 40, &[12, 6])).unwrap();
    let after = editor.current.clone();
    assert_eq!(contents(editor.project(), 1).next_id, 100);
    assert!(
        matches!(&contents(editor.project(), 1).node(40).unwrap().kind, ContentsKind::Group(v) if v.is_empty())
    );
    assert_eq!(editor.undo.len(), undo.len() + 1);
    assert!(editor.redo.is_empty());
    editor.undo();
    assert_eq!(editor.current, before);
    assert_eq!(editor.undo, undo);
    editor.redo();
    assert_eq!(editor.current, after);
    let inserted = editor.paste_contents(1, 40, 0, &clipboard).unwrap();
    assert_eq!(inserted, vec![100, 103]);
    assert_eq!(contents(editor.project(), 1).next_id, 104);
    assert_eq!(editor.project().version, 53);
}

#[test]
fn contents_clipboard_repeated_pastes_rekey_recursively_without_renaming() {
    let mut editor = fixture();
    let clipboard = editor.copy_contents(1, 0, &[40]).unwrap();
    let before = editor.current.clone();
    let previous_undo = editor.undo.clone();
    assert_eq!(
        editor.paste_contents(1, 0, 1, &clipboard).unwrap(),
        vec![100]
    );
    let first = editor.current.clone();
    assert_eq!(
        ids(contents(editor.project(), 1)),
        vec![73, 100, 101, 102, 103, 104, 40, 6, 19, 4, 12, 9, 25]
    );
    let mut expected = clipboard.contents.items[0].clone();
    expected.id = 100;
    let ContentsKind::Group(children) = &mut expected.kind else {
        panic!()
    };
    children[0].id = 101;
    children[1].id = 104;
    let ContentsKind::Group(inner) = &mut children[0].kind else {
        panic!()
    };
    inner[0].id = 102;
    inner[1].id = 103;
    assert_eq!(contents(editor.project(), 1).node(100).unwrap(), &expected);
    editor.undo();
    assert_eq!(editor.current, before);
    assert_eq!(editor.undo, previous_undo);
    editor.redo();
    assert_eq!(editor.current, first);
    assert_eq!(
        editor.paste_contents(1, 100, 2, &clipboard).unwrap(),
        vec![105]
    );
    assert_eq!(contents(editor.project(), 1).next_id, 110);
    assert_eq!(
        contents(editor.project(), 1).node(105).unwrap().name,
        expected.name
    );
    let bytes = project_file::encode(editor.project(), None).unwrap();
    assert_eq!(
        project_file::decode(&bytes).unwrap().project,
        *editor.project()
    );
}

#[test]
fn contents_clipboard_preserves_dormant_scalar_handles_unused_path_poses_and_colors() {
    let path = VectorPath {
        closed: true,
        vertices: vec![
            PathVertex::corner([0., 0.]),
            PathVertex::corner([40., 0.]),
            PathVertex::corner([0., 40.]),
        ],
    };
    let mut unused = path.clone();
    unused.vertices[0].position = [0.125, -0.0];
    let animation: PathAnimation = serde_json::from_value(serde_json::json!({
        "poses": [path.clone(), unused, path.clone()],
        "timing": {"value": 2.0, "keys": {"2": {"value":0.0,"interpolation":"Hold"},"29":{"value":2.0,"interpolation":"Smooth"}}}
    })).unwrap();
    let path_node = node(7, ContentsKind::Path { path, animation });
    let mut shape = node(31, ContentsKind::Parametric(ShapeKind::Star));
    shape.enabled = false;
    let track = shape.parameters.get_mut(&ContentsParam::Width).unwrap();
    track.value = 333.125;
    track.keys.insert(
        3,
        Keyframe {
            value: 50.,
            interpolation: Interpolation::Bezier(Bezier {
                x1: 0.25,
                y1: -0.5,
                x2: 0.75,
                y2: 1.25,
            }),
            temporal: TemporalHandles {
                incoming: Some(TemporalHandle {
                    slope: -0.125,
                    influence: 0.333,
                }),
                outgoing: Some(TemporalHandle {
                    slope: 0.25,
                    influence: 0.777,
                }),
                ..Default::default()
            },
        },
    );
    track.keys.insert(
        27,
        Keyframe {
            value: 150.,
            interpolation: Interpolation::Hold,
            temporal: TemporalHandles {
                mode: TemporalMode::Auto,
                ..Default::default()
            },
        },
    );
    let mut paint = node(
        18,
        ContentsKind::GradientStroke {
            style: ShapeStroke::default(),
            gradient: ShapeGradient::default(),
        },
    );
    paint.composite = PaintComposite::AbovePrevious;
    paint.blend = PaintBlend::Multiply;
    gradient_colors::edit(
        &mut paint,
        &GradientColorsEdit::SetAnimation {
            frame: 5,
            enabled: true,
        },
        300,
    )
    .unwrap();
    let mut colors = paint.kind.gradient().unwrap().colors_at(&paint, 5);
    colors.colors[0].red = 0.125;
    colors.colors.push(GradientColorStop {
        id: 7,
        position: 40.,
        midpoint: 30.,
        red: 33.25,
        green: 145.5,
        blue: 223.75,
    });
    colors.opacities.swap(0, 1);
    gradient_colors::edit(
        &mut paint,
        &GradientColorsEdit::Set { frame: 35, colors },
        300,
    )
    .unwrap();
    let mut editor = scene(
        vec![node(60, ContentsKind::Group(vec![path_node, shape, paint]))],
        90,
        54,
    );
    let snapshot = editor.copy_contents(1, 0, &[60]).unwrap();
    assert_eq!(editor.paste_contents(2, 0, 0, &snapshot).unwrap(), vec![1]);
    let original = contents(editor.project(), 1).node(60).unwrap();
    let cloned = contents(editor.project(), 2).node(1).unwrap();
    let (ContentsKind::Group(a), ContentsKind::Group(b)) = (&original.kind, &cloned.kind) else {
        panic!()
    };
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.parameters, b.parameters);
        assert_eq!(a.name, b.name);
        assert_eq!(a.enabled, b.enabled);
        let mut expected = a.clone();
        expected.id = b.id;
        assert_eq!(
            serde_json::to_vec(&expected).unwrap(),
            serde_json::to_vec(b).unwrap()
        );
    }
    assert_eq!(snapshot.contents.items[0], *original);
    assert_eq!(
        Project::from_json(&editor.project().to_json().unwrap()).unwrap(),
        *editor.project()
    );
}

#[test]
fn contents_clipboard_schema54_snapshot_promotes_after_undo_only_as_required() {
    let gradient = node(
        8,
        ContentsKind::GradientFill {
            even_odd: false,
            gradient: ShapeGradient::default(),
        },
    );
    let mut editor = scene(vec![gradient], 20, 53);
    let before = editor.current.clone();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::GradientColors {
                item: 8,
                edit: GradientColorsEdit::SetAnimation {
                    frame: 12,
                    enabled: true,
                },
            },
        })
        .unwrap();
    let snapshot = editor.copy_contents(1, 0, &[8]).unwrap();
    assert_eq!(snapshot.required_version, 54);
    editor.undo();
    assert_eq!(editor.current, before);
    editor.paste_contents(2, 0, 0, &snapshot).unwrap();
    assert_eq!(editor.project().version, 54);
    assert!(
        contents(editor.project(), 2)
            .node(1)
            .unwrap()
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .is_some()
    );
    editor.undo();
    assert_eq!(editor.current, before);
    assert!(editor.can_redo());
}

#[test]
fn contents_clipboard_cut_and_paste_preserve_legacy_schema_assets_and_inactive_sources() {
    for version in [43, 44, 53, 54] {
        let group = node(
            8,
            ContentsKind::Group(vec![node(12, ContentsKind::Fill { even_odd: true })]),
        );
        let mut editor = scene(vec![group], 20, version.max(44));
        editor
            .execute(Command::ImportAsset {
                content: Content::Image { png: "YWJj".into() },
                width: 64.,
                height: 48.,
                name: "Unreferenced asset".into(),
                folder: None,
                frame: None,
            })
            .unwrap();
        editor.current.project.version = version;
        if version == 43 {
            let group = contents_mut(&mut editor.current.project, 1)
                .node_mut(8)
                .unwrap();
            group.parameters.remove(&ContentsParam::Skew);
            group.parameters.remove(&ContentsParam::SkewAxis);
        }
        let mut inactive = editor.project().composition.clone();
        for layer in &mut inactive.layers {
            layer.id += 10;
        }
        editor
            .current
            .project
            .other_compositions
            .insert(2, inactive.clone());
        editor.current.project.next_composition_id = 3;
        editor.current.project.next_layer_id = 13;
        editor.project().validate().unwrap();
        let assets = editor.project().asset_library.clone();
        let clipboard = editor.copy_contents(1, 0, &[8]).unwrap();
        editor.execute(remove(1, 0, &[8])).unwrap();
        assert_eq!(editor.project().version, version);
        assert_eq!(editor.project().asset_library, assets);
        editor.paste_contents(1, 0, 0, &clipboard).unwrap();
        assert_eq!(editor.project().version, version);
        assert_eq!(editor.project().asset_library, assets);
        assert_eq!(editor.project().other_compositions[&2], inactive);
    }
    let mut source = scene(
        vec![node(5, ContentsKind::Fill { even_odd: false })],
        10,
        54,
    );
    let clipboard = source.copy_contents(1, 0, &[5]).unwrap();
    source.current.project.version = 43;
    source.paste_contents(2, 0, 0, &clipboard).unwrap();
    assert_eq!(
        source.project().version,
        43,
        "source declared 54 does not force unrelated version promotion"
    );
}

#[test]
fn contents_clipboard_invalid_source_selections_and_destinations_are_atomic() {
    let mut editor = fixture();
    let clipboard = editor.copy_contents(1, 0, &[40]).unwrap();
    for (parent, items) in [
        (0, vec![]),
        (0, vec![40, 40]),
        (0, vec![40, 19]),
        (40, vec![6, 9]),
        (19, vec![19]),
        (999, vec![19]),
        (0, vec![0]),
        (0, vec![999]),
    ] {
        let (current, undo, redo) = (
            editor.current.clone(),
            editor.undo.clone(),
            editor.redo.clone(),
        );
        assert!(editor.copy_contents(1, parent, &items).is_err());
        assert_unchanged(&editor, &current, &undo, &redo);
        reject(&mut editor, remove(1, parent, &items));
    }
    for (id, parent, index) in [(1, 999, 0), (1, 19, 0), (1, 0, 5), (1, 40, 3), (999, 0, 0)] {
        reject(&mut editor, paste(id, parent, index, &clipboard));
    }
    assert!(editor.copy_contents(999, 0, &[40]).is_err());
    reject(&mut editor, remove(999, 0, &[40]));
    let mut empty = clipboard.clone();
    empty.contents.items.clear();
    reject(&mut editor, paste(1, 0, 0, &empty));
    let mut invalid = clipboard.clone();
    invalid.contents.items[0].id = 0;
    reject(&mut editor, paste(1, 0, 0, &invalid));
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == 1)
        .unwrap()
        .locked = true;
    assert!(editor.copy_contents(1, 0, &[40]).is_err());
    reject(&mut editor, remove(1, 0, &[40]));
    reject(&mut editor, paste(1, 0, 0, &clipboard));
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == 1)
        .unwrap()
        .locked = false;
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == 1)
        .unwrap()
        .content = Content::Rectangle;
    assert!(editor.copy_contents(1, 0, &[40]).is_err());
    reject(&mut editor, remove(1, 0, &[40]));
    reject(&mut editor, paste(1, 0, 0, &clipboard));
}

#[test]
fn contents_clipboard_same_composition_and_fps_and_current_duration_are_required() {
    let mut shape = node(9, ContentsKind::Parametric(ShapeKind::Rectangle));
    shape
        .parameters
        .get_mut(&ContentsParam::Width)
        .unwrap()
        .keys
        .insert(
            29,
            Keyframe {
                value: 50.,
                interpolation: Interpolation::Linear,
                temporal: Default::default(),
            },
        );
    let mut editor = scene(vec![shape], 20, 53);
    let clipboard = editor.copy_contents(1, 0, &[9]).unwrap();
    editor.current.project.composition.fps = FrameRate::new(24, 1).unwrap();
    reject(&mut editor, paste(2, 0, 0, &clipboard));
    editor.current.project.composition.fps = clipboard.fps;
    editor.execute(remove(1, 0, &[9])).unwrap();
    editor.current.project.composition.duration = 29;
    reject(&mut editor, paste(2, 0, 0, &clipboard));
    editor.current.project.composition.duration = 30;
    editor.paste_contents(2, 0, 0, &clipboard).unwrap();
    assert_eq!(
        contents(editor.project(), 2).node(1).unwrap().parameters[&ContentsParam::Width]
            .keys
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![29]
    );
    editor.execute(Command::NewComposition).unwrap();
    reject(&mut editor, paste(2, 0, 0, &clipboard));
}

#[test]
fn contents_clipboard_node_depth_and_allocator_limits_reject_atomically() {
    let mut editor = fixture();
    let clipboard = editor.copy_contents(1, 0, &[40]).unwrap();
    contents_mut(&mut editor.current.project, 2).next_id = u64::MAX - 3;
    reject(&mut editor, paste(2, 0, 0, &clipboard));
    contents_mut(&mut editor.current.project, 2).next_id = 1;
    let fill = node(1, ContentsKind::Fill { even_odd: false });
    contents_mut(&mut editor.current.project, 2).items = (1..=252)
        .map(|id| {
            let mut n = fill.clone();
            n.id = id;
            n
        })
        .collect();
    contents_mut(&mut editor.current.project, 2).next_id = 253;
    reject(&mut editor, paste(2, 0, 0, &clipboard));
    contents_mut(&mut editor.current.project, 2).items.pop();
    editor.paste_contents(2, 0, 0, &clipboard).unwrap();
    assert_eq!(contents(editor.project(), 2).rows().len(), 256);
    let mut inner = node(8, ContentsKind::Group(vec![]));
    for id in (1..8).rev() {
        inner = node(id, ContentsKind::Group(vec![inner]));
    }
    *contents_mut(&mut editor.current.project, 2) = ShapeContents {
        items: vec![inner],
        next_id: 9,
    };
    editor.project().validate().unwrap();
    reject(&mut editor, paste(2, 8, 0, &clipboard));
}

#[test]
fn contents_clipboard_rejects_invalid_original_even_if_removal_would_repair_it() {
    for variant in 0..5 {
        let mut editor = fixture();
        let clipboard = editor.copy_contents(1, 0, &[40]).unwrap();
        match variant {
            0 => contents_mut(&mut editor.current.project, 1)
                .node_mut(19)
                .unwrap()
                .name
                .clear(),
            1 => contents_mut(&mut editor.current.project, 1).next_id = 40,
            2 => {
                editor.current.project.version = 43;
            }
            3 => {
                contents_mut(&mut editor.current.project, 1)
                    .node_mut(19)
                    .unwrap()
                    .parameters
                    .get_mut(&ContentsParam::Width)
                    .unwrap()
                    .keys
                    .insert(
                        300,
                        Keyframe {
                            value: 10.,
                            interpolation: Interpolation::Linear,
                            temporal: Default::default(),
                        },
                    );
            }
            _ => {
                contents_mut(&mut editor.current.project, 1)
                    .node_mut(19)
                    .unwrap()
                    .id = 9;
            }
        }
        assert!(editor.copy_contents(1, 0, &[40]).is_err());
        reject(&mut editor, remove(1, 0, &[40]));
        reject(&mut editor, paste(2, 0, 0, &clipboard));
    }
}

#[test]
fn contents_clipboard_pure_batches_are_atomic_and_mixed_empty_or_excessive_batches_reject() {
    let mut editor = fixture();
    let clipboard = editor.copy_contents(1, 0, &[40]).unwrap();
    let before = editor.current.clone();
    let undo = editor.undo.len();
    editor
        .execute(Command::Batch(vec![
            remove(1, 0, &[40]),
            Command::Batch(vec![paste(2, 0, 0, &clipboard)]),
        ]))
        .unwrap();
    assert_eq!(editor.undo.len(), undo + 1);
    editor.undo();
    assert_eq!(editor.current, before);
    reject(
        &mut editor,
        Command::Batch(vec![remove(1, 0, &[40]), remove(1, 0, &[999])]),
    );
    reject(
        &mut editor,
        Command::Batch(vec![paste(2, 0, 0, &clipboard), Command::ToggleVisible(1)]),
    );
    reject(
        &mut editor,
        Command::Batch(vec![paste(2, 0, 0, &clipboard), Command::Batch(vec![])]),
    );
    let mut deep = paste(2, 0, 0, &clipboard);
    for _ in 0..65 {
        deep = Command::Batch(vec![deep]);
    }
    reject(&mut editor, deep);
    reject(
        &mut editor,
        Command::Batch(vec![remove(1, 0, &[40]); 10000]),
    );
}

#[test]
fn contents_clipboard_key_limit_and_invalid_unused_path_payloads_reject_original() {
    let shape = node(5, ContentsKind::Parametric(ShapeKind::Rectangle));
    let mut editor = scene(vec![shape], 10, 53);
    editor.current.project.composition.duration = 10002;
    let track = contents_mut(&mut editor.current.project, 1)
        .node_mut(5)
        .unwrap()
        .parameters
        .get_mut(&ContentsParam::Width)
        .unwrap();
    track.keys = (0..10000)
        .map(|frame| {
            (
                frame,
                Keyframe {
                    value: 50.,
                    interpolation: Interpolation::Linear,
                    temporal: Default::default(),
                },
            )
        })
        .collect();
    let clipboard = editor.copy_contents(1, 0, &[5]).unwrap();
    editor.paste_contents(2, 0, 0, &clipboard).unwrap();
    let extra = Keyframe {
        value: 51.,
        interpolation: Interpolation::Hold,
        temporal: Default::default(),
    };
    contents_mut(&mut editor.current.project, 1)
        .node_mut(5)
        .unwrap()
        .parameters
        .get_mut(&ContentsParam::Width)
        .unwrap()
        .keys
        .insert(10000, extra);
    assert!(editor.copy_contents(1, 0, &[5]).is_err());
    reject(&mut editor, remove(1, 0, &[5]));
    reject(&mut editor, paste(2, 0, 0, &clipboard));

    let path = VectorPath {
        closed: false,
        vertices: vec![PathVertex::corner([0., 0.]), PathVertex::corner([10., 20.])],
    };
    let mut editor = scene(
        vec![node(
            5,
            ContentsKind::Path {
                path: path.clone(),
                animation: Default::default(),
            },
        )],
        10,
        53,
    );
    let clipboard = editor.copy_contents(1, 0, &[5]).unwrap();
    let mut invalid_unused = path.clone();
    invalid_unused.vertices[1].position = [1_000_001., 20.];
    let bad: PathAnimation = serde_json::from_value(
        serde_json::json!({"poses":[path,invalid_unused],"timing":{"value":0.,"keys":{}}}),
    )
    .unwrap();
    let ContentsKind::Path { animation, .. } = &mut contents_mut(&mut editor.current.project, 1)
        .node_mut(5)
        .unwrap()
        .kind
    else {
        panic!()
    };
    *animation = bad;
    assert!(editor.copy_contents(1, 0, &[5]).is_err());
    reject(&mut editor, remove(1, 0, &[5]));
    reject(&mut editor, paste(2, 0, 0, &clipboard));
}

#[test]
fn contents_clipboard_compound_key_duration_is_checked_after_source_deletion() {
    let mut paint = node(
        7,
        ContentsKind::GradientFill {
            even_odd: false,
            gradient: ShapeGradient::default(),
        },
    );
    gradient_colors::edit(
        &mut paint,
        &GradientColorsEdit::SetAnimation {
            frame: 39,
            enabled: true,
        },
        300,
    )
    .unwrap();
    let mut editor = scene(vec![paint], 10, 54);
    let clipboard = editor.copy_contents(1, 0, &[7]).unwrap();
    editor.execute(remove(1, 0, &[7])).unwrap();
    editor.current.project.composition.duration = 39;
    reject(&mut editor, paste(2, 0, 0, &clipboard));
    editor.current.project.composition.duration = 40;
    editor.paste_contents(2, 0, 0, &clipboard).unwrap();
    let pasted = contents(editor.project(), 2).node(1).unwrap();
    assert_eq!(pasted.kind, clipboard.contents.items[0].kind);
    editor.current.project.composition.duration = 100;
    editor.paste_contents(2, 0, 1, &clipboard).unwrap();
    assert_eq!(
        contents(editor.project(), 2).node(2).unwrap().kind,
        pasted_kind(&clipboard)
    );
}
fn pasted_kind(clipboard: &ContentsClipboard) -> ContentsKind {
    clipboard.contents.items[0].kind.clone()
}

#[test]
fn contents_clipboard_promotion_uses_each_existing_feature_minimum_only() {
    let mut composite = node(7, ContentsKind::Fill { even_odd: false });
    composite.composite = PaintComposite::AbovePrevious;
    let mut blend = composite.clone();
    blend.blend = PaintBlend::Multiply;
    let mut compound = node(
        7,
        ContentsKind::GradientFill {
            even_odd: false,
            gradient: ShapeGradient::default(),
        },
    );
    gradient_colors::edit(
        &mut compound,
        &GradientColorsEdit::SetAnimation {
            frame: 0,
            enabled: true,
        },
        300,
    )
    .unwrap();
    for (node, minimum) in [
        (node(7, ContentsKind::Fill { even_odd: false }), 43),
        (node(7, ContentsKind::Group(vec![])), 44),
        (
            node(
                7,
                ContentsKind::GradientFill {
                    even_odd: false,
                    gradient: ShapeGradient::default(),
                },
            ),
            45,
        ),
        (composite, 46),
        (blend, 47),
        (node(7, ContentsKind::TrimPaths), 50),
        (compound, 54),
    ] {
        let mut editor = scene(vec![node], 10, 54);
        let clipboard = editor.copy_contents(1, 0, &[7]).unwrap();
        editor.execute(remove(1, 0, &[7])).unwrap();
        editor.current.project.version = 43;
        editor.paste_contents(2, 0, 0, &clipboard).unwrap();
        assert_eq!(editor.project().version, minimum);
    }
}
