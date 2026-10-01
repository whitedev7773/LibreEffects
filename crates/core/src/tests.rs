use super::*;

#[test]
fn trim_at_playhead_keeps_current_frame_and_keys_with_atomic_group_rollback() {
    let mut e = editor_with_layer();
    key(&mut e, 10);
    set(&mut e, 100, 500.0);
    let keys = e.selected_layer().unwrap().properties.clone();
    e.execute(Command::TrimLayers {
        ids: vec![1],
        frame: 40,
        start: true,
    })
    .unwrap();
    e.execute(Command::TrimLayers {
        ids: vec![1],
        frame: 70,
        start: false,
    })
    .unwrap();
    let l = e.selected_layer().unwrap();
    assert_eq!((l.in_frame(), l.out_frame(150)), (40, 71));
    assert!(l.active_at(70, 150));
    assert!(!l.active_at(71, 150));
    assert_eq!(l.properties, keys);
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::ToggleLocked(2)).unwrap();
    let before = e.project().clone();
    assert!(
        e.execute(Command::TrimLayers {
            ids: vec![1, 2],
            frame: 50,
            start: true
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
    assert!(
        e.execute(Command::TrimLayers {
            ids: vec![1],
            frame: 80,
            start: true
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
}

#[test]
fn nudge_moves_selected_hierarchy_once_in_composition_coordinates() {
    let mut e = editor_with_layer();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetParent {
        id: 2,
        parent: Some(1),
        frame: 0,
    })
    .unwrap();
    set_property(&mut e, 1, Property::Rotation, 42.0);
    set_property(&mut e, 1, Property::ScaleX, 180.0);
    let before = e.project().clone();
    e.execute(Command::NudgeLayers {
        ids: vec![1, 2],
        frame: 0,
        delta: [10.0, -1.0],
    })
    .unwrap();
    for id in [1, 2] {
        let expected = before
            .composition()
            .corners_at(id, 0)
            .unwrap()
            .map(|[x, y]| [x + 10.0, y - 1.0]);
        assert_corners_close(
            e.project().composition().corners_at(id, 0).unwrap(),
            expected,
        );
    }
    e.undo();
    assert_eq!(e.project(), &before);
    e.execute(Command::NudgeLayers {
        ids: vec![2],
        frame: 0,
        delta: [1.0, 10.0],
    })
    .unwrap();
    let expected = before
        .composition()
        .corners_at(2, 0)
        .unwrap()
        .map(|[x, y]| [x + 1.0, y + 10.0]);
    assert_corners_close(
        e.project().composition().corners_at(2, 0).unwrap(),
        expected,
    );
}

#[test]
fn anchor_tool_preserves_parented_pose_and_keys_in_one_undo() {
    let mut e = editor_with_layer();
    e.execute(Command::AddRectangle).unwrap();
    set_property(&mut e, 1, Property::Rotation, 37.0);
    set_property(&mut e, 1, Property::ScaleX, 175.0);
    set_property(&mut e, 1, Property::ScaleY, -65.0);
    e.execute(Command::SetParent {
        id: 2,
        parent: Some(1),
        frame: 0,
    })
    .unwrap();
    set_property(&mut e, 2, Property::Rotation, -28.0);
    for property in [
        Property::AnchorX,
        Property::AnchorY,
        Property::PositionX,
        Property::PositionY,
    ] {
        e.execute(Command::ToggleKeyframe {
            id: 2,
            property,
            frame: 0,
        })
        .unwrap();
    }
    let before = e.project().clone();
    let corners = before.composition().corners_at(2, 30).unwrap();
    e.execute(Command::SetAnchor {
        id: 2,
        frame: 30,
        x: -40.0,
        y: 280.0,
    })
    .unwrap();
    assert_corners_close(
        corners,
        e.project().composition().corners_at(2, 30).unwrap(),
    );
    for property in [
        Property::AnchorX,
        Property::AnchorY,
        Property::PositionX,
        Property::PositionY,
    ] {
        assert!(
            e.project()
                .composition()
                .layer(2)
                .unwrap()
                .property(property)
                .keys()
                .contains_key(&30)
        );
    }
    e.undo();
    assert_eq!(e.project(), &before);
    assert!(
        e.execute(Command::SetAnchor {
            id: 2,
            frame: 30,
            x: f64::NAN,
            y: 0.0
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
}

#[test]
fn group_duplicate_remaps_internal_parents_and_keeps_external_parents() {
    let mut e = editor_with_layer();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetParent {
        id: 2,
        parent: Some(1),
        frame: 0,
    })
    .unwrap();
    e.execute(Command::SetParent {
        id: 3,
        parent: Some(2),
        frame: 0,
    })
    .unwrap();
    set_property(&mut e, 1, Property::Rotation, 31.0);
    let before = e.project().clone();
    e.execute(Command::DuplicateLayers(vec![2, 3, 2])).unwrap();
    let comp = e.project().composition();
    let child = &comp.layers()[0];
    let parent = &comp.layers()[2];
    assert_eq!(child.parent(), Some(parent.id()));
    assert_eq!(parent.parent(), Some(1));
    assert_corners_close(
        comp.corners_at(child.id(), 0).unwrap(),
        before.composition().corners_at(3, 0).unwrap(),
    );
    e.undo();
    assert_eq!(e.project(), &before);
    e.execute(Command::ToggleLocked(3)).unwrap();
    let locked = e.project().clone();
    assert!(e.execute(Command::DuplicateLayers(vec![2, 3])).is_err());
    assert_eq!(e.project(), &locked);
}

#[test]
fn split_preserves_keyframes_pose_and_exclusive_ranges_with_atomic_failures() {
    let mut e = editor_with_layer();
    key(&mut e, 0);
    set(&mut e, 100, 500.0);
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetParent {
        id: 2,
        parent: Some(1),
        frame: 0,
    })
    .unwrap();
    let before = e.project().clone();
    for frame in [0, 150] {
        assert!(
            e.execute(Command::SplitLayers {
                ids: vec![1, 2],
                frame
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }
    e.execute(Command::SplitLayers {
        ids: vec![1, 2],
        frame: 50,
    })
    .unwrap();
    let comp = e.project().composition();
    for (original, split) in [(2, 3), (1, 4)] {
        let head = comp.layer(original).unwrap();
        let tail = comp.layer(split).unwrap();
        assert!(head.active_at(49, comp.duration()));
        assert!(!head.active_at(50, comp.duration()));
        assert!(!tail.active_at(49, comp.duration()));
        assert!(tail.active_at(50, comp.duration()));
        assert_eq!(head.properties, tail.properties);
        for frame in [50, 75, 149] {
            assert_corners_close(
                comp.corners_at(split, frame).unwrap(),
                before.composition().corners_at(original, frame).unwrap(),
            );
        }
    }
    assert_eq!(comp.layer(3).unwrap().parent(), Some(4));
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
    e.undo();
    assert_eq!(e.project(), &before);
    e.execute(Command::SetLayerRange {
        id: 2,
        start: 70,
        end: 100,
    })
    .unwrap();
    let restricted = e.project().clone();
    assert!(
        e.execute(Command::SplitLayers {
            ids: vec![1, 2],
            frame: 50
        })
        .is_err()
    );
    assert_eq!(e.project(), &restricted);
}

#[test]
fn batch_key_move_handles_overlapping_sources_and_rolls_back_collisions() {
    let mut e = editor_with_layer();
    for frame in [10, 20, 40] {
        e.execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::PositionX,
            frame,
        })
        .unwrap();
    }
    let keys: Vec<_> = [10, 20]
        .map(|frame| KeyRef {
            id: 1,
            property: Property::PositionX,
            frame,
        })
        .into();
    let before = e.project().clone();
    e.execute(Command::MoveKeys {
        keys: keys.clone(),
        delta: 10,
    })
    .unwrap();
    assert_eq!(
        e.selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![20, 30, 40]
    );
    e.undo();
    assert_eq!(e.project(), &before);
    assert!(
        e.execute(Command::MoveKeys {
            keys: keys.clone(),
            delta: 20
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
    assert!(e.execute(Command::MoveKeys { keys, delta: -11 }).is_err());
    assert_eq!(e.project(), &before);
}
#[test]
fn key_clipboard_preserves_timing_interpolation_and_is_atomic() {
    let mut e = editor_with_layer();
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::PositionX,
        frame: 10,
    })
    .unwrap();
    e.execute(Command::SetInterpolation {
        id: 1,
        property: Property::PositionX,
        frame: 10,
        interpolation: Interpolation::Bezier(Bezier::default()),
    })
    .unwrap();
    let data = e
        .selected_layer()
        .unwrap()
        .property(Property::PositionX)
        .keys()[&10]
        .clone();
    let key = KeyCopy {
        key: KeyRef {
            id: 1,
            property: Property::PositionX,
            frame: 10,
        },
        data: data.clone(),
    };
    e.execute(Command::PasteKeys {
        keys: vec![key.clone()],
        frame: 60,
        target: Some(1),
    })
    .unwrap();
    assert_eq!(
        e.selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()[&60],
        data
    );
    let before = e.project().clone();
    assert!(
        e.execute(Command::PasteKeys {
            keys: vec![key],
            frame: 60,
            target: None
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
}
#[test]
fn layer_shift_moves_keys_and_range_once_and_rejects_overflow() {
    let mut e = editor_with_layer();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 10,
        end: 100,
    })
    .unwrap();
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::Rotation,
        frame: 20,
    })
    .unwrap();
    let before = e.project().clone();
    e.execute(Command::ShiftLayer { id: 1, delta: 20 }).unwrap();
    let l = e.selected_layer().unwrap();
    assert_eq!(l.in_frame(), 30);
    assert_eq!(l.out_frame(150), 120);
    assert!(l.property(Property::Rotation).keys().contains_key(&40));
    e.undo();
    assert_eq!(e.project(), &before);
    assert!(e.execute(Command::ShiftLayer { id: 1, delta: 80 }).is_err());
    assert_eq!(e.project(), &before);
}
#[test]
fn text_effect_mask_roundtrip_and_locked_batch_rollback() {
    let mut e = editor_with_layer();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: "한글 & <Text>".into(),
            font_size: 40.0,
        },
        width: 400.0,
        height: 80.0,
        name: "Title".into(),
    })
    .unwrap();
    e.execute(Command::SetMask {
        id: 2,
        mask: Some(Mask {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            inverted: true,
        }),
    })
    .unwrap();
    e.execute(Command::SetEffects {
        id: 2,
        effects: Effects {
            blur: 2.0,
            brightness: 0.5,
            grayscale: true,
        },
    })
    .unwrap();
    assert_eq!(e.project().version, 3);
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
    e.execute(Command::ToggleLocked(2)).unwrap();
    let before = e.project().clone();
    assert!(
        e.execute(Command::Batch(vec![
            Command::RenameLayer {
                id: 1,
                name: "Changed".into()
            },
            Command::SetColor {
                id: 2,
                color: 0xff0000
            }
        ]))
        .is_err()
    );
    assert_eq!(e.project(), &before);
}

#[test]
fn alignment_uses_world_bounds_with_parenting_and_one_undo_step() {
    let mut editor = editor_with_layer();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
    set_property(&mut editor, 1, Property::Rotation, 35.0);
    set_property(&mut editor, 1, Property::ScaleX, 150.0);
    set_property(&mut editor, 2, Property::Rotation, 18.0);
    set_property(&mut editor, 2, Property::PositionX, 700.0);
    set_property(&mut editor, 2, Property::PositionY, 420.0);
    let before = editor.project().clone();
    for alignment in [
        Alignment::Left,
        Alignment::HorizontalCenter,
        Alignment::Right,
        Alignment::Top,
        Alignment::VerticalCenter,
        Alignment::Bottom,
    ] {
        editor
            .execute(Command::AlignLayer {
                id: 2,
                frame: 0,
                alignment,
            })
            .unwrap();
        let corners = editor.project().composition().corners_at(2, 0).unwrap();
        let xs = corners.map(|p| p[0]);
        let ys = corners.map(|p| p[1]);
        let min_x = xs.into_iter().fold(f64::INFINITY, f64::min);
        let max_x = xs.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let min_y = ys.into_iter().fold(f64::INFINITY, f64::min);
        let max_y = ys.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let (actual, expected) = match alignment {
            Alignment::Left => (min_x, 0.0),
            Alignment::HorizontalCenter => ((min_x + max_x) / 2.0, 960.0),
            Alignment::Right => (max_x, 1920.0),
            Alignment::Top => (min_y, 0.0),
            Alignment::VerticalCenter => ((min_y + max_y) / 2.0, 540.0),
            Alignment::Bottom => (max_y, 1080.0),
        };
        assert!((actual - expected).abs() < 1e-7);
        editor.undo();
        assert_eq!(*editor.project(), before);
    }
    editor.execute(Command::ToggleLocked(2)).unwrap();
    let locked = editor.project().clone();
    assert!(
        editor
            .execute(Command::AlignLayer {
                id: 2,
                frame: 0,
                alignment: Alignment::Left
            })
            .is_err()
    );
    assert_eq!(*editor.project(), locked);
}

#[test]
fn bundled_curve_parent_study_roundtrips_and_inherits_motion() {
    let project = Project::from_json(include_str!(
        "../../../examples/curve-parent-study.lfe.json"
    ))
    .unwrap();
    let comp = project.composition();
    assert_eq!(comp.layer(2).unwrap().parent(), Some(1));
    assert!(matches!(
        comp.layer(1).unwrap().property(Property::PositionX).keys()[&0].interpolation,
        Interpolation::Bezier(_)
    ));
    assert_ne!(comp.corners_at(2, 0), comp.corners_at(2, 60));
    assert_eq!(
        Project::from_json(&project.to_json().unwrap()).unwrap(),
        project
    );
}

#[test]
fn temporal_bezier_solves_time_instead_of_using_parameter_as_time() {
    let curve = Bezier {
        x1: 0.42,
        y1: 0.0,
        x2: 1.0,
        y2: 1.0,
    };
    assert!((curve.progress(0.5) - 0.3153568125).abs() < 1e-8);
    assert_eq!(curve.progress(0.0), 0.0);
    assert_eq!(curve.progress(1.0), 1.0);
    let vertical = Bezier {
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 1.0,
    };
    assert!((vertical.progress(0.125) - 0.5).abs() < 1e-9);
    let mut editor = editor_with_layer();
    set(&mut editor, 0, 0.0);
    key(&mut editor, 0);
    set(&mut editor, 100, 100.0);
    editor
        .execute(Command::SetInterpolation {
            id: 1,
            property: Property::PositionX,
            frame: 0,
            interpolation: Interpolation::Bezier(curve),
        })
        .unwrap();
    let track = editor
        .selected_layer()
        .unwrap()
        .property(Property::PositionX);
    assert!((track.sample(50.0) - 31.53568125).abs() < 1e-6);
    assert!(track.sample(50.5) > track.value_at(50));
    assert_eq!(editor.project().version, 2);
    assert_eq!(
        Project::from_json(&editor.project().to_json().unwrap()).unwrap(),
        *editor.project()
    );
}

#[test]
fn invalid_bezier_data_and_commands_are_rejected_without_mutation() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    let before = editor.project().clone();
    for curve in [
        Bezier {
            x1: -0.1,
            ..Bezier::default()
        },
        Bezier {
            y2: f64::NAN,
            ..Bezier::default()
        },
        Bezier {
            x2: 1.1,
            ..Bezier::default()
        },
    ] {
        assert!(
            editor
                .execute(Command::SetInterpolation {
                    id: 1,
                    property: Property::PositionX,
                    frame: 0,
                    interpolation: Interpolation::Bezier(curve)
                })
                .is_err()
        );
        assert_eq!(*editor.project(), before);
    }
    let mut json: serde_json::Value = serde_json::from_str(&before.to_json().unwrap()).unwrap();
    json["composition"]["layers"][0]["properties"]["PositionX"]["keys"]["0"]["interpolation"] =
        serde_json::json!({"Bezier":{"x1":2.0,"y1":0.0,"x2":0.5,"y2":1.0}});
    assert!(Project::from_json(&json.to_string()).is_err());
}

#[test]
fn graph_key_edit_is_one_undo_step_and_failures_are_atomic() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 30, 400.0);
    let before = editor.project().clone();
    for (to, value) in [(0, 12.0), (40, f64::NAN), (150, 20.0)] {
        assert!(
            editor
                .execute(Command::EditKeyframe {
                    id: 1,
                    property: Property::PositionX,
                    from: 30,
                    to,
                    value
                })
                .is_err()
        );
        assert_eq!(*editor.project(), before);
    }
    editor
        .execute(Command::EditKeyframe {
            id: 1,
            property: Property::PositionX,
            from: 30,
            to: 45,
            value: 525.0,
        })
        .unwrap();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .value_at(45),
        525.0
    );
    editor.undo();
    assert_eq!(*editor.project(), before);
    editor.redo();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .value_at(45),
        525.0
    );
}

fn assert_corners_close(a: [[f64; 2]; 4], b: [[f64; 2]; 4]) {
    for (a, b) in a.into_iter().flatten().zip(b.into_iter().flatten()) {
        assert!((a - b).abs() < 1e-7, "{a} != {b}");
    }
}
fn set_property(editor: &mut Editor, id: LayerId, property: Property, value: f64) {
    editor
        .execute(Command::SetValue {
            id,
            property,
            frame: 0,
            value,
        })
        .unwrap();
}
#[test]
fn parenting_and_unparenting_preserve_pose_under_rotated_nonuniform_scale() {
    let mut editor = editor_with_layer();
    editor.execute(Command::AddRectangle).unwrap();
    set_property(&mut editor, 1, Property::Rotation, 37.0);
    set_property(&mut editor, 1, Property::ScaleX, 175.0);
    set_property(&mut editor, 1, Property::ScaleY, -65.0);
    set_property(&mut editor, 2, Property::PositionX, 300.0);
    set_property(&mut editor, 2, Property::Rotation, -22.0);
    let pose = editor.project().composition().corners_at(2, 0).unwrap();
    editor
        .execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
    assert_corners_close(
        editor.project().composition().corners_at(2, 0).unwrap(),
        pose,
    );
    let bound = editor.project().clone();
    set_property(&mut editor, 1, Property::PositionX, 1010.0);
    let moved = pose.map(|[x, y]| [x + 50.0, y]);
    assert_corners_close(
        editor.project().composition().corners_at(2, 0).unwrap(),
        moved,
    );
    editor
        .execute(Command::SetParent {
            id: 2,
            parent: None,
            frame: 0,
        })
        .unwrap();
    assert_corners_close(
        editor.project().composition().corners_at(2, 0).unwrap(),
        moved,
    );
    assert_eq!(
        editor.project().composition().layer(2).unwrap().parent(),
        None
    );
    editor.undo();
    editor.undo();
    assert_eq!(*editor.project(), bound);
    assert_eq!(
        Project::from_json(&bound.to_json().unwrap()).unwrap(),
        bound
    );
}
#[test]
fn parent_animation_affects_children_but_visibility_and_opacity_do_not() {
    let mut editor = editor_with_layer();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
    let pose = editor.project().composition().corners_at(2, 0).unwrap();
    editor
        .execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::PositionX,
            frame: 0,
        })
        .unwrap();
    editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame: 30,
            value: 1260.0,
        })
        .unwrap();
    set_property(&mut editor, 1, Property::Opacity, 0.0);
    editor.execute(Command::ToggleVisible(1)).unwrap();
    let comp = editor.project().composition();
    assert_corners_close(
        comp.corners_at(2, 15).unwrap(),
        pose.map(|[x, y]| [x + 150.0, y]),
    );
    assert!(comp.layer(2).unwrap().active_at(15, 150));
    assert_eq!(
        comp.layer(2)
            .unwrap()
            .property(Property::Opacity)
            .value_at(15),
        100.0
    );
    let local_delta = comp
        .position_space(2, 15)
        .unwrap()
        .inverse()
        .unwrap()
        .vector([20.0, 10.0]);
    assert!((local_delta[0] - 20.0).abs() < 1e-7);
}
#[test]
fn parent_hierarchy_rejects_cycles_missing_targets_deletion_and_zero_scale() {
    let mut editor = editor_with_layer();
    editor.execute(Command::AddRectangle).unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
    editor
        .execute(Command::SetParent {
            id: 3,
            parent: Some(2),
            frame: 0,
        })
        .unwrap();
    let before = editor.project().clone();
    for command in [
        Command::SetParent {
            id: 1,
            parent: Some(3),
            frame: 0,
        },
        Command::SetParent {
            id: 1,
            parent: Some(1),
            frame: 0,
        },
        Command::SetParent {
            id: 1,
            parent: Some(99),
            frame: 0,
        },
        Command::RemoveLayer(1),
    ] {
        assert!(editor.execute(command).is_err());
        assert_eq!(*editor.project(), before);
    }
    let mut invalid = before.clone();
    invalid
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == 1)
        .unwrap()
        .parent = Some(3);
    assert!(Project::from_json(&invalid.to_json().unwrap()).is_err());
    editor.execute(Command::AddRectangle).unwrap();
    set_property(&mut editor, 4, Property::ScaleX, 0.0);
    assert!(
        editor
            .execute(Command::SetParent {
                id: 3,
                parent: Some(4),
                frame: 0
            })
            .is_err()
    );
    editor.execute(Command::ToggleLocked(3)).unwrap();
    assert!(
        editor
            .execute(Command::SetParent {
                id: 3,
                parent: None,
                frame: 0
            })
            .is_err()
    );
}

#[test]
fn bundled_motion_study_loads_and_animates() {
    let project =
        Project::from_json(include_str!("../../../examples/motion-study.lfe.json")).unwrap();
    let comp = project.composition();
    assert_eq!(comp.layers().len(), 3);
    let track = comp.layer(1).unwrap().property(Property::PositionX);
    assert_eq!(track.value_at(0), 330.0);
    assert_eq!(track.value_at(60), 470.0);
    assert!(comp.layer(3).unwrap().locked());
}

#[test]
fn duplicate_layer_preserves_animation_with_unique_identity_and_history() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 30, 400.0);
    editor.execute(Command::DuplicateLayer(1)).unwrap();
    let layers = editor.project().composition().layers();
    assert_eq!(layers.len(), 2);
    assert_ne!(layers[0].id(), layers[1].id());
    assert_eq!(
        layers[0].property(Property::PositionX),
        layers[1].property(Property::PositionX)
    );
    assert_eq!(editor.selected(), Some(2));
    editor.undo();
    assert_eq!(editor.project().composition().layers().len(), 1);
    editor.redo();
    assert_eq!(editor.selected(), Some(2));
}

#[test]
fn layer_range_is_exclusive_validated_and_backward_compatible() {
    let mut editor = editor_with_layer();
    editor
        .execute(Command::SetLayerRange {
            id: 1,
            start: 10,
            end: 40,
        })
        .unwrap();
    let layer = editor.selected_layer().unwrap();
    assert!(!layer.active_at(9, 150));
    assert!(layer.active_at(10, 150));
    assert!(layer.active_at(39, 150));
    assert!(!layer.active_at(40, 150));
    let before = editor.project().clone();
    assert!(
        editor
            .execute(Command::SetLayerRange {
                id: 1,
                start: 40,
                end: 10
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    let mut json: serde_json::Value = serde_json::from_str(&before.to_json().unwrap()).unwrap();
    let layer = json["composition"]["layers"][0].as_object_mut().unwrap();
    layer.remove("in_frame");
    layer.remove("out_frame");
    let legacy = Project::from_json(&json.to_string()).unwrap();
    assert_eq!(legacy.composition().layers()[0].in_frame(), 0);
    assert_eq!(legacy.composition().layers()[0].out_frame(150), 150);
}

#[test]
fn keyframe_move_preserves_interpolation_and_rejects_collisions_atomically() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 30, 400.0);
    editor
        .execute(Command::SetInterpolation {
            id: 1,
            property: Property::PositionX,
            frame: 30,
            interpolation: Interpolation::Hold,
        })
        .unwrap();
    editor
        .execute(Command::MoveKeyframe {
            id: 1,
            property: Property::PositionX,
            from: 30,
            to: 45,
        })
        .unwrap();
    let track = editor
        .selected_layer()
        .unwrap()
        .property(Property::PositionX);
    assert!(!track.keys().contains_key(&30));
    assert_eq!(track.keys()[&45].interpolation, Interpolation::Hold);
    let before = editor.project().clone();
    assert!(
        editor
            .execute(Command::MoveKeyframe {
                id: 1,
                property: Property::PositionX,
                from: 45,
                to: 0
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    editor.undo();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .contains_key(&30)
    );
}

#[test]
fn disabling_animation_bakes_current_value_and_is_undoable() {
    let mut editor = editor_with_layer();
    set(&mut editor, 0, 0.0);
    key(&mut editor, 0);
    set(&mut editor, 30, 300.0);
    editor
        .execute(Command::ToggleAnimation {
            id: 1,
            property: Property::PositionX,
            frame: 15,
        })
        .unwrap();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .is_empty()
    );
    assert_eq!(value(&editor, 0), 150.0);
    editor.undo();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .len(),
        2
    );
}

#[test]
fn canvas_position_is_one_undo_step_and_invalid_y_is_atomic() {
    let mut editor = editor_with_layer();
    let before = editor.project().clone();
    editor
        .execute(Command::SetPosition {
            id: 1,
            frame: 0,
            x: 500.0,
            y: 400.0,
        })
        .unwrap();
    assert_eq!(value(&editor, 0), 500.0);
    editor.undo();
    assert_eq!(editor.project(), &before);
    assert!(
        editor
            .execute(Command::SetPosition {
                id: 1,
                frame: 0,
                x: 100.0,
                y: f64::NAN
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
}

#[test]
fn composition_settings_preserve_content_and_reject_destructive_shortening() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 100, 100.0);
    let command = |duration| Command::ConfigureComposition {
        name: "Main".into(),
        width: 1280,
        height: 720,
        fps: 24,
        duration,
    };
    let before = editor.project().clone();
    assert!(editor.execute(command(90)).is_err());
    assert_eq!(editor.project(), &before);
    editor.execute(command(240)).unwrap();
    assert_eq!(editor.project().composition().duration(), 240);
    assert_eq!(editor.project().composition().layers().len(), 1);
    let encoded = editor.project().to_json().unwrap();
    assert_eq!(Project::from_json(&encoded).unwrap(), *editor.project());
    editor.undo();
    assert_eq!(editor.project(), &before);
}

#[test]
fn locked_layers_reject_new_commands_and_rename_is_validated() {
    let mut editor = editor_with_layer();
    assert!(
        editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "  ".into()
            })
            .is_err()
    );
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: " Hero ".into(),
        })
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().name(), "Hero");
    editor.execute(Command::ToggleLocked(1)).unwrap();
    for command in [
        Command::DuplicateLayer(1),
        Command::RenameLayer {
            id: 1,
            name: "Other".into(),
        },
        Command::SetPosition {
            id: 1,
            frame: 0,
            x: 20.0,
            y: 20.0,
        },
        Command::SetLayerRange {
            id: 1,
            start: 1,
            end: 50,
        },
        Command::ToggleAnimation {
            id: 1,
            property: Property::PositionX,
            frame: 0,
        },
    ] {
        assert!(editor.execute(command).is_err());
    }
}

fn editor_with_layer() -> Editor {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    editor
}

fn set(editor: &mut Editor, frame: Frame, value: f64) {
    editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame,
            value,
        })
        .unwrap();
}

fn key(editor: &mut Editor, frame: Frame) {
    editor
        .execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::PositionX,
            frame,
        })
        .unwrap();
}

fn value(editor: &Editor, frame: Frame) -> f64 {
    editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .property(Property::PositionX)
        .value_at(frame)
}

#[test]
fn animation_clamps_endpoints_and_interpolates_in_frame_time() {
    let mut editor = editor_with_layer();
    set(&mut editor, 10, 100.0);
    key(&mut editor, 10);
    set(&mut editor, 30, 300.0);
    assert_eq!(value(&editor, 0), 100.0);
    assert_eq!(value(&editor, 10), 100.0);
    assert_eq!(value(&editor, 20), 200.0);
    assert_eq!(value(&editor, 30), 300.0);
    assert_eq!(value(&editor, 149), 300.0);
}

#[test]
fn hold_and_smooth_use_the_outgoing_key() {
    let mut editor = editor_with_layer();
    set(&mut editor, 0, 0.0);
    key(&mut editor, 0);
    set(&mut editor, 100, 100.0);
    for (interpolation, expected) in [(Interpolation::Hold, 0.0), (Interpolation::Smooth, 15.625)] {
        editor
            .execute(Command::SetInterpolation {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                interpolation,
            })
            .unwrap();
        assert_eq!(value(&editor, 25), expected);
        assert_eq!(value(&editor, 100), 100.0);
    }
}

#[test]
fn editing_same_frame_replaces_key_and_removing_last_key_preserves_value() {
    let mut editor = editor_with_layer();
    key(&mut editor, 10);
    set(&mut editor, 10, 42.0);
    set(&mut editor, 10, 84.0);
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .len(),
        1
    );
    key(&mut editor, 10);
    assert_eq!(value(&editor, 0), 84.0);
    assert_eq!(value(&editor, 149), 84.0);
}

#[test]
fn layer_order_and_selection_survive_undo_redo() {
    let mut editor = editor_with_layer();
    editor.execute(Command::AddRectangle).unwrap();
    assert_eq!(editor.project().composition().layers()[0].id(), 2);
    editor
        .execute(Command::MoveLayer { id: 2, index: 1 })
        .unwrap();
    assert_eq!(editor.selected(), Some(2));
    editor.execute(Command::RemoveLayer(2)).unwrap();
    assert_eq!(editor.selected(), Some(1));
    editor.undo();
    assert_eq!(editor.selected(), Some(2));
    assert_eq!(editor.project().composition().layers()[1].id(), 2);
    editor.redo();
    assert_eq!(editor.project().composition().layers().len(), 1);
}

#[test]
fn fresh_edit_clears_redo_and_noop_does_not_add_history() {
    let mut editor = editor_with_layer();
    set(&mut editor, 0, 42.0);
    editor.undo();
    assert!(editor.can_redo());
    set(&mut editor, 0, 52.0);
    assert!(!editor.can_redo());
    set(&mut editor, 0, 52.0);
    editor.undo();
    assert_eq!(value(&editor, 0), 960.0);
}

#[test]
fn locked_layers_reject_mutations_without_changing_history() {
    let mut editor = editor_with_layer();
    editor.execute(Command::ToggleLocked(1)).unwrap();
    let before = editor.project().clone();
    assert!(editor.execute(Command::RemoveLayer(1)).is_err());
    assert!(
        editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                value: 0.0,
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    editor.undo();
    assert!(!editor.selected_layer().unwrap().locked());
}

#[test]
fn invalid_commands_are_atomic() {
    let mut editor = editor_with_layer();
    let before = editor.project().clone();
    for (property, frame, value) in [
        (Property::Opacity, 0, 101.0),
        (Property::PositionX, 150, 0.0),
        (Property::PositionX, 0, f64::NAN),
        (Property::Rotation, 0, f64::INFINITY),
    ] {
        assert!(
            editor
                .execute(Command::SetValue {
                    id: 1,
                    property,
                    frame,
                    value
                })
                .is_err()
        );
        assert_eq!(editor.project(), &before);
    }
    assert!(
        editor
            .execute(Command::MoveLayer { id: 1, index: 10 })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
}

#[test]
fn serialized_project_roundtrips_animation_and_preserves_ids() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 60, 1200.0);
    let loaded = Project::from_json(&editor.project().to_json().unwrap()).unwrap();
    assert_eq!(&loaded, editor.project());
    editor.replace_project(loaded).unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    assert_eq!(editor.selected(), Some(2));
}

#[test]
fn corrupt_and_future_projects_are_rejected() {
    let editor = editor_with_layer();
    let mut project = editor.project().clone();
    project.version = 4;
    assert!(Project::from_json(&project.to_json().unwrap()).is_err());
    project.version = 1;
    project.composition.fps = 0;
    assert!(Project::from_json(&project.to_json().unwrap()).is_err());
    project.composition.fps = 30;
    project
        .composition
        .layers
        .push(project.composition.layers[0].clone());
    assert!(Project::from_json(&project.to_json().unwrap()).is_err());
    project.composition.layers.pop();
    project.composition.layers[0]
        .properties
        .remove(&Property::AnchorX);
    assert!(Project::from_json(&project.to_json().unwrap()).is_err());
}

#[test]
fn replacing_project_can_be_undone() {
    let mut editor = editor_with_layer();
    let previous = editor.project().clone();
    editor.replace_project(Project::default()).unwrap();
    assert!(editor.project().composition().layers().is_empty());
    editor.undo();
    assert_eq!(editor.project(), &previous);
    assert_eq!(editor.selected(), Some(1));
}

#[test]
fn document_boundary_clears_history_without_changing_loaded_project() {
    let mut editor = editor_with_layer();
    editor.replace_project(Project::default()).unwrap();
    editor.clear_history();
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
    editor.undo();
    assert!(editor.project().composition().layers().is_empty());
    editor.execute(Command::AddRectangle).unwrap();
    editor.undo();
    assert!(editor.project().composition().layers().is_empty());
}

#[test]
fn transform_applies_anchor_then_scale_then_rotation_then_position() {
    let mut editor = editor_with_layer();
    editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::Rotation,
            frame: 0,
            value: 90.0,
        })
        .unwrap();
    let corners = editor.selected_layer().unwrap().corners_at(0);
    assert!((corners[0][0] - 1060.0).abs() < 1e-9);
    assert!((corners[0][1] - 380.0).abs() < 1e-9);
}

#[test]
fn invalid_project_replacement_preserves_current_work_and_history() {
    let mut editor = editor_with_layer();
    let previous = editor.project().clone();
    let mut invalid = Project::default();
    invalid.composition.duration = 0;
    assert!(editor.replace_project(invalid).is_err());
    assert_eq!(editor.project(), &previous);
    editor.undo();
    assert!(editor.project().composition().layers().is_empty());
}
