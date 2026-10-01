//! Bounded producer/device transport. Disk access and mixing never block GPUI
//! or the WASAPI consumer. Device position, not queued samples, drives video.
use crate::audio_mix::{Levels, Mixer, SAMPLE_RATE};
use libre_effects_core::{Frame, FrameRate, Project};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
#[cfg(windows)]
#[path = "audio_device.rs"]
mod device;

const BLOCK: usize = 4800;
const PREROLL: usize = 24_000;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Range {
    pub fps: FrameRate,
    pub start: Frame,
    pub end: Frame,
    pub first: Frame,
    pub looping: bool,
    pub limit: Option<u64>,
}
impl Range {
    pub fn new(
        fps: FrameRate,
        start: Frame,
        end: Frame,
        first: Frame,
        looping: bool,
    ) -> Result<Self, String> {
        if start >= end || first < start || first >= end {
            return Err("Invalid audio preview range".into());
        }
        Ok(Self {
            fps,
            start,
            end,
            first,
            looping,
            limit: None,
        })
    }
    fn denominator(self) -> u128 {
        u128::from(self.fps.denominator()) * u128::from(SAMPLE_RATE)
    }
    fn remaining(self, sample: u64) -> u128 {
        let width = u128::from(self.end - self.start) * self.denominator();
        let offset = u128::from(self.first - self.start) * self.denominator()
            + u128::from(sample) * u128::from(self.fps.numerator());
        if self.looping {
            width - offset % width
        } else {
            width.saturating_sub(offset)
        }
    }
    pub fn frame(self, sample: u64) -> Frame {
        let width = u128::from(self.end - self.start) * self.denominator();
        self.start + ((width - self.remaining(sample)) / self.denominator()) as u32
    }
    fn seconds(self, sample: u64) -> f64 {
        let width = u128::from(self.end - self.start) * self.denominator();
        self.fps.seconds(u64::from(self.start))
            + (width - self.remaining(sample)) as f64
                / (f64::from(SAMPLE_RATE) * f64::from(self.fps.numerator()))
    }
    fn count(self, sample: u64, maximum: usize) -> usize {
        let until_wrap = self
            .remaining(sample)
            .div_ceil(u128::from(self.fps.numerator()));
        let until_limit = self
            .limit
            .map_or(u64::MAX, |limit| limit.saturating_sub(sample));
        until_wrap.min(u128::from(until_limit)).min(maximum as u128) as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Buffering,
    Playing,
    Ended,
    Failed(String),
}
#[derive(Clone, Debug)]
pub(crate) struct Status {
    pub phase: Phase,
    pub played: u64,
    pub submitted: u64,
    pub underruns: u64,
    pub levels: Levels,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            phase: Phase::Buffering,
            played: 0,
            submitted: 0,
            underruns: 0,
            levels: Levels::default(),
        }
    }
}
pub(crate) struct Session {
    cancel: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
    pub range: Range,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Session {
    pub fn start(project: &Project, range: Range) -> Result<Option<Self>, String> {
        let mixer = Mixer::new(project, true)?;
        if !mixer.has_audio() {
            return Ok(None);
        }
        #[cfg(not(windows))]
        {
            let _ = (mixer, range);
            Err("Audio preview currently requires Windows WASAPI".into())
        }
        #[cfg(windows)]
        {
            let cancel = Arc::new(AtomicBool::new(false));
            let status = Arc::new(Mutex::new(Status::default()));
            let (flag, result) = (cancel.clone(), status.clone());
            std::thread::Builder::new()
                .name("audio-output".into())
                .spawn(move || {
                    let outcome = run(mixer, range, &flag, &result);
                    flag.store(true, Ordering::Release);
                    if let Err(error) = outcome {
                        result.lock().unwrap().phase = Phase::Failed(error);
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(Some(Self {
                cancel,
                status,
                range,
            }))
        }
    }
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}

struct Block {
    pcm: Vec<[f32; 2]>,
    levels: Levels,
}
enum Message {
    Block(Block),
    End,
    Failed(String),
}
fn produce(mut mixer: Mixer, range: Range, cancel: &AtomicBool, sender: mpsc::SyncSender<Message>) {
    let mut sample = 0;
    while !cancel.load(Ordering::Acquire) {
        let mut pcm = Vec::with_capacity(BLOCK);
        mixer.levels = Levels::default();
        while pcm.len() < BLOCK {
            let count = range.count(sample, BLOCK - pcm.len());
            if count == 0 {
                break;
            }
            match mixer.render(
                range.seconds(sample),
                0,
                count,
                range.fps.seconds(u64::from(range.end)),
                cancel,
            ) {
                Ok(data) => pcm.extend(data),
                Err(error) => {
                    let _ = sender.send(Message::Failed(error));
                    return;
                }
            }
            sample += count as u64;
        }
        if pcm.is_empty() {
            let _ = sender.send(Message::End);
            return;
        }
        if sender
            .send(Message::Block(Block {
                pcm,
                levels: mixer.levels.clone(),
            }))
            .is_err()
        {
            return;
        }
    }
}

/// Scheduling logic is shared with a deterministic device simulation in tests.
trait Output {
    fn capacity(&self) -> usize;
    fn padding(&self) -> Result<usize, String>;
    fn position(&self) -> Result<u64, String>;
    fn write(&self, pcm: &[[f32; 2]]) -> Result<(), String>;
    fn start(&self) -> Result<(), String>;
    fn reset(&self) -> Result<(), String>;
}
#[cfg(windows)]
impl Output for device::Device {
    fn capacity(&self) -> usize {
        self.capacity as usize
    }
    fn padding(&self) -> Result<usize, String> {
        self.padding().map(|n| n as usize)
    }
    fn position(&self) -> Result<u64, String> {
        self.position()
    }
    fn write(&self, pcm: &[[f32; 2]]) -> Result<(), String> {
        self.write(pcm)
    }
    fn start(&self) -> Result<(), String> {
        self.start()
    }
    fn reset(&self) -> Result<(), String> {
        self.reset()
    }
}
struct Transport {
    pending: VecDeque<Block>,
    cursor: usize,
    buffered: usize,
    meters: VecDeque<(u64, Levels)>,
    ended: bool,
    running: bool,
    base: u64,
    status: Status,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            cursor: 0,
            buffered: 0,
            meters: VecDeque::new(),
            ended: false,
            running: false,
            base: 0,
            status: Status::default(),
        }
    }
}
impl Transport {
    fn accept(&mut self, message: Message) -> Result<(), String> {
        match message {
            Message::Block(block) => {
                self.buffered += block.pcm.len();
                self.pending.push_back(block);
            }
            Message::End => self.ended = true,
            Message::Failed(error) => return Err(error),
        }
        Ok(())
    }
    fn tick(&mut self, device: &impl Output) -> Result<bool, String> {
        let padding = device.padding()?;
        if padding > device.capacity() {
            return Err("Invalid audio-device padding".into());
        }
        if self.running {
            self.status.played = (self.base + device.position()?)
                .min(self.status.submitted)
                .max(self.status.played);
            while self
                .meters
                .front()
                .is_some_and(|(end, _)| *end <= self.status.played)
            {
                let (_, levels) = self.meters.pop_front().unwrap();
                self.status.levels = levels;
            }
            if padding == 0 {
                self.status.played = self.status.submitted;
                if self.ended && self.buffered == 0 {
                    self.status.phase = Phase::Ended;
                    return Ok(true);
                }
                // Freeze composition time during starvation; restart the device
                // clock only after refilling. Never skip unheard composition samples.
                device.reset()?;
                self.running = false;
                self.base = self.status.submitted;
                self.status.underruns += 1;
                self.status.phase = Phase::Buffering;
                self.status.levels = Levels::default();
            }
        }
        if !self.running && self.buffered < PREROLL && !self.ended {
            return Ok(false);
        }
        let mut free = device.capacity() - if self.running { padding } else { 0 };
        while free > 0 {
            let Some(block) = self.pending.front() else {
                break;
            };
            let count = free.min(block.pcm.len() - self.cursor);
            device.write(&block.pcm[self.cursor..self.cursor + count])?;
            self.cursor += count;
            self.buffered -= count;
            self.status.submitted += count as u64;
            free -= count;
            if self.cursor == block.pcm.len() {
                let block = self.pending.pop_front().unwrap();
                self.meters.push_back((self.status.submitted, block.levels));
                self.cursor = 0;
            }
        }
        if !self.running && self.status.submitted > self.base {
            device.start()?;
            self.running = true;
            self.status.phase = Phase::Playing;
        }
        if !self.running && self.ended && self.buffered == 0 {
            self.status.phase = Phase::Ended;
            return Ok(true);
        }
        Ok(false)
    }
}
#[cfg(windows)]
fn run(
    mixer: Mixer,
    range: Range,
    cancel: &Arc<AtomicBool>,
    status: &Mutex<Status>,
) -> Result<(), String> {
    let _apartment = device::Apartment::new()?;
    let device = device::Device::open()?;
    let (sender, receiver) = mpsc::sync_channel(12);
    let flag = cancel.clone();
    let producer = std::thread::Builder::new()
        .name("audio-mix".into())
        .spawn(move || produce(mixer, range, &flag, sender))
        .map_err(|e| e.to_string())?;
    let mut transport = Transport::default();
    let result = (|| {
        while !cancel.load(Ordering::Acquire) {
            while transport.buffered < 48_000 && !transport.ended {
                match receiver.try_recv() {
                    Ok(message) => transport.accept(message)?,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("Audio mixing worker stopped unexpectedly".into());
                    }
                }
            }
            let done = transport.tick(&device)?;
            *status.lock().unwrap() = transport.status.clone();
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    })();
    // Device reset is immediate; joining a canceled decoder happens off GPUI.
    let _ = device.reset();
    cancel.store(true, Ordering::Release);
    drop(receiver);
    let _ = producer.join();
    result
}

#[cfg(test)]
#[path = "audio_playback_tests.rs"]
mod tests;
