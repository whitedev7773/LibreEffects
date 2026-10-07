use super::EffectParam as P;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveChannel {
    Rgb,
    Red,
    Green,
    Blue,
}
impl CurveChannel {
    pub const ALL: [Self; 4] = [Self::Rgb, Self::Red, Self::Green, Self::Blue];
    pub fn label(self) -> &'static str {
        match self {
            Self::Rgb => "RGB",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
        }
    }
    pub fn point_labels(self) -> [&'static str; 5] {
        match self {
            Self::Rgb => [
                "RGB · 0%",
                "RGB · 25%",
                "RGB · 50%",
                "RGB · 75%",
                "RGB · 100%",
            ],
            Self::Red => [
                "Red · 0%",
                "Red · 25%",
                "Red · 50%",
                "Red · 75%",
                "Red · 100%",
            ],
            Self::Green => [
                "Green · 0%",
                "Green · 25%",
                "Green · 50%",
                "Green · 75%",
                "Green · 100%",
            ],
            Self::Blue => [
                "Blue · 0%",
                "Blue · 25%",
                "Blue · 50%",
                "Blue · 75%",
                "Blue · 100%",
            ],
        }
    }
    pub fn parameters(self) -> [P; 5] {
        match self {
            Self::Rgb => [P::Curve0, P::Curve25, P::Curve50, P::Curve75, P::Curve100],
            Self::Red => [
                P::RedCurve0,
                P::RedCurve25,
                P::RedCurve50,
                P::RedCurve75,
                P::RedCurve100,
            ],
            Self::Green => [
                P::GreenCurve0,
                P::GreenCurve25,
                P::GreenCurve50,
                P::GreenCurve75,
                P::GreenCurve100,
            ],
            Self::Blue => [
                P::BlueCurve0,
                P::BlueCurve25,
                P::BlueCurve50,
                P::BlueCurve75,
                P::BlueCurve100,
            ],
        }
    }
}

/// Shape-preserving cubic interpolation at five equally spaced input levels.
/// Inputs/outputs are normalized to [0,1]. Extrema and reversed curves are valid;
/// tangents cannot overshoot the neighboring control values.
pub fn sample_color_curve(values: [f64; 5], input: f64) -> f64 {
    let d: [f64; 4] = std::array::from_fn(|i| (values[i + 1] - values[i]) * 4.0);
    let endpoint = |a: f64, b: f64| {
        let slope = (3.0 * a - b) * 0.5;
        if slope * a <= 0.0 {
            0.0
        } else if a * b < 0.0 && slope.abs() > 3.0 * a.abs() {
            3.0 * a
        } else {
            slope
        }
    };
    let mut m = [0.0; 5];
    m[0] = endpoint(d[0], d[1]);
    m[4] = endpoint(d[3], d[2]);
    for i in 1..4 {
        m[i] = if d[i - 1] * d[i] <= 0.0 {
            0.0
        } else {
            2.0 * d[i - 1] * d[i] / (d[i - 1] + d[i])
        };
    }
    let x = input.clamp(0.0, 1.0) * 4.0;
    let i = (x.floor() as usize).min(3);
    let t = x - i as f64;
    let t2 = t * t;
    let t3 = t2 * t;
    ((2.0 * t3 - 3.0 * t2 + 1.0) * values[i]
        + (t3 - 2.0 * t2 + t) * m[i] * 0.25
        + (-2.0 * t3 + 3.0 * t2) * values[i + 1]
        + (t3 - t2) * m[i + 1] * 0.25)
        .clamp(values[i].min(values[i + 1]), values[i].max(values[i + 1]))
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    #[test]
    fn curves_interpolate_knots_identity_inverse_and_extrema_without_overshoot() {
        for values in [
            [0.0, 0.25, 0.5, 0.75, 1.0],
            [1.0, 0.75, 0.5, 0.25, 0.0],
            [0.1, 0.8, 0.2, 0.9, 0.3],
            [0.0, 0.0, 0.5, 1.0, 1.0],
        ] {
            for i in 0..5 {
                assert!((sample_color_curve(values, i as f64 / 4.0) - values[i]).abs() < 1e-12);
            }
            for n in 0..1024 {
                let x = n as f64 / 1024.0;
                let i = (x * 4.0).floor() as usize;
                let y = sample_color_curve(values, x);
                assert!(y >= values[i].min(values[i + 1]) && y <= values[i].max(values[i + 1]));
            }
        }
        for i in 0..=1024 {
            let x = i as f64 / 1024.0;
            assert!((sample_color_curve([0.0, 0.25, 0.5, 0.75, 1.0], x) - x).abs() < 1e-12);
            assert!((sample_color_curve([1.0, 0.75, 0.5, 0.25, 0.0], x) - (1.0 - x)).abs() < 1e-12);
        }
    }
    #[test]
    fn tonal_effects_roundtrip_keyframes_duplicate_reset_and_history() {
        let mut e = Editor::default();
        e.execute(Command::AddSolid).unwrap();
        for kind in [
            EffectKind::Curves,
            EffectKind::LinearGradient,
            EffectKind::RadialGradient,
        ] {
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(kind),
            })
            .unwrap();
        }
        let value = |effect, parameter, frame, value| Command::Effect {
            id: 1,
            edit: EffectEdit::SetValue {
                effect,
                parameter,
                frame,
                value,
            },
        };
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::ToggleAnimation {
                effect: 1,
                parameter: P::RedCurve50,
                frame: 0,
            },
        })
        .unwrap();
        e.execute(value(1, P::RedCurve50, 30, 220.0)).unwrap();
        e.execute(value(2, P::EndX, 0, 600.0)).unwrap();
        e.execute(value(3, P::DarkBlue, 0, 170.0)).unwrap();
        let saved = e.project().clone();
        assert_eq!(saved.version, 19);
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        let fx = &saved.composition().layer(1).unwrap().effect_stack()[0];
        assert!((fx.value_at(P::RedCurve50, 15) - 173.75).abs() < 1e-9);
        let mut old = saved.clone();
        old.version = 18;
        assert!(Project::from_json(&serde_json::to_string(&old).unwrap()).is_err());
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Duplicate(1),
        })
        .unwrap();
        assert_eq!(
            e.selected_layer().unwrap().effect_stack()[1].parameter(P::RedCurve50),
            fx.parameter(P::RedCurve50)
        );
        e.undo();
        assert_eq!(e.project(), &saved);
        e.redo();
        assert_eq!(e.selected_layer().unwrap().effect_stack().len(), 4);
        e.undo();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Reset(1),
        })
        .unwrap();
        assert_eq!(
            e.selected_layer().unwrap().effect_stack()[0].value_at(P::RedCurve50, 15),
            127.5
        );
        assert!(
            e.selected_layer().unwrap().effect_stack()[0]
                .parameter(P::RedCurve50)
                .unwrap()
                .keys()
                .is_empty()
        );
        e.undo();
        assert_eq!(e.project(), &saved);
        for bad in [f64::NAN, -1.0, 256.0] {
            assert!(e.execute(value(1, P::Curve50, 0, bad)).is_err());
            assert_eq!(e.project(), &saved);
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(e.execute(value(2, P::StartY, 0, 10.0)).is_err());
    }
    #[test]
    fn gradient_initial_and_reset_endpoints_follow_layer_size() {
        let mut e = Editor::default();
        e.execute(Command::AddSolid).unwrap();
        e.execute(Command::ConfigureSolid {
            id: 1,
            width: 320,
            height: 180,
            color: 0xffffff,
        })
        .unwrap();
        for kind in [EffectKind::LinearGradient, EffectKind::RadialGradient] {
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(kind),
            })
            .unwrap();
        }
        let check = |e: &Editor, w: f64, h: f64| {
            let effects = e.selected_layer().unwrap().effect_stack();
            assert_eq!(effects[0].value_at(P::EndY, 0), h);
            assert_eq!(effects[1].value_at(P::StartX, 0), w / 2.0);
            assert_eq!(effects[1].value_at(P::StartY, 0), h / 2.0);
            assert_eq!(effects[1].value_at(P::EndY, 0), h);
        };
        check(&e, 320.0, 180.0);
        e.execute(Command::ConfigureSolid {
            id: 1,
            width: 640,
            height: 360,
            color: 0xffffff,
        })
        .unwrap();
        check(&e, 320.0, 180.0);
        for effect in [1, 2] {
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Reset(effect),
            })
            .unwrap();
        }
        check(&e, 640.0, 360.0);
        let clip = e.copy_layers(&[1]).unwrap();
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::PasteLayers(clip)).unwrap();
        check(&e, 640.0, 360.0);
    }
}
