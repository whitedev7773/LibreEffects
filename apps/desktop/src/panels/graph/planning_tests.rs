use super::*;
use libre_effects_core::{
    EffectEdit, EffectKind, EffectParam, FrameRate, Property, TemporalHandle,
};

fn key(id: u64, property: Property, frame: u32) -> KeyRef {
    KeyRef {
        id,
        property: property.into(),
        frame,
    }
}
fn put(state: &mut EditorState, key: KeyRef, value: f64) {
    for edit in [
        TrackEdit::ToggleKey { frame: key.frame },
        TrackEdit::Value {
            frame: key.frame,
            value,
        },
    ] {
        state
            .editor
            .execute(Command::EditTrack {
                id: key.id,
                property: key.property,
                edit,
            })
            .unwrap();
    }
}
fn scene() -> (EditorState, Vec<KeyRef>) {
    let mut state = EditorState::default();
    for _ in 0..2 {
        state.editor.execute(Command::AddRectangle).unwrap();
    }
    let mut keys = Vec::new();
    for (id, property, values) in [
        (1, Property::PositionX, [1000.0, 2000.0]),
        (2, Property::Opacity, [20.0, 80.0]),
    ] {
        for (frame, value) in [10, 30].into_iter().zip(values) {
            let k = key(id, property, frame);
            put(&mut state, k, value);
            keys.push(k);
        }
        state
            .graph_pin_channel(channel(keys[keys.len() - 1]))
            .unwrap();
    }
    state.graph_activate_channel(channel(keys[0]), true);
    state.selected_keys = keys.iter().copied().collect();
    state.graph_key = Some(keys[0]);
    state.graph_open = true;
    state.editor.clear_history();
    (state, keys)
}
fn geometry() -> (View, Bounds<Pixels>, Point<Pixels>) {
    (
        View {
            start: 0.0,
            span: 100.0,
            low: -50.0,
            high: 2500.0,
        },
        Bounds::new(point(px(0.0), px(0.0)), size(px(1000.0), px(600.0))),
        point(px(100.0), px(200.0)),
    )
}

#[test]
fn native_opacity_is_rejected_before_graph_narrows_a_mixed_selection() {
    let mut state = EditorState::default();
    state.editor = crate::opacity_test_support::overshoot_editor(false);
    state.editor.execute(Command::AddRectangle).unwrap();
    let scalar = [
        key(2, Property::PositionX, 10),
        key(2, Property::PositionX, 30),
    ];
    for (key, value) in scalar.into_iter().zip([20.0, 80.0]) {
        put(&mut state, key, value);
    }
    state.graph_activate_channel(channel(scalar[0]), true);
    state.graph_open = true;
    state.graph_key = Some(scalar[0]);
    state.selected_keys = [scalar[0], scalar[1], key(1, Property::Opacity, 0)].into();
    state.editor.clear_history();
    let before = state.editor.project().clone();
    let keys = state.selected_keys.iter().copied().collect::<Vec<_>>();
    assert_eq!(selection::included(&state), scalar);
    assert!(
        selection::validate_scalar_selection(&state)
            .unwrap_err()
            .contains("Native Opacity")
    );
    assert!(selection::scale(&state, true, 2.0).is_err());
    assert!(EditPlan::paste(&state).is_err());
    let (view, bounds, start) = geometry();
    assert!(TimeGesture::new(&state, view, bounds, start, None).is_err());
    assert!(super::super::transform::Transform::new(&state, view, bounds, start).is_none());
    assert!(EditPlan::translate(&before, &keys, 5, state.graph_key).is_err());
    assert!(EditPlan::scale_time(&before, &keys, 10.0, 2.0, state.graph_key).is_err());
    assert!(EditPlan::scale_value(&before, &keys, 2.0, state.graph_key).is_err());
    assert!(EditPlan::delete(&before, &keys).is_err());
    assert!(EditPlan::interpolation(&before, &keys, Interpolation::Hold).is_err());
    assert!(EditPlan::temporal_mode(&before, &keys, TemporalMode::Auto).is_err());
    assert!(EditPlan::ease(&before, &keys, true, true).is_err());
    assert_eq!(state.editor.project(), &before);
    assert_eq!(
        state.selected_keys.iter().copied().collect::<Vec<_>>(),
        keys
    );
    assert!(!state.editor.can_undo());
}

#[test]
fn joined_position_is_rejected_before_graph_can_narrow_a_mixed_selection() {
    let mut state = EditorState::default();
    state.editor.execute(Command::AddRectangle).unwrap();
    state
        .editor
        .execute(Command::SetThreeD {
            id: 1,
            enabled: true,
        })
        .unwrap();
    state.editor.execute(Command::AddRectangle).unwrap();
    let scalar = [key(2, Property::Opacity, 10), key(2, Property::Opacity, 30)];
    for (key, value) in scalar.into_iter().zip([20.0, 80.0]) {
        put(&mut state, key, value);
    }
    state.graph_activate_channel(channel(scalar[0]), true);
    state.graph_open = true;
    state.graph_key = Some(scalar[0]);
    let stale_joined = key(1, Property::PositionX, 10);
    state.selected_keys = [scalar[0], scalar[1], stale_joined].into();
    state.editor.clear_history();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 2,
            name: "Redo branch".into(),
        })
        .unwrap();
    state.editor.undo();
    let before = state.editor.project().clone();
    let selection = state.selected_keys.clone();
    // The display may omit unavailable lanes, but action preflight must inspect
    // the raw selection before that filtering can erase the rejected member.
    assert_eq!(selection::included(&state), scalar);
    assert!(selection::validate_scalar_selection(&state).is_err());
    assert!(selection::scale(&state, true, 2.0).is_err());
    assert!(EditPlan::paste(&state).is_err());
    let (view, bounds, start) = geometry();
    assert!(TimeGesture::new(&state, view, bounds, start, None).is_err());
    assert!(super::super::transform::Transform::new(&state, view, bounds, start).is_none());
    let keys = state.selected_keys.iter().copied().collect::<Vec<_>>();
    assert!(EditPlan::translate(&before, &keys, 5, state.graph_key).is_err());
    assert!(EditPlan::scale_time(&before, &keys, 10.0, 2.0, state.graph_key).is_err());
    assert!(EditPlan::scale_value(&before, &keys, 2.0, state.graph_key).is_err());
    assert!(EditPlan::delete(&before, &keys).is_err());
    assert!(EditPlan::interpolation(&before, &keys, Interpolation::Hold).is_err());
    assert!(EditPlan::temporal_mode(&before, &keys, TemporalMode::Auto).is_err());
    assert!(EditPlan::ease(&before, &keys, true, true).is_err());
    assert_eq!(state.editor.project(), &before);
    assert_eq!(state.selected_keys, selection);
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
    state.selected_keys = scalar.into();
    assert!(selection::validate_scalar_selection(&state).is_ok());
    assert!(EditPlan::translate(&before, &scalar, 5, state.graph_key).is_ok());
}

#[test]
fn mixed_scalar_paste_into_joined_position_rejects_every_destination() {
    let mut state = EditorState::default();
    state.editor.execute(Command::AddRectangle).unwrap();
    state
        .editor
        .execute(Command::SetThreeD {
            id: 1,
            enabled: true,
        })
        .unwrap();
    state.editor.execute(Command::AddRectangle).unwrap();
    for (property, value) in [(Property::Opacity, 25.0), (Property::PositionX, 100.0)] {
        put(&mut state, key(2, property, 10), value);
    }
    let project = state.editor.project();
    let copies = [Property::Opacity, Property::PositionX].map(|property| {
        project
            .composition()
            .layer(2)
            .unwrap()
            .copy_key(property.into(), 10)
            .unwrap()
    });
    let destinations = [Property::Opacity, Property::PositionX].map(|property| GraphChannel {
        id: 1,
        property: property.into(),
    });
    let before = project.clone();
    assert!(
        EditPlan::paste_copies(
            project,
            &copies,
            &destinations,
            Some(destinations[0]),
            50,
            None,
            &[]
        )
        .is_err()
    );
    assert_eq!(state.editor.project(), &before);
    assert!(
        state
            .editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .property(Property::Opacity)
            .unwrap()
            .keys()
            .is_empty()
    );
}

#[test]
fn same_frame_different_lanes_are_distinct_and_vacated_destinations_are_legal() {
    let (mut state, keys) = scene();
    assert_eq!(selection::included(&state).len(), 4);
    let source = state.editor.project().clone();
    let plan = EditPlan::translate(&source, &keys, 20, Some(keys[2])).unwrap();
    assert_eq!(
        plan.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        vec![30, 50, 30, 50]
    );
    assert_eq!(
        plan.active,
        Some(KeyRef {
            frame: 30,
            ..keys[2]
        })
    );
    assert_eq!(plan.keys.iter().copied().collect::<BTreeSet<_>>().len(), 4);
    state.editor.execute(plan.command.unwrap()).unwrap();
    for (c, track) in plan.tracks {
        assert_eq!(
            state
                .editor
                .project()
                .composition()
                .layer(c.id)
                .unwrap()
                .track(c.property),
            Some(&track)
        );
    }
    state.editor.undo();
    assert_eq!(state.editor.project(), &source);
    state.editor.redo();
    let saved = Project::from_json(&state.editor.project().to_json().unwrap()).unwrap();
    assert_eq!(&saved, state.editor.project());
}
#[test]
fn one_track_collision_lock_stale_or_frame_error_rejects_entire_plan() {
    let (mut state, keys) = scene();
    put(&mut state, key(2, Property::Opacity, 50), 65.0);
    let before = state.editor.project().clone();
    assert!(EditPlan::translate(&before, &keys, 20, Some(keys[0])).is_err());
    assert!(EditPlan::translate(&before, &keys, -11, None).is_err());
    assert!(EditPlan::translate(&before, &keys, i64::MAX, None).is_err());
    let mut stale = keys.clone();
    stale.push(key(1, Property::PositionX, 11));
    assert!(EditPlan::translate(&before, &stale, 1, None).is_err());
    state.editor.execute(Command::ToggleLocked(2)).unwrap();
    let locked = state.editor.project().clone();
    assert!(EditPlan::translate(&locked, &keys, 1, None).is_err());
    assert!(EditPlan::delete(&locked, &keys).is_err());
    assert!(EditPlan::interpolation(&locked, &keys, Interpolation::Smooth).is_err());
    assert!(EditPlan::temporal_mode(&locked, &keys, TemporalMode::Auto).is_err());
    assert!(EditPlan::ease(&locked, &keys, true, true).is_err());
    assert_eq!(state.editor.project(), &locked);
}
#[test]
fn time_scaling_rounds_per_track_and_scales_slopes_without_mixing_units() {
    let (mut state, keys) = scene();
    for k in [keys[0], keys[2]] {
        state
            .editor
            .execute(Command::SetTemporalHandle {
                id: k.id,
                property: k.property,
                frame: k.frame,
                incoming: false,
                handle: TemporalHandle {
                    slope: 4.0,
                    influence: 0.25,
                },
            })
            .unwrap();
    }
    let before = state.editor.project().clone();
    for factor in [0.0, -1.0, f64::NAN, f64::INFINITY, 0.001] {
        assert!(EditPlan::scale_time(&before, &keys, 10.0, factor, None).is_err());
    }
    let plan = EditPlan::scale_time(&before, &keys, 10.0, 1.525, Some(keys[1])).unwrap();
    assert_eq!(
        plan.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        vec![10, 41, 10, 41]
    );
    assert_eq!(
        plan.active,
        Some(KeyRef {
            frame: 41,
            ..keys[1]
        })
    );
    for k in [keys[0], keys[2]] {
        let track = &plan.tracks[&channel(k)];
        assert_eq!(
            track.keys()[&10].value,
            before
                .composition()
                .layer(k.id)
                .unwrap()
                .track(k.property)
                .unwrap()
                .keys()[&10]
                .value
        );
        assert!((track.temporal_handle(10, false).unwrap().slope - 4.0 / 1.525).abs() < 1e-9);
    }
    state
        .editor
        .execute(Command::SetTemporalHandle {
            id: 1,
            property: keys[0].property,
            frame: 10,
            incoming: false,
            handle: TemporalHandle {
                slope: 1e9,
                influence: 0.25,
            },
        })
        .unwrap();
    assert!(EditPlan::scale_time(state.editor.project(), &keys, 10.0, 0.5, None).is_err());
}
#[test]
fn equal_time_multi_lane_selection_translates_but_cannot_scale() {
    let (mut state, keys) = scene();
    let keys = vec![keys[0], keys[2]];
    state.selected_keys = keys.iter().copied().collect();
    assert!(EditPlan::translate(state.editor.project(), &keys, 5, Some(keys[1])).is_ok());
    assert!(EditPlan::scale_time(state.editor.project(), &keys, 10.0, 2.0, None).is_err());
    let (view, bounds, start) = geometry();
    assert!(TimeGesture::new(&state, view, bounds, start, Some(1)).is_err());
    let mut draft = TimeGesture::new(&state, view, bounds, start, None).unwrap();
    draft.update_pointer(start + point(px(50.0), px(10000.0)), false, true);
    let plan = draft.preview.unwrap();
    assert!(plan.keys.iter().all(|k| k.frame == 15));
    for k in keys {
        assert_eq!(
            plan.tracks[&channel(k)].keys()[&15].value,
            state
                .editor
                .project()
                .composition()
                .layer(k.id)
                .unwrap()
                .track(k.property)
                .unwrap()
                .keys()[&10]
                .value
        );
    }
}
#[test]
fn grouped_snapping_ignores_other_lanes_occupancy_but_not_same_lane_occupancy() {
    let (mut state, keys) = scene();
    put(&mut state, key(2, Property::Opacity, 20), 50.0);
    state.selected_keys = [keys[0], keys[3]].into(); // x@10, opacity@30
    let (view, bounds, start) = geometry();
    let mut draft = TimeGesture::new(&state, view, bounds, start, None).unwrap();
    draft.targets = vec![20];
    draft.update_pointer(start + point(px(94.0), px(0.0)), false, false);
    assert_eq!(draft.guides.frame, Some(20));
    assert_eq!(
        draft
            .preview
            .unwrap()
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        vec![20, 40]
    );
    put(&mut state, key(1, Property::PositionX, 20), 3000.0);
    let mut draft = TimeGesture::new(&state, view, bounds, start, None).unwrap();
    draft.targets = vec![20];
    draft.update_pointer(start + point(px(94.0), px(0.0)), false, false);
    assert_eq!(draft.guides.frame, None);
    assert_eq!(draft.preview.unwrap().keys[0].frame, 19);
}
#[test]
fn no_op_and_cancel_preserve_serialized_source_assets_and_redo() {
    let (mut state, keys) = scene();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Renamed".into(),
        })
        .unwrap();
    state.editor.undo();
    assert!(state.editor.can_redo());
    let source = state.editor.project().to_json().unwrap();
    assert!(
        EditPlan::translate(state.editor.project(), &keys, 0, Some(keys[0]))
            .unwrap()
            .command
            .is_none()
    );
    assert!(
        EditPlan::scale_time(state.editor.project(), &keys, 10.0, 1.0, Some(keys[0]))
            .unwrap()
            .command
            .is_none()
    );
    let (view, bounds, start) = geometry();
    let mut draft = TimeGesture::new(&state, view, bounds, start, None).unwrap();
    draft.update_pointer(start + point(px(50.0), px(0.0)), false, true);
    draft.update_pointer(start, false, true);
    assert!(draft.preview.unwrap().command.is_none());
    assert_eq!(state.editor.project().to_json().unwrap(), source);
    assert!(state.editor.can_redo());
}
#[test]
fn stale_guard_freezes_full_source_identity_selection_views_and_modal_state() {
    for change in 0..14 {
        let (mut state, keys) = scene();
        let (view, bounds, start) = geometry();
        let draft = TimeGesture::new(&state, view, bounds, start, None).unwrap();
        assert!(draft.is_current(&state, Some(bounds)));
        match change {
            0 => state.frame += 1,
            1 => state.document_revision += 1,
            2 => {
                state.selected_keys.remove(&keys[1]);
            }
            3 => state.graph_key = Some(keys[1]),
            4 => {
                state.graph_activate_channel(channel(keys[2]), true);
            }
            5 => state.graph_view.speed = true,
            6 => state.graph_set_channel_height(channel(keys[2]), false, Some([-1.0, 100.0])),
            7 => state.timeline_start += 1,
            8 => state.timeline_zoom = 2.0,
            9 => state.tool = Tool::Hand,
            10 => state.fonts_open = true,
            11 => state.playing = true,
            12 => state.graph_open = false,
            _ => state
                .editor
                .execute(Command::RenameLayer {
                    id: 2,
                    name: "Changed without desktop revision".into(),
                })
                .unwrap(),
        }
        assert!(!draft.is_current(&state, Some(bounds)), "change {change}");
    }
    let (state, _) = scene();
    let (view, bounds, start) = geometry();
    let draft = TimeGesture::new(&state, view, bounds, start, None).unwrap();
    assert!(!draft.is_current(&state, None));
    assert!(!draft.is_current(
        &state,
        Some(Bounds::new(
            bounds.origin,
            size(px(999.0), bounds.size.height)
        ))
    ));
}
#[test]
fn lane_selection_and_fit_ignore_inspector_alias_and_keep_ordinate_ranges_separate() {
    let (mut state, keys) = scene();
    state.editor.select(2); // Persisted graph active need not equal Inspector head.
    assert_eq!(selection::active(&state), vec![keys[0], keys[1]]);
    state.timeline_start = 100;
    state.timeline_zoom = 64.0;
    assert_eq!(selection::included(&state).len(), 4); // Offscreen keys remain included.
    assert!(viewport::fit(&mut state, true));
    let x = state.graph_channel_height(channel(keys[0]), false).unwrap();
    let opacity = state.graph_channel_height(channel(keys[2]), false).unwrap();
    assert!(x[0] < 1000.0 && x[1] > 2000.0);
    assert!(opacity[0] < 20.0 && opacity[1] > 80.0 && opacity[1] < 100.0);
    assert!(state.timeline_start <= 10 && state.timeline_start + state.visible_frames() >= 30);
    let (view, bounds, start) = geometry();
    let pan = viewport::Pan::new(
        &state,
        view,
        bounds,
        &MouseDownEvent {
            position: start,
            ..Default::default()
        },
    );
    pan.apply(&mut state, start + point(px(0.0), px(30.0)));
    assert_eq!(
        state.graph_channel_height(channel(keys[2]), false),
        Some(opacity)
    );
    pan.restore(&mut state);
    assert_eq!(state.graph_channel_height(channel(keys[0]), false), Some(x));
}
#[test]
fn mixed_ordinate_edits_are_rejected_and_single_lane_helpers_are_unchanged() {
    let (state, keys) = scene();
    let samples: Vec<_> = keys
        .iter()
        .map(|key| selection::Sample {
            key: *key,
            value: 1.0,
            handle: None,
        })
        .collect();
    assert!(selection::translate(&samples, 0, 1.0, None).is_err());
    assert!(selection::scale(&state, false, 2.0).is_err());
    assert!(selection::translate(&samples[..2], 0, 1.0, None).is_ok());
}
#[test]
fn units_are_raw_and_effect_amounts_follow_their_kind_and_stable_id() {
    let (mut state, keys) = scene();
    for kind in [
        EffectKind::Brightness,
        EffectKind::Tint,
        EffectKind::HueSaturation,
        EffectKind::Glow,
        EffectKind::Brightness,
    ] {
        state
            .editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(kind),
            })
            .unwrap();
    }
    let project = state.editor.project();
    let descs: Vec<_> = project
        .composition()
        .layer(1)
        .unwrap()
        .effect_stack()
        .iter()
        .map(|e| {
            channels::describe(
                project,
                GraphChannel {
                    id: 1,
                    property: PropertyPath::Effect {
                        effect: e.id(),
                        parameter: EffectParam::Amount,
                    },
                },
            )
            .unwrap()
        })
        .collect();
    assert_eq!(
        descs
            .iter()
            .map(|d| d.units.label(false))
            .collect::<Vec<_>>(),
        vec!["ratio", "%", "ratio", "ratio", "ratio"]
    );
    assert_ne!(descs[0].label, descs[4].label);
    assert_eq!(
        channels::describe(project, channel(keys[0]))
            .unwrap()
            .units
            .label(true),
        "px/s"
    );
    assert_eq!(
        channels::describe(project, channel(keys[2]))
            .unwrap()
            .units
            .label(true),
        "%/s"
    );
}
#[test]
fn rational_fps_speed_fit_and_pointer_time_are_consistent() {
    let (mut state, keys) = scene();
    state
        .editor
        .execute(Command::ConfigureCompositionRate {
            name: "Rational".into(),
            width: 1920,
            height: 1080,
            fps: FrameRate::new(30000, 1001).unwrap(),
            duration: 150,
            display_start: 0,
        })
        .unwrap();
    state.graph_view.speed = true;
    assert!(viewport::fit(&mut state, true));
    for k in [keys[0], keys[2]] {
        let range = state.graph_channel_height(channel(k), true).unwrap();
        let track = state
            .editor
            .project()
            .composition()
            .layer(k.id)
            .unwrap()
            .track(k.property)
            .unwrap();
        for (_, v) in speed::ends(track, 10, 30000.0 / 1001.0) {
            assert!(v >= range[0] && v <= range[1]);
        }
    }
    let (view, bounds, start) = geometry();
    let mut draft = TimeGesture::new(&state, view, bounds, start, Some(1)).unwrap();
    draft.update_pointer(start + point(px(100.0), px(-999.0)), false, true);
    assert_eq!(
        draft
            .preview
            .unwrap()
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        vec![10, 40, 10, 40]
    );
}

#[test]
fn group_properties_and_ease_are_one_transaction_with_no_op_suppression() {
    let (mut state, keys) = scene();
    let before = state.editor.project().clone();
    let plan = EditPlan::interpolation(&before, &keys, Interpolation::Hold).unwrap();
    state.editor.execute(plan.command.unwrap()).unwrap();
    for key in &keys {
        assert_eq!(
            state
                .editor
                .project()
                .composition()
                .layer(key.id)
                .unwrap()
                .track(key.property)
                .unwrap()
                .keys()[&key.frame]
                .interpolation,
            Interpolation::Hold
        );
    }
    let hold = state.editor.project().clone();
    assert!(
        EditPlan::interpolation(&hold, &keys, Interpolation::Hold)
            .unwrap()
            .command
            .is_none()
    );
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
    state.editor.redo();
    assert_eq!(state.editor.project(), &hold);
    let ease = EditPlan::ease(&hold, &keys, true, true).unwrap();
    state.editor.execute(ease.command.unwrap()).unwrap();
    let eased = state.editor.project().clone();
    for key in &keys {
        let track = eased
            .composition()
            .layer(key.id)
            .unwrap()
            .track(key.property)
            .unwrap();
        for incoming in [true, false] {
            if let Some(handle) = track.temporal_handle(key.frame, incoming) {
                assert_eq!(handle.slope, 0.0);
            }
        }
    }
    assert!(
        EditPlan::ease(&eased, &keys, true, true)
            .unwrap()
            .command
            .is_none()
    );
    state.editor.undo();
    assert_eq!(state.editor.project(), &hold);
    let modes = EditPlan::temporal_mode(&hold, &keys, TemporalMode::Auto).unwrap();
    state.editor.execute(modes.command.unwrap()).unwrap();
    state.editor.undo();
    assert_eq!(state.editor.project(), &hold);
    let delete = EditPlan::delete(&hold, &keys).unwrap();
    assert!(delete.keys.is_empty() && delete.active.is_none());
    state.editor.execute(delete.command.unwrap()).unwrap();
    state.editor.undo();
    assert_eq!(state.editor.project(), &hold);
}

#[test]
fn time_scale_snapping_checks_rounded_destinations_per_track_and_release_modifiers() {
    let (state, keys) = scene();
    let (view, bounds, start) = geometry();
    let mut draft = TimeGesture::new(&state, view, bounds, start, Some(1)).unwrap();
    draft.targets = vec![40];
    let end = start + point(px(94.0), px(0.0));
    draft.update_pointer(end, false, false);
    assert_eq!(draft.guides.frame, Some(40));
    assert_eq!(
        draft
            .preview
            .as_ref()
            .unwrap()
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        vec![10, 40, 10, 40]
    );
    // Re-evaluating the release modifier bypasses the frozen snap switch.
    draft.update_pointer(end, false, true);
    assert_eq!(draft.guides.frame, None);
    assert_eq!(
        draft
            .preview
            .as_ref()
            .unwrap()
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        vec![10, 39, 10, 39]
    );
    draft.update_pointer(start, true, false);
    assert!(draft.preview.unwrap().command.is_none());
    assert_eq!(state.selected_keys, keys.iter().copied().collect());
}

#[test]
fn scalar_value_plans_keep_lane_units_and_suppress_identity_scaling() {
    let (state, keys) = scene();
    assert!(EditPlan::scale_value(state.editor.project(), &keys, 2.0, Some(keys[0])).is_err());
    assert!(
        EditPlan::scale_value(state.editor.project(), &keys[..2], 1.0, Some(keys[0]))
            .unwrap()
            .command
            .is_none()
    );
    let plan =
        EditPlan::scale_value(state.editor.project(), &keys[..2], -1.0, Some(keys[0])).unwrap();
    let track = &plan.tracks[&channel(keys[0])];
    assert_eq!(track.keys()[&10].value, 1000.0);
    assert_eq!(track.keys()[&30].value, 0.0);
    assert_eq!(plan.keys, keys[..2]);
    assert_eq!(plan.active, Some(keys[0]));
    assert!(EditPlan::scale_value(state.editor.project(), &keys[2..], 2.0, None).is_err()); // Opacity bound.
}

#[test]
fn alt_bypasses_translation_snap_but_centers_time_scale() {
    let (state, _) = scene();
    let (view, bounds, start) = geometry();
    let mut draft = TimeGesture::new(&state, view, bounds, start, None).unwrap();
    draft.targets = vec![20];
    let end = start + point(px(94.0), px(0.0));
    draft.update_pointer(end, false, false);
    assert_eq!(draft.guides.frame, Some(20));
    draft.update_pointer(end, true, false);
    assert_eq!(draft.guides.frame, None);
    assert_eq!(draft.preview.unwrap().keys[0].frame, 19);
    let mut scale = TimeGesture::new(&state, view, bounds, start, Some(1)).unwrap();
    scale.targets = vec![40];
    scale.update_pointer(end, true, false);
    assert_eq!(scale.guides.frame, Some(40));
    assert_eq!(
        scale
            .preview
            .unwrap()
            .keys
            .iter()
            .map(|k| k.frame)
            .collect::<Vec<_>>(),
        vec![0, 40, 0, 40]
    );
}

fn copied(state: &EditorState, keys: &[KeyRef]) -> Vec<KeyCopy> {
    keys.iter()
        .map(|key| {
            state
                .editor
                .project()
                .composition()
                .layer(key.id)
                .unwrap()
                .copy_key(key.property, key.frame)
                .unwrap()
        })
        .collect()
}
fn paste(state: &EditorState, copies: &[KeyCopy]) -> Result<EditPlan, String> {
    EditPlan::paste_copies(
        state.editor.project(),
        copies,
        &state.graph_included_channels(),
        state.graph_active_channel(),
        state.frame,
        state.graph_key,
        &selection::included(state),
    )
}
#[test]
fn graph_paste_single_source_targets_active_layer_but_requires_every_destination_included() {
    let (mut state, keys) = scene();
    let copies = copied(&state, &keys[..2]);
    state.graph_activate_channel(channel(keys[2]), true);
    state.frame = 50;
    assert!(
        paste(&state, &copies)
            .err()
            .unwrap()
            .contains("Pin or activate")
    );
    let destination = GraphChannel {
        id: 2,
        property: Property::PositionX.into(),
    };
    state.graph_pin_channel(destination).unwrap();
    let plan = paste(&state, &copies).unwrap();
    assert_eq!(
        plan.keys,
        vec![
            key(2, Property::PositionX, 50),
            key(2, Property::PositionX, 70)
        ]
    );
    assert_eq!(plan.active, Some(key(2, Property::PositionX, 50)));
    assert_eq!(plan.tracks.len(), 1);
    let before = state.editor.project().clone();
    state.editor.execute(plan.command.unwrap()).unwrap();
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
}
#[test]
fn graph_paste_multiple_sources_retains_identity_even_when_pinned_lanes_are_offscreen() {
    let (mut state, keys) = scene();
    let copies = copied(&state, &[keys[0], keys[3]]);
    state.frame = 50;
    state.timeline_start = 100;
    state.timeline_zoom = 64.0;
    let plan = paste(&state, &copies).unwrap();
    assert_eq!(
        plan.keys,
        vec![
            key(1, Property::PositionX, 50),
            key(2, Property::Opacity, 70)
        ]
    );
    assert_eq!(plan.tracks.len(), 2);
    // Clipboard scope alone determines pasted keys; unrelated selected keys are not copied.
    assert_eq!(plan.keys.len(), copies.len());
    state.graph_unpin_channel(channel(keys[2]));
    assert!(paste(&state, &copies).is_err());
}
#[test]
fn graph_cut_then_paste_does_not_require_copied_source_keys_to_still_exist() {
    let (mut state, keys) = scene();
    let cut = vec![keys[0], keys[3]];
    let copies = copied(&state, &cut);
    let before = state.editor.project().clone();
    state
        .editor
        .execute(Command::DeleteKeys(cut.clone()))
        .unwrap();
    state.selected_keys.clear();
    state.graph_key = None;
    state.frame = 10;
    let plan = paste(&state, &copies).unwrap();
    assert_eq!(plan.keys, cut);
    state.editor.execute(plan.command.unwrap()).unwrap();
    // Both tracks still had another key, so delete/paste restores their exact state.
    assert_eq!(state.editor.project(), &before);
}
#[test]
fn graph_empty_and_identical_occupied_paste_preserve_source_and_redo() {
    let (mut state, keys) = scene();
    let copies = copied(&state, &keys[..2]);
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Later".into(),
        })
        .unwrap();
    state.editor.undo();
    assert!(state.editor.can_redo());
    let before = state.editor.project().to_json().unwrap();
    let empty = EditPlan::paste(&state).unwrap();
    assert!(empty.command.is_none());
    assert_eq!(empty.keys, selection::included(&state));
    assert_eq!(empty.active, state.graph_key);
    state.frame = 10;
    // Core's no-overwrite rule also rejects an otherwise identical occupied paste.
    assert!(paste(&state, &copies).is_err());
    assert_eq!(state.editor.project().to_json().unwrap(), before);
    assert!(state.editor.can_redo());
}
#[test]
fn graph_paste_validates_all_destinations_atomically_and_does_not_expand_scope() {
    let (mut state, keys) = scene();
    let copies = copied(&state, &[keys[0], keys[3]]);
    state.frame = 50;
    put(&mut state, key(2, Property::Opacity, 70), 55.0);
    let before = state.editor.project().clone();
    let scope = state.graph_channels.clone();
    assert!(paste(&state, &copies).is_err());
    assert_eq!(state.editor.project(), &before);
    assert_eq!(state.graph_channels, scope);
    state.frame = 140;
    assert!(paste(&state, &copies).is_err());
    state.frame = 60;
    state.editor.execute(Command::ToggleLocked(2)).unwrap();
    assert!(paste(&state, &copies).is_err());
    let mut geometry = copies[..1].to_vec();
    geometry[0].key.property = PropertyPath::Path(libre_effects_core::PathTarget::Shape);
    assert!(paste(&state, &geometry).is_err());
}
#[test]
fn graph_paste_keeps_core_effect_kind_validation() {
    let (mut state, _) = scene();
    for (id, kind) in [(1, EffectKind::Brightness), (2, EffectKind::Tint)] {
        state
            .editor
            .execute(Command::Effect {
                id,
                edit: EffectEdit::Add(kind),
            })
            .unwrap();
    }
    let source = KeyRef {
        id: 1,
        property: PropertyPath::Effect {
            effect: 1,
            parameter: EffectParam::Amount,
        },
        frame: 10,
    };
    put(&mut state, source, 1.5);
    let copies = copied(&state, &[source]);
    let destination = GraphChannel {
        id: 2,
        property: source.property,
    };
    state.graph_pin_channel(destination).unwrap();
    state.graph_activate_channel(destination, true);
    state.frame = 50;
    let before = state.editor.project().clone();
    assert!(paste(&state, &copies).is_err());
    assert_eq!(state.editor.project(), &before);
}
