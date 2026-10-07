use super::*;

fn spec(frames: usize, start_hz: f64, end_hz: f64, bands: u16) -> SpectrumAnalysisSpec {
    SpectrumAnalysisSpec {
        duration_ms: frames as f64 / 48.0,
        start_hz,
        end_hz,
        bands,
    }
}

fn tone(frames: usize, bin: usize, left: f64, right: f64) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|index| {
            let phase = (std::f64::consts::TAU * bin as f64 * index as f64 / frames as f64).cos();
            [(left * phase) as f32, (right * phase) as f32]
        })
        .collect()
}

fn analyze(input: &[[f32; 2]], spec: &SpectrumAnalysisSpec) -> SpectrumFrame {
    analyze_spectrum(input, spec, &AtomicBool::new(false)).unwrap()
}

fn close(actual: f64, expected: f64) {
    let tolerance = 2.0e-6 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn hamming_profile_matches_independent_dft_with_exclusive_end_and_raw_window_gain() {
    let input: Vec<[f32; 2]> = (0..97)
        .map(|i| {
            [
                (i as f64 * 0.17).sin() as f32,
                (i as f64 * 0.31).cos() as f32,
            ]
        })
        .collect();
    let request = spec(input.len(), 187.5, 937.5, 4);
    let result = SpectrumAnalyzer::new()
        .analyze_profile(
            &input,
            &request,
            SpectrumAnalysisProfile::HammingV1,
            &SpectrumLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(result.profile, SpectrumAnalysisProfile::HammingV1);
    for (band, actual) in result.amplitudes.iter().enumerate() {
        // All frequencies fall on the independently known 512-point FFT bins;
        // compute the unpadded DFT directly, with separate stereo magnitudes.
        let frequency = 187.5 + 187.5 * band as f64;
        let mut coefficients = [[0.0; 2]; 2];
        for channel in 0..2 {
            let (mut real, mut imaginary) = (0.0, 0.0);
            for (i, frame) in input.iter().enumerate() {
                let window = 0.54 - 0.46 * (std::f64::consts::TAU * i as f64 / 97.0).cos();
                let phase = std::f64::consts::TAU * frequency * i as f64 / 48000.0;
                real += f64::from(frame[channel]) * window * phase.cos();
                imaginary -= f64::from(frame[channel]) * window * phase.sin();
            }
            coefficients[channel] = [real, imaginary];
        }
        close(
            *actual,
            (coefficients[0][0] + coefficients[1][0])
                .hypot(coefficients[0][1] + coefficients[1][1])
                / 97.0,
        );
    }
    let frames = 4096;
    let request = spec(frames, 234.375, 234.375, 1);
    for (left, right, expected) in [
        (0.5, 0.5, 0.27),
        (0.5, -0.5, 0.0),
        (0.5, 0.0, 0.135),
        (0.5, 0.25, 0.2025),
    ] {
        let input = tone(frames, 20, left, right);
        let result = SpectrumAnalyzer::new()
            .analyze_profile(
                &input,
                &request,
                SpectrumAnalysisProfile::HammingV1,
                &SpectrumLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        close(result.amplitudes[0], expected);
    }
}

#[test]
fn hamming_admission_cancellation_and_reuse_preserve_native_profile_identity() {
    let request = spec(4320, 20.0, 800.0, 1920);
    let work = request
        .estimate_profile_work(SpectrumAnalysisProfile::HammingV1)
        .unwrap();
    assert_eq!(work.input_frames, 4320);
    assert_eq!(work.fft_len, 32768);
    work.check_limits(&SpectrumLimits::default()).unwrap();
    let maximum = spec(48000, 0.0, 24000.0, 4096)
        .estimate_profile_work(SpectrumAnalysisProfile::HammingV1)
        .unwrap();
    assert_eq!(maximum.fft_len, MAX_FFT_LEN);
    maximum.check_limits(&SpectrumLimits::default()).unwrap();
    let input = tone(4320, 9, 0.5, 0.5);
    let mut analyzer = SpectrumAnalyzer::new();
    let native = analyzer
        .analyze(
            &input,
            &request,
            &SpectrumLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    let limits = SpectrumLimits {
        max_work_units: work.work_units - 1,
        ..Default::default()
    };
    assert_eq!(
        analyzer.analyze_profile(
            &input,
            &request,
            SpectrumAnalysisProfile::HammingV1,
            &limits,
            &AtomicBool::new(false)
        ),
        Err(SpectrumError::WorkBudgetExceeded)
    );
    assert_eq!(
        analyzer.analyze_profile_checked(
            &input,
            &request,
            SpectrumAnalysisProfile::HammingV1,
            &SpectrumLimits::default(),
            &mut |point| if matches!(point, CancelPoint::FftStage) {
                Err(SpectrumError::Cancelled)
            } else {
                Ok(())
            }
        ),
        Err(SpectrumError::Cancelled)
    );
    let hamming = analyzer
        .analyze_profile(
            &input,
            &request,
            SpectrumAnalysisProfile::HammingV1,
            &SpectrumLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_ne!(native.amplitudes, hamming.amplitudes);
    assert_eq!(
        analyzer
            .analyze(
                &input,
                &request,
                &SpectrumLimits::default(),
                &AtomicBool::new(false)
            )
            .unwrap(),
        native
    );
}

#[test]
fn periodic_hann_bin_tones_preserve_two_amplitudes_and_neighbour_lobes() {
    let frames = 2_048;
    let bin = 31;
    let spacing = f64::from(SAMPLE_RATE) / frames as f64;
    for amplitude in [0.125, 2.75] {
        let result = analyze(
            &tone(frames, bin, amplitude, amplitude),
            &spec(
                frames,
                (bin - 2) as f64 * spacing,
                (bin + 2) as f64 * spacing,
                5,
            ),
        );
        assert_eq!(result.profile, SpectrumAnalysisProfile::NativeV1);
        for (actual, factor) in result.amplitudes.iter().zip([0.0, 0.5, 1.0, 0.5, 0.0]) {
            close(*actual, amplitude * factor);
        }
    }
}

#[test]
fn silence_is_exact_zero_at_all_4096_bands() {
    let result = analyze(
        &vec![[0.0, 0.0]; 48_000],
        &spec(48_000, 0.0, NYQUIST_HZ, MAX_BANDS),
    );
    assert_eq!(result.amplitudes.len(), usize::from(MAX_BANDS));
    assert!(result.amplitudes.iter().all(|value| value.to_bits() == 0));
}

#[test]
fn dc_and_nyquist_are_not_doubled() {
    let frames = 1_024;
    for amplitude in [0.5, 3.25] {
        let dc = analyze(
            &vec![[amplitude as f32; 2]; frames],
            &spec(frames, 0.0, NYQUIST_HZ, 2),
        );
        close(dc.amplitudes[0], amplitude);
        close(dc.amplitudes[1], 0.0);
        let nyquist = analyze(
            &tone(frames, frames / 2, amplitude, amplitude),
            &spec(frames, 0.0, NYQUIST_HZ, 2),
        );
        close(nyquist.amplitudes[0], 0.0);
        close(nyquist.amplitudes[1], amplitude);
    }
}

#[test]
fn stereo_rms_preserves_antiphase_and_unequal_channel_energy() {
    let frames = 2_048;
    let bin = 19;
    let frequency = bin as f64 * f64::from(SAMPLE_RATE) / frames as f64;
    for (left, right) in [(1.5, -1.5), (0.25, 2.0), (0.0, 1.0), (2.0, 0.25)] {
        let result = analyze(
            &tone(frames, bin, left, right),
            &spec(frames, frequency, frequency, 1),
        );
        close(
            result.amplitudes[0],
            ((left * left + right * right) / 2.0).sqrt(),
        );
    }
}

#[test]
fn frequency_interpolation_is_linear_and_single_band_uses_start() {
    let frames = 1_024;
    let bin = 21;
    let spacing = f64::from(SAMPLE_RATE) / frames as f64;
    let input = tone(frames, bin, 2.0, 2.0);
    let result = analyze(
        &input,
        &spec(
            frames,
            (bin as f64 - 0.5) * spacing,
            (bin as f64 + 0.5) * spacing,
            3,
        ),
    );
    for (actual, expected) in result.amplitudes.iter().zip([1.5, 2.0, 1.5]) {
        close(*actual, expected);
    }
    let one = analyze(&input, &spec(frames, bin as f64 * spacing, NYQUIST_HZ, 1));
    close(one.amplitudes[0], 2.0);
    let repeated = analyze(
        &input,
        &spec(frames, bin as f64 * spacing, bin as f64 * spacing, 4_096),
    );
    assert_eq!(repeated.amplitudes.len(), 4_096);
    assert!(
        repeated
            .amplitudes
            .iter()
            .all(|value| *value == repeated.amplitudes[0])
    );
}

/// Independent O(N*L) direct DFT oracle, intentionally not the radix-2 code.
fn direct_dft_bins(input: &[[f32; 2]], fft_len: usize) -> Vec<f64> {
    (0..=fft_len / 2)
        .map(|bin| {
            let mut channels = [0.0; 2];
            for channel in 0..2 {
                let mut real = 0.0;
                let mut imaginary = 0.0;
                for (index, sample) in input.iter().enumerate() {
                    let window = (1.0
                        - (std::f64::consts::TAU * index as f64 / input.len() as f64).cos())
                        / 2.0;
                    let phase = -std::f64::consts::TAU * bin as f64 * index as f64 / fft_len as f64;
                    real += f64::from(sample[channel]) * window * phase.cos();
                    imaginary += f64::from(sample[channel]) * window * phase.sin();
                }
                let endpoint_factor = if bin == 0 || bin == fft_len / 2 {
                    1.0
                } else {
                    2.0
                };
                channels[channel] = (real * real + imaginary * imaginary).sqrt() * endpoint_factor
                    / (input.len() as f64 / 2.0);
            }
            ((channels[0] * channels[0] + channels[1] * channels[1]) / 2.0).sqrt()
        })
        .collect()
}

#[test]
fn padded_fft_matches_independent_dft_and_combines_channels_before_interpolation() {
    let frames = 72;
    let input: Vec<[f32; 2]> = (0..frames)
        .map(|i| {
            let position = i as f64;
            [
                (0.25 + 1.7 * (position * 0.47).cos()) as f32,
                (-0.6 + 0.8 * (position * 0.91).sin()) as f32,
            ]
        })
        .collect();
    let expected = direct_dft_bins(&input, 128);
    let result = analyze(&input, &spec(frames, 0.0, NYQUIST_HZ, 129));
    for (index, actual) in result.amplitudes.iter().enumerate() {
        let position = index as f64 / 2.0;
        let low = position.floor() as usize;
        let high = (low + 1).min(expected.len() - 1);
        close(
            *actual,
            expected[low] + (expected[high] - expected[low]) * (position - low as f64),
        );
    }
}

#[test]
fn native_window_rounding_fft_size_and_work_are_explicit() {
    let mut request = spec(4_320, 20.0, 800.0, 1_920);
    let work = request.estimate_work().unwrap();
    assert_eq!(work.input_frames, 4_320);
    assert_eq!(work.fft_len, 8_192);
    assert_eq!(work.fft_butterflies, 8_192 * 13);
    assert_eq!(work.output_bands, 1_920);
    assert_eq!(work.scratch_bytes, 2 * 8_192 * 16);
    assert_eq!(work.output_bytes, 1_920 * 8);
    assert_eq!(
        work.allocation_bytes,
        work.scratch_bytes + work.output_bytes
    );
    assert_eq!(
        work.work_units,
        4_320 * 4 + 8_192 * 4 + 8_192 * 13 * 8 + 1_920 * 16
    );
    request.duration_ms = 1.03125; // 49.5 frames, rounds to 50.
    assert_eq!(request.estimate_work().unwrap().input_frames, 50);
    request.duration_ms = 1.031;
    assert_eq!(request.estimate_work().unwrap().input_frames, 49);
    for frames in [MIN_INPUT_FRAMES, MAX_INPUT_FRAMES] {
        let work = spec(frames, 0.0, NYQUIST_HZ, MAX_BANDS)
            .estimate_work()
            .unwrap();
        assert_eq!(work.input_frames, frames);
        work.check_limits(&SpectrumLimits::default()).unwrap();
    }
}

#[test]
fn validation_rejects_nonfinite_and_out_of_range_descriptors() {
    let valid = spec(1_024, 0.0, NYQUIST_HZ, 1);
    for duration_ms in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        0.999,
        1_000.001,
    ] {
        assert_eq!(
            SpectrumAnalysisSpec {
                duration_ms,
                ..valid
            }
            .estimate_work(),
            Err(SpectrumError::InvalidDuration)
        );
    }
    for (start_hz, end_hz) in [
        (f64::NAN, 1.0),
        (0.0, f64::NAN),
        (f64::NEG_INFINITY, 1.0),
        (0.0, f64::INFINITY),
        (-0.1, 10.0),
        (2.0, 1.0),
        (0.0, 24_000.1),
    ] {
        assert_eq!(
            SpectrumAnalysisSpec {
                start_hz,
                end_hz,
                ..valid
            }
            .estimate_work(),
            Err(SpectrumError::InvalidFrequencyRange)
        );
    }
    for bands in [0, 4_097, u16::MAX] {
        assert_eq!(
            SpectrumAnalysisSpec { bands, ..valid }.estimate_work(),
            Err(SpectrumError::InvalidBandCount)
        );
    }
}

#[test]
fn wrong_length_and_nonfinite_samples_fail_before_allocation() {
    let request = spec(48, 0.0, NYQUIST_HZ, 2);
    let cancel = AtomicBool::new(false);
    let mut analyzer = SpectrumAnalyzer::new();
    for actual in [0, 47, 49, 48_001] {
        assert_eq!(
            analyzer.analyze(
                &vec![[0.0; 2]; actual],
                &request,
                &SpectrumLimits::default(),
                &cancel
            ),
            Err(SpectrumError::InputLength {
                expected: 48,
                actual
            })
        );
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for (frame, channel) in [(0, 0), (0, 1), (47, 1)] {
            let mut input = vec![[0.0; 2]; 48];
            input[frame][channel] = bad;
            assert_eq!(
                analyzer.analyze(&input, &request, &SpectrumLimits::default(), &cancel),
                Err(SpectrumError::NonFiniteInput { frame, channel })
            );
        }
    }
    assert_eq!(analyzer.retained_bytes(), 0);
}

#[test]
fn work_and_memory_admission_precedes_allocation_and_never_reduces_quality() {
    let request = spec(1_024, 0.0, NYQUIST_HZ, 129);
    let input = vec![[0.0; 2]; 1_024];
    let work = request.estimate_work().unwrap();
    let exact = SpectrumLimits {
        max_work_units: work.work_units,
        max_allocation_bytes: work.allocation_bytes,
    };
    let mut analyzer = SpectrumAnalyzer::new();
    let cancel = AtomicBool::new(false);
    assert_eq!(
        analyzer.analyze(
            &input,
            &request,
            &SpectrumLimits {
                max_work_units: work.work_units - 1,
                ..exact
            },
            &cancel
        ),
        Err(SpectrumError::WorkBudgetExceeded)
    );
    assert_eq!(analyzer.retained_bytes(), 0);
    assert_eq!(
        analyzer.analyze(
            &input,
            &request,
            &SpectrumLimits {
                max_allocation_bytes: work.allocation_bytes - 1,
                ..exact
            },
            &cancel
        ),
        Err(SpectrumError::MemoryBudgetExceeded)
    );
    assert_eq!(analyzer.retained_bytes(), 0);
    let result = analyzer.analyze(&input, &request, &exact, &cancel).unwrap();
    assert_eq!(result.amplitudes.len(), 129);
}

#[test]
fn cancellation_returns_no_frame_and_scratch_reuse_recovers_at_every_phase() {
    let request = spec(1_024, 0.0, NYQUIST_HZ, 64);
    let input = tone(1_024, 15, 0.75, -0.75);
    let expected = analyze(&input, &request);
    let mut analyzer = SpectrumAnalyzer::new();
    assert_eq!(
        analyzer.analyze(
            &input,
            &request,
            &SpectrumLimits::default(),
            &AtomicBool::new(true)
        ),
        Err(SpectrumError::Cancelled)
    );
    assert_eq!(analyzer.retained_bytes(), 0);
    for (point, occurrence) in [
        (CancelPoint::Admission, 1),
        (CancelPoint::Input, 2),
        (CancelPoint::Allocation, 1),
        (CancelPoint::Window, 3),
        (CancelPoint::Permutation, 2),
        (CancelPoint::FftStage, 17),
        (CancelPoint::Output, 31),
        (CancelPoint::Complete, 1),
    ] {
        let mut visits = 0;
        let result = analyzer.analyze_checked(
            &input,
            &request,
            &SpectrumLimits::default(),
            &mut |current| {
                if current == point {
                    visits += 1;
                    if visits == occurrence {
                        return Err(SpectrumError::Cancelled);
                    }
                }
                Ok(())
            },
        );
        assert_eq!(visits, occurrence);
        assert_eq!(result, Err(SpectrumError::Cancelled));
        assert_eq!(
            analyzer
                .analyze(
                    &input,
                    &request,
                    &SpectrumLimits::default(),
                    &AtomicBool::new(false)
                )
                .unwrap(),
            expected
        );
    }
}

#[test]
fn request_order_and_buffer_size_do_not_change_results() {
    let request = spec(1_024, 50.0, 3_000.0, 257);
    let input = tone(1_024, 11, 1.75, -0.25);
    let expected = analyze(&input, &request);
    let mut analyzer = SpectrumAnalyzer::new();
    let cancel = AtomicBool::new(false);
    analyzer
        .analyze(
            &vec![[0.0; 2]; 48_000],
            &spec(48_000, 0.0, NYQUIST_HZ, MAX_BANDS),
            &SpectrumLimits::default(),
            &cancel,
        )
        .unwrap();
    assert_eq!(
        analyzer
            .analyze(&input, &request, &SpectrumLimits::default(), &cancel)
            .unwrap(),
        expected
    );
    let work = request.estimate_work().unwrap();
    let tighter = SpectrumLimits {
        max_work_units: work.work_units,
        max_allocation_bytes: work.allocation_bytes,
    };
    assert_eq!(
        analyzer
            .analyze(&input, &request, &tighter, &cancel)
            .unwrap(),
        expected
    );
    assert!(analyzer.retained_bytes() <= work.scratch_bytes);
}

#[test]
fn finite_f32_extremes_remain_finite_without_clipping() {
    for amplitude in [f32::MAX, f32::MIN_POSITIVE, f32::from_bits(1)] {
        let result = analyze(
            &vec![[amplitude, -amplitude]; 1_024],
            &spec(1_024, 0.0, NYQUIST_HZ, 2),
        );
        assert!(
            result
                .amplitudes
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0)
        );
        assert!((result.amplitudes[0] / f64::from(amplitude) - 1.0).abs() < 1.0e-10);
    }
}
