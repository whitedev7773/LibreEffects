//! Portable effect stacks with relative animation time and fresh destination identities.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectPreset {
    version: u32,
    name: String,
    fps: FrameRate,
    effects: Vec<EffectInstance>,
}
impl EffectPreset {
    pub const MAX_BYTES: usize = 8 * 1024 * 1024;
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn effects(&self) -> &[EffectInstance] {
        &self.effects
    }
    pub fn fps(&self) -> FrameRate {
        self.fps
    }
    pub fn key_count(&self) -> usize {
        self.effects
            .iter()
            .flat_map(|e| e.parameters.values())
            .map(|t| t.keys.len())
            .sum()
    }
    /// Capture the complete stack, or a single instance. Legacy effects must be converted first.
    pub fn capture(
        layer: &Layer,
        effect: Option<EffectId>,
        fps: FrameRate,
        name: &str,
    ) -> Result<Self, String> {
        if effect.is_none() && layer.effects != Effects::default() {
            return Err(
                "Convert existing effects to an ordered stack before saving a preset".into(),
            );
        }
        let mut effects: Vec<_> = layer
            .effect_stack
            .iter()
            .filter(|e| effect.is_none_or(|id| id == e.id))
            .cloned()
            .collect();
        if effects.is_empty() {
            return Err("No effects to save".into());
        }
        let start = effects
            .iter()
            .flat_map(|e| e.parameters.values())
            .flat_map(|p| p.keys.keys())
            .copied()
            .min()
            .unwrap_or(0);
        for (i, e) in effects.iter_mut().enumerate() {
            e.id = i as u64 + 1;
            for track in e.parameters.values_mut() {
                track.keys = std::mem::take(&mut track.keys)
                    .into_iter()
                    .map(|(f, k)| (f - start, k))
                    .collect();
            }
        }
        let result = Self {
            version: if effects
                .iter()
                .flat_map(|e| e.parameters.values())
                .any(|t| t.keys.values().any(|k| !k.temporal.is_empty()))
            {
                2
            } else {
                1
            },
            name: name.trim().into(),
            fps,
            effects,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn renamed(mut self, name: &str) -> Result<Self, String> {
        self.name = name.trim().into();
        self.validate()?;
        Ok(self)
    }
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if text.len() > Self::MAX_BYTES {
            return Err("Preset exceeds 8 MiB".into());
        }
        Ok(text)
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > Self::MAX_BYTES {
            return Err("Preset exceeds 8 MiB".into());
        }
        let preset: Self =
            serde_json::from_str(text).map_err(|e| format!("Invalid effect preset: {e}"))?;
        preset.validate()?;
        Ok(preset)
    }
    fn validate(&self) -> Result<(), String> {
        if !(1..=2).contains(&self.version) {
            return Err("Unsupported effect preset version".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
        {
            return Err("Preset names must contain 1–128 bytes without control characters".into());
        }
        if !self.fps.valid() || self.effects.is_empty() || self.key_count() > 40000 {
            return Err("Invalid preset frame rate or effect/key count".into());
        }
        if self
            .effects
            .iter()
            .enumerate()
            .any(|(i, e)| e.id != i as u64 + 1)
        {
            return Err("Invalid preset effect identities".into());
        }
        for key in self
            .effects
            .iter()
            .flat_map(|e| e.parameters.values())
            .flat_map(|t| t.keys.values())
        {
            if !key.temporal.valid() || (self.version < 2 && !key.temporal.is_empty()) {
                return Err("Invalid preset temporal handles or version".into());
            }
        }
        validate_stack(
            &self.effects,
            self.effects.len() as u64 + 1,
            self.fps.max_duration(),
        )
    }
    pub(super) fn apply(
        &self,
        layer: &mut Layer,
        frame: Frame,
        fps: FrameRate,
        duration: Frame,
    ) -> Result<(), String> {
        self.validate()?;
        if frame >= duration {
            return Err("Preset start is outside the composition".into());
        }
        if layer.effect_stack.len() + self.effects.len() > 64 {
            return Err("A layer supports up to 64 effects".into());
        }
        let next = layer
            .next_effect_id
            .checked_add(self.effects.len() as u64)
            .ok_or("Effect identity limit reached")?;
        let mut copies = self.effects.clone();
        for (i, effect) in copies.iter_mut().enumerate() {
            effect.id = layer.next_effect_id + i as u64;
            for track in effect.parameters.values_mut() {
                let mut keys = BTreeMap::new();
                for (offset, key) in &track.keys {
                    let offset = self
                        .fps
                        .convert_frames(u64::from(*offset), fps, FrameRounding::Nearest)
                        .ok_or("Preset time overflow")?;
                    let at = u32::try_from(u64::from(frame) + offset)
                        .map_err(|_| "Preset time overflow")?;
                    if at >= duration {
                        return Err("Preset keys exceed the composition. Extend its duration or apply earlier.".into());
                    }
                    let mut key = key.clone();
                    key.temporal.rescale(self.fps.as_f64() / fps.as_f64());
                    if keys.insert(at, key).is_some() {
                        return Err(
                            "Destination FPS merges preset keys. Use a higher frame rate.".into(),
                        );
                    }
                }
                track.keys = keys;
            }
        }
        layer.effect_stack.extend(copies);
        layer.next_effect_id = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddSolid).unwrap();
        for kind in [EffectKind::Fill, EffectKind::GaussianBlur] {
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(kind),
            })
            .unwrap();
        }
        for edit in [
            EffectEdit::Rename {
                effect: 1,
                name: "Color pass".into(),
            },
            EffectEdit::SetValue {
                effect: 1,
                parameter: EffectParam::Red,
                frame: 10,
                value: 20.0,
            },
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter: EffectParam::Red,
                frame: 10,
            },
            EffectEdit::SetValue {
                effect: 1,
                parameter: EffectParam::Red,
                frame: 40,
                value: 240.0,
            },
            EffectEdit::Interpolate {
                effect: 1,
                parameter: EffectParam::Red,
                frame: 10,
                interpolation: Interpolation::Smooth,
            },
            EffectEdit::Bypass {
                effect: 2,
                bypassed: true,
            },
        ] {
            e.execute(Command::Effect { id: 1, edit }).unwrap();
        }
        e
    }
    #[test]
    fn animated_stack_remaps_time_and_ids_with_one_undo_and_file_roundtrip() {
        let mut e = source();
        let p = EffectPreset::capture(
            e.project().composition().layer(1).unwrap(),
            None,
            30.into(),
            "My effect",
        )
        .unwrap();
        assert_eq!(p.key_count(), 2);
        assert_eq!(
            p.effects[0].parameters[&EffectParam::Red]
                .keys
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![0, 30]
        );
        let p = EffectPreset::from_json(&p.to_json().unwrap()).unwrap();
        e.execute(Command::ConfigureComposition {
            name: "60 fps".into(),
            width: 1920,
            height: 1080,
            fps: 60,
            duration: 240,
        })
        .unwrap();
        let before = e.project().clone();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::ApplyPreset {
                preset: p,
                frame: 25,
            },
        })
        .unwrap();
        let after = e.project().clone();
        let stack = after.composition().layer(1).unwrap().effect_stack();
        assert_eq!(
            stack.iter().map(|e| e.id()).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(stack[2].name(), "Color pass");
        assert!(stack[3].bypassed());
        let track = stack[2].parameter(EffectParam::Red).unwrap();
        assert_eq!(
            track.keys().keys().copied().collect::<Vec<_>>(),
            vec![25, 85]
        );
        assert_eq!(track.keys()[&25].interpolation, Interpolation::Smooth);
        assert_eq!(track.value_at(55), 130.0);
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
    }
    #[test]
    fn malformed_outside_locked_and_merged_keys_are_atomic() {
        let mut e = source();
        let p = EffectPreset::capture(
            e.project().composition().layer(1).unwrap(),
            Some(1),
            240.into(),
            "Rapid",
        )
        .unwrap();
        let mut bad = p.clone();
        bad.version = 999;
        assert!(EffectPreset::from_json(&serde_json::to_string(&bad).unwrap()).is_err());
        bad = p.clone();
        bad.effects[0]
            .parameters
            .get_mut(&EffectParam::Red)
            .unwrap()
            .value = 999.0;
        assert!(bad.to_json().is_err());
        let before = e.project().clone();
        assert!(
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::ApplyPreset {
                    preset: p.clone(),
                    frame: 149
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::ApplyPreset {
                    preset: p.clone(),
                    frame: 0
                }
            })
            .is_err()
        );
        e.execute(Command::ToggleLocked(1)).unwrap();
        e.execute(Command::ConfigureComposition {
            name: "Low FPS".into(),
            width: 1920,
            height: 1080,
            fps: 1,
            duration: 150,
        })
        .unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::ApplyPreset {
                    preset: p,
                    frame: 0
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }

    #[test]
    fn multi_layer_application_is_one_transaction_and_preserves_legacy_color_space() {
        let mut e = source();
        let p = EffectPreset::capture(
            e.project().composition().layer(1).unwrap(),
            None,
            30.into(),
            "Stack",
        )
        .unwrap();
        e.execute(Command::AddSolid).unwrap();
        e.execute(Command::ToggleLocked(2)).unwrap();
        let before = e.project().clone();
        let commands = || {
            Command::Batch(
                [1, 2]
                    .into_iter()
                    .map(|id| Command::Effect {
                        id,
                        edit: EffectEdit::ApplyPreset {
                            preset: p.clone(),
                            frame: 50,
                        },
                    })
                    .collect(),
            )
        };
        assert!(e.execute(commands()).is_err());
        assert_eq!(e.project(), &before);
        e.execute(Command::ToggleLocked(2)).unwrap();
        let before = e.project().clone();
        e.execute(commands()).unwrap();
        assert_eq!(
            e.project()
                .composition()
                .layer(1)
                .unwrap()
                .effect_stack()
                .len(),
            4
        );
        assert_eq!(
            e.project()
                .composition()
                .layer(2)
                .unwrap()
                .effect_stack()
                .len(),
            2
        );
        e.undo();
        assert_eq!(e.project(), &before);
        let mut linear = p;
        linear.effects[0].color_space = EffectColorSpace::LinearRgb;
        let copy = EffectPreset::from_json(&linear.to_json().unwrap()).unwrap();
        e.execute(Command::Effect {
            id: 2,
            edit: EffectEdit::ApplyPreset {
                preset: copy,
                frame: 0,
            },
        })
        .unwrap();
        assert_eq!(
            e.project().composition().layer(2).unwrap().effect_stack()[0].color_space(),
            EffectColorSpace::LinearRgb
        );
    }
}
