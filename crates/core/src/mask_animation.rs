use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MaskParam {
    Opacity,
    Feather,
    Expansion,
}
impl MaskParam {
    pub const ALL: [Self; 3] = [Self::Opacity, Self::Feather, Self::Expansion];
    pub fn label(self) -> &'static str {
        match self {
            Self::Opacity => "Mask Opacity",
            Self::Feather => "Mask Feather",
            Self::Expansion => "Mask Expansion",
        }
    }
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::Opacity => (0.0, 100.0),
            Self::Feather => (0.0, 256.0),
            Self::Expansion => (-256.0, 256.0),
        }
    }
    pub fn accepts(self, value: f64) -> bool {
        let (min, max) = self.bounds();
        value.is_finite() && (min..=max).contains(&value)
    }
}
pub(super) fn defaults() -> BTreeMap<MaskParam, AnimatedProperty> {
    MaskParam::ALL
        .into_iter()
        .map(|p| {
            (
                p,
                AnimatedProperty::new(if p == MaskParam::Opacity { 100.0 } else { 0.0 }),
            )
        })
        .collect()
}
impl PathMask {
    pub fn value_at(&self, parameter: MaskParam, frame: Frame) -> f64 {
        let (min, max) = parameter.bounds();
        self.parameters[&parameter].value_at(frame).clamp(min, max)
    }
    pub(super) fn default_parameters(&self) -> bool {
        self.parameters == defaults()
    }
}
pub(super) fn migrate(project: &mut Project) {
    if project.version != 29 {
        return;
    }
    let mut changed = false;
    for l in project.compositions_mut().flat_map(|c| c.layers.iter_mut()) {
        if !l.path_masks.is_empty() {
            for (index, m) in l.path_masks.iter_mut().enumerate() {
                m.id = index as u64 + 1;
            }
            l.next_mask_id = l.path_masks.len() as u64 + 1;
            changed = true;
        }
    }
    if changed {
        project.version = 30;
    }
}
pub(super) fn set(layer: &mut Layer, masks: &[PathMask]) -> Result<(), String> {
    let mut masks = masks.to_vec();
    for m in &mut masks {
        if m.id == 0 {
            m.id = layer.next_mask_id;
            layer.next_mask_id = layer
                .next_mask_id
                .checked_add(1)
                .ok_or("Mask identity limit reached")?;
        } else if !layer.path_masks.iter().any(|old| old.id == m.id) {
            return Err("Mask no longer exists".into());
        }
    }
    layer.path_masks = masks;
    Ok(())
}
pub(super) fn validate(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    if layer.next_mask_id == 0 {
        return Err("Invalid mask identity counter".into());
    }
    let mut ids = BTreeSet::new();
    for mask in &layer.path_masks {
        if version < 30 {
            if mask.id != 0 || !mask.default_parameters() {
                return Err("Mask animation requires version 30".into());
            }
        } else if mask.id == 0 || mask.id >= layer.next_mask_id || !ids.insert(mask.id) {
            return Err("Invalid mask identity".into());
        }
        if mask.parameters.len() != MaskParam::ALL.len() {
            return Err("Invalid mask parameters".into());
        }
        for p in MaskParam::ALL {
            let track = mask.parameters.get(&p).ok_or("Missing mask parameter")?;
            if !p.accepts(track.value)
                || track.keys.len() > 10000
                || track
                    .keys
                    .iter()
                    .any(|(f, k)| *f >= duration || !p.accepts(k.value) || !k.interpolation.valid())
            {
                return Err("Invalid mask parameter or keyframe".into());
            }
        }
    }
    Ok(())
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::EditMask {
        id,
        mask,
        parameter,
        edit,
    } = command
    else {
        return None;
    };
    Some((|| {
        let duration = state.project.composition.duration;
        let layer = editing::editable(state, *id)?;
        let track = layer
            .path_masks
            .iter_mut()
            .find(|m| m.id == *mask)
            .and_then(|m| m.parameters.get_mut(parameter))
            .ok_or("Mask no longer exists")?;
        time_remap::edit_track(track, duration, edit, |v| parameter.accepts(v))
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        let path = VectorPath {
            closed: true,
            vertices: [[0.0, 0.0], [80.0, 0.0], [40.0, 80.0]]
                .map(PathVertex::corner)
                .to_vec(),
        };
        e.execute(Command::SetPathMasks {
            id: 1,
            masks: vec![
                PathMask {
                    path: path.clone(),
                    ..Default::default()
                },
                PathMask {
                    path,
                    ..Default::default()
                },
            ],
        })
        .unwrap();
        e
    }
    #[test]
    fn stable_masks_animate_reorder_shift_copy_and_roundtrip() {
        let mut e = scene();
        let path = PropertyPath::Mask {
            mask: 1,
            parameter: MaskParam::Feather,
        };
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
        let mut masks = e.selected_layer().unwrap().path_masks().to_vec();
        masks.reverse();
        e.execute(Command::SetPathMasks { id: 1, masks }).unwrap();
        assert_eq!(e.selected_layer().unwrap().path_masks()[1].id, 1);
        assert_eq!(
            e.selected_layer().unwrap().track_value(path, 10),
            Some(20.0)
        );
        let key = e.selected_layer().unwrap().copy_key(path, 20).unwrap();
        e.execute(Command::PasteKeys {
            keys: vec![key],
            frame: 40,
            target: None,
        })
        .unwrap();
        e.execute(Command::SetLayerRange {
            id: 1,
            start: 0,
            end: 100,
        })
        .unwrap();
        e.execute(Command::ShiftLayer { id: 1, delta: 5 }).unwrap();
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track(path)
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![5, 25, 45]
        );
        let saved = e.project().clone();
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        e.execute(Command::EditTrack {
            id: 1,
            property: path,
            edit: TrackEdit::Value {
                frame: 10,
                value: 300.0,
            },
        })
        .unwrap_err();
        assert_eq!(*e.project(), saved);
        let mut masks = e.selected_layer().unwrap().path_masks().to_vec();
        masks.retain(|m| m.id != 1);
        e.execute(Command::SetPathMasks { id: 1, masks }).unwrap();
        assert!(
            e.execute(Command::EditTrack {
                id: 1,
                property: path,
                edit: TrackEdit::Value {
                    frame: 10,
                    value: 10.0
                }
            })
            .is_err()
        );
        e.undo();
        assert_eq!(*e.project(), saved);
    }
    #[test]
    fn legacy_masks_migrate_once_without_changing_paths() {
        let e = scene();
        let mut json = serde_json::to_value(e.project()).unwrap();
        json["version"] = 29.into();
        let l = &mut json["composition"]["layers"][0];
        l.as_object_mut().unwrap().remove("next_mask_id");
        for m in l["path_masks"].as_array_mut().unwrap() {
            m.as_object_mut().unwrap().remove("id");
            m.as_object_mut().unwrap().remove("parameters");
        }
        let p = Project::from_json(&json.to_string()).unwrap();
        assert_eq!(p.version, 30);
        assert_eq!(
            p.composition().layers()[0].path_masks(),
            e.selected_layer().unwrap().path_masks()
        );
        assert_eq!(Project::from_json(&p.to_json().unwrap()).unwrap(), p);
    }
    #[test]
    fn compact_images_and_sequences_roundtrip_with_new_editor_features() {
        let mut e = scene();
        for content in [
            Content::Image { png: "YWJj".into() },
            Content::ImageSequence {
                frames: std::sync::Arc::new(vec!["one.png".into(), "two.png".into()]),
                fps: 30.into(),
                missing: MissingFramePolicy::Error,
                start_frame: 0,
                playback: Default::default(),
            },
        ] {
            e.execute(Command::AddContent {
                content,
                width: 100.0,
                height: 100.0,
                name: "Asset".into(),
            })
            .unwrap();
        }
        let json = e.project().to_json().unwrap();
        assert!(json.contains("image_assets") && json.contains("sequence_assets"));
        assert_eq!(Project::from_json(&json).unwrap(), *e.project());
        e.execute(Command::AnimatePath {
            id: 1,
            target: PathTarget::Mask(1),
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        assert_eq!(e.project().version, 31);
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
    }
}
