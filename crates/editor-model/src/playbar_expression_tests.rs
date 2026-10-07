//! Public-API contracts for typed text/path expressions; no private reference data.
use libre_effects_core::expression_runtime as ae;
use libre_effects_core::*;

fn text(editor: &mut Editor, name: &str, value: &str) -> LayerId {
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: value.into(),
                font_size: 18.,
            },
            width: 200.,
            height: 40.,
            name: name.into(),
        })
        .unwrap();
    editor.selected().unwrap()
}
fn expression(id: LayerId, target: ExpressionTarget, source: &str) -> Command {
    Command::SetExpression {
        id,
        target,
        source: source.into(),
        enabled: true,
    }
}
fn triangle() -> VectorPath {
    VectorPath {
        closed: true,
        vertices: [[0., 0.], [10., 0.], [0., 10.]]
            .into_iter()
            .map(PathVertex::corner)
            .collect(),
    }
}

#[test]
fn playbar_typed_shared_view_preserves_authored_source_and_uniform_style() {
    let mut e = Editor::default();
    let end = text(&mut e, "Endpoint", "00:03");
    let current = text(&mut e, "Counter", "old");
    let style = e
        .project()
        .composition()
        .layer(current)
        .unwrap()
        .base_character_style()
        .unwrap();
    let mut rich = RichText::new(
        "old",
        style.clone(),
        vec![TextStyleRun {
            start: 0,
            end: 3,
            style: style.clone(),
        }],
    )
    .unwrap();
    rich.point_origin = true;
    e.execute(Command::SetRichText {
        id: current,
        rich_text: Some(rich),
    })
    .unwrap();
    e.execute(Command::AddRectangle).unwrap();
    let bar = e.selected().unwrap();
    e.execute(Command::SetPathMasks {
        id: bar,
        masks: vec![PathMask {
            path: triangle(),
            ..Default::default()
        }],
    })
    .unwrap();
    e.execute(Command::Batch(vec![
        expression(
            current,
            ExpressionTarget::SourceText,
            "message = thisComp.layer('Endpoint').text.sourceText; 'end:' + message;",
        ),
        Command::SetExpressionLocalBindings {
            id: current,
            target: ExpressionTarget::SourceText,
            bindings: vec!["message".into()],
        },
        expression(
            bar,
            ExpressionTarget::MaskPath(1),
            "createPath([[0,0],[linear(time,0,2,0,20),0],[0,10]],[],[],true)",
        ),
        expression(
            bar,
            ExpressionTarget::Position,
            "linear(time,0,2,[10,20],[30,40])",
        ),
    ]))
    .unwrap();
    let source = e.project().clone();
    let json = source.to_json().unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["version"],
        76
    );
    let snapshot = source.expression_snapshot(1, 30).unwrap();
    let roots = source.expression_roots(1, 30, false).unwrap();
    let values = ae::ExpressionEvaluator::default()
        .evaluate(&snapshot, &roots)
        .unwrap();
    let view = source
        .with_evaluated_properties(1, 30, false, &values)
        .unwrap();
    let comp = view.composition();
    let rendered = comp.layer(current).unwrap();
    assert_eq!(rendered.source_text_at(30), Some("end:00:03"));
    assert!(rendered.rich_text().unwrap().point_origin);
    assert_eq!(rendered.rich_text().unwrap().runs[0].style, style);
    assert_eq!(rendered.rich_text().unwrap().runs[0].end, 9);
    assert_eq!(
        comp.layer(bar).unwrap().path_masks()[0].path.vertices[1].position,
        [10., 0.]
    );
    assert_eq!(
        comp.layer(bar).unwrap().position2_at(30, 1. / 30.).unwrap(),
        [20., 30.]
    );
    assert!(
        values.values.keys().any(|address| address.layer.0 == end
            && address.property == ae::ExpressionProperty::SourceText)
    );
    assert!(view.to_json().is_err());
    assert_eq!(e.project().to_json().unwrap(), json);
    assert_eq!(Project::from_json(&json).unwrap(), source);
    e.undo();
    assert!(
        e.project()
            .composition()
            .layer(current)
            .unwrap()
            .expressions()
            .is_empty()
    );
    e.redo();
    assert_eq!(e.project(), &source);
}

#[test]
fn playbar_schema_and_invalid_local_metadata_reject_without_source_or_history_loss() {
    let mut e = Editor::default();
    let id = text(&mut e, "Text", "base");
    e.execute(expression(
        id,
        ExpressionTarget::SourceText,
        "part='x';part",
    ))
    .unwrap();
    e.execute(Command::SetExpressionLocalBindings {
        id,
        target: ExpressionTarget::SourceText,
        bindings: vec!["part".into()],
    })
    .unwrap();
    let before = e.project().clone();
    for bindings in [
        vec!["time".into()],
        vec!["x;throw 1".into()],
        vec!["part".into(), "part".into()],
    ] {
        assert!(
            e.execute(Command::SetExpressionLocalBindings {
                id,
                target: ExpressionTarget::SourceText,
                bindings
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }
    let mut raw = serde_json::to_value(&before).unwrap();
    raw["version"] = 75.into();
    assert!(Project::from_json(&raw.to_string()).is_err());
    e.execute(expression(
        id,
        ExpressionTarget::SourceText,
        "part='changed';part",
    ))
    .unwrap();
    assert_eq!(
        e.project()
            .composition()
            .layer(id)
            .unwrap()
            .expression(ExpressionTarget::SourceText)
            .unwrap()
            .local_bindings,
        ["part"]
    );
}

#[test]
fn playbar_mask_identity_and_mixed_character_styles_reject_atomically() {
    let mut e = Editor::default();
    let id = text(&mut e, "Mixed", "AB");
    let a = e
        .project()
        .composition()
        .layer(id)
        .unwrap()
        .base_character_style()
        .unwrap();
    let mut b = a.clone();
    b.fill_color = 0xff0000;
    let rich = RichText::new(
        "AB",
        a.clone(),
        vec![
            TextStyleRun {
                start: 0,
                end: 1,
                style: a,
            },
            TextStyleRun {
                start: 1,
                end: 2,
                style: b,
            },
        ],
    )
    .unwrap();
    e.execute(Command::SetRichText {
        id,
        rich_text: Some(rich),
    })
    .unwrap();
    let before = e.project().clone();
    assert!(
        e.execute(expression(id, ExpressionTarget::SourceText, "'text'"))
            .is_err()
    );
    assert!(
        e.execute(expression(id, ExpressionTarget::MaskPath(999), "value"))
            .is_err()
    );
    assert_eq!(e.project(), &before);
}
