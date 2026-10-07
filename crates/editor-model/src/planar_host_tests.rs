//! Independently authored regressions through the real JSX-to-planar host path.
use crate::automation::{ScriptOutcome, run_script};
use libre_effects_core::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool, mpsc},
};

fn scene(keyed: bool) -> Editor {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Joined XY".into(),
        })
        .unwrap();
    let position = if keyed {
        SpatialPosition2 {
            value: None,
            keys: BTreeMap::from([(0, SpatialKey2::new([30., 40.]))]),
        }
    } else {
        SpatialPosition2::new([30., 40.])
    };
    editor
        .execute(Command::SetPlanarPosition { id: 1, position })
        .unwrap();
    editor.clear_history();
    editor
}

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

const CURVE: &str = r#"
var layer=app.project.activeItem.layer('Joined XY'), p=layer.transform.position;
if(layer.threeDLayer || p.propertyValueType!==PropertyValueType.TwoD_SPATIAL || p.dimensionsSeparated) throw Error('planar metadata');
p.dimensionsSeparated=false;
app.beginUndoGroup('Joined XY sequence');
var times=[0,1,3,4], values=[[30,40],[80,40],[80,40],[30,40]];
for(var n=0;n<times.length;n++) p.setValueAtTime(times[n],values[n]);
for(var k=1;k<=4;k++) {
 p.setInterpolationTypeAtKey(k,KeyframeInterpolationType.BEZIER,KeyframeInterpolationType.BEZIER);
 p.setTemporalEaseAtKey(k,[new KeyframeEase(k===2?1e-9:0,62.5)],[new KeyframeEase(0,37.5)]);
 p.setTemporalContinuousAtKey(k,false);p.setTemporalAutoBezierAtKey(k,false);
 p.setSpatialTangentsAtKey(k,k===2?[8,0]:k===3?[-8,0]:[0,0],k===2?[-8,0]:k===3?[8,0]:[0,0]);
 p.setSpatialContinuousAtKey(k,true);p.setSpatialAutoBezierAtKey(k,false);
}
p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.HOLD,KeyframeInterpolationType.BEZIER);
p.setInterpolationTypeAtKey(4,KeyframeInterpolationType.BEZIER,KeyframeInterpolationType.LINEAR);
p.setSpatialTangentsAtKey(1,[3,-7],[0,0]);
p.setSpatialTangentsAtKey(4,[0,0],[-2,6]);
p.setTemporalEaseAtKey(4,[new KeyframeEase(0,62.5)],[new KeyframeEase(1e-199,17.25)]);
if(p.numKeys!==4 || p.keyTime(2)!==1 || p.keyValue(3).length!==2 || p.keyValue(3)[0]!==80 || p.nearestKeyIndex(2)!==2) throw Error('planar key identity');
layer.name='XY 검증';
app.endUndoGroup();
"#;

#[test]
fn jsx_restores_joined_xy_metadata_without_a_base_and_commits_one_undo() {
    let mut editor = scene(true);
    let original = editor.project().clone();
    let result = run(original.clone(), 60, CURVE).unwrap();
    let layer = result.project.composition().layer(1).unwrap();
    let track = layer.planar_position().unwrap();
    assert!(!layer.is_three_d());
    assert!(layer.spatial_position().is_none());
    assert!(layer.property(Property::PositionX).is_none());
    assert!(layer.property(Property::PositionY).is_none());
    assert!(track.value.is_none());
    assert_eq!(track.keys.len(), 4);
    assert_eq!(track.keys[&0].in_interpolation, SpatialInterpolation::Hold);
    assert_eq!(
        track.keys[&0].out_interpolation,
        SpatialInterpolation::Bezier
    );
    assert_eq!(
        track.keys[&120].in_interpolation,
        SpatialInterpolation::Bezier
    );
    assert_eq!(
        track.keys[&120].out_interpolation,
        SpatialInterpolation::Linear
    );
    assert_eq!(track.keys[&0].in_tangent, [3., -7.]);
    assert_eq!(track.keys[&120].out_tangent, [-2., 6.]);
    assert_eq!(track.keys[&30].in_ease.speed.to_bits(), 1e-9f64.to_bits());
    assert_eq!(
        track.keys[&120].out_ease.speed.to_bits(),
        1e-199f64.to_bits()
    );
    assert_eq!(track.keys[&120].out_ease.influence, 17.25);
    assert!(track.keys.values().all(|key| key.spatial_continuous
        && !key.spatial_auto_bezier
        && !key.temporal_continuous
        && !key.temporal_auto_bezier));
    let sampled = layer.position2_at(60, 1. / 30.).unwrap();
    assert!(sampled[0] < 80.);
    assert_eq!(sampled[1], 40.);
    assert!(result.project.composition().camera().is_none());
    assert_eq!(editor.project(), &original);
    assert!(editor.commit_automation_project(result.project).unwrap());
    let after = editor.project().clone();
    editor.undo();
    assert_eq!(editor.project(), &original);
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(editor.project(), &after);
    let bytes = project_file::encode(editor.project(), None).unwrap();
    let decoded = project_file::decode(&bytes).unwrap();
    assert_eq!(decoded.project, after);
    assert_eq!(project_file::encode(&decoded.project, None).unwrap(), bytes);
}

#[test]
fn static_and_same_time_assignments_keep_source_identity_and_tangents() {
    let editor = scene(false);
    let result = run(editor.project().clone(), 30, r#"
        var l=app.project.activeItem.layer(1),p=l.transform.position;
        l.threeDLayer=false;p.setValue([-15,25]);
        if(p.numKeys!==0 || p.value[0]!==-15 || p.value.length!==2 || l.threeDLayer) throw Error('static');
        p.setValueAtTime(1,[20,30]);
        p.setSpatialTangentsAtKey(1,[-3,7],[5,-9]);
        p.setValueAtTime(1,[70,90]);
        if(p.numKeys!==1 || p.keyValue(1)[0]!==70 || p.value[1]!==90) throw Error('same time');
    "#).unwrap();
    let track = result
        .project
        .composition()
        .layer(1)
        .unwrap()
        .planar_position()
        .unwrap();
    assert_eq!(track.value, Some([-15., 25.]));
    assert_eq!(track.keys[&30].value, [70., 90.]);
    assert_eq!(track.keys[&30].in_tangent, [-3., 7.]);
    assert_eq!(track.keys[&30].out_tangent, [5., -9.]);
}

#[test]
fn planar_value_expressions_evaluate_authored_samples_and_report_enabled_state() {
    let mut editor = scene(true);
    editor
        .execute(Command::EditPlanarPosition {
            id: 1,
            edit: PlanarEdit::Key {
                frame: 30,
                value: [90., 80.],
            },
        })
        .unwrap();
    let source = editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .planar_position()
        .unwrap()
        .clone();
    let result = run(editor.project().clone(), 15, r#"
        var p=app.project.activeItem.layer(1).transform.position;
        if(p.value[0]!==60 || p.value[1]!==60 || p.expressionEnabled || p.expression!=='') throw Error('authored sample');
        p.expression='[value[0]+7,value[1]-11]';
        if(!p.expressionEnabled || p.expression!=='[value[0]+7,value[1]-11]' || p.value[0]!==67 || p.value[1]!==49) throw Error('enabled expression');
        if(p.keyValue(1)[0]!==30 || p.numKeys!==2) throw Error('authored keys');
        p.expressionEnabled=false;
        if(p.expressionEnabled || p.value[0]!==60 || p.value[1]!==60) throw Error('disabled expression');
        p.expressionEnabled=true;
    "#).unwrap();
    let layer = result.project.composition().layer(1).unwrap();
    assert_eq!(layer.planar_position(), Some(&source));
    assert!(layer.has_enabled_expression(ExpressionTarget::Position));
    let checked = run(
        result.project.clone(),
        15,
        "console.log(JSON.stringify(app.project.activeItem.layer(1).transform.position.value));",
    )
    .unwrap();
    assert_eq!(checked.output, vec!["[67,49]"]);
    assert_eq!(checked.project, result.project);
}

#[test]
fn planar_parent_and_duplicate_survive_whole_project_commit_and_save() {
    let mut editor = scene(true);
    editor
        .execute(Command::EditPlanarPosition {
            id: 1,
            edit: PlanarEdit::Tangents {
                frame: 0,
                incoming: [3., -7.],
                outgoing: [-2., 6.],
            },
        })
        .unwrap();
    editor
        .execute(Command::EditPlanarPosition {
            id: 1,
            edit: PlanarEdit::TemporalEase {
                frame: 0,
                incoming: SpatialEase {
                    speed: 1e-199,
                    influence: 62.5,
                },
                outgoing: SpatialEase {
                    speed: 0.,
                    influence: 37.5,
                },
            },
        })
        .unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetPlanarParent {
            id: 1,
            parent: Some(2),
        })
        .unwrap();
    editor.execute(Command::NewComposition).unwrap();
    let other_comp = editor.project().active_composition_id();
    editor.execute(Command::AddRectangle).unwrap();
    editor.activate_composition(1).unwrap();
    editor.select(1);
    editor.clear_history();
    let before = editor.project().clone();
    let result = run(before.clone(), 0, r#"
        var l=app.project.activeItem.layer('Joined XY');
        var copy=l.duplicate();copy.name='Planar copy';
        copy.transform.position.setValueAtTime(1,[50,60]);
        if(copy.threeDLayer || copy.transform.position.propertyValueType!==PropertyValueType.TwoD_SPATIAL) throw Error('duplicated metadata');
    "#).unwrap();
    let original = result.project.composition().layer(1).unwrap();
    let copy = result
        .project
        .composition()
        .layers()
        .iter()
        .find(|l| l.name() == "Planar copy")
        .unwrap();
    assert_eq!(copy.parent(), Some(2));
    assert_eq!(original.parent(), Some(2));
    assert_eq!(original.planar_position().unwrap().keys.len(), 1);
    assert_eq!(copy.planar_position().unwrap().keys.len(), 2);
    assert_eq!(
        copy.planar_position().unwrap().keys[&0],
        original.planar_position().unwrap().keys[&0]
    );
    assert_eq!(original.position2_at(0, 1. / 30.).unwrap(), [30., 40.]);
    assert!(copy.planar_position().unwrap().value.is_none());
    assert_eq!(
        result.project.composition_by_id(other_comp),
        before.composition_by_id(other_comp)
    );
    assert!(editor.commit_automation_project(result.project).unwrap());
    assert_eq!(editor.selected(), Some(1));
    let after = editor.project().clone();
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
    let bytes = project_file::encode(&after, None).unwrap();
    assert_eq!(project_file::decode(&bytes).unwrap().project, after);
}

#[test]
fn caught_unsupported_or_malformed_planar_calls_reject_the_entire_draft() {
    let editor = scene(true);
    let original = editor.project().clone();
    for operation in [
        "p.setValue([1])",
        "p.setValue([1,2,3])",
        "p.setValueAtTime(0.01,[1,2])",
        "p.setValueAtTime(1,[NaN,2])",
        "p.setSpatialTangentsAtKey(1,[1,2,3],[0,0])",
        "p.setSpatialTangentsAtKey(1,[1,2],[0])",
        "p.setTemporalAutoBezierAtKey(1,true)",
        "p.setTemporalContinuousAtKey(1,true)",
        "p.setSpatialAutoBezierAtKey(1,true)",
        "p.setSpatialContinuousAtKey(1,1)",
        "p.setTemporalEaseAtKey(1,[new KeyframeEase(-1e-9,50)],[new KeyframeEase(0,50)])",
        "p.setTemporalEaseAtKey(1,[new KeyframeEase(0,50),new KeyframeEase(0,50)],[new KeyframeEase(0,50)])",
        "p.setTemporalEaseAtKey(1,[{speed:0,influence:50,extra:1}],[new KeyframeEase(0,50)])",
        "p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.BEZIER,null)",
        "p.dimensionsSeparated=true",
        "p.keyValue(0)",
        "p.keyTime(99)",
        "p.removeKey(1)",
        "p.setValue([1,2])",
        "l.threeDLayer=true",
    ] {
        let script = format!(
            "var l=app.project.activeItem.layer(1),p=l.transform.position;l.name='rollback';try{{{operation}}}catch(e){{}};"
        );
        assert!(run(original.clone(), 0, &script).is_err(), "{operation}");
        assert_eq!(editor.project(), &original);
    }
}

#[test]
fn invalid_active_planar_metadata_is_rejected_at_the_final_boundary() {
    let original = scene(false).project().clone();
    for tail in [
        "p.setSpatialTangentsAtKey(2,[1,0],[0,1]);p.setSpatialContinuousAtKey(2,true);",
        "p.setInterpolationTypeAtKey(2,KeyframeInterpolationType.HOLD,KeyframeInterpolationType.LINEAR);",
        "p.setValueAtTime(0,[10,0]);p.setValueAtTime(1,[10,0]);p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.BEZIER,KeyframeInterpolationType.BEZIER);p.setTemporalEaseAtKey(1,[new KeyframeEase(0,50)],[new KeyframeEase(2,50)]);",
    ] {
        let script = format!(
            "var l=app.project.activeItem.layer(1),p=l.transform.position;l.name='rollback';p.setValueAtTime(0,[0,0]);p.setValueAtTime(1,[10,0]);p.setValueAtTime(2,[20,0]);{tail}"
        );
        assert!(run(original.clone(), 0, &script).is_err(), "{tail}");
    }
}

#[test]
fn removing_a_planar_key_reindexes_without_inventing_an_authored_base() {
    let original = scene(true).project().clone();
    let result = run(original, 0, r#"
        var p=app.project.activeItem.layer(1).transform.position;
        p.setValueAtTime(1,[70,90]);p.setValueAtTime(2,[100,120]);
        p.removeKey(2);
        if(p.numKeys!==2 || p.keyTime(2)!==2 || p.keyValue(2)[0]!==100 || p.nearestKeyIndex(1)!==1) throw Error('key reindex');
    "#).unwrap();
    let track = result
        .project
        .composition()
        .layer(1)
        .unwrap()
        .planar_position()
        .unwrap();
    assert!(track.value.is_none());
    assert_eq!(track.keys.keys().copied().collect::<Vec<_>>(), vec![0, 60]);
}

#[test]
fn inactive_planar_metadata_and_key_edits_work_without_inventing_a_playhead() {
    let mut editor = scene(true);
    editor.execute(Command::NewComposition).unwrap();
    let active = editor.project().active_composition_id();
    let result = run(editor.project().clone(), 0, r#"
        var l=app.project.item(1).layer(1),p=l.transform.position;
        if(l.threeDLayer || p.propertyValueType!==PropertyValueType.TwoD_SPATIAL || p.numKeys!==1 || p.dimensionsSeparated || p.expressionEnabled || p.keyValue(1)[0]!==30 || p.keyTime(1)!==0) throw Error('inactive metadata');
        p.setValueAtTime(1,[50,60]);
    "#).unwrap();
    assert_eq!(result.project.active_composition_id(), active);
    assert_eq!(
        result
            .project
            .composition_by_id(1)
            .unwrap()
            .layer(1)
            .unwrap()
            .planar_position()
            .unwrap()
            .keys[&30]
            .value,
        [50., 60.]
    );
    let error = run(
        editor.project().clone(),
        0,
        "try{app.project.item(1).layer(1).transform.position.value;}catch(e){}",
    )
    .unwrap_err();
    assert!(error.contains("inactive composition"), "{error}");
    let mut static_editor = scene(false);
    static_editor.execute(Command::NewComposition).unwrap();
    assert!(run(static_editor.project().clone(), 0, "if(app.project.item(1).layer(1).transform.position.value[0]!==30)throw Error('static read');").is_ok());
    static_editor.activate_composition(1).unwrap();
    static_editor
        .execute(Command::SetExpression {
            id: 1,
            target: ExpressionTarget::Position,
            source: "[value[0]+time,value[1]]".into(),
            enabled: true,
        })
        .unwrap();
    static_editor.activate_composition(2).unwrap();
    let metadata = run(
        static_editor.project().clone(),
        0,
        "if(!app.project.item(1).layer(1).transform.position.expressionEnabled)throw Error('expression metadata');",
    );
    assert!(metadata.is_ok(), "{metadata:?}");
    assert!(
        run(
            static_editor.project().clone(),
            0,
            "try{app.project.item(1).layer(1).transform.position.value;}catch(e){}"
        )
        .unwrap_err()
        .contains("inactive composition")
    );
}

#[test]
fn planar_manual_modes_do_not_change_legacy_or_xyz_dispatch() {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    let legacy = run(editor.project().clone(), 0, "var p=app.project.activeItem.layer(1).transform.position;p.setValue([5,6]);p.setValueAtTime(1,[7,8]);").unwrap();
    let layer = legacy.project.composition().layer(1).unwrap();
    assert!(layer.planar_position().is_none());
    assert!(layer.spatial_position().is_none());
    assert_eq!(
        layer.property(Property::PositionX).unwrap().value_at(30),
        7.
    );
    let xyz = run(editor.project().clone(), 0, "var l=app.project.activeItem.layer(1);l.threeDLayer=true;l.transform.position.setValue([5,6,7]);").unwrap();
    let layer = xyz.project.composition().layer(1).unwrap();
    assert!(layer.planar_position().is_none());
    assert_eq!(layer.spatial_position().unwrap().value, [5., 6., 7.]);
}
