//! Stable addresses shared by the timeline, graph and effect controls.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PropertyPath {
    TimeRemap,
    Transform(Property),
    Effect {
        effect: EffectId,
        parameter: EffectParam,
    },
}
impl From<Property> for PropertyPath {
    fn from(property: Property) -> Self {
        Self::Transform(property)
    }
}
#[derive(Clone, Debug)]
pub enum TrackEdit {
    Value {
        frame: Frame,
        value: f64,
    },
    Keyframe {
        from: Frame,
        to: Frame,
        value: f64,
    },
    ToggleKey {
        frame: Frame,
    },
    ToggleAnimation {
        frame: Frame,
    },
    Interpolate {
        frame: Frame,
        interpolation: Interpolation,
    },
}
impl Layer {
    pub fn track_label(&self, path: PropertyPath) -> Option<String> {
        Some(match path {
            PropertyPath::TimeRemap => "Time Remap (s)".into(),
            PropertyPath::Transform(p) => p.label().to_string(),
            PropertyPath::Effect { effect, parameter } => {
                let effect = self.effect_stack.iter().find(|e| e.id() == effect)?;
                let spec = effect
                    .kind()
                    .parameters()
                    .into_iter()
                    .find(|p| p.parameter == parameter)?;
                format!("{} · {}", effect.name(), spec.label)
            }
        })
    }
    pub fn track(&self, path: PropertyPath) -> Option<&AnimatedProperty> {
        match path {
            PropertyPath::TimeRemap => self.time_remap.as_ref(),
            PropertyPath::Transform(p) => self.properties.get(&p),
            PropertyPath::Effect { effect, parameter } => self
                .effect_stack
                .iter()
                .find(|e| e.id() == effect)?
                .parameter(parameter),
        }
    }
    pub fn track_value(&self, path: PropertyPath, frame: Frame) -> Option<f64> {
        Some(match path {
            PropertyPath::TimeRemap => self.time_remap.as_ref()?.value_at(frame),
            PropertyPath::Transform(p) => self.property(p).value_at(frame),
            PropertyPath::Effect { effect, parameter } => {
                let effect = self.effect_stack.iter().find(|e| e.id() == effect)?;
                effect.parameter(parameter)?;
                effect.value_at(parameter, frame)
            }
        })
    }
    pub fn track_paths(&self) -> Vec<PropertyPath> {
        Property::ALL
            .into_iter()
            .map(PropertyPath::from)
            .chain(self.time_remap.as_ref().map(|_| PropertyPath::TimeRemap))
            .chain(self.effect_stack.iter().flat_map(|e| {
                e.kind()
                    .parameters()
                    .into_iter()
                    .map(move |p| PropertyPath::Effect {
                        effect: e.id(),
                        parameter: p.parameter,
                    })
            }))
            .collect()
    }
    pub fn copy_key(&self, property: PropertyPath, frame: Frame) -> Option<KeyCopy> {
        let data = self.track(property)?.keys().get(&frame)?.clone();
        let effect_kind = match property {
            PropertyPath::Transform(_) | PropertyPath::TimeRemap => None,
            PropertyPath::Effect { effect, .. } => {
                Some(self.effect_stack.iter().find(|e| e.id() == effect)?.kind())
            }
        };
        Some(KeyCopy {
            key: KeyRef {
                id: self.id,
                property,
                frame,
            },
            data,
            effect_kind,
        })
    }
    pub(super) fn track_mut(
        &mut self,
        path: PropertyPath,
    ) -> Result<&mut AnimatedProperty, String> {
        match path {
            PropertyPath::TimeRemap => self.time_remap.as_mut(),
            PropertyPath::Transform(p) => self.properties.get_mut(&p),
            PropertyPath::Effect { effect, parameter } => self
                .effect_stack
                .iter_mut()
                .find(|e| e.id() == effect)
                .and_then(|e| e.parameter_mut(parameter)),
        }
        .ok_or_else(|| "Property no longer exists on this layer".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> (Editor, PropertyPath) {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::GaussianBlur),
        })
        .unwrap();
        let p = PropertyPath::Effect {
            effect: 1,
            parameter: EffectParam::Radius,
        };
        for path in [Property::PositionX.into(), p] {
            e.execute(Command::EditTrack {
                id: 1,
                property: path,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
            e.execute(Command::EditTrack {
                id: 1,
                property: path,
                edit: TrackEdit::Value {
                    frame: 20,
                    value: 40.0,
                },
            })
            .unwrap();
        }
        (e, p)
    }
    #[test]
    fn mixed_transform_and_effect_keys_move_copy_delete_and_roundtrip_together() {
        let (mut e, p) = scene();
        let refs: Vec<_> = [Property::PositionX.into(), p]
            .into_iter()
            .map(|property| KeyRef {
                id: 1,
                property,
                frame: 20,
            })
            .collect();
        let before = e.project().clone();
        e.execute(Command::MoveKeys {
            keys: refs.clone(),
            delta: 10,
        })
        .unwrap();
        let moved = e.project().clone();
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &moved);
        let keys = refs
            .iter()
            .map(|k| {
                e.selected_layer()
                    .unwrap()
                    .copy_key(k.property, 30)
                    .unwrap()
            })
            .collect();
        e.execute(Command::PasteKeys {
            keys,
            frame: 50,
            target: Some(1),
        })
        .unwrap();
        let restored = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        for path in [Property::PositionX.into(), p] {
            assert_eq!(
                restored
                    .composition()
                    .layer(1)
                    .unwrap()
                    .track(path)
                    .unwrap()
                    .keys()
                    .keys()
                    .copied()
                    .collect::<Vec<_>>(),
                vec![0, 30, 50]
            );
        }
        e.execute(Command::DeleteKeys(vec![
            KeyRef {
                id: 1,
                property: p,
                frame: 0,
            },
            KeyRef {
                id: 1,
                property: p,
                frame: 30,
            },
            KeyRef {
                id: 1,
                property: p,
                frame: 50,
            },
        ]))
        .unwrap();
        assert!(
            e.selected_layer()
                .unwrap()
                .track(p)
                .unwrap()
                .keys()
                .is_empty()
        );
        assert_eq!(e.selected_layer().unwrap().track_value(p, 80), Some(40.0));
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track(Property::PositionX.into())
                .unwrap()
                .keys()
                .len(),
            3
        );
    }
    #[test]
    fn effect_graph_edits_keep_interpolation_and_reject_invalid_values_atomically() {
        let (mut e, p) = scene();
        let interpolation = Interpolation::Bezier(Bezier::default());
        e.execute(Command::EditTrack {
            id: 1,
            property: p,
            edit: TrackEdit::Interpolate {
                frame: 20,
                interpolation,
            },
        })
        .unwrap();
        e.execute(Command::EditTrack {
            id: 1,
            property: p,
            edit: TrackEdit::Keyframe {
                from: 20,
                to: 30,
                value: 60.0,
            },
        })
        .unwrap();
        let key = &e.selected_layer().unwrap().track(p).unwrap().keys()[&30];
        assert_eq!((key.value, key.interpolation), (60.0, interpolation));
        let before = e.project().clone();
        for edit in [
            TrackEdit::Keyframe {
                from: 30,
                to: 0,
                value: 80.0,
            },
            TrackEdit::Keyframe {
                from: 30,
                to: 40,
                value: 101.0,
            },
            TrackEdit::Keyframe {
                from: 30,
                to: 150,
                value: 80.0,
            },
            TrackEdit::Value {
                frame: 40,
                value: f64::NAN,
            },
        ] {
            assert!(
                e.execute(Command::EditTrack {
                    id: 1,
                    property: p,
                    edit
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(
            e.execute(Command::EditTrack {
                id: 1,
                property: p,
                edit: TrackEdit::Keyframe {
                    from: 30,
                    to: 40,
                    value: 80.0
                }
            })
            .is_err()
        );
    }
    #[test]
    fn missing_effect_and_mismatched_paste_are_errors_without_partial_edits() {
        let (mut e, p) = scene();
        let copied = e.selected_layer().unwrap().copy_key(p, 20).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::Effect {
            id: 2,
            edit: EffectEdit::Add(EffectKind::Glow),
        })
        .unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::PasteKeys {
                keys: vec![copied],
                frame: 40,
                target: Some(2)
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        let missing = KeyRef {
            id: 1,
            property: PropertyPath::Effect {
                effect: 999,
                parameter: EffectParam::Radius,
            },
            frame: 20,
        };
        for command in [
            Command::MoveKeys {
                keys: vec![
                    KeyRef {
                        id: 1,
                        property: p,
                        frame: 20,
                    },
                    missing,
                ],
                delta: 5,
            },
            Command::DeleteKeys(vec![
                KeyRef {
                    id: 1,
                    property: p,
                    frame: 20,
                },
                missing,
            ]),
        ] {
            assert!(e.execute(command).is_err());
            assert_eq!(e.project(), &before);
        }
    }
}

pub(super) fn command(id: LayerId, property: PropertyPath, edit: TrackEdit) -> Command {
    match property {
        PropertyPath::TimeRemap => Command::EditTimeRemap { id, edit },
        PropertyPath::Transform(property) => match edit {
            TrackEdit::Value { frame, value } => Command::SetValue {
                id,
                property,
                frame,
                value,
            },
            TrackEdit::Keyframe { from, to, value } => Command::EditKeyframe {
                id,
                property,
                from,
                to,
                value,
            },
            TrackEdit::ToggleKey { frame } => Command::ToggleKeyframe {
                id,
                property,
                frame,
            },
            TrackEdit::ToggleAnimation { frame } => Command::ToggleAnimation {
                id,
                property,
                frame,
            },
            TrackEdit::Interpolate {
                frame,
                interpolation,
            } => Command::SetInterpolation {
                id,
                property,
                frame,
                interpolation,
            },
        },
        PropertyPath::Effect { effect, parameter } => Command::Effect {
            id,
            edit: match edit {
                TrackEdit::Value { frame, value } => EffectEdit::SetValue {
                    effect,
                    parameter,
                    frame,
                    value,
                },
                TrackEdit::Keyframe { from, to, value } => EffectEdit::EditKeyframe {
                    effect,
                    parameter,
                    from,
                    to,
                    value,
                },
                TrackEdit::ToggleKey { frame } => EffectEdit::ToggleKey {
                    effect,
                    parameter,
                    frame,
                },
                TrackEdit::ToggleAnimation { frame } => EffectEdit::ToggleAnimation {
                    effect,
                    parameter,
                    frame,
                },
                TrackEdit::Interpolate {
                    frame,
                    interpolation,
                } => EffectEdit::Interpolate {
                    effect,
                    parameter,
                    frame,
                    interpolation,
                },
            },
        },
    }
}
