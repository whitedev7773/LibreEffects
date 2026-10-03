//! Speed Graph time-box regressions. These exercise the same detached preview,
//! pointer-finalization and command paths used by the native graph.
use super::*;
use libre_effects_core::{FrameRate, Project, Property, TemporalHandle};

fn scene(frames: &[u32]) -> (EditorState, View, Bounds<Pixels>) {
    let mut state = EditorState::default();
    state
        .editor
        .execute(Command::ConfigureCompositionRate {
            name: "Speed time box".into(),
            width: 1280,
            height: 720,
            fps: FrameRate::new(30_000, 1001).unwrap(),
            duration: 150,
            display_start: 0,
        })
        .unwrap();
    state.editor.execute(Command::AddRectangle).unwrap();
    state.graph_property = Property::PositionX.into();
    state.graph_view.speed = true;
    state.snapping = false;
    for (&frame, value) in frames.iter().zip([200.0, 500.0, 900.0]) {
        edit(&mut state, TrackEdit::ToggleKey { frame });
        edit(&mut state, TrackEdit::Value { frame, value });
        state.selected_keys.insert(KeyRef {
            id: 1,
            property: state.graph_property,
            frame,
        });
    }
    state.graph_key = frames.get(1).map(|&frame| (1, frame));
    state.editor.clear_history();
    (
        state,
        View {
            start: 0.0,
            span: 150.0,
            low: -1500.0,
            high: 1500.0,
        },
        Bounds::new(point(px(100.0), px(100.0)), size(px(1000.0), px(400.0))),
    )
}
fn edit(state: &mut EditorState, edit: TrackEdit) {
    state
        .editor
        .execute(Command::EditTrack {
            id: 1,
            property: state.graph_property,
            edit,
        })
        .unwrap();
}
fn track(state: &EditorState) -> &AnimatedProperty {
    state
        .editor
        .selected_layer()
        .unwrap()
        .track(state.graph_property)
        .unwrap()
}
fn area(state: &EditorState) -> SelectionBox {
    SelectionBox::new(
        track(state),
        &selection::active(state).iter().map(|k| k.frame).collect(),
        true,
        state.editor.project().composition().fps().as_f64(),
    )
    .unwrap()
}
fn grab(state: &EditorState, view: View, bounds: Bounds<Pixels>, side: i8) -> Transform {
    let p = area(state)
        .handles(view, bounds)
        .into_iter()
        .find(|(h, _)| *h == Handle(side, 0))
        .unwrap()
        .1;
    Transform::new(state, view, bounds, p + point(px(2.0), px(-1.0))).unwrap()
}
fn manual(state: &mut EditorState) {
    let frames: Vec<_> = track(state).keys().keys().copied().collect();
    for (i, &frame) in frames.iter().enumerate() {
        for incoming in [true, false] {
            if (incoming && i == 0) || (!incoming && i + 1 == frames.len()) {
                continue;
            }
            state
                .editor
                .execute(Command::SetTemporalHandle {
                    id: 1,
                    property: state.graph_property,
                    frame,
                    incoming,
                    handle: TemporalHandle {
                        slope: if incoming { -12.0 } else { 18.0 },
                        influence: if incoming { 0.4 } else { 0.25 },
                    },
                })
                .unwrap();
        }
    }
}
fn assert_preserved(
    old: &AnimatedProperty,
    new: &AnimatedProperty,
    scale: KeyScale,
    frames: &[u32],
) {
    assert_eq!(
        serde_json::to_value(old).unwrap()["value"],
        serde_json::to_value(new).unwrap()["value"]
    );
    assert_eq!(new.keys().len(), old.keys().len());
    for (&frame, key) in old.keys() {
        let mut expected = key.clone();
        let to = if frames.contains(&frame) {
            for handle in [
                &mut expected.temporal.incoming,
                &mut expected.temporal.outgoing,
            ]
            .into_iter()
            .flatten()
            {
                handle.slope *= 1.0 / scale.time_scale;
            }
            scale.frame(frame, 150).unwrap()
        } else {
            frame
        };
        assert_eq!(new.keys()[&to], expected);
    }
    assert_eq!(scale.value_origin, 0.0);
    assert_eq!(scale.value_scale, 1.0);
}

#[test]
fn speed_box_uses_signed_finite_endpoints_in_rational_fps_units() {
    let (mut state, _, _) = scene(&[30, 50, 60]);
    edit(
        &mut state,
        TrackEdit::Value {
            frame: 60,
            value: 100.0,
        },
    );
    let fps = state.editor.project().composition().fps().as_f64();
    let b = area(&state);
    assert_eq!((b.first, b.last), (30.0, 60.0));
    assert_eq!((b.low, b.high), (-40.0 * fps, 15.0 * fps));
    assert_eq!(
        speed::ends(track(&state), 30, fps),
        vec![(false, 15.0 * fps)]
    );
    assert_eq!(
        speed::ends(track(&state), 60, fps),
        vec![(true, -40.0 * fps)]
    );
    assert!(SelectionBox::new(track(&state), &[30].into(), true, fps).is_none());
    assert!(SelectionBox::new(track(&state), &[30, 99].into(), true, fps).is_none());
}

#[test]
fn speed_box_has_only_horizontal_hit_targets_and_clips_pointer_hits() {
    let (mut state, view, bounds) = scene(&[30, 50, 60]);
    manual(&mut state);
    let b = area(&state);
    let handles = b.handles(view, bounds);
    assert_eq!(
        handles.iter().map(|(h, _)| *h).collect::<Vec<_>>(),
        vec![Handle(-1, 0), Handle(1, 0)]
    );
    for &(handle, at) in &handles {
        assert_eq!(b.hit(view, bounds, at), Some(handle));
    }
    let box_bounds = b.area(view, bounds);
    for y in [box_bounds.top(), box_bounds.bottom()] {
        for x in [
            box_bounds.left(),
            box_bounds.left() + box_bounds.size.width / 2.0,
            box_bounds.right(),
        ] {
            assert_eq!(b.hit(view, bounds, point(x, y)), None);
        }
    }
    let narrow = Bounds::new(handles[0].1, size(px(1.0), px(1.0)));
    let outside = point(narrow.left() - px(1.0), narrow.top());
    assert!(b.hit(view, narrow, outside).is_none());
}

#[test]
fn speed_box_flat_hold_and_singular_endpoints_keep_zero_line_handles() {
    for interpolation in [
        Interpolation::Smooth,
        Interpolation::Hold,
        Interpolation::Bezier(Bezier {
            x1: 0.0,
            y1: 0.5,
            x2: 1.0,
            y2: 0.5,
        }),
    ] {
        let (mut state, view, bounds) = scene(&[30, 60]);
        edit(
            &mut state,
            TrackEdit::Interpolate {
                frame: 30,
                interpolation,
            },
        );
        let before = track(&state).clone();
        let b = area(&state);
        assert_eq!((b.low, b.high), (0.0, 0.0));
        let handles = b.handles(view, bounds);
        assert_eq!(handles.len(), 2);
        assert!(
            handles
                .iter()
                .all(|(_, p)| p.y == view.point(bounds, 30.0, 0.0).y)
        );
        if matches!(interpolation, Interpolation::Bezier(_)) {
            assert!(speed::ends(&before, 30, 30.0).is_empty());
            assert!(speed::ends(&before, 60, 30.0).is_empty());
        }
        let mut t = grab(&state, view, bounds, 1);
        t.moving(t.start + point(px(100.0), px(-4000.0)), false, false);
        let (scale, preview) = t.preview.as_ref().unwrap();
        assert_preserved(&before, preview, *scale, &[30, 60]);
        assert_eq!(preview.keys()[&30].interpolation, interpolation);
    }
    let (mut state, view, bounds) = scene(&[30, 60]);
    edit(
        &mut state,
        TrackEdit::Value {
            frame: 60,
            value: 200.0,
        },
    );
    assert_eq!((area(&state).low, area(&state).high), (0.0, 0.0));
    // Even a vertically panned viewport where zero is outside retains usable handles.
    let panned = View {
        low: 100.0,
        high: 200.0,
        ..view
    };
    assert!(
        area(&state)
            .handles(panned, bounds)
            .iter()
            .all(|(_, p)| bounds.contains(p))
    );
}

#[test]
fn speed_box_boundary_handles_are_visible_without_changing_true_pivots_or_offsets() {
    let (state, _, bounds) = scene(&[0, 149]);
    let view = View {
        start: 0.0,
        span: 149.0,
        low: -1000.0,
        high: 1000.0,
    };
    let b = area(&state);
    let h = b.handles(view, bounds);
    assert_eq!(h[0].1.x, bounds.left() + px(4.0));
    assert_eq!(h[1].1.x, bounds.right() - px(4.0));
    for side in [-1, 1] {
        let mut t = grab(&state, view, bounds, side);
        t.moving(t.start, false, false);
        assert!(!t.has_changes());
        t.moving(
            t.start + point(px(side as f32 * -100.0), px(800.0)),
            false,
            false,
        );
        let (scale, _) = t.preview.as_ref().unwrap();
        assert_eq!(scale.time_origin, if side < 0 { 149.0 } else { 0.0 });
        assert!((scale.time_scale - 0.9).abs() < 1e-9);
    }
    // Value Graph's original decorative handle positions are unchanged.
    let values = SelectionBox::new(track(&state), &[0, 149].into(), false, 30.0).unwrap();
    assert_eq!(values.area(view, bounds).left(), bounds.left() - px(10.0));
}

#[test]
fn speed_box_left_right_and_alt_scale_ignore_all_vertical_motion() {
    for frames in [&[30, 60][..], &[30, 50, 60][..]] {
        let (state, view, bounds) = scene(frames);
        for side in [-1, 1] {
            for center in [false, true] {
                let mut t = grab(&state, view, bounds, side);
                let dx = px(side as f32 * 100.0);
                t.moving(t.start + point(dx, px(0.0)), center, false);
                let (scale, expected) = t.preview.as_ref().unwrap();
                assert_eq!(
                    scale.time_origin,
                    if center {
                        45.0
                    } else if side < 0 {
                        60.0
                    } else {
                        30.0
                    }
                );
                assert!((scale.time_scale - if center { 2.0 } else { 1.5 }).abs() < 1e-9);
                assert_preserved(track(&state), expected, *scale, frames);
                let expected = expected.clone();
                for y in [-5000.0, 9000.0, f32::NAN] {
                    t.moving(t.start + point(dx, px(y)), center, false);
                    assert_eq!(t.preview.as_ref().unwrap().1, expected);
                }
            }
        }
    }
}

#[test]
fn speed_box_all_interpolation_and_temporal_modes_share_preview_commit_and_roundtrip() {
    for frames in [&[30, 60][..], &[30, 50, 60][..]] {
        for kind in 0..7 {
            let (mut state, view, bounds) = scene(frames);
            match kind {
                1 | 2 | 6 => {
                    let interpolation = match kind {
                        1 => Interpolation::Smooth,
                        2 => Interpolation::Bezier(Bezier {
                            x1: 0.2,
                            y1: -0.3,
                            x2: 0.7,
                            y2: 1.2,
                        }),
                        _ => Interpolation::Hold,
                    };
                    for &frame in frames {
                        edit(
                            &mut state,
                            TrackEdit::Interpolate {
                                frame,
                                interpolation,
                            },
                        );
                    }
                }
                3 | 4 => {
                    manual(&mut state);
                    if kind == 4 {
                        for &frame in frames {
                            state
                                .editor
                                .execute(Command::SetTemporalMode {
                                    id: 1,
                                    property: state.graph_property,
                                    frame,
                                    mode: TemporalMode::Continuous,
                                })
                                .unwrap();
                        }
                    }
                }
                5 => {
                    for &frame in frames {
                        state
                            .editor
                            .execute(Command::SetTemporalMode {
                                id: 1,
                                property: state.graph_property,
                                frame,
                                mode: TemporalMode::Auto,
                            })
                            .unwrap();
                    }
                }
                _ => {}
            }
            state.editor.clear_history();
            let before = state.editor.project().clone();
            let old = track(&state).clone();
            let mut t = grab(&state, view, bounds, 1);
            // The last release coordinate can be far outside the canvas vertically.
            t.update_pointer(t.start + point(px(100.0), px(8000.0)), false, false);
            let (scale, expected) = t.preview.as_ref().unwrap();
            assert_preserved(&old, expected, *scale, frames);
            for &frame in frames {
                let to = scale.frame(frame, 150).unwrap();
                for incoming in [false, true] {
                    match (
                        old.velocity(frame as f64, incoming),
                        expected.velocity(to as f64, incoming),
                    ) {
                        (Some(a), Some(b)) => assert!((a / scale.time_scale - b).abs() < 1e-7),
                        (None, None) => {}
                        other => panic!("Changed endpoint availability: {other:?}"),
                    }
                }
            }
            let expected = expected.clone();
            let (command, keys) = t.command().unwrap();
            assert!(matches!(command, Command::ScaleKeys { .. }));
            assert_eq!(
                keys.iter().map(|k| k.frame).collect::<BTreeSet<_>>(),
                t.frames()
            );
            assert_eq!(
                keys[t.active].frame,
                if frames.len() == 3 { 60 } else { 75 }
            );
            state.editor.execute(command).unwrap();
            state.selected_keys = keys.into_iter().collect();
            assert_eq!(selection::active(&state).len(), frames.len());
            assert_eq!(track(&state), &expected);
            assert!(state.editor.can_undo());
            let saved = Project::from_json(&state.editor.project().to_json().unwrap()).unwrap();
            state.editor.undo();
            assert_eq!(state.editor.project(), &before);
            assert!(!state.editor.can_undo());
            state.editor.redo();
            assert_eq!(state.editor.project(), &saved);
            assert!(!state.editor.can_redo());
        }
    }
}

#[test]
fn speed_box_auto_recomputes_partial_selection_without_creating_manual_handles() {
    let (mut state, view, bounds) = scene(&[30, 50, 60]);
    for frame in [30, 50, 60] {
        state
            .editor
            .execute(Command::SetTemporalMode {
                id: 1,
                property: state.graph_property,
                frame,
                mode: TemporalMode::Auto,
            })
            .unwrap();
    }
    state.selected_keys.retain(|k| k.frame >= 50);
    let old = track(&state).clone();
    let mut t = grab(&state, view, bounds, 1);
    t.moving(t.start + point(px(100.0), px(3000.0)), false, false);
    let (scale, preview) = t.preview.as_ref().unwrap();
    assert_eq!(scale.time_scale, 2.5);
    assert_preserved(&old, preview, *scale, &[50, 60]);
    assert!(
        preview
            .keys()
            .values()
            .all(|k| k.temporal.mode == TemporalMode::Auto
                && k.temporal.incoming.is_none()
                && k.temporal.outgoing.is_none())
    );
    let recomputed = preview.temporal_handle(50, false).unwrap().slope;
    let frozen_divided = old.temporal_handle(50, false).unwrap().slope / scale.time_scale;
    assert!((recomputed - frozen_divided).abs() > 1.0);
}

#[test]
fn speed_box_offscreen_selected_keys_keep_full_selection_and_true_pivot() {
    let (state, view, bounds) = scene(&[30, 50, 60]);
    let view = View {
        start: 40.0,
        span: 30.0,
        ..view
    };
    let b = area(&state);
    assert_eq!((b.first, b.last), (30.0, 60.0));
    let handles = b.handles(view, bounds);
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].0, Handle(1, 0));
    assert!(bounds.contains(&handles[0].1));
    let mut t = grab(&state, view, bounds, 1);
    t.moving(t.start + point(px(100.0), px(-1000.0)), false, false);
    let (scale, expected) = t.preview.as_ref().unwrap();
    assert_eq!(scale.time_origin, 30.0);
    assert_eq!(t.keys.len(), 3);
    assert_eq!(t.frames(), [30, 52, 63].into());
    assert_preserved(track(&state), expected, *scale, &[30, 50, 60]);
}

#[test]
fn speed_box_snapping_freezes_targets_switch_and_view_and_has_no_value_guide() {
    let (mut state, view, bounds) = scene(&[30, 50, 60]);
    state.frame = 75;
    state.snapping = true;
    let mut t = grab(&state, view, bounds, 1);
    state.frame = 80;
    state.snapping = false;
    state.timeline_start = 90;
    state.graph_view.height = Some([0.0, 5.0]);
    assert!(t.is_current(&state));
    let end = t.start + point(px(105.0), px(-4000.0));
    t.update_pointer(end, false, false);
    assert_eq!(
        t.guides,
        snapping::Guides {
            frame: Some(75),
            value: None
        }
    );
    assert_eq!(t.frames(), [30, 60, 75].into());
    t.update_pointer(end, false, true);
    assert_eq!(t.guides, snapping::Guides::default());
    assert_eq!(t.frames(), [30, 61, 76].into());
    // Eight logical pixels: 8.1px away no longer snaps to the captured playhead.
    t.update_pointer(t.start + point(px(108.1), px(8000.0)), false, false);
    assert_eq!(t.guides, snapping::Guides::default());
    let mut unsnapped = grab(&state, view, bounds, 1);
    state.frame = 70;
    unsnapped.update_pointer(unsnapped.start + point(px(133.0), px(0.0)), false, true);
    assert_eq!(unsnapped.guides.frame, Some(80));
    assert_eq!(unsnapped.guides.value, None);
}

#[test]
fn speed_box_alt_changes_pivot_without_disabling_time_snapping() {
    let (mut state, view, bounds) = scene(&[30, 50, 60]);
    state.snapping = true;
    state.frame = 75;
    let mut t = grab(&state, view, bounds, 1);
    t.update_pointer(t.start + point(px(105.0), px(-1000.0)), true, false);
    let (scale, _) = t.preview.as_ref().unwrap();
    assert_eq!((scale.time_origin, scale.time_scale), (45.0, 2.0));
    assert_eq!(
        t.guides,
        snapping::Guides {
            frame: Some(75),
            value: None
        }
    );
    assert_eq!(t.frames(), [15, 55, 75].into());
}

#[test]
fn speed_box_identity_vertical_only_and_return_to_start_preserve_history_and_redo() {
    let (mut state, view, bounds) = scene(&[30, 50, 60]);
    manual(&mut state);
    state.editor.clear_history();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo survives".into(),
        })
        .unwrap();
    state.editor.undo();
    let before = state.editor.project().clone();
    let selected = state.selected_keys.clone();
    let mut t = grab(&state, view, bounds, 1);
    for dy in [-8000.0, 100.0] {
        t.update_pointer(t.start + point(px(0.0), px(dy)), true, false);
        assert!(!t.moved);
        assert!(!t.has_changes());
    }
    t.update_pointer(t.start + point(px(100.0), px(-8000.0)), false, false);
    assert!(t.has_changes());
    t.update_pointer(t.start + point(px(0.0), px(9000.0)), true, false);
    assert!(!t.has_changes());
    assert_eq!(&t.preview.as_ref().unwrap().1, track(&state));
    state.editor.execute(t.command().unwrap().0).unwrap();
    assert_eq!(state.editor.project(), &before);
    assert_eq!(state.selected_keys, selected);
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
}

#[test]
fn speed_box_rejects_collision_nonpositive_nonfinite_and_out_of_bounds_then_recovers() {
    let (mut state, view, bounds) = scene(&[30, 50, 60]);
    state.selected_keys.retain(|k| k.frame != 60);
    let before = state.editor.project().clone();
    let mut t = grab(&state, view, bounds, 1);
    for dx in [66.66667, -200.0, -400.0, 1000.0, f32::INFINITY, f32::NAN] {
        t.moving(t.start + point(px(dx), px(4000.0)), false, false);
        assert!(t.preview.is_err(), "dx={dx}");
        assert!(t.command().is_err());
        assert_eq!(t.guides, snapping::Guides::default());
        assert_eq!(state.editor.project(), &before);
        assert!(!state.editor.can_undo());
    }
    t.moving(t.start + point(px(33.33333), px(0.0)), false, false);
    assert_eq!(t.frames(), [30, 55].into());
    assert!(t.command().is_ok());
}

#[test]
fn speed_box_rejects_rounded_selected_collisions_and_manual_slope_overflow() {
    let (mut state, view, bounds) = scene(&[30, 31, 60]);
    let mut t = grab(&state, view, bounds, 1);
    t.moving(t.start + point(px(-160.0), px(0.0)), false, false);
    assert!(t.preview.as_ref().unwrap_err().contains("collide"));
    state
        .editor
        .execute(Command::SetTemporalHandle {
            id: 1,
            property: state.graph_property,
            frame: 30,
            incoming: false,
            handle: TemporalHandle {
                slope: 1e9,
                influence: 0.25,
            },
        })
        .unwrap();
    let mut t = grab(&state, view, bounds, 1);
    t.moving(t.start + point(px(-40.0), px(0.0)), false, false);
    assert!(t.preview.as_ref().unwrap_err().contains("velocity"));
}

#[test]
fn speed_box_stale_mode_channel_layer_selection_lock_and_document_are_rejected() {
    for change in 0..8 {
        let (mut state, view, bounds) = scene(&[30, 50, 60]);
        let t = grab(&state, view, bounds, 1);
        assert!(t.is_current(&state));
        match change {
            0 => state.graph_view.speed = false,
            1 => state.graph_property = Property::PositionY.into(),
            2 => state.editor.clear_selection(),
            3 => {
                state.selected_keys.pop_last();
            }
            4 => state.editor.execute(Command::ToggleLocked(1)).unwrap(),
            5 => state.document_revision += 1,
            6 => state.tool = Tool::Hand,
            _ => edit(
                &mut state,
                TrackEdit::Value {
                    frame: 50,
                    value: 501.0,
                },
            ),
        }
        assert!(!t.is_current(&state), "change={change}");
        if change == 4 {
            assert!(Transform::new(&state, view, bounds, t.start).is_none());
            assert!(state.editor.execute(t.command().unwrap().0).is_err());
        }
    }
}

#[test]
fn speed_box_does_not_expose_decorated_handles_for_wholly_offscreen_selection() {
    let (state, view, bounds) = scene(&[30, 50, 60]);
    for view in [
        View {
            start: 0.0,
            span: 29.9,
            ..view
        },
        View {
            start: 60.1,
            span: 50.0,
            ..view
        },
    ] {
        assert!(area(&state).handles(view, bounds).is_empty());
        for p in [
            point(bounds.left() + px(1.0), bounds.top() + px(100.0)),
            point(bounds.right() - px(1.0), bounds.top() + px(100.0)),
        ] {
            assert!(Transform::new(&state, view, bounds, p).is_none());
        }
    }
}

#[test]
fn speed_box_visible_boundary_handles_do_not_intercept_flat_speed_key_glyphs() {
    let (state, view, bounds) = scene(&[0, 149]);
    let view = View {
        span: 149.0,
        ..view
    };
    let b = area(&state);
    let fps = state.editor.project().composition().fps().as_f64();
    assert_eq!(b.handles(view, bounds).len(), 2);
    for frame in [0, 149] {
        for (incoming, value) in speed::ends(track(&state), frame, fps) {
            let mut key = view.point(bounds, frame as f64, value);
            key.x += px(if incoming { -5.0 } else { 5.0 });
            for dx in [-3.0, 0.0, 3.0] {
                for dy in [-3.0, 0.0, 3.0] {
                    let point = key + point(px(dx), px(dy));
                    assert!(b.hit(view, bounds, point).is_none());
                    // Transform routing precedes ordinary key routing in Graph::down.
                    assert!(Transform::new(&state, view, bounds, point).is_none());
                }
            }
        }
    }
    for (handle, at) in b.handles(view, bounds) {
        assert_eq!(
            Transform::new(&state, view, bounds, at).unwrap().handle,
            handle
        );
    }
}

#[test]
fn speed_box_curve_sampling_stays_with_frozen_view_after_timeline_zoom() {
    let (mut state, view, bounds) = scene(&[30, 50, 60]);
    let mut t = grab(&state, view, bounds, 1);
    t.update_pointer(t.start + point(px(100.0), px(5000.0)), false, false);
    let preview = &t.preview.as_ref().unwrap().1;
    let fps = state.editor.project().composition().fps().as_f64();
    let before = t.view.speed_curves(preview, fps);
    state.timeline_start = 90;
    state.graph_view.height = Some([-1.0, 1.0]);
    assert!(t.is_current(&state));
    let after = t.view.speed_curves(preview, fps);
    assert_eq!(after, before);
    assert_eq!(after.first().unwrap().first().unwrap().0, 0.0);
    assert_eq!(after.last().unwrap().last().unwrap().0, 150.0);
    let live = viewport::current(&state, track(&state)).speed_curves(preview, fps);
    assert_eq!(live.first().unwrap().first().unwrap().0, 90.0);
    assert_ne!(live, after);
}
