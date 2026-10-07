use super::*;
use ContentsAnimationAction::{AddKey, Disable, Enable, RemoveKey};

fn animation(
    parent: u64,
    items: &[u64],
    parameter: ContentsParam,
    frame: Frame,
    action: ContentsAnimationAction,
) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::SharedAnimation {
            parent,
            items: items.to_vec(),
            parameter,
            frame,
            action,
        },
    }
}

fn mixed() -> (Editor, [u64; 3]) {
    let (mut editor, a, b) = pair();
    let c = add(&mut editor, 0, ContentsKind::Parametric(ShapeKind::Star));
    *track_mut(&mut editor, b, ContentsParam::Width) = AnimatedProperty {
        value: 77.,
        keys: BTreeMap::from([(0, key(100.)), (20, key(300.))]),
    };
    let mut eased = key(90.);
    eased.interpolation = Interpolation::Bezier(Bezier::default());
    eased.temporal = TemporalHandles {
        mode: TemporalMode::Continuous,
        incoming: Some(TemporalHandle {
            slope: 3.25,
            influence: 0.25,
        }),
        outgoing: Some(TemporalHandle {
            slope: 3.25,
            influence: 0.75,
        }),
    };
    *track_mut(&mut editor, c, ContentsParam::Width) = AnimatedProperty {
        value: 76.,
        keys: BTreeMap::from([(0, key(80.)), (10, eased), (20, key(100.))]),
    };
    editor.clear_history();
    editor.project().validate().unwrap();
    (editor, [a, b, c])
}

#[test]
fn shared_animation_explicit_actions_have_literal_independent_tracks_and_one_undo() {
    for action in [Enable, Disable, AddKey, RemoveKey] {
        let (mut editor, [a, b, c]) = mixed();
        let p = ContentsParam::Width;
        let before = editor.current.clone();
        let mut expected = before.clone();
        let contents = contents_mut(&mut expected.project);
        match action {
            Enable => {
                contents
                    .node_mut(a)
                    .unwrap()
                    .parameters
                    .get_mut(&p)
                    .unwrap()
                    .keys
                    .insert(10, key(100.));
            }
            Disable => {
                contents
                    .node_mut(b)
                    .unwrap()
                    .parameters
                    .insert(p, AnimatedProperty::new(200.));
                contents
                    .node_mut(c)
                    .unwrap()
                    .parameters
                    .insert(p, AnimatedProperty::new(90.));
            }
            AddKey => {
                contents
                    .node_mut(a)
                    .unwrap()
                    .parameters
                    .get_mut(&p)
                    .unwrap()
                    .keys
                    .insert(10, key(100.));
                contents
                    .node_mut(b)
                    .unwrap()
                    .parameters
                    .get_mut(&p)
                    .unwrap()
                    .keys
                    .insert(10, key(200.));
            }
            RemoveKey => {
                contents
                    .node_mut(c)
                    .unwrap()
                    .parameters
                    .get_mut(&p)
                    .unwrap()
                    .keys
                    .remove(&10);
            }
        }
        editor
            .execute(animation(0, &[c, a, b], p, 10, action))
            .unwrap();
        assert_eq!(editor.current, expected, "{action:?}");
        assert_eq!(editor.undo, [before.clone()]);
        assert!(!editor.can_redo());
        editor.undo();
        assert_eq!(editor.current, before);
        assert!(!editor.can_undo());
        editor.redo();
        assert_eq!(editor.current, expected);
        let encoded = project_file::encode(editor.project(), Some(br#"{"version":1}"#)).unwrap();
        assert_eq!(
            project_file::decode(&encoded).unwrap().project,
            expected.project
        );
    }
}

#[test]
fn shared_animation_noops_preserve_complete_source_signed_zero_and_redo() {
    for action in [Enable, Disable, AddKey, RemoveKey] {
        let (mut editor, a, b) = pair();
        let p = ContentsParam::Transform(Property::PositionX);
        for item in [a, b] {
            *track_mut(&mut editor, item, p) = AnimatedProperty {
                value: -0.,
                keys: if matches!(action, Enable | AddKey) {
                    BTreeMap::from([(10, key(12.))])
                } else {
                    BTreeMap::new()
                },
            };
        }
        with_redo(&mut editor);
        let (before, undo, redo) = (
            editor.current.clone(),
            editor.undo.clone(),
            editor.redo.clone(),
        );
        editor
            .execute(animation(0, &[a, b], p, 10, action))
            .unwrap();
        assert_unchanged(&editor, &before, &undo, &redo);
    }
}

#[test]
fn shared_animation_last_key_removal_matches_singleton_and_preserves_other_bases() {
    let (mut editor, [a, b, c]) = mixed();
    let p = ContentsParam::Width;
    *track_mut(&mut editor, a, p) = AnimatedProperty {
        value: 432.,
        keys: BTreeMap::from([(10, key(123.))]),
    };
    let before = editor.current.clone();
    let mut singleton = Editor {
        current: before,
        undo: vec![],
        redo: vec![],
        context_generation: 0,
    };
    singleton
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: a,
                parameter: p,
                edit: TrackEdit::ToggleKey { frame: 10 },
            },
        })
        .unwrap();
    editor
        .execute(animation(0, &[a, b, c], p, 10, RemoveKey))
        .unwrap();
    assert_eq!(
        contents(editor.project()).node(a).unwrap().parameters[&p],
        contents(singleton.project()).node(a).unwrap().parameters[&p]
    );
    assert_eq!(
        contents(editor.project()).node(a).unwrap().parameters[&p],
        AnimatedProperty::new(123.)
    );
    assert_eq!(
        contents(editor.project()).node(b).unwrap().parameters[&p].value,
        77.
    );
    assert_eq!(
        contents(editor.project()).node(c).unwrap().parameters[&p].value,
        76.
    );
}

#[test]
fn shared_animation_insert_and_disable_use_clamped_samples_without_rewriting_other_tracks() {
    for action in [AddKey, Disable] {
        let (mut editor, a, b) = pair();
        let p = ContentsParam::Width;
        *track_mut(&mut editor, a, p) = AnimatedProperty {
            value: 432.,
            keys: BTreeMap::from([
                (
                    0,
                    Keyframe {
                        temporal: TemporalHandles {
                            outgoing: Some(TemporalHandle {
                                slope: 100000.,
                                influence: 0.5,
                            }),
                            ..Default::default()
                        },
                        ..key(100.)
                    },
                ),
                (20, key(100.)),
            ]),
        };
        track_mut(&mut editor, b, p).value = 37.;
        assert!(
            contents(editor.project()).node(a).unwrap().parameters[&p].value_at(10) > p.bounds().1
        );
        editor
            .execute(animation(0, &[a, b], p, 10, action))
            .unwrap();
        let first = &contents(editor.project()).node(a).unwrap().parameters[&p];
        if action == AddKey {
            assert_eq!(first.keys[&10], key(p.bounds().1));
            assert_eq!(first.value, 432.);
            assert_eq!(
                contents(editor.project()).node(b).unwrap().parameters[&p].keys[&10],
                key(37.)
            );
        } else {
            assert_eq!(*first, AnimatedProperty::new(p.bounds().1));
            assert_eq!(
                contents(editor.project()).node(b).unwrap().parameters[&p],
                AnimatedProperty::new(37.)
            );
        }
    }
}

#[test]
fn shared_animation_validates_noop_targets_frames_types_parents_and_source_before_skipping() {
    let (mut editor, a, b) = pair();
    let group = add(&mut editor, 0, ContentsKind::Group(vec![]));
    let child = add(
        &mut editor,
        group,
        ContentsKind::Parametric(ShapeKind::Rectangle),
    );
    with_redo(&mut editor);
    let p = ContentsParam::Width;
    for action in [Enable, Disable, AddKey, RemoveKey] {
        for (parent, items) in [
            (0, vec![]),
            (0, vec![a, a]),
            (0, vec![a, 999]),
            (0, vec![a, child]),
            (group, vec![a, child]),
            (a, vec![b]),
            (999, vec![a]),
        ] {
            reject(&mut editor, animation(parent, &items, p, 0, action));
        }
        for frame in [editor.project().composition.duration, u32::MAX] {
            reject(&mut editor, animation(0, &[a, b], p, frame, action));
        }
        reject(
            &mut editor,
            animation(
                0,
                &[a, b],
                ContentsParam::Shape(ShapeParam::FillOpacity),
                0,
                action,
            ),
        );
    }
    editor.current.project.composition.layers[0].locked = true;
    reject(&mut editor, animation(0, &[a, b], p, 0, Disable));
    editor.current.project.composition.layers[0].locked = false;
    track_mut(&mut editor, a, p).value = -1.;
    reject(&mut editor, animation(0, &[a, b], p, 0, Disable));
    reject(&mut editor, animation(0, &[a, b], p, 0, Enable));
    track_mut(&mut editor, a, p).value = 100.;
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
    reject(&mut editor, animation(0, &[a, b], p, 0, Disable));
}

#[test]
fn shared_animation_gradient_stop_ids_and_irrelevant_highlights_remain_excluded() {
    let mut editor = scene();
    let a = add(
        &mut editor,
        0,
        ContentsKind::GradientFill {
            gradient: ShapeGradient::default(),
            even_odd: false,
        },
    );
    let b = add(
        &mut editor,
        0,
        ContentsKind::GradientFill {
            gradient: ShapeGradient::default(),
            even_odd: false,
        },
    );
    let stop = contents(editor.project())
        .node(a)
        .unwrap()
        .parameter_order()
        .into_iter()
        .find(|p| matches!(p,ContentsParam::Gradient(g) if g.stop().is_some()))
        .unwrap();
    for action in [Enable, Disable, AddKey, RemoveKey] {
        reject(&mut editor, animation(0, &[a, b], stop, 0, action));
        reject(
            &mut editor,
            animation(
                0,
                &[a, b],
                ContentsParam::Gradient(GradientParam::HighlightAngle),
                0,
                action,
            ),
        );
    }
}

#[test]
fn shared_animation_key_growth_is_atomic_and_existing_keys_are_exact_noops_at_limit() {
    let (mut editor, a, b) = pair();
    editor.current.project.composition.duration = 20_001;
    editor.current.project.composition.layers[0].out_frame = Some(20_001);
    let p = ContentsParam::Width;
    for item in [a, b] {
        *track_mut(&mut editor, item, p) = AnimatedProperty {
            value: 42.,
            keys: (0..10_000).map(|frame| (frame * 2, key(100.))).collect(),
        };
    }
    with_redo(&mut editor);
    let (before, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    editor.execute(animation(0, &[a, b], p, 2, AddKey)).unwrap();
    editor.execute(animation(0, &[a, b], p, 1, Enable)).unwrap();
    assert_unchanged(&editor, &before, &undo, &redo);
    reject(&mut editor, animation(0, &[a, b], p, 1, AddKey));
    // Even when a static member is planned first, overflow rolls back both.
    *track_mut(&mut editor, a, p) = AnimatedProperty::new(37.);
    reject(&mut editor, animation(0, &[a, b], p, 1, AddKey));
}

#[test]
fn shared_animation_schema_assets_and_inactive_sources_survive_changed_and_noop_actions() {
    for version in [43, 44, PROJECT_VERSION] {
        let (mut editor, a, b) = pair();
        let group = add(&mut editor, 0, ContentsKind::Group(vec![]));
        editor
            .execute(Command::ImportAsset {
                content: Content::Image { png: "YWJj".into() },
                width: 64.,
                height: 48.,
                name: "Asset".into(),
                folder: None,
                frame: None,
            })
            .unwrap();
        with_redo(&mut editor);
        editor.current.project.version = version;
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
            .insert(2, inactive);
        editor.project().validate().unwrap();
        let before = editor.current.clone();
        let mut expected = before.clone();
        for item in [a, b] {
            contents_mut(&mut expected.project)
                .node_mut(item)
                .unwrap()
                .parameters
                .get_mut(&ContentsParam::Width)
                .unwrap()
                .keys
                .insert(10, key(100.));
        }
        editor
            .execute(animation(0, &[a, b], ContentsParam::Width, 10, Enable))
            .unwrap();
        assert_eq!(editor.current, expected);
        assert_eq!(editor.project().version, version);
        editor.undo();
        assert_eq!(editor.current, before);
        let (undo, redo) = (editor.undo.clone(), editor.redo.clone());
        editor
            .execute(animation(0, &[a, b], ContentsParam::Width, 10, Disable))
            .unwrap();
        assert_unchanged(&editor, &before, &undo, &redo);
    }
}

#[test]
fn shared_animation_metadata_budget_rejects_growth_and_validates_original_before_shrink() {
    let (mut editor, a, b) = budget_scene(0);
    let p = ContentsParam::Width;
    reject(&mut editor, animation(0, &[a, b], p, 10, Enable));
    reject(&mut editor, animation(0, &[a, b], p, 10, AddKey));
    let (before, undo, redo) = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    editor
        .execute(Command::Batch(vec![
            animation(0, &[a, b], p, 10, Enable),
            Command::Batch(vec![animation(0, &[a, b], p, 10, Disable)]),
        ]))
        .unwrap();
    assert_unchanged(&editor, &before, &undo, &redo);
    editor.current.project.composition.name.push('x');
    reject(&mut editor, animation(0, &[a, b], p, 10, Disable));
    reject(
        &mut editor,
        Command::Batch(vec![animation(0, &[a, b], p, 10, RemoveKey)]),
    );
}

#[test]
fn shared_animation_pure_classifier_keeps_mixed_and_empty_batches_on_legacy_route() {
    let command = animation(0, &[1, 2], ContentsParam::Width, 0, Enable);
    assert!(contents_bulk_fields::edits_only(&command));
    assert!(contents_bulk_fields::edits_only(&Command::Batch(vec![
        command.clone(),
        shared(0, &[1, 2], ContentsParam::Width, 0, 100.)
    ])));
    assert!(!contents_bulk_fields::edits_only(&Command::Batch(vec![
        command.clone(),
        Command::Batch(vec![])
    ])));
    assert!(!contents_bulk_fields::edits_only(&Command::Batch(vec![
        command,
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Rename {
                item: 1,
                name: "Name".into()
            }
        }
    ])));
}
