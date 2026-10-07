//! AE-shaped joined XY Position edits backed by the authored planar source.
//! Reads share the numeric property path so enabled expressions stay truthful.
use super::*;
use libre_effects_core::{PlanarEdit, SpatialEase, SpatialInterpolation};

fn vector(value: &Value) -> Result<[f64; 2], String> {
    let items = value
        .as_array()
        .ok_or("Joined planar Position requires an XY array")?;
    if items.len() != 2 {
        return Err(
            "Joined planar Position requires exactly two coordinates; Z is never discarded".into(),
        );
    }
    Ok([number(&items[0])?, number(&items[1])?])
}

fn interpolation(value: &str) -> Result<SpatialInterpolation, String> {
    match value {
        "LINEAR" => Ok(SpatialInterpolation::Linear),
        "BEZIER" => Ok(SpatialInterpolation::Bezier),
        "HOLD" => Ok(SpatialInterpolation::Hold),
        _ => Err("Unsupported planar interpolation type".into()),
    }
}

fn ease(args: &Value, field: &str) -> Result<SpatialEase, String> {
    let values = args
        .get(field)
        .and_then(Value::as_array)
        .ok_or("Planar temporal ease must be an array")?;
    if values.len() != 1 {
        return Err("Joined planar Position requires one KeyframeEase per side".into());
    }
    let value = values[0]
        .as_object()
        .ok_or("KeyframeEase must be an object")?;
    if value.len() != 2 || !value.contains_key("speed") || !value.contains_key("influence") {
        return Err("Only KeyframeEase speed and influence are supported".into());
    }
    let result = SpatialEase {
        speed: number(&value["speed"])?,
        influence: number(&value["influence"])?,
    };
    result.validate().map_err(|error| error.to_string())?;
    Ok(result)
}

impl AutomationHost {
    pub(super) fn planar_set(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let value = vector(args.get("value").ok_or("Missing Position value")?)?;
        let edit = match args.get("time") {
            Some(time) => PlanarEdit::Key {
                frame: frame_at(self.composition(comp)?, number(time)?, false)?,
                value,
            },
            None => PlanarEdit::Value(value),
        };
        self.apply(comp, Command::EditPlanarPosition { id, edit })
    }

    pub(super) fn planar_key(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let action = text(args, "action")?;
        let position = self
            .layer(comp, id)?
            .planar_position()
            .ok_or("Layer has no joined XY Position")?;
        let frames = position.keys.keys().copied().collect::<Vec<_>>();
        if action == "nearest" {
            let time = number(args.get("time").ok_or("Missing key time")?)?;
            let fps = self.composition(comp)?.fps().as_f64();
            return frames
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    (f64::from(**a) / fps - time)
                        .abs()
                        .total_cmp(&(f64::from(**b) / fps - time).abs())
                        .then_with(|| a.cmp(b))
                })
                .map(|(index, _)| json!(index + 1))
                .ok_or_else(|| "Property has no keys".into());
        }
        let index = integer(args, "index")?
            .checked_sub(1)
            .ok_or("Key indexes are 1-based")?;
        let index = usize::try_from(index).map_err(|_| "Key index is out of range")?;
        let frame = *frames.get(index).ok_or("Key index is out of range")?;
        if action == "get_time" {
            return Ok(json!(self.composition(comp)?.fps().seconds(frame.into())));
        }
        if action == "get_value" {
            return Ok(json!(position.keys[&frame].value));
        }
        let boolean = || {
            args.get("value")
                .and_then(Value::as_bool)
                .ok_or_else(|| "Keyframe flag must be boolean".to_string())
        };
        let edit = match action {
            "remove" => PlanarEdit::RemoveKey { frame },
            "interpolation" => PlanarEdit::Interpolation {
                frame,
                incoming: interpolation(text(args, "inType")?)?,
                outgoing: interpolation(if args.get("outType").is_some() {
                    text(args, "outType")?
                } else {
                    text(args, "inType")?
                })?,
            },
            "ease" => PlanarEdit::TemporalEase {
                frame,
                incoming: ease(args, "inEase")?,
                outgoing: ease(args, "outEase")?,
            },
            "temporal_continuous" => PlanarEdit::TemporalContinuous {
                frame,
                value: boolean()?,
            },
            "temporal_auto_bezier" => PlanarEdit::TemporalAutoBezier {
                frame,
                value: boolean()?,
            },
            "spatial_tangents" => PlanarEdit::Tangents {
                frame,
                incoming: vector(
                    args.get("inTangent")
                        .ok_or("Missing incoming spatial tangent")?,
                )?,
                outgoing: vector(
                    args.get("outTangent")
                        .ok_or("Missing outgoing spatial tangent")?,
                )?,
            },
            "spatial_continuous" => PlanarEdit::SpatialContinuous {
                frame,
                value: boolean()?,
            },
            "spatial_auto_bezier" => PlanarEdit::SpatialAutoBezier {
                frame,
                value: boolean()?,
            },
            _ => return Err(format!("Unsupported planar keyframe operation: {action}")),
        };
        self.apply(comp, Command::EditPlanarPosition { id, edit })
    }
}
