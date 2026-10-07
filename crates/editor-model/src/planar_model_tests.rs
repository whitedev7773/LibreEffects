//! Synthetic planar source/geometry/transaction boundaries, without reference data.
use libre_effects_core::*;
use std::collections::BTreeMap;

fn track() -> SpatialPosition2 {
    SpatialPosition2 {
        value: None,
        keys: BTreeMap::from([
            (0, SpatialKey2::new([20., 30.])),
            (60, SpatialKey2::new([80., 30.])),
        ]),
    }
}
fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureCompositionRate {
        name: "Planar native".into(),
        width: 640,
        height: 360,
        fps: 60.into(),
        duration: 300,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 0,
        end: 120,
    })
    .unwrap();
    for property in [Property::AnchorX, Property::AnchorY] {
        e.execute(Command::SetValue {
            id: 1,
            property,
            frame: 0,
            value: 0.,
        })
        .unwrap();
    }
    e.clear_history();
    e
}
fn reject(e: &mut Editor, command: Command) {
    let before = e.project().clone();
    let history = (e.can_undo(), e.can_redo(), e.context_generation());
    assert!(e.execute(command).is_err());
    assert_eq!(e.project(), &before);
    assert_eq!(
        (e.can_undo(), e.can_redo(), e.context_generation()),
        history
    );
}
#[test]
fn planar_source_roundtrips_without_static_base_scalar_shadow_or_camera() {
    let mut e = scene();
    let legacy = e.project().to_json().unwrap();
    assert!(!legacy.contains("planar_position"));
    e.execute(Command::SetPlanarPosition {
        id: 1,
        position: track(),
    })
    .unwrap();
    let l = e.project().composition().layer(1).unwrap();
    assert!(!l.is_three_d());
    assert!(l.property(Property::PositionX).is_none());
    assert!(l.property(Property::PositionY).is_none());
    assert!(e.project().composition().camera().is_none());
    assert_eq!(l.position2_at(30, 1. / 60.).unwrap(), [50., 30.]);
    let json = e.project().to_json().unwrap();
    let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(raw["version"], 75);
    assert!(
        raw["composition"]["layers"][0]["planar_position"]
            .get("value")
            .is_none()
    );
    assert!(
        raw["composition"]["layers"][0]
            .get("spatial_position")
            .is_none()
    );
    assert_eq!(Project::from_json(&json).unwrap(), *e.project());
    let bytes = project_file::encode(e.project(), None).unwrap();
    assert_eq!(
        project_file::encode(&project_file::decode(&bytes).unwrap().project, None).unwrap(),
        bytes
    );
    let mut wrong = raw;
    wrong["version"] = 74.into();
    assert!(
        Project::from_json(&wrong.to_string())
            .unwrap_err()
            .contains("version 75")
    );
    e.undo();
    assert_eq!(e.project().to_json().unwrap(), legacy);
    e.redo();
    assert_eq!(e.project().to_json().unwrap(), json);
}
#[test]
fn planar_parent_uses_fps_and_local_child_source_even_when_parent_is_hidden() {
    let mut e = scene();
    e.execute(Command::SetPlanarPosition {
        id: 1,
        position: track(),
    })
    .unwrap();
    e.execute(Command::SetValue {
        id: 1,
        property: Property::Opacity,
        frame: 0,
        value: 0.,
    })
    .unwrap();
    e.execute(Command::ToggleVisible(1)).unwrap();
    e.execute(Command::AddRectangle).unwrap();
    for (property, value) in [
        (Property::PositionX, 5.),
        (Property::PositionY, 7.),
        (Property::AnchorX, 0.),
        (Property::AnchorY, 0.),
    ] {
        e.execute(Command::SetValue {
            id: 2,
            property,
            frame: 0,
            value,
        })
        .unwrap();
    }
    e.execute(Command::SetPlanarParent {
        id: 2,
        parent: Some(1),
    })
    .unwrap();
    let c = e.project().composition();
    assert_eq!(
        c.world_transform(2, 30).unwrap().point([0., 0.]),
        [55., 37.]
    );
    assert!(c.layer_active(c.layer(2).unwrap(), 30, false));
    assert_eq!(c.layer(2).unwrap().opacity_at(30, 1. / 60.).unwrap(), 100.);
    e.execute(Command::DuplicateLayer(2)).unwrap();
    let id = e.selected().unwrap();
    assert_eq!(
        e.project().composition().layer(id).unwrap().parent(),
        Some(1)
    );
    assert_eq!(
        e.project()
            .composition()
            .world_transform(id, 30)
            .unwrap()
            .point([0., 0.]),
        [55., 37.]
    );
    reject(
        &mut e,
        Command::SetPlanarParent {
            id: 1,
            parent: Some(2),
        },
    );
}
#[test]
fn planar_shift_preserves_absence_and_exact_metadata_and_rolls_back_boundaries() {
    let mut e = scene();
    let mut p = track();
    p.keys.get_mut(&0).unwrap().in_tangent = [-0., 1e-199];
    p.keys.get_mut(&60).unwrap().out_ease = SpatialEase {
        speed: 1e-199,
        influence: 17.25,
    };
    e.execute(Command::SetPlanarPosition {
        id: 1,
        position: p.clone(),
    })
    .unwrap();
    e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
    let l = e.project().composition().layer(1).unwrap();
    assert_eq!(l.start_frame(), 10);
    let shifted = l.planar_position().unwrap();
    assert_eq!(shifted.value, None);
    assert_eq!(shifted.keys[&10], p.keys[&0]);
    assert_eq!(shifted.keys[&70], p.keys[&60]);
    assert_eq!(
        shifted.keys[&10].in_tangent[0].to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(l.position2_at(40, 1. / 60.).unwrap(), [50., 30.]);
    reject(&mut e, Command::ShiftLayer { id: 1, delta: -11 });
    reject(
        &mut e,
        Command::ConfigureCompositionRate {
            name: "Changed".into(),
            width: 640,
            height: 360,
            fps: 30.into(),
            duration: 300,
            display_start: 0,
        },
    );
}
#[test]
fn scalar_edits_dimension_change_and_final_key_removal_fail_atomically() {
    let mut e = scene();
    e.execute(Command::SetPlanarPosition {
        id: 1,
        position: track(),
    })
    .unwrap();
    reject(
        &mut e,
        Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame: 0,
            value: 10.,
        },
    );
    reject(
        &mut e,
        Command::SetThreeD {
            id: 1,
            enabled: true,
        },
    );
    reject(
        &mut e,
        Command::EditPlanarPosition {
            id: 1,
            edit: PlanarEdit::TemporalAutoBezier {
                frame: 0,
                value: true,
            },
        },
    );
    e.execute(Command::EditPlanarPosition {
        id: 1,
        edit: PlanarEdit::RemoveKey { frame: 60 },
    })
    .unwrap();
    reject(
        &mut e,
        Command::EditPlanarPosition {
            id: 1,
            edit: PlanarEdit::RemoveKey { frame: 0 },
        },
    );
    reject(
        &mut e,
        Command::SetPlanarPosition {
            id: 1,
            position: SpatialPosition2 {
                value: None,
                keys: BTreeMap::new(),
            },
        },
    );
}
#[test]
fn enabled_expression_samples_joined_value_then_materializes_only_transient_view() {
    use expression_runtime as ae;
    let mut e = scene();
    e.execute(Command::SetPlanarPosition {
        id: 1,
        position: track(),
    })
    .unwrap();
    e.execute(Command::SetExpression {
        id: 1,
        target: ExpressionTarget::Position,
        source: "[value[0] + 5, value[1] + 2]".into(),
        enabled: true,
    })
    .unwrap();
    let comp = e.project().active_composition_id();
    let snapshot = e.project().expression_snapshot(comp, 30).unwrap();
    let roots = e.project().expression_roots(comp, 30, false).unwrap();
    let values = ae::ExpressionEvaluator::default()
        .evaluate(&snapshot, &roots)
        .unwrap();
    let view = e
        .project()
        .with_evaluated_properties(comp, 30, false, &values)
        .unwrap();
    assert_eq!(
        view.composition()
            .world_transform(1, 30)
            .unwrap()
            .point([0., 0.]),
        [55., 32.]
    );
    assert!(
        view.composition()
            .layer(1)
            .unwrap()
            .planar_position()
            .is_none()
    );
    assert!(
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .planar_position()
            .is_some()
    );
    assert!(view.to_json().is_err());
}
