use super::*;

fn cached_scene(source: &str) -> CompositionSnapshot {
    let mut snapshot = scene();
    snapshot.layers[0].markers.push(MarkerSnapshot {
        time: 1.25,
        comment: "Synthetic cue".into(),
    });
    snapshot.layers[0].sliders.push(SliderSnapshot {
        name: "Amount".into(),
        property: PropertySnapshot::authored(PropertyValue::Scalar(12.0)),
    });
    expression(
        &mut snapshot.sources,
        &mut snapshot.layers[0].opacity,
        source,
    );
    snapshot
}

#[test]
fn cached_views_preserve_dependencies_and_each_read_charge() {
    let snapshot = cached_scene(
        "let sum=0; for(let i=0;i<4;i++){sum+=thisComp.layer('Caption').marker.key(1).time+effect('Amount')(1);} sum;",
    );
    let key = address(1, ExpressionProperty::Opacity);
    let result = ExpressionEvaluator::default()
        .evaluate(&snapshot, std::slice::from_ref(&key))
        .unwrap();
    assert_eq!(result.get(&key), Some(&PropertyValue::Scalar(53.0)));
    // Root + four (layer lookup, marker lookup, effect lookup, slider call,
    // slider sample). Cache hits still charge all observable host accesses.
    assert_eq!(result.host_reads, 21);
    assert_eq!(
        result.dependencies[&key],
        [address(1, ExpressionProperty::Slider("Amount".into()))]
    );
    let mut evaluator = ExpressionEvaluator::default();
    evaluator.limits.max_host_reads = 20;
    assert_eq!(
        evaluator.evaluate(&snapshot, &[key]).unwrap_err().kind,
        EvaluationErrorKind::Budget
    );
}

#[test]
fn cached_view_mutations_and_invalid_lookups_remain_sticky_failures() {
    for source in [
        "marker.key(1); try { marker.key(1).time=99; } catch(e) {} 10;",
        "effect('Amount')(1); try { effect('Amount').changed=99; } catch(e) {} 10;",
        "marker.key(1); try { marker.key(0); } catch(e) {} 10;",
        "effect('Amount')(1); try { effect('Amount')(2); } catch(e) {} 10;",
    ] {
        let snapshot = cached_scene(source);
        let error = ExpressionEvaluator::default()
            .evaluate(&snapshot, &[address(1, ExpressionProperty::Opacity)])
            .unwrap_err();
        assert!(
            matches!(
                error.kind,
                EvaluationErrorKind::Unsupported | EvaluationErrorKind::MissingReference
            ),
            "{source}: {error:?}"
        );
    }
}
