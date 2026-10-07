use super::*;

fn path(offset: f64) -> VectorPath {
    VectorPath {
        closed: true,
        vertices: [[2., 4.], [-6., 8.], [10., -12.], [-4., -6.]]
            .into_iter()
            .map(|[x, y]| PathVertex {
                position: [x + offset, y],
                incoming: [2., -1.],
                outgoing: [-3., 4.],
            })
            .collect(),
    }
}

fn animation(mode: usize) -> PathAnimation {
    if mode == 0 {
        return PathAnimation::default();
    }
    let mut animation: PathAnimation = serde_json::from_value(serde_json::json!({
        "poses": [path(0.), path(20.), path(50.), path(90.), path(20.)],
        "timing": {"value": 2., "keys": {"0": {"value": 0., "interpolation": "Linear"},
            "20": {"value": 1., "interpolation": "Smooth"},
            "40": {"value": 1., "interpolation": "Hold"}}}
    }))
    .unwrap();
    if mode == 2 {
        animation.timing.keys.clear();
    }
    animation
}

fn node(id: u64, kind: ContentsKind) -> ContentsNode {
    let parameters = if matches!(kind, ContentsKind::Group(_)) {
        Property::ALL
            .into_iter()
            .map(|p| {
                (
                    ContentsParam::Transform(p),
                    AnimatedProperty::new(
                        if matches!(p, Property::ScaleX | Property::ScaleY | Property::Opacity) {
                            100.
                        } else {
                            0.
                        },
                    ),
                )
            })
            .chain([
                (ContentsParam::Skew, AnimatedProperty::new(0.)),
                (ContentsParam::SkewAxis, AnimatedProperty::new(0.)),
            ])
            .collect()
    } else {
        BTreeMap::new()
    };
    ContentsNode {
        id,
        name: format!("Item {id}"),
        enabled: true,
        centered: false,
        kind,
        composite: PaintComposite::default(),
        blend: PaintBlend::Normal,
        parameters,
    }
}

fn path_node(id: u64, mode: usize) -> ContentsNode {
    node(
        id,
        ContentsKind::Path {
            path: path(0.),
            animation: animation(mode),
        },
    )
}

fn group(id: u64, children: Vec<ContentsNode>, values: [(Property, f64); 4]) -> ContentsNode {
    let mut group = node(id, ContentsKind::Group(children));
    for (property, value) in values {
        group
            .parameters
            .get_mut(&ContentsParam::Transform(property))
            .unwrap()
            .value = value;
    }
    group
}

fn scene(mode: usize) -> Editor {
    use Property::*;
    let nested = group(
        3,
        vec![path_node(4, mode)],
        [
            (PositionX, 1.),
            (PositionY, 5.),
            (ScaleX, 200.),
            (ScaleY, 50.),
        ],
    );
    let root = group(
        1,
        vec![path_node(2, mode), nested],
        [
            (PositionX, 3.),
            (PositionY, -2.),
            (ScaleX, 50.),
            (ScaleY, -100.),
        ],
    );
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents {
                items: vec![root, path_node(7, mode)],
                next_id: 8,
            }),
            name: "Cross-path source".into(),
            width: 100.,
            height: 100.,
        })
        .unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    for layer in &mut editor.current.project.composition.layers {
        for property in Property::ALL {
            layer.properties.insert(
                property,
                AnimatedProperty::new(if matches!(property, ScaleX | ScaleY | Opacity) {
                    100.
                } else {
                    0.
                }),
            );
        }
    }
    let layer = editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap();
    layer.parent = Some(2);
    layer.transform_offset = Affine([-2., 0., 1., 4., 0., 0.]);
    layer.path_masks = vec![PathMask {
        id: 1,
        path: path(90.),
        ..Default::default()
    }];
    layer.next_mask_id = 2;
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 2)
        .unwrap()
        .transform_offset = Affine([1., 0., 0., 1., 10., -6.]);
    editor.current.project.version = 44;
    editor.current.project.validate().unwrap();
    editor.execute(Command::ToggleVisible(3)).unwrap();
    editor.undo();
    editor.select(1);
    editor
}

fn layer(editor: &Editor) -> &Layer {
    editor.project().composition.layer(1).unwrap()
}
fn layer_mut(editor: &mut Editor) -> &mut Layer {
    editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap()
}
fn contents_mut(editor: &mut Editor) -> &mut ShapeContents {
    let Content::ShapeContents(contents) = &mut layer_mut(editor).content else {
        panic!()
    };
    contents
}
fn sample(editor: &Editor, item: u64, frame: Frame) -> VectorPath {
    layer(editor)
        .copy_path_pose(PathTarget::Contents(item), frame)
        .unwrap()
}
fn command(frame: Frame, transform: PathTransformSpec) -> Command {
    Command::TransformContentsPoints {
        id: 1,
        frame,
        selections: [(2, [0, 2].into()), (4, [0, 1].into()), (7, [1].into())].into(),
        transform,
    }
}
fn selected(item: u64) -> BTreeSet<usize> {
    match item {
        2 => [0, 2].into(),
        4 => [0, 1].into(),
        _ => [1].into(),
    }
}
fn spec() -> PathTransformSpec {
    [8., -4., 90., -200., 50., 6., -10.].into()
}

// Literal independent conjugations of world F(x,y)=(9-y/2,-2-2x):
// item 2 W=(-x-y+2,-4y-14), item 4 W=(-2x-y/2-4,-2y-34),
// item 7 W=(-2x+y+10,4y-6). No production affine helper is used.
fn expected(source: &VectorPath, item: u64) -> VectorPath {
    let mut result = source.clone();
    for index in selected(item) {
        let vertex = &mut result.vertices[index];
        let f = |[x, y]: [f64; 2]| match item {
            2 => [0.5 * x - 1.5 * y, -0.5 * x - 0.5 * y],
            4 => [0.5 * x - 0.375 * y, -2. * x - 0.5 * y],
            _ => [0.5 * x + 0.75 * y, x - 0.5 * y],
        };
        let [x, y] = f(vertex.position);
        vertex.position = match item {
            2 => [x - 12., y - 2.],
            4 => [x - 10., y - 20.],
            _ => [x - 3., y - 4.],
        };
        vertex.incoming = f(vertex.incoming);
        vertex.outgoing = f(vertex.outgoing);
    }
    result
}

fn unchanged(editor: &mut Editor, command: Command, success: bool) {
    let current = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    let before = serde_json::to_vec(&editor.current.project).unwrap();
    let result = editor.execute(command);
    assert_eq!(result.is_ok(), success, "{result:?}");
    assert_eq!(format!("{:?}", editor.current), format!("{current:?}"));
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    assert_eq!(serde_json::to_vec(&editor.current.project).unwrap(), before);
}

#[test]
fn cross_path_literal_oracle_nested_groups_parent_layers_and_history() {
    for mode in 0..3 {
        for frame in [10, 20] {
            let mut editor = scene(mode);
            let before = editor.current.clone();
            let undo_count = editor.undo.len();
            let samples: Vec<_> = [2, 4, 7]
                .map(|item| (item, sample(&editor, item, frame)))
                .into();
            editor.execute(command(frame, spec())).unwrap();
            for (item, original) in samples {
                assert_eq!(sample(&editor, item, frame), expected(&original, item));
            }
            assert_eq!(editor.project().version, before.project.version);
            assert_eq!(editor.project().asset_library, before.project.asset_library);
            assert_eq!(
                layer(&editor).path_masks,
                before.project.composition.layer(1).unwrap().path_masks
            );
            assert_eq!(
                editor.project().composition.layer(2),
                before.project.composition.layer(2)
            );
            assert_eq!(
                editor.project().composition.layer(3),
                before.project.composition.layer(3)
            );
            assert_eq!(editor.undo.len(), undo_count + 1);
            assert!(!editor.can_redo());
            let after = editor.current.clone();
            let json = editor.project().to_json().unwrap();
            assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
            let native = project_file::encode(editor.project(), None).unwrap();
            assert_eq!(
                project_file::decode(&native).unwrap().project,
                *editor.project()
            );
            editor.undo();
            assert_eq!(editor.current, before);
            editor.redo();
            assert_eq!(editor.current, after);
        }
    }
}

#[test]
fn cross_path_animation_only_changes_current_key_and_never_recycles_authored_slots() {
    for mode in [1, 2] {
        for frame in [10, 20] {
            let mut editor = scene(mode);
            let originals: Vec<_> = [2, 4, 7]
                .map(|item| {
                    (
                        item,
                        layer(&editor)
                            .path_animation(PathTarget::Contents(item))
                            .unwrap()
                            .clone(),
                    )
                })
                .into_iter()
                .map(|(item, (base, animation))| (item, base.clone(), animation.clone()))
                .collect();
            editor.execute(command(frame, spec())).unwrap();
            for (item, base, animation) in originals {
                let (new_base, new_animation) = layer(&editor)
                    .path_animation(PathTarget::Contents(item))
                    .unwrap();
                assert_eq!(*new_base, base);
                let old = serde_json::to_value(&animation).unwrap();
                let new = serde_json::to_value(new_animation).unwrap();
                let old_poses = old["poses"].as_array().unwrap();
                let new_poses = new["poses"].as_array().unwrap();
                assert_eq!(&new_poses[..old_poses.len()], old_poses);
                assert_eq!(new_poses.len(), old_poses.len() + 1);
                if mode == 1 {
                    assert_eq!(new_animation.timing.value, animation.timing.value);
                    for (f, key) in &animation.timing.keys {
                        if *f == frame {
                            assert_eq!(
                                new_animation.timing.keys[f].interpolation,
                                key.interpolation
                            );
                            assert_eq!(new_animation.timing.keys[f].temporal, key.temporal);
                        } else {
                            assert_eq!(&new_animation.timing.keys[f], key);
                        }
                    }
                    assert_eq!(new_animation.timing.keys[&frame].value, 5.);
                    assert_eq!(
                        new_animation.timing.keys.len(),
                        animation.timing.keys.len() + usize::from(frame == 10)
                    );
                } else {
                    assert!(new_animation.timing.keys.is_empty());
                    assert_eq!(new_animation.timing.value, 5.);
                }
            }
        }
    }
}

#[test]
fn cross_path_identity_fixed_members_and_exact_roundtrip_keep_redo_and_bytes() {
    for mode in 0..3 {
        let mut editor = scene(mode);
        for transform in [
            PathTransformSpec::default(),
            [0., 0., -1080., 100., 100., 1e308, -1e308].into(),
            [0., 0., 180., -100., -100., 1e308, -1e308].into(),
        ] {
            unchanged(&mut editor, command(10, transform), true);
            unchanged(
                &mut editor,
                Command::Batch(vec![Command::Batch(vec![command(10, transform)])]),
                true,
            );
        }
    }
    let mut editor = scene(0);
    unchanged(
        &mut editor,
        Command::Batch(vec![
            command(10, [8., -4., 0., 100., 100., 0., 0.].into()),
            command(10, [-8., 4., 0., 100., 100., 0., 0.].into()),
        ]),
        true,
    );
    // Every selected point of item 2 lies at the world pivot, with zero handles.
    // Other members still change, so item 2 must not allocate a current-frame key.
    let mut editor = scene(1);
    let (base, animation) = layer_mut(&mut editor)
        .path_animation_mut(PathTarget::Contents(2))
        .unwrap();
    for vertex in &mut base.vertices {
        *vertex = PathVertex::corner([0., 0.]);
    }
    *animation = serde_json::from_value(serde_json::json!({"poses":[base.clone(),base.clone()], "timing":{"value":0.,"keys":{"0":{"value":0.,"interpolation":"Hold"},"20":{"value":1.,"interpolation":"Linear"}}}})).unwrap();
    let original = (base.clone(), animation.clone());
    editor
        .execute(command(10, [0., 0., 90., 100., 100., 2., -14.].into()))
        .unwrap();
    let (base, animation) = layer(&editor)
        .path_animation(PathTarget::Contents(2))
        .unwrap();
    assert_eq!((base.clone(), animation.clone()), original);
}

#[test]
fn cross_path_invalid_member_or_source_rejects_every_member_and_keeps_history() {
    for case in 0..17 {
        let mut editor = scene(1);
        let mut cmd = command(10, spec());
        let Command::TransformContentsPoints {
            id,
            frame,
            selections,
            transform,
        } = &mut cmd
        else {
            panic!()
        };
        match case {
            0 => *id = 999,
            1 => *frame = editor.project().composition.duration,
            2 => selections.clear(),
            3 => {
                selections.insert(999, [0].into());
            }
            4 => {
                selections.insert(1, [0].into());
            }
            5 => {
                selections.insert(7, BTreeSet::new());
            }
            6 => {
                selections.insert(7, [999].into());
            }
            7 => layer_mut(&mut editor).locked = true,
            8 => contents_mut(&mut editor).node_mut(7).unwrap().enabled = false,
            9 => contents_mut(&mut editor).node_mut(3).unwrap().enabled = false,
            10 => {
                contents_mut(&mut editor)
                    .node_mut(3)
                    .unwrap()
                    .parameters
                    .get_mut(&ContentsParam::Transform(Property::ScaleX))
                    .unwrap()
                    .value = 0.;
            }
            11 => transform.translation[0] = f64::NAN,
            12 => transform.pivot[1] = f64::INFINITY,
            13 => transform.translation[0] = 1e12,
            14 => {
                let (base, _) = layer_mut(&mut editor)
                    .path_animation_mut(PathTarget::Contents(7))
                    .unwrap();
                base.vertices[0].position[0] = f64::NAN;
            }
            15 => {
                let (_, animation) = layer_mut(&mut editor)
                    .path_animation_mut(PathTarget::Contents(7))
                    .unwrap();
                animation.timing.value = 999.;
            }
            _ => {
                contents_mut(&mut editor).node_mut(7).unwrap().id = 2;
            }
        }
        unchanged(&mut editor, cmd, false);
    }
    let mut editor = scene(1);
    unchanged(
        &mut editor,
        Command::Batch(vec![
            command(10, spec()),
            Command::TransformContentsPoints {
                id: 1,
                frame: 10,
                selections: [(999, [0].into())].into(),
                transform: spec(),
            },
        ]),
        false,
    );
}

#[test]
fn cross_path_requires_contents_and_active_composition_without_converting_sources() {
    for content in [
        Content::Rectangle,
        Content::Shape(Shape {
            path: Some(path(0.)),
            ..Default::default()
        }),
    ] {
        let mut editor = scene(0);
        layer_mut(&mut editor).content = content;
        unchanged(&mut editor, command(10, spec()), false);
    }
    let mut editor = scene(0);
    let other = editor.current.project.composition.clone();
    editor.current.project.other_compositions.insert(2, other);
    editor.current.project.next_composition_id = 3;
    editor.current.project.composition.layers.clear();
    editor.current.project.validate().unwrap();
    unchanged(&mut editor, command(10, spec()), false);
}

#[test]
fn cross_path_geometric_identity_cardinals_tiny_changes_and_fixed_points() {
    let mut source = path(0.);
    source.vertices[0].incoming = [-0., 0.];
    source.vertices[0].outgoing = [0., -0.];
    let indices = [0].into();
    for world in [
        Affine::default(),
        Affine([-2., 0., 1., 4., 10., -6.]),
        Affine([0., -2., 4., 1., -100., 30.]),
    ] {
        let identity =
            transform_path_in_world(&source, &indices, &PathTransformSpec::default(), world)
                .unwrap();
        assert_eq!(
            serde_json::to_vec(&identity).unwrap(),
            serde_json::to_vec(&source).unwrap()
        );
    }
    let mut source = path(0.);
    source.vertices[0] = PathVertex {
        position: [0., 0.],
        incoming: [1., 0.],
        outgoing: [0., 1.],
    };
    let tiny = transform_path_in_world(
        &source,
        &indices,
        &[0., 0., 1e-14, 100., 100., 0., 0.].into(),
        Affine::default(),
    )
    .unwrap();
    assert_eq!(tiny.vertices[0].position, [0., 0.]);
    assert!(tiny.vertices[0].incoming[1] > 0.);
    assert!(tiny.vertices[0].outgoing[0] < 0.);
    assert_eq!(tiny.vertices[1..], source.vertices[1..]);
    // A tiny but nonzero output scale must survive, not cancel against source.
    let tiny = transform_path_in_world(
        &path(0.),
        &indices,
        &[0., 0., 0., 1e-15, 1e-15, 0., 0.].into(),
        Affine::default(),
    )
    .unwrap();
    assert_eq!(tiny.vertices[0].position, [2e-17, 4e-17]);
    // World anchor is precisely the pivot and remains bit-identical.
    source.vertices[0] = PathVertex::corner([0.125, -0.25]);
    let fixed = transform_path_in_world(
        &source,
        &indices,
        &[0., 0., 137., -170., 33., 9.5, -7.].into(),
        Affine([-2., 0., 1., 4., 10., -6.]),
    )
    .unwrap();
    assert_eq!(fixed.vertices[0], source.vertices[0]);
}

#[test]
fn cross_path_geometric_rejections_match_editor_inverse_eligibility_even_for_identity() {
    let source = path(0.);
    let indices = [0].into();
    for world in [
        Affine([0., 0., 0., 1., 0., 0.]),
        Affine([1., 1., 1., 1., 0., 0.]),
        Affine([1e-6, 0., 0., 1e-6, 0., 0.]),
        Affine([1e-13, 0., 0., 1e4, 0., 0.]),
        Affine([1., 0., 0., 1., f64::INFINITY, 0.]),
    ] {
        assert!(
            transform_path_in_world(&source, &indices, &PathTransformSpec::default(), world)
                .is_err()
        );
    }
    for indices in [BTreeSet::new(), [source.vertices.len()].into()] {
        assert!(
            transform_path_in_world(
                &source,
                &indices,
                &PathTransformSpec::default(),
                Affine::default()
            )
            .is_err()
        );
    }
}

#[test]
fn cross_path_uses_current_frame_animated_group_and_layer_ancestor_spaces() {
    let mut editor = scene(0);
    let group = contents_mut(&mut editor).node_mut(1).unwrap();
    group
        .parameters
        .get_mut(&ContentsParam::Transform(Property::PositionX))
        .unwrap()
        .keys = [(0, 3.), (20, 11.)]
        .map(|(frame, value)| {
            (
                frame,
                Keyframe {
                    value,
                    interpolation: Interpolation::Linear,
                    temporal: TemporalHandles::default(),
                },
            )
        })
        .into();
    let parent = editor
        .current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 2)
        .unwrap();
    parent
        .properties
        .get_mut(&Property::PositionY)
        .unwrap()
        .keys = [(0, 0.), (20, 8.)]
        .map(|(frame, value)| {
            (
                frame,
                Keyframe {
                    value,
                    interpolation: Interpolation::Linear,
                    temporal: TemporalHandles::default(),
                },
            )
        })
        .into();
    editor.execute(command(20, spec())).unwrap();
    for item in [2, 4, 7] {
        let mut oracle = expected(&path(0.), item);
        // Independently: +8 group X and +8 ancestor-layer Y change the local
        // conjugation's constant by (-6,-6), (-3,-12), or (1,-2).
        let delta = match item {
            2 => [-6., -6.],
            4 => [-3., -12.],
            _ => [1., -2.],
        };
        for index in selected(item) {
            for axis in 0..2 {
                oracle.vertices[index].position[axis] += delta[axis];
            }
        }
        assert_eq!(sample(&editor, item, 20), oracle);
    }
}

#[test]
fn cross_path_storage_capacity_never_recycles_unused_poses_and_noop_never_allocates() {
    let mut editor = scene(1);
    let (base, animation) = layer_mut(&mut editor)
        .path_animation_mut(PathTarget::Contents(7))
        .unwrap();
    let mut value = serde_json::to_value(&*animation).unwrap();
    value["poses"] = serde_json::to_value(vec![base.clone(); 10000]).unwrap();
    *animation = serde_json::from_value(value).unwrap();
    unchanged(&mut editor, command(10, PathTransformSpec::default()), true);
    unchanged(&mut editor, command(10, spec()), false);
    // Existing equal output may be reused even at capacity; all original slots
    // remain unchanged. Only the key reference changes.
    let transformed = expected(&path(0.), 7);
    let (_, animation) = layer_mut(&mut editor)
        .path_animation_mut(PathTarget::Contents(7))
        .unwrap();
    let mut value = serde_json::to_value(&*animation).unwrap();
    value["poses"][9999] = serde_json::to_value(&transformed).unwrap();
    *animation = serde_json::from_value(value.clone()).unwrap();
    editor
        .execute(Command::TransformContentsPoints {
            id: 1,
            frame: 10,
            selections: [(7, selected(7))].into(),
            transform: spec(),
        })
        .unwrap();
    assert_eq!(sample(&editor, 7, 10), transformed);
    let animation = layer(&editor)
        .path_animation(PathTarget::Contents(7))
        .unwrap()
        .1;
    assert_eq!(
        serde_json::to_value(animation).unwrap()["poses"],
        value["poses"]
    );
    assert_eq!(animation.timing.keys[&10].value, 9999.);
}

#[test]
fn cross_path_key_capacity_and_late_tangent_overflow_reject_atomically() {
    let mut editor = scene(1);
    editor.current.project.composition.duration = 20000;
    let (_, animation) = layer_mut(&mut editor)
        .path_animation_mut(PathTarget::Contents(7))
        .unwrap();
    animation.timing.keys = (0..10000)
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
        .collect();
    unchanged(
        &mut editor,
        command(10000, PathTransformSpec::default()),
        true,
    );
    unchanged(&mut editor, command(10000, spec()), false);
    let mut editor = scene(0);
    let (base, _) = layer_mut(&mut editor)
        .path_animation_mut(PathTarget::Contents(7))
        .unwrap();
    base.vertices[1].outgoing = [1_000_000., 0.];
    unchanged(
        &mut editor,
        command(10, [0., 0., 0., 400., 400., 0., 0.].into()),
        false,
    );
}

#[test]
fn cross_path_legacy_contents_and_embedded_assets_keep_source_identity() {
    let mut editor = scene(0);
    editor
        .execute(Command::ImportAsset {
            content: Content::Image { png: "YWJj".into() },
            width: 64.,
            height: 48.,
            name: "Unrelated image".into(),
            folder: None,
            frame: Some(0),
        })
        .unwrap();
    let contents = contents_mut(&mut editor);
    for item in [1, 3] {
        let node = contents.node_mut(item).unwrap();
        node.parameters.remove(&ContentsParam::Skew);
        node.parameters.remove(&ContentsParam::SkewAxis);
    }
    editor.current.project.version = 43;
    editor.current.project.validate().unwrap();
    let source = editor.project().clone();
    editor.execute(command(10, spec())).unwrap();
    assert_eq!(editor.project().version, 43);
    assert_eq!(editor.project().asset_library, source.asset_library);
    assert_eq!(
        editor.project().composition.layer(4),
        source.composition.layer(4)
    );
    // Loading deliberately migrates old Contents groups; editing must not.
    let loaded = Project::from_json(&editor.project().to_json().unwrap()).unwrap();
    assert_eq!(loaded.composition.layer(4), source.composition.layer(4));
}

#[test]
fn cross_path_pure_translation_ignores_finite_enormous_pivot_without_overflow() {
    let source = path(0.);
    let result = transform_path_in_world(
        &source,
        &[0].into(),
        &[2., 4., 0., 100., 100., 1e308, -1e308].into(),
        Affine([0.5, 0., 0., 0.5, 0., 0.]),
    )
    .unwrap();
    assert_eq!(result.vertices[0].position, [6., 12.]);
    assert_eq!(result.vertices[0].incoming, source.vertices[0].incoming);
    assert_eq!(result.vertices[0].outgoing, source.vertices[0].outgoing);
    assert_eq!(result.vertices[1..], source.vertices[1..]);
}

#[test]
fn cross_path_reusing_equal_pose_cannot_change_unselected_signed_zero_bits() {
    let mut editor = scene(1);
    let mut source = path(0.);
    source.vertices[3].incoming = [-0., 0.];
    let mut prior_output = expected(&source, 7);
    prior_output.vertices[3].incoming = [0., 0.];
    let (_, animation) = layer_mut(&mut editor)
        .path_animation_mut(PathTarget::Contents(7))
        .unwrap();
    *animation=serde_json::from_value(serde_json::json!({"poses":[source,prior_output],"timing":{"value":0.,"keys":{"0":{"value":0.,"interpolation":"Linear"},"20":{"value":0.,"interpolation":"Linear"}}}})).unwrap();
    editor
        .execute(Command::TransformContentsPoints {
            id: 1,
            frame: 10,
            selections: [(7, selected(7))].into(),
            transform: spec(),
        })
        .unwrap();
    assert_eq!(
        sample(&editor, 7, 10).vertices[3].incoming[0].to_bits(),
        (-0.0_f64).to_bits()
    );
    let animation = layer(&editor)
        .path_animation(PathTarget::Contents(7))
        .unwrap()
        .1;
    assert_eq!(animation.timing.keys[&10].value, 2.);
    assert_eq!(
        serde_json::to_value(animation).unwrap()["poses"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
