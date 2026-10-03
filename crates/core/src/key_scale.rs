//! Affine scaling of selected scalar keys as one atomic editor transaction.
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct KeyScale {
    pub time_origin: f64,
    pub time_scale: f64,
    pub value_origin: f64,
    pub value_scale: f64,
}
impl KeyScale {
    fn validate(self) -> Result<(), String> {
        if ![
            self.time_origin,
            self.time_scale,
            self.value_origin,
            self.value_scale,
        ]
        .into_iter()
        .all(f64::is_finite)
            || self.time_scale <= 0.0
        {
            return Err("Key scaling requires finite values and a positive time scale".into());
        }
        Ok(())
    }
    pub fn frame(self, frame: Frame, duration: Frame) -> Result<Frame, String> {
        self.validate()?;
        let to = if self.time_scale == 1.0 {
            frame as f64
        } else {
            (self.time_origin + (frame as f64 - self.time_origin) * self.time_scale).round()
        };
        if !to.is_finite() || to < 0.0 || to >= duration as f64 {
            return Err("Scaled key time is outside the composition".into());
        }
        Ok(to as Frame)
    }
}
pub(super) fn apply(state: &mut Snapshot, keys: &[KeyRef], scale: KeyScale) -> Result<(), String> {
    scale.validate()?;
    if keys.is_empty() {
        return Err("Select keyframes to scale".into());
    }
    let duration = state.project.composition.duration;
    let mut moved = Vec::new();
    for key in keys.iter().copied().collect::<BTreeSet<_>>() {
        if matches!(key.property, PropertyPath::Path(_)) {
            return Err("Geometry path keys do not support scalar key scaling".into());
        }
        let to = scale.frame(key.frame, duration)?;
        let track = editing::editable(state, key.id)?.track_mut(key.property)?;
        let mut data = track
            .keys
            .remove(&key.frame)
            .ok_or("Selected key no longer exists")?;
        if scale.value_scale != 1.0 {
            data.value = scale.value_origin + (data.value - scale.value_origin) * scale.value_scale;
        }
        // Influence is relative to segment duration. Scale signed slopes to keep
        // the same affine curve when neighboring selected times are exact frames.
        for handle in [&mut data.temporal.incoming, &mut data.temporal.outgoing]
            .into_iter()
            .flatten()
        {
            handle.slope *= scale.value_scale / scale.time_scale;
            if !handle.valid() {
                return Err("Scaled key velocity is outside the supported range".into());
            }
        }
        moved.push((key, to, data));
    }
    for (key, to, data) in moved {
        let track = editing::editable(state, key.id)?.track_mut(key.property)?;
        if track.keys.contains_key(&to) {
            return Err("Scaled keyframes would collide with another key".into());
        }
        track.keys.insert(to, data);
    }
    // Editor::execute validates property-specific value bounds before committing.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene(property: Property) -> (Editor, Vec<KeyRef>) {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        let keys: Vec<_> = [10, 20, 40]
            .into_iter()
            .map(|frame| KeyRef {
                id: 1,
                property: property.into(),
                frame,
            })
            .collect();
        for (key, value) in keys.iter().zip([20.0, 60.0, 30.0]) {
            e.execute(Command::EditTrack {
                id: 1,
                property: key.property,
                edit: TrackEdit::ToggleKey { frame: key.frame },
            })
            .unwrap();
            e.execute(Command::EditTrack {
                id: 1,
                property: key.property,
                edit: TrackEdit::Value {
                    frame: key.frame,
                    value,
                },
            })
            .unwrap();
        }
        (e, keys)
    }
    fn scale() -> KeyScale {
        KeyScale {
            time_origin: 10.0,
            time_scale: 2.0,
            value_origin: 20.0,
            value_scale: 1.5,
        }
    }
    #[test]
    fn scaling_preserves_affine_curve_tangents_modes_and_roundtrip() {
        for mode in [
            TemporalMode::Independent,
            TemporalMode::Continuous,
            TemporalMode::Auto,
        ] {
            let (mut e, keys) = scene(Property::PositionX);
            e.execute(Command::SetTemporalHandle {
                id: 1,
                property: keys[0].property,
                frame: 20,
                incoming: true,
                handle: TemporalHandle {
                    slope: 1.25,
                    influence: 0.7,
                },
            })
            .unwrap();
            e.execute(Command::SetTemporalMode {
                id: 1,
                property: keys[0].property,
                frame: 20,
                mode,
            })
            .unwrap();
            let before = e.project().clone();
            let old = before
                .composition()
                .layer(1)
                .unwrap()
                .track(keys[0].property)
                .unwrap();
            e.execute(Command::ScaleKeys {
                keys: keys.clone(),
                scale: scale(),
            })
            .unwrap();
            let track = e.selected_layer().unwrap().track(keys[0].property).unwrap();
            assert_eq!(
                track.keys.keys().copied().collect::<Vec<_>>(),
                vec![10, 30, 70]
            );
            assert_eq!(track.value, old.value);
            assert_eq!(track.keys[&30].temporal.mode, mode);
            assert_eq!(
                track.keys[&30].temporal.incoming.map(|h| h.influence),
                old.keys[&20].temporal.incoming.map(|h| h.influence)
            );
            for i in 0..=300 {
                let f = 10.0 + i as f64 / 10.0;
                let expected = 20.0 + (old.sample(f) - 20.0) * 1.5;
                assert!(
                    (track.sample(10.0 + (f - 10.0) * 2.0) - expected).abs() < 1e-6,
                    "{mode:?} at {f}"
                );
            }
            let after = e.project().clone();
            assert_eq!(
                Project::from_json(&after.to_json().unwrap()).unwrap(),
                after
            );
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &after);
        }
    }
    #[test]
    fn selected_destinations_are_allowed_but_all_collision_failures_are_atomic() {
        let (mut e, keys) = scene(Property::PositionX);
        let before = e.project().clone();
        let scale = KeyScale {
            time_origin: 0.0,
            ..scale()
        };
        assert!(
            e.execute(Command::ScaleKeys {
                keys: keys[..2].to_vec(),
                scale
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        assert!(
            e.execute(Command::ScaleKeys {
                keys: keys.clone(),
                scale: KeyScale {
                    time_scale: 0.01,
                    ..scale
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        e.execute(Command::ScaleKeys { keys, scale }).unwrap();
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .property(Property::PositionX)
                .keys
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![20, 40, 80]
        );
        e.undo();
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn invalid_ranges_locks_and_values_preserve_document_and_history() {
        let (mut e, keys) = scene(Property::Opacity);
        let before = e.project().clone();
        for scale in [
            KeyScale {
                value_scale: 5.0,
                ..scale()
            },
            KeyScale {
                time_scale: -1.0,
                ..scale()
            },
            KeyScale {
                time_scale: 20.0,
                ..scale()
            },
            KeyScale {
                value_origin: f64::NAN,
                ..scale()
            },
            KeyScale {
                time_scale: 0.0,
                ..scale()
            },
        ] {
            assert!(
                e.execute(Command::ScaleKeys {
                    keys: keys.clone(),
                    scale
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        let locked = e.project().clone();
        assert!(
            e.execute(Command::ScaleKeys {
                keys,
                scale: scale()
            })
            .is_err()
        );
        assert_eq!(e.project(), &locked);
        e.undo();
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn identity_is_exact_and_value_reflection_keeps_key_order() {
        let (mut e, keys) = scene(Property::PositionX);
        let before = e.project().clone();
        e.execute(Command::ScaleKeys {
            keys: keys.clone(),
            scale: KeyScale {
                time_origin: 1e100,
                time_scale: 1.0,
                value_origin: 1e100,
                value_scale: 1.0,
            },
        })
        .unwrap();
        assert_eq!(e.project(), &before);
        e.execute(Command::ScaleKeys {
            keys,
            scale: KeyScale {
                time_scale: 1.0,
                value_scale: -1.0,
                ..scale()
            },
        })
        .unwrap();
        let t = e.selected_layer().unwrap().property(Property::PositionX);
        assert_eq!(
            t.keys.values().map(|k| k.value).collect::<Vec<_>>(),
            vec![20.0, -20.0, 10.0]
        );
    }
}
