//! Vertical Speed Graph handles edit endpoint tangents, not key values/times or
//! a uniformly scaled interior derivative. The complete gesture stays detached.
use super::speed_tests::{area, edit, manual, scene, track};
use super::*;
use libre_effects_core::{FrameRate, Project, Property, TemporalHandle};

fn velocity_scene() -> (EditorState, View, Bounds<Pixels>) {
    let (mut state, mut view, bounds) = scene(&[30, 50, 60]);
    manual(&mut state);
    state.editor.clear_history();
    let fps = state.editor.project().composition().fps().as_f64();
    view.low = -20.0 * fps;
    view.high = 20.0 * fps;
    (state, view, bounds)
}
fn grab(state: &EditorState, view: View, bounds: Bounds<Pixels>, side: i8) -> Transform {
    let at = area(state)
        .handles(view, bounds)
        .into_iter()
        .find(|(handle, _)| *handle == Handle(0, side))
        .unwrap()
        .1;
    Transform::new(state, view, bounds, at + point(px(2.0), px(-1.0))).unwrap()
}
fn factor_pointer(t: &Transform, factor: f64, center: bool) -> Point<Pixels> {
    let origin = if center {
        (t.area.low + t.area.high) / 2.0
    } else if t.handle.1 < 0 {
        t.area.high
    } else {
        t.area.low
    };
    let edge = if t.handle.1 < 0 {
        t.area.low
    } else {
        t.area.high
    };
    t.start
        + point(
            px(0.0),
            px(
                (-(factor - 1.0) * (edge - origin) / (t.view.high - t.view.low)
                    * f32::from(t.bounds.size.height) as f64) as f32,
            ),
        )
}
fn assert_scaled(
    old: &AnimatedProperty,
    new: &AnimatedProperty,
    scale: KeyVelocityScale,
    frames: &[u32],
) {
    assert_eq!(
        old.keys().keys().collect::<Vec<_>>(),
        new.keys().keys().collect::<Vec<_>>()
    );
    for (&frame, key) in old.keys() {
        assert_eq!(key.value, new.keys()[&frame].value);
        assert_eq!(key.interpolation, new.keys()[&frame].interpolation);
        let before = old.key_velocity_handles(frame).unwrap();
        let after = new.key_velocity_handles(frame).unwrap();
        for (a, b) in before.into_iter().zip(after) {
            assert_eq!(a.is_some(), b.is_some());
            if let (Some(a), Some(b)) = (a, b) {
                let want = if frames.contains(&frame) {
                    scale.origin + scale.factor * (a.slope - scale.origin)
                } else {
                    a.slope
                };
                assert!(
                    (b.slope - want).abs() < 1e-8,
                    "{frame}: {} != {want}",
                    b.slope
                );
                assert_eq!(a.influence, b.influence);
            }
        }
    }
}

#[test]
fn velocity_handles_scale_signed_sides_about_opposite_edge_or_initial_midpoint() {
    let (state, view, bounds) = velocity_scene();
    for side in [-1, 1] {
        for center in [false, true] {
            for factor in [1.5, -1.0, 0.0] {
                let mut t = grab(&state, view, bounds, side);
                assert!(!t.has_changes());
                let end = factor_pointer(&t, factor, center);
                t.update_pointer(end, center, false);
                let scale = t.velocity_scale.unwrap();
                assert!((scale.factor - factor).abs() < 1e-7);
                assert!(
                    (scale.origin
                        - if center {
                            3.0
                        } else if side < 0 {
                            18.0
                        } else {
                            -12.0
                        })
                    .abs()
                        < 1e-9
                );
                let preview = t.preview.as_ref().unwrap().1.clone();
                assert_scaled(track(&state), &preview, scale, &[30, 50, 60]);
                for dx in [-9000.0, 3000.0, f32::NAN] {
                    t.update_pointer(point(t.start.x + px(dx), end.y), center, false);
                    assert_eq!(t.preview.as_ref().unwrap().1, preview);
                }
                assert_eq!(t.frames(), [30, 50, 60].into());
                assert_eq!(t.guides.frame, None);
                assert_eq!(track(&state), &t.original);
            }
        }
    }
}

#[test]
fn velocity_final_pointer_and_modifiers_commit_one_roundtrippable_undo_step() {
    let (mut state, view, bounds) = velocity_scene();
    let before = state.editor.project().clone();
    let mut t = grab(&state, view, bounds, 1);
    t.update_pointer(factor_pointer(&t, 1.5, false), false, false);
    // Mouse-up outside the plot is recomputed against the initial midpoint.
    let end = factor_pointer(&t, -2.0, true);
    t.update_pointer(end, true, false);
    let preview = t.preview.as_ref().unwrap().1.clone();
    assert!((t.velocity_scale.unwrap().factor + 2.0).abs() < 1e-7);
    let (command, keys) = t.command().unwrap();
    assert_eq!(keys, selection::active(&state));
    assert!(matches!(command, Command::ScaleKeyVelocities { .. }));
    state.editor.execute(command).unwrap();
    assert_eq!(track(&state), &preview);
    let saved = Project::from_json(&state.editor.project().to_json().unwrap()).unwrap();
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo());
    state.editor.redo();
    assert_eq!(state.editor.project(), &saved);
    let renderer = crate::rendering::Renderer::new();
    for frame in [30, 35, 43, 50, 55, 60] {
        assert_eq!(
            renderer.render(&saved, frame, 384).unwrap(),
            renderer.render_output(&saved, frame, 384, 216).unwrap()
        );
    }
    // Fixed key values force the segment integral to stay fixed even as endpoint
    // velocities change. It is incorrect to claim all interior speed is scaled.
    assert!(
        (preview.velocity(40.0, false).unwrap()
            - (t.velocity_scale.unwrap().origin
                + t.velocity_scale.unwrap().factor
                    * (t.original.velocity(40.0, false).unwrap()
                        - t.velocity_scale.unwrap().origin)))
            .abs()
            > 1.0
    );
}

#[test]
fn velocity_identity_axis_isolation_return_and_cancel_preserve_redo() {
    let (mut state, view, bounds) = velocity_scene();
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
    for dx in [-8000.0, 100.0, f32::NAN] {
        t.update_pointer(t.start + point(px(dx), px(0.0)), true, false);
        assert!(!t.moved && !t.has_changes());
        assert_eq!(t.guides, snapping::Guides::default());
    }
    t.update_pointer(factor_pointer(&t, 2.0, false), false, false);
    assert!(t.has_changes());
    t.update_pointer(t.start + point(px(9000.0), px(0.0)), true, false);
    assert!(!t.has_changes());
    state.editor.execute(t.command().unwrap().0).unwrap();
    assert_eq!(state.editor.project(), &before);
    assert_eq!(state.selected_keys, selected);
    assert!(!state.editor.can_undo() && state.editor.can_redo());
    t.update_pointer(factor_pointer(&t, -1.0, false), false, false);
    drop(t); // Escape, blur and deactivation all discard this detached draft.
    assert_eq!(state.editor.project(), &before);
    assert!(!state.editor.can_undo() && state.editor.can_redo());
}

#[test]
fn velocity_handles_reject_flat_hold_singular_and_unrepresentable_sides_but_keep_time() {
    for kind in 0..5 {
        let (mut state, view, bounds) = scene(&[30, 50, 60]);
        match kind {
            0 => {
                for frame in [30, 50, 60] {
                    edit(
                        &mut state,
                        TrackEdit::Value {
                            frame,
                            value: 200.0,
                        },
                    );
                }
            }
            1 | 2 => {
                if kind == 2 {
                    manual(&mut state);
                }
                // Hold remains unsupported even if the other selected endpoints
                // have ordinary finite velocities and the range is non-flat.
                edit(
                    &mut state,
                    TrackEdit::Interpolate {
                        frame: 30,
                        interpolation: Interpolation::Hold,
                    },
                );
            }
            3 => edit(
                &mut state,
                TrackEdit::Interpolate {
                    frame: 30,
                    interpolation: Interpolation::Bezier(Bezier {
                        x1: 0.0,
                        y1: 0.5,
                        x2: 0.8,
                        y2: 0.5,
                    }),
                },
            ),
            _ => edit(
                &mut state,
                TrackEdit::Interpolate {
                    frame: 30,
                    interpolation: Interpolation::Bezier(Bezier {
                        x1: 0.0001,
                        y1: 0.0001,
                        x2: 0.8,
                        y2: 0.5,
                    }),
                },
            ),
        }
        let b = area(&state);
        assert!(b.velocity_disabled_reason().is_some(), "kind={kind}");
        let handles = b.handles(view, bounds);
        assert_eq!(handles.len(), 2, "kind={kind}");
        assert!(handles.iter().all(|(h, _)| h.1 == 0));
        let mut t = Transform::new(&state, view, bounds, handles[1].1).unwrap();
        t.update_pointer(t.start + point(px(50.0), px(500.0)), false, false);
        assert!(t.preview.is_ok());
        assert!(matches!(t.command().unwrap().0, Command::ScaleKeys { .. }));
    }
}

#[test]
fn velocity_handles_use_true_visible_edges_clear_all_glyphs_and_never_make_corners() {
    let (state, view, bounds) = velocity_scene();
    let b = area(&state);
    let fps = state.editor.project().composition().fps().as_f64();
    for view in [
        view,
        View {
            start: 45.0,
            span: 15.0,
            low: b.low,
            high: b.high,
            ..view
        },
    ] {
        let handles = b.handles(view, bounds);
        assert!(handles.iter().any(|(h, _)| h.1 != 0));
        assert!(
            handles
                .iter()
                .all(|(h, p)| h.0 == 0 || h.1 == 0 && bounds.contains(p))
        );
        for (handle, p) in handles {
            assert!(bounds.contains(&p));
            assert_eq!(b.hit(view, bounds, p), Some(handle));
        }
        for &frame in track(&state).keys().keys() {
            for (incoming, speed) in speed::ends(track(&state), frame, fps) {
                let key = view.point(bounds, frame as f64, speed)
                    + point(px(if incoming { -5.0 } else { 5.0 }), px(0.0));
                for dx in [-3.0, 0.0, 3.0] {
                    for dy in [-3.0, 0.0, 3.0] {
                        assert!(b.hit(view, bounds, key + point(px(dx), px(dy))).is_none());
                    }
                }
            }
        }
    }
    let lower = View {
        high: b.high - 0.01,
        ..view
    };
    assert!(
        !b.handles(lower, bounds)
            .iter()
            .any(|(h, _)| *h == Handle(0, 1))
    );
    let upper = View {
        low: b.low + 0.01,
        ..view
    };
    assert!(
        !b.handles(upper, bounds)
            .iter()
            .any(|(h, _)| *h == Handle(0, -1))
    );
    for tiny in [
        size(px(1.0), px(1.0)),
        size(px(12.0), px(200.0)),
        size(px(500.0), px(12.0)),
    ] {
        assert!(
            b.handles(view, Bounds::new(bounds.origin, tiny))
                .iter()
                .all(|(h, _)| h.1 == 0)
        );
    }
    let value = SelectionBox::new(track(&state), &[30, 50, 60].into(), false, fps).unwrap();
    assert_eq!(value.handles(view, bounds).len(), 8);
}

#[test]
fn velocity_drag_rejects_stale_context_view_fps_and_geometry() {
    for change in 0..12 {
        let (mut state, view, bounds) = velocity_scene();
        let t = grab(&state, view, bounds, 1);
        assert!(t.is_current(&state));
        assert!(t.geometry_current(Some(bounds)));
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
            7 => state.timeline_start += 1,
            8 => state.timeline_zoom = 2.0,
            9 => state.graph_view.height = Some([-1.0, 1.0]),
            10 => edit(
                &mut state,
                TrackEdit::Value {
                    frame: 50,
                    value: 501.0,
                },
            ),
            _ => state
                .editor
                .execute(Command::ConfigureCompositionRate {
                    name: "New FPS".into(),
                    width: 1280,
                    height: 720,
                    fps: FrameRate::new(24, 1).unwrap(),
                    duration: 150,
                    display_start: 0,
                })
                .unwrap(),
        }
        assert!(!t.is_current(&state), "change={change}");
        assert!(!t.geometry_current(None));
        assert!(!t.geometry_current(Some(Bounds::new(
            bounds.origin,
            bounds.size + size(px(1.0), px(0.0))
        ))));
    }
}

#[test]
fn velocity_invalid_pointer_clears_guides_and_can_return_to_valid_draft() {
    let (state, view, bounds) = velocity_scene();
    let mut t = grab(&state, view, bounds, 1);
    for dy in [f32::NAN, f32::INFINITY, -1e20] {
        t.update_pointer(t.start + point(px(0.0), px(dy)), false, false);
        assert!(t.preview.is_err());
        assert!(t.command().is_err());
        assert_eq!(t.guides, snapping::Guides::default());
    }
    t.update_pointer(factor_pointer(&t, 1.5, false), false, false);
    assert!(t.has_changes());
    assert!(!state.editor.can_undo());
}

#[test]
fn velocity_snap_converts_rational_fps_once_freezes_targets_and_inverts_ctrl() {
    let (mut state, view, bounds) = velocity_scene();
    edit(&mut state, TrackEdit::ToggleKey { frame: 90 });
    for (frame, incoming, slope) in [(60, false, 6.0), (90, true, 23.0)] {
        state
            .editor
            .execute(Command::SetTemporalHandle {
                id: 1,
                property: state.graph_property,
                frame,
                incoming,
                handle: TemporalHandle {
                    slope,
                    influence: 0.3,
                },
            })
            .unwrap();
    }
    state.snapping = true;
    let mut t = grab(&state, view, bounds, 1);
    let fps = state.editor.project().composition().fps().as_f64();
    let end = t.start + point(px(4000.0), px(-52.0)); // raw 23.2 units/frame
    state.frame = 75;
    state.snapping = false;
    assert!(t.is_current(&state));
    t.update_pointer(end, false, false);
    assert_eq!(t.guides.frame, None);
    assert!((t.guides.value.unwrap() - 23.0 * fps).abs() < 1e-9);
    assert!((t.velocity_scale.unwrap().factor - 35.0 / 30.0).abs() < 1e-9);
    assert_scaled(
        &t.original,
        &t.preview.as_ref().unwrap().1,
        t.velocity_scale.unwrap(),
        &[30, 50, 60],
    );
    t.update_pointer(end, false, true);
    assert_eq!(t.guides, snapping::Guides::default());
    assert!((t.velocity_scale.unwrap().factor - 35.2 / 30.0).abs() < 1e-8);
    t.update_pointer(end, true, false);
    assert!((t.guides.value.unwrap() - 23.0 * fps).abs() < 1e-9);
    assert_eq!(t.velocity_scale.unwrap().origin, 3.0);
    assert!((t.velocity_scale.unwrap().factor - 20.0 / 15.0).abs() < 1e-9);
    t.update_pointer(t.start + point(px(0.0), px(-58.1)), false, false);
    assert_eq!(t.guides, snapping::Guides::default());
    t.update_pointer(t.start + point(px(1000.0), px(0.0)), true, false);
    assert!(!t.has_changes());
    assert_eq!(t.guides, snapping::Guides::default());
    let mut off = grab(&state, view, bounds, 1);
    off.update_pointer(off.start + point(px(0.0), px(-52.0)), false, true);
    assert!((off.guides.value.unwrap() - 23.0 * fps).abs() < 1e-9);
    // Editing an unselected target invalidates commit and cannot retarget the draft.
    state
        .editor
        .execute(Command::SetTemporalHandle {
            id: 1,
            property: state.graph_property,
            frame: 90,
            incoming: true,
            handle: TemporalHandle {
                slope: 25.0,
                influence: 0.3,
            },
        })
        .unwrap();
    assert!(!t.is_current(&state));
    t.update_pointer(end, false, false);
    assert!((t.guides.value.unwrap() - 23.0 * fps).abs() < 1e-9);
}

#[test]
fn velocity_handles_do_not_cover_unselected_endpoint_glyphs() {
    let (mut state, view, bounds) = velocity_scene();
    state.selected_keys.retain(|key| key.frame != 50);
    let fps = state.editor.project().composition().fps().as_f64();
    // The unselected middle key lies at the initial horizontal handle center;
    // its outgoing endpoint is at the decorated selected top edge.
    state
        .editor
        .execute(Command::MoveKeys {
            keys: vec![KeyRef {
                id: 1,
                property: state.graph_property,
                frame: 50,
            }],
            delta: -5,
        })
        .unwrap();
    state
        .editor
        .execute(Command::SetTemporalHandle {
            id: 1,
            property: state.graph_property,
            frame: 45,
            incoming: false,
            handle: TemporalHandle {
                slope: 19.0,
                influence: 0.3,
            },
        })
        .unwrap();
    let b = area(&state);
    let glyph = view.point(bounds, 45.0, 19.0 * fps) + point(px(5.0), px(0.0));
    let handles = b.handles(view, bounds);
    assert!(handles.iter().any(|(h, _)| *h == Handle(0, 1)));
    for dx in [-3.0, 0.0, 3.0] {
        for dy in [-3.0, 0.0, 3.0] {
            assert!(b.hit(view, bounds, glyph + point(px(dx), px(dy))).is_none());
        }
    }
}

#[test]
fn velocity_context_rejects_unrelated_document_changes_and_modal_starts() {
    let (mut state, view, bounds) = velocity_scene();
    let t = grab(&state, view, bounds, 1);
    state
        .editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Changed".into(),
        })
        .unwrap();
    assert!(!t.is_current(&state));
    state.editor.undo();
    assert!(t.is_current(&state));
    state
        .editor
        .execute(Command::ConfigureCompositionRate {
            name: "Speed time box".into(),
            width: 1280,
            height: 720,
            fps: FrameRate::new(30_000, 1001).unwrap(),
            duration: 151,
            display_start: 0,
        })
        .unwrap();
    assert!(!t.is_current(&state));
    for modal in 0..5 {
        let (mut state, view, bounds) = velocity_scene();
        let t = grab(&state, view, bounds, 1);
        match modal {
            0 => state.playing = true,
            1 => state.fonts_open = true,
            2 => state.media_open = true,
            3 => state.queue_open = true,
            _ => state.new_composition_requested = true,
        }
        assert!(!t.is_current(&state));
        assert!(Transform::new(&state, view, bounds, t.start).is_none());
    }
}

#[test]
fn velocity_transport_generation_and_hidden_graph_invalidate_draft() {
    let (mut state, view, bounds) = velocity_scene();
    let mut t = grab(&state, view, bounds, 1);
    state.graph_open = !state.graph_open;
    assert!(!t.is_current(&state));
    state.graph_open = !state.graph_open;
    assert!(t.is_current(&state));
    t.transport_generation = state.transport_generation().wrapping_sub(1);
    assert!(!state.playing);
    assert!(!t.is_current(&state));
}

#[test]
fn velocity_legacy_linear_fixed_pivot_has_no_phantom_materialization() {
    let (state, view, bounds) = scene(&[30, 50, 60]);
    let mut t = grab(&state, view, bounds, 1);
    let range = t.area.velocity_range.unwrap();
    assert_eq!(range, (15.0, 40.0));
    t.update_pointer(factor_pointer(&t, 2.0, false), false, false);
    let preview = &t.preview.as_ref().unwrap().1;
    assert_eq!(preview.keys()[&30], t.original.keys()[&30]);
    assert_eq!(preview.keys()[&30].temporal.outgoing, None);
    t.update_pointer(t.start, true, false);
    assert_eq!(&t.preview.as_ref().unwrap().1, track(&state));
}
