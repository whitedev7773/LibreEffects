use super::*;
use libre_effects_core::{Command, Content, Editor, Property};
fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Queue QA".into(),
        width: 64,
        height: 48,
        fps: 2,
        duration: 3,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Rectangle,
        width: 64.0,
        height: 48.0,
        name: "Red".into(),
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
        value: 50.0,
    })
    .unwrap();
    e.execute(Command::SetCompositionBackground(0x0000ff))
        .unwrap();
    e
}
#[test]
fn snapshot_outputs_history_and_restart_preserve_pixels_and_job_configuration() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("queue");
    let mut q = Queue::load(root.clone()).unwrap();
    let mut e = scene();
    q.enqueue(
        e.project(),
        None,
        0..3,
        &[Format::PngAlpha.into(), Format::PngBackground.into()],
        d.path(),
    )
    .unwrap();
    let snapshot = q.data.jobs[0].snapshot.clone();
    e.execute(Command::SetColor {
        id: 1,
        color: 0x00ff00,
    })
    .unwrap();
    q.edit(|d| {
        d.presets.push(Preset {
            name: "Delivery".into(),
            specs: vec![Format::PngAlpha.into(), Format::PngBackground.into()],
        });
        d.jobs[0].range = 1..3;
        Ok(())
    })
    .unwrap();
    q.history(false).unwrap();
    assert_eq!(q.data.jobs[0].range, 0..3);
    assert!(q.data.presets.is_empty());
    q.history(true).unwrap();
    assert_eq!(q.data.jobs[0].range, 1..3);
    drop(q);
    let mut q = Queue::load(root.clone()).unwrap();
    assert_eq!(q.data.presets[0].name, "Delivery");
    let destinations: Vec<_> = q.data.jobs[0]
        .outputs
        .iter()
        .map(|o| o.path.clone())
        .collect();
    q.begin().unwrap();
    let q = Arc::new(Mutex::new(q));
    run(q.clone());
    let q = q.lock().unwrap();
    assert!(!q.running);
    assert!(
        q.data.jobs[0]
            .outputs
            .iter()
            .all(|o| o.status == Status::Completed)
    );
    for (index, path) in destinations.iter().enumerate() {
        assert!(!path.join("frame-000000.png").exists());
        let pixel = image::open(path.join("frame-000001.png"))
            .unwrap()
            .to_rgba8()
            .get_pixel(32, 24)
            .0;
        assert_eq!(
            pixel,
            if index == 0 {
                [255, 0, 0, 128]
            } else {
                [128, 0, 127, 255]
            }
        );
        assert_eq!(std::fs::read_dir(path).unwrap().count(), 3);
    }
    let mut loaded = Queue::load(root.clone()).unwrap();
    assert_eq!(loaded.data.jobs[0].outputs[0].status, Status::Completed);
    loaded
        .edit(|d| {
            d.jobs.clear();
            Ok(())
        })
        .unwrap();
    loaded.history(false).unwrap();
    assert!(root.join(&snapshot).is_file());
    loaded
        .edit(|d| {
            d.jobs.clear();
            Ok(())
        })
        .unwrap();
    drop(loaded);
    Queue::load(root.clone()).unwrap();
    assert!(!root.join(snapshot).exists());
}
#[test]
fn failure_policy_cancel_interruption_and_retry_are_durable() {
    for stop in [true, false] {
        let d = tempfile::tempdir().unwrap();
        let mut q = Queue::load(d.path().join("queue")).unwrap();
        q.enqueue(
            scene().project(),
            None,
            0..3,
            &[Format::PngAlpha.into(), Format::PngBackground.into()],
            d.path(),
        )
        .unwrap();
        let occupied = q.data.jobs[0].outputs[0].path.clone();
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("keep"), b"keep").unwrap();
        q.edit(|d| {
            d.stop_on_error = stop;
            Ok(())
        })
        .unwrap();
        q.begin().unwrap();
        let shared = Arc::new(Mutex::new(q));
        run(shared.clone());
        let mut q = shared.lock().unwrap();
        assert_eq!(q.data.jobs[0].outputs[0].status, Status::Failed);
        assert_eq!(
            q.data.jobs[0].outputs[1].status,
            if stop {
                Status::Queued
            } else {
                Status::Completed
            }
        );
        assert_eq!(std::fs::read(occupied.join("keep")).unwrap(), b"keep");
        q.edit(|d| {
            d.jobs[0].outputs[0] =
                Output::new(Format::PngAlpha, occupied.with_file_name("retry.frames"));
            Ok(())
        })
        .unwrap();
        q.begin().unwrap();
        q.cancel.store(true, Ordering::Relaxed);
        drop(q);
        run(shared.clone());
        let mut q = shared.lock().unwrap();
        assert_eq!(q.data.jobs[0].outputs[0].status, Status::Queued);
        q.begin().unwrap();
        drop(q);
        run(shared.clone());
        let mut q = shared.lock().unwrap();
        assert_eq!(q.data.jobs[0].outputs[0].status, Status::Completed);
        q.data.jobs[0].outputs[0].status = Status::Rendering;
        q.save(&q.data).unwrap();
        let restored = Queue::load(q.root.clone()).unwrap();
        assert_eq!(restored.data.jobs[0].outputs[0].status, Status::Interrupted);
        assert!(!restored.running);
    }
}
#[test]
fn invalid_edits_aliases_source_paths_and_storage_fail_atomically() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("queue");
    let mut q = Queue::load(root.clone()).unwrap();
    let original = d.path().join("source.lfe.json");
    std::fs::write(&original, b"untouched").unwrap();
    q.enqueue(
        scene().project(),
        Some(original.clone()),
        0..3,
        &[Format::Mp4.into(), Format::MovAlpha.into()],
        d.path(),
    )
    .unwrap();
    let before = q.data.clone();
    assert!(
        q.edit(|d| {
            d.jobs[0].range = 2..1;
            Ok(())
        })
        .is_err()
    );
    assert!(
        q.edit(|d| {
            d.jobs[0].outputs[1] = d.jobs[0].outputs[0].clone();
            Ok(())
        })
        .is_err()
    );
    assert!(
        q.edit(|d| {
            d.jobs[0].outputs[0] = Output::new(Format::PngAlpha, original.clone());
            Ok(())
        })
        .is_err()
    );
    let alias = d.path().join("alias.mp4");
    std::fs::hard_link(&original, &alias).unwrap();
    assert!(
        q.edit(|d| {
            d.jobs[0].outputs[0].path = alias;
            Ok(())
        })
        .is_err()
    );
    assert!(
        q.edit(|d| {
            d.jobs[0].outputs[0].path = root.join("render.mp4");
            Ok(())
        })
        .is_err()
    );
    assert!(
        q.edit(|d| {
            d.jobs[0].snapshot = "../source.lfe.json".into();
            Ok(())
        })
        .is_err()
    );
    assert_eq!(q.data, before);
    std::fs::remove_file(root.join("queue.json")).unwrap();
    std::fs::create_dir(root.join("queue.json")).unwrap();
    assert!(
        q.edit(|d| {
            d.stop_on_error = false;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(q.data, before);
    assert_eq!(std::fs::read(original).unwrap(), b"untouched");
}
#[test]
fn canceled_sequence_never_publishes_a_partial_destination() {
    let d = tempfile::tempdir().unwrap();
    let mut e = scene();
    e.execute(Command::ConfigureComposition {
        name: "Cancel".into(),
        width: 128,
        height: 96,
        fps: 30,
        duration: 10000,
    })
    .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(AtomicU32::new(0));
    let (c, p) = (cancel.clone(), progress.clone());
    let stop = std::thread::spawn(move || {
        let now = std::time::Instant::now();
        while p.load(Ordering::Relaxed) < 2 && now.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        c.store(true, Ordering::Relaxed);
    });
    let path = d.path().join("frames");
    assert!(
        execute(
            e.project(),
            None,
            0..10000,
            &Output::new(Format::PngAlpha, path.clone()),
            cancel,
            progress
        )
        .unwrap_err()
        .contains("canceled")
    );
    stop.join().unwrap();
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
}
#[test]
#[ignore = "requires FFmpeg; checks queued snapshot PNG/MP4/MOV equivalence and clocks"]
fn queue_multiple_compositions_and_modules_render_exact_snapshots() {
    let d = tempfile::tempdir().unwrap();
    let mut q = Queue::load(d.path().join("queue")).unwrap();
    let mut e = scene();
    q.enqueue(
        e.project(),
        None,
        0..3,
        &[
            Format::Mp4.into(),
            Format::MovAlpha.into(),
            Format::PngAlpha.into(),
        ],
        d.path(),
    )
    .unwrap();
    e.execute(Command::DuplicateComposition).unwrap();
    let id = e.project().composition().layers()[0].id();
    e.execute(Command::SetColor {
        id,
        color: 0x00ff00,
    })
    .unwrap();
    q.enqueue(
        e.project(),
        None,
        1..3,
        &[Format::Mp4.into(), Format::MovAlpha.into()],
        d.path(),
    )
    .unwrap();
    q.edit(|d| {
        d.jobs.swap(0, 1);
        Ok(())
    })
    .unwrap();
    q.begin().unwrap();
    let shared = Arc::new(Mutex::new(q));
    run(shared.clone());
    let q = shared.lock().unwrap();
    for job in &q.data.jobs {
        for output in &job.outputs {
            assert_eq!(output.status, Status::Completed, "{}", output.message);
            if output.spec.format.sequence() {
                continue;
            }
            let mut cmd = std::process::Command::new(crate::video_export::ffmpeg_path());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            let decoded = cmd
                .args(["-v", "error", "-i"])
                .arg(&output.path)
                .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                .output()
                .unwrap();
            assert!(decoded.status.success());
            assert_eq!(decoded.stdout.len(), 64 * 48 * 4 * job.range.len());
            let p = &decoded.stdout[(24 * 64 + 32) * 4..][..4];
            let color = if job.id == 1 { 0 } else { 1 };
            let expected = if output.spec.format == Format::Mp4 {
                128
            } else {
                255
            };
            assert!((i32::from(p[color]) - expected).abs() < 8, "{p:?}");
            assert!(
                (i32::from(p[3])
                    - if output.spec.format == Format::Mp4 {
                        255
                    } else {
                        128
                    })
                .abs()
                    < 3
            );
        }
    }
}

#[test]
fn v3_queues_default_missing_font_policy_and_preserve_explicit_strict() {
    use crate::output_settings::FontPolicy;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("queue");
    let mut queue = Queue::load(root.clone()).unwrap();
    queue
        .enqueue(
            scene().project(),
            None,
            0..1,
            &[Format::PngAlpha.into(), Format::PngBackground.into()],
            directory.path(),
        )
        .unwrap();
    queue
        .edit(|data| {
            data.presets.push(Preset {
                name: "Font policies".into(),
                specs: vec![Format::PngAlpha.into(), Format::PngBackground.into()],
            });
            Ok(())
        })
        .unwrap();
    let mut legacy = serde_json::to_value(&queue.data).unwrap();
    legacy["version"] = 3.into();
    legacy["jobs"][0]["outputs"][0]["settings"]
        .as_object_mut()
        .unwrap()
        .remove("fonts");
    legacy["jobs"][0]["outputs"][1]["settings"]["fonts"] = "Strict".into();
    legacy["presets"][0]["specs"][0]["settings"]
        .as_object_mut()
        .unwrap()
        .remove("fonts");
    legacy["presets"][0]["specs"][1]["settings"]["fonts"] = "Strict".into();
    let path = root.join("queue.json");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let loaded = Queue::load(root.clone()).unwrap();
    assert_eq!(loaded.data.version, 4);
    for (index, expected) in [FontPolicy::Fallback, FontPolicy::Strict]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            loaded.data.jobs[0].outputs[index].spec.settings.fonts,
            expected
        );
        assert_eq!(loaded.data.presets[0].specs[index].settings.fonts, expected);
    }
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    // v3 readers reject this envelope before saving or executing any job.
    assert_eq!(saved["version"], 4);
    assert_eq!(
        saved["jobs"][0]["outputs"][0]["settings"]["fonts"],
        "Fallback"
    );
    assert_eq!(
        saved["jobs"][0]["outputs"][1]["settings"]["fonts"],
        "Strict"
    );
    assert_eq!(
        saved["presets"][0]["specs"][0]["settings"]["fonts"],
        "Fallback"
    );
    assert_eq!(
        saved["presets"][0]["specs"][1]["settings"]["fonts"],
        "Strict"
    );
    assert_eq!(Queue::load(root).unwrap().data, loaded.data);
}

#[test]
fn v2_queue_migration_preserves_existing_font_policy_while_retaining_silent_audio() {
    use crate::output_settings::{AudioOutput, FontPolicy};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("queue");
    let mut queue = Queue::load(root.clone()).unwrap();
    queue
        .enqueue(
            scene().project(),
            None,
            0..1,
            &[Format::PngAlpha.into()],
            directory.path(),
        )
        .unwrap();
    queue
        .edit(|data| {
            data.presets.push(Preset {
                name: "Strict legacy".into(),
                specs: vec![Format::PngAlpha.into()],
            });
            Ok(())
        })
        .unwrap();
    let mut legacy = serde_json::to_value(&queue.data).unwrap();
    legacy["version"] = 2.into();
    legacy["jobs"][0]["outputs"][0]["settings"]["fonts"] = "Strict".into();
    legacy["presets"][0]["specs"][0]["settings"]["fonts"] = "Strict".into();
    std::fs::write(
        root.join("queue.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    let loaded = Queue::load(root).unwrap();
    assert_eq!(loaded.data.version, 4);
    let output = &loaded.data.jobs[0].outputs[0].spec.settings;
    let preset = &loaded.data.presets[0].specs[0].settings;
    for settings in [output, preset] {
        assert_eq!(settings.fonts, FontPolicy::Strict);
        assert_eq!(settings.audio, AudioOutput::Off);
    }
}

#[test]
fn unsupported_queue_versions_and_font_policies_preserve_all_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("queue");
    let mut queue = Queue::load(root.clone()).unwrap();
    queue
        .enqueue(
            scene().project(),
            None,
            0..1,
            &[Format::PngAlpha.into()],
            directory.path(),
        )
        .unwrap();
    let original = serde_json::to_value(&queue.data).unwrap();
    let snapshot = root.join(&queue.data.jobs[0].snapshot);
    let snapshot_bytes = std::fs::read(&snapshot).unwrap();
    let orphan = root.join("snapshot-999-999.lfe.json");
    std::fs::write(&orphan, b"retain on rejected load").unwrap();
    let mut cases = Vec::new();
    for version in [0, 5, u32::MAX] {
        let mut value = original.clone();
        value["version"] = version.into();
        cases.push(value);
    }
    for version in [3, 4] {
        let mut value = original.clone();
        value["version"] = version.into();
        value["jobs"][0]["outputs"][0]["settings"]["fonts"] = "FuturePolicy".into();
        cases.push(value);
    }
    for value in cases {
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(root.join("queue.json"), &bytes).unwrap();
        assert!(Queue::load(root.clone()).is_err());
        assert_eq!(std::fs::read(root.join("queue.json")).unwrap(), bytes);
        assert_eq!(std::fs::read(&snapshot).unwrap(), snapshot_bytes);
        assert_eq!(std::fs::read(&orphan).unwrap(), b"retain on rejected load");
    }
}

#[test]
fn v1_font_policy_extensions_survive_the_entire_migration_chain() {
    use crate::output_settings::{AudioOutput, FontPolicy};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("queue");
    let mut queue = Queue::load(root.clone()).unwrap();
    queue
        .enqueue(
            scene().project(),
            None,
            0..1,
            &[Format::PngAlpha.into(), Format::PngBackground.into()],
            directory.path(),
        )
        .unwrap();
    queue
        .edit(|data| {
            data.presets.push(Preset {
                name: "Legacy policies".into(),
                specs: vec![Format::PngAlpha.into(), Format::PngBackground.into()],
            });
            Ok(())
        })
        .unwrap();
    let mut legacy = serde_json::to_value(&queue.data).unwrap();
    legacy["version"] = 1.into();
    legacy["jobs"][0]["outputs"][0]["settings"] = serde_json::json!({"fonts":"Strict"});
    legacy["jobs"][0]["outputs"][1]
        .as_object_mut()
        .unwrap()
        .remove("settings");
    legacy["presets"][0]["formats"] = serde_json::json!(["PngAlpha", "PngBackground"]);
    legacy["presets"][0]["specs"][0]["settings"] = serde_json::json!({"fonts":"Strict"});
    legacy["presets"][0]["specs"][1]
        .as_object_mut()
        .unwrap()
        .remove("settings");
    std::fs::write(
        root.join("queue.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    let loaded = Queue::load(root).unwrap();
    assert_eq!(loaded.data.version, 4);
    for (index, expected) in [FontPolicy::Strict, FontPolicy::Fallback]
        .into_iter()
        .enumerate()
    {
        for settings in [
            &loaded.data.jobs[0].outputs[index].spec.settings,
            &loaded.data.presets[0].specs[index].settings,
        ] {
            assert_eq!(settings.fonts, expected);
            assert_eq!(settings.audio, AudioOutput::Off);
        }
    }
}
