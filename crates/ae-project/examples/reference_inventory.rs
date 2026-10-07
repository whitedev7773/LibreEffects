//! Diagnostic inventory of a read-only AE scripting snapshot. This never creates
//! a native project, evaluates captured programs or opens captured media paths.
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};

const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Default, Serialize)]
struct Inventory {
    native_conversion_verified: bool,
    composition_ids: Vec<u64>,
    compositions: usize,
    layers: usize,
    properties: usize,
    keys: usize,
    enabled_expressions: usize,
    distinct_enabled_sources: usize,
    source_bytes: usize,
    capture_errors: usize,
    unavailable_values: usize,
    unavailable_character_runs: usize,
    three_d_layers: usize,
    adjustment_layers: usize,
    track_matte_receivers: usize,
    shape_layers: usize,
    text_documents: usize,
    effect_match_names: BTreeMap<String, usize>,
    font_postscript_names: BTreeSet<String>,
    source_item_ids: BTreeSet<u64>,
    missing_footage_item_ids: BTreeSet<u64>,
    composition_rates: BTreeMap<u64, f64>,
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], String> {
    value[field]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("Missing array: {field}"))
}
fn id(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .filter(|id| *id != 0)
        .ok_or_else(|| format!("Invalid nonzero ID: {field}"))
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("Missing string: {field}"))
}
fn collect(
    current: u64,
    items: &BTreeMap<u64, &Value>,
    active: &mut BTreeSet<u64>,
    visited: &mut BTreeSet<u64>,
    ordered: &mut Vec<u64>,
) -> Result<(), String> {
    if active.contains(&current) || active.len() >= 64 {
        return Err("Cyclic or oversized composition dependency chain".into());
    }
    if !visited.insert(current) {
        return Ok(());
    }
    let item = items.get(&current).ok_or("Dangling source item")?;
    if item["kind"] != "composition" {
        return Ok(());
    }
    active.insert(current);
    for layer in array(item, "layers")? {
        if !layer["sourceId"].is_null() {
            collect(id(layer, "sourceId")?, items, active, visited, ordered)?;
        }
    }
    active.remove(&current);
    ordered.push(current);
    Ok(())
}
fn document(value: &Value, report: &mut Inventory) -> Result<(), String> {
    if value["type"] != "text_document" {
        return Ok(());
    }
    report.text_documents += 1;
    report
        .font_postscript_names
        .insert(text(&value["attributes"], "font")?.into());
    if value["characterRunState"] != "available" {
        report.unavailable_character_runs += 1;
    }
    for run in array(value, "characterRuns")? {
        report
            .font_postscript_names
            .insert(text(&run["attributes"], "font")?.into());
    }
    Ok(())
}
fn inspect(bytes: &[u8], root: Option<u64>) -> Result<Inventory, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Snapshot exceeds 64 MiB".into());
    }
    let snapshot: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if snapshot["format"] != "libre-effects-ae-reference" || snapshot["version"] != 1 {
        return Err("Not a supported AE diagnostic snapshot".into());
    }
    let source = array(&snapshot, "items")?;
    if source.len() > 4096 {
        return Err("Item budget exceeded".into());
    }
    let mut items = BTreeMap::new();
    let mut layers = BTreeSet::new();
    for item in source {
        if items.insert(id(item, "id")?, item).is_some() {
            return Err("Duplicate item ID".into());
        }
        if item["kind"] == "composition" {
            for layer in array(item, "layers")? {
                if !layers.insert(id(layer, "id")?) {
                    return Err("Duplicate layer ID".into());
                }
                if layers.len() > 20_000 {
                    return Err("Layer budget exceeded".into());
                }
            }
        }
    }
    let roots: Vec<_> = match root {
        Some(root) => {
            if items
                .get(&root)
                .is_none_or(|item| item["kind"] != "composition")
            {
                return Err("Root is not a composition".into());
            }
            vec![root]
        }
        None => items
            .iter()
            .filter(|(_, v)| v["kind"] == "composition")
            .map(|(id, _)| *id)
            .collect(),
    };
    let mut report = Inventory {
        capture_errors: array(&snapshot, "errors")?.len(),
        ..Default::default()
    };
    let mut visited = BTreeSet::new();
    for root in roots {
        collect(
            root,
            &items,
            &mut BTreeSet::new(),
            &mut visited,
            &mut report.composition_ids,
        )?;
    }
    let mut programs = BTreeSet::new();
    for comp_id in report.composition_ids.clone() {
        report.compositions += 1;
        let comp = items[&comp_id];
        let rate = comp["frameRate"]
            .as_f64()
            .filter(|r| r.is_finite() && *r > 0.0)
            .ok_or("Invalid frame rate")?;
        report.composition_rates.insert(comp_id, rate);
        for layer in array(comp, "layers")? {
            report.layers += 1;
            report.three_d_layers +=
                usize::from(layer["threeDLayer"] == true || layer["threeDPerChar"] == true);
            report.adjustment_layers += usize::from(layer["adjustmentLayer"] == true);
            report.track_matte_receivers += usize::from(layer["hasTrackMatte"] == true);
            report.shape_layers += usize::from(layer["matchName"] == "ADBE Vector Layer");
            if !layer["sourceId"].is_null() {
                let source_id = id(layer, "sourceId")?;
                report.source_item_ids.insert(source_id);
                if items[&source_id]["footageMissing"] == true {
                    report.missing_footage_item_ids.insert(source_id);
                }
            }
            let mut stack: Vec<_> = array(layer, "properties")?.iter().map(|p| (p, 0)).collect();
            while let Some((property, depth)) = stack.pop() {
                report.properties += 1;
                if report.properties > 100_000 || depth > 32 {
                    return Err("Property count/depth budget exceeded".into());
                }
                if property["isEffect"] == true {
                    *report
                        .effect_match_names
                        .entry(text(property, "matchName")?.into())
                        .or_default() += 1;
                }
                if property["valueState"] == "unavailable" {
                    report.unavailable_values += 1;
                }
                if let Some(children) = property["children"].as_array() {
                    stack.extend(children.iter().map(|p| (p, depth + 1)));
                } else {
                    let keys = array(property, "keys")?;
                    report.keys += keys.len();
                    if report.keys > 250_000 {
                        return Err("Key budget exceeded".into());
                    }
                    document(&property["authoredValue"], &mut report)?;
                    for key in keys {
                        document(&key["value"], &mut report)?;
                    }
                    if property["attributes"]["expressionEnabled"] == true {
                        let source = text(&property["attributes"], "expression")?;
                        if source.len() > 16_384 {
                            return Err("Expression source budget exceeded".into());
                        }
                        report.enabled_expressions += 1;
                        programs.insert(source);
                        if programs.len() > 4096 {
                            return Err("Expression count budget exceeded".into());
                        }
                    }
                }
            }
        }
    }
    report.distinct_enabled_sources = programs.len();
    report.source_bytes = programs.iter().map(|source| source.len()).sum();
    // The report inventories declarations. Even an empty error list cannot
    // establish omitted source semantics, media fidelity or native conversion.
    Ok(report)
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("Usage: reference_inventory SNAPSHOT.json [COMPOSITION_ID]")?;
    let root = args
        .next()
        .map(|id| id.parse::<u64>().map_err(|e| e.to_string()))
        .transpose()?;
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&inspect(&bytes, root)?).map_err(|e| e.to_string())?
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn sample() -> Value {
        json!({"format":"libre-effects-ae-reference","version":1,"errors":[],"items":[
            {"id":1,"kind":"composition","frameRate":60,"layers":[{"id":11,"sourceId":2,"properties":[]}]},
            {"id":2,"kind":"composition","frameRate":23,"layers":[{"id":22,"sourceId":null,"properties":[]}]},
            {"id":3,"kind":"composition","frameRate":30,"layers":[]}
        ]})
    }
    #[test]
    fn closure_keeps_mixed_rates_and_never_claims_conversion() {
        let report = inspect(&serde_json::to_vec(&sample()).unwrap(), Some(1)).unwrap();
        assert_eq!(report.composition_ids, [2, 1]);
        assert_eq!(report.layers, 2);
        assert_eq!(report.composition_rates[&2], 23.0);
        assert!(!report.native_conversion_verified);
    }
    #[test]
    fn invalid_ids_links_cycles_versions_and_resource_limits_reject() {
        for case in 0..5 {
            let mut sample = sample();
            match case {
                0 => sample["items"][1]["id"] = json!(1),
                1 => sample["items"][0]["layers"][0]["sourceId"] = json!(99),
                2 => sample["items"][1]["layers"][0]["sourceId"] = json!(1),
                3 => sample["version"] = json!(2),
                _ => sample["items"][1]["layers"][0]["id"] = json!(11),
            }
            assert!(inspect(&serde_json::to_vec(&sample).unwrap(), Some(1)).is_err());
        }
        assert!(inspect(&vec![b' '; MAX_BYTES as usize + 1], None).is_err());
    }
}
