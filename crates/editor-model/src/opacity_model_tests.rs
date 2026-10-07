//! Independently authored schema73 source/transaction and raw sampling cases.
use libre_effects_core::*;
use serde_json::{Value, json};

pub(super) fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureCompositionRate {
        name: "Native opacity".into(),
        width: 640,
        height: 360,
        fps: 30.into(),
        duration: 300,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 0,
        end: 180,
    })
    .unwrap();
    e.clear_history();
    e
}
pub(super) fn edit(e: &mut Editor, edit: OpacityEdit) {
    e.execute(Command::SetOpacityTiming { id: 1, edit })
        .unwrap();
}
pub(super) fn excursion(e: &mut Editor, high: bool) {
    let value = if high { 100. } else { 0. };
    let speed = if high { 200. } else { -200. };
    for frame in [0, 30] {
        edit(e, OpacityEdit::Key { frame, value });
    }
    for frame in [0, 30] {
        edit(
            e,
            OpacityEdit::Interpolation {
                frame,
                incoming: OpacityInterpolation::Bezier,
                outgoing: OpacityInterpolation::Bezier,
            },
        );
        edit(
            e,
            OpacityEdit::TemporalEase {
                frame,
                incoming: OpacityEase {
                    speed: if frame == 0 {
                        -f64::from_bits(1)
                    } else {
                        -speed
                    },
                    influence: 100. / 3.,
                },
                outgoing: OpacityEase {
                    speed: if frame == 30 { -1e-199 } else { speed },
                    influence: 100. / 3.,
                },
            },
        );
    }
}
fn reject(e: &mut Editor, cmd: Command) {
    let before = e.project().clone();
    let receipt = (e.can_undo(), e.can_redo(), e.context_generation());
    assert!(e.execute(cmd).is_err());
    assert_eq!(e.project(), &before);
    assert_eq!(
        (e.can_undo(), e.can_redo(), e.context_generation()),
        receipt
    );
}
#[test]
fn raw_overshoot_has_one_value_source_and_exact_native_metadata_roundtrip() {
    for high in [false, true] {
        let mut e = scene();
        excursion(&mut e, high);
        let layer = e.project().composition().layer(1).unwrap();
        assert!(
            (layer.opacity_at(15, 1. / 30.).unwrap() - if high { 150. } else { -50. }).abs() < 1e-9
        );
        assert!(layer.property(Property::Opacity).is_none());
        assert!(layer.track(Property::Opacity.into()).is_none());
        assert!(!layer.track_paths().contains(&Property::Opacity.into()));
        let raw: Value = serde_json::from_str(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(raw["version"], 73);
        let record = &raw["composition"]["layers"][0];
        assert_eq!(
            record["properties"]["Opacity"]["keys"]["0"]["interpolation"],
            "Linear"
        );
        assert!(
            record["properties"]["Opacity"]["keys"]["0"]
                .get("temporal")
                .is_none()
        );
        assert_eq!(
            record["opacity_timing"]["keys"].as_object().unwrap().len(),
            2
        );
        for decoded in [
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            project_file::decode(&project_file::encode(e.project(), None).unwrap())
                .unwrap()
                .project,
        ] {
            let timing = decoded
                .composition()
                .layer(1)
                .unwrap()
                .opacity_timing()
                .unwrap();
            assert_eq!(
                timing.keys()[&0].in_ease.speed.to_bits(),
                (-f64::from_bits(1)).to_bits()
            );
            assert_eq!(
                timing.keys()[&30].out_ease.speed.to_bits(),
                (-1e-199f64).to_bits()
            );
            assert_eq!(
                project_file::encode(&decoded, None).unwrap(),
                project_file::encode(e.project(), None).unwrap()
            );
        }
    }
}
#[test]
fn value_updates_mode_changes_and_dormant_sides_do_not_rewrite_neighbor_metadata() {
    let mut e = scene();
    excursion(&mut e, false);
    let original = e
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .opacity_timing()
        .unwrap()
        .clone();
    edit(
        &mut e,
        OpacityEdit::Key {
            frame: 30,
            value: 30.,
        },
    );
    assert_eq!(
        e.project().composition().layer(1).unwrap().opacity_timing(),
        Some(&original)
    );
    edit(
        &mut e,
        OpacityEdit::Interpolation {
            frame: 0,
            incoming: OpacityInterpolation::Hold,
            outgoing: OpacityInterpolation::Hold,
        },
    );
    assert_eq!(
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .opacity_at(15, 1. / 30.)
            .unwrap(),
        0.
    );
    let layer = e.project().composition().layer(1).unwrap();
    assert_eq!(
        layer.opacity_timing().unwrap().keys()[&30],
        original.keys()[&30]
    );
    assert_eq!(
        layer.opacity_timing().unwrap().keys()[&0].in_ease,
        original.keys()[&0].in_ease
    );
    edit(
        &mut e,
        OpacityEdit::Interpolation {
            frame: 0,
            incoming: OpacityInterpolation::Bezier,
            outgoing: OpacityInterpolation::Bezier,
        },
    );
    assert_eq!(
        e.project().composition().layer(1).unwrap().opacity_timing(),
        Some(&original)
    );
    e.execute(Command::SetValue {
        id: 1,
        property: Property::Opacity,
        frame: 15,
        value: 20.,
    })
    .unwrap();
    let layer = e.project().composition().layer(1).unwrap();
    assert_eq!(layer.opacity_key_count(), 3);
    assert_eq!(layer.opacity_key_value(15), Some(20.));
    assert_eq!(
        layer.opacity_timing().unwrap().keys()[&0],
        original.keys()[&0]
    );
    assert_eq!(
        layer.opacity_timing().unwrap().keys()[&30],
        original.keys()[&30]
    );
}
#[test]
fn shifts_trim_copy_and_history_keep_every_timing_bit() {
    let mut e = scene();
    excursion(&mut e, false);
    let original = e.project().clone();
    let metadata = e
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .opacity_timing()
        .unwrap()
        .clone();
    e.clear_history();
    e.execute(Command::SetLayerStart { id: 1, frame: 5 })
        .unwrap();
    let layer = e.project().composition().layer(1).unwrap();
    for (frame, key) in metadata.keys() {
        assert_eq!(&layer.opacity_timing().unwrap().keys()[&(frame + 5)], key);
    }
    let shifted = e.project().clone();
    e.undo();
    assert_eq!(e.project(), &original);
    e.redo();
    assert_eq!(e.project(), &shifted);
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 10,
        end: 120,
    })
    .unwrap();
    let layer = e.project().composition().layer(1).unwrap();
    assert_eq!(layer.opacity_key_value(5), Some(0.));
    let keys = layer.opacity_timing().unwrap().clone();
    e.execute(Command::DuplicateLayer(1)).unwrap();
    assert_eq!(e.selected_layer().unwrap().opacity_timing(), Some(&keys));
    let clipboard = e.copy_layers(&[1]).unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureCompositionRate {
        name: "same fps".into(),
        width: 640,
        height: 360,
        fps: 30.into(),
        duration: 300,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::PasteLayers(clipboard.clone())).unwrap();
    assert_eq!(e.selected_layer().unwrap().opacity_timing(), Some(&keys));
    reject(
        &mut e,
        Command::ConfigureCompositionRate {
            name: "would retime".into(),
            width: 640,
            height: 360,
            fps: 60.into(),
            duration: 300,
            display_start: 0,
        },
    );
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureCompositionRate {
        name: "different fps".into(),
        width: 640,
        height: 360,
        fps: 60.into(),
        duration: 300,
        display_start: 0,
    })
    .unwrap();
    reject(&mut e, Command::PasteLayers(clipboard));
}
#[test]
fn legacy_curves_remain_exact_and_unsupported_promotion_is_atomic() {
    for mode in [
        Interpolation::Hold,
        Interpolation::Smooth,
        Interpolation::Bezier(Bezier::default()),
    ] {
        let mut e = scene();
        for (frame, value) in [(0, 20.), (30, 80.)] {
            edit(&mut e, OpacityEdit::Key { frame, value });
        }
        e.execute(Command::SetInterpolation {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            interpolation: mode,
        })
        .unwrap();
        let layer = e.project().composition().layer(1).unwrap();
        for frame in 0..31 {
            assert_eq!(
                layer.opacity_at(frame, 1. / 30.).unwrap().to_bits(),
                layer
                    .property(Property::Opacity)
                    .unwrap()
                    .value_at(frame)
                    .to_bits()
            );
        }
        let bytes = project_file::encode(e.project(), None).unwrap();
        assert_eq!(
            project_file::encode(&project_file::decode(&bytes).unwrap().project, None).unwrap(),
            bytes
        );
        reject(
            &mut e,
            Command::SetOpacityTiming {
                id: 1,
                edit: OpacityEdit::TemporalEase {
                    frame: 0,
                    incoming: OpacityEase::default(),
                    outgoing: OpacityEase::default(),
                },
            },
        );
        assert_eq!(project_file::encode(e.project(), None).unwrap(), bytes);
    }
}
#[test]
fn linear_and_false_noops_do_not_promote_source_or_discard_redo() {
    let mut e = scene();
    for frame in [0, 30] {
        edit(&mut e, OpacityEdit::Key { frame, value: 20. });
    }
    let mut raw = serde_json::to_value(e.project()).unwrap();
    raw["version"] = 63.into();
    let source = Project::from_json(&raw.to_string()).unwrap();
    e.replace_project(source).unwrap();
    e.clear_history();
    e.execute(Command::RenameLayer {
        id: 1,
        name: "temporary".into(),
    })
    .unwrap();
    e.undo();
    let original = e.project().clone();
    let receipt = e.context_generation();
    for edit in [
        OpacityEdit::Interpolation {
            frame: 0,
            incoming: OpacityInterpolation::Linear,
            outgoing: OpacityInterpolation::Linear,
        },
        OpacityEdit::TemporalContinuous {
            frame: 0,
            value: false,
        },
        OpacityEdit::TemporalAutoBezier {
            frame: 0,
            value: false,
        },
    ] {
        e.execute(Command::SetOpacityTiming { id: 1, edit })
            .unwrap();
    }
    assert_eq!(e.project(), &original);
    assert_eq!(e.context_generation(), receipt);
    assert!(e.can_redo());
    assert!(
        !e.project()
            .composition()
            .layer(1)
            .unwrap()
            .has_opacity_timing()
    );
}
#[test]
fn invalid_coverage_schema_or_shadow_timing_is_never_admitted() {
    let mut e = scene();
    excursion(&mut e, false);
    let raw = serde_json::to_value(e.project()).unwrap();
    let mut cases = vec![];
    let mut bad = raw.clone();
    bad["version"] = 72.into();
    cases.push(bad);
    let mut bad = raw.clone();
    bad["composition"]["layers"][0]["opacity_timing"]["keys"]
        .as_object_mut()
        .unwrap()
        .remove("30");
    cases.push(bad);
    let mut bad = raw.clone();
    bad["composition"]["layers"][0]["properties"]["Opacity"]["keys"]["0"]["interpolation"] =
        json!("Hold");
    cases.push(bad);
    let mut bad = raw.clone();
    bad["composition"]["layers"][0]["opacity_timing"]["unrecognized"] = true.into();
    cases.push(bad);
    let mut bad = raw.clone();
    bad["composition"]["layers"][0]["opacity_timing"]["keys"]["0"]["temporal_auto_bezier"] =
        true.into();
    cases.push(bad);
    for bad in cases {
        assert!(Project::from_json(&bad.to_string()).is_err());
    }
}
#[test]
fn generic_key_paths_and_caught_sampling_failures_preserve_source_history() {
    let mut e = scene();
    excursion(&mut e, false);
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::Rotation,
        frame: 0,
    })
    .unwrap();
    let key = KeyRef {
        id: 1,
        property: Property::Opacity.into(),
        frame: 0,
    };
    let other = KeyRef {
        id: 1,
        property: Property::Rotation.into(),
        frame: 0,
    };
    for command in [
        Command::MoveKeys {
            keys: vec![other, key],
            delta: 5,
        },
        Command::DeleteKeys(vec![other, key]),
        Command::ToggleKeyframe {
            id: 1,
            property: Property::Opacity,
            frame: 0,
        },
        Command::SetInterpolation {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            interpolation: Interpolation::Hold,
        },
        Command::SetTemporalHandle {
            id: 1,
            property: Property::Opacity.into(),
            frame: 0,
            incoming: false,
            handle: TemporalHandle {
                slope: 1.,
                influence: 0.5,
            },
        },
        Command::ScaleKeys {
            keys: vec![other, key],
            scale: KeyScale {
                time_origin: 0.,
                time_scale: 2.,
                value_origin: 0.,
                value_scale: 1.,
            },
        },
    ] {
        reject(&mut e, command);
    }
    let before = e.project().clone();
    let mut candidate = before.clone();
    candidate
        .apply_automation_command(
            1,
            Command::SetOpacityTiming {
                id: 1,
                edit: OpacityEdit::Interpolation {
                    frame: 30,
                    incoming: OpacityInterpolation::Hold,
                    outgoing: OpacityInterpolation::Linear,
                },
            },
        )
        .unwrap();
    assert!(e.commit_automation_project(candidate).is_err());
    assert_eq!(e.project(), &before);
}
#[test]
fn identity_expression_consumes_raw_opacity_and_clears_only_ephemeral_timing() {
    use libre_effects_core::expression_runtime as ae;
    let mut e = scene();
    excursion(&mut e, false);
    e.execute(Command::SetExpression {
        id: 1,
        target: ExpressionTarget::Opacity,
        source: "value".into(),
        enabled: true,
    })
    .unwrap();
    let source = e.project().clone();
    let snapshot = source.expression_snapshot(1, 15).unwrap();
    let ae::PropertyValue::Scalar(raw) = snapshot.layers[0].opacity.authored_value else {
        panic!("scalar source")
    };
    assert!((raw + 50.).abs() < 1e-9);
    let roots = source.expression_roots(1, 15, false).unwrap();
    let evaluated = ae::ExpressionEvaluator::default()
        .evaluate(&snapshot, &roots)
        .unwrap();
    let view = source
        .with_evaluated_properties(1, 15, false, &evaluated)
        .unwrap();
    let layer = view.composition().layer(1).unwrap();
    assert!(!layer.has_opacity_timing());
    assert!((layer.opacity_at(15, 1. / 30.).unwrap() + 50.).abs() < 1e-9);
    assert_eq!(e.project(), &source);
    assert!(view.to_json().is_err());
    assert!(project_file::encode(&view, None).is_err());
    assert!(e.commit_automation_project(view).is_err());
    assert_eq!(e.project(), &source);
}
#[test]
fn raw_key_rotation_batches_cannot_rebind_timing_to_different_values() {
    let mut e = scene();
    excursion(&mut e, false);
    edit(
        &mut e,
        OpacityEdit::Key {
            frame: 30,
            value: 80.,
        },
    );
    reject(
        &mut e,
        Command::Batch(vec![
            Command::MoveKeyframe {
                id: 1,
                property: Property::Opacity,
                from: 0,
                to: 1,
            },
            Command::MoveKeyframe {
                id: 1,
                property: Property::Opacity,
                from: 30,
                to: 0,
            },
            Command::MoveKeyframe {
                id: 1,
                property: Property::Opacity,
                from: 1,
                to: 30,
            },
        ]),
    );
    reject(
        &mut e,
        Command::EditKeyframe {
            id: 1,
            property: Property::Opacity,
            from: 0,
            to: 0,
            value: 50.,
        },
    );
    reject(
        &mut e,
        Command::Batch(vec![
            Command::ToggleAnimation {
                id: 1,
                property: Property::Opacity,
                frame: 15,
            },
            Command::ToggleKeyframe {
                id: 1,
                property: Property::Opacity,
                frame: 0,
            },
            Command::ToggleKeyframe {
                id: 1,
                property: Property::Opacity,
                frame: 30,
            },
        ]),
    );
}
