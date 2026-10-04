use super::*;

const TARGETS: [PathTarget; 3] = [
    PathTarget::Shape,
    PathTarget::Contents(2),
    PathTarget::Mask(2),
];

fn geometry(count: usize, closed: bool, offset: f64) -> VectorPath {
    VectorPath {
        closed,
        vertices: (0..count)
            .map(|i| {
                let n = i as f64;
                PathVertex {
                    position: [offset + 3. * n, offset - n * 0.5],
                    incoming: [-n - 0.25, n + 2.5],
                    outgoing: [2. * n + 0.5, -n - 3.75],
                }
            })
            .collect(),
    }
}

fn animation(base: &VectorPath, interpolation: Interpolation) -> PathAnimation {
    let mut poses = vec![base.clone()];
    for offset in [20., 50., 50.] {
        poses.push(geometry(base.vertices.len(), base.closed, offset));
    }
    PathAnimation {
        poses,
        timing: AnimatedProperty {
            value: 2.,
            keys: [(10, 2.), (30, 0.), (70, 1.), (90, 1.)]
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

fn scene(target: PathTarget, closed: bool, mode: usize) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(geometry(4, closed, 9.)),
                stroke_width: 7.,
                ..Default::default()
            }),
            width: 270.,
            height: 150.,
            name: "All poses".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetPathMasks {
            id: 1,
            masks: vec![
                PathMask {
                    path: geometry(3, true, 70.),
                    mode: PathMaskMode::Subtract,
                    inverted: true,
                    ..Default::default()
                },
                PathMask {
                    path: geometry(4, true, 23.),
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
            x: 73.,
            y: -19.,
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
    if mode != 0 {
        *track = animation(base, Interpolation::Linear);
        if mode == 2 {
            track.timing.keys.clear();
        }
    }
    editor.current.project.version = PROJECT_VERSION;
    editor.current.project.validate().unwrap();
    editor.select(1);
    editor.clear_history();
    editor
}

fn clone_editor(editor: &Editor) -> Editor {
    Editor {
        current: editor.current.clone(),
        undo: editor.undo.clone(),
        redo: editor.redo.clone(),
    }
}

fn target_mut(editor: &mut Editor, target: PathTarget) -> (&mut VectorPath, &mut PathAnimation) {
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap()
        .path_animation_mut(target)
        .unwrap()
}

fn command(target: PathTarget, indices: BTreeSet<usize>, transform: PathTransformSpec) -> Command {
    Command::TransformPathPoses {
        id: 1,
        target,
        indices,
        transform,
    }
}

fn fixed_spec() -> PathTransformSpec {
    [3., -5., 90., -200., 50., 8., -4.].into()
}

// Independent dyadic/cardinal oracle: x' = 9 - y/2, y' = 7 - 2x.
fn expected(path: &VectorPath, indices: &BTreeSet<usize>) -> VectorPath {
    let mut out = path.clone();
    for &i in indices {
        let before = path.vertices[i];
        out.vertices[i] = PathVertex {
            position: [9. - before.position[1] * 0.5, 7. - before.position[0] * 2.],
            incoming: [-before.incoming[1] * 0.5, -before.incoming[0] * 2.],
            outgoing: [-before.outgoing[1] * 0.5, -before.outgoing[0] * 2.],
        };
    }
    out
}

fn assert_unchanged(editor: &mut Editor, command: Command, success: bool) -> Option<String> {
    let before = editor.current.clone();
    let raw = serde_json::to_vec(&before.project).ok();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    let result = editor.execute(command);
    assert_eq!(result.is_ok(), success, "{result:?}");
    assert_eq!(editor.current, before);
    assert_eq!(serde_json::to_vec(&editor.current.project).ok(), raw);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    result.err()
}

#[test]
fn whole_pose_static_animated_dormant_targets_preserve_every_slot_and_unselected_field() {
    for (target, closed) in [
        (PathTarget::Shape, false),
        (PathTarget::Shape, true),
        (PathTarget::Contents(2), false),
        (PathTarget::Contents(2), true),
        (PathTarget::Mask(2), true),
    ] {
        for mode in 0..3 {
            for indices in [BTreeSet::from([1]), [0, 2].into(), [0, 1, 2, 3].into()] {
                let mut editor = scene(target, closed, mode);
                let before = editor.current.clone();
                let mut want = before.clone();
                let (base, track) = want
                    .project
                    .composition
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == 1)
                    .unwrap()
                    .path_animation_mut(target)
                    .unwrap();
                *base = expected(base, &indices);
                for pose in &mut track.poses {
                    *pose = expected(pose, &indices);
                }
                editor
                    .execute(command(target, indices, fixed_spec()))
                    .unwrap();
                assert_eq!(editor.current, want);
                assert_eq!(
                    serde_json::to_vec(editor.project()).unwrap(),
                    serde_json::to_vec(&want.project).unwrap()
                );
                assert_eq!(editor.undo.len(), 1);
                editor.undo();
                assert_eq!(editor.current, before);
                editor.redo();
                assert_eq!(editor.current, want);
            }
        }
    }
}

#[test]
fn whole_pose_keeps_easing_descending_repeated_references_and_roundtrips_json_lep() {
    for target in TARGETS {
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
            let mut editor = scene(target, true, 1);
            for key in target_mut(&mut editor, target).1.timing.keys.values_mut() {
                key.interpolation = interpolation;
            }
            let source = editor.current.clone();
            let (base, track) = source
                .project
                .composition
                .layer(1)
                .unwrap()
                .path_animation(target)
                .unwrap();
            let timing = serde_json::to_vec(&track.timing).unwrap();
            editor
                .execute(command(target, [0, 2].into(), fixed_spec()))
                .unwrap();
            let (after_base, after_track) = editor
                .project()
                .composition
                .layer(1)
                .unwrap()
                .path_animation(target)
                .unwrap();
            assert_eq!(serde_json::to_vec(&after_track.timing).unwrap(), timing);
            assert_eq!(after_track.poses.len(), 4);
            assert_eq!(after_track.poses[2], after_track.poses[3]);
            for frame in [0, 9, 10, 11, 15, 20, 29, 30, 31, 50, 69, 70, 80, 90, 120] {
                let want = expected(&track.at(base, frame), &[0, 2].into());
                let actual = after_track.at(after_base, frame);
                for (want, actual) in want.vertices.iter().zip(&actual.vertices) {
                    for (a, b) in want
                        .position
                        .into_iter()
                        .chain(want.incoming)
                        .chain(want.outgoing)
                        .zip(
                            actual
                                .position
                                .into_iter()
                                .chain(actual.incoming)
                                .chain(actual.outgoing),
                        )
                    {
                        // At most a few products/additions are reordered by interpolation;
                        // fixtures stay below 200 and this is < 32 ulps at that scale.
                        assert!((a - b).abs() < 1e-12, "frame {frame}: {a} != {b}");
                    }
                }
            }
            let project = editor.project();
            assert_eq!(
                Project::from_json(&project.to_json().unwrap()).unwrap(),
                *project
            );
            let view = br#"{"version":1,"frame":20}"#;
            let bytes = project_file::encode(project, Some(view)).unwrap();
            let loaded = project_file::decode(&bytes).unwrap();
            assert_eq!(loaded.project, *project);
            assert_eq!(loaded.view, Some(view.as_slice()));
            assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
            assert_eq!(project.version, PROJECT_VERSION);
        }
    }
}

#[test]
fn visible_pose_fixed_point_does_not_hide_changed_base_and_unused_pose() {
    for target in TARGETS {
        let mut editor = scene(target, true, 1);
        let (base, track) = target_mut(&mut editor, target);
        base.vertices = vec![PathVertex::corner([3., 5.]); 4];
        track.poses[2].vertices = vec![PathVertex::corner([0., 0.]); 4];
        track.poses[3].vertices = vec![PathVertex::corner([7., 9.]); 4];
        let before = editor.current.clone();
        let (base, track) = before
            .project
            .composition
            .layer(1)
            .unwrap()
            .path_animation(target)
            .unwrap();
        let visible = track.at(base, 10);
        editor
            .execute(command(
                target,
                [0, 1, 2, 3].into(),
                [0., 0., 90., 100., 100., 0., 0.].into(),
            ))
            .unwrap();
        let (after_base, after_track) = editor
            .project()
            .composition
            .layer(1)
            .unwrap()
            .path_animation(target)
            .unwrap();
        assert_eq!(after_track.at(after_base, 10), visible);
        assert_eq!(after_base.vertices[0].position, [-5., 3.]);
        assert_eq!(after_track.poses[3].vertices[0].position, [-9., 7.]);
        assert_eq!(editor.undo.len(), 1);
        editor.undo();
        assert_eq!(editor.current, before);
        assert!(editor.can_redo());
    }
}

fn history_scene(target: PathTarget, mode: usize) -> Editor {
    let mut editor = scene(target, true, mode);
    for (png, frame) in [("YWJj", Some(0)), ("ZGVm", None)] {
        editor
            .execute(Command::ImportAsset {
                content: Content::Image { png: png.into() },
                width: 64.,
                height: 48.,
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
    assert!(editor.can_undo() && editor.can_redo());
    editor
}

#[test]
fn whole_pose_exact_noops_nested_away_back_and_signed_zero_keep_history_schema_and_assets() {
    for target in TARGETS {
        for mode in 0..3 {
            let mut editor = history_scene(target, mode);
            let (base, track) = target_mut(&mut editor, target);
            base.vertices[0].incoming = [-0., 0.];
            for pose in &mut track.poses {
                pose.vertices[0].outgoing = [0., -0.];
            }
            let encoded = project_file::encode(editor.project(), None).unwrap();
            for values in [
                [0., 0., 0., 100., 100., 0., 0.],
                [0., 0., -1080., 100., 100., 1e308, -1e308],
                [0., 0., 180., -100., -100., 1e300, -1e300],
            ] {
                let cmd = command(target, [0, 2].into(), values.into());
                assert_unchanged(&mut editor, cmd.clone(), true);
                assert_unchanged(
                    &mut editor,
                    Command::Batch(vec![Command::Batch(vec![cmd])]),
                    true,
                );
                assert_eq!(
                    project_file::encode(editor.project(), None).unwrap(),
                    encoded
                );
            }
            assert_unchanged(
                &mut editor,
                Command::Batch(vec![
                    command(
                        target,
                        [0, 2].into(),
                        [8., -16., 0., 100., 100., 0., 0.].into(),
                    ),
                    Command::Batch(vec![command(
                        target,
                        [0, 2].into(),
                        [-8., 16., 0., 100., 100., 0., 0.].into(),
                    )]),
                ]),
                true,
            );
        }
    }
    // Minimum historical schema for a static shape must stay unchanged too.
    let mut editor = scene(PathTarget::Shape, true, 0);
    editor.current.project.version = 30;
    editor.current.project.validate().unwrap();
    editor
        .execute(command(PathTarget::Shape, [0].into(), fixed_spec()))
        .unwrap();
    assert_eq!(editor.project().version, 30);
}

#[test]
fn whole_pose_fixed_line_and_collapse_noops_preserve_all_slots_and_redo() {
    for target in TARGETS {
        for fixed_line in [false, true] {
            let mut editor = history_scene(target, 2);
            let (base, track) = target_mut(&mut editor, target);
            let path = VectorPath {
                closed: true,
                vertices: (0..4)
                    .map(|i| {
                        if fixed_line {
                            PathVertex {
                                position: [i as f64 * 0.125, 0.],
                                incoming: [-0.03125, -0.],
                                outgoing: [0.0625, 0.],
                            }
                        } else {
                            PathVertex::corner([0.13, -0.27])
                        }
                    })
                    .collect(),
            };
            *base = path.clone();
            track.poses.fill(path);
            let spec = if fixed_line {
                [0., 0., 0., 100., -350., 0., 0.]
            } else {
                [0., 0., 43.25, 0., 0., 0.13, -0.27]
            };
            assert_unchanged(
                &mut editor,
                command(target, [0, 1, 2, 3].into(), spec.into()),
                true,
            );
        }
    }
}

#[test]
fn whole_pose_target_lock_empty_and_invalid_indices_reject_atomically_before_noop() {
    for target in TARGETS {
        let mut editor = history_scene(target, 1);
        for indices in [
            BTreeSet::new(),
            [4].into(),
            [usize::MAX].into(),
            [0, 3, 4].into(),
        ] {
            assert_unchanged(
                &mut editor,
                command(target, indices, PathTransformSpec::default()),
                false,
            );
        }
        for (id, target) in [
            (999, target),
            (2, PathTarget::Shape),
            (1, PathTarget::Contents(999)),
            (1, PathTarget::Mask(999)),
            (1, PathTarget::Contents(1)),
            (1, PathTarget::Contents(3)),
        ] {
            assert_unchanged(
                &mut editor,
                Command::TransformPathPoses {
                    id,
                    target,
                    indices: [0].into(),
                    transform: PathTransformSpec::default(),
                },
                false,
            );
        }
        editor.execute(Command::ToggleLocked(1)).unwrap();
        for spec in [PathTransformSpec::default(), fixed_spec()] {
            assert!(
                assert_unchanged(&mut editor, command(target, [0].into(), spec), false)
                    .unwrap()
                    .contains("Unlock")
            );
        }
    }
}

#[test]
fn whole_pose_invalid_unselected_base_unused_pose_and_topology_report_the_source() {
    for target in TARGETS {
        for case in 0..5 {
            let mut editor = history_scene(target, 1);
            let (base, track) = target_mut(&mut editor, target);
            let label = match case {
                0 => {
                    base.vertices[3].position[0] = 1_000_001.;
                    "base"
                }
                1 => {
                    track.poses[3].vertices[3].incoming[1] = 1_000_001.;
                    "stored pose 3"
                }
                2 => {
                    track.poses[3].vertices.pop();
                    "Stored pose 3"
                }
                3 => {
                    track.poses[3].closed = false;
                    "Stored pose 3"
                }
                _ => {
                    track.poses[3].vertices.clear();
                    "stored pose 3"
                }
            };
            for spec in [
                PathTransformSpec::default(),
                [0., 0., 0., 0., 0., 0., 0.].into(),
            ] {
                let error = assert_unchanged(&mut editor, command(target, [0].into(), spec), false)
                    .unwrap();
                assert!(error.contains(label), "{error}");
            }
        }
    }
}

#[test]
fn whole_pose_late_output_overflow_and_nested_batches_are_atomic() {
    for target in TARGETS {
        let mut editor = history_scene(target, 1);
        target_mut(&mut editor, target).1.poses[3].vertices[0].position[0] = 1_000_000.;
        let shift = command(target, [0].into(), [1., 0., 0., 100., 100., 0., 0.].into());
        let error = assert_unchanged(&mut editor, shift.clone(), false).unwrap();
        assert!(error.contains("Stored pose 3"), "{error}");
        assert_unchanged(
            &mut editor,
            Command::Batch(vec![
                command(target, [1].into(), fixed_spec()),
                Command::Batch(vec![shift]),
            ]),
            false,
        );
        target_mut(&mut editor, target).1.poses[3].vertices[0].position[0] = 0.;
        target_mut(&mut editor, target).0.vertices[0].outgoing[1] = 1_000_000.;
        let error = assert_unchanged(
            &mut editor,
            command(target, [0].into(), [0., 0., 0., 100., 200., 0., 0.].into()),
            false,
        )
        .unwrap();
        assert!(error.contains("Base path"), "{error}");
    }
}

#[test]
fn whole_pose_checks_timing_and_full_project_validation_without_repair() {
    for case in 0..7 {
        let target = PathTarget::Shape;
        let mut editor = history_scene(target, 1);
        let duration = editor.project().composition.duration;
        let (_, track) = target_mut(&mut editor, target);
        match case {
            0 => track.timing.value = 99.,
            1 => track.timing.keys.get_mut(&10).unwrap().value = -1.,
            2 => track.timing.keys.get_mut(&10).unwrap().value = 1.5,
            3 => {
                let key = track.timing.keys.get(&10).unwrap().clone();
                track.timing.keys.insert(duration, key);
            }
            4 => {
                track.timing.keys.get_mut(&10).unwrap().interpolation =
                    Interpolation::Bezier(Bezier {
                        x1: -1.,
                        ..Default::default()
                    })
            }
            5 => {
                track.timing.keys.get_mut(&10).unwrap().temporal.outgoing = Some(TemporalHandle {
                    slope: 1.,
                    influence: 0.5,
                })
            }
            _ => editor.current.project.composition.width = 0,
        }
        assert_unchanged(
            &mut editor,
            command(target, [0].into(), PathTransformSpec::default()),
            false,
        );
    }
}

#[test]
fn whole_pose_pool_vertex_and_key_limits_accept_boundaries_and_reject_overflow() {
    // Exercise limits directly, avoiding unrelated document cloning/serialization.
    for (count, poses) in [(2, 10000), (20, 10000), (1000, 200)] {
        let mut base = geometry(count, false, 0.);
        let mut track = PathAnimation {
            poses: vec![base.clone(); poses],
            timing: AnimatedProperty::new(0.),
        };
        track
            .transform_poses(
                &mut base,
                &[0].into(),
                &[1., 0., 0., 100., 100., 0., 0.].into(),
                20000,
                PROJECT_VERSION,
            )
            .unwrap();
        assert_eq!(track.poses.len(), poses);
        assert_eq!(track.poses[poses - 1].vertices[0].position[0], 1.);
        track.poses.push(base.clone());
        let invalid = (base.clone(), track.clone());
        assert!(
            track
                .transform_poses(
                    &mut base,
                    &[0].into(),
                    &PathTransformSpec::default(),
                    20000,
                    PROJECT_VERSION
                )
                .unwrap_err()
                .contains("limit")
        );
        assert_eq!((base, track), invalid);
    }
    let mut base = geometry(2, false, 0.);
    let mut track = PathAnimation {
        poses: vec![base.clone()],
        timing: AnimatedProperty {
            value: 0.,
            keys: (0..10000)
                .map(|frame| {
                    (
                        frame,
                        Keyframe {
                            value: 0.,
                            interpolation: Interpolation::Linear,
                            temporal: TemporalHandles::default(),
                        },
                    )
                })
                .collect(),
        },
    };
    track
        .transform_poses(
            &mut base,
            &[0].into(),
            &PathTransformSpec::default(),
            20000,
            PROJECT_VERSION,
        )
        .unwrap();
    track.timing.keys.insert(
        10000,
        Keyframe {
            value: 0.,
            interpolation: Interpolation::Linear,
            temporal: TemporalHandles::default(),
        },
    );
    assert!(
        track
            .transform_poses(
                &mut base,
                &[0].into(),
                &PathTransformSpec::default(),
                20000,
                PROJECT_VERSION
            )
            .is_err()
    );
}

#[test]
fn whole_pose_pure_batches_preserve_schema_but_mixed_and_empty_batch_semantics_remain() {
    let target = PathTarget::Shape;
    let start = history_scene(target, 1);
    let transform = command(target, [0, 2].into(), fixed_spec());
    let mut direct = clone_editor(&start);
    direct.execute(transform.clone()).unwrap();
    let mut nested = clone_editor(&start);
    nested
        .execute(Command::Batch(vec![Command::Batch(vec![transform])]))
        .unwrap();
    assert_eq!(nested.current, direct.current);
    assert_eq!(nested.undo, direct.undo);
    assert_eq!(nested.redo, direct.redo);
    assert_eq!(nested.project().version, PROJECT_VERSION);
    let mut empty = clone_editor(&start);
    empty.execute(Command::Batch(vec![])).unwrap();
    assert_ne!(empty.project().version, PROJECT_VERSION);
    for cmd in [
        Command::Batch(vec![Command::Batch(vec![])]),
        Command::Batch(vec![
            command(target, [0].into(), PathTransformSpec::default()),
            Command::Batch(vec![]),
        ]),
    ] {
        let mut editor = clone_editor(&start);
        editor.execute(cmd).unwrap();
        assert_eq!(editor.current, empty.current);
        assert_eq!(editor.undo, empty.undo);
        assert_eq!(editor.redo, empty.redo);
    }
    let color = Command::SetColor {
        id: 2,
        color: 0x771122,
    };
    let mut mixed_expected = clone_editor(&start);
    mixed_expected.execute(color.clone()).unwrap();
    let mut mixed = clone_editor(&start);
    mixed
        .execute(Command::Batch(vec![
            command(target, [0].into(), PathTransformSpec::default()),
            color,
        ]))
        .unwrap();
    assert_eq!(mixed.current, mixed_expected.current);
    assert_eq!(mixed.undo, mixed_expected.undo);
    assert_eq!(mixed.redo, mixed_expected.redo);
}

#[test]
fn whole_pose_nonfinite_parameters_and_invalid_nested_selection_preserve_redo() {
    for target in TARGETS {
        let mut editor = history_scene(target, 2);
        for field in 0..7 {
            for value in [f64::NAN, f64::INFINITY, -f64::INFINITY] {
                let mut values = [0., 0., 0., 100., 100., 0., 0.];
                values[field] = value;
                assert_unchanged(
                    &mut editor,
                    command(target, [0, 2].into(), values.into()),
                    false,
                );
            }
        }
        assert_unchanged(
            &mut editor,
            Command::Batch(vec![
                command(target, [0, 2].into(), fixed_spec()),
                Command::Batch(vec![command(
                    target,
                    [0, usize::MAX].into(),
                    PathTransformSpec::default(),
                )]),
            ]),
            false,
        );
    }
}

#[test]
fn whole_pose_valid_stored_geometry_does_not_add_an_unbounded_overshoot_frame_scan() {
    let target = PathTarget::Shape;
    let mut editor = scene(target, true, 1);
    let (base, track) = target_mut(&mut editor, target);
    base.vertices = vec![PathVertex::corner([0., 0.]); 4];
    track.poses = [0., 1_000_000.]
        .map(|x| VectorPath {
            closed: true,
            vertices: vec![PathVertex::corner([x, 0.]); 4],
        })
        .to_vec();
    track.timing.value = 0.;
    track.timing.keys = [(0, 0.), (20, 1.)]
        .into_iter()
        .map(|(frame, value)| {
            (
                frame,
                Keyframe {
                    value,
                    interpolation: Interpolation::Bezier(Bezier {
                        x1: 0.25,
                        y1: 3.,
                        x2: 0.75,
                        y2: 3.,
                    }),
                    temporal: TemporalHandles::default(),
                },
            )
        })
        .collect();
    assert!(track.at(base, 10).vertices[0].position[0] > 1_000_000.);
    editor.current.project.validate().unwrap();
    editor
        .execute(command(
            target,
            [0].into(),
            [0., 1., 0., 100., 100., 0., 0.].into(),
        ))
        .unwrap();
    let (base, track) = editor
        .project()
        .composition
        .layer(1)
        .unwrap()
        .path_animation(target)
        .unwrap();
    assert!(base.valid() && track.poses.iter().all(VectorPath::valid));
    assert!(track.at(base, 10).vertices[0].position[0] > 1_000_000.);
    assert_eq!(track.at(base, 10).vertices[0].position[1], 1.);
}
