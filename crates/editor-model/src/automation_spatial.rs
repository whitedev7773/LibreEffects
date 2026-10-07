//! AE-shaped joined Position calls backed by the native vector source.
use super::*;
use libre_effects_core::{SpatialEase, SpatialEdit, SpatialInterpolation};

fn vector(value: &Value) -> Result<[f64; 3], String> {
    let items = value
        .as_array()
        .ok_or("Joined spatial Position requires an XYZ array")?;
    if items.len() != 3 {
        return Err("Joined spatial Position requires exactly three coordinates".into());
    }
    Ok([number(&items[0])?, number(&items[1])?, number(&items[2])?])
}
fn interpolation(value: &str) -> Result<SpatialInterpolation, String> {
    match value {
        "LINEAR" => Ok(SpatialInterpolation::Linear),
        "BEZIER" => Ok(SpatialInterpolation::Bezier),
        "HOLD" => Ok(SpatialInterpolation::Hold),
        _ => Err("Unsupported spatial interpolation type".into()),
    }
}
fn ease(args: &Value, field: &str) -> Result<SpatialEase, String> {
    let values = args
        .get(field)
        .and_then(Value::as_array)
        .ok_or("Spatial temporal ease must be an array")?;
    if values.len() != 1 {
        return Err("Joined spatial Position requires one KeyframeEase per side".into());
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
    pub(super) fn spatial_get(
        &self,
        comp: CompositionId,
        id: LayerId,
        metadata_only: bool,
    ) -> Result<Value, String> {
        let composition = self.composition(comp)?;
        let layer = self.layer(comp, id)?;
        let position = layer
            .spatial_position()
            .ok_or("Layer has no joined XYZ Position")?;
        if comp != self.project.active_composition_id()
            && !position.keys.is_empty()
            && !metadata_only
        {
            return Err("Cannot sample an inactive composition's animated value: its playhead is not available to this script".into());
        }
        let frame = self.frame.min(composition.duration().saturating_sub(1));
        let value = if metadata_only {
            Value::Null
        } else {
            json!(layer.position3_at(frame, composition.fps().seconds(1))?)
        };
        let program = layer.expression(libre_effects_core::ExpressionTarget::Position);
        Ok(
            json!({"value":value,"numKeys":position.keys.len(),"propertyValueType":"ThreeD_SPATIAL","dimensionsSeparated":false,
            "expression":program.map_or("",|program|program.source.as_str()),"expressionEnabled":false}),
        )
    }
    pub(super) fn spatial_set(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let value = vector(args.get("value").ok_or("Missing Position value")?)?;
        let edit = match args.get("time") {
            Some(time) => SpatialEdit::Key {
                frame: frame_at(self.composition(comp)?, number(time)?, false)?,
                value,
            },
            None => SpatialEdit::Value(value),
        };
        self.apply(comp, Command::SetSpatialPosition { id, edit })
    }
    pub(super) fn spatial_key(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let action = text(args, "action")?;
        let position = self
            .layer(comp, id)?
            .spatial_position()
            .ok_or("Layer has no joined XYZ Position")?;
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
            "remove" => SpatialEdit::RemoveKey { frame },
            "interpolation" => SpatialEdit::Interpolation {
                frame,
                incoming: interpolation(text(args, "inType")?)?,
                outgoing: interpolation(text(args, "outType")?)?,
            },
            "ease" => SpatialEdit::TemporalEase {
                frame,
                incoming: ease(args, "inEase")?,
                outgoing: ease(args, "outEase")?,
            },
            "temporal_continuous" => SpatialEdit::TemporalContinuous {
                frame,
                value: boolean()?,
            },
            "temporal_auto_bezier" => SpatialEdit::TemporalAutoBezier {
                frame,
                value: boolean()?,
            },
            "spatial_tangents" => SpatialEdit::Tangents {
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
            "spatial_continuous" => SpatialEdit::SpatialContinuous {
                frame,
                value: boolean()?,
            },
            "spatial_auto_bezier" => SpatialEdit::SpatialAutoBezier {
                frame,
                value: boolean()?,
            },
            _ => return Err(format!("Unsupported spatial keyframe operation: {action}")),
        };
        self.apply(comp, Command::SetSpatialPosition { id, edit })
    }
}
