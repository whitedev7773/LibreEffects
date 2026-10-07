//! Compare a detached frame snapshot against values captured from After Effects.
//! The input is local reference evidence, never a project import or pixel proof.
use libre_effects_ae_expressions::{
    CompositionSnapshot, ExpressionEvaluator, PropertyAddress, PropertyValue,
};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeSet, io::Read};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceCase {
    snapshot: CompositionSnapshot,
    expected: Vec<(PropertyAddress, PropertyValue)>,
}

fn close(actual: &PropertyValue, expected: &PropertyValue) -> bool {
    let number = |a: f64, b: f64| (a - b).abs() <= 1e-7;
    match (actual, expected) {
        (PropertyValue::Scalar(a), PropertyValue::Scalar(b)) => number(*a, *b),
        (PropertyValue::Vector2(a), PropertyValue::Vector2(b)) => {
            a.iter().zip(b).all(|(a, b)| number(*a, *b))
        }
        (PropertyValue::Vector3(a), PropertyValue::Vector3(b)) => {
            a.iter().zip(b).all(|(a, b)| number(*a, *b))
        }
        (PropertyValue::Path(a), PropertyValue::Path(b)) => {
            a.closed == b.closed
                && a.vertices.len() == b.vertices.len()
                && a.in_tangents.len() == b.in_tangents.len()
                && a.out_tangents.len() == b.out_tangents.len()
                && a.vertices
                    .iter()
                    .chain(&a.in_tangents)
                    .chain(&a.out_tangents)
                    .flatten()
                    .zip(
                        b.vertices
                            .iter()
                            .chain(&b.in_tangents)
                            .chain(&b.out_tangents)
                            .flatten(),
                    )
                    .all(|(a, b)| number(*a, *b))
        }
        (PropertyValue::Text(a), PropertyValue::Text(b)) => a == b,
        _ => false,
    }
}

fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("Usage: compare_reference <cases.json>")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Reference input exceeds 64 MiB".into());
    }
    let cases: Vec<ReferenceCase> =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))?;
    if cases.is_empty() || cases.len() > 256 {
        return Err("Reference requires 1..=256 cases; an empty comparison cannot pass".into());
    }
    for case in &cases {
        if case.expected.is_empty() || case.expected.len() > 16_384 {
            return Err("Each case requires 1..=16384 expected properties".into());
        }
        let unique: BTreeSet<_> = case.expected.iter().map(|(address, _)| address).collect();
        if unique.len() != case.expected.len() {
            return Err("Duplicate expected property address".into());
        }
    }
    let evaluator = ExpressionEvaluator::default();
    let mut reports = Vec::new();
    let mut success = true;
    for case in cases {
        let targets: Vec<_> = case
            .expected
            .iter()
            .map(|(address, _)| address.clone())
            .collect();
        match evaluator.evaluate(&case.snapshot, &targets) {
            Ok(values) => {
                let mismatches: Vec<_> = case.expected.iter().filter_map(|(address, expected)| {
                    let actual = values.get(address);
                    (!actual.is_some_and(|actual| close(actual, expected)))
                        .then(|| json!({ "address": address, "actual": actual, "expected": expected }))
                }).collect();
                success &= mismatches.is_empty();
                reports.push(
                    json!({ "composition": case.snapshot.id, "time": case.snapshot.time,
                    "checked": case.expected.len(), "mismatches": mismatches }),
                );
            }
            Err(error) => {
                success = false;
                reports.push(json!({ "composition": case.snapshot.id, "time": case.snapshot.time, "error": error }));
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "passed": success, "cases": reports }))?
    );
    Ok(success)
}

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
