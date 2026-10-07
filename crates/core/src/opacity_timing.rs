//! Exact per-side Opacity timing, with a single legacy value/key store.
use super::*;
use libre_effects_temporal_scalar as scalar;
use serde::de::{MapAccess, Visitor};
use std::fmt;

pub use scalar::{
    ScalarEase as OpacityEase, ScalarInterpolation as OpacityInterpolation,
    ScalarKeyTiming as OpacityKeyTiming,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpacityTiming {
    #[serde(deserialize_with = "unique_keys")]
    keys: BTreeMap<Frame, OpacityKeyTiming>,
}
impl OpacityTiming {
    pub fn keys(&self) -> &BTreeMap<Frame, OpacityKeyTiming> {
        &self.keys
    }
}
fn unique_keys<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<Frame, OpacityKeyTiming>, D::Error> {
    struct Keys;
    impl<'de> Visitor<'de> for Keys {
        type Value = BTreeMap<Frame, OpacityKeyTiming>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("unique bounded Opacity frame keys")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut keys = BTreeMap::new();
            while let Some((frame, key)) = map.next_entry::<Frame, OpacityKeyTiming>()? {
                if keys.len() >= 100_000 {
                    return Err(serde::de::Error::custom("Opacity key budget exceeded"));
                }
                if keys.insert(frame, key).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate numeric Opacity frame key",
                    ));
                }
            }
            Ok(keys)
        }
    }
    deserializer.deserialize_map(Keys)
}

#[cfg(test)]
mod fractional_sample_tests {
    use super::*;

    #[test]
    fn fractional_opacity_preserves_signed_ease_overshoot_and_dormant_sides() {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let mut layer = editor.project().composition().layers()[0].clone();
        layer.properties.get_mut(&Property::Opacity).unwrap().keys = [(0, 100.0), (100, 0.0)]
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
        assert_eq!(layer.opacity_sample(12.5, 0.01).unwrap(), 87.5);
        let mut first = OpacityKeyTiming::new();
        let mut last = OpacityKeyTiming::new();
        first.out_interpolation = OpacityInterpolation::Bezier;
        last.in_interpolation = OpacityInterpolation::Bezier;
        first.out_ease.speed = 120.0;
        last.in_ease.speed = 120.0;
        first.in_interpolation = OpacityInterpolation::Hold;
        first.in_ease.speed = -765.0;
        last.out_interpolation = OpacityInterpolation::Hold;
        last.out_ease.speed = -432.0;
        layer.opacity_timing = Some(OpacityTiming {
            keys: [(0, first), (100, last)].into(),
        });
        let before = layer.clone();
        // Linear x handles and signed y handles [100, 140, -40, 0] give this
        // independent cubic value at u=1/8; paint must clamp only downstream.
        let value = layer.opacity_sample(12.5, 0.01).unwrap();
        assert!((value - 105.546875).abs() < 1e-10);
        assert!(value > 100.0);
        assert_eq!(layer.opacity_at(0, 0.01).unwrap(), 100.0);
        assert_eq!(layer.opacity_at(100, 0.01).unwrap(), 0.0);
        assert_eq!(layer, before);
        layer
            .opacity_timing
            .as_mut()
            .unwrap()
            .keys
            .get_mut(&0)
            .unwrap()
            .out_interpolation = OpacityInterpolation::Hold;
        assert_eq!(layer.opacity_sample(99.75, 0.01).unwrap(), 100.0);
        assert_eq!(layer.opacity_sample(100.0, 0.01).unwrap(), 0.0);
        assert!(layer.opacity_sample(f64::INFINITY, 0.01).is_err());
        assert!(layer.opacity_sample(-0.5, 0.01).is_err());
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum OpacityEdit {
    Value(f64),
    /// Explicitly replace animation with a static authored value. Undo retains
    /// every original key and dormant side; removing a key never implies this.
    Collapse {
        value: f64,
    },
    Key {
        frame: Frame,
        value: f64,
    },
    RemoveKey {
        frame: Frame,
    },
    Interpolation {
        frame: Frame,
        incoming: OpacityInterpolation,
        outgoing: OpacityInterpolation,
    },
    TemporalEase {
        frame: Frame,
        incoming: OpacityEase,
        outgoing: OpacityEase,
    },
    TemporalContinuous {
        frame: Frame,
        value: bool,
    },
    TemporalAutoBezier {
        frame: Frame,
        value: bool,
    },
}

#[cfg(test)]
mod collapse_tests {
    use super::*;
    #[test]
    fn explicit_static_collapse_preserves_appearance_source_metadata_and_exact_undo() {
        let mut editor = Editor::default();
        editor.execute(Command::AddSolid).unwrap();
        for edit in [
            OpacityEdit::Key {
                frame: 0,
                value: 50.0,
            },
            OpacityEdit::Key {
                frame: 30,
                value: 50.0,
            },
            OpacityEdit::Interpolation {
                frame: 0,
                incoming: OpacityInterpolation::Linear,
                outgoing: OpacityInterpolation::Bezier,
            },
            OpacityEdit::Interpolation {
                frame: 30,
                incoming: OpacityInterpolation::Bezier,
                outgoing: OpacityInterpolation::Linear,
            },
            OpacityEdit::TemporalEase {
                frame: 0,
                incoming: OpacityEase {
                    speed: -1e-300,
                    influence: 23.0,
                },
                outgoing: OpacityEase {
                    speed: 100.0,
                    influence: 100.0 / 3.0,
                },
            },
            OpacityEdit::TemporalEase {
                frame: 30,
                incoming: OpacityEase {
                    speed: -100.0,
                    influence: 100.0 / 3.0,
                },
                outgoing: OpacityEase {
                    speed: 2e-199,
                    influence: 79.0,
                },
            },
        ] {
            editor
                .execute(Command::SetOpacityTiming { id: 1, edit })
                .unwrap();
        }
        editor.clear_history();
        let before = project_file::encode(editor.project(), None).unwrap();
        let source = editor.selected_layer().unwrap().clone();
        let sample = source.opacity_at(15, 1.0 / 30.0).unwrap();
        assert!((sample - 75.0).abs() < 1e-12);
        for value in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
            assert!(
                editor
                    .execute(Command::SetOpacityTiming {
                        id: 1,
                        edit: OpacityEdit::Collapse { value }
                    })
                    .is_err()
            );
            assert_eq!(
                project_file::encode(editor.project(), None).unwrap(),
                before
            );
        }
        editor
            .execute(Command::SetOpacityTiming {
                id: 1,
                edit: OpacityEdit::Collapse { value: sample },
            })
            .unwrap();
        let layer = editor.selected_layer().unwrap();
        assert!(!layer.has_opacity_timing());
        assert_eq!(layer.opacity_key_count(), 0);
        assert_eq!(
            layer.property(Property::Opacity).unwrap().value_at(0),
            sample
        );
        assert_eq!(layer.color(), source.color());
        assert_eq!(layer.content(), source.content());
        for frame in [0, 15, 30, 100] {
            assert_eq!(layer.opacity_at(frame, 1.0 / 30.0).unwrap(), sample);
        }
        let after = project_file::encode(editor.project(), None).unwrap();
        assert_eq!(
            project_file::decode(&after).unwrap().project,
            *editor.project()
        );
        editor.undo();
        assert_eq!(
            project_file::encode(editor.project(), None).unwrap(),
            before
        );
        editor.redo();
        assert_eq!(project_file::encode(editor.project(), None).unwrap(), after);
    }
}
impl Layer {
    pub fn has_opacity_timing(&self) -> bool {
        self.opacity_timing.is_some()
    }
    pub fn opacity_timing(&self) -> Option<&OpacityTiming> {
        self.opacity_timing.as_ref()
    }
    pub fn opacity_key_count(&self) -> usize {
        self.properties
            .get(&Property::Opacity)
            .map_or(0, |t| t.keys.len())
    }
    pub fn opacity_key_value(&self, frame: Frame) -> Option<f64> {
        Some(
            self.properties
                .get(&Property::Opacity)?
                .keys
                .get(&frame)?
                .value,
        )
    }
    /// Raw authored sample. Paint and bounded color controls clamp separately.
    pub fn opacity_at(&self, frame: Frame, seconds_per_frame: f64) -> Result<f64, String> {
        self.opacity_sample(f64::from(frame), seconds_per_frame)
    }
    /// Fractional raw sample. Integer key records retain both sides' exact ease.
    pub fn opacity_sample(&self, frame: f64, seconds_per_frame: f64) -> Result<f64, String> {
        if !frame.is_finite() || frame < 0.0 || frame > f64::from(Frame::MAX) {
            return Err("Opacity sample must be inside the finite frame range".into());
        }
        let track = self
            .properties
            .get(&Property::Opacity)
            .ok_or("Missing Opacity value source")?;
        let Some(timing) = &self.opacity_timing else {
            return Ok(track.sample(frame));
        };
        if !seconds_per_frame.is_finite() || seconds_per_frame <= 0. {
            return Err("Opacity seconds per frame must be finite and positive".into());
        }
        let (&first_frame, first) = track
            .keys
            .first_key_value()
            .ok_or("Native Opacity timing requires keys")?;
        let (&last_frame, last) = track.keys.last_key_value().unwrap();
        if frame <= f64::from(first_frame) {
            return Ok(first.value);
        }
        if frame >= f64::from(last_frame) {
            return Ok(last.value);
        }
        let (&a_frame, a) = track
            .keys
            .range(..=frame.floor() as Frame)
            .next_back()
            .ok_or("Missing Opacity segment")?;
        if frame == f64::from(a_frame) {
            return Ok(a.value);
        }
        let (&b_frame, b) = track
            .keys
            .range((
                std::ops::Bound::Excluded(a_frame),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .ok_or("Missing Opacity segment")?;
        scalar::sample_segment(
            a_frame,
            a.value,
            timing
                .keys
                .get(&a_frame)
                .ok_or("Missing incoming Opacity timing record")?,
            b_frame,
            b.value,
            timing
                .keys
                .get(&b_frame)
                .ok_or("Missing outgoing Opacity timing record")?,
            frame,
            seconds_per_frame,
        )
        .map_err(|e| e.to_string())
    }
    fn validate_opacity_sampling(&self, seconds_per_frame: f64) -> Result<(), String> {
        let Some(timing) = &self.opacity_timing else {
            return Ok(());
        };
        let track = self
            .properties
            .get(&Property::Opacity)
            .ok_or("Missing Opacity value source")?;
        let mut previous = None;
        for (&frame, key) in &track.keys {
            let time = timing
                .keys
                .get(&frame)
                .ok_or("Missing Opacity timing record")?;
            if let Some((a_frame, a_value, a_time)) = previous {
                scalar::validate_segment(
                    a_frame,
                    a_value,
                    a_time,
                    frame,
                    key.value,
                    time,
                    seconds_per_frame,
                )
                .map_err(|e| e.to_string())?;
            }
            previous = Some((frame, key.value, time));
        }
        Ok(())
    }
}
impl Project {
    pub fn validate_opacity_animation(&self) -> Result<(), String> {
        for (_, comp) in self.compositions() {
            for layer in &comp.layers {
                layer
                    .validate_opacity_sampling(comp.fps.seconds(1))
                    .map_err(|e| format!("{} Opacity: {e}", layer.name))?;
            }
        }
        Ok(())
    }
}
pub(super) fn materialized(project: &Project) -> bool {
    project
        .compositions()
        .into_iter()
        .any(|(_, comp)| comp.layers.iter().any(Layer::has_opacity_timing))
}
fn canonical(track: &AnimatedProperty) -> bool {
    track
        .keys
        .values()
        .all(|key| key.interpolation == Interpolation::Linear && key.temporal.is_empty())
}
pub(super) fn validate(layer: &Layer, version: u32) -> Result<(), String> {
    let Some(timing) = &layer.opacity_timing else {
        return Ok(());
    };
    if version < 73 {
        return Err("Native Opacity timing requires project version 73".into());
    }
    let track = layer
        .properties
        .get(&Property::Opacity)
        .ok_or("Missing Opacity value source")?;
    if timing.keys.is_empty() || !timing.keys.keys().eq(track.keys.keys()) {
        return Err("Native Opacity timing must exactly cover a nonempty value-key map".into());
    }
    if !canonical(track) {
        return Err(
            "Native Opacity timing requires neutral legacy interpolation and handles".into(),
        );
    }
    for key in timing.keys.values() {
        key.validate().map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(super) fn edits_only(project: &Project, command: &Command) -> bool {
    match command {
        Command::SetOpacityTiming { .. } => true,
        Command::SetValue {
            id,
            property: Property::Opacity,
            ..
        }
        | Command::SetColor { id, .. } => project
            .composition
            .layer(*id)
            .is_some_and(Layer::has_opacity_timing),
        Command::Batch(commands) => {
            !commands.is_empty() && commands.iter().all(|c| edits_only(project, c))
        }
        _ => false,
    }
}
pub(super) fn guard(state: &Snapshot, command: &Command) -> Result<(), String> {
    match command {
        Command::ToggleKeyframe {
            id,
            property: Property::Opacity,
            ..
        }
        | Command::SetInterpolation {
            id,
            property: Property::Opacity,
            ..
        }
        | Command::MoveKeyframe {
            id,
            property: Property::Opacity,
            ..
        }
        | Command::EditKeyframe {
            id,
            property: Property::Opacity,
            ..
        }
        | Command::ToggleAnimation {
            id,
            property: Property::Opacity,
            ..
        } if state
            .project
            .composition
            .layer(*id)
            .is_some_and(Layer::has_opacity_timing) =>
        {
            Err("Native Opacity timing requires its dedicated key/mode commands".into())
        }
        _ => Ok(()),
    }
}
fn promote(layer: &mut Layer) -> Result<(), String> {
    if layer.opacity_timing.is_some() {
        return Ok(());
    }
    let track = layer
        .properties
        .get(&Property::Opacity)
        .ok_or("Missing Opacity value source")?;
    if track.keys.is_empty() {
        return Err("Create Opacity keys before assigning temporal metadata".into());
    }
    if !canonical(track) {
        return Err("Native Opacity timing promotion requires Linear keys with empty legacy handles; existing curves are preserved".into());
    }
    layer.opacity_timing = Some(OpacityTiming {
        keys: track
            .keys
            .keys()
            .map(|frame| (*frame, OpacityKeyTiming::new()))
            .collect(),
    });
    Ok(())
}
fn set_key(layer: &mut Layer, frame: Frame, value: f64) -> Result<(), String> {
    let track = layer
        .properties
        .get_mut(&Property::Opacity)
        .ok_or("Missing Opacity value source")?;
    track
        .keys
        .entry(frame)
        .and_modify(|key| key.value = value)
        .or_insert(Keyframe {
            value,
            interpolation: Interpolation::Linear,
            temporal: TemporalHandles::default(),
        });
    if let Some(timing) = &mut layer.opacity_timing {
        timing
            .keys
            .entry(frame)
            .or_insert_with(OpacityKeyTiming::new);
    }
    Ok(())
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let (id, edit, ui_value) = match command {
        Command::SetOpacityTiming { id, edit } => (*id, edit.clone(), false),
        Command::SetValue {
            id,
            property: Property::Opacity,
            frame,
            value,
        } if state
            .project
            .composition
            .layer(*id)
            .is_some_and(Layer::has_opacity_timing) =>
        {
            (
                *id,
                OpacityEdit::Key {
                    frame: *frame,
                    value: *value,
                },
                true,
            )
        }
        _ => return None,
    };
    Some((|| {
        let duration = state.project.composition.duration;
        let seconds_per_frame = state.project.composition.fps.seconds(1);
        let layer = editing::editable(state, id)?;
        if let OpacityEdit::Value(value)
        | OpacityEdit::Key { value, .. }
        | OpacityEdit::Collapse { value } = &edit
        {
            if !Property::Opacity.accepts(*value) {
                return Err("Authored Opacity values must be finite and between 0 and 100".into());
            }
        }
        match edit {
            OpacityEdit::Collapse { value } => {
                let track = layer
                    .properties
                    .get_mut(&Property::Opacity)
                    .ok_or("Missing Opacity value source")?;
                track.keys.clear();
                track.value = value;
                layer.opacity_timing = None;
            }
            OpacityEdit::Value(value) => {
                let track = layer
                    .properties
                    .get_mut(&Property::Opacity)
                    .ok_or("Missing Opacity value source")?;
                if !track.keys.is_empty() {
                    return Err(
                        "setValue on animated Opacity requires an explicit static collapse".into(),
                    );
                }
                track.value = value;
            }
            OpacityEdit::Key { frame, value } => {
                if frame >= duration {
                    return Err("Opacity key is outside composition".into());
                }
                if ui_value && layer.opacity_at(frame, seconds_per_frame)? == value {
                    return Ok(());
                }
                set_key(layer, frame, value)?;
            }
            OpacityEdit::RemoveKey { frame } => {
                let track = layer
                    .properties
                    .get_mut(&Property::Opacity)
                    .ok_or("Missing Opacity value source")?;
                if track.keys.len() == 1 && track.keys.contains_key(&frame) {
                    return Err("Removing the final native Opacity key requires an explicit static collapse".into());
                }
                track.keys.remove(&frame).ok_or("Opacity key not found")?;
                if let Some(timing) = &mut layer.opacity_timing {
                    timing
                        .keys
                        .remove(&frame)
                        .ok_or("Opacity timing record not found")?;
                }
            }
            _ => {
                let frame = match &edit {
                    OpacityEdit::Interpolation { frame, .. }
                    | OpacityEdit::TemporalEase { frame, .. }
                    | OpacityEdit::TemporalContinuous { frame, .. }
                    | OpacityEdit::TemporalAutoBezier { frame, .. } => *frame,
                    _ => unreachable!(),
                };
                let track = layer
                    .properties
                    .get(&Property::Opacity)
                    .ok_or("Missing Opacity value source")?;
                let key = track.keys.get(&frame).ok_or("Opacity key not found")?;
                match &edit {
                    OpacityEdit::TemporalContinuous { value: true, .. } => {
                        return Err("Native Opacity temporal continuous mode is unsupported".into());
                    }
                    OpacityEdit::TemporalAutoBezier { value: true, .. } => {
                        return Err(
                            "Native Opacity temporal auto Bezier mode is unsupported".into()
                        );
                    }
                    OpacityEdit::TemporalContinuous { value: false, .. }
                        if layer.opacity_timing.is_none()
                            && key.temporal.mode == TemporalMode::Independent =>
                    {
                        return Ok(());
                    }
                    OpacityEdit::TemporalAutoBezier { value: false, .. }
                        if layer.opacity_timing.is_none()
                            && key.temporal.mode != TemporalMode::Auto =>
                    {
                        return Ok(());
                    }
                    OpacityEdit::Interpolation {
                        incoming: OpacityInterpolation::Linear,
                        outgoing: OpacityInterpolation::Linear,
                        ..
                    } if layer.opacity_timing.is_none() && canonical(track) => return Ok(()),
                    _ => {}
                }
                promote(layer)?;
                let key = layer
                    .opacity_timing
                    .as_mut()
                    .unwrap()
                    .keys
                    .get_mut(&frame)
                    .ok_or("Opacity timing record not found")?;
                match edit {
                    OpacityEdit::Interpolation {
                        incoming, outgoing, ..
                    } => {
                        key.in_interpolation = incoming;
                        key.out_interpolation = outgoing;
                    }
                    OpacityEdit::TemporalEase {
                        incoming, outgoing, ..
                    } => {
                        incoming.validate().map_err(|e| e.to_string())?;
                        outgoing.validate().map_err(|e| e.to_string())?;
                        key.in_ease = incoming;
                        key.out_ease = outgoing;
                    }
                    OpacityEdit::TemporalContinuous { value, .. } => {
                        key.temporal_continuous = value
                    }
                    OpacityEdit::TemporalAutoBezier { value, .. } => {
                        key.temporal_auto_bezier = value
                    }
                    _ => unreachable!(),
                }
            }
        }
        validate(layer, 73)?;
        if materialized(&state.project) {
            state.project.version = state.project.version.max(73);
        }
        Ok(())
    })())
}
/// Move only timing record addresses; the existing value store is shifted by
/// layer_timing in the same detached candidate transaction.
pub(super) fn shift(layer: &mut Layer, delta: i64, duration: Frame) -> Result<(), String> {
    let Some(timing) = &mut layer.opacity_timing else {
        return Ok(());
    };
    timing.keys = timing
        .keys
        .iter()
        .map(|(&frame, key)| {
            let frame = i64::from(frame)
                .checked_add(delta)
                .filter(|frame| *frame >= 0 && *frame < i64::from(duration))
                .ok_or("Opacity timing key would leave the composition")?;
            Ok((frame as Frame, *key))
        })
        .collect::<Result<_, String>>()?;
    Ok(())
}
