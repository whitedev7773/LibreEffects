# Native stereo spectrum analysis

This dependency implements explicitly named `NativeV1` and `HammingV1` profiles.
Neither establishes complete After Effects compatibility. It has no project, decoder, file, geometry,
wall-clock, playback, or preceding-frame dependency. There are no dependencies
outside Rust's standard library, and unsafe code is forbidden.

## Signal contract

1. Input is finite stereo `f32` PCM at exactly 48,000 samples per second. The
   duration must be finite and in 1–1,000 milliseconds. The input length must
   equal `round(duration_ms * 48)`, with positive half frames rounded up: 48 to
   48,000 frames. The caller supplies a centered, half-open window and source
   boundary zero padding. For center `t + offset` and rounded frame count `N`,
   sample `i` is at `t + offset - N / (2 * 48000) + i / 48000`. The analyzer
   cannot verify sample rate, source times, or zero padding from the PCM alone.
2. Each channel receives the periodic Hann window
   `w[i] = 0.5 - 0.5*cos(2*pi*i/N)` for `i = 0..N-1`. Both channels are padded
   with zeros to `L = next_power_of_two(N)`, at most 65,536. Band count never
   determines transform length. Each channel has a separate scalar, radix-2,
   forward, unnormalized, complex `f64` FFT.
3. Each channel's one-sided bin amplitude is `abs(FFT[k]) / (N/2)` at DC and
   Nyquist and `2*abs(FFT[k]) / (N/2)` at interior bins. The denominator is the
   Hann window's coherent sum, not the padded FFT length. The stereo amplitude
   at each bin is `sqrt((left_amplitude^2 + right_amplitude^2)/2)`. Antiphase
   stereo therefore retains its magnitude; a signal in one channel is scaled
   by `1/sqrt(2)` relative to that same signal in both channels.
4. The finite frequencies satisfy `0 <= start_hz <= end_hz <= 24000`. There
   are 1–4,096 output bands. One band samples `start_hz`. Otherwise band `i`
   samples `start_hz + (end_hz-start_hz)*i/(bands-1)`, including both endpoints.
   The result linearly interpolates the adjacent *combined bin magnitudes*.
   It does not interpolate complex FFT values or combine channels after
   frequency interpolation.
5. Output has exactly `bands` finite, nonnegative `f64` linear amplitudes and
   the `NativeV1` identity. Gain above one survives. There is no clipping, AGC,
   dB conversion/floor, track normalization, or temporal averaging. The caller
   owns height limits and any visualization.

For example, a 90 ms input is 4,320 frames and uses an 8,192-point FFT. Adding
output bands increases display sampling density, not the window's frequency
resolution. Hann coherent gain normalizes a bin-centered sinusoid; arbitrary
frequencies retain the window's leakage/scalloping and are not peak-corrected.

## HammingV1

`estimate_profile_work` and `analyze_profile` select this opt-in profile. Input,
one-sided endpoint factors, finite-result rules and
resource ceilings are shared with NativeV1. The differences are:

- The periodic window is `0.54 - 0.46*cos(2*pi*i/N)`.
- Stereo uses the arithmetic mean of both channels before taking magnitude.
  Independent AE captures confirm antiphase cancellation, a half-height
  one-channel signal, and the intermediate height of unequal channels.
- Bin magnitudes divide by `N`, preserving the window's amplitude attenuation;
  a bin-centered sine of amplitude 0.5 in both channels produces 0.27.
- Band `i` samples `start + (end-start)*i/bands`, excluding the end frequency.
- The FFT uses `min(4*next_power_of_two(N), 65536)` for denser interpolation.

Independent AE digital-bar captures of 100/400 Hz tones at 60/90/120/180 ms
motivate this profile. A continuous Hamming DFT predicts 1,031 of 1,032 measured
integer bar heights exactly and the remaining one within a pixel. That evidence
does not recover AE's internal transform, paint or all source
sampling rules. HammingV1 retains an explicit deterministic native contract.
NativeV1 remains the default and its coefficients are unchanged.

## Admission, allocation, and cancellation

`SpectrumAnalysisSpec::estimate_work()` validates the descriptor without reading
PCM or allocating. Its `input_frames`, `fft_len`, `fft_butterflies`, output band
count, byte counts, and scheduling `work_units` are available to a caller's
shared frame budget. Both channel FFTs are included: `L*log2(L)` butterflies.
The scheduling formula is `4*N + 4*L + 8*butterflies + 16*bands`; it accounts for
stereo input validation/windowing, both zero/reorder passes, both FFTs, and
two-bin stereo output sampling. These are conservative scheduling weights, not
measured running time or exact floating-point operation counts.

`SpectrumAnalyzer::analyze` checks `SpectrumLimits` and exact input length,
rejects nonfinite PCM, and uses fallible reservations. Default limits admit the
largest supported descriptor. Reusable scratch is two `L`-element complex
buffers; each element is two `f64` values. Output owns `bands` `f64` values.
`allocation_bytes` is their combined element storage, at most 2,129,920 bytes.
It excludes caller-owned input, other retained output frames, fixed stack
values, allocator bookkeeping, and the hosting application's caches. The host
must bound those separately, along with concurrent jobs and total frame work.

Only scratch is retained across calls. Growing the scratch releases both old
buffers before reserving replacements. A tighter memory limit also releases
overlarge retained buffers. Actual reserved capacities are checked against the
limit; failure is reported rather than silently reducing the FFT or band count.
`analyze_spectrum` is a convenience for a one-shot request with default limits.

Cancellation is read before admission, during input validation/windowing,
between scratch passes, during bit reversal, before every FFT stage, before
every output band, and before returning the complete frame. A cancelled or
failed request yields no partial result. Scratch is fully initialized on the
next request, so it cannot leak stale coefficients. Error context about the
source, layer, effect, and sample time belongs to the caller.

## Determinism and verification

On a fixed build/platform, identical finite input and settings produce the same
coefficients regardless of prior requests or buffer capacities. The algorithm
uses a fixed scalar order and no parallel reduction. Rust platform trigonometry
and floating-point implementation can vary, so cross-platform bit identity is
not promised. Synthetic amplitude tests use `2e-6 * max(1, abs(expected))`
absolute tolerance, accounting for `f32` input quantization. Same-build reuse
tests require exact equality.

Tests cover two known amplitudes and Hann side lobes, silence, DC and Nyquist,
antiphase and unequal channels, interpolation, one/max band counts, duration
rounding, non-power-of-two input against an independent direct DFT, descriptor
and PCM rejection, work/memory admission, deterministic cancellation during
both transforms/output, scratch recovery, request-order independence, and
finite `f32` extremes. All samples are generated synthetically.

Run `cargo test -p libre-effects-audio-spectrum` from the workspace root.
