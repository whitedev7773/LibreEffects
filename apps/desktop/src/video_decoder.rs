//! Per-renderer CFR decoder sessions. Pipes apply back pressure; neither read-ahead
//! nor frame caching grows with clip duration. Dropping a session reaps FFmpeg.
use base64::{Engine, engine::general_purpose::STANDARD};
use image::ImageEncoder;
use std::{
    collections::VecDeque,
    io::Read,
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime},
};

const CACHE_BYTES: usize = 32 * 1024 * 1024;
const SESSIONS: usize = 4;

#[derive(Clone, Debug, PartialEq)]
struct Source {
    path: String,
    modified: Option<SystemTime>,
    bytes: u64,
    fps: f64,
    width: u32,
    height: u32,
}
impl Source {
    fn new(path: &str, fps: f64, width: u32, height: u32, dimension: u32) -> Result<Self, String> {
        if !fps.is_finite()
            || !(1.0..=240.0).contains(&fps)
            || width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || dimension == 0
        {
            return Err("Invalid video decoder dimensions or frame rate".into());
        }
        let metadata = std::fs::metadata(path).map_err(|_| format!("Footage offline: {path}"))?;
        if !metadata.is_file() {
            return Err("Footage path is not a file".into());
        }
        let scale = (f64::from(dimension) / f64::from(width.max(height))).min(1.0);
        Ok(Self {
            path: path.into(),
            modified: metadata.modified().ok(),
            bytes: metadata.len(),
            fps,
            width: (f64::from(width) * scale).round().max(1.0) as u32,
            height: (f64::from(height) * scale).round().max(1.0) as u32,
        })
    }
    fn frame(&self, seconds: f64) -> Result<u64, String> {
        if !seconds.is_finite() || !(0.0..=86400.0).contains(&seconds) {
            return Err("Invalid video sample time".into());
        }
        Ok((seconds * self.fps + 1e-7).floor() as u64)
    }
}

struct Stream {
    source: Source,
    next: u64,
    child: Child,
    receiver: Option<mpsc::Receiver<Result<Vec<u8>, String>>>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Stream {
    fn drop(&mut self) {
        // Unblock both a pipe read and a full bounded channel before joining.
        self.receiver.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
impl Stream {
    fn open(source: Source, first: u64) -> Result<Self, String> {
        // Seek between CFR timestamps to avoid microsecond/container rounding
        // discarding the requested frame. Accurate input seek decodes the GOP.
        let seek = (first as f64 - 0.5).max(0.0) / source.fps;
        let mut command = crate::footage::command(&crate::video_export::ffmpeg_path());
        command
            .args([
                "-v",
                "error",
                "-nostdin",
                "-protocol_whitelist",
                "file,pipe",
                "-threads",
                "2",
                "-ss",
                &format!("{seek:.9}"),
                "-i",
                &source.path,
                "-map",
                "0:v:0",
                "-an",
                "-sn",
                "-dn",
                "-vf",
                &format!("scale={}:{},setsar=1", source.width, source.height),
                "-fps_mode",
                "passthrough",
                "-threads",
                "1",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot start video decoder: {e}"))?;
        let mut stdout = child.stdout.take().ok_or("Missing video decoder pipe")?;
        let bytes = source.width as usize * source.height as usize * 4;
        // At most two queued frames / 8 MiB, plus one in-flight pipe read. Large
        // images use a rendezvous channel instead of a second large allocation.
        let (sender, receiver) = mpsc::sync_channel((8 * 1024 * 1024 / bytes).min(2));
        let reader = std::thread::Builder::new()
            .name("video-pipe".into())
            .spawn(move || {
                loop {
                    let mut pixels = vec![0; bytes];
                    let result = stdout
                        .read_exact(&mut pixels)
                        .map(|()| pixels)
                        .map_err(|e| {
                            format!("Video decoder ended before the requested frame: {e}")
                        });
                    let failed = result.is_err();
                    if sender.send(result).is_err() || failed {
                        break;
                    }
                }
            });
        let reader = match reader {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        };
        Ok(Self {
            source,
            next: first,
            child,
            receiver: Some(receiver),
            reader: Some(reader),
        })
    }
    fn read(&mut self, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
        let started = Instant::now();
        loop {
            check_cancel(cancel)?;
            match self
                .receiver
                .as_ref()
                .unwrap()
                .recv_timeout(Duration::from_millis(5))
            {
                Ok(result) => {
                    self.next += 1;
                    return result;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Video decoder worker stopped".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if started.elapsed() > Duration::from_secs(15) {
                return Err("Video decoding exceeded its 15 second frame deadline".into());
            }
        }
    }
}

pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Preview rendering canceled".into())
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Metrics {
    pub starts: u64,
    pub frames: u64,
    pub hits: u64,
    pub decode: Duration,
    pub png: Duration,
}
#[derive(Default)]
pub(crate) struct Pool {
    streams: VecDeque<Stream>,
    cache: VecDeque<(Source, u64, Arc<str>)>,
    bytes: usize,
    pub metrics: Metrics,
}
impl Pool {
    pub fn clear(&mut self) {
        self.streams.clear();
        self.cache.clear();
        self.bytes = 0;
    }
    pub fn frame_png(
        &mut self,
        path: &str,
        seconds: f64,
        fps: f64,
        width: u32,
        height: u32,
        dimension: u32,
        cancel: &AtomicBool,
    ) -> Result<Arc<str>, String> {
        check_cancel(cancel)?;
        let source = Source::new(path, fps, width, height, dimension)?;
        let frame = source.frame(seconds)?;
        // Same path replacement closes the old handle even when its frame is cached.
        self.streams
            .retain(|s| s.source.path != source.path || s.source == source);
        if let Some(i) = self
            .cache
            .iter()
            .position(|(s, f, _)| *s == source && *f == frame)
        {
            let entry = self.cache.remove(i).unwrap();
            let result = entry.2.clone();
            self.cache.push_back(entry);
            self.metrics.hits += 1;
            return Ok(result);
        }
        let index = self
            .streams
            .iter()
            .enumerate()
            .filter(|(_, s)| s.source == source && s.next <= frame && frame - s.next <= 32)
            .min_by_key(|(_, s)| frame - s.next)
            .map(|(i, _)| i);
        let mut stream = if let Some(i) = index {
            self.streams.remove(i).unwrap()
        } else {
            while self.streams.len() >= SESSIONS {
                self.streams.pop_front();
            }
            self.metrics.starts += 1;
            Stream::open(source.clone(), frame)?
        };
        let mut requested = None;
        while stream.next <= frame {
            let index = stream.next;
            let start = Instant::now();
            let pixels = stream
                .read(cancel)
                .map_err(|error| format!("{error} ({path}, frame {index})"))?;
            self.metrics.decode += start.elapsed();
            self.metrics.frames += 1;
            check_cancel(cancel)?;
            let start = Instant::now();
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new_with_quality(
                &mut png,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Sub,
            )
            .write_image(
                &pixels,
                source.width,
                source.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?;
            let png: Arc<str> = STANDARD.encode(png).into();
            self.metrics.png += start.elapsed();
            if index == frame {
                requested = Some(png.clone());
            }
            while !self.cache.is_empty()
                && (self.bytes + png.len() > CACHE_BYTES || self.cache.len() >= 120)
            {
                self.bytes -= self.cache.pop_front().unwrap().2.len();
            }
            if png.len() <= CACHE_BYTES {
                self.bytes += png.len();
                self.cache.push_back((source.clone(), index, png));
            }
        }
        self.streams.push_back(stream);
        check_cancel(cancel)?;
        requested.ok_or_else(|| "Video decoder did not return the requested frame".into())
    }
}

#[cfg(test)]
#[path = "video_decoder_tests.rs"]
mod tests;
