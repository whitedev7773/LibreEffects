//! Graph snapping uses screen distance and one offset for the complete selection.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(super) struct Guides {
    pub frame: Option<u32>,
    /// Displayed units (property value or signed units/second).
    pub value: Option<f64>,
}

#[derive(Clone)]
pub(super) struct Targets {
    times: Vec<i64>,
    values: Vec<f64>,
    occupied: BTreeSet<u32>,
    fps: f64,
}
impl Targets {
    /// Snap a moving selection-box edge while retaining its fixed scaling origin.
    /// Time candidates must leave every selected destination distinct and vacant.
    pub fn scale(
        &self,
        mut scale: libre_effects_core::KeyScale,
        time_edge: Option<f64>,
        value_edge: Option<f64>,
        frames: &[u32],
        view: View,
        bounds: Bounds<Pixels>,
        duration: u32,
    ) -> (libre_effects_core::KeyScale, Guides) {
        let mut guides = Guides::default();
        if !scale.time_scale.is_finite()
            || scale.time_scale <= 0.0
            || !scale.value_scale.is_finite()
        {
            return (scale, guides);
        }
        if let Some(edge) = time_edge.filter(|edge| (*edge - scale.time_origin).abs() > 1e-9) {
            let wanted = scale.time_origin + (edge - scale.time_origin) * scale.time_scale;
            let tolerance = 8.0 * view.span / f32::from(bounds.size.width).max(1.0) as f64;
            let first = self
                .times
                .partition_point(|t| (*t as f64) < wanted - tolerance);
            let mut best: Option<(f64, i64, f64)> = None;
            for &target in self.times[first..]
                .iter()
                .take_while(|t| (**t as f64) <= wanted + tolerance)
            {
                let factor = (target as f64 - scale.time_origin) / (edge - scale.time_origin);
                let candidate = libre_effects_core::KeyScale {
                    time_scale: factor,
                    ..scale
                };
                let mut destinations = BTreeSet::new();
                if frames.iter().any(|&f| {
                    candidate.frame(f, duration).map_or(true, |to| {
                        self.occupied.contains(&to) || !destinations.insert(to)
                    })
                }) {
                    continue;
                }
                let distance = (target as f64 - wanted).abs();
                if best.is_none_or(|b| (distance, target) < (b.0, b.1)) {
                    best = Some((distance, target, factor));
                }
            }
            if let Some((_, target, factor)) = best {
                scale.time_scale = factor;
                guides.frame = u32::try_from(target).ok();
            }
        }
        if let Some(edge) = value_edge.filter(|edge| (*edge - scale.value_origin).abs() > 1e-9) {
            let wanted = scale.value_origin + (edge - scale.value_origin) * scale.value_scale;
            let tolerance =
                8.0 * (view.high - view.low) / f32::from(bounds.size.height).max(1.0) as f64;
            let first = self.values.partition_point(|v| *v < wanted - tolerance);
            let mut best: Option<(f64, f64, f64)> = None;
            for &target in self.values[first..]
                .iter()
                .take_while(|v| **v <= wanted + tolerance)
            {
                let factor = (target - scale.value_origin) / (edge - scale.value_origin);
                if !factor.is_finite() {
                    continue;
                }
                let distance = (target - wanted).abs();
                if best.is_none_or(|b| (distance, target) < (b.0, b.1)) {
                    best = Some((distance, target, factor));
                }
            }
            if let Some((_, target, factor)) = best {
                scale.value_scale = factor;
                guides.value = Some(target);
            }
        }
        (scale, guides)
    }
    /// Endpoint-velocity targets and pivots are units/frame; only the viewport
    /// and guide are units/second. This path deliberately does not reuse value
    /// scaling, which compares ordinary property values without FPS conversion.
    pub fn velocity_scale(
        &self,
        mut scale: libre_effects_core::KeyVelocityScale,
        edge: f64,
        view: View,
        bounds: Bounds<Pixels>,
        mut valid: impl FnMut(libre_effects_core::KeyVelocityScale) -> bool,
    ) -> (libre_effects_core::KeyVelocityScale, Guides) {
        let mut guides = Guides::default();
        let span = edge - scale.origin;
        if !scale.origin.is_finite() || !scale.factor.is_finite() || span.abs() < 1e-12 {
            return (scale, guides);
        }
        let wanted = scale.origin + span * scale.factor;
        let tolerance =
            8.0 * (view.high - view.low) / f32::from(bounds.size.height).max(1.0) as f64 / self.fps;
        let first = self
            .values
            .partition_point(|value| *value < wanted - tolerance);
        let target = self.values[first..]
            .iter()
            .take_while(|value| **value <= wanted + tolerance)
            .copied()
            .filter(|value| {
                let candidate = libre_effects_core::KeyVelocityScale {
                    origin: scale.origin,
                    factor: (value - scale.origin) / span,
                };
                candidate.factor.is_finite() && valid(candidate)
            })
            .min_by(|a, b| {
                (a - wanted)
                    .abs()
                    .total_cmp(&(b - wanted).abs())
                    .then_with(|| a.total_cmp(b))
            });
        if let Some(target) = target {
            scale.factor = (target - scale.origin) / span;
            guides.value = Some(target * self.fps);
        }
        (scale, guides)
    }
    /// Capture before clicking a key seeks the playhead to that key.
    pub fn new(
        state: &EditorState,
        track: &AnimatedProperty,
        keys: &[selection::Sample],
        side: Option<bool>,
    ) -> Self {
        let comp = state.editor.project().composition();
        let selected = keys.iter().map(|s| s.key).collect();
        let times = super::super::timeline_snap::targets(
            comp,
            Some(state.frame),
            &BTreeSet::new(),
            &selected,
        );
        let frames: BTreeSet<_> = keys.iter().map(|s| s.key.frame).collect();
        let occupied: BTreeSet<_> = track
            .keys()
            .keys()
            .copied()
            .filter(|f| !frames.contains(f))
            .collect();
        let mut values = vec![];
        for &frame in &occupied {
            if side.is_some() {
                values.extend(speed::ends(track, frame, 1.0).into_iter().map(|(_, v)| v));
            } else {
                values.push(track.keys()[&frame].value);
            }
        }
        values.retain(|v| v.is_finite());
        values.sort_by(f64::total_cmp);
        values.dedup();
        Self {
            times,
            values,
            occupied,
            fps: if side.is_some() {
                comp.fps().as_f64()
            } else {
                1.0
            },
        }
    }
    pub fn apply(
        &self,
        keys: &[selection::Sample],
        raw_delta: f64,
        raw_amount: f64,
        view: View,
        bounds: Bounds<Pixels>,
        duration: u32,
        side: Option<bool>,
        enabled: bool,
        time_moving: bool,
        value_moving: bool,
    ) -> (i64, f64, Guides) {
        let mut delta = selection::clamp_delta(keys, raw_delta.round() as i64, duration);
        let mut amount = raw_amount;
        let mut guides = Guides::default();
        if !enabled || keys.is_empty() {
            return (delta, amount, guides);
        }
        if time_moving {
            let tolerance = 8.0 * view.span / f32::from(bounds.size.width).max(1.0) as f64;
            let mut best: Option<(f64, i64, i64)> = None;
            for key in keys {
                let wanted = key.key.frame as f64 + raw_delta;
                let first = self
                    .times
                    .partition_point(|t| (*t as f64) < wanted - tolerance);
                for &target in self.times[first..]
                    .iter()
                    .take_while(|t| (**t as f64) <= wanted + tolerance)
                {
                    let adjusted = target - key.key.frame as i64;
                    if selection::clamp_delta(keys, adjusted, duration) != adjusted {
                        continue;
                    }
                    if keys.iter().any(|k| {
                        self.occupied
                            .contains(&((k.key.frame as i64 + adjusted) as u32))
                    }) {
                        continue;
                    }
                    let distance = (adjusted as f64 - raw_delta).abs();
                    if best.is_none_or(|b| (distance, target) < (b.0, b.2)) {
                        best = Some((distance, adjusted, target));
                    }
                }
            }
            if let Some((_, adjusted, target)) = best {
                delta = adjusted;
                guides.frame = u32::try_from(target).ok();
            }
        }
        if value_moving {
            let tolerance = 8.0 * (view.high - view.low)
                / f32::from(bounds.size.height).max(1.0) as f64
                / self.fps;
            let mut best: Option<(f64, f64, f64)> = None;
            for key in keys {
                let Some(anchor) = (if side.is_some() {
                    key.handle.map(|h| h.slope)
                } else {
                    Some(key.value)
                }) else {
                    continue;
                };
                let wanted = anchor + raw_amount;
                let first = self.values.partition_point(|v| *v < wanted - tolerance);
                for &target in self.values[first..]
                    .iter()
                    .take_while(|v| **v <= wanted + tolerance)
                {
                    let adjusted = target - anchor;
                    let distance = (adjusted - raw_amount).abs();
                    if best.is_none_or(|b| (distance, target) < (b.0, b.2)) {
                        best = Some((distance, adjusted, target));
                    }
                }
            }
            if let Some((_, adjusted, target)) = best {
                amount = adjusted;
                guides.value = Some(target * self.fps);
            }
        }
        (delta, amount, guides)
    }
}

/// Ctrl inverts the switch during a key drag; Alt suppresses snapping like the timeline.
pub(super) fn enabled(switch: bool, control: bool, alt: bool) -> bool {
    (switch ^ control) && !alt
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, KeyRef, Project, Property};
    #[test]
    fn transform_snap_keeps_pivots_and_only_changes_moving_axes() {
        let (s, k) = scene();
        let snap = targets(&s, &k, None);
        let (v, b) = geometry();
        let raw = libre_effects_core::KeyScale {
            time_origin: 10.0,
            time_scale: 1.975,
            value_origin: 600.0,
            value_scale: 0.385,
        };
        let (scale, guides) = snap.scale(raw, Some(30.0), Some(900.0), &[10, 30], v, b, 150);
        assert_eq!((scale.time_origin, scale.value_origin), (10.0, 600.0));
        assert_eq!((scale.time_scale, scale.value_scale), (2.0, 0.4));
        assert_eq!(
            guides,
            Guides {
                frame: Some(50),
                value: Some(720.0)
            }
        );
        let (still, g) = snap.scale(raw, None, None, &[10, 30], v, b, 150);
        assert_eq!(
            (still.time_scale, still.value_scale),
            (raw.time_scale, raw.value_scale)
        );
        assert_eq!(g, Guides::default());
        // A tighter viewport uses the same eight screen pixels, not eight frames.
        let (_, g) = snap.scale(
            raw,
            Some(30.0),
            Some(900.0),
            &[10, 30],
            View {
                span: 10.0,
                low: 650.0,
                high: 750.0,
                ..v
            },
            b,
            150,
        );
        assert_eq!(g, Guides::default());
    }
    #[test]
    fn transform_candidates_skip_collisions_rounding_and_composition_edges() {
        let (s, k) = scene();
        let mut snap = targets(&s, &k, None);
        let (v, b) = geometry();
        snap.times = vec![39, 40];
        snap.occupied = [40].into();
        let raw = libre_effects_core::KeyScale {
            time_origin: 25.0,
            time_scale: 2.96,
            value_origin: 0.0,
            value_scale: 1.0,
        };
        let (scale, g) = snap.scale(raw, Some(30.0), None, &[20, 30], v, b, 150);
        assert_eq!(g.frame, Some(39));
        assert_eq!(scale.frame(20, 150).unwrap(), 11);
        assert_eq!(scale.frame(30, 150).unwrap(), 39);
        snap.times = vec![23, 24];
        snap.occupied.clear();
        let (_, g) = snap.scale(
            libre_effects_core::KeyScale {
                time_origin: 20.0,
                time_scale: 0.37,
                ..raw
            },
            Some(30.0),
            None,
            &[20, 21, 30],
            v,
            b,
            150,
        );
        assert_eq!(g.frame, None);
        snap.times = vec![149, 150];
        let (scale, g) = snap.scale(
            libre_effects_core::KeyScale {
                time_origin: 20.0,
                time_scale: 12.96,
                ..raw
            },
            Some(30.0),
            None,
            &[20, 30],
            v,
            b,
            150,
        );
        assert_eq!(g.frame, Some(149));
        assert_eq!(scale.frame(30, 150).unwrap(), 149);
        let (_, g) = snap.scale(
            libre_effects_core::KeyScale {
                time_scale: -1.0,
                ..raw
            },
            Some(30.0),
            None,
            &[20, 30],
            v,
            b,
            150,
        );
        assert_eq!(g, Guides::default());
    }
    #[test]
    fn transform_value_reflection_snaps_signed_values_and_ignores_fixed_edges() {
        let (s, k) = scene();
        let mut snap = targets(&s, &k, None);
        let (v, b) = geometry();
        snap.values = vec![-20.0];
        let raw = libre_effects_core::KeyScale {
            time_origin: 20.0,
            time_scale: 1.0,
            value_origin: 0.0,
            value_scale: -1.99,
        };
        let (scale, g) = snap.scale(raw, None, Some(10.0), &[20, 30], v, b, 150);
        assert_eq!((scale.value_scale, g.value), (-2.0, Some(-20.0)));
        let (_, g) = snap.scale(raw, Some(20.0), Some(0.0), &[20, 30], v, b, 150);
        assert_eq!(g, Guides::default());
    }
    fn scene() -> (EditorState, Vec<selection::Sample>) {
        let mut s = EditorState::default();
        s.editor.execute(Command::AddRectangle).unwrap();
        for (frame, value) in [(10, 600.0), (30, 900.0), (60, 720.0)] {
            s.editor
                .execute(Command::ToggleKeyframe {
                    id: 1,
                    property: Property::PositionX,
                    frame,
                })
                .unwrap();
            s.editor
                .execute(Command::SetValue {
                    id: 1,
                    property: Property::PositionX,
                    frame,
                    value,
                })
                .unwrap();
        }
        s.frame = 50;
        s.graph_property = Property::PositionX.into();
        let keys = [10, 30].map(|frame| KeyRef {
            id: 1,
            property: s.graph_property,
            frame,
        });
        let samples = selection::snapshot(
            s.editor
                .selected_layer()
                .unwrap()
                .track(s.graph_property)
                .unwrap(),
            &keys,
            None,
        );
        (s, samples)
    }
    fn geometry() -> (View, Bounds<Pixels>) {
        (
            View {
                start: 0.0,
                span: 100.0,
                low: 400.0,
                high: 1200.0,
            },
            Bounds::new(point(px(0.0), px(0.0)), size(px(1000.0), px(400.0))),
        )
    }
    fn targets(s: &EditorState, k: &[selection::Sample], side: Option<bool>) -> Targets {
        Targets::new(
            s,
            s.editor
                .selected_layer()
                .unwrap()
                .track(s.graph_property)
                .unwrap(),
            k,
            side,
        )
    }
    #[test]
    fn captured_playhead_and_key_values_snap_group_without_changing_spacing() {
        let (mut s, k) = scene();
        let snap = targets(&s, &k, None);
        s.frame = 10;
        let (v, b) = geometry();
        let (d, a, g) = snap.apply(&k, 19.5, 114.0, v, b, 150, None, true, true, true);
        assert_eq!(
            (d, a, g),
            (
                20,
                120.0,
                Guides {
                    frame: Some(50),
                    value: Some(720.0)
                }
            )
        );
        let before = s.editor.project().clone();
        s.editor
            .execute(selection::translate(&k, d, a, None).unwrap())
            .unwrap();
        let t = s
            .editor
            .selected_layer()
            .unwrap()
            .track(s.graph_property)
            .unwrap();
        assert_eq!(
            t.keys().keys().copied().collect::<Vec<_>>(),
            vec![30, 50, 60]
        );
        assert_eq!(t.keys()[&30].value, 720.0);
        assert_eq!(t.keys()[&50].value, 1020.0);
        let saved = Project::from_json(&s.editor.project().to_json().unwrap()).unwrap();
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        s.editor.redo();
        assert_eq!(s.editor.project(), &saved);
        let renderer = crate::rendering::Renderer::new();
        for (frame, expected_x) in [(30, 720.0), (40, 870.0), (50, 1020.0), (55, 870.0)] {
            let preview = renderer.render(s.editor.project(), frame, 384).unwrap();
            let output = renderer.render_output(&saved, frame, 384, 216).unwrap();
            assert_eq!(preview, output);
            let xs: Vec<_> = output
                .enumerate_pixels()
                .filter(|(_, _, p)| p[3] > 0)
                .map(|(x, _, _)| x)
                .collect();
            let center = (*xs.iter().min().unwrap() + *xs.iter().max().unwrap() + 1) as f64 / 2.0;
            assert!((center - expected_x / 5.0).abs() < 1.0);
        }
    }
    #[test]
    fn zoom_boundary_collision_and_axis_constraints_are_respected() {
        let (s, k) = scene();
        let mut snap = targets(&s, &k, None);
        let (v, b) = geometry();
        let far = snap.apply(&k, 18.5, 100.0, v, b, 150, None, true, true, true);
        assert_eq!(far, (19, 100.0, Guides::default()));
        let zoom = View { span: 20.0, ..v };
        assert_eq!(
            snap.apply(&k, 19.5, 114.0, zoom, b, 150, None, true, true, false)
                .0,
            20
        );
        assert_eq!(
            snap.apply(&k, 19.5, 114.0, zoom, b, 150, None, true, true, false)
                .2,
            Guides::default()
        );
        snap.times = vec![59, 60];
        // The nearest target collides with an unselected key; use the next valid one.
        assert_eq!(
            snap.apply(
                &k,
                29.6,
                0.0,
                View { span: 150.0, ..v },
                b,
                150,
                None,
                true,
                true,
                false
            )
            .0,
            29
        );
        assert_eq!(
            snap.apply(&k, 130.0, 0.0, v, b, 150, None, true, true, false)
                .0,
            119
        );
        assert_eq!(
            snap.apply(&k, 19.5, 114.0, v, b, 150, None, true, false, false)
                .2,
            Guides::default()
        );
        assert_eq!(
            snap.apply(&k, 19.5, 114.0, v, b, 150, None, false, true, true)
                .2,
            Guides::default()
        );
        assert!(enabled(true, false, false));
        assert!(!enabled(true, true, false));
        assert!(enabled(false, true, false));
        assert!(!enabled(false, true, true));
    }
    #[test]
    fn speed_snap_uses_signed_slope_with_fps_scaled_tolerance() {
        let (s, k) = scene();
        let t = s
            .editor
            .selected_layer()
            .unwrap()
            .track(s.graph_property)
            .unwrap();
        let keys = selection::snapshot(t, &[k[0].key], Some(false));
        let snap = targets(&s, &keys, Some(false));
        let (v, b) = geometry();
        // First outgoing is 15/frame; the unselected final incoming is -6/frame.
        let (d, a, g) = snap.apply(
            &keys,
            0.0,
            -20.9,
            View {
                low: -600.0,
                high: 600.0,
                ..v
            },
            b,
            150,
            Some(false),
            true,
            false,
            true,
        );
        assert_eq!((d, a, g.value), (0, -21.0, Some(-180.0)));
        assert_eq!(g.frame, None);
        let mut e = Editor::default();
        e.replace_project(s.editor.project().clone()).unwrap();
        e.select(1);
        e.execute(selection::translate(&keys, d, a, Some(false)).unwrap())
            .unwrap();
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track(s.graph_property)
                .unwrap()
                .keys()[&10]
                .value,
            600.0
        );
    }
}

#[cfg(test)]
mod velocity_scale_tests {
    use super::*;
    use libre_effects_core::{FrameRate, KeyRef, KeyVelocityScale, Property, TemporalHandle};

    #[test]
    fn endpoint_snap_skips_invalid_reflection_candidate_and_preserves_valid_raw_edit() {
        let mut state = EditorState::default();
        state
            .editor
            .execute(Command::ConfigureCompositionRate {
                name: "Rational endpoint snapping".into(),
                width: 1280,
                height: 720,
                fps: FrameRate::new(30_000, 1001).unwrap(),
                duration: 150,
                display_start: 0,
            })
            .unwrap();
        state.editor.execute(Command::AddRectangle).unwrap();
        state.graph_property = Property::PositionX.into();
        for (frame, value) in [(10, 100.0), (30, 200.0), (60, 300.0)] {
            for edit in [
                TrackEdit::ToggleKey { frame },
                TrackEdit::Value { frame, value },
            ] {
                state
                    .editor
                    .execute(Command::EditTrack {
                        id: 1,
                        property: state.graph_property,
                        edit,
                    })
                    .unwrap();
            }
        }
        for (frame, incoming, slope) in [(10, false, 0.0), (30, true, 0.0), (30, false, 1e9)] {
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
        let keys = [10, 30].map(|frame| KeyRef {
            id: 1,
            property: state.graph_property,
            frame,
        });
        let track = state
            .editor
            .selected_layer()
            .unwrap()
            .track(state.graph_property)
            .unwrap();
        let mut targets = Targets::new(
            &state,
            track,
            &selection::snapshot(track, &keys, None),
            Some(false),
        );
        targets.values = vec![-1.0, 3.0];
        let raw = KeyVelocityScale {
            origin: 5e8,
            factor: -0.999999998,
        };
        let fps = state.editor.project().composition().fps().as_f64();
        let view = View {
            start: 0.0,
            span: 150.0,
            low: -1000.0 * fps,
            high: 1000.0 * fps,
        };
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(1000.0), px(400.0)));
        let valid = |scale| track.preview_key_velocity_scale(&[10, 30], scale).is_ok();
        assert!(valid(raw));
        assert!(!valid(KeyVelocityScale {
            factor: (-1.0 - 5e8) / 5e8,
            ..raw
        }));
        let (snapped, guides) = targets.velocity_scale(raw, 1e9, view, bounds, valid);
        assert!(valid(snapped));
        assert_eq!(
            guides,
            Guides {
                frame: None,
                value: Some(3.0 * fps)
            }
        );
        targets.values = vec![-1.0];
        let (still, guides) = targets.velocity_scale(raw, 1e9, view, bounds, valid);
        assert_eq!(still, raw);
        assert_eq!(guides, Guides::default());
    }
}
