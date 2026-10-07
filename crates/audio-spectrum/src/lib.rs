//! Bounded, stateless-in-time spectral analysis for finite 48 kHz stereo PCM.
//!
//! [`SpectrumAnalysisProfile::NativeV1`] defines Libre Effects behavior. It is
//! not a recovered or calibrated implementation of another application's
//! spectrum effect. See the crate README for its normalization and window
//! contract. Reusing [`SpectrumAnalyzer`] changes allocations, never results.

#![forbid(unsafe_code)]

mod fft;

use std::{
    error::Error,
    fmt,
    mem::size_of,
    sync::atomic::{AtomicBool, Ordering},
};

use fft::Complex;

pub const SAMPLE_RATE: u32 = 48_000;
pub const MIN_INPUT_FRAMES: usize = 48;
pub const MAX_INPUT_FRAMES: usize = 48_000;
pub const MAX_FFT_LEN: usize = 65_536;
pub const MAX_BANDS: u16 = 4_096;
pub const MIN_DURATION_MS: f64 = 1.0;
pub const MAX_DURATION_MS: f64 = 1_000.0;
pub const NYQUIST_HZ: f64 = 24_000.0;

/// The algorithm identity, independent of the package's release version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpectrumAnalysisProfile {
    NativeV1,
}

/// A single native analysis request. Input sample rate is always [`SAMPLE_RATE`].
///
/// The caller supplies exactly `round(duration_ms * 48)` stereo frames from a
/// centered, half-open source window, including any source-boundary zero pads.
/// For one band, only `start_hz` is sampled; `end_hz` must still be valid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpectrumAnalysisSpec {
    pub duration_ms: f64,
    pub start_hz: f64,
    pub end_hz: f64,
    pub bands: u16,
}

impl SpectrumAnalysisSpec {
    /// Validate all fields and estimate work without allocating or reading PCM.
    pub fn estimate_work(&self) -> Result<SpectrumWorkEstimate, SpectrumError> {
        if !self.duration_ms.is_finite()
            || !(MIN_DURATION_MS..=MAX_DURATION_MS).contains(&self.duration_ms)
        {
            return Err(SpectrumError::InvalidDuration);
        }
        if !self.start_hz.is_finite()
            || !self.end_hz.is_finite()
            || self.start_hz < 0.0
            || self.start_hz > self.end_hz
            || self.end_hz > NYQUIST_HZ
        {
            return Err(SpectrumError::InvalidFrequencyRange);
        }
        if !(1..=MAX_BANDS).contains(&self.bands) {
            return Err(SpectrumError::InvalidBandCount);
        }

        // Positive bounded duration makes round()'s half-away-from-zero rule
        // exactly the declared nearest-frame rule, with half frames rounded up.
        let input_frames = (self.duration_ms * (f64::from(SAMPLE_RATE) / 1_000.0)).round() as usize;
        if !(MIN_INPUT_FRAMES..=MAX_INPUT_FRAMES).contains(&input_frames) {
            return Err(SpectrumError::InvalidDuration);
        }
        let fft_len = input_frames
            .checked_next_power_of_two()
            .ok_or(SpectrumError::NumericRange)?;
        if fft_len > MAX_FFT_LEN {
            return Err(SpectrumError::NumericRange);
        }
        let output_bands = usize::from(self.bands);
        let scratch_bytes = fft_len
            .checked_mul(2 * size_of::<Complex>())
            .ok_or(SpectrumError::NumericRange)?;
        let output_bytes = output_bands
            .checked_mul(size_of::<f64>())
            .ok_or(SpectrumError::NumericRange)?;
        let allocation_bytes = scratch_bytes
            .checked_add(output_bytes)
            .ok_or(SpectrumError::NumericRange)?;
        // Two transforms each do (L / 2) * log2(L) butterflies.
        let fft_butterflies = (fft_len as u64)
            .checked_mul(u64::from(fft_len.ilog2()))
            .ok_or(SpectrumError::NumericRange)?;
        // Scheduling units, not elapsed time or a claim about exact FLOPs.
        // Includes zeroing two buffers, stereo input validation/windowing,
        // two bit-reversal passes, both FFTs and two-bin stereo output sampling.
        let work_units = (input_frames as u64)
            .checked_mul(4)
            .and_then(|v| v.checked_add((fft_len as u64).checked_mul(4)?))
            .and_then(|v| v.checked_add(fft_butterflies.checked_mul(8)?))
            .and_then(|v| v.checked_add((output_bands as u64).checked_mul(16)?))
            .ok_or(SpectrumError::NumericRange)?;
        Ok(SpectrumWorkEstimate {
            input_frames,
            fft_len,
            fft_butterflies,
            output_bands,
            scratch_bytes,
            output_bytes,
            allocation_bytes,
            work_units,
        })
    }
}

/// Per-request requirements for admission to a caller's shared per-frame budget.
///
/// Byte counts are element storage, excluding the small fixed stack values and
/// allocator bookkeeping. They include both channel transforms and the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpectrumWorkEstimate {
    pub input_frames: usize,
    pub fft_len: usize,
    pub fft_butterflies: u64,
    pub output_bands: usize,
    pub scratch_bytes: usize,
    pub output_bytes: usize,
    pub allocation_bytes: usize,
    pub work_units: u64,
}

impl SpectrumWorkEstimate {
    pub fn check_limits(&self, limits: &SpectrumLimits) -> Result<(), SpectrumError> {
        if self.work_units > limits.max_work_units {
            return Err(SpectrumError::WorkBudgetExceeded);
        }
        if self.allocation_bytes > limits.max_allocation_bytes {
            return Err(SpectrumError::MemoryBudgetExceeded);
        }
        Ok(())
    }
}

/// Local request ceilings. The caller must also charge shared frame/session work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpectrumLimits {
    pub max_work_units: u64,
    pub max_allocation_bytes: usize,
}

impl Default for SpectrumLimits {
    fn default() -> Self {
        Self {
            max_work_units: MAX_INPUT_FRAMES as u64 * 4
                + MAX_FFT_LEN as u64 * 4
                + MAX_FFT_LEN as u64 * 16 * 8
                + MAX_BANDS as u64 * 16,
            max_allocation_bytes: MAX_FFT_LEN * 2 * size_of::<Complex>()
                + usize::from(MAX_BANDS) * size_of::<f64>(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpectrumFrame {
    pub profile: SpectrumAnalysisProfile,
    /// Finite nonnegative linear amplitudes. Values above one are preserved.
    pub amplitudes: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpectrumError {
    InvalidDuration,
    InvalidFrequencyRange,
    InvalidBandCount,
    InputLength { expected: usize, actual: usize },
    NonFiniteInput { frame: usize, channel: usize },
    WorkBudgetExceeded,
    MemoryBudgetExceeded,
    AllocationFailed,
    NumericRange,
    Cancelled,
}

impl fmt::Display for SpectrumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDuration => {
                f.write_str("spectrum duration must be finite and in 1..=1000 milliseconds")
            }
            Self::InvalidFrequencyRange => f.write_str(
                "spectrum frequencies must be finite and satisfy 0 <= start <= end <= 24000 Hz",
            ),
            Self::InvalidBandCount => f.write_str("spectrum band count must be in 1..=4096"),
            Self::InputLength { expected, actual } => write!(
                f,
                "spectrum needs {expected} stereo frames, received {actual}"
            ),
            Self::NonFiniteInput { frame, channel } => write!(
                f,
                "spectrum input frame {frame}, channel {channel} is not finite"
            ),
            Self::WorkBudgetExceeded => f.write_str("spectrum work budget exceeded"),
            Self::MemoryBudgetExceeded => f.write_str("spectrum memory budget exceeded"),
            Self::AllocationFailed => f.write_str("spectrum buffer allocation failed"),
            Self::NumericRange => f.write_str("spectrum calculation exceeds finite numeric range"),
            Self::Cancelled => f.write_str("spectrum analysis cancelled"),
        }
    }
}

impl Error for SpectrumError {}

/// Reusable bounded buffers; there is no temporal history or coefficient cache.
#[derive(Default)]
pub struct SpectrumAnalyzer {
    left: Vec<Complex>,
    right: Vec<Complex>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyze finite stereo PCM without clipping, gain control or smoothing.
    ///
    /// Validation and resource admission happen before buffer allocation. A
    /// failed/cancelled request returns no frame; future requests clear scratch.
    pub fn analyze(
        &mut self,
        stereo: &[[f32; 2]],
        spec: &SpectrumAnalysisSpec,
        limits: &SpectrumLimits,
        cancel: &AtomicBool,
    ) -> Result<SpectrumFrame, SpectrumError> {
        self.analyze_checked(stereo, spec, limits, &mut |_| {
            if cancel.load(Ordering::Relaxed) {
                Err(SpectrumError::Cancelled)
            } else {
                Ok(())
            }
        })
    }

    fn analyze_checked(
        &mut self,
        stereo: &[[f32; 2]],
        spec: &SpectrumAnalysisSpec,
        limits: &SpectrumLimits,
        check: &mut impl FnMut(CancelPoint) -> Result<(), SpectrumError>,
    ) -> Result<SpectrumFrame, SpectrumError> {
        check(CancelPoint::Admission)?;
        let work = spec.estimate_work()?;
        work.check_limits(limits)?;
        if stereo.len() != work.input_frames {
            return Err(SpectrumError::InputLength {
                expected: work.input_frames,
                actual: stereo.len(),
            });
        }
        // Reject nonfinite input before any allocation, including samples at
        // the Hann window's zero endpoint that might otherwise hide an error.
        for (frame, sample) in stereo.iter().enumerate() {
            if frame % 256 == 0 {
                check(CancelPoint::Input)?;
            }
            for (channel, value) in sample.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SpectrumError::NonFiniteInput { frame, channel });
                }
            }
        }
        check(CancelPoint::Allocation)?;
        // A smaller requested memory ceiling also applies to retained buffers.
        // Release both before growing, so resizing never holds two generations.
        if self.left.capacity() < work.fft_len
            || self.right.capacity() < work.fft_len
            || self
                .retained_bytes()
                .checked_add(work.output_bytes)
                .is_none_or(|bytes| bytes > limits.max_allocation_bytes)
        {
            self.left = Vec::new();
            self.right = Vec::new();
        }
        reserve(&mut self.left, work.fft_len)?;
        reserve(&mut self.right, work.fft_len)?;
        if self
            .retained_bytes()
            .checked_add(work.output_bytes)
            .is_none_or(|bytes| bytes > limits.max_allocation_bytes)
        {
            self.left = Vec::new();
            self.right = Vec::new();
            return Err(SpectrumError::MemoryBudgetExceeded);
        }
        let mut amplitudes = Vec::new();
        reserve(&mut amplitudes, work.output_bands)?;
        if self
            .retained_bytes()
            .checked_add(amplitudes.capacity() * size_of::<f64>())
            .is_none_or(|bytes| bytes > limits.max_allocation_bytes)
        {
            return Err(SpectrumError::MemoryBudgetExceeded);
        }
        self.left.clear();
        self.left.resize(work.fft_len, Complex::ZERO);
        check(CancelPoint::Window)?;
        self.right.clear();
        self.right.resize(work.fft_len, Complex::ZERO);
        for (index, sample) in stereo.iter().enumerate() {
            if index % 256 == 0 {
                check(CancelPoint::Window)?;
            }
            let window =
                0.5 - 0.5 * (std::f64::consts::TAU * index as f64 / work.input_frames as f64).cos();
            self.left[index].re = f64::from(sample[0]) * window;
            self.right[index].re = f64::from(sample[1]) * window;
        }
        fft::transform(&mut self.left, check)?;
        fft::transform(&mut self.right, check)?;
        // A periodic Hann's coherent gain is exactly N/2. Padding does not
        // change that gain: normalize by the real window, not by FFT length.
        let coherent_sum = work.input_frames as f64 * 0.5;
        for index in 0..work.output_bands {
            check(CancelPoint::Output)?;
            let frequency = if index == 0 {
                spec.start_hz
            } else if index == work.output_bands - 1 {
                spec.end_hz
            } else {
                spec.start_hz
                    + (spec.end_hz - spec.start_hz)
                        * (index as f64 / (work.output_bands - 1) as f64)
            };
            let position = frequency * work.fft_len as f64 / f64::from(SAMPLE_RATE);
            let low = (position.floor() as usize).min(work.fft_len / 2);
            let high = (low + 1).min(work.fft_len / 2);
            let fraction = position - low as f64;
            let lower = self.bin_amplitude(low, coherent_sum);
            let upper = self.bin_amplitude(high, coherent_sum);
            let amplitude = lower + (upper - lower) * fraction;
            if !amplitude.is_finite() || amplitude < 0.0 {
                return Err(SpectrumError::NumericRange);
            }
            amplitudes.push(amplitude);
        }
        check(CancelPoint::Complete)?;
        Ok(SpectrumFrame {
            profile: SpectrumAnalysisProfile::NativeV1,
            amplitudes,
        })
    }

    fn retained_bytes(&self) -> usize {
        (self.left.capacity() + self.right.capacity()) * size_of::<Complex>()
    }

    fn bin_amplitude(&self, index: usize, coherent_sum: f64) -> f64 {
        let factor = if index == 0 || index == self.left.len() / 2 {
            1.0
        } else {
            2.0
        };
        let left = self.left[index].magnitude() * factor / coherent_sum;
        let right = self.right[index].magnitude() * factor / coherent_sum;
        ((left * left + right * right) * 0.5).sqrt()
    }
}

/// One-shot analysis with the maximum native per-request limits.
/// Use [`SpectrumAnalyzer`] when repeated calls should reuse FFT buffers.
pub fn analyze_spectrum(
    stereo: &[[f32; 2]],
    spec: &SpectrumAnalysisSpec,
    cancel: &AtomicBool,
) -> Result<SpectrumFrame, SpectrumError> {
    SpectrumAnalyzer::new().analyze(stereo, spec, &SpectrumLimits::default(), cancel)
}

fn reserve<T>(buffer: &mut Vec<T>, len: usize) -> Result<(), SpectrumError> {
    if buffer.capacity() < len {
        buffer
            .try_reserve_exact(len - buffer.len())
            .map_err(|_| SpectrumError::AllocationFailed)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CancelPoint {
    Admission,
    Input,
    Allocation,
    Window,
    Permutation,
    FftStage,
    Output,
    Complete,
}

#[cfg(test)]
mod tests;
