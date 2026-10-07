use super::*;
use GradientColorsInterpolation::{Hold, Linear, Smooth};

fn key_copy(
    offset: Frame,
    colors: &GradientColors,
    interpolation: GradientColorsInterpolation,
) -> GradientColorsKeyCopy {
    GradientColorsKeyCopy {
        offset,
        colors: colors.clone(),
        interpolation,
    }
}

fn actions(sample: &GradientColors) -> Vec<GradientColorsEdit> {
    vec![
        GradientColorsEdit::MoveKeys {
            frames: BTreeSet::from([10, 50]),
            to: 20,
        },
        GradientColorsEdit::DeleteKeys {
            frames: BTreeSet::from([10, 50]),
            frame: 30,
        },
        GradientColorsEdit::SetInterpolations {
            frames: BTreeSet::from([10, 50]),
            interpolation: Linear,
        },
        GradientColorsEdit::PasteKeys {
            keys: vec![key_copy(0, sample, Smooth)],
            frame: 90,
        },
    ]
}

#[test]
fn gradient_colors_multikey_move_overlaps_selected_sources_and_preserves_whole_source_and_modes() {
    let (mut e, start, end) = interpolation_scene();
    mode(&mut e, 10, Linear);
    edit(&mut e, GradientColorsEdit::ToggleKey { frame: 130 });
    mode(&mut e, 130, Smooth);
    e.clear_history();
    let before = e.current.clone();
    let mut expected = before.clone();
    let g = node_mut(&mut expected.project).kind.gradient_mut().unwrap();
    g.colors_animation = Some(
        serde_json::from_value(serde_json::json!({
            "keys": { "50": start, "90": end, "130": end },
            "outgoing_interpolation": { "50": "Linear", "130": "Smooth" }
        }))
        .unwrap(),
    );
    edit(
        &mut e,
        GradientColorsEdit::MoveKeys {
            frames: BTreeSet::from([10, 50]),
            to: 50,
        },
    );
    assert_eq!(e.current, expected);
    assert_eq!(e.undo.len(), 1);
    assert_eq!(animation(e.project()).interpolation(90), Some(Hold));
    e.undo();
    assert_eq!(e.current, before);
    e.redo();
    assert_eq!(e.current, expected);
    edit(
        &mut e,
        GradientColorsEdit::MoveKeys {
            frames: BTreeSet::from([50, 90]),
            to: 10,
        },
    );
    assert_eq!(e.current, before);
}

#[test]
fn gradient_colors_multikey_move_rejects_empty_missing_collision_bounds_and_checked_overflow() {
    let (mut e, _, _) = interpolation_scene();
    for (frames, to) in [
        (BTreeSet::new(), 0),
        (BTreeSet::from([10, 11]), 10),
        (BTreeSet::from([10]), 50),
        (BTreeSet::from([10, 50]), 110),
        (BTreeSet::from([10, 50]), 150),
        (BTreeSet::from([10, 50]), u32::MAX - 1),
    ] {
        reject(&mut e, command(GradientColorsEdit::MoveKeys { frames, to }));
    }
    // The anchor is itself in range, but adding the second offset overflows.
    let mut n = node(e.project()).clone();
    let source = n.clone();
    assert!(
        gradient_colors::edit(
            &mut n,
            &GradientColorsEdit::MoveKeys {
                frames: BTreeSet::from([10, 50]),
                to: u32::MAX - 20
            },
            u32::MAX
        )
        .is_err()
    );
    assert_eq!(n, source);
    // Endpoints are composition-scoped, not restricted to the layer's 0..100 range.
    e.current.project.composition.duration = 150;
    edit(
        &mut e,
        GradientColorsEdit::MoveKeys {
            frames: BTreeSet::from([10, 50]),
            to: 109,
        },
    );
    assert_eq!(
        animation(e.project())
            .keys()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [109, 149]
    );
}

#[test]
fn gradient_colors_multikey_delete_bakes_original_interpolated_playhead_in_one_history_step() {
    let (mut e, start, end) = interpolation_scene();
    mode(&mut e, 10, Linear);
    mode(&mut e, 50, Smooth);
    e.clear_history();
    let before = e.current.clone();
    edit(
        &mut e,
        GradientColorsEdit::DeleteKeys {
            frames: BTreeSet::from([10, 50]),
            frame: 20,
        },
    );
    let baked = e.current.clone();
    assert!(
        node(e.project())
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .is_none()
    );
    assert_eq!(e.undo.len(), 1);
    assert_eq!(e.project().version, 57);
    let sample = colors(e.project(), 0);
    assert_ne!(sample, start);
    assert_ne!(sample, end);
    assert_eq!(sample.colors[0].red, 63.75);
    assert_eq!(sample.colors[0].position, 25.);
    assert_eq!(sample.opacities[0].opacity, 25.);
    for frame in [0, 20, 50, 149] {
        assert_eq!(colors(e.project(), frame), sample);
    }
    assert!(
        node(e.project())
            .parameters
            .values()
            .all(|track| track.keys.is_empty())
    );
    e.undo();
    assert_eq!(e.current, before);
    e.redo();
    assert_eq!(e.current, baked);
}

#[test]
fn gradient_colors_multikey_delete_preserves_surviving_snapshot_and_dormant_mode() {
    let (mut e, _, end) = interpolation_scene();
    edit(&mut e, GradientColorsEdit::ToggleKey { frame: 90 });
    mode(&mut e, 10, Linear);
    mode(&mut e, 50, Smooth);
    mode(&mut e, 90, Linear);
    edit(
        &mut e,
        GradientColorsEdit::DeleteKeys {
            frames: BTreeSet::from([10, 90]),
            frame: 30,
        },
    );
    assert_eq!(animation(e.project()).keys(), &BTreeMap::from([(50, end)]));
    assert_eq!(
        gradient_json(e.project())["colors_animation"]["outgoing_interpolation"],
        serde_json::json!({"50": "Smooth"})
    );
    for (frames, frame) in [
        (BTreeSet::new(), 0),
        (BTreeSet::from([10, 50]), 20),
        (BTreeSet::from([50]), 150),
    ] {
        reject(
            &mut e,
            command(GradientColorsEdit::DeleteKeys { frames, frame }),
        );
    }
}

#[test]
fn gradient_colors_multikey_modes_are_atomic_sparse_and_only_promote_materialized_metadata() {
    let (mut e, _, _) = interpolation_scene();
    let source = e.current.clone();
    for frames in [
        BTreeSet::new(),
        BTreeSet::from([10, 11, 50]),
        BTreeSet::from([150]),
    ] {
        reject(
            &mut e,
            command(GradientColorsEdit::SetInterpolations {
                frames,
                interpolation: Linear,
            }),
        );
    }
    edit(
        &mut e,
        GradientColorsEdit::SetInterpolations {
            frames: BTreeSet::from([10, 50]),
            interpolation: Hold,
        },
    );
    assert_eq!(e.current, source);
    assert!(!e.can_undo());
    edit(
        &mut e,
        GradientColorsEdit::SetInterpolations {
            frames: BTreeSet::from([10, 50]),
            interpolation: Smooth,
        },
    );
    assert_eq!(e.project().version, 57);
    assert_eq!(e.undo.len(), 1);
    assert_eq!(
        gradient_json(e.project())["colors_animation"]["outgoing_interpolation"],
        serde_json::json!({"10": "Smooth", "50": "Smooth"})
    );
    assert_eq!(
        animation(e.project())
            .segment_status(50)
            .unwrap()
            .hold_reason,
        Some(GradientColorsHoldReason::NoNextKey)
    );
    edit(
        &mut e,
        GradientColorsEdit::SetInterpolations {
            frames: BTreeSet::from([10, 50]),
            interpolation: Hold,
        },
    );
    assert!(
        !gradient_json(e.project())
            .to_string()
            .contains("outgoing_interpolation")
    );
    assert_eq!(e.project().version, 57);
}

#[test]
fn gradient_colors_multikey_paste_preserves_local_ids_topology_modes_and_advances_allocator() {
    let (mut e, start, mut end) = interpolation_scene();
    // A previously copied local key may reintroduce IDs after Copy -> Undo.
    let mut extra = end.colors[0].clone();
    extra.id = 500;
    extra.position = 33.;
    end.colors.push(extra);
    e.clear_history();
    let before = e.current.clone();
    let keys = vec![key_copy(20, &end, Smooth), key_copy(0, &start, Linear)];
    edit(&mut e, GradientColorsEdit::PasteKeys { keys, frame: 100 });
    assert_eq!(e.undo.len(), 1);
    assert_eq!(e.project().version, 57);
    assert_eq!(node(e.project()).kind.gradient().unwrap().next_stop, 501);
    assert_eq!(animation(e.project()).keys().get(&100), Some(&start));
    assert_eq!(animation(e.project()).keys().get(&120), Some(&end));
    assert_eq!(animation(e.project()).interpolation(100), Some(Linear));
    assert_eq!(animation(e.project()).interpolation(120), Some(Smooth));
    assert_eq!(
        animation(e.project())
            .segment_status(100)
            .unwrap()
            .hold_reason,
        Some(GradientColorsHoldReason::IncompatibleTopology)
    );
    let mut expected = before.clone();
    expected.project.version = 57;
    let g = node_mut(&mut expected.project).kind.gradient_mut().unwrap();
    let mut expected_json = serde_json::to_value(g.colors_animation()).unwrap();
    expected_json["keys"]["100"] = serde_json::to_value(&start).unwrap();
    expected_json["keys"]["120"] = serde_json::to_value(&end).unwrap();
    expected_json["outgoing_interpolation"] = serde_json::json!({"100": "Linear", "120": "Smooth"});
    g.colors_animation = Some(serde_json::from_value(expected_json).unwrap());
    g.next_stop = 501;
    assert_eq!(e.current, expected);
    e.undo();
    assert_eq!(e.current, before);
    e.redo();
    assert_eq!(e.current, expected);
    edit(
        &mut e,
        GradientColorsEdit::AddStop {
            frame: 130,
            opacity: true,
            position: 50.,
        },
    );
    assert_eq!(colors(e.project(), 130).opacities.last().unwrap().id, 501);
}

#[test]
fn gradient_colors_multikey_paste_hold_and_dormant_modes_choose_the_existing_schema_contract() {
    for interpolation in [Hold, Linear, Smooth] {
        let (mut e, start, _) = interpolation_scene();
        edit(
            &mut e,
            GradientColorsEdit::PasteKeys {
                keys: vec![key_copy(0, &start, interpolation)],
                frame: 90,
            },
        );
        assert_eq!(
            e.project().version,
            if interpolation == Hold { 54 } else { 57 }
        );
        assert_eq!(
            animation(e.project()).interpolation(90),
            Some(interpolation)
        );
        assert_eq!(
            animation(e.project())
                .segment_status(90)
                .unwrap()
                .hold_reason,
            Some(GradientColorsHoldReason::NoNextKey)
        );
        let decoded = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(&decoded, e.project());
    }
}

#[test]
fn gradient_colors_multikey_paste_requires_unique_zero_origin_bounded_offsets_and_existing_animation()
 {
    let (mut e, start, end) = interpolation_scene();
    for keys in [
        vec![],
        vec![key_copy(1, &start, Hold)],
        vec![key_copy(0, &start, Hold), key_copy(0, &end, Linear)],
        vec![key_copy(0, &start, Hold), key_copy(50, &end, Hold)],
        vec![key_copy(0, &start, Hold), key_copy(u32::MAX, &end, Hold)],
        vec![key_copy(0, &start, Hold); 1001],
    ] {
        reject(
            &mut e,
            command(GradientColorsEdit::PasteKeys { keys, frame: 100 }),
        );
    }
    reject(
        &mut e,
        command(GradientColorsEdit::PasteKeys {
            keys: vec![key_copy(0, &start, Hold)],
            frame: 150,
        }),
    );
    let mut n = node(e.project()).clone();
    let source = n.clone();
    assert!(
        gradient_colors::edit(
            &mut n,
            &GradientColorsEdit::PasteKeys {
                keys: vec![key_copy(0, &start, Hold), key_copy(20, &end, Hold)],
                frame: u32::MAX - 10
            },
            u32::MAX
        )
        .is_err()
    );
    assert_eq!(n, source);
    let mut static_scene = scene();
    for action in actions(&start) {
        reject(&mut static_scene, command(action));
    }
}

#[test]
fn gradient_colors_multikey_paste_rejects_any_partial_collision_mode_or_signed_zero_change() {
    let (mut e, start, end) = interpolation_scene();
    for keys in [
        vec![key_copy(0, &start, Hold), key_copy(20, &end, Hold)],
        vec![key_copy(0, &start, Hold), key_copy(40, &end, Linear)],
        vec![key_copy(0, &end, Hold), key_copy(40, &end, Hold)],
    ] {
        reject(
            &mut e,
            command(GradientColorsEdit::PasteKeys { keys, frame: 10 }),
        );
    }
    let mut zero_changed = start.clone();
    zero_changed.colors[0].position = 0.;
    assert_ne!(zero_changed, start);
    reject(
        &mut e,
        command(GradientColorsEdit::PasteKeys {
            keys: vec![key_copy(0, &zero_changed, Hold)],
            frame: 10,
        }),
    );
}

#[test]
fn gradient_colors_multikey_noops_and_reversible_batches_keep_exact_redo_and_schema() {
    for version in [54, 55, 56, 57] {
        let (mut e, start, end) = interpolation_scene();
        e.current.project.version = version;
        mode(&mut e, 10, Linear);
        e.undo();
        let (before, undo, redo) = (e.current.clone(), e.undo.clone(), e.redo.clone());
        let bytes = e.project().to_json().unwrap();
        for action in [
            GradientColorsEdit::MoveKeys {
                frames: BTreeSet::from([10, 50]),
                to: 10,
            },
            GradientColorsEdit::SetInterpolations {
                frames: BTreeSet::from([10, 50]),
                interpolation: Hold,
            },
            GradientColorsEdit::PasteKeys {
                keys: vec![key_copy(40, &end, Hold), key_copy(0, &start, Hold)],
                frame: 10,
            },
        ] {
            edit(&mut e, action);
        }
        e.execute(Command::Batch(vec![
            command(GradientColorsEdit::MoveKeys {
                frames: BTreeSet::from([10, 50]),
                to: 20,
            }),
            command(GradientColorsEdit::SetInterpolations {
                frames: BTreeSet::from([20, 60]),
                interpolation: Smooth,
            }),
            command(GradientColorsEdit::SetInterpolations {
                frames: BTreeSet::from([20, 60]),
                interpolation: Hold,
            }),
            command(GradientColorsEdit::MoveKeys {
                frames: BTreeSet::from([20, 60]),
                to: 10,
            }),
            command(GradientColorsEdit::PasteKeys {
                keys: vec![key_copy(0, &start, Smooth)],
                frame: 90,
            }),
            command(GradientColorsEdit::DeleteKeys {
                frames: BTreeSet::from([90]),
                frame: 30,
            }),
        ]))
        .unwrap();
        assert_eq!(e.current, before);
        assert_eq!(e.project().to_json().unwrap(), bytes);
        assert_eq!(e.undo, undo);
        assert_eq!(e.redo, redo);
    }
}

#[test]
fn gradient_colors_multikey_paste_validates_all_values_and_local_identity_roles_atomically() {
    let (mut e, start, _) = interpolation_scene();
    let mut invalid = Vec::new();
    for id in [0, u64::MAX, 3] {
        let mut c = start.clone();
        c.colors[0].id = id;
        invalid.push(c);
    }
    let mut c = start.clone();
    c.colors[0].red = f64::NAN;
    invalid.push(c);
    let mut c = start.clone();
    c.opacities[0].opacity = 101.;
    invalid.push(c);
    let mut c = start.clone();
    c.colors.pop();
    invalid.push(c);
    let mut c = start.clone();
    c.colors[1].id = 1;
    invalid.push(c);
    for colors in invalid {
        reject(
            &mut e,
            command(GradientColorsEdit::PasteKeys {
                keys: vec![key_copy(0, &start, Hold), key_copy(10, &colors, Smooth)],
                frame: 90,
            }),
        );
    }
    let mut color_role = start.clone();
    color_role.colors[0].id = 100;
    let mut opacity_role = start.clone();
    opacity_role.opacities[0].id = 100;
    reject(
        &mut e,
        command(GradientColorsEdit::PasteKeys {
            keys: vec![
                key_copy(0, &color_role, Hold),
                key_copy(10, &opacity_role, Hold),
            ],
            frame: 90,
        }),
    );
}

#[test]
fn gradient_colors_multikey_key_and_stored_stop_budgets_reject_without_partial_paste() {
    let mut e = scene();
    let mut sample = colors(e.project(), 0);
    install_keys(&mut e, &sample, 999);
    reject(
        &mut e,
        command(GradientColorsEdit::PasteKeys {
            keys: vec![key_copy(0, &sample, Hold), key_copy(1, &sample, Hold)],
            frame: 1000,
        }),
    );
    edit(
        &mut e,
        GradientColorsEdit::PasteKeys {
            keys: vec![key_copy(0, &sample, Hold)],
            frame: 1000,
        },
    );
    assert_eq!(animation(e.project()).keys().len(), 1000);
    // An all-exact no-op still works at the key budget.
    edit(
        &mut e,
        GradientColorsEdit::PasteKeys {
            keys: vec![key_copy(0, &sample, Hold)],
            frame: 1000,
        },
    );
    for id in 5..35 {
        let mut stop = sample.colors[0].clone();
        stop.id = id;
        sample.colors.push(stop);
    }
    for id in 35..65 {
        let mut stop = sample.opacities[0].clone();
        stop.id = id;
        sample.opacities.push(stop);
    }
    install_keys(&mut e, &sample, 512);
    reject(
        &mut e,
        command(GradientColorsEdit::PasteKeys {
            keys: vec![key_copy(0, &sample, Linear)],
            frame: 1000,
        }),
    );
    edit(
        &mut e,
        GradientColorsEdit::MoveKeys {
            frames: BTreeSet::from([0, 1]),
            to: 1000,
        },
    );
    assert_eq!(animation(e.project()).keys().len(), 512);
}

#[test]
fn gradient_colors_multikey_rejects_locked_missing_and_invalid_original_source_for_every_variant() {
    let (mut e, sample, _) = interpolation_scene();
    e.execute(Command::ToggleLocked(1)).unwrap();
    for action in actions(&sample) {
        reject(&mut e, command(action));
    }
    e.execute(Command::ToggleLocked(1)).unwrap();
    for action in actions(&sample) {
        reject(
            &mut e,
            Command::Contents {
                id: 1,
                edit: ContentsEdit::GradientColors {
                    item: 999,
                    edit: action,
                },
            },
        );
    }
    e.current.project.version = 53;
    for action in actions(&sample) {
        reject(&mut e, command(action));
    }
    e.current.project.version = 54;
    let g = node_mut(&mut e.current.project)
        .kind
        .gradient_mut()
        .unwrap();
    let mut invalid = serde_json::to_value(&g.colors_animation).unwrap();
    invalid["outgoing_interpolation"] = serde_json::json!({"11": "Smooth"});
    g.colors_animation = Some(serde_json::from_value(invalid).unwrap());
    e.current.project.version = 57;
    for action in actions(&sample) {
        reject(&mut e, command(action));
    }
}

#[test]
fn gradient_colors_multikey_rejects_mixed_empty_excessive_and_deep_batches_for_every_variant() {
    let (mut e, sample, _) = interpolation_scene();
    for action in actions(&sample) {
        reject(
            &mut e,
            Command::Batch(vec![command(action.clone()), Command::AddNull]),
        );
        reject(
            &mut e,
            Command::Batch(vec![command(action.clone()), Command::Batch(vec![])]),
        );
        let mut deep = command(action.clone());
        for _ in 0..65 {
            deep = Command::Batch(vec![deep]);
        }
        reject(&mut e, deep);
        reject(&mut e, Command::Batch(vec![command(action); 10000]));
    }
}

#[test]
fn gradient_colors_multikey_edits_preserve_unrelated_media_assets_and_project_source() {
    let (mut e, sample, _) = interpolation_scene();
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
    let before = e.current.clone();
    assert_eq!(before.project.asset_library.assets().len(), 1);
    for action in actions(&sample) {
        e.current = before.clone();
        e.clear_history();
        let expected_version = match action {
            GradientColorsEdit::SetInterpolations { .. } | GradientColorsEdit::PasteKeys { .. } => {
                57
            }
            _ => 54,
        };
        edit(&mut e, action);
        assert_eq!(e.project().version, expected_version);
        assert_eq!(e.project().asset_library, before.project.asset_library);
        // Only the explicitly edited gradient node and required schema may differ.
        let mut expected = before.clone();
        *node_mut(&mut expected.project) = node(e.project()).clone();
        expected.project.version = expected_version;
        assert_eq!(e.current, expected);
        assert_eq!(e.undo.len(), 1);
        e.undo();
        assert_eq!(e.current, before);
    }
}
