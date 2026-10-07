use super::*;

fn handle(slope: f64, influence: f64) -> TemporalHandle {
    TemporalHandle { slope, influence }
}
fn scale(origin: f64, factor: f64) -> KeyVelocityScale {
    KeyVelocityScale { origin, factor }
}
fn track() -> AnimatedProperty {
    AnimatedProperty {
        value: 123.0,
        keys: [(10, 20.0), (30, 60.0), (80, -10.0)]
            .into_iter()
            .map(|(frame, value)| {
                (
                    frame,
                    Keyframe {
                        value,
                        interpolation: Interpolation::Linear,
                        temporal: TemporalHandles::default(),
                    },
                )
            })
            .collect(),
    }
}
fn scene(track: AnimatedProperty) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddRectangle).unwrap();
    e.current.project.composition.layers[0]
        .properties
        .insert(Property::PositionX, track);
    e.current.project.version = 36;
    e.current.project.validate().unwrap();
    // Both history stacks are populated, making failed/no-op transactions observable.
    e.execute(Command::RenameLayer {
        id: 1,
        name: "Keep Redo".into(),
    })
    .unwrap();
    e.undo();
    e
}
fn current_track(e: &Editor) -> &AnimatedProperty {
    e.project()
        .composition
        .layer(1)
        .unwrap()
        .property(Property::PositionX)
        .unwrap()
}
fn keys(frames: &[Frame]) -> Vec<KeyRef> {
    frames
        .iter()
        .map(|&frame| KeyRef {
            id: 1,
            property: Property::PositionX.into(),
            frame,
        })
        .collect()
}
fn command(frames: &[Frame], transform: KeyVelocityScale) -> Command {
    Command::ScaleKeyVelocities {
        keys: keys(frames),
        scale: transform,
    }
}
fn assert_untouched(e: &mut Editor, command: Command, success: bool) {
    let before = e.current.clone();
    let json = e.project().to_json().unwrap();
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    assert_eq!(e.execute(command).is_ok(), success);
    assert_eq!(e.current, before);
    assert_eq!(e.project().to_json().unwrap(), json);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
}
fn assert_fixed(a: &AnimatedProperty, b: &AnimatedProperty) {
    assert_eq!(a.value, b.value);
    assert_eq!(
        a.keys.keys().collect::<Vec<_>>(),
        b.keys.keys().collect::<Vec<_>>()
    );
    for (&frame, original) in &a.keys {
        assert_eq!(b.keys[&frame].value, original.value);
        assert_eq!(b.keys[&frame].interpolation, original.interpolation);
    }
}
fn assert_near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-9, "{actual} != {expected}");
}

#[test]
fn velocity_scale_independent_sides_keep_asymmetry_and_ignore_dormant_endpoints() {
    let mut before = track();
    before.keys.get_mut(&10).unwrap().temporal.incoming = Some(handle(700.0, 0.9));
    before.keys.get_mut(&30).unwrap().temporal = TemporalHandles {
        mode: TemporalMode::Independent,
        incoming: Some(handle(1.25, 0.2)),
        outgoing: Some(handle(-3.0, 0.8)),
    };
    before.keys.get_mut(&80).unwrap().temporal.outgoing = Some(handle(-600.0, 0.4));
    for factor in [-2.0, 0.0, 0.25, 2.0] {
        let transform = scale(1.0, factor);
        let preview = before
            .preview_key_velocity_scale(&[80, 30, 10, 30], transform)
            .unwrap();
        assert_fixed(&before, &preview);
        assert_eq!(
            preview.keys[&10].temporal.incoming,
            before.keys[&10].temporal.incoming
        );
        assert_eq!(
            preview.keys[&80].temporal.outgoing,
            before.keys[&80].temporal.outgoing
        );
        for frame in [10, 30, 80] {
            assert_eq!(
                preview.keys[&frame].temporal.mode,
                TemporalMode::Independent
            );
            let old = before.key_velocity_handles(frame).unwrap();
            let new = preview.key_velocity_handles(frame).unwrap();
            for (index, (a, b)) in old.into_iter().zip(new).enumerate() {
                match (a, b) {
                    (Some(a), Some(b)) => {
                        assert_near(b.slope, 1.0 + factor * (a.slope - 1.0));
                        assert_eq!(b.influence, a.influence);
                        assert_near(preview.velocity(frame as f64, index == 0).unwrap(), b.slope);
                    }
                    (None, None) => {}
                    _ => panic!("A real endpoint appeared or disappeared"),
                }
            }
        }
        let mut e = scene(before.clone());
        let mut expected = e.current.clone();
        expected.project.composition.layers[0]
            .properties
            .insert(Property::PositionX, preview.clone());
        e.execute(command(&[80, 10, 30, 30], transform)).unwrap();
        assert_eq!(e.current, expected);
    }
}

#[test]
fn velocity_scale_continuous_transforms_linked_and_dormant_metadata_once() {
    let mut before = track();
    for (i, key) in before.keys.values_mut().enumerate() {
        key.temporal = TemporalHandles {
            mode: TemporalMode::Continuous,
            incoming: Some(handle(i as f64 + 2.0, 0.15)),
            outgoing: Some(handle(i as f64 + 2.0, 0.85)),
        };
    }
    let after = before
        .preview_key_velocity_scale(&[10, 30, 80], scale(1.0, -2.0))
        .unwrap();
    assert_fixed(&before, &after);
    for (&frame, key) in &after.keys {
        assert_eq!(key.temporal.mode, TemporalMode::Continuous);
        assert!(key.temporal.valid());
        for (a, b) in [
            before.keys[&frame].temporal.incoming,
            before.keys[&frame].temporal.outgoing,
        ]
        .into_iter()
        .zip([key.temporal.incoming, key.temporal.outgoing])
        {
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_eq!(b.slope, 1.0 - 2.0 * (a.slope - 1.0));
            assert_eq!(b.influence, a.influence);
        }
    }
    let mut singleton = before.clone();
    singleton.keys.retain(|&f, _| f == 10);
    assert_eq!(singleton.key_velocity_handles(10).unwrap(), [None, None]);
    assert_eq!(
        singleton
            .preview_key_velocity_scale(&[10], scale(1.0, -2.0))
            .unwrap(),
        singleton
    );
}

#[test]
fn velocity_scale_auto_freezes_original_slope_only_for_effective_edits() {
    for values in [[20.0, 60.0, 90.0], [20.0, 60.0, -10.0]] {
        let mut before = track();
        for (key, value) in before.keys.values_mut().zip(values) {
            key.value = value;
            key.temporal.mode = TemporalMode::Auto;
        }
        for factor in [-1.0, 0.0, 2.5] {
            let transform = scale(0.0, factor);
            let after = before
                .preview_key_velocity_scale(&[10, 30, 80], transform)
                .unwrap();
            assert_fixed(&before, &after);
            for frame in [10, 30, 80] {
                let old = before
                    .key_velocity_handles(frame)
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .next()
                    .unwrap();
                if old.slope == 0.0 {
                    assert_eq!(after.keys[&frame], before.keys[&frame]);
                } else {
                    let metadata = after.keys[&frame].temporal;
                    assert_eq!(metadata.mode, TemporalMode::Continuous);
                    assert!(metadata.valid());
                    for handle in [metadata.incoming, metadata.outgoing].into_iter().flatten() {
                        assert_near(handle.slope, old.slope * factor);
                        assert_eq!(handle.influence, old.influence);
                    }
                }
            }
            let mut e = scene(before.clone());
            e.execute(command(&[10, 30, 80], transform)).unwrap();
            assert_eq!(current_track(&e), &after);
            assert_eq!(e.project().version, 36);
        }
        let origin = before.key_velocity_handles(10).unwrap()[1].unwrap().slope;
        assert_eq!(
            before
                .preview_key_velocity_scale(&[10], scale(origin, f64::MAX))
                .unwrap(),
            before
        );
    }
}

#[test]
fn velocity_scale_materializes_only_changed_legacy_sides_and_keeps_tags() {
    let mut before = track();
    before.keys.get_mut(&10).unwrap().interpolation = Interpolation::Smooth;
    let after = before
        .preview_key_velocity_scale(&[30], scale(0.0, 2.0))
        .unwrap();
    assert_fixed(&before, &after);
    assert_eq!(after.keys[&10], before.keys[&10]);
    assert_eq!(after.keys[&80], before.keys[&80]);
    assert!(after.keys[&30].temporal.incoming.is_none());
    assert_near(after.keys[&30].temporal.outgoing.unwrap().slope, -2.8);
    assert_eq!(
        after.keys[&30].temporal.outgoing.unwrap().influence,
        1.0 / 3.0
    );
    let smooth = before
        .preview_key_velocity_scale(&[10], scale(2.0, 0.0))
        .unwrap();
    assert_eq!(smooth.keys[&10].temporal.outgoing.unwrap().slope, 2.0);
    assert_eq!(smooth.keys[&10].interpolation, Interpolation::Smooth);

    for key in before.keys.values_mut() {
        key.interpolation = Interpolation::Bezier(Bezier {
            x1: 0.2,
            y1: -0.3,
            x2: 0.6,
            y2: 1.2,
        });
    }
    let old = before.key_velocity_handles(30).unwrap();
    let after = before
        .preview_key_velocity_scale(&[30], scale(0.5, -1.5))
        .unwrap();
    assert_fixed(&before, &after);
    for (a, b) in old.into_iter().flatten().zip(
        after
            .key_velocity_handles(30)
            .unwrap()
            .into_iter()
            .flatten(),
    ) {
        assert_eq!(a.influence, b.influence);
        assert_near(b.slope, 0.5 - 1.5 * (a.slope - 0.5));
    }
}

#[test]
fn velocity_scale_equal_values_can_gain_velocity_without_changing_values() {
    let mut before = track();
    for key in before.keys.values_mut() {
        key.value = 50.0;
    }
    for interpolation in [
        Interpolation::Linear,
        Interpolation::Smooth,
        Interpolation::Bezier(Bezier {
            x1: 0.2,
            y1: -0.3,
            x2: 0.6,
            y2: 1.2,
        }),
    ] {
        for key in before.keys.values_mut() {
            key.interpolation = interpolation;
        }
        let after = before
            .preview_key_velocity_scale(&[10, 30, 80], scale(1.0, 2.0))
            .unwrap();
        assert_fixed(&before, &after);
        for frame in [10, 30, 80] {
            for h in after
                .key_velocity_handles(frame)
                .unwrap()
                .into_iter()
                .flatten()
            {
                assert_eq!(h.slope, -1.0);
            }
        }
    }
}

#[test]
fn velocity_scale_hold_checks_actual_segments_even_with_stored_or_auto_handles() {
    for mode in [
        TemporalMode::Independent,
        TemporalMode::Continuous,
        TemporalMode::Auto,
    ] {
        for segment_frame in [10, 30] {
            let mut before = track();
            before.keys.get_mut(&segment_frame).unwrap().interpolation = Interpolation::Hold;
            before.keys.get_mut(&30).unwrap().temporal = if mode == TemporalMode::Auto {
                TemporalHandles {
                    mode,
                    ..Default::default()
                }
            } else {
                TemporalHandles {
                    mode,
                    incoming: Some(handle(1.0, 0.2)),
                    outgoing: Some(handle(1.0, 0.7)),
                }
            };
            assert!(before.temporal_handle(30, segment_frame == 10).is_some());
            assert!(before.key_velocity_handles(30).is_err());
            let mut e = scene(before);
            for factor in [1.0, 0.0, -1.0] {
                assert_untouched(&mut e, command(&[80, 30], scale(0.0, factor)), false);
            }
        }
    }
    // A final key's Hold tag controls no segment and must not block its incoming side.
    let mut before = track();
    before.keys.get_mut(&80).unwrap().interpolation = Interpolation::Hold;
    assert!(
        before
            .preview_key_velocity_scale(&[80], scale(0.0, 2.0))
            .is_ok()
    );
    before.keys.retain(|&f, _| f == 80);
    assert_eq!(
        before
            .preview_key_velocity_scale(&[80], scale(0.0, 2.0))
            .unwrap(),
        before
    );
}

#[test]
fn velocity_scale_singular_and_unrepresentable_legacy_sides_reject_atomically() {
    for (curve, frame) in [
        (
            Bezier {
                x1: 0.0,
                y1: 0.8,
                ..Default::default()
            },
            10,
        ),
        (
            Bezier {
                x1: 0.0,
                y1: 0.0,
                ..Default::default()
            },
            10,
        ),
        (
            Bezier {
                x1: 0.0005,
                ..Default::default()
            },
            10,
        ),
        (
            Bezier {
                x2: 1.0,
                y2: 0.2,
                ..Default::default()
            },
            30,
        ),
        (
            Bezier {
                x2: 0.9995,
                ..Default::default()
            },
            30,
        ),
    ] {
        let mut before = track();
        before.keys.get_mut(&10).unwrap().interpolation = Interpolation::Bezier(curve);
        let mut e = scene(before);
        for factor in [1.0, 2.0] {
            assert_untouched(&mut e, command(&[80, frame], scale(0.0, factor)), false);
        }
    }
    let mut before = track();
    before.keys = [(0, -1e6), (1, 1e6)]
        .into_iter()
        .map(|(f, value)| {
            (
                f,
                Keyframe {
                    value,
                    interpolation: Interpolation::Bezier(Bezier {
                        x1: 0.001,
                        y1: 3.0,
                        ..Default::default()
                    }),
                    temporal: Default::default(),
                },
            )
        })
        .collect();
    let mut e = scene(before);
    assert_untouched(&mut e, command(&[0], scale(0.0, 0.0)), false);
}

#[test]
fn velocity_scale_invalid_inputs_limits_keys_paths_and_locks_keep_history() {
    let mut e = scene(track());
    for transform in [
        scale(f64::NAN, 1.0),
        scale(f64::INFINITY, 0.0),
        scale(0.0, f64::NAN),
        scale(0.0, f64::INFINITY),
        scale(0.0, f64::MAX),
        scale(f64::MAX, -1.0),
        scale(0.0, 1e10),
        scale(1e10, 0.0),
    ] {
        assert_untouched(&mut e, command(&[10, 30], transform), false);
    }
    assert_untouched(&mut e, command(&[], scale(0.0, 1.0)), false);
    assert_untouched(&mut e, command(&[10, 31], scale(0.0, 2.0)), false);
    for invalid in [
        KeyRef {
            id: 999,
            property: Property::PositionX.into(),
            frame: 10,
        },
        KeyRef {
            id: 1,
            property: PropertyPath::TimeRemap,
            frame: 10,
        },
        KeyRef {
            id: 1,
            property: PropertyPath::Path(PathTarget::Shape),
            frame: 10,
        },
    ] {
        let mut refs = keys(&[10]);
        refs.push(invalid);
        assert_untouched(
            &mut e,
            Command::ScaleKeyVelocities {
                keys: refs,
                scale: scale(0.0, 2.0),
            },
            false,
        );
    }
    e.current.project.composition.layers[0].locked = true;
    assert_untouched(&mut e, command(&[10], scale(0.0, 1.0)), false);
    assert_untouched(&mut e, command(&[10], scale(0.0, 2.0)), false);

    let mut before = track();
    before.keys.get_mut(&10).unwrap().temporal.outgoing = Some(handle(1e9, 1.0));
    assert!(
        before
            .preview_key_velocity_scale(&[10], scale(0.0, -1.0))
            .is_ok()
    );
    assert!(
        before
            .preview_key_velocity_scale(&[10], scale(0.0, 1.000001))
            .is_err()
    );
    // The highest frame can be an incoming endpoint without an unchecked +1.
    let last = before.keys.remove(&80).unwrap();
    before.keys.insert(u32::MAX, last);
    assert!(
        before
            .preview_key_velocity_scale(&[u32::MAX], scale(0.0, 2.0))
            .is_ok()
    );
}

#[test]
fn velocity_scale_noops_preserve_legacy_metadata_schema_assets_and_redo_in_nested_batches() {
    for version in [1, 2, 35, 36, PROJECT_VERSION] {
        let mut e = scene(track());
        e.current.project.version = version;
        for transform in [scale(f64::MAX, 1.0), scale(2.0, -5.0)] {
            let direct = command(&[10], transform);
            for command in [
                direct.clone(),
                Command::Batch(vec![Command::Batch(vec![direct]), Command::Batch(vec![])]),
            ] {
                assert_untouched(&mut e, command, true);
            }
        }
    }
    let mut before = track();
    for k in before.keys.values_mut() {
        k.interpolation = Interpolation::Smooth;
    }
    let mut e = scene(before);
    for (png, frame) in [("YWJj", Some(0)), ("ZGVm", None)] {
        e.execute(Command::ImportAsset {
            content: Content::Image { png: png.into() },
            width: 64.0,
            height: 48.0,
            name: "Unrelated source".into(),
            folder: None,
            frame,
        })
        .unwrap();
    }
    e.execute(Command::RenameLayer {
        id: 1,
        name: "Redo assets".into(),
    })
    .unwrap();
    e.undo();
    e.current.project.version = PROJECT_VERSION;
    e.select(1);
    assert_untouched(
        &mut e,
        Command::Batch(vec![
            command(&[10, 30, 80], scale(0.0, -7.0)),
            Command::Batch(vec![command(&[30], scale(0.0, 0.0))]),
        ]),
        true,
    );
}

#[test]
fn velocity_scale_preview_commit_one_undo_redo_minimum_schema_and_lep_roundtrip() {
    for linked in [false, true] {
        let mut before = track();
        if linked {
            before.keys.get_mut(&30).unwrap().temporal.mode = TemporalMode::Auto;
        }
        let mut e = scene(before.clone());
        e.current.project.version = if linked { 36 } else { 1 };
        let original = e.current.clone();
        let history_len = e.undo.len();
        let transform = scale(0.75, -1.5);
        let preview = before
            .preview_key_velocity_scale(&[10, 30, 80], transform)
            .unwrap();
        e.execute(Command::Batch(vec![Command::Batch(vec![command(
            &[10, 30, 80],
            transform,
        )])]))
        .unwrap();
        assert_eq!(current_track(&e), &preview);
        assert_eq!(e.project().version, if linked { 36 } else { 35 });
        assert_eq!(e.undo.len(), history_len + 1);
        assert!(e.redo.is_empty());
        let after = e.current.clone();
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
        let lep = project_file::encode(e.project(), Some(b"{\"graph\":\"speed\"}")).unwrap();
        let decoded = project_file::decode(&lep).unwrap();
        assert_eq!(decoded.project, *e.project());
        assert_eq!(decoded.view.unwrap(), b"{\"graph\":\"speed\"}");
        e.undo();
        assert_eq!(e.current, original);
        e.redo();
        assert_eq!(e.current, after);
    }
}

#[test]
fn velocity_scale_multitrack_and_nested_failures_are_atomic_and_keep_unrelated_data() {
    let mut e = scene(track());
    e.execute(Command::AddRectangle).unwrap();
    e.current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == 2)
        .unwrap()
        .properties
        .insert(Property::PositionY, track());
    let mut refs = keys(&[10, 30]);
    refs.push(KeyRef {
        id: 2,
        property: Property::PositionY.into(),
        frame: 30,
    });
    let transform = scale(2.0, -0.5);
    let original = e.current.clone();
    let mut expected = original.clone();
    for (id, property, frames) in [
        (1, Property::PositionX, vec![10, 30]),
        (2, Property::PositionY, vec![30]),
    ] {
        let layer = expected
            .project
            .composition
            .layers
            .iter_mut()
            .find(|l| l.id == id)
            .unwrap();
        let preview = layer.properties[&property]
            .preview_key_velocity_scale(&frames, transform)
            .unwrap();
        layer.properties.insert(property, preview);
    }
    expected.project.version = expected.project.version.max(35);
    e.execute(Command::ScaleKeyVelocities {
        keys: refs.clone(),
        scale: transform,
    })
    .unwrap();
    assert_eq!(e.current, expected);
    e.undo();
    assert_eq!(e.current, original);
    e.current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == 2)
        .unwrap()
        .locked = true;
    assert_untouched(
        &mut e,
        Command::ScaleKeyVelocities {
            keys: refs,
            scale: transform,
        },
        false,
    );
    assert_untouched(
        &mut e,
        Command::Batch(vec![
            command(&[10], transform),
            Command::Batch(vec![command(&[99], transform)]),
        ]),
        false,
    );
}

#[test]
fn velocity_scale_exact_linear_fixed_point_is_noop_for_both_endpoints() {
    let mut before = track();
    let end = before.keys.remove(&30).unwrap();
    before.keys.insert(20, end);
    before.keys.remove(&80);
    assert_eq!(
        before.key_velocity_handles(10).unwrap()[1].unwrap().slope,
        4.0
    );
    assert_eq!(
        before.key_velocity_handles(20).unwrap()[0].unwrap().slope,
        4.0
    );
    let mut e = scene(before);
    e.current.project.version = 1;
    for factor in [0.0, 2.0, -3.0, f64::MAX] {
        assert_untouched(&mut e, command(&[10, 20], scale(4.0, factor)), true);
    }
}

#[test]
fn velocity_scale_explicit_linked_auto_noops_keep_exact_metadata_and_history() {
    for mode in [
        TemporalMode::Independent,
        TemporalMode::Continuous,
        TemporalMode::Auto,
    ] {
        let mut before = track();
        for key in before.keys.values_mut() {
            key.temporal = if mode == TemporalMode::Auto {
                TemporalHandles {
                    mode,
                    ..Default::default()
                }
            } else {
                TemporalHandles {
                    mode,
                    incoming: Some(handle(2.0, 0.001)),
                    outgoing: Some(handle(2.0, 1.0)),
                }
            };
        }
        let mut e = scene(before.clone());
        e.current.project.version = PROJECT_VERSION;
        assert_untouched(
            &mut e,
            Command::Batch(vec![command(&[10, 30, 80], scale(f64::MAX, 1.0))]),
            true,
        );
        for frame in [10, 30, 80] {
            let origin = before
                .key_velocity_handles(frame)
                .unwrap()
                .into_iter()
                .flatten()
                .next()
                .unwrap()
                .slope;
            assert_untouched(
                &mut e,
                Command::Batch(vec![
                    Command::Batch(vec![command(&[frame], scale(origin, 0.0))]),
                    command(&[frame], scale(origin, f64::MAX)),
                ]),
                true,
            );
        }
    }
}

#[test]
fn velocity_scale_near_identity_large_origin_retains_representable_precision() {
    for slope in [0.0, 1.0, -1.0] {
        let mut before = track();
        before.keys.get_mut(&10).unwrap().temporal.outgoing = Some(handle(slope, 0.4));
        let factor = 1.0000000000000002;
        let after = before
            .preview_key_velocity_scale(&[10], scale(1e16, factor))
            .unwrap();
        let expected = slope - 2.220446049250313;
        assert_near(after.keys[&10].temporal.outgoing.unwrap().slope, expected);
        assert_fixed(&before, &after);
    }
    let mut before = track();
    before.keys.get_mut(&10).unwrap().temporal.outgoing = Some(handle(1.0, 0.4));
    let after = before
        .preview_key_velocity_scale(&[10], scale(0.0, f64::EPSILON / 4.0))
        .unwrap();
    assert_eq!(
        after.keys[&10].temporal.outgoing.unwrap().slope,
        f64::EPSILON / 4.0
    );
}

#[test]
fn velocity_scale_diagonal_legacy_bezier_has_exact_secant_fixed_points() {
    let mut before = track();
    let end = before.keys.remove(&30).unwrap();
    before.keys.insert(20, end);
    before.keys.remove(&80);
    before.keys.get_mut(&10).unwrap().interpolation = Interpolation::Bezier(Bezier {
        x1: 0.17,
        y1: 0.17,
        x2: 0.63,
        y2: 0.63,
    });
    let mut e = scene(before);
    e.current.project.version = 2;
    for factor in [0.0, 2.0, -3.0] {
        assert_untouched(&mut e, command(&[10, 20], scale(4.0, factor)), true);
    }
}

#[test]
fn velocity_scale_real_edits_preserve_high_schema_and_unrelated_assets() {
    let mut e = scene(track());
    for (png, frame) in [("YWJj", Some(0)), ("ZGVm", None)] {
        e.execute(Command::ImportAsset {
            content: Content::Image { png: png.into() },
            width: 64.0,
            height: 48.0,
            name: "Unrelated source".into(),
            folder: None,
            frame,
        })
        .unwrap();
    }
    e.current.project.version = PROJECT_VERSION;
    let before = e.current.clone();
    let mut expected = before.clone();
    let preview = current_track(&e)
        .preview_key_velocity_scale(&[10, 30], scale(1.0, 2.0))
        .unwrap();
    expected
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == 1)
        .unwrap()
        .properties
        .insert(Property::PositionX, preview);
    e.execute(Command::Batch(vec![command(&[10, 30], scale(1.0, 2.0))]))
        .unwrap();
    assert_eq!(e.current, expected);
    e.undo();
    assert_eq!(e.current, before);
}
