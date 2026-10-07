//! Isolated, resource-bounded JavaScript/ExtendScript subset. The VM lives on the
//! caller's worker thread; UI waits keep its stack and closures alive, never replay
//! source. No filesystem, network, module loader, subprocess or editor references
//! are installed in the VM. Only a detached, validated project draft is exposed.
use crate::automation_host::AutomationHost;
use libre_effects_core::{Frame, LayerId, Project};
use rquickjs::{Context, Function, Runtime};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, RecvTimeoutError, Sender},
};
use std::time::{Duration, Instant};

pub const MAX_SCRIPT_BYTES: usize = 1024 * 1024;
const MAX_BRIDGE_BYTES: usize = 1024 * 1024;
const MAX_UI_CONTROLS: usize = 256;
const MAX_UI_EVENTS: usize = 10_000;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const EXECUTION_SLICE: Duration = Duration::from_secs(2);
const EXECUTION_TOTAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiNode {
    pub id: u64,
    pub kind: String,
    pub text: String,
    pub enabled: bool,
    pub visible: bool,
    pub active: bool,
    /// A VM-wide serial for an explicit `active = true`, not persistent focus.
    #[serde(default)]
    pub focus_request: u64,
    pub multiline: bool,
    pub orientation: String,
    pub children: Vec<UiNode>,
    pub minimum_size: Option<[f64; 2]>,
    pub preferred_size: Option<[f64; 2]>,
    pub help_tip: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiRequest {
    Dialog {
        id: u64,
        title: String,
        root: UiNode,
        default_element: Option<u64>,
        cancel_element: Option<u64>,
        resizable: bool,
    },
    Alert {
        message: String,
    },
    Confirm {
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiResponse {
    Click {
        dialog_id: u64,
        control_id: u64,
    },
    Change {
        dialog_id: u64,
        control_id: u64,
        text: String,
    },
    Close {
        dialog_id: u64,
    },
    Resize {
        dialog_id: u64,
    },
    AlertDismissed,
    Confirm {
        value: bool,
    },
}

#[derive(Debug)]
pub struct ScriptOutcome {
    pub project: Project,
    pub selected_layer_ids: Vec<LayerId>,
    pub output: Vec<String>,
}

struct Budget {
    started: Instant,
    spent: Duration,
}
impl Budget {
    fn expired(&self) -> bool {
        self.started.elapsed() >= EXECUTION_SLICE
            || self.spent + self.started.elapsed() >= EXECUTION_TOTAL
    }
    fn pause(&mut self) {
        self.spent += self.started.elapsed();
    }
    fn resume(&mut self) {
        self.started = Instant::now();
    }
}

/// The desktop must call this on a dedicated worker thread and hold the immutable
/// base project/context until it can atomically validate and install the result.
/// Closing/canceling any script dialog rejects the complete candidate.
pub fn run_script(
    project: Project,
    selected_layer_ids: Vec<LayerId>,
    frame: Frame,
    source: &str,
    requests: Sender<UiRequest>,
    responses: Receiver<UiResponse>,
    cancel: Arc<AtomicBool>,
) -> Result<ScriptOutcome, String> {
    let source = preprocess(source)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Script canceled".into());
    }
    let host = Rc::new(RefCell::new(Some(AutomationHost::new(
        &project,
        &selected_layer_ids,
        frame,
    )?)));
    let failure: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let output: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let budget = Arc::new(Mutex::new(Budget {
        started: Instant::now(),
        spent: Duration::ZERO,
    }));
    let runtime = Runtime::new().map_err(|e| format!("Cannot initialize script runtime: {e}"))?;
    runtime.set_memory_limit(64 * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    // A rejected async function can have no queued jobs at all. Reject Promise
    // creation itself so a caught/unobserved rejection cannot commit a partial
    // synchronous-looking draft. No Promise jobs are executed by this host.
    let promise_failure = failure.clone();
    runtime.set_promise_hook(Some(Box::new(move |_, _, _, _| {
        let mut failure = promise_failure.borrow_mut();
        if failure.is_none() {
            *failure = Some("Asynchronous JavaScript is unsupported".into());
        }
    })));
    let interrupt_budget = budget.clone();
    let interrupt_cancel = cancel.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        interrupt_cancel.load(Ordering::Relaxed)
            || interrupt_budget.lock().map_or(true, |b| b.expired())
    })));
    let context =
        Context::full(&runtime).map_err(|e| format!("Cannot initialize script context: {e}"))?;
    let evaluation = context.with(|ctx| -> Result<(), String> {
        let globals = ctx.globals();
        let bridge_host = host.clone();
        let bridge_failure = failure.clone();
        let bridge_cancel = cancel.clone();
        let bridge = Function::new(ctx.clone(), move |op: String, json: String| -> String {
            let result = if bridge_cancel.load(Ordering::Relaxed) {
                Err("Script canceled".into())
            } else if let Some(message) = bridge_failure.borrow().clone() {
                Err(message)
            } else if json.len() > MAX_BRIDGE_BYTES || op.len() > 128 {
                Err("Script host request exceeds its size limit".into())
            } else {
                serde_json::from_str(&json)
                    .map_err(|e| format!("Invalid script host request: {e}"))
                    .and_then(|args| {
                        bridge_host
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .dispatch(&op, args)
                    })
            };
            envelope(result, &bridge_failure)
        })
        .map_err(|e| e.to_string())?;
        globals
            .set("__le_host", bridge)
            .map_err(|e| e.to_string())?;
        let ui_failure = failure.clone();
        let ui_cancel = cancel.clone();
        let ui_budget = budget.clone();
        let event_count = Rc::new(RefCell::new(0_usize));
        let ui = Function::new(ctx.clone(), move |json: String| -> String {
            let result = (|| {
                if let Some(message) = ui_failure.borrow().clone() {
                    return Err(message);
                }
                if ui_cancel.load(Ordering::Relaxed) {
                    return Err("Script canceled".into());
                }
                if json.len() > MAX_BRIDGE_BYTES {
                    return Err("Script dialog exceeds its size limit".into());
                }
                let mut count = event_count.borrow_mut();
                *count += 1;
                if *count > MAX_UI_EVENTS {
                    return Err("Script UI event budget exceeded".into());
                }
                let request: UiRequest =
                    serde_json::from_str(&json).map_err(|e| format!("Invalid script UI: {e}"))?;
                validate_request(&request)?;
                requests
                    .send(request.clone())
                    .map_err(|_| "Script UI was closed".to_string())?;
                ui_budget
                    .lock()
                    .map_err(|_| "Script execution budget unavailable".to_string())?
                    .pause();
                let response = loop {
                    if ui_cancel.load(Ordering::Relaxed) {
                        break Err("Script canceled".into());
                    }
                    match responses.recv_timeout(Duration::from_millis(50)) {
                        Ok(response) => break validate_response(&request, response),
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => {
                            break Err("Script UI was closed".into());
                        }
                    }
                };
                ui_budget
                    .lock()
                    .map_err(|_| "Script execution budget unavailable".to_string())?
                    .resume();
                response
                    .and_then(|response| serde_json::to_value(response).map_err(|e| e.to_string()))
            })();
            envelope(result, &ui_failure)
        })
        .map_err(|e| e.to_string())?;
        globals.set("__le_ui", ui).map_err(|e| e.to_string())?;
        let fatal_failure = failure.clone();
        globals
            .set(
                "__le_fail",
                Function::new(ctx.clone(), move |message: String| {
                    let mut failure = fatal_failure.borrow_mut();
                    if failure.is_none() {
                        *failure = Some(message.chars().take(2048).collect());
                    }
                })
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let print_output = output.clone();
        let print_failure = failure.clone();
        globals
            .set(
                "__le_print",
                Function::new(ctx.clone(), move |message: String| -> String {
                    let result = if print_output.borrow().len() >= 1024
                        || message.len()
                            + print_output
                                .borrow()
                                .iter()
                                .map(|line| line.len() + 1)
                                .sum::<usize>()
                            > MAX_OUTPUT_BYTES
                    {
                        Err("Script output exceeds 64 KiB".into())
                    } else {
                        print_output.borrow_mut().push(message);
                        Ok(serde_json::Value::Null)
                    };
                    envelope(result, &print_failure)
                })
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        // The finalizer is held by Rust, not installed as a user-overwritable
        // global. It verifies facade state only after source returns normally.
        let finalize = ctx
            .eval::<Function, _>(include_str!("automation.js"))
            .map_err(|e| format_js_error(&ctx, e))?;
        ctx.eval::<(), _>(source)
            .map_err(|e| format_js_error(&ctx, e))?;
        finalize
            .call::<_, ()>(())
            .map_err(|e| format_js_error(&ctx, e))?;
        Ok(())
    });
    if cancel.load(Ordering::Relaxed) {
        return Err("Script canceled".into());
    }
    if let Some(message) = failure.borrow().clone() {
        return Err(message);
    }
    if budget.lock().map_or(true, |b| b.expired()) {
        return Err("Script execution time limit exceeded".into());
    }
    evaluation?;
    // Check outside Context::with, which owns the runtime lock. Never run
    // delayed jobs after a synchronous ExtendScript transaction has completed.
    if runtime.is_job_pending() {
        return Err("Asynchronous JavaScript jobs are unsupported".into());
    }
    let project = host.borrow_mut().take().unwrap().finish()?;
    let selected_layer_ids = selected_layer_ids
        .into_iter()
        .filter(|id| project.composition().layer(*id).is_some())
        .collect();
    let output = output.borrow().clone();
    Ok(ScriptOutcome {
        project,
        selected_layer_ids,
        output,
    })
}

fn envelope(
    result: Result<serde_json::Value, String>,
    failure: &RefCell<Option<String>>,
) -> String {
    let value = match result {
        Ok(value) => serde_json::json!({"ok": true, "value": value}),
        Err(message) => {
            if failure.borrow().is_none() {
                *failure.borrow_mut() = Some(message.clone());
            }
            serde_json::json!({"ok": false, "error": message})
        }
    };
    let json = value.to_string();
    if json.len() > MAX_BRIDGE_BYTES {
        let message = "Script host response exceeds its size limit";
        if failure.borrow().is_none() {
            *failure.borrow_mut() = Some(message.into());
        }
        return serde_json::json!({"ok":false,"error":message}).to_string();
    }
    json
}

fn format_js_error(ctx: &rquickjs::Ctx<'_>, error: rquickjs::Error) -> String {
    if error.is_exception() {
        let value = ctx.catch();
        if let Some(exception) = value.as_exception() {
            let message = exception
                .message()
                .unwrap_or_else(|| "JavaScript exception".into());
            let stack = exception.stack().unwrap_or_default();
            return format!("{message}\n{stack}").chars().take(8192).collect();
        }
        return format!("JavaScript exception: {value:?}")
            .chars()
            .take(8192)
            .collect();
    }
    format!("JavaScript failed: {error}")
}

fn preprocess(source: &str) -> Result<String, String> {
    if source.len() > MAX_SCRIPT_BYTES {
        return Err("Script source exceeds 1 MiB".into());
    }
    if source.contains('\0') {
        return Err("Script source contains a NUL byte".into());
    }
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut result = String::with_capacity(source.len());
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            if matches!(
                trimmed,
                "#target aftereffects" | "#target \"aftereffects\"" | "#target 'aftereffects'"
            ) {
                result.push_str("// target aftereffects (bounded compatibility)\n");
            } else {
                return Err(format!(
                    "Unsupported ExtendScript directive on line {}: {trimmed}",
                    index + 1
                ));
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    Ok(result)
}

pub fn validate_request(request: &UiRequest) -> Result<(), String> {
    if let UiRequest::Dialog {
        id,
        root,
        default_element,
        cancel_element,
        ..
    } = request
    {
        let mut ids = std::collections::BTreeSet::new();
        fn visit(
            node: &UiNode,
            depth: usize,
            ids: &mut std::collections::BTreeSet<u64>,
        ) -> Result<(), String> {
            if depth > 16 || ids.len() >= MAX_UI_CONTROLS || !ids.insert(node.id) {
                return Err("ScriptUI control count, nesting or identity limit exceeded".into());
            }
            if !matches!(
                node.kind.as_str(),
                "dialog" | "group" | "panel" | "statictext" | "edittext" | "button"
            ) {
                return Err(format!("Unsupported ScriptUI control: {}", node.kind));
            }
            if node.text.len() > 256 * 1024
                || (node.kind == "edittext" && node.text.len() > 16 * 1024)
                || node.help_tip.len() > 4096
            {
                return Err("ScriptUI text exceeds its size limit".into());
            }
            for size in [node.minimum_size, node.preferred_size]
                .into_iter()
                .flatten()
            {
                if size
                    .iter()
                    .any(|v| !v.is_finite() || *v < 0.0 || *v > 4096.0)
                {
                    return Err("ScriptUI size must be between 0 and 4096".into());
                }
            }
            for child in &node.children {
                visit(child, depth + 1, ids)?;
            }
            Ok(())
        }
        visit(root, 0, &mut ids)?;
        if *id != root.id
            || default_element.is_some_and(|id| !ids.contains(&id))
            || cancel_element.is_some_and(|id| !ids.contains(&id))
        {
            return Err("Invalid ScriptUI dialog identity".into());
        }
    }
    Ok(())
}

pub fn validate_response(request: &UiRequest, response: UiResponse) -> Result<UiResponse, String> {
    let valid = match (request, &response) {
        (
            UiRequest::Dialog { id, root, .. },
            UiResponse::Click {
                dialog_id,
                control_id,
            },
        ) => {
            *id == *dialog_id
                && find_control(root, *control_id)
                    .is_some_and(|n| n.kind == "button" && n.enabled && n.visible)
        }
        (
            UiRequest::Dialog { id, root, .. },
            UiResponse::Change {
                dialog_id,
                control_id,
                text,
            },
        ) => {
            *id == *dialog_id
                && text.len() <= 16 * 1024
                && find_control(root, *control_id)
                    .is_some_and(|n| n.kind == "edittext" && n.enabled && n.visible)
        }
        (
            UiRequest::Dialog { id, .. },
            UiResponse::Close { dialog_id } | UiResponse::Resize { dialog_id },
        ) => *id == *dialog_id,
        (UiRequest::Alert { .. }, UiResponse::AlertDismissed)
        | (UiRequest::Confirm { .. }, UiResponse::Confirm { .. }) => true,
        _ => false,
    };
    if valid {
        Ok(response)
    } else {
        Err("Stale or invalid ScriptUI response".into())
    }
}
fn find_control(node: &UiNode, id: u64) -> Option<&UiNode> {
    if !node.visible || !node.enabled {
        return None;
    }
    if node.id == id {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_control(child, id))
}

#[cfg(test)]
mod tests;
