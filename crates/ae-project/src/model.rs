//! Schema 1 is a Libre Effects interchange contract, not Adobe's AEP schema.
//! Unknown JSON fields are rejected. Semantic Option fields must appear explicitly
//! (nullable); omission cannot silently remove parenting, expressions or paint.
//! Only the non-semantic provenance source hash may be omitted.
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// A custom deserializer makes a nullable field required during struct decoding.
/// Without this annotation Serde would treat a missing Option field as None.
fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

pub type ItemId = u64;
pub type LayerId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RationalTime {
    pub numerator: i64,
    pub denominator: u32,
}
impl RationalTime {
    /// Exact comparison, including differently represented equivalent fractions.
    /// Only call on validated times (positive denominators).
    pub fn compare(self, other: Self) -> Ordering {
        (i128::from(self.numerator) * i128::from(other.denominator))
            .cmp(&(i128::from(other.numerator) * i128::from(self.denominator)))
    }
    pub fn seconds(self) -> f64 {
        self.numerator as f64 / f64::from(self.denominator)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}
impl FrameRate {
    pub fn frame_duration(self) -> RationalTime {
        RationalTime {
            numerator: i64::from(self.denominator),
            denominator: self.numerator,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Descriptive origin only. Never used to select or reconstruct content.
    pub source_name: String,
    pub producer: String,
    pub producer_version: String,
    /// Optional lowercase SHA-256 of the source bytes; not an identity shortcut.
    pub source_sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsupportedFeature {
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AeProject {
    pub schema_version: u32,
    pub provenance: Provenance,
    pub root_composition_ids: Vec<ItemId>,
    pub items: Vec<Item>,
    /// Project-wide unknown semantics block every conversion closure.
    pub unsupported: Vec<UnsupportedFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Item {
    Composition(Composition),
    Footage(Footage),
    Unsupported(UnsupportedItem),
}
impl Item {
    pub fn id(&self) -> ItemId {
        match self {
            Self::Composition(value) => value.id,
            Self::Footage(value) => value.id,
            Self::Unsupported(value) => value.id,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Composition(value) => &value.name,
            Self::Footage(value) => &value.name,
            Self::Unsupported(value) => &value.name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub id: ItemId,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: FrameRate,
    pub frame_rate: FrameRate,
    pub duration: RationalTime,
    /// Stack order, front/top first. Layer IDs are globally unique.
    pub layers: Vec<Layer>,
    pub markers: Vec<Marker>,
    pub unsupported: Vec<UnsupportedFeature>,
}
impl Composition {
    pub fn frame_duration(&self) -> RationalTime {
        self.frame_rate.frame_duration()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Footage {
    pub id: ItemId,
    pub name: String,
    /// Preserved as data; the reader never opens, resolves or follows this path.
    pub source_path: String,
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: FrameRate,
    #[serde(deserialize_with = "required_nullable")]
    pub frame_rate: Option<FrameRate>,
    #[serde(deserialize_with = "required_nullable")]
    pub duration: Option<RationalTime>,
    pub unsupported: Vec<UnsupportedFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsupportedItem {
    pub id: ItemId,
    pub name: String,
    /// Known item links must be retained even when payload semantics are unknown.
    pub dependency_ids: Vec<ItemId>,
    pub unsupported: Vec<UnsupportedFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    #[serde(deserialize_with = "required_nullable")]
    pub parent_id: Option<LayerId>,
    pub source: LayerSource,
    /// Independent source origin, never an alias for the in point.
    pub start_time: RationalTime,
    pub in_point: RationalTime,
    pub out_point: RationalTime,
    pub enabled: bool,
    pub label: u8,
    pub three_d: bool,
    pub properties: Vec<NumericProperty>,
    pub sliders: Vec<SliderControl>,
    pub markers: Vec<Marker>,
    pub unsupported: Vec<UnsupportedFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayerSource {
    Null {
        width: u32,
        height: u32,
    },
    Solid {
        width: u32,
        height: u32,
        color: [f64; 3],
    },
    Text {
        width: u32,
        height: u32,
        document: TextDocument,
        keys: Vec<TextKey>,
    },
    Composition {
        item_id: ItemId,
    },
    Footage {
        item_id: ItemId,
    },
    Unsupported {
        dependency_ids: Vec<ItemId>,
        feature: UnsupportedFeature,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumericProperty {
    /// Stable AE match-name path. Names are data, not decoding heuristics.
    pub match_names: Vec<String>,
    /// None means unavailable authored base, not a guessed zero/default.
    #[serde(deserialize_with = "required_nullable")]
    pub value: Option<Vec<f64>>,
    /// Dimensions remain explicit even when the authored base is absent.
    pub dimensions: u8,
    pub keys: Vec<NumericKey>,
    #[serde(deserialize_with = "required_nullable")]
    pub expression: Option<Expression>,
    pub unsupported: Vec<UnsupportedFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumericKey {
    pub time: RationalTime,
    pub value: Vec<f64>,
    pub in_interpolation: Interpolation,
    pub out_interpolation: Interpolation,
    /// None explicitly means metadata unavailable; never synthesize easing.
    #[serde(deserialize_with = "required_nullable")]
    pub in_ease: Option<Vec<KeyframeEase>>,
    #[serde(deserialize_with = "required_nullable")]
    pub out_ease: Option<Vec<KeyframeEase>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Linear,
    Hold,
    Bezier,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyframeEase {
    /// Authored units per second; influence is a percentage, not a fraction.
    pub speed: f64,
    pub influence: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expression {
    pub source: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SliderControl {
    /// Effect identity local to a layer; independent of its display name.
    pub id: u64,
    pub name: String,
    pub property: NumericProperty,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Marker {
    pub time: RationalTime,
    pub duration: RationalTime,
    pub comment: String,
    pub chapter: String,
    pub url: String,
    pub frame_target: String,
    pub cue_point_name: String,
    pub event_cue_point: bool,
    pub protected_region: bool,
    pub label: u8,
    pub parameters: Vec<MarkerParameter>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerParameter {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextKey {
    pub time: RationalTime,
    pub document: TextDocument,
    pub in_interpolation: Interpolation,
    pub out_interpolation: Interpolation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextDocument {
    /// Preserved byte-for-byte, including CR, LF and CRLF.
    pub text: String,
    pub origin: TextOrigin,
    pub default_style: CharacterStyle,
    /// Empty means default style for all text. Otherwise exact full coverage.
    pub runs: Vec<CharacterRun>,
    pub paragraph: ParagraphStyle,
    pub unsupported: Vec<UnsupportedFeature>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterRun {
    pub start_utf16: u32,
    pub end_utf16: u32,
    pub style: CharacterStyle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontIdentity {
    pub postscript_name: String,
    pub family: String,
    pub style: String,
    pub weight: u16,
    pub italic: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterStyle {
    pub font: FontIdentity,
    pub font_size: f64,
    #[serde(deserialize_with = "required_nullable")]
    pub fill_rgb: Option<[f64; 3]>,
    #[serde(deserialize_with = "required_nullable")]
    pub stroke_rgb: Option<[f64; 3]>,
    pub stroke_width: f64,
    pub stroke_over_fill: bool,
    pub stroke_join: StrokeJoin,
    pub tracking: f64,
    pub baseline_shift: f64,
    /// Percentages; 100 is unscaled.
    pub horizontal_scale: f64,
    pub vertical_scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeJoin {
    Miter,
    Round,
    Bevel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphAlignment {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParagraphStyle {
    pub alignment: ParagraphAlignment,
    /// Line height as a font-size multiplier in this interchange contract.
    pub leading: f64,
    #[serde(deserialize_with = "required_nullable")]
    pub box_size: Option<[f64; 2]>,
    pub left_indent: f64,
    pub right_indent: f64,
    pub first_line_indent: f64,
    pub space_before: f64,
    pub space_after: f64,
}

/// Coordinate convention of authored point text. AE baseline coordinates are
/// retained but require a verified adapter before native conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOrigin {
    NativeTopLeft,
    AeBaseline,
}
