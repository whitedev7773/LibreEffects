//! Real JSX/ScriptUI-facing scalar restoration, independently authored.
use crate::{
    automation::{ScriptOutcome, run_script},
    opacity_model_tests::{excursion, scene},
};
use libre_effects_core::*;
use std::sync::{Arc, atomic::AtomicBool, mpsc};
fn run(project: Project, frame: Frame, source: &str) -> Result<ScriptOutcome, String> {
    let (tx, _rx) = mpsc::channel();
    let (_tx, rx) = mpsc::channel();
    run_script(
        project,
        vec![1],
        frame,
        source,
        tx,
        rx,
        Arc::new(AtomicBool::new(false)),
    )
}
const RESTORE: &str = r#"
var l=app.project.activeItem.layer(1),p=l.transform.opacity;
if(p.propertyValueType!==PropertyValueType.OneD)throw Error('wrong type');
app.beginUndoGroup('Independent scalar sequence');
var times=[0,1,3,4],values=[0,100,100,0];
for(var i=0;i<4;i++)p.setValueAtTime(times[i],values[i]);
for(var k=1;k<=4;k++){
 p.setInterpolationTypeAtKey(k,KeyframeInterpolationType.BEZIER,KeyframeInterpolationType.BEZIER);
 p.setTemporalEaseAtKey(k,[new KeyframeEase(k===1?-5e-324:k===2?-7e-6:k===3?-3e-7:0,65)],[new KeyframeEase(k===4?-1e-199:0,35)]);
 p.setTemporalContinuousAtKey(k,false);p.setTemporalAutoBezierAtKey(k,false);
}
if(p.numKeys!==4||p.keyTime(3)!==3||p.keyValue(3)!==100||p.nearestKeyIndex(2)!==2)throw Error('key identity');
l.name='Opacity 검증';app.endUndoGroup();
"#;

#[test]
fn jsx_reads_native_opacity_key_sides_and_exact_signed_ease_without_editing() {
    let project = run(scene().project().clone(), 0, RESTORE).unwrap().project;
    let result = run(project.clone(), 0, r#"
        var p=app.project.activeItem.layer(1).transform.opacity;
        if(p.keyInInterpolationType(1)!==KeyframeInterpolationType.BEZIER || p.keyOutInterpolationType(4)!==KeyframeInterpolationType.BEZIER) throw Error('interpolation');
        var first=p.keyInTemporalEase(1), last=p.keyOutTemporalEase(4);
        if(first.length!==1 || !(first[0] instanceof KeyframeEase) || first[0].speed!==-5e-324 || first[0].influence!==65) throw Error('incoming ease');
        if(last.length!==1 || last[0].speed!==-1e-199 || last[0].influence!==35) throw Error('outgoing ease');
        if(p.keyTemporalContinuous(2)!==false || p.keyTemporalAutoBezier(2)!==false) throw Error('flags');
        first[0].speed=999;
        if(p.keyInTemporalEase(1)[0].speed!==-5e-324) throw Error('read alias mutated source');
    "#).unwrap();
    assert_eq!(result.project, project);
}

#[test]
fn jsx_key_metadata_rejects_bad_indexes_legacy_timing_and_spatial_scalar_reads() {
    let project = run(scene().project().clone(), 0, RESTORE).unwrap().project;
    for source in [
        "var p=app.project.activeItem.layer(1).transform.opacity;try{p.keyInTemporalEase(0)}catch(e){}",
        "var p=app.project.activeItem.layer(1).transform.opacity;try{p.keyOutInterpolationType(5)}catch(e){}",
        "var p=app.project.activeItem.layer(1).transform.opacity;try{p.keyInTemporalEase(1.5)}catch(e){}",
        "var p=app.project.activeItem.layer(1).transform.opacity;try{p.keyInSpatialTangent(1)}catch(e){}",
    ] {
        assert!(run(project.clone(), 0, source).is_err(), "{source}");
    }
    assert!(
        run(
            scene().project().clone(),
            0,
            r#"
        var p=app.project.activeItem.layer(1).transform.opacity;
        p.setValueAtTime(0,20);
        try{p.keyInTemporalEase(1)}catch(e){}
    "#
        )
        .is_err()
    );
}
#[test]
fn jsx_restores_each_side_and_dormant_signed_speeds_as_one_native_transaction() {
    let mut e = scene();
    let original = e.project().clone();
    let result = run(original.clone(), 0, RESTORE).unwrap();
    let l = result.project.composition().layer(1).unwrap();
    let t = l.opacity_timing().unwrap();
    assert_eq!(t.keys().len(), 4);
    assert_eq!(
        t.keys()[&0].in_ease.speed.to_bits(),
        (-f64::from_bits(1)).to_bits()
    );
    assert_eq!(t.keys()[&30].in_ease.speed.to_bits(), (-7e-6f64).to_bits());
    assert_eq!(t.keys()[&90].in_ease.speed.to_bits(), (-3e-7f64).to_bits());
    assert_eq!(
        t.keys()[&120].out_ease.speed.to_bits(),
        (-1e-199f64).to_bits()
    );
    assert!(
        t.keys()
            .values()
            .all(|k| k.in_interpolation == OpacityInterpolation::Bezier
                && k.out_interpolation == OpacityInterpolation::Bezier
                && !k.temporal_continuous
                && !k.temporal_auto_bezier)
    );
    assert!(l.opacity_at(60, 1. / 30.).unwrap() > 100.);
    assert_eq!(e.project(), &original);
    assert!(e.commit_automation_project(result.project).unwrap());
    let after = e.project().clone();
    e.undo();
    assert_eq!(e.project(), &original);
    assert!(!e.can_undo());
    e.redo();
    assert_eq!(e.project(), &after);
    let bytes = project_file::encode(e.project(), None).unwrap();
    assert_eq!(
        project_file::encode(&project_file::decode(&bytes).unwrap().project, None).unwrap(),
        bytes
    );
}
#[test]
fn host_value_and_expression_reads_keep_raw_overshoot() {
    let mut e = scene();
    excursion(&mut e, false);
    let value = run(
        e.project().clone(),
        15,
        "console.log(app.project.activeItem.layer(1).transform.opacity.value);",
    )
    .unwrap();
    let raw: f64 = value.output[0].parse().unwrap();
    assert!((raw + 50.).abs() < 1e-9);
    e.execute(Command::SetExpression {
        id: 1,
        target: ExpressionTarget::Opacity,
        source: "value + 5".into(),
        enabled: true,
    })
    .unwrap();
    let value = run(
        e.project().clone(),
        15,
        "console.log(app.project.activeItem.layer(1).transform.opacity.value);",
    )
    .unwrap();
    let raw: f64 = value.output[0].parse().unwrap();
    assert!((raw + 45.).abs() < 1e-9);
    assert_eq!(&value.project, e.project());
}
#[test]
fn swallowed_unsupported_or_malformed_opacity_calls_reject_every_staged_change() {
    let mut e = scene();
    excursion(&mut e, false);
    let original = e.project().clone();
    for operation in [
        "p.setTemporalAutoBezierAtKey(1,true)",
        "p.setTemporalContinuousAtKey(1,true)",
        "p.setSpatialTangentsAtKey(1,[0,0,0],[0,0,0])",
        "p.setValue([1])",
        "p.setValueAtTime(1,-1)",
        "p.setTemporalEaseAtKey(1,[new KeyframeEase(0,0)],[new KeyframeEase(0,50)])",
        "p.setTemporalEaseAtKey(1,[new KeyframeEase(0,50),new KeyframeEase(0,50)],[new KeyframeEase(0,50)])",
        "p.setValue(50)",
        "p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.BEZIER,42)",
        "p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.BEZIER,null)",
        "p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.BEZIER,false)",
        "p.keyValue(0)",
        "p.keyTime(99)",
    ] {
        let script = format!(
            "var l=app.project.activeItem.layer(1),p=l.transform.opacity;l.name='rollback';try{{{operation}}}catch(e){{}};"
        );
        assert!(run(original.clone(), 0, &script).is_err(), "{operation}");
        assert_eq!(e.project(), &original);
    }
}
#[test]
fn inactive_native_opacity_metadata_and_keys_work_without_inventing_a_playhead() {
    let mut e = scene();
    excursion(&mut e, false);
    e.execute(Command::NewComposition).unwrap();
    let active = e.project().active_composition_id();
    let result=run(e.project().clone(),0,"var p=app.project.item(1).layer(1).transform.opacity;if(p.numKeys!==2||p.propertyValueType!==PropertyValueType.OneD||p.keyValue(1)!==0)throw Error('metadata');p.setValueAtTime(2,20);").unwrap();
    assert_eq!(result.project.active_composition_id(), active);
    assert_eq!(
        result
            .project
            .composition_by_id(1)
            .unwrap()
            .layer(1)
            .unwrap()
            .opacity_key_value(60),
        Some(20.)
    );
    let error = run(
        e.project().clone(),
        0,
        "app.project.item(1).layer(1).transform.opacity.value;",
    )
    .unwrap_err();
    assert!(error.contains("inactive composition"));
}
#[test]
fn scalar_hold_keeps_dormant_ease_and_incoming_hold_fails_closed() {
    let mut e = scene();
    excursion(&mut e, false);
    let original = e.project().clone();
    let held=run(original.clone(),15,"var p=app.project.activeItem.layer(1).transform.opacity;p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.HOLD,KeyframeInterpolationType.HOLD);p.setTemporalEaseAtKey(1,[new KeyframeEase(-1e-200,25)],[new KeyframeEase(1000,70)]);console.log(p.value);").unwrap();
    assert_eq!(held.output, ["0"]);
    assert_eq!(
        held.project
            .composition()
            .layer(1)
            .unwrap()
            .opacity_timing()
            .unwrap()
            .keys()[&0]
            .out_ease
            .speed,
        1000.
    );
    let invalid=run(original,0,"var p=app.project.activeItem.layer(1).transform.opacity;p.setInterpolationTypeAtKey(2,KeyframeInterpolationType.HOLD,KeyframeInterpolationType.LINEAR);").unwrap_err();
    assert!(invalid.contains("incoming hold"));
}
#[test]
fn explicit_invalid_outgoing_type_also_rejects_the_legacy_vector_route() {
    let source = "var l=app.project.activeItem.layer(1),p=l.transform.position;l.name='must roll back';p.setValueAtTime(0,[1,2]);try{p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.LINEAR,42);}catch(e){};";
    assert!(run(scene().project().clone(), 0, source).is_err());
}
