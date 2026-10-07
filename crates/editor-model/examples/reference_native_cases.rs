//! Pair authentic expected values with snapshots derived from a reopened LEP.
//! Usage: reference_native_cases PROJECT.lep AE_CASES.json NEW_CASES.json
use libre_effects_core::{Project, project_file};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn read(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("Input exceeds byte budget".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("Expected LEP, authentic cases and new output".into());
    }
    let bytes = read(Path::new(&args[0]), 256 * 1024 * 1024)?;
    let project: Project = project_file::decode(&bytes)?.project;
    let cases: Vec<Value> = serde_json::from_slice(&read(Path::new(&args[1]), 64 * 1024 * 1024)?)?;
    if cases.is_empty() || cases.len() > 256 {
        return Err("Case count outside 1..256".into());
    }
    let mut native = Vec::new();
    for case in cases {
        let id = case["snapshot"]["id"]
            .as_u64()
            .ok_or("Missing composition ID")?;
        let comp = project
            .composition_by_id(id)
            .ok_or("Composition absent from LEP")?;
        let seconds = case["snapshot"]["time"]
            .as_f64()
            .ok_or("Missing sample time")?;
        let frame = seconds * comp.fps().as_f64();
        if !frame.is_finite()
            || frame < 0.0
            || frame >= f64::from(comp.duration())
            || (frame - frame.round()).abs() > 1e-7
        {
            return Err("Case requires an explicit subframe sampling contract".into());
        }
        let expected = case["expected"]
            .as_array()
            .filter(|v| !v.is_empty())
            .ok_or("Empty expected values")?;
        native.push(json!({"snapshot":project.expression_snapshot(id,frame.round() as u32)?,"expected":expected}));
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?
        .write_all(&serde_json::to_vec_pretty(&native)?)?;
    println!(
        "Paired {} authentic cases with reopened native snapshots",
        native.len()
    );
    Ok(())
}
