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
