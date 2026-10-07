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
    Luma,
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
            (Self::Luma, false) => "Luma 0–255",
            (Self::Luma, true) => "Luma units/s",
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
        TextParam::FontSize
        | TextParam::StrokeWidth
        | TextParam::AnimatorPositionX
        | TextParam::AnimatorPositionY => Unit::Pixels,
        TextParam::AnimatorRotation => Unit::Degrees,
        TextParam::Tracking => Unit::ThousandthsEm,
        TextParam::Leading => Unit::Ratio,
        TextParam::FillOpacity
        | TextParam::StrokeOpacity
        | TextParam::AnimatorStart
        | TextParam::AnimatorEnd
        | TextParam::AnimatorOffset
        | TextParam::AnimatorAmount
        | TextParam::AnimatorScaleX
        | TextParam::AnimatorScaleY
        | TextParam::AnimatorOpacity => Unit::Percent,
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
    if matches!(
        channel.property,
        PropertyPath::Path(_) | PropertyPath::SourceText
    ) {
        return None;
    }
    layer.track(channel.property)?;
    let mut label = layer.track_label(channel.property)?;
    let units = match channel.property {
        PropertyPath::Path(_) | PropertyPath::SourceText => return None,
        PropertyPath::Transform(p) => transform_unit(p),
        PropertyPath::Shape(p) => shape_unit(p),
        PropertyPath::Text(parameter) => text_unit(parameter),
        PropertyPath::TextAnimator {
            animator,
            parameter,
        } => {
            label = format!("{label} [animator #{animator}]");
            text_unit(parameter)
        }
        PropertyPath::TextSelector { selector, .. } => {
            label = format!("{label} [selector #{selector}]");
            Unit::Percent
        }
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
                EffectParam::LumaThreshold | EffectParam::LumaSoftness => Unit::Luma,
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
    fn animator_transform_graph_lanes_keep_independent_keys_units_and_identity_colors() {
        let mut state = EditorState::default();
        state.editor = Editor::default();
        state
            .editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Transform lanes".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Title".into(),
            })
            .unwrap();
        let parameters = [
            TextParam::AnimatorScaleX,
            TextParam::AnimatorScaleY,
            TextParam::AnimatorRotation,
        ];
        let lanes = parameters.map(|parameter| GraphChannel {
            id: 1,
            property: PropertyPath::Text(parameter),
        });
        let colors = lanes.map(super::super::channel_color);
        let original = state.editor.project().clone();
        for lane in lanes {
            assert!(describe(state.editor.project(), lane).is_none());
            assert!(state.graph_pin_channel(lane).is_err());
        }
        assert_eq!(state.editor.project(), &original);
        for (index, parameter) in parameters.into_iter().enumerate() {
            state
                .editor
                .execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit: TrackEdit::ToggleAnimation {
                        frame: 7 + index as u32,
                    },
                })
                .unwrap();
            let descriptor = describe(state.editor.project(), lanes[index]).unwrap();
            assert_eq!(
                descriptor.units,
                if index == 2 {
                    Unit::Degrees
                } else {
                    Unit::Percent
                }
            );
            assert!(descriptor.label.ends_with(parameter.label()));
            state.graph_pin_channel(lanes[index]).unwrap();
        }
        assert!(state.graph_activate_channel(lanes[0], false));
        assert_eq!(
            included(&state).into_iter().collect::<BTreeSet<_>>(),
            lanes.into()
        );
        assert_eq!(
            all_keys(state.editor.project(), &lanes),
            lanes
                .into_iter()
                .enumerate()
                .map(|(index, lane)| KeyRef {
                    id: lane.id,
                    property: lane.property,
                    frame: 7 + index as u32,
                })
                .collect()
        );
        let source = state.editor.project().clone();
        // Renaming a visible label or activating/reordering lanes cannot alter
        // the colors, which belong to the stable layer/property identity.
        state
            .editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Renamed".into(),
            })
            .unwrap();
        for lane in lanes.into_iter().rev() {
            assert!(state.graph_activate_channel(lane, false));
        }
        assert_eq!(lanes.map(super::super::channel_color), colors);
        assert!(colors.into_iter().all(|color| {
            [0xffc66d, 0x7bd8a5, 0xd3a3ff, 0x72c8ff, 0xff92b0, 0x9de3de].contains(&color)
        }));
        state.editor.undo();
        assert_eq!(state.editor.project(), &source);
    }

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
                TextParam::FontSize
                | TextParam::StrokeWidth
                | TextParam::AnimatorPositionX
                | TextParam::AnimatorPositionY => ("px", "px/s"),
                TextParam::AnimatorRotation => ("deg", "deg/s"),
                TextParam::Tracking => ("1/1000 em", "(1/1000 em)/s"),
                TextParam::Leading => ("ratio", "ratio/s"),
                TextParam::FillOpacity
                | TextParam::StrokeOpacity
                | TextParam::AnimatorStart
                | TextParam::AnimatorEnd
                | TextParam::AnimatorOffset
                | TextParam::AnimatorAmount
                | TextParam::AnimatorScaleX
                | TextParam::AnimatorScaleY
                | TextParam::AnimatorOpacity => ("%", "%/s"),
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

#[cfg(test)]
mod luma_units_tests {
    use super::*;
    use libre_effects_core::{Command, Editor, EffectEdit};

    #[test]
    fn luma_lanes_are_independent_with_explicit_luma_value_and_speed_units() {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::Brightness),
            })
            .unwrap();
        let legacy = GraphChannel {
            id: 1,
            property: PropertyPath::Effect {
                effect: 1,
                parameter: EffectParam::Amount,
            },
        };
        let before = describe(editor.project(), legacy).unwrap();
        assert_eq!(before.units, Unit::Ratio);
        editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::LumaKey),
            })
            .unwrap();
        let channels =
            [EffectParam::LumaThreshold, EffectParam::LumaSoftness].map(|parameter| GraphChannel {
                id: 1,
                property: PropertyPath::Effect {
                    effect: 2,
                    parameter,
                },
            });
        for (index, channel) in channels.into_iter().enumerate() {
            let PropertyPath::Effect { effect, parameter } = channel.property else {
                unreachable!()
            };
            editor
                .execute(Command::Effect {
                    id: 1,
                    edit: EffectEdit::ToggleKey {
                        effect,
                        parameter,
                        frame: 7 + index as u32,
                    },
                })
                .unwrap();
            let descriptor = describe(editor.project(), channel).unwrap();
            assert_eq!(descriptor.channel, channel);
            assert_eq!(descriptor.units.label(false), "Luma 0–255");
            assert_eq!(descriptor.units.label(true), "Luma units/s");
            assert!(descriptor.label.contains("Luma Key"));
            assert!(descriptor.label.contains("[effect #2]"));
            assert!(
                descriptor
                    .label
                    .contains(if index == 0 { "Threshold" } else { "Softness" })
            );
        }
        assert_eq!(
            all_keys(editor.project(), &channels),
            channels
                .into_iter()
                .enumerate()
                .map(|(index, channel)| KeyRef {
                    id: 1,
                    property: channel.property,
                    frame: 7 + index as u32
                })
                .collect()
        );
        let after = describe(editor.project(), legacy).unwrap();
        assert_eq!((after.label, after.units), (before.label, before.units));
        let wrong_kind = GraphChannel {
            id: 1,
            property: PropertyPath::Effect {
                effect: 1,
                parameter: EffectParam::LumaThreshold,
            },
        };
        assert!(describe(editor.project(), wrong_kind).is_none());
        editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Bypass {
                    effect: 2,
                    bypassed: true,
                },
            })
            .unwrap();
        assert!(
            channels
                .into_iter()
                .all(|channel| describe(editor.project(), channel).is_some())
        );
        editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Remove(2),
            })
            .unwrap();
        assert!(
            channels
                .into_iter()
                .all(|channel| describe(editor.project(), channel).is_none())
        );
        assert!(all_keys(editor.project(), &channels).is_empty());
    }
}

#[cfg(test)]
mod source_text_channel_tests {
    use super::*;
    use libre_effects_core::{Command, Editor, TrackEdit};

    #[test]
    fn source_text_is_never_described_or_collected_as_a_value_or_speed_lane() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "First".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Title".into(),
            })
            .unwrap();
        let source = GraphChannel {
            id: 1,
            property: PropertyPath::SourceText,
        };
        let numeric = GraphChannel {
            id: 1,
            property: Property::PositionX.into(),
        };
        editor
            .execute(Command::EditTrack {
                id: 1,
                property: numeric.property,
                edit: TrackEdit::ToggleKey { frame: 10 },
            })
            .unwrap();
        for animated in [false, true] {
            if animated {
                editor
                    .execute(Command::EditTrack {
                        id: 1,
                        property: source.property,
                        edit: TrackEdit::ToggleAnimation { frame: 10 },
                    })
                    .unwrap();
                editor
                    .execute(Command::EditSourceText {
                        id: 1,
                        frame: 30,
                        text: "Second 🦋".into(),
                    })
                    .unwrap();
            }
            let before = editor.project().clone();
            assert!(describe(&before, source).is_none());
            assert!(describe(&before, numeric).is_some());
            assert_eq!(
                all_keys(&before, &[source, numeric]),
                [KeyRef {
                    id: 1,
                    property: numeric.property,
                    frame: 10,
                }]
                .into()
            );
            assert_eq!(editor.project(), &before);
        }
    }
}

#[cfg(test)]
mod secondary_selector_graph_tests {
    use super::*;
    use libre_effects_core::{Command, Editor, TextSelectorParam, TrackEdit};

    #[test]
    fn secondary_selector_graphs_use_percent_and_bounded_stable_channel_plans() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Selectors".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Text".into(),
            })
            .unwrap();
        for _ in 0..2 {
            editor
                .execute(Command::AddTextRangeSelector { id: 1 })
                .unwrap();
        }
        let mut state = EditorState::default();
        state.editor = editor;
        let mut lanes = Vec::new();
        for parameter in TextSelectorParam::ALL {
            let lane = GraphChannel {
                id: 1,
                property: PropertyPath::TextSelector {
                    selector: 2,
                    parameter,
                },
            };
            assert!(describe(state.editor.project(), lane).is_none());
            assert!(state.graph_pin_channel(lane).is_err());
            let start = if parameter == TextSelectorParam::Offset {
                -25.
            } else {
                25.
            };
            state
                .editor
                .execute(
                    state
                        .editor
                        .selected_layer()
                        .unwrap()
                        .text_selector_value_command(2, parameter, start, 10)
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
            state
                .editor
                .execute(Command::EditTrack {
                    id: 1,
                    property: lane.property,
                    edit: TrackEdit::ToggleAnimation { frame: 10 },
                })
                .unwrap();
            state
                .editor
                .execute(Command::EditTrack {
                    id: 1,
                    property: lane.property,
                    edit: TrackEdit::Value {
                        frame: 20,
                        value: 75.,
                    },
                })
                .unwrap();
            let descriptor = describe(state.editor.project(), lane).unwrap();
            assert_eq!(descriptor.units, Unit::Percent);
            assert_eq!(descriptor.units.label(false), "%");
            assert_eq!(descriptor.units.label(true), "%/s");
            assert!(descriptor.label.contains("[selector #2]"));
            assert!(descriptor.label.contains(parameter.label()));
            state.graph_pin_channel(lane).unwrap();
            let keys = [10, 20].map(|frame| KeyRef {
                id: lane.id,
                property: lane.property,
                frame,
            });
            let source = state.editor.project().clone();
            assert!(
                super::super::planning::EditPlan::scale_value(&source, &keys, 2., Some(keys[0]))
                    .is_err()
            );
            let plan =
                super::super::planning::EditPlan::scale_value(&source, &keys, 0.5, Some(keys[0]))
                    .unwrap();
            assert!(plan.command.is_some());
            assert_eq!(plan.keys, keys);
            assert_eq!(
                plan.tracks[&lane].keys()[&20].value,
                start + (75. - start) * 0.5
            );
            assert_eq!(state.editor.project(), &source);
            assert_eq!(
                state
                    .editor
                    .selected_layer()
                    .unwrap()
                    .track(PropertyPath::TextSelector {
                        selector: 1,
                        parameter
                    }),
                None
            );
            lanes.push(lane);
        }
        // Pinning preserves the previously active legacy transform. Select a
        // selector lane explicitly before testing removal of all included lanes.
        assert!(state.graph_activate_channel(lanes[0], false));
        let source = state.editor.project().clone();
        let keys = all_keys(&source, &lanes);
        assert_eq!(keys.len(), 8);
        let labels: Vec<_> = lanes
            .iter()
            .map(|lane| describe(&source, *lane).unwrap().label)
            .collect();
        let colors: Vec<_> = lanes
            .iter()
            .copied()
            .map(super::super::channel_color)
            .collect();
        state
            .editor
            .execute(Command::MoveTextRangeSelector {
                id: 1,
                selector: 2,
                index: 0,
            })
            .unwrap();
        state
            .graph_channels
            .reconcile(Some(state.editor.project().composition()), false);
        assert_eq!(all_keys(state.editor.project(), &lanes), keys);
        assert_eq!(
            lanes
                .iter()
                .map(|lane| describe(state.editor.project(), *lane).unwrap().label)
                .collect::<Vec<_>>(),
            labels
        );
        assert_eq!(
            lanes
                .iter()
                .copied()
                .map(super::super::channel_color)
                .collect::<Vec<_>>(),
            colors
        );
        state
            .editor
            .execute(Command::RemoveTextRangeSelector { id: 1, selector: 2 })
            .unwrap();
        state
            .graph_channels
            .reconcile(Some(state.editor.project().composition()), false);
        assert!(included(&state).is_empty());
        assert!(all_keys(state.editor.project(), &lanes).is_empty());
        state.editor.undo();
        state
            .graph_channels
            .reconcile(Some(state.editor.project().composition()), true);
        assert_eq!(all_keys(state.editor.project(), &lanes), keys);
        assert_eq!(included(&state), lanes);
    }
}

#[cfg(test)]
mod animator_stack_units_tests {
    use super::*;
    use libre_effects_core::{Command, Content, Editor, TextParam, TrackEdit};
    #[test]
    fn extra_animator_graph_descriptors_and_plans_keep_parameter_units_and_ids() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Stack".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Text".into(),
            })
            .unwrap();
        editor.execute(Command::AddTextAnimator { id: 1 }).unwrap();
        for parameter in TextParam::ALL.into_iter().filter(|p| p.is_animator()) {
            let lane = GraphChannel {
                id: 1,
                property: PropertyPath::TextAnimator {
                    animator: 1,
                    parameter,
                },
            };
            assert!(describe(editor.project(), lane).is_none());
            editor
                .execute(Command::EditTrack {
                    id: 1,
                    property: lane.property,
                    edit: TrackEdit::ToggleAnimation { frame: 10 },
                })
                .unwrap();
            editor
                .execute(Command::EditTrack {
                    id: 1,
                    property: lane.property,
                    edit: TrackEdit::Value {
                        frame: 20,
                        value: 25.,
                    },
                })
                .unwrap();
            let descriptor = describe(editor.project(), lane).unwrap();
            assert_eq!(descriptor.units, text_unit(parameter));
            assert!(descriptor.label.contains("[animator #1]"));
            let keys = [10, 20].map(|frame| KeyRef {
                id: 1,
                property: lane.property,
                frame,
            });
            let original = editor.project().clone();
            let plan =
                super::super::planning::EditPlan::scale_value(&original, &keys, 0.5, Some(keys[0]))
                    .unwrap();
            assert_eq!(plan.keys, keys);
            let base = editor
                .selected_layer()
                .unwrap()
                .track_value(lane.property, 10)
                .unwrap();
            // Value-scale UI pivots around the minimum selected value, not
            // the active key (which only determines post-edit focus).
            let origin = base.min(25.);
            assert_eq!(
                plan.tracks[&lane].keys()[&20].value,
                origin + (25. - origin) * 0.5
            );
            assert_eq!(editor.project(), &original);
        }
    }
}
