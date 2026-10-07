use super::*;
use GradientColorsInterpolation::{Hold, Linear, Smooth};

fn command(frames: &[Frame], to: Frame) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::GradientColors {
            item: 1,
            edit: GradientColorsEdit::ScaleKeys {
                frames: frames.iter().copied().collect(),
                to,
            },
        },
    }
}
fn edit(e: &mut Editor, edit: GradientColorsEdit) {
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::GradientColors { item: 1, edit },
    })
    .unwrap();
}
fn scene(frames: &[Frame]) -> Editor {
    scene_with_kind(
        frames,
        ContentsKind::GradientFill {
            even_odd: false,
            gradient: ShapeGradient::default(),
        },
    )
}
fn scene_with_kind(frames: &[Frame], kind: ContentsKind) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::ShapeContents(ShapeContents::default()),
        width: 200.,
        height: 100.,
        name: "Gradient time scaling".into(),
    })
    .unwrap();
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::Add { parent: 0, kind },
    })
    .unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 0,
        end: 100,
    })
    .unwrap();
    edit(
        &mut e,
        GradientColorsEdit::SetAnimation {
            frame: frames[0],
            enabled: true,
        },
    );
    for (index, &frame) in frames.iter().enumerate() {
        edit(
            &mut e,
            GradientColorsEdit::Color {
                frame,
                stop: 1,
                color: 0x123456 + index as u32,
            },
        );
    }
    e.clear_history();
    e
}
fn node(project: &Project) -> &ContentsNode {
    let Content::ShapeContents(contents) = &project.composition.layer(1).unwrap().content else {
        panic!()
    };
    contents.node(1).unwrap()
}
fn node_mut(project: &mut Project) -> &mut ContentsNode {
    let Content::ShapeContents(contents) = &mut project
        .composition
        .layers
        .iter_mut()
        .find(|layer| layer.id == 1)
        .unwrap()
        .content
    else {
        panic!()
    };
    contents.node_mut(1).unwrap()
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
fn reject(e: &mut Editor, command: Command) {
    let before = e.current.clone();
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    assert!(e.execute(command).is_err());
    assert_eq!(e.current, before);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
}
fn frames(e: &Editor) -> Vec<Frame> {
    animation(e.project()).keys().keys().copied().collect()
}

#[test]
fn gradient_colors_scale_sparse_selection_preserves_full_source_and_one_step_history() {
    let mut e = scene(&[10, 20, 30, 50, 90]);
    edit(
        &mut e,
        GradientColorsEdit::AddStop {
            frame: 30,
            opacity: false,
            position: 35.,
        },
    );
    let mut middle = animation(e.project()).keys()[&30].clone();
    middle.colors.reverse();
    middle.opacities.reverse();
    middle.colors[1].red = -0.;
    middle.opacities[1].position = -0.;
    edit(
        &mut e,
        GradientColorsEdit::Set {
            frame: 30,
            colors: middle,
        },
    );
    mode(&mut e, 10, Linear);
    mode(&mut e, 30, Smooth);
    mode(&mut e, 90, Smooth);
    e.clear_history();
    let before = e.current.clone();
    let mapping = animation(e.project())
        .scaled_key_frames(&BTreeSet::from([10, 30, 50]), 70, 150)
        .unwrap();
    assert_eq!(mapping, BTreeMap::from([(10, 10), (30, 40), (50, 70)]));
    let mut expected = before.clone();
    let a = node_mut(&mut expected.project)
        .kind
        .gradient_mut()
        .unwrap()
        .colors_animation
        .as_mut()
        .unwrap();
    let old_middle = a.keys.remove(&30).unwrap();
    let old_end = a.keys.remove(&50).unwrap();
    a.keys.insert(40, old_middle);
    a.keys.insert(70, old_end);
    a.outgoing_interpolation.remove(&30);
    a.outgoing_interpolation.insert(40, Smooth);
    e.execute(command(&[10, 30, 50], 70)).unwrap();
    assert_eq!(e.current, expected);
    assert_eq!(e.undo.len(), 1);
    assert_eq!(frames(&e), [10, 20, 40, 70, 90]);
    assert_eq!(animation(e.project()).interpolation(70), Some(Hold));
    assert_eq!(animation(e.project()).interpolation(90), Some(Smooth));
    let json = e.project().to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), expected.project);
    assert_eq!(Project::from_json(&json).unwrap().to_json().unwrap(), json);
    e.undo();
    assert_eq!(e.current, before);
    e.redo();
    assert_eq!(e.current, expected);
}

#[test]
fn gradient_colors_scale_selected_source_overlap_retains_sparse_and_dormant_modes() {
    let mut e = scene(&[10, 20, 30]);
    mode(&mut e, 10, Linear);
    mode(&mut e, 30, Smooth);
    let before = e.current.clone();
    e.execute(command(&[10, 20, 30], 50)).unwrap();
    assert_eq!(frames(&e), [10, 30, 50]);
    let old = animation(&before.project);
    let scaled = animation(e.project());
    for (from, to) in [(10, 10), (20, 30), (30, 50)] {
        assert_eq!(scaled.keys()[&to], old.keys()[&from]);
        assert_eq!(scaled.interpolation(to), old.interpolation(from));
    }
    assert_eq!(scaled.outgoing_interpolation.len(), 2);
    assert_eq!(scaled.interpolation(30), Some(Hold));
    assert_eq!(scaled.interpolation(50), Some(Smooth));
    e.execute(command(&[10, 30, 50], 30)).unwrap();
    assert_eq!(e.current, before);
}

#[test]
fn gradient_colors_scale_rounds_half_frames_later_with_exact_endpoints() {
    for (source, to, expected) in [
        (vec![10, 11, 13, 14], 16, vec![10, 12, 15, 16]),
        (vec![10, 12, 14], 13, vec![10, 12, 13]),
        (vec![10, 12, 18], 15, vec![10, 11, 15]),
        (vec![0, 2, 7], 12, vec![0, 3, 12]),
    ] {
        let mut e = scene(&source);
        let mapping = animation(e.project())
            .scaled_key_frames(&source.iter().copied().collect(), to, 150)
            .unwrap();
        assert_eq!(mapping.values().copied().collect::<Vec<_>>(), expected);
        e.execute(command(&source, to)).unwrap();
        assert_eq!(frames(&e), expected);
    }
}

#[test]
fn gradient_colors_scale_preview_uses_wide_integer_arithmetic_at_frame_limits() {
    let e = scene(&[10, 20]);
    let sample = animation(e.project()).keys()[&10].clone();
    for (source, to, expected) in [
        (
            vec![0, 2_147_483_647, u32::MAX - 1],
            u32::MAX - 2,
            vec![0, 2_147_483_647, u32::MAX - 2],
        ),
        (
            vec![u32::MAX - 11, u32::MAX - 9, u32::MAX - 7],
            u32::MAX - 1,
            vec![u32::MAX - 11, u32::MAX - 6, u32::MAX - 1],
        ),
    ] {
        let a = GradientColorsAnimation {
            keys: source.iter().map(|&f| (f, sample.clone())).collect(),
            outgoing_interpolation: BTreeMap::new(),
        };
        let selected = source.iter().copied().collect();
        assert_eq!(
            a.scaled_key_frames(&selected, to, u32::MAX)
                .unwrap()
                .values()
                .copied()
                .collect::<Vec<_>>(),
            expected
        );
        assert!(a.scaled_key_frames(&selected, u32::MAX, u32::MAX).is_err());
        assert!(a.scaled_key_frames(&selected, to, 0).is_err());
    }
}

#[test]
fn gradient_colors_scale_collisions_missing_selection_and_bounds_reject_atomically() {
    let mut e = scene(&[10, 11, 12, 20, 30, 40]);
    mode(&mut e, 10, Smooth);
    e.undo();
    assert!(e.can_redo());
    for (selected, to) in [
        (vec![], 50),
        (vec![10], 50),
        (vec![10, 99], 50),
        (vec![10, 20], 10),
        (vec![10, 20], 0),
        (vec![10, 20], 150),
        (vec![10, 20], u32::MAX),
        (vec![10, 11, 12], 11),
        (vec![10, 11, 20], 11),
        (vec![10, 20, 30], 40),
    ] {
        reject(&mut e, command(&selected, to));
    }
    assert!(
        animation(e.project())
            .scaled_key_frames(&BTreeSet::from([10, 20]), 19, 20)
            .is_err()
    );
    let mut detached = node(e.project()).clone();
    let original = detached.clone();
    assert!(
        super::edit(
            &mut detached,
            &GradientColorsEdit::ScaleKeys {
                frames: BTreeSet::from([10, 11, 20]),
                to: 11,
            },
            150,
        )
        .is_err()
    );
    assert_eq!(detached, original);
}

#[test]
fn gradient_colors_scale_composition_bounds_allow_end_beyond_layer_out_point() {
    let mut e = scene(&[10, 20]);
    e.execute(command(&[10, 20], 149)).unwrap();
    assert_eq!(frames(&e), [10, 149]);
    assert_eq!(
        e.project().composition.layer(1).unwrap().out_frame(150),
        100
    );
}

#[test]
fn gradient_colors_scale_identity_and_reversible_batch_preserve_redo_and_schema() {
    for version in [54, 57] {
        let mut e = scene(&[10, 20, 30]);
        e.current.project.version = version;
        mode(&mut e, 10, Smooth);
        e.undo();
        let before = e.current.clone();
        let source = e.project().to_json().unwrap();
        let undo = e.undo.clone();
        let redo = e.redo.clone();
        assert!(e.can_redo());
        e.execute(command(&[10, 20, 30], 30)).unwrap();
        e.execute(Command::Batch(vec![
            command(&[10, 20, 30], 50),
            command(&[10, 30, 50], 30),
        ]))
        .unwrap();
        assert_eq!(e.current, before);
        assert_eq!(e.project().to_json().unwrap(), source);
        assert_eq!(e.undo, undo);
        assert_eq!(e.redo, redo);
        e.execute(command(&[10, 20, 30], 50)).unwrap();
        assert_eq!(e.project().version, version);
        assert_eq!(e.undo.len(), undo.len() + 1);
        assert!(!e.can_redo());
    }
}

#[test]
fn gradient_colors_scale_requires_unlocked_valid_animated_paint_and_bounded_pure_batch() {
    let mut e = scene(&[10, 20]);
    e.execute(Command::ToggleLocked(1)).unwrap();
    reject(&mut e, command(&[10, 20], 30));
    e.undo();
    for cmd in [
        Command::Contents {
            id: 1,
            edit: ContentsEdit::GradientColors {
                item: 999,
                edit: GradientColorsEdit::ScaleKeys {
                    frames: BTreeSet::from([10, 20]),
                    to: 30,
                },
            },
        },
        Command::Batch(vec![command(&[10, 20], 30), Command::AddNull]),
        Command::Batch(vec![command(&[10, 20], 30), Command::Batch(vec![])]),
        Command::Batch(vec![command(&[10, 20], 30); 10000]),
    ] {
        reject(&mut e, cmd);
    }
    let mut deep = command(&[10, 20], 30);
    for _ in 0..65 {
        deep = Command::Batch(vec![deep]);
    }
    reject(&mut e, deep);
    e.current.project.version = 53;
    reject(&mut e, command(&[10, 20], 30));
    e.current.project.version = 54;
    edit(
        &mut e,
        GradientColorsEdit::SetAnimation {
            frame: 10,
            enabled: false,
        },
    );
    reject(&mut e, command(&[10, 20], 30));
}

#[test]
fn gradient_colors_scale_maximum_selection_preserves_key_and_stop_budgets() {
    let mut e = scene(&[0, 1]);
    e.current.project.composition.duration = 2000;
    let a = node_mut(&mut e.current.project)
        .kind
        .gradient_mut()
        .unwrap()
        .colors_animation
        .as_mut()
        .unwrap();
    let sample = a.keys[&0].clone();
    a.keys = (0..1000).map(|f| (f, sample.clone())).collect();
    let selected = (0..1000).collect::<Vec<_>>();
    e.current.project.validate().unwrap();
    e.clear_history();
    e.execute(command(&selected, 1998)).unwrap();
    assert_eq!(frames(&e), (0..1000).map(|f| f * 2).collect::<Vec<_>>());
    assert_eq!(e.undo.len(), 1);
    e.undo();
    let a = animation(e.project());
    let mut oversized = a.clone();
    oversized.keys.insert(1000, sample);
    assert!(
        oversized
            .scaled_key_frames(&(0..=1000).collect(), 1998, 2000)
            .is_err()
    );
}

#[test]
fn gradient_colors_scale_stroke_preserves_unrelated_assets_and_source_version() {
    let mut e = scene_with_kind(
        &[10, 20],
        ContentsKind::GradientStroke {
            style: ShapeStroke::default(),
            gradient: ShapeGradient::default(),
        },
    );
    e.execute(Command::AddContent {
        content: Content::Video {
            path: "unrelated.mov".into(),
            audio: None,
            duration: 5.,
            source_fps: 30.,
            start_frame: 0,
            playback: Default::default(),
        },
        width: 100.,
        height: 100.,
        name: "Unrelated footage".into(),
    })
    .unwrap();
    e.current.project.version = 54;
    e.current.project.validate().unwrap();
    e.clear_history();
    let before = e.current.clone();
    e.execute(command(&[10, 20], 30)).unwrap();
    let after = e.current.clone();
    let mut expected = before.clone();
    let a = node_mut(&mut expected.project)
        .kind
        .gradient_mut()
        .unwrap()
        .colors_animation
        .as_mut()
        .unwrap();
    let end = a.keys.remove(&20).unwrap();
    a.keys.insert(30, end);
    assert_eq!(after, expected);
    assert_eq!(after.project.version, 54);
    assert_eq!(after.project.asset_library.assets().len(), 1);
    assert_eq!(e.undo.len(), 1);
    e.undo();
    assert_eq!(e.current, before);
    e.redo();
    assert_eq!(e.current, after);
}

#[test]
fn gradient_colors_scale_preserves_scalar_curves_and_rolls_back_a_late_batch_collision() {
    let mut e = scene(&[10, 20, 30, 60]);
    e.execute(Command::Contents {
        id: 1,
        edit: ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::GradientStroke {
                style: ShapeStroke::default(),
                gradient: ShapeGradient::default(),
            },
        },
    })
    .unwrap();
    // An endpoint channel may coexist with compound Colors; a different paint
    // retains its legacy per-stop curve. Neither belongs to the scale operation.
    for (item, parameter) in [(1, GradientParam::EndX), (2, GradientParam::Red(1))] {
        for edit in [
            TrackEdit::ToggleAnimation { frame: 10 },
            TrackEdit::Value {
                frame: 30,
                value: 200.,
            },
            TrackEdit::Interpolate {
                frame: 10,
                interpolation: Interpolation::Bezier(Bezier::default()),
            },
        ] {
            e.execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Track {
                    item,
                    parameter: ContentsParam::Gradient(parameter),
                    edit,
                },
            })
            .unwrap();
        }
        e.execute(Command::SetTemporalHandle {
            id: 1,
            property: PropertyPath::Contents {
                item,
                parameter: ContentsParam::Gradient(parameter),
            },
            frame: 10,
            incoming: false,
            handle: TemporalHandle {
                slope: 2.,
                influence: 0.4,
            },
        })
        .unwrap();
    }
    e.clear_history();
    let before = e.current.clone();
    e.execute(command(&[10, 20, 30], 50)).unwrap();
    let mut expected = before.clone();
    let a = node_mut(&mut expected.project)
        .kind
        .gradient_mut()
        .unwrap()
        .colors_animation
        .as_mut()
        .unwrap();
    let middle = a.keys.remove(&20).unwrap();
    let last = a.keys.remove(&30).unwrap();
    a.keys.insert(30, middle);
    a.keys.insert(50, last);
    assert_eq!(e.current, expected);
    e.undo();
    assert_eq!(e.current, before);
    assert!(e.can_redo());
    reject(
        &mut e,
        Command::Batch(vec![command(&[10, 20, 30], 50), command(&[10, 30, 50], 60)]),
    );
    assert!(e.can_redo());
    e.redo();
    assert_eq!(e.current, expected);
}
