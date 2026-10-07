use super::*;
use libre_effects_core::{Command, Editor, LayerSwitch, TrackEdit};

fn scene(path: &str, fps: FrameRate) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureCompositionRate {
        name: "Audio mix QA".into(),
        width: 64,
        height: 48,
        fps,
        duration: 180,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Audio {
            path: path.into(),
            audio: AudioMetadata {
                stream_index: 0,
                sample_rate: SAMPLE_RATE,
                channels: 2,
                channel_layout: "stereo".into(),
                duration: 3.0,
                start_time: 0.0,
                file_offset: 0.0,
            },
            start_frame: 0,
            playback: Default::default(),
        },
        width: 1.0,
        height: 1.0,
        name: "Sound".into(),
    })
    .unwrap();
    e
}
fn cached(e: &Editor, preview: bool) -> Mixer {
    let mut m = Mixer::new(e.project(), preview).unwrap();
    for source in 0..m.cache.sources.len() {
        for second in 0..3 {
            m.cache.chunks.push_back(Chunk {
                source,
                second,
                pcm: (0..SAMPLE_RATE)
                    .map(|i| {
                        let time = second as f32 + i as f32 / SAMPLE_RATE as f32;
                        [(1.0 + time) / 8.0, -(1.0 + time) / 8.0]
                    })
                    .collect(),
            });
        }
    }
    m
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-6, "{a} != {b}");
}

#[test]
fn mixer_uses_continuous_clock_and_identical_block_boundaries() {
    let mut e = scene("missing.wav", "30000/1001".parse().unwrap());
    e.execute(Command::SetVideoSpeed { id: 1, speed: 1.5 })
        .unwrap();
    let mut one = cached(&e, false);
    let a = one
        .render(0.1, 0, 48000, 3.0, &AtomicBool::new(false))
        .unwrap();
    let mut blocks = cached(&e, false);
    let mut b = Vec::new();
    for start in (0..48000).step_by(157) {
        b.extend(
            blocks
                .render(
                    0.1,
                    start as u64,
                    157.min(48000 - start),
                    3.0,
                    &AtomicBool::new(false),
                )
                .unwrap(),
        );
    }
    assert_eq!(a, b);
    near(a[0][0], (1.0 + 0.15) / 8.0);
    near(a[32000][0], (1.0 + 1.15) / 8.0);
    assert_eq!(sample_count(31, "30000/1001".parse().unwrap()), 49650);
    assert_eq!(
        sample_count(180000, "30000/1001".parse().unwrap()),
        288288000
    );
}

#[test]
fn mixer_trims_splits_reverses_freezes_and_clips_the_work_area() {
    let mut e = scene("missing.wav", 30.into());
    e.execute(Command::SetLayerRange {
        id: 1,
        start: 30,
        end: 60,
    })
    .unwrap();
    let mut m = cached(&e, false);
    let b = m
        .render(0.99, 0, 1000, 6.0, &AtomicBool::new(false))
        .unwrap();
    assert!(b[..480].iter().all(|v| *v == [0.0; 2]));
    near(b[480][0], 0.25);
    e.execute(Command::ReverseVideo { id: 1 }).unwrap();
    let b = cached(&e, false)
        .render(1.0, 0, 1000, 6.0, &AtomicBool::new(false))
        .unwrap();
    near(b[0][0], (1.0 + 59.0 / 30.0) / 8.0);
    near(b[999][0], (1.0 + 59.0 / 30.0 - 999.0 / 48000.0) / 8.0);
    e.execute(Command::FreezeVideo { id: 1, frame: 45 })
        .unwrap();
    assert!(
        Mixer::new(e.project(), false)
            .unwrap()
            .render(1.0, 0, 48000, 6.0, &AtomicBool::new(false))
            .unwrap()
            .iter()
            .all(|v| *v == [0.0; 2])
    );
    let mut e = scene("missing.wav", 30.into());
    let before = cached(&e, false)
        .render(1.0, 0, 48000, 6.0, &AtomicBool::new(false))
        .unwrap();
    e.execute(Command::SplitLayers {
        ids: vec![1],
        frame: 45,
    })
    .unwrap();
    let after = cached(&e, false)
        .render(1.0, 0, 48000, 6.0, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(before, after);
    e.undo();
    assert_eq!(
        before,
        cached(&e, false)
            .render(1.0, 0, 48000, 6.0, &AtomicBool::new(false))
            .unwrap()
    );
    e.redo();
    let json = e.project().to_json().unwrap();
    e.replace_project(Project::from_json(&json).unwrap())
        .unwrap();
    assert_eq!(
        before,
        cached(&e, false)
            .render(1.0, 0, 48000, 6.0, &AtomicBool::new(false))
            .unwrap()
    );
    let b = cached(&e, false)
        .render(0.0, 0, 48000, 0.5, &AtomicBool::new(false))
        .unwrap();
    assert!(b[24000..].iter().all(|v| *v == [0.0; 2]));
}

#[test]
fn nested_audio_follows_remap_guide_solo_and_ignores_visual_visibility() {
    let mut e = scene("missing.wav", 30.into());
    e.execute(Command::ToggleVisible(1)).unwrap();
    near(
        cached(&e, false)
            .render(1.0, 0, 1, 6.0, &AtomicBool::new(false))
            .unwrap()[0][0],
        0.25,
    );
    e.execute(Command::Precompose {
        layers: vec![1],
        name: "Nested".into(),
    })
    .unwrap();
    let id = e.selected().unwrap();
    e.execute(Command::SetTimeRemap { id, enabled: true })
        .unwrap();
    let last = *e
        .selected_layer()
        .unwrap()
        .time_remap()
        .unwrap()
        .keys()
        .keys()
        .next_back()
        .unwrap();
    e.execute(Command::EditTimeRemap {
        id,
        edit: TrackEdit::Value {
            frame: 0,
            value: 2.0,
        },
    })
    .unwrap();
    e.execute(Command::EditTimeRemap {
        id,
        edit: TrackEdit::Value {
            frame: last,
            value: 0.0,
        },
    })
    .unwrap();
    let b = cached(&e, false)
        .render(0.0, 0, 48000, 6.0, &AtomicBool::new(false))
        .unwrap();
    near(b[0][0], 0.375);
    near(b[24000][0], (3.0 - 30.0 / last as f32) / 8.0);
    e.execute(Command::SetLayerSwitch {
        id,
        switch: LayerSwitch::Guide,
        enabled: true,
    })
    .unwrap();
    assert!(!cached(&e, false).has_audio());
    assert!(cached(&e, true).has_audio());
    e.execute(Command::AddRectangle).unwrap();
    e.execute(Command::SetLayerSwitch {
        id: e.selected().unwrap(),
        switch: LayerSwitch::Solo,
        enabled: true,
    })
    .unwrap();
    assert!(!cached(&e, true).has_audio());
}

#[test]
fn mixing_sums_channels_preserves_phase_and_reports_master_clipping() {
    let mut e = scene("missing.wav", 30.into());
    for _ in 0..4 {
        e.execute(Command::DuplicateLayer(e.selected().unwrap()))
            .unwrap();
    }
    let mut m = cached(&e, false);
    assert_eq!(m.cache.sources.len(), 1);
    let output = m.render(1.0, 0, 1, 6.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(output[0], [1.0, -1.0]);
    assert_eq!(m.levels.peak, [1.25; 2]);
    assert_eq!(m.levels.clipped_frames, 1);
    assert!(
        m.render(1.0, 0, 100, 6.0, &AtomicBool::new(true))
            .unwrap_err()
            .contains("canceled")
    );
}

#[test]
fn animated_audio_controls_and_nested_pan_apply_before_master_metering() {
    use libre_effects_core::AudioParam;
    let mut e = scene("missing.wav", 30.into());
    let level = -6.020599913279624;
    e.execute(Command::EditAudio {
        id: 1,
        parameter: AudioParam::RightLevel,
        edit: TrackEdit::Value {
            frame: 0,
            value: level,
        },
    })
    .unwrap();
    e.execute(Command::FadeAudio {
        id: 1,
        start: 0,
        end: 30,
        fade_in: true,
    })
    .unwrap();
    let mut mixer = cached(&e, false);
    let v = mixer
        .render(0.5, 0, 1, 6.0, &AtomicBool::new(false))
        .unwrap()[0];
    near(v[0], 0.09375);
    near(v[1], -0.046875);
    assert_eq!(mixer.levels.frames, 1);
    near(mixer.levels.sum_squares[0].sqrt() as f32, v[0].abs());
    let source = e.project().active_composition_id();
    e.execute(Command::Precompose {
        layers: vec![1],
        name: "Bus".into(),
    })
    .unwrap();
    let id = e.selected().unwrap();
    e.execute(Command::EditAudio {
        id,
        parameter: AudioParam::Pan,
        edit: TrackEdit::Value {
            frame: 0,
            value: 100.0,
        },
    })
    .unwrap();
    let v = cached(&e, false)
        .render(0.5, 0, 1, 6.0, &AtomicBool::new(false))
        .unwrap()[0];
    assert_eq!(v[0], 0.0);
    near(v[1], 0.046875);
    // Outer bus levels apply before its pan, after all inner bus processing.
    e.execute(Command::EditAudio {
        id,
        parameter: AudioParam::RightLevel,
        edit: TrackEdit::Value {
            frame: 0,
            value: level,
        },
    })
    .unwrap();
    let v = cached(&e, false)
        .render(0.5, 0, 1, 6.0, &AtomicBool::new(false))
        .unwrap()[0];
    near(v[1], 0.0703125);
    e.execute(Command::SetAudioEnabled { id, enabled: false })
        .unwrap();
    assert!(!cached(&e, false).has_audio());
    e.undo();
    assert!(cached(&e, false).has_audio());
    assert_eq!(e.project().active_composition_id(), source);
    let restored = libre_effects_core::Project::from_json(&e.project().to_json().unwrap()).unwrap();
    assert_eq!(&restored, e.project());
}

#[test]
#[ignore = "requires FFmpeg; checks automated channel levels, pan, fade and nested bus output"]
fn automated_mix_pcm_matches_independent_sample_math_and_silent_visual_pixels() {
    use libre_effects_core::AudioParam;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("voice.wav");
    wav(&source);
    let mut e = scene(source.to_str().unwrap(), 30.into());
    let before = e.project().clone();
    e.execute(Command::EditAudio {
        id: 1,
        parameter: AudioParam::LeftLevel,
        edit: TrackEdit::Value {
            frame: 0,
            value: -6.020599913279624,
        },
    })
    .unwrap();
    e.execute(Command::FadeAudio {
        id: 1,
        start: 0,
        end: 30,
        fade_in: true,
    })
    .unwrap();
    e.execute(Command::Precompose {
        layers: vec![1],
        name: "Audio bus".into(),
    })
    .unwrap();
    let id = e.selected().unwrap();
    e.execute(Command::EditAudio {
        id,
        parameter: AudioParam::Pan,
        edit: TrackEdit::Value {
            frame: 0,
            value: -50.0,
        },
    })
    .unwrap();
    e.execute(Command::EditAudio {
        id,
        parameter: AudioParam::RightLevel,
        edit: TrackEdit::Value {
            frame: 0,
            value: -6.020599913279624,
        },
    })
    .unwrap();
    let project = libre_effects_core::Project::from_json(&e.project().to_json().unwrap()).unwrap();
    let out = dir.path().join("mix.mov");
    crate::video_export::export_video_with_settings(
        &project,
        0..45,
        crate::video_export::VideoPreset::ProResAlpha,
        &Default::default(),
        &out,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let data = decoded(&out);
    let source_samples = decoded(&source);
    assert_eq!(data.len(), 72000);
    let q = std::f64::consts::FRAC_1_SQRT_2;
    for (i, actual) in data.iter().enumerate() {
        let fade = (i as f64 / 48000.0).min(1.0);
        let l = f64::from(source_samples[i][0]) * 0.5 * fade;
        let r = f64::from(source_samples[i][1]) * fade * 0.5;
        let expected = [l + r * q, r * q];
        for c in 0..2 {
            assert!(
                (f64::from(actual[c]) - expected[c]).abs() < 3e-7,
                "sample {i} channel {c}"
            );
        }
    }
    let mut renderer = crate::rendering::Renderer::new();
    assert_eq!(
        renderer.render(&project, 15, 64).unwrap(),
        renderer.render(&before, 15, 64).unwrap()
    );
}

fn wav(path: &Path) {
    let frames = 3 * SAMPLE_RATE;
    let length = frames * 8;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(length + 36).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&3u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
    f.write_all(&(SAMPLE_RATE * 8).to_le_bytes()).unwrap();
    f.write_all(&8u16.to_le_bytes()).unwrap();
    f.write_all(&32u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&length.to_le_bytes()).unwrap();
    for i in 0..frames {
        let t = f64::from(i) / f64::from(SAMPLE_RATE);
        let x = if (0.75..1.0).contains(&t) {
            0.0
        } else {
            (t * 220.0 * std::f64::consts::TAU).sin() as f32 * 0.25
        };
        f.write_all(&x.to_le_bytes()).unwrap();
        f.write_all(&(-x * 0.5).to_le_bytes()).unwrap();
    }
}
fn decoded(path: &Path) -> Vec<[f32; 2]> {
    let mut cmd = crate::footage::command(&crate::video_export::ffmpeg_path());
    cmd.args(["-v", "error", "-i"]).arg(path).args([
        "-map",
        "0:a:0",
        "-f",
        "f32le",
        "-c:a",
        "pcm_f32le",
        "-ar",
        "48000",
        "-ac",
        "2",
        "pipe:1",
    ]);
    let data = crate::footage::output(cmd, 4 * 1024 * 1024).unwrap();
    data.chunks_exact(8)
        .map(|b| {
            [
                f32::from_le_bytes(b[..4].try_into().unwrap()),
                f32::from_le_bytes(b[4..].try_into().unwrap()),
            ]
        })
        .collect()
}

#[test]
#[ignore = "requires FFmpeg; validates sample-aligned AAC/PCM output and cancellation"]
fn encoded_audio_preserves_samples_range_clock_channels_and_explicit_off() {
    use crate::{
        output_settings::{AudioOutput, Settings},
        video_export::{VideoPreset, export_video_with_settings},
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.wav");
    wav(&source);
    let e = scene(source.to_str().unwrap(), "30000/1001".parse().unwrap());
    for (index, preset, fps) in [
        (0, VideoPreset::H264, None),
        (1, VideoPreset::ProResAlpha, None),
        (2, VideoPreset::ProResAlpha, Some(24.into())),
    ] {
        let output = dir
            .path()
            .join(format!("out-{index}.{}", preset.extension()));
        let settings = Settings {
            fps,
            ..Default::default()
        };
        let plan = settings
            .plan(
                e.project().composition(),
                7..38,
                if preset == VideoPreset::H264 {
                    crate::output_settings::Format::Mp4
                } else {
                    crate::output_settings::Format::MovAlpha
                },
            )
            .unwrap();
        export_video_with_settings(
            e.project(),
            7..38,
            preset,
            &settings,
            &output,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let actual = decoded(&output);
        let count = sample_count(plan.frames, plan.fps) as usize;
        assert!(
            actual.len() >= count && actual.len() <= count + 1024,
            "{} / {count}",
            actual.len()
        );
        let origin = e.project().composition().fps().seconds(7);
        let mut mixer = Mixer::new(e.project(), false).unwrap();
        let mut expected = Vec::new();
        for start in (0..count).step_by(BLOCK) {
            expected.extend(
                mixer
                    .render(
                        origin,
                        start as u64,
                        BLOCK.min(count - start),
                        e.project().composition().fps().seconds(38),
                        &AtomicBool::new(false),
                    )
                    .unwrap(),
            );
        }
        let mse: f64 = expected
            .iter()
            .zip(&actual)
            .flat_map(|(a, b)| (0..2).map(move |c| f64::from(a[c] - b[c]).powi(2)))
            .sum::<f64>()
            / (count * 2) as f64;
        assert!(
            mse < if preset == VideoPreset::ProResAlpha {
                1e-12
            } else {
                0.00003
            },
            "{preset:?} MSE {mse}"
        );
        let metadata = crate::footage::command(&crate::footage::probe_path())
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name,sample_rate,channels,duration_ts,time_base,start_time",
                "-of",
                "json",
            ])
            .arg(&output)
            .output()
            .unwrap();
        let data: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
        let audio = data["streams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["sample_rate"].is_string())
            .unwrap();
        assert_eq!(audio["sample_rate"], "48000");
        assert_eq!(audio["channels"], 2);
        assert_eq!(audio["time_base"], "1/48000");
        assert_eq!(
            audio["codec_name"],
            if preset == VideoPreset::H264 {
                "aac"
            } else {
                "pcm_s24le"
            }
        );
        assert!(
            (audio["start_time"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap())
            .abs()
                < 1e-6
        );
        assert!(
            (audio["duration_ts"].as_u64().unwrap() as i64 - count as i64).abs() <= 1,
            "{audio}"
        );
    }
    let output = dir.path().join("off.mp4");
    let settings = Settings {
        audio: AudioOutput::Off,
        ..Default::default()
    };
    export_video_with_settings(
        e.project(),
        7..38,
        VideoPreset::H264,
        &settings,
        &output,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let p = crate::footage::command(&crate::footage::probe_path())
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&p.stdout).unwrap()["streams"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let original = std::fs::read(&output).unwrap();
    std::fs::rename(&source, dir.path().join("offline.wav")).unwrap();
    assert!(
        export_video_with_settings(
            e.project(),
            7..38,
            VideoPreset::H264,
            &Settings::default(),
            &output,
            Default::default(),
            Default::default()
        )
        .unwrap_err()
        .contains("Audio offline")
    );
    assert_eq!(std::fs::read(&output).unwrap(), original);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 5);
}

#[test]
#[ignore = "requires FFmpeg; checks chunk seeks against continuous decode and video offsets"]
fn compressed_seek_chunks_and_delayed_video_match_continuous_samples() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.wav");
    wav(&source);
    for (ext, codec) in [
        ("wav", "pcm_f32le"),
        ("flac", "flac"),
        ("mp3", "libmp3lame"),
        ("m4a", "aac"),
    ] {
        let path = dir.path().join(format!("converted.{ext}"));
        let mut cmd = crate::footage::command(&crate::video_export::ffmpeg_path());
        cmd.args(["-v", "error", "-i"])
            .arg(&source)
            .args(["-ar", "44100", "-c:a", codec])
            .arg(&path);
        assert!(cmd.status().unwrap().success());
        let audio = crate::audio::probe(&path).unwrap().audio.unwrap();
        let full = decoded(&path);
        for second in [0, 1, 2] {
            let chunk = decode_second(
                path.to_str().unwrap(),
                &audio,
                second,
                &AtomicBool::new(false),
            )
            .unwrap();
            let reference = &full[second as usize * 48000..];
            let n = chunk.len().min(reference.len()).min(48000);
            let mse = chunk[..n]
                .iter()
                .zip(reference)
                .flat_map(|(a, b)| (0..2).map(move |c| f64::from(a[c] - b[c]).powi(2)))
                .sum::<f64>()
                / (n * 2) as f64;
            assert!(mse < 0.000002, "{ext} second {second} MSE {mse}");
        }
    }
    let movie = dir.path().join("delayed.mov");
    assert!(
        crate::footage::command(&crate::video_export::ffmpeg_path())
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x48:r=30:d=3.5",
                "-itsoffset",
                "0.5",
                "-i"
            ])
            .arg(&source)
            .args([
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-c:v",
                "libx264",
                "-c:a",
                "pcm_f32le",
                "-t",
                "3.5"
            ])
            .arg(&movie)
            .status()
            .unwrap()
            .success()
    );
    let info = crate::footage::probe(&movie).unwrap();
    let mut e = Editor::default();
    e.execute(Command::AddContent {
        content: Content::Video {
            path: info.path,
            audio: info.audio,
            duration: info.duration,
            source_fps: info.source_fps,
            start_frame: 0,
            playback: Default::default(),
        },
        width: 64.0,
        height: 48.0,
        name: "Delayed".into(),
    })
    .unwrap();
    let mut mixer = Mixer::new(e.project(), false).unwrap();
    let first = mixer
        .render(0.0, 0, 48000, 4.0, &AtomicBool::new(false))
        .unwrap();
    assert!(first[..24000].iter().all(|x| *x == [0.0; 2]));
    let reference = decoded(&source);
    for i in 24000..48000 {
        for c in 0..2 {
            near(first[i][c], reference[i - 24000][c]);
        }
    }
}

#[test]
#[ignore = "requires FFmpeg; verifies cancellation during audio preparation"]
fn cancel_audio_mix_preserves_destination_and_removes_temporary_mix() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.wav");
    wav(&source);
    let mut e = scene(source.to_str().unwrap(), 30.into());
    e.execute(Command::ConfigureComposition {
        name: "Cancel".into(),
        width: 64,
        height: 48,
        fps: 30,
        duration: 10000,
    })
    .unwrap();
    let output = dir.path().join("keep.mp4");
    std::fs::write(&output, b"original").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(80));
        signal.store(true, Ordering::Relaxed);
    });
    let result = crate::video_export::export_video(
        e.project(),
        0..10000,
        crate::video_export::VideoPreset::H264,
        &output,
        cancel,
        Default::default(),
    );
    timer.join().unwrap();
    assert!(result.unwrap_err().contains("canceled"));
    assert_eq!(std::fs::read(&output).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn pcm_cache_bounds_chunks_keys_and_bytes_without_caching_invalid_data() {
    let mut cache = PcmCache::default();
    let mut audio = AudioMetadata {
        stream_index: 0,
        sample_rate: SAMPLE_RATE,
        channels: 2,
        channel_layout: "stereo".into(),
        duration: 3.0,
        start_time: 0.0,
        file_offset: 0.0,
    };
    let source = cache.register("synthetic.wav", &audio).unwrap();
    for second in 0..140 {
        cache
            .insert_chunk(
                Chunk {
                    source,
                    second,
                    pcm: vec![[2.0; 2]; 16],
                },
                &AtomicBool::new(false),
            )
            .unwrap();
    }
    assert_eq!(cache.chunks.len(), CACHE_CHUNKS);
    assert_eq!(cache.chunks.front().unwrap().second, 12);
    assert!(cache.memory_bytes() <= CACHE_BYTES);
    assert!(
        cache
            .insert_chunk(
                Chunk {
                    source,
                    second: 500,
                    pcm: vec![[f32::INFINITY; 2]]
                },
                &AtomicBool::new(false)
            )
            .is_err()
    );
    assert!(
        cache
            .insert_chunk(
                Chunk {
                    source,
                    second: 500,
                    pcm: vec![[0.0; 2]; 48_001]
                },
                &AtomicBool::new(false)
            )
            .is_err()
    );
    assert_eq!(cache.chunks.back().unwrap().second, 139);
    assert!(cache.make_room(usize::MAX).is_err());
    assert!(cache.chunks.is_empty());
    assert!(
        cache
            .register(&"x".repeat(MAX_SOURCE_PATH_BYTES + 1), &audio)
            .is_err()
    );
    for index in 1..CACHE_SOURCES {
        audio.duration = index as f64 / 100.0;
        cache.register("synthetic.wav", &audio).unwrap();
    }
    // One generated duration matches the first source's metadata, so add one
    // more distinct key before checking the exact bound.
    audio.duration = 100.0;
    cache.register("synthetic.wav", &audio).unwrap();
    assert_eq!(cache.sources.len(), CACHE_SOURCES);
    audio.duration = 101.0;
    assert!(
        cache
            .register("synthetic.wav", &audio)
            .unwrap_err()
            .contains("4096")
    );
}

#[test]
fn shared_cache_pins_file_across_distinct_metadata_keys() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"first").unwrap();
    let path = file.path().to_str().unwrap();
    let e = scene(path, 30.into());
    let (_, audio) = e.project().composition().layers()[0]
        .content()
        .audio()
        .unwrap();
    let mut cache = PcmCache::default();
    let first = cache.register(path, audio).unwrap();
    cache
        .validate_source(first, &AtomicBool::new(false))
        .unwrap();
    let mut alternate = audio.clone();
    alternate.stream_index = 1;
    let second = cache.register(path, &alternate).unwrap();
    std::fs::write(file.path(), b"changed file").unwrap();
    assert!(
        cache
            .validate_source(second, &AtomicBool::new(false))
            .unwrap_err()
            .contains("source changed")
    );
}

#[test]
fn shared_audio_clock_rejects_nonfinite_positions() {
    let e = scene("synthetic.wav", 30.into());
    let layer = &e.project().composition().layers()[0];
    assert!(voice_position(std::iter::once((layer, 30.into(), 180)), f64::NAN).is_err());
    assert!(voice_position(std::iter::once((layer, 30.into(), 180)), f64::INFINITY).is_err());
}
