//! Construct a private, reviewed reference from a data contract supplied at runtime.
//! No source project text, programs, media, fonts, or identifiers are embedded here.
//! This is not an AEP parser/import UI. The preparer owns field interpretation and
//! must document every approximation alongside its original evidence.
#[allow(dead_code)]
#[path = "../../../apps/desktop/src/automation_process.rs"]
mod automation_process;

use libre_effects_core::{expression_runtime as ae, *};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    contract_version: u32,
    composition: CompositionInput,
    sources: Vec<Source>,
    layers: Vec<LayerInput>,
    provenance: Value,
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
struct Source {
    /// Exact bytes of a separately inspected private file. No newline conversion.
    inspected_path: PathBuf,
    source: String,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerInput {
    source_id: u64,
    source_item_id: u64,
    name: String,
    content: Content,
    width: f64,
    height: f64,
    visible: bool,
    audio_enabled: bool,
    label: u8,
    start_frame: i64,
    range: [Frame; 2],
    /// Every native scalar is explicitly supplied, including disclosed defaults.
    properties: BTreeMap<Property, f64>,
    text_style: Option<TextStyle>,
    rich_text: Option<RichInput>,
    sliders: Vec<Slider>,
    markers: Vec<MarkerInput>,
    expressions: Vec<ExpressionInput>,
    /// Only these finite out-of-bounds source XY values may be adapted.
    adapt_position: Option<[f64; 2]>,
    /// Source timing retained for the detached frame-zero adaptation evaluation.
    raw_in_point: f64,
    raw_out_point: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RichInput {
    default_style: TextCharacterStyle,
    /// Input ranges are UTF-16; native storage remains UTF-8.
    runs: Vec<TextStyleRun>,
    point_origin: bool,
    /// Source cache evidence only; absent cache entries remain absent.
    expected_baselines: Vec<Option<f64>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Slider {
    name: String,
    value: f64,
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
struct ExpressionInput {
    target: ExpressionTarget,
    source_index: usize,
    enabled: bool,
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn evaluate(
    snapshot: &ae::CompositionSnapshot,
    roots: &[ae::PropertyAddress],
) -> Result<ae::EvaluatedProperties, Box<dyn std::error::Error>> {
    eprintln!(
        "Evaluating {} requested properties at {} seconds with unchanged production child limits",
        roots.len(),
        snapshot.time
    );
    let started = std::time::Instant::now();
    let result = automation_process::evaluate_expressions(
        snapshot,
        roots,
        Arc::new(AtomicBool::new(false)),
    )?;
    eprintln!(
        "Bounded evaluation passed in {:?}; {} expression evaluations",
        started.elapsed(),
        result.expression_evaluations
    );
    if automation_process::worker_active() {
        return Err("Expression child remains active".into());
    }
    Ok(result)
}
fn address(comp: u64, layer: u64, property: ae::ExpressionProperty) -> ae::PropertyAddress {
    ae::PropertyAddress {
        composition: ae::CompositionId(comp),
        layer: ae::LayerId(layer),
        property,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(result) = automation_process::dispatch_worker() {
        result?;
        return Ok(());
    }
    let mut args = std::env::args_os().skip(1);
    let input_path = PathBuf::from(
        args.next()
            .ok_or("Usage: reference_lyric_project reviewed-input.json NEW.lep")?,
    );
    let destination = PathBuf::from(args.next().ok_or("Missing output path")?);
    if args.next().is_some()
        || destination
            .extension()
            .is_none_or(|s| !s.eq_ignore_ascii_case("lep"))
    {
        return Err("Expected input JSON and a new .lep output path".into());
    }
    let input_bytes = fs::read(&input_path)?;
    if input_bytes.len() > 16 * 1024 * 1024 {
        return Err("Input exceeds16MiB".into());
    }
    let input: Input = serde_json::from_slice(&input_bytes)?;
    if input.contract_version != 1
        || input.layers.is_empty()
        || input.layers.len() > 1000
        || input.sources.is_empty()
    {
        return Err("Unsupported or empty reference data contract".into());
    }
    let evidence_path = destination.with_extension("provenance.json");
    let native_json_path = destination.with_extension("native.json");
    if [&destination, &evidence_path, &native_json_path]
        .iter()
        .any(|p| p.exists())
    {
        return Err("Refusing to replace any previous project/evidence output".into());
    }
    // This example deliberately has no source extraction, rewriting, or fallback.
    // A changed byte in either the manifest binding or inspected payload rejects.
    for source in &input.sources {
        if fs::read(&source.inspected_path)? != source.source.as_bytes()
            || source.sha256.len() != 64
        {
            return Err(
                "Expression does not match its separately inspected exact-byte file".into(),
            );
        }
    }
    let mut editor = Editor::default();
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
    let comp = editor.project().active_composition_id();
    let mut ids = BTreeMap::new();
    let mut evidence = Vec::new();
    let mut baseline_checks = Vec::new();
    // AddContent inserts at the top, so reverse source-stack traversal preserves it.
    for layer in input.layers.iter().rev() {
        if ids.contains_key(&layer.source_id) {
            return Err("Duplicate source layer ID".into());
        }
        if layer.properties.len() != Property::ALL.len()
            || Property::ALL
                .iter()
                .any(|p| !layer.properties.contains_key(p))
        {
            return Err("All native scalar properties require an explicit reviewed value".into());
        }
        editor.execute(Command::AddContent {
            content: layer.content.clone(),
            width: layer.width,
            height: layer.height,
            name: layer.name.clone(),
        })?;
        let id = editor.selected().ok_or("New layer was not selected")?;
        ids.insert(layer.source_id, id);
        let local_start = i64::from(layer.range[0]) - layer.start_frame;
        let local_end = i64::from(layer.range[1]) - layer.start_frame;
        let mut commands = vec![
            Command::SetLayerRange {
                id,
                start: u32::try_from(local_start)?,
                end: u32::try_from(local_end)?,
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
        if layer.content.audio().is_some() {
            commands.push(Command::SetAudioEnabled {
                id,
                enabled: layer.audio_enabled,
            });
        }
        for (property, value) in &layer.properties {
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
        if let Some(rich) = &layer.rich_text {
            let Content::Text { text, .. } = &layer.content else {
                return Err("Rich text requires text content".into());
            };
            let mut native =
                RichText::from_utf16_runs(text, rich.default_style.clone(), rich.runs.clone())?;
            native.point_origin = rich.point_origin;
            let metrics = native.line_metrics(
                text,
                layer
                    .text_style
                    .as_ref()
                    .ok_or("Missing point-text settings")?,
            )?;
            if metrics.len() != rich.expected_baselines.len() {
                return Err("Native hard-line count differs from source cache".into());
            }
            for (line, (metric, expected)) in
                metrics.iter().zip(&rich.expected_baselines).enumerate()
            {
                if let Some(expected) = expected {
                    let error = (metric.baseline - expected).abs();
                    if error > 0.0001 {
                        return Err("Native baseline differs from source cache".into());
                    }
                    baseline_checks.push(json!({"source_layer_id":layer.source_id,"line":line,"source_cached_baseline":expected,"native_baseline":metric.baseline,"absolute_error":error}));
                }
            }
            commands.push(Command::SetRichText {
                id,
                rich_text: Some(native),
            });
        }
        for expression in &layer.expressions {
            let source = input
                .sources
                .get(expression.source_index)
                .ok_or("Invalid reviewed-source index")?;
            commands.push(Command::SetExpression {
                id,
                target: expression.target,
                source: source.source.clone(),
                enabled: expression.enabled,
            });
        }
        editor.execute(Command::Batch(commands))?;
        for slider in &layer.sliders {
            editor.execute(Command::Effect {
                id,
                edit: EffectEdit::Add(EffectKind::SliderControl),
            })?;
            let effect = editor
                .project()
                .composition()
                .layer(id)
                .ok_or("Missing layer")?
                .effect_stack()
                .last()
                .ok_or("Missing slider")?
                .id();
            editor.execute(Command::Batch(vec![
                Command::Effect {
                    id,
                    edit: EffectEdit::Rename {
                        effect,
                        name: slider.name.clone(),
                    },
                },
                Command::Effect {
                    id,
                    edit: EffectEdit::SetValue {
                        effect,
                        parameter: EffectParam::Amount,
                        frame: 0,
                        value: slider.value,
                    },
                },
            ]))?;
        }
        for marker in &layer.markers {
            let target = MarkerTarget::Layer(id);
            editor.execute(Command::Marker {
                target,
                edit: MarkerEdit::Add {
                    frame: marker.frame,
                },
            })?;
            let marker_id = editor
                .project()
                .composition()
                .layer(id)
                .ok_or("Missing layer")?
                .markers()
                .iter()
                .find(|m| m.frame() == marker.frame)
                .ok_or("Missing marker")?
                .id();
            editor.execute(Command::Marker {
                target,
                edit: MarkerEdit::Update {
                    id: marker_id,
                    frame: marker.frame,
                    duration: marker.duration,
                    name: marker.name.clone(),
                    color: marker.color,
                },
            })?;
        }
        evidence.push(json!({"source_layer_id":layer.source_id,"native_layer_id":id,"source_item_id":layer.source_item_id,"name":layer.name}));
    }
    let mut snapshot = editor.project().expression_snapshot(comp, 0)?;
    let mut roots = Vec::new();
    for layer in &input.layers {
        let id = ids[&layer.source_id];
        let detached = snapshot
            .layers
            .iter_mut()
            .find(|l| l.id.0 == id)
            .ok_or("Missing snapshot layer")?;
        detached.in_point = layer.raw_in_point;
        detached.out_point = layer.raw_out_point;
        if let Some(raw) = layer.adapt_position {
            if raw.iter().any(|v| !v.is_finite()) || raw.iter().all(|v| v.abs() <= 1_000_000.0) {
                return Err("Position adaptation requires finite out-of-bounds source data".into());
            }
            if !layer
                .expressions
                .iter()
                .any(|e| e.target == ExpressionTarget::Position && e.enabled)
            {
                return Err("Position adaptation requires its enabled inspected expression".into());
            }
            detached.position.authored_value = ae::PropertyValue::Vector2(raw);
            roots.push(address(comp, id, ae::ExpressionProperty::Position));
        }
    }
    let adapted = evaluate(&snapshot, &roots)?;
    let mut adaptations = Vec::new();
    for layer in &input.layers {
        if let Some(raw) = layer.adapt_position {
            let id = ids[&layer.source_id];
            let value = adapted
                .get(&address(comp, id, ae::ExpressionProperty::Position))
                .ok_or("Missing bounded adapted position")?;
            let xy = match value {
                ae::PropertyValue::Vector2(v) => *v,
                ae::PropertyValue::Vector3(v) => [v[0], v[1]],
                _ => return Err("Expected vector Position result".into()),
            };
            editor.execute(Command::Batch(vec![
                Command::SetValue {
                    id,
                    property: Property::PositionX,
                    frame: 0,
                    value: xy[0],
                },
                Command::SetValue {
                    id,
                    property: Property::PositionY,
                    frame: 0,
                    value: xy[1],
                },
            ]))?;
            adaptations.push(json!({"source_layer_id":layer.source_id,"native_layer_id":id,"raw_position_xy":raw,"native_authored_position_xy":xy,"method":"exact inspected Position expression at frame0 in production bounded child","disabled_expression_fidelity":false}));
        }
    }
    editor.clear_history();
    let project = editor.project();
    let actual_stack: Vec<_> = project
        .composition()
        .layers()
        .iter()
        .map(Layer::id)
        .collect();
    let expected_stack: Vec<_> = input.layers.iter().map(|l| ids[&l.source_id]).collect();
    if actual_stack != expected_stack {
        return Err("Source stack order mismatch".into());
    }
    let initial_snapshot = project.expression_snapshot(comp, c.initial_frame)?;
    let initial_roots = project.expression_roots(comp, c.initial_frame, false)?;
    if initial_roots.is_empty() {
        return Err("Initial frame has no active expression-driven content".into());
    }
    let initial_evaluated = evaluate(&initial_snapshot, &initial_roots)?;
    project.with_evaluated_properties(comp, c.initial_frame, false, &initial_evaluated)?;
    let view = serde_json::to_vec(
        &json!({"version":1,"compositions":{comp.to_string():{"frame":c.initial_frame}},"workspace":{}}),
    )?;
    let bytes = project_file::encode(project, Some(&view))?;
    let decoded = project_file::decode(&bytes)?;
    if decoded.project != *project || project_file::encode(&decoded.project, decoded.view)? != bytes
    {
        return Err("Native deterministic roundtrip failed".into());
    }
    let receipt = json!({
        "contract_version":input.contract_version,
        "input_path":input_path,
        "source_composition_id":c.source_id,
        "native_composition_id":comp,
        "source_to_native_layers":evidence,
        "position_adaptations":adaptations,
        "reviewed_source_sha256":input.sources.iter().map(|s| &s.sha256).collect::<Vec<_>>(),
        "source_count":initial_snapshot.sources.len(),
        "binding_count":project.composition().layers().iter().map(|l| l.expressions().len()).sum::<usize>(),
        "marker_count":project.composition().layers().iter().map(|l| l.markers().len()).sum::<usize>(),
        "native_baseline_checks":baseline_checks,
        "initial_frame":c.initial_frame,
        "initial_evaluation":initial_evaluated,
        "position_adaptation_evaluation":adapted,
        "native_roundtrip_bytes_equal":true,
        "expression_child_reaped":!automation_process::worker_active(),
        "provenance":input.provenance,
    });
    write_new(&evidence_path, &serde_json::to_vec_pretty(&receipt)?)?;
    write_new(&native_json_path, project.to_json()?.as_bytes())?;
    write_new(&destination, &bytes)?;
    println!(
        "Wrote private native project: {} layers, {} bytes; exact-byte reviewed programs; deterministic roundtrip passed",
        input.layers.len(),
        bytes.len()
    );
    Ok(())
}
