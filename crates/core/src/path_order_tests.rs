use super::*;

fn geometry(count: usize, closed: bool, offset: f64) -> VectorPath {
    VectorPath {
        closed,
        vertices: (0..count)
            .map(|i| {
                let n = i as f64;
                PathVertex {
                    position: [offset + 3.0 * n, offset - n * n],
                    incoming: [-n - 0.25, n + 2.5],
                    outgoing: [2.0 * n + 0.5, -n - 3.75],
                }
            })
            .collect(),
    }
}

fn expected(path: &VectorPath, order: PathOrder) -> VectorPath {
    let count = path.vertices.len();
    let vertices = (0..count)
        .map(|i| {
            let source = match order {
                PathOrder::Reverse if path.closed => (count - i) % count,
                PathOrder::Reverse => count - 1 - i,
                PathOrder::FirstVertex(first) => (i + first) % count,
            };
            let mut vertex = path.vertices[source];
            if order == PathOrder::Reverse {
                vertex.incoming = path.vertices[source].outgoing;
                vertex.outgoing = path.vertices[source].incoming;
            }
            vertex
        })
        .collect();
    VectorPath {
        vertices,
        closed: path.closed,
    }
}

fn cubic(path: &VectorPath, segment: usize, t: f64) -> [f64; 2] {
    let a = path.vertices[segment];
    let b = path.vertices[(segment + 1) % path.vertices.len()];
    std::array::from_fn(|axis| {
        let u = 1.0 - t;
        u.powi(3) * a.position[axis]
            + 3.0 * u.powi(2) * t * (a.position[axis] + a.outgoing[axis])
            + 3.0 * u * t.powi(2) * (b.position[axis] + b.incoming[axis])
            + t.powi(3) * b.position[axis]
    })
}

#[test]
fn path_order_reverse_preserves_cubics_swaps_asymmetric_handles_and_is_involutive() {
    for closed in [false, true] {
        let path = geometry(5, closed, 12.0);
        let reversed = path.reordered(PathOrder::Reverse).unwrap();
        assert_eq!(reversed, expected(&path, PathOrder::Reverse));
        assert_eq!(reversed.reordered(PathOrder::Reverse).unwrap(), path);
        let segments = path.vertices.len() - usize::from(!closed);
        for segment in 0..segments {
            for t in [0.0, 0.1, 0.33, 0.5, 0.9, 1.0] {
                let a = cubic(&path, segments - 1 - segment, 1.0 - t);
                let b = cubic(&reversed, segment, t);
                assert!((a[0] - b[0]).abs() < 1e-10);
                assert!((a[1] - b[1]).abs() < 1e-10);
            }
        }
    }
}

#[test]
fn path_order_first_vertex_rotates_without_changing_handles_or_winding() {
    let path = geometry(5, true, 9.0);
    for first in 0..path.vertices.len() {
        let reordered = path.reordered(PathOrder::FirstVertex(first)).unwrap();
        assert_eq!(reordered, expected(&path, PathOrder::FirstVertex(first)));
        for segment in 0..path.vertices.len() {
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                assert_eq!(
                    cubic(&reordered, segment, t),
                    cubic(&path, (segment + first) % path.vertices.len(), t)
                );
            }
        }
    }
    assert_eq!(
        serde_json::to_vec(&path.reordered(PathOrder::FirstVertex(0)).unwrap()).unwrap(),
        serde_json::to_vec(&path).unwrap()
    );
}

#[test]
fn path_order_checks_minimum_maximum_open_paths_and_invalid_indices() {
    for (count, closed) in [(2, false), (3, true), (1024, false), (1024, true)] {
        let mut path = geometry(count, closed, 0.0);
        // Keep the large fixture inside the coordinate limits.
        for vertex in &mut path.vertices {
            vertex.position[1] *= 0.5;
        }
        assert!(path.valid());
        assert!(path.reordered(PathOrder::Reverse).unwrap().valid());
        let before = path.clone();
        for index in [count, usize::MAX] {
            assert!(path.reordered(PathOrder::FirstVertex(index)).is_err());
            assert_eq!(path, before);
        }
        for index in [0, count - 1] {
            let result = path.reordered(PathOrder::FirstVertex(index));
            assert_eq!(result.is_ok(), closed);
        }
    }
    let mut invalid = vec![
        geometry(0, false, 0.0),
        geometry(1, false, 0.0),
        geometry(2, true, 0.0),
        geometry(1025, false, 0.0),
    ];
    for value in [f64::NAN, f64::INFINITY, 1_000_001.0] {
        let mut path = geometry(3, true, 0.0);
        path.vertices[1].incoming[0] = value;
        invalid.push(path);
    }
    for path in invalid {
        assert!(path.reordered(PathOrder::Reverse).is_err());
        assert!(path.reordered(PathOrder::FirstVertex(0)).is_err());
    }
}

fn animation(base: &VectorPath, interpolation: Interpolation) -> PathAnimation {
    let mut poses = vec![base.clone()];
    for shift in [20.0, 50.0, 50.0] {
        let mut pose = base.clone();
        for (i, vertex) in pose.vertices.iter_mut().enumerate() {
            vertex.position[0] += shift + i as f64;
            vertex.position[1] -= shift * 0.25;
            vertex.incoming[0] += shift * 0.1;
            vertex.outgoing[1] -= shift * 0.2;
        }
        poses.push(pose);
    }
    // Pose 3 duplicates pose 2 but is unused; neither slot may be removed.
    PathAnimation {
        poses,
        timing: AnimatedProperty {
            value: 2.0,
            keys: [(10, 2.0), (30, 0.0), (70, 1.0), (90, 1.0)]
                .into_iter()
                .map(|(frame, value)| {
                    (
                        frame,
                        Keyframe {
                            value,
                            interpolation,
                            temporal: TemporalHandles::default(),
                        },
                    )
                })
                .collect(),
        },
    }
}

fn order_scene(target: PathTarget, closed: bool, animated: bool) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(geometry(5, closed, 9.0)),
                stroke_width: 7.0,
                ..Default::default()
            }),
            width: 270.0,
            height: 150.0,
            name: "Reordered path".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetPathMasks {
            id: 1,
            masks: vec![
                PathMask {
                    path: geometry(4, true, 70.0),
                    mode: PathMaskMode::Subtract,
                    inverted: true,
                    ..Default::default()
                },
                PathMask {
                    path: geometry(5, true, 23.0),
                    mode: PathMaskMode::Intersect,
                    ..Default::default()
                },
            ],
        })
        .unwrap();
    if matches!(target, PathTarget::Contents(_)) {
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Promote,
            })
            .unwrap();
    }
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetPosition {
            id: 2,
            frame: 15,
            x: 73.0,
            y: -19.0,
        })
        .unwrap();
    let layer = editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap();
    let (base, track) = layer.path_animation_mut(target).unwrap();
    if animated {
        *track = animation(base, Interpolation::Linear);
    }
    // Imported files may have a higher version than their current feature minimum.
    editor.current.project.version = PROJECT_VERSION;
    editor.current.project.validate().unwrap();
    editor.select(1);
    editor.clear_history();
    editor
}

fn assert_reorder_preserved(editor: &mut Editor, target: PathTarget, order: PathOrder) {
    let before = editor.current.clone();
    let before_layer = before.project.composition.layer(1).unwrap();
    let (base, track) = before_layer.path_animation(target).unwrap();
    let timing_bytes = serde_json::to_vec(&track.timing).unwrap();
    let mut expected_snapshot = before.clone();
    let expected_layer = expected_snapshot
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap();
    let (expected_base, expected_track) = expected_layer.path_animation_mut(target).unwrap();
    *expected_base = expected(base, order);
    for pose in &mut expected_track.poses {
        *pose = expected(pose, order);
    }
    editor
        .execute(Command::ReorderPath {
            id: 1,
            target,
            order,
        })
        .unwrap();
    assert_eq!(editor.current, expected_snapshot);
    // Covers all unrelated fields, current schema, IDs, content order, and key metadata.
    assert_eq!(
        editor.project().to_json().unwrap().as_bytes(),
        expected_snapshot.project.to_json().unwrap().as_bytes()
    );
    let layer = editor.project().composition.layer(1).unwrap();
    let (after_base, after_track) = layer.path_animation(target).unwrap();
    assert_eq!(
        serde_json::to_vec(&after_track.timing).unwrap(),
        timing_bytes
    );
    assert_eq!(after_track.poses.len(), track.poses.len());
    for frame in [0, 9, 10, 11, 15, 20, 29, 30, 31, 50, 69, 70, 80, 90, 120] {
        assert_eq!(
            after_track.at(after_base, frame),
            expected(&track.at(base, frame), order),
            "frame {frame}, target {target:?}, order {order:?}"
        );
    }
    let saved = editor.current.clone();
    let json = editor.project().to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
    assert_eq!(editor.undo.len(), 1);
    editor.undo();
    assert_eq!(editor.current, before);
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(editor.current, saved);
}

#[test]
fn path_order_static_shape_contents_and_masks_preserve_data_roundtrip_and_history() {
    for (target, closed) in [
        (PathTarget::Shape, true),
        (PathTarget::Shape, false),
        (PathTarget::Contents(2), true),
        (PathTarget::Contents(2), false),
        (PathTarget::Mask(2), true),
    ] {
        for order in [PathOrder::Reverse, PathOrder::FirstVertex(3)] {
            if !closed && matches!(order, PathOrder::FirstVertex(_)) {
                continue;
            }
            let mut editor = order_scene(target, closed, false);
            assert_reorder_preserved(&mut editor, target, order);
        }
    }
}

#[test]
fn path_order_animated_targets_preserve_all_poses_and_eased_descending_references() {
    for (target, closed) in [
        (PathTarget::Shape, true),
        (PathTarget::Shape, false),
        (PathTarget::Contents(2), true),
        (PathTarget::Contents(2), false),
        (PathTarget::Mask(2), true),
    ] {
        for interpolation in [
            Interpolation::Linear,
            Interpolation::Hold,
            Interpolation::Smooth,
            Interpolation::Bezier(Bezier {
                x1: 0.2,
                y1: -0.3,
                x2: 0.7,
                y2: 1.4,
            }),
        ] {
            for order in [PathOrder::Reverse, PathOrder::FirstVertex(4)] {
                if !closed && matches!(order, PathOrder::FirstVertex(_)) {
                    continue;
                }
                let mut editor = order_scene(target, closed, true);
                let layer = editor
                    .current
                    .project
                    .composition
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == 1)
                    .unwrap();
                for key in layer
                    .path_animation_mut(target)
                    .unwrap()
                    .1
                    .timing
                    .keys
                    .values_mut()
                {
                    key.interpolation = interpolation;
                }
                assert_reorder_preserved(&mut editor, target, order);
            }
        }
    }
}

#[test]
fn path_order_preserves_unkeyed_pose_slots_and_opaque_temporal_metadata() {
    let mut base = geometry(4, true, 11.0);
    let mut track = animation(&base, Interpolation::Smooth);
    track.timing.keys.get_mut(&10).unwrap().temporal = TemporalHandles {
        mode: TemporalMode::Independent,
        incoming: Some(TemporalHandle {
            slope: -0.4,
            influence: 0.25,
        }),
        outgoing: Some(TemporalHandle {
            slope: 0.75,
            influence: 0.6,
        }),
    };
    let before = track.clone();
    let original_base = base.clone();
    let timing = serde_json::to_vec(&track.timing).unwrap();
    track.reorder(&mut base, PathOrder::Reverse).unwrap();
    assert_eq!(serde_json::to_vec(&track.timing).unwrap(), timing);
    track.reorder(&mut base, PathOrder::Reverse).unwrap();
    assert_eq!(track, before);
    assert_eq!(base, original_base);

    for target in [
        PathTarget::Shape,
        PathTarget::Contents(2),
        PathTarget::Mask(2),
    ] {
        let mut editor = order_scene(target, true, true);
        let layer = editor
            .current
            .project
            .composition
            .layers
            .iter_mut()
            .find(|layer| layer.id == 1)
            .unwrap();
        layer
            .path_animation_mut(target)
            .unwrap()
            .1
            .timing
            .keys
            .clear();
        assert_reorder_preserved(&mut editor, target, PathOrder::FirstVertex(2));
    }
}

fn assert_no_mutation(editor: &mut Editor, command: Command, success: bool) {
    let before = editor.current.clone();
    let bytes = editor.project().to_json().unwrap();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    assert_eq!(editor.execute(command).is_ok(), success);
    assert_eq!(editor.current, before);
    assert_eq!(editor.project().to_json().unwrap(), bytes);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
}

#[test]
fn path_order_noops_and_rejections_preserve_project_selection_and_both_history_stacks() {
    for target in [
        PathTarget::Shape,
        PathTarget::Contents(2),
        PathTarget::Mask(2),
    ] {
        for animated in [false, true] {
            let mut editor = order_scene(target, true, animated);
            for color in [0x123456, 0xabcdef] {
                editor.execute(Command::SetColor { id: 2, color }).unwrap();
            }
            editor.undo();
            assert!(editor.can_undo() && editor.can_redo());
            assert_no_mutation(
                &mut editor,
                Command::ReorderPath {
                    id: 1,
                    target,
                    order: PathOrder::FirstVertex(0),
                },
                true,
            );
            for (id, invalid_target, order) in [
                (1, target, PathOrder::FirstVertex(5)),
                (1, target, PathOrder::FirstVertex(usize::MAX)),
                (999, target, PathOrder::Reverse),
                (2, PathTarget::Shape, PathOrder::Reverse),
                (1, PathTarget::Mask(999), PathOrder::Reverse),
                (1, PathTarget::Contents(999), PathOrder::Reverse),
                (1, PathTarget::Contents(1), PathOrder::Reverse),
                (1, PathTarget::Contents(3), PathOrder::Reverse),
                (1, PathTarget::Contents(4), PathOrder::Reverse),
            ] {
                assert_no_mutation(
                    &mut editor,
                    Command::ReorderPath {
                        id,
                        target: invalid_target,
                        order,
                    },
                    false,
                );
            }
            editor.execute(Command::ToggleLocked(1)).unwrap();
            for order in [
                PathOrder::Reverse,
                PathOrder::FirstVertex(0),
                PathOrder::FirstVertex(2),
            ] {
                assert_no_mutation(
                    &mut editor,
                    Command::ReorderPath {
                        id: 1,
                        target,
                        order,
                    },
                    false,
                );
            }
        }
    }
    for target in [PathTarget::Shape, PathTarget::Contents(2)] {
        let mut editor = order_scene(target, false, true);
        for index in [0, 1, 5, usize::MAX] {
            assert_no_mutation(
                &mut editor,
                Command::ReorderPath {
                    id: 1,
                    target,
                    order: PathOrder::FirstVertex(index),
                },
                false,
            );
        }
    }
}

#[test]
fn path_order_candidate_validation_and_invalid_pose_fail_atomically() {
    let target = PathTarget::Shape;
    let mut editor = order_scene(target, true, true);
    editor.current.project.composition.width = 0;
    assert_no_mutation(
        &mut editor,
        Command::ReorderPath {
            id: 1,
            target,
            order: PathOrder::Reverse,
        },
        false,
    );
    let mut base = geometry(5, true, 0.0);
    let mut track = animation(&base, Interpolation::Linear);
    track.poses[3].vertices.clear();
    let before = (base.clone(), track.clone());
    assert!(track.reorder(&mut base, PathOrder::Reverse).is_err());
    assert_eq!((base, track), before);
}

fn clone_editor(editor: &Editor) -> Editor {
    Editor {
        current: editor.current.clone(),
        undo: editor.undo.clone(),
        redo: editor.redo.clone(),
        context_generation: editor.context_generation,
    }
}

fn assert_same_editor(actual: &Editor, expected: &Editor) {
    assert_eq!(actual.current, expected.current);
    assert_eq!(
        actual.project().to_json().unwrap(),
        expected.project().to_json().unwrap()
    );
    assert_eq!(actual.undo, expected.undo);
    assert_eq!(actual.redo, expected.redo);
}

fn batch_scene(target: PathTarget, animated: bool) -> Editor {
    let mut editor = order_scene(target, true, animated);
    // Include a shared media source and an unused imported asset in preservation checks.
    for (png, frame) in [("YWJj", Some(0)), ("ZGVm", None)] {
        editor
            .execute(Command::ImportAsset {
                content: Content::Image { png: png.into() },
                width: 64.0,
                height: 48.0,
                name: format!("Unrelated {png}"),
                folder: None,
                frame,
            })
            .unwrap();
    }
    for color in [0x123456, 0xabcdef] {
        editor.execute(Command::SetColor { id: 2, color }).unwrap();
    }
    editor.undo();
    editor.select(1);
    editor.current.project.version = PROJECT_VERSION;
    editor.current.project.validate().unwrap();
    assert!(editor.can_undo() && editor.can_redo());
    assert_eq!(editor.project().asset_library().assets().len(), 2);
    editor
}

fn reorder(target: PathTarget, order: PathOrder) -> Command {
    Command::ReorderPath {
        id: 1,
        target,
        order,
    }
}

#[test]
fn path_order_flat_and_nested_batches_match_direct_edits_without_migrating_assets_or_schema() {
    for target in [
        PathTarget::Shape,
        PathTarget::Contents(2),
        PathTarget::Mask(2),
    ] {
        for animated in [false, true] {
            for order in [PathOrder::Reverse, PathOrder::FirstVertex(2)] {
                let start = batch_scene(target, animated);
                let assets = serde_json::to_vec(start.project().asset_library()).unwrap();
                let mut direct = clone_editor(&start);
                direct.execute(reorder(target, order)).unwrap();
                assert_eq!(direct.project().version, PROJECT_VERSION);
                assert_eq!(
                    serde_json::to_vec(direct.project().asset_library()).unwrap(),
                    assets
                );
                assert_eq!(direct.undo.len(), start.undo.len() + 1);
                assert!(direct.redo.is_empty());
                for command in [
                    Command::Batch(vec![reorder(target, order)]),
                    Command::Batch(vec![Command::Batch(vec![Command::Batch(vec![reorder(
                        target, order,
                    )])])]),
                ] {
                    let mut batched = clone_editor(&start);
                    batched.execute(command).unwrap();
                    assert_same_editor(&batched, &direct);
                    batched.undo();
                    assert_eq!(batched.current, start.current);
                    assert_eq!(batched.undo, start.undo);
                    assert_eq!(batched.redo.len(), 1);
                    batched.redo();
                    assert_same_editor(&batched, &direct);
                }
            }
        }
    }
}

#[test]
fn path_order_noop_and_failing_batches_keep_high_schema_assets_and_both_history_stacks() {
    for target in [
        PathTarget::Shape,
        PathTarget::Contents(2),
        PathTarget::Mask(2),
    ] {
        for animated in [false, true] {
            let mut editor = batch_scene(target, animated);
            for command in [
                reorder(target, PathOrder::FirstVertex(0)),
                Command::Batch(vec![reorder(target, PathOrder::FirstVertex(0))]),
                Command::Batch(vec![Command::Batch(vec![Command::Batch(vec![reorder(
                    target,
                    PathOrder::FirstVertex(0),
                )])])]),
                Command::Batch(vec![
                    reorder(target, PathOrder::Reverse),
                    reorder(target, PathOrder::Reverse),
                ]),
                Command::Batch(vec![
                    Command::Batch(vec![reorder(target, PathOrder::Reverse)]),
                    Command::Batch(vec![Command::Batch(vec![reorder(
                        target,
                        PathOrder::Reverse,
                    )])]),
                ]),
            ] {
                assert_no_mutation(&mut editor, command, true);
            }
            for invalid in [
                reorder(target, PathOrder::FirstVertex(usize::MAX)),
                reorder(PathTarget::Mask(999), PathOrder::Reverse),
                Command::ReorderPath {
                    id: 999,
                    target,
                    order: PathOrder::Reverse,
                },
            ] {
                for command in [
                    Command::Batch(vec![reorder(target, PathOrder::Reverse), invalid.clone()]),
                    Command::Batch(vec![
                        Command::Batch(vec![reorder(target, PathOrder::FirstVertex(3))]),
                        Command::Batch(vec![Command::Batch(vec![invalid.clone()])]),
                    ]),
                ] {
                    assert_no_mutation(&mut editor, command, false);
                }
            }
        }
    }
}

#[test]
fn path_order_mixed_and_empty_batches_keep_existing_migration_and_history_behavior() {
    let target = PathTarget::Shape;
    let start = batch_scene(target, true);
    let color = Command::SetColor {
        id: 2,
        color: 0x345678,
    };
    let mut expected = clone_editor(&start);
    expected.execute(color.clone()).unwrap();
    assert!(expected.project().version < PROJECT_VERSION);
    assert_eq!(expected.undo.len(), start.undo.len() + 1);
    assert!(expected.redo.is_empty());
    for command in [
        Command::Batch(vec![
            reorder(target, PathOrder::FirstVertex(0)),
            color.clone(),
        ]),
        Command::Batch(vec![
            Command::Batch(vec![reorder(target, PathOrder::Reverse)]),
            Command::Batch(vec![color, reorder(target, PathOrder::Reverse)]),
        ]),
    ] {
        let mut editor = clone_editor(&start);
        editor.execute(command).unwrap();
        assert_same_editor(&editor, &expected);
    }

    let mut expected_empty = clone_editor(&start);
    expected_empty.execute(Command::Batch(vec![])).unwrap();
    assert!(expected_empty.project().version < PROJECT_VERSION);
    assert_eq!(expected_empty.undo.len(), start.undo.len() + 1);
    assert!(expected_empty.redo.is_empty());
    for command in [
        Command::Batch(vec![Command::Batch(vec![])]),
        Command::Batch(vec![
            reorder(target, PathOrder::FirstVertex(0)),
            Command::Batch(vec![Command::Batch(vec![])]),
        ]),
    ] {
        let mut editor = clone_editor(&start);
        editor.execute(command).unwrap();
        assert_same_editor(&editor, &expected_empty);
    }
}
