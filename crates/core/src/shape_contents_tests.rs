use super::*;
fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Shape(Shape {
            kind: ShapeKind::Star,
            stroke_width: 12.,
            ..Default::default()
        }),
        width: 200.,
        height: 120.,
        name: "Contents".into(),
    })
    .unwrap();
    e
}
fn edit(e: &mut Editor, edit: ContentsEdit) {
    e.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn contents(e: &Editor) -> &ShapeContents {
    let Content::ShapeContents(c) = e.selected_layer().unwrap().content() else {
        panic!()
    };
    c
}
#[test]
fn contents_migration_preserves_tracks_identity_history_and_retiming() {
    let mut e = scene();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 0,
        end: 120,
    })
    .unwrap();
    for p in [
        ShapeParam::Points,
        ShapeParam::FillRed,
        ShapeParam::StrokeOpacity,
        ShapeParam::InnerRadius,
    ] {
        for edit in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value {
                frame: 60,
                value: 10.,
            },
        ] {
            e.execute(Command::EditShape {
                id: 1,
                parameter: p,
                edit,
            })
            .unwrap();
        }
    }
    let old = e.project().clone();
    edit(&mut e, ContentsEdit::Promote);
    let saved = e.project().clone();
    assert_eq!(saved.version, 43);
    let Content::Shape(old_shape) = old.composition().layer(1).unwrap().content() else {
        panic!()
    };
    for (p, item) in [
        (ShapeParam::Points, 2),
        (ShapeParam::InnerRadius, 2),
        (ShapeParam::FillRed, 4),
        (ShapeParam::StrokeOpacity, 3),
    ] {
        assert_eq!(
            contents(&e)
                .node(item)
                .unwrap()
                .parameters
                .get(&ContentsParam::Shape(p)),
            old_shape.parameters.get(&p)
        );
    }
    assert_eq!(
        Project::from_json(&saved.to_json().unwrap()).unwrap(),
        saved
    );
    let mut bad: serde_json::Value = serde_json::from_str(&saved.to_json().unwrap()).unwrap();
    bad["version"] = 42.into();
    assert!(Project::from_json(&bad.to_string()).is_err());
    e.undo();
    assert_eq!(e.project(), &old);
    e.redo();
    assert_eq!(e.project(), &saved);
    edit(&mut e, ContentsEdit::Duplicate(1));
    let c = contents(&e);
    assert_eq!(c.rows().len(), 8);
    assert_eq!(
        c.rows()
            .iter()
            .map(|(_, _, n)| n.id)
            .collect::<BTreeSet<_>>()
            .len(),
        8
    );
    let points = PropertyPath::Contents {
        item: 2,
        parameter: ContentsParam::Shape(ShapeParam::Points),
    };
    let copy = e.selected_layer().unwrap().copy_key(points, 60).unwrap();
    e.execute(Command::PasteKeys {
        keys: vec![copy],
        frame: 90,
        target: None,
    })
    .unwrap();
    assert_eq!(
        e.selected_layer().unwrap().track_value(points, 90),
        Some(10.)
    );
    e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
    assert!(
        e.selected_layer()
            .unwrap()
            .track(points)
            .unwrap()
            .keys()
            .contains_key(&70)
    );
    assert_eq!(
        e.selected_layer().unwrap().track_value(points, 40),
        Some(7.5)
    );
    e.undo();
    e.undo();
    e.undo();
    assert_eq!(e.project(), &saved);
}
#[test]
fn contents_reordering_reparenting_and_invalid_edits_are_atomic() {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Group(vec![]),
        },
    );
    edit(
        &mut e,
        ContentsEdit::Move {
            item: 2,
            parent: 5,
            index: 0,
        },
    );
    assert!(matches!(&contents(&e).node(5).unwrap().kind,ContentsKind::Group(v) if v[0].id==2));
    for bad in [
        ContentsEdit::Move {
            item: 1,
            parent: 5,
            index: 0,
        },
        ContentsEdit::Move {
            item: 2,
            parent: 2,
            index: 0,
        },
        ContentsEdit::Move {
            item: 2,
            parent: 1,
            index: 999,
        },
        ContentsEdit::Remove(999),
        ContentsEdit::Rename {
            item: 2,
            name: " ".into(),
        },
        ContentsEdit::Track {
            item: 4,
            parameter: ContentsParam::Width,
            edit: TrackEdit::Value {
                frame: 0,
                value: 123.,
            },
        },
        ContentsEdit::Track {
            item: 2,
            parameter: ContentsParam::Width,
            edit: TrackEdit::Value {
                frame: 0,
                value: f64::NAN,
            },
        },
    ] {
        let before = e.project().clone();
        assert!(e.execute(Command::Contents { id: 1, edit: bad }).is_err());
        assert_eq!(e.project(), &before);
    }
    let before = e.project().clone();
    edit(&mut e, ContentsEdit::Remove(5));
    assert!(contents(&e).node(2).is_none());
    e.undo();
    assert_eq!(e.project(), &before);
    e.current.project.composition.layers[0].locked = true;
    let before = e.project().clone();
    assert!(
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Remove(1)
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
}
#[test]
fn contents_paths_convert_animate_and_follow_group_transforms() {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    edit(&mut e, ContentsEdit::ConvertPath { item: 2, frame: 0 });
    for (p, v) in [
        (Property::PositionX, 40.),
        (Property::PositionY, 20.),
        (Property::ScaleX, 150.),
        (Property::Rotation, 30.),
    ] {
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 1,
                parameter: ContentsParam::Transform(p),
                edit: TrackEdit::Value { frame: 0, value: v },
            },
        );
    }
    let target = PathTarget::Contents(2);
    let original = e
        .selected_layer()
        .unwrap()
        .path_animation(target)
        .unwrap()
        .0
        .clone();
    e.execute(Command::AnimatePath {
        id: 1,
        target,
        edit: TrackEdit::ToggleAnimation { frame: 0 },
    })
    .unwrap();
    let mut moved = original.clone();
    moved.vertices[0].position[0] += 40.;
    e.execute(Command::EditPath {
        id: 1,
        target,
        frame: 60,
        path: moved,
    })
    .unwrap();
    let path = contents(&e).editable_paths(30);
    assert_eq!(path.len(), 1);
    assert_eq!(path[0].0, 2);
    assert_eq!(
        path[0].1.vertices[0].position[0],
        original.vertices[0].position[0] + 20.
    );
    let q = path[0].2.point([100., 0.]);
    assert!((q[0] - (40. + 150. * 30f64.to_radians().cos())).abs() < 1e-9);
    assert!((q[1] - 95.).abs() < 1e-9);
    let save = e.project().to_json().unwrap();
    assert_eq!(Project::from_json(&save).unwrap(), *e.project());
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: 1,
            enabled: false,
        },
    );
    assert!(contents(&e).editable_paths(30).is_empty());
    assert_eq!(contents(&e).svg_at(30), "");
}
