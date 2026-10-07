use super::*;
use libre_effects_core::TemporalHandle;

#[derive(Clone, Copy)]
pub(super) struct Tangent {
    pub frame: u32,
    pub incoming: bool,
    value: f64,
    span: f64,
    pub handle: TemporalHandle,
}
impl Tangent {
    pub fn new(track: &AnimatedProperty, frame: u32, incoming: bool) -> Option<Self> {
        let neighbor = if incoming {
            *track.keys().range(..frame).next_back()?.0
        } else {
            *track
                .keys()
                .range((std::ops::Bound::Excluded(frame), std::ops::Bound::Unbounded))
                .next()?
                .0
        };
        let handle = track.temporal_handle(frame, incoming)?;
        handle.valid().then_some(Self {
            frame,
            incoming,
            value: track.keys().get(&frame)?.value,
            span: frame.abs_diff(neighbor) as f64,
            handle,
        })
    }
    fn direction(self) -> f64 {
        if self.incoming { -1.0 } else { 1.0 }
    }
    pub fn points(
        self,
        view: View,
        bounds: Bounds<Pixels>,
        speed: bool,
        fps: f64,
    ) -> (Point<Pixels>, Point<Pixels>) {
        let dt = self.direction() * self.span * self.handle.influence;
        let base = if speed {
            self.handle.slope * fps
        } else {
            self.value
        };
        let value = if speed {
            base
        } else {
            base + self.handle.slope * dt
        };
        (
            view.point(bounds, self.frame as f64, base),
            view.point(bounds, self.frame as f64 + dt, value),
        )
    }
    /// Relative pointer motion avoids a jump when the handle is grabbed off-center.
    pub fn dragged(
        self,
        view: View,
        bounds: Bounds<Pixels>,
        speed: bool,
        fps: f64,
        delta: Point<Pixels>,
        keep_speed: bool,
    ) -> TemporalHandle {
        let (_, end) = self.points(view, bounds, speed, fps);
        let (time, value) = view.value(bounds, end + delta);
        let influence =
            ((time - self.frame as f64) * self.direction() / self.span).clamp(0.001, 1.0);
        let slope = if keep_speed {
            self.handle.slope
        } else if speed {
            value / fps
        } else {
            (value - self.value) / (self.direction() * self.span * influence)
        };
        TemporalHandle {
            influence,
            slope: slope.clamp(-1e9, 1e9),
        }
    }
    pub fn command(
        self,
        id: LayerId,
        property: PropertyPath,
        handle: TemporalHandle,
        split: bool,
    ) -> Command {
        let edit = Command::SetTemporalHandle {
            id,
            property,
            frame: self.frame,
            incoming: self.incoming,
            handle,
        };
        if split {
            Command::Batch(vec![
                Command::SetTemporalMode {
                    id,
                    property,
                    frame: self.frame,
                    mode: TemporalMode::Independent,
                },
                edit,
            ])
        } else {
            edit
        }
    }
}

pub(super) fn for_selection(
    track: &AnimatedProperty,
    frames: &std::collections::BTreeSet<u32>,
) -> Vec<Tangent> {
    frames
        .iter()
        .flat_map(|&f| {
            [true, false]
                .into_iter()
                .filter_map(move |side| Tangent::new(track, f, side))
        })
        .collect()
}

pub(super) fn fit_view(mut view: View, tangents: &[Tangent], speed: bool, fps: f64) -> View {
    let mut low = view.low;
    let mut high = view.high;
    for t in tangents {
        if (t.frame as f64) < view.start || t.frame as f64 > view.start + view.span {
            continue;
        }
        let value = if speed {
            t.handle.slope * fps
        } else {
            t.value + t.direction() * t.span * t.handle.influence * t.handle.slope
        };
        low = low.min(value);
        high = high.max(value);
    }
    if low < view.low || high > view.high {
        let padding = ((high - low) * 0.08).max(1.0);
        view.low = low - padding;
        view.high = high + padding;
    }
    view
}

pub(super) fn paint(
    window: &mut Window,
    tangent: Tangent,
    view: View,
    bounds: Bounds<Pixels>,
    speed: bool,
    fps: f64,
) {
    let (key, end) = tangent.points(view, bounds, speed, fps);
    stroke(window, [key, end], ui::BLUE, 1.0);
    // Hollow diamonds distinguish direction handles from square keyframe markers.
    stroke(
        window,
        [
            end + point(px(0.0), px(-4.0)),
            end + point(px(4.0), px(0.0)),
            end + point(px(0.0), px(4.0)),
            end + point(px(-4.0), px(0.0)),
            end + point(px(0.0), px(-4.0)),
        ],
        ui::BLUE,
        1.5,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, Project, Property};
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        for (frame, value) in [(0, 600.0), (30, 900.0), (60, 600.0)] {
            e.execute(Command::ToggleKeyframe {
                id: 1,
                property: Property::PositionX,
                frame,
            })
            .unwrap();
            e.execute(Command::SetValue {
                id: 1,
                property: Property::PositionX,
                frame,
                value,
            })
            .unwrap();
        }
        e
    }
    fn track(e: &Editor) -> &AnimatedProperty {
        e.selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .expect("2D test layer has independent Position tracks")
    }
    fn geometry() -> (View, Bounds<Pixels>) {
        (
            View {
                start: 0.0,
                span: 60.0,
                low: -600.0,
                high: 1200.0,
            },
            Bounds::new(point(px(100.0), px(100.0)), size(px(600.0), px(300.0))),
        )
    }
    #[test]
    fn incoming_outgoing_pointer_mapping_clamps_influence_and_preserves_shift_speed() {
        let e = scene();
        let (v, b) = geometry();
        for incoming in [true, false] {
            let t = Tangent::new(track(&e), 30, incoming).unwrap();
            for speed in [false, true] {
                let (_, end) = t.points(v, b, speed, 30.0);
                let want = TemporalHandle {
                    slope: -4.0,
                    influence: 0.7,
                };
                let (_, to) = Tangent { handle: want, ..t }.points(v, b, speed, 30.0);
                let h = t.dragged(v, b, speed, 30.0, to - end, false);
                assert!((h.slope - want.slope).abs() < 1e-5);
                assert!((h.influence - want.influence).abs() < 1e-5);
                let h = t.dragged(v, b, speed, 30.0, to - end, true);
                assert_eq!(h.slope, t.handle.slope);
                let h = t.dragged(v, b, speed, 30.0, point(px(10000.0), px(0.0)), true);
                assert_eq!(h.influence, if incoming { 0.001 } else { 1.0 });
            }
        }
        assert!(Tangent::new(track(&e), 0, true).is_none());
        assert!(Tangent::new(track(&e), 60, false).is_none());
    }
    #[test]
    fn preview_matches_commit_linked_or_split_and_one_undo_restores_auto() {
        for split in [false, true] {
            let mut e = scene();
            let p = Property::PositionX.into();
            e.execute(Command::SetTemporalMode {
                id: 1,
                property: p,
                frame: 30,
                mode: TemporalMode::Auto,
            })
            .unwrap();
            let before = e.project().clone();
            let t = Tangent::new(track(&e), 30, false).unwrap();
            let h = TemporalHandle {
                slope: -4.0,
                influence: 0.7,
            };
            let preview = track(&e)
                .preview_temporal_handle(30, false, h, split)
                .unwrap();
            assert_eq!(e.project(), &before);
            e.execute(t.command(1, p, h, split)).unwrap();
            assert_eq!(track(&e), &preview);
            let key = &track(&e).keys()[&30];
            assert_eq!(
                key.temporal.mode,
                if split {
                    TemporalMode::Independent
                } else {
                    TemporalMode::Continuous
                }
            );
            assert_eq!(
                key.temporal.incoming.unwrap().slope,
                if split { 0.0 } else { -4.0 }
            );
            assert_eq!(key.temporal.incoming.unwrap().influence, 1.0 / 3.0);
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &saved);
        }
    }
    #[test]
    fn invalid_and_locked_edits_are_atomic_and_flat_curves_have_handles() {
        let mut e = scene();
        let p = Property::PositionX.into();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame: 30,
            value: 600.0,
        })
        .unwrap();
        let t = Tangent::new(track(&e), 30, false).unwrap();
        assert_eq!(t.handle.slope, 0.0);
        let before = e.project().clone();
        let bad = TemporalHandle {
            slope: f64::NAN,
            influence: 0.5,
        };
        assert!(
            track(&e)
                .preview_temporal_handle(30, false, bad, true)
                .is_err()
        );
        assert!(e.execute(t.command(1, p, bad, true)).is_err());
        assert_eq!(e.project(), &before);
        e.execute(Command::ToggleLocked(1)).unwrap();
        let locked = e.project().clone();
        assert!(
            e.execute(t.command(
                1,
                p,
                TemporalHandle {
                    slope: 4.0,
                    influence: 0.7
                },
                true
            ))
            .is_err()
        );
        assert_eq!(e.project(), &locked);
    }
    #[test]
    fn selected_handles_fit_height_and_hold_and_missing_sides_are_not_drawn() {
        let mut e = scene();
        let p = Property::PositionX.into();
        e.execute(Command::SetTemporalHandle {
            id: 1,
            property: p,
            frame: 30,
            incoming: true,
            handle: TemporalHandle {
                slope: -50.0,
                influence: 0.9,
            },
        })
        .unwrap();
        let ts = for_selection(track(&e), &[30].into());
        let (v, b) = geometry();
        for speed in [false, true] {
            let fitted = fit_view(v, &ts, speed, 30.0);
            assert!(
                ts.iter()
                    .all(|t| b.contains(&t.points(fitted, b, speed, 30.0).1))
            );
        }
        e.execute(Command::SetInterpolation {
            id: 1,
            property: Property::PositionX,
            frame: 0,
            interpolation: Interpolation::Hold,
        })
        .unwrap();
        assert!(Tangent::new(track(&e), 0, false).is_none());
        assert!(Tangent::new(track(&e), 30, true).is_none());
        assert!(Tangent::new(track(&e), u32::MAX, false).is_none());
    }
    #[test]
    fn pointer_handle_edit_matches_saved_preview_and_output_pixels() {
        let mut e = scene();
        let (v, b) = geometry();
        let t = Tangent::new(track(&e), 30, true).unwrap();
        let target = Tangent {
            handle: TemporalHandle {
                slope: 4.0,
                influence: 1.0 / 3.0,
            },
            ..t
        };
        let delta = target.points(v, b, false, 30.0).1 - t.points(v, b, false, 30.0).1;
        let handle = t.dragged(v, b, false, 30.0, delta, false);
        e.execute(t.command(1, Property::PositionX.into(), handle, false))
            .unwrap();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let renderer = crate::rendering::Renderer::new();
        for (frame, want) in [(15, 772.5), (45, 750.0)] {
            let preview = renderer.render(e.project(), frame, 384).unwrap();
            let output = renderer.render_output(&saved, frame, 384, 216).unwrap();
            assert_eq!(preview, output);
            let xs = output
                .enumerate_pixels()
                .filter(|(_, _, p)| p[3] > 0)
                .map(|(x, _, _)| x)
                .collect::<Vec<_>>();
            let center = (*xs.iter().min().unwrap() + *xs.iter().max().unwrap() + 1) as f64 / 2.0;
            assert!((center - want / 5.0).abs() < 1.0);
        }
    }
}
