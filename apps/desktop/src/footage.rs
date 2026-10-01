//! Linked local footage. Decode off the UI thread and keep a bounded frame cache.
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    collections::VecDeque,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};

#[derive(Debug)]
pub(crate) struct VideoInfo {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub source_fps: f64,
}
fn command(executable: &Path) -> Command {
    let mut c = Command::new(executable);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}
pub(crate) fn probe_path() -> PathBuf {
    if let Some(p) = std::env::var_os("LIBRE_EFFECTS_FFPROBE") {
        return p.into();
    }
    let ffmpeg = crate::video_export::ffmpeg_path();
    if ffmpeg.is_absolute() {
        ffmpeg.with_file_name(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        })
    } else {
        "ffprobe".into()
    }
}
// File-backed pipes avoid deadlocks and unbounded memory on malformed media.
fn output(mut cmd: Command, limit: u64) -> Result<Vec<u8>, String> {
    let mut stdout = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut stderr = tempfile::tempfile().map_err(|e| e.to_string())?;
    cmd.stdin(Stdio::null())
        .stdout(stdout.try_clone().map_err(|e| e.to_string())?)
        .stderr(stderr.try_clone().map_err(|e| e.to_string())?);
    let mut child = cmd.spawn().map_err(|e| {
        format!("Cannot start media decoder: {e}. Install FFmpeg and FFprobe on PATH.")
    })?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
        if started.elapsed() > Duration::from_secs(15)
            || stdout.metadata().map(|m| m.len()).unwrap_or(limit + 1) > limit
            || stderr.metadata().map(|m| m.len()).unwrap_or(65537) > 65536
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Media decoding exceeded its time or size limit".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !status.success() {
        let _ = stderr.seek(SeekFrom::Start(0));
        let mut diagnostic = String::new();
        let _ = stderr.take(2048).read_to_string(&mut diagnostic);
        return Err(format!("Cannot decode footage: {}", diagnostic.trim()));
    }
    stdout.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    stdout
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Decoded media is too large".into());
    }
    Ok(bytes)
}
pub(crate) fn probe(path: &Path) -> Result<VideoInfo, String> {
    if !path.is_file() {
        return Err("Choose an existing local video file".into());
    }
    let absolute = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let path = crate::media_io::path_string(&absolute)?;
    let mut cmd = command(&probe_path());
    cmd.args(["-v", "error", "-protocol_whitelist", "file,pipe", "-select_streams", "v:0", "-show_entries", "stream=width,height,duration,sample_aspect_ratio,avg_frame_rate,r_frame_rate:stream_tags=DURATION:stream_side_data=rotation:format=duration", "-of", "json"]).arg(&path);
    let json: serde_json::Value =
        serde_json::from_slice(&output(cmd, 1024 * 1024)?).map_err(|e| e.to_string())?;
    let stream = json["streams"]
        .as_array()
        .and_then(|s| s.first())
        .ok_or("No video stream found")?;
    let width = stream["width"].as_u64().unwrap_or(0) as u32;
    let rate = |key: &str| -> Option<f64> {
        let (n, d) = stream[key].as_str()?.split_once('/')?;
        Some(n.parse::<f64>().ok()? / d.parse::<f64>().ok()?)
    };
    let source_fps = rate("avg_frame_rate").ok_or("Video frame rate is unknown")?;
    if !source_fps.is_finite()
        || !(1.0..=240.0).contains(&source_fps)
        || rate("r_frame_rate").is_none_or(|r| !r.is_finite() || (r - source_fps).abs() > 0.01)
    {
        return Err("Import constant-frame-rate footage (1–240 fps); convert variable-frame-rate footage first".into());
    }
    let height = stream["height"].as_u64().unwrap_or(0) as u32;
    let duration = stream["duration"]
        .as_str()
        .and_then(|v| v.parse::<f64>().ok())
        .or_else(|| {
            // Matroska stores per-stream duration in a tag. The container may be
            // longer because of audio; using it would decode past the final video frame.
            let parts = stream["tags"]["DURATION"]
                .as_str()?
                .split(':')
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            (parts.len() == 3).then(|| parts[0] * 3600.0 + parts[1] * 60.0 + parts[2])
        })
        .or_else(|| json["format"]["duration"].as_str()?.parse::<f64>().ok())
        .unwrap_or(0.0);
    let sar = stream["sample_aspect_ratio"].as_str().unwrap_or("1:1");
    if !matches!(sar, "1:1" | "0:1" | "N/A") {
        return Err("Convert anamorphic footage to square pixels before importing".into());
    }
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|s| s.iter().find_map(|v| v["rotation"].as_i64()))
        .unwrap_or(0);
    let (width, height) = if rotation.rem_euclid(180) == 90 {
        (height, width)
    } else {
        (width, height)
    };
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || !duration.is_finite()
        || !(0.0..=86400.0).contains(&duration)
        || duration == 0.0
    {
        return Err(
            "Video must have a known duration up to 24 hours and dimensions up to 4096 × 4096"
                .into(),
        );
    }
    Ok(VideoInfo {
        path,
        width,
        height,
        duration,
        source_fps,
    })
}

#[derive(PartialEq)]
struct Key {
    path: String,
    modified: Option<SystemTime>,
    bytes: u64,
    micros: u64,
    width: u32,
    height: u32,
}
static CACHE: OnceLock<Mutex<VecDeque<(Key, String)>>> = OnceLock::new();
pub(crate) fn clear_cache() {
    if let Ok(mut c) = CACHE.get_or_init(Default::default).lock() {
        c.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command as Edit, Content, Editor};

    #[test]
    fn missing_sources_are_reported_without_starting_a_decoder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            probe(&dir.path().join("missing.mov"))
                .unwrap_err()
                .contains("existing")
        );
        assert!(
            frame_png(
                dir.path().join("missing.mov").to_str().unwrap(),
                0.0,
                64,
                48,
                64
            )
            .unwrap_err()
            .contains("offline")
        );
    }

    #[test]
    #[ignore = "requires FFmpeg and FFprobe; imports and re-exports generated CFR footage"]
    fn footage_import_sampling_and_video_export_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("한글 source.mkv");
        let status = command(&crate::video_export::ffmpeg_path()).args(["-v", "error", "-f", "lavfi", "-i", "color=red:r=4:s=64x48:d=1.5,drawbox=c=lime:t=fill:enable='gte(t,0.5)',drawbox=c=blue:t=fill:enable='gte(t,1)'", "-f", "lavfi", "-i", "sine=duration=3", "-c:v", "ffv1", "-c:a", "pcm_s16le"]).arg(&path).status().unwrap();
        assert!(status.success());
        let info = probe(&path).unwrap();
        assert_eq!((info.width, info.height, info.source_fps), (64, 48, 4.0));
        assert!((info.duration - 1.5).abs() < 0.001);
        let mut e = Editor::default();
        e.execute(Edit::ConfigureComposition {
            name: "Footage QA".into(),
            width: 64,
            height: 48,
            fps: 8,
            duration: 20,
        })
        .unwrap();
        e.execute(Edit::AddContent {
            content: Content::Video {
                path: info.path,
                duration: info.duration,
                source_fps: info.source_fps,
                start_frame: 2,
                playback: Default::default(),
            },
            width: 64.0,
            height: 48.0,
            name: "Source".into(),
        })
        .unwrap();
        let renderer = crate::rendering::Renderer::new();
        for (frame, channel) in [(2, 0), (6, 1), (10, 2), (13, 2)] {
            let image = renderer.render(e.project(), frame, 64).unwrap();
            let pixel = image.get_pixel(20, 20);
            assert!(pixel[channel] > 240, "frame {frame}: {pixel:?}");
            assert_eq!(pixel[3], 255);
        }
        assert_eq!(
            renderer
                .render(e.project(), 14, 64)
                .unwrap()
                .get_pixel(20, 20)[3],
            0
        );
        let destination = dir.path().join("composite.mp4");
        crate::video_export::export_video(
            e.project(),
            2..14,
            crate::video_export::VideoPreset::H264,
            &destination,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let mut decoder = command(&crate::video_export::ffmpeg_path());
        decoder
            .args(["-v", "error", "-i"])
            .arg(&destination)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
        let pixels = output(decoder, 1024 * 1024).unwrap();
        assert_eq!(pixels.len(), 64 * 48 * 4 * 12);
        for (frame, channel) in [(0, 0), (4, 1), (8, 2), (11, 2)] {
            let p = &pixels[frame * 64 * 48 * 4 + (20 * 64 + 20) * 4..][..4];
            assert!(p[channel] > 230, "{frame}: {p:?}");
        }
        std::fs::remove_file(&path).unwrap();
        assert!(
            renderer
                .render(e.project(), 2, 64)
                .unwrap_err()
                .contains("offline")
        );
        let completed = std::fs::read(&destination).unwrap();
        assert!(
            crate::video_export::export_video(
                e.project(),
                2..14,
                crate::video_export::VideoPreset::H264,
                &destination,
                Default::default(),
                Default::default()
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&destination).unwrap(), completed);
    }

    #[test]
    #[ignore = "requires FFmpeg and FFprobe; validates fractional frame timestamps"]
    fn final_frame_at_different_composition_rate_is_decodable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("24fps.mp4");
        assert!(
            command(&crate::video_export::ffmpeg_path())
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=blue:r=24:s=64x48:d=1",
                    "-c:v",
                    "libx264"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let info = probe(&path).unwrap();
        let content = Content::Video {
            path: info.path.clone(),
            duration: info.duration,
            source_fps: info.source_fps,
            start_frame: 0,
            playback: Default::default(),
        };
        let seconds = content.video_time(29, 30).unwrap();
        let data = STANDARD
            .decode(frame_png(&info.path, seconds, 64, 48, 64).unwrap())
            .unwrap();
        let decoded = image::load_from_memory(&data).unwrap().to_rgba8();
        assert!(decoded.get_pixel(20, 20)[2] > 240);
    }

    #[test]
    #[ignore = "requires FFmpeg; verifies retimed preview pixels against encoded output"]
    fn retimed_footage_preview_and_exports_agree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timing.mkv");
        assert!(command(&crate::video_export::ffmpeg_path())
            .args(["-v", "error", "-f", "lavfi", "-i", "color=red:r=4:s=64x48:d=1.5,drawbox=c=lime:t=fill:enable='gte(t,0.5)',drawbox=c=blue:t=fill:enable='gte(t,1)'", "-c:v", "ffv1"])
            .arg(&path).status().unwrap().success());
        let mut e = Editor::default();
        e.execute(Edit::ConfigureComposition {
            name: "Timing QA".into(),
            width: 64,
            height: 48,
            fps: 8,
            duration: 20,
        })
        .unwrap();
        e.execute(Edit::AddContent {
            content: Content::Video {
                path: path.to_str().unwrap().into(),
                duration: 1.5,
                source_fps: 4.0,
                start_frame: 2,
                playback: Default::default(),
            },
            width: 64.0,
            height: 48.0,
            name: "Video".into(),
        })
        .unwrap();
        let id = e.selected().unwrap();
        let renderer = crate::rendering::Renderer::new();
        // Each case uses the same nonzero work-area start and unchanged layer duration.
        for (name, edit, channels) in [
            (
                "reverse",
                Edit::ReverseVideo { id },
                vec![2, 2, 2, 2, 1, 1, 1, 1, 0, 0, 0, 0],
            ),
            (
                "slow",
                Edit::SetVideoSpeed { id, speed: 0.5 },
                vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1],
            ),
            ("freeze", Edit::FreezeVideo { id, frame: 7 }, vec![1; 12]),
            (
                "slip",
                Edit::SetVideoSourceIn { id, seconds: 0.5 },
                vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3],
            ),
        ] {
            e.execute(edit).unwrap();
            let saved = e.project().to_json().unwrap();
            let project = libre_effects_core::Project::from_json(&saved).unwrap();
            for (offset, channel) in channels.iter().copied().enumerate() {
                let image = renderer.render(&project, 2 + offset as u32, 64).unwrap();
                let p = image.get_pixel(20, 20);
                if channel == 3 {
                    assert_eq!(p[3], 0);
                } else {
                    assert!(p[channel] > 240, "{name} frame {offset}: {p:?}");
                }
            }
            let destination = dir.path().join(format!("{name}.mp4"));
            crate::video_export::export_video(
                &project,
                2..14,
                crate::video_export::VideoPreset::H264,
                &destination,
                Default::default(),
                Default::default(),
            )
            .unwrap();
            let mut decoder = command(&crate::video_export::ffmpeg_path());
            decoder
                .args(["-v", "error", "-i"])
                .arg(&destination)
                .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
            let pixels = output(decoder, 1024 * 1024).unwrap();
            assert_eq!(pixels.len(), 64 * 48 * 4 * channels.len());
            for (offset, channel) in channels.iter().copied().enumerate() {
                let p = &pixels[offset * 64 * 48 * 4 + (20 * 64 + 20) * 4..][..4];
                if channel == 3 {
                    assert!(p[..3].iter().all(|v| *v < 10), "{name}: {p:?}");
                } else {
                    assert!(p[channel] > 230, "{name} frame {offset}: {p:?}");
                }
            }
            e.undo();
        }
    }
}
pub(crate) fn interpreted_frame_png(
    path: &str,
    seconds: f64,
    width: u32,
    height: u32,
    max_dimension: u32,
    interpretation: libre_effects_core::FootageInterpretation,
) -> Result<String, String> {
    let alpha_changed = interpretation.alpha != libre_effects_core::AlphaInterpretation::Straight
        || interpretation.invert_alpha;
    // Interpret before scaling so matte fringes and hidden colors use source pixels.
    let png = frame_png(
        path,
        seconds,
        width,
        height,
        if alpha_changed {
            width.max(height)
        } else {
            max_dimension
        },
    )?;
    Ok(crate::source_render::alpha_png(&png, interpretation)?.into_owned())
}
pub(crate) fn frame_png(
    path: &str,
    seconds: f64,
    width: u32,
    height: u32,
    max_dimension: u32,
) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|_| format!("Footage offline: {path}. Use File → Relink selected video."))?;
    if !metadata.is_file() {
        return Err("Footage path is not a file".into());
    }
    let scale = (max_dimension as f64 / width.max(height) as f64).min(1.0);
    let (width, height) = (
        (width as f64 * scale).round().max(1.0) as u32,
        (height as f64 * scale).round().max(1.0) as u32,
    );
    let key = Key {
        path: path.into(),
        modified: metadata.modified().ok(),
        bytes: metadata.len(),
        micros: (seconds * 1_000_000.0).round() as u64,
        width,
        height,
    };
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(mut c) = cache.lock() {
        if let Some(index) = c.iter().position(|(k, _)| *k == key) {
            let entry = c.remove(index).unwrap();
            let result = entry.1.clone();
            c.push_back(entry);
            return Ok(result);
        }
    }
    let mut cmd = command(&crate::video_export::ffmpeg_path());
    cmd.args([
        "-v",
        "error",
        "-nostdin",
        "-protocol_whitelist",
        "file,pipe",
        "-ss",
        &format!("{seconds:.6}"),
        "-i",
        path,
        "-map",
        "0:v:0",
        "-an",
        "-sn",
        "-frames:v",
        "1",
        "-vf",
        &format!("scale={width}:{height},setsar=1"),
        "-pix_fmt",
        "rgba",
        "-c:v",
        "png",
        "-f",
        "image2pipe",
        "pipe:1",
    ]);
    let data = output(cmd, 80 * 1024 * 1024)?;
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(format!("No video frame at {seconds:.3}s in {path}"));
    }
    let encoded = STANDARD.encode(data);
    if let Ok(mut c) = cache.lock() {
        while !c.is_empty()
            && (c.len() >= 24
                || c.iter().map(|(_, s)| s.len()).sum::<usize>() + encoded.len() > 32 * 1024 * 1024)
        {
            c.pop_front();
        }
        if encoded.len() <= 32 * 1024 * 1024 {
            c.push_back((key, encoded.clone()));
        }
    }
    Ok(encoded)
}
