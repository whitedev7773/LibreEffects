//! Stream composited RGBA frames to FFmpeg; publish only a complete video.
use crate::output_settings::{Format, RateControl, Settings};
use crate::rendering::Renderer;
use libre_effects_core::Project;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    ops::Range,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoPreset {
    H264,
    ProResAlpha,
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command as Edit, Content, Editor, Property};
    fn scene() -> Project {
        let mut e = Editor::default();
        e.execute(Edit::ConfigureComposition {
            name: "Export QA".into(),
            width: 101,
            height: 99,
            fps: 24,
            duration: 8,
        })
        .unwrap();
        e.execute(Edit::AddContent {
            content: Content::Rectangle,
            width: 40.0,
            height: 40.0,
            name: "Red".into(),
        })
        .unwrap();
        e.execute(Edit::SetColor {
            id: 1,
            color: 0xff0000,
        })
        .unwrap();
        e.execute(Edit::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 0.0,
        })
        .unwrap();
        e.execute(Edit::ToggleKeyframe {
            id: 1,
            property: Property::Opacity,
            frame: 0,
        })
        .unwrap();
        for (frame, value) in [(2, 50.0), (5, 100.0)] {
            e.execute(Edit::SetValue {
                id: 1,
                property: Property::Opacity,
                frame,
                value,
            })
            .unwrap();
        }
        e.project().clone()
    }
    #[test]
    #[ignore = "requires FFmpeg; validates exact fractional clocks and start timecode"]
    fn fractional_rate_mp4_and_mov_preserve_frame_count_duration_and_timecode() {
        use libre_effects_core::FrameRate;
        let dir = tempfile::tempdir().unwrap();
        for rate in ["24000/1001", "30000/1001"] {
            let fps: FrameRate = rate.parse().unwrap();
            let mut e = Editor::default();
            e.replace_project(scene()).unwrap();
            e.execute(Edit::ConfigureCompositionRate {
                name: "Fractional export".into(),
                width: 101,
                height: 99,
                fps,
                duration: 120,
                display_start: fps.nominal() * 3600,
            })
            .unwrap();
            e.execute(Edit::Precompose {
                layers: vec![1],
                name: "Nested clock".into(),
            })
            .unwrap();
            let project = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
                let path = dir
                    .path()
                    .join(format!("fractional.{}", preset.extension()));
                export_video(
                    &project,
                    2..62,
                    preset,
                    &path,
                    Default::default(),
                    Default::default(),
                )
                .unwrap();
                let probe = command(&crate::footage::probe_path()).args([
                    "-v", "error", "-select_streams", "v:0", "-count_frames", "-show_entries",
                    "stream=r_frame_rate,avg_frame_rate,time_base,duration_ts,nb_read_frames:stream_tags=timecode", "-of", "json",
                ]).arg(&path).output().unwrap();
                assert!(
                    probe.status.success(),
                    "{}",
                    String::from_utf8_lossy(&probe.stderr)
                );
                let metadata: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
                let stream = &metadata["streams"][0];
                assert_eq!(stream["r_frame_rate"], rate);
                assert_eq!(stream["avg_frame_rate"], rate);
                assert_eq!(stream["nb_read_frames"], "60");
                assert_eq!(stream["tags"]["timecode"], "01:00:00:02");
                let (n, d) = stream["time_base"]
                    .as_str()
                    .unwrap()
                    .split_once('/')
                    .unwrap();
                let ticks = stream["duration_ts"].as_u64().unwrap();
                assert_eq!(
                    u128::from(ticks) * n.parse::<u128>().unwrap() * u128::from(fps.numerator()),
                    60 * u128::from(fps.denominator()) * d.parse::<u128>().unwrap()
                );
                let decoded = command(&ffmpeg_path())
                    .args(["-v", "error", "-i"])
                    .arg(&path)
                    .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(decoded.status.success());
                let (width, height) = if preset == VideoPreset::H264 {
                    (102, 100)
                } else {
                    (101, 99)
                };
                assert_eq!(decoded.stdout.len(), width * height * 4 * 60);
                let first = &decoded.stdout[(49 * width + 50) * 4..][..4];
                if preset == VideoPreset::H264 {
                    assert!((i32::from(first[0]) - 128).abs() < 8, "{first:?}");
                } else {
                    assert!((i32::from(first[3]) - 128).abs() < 3, "{first:?}");
                }
            }
        }
    }
    #[test]
    fn canceled_or_failed_encoder_preserves_destination_and_cleans_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.mp4");
        std::fs::write(&path, b"original").unwrap();
        for canceled in [true, false] {
            let result = encode(
                &scene(),
                2..5,
                VideoPreset::H264,
                &path,
                &dir.path().join("missing-ffmpeg"),
                Arc::new(AtomicBool::new(canceled)),
                Default::default(),
            );
            assert!(result.is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"original");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
    #[test]
    #[ignore = "requires FFmpeg with libx264 and prores_ks; run explicitly for export validation"]
    fn ffmpeg_roundtrip_preserves_range_rate_alpha_and_black_matte() {
        let dir = tempfile::tempdir().unwrap();
        for (nested, stacked) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut e = Editor::default();
            e.replace_project(scene()).unwrap();
            if stacked {
                use libre_effects_core::{EffectEdit, EffectKind, EffectParam};
                e.execute(Edit::Effect {
                    id: 1,
                    edit: EffectEdit::Add(EffectKind::Fill),
                })
                .unwrap();
                for parameter in [EffectParam::Red, EffectParam::Blue] {
                    e.execute(Edit::Effect {
                        id: 1,
                        edit: EffectEdit::SetValue {
                            effect: 1,
                            parameter,
                            frame: 0,
                            value: 0.0,
                        },
                    })
                    .unwrap();
                }
            }
            if nested {
                e.execute(Edit::Precompose {
                    layers: vec![1],
                    name: "Nested export".into(),
                })
                .unwrap();
            }
            // A full-frame preview guide must not change either encoded format.
            e.execute(Edit::AddContent {
                content: Content::Rectangle,
                width: 101.0,
                height: 99.0,
                name: "Preview guide".into(),
            })
            .unwrap();
            e.execute(Edit::SetLayerSwitch {
                id: e.selected().unwrap(),
                switch: libre_effects_core::LayerSwitch::Guide,
                enabled: true,
            })
            .unwrap();
            for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
                let path = dir
                    .path()
                    .join(format!("한글 output.{}", preset.extension()));
                let progress = Arc::new(AtomicU32::new(0));
                export_video(
                    e.project(),
                    2..5,
                    preset,
                    &path,
                    Default::default(),
                    progress.clone(),
                )
                .unwrap();
                assert_eq!(progress.load(Ordering::Relaxed), 3);
                let metadata = command(&crate::footage::probe_path())
                    .args([
                        "-v",
                        "error",
                        "-select_streams",
                        "v:0",
                        "-show_entries",
                        "stream=color_range,color_space,color_transfer,color_primaries",
                        "-of",
                        "json",
                    ])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(metadata.status.success());
                let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
                let stream = &metadata["streams"][0];
                assert_eq!(stream["color_space"], "bt709");
                assert_eq!(stream["color_primaries"], "bt709");
                assert_eq!(stream["color_transfer"], "iec61966-2-1");
                if preset == VideoPreset::H264 {
                    assert_eq!(stream["color_range"], "tv");
                }
                let decoded = command(&ffmpeg_path())
                    .args(["-v", "error", "-i"])
                    .arg(&path)
                    .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    decoded.status.success(),
                    "{}",
                    String::from_utf8_lossy(&decoded.stderr)
                );
                let (w, h) = if preset == VideoPreset::H264 {
                    (102, 100)
                } else {
                    (101, 99)
                };
                assert_eq!(decoded.stdout.len(), w * h * 4 * 3);
                let center = &decoded.stdout[(49 * w + 50) * 4..][..4];
                let background = &decoded.stdout[..4];
                if preset == VideoPreset::H264 {
                    assert!(
                        (center[usize::from(stacked)] as i32 - 128).abs() < 8,
                        "{center:?}"
                    );
                    assert!(center[usize::from(!stacked)] < 8 && center[2] < 8);
                    assert_eq!(background, [0, 0, 0, 255]);
                } else {
                    assert!((center[3] as i32 - 128).abs() < 3, "{center:?}");
                    assert!(center[usize::from(stacked)] > 245);
                    assert_eq!(background[3], 0);
                }
                // Decode at 24 fps; an incorrect encoded time base changes this count.
                let rate = command(&ffmpeg_path())
                    .args(["-v", "error", "-i"])
                    .arg(&path)
                    .args([
                        "-vf", "fps=24", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(rate.status.success());
                assert_eq!(rate.stdout.len(), decoded.stdout.len());
            }
        }
    }
    #[test]
    #[ignore = "requires FFmpeg; validates cancellation after encoder startup"]
    fn cancel_running_encoder_leaves_existing_video_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keep.mp4");
        std::fs::write(&path, b"keep me").unwrap();
        let mut e = Editor::default();
        e.replace_project(scene()).unwrap();
        e.execute(Edit::ConfigureComposition {
            name: "Long".into(),
            width: 101,
            height: 99,
            fps: 24,
            duration: 10000,
        })
        .unwrap();
        let progress = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (p, c) = (progress.clone(), cancel.clone());
        let stopper = std::thread::spawn(move || {
            let deadline = Instant::now();
            while p.load(Ordering::Relaxed) < 2 && deadline.elapsed() < Duration::from_secs(15) {
                std::thread::sleep(Duration::from_millis(2));
            }
            c.store(true, Ordering::Relaxed);
        });
        let result = export_video(
            e.project(),
            0..10000,
            VideoPreset::H264,
            &path,
            cancel,
            progress.clone(),
        );
        stopper.join().unwrap();
        assert!(result.unwrap_err().contains("canceled"));
        assert!(progress.load(Ordering::Relaxed) >= 2);
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    #[ignore = "requires FFmpeg; checks colored mattes, translucent edges and odd-size padding"]
    fn composition_background_is_baked_into_mp4_but_not_alpha_mov() {
        let dir = tempfile::tempdir().unwrap();
        for (preset, color, expected_center) in [
            (VideoPreset::H264, 0x2060a0, [144_u8, 48, 80]),
            (VideoPreset::H264, 0xffffff, [255, 127, 127]),
            (VideoPreset::ProResAlpha, 0x2060a0, [255, 0, 0]),
        ] {
            let mut e = Editor::default();
            e.replace_project(scene()).unwrap();
            e.execute(Edit::SetCompositionBackground(color)).unwrap();
            let project = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            let path = dir
                .path()
                .join(format!("matte-{color:06x}.{}", preset.extension()));
            export_video(
                &project,
                2..5,
                preset,
                &path,
                Default::default(),
                Default::default(),
            )
            .unwrap();
            let decoded = command(&ffmpeg_path())
                .args(["-v", "error", "-i"])
                .arg(&path)
                .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                .output()
                .unwrap();
            assert!(decoded.status.success());
            let (width, height) = if preset == VideoPreset::H264 {
                (102, 100)
            } else {
                (101, 99)
            };
            assert_eq!(decoded.stdout.len(), width * height * 4 * 3);
            let pixel = |x, y| &decoded.stdout[(y * width + x) * 4..][..4];
            let center = pixel(50, 49);
            for channel in 0..3 {
                assert!(
                    (center[channel] as i32 - expected_center[channel] as i32).abs() < 9,
                    "{preset:?} center: {center:?}"
                );
            }
            if preset == VideoPreset::H264 {
                let expected = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
                // Includes the added right and bottom rows, which must not become black seams.
                for (x, y) in [(0, 0), (101, 0), (0, 99), (101, 99)] {
                    let p = pixel(x, y);
                    for channel in 0..3 {
                        assert!(
                            (p[channel] as i32 - expected[channel] as i32).abs() < 5,
                            "{x},{y}: {p:?}"
                        );
                    }
                    assert_eq!(p[3], 255);
                }
            } else {
                assert!((center[3] as i32 - 128).abs() < 3);
                assert_eq!(pixel(0, 0)[3], 0);
            }
        }
    }
}
impl VideoPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264 MP4",
            Self::ProResAlpha => "ProRes 4444 MOV · Alpha",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::H264 => "mp4",
            Self::ProResAlpha => "mov",
        }
    }
}
pub(crate) fn ffmpeg_path() -> PathBuf {
    std::env::var_os("LIBRE_EFFECTS_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "ffmpeg".into())
}
fn command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}
struct Encoder {
    child: Arc<Mutex<Child>>,
    done: Arc<AtomicBool>,
}
impl Drop for Encoder {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(crate) fn export_video(
    project: &Project,
    range: Range<u32>,
    preset: VideoPreset,
    destination: &Path,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
) -> Result<(), String> {
    export_video_with_settings(
        project,
        range,
        preset,
        &Settings::default(),
        destination,
        cancel,
        progress,
    )
}
pub(crate) fn export_video_with_settings(
    project: &Project,
    range: Range<u32>,
    preset: VideoPreset,
    settings: &Settings,
    destination: &Path,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
) -> Result<(), String> {
    encode_with_settings(
        project,
        range,
        preset,
        settings,
        destination,
        &ffmpeg_path(),
        cancel,
        progress,
    )
}
#[cfg(test)]
fn encode(
    project: &Project,
    range: Range<u32>,
    preset: VideoPreset,
    destination: &Path,
    executable: &Path,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
) -> Result<(), String> {
    encode_with_settings(
        project,
        range,
        preset,
        &Settings::default(),
        destination,
        executable,
        cancel,
        progress,
    )
}
fn encode_with_settings(
    project: &Project,
    range: Range<u32>,
    preset: VideoPreset,
    settings: &Settings,
    destination: &Path,
    executable: &Path,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
) -> Result<(), String> {
    let comp = project.composition();
    let format = if preset == VideoPreset::H264 {
        Format::Mp4
    } else {
        Format::MovAlpha
    };
    let plan = settings.plan(comp, range.clone(), format)?;
    crate::project_io::validate_render(project, destination, &range)?;
    if range.is_empty() || range.end > comp.duration() {
        return Err("Choose a non-empty work area inside the composition".into());
    }
    if comp.width() as u64 * comp.height() as u64 > 33_554_432 {
        return Err("Video export supports up to 32 megapixels per frame".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Render canceled".into());
    }
    let directory = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let output = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    let audio = if settings.audio == crate::output_settings::AudioOutput::Auto {
        crate::audio_mix::prepare(project, &plan, directory, &cancel, &progress)?
    } else {
        None
    };
    let mut log = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut cmd = command(executable);
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
    ])
    .arg(format!("{}x{}", plan.width, plan.height))
    .args(["-framerate", &plan.fps.to_string(), "-i", "pipe:0"]);
    if let Some(audio) = &audio {
        // The default 1 kHz movie clock rounds AAC edit-list duration to a
        // millisecond. Use the mix clock so its final sample remains representable.
        cmd.args(["-f", "f32le", "-ar", "48000", "-ac", "2", "-i"])
            .arg(audio.path())
            .args([
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-movie_timescale",
                "48000",
            ]);
        if preset == VideoPreset::H264 {
            cmd.args(["-c:a", "aac", "-b:a", "192k"]);
        } else {
            cmd.args(["-c:a", "pcm_s24le"]);
        }
    } else {
        cmd.args(["-map", "0:v:0", "-an"]);
    }
    // Working pixels are nonlinear sRGB. Preserve that transfer function, explicitly
    // encode a BT.709 YCbCr matrix at limited range, and tag both the stream/container.
    cmd.args([
        "-color_primaries",
        "bt709",
        "-color_trc",
        "iec61966-2-1",
        "-colorspace",
        "bt709",
        "-color_range",
        "tv",
    ]);
    match preset {
        VideoPreset::H264 => {
            cmd.arg("-vf")
                .arg(format!(
                    "pad=ceil(iw/2)*2:ceil(ih/2)*2:color=0x{:06x},scale=in_range=full:out_range=limited:out_color_matrix=bt709,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709",
                    if settings.channels(format)==crate::output_settings::Channels::Alpha {0} else {comp.background_color()}
                ))
                .args([
                    "-c:v",
                    "libx264",
                    "-preset",
                    settings.encoder_speed.as_deref().unwrap_or("medium"),
                    "-pix_fmt",
                    "yuv420p",
                    "-movflags",
                    "+faststart",
                    "-f",
                    "mp4",
                ]);
            match settings.rate_control.unwrap_or(RateControl::Crf(18)) {
                RateControl::Crf(n) => {
                    cmd.args(["-crf", &n.to_string()]);
                }
                RateControl::Bitrate(n) => {
                    cmd.args(["-b:v", &format!("{n}k")]);
                }
            }
        }
        VideoPreset::ProResAlpha => {
            cmd.args([
                "-vf",
                "scale=in_range=full:out_range=limited:out_color_matrix=bt709,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709",
                "-c:v",
                "prores_ks",
                "-profile:v",
                "4",
                "-pix_fmt",
                if settings.channels(format)==crate::output_settings::Channels::Rgba {"yuva444p10le"}else{"yuv444p10le"},
                "-alpha_bits",
                if settings.channels(format)==crate::output_settings::Channels::Rgba {"16"}else{"0"},
                "-movflags",
                "+write_colr",
                "-f",
                "mov",
            ]);
        }
    }
    if comp.display_start() != 0 {
        cmd.args(["-timecode", &plan.timecode]);
    }
    cmd.arg(output.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log.try_clone().map_err(|e| e.to_string())?));
    let mut child = cmd.spawn().map_err(|e| format!("Cannot start FFmpeg: {e}. Install FFmpeg on PATH or set LIBRE_EFFECTS_FFMPEG to its executable."))?;
    let mut input = child.stdin.take().ok_or("Cannot open encoder input")?;
    let encoder = Encoder {
        child: Arc::new(Mutex::new(child)),
        done: Arc::new(AtomicBool::new(false)),
    };
    let timed_out = Arc::new(AtomicBool::new(false));
    // Kill the encoder independently of a blocked pipe write, so cancellation also
    // works while FFmpeg is stalled or finalizing the container.
    let (child, done, canceled, completed, timeout) = (
        encoder.child.clone(),
        encoder.done.clone(),
        cancel.clone(),
        progress.clone(),
        timed_out.clone(),
    );
    let monitor = std::thread::spawn(move || {
        let mut last_progress = completed.load(Ordering::Relaxed);
        let mut changed = Instant::now();
        while !done.load(Ordering::Relaxed) {
            let current = completed.load(Ordering::Relaxed);
            if current != last_progress {
                changed = Instant::now();
                last_progress = current;
            }
            let stalled = changed.elapsed() > Duration::from_secs(120);
            if canceled.load(Ordering::Relaxed) || stalled {
                timeout.store(stalled, Ordering::Relaxed);
                if let Ok(mut child) = child.lock() {
                    let _ = child.kill();
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    let result = (|| {
        let renderer = Renderer::with_cancel(cancel.clone());
        for index in 0..plan.frames {
            let frame = plan.source_frame(index);
            if cancel.load(Ordering::Relaxed) {
                return Err("Render canceled".into());
            }
            let mut pixels = renderer.render_output(project, frame, plan.width, plan.height)?;
            settings.apply_channels(&mut pixels, format, comp.background_color());
            input
                .write_all(pixels.as_raw())
                .map_err(|e| format!("Encoder input failed: {e}"))?;
            progress.store(index as u32 + 1, Ordering::Relaxed);
        }
        drop(input);
        loop {
            if let Some(status) = encoder
                .child
                .lock()
                .map_err(|e| e.to_string())?
                .try_wait()
                .map_err(|e| e.to_string())?
            {
                if !status.success() {
                    return Err(format!("FFmpeg exited with {status}"));
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    })();
    drop(encoder);
    let _ = monitor.join();
    if cancel.load(Ordering::Relaxed) {
        return Err("Render canceled; destination unchanged".into());
    }
    if timed_out.load(Ordering::Relaxed) {
        return Err("Encoder stalled for 120 seconds; destination unchanged".into());
    }
    if let Err(error) = result {
        let mut diagnostic = String::new();
        let _ = log.seek(SeekFrom::Start(0));
        let _ = log.take(8192).read_to_string(&mut diagnostic);
        return Err(format!("{error}: {}", diagnostic.trim()));
    }
    output.as_file().sync_all().map_err(|e| e.to_string())?;
    if output
        .as_file()
        .metadata()
        .map_err(|e| e.to_string())?
        .len()
        == 0
    {
        return Err("Encoder produced an empty video".into());
    }
    output.persist(destination).map_err(|e| e.to_string())?;
    Ok(())
}
