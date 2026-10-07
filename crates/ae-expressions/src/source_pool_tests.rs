use super::*;

#[test]
fn source_table_rejects_duplicates_and_invalid_ids_even_without_requested_roots() {
    let evaluator = ExpressionEvaluator::default();
    let mut snapshot = scene();
    snapshot.sources = vec!["42;".into(), "42;".into()];
    let error = evaluator.evaluate(&snapshot, &[]).unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::InvalidSnapshot);
    assert!(error.message.contains("Duplicate expression source"));

    snapshot.sources.truncate(1);
    for enabled in [false, true] {
        for id in [1, u32::MAX] {
            snapshot.layers[0].opacity.expression = Some(ExpressionProgram {
                source_id: ExpressionSourceId(id),
                enabled,
                local_bindings: vec![],
            });
            let error = evaluator.evaluate(&snapshot, &[]).unwrap_err();
            assert_eq!(error.kind, EvaluationErrorKind::InvalidSnapshot);
            assert!(error.message.contains("source ID"));
        }
    }
    snapshot.sources.clear();
    snapshot.layers[0].opacity.expression = Some(ExpressionProgram {
        source_id: ExpressionSourceId(0),
        enabled: false,
        local_bindings: vec![],
    });
    assert_eq!(
        evaluator.evaluate(&snapshot, &[]).unwrap_err().kind,
        EvaluationErrorKind::InvalidSnapshot
    );
}

#[test]
fn disabled_and_unused_source_entries_still_consume_the_byte_budget() {
    let mut snapshot = scene();
    snapshot
        .sources
        .push("x".repeat(MAX_EXPRESSION_SOURCE_BYTES + 1));
    snapshot.layers[0].opacity.expression = Some(ExpressionProgram {
        source_id: ExpressionSourceId(0),
        enabled: false,
        local_bindings: vec![],
    });
    let error = ExpressionEvaluator::default()
        .evaluate(&snapshot, &[])
        .unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::Budget);
    assert!(error.message.contains("source budget"));
}

#[test]
fn source_identity_preserves_whitespace_crlf_and_unicode_exactly() {
    let mut snapshot = scene();
    snapshot.layers = vec![
        layer(1, "One"),
        layer(2, "Two"),
        layer(3, "Three"),
        layer(4, "Four"),
    ];
    let originals = [
        "value+time;",
        " value+time;",
        "// 표시\nvalue+time;",
        "// 표시\r\nvalue+time;",
    ];
    for (item, source) in snapshot.layers.iter_mut().zip(originals) {
        expression(&mut snapshot.sources, &mut item.opacity, source);
    }
    assert_eq!(snapshot.sources, originals);
    let encoded = serde_json::to_string(&snapshot).unwrap();
    let received: CompositionSnapshot = serde_json::from_str(&encoded).unwrap();
    assert_eq!(received, snapshot);
    let requested: Vec<_> = (1..=4)
        .map(|index| address(index, ExpressionProperty::Opacity))
        .collect();
    let result = ExpressionEvaluator::default()
        .evaluate(&received, &requested)
        .unwrap();
    assert_eq!(result.expression_evaluations, 4);
    for key in &requested {
        assert_eq!(result.get(key), Some(&PropertyValue::Scalar(77.0)));
    }
}

#[test]
fn old_private_snapshot_shape_and_malformed_source_ids_reject_on_decode() {
    let snapshot = scene();
    let mut old = serde_json::to_value(&snapshot).unwrap();
    old.as_object_mut().unwrap().remove("sources");
    let error = serde_json::from_value::<CompositionSnapshot>(old).unwrap_err();
    assert!(error.to_string().contains("missing field `sources`"));

    let mut old_program = serde_json::to_value(&snapshot).unwrap();
    old_program["layers"][0]["opacity"]["expression"] =
        serde_json::json!({ "source": "7;", "enabled": true });
    let error = serde_json::from_value::<CompositionSnapshot>(old_program).unwrap_err();
    assert!(error.to_string().contains("unknown field `source`"));
    for id in [
        serde_json::json!(-1),
        serde_json::json!(4294967296_u64),
        serde_json::json!(0.5),
        serde_json::json!("0"),
    ] {
        assert!(serde_json::from_value::<ExpressionSourceId>(id).is_err());
    }
    assert_eq!(serde_json::to_value(ExpressionSourceId(13)).unwrap(), 13);
}

// Independently authored arithmetic/context programs. Padding comments make
// source sizes reproducible without incorporating any original template text.
fn pooled_programs() -> [String; 6] {
    [
        r#"
const own = thisLayer.index;
const gain = effect('Gain')(1);
const cue = marker.key(1);
const shift = own + time + gain + cue.time + startTime + inPoint + outPoint;
const prior = own > 1 ? thisComp.layer(own - 1).opacity : 0;
[value[0] + shift, value[1] + prior];
"#,
        r#"
const point = transform.position;
const answer = value.map((component, axis) => component + point[axis] / 10);
answer;
"#,
        r#"
const gain = effect('Gain')('Slider');
const shift = thisLayer.index + gain + time + marker.numKeys;
if (time >= marker.key(1).time) { value + shift; } else { value - shift; }
"#,
        r#"
function subtract(left, right) { return left - right; }
const own = thisLayer.index;
const gain = thisLayer.effect('Gain')(1);
const shift = own + time + gain + thisLayer.marker.key(1).time + startTime + inPoint + outPoint;
const prior = own > 1 ? thisComp.layer(own - 1).transform.opacity : 0;
[subtract(value[0], shift), subtract(value[1], prior)];
"#,
        r#"
function adjusted(component, position) { return component - position / 20; }
const point = thisLayer.position;
[adjusted(value[0], point[0]), adjusted(value[1], point[1])];
"#,
        r#"
function adjusted(base, index, gain, clock, count) { return base - index - gain - clock - count; }
const gain = thisLayer.effect('Gain')('ADBE Slider Control-0001');
adjusted(value, thisLayer.index, gain, time, thisLayer.marker.numKeys);
"#,
    ]
    .map(|source| {
        let mut source = source.to_string();
        let padding = 3176 - source.len() - 4;
        source.push_str("/*");
        source.push_str(&" ".repeat(padding));
        source.push_str("*/");
        assert_eq!(source.len(), 3176);
        source
    })
}

#[test]
fn six_pooled_programs_evaluate_192_bindings_with_independent_property_context() {
    let mut snapshot = scene();
    snapshot.layers.clear();
    let programs = pooled_programs();
    let mut requested = Vec::new();
    for index in 0..64 {
        let mut item = layer(index + 1, &format!("Synthetic {}", index + 1));
        let n = index as f64;
        item.start_time = -n / 8.0;
        item.in_point = n / 4.0;
        item.out_point = 20.0 + n / 2.0;
        item.position.authored_value = PropertyValue::Vector2([n * 10.0, n * 20.0]);
        item.scale.authored_value = PropertyValue::Vector2([100.0 + n, 200.0 + n]);
        item.opacity.authored_value = PropertyValue::Scalar(50.0 + n);
        item.sliders.push(SliderSnapshot {
            name: "Gain".into(),
            property: PropertySnapshot::authored(PropertyValue::Scalar(n / 2.0)),
        });
        item.markers.push(MarkerSnapshot {
            time: n / 100.0,
            comment: format!("Cue {index}"),
        });
        let group = (index as usize % 2) * 3;
        for (property, source) in [&mut item.position, &mut item.scale, &mut item.opacity]
            .into_iter()
            .zip(&programs[group..group + 3])
        {
            expression(&mut snapshot.sources, property, source);
        }
        // Scale requests discover positions and neighboring opacity before
        // some of their own requested roots are encountered.
        for property in [
            ExpressionProperty::Scale,
            ExpressionProperty::Opacity,
            ExpressionProperty::Position,
        ] {
            requested.push(address(index + 1, property));
        }
        snapshot.layers.push(item);
    }
    assert_eq!(requested.len(), 192);
    assert_eq!(snapshot.sources.len(), 6);
    assert_eq!(
        snapshot.sources.iter().map(String::len).sum::<usize>(),
        19_056
    );
    assert_eq!(requested.len() * 3176, 609_792);
    let authored = snapshot.clone();
    let evaluator = ExpressionEvaluator::default();
    for time in [2.0, 3.25] {
        snapshot.time = time;
        let result = evaluator.evaluate(&snapshot, &requested).unwrap();
        assert_eq!(result.expression_evaluations, 192);
        assert_eq!(result.values.len(), 256);
        let mut previous_opacity = 0.0;
        for index in 0..64 {
            let n = index as f64;
            let even = index % 2 == 0;
            let sign = if even { 1.0 } else { -1.0 };
            let shift = n + 1.0 + time + n / 2.0 + n / 100.0 - n / 8.0 + n / 4.0 + 20.0 + n / 2.0;
            let position = [n * 10.0 + sign * shift, n * 20.0 + sign * previous_opacity];
            let divisor = if even { 10.0 } else { -20.0 };
            let scale = [
                100.0 + n + position[0] / divisor,
                200.0 + n + position[1] / divisor,
            ];
            let opacity = 50.0 + n + sign * (n + 1.0 + n / 2.0 + time + 1.0);
            let position_key = address(index + 1, ExpressionProperty::Position);
            let scale_key = address(index + 1, ExpressionProperty::Scale);
            let opacity_key = address(index + 1, ExpressionProperty::Opacity);
            for (key, expected) in [(&position_key, position), (&scale_key, scale)] {
                let PropertyValue::Vector2(actual) = result.get(key).unwrap() else {
                    panic!("Expected vector");
                };
                for axis in 0..2 {
                    assert!(
                        (actual[axis] - expected[axis]).abs() < 1e-9,
                        "layer {} at {time}: {key:?}",
                        index + 1
                    );
                }
            }
            assert_eq!(
                result.get(&opacity_key),
                Some(&PropertyValue::Scalar(opacity))
            );
            assert_eq!(result.dependencies[&scale_key], vec![position_key.clone()]);
            let gain_key = address(index + 1, ExpressionProperty::Slider("Gain".into()));
            assert_eq!(result.dependencies[&opacity_key], vec![gain_key.clone()]);
            let mut position_deps = vec![gain_key];
            if index > 0 {
                position_deps.push(address(index, ExpressionProperty::Opacity));
            }
            position_deps.sort();
            assert_eq!(result.dependencies[&position_key], position_deps);
            previous_opacity = opacity;
        }
    }
    snapshot.time = authored.time;
    assert_eq!(snapshot, authored);

    let mut limited = ExpressionEvaluator::default();
    limited.limits.max_expression_evaluations = 191;
    assert_eq!(
        limited.evaluate(&snapshot, &requested).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );
    limited.limits = EvaluationLimits::default();
    limited.limits.max_dependency_depth = 1;
    assert_eq!(
        limited.evaluate(&snapshot, &requested).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );
}

#[test]
fn identical_sources_still_detect_per_property_cycles_and_latch_caught_failures() {
    let mut snapshot = scene();
    let source =
        "try { thisComp.layer(thisLayer.index === 1 ? 2 : 1).opacity; } catch (error) { 99; }";
    for item in &mut snapshot.layers {
        expression(&mut snapshot.sources, &mut item.opacity, source);
    }
    assert_eq!(snapshot.sources.len(), 1);
    let error = ExpressionEvaluator::default()
        .evaluate(&snapshot, &[address(1, ExpressionProperty::Opacity)])
        .unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::Cycle);
    assert!(
        error
            .message
            .contains("Caption.Opacity -> Control.Opacity -> Caption.Opacity")
    );
}

#[test]
fn recursive_shared_wrapper_keeps_parameters_locals_and_completion_per_call() {
    let mut snapshot = scene();
    snapshot.layers.push(layer(3, "Third"));
    let source = r#"
if (this !== undefined) { throw new Error('Expected strict eval'); }
if (typeof data !== 'undefined' || typeof programs !== 'undefined') {
  throw new Error('Host scope leaked');
}
var visits = typeof visits === 'undefined' ? [] : visits;
var owner = thisLayer.index;
let local = value;
const ownTime = time;
visits.push(owner);
function ownValue() { return owner + visits[0] + local + ownTime; }
const next = owner < 3 ? thisComp.layer(owner + 1).opacity : 0;
visits.push(next);
if (time < 3) { ownValue() + visits[1]; } else { ownValue() - visits[1]; }
"#;
    for (index, item) in snapshot.layers.iter_mut().enumerate() {
        item.opacity.authored_value = PropertyValue::Scalar(10.0 * (index + 1) as f64);
        expression(&mut snapshot.sources, &mut item.opacity, source);
    }
    assert_eq!(snapshot.sources.len(), 1);
    let authored = snapshot.clone();
    let requested: Vec<_> = (1..=3)
        .map(|index| address(index, ExpressionProperty::Opacity))
        .collect();
    let evaluator = ExpressionEvaluator::default();
    for (time, expected) in [(2.0, [78.0, 64.0, 38.0]), (4.0, [28.0, -12.0, 40.0])] {
        snapshot.time = time;
        let result = evaluator.evaluate(&snapshot, &requested).unwrap();
        assert_eq!(result.expression_evaluations, 3);
        for (index, key) in requested.iter().enumerate() {
            assert_eq!(
                result.get(key),
                Some(&PropertyValue::Scalar(expected[index]))
            );
            assert_eq!(
                result.dependencies[key],
                requested
                    .get(index + 1)
                    .cloned()
                    .into_iter()
                    .collect::<Vec<_>>()
            );
        }
    }
    let mut limited = ExpressionEvaluator::default();
    limited.limits.max_dependency_depth = 2;
    let error = limited.evaluate(&snapshot, &requested).unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::Budget);
    assert!(error.message.contains("dependency-depth"));
    snapshot.time = authored.time;
    assert_eq!(snapshot, authored);
}

#[test]
fn shared_wrapper_failure_keeps_property_context_and_next_batch_is_fresh() {
    let mut snapshot = scene();
    let source = "if (thisLayer.index === 2) { throw new Error('second property'); } value;";
    for item in &mut snapshot.layers {
        expression(&mut snapshot.sources, &mut item.opacity, source);
    }
    let keys = [
        address(1, ExpressionProperty::Opacity),
        address(2, ExpressionProperty::Opacity),
    ];
    let evaluator = ExpressionEvaluator::default();
    let error = evaluator.evaluate(&snapshot, &keys).unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::JavaScript);
    assert_eq!(error.property, Some(keys[1].clone()));
    assert!(error.message.contains("second property"));

    // The same source ID in another batch must compile its new exact text.
    snapshot.sources[0] = "if (time > 1) { value + thisLayer.index; } else { 0; }".into();
    let result = evaluator.evaluate(&snapshot, &keys).unwrap();
    assert_eq!(result.expression_evaluations, 2);
    assert_eq!(result.get(&keys[0]), Some(&PropertyValue::Scalar(76.0)));
    assert_eq!(result.get(&keys[1]), Some(&PropertyValue::Scalar(77.0)));

    evaluator.cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        evaluator.evaluate(&snapshot, &keys).unwrap_err().kind,
        EvaluationErrorKind::Canceled
    );
    evaluator.cancel.store(false, Ordering::Relaxed);
    assert_eq!(evaluator.evaluate(&snapshot, &keys).unwrap(), result);
}
