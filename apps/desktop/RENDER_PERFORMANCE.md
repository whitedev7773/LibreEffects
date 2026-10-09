# Render performance qualification — v7

Measured 2026-10-09 on Ryzen 5 4600G (6 cores / 12 threads), GTX 1650 and 32 GB RAM.
Baseline: shipped v6. Project: unchanged `Ayase-compatible.lep`, composition 616,
“TO RENDER: Black-White Theme”, 1920×960 at 60 fps.

| Operation | v6 | v7 | Time reduction |
| --- | ---: | ---: | ---: |
| New adjacent preview frame, Auto 1280×640 | 1227.18 ms | 754.86 ms | 38.49% |
| New adjacent preview frame, Full 1920×960 | 1894.37 ms | 1046.90 ms | 44.74% |
| Native MP4 export, 32 frames | 61.559 s | 15.364 s | 75.04% |

Preview values are medians of nine new adjacent frames per resolution, from
three alternating v6/v7 runs. Frames 11466–11468 follow initial frame 11465;
already-completed frame cache hits are excluded. Initial-frame medians were
1968.26→1517.94 ms in Auto and
2762.51→1978.65 ms in Full.
MP4 is one run per version: frames 11465–11496 inclusive, H.264 medium defaults,
audio off, strict fonts. Timing includes startup, preflight, render, encode and
publication. v7 automatically admitted 6 render workers in this run.
All compilation and tests finished before measurement. The original open AE
instance had idle caching temporarily disabled and an 8 GB memory allowance
(24 GB reserved for other applications) for both versions. After measurement, the original 6 GB reservation and enabled
idle cache were restored; the original dirty project was left open. These samples do not
guarantee performance for other frames, effects, memory pressure or output codecs.

## Implemented changes

- CUDA color-space conversion and eligible Gaussian blur now share one upload
  and readback. Exact transparent-support cropping excludes unused margins while
  preserving the complete blur halo and full output canvas. Gaussian kernels use
  independent 256-row bands for eligible radii, and reuse host readback memory.
- Exact fractional Box3 and alpha/channel color lookup CUDA kernels have
  independent CPU-oracle hardware tests. The measured Ayase frames exercised
  CUDA Gaussian/color-blur and NVDEC; their fractional Box3 counter was zero.
- Export uses bounded, ordered frame workers, each with its own renderer,
  decoder and expression process. CPU and available RAM admission choose up to
  six workers. Large canvases remain serial. A positive
  `LIBRE_EFFECTS_RENDER_WORKERS` caps the automatically admitted count; `1`
  forces serial operation. Cancellation/failure releases all bounded queues
  and joins workers before returning. Output is published only when complete.
- Expression worker processes are reused with bounded recycling, while every
  batch receives a fresh evaluator. Independent watchdogs still terminate
  native hangs and cancellation. Active expression/JSX evaluation retains the
  existing global serialization guard.
- Exact complete SVG path strings reuse immutable geometry in a renderer-owned
  bounded cache. Current paint, transforms, clipping and effects still resolve
  each frame. Existing text, source image and intermediate caches are retained.
- `LibreEffectsRender.exe` provides the same renderer/export CLI without GPUI
  initialization. The editor executable remains `LibreEffects.exe`.

No frame skipping or reduced filter quality was introduced. Complete-frame
playback gating and playback-speed controls remain. Fresh preview rendering is
still serial; export worker parallelism does not imply parallel preview filling.
Cache payload ceilings total 464 MiB per renderer, separate from frame resources,
preview RAM and other allocations. At most 13 raw export frames can be in flight
or queued with six workers. Admission reserves 2 GiB for the system and estimates
2 GiB plus two raw frames per lane; this is not a process-wide hard memory cap.

## Actual After Effects observations

Installed AE 26.5x89 aerender rendered the original AEP, same composition and
32-frame interval, Full/Best, MFR enabled and 25% memory allowance. A new process
was used without `-reuse`, and it exited without saving project changes.
Total wall time was **27.056 s**, including startup/load/shutdown; AE's own log
reported **6 s** for the render phase, at whole-second precision. The live AE
Composition Profiler showed **521 ms** for Full frame 11467 in one observation.

These establish that AE remains faster in the observed render work. They are
not a codec/cache-matched ratio: AE wrote Lossless BGR24 AVI with PCM audio,
Libre Effects wrote H.264 without audio, and existing AE disk cache was readable.
Do not compare total process times and claim Libre Effects beats AE. Full fresh
preview is not realtime and AE performance or complete pixel parity is not met.
CPU scene preparation, SVG painting/compositing and expression work remain costs.

## Validation and delivery

- Moon desktop check, workspace/vendor formatting and whitespace checks passed.
- Workspace: 2,836 passed, 0 failed, 64 ignored; resvg: 39 passed; usvg: 19 passed.
- Actual CUDA hardware: 5 passed, including independent complete-canvas byte
  oracles, band boundaries, large radii and multi-thread context migration.
- Reused expression process harness: 40 alternating batches through recycling,
  native timeout, cancellation, recovery and process cleanup passed.
- Frames 300, 4360, 11465 and 11468: exact RGBA matches existing Libre Effects
  references. All 32 decoded MP4 frames match v6 with identical order.
- Source AEP and compatible LEP SHA256 hashes remain unchanged. Existing AE
  visual differences and parent PREVIEW Audio Spectrum budget limits remain.
- CUDA/NVDEC jobs completed with no recorded hardware failures/fallbacks.
  AMD/Intel paths need qualification on their respective hardware.

Delivery: `dist/ayase-compatible-performance-v7`, including both executables,
LEP, JSON measurements, pixel receipts, AE log, usage guide and UI notes.
Reproduce with `target/performance-v7-qualify.ps1 -RunFolder NEW_FOLDER` using
fresh output destinations. The GUI and console binaries accept
`--preview-benchmark PROJECT --frames 11465,11466,11467,11468 --dimension 1920
--composition 616` and `--render PROJECT --output OUTPUT.mp4 --composition 616
--start 11465 --end 11497 --audio off --fonts strict`.
