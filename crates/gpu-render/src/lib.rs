//! Bounded Windows hardware compute for the existing RGBA8 Gaussian box kernel.
//! Failed/unsupported jobs never mutate caller pixels. Other platforms use CPU.
use serde::Serialize;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

mod crop;
#[cfg(windows)]
mod cuda;
#[cfg(windows)]
mod d3d11;
#[derive(Clone, Copy)]
enum Backend {
    Cpu,
    Cuda,
    D3d11,
}
static BACKEND: OnceLock<Backend> = OnceLock::new();
static CUDA_FAILED: AtomicBool = AtomicBool::new(false);
static CUDA_JOBS: AtomicU64 = AtomicU64::new(0);
static D3D11_JOBS: AtomicU64 = AtomicU64::new(0);
static BOX3_JOBS: AtomicU64 = AtomicU64::new(0);
static LUT_JOBS: AtomicU64 = AtomicU64::new(0);
static COLOR_BLUR_JOBS: AtomicU64 = AtomicU64::new(0);
static FALLBACK: OnceLock<String> = OnceLock::new();
static JOBS: AtomicU64 = AtomicU64::new(0);
static PIXELS: AtomicU64 = AtomicU64::new(0);
static MICROS: AtomicU64 = AtomicU64::new(0);
static FAILURES: AtomicU64 = AtomicU64::new(0);
static DISABLED: AtomicBool = AtomicBool::new(false);
static ERROR: OnceLock<String> = OnceLock::new();

/// Process-local hardware diagnostics; software adapters are never accepted.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub backend: &'static str,
    pub adapter: Option<String>,
    pub accelerated_jobs: u64,
    pub cuda_jobs: u64,
    pub d3d11_jobs: u64,
    pub fractional_box3_jobs: u64,
    pub color_lookup_jobs: u64,
    pub color_blur_jobs: u64,
    pub fallback_reason: Option<String>,
    pub accelerated_pixels: u64,
    /// Wall time including initialization, upload, compute, mutex wait and readback.
    pub hardware_job_milliseconds: f64,
    pub failed_jobs: u64,
    pub disabled: bool,
    pub error: Option<String>,
}
/// Does not initialize hardware solely to read counters.
pub fn status() -> Status {
    Status {
        backend: match current_backend() {
            Backend::Cpu => "CPU",
            Backend::Cuda => "CUDA",
            Backend::D3d11 => "Direct3D11 compute",
        },
        adapter: adapter_name(),
        accelerated_jobs: JOBS.load(Ordering::Relaxed),
        cuda_jobs: CUDA_JOBS.load(Ordering::Relaxed),
        d3d11_jobs: D3D11_JOBS.load(Ordering::Relaxed),
        fractional_box3_jobs: BOX3_JOBS.load(Ordering::Relaxed),
        color_lookup_jobs: LUT_JOBS.load(Ordering::Relaxed),
        color_blur_jobs: COLOR_BLUR_JOBS.load(Ordering::Relaxed),
        fallback_reason: FALLBACK.get().cloned(),
        accelerated_pixels: PIXELS.load(Ordering::Relaxed),
        hardware_job_milliseconds: MICROS.load(Ordering::Relaxed) as f64 / 1000.0,
        failed_jobs: FAILURES.load(Ordering::Relaxed),
        disabled: DISABLED.load(Ordering::Relaxed),
        error: ERROR.get().cloned(),
    }
}
#[cfg(windows)]
fn adapter_name() -> Option<String> {
    match current_backend() {
        Backend::Cuda => cuda::adapter_name(),
        Backend::D3d11 => d3d11::adapter_name(),
        Backend::Cpu => None,
    }
}
fn current_backend() -> Backend {
    match BACKEND.get().copied().unwrap_or(Backend::Cpu) {
        Backend::Cuda if CUDA_FAILED.load(Ordering::Relaxed) => Backend::D3d11,
        other => other,
    }
}
fn choose_backend() -> Backend {
    *BACKEND.get_or_init(|| {
        let setting = std::env::var("LIBRE_EFFECTS_RENDER_BACKEND").unwrap_or_default();
        if setting == "cpu" {
            return Backend::Cpu;
        }
        #[cfg(windows)]
        {
            if setting != "d3d11" {
                match cuda::initialize() {
                    Ok(()) => return Backend::Cuda,
                    Err(error) => {
                        let _ = FALLBACK.set(error);
                    }
                }
            }
            match d3d11::initialize() {
                Ok(()) => return Backend::D3d11,
                Err(error) => {
                    let _ = ERROR.set(error);
                    DISABLED.store(true, Ordering::Relaxed);
                }
            }
        }
        Backend::Cpu
    })
}
/// Probe CUDA independently of the selected compositor, for NVDEC selection.
pub fn cuda_available() -> bool {
    #[cfg(windows)]
    {
        cuda::initialize().is_ok()
    }
    #[cfg(not(windows))]
    {
        false
    }
}
/// Explicit hardware probe for diagnostics, without submitting image work.
pub fn initialize() -> Status {
    choose_backend();
    status()
}
#[cfg(not(windows))]
fn adapter_name() -> Option<String> {
    None
}

/// Exact-byte five-pass blur; true implies a fully completed hardware job.
/// Radius-zero passes are identities. GPU failures leave input unchanged and
/// disable further GPU submissions until the process restarts.
pub fn box_blur(pixels: &mut [u8], width: u32, height: u32, radii: [[u32; 2]; 5]) -> bool {
    if !valid(pixels.len(), width, height, radii) || DISABLED.load(Ordering::Relaxed) {
        return false;
    }
    #[cfg(windows)]
    {
        let started = std::time::Instant::now();
        let selected = choose_backend();
        let result = match selected {
            Backend::Cpu => return false,
            Backend::Cuda if !CUDA_FAILED.load(Ordering::Relaxed) => {
                match cuda::apply(pixels, width, height, radii) {
                    Ok(()) => {
                        CUDA_JOBS.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(error) => {
                        FAILURES.fetch_add(1, Ordering::Relaxed);
                        CUDA_FAILED.store(true, Ordering::Relaxed);
                        let _ = FALLBACK.set(error);
                        d3d11::apply(pixels, width, height, radii).inspect(|_| {
                            D3D11_JOBS.fetch_add(1, Ordering::Relaxed);
                        })
                    }
                }
            }
            _ => d3d11::apply(pixels, width, height, radii).inspect(|_| {
                D3D11_JOBS.fetch_add(1, Ordering::Relaxed);
            }),
        };
        match result {
            Ok(()) => {
                JOBS.fetch_add(1, Ordering::Relaxed);
                PIXELS.fetch_add(u64::from(width) * u64::from(height), Ordering::Relaxed);
                MICROS.fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
                return true;
            }
            Err(error) => {
                FAILURES.fetch_add(1, Ordering::Relaxed);
                DISABLED.store(true, Ordering::Relaxed);
                let _ = ERROR.set(error);
            }
        }
    }
    false
}
fn valid(bytes: usize, width: u32, height: u32, radii: [[u32; 2]; 5]) -> bool {
    let count = u64::from(width) * u64::from(height);
    width > 0
        && height > 0
        && width <= 16384
        && height <= 16384
        && count <= 33_554_432
        && count * 4 == bytes as u64
        && radii.iter().flatten().all(|r| *r <= 8192)
}

/// Keeps both exact color conversions and all ten Gaussian axes on the device.
/// One upload/readback; unsupported or failed jobs leave caller pixels intact.
pub fn color_blur(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radii: [[u32; 2]; 5],
    tables: [&[u8; 65536]; 2],
) -> bool {
    if !valid(pixels.len(), width, height, radii)
        || DISABLED.load(Ordering::Relaxed)
        || CUDA_FAILED.load(Ordering::Relaxed)
    {
        return false;
    }
    #[cfg(windows)]
    {
        if !matches!(choose_backend(), Backend::Cuda) {
            return false;
        }
        let started = std::time::Instant::now();
        match cuda::apply_color_blur(pixels, width, height, radii, tables) {
            Ok(()) => {
                JOBS.fetch_add(1, Ordering::Relaxed);
                CUDA_JOBS.fetch_add(1, Ordering::Relaxed);
                COLOR_BLUR_JOBS.fetch_add(1, Ordering::Relaxed);
                PIXELS.fetch_add(u64::from(width) * u64::from(height), Ordering::Relaxed);
                MICROS.fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
                return true;
            }
            Err(error) => {
                FAILURES.fetch_add(1, Ordering::Relaxed);
                CUDA_FAILED.store(true, Ordering::Relaxed);
                let _ = FALLBACK.set(error);
            }
        }
    }
    false
}

/// Exact f64 fractional-box profile, preserving axis order and all six byte
/// quantizations. Unsupported devices and failed jobs leave input unchanged.
pub fn fractional_box3(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radii: [f64; 2],
    vertical_first: bool,
) -> bool {
    if !valid(pixels.len(), width, height, [[0; 2]; 5])
        || !radii
            .iter()
            .all(|r| r.is_finite() && (0.0..=8192.0).contains(r))
        || DISABLED.load(Ordering::Relaxed)
        || CUDA_FAILED.load(Ordering::Relaxed)
    {
        return false;
    }
    #[cfg(windows)]
    {
        if !matches!(choose_backend(), Backend::Cuda) {
            return false;
        }
        let started = std::time::Instant::now();
        match cuda::apply_box3(pixels, width, height, radii, vertical_first) {
            Ok(()) => {
                JOBS.fetch_add(1, Ordering::Relaxed);
                CUDA_JOBS.fetch_add(1, Ordering::Relaxed);
                BOX3_JOBS.fetch_add(1, Ordering::Relaxed);
                PIXELS.fetch_add(u64::from(width) * u64::from(height), Ordering::Relaxed);
                MICROS.fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
                return true;
            }
            Err(error) => {
                FAILURES.fetch_add(1, Ordering::Relaxed);
                CUDA_FAILED.store(true, Ordering::Relaxed);
                let _ = FALLBACK.set(error);
            }
        }
    }
    false
}

/// Applies an exact alpha/channel byte table to RGB, preserving alpha.
pub fn channel_lut(pixels: &mut [u8], width: u32, height: u32, table: &[u8; 65536]) -> bool {
    if !valid(pixels.len(), width, height, [[0; 2]; 5])
        || DISABLED.load(Ordering::Relaxed)
        || CUDA_FAILED.load(Ordering::Relaxed)
    {
        return false;
    }
    #[cfg(windows)]
    {
        if !matches!(choose_backend(), Backend::Cuda) {
            return false;
        }
        let started = std::time::Instant::now();
        match cuda::apply_lut(pixels, width, height, table) {
            Ok(()) => {
                JOBS.fetch_add(1, Ordering::Relaxed);
                CUDA_JOBS.fetch_add(1, Ordering::Relaxed);
                LUT_JOBS.fetch_add(1, Ordering::Relaxed);
                PIXELS.fetch_add(u64::from(width) * u64::from(height), Ordering::Relaxed);
                MICROS.fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
                return true;
            }
            Err(error) => {
                FAILURES.fetch_add(1, Ordering::Relaxed);
                CUDA_FAILED.store(true, Ordering::Relaxed);
                let _ = FALLBACK.set(error);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_jobs_preserve_input_and_do_not_initialize_hardware() {
        let mut bytes = vec![11; 28];
        let old = bytes.clone();
        for (w, h, radii) in [
            (0, 7, [[0; 2]; 5]),
            (7, 2, [[0; 2]; 5]),
            (7, 1, [[8193; 2]; 5]),
        ] {
            assert!(!box_blur(&mut bytes, w, h, radii));
            assert_eq!(bytes, old);
            assert!(!color_blur(
                &mut bytes,
                w,
                h,
                radii,
                [&[0; 65536], &[0; 65536]]
            ));
            assert_eq!(bytes, old);
        }
    }
    #[test]
    #[ignore = "requires an actual CUDA hardware adapter"]
    fn hardware_resident_color_blur_matches_independent_full_canvas_oracle() {
        let tables: [Box<[u8; 65536]>; 2] = [
            Box::new(std::array::from_fn(|i| ((i % 256) * (i / 256) / 255) as u8)),
            Box::new(std::array::from_fn(|i| ((i % 256) * 3 / 4) as u8)),
        ];
        let lut = |input: &mut Vec<u8>, table: &[u8; 65536]| {
            for p in input.chunks_exact_mut(4) {
                let base = usize::from(p[3]) * 256;
                for c in 0..3 {
                    p[c] = table[base + usize::from(p[c])];
                }
            }
        };
        for (w, h, padded) in [(13, 7, false), (512, 512, false), (1024, 512, true)] {
            let mut input: Vec<u8> = (0..w * h)
                .flat_map(|i| {
                    let a = ((i * 73 + 19) % 256) as u8;
                    [a / 2, a / 3, a / 7, a]
                })
                .collect();
            if padded {
                input.fill(0);
                input[(w * 41 + 33) * 4..(w * 41 + 33) * 4 + 4].copy_from_slice(&[31, 29, 7, 81]);
                input[0] = 17; // Zero alpha still carries support before input LUT.
            }
            for radii in [[[0, 0]; 5], [[3, 2]; 5], [[0, 9]; 5], [[10, 0]; 5]] {
                let mut expected = input.clone();
                lut(&mut expected, &tables[0]);
                expected = oracle(expected, w, h, radii);
                lut(&mut expected, &tables[1]);
                let mut actual = input.clone();
                assert!(
                    color_blur(
                        &mut actual,
                        w as u32,
                        h as u32,
                        radii,
                        [&tables[0], &tables[1]]
                    ),
                    "{:?}",
                    status()
                );
                assert_eq!(actual, expected, "{w}x{h} {radii:?}");
            }
        }
        assert_eq!(status().failed_jobs, 0);
    }
    #[test]
    #[ignore = "requires an actual CUDA hardware adapter"]
    fn hardware_segmented_columns_match_oracle_across_band_boundaries() {
        let (w, h) = (521usize, 677usize);
        let input: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let a = ((i * 71 + 23) % 256) as u8;
                [a / 2, a / 3, a / 7, a]
            })
            .collect();
        for radii in [
            [[1, 1]; 5],
            [[19, 31]; 5],
            [[127, 128]; 5],
            [[129, 0]; 5],
            [[0, 8192]; 5],
        ] {
            let expected = oracle(input.clone(), w, h, radii);
            let mut actual = input.clone();
            assert!(
                box_blur(&mut actual, w as u32, h as u32, radii),
                "{:?}",
                status()
            );
            assert_eq!(actual, expected, "{radii:?}");
        }
        assert_eq!(status().failed_jobs, 0);
    }
    pub(super) fn oracle(
        mut input: Vec<u8>,
        width: usize,
        height: usize,
        radii: [[u32; 2]; 5],
    ) -> Vec<u8> {
        for pair in radii {
            for axis in [1, 0] {
                let radius = pair[axis] as usize;
                if radius == 0 {
                    continue;
                }
                let (length, lines, stride) = if axis == 0 {
                    (width, height, 1)
                } else {
                    (height, width, width)
                };
                let mut output = vec![0; input.len()];
                for line in 0..lines {
                    let base = if axis == 0 { line * width } else { line };
                    // Independent whole-line prefix sums, not the GPU sliding recurrence.
                    let mut prefix = vec![[0u32; 4]; length + 1];
                    for i in 0..length {
                        for c in 0..4 {
                            prefix[i + 1][c] =
                                prefix[i][c] + input[(base + i * stride) * 4 + c] as u32;
                        }
                    }
                    for i in 0..length {
                        for c in 0..4 {
                            let sum = prefix[(i + radius + 1).min(length)][c]
                                - prefix[i.saturating_sub(radius)][c];
                            let scaled = sum as f32 * (1.0 / (2 * radius + 1) as f32);
                            let rounded = (scaled + 12582912.0) - 12582912.0;
                            output[(base + i * stride) * 4 + c] = rounded as u8;
                        }
                    }
                }
                input = output;
            }
        }
        input
    }
    #[test]
    #[ignore = "requires an actual Windows CUDA or Direct3D11 hardware adapter"]
    fn hardware_box_matches_independent_byte_oracle() {
        let before = status();
        for (width, height) in [(1u32, 1u32), (1, 17), (19, 1), (7, 5), (131, 97), (513, 9)] {
            for mode in 0..3 {
                let input: Vec<u8> = (0..width * height)
                    .flat_map(|i| {
                        let a = ((i * 53 + 197) % 256) as u8;
                        match mode {
                            0 => [a / 2, a / 3, a / 5, a],
                            1 => [255; 4],
                            _ => [a, 255 - a, a / 3, 255],
                        }
                    })
                    .collect();
                for radii in [
                    [[0; 2]; 5],
                    [[1, 1]; 5],
                    [[3, 2], [4, 2], [3, 4], [4, 3], [7, 1]],
                    [[0, 17]; 5],
                    [[8192, 8192]; 5],
                ] {
                    let expected = oracle(input.clone(), width as usize, height as usize, radii);
                    let mut actual = input.clone();
                    assert!(
                        box_blur(&mut actual, width, height, radii),
                        "{:?}",
                        status()
                    );
                    assert_eq!(actual, expected, "{width}x{height}, {radii:?}");
                }
            }
        }
        assert!(status().adapter.is_some());
        assert_eq!(status().accelerated_jobs, before.accelerated_jobs + 90);
        if std::env::var("LIBRE_EFFECTS_RENDER_BACKEND").is_ok_and(|s| s == "cuda") {
            assert_eq!(status().backend, "CUDA");
            assert_eq!(status().cuda_jobs, before.cuda_jobs + 90);
            // Context ownership must survive preview/export worker migration.
            let threads: Vec<_> = (0..4)
                .map(|_| {
                    std::thread::spawn(|| {
                        let input = vec![129; 131 * 97 * 4];
                        let expected = oracle(input.clone(), 131, 97, [[3, 2]; 5]);
                        for _ in 0..10 {
                            let mut actual = input.clone();
                            assert!(box_blur(&mut actual, 131, 97, [[3, 2]; 5]));
                            assert_eq!(actual, expected);
                        }
                    })
                })
                .collect();
            for thread in threads {
                thread.join().unwrap();
            }
            assert_eq!(status().cuda_jobs, before.cuda_jobs + 130);
            assert_eq!(status().failed_jobs, 0);
        }
    }
}

#[cfg(test)]
mod fractional_tests {
    use super::*;
    fn oracle(
        mut input: Vec<u8>,
        width: usize,
        height: usize,
        radii: [f64; 2],
        vertical_first: bool,
    ) -> Vec<u8> {
        for axis in if vertical_first { [1, 0] } else { [0, 1] } {
            let radius = radii[axis];
            if radius <= 0.5 {
                continue;
            }
            let n = (radius + 0.5).floor() as usize;
            let edge = radius - (n as f64 - 0.5);
            let (length, lines, stride) = if axis == 0 {
                (width, height, 1)
            } else {
                (height, width, width)
            };
            for _ in 0..3 {
                let mut output = vec![0; input.len()];
                for line in 0..lines {
                    let base = if axis == 0 { line * width } else { line };
                    let mut prefix = vec![[0u32; 4]; length + 1];
                    for i in 0..length {
                        for c in 0..4 {
                            prefix[i + 1][c] =
                                prefix[i][c] + u32::from(input[(base + i * stride) * 4 + c]);
                        }
                    }
                    for i in 0..length {
                        for c in 0..4 {
                            let interior = prefix[(i + n).min(length)][c]
                                - prefix[(i + 1).saturating_sub(n)][c];
                            let left = i
                                .checked_sub(n)
                                .map_or(0, |j| input[(base + j * stride) * 4 + c]);
                            let right = if i + n < length {
                                input[(base + (i + n) * stride) * 4 + c]
                            } else {
                                0
                            };
                            let value = (f64::from(interior)
                                + edge * f64::from(u32::from(left) + u32::from(right)))
                                / (2.0 * radius);
                            output[(base + i * stride) * 4 + c] =
                                (value + 32.0 * f64::EPSILON * value.abs())
                                    .floor()
                                    .clamp(0.0, 255.0) as u8;
                        }
                    }
                }
                input = output;
            }
        }
        input
    }
    #[test]
    fn invalid_fractional_and_lookup_jobs_leave_pixels_unchanged() {
        let mut input = vec![37; 16];
        for r in [f64::NAN, -1.0, f64::INFINITY, 8192.01] {
            assert!(!fractional_box3(&mut input, 2, 2, [r, 1.0], false));
            assert_eq!(input, vec![37; 16]);
        }
        assert!(!channel_lut(&mut input, 3, 2, &[0; 65536]));
        assert_eq!(input, vec![37; 16]);
    }
    #[test]
    #[ignore = "requires an actual CUDA hardware adapter"]
    fn hardware_fractional_and_lookup_match_independent_byte_oracles() {
        for (width, height) in [(1, 1), (7, 5), (65, 33), (131, 97)] {
            let input: Vec<u8> = (0..width * height)
                .flat_map(|i| {
                    let a = ((i * 73 + 19) % 256) as u8;
                    [a / 2, a / 3, a / 7, a]
                })
                .collect();
            for radii in [
                [0.0, 0.5],
                [0.5000000001, 0.6],
                [1.0, 1.5],
                [2.32, 8.5],
                [0.0, 17.7],
                [8192.0, 8192.0],
            ] {
                for vertical_first in [false, true] {
                    let expected = oracle(
                        input.clone(),
                        width as usize,
                        height as usize,
                        radii,
                        vertical_first,
                    );
                    let mut actual = input.clone();
                    assert!(
                        fractional_box3(&mut actual, width, height, radii, vertical_first),
                        "{:?}",
                        status()
                    );
                    assert_eq!(
                        actual, expected,
                        "{width}x{height}, {radii:?}, vertical={vertical_first}"
                    );
                }
            }
        }
        let table: Box<[u8; 65536]> =
            Box::new(std::array::from_fn(|i| ((i * 17 + i / 256) % 256) as u8));
        let mut input: Vec<u8> = (0..65536usize)
            .flat_map(|i| {
                [
                    i as u8,
                    255 - i as u8,
                    (i as u8).wrapping_mul(73),
                    (i / 256) as u8,
                ]
            })
            .collect();
        let expected: Vec<u8> = input
            .chunks_exact(4)
            .flat_map(|p| {
                let base = usize::from(p[3]) * 256;
                [
                    table[base + usize::from(p[0])],
                    table[base + usize::from(p[1])],
                    table[base + usize::from(p[2])],
                    p[3],
                ]
            })
            .collect();
        assert!(channel_lut(&mut input, 256, 256, &table), "{:?}", status());
        assert_eq!(input, expected);
        let input = vec![129; 1920 * 960 * 4];
        let expected = oracle(input.clone(), 1920, 960, [12.32, 8.5], false);
        let mut actual = input;
        let started = std::time::Instant::now();
        assert!(
            fractional_box3(&mut actual, 1920, 960, [12.32, 8.5], false),
            "{:?}",
            status()
        );
        println!("Full HD fractional box3 hardware: {:?}", started.elapsed());
        assert_eq!(actual, expected);
        assert_eq!(status().failed_jobs, 0);
    }
    #[test]
    #[ignore = "requires an actual CUDA hardware adapter"]
    fn hardware_transparent_support_crop_preserves_full_canvas_bytes() {
        let (w, h) = (1024usize, 512usize);
        let mut input = vec![0; w * h * 4];
        for (x, y, pixel) in [
            (0, 0, [17, 0, 0, 0]),
            (33, 41, [31, 29, 7, 81]),
            (100, 112, [83, 77, 19, 129]),
        ] {
            input[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&pixel);
        }
        for vertical in [false, true] {
            let expected = oracle(input.clone(), w, h, [2.32, 8.5], vertical);
            let mut actual = input.clone();
            assert!(fractional_box3(
                &mut actual,
                w as u32,
                h as u32,
                [2.32, 8.5],
                vertical
            ));
            assert_eq!(actual, expected);
        }
        let expected = super::tests::oracle(input.clone(), w, h, [[3, 2]; 5]);
        let mut actual = input.clone();
        assert!(box_blur(&mut actual, w as u32, h as u32, [[3, 2]; 5]));
        assert_eq!(actual, expected);
        let table = Box::new(std::array::from_fn(|i| ((i / 256) * (i % 256) / 255) as u8));
        let expected: Vec<u8> = input
            .chunks_exact(4)
            .flat_map(|p| {
                let base = usize::from(p[3]) * 256;
                [
                    table[base + usize::from(p[0])],
                    table[base + usize::from(p[1])],
                    table[base + usize::from(p[2])],
                    p[3],
                ]
            })
            .collect();
        assert!(channel_lut(&mut input, w as u32, h as u32, &table));
        assert_eq!(input, expected);
    }
}
