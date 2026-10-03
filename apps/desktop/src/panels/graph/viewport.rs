//! Graph navigation changes only optional desktop view state.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(super) struct Pan {
    view: View,
    time: (u32, f32),
    height: Option<[f64; 2]>,
    bounds: Bounds<Pixels>,
    start: Point<Pixels>,
    pub button: MouseButton,
}
impl Pan {
    pub fn new(
        state: &EditorState,
        view: View,
        bounds: Bounds<Pixels>,
        event: &MouseDownEvent,
    ) -> Self {
        Self {
            view,
            time: (state.timeline_start, state.timeline_zoom),
            height: state.graph_view.height,
            bounds,
            start: event.position,
            button: event.button,
        }
    }
    pub fn apply(self, state: &mut EditorState, position: Point<Pixels>) {
        let delta = position - self.start;
        let dx = f32::from(delta.x) as f64 / f32::from(self.bounds.size.width).max(1.0) as f64;
        let dy = f32::from(delta.y) as f64 / f32::from(self.bounds.size.height).max(1.0) as f64;
        self.restore(state);
        horizontal(state, dx, 0.0, false);
        if self.height.is_some() {
            state.graph_view.height = Some(vertical(self.view, dy, 0.0, false));
        }
    }
    pub fn restore(self, state: &mut EditorState) {
        (state.timeline_start, state.timeline_zoom) = self.time;
        state.graph_view.height = self.height;
    }
}

#[derive(Clone, Copy)]
pub(super) struct Zoom {
    origin: Pan,
    pub scrub: bool,
}
impl Zoom {
    pub fn new(
        state: &EditorState,
        view: View,
        bounds: Bounds<Pixels>,
        event: &MouseDownEvent,
    ) -> Self {
        Self {
            origin: Pan::new(state, view, bounds, event),
            scrub: event.modifiers.alt,
        }
    }
    pub fn restore(self, state: &mut EditorState) {
        self.origin.restore(state);
    }
    fn delta(self, end: Point<Pixels>) -> (f64, f64) {
        let delta = end - self.origin.start;
        (f32::from(delta.x) as f64, f32::from(delta.y) as f64)
    }
    fn scale(self, state: &mut EditorState, x: f64, y: f64) {
        self.restore(state);
        let p = self.origin.start;
        let b = self.origin.bounds;
        let fx = f32::from(p.x - b.left()) as f64 / f32::from(b.size.width).max(1.0) as f64;
        let fy = f32::from(b.bottom() - p.y) as f64 / f32::from(b.size.height).max(1.0) as f64;
        horizontal(state, x, fx, true);
        if self.origin.height.is_some() {
            state.graph_view.height = Some(vertical(self.origin.view, y, fy, true));
        }
    }
    pub fn moving(self, state: &mut EditorState, end: Point<Pixels>) {
        if self.scrub {
            let (dx, dy) = self.delta(end);
            if dx.abs().max(dy.abs()) >= 3.0 {
                self.scale(state, dx * 2.0, -dy * 2.0);
            } else {
                self.restore(state);
            }
        }
    }
    pub fn area(self, end: Point<Pixels>) -> Option<Bounds<Pixels>> {
        let (dx, dy) = self.delta(end);
        if self.scrub || dx.abs().max(dy.abs()) < 3.0 {
            return None;
        }
        let b = self.origin.bounds;
        let a = self.origin.start;
        let left = a.x.min(end.x).max(b.left()).min(b.right());
        let right = a.x.max(end.x).max(b.left()).min(b.right());
        let top = a.y.min(end.y).max(b.top()).min(b.bottom());
        let bottom = a.y.max(end.y).max(b.top()).min(b.bottom());
        Some(Bounds::new(
            point(left, top),
            size(right - left, bottom - top),
        ))
    }
    pub fn finish(self, state: &mut EditorState, end: Point<Pixels>) {
        let (dx, dy) = self.delta(end);
        if dx.abs().max(dy.abs()) < 3.0 {
            let step = 2.0_f64.ln() / 0.005 * if self.scrub { -1.0 } else { 1.0 };
            self.scale(state, step, step);
        } else if self.scrub {
            self.moving(state, end);
        } else if let Some(area) = self.area(end) {
            self.restore(state);
            let view = self.origin.view;
            let (from, high) = view.value(self.origin.bounds, area.origin);
            let (to, low) = view.value(self.origin.bounds, area.bottom_right());
            if area.size.width >= px(3.0) {
                let duration = state.editor.project().composition().duration();
                state.timeline_zoom =
                    (duration as f64 / (to - from).max(2.0)).clamp(1.0, 64.0) as f32;
                let span = state.visible_frames();
                state.timeline_start = ((from + to - span as f64) / 2.0)
                    .round()
                    .clamp(0.0, duration.saturating_sub(span) as f64)
                    as u32;
            }
            if self.origin.height.is_some() && area.size.height >= px(3.0) {
                // Use the same finite range limits as wheel navigation.
                let factor = ((view.high - view.low) / (high - low).max(1e-6)).ln() / 0.005;
                let center = (low + high) / 2.0;
                let half_span = (view.high - view.low) / 2.0;
                let centered = View {
                    low: center - half_span,
                    high: center + half_span,
                    ..view
                };
                state.graph_view.height = Some(vertical(centered, factor, 0.5, true));
            }
        }
    }
}

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
    fn zoom_scene(automatic: bool, alt: bool) -> (EditorState, Zoom) {
        let mut s = scene();
        s.timeline_zoom = 3.0;
        s.timeline_start = 20;
        s.graph_view.height = (!automatic).then_some([-100.0, 300.0]);
        let v = View {
            start: 20.0,
            span: 50.0,
            low: -100.0,
            high: 300.0,
        };
        let bounds = Bounds::new(point(px(100.0), px(200.0)), size(px(1000.0), px(200.0)));
        let event = MouseDownEvent {
            position: point(px(350.0), px(350.0)),
            modifiers: gpui::Modifiers {
                alt,
                ..Default::default()
            },
            ..Default::default()
        };
        let zoom = Zoom::new(&s, v, bounds, &event);
        (s, zoom)
    }
    #[test]
    fn zoom_click_and_alt_click_keep_pointer_time_and_value() {
        for automatic in [false, true] {
            for alt in [false, true] {
                let (mut s, zoom) = zoom_scene(automatic, alt);
                zoom.finish(&mut s, zoom.origin.start);
                let factor = if alt { 0.5 } else { 2.0 };
                assert_eq!(s.timeline_zoom, 3.0 * factor);
                // The frame under the pointer remains within the integer viewport precision.
                assert!(
                    (s.timeline_start as f64 + s.visible_frames() as f64 * 0.25 - 32.5).abs()
                        <= 0.5
                );
                if automatic {
                    assert_eq!(s.graph_view.height, None);
                } else {
                    let [low, high] = s.graph_view.height.unwrap();
                    assert!((low + (high - low) * 0.25).abs() < 1e-9);
                    assert!((high - low - 400.0 / factor as f64).abs() < 1e-9);
                }
            }
        }
    }
    #[test]
    fn zoom_marquee_handles_reverse_drag_single_axis_and_plot_edges() {
        for automatic in [false, true] {
            let (mut s, mut zoom) = zoom_scene(automatic, false);
            let a = point(px(350.0), px(250.0));
            let b = point(px(850.0), px(350.0));
            zoom.origin.start = a;
            zoom.finish(&mut s, b);
            assert_eq!((s.timeline_start, s.visible_frames()), (33, 25));
            if let Some([low, high]) = s.graph_view.height {
                assert!(low.abs() < 1e-9 && (high - 200.0).abs() < 1e-9);
            } else {
                assert!(automatic);
            }
            let expected = (s.timeline_start, s.timeline_zoom, s.graph_view.height);
            zoom.origin.start = b;
            zoom.finish(&mut s, a);
            assert_eq!(
                (s.timeline_start, s.timeline_zoom, s.graph_view.height),
                expected
            );
            // Nearly vertical marquee only changes height.
            zoom.finish(&mut s, point(b.x + px(1.0), a.y));
            assert_eq!((s.timeline_start, s.timeline_zoom), (20, 3.0));
            // Release beyond the plot clamps the target rectangle to visible content.
            let outside = point(px(2000.0), px(-500.0));
            let area = zoom.area(outside).unwrap();
            assert_eq!(area.right(), zoom.origin.bounds.right());
            assert_eq!(area.top(), zoom.origin.bounds.top());
            zoom.finish(&mut s, outside);
            assert!(s.timeline_start + s.visible_frames() <= 150);
            assert_eq!(s.graph_view.height.is_none(), automatic);
        }
    }
    #[test]
    fn zoom_scrub_is_absolute_cancelable_and_never_edits_the_document() {
        for automatic in [false, true] {
            let (mut s, zoom) = zoom_scene(automatic, true);
            s.editor.execute(Command::ToggleLocked(1)).unwrap();
            let key = KeyRef {
                id: 1,
                property: Property::PositionX.into(),
                frame: 30,
            };
            s.selected_keys.insert(key);
            s.graph_key = Some((1, 30));
            s.frame = 30;
            let before = s.editor.project().clone();
            let destination = zoom.origin.start + point(px(100.0), px(-50.0));
            zoom.moving(&mut s, destination);
            let expected = (s.timeline_start, s.timeline_zoom, s.graph_view.height);
            zoom.moving(&mut s, destination);
            zoom.finish(&mut s, destination);
            assert_eq!(
                (s.timeline_start, s.timeline_zoom, s.graph_view.height),
                expected
            );
            assert!(s.timeline_zoom > 3.0);
            if let Some([low, high]) = s.graph_view.height {
                assert!(high - low < 400.0);
            }
            zoom.restore(&mut s);
            assert_eq!((s.timeline_start, s.timeline_zoom), (20, 3.0));
            assert_eq!(s.graph_view.height, (!automatic).then_some([-100.0, 300.0]));
            zoom.finish(&mut s, point(px(1e8), px(-1e8)));
            assert_eq!(s.timeline_zoom, 64.0);
            assert_eq!(s.selected_keys, [key].into());
            assert_eq!((s.graph_key, s.frame), (Some((1, 30)), 30));
            assert_eq!(s.editor.project(), &before);
            s.editor.undo();
            assert!(!s.editor.selected_layer().unwrap().locked());
            s.editor.redo();
            let mut views = crate::view_state::ProjectViews::default();
            views.compositions.insert(
                1,
                crate::view_state::CompositionView {
                    graph_view: s.graph_view.clone(),
                    timeline_start: s.timeline_start,
                    timeline_zoom: s.timeline_zoom,
                    ..Default::default()
                },
            );
            let json = views.write(s.editor.project()).unwrap();
            let loaded = libre_effects_core::Project::from_json(&json).unwrap();
            assert_eq!(loaded, before);
            assert_eq!(crate::view_state::ProjectViews::read(&json, &loaded), views);
            let renderer = crate::rendering::Renderer::new();
            for frame in [10, 20, 30, 60] {
                assert_eq!(
                    renderer.render(&before, frame, 384).unwrap(),
                    renderer.render_output(&loaded, frame, 384, 216).unwrap()
                );
            }
        }
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
    #[test]
    fn pointer_pan_is_absolute_cancelable_and_preserves_selection_and_document() {
        for button in [MouseButton::Left, MouseButton::Middle] {
            for automatic in [false, true] {
                let mut s = scene();
                s.timeline_zoom = 2.5;
                s.timeline_start = 45;
                s.frame = 30;
                s.graph_view.height = (!automatic).then_some([-100.0, 300.0]);
                let key = KeyRef {
                    id: 1,
                    property: s.graph_property,
                    frame: 30,
                };
                s.selected_keys.insert(key);
                s.graph_key = Some((1, 30));
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
                let before = s.editor.project().clone();
                let bounds = Bounds::new(point(px(100.0), px(100.0)), size(px(1000.0), px(200.0)));
                let event = MouseDownEvent {
                    button,
                    position: point(px(500.0), px(150.0)),
                    ..Default::default()
                };
                let v = current(
                    &s,
                    s.editor
                        .selected_layer()
                        .unwrap()
                        .track(s.graph_property)
                        .unwrap(),
                );
                let pan = Pan::new(&s, v, bounds, &event);
                let destination = event.position + point(px(100.0), px(50.0));
                pan.apply(&mut s, destination);
                pan.apply(&mut s, destination); // Repeated mouse events must not accumulate drift.
                assert_eq!((s.timeline_start, s.timeline_zoom, s.frame), (39, 2.5, 30));
                assert_eq!(s.graph_view.height, (!automatic).then_some([0.0, 400.0]));
                assert_eq!(s.selected_keys, [key].into());
                assert_eq!(s.graph_key, Some((1, 30)));
                assert_eq!(s.editor.project(), &before);
                pan.restore(&mut s);
                assert_eq!(s.timeline_start, 45);
                assert_eq!(s.graph_view.height, (!automatic).then_some([-100.0, 300.0]));
                s.editor.undo();
                assert!(!s.editor.selected_layer().unwrap().locked());
                s.editor.redo();
                assert_eq!(s.editor.project(), &before);
            }
        }
    }
    #[test]
    fn pointer_pan_clamps_time_and_saved_view_never_changes_output() {
        let mut s = scene();
        s.timeline_zoom = 3.0;
        s.timeline_start = 20;
        s.graph_view.height = Some([400.0, 2200.0]);
        let track = s
            .editor
            .selected_layer()
            .unwrap()
            .track(s.graph_property)
            .unwrap();
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(500.0), px(200.0)));
        let pan = Pan::new(&s, current(&s, track), bounds, &MouseDownEvent::default());
        pan.apply(&mut s, point(px(-5000.0), px(40.0)));
        assert_eq!(s.timeline_start, 100);
        pan.apply(&mut s, point(px(5000.0), px(40.0)));
        assert_eq!(s.timeline_start, 0);
        let mut views = crate::view_state::ProjectViews::default();
        views.compositions.insert(
            1,
            crate::view_state::CompositionView {
                graph_view: s.graph_view.clone(),
                timeline_start: s.timeline_start,
                timeline_zoom: s.timeline_zoom,
                ..Default::default()
            },
        );
        let json = views.write(s.editor.project()).unwrap();
        let loaded = libre_effects_core::Project::from_json(&json).unwrap();
        assert_eq!(&loaded, s.editor.project());
        assert_eq!(crate::view_state::ProjectViews::read(&json, &loaded), views);
        let renderer = crate::rendering::Renderer::new();
        for frame in [10, 20, 30, 60] {
            assert_eq!(
                renderer.render(s.editor.project(), frame, 384).unwrap(),
                renderer.render_output(&loaded, frame, 384, 216).unwrap()
            );
        }
    }
}
