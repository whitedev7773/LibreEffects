//! Shared, non-destructive output checks. A successful check is a point-in-time
//! readiness test, not a promise of free disk space or future filesystem access.
use crate::output_settings::{AudioOutput, FontPolicy, Format, Plan, Settings};
use libre_effects_core::{Content, Project};
use std::{
    collections::BTreeSet,
    fmt,
    io::{Read, Seek, SeekFrom, Write},
    ops::Range,
    path::Path,
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Warning,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Code {
    Settings,
    Source,
    Destination,
    FontSubstitution,
    EncoderUnavailable,
    EncoderCapability,
    Canceled,
}
impl Code {
    fn label(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Source => "source",
            Self::Destination => "destination",
            Self::FontSubstitution => "font-substitution",
            Self::EncoderUnavailable => "encoder-unavailable",
            Self::EncoderCapability => "encoder-capability",
            Self::Canceled => "canceled",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    pub message: String,
    pub remedy: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    pub diagnostics: Vec<Diagnostic>,
}
impl Report {
    fn error(&mut self, code: Code, message: impl Into<String>, remedy: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            code,
            message: message.into(),
            remedy: remedy.into(),
        });
    }
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
    pub fn completion(&self, success: &str) -> String {
        if self.diagnostics.is_empty() {
            success.into()
        } else {
            format!("{success} · {}", self)
        }
    }
}
impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index != 0 {
                writeln!(f)?;
            }
            write!(
                f,
                "{} [{}]: {} {}",
                if diagnostic.severity == Severity::Error {
                    "Error"
                } else {
                    "Warning"
                },
                diagnostic.code.label(),
                diagnostic.message,
                diagnostic.remedy,
            )?;
        }
        Ok(())
    }
}
#[derive(Debug)]
pub(crate) struct Prepared {
    pub plan: Plan,
    pub report: Report,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    File,
    NewSequence,
}
pub(crate) fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// Run once per output, on a worker. Never cached across renders: executable,
/// mounts, permissions and linked source files can change between queue jobs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    project: &Project,
    project_path: Option<&Path>,
    range: Range<u32>,
    format: Format,
    settings: &Settings,
    destination: &Path,
    kind: Destination,
    executable: &Path,
    cancel: &AtomicBool,
) -> Result<Prepared, Report> {
    let mut report = Report::default();
    if canceled(cancel, &mut report) {
        return Err(report);
    }
    let plan = match settings.plan(project.composition(), range.clone(), format) {
        Ok(plan) => Some(plan),
        Err(error) => {
            report.error(
                Code::Settings,
                error,
                "Adjust the output settings or frame range.",
            );
            None
        }
    };
    if let Some(source) = project_path {
        if let Err(error) = crate::project_io::protect_source(destination, source) {
            report.error(
                Code::Source,
                error,
                "Keep the project and output paths distinct.",
            );
        }
    }
    let mut fonts = BTreeSet::new();
    if let Err(error) = crate::project_io::validate_render_with(
        project,
        destination,
        &range,
        cancel,
        &mut |comp, layer| {
            if !matches!(layer.content(), Content::Text { .. }) || !fonts.insert(layer.id()) {
                return;
            }
            let requested = layer.text_style();
            if crate::fonts::warning(&requested).is_none() {
                return;
            }
            let actual = crate::fonts::resolved(&requested);
            report.diagnostics.push(Diagnostic {
                severity: if settings.fonts == FontPolicy::Strict { Severity::Error } else { Severity::Warning },
                code: Code::FontSubstitution,
                message: format!(
                    "{} / {}: requested {} / {} ({}{}); using {} / {} ({}{}).",
                    comp.name(), layer.name(), requested.font_family,
                    if requested.font_face.is_empty() { "automatic face" } else { &requested.font_face },
                    requested.weight, if requested.italic { " italic" } else { "" },
                    actual.font_family, actual.font_face, actual.weight,
                    if actual.italic { " italic" } else { "" },
                ),
                remedy: if settings.fonts == FontPolicy::Strict {
                    "Fonts policy: strict. Install the requested font (restart a running editor), replace it in Manage project fonts, or choose Fonts: fallback.".into()
                } else {
                    "Fonts policy: fallback. Rendering uses the preview's replacement; install the font (restart a running editor) or replace it in Manage project fonts for an exact match.".into()
                },
            });
        },
    ) {
        if canceled(cancel, &mut report) {
            return Err(report);
        }
        report.error(
            Code::Source,
            error,
            "Resolve the source or range issue before retrying.",
        );
    }
    if let Err(error) = check_destination(destination, kind) {
        report.error(
            Code::Destination,
            error,
            "Choose an existing writable output folder and a valid output name.",
        );
    }
    if canceled(cancel, &mut report) || report.has_errors() {
        return Err(report);
    }
    let plan = plan.expect("invalid plans have a diagnostic");
    if !format.sequence() {
        let has_audio = if settings.audio == AudioOutput::Auto {
            match crate::audio_mix::Mixer::new(project, false) {
                Ok(mixer) => mixer.has_audio(),
                Err(error) => {
                    report.error(
                        Code::Source,
                        error,
                        "Repair source audio, or choose Audio: off.",
                    );
                    return Err(report);
                }
            }
        } else {
            false
        };
        if let Err(diagnostic) = check_encoder(
            project,
            &plan,
            format,
            settings,
            destination,
            executable,
            has_audio,
            cancel,
        ) {
            report.diagnostics.push(diagnostic);
            return Err(report);
        }
    }
    if canceled(cancel, &mut report) {
        return Err(report);
    }
    Ok(Prepared { plan, report })
}
fn canceled(cancel: &AtomicBool, report: &mut Report) -> bool {
    if !cancel.load(Ordering::Relaxed) {
        return false;
    }
    report.error(
        Code::Canceled,
        "Render canceled; destination unchanged.",
        "Retry when ready.",
    );
    true
}

fn check_destination(destination: &Path, kind: Destination) -> Result<(), String> {
    if destination.file_name().is_none() {
        return Err("Output needs a filename or a new sequence folder name.".into());
    }
    match std::fs::symlink_metadata(destination) {
        Ok(metadata) => {
            if kind == Destination::NewSequence {
                return Err(
                    "PNG sequence destination must be a new folder; choose another output path."
                        .into(),
                );
            }
            if !metadata.is_file() && !metadata.file_type().is_symlink() {
                return Err(format!(
                    "Output is not a regular file: {}.",
                    destination.display()
                ));
            }
            // This catches Windows's read-only-file replacement failure without
            // opening/truncating the existing output. The actual publish can
            // still fail (e.g. an open Windows handle), and remains atomic.
            if metadata.permissions().readonly() {
                return Err(format!(
                    "Existing output is read-only: {}.",
                    destination.display()
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Cannot inspect output {}: {error}.",
                destination.display()
            ));
        }
    }
    let directory = parent(destination);
    let metadata = std::fs::metadata(directory).map_err(|error| {
        format!(
            "Cannot access output folder {}: {error}.",
            directory.display()
        )
    })?;
    if !metadata.is_dir() {
        return Err(format!(
            "Output parent is not a folder: {}.",
            directory.display()
        ));
    }
    // Windows marks many writable directories read-only; only use this extra
    // conservative check on Unix, followed by a real sibling create/write/sync.
    #[cfg(unix)]
    if metadata.permissions().readonly() {
        return Err(format!(
            "Output folder is marked read-only: {}.",
            directory.display()
        ));
    }
    let mut probe = tempfile::Builder::new()
        .prefix(".libre-preflight-")
        .tempfile_in(directory)
        .map_err(|error| {
            format!(
                "Cannot create a temporary output in {}: {error}.",
                directory.display()
            )
        })?;
    probe
        .write_all(b"Libre Effects output readiness check")
        .and_then(|_| probe.as_file().sync_all())
        .map_err(|error| {
            format!(
                "Cannot write temporary output in {}: {error}.",
                directory.display()
            )
        })?;
    // No disk-free estimate: compressed size, audio mixes and other writers are
    // not predictable. Every real write must still handle disk-full safely.
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn check_encoder(
    project: &Project,
    plan: &Plan,
    format: Format,
    settings: &Settings,
    destination: &Path,
    executable: &Path,
    has_audio: bool,
    cancel: &AtomicBool,
) -> Result<(), Diagnostic> {
    const LOG_LIMIT: u64 = 64 * 1024;
    const OUTPUT_LIMIT: u64 = 1024 * 1024;
    let failure = |code, message: String| Diagnostic {
        severity: Severity::Error,
        code,
        message,
        remedy: match code {
            Code::Canceled => "Retry when ready.".into(),
            Code::EncoderUnavailable => {
                "Install FFmpeg on PATH or set LIBRE_EFFECTS_FFMPEG to its executable.".into()
            }
            Code::Destination => "Choose an existing writable output folder.".into(),
            _ => format!(
                "Use an FFmpeg build supporting {}{} and the selected output settings, or choose another format.",
                if format == Format::Mp4 {
                    "libx264 / MP4"
                } else {
                    "prores_ks / MOV"
                },
                if has_audio {
                    if format == Format::Mp4 {
                        " with AAC audio"
                    } else {
                        " with PCM audio"
                    }
                } else {
                    ""
                }
            ),
        },
    };
    let io_error = |error: std::io::Error| {
        failure(
            Code::Destination,
            format!("Cannot prepare the temporary encoder check: {error}."),
        )
    };
    let scratch = tempfile::Builder::new()
        .prefix(".libre-encoder-check-")
        .tempdir_in(parent(destination))
        .map_err(io_error)?;
    let input = scratch.path().join("input.rgba");
    std::fs::write(&input, [0_u8; 16 * 16 * 4]).map_err(io_error)?;
    let output = scratch.path().join("output");
    let mut log = tempfile::tempfile().map_err(io_error)?;
    let mut cmd = crate::video_export::command(executable);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        "16x16",
        "-framerate",
    ])
    .arg(plan.fps.to_string())
    .args(["-i"])
    .arg(&input);
    if has_audio {
        // The same zero bytes also form a tiny silent stereo float input. No
        // source decoding or project-frame rendering occurs during this check.
        cmd.args(["-f", "f32le", "-ar", "48000", "-ac", "2", "-i"])
            .arg(&input)
            .args([
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-movie_timescale",
                "48000",
            ]);
        if format == Format::Mp4 {
            cmd.args(["-c:a", "aac", "-b:a", "192k"]);
        } else {
            cmd.args(["-c:a", "pcm_s24le"]);
        }
    } else {
        cmd.args(["-map", "0:v:0", "-an"]);
    }
    crate::video_export::add_output_args(
        &mut cmd,
        if format == Format::Mp4 {
            crate::video_export::VideoPreset::H264
        } else {
            crate::video_export::VideoPreset::ProResAlpha
        },
        settings,
        project.composition().background_color(),
    );
    if project.composition().display_start() != 0 {
        cmd.args(["-timecode", &plan.timecode]);
    }
    cmd.args(["-frames:v", "1"])
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log.try_clone().map_err(io_error)?);
    if cancel.load(Ordering::Relaxed) {
        return Err(failure(
            Code::Canceled,
            "Render canceled; destination unchanged.".into(),
        ));
    }
    let mut child = cmd.spawn().map_err(|error| {
        failure(
            Code::EncoderUnavailable,
            format!("Cannot start FFmpeg at {}: {error}.", executable.display()),
        )
    })?;
    let started = Instant::now();
    let status = loop {
        let canceled = cancel.load(Ordering::Relaxed);
        let log_size = log.metadata().map(|m| m.len()).unwrap_or(LOG_LIMIT + 1);
        let output_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
        if canceled
            || started.elapsed() > Duration::from_secs(10)
            || log_size > LOG_LIMIT
            || output_size > OUTPUT_LIMIT
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(failure(
                if canceled {
                    Code::Canceled
                } else {
                    Code::EncoderCapability
                },
                if canceled {
                    "Render canceled; destination unchanged.".into()
                } else {
                    "FFmpeg capability check exceeded its 10-second or size limit; destination unchanged.".into()
                },
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failure(
                    Code::EncoderCapability,
                    format!("Cannot complete the FFmpeg capability check: {error}."),
                ));
            }
        }
    };
    if !status.success()
        || std::fs::metadata(&output)
            .ok()
            .is_none_or(|m| m.len() == 0 || m.len() > OUTPUT_LIMIT)
    {
        let mut diagnostic = String::new();
        let _ = log.seek(SeekFrom::Start(0));
        let _ = log.take(4096).read_to_string(&mut diagnostic);
        return Err(failure(
            Code::EncoderCapability,
            format!(
                "FFmpeg cannot encode the selected {} settings ({status}): {}",
                format.label(),
                diagnostic.trim()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "output_preflight_tests.rs"]
mod tests;
