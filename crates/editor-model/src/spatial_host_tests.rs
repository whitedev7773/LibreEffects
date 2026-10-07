//! Real JavaScript-to-native vector source regressions, independently authored.
use crate::automation::{ScriptOutcome, run_script};
use crate::spatial_model_tests::scene;
use libre_effects_core::*;
use std::sync::{Arc, atomic::AtomicBool, mpsc};

fn run(project: Project, source: &str) -> Result<ScriptOutcome, String> {
    let (tx, _rx) = mpsc::channel();
    let (_tx, rx) = mpsc::channel();
    run_script(
        project,
        vec![1],
        0,
        source,
        tx,
        rx,
        Arc::new(AtomicBool::new(false)),
    )
}
const CURVE: &str = r#"
var layer=app.project.activeItem.layer(1), p=layer.transform.position;
if (!layer.threeDLayer || p.propertyValueType!==PropertyValueType.ThreeD_SPATIAL || p.dimensionsSeparated) throw Error('wrong vector metadata');
p.dimensionsSeparated=false;
var times=[0,1,4,5], values=[[180,100,0],[160,100,60],[160,100,60],[180,100,0]];
app.beginUndoGroup('Native vector sequence');
for(var i=0;i<4;i++) p.setValueAtTime(times[i],values[i]);
for(var i=1;i<=4;i++) {
 p.setInterpolationTypeAtKey(i,KeyframeInterpolationType.BEZIER,KeyframeInterpolationType.BEZIER);
 p.setTemporalEaseAtKey(i,[new KeyframeEase(i===2?1e-9:0,65)],[new KeyframeEase(0,35)]);
 p.setTemporalContinuousAtKey(i,false);p.setTemporalAutoBezierAtKey(i,false);
 var tin=i===2?[0.5,0,0]:i===3?[-0.5,0,0]:[0,0,0], tout=i===3?[0.5,0,0]:[0,0,0];
 p.setSpatialTangentsAtKey(i,tin,tout);p.setSpatialContinuousAtKey(i,true);p.setSpatialAutoBezierAtKey(i,false);
}
p.setSpatialTangentsAtKey(1,[-3,4,-5],[0,0,0]);
p.setTemporalEaseAtKey(4,[new KeyframeEase(0,65)],[new KeyframeEase(1e-199,17.25)]);
if(p.numKeys!==4 || p.keyTime(2)!==1 || p.keyValue(3)[2]!==60 || p.nearestKeyIndex(2.5)!==2) throw Error('key identity');
layer.name='Spatial 검증';
app.endUndoGroup();
"#;

#[test]
fn jsx_reads_joined_xyz_key_tangents_and_ease_without_editing() {
    let project = run(scene().project().clone(), CURVE).unwrap().project;
    let result = run(project.clone(), r#"
        var p=app.project.activeItem.layer(1).transform.position;
        if(p.keyInInterpolationType(2)!==KeyframeInterpolationType.BEZIER || p.keyOutInterpolationType(2)!==KeyframeInterpolationType.BEZIER) throw Error('sides');
        var tangent=p.keyInSpatialTangent(1), ease=p.keyOutTemporalEase(4);
        if(tangent.length!==3 || tangent[0]!==-3 || tangent[1]!==4 || tangent[2]!==-5) throw Error('XYZ tangent');
        if(ease.length!==1 || ease[0].speed!==1e-199 || ease[0].influence!==17.25) throw Error('ease');
        if(p.keySpatialContinuous(2)!==true || p.keySpatialAutoBezier(2)!==false) throw Error('spatial flags');
    "#).unwrap();
    assert_eq!(result.project, project);
}
#[test]
fn actual_jsx_builds_joined_curves_in_one_atomic_history_entry() {
    let mut e = scene();
    let before = e.project().clone();
    let result = run(before.clone(), CURVE).unwrap();
    assert_eq!(e.project(), &before);
    let source = result
        .project
        .composition()
        .layer(1)
        .unwrap()
        .spatial_position()
        .unwrap();
    assert_eq!(source.keys.len(), 4);
    assert_eq!(source.keys[&0].in_tangent, [-3., 4., -5.]);
    assert_eq!(source.keys[&30].in_ease.speed.to_bits(), 1e-9f64.to_bits());
    assert_eq!(
        source.keys[&150].out_ease.speed.to_bits(),
        1e-199f64.to_bits()
    );
    assert!(source.keys.values().all(|key| key.spatial_continuous
        && !key.spatial_auto_bezier
        && !key.temporal_continuous
        && !key.temporal_auto_bezier));
    let sampled = source.sample(75., 1. / 30.).unwrap();
    assert!(sampled[0] < 160.);
    assert_eq!(sampled[2], 60.);
    assert!(e.commit_automation_project(result.project).unwrap());
    let after = e.project().clone();
    e.undo();
    assert_eq!(e.project(), &before);
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
fn caught_unsupported_calls_and_malformed_vectors_never_commit_a_partial_rename() {
    for operation in [
        "p.setValue([1,2])",
        "p.setValue([1,2,3,4])",
        "p.dimensionsSeparated=true",
        "p.setTemporalAutoBezierAtKey(1,true)",
        "p.setTemporalContinuousAtKey(1,true)",
        "p.setSpatialAutoBezierAtKey(1,true)",
        "p.setTemporalEaseAtKey(1,[new KeyframeEase(-0.00001,50)],[new KeyframeEase(0,50)])",
        "p.setTemporalEaseAtKey(1,[new KeyframeEase(0,50),new KeyframeEase(0,50)],[new KeyframeEase(0,50)])",
        "p.setSpatialTangentsAtKey(1,[1,2],[0,0,0])",
        "p.keyValue(0)",
        "p.keyTime(99)",
        "p.removeKey(1)",
        "p.setValue([1,2,3])",
        "layer.threeDLayer=false",
    ] {
        let mut e = scene();
        e.execute(Command::SetSpatialPosition {
            id: 1,
            edit: SpatialEdit::Key {
                frame: 0,
                value: [1., 2., 3.],
            },
        })
        .unwrap();
        let original = e.project().clone();
        let source = format!(
            "var layer=app.project.activeItem.layer(1),p=layer.transform.position;layer.name='must roll back';try{{{operation}}}catch(e){{}};"
        );
        assert!(run(original.clone(), &source).is_err(), "{operation}");
        assert_eq!(e.project(), &original);
    }
}
#[test]
fn temporal_or_spatial_incompatibilities_fail_at_final_transaction_boundary() {
    for tail in [
        "p.setInterpolationTypeAtKey(2,KeyframeInterpolationType.HOLD,KeyframeInterpolationType.LINEAR);",
        "p.setSpatialTangentsAtKey(2,[1,0,0],[0,1,0]);p.setSpatialContinuousAtKey(2,true);",
        "p.setValueAtTime(0,[2,2,2]);p.setValueAtTime(1,[2,2,2]);p.setTemporalEaseAtKey(1,[new KeyframeEase(0,50)],[new KeyframeEase(1,50)]);p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.BEZIER,KeyframeInterpolationType.BEZIER);",
    ] {
        let source = format!(
            "var p=app.project.activeItem.layer(1).transform.position;p.setValueAtTime(0,[0,0,0]);p.setValueAtTime(1,[10,0,0]);p.setValueAtTime(2,[20,0,0]);{tail}"
        );
        assert!(run(scene().project().clone(), &source).is_err(), "{tail}");
    }
}
#[test]
fn conversion_property_identity_and_same_time_updates_are_native() {
    let mut e = Editor::default();
    e.execute(Command::AddRectangle).unwrap();
    let outcome=run(e.project().clone(),r#"
      var l=app.project.activeItem.layer(1),p=l.transform.position;
      if(l.threeDLayer || p.propertyValueType!==PropertyValueType.TwoD_SPATIAL) throw Error('legacy');
      l.threeDLayer=true;p.setValue([12,34,56]);
      if(!l.threeDLayer || p.propertyValueType!==PropertyValueType.ThreeD_SPATIAL || p.value[2]!==56) throw Error('conversion');
      p.setValueAtTime(1,[20,30,40]);p.setSpatialTangentsAtKey(1,[-1,2,-3],[4,-5,6]);
      p.setValueAtTime(1,[50,60,70]);if(p.numKeys!==1 || p.keyValue(1)[2]!==70) throw Error('replacement');
    "#).unwrap();
    let p = outcome
        .project
        .composition()
        .layer(1)
        .unwrap()
        .spatial_position()
        .unwrap();
    assert_eq!(p.keys[&30].value, [50., 60., 70.]);
    assert_eq!(p.keys[&30].in_tangent, [-1., 2., -3.]);
    assert_eq!(p.keys[&30].out_tangent, [4., -5., 6.]);
    assert_eq!(p.value, [12., 34., 56.]);
    assert!(outcome.project.composition().camera().is_none());
}
#[test]
fn inactive_composition_metadata_and_key_edits_do_not_invent_a_playhead() {
    let mut e = scene();
    e.execute(Command::SetSpatialPosition {
        id: 1,
        edit: SpatialEdit::Key {
            frame: 0,
            value: [1., 2., 3.],
        },
    })
    .unwrap();
    let original_id = e.project().active_composition_id();
    e.execute(Command::NewComposition).unwrap();
    let active = e.project().active_composition_id();
    let result=run(e.project().clone(),r#"
        var p=app.project.item(1).layer(1).transform.position;
        if(p.propertyValueType!==PropertyValueType.ThreeD_SPATIAL || p.numKeys!==1 || p.keyValue(1)[2]!==3) throw Error('metadata');
        p.setValueAtTime(2,[4,5,6]);
    "#).unwrap();
    assert_eq!(result.project.active_composition_id(), active);
    assert_eq!(
        result
            .project
            .composition_by_id(original_id)
            .unwrap()
            .layer(1)
            .unwrap()
            .spatial_position()
            .unwrap()
            .keys[&60]
            .value,
        [4., 5., 6.]
    );
    let failure = run(
        e.project().clone(),
        "try{app.project.item(1).layer(1).transform.position.value;}catch(e){}",
    )
    .unwrap_err();
    assert!(failure.contains("inactive composition"), "{failure}");
}
#[test]
fn public_xyz_scriptui_example_applies_once_and_cancel_keeps_exact_source() {
    use crate::automation::{UiNode, UiRequest, UiResponse};
    fn button(node: &UiNode, text: &str) -> Option<u64> {
        if node.kind == "button" && node.text == text {
            return Some(node.id);
        }
        node.children.iter().find_map(|node| button(node, text))
    }
    for accept in [false, true] {
        let original = scene().project().clone();
        let input = original.clone();
        let (request_tx, request_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_script(
                input,
                vec![1],
                0,
                include_str!("../../../examples/scripts/spatial-position.jsx"),
                request_tx,
                response_rx,
                Arc::new(AtomicBool::new(false)),
            )
        });
        let UiRequest::Dialog { id, root, .. } = request_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
        else {
            panic!("Expected native example dialog")
        };
        let control_id = button(&root, if accept { "Apply" } else { "Cancel" }).unwrap();
        response_tx
            .send(UiResponse::Click {
                dialog_id: id,
                control_id,
            })
            .unwrap();
        let result = worker.join().unwrap();
        if accept {
            let result = result.unwrap();
            let p = result
                .project
                .composition()
                .layer(1)
                .unwrap()
                .spatial_position()
                .unwrap();
            assert_eq!(p.keys.len(), 2);
            assert_eq!(p.keys[&0].value, [340., 135., 500.]);
            assert_eq!(p.keys[&30].value, [300., 145., 0.]);
            assert_eq!(p.keys[&0].in_tangent, [-3., 4., -5.]);
            assert_eq!(p.keys[&30].out_tangent, [6., -8., 10.]);
            assert_eq!(p.keys[&30].out_ease.speed.to_bits(), 2e-199f64.to_bits());
        } else {
            assert_eq!(result.unwrap_err(), "Script canceled");
            assert!(
                original
                    .composition()
                    .layer(1)
                    .unwrap()
                    .spatial_position()
                    .unwrap()
                    .keys
                    .is_empty()
            );
        }
    }
}
