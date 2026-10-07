use super::*;

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents::default()),
            width: 200.,
            height: 120.,
            name: "Contents".into(),
        })
        .unwrap();
    editor
}

fn contents(project: &Project) -> &ShapeContents {
    let Content::ShapeContents(contents) = &project.composition.layer(1).unwrap().content else {
        panic!("expected Contents")
    };
    contents
}

fn contents_mut(project: &mut Project) -> &mut ShapeContents {
    let Content::ShapeContents(contents) = &mut project.composition.layers[0].content else {
        panic!("expected Contents")
    };
    contents
}

fn add(editor: &mut Editor, parent: u64, kind: ContentsKind) -> u64 {
    let before: BTreeSet<_> = contents(editor.project())
        .rows()
        .iter()
        .map(|(_, _, n)| n.id)
        .collect();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add { parent, kind },
        })
        .unwrap();
    contents(editor.project())
        .rows()
        .into_iter()
        .find(|(_, _, n)| !before.contains(&n.id))
        .unwrap()
        .2
        .id
}

fn pair() -> (Editor, u64, u64) {
    let mut editor = scene();
    let a = add(
        &mut editor,
        0,
        ContentsKind::Parametric(ShapeKind::Rectangle),
    );
    let b = add(&mut editor, 0, ContentsKind::Parametric(ShapeKind::Ellipse));
    editor.clear_history();
    (editor, a, b)
}

fn shared(
    parent: u64,
    items: &[u64],
    parameter: ContentsParam,
    frame: Frame,
    value: f64,
) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::SetSharedValue {
            parent,
            items: items.to_vec(),
            parameter,
            frame,
            value,
        },
    }
}

fn key(value: f64) -> Keyframe {
    Keyframe {
        value,
        interpolation: Interpolation::Linear,
        temporal: TemporalHandles::default(),
    }
}

fn track_mut(editor: &mut Editor, item: u64, parameter: ContentsParam) -> &mut AnimatedProperty {
    contents_mut(&mut editor.current.project)
        .node_mut(item)
        .unwrap()
        .parameters
        .get_mut(&parameter)
        .unwrap()
}

fn with_redo(editor: &mut Editor) {
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "C".into(),
        })
        .unwrap();
    editor.undo();
    assert!(editor.can_redo());
}

fn assert_unchanged(editor: &Editor, current: &Snapshot, undo: &[Snapshot], redo: &[Snapshot]) {
    assert_eq!(&editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    // PartialEq alone deliberately treats opposite signed zeros as equal.
    assert_eq!(
        serde_json::to_vec(editor.project()).unwrap(),
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

#[test]
fn contents_bulk_intersects_typed_heterogeneous_parameters_without_side_effects() {
    use ContentsParam::{Height, Shape as S, Transform as T, Width};
    use Property::{PositionX, PositionY};
    let cases = [
        (
            ContentsKind::Parametric(ShapeKind::Rectangle),
            ContentsKind::Parametric(ShapeKind::Ellipse),
            vec![Width, Height, T(PositionX), T(PositionY)],
        ),
        (
            ContentsKind::Parametric(ShapeKind::Polygon),
            ContentsKind::Parametric(ShapeKind::Star),
            vec![
                Width,
                Height,
                T(PositionX),
                T(PositionY),
                S(ShapeParam::Points),
            ],
        ),
        (
            ContentsKind::Fill { even_odd: false },
            ContentsKind::GradientFill {
                even_odd: true,
                gradient: ShapeGradient::default(),
            },
            vec![S(ShapeParam::FillOpacity)],
        ),
        (
            ContentsKind::Fill { even_odd: false },
            ContentsKind::Stroke(ShapeStroke::default()),
            vec![],
        ),
        (
            ContentsKind::Group(vec![]),
            ContentsKind::Parametric(ShapeKind::RoundedRectangle),
            vec![T(PositionX), T(PositionY)],
        ),
        (
            ContentsKind::Parametric(ShapeKind::Rectangle),
            ContentsKind::Path {
                path: VectorPath {
                    vertices: vec![PathVertex::corner([0., 0.]), PathVertex::corner([1., 1.])],
                    closed: false,
                },
                animation: PathAnimation::default(),
            },
            vec![],
        ),
    ];
    for (a_kind, b_kind, expected) in cases {
        let mut editor = scene();
        let a = add(&mut editor, 0, a_kind);
        let b = add(&mut editor, 0, b_kind);
        with_redo(&mut editor);
        let (current, undo, redo) = (
            editor.current.clone(),
            editor.undo.clone(),
            editor.redo.clone(),
        );
        for ids in [[a, b], [b, a]] {
            assert_eq!(
                contents(editor.project())
                    .shared_parameters(0, &ids)
                    .unwrap(),
                expected
            );
        }
        assert_unchanged(&editor, &current, &undo, &redo);
    }
}

#[test]
fn contents_bulk_uses_first_source_siblings_parameter_order_not_selection_order() {
    let mut editor = scene();
    let a = add(&mut editor, 0, ContentsKind::Group(vec![]));
    let b = add(&mut editor, 0, ContentsKind::Group(vec![]));
    let c = contents(editor.project());
    assert_eq!(c.items[0].id, b);
    assert_eq!(
        c.shared_parameters(0, &[a, b]).unwrap(),
        c.node(b).unwrap().parameter_order()
    );
    assert_eq!(
        c.shared_parameters(0, &[b, a]).unwrap(),
        c.node(b).unwrap().parameter_order()
    );
    assert_ne!(
        c.node(b).unwrap().parameter_order(),
        c.node(b)
            .unwrap()
            .parameters
            .keys()
            .copied()
            .collect::<Vec<_>>()
    );
}

#[test]
fn contents_bulk_gradient_stops_never_correspond_and_highlights_require_all_radial() {
    let mut editor = scene();
    let a = add(
        &mut editor,
        0,
        ContentsKind::GradientFill {
            even_odd: false,
            gradient: ShapeGradient::default(),
        },
    );
    let b = add(
        &mut editor,
        0,
        ContentsKind::GradientStroke {
            style: ShapeStroke::default(),
            gradient: ShapeGradient::default(),
        },
    );
    let endpoints = [
        GradientParam::StartX,
        GradientParam::StartY,
        GradientParam::EndX,
        GradientParam::EndY,
    ]
    .map(ContentsParam::Gradient)
    .to_vec();
    for radial_count in 0..=2 {
        let c = contents_mut(&mut editor.current.project);
        c.node_mut(a).unwrap().kind.gradient_mut().unwrap().radial = radial_count >= 1;
        c.node_mut(b).unwrap().kind.gradient_mut().unwrap().radial = radial_count >= 2;
        let mut expected = endpoints.clone();
        if radial_count == 2 {
            expected.extend([
                ContentsParam::Gradient(GradientParam::HighlightLength),
                ContentsParam::Gradient(GradientParam::HighlightAngle),
            ]);
        }
        assert_eq!(c.shared_parameters(0, &[a, b]).unwrap(), expected);
        let stops: Vec<_> = c
            .node(a)
            .unwrap()
            .parameters
            .keys()
            .filter(|p| matches!(p, ContentsParam::Gradient(g) if g.stop().is_some()))
            .copied()
            .collect();
        assert!(!stops.is_empty());
        for parameter in stops {
            let value = contents(editor.project())
                .node(a)
                .unwrap()
                .value_at(parameter, 0);
            reject(&mut editor, shared(0, &[a, b], parameter, 0, value));
        }
        if radial_count < 2 {
            reject(
                &mut editor,
                shared(
                    0,
                    &[a, b],
                    ContentsParam::Gradient(GradientParam::HighlightAngle),
                    0,
                    0.,
                ),
            );
        }
    }
    editor
        .execute(shared(
            0,
            &[a, b],
            ContentsParam::Gradient(GradientParam::EndX),
            0,
            123.5,
        ))
        .unwrap();
    assert_eq!(
        contents(editor.project())
            .node(a)
            .unwrap()
            .value_at(ContentsParam::Gradient(GradientParam::EndX), 0),
        123.5
    );
}

#[test]
fn contents_bulk_dash_intersection_uses_existing_pattern_positions_without_adding() {
    let mut editor = scene();
    let a = add(
        &mut editor,
        0,
        ContentsKind::Stroke(ShapeStroke {
            dashes: vec![8., 3., 9.],
            ..Default::default()
        }),
    );
    let b = add(
        &mut editor,
        0,
        ContentsKind::GradientStroke {
            style: ShapeStroke {
                dashes: vec![6., 2.],
                ..Default::default()
            },
            gradient: ShapeGradient::default(),
        },
    );
    let expected = [
        ShapeParam::StrokeWidth,
        ShapeParam::StrokeOpacity,
        ShapeParam::MiterLimit,
        ShapeParam::DashOffset,
        ShapeParam::DashLength(0),
        ShapeParam::DashLength(1),
    ]
    .map(ContentsParam::Shape);
    let mut expected = expected.to_vec();
    expected.sort();
    assert_eq!(
        contents(editor.project())
            .shared_parameters(0, &[a, b])
            .unwrap(),
        expected
    );
    editor
        .execute(shared(
            0,
            &[a, b],
            ContentsParam::Shape(ShapeParam::DashLength(1)),
            0,
            4.,
        ))
        .unwrap();
    for id in [a, b] {
        let node = contents(editor.project()).node(id).unwrap();
        assert_eq!(
            node.value_at(ContentsParam::Shape(ShapeParam::DashLength(1)), 0),
            4.
        );
        assert_eq!(
            node.kind.stroke().unwrap().dashes.len(),
            if id == a { 3 } else { 2 }
        );
    }
    reject(
        &mut editor,
        shared(
            0,
            &[a, b],
            ContentsParam::Shape(ShapeParam::DashLength(2)),
            0,
            4.,
        ),
    );
}

#[test]
fn contents_bulk_static_and_animated_changes_match_literal_tracks_in_one_history_step() {
    let mut editor = scene();
    let group = add(&mut editor, 0, ContentsKind::Group(vec![]));
    let a = add(
        &mut editor,
        group,
        ContentsKind::Parametric(ShapeKind::Rectangle),
    );
    let b = add(
        &mut editor,
        group,
        ContentsKind::Parametric(ShapeKind::Ellipse),
    );
    let c = add(
        &mut editor,
        group,
        ContentsKind::Parametric(ShapeKind::Star),
    );
    let d = add(
        &mut editor,
        group,
        ContentsKind::Parametric(ShapeKind::Polygon),
    );
    let untouched = add(&mut editor, group, ContentsKind::Group(vec![]));
    add(
        &mut editor,
        untouched,
        ContentsKind::Parametric(ShapeKind::RoundedRectangle),
    );
    let p = ContentsParam::Width;
    *track_mut(&mut editor, b, p) = AnimatedProperty {
        value: 77.,
        keys: BTreeMap::from([(0, key(100.)), (20, key(300.))]),
    };
    let eased = Keyframe {
        value: 90.,
        interpolation: Interpolation::Bezier(Bezier::default()),
        temporal: TemporalHandles {
            mode: TemporalMode::Continuous,
            incoming: Some(TemporalHandle {
                slope: 3.25,
                influence: 0.25,
            }),
            outgoing: Some(TemporalHandle {
                slope: 3.25,
                influence: 0.75,
            }),
        },
    };
    *track_mut(&mut editor, c, p) = AnimatedProperty {
        value: 76.,
        keys: BTreeMap::from([(0, key(80.)), (10, eased.clone()), (20, key(100.))]),
    };
    *track_mut(&mut editor, d, p) = AnimatedProperty {
        value: 75.,
        keys: BTreeMap::from([(0, key(400.)), (20, key(600.))]),
    };
    contents_mut(&mut editor.current.project)
        .node_mut(a)
        .unwrap()
        .enabled = false;
    editor.current.project.version = PROJECT_VERSION;
    editor.project().validate().unwrap();
    editor.clear_history();
    let before = editor.current.clone();
    let mut expected = before.clone();
    let expected_contents = contents_mut(&mut expected.project);
    expected_contents.node_mut(a).unwrap().parameters.insert(
        p,
        AnimatedProperty {
            value: 500.,
            keys: BTreeMap::new(),
        },
    );
    expected_contents.node_mut(b).unwrap().parameters.insert(
        p,
        AnimatedProperty {
            value: 77.,
            keys: BTreeMap::from([(0, key(100.)), (10, key(500.)), (20, key(300.))]),
        },
    );
    expected_contents.node_mut(c).unwrap().parameters.insert(
        p,
        AnimatedProperty {
            value: 76.,
            keys: BTreeMap::from([
                (0, key(80.)),
                (
                    10,
                    Keyframe {
                        value: 500.,
                        ..eased
                    },
                ),
                (20, key(100.)),
            ]),
        },
    );
    editor
        .execute(shared(group, &[d, c, a, b], p, 10, 500.))
        .unwrap();
    assert_eq!(editor.current, expected);
    assert_eq!(editor.undo, vec![before.clone()]);
    assert!(editor.redo.is_empty());
    assert!(
        !contents(editor.project()).node(d).unwrap().parameters[&p]
            .keys
            .contains_key(&10)
    );
    editor.undo();
    assert_eq!(editor.current, before);
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(editor.current, expected);
    let view = br#"{"version":1,"graph":{"pins":["unchanged"]}}"#;
    let bytes = project_file::encode(editor.project(), Some(view)).unwrap();
    let decoded = project_file::decode(&bytes).unwrap();
    assert_eq!(decoded.project, expected.project);
    assert_eq!(decoded.view, Some(view.as_slice()));
    assert_eq!(
        project_file::encode(&decoded.project, decoded.view).unwrap(),
        bytes
    );
    assert_eq!(
        Project::from_json(&editor.project().to_json().unwrap()).unwrap(),
        expected.project
    );
}

#[test]
fn contents_bulk_exact_sample_noop_preserves_signed_zero_dormant_data_and_redo() {
    let (mut editor, a, b) = pair();
    let p = ContentsParam::Transform(Property::PositionX);
    track_mut(&mut editor, a, p).value = -0.;
    *track_mut(&mut editor, b, p) = AnimatedProperty {
        value: -32.,
        keys: BTreeMap::from([(0, key(-10.)), (20, key(10.))]),
    };
    with_redo(&mut editor);
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    editor.execute(shared(0, &[a, b], p, 10, 0.)).unwrap();
    assert_unchanged(&editor, &current, &undo, &redo);
    assert!(
        contents(editor.project()).node(a).unwrap().parameters[&p]
            .value
            .is_sign_negative()
    );
    assert_eq!(
        contents(editor.project()).node(b).unwrap().parameters[&p]
            .keys
            .len(),
        2
    );
    // A real change below UI rounding precision must not be treated as equal.
    editor.execute(shared(0, &[a, b], p, 10, 1e-12)).unwrap();
    assert_eq!(editor.undo.len(), undo.len() + 1);
    for id in [a, b] {
        assert_eq!(
            contents(editor.project()).node(id).unwrap().value_at(p, 10),
            1e-12
        );
    }
}

#[test]
fn contents_bulk_noop_uses_clamped_render_sample() {
    let (mut editor, a, b) = pair();
    let p = ContentsParam::Width;
    let handles = TemporalHandles {
        outgoing: Some(TemporalHandle {
            slope: 100000.,
            influence: 0.5,
        }),
        ..Default::default()
    };
    *track_mut(&mut editor, a, p) = AnimatedProperty {
        value: 432.,
        keys: BTreeMap::from([
            (
                0,
                Keyframe {
                    temporal: handles,
                    ..key(100.)
                },
            ),
            (20, key(100.)),
        ]),
    };
    track_mut(&mut editor, b, p).value = p.bounds().1;
    assert!(contents(editor.project()).node(a).unwrap().parameters[&p].value_at(10) > p.bounds().1);
    assert_eq!(
        contents(editor.project()).node(a).unwrap().value_at(p, 10),
        p.bounds().1
    );
    with_redo(&mut editor);
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    editor
        .execute(shared(0, &[a, b], p, 10, p.bounds().1))
        .unwrap();
    assert_unchanged(&editor, &current, &undo, &redo);
}

#[test]
fn contents_bulk_invalid_targets_values_locks_and_frames_reject_atomically() {
    let (mut editor, a, b) = pair();
    let group = add(&mut editor, 0, ContentsKind::Group(vec![]));
    let child = add(
        &mut editor,
        group,
        ContentsKind::Parametric(ShapeKind::Rectangle),
    );
    with_redo(&mut editor);
    for (parent, ids) in [
        (0, vec![]),
        (0, vec![a, a]),
        (0, vec![a, 999]),
        (0, vec![a, child]),
        (group, vec![a, child]),
        (a, vec![b]),
        (999, vec![a]),
    ] {
        assert!(
            contents(editor.project())
                .shared_parameters(parent, &ids)
                .is_err()
        );
        reject(
            &mut editor,
            shared(parent, &ids, ContentsParam::Width, 0, 100.),
        );
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0., 32768.000001] {
        reject(
            &mut editor,
            shared(0, &[a, b], ContentsParam::Width, 0, value),
        );
    }
    let duration = editor.project().composition.duration;
    for frame in [duration, u32::MAX] {
        reject(
            &mut editor,
            shared(0, &[a, b], ContentsParam::Width, frame, 100.),
        );
    }
    reject(
        &mut editor,
        shared(
            0,
            &[a, b],
            ContentsParam::Shape(ShapeParam::FillOpacity),
            0,
            100.,
        ),
    );
    reject(
        &mut editor,
        Command::Contents {
            id: 999,
            edit: ContentsEdit::SetSharedValue {
                parent: 0,
                items: vec![a, b],
                parameter: ContentsParam::Width,
                frame: 0,
                value: 100.,
            },
        },
    );
    editor.current.project.composition.layers[0].locked = true;
    reject(
        &mut editor,
        shared(0, &[a, b], ContentsParam::Width, 0, 100.),
    );
    editor.current.project.composition.layers[0].locked = false;
    editor.current.project.composition.layers[0].content = Content::Rectangle;
    reject(
        &mut editor,
        shared(0, &[a, b], ContentsParam::Width, 0, 100.),
    );
}

#[test]
fn contents_bulk_inclusive_bounds_and_singleton_targets_are_valid() {
    let (mut editor, a, b) = pair();
    for value in [0.001, 32768.] {
        editor
            .execute(shared(0, &[a, b], ContentsParam::Width, 0, value))
            .unwrap();
        assert_eq!(
            contents(editor.project())
                .node(a)
                .unwrap()
                .value_at(ContentsParam::Width, 0),
            value
        );
    }
    editor
        .execute(shared(0, &[a], ContentsParam::Width, 0, 17.))
        .unwrap();
    assert_eq!(
        contents(editor.project())
            .node(b)
            .unwrap()
            .value_at(ContentsParam::Width, 0),
        32768.
    );
}

#[test]
fn contents_bulk_cannot_repair_invalid_original_or_inactive_composition() {
    let (mut editor, a, b) = pair();
    with_redo(&mut editor);
    track_mut(&mut editor, a, ContentsParam::Width).value = -1.;
    reject(
        &mut editor,
        shared(0, &[a, b], ContentsParam::Width, 0, 100.),
    );
    reject(
        &mut editor,
        Command::Batch(vec![shared(0, &[a, b], ContentsParam::Width, 0, 100.)]),
    );
    track_mut(&mut editor, a, ContentsParam::Width).value = 100.;
    let mut inactive = editor.project().composition.clone();
    inactive.layers[0].id = 2;
    if let Content::ShapeContents(c) = &mut inactive.layers[0].content {
        c.node_mut(a)
            .unwrap()
            .parameters
            .remove(&ContentsParam::Height);
    }
    editor.current.project.next_layer_id = 3;
    editor.current.project.next_composition_id = 3;
    editor
        .current
        .project
        .other_compositions
        .insert(2, inactive);
    reject(
        &mut editor,
        shared(0, &[a, b], ContentsParam::Width, 0, 100.),
    );
}

#[test]
fn contents_bulk_key_limit_skips_equal_tracks_and_rejects_growth_as_one_transaction() {
    let (mut editor, a, b) = pair();
    editor.current.project.composition.duration = 20_001;
    editor.current.project.composition.layers[0].out_frame = Some(20_001);
    let p = ContentsParam::Width;
    *track_mut(&mut editor, b, p) = AnimatedProperty {
        value: 42.,
        keys: (0..10_000).map(|frame| (frame * 2, key(100.))).collect(),
    };
    with_redo(&mut editor);
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    editor.execute(shared(0, &[a, b], p, 1, 100.)).unwrap();
    assert_unchanged(&editor, &current, &undo, &redo);
    reject(&mut editor, shared(0, &[a, b], p, 1, 101.));
    editor.execute(shared(0, &[a, b], p, 2, 101.)).unwrap();
    assert_eq!(
        contents(editor.project()).node(b).unwrap().parameters[&p]
            .keys
            .len(),
        10_000
    );
    assert_eq!(
        contents(editor.project()).node(a).unwrap().value_at(p, 2),
        101.
    );
}

#[test]
fn contents_bulk_changed_and_unchanged_preserve_legacy_schema_assets_and_inactive_source() {
    for version in [43, 44, PROJECT_VERSION] {
        let (mut editor, a, b) = pair();
        let group = add(&mut editor, 0, ContentsKind::Group(vec![]));
        for png in ["YWJj", "ZGVm"] {
            editor
                .execute(Command::ImportAsset {
                    content: Content::Image { png: png.into() },
                    width: 64.,
                    height: 48.,
                    name: format!("Asset {png}"),
                    folder: None,
                    frame: None,
                })
                .unwrap();
        }
        with_redo(&mut editor);
        editor.current.project.version = version;
        assert_eq!(editor.project().asset_library().assets().len(), 2);
        if version == 43 {
            let node = contents_mut(&mut editor.current.project)
                .node_mut(group)
                .unwrap();
            node.parameters.remove(&ContentsParam::Skew);
            node.parameters.remove(&ContentsParam::SkewAxis);
        }
        let mut inactive = editor.project().composition.clone();
        inactive.layers[0].id = 2;
        editor.current.project.next_layer_id = 3;
        editor.current.project.next_composition_id = 3;
        editor
            .current
            .project
            .other_compositions
            .insert(2, inactive.clone());
        editor.project().validate().unwrap();
        let (current, undo, redo) = (
            editor.current.clone(),
            editor.undo.clone(),
            editor.redo.clone(),
        );
        editor
            .execute(shared(0, &[a, b], ContentsParam::Width, 0, 100.))
            .unwrap();
        assert_unchanged(&editor, &current, &undo, &redo);
        let mut expected = current.clone();
        for id in [a, b] {
            contents_mut(&mut expected.project)
                .node_mut(id)
                .unwrap()
                .parameters
                .get_mut(&ContentsParam::Width)
                .unwrap()
                .value = 101.;
        }
        editor
            .execute(shared(0, &[a, b], ContentsParam::Width, 0, 101.))
            .unwrap();
        assert_eq!(editor.current, expected);
        assert_eq!(editor.project().other_compositions[&2], inactive);
        let bytes = project_file::encode(editor.project(), None).unwrap();
        // Loading v43 has an established v44 skew migration. The value command
        // itself must not run it; compare codec output with that explicit oracle.
        let mut decoded_expected = expected.project.clone();
        if version == 43 {
            decoded_expected.version = 44;
            for composition in decoded_expected.compositions_mut() {
                let Content::ShapeContents(c) = &mut composition.layers[0].content else {
                    panic!()
                };
                let node = c.node_mut(group).unwrap();
                node.parameters
                    .insert(ContentsParam::Skew, AnimatedProperty::new(0.));
                node.parameters
                    .insert(ContentsParam::SkewAxis, AnimatedProperty::new(0.));
            }
        }
        assert_eq!(
            project_file::decode(&bytes).unwrap().project,
            decoded_expected
        );
        editor.undo();
        assert_eq!(editor.current, current);
        editor.redo();
        assert_eq!(editor.current, expected);
    }
}

#[test]
fn contents_bulk_nested_pure_exact_return_preserves_source_and_redo() {
    let (mut editor, a, b) = pair();
    with_redo(&mut editor);
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    let p = ContentsParam::Width;
    editor
        .execute(Command::Batch(vec![
            shared(0, &[a, b], p, 0, 200.),
            Command::Batch(vec![shared(0, &[b, a], p, 0, 100.)]),
        ]))
        .unwrap();
    assert_unchanged(&editor, &current, &undo, &redo);
    reject(
        &mut editor,
        Command::Batch(vec![
            shared(0, &[a, b], p, 0, 200.),
            shared(0, &[a, 999], p, 0, 100.),
        ]),
    );
}

#[test]
fn contents_bulk_pure_classifier_excludes_ordinary_mixed_and_empty_batches() {
    let command = shared(0, &[1, 2], ContentsParam::Width, 0, 100.);
    assert!(contents_bulk_fields::edits_only(&command));
    assert!(contents_bulk_fields::edits_only(&Command::Batch(vec![
        command.clone(),
        Command::Batch(vec![command.clone()])
    ])));
    let ordinary = Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item: 1,
            parameter: ContentsParam::Width,
            edit: TrackEdit::Value {
                frame: 0,
                value: 100.,
            },
        },
    };
    for other in [
        ordinary,
        Command::Batch(vec![]),
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Rename {
                item: 1,
                name: "Other".into(),
            },
        },
    ] {
        assert!(!contents_bulk_fields::edits_only(&other));
        assert!(!contents_bulk_fields::edits_only(&Command::Batch(vec![
            command.clone(),
            Command::Batch(vec![other])
        ])));
    }
}

#[test]
fn contents_bulk_mixed_and_empty_batches_keep_legacy_migration_behavior() {
    for include_empty in [false, true] {
        let (mut editor, a, b) = pair();
        editor.current.project.version = PROJECT_VERSION;
        let mut expected = Editor {
            current: editor.current.clone(),
            undo: editor.undo.clone(),
            redo: editor.redo.clone(),
            context_generation: editor.context_generation,
        };
        let other = if include_empty {
            Command::Batch(vec![])
        } else {
            Command::Contents {
                id: 1,
                edit: ContentsEdit::Rename {
                    item: a,
                    name: "Renamed".into(),
                },
            }
        };
        expected.execute(other.clone()).unwrap();
        editor
            .execute(Command::Batch(vec![
                shared(0, &[a, b], ContentsParam::Width, 0, 100.),
                other,
            ]))
            .unwrap();
        assert_unchanged(&editor, &expected.current, &expected.undo, &expected.redo);
        assert!(editor.project().version < PROJECT_VERSION);
    }
}

const METADATA_BUDGET: usize = 16 * 1024 * 1024;

fn budget_scene(spare: usize) -> (Editor, u64, u64) {
    let (mut editor, a, b) = pair();
    let mut text_editor = Editor::default();
    text_editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "\u{0001}".repeat(16384),
                font_size: 24.,
            },
            width: 10.,
            height: 10.,
            name: "Text".into(),
        })
        .unwrap();
    let template = text_editor.project().composition.layers[0].clone();
    for id in 2..=181 {
        let mut layer = template.clone();
        layer.id = id;
        editor.current.project.composition.layers.push(layer);
    }
    editor.current.project.next_layer_id = 182;
    editor.current.project.version = PROJECT_VERSION;
    let size = |project: &Project| serde_json::to_vec(project).unwrap().len();
    let mut remaining = (size(editor.project()) - (METADATA_BUDGET - spare)).div_ceil(5);
    for layer in &mut editor.current.project.composition.layers[1..] {
        let Content::Text { text, .. } = &mut layer.content else {
            panic!()
        };
        let count = remaining.min(text.len());
        *text = "x".repeat(count) + &"\u{0001}".repeat(text.len() - count);
        remaining -= count;
    }
    assert_eq!(remaining, 0);
    let padding = METADATA_BUDGET - spare - size(editor.project());
    assert!(padding < 5);
    editor
        .current
        .project
        .composition
        .name
        .push_str(&"x".repeat(padding));
    editor.project().validate().unwrap();
    document::validate_budget(editor.project()).unwrap();
    assert_eq!(size(editor.project()), METADATA_BUDGET - spare);
    // Seed independent valid snapshots without changing the precisely sized source.
    let mut prior = editor.current.clone();
    prior.project.composition.layers[0].name = "C".into();
    editor.undo = vec![prior.clone()];
    editor.redo = vec![prior];
    (editor, a, b)
}

#[test]
fn contents_bulk_metadata_boundary_rejects_growth_but_accepts_final_exact_return() {
    let (mut editor, a, b) = budget_scene(0);
    let p = ContentsParam::Width;
    let (current, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    reject(&mut editor, shared(0, &[a, b], p, 0, 1000.));
    editor
        .execute(Command::Batch(vec![
            shared(0, &[a, b], p, 0, 1000.),
            Command::Batch(vec![shared(0, &[a, b], p, 0, 100.)]),
        ]))
        .unwrap();
    assert_unchanged(&editor, &current, &undo, &redo);
    // 100.0 -> 1000.0 costs exactly one byte for each of the two static tracks.
    let (mut boundary, a, b) = budget_scene(2);
    boundary.execute(shared(0, &[a, b], p, 0, 1000.)).unwrap();
    assert_eq!(
        serde_json::to_vec(boundary.project()).unwrap().len(),
        METADATA_BUDGET
    );
    document::validate_budget(boundary.project()).unwrap();
    let bytes = project_file::encode(boundary.project(), None).unwrap();
    assert_eq!(
        project_file::decode(&bytes).unwrap().project,
        *boundary.project()
    );
}

#[test]
fn contents_bulk_oversized_original_is_rejected_even_when_assignment_would_shrink_it() {
    let (mut editor, a, b) = budget_scene(0);
    editor.current.project.composition.name.push('x');
    editor.project().validate().unwrap();
    for command in [
        shared(0, &[a, b], ContentsParam::Width, 0, 1.),
        shared(0, &[a, b], ContentsParam::Width, 0, 100.),
        Command::Batch(vec![shared(0, &[a, b], ContentsParam::Width, 0, 1.)]),
    ] {
        reject(&mut editor, command);
    }
}

#[test]
fn contents_bulk_group_values_are_local_and_leave_descendants_and_all_path_poses_unchanged() {
    let mut editor = scene();
    let a = add(&mut editor, 0, ContentsKind::Group(vec![]));
    let b = add(&mut editor, 0, ContentsKind::Parametric(ShapeKind::Ellipse));
    let path = VectorPath {
        vertices: vec![PathVertex::corner([3., 7.]), PathVertex::corner([15., 18.])],
        closed: false,
    };
    let mut pose = path.clone();
    pose.vertices[0].position = [11., -9.];
    let animation: PathAnimation = serde_json::from_value(serde_json::json!({
        "poses": [path, pose, path],
        "timing": { "value": 2., "keys": { "0": { "value": 0., "interpolation": "Hold" }, "20": { "value": 1., "interpolation": "Smooth" } } }
    })).unwrap();
    add(&mut editor, a, ContentsKind::Path { path, animation });
    track_mut(&mut editor, a, ContentsParam::Transform(Property::ScaleX)).value = -150.;
    track_mut(&mut editor, a, ContentsParam::Skew).value = 17.;
    let p = ContentsParam::Transform(Property::PositionX);
    let mut expected = editor.current.clone();
    for id in [a, b] {
        contents_mut(&mut expected.project)
            .node_mut(id)
            .unwrap()
            .parameters
            .get_mut(&p)
            .unwrap()
            .value = 23.;
    }
    let addresses = editor.project().composition.layer(1).unwrap().track_paths();
    editor.execute(shared(0, &[b, a], p, 12, 23.)).unwrap();
    assert_eq!(
        serde_json::to_vec(editor.project()).unwrap(),
        serde_json::to_vec(&expected.project).unwrap()
    );
    assert_eq!(
        editor.project().composition.layer(1).unwrap().track_paths(),
        addresses
    );
}

#[test]
fn contents_bulk_trim_numeric_channels_use_the_same_atomic_sample_contract() {
    let mut editor = scene();
    let a = add(&mut editor, 0, ContentsKind::TrimPaths);
    let b = add(&mut editor, 0, ContentsKind::TrimPaths);
    assert_eq!(
        contents(editor.project())
            .shared_parameters(0, &[a, b])
            .unwrap(),
        TrimParam::ALL.map(ContentsParam::Trim)
    );
    let p = ContentsParam::Trim(TrimParam::Offset);
    *track_mut(&mut editor, b, p) = AnimatedProperty {
        value: -720.,
        keys: BTreeMap::from([(0, key(0.)), (20, key(720.))]),
    };
    let untouched = track_mut(&mut editor, b, p).clone();
    editor.execute(shared(0, &[a, b], p, 10, 360.)).unwrap();
    assert_eq!(track_mut(&mut editor, b, p), &untouched);
    assert_eq!(track_mut(&mut editor, a, p).value, 360.);
    assert!(track_mut(&mut editor, a, p).keys.is_empty());
}

mod animation;
