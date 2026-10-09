//! Bounded frame parallelism. The consumer sees complete frames in source order.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

const MAX_WORKERS: usize = 6;
const GIB: u64 = 1024 * 1024 * 1024;

#[cfg(windows)]
fn available_memory() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: initialized size and writable native structure, no retained pointers.
    unsafe {
        GlobalMemoryStatusEx(&mut status).ok()?;
    }
    Some(status.ullAvailPhys)
}
#[cfg(not(windows))]
fn available_memory() -> Option<u64> {
    None
}

fn choose_workers(
    width: u32,
    height: u32,
    frames: u32,
    logical: usize,
    available: Option<u64>,
    requested: Option<usize>,
) -> usize {
    let pixels = u64::from(width) * u64::from(height);
    if frames <= 1 || pixels > 4_194_304 {
        return 1;
    }
    // Keep 2 GiB for the system; each lane admits 2 GiB plus two raw frames.
    // This covers the renderer caches, checked support, decoder and expression
    // process. Unknown memory keeps the previous two-lane ceiling.
    let memory_limit = available
        .map_or(2, |bytes| {
            (bytes.saturating_sub(2 * GIB) / (2 * GIB + pixels * 8)) as usize
        })
        .max(1);
    let cpu_limit = (logical / 2).max(1).min(MAX_WORKERS);
    requested
        .unwrap_or(cpu_limit)
        .max(1)
        .min(cpu_limit)
        .min(memory_limit)
        .min(frames as usize)
}

pub(crate) fn workers(width: u32, height: u32, frames: u32) -> usize {
    let requested = std::env::var("LIBRE_EFFECTS_RENDER_WORKERS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|v| *v >= 1);
    let lanes = choose_workers(
        width,
        height,
        frames,
        std::thread::available_parallelism().map_or(1, |v| v.get()),
        available_memory(),
        requested,
    );
    if std::env::var_os("LIBRE_EFFECTS_RENDER_PROFILE").is_some() {
        eprintln!("Frame render workers: {lanes} (ordered output)");
    }
    lanes
}

pub(crate) fn ordered<T: Send>(
    frames: u32,
    lanes: usize,
    cancel: &AtomicBool,
    produce: impl Fn(usize, u32) -> Result<T, String> + Sync,
    mut consume: impl FnMut(u32, T) -> Result<(), String>,
) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    let lanes = lanes.clamp(1, MAX_WORKERS).min(frames as usize);
    if lanes == 1 {
        for index in 0..frames {
            if cancel.load(Ordering::Relaxed) {
                return Err("Render canceled".into());
            }
            consume(index, produce(0, index)?)?;
        }
        return Ok(());
    }
    let stopped = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let mut receivers = Vec::new();
        let mut threads = Vec::new();
        for lane in 0..lanes {
            let (tx, rx) = mpsc::sync_channel(1);
            receivers.push(rx);
            let (produce, stopped) = (&produce, &stopped);
            threads.push(scope.spawn(move || {
                for index in (lane as u32..frames).step_by(lanes) {
                    if stopped.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = produce(lane, index);
                    let failed = result.is_err();
                    if tx.send(result).is_err() || failed {
                        break;
                    }
                }
            }));
        }
        let result = (|| {
            for index in 0..frames {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Render canceled".into());
                }
                let value = receivers[index as usize % lanes]
                    .recv()
                    .map_err(|_| "Frame render worker stopped unexpectedly".to_string())??;
                if cancel.load(Ordering::Relaxed) {
                    return Err("Render canceled".into());
                }
                consume(index, value)?;
            }
            Ok(())
        })();
        stopped.store(true, Ordering::Relaxed);
        // Unblock every bounded sender before joining on failure/cancellation.
        drop(receivers);
        let mut panicked = false;
        for thread in threads {
            panicked |= thread.join().is_err();
        }
        if panicked {
            Err("Frame render worker failed".into())
        } else {
            result
        }
    })
}

pub(crate) fn renderers(lanes: usize, cancel: Arc<AtomicBool>) -> Vec<crate::rendering::Renderer> {
    // Each lane advances its own media decoder and owns an isolated expression
    // process. No out-of-order decode requests force cross-lane backward seeks.
    (0..lanes)
        .map(|_| crate::rendering::Renderer::with_cancel(cancel.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uneven_parallel_frames_are_emitted_in_order() {
        for lanes in [2, 4, 6] {
            let mut output = Vec::new();
            ordered(
                17,
                lanes,
                &AtomicBool::new(false),
                |lane, index| {
                    if lane == 0 {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                    Ok(index * 13)
                },
                |index, value| {
                    output.push((index, value));
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(output, (0..17).map(|i| (i, i * 13)).collect::<Vec<_>>());
        }
    }
    #[test]
    fn worker_selection_respects_cpu_memory_dimensions_and_explicit_limit() {
        assert_eq!(choose_workers(1920, 960, 32, 12, Some(16 * GIB), None), 6);
        assert_eq!(choose_workers(1920, 960, 32, 12, Some(8 * GIB), None), 2);
        assert_eq!(choose_workers(1920, 960, 32, 12, Some(2 * GIB), None), 1);
        assert_eq!(choose_workers(1920, 960, 32, 12, None, Some(6)), 2);
        assert_eq!(
            choose_workers(1920, 960, 32, 12, Some(16 * GIB), Some(1)),
            1
        );
        assert_eq!(
            choose_workers(1920, 960, 32, 4, Some(64 * GIB), Some(100)),
            2
        );
        assert_eq!(choose_workers(3840, 2160, 32, 12, Some(64 * GIB), None), 1);
        assert_eq!(choose_workers(1920, 960, 1, 12, Some(64 * GIB), None), 1);
    }
    #[test]
    fn errors_cancellation_and_panics_release_blocked_producers() {
        let cancel = AtomicBool::new(false);
        assert!(
            ordered(
                1000,
                2,
                &cancel,
                |_, i| Ok(i),
                |_, _| Err("encoder failed".into())
            )
            .unwrap_err()
            .contains("encoder")
        );
        assert!(
            ordered(
                1000,
                2,
                &cancel,
                |_, i| if i == 3 {
                    Err("frame failed".into())
                } else {
                    Ok(i)
                },
                |_, _| Ok(())
            )
            .unwrap_err()
            .contains("frame failed")
        );
        assert!(
            ordered(
                1000,
                2,
                &cancel,
                |_, i| {
                    if i == 3 {
                        panic!("fixture");
                    }
                    Ok(i)
                },
                |_, _| Ok(())
            )
            .is_err()
        );
        assert!(
            ordered(
                1000,
                2,
                &cancel,
                |_, i| Ok(i),
                |_, _| {
                    cancel.store(true, Ordering::Relaxed);
                    Ok(())
                }
            )
            .unwrap_err()
            .contains("canceled")
        );
    }
}
