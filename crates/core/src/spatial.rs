//! Authoritative joined XYZ source and explicit native editing contracts.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum SpatialEdit {
    Value([f64; 3]),
    Key {
        frame: Frame,
        value: [f64; 3],
    },
    RemoveKey {
        frame: Frame,
    },
    Interpolation {
        frame: Frame,
        incoming: SpatialInterpolation,
        outgoing: SpatialInterpolation,
    },
    TemporalEase {
        frame: Frame,
        incoming: SpatialEase,
        outgoing: SpatialEase,
    },
    TemporalContinuous {
        frame: Frame,
        value: bool,
    },
    TemporalAutoBezier {
        frame: Frame,
        value: bool,
    },
    Tangents {
        frame: Frame,
        incoming: [f64; 3],
        outgoing: [f64; 3],
    },
    SpatialContinuous {
        frame: Frame,
        value: bool,
    },
    SpatialAutoBezier {
        frame: Frame,
        value: bool,
    },
}

pub(super) fn position_axis(property: Property) -> bool {
    matches!(property, Property::PositionX | Property::PositionY)
}
impl Layer {
    pub fn is_three_d(&self) -> bool {
        self.spatial_position.is_some()
    }
    pub fn spatial_position(&self) -> Option<&SpatialPosition3> {
        self.spatial_position.as_ref()
    }
    pub fn position3_at(&self, frame: Frame, seconds_per_frame: f64) -> Result<[f64; 3], String> {
        self.spatial_position
            .as_ref()
            .ok_or("Layer has no joined XYZ Position")?
            .sample(f64::from(frame), seconds_per_frame)
            .map_err(|error| error.to_string())
    }
}
impl Composition {
    pub fn has_spatial_layers(&self) -> bool {
        self.layers.iter().any(Layer::is_three_d)
    }
    pub fn camera(&self) -> Option<&Camera3> {
        self.camera.as_ref()
    }
}
impl Project {
    /// Final source transaction check, separate from intermediate metadata edits.
    pub fn validate_spatial_animation(&self) -> Result<(), String> {
        for (_, comp) in self.compositions() {
            for layer in &comp.layers {
                if let Some(position) = &layer.spatial_position {
                    position
                        .validate_sampling(comp.fps.seconds(1))
                        .map_err(|error| format!("{} Position: {error}", layer.name))?;
                }
            }
        }
        Ok(())
    }
}
pub(super) fn materialized(project: &Project) -> bool {
    project
        .compositions()
        .into_iter()
        .any(|(_, comp)| comp.camera.is_some() || comp.has_spatial_layers())
}
pub(super) fn validate_layer(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    let Some(position) = &layer.spatial_position else {
        return Ok(());
    };
    if version < 72 {
        return Err("Joined spatial Position requires project version 72".into());
    }
    if layer
        .properties
        .keys()
        .any(|property| position_axis(*property))
    {
        return Err("Joined XYZ Position cannot coexist with scalar X/Y source tracks".into());
    }
    position.validate().map_err(|error| error.to_string())?;
    if position
        .value
        .iter()
        .chain(position.keys.values().flat_map(|key| {
            key.value
                .iter()
                .chain(&key.in_tangent)
                .chain(&key.out_tangent)
        }))
        .any(|v| v.abs() > 1e9)
    {
        return Err("Spatial coordinates and tangent offsets must be within ±1e9".into());
    }
    if position.keys.keys().any(|frame| *frame >= duration) {
        return Err("Spatial keys must be inside the composition".into());
    }
    if !matches!(
        layer.content,
        Content::Rectangle | Content::Solid | Content::Text { .. } | Content::Null
    ) || layer.transform_offset != Affine::default()
        || layer.mask.is_some()
        || !layer.path_masks.is_empty()
        || layer.track_matte.is_some()
        || layer.effects != Effects::default()
        || layer
            .effect_stack
            .iter()
            .any(|effect| effect.kind() != EffectKind::SliderControl)
        || !layer.blend_mode.is_normal()
    {
        return Err("Spatial planes currently support plain Rectangle, Solid, Text or Null with identity compensation, normal blending and no rendering effects/masks/mattes".into());
    }
    Ok(())
}
pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::SetThreeD { .. }
        | Command::SetSpatialPosition { .. }
        | Command::SetSpatialParent { .. }
        | Command::SetCamera { .. } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}
pub(super) fn guard(state: &Snapshot, command: &Command) -> Result<(), String> {
    let comp = &state.project.composition;
    let joined = |id| comp.layer(id).is_some_and(Layer::is_three_d);
    match command {
        Command::SetValue { id, property, .. }
        | Command::ToggleKeyframe { id, property, .. }
        | Command::SetInterpolation { id, property, .. }
            if joined(*id) && position_axis(*property) =>
        {
            Err("Joined XYZ Position requires spatial commands, not scalar X/Y edits".into())
        }
        Command::SetPosition { id, .. } | Command::AlignLayer { id, .. } if joined(*id) => {
            Err("Spatial geometry requires an explicit joined Position edit".into())
        }
        Command::SetParent { id, parent, .. } if joined(*id) || parent.is_some_and(joined) => {
            Err("Use explicit local spatial parenting; 2D compensation is unsupported".into())
        }
        Command::RemoveLayer(id)
            if comp
                .layers
                .iter()
                .any(|layer| layer.parent == Some(*id) && layer.is_three_d()) =>
        {
            Err(
                "Remove or explicitly reparent spatial children before deleting their parent"
                    .into(),
            )
        }
        _ => Ok(()),
    }
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    if !matches!(
        command,
        Command::SetThreeD { .. }
            | Command::SetSpatialPosition { .. }
            | Command::SetSpatialParent { .. }
            | Command::SetCamera { .. }
    ) {
        return None;
    }
    Some((|| {
        let duration = state.project.composition.duration;
        match command {
            Command::SetCamera { camera } => {
                if let Some(camera) = camera {
                    camera.validate()?;
                }
                state.project.composition.camera = camera.clone();
            }
            Command::SetThreeD { id, enabled } => {
                let comp = &state.project.composition;
                let before = comp.layer(*id).ok_or("Layer not found")?;
                if before.locked {
                    return Err("Unlock the layer before editing".into());
                }
                if before.is_three_d() == *enabled {
                    return Ok(());
                }
                if before.parent.is_some()
                    || comp.layers.iter().any(|layer| layer.parent == Some(*id))
                {
                    return Err(
                        "Detach the layer hierarchy before changing dimensional mode".into(),
                    );
                }
                if *enabled {
                    let x = before
                        .property(Property::PositionX)
                        .ok_or("Missing X source")?;
                    let y = before
                        .property(Property::PositionY)
                        .ok_or("Missing Y source")?;
                    if !x.keys.is_empty() || !y.keys.is_empty() || !before.expressions.is_empty() {
                        return Err(
                            "Native 3D conversion requires static X/Y and no expression programs"
                                .into(),
                        );
                    }
                    let position = SpatialPosition3::new([x.value, y.value, 0.]);
                    let layer = editing::editable(state, *id)?;
                    layer.properties.remove(&Property::PositionX);
                    layer.properties.remove(&Property::PositionY);
                    layer.spatial_position = Some(position);
                    validate_layer(layer, duration, 72)?;
                } else {
                    let position = before.spatial_position.as_ref().unwrap();
                    if !position.keys.is_empty()
                        || position.value[2] != 0.
                        || !Property::PositionX.accepts(position.value[0])
                        || !Property::PositionY.accepts(position.value[1])
                    {
                        return Err("Native 2D conversion requires an unkeyed, zero-Z Position in 2D bounds".into());
                    }
                    let [x, y, _] = position.value;
                    let layer = editing::editable(state, *id)?;
                    layer
                        .properties
                        .insert(Property::PositionX, AnimatedProperty::new(x));
                    layer
                        .properties
                        .insert(Property::PositionY, AnimatedProperty::new(y));
                    layer.spatial_position = None;
                }
            }
            Command::SetSpatialParent { id, parent } => {
                let comp = &state.project.composition;
                if !comp.can_parent(*id, *parent) {
                    return Err("Invalid spatial parent or cycle".into());
                }
                if !comp.layer(*id).is_some_and(Layer::is_three_d)
                    || parent.is_some_and(|id| !comp.layer(id).is_some_and(Layer::is_three_d))
                {
                    return Err("Spatial parenting requires joined XYZ layers on both sides".into());
                }
                editing::editable(state, *id)?.parent = *parent;
            }
            Command::SetSpatialPosition { id, edit } => {
                let layer = editing::editable(state, *id)?;
                let mut position = layer
                    .spatial_position
                    .clone()
                    .ok_or("Enable joined XYZ Position first")?;
                match edit {
                    SpatialEdit::Value(value) => {
                        if !position.keys.is_empty() {
                            return Err(
                                "Static assignment to animated Position is unsupported".into()
                            );
                        }
                        position.value = *value;
                    }
                    SpatialEdit::Key { frame, value } => {
                        if *frame >= duration {
                            return Err("Spatial key is outside composition".into());
                        }
                        position
                            .keys
                            .entry(*frame)
                            .and_modify(|key| key.value = *value)
                            .or_insert_with(|| SpatialKey3::new(*value));
                    }
                    SpatialEdit::RemoveKey { frame } => {
                        if position.keys.len() == 1 && position.keys.contains_key(frame) {
                            return Err("Removing the final spatial key requires an explicit static collapse".into());
                        }
                        position.keys.remove(frame).ok_or("Spatial key not found")?;
                    }
                    _ => {
                        let frame = match edit {
                            SpatialEdit::Interpolation { frame, .. }
                            | SpatialEdit::TemporalEase { frame, .. }
                            | SpatialEdit::TemporalContinuous { frame, .. }
                            | SpatialEdit::TemporalAutoBezier { frame, .. }
                            | SpatialEdit::Tangents { frame, .. }
                            | SpatialEdit::SpatialContinuous { frame, .. }
                            | SpatialEdit::SpatialAutoBezier { frame, .. } => *frame,
                            _ => unreachable!(),
                        };
                        let key = position
                            .keys
                            .get_mut(&frame)
                            .ok_or("Spatial key not found")?;
                        match edit {
                            SpatialEdit::Interpolation {
                                incoming, outgoing, ..
                            } => {
                                key.in_interpolation = incoming.clone();
                                key.out_interpolation = outgoing.clone();
                            }
                            SpatialEdit::TemporalEase {
                                incoming, outgoing, ..
                            } => {
                                key.in_ease = incoming.clone();
                                key.out_ease = outgoing.clone();
                            }
                            SpatialEdit::TemporalContinuous { value, .. } => {
                                key.temporal_continuous = *value
                            }
                            SpatialEdit::TemporalAutoBezier { value, .. } => {
                                key.temporal_auto_bezier = *value
                            }
                            SpatialEdit::Tangents {
                                incoming, outgoing, ..
                            } => {
                                key.in_tangent = *incoming;
                                key.out_tangent = *outgoing;
                            }
                            SpatialEdit::SpatialContinuous { value, .. } => {
                                key.spatial_continuous = *value
                            }
                            SpatialEdit::SpatialAutoBezier { value, .. } => {
                                key.spatial_auto_bezier = *value
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                position.validate().map_err(|error| error.to_string())?;
                layer.spatial_position = Some(position);
            }
            _ => unreachable!(),
        }
        if materialized(&state.project) {
            state.project.version = state.project.version.max(72);
        }
        Ok(())
    })())
}
