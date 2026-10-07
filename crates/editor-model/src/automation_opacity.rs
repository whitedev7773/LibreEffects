//! Native per-side scalar timing; raw speed/sec is never rescaled for storage.
use super::*;
use libre_effects_core::{OpacityEase, OpacityEdit, OpacityInterpolation};

fn interpolation(value: &str) -> Result<OpacityInterpolation, String> {
    match value {
        "LINEAR" => Ok(OpacityInterpolation::Linear),
        "BEZIER" => Ok(OpacityInterpolation::Bezier),
        "HOLD" => Ok(OpacityInterpolation::Hold),
        _ => Err("Unknown Opacity interpolation type".into()),
    }
}
fn ease(args: &Value, field: &str) -> Result<OpacityEase, String> {
    let values = args
        .get(field)
        .and_then(Value::as_array)
        .ok_or("Opacity temporal ease must be an array")?;
    if values.len() != 1 {
        return Err("Scalar temporal ease requires exactly one KeyframeEase per side".into());
    }
    let value = values[0]
        .as_object()
        .ok_or("KeyframeEase must be an object")?;
    if value.len() != 2 || !value.contains_key("speed") || !value.contains_key("influence") {
        return Err("Only KeyframeEase speed and influence are supported".into());
    }
    let ease = OpacityEase {
        speed: number(&value["speed"])?,
        influence: number(&value["influence"])?,
    };
    ease.validate().map_err(|e| e.to_string())?;
    Ok(ease)
}
impl AutomationHost {
    pub(super) fn opacity_set(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let value = number(args.get("value").ok_or("Missing Opacity value")?)?;
        let edit = match args.get("time") {
            Some(time) => OpacityEdit::Key {
                frame: frame_at(self.composition(comp)?, number(time)?, false)?,
                value,
            },
            None => OpacityEdit::Value(value),
        };
        self.apply(comp, Command::SetOpacityTiming { id, edit })
    }
    pub(super) fn opacity_key(
        &mut self,
        comp: CompositionId,
        id: LayerId,
        args: &Value,
    ) -> Result<Value, String> {
        let action = text(args, "action")?;
        let layer = self.layer(comp, id)?;
        let frames = property_frames(layer, "opacity")?;
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
                        .then_with(|| a.cmp(b))
                })
                .map(|(index, _)| json!(index + 1))
                .ok_or("Property has no keys".into());
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
            return Ok(json!(
                layer
                    .opacity_key_value(frame)
                    .ok_or("Opacity key not found")?
            ));
        }
        if action == "remove" && !layer.has_opacity_timing() {
            return self.apply(
                comp,
                Command::ToggleKeyframe {
                    id,
                    property: Property::Opacity,
                    frame,
                },
            );
        }
        let flag = || {
            args.get("value")
                .and_then(Value::as_bool)
                .ok_or("Keyframe flag must be boolean")
        };
        let edit =
            match action {
                "remove" => OpacityEdit::RemoveKey { frame },
                "interpolation" => OpacityEdit::Interpolation {
                    frame,
                    incoming: interpolation(text(args, "inType")?)?,
                    outgoing: interpolation(if args.get("outType").is_some() {
                        text(args, "outType")?
                    } else {
                        text(args, "inType")?
                    })?,
                },
                "ease" => OpacityEdit::TemporalEase {
                    frame,
                    incoming: ease(args, "inEase")?,
                    outgoing: ease(args, "outEase")?,
                },
                "temporal_continuous" => OpacityEdit::TemporalContinuous {
                    frame,
                    value: flag()?,
                },
                "temporal_auto_bezier" => OpacityEdit::TemporalAutoBezier {
                    frame,
                    value: flag()?,
                },
                "spatial_tangents" | "spatial_continuous" | "spatial_auto_bezier" => return Err(
                    "Opacity is scalar and has no spatial tangents or spatial interpolation flags"
                        .into(),
                ),
                _ => return Err(format!("Unsupported Opacity keyframe operation: {action}")),
            };
        self.apply(comp, Command::SetOpacityTiming { id, edit })
    }
}
