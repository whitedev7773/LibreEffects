//! Joined XY source independent of the layer's 3D/camera mode.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum PlanarEdit {
    Value([f64; 2]),
    Key {
        frame: Frame,
        value: [f64; 2],
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
        incoming: [f64; 2],
        outgoing: [f64; 2],
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

impl Layer {
    pub fn planar_position(&self) -> Option<&SpatialPosition2> {
        self.planar_position.as_ref()
    }
    pub fn has_joined_position(&self) -> bool {
        self.planar_position.is_some() || self.is_three_d()
    }
    /// Authored XY sample; callers must pass the owning composition's timebase.
    pub fn position2_at(&self, frame: Frame, seconds_per_frame: f64) -> Result<[f64; 2], String> {
        self.position2_sample(f64::from(frame), seconds_per_frame)
    }
    /// Fractional authored XY sample, preserving the joined track's own timing.
    pub fn position2_sample(&self, frame: f64, seconds_per_frame: f64) -> Result<[f64; 2], String> {
        if !frame.is_finite() || frame < 0.0 || frame > f64::from(Frame::MAX) {
            return Err("Position sample must be inside the finite frame range".into());
        }
        if let Some(track) = &self.planar_position {
            return track
                .sample(frame, seconds_per_frame)
                .map_err(|e| e.to_string());
        }
        let axis = |property| {
            self.properties
                .get(&property)
                .map(|track| track.sample(frame))
                .ok_or("Layer has no scalar XY Position")
        };
        Ok([axis(Property::PositionX)?, axis(Property::PositionY)?])
    }
}

#[cfg(test)]
mod fractional_sample_tests {
    use super::*;

    #[test]
    fn fractional_xy_samples_both_scalar_and_joined_sources_without_mutation() {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let mut layer = editor.project().composition().layers()[0].clone();
        for (property, first, last) in [
            (Property::PositionX, 0.0, 100.0),
            (Property::PositionY, 10.0, -10.0),
        ] {
            let track = layer.properties.get_mut(&property).unwrap();
            track.keys = [(0, first), (10, last)]
                .map(|(frame, value)| {
                    (
                        frame,
                        Keyframe {
                            temporal: TemporalHandles::default(),
                            value,
                            interpolation: Interpolation::Linear,
                        },
                    )
                })
                .into();
        }
        let scalar = layer.clone();
        assert_eq!(
            layer.position2_sample(2.5, 1.0 / 60.0).unwrap(),
            [25.0, 5.0]
        );
        assert_eq!(layer.position2_at(2, 1.0 / 60.0).unwrap(), [20.0, 6.0]);
        assert_eq!(layer, scalar);

        layer.properties.remove(&Property::PositionX);
        layer.properties.remove(&Property::PositionY);
        layer.planar_position = Some(SpatialPosition2 {
            value: None,
            keys: [
                (0, SpatialKey2::new([0.0, 10.0])),
                (10, SpatialKey2::new([100.0, -10.0])),
            ]
            .into(),
        });
        let joined = layer.clone();
        let value = layer.position2_sample(2.5, 1.0 / 60.0).unwrap();
        // The joined spatial sampler guarantees 1e-7 of control-polygon
        // length in distance, rather than bit-exact linear interpolation.
        let tolerance = 1e-7 * 100.0_f64.hypot(20.0);
        assert!(
            (value[0] - 25.0).hypot(value[1] - 5.0) <= tolerance,
            "joined sample {value:?} exceeds its distance tolerance {tolerance}"
        );
        assert_eq!(layer.position2_at(10, 1.0 / 60.0).unwrap(), [100.0, -10.0]);
        assert_eq!(layer, joined);
        assert!(layer.position2_sample(f64::NAN, 1.0 / 60.0).is_err());
        assert!(layer.position2_sample(-0.5, 1.0 / 60.0).is_err());
    }
}
impl Project {
    pub fn validate_planar_animation(&self) -> Result<(), String> {
        for (_, comp) in self.compositions() {
            for layer in &comp.layers {
                if let Some(track) = &layer.planar_position {
                    track
                        .validate_sampling(comp.fps.seconds(1))
                        .map_err(|e| format!("{} Position XY: {e}", layer.name))?;
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
        .any(|(_, comp)| comp.layers.iter().any(|l| l.planar_position.is_some()))
}
pub(super) fn validate_layer(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    let Some(track) = &layer.planar_position else {
        return Ok(());
    };
    if version < 75 {
        return Err("Joined planar Position requires project version 75".into());
    }
    if layer.is_three_d() || layer.properties.keys().any(|p| spatial::position_axis(*p)) {
        return Err(
            "Joined XY Position cannot coexist with XYZ or scalar X/Y source tracks".into(),
        );
    }
    track.validate().map_err(|e| e.to_string())?;
    if track
        .value
        .iter()
        .flatten()
        .chain(
            track
                .keys
                .values()
                .flat_map(|k| k.value.iter().chain(&k.in_tangent).chain(&k.out_tangent)),
        )
        .any(|v| v.abs() > 1_000_000.)
    {
        return Err("Planar coordinates and tangent offsets must be within ±1000000".into());
    }
    if track.keys.keys().any(|f| *f >= duration) {
        return Err("Planar keys must be inside the composition".into());
    }
    Ok(())
}
pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::SetPlanarPosition { .. }
        | Command::EditPlanarPosition { .. }
        | Command::SetPlanarParent { .. } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}
pub(super) fn guard(state: &Snapshot, command: &Command) -> Result<(), String> {
    let planar = |id| {
        state
            .project
            .composition
            .layer(id)
            .is_some_and(|l| l.planar_position.is_some())
    };
    match command {
        Command::SetValue { id, property, .. }
        | Command::ToggleKeyframe { id, property, .. }
        | Command::SetInterpolation { id, property, .. }
        | Command::ToggleAnimation { id, property, .. }
        | Command::MoveKeyframe { id, property, .. }
            if planar(*id) && spatial::position_axis(*property) =>
        {
            Err("Joined XY Position requires vector edits, not scalar X/Y edits".into())
        }
        Command::SetPosition { id, .. }
        | Command::AlignLayer { id, .. }
        | Command::SetAnchor { id, .. }
            if planar(*id) =>
        {
            Err("Joined XY geometry requires an explicit vector Position edit".into())
        }
        Command::SetThreeD { id, enabled: true } if planar(*id) => Err(
            "Changing joined XY to XYZ requires an explicit source-preserving conversion".into(),
        ),
        _ => Ok(()),
    }
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    if !matches!(
        command,
        Command::SetPlanarPosition { .. }
            | Command::EditPlanarPosition { .. }
            | Command::SetPlanarParent { .. }
    ) {
        return None;
    }
    Some((|| {
        let duration = state.project.composition.duration;
        match command {
            Command::SetPlanarParent { id, parent } => {
                let comp = &state.project.composition;
                if !comp.can_parent(*id, *parent) {
                    return Err("Invalid planar parent or cycle".into());
                }
                if comp.layer(*id).ok_or("Layer not found")?.is_three_d()
                    || parent.is_some_and(|p| comp.layer(p).is_some_and(Layer::is_three_d))
                {
                    return Err("Local planar parenting requires 2D layers".into());
                }
                let layer = editing::editable(state, *id)?;
                layer.parent = *parent;
                layer.transform_offset = Affine::default();
            }
            Command::SetPlanarPosition { id, position } => {
                let layer = editing::editable(state, *id)?;
                if layer.is_three_d() {
                    return Err("Joined XY source cannot replace a 3D layer".into());
                }
                if layer.planar_position.is_none()
                    && [Property::PositionX, Property::PositionY]
                        .into_iter()
                        .any(|p| layer.properties.get(&p).is_some_and(|t| !t.keys.is_empty()))
                {
                    return Err(
                        "Replacing animated scalar X/Y requires an explicit conversion".into(),
                    );
                }
                position.validate().map_err(|e| e.to_string())?;
                layer.properties.remove(&Property::PositionX);
                layer.properties.remove(&Property::PositionY);
                layer.planar_position = Some(position.clone());
                validate_layer(layer, duration, 75)?;
            }
            Command::EditPlanarPosition { id, edit } => {
                let layer = editing::editable(state, *id)?;
                let mut track = layer
                    .planar_position
                    .clone()
                    .ok_or("Layer has no joined XY Position")?;
                match edit {
                    PlanarEdit::Value(v) => {
                        if !track.keys.is_empty() {
                            return Err(
                                "Static assignment to animated Position is unsupported".into()
                            );
                        }
                        track.value = Some(*v);
                    }
                    PlanarEdit::Key { frame, value } => {
                        if *frame >= duration {
                            return Err("Planar key is outside composition".into());
                        }
                        track
                            .keys
                            .entry(*frame)
                            .and_modify(|k| k.value = *value)
                            .or_insert_with(|| SpatialKey2::new(*value));
                    }
                    PlanarEdit::RemoveKey { frame } => {
                        if track.keys.len() == 1 && track.keys.contains_key(frame) {
                            return Err("Removing the final planar key requires an explicit static collapse".into());
                        }
                        track.keys.remove(frame).ok_or("Planar key not found")?;
                    }
                    _ => {
                        let frame = match edit {
                            PlanarEdit::Interpolation { frame, .. }
                            | PlanarEdit::TemporalEase { frame, .. }
                            | PlanarEdit::TemporalContinuous { frame, .. }
                            | PlanarEdit::TemporalAutoBezier { frame, .. }
                            | PlanarEdit::Tangents { frame, .. }
                            | PlanarEdit::SpatialContinuous { frame, .. }
                            | PlanarEdit::SpatialAutoBezier { frame, .. } => frame,
                            _ => unreachable!(),
                        };
                        let key = track.keys.get_mut(frame).ok_or("Planar key not found")?;
                        match edit {
                            PlanarEdit::Interpolation {
                                incoming, outgoing, ..
                            } => {
                                key.in_interpolation = *incoming;
                                key.out_interpolation = *outgoing;
                            }
                            PlanarEdit::TemporalEase {
                                incoming, outgoing, ..
                            } => {
                                key.in_ease = *incoming;
                                key.out_ease = *outgoing;
                            }
                            PlanarEdit::TemporalContinuous { value, .. } => {
                                key.temporal_continuous = *value
                            }
                            PlanarEdit::TemporalAutoBezier { value, .. } => {
                                key.temporal_auto_bezier = *value
                            }
                            PlanarEdit::Tangents {
                                incoming, outgoing, ..
                            } => {
                                key.in_tangent = *incoming;
                                key.out_tangent = *outgoing;
                            }
                            PlanarEdit::SpatialContinuous { value, .. } => {
                                key.spatial_continuous = *value
                            }
                            PlanarEdit::SpatialAutoBezier { value, .. } => {
                                key.spatial_auto_bezier = *value
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                track.validate().map_err(|e| e.to_string())?;
                layer.planar_position = Some(track);
                validate_layer(layer, duration, 75)?;
            }
            _ => unreachable!(),
        }
        if materialized(&state.project) {
            state.project.version = state.project.version.max(75);
        }
        Ok(())
    })())
}
