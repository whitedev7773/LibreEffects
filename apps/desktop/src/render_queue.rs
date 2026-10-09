//! Durable, serial render jobs. Queue configuration has its own local history;
//! rendered files and execution state are never rolled back by Undo.
use libre_effects_core::Project;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Read,
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

#[cfg(test)]
#[path = "render_queue_tests.rs"]
mod tests;

pub(crate) use crate::output_settings::Format;
use crate::output_settings::{Settings, Spec};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Status {
    Queued,
    Rendering,
    Completed,
    Failed,
    Canceled,
    Interrupted,
}
impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Rendering => "Rendering",
            Self::Completed => "Done",
            Self::Failed => "Failed",
            Self::Canceled => "Canceled",
            Self::Interrupted => "Interrupted",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Output {
    #[serde(flatten)]
    pub spec: Spec,
    pub path: PathBuf,
    pub status: Status,
    pub message: String,
}
impl Output {
    pub fn new(format: Format, path: PathBuf) -> Self {
        Self {
            spec: format.into(),
            path,
            status: Status::Queued,
            message: String::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Job {
    pub id: u64,
    pub name: String,
    pub snapshot: String,
    pub project_path: Option<PathBuf>,
    // Required in v5: older readers must not silently discard original protection.
    pub protected_sources: Vec<PathBuf>,
    pub range: Range<u32>,
    pub duration: u32,
    pub description: String,
    pub enabled: bool,
    pub outputs: Vec<Output>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Preset {
    pub name: String,
    pub specs: Vec<Spec>,
}
// Version 5 persists imported originals separately from the native project path.
// Older readers must reject this envelope instead of silently losing protection.
const QUEUE_VERSION: u32 = 5;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Data {
    version: u32,
    pub jobs: Vec<Job>,
    pub presets: Vec<Preset>,
    pub stop_on_error: bool,
}
impl Default for Data {
    fn default() -> Self {
        Self {
            version: QUEUE_VERSION,
            jobs: Vec::new(),
            presets: Vec::new(),
            stop_on_error: true,
        }
    }
}
pub(crate) struct Queue {
    pub data: Data,
    root: PathBuf,
    next_id: u64,
    undo: Vec<Data>,
    redo: Vec<Data>,
    pub running: bool,
    pub active: Option<(u64, usize)>,
    pub progress: Arc<AtomicU32>,
    pub cancel: Arc<AtomicBool>,
    pub message: String,
}
pub(crate) fn default_root() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_STATE_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .map(|p| p.join("LibreEffects/render-queue"))
        .ok_or("Cannot locate render queue storage".into())
}
// Version 1 had no configurable output settings. Preserve an explicit font
// policy extension if present rather than quietly weakening it during migration.
// Unknown policy values remain intact so deserialization rejects them safely.
fn legacy_settings(existing: Option<&serde_json::Value>) -> serde_json::Value {
    let mut settings = serde_json::to_value(Settings::default()).unwrap();
    if let Some(fonts) = existing.and_then(|settings| settings.get("fonts")) {
        settings["fonts"] = fonts.clone();
    }
    settings
}

impl Queue {
    pub fn load(root: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
        let mut data: Data = match std::fs::File::open(root.join("queue.json")) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 1024 * 1024 {
                    return Err("Render queue exceeds 1 MiB".into());
                }
                {
                    let mut value: serde_json::Value = serde_json::from_slice(&bytes)
                        .map_err(|e| format!("Cannot read saved render queue: {e}"))?;
                    if value["version"] == 1 {
                        if let Some(jobs) = value
                            .get_mut("jobs")
                            .and_then(serde_json::Value::as_array_mut)
                        {
                            for job in jobs {
                                if let Some(outputs) = job
                                    .get_mut("outputs")
                                    .and_then(serde_json::Value::as_array_mut)
                                {
                                    for output in outputs {
                                        let settings = legacy_settings(output.get("settings"));
                                        output
                                            .as_object_mut()
                                            .ok_or("Invalid legacy output")?
                                            .insert("settings".into(), settings);
                                    }
                                }
                            }
                        }
                        if let Some(presets) = value
                            .get_mut("presets")
                            .and_then(serde_json::Value::as_array_mut)
                        {
                            for preset in presets {
                                let formats = preset["formats"]
                                    .as_array()
                                    .ok_or("Invalid legacy preset")?
                                    .clone();
                                let specs = formats
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, format)| {
                                        let settings = legacy_settings(
                                            preset
                                                .get("specs")
                                                .and_then(|specs| specs.get(index))
                                                .and_then(|spec| spec.get("settings")),
                                        );
                                        serde_json::json!({"format":format,"settings":settings})
                                    })
                                    .collect();
                                preset["specs"] = serde_json::Value::Array(specs);
                                preset.as_object_mut().unwrap().remove("formats");
                            }
                        }
                        value["version"] = 2.into();
                    }
                    if value["version"] == 2 {
                        // Old saved jobs and presets were silent. Preserve that intent;
                        // newly created output modules use automatic audio.
                        if let Some(jobs) = value
                            .get_mut("jobs")
                            .and_then(serde_json::Value::as_array_mut)
                        {
                            for job in jobs {
                                if let Some(outputs) = job
                                    .get_mut("outputs")
                                    .and_then(serde_json::Value::as_array_mut)
                                {
                                    for output in outputs {
                                        output
                                            .get_mut("settings")
                                            .and_then(serde_json::Value::as_object_mut)
                                            .ok_or("Invalid legacy output settings")?
                                            .insert("audio".into(), "Off".into());
                                    }
                                }
                            }
                        }
                        if let Some(presets) = value
                            .get_mut("presets")
                            .and_then(serde_json::Value::as_array_mut)
                        {
                            for preset in presets {
                                if let Some(specs) = preset
                                    .get_mut("specs")
                                    .and_then(serde_json::Value::as_array_mut)
                                {
                                    for spec in specs {
                                        spec.get_mut("settings")
                                            .and_then(serde_json::Value::as_object_mut)
                                            .ok_or("Invalid legacy preset settings")?
                                            .insert("audio".into(), "Off".into());
                                    }
                                }
                            }
                        }
                        value["version"] = 3.into();
                    }
                    if value["version"] == 3 {
                        // Existing policies (including Strict) survive unchanged.
                        // Settings' serde default fills only absent font policies;
                        // the subsequent atomic save makes them explicit in v4.
                        value["version"] = 4.into();
                    }
                    if value["version"] == 4 {
                        // Earlier jobs knew only their project_path. Keep it, and
                        // preserve any explicit source-protection extension.
                        if let Some(jobs) = value
                            .get_mut("jobs")
                            .and_then(serde_json::Value::as_array_mut)
                        {
                            for job in jobs {
                                job.as_object_mut()
                                    .ok_or("Invalid legacy job")?
                                    .entry("protected_sources")
                                    .or_insert_with(|| serde_json::json!([]));
                            }
                        }
                        value["version"] = QUEUE_VERSION.into();
                    }
                    serde_json::from_value(value)
                        .map_err(|e| format!("Cannot read saved render queue: {e}"))?
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e.to_string()),
        };
        validate(&data)?;
        for output in data.jobs.iter_mut().flat_map(|j| &mut j.outputs) {
            if output.status == Status::Rendering {
                output.status = Status::Interrupted;
                output.message =
                    "Editor stopped during rendering. Inspect the destination before retrying."
                        .into();
            }
        }
        let next_id = data.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
        let q = Self {
            data,
            root,
            next_id,
            undo: Vec::new(),
            redo: Vec::new(),
            running: false,
            active: None,
            progress: Default::default(),
            cancel: Default::default(),
            message: String::new(),
        };
        q.save(&q.data)?;
        // Only our generated, unreferenced snapshot files are reclaimed at startup.
        // In-session removals retain files so queue Undo remains possible.
        for entry in std::fs::read_dir(&q.root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name
                .strip_prefix("snapshot-")
                .and_then(|s| s.strip_suffix(".lfe.json"))
                .is_some_and(|s| {
                    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'-')
                })
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
                && !q.data.jobs.iter().any(|j| j.snapshot == name)
            {
                std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(q)
    }
    fn save(&self, data: &Data) -> Result<(), String> {
        validate(data)?;
        let bytes = serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("Render queue exceeds 1 MiB".into());
        }
        crate::project_io::write_bytes(&self.root.join("queue.json"), &bytes)
    }
    pub fn edit(
        &mut self,
        change: impl FnOnce(&mut Data) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.running {
            return Err("Stop the queue before changing jobs".into());
        }
        let mut next = self.data.clone();
        change(&mut next)?;
        validate(&next)?;
        self.validate_paths(&next)?;
        self.save(&next)?;
        if next != self.data {
            self.undo.push(std::mem::replace(&mut self.data, next));
            if self.undo.len() > 20 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        Ok(())
    }
    pub fn history(&mut self, redo: bool) -> Result<(), String> {
        if self.running {
            return Err("Stop rendering before editing the queue".into());
        }
        let source = if redo { &self.redo } else { &self.undo };
        let Some(next) = source.last().cloned() else {
            return Ok(());
        };
        self.validate_paths(&next)?;
        self.save(&next)?;
        let old = std::mem::replace(&mut self.data, next);
        if redo {
            self.redo.pop();
            self.undo.push(old);
        } else {
            self.undo.pop();
            self.redo.push(old);
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn enqueue(
        &mut self,
        project: &Project,
        project_path: Option<PathBuf>,
        range: Range<u32>,
        specs: &[Spec],
        directory: &Path,
    ) -> Result<u64, String> {
        self.enqueue_protected(project, project_path, None, range, specs, directory)
    }
    pub fn enqueue_protected(
        &mut self,
        project: &Project,
        project_path: Option<PathBuf>,
        imported_original: Option<PathBuf>,
        range: Range<u32>,
        specs: &[Spec],
        directory: &Path,
    ) -> Result<u64, String> {
        if self.running {
            return Err("Stop the queue before adding jobs".into());
        }
        if self.data.jobs.len() >= 100 || specs.is_empty() || specs.len() > 8 {
            return Err("Queue supports 100 jobs and 1–8 outputs per job".into());
        }
        let comp = project.composition();
        if range.is_empty() || range.end > comp.duration() {
            return Err("Invalid render range".into());
        }
        let directory = PathBuf::from(crate::media_io::path_string(
            &std::fs::canonicalize(directory).map_err(|e| e.to_string())?,
        )?);
        let mut id = self.next_id;
        while specs.iter().enumerate().any(|(n, f)| {
            directory
                .join(format!("render-{id}-{}.{}", n + 1, f.format.extension()))
                .exists()
        }) {
            id = id
                .checked_add(1)
                .filter(|n| *n < u64::MAX)
                .ok_or("Queue job ID overflow")?;
        }
        let mut outputs = Vec::new();
        for (n, spec) in specs.iter().cloned().enumerate() {
            spec.settings.plan(comp, range.clone(), spec.format)?;
            let format = spec.format;
            let path = directory.join(format!("render-{id}-{}.{}", n + 1, format.extension()));
            if path.exists() {
                return Err(format!(
                    "Output already exists: {}. Choose another folder or change the existing queue.",
                    path.display()
                ));
            }
            let mut output = Output::new(format, path);
            output.spec = spec;
            outputs.push(output);
        }
        let json = project.to_json()?;
        crate::project_io::validate_project_size(&json)?;
        let size: u64 = std::fs::read_dir(&self.root)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();
        if size.saturating_add(json.len() as u64) > 1024 * 1024 * 1024 {
            return Err("Queue snapshot storage exceeds 1 GiB; clear unused jobs and restart to reclaim history snapshots".into());
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let snapshot = format!("snapshot-{stamp}-{}.lfe.json", std::process::id());
        let path = self.root.join(&snapshot);
        crate::project_io::write_project(&path, &json)?;
        let job = Job {
            id,
            name: comp.name().into(),
            snapshot,
            project_path,
            protected_sources: imported_original.into_iter().collect(),
            range,
            duration: comp.duration(),
            description: format!("{} × {} · {} fps", comp.width(), comp.height(), comp.fps()),
            enabled: true,
            outputs,
        };
        if let Err(error) = self.edit(|data| {
            data.jobs.push(job);
            Ok(())
        }) {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        self.next_id = id + 1;
        Ok(id)
    }
    fn validate_paths(&self, data: &Data) -> Result<(), String> {
        let mut sources = Vec::new();
        for entry in std::fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_file() {
                sources.push(entry.path());
            }
        }
        for job in &data.jobs {
            let project = crate::project_io::read_project(&self.root.join(&job.snapshot))?;
            for output in &job.outputs {
                output.spec.settings.plan(
                    project.composition(),
                    job.range.clone(),
                    output.spec.format,
                )?;
            }
            if project.composition().duration() != job.duration {
                return Err("Queue snapshot duration mismatch".into());
            }
            sources.extend(
                crate::media_io::video_paths(&project)
                    .into_iter()
                    .map(PathBuf::from),
            );
            sources.extend(job.project_path.iter().cloned());
            sources.extend(job.protected_sources.iter().cloned());
        }
        let mut outputs: Vec<&Path> = Vec::new();
        for output in data.jobs.iter().flat_map(|j| &j.outputs) {
            let parent = output
                .path
                .parent()
                .ok_or("Output requires a parent folder")?;
            // Missing/offline destination folders fail only their output at runtime.
            let parent = std::fs::canonicalize(parent)
                .or_else(|_| crate::media_io::clean_absolute(parent).map_err(std::io::Error::other))
                .map_err(|e| e.to_string())?;
            if parent.starts_with(&self.root) {
                return Err("Do not render into queue storage".into());
            }
            for source in &sources {
                crate::project_io::protect_source(&output.path, source)?;
            }
            for other in &outputs {
                crate::project_io::protect_source(&output.path, other)
                    .map_err(|_| "Queue output paths must be distinct")?;
            }
            outputs.push(&output.path);
        }
        Ok(())
    }
    pub fn begin(&mut self) -> Result<(), String> {
        if self.running {
            return Err("Queue is already rendering".into());
        }
        self.validate_paths(&self.data)?;
        if !self
            .data
            .jobs
            .iter()
            .any(|j| j.enabled && j.outputs.iter().any(|o| o.status == Status::Queued))
        {
            return Err("No queued outputs. Retry failed or canceled outputs first.".into());
        }
        self.undo.clear();
        self.redo.clear();
        self.cancel.store(false, Ordering::Relaxed);
        self.running = true;
        self.message = "Rendering queue…".into();
        Ok(())
    }
}
fn validate(data: &Data) -> Result<(), String> {
    if data.version != QUEUE_VERSION || data.jobs.len() > 100 || data.presets.len() > 32 {
        return Err("Unsupported or oversized render queue".into());
    }
    let mut ids = BTreeSet::new();
    for job in &data.jobs {
        if job.id == 0
            || job.id == u64::MAX
            || !ids.insert(job.id)
            || job.name.len() > 1024
            || job.description.len() > 1024
            || job.duration == 0
            || job.range.is_empty()
            || job.range.end > job.duration
            || job.outputs.is_empty()
            || job.outputs.len() > 8
            || !job.snapshot.starts_with("snapshot-")
            || !job.snapshot.ends_with(".lfe.json")
            || job.snapshot.len() > 128
            || job.snapshot.contains(['/', '\\', ':'])
        {
            return Err("Invalid render queue job".into());
        }
        if job.project_path.as_ref().is_some_and(|p| !p.is_absolute())
            || job.protected_sources.len() > 1
            || job.protected_sources.iter().any(|p| !p.is_absolute())
        {
            return Err("Queue project paths must be absolute".into());
        }
        for output in &job.outputs {
            output.spec.settings.validate(output.spec.format)?;
            if !output.path.is_absolute()
                || output.message.len() > 16384
                || (!output.spec.format.sequence()
                    && !output.path.extension().is_some_and(|s| {
                        s.to_string_lossy()
                            .eq_ignore_ascii_case(output.spec.format.extension())
                    }))
            {
                return Err("Invalid queue output path or format".into());
            }
        }
    }
    let mut names = BTreeSet::new();
    for preset in &data.presets {
        for spec in &preset.specs {
            spec.settings.validate(spec.format)?;
        }
        if preset.name.trim().is_empty()
            || preset.name.len() > 80
            || preset.specs.is_empty()
            || preset.specs.len() > 8
            || !names.insert(&preset.name)
        {
            return Err("Presets require unique names and 1–8 formats".into());
        }
    }
    Ok(())
}

/// Runs one output at a time. Persist Rendering before starting side effects.
pub(crate) fn run(queue: Arc<Mutex<Queue>>) {
    let result = (|| -> Result<(), String> {
        loop {
            let (root, job, index, cancel, progress) = {
                let mut q = queue.lock().map_err(|e| e.to_string())?;
                if q.cancel.load(Ordering::Relaxed) {
                    break;
                }
                let Some((job, index)) = q.data.jobs.iter().filter(|j| j.enabled).find_map(|j| {
                    j.outputs
                        .iter()
                        .position(|o| o.status == Status::Queued)
                        .map(|i| (j.clone(), i))
                }) else {
                    break;
                };
                let mut next = q.data.clone();
                next.jobs
                    .iter_mut()
                    .find(|j| j.id == job.id)
                    .unwrap()
                    .outputs[index]
                    .status = Status::Rendering;
                q.save(&next)?;
                q.data = next;
                q.active = Some((job.id, index));
                q.progress.store(0, Ordering::Relaxed);
                (
                    q.root.clone(),
                    job,
                    index,
                    q.cancel.clone(),
                    q.progress.clone(),
                )
            };
            let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let project = crate::project_io::read_project(&root.join(&job.snapshot))?;
                execute_protected(
                    &project,
                    job.project_path.as_deref(),
                    &job.protected_sources,
                    job.range.clone(),
                    &job.outputs[index],
                    cancel.clone(),
                    progress,
                )
            }))
            .unwrap_or_else(|_| Err("Render worker failed".into()));
            let mut q = queue.lock().map_err(|e| e.to_string())?;
            let mut next = q.data.clone();
            let output = &mut next
                .jobs
                .iter_mut()
                .find(|j| j.id == job.id)
                .unwrap()
                .outputs[index];
            let failed = rendered.is_err();
            output.status = if rendered.is_ok() {
                Status::Completed
            } else if cancel.load(Ordering::Relaxed) {
                Status::Canceled
            } else {
                Status::Failed
            };
            output.message = match rendered {
                Ok(report) => report.completion("Completed"),
                Err(error) => error,
            };
            while output.message.len() > 16000 {
                output.message.pop();
            }
            q.save(&next)?;
            q.data = next;
            if failed && q.data.stop_on_error {
                break;
            }
        }
        Ok(())
    })();
    if let Ok(mut q) = queue.lock() {
        q.running = false;
        q.active = None;
        if result.is_err() {
            for o in q.data.jobs.iter_mut().flat_map(|j| &mut j.outputs) {
                if o.status == Status::Rendering {
                    o.status = Status::Interrupted;
                    o.message =
                        "Could not persist the result; inspect output before retrying".into();
                }
            }
        }
        q.message = result.err().unwrap_or_else(|| {
            if q.cancel.load(Ordering::Relaxed) {
                "Queue stopped; remaining outputs stay queued".into()
            } else {
                "Queue finished or stopped on error".into()
            }
        });
    }
}
#[cfg(test)]
pub(crate) fn execute(
    project: &Project,
    project_path: Option<&Path>,
    range: Range<u32>,
    output: &Output,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
) -> Result<crate::output_preflight::Report, String> {
    execute_protected(project, project_path, &[], range, output, cancel, progress)
}
pub(crate) fn execute_protected(
    project: &Project,
    project_path: Option<&Path>,
    protected_sources: &[PathBuf],
    range: Range<u32>,
    output: &Output,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
) -> Result<crate::output_preflight::Report, String> {
    for source in protected_sources {
        crate::project_io::protect_source(&output.path, source)?;
    }
    if !output.spec.format.sequence() {
        return crate::video_export::export_video_to(
            project,
            project_path,
            range,
            if output.spec.format == Format::Mp4 {
                crate::video_export::VideoPreset::H264
            } else {
                crate::video_export::VideoPreset::ProResAlpha
            },
            &output.spec.settings,
            &output.path,
            cancel,
            progress,
        );
    }
    let prepared = crate::output_preflight::check(
        project,
        project_path,
        range.clone(),
        output.spec.format,
        &output.spec.settings,
        &output.path,
        crate::output_preflight::Destination::NewSequence,
        &crate::video_export::ffmpeg_path(),
        &cancel,
    )
    .map_err(|report| report.to_string())?;
    let plan = prepared.plan;
    let staging = tempfile::Builder::new()
        .prefix(".libre-render-")
        .tempdir_in(output.path.parent().ok_or("Output needs a parent")?)
        .map_err(|e| e.to_string())?;
    let lanes = crate::render_pipeline::workers(plan.width, plan.height, plan.frames);
    let renderers = crate::render_pipeline::renderers(lanes, cancel.clone());
    crate::render_pipeline::ordered(
        plan.frames,
        lanes,
        &cancel,
        |lane, index| {
            let frame = plan.source_frame(index);
            renderers[lane].render_output(project, frame, plan.width, plan.height)
        },
        |index, mut pixels| {
            output.spec.settings.apply_channels(
                &mut pixels,
                output.spec.format,
                project.composition().background_color(),
            );
            std::fs::write(
                staging
                    .path()
                    .join(format!("frame-{:06}.png", plan.sequence_first() + index)),
                output.spec.settings.png_bytes(pixels, output.spec.format)?,
            )
            .map_err(|e| e.to_string())?;
            progress.store(index as u32 + 1, Ordering::Relaxed);
            Ok(())
        },
    )?;
    let manifest = serde_json::json!({"composition":project.composition().name(),"fps":plan.fps,"width":plan.width,"height":plan.height,"first_frame":plan.sequence_first(),"source_range":range,"rendered_frames":plan.frames,"complete":true,"alpha":output.spec.settings.channels(output.spec.format)==crate::output_settings::Channels::Rgba,"pattern":"frame-%06d.png"});
    crate::project_io::write_bytes(
        &staging.path().join("sequence.json"),
        manifest.to_string().as_bytes(),
    )?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Render canceled; sequence destination unchanged".into());
    }
    std::fs::rename(staging.path(), &output.path).map_err(|e| e.to_string())?;
    Ok(prepared.report)
}
