use super::*;
use crate::footage::{command, output};
use crate::video_export::ffmpeg_path;
use std::path::Path;

fn generate(path: &Path, rate: &str, codec: &str, size: &str, frames: u32) {
    let filter = format!("testsrc2=size={size}:rate={rate},format=rgba,colorchannelmixer=aa=0.5");
    let mut c = command(&ffmpeg_path());
    c.args([
        "-v",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        &filter,
        "-frames:v",
        &frames.to_string(),
        "-c:v",
        codec,
        "-threads",
        "2",
    ]);
    if codec == "libx264" {
        c.args(["-pix_fmt", "yuv420p", "-g", "48", "-bf", "3"]);
    }
    assert!(c.arg(path).status().unwrap().success());
}
fn reference(path: &Path, size: &str) -> Vec<u8> {
    let mut c = command(&ffmpeg_path());
    c.args(["-v", "error", "-i"]).arg(path).args([
        "-map",
        "0:v:0",
        "-an",
        "-vf",
        &format!("scale={size}:flags=bicubic+accurate_rnd+full_chroma_int,setsar=1"),
        "-fps_mode",
        "passthrough",
        "-pix_fmt",
        "rgba",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    output(c, 128 * 1024 * 1024).unwrap()
}
fn pixels(png: &str) -> image::RgbaImage {
    image::load_from_memory(&STANDARD.decode(png).unwrap())
        .unwrap()
        .into_rgba8()
}

#[test]
fn invalid_requests_and_cancellation_do_not_launch_decoders() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("empty");
    std::fs::write(&file, []).unwrap();
    let path = file.to_str().unwrap();
    let cancel = AtomicBool::new(false);
    let mut pool = Pool::default();
    for (fps, w, h, t) in [
        (f64::NAN, 64, 48, 0.),
        (30., 0, 48, 0.),
        (30., 64, 5000, 0.),
        (30., 64, 48, f64::INFINITY),
        (30., 64, 48, -1.),
    ] {
        assert!(pool.frame_png(path, t, fps, w, h, 64, &cancel).is_err());
    }
    assert_eq!(pool.metrics.starts, 0);
    cancel.store(true, Ordering::Release);
    assert!(
        pool.frame_png(path, 0., 30., 64, 48, 64, &cancel)
            .unwrap_err()
            .contains("canceled")
    );
    assert_eq!(pool.metrics.starts, 0);
}

#[test]
fn hardware_decoder_requires_hardware_frame_output_and_bounds_diagnostics() {
    assert_eq!(Backend::Cuda.arguments(), Some(("cuda", "cuda")));
    assert_eq!(Backend::D3d11.arguments(), Some(("d3d11va", "d3d11")));
    assert_eq!(Backend::Qsv.arguments(), Some(("qsv", "qsv")));
    assert_eq!(Backend::Cpu.arguments(), None);
    let mut diagnostics = Diagnostics::default();
    diagnostics.append(&vec![b'a'; 64 * 1024]);
    diagnostics.append(b"last decoder error");
    assert_eq!(diagnostics.0.len(), 16 * 1024);
    assert!(diagnostics.message().ends_with("last decoder error"));
}

#[test]
#[ignore = "requires NVIDIA NVDEC and FFmpeg CUDA; exact fractional CFR decode, seeks, loops and alpha fallback"]
fn nvdec_frames_and_unsupported_alpha_match_software_reference() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hardware.mp4");
    generate(&path, "30000/1001", "libx264", "192x128", 96);
    let reference = reference(&path, "128:85");
    let fps = 30000. / 1001.;
    let cancel = AtomicBool::new(false);
    let mut pool = Pool {
        backend: Some(Backend::Cuda),
        ..Pool::default()
    };
    for f in 0..96 {
        let png = pool
            .frame_png(
                path.to_str().unwrap(),
                f as f64 / fps,
                fps,
                192,
                128,
                128,
                &cancel,
            )
            .unwrap();
        assert_eq!(
            pixels(&png).as_raw(),
            &reference[f * 128 * 85 * 4..(f + 1) * 128 * 85 * 4],
            "CUDA frame {f}"
        );
    }
    assert_eq!(pool.metrics.cuda_frames, 96);
    assert_eq!(pool.metrics.software_frames, 0);
    assert_eq!(pool.metrics.hardware_fallbacks, 0);
    assert_eq!(pool.metrics.starts, 1);
    pool.clear();
    for f in [61, 62, 4, 5, 88, 89, 0] {
        let png = pool
            .frame_png(
                path.to_str().unwrap(),
                f as f64 / fps,
                fps,
                192,
                128,
                128,
                &cancel,
            )
            .unwrap();
        assert_eq!(
            pixels(&png).as_raw(),
            &reference[f * 128 * 85 * 4..(f + 1) * 128 * 85 * 4],
            "CUDA seek frame {f}"
        );
    }
    let path = dir.path().join("alpha.mov");
    generate(&path, "30", "qtrle", "64x48", 12);
    let alpha = super::tests::reference(&path, "64:48");
    for f in [0, 1, 8, 0] {
        let png = pool
            .frame_png(
                path.to_str().unwrap(),
                f as f64 / 30.,
                30.,
                64,
                48,
                64,
                &cancel,
            )
            .unwrap();
        assert_eq!(
            pixels(&png).as_raw(),
            &alpha[f * 64 * 48 * 4..(f + 1) * 64 * 48 * 4],
            "Unsupported alpha frame {f}"
        );
    }
    assert_eq!(pool.metrics.hardware_fallbacks, 1);
    assert!(pool.metrics.software_frames > 0);
    assert!(
        pool.fallback_reason
            .as_ref()
            .unwrap()
            .contains("CUDA / NVDEC")
    );
    assert!(pool.bytes <= CACHE_BYTES && pool.streams.len() <= SESSIONS);
    pool.clear();
    std::fs::remove_file(path).unwrap();
    eprintln!("NVDEC qualification: {:?}", pool.metrics);
}

#[test]
#[ignore = "requires FFmpeg; verifies neutral limited-range YUV with an independent RGB oracle"]
fn limited_range_video_decode_preserves_neutral_rgb_rounding() {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("neutral.yuv");
    let path = dir.path().join("neutral.mkv");
    let width = 96usize;
    let height = 16usize;
    let values = [16u8, 32, 64, 128, 180, 235];
    let mut data = Vec::with_capacity(width * height * 3 / 2);
    for _ in 0..height {
        for y in values {
            data.extend(std::iter::repeat_n(y, 16));
        }
    }
    data.extend(std::iter::repeat_n(128, width * height / 2));
    std::fs::write(&raw, &data).unwrap();
    assert!(
        command(&ffmpeg_path())
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "rawvideo",
                "-pixel_format",
                "yuv420p",
                "-video_size",
                "96x16",
                "-framerate",
                "30",
                "-color_range",
                "tv",
                "-colorspace",
                "bt709",
                "-i",
            ])
            .arg(&raw)
            .args([
                "-frames:v",
                "1",
                "-c:v",
                "ffv1",
                "-color_range",
                "tv",
                "-colorspace",
                "bt709"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let cancel = AtomicBool::new(false);
    let mut pool = Pool::default();
    let png = pool
        .frame_png(
            path.to_str().unwrap(),
            0.0,
            30.0,
            width as u32,
            height as u32,
            width as u32,
            &cancel,
        )
        .unwrap();
    let decoded = pixels(&png);
    // Neutral chroma contributes zero; limited luma spans 16..235 over 0..255.
    for (i, y) in values.into_iter().enumerate() {
        let gray = ((f64::from(y) - 16.0) * 255.0 / 219.0).round() as u8;
        for row in 0..height {
            for column in i * 16..(i + 1) * 16 {
                assert_eq!(
                    decoded.get_pixel(column as u32, row as u32).0,
                    [gray, gray, gray, 255],
                    "Y={y}"
                );
            }
        }
    }
    let still = crate::footage::frame_png(
        path.to_str().unwrap(),
        0.0,
        width as u32,
        height as u32,
        width as u32,
    )
    .unwrap();
    assert_eq!(
        pixels(&still),
        decoded,
        "still and persistent decode must agree"
    );
    assert_eq!(std::fs::read(raw).unwrap(), data);
}

#[test]
#[ignore = "requires FFmpeg; verifies persistent CFR decoding, seeks, alpha, scaling, replacement and bounded caches"]
fn sequential_seek_loop_and_source_changes_match_continuous_reference() {
    let dir = tempfile::tempdir().unwrap();
    for (name, rate, fps, codec) in [
        ("fractional.mkv", "30000/1001", 30000. / 1001., "ffv1"),
        ("gop.mp4", "24000/1001", 24000. / 1001., "libx264"),
        ("alpha.mov", "30", 30., "qtrle"),
    ] {
        let path = dir.path().join(name);
        generate(&path, rate, codec, "64x48", 96);
        let all = reference(&path, "64:48");
        assert_eq!(all.len(), 96 * 64 * 48 * 4);
        let mut pool = Pool::default();
        let cancel = AtomicBool::new(false);
        for frame in 0..96 {
            let png = pool
                .frame_png(
                    path.to_str().unwrap(),
                    frame as f64 / fps,
                    fps,
                    64,
                    48,
                    64,
                    &cancel,
                )
                .unwrap();
            assert_eq!(
                pixels(&png).as_raw(),
                &all[frame * 64 * 48 * 4..(frame + 1) * 64 * 48 * 4],
                "{name} {frame}"
            );
        }
        assert_eq!(
            pool.metrics.starts, 1,
            "sequential playback must reuse the process"
        );
        assert_eq!(pool.metrics.frames, 96);
        for frame in [0, 53, 7, 95, 12, 1, 71, 71, 42] {
            let png = pool
                .frame_png(
                    path.to_str().unwrap(),
                    frame as f64 / fps,
                    fps,
                    64,
                    48,
                    64,
                    &cancel,
                )
                .unwrap();
            assert_eq!(
                pixels(&png).as_raw(),
                &all[frame * 64 * 48 * 4..(frame + 1) * 64 * 48 * 4]
            );
        }
        assert_eq!(
            pool.metrics.starts, 1,
            "cached reverse/hold/loop should not restart"
        );
        pool.clear();
        for frame in [74, 75, 77, 12, 13, 90, 91, 0, 1] {
            let png = pool
                .frame_png(
                    path.to_str().unwrap(),
                    frame as f64 / fps,
                    fps,
                    64,
                    48,
                    64,
                    &cancel,
                )
                .unwrap();
            assert_eq!(
                pixels(&png).as_raw(),
                &all[frame * 64 * 48 * 4..(frame + 1) * 64 * 48 * 4],
                "{name} seek {frame}"
            );
            assert!(pool.streams.len() <= SESSIONS);
            assert!(pool.bytes <= CACHE_BYTES);
        }
        let small = reference(&path, "32:24");
        let png = pool
            .frame_png(path.to_str().unwrap(), 11. / fps, fps, 64, 48, 32, &cancel)
            .unwrap();
        assert_eq!(
            pixels(&png).as_raw(),
            &small[11 * 32 * 24 * 4..12 * 32 * 24 * 4]
        );
        // Metadata changes invalidate cached frames and the open source handle.
        let before = pool.metrics.starts;
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::now() + Duration::from_secs(2))
            .unwrap();
        pool.frame_png(path.to_str().unwrap(), 11. / fps, fps, 64, 48, 32, &cancel)
            .unwrap();
        assert_eq!(pool.metrics.starts, before + 1);
        pool.clear();
        assert!(pool.streams.is_empty());
        assert_eq!(pool.bytes, 0);
        assert!(
            pool.frame_png(path.to_str().unwrap(), 200. / fps, fps, 64, 48, 64, &cancel)
                .is_err()
        );
    }
}

#[test]
#[ignore = "requires FFmpeg; records cold seek and sequential decode/PNG costs against per-frame processes"]
fn decoder_process_and_latency_benchmark() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("benchmark.mp4");
    generate(&path, "30", "libx264", "640x360", 90);
    let path = path.to_str().unwrap();
    let mut old = Vec::new();
    let start = Instant::now();
    for f in 0..60 {
        old.push(pixels(
            &crate::footage::frame_png(path, f as f64 / 30., 640, 360, 640).unwrap(),
        ));
    }
    let baseline = start.elapsed();
    let mut pool = Pool::default();
    let cancel = AtomicBool::new(false);
    let start = Instant::now();
    let first = pool
        .frame_png(path, 0., 30., 640, 360, 640, &cancel)
        .unwrap();
    let cold = start.elapsed();
    assert_eq!(pixels(&first), old[0]);
    for f in 1..60 {
        let p = pool
            .frame_png(path, f as f64 / 30., 30., 640, 360, 640, &cancel)
            .unwrap();
        assert_eq!(pixels(&p), old[f]);
    }
    let sequential = start.elapsed();
    assert_eq!(pool.metrics.starts, 1);
    assert!(pool.bytes <= CACHE_BYTES && pool.cache.len() <= 120);
    eprintln!(
        "Decoder benchmark 640x360 H264 60 frames: old={baseline:?}, persistent={sequential:?}, cold={cold:?}, metrics={:?}, PNG cache={} bytes",
        pool.metrics, pool.bytes
    );
    // Do not assert timing thresholds on shared CI machines.
}

#[test]
#[ignore = "requires FFmpeg; cancels a waiting reader and reaps a process with a full prefetch channel"]
fn cancellation_unblocks_waiting_reads_and_full_prefetch_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancel.mkv");
    generate(&path, "30", "ffv1", "64x48", 180);
    let source = Source::new(path.to_str().unwrap(), 30., 64, 48, 64).unwrap();
    let mut stream = Stream::open(source, 0).unwrap();
    let flag = Arc::new(AtomicBool::new(false));
    assert_eq!(stream.read(&flag).unwrap().len(), 64 * 48 * 4);
    // Keep the real pipe's channel full while injecting a deterministic pending
    // receive. Cancellation must work without the decoder delivering another frame.
    let actual = stream.receiver.take();
    let (_sender, pending) = mpsc::channel();
    stream.receiver = Some(pending);
    let cancel = flag.clone();
    let signal = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        cancel.store(true, Ordering::Release);
    });
    let started = Instant::now();
    assert!(stream.read(&flag).unwrap_err().contains("canceled"));
    signal.join().unwrap();
    drop(actual);
    drop(stream);
    assert!(started.elapsed() < Duration::from_secs(2));
    // Windows will reject deletion if the old decoder still holds this source.
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "requires FFmpeg; interleaves four sources beyond the frame-cache capacity"]
fn interleaved_sources_keep_sessions_and_evict_old_frames() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.mkv");
    generate(&first, "30", "ffv1", "64x48", 150);
    let mut paths = vec![first.clone()];
    for i in 1..4 {
        let path = dir.path().join(format!("source-{i}.mkv"));
        std::fs::copy(&first, &path).unwrap();
        paths.push(path);
    }
    let all = reference(&first, "64:48");
    let flag = AtomicBool::new(false);
    let mut pool = Pool::default();
    for f in 0..150 {
        for path in &paths {
            let p = pool
                .frame_png(
                    path.to_str().unwrap(),
                    f as f64 / 30.,
                    30.,
                    64,
                    48,
                    64,
                    &flag,
                )
                .unwrap();
            assert_eq!(
                pixels(&p).as_raw(),
                &all[f * 64 * 48 * 4..(f + 1) * 64 * 48 * 4]
            );
        }
    }
    assert_eq!(pool.metrics.starts, 4);
    assert_eq!(pool.streams.len(), 4);
    assert_eq!(pool.cache.len(), 120);
    assert!(pool.bytes <= CACHE_BYTES);
    let p = pool
        .frame_png(first.to_str().unwrap(), 0., 30., 64, 48, 64, &flag)
        .unwrap();
    assert_eq!(pixels(&p).as_raw(), &all[..64 * 48 * 4]);
    assert_eq!(pool.metrics.starts, 5);
    assert_eq!(pool.streams.len(), 4);
    pool.clear();
    assert_eq!(pool.bytes, 0);
    assert!(pool.streams.is_empty());
}
