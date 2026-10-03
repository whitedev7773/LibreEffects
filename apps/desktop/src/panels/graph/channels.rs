//! Scalar lane identity and explicit raw-track units. Labels never provide identity.
use super::*;
use crate::view_state::GraphChannel;
use libre_effects_core::{
    AudioParam, Content, ContentsParam, EffectKind, EffectParam, GradientParam, KeyRef, MaskParam,
    Project, Property, ShapeParam,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unit {
    Pixels,
    Degrees,
    Percent,
    Rgb,
    Decibels,
    Seconds,
    Ratio,
    Count,
}
impl Unit {
    pub fn label(self, speed: bool) -> &'static str {
        match (self, speed) {
            (Self::Pixels, false) => "px",
            (Self::Pixels, true) => "px/s",
            (Self::Degrees, false) => "deg",
            (Self::Degrees, true) => "deg/s",
            (Self::Percent, false) => "%",
            (Self::Percent, true) => "%/s",
            (Self::Rgb, false) => "RGB 0–255",
            (Self::Rgb, true) => "RGB units/s",
            (Self::Decibels, false) => "dB",
            (Self::Decibels, true) => "dB/s",
            (Self::Seconds, false) => "s",
            (Self::Seconds, true) => "s/s",
            (Self::Ratio, false) => "ratio",
            (Self::Ratio, true) => "ratio/s",
            (Self::Count, false) => "count",
            (Self::Count, true) => "count/s",
        }
    }
}
#[derive(Clone, Debug)]
pub(super) struct Descriptor {
    pub channel: GraphChannel,
    pub label: String,
    pub units: Unit,
}
fn transform_unit(p: Property) -> Unit {
    match p {
        Property::PositionX | Property::PositionY | Property::AnchorX | Property::AnchorY => {
            Unit::Pixels
        }
        Property::Rotation => Unit::Degrees,
        Property::ScaleX | Property::ScaleY | Property::Opacity => Unit::Percent,
    }
}
fn shape_unit(p: ShapeParam) -> Unit {
    match p {
        ShapeParam::StrokeWidth
        | ShapeParam::Roundness
        | ShapeParam::DashOffset
        | ShapeParam::DashLength(_) => Unit::Pixels,
        ShapeParam::InnerRadius | ShapeParam::FillOpacity | ShapeParam::StrokeOpacity => {
            Unit::Percent
        }
        ShapeParam::Points => Unit::Count,
        ShapeParam::MiterLimit => Unit::Ratio,
        ShapeParam::FillRed
        | ShapeParam::FillGreen
        | ShapeParam::FillBlue
        | ShapeParam::StrokeRed
        | ShapeParam::StrokeGreen
        | ShapeParam::StrokeBlue => Unit::Rgb,
    }
}
fn gradient_unit(p: GradientParam) -> Unit {
    match p {
        GradientParam::StartX
        | GradientParam::StartY
        | GradientParam::EndX
        | GradientParam::EndY => Unit::Pixels,
        GradientParam::HighlightAngle => Unit::Degrees,
        GradientParam::Red(_) | GradientParam::Green(_) | GradientParam::Blue(_) => Unit::Rgb,
        GradientParam::HighlightLength
        | GradientParam::ColorPosition(_)
        | GradientParam::ColorMidpoint(_)
        | GradientParam::OpacityPosition(_)
        | GradientParam::Opacity(_)
        | GradientParam::OpacityMidpoint(_) => Unit::Percent,
    }
}
pub(super) fn describe(project: &Project, channel: GraphChannel) -> Option<Descriptor> {
    let layer = project.composition().layer(channel.id)?;
    if matches!(channel.property, PropertyPath::Path(_)) {
        return None;
    }
    layer.track(channel.property)?;
    let mut label = layer.track_label(channel.property)?;
    let units = match channel.property {
        PropertyPath::Path(_) => return None,
        PropertyPath::Transform(p) => transform_unit(p),
        PropertyPath::Shape(p) => shape_unit(p),
        PropertyPath::Text(TextParam::StrokeWidth) => Unit::Pixels,
        PropertyPath::Text(_) => Unit::Rgb,
        PropertyPath::Mask { mask, parameter } => {
            label = format!("{label} [mask #{mask}]");
            match parameter {
                MaskParam::Opacity => Unit::Percent,
                MaskParam::Feather | MaskParam::Expansion => Unit::Pixels,
            }
        }
        PropertyPath::Audio(AudioParam::LeftLevel | AudioParam::RightLevel) => Unit::Decibels,
        PropertyPath::Audio(AudioParam::Pan | AudioParam::Fade) => Unit::Percent,
        PropertyPath::TimeRemap => Unit::Seconds,
        PropertyPath::Contents { item, parameter } => {
            // Include ancestry and stable IDs; duplicate group/item names remain distinct.
            let Content::ShapeContents(contents) = layer.content() else {
                return None;
            };
            let mut ancestors = Vec::new();
            for (depth, _, node) in contents.rows() {
                ancestors.truncate(depth);
                ancestors.push(format!("{} [#{}]", node.name, node.id));
                if node.id == item {
                    break;
                }
            }
            label = format!(
                "Contents · {} · {}",
                ancestors.join(" / "),
                parameter.label()
            );
            match parameter {
                ContentsParam::Width | ContentsParam::Height => Unit::Pixels,
                ContentsParam::Transform(p) => transform_unit(p),
                ContentsParam::Skew | ContentsParam::SkewAxis => Unit::Degrees,
                ContentsParam::Shape(p) => shape_unit(p),
                ContentsParam::Gradient(p) => gradient_unit(p),
            }
        }
        PropertyPath::Effect { effect, parameter } => {
            let instance = layer.effect_stack().iter().find(|e| e.id() == effect)?;
            label = format!("{label} [effect #{effect}]");
            match parameter {
                EffectParam::Radius
                | EffectParam::OffsetX
                | EffectParam::OffsetY
                | EffectParam::StartX
                | EffectParam::StartY
                | EffectParam::EndX
                | EffectParam::EndY => Unit::Pixels,
                EffectParam::Opacity | EffectParam::BlendOriginal => Unit::Percent,
                EffectParam::Hue => Unit::Degrees,
                EffectParam::Black | EffectParam::White | EffectParam::Gamma => Unit::Ratio,
                // Amount is intentionally effect-kind aware. Brightness, Saturation
                // and Glow intensity store raw multipliers, not percentages.
                EffectParam::Amount => match instance.kind() {
                    EffectKind::Tint => Unit::Percent,
                    EffectKind::Brightness | EffectKind::HueSaturation | EffectKind::Glow => {
                        Unit::Ratio
                    }
                    _ => return None,
                },
                EffectParam::Red
                | EffectParam::Green
                | EffectParam::Blue
                | EffectParam::DarkRed
                | EffectParam::DarkGreen
                | EffectParam::DarkBlue
                | EffectParam::Curve0
                | EffectParam::Curve25
                | EffectParam::Curve50
                | EffectParam::Curve75
                | EffectParam::Curve100
                | EffectParam::RedCurve0
                | EffectParam::RedCurve25
                | EffectParam::RedCurve50
                | EffectParam::RedCurve75
                | EffectParam::RedCurve100
                | EffectParam::GreenCurve0
                | EffectParam::GreenCurve25
                | EffectParam::GreenCurve50
                | EffectParam::GreenCurve75
                | EffectParam::GreenCurve100
                | EffectParam::BlueCurve0
                | EffectParam::BlueCurve25
                | EffectParam::BlueCurve50
                | EffectParam::BlueCurve75
                | EffectParam::BlueCurve100 => Unit::Rgb,
            }
        }
    };
    Some(Descriptor {
        channel,
        label: format!("{} [layer #{}] · {label}", layer.name(), layer.id()),
        units,
    })
}
pub(super) fn included(state: &EditorState) -> Vec<GraphChannel> {
    state.graph_included_channels()
}
pub(super) fn all_keys(project: &Project, channels: &[GraphChannel]) -> BTreeSet<KeyRef> {
    channels
        .iter()
        .filter_map(|channel| {
            describe(project, *channel)?;
            Some((
                channel,
                project
                    .composition()
                    .layer(channel.id)?
                    .track(channel.property)?,
            ))
        })
        .flat_map(|(channel, track)| {
            track.keys().keys().map(move |&frame| KeyRef {
                id: channel.id,
                property: channel.property,
                frame,
            })
        })
        .collect()
}
