//! Independent temporal tangents; legacy segment curves remain unchanged until edited.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemporalHandle {
    /// Signed property units per frame. UI converts this to units per second.
    pub slope: f64,
    /// Fraction of the adjacent segment's duration (0.1–100 percent).
    pub influence: f64,
}
impl TemporalHandle {
    pub fn valid(self) -> bool {
        self.slope.is_finite()
            && self.slope.abs() <= 1e9
            && self.influence.is_finite()
            && (0.001..=1.0).contains(&self.influence)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TemporalHandles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incoming: Option<TemporalHandle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outgoing: Option<TemporalHandle>,
}
impl TemporalHandles {
    pub fn is_empty(&self) -> bool {
        self.incoming.is_none() && self.outgoing.is_none()
    }
    pub(crate) fn valid(self) -> bool {
        self.incoming.is_none_or(TemporalHandle::valid)
            && self.outgoing.is_none_or(TemporalHandle::valid)
    }
    pub(crate) fn rescale(&mut self, ratio: f64) {
        for h in [&mut self.incoming, &mut self.outgoing]
            .into_iter()
            .flatten()
        {
            h.slope *= ratio;
        }
    }
}
fn controls(a: &Keyframe, b: &Keyframe, span: f64) -> (f64, f64, f64, f64) {
    let curve = match a.interpolation {
        Interpolation::Bezier(c) => c,
        Interpolation::Smooth => Bezier::default(),
        _ => Bezier {
            x1: 1.0 / 3.0,
            y1: 1.0 / 3.0,
            x2: 2.0 / 3.0,
            y2: 2.0 / 3.0,
        },
    };
    let delta = b.value - a.value;
    let (mut x1, mut y1, mut x2, mut y2) = (
        curve.x1,
        a.value + delta * curve.y1,
        curve.x2,
        a.value + delta * curve.y2,
    );
    if let Some(h) = a.temporal.outgoing {
        x1 = h.influence;
        y1 = a.value + h.slope * span * x1;
    }
    if let Some(h) = b.temporal.incoming {
        x2 = 1.0 - h.influence;
        y2 = b.value - h.slope * span * h.influence;
    }
    (x1, y1, x2, y2)
}
pub(super) fn sample(a: &Keyframe, b: &Keyframe, span: f64, time: f64) -> f64 {
    let (x1, y1, x2, y2) = controls(a, b, span);
    let cubic = |a: f64, b: f64, c: f64, d: f64, t: f64| {
        let u = 1.0 - t;
        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..48 {
        let t = (lo + hi) * 0.5;
        if cubic(0.0, x1, x2, 1.0, t) < time {
            lo = t;
        } else {
            hi = t;
        }
    }
    cubic(a.value, y1, y2, b.value, (lo + hi) * 0.5)
}
impl AnimatedProperty {
    /// Signed, unclamped scalar velocity in property units per frame. At an exact
    /// key, `incoming` selects the left-hand limit; otherwise the right-hand limit.
    /// None denotes a Hold jump or a vertical tangent, never an arbitrary finite spike.
    pub fn velocity(&self, frame: f64, incoming: bool) -> Option<f64> {
        if !frame.is_finite() {
            return None;
        }
        if frame < 0.0 {
            return Some(0.0);
        }
        let exact = frame.fract() == 0.0;
        let f = frame.floor().min(u32::MAX as f64) as Frame;
        let left = if exact && incoming {
            self.keys.range(..f).next_back()
        } else {
            self.keys.range(..=f).next_back()
        };
        let Some((&start, a)) = left else {
            return Some(0.0);
        };
        let Some((&end, b)) = self
            .keys
            .range((std::ops::Bound::Excluded(start), std::ops::Bound::Unbounded))
            .next()
        else {
            return Some(0.0);
        };
        if frame > end as f64 {
            return Some(0.0);
        }
        let span = (end - start) as f64;
        let time = ((frame - start as f64) / span).clamp(0.0, 1.0);
        if a.interpolation == Interpolation::Hold {
            return if time == 1.0 && a.value != b.value {
                None
            } else {
                Some(0.0)
            };
        }
        if a.temporal.outgoing.is_none() && b.temporal.incoming.is_none() {
            match a.interpolation {
                Interpolation::Linear => return Some((b.value - a.value) / span),
                Interpolation::Smooth => {
                    return Some((b.value - a.value) / span * 6.0 * time * (1.0 - time));
                }
                _ => {}
            }
        }
        let (x1, y1, x2, y2) = controls(a, b, span);
        let cubic_x = |t: f64| {
            let u = 1.0 - t;
            3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t
        };
        let t = if time == 0.5 && x1 == 1.0 && x2 == 0.0 {
            0.5
        } else if time == 0.0 || time == 1.0 {
            time
        } else {
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..48 {
                let t = (lo + hi) * 0.5;
                if cubic_x(t) < time {
                    lo = t;
                } else {
                    hi = t;
                }
            }
            (lo + hi) * 0.5
        };
        let derivatives = |a: f64, b: f64, c: f64, d: f64| {
            let u = 1.0 - t;
            [
                3.0 * (u * u * (b - a) + 2.0 * u * t * (c - b) + t * t * (d - c)),
                6.0 * (u * (c - 2.0 * b + a) + t * (d - 2.0 * c + b)),
                6.0 * (d - 3.0 * c + 3.0 * b - a),
            ]
        };
        let dx = derivatives(0.0, x1, x2, 1.0);
        let dy = derivatives(a.value, y1, y2, b.value);
        for (x, y) in dx.into_iter().zip(dy) {
            if x.abs() > 1e-12 {
                let v = y / x / span;
                return v.is_finite().then_some(v);
            }
            if y.abs() > 1e-10 {
                return None;
            }
        }
        Some(0.0)
    }
    /// Existing finite tangent, or a derived legacy tangent. Vertical legacy handles return None.
    pub fn temporal_handle(&self, frame: Frame, incoming: bool) -> Option<TemporalHandle> {
        let key = self.keys.get(&frame)?;
        if let Some(h) = if incoming {
            key.temporal.incoming
        } else {
            key.temporal.outgoing
        } {
            return Some(h);
        }
        let ((start, a), (end, b)) = if incoming {
            (self.keys.range(..frame).next_back()?, (&frame, key))
        } else {
            ((&frame, key), self.keys.range(frame + 1..).next()?)
        };
        if a.interpolation == Interpolation::Hold {
            return None;
        }
        let span = (end - start) as f64;
        let (x1, y1, x2, y2) = controls(a, b, span);
        let influence = if incoming { 1.0 - x2 } else { x1 };
        if influence < 0.001 {
            return None;
        }
        Some(TemporalHandle {
            influence,
            slope: if incoming {
                (b.value - y2) / (span * influence)
            } else {
                (y1 - a.value) / (span * influence)
            },
        })
    }
    pub(super) fn set_interpolation(
        &mut self,
        frame: Frame,
        interpolation: Interpolation,
    ) -> Result<(), String> {
        if !interpolation.valid() {
            return Err("Invalid interpolation".into());
        }
        let key = self.keys.get_mut(&frame).ok_or("Select a keyframe first")?;
        key.interpolation = interpolation;
        key.temporal.outgoing = None;
        if let Some((_, next)) = self.keys.range_mut(frame + 1..).next() {
            next.temporal.incoming = None;
        }
        Ok(())
    }
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::SetTemporalHandle {
        id,
        property,
        frame,
        incoming,
        handle,
    } = *command
    else {
        return None;
    };
    Some((|| {
        if !handle.valid() || matches!(property, PropertyPath::Path(_)) {
            return Err("Invalid scalar temporal handle".into());
        }
        let layer = state
            .project
            .composition
            .layers
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or("Layer not found")?;
        if layer.locked {
            return Err("Unlock the layer before editing".into());
        }
        let track = layer.track_mut(property)?;
        if !track.keys.contains_key(&frame) {
            return Err("Select a keyframe first".into());
        }
        let segment = if incoming {
            *track
                .keys
                .range(..frame)
                .next_back()
                .ok_or("No incoming segment")?
                .0
        } else {
            track
                .keys
                .range(frame + 1..)
                .next()
                .ok_or("No outgoing segment")?;
            frame
        };
        if track.keys[&segment].interpolation == Interpolation::Hold {
            track.keys.get_mut(&segment).unwrap().interpolation = Interpolation::Linear;
        }
        let key = track.keys.get_mut(&frame).unwrap();
        if incoming {
            key.temporal.incoming = Some(handle);
        } else {
            key.temporal.outgoing = Some(handle);
        }
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn velocity_matches_fractional_samples_and_integrates_to_value_change() {
        let p = Property::PositionX.into();
        let mut e = scene(p);
        for mode in [
            Interpolation::Linear,
            Interpolation::Smooth,
            Interpolation::Bezier(Bezier {
                x1: 0.15,
                y1: -0.3,
                x2: 0.8,
                y2: 1.4,
            }),
        ] {
            e.execute(Command::SetInterpolation {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                interpolation: mode,
            })
            .unwrap();
            let t = track(&e, p);
            for f in [0.1, 2.0, 8.5, 15.0, 24.0, 29.9] {
                let finite = (t.sample(f + 1e-4) - t.sample(f - 1e-4)) / 2e-4;
                assert!(
                    (t.velocity(f, false).unwrap() - finite).abs() < 1e-5,
                    "{mode:?} at {f}"
                );
            }
            let area = (0..10000)
                .map(|i| {
                    t.velocity((i as f64 + 0.5) * 30.0 / 10000.0, false)
                        .unwrap()
                })
                .sum::<f64>()
                * 30.0
                / 10000.0;
            assert!((area - 30.0).abs() < 1e-4);
        }
        set(&mut e, p, 0, false, 4.0, 0.5).unwrap();
        set(&mut e, p, 30, true, -2.0, 0.3).unwrap();
        assert!((track(&e, p).velocity(0.0, false).unwrap() - 4.0).abs() < 1e-12);
        assert!((track(&e, p).velocity(30.0, true).unwrap() + 2.0).abs() < 1e-12);
        assert_eq!(track(&e, p).velocity(30.0, false), Some(-1.0));
        let t = track(&e, p);
        let finite = (t.sample(12.5001) - t.sample(12.4999)) / 0.0002;
        assert!((t.velocity(12.5, false).unwrap() - finite).abs() < 1e-5);
    }
    #[test]
    fn velocity_limits_distinguish_jumps_vertical_handles_and_removable_singularities() {
        let p = Property::PositionX.into();
        let mut e = scene(p);
        e.execute(Command::SetInterpolation {
            id: 1,
            property: Property::PositionX,
            frame: 0,
            interpolation: Interpolation::Hold,
        })
        .unwrap();
        assert_eq!(track(&e, p).velocity(29.9, false), Some(0.0));
        assert_eq!(track(&e, p).velocity(30.0, true), None);
        assert_eq!(track(&e, p).velocity(30.0, false), Some(-1.0));
        assert_eq!(track(&e, p).velocity(100.0, false), Some(0.0));
        assert_eq!(track(&e, p).velocity(f64::NAN, false), None);
        for (curve, frame, want) in [
            (
                Bezier {
                    x1: 0.0,
                    y1: 0.5,
                    x2: 0.8,
                    y2: 0.7,
                },
                0.0,
                None,
            ),
            (
                Bezier {
                    x1: 0.0,
                    y1: 0.0,
                    x2: 1.0,
                    y2: 1.0,
                },
                0.0,
                Some(1.0),
            ),
            (
                Bezier {
                    x1: 0.0,
                    y1: 0.0,
                    x2: 1.0,
                    y2: 1.0,
                },
                30.0,
                Some(1.0),
            ),
            (
                Bezier {
                    x1: 1.0,
                    y1: 0.0,
                    x2: 0.0,
                    y2: 1.0,
                },
                15.0,
                None,
            ),
            (
                Bezier {
                    x1: 1.0,
                    y1: 1.0,
                    x2: 0.0,
                    y2: 0.0,
                },
                15.0,
                Some(1.0),
            ),
        ] {
            e.execute(Command::SetInterpolation {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                interpolation: Interpolation::Bezier(curve),
            })
            .unwrap();
            assert_eq!(
                track(&e, p).velocity(frame, frame == 30.0),
                want,
                "{curve:?}"
            );
        }
    }
    fn scene(path: PropertyPath) -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::GaussianBlur),
        })
        .unwrap();
        for (frame, value) in [(0, 10.0), (30, 40.0), (60, 10.0)] {
            e.execute(Command::EditTrack {
                id: 1,
                property: path,
                edit: TrackEdit::ToggleKey { frame },
            })
            .unwrap();
            e.execute(Command::EditTrack {
                id: 1,
                property: path,
                edit: TrackEdit::Value { frame, value },
            })
            .unwrap();
        }
        e
    }
    fn track(e: &Editor, path: PropertyPath) -> &AnimatedProperty {
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .track(path)
            .unwrap()
    }
    fn set(
        e: &mut Editor,
        path: PropertyPath,
        frame: Frame,
        incoming: bool,
        slope: f64,
        influence: f64,
    ) -> Result<(), String> {
        e.execute(Command::SetTemporalHandle {
            id: 1,
            property: path,
            frame,
            incoming,
            handle: TemporalHandle { slope, influence },
        })
    }
    #[test]
    fn incoming_and_outgoing_change_only_their_own_segment_and_roundtrip() {
        let p = Property::PositionX.into();
        let mut e = scene(p);
        let before = e.project().clone();
        set(&mut e, p, 30, true, 0.0, 0.6).unwrap();
        assert!((track(&e, p).sample(15.0) - 25.0).abs() > 1.0);
        assert_eq!(track(&e, p).sample(45.0), 25.0);
        let left = track(&e, p).sample(15.0);
        set(&mut e, p, 30, false, -4.0, 0.5).unwrap();
        assert_eq!(track(&e, p).sample(15.0), left);
        assert!((track(&e, p).sample(45.0) - 25.0).abs() > 1.0);
        let after = e.project().clone();
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
        e.undo();
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        e.redo();
        assert_eq!(e.project(), &after);
    }
    #[test]
    fn flat_segments_support_overshoot_and_derivatives_use_property_units_per_frame() {
        let p = Property::PositionX.into();
        let mut e = scene(p);
        e.execute(Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame: 30,
            value: 10.0,
        })
        .unwrap();
        set(&mut e, p, 0, false, 4.0, 0.5).unwrap();
        set(&mut e, p, 30, true, -2.0, 0.25).unwrap();
        let t = track(&e, p);
        let epsilon = 1e-5;
        assert!(t.sample(15.0) > 10.0);
        assert!(((t.sample(epsilon) - 10.0) / epsilon - 4.0).abs() < 1e-4);
        assert!(((10.0 - t.sample(30.0 - epsilon)) / epsilon + 2.0).abs() < 1e-4);
        assert_eq!(t.sample(0.0), 10.0);
        assert_eq!(t.sample(30.0), 10.0);
    }
    #[test]
    fn unedited_legacy_curves_keep_exact_samples_including_vertical_and_hold() {
        let p = Property::PositionX.into();
        let mut e = scene(p);
        for interpolation in [
            Interpolation::Linear,
            Interpolation::Hold,
            Interpolation::Smooth,
            Interpolation::Bezier(Bezier {
                x1: 0.0,
                y1: 0.8,
                x2: 1.0,
                y2: 0.4,
            }),
        ] {
            e.execute(Command::SetInterpolation {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                interpolation,
            })
            .unwrap();
            for i in 1..300 {
                let f = i as f64 / 10.0;
                let t = f / 30.0;
                let progress = match interpolation {
                    Interpolation::Linear => t,
                    Interpolation::Hold => 0.0,
                    Interpolation::Smooth => t * t * (3.0 - 2.0 * t),
                    Interpolation::Bezier(b) => b.progress(t),
                };
                assert_eq!(track(&e, p).sample(f), 10.0 + 30.0 * progress);
            }
        }
        assert_eq!(track(&e, p).temporal_handle(0, false), None);
        // Changing only the other end keeps the vertical legacy outgoing control.
        let a = track(&e, p).keys[&0].clone();
        set(&mut e, p, 30, true, 0.0, 0.4).unwrap();
        let b = &track(&e, p).keys[&30];
        assert_eq!(controls(&a, b, 30.0).0, 0.0);
        assert_eq!(controls(&a, b, 30.0).1, 34.0);
    }
    #[test]
    fn preset_interpolation_resets_only_the_affected_segment_handles() {
        let p = Property::PositionX.into();
        let mut e = scene(p);
        for (f, incoming) in [(0, false), (30, true), (30, false), (60, true)] {
            set(&mut e, p, f, incoming, 0.0, 0.5).unwrap();
        }
        e.execute(Command::SetInterpolation {
            id: 1,
            property: Property::PositionX,
            frame: 30,
            interpolation: Interpolation::Hold,
        })
        .unwrap();
        assert!(track(&e, p).keys[&30].temporal.incoming.is_some());
        assert!(track(&e, p).keys[&0].temporal.outgoing.is_some());
        assert!(track(&e, p).keys[&30].temporal.outgoing.is_none());
        assert!(track(&e, p).keys[&60].temporal.incoming.is_none());
        assert_eq!(track(&e, p).sample(45.0), 40.0);
        set(&mut e, p, 60, true, 0.0, 0.4).unwrap();
        assert_ne!(track(&e, p).sample(45.0), 40.0);
    }
    #[test]
    fn edits_moves_copies_and_effect_presets_preserve_tangents_and_fps_units() {
        for p in [
            Property::PositionX.into(),
            PropertyPath::Effect {
                effect: 1,
                parameter: EffectParam::Radius,
            },
        ] {
            let mut e = scene(p);
            set(&mut e, p, 30, true, 2.0, 0.4).unwrap();
            set(&mut e, p, 30, false, -3.0, 0.6).unwrap();
            let handles = track(&e, p).keys[&30].temporal;
            e.execute(Command::EditTrack {
                id: 1,
                property: p,
                edit: TrackEdit::Value {
                    frame: 30,
                    value: 35.0,
                },
            })
            .unwrap();
            assert_eq!(track(&e, p).keys[&30].temporal, handles);
            let key = e.selected_layer().unwrap().copy_key(p, 30).unwrap();
            e.execute(Command::PasteKeys {
                keys: vec![key],
                frame: 90,
                target: Some(1),
            })
            .unwrap();
            assert_eq!(track(&e, p).keys[&90].temporal, handles);
            e.execute(Command::MoveKeys {
                keys: vec![KeyRef {
                    id: 1,
                    property: p,
                    frame: 90,
                }],
                delta: 10,
            })
            .unwrap();
            assert_eq!(track(&e, p).keys[&100].temporal, handles);
            let clipboard = e.copy_layers(&[1]).unwrap();
            let preset =
                EffectPreset::capture(e.selected_layer().unwrap(), None, 30.into(), "Timing")
                    .unwrap();
            e.execute(Command::ConfigureComposition {
                name: "60fps".into(),
                width: 1920,
                height: 1080,
                fps: 60,
                duration: 400,
            })
            .unwrap();
            e.execute(Command::PasteLayers(clipboard)).unwrap();
            let copied = e.selected_layer().unwrap().track(p).unwrap();
            assert_eq!(copied.keys[&60].temporal.incoming.unwrap().slope, 1.0);
            if matches!(p, PropertyPath::Effect { .. }) {
                let mut json: serde_json::Value =
                    serde_json::from_str(&preset.to_json().unwrap()).unwrap();
                assert_eq!(json["version"], 2);
                json["version"] = 1.into();
                assert!(EffectPreset::from_json(&json.to_string()).is_err());
                e.execute(Command::Effect {
                    id: 1,
                    edit: EffectEdit::ApplyPreset { preset, frame: 0 },
                })
                .unwrap();
                let t = e
                    .project()
                    .composition()
                    .layer(1)
                    .unwrap()
                    .track(PropertyPath::Effect {
                        effect: 2,
                        parameter: EffectParam::Radius,
                    })
                    .unwrap();
                assert_eq!(t.keys[&60].temporal.outgoing.unwrap().slope, -1.5);
            }
        }
    }
    #[test]
    fn invalid_locked_missing_endpoint_and_downgraded_schema_are_rejected_atomically() {
        let p = Property::PositionX.into();
        let mut e = scene(p);
        let before = e.project().clone();
        for (frame, incoming, slope, influence) in [
            (0, true, 0.0, 0.5),
            (60, false, 0.0, 0.5),
            (5, false, 0.0, 0.5),
            (30, false, f64::NAN, 0.5),
            (30, true, 0.0, 0.0001),
            (30, true, 0.0, 1.1),
        ] {
            assert!(set(&mut e, p, frame, incoming, slope, influence).is_err());
            assert_eq!(e.project(), &before);
        }
        set(&mut e, p, 30, true, 1.0, 0.5).unwrap();
        let mut json: serde_json::Value =
            serde_json::from_str(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(json["version"], 35);
        json["version"] = 34.into();
        assert!(Project::from_json(&json.to_string()).is_err());
        e.execute(Command::ToggleLocked(1)).unwrap();
        let before = e.project().clone();
        assert!(set(&mut e, p, 30, false, 0.0, 0.5).is_err());
        assert_eq!(e.project(), &before);
    }
}
