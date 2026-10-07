use super::*;
use libre_effects_core::{Command, Content, Editor};
#[test]
fn metadata_distinguishes_cover_art_and_timed_video_and_validates_bounds() {
    let mut data = serde_json::json!({"format":{"start_time":"2", "duration":"10"}, "streams":[{"codec_type":"video", "start_time":"3", "disposition":{"attached_pic":1}}, {"index":2,"codec_type":"audio","start_time":"2.25", "duration":"9.5", "sample_rate":"48000","channels":2,"channel_layout":"stereo"}]});
    let probe = parse_probe(&data).unwrap();
    assert!(!probe.has_video);
    assert_eq!(
        (
            probe.audio.as_ref().unwrap().start_time,
            probe.audio.unwrap().file_offset
        ),
        (0.0, 0.25)
    );
    data["streams"][0]["disposition"]["attached_pic"] = 0.into();
    let probe = parse_probe(&data).unwrap();
    assert!(probe.has_video);
    assert_eq!(probe.audio.unwrap().start_time, -0.75);
    data["streams"][1]["channels"] = 0.into();
    assert!(parse_probe(&data).is_err());
}
#[test]
fn envelopes_preserve_opposite_phase_channels_and_silence_without_downmix() {
    let samples: Vec<u8> = (0..800)
        .flat_map(|i| if i < 80 { [0.5f32, -0.5] } else { [0.0, 0.0] })
        .flat_map(f32::to_le_bytes)
        .collect();
    let peaks = envelope(&samples, 2, 8000).unwrap();
    assert_eq!(peaks.len(), 10);
    assert_eq!((peaks[0].min, peaks[0].max), (-0.5, 0.5));
    assert!(peaks[1..].iter().all(|p| p.min == 0.0 && p.max == 0.0));
    assert!(envelope(&[0], 2, 8000).is_err());
    assert!(envelope(&f32::NAN.to_le_bytes(), 1, 8000).is_err());
}
fn wav(path: &Path, amplitude: i16) {
    let rate = 48000u32;
    let frames = rate * 12;
    let bytes = frames * 4;
    let mut data = Vec::new();
    data.extend(b"RIFF");
    data.extend((36 + bytes).to_le_bytes());
    data.extend(b"WAVEfmt ");
    data.extend(16u32.to_le_bytes());
    data.extend(1u16.to_le_bytes());
    data.extend(2u16.to_le_bytes());
    data.extend(rate.to_le_bytes());
    data.extend((rate * 4).to_le_bytes());
    data.extend(4u16.to_le_bytes());
    data.extend(16u16.to_le_bytes());
    data.extend(b"data");
    data.extend(bytes.to_le_bytes());
    for i in 0..frames {
        let sample = if i / rate % 2 == 0 { amplitude } else { 0 };
        data.extend(sample.to_le_bytes());
        data.extend((-sample).to_le_bytes());
    }
    std::fs::write(path, data).unwrap();
}
#[test]
#[ignore = "requires FFmpeg and FFprobe; validates real audio imports, timestamps, waveforms, portability and relinking"]
fn standalone_and_embedded_audio_roundtrip_waveforms_and_media_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let sound = dir.path().join("opposite stereo.wav");
    wav(&sound, 16384);
    let probe = probe(&sound).unwrap();
    assert!(!probe.has_video);
    let audio = probe.audio.unwrap();
    assert_eq!(
        (
            audio.sample_rate,
            audio.channels,
            audio.duration,
            audio.start_time
        ),
        (48000, 2, 12.0, 0.0)
    );
    for chunk in [0, 1] {
        let bins = decode_chunk(sound.to_str().unwrap(), &audio, chunk).unwrap();
        assert!((bins[20].max - 0.5).abs() < 0.001 && (bins[20].min + 0.5).abs() < 0.001);
        assert_eq!(bins[120].max, 0.0);
    }
    let mut e = Editor::default();
    e.execute(Command::Batch(
        crate::editor::assets::read_assets(&[sound.clone()], None).unwrap(),
    ))
    .unwrap();
    e.execute(Command::AddAssetLayer { asset: 1, frame: 0 })
        .unwrap();
    let project_path = dir.path().join("audio.lep");
    crate::media_io::save(e.project(), &Default::default(), &project_path).unwrap();
    let bytes = std::fs::read(&project_path).unwrap();
    let stored = libre_effects_core::project_file::decode(&bytes)
        .unwrap()
        .project;
    assert_eq!(
        crate::media_io::video_paths(&stored),
        std::collections::BTreeSet::from(["opposite stereo.wav".into()])
    );
    let loaded = crate::project_io::read_project(&project_path).unwrap();
    assert_eq!(&loaded, e.project());
    assert!(
        crate::rendering::Renderer::new()
            .render(&loaded, 0, 64)
            .unwrap()
            .pixels()
            .all(|p| p[3] == 0)
    );
    assert!(crate::project_io::validate_render(&loaded, &sound, &(0..10)).is_err());
    let collected =
        crate::media_io::collect(&loaded, &Default::default(), dir.path(), |_, _| Ok(())).unwrap();
    assert_eq!(collected.files, 1);
    let copy = crate::project_io::read_project(&collected.project_path).unwrap();
    let (collected_path, collected_audio) =
        copy.composition().layers()[0].content().audio().unwrap();
    assert!(
        (decode_chunk(collected_path, collected_audio, 0).unwrap()[20].max - 0.5).abs() < 0.001
    );
    let replacement = dir.path().join("new.wav");
    wav(&replacement, 8192);
    let change = crate::media_io::replacement_for_project(
        &loaded,
        loaded.composition().layers()[0].content().linked_paths()[0].clone(),
        &replacement,
    )
    .unwrap();
    e.execute(Command::RelinkMedia(vec![change])).unwrap();
    let (path, audio) = e.selected_layer().unwrap().content().audio().unwrap();
    assert!((decode_chunk(path, audio, 0).unwrap()[20].max - 0.25).abs() < 0.001);
    let movie = dir.path().join("delayed.mov");
    assert!(
        crate::footage::command(&crate::video_export::ffmpeg_path())
            .args(["-v", "error", "-itsoffset", "0.5", "-i"])
            .arg(&sound)
            .args([
                "-f",
                "lavfi",
                "-i",
                "color=blue:s=64x48:r=30:d=12.5",
                "-map",
                "1:v:0",
                "-map",
                "0:a:0",
                "-c:v",
                "libx264",
                "-c:a",
                "pcm_s16le"
            ])
            .arg(&movie)
            .status()
            .unwrap()
            .success()
    );
    let info = crate::footage::probe(&movie).unwrap();
    let audio = info.audio.unwrap();
    assert!((audio.start_time - 0.5).abs() < 0.001);
    assert!((audio.file_offset - 0.5).abs() < 0.001);
    assert!((decode_chunk(&info.path, &audio, 1).unwrap()[20].max - 0.5).abs() < 0.001);
    let commands = crate::editor::assets::read_assets(&[movie], None).unwrap();
    e.execute(Command::Batch(commands)).unwrap();
    assert!(
        e.project()
            .asset_library()
            .assets()
            .values()
            .any(|a| matches!(a.content(), Content::Video { audio: Some(_), .. }))
    );
}

#[test]
#[ignore = "requires FFmpeg/FFprobe with FLAC, MP3 and AAC encoders"]
fn compressed_audio_formats_import_and_decode_waveforms() {
    let dir = tempfile::tempdir().unwrap();
    for (extension, codec) in [("flac", "flac"), ("mp3", "libmp3lame"), ("m4a", "aac")] {
        let path = dir.path().join(format!("source.{extension}"));
        assert!(
            crate::footage::command(&crate::video_export::ffmpeg_path())
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=44100:duration=1.2",
                    "-ac",
                    "2",
                    "-c:a",
                    codec
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let mut editor = Editor::default();
        editor
            .execute(Command::Batch(
                crate::editor::assets::read_assets(&[path], None).unwrap(),
            ))
            .unwrap();
        let source = editor
            .project()
            .asset_library()
            .assets()
            .values()
            .next()
            .unwrap();
        let (path, audio) = source.content().audio().unwrap();
        assert_eq!(
            (audio.sample_rate, audio.channels, audio.start_time),
            (44100, 2, 0.0)
        );
        assert!((1.19..1.3).contains(&audio.duration));
        let peaks = decode_chunk(path, audio, 0).unwrap();
        assert!(peaks[20].max > 0.05 && peaks[20].min < -0.05, "{extension}");
    }
}
