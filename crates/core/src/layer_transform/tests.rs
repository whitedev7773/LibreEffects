use super::*;

const OPS: [LayerTransformOp; 5] = [
    LayerTransformOp::ResetScaleRotation,
    LayerTransformOp::FlipHorizontal,
    LayerTransformOp::FlipVertical,
    LayerTransformOp::FitInsideComposition,
    LayerTransformOp::CenterAnchorInSourceBounds,
];
fn command(ids: &[LayerId], frame: Frame, operation: LayerTransformOp) -> Command {
    Command::TransformLayers {
        ids: ids.to_vec(),
        frame,
        operation,
    }
}
fn layer_mut(e: &mut Editor, id: LayerId) -> &mut Layer {
    e.current
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == id)
        .unwrap()
}
fn value(e: &Editor, id: LayerId, property: Property, frame: Frame) -> f64 {
    e.project()
        .composition
        .layer(id)
        .unwrap()
        .property(property)
        .unwrap()
        .value_at(frame)
}
fn set(e: &mut Editor, id: LayerId, values: &[(Property, f64)]) {
    for &(p, v) in values {
        layer_mut(e, id)
            .properties
            .insert(p, AnimatedProperty::new(v));
    }
}
fn scene() -> Editor {
    let mut e = Editor::default();
    for (width, height) in [(320.0, 180.0), (240.0, 400.0), (60.0, 90.0)] {
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width,
            height,
            name: "Source".into(),
        })
        .unwrap();
    }
    set(
        &mut e,
        1,
        &[
            (Property::PositionX, 210.0),
            (Property::PositionY, 170.0),
            (Property::AnchorX, 17.0),
            (Property::AnchorY, 29.0),
            (Property::ScaleX, -73.0),
            (Property::ScaleY, 124.0),
            (Property::Rotation, 31.0),
            (Property::Opacity, 42.0),
        ],
    );
    set(
        &mut e,
        2,
        &[
            (Property::PositionX, 630.0),
            (Property::PositionY, 310.0),
            (Property::AnchorX, 12.0),
            (Property::AnchorY, 19.0),
            (Property::ScaleX, 113.0),
            (Property::ScaleY, -87.0),
            (Property::Rotation, -18.0),
        ],
    );
    e.current.project.version = PROJECT_VERSION;
    e.select(2);
    e.current.project.validate().unwrap();
    e.clear_history();
    e
}
fn animate(
    e: &mut Editor,
    id: LayerId,
    property: Property,
    interpolation: Interpolation,
    temporal: TemporalHandles,
) {
    let v = value(e, id, property, 0);
    layer_mut(e, id).properties.insert(
        property,
        AnimatedProperty {
            value: v + 1.0,
            keys: [0, 20, 40]
                .into_iter()
                .map(|f| {
                    (
                        f,
                        Keyframe {
                            value: v,
                            interpolation,
                            temporal,
                        },
                    )
                })
                .collect(),
        },
    );
}
fn redo_sentinel(e: &mut Editor) {
    let mut next = e.current.clone();
    next.project.composition.layers[0].name.push_str(" Redo");
    e.redo.push(next);
}
fn unchanged(e: &mut Editor, c: Command, success: bool) {
    let before = e.current.clone();
    let bytes = e.project().to_json().unwrap();
    let undo = e.undo.clone();
    let redo = e.redo.clone();
    let result = e.execute(c);
    assert_eq!(result.is_ok(), success, "{result:?}");
    assert_eq!(e.current, before);
    assert_eq!(e.project().to_json().unwrap(), bytes);
    assert_eq!(e.undo, undo);
    assert_eq!(e.redo, redo);
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn same_world(before: &Composition, after: &Composition, id: LayerId, frame: Frame) {
    for (a, b) in before
        .corners_at(id, frame)
        .unwrap()
        .into_iter()
        .flatten()
        .zip(after.corners_at(id, frame).unwrap().into_iter().flatten())
    {
        near(a, b);
    }
}
fn assert_fitted(e: &Editor, id: LayerId, frame: Frame) {
    let comp = &e.project().composition;
    let bounds = comp.layer_bounds(id, frame).unwrap();
    near((bounds[0] + bounds[2]) * 0.5, comp.width as f64 * 0.5);
    near((bounds[1] + bounds[3]) * 0.5, comp.height as f64 * 0.5);
    let ratios = [
        (bounds[2] - bounds[0]) / comp.width as f64,
        (bounds[3] - bounds[1]) / comp.height as f64,
    ];
    assert!(ratios.iter().all(|v| *v <= 1.0 + 1e-12));
    near(ratios[0].max(ratios[1]), 1.0);
}

#[test]
fn layer_essentials_static_all_operations_are_one_undo_and_roundtrip() {
    for operation in OPS {
        let mut e = scene();
        let before = e.current.clone();
        redo_sentinel(&mut e);
        e.execute(command(&[1, 2], 17, operation)).unwrap();
        assert_eq!(e.selected(), Some(2));
        assert_eq!(e.undo.len(), 1);
        assert!(!e.can_redo());
        let after = e.current.clone();
        for id in [1, 2] {
            let old = before.project.composition.layer(id).unwrap();
            let current = e.project().composition.layer(id).unwrap();
            assert!(current.properties.values().all(|p| p.keys.is_empty()));
            match operation {
                LayerTransformOp::ResetScaleRotation => {
                    assert_eq!(value(&e, id, Property::ScaleX, 17), 100.0);
                    assert_eq!(value(&e, id, Property::ScaleY, 17), 100.0);
                    assert_eq!(value(&e, id, Property::Rotation, 17), 0.0);
                    for p in [
                        Property::PositionX,
                        Property::PositionY,
                        Property::AnchorX,
                        Property::AnchorY,
                        Property::Opacity,
                    ] {
                        assert_eq!(current.property(p).unwrap(), old.property(p).unwrap());
                    }
                }
                LayerTransformOp::FlipHorizontal | LayerTransformOp::FlipVertical => {
                    let p = if operation == LayerTransformOp::FlipHorizontal {
                        Property::ScaleX
                    } else {
                        Property::ScaleY
                    };
                    assert_eq!(
                        current.property(p).unwrap().value,
                        -old.property(p).unwrap().value
                    );
                    for other in Property::ALL.into_iter().filter(|o| *o != p) {
                        assert_eq!(
                            current.property(other).unwrap(),
                            old.property(other).unwrap()
                        );
                    }
                }
                LayerTransformOp::FitInsideComposition => {
                    assert_fitted(&e, id, 17);
                    near(
                        value(&e, id, Property::ScaleX, 17)
                            / old.property(Property::ScaleX).unwrap().value,
                        value(&e, id, Property::ScaleY, 17)
                            / old.property(Property::ScaleY).unwrap().value,
                    );
                    for p in [
                        Property::Rotation,
                        Property::AnchorX,
                        Property::AnchorY,
                        Property::Opacity,
                    ] {
                        assert_eq!(current.property(p).unwrap(), old.property(p).unwrap());
                    }
                }
                LayerTransformOp::CenterAnchorInSourceBounds => {
                    assert_eq!(value(&e, id, Property::AnchorX, 17), current.width * 0.5);
                    assert_eq!(value(&e, id, Property::AnchorY, 17), current.height * 0.5);
                    same_world(
                        &before.project.composition,
                        &e.project().composition,
                        id,
                        17,
                    );
                }
            }
        }
        assert_eq!(
            e.project().composition.layer(3),
            before.project.composition.layer(3)
        );
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
        let encoded = project_file::encode(e.project(), None).unwrap();
        assert_eq!(
            project_file::decode(&encoded).unwrap().project,
            *e.project()
        );
        e.undo();
        assert_eq!(e.current, before);
        assert!(!e.can_undo());
        e.redo();
        assert_eq!(e.current, after);
    }
}

#[test]
fn layer_essentials_animated_current_and_middle_frames_preserve_metadata_and_untouched_keys() {
    let handles = TemporalHandles {
        incoming: Some(TemporalHandle {
            slope: 0.25,
            influence: 0.2,
        }),
        outgoing: Some(TemporalHandle {
            slope: -0.75,
            influence: 0.4,
        }),
        ..Default::default()
    };
    for operation in OPS {
        for frame in [10, 20] {
            for (interpolation, temporal) in [
                (Interpolation::Hold, handles),
                (Interpolation::Bezier(Bezier::default()), handles),
                (
                    Interpolation::Smooth,
                    TemporalHandles {
                        mode: TemporalMode::Auto,
                        ..Default::default()
                    },
                ),
            ] {
                let mut e = scene();
                for p in Property::ALL {
                    animate(&mut e, 1, p, interpolation, temporal);
                }
                // A static track alongside animated tracks exercises mixed policy.
                set(&mut e, 1, &[(Property::PositionY, 170.0)]);
                let before = e.current.clone();
                e.execute(command(&[1], frame, operation)).unwrap();
                let old = before.project.composition.layer(1).unwrap();
                let after = e.project().composition.layer(1).unwrap();
                for p in Property::ALL {
                    let a = old.property(p).unwrap();
                    let b = after.property(p).unwrap();
                    if a == b {
                        continue;
                    }
                    if a.keys.is_empty() {
                        assert!(b.keys.is_empty());
                        continue;
                    }
                    assert_eq!(b.value, a.value);
                    assert_eq!(b.keys.len(), a.keys.len() + usize::from(frame == 10));
                    for (&f, k) in &a.keys {
                        if f != frame {
                            assert_eq!(&b.keys[&f], k);
                        }
                    }
                    assert_eq!(b.keys[&frame].interpolation, interpolation);
                    assert_eq!(
                        b.keys[&frame].temporal,
                        if frame == 20 {
                            temporal
                        } else {
                            TemporalHandles::default()
                        }
                    );
                }
                assert_eq!(e.undo.len(), 1);
            }
        }
    }
}

#[test]
fn layer_essentials_only_changed_channels_add_keys() {
    let mut e = scene();
    set(
        &mut e,
        1,
        &[
            (Property::ScaleX, 100.0),
            (Property::ScaleY, 80.0),
            (Property::Rotation, 0.0),
        ],
    );
    for p in Property::ALL {
        animate(
            &mut e,
            1,
            p,
            Interpolation::Hold,
            TemporalHandles::default(),
        );
    }
    let before = e.current.clone();
    e.execute(command(&[1], 10, LayerTransformOp::ResetScaleRotation))
        .unwrap();
    for p in Property::ALL {
        let a = before
            .project
            .composition
            .layer(1)
            .unwrap()
            .property(p)
            .unwrap();
        let b = e
            .project()
            .composition
            .layer(1)
            .unwrap()
            .property(p)
            .unwrap();
        if p == Property::ScaleY {
            assert!(b.keys.contains_key(&10));
        } else {
            assert_eq!(a, b);
        }
    }
    let mut e = scene();
    set(
        &mut e,
        1,
        &[(Property::Rotation, 0.0), (Property::AnchorX, 160.0)],
    );
    for p in Property::ALL {
        animate(
            &mut e,
            1,
            p,
            Interpolation::Linear,
            TemporalHandles::default(),
        );
    }
    let before = e.current.clone();
    e.execute(command(
        &[1],
        10,
        LayerTransformOp::CenterAnchorInSourceBounds,
    ))
    .unwrap();
    for p in [
        Property::AnchorX,
        Property::PositionX,
        Property::ScaleX,
        Property::ScaleY,
        Property::Rotation,
        Property::Opacity,
    ] {
        assert_eq!(
            e.project()
                .composition
                .layer(1)
                .unwrap()
                .property(p)
                .unwrap(),
            before
                .project
                .composition
                .layer(1)
                .unwrap()
                .property(p)
                .unwrap()
        );
    }
}

#[test]
fn layer_essentials_root_selection_and_compensation_offsets() {
    for operation in OPS {
        let mut e = scene();
        layer_mut(&mut e, 2).parent = Some(1);
        layer_mut(&mut e, 2).transform_offset = Affine([0.8, 0.2, -0.1, 1.2, 33.0, -17.0]);
        layer_mut(&mut e, 3).parent = Some(2);
        let before = e.current.clone();
        e.execute(command(&[2, 1, 2], 12, operation)).unwrap();
        let after = e.project().composition.layer(2).unwrap();
        assert_eq!(after.parent, Some(1));
        assert_eq!(
            after.transform_offset,
            before
                .project
                .composition
                .layer(2)
                .unwrap()
                .transform_offset
        );
        if operation == LayerTransformOp::CenterAnchorInSourceBounds {
            for id in [1, 2, 3] {
                same_world(
                    &before.project.composition,
                    &e.project().composition,
                    id,
                    12,
                );
            }
        } else {
            assert_eq!(after, before.project.composition.layer(2).unwrap());
        }
        assert_eq!(
            e.project().composition.layer(3),
            before.project.composition.layer(3)
        );
    }
}

#[test]
fn layer_essentials_fit_child_through_rotated_reflected_nonuniform_parent_and_offset() {
    let mut e = scene();
    layer_mut(&mut e, 2).parent = Some(1);
    layer_mut(&mut e, 2).transform_offset = Affine([0.8, 0.2, -0.1, 1.2, 33.0, -17.0]);
    let before = e.current.clone();
    e.execute(command(&[2, 3], 13, LayerTransformOp::FitInsideComposition))
        .unwrap();
    assert_fitted(&e, 2, 13);
    assert_fitted(&e, 3, 13);
    assert_eq!(
        e.project().composition.layer(1),
        before.project.composition.layer(1)
    );
    redo_sentinel(&mut e);
    unchanged(
        &mut e,
        command(&[2, 3], 13, LayerTransformOp::FitInsideComposition),
        true,
    );
}

#[test]
fn layer_essentials_center_works_under_singular_parent_and_reset_flips_need_no_inverse() {
    for operation in [
        LayerTransformOp::CenterAnchorInSourceBounds,
        LayerTransformOp::ResetScaleRotation,
        LayerTransformOp::FlipHorizontal,
        LayerTransformOp::FlipVertical,
    ] {
        let mut e = scene();
        layer_mut(&mut e, 2).parent = Some(1);
        layer_mut(&mut e, 2).transform_offset = Affine([1.0, 0.3, -0.2, 0.7, 8.0, 2.0]);
        set(&mut e, 1, &[(Property::ScaleX, 0.0)]);
        let before = e.current.clone();
        e.execute(command(&[2], 0, operation)).unwrap();
        if operation == LayerTransformOp::CenterAnchorInSourceBounds {
            same_world(&before.project.composition, &e.project().composition, 2, 0);
        }
    }
}

#[test]
fn layer_essentials_validation_is_atomic_for_all_explicit_members() {
    for operation in OPS {
        for defect in 0..5 {
            let mut e = scene();
            let mut ids = vec![1, 2];
            let mut frame = 0;
            match defect {
                0 => layer_mut(&mut e, 2).locked = true,
                1 => ids.push(999),
                2 => frame = 150,
                3 => ids.clear(),
                _ => {
                    layer_mut(&mut e, 2).parent = Some(1);
                    layer_mut(&mut e, 2).locked = true;
                }
            }
            redo_sentinel(&mut e);
            unchanged(&mut e, command(&ids, frame, operation), false);
        }
        let mut e = scene();
        layer_mut(&mut e, 2).content = Content::Audio {
            path: "sound.wav".into(),
            audio: AudioMetadata {
                stream_index: 0,
                sample_rate: 48000,
                channels: 2,
                channel_layout: "stereo".into(),
                duration: 3.0,
                start_time: 0.0,
                file_offset: 0.0,
            },
            start_frame: 0,
            playback: VideoPlayback::default(),
        };
        layer_mut(&mut e, 2).parent = Some(1);
        redo_sentinel(&mut e);
        unchanged(&mut e, command(&[1, 2], 0, operation), false);
    }
    for operation in OPS {
        let mut e = scene();
        layer_mut(&mut e, 2).content = Content::Null;
        if matches!(
            operation,
            LayerTransformOp::FitInsideComposition | LayerTransformOp::CenterAnchorInSourceBounds
        ) {
            unchanged(&mut e, command(&[1, 2], 0, operation), false);
        } else {
            e.execute(command(&[1, 2], 0, operation)).unwrap();
        }
    }
}

#[test]
fn layer_essentials_fit_rejects_collapsed_singular_and_out_of_range_results_atomically() {
    for defect in 0..5 {
        let mut e = scene();
        match defect {
            0 => set(&mut e, 2, &[(Property::ScaleX, 0.0)]),
            1 => {
                layer_mut(&mut e, 2).parent = Some(3);
                set(&mut e, 3, &[(Property::ScaleY, 0.0)]);
            }
            2 => {
                let l = layer_mut(&mut e, 2);
                l.width = 1.0;
                l.height = 1.0;
            }
            3 => layer_mut(&mut e, 2).transform_offset = Affine([1.0, 0.0, 0.0, 1.0, 1e10, 0.0]),
            _ => {
                layer_mut(&mut e, 2).parent = Some(1);
                set(&mut e, 2, &[(Property::ScaleY, 0.0)]);
            }
        }
        redo_sentinel(&mut e);
        unchanged(
            &mut e,
            command(&[1, 2], 0, LayerTransformOp::FitInsideComposition),
            false,
        );
    }
    let mut e = scene();
    set(
        &mut e,
        2,
        &[
            (Property::PositionX, 1_000_000.0),
            (Property::AnchorX, -1_000_000.0),
            (Property::Rotation, 0.0),
            (Property::ScaleX, 100.0),
        ],
    );
    unchanged(
        &mut e,
        command(&[1, 2], 0, LayerTransformOp::CenterAnchorInSourceBounds),
        false,
    );
}

#[test]
fn layer_essentials_exact_noops_preserve_legacy_schema_keys_assets_and_redo() {
    for version in [1, 2, 3, 35, 36, PROJECT_VERSION] {
        for operation in [
            LayerTransformOp::ResetScaleRotation,
            LayerTransformOp::CenterAnchorInSourceBounds,
            LayerTransformOp::FlipHorizontal,
            LayerTransformOp::FlipVertical,
        ] {
            let mut e = scene();
            set(
                &mut e,
                1,
                &[
                    (Property::ScaleX, 100.0),
                    (Property::ScaleY, 100.0),
                    (Property::Rotation, 0.0),
                    (Property::AnchorX, 160.0),
                    (Property::AnchorY, 90.0),
                ],
            );
            if operation == LayerTransformOp::FlipHorizontal {
                set(&mut e, 1, &[(Property::ScaleX, 0.0)]);
            }
            if operation == LayerTransformOp::FlipVertical {
                set(&mut e, 1, &[(Property::ScaleY, -0.0)]);
            }
            for p in Property::ALL {
                animate(
                    &mut e,
                    1,
                    p,
                    Interpolation::Hold,
                    TemporalHandles::default(),
                );
            }
            e.current.project.version = version;
            redo_sentinel(&mut e);
            let c = command(&[1], 10, operation);
            unchanged(&mut e, c.clone(), true);
            unchanged(
                &mut e,
                Command::Batch(vec![Command::Batch(vec![]), Command::Batch(vec![c])]),
                true,
            );
        }
    }
    let mut e = scene();
    for (png, frame) in [("YWJj", Some(0)), ("ZGVm", None)] {
        e.execute(Command::ImportAsset {
            content: Content::Image { png: png.into() },
            width: 64.0,
            height: 48.0,
            name: "Shared source".into(),
            folder: None,
            frame,
        })
        .unwrap();
    }
    set(
        &mut e,
        1,
        &[
            (Property::ScaleX, 100.0),
            (Property::ScaleY, 100.0),
            (Property::Rotation, 0.0),
        ],
    );
    e.current.project.version = PROJECT_VERSION;
    redo_sentinel(&mut e);
    unchanged(
        &mut e,
        command(&[1], 0, LayerTransformOp::ResetScaleRotation),
        true,
    );
    let library = e.project().asset_library.clone();
    e.execute(command(&[1], 0, LayerTransformOp::FlipHorizontal))
        .unwrap();
    assert_eq!(e.project().asset_library, library);
    assert_eq!(e.project().version, PROJECT_VERSION);
}

#[test]
fn layer_essentials_nested_empty_batches_keep_the_existing_migration_policy() {
    let c = command(&[1], 0, LayerTransformOp::FlipHorizontal);
    assert!(edits_only(&c));
    assert!(edits_only(&Command::Batch(vec![Command::Batch(vec![]), c])));
    assert!(!edits_only(&Command::Batch(vec![])));
    assert!(!edits_only(&Command::Batch(vec![Command::Batch(vec![])])));
    assert!(!edits_only(&Command::Batch(vec![
        command(&[1], 0, LayerTransformOp::FlipHorizontal),
        Command::RenameLayer {
            id: 1,
            name: "Change".into()
        }
    ])));
    let mut e = scene();
    let library = e.project().asset_library.clone();
    e.current.project.version = 1;
    e.execute(command(&[1], 0, LayerTransformOp::ResetScaleRotation))
        .unwrap();
    assert_eq!(e.project().version, 1);
    assert_eq!(e.project().asset_library, library);
}

#[test]
fn layer_essentials_fit_roundoff_is_idempotent_but_does_not_hide_real_offsets() {
    for angle in [0.0, 13.123456789, 45.0, 89.9999, 170.27] {
        let mut e = scene();
        set(&mut e, 1, &[(Property::Rotation, angle)]);
        for p in [
            Property::ScaleX,
            Property::ScaleY,
            Property::PositionX,
            Property::PositionY,
        ] {
            animate(
                &mut e,
                1,
                p,
                Interpolation::Linear,
                TemporalHandles::default(),
            );
        }
        e.execute(command(&[1], 10, LayerTransformOp::FitInsideComposition))
            .unwrap();
        assert_fitted(&e, 1, 10);
        redo_sentinel(&mut e);
        for _ in 0..8 {
            unchanged(
                &mut e,
                command(&[1], 10, LayerTransformOp::FitInsideComposition),
                true,
            );
        }
        let fitted = e.current.clone();
        let before_x = value(&e, 1, Property::PositionX, 10);
        layer_mut(&mut e, 1)
            .properties
            .get_mut(&Property::PositionX)
            .unwrap()
            .keys
            .get_mut(&10)
            .unwrap()
            .value += 1e-6;
        let scale_x = e
            .project()
            .composition
            .layer(1)
            .unwrap()
            .property(Property::ScaleX)
            .unwrap()
            .clone();
        let scale_y = e
            .project()
            .composition
            .layer(1)
            .unwrap()
            .property(Property::ScaleY)
            .unwrap()
            .clone();
        e.execute(command(&[1], 10, LayerTransformOp::FitInsideComposition))
            .unwrap();
        near(value(&e, 1, Property::PositionX, 10), before_x);
        assert_eq!(
            e.project()
                .composition
                .layer(1)
                .unwrap()
                .property(Property::ScaleX)
                .unwrap(),
            &scale_x
        );
        assert_eq!(
            e.project()
                .composition
                .layer(1)
                .unwrap()
                .property(Property::ScaleY)
                .unwrap(),
            &scale_y
        );
        assert_eq!(e.current, fitted);
    }
}

#[test]
fn layer_essentials_fit_recenter_only_avoids_new_animated_scale_keys() {
    let mut e = scene();
    e.execute(command(&[1], 0, LayerTransformOp::FitInsideComposition))
        .unwrap();
    for p in [Property::ScaleX, Property::ScaleY] {
        animate(
            &mut e,
            1,
            p,
            Interpolation::Hold,
            TemporalHandles::default(),
        );
    }
    set(
        &mut e,
        1,
        &[(Property::PositionX, 123.0), (Property::PositionY, 456.0)],
    );
    let before = e.current.clone();
    e.clear_history();
    e.execute(command(&[1], 10, LayerTransformOp::FitInsideComposition))
        .unwrap();
    for p in [Property::ScaleX, Property::ScaleY] {
        assert_eq!(
            e.project()
                .composition
                .layer(1)
                .unwrap()
                .property(p)
                .unwrap(),
            before
                .project
                .composition
                .layer(1)
                .unwrap()
                .property(p)
                .unwrap()
        );
    }
    assert_fitted(&e, 1, 10);
    assert_eq!(e.undo.len(), 1);
}

#[test]
fn layer_essentials_unrelated_geometry_and_inactive_compositions_stay_exact() {
    let mut e = scene();
    let mut other = e.project().composition.clone();
    for l in &mut other.layers {
        l.id += 10;
    }
    e.current.project.other_compositions.insert(2, other);
    e.current.project.next_composition_id = 3;
    e.current.project.next_layer_id = 20;
    layer_mut(&mut e, 1).effects = Effects {
        blur: 7.0,
        brightness: 1.2,
        grayscale: true,
    };
    layer_mut(&mut e, 1).mask = Some(Mask {
        x: 11.0,
        y: 21.0,
        width: 80.0,
        height: 90.0,
        inverted: false,
    });
    for operation in OPS {
        let before = e.current.clone();
        e.execute(command(&[1], 12, operation)).unwrap();
        let mut actual = e.current.clone();
        // Only transform tracks are allowed to differ.
        actual
            .project
            .composition
            .layers
            .iter_mut()
            .find(|l| l.id == 1)
            .unwrap()
            .properties = before
            .project
            .composition
            .layer(1)
            .unwrap()
            .properties
            .clone();
        assert_eq!(actual, before);
    }
}

#[test]
fn layer_essentials_deep_parent_fit_and_cancelling_translation_conditioning() {
    let mut e = scene();
    for id in 4..=12 {
        e.execute(Command::AddRectangle).unwrap();
        layer_mut(&mut e, id).parent = Some(id - 1);
        set(
            &mut e,
            id,
            &[
                (Property::PositionX, 3.0),
                (Property::PositionY, -2.0),
                (Property::Rotation, 1.0),
                (Property::ScaleX, 99.0),
                (Property::ScaleY, 101.0),
            ],
        );
    }
    e.clear_history();
    e.execute(command(&[12], 0, LayerTransformOp::FitInsideComposition))
        .unwrap();
    assert_fitted(&e, 12, 0);
    redo_sentinel(&mut e);
    unchanged(
        &mut e,
        command(&[12], 0, LayerTransformOp::FitInsideComposition),
        true,
    );
    let mut e = scene();
    layer_mut(&mut e, 2).parent = Some(1);
    layer_mut(&mut e, 1).transform_offset = Affine([1.0, 0.0, 0.0, 1.0, 1e12, 1e12]);
    layer_mut(&mut e, 2).transform_offset = Affine([1.0, 0.0, 0.0, 1.0, -1e12, -1e12]);
    set(
        &mut e,
        1,
        &[
            (Property::Rotation, 0.0),
            (Property::ScaleX, 100.0),
            (Property::ScaleY, 100.0),
            (Property::PositionX, 0.0),
            (Property::PositionY, 0.0),
            (Property::AnchorX, 0.0),
            (Property::AnchorY, 0.0),
        ],
    );
    // Although the position-space translations cancel, the renderer composes
    // them around the child local transform. Reject lost subpixel accuracy.
    redo_sentinel(&mut e);
    unchanged(
        &mut e,
        command(&[2], 0, LayerTransformOp::FitInsideComposition),
        false,
    );
}

#[test]
fn layer_essentials_ignore_unrelated_opacity_overshoot_and_reset_repairs_scale_overshoot() {
    fn overshoot(start: f64, end: f64) -> AnimatedProperty {
        AnimatedProperty {
            value: start,
            keys: [(0, start), (20, end)]
                .into_iter()
                .map(|(f, value)| {
                    (
                        f,
                        Keyframe {
                            value,
                            interpolation: Interpolation::Bezier(Bezier {
                                x1: 0.25,
                                y1: 3.0,
                                x2: 0.75,
                                y2: 3.0,
                            }),
                            temporal: TemporalHandles::default(),
                        },
                    )
                })
                .collect(),
        }
    }
    for operation in OPS {
        let mut e = scene();
        let track = overshoot(0.0, 100.0);
        assert!(track.value_at(10) > 100.0);
        layer_mut(&mut e, 1)
            .properties
            .insert(Property::Opacity, track.clone());
        e.current.project.validate().unwrap();
        e.execute(command(&[1], 10, operation)).unwrap();
        assert_eq!(
            e.project()
                .composition
                .layer(1)
                .unwrap()
                .property(Property::Opacity)
                .unwrap(),
            &track
        );
    }
    let mut e = scene();
    let track = overshoot(100.0, 10_000.0);
    assert!(track.value_at(10) > 10_000.0);
    layer_mut(&mut e, 1)
        .properties
        .insert(Property::ScaleX, track);
    e.current.project.validate().unwrap();
    e.execute(command(&[1], 10, LayerTransformOp::ResetScaleRotation))
        .unwrap();
    assert_eq!(value(&e, 1, Property::ScaleX, 10), 100.0);
}

#[test]
fn layer_essentials_fit_rejects_rank_loss_but_recovers_tiny_uniform_source() {
    let mut e = scene();
    set(
        &mut e,
        1,
        &[
            (Property::ScaleX, f64::from_bits(1)),
            (Property::ScaleY, 100.0),
            (Property::Rotation, 31.0),
        ],
    );
    let [a, b, c, d, _, _] = e.project().composition.world_transform(1, 0).unwrap().0;
    assert_eq!([a, b], [0.0, 0.0]);
    // Both AABB dimensions are positive despite the collapsed source axis.
    assert!(c.abs() > 0.0 && d.abs() > 0.0);
    redo_sentinel(&mut e);
    unchanged(
        &mut e,
        command(&[1], 0, LayerTransformOp::FitInsideComposition),
        false,
    );
    let mut e = scene();
    set(
        &mut e,
        1,
        &[
            (Property::ScaleX, 1e-200),
            (Property::ScaleY, 1e-200),
            (Property::Rotation, 31.0),
        ],
    );
    let [a, b, c, d, _, _] = e.project().composition.world_transform(1, 0).unwrap().0;
    assert_eq!(a * d - b * c, 0.0); // Raw determinant underflow is not rank loss.
    e.execute(command(&[1], 0, LayerTransformOp::FitInsideComposition))
        .unwrap();
    assert_fitted(&e, 1, 0);
    assert!(value(&e, 1, Property::ScaleX, 0) > 0.0);
    assert_eq!(
        value(&e, 1, Property::ScaleX, 0),
        value(&e, 1, Property::ScaleY, 0)
    );
    redo_sentinel(&mut e);
    unchanged(
        &mut e,
        command(&[1], 0, LayerTransformOp::FitInsideComposition),
        true,
    );
}

#[test]
fn layer_essentials_center_preserves_unchanged_overshooting_position_channels() {
    for partial in [false, true] {
        let mut e = scene();
        set(
            &mut e,
            1,
            &[
                (Property::AnchorX, 160.0),
                (Property::AnchorY, if partial { 19.0 } else { 90.0 }),
                (Property::Rotation, 0.0),
            ],
        );
        let track = AnimatedProperty {
            value: 0.0,
            keys: [(0, 0.0), (20, 1_000_000.0)]
                .into_iter()
                .map(|(f, value)| {
                    (
                        f,
                        Keyframe {
                            value,
                            interpolation: Interpolation::Bezier(Bezier {
                                x1: 0.25,
                                y1: 3.0,
                                x2: 0.75,
                                y2: 3.0,
                            }),
                            temporal: TemporalHandles::default(),
                        },
                    )
                })
                .collect(),
        };
        assert!(track.value_at(10) > 1_000_000.0);
        layer_mut(&mut e, 1)
            .properties
            .insert(Property::PositionX, track.clone());
        e.current.project.validate().unwrap();
        redo_sentinel(&mut e);
        let before = e.current.clone();
        let c = command(&[1], 10, LayerTransformOp::CenterAnchorInSourceBounds);
        if partial {
            e.execute(c).unwrap();
            assert_eq!(value(&e, 1, Property::AnchorY, 10), 90.0);
            assert_ne!(value(&e, 1, Property::PositionY, 10), 170.0);
            same_world(&before.project.composition, &e.project().composition, 1, 10);
            assert_eq!(e.undo.len(), 1);
        } else {
            unchanged(&mut e, c, true);
        }
        assert_eq!(
            e.project()
                .composition
                .layer(1)
                .unwrap()
                .property(Property::PositionX)
                .unwrap(),
            &track
        );
    }
}
