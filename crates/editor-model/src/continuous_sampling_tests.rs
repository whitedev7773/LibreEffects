//! Synthetic continuous-time boundaries; no supplied project data.
use libre_effects_core::expression_runtime as ae;
use libre_effects_core::*;

fn at(seconds: f64, fps: u32) -> CompositionSample {
    CompositionSample::from_seconds(seconds, fps.into()).unwrap()
}
fn empty(comp: u64, sample: CompositionSample) -> ae::EvaluatedProperties {
    ae::EvaluatedProperties {
        composition: ae::CompositionId(comp),
        time: sample.seconds(),
        values: Default::default(),
        dependencies: Default::default(),
        expression_evaluations: 0,
        host_reads: 0,
    }
}
fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::AddRectangle).unwrap();
    e
}
fn value(e: &mut Editor, id: u64, p: Property, frame: u32, v: f64) {
    e.execute(Command::SetValue {
        id,
        property: p,
        frame,
        value: v,
    })
    .unwrap();
}

#[test]
fn continuous_metadata_is_opt_in_and_schema_guarded_with_exact_history() {
    let mut e = scene();
    let old = e.project().to_json().unwrap();
    assert_eq!(e.project().composition().preserve_nested_frame_rate(), None);
    e.execute(Command::SetPreserveNestedFrameRate {
        composition: 1,
        preserve: false,
    })
    .unwrap();
    let source = e.project().to_json().unwrap();
    let mut raw: serde_json::Value = serde_json::from_str(&source).unwrap();
    assert_eq!(raw["version"], 77);
    raw["version"] = 76.into();
    assert!(Project::from_json(&raw.to_string()).is_err());
    assert_eq!(
        Project::from_json(&source).unwrap().to_json().unwrap(),
        source
    );
    e.undo();
    assert_eq!(e.project().to_json().unwrap(), old);
    e.redo();
    assert_eq!(e.project().to_json().unwrap(), source);
    e.execute(Command::SetColor {
        id: 1,
        color: 0x123456,
    })
    .unwrap();
    assert_eq!(serde_json::to_value(e.project()).unwrap()["version"], 77);
}

#[test]
fn continuous_authored_fractional_view_has_no_guest_and_cannot_be_saved() {
    let mut e = scene();
    value(&mut e, 1, Property::PositionX, 0, 0.);
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::PositionX,
        frame: 0,
    })
    .unwrap();
    value(&mut e, 1, Property::PositionX, 30, 120.);
    let original = e.project().clone();
    let sample = at(0.05, 30); // frame 1.5
    let view = original
        .with_evaluated_properties_at_sample(1, sample, false, &empty(1, sample))
        .unwrap();
    assert_eq!(
        view.composition()
            .layer(1)
            .unwrap()
            .position2_at(1, 1. / 30.)
            .unwrap()[0],
        6.
    );
    assert!(view.evaluated_at_sample(1, sample));
    assert!(!view.evaluated_at_sample(1, at(0.06, 30)));
    assert!(view.to_json().is_err());
    assert!(serde_json::to_value(&view).is_err());
    assert!(project_file::encode(&view, None).is_err());
    assert!(e.replace_project(view).is_err());
    assert_eq!(e.project(), &original);
}

#[test]
fn continuous_expression_value_and_time_share_one_fractional_context() {
    let mut e = scene();
    value(&mut e, 1, Property::PositionX, 0, 0.);
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::PositionX,
        frame: 0,
    })
    .unwrap();
    value(&mut e, 1, Property::PositionX, 30, 120.);
    e.execute(Command::SetExpression {
        id: 1,
        target: ExpressionTarget::Position,
        source: "[value[0] + time, value[1]]".into(),
        enabled: true,
    })
    .unwrap();
    let sample = at(0.05, 30);
    let snapshot = e
        .project()
        .expression_snapshot_at_sample(1, sample)
        .unwrap();
    assert_eq!(snapshot.time, 0.05);
    assert_eq!(
        snapshot.layers[0].position.authored_value,
        ae::PropertyValue::Vector2([6., 540.])
    );
    let roots = e
        .project()
        .expression_roots_at_sample(1, sample, false)
        .unwrap();
    let values = ae::ExpressionEvaluator::default()
        .evaluate(&snapshot, &roots)
        .unwrap();
    let view = e
        .project()
        .with_evaluated_properties_at_sample(1, sample, false, &values)
        .unwrap();
    assert_eq!(
        view.composition()
            .layer(1)
            .unwrap()
            .position2_at(1, 1. / 30.)
            .unwrap(),
        [6.05, 540.]
    );
}

#[test]
fn continuous_unsupported_fractional_families_fail_without_touching_source() {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Shape(Shape::default()),
        width: 30.,
        height: 30.,
        name: "Shape".into(),
    })
    .unwrap();
    e.execute(Command::EditTrack {
        id: 1,
        property: PropertyPath::Shape(ShapeParam::Roundness),
        edit: TrackEdit::ToggleKey { frame: 0 },
    })
    .unwrap();
    let before = e.project().clone();
    let sample = at(0.05, 30);
    let error = before
        .with_evaluated_properties_at_sample(1, sample, false, &empty(1, sample))
        .unwrap_err();
    assert!(error.contains("animated shape"), "{error}");
    assert_eq!(e.project(), &before);
    // Whole-frame legacy sampling remains valid.
    let whole = CompositionSample::from_frame(1, 30.into()).unwrap();
    assert!(
        before
            .with_evaluated_properties_at_sample(1, whole, false, &empty(1, whole))
            .is_ok()
    );
}

#[test]
fn continuous_nested_offset_remap_and_freeze_preserve_seconds() {
    let mut e = scene();
    e.execute(Command::SetPreserveNestedFrameRate {
        composition: 1,
        preserve: false,
    })
    .unwrap();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureCompositionRate {
        name: "Parent".into(),
        width: 100,
        height: 100,
        fps: 23.into(),
        duration: 115,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::AddCompositionLayer {
        composition: 1,
        frame: 0,
    })
    .unwrap();
    let id = e.selected().unwrap();
    let source = e.project().composition_by_id(1).unwrap();
    let layer = e.project().composition().layer(id).unwrap();
    let parent = CompositionSample::from_frame(1, 23.into()).unwrap();
    let sample = layer
        .composition_sample(parent, 23.into(), source)
        .unwrap()
        .unwrap();
    assert!((sample.frame() - 30. / 23.).abs() < 1e-12);
    assert_eq!(sample.seconds(), parent.seconds());
    e.execute(Command::FreezeTimeRemap { id, frame: 1 })
        .unwrap();
    let layer = e.project().composition().layer(id).unwrap();
    assert_eq!(layer.time_remap().unwrap().value_at(1), parent.seconds());
    e.execute(Command::SetPreserveNestedFrameRate {
        composition: 1,
        preserve: true,
    })
    .unwrap();
    let layer = e.project().composition().layer(id).unwrap();
    let source = e.project().composition_by_id(1).unwrap();
    assert_eq!(
        layer
            .composition_sample(parent, 23.into(), source)
            .unwrap()
            .unwrap()
            .frame(),
        1.
    );
}
