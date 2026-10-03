use super::*;
#[path = "contents_reorder_tests.rs"]
mod reorder;
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

fn contents_value(e: &mut Editor, item: u64, parameter: ContentsParam, value: f64) {
    edit(
        e,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value { frame: 0, value },
        },
    );
}

#[test]
fn paint_blend_modes_roundtrip_and_reject_stale_versions_or_invalid_targets() {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    let old = e.project().clone();
    assert!(!old.to_json().unwrap().contains("\"blend\""));
    for mode in PaintBlend::ALL {
        edit(&mut e, ContentsEdit::Blend { item: 4, mode });
        assert_eq!(contents(&e).node(4).unwrap().blend, mode);
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
    }
    assert_eq!(e.project().version, 47);
    let saved = e.project().clone();
    e.undo();
    e.redo();
    assert_eq!(e.project(), &saved);
    edit(&mut e, ContentsEdit::Duplicate(4));
    assert_eq!(contents(&e).node(5).unwrap().blend, PaintBlend::Luminosity);
    e.undo();
    for item in [1, 2, 999] {
        assert!(
            e.execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Blend {
                    item,
                    mode: PaintBlend::Multiply
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &saved);
    }
    let mut invalid = saved.clone();
    invalid.version = 46;
    assert!(Project::from_json(&serde_json::to_string(&invalid).unwrap()).is_err());
    invalid = saved.clone();
    let Content::ShapeContents(c) = &mut invalid.composition.layers[0].content else {
        panic!()
    };
    c.node_mut(2).unwrap().blend = PaintBlend::Screen;
    assert!(invalid.validate().is_err());
    e.current.project.composition.layers[0].locked = true;
    let locked = e.project().clone();
    assert!(
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Blend {
                item: 4,
                mode: PaintBlend::Normal
            }
        })
        .is_err()
    );
    assert_eq!(e.project(), &locked);
}

#[test]
fn paint_composite_preserves_order_tracks_history_and_old_documents() {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    edit(
        &mut e,
        ContentsEdit::Track {
            item: 4,
            parameter: ContentsParam::Shape(ShapeParam::FillOpacity),
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
    let old = e.project().clone();
    let json = old.to_json().unwrap();
    assert!(!json.contains("\"composite\""));
    assert_eq!(Project::from_json(&json).unwrap(), old);
    edit(
        &mut e,
        ContentsEdit::Composite {
            item: 4,
            mode: PaintComposite::AbovePrevious,
        },
    );
    let saved = e.project().clone();
    assert_eq!(saved.version, 46);
    let Content::ShapeContents(before) = &old.composition.layers[0].content else {
        panic!()
    };
    assert_eq!(
        contents(&e)
            .rows()
            .iter()
            .map(|(_, _, n)| n.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    for (_, _, node) in before.rows() {
        assert_eq!(
            node.parameters,
            contents(&e).node(node.id).unwrap().parameters
        );
    }
    assert_eq!(
        Project::from_json(&saved.to_json().unwrap()).unwrap(),
        saved
    );
    e.undo();
    assert_eq!(e.project(), &old);
    e.redo();
    assert_eq!(e.project(), &saved);
    edit(&mut e, ContentsEdit::Duplicate(4));
    assert_eq!(
        contents(&e).node(5).unwrap().composite,
        PaintComposite::AbovePrevious
    );
    e.undo();
    for item in [1, 2, 999] {
        assert!(
            e.execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Composite {
                    item,
                    mode: PaintComposite::AbovePrevious
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &saved);
    }
    let mut invalid = saved.clone();
    invalid.version = 45;
    assert!(Project::from_json(&serde_json::to_string(&invalid).unwrap()).is_err());
    invalid = saved.clone();
    let Content::ShapeContents(c) = &mut invalid.composition.layers[0].content else {
        panic!()
    };
    c.node_mut(1).unwrap().composite = PaintComposite::AbovePrevious;
    assert!(invalid.validate().is_err());
    edit(
        &mut e,
        ContentsEdit::Composite {
            item: 4,
            mode: PaintComposite::BelowPrevious,
        },
    );
    assert!(!e.project().to_json().unwrap().contains("\"composite\""));
    e.current.project.composition.layers[0].locked = true;
    let locked = e.project().clone();
    assert!(
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Composite {
                item: 4,
                mode: PaintComposite::AbovePrevious
            }
        })
        .is_err()
    );
    assert_eq!(e.project(), &locked);
}

#[test]
fn contents_skew_applies_after_scale_before_rotation_and_preserves_edit_coordinates() {
    use ContentsParam::{Skew, SkewAxis, Transform as T};
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    edit(&mut e, ContentsEdit::ConvertPath { item: 2, frame: 0 });
    contents_value(&mut e, 1, Skew, 45.);
    for (axis, point) in [(0., [30., -20.]), (90., [10., -10.]), (45., [15., -25.])] {
        contents_value(&mut e, 1, SkewAxis, axis);
        let m = contents(&e).node(1).unwrap().transform(0);
        let actual = m.point([10., -20.]);
        for i in 0..2 {
            assert!(
                (actual[i] - point[i]).abs() < 1e-9,
                "axis {axis}: {actual:?}"
            );
        }
    }
    contents_value(&mut e, 1, SkewAxis, 0.);
    for (p, v) in [
        (Property::AnchorX, 10.),
        (Property::AnchorY, 20.),
        (Property::PositionX, 100.),
        (Property::PositionY, 200.),
        (Property::ScaleX, 200.),
        (Property::ScaleY, 300.),
        (Property::Rotation, 90.),
    ] {
        contents_value(&mut e, 1, T(p), v);
    }
    let paths = contents(&e).editable_paths(0);
    let m = paths[0].2;
    // (12,23) - anchor -> (2,3); scale -> (4,9); shear -> (-5,9);
    // rotate 90 -> (-9,-5); translate -> (91,195).
    let point = m.point([12., 23.]);
    assert!((point[0] - 91.).abs() < 1e-9 && (point[1] - 195.).abs() < 1e-9);
    assert_eq!(m.point([10., 20.]), [100., 200.]);
    for vertex in &paths[0].1.vertices {
        let restored = m.inverse().unwrap().point(m.point(vertex.position));
        for i in 0..2 {
            assert!((restored[i] - vertex.position[i]).abs() < 1e-9);
        }
    }
    // Nested transform order is also the Pen editing order.
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::Group(vec![]),
        },
    );
    edit(
        &mut e,
        ContentsEdit::Move {
            item: 1,
            parent: 5,
            index: 0,
        },
    );
    contents_value(&mut e, 5, Skew, -45.);
    let nested = contents(&e).editable_paths(0)[0].2.point([12., 23.]);
    assert!((nested[0] - 286.).abs() < 1e-9 && (nested[1] - 195.).abs() < 1e-9);
}

#[test]
fn contents_skew_animation_roundtrips_retimes_and_rejects_singular_input_atomically() {
    let mut e = scene();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 0,
        end: 120,
    })
    .unwrap();
    edit(&mut e, ContentsEdit::Promote);
    let baseline = e.project().clone();
    for parameter in [ContentsParam::Skew, ContentsParam::SkewAxis] {
        for change in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value {
                frame: 60,
                value: 60.,
            },
        ] {
            edit(
                &mut e,
                ContentsEdit::Track {
                    item: 1,
                    parameter,
                    edit: change,
                },
            );
        }
        assert_eq!(contents(&e).node(1).unwrap().value_at(parameter, 30), 30.);
    }
    let saved = e.project().clone();
    assert_eq!(
        Project::from_json(&saved.to_json().unwrap()).unwrap(),
        saved
    );
    let path = PropertyPath::Contents {
        item: 1,
        parameter: ContentsParam::Skew,
    };
    let key = e.selected_layer().unwrap().copy_key(path, 60).unwrap();
    e.execute(Command::PasteKeys {
        keys: vec![key],
        frame: 90,
        target: None,
    })
    .unwrap();
    e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
    assert_eq!(e.selected_layer().unwrap().track_value(path, 40), Some(30.));
    assert!(
        e.selected_layer()
            .unwrap()
            .track(path)
            .unwrap()
            .keys()
            .contains_key(&100)
    );
    e.undo();
    e.undo();
    assert_eq!(e.project(), &saved);
    for value in [-90., 90., f64::INFINITY, f64::NAN] {
        assert!(
            e.execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Track {
                    item: 1,
                    parameter: ContentsParam::Skew,
                    edit: TrackEdit::Value { frame: 0, value }
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &saved);
    }
    for _ in 0..4 {
        e.undo();
    }
    assert_eq!(e.project(), &baseline);
    for _ in 0..4 {
        e.redo();
    }
    assert_eq!(e.project(), &saved);
}

#[test]
fn v43_contents_migration_adds_zero_skew_without_changing_existing_tracks() {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Group(vec![]),
        },
    );
    contents_value(&mut e, 1, ContentsParam::Transform(Property::Rotation), 33.);
    let expected = e.project().clone();
    let mut old = expected.clone();
    old.version = 43;
    let Content::ShapeContents(c) = &mut old.composition.layers[0].content else {
        panic!()
    };
    for id in [1, 5] {
        let node = c.node_mut(id).unwrap();
        node.parameters.remove(&ContentsParam::Skew);
        node.parameters.remove(&ContentsParam::SkewAxis);
    }
    let before = c.svg_at(0);
    let json = serde_json::to_string(&old).unwrap();
    let loaded = Project::from_json(&json).unwrap();
    assert_eq!(loaded, expected);
    let Content::ShapeContents(c) = &loaded.composition.layers[0].content else {
        panic!()
    };
    assert_eq!(before, c.svg_at(0));
    assert_eq!(
        c.node(1).unwrap().parameter_order(),
        vec![
            ContentsParam::Transform(Property::AnchorX),
            ContentsParam::Transform(Property::AnchorY),
            ContentsParam::Transform(Property::PositionX),
            ContentsParam::Transform(Property::PositionY),
            ContentsParam::Transform(Property::ScaleX),
            ContentsParam::Transform(Property::ScaleY),
            ContentsParam::Skew,
            ContentsParam::SkewAxis,
            ContentsParam::Transform(Property::Rotation),
            ContentsParam::Transform(Property::Opacity),
        ]
    );
    old.version = 44;
    assert!(Project::from_json(&serde_json::to_string(&old).unwrap()).is_err());
    let mut disguised = expected;
    disguised.version = 43;
    assert!(Project::from_json(&serde_json::to_string(&disguised).unwrap()).is_err());
}

#[test]
fn skew_overshoot_stays_finite_and_sampling_keeps_the_visible_limit() {
    for direction in [-2., 3.] {
        let mut e = scene();
        edit(&mut e, ContentsEdit::Promote);
        contents_value(&mut e, 1, ContentsParam::Skew, -89. / 3.);
        for change in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value {
                frame: 40,
                value: 89. / 3.,
            },
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Bezier(Bezier {
                    x1: 1. / 3.,
                    y1: direction,
                    x2: 2. / 3.,
                    y2: direction,
                }),
            },
        ] {
            edit(
                &mut e,
                ContentsEdit::Track {
                    item: 1,
                    parameter: ContentsParam::Skew,
                    edit: change,
                },
            );
        }
        let expected = if direction < 0. { -89. } else { 89. };
        let before = e.project().clone();
        let n = contents(&e).node(1).unwrap();
        assert_eq!(n.value_at(ContentsParam::Skew, 20), expected);
        assert!(n.transform(20).inverse().is_some());
        for change in [
            TrackEdit::ToggleKey { frame: 20 },
            TrackEdit::ToggleAnimation { frame: 20 },
        ] {
            edit(
                &mut e,
                ContentsEdit::Track {
                    item: 1,
                    parameter: ContentsParam::Skew,
                    edit: change,
                },
            );
            assert_eq!(
                contents(&e).node(1).unwrap().parameters[&ContentsParam::Skew].value_at(20),
                expected
            );
            assert_eq!(
                Project::from_json(&e.project().to_json().unwrap()).unwrap(),
                *e.project()
            );
            e.undo();
            assert_eq!(e.project(), &before);
        }
    }
}
#[test]
fn contents_stroke_structure_preserves_other_tracks_and_restores_removed_dash_keys() {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    edit(&mut e, ContentsEdit::AddDash(3));
    edit(&mut e, ContentsEdit::AddDash(3));
    for p in [
        ShapeParam::DashLength(0),
        ShapeParam::DashLength(1),
        ShapeParam::StrokeWidth,
    ] {
        for change in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value {
                frame: 60,
                value: 40.,
            },
        ] {
            edit(
                &mut e,
                ContentsEdit::Track {
                    item: 3,
                    parameter: ContentsParam::Shape(p),
                    edit: change,
                },
            );
        }
    }
    let before = e.project().clone();
    let tracks = contents(&e).node(3).unwrap().parameters.clone();
    edit(
        &mut e,
        ContentsEdit::StrokeCap {
            item: 3,
            cap: StrokeCap::Square,
        },
    );
    edit(
        &mut e,
        ContentsEdit::StrokeJoin {
            item: 3,
            join: StrokeJoin::Bevel,
        },
    );
    assert_eq!(contents(&e).node(3).unwrap().parameters, tracks);
    let styled = e.project().clone();
    edit(&mut e, ContentsEdit::RemoveDash(3));
    let n = contents(&e).node(3).unwrap();
    assert!(
        !n.parameters
            .contains_key(&ContentsParam::Shape(ShapeParam::DashLength(1)))
    );
    assert_eq!(
        n.parameters[&ContentsParam::Shape(ShapeParam::DashLength(0))],
        tracks[&ContentsParam::Shape(ShapeParam::DashLength(0))]
    );
    e.undo();
    assert_eq!(e.project(), &styled);
    e.redo();
    e.undo();
    e.undo();
    e.undo();
    assert_eq!(e.project(), &before);
    for _ in 2..ShapeStroke::MAX_DASHES {
        edit(&mut e, ContentsEdit::AddDash(3));
    }
    let full = e.project().clone();
    for bad in [
        ContentsEdit::AddDash(3),
        ContentsEdit::RemoveDash(4),
        ContentsEdit::StrokeCap {
            item: 2,
            cap: StrokeCap::Round,
        },
        ContentsEdit::StrokeJoin {
            item: 999,
            join: StrokeJoin::Miter,
        },
    ] {
        assert!(e.execute(Command::Contents { id: 1, edit: bad }).is_err());
        assert_eq!(e.project(), &full);
    }
    for _ in 0..ShapeStroke::MAX_DASHES {
        edit(&mut e, ContentsEdit::RemoveDash(3));
    }
    let empty = e.project().clone();
    assert!(
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::RemoveDash(3)
        })
        .is_err()
    );
    assert_eq!(e.project(), &empty);
    assert_eq!(
        Project::from_json(&empty.to_json().unwrap()).unwrap(),
        empty
    );
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
    assert_eq!(saved.version, 44);
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

#[test]
fn group_space_includes_empty_selected_group_and_animated_ancestors() {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    for id in [2, 3, 4] {
        edit(&mut e, ContentsEdit::Remove(id));
    }
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Group(vec![]),
        },
    );
    for (item, parameter, value) in [
        (1, ContentsParam::Transform(Property::PositionX), 40.),
        (1, ContentsParam::Transform(Property::PositionY), 70.),
        (1, ContentsParam::Transform(Property::Rotation), 90.),
        (5, ContentsParam::Transform(Property::AnchorX), 3.),
        (5, ContentsParam::Transform(Property::AnchorY), 4.),
        (5, ContentsParam::Transform(Property::PositionX), 10.),
        (5, ContentsParam::Transform(Property::PositionY), 20.),
        (5, ContentsParam::Transform(Property::ScaleX), -200.),
        (5, ContentsParam::Transform(Property::ScaleY), 50.),
        (5, ContentsParam::Skew, 45.),
    ] {
        contents_value(&mut e, item, parameter, value);
    }
    let parameter = ContentsParam::Transform(Property::PositionX);
    edit(
        &mut e,
        ContentsEdit::Track {
            item: 1,
            parameter,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
    edit(
        &mut e,
        ContentsEdit::Track {
            item: 1,
            parameter,
            edit: TrackEdit::Value {
                frame: 20,
                value: 60.,
            },
        },
    );
    let before = e.project().clone();
    assert!(contents(&e).editable_paths(10).is_empty());
    for frame in [0, 5, 10, 20] {
        let transform = contents(&e).group_transform(5, frame).unwrap();
        for [x, y] in [[0., 0.], [1., 0.], [0., 1.], [-13., 29.]] {
            // Independent scalar scale/reflection -> shear -> translation, then
            // the outer 90-degree rotation and linearly animated translation.
            let inner_x = -2. * (x - 3.) - 0.5 * (y - 4.) + 10.;
            let inner_y = 0.5 * (y - 4.) + 20.;
            let expected = [40. + frame as f64 - inner_y, 70. + inner_x];
            let actual = transform.point([x, y]);
            assert!((actual[0] - expected[0]).abs() < 1e-9);
            assert!((actual[1] - expected[1]).abs() < 1e-9);
        }
    }
    assert_eq!(e.project(), &before);
}

#[test]
fn group_space_excludes_disabled_ancestry_and_non_groups_without_hiding_singularity() {
    let mut e = scene();
    edit(&mut e, ContentsEdit::Promote);
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Group(vec![]),
        },
    );
    assert!(contents(&e).group_transform(5, 0).is_some());
    for id in [0, 2, 3, 4, 999] {
        assert!(contents(&e).group_transform(id, 0).is_none());
    }
    for id in [1, 5] {
        edit(
            &mut e,
            ContentsEdit::Enabled {
                item: id,
                enabled: false,
            },
        );
        assert!(contents(&e).group_transform(5, 0).is_none());
        edit(
            &mut e,
            ContentsEdit::Enabled {
                item: id,
                enabled: true,
            },
        );
    }
    contents_value(&mut e, 5, ContentsParam::Transform(Property::ScaleX), 0.);
    assert!(
        contents(&e)
            .group_transform(5, 0)
            .unwrap()
            .inverse()
            .is_none()
    );
}
