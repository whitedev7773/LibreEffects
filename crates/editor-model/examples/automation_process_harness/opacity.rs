//! Real producer/consumer processes and independent typed-source expectation.
use super::*;
use libre_effects_core::{OpacityEase, OpacityEdit, OpacityInterpolation, project_file};
fn edit(editor: &mut Editor, edit: OpacityEdit) {
    editor
        .execute(Command::SetOpacityTiming { id: 1, edit })
        .unwrap();
}
pub(super) fn run_case() {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "Opacity IPC".into(),
            width: 640,
            height: 360,
            fps: 30.into(),
            duration: 300,
            display_start: 0,
        })
        .unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetLayerRange {
            id: 1,
            start: 0,
            end: 120,
        })
        .unwrap();
    for frame in [0, 30] {
        edit(&mut editor, OpacityEdit::Key { frame, value: 0. });
    }
    for frame in [0, 30] {
        edit(
            &mut editor,
            OpacityEdit::Interpolation {
                frame,
                incoming: OpacityInterpolation::Bezier,
                outgoing: OpacityInterpolation::Bezier,
            },
        );
        edit(
            &mut editor,
            OpacityEdit::TemporalEase {
                frame,
                incoming: OpacityEase {
                    speed: if frame == 0 { -f64::from_bits(1) } else { 200. },
                    influence: 100. / 3.,
                },
                outgoing: OpacityEase {
                    speed: if frame == 0 { -200. } else { -1e-199 },
                    influence: 100. / 3.,
                },
            },
        );
    }
    editor.clear_history();
    let original = editor.project().clone();
    let original_bytes = project_file::encode(&original, None).unwrap();
    let script = r#"
      var l=app.project.activeItem.layer(1),p=l.transform.opacity;
      p.setValueAtTime(1,80);
      p.setTemporalEaseAtKey(2,[new KeyframeEase(-0.000007,65)],[new KeyframeEase(-1e-199,17.25)]);
      var c=l.duplicate();c.moveToBeginning();c.name='Shifted opacity';c.startTime=2;
    "#;
    let result = run(
        original.clone(),
        script,
        Duration::from_secs(2),
        Duration::from_secs(4),
        no_ui,
    )
    .unwrap();
    let mut expected = Editor::default();
    expected.replace_project(original.clone()).unwrap();
    edit(
        &mut expected,
        OpacityEdit::Key {
            frame: 30,
            value: 80.,
        },
    );
    edit(
        &mut expected,
        OpacityEdit::TemporalEase {
            frame: 30,
            incoming: OpacityEase {
                speed: -7e-6,
                influence: 65.,
            },
            outgoing: OpacityEase {
                speed: -1e-199,
                influence: 17.25,
            },
        },
    );
    expected.execute(Command::DuplicateLayer(1)).unwrap();
    let id = expected.selected().unwrap();
    expected
        .execute(Command::MoveLayer { id, index: 0 })
        .unwrap();
    expected
        .execute(Command::RenameLayer {
            id,
            name: "Shifted opacity".into(),
        })
        .unwrap();
    expected
        .execute(Command::SetLayerStart { id, frame: 60 })
        .unwrap();
    assert_eq!(
        project_file::encode(&result.project, None).unwrap(),
        project_file::encode(expected.project(), None).unwrap()
    );
    let copy = result
        .project
        .composition()
        .layer(id)
        .unwrap()
        .opacity_timing()
        .unwrap();
    assert_eq!(
        copy.keys()[&60].in_ease.speed.to_bits(),
        (-f64::from_bits(1)).to_bits()
    );
    assert_eq!(
        copy.keys()[&90].out_ease.speed.to_bits(),
        (-1e-199f64).to_bits()
    );
    assert!(editor.commit_automation_project(result.project).unwrap());
    let applied = editor.project().clone();
    editor.undo();
    assert_eq!(
        project_file::encode(editor.project(), None).unwrap(),
        original_bytes
    );
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(editor.project(), &applied);
    editor.undo();
    let failed=run(original.clone(),"var l=app.project.activeItem.layer(1);l.name='must roll back';try{l.transform.opacity.setTemporalAutoBezierAtKey(1,true);}catch(e){}",Duration::from_secs(2),Duration::from_secs(4),no_ui).unwrap_err();
    assert!(failed.contains("unsupported"), "{failed}");
    assert_eq!(editor.project(), &original);
    assert!(editor.can_redo());
    assert_eq!(
        project_file::encode(editor.project(), None).unwrap(),
        original_bytes
    );
    println!(
        "PASS: real-child native Opacity input/output, signed subnormal and dormant metadata, exact full-source duplicate/startTime, one Undo/Redo and caught-error rollback"
    );
}
