//! Public native-source/geometry regressions; no original program data.
use libre_effects_core::*;
use serde_json::Value;

pub(super) fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureCompositionRate {
        name: "Native plane".into(),
        width: 640,
        height: 360,
        fps: 30.into(),
        duration: 600,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetCamera {
        camera: Some(Camera3 {
            position: [320., 180., -600.],
            focal_distance: 600.,
            principal_point: [320., 180.],
            near_clip: 1.,
        }),
    })
    .unwrap();
    e.execute(Command::SetThreeD {
        id: 1,
        enabled: true,
    })
    .unwrap();
    e.clear_history();
    e
}
fn edit(e: &mut Editor, edit: SpatialEdit) {
    e.execute(Command::SetSpatialPosition { id: 1, edit })
        .unwrap();
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
fn curve(e: &mut Editor) {
    for (frame, value) in [
        (0, [180., 100., 0.]),
        (30, [160., 100., 60.]),
        (120, [160., 100., 60.]),
        (150, [180., 100., 0.]),
    ] {
        edit(e, SpatialEdit::Key { frame, value });
    }
    for frame in [0, 30, 120, 150] {
        edit(
            e,
            SpatialEdit::Interpolation {
                frame,
                incoming: SpatialInterpolation::Bezier,
                outgoing: SpatialInterpolation::Bezier,
            },
        );
        edit(
            e,
            SpatialEdit::TemporalEase {
                frame,
                incoming: SpatialEase {
                    speed: if frame == 30 { 1e-9 } else { 0. },
                    influence: 65.,
                },
                outgoing: SpatialEase {
                    speed: 0.,
                    influence: 35.,
                },
            },
        );
        let (incoming, outgoing) = match frame {
            30 => ([0.5, 0., 0.], [0.; 3]),
            120 => ([-0.5, 0., 0.], [0.5, 0., 0.]),
            _ => ([0.; 3], [0.; 3]),
        };
        edit(
            e,
            SpatialEdit::Tangents {
                frame,
                incoming,
                outgoing,
            },
        );
        edit(e, SpatialEdit::SpatialContinuous { frame, value: true });
    }
}
#[test]
fn joined_source_has_no_scalar_shadow_and_exact_native_roundtrip() {
    let mut e = scene();
    curve(&mut e);
    edit(
        &mut e,
        SpatialEdit::Tangents {
            frame: 0,
            incoming: [1e-202, 2e-202, -3e-202],
            outgoing: [0.; 3],
        },
    );
    edit(
        &mut e,
        SpatialEdit::TemporalEase {
            frame: 150,
            incoming: SpatialEase {
                speed: 0.,
                influence: 65.,
            },
            outgoing: SpatialEase {
                speed: 1e-199,
                influence: 17.25,
            },
        },
    );
    let layer = e.project().composition().layer(1).unwrap();
    assert!(layer.property(Property::PositionX).is_none());
    assert!(layer.property(Property::PositionY).is_none());
    assert_eq!(layer.spatial_position().unwrap().keys.len(), 4);
    assert_eq!(layer.position3_at(30, 1. / 30.).unwrap(), [160., 100., 60.]);
    let middle = layer.position3_at(75, 1. / 30.).unwrap();
    assert!(middle[0] < 160. && middle[0] > 159.7);
    assert_eq!(middle[2], 60.);
    e.project().validate_spatial_animation().unwrap();
    let raw: Value = serde_json::from_str(&e.project().to_json().unwrap()).unwrap();
    assert_eq!(raw["version"], 72);
    assert!(
        raw["composition"]["layers"][0]["properties"]
            .get("PositionX")
            .is_none()
    );
    let bytes = project_file::encode(e.project(), None).unwrap();
    let decoded = project_file::decode(&bytes).unwrap();
    assert_eq!(project_file::encode(&decoded.project, None).unwrap(), bytes);
    assert_eq!(&decoded.project, e.project());
    assert_eq!(
        decoded
            .project
            .composition()
            .layer(1)
            .unwrap()
            .spatial_position()
            .unwrap()
            .keys[&0]
            .in_tangent[0]
            .to_bits(),
        1e-202f64.to_bits()
    );
    let mut invalid = raw.clone();
    invalid["version"] = 71.into();
    assert!(Project::from_json(&invalid.to_string()).is_err());
    let key = invalid["composition"]["layers"][0]["spatial_position"]["keys"]["0"].clone();
    invalid["version"] = 72.into();
    invalid["composition"]["layers"][0]["spatial_position"]["keys"]["00"] = key;
    // The outer project decoder also rejects noncanonical frame strings;
    // the pure spatial crate separately verifies duplicate numeric map keys.
    assert!(Project::from_json(&invalid.to_string()).is_err());
}
#[test]
fn same_time_replacement_duplicate_trim_shift_and_undo_preserve_metadata() {
    let mut e = scene();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 10,
        end: 500,
    })
    .unwrap();
    curve(&mut e);
    let old = e
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .spatial_position()
        .unwrap()
        .keys[&120]
        .clone();
    let mut expected = old.clone();
    expected.value = [160., 100., 80.];
    edit(
        &mut e,
        SpatialEdit::Key {
            frame: 120,
            value: expected.value,
        },
    );
    assert_eq!(
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .spatial_position()
            .unwrap()
            .keys[&120],
        expected
    );
    let keys = e
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .spatial_position()
        .unwrap()
        .keys
        .clone();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 50,
        end: 300,
    })
    .unwrap();
    assert_eq!(
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .spatial_position()
            .unwrap()
            .keys,
        keys
    );
    let before = e.project().clone();
    e.clear_history();
    e.execute(Command::SetLayerStart { id: 1, frame: 15 })
        .unwrap();
    let shifted = e.project().composition().layer(1).unwrap();
    assert_eq!(shifted.in_frame(), 65);
    for (frame, key) in &keys {
        assert_eq!(
            &shifted.spatial_position().unwrap().keys[&(frame + 15)],
            key
        );
    }
    let after = e.project().clone();
    e.undo();
    assert_eq!(e.project(), &before);
    e.redo();
    assert_eq!(e.project(), &after);
    e.execute(Command::DuplicateLayer(1)).unwrap();
    let copy = e.selected().unwrap();
    assert_eq!(
        e.project()
            .composition()
            .layer(copy)
            .unwrap()
            .spatial_position(),
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .spatial_position()
    );
    reject(&mut e, Command::SetLayerStart { id: 1, frame: -100 });
    reject(
        &mut e,
        Command::ConfigureCompositionRate {
            name: "Native plane".into(),
            width: 640,
            height: 360,
            fps: 60.into(),
            duration: 600,
            display_start: 0,
        },
    );
}
#[test]
fn scalar_bypasses_and_mixed_key_operations_reject_atomically() {
    let mut e = scene();
    edit(
        &mut e,
        SpatialEdit::Key {
            frame: 30,
            value: [10., 20., 30.],
        },
    );
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::Opacity,
        frame: 30,
    })
    .unwrap();
    let joined = KeyRef {
        id: 1,
        property: Property::PositionX.into(),
        frame: 30,
    };
    let scalar = KeyRef {
        id: 1,
        property: Property::Opacity.into(),
        frame: 30,
    };
    reject(
        &mut e,
        Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame: 30,
            value: 99.,
        },
    );
    reject(
        &mut e,
        Command::MoveKeys {
            keys: vec![scalar, joined],
            delta: 5,
        },
    );
    reject(&mut e, Command::DeleteKeys(vec![scalar, joined]));
    reject(
        &mut e,
        Command::SetPosition {
            id: 1,
            frame: 30,
            x: 9.,
            y: 8.,
        },
    );
    reject(
        &mut e,
        Command::SetThreeD {
            id: 1,
            enabled: false,
        },
    );
    reject(
        &mut e,
        Command::SetSpatialPosition {
            id: 1,
            edit: SpatialEdit::SpatialAutoBezier {
                frame: 30,
                value: true,
            },
        },
    );
}
#[test]
fn camera_projection_parenting_depth_ties_and_visible_failures_are_explicit() {
    let mut e = scene();
    edit(&mut e, SpatialEdit::Value([320., 180., 600.]));
    let geometry = e.project().composition().projected_geometry(1, 0).unwrap();
    assert_eq!(geometry.depth, Some(1200.));
    assert_eq!(geometry.transform.0, [0.5, 0., 0., 0.5, 240., 130.]);
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetThreeD {
        id: 2,
        enabled: true,
    })
    .unwrap();
    assert_eq!(
        e.project().composition().render_order(0, false).unwrap(),
        vec![1, 2]
    );
    e.execute(Command::SetSpatialPosition {
        id: 2,
        edit: SpatialEdit::Value([320., 180., 600.]),
    })
    .unwrap();
    assert_eq!(
        e.project().composition().render_order(0, false).unwrap(),
        vec![1, 2]
    );
    e.execute(Command::SetSpatialParent {
        id: 2,
        parent: Some(1),
    })
    .unwrap();
    assert_eq!(
        e.project()
            .composition()
            .projected_geometry(2, 0)
            .unwrap()
            .depth,
        Some(1800.)
    );
    reject(
        &mut e,
        Command::SetSpatialParent {
            id: 1,
            parent: Some(2),
        },
    );
    reject(&mut e, Command::RemoveLayer(1));
    e.execute(Command::SetCamera { camera: None }).unwrap();
    assert!(
        e.project()
            .composition()
            .projected_geometry(1, 0)
            .unwrap_err()
            .contains("explicit native camera")
    );
    let mut near = scene();
    edit(&mut near, SpatialEdit::Value([320., 180., -600.]));
    assert!(
        near.project()
            .composition()
            .projected_geometry(1, 0)
            .unwrap_err()
            .contains("near plane")
    );
    let mut mixed = scene();
    mixed.execute(Command::AddRectangle).unwrap();
    assert!(
        mixed
            .project()
            .composition()
            .render_order(0, false)
            .unwrap_err()
            .contains("Mixed visible")
    );
}
#[test]
fn planar_conversion_noops_keep_redo_and_legacy_projects_unchanged() {
    let mut e = scene();
    e.execute(Command::RenameLayer {
        id: 1,
        name: "Temporary".into(),
    })
    .unwrap();
    e.undo();
    let source = e.project().clone();
    e.execute(Command::SetThreeD {
        id: 1,
        enabled: true,
    })
    .unwrap();
    assert_eq!(e.project(), &source);
    assert!(e.can_redo());
    e.execute(Command::SetThreeD {
        id: 1,
        enabled: false,
    })
    .unwrap();
    assert!(
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .property(Property::PositionX)
            .is_some()
    );
    let mut old = Editor::default();
    old.execute(Command::AddRectangle).unwrap();
    let bytes = project_file::encode(old.project(), None).unwrap();
    assert_eq!(
        project_file::encode(&project_file::decode(&bytes).unwrap().project, None).unwrap(),
        bytes
    );
}
#[test]
fn clipboard_trim_and_scalar_geometry_guards_preserve_joined_source() {
    let mut e = scene();
    curve(&mut e);
    let track = e
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .spatial_position()
        .unwrap()
        .clone();
    let clipboard = e.copy_layers(&[1]).unwrap();
    e.execute(Command::PasteLayers(clipboard.clone())).unwrap();
    assert_eq!(e.selected_layer().unwrap().spatial_position(), Some(&track));
    let id = e.selected().unwrap();
    reject(
        &mut e,
        Command::NudgeLayers {
            ids: vec![1, id],
            frame: 75,
            delta: [2., 3.],
        },
    );
    reject(
        &mut e,
        Command::SetAnchor {
            id,
            frame: 75,
            x: 4.,
            y: 5.,
        },
    );
    e.execute(Command::TrimLayers {
        ids: vec![id],
        frame: 75,
        start: true,
    })
    .unwrap();
    assert_eq!(
        e.project()
            .composition()
            .layer(id)
            .unwrap()
            .spatial_position(),
        Some(&track)
    );
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureCompositionRate {
        name: "Different FPS".into(),
        width: 640,
        height: 360,
        fps: 60.into(),
        duration: 600,
        display_start: 0,
    })
    .unwrap();
    reject(&mut e, Command::PasteLayers(clipboard.clone()));
    e.execute(Command::ConfigureCompositionRate {
        name: "Too short".into(),
        width: 640,
        height: 360,
        fps: 30.into(),
        duration: 100,
        display_start: 0,
    })
    .unwrap();
    reject(&mut e, Command::PasteLayers(clipboard));
}
#[test]
fn malformed_spatial_source_and_scalar_scale_bypasses_are_rejected() {
    let mut e = scene();
    edit(
        &mut e,
        SpatialEdit::Key {
            frame: 30,
            value: [1., 2., 3.],
        },
    );
    let raw: Value = serde_json::from_str(&e.project().to_json().unwrap()).unwrap();
    for field in ["unexpected", "rotation_x"] {
        let mut bad = raw.clone();
        bad["composition"]["camera"][field] = 1.into();
        assert!(Project::from_json(&bad.to_string()).is_err());
    }
    let mut shadow = raw.clone();
    shadow["composition"]["layers"][0]["properties"]["PositionX"] =
        serde_json::json!({"value":0,"keys":{}});
    assert!(Project::from_json(&shadow.to_string()).is_err());
    let keys = vec![KeyRef {
        id: 1,
        property: Property::PositionX.into(),
        frame: 30,
    }];
    reject(
        &mut e,
        Command::ScaleKeys {
            keys: keys.clone(),
            scale: KeyScale {
                time_origin: 0.,
                time_scale: 2.,
                value_origin: 0.,
                value_scale: 1.,
            },
        },
    );
    reject(
        &mut e,
        Command::ScaleKeyVelocities {
            keys,
            scale: KeyVelocityScale {
                origin: 0.,
                factor: 2.,
            },
        },
    );
    reject(
        &mut e,
        Command::EditTrack {
            id: 1,
            property: Property::PositionY.into(),
            edit: TrackEdit::Value {
                frame: 30,
                value: 90.,
            },
        },
    );
}
#[test]
fn nonpainting_nulls_do_not_require_a_camera_or_block_spatial_pixels() {
    let mut e = scene();
    e.execute(Command::AddNull).unwrap();
    assert_eq!(
        e.project().composition().render_order(0, false).unwrap(),
        vec![1]
    );
    let mut null = Editor::default();
    null.execute(Command::AddNull).unwrap();
    null.execute(Command::SetThreeD {
        id: 1,
        enabled: true,
    })
    .unwrap();
    assert_eq!(
        null.project().composition().render_order(0, false).unwrap(),
        Vec::<LayerId>::new()
    );
    null.execute(Command::SetSpatialPosition {
        id: 1,
        edit: SpatialEdit::Value([0., 0., -1e8]),
    })
    .unwrap();
    assert!(
        null.project()
            .composition()
            .render_order(0, false)
            .unwrap()
            .is_empty()
    );
}
