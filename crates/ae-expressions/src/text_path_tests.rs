use super::*;

fn text_scene(source: &str, locals: &[&str]) -> CompositionSnapshot {
    let mut snapshot = scene();
    let mut text = PropertySnapshot::authored(PropertyValue::Text("initial".into()));
    expression(&mut snapshot.sources, &mut text, source);
    text.expression.as_mut().unwrap().local_bindings =
        locals.iter().map(|name| (*name).into()).collect();
    snapshot.layers[0].source_text = Some(text);
    snapshot.layers[1].source_text =
        Some(PropertySnapshot::authored(PropertyValue::Text("17".into())));
    snapshot
}

fn evaluate_text(source: &str, locals: &[&str]) -> Result<PropertyValue, EvaluationError> {
    let snapshot = text_scene(source, locals);
    let key = address(1, ExpressionProperty::SourceText);
    let evaluated =
        ExpressionEvaluator::default().evaluate(&snapshot, std::slice::from_ref(&key))?;
    Ok(evaluated.values[&key].clone())
}

fn authored_path() -> ExpressionPath {
    ExpressionPath {
        vertices: vec![[0., 0.], [40., 0.], [40., 20.], [0., 20.]],
        in_tangents: vec![[0., 0.]; 4],
        out_tangents: vec![[0., 0.]; 4],
        closed: true,
    }
}

fn path_scene(source: &str) -> CompositionSnapshot {
    let mut snapshot = scene();
    let mut path = PropertySnapshot::authored(PropertyValue::Path(authored_path()));
    expression(&mut snapshot.sources, &mut path, source);
    snapshot.layers[0].masks.push(MaskSnapshot {
        id: u64::MAX,
        property: path,
    });
    snapshot
}

fn evaluate_path(source: &str) -> Result<PropertyValue, EvaluationError> {
    let snapshot = path_scene(source);
    let key = address(1, ExpressionProperty::MaskPath(u64::MAX));
    let evaluated =
        ExpressionEvaluator::default().evaluate(&snapshot, std::slice::from_ref(&key))?;
    Ok(evaluated.values[&key].clone())
}

#[test]
fn text_uses_lazy_dependencies_and_explicit_local_completion_values() {
    let source = "caption = thisComp.layer('Control').text.sourceText; count = parseInt(caption,10) + time; if(count > 10){'Count '+count;}else{'small';}";
    let snapshot = text_scene(source, &["caption", "count"]);
    let before = snapshot.clone();
    let key = address(1, ExpressionProperty::SourceText);
    let dependency = address(2, ExpressionProperty::SourceText);
    let evaluated = ExpressionEvaluator::default()
        .evaluate(&snapshot, std::slice::from_ref(&key))
        .unwrap();
    assert_eq!(
        evaluated.get(&key),
        Some(&PropertyValue::Text("Count 19".into()))
    );
    assert_eq!(evaluated.dependencies[&key], [dependency]);
    assert_eq!(evaluated.expression_evaluations, 1);
    assert_eq!(snapshot, before);
    assert_eq!(snapshot.sources[0], source);
}

#[test]
fn strict_default_still_rejects_undeclared_assignments() {
    assert_eq!(
        evaluate_text("caption = 'abc'; caption;", &[])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::JavaScript
    );
    assert_eq!(
        opacity("caption = 3; caption;").unwrap_err().kind,
        EvaluationErrorKind::JavaScript
    );
    assert_eq!(
        evaluate_text("caption = 'abc'; caption;", &["caption"]).unwrap(),
        PropertyValue::Text("abc".into())
    );
}

#[test]
fn locals_are_fresh_across_properties_dependencies_and_evaluations() {
    let source = "counter = (typeof counter === 'undefined' ? 0 : counter) + 1; if(thisLayer.index === 1){ other = thisComp.layer(2).text.sourceText; String(counter)+other; } else { String(counter); }";
    let mut snapshot = text_scene(source, &["counter", "other"]);
    snapshot.layers[1].source_text.as_mut().unwrap().expression = snapshot.layers[0]
        .source_text
        .as_ref()
        .unwrap()
        .expression
        .clone();
    let key = address(1, ExpressionProperty::SourceText);
    let evaluator = ExpressionEvaluator::default();
    for _ in 0..2 {
        let evaluated = evaluator
            .evaluate(&snapshot, std::slice::from_ref(&key))
            .unwrap();
        assert_eq!(evaluated.get(&key), Some(&PropertyValue::Text("11".into())));
        assert_eq!(evaluated.expression_evaluations, 2);
    }
}

#[test]
fn wrapper_identity_includes_local_bindings() {
    let source = "typeof optionalName;";
    let mut snapshot = text_scene(source, &["optionalName"]);
    expression(
        &mut snapshot.sources,
        snapshot.layers[1].source_text.as_mut().unwrap(),
        source,
    );
    let keys = [
        address(1, ExpressionProperty::SourceText),
        address(2, ExpressionProperty::SourceText),
    ];
    let evaluated = ExpressionEvaluator::default()
        .evaluate(&snapshot, &keys)
        .unwrap();
    assert_eq!(
        evaluated.get(&keys[0]),
        Some(&PropertyValue::Text("undefined".into()))
    );
    assert_eq!(
        evaluated.get(&keys[1]),
        Some(&PropertyValue::Text("undefined".into()))
    );
    // The assignment must fail for the same source when only one binding opts in.
    snapshot.sources[0] = "optionalName = 'set'; optionalName;".into();
    let error = ExpressionEvaluator::default()
        .evaluate(&snapshot, &keys)
        .unwrap_err();
    assert_eq!(error.kind, EvaluationErrorKind::JavaScript);
    assert_eq!(error.property, Some(keys[1].clone()));
}

#[test]
fn local_metadata_rejects_injection_shadowing_duplicates_and_oversize() {
    for bindings in [
        vec!["x;globalThis.bad=1".into()],
        vec!["eval".into()],
        vec!["arguments".into()],
        vec!["let".into()],
        vec!["thisComp".into()],
        vec!["linear".into()],
        vec!["createPath".into()],
        vec!["Math".into()],
        vec!["globalThis".into()],
        vec!["__proto__".into()],
        vec!["Window".into()],
        vec!["é".into()],
        vec!["".into()],
        vec!["1abc".into()],
        vec!["a".into(), "a".into()],
        vec!["x".repeat(65)],
        (0..65).map(|i| format!("local{i}")).collect(),
    ] {
        assert!(validate_local_bindings(&bindings).is_err(), "{bindings:?}");
        let mut snapshot = text_scene("'good'", &[]);
        let program = snapshot.layers[0]
            .source_text
            .as_mut()
            .unwrap()
            .expression
            .as_mut()
            .unwrap();
        program.local_bindings = bindings;
        program.enabled = false;
        assert_eq!(
            ExpressionEvaluator::default()
                .evaluate(&snapshot, &[])
                .unwrap_err()
                .kind,
            EvaluationErrorKind::InvalidSnapshot
        );
    }
    validate_local_bindings(&["_local".into(), "$local".into(), "x".repeat(64)]).unwrap();
}

#[test]
fn local_bindings_preserve_strict_functions_and_global_isolation() {
    assert_eq!(evaluate_text("scratch = 7; function f(x){return x+1;} var n=f(scratch); let suffix='!'; String(n)+suffix;", &["scratch"]).unwrap(), PropertyValue::Text("8!".into()));
    assert_eq!(
        evaluate_text(
            "scratch = 7; eval('scratch += 2'); String(scratch);",
            &["scratch"]
        )
        .unwrap(),
        PropertyValue::Text("9".into())
    );
    assert_eq!(
        evaluate_text(
            "scratch='private'; (0,eval)('typeof scratch');",
            &["scratch"]
        )
        .unwrap(),
        PropertyValue::Text("undefined".into())
    );
    assert_eq!(
        evaluate_text(
            "scratch='private'; Function('return typeof scratch')();",
            &["scratch"]
        )
        .unwrap(),
        PropertyValue::Text("undefined".into())
    );
    assert_eq!(
        evaluate_text(
            "String(this===undefined && Object.isFrozen(globalThis));",
            &[]
        )
        .unwrap(),
        PropertyValue::Text("true".into())
    );
}

#[test]
fn text_dependency_cycles_and_caught_host_failures_are_sticky() {
    let mut snapshot = text_scene(
        "try{thisComp.layer(2).text.sourceText}catch(e){} 'fallback';",
        &[],
    );
    expression(
        &mut snapshot.sources,
        snapshot.layers[1].source_text.as_mut().unwrap(),
        "thisComp.layer(1).text.sourceText;",
    );
    let key = address(1, ExpressionProperty::SourceText);
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, &[key])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::Cycle
    );
    assert_eq!(
        evaluate_text(
            "try{thisComp.layer('Absent').text.sourceText}catch(e){} 'fallback';",
            &[]
        )
        .unwrap_err()
        .kind,
        EvaluationErrorKind::MissingReference
    );
    assert_eq!(
        evaluate_text(
            "try{thisComp.layer(2).text.sourceText='edit'}catch(e){} 'fallback';",
            &[]
        )
        .unwrap_err()
        .kind,
        EvaluationErrorKind::Unsupported
    );
}

#[test]
fn text_requires_primitive_utf8_with_exact_byte_bound() {
    for source in [
        "new String('boxed')",
        "({toString(){return 'coerced'}})",
        "7",
        "['text']",
        "'\\uD800'",
        "'\\uDC00'",
        "'é'.repeat(8193)",
        "'x'.repeat(16385)",
    ] {
        assert_eq!(
            evaluate_text(source, &[]).unwrap_err().kind,
            EvaluationErrorKind::InvalidResult,
            "{source}"
        );
    }
    assert_eq!(
        evaluate_text("'é'.repeat(8192)", &[]).unwrap(),
        PropertyValue::Text("é".repeat(8192))
    );
    assert_eq!(
        evaluate_text("'😀'.repeat(4096)", &[]).unwrap(),
        PropertyValue::Text("😀".repeat(4096))
    );
    assert_eq!(
        evaluate_text("''", &[]).unwrap(),
        PropertyValue::Text(String::new())
    );
    assert_eq!(
        evaluate_text("'한글\\r\\n😀'", &[]).unwrap(),
        PropertyValue::Text("한글\r\n😀".into())
    );
}

#[test]
fn create_path_copies_coordinates_and_expands_empty_relative_handles() {
    let value = evaluate_path("const points=[[0,0],[40,0],[40,20],[0,20]]; const p=createPath(points,[],[],true); points[0][0]=999; p;").unwrap();
    assert_eq!(value, PropertyValue::Path(authored_path()));
    assert_eq!(
        evaluate_path("value;").unwrap(),
        PropertyValue::Path(authored_path())
    );
    let value =
        evaluate_path("createPath([[1,2],[3,4]],[[5,6],[7,8]],[[-1,-2],[-3,-4]],false)").unwrap();
    assert_eq!(
        value,
        PropertyValue::Path(ExpressionPath {
            vertices: vec![[1., 2.], [3., 4.]],
            in_tangents: vec![[5., 6.], [7., 8.]],
            out_tangents: vec![[-1., -2.], [-3., -4.]],
            closed: false
        })
    );
    assert!(evaluate_path("createPath([[0,0],[0,0],[0,0],[0,0]],[],[],true)").is_ok());
}

#[test]
fn path_results_cannot_be_forged_coerced_or_observed_through_getters() {
    for source in [
        "({vertices:[[0,0],[1,0],[1,1]],in_tangents:[],out_tangents:[],closed:true})",
        "new Proxy(value,{})",
        "({get vertices(){throw Error('getter')}})",
        "({toJSON(){return value}})",
        "'path'",
        "42",
    ] {
        assert_eq!(
            evaluate_path(source).unwrap_err().kind,
            EvaluationErrorKind::InvalidResult,
            "{source}"
        );
    }
    // A rejected getter is never called: its own failure would otherwise mask
    // the invalid-result diagnostic. Catching the helper rejection stays fatal.
    assert_eq!(evaluate_path("const points=[[0,0],[1,0],[1,1]]; Object.defineProperty(points,0,{get(){thisComp.layer('Absent');return [0,0]}}); try{createPath(points,[],[],true)}catch(e){} value;").unwrap_err().kind, EvaluationErrorKind::InvalidResult);
}

#[test]
fn create_path_bounds_and_no_coercion_are_enforced() {
    for source in [
        "createPath([[0,0]],[],[],false)",
        "createPath([[0,0],[1,1]],[],[],true)",
        "createPath([[0,0],[1,0],[1,1]],[[0,0]],[],true)",
        "createPath([[0,0],[1,0],[1,1]],[],[],'true')",
        "createPath([[0,0],[1000001,0],[1,1]],[],[],true)",
        "createPath([[0,0],[NaN,0],[1,1]],[],[],true)",
        "createPath([[0,0],['1',0],[1,1]],[],[],true)",
        "createPath(new Array(1025),[],[],true)",
        "createPath(new Array(4294967295),[],[],true)",
    ] {
        assert_eq!(
            evaluate_path(source).unwrap_err().kind,
            EvaluationErrorKind::InvalidResult,
            "{source}"
        );
    }
}

#[test]
fn path_proxy_descriptor_traps_cannot_hide_caught_host_failures() {
    let source = "const points=new Proxy([[0,0],[1,0],[1,1]],{getOwnPropertyDescriptor(target,key){try{thisComp.layer('Absent')}catch(e){}return Reflect.getOwnPropertyDescriptor(target,key)}}); try{createPath(points,[],[],true)}catch(e){} value;";
    assert_eq!(
        evaluate_path(source).unwrap_err().kind,
        EvaluationErrorKind::MissingReference
    );
}

#[test]
fn linear_clamps_scalar_and_vector_endpoints_without_coercion() {
    assert_eq!(
        opacity("linear(-10,0,10,2,8)").unwrap(),
        PropertyValue::Scalar(2.)
    );
    assert_eq!(
        opacity("linear(20,0,10,2,8)").unwrap(),
        PropertyValue::Scalar(8.)
    );
    assert_eq!(
        opacity("linear(5,0,10,2,8)").unwrap(),
        PropertyValue::Scalar(5.)
    );
    let mut snapshot = scene();
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].position,
        "linear(2,0,8,[0,8],[16,24])",
    );
    let key = address(1, ExpressionProperty::Position);
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, std::slice::from_ref(&key))
            .unwrap()
            .get(&key),
        Some(&PropertyValue::Vector2([4., 12.]))
    );
    for source in [
        "linear('5',0,10,2,8)",
        "linear(5,0,0,2,8)",
        "linear(5,10,0,2,8)",
        "linear(5,0,10,NaN,8)",
        "linear(5,0,10,[0,1],[2,3,4])",
        "linear(5,0,10,2,'8')",
    ] {
        assert_eq!(
            opacity(source).unwrap_err().kind,
            EvaluationErrorKind::InvalidResult,
            "{source}"
        );
    }
}

#[test]
fn snapshot_types_and_mask_identities_are_validated_before_any_expression() {
    let mut snapshot = path_scene("value");
    let duplicate = snapshot.layers[0].masks[0].clone();
    snapshot.layers[0].masks.push(duplicate);
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, &[])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::InvalidSnapshot
    );
    snapshot.layers[0].masks.pop();
    snapshot.layers[0].masks[0].property.authored_value = PropertyValue::Text("wrong".into());
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, &[])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::InvalidSnapshot
    );
    snapshot.layers[0].masks.clear();
    snapshot.layers[0].source_text = Some(PropertySnapshot::authored(PropertyValue::Scalar(1.)));
    assert_eq!(
        ExpressionEvaluator::default()
            .evaluate(&snapshot, &[])
            .unwrap_err()
            .kind,
        EvaluationErrorKind::InvalidSnapshot
    );
}

#[test]
fn numeric_wire_shape_remains_compatible_and_new_types_roundtrip() {
    let snapshot = scene();
    let json = serde_json::to_value(&snapshot).unwrap();
    assert!(json["layers"][0].get("source_text").is_none());
    assert!(json["layers"][0].get("masks").is_none());
    assert_eq!(json["layers"][0]["opacity"]["authored_value"], 75.);
    assert_eq!(
        serde_json::from_value::<CompositionSnapshot>(json).unwrap(),
        snapshot
    );
    for snapshot in [text_scene("'new'", &["scratch"]), path_scene("value")] {
        assert_eq!(
            serde_json::from_str::<CompositionSnapshot>(&serde_json::to_string(&snapshot).unwrap())
                .unwrap(),
            snapshot
        );
    }
}
