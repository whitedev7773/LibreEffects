//! Append reviewed image/shape/precomposition graphs to a frozen native project.
//! Private source values, media, identities and evidence are runtime inputs only.
//! No source-project parser, script execution, or visual-parity claim is provided.
//! Usage: reference_player_project INPUT.json INPUT_SHA256 NEW.lep

use libre_effects_core::*;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const INPUT_LIMIT: usize = 16 * 1024 * 1024;
const IMAGE_LIMIT: usize = 9 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    contract_version: u32,
    contract_kind: String,
    base_project: FileInput,
    base_schema: u32,
    existing_compositions: Vec<ExistingComposition>,
    existing_media: Vec<ExistingMedia>,
    image_assets: Vec<ImageInput>,
    compositions: Vec<CompositionInput>,
    evidence_files: Vec<FileInput>,
    provenance: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileInput {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExistingComposition {
    source_id: u64,
    native_id: CompositionId,
    name: String,
    layer_count: usize,
    preserve_nested_frame_rate: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExistingMedia {
    source_item_id: u64,
    native_asset_id: AssetId,
    expected_asset: MediaAsset,
    file: FileInput,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageInput {
    source_item_id: u64,
    name: String,
    width: u32,
    height: u32,
    size_bytes: usize,
    file: FileInput,
    relink_evidence: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompositionInput {
    source_id: u64,
    name: String,
    width: u32,
    height: u32,
    fps: FrameRate,
    duration: Frame,
    display_start: Frame,
    work_area: [Frame; 2],
    background: u32,
    initial_frame: Frame,
    preserve_nested_frame_rate: bool,
    layers: Vec<LayerInput>,
    source_evidence: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerInput {
    source_id: u64,
    stack_index_1based: usize,
    name: String,
    content: LayerContent,
    width: f64,
    height: f64,
    parent_source_id: Option<u64>,
    track_matte_source_id: Option<u64>,
    visible: bool,
    audio_enabled: bool,
    label: u8,
    start_frame: i64,
    range: [Frame; 2],
    authored_properties: BTreeMap<Property, f64>,
    materialization_defaults: BTreeMap<Property, f64>,
    fill_effects: Vec<FillInput>,
    source_evidence: Value,
}
#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum LayerContent {
    Image { source_item_id: u64 },
    Composition { source_id: u64 },
    RoundedRectangle { shape: Box<RectangleInput> },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RectangleInput {
    group_name: String,
    rectangle_name: String,
    stroke_name: String,
    fill_name: String,
    group_properties: BTreeMap<Property, f64>,
    rectangle_position: [f64; 2],
    rectangle_size: [f64; 2],
    roundness: f64,
    stroke_width: f64,
    stroke_rgb: [f64; 3],
    stroke_opacity: f64,
    fill_rgb: [f64; 3],
    fill_opacity: f64,
    even_odd: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FillInput {
    name: String,
    enabled: bool,
    rgb: [f64; 3],
    opacity: f64,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(u64::try_from(limit)? + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(format!("File exceeds its byte budget: {}", path.display()).into());
    }
    Ok(bytes)
}
fn verify_digest(bytes: &[u8], expected: &str) -> Result<()> {
    if expected.len() != 64 || digest(bytes) != expected {
        return Err("SHA-256 differs from the reviewed runtime input".into());
    }
    Ok(())
}
fn reviewed_read(file: &FileInput, limit: usize) -> Result<Vec<u8>> {
    let bytes = bounded_read(&file.path, limit)?;
    verify_digest(&bytes, &file.sha256)?;
    Ok(bytes)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
// Keep the example dependency-free apart from the existing example dependencies.
// This canonical encoding retains original PNG bytes without decoding pixels.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        encoded.push(ALPHABET[((bits >> 18) & 63) as usize] as char);
        encoded.push(ALPHABET[((bits >> 12) & 63) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[((bits >> 6) & 63) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(bits & 63) as usize] as char
        } else {
            '='
        });
    }
    encoded
}
fn validate_input(input: &Input, base: &Project) -> Result<()> {
    if input.contract_version != 1
        || input.contract_kind != "reviewed_player_append"
        || input.compositions.is_empty()
        || input.compositions.len() > 32
        || input.image_assets.len() > 64
        || input.evidence_files.len() > 32
        || input.existing_compositions.len() != base.compositions().len()
        || input.existing_media.len() != base.asset_library().assets().len()
        || serde_json::to_value(base)?["version"].as_u64() != Some(u64::from(input.base_schema))
    {
        return Err("Unsupported contract, resource budget, or frozen project inventory".into());
    }
    let mut comp_ids = BTreeSet::new();
    let mut native_ids = BTreeSet::new();
    for old in &input.existing_compositions {
        let comp = base
            .composition_by_id(old.native_id)
            .ok_or("Missing existing composition")?;
        if !comp_ids.insert(old.source_id)
            || !native_ids.insert(old.native_id)
            || comp.name() != old.name
            || comp.layers().len() != old.layer_count
        {
            return Err("Existing source/native composition mapping differs".into());
        }
    }
    let mut image_ids = BTreeSet::new();
    for image in &input.image_assets {
        if !image_ids.insert(image.source_item_id)
            || image.width == 0
            || image.height == 0
            || image.size_bytes > IMAGE_LIMIT
        {
            return Err("Duplicate or invalid image declaration".into());
        }
    }
    for comp in &input.compositions {
        if !comp_ids.insert(comp.source_id)
            || comp.layers.is_empty()
            || comp.layers.len() > 1000
            || comp.initial_frame >= comp.duration
            || comp.work_area[0] >= comp.work_area[1]
            || comp.work_area[1] > comp.duration
        {
            return Err("Invalid composition identity, layer budget, view, or work area".into());
        }
    }
    let mut total_layers = 0;
    for comp in &input.compositions {
        total_layers += comp.layers.len();
        let mut ids = BTreeSet::new();
        for (index, layer) in comp.layers.iter().enumerate() {
            if !ids.insert(layer.source_id)
                || layer.stack_index_1based != index + 1
                || layer.range[0] >= layer.range[1]
                || layer.range[1] > comp.duration
                || layer.fill_effects.len() > 8
            {
                return Err("Invalid layer identity, stack, timing, or effect count".into());
            }
            for property in Property::ALL {
                if layer.authored_properties.contains_key(&property)
                    == layer.materialization_defaults.contains_key(&property)
                {
                    return Err(
                        "Each scalar requires exactly one observed or default policy".into(),
                    );
                }
            }
            match &layer.content {
                LayerContent::Image { source_item_id } if image_ids.contains(source_item_id) => {}
                LayerContent::Composition { source_id }
                    if comp_ids.contains(source_id) && *source_id != comp.source_id => {}
                LayerContent::RoundedRectangle { shape } => {
                    if shape.group_properties.len() != Property::ALL.len() {
                        return Err(
                            "Shape group requires all transform materialization values".into()
                        );
                    }
                    shape_content(shape)?;
                }
                _ => return Err("Missing image/composition dependency or self-reference".into()),
            }
            if !layer.audio_enabled && !matches!(layer.content, LayerContent::Composition { .. }) {
                return Err("Inert content cannot carry a changed native audio switch".into());
            }
        }
        for layer in &comp.layers {
            if [layer.parent_source_id, layer.track_matte_source_id]
                .into_iter()
                .flatten()
                .any(|id| id == layer.source_id || !ids.contains(&id))
            {
                return Err(
                    "Parent or matte must refer to another layer in the same composition".into(),
                );
            }
            if let Some(id) = layer.track_matte_source_id {
                if comp
                    .layers
                    .iter()
                    .find(|l| l.source_id == id)
                    .is_none_or(|l| l.visible)
                {
                    return Err("This reviewed subset requires hidden Alpha matte providers".into());
                }
            }
        }
    }
    if total_layers > 2000 {
        return Err("Appended layer budget exceeded".into());
    }
    Ok(())
}
fn shape_content(shape: &RectangleInput) -> Result<Content> {
    let mut rect =
        ContentsNode::with_defaults(ContentsKind::Parametric(ShapeKind::RoundedRectangle));
    rect.name = shape.rectangle_name.clone();
    for (parameter, value) in [
        (ContentsParam::Width, shape.rectangle_size[0]),
        (ContentsParam::Height, shape.rectangle_size[1]),
        (ContentsParam::Shape(ShapeParam::Roundness), shape.roundness),
        (
            ContentsParam::Transform(Property::PositionX),
            shape.rectangle_position[0],
        ),
        (
            ContentsParam::Transform(Property::PositionY),
            shape.rectangle_position[1],
        ),
    ] {
        rect.set_static_value(parameter, value)?;
    }
    let mut stroke = ContentsNode::with_defaults(ContentsKind::Stroke(ShapeStroke::default()));
    stroke.name = shape.stroke_name.clone();
    stroke.set_static_value(
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        shape.stroke_width,
    )?;
    stroke.set_static_value(
        ContentsParam::Shape(ShapeParam::StrokeOpacity),
        shape.stroke_opacity,
    )?;
    for (parameter, value) in [
        ShapeParam::StrokeRed,
        ShapeParam::StrokeGreen,
        ShapeParam::StrokeBlue,
    ]
    .into_iter()
    .zip(shape.stroke_rgb)
    {
        stroke.set_static_value(ContentsParam::Shape(parameter), value)?;
    }
    let mut fill = ContentsNode::with_defaults(ContentsKind::Fill {
        even_odd: shape.even_odd,
    });
    fill.name = shape.fill_name.clone();
    fill.set_static_value(
        ContentsParam::Shape(ShapeParam::FillOpacity),
        shape.fill_opacity,
    )?;
    for (parameter, value) in [
        ShapeParam::FillRed,
        ShapeParam::FillGreen,
        ShapeParam::FillBlue,
    ]
    .into_iter()
    .zip(shape.fill_rgb)
    {
        fill.set_static_value(ContentsParam::Shape(parameter), value)?;
    }
    let mut group = ContentsNode::with_defaults(ContentsKind::Group(vec![rect, stroke, fill]));
    group.name = shape.group_name.clone();
    for (property, value) in &shape.group_properties {
        group.set_static_value(ContentsParam::Transform(*property), *value)?;
    }
    Ok(Content::ShapeContents(ShapeContents::from_nodes(vec![
        group,
    ])?))
}
fn verify_existing_media(input: &Input, base: &Project) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut ids = BTreeSet::new();
    let mut media = Vec::new();
    for item in &input.existing_media {
        if !ids.insert(item.native_asset_id)
            || base.asset_library().assets().get(&item.native_asset_id)
                != Some(&item.expected_asset)
        {
            return Err("Existing media metadata differs or is duplicated".into());
        }
        let Content::Audio { path, .. } = item.expected_asset.content() else {
            return Err("This reviewed subset preserves external audio assets only".into());
        };
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("Existing media must use a portable relative path".into());
        }
        media.push((path, reviewed_read(&item.file, 256 * 1024 * 1024)?));
    }
    Ok(media)
}
fn import_images(input: &Input, editor: &mut Editor) -> Result<BTreeMap<u64, (AssetId, Content)>> {
    let mut images = BTreeMap::new();
    for image in &input.image_assets {
        let bytes = reviewed_read(&image.file, IMAGE_LIMIT)?;
        if bytes.len() != image.size_bytes
            || bytes.len() < 33
            || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
            || &bytes[12..16] != b"IHDR"
            || u32::from_be_bytes(bytes[16..20].try_into()?) != image.width
            || u32::from_be_bytes(bytes[20..24].try_into()?) != image.height
        {
            return Err("PNG byte count, signature, or dimensions differ".into());
        }
        let content = Content::Image {
            png: Arc::from(base64(&bytes)),
        };
        let before: BTreeSet<_> = editor
            .project()
            .asset_library()
            .assets()
            .keys()
            .copied()
            .collect();
        editor.execute(Command::ImportAsset {
            content: content.clone(),
            width: f64::from(image.width),
            height: f64::from(image.height),
            name: image.name.clone(),
            folder: None,
            frame: None,
        })?;
        let added: Vec<_> = editor
            .project()
            .asset_library()
            .assets()
            .keys()
            .filter(|id| !before.contains(id))
            .copied()
            .collect();
        if added.len() != 1 {
            return Err("Image did not allocate exactly one new shared asset".into());
        }
        images.insert(image.source_item_id, (added[0], content));
    }
    Ok(images)
}
fn content_for(
    layer: &LayerInput,
    comps: &BTreeMap<u64, CompositionId>,
    images: &BTreeMap<u64, (AssetId, Content)>,
) -> Result<Content> {
    Ok(match &layer.content {
        LayerContent::Image { source_item_id } => images[source_item_id].1.clone(),
        LayerContent::Composition { source_id } => Content::Composition {
            composition: comps[source_id],
            start_frame: layer.start_frame,
        },
        LayerContent::RoundedRectangle { shape } => shape_content(shape)?,
    })
}
fn create_layer(
    editor: &mut Editor,
    layer: &LayerInput,
    comps: &BTreeMap<u64, CompositionId>,
    images: &BTreeMap<u64, (AssetId, Content)>,
) -> Result<LayerId> {
    if let LayerContent::Image { source_item_id } = &layer.content {
        editor.execute(Command::AddAssetLayer {
            asset: images[source_item_id].0,
            frame: 0,
        })?;
    } else {
        let mut content = content_for(layer, comps, images)?;
        if let Content::Composition { start_frame, .. } = &mut content {
            *start_frame = 0;
        }
        editor.execute(Command::AddContent {
            content,
            width: layer.width,
            height: layer.height,
            name: layer.name.clone(),
        })?;
    }
    let id = editor.selected().ok_or("New layer was not selected")?;
    let mut commands = vec![
        Command::RenameLayer {
            id,
            name: layer.name.clone(),
        },
        Command::SetLayerRange {
            id,
            start: u32::try_from(i64::from(layer.range[0]) - layer.start_frame)?,
            end: u32::try_from(i64::from(layer.range[1]) - layer.start_frame)?,
        },
        Command::SetLayerStart {
            id,
            frame: layer.start_frame,
        },
        Command::SetLayerLabel {
            id,
            index: layer.label,
        },
    ];
    if editor.selected_layer().ok_or("Missing layer")?.can_audio() {
        commands.push(Command::SetAudioEnabled {
            id,
            enabled: layer.audio_enabled,
        });
    }
    for (property, value) in layer
        .authored_properties
        .iter()
        .chain(&layer.materialization_defaults)
    {
        commands.push(Command::SetValue {
            id,
            property: *property,
            frame: 0,
            value: *value,
        });
    }
    if !layer.visible {
        commands.push(Command::ToggleVisible(id));
    }
    editor.execute(Command::Batch(commands))?;
    for fill in &layer.fill_effects {
        editor.execute(Command::Effect {
            id,
            edit: EffectEdit::Add(EffectKind::Fill),
        })?;
        let effect = editor
            .selected_layer()
            .and_then(|l| l.effect_stack().last())
            .ok_or("Missing new Fill")?
            .id();
        editor.execute(Command::Effect {
            id,
            edit: EffectEdit::Rename {
                effect,
                name: fill.name.clone(),
            },
        })?;
        editor.execute(Command::Effect {
            id,
            edit: EffectEdit::Bypass {
                effect,
                bypassed: !fill.enabled,
            },
        })?;
        for (parameter, value) in [
            EffectParam::Red,
            EffectParam::Green,
            EffectParam::Blue,
            EffectParam::Opacity,
        ]
        .into_iter()
        .zip([fill.rgb[0], fill.rgb[1], fill.rgb[2], fill.opacity])
        {
            editor.execute(Command::Effect {
                id,
                edit: EffectEdit::SetValue {
                    effect,
                    parameter,
                    frame: 0,
                    value,
                },
            })?;
        }
    }
    Ok(id)
}
fn without_sampling(comp: &Composition) -> Result<Value> {
    let mut value = serde_json::to_value(comp)?;
    value
        .as_object_mut()
        .ok_or("Invalid composition object")?
        .remove("preserve_nested_frame_rate");
    Ok(value)
}
fn verify_append(
    input: &Input,
    base: &Project,
    project: &Project,
    comps: &BTreeMap<u64, CompositionId>,
    layers: &BTreeMap<u64, BTreeMap<u64, LayerId>>,
    images: &BTreeMap<u64, (AssetId, Content)>,
) -> Result<Value> {
    if project.active_composition_id() != base.active_composition_id()
        || project.compositions().len() != base.compositions().len() + input.compositions.len()
        || project.asset_library().assets().len()
            != base.asset_library().assets().len() + images.len()
    {
        return Err("Unexpected composition/asset count or changed active composition".into());
    }
    let mut preservation = Vec::new();
    for old in &input.existing_compositions {
        let before = base
            .composition_by_id(old.native_id)
            .ok_or("Missing old composition")?;
        let after = project
            .composition_by_id(old.native_id)
            .ok_or("Missing preserved composition")?;
        if without_sampling(before)? != without_sampling(after)?
            || after.preserve_nested_frame_rate() != Some(old.preserve_nested_frame_rate)
        {
            return Err("Existing composition changed beyond its explicit sampling policy".into());
        }
        preservation.push(json!({"source_id":old.source_id,"native_id":old.native_id,
            "payload_except_sampling_equal":true,"preserve_nested_frame_rate":old.preserve_nested_frame_rate,
            "previous_preserve_nested_frame_rate":before.preserve_nested_frame_rate(),
            "preserved_payload_sha256":digest(&serde_json::to_vec(&without_sampling(before)?)?)}));
    }
    for (id, asset) in base.asset_library().assets() {
        if project.asset_library().assets().get(id) != Some(asset) {
            return Err("Existing asset changed".into());
        }
    }
    let before_library = serde_json::to_value(base.asset_library())?;
    let after_library = serde_json::to_value(project.asset_library())?;
    for key in ["folders", "composition_folders"] {
        if before_library[key] != after_library[key] {
            return Err("Existing project folders changed".into());
        }
    }
    let mut asset_receipts = Vec::new();
    for source in &input.image_assets {
        let (id, content) = &images[&source.source_item_id];
        let asset = project
            .asset_library()
            .assets()
            .get(id)
            .ok_or("Missing new asset")?;
        if asset.content() != content
            || asset.name() != source.name
            || asset.width() != f64::from(source.width)
            || asset.height() != f64::from(source.height)
            || asset.folder().is_some()
        {
            return Err("New image asset changed".into());
        }
        asset_receipts.push(json!({"source_item_id":source.source_item_id,"native_asset_id":id,
            "png_sha256":source.file.sha256,"bytes":source.size_bytes,"exact_source_bytes_embedded":true,
            "relink_evidence":source.relink_evidence}));
    }
    let mut comp_receipts = Vec::new();
    for source in &input.compositions {
        let id = comps[&source.source_id];
        let comp = project
            .composition_by_id(id)
            .ok_or("Missing appended composition")?;
        let ids = &layers[&source.source_id];
        if comp.name() != source.name
            || comp.width() != source.width
            || comp.height() != source.height
            || comp.fps() != source.fps
            || comp.duration() != source.duration
            || comp.display_start() != source.display_start
            || comp.background_color() != source.background
            || comp.work_area() != (source.work_area[0]..source.work_area[1])
            || comp.preserve_nested_frame_rate() != Some(source.preserve_nested_frame_rate)
            || comp.camera().is_some()
            || comp.layers().iter().map(Layer::id).collect::<Vec<_>>()
                != source
                    .layers
                    .iter()
                    .map(|l| ids[&l.source_id])
                    .collect::<Vec<_>>()
        {
            return Err("Appended composition settings or stack changed".into());
        }
        let mut layer_receipts = Vec::new();
        for declared in &source.layers {
            let layer = comp
                .layer(ids[&declared.source_id])
                .ok_or("Missing appended layer")?;
            let expected_asset = match &declared.content {
                LayerContent::Image { source_item_id } => Some(images[source_item_id].0),
                _ => None,
            };
            if layer.content() != &content_for(declared, comps, images)?
                || layer.name() != declared.name
                || layer.width() != declared.width
                || layer.height() != declared.height
                || layer.visible() != declared.visible
                || layer.audio_enabled() != declared.audio_enabled
                || layer.label_index() != declared.label
                || layer.start_frame() != declared.start_frame
                || [layer.in_frame(), layer.out_frame(source.duration)] != declared.range
                || layer.parent() != declared.parent_source_id.map(|p| ids[&p])
                || layer.track_matte()
                    != declared.track_matte_source_id.map(|p| TrackMatte {
                        source: ids[&p],
                        mode: MatteMode::Alpha,
                    })
                || layer.asset_id() != expected_asset
                || layer.is_three_d()
                || layer.solo()
                || layer.guide()
                || layer.blend_mode() != BlendMode::Normal
                || !layer.expressions().is_empty()
                || !layer.path_masks().is_empty()
                || layer.mask().is_some()
                || !layer.markers().is_empty()
                || layer.effect_stack().len() != declared.fill_effects.len()
                || layer.planar_position().is_some()
                || layer.opacity_key_count() != 0
                || serde_json::to_value(layer)?["transform_offset"]
                    != serde_json::to_value(Affine::default())?
            {
                return Err(
                    format!("Layer metadata differs for source {}", declared.source_id).into(),
                );
            }
            for (property, value) in declared
                .authored_properties
                .iter()
                .chain(&declared.materialization_defaults)
            {
                let track = layer.property(*property).ok_or("Missing reviewed scalar")?;
                if !track.keys().is_empty() || track.value_at(0) != *value {
                    return Err("Observed scalar or explicit materialization value changed".into());
                }
            }
            for (effect, expected) in layer.effect_stack().iter().zip(&declared.fill_effects) {
                if effect.kind() != EffectKind::Fill
                    || effect.name() != expected.name
                    || effect.bypassed() == expected.enabled
                {
                    return Err("Fill order, kind, name, or switch changed".into());
                }
                for (parameter, value) in [
                    EffectParam::Red,
                    EffectParam::Green,
                    EffectParam::Blue,
                    EffectParam::Opacity,
                ]
                .into_iter()
                .zip([
                    expected.rgb[0],
                    expected.rgb[1],
                    expected.rgb[2],
                    expected.opacity,
                ]) {
                    if effect.value_at(parameter, 0) != value
                        || effect
                            .parameter(parameter)
                            .is_none_or(|p| !p.keys().is_empty())
                    {
                        return Err("Fill color/opacity differs or acquired animation".into());
                    }
                }
            }
            if let (Some(asset_id), Content::Image { png: layer_png }) =
                (expected_asset, layer.content())
            {
                let Content::Image { png: asset_png } =
                    project.asset_library().assets()[&asset_id].content()
                else {
                    return Err("Wrong asset kind".into());
                };
                if !Arc::ptr_eq(layer_png, asset_png) {
                    return Err("Image layer lost shared asset byte identity".into());
                }
            }
            layer_receipts.push(json!({"source_id":declared.source_id,"native_id":layer.id(),
                "parent_native_id":layer.parent(),"track_matte":layer.track_matte(),"native_asset_id":layer.asset_id(),
                "audio_enabled":layer.audio_enabled(),"fill_count":layer.effect_stack().len(),
                "world_transform_at_zero":comp.world_transform(layer.id(),0),
                "authored_properties":declared.authored_properties,"materialization_defaults":declared.materialization_defaults}));
        }
        comp_receipts.push(json!({"source_id":source.source_id,"native_id":id,"layers":layer_receipts,
            "fps":source.fps,"duration":source.duration,"preserve_nested_frame_rate":source.preserve_nested_frame_rate}));
    }
    Ok(
        json!({"preserved_compositions":preservation,"new_compositions":comp_receipts,"new_image_assets":asset_receipts,
        "existing_assets_equal":true,"existing_programs_equal":true,"source_programs_executed":false,
        "image_byte_sharing_verified":true,"composition_count":project.compositions().len(),
        "layer_count":project.compositions().iter().map(|(_,c)|c.layers().len()).sum::<usize>()}),
    )
}
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let input_path = PathBuf::from(
        args.next()
            .ok_or("Expected INPUT.json INPUT_SHA256 NEW.lep")?,
    );
    let input_sha256 = args
        .next()
        .ok_or("Missing input SHA-256")?
        .into_string()
        .map_err(|_| "Digest must be ASCII")?;
    let destination = PathBuf::from(args.next().ok_or("Missing new output path")?);
    if args.next().is_some()
        || destination
            .extension()
            .is_none_or(|s| !s.eq_ignore_ascii_case("lep"))
    {
        return Err("Expected INPUT.json INPUT_SHA256 NEW.lep".into());
    }
    let evidence_path = destination.with_extension("provenance.json");
    let native_path = destination.with_extension("native.json");
    if [&destination, &evidence_path, &native_path]
        .iter()
        .any(|p| p.exists())
    {
        return Err("Refusing to replace any previous output".into());
    }
    let input_bytes = bounded_read(&input_path, INPUT_LIMIT)?;
    verify_digest(&input_bytes, &input_sha256)?;
    let input: Input = serde_json::from_slice(&input_bytes)?;
    let base_bytes = reviewed_read(&input.base_project, project_file::MAX_FILE_BYTES)?;
    let decoded = project_file::decode(&base_bytes)?;
    let base = decoded.project;
    validate_input(&input, &base)?;
    for evidence in &input.evidence_files {
        reviewed_read(evidence, 64 * 1024 * 1024)?;
    }
    let media = verify_existing_media(&input, &base)?;
    let old_view: Value = serde_json::from_slice(decoded.view.ok_or("Missing frozen view")?)?;
    let mut view = old_view.clone();
    if !matches!(view["version"].as_u64(), Some(1 | 2))
        || !view["workspace"].is_object()
        || !view["compositions"].is_object()
    {
        return Err("Unsupported frozen view contract".into());
    }
    let mut editor = Editor::default();
    editor.replace_project(base.clone())?;
    editor.clear_history();
    let mut comp_ids = BTreeMap::new();
    for old in &input.existing_compositions {
        comp_ids.insert(old.source_id, old.native_id);
        editor.execute(Command::SetPreserveNestedFrameRate {
            composition: old.native_id,
            preserve: old.preserve_nested_frame_rate,
        })?;
    }
    let images = import_images(&input, &mut editor)?;
    for comp in &input.compositions {
        editor.execute(Command::NewComposition)?;
        let id = editor.project().active_composition_id();
        comp_ids.insert(comp.source_id, id);
        editor.execute(Command::ConfigureCompositionRate {
            name: comp.name.clone(),
            width: comp.width,
            height: comp.height,
            fps: comp.fps,
            duration: comp.duration,
            display_start: comp.display_start,
        })?;
        editor.execute(Command::SetCompositionBackground(comp.background))?;
        editor.execute(Command::SetWorkArea {
            start: comp.work_area[0],
            end: comp.work_area[1],
        })?;
        editor.execute(Command::SetPreserveNestedFrameRate {
            composition: id,
            preserve: comp.preserve_nested_frame_rate,
        })?;
        if view["compositions"]
            .as_object_mut()
            .unwrap()
            .insert(id.to_string(), json!({"frame":comp.initial_frame}))
            .is_some()
        {
            return Err("New composition would overwrite an existing view".into());
        }
    }
    let mut layer_ids = BTreeMap::new();
    for comp in &input.compositions {
        editor.activate_composition(comp_ids[&comp.source_id])?;
        let mut ids = BTreeMap::new();
        for layer in comp.layers.iter().rev() {
            ids.insert(
                layer.source_id,
                create_layer(&mut editor, layer, &comp_ids, &images)?,
            );
        }
        for layer in &comp.layers {
            editor.execute(Command::SetPlanarParent {
                id: ids[&layer.source_id],
                parent: layer.parent_source_id.map(|p| ids[&p]),
            })?;
            if let Some(source) = layer.track_matte_source_id {
                editor.execute(Command::SetTrackMatte {
                    id: ids[&layer.source_id],
                    matte: Some(TrackMatte {
                        source: ids[&source],
                        mode: MatteMode::Alpha,
                    }),
                })?;
            }
        }
        layer_ids.insert(comp.source_id, ids);
    }
    editor.activate_composition(base.active_composition_id())?;
    editor.clear_history();
    let project = editor.project();
    project.validate_planar_animation()?;
    project.validate_opacity_animation()?;
    verify_append(&input, &base, project, &comp_ids, &layer_ids, &images)?;
    let mut retained_view = view.clone();
    for comp in &input.compositions {
        retained_view["compositions"]
            .as_object_mut()
            .unwrap()
            .remove(&comp_ids[&comp.source_id].to_string());
    }
    if retained_view != old_view {
        return Err("Existing views changed".into());
    }
    let view_bytes = serde_json::to_vec(&view)?;
    let bytes = project_file::encode(project, Some(&view_bytes))?;
    let reopened = project_file::decode(&bytes)?;
    if reopened.project != *project
        || project_file::encode(&reopened.project, reopened.view)? != bytes
    {
        return Err("Native deterministic roundtrip failed".into());
    }
    let checks = verify_append(
        &input,
        &base,
        &reopened.project,
        &comp_ids,
        &layer_ids,
        &images,
    )?;
    let receipt = json!({"contract_version":input.contract_version,"input_path":input_path,"input_sha256":input_sha256,
        "base_project_sha256":input.base_project.sha256,"native_project_sha256":digest(&bytes),"native_project_bytes":bytes.len(),
        "native_schema":serde_json::to_value(project)?["version"],"source_to_native_compositions":comp_ids,
        "existing_views_equal":true,"native_roundtrip_bytes_equal":true,"checks":checks,
        "existing_media":input.existing_media.iter().map(|m|json!({"source_item_id":m.source_item_id,"native_asset_id":m.native_asset_id,"sha256":m.file.sha256})).collect::<Vec<_>>(),
        "source_evidence":input.compositions.iter().map(|c|json!({"source_id":c.source_id,"composition":c.source_evidence,
            "layers":c.layers.iter().map(|l|json!({"source_id":l.source_id,"evidence":l.source_evidence})).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "provenance":input.provenance});
    verify_digest(&bounded_read(&input_path, INPUT_LIMIT)?, &input_sha256)?;
    reviewed_read(&input.base_project, project_file::MAX_FILE_BYTES)?;
    let output_dir = destination.parent().ok_or("Missing output directory")?;
    for (relative, media_bytes) in &media {
        let path = output_dir.join(relative);
        if path.exists() {
            verify_digest(
                &bounded_read(&path, 256 * 1024 * 1024)?,
                &digest(media_bytes),
            )?;
        } else {
            fs::create_dir_all(path.parent().ok_or("Missing media directory")?)?;
            write_new(&path, media_bytes)?;
        }
    }
    write_new(&evidence_path, &serde_json::to_vec_pretty(&receipt)?)?;
    write_new(&native_path, project.to_json()?.as_bytes())?;
    write_new(&destination, &bytes)?;
    let persisted = bounded_read(&destination, project_file::MAX_FILE_BYTES)?;
    verify_digest(&persisted, &digest(&bytes))?;
    let persisted = project_file::decode(&persisted)?;
    verify_append(
        &input,
        &base,
        &persisted.project,
        &comp_ids,
        &layer_ids,
        &images,
    )?;
    println!(
        "Appended {} reviewed compositions; preservation, shared images and deterministic persisted readback passed ({} bytes).",
        input.compositions.len(),
        bytes.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::base64;
    #[test]
    fn canonical_binary_encoding_preserves_padding_boundaries() {
        for (bytes, expected) in [
            (b"".as_slice(), ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foobar", "Zm9vYmFy"),
            (&[0, 255, 128, 0], "AP+AAA=="),
        ] {
            assert_eq!(base64(bytes), expected);
        }
    }
}
