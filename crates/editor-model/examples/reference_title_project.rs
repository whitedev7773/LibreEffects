//! Append a reviewed, private text/shape composition to a frozen native project.
//! All text, IDs, media, fonts, and evidence arrive through the runtime contract.
//! This is not a source-project parser. No expression program is executed.
//! Usage: reference_title_project INPUT.json INPUT_SHA256 NEW.lep

use libre_effects_core::*;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    contract_version: u32,
    contract_kind: String,
    base_project: BaseProject,
    composition: CompositionInput,
    reuse_assets: Vec<ReusedAsset>,
    layers: Vec<LayerInput>,
    provenance: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BaseProject {
    path: PathBuf,
    sha256: String,
    schema: u32,
    preserve_composition_id: CompositionId,
    preserve_asset_id: AssetId,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompositionInput {
    source_id: u64,
    name: String,
    width: u32,
    height: u32,
    fps: u32,
    duration: Frame,
    display_start: Frame,
    work_area: [Frame; 2],
    background: u32,
    initial_frame: Frame,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReusedAsset {
    source_item_id: u64,
    native_asset_id: AssetId,
    expected_asset: MediaAsset,
    source_duration: Value,
    media_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerInput {
    source_id: u64,
    source_item_id: u64,
    stack_index_1based: usize,
    parent_source_id: Option<u64>,
    name: String,
    content: Option<Content>,
    reuse_asset_id: Option<AssetId>,
    width: f64,
    height: f64,
    visible: bool,
    audio_enabled: bool,
    label: u8,
    start_frame: i64,
    range: [Frame; 2],
    three_d: bool,
    /// Only actually observed source scalars belong in this map.
    authored_properties: BTreeMap<Property, f64>,
    /// Disclosed native conventions, never evidence of source-authored values.
    materialization_defaults: BTreeMap<Property, f64>,
    absent_source_properties: Value,
    text_style: Option<TextStyle>,
    rich_text: Option<RichInput>,
    opacity_keys: Vec<OpacityKeyInput>,
    position_2d: Option<SpatialPosition2>,
    #[serde(default)]
    path_masks: Vec<PathMask>,
    #[serde(default)]
    expressions: Vec<NumericExpression>,
    #[serde(default)]
    markers: Vec<MarkerInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerInput {
    frame: Frame,
    duration: Frame,
    name: String,
    color: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RichInput {
    default_style: TextCharacterStyle,
    /// The reviewed source ranges count UTF-16 code units.
    runs: Vec<TextStyleRun>,
    point_origin: bool,
    expected_baselines: Vec<Option<f64>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpacityKeyInput {
    frame: Frame,
    value: f64,
    timing: OpacityKeyTiming,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bounded_read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(u64::try_from(maximum)? + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(format!("Input exceeds its byte budget: {}", path.display()).into());
    }
    Ok(bytes)
}
fn verify_digest(bytes: &[u8], expected: &str) -> Result<()> {
    if expected.len() != 64 || digest(bytes) != expected {
        return Err("SHA-256 does not match the reviewed runtime contract".into());
    }
    Ok(())
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
fn rich_text(layer: &LayerInput) -> Result<Option<RichText>> {
    let Some(rich) = &layer.rich_text else {
        return Ok(None);
    };
    let Some(Content::Text { text, .. }) = &layer.content else {
        return Err("Rich text requires text content".into());
    };
    let mut native =
        RichText::from_utf16_runs(text, rich.default_style.clone(), rich.runs.clone())?;
    native.point_origin = rich.point_origin;
    let metrics = native.line_metrics(
        text,
        layer.text_style.as_ref().ok_or("Missing text settings")?,
    )?;
    if metrics.len() != rich.expected_baselines.len() {
        return Err("Hard-line count differs from the reviewed source cache".into());
    }
    for (metric, expected) in metrics.iter().zip(&rich.expected_baselines) {
        if expected.is_some_and(|value| (metric.baseline - value).abs() > 0.0001) {
            return Err("Native baseline differs from an observed source cache value".into());
        }
    }
    Ok(Some(native))
}
fn expression_inventory(project: &Project) -> Value {
    let mut sources = BTreeSet::new();
    let mut bindings = Vec::new();
    for (composition, comp) in project.compositions() {
        for layer in comp.layers() {
            for expression in layer.expressions() {
                let sha256 = digest(expression.source.as_bytes());
                sources.insert(sha256.clone());
                bindings.push(json!({
                    "composition_id":composition,"layer_id":layer.id(),
                    "target":expression.target,"enabled":expression.enabled,
                    "source_sha256":sha256,"source_bytes":expression.source.len()
                }));
            }
        }
    }
    json!({"unique_source_count":sources.len(),"source_sha256":sources,
        "binding_count":bindings.len(),"bindings":bindings})
}
fn validate_input(input: &Input) -> Result<()> {
    if input.contract_version != 1
        || input.contract_kind != "reviewed_composition_append"
        || input.layers.is_empty()
        || input.layers.len() > 1000
        || input.composition.initial_frame >= input.composition.duration
    {
        return Err("Unsupported, empty, or out-of-range append contract".into());
    }
    let mut ids = BTreeSet::new();
    for (index, layer) in input.layers.iter().enumerate() {
        if !ids.insert(layer.source_id)
            || layer.stack_index_1based != index + 1
            || layer.three_d
            || layer.range[0] >= layer.range[1]
            || layer.range[1] > input.composition.duration
        {
            return Err("Invalid source ID, stack, 2D mode, or trim".into());
        }
        for property in Property::ALL {
            let authored = layer.authored_properties.contains_key(&property);
            let defaulted = layer.materialization_defaults.contains_key(&property);
            let joined = layer.position_2d.is_some()
                && matches!(property, Property::PositionX | Property::PositionY);
            if (authored && defaulted)
                || (joined && (authored || defaulted))
                || (!joined && !(authored || defaulted))
            {
                return Err(
                    "Each scalar needs one explicit source or native-default policy".into(),
                );
            }
        }
        let mut frames = BTreeSet::new();
        for key in &layer.opacity_keys {
            if !frames.insert(key.frame) || key.frame >= input.composition.duration {
                return Err("Duplicate or out-of-range Opacity key".into());
            }
        }
        if !layer.opacity_keys.is_empty()
            && (layer.authored_properties.contains_key(&Property::Opacity)
                || !layer
                    .materialization_defaults
                    .contains_key(&Property::Opacity))
        {
            return Err("This contract requires an explicitly unused native Opacity base".into());
        }
        rich_text(layer)?;
    }
    if input.layers.iter().any(|layer| {
        layer
            .parent_source_id
            .is_some_and(|parent| !ids.contains(&parent))
    }) {
        return Err("Parent ID is outside the appended composition".into());
    }
    Ok(())
}
fn verify_resources(input: &Input, base: &Project, destination: &Path) -> Result<Value> {
    let base_dir = input
        .base_project
        .path
        .parent()
        .ok_or("Missing base directory")?;
    let output_dir = destination.parent().ok_or("Missing output directory")?;
    let mut assets = Vec::new();
    let mut ids = BTreeSet::new();
    for asset in &input.reuse_assets {
        if !ids.insert(asset.native_asset_id)
            || base.asset_library().assets().get(&asset.native_asset_id)
                != Some(&asset.expected_asset)
        {
            return Err("Reused asset is duplicated or differs from reviewed metadata".into());
        }
        let Content::Audio { path, .. } = asset.expected_asset.content() else {
            return Err("This title contract reuses audio assets only".into());
        };
        let source = fs::canonicalize(base_dir.join(path))?;
        let output = fs::canonicalize(output_dir.join(path))?;
        if source != output {
            return Err(
                "Output must reference the same existing media file without copying".into(),
            );
        }
        let bytes = bounded_read(&source, 256 * 1024 * 1024)?;
        verify_digest(&bytes, &asset.media_sha256)?;
        assets.push(json!({"source_item_id":asset.source_item_id,
            "native_asset_id":asset.native_asset_id,"media_sha256":asset.media_sha256,
            "source_duration":asset.source_duration,"canonical_path":source,
            "metadata_equal":true,"same_existing_file":true,"redecoded":false}));
    }
    let fonts = input
        .provenance
        .get("used_fonts")
        .and_then(Value::as_object)
        .ok_or("Missing reviewed font inventory")?;
    let mut verified_fonts = Vec::new();
    for (face, font) in fonts {
        let path = Path::new(font["path"].as_str().ok_or("Missing font path")?);
        let sha256 = font["sha256"].as_str().ok_or("Missing font digest")?;
        verify_digest(&bounded_read(path, 64 * 1024 * 1024)?, sha256)?;
        verified_fonts.push(json!({"face":face,"path":path,"sha256":sha256,
            "family":font["family"],"weight":font["weight"]}));
    }
    for layer in &input.layers {
        if let Some(rich) = &layer.rich_text {
            for style in
                std::iter::once(&rich.default_style).chain(rich.runs.iter().map(|r| &r.style))
            {
                let font = fonts
                    .get(&style.font_face)
                    .ok_or("Text uses an unverified font face")?;
                if font["family"].as_str() != Some(&style.font_family)
                    || font["weight"].as_u64() != Some(u64::from(style.weight))
                {
                    return Err("Font family or weight differs from reviewed exact face".into());
                }
            }
        }
        match layer.reuse_asset_id {
            Some(id) if ids.contains(&id) => {
                if input
                    .reuse_assets
                    .iter()
                    .find(|a| a.native_asset_id == id)
                    .is_none_or(|a| {
                        a.source_item_id != layer.source_item_id
                            || layer
                                .content
                                .as_ref()
                                .is_some_and(|content| a.expected_asset.content() != content)
                    })
                {
                    return Err("Layer asset binding differs from reviewed source".into());
                }
            }
            None if matches!(
                layer.content,
                Some(Content::Text { .. } | Content::ShapeContents(_))
            ) => {}
            _ => return Err("Every media layer must reuse a reviewed existing asset".into()),
        }
    }
    Ok(json!({"assets":assets,"fonts":verified_fonts}))
}
fn create_layer(editor: &mut Editor, layer: &LayerInput) -> Result<LayerId> {
    if let Some(asset) = layer.reuse_asset_id {
        editor.execute(Command::AddAssetLayer { asset, frame: 0 })?;
    } else {
        editor.execute(Command::AddContent {
            content: layer
                .content
                .clone()
                .ok_or("Missing non-asset layer content")?,
            width: layer.width,
            height: layer.height,
            name: layer.name.clone(),
        })?;
    }
    let id = editor.selected().ok_or("New layer was not selected")?;
    // Start commands shift trims and keys. Set the pre-shift trim first and add
    // composition-space keys only after the signed origin has been established.
    let start = u32::try_from(i64::from(layer.range[0]) - layer.start_frame)?;
    let end = u32::try_from(i64::from(layer.range[1]) - layer.start_frame)?;
    let mut commands = vec![
        Command::RenameLayer {
            id,
            name: layer.name.clone(),
        },
        Command::SetLayerRange { id, start, end },
        Command::SetLayerStart {
            id,
            frame: layer.start_frame,
        },
        Command::SetLayerLabel {
            id,
            index: layer.label,
        },
    ];
    if layer.reuse_asset_id.is_some()
        || layer
            .content
            .as_ref()
            .is_some_and(|content| content.audio().is_some())
    {
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
    if let Some(style) = &layer.text_style {
        commands.push(Command::SetTextStyle {
            id,
            style: style.clone(),
        });
    }
    if let Some(rich_text) = rich_text(layer)? {
        commands.push(Command::SetColor {
            id,
            color: rich_text.default_style.fill_color,
        });
        commands.push(Command::SetRichText {
            id,
            rich_text: Some(rich_text),
        });
    }
    if let Some(position) = &layer.position_2d {
        commands.push(Command::SetPlanarPosition {
            id,
            position: position.clone(),
        });
    }
    for key in &layer.opacity_keys {
        let frame = key.frame;
        let timing = &key.timing;
        for edit in [
            OpacityEdit::Key {
                frame,
                value: key.value,
            },
            OpacityEdit::Interpolation {
                frame,
                incoming: timing.in_interpolation,
                outgoing: timing.out_interpolation,
            },
            OpacityEdit::TemporalEase {
                frame,
                incoming: timing.in_ease,
                outgoing: timing.out_ease,
            },
            OpacityEdit::TemporalContinuous {
                frame,
                value: timing.temporal_continuous,
            },
            OpacityEdit::TemporalAutoBezier {
                frame,
                value: timing.temporal_auto_bezier,
            },
        ] {
            commands.push(Command::SetOpacityTiming { id, edit });
        }
    }
    if !layer.path_masks.is_empty() {
        if layer
            .path_masks
            .iter()
            .enumerate()
            .any(|(index, mask)| mask.id != index as u64 + 1)
        {
            return Err("Reviewed new mask identities must follow stack order from one".into());
        }
        let mut fresh = layer.path_masks.clone();
        for mask in &mut fresh {
            mask.id = 0;
        }
        commands.push(Command::SetPathMasks { id, masks: fresh });
    }
    for (index, marker) in layer.markers.iter().enumerate() {
        commands.push(Command::Marker {
            target: MarkerTarget::Layer(id),
            edit: MarkerEdit::Add {
                frame: marker.frame,
            },
        });
        commands.push(Command::Marker {
            target: MarkerTarget::Layer(id),
            edit: MarkerEdit::Update {
                id: index as u64 + 1,
                frame: marker.frame,
                duration: marker.duration,
                name: marker.name.clone(),
                color: marker.color,
            },
        });
    }
    for program in &layer.expressions {
        commands.push(Command::SetExpression {
            id,
            target: program.target,
            source: program.source.clone(),
            enabled: program.enabled,
        });
        if !program.local_bindings.is_empty() {
            commands.push(Command::SetExpressionLocalBindings {
                id,
                target: program.target,
                bindings: program.local_bindings.clone(),
            });
        }
    }
    editor.execute(Command::Batch(commands))?;
    Ok(id)
}
fn verify_append(
    input: &Input,
    base: &Project,
    project: &Project,
    comp_id: CompositionId,
    ids: &BTreeMap<u64, LayerId>,
) -> Result<Value> {
    if project.active_composition_id() != base.active_composition_id()
        || project.compositions().len() != base.compositions().len() + 1
        || project.asset_library() != base.asset_library()
    {
        return Err("Existing active composition or asset library was changed".into());
    }
    for (id, comp) in base.compositions() {
        if project.composition_by_id(id) != Some(comp) {
            return Err("Existing composition payload or source IDs were changed".into());
        }
    }
    // Every pre-existing composition (including its exact expression metadata)
    // was compared above. The appended composition may add new programs.
    let comp = project
        .composition_by_id(comp_id)
        .ok_or("Missing appended composition")?;
    let c = &input.composition;
    if comp.name() != c.name
        || comp.width() != c.width
        || comp.height() != c.height
        || comp.fps() != c.fps.into()
        || comp.duration() != c.duration
        || comp.display_start() != c.display_start
        || comp.background_color() != c.background
        || comp.work_area() != (c.work_area[0]..c.work_area[1])
        || comp.camera().is_some()
        || comp.layers().iter().map(Layer::id).collect::<Vec<_>>()
            != input
                .layers
                .iter()
                .map(|l| ids[&l.source_id])
                .collect::<Vec<_>>()
    {
        return Err("Appended composition settings or stack differ from source".into());
    }
    let mut evidence = Vec::new();
    let mut samples = Vec::new();
    for source in &input.layers {
        let id = ids[&source.source_id];
        let layer = comp.layer(id).ok_or("Missing appended layer")?;
        let native = serde_json::to_value(layer)?;
        let expected_content = match source.reuse_asset_id {
            Some(asset) => input
                .reuse_assets
                .iter()
                .find(|a| a.native_asset_id == asset)
                .ok_or("Missing expected asset")?
                .expected_asset
                .content(),
            None => source.content.as_ref().ok_or("Missing expected content")?,
        };
        if layer.name() != source.name
            || layer.content() != expected_content
            || layer.width() != source.width
            || layer.height() != source.height
            || layer.visible() != source.visible
            || layer.audio_enabled() != source.audio_enabled
            || layer.label_index() != source.label
            || layer.start_frame() != source.start_frame
            || [layer.in_frame(), layer.out_frame(c.duration)] != source.range
            || layer.parent() != source.parent_source_id.map(|p| ids[&p])
            || layer.asset_id() != source.reuse_asset_id
            || layer.is_three_d()
            || layer.expressions() != source.expressions
            || !layer.effect_stack().is_empty()
            || layer.path_masks() != source.path_masks
            || layer.markers().len() != source.markers.len()
            || layer
                .markers()
                .iter()
                .zip(&source.markers)
                .any(|(actual, expected)| {
                    actual.frame() != expected.frame
                        || actual.duration() != expected.duration
                        || actual.name() != expected.name
                        || actual.color() != expected.color
                })
            || layer.mask().is_some()
            || native["transform_offset"] != serde_json::to_value(Affine::default())?
            || layer.planar_position() != source.position_2d.as_ref()
        {
            return Err(format!("Layer metadata mismatch for source {}", source.source_id).into());
        }
        for (property, value) in source
            .authored_properties
            .iter()
            .chain(&source.materialization_defaults)
        {
            let name = serde_json::to_value(property)?
                .as_str()
                .ok_or("Invalid property name")?
                .to_owned();
            if native["properties"][&name]["value"].as_f64() != Some(*value) {
                return Err("Source scalar or explicit native placeholder changed".into());
            }
        }
        if source.position_2d.is_some()
            && (layer.property(Property::PositionX).is_some()
                || layer.property(Property::PositionY).is_some())
        {
            return Err("Joined Position retained an unauthorized scalar base".into());
        }
        if source
            .text_style
            .as_ref()
            .is_some_and(|style| layer.text_style() != *style)
            || layer.rich_text() != rich_text(source)?.as_ref()
            || layer.opacity_key_count() != source.opacity_keys.len()
        {
            return Err("Text style, UTF-16 mapping, or Opacity key count changed".into());
        }
        let mut sample_frames = BTreeSet::from([0, c.initial_frame, c.duration - 1]);
        let mut previous = None;
        for key in &source.opacity_keys {
            if layer.opacity_key_value(key.frame) != Some(key.value)
                || layer
                    .opacity_timing()
                    .and_then(|t| t.keys().get(&key.frame))
                    != Some(&key.timing)
                || layer.opacity_at(key.frame, comp.fps().seconds(1))? != key.value
            {
                return Err("Opacity value, exact per-side metadata, or key sample changed".into());
            }
            if let Some(frame) = previous {
                sample_frames.insert(frame + (key.frame - frame) / 2);
            }
            previous = Some(key.frame);
            for frame in [
                key.frame.saturating_sub(1),
                key.frame,
                key.frame.saturating_add(1),
            ] {
                if frame < c.duration {
                    sample_frames.insert(frame);
                }
            }
        }
        if let Some(position) = &source.position_2d {
            let mut previous = None;
            for (&frame, key) in &position.keys {
                if layer.position2_at(frame, comp.fps().seconds(1))? != key.value {
                    return Err("Planar key sample differs from its authored XY value".into());
                }
                if let Some(start) = previous {
                    sample_frames.insert(start + (frame - start) / 2);
                }
                previous = Some(frame);
                for f in [frame.saturating_sub(1), frame, frame.saturating_add(1)] {
                    if f < c.duration {
                        sample_frames.insert(f);
                    }
                }
            }
        }
        for frame in sample_frames {
            let position = layer.position2_at(frame, comp.fps().seconds(1))?;
            let opacity = layer.opacity_at(frame, comp.fps().seconds(1))?;
            let world = comp
                .world_transform(id, frame)
                .ok_or("Invalid parented geometry sample")?;
            if !opacity.is_finite() || position.iter().any(|value| !value.is_finite()) {
                return Err("Nonfinite diagnostic sample".into());
            }
            samples.push(json!({"source_layer_id":source.source_id,"frame":frame,
                "local_position":position,"local_opacity":opacity,"world_transform":world,
                "active":comp.layer_active(layer,frame,false)}));
        }
        evidence.push(
            json!({"source_layer_id":source.source_id,"native_layer_id":id,
            "source_item_id":source.source_item_id,"parent_source_id":source.parent_source_id,
            "native_parent_id":layer.parent(),"stack_index_1based":source.stack_index_1based,
            "start_frame":source.start_frame,"range":source.range,
            "authored_properties":source.authored_properties,
            "materialization_defaults":source.materialization_defaults,
            "absent_source_properties":source.absent_source_properties,
            "keyed_opacity_base":if source.opacity_keys.is_empty() { Value::Null } else {
                json!({"source_authored_base":null,"source_base_absent":true,
                    "native_unused_placeholder":source.materialization_defaults[&Property::Opacity],
                    "keys_are_authoritative":true})
            },
            "planar_base":source.position_2d.as_ref().map(|p|p.value),
            "source_cached_baselines":source.rich_text.as_ref().map(|r|&r.expected_baselines),
            "opacity_key_count":source.opacity_keys.len(),
            "planar_key_count":source.position_2d.as_ref().map_or(0,|p|p.keys.len())}),
        );
    }
    Ok(json!({"layers":evidence,"samples":samples,
        "existing_compositions_equal":true,"existing_asset_library_equal":true,
        "existing_expression_inventory_equal":true,"all_layers_2d":true,
        "camera_absent":true,"source_programs_executed":false,
        "parent_count":input.layers.iter().filter(|l|l.parent_source_id.is_some()).count(),
        "opacity_key_count":input.layers.iter().map(|l|l.opacity_keys.len()).sum::<usize>(),
        "planar_key_count":input.layers.iter().filter_map(|l|l.position_2d.as_ref()).map(|p|p.keys.len()).sum::<usize>()}))
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let input_path = PathBuf::from(
        args.next()
            .ok_or("Expected INPUT.json INPUT_SHA256 NEW.lep")?,
    );
    let input_sha256 = args
        .next()
        .ok_or("Missing reviewed input SHA-256")?
        .into_string()
        .map_err(|_| "Input SHA-256 must be ASCII")?;
    let destination = PathBuf::from(args.next().ok_or("Missing new output path")?);
    if args.next().is_some()
        || destination
            .extension()
            .is_none_or(|s| !s.eq_ignore_ascii_case("lep"))
    {
        return Err("Expected INPUT.json INPUT_SHA256 and a new .lep output path".into());
    }
    let evidence_path = destination.with_extension("provenance.json");
    let native_path = destination.with_extension("native.json");
    if [&destination, &evidence_path, &native_path]
        .iter()
        .any(|p| p.exists())
    {
        return Err("Refusing to replace any previous project or evidence output".into());
    }
    let input_bytes = bounded_read(&input_path, 16 * 1024 * 1024)?;
    verify_digest(&input_bytes, &input_sha256)?;
    let input: Input = serde_json::from_slice(&input_bytes)?;
    validate_input(&input)?;
    let base_bytes = bounded_read(&input.base_project.path, project_file::MAX_FILE_BYTES)?;
    verify_digest(&base_bytes, &input.base_project.sha256)?;
    let decoded = project_file::decode(&base_bytes)?;
    let base = decoded.project;
    if serde_json::to_value(&base)?["version"].as_u64()
        != Some(u64::from(input.base_project.schema))
        || base
            .composition_by_id(input.base_project.preserve_composition_id)
            .is_none()
        || !base
            .asset_library()
            .assets()
            .contains_key(&input.base_project.preserve_asset_id)
    {
        return Err("Frozen project schema or protected IDs differ from the contract".into());
    }
    let resources = verify_resources(&input, &base, &destination)?;
    let mut view: Value =
        serde_json::from_slice(decoded.view.ok_or("Frozen project has no view contract")?)?;
    if !matches!(view["version"].as_u64(), Some(1 | 2)) || !view["workspace"].is_object() {
        return Err("Unsupported frozen project view contract".into());
    }
    let old_view = view.clone();
    let mut editor = Editor::default();
    editor.replace_project(base.clone())?;
    editor.clear_history();
    editor.execute(Command::NewComposition)?;
    let comp_id = editor.project().active_composition_id();
    let c = &input.composition;
    editor.execute(Command::ConfigureCompositionRate {
        name: c.name.clone(),
        width: c.width,
        height: c.height,
        fps: c.fps.into(),
        duration: c.duration,
        display_start: c.display_start,
    })?;
    editor.execute(Command::SetCompositionBackground(c.background))?;
    editor.execute(Command::SetWorkArea {
        start: c.work_area[0],
        end: c.work_area[1],
    })?;
    let mut ids = BTreeMap::new();
    // AddContent and AddAssetLayer insert at the top of the layer stack.
    for layer in input.layers.iter().rev() {
        ids.insert(layer.source_id, create_layer(&mut editor, layer)?);
    }
    for layer in &input.layers {
        editor.execute(Command::SetPlanarParent {
            id: ids[&layer.source_id],
            parent: layer.parent_source_id.map(|p| ids[&p]),
        })?;
    }
    editor.activate_composition(base.active_composition_id())?;
    editor.clear_history();
    let project = editor.project();
    project.validate_planar_animation()?;
    project.validate_opacity_animation()?;
    let checks = verify_append(&input, &base, project, comp_id, &ids)?;
    let views = view["compositions"]
        .as_object_mut()
        .ok_or("Missing composition view map")?;
    if views
        .insert(comp_id.to_string(), json!({"frame":c.initial_frame}))
        .is_some()
    {
        return Err("New composition would overwrite an existing view".into());
    }
    let mut retained_view = view.clone();
    retained_view["compositions"]
        .as_object_mut()
        .unwrap()
        .remove(&comp_id.to_string());
    if retained_view != old_view {
        return Err("Existing view payload changed".into());
    }
    let view_bytes = serde_json::to_vec(&view)?;
    let bytes = project_file::encode(project, Some(&view_bytes))?;
    let reopened = project_file::decode(&bytes)?;
    if reopened.project != *project
        || project_file::encode(&reopened.project, reopened.view)? != bytes
    {
        return Err("Native deterministic roundtrip failed".into());
    }
    verify_append(&input, &base, &reopened.project, comp_id, &ids)?;
    let receipt = json!({"contract_version":input.contract_version,"input_path":input_path,
        "input_sha256":input_sha256,"base_project_sha256":input.base_project.sha256,
        "native_project_sha256":digest(&bytes),"native_project_bytes":bytes.len(),
        "native_schema":serde_json::to_value(project)?["version"],
        "source_composition_id":c.source_id,"native_composition_id":comp_id,
        "active_composition_id":project.active_composition_id(),"existing_views_equal":true,
        "new_composition_view_frame":c.initial_frame,"native_roundtrip_bytes_equal":true,
        "original_expressions":expression_inventory(&base),
        "result_expressions":expression_inventory(project),"resources":resources,"checks":checks,
        "provenance":input.provenance});
    // Recheck the input and frozen file before publishing any new output.
    verify_digest(&bounded_read(&input_path, 16 * 1024 * 1024)?, &input_sha256)?;
    verify_digest(
        &bounded_read(&input.base_project.path, project_file::MAX_FILE_BYTES)?,
        &input.base_project.sha256,
    )?;
    write_new(&evidence_path, &serde_json::to_vec_pretty(&receipt)?)?;
    write_new(&native_path, project.to_json()?.as_bytes())?;
    write_new(&destination, &bytes)?;
    println!(
        "Appended {} reviewed layers; preserved all original compositions, assets, programs, and active view; deterministic native roundtrip passed ({} bytes).",
        input.layers.len(),
        bytes.len()
    );
    Ok(())
}
