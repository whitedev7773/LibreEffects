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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Format {
    Mp4,
    MovAlpha,
    PngAlpha,
    PngBackground,
}
impl Format {
    pub const ALL: [Self; 4] = [
        Self::Mp4,
        Self::MovAlpha,
        Self::PngAlpha,
        Self::PngBackground,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "H.264 MP4",
            Self::MovAlpha => "ProRes 4444 · Alpha",
            Self::PngAlpha => "PNG sequence · Alpha",
            Self::PngBackground => "PNG sequence · Background",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::MovAlpha => "mov",
            _ => "frames",
        }
    }
    pub fn sequence(self) -> bool {
        matches!(self, Self::PngAlpha | Self::PngBackground)
    }
}
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
    pub format: Format,
    pub path: PathBuf,
    pub status: Status,
    pub message: String,
}
impl Output {
    pub fn new(format: Format, path: PathBuf) -> Self {
        Self {
            format,
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
    pub range: Range<u32>,
    pub duration: u32,
    pub description: String,
    pub enabled: bool,
    pub outputs: Vec<Output>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Preset {
    pub name: String,
    pub formats: Vec<Format>,
}
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
            version: 1,
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
                serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Cannot read saved render queue: {e}"))?
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
    pub fn enqueue(
        &mut self,
        project: &Project,
        project_path: Option<PathBuf>,
        range: Range<u32>,
        formats: &[Format],
        directory: &Path,
    ) -> Result<u64, String> {
        if self.running {
            return Err("Stop the queue before adding jobs".into());
        }
        if self.data.jobs.len() >= 100 || formats.is_empty() || formats.len() > 8 {
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
        while formats.iter().enumerate().any(|(n, f)| {
            directory
                .join(format!("render-{id}-{}.{}", n + 1, f.extension()))
                .exists()
        }) {
            id = id
                .checked_add(1)
                .filter(|n| *n < u64::MAX)
                .ok_or("Queue job ID overflow")?;
        }
        let mut outputs = Vec::new();
        for (n, format) in formats.iter().copied().enumerate() {
            let path = directory.join(format!("render-{id}-{}.{}", n + 1, format.extension()));
            if path.exists() {
                return Err(format!(
                    "Output already exists: {}. Choose another folder or change the existing queue.",
                    path.display()
                ));
            }
            outputs.push(Output::new(format, path));
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
            if project.composition().duration() != job.duration {
                return Err("Queue snapshot duration mismatch".into());
            }
            sources.extend(
                crate::media_io::video_paths(&project)
                    .into_iter()
                    .map(PathBuf::from),
            );
            sources.extend(job.project_path.iter().cloned());
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
    if data.version != 1 || data.jobs.len() > 100 || data.presets.len() > 32 {
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
        if job.project_path.as_ref().is_some_and(|p| !p.is_absolute()) {
            return Err("Queue project paths must be absolute".into());
        }
        for output in &job.outputs {
            if !output.path.is_absolute()
                || output.message.len() > 16384
                || (!output.format.sequence()
                    && !output.path.extension().is_some_and(|s| {
                        s.to_string_lossy()
                            .eq_ignore_ascii_case(output.format.extension())
                    }))
            {
                return Err("Invalid queue output path or format".into());
            }
        }
    }
    let mut names = BTreeSet::new();
    for preset in &data.presets {
        if preset.name.trim().is_empty()
            || preset.name.len() > 80
            || preset.formats.is_empty()
            || preset.formats.len() > 8
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
                execute(
                    &project,
                    job.project_path.as_deref(),
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
            output.message = rendered.err().unwrap_or_else(|| "Completed".into());
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
pub(crate) fn execute(
    project: &Project,
    project_path: Option<&Path>,
    range: Range<u32>,
    output: &Output,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
) -> Result<(), String> {
    if let Some(path) = project_path {
        crate::project_io::protect_source(&output.path, path)?;
    }
    crate::project_io::validate_render(project, &output.path, &range)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Render canceled".into());
    }
    if !output.format.sequence() {
        return crate::video_export::export_video(
            project,
            range,
            if output.format == Format::Mp4 {
                crate::video_export::VideoPreset::H264
            } else {
                crate::video_export::VideoPreset::ProResAlpha
            },
            &output.path,
            cancel,
            progress,
        );
    }
    if output.path.exists() {
        return Err(
            "PNG sequence destination must be a new folder; choose another output path".into(),
        );
    }
    let staging = tempfile::Builder::new()
        .prefix(".libre-render-")
        .tempdir_in(output.path.parent().ok_or("Output needs a parent")?)
        .map_err(|e| e.to_string())?;
    let renderer = crate::rendering::Renderer::new();
    for (index, frame) in range.clone().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Render canceled; sequence destination unchanged".into());
        }
        let mut pixels = renderer.render(project, frame, u32::MAX)?;
        if output.format == Format::PngBackground {
            crate::rendering::composite_background(
                &mut pixels,
                project.composition().background_color(),
            );
        }
        pixels
            .save(staging.path().join(format!("frame-{frame:06}.png")))
            .map_err(|e| e.to_string())?;
        progress.store(index as u32 + 1, Ordering::Relaxed);
    }
    let manifest = serde_json::json!({"composition":project.composition().name(),"fps":project.composition().fps(),"width":project.composition().width(),"height":project.composition().height(),"first_frame":range.start,"rendered_frames":range.len(),"complete":true,"alpha":output.format==Format::PngAlpha,"pattern":"frame-%06d.png"});
    crate::project_io::write_bytes(
        &staging.path().join("sequence.json"),
        manifest.to_string().as_bytes(),
    )?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Render canceled; sequence destination unchanged".into());
    }
    std::fs::rename(staging.path(), &output.path).map_err(|e| e.to_string())?;
    Ok(())
}
