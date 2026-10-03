//! Ordered effect instances with bounded, animated scalar parameters.
use super::*;
#[path = "effect_presets.rs"]
mod presets;
pub use presets::EffectPreset;

pub type EffectId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectKind {
    GaussianBlur,
    Brightness,
    Grayscale,
    Fill,
    Tint,
    HueSaturation,
    Levels,
    DropShadow,
    Glow,
    Curves,
    LinearGradient,
    RadialGradient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EffectParam {
    Radius,
    Amount,
    Red,
    Green,
    Blue,
    DarkRed,
    DarkGreen,
    DarkBlue,
    Opacity,
    OffsetX,
    OffsetY,
    Hue,
    Black,
    White,
    Gamma,
    Curve0,
    Curve25,
    Curve50,
    Curve75,
    Curve100,
    RedCurve0,
    RedCurve25,
    RedCurve50,
    RedCurve75,
    RedCurve100,
    GreenCurve0,
    GreenCurve25,
    GreenCurve50,
    GreenCurve75,
    GreenCurve100,
    BlueCurve0,
    BlueCurve25,
    BlueCurve50,
    BlueCurve75,
    BlueCurve100,
    StartX,
    StartY,
    EndX,
    EndY,
    BlendOriginal,
}

#[derive(Clone, Copy, Debug)]
pub struct ParameterSpec {
    pub parameter: EffectParam,
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}
impl ParameterSpec {
    fn accepts(self, value: f64) -> bool {
        value.is_finite() && (self.min..=self.max).contains(&value)
    }
}
impl EffectKind {
    pub const ALL: [Self; 12] = [
        Self::GaussianBlur,
        Self::Brightness,
        Self::Grayscale,
        Self::Fill,
        Self::Tint,
        Self::HueSaturation,
        Self::Levels,
        Self::DropShadow,
        Self::Glow,
        Self::Curves,
        Self::LinearGradient,
        Self::RadialGradient,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::GaussianBlur => "Gaussian Blur",
            Self::Brightness => "Brightness",
            Self::Grayscale => "Grayscale",
            Self::Fill => "Fill",
            Self::Tint => "Tint",
            Self::HueSaturation => "Hue / Saturation",
            Self::Levels => "Levels",
            Self::DropShadow => "Drop Shadow",
            Self::Glow => "Glow",
            Self::Curves => "Curves",
            Self::LinearGradient => "Linear Gradient",
            Self::RadialGradient => "Radial Gradient",
        }
    }
    pub fn parameters(self) -> Vec<ParameterSpec> {
        use EffectParam::*;
        let spec = |parameter, label, min, max, default| ParameterSpec {
            parameter,
            label,
            min,
            max,
            default,
        };
        let colors = || {
            vec![
                spec(Red, "Red", 0.0, 255.0, 255.0),
                spec(Green, "Green", 0.0, 255.0, 255.0),
                spec(Blue, "Blue", 0.0, 255.0, 255.0),
            ]
        };
        let radius = || spec(Radius, "Radius (px)", 0.0, 100.0, 10.0);
        let opacity = || spec(Opacity, "Opacity (%)", 0.0, 100.0, 100.0);
        match self {
            Self::Curves => CurveChannel::ALL
                .into_iter()
                .flat_map(|channel| {
                    channel
                        .parameters()
                        .into_iter()
                        .enumerate()
                        .map(move |(i, p)| {
                            spec(p, channel.point_labels()[i], 0.0, 255.0, i as f64 * 63.75)
                        })
                })
                .collect(),
            Self::LinearGradient | Self::RadialGradient => vec![
                spec(StartX, "Start X (px)", -32768.0, 32768.0, 0.0),
                spec(StartY, "Start Y (px)", -32768.0, 32768.0, 0.0),
                spec(EndX, "End X (px)", -32768.0, 32768.0, 0.0),
                spec(EndY, "End Y (px)", -32768.0, 32768.0, 100.0),
                spec(DarkRed, "Start red", 0.0, 255.0, 0.0),
                spec(DarkGreen, "Start green", 0.0, 255.0, 0.0),
                spec(DarkBlue, "Start blue", 0.0, 255.0, 0.0),
                spec(Red, "End red", 0.0, 255.0, 255.0),
                spec(Green, "End green", 0.0, 255.0, 255.0),
                spec(Blue, "End blue", 0.0, 255.0, 255.0),
                spec(BlendOriginal, "Original (%)", 0.0, 100.0, 0.0),
            ],
            Self::GaussianBlur => vec![radius()],
            Self::Brightness => vec![spec(Amount, "Multiplier", 0.0, 4.0, 1.0)],
            Self::Grayscale => vec![],
            Self::Fill => {
                let mut p = colors();
                p.push(opacity());
                p
            }
            Self::Tint => {
                let mut p = vec![
                    spec(DarkRed, "Black → Red", 0.0, 255.0, 0.0),
                    spec(DarkGreen, "Black → Green", 0.0, 255.0, 0.0),
                    spec(DarkBlue, "Black → Blue", 0.0, 255.0, 0.0),
                ];
                p.extend(colors());
                p.push(spec(Amount, "Amount (%)", 0.0, 100.0, 100.0));
                p
            }
            Self::HueSaturation => vec![
                spec(Hue, "Hue (deg)", -360.0, 360.0, 0.0),
                spec(Amount, "Saturation", 0.0, 4.0, 1.0),
            ],
            Self::Levels => vec![
                spec(Black, "Input black", 0.0, 1.0, 0.0),
                spec(White, "Input white", 0.0, 1.0, 1.0),
                spec(Gamma, "Gamma", 0.1, 10.0, 1.0),
            ],
            Self::DropShadow => {
                let mut p = colors();
                for s in &mut p {
                    s.default = 0.0;
                }
                p.extend([
                    opacity(),
                    radius(),
                    spec(OffsetX, "Offset X (px)", -1000.0, 1000.0, 12.0),
                    spec(OffsetY, "Offset Y (px)", -1000.0, 1000.0, 12.0),
                ]);
                p
            }
            Self::Glow => vec![radius(), spec(Amount, "Intensity", 0.0, 4.0, 1.0)],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectColorSpace {
    #[default]
    Srgb,
    LinearRgb,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectInstance {
    id: EffectId,
    kind: EffectKind,
    name: String,
    #[serde(default)]
    bypassed: bool,
    #[serde(default)]
    color_space: EffectColorSpace,
    parameters: BTreeMap<EffectParam, AnimatedProperty>,
}
impl EffectInstance {
    pub fn id(&self) -> EffectId {
        self.id
    }
    pub fn kind(&self) -> EffectKind {
        self.kind
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn bypassed(&self) -> bool {
        self.bypassed
    }
    pub fn color_space(&self) -> EffectColorSpace {
        self.color_space
    }
    pub fn parameter(&self, param: EffectParam) -> Option<&AnimatedProperty> {
        self.parameters.get(&param)
    }
    pub(super) fn parameter_mut(&mut self, param: EffectParam) -> Option<&mut AnimatedProperty> {
        self.parameters.get_mut(&param)
    }
    pub fn value_at(&self, param: EffectParam, frame: Frame) -> f64 {
        let spec = self
            .kind
            .parameters()
            .into_iter()
            .find(|s| s.parameter == param)
            .expect("parameter belongs to effect kind");
        self.parameters[&param]
            .value_at(frame)
            .clamp(spec.min, spec.max)
    }
    pub fn curve_values(&self, channel: CurveChannel, frame: Frame) -> [f64; 5] {
        channel
            .parameters()
            .map(|p| self.value_at(p, frame) / 255.0)
    }
    fn gradient_defaults(&mut self, width: f64, height: f64) {
        if matches!(
            self.kind,
            EffectKind::LinearGradient | EffectKind::RadialGradient
        ) {
            self.parameters.get_mut(&EffectParam::EndY).unwrap().value = height;
            if self.kind == EffectKind::RadialGradient {
                self.parameters.get_mut(&EffectParam::StartX).unwrap().value = width / 2.0;
                self.parameters.get_mut(&EffectParam::StartY).unwrap().value = height / 2.0;
                self.parameters.get_mut(&EffectParam::EndX).unwrap().value = width / 2.0;
            }
        }
    }
    fn new(id: EffectId, kind: EffectKind) -> Self {
        Self {
            id,
            kind,
            name: kind.label().into(),
            bypassed: false,
            color_space: EffectColorSpace::Srgb,
            parameters: kind
                .parameters()
                .iter()
                .map(|s| (s.parameter, AnimatedProperty::new(s.default)))
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum EffectEdit {
    ApplyPreset {
        preset: EffectPreset,
        frame: Frame,
    },
    EditKeyframe {
        effect: EffectId,
        parameter: EffectParam,
        from: Frame,
        to: Frame,
        value: f64,
    },
    Add(EffectKind),
    ConvertLegacy,
    Remove(EffectId),
    Duplicate(EffectId),
    Move {
        effect: EffectId,
        index: usize,
    },
    Bypass {
        effect: EffectId,
        bypassed: bool,
    },
    Rename {
        effect: EffectId,
        name: String,
    },
    Reset(EffectId),
    SetValue {
        effect: EffectId,
        parameter: EffectParam,
        frame: Frame,
        value: f64,
    },
    ToggleAnimation {
        effect: EffectId,
        parameter: EffectParam,
        frame: Frame,
    },
    ToggleKey {
        effect: EffectId,
        parameter: EffectParam,
        frame: Frame,
    },
    Interpolate {
        effect: EffectId,
        parameter: EffectParam,
        frame: Frame,
        interpolation: Interpolation,
    },
}
impl Layer {
    pub fn effect_stack(&self) -> &[EffectInstance] {
        &self.effect_stack
    }
    pub(super) fn all_tracks_mut(&mut self) -> impl Iterator<Item = &mut AnimatedProperty> {
        self.properties
            .values_mut()
            .chain(
                match &mut self.content {
                    Content::Shape(s) => Some(
                        std::iter::once(&mut s.path_animation.timing)
                            .chain(s.parameters.values_mut()),
                    ),
                    _ => None,
                }
                .into_iter()
                .flatten(),
            )
            .chain(self.time_remap.iter_mut())
            .chain(self.audio_controls.parameters.values_mut())
            .chain(self.path_masks.iter_mut().flat_map(|m| {
                m.parameters
                    .values_mut()
                    .chain(std::iter::once(&mut m.animation.timing))
            }))
            .chain(
                self.effect_stack
                    .iter_mut()
                    .flat_map(|e| e.parameters.values_mut()),
            )
    }
}
pub(super) fn first_effect_id() -> EffectId {
    1
}
pub(super) fn validate(layer: &Layer, duration: Frame) -> Result<(), String> {
    validate_stack(&layer.effect_stack, layer.next_effect_id, duration)
}
fn validate_stack(stack: &[EffectInstance], next: EffectId, duration: Frame) -> Result<(), String> {
    if stack.len() > 64 || next == 0 {
        return Err("A layer supports up to 64 effects".into());
    }
    let mut ids = BTreeSet::new();
    for effect in stack {
        if effect.id == 0
            || effect.id >= next
            || !ids.insert(effect.id)
            || effect.name.trim().is_empty()
            || effect.name.len() > 256
        {
            return Err("Invalid effect identity or name".into());
        }
        let specs = effect.kind.parameters();
        if effect.parameters.len() != specs.len() {
            return Err("Invalid effect parameter set".into());
        }
        for spec in specs {
            let track = effect
                .parameters
                .get(&spec.parameter)
                .ok_or("Missing effect parameter")?;
            if !spec.accepts(track.value)
                || track.keys.len() > 10000
                || track.keys.iter().any(|(f, k)| {
                    *f >= duration || !spec.accepts(k.value) || !k.interpolation.valid()
                })
            {
                return Err("Invalid effect animation or parameter value".into());
            }
        }
    }
    Ok(())
}
fn add(layer: &mut Layer, kind: EffectKind) -> Result<EffectId, String> {
    if layer.effect_stack.len() >= 64 || layer.next_effect_id == u64::MAX {
        return Err("Effect limit reached".into());
    }
    let id = layer.next_effect_id;
    layer.next_effect_id += 1;
    let mut effect = EffectInstance::new(id, kind);
    effect.gradient_defaults(layer.width, layer.height);
    layer.effect_stack.push(effect);
    Ok(id)
}
pub(super) fn apply(state: &mut Snapshot, id: LayerId, edit: EffectEdit) -> Result<(), String> {
    let duration = state.project.composition.duration;
    let fps = state.project.composition.fps;
    let layer = state
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| l.id == id)
        .ok_or("Layer not found")?;
    if layer.locked {
        return Err("Unlock the layer before editing effects".into());
    }
    if matches!(layer.content, Content::Null) {
        return Err("Null objects have no rendered pixels to affect".into());
    }
    match edit {
        EffectEdit::ApplyPreset { preset, frame } => preset.apply(layer, frame, fps, duration)?,
        EffectEdit::EditKeyframe {
            effect,
            parameter,
            from,
            to,
            value,
        } => {
            if to >= duration {
                return Err("Effect keyframe is outside the composition".into());
            }
            let effect = layer
                .effect_stack
                .iter_mut()
                .find(|e| e.id == effect)
                .ok_or("Effect not found")?;
            let spec = effect
                .kind
                .parameters()
                .into_iter()
                .find(|s| s.parameter == parameter)
                .ok_or("Parameter not found")?;
            if !spec.accepts(value) {
                return Err(format!(
                    "{} must be between {} and {}",
                    spec.label, spec.min, spec.max
                ));
            }
            let track = effect
                .parameters
                .get_mut(&parameter)
                .ok_or("Parameter not found")?;
            if from != to && track.keys.contains_key(&to) {
                return Err("Destination already contains a key".into());
            }
            let mut key = track.keys.remove(&from).ok_or("Keyframe not found")?;
            key.value = value;
            track.keys.insert(to, key);
        }
        EffectEdit::Add(kind) => {
            add(layer, kind)?;
        }
        EffectEdit::ConvertLegacy => {
            let legacy = layer.effects;
            let mut migrated = Vec::new();
            for (kind, param, value) in [
                (EffectKind::GaussianBlur, EffectParam::Radius, legacy.blur),
                (
                    EffectKind::Grayscale,
                    EffectParam::Amount,
                    if legacy.grayscale { 1.0 } else { 0.0 },
                ),
                (
                    EffectKind::Brightness,
                    EffectParam::Amount,
                    legacy.brightness,
                ),
            ] {
                if (kind == EffectKind::Brightness && value == 1.0)
                    || (kind != EffectKind::Brightness && value == 0.0)
                {
                    continue;
                }
                add(layer, kind)?;
                let mut instance = layer.effect_stack.pop().unwrap();
                instance.color_space = EffectColorSpace::LinearRgb;
                if let Some(track) = instance.parameters.get_mut(&param) {
                    track.value = value;
                }
                migrated.push(instance);
            }
            layer.effect_stack.splice(0..0, migrated);
            layer.effects = Effects::default();
        }
        EffectEdit::Remove(effect) => {
            let at = layer
                .effect_stack
                .iter()
                .position(|e| e.id == effect)
                .ok_or("Effect not found")?;
            layer.effect_stack.remove(at);
        }
        EffectEdit::Duplicate(effect) => {
            let at = layer
                .effect_stack
                .iter()
                .position(|e| e.id == effect)
                .ok_or("Effect not found")?;
            let mut copy = layer.effect_stack[at].clone();
            copy.id = add(layer, copy.kind)?;
            layer.effect_stack.pop();
            layer.effect_stack.insert(at + 1, copy);
        }
        EffectEdit::Move { effect, index } => {
            if index >= layer.effect_stack.len() {
                return Err("Effect destination is outside the stack".into());
            }
            let at = layer
                .effect_stack
                .iter()
                .position(|e| e.id == effect)
                .ok_or("Effect not found")?;
            let effect = layer.effect_stack.remove(at);
            layer.effect_stack.insert(index, effect);
        }
        EffectEdit::Bypass { effect, bypassed } => {
            layer
                .effect_stack
                .iter_mut()
                .find(|e| e.id == effect)
                .ok_or("Effect not found")?
                .bypassed = bypassed
        }
        EffectEdit::Rename { effect, name } => {
            layer
                .effect_stack
                .iter_mut()
                .find(|e| e.id == effect)
                .ok_or("Effect not found")?
                .name = name.trim().into()
        }
        EffectEdit::Reset(effect) => {
            let e = layer
                .effect_stack
                .iter_mut()
                .find(|e| e.id == effect)
                .ok_or("Effect not found")?;
            let mut fresh = EffectInstance::new(effect, e.kind);
            fresh.gradient_defaults(layer.width, layer.height);
            fresh.name = e.name.clone();
            fresh.color_space = e.color_space;
            *e = fresh;
        }
        EffectEdit::SetValue {
            effect,
            parameter,
            frame,
            ..
        }
        | EffectEdit::ToggleAnimation {
            effect,
            parameter,
            frame,
        }
        | EffectEdit::ToggleKey {
            effect,
            parameter,
            frame,
        }
        | EffectEdit::Interpolate {
            effect,
            parameter,
            frame,
            ..
        } => {
            if frame >= duration {
                return Err("Effect keyframe is outside the composition".into());
            }
            let effect = layer
                .effect_stack
                .iter_mut()
                .find(|e| e.id == effect)
                .ok_or("Effect not found")?;
            let spec = effect
                .kind
                .parameters()
                .into_iter()
                .find(|s| s.parameter == parameter)
                .ok_or("Parameter not found")?;
            let track = effect
                .parameters
                .get_mut(&parameter)
                .ok_or("Parameter not found")?;
            match edit {
                EffectEdit::SetValue { value, .. } => {
                    if !spec.accepts(value) {
                        return Err(format!(
                            "{} must be between {} and {}",
                            spec.label, spec.min, spec.max
                        ));
                    }
                    if track.keys.is_empty() {
                        track.value = value;
                    } else {
                        let interpolation = track
                            .keys
                            .get(&frame)
                            .map_or(Interpolation::Linear, |k| k.interpolation);
                        track.keys.insert(
                            frame,
                            Keyframe {
                                temporal: track
                                    .keys
                                    .get(&frame)
                                    .map_or(TemporalHandles::default(), |k| k.temporal),
                                value,
                                interpolation,
                            },
                        );
                    }
                }
                EffectEdit::ToggleAnimation { .. } => {
                    if track.keys.is_empty() {
                        track.keys.insert(
                            frame,
                            Keyframe {
                                temporal: TemporalHandles::default(),
                                value: track.value,
                                interpolation: Interpolation::Linear,
                            },
                        );
                    } else {
                        track.value = track.value_at(frame).clamp(spec.min, spec.max);
                        track.keys.clear();
                    }
                }
                EffectEdit::ToggleKey { .. } => {
                    let value = track.value_at(frame).clamp(spec.min, spec.max);
                    if track.keys.remove(&frame).is_none() {
                        track.keys.insert(
                            frame,
                            Keyframe {
                                temporal: TemporalHandles::default(),
                                value,
                                interpolation: Interpolation::Linear,
                            },
                        );
                    } else if track.keys.is_empty() {
                        track.value = value;
                    }
                }
                EffectEdit::Interpolate { interpolation, .. } => {
                    track.set_interpolation(frame, interpolation)?;
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e
    }
    fn edit(e: &mut Editor, edit: EffectEdit) {
        e.execute(Command::Effect { id: 1, edit }).unwrap();
    }
    #[test]
    fn instances_reorder_duplicate_bypass_reset_and_roundtrip_in_one_undo_each() {
        let mut e = scene();
        edit(&mut e, EffectEdit::Add(EffectKind::GaussianBlur));
        edit(
            &mut e,
            EffectEdit::SetValue {
                effect: 1,
                parameter: EffectParam::Radius,
                frame: 0,
                value: 42.0,
            },
        );
        edit(&mut e, EffectEdit::Duplicate(1));
        edit(
            &mut e,
            EffectEdit::Rename {
                effect: 2,
                name: "Soft edge".into(),
            },
        );
        edit(
            &mut e,
            EffectEdit::Bypass {
                effect: 2,
                bypassed: true,
            },
        );
        edit(
            &mut e,
            EffectEdit::Move {
                effect: 2,
                index: 0,
            },
        );
        let stack = e.selected_layer().unwrap().effect_stack();
        assert_eq!(
            stack.iter().map(EffectInstance::id).collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert!(stack[0].bypassed());
        assert_eq!(stack[0].value_at(EffectParam::Radius, 0), 42.0);
        let before = e.project().clone();
        edit(&mut e, EffectEdit::Reset(2));
        assert_eq!(
            e.selected_layer().unwrap().effect_stack()[0].value_at(EffectParam::Radius, 0),
            10.0
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        let saved = e.project();
        assert_eq!(saved.version, 12);
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            *saved
        );
        edit(&mut e, EffectEdit::Remove(2));
        assert_eq!(e.selected_layer().unwrap().effect_stack().len(), 1);
        edit(&mut e, EffectEdit::Add(EffectKind::Fill));
        assert_eq!(e.selected_layer().unwrap().effect_stack()[1].id(), 3);
    }
    #[test]
    fn effect_keys_follow_shift_split_clipboard_fps_and_duration_validation() {
        let mut e = scene();
        e.execute(Command::SetLayerRange {
            id: 1,
            start: 0,
            end: 60,
        })
        .unwrap();
        edit(&mut e, EffectEdit::Add(EffectKind::GaussianBlur));
        edit(
            &mut e,
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter: EffectParam::Radius,
                frame: 0,
            },
        );
        edit(
            &mut e,
            EffectEdit::SetValue {
                effect: 1,
                parameter: EffectParam::Radius,
                frame: 30,
                value: 40.0,
            },
        );
        assert_eq!(
            e.selected_layer().unwrap().effect_stack()[0].value_at(EffectParam::Radius, 15),
            25.0
        );
        e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
        assert_eq!(
            e.selected_layer().unwrap().effect_stack()[0]
                .parameter(EffectParam::Radius)
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![10, 40]
        );
        let before = e.project().clone();
        e.execute(Command::SplitLayers {
            ids: vec![1],
            frame: 35,
        })
        .unwrap();
        assert_eq!(
            e.project().composition().layers()[0].effect_stack(),
            e.project().composition().layers()[1].effect_stack()
        );
        e.undo();
        assert_eq!(e.project(), &before);
        let clipboard = e.copy_layers(&[1]).unwrap();
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::ConfigureComposition {
            name: "60fps".into(),
            width: 1920,
            height: 1080,
            fps: 60,
            duration: 300,
        })
        .unwrap();
        e.execute(Command::PasteLayers(clipboard)).unwrap();
        let track = e.selected_layer().unwrap().effect_stack()[0]
            .parameter(EffectParam::Radius)
            .unwrap();
        assert_eq!(
            track.keys().keys().copied().collect::<Vec<_>>(),
            vec![20, 80]
        );
        assert_eq!(track.value_at(50), 25.0);
        let id = e.selected().unwrap();
        e.execute(Command::SetLayerRange {
            id,
            start: 0,
            end: 50,
        })
        .unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::ConfigureComposition {
                name: "Short".into(),
                width: 1920,
                height: 1080,
                fps: 60,
                duration: 50
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn invalid_effect_edits_locked_batches_and_null_targets_are_atomic() {
        let mut e = scene();
        edit(&mut e, EffectEdit::Add(EffectKind::Fill));
        let before = e.project().clone();
        for operation in [
            EffectEdit::SetValue {
                effect: 1,
                parameter: EffectParam::Red,
                frame: 0,
                value: f64::NAN,
            },
            EffectEdit::SetValue {
                effect: 1,
                parameter: EffectParam::Red,
                frame: 0,
                value: 256.0,
            },
            EffectEdit::ToggleKey {
                effect: 1,
                parameter: EffectParam::Radius,
                frame: 0,
            },
            EffectEdit::ToggleKey {
                effect: 1,
                parameter: EffectParam::Red,
                frame: 150,
            },
            EffectEdit::Rename {
                effect: 1,
                name: " ".into(),
            },
            EffectEdit::Remove(999),
        ] {
            assert!(
                e.execute(Command::Effect {
                    id: 1,
                    edit: operation
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Reset(1)
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        e.execute(Command::AddNull).unwrap();
        assert!(
            e.execute(Command::Effect {
                id: 2,
                edit: EffectEdit::Add(EffectKind::Glow)
            })
            .is_err()
        );
    }
    #[test]
    fn legacy_conversion_retains_order_color_space_and_roundtrip() {
        let mut e = scene();
        e.execute(Command::SetEffects {
            id: 1,
            effects: Effects {
                blur: 13.0,
                brightness: 1.8,
                grayscale: true,
            },
        })
        .unwrap();
        let before = e.project().clone();
        edit(&mut e, EffectEdit::ConvertLegacy);
        assert_eq!(e.selected_layer().unwrap().effects(), Effects::default());
        let stack = e.selected_layer().unwrap().effect_stack();
        assert_eq!(
            stack.iter().map(EffectInstance::kind).collect::<Vec<_>>(),
            vec![
                EffectKind::GaussianBlur,
                EffectKind::Grayscale,
                EffectKind::Brightness
            ]
        );
        assert!(
            stack
                .iter()
                .all(|e| e.color_space() == EffectColorSpace::LinearRgb)
        );
        assert_eq!(stack[2].value_at(EffectParam::Amount, 0), 1.8);
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
        e.undo();
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn disabling_animation_retains_sample_and_last_key_removal_retains_value() {
        let mut e = scene();
        edit(&mut e, EffectEdit::Add(EffectKind::Brightness));
        let parameter = EffectParam::Amount;
        edit(
            &mut e,
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter,
                frame: 0,
            },
        );
        edit(
            &mut e,
            EffectEdit::SetValue {
                effect: 1,
                parameter,
                frame: 20,
                value: 3.0,
            },
        );
        edit(
            &mut e,
            EffectEdit::Interpolate {
                effect: 1,
                parameter,
                frame: 0,
                interpolation: Interpolation::Hold,
            },
        );
        assert_eq!(
            e.selected_layer().unwrap().effect_stack()[0].value_at(parameter, 10),
            1.0
        );
        edit(
            &mut e,
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter,
                frame: 20,
            },
        );
        let track = e.selected_layer().unwrap().effect_stack()[0]
            .parameter(parameter)
            .unwrap();
        assert!(track.keys().is_empty());
        assert_eq!(track.value_at(0), 3.0);
        edit(
            &mut e,
            EffectEdit::ToggleKey {
                effect: 1,
                parameter,
                frame: 5,
            },
        );
        edit(
            &mut e,
            EffectEdit::ToggleKey {
                effect: 1,
                parameter,
                frame: 5,
            },
        );
        assert_eq!(
            e.selected_layer().unwrap().effect_stack()[0].value_at(parameter, 0),
            3.0
        );
    }
}
