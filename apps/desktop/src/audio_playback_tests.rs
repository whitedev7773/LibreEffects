use super::*;
use std::cell::RefCell;

#[test]
fn rational_loop_clock_and_nonloop_end_do_not_accumulate_rounding() {
    let fps = FrameRate::new(30000, 1001).unwrap();
    let range = Range::new(fps, 3, 13, 8, true).unwrap();
    for sample in [
        0,
        1,
        8007,
        8008,
        16016,
        48000 * 61,
        4_294_967_296,
        48_000 * 86_400_u64,
    ] {
        let expected = 3.0 + ((5.0 + sample as f64 * 30000.0 / 1001.0 / 48000.0) % 10.0);
        assert!((range.seconds(sample) * fps.as_f64() - expected).abs() < 1e-7);
        assert!(range.frame(sample) >= 3 && range.frame(sample) < 13);
        assert!(range.count(sample, 4800) > 0);
    }
    let finite = Range::new(fps, 3, 13, 8, false).unwrap();
    assert_eq!(finite.count(0, usize::MAX), 8008);
    assert_eq!(finite.count(8008, 4800), 0);
    assert_eq!(finite.frame(8008), 13);
    let mut scrub = finite;
    scrub.limit = Some(4800);
    assert_eq!(scrub.count(4799, 4800), 1);
    assert_eq!(scrub.count(4800, 4800), 0);
    assert!(Range::new(fps, 0, 0, 0, true).is_err());
}

#[derive(Default)]
struct FakeState {
    position: u64,
    padding: usize,
    started: bool,
    writes: Vec<[f32; 2]>,
    resets: usize,
}
#[derive(Default)]
struct Fake(RefCell<FakeState>);
impl Fake {
    fn advance(&self, count: usize) {
        let mut s = self.0.borrow_mut();
        if s.started {
            s.position += count as u64;
            s.padding = s.padding.saturating_sub(count);
        }
    }
}
impl Output for Fake {
    fn capacity(&self) -> usize {
        4800
    }
    fn padding(&self) -> Result<usize, String> {
        Ok(self.0.borrow().padding)
    }
    fn position(&self) -> Result<u64, String> {
        Ok(self.0.borrow().position)
    }
    fn write(&self, pcm: &[[f32; 2]]) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        assert!(s.padding + pcm.len() <= self.capacity());
        s.padding += pcm.len();
        s.writes.extend_from_slice(pcm);
        Ok(())
    }
    fn start(&self) -> Result<(), String> {
        self.0.borrow_mut().started = true;
        Ok(())
    }
    fn reset(&self) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        s.position = 0;
        s.padding = 0;
        s.started = false;
        s.resets += 1;
        Ok(())
    }
}
fn block(value: f32) -> Message {
    Message::Block(Block {
        pcm: vec![[value, -value]; BLOCK],
        levels: Levels {
            peak: [value; 2],
            frames: BLOCK as u64,
            sum_squares: [f64::from(value).powi(2) * BLOCK as f64; 2],
            clipped_frames: 0,
        },
    })
}
#[test]
fn device_clock_preroll_starvation_resume_and_drain_preserve_every_sample() {
    let device = Fake::default();
    let mut t = Transport::default();
    t.accept(block(0.1)).unwrap();
    t.tick(&device).unwrap();
    assert_eq!(t.status.phase, Phase::Buffering);
    assert_eq!(t.status.submitted, 0);
    for _ in 0..4 {
        t.accept(block(0.1)).unwrap();
    }
    t.tick(&device).unwrap();
    assert_eq!(t.status.phase, Phase::Playing);
    assert_eq!(t.status.played, 0); // queued is not heard
    assert_eq!(t.status.submitted, 4800);
    for _ in 0..50 {
        device.advance(480);
        t.tick(&device).unwrap();
    }
    assert_eq!(t.status.phase, Phase::Buffering);
    assert_eq!(t.status.played, 24000);
    assert_eq!(t.status.underruns, 1);
    device.advance(48000);
    t.tick(&device).unwrap();
    assert_eq!(t.status.played, 24000);
    t.accept(block(0.2)).unwrap();
    t.accept(Message::End).unwrap();
    t.tick(&device).unwrap();
    for _ in 0..9 {
        device.advance(480);
        assert!(!t.tick(&device).unwrap());
    }
    device.advance(480);
    assert!(t.tick(&device).unwrap());
    assert_eq!(t.status.phase, Phase::Ended);
    assert_eq!(t.status.played, 28800);
    assert_eq!(t.status.levels.peak, [0.2; 2]);
    let d = device.0.borrow();
    assert_eq!(d.writes.len(), 28800);
    assert!(d.writes[..24000].iter().all(|v| *v == [0.1, -0.1]));
    assert!(d.writes[24000..].iter().all(|v| *v == [0.2, -0.2]));
}
#[test]
fn cancellation_and_worker_errors_are_explicit() {
    let flag = Arc::new(AtomicBool::new(false));
    let range = Range::new(FrameRate::new(30, 1).unwrap(), 0, 30, 0, true).unwrap();
    let session = Session {
        cancel: flag.clone(),
        status: Default::default(),
        range,
        permitted: Arc::new(AtomicU64::new(u64::MAX)),
    };
    drop(session);
    assert!(flag.load(Ordering::Acquire));
    let mut t = Transport::default();
    assert_eq!(
        t.accept(Message::Failed("source missing".into())),
        Err("source missing".into())
    );
    assert!(
        Session::start(&Project::default(), range)
            .unwrap()
            .is_none()
    );
}

#[test]
fn render_gate_freezes_audio_and_resumes_without_losing_or_repeating_samples() {
    let device = Fake::default();
    let mut transport = Transport::default();
    transport.permitted = 0;
    for _ in 0..5 {
        transport.accept(block(0.1)).unwrap();
    }
    transport.tick(&device).unwrap();
    assert_eq!(transport.status.submitted, 0);
    for boundary in [800, 1600, 2400, 3200, 4000, 4800, 5600] {
        transport.permitted = boundary;
        transport.tick(&device).unwrap();
        assert_eq!(transport.status.submitted, boundary);
        device.advance(800);
        transport.tick(&device).unwrap();
        assert_eq!(transport.status.played, boundary);
        assert_eq!(transport.status.phase, Phase::Buffering);
        device.advance(48000);
        transport.tick(&device).unwrap();
        assert_eq!(transport.status.played, boundary);
        assert_eq!(transport.status.underruns, 0);
    }
    assert_eq!(device.0.borrow().writes.len(), 5600);
    assert!(
        device
            .0
            .borrow()
            .writes
            .iter()
            .all(|sample| *sample == [0.1, -0.1])
    );
}

#[test]
fn speed_and_fractional_frame_boundaries_keep_audio_inside_rendered_intervals() {
    let fps = FrameRate::new(30000, 1001).unwrap();
    for quarters in [1, 2, 4, 6, 8] {
        let mut range = Range::new(fps, 3, 13, 8, true).unwrap();
        range.speed_quarters = quarters;
        for intervals in 1..1000 {
            let sample = range.frame_boundary(intervals);
            let exact = intervals as f64 * 48000.0 / fps.as_f64() * 4.0 / f64::from(quarters);
            assert!(sample as f64 >= exact - 1e-8);
            assert!((sample as f64) < exact + 1.0 + 1e-8);
            let expected = 3.0
                + ((5.0 + sample as f64 * fps.as_f64() / 48000.0 * f64::from(quarters) / 4.0)
                    % 10.0);
            assert!((range.seconds(sample) * fps.as_f64() - expected).abs() < 1e-8);
        }
    }
}

fn audio_project(seconds: u32, fps: FrameRate) -> (tempfile::TempDir, libre_effects_core::Editor) {
    use libre_effects_core::{AudioMetadata, Command, Content, Editor};
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preview.wav");
    let frames = SAMPLE_RATE * seconds;
    let mut f = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(frames * 8 + 36).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&3u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
    f.write_all(&(SAMPLE_RATE * 8).to_le_bytes()).unwrap();
    f.write_all(&8u16.to_le_bytes()).unwrap();
    f.write_all(&32u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&(frames * 8).to_le_bytes()).unwrap();
    for i in 0..frames {
        let x = tone(f64::from(i));
        f.write_all(&x.to_le_bytes()).unwrap();
        f.write_all(&(-x * 0.5).to_le_bytes()).unwrap();
    }
    f.flush().unwrap();
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "Preview QA".into(),
            width: 64,
            height: 48,
            fps,
            duration: (f64::from(seconds) * fps.as_f64()).ceil() as u32,
            display_start: 0,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            name: "Preview sound".into(),
            width: 1.0,
            height: 1.0,
            content: Content::Audio {
                path: path.to_string_lossy().into(),
                audio: AudioMetadata {
                    stream_index: 0,
                    sample_rate: SAMPLE_RATE,
                    channels: 2,
                    channel_layout: "stereo".into(),
                    duration: f64::from(seconds),
                    start_time: 0.0,
                    file_offset: 0.0,
                },
                start_frame: 0,
                playback: Default::default(),
            },
        })
        .unwrap();
    (dir, editor)
}
fn tone(sample: f64) -> f32 {
    (sample * 220.0 * std::f64::consts::TAU / 48000.0).sin() as f32 * 0.02
}

#[test]
#[ignore = "Requires FFmpeg"]
fn preview_speed_pcm_samples_match_source_time_at_every_speed() {
    let fps = FrameRate::new(60, 1).unwrap();
    let (_dir, editor) = audio_project(2, fps);
    for quarters in [1, 2, 4, 6, 8] {
        let mut range = Range::new(fps, 0, 120, 0, false).unwrap();
        range.speed_quarters = quarters;
        range.limit = Some(4800);
        let (sender, receiver) = mpsc::sync_channel(4);
        produce(
            Mixer::new(editor.project(), true).unwrap(),
            range,
            &AtomicBool::new(false),
            sender,
        );
        let Message::Block(block) = receiver.recv().unwrap() else {
            panic!("missing audio");
        };
        assert_eq!(block.pcm.len(), 4800);
        for (index, sample) in block.pcm.iter().enumerate() {
            let source = index as f64 * f64::from(quarters) / 4.0;
            let lo = source.floor();
            let expected =
                f64::from(tone(lo)) + f64::from(tone(lo + 1.0) - tone(lo)) * (source - lo);
            assert!((f64::from(sample[0]) - expected).abs() < 1e-7);
            assert!((f64::from(sample[1]) + expected * 0.5).abs() < 1e-7);
        }
        assert!(matches!(receiver.recv().unwrap(), Message::End));
    }
}

#[cfg(windows)]
#[test]
#[ignore = "Requires default Windows audio output and FFmpeg; plays a short quiet tone"]
fn wasapi_waits_for_rendered_frames_before_playing_and_during_stalls() {
    let fps = FrameRate::new(60, 1).unwrap();
    let (_dir, editor) = audio_project(2, fps);
    let mut range = Range::new(fps, 0, 10, 0, false).unwrap();
    range.render_gated = true;
    let session = Session::start(editor.project(), range).unwrap().unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(session.status().submitted, 0);
    for intervals in 1..=10 {
        session.permit_frames(intervals);
        let boundary = range.frame_boundary(intervals);
        let started = std::time::Instant::now();
        loop {
            let status = session.status();
            assert!(!matches!(status.phase, Phase::Failed(_)), "{status:?}");
            assert!(status.played <= boundary && status.submitted <= boundary);
            if status.played == boundary {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5), "{status:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(session.status().played, boundary);
        assert_eq!(session.status().underruns, 0);
    }
    assert_eq!(session.status().phase, Phase::Ended);
}

#[test]
#[ignore = "Requires FFmpeg"]
fn preview_fractional_loop_and_scrub_pcm_match_source_math() {
    let fps = FrameRate::new(24000, 1001).unwrap();
    let (_dir, editor) = audio_project(2, fps);
    let mut range = Range::new(fps, 3, 14, 9, true).unwrap();
    range.limit = Some(3 * 48000);
    let (sender, receiver) = mpsc::sync_channel(40);
    produce(
        Mixer::new(editor.project(), true).unwrap(),
        range,
        &AtomicBool::new(false),
        sender,
    );
    let mut actual = Vec::new();
    while let Ok(message) = receiver.recv() {
        match message {
            Message::Block(b) => actual.extend(b.pcm),
            Message::End => break,
            Message::Failed(e) => panic!("{e}"),
        }
    }
    assert_eq!(actual.len(), 144000);
    for (sample, a) in actual.iter().enumerate() {
        let frame = 3.0 + (6.0 + sample as f64 * fps.as_f64() / 48000.0) % 11.0;
        let source = frame / fps.as_f64() * 48000.0;
        let lo = source.floor();
        let x = f64::from(tone(lo)) + f64::from(tone(lo + 1.0) - tone(lo)) * (source - lo);
        assert!((f64::from(a[0]) - x).abs() < 1e-7, "sample {sample}");
        assert!((f64::from(a[1]) + x * 0.5).abs() < 1e-7);
    }
    range.looping = false;
    range.limit = Some(4800);
    let (sender, receiver) = mpsc::sync_channel(4);
    produce(
        Mixer::new(editor.project(), true).unwrap(),
        range,
        &AtomicBool::new(false),
        sender,
    );
    let Message::Block(scrub) = receiver.recv().unwrap() else {
        panic!("missing scrub")
    };
    assert_eq!(scrub.pcm, actual[..4800]);
    assert!(matches!(receiver.recv().unwrap(), Message::End));
}

#[cfg(windows)]
#[test]
#[ignore = "Requires default Windows audio output and FFmpeg; plays a quiet 65-second tone"]
fn device_clock_minute_seek_loop_pause_and_cancel() {
    use std::time::Instant;
    let fps = FrameRate::new(30000, 1001).unwrap();
    let (_dir, editor) = audio_project(65, fps);
    let duration = editor.project().composition().duration();
    let range = Range::new(fps, 0, duration, 0, false).unwrap();
    let session = Session::start(editor.project(), range).unwrap().unwrap();
    let deadline = Instant::now();
    let mut reference = None;
    let mut max_drift: f64 = 0.0;
    loop {
        let s = session.status();
        assert!(!matches!(s.phase, Phase::Failed(_)), "{:?}", s.phase);
        assert!(
            deadline.elapsed() < Duration::from_secs(85),
            "device stalled: {s:?}"
        );
        if s.played > 48000 {
            let (sample, instant) = *reference.get_or_insert((s.played, Instant::now()));
            let drift =
                ((s.played - sample) as f64 / 48000.0 - instant.elapsed().as_secs_f64()).abs();
            max_drift = max_drift.max(drift);
            assert_eq!(s.underruns, 0, "audio underrun: {s:?}");
            assert!(max_drift < 0.1, "device drift {max_drift}");
        }
        if s.played >= 62 * 48000 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let before_pause = session.status();
    let old = session.status.clone();
    drop(session);
    std::thread::sleep(Duration::from_millis(100));
    let stopped = old.lock().unwrap().played;
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(old.lock().unwrap().played, stopped);
    assert!(stopped - before_pause.played < 4800);
    for first in [17, 5] {
        let mut range = Range::new(fps, 3, 23, first, true).unwrap();
        range.limit = Some(48000);
        let session = Session::start(editor.project(), range).unwrap().unwrap();
        let deadline = Instant::now();
        loop {
            let s = session.status();
            assert!(!matches!(s.phase, Phase::Failed(_)), "{:?}", s.phase);
            assert!(deadline.elapsed() < Duration::from_secs(10));
            if s.phase == Phase::Ended {
                assert_eq!(s.played, 48000);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    println!(
        "WASAPI: 62 seconds, max device/wall clock delta {max_drift:.6}s, zero underruns; pause, seek, loop and finite drain passed"
    );
}
