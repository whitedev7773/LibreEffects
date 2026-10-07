use super::*;

fn position(source: &str, three_d: bool) -> Result<PropertyValue, EvaluationError> {
    let mut snapshot = scene();
    if three_d {
        snapshot.layers[0].position.authored_value = PropertyValue::Vector3([0.0; 3]);
    }
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].position,
        source,
    );
    let key = address(1, ExpressionProperty::Position);
    Ok(ExpressionEvaluator::default()
        .evaluate(&snapshot, std::slice::from_ref(&key))?
        .values[&key]
        .clone())
}

#[test]
fn vector_helpers_pad_missing_axes_and_preserve_result_dimensions() {
    assert_eq!(
        position("add([10,20],[1,2,3])", true).unwrap(),
        PropertyValue::Vector3([11.0, 22.0, 3.0])
    );
    assert_eq!(
        position("sub([10,20],[1,2,3])", true).unwrap(),
        PropertyValue::Vector3([9.0, 18.0, -3.0])
    );
    assert_eq!(
        position("div(mul([3,-4],2),4)", false).unwrap(),
        PropertyValue::Vector2([1.5, -2.0])
    );
    assert_eq!(
        position("cross([1,0,0],[0,1,0])", true).unwrap(),
        PropertyValue::Vector3([0.0, 0.0, 1.0])
    );
    assert_eq!(
        opacity("dot([1,2,3],[4,5])").unwrap(),
        PropertyValue::Scalar(14.0)
    );
    assert_eq!(
        opacity("length([3,4])").unwrap(),
        PropertyValue::Scalar(5.0)
    );
    assert_eq!(
        opacity("length([2,3],[5,7])").unwrap(),
        PropertyValue::Scalar(5.0)
    );
}

#[test]
fn normalize_handles_extreme_and_subnormal_inputs_without_losing_direction() {
    for source in [
        "normalize([3,4])",
        "normalize([3e307,4e307])",
        "normalize([1.5e-323,2e-323])",
    ] {
        let PropertyValue::Vector2(value) = position(source, false).unwrap() else {
            panic!()
        };
        assert!((value[0] - 0.6).abs() < 1e-12, "{source}: {value:?}");
        assert!((value[1] - 0.8).abs() < 1e-12, "{source}: {value:?}");
    }
}

#[test]
fn scalar_and_vector_clamp_accept_reversed_limits_and_linear_accepts_three_arguments() {
    assert_eq!(
        opacity("clamp(120,100,0)").unwrap(),
        PropertyValue::Scalar(100.0)
    );
    assert_eq!(
        position("clamp([-2,50],[10,0],[0,40])", false).unwrap(),
        PropertyValue::Vector2([0.0, 40.0])
    );
    assert_eq!(
        opacity("linear(0.25,20,100)").unwrap(),
        PropertyValue::Scalar(40.0)
    );
    assert_eq!(
        position("linear(0.5,[0,10],[20,30])", false).unwrap(),
        PropertyValue::Vector2([10.0, 20.0])
    );
    assert_eq!(
        opacity("linear(-1,20,100)").unwrap(),
        PropertyValue::Scalar(20.0)
    );
    assert_eq!(
        opacity("linear(2,20,100)").unwrap(),
        PropertyValue::Scalar(100.0)
    );
}

#[test]
fn time_to_frames_uses_snapshot_clock_and_explicit_signed_rounding() {
    for (source, expected) in [
        ("timeToFrames()", 120.0),
        ("timeToFrames(0.15,10)", 1.0),
        ("timeToFrames(-0.15,10)", -2.0),
        ("timeToFrames(0.15,10,true)", 2.0),
        ("timeToFrames(-0.15,10,true)", -2.0),
        ("timeToFrames(framesToTime(12))", 12.0),
        ("radiansToDegrees(degreesToRadians(90))", 90.0),
    ] {
        assert_eq!(
            opacity(source).unwrap(),
            PropertyValue::Scalar(expected),
            "{source}"
        );
    }
}

#[test]
fn math_helpers_reject_coercion_sparse_arrays_overflow_and_caught_failures() {
    for source in [
        "add([],[]); 1",
        "add([1,,3],[1,2,3]); 1",
        "add(['1'],[2]); 1",
        "add(new Array(4294967295),[1]); 1",
        "add([1,2,3,4,5],[1]); 1",
        "dot([Infinity],[1])",
        "length([1e308,1e308,1e308,1e308])",
        "div([1,2],0); 1",
        "normalize([0,0]); 1",
        "cross([1,0],[0,1]); 1",
        "clamp([1,2],0,100); 1",
        "linear(1,2,3,4)",
        "degreesToRadians('90')",
        "timeToFrames(1,0)",
        "timeToFrames(NaN)",
        "timeToFrames(1,30,1)",
        "timeToFrames(Number.MAX_VALUE)",
        "try { div([1,2],0); } catch(e) {} 1",
        "const v=[1,2];Object.defineProperty(v,0,{get(){throw Error('getter ran')}});length(v)",
    ] {
        assert_eq!(
            opacity(source).unwrap_err().kind,
            EvaluationErrorKind::InvalidResult,
            "{source}"
        );
    }
}

#[test]
fn helper_functions_are_frozen_budgeted_and_reserved_from_explicit_locals() {
    for name in [
        "timeToFrames",
        "add",
        "sub",
        "mul",
        "div",
        "dot",
        "cross",
        "length",
        "normalize",
        "clamp",
        "degreesToRadians",
        "radiansToDegrees",
    ] {
        assert!(validate_local_bindings(&[name.into()]).is_err(), "{name}");
        assert_eq!(
            opacity(&format!("{name}.injected=1; 1")).unwrap_err().kind,
            EvaluationErrorKind::JavaScript
        );
    }
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        "try{for(var i=0;i<20;i++){length([3,4])}}catch(e){} 1",
    );
    let mut evaluator = ExpressionEvaluator::default();
    evaluator.limits.max_host_reads = 8;
    assert_eq!(
        evaluator
            .evaluate(&snapshot, &[address(1, ExpressionProperty::Opacity)])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::Budget
    );
}
