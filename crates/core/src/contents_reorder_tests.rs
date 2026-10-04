use super::*;
#[path = "contents_move_tests.rs"]
mod move_siblings;

fn reorder(parent: u64, order: &[u64]) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::Reorder {
            parent,
            order: order.to_vec(),
        },
    }
}

fn children(project: &Project, parent: u64) -> &[ContentsNode] {
    let Content::ShapeContents(contents) = &project.composition.layer(1).unwrap().content else {
        panic!()
    };
    if parent == 0 {
        &contents.items
    } else {
        let ContentsKind::Group(children) = &contents.node(parent).unwrap().kind else {
            panic!()
        };
        children
    }
}

fn ids(project: &Project, parent: u64) -> Vec<u64> {
    children(project, parent)
        .iter()
        .map(|node| node.id)
        .collect()
}

fn reorder_scene() -> Editor {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    edit(&mut e, ContentsEdit::ConvertPath { item: 2, frame: 0 });
    for parent in [0, 0, 1, 7] {
        edit(
            &mut e,
            ContentsEdit::Add {
                parent,
                kind: ContentsKind::Group(vec![]),
            },
        );
    }
    for (item, index) in [(2, 1), (3, 2)] {
        edit(
            &mut e,
            ContentsEdit::Move {
                item,
                parent: 7,
                index,
            },
        );
    }
    // IDs deliberately disagree with both creation and tree order.
    let Content::ShapeContents(c) = &mut e.current.project.composition.layers[0].content else {
        panic!()
    };
    fn rekey(nodes: &mut [ContentsNode]) {
        for node in nodes {
            node.id = [0, 60, 91, 12, 84, 23, 7, 35, 51][node.id as usize];
            if let ContentsKind::Group(children) = &mut node.kind {
                rekey(children);
            }
        }
    }
    rekey(&mut c.items);
    let mut value = serde_json::to_value(&c).unwrap();
    value["next_id"] = 200.into();
    *c = serde_json::from_value(value).unwrap();
    e.current.project.version = PROJECT_VERSION;
    e.current.project.validate().unwrap();
    e.clear_history();
    e
}

fn clone_editor(e: &Editor) -> Editor {
    Editor {
        current: e.current.clone(),
        undo: e.undo.clone(),
        redo: e.redo.clone(),
    }
}

fn assert_same_editor(actual: &Editor, expected: &Editor) {
    assert_eq!(actual.current, expected.current);
    assert_eq!(actual.undo, expected.undo);
    assert_eq!(actual.redo, expected.redo);
}

#[test]
fn contents_reorder_root_and_nested_preserve_whole_siblings_and_one_undo_redo() {
    for (parent, order) in [
        (0, vec![60, 7, 23]),
        (60, vec![84, 35]),
        (35, vec![12, 51, 91]),
    ] {
        let mut e = reorder_scene();
        let before = e.current.clone();
        let by_id = children(e.project(), parent)
            .iter()
            .map(|node| (node.id, node.clone()))
            .collect::<BTreeMap<_, _>>();
        e.execute(reorder(parent, &order)).unwrap();
        assert_eq!(ids(e.project(), parent), order);
        for node in children(e.project(), parent) {
            assert_eq!(node, &by_id[&node.id]);
        }
        let after = e.current.clone();
        assert_eq!(e.undo, vec![before.clone()]);
        assert!(e.redo.is_empty());
        e.undo();
        assert_eq!(e.current, before);
        assert!(!e.can_undo());
        e.redo();
        assert_eq!(e.current, after);
        assert!(!e.can_redo());
    }
}

#[test]
fn contents_reorder_empty_singleton_and_actual_noop_preserve_both_history_stacks() {
    let mut e = reorder_scene();
    e.execute(reorder(0, &[60, 7, 23])).unwrap();
    e.execute(reorder(60, &[84, 35])).unwrap();
    e.undo();
    assert!(e.can_undo() && e.can_redo());
    for parent in [0, 60, 35, 51, 7, 23] {
        let order = ids(e.project(), parent);
        let before = clone_editor(&e);
        e.execute(reorder(parent, &order)).unwrap();
        assert_same_editor(&e, &before);
    }
    for content in [
        ShapeContents::default(),
        contents(&scene_with_single_group()).clone(),
    ] {
        let mut single = Editor::default();
        single
            .execute(Command::AddContent {
                content: Content::ShapeContents(content),
                width: 100.,
                height: 100.,
                name: "Small".into(),
            })
            .unwrap();
        let order = ids(single.project(), 0);
        let before = clone_editor(&single);
        single.execute(reorder(0, &order)).unwrap();
        assert_same_editor(&single, &before);
    }
}

fn scene_with_single_group() -> Editor {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    e
}

#[test]
fn contents_reorder_rejects_invalid_permutations_and_parents_atomically() {
    let mut e = reorder_scene();
    e.execute(reorder(0, &[60, 7, 23])).unwrap();
    e.execute(reorder(60, &[84, 35])).unwrap();
    e.undo();
    let before = clone_editor(&e);
    for (parent, order) in [
        (0, vec![]),
        (0, vec![60, 7]),
        (0, vec![60, 7, 23, 35]),
        (0, vec![60, 7, 7]),
        (0, vec![60, 7, 999]),
        (0, vec![60, 7, 35]),
        (60, vec![35, 91]),
        (35, vec![12, 51, 84]),
        (35, vec![12, 51, 35]),
        (12, vec![]),
        (91, vec![]),
        (999, vec![]),
        (51, vec![91]),
    ] {
        assert!(
            e.execute(reorder(parent, &order)).is_err(),
            "{parent}: {order:?}"
        );
        assert_same_editor(&e, &before);
    }
}

#[test]
fn contents_reorder_rejects_locked_wrong_content_and_stale_layers_without_history_changes() {
    for kind in 0..4 {
        let mut e = reorder_scene();
        e.execute(reorder(0, &[60, 7, 23])).unwrap();
        e.execute(reorder(60, &[84, 35])).unwrap();
        e.undo();
        match kind {
            0 => e.current.project.composition.layers[0].locked = true,
            1 => e.current.project.composition.layers[0].content = Content::Rectangle,
            2 => e.current.project.composition.layers[0].content = Content::Shape(Shape::default()),
            _ => {}
        }
        for order in [vec![7, 23, 60], vec![60, 7, 23]] {
            let before = clone_editor(&e);
            let command = Command::Contents {
                id: if kind == 3 { 999 } else { 1 },
                edit: ContentsEdit::Reorder { parent: 0, order },
            };
            assert!(e.execute(command).is_err());
            assert_same_editor(&e, &before);
        }
    }
}

fn rich_track(values: [f64; 4]) -> AnimatedProperty {
    AnimatedProperty {
        value: values[2],
        keys: [10, 30, 70, 90]
            .into_iter()
            .zip(values)
            .zip([
                Interpolation::Hold,
                Interpolation::Bezier(Bezier {
                    x1: 0.17,
                    y1: -0.3,
                    x2: 0.82,
                    y2: 1.2,
                }),
                Interpolation::Smooth,
                Interpolation::Linear,
            ])
            .map(|((frame, value), interpolation)| {
                (
                    frame,
                    Keyframe {
                        value,
                        interpolation,
                        temporal: TemporalHandles {
                            mode: TemporalMode::Independent,
                            incoming: Some(TemporalHandle {
                                slope: -0.21,
                                influence: 0.31,
                            }),
                            outgoing: Some(TemporalHandle {
                                slope: 0.43,
                                influence: 0.62,
                            }),
                        },
                    },
                )
            })
            .collect(),
    }
}

fn rich_scene() -> Editor {
    let mut e = reorder_scene();
    let gradient: ShapeGradient = serde_json::from_value(serde_json::json!({
        "radial": true, "colors": [9, 2, 27], "opacities": [5, 19], "next_stop": 80
    }))
    .unwrap();
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 35,
            kind: ContentsKind::GradientStroke {
                gradient: gradient.clone(),
                style: ShapeStroke {
                    cap: StrokeCap::Square,
                    join: StrokeJoin::Bevel,
                    miter_limit: 13.,
                    dashes: vec![11., 5., 2.],
                    dash_offset: -17.,
                },
            },
        },
    );
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::GradientFill {
                even_odd: true,
                gradient,
            },
        },
    );
    let layer = &mut e.current.project.composition.layers[0];
    layer.transform_offset = Affine([1.1, 0.2, -0.3, 0.9, 16., -37.]);
    let Content::ShapeContents(c) = &mut layer.content else {
        panic!()
    };
    for id in [60, 35] {
        let node = c.node_mut(id).unwrap();
        node.parameters.insert(
            ContentsParam::Transform(Property::Rotation),
            rich_track([31., 57., -14., 81.]),
        );
        node.parameters.insert(
            ContentsParam::Transform(Property::ScaleX),
            rich_track([-110., -70., -140., -90.]),
        );
        node.parameters
            .insert(ContentsParam::Skew, rich_track([11., -31., 41., 20.]));
        node.parameters
            .insert(ContentsParam::SkewAxis, rich_track([17., 25., 81., -12.]));
    }
    c.node_mut(23).unwrap().enabled = false;
    c.node_mut(51).unwrap().name = "Empty \"group\" 雪\n".into();
    for id in [12, 84, 200, 201] {
        let node = c.node_mut(id).unwrap();
        node.blend = PaintBlend::Multiply;
        node.composite = PaintComposite::AbovePrevious;
        for track in node.parameters.values_mut() {
            *track = rich_track([track.value; 4]);
        }
    }
    let ContentsKind::Path { path, animation } = &mut c.node_mut(91).unwrap().kind else {
        panic!()
    };
    for (i, vertex) in path.vertices.iter_mut().enumerate() {
        vertex.incoming = [-2.5 - i as f64, 7.75];
        vertex.outgoing = [5.25, -1.5 - i as f64];
    }
    let poses = [17., -31., 46., 46.]
        .into_iter()
        .map(|shift| {
            let mut pose = path.clone();
            for vertex in &mut pose.vertices {
                vertex.position[0] += shift;
                vertex.incoming[1] -= shift * 0.2;
                vertex.outgoing[0] += shift * 0.3;
            }
            pose
        })
        .collect::<Vec<_>>();
    // Slot three is a duplicate unused pose; neither it nor pose references may be rewritten.
    let mut timing = rich_track([2., 0., 1., 1.]);
    for key in timing.keys.values_mut() {
        key.temporal = TemporalHandles::default();
    }
    *animation = serde_json::from_value(serde_json::json!({
        "poses": poses, "timing": timing
    }))
    .unwrap();
    e.current.project.version = PROJECT_VERSION;
    e.current.project.validate().unwrap();
    e.clear_history();
    e
}

fn expected_reorder(project: &mut Project, parent: u64, order: &[u64]) {
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
    let nodes = if parent == 0 {
        &mut c.items
    } else {
        let ContentsKind::Group(nodes) = &mut c.node_mut(parent).unwrap().kind else {
            panic!()
        };
        nodes
    };
    let original = nodes.clone();
    *nodes = order
        .iter()
        .map(|id| original.iter().find(|n| n.id == *id).unwrap().clone())
        .collect();
}

#[test]
fn contents_reorder_preserves_animation_transforms_paints_and_stable_property_references() {
    for (parent, order) in [
        (0, vec![60, 201, 7, 23]),
        (60, vec![84, 35]),
        (35, vec![200, 12, 51, 91]),
    ] {
        let mut e = rich_scene();
        let before = e.project().clone();
        let layer = before.composition.layer(1).unwrap();
        let references = layer
            .track_paths()
            .into_iter()
            .map(|path| {
                let track = layer.track(path).unwrap();
                (path, layer.track_label(path), track.clone())
            })
            .collect::<Vec<_>>();
        let keys = references
            .iter()
            .flat_map(|(path, _, track)| {
                track.keys.keys().map(|frame| KeyRef {
                    id: 1,
                    property: *path,
                    frame: *frame,
                })
            })
            .collect::<Vec<_>>();
        assert!(!keys.is_empty());
        let mut expected = before.clone();
        expected_reorder(&mut expected, parent, &order);
        e.execute(reorder(parent, &order)).unwrap();
        // Exact whole-project serialization covers next IDs, all node fields,
        // disabled subtrees, base geometry, every pose and unrelated layer data.
        assert_eq!(
            serde_json::to_vec(e.project()).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        let layer = e.project().composition.layer(1).unwrap();
        for (path, label, track) in references {
            assert_eq!(layer.track_label(path), label);
            assert_eq!(layer.track(path), Some(&track));
        }
        for key in keys {
            assert_eq!(
                layer.track(key.property).unwrap().keys().get(&key.frame),
                before
                    .composition
                    .layer(key.id)
                    .unwrap()
                    .track(key.property)
                    .unwrap()
                    .keys()
                    .get(&key.frame)
            );
        }
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &expected);
    }
}

#[test]
fn contents_reorder_native_lep_roundtrip_retains_order_payloads_and_container_version() {
    let mut e = rich_scene();
    e.execute(Command::Batch(vec![
        reorder(0, &[201, 60, 23, 7]),
        Command::Batch(vec![reorder(35, &[12, 51, 91, 200])]),
    ]))
    .unwrap();
    let bytes = project_file::encode(e.project(), Some(br#"{"frame":30}"#)).unwrap();
    assert_eq!(&bytes[..8], project_file::MAGIC);
    assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
    let decoded = project_file::decode(&bytes).unwrap();
    assert_eq!(decoded.project, *e.project());
    assert_eq!(decoded.view, Some(br#"{"frame":30}"#.as_slice()));
    assert_eq!(
        project_file::encode(&decoded.project, decoded.view).unwrap(),
        bytes
    );
}

fn historical_scene(version: u32) -> Editor {
    let mut e = reorder_scene();
    for (png, frame) in [("YWJj", Some(0)), ("ZGVm", None)] {
        e.execute(Command::ImportAsset {
            content: Content::Image { png: png.into() },
            width: 64.,
            height: 48.,
            name: format!("Asset {png}"),
            folder: None,
            frame,
        })
        .unwrap();
    }
    e.execute(Command::DuplicateLayers(vec![2])).unwrap();
    for color in [0x123456, 0xabcdef] {
        e.execute(Command::SetColor { id: 2, color }).unwrap();
    }
    e.undo();
    e.select(1);
    if version == 43 {
        let Content::ShapeContents(c) = &mut e
            .current
            .project
            .composition
            .layers
            .iter_mut()
            .find(|l| l.id == 1)
            .unwrap()
            .content
        else {
            panic!()
        };
        fn remove_skew(nodes: &mut [ContentsNode]) {
            for node in nodes {
                node.parameters.remove(&ContentsParam::Skew);
                node.parameters.remove(&ContentsParam::SkewAxis);
                if let ContentsKind::Group(children) = &mut node.kind {
                    remove_skew(children);
                }
            }
        }
        remove_skew(&mut c.items);
    }
    e.current.project.version = version;
    e.current.project.validate().unwrap();
    assert!(e.can_undo() && e.can_redo());
    assert_eq!(e.project().asset_library().assets().len(), 2);
    e
}

#[test]
fn contents_reorder_direct_and_nested_batches_preserve_historical_schema_and_assets() {
    for version in [43, 44, 45, PROJECT_VERSION] {
        for (parent, order) in [
            (0, vec![60, 7, 23]),
            (60, vec![84, 35]),
            (35, vec![12, 91, 51]),
        ] {
            let start = historical_scene(version);
            let mut expected = start.current.clone();
            expected_reorder(&mut expected.project, parent, &order);
            for command in [
                reorder(parent, &order),
                Command::Batch(vec![reorder(parent, &order)]),
                Command::Batch(vec![Command::Batch(vec![Command::Batch(vec![reorder(
                    parent, &order,
                )])])]),
            ] {
                let mut e = clone_editor(&start);
                e.execute(command).unwrap();
                assert_eq!(e.current, expected);
                assert_eq!(e.project().version, version);
                assert_eq!(e.project().asset_library(), start.project().asset_library());
                assert_eq!(e.project().composition.layer(3).unwrap().asset, Some(1));
                assert_eq!(e.undo.len(), start.undo.len() + 1);
                assert!(e.redo.is_empty());
                let after = clone_editor(&e);
                e.undo();
                assert_eq!(e.current, start.current);
                assert_eq!(e.undo, start.undo);
                e.redo();
                assert_same_editor(&e, &after);
            }
        }
    }
}

#[test]
fn contents_reorder_noop_and_failed_nested_batches_preserve_schema_assets_and_both_histories() {
    for version in [43, 44, 45, PROJECT_VERSION] {
        let start = historical_scene(version);
        for command in [
            reorder(0, &[7, 23, 60]),
            reorder(51, &[]),
            Command::Batch(vec![reorder(60, &[35, 84])]),
            Command::Batch(vec![Command::Batch(vec![Command::Batch(vec![reorder(
                35,
                &[51, 91, 12],
            )])])]),
            Command::Batch(vec![reorder(0, &[60, 7, 23]), reorder(0, &[7, 23, 60])]),
            Command::Batch(vec![
                Command::Batch(vec![reorder(35, &[12, 91, 51])]),
                Command::Batch(vec![Command::Batch(vec![reorder(35, &[51, 91, 12])])]),
            ]),
        ] {
            let mut e = clone_editor(&start);
            e.execute(command).unwrap();
            assert_same_editor(&e, &start);
        }
        for invalid in [
            reorder(60, &[35, 35]),
            reorder(51, &[91]),
            reorder(999, &[]),
        ] {
            let mut e = clone_editor(&start);
            assert!(
                e.execute(Command::Batch(vec![
                    reorder(0, &[60, 7, 23]),
                    Command::Batch(vec![Command::Batch(vec![invalid])])
                ]))
                .is_err()
            );
            assert_same_editor(&e, &start);
        }
    }
}

#[test]
fn contents_reorder_and_path_order_can_share_one_preservation_only_batch() {
    for version in [43, 44, PROJECT_VERSION] {
        let start = historical_scene(version);
        let path = || Command::ReorderPath {
            id: 1,
            target: PathTarget::Contents(91),
            order: PathOrder::Reverse,
        };
        let mut direct = clone_editor(&start);
        direct.execute(reorder(35, &[12, 91, 51])).unwrap();
        direct.execute(path()).unwrap();
        let mut batched = clone_editor(&start);
        batched
            .execute(Command::Batch(vec![
                Command::Batch(vec![path()]),
                reorder(35, &[12, 91, 51]),
            ]))
            .unwrap();
        assert_eq!(batched.current, direct.current);
        assert_eq!(batched.project().version, version);
        assert_eq!(batched.undo.len(), start.undo.len() + 1);
        batched.undo();
        assert_eq!(batched.current, start.current);
        let mut no_op = clone_editor(&start);
        no_op
            .execute(Command::Batch(vec![
                path(),
                Command::Batch(vec![reorder(35, &[12, 91, 51]), path()]),
                reorder(35, &[51, 91, 12]),
            ]))
            .unwrap();
        assert_same_editor(&no_op, &start);
    }
}

#[test]
fn contents_reorder_mixed_and_empty_batches_keep_existing_migration_and_history_behavior() {
    let start = historical_scene(PROJECT_VERSION);
    let rename = Command::Contents {
        id: 1,
        edit: ContentsEdit::Rename {
            item: 60,
            name: "Renamed".into(),
        },
    };
    let mut expected = clone_editor(&start);
    expected.execute(rename.clone()).unwrap();
    assert!(expected.project().version < PROJECT_VERSION);
    assert_eq!(
        expected.project().composition.layer(3).unwrap().asset,
        Some(1)
    );
    for command in [
        Command::Batch(vec![reorder(0, &[7, 23, 60]), rename.clone()]),
        Command::Batch(vec![
            Command::Batch(vec![reorder(0, &[60, 7, 23])]),
            Command::Batch(vec![rename, reorder(0, &[7, 23, 60])]),
        ]),
    ] {
        let mut e = clone_editor(&start);
        e.execute(command).unwrap();
        assert_same_editor(&e, &expected);
    }
    let mut expected_empty = clone_editor(&start);
    expected_empty.execute(Command::Batch(vec![])).unwrap();
    assert!(expected_empty.project().version < PROJECT_VERSION);
    assert_eq!(
        expected_empty.project().composition.layer(3).unwrap().asset,
        Some(1)
    );
    for command in [
        Command::Batch(vec![Command::Batch(vec![])]),
        Command::Batch(vec![
            reorder(0, &[7, 23, 60]),
            Command::Batch(vec![Command::Batch(vec![])]),
        ]),
        Command::Batch(vec![
            reorder(0, &[60, 7, 23]),
            Command::Batch(vec![]),
            reorder(0, &[7, 23, 60]),
        ]),
    ] {
        let mut e = clone_editor(&start);
        e.execute(command).unwrap();
        assert_same_editor(&e, &expected_empty);
    }
}

#[test]
fn contents_reorder_classifier_excludes_every_other_contents_variant_and_empty_batches() {
    for edit in [
        ContentsEdit::Promote,
        ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::Group(vec![]),
        },
        ContentsEdit::Remove(1),
        ContentsEdit::Duplicate(1),
        ContentsEdit::Move {
            item: 1,
            parent: 0,
            index: 0,
        },
        ContentsEdit::MoveSiblings {
            source_parent: 0,
            items: vec![1],
            parent: 7,
            index: 0,
        },
        ContentsEdit::Rename {
            item: 1,
            name: "Group".into(),
        },
        ContentsEdit::Enabled {
            item: 1,
            enabled: true,
        },
        ContentsEdit::Track {
            item: 1,
            parameter: ContentsParam::Skew,
            edit: TrackEdit::Value {
                frame: 0,
                value: 0.,
            },
        },
        ContentsEdit::ConvertPath { item: 1, frame: 0 },
        ContentsEdit::FillRule {
            item: 1,
            even_odd: false,
        },
        ContentsEdit::Blend {
            item: 1,
            mode: PaintBlend::Normal,
        },
        ContentsEdit::Composite {
            item: 1,
            mode: PaintComposite::BelowPrevious,
        },
        ContentsEdit::GradientType {
            item: 1,
            radial: false,
        },
        ContentsEdit::AddGradientStop {
            item: 1,
            opacity: false,
            position: 50.,
            frame: 0,
        },
        ContentsEdit::RemoveGradientStop { item: 1, stop: 1 },
        ContentsEdit::StrokeCap {
            item: 1,
            cap: StrokeCap::Butt,
        },
        ContentsEdit::StrokeJoin {
            item: 1,
            join: StrokeJoin::Round,
        },
        ContentsEdit::AddDash(1),
        ContentsEdit::RemoveDash(1),
    ] {
        let command = Command::Contents { id: 1, edit };
        assert!(!command.reorders_only(), "{command:?}");
        assert!(
            !Command::Batch(vec![reorder(0, &[]), Command::Batch(vec![command])]).reorders_only()
        );
    }
    for command in [
        Command::Batch(vec![]),
        Command::Batch(vec![Command::Batch(vec![])]),
        Command::Batch(vec![reorder(0, &[]), Command::Batch(vec![])]),
    ] {
        assert!(!command.reorders_only());
    }
    assert!(reorder(0, &[]).reorders_only());
    assert!(
        Command::Batch(vec![
            reorder(0, &[]),
            Command::Batch(vec![reorder(51, &[])])
        ])
        .reorders_only()
    );
}
