//! UI-independent editing model. Frame numbers are integral; layer index zero is on top.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub type Frame = u32;
pub type LayerId = u64;
pub type CompositionId = u64;

mod blend;
pub use blend::BlendMode;
mod color_curves;
pub use color_curves::{CurveChannel, sample_color_curve};
mod compositions;
mod document;
mod editing;
mod effects;
pub use effects::{
    EffectColorSpace, EffectEdit, EffectId, EffectInstance, EffectKind, EffectParam, ParameterSpec,
};
mod geometry;
mod layer_workflow;
pub use layer_workflow::{LayerClipboard, LayerSwitch};
mod markers;
mod matte;
pub use matte::{MatteMode, TrackMatte};
mod media;
pub use media::MediaReplacement;
mod guides;
mod precompositions;
pub use guides::{Guide, GuideAxis};
mod selection_transform;
pub use selection_transform::AlignTarget;
mod audio_controls;
pub use audio_controls::AudioParam;
mod audio;
pub use audio::AudioMetadata;
mod assets;
mod footage_interpretation;
mod image_sequence;
pub use footage_interpretation::{AlphaInterpretation, FootageInterpretation};
pub use image_sequence::MissingFramePolicy;
mod time;
mod time_remap;
pub use assets::{AssetId, AssetLibrary, FolderId, MediaAsset, ProjectFolder, ProjectItem};
mod paths;
pub use paths::{PathMask, PathMaskMode, PathVertex, VectorPath};
mod shapes;
mod text_style;
pub use shapes::{Shape, ShapeKind};
pub use text_style::{TextAlign, TextStyle};
mod tracks;
pub use editing::{Content, Effects, KeyCopy, KeyRef, Mask, VideoPlayback};
pub use geometry::{Affine, Bezier};
pub use markers::{Marker, MarkerEdit, MarkerId, MarkerTarget};
pub use time::{FrameRate, FrameRounding};
pub use tracks::{PropertyPath, TrackEdit};

#[derive(Clone, Copy, Debug)]
pub enum Alignment {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Interpolation {
    #[default]
    Linear,
    Hold,
    /// Smoothstep interpolation, not After Effects temporal Bezier compatibility.
    Smooth,
    Bezier(Bezier),
}

impl Interpolation {
    pub fn next(self) -> Self {
        match self {
            Self::Linear => Self::Hold,
            Self::Hold => Self::Smooth,
            Self::Smooth => Self::Bezier(Bezier::default()),
            Self::Bezier(_) => Self::Linear,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Hold => "Hold",
            Self::Smooth => "Smoothstep",
            Self::Bezier(_) => "Bezier",
        }
    }
    fn valid(self) -> bool {
        match self {
            Self::Bezier(curve) => curve.valid(),
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Property {
    PositionX,
    PositionY,
    AnchorX,
    AnchorY,
    ScaleX,
    ScaleY,
    Rotation,
    Opacity,
}

impl Property {
    pub const ALL: [Self; 8] = [
        Self::PositionX,
        Self::PositionY,
        Self::AnchorX,
        Self::AnchorY,
        Self::ScaleX,
        Self::ScaleY,
        Self::Rotation,
        Self::Opacity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::PositionX => "Position X",
            Self::PositionY => "Position Y",
            Self::AnchorX => "Anchor X",
            Self::AnchorY => "Anchor Y",
            Self::ScaleX => "Scale X (%)",
            Self::ScaleY => "Scale Y (%)",
            Self::Rotation => "Rotation (deg)",
            Self::Opacity => "Opacity (%)",
        }
    }

    fn accepts(self, value: f64) -> bool {
        value.is_finite()
            && match self {
                Self::Opacity => (0.0..=100.0).contains(&value),
                Self::ScaleX | Self::ScaleY => (-10_000.0..=10_000.0).contains(&value),
                _ => value.abs() <= 1_000_000.0,
            }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub value: f64,
    /// Controls the segment leaving this keyframe.
    pub interpolation: Interpolation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimatedProperty {
    value: f64,
    keys: BTreeMap<Frame, Keyframe>,
}

impl AnimatedProperty {
    fn new(value: f64) -> Self {
        Self {
            value,
            keys: BTreeMap::new(),
        }
    }

    pub fn keys(&self) -> &BTreeMap<Frame, Keyframe> {
        &self.keys
    }

    pub fn value_at(&self, frame: Frame) -> f64 {
        self.sample(frame as f64)
    }

    /// Fractional frames are used by the value graph and future subframe rendering.
    pub fn sample(&self, frame: f64) -> f64 {
        let frame = if frame.is_finite() {
            frame.max(0.0)
        } else {
            0.0
        };
        let left = self.keys.range(..=frame.floor() as Frame).next_back();
        let right = self.keys.range(frame.ceil() as Frame..).next();
        match (left, right) {
            (Some((start, a)), Some((end, b))) if start != end => {
                let t = (frame - *start as f64) / (end - start) as f64;
                let t = match a.interpolation {
                    Interpolation::Linear => t,
                    Interpolation::Hold => 0.0,
                    Interpolation::Smooth => t * t * (3.0 - 2.0 * t),
                    Interpolation::Bezier(curve) => curve.progress(t),
                };
                a.value + (b.value - a.value) * t
            }
            (Some((_, key)), _) | (_, Some((_, key))) => key.value,
            _ => self.value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    #[serde(default, skip_serializing_if = "TextStyle::is_default")]
    text_style: TextStyle,
    #[serde(
        default,
        skip_serializing_if = "audio_controls::AudioControls::is_default"
    )]
    audio_controls: audio_controls::AudioControls,
    #[serde(default, skip_serializing_if = "FootageInterpretation::is_default")]
    footage_interpretation: FootageInterpretation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asset: Option<AssetId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    time_remap: Option<AnimatedProperty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    track_matte: Option<TrackMatte>,
    #[serde(default, skip_serializing_if = "BlendMode::is_normal")]
    blend_mode: BlendMode,
    #[serde(default, skip_serializing_if = "markers::Markers::is_default")]
    markers: markers::Markers,
    #[serde(default)]
    content: Content,
    #[serde(default)]
    effects: Effects,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    effect_stack: Vec<EffectInstance>,
    #[serde(default = "effects::first_effect_id")]
    next_effect_id: EffectId,
    #[serde(default)]
    mask: Option<Mask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    path_masks: Vec<PathMask>,
    id: LayerId,
    name: String,
    visible: bool,
    locked: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    solo: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    shy: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    guide: bool,
    width: f64,
    height: f64,
    color: u32,
    properties: BTreeMap<Property, AnimatedProperty>,
    #[serde(default)]
    in_frame: Frame,
    #[serde(default)]
    out_frame: Option<Frame>,
    #[serde(default)]
    parent: Option<LayerId>,
    /// Compensation applied before local transforms, preserving pose on reparenting.
    #[serde(default)]
    transform_offset: Affine,
}

impl Layer {
    pub fn text_style(&self) -> TextStyle {
        self.text_style
    }

    pub fn blend_mode(&self) -> BlendMode {
        self.blend_mode
    }
    pub fn solo(&self) -> bool {
        self.solo
    }
    pub fn shy(&self) -> bool {
        self.shy
    }
    pub fn guide(&self) -> bool {
        self.guide
    }
    pub fn content(&self) -> &Content {
        &self.content
    }
    pub fn effects(&self) -> Effects {
        self.effects
    }
    pub fn path_masks(&self) -> &[PathMask] {
        &self.path_masks
    }
    pub fn mask(&self) -> Option<Mask> {
        self.mask
    }
    pub fn width(&self) -> f64 {
        self.width
    }
    pub fn height(&self) -> f64 {
        self.height
    }
    pub fn parent(&self) -> Option<LayerId> {
        self.parent
    }
    pub fn local_transform(&self, frame: Frame) -> Affine {
        let v = |p| self.property(p).value_at(frame);
        let (sin, cos) = v(Property::Rotation).to_radians().sin_cos();
        let (sx, sy) = (v(Property::ScaleX) / 100.0, v(Property::ScaleY) / 100.0);
        let (a, b, c, d) = (cos * sx, sin * sx, -sin * sy, cos * sy);
        Affine([
            a,
            b,
            c,
            d,
            v(Property::PositionX) - a * v(Property::AnchorX) - c * v(Property::AnchorY),
            v(Property::PositionY) - b * v(Property::AnchorX) - d * v(Property::AnchorY),
        ])
    }
    pub fn in_frame(&self) -> Frame {
        self.in_frame
    }
    pub fn out_frame(&self, duration: Frame) -> Frame {
        self.out_frame.unwrap_or(duration)
    }
    pub fn active_at(&self, frame: Frame, duration: Frame) -> bool {
        self.visible && frame >= self.in_frame && frame < self.out_frame(duration)
    }
    pub fn id(&self) -> LayerId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn locked(&self) -> bool {
        self.locked
    }
    pub fn color(&self) -> u32 {
        self.color
    }
    pub fn property(&self, property: Property) -> &AnimatedProperty {
        &self.properties[&property]
    }

    /// Unparented corners, ignoring compensation. Renderers should use Composition::corners_at.
    pub fn corners_at(&self, frame: Frame) -> [[f64; 2]; 4] {
        let value = |property| self.property(property).value_at(frame);
        let angle = value(Property::Rotation).to_radians();
        let (sin, cos) = angle.sin_cos();
        [
            [0.0, 0.0],
            [self.width, 0.0],
            [self.width, self.height],
            [0.0, self.height],
        ]
        .map(|[x, y]| {
            let x = (x - value(Property::AnchorX)) * value(Property::ScaleX) / 100.0;
            let y = (y - value(Property::AnchorY)) * value(Property::ScaleY) / 100.0;
            [
                x * cos - y * sin + value(Property::PositionX),
                x * sin + y * cos + value(Property::PositionY),
            ]
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Composition {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    guides: Vec<Guide>,
    #[serde(default, skip_serializing_if = "markers::Markers::is_default")]
    markers: markers::Markers,
    name: String,
    width: u32,
    height: u32,
    fps: FrameRate,
    /// Non-drop-frame display offset; source sampling remains zero based.
    #[serde(default)]
    display_start: Frame,
    duration: Frame,
    /// Preview and opaque-output matte; does not change the composition's alpha.
    #[serde(default)]
    background_color: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    work_area: Option<[Frame; 2]>,
    #[serde(default, skip_serializing_if = "is_false")]
    hide_shy: bool,
    layers: Vec<Layer>,
}

impl Composition {
    pub fn display_start(&self) -> Frame {
        self.display_start
    }
    pub fn timecode(&self, frame: Frame) -> String {
        self.fps
            .timecode(u64::from(self.display_start) + u64::from(frame))
    }
    pub fn hide_shy(&self) -> bool {
        self.hide_shy
    }
    pub fn layer_enabled(&self, layer: &Layer, include_guides: bool) -> bool {
        layer.visible
            && (include_guides || !layer.guide)
            && (layer.solo || !self.layers.iter().any(|l| l.solo))
    }
    pub fn layer_active(&self, layer: &Layer, frame: Frame, include_guides: bool) -> bool {
        self.layer_enabled(layer, include_guides) && layer.active_at(frame, self.duration)
    }
    pub fn world_transform(&self, id: LayerId, frame: Frame) -> Option<Affine> {
        let mut current = Some(id);
        let mut result = Affine::default();
        for _ in 0..=self.layers.len() {
            let Some(id) = current else {
                return result.valid().then_some(result);
            };
            let layer = self.layer(id)?;
            result = layer
                .transform_offset
                .compose(layer.local_transform(frame))
                .compose(result);
            current = layer.parent;
        }
        None
    }
    pub fn position_space(&self, id: LayerId, frame: Frame) -> Option<Affine> {
        let layer = self.layer(id)?;
        let parent = match layer.parent {
            Some(parent) => self.world_transform(parent, frame)?,
            None => Affine::default(),
        };
        Some(parent.compose(layer.transform_offset))
    }
    pub fn corners_at(&self, id: LayerId, frame: Frame) -> Option<[[f64; 2]; 4]> {
        let layer = self.layer(id)?;
        let world = self.world_transform(id, frame)?;
        Some(
            [
                [0.0, 0.0],
                [layer.width, 0.0],
                [layer.width, layer.height],
                [0.0, layer.height],
            ]
            .map(|p| world.point(p)),
        )
    }
    pub fn can_parent(&self, child: LayerId, parent: Option<LayerId>) -> bool {
        let mut current = parent;
        for _ in 0..=self.layers.len() {
            let Some(id) = current else {
                return true;
            };
            if id == child {
                return false;
            }
            let Some(layer) = self.layer(id) else {
                return false;
            };
            current = layer.parent;
        }
        false
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn fps(&self) -> FrameRate {
        self.fps
    }
    pub fn duration(&self) -> Frame {
        self.duration
    }
    pub fn background_color(&self) -> u32 {
        self.background_color
    }
    pub fn work_area(&self) -> std::ops::Range<Frame> {
        let [start, end] = self.work_area.unwrap_or([0, self.duration]);
        start..end
    }
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    #[serde(default, skip_serializing_if = "AssetLibrary::is_default")]
    asset_library: AssetLibrary,
    version: u32,
    next_layer_id: LayerId,
    composition: Composition,
    #[serde(default = "first_composition_id")]
    composition_id: CompositionId,
    #[serde(default = "next_composition_id")]
    next_composition_id: CompositionId,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    other_compositions: BTreeMap<CompositionId, Composition>,
}
fn first_composition_id() -> CompositionId {
    1
}
fn is_false(value: &bool) -> bool {
    !value
}
fn next_composition_id() -> CompositionId {
    2
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: 1,
            asset_library: AssetLibrary::default(),
            next_layer_id: 1,
            composition_id: 1,
            next_composition_id: 2,
            other_compositions: BTreeMap::new(),
            composition: Composition {
                guides: Vec::new(),
                markers: Default::default(),
                name: "Composition 01".into(),
                width: 1920,
                height: 1080,
                fps: 30.into(),
                display_start: 0,
                duration: 150,
                background_color: 0x000000,
                work_area: None,
                hide_shy: false,
                layers: Vec::new(),
            },
        }
    }
}

impl Project {
    pub fn composition(&self) -> &Composition {
        &self.composition
    }

    pub fn to_json(&self) -> Result<String, String> {
        document::encode(self)
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut project = document::decode(json)?;
        project.validate()?;
        project.sync_assets()?;
        project.validate()?;
        Ok(project)
    }

    fn validate(&self) -> Result<(), String> {
        if !(1..=29).contains(&self.version) {
            return Err("Unsupported project version".into());
        }
        if self.version < 9
            && (self.composition_id != 1
                || self.next_composition_id != 2
                || !self.other_compositions.is_empty())
        {
            return Err("Multiple compositions require project version 9".into());
        }
        if self.composition_id == 0
            || self.composition_id >= self.next_composition_id
            || self.other_compositions.len() >= 100
            || self.other_compositions.contains_key(&self.composition_id)
            || self
                .other_compositions
                .keys()
                .any(|id| *id == 0 || *id >= self.next_composition_id)
            || self.next_composition_id == u64::MAX
        {
            return Err("Invalid composition IDs".into());
        }
        let mut ids = BTreeSet::new();
        let mut images = BTreeSet::new();
        let mut image_bytes = 0usize;
        assets::validate(self)?;
        for asset in self.asset_library.assets.values() {
            if let Content::Image { png } = &asset.content {
                if images.insert(png.as_ptr() as usize) {
                    image_bytes = image_bytes.saturating_add(png.len());
                }
            }
        }
        if image_bytes > document::MAX_IMAGE_BYTES {
            return Err("Embedded image assets exceed 128 MiB".into());
        }
        for (_, comp) in self.compositions() {
            for layer in &comp.layers {
                let has_path = matches!(&layer.content, Content::Shape(s) if s.path.is_some());
                if (has_path || !layer.path_masks.is_empty()) && self.version < 29 {
                    return Err("Vector paths require project version 29".into());
                }
                if layer.path_masks.len() > 64 || layer.path_masks.iter().any(|m| !m.valid()) {
                    return Err(
                        "Invalid path mask: at most 64 closed paths with 3–1024 finite vertices"
                            .into(),
                    );
                }
            }
            matte::validate(comp, self.version)?;
            guides::validate(&comp.guides)?;
            if self.version < 27
                && comp
                    .layers
                    .iter()
                    .any(|l| matches!(l.content(), Content::Shape(_)))
            {
                return Err("Shape content requires project version 27".into());
            }
            if self.version < 20 && !comp.guides.is_empty() {
                return Err("Composition guides require project version 20".into());
            }
            if self.version < 19
                && comp.layers.iter().flat_map(|l| &l.effect_stack).any(|e| {
                    matches!(
                        e.kind(),
                        EffectKind::Curves
                            | EffectKind::LinearGradient
                            | EffectKind::RadialGradient
                    )
                })
            {
                return Err("Curves and gradients require project version 19".into());
            }
            if self.version < 17 && comp.layers.iter().any(|l| !l.blend_mode.is_normal()) {
                return Err("Layer blending modes require project version 17".into());
            }
            if self.version < 16
                && comp
                    .layers
                    .iter()
                    .any(|l| matches!(l.content, Content::Solid | Content::Adjustment))
            {
                return Err("Solid and adjustment sources require project version 16".into());
            }
            if self.version < 14 && (comp.fps.denominator() != 1 || comp.display_start != 0) {
                return Err(
                    "Fractional frame rates and start timecode require project version 14".into(),
                );
            }
            comp.markers.validate(comp.duration)?;
            if self.version < 13
                && (!comp.markers.is_default()
                    || comp.layers.iter().any(|l| !l.markers.is_default()))
            {
                return Err("Markers require project version 13".into());
            }
            if self.version < 12 && comp.layers.iter().any(|l| !l.effect_stack.is_empty()) {
                return Err("Effect stacks require project version 12".into());
            }
            if self.version < 11
                && (comp.hide_shy
                    || comp
                        .layers
                        .iter()
                        .any(|l| l.solo || l.shy || l.guide || matches!(l.content, Content::Null)))
            {
                return Err("Layer switches and null objects require project version 11".into());
            }
            if !(1..=16_384).contains(&comp.width)
                || !(1..=16_384).contains(&comp.height)
                || !comp.fps.valid()
                || comp.duration == 0
                || comp.duration > comp.fps.max_duration()
                || comp.display_start >= comp.fps.nominal() * 86_400
                || comp.layers.len() > 1_000
                || comp.name.len() > 1024
                || comp.background_color > 0xffffff
                || comp.work_area().is_empty()
                || comp.work_area().end > comp.duration
            {
                return Err("Invalid composition settings".into());
            }
            for layer in &comp.layers {
                time_remap::validate(layer, comp.duration, self.version)?;
                if !layer.text_style.valid()
                    || (!layer.text_style.is_default() && self.version < 28)
                {
                    return Err("Invalid or unsupported text style".into());
                }
                audio_controls::validate(layer, comp.duration, self.version)?;
                layer.markers.validate(comp.duration)?;
                effects::validate(layer, comp.duration)?;
                if let Content::Image { png } = &layer.content {
                    if images.insert(png.as_ptr() as usize) {
                        image_bytes = image_bytes.saturating_add(png.len());
                    }
                    if image_bytes > document::MAX_IMAGE_BYTES {
                        return Err("Embedded images exceed 128 MiB. Remove unused image layers before importing more.".into());
                    }
                }
                editing::validate_content(&layer.content, layer.effects, layer.mask)?;
                if layer.id == 0
                    || !layer.transform_offset.valid()
                    || !comp.can_parent(layer.id, layer.parent)
                    || layer.in_frame >= layer.out_frame(comp.duration)
                    || layer.out_frame(comp.duration) > comp.duration
                    || layer.id >= self.next_layer_id
                    || !ids.insert(layer.id)
                    || layer.name.len() > 1024
                    || layer.color > 0xff_ffff
                    || !layer.width.is_finite()
                    || !(1.0..=16_384.0).contains(&layer.width)
                    || !layer.height.is_finite()
                    || !(1.0..=16_384.0).contains(&layer.height)
                    || layer.properties.len() != Property::ALL.len()
                {
                    return Err("Invalid layer".into());
                }
                for property in Property::ALL {
                    let track = layer
                        .properties
                        .get(&property)
                        .ok_or("Missing transform property")?;
                    if !property.accepts(track.value)
                        || track.keys.iter().any(|(frame, key)| {
                            *frame >= comp.duration
                                || !property.accepts(key.value)
                                || !key.interpolation.valid()
                        })
                    {
                        return Err("Invalid property or keyframe".into());
                    }
                }
            }
        }
        if ids.len() > 1000 {
            return Err("Project limit is 1000 layers across all compositions".into());
        }
        if self.next_layer_id == 0 || self.next_layer_id == u64::MAX {
            return Err("Invalid next layer ID".into());
        }
        precompositions::validate(self)?;
        Ok(())
    }
}

/// The future scripting bridge and native controls both dispatch these commands.
#[derive(Clone, Debug)]
pub enum Command {
    SetSequenceMissing {
        asset: AssetId,
        missing: MissingFramePolicy,
    },
    RelinkSequence {
        asset: AssetId,
        frames: std::sync::Arc<Vec<String>>,
    },
    InterpretAsset {
        asset: AssetId,
        interpretation: FootageInterpretation,
    },
    CompositionFromAsset(AssetId),
    ImportAsset {
        content: Content,
        width: f64,
        height: f64,
        name: String,
        folder: Option<FolderId>,
        frame: Option<Frame>,
    },
    AddAssetLayer {
        asset: AssetId,
        frame: Frame,
    },
    NewProjectFolder {
        name: String,
        parent: Option<FolderId>,
    },
    RenameProjectItem {
        item: ProjectItem,
        name: String,
    },
    MoveProjectItem {
        item: ProjectItem,
        folder: Option<FolderId>,
    },
    DeleteProjectItem(ProjectItem),
    SetAudioEnabled {
        id: LayerId,
        enabled: bool,
    },
    EditAudio {
        id: LayerId,
        parameter: AudioParam,
        edit: TrackEdit,
    },
    FadeAudio {
        id: LayerId,
        start: Frame,
        end: Frame,
        fade_in: bool,
    },
    SetTimeRemap {
        id: LayerId,
        enabled: bool,
    },
    FreezeTimeRemap {
        id: LayerId,
        frame: Frame,
    },
    EditTimeRemap {
        id: LayerId,
        edit: TrackEdit,
    },
    SetGuides(Vec<Guide>),
    SetTrackMatte {
        id: LayerId,
        matte: Option<TrackMatte>,
    },
    SetBlendMode {
        id: LayerId,
        mode: BlendMode,
    },
    AddSolid,
    AddAdjustment,
    ConfigureSolid {
        id: LayerId,
        width: u32,
        height: u32,
        color: u32,
    },
    RelinkMedia(Vec<MediaReplacement>),
    Marker {
        target: MarkerTarget,
        edit: MarkerEdit,
    },
    EditTrack {
        id: LayerId,
        property: PropertyPath,
        edit: TrackEdit,
    },
    Batch(Vec<Command>),
    Effect {
        id: LayerId,
        edit: EffectEdit,
    },
    AddNull,
    SetLayerSwitch {
        id: LayerId,
        switch: LayerSwitch,
        enabled: bool,
    },
    SetHideShy(bool),
    PasteLayers(LayerClipboard),
    NewComposition,
    DuplicateComposition,
    DeleteComposition,
    AddCompositionLayer {
        composition: CompositionId,
        frame: Frame,
    },
    Precompose {
        layers: Vec<LayerId>,
        name: String,
    },
    SetCompositionBackground(u32),
    SetWorkArea {
        start: Frame,
        end: Frame,
    },
    AddBackgroundSolid,
    TrimLayers {
        ids: Vec<LayerId>,
        frame: Frame,
        start: bool,
    },
    NudgeLayers {
        ids: Vec<LayerId>,
        frame: Frame,
        delta: [f64; 2],
    },
    DuplicateLayers(Vec<LayerId>),
    SplitLayers {
        ids: Vec<LayerId>,
        frame: Frame,
    },
    SetAnchor {
        id: LayerId,
        frame: Frame,
        x: f64,
        y: f64,
    },
    AddContent {
        content: Content,
        width: f64,
        height: f64,
        name: String,
    },
    SetTextStyle {
        id: LayerId,
        style: TextStyle,
    },
    SetContent {
        id: LayerId,
        content: Content,
    },
    /// Changes footage sampling only; keeps the layer range and transform keys.
    SetVideoSpeed {
        id: LayerId,
        speed: f64,
    },
    SetVideoSourceIn {
        id: LayerId,
        seconds: f64,
    },
    ReverseVideo {
        id: LayerId,
    },
    FreezeVideo {
        id: LayerId,
        frame: Frame,
    },
    SetEffects {
        id: LayerId,
        effects: Effects,
    },
    SetPathMasks {
        id: LayerId,
        masks: Vec<PathMask>,
    },
    SetMask {
        id: LayerId,
        mask: Option<Mask>,
    },
    SetColor {
        id: LayerId,
        color: u32,
    },
    ShiftLayer {
        id: LayerId,
        delta: i64,
    },
    MoveKeys {
        keys: Vec<KeyRef>,
        delta: i64,
    },
    DeleteKeys(Vec<KeyRef>),
    PasteKeys {
        keys: Vec<KeyCopy>,
        frame: Frame,
        target: Option<LayerId>,
    },
    AddRectangle,
    AlignLayer {
        id: LayerId,
        frame: Frame,
        alignment: Alignment,
    },
    AlignLayers {
        ids: Vec<LayerId>,
        frame: Frame,
        alignment: Alignment,
        target: AlignTarget,
    },
    DistributeLayers {
        ids: Vec<LayerId>,
        frame: Frame,
        alignment: Alignment,
    },
    RotateLayers {
        ids: Vec<LayerId>,
        frame: Frame,
        degrees: f64,
    },
    ScaleLayers {
        ids: Vec<LayerId>,
        frame: Frame,
        factor: [f64; 2],
        /// Percentage-point adjustment for a driving axis starting at zero.
        offset: [f64; 2],
    },
    SetParent {
        id: LayerId,
        parent: Option<LayerId>,
        frame: Frame,
    },
    EditKeyframe {
        id: LayerId,
        property: Property,
        from: Frame,
        to: Frame,
        value: f64,
    },
    SetPosition {
        id: LayerId,
        frame: Frame,
        x: f64,
        y: f64,
    },
    DuplicateLayer(LayerId),
    RenameLayer {
        id: LayerId,
        name: String,
    },
    SetLayerRange {
        id: LayerId,
        start: Frame,
        end: Frame,
    },
    ConfigureComposition {
        name: String,
        width: u32,
        height: u32,
        fps: u32,
        duration: Frame,
    },
    ConfigureCompositionRate {
        name: String,
        width: u32,
        height: u32,
        fps: FrameRate,
        duration: Frame,
        display_start: Frame,
    },
    MoveKeyframe {
        id: LayerId,
        property: Property,
        from: Frame,
        to: Frame,
    },
    ToggleAnimation {
        id: LayerId,
        property: Property,
        frame: Frame,
    },
    RemoveLayer(LayerId),
    MoveLayer {
        id: LayerId,
        index: usize,
    },
    ToggleVisible(LayerId),
    ToggleLocked(LayerId),
    SetValue {
        id: LayerId,
        property: Property,
        frame: Frame,
        value: f64,
    },
    ToggleKeyframe {
        id: LayerId,
        property: Property,
        frame: Frame,
    },
    SetInterpolation {
        id: LayerId,
        property: Property,
        frame: Frame,
        interpolation: Interpolation,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Snapshot {
    project: Project,
    selected: Option<LayerId>,
}

#[derive(Default)]
pub struct Editor {
    current: Snapshot,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Editor {
    pub fn project(&self) -> &Project {
        &self.current.project
    }
    pub fn selected(&self) -> Option<LayerId> {
        self.current.selected
    }
    pub fn selected_layer(&self) -> Option<&Layer> {
        self.selected()
            .and_then(|id| self.project().composition.layer(id))
    }
    pub fn select(&mut self, id: LayerId) {
        if self.project().composition.layer(id).is_some() {
            self.current.selected = Some(id);
        }
    }
    pub fn clear_selection(&mut self) {
        self.current.selected = None;
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn record(&mut self, previous: Snapshot) {
        const HISTORY_LIMIT: usize = 100;
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(previous);
        self.redo.clear();
    }

    /// Loading is undoable, so opening a project does not discard current work.
    pub fn replace_project(&mut self, project: Project) -> Result<(), String> {
        project.validate()?;
        let selected = project.composition.layers.first().map(Layer::id);
        let previous = std::mem::replace(&mut self.current, Snapshot { project, selected });
        self.record(previous);
        Ok(())
    }

    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.redo
                .push(std::mem::replace(&mut self.current, previous));
        }
    }
    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.current, next));
        }
    }

    pub fn execute(&mut self, command: Command) -> Result<(), String> {
        // Apply to a candidate so invalid commands never partially mutate the project.
        let mut next = self.current.clone();
        apply(&mut next, command)?;
        // Older applications must reject projects they cannot render faithfully.
        if next.project.composition.layers.iter().any(|layer| {
            layer.parent.is_some()
                || layer.transform_offset != Affine::default()
                || layer.properties.values().any(|track| {
                    track
                        .keys
                        .values()
                        .any(|key| matches!(key.interpolation, Interpolation::Bezier(_)))
                })
        }) {
            next.project.version = 2;
        }
        if next.project.composition.layers.iter().any(|l| {
            l.content != Content::Rectangle || l.effects != Effects::default() || l.mask.is_some()
        }) {
            next.project.version = 3;
        }
        if next
            .project
            .composition
            .layers
            .iter()
            .any(|l| matches!(l.content, Content::Video { .. }))
        {
            next.project.version = 4;
        }
        if next.project.composition.layers.iter().any(|l| {
            matches!(l.content, Content::Video { playback, .. } if playback != VideoPlayback::default())
        }) {
            next.project.version = 5;
        }
        if next.project.composition.background_color != 0 {
            next.project.version = 6;
        }
        if next
            .project
            .composition
            .layers
            .iter()
            .any(|l| matches!(l.content, Content::Image { .. }))
        {
            next.project.version = 7;
        }
        if next.project.composition.work_area.is_some() {
            next.project.version = 8;
        }
        if next.project.next_composition_id > 2
            || next.project.composition_id != 1
            || !next.project.other_compositions.is_empty()
        {
            next.project.version = 9;
        }
        if next.project.compositions().into_iter().any(|(_, comp)| {
            comp.layers
                .iter()
                .any(|layer| matches!(layer.content, Content::Composition { .. }))
        }) {
            next.project.version = 10;
        }
        if next.project.compositions().into_iter().any(|(_, comp)| {
            comp.hide_shy
                || comp
                    .layers
                    .iter()
                    .any(|l| l.solo || l.shy || l.guide || matches!(l.content, Content::Null))
        }) {
            next.project.version = 11;
        }
        if next
            .project
            .compositions()
            .into_iter()
            .any(|(_, c)| c.layers.iter().any(|l| !l.effect_stack.is_empty()))
        {
            next.project.version = 12;
        }
        if next.project.compositions().into_iter().any(|(_, c)| {
            !c.markers.is_default() || c.layers.iter().any(|l| !l.markers.is_default())
        }) {
            next.project.version = 13;
        }
        if next
            .project
            .compositions()
            .into_iter()
            .any(|(_, c)| c.fps.denominator() != 1 || c.display_start != 0)
        {
            next.project.version = 14;
        }
        if next.project.compositions().into_iter().any(|(_, c)| {
            c.layers
                .iter()
                .any(|l| matches!(l.content, Content::Solid | Content::Adjustment))
        }) {
            next.project.version = 16;
        }
        if next
            .project
            .compositions()
            .into_iter()
            .any(|(_, c)| c.layers.iter().any(|l| !l.blend_mode.is_normal()))
        {
            next.project.version = 17;
        }
        if next
            .project
            .compositions()
            .into_iter()
            .any(|(_, c)| c.layers.iter().any(|l| l.track_matte.is_some()))
        {
            next.project.version = 18;
        }
        if next.project.compositions().into_iter().any(|(_, c)| {
            c.layers.iter().flat_map(|l| &l.effect_stack).any(|e| {
                matches!(
                    e.kind(),
                    EffectKind::Curves | EffectKind::LinearGradient | EffectKind::RadialGradient
                )
            })
        }) {
            next.project.version = 19;
        }
        if next
            .project
            .compositions()
            .into_iter()
            .any(|(_, c)| !c.guides.is_empty())
        {
            next.project.version = 20;
        }
        if next
            .project
            .compositions()
            .into_iter()
            .any(|(_, c)| c.layers.iter().any(|l| l.time_remap.is_some()))
        {
            next.project.version = 21;
        }
        next.project.sync_assets()?;
        if next
            .project
            .compositions()
            .into_iter()
            .any(|(_, c)| c.layers.iter().any(|l| !l.audio_controls.is_default()))
        {
            next.project.version = 26;
        }
        if next.project.compositions().into_iter().any(|(_, c)| {
            c.layers
                .iter()
                .any(|l| matches!(l.content, Content::Shape(_)))
        }) {
            next.project.version = next.project.version.max(27);
        }
        if next
            .project
            .compositions()
            .into_iter()
            .any(|(_, c)| c.layers.iter().any(|l| !l.text_style.is_default()))
        {
            next.project.version = next.project.version.max(28);
        }
        if next.project.compositions().into_iter().any(|(_, c)| {
            c.layers.iter().any(|l| {
                !l.path_masks.is_empty()
                    || matches!(&l.content, Content::Shape(s) if s.path.is_some())
            })
        }) {
            next.project.version = next.project.version.max(29);
        }
        next.project.validate()?;
        if next != self.current {
            let previous = std::mem::replace(&mut self.current, next);
            self.record(previous);
        }
        Ok(())
    }
}

fn apply(state: &mut Snapshot, command: Command) -> Result<(), String> {
    if let Some(result) = audio_controls::apply(state, &command) {
        return result;
    }
    if let Some(result) = image_sequence::apply(state, &command) {
        return result;
    }
    if let Some(result) = footage_interpretation::apply(state, &command) {
        return result;
    }
    if let Some(result) = assets::apply(state, &command) {
        return result;
    }
    if let Some(result) = time_remap::apply(state, &command) {
        return result;
    }
    if let Command::SetGuides(guides) = command {
        guides::validate(&guides)?;
        state.project.composition.guides = guides;
        return Ok(());
    }
    if let Command::SetTrackMatte { id, matte } = command {
        return matte::set(state, id, matte);
    }
    if let Command::RelinkMedia(replacements) = command {
        return media::relink(state, replacements);
    }
    if let Command::ConfigureComposition {
        name,
        width,
        height,
        fps,
        duration,
    } = command
    {
        return apply(
            state,
            Command::ConfigureCompositionRate {
                name,
                width,
                height,
                fps: fps.into(),
                duration,
                display_start: state.project.composition.display_start,
            },
        );
    }
    if let Command::Marker { target, edit } = command {
        return markers::apply(state, target, edit);
    }
    if let Command::EditTrack { id, property, edit } = command {
        return apply(state, tracks::command(id, property, edit));
    }
    if let Command::Effect { id, edit } = command {
        return effects::apply(state, id, edit);
    }
    if let Some(result) = layer_workflow::apply(state, &command) {
        return result;
    }
    if let Some(result) = precompositions::apply(state, &command) {
        return result;
    }
    if let Some(result) = compositions::apply(state, &command) {
        return result;
    }
    if let Command::SetWorkArea { start, end } = command {
        let comp = &mut state.project.composition;
        if start >= end || end > comp.duration {
            return Err("Work area must be nonempty and inside the composition".into());
        }
        comp.work_area = (start != 0 || end != comp.duration).then_some([start, end]);
        return Ok(());
    }
    if let Command::SetCompositionBackground(color) = command {
        if color > 0xffffff {
            return Err("Background color must be a 24-bit RGB color".into());
        }
        state.project.composition.background_color = color;
        return Ok(());
    }
    if let Some(result) = editing::apply_extended(state, &command) {
        return result;
    }
    if let Some(result) = selection_transform::apply(state, &command) {
        return result;
    }
    if let Command::AlignLayer {
        id,
        frame,
        alignment,
    } = command
    {
        let comp = &state.project.composition;
        let layer = comp.layer(id).ok_or("Layer not found")?;
        let corners = comp
            .corners_at(id, frame)
            .ok_or("Invalid layer transform")?;
        let min_x = corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let max_x = corners
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let max_y = corners
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        let delta = match alignment {
            Alignment::Left => [-min_x, 0.0],
            Alignment::HorizontalCenter => [(comp.width as f64 - min_x - max_x) / 2.0, 0.0],
            Alignment::Right => [comp.width as f64 - max_x, 0.0],
            Alignment::Top => [0.0, -min_y],
            Alignment::VerticalCenter => [0.0, (comp.height as f64 - min_y - max_y) / 2.0],
            Alignment::Bottom => [0.0, comp.height as f64 - max_y],
        };
        let delta = comp
            .position_space(id, frame)
            .and_then(Affine::inverse)
            .ok_or("Cannot align through a zero-scale parent")?
            .vector(delta);
        let x = layer.property(Property::PositionX).value_at(frame) + delta[0];
        let y = layer.property(Property::PositionY).value_at(frame) + delta[1];
        return apply(state, Command::SetPosition { id, frame, x, y });
    }
    if let Command::EditKeyframe {
        id,
        property,
        from,
        to,
        value,
    } = command
    {
        apply(
            state,
            Command::MoveKeyframe {
                id,
                property,
                from,
                to,
            },
        )?;
        return apply(
            state,
            Command::SetValue {
                id,
                property,
                frame: to,
                value,
            },
        );
    }
    if let Command::SetPosition { id, frame, x, y } = command {
        apply(
            state,
            Command::SetValue {
                id,
                property: Property::PositionX,
                frame,
                value: x,
            },
        )?;
        return apply(
            state,
            Command::SetValue {
                id,
                property: Property::PositionY,
                frame,
                value: y,
            },
        );
    }
    let comp = &mut state.project.composition;
    if let Command::SetParent { id, parent, frame } = command {
        let layer = comp.layer(id).ok_or("Layer not found")?;
        if layer.locked {
            return Err("Unlock the layer before editing".into());
        }
        if frame >= comp.duration || !comp.can_parent(id, parent) {
            return Err("Invalid parent: missing layer or circular hierarchy".into());
        }
        if layer.parent == parent {
            return Ok(());
        }
        let old_space = comp
            .position_space(id, frame)
            .ok_or("Invalid parent transform")?;
        let new_space = match parent {
            Some(id) => comp
                .world_transform(id, frame)
                .ok_or("Invalid parent transform")?,
            None => Affine::default(),
        };
        let offset = new_space
            .inverse()
            .ok_or("Cannot parent to a layer with zero scale")?
            .compose(old_space);
        if !offset.valid() {
            return Err("Parent transform is outside supported range".into());
        }
        let layer = comp.layers.iter_mut().find(|l| l.id == id).unwrap();
        layer.parent = parent;
        layer.transform_offset = offset;
        return Ok(());
    }
    if let Command::ConfigureCompositionRate {
        name,
        width,
        height,
        fps,
        duration,
        display_start,
    } = &command
    {
        if *width as u64 * *height as u64 > 33_554_432 {
            return Err("Composition exceeds the 32 megapixel render limit".into());
        }
        if name.trim().is_empty()
            || name.len() > 1024
            || !(1..=16_384).contains(width)
            || !(1..=16_384).contains(height)
            || !fps.valid()
            || *duration == 0
            || *duration > fps.max_duration()
            || *display_start >= fps.nominal() * 86_400
        {
            return Err("Invalid composition settings".into());
        }
        if comp.layers.iter().any(|layer| {
            layer.in_frame >= *duration
                || layer.out_frame.is_some_and(|end| end > *duration)
                || layer
                    .properties
                    .values()
                    .any(|track| track.keys.keys().any(|frame| frame >= duration))
        }) {
            return Err("Duration would exclude existing layer ranges or keyframes".into());
        }
        comp.name = name.trim().into();
        comp.width = *width;
        comp.height = *height;
        comp.fps = *fps;
        comp.display_start = *display_start;
        comp.duration = *duration;
        if let Some([start, end]) = comp.work_area {
            comp.work_area = Some([start.min(duration - 1), end.min(*duration)]);
        }
        return Ok(());
    }
    if let Command::AddRectangle = command {
        if comp.layers.len() >= 1_000 || state.project.next_layer_id >= u64::MAX - 1 {
            return Err("Layer limit reached".into());
        }
        let id = state.project.next_layer_id;
        state.project.next_layer_id += 1;
        let colors = [0x9a8cff, 0x53d8c4, 0xffbc70, 0xf580ad];
        comp.layers.insert(
            0,
            Layer {
                footage_interpretation: Default::default(),
                asset: None,
                text_style: Default::default(),
                audio_controls: Default::default(),
                time_remap: None,
                track_matte: None,
                blend_mode: BlendMode::Normal,
                markers: Default::default(),
                content: Content::default(),
                effects: Effects::default(),
                effect_stack: Vec::new(),
                next_effect_id: 1,
                mask: None,
                path_masks: Vec::new(),
                id,
                name: format!("Rectangle {id}"),
                visible: true,
                locked: false,
                solo: false,
                shy: false,
                guide: false,
                width: 320.0,
                height: 200.0,
                color: colors[((id - 1) % 4) as usize],
                in_frame: 0,
                out_frame: None,
                parent: None,
                transform_offset: Affine::default(),
                properties: Property::ALL
                    .into_iter()
                    .map(|property| {
                        let value = match property {
                            Property::PositionX => comp.width as f64 / 2.0,
                            Property::PositionY => comp.height as f64 / 2.0,
                            Property::AnchorX => 160.0,
                            Property::AnchorY => 100.0,
                            Property::ScaleX | Property::ScaleY | Property::Opacity => 100.0,
                            Property::Rotation => 0.0,
                        };
                        (property, AnimatedProperty::new(value))
                    })
                    .collect(),
            },
        );
        state.selected = Some(id);
        return Ok(());
    }
    let id = match &command {
        Command::RemoveLayer(id)
        | Command::ToggleVisible(id)
        | Command::ToggleLocked(id)
        | Command::DuplicateLayer(id) => *id,
        Command::RenameLayer { id, .. }
        | Command::SetLayerRange { id, .. }
        | Command::MoveKeyframe { id, .. }
        | Command::ToggleAnimation { id, .. } => *id,
        Command::MoveLayer { id, .. }
        | Command::SetValue { id, .. }
        | Command::ToggleKeyframe { id, .. }
        | Command::SetInterpolation { id, .. } => *id,
        Command::AlignLayer { .. }
        | Command::SetParent { .. }
        | Command::EditKeyframe { .. }
        | Command::AddRectangle
        | Command::ConfigureComposition { .. }
        | Command::SetPosition { .. } => unreachable!(),
        _ => unreachable!("extended command handled above"),
    };
    let index = comp
        .layers
        .iter()
        .position(|layer| layer.id == id)
        .ok_or("Layer not found")?;
    let layer = &mut comp.layers[index];
    if layer.locked && !matches!(command, Command::ToggleLocked(_)) {
        return Err("Unlock the layer before editing".into());
    }
    match command {
        Command::DuplicateLayer(_) => {
            if comp.layers.len() >= 1_000 || state.project.next_layer_id >= u64::MAX - 1 {
                return Err("Layer limit reached".into());
            }
            let mut copy = comp.layers[index].clone();
            copy.id = state.project.next_layer_id;
            // Keep a bounded name even when duplicating repeatedly.
            if copy.name.len() < 1000 {
                copy.name.push_str(" copy");
            }
            state.project.next_layer_id += 1;
            state.selected = Some(copy.id);
            comp.layers.insert(index, copy);
        }
        Command::RenameLayer { name, .. } => {
            if name.trim().is_empty() || name.len() > 1024 {
                return Err("Enter a layer name (1–1024 bytes)".into());
            }
            layer.name = name.trim().into();
        }
        Command::SetLayerRange { start, end, .. } => {
            if start >= end || end > comp.duration {
                return Err("Layer range must fit the composition".into());
            }
            layer.in_frame = start;
            layer.out_frame = Some(end);
        }
        Command::MoveKeyframe {
            property, from, to, ..
        } => {
            if to >= comp.duration {
                return Err("Keyframe is outside the composition".into());
            }
            let track = layer
                .properties
                .get_mut(&property)
                .ok_or("Property not found")?;
            if from != to && track.keys.contains_key(&to) {
                return Err("A keyframe already exists at that frame".into());
            }
            let key = track.keys.remove(&from).ok_or("Keyframe not found")?;
            track.keys.insert(to, key);
        }
        Command::ToggleAnimation {
            property, frame, ..
        } => {
            if frame >= comp.duration {
                return Err("Frame out of range".into());
            }
            let track = layer
                .properties
                .get_mut(&property)
                .ok_or("Property not found")?;
            let value = track.value_at(frame);
            if track.keys.is_empty() {
                track.keys.insert(
                    frame,
                    Keyframe {
                        value,
                        interpolation: Interpolation::Linear,
                    },
                );
            } else {
                track.value = value;
                track.keys.clear();
            }
        }
        Command::RemoveLayer(_) => {
            if comp.layers.iter().any(|l| l.parent == Some(id)) {
                return Err("Unparent child layers before deleting this parent".into());
            }
            comp.layers.remove(index);
            if state.selected == Some(id) {
                state.selected = comp.layers.first().map(Layer::id);
            }
        }
        Command::MoveLayer { index: target, .. } => {
            if target >= comp.layers.len() {
                return Err("Layer index out of range".into());
            }
            let layer = comp.layers.remove(index);
            comp.layers.insert(target, layer);
        }
        Command::ToggleVisible(_) => layer.visible = !layer.visible,
        Command::ToggleLocked(_) => layer.locked = !layer.locked,
        Command::SetValue {
            property,
            frame,
            value,
            ..
        } => {
            if frame >= comp.duration || !property.accepts(value) {
                return Err("Value or frame out of range".into());
            }
            let track = layer
                .properties
                .get_mut(&property)
                .ok_or("Property not found")?;
            if track.keys.is_empty() {
                track.value = value;
            } else {
                let interpolation = track
                    .keys
                    .range(..=frame)
                    .next_back()
                    .map_or(Interpolation::Linear, |(_, key)| key.interpolation);
                track.keys.insert(
                    frame,
                    Keyframe {
                        value,
                        interpolation,
                    },
                );
            }
        }
        Command::ToggleKeyframe {
            property, frame, ..
        } => {
            if frame >= comp.duration {
                return Err("Frame out of range".into());
            }
            let track = layer
                .properties
                .get_mut(&property)
                .ok_or("Property not found")?;
            let value = track.value_at(frame);
            if track.keys.remove(&frame).is_some() {
                // Removing the final key retains its value as a static property.
                if track.keys.is_empty() {
                    track.value = value;
                }
            } else {
                track.keys.insert(
                    frame,
                    Keyframe {
                        value,
                        interpolation: Interpolation::Linear,
                    },
                );
            }
        }
        Command::SetInterpolation {
            property,
            frame,
            interpolation,
            ..
        } => {
            if !interpolation.valid() {
                return Err("Invalid Bezier handles".into());
            }
            let key = layer
                .properties
                .get_mut(&property)
                .and_then(|track| track.keys.get_mut(&frame))
                .ok_or("Select a frame containing a keyframe")?;
            key.interpolation = interpolation;
        }
        Command::AlignLayer { .. }
        | Command::SetParent { .. }
        | Command::EditKeyframe { .. }
        | Command::AddRectangle
        | Command::ConfigureComposition { .. }
        | Command::SetPosition { .. } => unreachable!(),
        _ => unreachable!("extended command handled above"),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
