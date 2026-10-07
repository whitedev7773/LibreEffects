use super::*;

#[test]
fn active_execution_and_wall_ceiling_are_independent() {
    let limits = EvaluationLimits::default();
    assert_eq!(limits.execution_time, Duration::from_millis(100));
    assert_eq!(limits.wall_time, Duration::from_secs(2));
    // A preempted evaluator has not spent its active-work allowance. This is a
    // specified clock-policy boundary, not a claim about a prior native event.
    assert_eq!(
        stop_reason(
            Duration::from_millis(10),
            Duration::from_millis(400),
            8,
            &limits
        ),
        None
    );
    assert_eq!(
        stop_reason(
            Duration::from_millis(100),
            Duration::from_millis(110),
            8,
            &limits
        ),
        Some(BudgetStop::Execution)
    );
    assert_eq!(
        stop_reason(Duration::ZERO, Duration::from_secs(2), 0, &limits),
        Some(BudgetStop::Wall)
    );
    assert_eq!(
        stop_reason(Duration::ZERO, Duration::ZERO, 10_001, &limits),
        Some(BudgetStop::Interrupts)
    );
    assert_eq!(
        stop_reason(
            Duration::from_millis(99),
            Duration::from_millis(1_999),
            10_000,
            &limits
        ),
        None
    );
}

#[test]
fn wall_ceiling_cannot_be_disabled_or_extended_beyond_process_contract() {
    for wall_time in [
        Duration::ZERO,
        Duration::from_secs(2) + Duration::from_nanos(1),
    ] {
        let limits = EvaluationLimits {
            wall_time,
            ..EvaluationLimits::default()
        };
        assert_eq!(
            limits.validate().unwrap_err().kind,
            EvaluationErrorKind::Budget
        );
    }
    let evaluator = ExpressionEvaluator {
        limits: EvaluationLimits {
            wall_time: Duration::from_nanos(1),
            ..EvaluationLimits::default()
        },
        ..ExpressionEvaluator::default()
    };
    let error = evaluator.evaluate(&snapshot(), &[]).unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::Budget);
    assert!(error.message.contains("wall-time budget"), "{error:?}");
}

fn snapshot() -> CompositionSnapshot {
    CompositionSnapshot {
        id: CompositionId(1),
        width: 100,
        height: 100,
        duration: 1.0,
        frame_rate: FrameRate {
            numerator: 30,
            denominator: 1,
        },
        time: 0.0,
        sources: vec![],
        layers: vec![],
    }
}

fn validate(snapshot: &CompositionSnapshot) -> Result<(), EvaluationError> {
    prepare(snapshot, &[], &EvaluationLimits::default()).map(|_| ())
}

#[test]
fn source_table_entry_ceiling_includes_unused_entries() {
    let mut snapshot = snapshot();
    snapshot.sources = (0..MAX_EXPRESSION_SOURCES)
        .map(|index| format!("{index};"))
        .collect();
    validate(&snapshot).unwrap();
    snapshot.sources.push("another unused source".into());
    assert_eq!(
        validate(&snapshot).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );
}

#[test]
fn source_bytes_include_unused_entries_and_count_utf8_bytes() {
    let mut snapshot = snapshot();
    snapshot.sources = vec!["é".repeat(MAX_EXPRESSION_SOURCE_BYTES / 2)];
    validate(&snapshot).unwrap();
    snapshot.sources[0].push('é');
    assert_eq!(
        validate(&snapshot).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );

    snapshot.sources = (0..MAX_TOTAL_EXPRESSION_SOURCE_BYTES / MAX_EXPRESSION_SOURCE_BYTES)
        .map(|index| {
            let mut source = " ".repeat(MAX_EXPRESSION_SOURCE_BYTES - 1);
            source.push(char::from(b'0' + index as u8));
            source
        })
        .collect();
    validate(&snapshot).unwrap();
    snapshot.sources.push("x".into());
    assert_eq!(
        validate(&snapshot).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );
}

#[test]
fn bridge_serializes_source_once_with_ids_and_null_disabled_bindings() {
    let mut snapshot = snapshot();
    snapshot.sources = vec!["value + 7;".into()];
    let enabled = PropertySnapshot {
        authored_value: PropertyValue::Vector2([1.0, 2.0]),
        expression: Some(ExpressionProgram {
            source_id: ExpressionSourceId(0),
            enabled: true,
            local_bindings: vec![],
        }),
    };
    snapshot.layers.push(LayerSnapshot {
        id: LayerId(1),
        name: "Synthetic".into(),
        start_time: 0.0,
        in_point: 0.0,
        out_point: 1.0,
        position: enabled.clone(),
        scale: enabled,
        opacity: PropertySnapshot {
            authored_value: PropertyValue::Scalar(1.0),
            expression: Some(ExpressionProgram {
                source_id: ExpressionSourceId(0),
                enabled: false,
                local_bindings: vec![],
            }),
        },
        source_text: None,
        masks: vec![],
        sliders: vec![],
        markers: vec![],
    });
    let (wire, _) = prepare(&snapshot, &[], &EvaluationLimits::default()).unwrap();
    let encoded = serialize_wire(&wire).unwrap();
    assert_eq!(encoded.matches("value + 7;").count(), 1);
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["sources"], serde_json::json!(["value + 7;"]));
    assert_eq!(value["properties"][0]["source_id"], 0);
    assert_eq!(value["properties"][1]["source_id"], 0);
    assert!(value["properties"][2]["source_id"].is_null());
    assert!(
        value["properties"]
            .as_array()
            .unwrap()
            .iter()
            .all(|property| property.get("source").is_none())
    );
}

#[test]
fn wrappers_compile_once_per_used_exact_source_and_are_immutable() {
    let mut snapshot = snapshot();
    snapshot.sources = vec!["value;".into(), " value;".into(), "(".into()];
    let property = |value, source_id| PropertySnapshot {
        authored_value: value,
        expression: Some(ExpressionProgram {
            source_id: ExpressionSourceId(source_id),
            enabled: source_id != 2,
            local_bindings: vec![],
        }),
    };
    for id in 1..=3 {
        snapshot.layers.push(LayerSnapshot {
            id: LayerId(id),
            name: format!("Layer {id}"),
            start_time: 0.0,
            in_point: 0.0,
            out_point: 1.0,
            position: property(PropertyValue::Vector2([id as f64, 2.0]), 0),
            scale: property(PropertyValue::Vector2([100.0, 100.0]), 0),
            opacity: property(PropertyValue::Scalar(50.0), if id == 3 { 2 } else { 1 }),
            source_text: None,
            masks: vec![],
            sliders: vec![],
            markers: vec![],
        });
    }
    let requested: Vec<_> = snapshot
        .layers
        .iter()
        .flat_map(|layer| {
            [
                ExpressionProperty::Position,
                ExpressionProperty::Scale,
                ExpressionProperty::Opacity,
            ]
            .map(|property| PropertyAddress {
                composition: snapshot.id,
                layer: layer.id,
                property,
            })
        })
        .collect();
    let (wire, _) = prepare(&snapshot, &requested, &EvaluationLimits::default()).unwrap();
    let input = serialize_wire(&wire).unwrap();
    let runtime = Runtime::new().unwrap();
    let context = Context::full(&runtime).unwrap();
    context.with(|ctx| {
        // Observe actual constructor calls without adding production telemetry
        // or making the private wrapper cache visible to expressions.
        let inspect: Function = ctx
            .eval(
                r#"(() => {
                    const wrappers = [];
                    globalThis.Function = new Proxy(Function, {
                        construct(target, args) {
                            const wrapper = Reflect.construct(target, args);
                            wrappers.push(wrapper);
                            return wrapper;
                        }
                    });
                    return () => JSON.stringify({
                        count: wrappers.length,
                        frozen: wrappers.every(wrapper => Object.isFrozen(wrapper)
                            && Object.isFrozen(wrapper.prototype))
                    });
                })()"#,
            )
            .unwrap();
        let evaluate: Function = ctx.eval(include_str!("host.js")).unwrap();
        let output: String = evaluate.call((input,)).unwrap();
        let output: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(output["error"].is_null(), "{output}");
        assert_eq!(output["expression_evaluations"], 8);
        assert_eq!(output["values"].as_array().unwrap().len(), 9);
        let observed: String = inspect.call(()).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&observed).unwrap(),
            serde_json::json!({"count": 2, "frozen": true})
        );
    });
}

#[test]
fn bounded_writer_accepts_exact_limit_and_never_appends_overflow() {
    let mut writer = BoundedWireWriter {
        bytes: vec![],
        exceeded: false,
    };
    writer.write_all(&vec![b' '; MAX_WIRE_BYTES]).unwrap();
    assert_eq!(writer.bytes.len(), MAX_WIRE_BYTES);
    assert!(writer.bytes.capacity() <= MAX_WIRE_BYTES);
    writer.write_all(&[]).unwrap();
    assert!(writer.write_all(b"x").is_err());
    assert!(writer.exceeded);
    assert_eq!(writer.bytes.len(), MAX_WIRE_BYTES);
}

#[test]
fn encoded_json_expansion_is_bounded_before_entering_quickjs() {
    let mut snapshot = snapshot();
    // The table remains below both source byte ceilings. Control characters
    // require six JSON bytes each, so valid metadata pushes the bridge over 4 MiB.
    snapshot.sources = (0..8)
        .map(|index| format!("{}{}", "\0".repeat(MAX_EXPRESSION_SOURCE_BYTES - 1), index))
        .collect();
    snapshot.layers = (0..20)
        .map(|index| LayerSnapshot {
            id: LayerId(index),
            name: "\0".repeat(16_000),
            start_time: 0.0,
            in_point: 0.0,
            out_point: 1.0,
            position: PropertySnapshot::authored(PropertyValue::Vector2([0.0, 0.0])),
            scale: PropertySnapshot::authored(PropertyValue::Vector2([100.0, 100.0])),
            opacity: PropertySnapshot::authored(PropertyValue::Scalar(100.0)),
            source_text: None,
            masks: vec![],
            sliders: vec![],
            markers: vec![],
        })
        .collect();
    let (wire, _) = prepare(&snapshot, &[], &EvaluationLimits::default()).unwrap();
    let error = serialize_wire(&wire).unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::Budget);
    assert!(error.message.contains("4 MiB"));
    let error = ExpressionEvaluator::default()
        .evaluate(&snapshot, &[])
        .unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::Budget);
    assert!(error.message.contains("4 MiB"));
}
