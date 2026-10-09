# Hardware blur compute

Windows CUDA cores accelerate the existing five-pass Gaussian box kernel on
NVIDIA devices. The installed driver JIT-compiles embedded PTX through the CUDA
Driver API; no toolkit/compiler installation is needed. CUDA context push/pop
under a mutex allows preview and export work to move between worker threads.
Direct3D11 compute is the fallback and remains selectable. Direct3D adapters
are enumerated by DXGI high-performance preference; software/WARP adapters are
excluded. Other platforms retain CPU rendering.
The shader uses premultiplied RGBA8, transparent borders, the original V/H pass
order, CPU-generated radius pairs and nearest-even byte rounding after each
axis. Horizontal passes use a padded tiled transpose so both axes read adjacent
pixels across GPU lanes. No authored project field or blur profile changes.

Each backend serializes its context. Diagnostics never lock either context.
CUDA retains at most two 128-MiB device buffers, one 128-MiB host readback buffer
and one 64-KiB device lookup table. The host readback buffer is reused rather
than zero-allocating an image for each job. Direct3D11 retains at most two
128-MiB structured buffers and one 128-MiB staging buffer;
old capacities are released before allocating replacements. Image jobs require
exact lengths, at most 32 megapixels, axes no larger than 16384 and radii no larger
than 8192. Unsupported jobs use CPU. GPU initialization, allocation, dispatch or
readback errors retain caller pixels for a complete retry. CUDA failures disable
CUDA submissions for the process and retry Direct3D11. If that also fails,
hardware is disabled and CPU performs the job. The GPU budget is separate from
checked CPU buffers. Only synchronized readback commits caller pixels.

The desktop installs the accelerator for each synchronous render scope, covering
nested masks, footage/effects and final paint in both previews and exports.
Small jobs below 64K pixels stay on CPU. CUDA also implements exact fractional
Box3 (three passes per axis) and alpha/channel color-space lookup tables. Box3
keeps f64 arithmetic, axis order, transparent borders and all six truncations;
Direct3D11 continues to use the CPU fallback for these additional operations.
A single unclipped LinearRGB ordinary Gaussian can keep both color conversions
and all blur axes on the device, using one image upload/readback. Other filter
graphs retain the generic path; checked CPU allocation admission is preserved.
Ordinary Gaussian columns are split into 256-row bands for radii up to 128,
with independently initialized exact integer window sums. Larger radii use
whole columns to avoid repeated long initialization windows. This increases
available GPU work without changing pass order or byte quantization.

CUDA can exclude fully zero RGBA margins while retaining the entire blur halo.
Nonzero RGB at zero alpha still counts as source support. The final full canvas
is restored only after successful synchronization/readback. This is an exact
support reduction, not a viewport clip or preview quality reduction. Color
tables that map zero to nonzero decline cropping. Full/near-full support also
retains the original canvas. Separate hardware tests compare complete canvas
bytes against independent CPU Gaussian/Box3/LUT oracles, including band edges,
large radii, transparent padding and color-conversion/blur chains.

IIR blur, SVG geometry, text shaping, blending and encoding retain their
existing paths. The desktop's
video sessions separately accelerate decoding with NVDEC/D3D11VA/Quick Sync;
see the [desktop hardware notes](../../apps/desktop/README.md#hardware-gpu-acceleration).

`LIBRE_EFFECTS_RENDER_BACKEND=auto|cuda|d3d11|cpu` selects a compute preference;
`cpu` selects the unchanged software path. Hardware preferences allow fallback
and diagnostics expose the backend that actually completed work. The
desktop's `--gpu-info` probes the adapter, while `--preview-benchmark` includes
hardware job counters and wall time (including transfer and readback). Stage
timings include nested work: expression, raster, embedded PNG and video times
overlap lowering/paint; do not add all fields to obtain a frame total.

Run the ignored `hardware_box_matches_independent_byte_oracle` test explicitly
on a Windows hardware machine. It checks 90 size/radius/color combinations against
whole-line CPU prefix sums, including transparent alpha, one-pixel axes,
directional identity passes and radii larger than the image. Normal unit tests
do not require a GPU. With `LIBRE_EFFECTS_RENDER_BACKEND=cuda`, the qualification
also demands 90 CUDA jobs and zero fallback, then verifies 40 more jobs across
four worker threads to exercise context migration and serialization.

## Previous Direct3D11 delivery measurements

On the qualified Ayase project, root composition 616, NVIDIA GeForce GTX 1650,
the delivered optimized executable produced these sequential measurements with
no Cargo compilation running:

| Operation | Previous delivery | Optimized CPU | Optimized GPU |
| --- | ---: | ---: | ---: |
| Fresh 1280x640 preview, mean of frames 11465–11467 | 6.709 s | 2.791 s | 1.635 s |
| 1920x960 PNG export, frame 11465, strict fonts | 14.945 s | 6.660 s | 3.107 s |

The preview is 4.10x faster and the still export 4.81x faster than the previous
delivery; these gains include release optimization. Comparing the same release
with hardware enabled/disabled isolates the GPU contribution. Export timing
includes process startup and file encoding, and waits for the GUI-subsystem
executable to exit. Preview timing measures fresh frame work, excluding project
startup; cached geometry and shared pixels remain intact.

All four full-resolution native RGBA hashes at frames 300, 4360, 8880 and 11465
match the previous CPU delivery exactly. The PNG files from all three paths are
also byte-identical. This preserves existing compatibility and does not claim
that the remaining AE/reference differences have been eliminated. Measurements
and binary/project hashes are in the local delivery's `performance.json` and
`validation.json`.

Direct3D resources and readback follow Microsoft's
[resource mapping contract](https://learn.microsoft.com/en-us/windows/win32/direct3d11/how-to--use-dynamic-resources).
CUDA follows NVIDIA's [Driver API context and module contracts](https://docs.nvidia.com/cuda/cuda-programming-guide/03-advanced/driver-api.html)
and [PTX instruction specification](https://docs.nvidia.com/cuda/parallel-thread-execution/).

## CUDA/NVDEC delivery measurements

The next optimized delivery on the same GTX 1650 and unchanged Ayase LEP uses
CUDA compute and NVDEC by default. Sequential runs without a compiler active:

| Operation | Previous Direct3D11 delivery | New CPU | New CUDA/NVDEC |
| --- | ---: | ---: | ---: |
| Fresh 1280x640 preview, mean of frames 11465–11467 | 1.824 s | 3.001 s | 1.636 s |
| 1920x960 PNG export, frame 11465, strict fonts | 3.365 s | 7.783 s | 3.141 s |

Against CPU in the same executable, CUDA/NVDEC reduces preview time by 45.5%
and PNG export time by 59.6%. Against the previous hardware delivery, those
reductions are 10.3% and 6.7%. These are three fresh preview frames and one
process-inclusive still export, not sustained playback FPS or universal gains.
CUDA compute with CPU video decoding measured 1.523 s per preview frame;
NVDEC initialization/download costs made this short preview sample slightly
slower. Video-engine usage does not by itself guarantee faster total conversion.

The default run completed 21 CUDA jobs and decoded two actual NVDEC frames with
zero failures/fallbacks. A separate Direct3D11 run completed hardware compute
and D3D11VA decoding; requesting unavailable Intel Quick Sync completed through
one CPU fallback. AMD and Intel physical devices were unavailable for testing.
All four established full-resolution native RGBA hashes and all three exported
PNG files remain identical. The CUDA oracle checks 90 independent byte cases
and 40 further jobs across four worker threads. NVDEC qualification checks 128
hardware frames with fractional CFR/seeks/loops, plus exact alpha CPU fallback.
The local `dist/ayase-compatible-cuda-v3` includes the executable, unchanged LEP,
performance receipts, native comparisons and validation details.
