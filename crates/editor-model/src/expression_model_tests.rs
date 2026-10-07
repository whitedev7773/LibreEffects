//! Persisted-expression integration tests using only public, UI-free APIs.
use libre_effects_core::expression_runtime as ae;
use libre_effects_core::*;
use serde_json::{Value, json};

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    editor.clear_history();
    editor
}
fn raw(project: &Project) -> Value {
    serde_json::from_str(&project.to_json().unwrap()).unwrap()
}
fn layer(editor: &Editor) -> &Layer {
    editor.project().composition().layer(1).unwrap()
}
fn expr(id: LayerId, target: ExpressionTarget, source: &str) -> Command {
    Command::SetExpression {
        id,
        target,
        source: source.into(),
        enabled: true,
    }
}
fn set(editor: &mut Editor, id: LayerId, property: Property, frame: Frame, value: f64) {
    editor
        .execute(Command::SetValue {
            id,
            property,
            frame,
            value,
        })
        .unwrap();
}
fn slider(editor: &mut Editor, id: LayerId, name: &str, value: f64) -> EffectId {
    editor
        .execute(Command::Effect {
            id,
            edit: EffectEdit::Add(EffectKind::SliderControl),
        })
        .unwrap();
    let effect = editor
        .project()
        .composition()
        .layer(id)
        .unwrap()
        .effect_stack()
        .last()
        .unwrap()
        .id();
    editor
        .execute(Command::Effect {
            id,
            edit: EffectEdit::Rename {
                effect,
                name: name.into(),
            },
        })
        .unwrap();
    editor
        .execute(Command::Effect {
            id,
            edit: EffectEdit::SetValue {
                effect,
                parameter: EffectParam::Amount,
                frame: 0,
                value,
            },
        })
        .unwrap();
    effect
}
fn evaluate(
    project: &Project,
    frame: Frame,
) -> Result<ae::EvaluatedProperties, ae::EvaluationError> {
    let snapshot = project.expression_snapshot(1, frame).unwrap();
    let roots = project.expression_roots(1, frame, false).unwrap();
    ae::ExpressionEvaluator::default().evaluate(&snapshot, &roots)
}
fn reject(editor: &mut Editor, command: Command) {
    let before = editor.project().clone();
    let state = (
        editor.selected(),
        editor.can_undo(),
        editor.can_redo(),
        editor.context_generation(),
    );
    assert!(editor.execute(command).is_err());
    assert_eq!(editor.project(), &before);
    assert_eq!(
        (
            editor.selected(),
            editor.can_undo(),
            editor.can_redo(),
            editor.context_generation()
        ),
        state
    );
}

#[test]
fn sparse_schema_noop_disable_source_and_history_are_preserved() {
    let mut e = scene();
    let legacy = raw(e.project());
    assert_eq!(legacy["version"], 1);
    assert!(
        legacy["composition"]["layers"][0]
            .get("expressions")
            .is_none()
    );
    for command in [
        Command::RemoveExpression {
            id: 1,
            target: ExpressionTarget::Position,
        },
        Command::SetExpressionEnabled {
            id: 1,
            target: ExpressionTarget::Position,
            enabled: false,
        },
        expr(1, ExpressionTarget::Position, ""),
    ] {
        let generation = e.context_generation();
        e.execute(command).unwrap();
        assert_eq!(raw(e.project()), legacy);
        assert_eq!(e.context_generation(), generation);
        assert!(!e.can_undo());
    }
    let source = "  // Keep my spacing\n[value[0] + 12, value[1]];\n";
    e.execute(expr(1, ExpressionTarget::Position, source))
        .unwrap();
    let authored = e.project().clone();
    assert_eq!(raw(&authored)["version"], 65);
    e.execute(Command::SetExpressionEnabled {
        id: 1,
        target: ExpressionTarget::Position,
        enabled: false,
    })
    .unwrap();
    assert_eq!(
        layer(&e)
            .expression_for(ExpressionTarget::Position)
            .unwrap()
            .source,
        source
    );
    e.undo();
    assert_eq!(e.project(), &authored);
    assert!(e.can_redo());
    let generation = e.context_generation();
    e.execute(expr(1, ExpressionTarget::Position, source))
        .unwrap();
    assert_eq!(e.project(), &authored);
    assert!(e.can_redo());
    assert_eq!(e.context_generation(), generation);
    e.execute(Command::RemoveExpression {
        id: 1,
        target: ExpressionTarget::Opacity,
    })
    .unwrap();
    assert_eq!(e.project(), &authored);
    assert!(e.can_redo());
    assert_eq!(e.context_generation(), generation);
    e.redo();
    assert!(!layer(&e).has_enabled_expression(ExpressionTarget::Position));
    assert_eq!(
        Project::from_json(&e.project().to_json().unwrap()).unwrap(),
        *e.project()
    );
    let bytes = project_file::encode(e.project(), None).unwrap();
    assert_eq!(project_file::decode(&bytes).unwrap().project, *e.project());
}

#[test]
fn numeric_binding_validation_is_atomic_and_version_gated() {
    let mut e = scene();
    for source in ["x".repeat(MAX_EXPRESSION_SOURCE_BYTES + 1), "1\0".into()] {
        reject(&mut e, expr(1, ExpressionTarget::Opacity, &source));
    }
    reject(&mut e, expr(1, ExpressionTarget::Slider(999), "1"));
    reject(
        &mut e,
        Command::SetExpressionEnabled {
            id: 1,
            target: ExpressionTarget::Opacity,
            enabled: true,
        },
    );
    let max = format!("//{}", "x".repeat(MAX_EXPRESSION_SOURCE_BYTES - 2));
    e.execute(Command::SetExpression {
        id: 1,
        target: ExpressionTarget::Opacity,
        source: max,
        enabled: false,
    })
    .unwrap();
    let valid = raw(e.project());
    let mut old = valid.clone();
    old["version"] = json!(64);
    assert!(Project::from_json(&old.to_string()).is_err());
    let mut duplicate = valid.clone();
    let binding = duplicate["composition"]["layers"][0]["expressions"][0].clone();
    duplicate["composition"]["layers"][0]["expressions"]
        .as_array_mut()
        .unwrap()
        .push(binding);
    assert!(Project::from_json(&duplicate.to_string()).is_err());
    let mut invalid_target = valid;
    invalid_target["composition"]["layers"][0]["expressions"][0]["target"] = json!({"Slider": 9});
    assert!(Project::from_json(&invalid_target.to_string()).is_err());
}

#[test]
fn real_slider_effect_bounds_rename_duplicate_cleanup_and_noops() {
    let mut e = scene();
    let effect = slider(&mut e, 1, "Duration (FPS)", 32.0);
    assert_eq!(raw(e.project())["version"], 65);
    assert_eq!(
        layer(&e).effect_stack()[0].kind(),
        EffectKind::SliderControl
    );
    assert_eq!(EffectKind::SliderControl.parameters()[0].label, "Slider");
    let mut old = raw(e.project());
    old["version"] = json!(64);
    assert!(Project::from_json(&old.to_string()).is_err());
    for value in [f64::NAN, f64::INFINITY, 1_000_000.1, -1_000_000.1] {
        reject(
            &mut e,
            Command::Effect {
                id: 1,
                edit: EffectEdit::SetValue {
                    effect,
                    parameter: EffectParam::Amount,
                    frame: 0,
                    value,
                },
            },
        );
    }
    e.execute(expr(1, ExpressionTarget::Slider(effect), "value + 1"))
        .unwrap();
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Duplicate(effect),
    })
    .unwrap();
    let copy = layer(&e).effect_stack()[1].id();
    assert_eq!(
        layer(&e)
            .expression_for(ExpressionTarget::Slider(copy))
            .unwrap()
            .source,
        "value + 1"
    );
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Rename {
            effect: copy,
            name: "Second".into(),
        },
    })
    .unwrap();
    assert!(layer(&e).has_enabled_expression(ExpressionTarget::Slider(copy)));
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Remove(effect),
    })
    .unwrap();
    assert!(
        layer(&e)
            .expression_for(ExpressionTarget::Slider(effect))
            .is_none()
    );
    assert!(layer(&e).has_enabled_expression(ExpressionTarget::Slider(copy)));
    e.undo();
    let before = e.project().clone();
    let generation = e.context_generation();
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::SetValue {
            effect,
            parameter: EffectParam::Amount,
            frame: 0,
            value: 32.0,
        },
    })
    .unwrap();
    assert_eq!(e.project(), &before);
    assert_eq!(e.context_generation(), generation);
    assert!(e.can_redo());
}

#[test]
fn null_controls_are_real_effects_and_duplicate_names_resolve_first_in_stack() {
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Null,
        width: 10.0,
        height: 10.0,
        name: "Control".into(),
    })
    .unwrap();
    let first = slider(&mut e, 1, "Knob", 4.0);
    let second = slider(&mut e, 1, "Knob", 17.0);
    e.execute(expr(
        1,
        ExpressionTarget::Slider(second),
        "missingReference",
    ))
    .unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(expr(
        2,
        ExpressionTarget::Opacity,
        "thisComp.layer('Control').effect('Knob')('Slider')",
    ))
    .unwrap();
    let snapshot = e.project().expression_snapshot(1, 0).unwrap();
    assert_eq!(snapshot.layers[1].sliders.len(), 1);
    let results = evaluate(e.project(), 0).unwrap();
    assert_eq!(results.expression_evaluations, 1);
    let view = e
        .project()
        .with_evaluated_properties(1, 0, false, &results)
        .unwrap();
    assert_eq!(
        view.composition()
            .layer(2)
            .unwrap()
            .property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(0),
        4.0
    );
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Move {
            effect: second,
            index: 0,
        },
    })
    .unwrap();
    assert!(evaluate(e.project(), 0).is_err());
    e.execute(Command::RemoveExpression {
        id: 1,
        target: ExpressionTarget::Slider(second),
    })
    .unwrap();
    assert_eq!(
        e.project().expression_snapshot(1, 0).unwrap().layers[1].sliders[0]
            .property
            .authored_value,
        ae::PropertyValue::Scalar(17.0)
    );
    assert_eq!(layer(&e).effect_stack()[1].id(), first);
}

#[test]
fn snapshot_samples_exact_rational_frame_and_independent_origin_and_sorted_markers() {
    let mut e = scene();
    e.execute(Command::ConfigureCompositionRate {
        name: "Fractional".into(),
        width: 1200,
        height: 800,
        fps: FrameRate::new(30_000, 1001).unwrap(),
        duration: 300,
        display_start: 120,
    })
    .unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 40,
        end: 250,
    })
    .unwrap();
    e.execute(Command::SetLayerStart { id: 1, frame: -20 })
        .unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 40,
        end: 250,
    })
    .unwrap();
    set(&mut e, 1, Property::PositionX, 0, 10.0);
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::PositionX,
        frame: 0,
    })
    .unwrap();
    set(&mut e, 1, Property::PositionX, 100, 110.0);
    let knob = slider(&mut e, 1, "Clock", 0.0);
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::ToggleAnimation {
            effect: knob,
            parameter: EffectParam::Amount,
            frame: 0,
        },
    })
    .unwrap();
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::SetValue {
            effect: knob,
            parameter: EffectParam::Amount,
            frame: 100,
            value: 100.0,
        },
    })
    .unwrap();
    for (index, frame) in [100, 20].into_iter().enumerate() {
        e.execute(Command::Marker {
            target: MarkerTarget::Layer(1),
            edit: MarkerEdit::Add { frame },
        })
        .unwrap();
        e.execute(Command::Marker {
            target: MarkerTarget::Layer(1),
            edit: MarkerEdit::Update {
                id: index as u64 + 1,
                frame,
                duration: 0,
                name: format!("m{frame}"),
                color: 0,
            },
        })
        .unwrap();
    }
    let snapshot = e.project().expression_snapshot(1, 50).unwrap();
    let l = &snapshot.layers[0];
    assert_eq!(snapshot.id, ae::CompositionId(1));
    assert_eq!(l.id, ae::LayerId(1));
    assert_eq!(
        snapshot.frame_rate,
        ae::FrameRate {
            numerator: 30_000,
            denominator: 1001
        }
    );
    assert_eq!(snapshot.time, 50.0 * 1001.0 / 30_000.0);
    assert_eq!(l.start_time, -20.0 * 1001.0 / 30_000.0);
    assert_eq!(l.in_point, 40.0 * 1001.0 / 30_000.0);
    assert_eq!(l.out_point, 250.0 * 1001.0 / 30_000.0);
    assert_eq!(
        l.position.authored_value,
        ae::PropertyValue::Vector2([60.0, 540.0])
    );
    assert_eq!(
        l.sliders[0].property.authored_value,
        ae::PropertyValue::Scalar(50.0)
    );
    assert_eq!(
        l.markers
            .iter()
            .map(|m| m.comment.as_str())
            .collect::<Vec<_>>(),
        ["m20", "m100"]
    );
    assert!(e.project().expression_snapshot(1, 300).is_err());
}

#[test]
fn source_duplicate_and_timing_changes_preserve_programs_and_authored_track_contract() {
    let mut e = scene();
    e.execute(expr(1, ExpressionTarget::Opacity, "value / 2"))
        .unwrap();
    set(&mut e, 1, Property::Opacity, 0, 60.0);
    assert_eq!(
        layer(&e)
            .property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(0),
        60.0
    );
    let source = layer(&e).expressions().to_vec();
    e.execute(Command::DuplicateLayer(1)).unwrap();
    e.execute(Command::SetLayerRange {
        id: 2,
        start: 20,
        end: 100,
    })
    .unwrap();
    e.execute(Command::ShiftLayer { id: 2, delta: 5 }).unwrap();
    assert_eq!(
        e.project().composition().layer(2).unwrap().expressions(),
        source
    );
    e.execute(Command::DuplicateComposition).unwrap();
    for l in e.project().composition().layers() {
        assert_eq!(l.expressions(), source);
    }
    assert_eq!(
        e.project()
            .composition_by_id(1)
            .unwrap()
            .layer(1)
            .unwrap()
            .expressions(),
        source
    );
}

#[test]
fn detached_results_clear_only_target_keys_never_persist_and_reject_stale_or_bad_views() {
    let mut e = scene();
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::Opacity,
        frame: 0,
    })
    .unwrap();
    set(&mut e, 1, Property::Opacity, 60, 40.0);
    e.execute(expr(1, ExpressionTarget::Opacity, "value / 2"))
        .unwrap();
    let before = e.project().clone();
    let evaluated = evaluate(&before, 30).unwrap();
    let key = evaluated
        .values
        .keys()
        .find(|key| key.property == ae::ExpressionProperty::Opacity)
        .unwrap()
        .clone();
    let view = before
        .with_evaluated_properties(1, 30, false, &evaluated)
        .unwrap();
    assert_eq!(view.evaluated_frame(), Some((1, 30)));
    let l = view.composition().layer(1).unwrap();
    assert_eq!(
        l.property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(30),
        35.0
    );
    assert!(
        l.property(Property::Opacity)
            .expect("known scalar fixture property")
            .keys()
            .is_empty()
    );
    assert_eq!(l.expressions(), layer(&e).expressions());
    assert_eq!(e.project(), &before);
    assert!(view.to_json().is_err());
    assert!(serde_json::to_string(&view).is_err());
    assert!(project_file::encode(&view, None).is_err());
    assert!(e.commit_automation_project(view).is_err());
    assert!(
        before
            .with_evaluated_properties(1, 31, false, &evaluated)
            .is_err()
    );
    // Evaluated Opacity is raw; finite overshoot clamps only while painting.
    for value in [-1.0, 101.0] {
        let mut raw = evaluated.clone();
        raw.values
            .insert(key.clone(), ae::PropertyValue::Scalar(value));
        let projected = before
            .with_evaluated_properties(1, 30, false, &raw)
            .unwrap();
        assert_eq!(
            projected
                .composition()
                .layer(1)
                .unwrap()
                .opacity_at(30, 1.0 / 30.0)
                .unwrap(),
            value
        );
        assert!(projected.to_json().is_err());
    }
    for value in [
        ae::PropertyValue::Scalar(f64::NAN),
        ae::PropertyValue::Scalar(f64::INFINITY),
        ae::PropertyValue::Vector2([1.0, 2.0]),
    ] {
        let mut bad = evaluated.clone();
        bad.values.insert(key.clone(), value);
        assert!(
            before
                .with_evaluated_properties(1, 30, false, &bad)
                .is_err()
        );
    }
    let mut missing = evaluated.clone();
    missing.values.remove(&key);
    assert!(
        before
            .with_evaluated_properties(1, 30, false, &missing)
            .is_err()
    );
    let mut cycle = evaluated.clone();
    cycle.dependencies.insert(key.clone(), vec![key]);
    assert!(
        before
            .with_evaluated_properties(1, 30, false, &cycle)
            .is_err()
    );
}

#[test]
fn hidden_templates_are_lazy_dependencies_and_cycles_or_errors_never_fallback() {
    let mut e = scene();
    e.execute(Command::RenameLayer {
        id: 1,
        name: "Template".into(),
    })
    .unwrap();
    e.execute(expr(
        1,
        ExpressionTarget::Position,
        "thisLayer.marker.key('Missing').time",
    ))
    .unwrap();
    e.execute(Command::ToggleVisible(1)).unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(expr(2, ExpressionTarget::Opacity, "42")).unwrap();
    assert_eq!(e.project().expression_roots(1, 0, false).unwrap().len(), 1);
    assert_eq!(evaluate(e.project(), 0).unwrap().expression_evaluations, 1);
    e.execute(expr(
        2,
        ExpressionTarget::Position,
        "thisComp.layer('Template').position",
    ))
    .unwrap();
    assert!(evaluate(e.project(), 0).is_err());
    e.execute(expr(
        1,
        ExpressionTarget::Position,
        "thisComp.layer(1).position",
    ))
    .unwrap();
    assert_eq!(
        evaluate(e.project(), 0).unwrap_err().kind,
        ae::EvaluationErrorKind::Cycle
    );
    e.execute(Command::SetExpressionEnabled {
        id: 1,
        target: ExpressionTarget::Position,
        enabled: false,
    })
    .unwrap();
    assert!(evaluate(e.project(), 0).is_ok());
}

#[test]
fn render_roots_include_hidden_parents_and_mattes_but_exclude_inactive_and_guides() {
    let mut e = scene();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetParent {
        id: 2,
        parent: Some(1),
        frame: 0,
    })
    .unwrap();
    e.execute(Command::ToggleVisible(1)).unwrap();
    e.execute(expr(1, ExpressionTarget::Position, "[10,20]"))
        .unwrap();
    e.execute(expr(1, ExpressionTarget::Opacity, "missingOpacity"))
        .unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetTrackMatte {
        id: 2,
        matte: Some(TrackMatte {
            source: 3,
            mode: MatteMode::Alpha,
        }),
    })
    .unwrap();
    e.execute(expr(3, ExpressionTarget::Opacity, "50")).unwrap();
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetLayerRange {
        id: 4,
        start: 40,
        end: 100,
    })
    .unwrap();
    e.execute(expr(4, ExpressionTarget::Opacity, "missingInactive"))
        .unwrap();
    let roots = e.project().expression_roots(1, 0, false).unwrap();
    assert_eq!(roots.len(), 2);
    assert!(
        roots
            .iter()
            .any(|key| key.layer == ae::LayerId(1)
                && key.property == ae::ExpressionProperty::Position)
    );
    assert!(
        roots
            .iter()
            .any(|key| key.layer == ae::LayerId(3)
                && key.property == ae::ExpressionProperty::Opacity)
    );
    assert!(evaluate(e.project(), 0).is_ok());
}

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

#[test]
fn six_synthetic_lyric_programs_flow_through_persisted_snapshot_and_detached_view() {
    let mut e = scene();
    e.execute(Command::ConfigureCompositionRate {
        name: "Lyrics".into(),
        width: 1200,
        height: 800,
        fps: 60.into(),
        duration: 720,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::RenameLayer {
        id: 1,
        name: "Caption".into(),
    })
    .unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 1,
        end: 700,
    })
    .unwrap();
    e.execute(Command::SetLayerStart { id: 1, frame: 1 })
        .unwrap();
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 1,
        end: 700,
    })
    .unwrap();
    for name in [
        "Gap", "Bezier", "Duration", "Dormant", "Arrive", "Reading", "Depart", "Gone",
    ] {
        e.execute(Command::AddRectangle).unwrap();
        let id = e.selected().unwrap();
        e.execute(Command::RenameLayer {
            id,
            name: name.into(),
        })
        .unwrap();
        e.execute(Command::ToggleVisible(id)).unwrap();
    }
    set(&mut e, 2, Property::PositionY, 0, 90.8);
    for (name, value) in [
        ("Start Pont", 0.0),
        ("Y1", 0.95),
        ("Y2", 0.9),
        ("End Point", 1.0),
    ] {
        slider(&mut e, 3, name, value);
    }
    slider(&mut e, 4, "Duration (FPS)", 32.0);
    for (index, id) in (5..=9).enumerate() {
        set(
            &mut e,
            id,
            Property::PositionX,
            0,
            100.0 + index as f64 * 250.0,
        );
        set(
            &mut e,
            id,
            Property::PositionY,
            0,
            500.0 - index as f64 * 50.0,
        );
        set(&mut e, id, Property::ScaleX, 0, 20.0 + index as f64 * 20.0);
        set(&mut e, id, Property::ScaleY, 0, 30.0 + index as f64 * 15.0);
        set(&mut e, id, Property::Opacity, 0, index as f64 * 25.0);
    }
    for (index, (frame, name)) in [(0, "Show"), (120, "Focus"), (240, "Hide"), (360, "End")]
        .into_iter()
        .enumerate()
    {
        e.execute(Command::Marker {
            target: MarkerTarget::Layer(1),
            edit: MarkerEdit::Add { frame },
        })
        .unwrap();
        e.execute(Command::Marker {
            target: MarkerTarget::Layer(1),
            edit: MarkerEdit::Update {
                id: index as u64 + 1,
                frame,
                duration: 0,
                name: name.into(),
                color: 0,
            },
        })
        .unwrap();
    }
    e.execute(expr(6, ExpressionTarget::Position, "const dy = parseInt(thisComp.layer('Gap').transform.position[1],10); [thisComp.width*0.5, thisComp.height*0.5+dy]")).unwrap();
    e.execute(expr(
        7,
        ExpressionTarget::Position,
        "[thisComp.width*0.5, thisComp.height*0.5]",
    ))
    .unwrap();
    e.execute(expr(8, ExpressionTarget::Position, "const dy = parseInt(thisComp.layer('Gap').transform.position[1],10); [thisComp.width*0.5, thisComp.height*0.5-dy]")).unwrap();
    for (target, property) in [
        (ExpressionTarget::Position, "position"),
        (ExpressionTarget::Scale, "scale"),
        (ExpressionTarget::Opacity, "opacity"),
    ] {
        e.execute(expr(1, target, &staged_program(property)))
            .unwrap();
    }
    let authored = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    assert_eq!(
        authored
            .composition()
            .layers()
            .iter()
            .map(|layer| layer.expressions().len())
            .sum::<usize>(),
        6
    );
    for frame in [
        1, 17, 33, 119, 120, 136, 152, 239, 240, 256, 272, 359, 360, 376, 392, 419,
    ] {
        let values = evaluate(&authored, frame).unwrap();
        assert_eq!(values.expression_evaluations, 6);
        let view = authored
            .with_evaluated_properties(1, frame, false, &values)
            .unwrap();
        let caption = view.composition().layer(1).unwrap();
        if let Some((position, scale, opacity)) = match frame {
            1 => Some(([100.0, 500.0], [20.0, 30.0], 0.0)),
            33 | 119 | 120 => Some(([600.0, 490.0], [40.0, 45.0], 25.0)),
            152 | 239 | 240 => Some(([600.0, 400.0], [60.0, 60.0], 50.0)),
            272 | 359 | 360 => Some(([600.0, 310.0], [80.0, 75.0], 75.0)),
            392 | 419 => Some(([1100.0, 300.0], [100.0, 90.0], 100.0)),
            _ => None,
        } {
            for (property, expected) in [
                (Property::PositionX, position[0]),
                (Property::PositionY, position[1]),
                (Property::ScaleX, scale[0]),
                (Property::ScaleY, scale[1]),
                (Property::Opacity, opacity),
            ] {
                assert!(
                    (caption
                        .property(property)
                        .expect("known scalar fixture property")
                        .value_at(frame)
                        - expected)
                        .abs()
                        < 1e-8,
                    "{frame} {property:?}"
                );
            }
        }
        assert_eq!(authored, *e.project());
    }
}

#[test]
fn inactive_composition_expression_edits_preserve_active_context_and_invalid_source_cannot_repair()
{
    let mut e = scene();
    e.execute(Command::NewComposition).unwrap();
    let active = e.project().active_composition_id();
    let mut draft = e.project().clone();
    draft
        .apply_automation_command(1, expr(1, ExpressionTarget::Opacity, "25"))
        .unwrap();
    assert_eq!(draft.active_composition_id(), active);
    assert!(draft.composition().layers().is_empty());
    assert_eq!(raw(&draft)["version"], 65);
    let snapshot = draft.expression_snapshot(1, 30).unwrap();
    let roots = draft.expression_roots(1, 30, false).unwrap();
    let evaluated = ae::ExpressionEvaluator::default()
        .evaluate(&snapshot, &roots)
        .unwrap();
    let view = draft
        .with_evaluated_properties(1, 30, false, &evaluated)
        .unwrap();
    assert_eq!(view.active_composition_id(), active);
    assert_eq!(
        view.composition_by_id(1)
            .unwrap()
            .layer(1)
            .unwrap()
            .property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(30),
        25.0
    );
    assert_eq!(
        draft
            .composition_by_id(1)
            .unwrap()
            .layer(1)
            .unwrap()
            .property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(30),
        100.0
    );
    let mut invalid = raw(&draft);
    invalid["version"] = json!(64);
    let mut invalid: Project = serde_json::from_value(invalid).unwrap();
    assert!(
        invalid
            .apply_automation_command(1, expr(1, ExpressionTarget::Opacity, "25"))
            .is_err()
    );
    assert!(
        invalid
            .apply_automation_command(
                1,
                Command::RemoveExpression {
                    id: 1,
                    target: ExpressionTarget::Opacity
                }
            )
            .is_err()
    );
}

#[test]
fn guide_and_solo_root_switches_and_trimmed_mattes_avoid_unrelated_programs() {
    let mut e = scene();
    e.execute(expr(1, ExpressionTarget::Opacity, "42")).unwrap();
    e.execute(Command::SetLayerSwitch {
        id: 1,
        switch: LayerSwitch::Guide,
        enabled: true,
    })
    .unwrap();
    assert!(
        e.project()
            .expression_roots(1, 0, false)
            .unwrap()
            .is_empty()
    );
    assert_eq!(e.project().expression_roots(1, 0, true).unwrap().len(), 1);
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetLayerSwitch {
        id: 2,
        switch: LayerSwitch::Solo,
        enabled: true,
    })
    .unwrap();
    assert!(e.project().expression_roots(1, 0, true).unwrap().is_empty());
    e.execute(Command::SetTrackMatte {
        id: 2,
        matte: Some(TrackMatte {
            source: 1,
            mode: MatteMode::Alpha,
        }),
    })
    .unwrap();
    assert_eq!(e.project().expression_roots(1, 0, false).unwrap().len(), 1);
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 10,
        end: 100,
    })
    .unwrap();
    assert!(
        e.project()
            .expression_roots(1, 0, false)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn equal_animated_slider_sample_is_not_an_implicit_key_and_keeps_redo() {
    let mut e = scene();
    let effect = slider(&mut e, 1, "Animated", -10.0);
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::ToggleAnimation {
            effect,
            parameter: EffectParam::Amount,
            frame: 0,
        },
    })
    .unwrap();
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::SetValue {
            effect,
            parameter: EffectParam::Amount,
            frame: 20,
            value: 10.0,
        },
    })
    .unwrap();
    let before = e.project().clone();
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Rename {
            effect,
            name: "Later".into(),
        },
    })
    .unwrap();
    e.undo();
    let generation = e.context_generation();
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::SetValue {
            effect,
            parameter: EffectParam::Amount,
            frame: 10,
            value: 0.0,
        },
    })
    .unwrap();
    assert_eq!(e.project(), &before);
    assert_eq!(e.context_generation(), generation);
    assert!(e.can_redo());
    assert_eq!(
        layer(&e).effect_stack()[0]
            .parameter(EffectParam::Amount)
            .unwrap()
            .keys()
            .len(),
        2
    );
    reject(
        &mut e,
        Command::Effect {
            id: 1,
            edit: EffectEdit::SetValue {
                effect,
                parameter: EffectParam::Amount,
                frame: 150,
                value: 10.0,
            },
        },
    );
}

#[test]
fn visible_null_preview_controls_use_evaluated_transforms_but_exports_and_hidden_nulls_stay_lazy() {
    let mut e = Editor::default();
    for name in ["Visible control", "Hidden template"] {
        e.execute(Command::AddContent {
            content: Content::Null,
            width: 10.0,
            height: 10.0,
            name: name.into(),
        })
        .unwrap();
    }
    e.execute(expr(1, ExpressionTarget::Position, "[100,200]"))
        .unwrap();
    e.execute(expr(1, ExpressionTarget::Scale, "[50,75]"))
        .unwrap();
    e.execute(expr(
        1,
        ExpressionTarget::Opacity,
        "thisLayer.marker.key('Missing').time",
    ))
    .unwrap();
    e.execute(expr(
        2,
        ExpressionTarget::Position,
        "thisLayer.marker.key('Missing').time",
    ))
    .unwrap();
    e.execute(Command::ToggleVisible(2)).unwrap();
    assert!(
        e.project()
            .expression_roots(1, 0, false)
            .unwrap()
            .is_empty()
    );
    let roots = e.project().expression_roots(1, 0, true).unwrap();
    assert_eq!(roots.len(), 2);
    assert!(
        roots
            .iter()
            .all(|key| key.layer == ae::LayerId(1)
                && key.property != ae::ExpressionProperty::Opacity)
    );
    let snapshot = e.project().expression_snapshot(1, 0).unwrap();
    let values = ae::ExpressionEvaluator::default()
        .evaluate(&snapshot, &roots)
        .unwrap();
    assert_eq!(values.expression_evaluations, 2);
    let view = e
        .project()
        .with_evaluated_properties(1, 0, true, &values)
        .unwrap();
    let null = view.composition().layer(1).unwrap();
    assert_eq!(
        null.property(Property::PositionX)
            .expect("known scalar fixture property")
            .value_at(0),
        100.0
    );
    assert_eq!(
        null.property(Property::PositionY)
            .expect("known scalar fixture property")
            .value_at(0),
        200.0
    );
    assert_eq!(
        null.property(Property::ScaleX)
            .expect("known scalar fixture property")
            .value_at(0),
        50.0
    );
    assert_eq!(
        null.property(Property::ScaleY)
            .expect("known scalar fixture property")
            .value_at(0),
        75.0
    );
    assert_eq!(
        null.property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(0),
        100.0
    );
    assert_eq!(
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .property(Property::PositionX)
            .expect("known scalar fixture property")
            .value_at(0),
        960.0
    );
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 10,
        end: 100,
    })
    .unwrap();
    assert!(e.project().expression_roots(1, 0, true).unwrap().is_empty());
}

#[test]
fn null_preview_roots_include_hidden_ancestors_and_expression_transform_guard_fails_closed() {
    let mut e = Editor::default();
    for name in ["Parent", "Control"] {
        e.execute(Command::AddContent {
            content: Content::Null,
            width: 10.0,
            height: 10.0,
            name: name.into(),
        })
        .unwrap();
    }
    e.execute(Command::SetParent {
        id: 2,
        parent: Some(1),
        frame: 0,
    })
    .unwrap();
    e.execute(Command::ToggleVisible(1)).unwrap();
    assert!(!e.project().composition().has_expression_transform(2));
    e.execute(expr(1, ExpressionTarget::Position, "[200,300]"))
        .unwrap();
    assert!(e.project().composition().has_expression_transform(2));
    let roots = e.project().expression_roots(1, 0, true).unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].layer, ae::LayerId(1));
    assert_eq!(roots[0].property, ae::ExpressionProperty::Position);
    assert!(
        e.project()
            .expression_roots(1, 0, false)
            .unwrap()
            .is_empty()
    );
    e.execute(Command::SetExpressionEnabled {
        id: 1,
        target: ExpressionTarget::Position,
        enabled: false,
    })
    .unwrap();
    e.execute(expr(1, ExpressionTarget::Opacity, "50")).unwrap();
    assert!(!e.project().composition().has_expression_transform(2));
    assert!(e.project().composition().has_expression_transform(999));
    let mut invalid = raw(e.project());
    invalid["composition"]["layers"][0]["parent"] = json!(999);
    let missing: Project = serde_json::from_value(invalid).unwrap();
    assert!(missing.composition().has_expression_transform(2));
    let mut invalid = raw(e.project());
    invalid["composition"]["layers"][1]["parent"] = json!(2);
    let cyclic: Project = serde_json::from_value(invalid).unwrap();
    assert!(cyclic.composition().has_expression_transform(2));
}
