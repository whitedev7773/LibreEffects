//! Bounded After Effects-shaped host operations on an isolated project draft.
//!
//! This is a compatibility subset, not an AE implementation. Unsupported host
//! calls permanently reject the draft, even if JavaScript catches the exception.
use std::collections::BTreeSet;
#[path = "automation_opacity.rs"]
mod opacity;
#[path = "automation_planar.rs"]
mod planar;
#[path = "automation_spatial.rs"]
mod spatial;

use libre_effects_core::{
    Command, Composition, CompositionId, Content, Frame, Interpolation, Layer, LayerId, MarkerEdit,
    MarkerTarget, Project, Property, TemporalMode,
};
use serde_json::{Value, json};

pub const MAX_HOST_CALLS: usize = 10_000;
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_HOST_LAYERS: usize = 10_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub struct AutomationHost {
    project: Project,
    selected: BTreeSet<LayerId>,
    frame: Frame,
    calls: usize,
    rejected: Option<String>,
}

impl AutomationHost {
    pub fn new(project: &Project, selected: &[LayerId], frame: Frame) -> Result<Self, String> {
        project.validate_automation_project()?;
        let compositions = project.compositions();
        if compositions
            .iter()
            .map(|(_, comp)| comp.layers().len())
            .sum::<usize>()
            > MAX_HOST_LAYERS
        {
            return Err("Automation is limited to 10000 project layers".into());
        }
        if compositions.iter().any(|(id, comp)| {
            *id > MAX_SAFE_INTEGER
                || comp
                    .layers()
                    .iter()
                    .any(|layer| layer.id() > MAX_SAFE_INTEGER)
        }) || project
            .asset_library()
            .assets()
            .keys()
            .chain(project.asset_library().folders().keys())
            .any(|id| *id > MAX_SAFE_INTEGER)
        {
            return Err("Project IDs exceed JavaScript's exact integer range".into());
        }
        Ok(Self {
            project: project.clone(),
            selected: selected.iter().copied().collect(),
            frame,
            calls: 0,
            rejected: None,
        })
    }

    pub fn snapshot(&self) -> Value {
        let mut items = self.project.compositions().into_iter().map(|(id, comp)| json!({
                "id": id, "type": "composition", "name": comp.name(),
                "width": comp.width(), "height": comp.height(),
                "frameRate": comp.fps().as_f64(), "frameDuration": 1.0 / comp.fps().as_f64(),
                "duration": comp.fps().seconds(comp.duration().into()),
                "time": if id == self.project.active_composition_id() { Some(comp.fps().seconds(self.frame.min(comp.duration().saturating_sub(1)).into())) } else { None },
                "layers": comp.layers().iter().enumerate().map(|(index, layer)| self.layer_snapshot(comp, index, layer)).collect::<Vec<_>>()
            })).collect::<Vec<_>>();
        items.extend(self.project.asset_library().assets().iter().map(|(id, asset)| json!({
            "id": id, "type": "footage", "name": asset.name(), "width": asset.width(), "height": asset.height()
        })));
        items.extend(
            self.project
                .asset_library()
                .folders()
                .iter()
                .map(|(id, folder)| {
                    json!({
                        "id": id, "type": "folder", "name": folder.name()
                    })
                }),
        );
        json!({"activeItem": self.project.active_composition_id(), "items": items})
    }

    fn layer_snapshot(&self, comp: &Composition, index: usize, layer: &Layer) -> Value {
        json!({ "id": layer.id(), "index": index + 1, "name": layer.name(),
            "enabled": layer.visible(), "locked": layer.locked(), "hasText": matches!(layer.content(), Content::Text { .. }),
            "selected": self.selected.contains(&layer.id()), "threeDLayer": layer.is_three_d(),
            "startTime": layer.start_frame() as f64 / comp.fps().as_f64(), "label": layer.label_index(),
            "inPoint": layer.in_frame_sample() / comp.fps().as_f64(),
            "outPoint": layer.out_frame_sample(comp.duration()) / comp.fps().as_f64() })
    }

    pub fn finish(self) -> Result<Project, String> {
        if let Some(error) = self.rejected {
            return Err(error);
        }
        self.project.validate_automation_project()?;
        self.project.validate_spatial_animation()?;
        self.project.validate_planar_animation()?;
        self.project.validate_opacity_animation()?;
        Ok(self.project)
    }

    pub fn dispatch(&mut self, op: &str, args: Value) -> Result<Value, String> {
        if let Some(error) = &self.rejected {
            return Err(error.clone());
        }
        self.calls += 1;
        let result = if self.calls > MAX_HOST_CALLS {
            Err("Automation host call limit exceeded".into())
        } else if serde_json::to_vec(&args).map_or(true, |bytes| bytes.len() > MAX_ARGUMENT_BYTES) {
            Err("Automation host arguments exceed 256 KiB".into())
        } else {
            self.dispatch_inner(op, &args)
        };
        if let Err(error) = &result {
            self.rejected = Some(error.clone());
        }
        result
    }

    fn composition(&self, id: CompositionId) -> Result<&Composition, String> {
        self.project
            .composition_by_id(id)
            .ok_or_else(|| "Composition no longer exists".into())
    }

    fn layer(&self, comp: CompositionId, id: LayerId) -> Result<&Layer, String> {
        self.composition(comp)?
            .layer(id)
            .ok_or_else(|| "Layer no longer exists in this composition".into())
    }

    fn apply(&mut self, comp: CompositionId, command: Command) -> Result<Value, String> {
        self.project.apply_automation_command(comp, command)?;
        Ok(Value::Null)
    }

    fn dispatch_inner(&mut self, op: &str, args: &Value) -> Result<Value, String> {
        if op == "snapshot" {
            return Ok(self.snapshot());
        }
        if op == "unsupported" {
            return Err(format!(
                "Unsupported After Effects API: {}",
                args.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown host operation")
            ));
        }
        let comp = integer(args, "comp")?;
        let id = integer(args, "id")?;
        self.layer(comp, id)?;
        match op {
            "layer_get" => {
                let composition = self.composition(comp)?;
                let (index, layer) = composition
                    .layers()
                    .iter()
                    .enumerate()
                    .find(|(_, layer)| layer.id() == id)
                    .ok_or("Layer no longer exists")?;
                Ok(self.layer_snapshot(composition, index, layer))
            }
            "layer_set" => self.layer_set(comp, id, args),
            "layer_duplicate" => {
                if self
                    .project
                    .compositions()
                    .iter()
                    .map(|(_, comp)| comp.layers().len())
                    .sum::<usize>()
                    >= MAX_HOST_LAYERS
                {
                    return Err("Automation is limited to 10000 project layers".into());
                }
                let before: BTreeSet<_> = self
                    .composition(comp)?
                    .layers()
                    .iter()
                    .map(Layer::id)
                    .collect();
                self.apply(comp, Command::DuplicateLayer(id))?;
                let new_id = self
                    .composition(comp)?
                    .layers()
                    .iter()
                    .find(|layer| !before.contains(&layer.id()))
                    .ok_or("Duplicate did not create a layer")?
                    .id();
                if new_id > MAX_SAFE_INTEGER {
                    return Err("Layer ID exceeds JavaScript's exact integer range".into());
                }
                Ok(json!(new_id))
            }
            "layer_remove" => self.apply(comp, Command::RemoveLayer(id)),
            "layer_move_to_beginning" => self.apply(comp, Command::MoveLayer { id, index: 0 }),
            "property_get" => self.property_get(
                comp,
                id,
                text(args, "property")?,
                args.get("field").and_then(Value::as_str) == Some("metadata"),
            ),
            "property_expression" => {
                let target = expression_target(text(args, "property")?)
                    .ok_or("Only Position, Scale and Opacity support numeric expressions")?;
                let command = match text(args, "field")? {
                    "source" => Command::SetExpression {
                        id,
                        target,
                        source: text(args, "value")?.into(),
                        enabled: true,
                    },
                    "enabled" => Command::SetExpressionEnabled {
                        id,
                        target,
                        enabled: args
                            .get("value")
                            .and_then(Value::as_bool)
                            .ok_or("expressionEnabled must be boolean")?,
                    },
                    _ => return Err("Unknown expression attribute".into()),
                };
                self.apply(comp, command)
            }
            "property_set" => self.property_set(comp, id, args),
            "property_key" => self.property_key(comp, id, args),
            "property_key_metadata" => self.property_key_metadata(comp, id, args),
            "marker_set" => self.marker_set(comp, id, args),
            _ => Err(format!("Unsupported automation host operation: {op}")),
        }
    }

    fn layer_set(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let field = text(args, "field")?;
        let value = args.get("value").ok_or("Missing field value")?;
        match field {
            "name" => {
                let name = value.as_str().ok_or("Layer name must be text")?;
                // The editor's rename command trims whitespace; reject instead of
                // silently changing the requested AE name.
                if name.trim() != name {
                    return Err(
                        "Layer names with leading or trailing whitespace are unsupported".into(),
                    );
                }
                if name == self.layer(comp, id)?.name() {
                    return Ok(Value::Null);
                }
                self.apply(
                    comp,
                    Command::RenameLayer {
                        id,
                        name: name.into(),
                    },
                )
            }
            "enabled" => {
                let enabled = value.as_bool().ok_or("enabled must be boolean")?;
                if enabled == self.layer(comp, id)?.visible() {
                    return Ok(Value::Null);
                }
                self.apply(comp, Command::ToggleVisible(id))
            }
            "inPoint" | "outPoint" => {
                let composition = self.composition(comp)?;
                let layer = self.layer(comp, id)?;
                let seconds = number(value)?;
                let fps = composition.fps().as_f64();
                let previous = if field == "inPoint" {
                    layer.in_frame_sample()
                } else {
                    layer.out_frame_sample(composition.duration())
                };
                // Reading and assigning the same time must preserve exact source bytes.
                if seconds == previous / fps {
                    return Ok(Value::Null);
                }
                let sample = seconds * fps;
                // Remove only multiplication noise around an authored integer frame.
                let assigned = if (sample - sample.round()).abs()
                    <= 4.0 * f64::EPSILON * sample.abs().max(1.0)
                {
                    sample.round()
                } else {
                    sample
                };
                let start = if field == "inPoint" {
                    assigned
                } else {
                    layer.in_frame_sample()
                };
                let end = if field == "outPoint" {
                    assigned
                } else {
                    layer.out_frame_sample(composition.duration())
                };
                if start >= 0.0
                    && start.fract() == 0.0
                    && end.fract() == 0.0
                    && end <= f64::from(composition.duration())
                {
                    self.apply(
                        comp,
                        Command::SetLayerRange {
                            id,
                            start: start as Frame,
                            end: end as Frame,
                        },
                    )
                } else {
                    self.apply(comp, Command::SetLayerRangeSamples { id, start, end })
                }
            }
            "startTime" => {
                let frame = origin_frame(self.composition(comp)?, number(value)?)?;
                if frame == self.layer(comp, id)?.start_frame() {
                    return Ok(Value::Null);
                }
                self.apply(comp, Command::SetLayerStart { id, frame })
            }
            "label" => {
                let index = number(value)?;
                if index.fract() != 0.0 || !(0.0..=16.0).contains(&index) {
                    return Err("Layer label must be an integer from 0 (None) through 16".into());
                }
                let index = index as u8;
                if index == self.layer(comp, id)?.label_index() {
                    return Ok(Value::Null);
                }
                self.apply(comp, Command::SetLayerLabel { id, index })
            }
            "threeDLayer" => self.apply(
                comp,
                Command::SetThreeD {
                    id,
                    enabled: value.as_bool().ok_or("threeDLayer must be boolean")?,
                },
            ),
            _ => Err(format!("Unsupported layer assignment: {field}")),
        }
    }

    fn property_get(
        &self,
        comp: CompositionId,
        id: LayerId,
        property: &str,
        metadata_only: bool,
    ) -> Result<Value, String> {
        let composition = self.composition(comp)?;
        let layer = self.layer(comp, id)?;
        if layer.is_three_d() {
            if property == "position" {
                return self.spatial_get(comp, id, metadata_only);
            }
            if property == "scale" {
                return Err("3D Scale scripting is outside the fixed-plane Position subset".into());
            }
        }
        let frame = self.frame.min(composition.duration().saturating_sub(1));
        let target = expression_target(property);
        let program = target.and_then(|target| layer.expression(target));
        let enabled = program.is_some_and(|program| program.enabled);
        let inactive_animation = comp != self.project.active_composition_id()
            && (enabled
                || match property {
                    "position" => {
                        if let Some(position) = layer.planar_position() {
                            !position.keys.is_empty()
                        } else {
                            !layer
                                .property(Property::PositionX)
                                .expect("validated scalar host property")
                                .keys()
                                .is_empty()
                                || !layer
                                    .property(Property::PositionY)
                                    .expect("validated scalar host property")
                                    .keys()
                                    .is_empty()
                        }
                    }
                    "opacity" => layer.opacity_key_count() != 0,
                    "sourceText" => layer.source_text_animation().animated(),
                    "scale" => {
                        !layer
                            .property(Property::ScaleX)
                            .expect("validated scalar host property")
                            .keys()
                            .is_empty()
                            || !layer
                                .property(Property::ScaleY)
                                .expect("validated scalar host property")
                                .keys()
                                .is_empty()
                    }
                    _ => false,
                });
        if inactive_animation && !metadata_only {
            return Err("Cannot sample an inactive composition's animated value: its playhead is not available to this script".into());
        }
        let (value, keys, value_type) = match property {
            "position" => {
                let frames = property_frames(layer, property)?;
                (
                    if metadata_only {
                        Value::Null
                    } else {
                        json!(layer.position2_at(frame, composition.fps().seconds(1))?)
                    },
                    frames.len(),
                    "TwoD_SPATIAL",
                )
            }
            "scale" => (
                json!([
                    layer
                        .property(Property::ScaleX)
                        .expect("validated scalar host property")
                        .value_at(frame),
                    layer
                        .property(Property::ScaleY)
                        .expect("validated scalar host property")
                        .value_at(frame)
                ]),
                property_frames(layer, property)?.len(),
                "TwoD",
            ),
            "opacity" => (
                if metadata_only {
                    Value::Null
                } else {
                    json!(layer.opacity_at(frame, composition.fps().seconds(1))?)
                },
                layer.opacity_key_count(),
                "OneD",
            ),
            "sourceText" => {
                let text = layer
                    .source_text_at(frame)
                    .ok_or("Source Text requires a text layer")?;
                let keys = layer
                    .track(libre_effects_core::PropertyPath::SourceText)
                    .map_or(0, |track| track.keys().len());
                (json!({"text": text}), keys, "TEXT_DOCUMENT")
            }
            "marker" => (Value::Null, layer.markers().len(), "MARKER"),
            _ => return Err(format!("Unsupported property: {property}")),
        };
        let value = if metadata_only || inactive_animation {
            Value::Null
        } else if enabled {
            // This host is executed inside the application's killable JSX worker.
            // Only immutable snapshots enter the separate read-only expression VM.
            use libre_effects_core::expression_runtime as ae;
            let property = match target.unwrap() {
                libre_effects_core::ExpressionTarget::Position => ae::ExpressionProperty::Position,
                libre_effects_core::ExpressionTarget::Scale => ae::ExpressionProperty::Scale,
                libre_effects_core::ExpressionTarget::Opacity => ae::ExpressionProperty::Opacity,
                _ => unreachable!(),
            };
            let address = ae::PropertyAddress {
                composition: ae::CompositionId(comp),
                layer: ae::LayerId(id),
                property,
            };
            let snapshot = self.project.expression_snapshot(comp, frame)?;
            let evaluated = ae::ExpressionEvaluator::default()
                .evaluate(&snapshot, std::slice::from_ref(&address))
                .map_err(|error| format!("Expression evaluation failed: {error}"))?;
            serde_json::to_value(
                evaluated
                    .values
                    .get(&address)
                    .ok_or("Expression did not return the requested property")?,
            )
            .map_err(|error| error.to_string())?
        } else {
            value
        };
        Ok(
            json!({"value":value,"numKeys":keys,"propertyValueType":value_type,"dimensionsSeparated":false,
            "expression":target.map(|_|program.map_or("",|program|program.source.as_str())),"expressionEnabled":enabled}),
        )
    }

    fn property_set(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let property = text(args, "property")?;
        if property == "opacity" {
            return self.opacity_set(comp, id, args);
        }
        if property == "position" && self.layer(comp, id)?.planar_position().is_some() {
            return self.planar_set(comp, id, args);
        }
        if self.layer(comp, id)?.is_three_d() {
            if property == "position" {
                return self.spatial_set(comp, id, args);
            }
            if property == "scale" {
                return Err("3D Scale scripting is outside the fixed-plane Position subset".into());
            }
        }
        if property == "marker" {
            return self.marker_set(comp, id, args);
        }
        let value = args.get("value").ok_or("Missing property value")?;
        if property == "sourceText" {
            if args.get("time").is_some()
                || self.layer(comp, id)?.source_text_animation().animated()
            {
                return Err(
                    "Animated Source Text assignment is outside this scripting subset".into(),
                );
            }
            let object = value
                .as_object()
                .ok_or("Source Text requires a TextDocument with a text field")?;
            if object.len() != 1 || !object.contains_key("text") {
                return Err(
                    "Only TextDocument.text is supported; rich text attributes are not discarded"
                        .into(),
                );
            }
            let value = text(value, "text")?;
            if self.layer(comp, id)?.source_text_at(0) == Some(value) {
                return Ok(Value::Null);
            }
            return self.apply(
                comp,
                Command::EditSourceText {
                    id,
                    frame: 0,
                    text: value.into(),
                },
            );
        }
        let properties = property_components(property)?;
        property_frames(self.layer(comp, id)?, property)?;
        let values = property_values(property, value)?;
        let timed = args.get("time");
        let frame = match timed {
            Some(time) => frame_at(self.composition(comp)?, number(time)?, false)?,
            None => 0,
        };
        if timed.is_none()
            && properties.iter().any(|property| {
                !self
                    .layer(comp, id)
                    .unwrap()
                    .property(*property)
                    .expect("validated scalar host property")
                    .keys()
                    .is_empty()
            })
        {
            return Err("setValue cannot replace an animated property; use setValueAtTime or remove its keys".into());
        }
        let mut commands = Vec::new();
        for (property, value) in properties.into_iter().zip(values) {
            let track = self
                .layer(comp, id)?
                .property(property)
                .expect("validated scalar host property");
            if timed.is_some() && !track.keys().contains_key(&frame) {
                commands.push(Command::ToggleKeyframe {
                    id,
                    property,
                    frame,
                });
            }
            if timed.is_some() || track.value_at(0) != value {
                commands.push(Command::SetValue {
                    id,
                    property,
                    frame,
                    value,
                });
            }
        }
        self.apply(comp, Command::Batch(commands))
    }

    fn marker_set(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let composition = self.composition(comp)?;
        let frame = frame_at(
            composition,
            number(args.get("time").ok_or("Marker time is required")?)?,
            false,
        )?;
        let value = args.get("value").ok_or("Marker value is required")?;
        let object = value.as_object().ok_or("MarkerValue is required")?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "comment" | "duration"))
        {
            return Err("Only MarkerValue.comment and duration are supported".into());
        }
        let name = text(value, "comment")?.to_owned();
        let duration = match value.get("duration") {
            Some(value) => duration_frames(composition, number(value)?)?,
            None => 0,
        };
        let target = MarkerTarget::Layer(id);
        let existing = self
            .layer(comp, id)?
            .markers()
            .iter()
            .find(|marker| marker.frame() == frame);
        if existing.is_some_and(|marker| marker.name() == name && marker.duration() == duration) {
            return Ok(Value::Null);
        }
        let marker_id = if let Some(marker) = existing {
            marker.id()
        } else {
            self.apply(
                comp,
                Command::Marker {
                    target,
                    edit: MarkerEdit::Add { frame },
                },
            )?;
            self.layer(comp, id)?
                .markers()
                .iter()
                .find(|marker| marker.frame() == frame)
                .ok_or("Marker was not created")?
                .id()
        };
        let color = self
            .layer(comp, id)?
            .markers()
            .iter()
            .find(|marker| marker.id() == marker_id)
            .ok_or("Marker no longer exists")?
            .color();
        self.apply(
            comp,
            Command::Marker {
                target,
                edit: MarkerEdit::Update {
                    id: marker_id,
                    frame,
                    duration,
                    name,
                    color,
                },
            },
        )
    }

    fn property_key_metadata(
        &self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let property = text(args, "property")?;
        let layer = self.layer(comp, id)?;
        let frames = property_frames(layer, property)?;
        let index = integer(args, "index")?
            .checked_sub(1)
            .ok_or("Key indexes are 1-based")?;
        let index = usize::try_from(index).map_err(|_| "Key index is out of range")?;
        let frame = *frames.get(index).ok_or("Key index is out of range")?;
        let encoded = match property {
            "opacity" => serde_json::to_value(
                layer
                    .opacity_timing()
                    .and_then(|timing| timing.keys().get(&frame))
                    .ok_or("Key metadata requires native per-side Opacity timing")?,
            ),
            "position" => {
                if let Some(position) = layer.planar_position() {
                    serde_json::to_value(position.keys.get(&frame).ok_or("Position key not found")?)
                } else if let Some(position) = layer.spatial_position() {
                    serde_json::to_value(position.keys.get(&frame).ok_or("Position key not found")?)
                } else {
                    return Err("Key metadata requires native joined Position; legacy axes are not inferred".into());
                }
            }
            _ => {
                return Err(
                    "Key metadata reads currently support native Position and Opacity".into(),
                );
            }
        };
        encoded.map_err(|error| error.to_string())
    }

    fn property_key(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let property = text(args, "property")?;
        if property == "opacity" {
            return self.opacity_key(comp, id, args);
        }
        if property == "position" && self.layer(comp, id)?.planar_position().is_some() {
            return self.planar_key(comp, id, args);
        }
        let action = text(args, "action")?;
        if self.layer(comp, id)?.is_three_d() {
            if property == "position" {
                return self.spatial_key(comp, id, args);
            }
            if property == "scale" {
                return Err("3D Scale scripting is outside the fixed-plane Position subset".into());
            }
        }
        let layer = self.layer(comp, id)?;
        let frames = property_frames(layer, property)?;
        if action == "nearest" {
            let time = number(args.get("time").ok_or("Missing key time")?)?;
            let fps = self.composition(comp)?.fps().as_f64();
            return frames
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    ((f64::from(**a) / fps) - time)
                        .abs()
                        .total_cmp(&((f64::from(**b) / fps) - time).abs())
                })
                .map(|(index, _)| json!(index + 1))
                .ok_or_else(|| "Property has no keys".into());
        }
        let index = integer(args, "index")?;
        let frame = *frames
            .get(index.checked_sub(1).ok_or("Key indexes are 1-based")? as usize)
            .ok_or("Key index is out of range")?;
        match action {
            "get_time" => Ok(json!(self.composition(comp)?.fps().seconds(frame.into()))),
            "get_value" => match property {
                "marker" => {
                    let marker = &layer.markers()[index as usize - 1];
                    Ok(
                        json!({"comment": marker.name(), "duration": self.composition(comp)?.fps().seconds(marker.duration().into())}),
                    )
                }
                "sourceText" => Ok(
                    json!({"text": layer.source_text_at(frame).ok_or("Source Text requires a text layer")?}),
                ),
                "position" => Ok(json!([
                    layer
                        .property(Property::PositionX)
                        .expect("validated scalar host property")
                        .value_at(frame),
                    layer
                        .property(Property::PositionY)
                        .expect("validated scalar host property")
                        .value_at(frame)
                ])),
                "scale" => Ok(json!([
                    layer
                        .property(Property::ScaleX)
                        .expect("validated scalar host property")
                        .value_at(frame),
                    layer
                        .property(Property::ScaleY)
                        .expect("validated scalar host property")
                        .value_at(frame)
                ])),
                "opacity" => Ok(json!(
                    layer
                        .property(Property::Opacity)
                        .expect("validated scalar host property")
                        .value_at(frame)
                )),
                _ => Err(format!("Unsupported property: {property}")),
            },
            "remove" => {
                if property == "marker" {
                    let marker_id = layer.markers()[index as usize - 1].id();
                    return self.apply(
                        comp,
                        Command::Marker {
                            target: MarkerTarget::Layer(id),
                            edit: MarkerEdit::Remove(marker_id),
                        },
                    );
                }
                let commands = property_components(property)?
                    .into_iter()
                    .map(|property| Command::ToggleKeyframe {
                        id,
                        property,
                        frame,
                    })
                    .collect();
                self.apply(comp, Command::Batch(commands))
            }
            "interpolation" => {
                let incoming = text(args, "inType")?;
                let outgoing = if args.get("outType").is_some() {
                    text(args, "outType")?
                } else {
                    incoming
                };
                if incoming != outgoing {
                    return Err(
                        "Mixed incoming/outgoing AE interpolation types are unsupported".into(),
                    );
                }
                let interpolation = match outgoing {
                    "LINEAR" => Interpolation::Linear,
                    "HOLD" => Interpolation::Hold,
                    // There is no AE Bezier-mode field in the source format. A
                    // caller can set explicit scalar temporal ease instead.
                    "BEZIER" => return Err("AE Bezier interpolation mode is unsupported; use explicit scalar temporal ease".into()),
                    _ => return Err("Unknown interpolation type".into()),
                };
                // A source segment owns one interpolation type. AE has separate
                // incoming/outgoing types; refuse an incoming change that cannot
                // be represented without changing the preceding key's type.
                for component in property_components(property)? {
                    let track = layer
                        .property(component)
                        .expect("validated scalar host property");
                    if track
                        .keys()
                        .range(..frame)
                        .next_back()
                        .is_some_and(|(_, key)| {
                            key.interpolation != interpolation || key.temporal.outgoing.is_some()
                        })
                        || track.keys()[&frame].temporal.incoming.is_some()
                    {
                        return Err(
                            "Changing an independent incoming AE interpolation type is unsupported"
                                .into(),
                        );
                    }
                }
                let commands = property_components(property)?
                    .into_iter()
                    .map(|property| Command::SetInterpolation {
                        id,
                        property,
                        frame,
                        interpolation,
                    })
                    .collect();
                self.apply(comp, Command::Batch(commands))
            }
            "ease" => {
                Err("Temporal ease requires native scalar Opacity or joined XY/XYZ Position".into())
            }
            "temporal_continuous" | "temporal_auto_bezier" => {
                if args.get("value").and_then(Value::as_bool) != Some(false) {
                    return Err(
                        "AE automatic/continuous temporal interpolation is unsupported".into(),
                    );
                }
                if property_components(property)?.iter().any(|property| {
                    layer
                        .property(*property)
                        .expect("validated scalar host property")
                        .keys()[&frame]
                        .temporal
                        .mode
                        != TemporalMode::Independent
                }) {
                    return Err(
                        "Native automatic/continuous key modes cannot be relabeled as AE modes"
                            .into(),
                    );
                }
                Ok(Value::Null)
            }
            "spatial_tangents" | "spatial_continuous" | "spatial_auto_bezier" => Err(
                "Spatial Bezier tangents and automatic spatial interpolation are unsupported"
                    .into(),
            ),
            _ => Err(format!("Unsupported keyframe operation: {action}")),
        }
    }
}

fn origin_frame(comp: &Composition, seconds: f64) -> Result<i64, String> {
    let frame = seconds * comp.fps().as_f64();
    if !frame.is_finite() || frame.abs() > 100_000_000.0 || (frame - frame.round()).abs() > 1e-6 {
        return Err("startTime requires a frame-aligned signed origin within 100000000 frames; subframes are not rounded".into());
    }
    Ok(frame.round() as i64)
}

fn integer(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .filter(|number| *number <= MAX_SAFE_INTEGER)
        .ok_or_else(|| format!("{field} must be a nonnegative exact integer"))
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} must be text"))
}
fn number(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| "Expected a finite number".into())
}
fn duration_frames(comp: &Composition, seconds: f64) -> Result<Frame, String> {
    let frame = seconds * comp.fps().as_f64();
    if !frame.is_finite()
        || frame < 0.0
        || frame > f64::from(u32::MAX)
        || (frame - frame.round()).abs() > 1e-6
    {
        return Err("This scripting subset requires nonnegative frame-aligned times; subframes are not rounded".into());
    }
    Ok(frame.round() as Frame)
}
fn frame_at(comp: &Composition, seconds: f64, allow_end: bool) -> Result<Frame, String> {
    let frame = duration_frames(comp, seconds)?;
    if frame > comp.duration() || (!allow_end && frame == comp.duration()) {
        return Err("Time is outside the composition".into());
    }
    Ok(frame)
}
fn expression_target(property: &str) -> Option<libre_effects_core::ExpressionTarget> {
    use libre_effects_core::ExpressionTarget as T;
    match property {
        "position" => Some(T::Position),
        "scale" => Some(T::Scale),
        "opacity" => Some(T::Opacity),
        _ => None,
    }
}
fn property_components(property: &str) -> Result<Vec<Property>, String> {
    match property {
        "position" => Ok(vec![Property::PositionX, Property::PositionY]),
        "scale" => Ok(vec![Property::ScaleX, Property::ScaleY]),
        "opacity" => Ok(vec![Property::Opacity]),
        _ => Err(format!("Unsupported numeric property: {property}")),
    }
}
fn property_values(property: &str, value: &Value) -> Result<Vec<f64>, String> {
    match property {
        "position" | "scale" => {
            let values = value
                .as_array()
                .ok_or("Position requires exactly two coordinates")?;
            if values.len() != 2 {
                return Err(
                    "Only 2D Position is supported; Z coordinates are never discarded".into(),
                );
            }
            values.iter().map(number).collect()
        }
        "opacity" => Ok(vec![number(value)?]),
        _ => Err(format!("Unsupported numeric property: {property}")),
    }
}
fn property_frames(layer: &Layer, property: &str) -> Result<Vec<Frame>, String> {
    match property {
        "position" if layer.is_three_d() => Ok(layer
            .spatial_position()
            .ok_or("3D Position requires native joined XYZ data")?
            .keys
            .keys()
            .copied()
            .collect()),
        "position" if layer.planar_position().is_some() => Ok(layer
            .planar_position()
            .expect("checked joined XY Position")
            .keys
            .keys()
            .copied()
            .collect()),
        "marker" => Ok(layer
            .markers()
            .iter()
            .map(|marker| marker.frame())
            .collect()),
        "sourceText" => Ok(layer
            .track(libre_effects_core::PropertyPath::SourceText)
            .ok_or("Source Text requires a text layer")?
            .keys()
            .keys()
            .copied()
            .collect()),
        "position" | "scale" => {
            let components = property_components(property)?;
            let x: Vec<_> = layer
                .property(components[0])
                .expect("validated scalar host property")
                .keys()
                .keys()
                .copied()
                .collect();
            let y: Vec<_> = layer
                .property(components[1])
                .expect("validated scalar host property")
                .keys()
                .keys()
                .copied()
                .collect();
            if x != y {
                return Err("Position has independently timed X/Y keys; joined AE spatial keys are unavailable".into());
            }
            if layer
                .property(components[0])
                .expect("validated scalar host property")
                .keys()
                .values()
                .zip(
                    layer
                        .property(components[1])
                        .expect("validated scalar host property")
                        .keys()
                        .values(),
                )
                .any(|(x, y)| {
                    x.interpolation != y.interpolation
                        || !x.temporal.is_empty()
                        || !y.temporal.is_empty()
                })
            {
                return Err("Position has independent scalar easing; AE spatial interpolation is unavailable".into());
            }
            Ok(x)
        }
        "opacity" => Ok(if let Some(timing) = layer.opacity_timing() {
            timing.keys().keys().copied().collect()
        } else {
            layer
                .property(Property::Opacity)
                .ok_or("Missing Opacity value source")?
                .keys()
                .keys()
                .copied()
                .collect()
        }),
        _ => Err(format!("Unsupported property: {property}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, OpacityEase};

    fn fixture() -> Editor {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor.clear_history();
        editor
    }

    #[test]
    fn snapshot_uses_stable_ids_and_one_based_stack_indexes() {
        let mut editor = fixture();
        editor.execute(Command::AddRectangle).unwrap();
        let host = AutomationHost::new(editor.project(), &[1], 15).unwrap();
        let snapshot = host.snapshot();
        assert_eq!(snapshot["items"][0]["layers"][0]["id"], 2);
        assert_eq!(snapshot["items"][0]["layers"][0]["index"], 1);
        assert_eq!(snapshot["items"][0]["layers"][1]["selected"], true);
        assert_eq!(snapshot["items"][0]["time"], 0.5);
    }

    #[test]
    fn multiple_compositions_commit_as_one_undo_and_preserve_active_selection() {
        let mut editor = fixture();
        editor.execute(Command::DuplicateComposition).unwrap();
        editor.activate_composition(1).unwrap();
        editor.select(1);
        editor.clear_history();
        let original = editor.project().clone();
        let mut host = AutomationHost::new(&original, &[1], 0).unwrap();
        host.dispatch(
            "layer_set",
            json!({"comp":1,"id":1,"field":"name","value":"First"}),
        )
        .unwrap();
        host.dispatch(
            "layer_set",
            json!({"comp":2,"id":2,"field":"name","value":"Second"}),
        )
        .unwrap();
        let copy = host
            .dispatch("layer_duplicate", json!({"comp":2,"id":2}))
            .unwrap()
            .as_u64()
            .unwrap();
        host.dispatch(
            "layer_set",
            json!({"comp":2,"id":copy,"field":"enabled","value":false}),
        )
        .unwrap();
        let candidate = host.finish().unwrap();
        assert_eq!(editor.project(), &original);
        assert!(editor.commit_automation_project(candidate.clone()).unwrap());
        assert_eq!(editor.project().active_composition_id(), 1);
        assert_eq!(editor.selected(), Some(1));
        editor.undo();
        assert_eq!(editor.project(), &original);
        assert!(!editor.can_undo());
        editor.redo();
        assert_eq!(editor.project(), &candidate);
    }

    #[test]
    fn caught_error_poison_prevents_partial_commit() {
        let editor = fixture();
        let original = editor.project().clone();
        let mut host = AutomationHost::new(&original, &[], 0).unwrap();
        host.dispatch(
            "layer_set",
            json!({"comp":1,"id":1,"field":"name","value":"Draft only"}),
        )
        .unwrap();
        assert!(
            host.dispatch(
                "layer_set",
                json!({"comp":1,"id":1,"field":"label","value":17})
            )
            .is_err()
        );
        assert!(host.dispatch("snapshot", json!({})).is_err());
        assert!(host.finish().is_err());
        assert_eq!(editor.project(), &original);
    }

    #[test]
    fn timed_values_create_keys_without_rounding_or_discarding_z() {
        let editor = fixture();
        let mut host = AutomationHost::new(editor.project(), &[], 0).unwrap();
        host.dispatch(
            "property_set",
            json!({"comp":1,"id":1,"property":"position","time":0.5,"value":[20,30]}),
        )
        .unwrap();
        host.dispatch(
            "property_set",
            json!({"comp":1,"id":1,"property":"position","time":1.0,"value":[40,50]}),
        )
        .unwrap();
        assert_eq!(
            host.dispatch(
                "property_key",
                json!({"comp":1,"id":1,"property":"position","action":"get_time","index":1})
            )
            .unwrap(),
            0.5
        );
        assert_eq!(
            host.dispatch(
                "property_key",
                json!({"comp":1,"id":1,"property":"position","action":"nearest","time":0.9})
            )
            .unwrap(),
            2
        );
        let candidate = host.finish().unwrap();
        let layer = candidate.composition().layer(1).unwrap();
        assert_eq!(
            layer
                .property(Property::PositionX)
                .expect("validated scalar host property")
                .value_at(15),
            20.0
        );
        assert_eq!(
            layer
                .property(Property::PositionY)
                .expect("validated scalar host property")
                .value_at(30),
            50.0
        );
        assert_eq!(
            layer
                .property(Property::PositionX)
                .expect("validated scalar host property")
                .keys()
                .len(),
            2
        );
        for args in [
            json!({"comp":1,"id":1,"property":"position","value":[1,2,3]}),
            json!({"comp":1,"id":1,"property":"opacity","time":0.01,"value":30}),
        ] {
            let mut rejected = AutomationHost::new(editor.project(), &[], 0).unwrap();
            assert!(rejected.dispatch("property_set", args).is_err());
            assert!(rejected.finish().is_err());
        }
    }

    #[test]
    fn markers_replace_existing_time_and_survive_json_round_trip() {
        let editor = fixture();
        let mut host = AutomationHost::new(editor.project(), &[], 0).unwrap();
        for comment in ["Old", "Focus"] {
            host.dispatch(
                "marker_set",
                json!({"comp":1,"id":1,"time":0.5,"value":{"comment":comment,"duration":0.5}}),
            )
            .unwrap();
        }
        let candidate = host.finish().unwrap();
        let roundtrip = Project::from_json(&candidate.to_json().unwrap()).unwrap();
        let markers = roundtrip.composition().layer(1).unwrap().markers();
        assert_eq!(markers.len(), 1);
        assert_eq!(
            (markers[0].frame(), markers[0].duration(), markers[0].name()),
            (15, 15, "Focus")
        );
    }

    #[test]
    fn no_op_and_invalid_candidate_preserve_redo_and_context() {
        let mut editor = fixture();
        editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Temporary".into(),
            })
            .unwrap();
        editor.undo();
        let original = editor.project().clone();
        let receipt = editor.context_generation();
        let mut host = AutomationHost::new(&original, &[], 0).unwrap();
        host.dispatch(
            "layer_set",
            json!({"comp":1,"id":1,"field":"enabled","value":true}),
        )
        .unwrap();
        host.dispatch(
            "property_set",
            json!({"comp":1,"id":1,"property":"opacity","value":100}),
        )
        .unwrap();
        assert!(
            !editor
                .commit_automation_project(host.finish().unwrap())
                .unwrap()
        );
        let mut invalid = serde_json::to_value(&original).unwrap();
        invalid["composition"]["width"] = json!(0);
        let invalid = serde_json::from_value(invalid).unwrap();
        assert!(editor.commit_automation_project(invalid).is_err());
        assert_eq!(editor.project(), &original);
        assert_eq!(editor.context_generation(), receipt);
        assert!(editor.can_redo());
        assert!(!editor.can_undo());
    }

    #[test]
    fn source_schema_and_unrelated_sources_are_preserved() {
        let editor = fixture();
        let mut source = serde_json::to_value(editor.project()).unwrap();
        source["version"] = json!(63);
        let original: Project = serde_json::from_value(source.clone()).unwrap();
        let mut host = AutomationHost::new(&original, &[], 0).unwrap();
        host.dispatch(
            "layer_set",
            json!({"comp":1,"id":1,"field":"name","value":"Only this name"}),
        )
        .unwrap();
        let actual = serde_json::to_value(host.finish().unwrap()).unwrap();
        source["composition"]["layers"][0]["name"] = json!("Only this name");
        assert_eq!(actual, source);
    }

    #[test]
    fn host_call_budget_is_latched() {
        let editor = fixture();
        let mut host = AutomationHost::new(editor.project(), &[], 0).unwrap();
        host.calls = MAX_HOST_CALLS;
        assert!(
            host.dispatch("snapshot", json!({}))
                .unwrap_err()
                .contains("limit")
        );
        assert!(host.finish().is_err());
    }

    #[test]
    fn opacity_interior_temporal_ease_preserves_signed_units_per_second() {
        let editor = fixture();
        let mut host = AutomationHost::new(editor.project(), &[], 0).unwrap();
        for (time, value) in [(0.0, 100.0), (1.0, 60.0), (2.0, 0.0)] {
            host.dispatch(
                "property_set",
                json!({"comp":1,"id":1,"property":"opacity","time":time,"value":value}),
            )
            .unwrap();
        }
        host.dispatch("property_key", json!({"comp":1,"id":1,"property":"opacity","action":"ease","index":2,"inEase":[{"speed":-30,"influence":25}],"outEase":[{"speed":-60,"influence":50}]})).unwrap();
        let candidate = host.finish().unwrap();
        let layer = candidate.composition().layer(1).unwrap();
        assert!(layer.property(Property::Opacity).is_none());
        let key = &layer.opacity_timing().unwrap().keys()[&30];
        assert_eq!(
            key.in_ease,
            OpacityEase {
                speed: -30.0,
                influence: 25.0
            }
        );
        assert_eq!(
            key.out_ease,
            OpacityEase {
                speed: -60.0,
                influence: 50.0
            }
        );
        assert_eq!(layer.opacity_key_value(30), Some(60.0));
    }

    #[test]
    fn unsupported_fields_and_spatial_attributes_are_never_silent_no_ops() {
        let editor = fixture();
        for (op, args) in [
            (
                "layer_set",
                json!({"comp":1,"id":1,"field":"startTime","value":0.001}),
            ),
            (
                "layer_set",
                json!({"comp":1,"id":1,"field":"label","value":17}),
            ),
            ("unsupported", json!({"name":"File.open"})),
        ] {
            let mut host = AutomationHost::new(editor.project(), &[], 0).unwrap();
            assert!(host.dispatch(op, args).is_err());
            assert!(host.finish().is_err());
        }
    }

    #[test]
    fn unicode_source_text_and_trim_round_trip_without_retiming_keys() {
        let mut editor = fixture();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Template".into(),
                    font_size: 48.0,
                },
                width: 640.0,
                height: 120.0,
                name: "Text".into(),
            })
            .unwrap();
        let mut host = AutomationHost::new(editor.project(), &[], 0).unwrap();
        host.dispatch("property_set", json!({"comp":1,"id":2,"property":"sourceText","value":{"text":"일본어\r発音\r한국어"}})).unwrap();
        host.dispatch(
            "property_set",
            json!({"comp":1,"id":2,"property":"opacity","time":1,"value":60}),
        )
        .unwrap();
        host.dispatch(
            "layer_set",
            json!({"comp":1,"id":2,"field":"inPoint","value":0.5}),
        )
        .unwrap();
        host.dispatch(
            "layer_set",
            json!({"comp":1,"id":2,"field":"outPoint","value":2}),
        )
        .unwrap();
        let candidate = host.finish().unwrap();
        let bytes = libre_effects_core::project_file::encode(&candidate, None).unwrap();
        let decoded = libre_effects_core::project_file::decode(&bytes).unwrap();
        assert_eq!(decoded.project, candidate);
        let layer = decoded.project.composition().layer(2).unwrap();
        assert_eq!(layer.source_text_at(0), Some("일본어\r発音\r한국어"));
        assert_eq!(layer.in_frame(), 15);
        assert_eq!(layer.out_frame(150), 60);
        assert!(
            layer
                .property(Property::Opacity)
                .expect("validated scalar host property")
                .keys()
                .contains_key(&30)
        );
    }

    #[test]
    fn project_item_count_includes_readonly_folders() {
        let mut editor = fixture();
        editor
            .execute(Command::NewProjectFolder {
                name: "Media".into(),
                parent: None,
            })
            .unwrap();
        let host = AutomationHost::new(editor.project(), &[], 0).unwrap();
        let snapshot = host.snapshot();
        assert_eq!(snapshot["items"].as_array().unwrap().len(), 2);
        assert_eq!(snapshot["items"][1]["type"], "folder");
        assert_eq!(snapshot["items"][1]["name"], "Media");
    }

    #[test]
    fn draft_command_failure_is_atomic_and_preserves_schema() {
        let editor = fixture();
        let mut candidate = editor.project().clone();
        let original = candidate.clone();
        assert!(
            candidate
                .apply_automation_command(
                    1,
                    Command::Batch(vec![
                        Command::RenameLayer {
                            id: 1,
                            name: "Must roll back".into()
                        },
                        Command::SetLayerRange {
                            id: 1,
                            start: 149,
                            end: 148
                        },
                    ])
                )
                .is_err()
        );
        assert_eq!(candidate, original);
        assert!(
            candidate
                .apply_automation_command(1, Command::AddRectangle)
                .is_err()
        );
        assert_eq!(candidate, original);
    }

    #[test]
    fn temporal_ease_does_not_silently_replace_hold_segments() {
        let mut editor = fixture();
        for frame in [0, 30, 60] {
            editor
                .execute(Command::ToggleKeyframe {
                    id: 1,
                    property: Property::Opacity,
                    frame,
                })
                .unwrap();
        }
        editor
            .execute(Command::SetInterpolation {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                interpolation: Interpolation::Hold,
            })
            .unwrap();
        let original = editor.project().clone();
        let mut host = AutomationHost::new(&original, &[], 0).unwrap();
        let error = host.dispatch("property_key", json!({"comp":1,"id":1,"property":"opacity","action":"ease","index":2,"inEase":[{"speed":0,"influence":33}],"outEase":[{"speed":0,"influence":33}]})).unwrap_err();
        assert!(error.contains("promotion requires Linear"), "{error}");
        assert!(host.finish().is_err());
        assert_eq!(editor.project(), &original);
    }

    #[test]
    fn inactive_animation_requires_its_own_playhead_but_metadata_and_static_reads_work() {
        let mut editor = fixture();
        editor.execute(Command::DuplicateComposition).unwrap();
        editor
            .execute(Command::ToggleKeyframe {
                id: 2,
                property: Property::Opacity,
                frame: 0,
            })
            .unwrap();
        editor.activate_composition(1).unwrap();
        let mut host = AutomationHost::new(editor.project(), &[], 15).unwrap();
        assert!(host.snapshot()["items"][1]["time"].is_null());
        let metadata = host
            .dispatch(
                "property_get",
                json!({"comp":2,"id":2,"property":"opacity","field":"metadata"}),
            )
            .unwrap();
        assert_eq!(metadata["numKeys"], 1);
        assert!(metadata["value"].is_null());
        assert!(
            host.dispatch(
                "property_get",
                json!({"comp":2,"id":2,"property":"position"})
            )
            .is_ok()
        );
        assert!(
            host.dispatch(
                "property_get",
                json!({"comp":2,"id":2,"property":"opacity","field":"value"})
            )
            .unwrap_err()
            .contains("inactive")
        );
        assert!(host.finish().is_err());
    }
}
