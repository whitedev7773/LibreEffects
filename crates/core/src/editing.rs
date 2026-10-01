use super::*;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Content {
    #[default]
    Rectangle,
    Text {
        text: String,
        font_size: f64,
    },
    Image {
        png: String,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    pub blur: f64,
    pub brightness: f64,
    pub grayscale: bool,
}
impl Default for Effects {
    fn default() -> Self {
        Self {
            blur: 0.0,
            brightness: 1.0,
            grayscale: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mask {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub inverted: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyRef {
    pub id: LayerId,
    pub property: Property,
    pub frame: Frame,
}
#[derive(Clone, Debug)]
pub struct KeyCopy {
    pub key: KeyRef,
    pub data: Keyframe,
}

pub(super) fn validate_content(
    content: &Content,
    effects: Effects,
    mask: Option<Mask>,
) -> Result<(), String> {
    let valid = match content {
        Content::Rectangle => true,
        Content::Text { text, font_size } => {
            text.len() <= 16384 && font_size.is_finite() && (1.0..=2048.0).contains(font_size)
        }
        Content::Image { png } => {
            !png.is_empty()
                && png.len() <= 12 * 1024 * 1024
                && png
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
        }
    };
    if !valid
        || !effects.blur.is_finite()
        || !(0.0..=100.0).contains(&effects.blur)
        || !effects.brightness.is_finite()
        || !(0.0..=4.0).contains(&effects.brightness)
        || mask.is_some_and(|m| {
            [m.x, m.y, m.width, m.height]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 16384.0)
                || m.width <= 0.0
                || m.height <= 0.0
        })
    {
        return Err("Invalid layer content, mask or effect settings".into());
    }
    Ok(())
}
fn editable(state: &mut Snapshot, id: LayerId) -> Result<&mut Layer, String> {
    let l = state
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == id)
        .ok_or("Layer not found")?;
    if l.locked {
        return Err("Unlock the layer before editing".into());
    }
    Ok(l)
}
fn shifted(frame: Frame, delta: i64, duration: Frame, endpoint: bool) -> Result<Frame, String> {
    let f = frame as i64 + delta;
    if f < 0 || f >= duration as i64 + i64::from(endpoint) {
        return Err("Move would leave the composition".into());
    }
    Ok(f as Frame)
}
pub(super) fn apply_extended(
    state: &mut Snapshot,
    command: &Command,
) -> Option<Result<(), String>> {
    if !matches!(
        command,
        Command::Batch(_)
            | Command::DuplicateLayers(_)
            | Command::SplitLayers { .. }
            | Command::SetAnchor { .. }
            | Command::AddContent { .. }
            | Command::SetContent { .. }
            | Command::SetEffects { .. }
            | Command::SetMask { .. }
            | Command::SetColor { .. }
            | Command::ShiftLayer { .. }
            | Command::MoveKeys { .. }
            | Command::DeleteKeys(_)
            | Command::PasteKeys { .. }
    ) {
        return None;
    }
    Some((|| {
        match command {
            Command::DuplicateLayers(ids) | Command::SplitLayers { ids, .. } => {
                let ids: BTreeSet<_> = ids.iter().copied().collect();
                if ids.is_empty() {
                    return Err("Select a layer first".into());
                }
                let duration = state.project.composition.duration;
                for id in &ids {
                    let layer = editable(state, *id)?;
                    if let Command::SplitLayers { frame, .. } = command {
                        if *frame <= layer.in_frame || *frame >= layer.out_frame(duration) {
                            return Err(
                                "Place the playhead inside every selected layer to split".into()
                            );
                        }
                    }
                }
                if state.project.composition.layers.len() + ids.len() > 1000
                    || state
                        .project
                        .next_layer_id
                        .checked_add(ids.len() as u64)
                        .is_none_or(|n| n >= u64::MAX)
                {
                    return Err("Layer limit reached".into());
                }
                let originals: Vec<_> = state
                    .project
                    .composition
                    .layers
                    .iter()
                    .filter(|l| ids.contains(&l.id))
                    .cloned()
                    .collect();
                let mut mapping = BTreeMap::new();
                for layer in &originals {
                    mapping.insert(layer.id, state.project.next_layer_id);
                    state.project.next_layer_id += 1;
                }
                for mut copy in originals {
                    let index = state
                        .project
                        .composition
                        .layers
                        .iter()
                        .position(|l| l.id == copy.id)
                        .unwrap();
                    copy.id = mapping[&copy.id];
                    if let Command::SplitLayers { frame, .. } = command {
                        state.project.composition.layers[index].out_frame = Some(*frame);
                        copy.in_frame = *frame;
                    } else if copy.name.len() < 1000 {
                        copy.name.push_str(" copy");
                    }
                    copy.parent = copy.parent.map(|p| mapping.get(&p).copied().unwrap_or(p));
                    state.selected = Some(copy.id);
                    state.project.composition.layers.insert(index, copy);
                }
            }
            Command::SetAnchor { id, frame, x, y } => {
                let layer = editable(state, *id)?;
                let old = [
                    layer.property(Property::AnchorX).value_at(*frame),
                    layer.property(Property::AnchorY).value_at(*frame),
                ];
                // Compensate in position-property space so the rendered layer stays still,
                // including when it has a rotated/scaled parent or a parenting offset.
                let delta = layer
                    .local_transform(*frame)
                    .vector([x - old[0], y - old[1]]);
                let position = [
                    layer.property(Property::PositionX).value_at(*frame) + delta[0],
                    layer.property(Property::PositionY).value_at(*frame) + delta[1],
                ];
                apply(
                    state,
                    Command::SetValue {
                        id: *id,
                        property: Property::AnchorX,
                        frame: *frame,
                        value: *x,
                    },
                )?;
                apply(
                    state,
                    Command::SetValue {
                        id: *id,
                        property: Property::AnchorY,
                        frame: *frame,
                        value: *y,
                    },
                )?;
                apply(
                    state,
                    Command::SetPosition {
                        id: *id,
                        frame: *frame,
                        x: position[0],
                        y: position[1],
                    },
                )?;
            }
            Command::Batch(commands) => {
                if commands.len() > 10000 {
                    return Err("Too many edits".into());
                }
                for c in commands {
                    apply(state, c.clone())?;
                }
            }
            Command::AddContent {
                content,
                width,
                height,
                name,
            } => {
                validate_content(content, Effects::default(), None)?;
                apply(state, Command::AddRectangle)?;
                let l = editable(state, state.selected.unwrap())?;
                l.content = content.clone();
                l.width = *width;
                l.height = *height;
                l.name = name.clone();
                l.color = 0xffffff;
                l.properties.get_mut(&Property::AnchorX).unwrap().value = width / 2.0;
                l.properties.get_mut(&Property::AnchorY).unwrap().value = height / 2.0;
            }
            Command::SetContent { id, content } => editable(state, *id)?.content = content.clone(),
            Command::SetEffects { id, effects } => editable(state, *id)?.effects = *effects,
            Command::SetMask { id, mask } => editable(state, *id)?.mask = *mask,
            Command::SetColor { id, color } => editable(state, *id)?.color = *color,
            Command::ShiftLayer { id, delta } => {
                let duration = state.project.composition.duration;
                let l = editable(state, *id)?;
                let end = l.out_frame(duration);
                l.in_frame = shifted(l.in_frame, *delta, duration, false)?;
                l.out_frame = Some(shifted(end, *delta, duration, true)?);
                for track in l.properties.values_mut() {
                    track.keys = track
                        .keys
                        .iter()
                        .map(|(f, k)| Ok((shifted(*f, *delta, duration, false)?, k.clone())))
                        .collect::<Result<_, String>>()?;
                }
            }
            Command::MoveKeys { keys, delta } => {
                let duration = state.project.composition.duration;
                let mut removed = Vec::new();
                for key in keys.iter().copied().collect::<BTreeSet<_>>() {
                    let track = editable(state, key.id)?
                        .properties
                        .get_mut(&key.property)
                        .unwrap();
                    let data = track
                        .keys
                        .remove(&key.frame)
                        .ok_or("Selected key no longer exists")?;
                    removed.push((key, data));
                }
                for (key, data) in removed {
                    let to = shifted(key.frame, *delta, duration, false)?;
                    let track = editable(state, key.id)?
                        .properties
                        .get_mut(&key.property)
                        .unwrap();
                    if track.keys.contains_key(&to) {
                        return Err("Destination already contains a key".into());
                    }
                    track.keys.insert(to, data);
                }
            }
            Command::DeleteKeys(keys) => {
                for key in keys.iter().copied().collect::<BTreeSet<_>>() {
                    let track = editable(state, key.id)?
                        .properties
                        .get_mut(&key.property)
                        .unwrap();
                    let value = track.value_at(key.frame);
                    track
                        .keys
                        .remove(&key.frame)
                        .ok_or("Selected key no longer exists")?;
                    if track.keys.is_empty() {
                        track.value = value;
                    }
                }
            }
            Command::PasteKeys {
                keys,
                frame,
                target,
            } => {
                let first = keys
                    .iter()
                    .map(|k| k.key.frame)
                    .min()
                    .ok_or("Copy keyframes first")?;
                let duration = state.project.composition.duration;
                for key in keys {
                    let to = shifted(key.key.frame, *frame as i64 - first as i64, duration, false)?;
                    let track = editable(state, target.unwrap_or(key.key.id))?
                        .properties
                        .get_mut(&key.key.property)
                        .unwrap();
                    if track.keys.contains_key(&to) {
                        return Err("Paste would overwrite a keyframe".into());
                    }
                    track.keys.insert(to, key.data.clone());
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    })())
}
