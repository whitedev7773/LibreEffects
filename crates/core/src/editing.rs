use super::*;

/// Source time at the video's origin and source seconds per composition second.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoPlayback {
    pub source_in: f64,
    pub speed: f64,
}
impl Default for VideoPlayback {
    fn default() -> Self {
        Self {
            source_in: 0.0,
            speed: 1.0,
        }
    }
}
impl VideoPlayback {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

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
    Video {
        path: String,
        duration: f64,
        source_fps: f64,
        /// Composition frame at which playback.source_in occurs; trimming never changes it.
        start_frame: i64,
        #[serde(default, skip_serializing_if = "VideoPlayback::is_default")]
        playback: VideoPlayback,
    },
}
impl Content {
    /// Unquantized source seconds, including times outside the source's range.
    pub fn video_source_time(&self, frame: Frame, fps: u32) -> Option<f64> {
        let Self::Video {
            start_frame,
            playback,
            ..
        } = self
        else {
            return None;
        };
        if fps == 0 {
            return None;
        }
        Some(
            playback.source_in + (frame as f64 - *start_frame as f64) / fps as f64 * playback.speed,
        )
    }
    pub fn video_time(&self, frame: Frame, fps: u32) -> Option<f64> {
        let Self::Video {
            duration,
            source_fps,
            ..
        } = self
        else {
            return None;
        };
        let seconds = self.video_source_time(frame, fps)?;
        // Reversing an interval can land a few ulps below zero on its last frame.
        let seconds = if (-1e-9..0.0).contains(&seconds) {
            0.0
        } else {
            seconds
        };
        (seconds >= 0.0 && seconds < *duration)
            .then(|| ((seconds * source_fps + 1e-7).floor() / source_fps).max(0.0))
    }
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
        Content::Video {
            path,
            duration,
            start_frame,
            source_fps,
            playback,
        } => {
            !path.is_empty()
                && source_fps.is_finite()
                && (1.0..=240.0).contains(source_fps)
                && path.len() <= 32768
                && !path.contains('\0')
                && duration.is_finite()
                && (0.0..=86400.0).contains(duration)
                && *duration > 0.0
                && start_frame.abs_diff(0) <= 100_000_000
                && playback.source_in.is_finite()
                && playback.source_in.abs() <= 8_640_000.0
                && playback.speed.is_finite()
                && (playback.speed == 0.0 || (0.01..=100.0).contains(&playback.speed.abs()))
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
            | Command::AddBackgroundSolid
            | Command::TrimLayers { .. }
            | Command::NudgeLayers { .. }
            | Command::DuplicateLayers(_)
            | Command::SplitLayers { .. }
            | Command::SetAnchor { .. }
            | Command::AddContent { .. }
            | Command::SetContent { .. }
            | Command::SetVideoSpeed { .. }
            | Command::SetVideoSourceIn { .. }
            | Command::ReverseVideo { .. }
            | Command::FreezeVideo { .. }
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
            Command::AddBackgroundSolid => {
                let comp = &state.project.composition;
                let (width, height, color) =
                    (comp.width as f64, comp.height as f64, comp.background_color);
                apply(
                    state,
                    Command::AddContent {
                        content: Content::Rectangle,
                        width,
                        height,
                        name: "Background".into(),
                    },
                )?;
                let id = state.selected.unwrap();
                apply(state, Command::SetColor { id, color })?;
                let index = state.project.composition.layers.len() - 1;
                apply(state, Command::MoveLayer { id, index })?;
            }
            Command::SetVideoSpeed { id, .. }
            | Command::SetVideoSourceIn { id, .. }
            | Command::ReverseVideo { id }
            | Command::FreezeVideo { id, .. } => {
                let comp = &state.project.composition;
                let (fps, duration) = (comp.fps, comp.duration);
                let layer = editable(state, *id)?;
                let start = layer.in_frame;
                let end = layer.out_frame(duration);
                let Content::Video {
                    playback,
                    duration: source_duration,
                    ..
                } = layer.content
                else {
                    return Err("Select a video layer first".into());
                };
                let mut next = playback;
                next.source_in = layer.content.video_source_time(start, fps).unwrap();
                match command {
                    Command::SetVideoSpeed { speed, .. } => {
                        if !speed.is_finite()
                            || (*speed != 0.0 && !(0.01..=100.0).contains(&speed.abs()))
                        {
                            return Err(
                                "Speed must be 0%, or between 1% and 10000% in either direction"
                                    .into(),
                            );
                        }
                        next.speed = *speed;
                    }
                    Command::SetVideoSourceIn { seconds, .. } => {
                        if !seconds.is_finite() || *seconds < 0.0 || *seconds >= source_duration {
                            return Err("Source In must be inside the video, in seconds".into());
                        }
                        next.source_in = *seconds;
                    }
                    Command::ReverseVideo { .. } => {
                        if layer.content.video_time(start, fps).is_none()
                            || layer.content.video_time(end - 1, fps).is_none()
                        {
                            return Err(
                                "Trim the layer to valid source frames before reversing".into()
                            );
                        }
                        // Reverse the visible discrete interval exactly, including mixed frame rates.
                        next.source_in = layer.content.video_source_time(end - 1, fps).unwrap();
                        next.speed = -playback.speed;
                    }
                    Command::FreezeVideo { frame, .. } => {
                        if *frame < start || *frame >= end {
                            return Err(
                                "Place the playhead inside the video layer to freeze".into()
                            );
                        }
                        next.source_in = layer
                            .content
                            .video_time(*frame, fps)
                            .ok_or("There is no source frame at the playhead")?;
                        next.speed = 0.0;
                    }
                    _ => unreachable!(),
                }
                let Content::Video {
                    start_frame,
                    playback,
                    ..
                } = &mut layer.content
                else {
                    unreachable!()
                };
                *start_frame = start as i64;
                *playback = next;
            }
            Command::TrimLayers { ids, frame, start } => {
                let duration = state.project.composition.duration;
                if *frame >= duration || ids.is_empty() {
                    return Err("Select layers and a frame inside the composition".into());
                }
                for id in ids.iter().copied().collect::<BTreeSet<_>>() {
                    let layer = editable(state, id)?;
                    let (a, b) = if *start {
                        (*frame, layer.out_frame(duration))
                    } else {
                        (layer.in_frame, frame + 1)
                    };
                    apply(
                        state,
                        Command::SetLayerRange {
                            id,
                            start: a,
                            end: b,
                        },
                    )?;
                }
            }
            Command::NudgeLayers { ids, frame, delta } => {
                let ids: BTreeSet<_> = ids.iter().copied().collect();
                if ids.is_empty() {
                    return Err("Select a layer first".into());
                }
                for id in &ids {
                    editable(state, *id)?;
                }
                let comp = &state.project.composition;
                let commands: Result<Vec<_>, String> = ids
                    .iter()
                    .filter(|id| {
                        !ids.iter()
                            .any(|parent| parent != *id && !comp.can_parent(*parent, Some(**id)))
                    })
                    .map(|id| {
                        let layer = comp.layer(*id).ok_or("Layer not found")?;
                        let space = comp
                            .position_space(*id, *frame)
                            .and_then(Affine::inverse)
                            .ok_or("Layer transform cannot be inverted")?;
                        let d = space.vector(*delta);
                        Ok(Command::SetPosition {
                            id: *id,
                            frame: *frame,
                            x: layer.property(Property::PositionX).value_at(*frame) + d[0],
                            y: layer.property(Property::PositionY).value_at(*frame) + d[1],
                        })
                    })
                    .collect();
                for command in commands? {
                    apply(state, command)?;
                }
            }
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
                let comp = &state.project.composition;
                let (fps, duration, cw, ch) = (comp.fps, comp.duration, comp.width, comp.height);
                let l = editable(state, state.selected.unwrap())?;
                l.content = content.clone();
                l.width = *width;
                l.height = *height;
                l.name = name.clone();
                l.color = 0xffffff;
                l.properties.get_mut(&Property::AnchorX).unwrap().value = width / 2.0;
                l.properties.get_mut(&Property::AnchorY).unwrap().value = height / 2.0;
                if let Content::Video {
                    duration: seconds,
                    start_frame,
                    ..
                } = content
                {
                    if *start_frame < 0 || *start_frame >= duration as i64 {
                        return Err("Import video inside the composition".into());
                    }
                    l.in_frame = *start_frame as u32;
                    l.out_frame = Some(
                        (l.in_frame as u64 + (seconds * fps as f64).ceil() as u64)
                            .min(duration as u64) as u32,
                    );
                    let scale = (cw as f64 / width).min(ch as f64 / height).min(1.0) * 100.0;
                    l.properties.get_mut(&Property::ScaleX).unwrap().value = scale;
                    l.properties.get_mut(&Property::ScaleY).unwrap().value = scale;
                }
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
                if let Content::Video { start_frame, .. } = &mut l.content {
                    *start_frame = start_frame
                        .checked_add(*delta)
                        .ok_or("Video timing overflow")?;
                }
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
