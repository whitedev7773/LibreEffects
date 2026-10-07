//! A script works on an isolated project. UI events cross a channel; source edits
//! become visible only after successful completion and a current-editor receipt.
use super::*;
use libre_effects_editor_model::automation::{ScriptOutcome, UiRequest, UiResponse};
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub(crate) struct Session {
    pub operation: u64,
    pub name: String,
    ui: libre_effects_editor_model::automation_ui::UiSession,
    responses: Option<mpsc::Sender<UiResponse>>,
    cancel: Arc<AtomicBool>,
}
impl std::ops::Deref for Session {
    type Target = libre_effects_editor_model::automation_ui::UiSession;
    fn deref(&self) -> &Self::Target {
        &self.ui
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Session {
    fn send(&mut self, response: Option<UiResponse>) {
        if let Some(response) = response {
            if self
                .responses
                .as_ref()
                .is_none_or(|sender| sender.send(response).is_err())
            {
                self.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
    fn receive(&mut self, request: UiRequest) {
        let response = self.ui.receive(request);
        self.send(response);
    }
    fn respond(&mut self, response: UiResponse) {
        match self.ui.respond(response) {
            Ok(response) => self.send(response),
            Err(_) => self.cancel.store(true, Ordering::Relaxed),
        }
    }
}

#[derive(Clone)]
struct Receipt {
    operation: u64,
    document: u64,
    core: u64,
    input: u64,
    transport: u64,
    selected: Option<LayerId>,
    layers: BTreeSet<LayerId>,
    frame: Frame,
    source: Project,
}
impl Receipt {
    fn current(&self, state: &EditorState) -> bool {
        state.automation.as_ref().is_some_and(|session| {
            session.operation == self.operation && !session.cancel.load(Ordering::Relaxed)
        }) && self.document == state.document_revision
            && self.core == state.editor.context_generation()
            && self.input == state.input_context_generation()
            && self.transport == state.transport_generation()
            && self.selected == state.editor.selected()
            && self.layers == state.selected_layers
            && self.frame == state.frame
            && self.source == *state.editor.project()
            && !state.playing
            && !state.preview_caching
    }
}

impl EditorState {
    pub(crate) fn automation_available(&self) -> bool {
        !crate::automation_process::worker_active()
            && self.automation.is_none()
            && self.svg_import_available()
            && !self.playing
            && !self.preview_caching
    }
    pub(crate) fn cancel_automation(&mut self) {
        if self.automation.take().is_some() {
            self.status = "Script canceled; project unchanged".into();
        }
    }
    pub(crate) fn automation_response(
        &mut self,
        operation: u64,
        revision: u64,
        response: UiResponse,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self
            .automation
            .as_mut()
            .filter(|s| s.operation == operation && s.revision == revision)
        {
            session.respond(response);
            cx.notify();
        }
    }
    pub(super) fn run_script_file(&mut self, cx: &mut Context<Self>) {
        if !self.automation_available() {
            return;
        }
        self.stop();
        let operation = self.begin_file_operation();
        let cancel = Arc::new(AtomicBool::new(false));
        self.automation = Some(Session {
            operation,
            name: "Choose a JSX script".into(),
            ui: Default::default(),
            responses: None,
            cancel: cancel.clone(),
        });
        let receipt = Receipt {
            operation,
            document: self.document_revision,
            core: self.editor.context_generation(),
            input: self.input_context_generation(),
            transport: self.transport_generation(),
            selected: self.editor.selected(),
            layers: self.selected_layers.clone(),
            frame: self.frame,
            source: self.editor.project().clone(),
        };
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                "Run JavaScript / JSX · bounded native scripting API · maximum 1 MiB".into(),
            ),
        });
        self.status =
            "Choose a .jsx or .js file · Changes apply together after the script succeeds".into();
        cx.spawn(async move |entity, cx| {
            let result = match prompt.await {
                Ok(Ok(Some(mut paths))) if paths.len() == 1 => Ok(paths.pop()),
                Ok(Ok(None)) => Ok(None),
                Ok(Ok(Some(paths))) if paths.is_empty() => Ok(None),
                Ok(Ok(_)) => Err("Choose exactly one script file".into()),
                Ok(Err(error)) => Err(format!("Script chooser failed: {error}")),
                Err(error) => Err(format!("Script chooser failed: {error}")),
            };
            let path = match result {
                Ok(Some(path)) => path,
                other => {
                    let _ = entity.update(cx, |s, cx| {
                        s.finish_automation(&receipt, other.map(|_| None));
                        cx.notify();
                    });
                    return;
                }
            };
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .take(128)
                .collect::<String>();
            let (request_tx, request_rx) = mpsc::channel();
            let (response_tx, response_rx) = mpsc::channel();
            let (result_tx, result_rx) = mpsc::channel();
            let current = entity.update(cx, |s, cx| {
                if !receipt.current(s) {
                    s.finish_automation(&receipt, Ok(None));
                    cx.notify();
                    return false;
                }
                let session = s.automation.as_mut().unwrap();
                session.name = name;
                session.responses = Some(response_tx);
                s.status = "Running script on an isolated project…".into();
                cx.notify();
                true
            });
            if !matches!(current, Ok(true)) {
                return;
            }
            let project = receipt.source.clone();
            let layers = receipt
                .layers
                .iter()
                .copied()
                .chain(receipt.selected)
                .collect();
            let frame = receipt.frame;
            let worker = std::thread::Builder::new()
                .name("jsx-supervisor".into())
                .stack_size(4 * 1024 * 1024)
                .spawn(move || {
                    let result = read_script(&path).and_then(|source| {
                        crate::automation_process::run_script(
                            project,
                            layers,
                            frame,
                            source,
                            request_tx,
                            response_rx,
                            cancel,
                        )
                    });
                    let _ = result_tx.send(result);
                });
            if let Err(error) = worker {
                let _ = entity.update(cx, |s, cx| {
                    s.finish_automation(&receipt, Err(format!("Cannot start script: {error}")));
                    cx.notify();
                });
                return;
            }
            loop {
                let current = entity.update(cx, |s, cx| {
                    if !receipt.current(s) {
                        s.finish_automation(&receipt, Ok(None));
                        cx.notify();
                        return false;
                    }
                    while let Ok(request) = request_rx.try_recv() {
                        if let Some(session) = s.automation.as_mut() {
                            session.receive(request);
                        }
                        cx.notify();
                    }
                    true
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
                match result_rx.try_recv() {
                    Ok(result) => {
                        let _ = entity.update(cx, |s, cx| {
                            s.finish_automation(&receipt, result.map(Some));
                            cx.notify();
                        });
                        return;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        let _ = entity.update(cx, |s, cx| {
                            s.finish_automation(
                                &receipt,
                                Err("Script worker stopped unexpectedly".into()),
                            );
                            cx.notify();
                        });
                        return;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
        })
        .detach();
    }
    fn finish_automation(
        &mut self,
        receipt: &Receipt,
        result: Result<Option<ScriptOutcome>, String>,
    ) {
        if self
            .automation
            .as_ref()
            .is_none_or(|s| s.operation != receipt.operation)
        {
            return;
        }
        let current = receipt.current(self);
        self.automation = None;
        if !current {
            self.status = "Script canceled: editor context changed; project unchanged".into();
            return;
        }
        let spacing = crate::authored_spacing_notice::authored_layers(self.editor.project());
        self.status = match result {
            Ok(Some(outcome)) => match self.editor.commit_automation_project(outcome.project) {
                Ok(changed) => {
                    if changed {
                        self.begin_input_action();
                        self.retire_colors_clipboard();
                        self.colors_key_owned.set(false);
                        self.selected_layers = outcome
                            .selected_layer_ids
                            .iter()
                            .copied()
                            .filter(|id| self.editor.project().composition().layer(*id).is_some())
                            .collect();
                        self.selected_keys.clear();
                        self.contents_selection = None;
                        self.gradient_controls = None;
                        self.graph_key = None;
                        self.composition_started = true;
                        self.preview_revision = self.preview_revision.wrapping_add(1);
                        self.document_revision = self.document_revision.wrapping_add(1);
                        self.normalize();
                    }
                    let output = outcome
                        .output
                        .last()
                        .map(|text| format!(" · {}", text.chars().take(180).collect::<String>()))
                        .unwrap_or_default();
                    format!(
                        "Script complete · {}{}{output}",
                        if changed {
                            "One Undo"
                        } else {
                            "No project changes"
                        },
                        crate::authored_spacing_notice::suffix(
                            crate::authored_spacing_notice::reset_count(
                                &spacing,
                                self.editor.project(),
                            )
                        )
                    )
                }
                Err(error) => format!("Script rejected; project unchanged: {error}"),
            },
            Ok(None) => "Script canceled; project unchanged".into(),
            Err(error) => format!("Script failed; project unchanged: {error}"),
        };
    }
}

fn read_script(path: &Path) -> Result<String, String> {
    if !path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jsx") || ext.eq_ignore_ascii_case("js"))
    {
        return Err("Choose a .jsx or .js text script".into());
    }
    if !std::fs::metadata(path)
        .map_err(|e| format!("Cannot inspect script: {e}"))?
        .is_file()
    {
        return Err("Script must be a regular file".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("Cannot read script: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Script must be a regular file".into());
    }
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_SOURCE_BYTES as u64 {
        return Err("Script exceeds 1 MiB".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read script: {e}"))?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err("Script exceeds 1 MiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "Script must be UTF-8 text".into())
}
