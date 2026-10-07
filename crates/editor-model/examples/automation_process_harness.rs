//! GPUI-free acceptance harness using the exact production process supervisor.
//! Run: cargo run -p libre-effects-editor-model --example automation_process_harness
#[allow(dead_code)]
#[path = "../../../apps/desktop/src/automation_process.rs"]
mod automation_process;
#[path = "automation_process_harness/opacity.rs"]
mod opacity_case;

use libre_effects_core::{Command, Editor, Project};
use libre_effects_editor_model::automation::{ScriptOutcome, UiRequest, UiResponse};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn run(
    project: Project,
    source: &str,
    slice: Duration,
    total: Duration,
    ui: impl FnOnce(mpsc::Receiver<UiRequest>, mpsc::Sender<UiResponse>, Arc<AtomicBool>)
    + Send
    + 'static,
) -> Result<ScriptOutcome, String> {
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let ui_cancel = cancel.clone();
    let ui_thread = std::thread::spawn(move || ui(request_rx, response_tx, ui_cancel));
    let result = automation_process::run_script_with(
        &std::env::current_exe().unwrap(),
        project,
        vec![1],
        0,
        source.to_string(),
        request_tx,
        response_rx,
        cancel,
        slice,
        total,
    );
    ui_thread.join().unwrap();
    result
}
fn no_ui(
    requests: mpsc::Receiver<UiRequest>,
    _responses: mpsc::Sender<UiResponse>,
    _cancel: Arc<AtomicBool>,
) {
    assert!(requests.recv().is_err(), "Unexpected UI request");
}
fn main() {
    // Fixed synthetic transport fixtures exist only in this harness binary.
    // Production's private worker dispatcher recognizes no test modes.
    match std::env::args().nth(1).as_deref() {
        Some("--fixture-empty") => return,
        Some("--fixture-crash") => std::process::exit(23),
        Some("--fixture-stall") => {
            std::thread::sleep(Duration::from_secs(60));
            return;
        }
        Some("--fixture-oversized") => {
            use std::io::Write;
            std::io::stdout()
                .write_all(b"LEW1\xff\xff\xff\xff")
                .unwrap();
            return;
        }
        Some("--fixture-flood") => {
            use std::io::Write;
            let mut output = std::io::stdout().lock();
            loop {
                if output.write_all(b"LEW1\x00\x00\x00\x02{}").is_err() {
                    return;
                }
            }
        }
        _ => {}
    }

    if let Some(result) = automation_process::dispatch_worker() {
        if result.is_err() {
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--opacity-roundtrip") {
        return opacity_case::run_case();
    }
    if std::env::args().nth(1).as_deref() == Some("--spatial-roundtrip") {
        use libre_effects_core::{SpatialEase, SpatialEdit, SpatialInterpolation, project_file};
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor
            .execute(Command::SetThreeD {
                id: 1,
                enabled: true,
            })
            .unwrap();
        editor
            .execute(Command::SetLayerRange {
                id: 1,
                start: 0,
                end: 90,
            })
            .unwrap();
        editor.clear_history();
        let original = editor.project().clone();
        let source = r#"
          var l=app.project.activeItem.layer(1),p=l.transform.position;
          l.name='Vector source';p.setValueAtTime(0,[12,34,56]);p.setValueAtTime(1,[78,90,123]);
          p.setInterpolationTypeAtKey(1,KeyframeInterpolationType.BEZIER,KeyframeInterpolationType.BEZIER);
          p.setInterpolationTypeAtKey(2,KeyframeInterpolationType.BEZIER,KeyframeInterpolationType.BEZIER);
          p.setTemporalEaseAtKey(1,[new KeyframeEase(1e-199,17.25)],[new KeyframeEase(0,35)]);
          p.setTemporalEaseAtKey(2,[new KeyframeEase(0,65)],[new KeyframeEase(1e-198,27.5)]);
          p.setSpatialTangentsAtKey(1,[-3,4,-5],[0,0,0]);p.setSpatialTangentsAtKey(2,[0,0,0],[6,-7,8]);
          p.setSpatialContinuousAtKey(1,true);p.setSpatialContinuousAtKey(2,true);
          var copy=l.duplicate();copy.moveToBeginning();copy.name='Shifted vector';copy.startTime=2;
        "#;
        let result = run(
            original.clone(),
            source,
            Duration::from_secs(2),
            Duration::from_secs(4),
            no_ui,
        )
        .unwrap();
        let mut expected = Editor::default();
        expected.replace_project(original.clone()).unwrap();
        expected
            .execute(Command::RenameLayer {
                id: 1,
                name: "Vector source".into(),
            })
            .unwrap();
        for (frame, value) in [(0, [12., 34., 56.]), (30, [78., 90., 123.])] {
            expected
                .execute(Command::SetSpatialPosition {
                    id: 1,
                    edit: SpatialEdit::Key { frame, value },
                })
                .unwrap();
            expected
                .execute(Command::SetSpatialPosition {
                    id: 1,
                    edit: SpatialEdit::Interpolation {
                        frame,
                        incoming: SpatialInterpolation::Bezier,
                        outgoing: SpatialInterpolation::Bezier,
                    },
                })
                .unwrap();
            let (incoming, outgoing, tin, tout) = if frame == 0 {
                (
                    SpatialEase {
                        speed: 1e-199,
                        influence: 17.25,
                    },
                    SpatialEase {
                        speed: 0.,
                        influence: 35.,
                    },
                    [-3., 4., -5.],
                    [0.; 3],
                )
            } else {
                (
                    SpatialEase {
                        speed: 0.,
                        influence: 65.,
                    },
                    SpatialEase {
                        speed: 1e-198,
                        influence: 27.5,
                    },
                    [0.; 3],
                    [6., -7., 8.],
                )
            };
            expected
                .execute(Command::SetSpatialPosition {
                    id: 1,
                    edit: SpatialEdit::TemporalEase {
                        frame,
                        incoming,
                        outgoing,
                    },
                })
                .unwrap();
            expected
                .execute(Command::SetSpatialPosition {
                    id: 1,
                    edit: SpatialEdit::Tangents {
                        frame,
                        incoming: tin,
                        outgoing: tout,
                    },
                })
                .unwrap();
            expected
                .execute(Command::SetSpatialPosition {
                    id: 1,
                    edit: SpatialEdit::SpatialContinuous { frame, value: true },
                })
                .unwrap();
        }
        expected.execute(Command::DuplicateLayer(1)).unwrap();
        let copy = expected.selected().unwrap();
        expected
            .execute(Command::MoveLayer { id: copy, index: 0 })
            .unwrap();
        expected
            .execute(Command::RenameLayer {
                id: copy,
                name: "Shifted vector".into(),
            })
            .unwrap();
        expected
            .execute(Command::SetLayerStart {
                id: copy,
                frame: 60,
            })
            .unwrap();
        assert_eq!(
            project_file::encode(&result.project, None).unwrap(),
            project_file::encode(expected.project(), None).unwrap()
        );
        assert_eq!(editor.project(), &original);
        assert!(editor.commit_automation_project(result.project).unwrap());
        editor.undo();
        assert_eq!(editor.project(), &original);
        assert!(!editor.can_undo());
        println!(
            "PASS: real child process joined XYZ/key maps, dormant tangents/tiny speeds, duplicate/startTime and exact atomic Undo"
        );
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--project-roundtrip") {
        let path = std::env::args().nth(2).expect("Synthetic project path");
        let bytes = std::fs::read(&path).unwrap();
        let decoded = libre_effects_core::project_file::decode(&bytes).unwrap();
        let project = decoded.project;
        let source_encoding =
            libre_effects_core::project_file::encode(&project, decoded.view).unwrap();
        let mut expected = project.clone();
        let id = expected.composition().layers()[0].id();
        expected
            .apply_automation_command(
                expected.active_composition_id(),
                Command::RenameLayer {
                    id,
                    name: "IPC mapped layer".into(),
                },
            )
            .unwrap();
        let result = run(
            project.clone(),
            "app.project.activeItem.layer(1).name='IPC mapped layer';",
            Duration::from_secs(2),
            Duration::from_secs(3),
            no_ui,
        )
        .unwrap();
        assert_eq!(result.project.media_sharing(), expected.media_sharing());
        assert_eq!(
            libre_effects_core::project_file::encode(&result.project, decoded.view).unwrap(),
            libre_effects_core::project_file::encode(&expected, decoded.view).unwrap()
        );
        assert_eq!(
            libre_effects_core::project_file::encode(&project, decoded.view).unwrap(),
            source_encoding
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        println!(
            "PASS: supplied synthetic map fixture rename preserves exact LEP chunks, VIEW and original bytes"
        );
        return;
    }
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    let project = editor.project().clone();
    let quick = Duration::from_millis(500);
    let total = Duration::from_secs(3);

    let result = run(project.clone(), "app.project.activeItem.layer(1).name = 'Process success'; $.writeln([3,1,2].sort().reverse().join(','));", quick, total, no_ui).unwrap();
    assert_eq!(
        result.project.composition().layer(1).unwrap().name(),
        "Process success"
    );
    assert_eq!(result.output, ["3,2,1"]);
    assert_eq!(
        *editor.project(),
        project,
        "Worker never mutates the source editor"
    );
    assert!(editor.commit_automation_project(result.project).unwrap());
    editor.undo();
    assert_eq!(*editor.project(), project);
    println!("PASS: ordinary JSX, Array sort/reverse/join, atomic commit and Undo");

    // Exercise the same public synthetic sample as native QA across real child
    // processes, including the first generated key at JSON object key "0".
    let mut text_editor = Editor::default();
    text_editor
        .execute(Command::AddContent {
            content: libre_effects_core::Content::Text {
                text: "Template".into(),
                font_size: 32.,
            },
            name: "Template".into(),
            width: 300.,
            height: 80.,
        })
        .unwrap();
    let text_baseline = text_editor.project().clone();
    let sample = include_str!("../../../examples/scripts/duplicate-text.jsx");
    let generated = run(
        text_baseline.clone(),
        sample,
        quick,
        total,
        |requests, responses, _| {
            fn input(node: &libre_effects_editor_model::automation::UiNode) -> Option<u64> {
                if node.kind == "edittext" {
                    return Some(node.id);
                }
                node.children.iter().find_map(input)
            }
            let UiRequest::Dialog { id, root, .. } = requests.recv().unwrap() else {
                panic!("Expected sample dialog")
            };
            responses
                .send(UiResponse::Change {
                    dialog_id: id,
                    control_id: input(&root).unwrap(),
                    text: "One\n한국어\n三".into(),
                })
                .unwrap();
            let UiRequest::Dialog {
                id,
                default_element,
                ..
            } = requests.recv().unwrap()
            else {
                panic!("Expected refreshed sample dialog")
            };
            responses
                .send(UiResponse::Click {
                    dialog_id: id,
                    control_id: default_element.unwrap(),
                })
                .unwrap();
        },
    )
    .unwrap();
    assert_eq!(generated.output, ["Created 3 text layers"]);
    assert_eq!(generated.project.composition().layers().len(), 4);
    for layer in generated
        .project
        .composition()
        .layers()
        .iter()
        .filter(|layer| layer.id() != 1)
    {
        let opacity = layer
            .property(libre_effects_core::Property::Opacity)
            .expect("known scalar fixture property");
        assert_eq!(opacity.keys().len(), 2);
        assert_eq!(opacity.keys().get(&0).unwrap().value, 0.);
        assert_eq!(layer.markers().len(), 1);
    }
    assert_eq!(*text_editor.project(), text_baseline);
    let applied = generated.project.clone();
    assert!(
        text_editor
            .commit_automation_project(generated.project)
            .unwrap()
    );
    text_editor.undo();
    assert_eq!(*text_editor.project(), text_baseline);
    text_editor.redo();
    assert_eq!(*text_editor.project(), applied);
    assert_eq!(
        Project::from_json(&applied.to_json().unwrap()).unwrap(),
        applied
    );
    // A second independent child must decode the already-animated Start input,
    // then return it unchanged through Complete, preserving exact source bytes.
    let returned = run(
        applied.clone(),
        "$.writeln('Animated input preserved');",
        quick,
        total,
        no_ui,
    )
    .unwrap();
    assert_eq!(returned.project, applied);
    assert_eq!(
        returned.project.to_json().unwrap(),
        applied.to_json().unwrap()
    );
    println!("PASS: public three-line sample, keyed Complete and Start, Undo/Redo and save/reopen");

    let mut library_editor = Editor::default();
    library_editor.replace_project(project.clone()).unwrap();
    library_editor
        .execute(Command::NewProjectFolder {
            name: "IPC folder".into(),
            parent: None,
        })
        .unwrap();
    library_editor
        .execute(Command::ImportAsset {
            content: libre_effects_core::Content::Image { png: "YWJj".into() },
            width: 8.,
            height: 8.,
            name: "IPC image".into(),
            folder: Some(1),
            frame: None,
        })
        .unwrap();
    library_editor
        .execute(Command::MoveProjectItem {
            item: libre_effects_core::ProjectItem::Composition(1),
            folder: Some(1),
        })
        .unwrap();
    library_editor.execute(Command::NewComposition).unwrap();
    library_editor.execute(Command::AddRectangle).unwrap();
    let library_project = library_editor.project().clone();
    let returned = run(
        library_project.clone(),
        "$.writeln('Library preserved');",
        quick,
        total,
        no_ui,
    )
    .unwrap();
    assert_eq!(returned.project, library_project);
    assert_eq!(
        returned.project.to_json().unwrap(),
        library_project.to_json().unwrap()
    );
    println!("PASS: populated numeric asset/folder/composition maps cross both process directions");

    let result = run(
        project.clone(),
        "app.project.activeItem.layer(1).name='partial'; var a=[]; a.length=4294967295; a.reverse();",
        quick,
        total,
        no_ui,
    );
    assert!(result.unwrap_err().contains("execution time limit"));
    assert_eq!(*editor.project(), project);
    assert!(!automation_process::worker_active());
    println!("PASS: native sparse-array reverse watchdog kills/reaps and rolls back");

    let source = "app.project.activeItem.layer(1).name='partial'; alert('Cancel this script'); var a=[]; a.length=4294967295; a.sort();";
    let started = Instant::now();
    let result = run(
        project.clone(),
        source,
        quick,
        total,
        |requests, responses, cancel| {
            assert!(matches!(
                requests.recv_timeout(Duration::from_secs(2)).unwrap(),
                UiRequest::Alert { .. }
            ));
            responses.send(UiResponse::AlertDismissed).unwrap();
            std::thread::sleep(Duration::from_millis(80));
            cancel.store(true, Ordering::Relaxed);
        },
    );
    assert!(result.unwrap_err().contains("canceled"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(*editor.project(), project);
    assert!(!automation_process::worker_active());
    println!("PASS: cancellation during native builtin promptly kills/reaps and rolls back");

    let result = run(project.clone(), "alert('Wait longer than the execution slice'); app.project.activeItem.layer(1).name='After UI';", quick, total, |requests, responses, _| {
        assert!(matches!(requests.recv().unwrap(), UiRequest::Alert { .. }));
        std::thread::sleep(Duration::from_millis(700));
        responses.send(UiResponse::AlertDismissed).unwrap();
    }).unwrap();
    assert_eq!(
        result.project.composition().layer(1).unwrap().name(),
        "After UI"
    );
    println!("PASS: actual UI wait pauses watchdog without replaying source");

    let source = "alert('Resume native work'); var a=[]; a.length=4294967295; a.reverse();";
    let result = run(
        project.clone(),
        source,
        quick,
        total,
        |requests, responses, _| {
            requests.recv().unwrap();
            std::thread::sleep(Duration::from_millis(700));
            responses.send(UiResponse::AlertDismissed).unwrap();
            // Keep the UI response channel alive; this should fail by watchdog.
            assert!(requests.recv().is_err());
        },
    );
    assert!(result.unwrap_err().contains("execution time limit"));
    println!("PASS: response resumes independent execution watchdog");

    let source = "app.project.activeItem.layer(1).name='partial'; var w=new Window('dialog','Cancel'); w.add('button',undefined,'OK'); w.show();";
    let result = run(
        project.clone(),
        source,
        quick,
        total,
        |requests, responses, _| {
            let UiRequest::Dialog { id, .. } = requests.recv().unwrap() else {
                panic!("Expected dialog")
            };
            responses.send(UiResponse::Close { dialog_id: id }).unwrap();
        },
    );
    assert!(result.is_err());
    assert_eq!(*editor.project(), project);
    println!("PASS: native ScriptUI Close cancels complete candidate");

    let result = run(
        project.clone(),
        "alert('Disconnect');",
        quick,
        total,
        |requests, _, _| {
            requests.recv().unwrap();
        },
    );
    assert!(result.is_err());
    assert!(!automation_process::worker_active());
    println!("PASS: UI disconnect rolls back and releases worker slot");

    // Invalid commands cannot catch an error and commit an earlier mutation.
    let result = run(
        project.clone(),
        "app.project.activeItem.layer(1).name='partial'; try { new File('/tmp/no-script-capability'); } catch(e) {}",
        quick,
        total,
        no_ui,
    );
    assert!(result.is_err());
    assert_eq!(*editor.project(), project);
    println!("PASS: forbidden OS API cannot commit partial edits");
    let mut image_editor = Editor::default();
    image_editor
        .execute(Command::AddContent {
            content: libre_effects_core::Content::Image {
                png: "A".repeat(9 * 1024 * 1024).into(),
            },
            width: 8.,
            height: 8.,
            name: "Large shared image".into(),
        })
        .unwrap();
    let image_project = image_editor.project().clone();
    let result = run(
        image_project.clone(),
        "$.writeln('No image stripping');",
        quick,
        total,
        no_ui,
    );
    assert!(result.unwrap_err().contains("size limit"));
    assert_eq!(*image_editor.project(), image_project);
    assert!(!automation_process::worker_active());
    println!("PASS: oversized raw image transport rejects unchanged before spawning");
    let mut shared_editor = Editor::default();
    shared_editor
        .execute(Command::AddContent {
            content: libre_effects_core::Content::Image { png: "YWJj".into() },
            name: "Shared image".into(),
            width: 8.,
            height: 8.,
        })
        .unwrap();
    shared_editor.execute(Command::DuplicateLayer(1)).unwrap();
    let shared = shared_editor.project().clone();
    let baseline_bytes = libre_effects_core::project_file::encode(&shared, None).unwrap();
    let unchanged = run(
        shared.clone(),
        "$.writeln('Shared media unchanged');",
        quick,
        total,
        no_ui,
    )
    .unwrap();
    assert_eq!(unchanged.project.media_sharing(), shared.media_sharing());
    assert_eq!(
        libre_effects_core::project_file::encode(&unchanged.project, None).unwrap(),
        baseline_bytes
    );
    let mut expected = shared.clone();
    let original_id = expected.composition().layers()[0].id();
    expected
        .apply_automation_command(
            expected.active_composition_id(),
            Command::DuplicateLayer(original_id),
        )
        .unwrap();
    expected
        .apply_automation_command(
            expected.active_composition_id(),
            Command::RemoveLayer(original_id),
        )
        .unwrap();
    let replaced = run(
        shared.clone(),
        "var l=app.project.activeItem.layer(1); l.duplicate(); l.remove();",
        quick,
        total,
        no_ui,
    )
    .unwrap();
    assert_eq!(replaced.project.media_sharing(), expected.media_sharing());
    assert_eq!(
        libre_effects_core::project_file::encode(&replaced.project, None).unwrap(),
        libre_effects_core::project_file::encode(&expected, None).unwrap()
    );
    assert_eq!(
        libre_effects_core::project_file::encode(&shared, None).unwrap(),
        baseline_bytes
    );
    println!(
        "PASS: shared image aliases survive no-op, duplicate/delete and exact native serialization"
    );

    use automation_process::{PipeEvent, WorkerProcess};
    let executable = std::env::current_exe().unwrap();
    for flag in ["--fixture-empty", "--fixture-crash"] {
        let mut child = WorkerProcess::spawn(&executable, flag, 64).unwrap();
        let started = Instant::now();
        loop {
            if matches!(child.receive().unwrap(), Some(PipeEvent::Eof)) {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
        }
    }
    let mut child = WorkerProcess::spawn(&executable, "--fixture-oversized", 64).unwrap();
    loop {
        if let Some(PipeEvent::Failed(error)) = child.receive().unwrap() {
            assert!(error.contains("oversized"));
            break;
        }
    }
    drop(child);
    for flag in ["--fixture-stall", "--fixture-flood"] {
        let mut child = WorkerProcess::spawn(&executable, flag, 2 * 1024 * 1024).unwrap();
        child.send(vec![b'a'; 1024 * 1024]).unwrap();
        std::thread::sleep(Duration::from_millis(60));
        let started = Instant::now();
        drop(child);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "Blocked pipe cleanup did not return promptly"
        );
    }
    println!("PASS: no-output, crash, oversize frame, blocked stdin and full stdout cleanup");
    use libre_effects_ae_expressions as ae;
    let snapshot = ae::CompositionSnapshot {
        id: ae::CompositionId(1),
        width: 1920,
        height: 1080,
        duration: 5.,
        frame_rate: ae::FrameRate {
            numerator: 30,
            denominator: 1,
        },
        time: 1.,
        sources: vec![
            "[value[0] + thisComp.width/2, value[1]]".into(),
            "value + time * 3".into(),
        ],
        layers: vec![ae::LayerSnapshot {
            id: ae::LayerId(1),
            name: "Expression fixture".into(),
            start_time: 0.,
            in_point: 0.,
            out_point: 5.,
            position: ae::PropertySnapshot {
                authored_value: ae::PropertyValue::Vector2([1., 2.]),
                expression: Some(ae::ExpressionProgram {
                    source_id: ae::ExpressionSourceId(0),
                    enabled: true,
                    local_bindings: vec![],
                }),
            },
            scale: ae::PropertySnapshot::authored(ae::PropertyValue::Vector2([100., 100.])),
            opacity: ae::PropertySnapshot {
                authored_value: ae::PropertyValue::Scalar(80.),
                expression: Some(ae::ExpressionProgram {
                    source_id: ae::ExpressionSourceId(1),
                    enabled: true,
                    local_bindings: vec![],
                }),
            },
            source_text: None,
            masks: vec![],
            sliders: vec![],
            markers: vec![],
        }],
    };
    let original_snapshot = snapshot.clone();
    let roots = [
        ae::ExpressionProperty::Position,
        ae::ExpressionProperty::Opacity,
    ]
    .map(|property| ae::PropertyAddress {
        composition: snapshot.id,
        layer: ae::LayerId(1),
        property,
    });
    let values = automation_process::evaluate_expressions(
        &snapshot,
        &roots,
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(
        values.get(&roots[0]),
        Some(&ae::PropertyValue::Vector2([961., 2.]))
    );
    assert_eq!(values.get(&roots[1]), Some(&ae::PropertyValue::Scalar(83.)));
    assert_eq!(snapshot, original_snapshot);
    println!("PASS: expression batch evaluates out of process without changing authored values");

    let mut hostile = snapshot.clone();
    hostile.sources[1] = "while(true) {}".into();
    let failure = automation_process::evaluate_expressions(
        &hostile,
        &roots,
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap_err();
    assert_eq!(failure.kind, ae::EvaluationErrorKind::Budget);
    assert!(!automation_process::worker_active());
    println!("PASS: expression budget failure returns no partial frame and reaps worker");

    // BigInt exponentiation is a native path, not an interruptible JS loop.
    // Use a 150 ms process deadline, below the native work on this fixture.
    hostile.sources[1] = "3n ** 33554432n; value".into();
    let started = Instant::now();
    let failure = automation_process::evaluate_expressions_with_timeout(
        &hostile,
        &roots,
        Arc::new(AtomicBool::new(false)),
        Duration::from_millis(150),
    )
    .unwrap_err();
    assert_eq!(failure.kind, ae::EvaluationErrorKind::Budget, "{failure}");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(
        failure.message.contains("worker execution time limit"),
        "Expected independent process watchdog: {failure}"
    );
    assert!(!automation_process::worker_active());
    println!(
        "PASS: expression native BigInt exponentiation is stopped by the independent process watchdog"
    );

    let canceled = Arc::new(AtomicBool::new(true));
    assert_eq!(
        automation_process::evaluate_expressions(&snapshot, &roots, canceled)
            .unwrap_err()
            .kind,
        ae::EvaluationErrorKind::Canceled
    );
    let waiting_snapshot = snapshot.clone();
    let waiting_roots = roots.clone();
    run(
        project.clone(),
        "alert('Hold the single worker permit');",
        quick,
        total,
        move |requests, responses, _| {
            requests.recv().unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            let scheduled = cancel.clone();
            let thread = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(80));
                scheduled.store(true, Ordering::Relaxed);
            });
            let error =
                automation_process::evaluate_expressions(&waiting_snapshot, &waiting_roots, cancel)
                    .unwrap_err();
            assert_eq!(error.kind, ae::EvaluationErrorKind::Canceled);
            thread.join().unwrap();
            responses.send(UiResponse::AlertDismissed).unwrap();
        },
    )
    .unwrap();
    assert!(!automation_process::worker_active());
    println!(
        "PASS: expression waiting for a modal JSX worker is cancellable and does not launch a second child"
    );
    let mut preceding_snapshot = snapshot.clone();
    preceding_snapshot.sources[1] = "while(true) {}".into();
    let preceding_roots = roots.clone();
    let preceding = std::thread::spawn(move || {
        automation_process::evaluate_expressions(
            &preceding_snapshot,
            &preceding_roots,
            Arc::new(AtomicBool::new(false)),
        )
    });
    let started = Instant::now();
    while !automation_process::worker_active() {
        assert!(started.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(1));
    }
    let outcome = run(
        project.clone(),
        "app.project.activeItem.layer(1).name='After preview worker';",
        quick,
        total,
        no_ui,
    )
    .unwrap();
    assert_eq!(
        outcome.project.composition().layer(1).unwrap().name(),
        "After preview worker"
    );
    assert_eq!(
        preceding.join().unwrap().unwrap_err().kind,
        ae::EvaluationErrorKind::Budget
    );
    assert!(!automation_process::worker_active());
    println!(
        "PASS: JSX selected during an expression batch waits for its worker permit and then succeeds"
    );
}
