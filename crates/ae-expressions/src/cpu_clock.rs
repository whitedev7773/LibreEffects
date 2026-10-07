//! Measure work on the evaluator's calling thread without charging descheduling.
//!
//! Linux/macOS and Windows use native thread CPU clocks. Other targets explicitly
//! report a monotonic wall-clock fallback. A native clock failure is an error,
//! never permission to switch clocks. This module does not choose any budgets.

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CpuClockKind {
    ThreadCpu,
    WallFallback,
}

pub(crate) struct CpuClock {
    started: platform::Stamp,
    last_sample: Cell<platform::Stamp>,
    // Native clocks sample the current thread each time. Prevent both moving the
    // clock to another thread and sharing it across threads, without allocating.
    _same_thread: PhantomData<Rc<()>>,
}

impl CpuClock {
    pub(crate) fn start() -> Result<Self, String> {
        Self::start_with(platform::sample())
    }

    pub(crate) fn elapsed(&self) -> Result<Duration, String> {
        self.elapsed_with(platform::sample())
    }

    pub(crate) fn kind(&self) -> CpuClockKind {
        if cfg!(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows"
        )) {
            CpuClockKind::ThreadCpu
        } else {
            CpuClockKind::WallFallback
        }
    }

    fn start_with(sample: Result<platform::Stamp, String>) -> Result<Self, String> {
        let started = sample?;
        Ok(Self {
            started,
            last_sample: Cell::new(started),
            _same_thread: PhantomData,
        })
    }

    fn elapsed_with(&self, sample: Result<platform::Stamp, String>) -> Result<Duration, String> {
        let sample = sample?;
        if sample < self.last_sample.get() {
            return Err("Expression execution clock moved backwards".into());
        }
        let elapsed = platform::duration_between(sample, self.started)
            .ok_or("Expression execution clock moved backwards")?;
        self.last_sample.set(sample);
        Ok(elapsed)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod platform {
    use super::{Duration, timespec_duration};
    use std::mem::MaybeUninit;

    pub(super) type Stamp = Duration;
    pub(super) fn sample() -> Result<Stamp, String> {
        let mut value = MaybeUninit::<libc::timespec>::uninit();
        // SAFETY: value is a writable, correctly aligned timespec. This fixed
        // clock ID samples only the calling thread; it changes no clock state.
        let status =
            unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()) };
        if status != 0 {
            return Err(format!(
                "Could not read expression thread CPU clock: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: a successful clock_gettime initialized both timespec fields.
        let value = unsafe { value.assume_init() };
        timespec_duration(i128::from(value.tv_sec), i128::from(value.tv_nsec))
    }

    pub(super) fn duration_between(later: Stamp, earlier: Stamp) -> Option<Duration> {
        later.checked_sub(earlier)
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{Duration, filetime_ticks, windows_cpu_duration};
    use std::mem::MaybeUninit;
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

    pub(super) type Stamp = Duration;
    pub(super) fn sample() -> Result<Stamp, String> {
        let mut creation = MaybeUninit::<FILETIME>::uninit();
        let mut exit = MaybeUninit::<FILETIME>::uninit();
        let mut kernel = MaybeUninit::<FILETIME>::uninit();
        let mut user = MaybeUninit::<FILETIME>::uninit();
        // SAFETY: all outputs point to distinct, writable, aligned FILETIMEs.
        // GetCurrentThread returns a calling-thread pseudo handle, requiring no
        // close and stored nowhere. GetThreadTimes only reads thread accounting.
        let status = unsafe {
            GetThreadTimes(
                GetCurrentThread(),
                creation.as_mut_ptr(),
                exit.as_mut_ptr(),
                kernel.as_mut_ptr(),
                user.as_mut_ptr(),
            )
        };
        if status == 0 {
            return Err(format!(
                "Could not read expression thread CPU clock: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: success initializes kernel/user time. The current thread's
        // exit time is undefined, so that output is intentionally never read.
        let (kernel, user) = unsafe { (kernel.assume_init(), user.assume_init()) };
        windows_cpu_duration(
            filetime_ticks(kernel.dwLowDateTime, kernel.dwHighDateTime),
            filetime_ticks(user.dwLowDateTime, user.dwHighDateTime),
        )
    }

    pub(super) fn duration_between(later: Stamp, earlier: Stamp) -> Option<Duration> {
        later.checked_sub(earlier)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::Duration;
    use std::time::Instant;

    pub(super) type Stamp = Instant;
    pub(super) fn sample() -> Result<Stamp, String> {
        Ok(Instant::now())
    }

    pub(super) fn duration_between(later: Stamp, earlier: Stamp) -> Option<Duration> {
        later.checked_duration_since(earlier)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn timespec_duration(seconds: i128, nanoseconds: i128) -> Result<Duration, String> {
    let seconds =
        u64::try_from(seconds).map_err(|_| "Invalid seconds in expression thread CPU clock")?;
    let nanoseconds = u32::try_from(nanoseconds)
        .ok()
        .filter(|value| *value < 1_000_000_000)
        .ok_or("Invalid nanoseconds in expression thread CPU clock")?;
    Ok(Duration::new(seconds, nanoseconds))
}

#[cfg(any(target_os = "windows", test))]
fn filetime_ticks(low: u32, high: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

#[cfg(any(target_os = "windows", test))]
fn windows_cpu_duration(kernel_ticks: u64, user_ticks: u64) -> Result<Duration, String> {
    const TICKS_PER_SECOND: u64 = 10_000_000;
    let ticks = kernel_ticks
        .checked_add(user_ticks)
        .ok_or("Expression thread CPU clock overflow")?;
    let seconds = ticks / TICKS_PER_SECOND;
    // The remainder is below 10,000,000, so conversion and multiplication are
    // bounded. Keep them checked to preserve that invariant if units change.
    let nanoseconds = u32::try_from(ticks % TICKS_PER_SECOND)
        .ok()
        .and_then(|value| value.checked_mul(100))
        .ok_or("Expression thread CPU clock overflow")?;
    Ok(Duration::new(seconds, nanoseconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_timespec_ranges_without_normalizing_bad_samples() {
        assert_eq!(timespec_duration(2, 34).unwrap(), Duration::new(2, 34));
        assert_eq!(
            timespec_duration(i128::from(u64::MAX), 999_999_999).unwrap(),
            Duration::MAX
        );
        for (seconds, nanoseconds) in [
            (-1, 0),
            (i128::from(u64::MAX) + 1, 0),
            (0, -1),
            (0, 1_000_000_000),
            (0, i128::MAX),
        ] {
            assert!(timespec_duration(seconds, nanoseconds).is_err());
        }
    }

    #[test]
    fn combines_filetime_words_without_truncation() {
        assert_eq!(
            filetime_ticks(0x7654_3210, 0xfedc_ba98),
            0xfedc_ba98_7654_3210
        );
        assert_eq!(filetime_ticks(u32::MAX, u32::MAX), u64::MAX);
    }

    #[test]
    fn sums_windows_cpu_times_and_checks_overflow() {
        assert_eq!(
            windows_cpu_duration(8_000_000, 12_000_003).unwrap(),
            Duration::new(2, 300)
        );
        assert_eq!(
            windows_cpu_duration(u64::MAX, 0).unwrap(),
            Duration::new(1_844_674_407_370, 955_161_500)
        );
        assert!(windows_cpu_duration(u64::MAX, 1).is_err());
    }

    #[test]
    fn sample_failures_are_returned_without_clock_fallback() {
        let failure = "synthetic native clock failure".to_string();
        assert!(matches!(
            CpuClock::start_with(Err(failure.clone())),
            Err(error) if error == failure
        ));
        let clock = CpuClock::start().unwrap();
        let previous = clock.last_sample.get();
        let kind = clock.kind();
        assert_eq!(clock.elapsed_with(Err(failure.clone())), Err(failure));
        assert_eq!(clock.last_sample.get(), previous);
        assert_eq!(clock.kind(), kind);
    }

    #[test]
    fn rejects_regression_between_samples_even_above_start() {
        let start = platform::sample().unwrap();
        let first = start.checked_add(Duration::from_secs(2)).unwrap();
        let regressed = start.checked_add(Duration::from_secs(1)).unwrap();
        let clock = CpuClock::start_with(Ok(start)).unwrap();
        assert_eq!(clock.elapsed_with(Ok(start)).unwrap(), Duration::ZERO);
        assert_eq!(
            clock.elapsed_with(Ok(first)).unwrap(),
            Duration::from_secs(2)
        );
        assert!(clock.elapsed_with(Ok(regressed)).is_err());
        assert_eq!(clock.last_sample.get(), first);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_native_thread_cpu_clock_smoke() {
        let clock = CpuClock::start().unwrap();
        assert_eq!(clock.kind(), CpuClockKind::ThreadCpu);
        let before = clock.elapsed().unwrap();
        let mut value = 1_u64;
        for index in 1..100_000_u64 {
            value = std::hint::black_box(value.wrapping_mul(31).wrapping_add(index));
        }
        std::hint::black_box(value);
        let after = clock.elapsed().unwrap();
        assert!(after > before);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    #[test]
    fn identifies_unsupported_target_wall_fallback() {
        assert_eq!(
            CpuClock::start().unwrap().kind(),
            CpuClockKind::WallFallback
        );
    }
}
