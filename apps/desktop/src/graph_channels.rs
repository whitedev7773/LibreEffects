//! Bounded, view-only graph lane identities and their versioned wire addresses.
use libre_effects_core::{
    AudioParam, Composition, ContentsParam, EffectParam, KeyRef, LayerId, MaskParam, Property,
    PropertyPath, ShapeParam, TextParam,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_PINNED_CHANNELS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GraphChannel {
    pub id: LayerId,
    pub property: PropertyPath,
}
impl From<KeyRef> for GraphChannel {
    fn from(key: KeyRef) -> Self {
        Self {
            id: key.id,
            property: key.property,
        }
    }
}
impl GraphChannel {
    pub fn available(self, composition: &Composition) -> bool {
        !matches!(self.property, PropertyPath::Path(_))
            && composition
                .layer(self.id)
                .is_some_and(|layer| layer.track(self.property).is_some())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct GraphRanges {
    pub value: Option<[f64; 2]>,
    pub speed: Option<[f64; 2]>,
}
impl GraphRanges {
    pub fn normalize(&mut self) {
        self.value = normalized_height(self.value);
        self.speed = normalized_height(self.speed);
    }
    fn is_auto(&self) -> bool {
        self.value.is_none() && self.speed.is_none()
    }
}
pub(super) fn normalized_height(height: Option<[f64; 2]>) -> Option<[f64; 2]> {
    height.filter(|[low, high]| {
        low.is_finite()
            && high.is_finite()
            && low.abs() <= 1e15
            && high.abs() <= 1e15
            && high - low >= 1e-6
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GraphChannels {
    pub explicit: bool,
    pub pinned: Vec<GraphChannel>,
    pub active: Option<GraphChannel>,
    pub ranges: BTreeMap<GraphChannel, GraphRanges>,
    // An address can outlive its object while Undo is possible. Ordinary creation
    // must not silently attach that view intent to a newly reused counter ID.
    unavailable: BTreeSet<GraphChannel>,
}
impl GraphChannels {
    pub fn is_legacy(&self) -> bool {
        !self.explicit
    }
    pub fn is_pinned(&self, channel: GraphChannel) -> bool {
        self.pinned.contains(&channel)
    }
    pub fn is_available(&self, channel: GraphChannel) -> bool {
        self.contains(channel) && !self.unavailable.contains(&channel)
    }
    pub fn contains(&self, channel: GraphChannel) -> bool {
        self.is_pinned(channel) || self.active == Some(channel)
    }
    pub fn included(&self) -> Vec<GraphChannel> {
        self.pinned
            .iter()
            .copied()
            .chain(self.active.filter(|c| !self.is_pinned(*c)))
            .filter(|c| !self.unavailable.contains(c))
            .collect()
    }
    pub fn activate(&mut self, channel: GraphChannel) {
        self.explicit = true;
        self.active = Some(channel);
        self.trim();
    }
    pub fn pin(&mut self, channel: GraphChannel) -> Result<(), String> {
        if !self.is_pinned(channel) {
            if self.pinned.len() >= MAX_PINNED_CHANNELS {
                return Err(format!(
                    "The Graph supports up to {MAX_PINNED_CHANNELS} pinned channels"
                ));
            }
            self.pinned.push(channel);
        }
        self.explicit = true;
        self.unavailable.remove(&channel);
        self.trim();
        Ok(())
    }
    pub fn unpin(&mut self, channel: GraphChannel) {
        self.pinned.retain(|c| *c != channel);
        if self.unavailable.contains(&channel) && self.active == Some(channel) {
            self.active = None;
        }
        self.unavailable.remove(&channel);
        self.trim();
    }
    fn trim(&mut self) {
        let mut seen = BTreeSet::new();
        self.pinned.retain(|c| seen.insert(*c));
        self.pinned.truncate(MAX_PINNED_CHANNELS);
        self.ranges.retain(|c, ranges| {
            ranges.normalize();
            (self.pinned.contains(c) || self.active == Some(*c)) && !ranges.is_auto()
        });
        self.unavailable.retain(|c| self.pinned.contains(c));
    }
    /// Reconciliation never creates a source track, key, command or history entry.
    pub fn reconcile(&mut self, composition: Option<&Composition>, history: bool) {
        if !self.explicit {
            return;
        }
        self.trim();
        let available = |channel: GraphChannel| composition.is_some_and(|c| channel.available(c));
        if !history {
            // An unavailable address becoming available through an ordinary edit
            // denotes a new object. Drop the old pin rather than reviving it.
            self.pinned
                .retain(|c| !self.unavailable.contains(c) || !available(*c));
        }
        self.unavailable = self
            .pinned
            .iter()
            .copied()
            .filter(|c| !available(*c))
            .collect();
        if self.active.is_some_and(|c| !available(c)) {
            self.active = self.pinned.iter().copied().find(|c| available(*c));
        }
        self.trim();
    }
    /// Save/Open discard unavailable addresses, but callers must prune a COPY on Save.
    pub fn prune(&mut self, composition: &Composition) {
        self.pinned
            .retain(|c| c.available(composition) && !self.unavailable.contains(c));
        if self
            .active
            .is_some_and(|c| !c.available(composition) || self.unavailable.contains(&c))
        {
            self.active = self.pinned.first().copied();
        }
        self.unavailable.clear();
        self.trim();
        self.explicit = !self.pinned.is_empty() || self.active.is_some() || !self.ranges.is_empty();
    }
}

// This DTO belongs to desktop VIEW schema v2, not the render project schema.
// PropertyPath intentionally has no core serde dependency; every scalar variant
// gets a typed, unambiguous address here. Path timing is never a numeric lane.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PropertyAddress {
    Transform { parameter: Property },
    Shape { parameter: ShapeParam },
    Text { parameter: TextParam },
    Contents { item: u64, parameter: ContentsParam },
    Mask { mask: u64, parameter: MaskParam },
    Audio { parameter: AudioParam },
    TimeRemap {},
    Effect { effect: u64, parameter: EffectParam },
}
impl TryFrom<PropertyPath> for PropertyAddress {
    type Error = String;
    fn try_from(path: PropertyPath) -> Result<Self, String> {
        Ok(match path {
            PropertyPath::Transform(parameter) => Self::Transform { parameter },
            PropertyPath::Shape(parameter) => Self::Shape { parameter },
            PropertyPath::Text(parameter) => Self::Text { parameter },
            PropertyPath::Contents { item, parameter } => Self::Contents { item, parameter },
            PropertyPath::Mask { mask, parameter } => Self::Mask { mask, parameter },
            PropertyPath::Audio(parameter) => Self::Audio { parameter },
            PropertyPath::TimeRemap => Self::TimeRemap {},
            PropertyPath::Effect { effect, parameter } => Self::Effect { effect, parameter },
            PropertyPath::Path(_) => return Err("Path timing cannot be a Graph channel".into()),
        })
    }
}
impl TryFrom<PropertyAddress> for PropertyPath {
    type Error = String;
    fn try_from(address: PropertyAddress) -> Result<Self, String> {
        Ok(match address {
            PropertyAddress::Transform { parameter } => Self::Transform(parameter),
            PropertyAddress::Shape { parameter } => Self::Shape(parameter),
            PropertyAddress::Text { parameter } => Self::Text(parameter),
            PropertyAddress::Contents { item, parameter } if item > 0 => {
                Self::Contents { item, parameter }
            }
            PropertyAddress::Mask { mask, parameter } if mask > 0 => Self::Mask { mask, parameter },
            PropertyAddress::Audio { parameter } => Self::Audio(parameter),
            PropertyAddress::TimeRemap {} => Self::TimeRemap,
            PropertyAddress::Effect { effect, parameter } if effect > 0 => {
                Self::Effect { effect, parameter }
            }
            _ => return Err("Invalid zero Graph property ID".into()),
        })
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelAddress {
    id: LayerId,
    property: PropertyAddress,
}
impl TryFrom<ChannelAddress> for GraphChannel {
    type Error = String;
    fn try_from(address: ChannelAddress) -> Result<Self, String> {
        if address.id == 0 {
            return Err("Invalid zero Graph layer ID".into());
        }
        Ok(Self {
            id: address.id,
            property: address.property.try_into()?,
        })
    }
}
impl TryFrom<GraphChannel> for ChannelAddress {
    type Error = String;
    fn try_from(channel: GraphChannel) -> Result<Self, String> {
        if channel.id == 0 {
            return Err("Invalid zero Graph layer ID".into());
        }
        Ok(Self {
            id: channel.id,
            property: channel.property.try_into()?,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelRange {
    channel: ChannelAddress,
    value: Option<[f64; 2]>,
    speed: Option<[f64; 2]>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelsDto {
    version: u32,
    pinned: Vec<ChannelAddress>,
    active: Option<ChannelAddress>,
    ranges: Vec<ChannelRange>,
}
impl Serialize for GraphChannels {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let dto = ChannelsDto {
            version: 1,
            pinned: self
                .pinned
                .iter()
                .copied()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()
                .map_err(serde::ser::Error::custom)?,
            active: self
                .active
                .map(TryInto::try_into)
                .transpose()
                .map_err(serde::ser::Error::custom)?,
            ranges: self
                .ranges
                .iter()
                .map(|(channel, ranges)| {
                    Ok(ChannelRange {
                        channel: (*channel).try_into()?,
                        value: ranges.value,
                        speed: ranges.speed,
                    })
                })
                .collect::<Result<_, String>>()
                .map_err(serde::ser::Error::custom)?,
        };
        dto.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for GraphChannels {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let dto = ChannelsDto::deserialize(deserializer)?;
        if dto.version != 1 {
            return Err(serde::de::Error::custom(
                "Unsupported Graph channel address version",
            ));
        }
        if dto.pinned.len() > MAX_PINNED_CHANNELS || dto.ranges.len() > MAX_PINNED_CHANNELS + 1 {
            return Err(serde::de::Error::custom(
                "Graph channel count exceeds limit",
            ));
        }
        let mut state = Self {
            explicit: true,
            ..Self::default()
        };
        for address in dto.pinned {
            let channel: GraphChannel = address.try_into().map_err(serde::de::Error::custom)?;
            if state.pinned.contains(&channel) {
                return Err(serde::de::Error::custom("Duplicate pinned Graph channel"));
            }
            state.pinned.push(channel);
        }
        state.active = dto
            .active
            .map(TryInto::try_into)
            .transpose()
            .map_err(serde::de::Error::custom)?;
        for range in dto.ranges {
            let channel: GraphChannel =
                range.channel.try_into().map_err(serde::de::Error::custom)?;
            if !state.contains(channel) {
                return Err(serde::de::Error::custom(
                    "Graph range is not an included channel",
                ));
            }
            let ranges = GraphRanges {
                value: range.value,
                speed: range.speed,
            };
            if state.ranges.insert(channel, ranges).is_some() {
                return Err(serde::de::Error::custom("Duplicate Graph channel range"));
            }
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_scalar_addresses_roundtrip_without_core_schema_changes() {
        for property in [
            Property::PositionX.into(),
            PropertyPath::Shape(ShapeParam::DashLength(1)),
            PropertyPath::Text(TextParam::FillRed),
            PropertyPath::Contents {
                item: 7,
                parameter: ContentsParam::Width,
            },
            PropertyPath::Contents {
                item: 8,
                parameter: ContentsParam::Transform(Property::Rotation),
            },
            PropertyPath::Contents {
                item: 8,
                parameter: ContentsParam::Shape(ShapeParam::FillOpacity),
            },
            PropertyPath::Contents {
                item: 8,
                parameter: ContentsParam::Gradient(libre_effects_core::GradientParam::Red(2)),
            },
            PropertyPath::Mask {
                mask: 5,
                parameter: MaskParam::Feather,
            },
            PropertyPath::Audio(AudioParam::Pan),
            PropertyPath::TimeRemap,
            PropertyPath::Effect {
                effect: 9,
                parameter: EffectParam::Radius,
            },
        ] {
            let channel = GraphChannel { id: 3, property };
            let mut state = GraphChannels::default();
            state.pin(channel).unwrap();
            state.activate(channel);
            state.ranges.insert(
                channel,
                GraphRanges {
                    value: Some([-10., 20.]),
                    speed: Some([-1., 2.]),
                },
            );
            let encoded = serde_json::to_vec(&state).unwrap();
            let decoded: GraphChannels = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn scalar_address_wire_rejects_path_timing_and_future_fields() {
        let path = GraphChannel {
            id: 1,
            property: PropertyPath::Path(libre_effects_core::PathTarget::Shape),
        };
        let mut state = GraphChannels::default();
        state.pin(path).unwrap();
        assert!(serde_json::to_vec(&state).is_err());
        assert!(
            serde_json::from_str::<GraphChannels>(
                r#"{"version":1,"pinned":[],"active":null,"ranges":[],"future":true}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<GraphChannels>(
            r#"{"version":1,"pinned":[],"active":{"id":1,"property":{"kind":"time_remap","future":true}},"ranges":[]}"#
        ).is_err());
    }

    #[test]
    fn invalid_channel_ranges_normalize_each_mode_independently() {
        for invalid in [
            [2., 1.],
            [0., 0.],
            [f64::NAN, 1.],
            [0., f64::INFINITY],
            [-1e16, 1e16],
        ] {
            let mut ranges = GraphRanges {
                value: Some(invalid),
                speed: Some([-12., 24.]),
            };
            ranges.normalize();
            assert_eq!(ranges.value, None);
            assert_eq!(ranges.speed, Some([-12., 24.]));
            let mut ranges = GraphRanges {
                value: Some([-12., 24.]),
                speed: Some(invalid),
            };
            ranges.normalize();
            assert_eq!(ranges.value, Some([-12., 24.]));
            assert_eq!(ranges.speed, None);
        }
    }
}

#[cfg(test)]
mod trim_channel_tests {
    use super::*;
    use libre_effects_core::{Command, Content, ContentsEdit, ContentsKind, Editor, TrimParam};

    fn fixture() -> (Editor, u64, Vec<GraphChannel>) {
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
        let channels = TrimParam::ALL
            .into_iter()
            .map(|parameter| GraphChannel {
                id: 1,
                property: PropertyPath::Contents {
                    item,
                    parameter: ContentsParam::Trim(parameter),
                },
            })
            .collect();
        (editor, item, channels)
    }

    #[test]
    fn trim_addresses_roundtrip_in_existing_typed_contents_address_version() {
        let (editor, item, channels) = fixture();
        let mut state = GraphChannels::default();
        for &channel in &channels {
            state.pin(channel).unwrap();
            state.ranges.insert(
                channel,
                GraphRanges {
                    value: Some([-360., 720.]),
                    speed: Some([-100., 100.]),
                },
            );
        }
        state.activate(channels[2]);
        let source = editor.project().to_json().unwrap();
        let value = serde_json::to_value(&state).unwrap();
        assert_eq!(value["version"], 1);
        for (index, name) in ["Trim.Start", "Trim.End", "Trim.Offset"]
            .into_iter()
            .enumerate()
        {
            assert_eq!(value["pinned"][index]["property"]["kind"], "contents");
            assert_eq!(value["pinned"][index]["property"]["item"], item);
            assert_eq!(value["pinned"][index]["property"]["parameter"], name);
        }
        let mut loaded: GraphChannels = serde_json::from_value(value).unwrap();
        loaded.reconcile(Some(editor.project().composition()), false);
        assert_eq!(loaded, state);
        assert_eq!(editor.project().to_json().unwrap(), source);
    }

    #[test]
    fn trim_delete_save_copy_and_history_preserve_only_original_pin_identities() {
        let (mut editor, item, channels) = fixture();
        let mut state = GraphChannels::default();
        for &channel in &channels {
            state.pin(channel).unwrap();
            state.ranges.insert(
                channel,
                GraphRanges {
                    value: Some([0., 100.]),
                    speed: Some([-10., 10.]),
                },
            );
        }
        state.activate(channels[1]);
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Duplicate(item),
            })
            .unwrap();
        state.reconcile(Some(editor.project().composition()), false);
        assert_eq!(state.pinned, channels);
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Remove(item),
            })
            .unwrap();
        state.reconcile(Some(editor.project().composition()), false);
        assert_eq!(state.pinned, channels);
        assert!(state.included().is_empty());
        assert_eq!(state.active, None);
        let live = state.clone();
        let source = editor.project().clone();
        let mut saved = state.clone();
        saved.prune(editor.project().composition());
        assert!(saved.is_legacy());
        assert!(saved.pinned.is_empty());
        assert!(saved.ranges.is_empty());
        assert_eq!(state, live);
        assert_eq!(editor.project(), &source);
        editor.undo();
        state.reconcile(Some(editor.project().composition()), true);
        assert_eq!(state.included(), channels);
        assert!(channels.iter().all(|c| state.ranges.contains_key(c)));
        editor.redo();
        state.reconcile(Some(editor.project().composition()), true);
        assert!(state.included().is_empty());
        editor.undo();
        state.reconcile(Some(editor.project().composition()), true);
        assert_eq!(state.included(), channels);
    }

    #[test]
    fn reused_trim_item_id_does_not_revive_removed_pin_intent() {
        let (mut editor, item, channels) = fixture();
        let mut state = GraphChannels::default();
        state.pin(channels[0]).unwrap();
        state.activate(channels[0]);
        editor.undo();
        state.reconcile(Some(editor.project().composition()), true);
        assert!(!state.is_available(channels[0]));
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
        assert!(matches!(
            contents.node(item).unwrap().kind,
            ContentsKind::TrimPaths
        ));
        state.reconcile(Some(editor.project().composition()), false);
        assert!(!state.is_pinned(channels[0]));
        assert_eq!(state.active, None);
        assert!(state.included().is_empty());
    }
}
