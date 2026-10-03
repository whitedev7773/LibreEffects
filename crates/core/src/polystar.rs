//! Parametric polygon/star geometry, with a partial tip for fractional stars.
use crate::ShapeKind;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

pub(super) fn vertices(
    kind: ShapeKind,
    points: f64,
    inner_percent: f64,
    width: f64,
    height: f64,
) -> Vec<[f64; 2]> {
    let points = points.clamp(3., 128.);
    let star = kind == ShapeKind::Star;
    let fraction = points.fract();
    let inner = inner_percent / 100.;
    let point = |angle: f64, radius: f64| {
        [
            width / 2. + angle.cos() * width / 2. * radius,
            height / 2. + angle.sin() * height / 2. * radius,
        ]
    };
    if !star || fraction == 0. {
        // Keep the original integer geometry (and stroke starting point) intact.
        let count = points.floor() as u32 * if star { 2 } else { 1 };
        return (0..count)
            .map(|i| {
                point(
                    TAU * i as f64 / count as f64 - FRAC_PI_2,
                    if star && i % 2 == 1 { inner } else { 1. },
                )
            })
            .collect();
    }
    // The seam's partial tip grows from the inner to the outer radius. All
    // complete tips/valleys lie on a pi/points angular lattice; closure supplies
    // the other fractional half-step. This converges to each integer outline.
    let step = PI / points;
    let mut result = Vec::with_capacity(2 * points.ceil() as usize);
    result.push(point(
        -FRAC_PI_2 + step * (1. - fraction),
        inner + fraction * (1. - inner),
    ));
    result.extend((1..2 * points.ceil() as u32).map(|i| {
        point(
            -FRAC_PI_2 + step * i as f64,
            if i % 2 == 1 { inner } else { 1. },
        )
    }));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn integer_outlines_preserve_legacy_geometry_and_fractional_stars_are_bounded() {
        for kind in [ShapeKind::Polygon, ShapeKind::Star] {
            for n in 3..=128 {
                let v = vertices(kind, n as f64, 50., 200., 120.);
                let count = n * if kind == ShapeKind::Star { 2 } else { 1 };
                assert_eq!(v.len(), count);
                for (i, p) in v.iter().enumerate() {
                    let angle = TAU * i as f64 / count as f64 - FRAC_PI_2;
                    let radius = if kind == ShapeKind::Star && i % 2 == 1 {
                        0.5
                    } else {
                        1.
                    };
                    assert_eq!(
                        *p,
                        [
                            100. + angle.cos() * 100. * radius,
                            60. + angle.sin() * 60. * radius
                        ]
                    );
                }
                if n < 128 {
                    assert_eq!(
                        vertices(ShapeKind::Polygon, n as f64 + 0.99, 50., 200., 120.),
                        vertices(ShapeKind::Polygon, n as f64, 50., 200., 120.)
                    );
                }
            }
        }
        let p = vertices(ShapeKind::Star, 3.5, 50., 200., 120.);
        assert_eq!(p.len(), 8);
        let x = (p[0][0] - 100.) / 100.;
        let y = (p[0][1] - 60.) / 60.;
        assert!((x.hypot(y) - 0.75).abs() < 1e-12);
        assert!((y.atan2(x) + 5. * PI / 14.).abs() < 1e-12);
        for count in [3.000001, 3.5, 5.25, 8.75, 127.999999] {
            for inner in [0., 50., 100.] {
                for [x, y] in vertices(ShapeKind::Star, count, inner, 200., 120.) {
                    assert!(x.is_finite() && y.is_finite());
                    assert!(((x - 100.) / 100.).hypot((y - 60.) / 60.) <= 1. + 1e-12);
                }
            }
        }
        // A fractional tip must converge from either side without an area jump.
        let area = |v: Vec<[f64; 2]>| {
            (0..v.len())
                .map(|i| {
                    let a = v[i];
                    let b = v[(i + 1) % v.len()];
                    a[0] * b[1] - a[1] * b[0]
                })
                .sum::<f64>()
                .abs()
                / 2.
        };
        for n in 4..128 {
            let exact = area(vertices(ShapeKind::Star, n as f64, 50., 200., 120.));
            for eps in [-1e-8, 1e-8] {
                assert!(
                    (area(vertices(ShapeKind::Star, n as f64 + eps, 50., 200., 120.)) - exact)
                        .abs()
                        < 0.001
                );
            }
        }
    }

    fn scene(kind: ShapeKind) -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                kind,
                ..Default::default()
            }),
            width: 200.,
            height: 120.,
            name: "Points".into(),
        })
        .unwrap();
        e
    }
    fn edit(e: &mut Editor, edit: TrackEdit) {
        e.execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::Shape(ShapeParam::Points),
            edit,
        })
        .unwrap();
    }
    #[test]
    fn point_tracks_animate_fractional_geometry_and_share_key_history() {
        for kind in [ShapeKind::Polygon, ShapeKind::Star] {
            let mut e = scene(kind);
            let old = e.project().clone();
            edit(
                &mut e,
                TrackEdit::Value {
                    frame: 0,
                    value: 5.25,
                },
            );
            edit(&mut e, TrackEdit::ToggleAnimation { frame: 0 });
            let enabled = e.project().clone();
            edit(
                &mut e,
                TrackEdit::Value {
                    frame: 60,
                    value: 8.75,
                },
            );
            let saved = e.project().clone();
            e.undo();
            assert_eq!(e.project(), &enabled);
            e.redo();
            assert_eq!(e.project(), &saved);
            let path = PropertyPath::Shape(ShapeParam::Points);
            assert_eq!(e.selected_layer().unwrap().track_value(path, 30), Some(7.));
            let Content::Shape(shape) = e.selected_layer().unwrap().content() else {
                unreachable!()
            };
            assert_eq!(shape.points, 5);
            assert_ne!(
                shape.svg_at(200., 120., 0xffffff, 0),
                shape.svg_at(200., 120., 0xffffff, 60)
            );
            let json = saved.to_json().unwrap();
            assert_eq!(Project::from_json(&json).unwrap(), saved);
            let mut downgraded: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(downgraded["version"], 42);
            downgraded["version"] = 41.into();
            assert!(Project::from_json(&downgraded.to_string()).is_err());
            assert_eq!(Project::from_json(&old.to_json().unwrap()).unwrap(), old);
            let key = e.selected_layer().unwrap().copy_key(path, 60).unwrap();
            e.execute(Command::PasteKeys {
                keys: vec![key],
                frame: 100,
                target: None,
            })
            .unwrap();
            e.execute(Command::MoveKeys {
                keys: vec![KeyRef {
                    id: 1,
                    property: path,
                    frame: 60,
                }],
                delta: 20,
            })
            .unwrap();
            assert_eq!(e.selected_layer().unwrap().track_value(path, 40), Some(7.));
            e.undo();
            e.undo();
            assert_eq!(e.project(), &saved);
            edit(
                &mut e,
                TrackEdit::Interpolate {
                    frame: 0,
                    interpolation: Interpolation::Hold,
                },
            );
            assert_eq!(
                e.selected_layer().unwrap().track_value(path, 30),
                Some(5.25)
            );
            e.undo();
            edit(&mut e, TrackEdit::ToggleAnimation { frame: 15 });
            assert_eq!(
                e.selected_layer().unwrap().track_value(path, 100),
                Some(6.125)
            );
            e.undo();
            assert_eq!(e.project(), &saved);
        }
    }

    #[test]
    fn invalid_or_unavailable_point_edits_and_pastes_are_atomic() {
        let mut e = scene(ShapeKind::Star);
        let initial = e.project().clone();
        for value in [2.99, 128.01, f64::NAN, f64::INFINITY] {
            assert!(
                e.execute(Command::EditShape {
                    id: 1,
                    parameter: ShapeParam::Points,
                    edit: TrackEdit::Value { frame: 0, value }
                })
                .is_err()
            );
            assert_eq!(e.project(), &initial);
        }
        edit(&mut e, TrackEdit::ToggleAnimation { frame: 0 });
        let key = e
            .selected_layer()
            .unwrap()
            .copy_key(PropertyPath::Shape(ShapeParam::Points), 0)
            .unwrap();
        for kind in [
            ShapeKind::Rectangle,
            ShapeKind::RoundedRectangle,
            ShapeKind::Ellipse,
        ] {
            let mut other = scene(kind);
            let before = other.project().clone();
            assert!(
                other
                    .execute(Command::PasteKeys {
                        keys: vec![key.clone()],
                        frame: 0,
                        target: Some(1)
                    })
                    .is_err()
            );
            assert_eq!(other.project(), &before);
        }
        let saved = e.project().clone();
        let Content::Shape(mut shape) = e.selected_layer().unwrap().content().clone() else {
            unreachable!()
        };
        shape.path = Some(VectorPath {
            closed: true,
            vertices: vec![
                PathVertex::corner([0., 0.]),
                PathVertex::corner([100., 0.]),
                PathVertex::corner([0., 100.]),
            ],
        });
        assert!(
            e.execute(Command::SetContent {
                id: 1,
                content: Content::Shape(shape)
            })
            .is_err()
        );
        assert_eq!(e.project(), &saved);
        e.execute(Command::ToggleLocked(1)).unwrap();
        let locked = e.project().clone();
        assert!(
            e.execute(Command::EditShape {
                id: 1,
                parameter: ShapeParam::Points,
                edit: TrackEdit::Value {
                    frame: 20,
                    value: 6.5
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &locked);
    }
}
