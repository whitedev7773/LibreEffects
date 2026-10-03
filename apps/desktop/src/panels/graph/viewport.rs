//! Graph navigation changes only optional desktop view state.
use super::*;
use std::collections::BTreeSet;

pub(super) fn current(state: &EditorState, track: &AnimatedProperty) -> View {
    let fps = state.editor.project().composition().fps().as_f64();
    let mut result = view(
        track,
        state.timeline_start,
        state.visible_frames(),
        state.graph_view.speed,
        fps,
    );
    if let Some([low, high]) = state.graph_view.height {
        result.low = low;
        result.high = high;
        result
    } else {
        tangent::fit_view(
            result,
            &tangent::for_selection(
                track,
                &selection::active(state).iter().map(|k| k.frame).collect(),
            ),
            state.graph_view.speed,
            fps,
        )
    }
}

pub(super) fn fit(state: &mut EditorState, selected_only: bool) -> bool {
    let Some(layer) = state.editor.selected_layer() else {
        return false;
    };
    let Some(track) = layer.track(state.graph_property) else {
        return false;
    };
    let comp = state.editor.project().composition();
    let duration = comp.duration();
    let frames: BTreeSet<_> = if selected_only {
        selection::active(state).iter().map(|k| k.frame).collect()
    } else {
        track
            .keys()
            .keys()
            .copied()
            .filter(|&f| f < duration)
            .collect()
    };
    if selected_only && frames.is_empty() {
        return false;
    }
    let (first, last) = frames
        .first()
        .zip(frames.last())
        .map_or((0, duration.saturating_sub(1)), |(&a, &b)| (a, b));
    let padding = ((last - first) as f64 * 0.08).ceil().max(2.0) as u32;
    let from = first.saturating_sub(padding);
    let end = last.saturating_add(padding).min(duration);
    let zoom = (duration as f32 / (end - from).max(2) as f32).clamp(1.0, 64.0);
    let visible = ((duration as f32 / zoom).ceil() as u32).max(2);
    let start = ((first as u64 + last as u64).saturating_sub(visible as u64) / 2) as u32;
    let start = start.min(duration.saturating_sub(visible));
    let fps = comp.fps().as_f64();
    // Fit the chosen keys and the curve between them, independently of nearby,
    // unselected extreme values that happen to lie inside the padded time range.
    let fitted = tangent::fit_view(
        view(track, first, last - first, state.graph_view.speed, fps),
        &tangent::for_selection(track, &frames),
        state.graph_view.speed,
        fps,
    );
    state.timeline_start = start;
    state.timeline_zoom = zoom;
    state.graph_view.height = Some([fitted.low, fitted.high]);
    true
}

pub(super) fn vertical(view: View, delta: f64, anchor: f64, zoom: bool) -> [f64; 2] {
    let span = view.high - view.low;
    let (low, high) = if zoom {
        let size = (span * (-delta * 0.005).exp()).clamp(1e-6, 1e15);
        let fraction = anchor.clamp(0.0, 1.0);
        let value = view.low + span * fraction;
        (value - size * fraction, value + size * (1.0 - fraction))
    } else {
        let offset = (delta * span).clamp(-1e14, 1e14);
        (view.low + offset, view.high + offset)
    };
    if low.is_finite()
        && high.is_finite()
        && low.abs() <= 1e15
        && high.abs() <= 1e15
        && high - low >= 1e-6
    {
        [low, high]
    } else {
        [view.low, view.high]
    }
}

pub(super) fn horizontal(state: &mut EditorState, delta: f64, anchor: f64, zoom: bool) {
    let duration = state.editor.project().composition().duration();
    let before = state.visible_frames() as f64;
    let fraction = anchor.clamp(0.0, 1.0);
    let start = if zoom {
        state.timeline_zoom =
            (state.timeline_zoom as f64 * (delta * 0.005).exp()).clamp(1.0, 64.0) as f32;
        state.timeline_start as f64 + fraction * (before - state.visible_frames() as f64)
    } else {
        state.timeline_start as f64 - delta * before
    };
    state.timeline_start = start
        .round()
        .clamp(0.0, duration.saturating_sub(state.visible_frames()) as f64)
        as u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{KeyRef, Property};
    fn scene() -> EditorState {
        let mut s = EditorState::default();
        s.editor.execute(Command::AddRectangle).unwrap();
        for (frame, value) in [(10, 600.0), (30, 900.0), (60, 720.0), (120, 2000.0)] {
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
        s.graph_property = Property::PositionX.into();
        s
    }
    #[test]
    fn fitting_selected_keys_excludes_distant_extremes_and_never_edits_history() {
        let mut s = scene();
        let before = s.editor.project().clone();
        s.selected_keys = [10, 30]
            .map(|frame| KeyRef {
                id: 1,
                property: s.graph_property,
                frame,
            })
            .into();
        assert!(fit(&mut s, true));
        assert!(s.timeline_start <= 10 && s.timeline_start + s.visible_frames() >= 30);
        let [low, high] = s.graph_view.height.unwrap();
        assert!(low < 600.0 && high > 900.0 && high < 2000.0);
        assert_eq!(s.editor.project(), &before);
        assert!(fit(&mut s, false));
        assert!(s.timeline_start <= 10 && s.timeline_start + s.visible_frames() >= 120);
        assert!(s.graph_view.height.unwrap()[1] > 2000.0);
        s.editor.undo(); // Navigation did not add an Undo entry: last value edit is undone.
        assert_ne!(s.editor.project(), &before);
        s.editor.redo();
        assert_eq!(s.editor.project(), &before);
    }
    #[test]
    fn empty_single_key_and_signed_speed_fits_stay_finite() {
        let mut s = scene();
        assert!(!fit(&mut s, true));
        s.selected_keys.insert(KeyRef {
            id: 1,
            property: s.graph_property,
            frame: 30,
        });
        s.graph_view.speed = true;
        assert!(fit(&mut s, true));
        let [low, high] = s.graph_view.height.unwrap();
        assert!(low < -180.0 && high > 450.0); // both one-sided velocities
        s.editor
            .execute(Command::DeleteKeys(
                s.editor
                    .selected_layer()
                    .unwrap()
                    .property(Property::PositionX)
                    .keys()
                    .keys()
                    .map(|&frame| KeyRef {
                        id: 1,
                        property: s.graph_property,
                        frame,
                    })
                    .collect(),
            ))
            .unwrap();
        assert!(fit(&mut s, false));
        assert!(s.graph_view.height.unwrap().iter().all(|v| v.is_finite()));
    }
    #[test]
    fn navigation_preserves_cursor_anchor_and_clamps_composition_edges() {
        let mut s = scene();
        horizontal(&mut s, 100.0, 0.25, true);
        let frame = s.timeline_start as f64 + s.visible_frames() as f64 * 0.25;
        assert!((frame - 37.5).abs() <= 1.0);
        horizontal(&mut s, -1000.0, 0.0, false);
        assert_eq!(s.timeline_start + s.visible_frames(), 150);
        horizontal(&mut s, 1000.0, 0.0, false);
        assert_eq!(s.timeline_start, 0);
        let v = View {
            start: 0.0,
            span: 150.0,
            low: -100.0,
            high: 300.0,
        };
        let [a, b] = vertical(v, 100.0, 0.75, true);
        assert!((a + (b - a) * 0.75 - 200.0).abs() < 1e-9);
        assert!(b - a < 400.0);
        assert_eq!(vertical(v, 0.25, 0.0, false), [0.0, 400.0]);
        assert_eq!(vertical(v, f64::NAN, 0.5, true), [-100.0, 300.0]);
    }
}
