use super::*;

#[path = "trim_fixtures/legacy_svg_compare.rs"]
mod legacy_svg_compare;

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents::default()),
            width: 200.,
            height: 120.,
            name: "Trim fixtures".into(),
        })
        .unwrap();
    editor
}
fn edit(editor: &mut Editor, edit: ContentsEdit) {
    editor.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn contents(editor: &Editor) -> &ShapeContents {
    let Content::ShapeContents(contents) = &editor.project().composition.layers[0].content else {
        panic!()
    };
    contents
}
fn contents_mut(editor: &mut Editor) -> &mut ShapeContents {
    let Content::ShapeContents(contents) =
        &mut editor.current.project.composition.layers[0].content
    else {
        panic!()
    };
    contents
}
fn add(editor: &mut Editor, parent: u64, kind: ContentsKind) -> u64 {
    edit(editor, ContentsEdit::Add { parent, kind });
    contents(editor)
        .rows()
        .iter()
        .map(|(_, _, n)| n.id)
        .max()
        .unwrap()
}
fn line(x: f64, y: f64, length: f64) -> VectorPath {
    VectorPath {
        vertices: vec![
            PathVertex::corner([x, y]),
            PathVertex::corner([x + length, y]),
        ],
        closed: false,
    }
}
fn rectangle() -> VectorPath {
    VectorPath {
        vertices: [[0., 0.], [100., 0.], [100., 100.], [0., 100.]]
            .into_iter()
            .map(PathVertex::corner)
            .collect(),
        closed: true,
    }
}
fn source(editor: &mut Editor, parent: u64, path: VectorPath) -> u64 {
    add(
        editor,
        parent,
        ContentsKind::Path {
            path,
            animation: Default::default(),
        },
    )
}
fn stroke(editor: &mut Editor, parent: u64) -> u64 {
    add(editor, parent, ContentsKind::Stroke(Default::default()))
}
fn order(editor: &mut Editor, parent: u64, ids: &[u64]) {
    edit(
        editor,
        ContentsEdit::Reorder {
            parent,
            order: ids.to_vec(),
        },
    );
}
fn value(editor: &mut Editor, item: u64, parameter: ContentsParam, value: f64) {
    edit(
        editor,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value { frame: 0, value },
        },
    );
}
fn trim(editor: &mut Editor, parent: u64, start: f64, end: f64, offset: f64) -> u64 {
    let id = add(editor, parent, ContentsKind::TrimPaths);
    for (p, v) in [
        (TrimParam::Start, start),
        (TrimParam::End, end),
        (TrimParam::Offset, offset),
    ] {
        value(editor, id, ContentsParam::Trim(p), v);
    }
    id
}
fn paths(svg: &str) -> Vec<&str> {
    svg.split("<path d='")
        .skip(1)
        .map(|s| s.split('\'').next().unwrap())
        .collect()
}
fn run_endpoints(data: &str) -> Vec<([f64; 2], [f64; 2])> {
    data.split('M')
        .filter(|s| !s.is_empty())
        .map(|run| {
            let numbers: Vec<f64> = run
                .replace(['C', 'Z'], " ")
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            (
                [numbers[0], numbers[1]],
                [numbers[numbers.len() - 2], numbers[numbers.len() - 1]],
            )
        })
        .collect()
}
fn near(actual: [f64; 2], expected: [f64; 2]) {
    assert!(
        (actual[0] - expected[0]).hypot(actual[1] - expected[1]) <= 1. / 1024.,
        "{actual:?} != {expected:?}"
    );
}
fn track_command(item: u64, p: TrimParam, frame: Frame, value: f64) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item,
            parameter: ContentsParam::Trim(p),
            edit: TrackEdit::Value { frame, value },
        },
    }
}

#[test]
fn trim_defaults_stable_names_order_bounds_and_schema() {
    let mut e = scene();
    let id = add(&mut e, 0, ContentsKind::TrimPaths);
    let node = contents(&e).node(id).unwrap();
    assert_eq!(
        node.parameter_order(),
        TrimParam::ALL.map(ContentsParam::Trim)
    );
    for (p, expected) in [
        (TrimParam::Start, 0.),
        (TrimParam::End, 100.),
        (TrimParam::Offset, 0.),
    ] {
        assert_eq!(node.value_at(ContentsParam::Trim(p), 0), expected);
        let wire = format!("Trim.{}", p.name());
        assert_eq!(String::from(ContentsParam::Trim(p)), wire);
        assert_eq!(
            ContentsParam::try_from(wire).unwrap(),
            ContentsParam::Trim(p)
        );
    }
    assert_eq!(TrimParam::Start.bounds(), (0., 100.));
    assert_eq!(TrimParam::Offset.bounds(), (-1_000_000., 1_000_000.));
    assert!(ContentsParam::try_from("Trim.Unknown".to_owned()).is_err());
    assert_eq!(e.project().version, 50);
    contents(&e).validate(150).unwrap();
    let roundtrip = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    assert_eq!(roundtrip, *e.project());
    for version in [43, 47, 48, 49] {
        let mut invalid = roundtrip.clone();
        invalid.version = version;
        assert!(Project::from_json(&serde_json::to_string(&invalid).unwrap()).is_err());
    }
}

#[test]
fn trim_disabled_nested_inactive_nodes_still_require_schema_50() {
    let mut e = scene();
    let group = add(&mut e, 0, ContentsKind::Group(vec![]));
    let id = trim(&mut e, group, 0., 100., 720.);
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: id,
            enabled: false,
        },
    );
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: group,
            enabled: false,
        },
    );
    e.execute(Command::NewComposition).unwrap();
    assert_eq!(e.project().version, 50);
    assert!(e.project().composition.layers.is_empty());
    let mut invalid = e.project().clone();
    invalid.version = 49;
    assert!(invalid.validate().is_err());
    e.execute(Command::SetCompositionBackground(0x123456))
        .unwrap();
    assert_eq!(e.project().version, 50);
}

#[test]
fn trim_add_appends_only_to_immediate_group_and_is_one_undo() {
    let mut e = scene();
    let group = add(&mut e, 0, ContentsKind::Group(vec![]));
    let a = source(&mut e, group, line(0., 0., 100.));
    let s = stroke(&mut e, group);
    let before = e.current.clone();
    let depth = e.undo.len();
    let t = add(&mut e, group, ContentsKind::TrimPaths);
    let ContentsKind::Group(children) = &contents(&e).node(group).unwrap().kind else {
        panic!()
    };
    assert_eq!(children.iter().map(|n| n.id).collect::<Vec<_>>(), [a, s, t]);
    assert_eq!(e.undo.len(), depth + 1);
    e.undo();
    assert_eq!(e.current, before);
    e.redo();
    assert!(contents(&e).node(t).is_some());
    let b = source(&mut e, group, line(0., 10., 100.));
    let ContentsKind::Group(children) = &contents(&e).node(group).unwrap().kind else {
        panic!()
    };
    assert_eq!(children[0].id, b); // Legacy path insertion stays at the front.
    let root = trim(&mut e, 0, 0., 100., 0.);
    assert_eq!(
        contents(&e).items.iter().map(|n| n.id).collect::<Vec<_>>(),
        [group, root]
    );
}

#[test]
fn trim_duplicate_move_reorder_enable_delete_and_undo_preserve_ids_and_tracks() {
    let mut e = scene();
    let g = add(&mut e, 0, ContentsKind::Group(vec![]));
    let a = source(&mut e, g, line(0., 0., 100.));
    let t = trim(&mut e, g, 10., 80., -721.);
    let before = e.current.clone();
    edit(&mut e, ContentsEdit::Duplicate(t));
    let copy = contents(&e)
        .rows()
        .iter()
        .map(|(_, _, n)| n.id)
        .max()
        .unwrap();
    assert_ne!(copy, t);
    assert_eq!(
        contents(&e).node(copy).unwrap().parameters,
        contents(&e).node(t).unwrap().parameters
    );
    edit(
        &mut e,
        ContentsEdit::Move {
            item: copy,
            parent: 0,
            index: 1,
        },
    );
    order(&mut e, g, &[t, a]);
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: copy,
            enabled: false,
        },
    );
    edit(&mut e, ContentsEdit::Remove(copy));
    for _ in 0..5 {
        e.undo();
    }
    assert_eq!(e.current, before);
    for _ in 0..5 {
        e.redo();
    }
    assert!(contents(&e).node(copy).is_none());
}

#[test]
fn trim_invalid_track_type_paint_settings_missing_targets_and_batch_are_atomic() {
    let mut e = scene();
    let a = source(&mut e, 0, line(0., 0., 100.));
    let t = trim(&mut e, 0, 0., 100., 0.);
    let before = e.current.clone();
    let undo = e.undo.len();
    let mut invalid = vec![
        track_command(t, TrimParam::Start, 0, -0.1),
        track_command(t, TrimParam::End, 0, 100.1),
        track_command(t, TrimParam::Offset, 0, 1_000_001.),
        track_command(t, TrimParam::Start, 0, f64::NAN),
        track_command(t, TrimParam::End, 150, 100.),
        track_command(a, TrimParam::End, 0, 100.),
        track_command(999, TrimParam::End, 0, 100.),
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Blend {
                item: t,
                mode: PaintBlend::Multiply,
            },
        },
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Composite {
                item: t,
                mode: PaintComposite::AbovePrevious,
            },
        },
    ];
    invalid.push(Command::Batch(vec![
        track_command(t, TrimParam::End, 0, 50.),
        track_command(t, TrimParam::Start, 0, -1.),
    ]));
    for command in invalid {
        assert!(e.execute(command).is_err());
        assert_eq!(e.current, before);
        assert_eq!(e.undo.len(), undo);
    }
    e.current.project.composition.layers[0].locked = true;
    let locked = e.current.clone();
    assert!(
        e.execute(track_command(t, TrimParam::End, 0, 100.))
            .is_err()
    );
    assert_eq!(e.current, locked);
}

#[test]
fn trim_validation_rejects_missing_extra_and_foreign_parameters() {
    let mut e = scene();
    let t = trim(&mut e, 0, 0., 100., 0.);
    let original = contents(&e).clone();
    for mode in 0..3 {
        let mut bad = original.clone();
        let n = bad.node_mut(t).unwrap();
        match mode {
            0 => {
                n.parameters.remove(&ContentsParam::Trim(TrimParam::End));
            }
            1 => {
                n.parameters
                    .insert(ContentsParam::Width, AnimatedProperty::new(100.));
            }
            _ => {
                n.blend = PaintBlend::Screen;
            }
        }
        assert!(bad.validate(150).is_err());
    }
}

#[test]
fn trim_equal_value_keeps_interpolation_keys_metadata_and_both_history_stacks() {
    let mut e = scene();
    let t = trim(&mut e, 0, 0., 100., 0.);
    edit(
        &mut e,
        ContentsEdit::Track {
            item: t,
            parameter: ContentsParam::Trim(TrimParam::End),
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
    e.execute(track_command(t, TrimParam::End, 30, 0.)).unwrap();
    edit(
        &mut e,
        ContentsEdit::Track {
            item: t,
            parameter: ContentsParam::Trim(TrimParam::End),
            edit: TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Smooth,
            },
        },
    );
    e.execute(track_command(t, TrimParam::Offset, 0, 360.))
        .unwrap();
    e.undo();
    let before = e.current.clone();
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    let displayed = contents(&e)
        .node(t)
        .unwrap()
        .value_at(ContentsParam::Trim(TrimParam::End), 15);
    let command = || Command::EditTrack {
        id: 1,
        property: PropertyPath::Contents {
            item: t,
            parameter: ContentsParam::Trim(TrimParam::End),
        },
        edit: TrackEdit::Value {
            frame: 15,
            value: displayed,
        },
    };
    e.execute(track_command(t, TrimParam::End, 15, displayed))
        .unwrap();
    e.execute(command()).unwrap();
    e.execute(Command::Batch(vec![
        command(),
        Command::Batch(vec![command()]),
    ]))
    .unwrap();
    assert_eq!(e.current, before);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
    assert!(
        !contents(&e).node(t).unwrap().parameters[&ContentsParam::Trim(TrimParam::End)]
            .keys
            .contains_key(&15)
    );
    e.redo();
    assert_eq!(
        contents(&e)
            .node(t)
            .unwrap()
            .value_at(ContentsParam::Trim(TrimParam::Offset), 0),
        360.
    );
}

#[test]
fn trim_equal_clamped_value_is_noop_but_full_turn_and_explicit_key_are_edits() {
    let mut e = scene();
    let t = trim(&mut e, 0, 0., 100., 0.);
    let p = ContentsParam::Trim(TrimParam::End);
    let track = contents_mut(&mut e)
        .node_mut(t)
        .unwrap()
        .parameters
        .get_mut(&p)
        .unwrap();
    track.keys.insert(
        0,
        Keyframe {
            value: 0.,
            interpolation: Interpolation::Bezier(Bezier {
                x1: 0.2,
                y1: 3.,
                x2: 0.8,
                y2: 3.,
            }),
            temporal: Default::default(),
        },
    );
    track.keys.insert(
        30,
        Keyframe {
            value: 100.,
            interpolation: Interpolation::Linear,
            temporal: Default::default(),
        },
    );
    assert!(track.value_at(15) > 100.);
    let before = e.current.clone();
    let depth = e.undo.len();
    e.execute(track_command(t, TrimParam::End, 15, 100.))
        .unwrap();
    assert_eq!(e.current, before);
    assert_eq!(e.undo.len(), depth);
    e.execute(track_command(t, TrimParam::Offset, 0, 360.))
        .unwrap();
    assert_eq!(e.undo.len(), depth + 1);
    edit(
        &mut e,
        ContentsEdit::Track {
            item: t,
            parameter: p,
            edit: TrackEdit::ToggleKey { frame: 15 },
        },
    );
    assert!(
        contents(&e).node(t).unwrap().parameters[&p]
            .keys
            .contains_key(&15)
    );
}

#[test]
fn trim_paint_before_operator_observes_trimmed_geometry() {
    let mut e = scene();
    let a = source(&mut e, 0, line(0., 0., 100.));
    let s = stroke(&mut e, 0);
    let t = trim(&mut e, 0, 25., 75., 0.);
    order(&mut e, 0, &[a, s, t]);
    let before = contents(&e).clone();
    let svg = before.svg_at(0).unwrap();
    let runs = run_endpoints(paths(&svg)[0]);
    near(runs[0].0, [25., 0.]);
    near(runs[0].1, [75., 0.]);
    assert_eq!(contents(&e), &before);
}

#[test]
fn trim_paints_keep_their_original_paths_above_membership() {
    let mut e = scene();
    let a = source(&mut e, 0, line(0., 0., 100.));
    let s1 = stroke(&mut e, 0);
    let b = source(&mut e, 0, line(0., 10., 100.));
    let t = trim(&mut e, 0, 0., 50., 0.);
    let s2 = stroke(&mut e, 0);
    order(&mut e, 0, &[a, s1, b, t, s2]);
    let svg = contents(&e).svg_at(0).unwrap();
    let painted = paths(&svg);
    assert_eq!(painted.len(), 2);
    let later = run_endpoints(painted[0]);
    let earlier = run_endpoints(painted[1]);
    assert_eq!(later.len(), 2);
    assert_eq!(earlier.len(), 1);
    near(later[0].1, [50., 0.]);
    near(later[1].1, [50., 10.]);
    near(earlier[0].1, [50., 0.]);
}

#[test]
fn trim_later_source_is_untouched_and_two_operators_keep_original_scope() {
    let mut e = scene();
    let a = source(&mut e, 0, line(0., 0., 100.));
    let t1 = trim(&mut e, 0, 0., 50., 0.);
    let b = source(&mut e, 0, line(0., 10., 100.));
    let s = stroke(&mut e, 0);
    order(&mut e, 0, &[a, t1, b, s]);
    let svg = contents(&e).svg_at(0).unwrap();
    let runs = run_endpoints(paths(&svg)[0]);
    near(runs[0].1, [50., 0.]);
    near(runs[1].1, [100., 10.]);
    let t2 = trim(&mut e, 0, 0., 50., 0.);
    order(&mut e, 0, &[a, t1, b, t2, s]);
    let svg = contents(&e).svg_at(0).unwrap();
    let runs = run_endpoints(paths(&svg)[0]);
    near(runs[0].1, [25., 0.]);
    near(runs[1].1, [50., 10.]);
}

#[test]
fn trim_parent_changes_exports_but_never_retroactively_changes_child_paint() {
    let mut e = scene();
    let group = add(&mut e, 0, ContentsKind::Group(vec![]));
    let a = source(&mut e, group, line(0., 0., 100.));
    let child_stroke = stroke(&mut e, group);
    order(&mut e, group, &[a, child_stroke]);
    let t = trim(&mut e, 0, 0., 50., 0.);
    let parent_stroke = stroke(&mut e, 0);
    order(&mut e, 0, &[group, t, parent_stroke]);
    let svg = contents(&e).svg_at(0).unwrap();
    let painted = paths(&svg);
    near(run_endpoints(painted[0])[0].1, [50., 0.]);
    near(run_endpoints(painted[1])[0].1, [100., 0.]);
    let inner = trim(&mut e, group, 0., 50., 0.);
    order(&mut e, group, &[a, inner, child_stroke]);
    let svg = contents(&e).svg_at(0).unwrap();
    let painted = paths(&svg);
    near(run_endpoints(painted[0])[0].1, [25., 0.]);
    near(run_endpoints(painted[1])[0].1, [50., 0.]);
}

#[test]
fn trim_parent_measures_descendant_nonuniform_transform_in_its_own_local_space() {
    let mut e = scene();
    let g = add(&mut e, 0, ContentsKind::Group(vec![]));
    let a = source(&mut e, g, rectangle());
    value(&mut e, g, ContentsParam::Transform(Property::ScaleX), 200.);
    let t = trim(&mut e, 0, 0., 25., 0.);
    let s = stroke(&mut e, 0);
    order(&mut e, 0, &[g, t, s]);
    let svg = contents(&e).svg_at(0).unwrap();
    near(run_endpoints(paths(&svg)[0])[0].1, [150., 0.]);
    edit(
        &mut e,
        ContentsEdit::Move {
            item: t,
            parent: g,
            index: 1,
        },
    );
    order(&mut e, g, &[a, t]);
    let svg = contents(&e).svg_at(0).unwrap();
    near(run_endpoints(paths(&svg)[0])[0].1, [200., 0.]);
}

#[test]
fn trim_zero_opacity_group_exports_but_disabled_group_or_source_does_not() {
    let mut e = scene();
    let g = add(&mut e, 0, ContentsKind::Group(vec![]));
    let a = source(&mut e, g, line(0., 0., 100.));
    value(&mut e, g, ContentsParam::Transform(Property::Opacity), 0.);
    let t = trim(&mut e, 0, 0., 50., 0.);
    let s = stroke(&mut e, 0);
    order(&mut e, 0, &[g, t, s]);
    let svg = contents(&e).svg_at(0).unwrap();
    near(run_endpoints(paths(&svg)[0])[0].1, [50., 0.]);
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: a,
            enabled: false,
        },
    );
    assert_eq!(paths(&contents(&e).svg_at(0).unwrap())[0], "");
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: a,
            enabled: true,
        },
    );
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: g,
            enabled: false,
        },
    );
    assert_eq!(paths(&contents(&e).svg_at(0).unwrap())[0], "");
}

#[test]
fn trim_fullspan_disabled_and_reversed_fullspan_keep_exact_svg() {
    let mut e = scene();
    let g = add(&mut e, 0, ContentsKind::Group(vec![]));
    let a = source(&mut e, g, rectangle());
    let s = stroke(&mut e, g);
    order(&mut e, g, &[a, s]);
    value(&mut e, g, ContentsParam::Skew, 13.);
    value(&mut e, g, ContentsParam::Transform(Property::Rotation), 17.);
    let parent_stroke = stroke(&mut e, 0);
    order(&mut e, 0, &[g, parent_stroke]);
    let original = contents(&e)
        .svg_at_with_prefix(0, "<identity 'scope'>")
        .unwrap();
    let t = trim(&mut e, g, 0., 100., 1_000_000.);
    assert_eq!(
        contents(&e)
            .svg_at_with_prefix(0, "<identity 'scope'>")
            .unwrap(),
        original
    );
    value(&mut e, t, ContentsParam::Trim(TrimParam::Start), 100.);
    value(&mut e, t, ContentsParam::Trim(TrimParam::End), 0.);
    assert_eq!(
        contents(&e)
            .svg_at_with_prefix(0, "<identity 'scope'>")
            .unwrap(),
        original
    );
    value(&mut e, t, ContentsParam::Trim(TrimParam::End), 20.);
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: t,
            enabled: false,
        },
    );
    assert_eq!(
        contents(&e)
            .svg_at_with_prefix(0, "<identity 'scope'>")
            .unwrap(),
        original
    );
}

#[test]
fn trim_successive_operators_use_combined_surviving_length_without_gap_bridge() {
    let mut e = scene();
    let a = source(&mut e, 0, line(0., 0., 100.));
    let t1 = trim(&mut e, 0, 0., 50., 270.);
    let t2 = trim(&mut e, 0, 25., 75., 0.);
    let s = stroke(&mut e, 0);
    order(&mut e, 0, &[a, t1, t2, s]);
    let svg = contents(&e).svg_at(0).unwrap();
    let runs = run_endpoints(paths(&svg)[0]);
    assert_eq!(runs.len(), 2);
    near(runs[0].0, [87.5, 0.]);
    near(runs[0].1, [100., 0.]);
    near(runs[1].0, [0., 0.]);
    near(runs[1].1, [12.5, 0.]);
    let before = svg;
    let _ = trim(&mut e, 0, 0., 100., -720.);
    assert_eq!(contents(&e).svg_at(0).unwrap(), before);
}

#[test]
fn trim_empty_interval_and_empty_group_emit_no_fake_point() {
    let mut e = scene();
    let _ = add(&mut e, 0, ContentsKind::Group(vec![]));
    let a = source(&mut e, 0, line(0., 0., 100.));
    let s = stroke(&mut e, 0);
    let t = trim(&mut e, 0, 50., 50., 720.);
    let ids: Vec<_> = contents(&e).items.iter().map(|n| n.id).collect();
    assert!(ids.contains(&a) && ids.contains(&s) && ids.contains(&t));
    let svg = contents(&e).svg_at(0).unwrap();
    assert_eq!(paths(&svg), [""]);
    assert!(!svg.contains("M"));
}

#[test]
fn trim_current_frame_parametric_and_pose_sampling_precedes_operator() {
    let mut e = scene();
    let a = add(&mut e, 0, ContentsKind::Parametric(ShapeKind::Rectangle));
    edit(
        &mut e,
        ContentsEdit::Track {
            item: a,
            parameter: ContentsParam::Width,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
    edit(
        &mut e,
        ContentsEdit::Track {
            item: a,
            parameter: ContentsParam::Width,
            edit: TrackEdit::Value {
                frame: 30,
                value: 200.,
            },
        },
    );
    let t = trim(&mut e, 0, 0., 25., 0.);
    let s = stroke(&mut e, 0);
    order(&mut e, 0, &[a, t, s]);
    let before = contents(&e).clone();
    for f in [0, 15, 30] {
        let sampled = before.node(a).unwrap().path_at(f).unwrap();
        let svg = before.svg_at(f).unwrap();
        let start = run_endpoints(paths(&svg)[0])[0].0;
        near(start, sampled.vertices[0].position);
    }
    assert_ne!(before.svg_at(0).unwrap(), before.svg_at(30).unwrap());
    assert_eq!(contents(&e), &before);
    edit(&mut e, ContentsEdit::Remove(a));
    let p = source(&mut e, 0, line(0., 0., 100.));
    order(&mut e, 0, &[p, t, s]);
    e.execute(Command::AnimatePath {
        id: 1,
        target: PathTarget::Contents(p),
        edit: TrackEdit::ToggleAnimation { frame: 0 },
    })
    .unwrap();
    e.execute(Command::EditPath {
        id: 1,
        target: PathTarget::Contents(p),
        frame: 30,
        path: line(0., 0., 200.),
    })
    .unwrap();
    let before = contents(&e).clone();
    for (frame, end) in [(0, 25.), (15, 37.5), (30, 50.)] {
        let svg = before.svg_at(frame).unwrap();
        near(run_endpoints(paths(&svg)[0])[0].1, [end, 0.]);
    }
    assert_eq!(contents(&e), &before);
}

#[test]
fn trim_work_output_and_cancellation_errors_preserve_source_and_context() {
    let mut e = scene();
    let a = source(&mut e, 0, line(0., 0., 100.));
    let t = trim(&mut e, 0, 25., 75., 0.);
    let s = stroke(&mut e, 0);
    order(&mut e, 0, &[a, t, s]);
    let before = contents(&e).clone();
    let mut budget = ContentsRenderBudget {
        layer_work_limit: 0,
        ..Default::default()
    };
    let error = before
        .svg_at_with_budget(17, "limits", &mut budget, None)
        .unwrap_err();
    assert_eq!(error.kind, ContentsRenderErrorKind::WorkLimit);
    assert_eq!(error.frame, 17);
    assert_eq!(error.operator_id, Some(t));
    assert_eq!(error.source_id, Some(a));
    let mut budget = ContentsRenderBudget {
        output_byte_limit: 80,
        ..Default::default()
    };
    let error = before
        .svg_at_with_budget(17, "limits", &mut budget, None)
        .unwrap_err();
    assert_eq!(error.kind, ContentsRenderErrorKind::OutputLimit);
    let error = before
        .svg_at_with_budget(
            17,
            "limits",
            &mut ContentsRenderBudget::default(),
            Some(&|| true),
        )
        .unwrap_err();
    assert_eq!(error.kind, ContentsRenderErrorKind::Cancelled);
    assert_eq!(contents(&e), &before);
}

#[test]
fn trim_shared_frame_budget_survives_new_layer_and_default_fullspan_has_zero_work() {
    let mut e = scene();
    let a = source(&mut e, 0, line(0., 0., 100.));
    let t = trim(&mut e, 0, 25., 75., 0.);
    let s = stroke(&mut e, 0);
    order(&mut e, 0, &[a, t, s]);
    let mut budget = ContentsRenderBudget::default();
    contents(&e)
        .svg_at_with_budget(0, "frame", &mut budget, None)
        .unwrap();
    assert!(budget.frame_work > 0);
    let first_work = budget.frame_work;
    budget.frame_work_limit = first_work;
    let error = contents(&e)
        .svg_at_with_budget(0, "frame", &mut budget, None)
        .unwrap_err();
    assert_eq!(error.kind, ContentsRenderErrorKind::WorkLimit);
    value(&mut e, t, ContentsParam::Trim(TrimParam::Start), 0.);
    value(&mut e, t, ContentsParam::Trim(TrimParam::End), 100.);
    let mut budget = ContentsRenderBudget {
        layer_work_limit: 0,
        frame_work_limit: 0,
        ..Default::default()
    };
    contents(&e)
        .svg_at_with_budget(0, "frame", &mut budget, None)
        .unwrap();
    assert_eq!(budget.frame_work, 0);
}

#[test]
fn trim_output_limit_checks_final_strings_not_repeated_temporary_copies() {
    let mut e = scene();
    let group = add(&mut e, 0, ContentsKind::Group(vec![]));
    let a = source(&mut e, group, line(0., 0., 100.));
    let s = stroke(&mut e, group);
    order(&mut e, group, &[a, s]);
    let expected = contents(&e).svg_at_with_prefix(0, "x").unwrap();
    let mut budget = ContentsRenderBudget {
        output_byte_limit: expected.len(),
        ..Default::default()
    };
    assert_eq!(
        contents(&e)
            .svg_at_with_budget(0, "x", &mut budget, None)
            .unwrap(),
        expected
    );
    assert!(budget.output_bytes > budget.output_byte_limit);
    let mut budget = ContentsRenderBudget {
        output_byte_limit: expected.len() - 1,
        ..Default::default()
    };
    assert_eq!(
        contents(&e)
            .svg_at_with_budget(0, "x", &mut budget, None)
            .unwrap_err()
            .kind,
        ContentsRenderErrorKind::OutputLimit
    );
}

#[test]
fn trim_legacy_oracle_svg_and_exact_identity_across_primitives_paints_and_animations() {
    // Generated by the immutable pre-Trim library, never the implementation under test.
    // Provenance hashes and generator are preserved in the QA legacy fixture directory.
    let source: ShapeContents =
        serde_json::from_str(include_str!("trim_fixtures/legacy_contents.json")).unwrap();
    let records: serde_json::Value =
        serde_json::from_str(include_str!("trim_fixtures/legacy_svg.json")).unwrap();
    source.validate(1000).unwrap();
    let mut e = scene();
    e.execute(Command::SetContent {
        id: 1,
        content: Content::ShapeContents(source.clone()),
    })
    .unwrap();
    let references: Vec<_> = records
        .as_array()
        .unwrap()
        .iter()
        .map(|record| {
            let frame = record["frame"].as_u64().unwrap() as Frame;
            let svg = source.svg_at(frame).unwrap();
            let prefixed_svg = source
                .svg_at_with_prefix(frame, "trim-legacy-reference")
                .unwrap();
            // Keep the independent oracle. Only path coordinates may differ by
            // at most two ULPs between the original Linux and Windows runtimes.
            legacy_svg_compare::compare(&svg, record["svg"].as_str().unwrap())
                .unwrap_or_else(|error| panic!("legacy frame {frame}: {error}"));
            legacy_svg_compare::compare(&prefixed_svg, record["prefixed_svg"].as_str().unwrap())
                .unwrap_or_else(|error| panic!("prefixed legacy frame {frame}: {error}"));
            (frame, svg, prefixed_svg)
        })
        .collect();
    // Disabled, default, reversed full-span and whole-turn offsets cannot rewrite
    // any SVG number/command or transform arithmetic at any group boundary.
    let groups: Vec<_> = source
        .rows()
        .iter()
        .filter_map(|(_, _, n)| matches!(n.kind, ContentsKind::Group(_)).then_some(n.id))
        .chain([0])
        .collect();
    let mut operators = Vec::new();
    for group in groups {
        operators.push(trim(&mut e, group, 0., 100., 720.));
    }
    for pass in 0..3 {
        for &operator in &operators {
            if pass == 1 {
                value(
                    &mut e,
                    operator,
                    ContentsParam::Trim(TrimParam::Start),
                    100.,
                );
                value(&mut e, operator, ContentsParam::Trim(TrimParam::End), 0.);
            } else if pass == 2 {
                value(&mut e, operator, ContentsParam::Trim(TrimParam::End), 37.);
                edit(
                    &mut e,
                    ContentsEdit::Enabled {
                        item: operator,
                        enabled: false,
                    },
                );
            }
        }
        // Identity is a separate, stricter contract: no coordinate tolerance or
        // normalization is allowed against the unmodified same-runtime source.
        for (frame, svg, prefixed_svg) in &references {
            assert_eq!(
                contents(&e).svg_at(*frame).unwrap(),
                *svg,
                "identity pass {pass}, frame {frame}"
            );
            assert_eq!(
                contents(&e)
                    .svg_at_with_prefix(*frame, "trim-legacy-reference")
                    .unwrap(),
                *prefixed_svg,
                "prefixed identity pass {pass}, frame {frame}"
            );
        }
    }
    assert_eq!(
        source,
        serde_json::from_str(include_str!("trim_fixtures/legacy_contents.json")).unwrap()
    );
}

#[test]
fn trim_wrapped_closed_seam_is_continuous_and_partial_fill_does_not_close_stroke() {
    let mut e = scene();
    let a = source(&mut e, 0, rectangle());
    let f = add(&mut e, 0, ContentsKind::Fill { even_odd: true });
    let s = stroke(&mut e, 0);
    let t = trim(&mut e, 0, 0., 50., 270.);
    order(&mut e, 0, &[a, f, s, t]);
    let svg = contents(&e).svg_at(0).unwrap();
    let painted = paths(&svg);
    assert_eq!(painted.len(), 2);
    assert_eq!(painted[0], painted[1]);
    assert_eq!(painted[0].matches('M').count(), 1);
    assert!(!painted[0].contains('Z'));
    let runs = run_endpoints(painted[0]);
    near(runs[0].0, [0., 100.]);
    near(runs[0].1, [100., 0.]);
    assert!(svg.contains("fill-rule='evenodd'"));
    assert!(svg.contains("fill='none'"));
}

#[test]
fn trim_all_animated_primitives_equal_trim_of_the_current_sampled_source_path() {
    for kind in [
        ShapeKind::Rectangle,
        ShapeKind::RoundedRectangle,
        ShapeKind::Ellipse,
        ShapeKind::Polygon,
        ShapeKind::Star,
    ] {
        let mut e = scene();
        let a = add(&mut e, 0, ContentsKind::Parametric(kind));
        for (parameter, end) in [
            (ContentsParam::Width, 173.),
            (ContentsParam::Height, 91.),
            (ContentsParam::Shape(ShapeParam::Roundness), 31.),
            (ContentsParam::Shape(ShapeParam::Points), 9.),
            (ContentsParam::Shape(ShapeParam::InnerRadius), 68.),
        ] {
            if contents(&e)
                .node(a)
                .unwrap()
                .parameters
                .contains_key(&parameter)
            {
                edit(
                    &mut e,
                    ContentsEdit::Track {
                        item: a,
                        parameter,
                        edit: TrackEdit::ToggleAnimation { frame: 0 },
                    },
                );
                edit(
                    &mut e,
                    ContentsEdit::Track {
                        item: a,
                        parameter,
                        edit: TrackEdit::Value {
                            frame: 60,
                            value: end,
                        },
                    },
                );
            }
        }
        let s = stroke(&mut e, 0);
        let t = trim(&mut e, 0, 12., 79., 38.);
        order(&mut e, 0, &[a, s, t]);
        let original = contents(&e).clone();
        for frame in [0, 15, 30, 45, 60] {
            let path = original.node(a).unwrap().path_at(frame).unwrap();
            let mut materialized = original.clone();
            let node = materialized.node_mut(a).unwrap();
            node.kind = ContentsKind::Path {
                path,
                animation: Default::default(),
            };
            node.parameters.clear();
            assert_eq!(
                original.svg_at(frame).unwrap(),
                materialized.svg_at(frame).unwrap(),
                "{kind:?}, frame {frame}"
            );
        }
        assert_eq!(contents(&e), &original);
    }
}
