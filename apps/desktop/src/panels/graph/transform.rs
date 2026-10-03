//! Scalar graph selection scaling. Speed mode exposes time-only handles.
//! Pointer deltas, source keys and snapping use a frozen mouse-down snapshot.
use super::*;
use libre_effects_core::{KeyRef, KeyScale};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Handle(pub i8, pub i8);

#[derive(Clone, Copy)]
pub(super) struct SelectionBox {
    first: f64,
    last: f64,
    low: f64,
    high: f64,
    speed: bool,
    speed_ends: [[Option<f64>; 2]; 2],
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Project, Property};
    fn scene() -> (EditorState, View, Bounds<Pixels>) {
        let mut s = EditorState::default();
        s.editor.execute(Command::AddRectangle).unwrap();
        s.graph_property = Property::PositionX.into();
        for (frame, value) in [(30, 720.0), (50, 1020.0), (60, 720.0)] {
            s.editor
                .execute(Command::EditTrack {
                    id: 1,
                    property: s.graph_property,
                    edit: TrackEdit::ToggleKey { frame },
                })
                .unwrap();
            s.editor
                .execute(Command::EditTrack {
                    id: 1,
                    property: s.graph_property,
                    edit: TrackEdit::Value { frame, value },
                })
                .unwrap();
            s.selected_keys.insert(KeyRef {
                id: 1,
                property: s.graph_property,
                frame,
            });
        }
        (
            s,
            View {
                start: 0.0,
                span: 150.0,
                low: 500.0,
                high: 1500.0,
            },
            Bounds::new(point(px(100.0), px(100.0)), size(px(1000.0), px(400.0))),
        )
    }
    fn grab(s: &EditorState, view: View, bounds: Bounds<Pixels>, handle: Handle) -> Transform {
        let t = s
            .editor
            .selected_layer()
            .unwrap()
            .track(s.graph_property)
            .unwrap();
        let area = SelectionBox::new(
            t,
            &selection::active(s).iter().map(|k| k.frame).collect(),
            s.graph_view.speed,
            s.editor.project().composition().fps().as_f64(),
        )
        .unwrap();
        let (_, p) = area
            .handles(view, bounds)
            .into_iter()
            .find(|(h, _)| *h == handle)
            .unwrap();
        Transform::new(s, view, bounds, p + point(px(2.0), px(-1.0))).unwrap()
    }
    #[test]
    fn handles_scale_opposite_edge_or_center_without_pointer_jump() {
        let (s, view, bounds) = scene();
        for h in [
            Handle(-1, 0),
            Handle(1, 0),
            Handle(0, -1),
            Handle(0, 1),
            Handle(1, 1),
        ] {
            let mut t = grab(&s, view, bounds, h);
            for centered in [false, true] {
                let end = t.start + point(px(h.0 as f32 * 100.0), px(h.1 as f32 * -60.0));
                t.moving(end, centered, false);
                let (scale, track) = t.preview.as_ref().unwrap();
                let factor = if centered { 2.0 } else { 1.5 };
                assert!((scale.time_scale - if h.0 == 0 { 1.0 } else { factor }).abs() < 1e-9);
                assert!((scale.value_scale - if h.1 == 0 { 1.0 } else { factor }).abs() < 1e-9);
                let once = track.clone();
                t.moving(end, centered, false);
                assert_eq!(&t.preview.as_ref().unwrap().1, &once);
            }
            t.moving(t.start, false, false);
            assert_eq!(
                &t.preview.as_ref().unwrap().1,
                s.editor
                    .selected_layer()
                    .unwrap()
                    .track(s.graph_property)
                    .unwrap()
            );
        }
    }
    #[test]
    fn preview_commit_roundtrip_and_output_use_the_same_scaled_keys() {
        let (mut s, view, bounds) = scene();
        let before = s.editor.project().clone();
        let mut t = grab(&s, view, bounds, Handle(1, 1));
        t.moving(t.start + point(px(100.0), px(-120.0)), false, false);
        let preview = t.preview.as_ref().unwrap().1.clone();
        let (command, keys) = t.command().unwrap();
        assert_eq!(
            keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
            vec![30, 60, 75]
        );
        s.editor.execute(command).unwrap();
        assert_eq!(
            s.editor
                .selected_layer()
                .unwrap()
                .track(s.graph_property)
                .unwrap(),
            &preview
        );
        let saved = Project::from_json(&s.editor.project().to_json().unwrap()).unwrap();
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        s.editor.redo();
        assert_eq!(s.editor.project(), &saved);
        let r = crate::rendering::Renderer::new();
        for (frame, value) in [
            (30, 720.0),
            (45, 1020.0),
            (60, 1320.0),
            (67, 1040.0),
            (75, 720.0),
        ] {
            assert!((preview.value_at(frame) - value).abs() < 1e-8);
            assert_eq!(
                r.render(&saved, frame, 384).unwrap(),
                r.render_output(&saved, frame, 384, 216).unwrap()
            );
        }
    }
    #[test]
    fn invalid_drag_is_non_destructive_and_can_return_to_valid_positions() {
        let (mut s, view, bounds) = scene();
        s.selected_keys.retain(|k| k.frame != 60);
        let before = s.editor.project().clone();
        let mut t = grab(&s, view, bounds, Handle(1, 0));
        for dx in [66.66667, -200.0, 1000.0] {
            t.moving(t.start + point(px(dx), px(0.0)), false, false);
            assert!(t.preview.is_err());
            assert!(t.command().is_err());
            assert_eq!(s.editor.project(), &before);
        }
        t.moving(t.start + point(px(33.33333), px(0.0)), false, false);
        assert_eq!(t.frames(), [30, 55].into());
        assert!(t.command().is_ok());
        drop(t); // Cancel: preview never touches the document or its Undo history.
        assert_eq!(s.editor.project(), &before);
        s.editor.execute(Command::ToggleLocked(1)).unwrap();
        let t = s
            .editor
            .selected_layer()
            .unwrap()
            .track(s.graph_property)
            .unwrap();
        let area = SelectionBox::new(t, &[30, 50].into(), false, 30.0).unwrap();
        let p = area.handles(view, bounds)[0].1;
        assert!(Transform::new(&s, view, bounds, p).is_none());
        s.editor.undo();
    }
    #[test]
    fn flat_selection_only_has_time_handles_and_value_reflection_is_supported() {
        let (s, view, bounds) = scene();
        let track = s
            .editor
            .selected_layer()
            .unwrap()
            .track(s.graph_property)
            .unwrap();
        let flat = SelectionBox::new(track, &[30, 60].into(), false, 30.0).unwrap();
        assert_eq!(
            flat.handles(view, bounds)
                .iter()
                .map(|(h, _)| *h)
                .collect::<Vec<_>>(),
            vec![Handle(-1, 0), Handle(1, 0)]
        );
        assert!(SelectionBox::new(track, &[30].into(), false, 30.0).is_none());
        let mut t = grab(&s, view, bounds, Handle(0, 1));
        t.moving(t.start + point(px(0.0), px(240.0)), false, false);
        let (scale, preview) = t.preview.unwrap();
        assert_eq!(scale.value_scale, -1.0);
        assert_eq!(preview.keys()[&50].value, 420.0);
        assert_eq!(
            preview.keys().keys().copied().collect::<Vec<_>>(),
            vec![30, 50, 60]
        );
    }
    #[test]
    fn snapped_corner_preview_commit_undo_and_saved_output_agree() {
        let (mut s, view, bounds) = scene();
        for edit in [
            TrackEdit::ToggleKey { frame: 90 },
            TrackEdit::Value {
                frame: 90,
                value: 1320.0,
            },
        ] {
            s.editor
                .execute(Command::EditTrack {
                    id: 1,
                    property: s.graph_property,
                    edit,
                })
                .unwrap();
        }
        s.frame = 75;
        let before = s.editor.project().clone();
        let mut t = grab(&s, view, bounds, Handle(1, 1));
        // Raw edge: 75.75 frames and 1310 units, both within eight pixels.
        let end = t.start + point(px(105.0), px(-116.0));
        t.moving(end, false, true);
        assert_eq!(
            t.guides,
            snapping::Guides {
                frame: Some(75),
                value: Some(1320.0)
            }
        );
        let expected = t.preview.as_ref().unwrap().1.clone();
        t.moving(end, false, false);
        assert_eq!(t.guides, snapping::Guides::default());
        assert!((t.preview.as_ref().unwrap().1.keys()[&61].value - 1310.0).abs() < 1e-8);
        t.moving(end, false, true);
        let (command, moved) = t.command().unwrap();
        assert_eq!(
            moved.iter().map(|k| k.frame).collect::<Vec<_>>(),
            vec![30, 60, 75]
        );
        s.editor.execute(command).unwrap();
        assert_eq!(
            s.editor
                .selected_layer()
                .unwrap()
                .track(s.graph_property)
                .unwrap(),
            &expected
        );
        assert_eq!(expected.keys()[&90].value, 1320.0);
        let saved = Project::from_json(&s.editor.project().to_json().unwrap()).unwrap();
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        s.editor.redo();
        assert_eq!(s.editor.project(), &saved);
        let r = crate::rendering::Renderer::new();
        for f in [30, 45, 60, 67, 75, 90] {
            assert_eq!(
                r.render(&saved, f, 384).unwrap(),
                r.render_output(&saved, f, 384, 216).unwrap()
            );
        }
    }
}
impl SelectionBox {
    pub fn new(
        track: &AnimatedProperty,
        frames: &BTreeSet<u32>,
        speed: bool,
        fps: f64,
    ) -> Option<Self> {
        if frames.len() < 2 || frames.iter().any(|f| !track.keys().contains_key(f)) {
            return None;
        }
        // Include the entire active selection, even outside the viewport. Missing,
        // Hold-jump and singular endpoint sides are gaps, never invented velocities.
        let values: Vec<_> = if speed {
            frames
                .iter()
                .flat_map(|&f| speed::ends(track, f, fps))
                .map(|(_, v)| v)
                .filter(|v| v.is_finite())
                .collect()
        } else {
            frames.iter().map(|f| track.keys()[f].value).collect()
        };
        let (low, high) = if values.is_empty() {
            // No finite endpoint is still a valid time selection. Show its handles
            // on the zero line rather than mixing property values into speed units.
            (0.0, 0.0)
        } else {
            (
                values.iter().copied().fold(f64::INFINITY, f64::min),
                values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            )
        };
        Some(Self {
            first: *frames.first()? as f64,
            last: *frames.last()? as f64,
            low,
            high,
            speed,
            speed_ends: [*frames.first()?, *frames.last()?].map(|frame| {
                let ends = if speed {
                    speed::ends(track, frame, fps)
                } else {
                    vec![]
                };
                [false, true].map(|side| {
                    ends.iter().find_map(|&(incoming, value)| {
                        (incoming == side && value.is_finite()).then_some(value)
                    })
                })
            }),
        })
    }
    fn area(self, view: View, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        // Inset keys remain individually selectable inside the transform handles.
        Bounds::from_corners(
            view.point(bounds, self.first, self.high) - point(px(10.0), px(10.0)),
            view.point(bounds, self.last, self.low) + point(px(10.0), px(10.0)),
        )
    }
    fn handles(self, view: View, bounds: Bounds<Pixels>) -> Vec<(Handle, Point<Pixels>)> {
        let area = self.area(view, bounds);
        if self.speed {
            // The decorative 10px inset must not hide visible time edges, nor
            // expose a handle for a genuinely offscreen boundary. The stored
            // frame pivot and pointer delta are independent of this decoration.
            let inset_x = px(4.0).min(bounds.size.width / 2.0);
            let inset_y = px(4.0).min(bounds.size.height / 2.0);
            let middle = (area.top() + area.size.height / 2.0)
                .clamp(bounds.top() + inset_y, bounds.bottom() - inset_y);
            return [(self.first, area.left(), -1), (self.last, area.right(), 1)]
                .into_iter()
                .enumerate()
                .filter_map(|(index, (frame, decorated, side))| {
                    let true_x = view.point(bounds, frame, 0.0).x;
                    if true_x < bounds.left() || true_x > bounds.right() {
                        return None;
                    }
                    let x = decorated.clamp(bounds.left() + inset_x, bounds.right() - inset_x);
                    // Keep ordinary speed-key glyphs clickable, including flat
                    // selections at frame zero/final edge. Choose the closest
                    // free point along the side, then remain inside the plot.
                    let y = [0.0, -14.0, 14.0, -28.0, 28.0, -42.0, 42.0]
                        .into_iter()
                        .map(|offset| {
                            (middle + px(offset))
                                .clamp(bounds.top() + inset_y, bounds.bottom() - inset_y)
                        })
                        .find(|y| {
                            self.speed_ends[index]
                                .iter()
                                .enumerate()
                                .all(|(incoming, value)| {
                                    value.is_none_or(|value| {
                                        let key_x =
                                            true_x + px(if incoming == 1 { -5.0 } else { 5.0 });
                                        let key_y = view.point(bounds, frame, value).y;
                                        f32::from(x - key_x).abs() > 10.0
                                            || f32::from(*y - key_y).abs() > 10.0
                                    })
                                })
                        })?;
                    Some((Handle(side, 0), point(x, y)))
                })
                .collect();
        }
        let mut handles = Vec::new();
        for y in [-1, 0, 1] {
            for x in [-1, 0, 1] {
                if (x == 0 && y == 0) || (y != 0 && self.high - self.low < 1e-9) {
                    continue;
                }
                handles.push((
                    Handle(x, y),
                    point(
                        area.left() + area.size.width * ((x + 1) as f32 / 2.0),
                        area.bottom() - area.size.height * ((y + 1) as f32 / 2.0),
                    ),
                ));
            }
        }
        handles
    }
    pub fn hit(self, view: View, bounds: Bounds<Pixels>, p: Point<Pixels>) -> Option<Handle> {
        if !bounds.contains(&p) {
            return None;
        }
        self.handles(view, bounds).into_iter().find_map(|(h, at)| {
            (bounds.contains(&at)
                && (f32::from(p.x - at.x)).abs() <= 6.0
                && (f32::from(p.y - at.y)).abs() <= 6.0)
                .then_some(h)
        })
    }
    pub fn paint(self, view: View, bounds: Bounds<Pixels>, window: &mut Window, invalid: bool) {
        let area = self.area(view, bounds);
        let color = if invalid { 0xff6b6b } else { ui::BLUE };
        stroke(
            window,
            [
                area.origin,
                point(area.right(), area.top()),
                point(area.right(), area.bottom()),
                point(area.left(), area.bottom()),
                area.origin,
            ],
            color,
            1.0,
        );
        for (_, at) in self.handles(view, bounds) {
            window.paint_quad(fill(
                Bounds::new(at - point(px(3.0), px(3.0)), size(px(6.0), px(6.0))),
                rgb(color),
            ));
        }
    }
}

#[derive(Clone)]
pub(super) struct Transform {
    pub view: View,
    bounds: Bounds<Pixels>,
    start: Point<Pixels>,
    area: SelectionBox,
    handle: Handle,
    pub keys: Vec<KeyRef>,
    pub active: usize,
    revision: u64,
    tool: Tool,
    snap: bool,
    original: AnimatedProperty,
    duration: u32,
    targets: snapping::Targets,
    pub guides: snapping::Guides,
    pub preview: Result<(KeyScale, AnimatedProperty), String>,
    pub moved: bool,
}
impl Transform {
    pub fn new(
        state: &EditorState,
        view: View,
        bounds: Bounds<Pixels>,
        start: Point<Pixels>,
    ) -> Option<Self> {
        if state.editor.selected_layer()?.locked() {
            return None;
        }
        let track = state.editor.selected_layer()?.track(state.graph_property)?;
        let keys = selection::active(state);
        let area = SelectionBox::new(
            track,
            &keys.iter().map(|k| k.frame).collect(),
            state.graph_view.speed,
            state.editor.project().composition().fps().as_f64(),
        )?;
        let handle = area.hit(view, bounds, start)?;
        let targets =
            snapping::Targets::new(state, track, &selection::snapshot(track, &keys, None), None);
        let identity = KeyScale {
            time_origin: 0.0,
            time_scale: 1.0,
            value_origin: 0.0,
            value_scale: 1.0,
        };
        Some(Self {
            view,
            bounds,
            start,
            area,
            handle,
            active: selected(state)
                .and_then(|(id, frame, _)| keys.iter().position(|k| k.id == id && k.frame == frame))
                .unwrap_or(0),
            keys,
            revision: state.document_revision,
            tool: state.tool,
            snap: state.snapping,
            original: track.clone(),
            duration: state.editor.project().composition().duration(),
            targets,
            guides: Default::default(),
            preview: Ok((identity, track.clone())),
            moved: false,
        })
    }
    /// Reject stale previews even if an observer has not run before mouse-up.
    pub fn is_current(&self, state: &EditorState) -> bool {
        state.document_revision == self.revision
            && state.tool == self.tool
            && state.graph_view.speed == self.area.speed
            && selection::active(state) == self.keys
            && state.editor.selected_layer().is_some_and(|layer| {
                !layer.locked() && layer.track(state.graph_property) == Some(&self.original)
            })
    }
    pub fn has_changes(&self) -> bool {
        self.preview
            .as_ref()
            .is_ok_and(|(_, track)| track != &self.original)
    }
    /// Ctrl inverts the mouse-down snap switch; Alt changes the pivot only.
    pub fn update_pointer(&mut self, end: Point<Pixels>, center: bool, control: bool) {
        self.moving(end, center, self.snap ^ control);
    }
    fn scale(&self, end: Point<Pixels>, center: bool) -> KeyScale {
        let delta = end - self.start;
        let dt = f32::from(delta.x) as f64 / f32::from(self.bounds.size.width).max(1.0) as f64
            * self.view.span;
        let dv = if self.area.speed {
            0.0
        } else {
            -(f32::from(delta.y) as f64) / f32::from(self.bounds.size.height).max(1.0) as f64
                * (self.view.high - self.view.low)
        };
        let a = self.area;
        let time_origin = if center {
            (a.first + a.last) / 2.0
        } else if self.handle.0 < 0 {
            a.last
        } else {
            a.first
        };
        let value_origin = if self.area.speed {
            0.0
        } else if center {
            (a.low + a.high) / 2.0
        } else if self.handle.1 < 0 {
            a.high
        } else {
            a.low
        };
        let time_edge = if self.handle.0 < 0 { a.first } else { a.last };
        let value_edge = if self.handle.1 < 0 { a.low } else { a.high };
        KeyScale {
            time_origin,
            value_origin,
            time_scale: if self.handle.0 == 0 {
                1.0
            } else {
                1.0 + dt / (time_edge - time_origin)
            },
            value_scale: if self.handle.1 == 0 {
                1.0
            } else {
                1.0 + dv / (value_edge - value_origin)
            },
        }
    }
    pub fn moving(&mut self, end: Point<Pixels>, center: bool, snap: bool) {
        let delta = end - self.start;
        let distance = f32::from(delta.x).abs()
            + if self.area.speed {
                0.0
            } else {
                f32::from(delta.y).abs()
            };
        if !self.moved && distance < 3.0 {
            return;
        }
        self.moved = true;
        let mut scale = self.scale(end, center);
        self.guides = Default::default();
        if snap {
            let time_edge =
                (self.handle.0 != 0 && delta.x != px(0.0)).then_some(if self.handle.0 < 0 {
                    self.area.first
                } else {
                    self.area.last
                });
            let value_edge =
                (self.handle.1 != 0 && delta.y != px(0.0)).then_some(if self.handle.1 < 0 {
                    self.area.low
                } else {
                    self.area.high
                });
            (scale, self.guides) = self.targets.scale(
                scale,
                time_edge,
                value_edge,
                &self.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
                self.view,
                self.bounds,
                self.duration,
            );
        }
        self.preview = self
            .original
            .preview_key_scale(
                &self.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
                scale,
                self.duration,
            )
            .map(|track| (scale, track));
        if self.preview.is_err() {
            self.guides = Default::default();
        }
    }
    pub fn frames(&self) -> BTreeSet<u32> {
        self.keys
            .iter()
            .map(|k| {
                self.preview
                    .as_ref()
                    .ok()
                    .and_then(|(scale, _)| scale.frame(k.frame, self.duration).ok())
                    .unwrap_or(k.frame)
            })
            .collect()
    }
    pub fn command(&self) -> Result<(Command, Vec<KeyRef>), String> {
        let (scale, _) = self.preview.as_ref().map_err(Clone::clone)?;
        let moved = self
            .keys
            .iter()
            .map(|key| {
                Ok(KeyRef {
                    frame: scale.frame(key.frame, self.duration)?,
                    ..*key
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok((
            Command::ScaleKeys {
                keys: self.keys.clone(),
                scale: *scale,
            },
            moved,
        ))
    }
}

#[cfg(test)]
#[path = "transform_speed_tests.rs"]
mod speed_tests;
