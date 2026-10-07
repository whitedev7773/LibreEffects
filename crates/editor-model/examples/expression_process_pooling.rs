//! GPUI-free real-child test of the production expression transport.
//! Usage: cargo run -p libre-effects-editor-model --example expression_process_pooling
#[allow(dead_code)]
#[path = "../../../apps/desktop/src/automation_process.rs"]
mod automation_process;
#[path = "support/pooling_fixture.rs"]
mod fixture;
use libre_effects_core::{expression_runtime as ae, *};
use std::sync::{Arc, atomic::AtomicBool};

fn main() {
    if let Some(result) = automation_process::dispatch_worker() {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    let editor = fixture::scene();
    let project = editor.project();
    let before = project_file::encode(project, None).unwrap();
    let snapshot = project.expression_snapshot(1, 120).unwrap();
    let roots = project.expression_roots(1, 120, false).unwrap();
    assert_eq!(snapshot.sources.len(), 6);
    assert_eq!(
        snapshot.sources.iter().map(String::len).sum::<usize>(),
        19_056
    );
    let wire = serde_json::to_vec(&snapshot).unwrap();
    assert!(wire.len() < 100_000);
    let decoded: ae::CompositionSnapshot = serde_json::from_slice(&wire).unwrap();
    assert_eq!(decoded, snapshot);
    let run = |snapshot: &ae::CompositionSnapshot| {
        automation_process::evaluate_expressions(snapshot, &roots, Arc::new(AtomicBool::new(false)))
    };
    fixture::assert_values(project, 120, &run(&snapshot).unwrap());
    assert!(!automation_process::worker_active());
    let mut invalid = snapshot.clone();
    invalid.layers[0]
        .opacity
        .expression
        .as_mut()
        .unwrap()
        .source_id = ae::ExpressionSourceId(u32::MAX);
    invalid.layers[0]
        .opacity
        .expression
        .as_mut()
        .unwrap()
        .enabled = false;
    assert_eq!(
        run(&invalid).unwrap_err().kind,
        ae::EvaluationErrorKind::InvalidSnapshot
    );
    assert!(!automation_process::worker_active());
    let mut duplicate = snapshot.clone();
    duplicate.sources.push(duplicate.sources[0].clone());
    assert_eq!(
        run(&duplicate).unwrap_err().kind,
        ae::EvaluationErrorKind::InvalidSnapshot
    );
    assert!(!automation_process::worker_active());
    let canceled = automation_process::evaluate_expressions(
        &snapshot,
        &roots,
        Arc::new(AtomicBool::new(true)),
    )
    .unwrap_err();
    assert_eq!(canceled.kind, ae::EvaluationErrorKind::Canceled);
    assert!(!automation_process::worker_active());
    let next = project.expression_snapshot(1, 150).unwrap();
    fixture::assert_values(project, 150, &run(&next).unwrap());
    assert!(!automation_process::worker_active());
    assert_eq!(project_file::encode(project, None).unwrap(), before);
    println!(
        "PASS: real worker IPC,192 bindings/6 exact sources, independent values at2 times, invalid disabled ID, duplicate source, cancellation, permit reuse, unchanged native bytes"
    );
}
