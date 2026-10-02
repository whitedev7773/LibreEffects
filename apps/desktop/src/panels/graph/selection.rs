use super::*;
use libre_effects_core::{KeyRef, TemporalHandle};
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct Sample {
    pub key: KeyRef,
    pub value: f64,
    pub handle: Option<TemporalHandle>,
}
pub(super) fn active(state: &EditorState) -> Vec<KeyRef> {
    state
        .selected_keys
        .iter()
        .copied()
        .filter(|k| {
            Some(k.id) == state.editor.selected()
                && k.property == state.graph_property
                && state
                    .editor
                    .selected_layer()
                    .and_then(|l| l.track(k.property))
                    .is_some_and(|t| t.keys().contains_key(&k.frame))
        })
        .collect()
}
pub(super) fn snapshot(
    track: &AnimatedProperty,
    keys: &[KeyRef],
    incoming: Option<bool>,
) -> Vec<Sample> {
    keys.iter()
        .filter_map(|key| {
            Some(Sample {
                key: *key,
                value: track.keys().get(&key.frame)?.value,
                handle: incoming.and_then(|side| {
                    let exists = if side {
                        track.keys().range(..key.frame).next_back().is_some()
                    } else {
                        track.keys().range(key.frame + 1..).next().is_some()
                    };
                    exists.then(|| {
                        track
                            .temporal_handle(key.frame, side)
                            .unwrap_or(TemporalHandle {
                                slope: 0.0,
                                influence: 1.0 / 3.0,
                            })
                    })
                }),
            })
        })
        .collect()
}
pub(super) fn clicked(mut keys: BTreeSet<KeyRef>, key: KeyRef, toggle: bool) -> BTreeSet<KeyRef> {
    if toggle {
        if !keys.remove(&key) {
            keys.insert(key);
        }
    } else if !keys.contains(&key) {
        keys = [key].into();
    }
    keys
}
pub(super) fn clamp_delta(keys: &[Sample], delta: i64, duration: u32) -> i64 {
    let first = keys.iter().map(|k| k.key.frame).min().unwrap_or(0);
    let last = keys.iter().map(|k| k.key.frame).max().unwrap_or(0);
    delta.clamp(-(first as i64), duration as i64 - 1 - last as i64)
}
/// Remove the whole selection before inserting moved keys, so occupied selected
/// source frames are valid destinations. The core applies this as one transaction.
pub(super) fn translate(
    keys: &[Sample],
    delta: i64,
    amount: f64,
    incoming: Option<bool>,
) -> Result<Command, String> {
    if !amount.is_finite() {
        return Err("Enter a finite graph offset".into());
    }
    let mut commands = vec![Command::MoveKeys {
        keys: keys.iter().map(|k| k.key).collect(),
        delta,
    }];
    for sample in keys {
        let to = u32::try_from(sample.key.frame as i64 + delta)
            .map_err(|_| "Key time is outside the composition")?;
        let KeyRef { id, property, .. } = sample.key;
        if let Some(incoming) = incoming {
            if amount != 0.0
                && let Some(mut handle) = sample.handle
            {
                handle.slope += amount;
                commands.push(Command::SetTemporalHandle {
                    id,
                    property,
                    frame: to,
                    incoming,
                    handle,
                });
            }
        } else if amount != 0.0 {
            commands.push(Command::EditTrack {
                id,
                property,
                edit: TrackEdit::Keyframe {
                    from: to,
                    to,
                    value: sample.value + amount,
                },
            });
        }
    }
    Ok(Command::Batch(commands))
}
pub(super) fn inside(a: Point<Pixels>, b: Point<Pixels>, p: Point<Pixels>) -> bool {
    p.x >= a.x.min(b.x) - px(4.0)
        && p.x <= a.x.max(b.x) + px(4.0)
        && p.y >= a.y.min(b.y) - px(4.0)
        && p.y <= a.y.max(b.y) + px(4.0)
}
pub(super) fn ease(
    track: &AnimatedProperty,
    keys: &[KeyRef],
    incoming: bool,
    outgoing: bool,
) -> Command {
    let mut commands = Vec::new();
    for &KeyRef {
        id,
        property,
        frame,
    } in keys
    {
        let sides = [
            (
                true,
                incoming && track.keys().range(..frame).next_back().is_some(),
            ),
            (
                false,
                outgoing && track.keys().range(frame + 1..).next().is_some(),
            ),
        ];
        if sides.iter().any(|(_, yes)| *yes) {
            commands.push(Command::SetTemporalMode {
                id,
                property,
                frame,
                mode: TemporalMode::Independent,
            });
            for (incoming, _) in sides.into_iter().filter(|(_, yes)| *yes) {
                commands.push(Command::SetTemporalHandle {
                    id,
                    property,
                    frame,
                    incoming,
                    handle: TemporalHandle {
                        slope: 0.0,
                        influence: 1.0 / 3.0,
                    },
                });
            }
        }
    }
    Command::Batch(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, Project, Property};
    fn scene(property: Property) -> (Editor, Vec<KeyRef>) {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        let keys = [0, 20, 40].map(|frame| KeyRef {
            id: 1,
            property: property.into(),
            frame,
        });
        for (k, value) in keys.into_iter().zip([30.0, 60.0, 40.0]) {
            e.execute(Command::EditTrack {
                id: k.id,
                property: k.property,
                edit: TrackEdit::ToggleKey { frame: k.frame },
            })
            .unwrap();
            e.execute(Command::EditTrack {
                id: k.id,
                property: k.property,
                edit: TrackEdit::Value {
                    frame: k.frame,
                    value,
                },
            })
            .unwrap();
        }
        (e, keys.to_vec())
    }
    fn samples(e: &Editor, keys: &[KeyRef], side: Option<bool>) -> Vec<Sample> {
        snapshot(
            e.selected_layer().unwrap().track(keys[0].property).unwrap(),
            keys,
            side,
        )
    }
    #[test]
    fn speed_retime_without_vertical_change_preserves_auto_mode() {
        let (mut e, keys) = scene(Property::PositionX);
        e.execute(Command::SetTemporalMode {
            id: 1,
            property: keys[0].property,
            frame: 20,
            mode: TemporalMode::Auto,
        })
        .unwrap();
        let original = e.project().clone();
        let selected = &keys[1..2];
        let before = e
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()[&20]
            .clone();
        e.execute(translate(&samples(&e, selected, Some(false)), 5, 0.0, Some(false)).unwrap())
            .unwrap();
        let track = e.selected_layer().unwrap().property(Property::PositionX);
        assert_eq!(track.keys()[&25], before);
        assert!(!track.keys().contains_key(&20));
        e.undo();
        assert_eq!(e.project(), &original);
    }
    #[test]
    fn group_value_move_allows_selected_destinations_preserves_handles_and_undo() {
        let (mut e, keys) = scene(Property::PositionX);
        e.execute(Command::SetTemporalMode {
            id: 1,
            property: keys[0].property,
            frame: 20,
            mode: TemporalMode::Auto,
        })
        .unwrap();
        let original = e.project().clone();
        e.execute(translate(&samples(&e, &keys, None), 20, 10.0, None).unwrap())
            .unwrap();
        let t = e.selected_layer().unwrap().property(Property::PositionX);
        assert_eq!(
            t.keys().keys().copied().collect::<Vec<_>>(),
            vec![20, 40, 60]
        );
        assert_eq!(
            t.keys().values().map(|k| k.value).collect::<Vec<_>>(),
            vec![40.0, 70.0, 50.0]
        );
        assert_eq!(t.keys()[&40].temporal.mode, TemporalMode::Auto);
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        e.undo();
        assert_eq!(e.project(), &original);
        e.redo();
        assert_eq!(e.project(), &saved);
    }
    #[test]
    fn group_collision_invalid_value_and_lock_fail_atomically() {
        let (mut e, keys) = scene(Property::Opacity);
        let before = e.project().clone();
        assert!(
            e.execute(translate(&samples(&e, &keys[..2], None), 20, 10.0, None).unwrap())
                .is_err()
        );
        assert_eq!(e.project(), &before);
        assert!(
            e.execute(translate(&samples(&e, &keys, None), 5, 80.0, None).unwrap())
                .is_err()
        );
        assert_eq!(e.project(), &before);
        assert!(translate(&samples(&e, &keys, None), -1, 0.0, None).is_err());
        assert!(translate(&samples(&e, &keys, None), 0, f64::NAN, None).is_err());
        e.execute(Command::ToggleLocked(1)).unwrap();
        let locked = e.project().clone();
        assert!(
            e.execute(translate(&samples(&e, &keys, None), 1, 0.0, None).unwrap())
                .is_err()
        );
        assert_eq!(e.project(), &locked);
    }
    #[test]
    fn speed_group_offsets_each_velocity_keeps_values_and_skips_missing_side() {
        let (mut e, keys) = scene(Property::PositionX);
        e.execute(Command::SetTemporalMode {
            id: 1,
            property: keys[0].property,
            frame: 20,
            mode: TemporalMode::Auto,
        })
        .unwrap();
        let before = e.project().clone();
        let data = samples(&e, &keys, Some(false));
        assert!(data[2].handle.is_none());
        e.execute(translate(&data, 5, -2.0, Some(false)).unwrap())
            .unwrap();
        let t = e.selected_layer().unwrap().property(Property::PositionX);
        assert!((t.keys()[&5].temporal.outgoing.unwrap().slope + 0.5).abs() < 1e-12);
        assert_eq!(t.keys()[&25].temporal.mode, TemporalMode::Continuous);
        assert_eq!(t.keys()[&25].temporal.incoming.unwrap().slope, -2.0);
        assert_eq!(t.keys()[&25].temporal.outgoing.unwrap().slope, -2.0);
        assert_eq!(
            t.keys().values().map(|k| k.value).collect::<Vec<_>>(),
            vec![30.0, 60.0, 40.0]
        );
        e.undo();
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn selection_toggle_preserves_group_and_bounds_preserve_spacing() {
        let (e, keys) = scene(Property::PositionX);
        let chosen = clicked([keys[0], keys[1]].into(), keys[0], false);
        assert_eq!(chosen.len(), 2);
        assert_eq!(clicked(chosen.clone(), keys[0], true), [keys[1]].into());
        assert_eq!(clicked(chosen.clone(), keys[2], true).len(), 3);
        assert_eq!(clicked(chosen, keys[2], false), [keys[2]].into());
        let data = samples(&e, &keys, None);
        assert_eq!(clamp_delta(&data, -50, 60), 0);
        assert_eq!(clamp_delta(&data, 50, 60), 19);
        let a = point(px(100.0), px(100.0));
        let b = point(px(10.0), px(10.0));
        assert!(inside(a, b, point(px(50.0), px(50.0))));
        assert!(!inside(a, b, point(px(110.0), px(50.0))));
    }
    #[test]
    fn group_ease_changes_all_selected_sides_and_delete_is_one_undo() {
        let (mut e, keys) = scene(Property::PositionX);
        for k in &keys {
            e.execute(Command::SetTemporalMode {
                id: 1,
                property: k.property,
                frame: k.frame,
                mode: TemporalMode::Auto,
            })
            .unwrap();
        }
        let before = e.project().clone();
        let t = e.selected_layer().unwrap().property(Property::PositionX);
        e.execute(ease(t, &keys, true, true)).unwrap();
        let t = e.selected_layer().unwrap().property(Property::PositionX);
        for k in &keys {
            let h = t.keys()[&k.frame].temporal;
            assert_eq!(h.mode, TemporalMode::Independent);
            if k.frame > 0 {
                assert_eq!(h.incoming.unwrap().slope, 0.0);
            }
            if k.frame < 40 {
                assert_eq!(h.outgoing.unwrap().slope, 0.0);
            }
        }
        e.undo();
        assert_eq!(e.project(), &before);
        e.execute(Command::DeleteKeys(keys)).unwrap();
        assert!(
            e.selected_layer()
                .unwrap()
                .property(Property::PositionX)
                .keys()
                .is_empty()
        );
        e.undo();
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn selected_channel_filter_excludes_hidden_layers_properties_and_stale_keys() {
        let (e, keys) = scene(Property::PositionX);
        let mut state = EditorState::default();
        state.editor = e;
        state.graph_property = Property::PositionX.into();
        state.selected_keys = keys.iter().copied().collect();
        for k in [
            KeyRef {
                frame: 12,
                ..keys[0]
            },
            KeyRef { id: 9, ..keys[0] },
            KeyRef {
                property: Property::PositionY.into(),
                ..keys[0]
            },
        ] {
            state.selected_keys.insert(k);
        }
        assert_eq!(active(&state), keys);
    }
    #[test]
    fn group_edit_saved_preview_and_output_pixels_match() {
        let (mut e, keys) = scene(Property::PositionX);
        e.execute(translate(&samples(&e, &keys, None), 20, 600.0, None).unwrap())
            .unwrap();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let r = crate::rendering::Renderer::new();
        for frame in [20, 30, 40, 50, 60] {
            let preview = r.render(e.project(), frame, 384).unwrap();
            let output = r.render_output(&saved, frame, 384, 216).unwrap();
            assert_eq!(preview, output);
            let xs = output
                .enumerate_pixels()
                .filter(|(_, _, p)| p[3] > 0)
                .map(|(x, _, _)| x)
                .collect::<Vec<_>>();
            let center = (*xs.iter().min().unwrap() + *xs.iter().max().unwrap() + 1) as f64 / 2.0;
            let want = match frame {
                20 => 630.0,
                30 => 645.0,
                40 => 660.0,
                50 => 650.0,
                _ => 640.0,
            };
            assert!((center - want / 5.0).abs() < 1.0);
        }
    }
}
