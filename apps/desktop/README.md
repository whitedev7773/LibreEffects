# Libre Effects Desktop

A Windows-first motion graphics editor built with Rust and GPUI. Its workspace
and basic editing workflow follow After Effects conventions. It is an early 2D
editor, not a complete After Effects replacement or an AEP-compatible application.

![Libre Effects workspace with the Motion Study sample](screenshots/workspace.png)

## Workspace

- Compact menu bar and toolbar, Project, Composition, Properties, and Timeline panels.
- Drag panel dividers to resize; double-click a divider or choose Window → Reset
  default workspace to restore the layout.
- Wanted Sans and Gravity Icons are embedded in the executable, with their licenses
  under assets/. No system font installation or runtime download is required.
- Composition settings (Ctrl+K): name, dimensions, integer frame rate and duration
  in frames. Settings are undoable; shortening across existing keys or layer ranges
  is rejected rather than silently discarding edits.

## Editing

- Rectangle layers with stable IDs, rename, duplicate, ordering, visibility and locking.
- Position, anchor, scale, rotation and opacity; X and Y are separate channels.
- Click a numeric field, type a value, press Enter to apply or Escape to cancel.
  Leaving a field also commits its value. Fields support Unicode text, selection,
  clipboard operations and platform text input.
- Select and drag a rectangle in the composition to move it. X/Y movement is one
  undo step. The displayed corner markers indicate selection; resizing/rotation
  by dragging those markers is not implemented.
- Hand tool pans the composition. Fit resets pan and scale; zoom controls support
  6.25%–800%. The transparency grid can be toggled.
- Layer In/Out fields trim visibility in whole frames; Out is exclusive.

## Animation

Open `examples/motion-study.lfe.json` from the repository root for a three-layer
animation sample. Press Space to play, U to reveal animated properties, and drag
a diamond to change its timing.

- A stopwatch enables animation at the current frame. Once animated, editing a
  property inserts or updates its keyframe at the current frame.
- Disabling a stopwatch removes that property's keys and retains its evaluated
  value at the current frame. Undo restores the animation.
- Diamonds add/remove a key at the current frame. Click a timeline diamond to
  select and seek to it; drag it to move it; Delete removes the selected key.
- Moving a key onto an occupied frame is rejected, preserving both keys.
- Linear, Hold and Smooth interpolation. The interpolation belongs to the outgoing
  key. Smooth is smoothstep, not AE temporal Bezier or Easy Ease.
- Drag the ruler to scrub. Timeline zoom and pan keep frame mapping consistent.
- B/N set the beginning/end of the playback work area. Playback loops within it.
  Work area, viewport position, selection and panel layout are session state.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl+N / Ctrl+O / Ctrl+S | New / Open / Save as |
| Ctrl+Z / Ctrl+Shift+Z | Undo / Redo |
| Ctrl+Y / Ctrl+D | Add rectangle / Duplicate selected layer |
| Ctrl+K | Composition settings |
| V / H | Selection / Hand tool |
| Space | Play / Pause |
| Home / End | First / Last composition frame |
| Page Up / Page Down | Previous / Next frame (Shift: 10 frames) |
| P / A / S / R / T | Position / Anchor / Scale / Rotation / Opacity |
| U | Reveal animated properties |
| J / K | Previous / Next key on the selected layer |
| B / N | Work area start / end |
| + / − | Timeline zoom |
| Delete | Delete selected timeline key, otherwise selected layer |

Help → Keyboard shortcuts lists the controls in the app. Text fields keep typing
isolated from editor shortcuts. Buttons support Tab and Enter/Space.

## Project files and limitations

Versioned .lfe.json files contain the composition, layer ranges, transforms and
keyframes. Files from the initial rectangle editor remain readable. Save as writes
the snapshot captured when clicked using a temporary file before replacement.
The size limit is 16 MiB. Undo history is capped at 100 edits; New/Open are undoable.

Save before closing: autosave and unsaved-close protection are not implemented.
Ctrl+S currently opens Save as, not a silent save to the last path.

Still pending: multiple compositions and precompositions, layer parenting, Bezier
graph editor, text/media/audio layers, masks, effects, 3D, rendering/export, JSX,
ExtendScript and expressions. The preview currently renders rectangles only.
There is no claim of AEP or Adobe script compatibility.

## Development

Use proto use at the repository root for the versions pinned in .prototools.

~~~sh
moon run desktop:dev
moon run desktop:check
moon run desktop:build
cargo test --workspace
cargo fmt --all --check
~~~

When Moon is unavailable, the corresponding local commands are:

~~~sh
cargo run -p libre-effects-desktop
cargo check -p libre-effects-desktop
cargo build -p libre-effects-desktop --release
~~~

The executable is target/debug/libre-effects.exe (or target/release/ for release).
The first GPUI build can take a while.

Windows uses Win32/DirectWrite. macOS requires Xcode command line tools. Linux
requires Vulkan, a C toolchain, cmake, libvulkan1, libwayland-dev, libx11-xcb-dev,
libxkbcommon-x11-dev and libfontconfig-dev. WSLg uses XWayland when available due
to the GPUI 0.2.2/xdg_wm_base version mismatch.
