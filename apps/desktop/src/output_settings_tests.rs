use super::*;
use libre_effects_core::{Command, Content, Editor, Property};

fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Output settings".into(),
        width: 64,
        height: 48,
        fps: 2,
        duration: 4,
    })
    .unwrap();
    e.execute(Command::SetCompositionBackground(0x0000ff))
        .unwrap();
    e.execute(Command::AddContent {
        content: Content::Rectangle,
        width: 32.0,
        height: 24.0,
        name: "Animated opacity".into(),
    })
    .unwrap();
    e.execute(Command::SetColor {
        id: 1,
        color: 0xff0000,
    })
    .unwrap();
    e.execute(Command::SetValue {
        id: 1,
        property: Property::Opacity,
        frame: 0,
        value: 25.0,
    })
    .unwrap();
    e.execute(Command::ToggleKeyframe {
        id: 1,
        property: Property::Opacity,
        frame: 0,
    })
    .unwrap();
    e.execute(Command::SetValue {
        id: 1,
        property: Property::Opacity,
        frame: 3,
        value: 100.0,
    })
    .unwrap();
    e
}
#[test]
fn output_clock_preserves_elapsed_time_and_never_samples_outside_range() {
    let e = scene();
    let s = Settings {
        fps: Some(4.into()),
        ..Default::default()
    };
    let p = s
        .plan(e.project().composition(), 1..4, Format::MovAlpha)
        .unwrap();
    assert_eq!(p.frames, 6);
    assert_eq!(
        (0..p.frames).map(|i| p.source_frame(i)).collect::<Vec<_>>(),
        vec![1, 1, 2, 2, 3, 3]
    );
    assert_eq!(p.sequence_first(), 0);
    for source in ["24000/1001", "30000/1001", "60"] {
        let mut e = scene();
        let source: FrameRate = source.parse().unwrap();
        e.execute(Command::ConfigureCompositionRate {
            name: "Clock".into(),
            width: 64,
            height: 48,
            fps: source,
            duration: 10001,
            display_start: 0,
        })
        .unwrap();
        for output in ["24000/1001", "30000/1001", "24", "60"] {
            let s = Settings {
                fps: Some(output.parse().unwrap()),
                ..Default::default()
            };
            let p = s
                .plan(e.project().composition(), 37..10000, Format::MovAlpha)
                .unwrap();
            let duration = source.seconds(9963);
            let encoded = p.fps.seconds(u64::from(p.frames));
            assert!(
                encoded + 1e-10 >= duration && encoded - duration < 1.0 / p.fps.as_f64() + 1e-10
            );
            assert_eq!(p.source_frame(0), 37);
            assert!(p.source_frame(p.frames - 1) < 10000);
        }
    }
}
#[test]
fn channels_size_and_codec_combinations_validate_before_output() {
    for (field, value, format) in [
        (Field::Size, "0x10", Format::PngAlpha),
        (Field::Size, "16384x16384", Format::MovAlpha),
        (Field::Size, "63x48", Format::Mp4),
        (Field::Fps, "241", Format::Mp4),
        (Field::Channels, "rgba", Format::Mp4),
        (Field::Quality, "crf:52", Format::Mp4),
        (Field::Quality, "kbps:0", Format::Mp4),
        (Field::Quality, "crf:18", Format::MovAlpha),
        (Field::Speed, "slow", Format::PngAlpha),
        (Field::Speed, "unknown", Format::Mp4),
    ] {
        let mut s = Settings::default();
        assert!(
            s.change(field, value)
                .and_then(|_| s.validate(format))
                .is_err(),
            "{field:?} {value}"
        );
    }
    let e = scene();
    let renderer = crate::rendering::Renderer::new();
    let mut pixels = renderer.render_output(e.project(), 1, 32, 24).unwrap();
    assert_eq!(pixels.dimensions(), (32, 24));
    assert_eq!(pixels.get_pixel(16, 12).0, [255, 0, 0, 128]);
    assert_eq!(pixels.get_pixel(0, 0)[3], 0);
    let alpha = Settings {
        channels: Channels::Alpha,
        ..Default::default()
    };
    alpha.apply_channels(&mut pixels, Format::MovAlpha, 0x0000ff);
    assert_eq!(pixels.get_pixel(16, 12).0, [128, 128, 128, 255]);
    assert_eq!(pixels.get_pixel(0, 0).0, [0, 0, 0, 255]);
    let mut pixels = renderer.render_output(e.project(), 1, 128, 96).unwrap();
    Settings {
        channels: Channels::Rgb,
        ..Default::default()
    }
    .apply_channels(&mut pixels, Format::MovAlpha, 0x0000ff);
    assert_eq!(pixels.get_pixel(64, 48).0, [128, 0, 127, 255]);
    assert_eq!(pixels.get_pixel(0, 0).0, [0, 0, 255, 255]);
    for (channels, color) in [
        (Channels::Rgb, image::ColorType::Rgb8),
        (Channels::Rgba, image::ColorType::Rgba8),
        (Channels::Alpha, image::ColorType::L8),
    ] {
        let settings = Settings {
            channels,
            ..Default::default()
        };
        let bytes = settings
            .png_bytes(pixels.clone(), Format::PngAlpha)
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().color(), color);
    }
}
#[test]
fn configured_queue_presets_history_restart_and_png_clock_roundtrip() {
    use crate::render_queue::{Preset, Queue, Status};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("queue");
    let e = scene();
    let mut q = Queue::load(root.clone()).unwrap();
    let spec = Spec {
        format: Format::PngAlpha,
        settings: Settings {
            size: Some([32, 24]),
            fps: Some(4.into()),
            channels: Channels::Alpha,
            ..Default::default()
        },
    };
    q.enqueue(e.project(), None, 1..4, &[spec.clone()], dir.path())
        .unwrap();
    q.edit(|d| {
        d.presets.push(Preset {
            name: "Custom alpha".into(),
            specs: vec![spec.clone()],
        });
        d.jobs[0].outputs[0].spec.settings.size = Some([128, 96]);
        Ok(())
    })
    .unwrap();
    q.history(false).unwrap();
    assert_eq!(q.data.jobs[0].outputs[0].spec, spec);
    q.history(true).unwrap();
    drop(q);
    let mut q = Queue::load(root).unwrap();
    assert_eq!(q.data.presets[0].specs[0], spec);
    q.enqueue(
        e.project(),
        None,
        1..4,
        &q.data.presets[0].specs.clone(),
        dir.path(),
    )
    .unwrap();
    assert_eq!(q.data.jobs[1].outputs[0].spec, spec);
    q.begin().unwrap();
    let q = std::sync::Arc::new(std::sync::Mutex::new(q));
    crate::render_queue::run(q.clone());
    let q = q.lock().unwrap();
    for (j, width, height) in [(&q.data.jobs[0], 128, 96), (&q.data.jobs[1], 32, 24)] {
        let out = &j.outputs[0];
        assert_eq!(out.status, Status::Completed, "{}", out.message);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.path.join("sequence.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["fps"], 4);
        assert_eq!(manifest["rendered_frames"], 6);
        assert_eq!(manifest["first_frame"], 0);
        assert_eq!(manifest["alpha"], false);
        for (index, a) in [128, 128, 191, 191, 255, 255].into_iter().enumerate() {
            let image = image::open(out.path.join(format!("frame-{index:06}.png")))
                .unwrap()
                .to_rgba8();
            assert_eq!(image.dimensions(), (width, height));
            assert_eq!(image.get_pixel(width / 2, height / 2).0, [a, a, a, 255]);
        }
    }
}
#[test]
fn queue_v1_migrates_defaults_without_losing_jobs_presets_or_results() {
    use crate::render_queue::{Queue, Status};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("queue");
    let mut q = Queue::load(root.clone()).unwrap();
    q.enqueue(
        scene().project(),
        None,
        0..3,
        &[Format::Mp4.into()],
        dir.path(),
    )
    .unwrap();
    let mut data = serde_json::to_value(&q.data).unwrap();
    data["version"] = 1.into();
    data["jobs"][0]["outputs"][0]
        .as_object_mut()
        .unwrap()
        .remove("settings");
    data["jobs"][0]["outputs"][0]["status"] = "Completed".into();
    data["presets"] = serde_json::json!([{"name":"Legacy","formats":["Mp4","MovAlpha"]}]);
    std::fs::write(root.join("queue.json"), serde_json::to_vec(&data).unwrap()).unwrap();
    drop(q);
    let q = Queue::load(root.clone()).unwrap();
    assert_eq!(q.data.jobs[0].outputs[0].status, Status::Completed);
    let legacy = |format| Spec {
        format,
        settings: Settings {
            audio: AudioOutput::Off,
            ..Default::default()
        },
    };
    assert_eq!(q.data.jobs[0].outputs[0].spec, legacy(Format::Mp4));
    assert_eq!(
        q.data.presets[0].specs,
        vec![legacy(Format::Mp4), legacy(Format::MovAlpha)]
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("queue.json")).unwrap()).unwrap();
    assert_eq!(saved["version"], 4);
}

fn command(exe: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut c = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}

#[test]
fn audio_queue_settings_undo_restart_and_v2_silent_migration() {
    use crate::render_queue::{Preset, Queue};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("queue");
    let mut q = Queue::load(root.clone()).unwrap();
    q.enqueue(
        scene().project(),
        None,
        0..3,
        &[Format::Mp4.into()],
        dir.path(),
    )
    .unwrap();
    q.edit(|data| {
        data.jobs[0].outputs[0]
            .spec
            .settings
            .change(Field::Audio, "off")?;
        data.presets.push(Preset {
            name: "Silent".into(),
            specs: vec![data.jobs[0].outputs[0].spec.clone()],
        });
        Ok(())
    })
    .unwrap();
    q.history(false).unwrap();
    assert_eq!(
        q.data.jobs[0].outputs[0].spec.settings.audio,
        AudioOutput::Auto
    );
    q.history(true).unwrap();
    drop(q);
    let q = Queue::load(root.clone()).unwrap();
    assert_eq!(
        q.data.jobs[0].outputs[0].spec.settings.audio,
        AudioOutput::Off
    );
    assert_eq!(q.data.presets[0].specs[0].settings.audio, AudioOutput::Off);
    let mut data = serde_json::to_value(&q.data).unwrap();
    data["version"] = 2.into();
    data["jobs"][0]["outputs"][0]["settings"]
        .as_object_mut()
        .unwrap()
        .remove("audio");
    data["presets"][0]["specs"][0]["settings"]
        .as_object_mut()
        .unwrap()
        .remove("audio");
    drop(q);
    std::fs::write(root.join("queue.json"), serde_json::to_vec(&data).unwrap()).unwrap();
    let q = Queue::load(root).unwrap();
    assert_eq!(
        q.data.jobs[0].outputs[0].spec.settings.audio,
        AudioOutput::Off
    );
    assert_eq!(q.data.presets[0].specs[0].settings.audio, AudioOutput::Off);
    assert!(Settings::default().change(Field::Audio, "garbage").is_err());
}
#[test]
fn malformed_legacy_queue_is_rejected_without_overwriting_it() {
    use crate::render_queue::Queue;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("queue.json");
    for value in [
        serde_json::json!({"version":1,"jobs":[{"outputs":[false]}],"presets":[]}),
        serde_json::json!({"version":2,"jobs":[{"outputs":[{"settings":false}]}],"presets":[]}),
        serde_json::json!({"version":2,"jobs":[],"presets":[{"specs":[{"settings":"bad"}]}]}),
        serde_json::json!({"version":2,"jobs":[false],"presets":[]}),
        serde_json::json!({"version":2,"jobs":[],"presets":[false]}),
    ] {
        let original = serde_json::to_vec(&value).unwrap();
        std::fs::write(&path, &original).unwrap();
        assert!(Queue::load(dir.path().to_path_buf()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
#[test]
#[ignore = "requires FFmpeg; validates output size, rational clock, rate control and channel pixels"]
fn configured_video_streams_match_size_rate_codec_channels_and_quality() {
    let dir = tempfile::tempdir().unwrap();
    let e = scene();
    for (index, format, channels, quality, fps) in [
        (
            0,
            Format::Mp4,
            Channels::Rgb,
            Some(RateControl::Crf(30)),
            "4",
        ),
        (
            1,
            Format::Mp4,
            Channels::Alpha,
            Some(RateControl::Bitrate(700)),
            "30000/1001",
        ),
        (2, Format::MovAlpha, Channels::Rgba, None, "4"),
        (3, Format::MovAlpha, Channels::Rgb, None, "4"),
        (4, Format::MovAlpha, Channels::Alpha, None, "4"),
    ] {
        let settings = Settings {
            size: Some([32, 24]),
            fps: Some(fps.parse().unwrap()),
            channels,
            rate_control: quality,
            encoder_speed: quality.map(|_| "fast".into()),
            audio: Default::default(),
            fonts: Default::default(),
        };
        let path = dir
            .path()
            .join(format!("out-{index}.{}", format.extension()));
        let plan = settings
            .plan(e.project().composition(), 1..4, format)
            .unwrap();
        let mut output = crate::render_queue::Output::new(format, path.clone());
        output.spec.settings = settings;
        crate::render_queue::execute(
            e.project(),
            None,
            1..4,
            &output,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let probe = command(crate::footage::probe_path())
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height,r_frame_rate,nb_frames,duration,codec_name,pix_fmt",
                "-of",
                "json",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(probe.status.success());
        let metadata: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let stream = &metadata["streams"][0];
        let pix_fmt = stream["pix_fmt"].as_str().unwrap();
        assert_eq!(
            pix_fmt.starts_with("yuva"),
            format == Format::MovAlpha && channels == Channels::Rgba
        );
        assert_eq!(stream["width"], 32);
        assert_eq!(stream["height"], 24);
        assert_eq!(stream["nb_frames"], plan.frames.to_string());
        assert_eq!(stream["r_frame_rate"], if fps == "4" { "4/1" } else { fps });
        assert_eq!(
            stream["codec_name"],
            if format == Format::Mp4 {
                "h264"
            } else {
                "prores"
            }
        );
        let duration: f64 = stream["duration"].as_str().unwrap().parse().unwrap();
        assert!((duration - plan.fps.seconds(u64::from(plan.frames))).abs() < 0.00001);
        if let Some(quality) = quality {
            let bytes = std::fs::read(&path).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            match quality {
                RateControl::Crf(_) => assert!(text.contains("crf=30.0")),
                RateControl::Bitrate(_) => assert!(text.contains("bitrate=700")),
            };
        }
        let decoded = command(crate::video_export::ffmpeg_path())
            .args(["-v", "error", "-i"])
            .arg(&path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        let stride = 32 * 24 * 4;
        assert_eq!(decoded.stdout.len(), stride * plan.frames as usize);
        for i in 0..plan.frames as usize {
            let a = match plan.source_frame(i as u32) {
                1 => 128,
                2 => 191,
                _ => 255,
            };
            let expected = match channels {
                Channels::Alpha => [a, a, a, 255],
                Channels::Rgb => [a, 0, 255 - a, 255],
                _ => [255, 0, 0, a],
            };
            let offset = i * stride + (12 * 32 + 16) * 4;
            let pixel = &decoded.stdout[offset..offset + 4];
            assert!(
                pixel
                    .iter()
                    .zip(expected)
                    .all(|(p, e)| (i32::from(*p) - e).abs() < 12),
                "{index} frame {i}: {pixel:?} vs {expected:?}"
            );
        }
    }
}
