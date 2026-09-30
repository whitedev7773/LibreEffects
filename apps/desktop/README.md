# Libre Effects Desktop

Built with [GPUI](https://www.gpui.rs).

Windows-first motion graphics editor based on the OpenCut rewrite. This is an early
2D editing foundation, not yet an After Effects-compatible application.

## Available now

- A default 1920 x 1080 composition, 30 fps, 150 frames (five seconds).
- Rectangle layers with stable IDs, selection, ordering, visibility, locking, and deletion.
- Position, anchor point, scale, rotation, and opacity controls. X and Y are separate channels.
- Per-property keyframes and Linear, Hold, and Smooth interpolation.
- Composition preview, looping playback, frame stepping, and timeline seeking.
- Undo/redo (up to 100 edits) and versioned `.lfe.json` project files.
- A shared command model in `crates/core`, independent of GPUI.

## Try an animation

1. Click **+ Rectangle** in Project.
2. At frame zero, click **Add key** under Position X in Transform.
3. Click the timeline ruler at approximately two seconds.
4. Click **+** next to Position X several times. Once a property has a key,
   editing its value inserts or updates a key at the current frame.
5. Click **Start**, then **Play**. Click **Pause** to stop the loop.
6. Click a diamond in the timeline to return to that key. Click its interpolation
   button in Transform to cycle Linear → Hold → Smooth.
7. Use **Save as** to save a `.lfe.json` file, and **Open** to reload it.

Buttons support Tab and Enter/Space. With the ruler focused, Left/Right step a
frame, Home/End seek, and Space toggles playback. Position/anchor controls change
by 10 pixels; scale, rotation, and opacity controls change by 5 units.

The interpolation on a key controls the segment following it. Smooth uses
smoothstep, not AE's temporal Bezier easing. Removing the last key retains its
value as a static property. Locked layers must be unlocked before editing.

New and Open are undoable. Save as writes the snapshot captured when clicked,
using a temporary file before replacing the destination. Project files are limited
to 16 MiB. Save your work before closing; session recovery is not implemented.

## Next milestones

JSX/ExtendScript execution is **not implemented**. The command API is the intended
foundation for a scripting bridge; it does not currently run JavaScript or claim
compatibility with existing AE scripts. Planned work includes the AE-style object
model, undo groups, expression evaluation, and explicit compatibility tests.

Also pending: composition settings UI, numeric text entry, keyframe dragging,
Bezier graph editing, parenting, precompositions, layer in/out points, text and
media layers, masks, effects, and video export. The current preview renders rectangles.

## Running

Rust is pinned in `.prototools` at the repo root (`proto use` installs it).

```sh
moon run desktop:dev     # cargo run
moon run desktop:check   # cargo check
moon run desktop:build   # cargo build --release
```

The first build compiles GPUI from source and takes a while. The root `Cargo.lock` is committed.

The Windows executable is `target/release/libre-effects.exe`. For a debug build,
use `cargo run -p libre-effects-desktop` from the repository root.

```sh
cargo test --workspace
cargo fmt --all --check
```

## Platform requirements

- **macOS**: Xcode command line tools (Metal renderer).
- **Windows**: no extra dependencies (Win32 + DirectWrite).
- **Linux**: renders via Vulkan (Blade), windows via Wayland or X11 (both enabled by default). System packages (Debian/Ubuntu names): `libvulkan1` + working Vulkan drivers, `libwayland-dev`, `libx11-xcb-dev`, `libxkbcommon-x11-dev`, `libfontconfig-dev`, plus a C toolchain and `cmake`.
- **WSL2/WSLg**: uses XWayland automatically when available. GPUI 0.2.2 requires `xdg_wm_base` v2–5, while WSLg advertises v1.
