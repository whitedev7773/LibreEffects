use super::*;
use libre_effects_core::{Command, Content, Editor, Property};
use std::sync::mpsc;
use std::thread;

fn project() -> Project {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Template".into(),
                font_size: 32.0,
            },
            name: "LyricLayer".into(),
            width: 300.0,
            height: 80.0,
        })
        .unwrap();
    editor.project().clone()
}
fn run(source: &str) -> Result<ScriptOutcome, String> {
    let (tx, _rx) = mpsc::channel();
    let (_tx, rx) = mpsc::channel();
    run_script(
        project(),
        vec![1],
        0,
        source,
        tx,
        rx,
        Arc::new(AtomicBool::new(false)),
    )
}
fn find<'a>(node: &'a UiNode, text: &str) -> Option<&'a UiNode> {
    if node.text == text {
        return Some(node);
    }
    node.children.iter().find_map(|node| find(node, text))
}

#[test]
fn real_js_regex_unicode_closures_and_stable_layer_identity() {
    let outcome = run(r#"#target aftereffects
      (function () {
        var comp = app.project.activeItem;
        if (!(comp instanceof CompItem) || comp !== app.project.item(1)) throw Error('bad identity');
        var template = comp.layer('LyricLayer'), copy = template.duplicate();
        copy.moveToBeginning();
        if (template.index !== 2 || copy.index !== 1 || template.id === copy.id) throw Error('bad stable IDs');
        var getName = (function(prefix) { return function(x) {return prefix + x.replace(/\s+/g, ' ')}; })('가사: ');
        copy.name = getName('日本語   👋');
        var source = copy.property('ADBE Text Properties').property('ADBE Text Document');
        var document = source.value;
        document.text = '日本語\r한국어\n👋';
        source.setValue(document);
        var position = copy.property('ADBE Transform Group').property('ADBE Position');
        if (position.propertyValueType !== PropertyValueType.TwoD_SPATIAL) throw Error('must be 2D');
        position.setValueAtTime(0,[20,40]);
        position.setValueAtTime(1,[80,100]);
        position.setInterpolationTypeAtKey(1,KeyframeInterpolationType.LINEAR);
        var opacity = copy.transform.opacity;
        opacity.setValue(60);
        copy.outPoint = 2;
        copy.property('Marker').setValueAtTime(1,new MarkerValue('Focus'));
        console.log(comp.numLayers,position.numKeys,position.keyTime(2));
      })();
    "#).unwrap();
    assert_eq!(outcome.project.composition().layers().len(), 2);
    let copy = &outcome.project.composition().layers()[0];
    assert_eq!(copy.name(), "가사: 日本語 👋");
    assert_eq!(copy.source_text_at(0), Some("日本語\r한국어\n👋"));
    assert_eq!(
        copy.property(Property::PositionX)
            .expect("known scalar fixture property")
            .value_at(15),
        50.0
    );
    assert_eq!(
        copy.property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(0),
        60.0
    );
    assert_eq!(copy.out_frame(150), 60);
    assert_eq!(copy.markers()[0].name(), "Focus");
    assert_eq!(outcome.output, vec!["2 2 1"]);
    assert_eq!(outcome.selected_layer_ids, vec![1]);
}

#[test]
fn jsx_reads_scale_keys_as_detached_two_component_values() {
    let result = run(r#"
        var p=app.project.activeItem.layer(1).transform.scale;
        p.setValueAtTime(0,[100,50]);p.setValueAtTime(1,[200,25]);
        if(p.numKeys!==2 || p.keyTime(2)!==1)throw Error('scale key identity');
        var v=p.keyValue(2);
        if(v.length!==2 || v[0]!==200 || v[1]!==25)throw Error('scale key value');
        v[0]=0;
        if(p.keyValue(2)[0]!==200)throw Error('scale key read aliased source');
    "#)
    .unwrap();
    let layer = result.project.composition().layer(1).unwrap();
    assert_eq!(
        layer.property(Property::ScaleX).unwrap().value_at(30),
        200.0
    );
    assert_eq!(layer.property(Property::ScaleY).unwrap().value_at(30), 25.0);
}

#[test]
fn scriptui_preserves_callbacks_unicode_live_status_and_modal_confirm() {
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        run_script(
            project(),
            vec![1],
            0,
            r#"
      var count = 0, win = new Window('dialog','Synthetic lyrics');
      win.minimumSize = [720,620];
      var input = win.add('edittext',undefined,'',{multiline:true,wantReturn:true});
      input.minimumSize = [680,350];
      var status = win.add('statictext',undefined,'Waiting');
      var sample = win.add('button',undefined,'Sample');
      var ok = win.add('button',undefined,'Apply');
      ok.preferredSize = [150,34];
      var cancel = win.add('button',undefined,'Cancel');
      input.onChanging = function () { count++; status.text = 'Changed ' + count + ': ' + input.text; };
      sample.onClick = function () { if(confirm('Replace input?')) { input.text = '日本語\n한국어'; input.onChanging(); input.active = true; } };
      ok.onClick = function () { app.project.activeItem.layer(1).name = input.text.replace(/\n/g,' / '); win.close(1); };
      win.onShow = function () { status.text = 'Ready'; input.active = true; };
      win.defaultElement = ok; win.cancelElement = cancel;
      win.show(); console.log('callbacks',count);
    "#,
            request_tx,
            response_rx,
            Arc::new(AtomicBool::new(false)),
        )
    });
    let UiRequest::Dialog { id, root, .. } =
        request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
    else {
        panic!()
    };
    assert!(find(&root, "Ready").is_some());
    assert_eq!(root.minimum_size, Some([720.0, 620.0]));
    assert_eq!(
        find(&root, "Apply").unwrap().preferred_size,
        Some([150.0, 34.0])
    );
    let input = root
        .children
        .iter()
        .find(|n| n.kind == "edittext")
        .unwrap()
        .id;
    let field = root.children.iter().find(|n| n.id == input).unwrap();
    assert!(field.active);
    assert_eq!(field.minimum_size, Some([680.0, 350.0]));
    let initial_focus = field.focus_request;
    assert!(initial_focus > 0);
    response_tx
        .send(UiResponse::Change {
            dialog_id: id,
            control_id: input,
            text: "First 👋".into(),
        })
        .unwrap();
    let UiRequest::Dialog { root, .. } = request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
    else {
        panic!()
    };
    assert!(find(&root, "Changed 1: First 👋").is_some());
    assert_eq!(
        find(&root, "First 👋").unwrap().focus_request,
        initial_focus
    );
    response_tx
        .send(UiResponse::Click {
            dialog_id: id,
            control_id: find(&root, "Sample").unwrap().id,
        })
        .unwrap();
    assert!(matches!(
        request_rx.recv_timeout(Duration::from_secs(10)).unwrap(),
        UiRequest::Confirm { .. }
    ));
    response_tx
        .send(UiResponse::Confirm { value: true })
        .unwrap();
    let UiRequest::Dialog { root, .. } = request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
    else {
        panic!()
    };
    assert!(find(&root, "Changed 2: 日本語\n한국어").is_some());
    assert!(find(&root, "日本語\n한국어").unwrap().focus_request > initial_focus);
    response_tx
        .send(UiResponse::Click {
            dialog_id: id,
            control_id: find(&root, "Apply").unwrap().id,
        })
        .unwrap();
    let outcome = worker.join().unwrap().unwrap();
    assert_eq!(
        outcome.project.composition().layer(1).unwrap().name(),
        "日本語 / 한국어"
    );
    assert_eq!(outcome.output, vec!["callbacks 2"]);
}

#[test]
fn scriptui_focus_assignments_are_ordered_even_when_already_active() {
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        run_script(
            project(),
            vec![1],
            0,
            r#"
            var win = new Window('dialog','Focus sequence');
            var first = win.add('edittext',undefined,'First');
            var second = win.add('edittext',undefined,'Second');
            var repeat = win.add('button',undefined,'Repeat');
            var clear = win.add('button',undefined,'Clear');
            var done = win.add('button',undefined,'Done');
            first.active = true; second.active = true; first.active = true;
            repeat.onClick = function () { first.active = true; };
            clear.onClick = function () { first.active = false; };
            done.onClick = function () { win.close(1); };
            win.show();
        "#,
            request_tx,
            response_rx,
            Arc::new(AtomicBool::new(false)),
        )
    });
    let receive = || {
        let UiRequest::Dialog { id, root, .. } =
            request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
        else {
            panic!("Expected a dialog");
        };
        (id, root)
    };
    let (id, root) = receive();
    let first = find(&root, "First").unwrap();
    let second = find(&root, "Second").unwrap();
    assert!(first.active && second.active);
    assert_eq!(first.focus_request, 3);
    assert_eq!(second.focus_request, 2);
    let repeat = find(&root, "Repeat").unwrap().id;
    for expected in [4, 5] {
        response_tx
            .send(UiResponse::Click {
                dialog_id: id,
                control_id: repeat,
            })
            .unwrap();
        let (_, root) = receive();
        assert_eq!(find(&root, "First").unwrap().focus_request, expected);
        assert_eq!(find(&root, "Second").unwrap().focus_request, 2);
    }
    response_tx
        .send(UiResponse::Click {
            dialog_id: id,
            control_id: find(&root, "Clear").unwrap().id,
        })
        .unwrap();
    let (_, root) = receive();
    assert!(!find(&root, "First").unwrap().active);
    assert_eq!(find(&root, "First").unwrap().focus_request, 5);
    response_tx
        .send(UiResponse::Click {
            dialog_id: id,
            control_id: find(&root, "Done").unwrap().id,
        })
        .unwrap();
    assert!(worker.join().unwrap().is_ok());
}

#[test]
fn canceled_dialog_rejects_even_pre_dialog_mutations() {
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        run_script(
            project(),
            vec![1],
            0,
            "app.project.activeItem.layer(1).name='Changed'; new Window('dialog','Cancel').show();",
            request_tx,
            response_rx,
            Arc::new(AtomicBool::new(false)),
        )
    });
    let UiRequest::Dialog { id, .. } = request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
    else {
        panic!()
    };
    response_tx
        .send(UiResponse::Close { dialog_id: id })
        .unwrap();
    assert_eq!(worker.join().unwrap().unwrap_err(), "Script canceled");
}

#[test]
fn swallowed_unsupported_and_invalid_calls_still_reject_whole_candidate() {
    for code in [
        "try { app.project.activeItem.layer(1).startTime = 0.001; } catch(e) {}",
        "try { app.project.activeItem.layer(1).label = 17; } catch(e) {}",
        "try { app.project.activeItem.layer(1).transform.position.setValue([1,2,0]); } catch(e) {}",
        "try { app.project.activeItem.layer(1).threeDLayer = 'yes'; } catch(e) {}",
        "try { app.executeCommand(12); } catch(e) {}",
        "try { app.project.activeItem.layer(1).property('Marker').value; } catch(e) {}",
        "try { new File('/tmp/forbidden'); } catch(e) {}",
        "try { new Window('palette','Unsupported'); } catch(e) {}",
        "try { new Window('dialog','Unsupported').add('dropdownlist'); } catch(e) {}",
        "try { app.project.item(0); } catch(e) {}",
        "try { app.project.activeItem.layer(1).name=' Bad '; } catch(e) {}",
    ] {
        let code = format!("app.project.activeItem.layer(1).name='Changed'; {code}");
        assert!(run(&code).is_err(), "silently committed: {code}");
    }
}

#[test]
fn three_dimensional_value_type_is_truthful_and_not_coerced() {
    let outcome =
        run("console.log(app.project.activeItem.layer(1).transform.position.propertyValueType);")
            .unwrap();
    assert_eq!(outcome.output, vec!["TwoD_SPATIAL"]);
    let error = run("if(app.project.activeItem.layer(1).transform.position.propertyValueType !== PropertyValueType.ThreeD_SPATIAL) throw Error('Template requires real 3D');").unwrap_err();
    assert!(error.contains("Template requires real 3D"));
}

#[test]
fn source_directives_undefined_runtime_and_async_work_reject() {
    for source in [
        "#include 'secret.jsx'",
        "#targetengine persistent",
        "#target photoshop",
        "var = ;",
        "throw Error('stop');",
        "Promise.resolve().then(function(){app.project.activeItem.layer(1).name='Delayed';});",
    ] {
        assert!(run(source).is_err(), "accepted {source}");
    }
    assert!(run(&" ".repeat(MAX_SCRIPT_BYTES + 1)).is_err());
    assert!(
        run("console.log(typeof __le_host,typeof __le_ui);")
            .unwrap()
            .output[0]
            .contains("undefined undefined")
    );
    assert!(run("var privateFromPriorRun = 42;").is_ok());
    assert_eq!(
        run("console.log(typeof privateFromPriorRun);")
            .unwrap()
            .output,
        vec!["undefined"]
    );
}

#[test]
fn infinite_javascript_is_interrupted_without_host_calls() {
    let started = Instant::now();
    let error = run("for (;;) {}").unwrap_err();
    assert!(error.contains("time limit"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(8));
}

#[test]
fn cancel_while_waiting_and_disconnected_ui_fail_promptly() {
    let (request_tx, request_rx) = mpsc::channel();
    let (_response_tx, response_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let worker = thread::spawn(move || {
        run_script(
            project(),
            vec![1],
            0,
            "new Window('dialog','Wait').show();",
            request_tx,
            response_rx,
            worker_cancel,
        )
    });
    request_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap_err(), "Script canceled");
    let (tx, rx) = mpsc::channel();
    drop(rx);
    let (_tx, rx) = mpsc::channel();
    assert!(
        run_script(
            project(),
            vec![1],
            0,
            "alert('Disconnected');",
            tx,
            rx,
            Arc::new(AtomicBool::new(false))
        )
        .unwrap_err()
        .contains("closed")
    );
}

#[test]
fn stale_ui_response_and_disabled_button_are_rejected() {
    for stale in [true, false] {
        let (request_tx, request_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            run_script(
                project(),
                vec![1],
                0,
                "var w=new Window('dialog','W');var b=w.add('button',undefined,'Disabled');b.enabled=false;w.show();",
                request_tx,
                response_rx,
                Arc::new(AtomicBool::new(false)),
            )
        });
        let UiRequest::Dialog { id, root, .. } =
            request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
        else {
            panic!()
        };
        response_tx
            .send(UiResponse::Click {
                dialog_id: if stale { id + 100 } else { id },
                control_id: root.children[0].id,
            })
            .unwrap();
        assert!(
            worker
                .join()
                .unwrap()
                .unwrap_err()
                .contains("Stale or invalid")
        );
    }
}

#[test]
fn waiting_for_user_does_not_consume_execution_slice() {
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        run_script(
            project(),
            vec![1],
            0,
            "var w=new Window('dialog','Wait');var b=w.add('button',undefined,'OK');w.defaultElement=b;w.show();console.log('resumed');",
            request_tx,
            response_rx,
            Arc::new(AtomicBool::new(false)),
        )
    });
    let UiRequest::Dialog { id, root, .. } =
        request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
    else {
        panic!()
    };
    thread::sleep(EXECUTION_SLICE + Duration::from_millis(100));
    response_tx
        .send(UiResponse::Click {
            dialog_id: id,
            control_id: root.children[0].id,
        })
        .unwrap();
    assert_eq!(worker.join().unwrap().unwrap().output, vec!["resumed"]);
}

#[test]
fn memory_stack_output_and_ui_limits_reject_without_crashing() {
    for source in [
        "var x=[];for(var i=0;i<1000000;i++)x.push('x'.repeat(1024));",
        "function recurse(){return recurse()+1;}recurse();",
        "console.log('x'.repeat(65537));",
        "var w=new Window('dialog','Many');for(var i=0;i<256;i++)w.add('button');",
    ] {
        assert!(run(source).is_err());
    }
}

#[test]
fn mixed_project_items_keep_type_identity_and_one_based_lookup() {
    let mut editor = Editor::default();
    editor
        .execute(Command::NewProjectFolder {
            name: "Media".into(),
            parent: None,
        })
        .unwrap();
    editor
        .execute(Command::ImportAsset {
            content: Content::Image { png: "YWJj".into() },
            width: 20.0,
            height: 10.0,
            name: "Image".into(),
            folder: Some(1),
            frame: None,
        })
        .unwrap();
    let (tx, _rx) = mpsc::channel();
    let (_tx, rx) = mpsc::channel();
    let outcome = run_script(editor.project().clone(),vec![],0,r#"
      if(app.project.numItems !== 3 || app.project.items.length !== 3) throw Error('missing project items');
      var kinds=[];
      for(var i=1;i<=app.project.numItems;i++) {
        var item=app.project.item(i);
        if(item!==app.project.items[i]) throw Error('unstable item wrapper');
        if(item instanceof CompItem) kinds.push('comp');
        else if(item instanceof FootageItem) kinds.push('footage');
        else if(item instanceof FolderItem) kinds.push('folder');
        else throw Error('wrong project item type');
      }
      console.log(kinds.sort().join(','));
    "#,tx,rx,Arc::new(AtomicBool::new(false))).unwrap();
    assert_eq!(outcome.output, vec!["comp,folder,footage"]);
}

#[test]
fn scriptui_identity_edittext_and_empty_output_limits_cannot_be_bypassed() {
    for source in [
        "var w=new Window('dialog','W');try{w.id=100;}catch(e){}",
        "var w=new Window('dialog','W');var t=w.add('edittext',undefined,'x'.repeat(16385));w.show();",
        "for(var i=0;i<1025;i++)console.log('');",
        "var w=new Window('dialog','W');try{w.onChange=function(){};}catch(e){}",
        "try{PropertyValueType.Unsupported;}catch(e){}",
    ] {
        assert!(run(source).is_err(), "accepted {source}");
    }
}

#[test]
fn native_close_cannot_be_reinterpreted_as_success_by_cancel_callback() {
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        run_script(
            project(),
            vec![1],
            0,
            "var w=new Window('dialog','Close');var b=w.add('button',undefined,'Cancel');w.cancelElement=b;b.onClick=function(){w.close(1);};w.show();",
            request_tx,
            response_rx,
            Arc::new(AtomicBool::new(false)),
        )
    });
    let UiRequest::Dialog { id, .. } = request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
    else {
        panic!()
    };
    response_tx
        .send(UiResponse::Close { dialog_id: id })
        .unwrap();
    assert_eq!(worker.join().unwrap().unwrap_err(), "Script canceled");
}

#[test]
fn unmatched_undo_groups_and_repeated_window_show_reject() {
    for source in [
        "app.beginUndoGroup('forgot');app.project.activeItem.layer(1).name='Changed';",
        "try{app.endUndoGroup();}catch(e){}",
        "var w=new Window('dialog','Once');w.onShow=function(){w.close(1);};w.show();try{w.show();}catch(e){}",
    ] {
        assert!(run(source).is_err(), "accepted {source}");
    }
    assert!(run("app.beginUndoGroup('outer');app.beginUndoGroup('inner');app.endUndoGroup();app.endUndoGroup();").is_ok());
}

#[test]
fn caught_callback_exception_rejects_the_whole_transaction() {
    let error = run("var w=new Window('dialog','Show');w.onShow=function(){app.project.activeItem.layer(1).name='Changed';throw Error('onShow error');};try{w.show();}catch(e){}").unwrap_err();
    assert!(error.contains("onShow callback failed"));
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        run_script(
            project(),
            vec![1],
            0,
            "var w=new Window('dialog','Click');var b=w.add('button',undefined,'Apply');b.onClick=function(){app.project.activeItem.layer(1).name='Changed';throw Error('failed after mutation');};try{w.show();}catch(e){}",
            request_tx,
            response_rx,
            Arc::new(AtomicBool::new(false)),
        )
    });
    let UiRequest::Dialog { id, root, .. } =
        request_rx.recv_timeout(Duration::from_secs(10)).unwrap()
    else {
        panic!()
    };
    response_tx
        .send(UiResponse::Click {
            dialog_id: id,
            control_id: root.children[0].id,
        })
        .unwrap();
    assert!(
        worker
            .join()
            .unwrap()
            .unwrap_err()
            .contains("onClick callback failed")
    );
}

#[test]
fn inactive_composition_metadata_is_available_but_unbound_time_samples_reject() {
    let mut editor = Editor::default();
    editor.replace_project(project()).unwrap();
    for property in [Property::PositionX, Property::PositionY] {
        editor
            .execute(Command::ToggleKeyframe {
                id: 1,
                property,
                frame: 0,
            })
            .unwrap();
    }
    editor.execute(Command::NewComposition).unwrap();
    for (source, success) in [
        (
            "console.log(app.project.item(1).layer(1).transform.position.numKeys, app.project.item(1).layer(1).transform.position.propertyValueType);",
            true,
        ),
        (
            "console.log(app.project.item(1).layer(1).transform.opacity.value);",
            true,
        ),
        ("try{app.project.item(1).time;}catch(e){}", false),
        (
            "try{app.project.item(1).layer(1).transform.position.value;}catch(e){}",
            false,
        ),
    ] {
        let (tx, _rx) = mpsc::channel();
        let (_tx, rx) = mpsc::channel();
        let result = run_script(
            editor.project().clone(),
            vec![],
            0,
            source,
            tx,
            rx,
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(result.is_ok(), success, "{source}: {result:?}");
    }
}

#[test]
fn async_rejections_without_queued_jobs_cannot_commit_partial_work() {
    for source in [
        "app.project.activeItem.layer(1).name='Changed';Promise.reject(Error('lost'));",
        "app.project.activeItem.layer(1).name='Changed';(async function(){throw Error('lost');})();",
        "app.project.activeItem.layer(1).name='Changed';Promise.resolve(42);",
        "app.project.activeItem.layer(1).name='Changed';new Promise(function(resolve){resolve(1);});",
    ] {
        assert!(
            run(source).unwrap_err().contains("Asynchronous"),
            "{source}"
        );
    }
}

fn timing_project() -> Project {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "Synthetic 60 fps template".into(),
            width: 1920,
            height: 886,
            fps: 60.into(),
            duration: 21_600,
            display_start: 0,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Three\rSynthetic\rLines".into(),
                font_size: 32.0,
            },
            width: 300.0,
            height: 100.0,
            name: "Template".into(),
        })
        .unwrap();
    // An independently authored one-frame origin, with the same timing shape as
    // the inspected template. Trim and source origin are deliberately separate.
    editor
        .execute(Command::SetLayerRange {
            id: 1,
            start: 0,
            end: 698,
        })
        .unwrap();
    editor
        .execute(Command::SetLayerStart { id: 1, frame: 1 })
        .unwrap();
    editor.project().clone()
}
fn run_on(project: Project, source: &str) -> Result<ScriptOutcome, String> {
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

#[test]
fn jsx_independent_origin_and_labels_enable_frame_aligned_lyric_timing_sequence() {
    let original = timing_project();
    let outcome=run_on(original.clone(),r#"
        var comp=app.project.activeItem, template=comp.layer('Template');
        if (template.startTime !== comp.frameDuration || template.inPoint !== comp.frameDuration) throw Error('one frame origin');
        var times=[0,1,2], copies=[];
        app.beginUndoGroup('Independent timing');
        for(var i=0;i<times.length;i++) {
            var copy=template.duplicate();copy.name='Line '+i;
            var text=copy.property('Source Text').value;text.text='A\rB\r한글';copy.property('Source Text').setValue(text);
            copy.startTime=times[i];copy.outPoint=times[i]+5;
            if(i>=times.length-2) copy.label=4;
            copy.property('Marker').setValueAtTime(times[i],new MarkerValue('Focus'));
            copy.property('Marker').setValueAtTime(times[i]+1,new MarkerValue('Hide'));
            copy.property('Marker').setValueAtTime(times[i]+2,new MarkerValue('End'));
            copies.push(copy);
        }
        app.endUndoGroup();
        console.log(copies[0].startTime,copies[1].startTime,copies[2].label);
    "#).unwrap();
    assert_eq!(outcome.output, vec!["0 1 4"]);
    assert_eq!(outcome.project.composition().layers().len(), 4);
    assert_eq!(
        outcome.project.composition().layer(1),
        original.composition().layer(1)
    );
    for index in 0..3 {
        let layer = outcome
            .project
            .composition()
            .layers()
            .iter()
            .find(|l| l.name() == format!("Line {index}"))
            .unwrap();
        assert_eq!(layer.start_frame(), index * 60);
        assert_eq!(layer.in_frame(), index as u32 * 60);
        assert_eq!(layer.out_frame(21_600), index as u32 * 60 + 300);
        assert_eq!(layer.label_index(), if index == 0 { 0 } else { 4 });
        assert_eq!(
            layer.color(),
            original.composition().layer(1).unwrap().color()
        );
        assert_eq!(
            layer
                .markers()
                .iter()
                .map(|m| (m.frame(), m.name()))
                .collect::<Vec<_>>(),
            vec![
                (index as u32 * 60, "Focus"),
                (index as u32 * 60 + 60, "Hide"),
                (index as u32 * 60 + 120, "End")
            ]
        );
        assert!(matches!(layer.content(),Content::Text {text,..} if text=="A\rB\r한글"));
    }
    let mut editor = Editor::default();
    editor.replace_project(original.clone()).unwrap();
    editor.clear_history();
    editor
        .commit_automation_project(outcome.project.clone())
        .unwrap();
    editor.undo();
    assert_eq!(editor.project(), &original);
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(editor.project(), &outcome.project);
    let encoded = libre_effects_core::project_file::encode(editor.project(), None).unwrap();
    assert_eq!(
        libre_effects_core::project_file::decode(&encoded)
            .unwrap()
            .project,
        *editor.project()
    );
}

#[test]
fn jsx_preserves_exact_source_endpoints_through_edit_reopen_and_history() {
    let original = project();
    let outcome = run_on(
        original.clone(),
        r#"
        var l=app.project.activeItem.layer(1);
        l.inPoint=-1.125; l.outPoint=20.025;
        if(l.inPoint!==-1.125 || l.outPoint!==20.025)throw Error('source endpoint rounded');
        l.inPoint=-0.75;
        if(l.outPoint!==20.025)throw Error('other endpoint lost');
    "#,
    )
    .unwrap();
    let layer = outcome.project.composition().layer(1).unwrap();
    assert_eq!(layer.in_frame_sample(), -22.5);
    assert_eq!(layer.out_frame_sample(150), 600.75);
    assert_eq!((layer.in_frame(), layer.out_frame(150)), (0, 150));
    assert_eq!(layer.start_frame(), 0);
    let saved = libre_effects_core::project_file::decode(
        &libre_effects_core::project_file::encode(&outcome.project, None).unwrap(),
    )
    .unwrap()
    .project;
    assert_eq!(saved, outcome.project);
    assert_eq!(
        run_on(
            saved.clone(),
            "var l=app.project.activeItem.layer(1);l.inPoint=l.inPoint;l.outPoint=l.outPoint;"
        )
        .unwrap()
        .project,
        saved
    );
    let mut editor = Editor::default();
    editor.replace_project(original.clone()).unwrap();
    editor.clear_history();
    editor.commit_automation_project(saved.clone()).unwrap();
    editor.undo();
    assert_eq!(editor.project(), &original);
    editor.redo();
    assert_eq!(editor.project(), &saved);
    for invalid in ["NaN", "Infinity", "-2", "1e30"] {
        let code = format!(
            "var l=app.project.activeItem.layer(1);l.name='Draft';try{{l.outPoint={invalid};}}catch(e){{}}"
        );
        assert!(run_on(saved.clone(), &code).is_err(), "accepted {invalid}");
    }
}

#[test]
fn jsx_trimming_and_start_assignment_are_distinct_and_origin_no_op_is_exact() {
    let original = timing_project();
    let no_op = run_on(
        original.clone(),
        "var l=app.project.activeItem.layer(1);l.startTime=l.startTime;l.label=l.label;",
    )
    .unwrap();
    assert_eq!(no_op.project, original);
    let outcome = run_on(
        original,
        r#"
        var l=app.project.activeItem.layer(1), step=l.containingComp.frameDuration;
        l.inPoint=10*step;
        if (Math.abs(l.startTime-step)>1e-9) throw Error('trim changed origin');
        l.startTime=60*step;
        if (Math.abs(l.inPoint-69*step)>1e-9) throw Error('wrong trim shift');
        l.outPoint=120*step;
        if (Math.abs(l.startTime-60*step)>1e-9) throw Error('out trim changed origin');
    "#,
    )
    .unwrap();
    let layer = outcome.project.composition().layer(1).unwrap();
    assert_eq!(
        (
            layer.start_frame(),
            layer.in_frame(),
            layer.out_frame(21_600)
        ),
        (60, 69, 120)
    );
}

#[test]
fn jsx_labels_reject_invalid_values_and_out_of_comp_shifts_poison_transaction() {
    for expression in ["-1", "17", "4.5", "NaN", "Infinity", "'4'"] {
        let code = format!(
            "var l=app.project.activeItem.layer(1);l.name='Draft';try{{l.label={expression};}}catch(e){{}}"
        );
        assert!(
            run_on(timing_project(), &code).is_err(),
            "accepted {expression}"
        );
    }
    for origin in ["0.001", "359", "-1", "1e30"] {
        let code = format!(
            "var l=app.project.activeItem.layer(1);l.label=4;try{{l.startTime={origin};}}catch(e){{}}"
        );
        assert!(
            run_on(timing_project(), &code).is_err(),
            "accepted unrepresentable origin {origin}"
        );
    }
}

#[test]
fn jsx_all_label_indices_round_trip_without_changing_rendered_source() {
    for label in 0..=16 {
        let original = timing_project();
        let before = original.composition().layer(1).unwrap().clone();
        let outcome = run_on(
            original,
            &format!("var l=app.project.activeItem.layer(1);l.label={label};console.log(l.label);"),
        )
        .unwrap();
        assert_eq!(outcome.output, vec![label.to_string()]);
        let after = outcome.project.composition().layer(1).unwrap();
        assert_eq!(after.label_index(), label);
        assert_eq!(after.content(), before.content());
        assert_eq!(after.color(), before.color());
        for property in Property::ALL {
            assert_eq!(after.property(property), before.property(property));
        }
        let json = outcome.project.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), outcome.project);
    }
}

#[test]
fn jsx_signed_origin_preserves_positive_trim_offset_when_representable() {
    let outcome=run_on(timing_project(),"var l=app.project.activeItem.layer(1);l.inPoint=2;l.startTime=-1;console.log(l.startTime);").unwrap();
    let layer = outcome.project.composition().layer(1).unwrap();
    assert_eq!(layer.start_frame(), -60);
    assert_eq!(layer.in_frame(), 59);
    assert_eq!(outcome.output, vec!["-1"]);
}

#[test]
fn jsx_numeric_expression_assignment_and_value_reads_use_evaluated_results() {
    let original = timing_project();
    let result=run_on(original.clone(),r#"
        var t=app.project.activeItem.layer(1).transform;
        t.position.expression='[value[0]+20,value[1]-5]';
        t.scale.expression='[50,75]';t.opacity.expression='40';
        if(!t.opacity.expressionEnabled || t.opacity.value!==40) throw Error('not evaluated');
        console.log(t.scale.value.join(','));
        t.opacity.setValue(80);if(t.opacity.value!==40) throw Error('authored edit overrode program');
        t.opacity.expressionEnabled=false;if(t.opacity.value!==80) throw Error('disabled did not use authored');
        t.scale.expression='';if(t.scale.expression!=='') throw Error('clear failed');
    "#).unwrap();
    assert_eq!(result.output, vec!["50,75"]);
    let layer = result.project.composition().layer(1).unwrap();
    assert_eq!(
        layer
            .property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(0),
        80.0
    );
    assert!(!layer.has_enabled_expression(libre_effects_core::ExpressionTarget::Opacity));
    assert_eq!(
        layer
            .property(Property::PositionX)
            .expect("known scalar fixture property"),
        original
            .composition()
            .layer(1)
            .unwrap()
            .property(Property::PositionX)
            .expect("known scalar fixture property")
    );
    assert!(
        layer
            .expression(libre_effects_core::ExpressionTarget::Scale)
            .is_none()
    );
}
#[test]
fn jsx_caught_expression_failure_rejects_draft_and_inactive_sampling_is_not_guessed() {
    assert!(run_on(timing_project(),"var l=app.project.activeItem.layer(1);l.name='Draft';l.transform.opacity.expression='throw Error(\"bad\");';try{l.transform.opacity.value;}catch(e){}").is_err());
    let mut e = Editor::default();
    e.replace_project(timing_project()).unwrap();
    e.execute(Command::NewComposition).unwrap();
    let result = run_on(
        e.project().clone(),
        "var l=app.project.item(1).layer(1);l.transform.position.expression='[time,0]';try{l.transform.position.value;}catch(e){}",
    );
    assert!(result.is_err());
}
