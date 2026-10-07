//! Execute externally supplied JSX once through the production process boundary.
//! This example never embeds or rewrites a user's script. See the adjacent guide.
#[allow(dead_code)]
#[path = "../../../apps/desktop/src/automation_process.rs"]
mod automation_process;

use libre_effects_core::{Command, Editor, project_file};
use libre_effects_editor_model::automation::{self, UiNode, UiRequest, UiResponse};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    #[serde(default)]
    frame: u32,
    #[serde(default)]
    selected_layer_ids: Vec<u64>,
    expected: Expected,
    steps: Vec<Step>,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Expected {
    Success { changed: bool },
    Failure { contains: String },
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    action: Action,
    #[serde(default)]
    required_text: Vec<String>,
    #[serde(default)]
    required_fragments: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Change { selector: Selector, text: String },
    Click { selector: Selector },
    Close {},
    Resize {},
    Confirm { value: bool },
    DismissAlert {},
    Cancel {},
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selector {
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

fn nodes<'a>(node: &'a UiNode, active: bool, output: &mut Vec<&'a UiNode>) {
    let active = active && node.enabled && node.visible;
    if active {
        output.push(node);
        for child in &node.children {
            nodes(child, active, output);
        }
    }
}
fn select(root: &UiNode, selector: &Selector) -> Result<u64, String> {
    let mut live = Vec::new();
    nodes(root, true, &mut live);
    let matches: Vec<_> = live
        .into_iter()
        .filter(|node| {
            node.kind == selector.kind
                && selector.text.as_ref().is_none_or(|text| text == &node.text)
        })
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "Selector {selector:?} matched {} live controls, expected one",
            matches.len()
        ));
    }
    Ok(matches[0].id)
}
fn texts(request: &UiRequest) -> Vec<&str> {
    match request {
        UiRequest::Dialog { title, root, .. } => {
            let mut live = Vec::new();
            nodes(root, true, &mut live);
            std::iter::once(title.as_str())
                .chain(live.into_iter().map(|node| node.text.as_str()))
                .collect()
        }
        UiRequest::Alert { message } | UiRequest::Confirm { message } => vec![message],
    }
}
fn response(request: &UiRequest, step: &Step) -> Result<Option<UiResponse>, String> {
    let present = texts(request);
    for expected in &step.required_text {
        if !present.contains(&expected.as_str()) {
            return Err(format!("Missing exact UI text: {expected:?}"));
        }
    }
    for expected in &step.required_fragments {
        if !present.iter().any(|text| text.contains(expected)) {
            return Err(format!("Missing UI text fragment: {expected:?}"));
        }
    }
    let result = match (request, &step.action) {
        (UiRequest::Dialog { id, root, .. }, Action::Change { selector, text }) => {
            if selector.kind != "edittext" || text.len() > 16_384 {
                return Err("Change requires an edittext selector and at most 16384 bytes".into());
            }
            UiResponse::Change {
                dialog_id: *id,
                control_id: select(root, selector)?,
                text: text.clone(),
            }
        }
        (UiRequest::Dialog { id, root, .. }, Action::Click { selector }) => {
            if selector.kind != "button" {
                return Err("Click requires a button selector".into());
            }
            UiResponse::Click {
                dialog_id: *id,
                control_id: select(root, selector)?,
            }
        }
        (UiRequest::Dialog { id, .. }, Action::Close {}) => UiResponse::Close { dialog_id: *id },
        (UiRequest::Dialog { id, .. }, Action::Resize {}) => UiResponse::Resize { dialog_id: *id },
        (UiRequest::Confirm { .. }, Action::Confirm { value }) => {
            UiResponse::Confirm { value: *value }
        }
        (UiRequest::Alert { .. }, Action::DismissAlert {}) => UiResponse::AlertDismissed,
        (_, Action::Cancel {}) => return Ok(None),
        _ => {
            return Err(format!(
                "Action {:?} does not match the actual UI request",
                step.action
            ));
        }
    };
    Ok(Some(result))
}
fn drive(
    requests: mpsc::Receiver<UiRequest>,
    responses: mpsc::Sender<UiResponse>,
    cancel: Arc<AtomicBool>,
    steps: Vec<Step>,
) -> (Vec<Value>, Result<(), String>) {
    let mut transcript = Vec::new();
    let result = (|| {
        for (index, step) in steps.iter().enumerate() {
            let request = requests
                .recv_timeout(Duration::from_secs(35))
                .map_err(|error| format!("Missing UI request for step {index}: {error}"))?;
            transcript.push(json!({"step":index,"request":request}));
            let reply =
                response(&request, step).map_err(|error| format!("Step {index}: {error}"))?;
            transcript.last_mut().unwrap()["response"] =
                serde_json::to_value(&reply).map_err(|error| error.to_string())?;
            let Some(reply) = reply else {
                if index + 1 != steps.len() {
                    return Err("Cancel must be the final planned step".into());
                }
                cancel.store(true, Ordering::Relaxed);
                return Ok(());
            };
            responses
                .send(reply)
                .map_err(|error| format!("Cannot send UI response: {error}"))?;
        }
        match requests.recv_timeout(Duration::from_secs(35)) {
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                Err("Worker did not finish after the final UI step".into())
            }
            Ok(request) => {
                transcript.push(json!({"unexpected_request":request}));
                Err("Unexpected additional UI request (preserved in transcript)".into())
            }
        }
    })();
    (transcript, result)
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut input = Vec::new();
    fs::File::open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .take(limit as u64 + 1)
        .read_to_end(&mut input)
        .map_err(|error| error.to_string())?;
    if input.len() > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()));
    }
    Ok(input)
}
fn write_new(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|error| error.to_string())
}
fn arguments() -> Result<[PathBuf; 4], String> {
    let mut values: [Option<PathBuf>; 4] = Default::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        let index = match flag.to_str() {
            Some("--project") => 0, Some("--script") => 1, Some("--plan") => 2, Some("--output") => 3,
            _ => return Err("Usage: external_script_harness --project FILE.lep --script FILE.jsx --plan PLAN.json --output NEW_DIRECTORY".into()),
        };
        if values[index].is_some() {
            return Err("Duplicate argument".into());
        }
        values[index] = Some(args.next().ok_or("Missing argument path")?.into());
    }
    if values.iter().any(Option::is_none) {
        return Err("All four input/output arguments are required".into());
    }
    Ok(values.map(Option::unwrap))
}
fn run() -> Result<(), String> {
    let [project_path, script_path, plan_path, output] = arguments()?;
    let input = read_bounded(&project_path, 64 * 1024 * 1024)?;
    let decoded = project_file::decode(&input)?;
    let source = String::from_utf8(read_bounded(&script_path, automation::MAX_SCRIPT_BYTES)?)
        .map_err(|error| format!("Script is not UTF-8: {error}"))?;
    let plan: Plan = serde_json::from_slice(&read_bounded(&plan_path, 1024 * 1024)?)
        .map_err(|error| error.to_string())?;
    if plan.steps.len() > 128 {
        return Err("Plan exceeds 128 UI steps".into());
    }
    if plan.frame >= decoded.project.composition().duration() {
        return Err("Plan frame is outside the active composition".into());
    }
    let baseline = project_file::encode(&decoded.project, decoded.view)?;
    fs::create_dir(&output).map_err(|error| format!("Output must be a new directory: {error}"))?;
    write_new(&output, "baseline.lep", &baseline)?;
    let mut editor = Editor::default();
    editor.replace_project(decoded.project.clone())?;
    editor.clear_history();
    let background = editor.project().composition().background_color();
    editor.execute(Command::SetCompositionBackground(background ^ 0x000101))?;
    let seeded_redo = editor.project().clone();
    editor.undo();
    if editor.can_undo() || !editor.can_redo() || editor.project() != &decoded.project {
        return Err("Failed to create a source-neutral Redo branch".into());
    }
    let source_len = source.len();
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let ui_cancel = cancel.clone();
    let ui = std::thread::spawn(move || {
        let result = drive(request_rx, response_tx, ui_cancel.clone(), plan.steps);
        if result.1.is_err() {
            ui_cancel.store(true, Ordering::Relaxed);
        }
        result
    });
    // Exactly the production supervision/budgets; no test-specific relaxed timeout.
    let outcome = automation_process::run_script(
        decoded.project.clone(),
        plan.selected_layer_ids,
        plan.frame,
        source,
        request_tx,
        response_rx,
        cancel,
    );
    let (transcript, ui_result) = ui.join().map_err(|_| "UI driver panicked")?;
    write_new(
        &output,
        "ui-transcript.json",
        &serde_json::to_vec_pretty(&transcript).map_err(|error| error.to_string())?,
    )?;
    ui_result?;
    let (changed, result_error, script_output) = match (outcome, plan.expected) {
        (Ok(outcome), Expected::Success { changed: expected }) => {
            let changed = editor.commit_automation_project(outcome.project)?;
            if changed != expected {
                return Err(format!("Expected changed={expected}, got {changed}"));
            }
            (changed, None, outcome.output)
        }
        (Err(error), Expected::Failure { contains }) if error.contains(&contains) => {
            (false, Some(error), Vec::new())
        }
        (Ok(_), Expected::Failure { .. }) => {
            return Err(
                "Expected a rejected transaction, but the worker completed successfully".into(),
            );
        }
        (Err(error), _) => return Err(format!("Unexpected worker failure: {error}")),
    };
    let result = project_file::encode(editor.project(), decoded.view)?;
    write_new(&output, "result.lep", &result)?;
    if changed {
        if !editor.can_undo() || editor.can_redo() {
            return Err("Successful transaction did not replace the old Redo branch".into());
        }
        editor.undo();
        let undone = project_file::encode(editor.project(), decoded.view)?;
        if undone != baseline || editor.can_undo() || !editor.can_redo() {
            return Err("One Undo did not restore the complete baseline".into());
        }
        write_new(&output, "undo.lep", &undone)?;
        editor.redo();
        let redone = project_file::encode(editor.project(), decoded.view)?;
        if redone != result || editor.can_redo() {
            return Err("One Redo did not restore the complete result".into());
        }
        write_new(&output, "redo.lep", &redone)?;
    } else {
        if result != baseline || editor.can_undo() || !editor.can_redo() {
            return Err("Rejected/no-op run changed source or history".into());
        }
        editor.redo();
        if editor.project() != &seeded_redo {
            return Err("Rejected/no-op run replaced the pre-existing Redo branch".into());
        }
        editor.undo();
    }
    let reopened = project_file::decode(&result)?;
    if project_file::encode(&reopened.project, reopened.view)? != result {
        return Err("Result Save/Reopen is not byte-identical".into());
    }
    let receipt = json!({"passed":true,"changed":changed,"error":result_error,"script_output":script_output,"script_bytes":source_len,"ui_steps":transcript.len(),"input_roundtrip_byte_equal":input==baseline,"one_undo_redo_checked":changed,"prior_redo_preserved":!changed,"reopen_byte_equal":true});
    write_new(
        &output,
        "receipt.json",
        &serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
    )?;
    println!("{}", serde_json::to_string(&receipt).unwrap());
    Ok(())
}
fn main() {
    if let Some(result) = automation_process::dispatch_worker() {
        if result.is_err() {
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: u64, kind: &str, text: &str, enabled: bool, children: Vec<UiNode>) -> UiNode {
        serde_json::from_value(json!({"id":id,"kind":kind,"text":text,"enabled":enabled,
            "visible":true,"active":false,"focus_request":0,"multiline":false,"orientation":"column",
            "children":children,"minimum_size":null,"preferred_size":null,"help_tip":""})).unwrap()
    }
    fn button() -> Selector {
        Selector {
            kind: "button".into(),
            text: Some("Apply".into()),
        }
    }
    #[test]
    fn selectors_reject_ambiguous_and_disabled_controls() {
        let root = node(
            1,
            "dialog",
            "",
            true,
            vec![
                node(2, "button", "Apply", true, vec![]),
                node(3, "button", "Apply", true, vec![]),
            ],
        );
        assert!(select(&root, &button()).unwrap_err().contains("matched 2"));
        let root = node(
            1,
            "dialog",
            "",
            true,
            vec![node(
                2,
                "group",
                "",
                false,
                vec![node(3, "button", "Apply", true, vec![])],
            )],
        );
        assert!(select(&root, &button()).unwrap_err().contains("matched 0"));
    }
    #[test]
    fn selectors_use_actual_stable_control_ids() {
        let root = node(
            100,
            "dialog",
            "",
            true,
            vec![
                node(17, "button", "Other", true, vec![]),
                node(993, "button", "Apply", true, vec![]),
            ],
        );
        assert_eq!(select(&root, &button()).unwrap(), 993);
    }
    #[test]
    fn dialog_actions_cannot_accept_a_nested_confirmation() {
        let step = Step {
            action: Action::Click { selector: button() },
            required_text: vec![],
            required_fragments: vec![],
        };
        assert!(
            response(
                &UiRequest::Confirm {
                    message: "Apply?".into()
                },
                &step
            )
            .is_err()
        );
    }
    #[test]
    fn assertions_are_checked_before_dismissal() {
        let step = Step {
            action: Action::DismissAlert {},
            required_text: vec![],
            required_fragments: vec!["complete".into()],
        };
        assert!(
            response(
                &UiRequest::Alert {
                    message: "Processing error".into()
                },
                &step
            )
            .is_err()
        );
    }
    #[test]
    fn unrecognized_plan_and_action_fields_reject() {
        assert!(
            serde_json::from_value::<Plan>(
                json!({"expected":{"kind":"success","changed":false},"steps":[],"ignored":true})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<Action>(json!({"kind":"cancel","accept":true})).is_err());
    }
}
