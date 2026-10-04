//! Scalar lane identity and explicit raw-track units. Labels never provide identity.
use super::*;
use crate::view_state::GraphChannel;
use libre_effects_core::{
    AudioParam, Content, ContentsParam, EffectKind, EffectParam, GradientParam, KeyRef, MaskParam,
    Project, Property, ShapeParam, TrimParam,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unit {
    Pixels,
    ThousandthsEm,
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
            (Self::ThousandthsEm, false) => "1/1000 em",
            (Self::ThousandthsEm, true) => "(1/1000 em)/s",
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
pub(super) fn text_unit(parameter: TextParam) -> Unit {
    match parameter {
        TextParam::FontSize | TextParam::StrokeWidth => Unit::Pixels,
        TextParam::Tracking => Unit::ThousandthsEm,
        TextParam::Leading => Unit::Ratio,
        TextParam::FillRed
        | TextParam::FillGreen
        | TextParam::FillBlue
        | TextParam::StrokeRed
        | TextParam::StrokeGreen
        | TextParam::StrokeBlue => Unit::Rgb,
    }
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
        PropertyPath::Text(parameter) => text_unit(parameter),
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
                ContentsParam::Trim(TrimParam::Start | TrimParam::End) => Unit::Percent,
                ContentsParam::Trim(TrimParam::Offset) => Unit::Degrees,
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

#[cfg(test)]
mod typography_units_tests {
    use super::*;
    use libre_effects_core::{Command, Editor, TrackEdit};

    #[test]
    fn actual_text_lane_descriptors_use_distinct_scalar_units_in_value_and_speed_modes() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Text".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Title".into(),
            })
            .unwrap();
        for parameter in TextParam::ALL {
            let channel = GraphChannel {
                id: 1,
                property: PropertyPath::Text(parameter),
            };
            // Unmaterialized typography stays sparse and does not become an
            // accidental graph curve just because the UI can show its base.
            assert!(describe(editor.project(), channel).is_none());
            editor
                .execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit: TrackEdit::ToggleAnimation { frame: 7 },
                })
                .unwrap();
            let descriptor = describe(editor.project(), channel).unwrap();
            let (value_unit, speed_unit) = match parameter {
                TextParam::FontSize | TextParam::StrokeWidth => ("px", "px/s"),
                TextParam::Tracking => ("1/1000 em", "(1/1000 em)/s"),
                TextParam::Leading => ("ratio", "ratio/s"),
                _ => ("RGB 0–255", "RGB units/s"),
            };
            assert_eq!(descriptor.channel, channel);
            assert_eq!(descriptor.units.label(false), value_unit);
            assert_eq!(descriptor.units.label(true), speed_unit);
            assert!(descriptor.label.ends_with(parameter.label()));
            assert_eq!(
                all_keys(editor.project(), &[channel]),
                [KeyRef {
                    id: 1,
                    property: channel.property,
                    frame: 7
                }]
                .into()
            );
        }
    }
}

#[cfg(test)]
mod trim_units_tests {
    use super::*;
    use libre_effects_core::{Command, ContentsEdit, ContentsKind, Editor, TrackEdit};

    #[test]
    fn trim_lanes_use_percent_and_unwrapped_degree_units_and_keep_legacy_descriptors() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Shape(Default::default()),
                width: 200.,
                height: 120.,
                name: "Trim controls fixture".into(),
            })
            .unwrap();
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Promote,
            })
            .unwrap();
        let legacy = editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .track_paths()
            .into_iter()
            .filter_map(|property| describe(editor.project(), GraphChannel { id: 1, property }))
            .map(|descriptor| (descriptor.channel, descriptor.label, descriptor.units))
            .collect::<Vec<_>>();
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 1,
                    kind: ContentsKind::TrimPaths,
                },
            })
            .unwrap();
        let Content::ShapeContents(contents) = editor.selected_layer().unwrap().content() else {
            panic!("expected Contents");
        };
        let item = contents
            .rows()
            .into_iter()
            .find(|(_, _, node)| matches!(node.kind, ContentsKind::TrimPaths))
            .unwrap()
            .2
            .id;
        for (channel, label, units) in legacy {
            let descriptor = describe(editor.project(), channel).unwrap();
            assert_eq!((descriptor.label, descriptor.units), (label, units));
        }
        for (parameter, value_unit, speed_unit) in [
            (TrimParam::Start, "%", "%/s"),
            (TrimParam::End, "%", "%/s"),
            (TrimParam::Offset, "deg", "deg/s"),
        ] {
            let channel = GraphChannel {
                id: 1,
                property: PropertyPath::Contents {
                    item,
                    parameter: ContentsParam::Trim(parameter),
                },
            };
            editor
                .execute(Command::EditTrack {
                    id: 1,
                    property: channel.property,
                    edit: TrackEdit::ToggleKey { frame: 7 },
                })
                .unwrap();
            let descriptor = describe(editor.project(), channel).unwrap();
            assert_eq!(descriptor.channel, channel);
            assert_eq!(descriptor.units.label(false), value_unit);
            assert_eq!(descriptor.units.label(true), speed_unit);
            assert!(
                descriptor
                    .label
                    .contains(&format!("Trim Paths {item} [#{item}]"))
            );
            assert!(descriptor.label.ends_with(parameter.label()));
            assert_eq!(
                all_keys(editor.project(), &[channel]),
                [KeyRef {
                    id: 1,
                    property: channel.property,
                    frame: 7,
                }]
                .into()
            );
        }
    }
}
