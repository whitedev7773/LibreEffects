//! Timeline key symbols describe incoming and outgoing interpolation independently.
use gpui::{Bounds, PathBuilder, Pixels, Point, Window, canvas, point, prelude::*, px, rgb};
use libre_effects_core::{AnimatedProperty, Interpolation, Layer, PropertyPath, TemporalMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Linear,
    Hold,
    Auto,
    Bezier,
    Mixed,
}
impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Hold => "Hold",
            Self::Auto => "Auto Bezier",
            Self::Bezier => "Bezier",
            Self::Mixed => "Mixed",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Active,
    End,
    Held,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Half {
    kind: Kind,
    state: State,
}
impl Half {
    fn label(self) -> String {
        match self.state {
            State::Active => self.kind.label().into(),
            State::End => "No adjacent key".into(),
            State::Held => "Held by previous key".into(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Glyph {
    incoming: Half,
    outgoing: Half,
}
impl Glyph {
    fn track(track: &AnimatedProperty, frame: u32) -> Option<Self> {
        let key = track.keys().get(&frame)?;
        let previous = track.keys().range(..frame).next_back().map(|(_, k)| k);
        let next = track
            .keys()
            .range((std::ops::Bound::Excluded(frame), std::ops::Bound::Unbounded))
            .next();
        let classify = |incoming: bool| {
            let segment = if incoming {
                previous.unwrap_or(key)
            } else {
                key
            };
            if segment.interpolation == Interpolation::Hold {
                return Kind::Hold;
            }
            match key.temporal.mode {
                TemporalMode::Auto => Kind::Auto,
                TemporalMode::Continuous => Kind::Bezier,
                TemporalMode::Independent => {
                    let handle = if incoming {
                        key.temporal.incoming
                    } else {
                        key.temporal.outgoing
                    };
                    if handle.is_some() || !matches!(segment.interpolation, Interpolation::Linear) {
                        Kind::Bezier
                    } else {
                        Kind::Linear
                    }
                }
            }
        };
        let mut incoming = Half {
            kind: classify(true),
            state: State::Active,
        };
        let mut outgoing = Half {
            kind: classify(false),
            state: State::Active,
        };
        if previous.is_none() {
            incoming = Half {
                kind: outgoing.kind,
                state: State::End,
            };
        } else if previous.is_some_and(|k| k.interpolation == Interpolation::Hold) {
            incoming.state = State::Held;
        }
        if next.is_none() {
            outgoing = Half {
                kind: incoming.kind,
                state: State::End,
            };
        }
        Some(Self { incoming, outgoing })
    }
    pub fn row(layer: &Layer, properties: &[PropertyPath], frame: u32) -> Option<(Self, String)> {
        let mut glyph: Option<Self> = None;
        let mut labels = vec![format!("Frame {frame}")];
        for &property in properties {
            let Some(g) = layer.track(property).and_then(|t| Self::track(t, frame)) else {
                continue;
            };
            labels.push(format!(
                "{}: {} in / {} out",
                layer.track_label(property).unwrap_or_default(),
                g.incoming.label(),
                g.outgoing.label()
            ));
            glyph = Some(match glyph {
                None => g,
                Some(old) => Self {
                    incoming: merge(old.incoming, g.incoming),
                    outgoing: merge(old.outgoing, g.outgoing),
                },
            });
        }
        glyph.map(|g| (g, labels.join(" · ")))
    }
    pub fn element(self, selected: bool) -> impl IntoElement {
        canvas(
            |bounds, _, _| bounds,
            move |_, bounds, window, _| {
                self.paint(bounds, selected, window);
            },
        )
        .size_full()
    }
    fn paint(self, bounds: Bounds<Pixels>, selected: bool, window: &mut Window) {
        let center = bounds.center();
        let color = if selected {
            crate::ui::BLUE
        } else {
            crate::ui::TEXT
        };
        for (half, sign) in [(self.incoming, -1.0), (self.outgoing, 1.0)] {
            let points = outline(half.kind, sign);
            let at =
                |p: (f32, f32)| -> Point<Pixels> { center + point(px(p.0 * 5.0), px(p.1 * 5.0)) };
            let mut path = PathBuilder::fill();
            let mut border = PathBuilder::stroke(px(1.0));
            path.move_to(at(points[0]));
            border.move_to(at(points[0]));
            for p in points.into_iter().skip(1) {
                path.line_to(at(p));
                border.line_to(at(p));
            }
            path.close();
            border.close();
            if let Ok(path) = path.build() {
                window.paint_path(
                    path,
                    rgb(if half.state == State::Active {
                        color
                    } else {
                        0x555555
                    }),
                );
            }
            if let Ok(border) = border.build() {
                window.paint_path(border, rgb(color));
            }
            if half.kind == Kind::Mixed {
                // A small cutout distinguishes a combined row from a linear key.
                window.paint_quad(gpui::fill(
                    Bounds::new(
                        center + point(px(sign * 2.0 - 0.7), px(-0.7)),
                        gpui::size(px(1.4), px(1.4)),
                    ),
                    rgb(crate::ui::BG),
                ));
            }
        }
    }
}
fn merge(a: Half, b: Half) -> Half {
    if a == b {
        a
    } else {
        Half {
            kind: Kind::Mixed,
            state: State::Active,
        }
    }
}
fn outline(kind: Kind, sign: f32) -> Vec<(f32, f32)> {
    let points = match kind {
        Kind::Linear | Kind::Mixed => vec![(0.0, -1.0), (1.0, 0.0), (0.0, 1.0)],
        Kind::Hold => vec![(0.0, -1.0), (1.0, -1.0), (1.0, 1.0), (0.0, 1.0)],
        Kind::Bezier => vec![(0.0, -1.0), (1.0, -1.0), (0.4, 0.0), (1.0, 1.0), (0.0, 1.0)],
        Kind::Auto => (0..=12)
            .map(|i| {
                let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 12.0;
                (angle.cos(), angle.sin())
            })
            .collect(),
    };
    points.into_iter().map(|(x, y)| (x * sign, y)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{
        Command, Editor, KeyRef, Project, Property, TemporalHandle, TrackEdit,
    };
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        for property in [Property::PositionX, Property::PositionY] {
            for frame in [10, 30, 60] {
                e.execute(Command::EditTrack {
                    id: 1,
                    property: property.into(),
                    edit: TrackEdit::ToggleKey { frame },
                })
                .unwrap();
            }
        }
        e
    }
    fn glyph(e: &Editor, frame: u32) -> Glyph {
        Glyph::track(
            e.selected_layer()
                .unwrap()
                .property(Property::PositionX)
                .expect("2D test layer has independent Position tracks"),
            frame,
        )
        .unwrap()
    }
    fn interpolate(e: &mut Editor, frame: u32, interpolation: Interpolation) {
        e.execute(Command::EditTrack {
            id: 1,
            property: Property::PositionX.into(),
            edit: TrackEdit::Interpolate {
                frame,
                interpolation,
            },
        })
        .unwrap();
    }
    #[test]
    fn endpoints_and_hold_suppression_are_visible_without_inventing_adjacent_segments() {
        let mut e = scene();
        assert_eq!(glyph(&e, 10).incoming.state, State::End);
        assert_eq!(glyph(&e, 60).outgoing.state, State::End);
        assert_eq!(
            glyph(&e, 30),
            Glyph {
                incoming: Half {
                    kind: Kind::Linear,
                    state: State::Active
                },
                outgoing: Half {
                    kind: Kind::Linear,
                    state: State::Active
                }
            }
        );
        interpolate(&mut e, 30, Interpolation::Hold);
        assert_eq!(glyph(&e, 30).incoming.kind, Kind::Linear);
        assert_eq!(glyph(&e, 30).outgoing.kind, Kind::Hold);
        assert_eq!(
            glyph(&e, 60).incoming,
            Half {
                kind: Kind::Hold,
                state: State::Held
            }
        );
        interpolate(&mut e, 10, Interpolation::Hold);
        assert_eq!(glyph(&e, 30).incoming.state, State::Held);
        assert!(glyph(&e, 60).incoming.label().contains("previous key"));
    }
    #[test]
    fn directional_handles_and_legacy_curves_classify_each_half_separately() {
        let mut e = scene();
        e.execute(Command::SetTemporalHandle {
            id: 1,
            property: Property::PositionX.into(),
            frame: 30,
            incoming: true,
            handle: TemporalHandle {
                slope: 0.0,
                influence: 1.0 / 3.0,
            },
        })
        .unwrap();
        assert_eq!(
            (glyph(&e, 30).incoming.kind, glyph(&e, 30).outgoing.kind),
            (Kind::Bezier, Kind::Linear)
        );
        e.undo();
        interpolate(&mut e, 30, Interpolation::Smooth);
        assert_eq!(
            (glyph(&e, 30).incoming.kind, glyph(&e, 30).outgoing.kind),
            (Kind::Linear, Kind::Bezier)
        );
        assert_eq!(glyph(&e, 60).incoming.kind, Kind::Bezier);
        interpolate(&mut e, 10, Interpolation::Bezier(Default::default()));
        assert_eq!(glyph(&e, 30).incoming.kind, Kind::Bezier);
    }
    #[test]
    fn modes_combined_channels_and_missing_axis_keys_report_actual_stored_state() {
        let mut e = scene();
        for (mode, kind) in [
            (TemporalMode::Auto, Kind::Auto),
            (TemporalMode::Continuous, Kind::Bezier),
        ] {
            e.execute(Command::SetTemporalMode {
                id: 1,
                property: Property::PositionX.into(),
                frame: 30,
                mode,
            })
            .unwrap();
            let g = glyph(&e, 30);
            assert_eq!((g.incoming.kind, g.outgoing.kind), (kind, kind));
            let properties = [Property::PositionX.into(), Property::PositionY.into()];
            let (mixed, tip) = Glyph::row(e.selected_layer().unwrap(), &properties, 30).unwrap();
            assert_eq!(mixed.incoming.kind, Kind::Mixed);
            assert!(
                tip.contains("Position X")
                    && tip.contains("Position Y")
                    && tip.contains(kind.label())
            );
        }
        e.execute(Command::DeleteKeys(vec![KeyRef {
            id: 1,
            property: Property::PositionY.into(),
            frame: 30,
        }]))
        .unwrap();
        let (single, tip) = Glyph::row(
            e.selected_layer().unwrap(),
            &[Property::PositionX.into(), Property::PositionY.into()],
            30,
        )
        .unwrap();
        assert_eq!(single, glyph(&e, 30));
        assert!(!tip.contains("Position Y"));
    }
    #[test]
    fn key_display_is_read_only_and_tracks_undo_redo_and_saved_state() {
        let mut e = scene();
        let before = e.project().clone();
        let original = glyph(&e, 30);
        interpolate(&mut e, 30, Interpolation::Hold);
        let after = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let changed = glyph(&e, 30);
        assert_ne!(original, changed);
        assert_eq!(
            Glyph::track(
                after
                    .composition()
                    .layer(1)
                    .unwrap()
                    .property(Property::PositionX)
                    .expect("2D test layer has independent Position tracks"),
                30
            ),
            Some(changed)
        );
        e.undo();
        assert_eq!(glyph(&e, 30), original);
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(glyph(&e, 30), changed);
        assert_eq!(e.project(), &after);
        let r = crate::rendering::Renderer::new();
        for frame in [10, 20, 30, 45, 60] {
            assert_eq!(
                r.render(&after, frame, 384).unwrap(),
                r.render_output(&after, frame, 384, 216).unwrap()
            );
        }
    }
    #[test]
    fn half_outlines_are_finite_bounded_and_mirrored() {
        for kind in [
            Kind::Linear,
            Kind::Hold,
            Kind::Auto,
            Kind::Bezier,
            Kind::Mixed,
        ] {
            let left = outline(kind, -1.0);
            let right = outline(kind, 1.0);
            for ((x, y), (rx, ry)) in left.into_iter().zip(right) {
                assert!(
                    x.is_finite() && y.is_finite() && x.abs() <= 1.000001 && y.abs() <= 1.000001
                );
                assert_eq!((-x, y), (rx, ry));
            }
        }
    }
}
