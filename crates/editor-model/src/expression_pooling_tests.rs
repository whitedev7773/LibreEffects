use libre_effects_core::{expression_runtime as ae, *};
use std::collections::BTreeSet;
#[path = "../examples/support/pooling_fixture.rs"]
mod fixture;

#[test]
fn native_192_bindings_share_six_sources_and_keep_independent_contexts() {
    let editor = fixture::scene();
    let project = editor.project();
    let before = project_file::encode(project, None).unwrap();
    let authored = project.to_json().unwrap();
    let snapshot = project.expression_snapshot(1, 120).unwrap();
    assert_eq!(snapshot.sources.len(), 6);
    assert_eq!(
        snapshot.sources.iter().map(String::len).sum::<usize>(),
        19_056
    );
    assert_eq!(
        snapshot.sources.iter().collect::<BTreeSet<_>>(),
        fixture::programs().iter().collect()
    );
    let bindings = snapshot
        .layers
        .iter()
        .flat_map(|layer| [&layer.position, &layer.scale, &layer.opacity])
        .map(|p| p.expression.as_ref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 192);
    assert_eq!(
        bindings
            .iter()
            .map(|p| snapshot.sources[p.source_id.0 as usize].len())
            .sum::<usize>(),
        609_792
    );
    let wire = serde_json::to_vec(&snapshot).unwrap();
    assert!(wire.len() < 100_000);
    let from_wire: ae::CompositionSnapshot = serde_json::from_slice(&wire).unwrap();
    assert_eq!(snapshot, from_wire);
    let roots = project.expression_roots(1, 120, false).unwrap();
    assert_eq!(roots.len(), 192);
    let result = ae::ExpressionEvaluator::default()
        .evaluate(&from_wire, &roots)
        .unwrap();
    fixture::assert_values(project, 120, &result);
    let next = project.expression_snapshot(1, 150).unwrap();
    let later = ae::ExpressionEvaluator::default()
        .evaluate(&next, &roots)
        .unwrap();
    fixture::assert_values(project, 150, &later);
    assert_ne!(result.values, later.values);
    assert_eq!(project.to_json().unwrap(), authored);
    assert_eq!(project_file::encode(project, None).unwrap(), before);
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
    let restored = project_file::decode(&before).unwrap();
    assert_eq!(
        project_file::encode(&restored.project, None).unwrap(),
        before
    );
    assert_eq!(
        restored.project.expression_snapshot(1, 120).unwrap(),
        snapshot
    );
}

#[test]
fn exact_line_endings_and_disabled_sources_survive_snapshot_and_history() {
    let mut editor = Editor::default();
    for _ in 0..3 {
        editor.execute(Command::AddRectangle).unwrap();
    }
    editor.clear_history();
    for (id, source, enabled) in [
        (1, "value;\r\n", true),
        (2, "value;\n", true),
        (3, "(", false),
    ] {
        editor
            .execute(Command::SetExpression {
                id,
                target: ExpressionTarget::Opacity,
                source: source.into(),
                enabled,
            })
            .unwrap();
    }
    let before = editor.project().clone();
    let snapshot = before.expression_snapshot(1, 0).unwrap();
    assert_eq!(snapshot.sources.len(), 3);
    assert_eq!(
        snapshot
            .sources
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["value;\r\n", "value;\n", "("])
    );
    let roots = before.expression_roots(1, 0, false).unwrap();
    let result = ae::ExpressionEvaluator::default()
        .evaluate(&snapshot, &roots)
        .unwrap();
    assert_eq!(result.expression_evaluations, 2);
    editor.undo();
    editor.redo();
    assert_eq!(editor.project(), &before);
    let id = snapshot
        .layers
        .iter()
        .find(|l| l.id == ae::LayerId(3))
        .unwrap()
        .opacity
        .expression
        .as_ref()
        .unwrap()
        .source_id;
    assert_eq!(snapshot.sources[id.0 as usize], "(");
}

#[test]
fn native_unique_source_budget_rejects_without_mutating_authored_project() {
    let mut editor = Editor::default();
    for id in 1..=33 {
        editor.execute(Command::AddRectangle).unwrap();
        let body = format!("value;/*{id:02}{}*/", "p".repeat(16_384 - 12));
        assert_eq!(body.len(), 16_384);
        editor
            .execute(Command::SetExpression {
                id,
                target: ExpressionTarget::Opacity,
                source: body,
                enabled: false,
            })
            .unwrap();
        editor.clear_history();
    }
    let before = editor.project().to_json().unwrap();
    let error = editor.project().expression_snapshot(1, 0).unwrap_err();
    assert!(error.contains("unique-source budget"), "{error}");
    assert_eq!(editor.project().to_json().unwrap(), before);
}
