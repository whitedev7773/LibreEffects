//! Killable, GPUI-free workers for untrusted JavaScript.
//!
//! A separate copy of the installed executable owns QuickJS. A native builtin
//! need not visit QuickJS's interrupt hook; the supervisor can still kill/reap
//! the process. No filesystem, network, shell or module APIs are added to JS.
//! This is an execution/resource boundary, not an OS privilege sandbox.
use libre_effects_ae_expressions as ae;
use libre_effects_core::{Content, Frame, LayerId, MediaSharing, Project};
use libre_effects_editor_model::automation::{self, ScriptOutcome, UiRequest, UiResponse};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(crate) const WORKER_FLAG: &str = "--automation-worker";
const EXPRESSION_WORKER_FLAG: &str = "--expression-worker";
const EXPRESSION_SESSION_FLAG: &str = "--expression-session-worker";
const MAX_EXPRESSION_INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_EXPRESSION_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_PROJECT_BYTES: usize = 16 * 1024 * 1024;
const MAX_FRAME_BYTES: usize = 18 * 1024 * 1024;
const MAX_UI_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_IPC_BYTES: usize = 256 * 1024 * 1024;
const MAX_UI_EVENTS: usize = 10_000;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const EXECUTION_SLICE: Duration = Duration::from_secs(2);
const EXECUTION_TOTAL: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(10);
const FRAME_MAGIC: [u8; 4] = *b"LEW1";

static WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) fn worker_active() -> bool {
    WORKER_ACTIVE.load(Ordering::Acquire)
}
struct WorkerGuard;
impl WorkerGuard {
    fn acquire() -> Result<Self, String> {
        WORKER_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "A previous JavaScript worker is still stopping".into())
    }
    fn wait(cancel: &AtomicBool) -> Result<Self, String> {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("JavaScript worker canceled while waiting".into());
            }
            if let Ok(guard) = Self::acquire() {
                return Ok(guard);
            }
            std::thread::sleep(POLL);
        }
    }
}
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        WORKER_ACTIVE.store(false, Ordering::Release);
    }
}

// Externally tagged envelopes deserialize Project directly from the JSON stream.
// Internal/untagged envelopes buffer serde::Content, which loses JSON's integer
// map-key decoding for animation frames, asset IDs and composition IDs.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ParentMessage {
    Start {
        project: Project,
        sharing: MediaSharing,
        selected_layer_ids: Vec<LayerId>,
        frame: Frame,
        source: String,
    },
    Response {
        response: UiResponse,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum WorkerMessage {
    Ui {
        request: UiRequest,
    },
    Complete {
        project: Project,
        sharing: MediaSharing,
        selected_layer_ids: Vec<LayerId>,
        output: Vec<String>,
    },
    Failed {
        error: String,
    },
}

/// A bounded serializer stops *during* encoding, before any oversized Vec can
/// be allocated. Used for input, output and independent project budgets.
struct LimitedWriter {
    bytes: Vec<u8>,
    written: usize,
    limit: usize,
    retain: bool,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.written) {
            return Err(io::Error::other("Worker message exceeds its byte budget"));
        }
        if self.retain {
            self.bytes
                .try_reserve(bytes.len())
                .map_err(|_| io::Error::other("Cannot allocate bounded worker message"))?;
            self.bytes.extend_from_slice(bytes);
        }
        self.written += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, String> {
    let mut writer = LimitedWriter {
        bytes: Vec::new(),
        written: 0,
        limit,
        retain: true,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|e| format!("Worker encoding failed: {e}"))?;
    Ok(writer.bytes)
}
fn check_size<T: Serialize>(value: &T, limit: usize) -> Result<(), String> {
    let mut writer = LimitedWriter {
        bytes: Vec::new(),
        written: 0,
        limit,
        retain: false,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|e| format!("Worker input exceeds its size limit: {e}"))
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    // serde_json's default nesting limit remains enabled.
    serde_json::from_slice(bytes).map_err(|e| format!("Invalid worker message: {e}"))
}
fn write_frame(writer: &mut impl Write, bytes: &[u8], limit: usize) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > limit || bytes.len() > u32::MAX as usize {
        return Err("Worker frame exceeds its size limit".into());
    }
    writer
        .write_all(&FRAME_MAGIC)
        .and_then(|_| writer.write_all(&(bytes.len() as u32).to_be_bytes()))
        .and_then(|_| writer.write_all(bytes))
        .and_then(|_| writer.flush())
        .map_err(|e| format!("Worker pipe write failed: {e}"))
}
fn read_frame(reader: &mut impl Read, limit: usize) -> Result<Option<Vec<u8>>, String> {
    let mut header = [0_u8; 8];
    // Distinguish a clean EOF between frames from a truncated frame/header.
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("Worker pipe read failed: {error}")),
        }
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|e| format!("Truncated worker frame: {e}"))?;
    let len = u32::from_be_bytes(header[4..].try_into().unwrap()) as usize;
    if header[..4] != FRAME_MAGIC || len == 0 || len > limit {
        return Err("Invalid or oversized worker frame".into());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| "Cannot allocate bounded worker frame".to_string())?;
    bytes.resize(len, 0);
    reader
        .read_exact(&mut bytes)
        .map_err(|e| format!("Truncated worker message: {e}"))?;
    Ok(Some(bytes))
}

pub(crate) enum PipeEvent {
    Frame(Vec<u8>),
    Eof,
    Failed(String),
}
/// Generic transport shared by JSX and expression adapters. Only this owner
/// touches Child; blocking pipe operations are bounded, separate I/O threads.
pub(crate) struct WorkerProcess {
    child: Child,
    outgoing: Option<SyncSender<Vec<u8>>>,
    events: Option<Receiver<PipeEvent>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    transferred: usize,
}
impl WorkerProcess {
    pub(crate) fn spawn(executable: &Path, flag: &str, max_frame: usize) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .arg(flag)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = command
            .spawn()
            .map_err(|e| format!("Cannot start JavaScript worker: {e}"))?;
        let mut process = Self {
            child,
            outgoing: None,
            events: None,
            reader: None,
            writer: None,
            transferred: 0,
        };
        let mut stdout = process
            .child
            .stdout
            .take()
            .ok_or("Missing worker output pipe")?;
        let mut stdin = process
            .child
            .stdin
            .take()
            .ok_or("Missing worker input pipe")?;
        let (event_tx, event_rx) = mpsc::sync_channel(1);
        let (outgoing_tx, outgoing_rx) = mpsc::sync_channel::<Vec<u8>>(1);
        process.events = Some(event_rx);
        process.outgoing = Some(outgoing_tx);
        let read_events = event_tx.clone();
        process.reader = Some(
            std::thread::Builder::new()
                .name("javascript-pipe-read".into())
                .spawn(move || {
                    loop {
                        let event = match read_frame(&mut stdout, max_frame) {
                            Ok(Some(bytes)) => PipeEvent::Frame(bytes),
                            Ok(None) => PipeEvent::Eof,
                            Err(error) => PipeEvent::Failed(error),
                        };
                        let terminal = !matches!(event, PipeEvent::Frame(_));
                        if read_events.send(event).is_err() || terminal {
                            break;
                        }
                    }
                })
                .map_err(|e| format!("Cannot start worker output reader: {e}"))?,
        );
        process.writer = Some(
            std::thread::Builder::new()
                .name("javascript-pipe-write".into())
                .spawn(move || {
                    while let Ok(bytes) = outgoing_rx.recv() {
                        if let Err(error) = write_frame(&mut stdin, &bytes, max_frame) {
                            let _ = event_tx.send(PipeEvent::Failed(error));
                            break;
                        }
                    }
                })
                .map_err(|e| format!("Cannot start worker input writer: {e}"))?,
        );
        Ok(process)
    }
    fn account(&mut self, bytes: usize) -> Result<(), String> {
        self.transferred = self
            .transferred
            .checked_add(bytes)
            .ok_or("Worker IPC budget exceeded")?;
        if self.transferred > MAX_TOTAL_IPC_BYTES {
            return Err("Worker IPC budget exceeded".into());
        }
        Ok(())
    }
    pub(crate) fn send(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        self.account(bytes.len() + 8)?;
        self.outgoing
            .as_ref()
            .ok_or("Worker input closed")?
            .try_send(bytes)
            .map_err(|_| "Worker input pipe stalled or closed".into())
    }
    pub(crate) fn receive(&mut self) -> Result<Option<PipeEvent>, String> {
        match self
            .events
            .as_ref()
            .ok_or("Worker output closed")?
            .recv_timeout(POLL)
        {
            Ok(event) => {
                if let PipeEvent::Frame(bytes) = &event {
                    self.account(bytes.len() + 8)?;
                }
                Ok(Some(event))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("Worker output disconnected".into()),
        }
    }
    pub(crate) fn status(&mut self) -> Result<Option<ExitStatus>, String> {
        self.child
            .try_wait()
            .map_err(|e| format!("Cannot inspect worker: {e}"))
    }
}
impl Drop for WorkerProcess {
    fn drop(&mut self) {
        // Dropping receivers unblocks any full bounded queue, while killing the
        // child unblocks a thread stuck in an OS pipe read/write. Reap before
        // releasing the single-worker guard; never leave runaway native code.
        self.events.take();
        self.outgoing.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(thread) = self.reader.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.writer.take() {
            let _ = thread.join();
        }
    }
}

struct Deadline {
    resumed: Instant,
    spent: Duration,
    paused: bool,
    slice: Duration,
    total: Duration,
}
impl Deadline {
    fn new(slice: Duration, total: Duration) -> Self {
        Self {
            resumed: Instant::now(),
            spent: Duration::ZERO,
            paused: false,
            slice,
            total,
        }
    }
    fn check(&self, cancel: &AtomicBool) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("JavaScript worker canceled".into());
        }
        if !self.paused
            && (self.resumed.elapsed() >= self.slice
                || self.spent + self.resumed.elapsed() >= self.total)
        {
            return Err("JavaScript worker execution time limit exceeded".into());
        }
        Ok(())
    }
    fn pause(&mut self) {
        self.spent += self.resumed.elapsed();
        self.paused = true;
    }
    fn resume(&mut self) {
        self.resumed = Instant::now();
        self.paused = false;
    }
}

/// Source-derived file capabilities. A returned project may reuse/duplicate
/// existing media, but it cannot introduce a path the user never supplied.
struct SourceScope {
    media: BTreeSet<(u8, String)>,
}
impl SourceScope {
    fn collect(project: &Project) -> Self {
        let mut media = BTreeSet::new();
        for (_, composition) in project.compositions() {
            for layer in composition.layers() {
                Self::add(layer.content(), &mut media);
            }
        }
        for asset in project.asset_library().assets().values() {
            Self::add(asset.content(), &mut media);
        }
        Self { media }
    }
    fn add(content: &Content, media: &mut BTreeSet<(u8, String)>) {
        match content {
            Content::Image { png } => {
                media.insert((0, png.to_string()));
            }
            Content::ImageSequence { frames, .. } => {
                media.extend(frames.iter().map(|path| (1, path.clone())));
            }
            Content::Video { path, .. } => {
                media.insert((2, path.clone()));
            }
            Content::Audio { path, .. } => {
                media.insert((3, path.clone()));
            }
            _ => {}
        }
    }
    fn validate(&self, candidate: &Project) -> Result<(), String> {
        if !Self::collect(candidate).media.is_subset(&self.media) {
            return Err("Worker attempted to introduce an unapproved media source".into());
        }
        Ok(())
    }
}

pub(crate) fn run_script(
    project: Project,
    selected_layer_ids: Vec<LayerId>,
    frame: Frame,
    source: String,
    requests: Sender<UiRequest>,
    responses: Receiver<UiResponse>,
    cancel: Arc<AtomicBool>,
) -> Result<ScriptOutcome, String> {
    let executable =
        std::env::current_exe().map_err(|e| format!("Cannot locate JavaScript worker: {e}"))?;
    run_script_with(
        &executable,
        project,
        selected_layer_ids,
        frame,
        source,
        requests,
        responses,
        cancel,
        EXECUTION_SLICE,
        EXECUTION_TOTAL,
    )
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_script_with(
    executable: &Path,
    project: Project,
    selected_layer_ids: Vec<LayerId>,
    frame: Frame,
    source: String,
    requests: Sender<UiRequest>,
    responses: Receiver<UiResponse>,
    cancel: Arc<AtomicBool>,
    slice: Duration,
    total: Duration,
) -> Result<ScriptOutcome, String> {
    // A preview worker can acquire the permit while the file chooser is open.
    // Wait cancellably rather than failing a valid user-selected script.
    let _guard = WorkerGuard::wait(&cancel)?;
    if source.len() > automation::MAX_SCRIPT_BYTES {
        return Err("Script exceeds 1 MiB".into());
    }
    if selected_layer_ids.len() > 10_000 {
        return Err("Too many selected script layers".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Script canceled".into());
    }
    check_size(&project, MAX_PROJECT_BYTES)?;
    let scope = SourceScope::collect(&project);
    let original_assets = project.asset_library().clone();
    let original_project = project.clone();
    let start = encode(
        &ParentMessage::Start {
            sharing: project.media_sharing(),
            project,
            selected_layer_ids,
            frame,
            source,
        },
        MAX_FRAME_BYTES,
    )?;
    let mut deadline = Deadline::new(slice, total);
    let mut worker = WorkerProcess::spawn(executable, WORKER_FLAG, MAX_FRAME_BYTES)?;
    worker.send(start)?;
    let mut pending = None;
    let mut complete = None;
    let mut eof = false;
    let mut ui_events = 0;
    loop {
        deadline.check(&cancel)?;
        if let Some(request) = pending.as_ref() {
            match responses.try_recv() {
                Ok(response) => {
                    let response = automation::validate_response(request, response)?;
                    // Start the clock before encoding/writing the response: a
                    // blocked input pipe cannot masquerade as a user's UI wait.
                    deadline.resume();
                    pending = None;
                    worker.send(encode(&ParentMessage::Response { response }, MAX_UI_BYTES)?)?;
                }
                Err(mpsc::TryRecvError::Disconnected) => return Err("Script UI was closed".into()),
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !eof {
            match worker.receive()? {
                Some(PipeEvent::Frame(bytes)) => {
                    deadline.check(&cancel)?;
                    if complete.is_some() || pending.is_some() {
                        return Err("Unexpected worker message ordering".into());
                    }
                    match decode::<WorkerMessage>(&bytes)? {
                        WorkerMessage::Ui { request } => {
                            if bytes.len() > MAX_UI_BYTES {
                                return Err("Script UI exceeds its IPC limit".into());
                            }
                            automation::validate_request(&request)?;
                            ui_events += 1;
                            if ui_events > MAX_UI_EVENTS {
                                return Err("Script UI event budget exceeded".into());
                            }
                            // This is an actual complete UI request from the
                            // blocked VM bridge, not a worker heartbeat.
                            requests
                                .send(request.clone())
                                .map_err(|_| "Script UI was closed".to_string())?;
                            pending = Some(request);
                            deadline.check(&cancel)?;
                            deadline.pause();
                        }
                        WorkerMessage::Complete {
                            mut project,
                            sharing,
                            selected_layer_ids,
                            output,
                        } => {
                            check_size(&project, MAX_PROJECT_BYTES)?;
                            project.restore_media_sharing(&sharing, Some(&original_project))?;
                            project.validate_automation_project()?;
                            scope.validate(&project)?;
                            if project.asset_library() != &original_assets {
                                return Err("Worker changed the source asset library".into());
                            }
                            if selected_layer_ids.len() > 10_000
                                || output.len() > 1024
                                || output.iter().map(|line| line.len() + 1).sum::<usize>()
                                    > MAX_OUTPUT_BYTES
                            {
                                return Err("Worker result exceeds its size limit".into());
                            }
                            complete = Some(ScriptOutcome {
                                project,
                                selected_layer_ids,
                                output,
                            });
                        }
                        WorkerMessage::Failed { error } => {
                            return Err(error.chars().take(4096).collect());
                        }
                    }
                }
                Some(PipeEvent::Eof) => {
                    eof = true;
                    if complete.is_none() {
                        return Err("JavaScript worker exited without a result".into());
                    }
                }
                Some(PipeEvent::Failed(error)) => return Err(error),
                None => {}
            }
        } else {
            std::thread::sleep(POLL);
        }
        if let Some(status) = worker.status()? {
            if !status.success() {
                return Err("JavaScript worker crashed; candidate discarded".into());
            }
            if eof {
                return complete.ok_or_else(|| "JavaScript worker returned no result".into());
            }
        }
    }
}

/// Call before platform work, single-instance activation, or GPUI initialization.
/// The fixed private flag is the only child argument; source and project arrive
/// over stdin, never as a filename, shell command, or ambient JS capability.
pub(crate) fn dispatch_worker() -> Option<Result<(), String>> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next()?;
    if mode != WORKER_FLAG && mode != EXPRESSION_WORKER_FLAG && mode != EXPRESSION_SESSION_FLAG {
        return None;
    }
    if args.next().is_some() {
        return Some(Err("Worker mode accepts no file arguments".into()));
    }
    Some(if mode == WORKER_FLAG {
        worker_main()
    } else if mode == EXPRESSION_SESSION_FLAG {
        expression_session_main()
    } else {
        expression_worker_main()
    })
}

fn worker_limits() -> Result<(), String> {
    #[cfg(unix)]
    {
        let limit = |resource, value| -> Result<(), String> {
            let limit = libc::rlimit {
                rlim_cur: value,
                rlim_max: value,
            };
            // Only this freshly launched headless worker changes its limits.
            if unsafe { libc::setrlimit(resource, &limit) } != 0 {
                return Err(format!(
                    "Cannot bound JavaScript worker: {}",
                    io::Error::last_os_error()
                ));
            }
            Ok(())
        };
        limit(libc::RLIMIT_CORE, 0)?;
        limit(libc::RLIMIT_CPU, 35)?;
        // RLIMIT_AS is meaningful on Linux. macOS and Windows retain VM heap,
        // stack and IPC bounds, but have no verified hard process-memory cap.
        #[cfg(target_os = "linux")]
        limit(libc::RLIMIT_AS, 1024 * 1024 * 1024)?;
    }
    Ok(())
}
fn worker_main() -> Result<(), String> {
    worker_limits()?;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let first =
        read_frame(&mut input, MAX_FRAME_BYTES)?.ok_or("Worker requires one start message")?;
    let ParentMessage::Start {
        mut project,
        sharing,
        selected_layer_ids,
        frame,
        source,
    } = decode(&first)?
    else {
        return Err("Worker requires a start message".into());
    };
    if source.len() > automation::MAX_SCRIPT_BYTES || selected_layer_ids.len() > 10_000 {
        return Err("Worker source or selection budget exceeded".into());
    }
    check_size(&project, MAX_PROJECT_BYTES)?;
    project.restore_media_sharing(&sharing, None)?;
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("isolated-jsx-vm".into())
        .stack_size(4 * 1024 * 1024)
        .spawn(move || {
            let result = automation::run_script(
                project,
                selected_layer_ids,
                frame,
                &source,
                request_tx,
                response_rx,
                Arc::new(AtomicBool::new(false)),
            );
            let _ = result_tx.send(result);
        })
        .map_err(|e| format!("Cannot start isolated VM: {e}"))?;
    loop {
        if let Ok(request) = request_rx.try_recv() {
            write_frame(
                &mut output,
                &encode(&WorkerMessage::Ui { request }, MAX_UI_BYTES)?,
                MAX_UI_BYTES,
            )?;
            let response = read_frame(&mut input, MAX_UI_BYTES)?.ok_or("Script UI disconnected")?;
            let ParentMessage::Response { response } = decode(&response)? else {
                return Err("Expected ScriptUI response".into());
            };
            response_tx
                .send(response)
                .map_err(|_| "Script VM disconnected".to_string())?;
        }
        match result_rx.recv_timeout(POLL) {
            Ok(result) => {
                let message = match result {
                    Ok(outcome) => {
                        check_size(&outcome.project, MAX_PROJECT_BYTES)?;
                        WorkerMessage::Complete {
                            sharing: outcome.project.media_sharing(),
                            project: outcome.project,
                            selected_layer_ids: outcome.selected_layer_ids,
                            output: outcome.output,
                        }
                    }
                    Err(error) => WorkerMessage::Failed {
                        error: error.chars().take(4096).collect(),
                    },
                };
                return write_frame(
                    &mut output,
                    &encode(&message, MAX_FRAME_BYTES)?,
                    MAX_FRAME_BYTES,
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Script VM stopped unexpectedly".into());
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpressionInput {
    snapshot: ae::CompositionSnapshot,
    roots: Vec<ae::PropertyAddress>,
}
#[derive(Serialize)]
struct ExpressionInputRef<'a> {
    snapshot: &'a ae::CompositionSnapshot,
    roots: &'a [ae::PropertyAddress],
}

/// A renderer owns this child; every request creates a fresh VM and context.
/// Recycle after 16 batches or 128 MiB IPC, within the existing process limits.
#[derive(Default)]
pub(crate) struct ExpressionSession {
    worker: Option<WorkerProcess>,
    batches: u32,
}
impl ExpressionSession {
    pub(crate) fn clear(&mut self) {
        self.worker.take();
        self.batches = 0;
    }
    pub(crate) fn evaluate(
        &mut self,
        snapshot: &ae::CompositionSnapshot,
        roots: &[ae::PropertyAddress],
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> Result<ae::EvaluatedProperties, ae::EvaluationError> {
        let permit = WorkerGuard::wait(cancel);
        let result =
            (|| -> Result<Result<ae::EvaluatedProperties, ae::EvaluationError>, String> {
                permit.as_ref().map_err(Clone::clone)?;
                if roots.len() > 16_384 {
                    return Err("Expression root budget exceeded".into());
                }
                let input = encode(
                    &ExpressionInputRef { snapshot, roots },
                    MAX_EXPRESSION_INPUT_BYTES,
                )?;
                if self.batches >= 16
                    || self
                        .worker
                        .as_ref()
                        .is_some_and(|w| w.transferred >= MAX_TOTAL_IPC_BYTES / 2)
                {
                    self.clear();
                }
                let deadline = Deadline::new(timeout, timeout);
                if self.worker.is_none() {
                    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                    self.worker = Some(WorkerProcess::spawn(
                        &executable,
                        EXPRESSION_SESSION_FLAG,
                        MAX_EXPRESSION_FRAME_BYTES,
                    )?);
                }
                let worker = self.worker.as_mut().unwrap();
                worker.send(input)?;
                loop {
                    deadline.check(cancel)?;
                    match worker.receive()? {
                        Some(PipeEvent::Frame(bytes)) => {
                            deadline.check(cancel)?;
                            let mut result: Result<ae::EvaluatedProperties, ae::EvaluationError> =
                                decode(&bytes)?;
                            if let Err(error) = &mut result {
                                error.message = error.message.chars().take(4096).collect();
                            }
                            if let Ok(values) = &result {
                                validate_expression_result(snapshot, roots, values)?;
                            }
                            self.batches += 1;
                            return Ok(result);
                        }
                        Some(PipeEvent::Eof) => {
                            return Err("Expression worker exited without a result".into());
                        }
                        Some(PipeEvent::Failed(error)) => return Err(error),
                        None => {
                            if worker.status()?.is_some() {
                                return Err("Expression worker crashed; frame discarded".into());
                            }
                        }
                    }
                }
            })();
        if result.is_err() || result.as_ref().is_ok_and(|r| r.is_err()) {
            // Kill and reap on every timeout, cancellation, protocol or VM error.
            self.clear();
        }
        drop(permit);
        result.map_err(|message| ae::EvaluationError {
            kind: if cancel.load(Ordering::Relaxed) {
                ae::EvaluationErrorKind::Canceled
            } else if message.contains("limit") || message.contains("budget") {
                ae::EvaluationErrorKind::Budget
            } else {
                ae::EvaluationErrorKind::Runtime
            },
            message,
            property: None,
        })?
    }
}

/// Called on an existing background render/export worker, once per immutable
/// composition/time batch. Waiting for the one process permit is cancellable.
/// No expression JavaScript ever runs in this parent process.
pub(crate) fn evaluate_expressions(
    snapshot: &ae::CompositionSnapshot,
    roots: &[ae::PropertyAddress],
    cancel: Arc<AtomicBool>,
) -> Result<ae::EvaluatedProperties, ae::EvaluationError> {
    evaluate_expressions_with_timeout(snapshot, roots, cancel, EXECUTION_SLICE)
}

/// The harness lowers the production deadline to exercise native-loop preemption
/// without relying on the speed of a particular CPU or allocating enormous data.
pub(crate) fn evaluate_expressions_with_timeout(
    snapshot: &ae::CompositionSnapshot,
    roots: &[ae::PropertyAddress],
    cancel: Arc<AtomicBool>,
    timeout: Duration,
) -> Result<ae::EvaluatedProperties, ae::EvaluationError> {
    let result = (|| -> Result<Result<ae::EvaluatedProperties, ae::EvaluationError>, String> {
        let _guard = WorkerGuard::wait(&cancel)?;
        if roots.len() > 16_384 {
            return Err("Expression root budget exceeded".into());
        }
        let input = encode(
            &ExpressionInputRef { snapshot, roots },
            MAX_EXPRESSION_INPUT_BYTES,
        )?;
        let executable =
            std::env::current_exe().map_err(|e| format!("Cannot locate expression worker: {e}"))?;
        let deadline = Deadline::new(timeout, timeout);
        let mut worker = WorkerProcess::spawn(
            &executable,
            EXPRESSION_WORKER_FLAG,
            MAX_EXPRESSION_FRAME_BYTES,
        )?;
        worker.send(input)?;
        let mut complete = None;
        let mut eof = false;
        loop {
            deadline.check(&cancel)?;
            if !eof {
                match worker.receive()? {
                    Some(PipeEvent::Frame(bytes)) => {
                        deadline.check(&cancel)?;
                        if complete.is_some() {
                            return Err("Expression worker returned multiple results".into());
                        }
                        let mut result: Result<ae::EvaluatedProperties, ae::EvaluationError> =
                            decode(&bytes)?;
                        if let Err(error) = &mut result {
                            error.message = error.message.chars().take(4096).collect();
                        }
                        if let Ok(values) = &result {
                            validate_expression_result(snapshot, roots, values)?;
                        }
                        complete = Some(result);
                    }
                    Some(PipeEvent::Eof) => {
                        eof = true;
                        if complete.is_none() {
                            return Err("Expression worker exited without a result".into());
                        }
                    }
                    Some(PipeEvent::Failed(error)) => return Err(error),
                    None => {}
                }
            } else {
                std::thread::sleep(POLL);
            }
            if let Some(status) = worker.status()? {
                if !status.success() {
                    return Err("Expression worker crashed; frame discarded".into());
                }
                if eof {
                    return complete.ok_or_else(|| "Expression worker returned no result".into());
                }
            }
        }
    })();
    result.map_err(|message| ae::EvaluationError {
        kind: if cancel.load(Ordering::Relaxed) {
            ae::EvaluationErrorKind::Canceled
        } else if message.contains("limit") || message.contains("budget") {
            ae::EvaluationErrorKind::Budget
        } else {
            ae::EvaluationErrorKind::Runtime
        },
        message,
        property: None,
    })?
}

fn validate_expression_result(
    snapshot: &ae::CompositionSnapshot,
    roots: &[ae::PropertyAddress],
    result: &ae::EvaluatedProperties,
) -> Result<(), String> {
    if result.composition != snapshot.id
        || result.time != snapshot.time
        || result.values.len() > 16_384
        || result.dependencies.len() > 16_384
        || result.expression_evaluations > 2_048
        || result.host_reads > 20_000
        || roots.iter().any(|root| !result.values.contains_key(root))
    {
        return Err("Invalid expression worker frame identity or result budget".into());
    }
    let layers = snapshot
        .layers
        .iter()
        .map(|layer| (layer.id, layer))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (address, value) in &result.values {
        if address.composition != snapshot.id || !value.is_valid() {
            return Err("Invalid expression worker property".into());
        }
        let layer = layers
            .get(&address.layer)
            .ok_or("Expression worker returned an unknown layer")?;
        let source = match &address.property {
            ae::ExpressionProperty::Position => &layer.position,
            ae::ExpressionProperty::Scale => &layer.scale,
            ae::ExpressionProperty::Opacity => &layer.opacity,
            ae::ExpressionProperty::SourceText => layer
                .source_text
                .as_ref()
                .ok_or("Expression worker returned Source Text for a non-text layer")?,
            ae::ExpressionProperty::MaskPath(id) => {
                &layer
                    .masks
                    .iter()
                    .find(|mask| mask.id == *id)
                    .ok_or("Expression worker returned an unknown mask")?
                    .property
            }
            ae::ExpressionProperty::Slider(name) => {
                &layer
                    .sliders
                    .iter()
                    .find(|slider| &slider.name == name)
                    .ok_or("Expression worker returned an unknown slider")?
                    .property
            }
        };
        if std::mem::discriminant(value) != std::mem::discriminant(&source.authored_value) {
            return Err("Expression worker changed a property's dimensions".into());
        }
    }
    for (address, dependencies) in &result.dependencies {
        if !result.values.contains_key(address)
            || dependencies.len() > 16_384
            || dependencies
                .iter()
                .any(|dependency| !result.values.contains_key(dependency))
        {
            return Err("Invalid expression worker dependencies".into());
        }
    }
    fn depth<'a>(
        address: &'a ae::PropertyAddress,
        result: &'a ae::EvaluatedProperties,
        active: &mut BTreeSet<&'a ae::PropertyAddress>,
        depths: &mut std::collections::BTreeMap<&'a ae::PropertyAddress, usize>,
    ) -> Result<usize, String> {
        if let Some(depth) = depths.get(address) {
            return Ok(*depth);
        }
        if active.len() >= 32 || !active.insert(address) {
            return Err("Expression worker returned cyclic or excessive dependencies".into());
        }
        let mut maximum = 0;
        for dependency in result.dependencies.get(address).into_iter().flatten() {
            maximum = maximum.max(depth(dependency, result, active, depths)?);
        }
        active.remove(address);
        let depth = maximum + 1;
        if depth > 32 {
            return Err("Expression worker dependency depth budget exceeded".into());
        }
        depths.insert(address, depth);
        Ok(depth)
    }
    let mut depths = std::collections::BTreeMap::new();
    for address in result.values.keys() {
        depth(address, result, &mut BTreeSet::new(), &mut depths)?;
    }
    Ok(())
}

fn expression_worker_main() -> Result<(), String> {
    worker_limits()?;
    let mut input = io::stdin().lock();
    let bytes = read_frame(&mut input, MAX_EXPRESSION_INPUT_BYTES)?
        .ok_or("Expression worker requires a snapshot")?;
    let input: ExpressionInput = decode(&bytes)?;
    if input.roots.len() > 16_384 {
        return Err("Expression root budget exceeded".into());
    }
    let result = ae::ExpressionEvaluator::default().evaluate(&input.snapshot, &input.roots);
    let bytes = encode(&result, MAX_EXPRESSION_FRAME_BYTES)?;
    write_frame(&mut io::stdout().lock(), &bytes, MAX_EXPRESSION_FRAME_BYTES)
}

fn expression_session_main() -> Result<(), String> {
    worker_limits()?;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut transferred = 0usize;
    for _ in 0..16 {
        let Some(bytes) = read_frame(&mut input, MAX_EXPRESSION_INPUT_BYTES)? else {
            return Ok(());
        };
        transferred = transferred
            .checked_add(bytes.len() + 8)
            .ok_or("Worker IPC budget exceeded")?;
        let input: ExpressionInput = decode(&bytes)?;
        if input.roots.len() > 16_384 {
            return Err("Expression root budget exceeded".into());
        }
        // No globals, compiled closures, dependency or result caches survive a request.
        let result = ae::ExpressionEvaluator::default().evaluate(&input.snapshot, &input.roots);
        let bytes = encode(&result, MAX_EXPRESSION_FRAME_BYTES)?;
        transferred = transferred
            .checked_add(bytes.len() + 8)
            .ok_or("Worker IPC budget exceeded")?;
        if transferred > MAX_TOTAL_IPC_BYTES {
            return Err("Worker IPC budget exceeded".into());
        }
        write_frame(&mut output, &bytes, MAX_EXPRESSION_FRAME_BYTES)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn animated_project() -> Project {
        use libre_effects_core::{Command, Editor, Property};
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        for frame in [0, 24] {
            editor
                .execute(Command::ToggleKeyframe {
                    id: 1,
                    property: Property::Opacity,
                    frame,
                })
                .unwrap();
        }
        editor
            .execute(Command::NewProjectFolder {
                name: "IPC folder".into(),
                parent: None,
            })
            .unwrap();
        editor
            .execute(Command::ImportAsset {
                content: Content::Image { png: "YWJj".into() },
                width: 8.,
                height: 8.,
                name: "IPC image".into(),
                folder: Some(1),
                frame: None,
            })
            .unwrap();
        editor
            .execute(Command::MoveProjectItem {
                item: libre_effects_core::ProjectItem::Composition(1),
                folder: Some(1),
            })
            .unwrap();
        editor.execute(Command::NewComposition).unwrap();
        editor.project().clone()
    }

    #[test]
    fn animated_project_start_envelope_preserves_numeric_frame_keys() {
        let project = animated_project();
        let message = ParentMessage::Start {
            sharing: project.media_sharing(),
            project: project.clone(),
            selected_layer_ids: vec![1],
            frame: 24,
            source: "$.writeln('animated input');".into(),
        };
        let bytes = encode(&message, MAX_FRAME_BYTES).unwrap();
        let ParentMessage::Start {
            project: actual, ..
        } = decode(&bytes).unwrap()
        else {
            panic!("Wrong envelope variant")
        };
        assert_eq!(actual, project);
    }

    #[test]
    fn animated_project_complete_envelope_preserves_numeric_frame_keys() {
        let project = animated_project();
        let message = WorkerMessage::Complete {
            sharing: project.media_sharing(),
            project: project.clone(),
            selected_layer_ids: vec![1],
            output: vec![],
        };
        let bytes = encode(&message, MAX_FRAME_BYTES).unwrap();
        let WorkerMessage::Complete {
            project: actual, ..
        } = decode(&bytes).unwrap()
        else {
            panic!("Wrong envelope variant")
        };
        assert_eq!(actual, project);
    }

    #[test]
    fn frame_budgets_reject_before_payload_allocation() {
        let mut invalid = FRAME_MAGIC.to_vec();
        invalid.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(
            read_frame(&mut invalid.as_slice(), 64)
                .unwrap_err()
                .contains("oversized")
        );
        assert!(encode(&"a".repeat(65), 64).is_err());
        assert!(check_size(&vec!["a"; 128], 64).is_err());
        assert!(read_frame(&mut &[b'L'][..], 64).is_err());
        assert!(read_frame(&mut &[][..], 64).unwrap().is_none());
    }
    #[test]
    fn framed_roundtrip_and_truncation() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, b"{\"value\":12}", 64).unwrap();
        assert_eq!(
            read_frame(&mut bytes.as_slice(), 64).unwrap().unwrap(),
            b"{\"value\":12}"
        );
        bytes.pop();
        assert!(read_frame(&mut bytes.as_slice(), 64).is_err());
    }
    #[test]
    fn deadline_pauses_only_pending_ui_and_cancel_always_wins() {
        let cancel = AtomicBool::new(false);
        let mut deadline = Deadline::new(Duration::from_millis(10), Duration::from_millis(20));
        deadline.pause();
        std::thread::sleep(Duration::from_millis(25));
        assert!(deadline.check(&cancel).is_ok());
        cancel.store(true, Ordering::Relaxed);
        assert!(deadline.check(&cancel).unwrap_err().contains("canceled"));
        cancel.store(false, Ordering::Relaxed);
        deadline.resume();
        std::thread::sleep(Duration::from_millis(15));
        assert!(deadline.check(&cancel).unwrap_err().contains("time limit"));
    }
    #[test]
    fn only_one_process_guard_is_active() {
        let guard = WorkerGuard::acquire().unwrap();
        assert!(worker_active());
        assert!(WorkerGuard::acquire().is_err());
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(25));
            signal.store(true, Ordering::Relaxed);
        });
        assert!(WorkerGuard::wait(&cancel).is_err());
        thread.join().unwrap();
        assert!(worker_active());
        drop(guard);
        assert!(!worker_active());
    }
    #[test]
    fn source_scope_rejects_new_media_and_preserves_existing_images() {
        use libre_effects_core::{Command, Editor};
        let mut editor = Editor::default();
        let scope = SourceScope::collect(editor.project());
        editor
            .execute(Command::AddContent {
                content: Content::Image { png: "YWJj".into() },
                width: 8.,
                height: 8.,
                name: "Image".into(),
            })
            .unwrap();
        assert!(scope.validate(editor.project()).is_err());
        assert!(
            SourceScope::collect(editor.project())
                .validate(editor.project())
                .is_ok()
        );
    }
    #[test]
    fn cumulative_deadline_spans_ui_exchanges() {
        let cancel = AtomicBool::new(false);
        let mut deadline = Deadline::new(Duration::from_millis(100), Duration::from_millis(30));
        std::thread::sleep(Duration::from_millis(15));
        deadline.pause();
        std::thread::sleep(Duration::from_millis(40));
        assert!(deadline.check(&cancel).is_ok());
        deadline.resume();
        std::thread::sleep(Duration::from_millis(20));
        assert!(deadline.check(&cancel).is_err());
    }
    #[test]
    fn expression_results_reject_wrong_frames_dimensions_and_cycles() {
        use libre_effects_core::{Command, Editor};
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let snapshot = editor.project().expression_snapshot(1, 0).unwrap();
        let address = ae::PropertyAddress {
            composition: snapshot.id,
            layer: ae::LayerId(1),
            property: ae::ExpressionProperty::Opacity,
        };
        let valid = ae::EvaluatedProperties {
            composition: snapshot.id,
            time: snapshot.time,
            values: std::collections::BTreeMap::from([(
                address.clone(),
                ae::PropertyValue::Scalar(80.),
            )]),
            dependencies: Default::default(),
            expression_evaluations: 1,
            host_reads: 0,
        };
        let roots = std::slice::from_ref(&address);
        assert!(validate_expression_result(&snapshot, roots, &valid).is_ok());
        let mut wrong = valid.clone();
        wrong.time += 1.;
        assert!(validate_expression_result(&snapshot, roots, &wrong).is_err());
        let mut wrong = valid.clone();
        wrong
            .values
            .insert(address.clone(), ae::PropertyValue::Vector2([80., 80.]));
        assert!(validate_expression_result(&snapshot, roots, &wrong).is_err());
        let mut wrong = valid.clone();
        wrong
            .values
            .insert(address.clone(), ae::PropertyValue::Scalar(f64::NAN));
        assert!(validate_expression_result(&snapshot, roots, &wrong).is_err());
        let mut wrong = valid.clone();
        wrong
            .dependencies
            .insert(address.clone(), vec![address.clone()]);
        assert!(validate_expression_result(&snapshot, roots, &wrong).is_err());
        let mut wrong = valid.clone();
        wrong.values.clear();
        assert!(validate_expression_result(&snapshot, roots, &wrong).is_err());
    }
}
