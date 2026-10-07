use super::*;
use libre_effects_core::{Command, Editor, LayerSwitch, TextStyle};
use std::sync::Arc;

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Preflight QA".into(),
            width: 64,
            height: 48,
            fps: 24,
            duration: 48,
        })
        .unwrap();
    editor
}
fn font_scene(style: TextStyle) -> Editor {
    let mut editor = scene();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Output text".into(),
                font_size: 16.0,
            },
            width: 60.0,
            height: 30.0,
            name: "Title".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetTextStyle { id: 1, style })
        .unwrap();
    editor
}
fn png_check(editor: &Editor, path: &Path, settings: &Settings) -> Result<Prepared, Report> {
    check(
        editor.project(),
        None,
        0..1,
        Format::PngAlpha,
        settings,
        path,
        Destination::File,
        Path::new("unused-ffmpeg"),
        &AtomicBool::new(false),
    )
}
fn assert_preserved(path: &Path, directory: &Path) {
    assert_eq!(std::fs::read(path).unwrap(), b"keep existing output");
    assert_eq!(
        std::fs::read_dir(directory).unwrap().count(),
        1,
        "preflight must clean all temporary files"
    );
}
fn error_code(report: &Report, code: Code) -> bool {
    report
        .diagnostics
        .iter()
        .any(|d| d.code == code && d.severity == Severity::Error)
}

#[test]
fn readiness_checks_preserve_existing_output_and_leave_no_probe_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.png");
    std::fs::write(&path, b"keep existing output").unwrap();
    let editor = scene();
    let prepared = png_check(&editor, &path, &Settings::default()).unwrap();
    assert!(prepared.report.diagnostics.is_empty());
    assert_eq!(
        (
            prepared.plan.width,
            prepared.plan.height,
            prepared.plan.frames
        ),
        (64, 48, 1)
    );
    assert_preserved(&path, directory.path());
    let report = png_check(
        &editor,
        &path,
        &Settings {
            size: Some([0, 48]),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(error_code(&report, Code::Settings));
    assert_preserved(&path, directory.path());
    let report = check(
        editor.project(),
        None,
        48..49,
        Format::PngAlpha,
        &Settings::default(),
        &path,
        Destination::File,
        Path::new("unused"),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error_code(&report, Code::Settings));
    assert_preserved(&path, directory.path());
    let report = check(
        editor.project(),
        Some(&path),
        0..1,
        Format::PngAlpha,
        &Settings::default(),
        &path,
        Destination::File,
        Path::new("unused"),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error_code(&report, Code::Source));
    assert_preserved(&path, directory.path());
    let report = check(
        editor.project(),
        None,
        0..1,
        Format::PngAlpha,
        &Settings::default(),
        &path,
        Destination::File,
        Path::new("unused"),
        &AtomicBool::new(true),
    )
    .unwrap_err();
    assert!(error_code(&report, Code::Canceled));
    assert_preserved(&path, directory.path());
}

#[test]
fn missing_and_substituted_fonts_warn_or_fail_under_an_explicit_policy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.png");
    std::fs::write(&path, b"keep existing output").unwrap();
    for style in [
        TextStyle {
            font_family: "Missing Preflight QA font".into(),
            ..Default::default()
        },
        TextStyle {
            font_face: "Missing Wanted Sans face".into(),
            ..Default::default()
        },
        TextStyle {
            italic: true,
            ..Default::default()
        },
    ] {
        let editor = font_scene(style);
        let prepared = png_check(&editor, &path, &Settings::default()).unwrap();
        assert_eq!(prepared.report.diagnostics.len(), 1);
        assert_eq!(prepared.report.diagnostics[0].severity, Severity::Warning);
        let text = prepared.report.to_string();
        assert!(text.contains("Preflight QA / Title"));
        assert!(text.contains("using Wanted Sans / WantedSans-Regular"));
        assert!(text.contains("Fonts policy: fallback"));
        assert_preserved(&path, directory.path());
        let error = png_check(
            &editor,
            &path,
            &Settings {
                fonts: FontPolicy::Strict,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error_code(&error, Code::FontSubstitution));
        assert!(error.to_string().contains("Fonts policy: strict"));
        assert_preserved(&path, directory.path());
    }
}

#[test]
fn font_diagnostics_follow_visible_nested_ranges() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("output.png");
    let mut editor = font_scene(TextStyle {
        font_family: "Missing Preflight QA font".into(),
        ..Default::default()
    });
    editor
        .execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Guide,
            enabled: true,
        })
        .unwrap();
    let strict = Settings {
        fonts: FontPolicy::Strict,
        ..Default::default()
    };
    assert!(png_check(&editor, &path, &strict).is_ok());
    editor
        .execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Guide,
            enabled: false,
        })
        .unwrap();
    editor.execute(Command::NewComposition).unwrap();
    // An inactive composition must not block an unrelated output.
    assert!(png_check(&editor, &path, &strict).is_ok());
    editor
        .execute(Command::AddCompositionLayer {
            composition: 1,
            frame: 10,
        })
        .unwrap();
    assert!(png_check(&editor, &path, &strict).is_ok());
    let error = check(
        editor.project(),
        None,
        10..11,
        Format::PngAlpha,
        &strict,
        &path,
        Destination::File,
        Path::new("unused"),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error_code(&error, Code::FontSubstitution));
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn output_parent_and_existing_sequence_errors_are_actionable_and_non_destructive() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.png");
    std::fs::write(&path, b"keep existing output").unwrap();
    let editor = scene();
    for output in [
        directory.path().join("missing/output.png"),
        path.join("output.png"),
        directory.path().to_path_buf(),
    ] {
        let error = png_check(&editor, &output, &Settings::default()).unwrap_err();
        assert!(error_code(&error, Code::Destination));
        assert!(error.to_string().contains("writable output folder"));
        assert_preserved(&path, directory.path());
    }
    let error = check(
        editor.project(),
        None,
        0..1,
        Format::PngAlpha,
        &Settings::default(),
        &path,
        Destination::NewSequence,
        Path::new("unused"),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error.to_string().contains("new folder"));
    assert_preserved(&path, directory.path());
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    let original = permissions.clone();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();
    let result = png_check(&editor, &path, &Settings::default());
    std::fs::set_permissions(&path, original).unwrap();
    assert!(error_code(&result.unwrap_err(), Code::Destination));
    assert_preserved(&path, directory.path());
}

#[test]
#[cfg(unix)]
fn readonly_output_parent_does_not_touch_an_existing_destination() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.png");
    std::fs::write(&path, b"keep existing output").unwrap();
    let permissions = std::fs::metadata(directory.path()).unwrap().permissions();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = png_check(&scene(), &path, &Settings::default());
    std::fs::set_permissions(directory.path(), permissions).unwrap();
    assert!(error_code(&result.unwrap_err(), Code::Destination));
    assert_preserved(&path, directory.path());
}

#[test]
fn missing_encoder_is_reported_before_any_destination_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.mp4");
    std::fs::write(&path, b"keep existing output").unwrap();
    let editor = scene();
    let report = check(
        editor.project(),
        None,
        0..1,
        Format::Mp4,
        &Settings::default(),
        &path,
        Destination::File,
        &directory.path().join("missing-ffmpeg"),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error_code(&report, Code::EncoderUnavailable));
    assert!(report.to_string().contains("LIBRE_EFFECTS_FFMPEG"));
    assert_preserved(&path, directory.path());
}

#[cfg(unix)]
fn fake_encoder(directory: &Path, script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let executable = directory.join("ffmpeg");
    std::fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    executable
}

#[test]
#[cfg(unix)]
fn missing_codec_and_false_success_leave_existing_output_intact() {
    let directory = tempfile::tempdir().unwrap();
    let executables = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.mp4");
    std::fs::write(&path, b"keep existing output").unwrap();
    for script in ["echo 'Unknown encoder libx264' >&2; exit 1", "exit 0"] {
        let executable = fake_encoder(executables.path(), script);
        let report = check(
            scene().project(),
            None,
            0..1,
            Format::Mp4,
            &Settings::default(),
            &path,
            Destination::File,
            &executable,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error_code(&report, Code::EncoderCapability));
        assert!(report.to_string().contains("libx264 / MP4"));
        assert_preserved(&path, directory.path());
    }
}

#[test]
#[cfg(unix)]
fn canceling_an_encoder_probe_reaps_it_and_preserves_destination() {
    let directory = tempfile::tempdir().unwrap();
    let executables = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.mp4");
    std::fs::write(&path, b"keep existing output").unwrap();
    let executable = fake_encoder(executables.path(), "while :; do :; done");
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        worker_cancel.store(true, Ordering::Relaxed);
    });
    let start = Instant::now();
    let report = check(
        scene().project(),
        None,
        0..1,
        Format::Mp4,
        &Settings::default(),
        &path,
        Destination::File,
        &executable,
        &cancel,
    )
    .unwrap_err();
    stopper.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(error_code(&report, Code::Canceled));
    assert_preserved(&path, directory.path());
}

#[test]
#[ignore = "requires FFmpeg; checks actual selected codecs, containers and audio capabilities"]
fn real_encoder_preflight_checks_both_codecs_without_changing_destination() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.video");
    std::fs::write(&path, b"keep existing output").unwrap();
    for format in [Format::Mp4, Format::MovAlpha] {
        check(
            scene().project(),
            None,
            0..1,
            format,
            &Settings::default(),
            &path,
            Destination::File,
            &crate::video_export::ffmpeg_path(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_preserved(&path, directory.path());
        let editor = scene();
        let settings = Settings::default();
        let plan = settings
            .plan(editor.project().composition(), 0..1, format)
            .unwrap();
        check_encoder(
            editor.project(),
            &plan,
            format,
            &settings,
            &path,
            &crate::video_export::ffmpeg_path(),
            true,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_preserved(&path, directory.path());
    }
    let editor = font_scene(TextStyle {
        font_family: "Missing Preflight QA font".into(),
        ..Default::default()
    });
    let report = crate::video_export::export_video_with_settings(
        editor.project(),
        0..1,
        crate::video_export::VideoPreset::H264,
        &Settings::default(),
        &path,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].severity, Severity::Warning);
    assert_eq!(&std::fs::read(&path).unwrap()[4..8], b"ftyp");
    std::fs::write(&path, b"keep existing output").unwrap();
    let progress = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let error = crate::video_export::export_video_with_settings(
        editor.project(),
        0..1,
        crate::video_export::VideoPreset::H264,
        &Settings {
            fonts: FontPolicy::Strict,
            ..Default::default()
        },
        &path,
        Default::default(),
        progress.clone(),
    )
    .unwrap_err();
    assert!(error.contains("Fonts policy: strict"));
    assert_eq!(progress.load(Ordering::Relaxed), 0);
    assert_preserved(&path, directory.path());
}

#[test]
fn hidden_text_used_as_a_matte_still_has_font_preflight() {
    use libre_effects_core::{MatteMode, TrackMatte};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("output.png");
    let mut editor = font_scene(TextStyle {
        font_family: "Missing Preflight QA font".into(),
        ..Default::default()
    });
    editor.execute(Command::ToggleVisible(1)).unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetTrackMatte {
            id: 2,
            matte: Some(TrackMatte {
                source: 1,
                mode: MatteMode::Alpha,
            }),
        })
        .unwrap();
    let report = png_check(&editor, &path, &Settings::default())
        .unwrap()
        .report;
    assert_eq!(report.diagnostics.len(), 1);
    assert!(report.to_string().contains("Preflight QA / Title"));
    assert!(!path.exists());
}

#[test]
fn queue_persists_fallback_warnings_and_strict_errors_without_publishing_partial_outputs() {
    use crate::output_settings::Spec;
    use crate::render_queue::{Queue, Status};
    use std::sync::Mutex;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("queue");
    let editor = font_scene(TextStyle {
        font_family: "Missing Preflight QA font".into(),
        ..Default::default()
    });
    let mut queue = Queue::load(root.clone()).unwrap();
    let specs = [
        Format::PngAlpha.into(),
        Spec {
            format: Format::PngAlpha,
            settings: Settings {
                fonts: FontPolicy::Strict,
                ..Default::default()
            },
        },
    ];
    queue
        .enqueue(editor.project(), None, 0..1, &specs, directory.path())
        .unwrap();
    queue.begin().unwrap();
    let queue = Arc::new(Mutex::new(queue));
    crate::render_queue::run(queue.clone());
    let queue = queue.lock().unwrap();
    let outputs = &queue.data.jobs[0].outputs;
    assert_eq!(outputs[0].status, Status::Completed);
    assert!(outputs[0].message.contains("Warning [font-substitution]"));
    assert!(outputs[0].path.join("frame-000000.png").is_file());
    assert_eq!(outputs[1].status, Status::Failed);
    assert!(outputs[1].message.contains("Fonts policy: strict"));
    assert!(!outputs[1].path.exists());
    let saved = queue.data.clone();
    drop(queue);
    assert_eq!(Queue::load(root).unwrap().data, saved);
}

#[test]
fn font_policy_settings_are_backward_compatible_and_explicitly_editable() {
    let original = Settings::default();
    let mut legacy = serde_json::to_value(&original).unwrap();
    legacy.as_object_mut().unwrap().remove("fonts");
    let mut restored: Settings = serde_json::from_value(legacy).unwrap();
    assert_eq!(restored.fonts, FontPolicy::Fallback);
    restored
        .change(crate::output_settings::Field::Fonts, "strict")
        .unwrap();
    assert_eq!(restored.fonts, FontPolicy::Strict);
    assert!(restored.summary(Format::PngAlpha).contains("Fonts: strict"));
    assert_eq!(
        serde_json::from_str::<Settings>(&serde_json::to_string(&restored).unwrap()).unwrap(),
        restored
    );
    assert!(
        restored
            .change(crate::output_settings::Field::Fonts, "silent")
            .is_err()
    );
    assert_eq!(restored.fonts, FontPolicy::Strict);
}

#[test]
fn offline_footage_and_hardlinked_sources_remain_protected_by_shared_preflight() {
    let directory = tempfile::tempdir().unwrap();
    let source_directory = tempfile::tempdir().unwrap();
    let source = source_directory.path().join("source.mp4");
    let output = directory.path().join("existing.png");
    std::fs::write(&source, b"keep existing output").unwrap();
    std::fs::hard_link(&source, &output).unwrap();
    let mut editor = scene();
    editor
        .execute(Command::AddContent {
            content: Content::Video {
                path: source.to_string_lossy().into(),
                duration: 1.0,
                source_fps: 24.0,
                start_frame: 0,
                playback: Default::default(),
                audio: None,
            },
            width: 64.0,
            height: 48.0,
            name: "Video".into(),
        })
        .unwrap();
    assert!(error_code(
        &png_check(&editor, &output, &Settings::default()).unwrap_err(),
        Code::Source
    ));
    assert_preserved(&output, directory.path());
    std::fs::remove_file(&source).unwrap();
    let error = png_check(&editor, &output, &Settings::default()).unwrap_err();
    assert!(error_code(&error, Code::Source));
    assert!(error.to_string().contains("offline"));
    assert_preserved(&output, directory.path());
}

#[test]
fn cancellation_interrupts_source_range_preflight_without_writing_output() {
    let directory = tempfile::tempdir().unwrap();
    let sources = tempfile::tempdir().unwrap();
    let output = directory.path().join("existing.png");
    std::fs::write(&output, b"keep existing output").unwrap();
    for index in [1, 2] {
        image::RgbaImage::new(16, 16)
            .save(sources.path().join(format!("shot_{index:04}.png")))
            .unwrap();
    }
    let mut editor = Editor::default();
    editor
        .execute(
            crate::image_sequence::discover(&sources.path().join("shot_0001.png"), 24.into(), None)
                .unwrap(),
        )
        .unwrap();
    editor.execute(Command::CompositionFromAsset(1)).unwrap();
    let cancel = AtomicBool::new(false);
    let result = crate::project_io::validate_render_with(
        editor.project(),
        &output,
        &(0..2),
        &cancel,
        &mut |_, _| cancel.store(true, Ordering::Relaxed),
    );
    assert!(
        result
            .unwrap_err()
            .to_ascii_lowercase()
            .contains("canceled")
    );
    assert_preserved(&output, directory.path());
}
