use super::*;

fn move_siblings(source_parent: u64, items: &[u64], parent: u64, index: usize) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::MoveSiblings {
            source_parent,
            items: items.to_vec(),
            parent,
            index,
        },
    }
}

fn mutable_children(project: &mut Project, parent: u64) -> &mut Vec<ContentsNode> {
    let Content::ShapeContents(c) = &mut project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap()
        .content
    else {
        panic!()
    };
    if parent == 0 {
        &mut c.items
    } else {
        let ContentsKind::Group(children) = &mut c.node_mut(parent).unwrap().kind else {
            panic!()
        };
        children
    }
}

fn expected_move(project: &mut Project, source: u64, items: &[u64], parent: u64, index: usize) {
    let original = children(project, source).to_vec();
    let moved = original
        .iter()
        .filter(|node| items.contains(&node.id))
        .cloned()
        .collect::<Vec<_>>();
    *mutable_children(project, source) = original
        .into_iter()
        .filter(|node| !items.contains(&node.id))
        .collect();
    for (offset, node) in moved.into_iter().enumerate() {
        mutable_children(project, parent).insert(index + offset, node);
    }
}

#[test]
fn contents_move_siblings_uses_source_order_and_retains_unselected_order_and_empty_groups() {
    for (source, items, parent, index, source_after, destination_after) in [
        (0, vec![23, 7], 60, 1, vec![60], vec![35, 7, 23, 84]),
        (60, vec![84, 35], 7, 0, vec![], vec![35, 84]),
        (35, vec![12, 51], 0, 1, vec![91], vec![7, 51, 12, 23, 60]),
        (35, vec![12, 91], 51, 0, vec![51], vec![91, 12]),
        (60, vec![35], 0, 3, vec![84], vec![7, 23, 60, 35]),
        (35, vec![91], 60, 1, vec![51, 12], vec![35, 91, 84]),
    ] {
        let mut e = reorder_scene();
        let mut expected = e.project().clone();
        expected_move(&mut expected, source, &items, parent, index);
        let moved = items
            .iter()
            .map(|id| (*id, contents(&e).node(*id).unwrap().clone()))
            .collect::<Vec<_>>();
        e.execute(move_siblings(source, &items, parent, index))
            .unwrap();
        assert_eq!(ids(e.project(), source), source_after);
        assert_eq!(ids(e.project(), parent), destination_after);
        assert_eq!(e.project(), &expected);
        for (id, node) in moved {
            assert_eq!(contents(&e).node(id), Some(&node));
        }
        if source != 0 {
            assert!(matches!(
                contents(&e).node(source).unwrap().kind,
                ContentsKind::Group(_)
            ));
        }
    }
}

#[test]
fn contents_move_siblings_retains_whole_animated_payloads_and_stable_track_references() {
    for (source, items, parent, index) in [
        (60, vec![84, 35], 23, 0),
        (35, vec![202, 200, 91, 51], 7, 0),
        (0, vec![201, 23], 35, 2),
    ] {
        let mut e = rich_scene();
        edit(
            &mut e,
            ContentsEdit::Add {
                parent: 35,
                kind: ContentsKind::TrimPaths,
            },
        );
        let Content::ShapeContents(c) = &mut e.current.project.composition.layers[0].content else {
            panic!()
        };
        for (parameter, values) in [
            (TrimParam::Start, [2., 30., 70., 90.]),
            (TrimParam::End, [95., 40., 20., 7.]),
            (TrimParam::Offset, [-370., 0., 390., 720.]),
        ] {
            c.node_mut(202)
                .unwrap()
                .parameters
                .insert(ContentsParam::Trim(parameter), rich_track(values));
        }
        e.current.project.version = PROJECT_VERSION;
        e.clear_history();
        // Animated/reflected and singular destination transforms never need an
        // inverse. The disabled destination also retains the complete payload.
        for node in mutable_children(&mut e.current.project, 0) {
            if node.id == 7 || node.id == 23 {
                node.parameters.insert(
                    ContentsParam::Transform(Property::ScaleX),
                    rich_track([-130., 0., 75., -22.]),
                );
            }
        }
        e.current.project.validate().unwrap();
        let before = e.current.clone();
        let layer = e.project().composition.layer(1).unwrap();
        let references = layer
            .track_paths()
            .into_iter()
            .map(|path| {
                (
                    path,
                    layer.track_label(path),
                    layer.track(path).unwrap().clone(),
                )
            })
            .collect::<Vec<_>>();
        let mut expected = before.project.clone();
        expected_move(&mut expected, source, &items, parent, index);
        e.execute(move_siblings(source, &items, parent, index))
            .unwrap();
        // Exact serialization includes next_id, unused path poses, timing,
        // interpolation/temporal handles, disabled nodes, paints and geometry.
        assert_eq!(
            serde_json::to_vec(e.project()).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        let layer = e.project().composition.layer(1).unwrap();
        for (path, label, track) in references {
            assert_eq!(layer.track_label(path), label);
            assert_eq!(layer.track(path), Some(&track));
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
fn contents_move_siblings_rejects_invalid_selections_parents_indices_and_cycles_atomically() {
    let start = historical_scene(PROJECT_VERSION);
    for (source, items, parent, index) in [
        (35, vec![], 7, 0),
        (35, vec![91, 91], 7, 0),
        (35, vec![0], 7, 0),
        (35, vec![999], 7, 0),
        (35, vec![91, 999], 7, 0),
        (35, vec![91, 84], 7, 0),
        (60, vec![35, 91], 7, 0),
        (999, vec![91], 7, 0),
        (91, vec![91], 7, 0),
        (35, vec![91], 999, 0),
        (35, vec![91], 84, 0),
        (35, vec![91], 7, 1),
        (35, vec![91], 0, 4),
        (35, vec![91], 0, usize::MAX),
        (35, vec![91], 35, 0),
        (0, vec![7], 0, 0),
        (0, vec![60], 60, 0),
        (0, vec![60], 35, 0),
        (0, vec![60], 51, 0),
        (60, vec![35], 51, 0),
        (35, vec![91, 51], 51, 0),
    ] {
        for nested in [false, true] {
            let mut e = clone_editor(&start);
            let command = move_siblings(source, &items, parent, index);
            let command = if nested {
                Command::Batch(vec![
                    reorder(0, &[60, 7, 23]),
                    Command::Batch(vec![Command::Batch(vec![command])]),
                ])
            } else {
                command
            };
            assert!(
                e.execute(command).is_err(),
                "{source} {items:?} {parent} {index}"
            );
            assert_same_editor(&e, &start);
        }
    }
}

#[test]
fn contents_move_siblings_rejects_locked_wrong_content_and_missing_layers() {
    for kind in 0..4 {
        let mut e = historical_scene(PROJECT_VERSION);
        // Imported and duplicated asset layers precede the original Contents
        // layer in this fixture; address the command target by its stable ID.
        let layer = e
            .current
            .project
            .composition
            .layers
            .iter_mut()
            .find(|layer| layer.id == 1)
            .unwrap();
        match kind {
            0 => layer.locked = true,
            1 => layer.content = Content::Rectangle,
            2 => layer.content = Content::Shape(Shape::default()),
            _ => {}
        }
        let before = clone_editor(&e);
        let mut command = move_siblings(35, &[91], 7, 0);
        if kind == 3 {
            let Command::Contents { id, .. } = &mut command else {
                panic!()
            };
            *id = 999;
        }
        assert!(e.execute(command).is_err(), "rejection case {kind}");
        assert_same_editor(&e, &before);
    }
}

#[test]
fn contents_move_siblings_preserves_historical_schema_assets_and_one_history_record() {
    for version in [43, 44, 45, PROJECT_VERSION] {
        let start = historical_scene(version);
        let command = move_siblings(35, &[12, 91], 7, 0);
        for command in [
            command.clone(),
            Command::Batch(vec![command.clone()]),
            Command::Batch(vec![Command::Batch(vec![Command::Batch(vec![command])])]),
        ] {
            let mut e = clone_editor(&start);
            let mut expected = start.current.clone();
            expected_move(&mut expected.project, 35, &[12, 91], 7, 0);
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

fn roundtrip() -> Command {
    Command::Batch(vec![
        move_siblings(35, &[12, 51], 7, 0),
        Command::Batch(vec![
            move_siblings(7, &[12, 51], 35, 1),
            reorder(35, &[51, 91, 12]),
        ]),
    ])
}

#[test]
fn contents_move_siblings_roundtrip_with_reorder_preserves_exact_source_and_both_histories() {
    for version in [43, 44, 45, PROJECT_VERSION] {
        let start = historical_scene(version);
        let mut e = clone_editor(&start);
        e.execute(roundtrip()).unwrap();
        assert_same_editor(&e, &start);
        // Returning a contiguous block needs no permutation to be a no-op.
        e.execute(Command::Batch(vec![
            move_siblings(35, &[12, 91], 7, 0),
            Command::Batch(vec![move_siblings(7, &[91, 12], 35, 1)]),
        ]))
        .unwrap();
        assert_same_editor(&e, &start);
    }
}

#[test]
fn contents_move_siblings_later_nested_failure_rolls_back_earlier_valid_moves() {
    for version in [43, 44, PROJECT_VERSION] {
        let start = historical_scene(version);
        for invalid in [
            move_siblings(35, &[12], 7, 999),
            move_siblings(7, &[91], 7, 0),
            move_siblings(0, &[60], 35, 0),
            reorder(35, &[91, 12]),
        ] {
            let mut e = clone_editor(&start);
            assert!(
                e.execute(Command::Batch(vec![
                    move_siblings(35, &[91], 7, 0),
                    Command::Batch(vec![Command::Batch(vec![invalid])]),
                ]))
                .is_err()
            );
            assert_same_editor(&e, &start);
        }
    }
}

#[test]
fn contents_move_siblings_classifier_is_separate_and_narrow() {
    let move_command = move_siblings(35, &[91], 7, 0);
    assert!(move_command.contents_sibling_moves_only());
    assert!(!move_command.reorders_only());
    assert!(roundtrip().contents_sibling_moves_only());
    assert!(!reorder(0, &[]).contents_sibling_moves_only());
    assert!(!Command::Batch(vec![reorder(0, &[])]).contents_sibling_moves_only());
    assert!(
        Command::Batch(vec![reorder(0, &[]), move_command.clone()]).contents_sibling_moves_only()
    );
    for command in [
        Command::Batch(vec![]),
        move_siblings(35, &[], 7, 0),
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Move {
                item: 91,
                parent: 7,
                index: 0,
            },
        },
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Rename {
                item: 7,
                name: "Destination".into(),
            },
        },
        Command::ReorderPath {
            id: 1,
            target: PathTarget::Contents(91),
            order: PathOrder::Reverse,
        },
    ] {
        assert!(!command.contents_sibling_moves_only(), "{command:?}");
        assert!(
            !Command::Batch(vec![move_command.clone(), Command::Batch(vec![command])])
                .contents_sibling_moves_only()
        );
    }
}

#[test]
fn contents_move_siblings_mixed_and_empty_batches_keep_legacy_migration_behavior() {
    let start = historical_scene(PROJECT_VERSION);
    let rename = Command::Contents {
        id: 1,
        edit: ContentsEdit::Rename {
            item: 7,
            name: "Destination".into(),
        },
    };
    let mut expected_renamed = clone_editor(&start);
    expected_renamed.execute(rename.clone()).unwrap();
    for command in [
        Command::Batch(vec![roundtrip(), rename.clone()]),
        Command::Batch(vec![rename, Command::Batch(vec![roundtrip()])]),
    ] {
        let mut e = clone_editor(&start);
        e.execute(command).unwrap();
        assert_same_editor(&e, &expected_renamed);
    }
    let mut expected_empty = clone_editor(&start);
    expected_empty.execute(Command::Batch(vec![])).unwrap();
    for command in [
        Command::Batch(vec![roundtrip(), Command::Batch(vec![])]),
        Command::Batch(vec![Command::Batch(vec![]), roundtrip()]),
        Command::Batch(vec![
            roundtrip(),
            Command::Contents {
                id: 1,
                edit: ContentsEdit::Move {
                    item: 91,
                    parent: 35,
                    index: 1,
                },
            },
        ]),
        Command::Batch(vec![
            roundtrip(),
            Command::ReorderPath {
                id: 1,
                target: PathTarget::Contents(91),
                order: PathOrder::Reverse,
            },
            Command::ReorderPath {
                id: 1,
                target: PathTarget::Contents(91),
                order: PathOrder::Reverse,
            },
        ]),
    ] {
        let mut e = clone_editor(&start);
        e.execute(command).unwrap();
        assert_same_editor(&e, &expected_empty);
    }
    assert!(expected_empty.project().version < PROJECT_VERSION);
}

fn tree_scene(items: Vec<ContentsNode>, next_id: u64) -> Editor {
    let mut e = reorder_scene();
    e.current.project.composition.layers[0].content = Content::ShapeContents(
        serde_json::from_value(serde_json::json!({ "items": items, "next_id": next_id })).unwrap(),
    );
    e.current.project.validate().unwrap();
    e.clear_history();
    e
}

fn group_node(id: u64, children: Vec<ContentsNode>) -> ContentsNode {
    let e = reorder_scene();
    let mut node = contents(&e).node(7).unwrap().clone();
    node.id = id;
    node.kind = ContentsKind::Group(children);
    node
}

fn leaf_node(id: u64) -> ContentsNode {
    let e = reorder_scene();
    let mut node = contents(&e).node(91).unwrap().clone();
    node.id = id;
    node
}

fn nested_groups(depth: u64) -> ContentsNode {
    let mut node = group_node(depth, vec![]);
    for id in (1..depth).rev() {
        node = group_node(id, vec![node]);
    }
    node
}

#[test]
fn contents_move_siblings_enforces_actual_eight_group_depth_including_empty_groups() {
    let start = tree_scene(
        vec![nested_groups(8), group_node(90, vec![]), leaf_node(91)],
        100,
    );
    let mut leaf = clone_editor(&start);
    leaf.execute(move_siblings(0, &[91], 8, 0)).unwrap();
    assert_eq!(ids(leaf.project(), 8), vec![91]);
    let mut boundary = clone_editor(&start);
    boundary.execute(move_siblings(0, &[90], 7, 1)).unwrap();
    assert_eq!(ids(boundary.project(), 7), vec![8, 90]);
    for command in [
        move_siblings(0, &[90], 8, 0),
        Command::Batch(vec![
            move_siblings(0, &[91], 8, 0),
            move_siblings(0, &[90], 8, 1),
        ]),
    ] {
        let mut e = clone_editor(&start);
        assert!(e.execute(command).unwrap_err().contains("8 groups"));
        assert_same_editor(&e, &start);
    }
    // An invalid source cannot be repaired by moving its ninth group outward.
    let mut e = clone_editor(&start);
    mutable_children(&mut e.current.project, 8).push(group_node(92, vec![]));
    let before = clone_editor(&e);
    assert!(e.execute(move_siblings(8, &[92], 0, 0)).is_err());
    assert_same_editor(&e, &before);
}

#[test]
fn contents_move_siblings_enforces_actual_256_nodes_without_allocating_ids() {
    let mut nodes = vec![group_node(1, vec![])];
    let template = leaf_node(2);
    nodes.extend((2..=256).map(|id| {
        let mut node = template.clone();
        node.id = id;
        node
    }));
    let mut e = tree_scene(nodes, 900);
    e.execute(move_siblings(0, &[256, 128, 2], 1, 0)).unwrap();
    assert_eq!(ids(e.project(), 1), vec![2, 128, 256]);
    assert_eq!(contents(&e).rows().len(), 256);
    assert_eq!(serde_json::to_value(contents(&e)).unwrap()["next_id"], 900);
    mutable_children(&mut e.current.project, 0).push(leaf_node(257));
    let before = clone_editor(&e);
    assert!(e.execute(move_siblings(0, &[3], 1, 0)).is_err());
    assert_same_editor(&e, &before);
}

#[test]
fn contents_move_siblings_validates_complete_declared_source_schema_even_for_noop() {
    for corrupt in 0..6 {
        let mut e = historical_scene(if corrupt == 0 { 43 } else { PROJECT_VERSION });
        match corrupt {
            0 => {
                mutable_children(&mut e.current.project, 0)[0]
                    .parameters
                    .insert(ContentsParam::Skew, AnimatedProperty::new(0.));
            }
            1 => mutable_children(&mut e.current.project, 35)[1].name.clear(),
            2 => mutable_children(&mut e.current.project, 0)[0].id = 23,
            3 => {
                mutable_children(&mut e.current.project, 60)[0]
                    .parameters
                    .clear();
            }
            4 => {
                // Unrelated asset-linked layer corruption cannot be hidden by a move.
                e.current
                    .project
                    .composition
                    .layers
                    .iter_mut()
                    .find(|l| l.id == 3)
                    .unwrap()
                    .asset = Some(999);
            }
            _ => {
                let layer = e
                    .current
                    .project
                    .composition
                    .layers
                    .iter_mut()
                    .find(|l| l.id == 1)
                    .unwrap();
                let Content::ShapeContents(c) = &mut layer.content else {
                    panic!()
                };
                let mut value = serde_json::to_value(&c).unwrap();
                value["next_id"] = 2.into();
                *c = serde_json::from_value(value).unwrap();
            }
        }
        assert!(e.project().validate().is_err(), "corruption {corrupt}");
        let before = clone_editor(&e);
        for command in [move_siblings(35, &[91], 7, 0), roundtrip()] {
            assert!(e.execute(command).is_err(), "corruption {corrupt}");
            assert_same_editor(&e, &before);
        }
    }
}

#[test]
fn contents_move_siblings_native_and_json_roundtrips_keep_schema_ids_payloads_and_container() {
    let mut e = rich_scene();
    e.execute(Command::Batch(vec![
        move_siblings(35, &[200, 91], 7, 0),
        move_siblings(60, &[35], 23, 0),
        reorder(0, &[201, 60, 7, 23]),
    ]))
    .unwrap();
    let json = e.project().to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), *e.project());
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
