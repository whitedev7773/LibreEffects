use crate::*;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[path = "source_pool_tests.rs"]
mod source_pooling;

#[path = "text_path_tests.rs"]
mod text_paths;

fn layer(id: u64, name: &str) -> LayerSnapshot {
    LayerSnapshot {
        id: LayerId(id),
        name: name.into(),
        start_time: -0.5,
        in_point: 1.0,
        out_point: 9.0,
        position: PropertySnapshot::authored(PropertyValue::Vector2([10.0, 20.0])),
        scale: PropertySnapshot::authored(PropertyValue::Vector2([100.0, 100.0])),
        opacity: PropertySnapshot::authored(PropertyValue::Scalar(75.0)),
        source_text: None,
        masks: vec![],
        sliders: vec![],
        markers: vec![],
    }
}
fn scene() -> CompositionSnapshot {
    CompositionSnapshot {
        id: CompositionId(1),
        width: 1200,
        height: 800,
        duration: 12.0,
        frame_rate: FrameRate {
            numerator: 60,
            denominator: 1,
        },
        time: 2.0,
        sources: vec![],
        layers: vec![layer(1, "Caption"), layer(2, "Control")],
    }
}
fn address(layer: u64, property: ExpressionProperty) -> PropertyAddress {
    PropertyAddress {
        composition: CompositionId(1),
        layer: LayerId(layer),
        property,
    }
}
fn expression(sources: &mut Vec<String>, property: &mut PropertySnapshot, source: &str) {
    let index = sources
        .iter()
        .position(|entry| entry == source)
        .unwrap_or_else(|| {
            sources.push(source.into());
            sources.len() - 1
        });
    property.expression = Some(ExpressionProgram {
        source_id: ExpressionSourceId(index.try_into().unwrap()),
        enabled: true,
        local_bindings: vec![],
    });
}
fn opacity(source: &str) -> Result<PropertyValue, EvaluationError> {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        source,
    );
    let key = address(1, ExpressionProperty::Opacity);
    Ok(ExpressionEvaluator::default()
        .evaluate(&snapshot, std::slice::from_ref(&key))?
        .get(&key)
        .unwrap()
        .clone())
}
#[test]
fn actual_javascript_completion_values_and_lexical_functions() {
    assert_eq!(
        opacity(
            "function cube(x) { return x*x*x; } var x=3; if (time > 1) { cube(x); } else { 0; }"
        )
        .unwrap(),
        PropertyValue::Scalar(27.0)
    );
    assert_eq!(
        opacity("let total=0; for(let i=0;i<5;i++){ total+=i; } total;").unwrap(),
        PropertyValue::Scalar(10.0)
    );
    assert_eq!(
        opacity("try { throw new Error('ordinary'); } catch(e) { 17; }").unwrap(),
        PropertyValue::Scalar(17.0)
    );
    assert_eq!(
        opacity("var x = 1;").unwrap_err().kind,
        EvaluationErrorKind::InvalidResult
    );
}
#[test]
fn authored_value_and_independent_signed_origin() {
    assert_eq!(
        opacity("value + startTime + thisLayer.startTime + inPoint + outPoint").unwrap(),
        PropertyValue::Scalar(84.0)
    );
}
#[test]
fn named_layers_controls_vectors_and_rational_frames() {
    let mut snapshot = scene();
    snapshot.frame_rate = FrameRate {
        numerator: 30_000,
        denominator: 1001,
    };
    snapshot.layers[1].sliders.push(SliderSnapshot {
        name: "Start Pont".into(),
        property: PropertySnapshot::authored(PropertyValue::Scalar(12.0)),
    });
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].position,
        "const p=thisComp.layer('Control').transform.position; [p[0] + thisComp.width/2, thisComp.layer(2).effect('Start Pont')(1) + framesToTime(30)]",
    );
    let key = address(1, ExpressionProperty::Position);
    let result = ExpressionEvaluator::default()
        .evaluate(&snapshot, std::slice::from_ref(&key))
        .unwrap();
    assert_eq!(
        result.get(&key),
        Some(&PropertyValue::Vector2([610.0, 13.001]))
    );
    assert_eq!(result.dependencies[&key].len(), 2);
    assert_eq!(
        opacity("framesToTime(18, 24)").unwrap(),
        PropertyValue::Scalar(0.75)
    );
}
#[test]
fn markers_are_sorted_one_based_composition_times() {
    let mut snapshot = scene();
    snapshot.layers[0].markers = vec![
        MarkerSnapshot {
            time: 2.25,
            comment: "Focus".into(),
        },
        MarkerSnapshot {
            time: 5.5,
            comment: "Hide".into(),
        },
    ];
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "var found = 0; for (var i=1;i<=thisLayer.marker.numKeys;i++) { var key=thisLayer.marker.key(i); if(key.comment==='Focus') found=key.time; } found*10;",
    );
    let key = address(1, ExpressionProperty::Opacity);
    let result = ExpressionEvaluator::default()
        .evaluate(&snapshot, std::slice::from_ref(&key))
        .unwrap();
    assert_eq!(result.get(&key), Some(&PropertyValue::Scalar(22.5)));
}
#[test]
fn dependencies_are_evaluated_once_and_rebuilt_each_frame() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[1].opacity,
        "time*10",
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "thisComp.layer('Control').opacity + thisComp.layer('Control').transform.opacity",
    );
    let key = address(1, ExpressionProperty::Opacity);
    let evaluator = ExpressionEvaluator::default();
    let result = evaluator
        .evaluate(&snapshot, std::slice::from_ref(&key))
        .unwrap();
    assert_eq!(result.get(&key), Some(&PropertyValue::Scalar(40.0)));
    assert_eq!(result.expression_evaluations, 2);
    assert_eq!(
        result.dependencies[&key],
        vec![address(2, ExpressionProperty::Opacity)]
    );
    snapshot.time = 3.0;
    assert_eq!(
        evaluator
            .evaluate(&snapshot, std::slice::from_ref(&key))
            .unwrap()
            .get(&key),
        Some(&PropertyValue::Scalar(60.0))
    );
}
#[test]
fn detects_dependency_cycles_even_when_guest_catches() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "try {thisComp.layer('Control').opacity;} catch(e) {42;}",
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[1].opacity,
        "thisComp.layer('Caption').opacity",
    );
    let error = ExpressionEvaluator::default()
        .evaluate(&snapshot, &[address(1, ExpressionProperty::Opacity)])
        .unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::Cycle);
    assert!(error.message.contains("Caption.Opacity"));
}
#[test]
fn rejects_missing_host_operations_even_when_caught() {
    for source in [
        "try {thisComp.layer('Absent')} catch(e) {} 9;",
        "try {thisLayer.opacity=4} catch(e) {} 9;",
        "try {thisLayer.position[0]=4} catch(e) {} 9;",
        "try {app.project} catch(e) {} 9;",
        "try {Math.random()} catch(e) {} 9;",
    ] {
        assert!(opacity(source).is_err(), "{source}");
    }
}
#[test]
fn no_async_files_network_ui_or_modules() {
    for source in [
        "File('x')",
        "fetch('https://example.com')",
        "Window('dialog')",
        "(async function(){ return 1; })(); 7;",
        "import('file:///tmp/x'); 7;",
    ] {
        assert!(opacity(source).is_err(), "{source}");
    }
}
#[test]
fn result_types_and_nonfinite_values_are_not_coerced() {
    for source in [
        "NaN",
        "Infinity",
        "1/0",
        "'12'",
        "true",
        "({valueOf(){return 12;}})",
        "[1,2]",
        "undefined",
    ] {
        assert_eq!(
            opacity(source).unwrap_err().kind,
            EvaluationErrorKind::InvalidResult,
            "{source}"
        );
    }
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].position,
        "[1, NaN]",
    );
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, &[address(1, ExpressionProperty::Position)])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::InvalidResult
    );
}
#[test]
fn bounded_infinite_loop_and_cancellation() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "while(true){}",
    );
    let mut evaluator = ExpressionEvaluator::default();
    evaluator.limits.max_interrupts = 10;
    evaluator.limits.execution_time = Duration::from_millis(50);
    assert_eq!(
        evaluator
            .evaluate(&snapshot, &[address(1, ExpressionProperty::Opacity)])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::Budget
    );
    evaluator.cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        evaluator.evaluate(&snapshot, &[]).unwrap_err().kind,
        EvaluationErrorKind::Canceled
    );
}

#[test]
fn active_cpu_limit_stops_guest_loop_without_using_interrupt_count_as_the_limit() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "while (true) {} ",
    );
    let mut evaluator = ExpressionEvaluator::default();
    evaluator.limits.execution_time = Duration::from_millis(20);
    evaluator.limits.max_interrupts = 100_000;
    let error = evaluator
        .evaluate(&snapshot, &[address(1, ExpressionProperty::Opacity)])
        .unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::Budget);
    assert!(error.message.contains("execution budget"), "{error:?}");
    assert!(!error.message.contains("interrupt-count"));
}
#[test]
fn invalid_snapshots_and_disabled_expressions() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "unsupported syntax!",
    );
    snapshot.layers[0]
        .opacity
        .expression
        .as_mut()
        .unwrap()
        .enabled = false;
    let key = address(1, ExpressionProperty::Opacity);
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, std::slice::from_ref(&key))
            .unwrap()
            .get(&key),
        Some(&PropertyValue::Scalar(75.0))
    );
    snapshot.layers[0].markers = vec![
        MarkerSnapshot {
            time: 2.0,
            comment: "A".into(),
        },
        MarkerSnapshot {
            time: 1.0,
            comment: "B".into(),
        },
    ];
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, &[key])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::InvalidSnapshot
    );
}
#[test]
fn duplicate_layer_names_resolve_first_and_ids_never_round() {
    let mut snapshot = scene();
    snapshot.layers[1].name = "Caption".into();
    snapshot.layers[1].id = LayerId(u64::MAX);
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[1].opacity,
        "thisComp.layer('Caption').opacity + 1",
    );
    let key = address(u64::MAX, ExpressionProperty::Opacity);
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, std::slice::from_ref(&key))
            .unwrap()
            .get(&key),
        Some(&PropertyValue::Scalar(76.0))
    );
}

// Independently authored five-state fixture. This algorithm uses de Casteljau
// interpolation and a data-driven stage table, not the supplied expression text.
fn staged_program(property: &str) -> String {
    format!(
        r#"
function mix(a,b,t) {{ return a+(b-a)*t; }}
function curve(t,a,b,c,d) {{
  const left=mix(a,b,t), center=mix(b,c,t), right=mix(c,d,t);
  return mix(mix(left,center,t),mix(center,right,t),t);
}}
const names=['Dormant','Arrive','Reading','Depart','Gone'];
const points=names.map(name=>thisComp.layer(name).transform.{property});
const knobs=thisComp.layer('Bezier');
const shape=['Start Pont','Y1','Y2','End Point'].map(name=>knobs.effect(name)(1));
const span=framesToTime(thisComp.layer('Duration').effect('Duration (FPS)')(1));
const starts=[inPoint];
for(let i=1;i<=thisLayer.marker.numKeys;i++) {{
  const m=thisLayer.marker.key(i);
  if(['Focus','Hide','End'].indexOf(m.comment)>=0) starts.push(m.time);
}}
let answer=points[0];
for(let stage=0;stage<starts.length;stage++) {{
  if(time>=starts[stage]) {{
    if(time>=starts[stage]+span) {{ answer=points[stage+1]; }}
    else {{
      const u=curve((time-starts[stage])/span,...shape);
      const v=curve(u,0,0,1,1);
      const from=points[stage], to=points[stage+1];
      answer=Array.isArray(from)?from.map((item,index)=>mix(item,to[index],v)):mix(from,to,v);
    }}
  }}
}}
answer;
"#
    )
}

fn five_state_scene() -> CompositionSnapshot {
    let mut snapshot = scene();
    snapshot.layers = vec![
        layer(1, "Caption"),
        layer(2, "Gap"),
        layer(3, "Bezier"),
        layer(4, "Duration"),
    ];
    snapshot.layers[0].in_point = 1.0 / 60.0;
    snapshot.layers[0].start_time = 1.0 / 60.0;
    snapshot.layers[0].markers = vec![
        MarkerSnapshot {
            time: 0.0,
            comment: "Show".into(),
        },
        MarkerSnapshot {
            time: 2.0,
            comment: "Focus".into(),
        },
        MarkerSnapshot {
            time: 4.0,
            comment: "Hide".into(),
        },
        MarkerSnapshot {
            time: 6.0,
            comment: "End".into(),
        },
    ];
    snapshot.layers[1].position.authored_value = PropertyValue::Vector2([0.0, 90.8]);
    snapshot.layers[2].sliders = [
        ("Start Pont", 0.0),
        ("Y1", 0.95),
        ("Y2", 0.9),
        ("End Point", 1.0),
    ]
    .into_iter()
    .map(|(name, value)| SliderSnapshot {
        name: name.into(),
        property: PropertySnapshot::authored(PropertyValue::Scalar(value)),
    })
    .collect();
    snapshot.layers[3].sliders.push(SliderSnapshot {
        name: "Duration (FPS)".into(),
        property: PropertySnapshot::authored(PropertyValue::Scalar(32.0)),
    });
    for (index, name) in ["Dormant", "Arrive", "Reading", "Depart", "Gone"]
        .iter()
        .enumerate()
    {
        let mut state = layer(index as u64 + 5, name);
        state.position.authored_value =
            PropertyValue::Vector2([100.0 + index as f64 * 250.0, 500.0 - index as f64 * 50.0]);
        state.scale.authored_value =
            PropertyValue::Vector2([20.0 + index as f64 * 20.0, 30.0 + index as f64 * 15.0]);
        state.opacity.authored_value = PropertyValue::Scalar(index as f64 * 25.0);
        snapshot.layers.push(state);
    }
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[5].position,
        "const dy = parseInt(thisComp.layer('Gap').transform.position[1],10); [thisComp.width*0.5, thisComp.height*0.5+dy]",
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[6].position,
        "[thisComp.width*0.5, thisComp.height*0.5]",
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[7].position,
        "const dy = parseInt(thisComp.layer('Gap').transform.position[1],10); [thisComp.width*0.5, thisComp.height*0.5-dy]",
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].position,
        &staged_program("position"),
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].scale,
        &staged_program("scale"),
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        &staged_program("opacity"),
    );
    snapshot
}

#[test]
fn six_program_state_graph_at_all_transition_boundaries() {
    let mut snapshot = five_state_scene();
    let authored = snapshot.clone();
    let properties = [
        ExpressionProperty::Position,
        ExpressionProperty::Scale,
        ExpressionProperty::Opacity,
    ];
    let requested = properties
        .iter()
        .map(|property| address(1, property.clone()))
        .collect::<Vec<_>>();
    let positions = [
        [100.0, 500.0],
        [600.0, 490.0],
        [600.0, 400.0],
        [600.0, 310.0],
        [1100.0, 300.0],
    ];
    let starts = [1.0 / 60.0, 2.0, 4.0, 6.0];
    let span = 32.0 / 60.0;
    // At half duration, cubic control values yield 0.81875; applying the
    // independent endpoint-slope-zero curve gives this exact weight.
    let half_weight = 0.91335400390625;
    let evaluator = ExpressionEvaluator::default();
    for (stage, start) in starts.into_iter().enumerate() {
        for (offset, weight) in [(0.0, 0.0), (span / 2.0, half_weight), (span, 1.0)] {
            snapshot.time = start + offset;
            let result = evaluator.evaluate(&snapshot, &requested).unwrap();
            assert_eq!(result.expression_evaluations, 6);
            let expected: [f64; 2] = std::array::from_fn(|axis| {
                positions[stage][axis]
                    + (positions[stage + 1][axis] - positions[stage][axis]) * weight
            });
            match result.get(&requested[0]).unwrap() {
                PropertyValue::Vector2(value) => {
                    for axis in 0..2 {
                        assert!(
                            (value[axis] - expected[axis]).abs() < 1e-9,
                            "stage={stage}, offset={offset}"
                        )
                    }
                }
                _ => panic!("Position must be a vector"),
            }
            match result.get(&requested[1]).unwrap() {
                PropertyValue::Vector2(value) => {
                    assert!((value[0] - (20.0 + stage as f64 * 20.0 + 20.0 * weight)).abs() < 1e-9);
                    assert!((value[1] - (30.0 + stage as f64 * 15.0 + 15.0 * weight)).abs() < 1e-9);
                }
                _ => panic!("Scale must be a vector"),
            }
            match result.get(&requested[2]).unwrap() {
                PropertyValue::Scalar(value) => {
                    assert!((value - (stage as f64 * 25.0 + 25.0 * weight)).abs() < 1e-9)
                }
                _ => panic!("Opacity must be a scalar"),
            }
        }
    }
    snapshot.time = -1.0;
    assert_eq!(
        evaluator
            .evaluate(&snapshot, &requested)
            .unwrap()
            .get(&requested[0]),
        Some(&PropertyValue::Vector2(positions[0]))
    );
    snapshot.time = 10.0;
    assert_eq!(
        evaluator
            .evaluate(&snapshot, &requested)
            .unwrap()
            .get(&requested[0]),
        Some(&PropertyValue::Vector2(positions[4]))
    );
    snapshot.time = authored.time;
    assert_eq!(
        snapshot, authored,
        "Evaluation never edits authored tracks/programs"
    );
}

#[test]
fn bounds_host_reads_dependency_depth_and_expression_count() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "thisComp.layer(2).opacity+thisComp.layer(2).opacity",
    );
    expression(&mut snapshot.sources, &mut snapshot.layers[1].opacity, "19");
    let requested = [address(1, ExpressionProperty::Opacity)];
    let mut evaluator = ExpressionEvaluator::default();
    evaluator.limits.max_host_reads = 2;
    assert_eq!(
        evaluator.evaluate(&snapshot, &requested).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );
    evaluator.limits = EvaluationLimits::default();
    evaluator.limits.max_dependency_depth = 1;
    assert_eq!(
        evaluator.evaluate(&snapshot, &requested).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );
    evaluator.limits = EvaluationLimits::default();
    evaluator.limits.max_expression_evaluations = 1;
    assert_eq!(
        evaluator.evaluate(&snapshot, &requested).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );
}

#[test]
fn separate_program_scopes_cannot_override_globals_or_host_functions() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "try { thisComp.layer.overwrite = 1; } catch(e) {} var privateValue=123; 7;",
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[1].opacity,
        "typeof privateValue === 'undefined' && !('overwrite' in thisComp.layer) ? 9 : 0;",
    );
    let keys = [
        address(1, ExpressionProperty::Opacity),
        address(2, ExpressionProperty::Opacity),
    ];
    let result = ExpressionEvaluator::default()
        .evaluate(&snapshot, &keys)
        .unwrap();
    assert_eq!(result.get(&keys[0]), Some(&PropertyValue::Scalar(7.0)));
    assert_eq!(result.get(&keys[1]), Some(&PropertyValue::Scalar(9.0)));
}

#[test]
fn syntax_failures_have_property_identity_and_source_limit_is_checked() {
    let mut snapshot = scene();
    let key = address(1, ExpressionProperty::Opacity);
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "if (",
    );
    let error = ExpressionEvaluator::default()
        .evaluate(&snapshot, std::slice::from_ref(&key))
        .unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::JavaScript);
    assert_eq!(error.property, Some(key.clone()));
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        &" ".repeat(65_537),
    );
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, &[key])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::Budget
    );
}

#[test]
fn hidden_iterator_prototypes_cannot_corrupt_host_iteration_or_results() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        r#"
const targets=[[][Symbol.iterator](),new Map().entries(),new Set().values(),''[Symbol.iterator](),''.matchAll(/./g),(function*(){})()];
for (const target of targets) {
  try { Object.getPrototypeOf(target).next=function(){ return {done:true}; }; } catch(e) {}
  try { Object.getPrototypeOf(Object.getPrototypeOf(target))[Symbol.iterator]=function(){return this;}; } catch(e) {}
}
7;
"#,
    );
    expression(&mut snapshot.sources, &mut snapshot.layers[1].opacity, "11");
    let requested = [
        address(1, ExpressionProperty::Opacity),
        address(2, ExpressionProperty::Opacity),
    ];
    let result = ExpressionEvaluator::default()
        .evaluate(&snapshot, &requested)
        .unwrap();
    assert_eq!(result.get(&requested[0]), Some(&PropertyValue::Scalar(7.0)));
    assert_eq!(
        result.get(&requested[1]),
        Some(&PropertyValue::Scalar(11.0))
    );
    assert_eq!(result.expression_evaluations, 2);
}

#[test]
fn caught_host_rejection_inside_returned_array_getter_still_fails() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].position,
        r#"
const result=[1,2];
Object.defineProperty(result,0,{get(){try {thisComp.layer('Absent')} catch(e) {} return 1;}});
result;
"#,
    );
    let error = ExpressionEvaluator::default()
        .evaluate(&snapshot, &[address(1, ExpressionProperty::Position)])
        .unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::MissingReference);
}

#[test]
fn hidden_async_and_host_function_prototypes_are_frozen_without_running_jobs() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        r#"
try {framesToTime.prototype.shared=123;} catch(e) {}
try {Object.getPrototypeOf(async function(){}).shared=123;} catch(e) {}
try {Object.getPrototypeOf(Object.getPrototypeOf((async function*(){})())).shared=123;} catch(e) {}
7;
"#,
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[1].opacity,
        r#"
(framesToTime.prototype.shared===undefined &&
Object.getPrototypeOf(async function(){}).shared===undefined &&
Object.getPrototypeOf(Object.getPrototypeOf((async function*(){})())).shared===undefined) ? 11 : 0;
"#,
    );
    let requested = [
        address(1, ExpressionProperty::Opacity),
        address(2, ExpressionProperty::Opacity),
    ];
    let result = ExpressionEvaluator::default()
        .evaluate(&snapshot, &requested)
        .unwrap();
    assert_eq!(
        result.get(&requested[1]),
        Some(&PropertyValue::Scalar(11.0))
    );
}

#[test]
fn symbol_keyed_intrinsic_state_cannot_leak_between_properties() {
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        r#"
try {Array.prototype[Symbol.unscopables].shared=123;} catch(e) {}
try {RegExp.prototype[Symbol.match].shared=123;} catch(e) {}
7;
"#,
    );
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[1].opacity,
        r#"
(Array.prototype[Symbol.unscopables].shared===undefined && RegExp.prototype[Symbol.match].shared===undefined) ? 13 : 0;
"#,
    );
    let requested = [
        address(1, ExpressionProperty::Opacity),
        address(2, ExpressionProperty::Opacity),
    ];
    let result = ExpressionEvaluator::default()
        .evaluate(&snapshot, &requested)
        .unwrap();
    assert_eq!(
        result.get(&requested[1]),
        Some(&PropertyValue::Scalar(13.0))
    );
}

#[test]
fn native_sparse_array_loops_are_rejected_before_they_can_block_interrupts() {
    // Deliberately do not execute the original native methods with these sizes.
    // The host replaces these entry points before any guest code can run.
    for method in [
        "concat",
        "join",
        "toLocaleString",
        "shift",
        "unshift",
        "reverse",
        "sort",
        "slice",
        "splice",
        "copyWithin",
        "flat",
        "flatMap",
        "fill",
        "toReversed",
        "toSorted",
        "toSpliced",
        "with",
    ] {
        let source = format!(
            "try {{ Array.prototype.{method}.call({{length:Number.MAX_SAFE_INTEGER}}); }} catch(e) {{}} 1;"
        );
        assert_eq!(
            opacity(&source).unwrap_err().kind,
            EvaluationErrorKind::Unsupported,
            "{method}"
        );
    }
    assert_eq!(opacity("try {Array.prototype.values.call({length:Number.MAX_SAFE_INTEGER}).drop(Number.MAX_SAFE_INTEGER).next();}catch(e){} 1;").unwrap_err().kind,EvaluationErrorKind::Unsupported);
}

#[test]
fn process_snapshot_and_result_json_roundtrip_preserve_typed_addresses() {
    let mut snapshot = five_state_scene();
    snapshot.id = CompositionId(u64::MAX);
    snapshot.layers[0].id = LayerId(u64::MAX);
    snapshot.layers[0].markers[0].comment = "Show\r\n표시".into();
    let roots = vec![PropertyAddress {
        composition: snapshot.id,
        layer: snapshot.layers[0].id,
        property: ExpressionProperty::Position,
    }];
    let request_json = serde_json::to_string(&(snapshot.clone(), roots.clone())).unwrap();
    let (received, received_roots): (CompositionSnapshot, Vec<PropertyAddress>) =
        serde_json::from_str(&request_json).unwrap();
    assert_eq!(received, snapshot);
    assert_eq!(received_roots, roots);
    let evaluated = ExpressionEvaluator::default()
        .evaluate(&received, &received_roots)
        .unwrap();
    let response_json = serde_json::to_string(&evaluated).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&response_json).unwrap();
    assert!(wire["values"].is_array());
    assert!(wire["dependencies"].is_array());
    assert_eq!(
        serde_json::from_str::<EvaluatedProperties>(&response_json).unwrap(),
        evaluated
    );
}

#[test]
fn process_json_rejects_duplicate_value_and_dependency_addresses() {
    let snapshot = scene();
    let evaluated = ExpressionEvaluator::default()
        .evaluate(&snapshot, &[address(1, ExpressionProperty::Opacity)])
        .unwrap();
    for field in ["values", "dependencies"] {
        let mut wire = serde_json::to_value(&evaluated).unwrap();
        let pairs = wire[field].as_array_mut().unwrap();
        pairs.push(pairs[0].clone());
        let error = serde_json::from_value::<EvaluatedProperties>(wire).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Duplicate expression property address")
        );
    }
}

#[test]
fn process_error_json_roundtrip_preserves_property_and_kind() {
    let error = EvaluationError {
        kind: EvaluationErrorKind::Cycle,
        message: "Synthetic cycle diagnostic".into(),
        property: Some(address(
            u64::MAX,
            ExpressionProperty::Slider("quoted \" control".into()),
        )),
    };
    let outcome: Result<EvaluatedProperties, EvaluationError> = Err(error);
    let wire = serde_json::to_string(&outcome).unwrap();
    assert_eq!(
        serde_json::from_str::<Result<EvaluatedProperties, EvaluationError>>(&wire).unwrap(),
        outcome
    );
}
