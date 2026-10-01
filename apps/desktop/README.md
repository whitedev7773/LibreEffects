# Libre Effects Desktop

A Windows-first motion graphics editor built with Rust and GPUI. Its workspace
and basic editing workflow follow After Effects conventions. It is an early 2D
editor, not a complete After Effects replacement or an AEP-compatible application.

Only one editor runs per user, including builds launched from different folders
or executable names. Launching again requests activation of the existing window
and exits before GPUI, media caches or recovery start. The OS releases ownership
after a crash; do not delete `LibreEffects/editor.lock` in the user data directory.
Command-line renders remain independent of the interactive editor.

![Libre Effects workspace with the Content and Motion Study sample](screenshots/workspace.png)

## Workspace

- Compact menu bar and toolbar; Project and Composition above the Timeline, with a
  full-height right dock for Properties / Info / Preview and a separate Align panel.
  The default proportions follow the open After Effects 2026 workspace measured at
  1920 × 1032. The Timeline ends at the right dock; toggling the graph changes only
  its time area, preserving the layer list, composition tab and ruler.
- Project and Timeline search fields filter composition and layer names as you type.
- Preview controls live in the right dock; Info shows the current composition and time.
- Drag panel dividers to resize; double-click a divider or choose Window → Reset
  default workspace to restore the layout.
- Save (Ctrl+S) also records panel proportions, the timeline column width, right
  dock sections, Effect Controls tab, Align target and Snap preference in optional `editor_view`
  metadata. Each composition remembers its playhead, timeline zoom/position,
  preview zoom/pan/resolution, checkerboard and graph visibility.
  View changes do not add Undo steps or mark the render document dirty; save
  explicitly to preserve them across reopening. New documents and recovery
  checkpoints start with default views. Older files without this metadata open
  normally; invalid/future view metadata is ignored and unsafe ranges are clamped.
- The Timeline has compact layer rows, grouped X/Y transform values, a Parent & Link
  column, and a draggable boundary between its layer list and time area.
- Wanted Sans and Gravity Icons are embedded in the executable, with their licenses
  under assets/. No system font installation or runtime download is required.
- Composition settings (Ctrl+K): name, dimensions, exact rational frame rate, duration
  and RGB background color, with a live swatch and Black/White/Slate/Navy presets.
  FPS accepts integers, ratios (`24000/1001`, `30000/1001`) and NTSC shorthand
  (`23.976`, `29.97`, `59.94`, `119.88`). HD/UHD presets set size and rate.
  Duration accepts frames (`240` or `240f`), elapsed seconds (`10s`), or
  `HH:MM:SS:FF`; seconds round to the nearest frame. The 5/10/30/60-second
  buttons set duration. Rates support 1–240 fps and durations up to 24 hours.
  Start timecode is a non-drop-frame display offset before 24:00:00:00. It does
  not shift keys, source sampling or CLI frame numbers. At 29.97 fps, a one-hour
  NDF label spans 3603.6 elapsed seconds; drop-frame numbering is not implemented.
  Changing FPS retains existing frame numbers. The timeline displays a compact
  decimal FPS label; settings and encoding keep the exact numerator/denominator.
  Settings are undoable; shortening across existing keys or layer ranges
  is rejected rather than silently discarding edits.

## Editing

- Rectangle, text and embedded image layers with stable IDs, rename, duplicate,
  ordering, visibility and locking. Layer → New text uses embedded Wanted Sans.
  Properties edits the text, font size and hexadecimal fill color.
- Ctrl+I imports PNG/JPEG images (up to 8 MiB and 4096 × 4096 pixels). Images are
  re-encoded as PNG and stored inside the project, so moving the original is safe.
- Ctrl+Shift+I imports a local video as a linked layer at the playhead. See
  Video footage below for source requirements and relinking.
- Position, anchor, scale, rotation and opacity; X and Y remain separate animation
  channels but share one row. Click X or Y to select the Graph Editor channel.
- Click a numeric field, type a value, press Enter to apply or Escape to cancel.
  Leaving a field also commits its value. Fields support Unicode text, selection,
  clipboard operations and platform text input.
- Drag numeric values horizontally to scrub; Shift changes values faster and Alt
  provides fine adjustment. A gesture commits one undo step, including outside
  the input's bounds; Escape cancels.
- Ctrl-click toggles layer selection; Shift-click selects a layer range. Drag empty
  timeline space to box-select keys or layer bars. Ctrl+A selects all layers, or
  visible keys when keys are selected. Delete/duplicate act on the layer selection.
- Drag selected layers in the composition to move them together in one undo step.
  Selected parent/child groups move once through their selected root ancestors.
  Drag any of the eight handles to scale, or use W/Y for rotation/anchor editing.
  Rotation and scale gestures affect all selected roots around each root's own
  anchor: rotation adds the same angle; scale multiplies by the same axis factors.
  A driver axis starting at zero instead adds its percentage-point change.
  Selected descendants inherit the root transform once. Unlock every selected
  layer first; numeric fields and the anchor tool still edit one layer.
- Hand tool pans the composition. Fit resets pan and scale; zoom controls support
  6.25%–800%. The transparency grid can be toggled.
- Drag layer bars to shift the selected layers and their keys; drag bar edges or
  edit In/Out fields to trim visibility. Out is exclusive. Moves outside the
  composition and edits to locked layers are rejected atomically.
- Align offers six edge/center actions against Composition or Selection bounds.
  Selection alignment needs two independent roots; the six distribution buttons
  need three and space the chosen edges/centers between the existing extremes.
  Rotated/negative-scale source bounds are included; masks and effect expansion
  are excluded. Selected parents carry selected children without a second edit.
  Each operation commits one undo step; animated positions receive a current-frame
  key. Alignment through a zero-scale parent is rejected when movement is needed.

## Viewer guides and channels

- Composition's **Guides** menu (also View) toggles rulers, grid, guides and the
  90% action-safe / 80% title-safe rectangles. Ctrl+R toggles rulers. Fit reserves
  ruler space; guide positions, grid spacing and labels follow composition pixels
  through zoom and pan. The grid menu cycles 50 / 100 / 200-pixel spacing.
- Drag from the top ruler for a horizontal guide or the left ruler for a vertical
  guide. Drag a guide to move it; drag back onto a ruler or outside the viewer to
  remove it. Escape cancels; Lock guides prevents editing and clearing.
  Up to 256 guides per composition are saved with the document (version 20), and
  add/move/remove/clear support Undo/Redo. Duplicate composition copies guides;
  a new composition or precomposed source starts without them.
- Guide/grid snapping moves selected layers as one unit using their transformed
  source edges and centers. It activates within 8 logical pixels, independent of
  zoom; Alt bypasses it. Only visible guides/grid snap. Rotation and scale gestures
  are unaffected. Grid subdivisions under 8 screen pixels are hidden for clarity.
- The channel menu shows RGB, Red, Green, Blue or Alpha. Individual channels use
  opaque grayscale; the color channels show straight RGB. Info always samples
  unmodified RGBA before the background, selection outlines and layout aids.
  It reports composition X/Y and the sample buffer resolution; Half/Quarter or
  the existing 1280-pixel preview cap can differ from a full-resolution export.
- Both viewer menus support Up/Down, Enter/Space and Escape. Display preferences
  are saved per composition in `editor_view`; they do not create Undo steps or
  mark the document dirty. Save explicitly after changing only view preferences.
  Guides and all display preferences are excluded from PNG, MP4, MOV and nested
  compositions. No guide preset import/export or custom ruler origin is provided.

## Parenting

Layer > New null object creates an invisible 100 × 100 transform controller,
with its anchor at the upper-left corner. Its outline is a viewer overlay and
is never rendered into PNG/video or a nested composition. Assign children through
Parent & Link; its animated transforms affect children even when the Null is hidden.
Drag the Gravity link icon beside the parent dropdown onto another layer's name
for Pick Whip parenting. Valid targets highlight; Escape cancels. The current
world pose is preserved by a transform offset. Self/circular links, locked children,
zero-scale parents and a drag from an outdated document are rejected. Clicking the
icon also opens the existing keyboard-operable parent menu.

### Layer switches and clipboard

Open `examples/layer-workflow.lfe.json` to try a Shy Null moving a child panel
under a full-width Guide line. Hide Shy removes only the controller's timeline row;
export excludes the line while keeping the animated panel.

- Timeline Solo isolates enabled solo layers in both preview and output. Visibility
  and In/Out still apply; soloing a hidden, out-of-range or non-rendering layer can
  leave an empty frame. Parent transforms continue to affect soloed children.
- Mark layers Shy, then enable Hide Shy above the timeline to hide their rows.
  Their rendered pixels remain unchanged. Timeline range selection and Ctrl+A skip
  hidden Shy rows. Hide Shy and each switch are saved and support Undo/Redo.
- Guide layers appear only in their own composition preview. PNG/MP4/MOV and
  containing compositions exclude them. Solo and Guide remain independent switches.
  Locked layers must be unlocked before changing their switches.
- Ctrl+C / Ctrl+X / Ctrl+V copy, cut and paste selected keys when keys are selected,
  otherwise layers. Edit > Copy layers / Paste layers explicitly selects layer mode.
  Layer copies preserve stack order, properties, masks, effects, switches and shared
  image assets; pasted IDs are new and copied parent links are reconnected.
- Paste inserts above the selected layer and retains timeline times. Between
  compositions in the same project, seconds are preserved to the nearest destination
  frame, including video/nested source origins. Key collisions and too-short target
  durations are rejected without changing the document. Copy the complete parent
  hierarchy when pasting into another composition; local copies can retain an
  existing external parent. Circular or missing composition references are rejected.
- The editor clipboard is in memory and is cleared by New/Open/Recovery. Copying keys
  replaces copied layers and vice versa. Cross-project/OS layer clipboard and
  paste-at-playhead layer timing are not implemented.

These switch semantics follow Adobe's [layer switches](https://helpx.adobe.com/after-effects/desktop/work-with-layers/manage-layers/layers.html)
and [Null/Guide layer descriptions](https://helpx.adobe.com/after-effects/desktop/work-with-layers/layer-properties/layer-properties.html).

### Parent links

Timeline or Properties → Parent & Link selects a parent or None. Connecting, reparenting and
disconnecting preserve the current frame's full 2D pose, including rotated,
nonuniform and negative scales. Descendants follow parent transforms and remain
independent in visibility, opacity and layer timing. Canvas dragging accounts for
the parent's transform and commits one undo step.

Parent relationships and compensation matrices are saved in the project. Numeric
transform fields remain local to the layer's compensated coordinate system.
Disconnecting an animated parent preserves only the current pose, not the parent's
motion over the whole composition. Cycles and missing parents are rejected; zero
scale parents cannot be assigned. Deleting a parent detaches surviving children
while preserving their current pose. A locked affected child prevents the edit.

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
- Ctrl/Shift-click diamonds or drag a selection box to select multiple keys. Drag
  selected keys to shift them together across layers/channels in one undo step.
  Ctrl+C / Ctrl+V copies and pastes their relative timing and interpolation at the
  playhead. A single source layer pastes to the selected layer; multiple source
  layers retain their original layer mapping. Clipboard contents are session-local.
- Linear, Hold and Smooth interpolation. The interpolation belongs to the outgoing
  key. Smooth is smoothstep, not AE temporal Bezier or Easy Ease.
- Temporal cubic Bezier interpolation, including overshoot. The curve solver
  inverts time before evaluating progress. Preview opacity is clamped to 0–100%.
- Drag the ruler to scrub. Timeline zoom and pan keep frame mapping consistent.
- B/N set the beginning/end of the playback work area. Playback loops within it.
  Work area is saved with the composition and supports Undo/Redo. View state is
  saved separately from render settings; selections remain session state.

### Timeline markers and snapping

Use **Comp Marker** or **Layer Marker** to add an annotation at the playhead.
Click a flag to seek and edit its name, frame, duration and RGB hex color.
Previous/Next traverses composition markers and the selected layer's markers.
Markers support Undo/Redo and project-file round trips; they never render into
the composition image or exports. `examples/effect-study.lfe.json` includes
Reveal/Hold ranges and a layer cue at the end of its blur animation.

Markers use composition frames. Moving a layer moves its markers; trimming keeps
their times. Splitting clips a range into both halves. A split that would put two
right-hand markers at the same frame is rejected. Clipboard FPS conversion also
rejects merged starts or a nonzero duration rounded to zero. Pre-compose moves
layer annotations into the source and keeps composition markers in the parent.

**Snap** aligns ruler scrubbing, dragged keys and layer moves/trims with the
playhead, work area, layer boundaries, keys and marker endpoints within eight
logical pixels. Hold Alt to bypass it. Selected keys move with one shared offset;
hidden Shy layers are excluded. Save records the Snap preference with workspace metadata.

## Graph Editor

![Opacity value graph in the timeline](screenshots/graph-editor.png)

Click the graph icon in the timeline, use Animation → Toggle Graph Editor, or press
Shift+F3. Click a property label or its X/Y channel in the persistent layer list. The value graph shares timeline
zoom and pan; its vertical range fits the visible curve.

- Click a key to select it; drag to change both frame and value. Release commits
  one undo step. Escape cancels; occupied frames and invalid values are rejected.
- Keyframe... opens a compact popup with Frame and Value fields for precise edits.
  Escape or Close dismisses it. Delete removes only the selected
  graph key. The diamond adds/removes a key at the playhead.
- Linear, Hold, Ease, Ease In and Ease Out presets affect the selected key's
  outgoing segment. F9 applies Ease while the graph has focus.
- Drag the two blue handles directly on the value graph, or use the outgoing
  segment chart and X1/Y1/X2/Y2 fields in the popup. Flat segments use the popup.
  X is normalized time (0–1); Y is normalized progress (−2–3). Handles beyond the
  small chart's vertical range remain accessible through numeric fields.
- This is a single-channel value graph with normalized segment easing. AE's speed
  graph, spatial Bezier paths, multi-key graph editing, and linked incoming/outgoing
  velocity handles are not implemented. F9 is not full AE Easy Ease compatibility.

Open `examples/curve-parent-study.lfe.json` for an overshooting Bezier animation
with a child layer. The source generator is
`crates/core/examples/make_animation_study.rs`:

~~~sh
cargo run -p libre-effects-core --example make_animation_study -- examples/curve-parent-study.lfe.json
~~~

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl+N / Ctrl+O / Ctrl+S | New / Open / Save |
| Ctrl+Shift+S / Ctrl+I | Save as / Import image |
| Ctrl+Shift+I | Import video footage |
| Ctrl+C / Ctrl+V | Copy / Paste selected keys |
| Ctrl+Z / Ctrl+Shift+Z | Undo / Redo |
| Ctrl+Y / Ctrl+Alt+Y / Ctrl+D | Add solid / Add adjustment layer / Duplicate selection |
| Ctrl+K | Composition settings |
| Ctrl+Alt+T | Enable / disable selected video or precomposition Time Remap |
| V / H / W / Y | Selection / Hand / Rotation / Anchor Point tool |
| Ctrl+Shift+D | Split selected layers at the playhead |
| Alt+[ / Alt+] | Trim selected layers' In / Out to the playhead |
| Arrow keys / Shift+Arrow | Move selected layers 1 / 10 composition pixels |
| Space | Play / Pause |
| Home / End | First / Last composition frame |
| Page Up / Page Down | Previous / Next frame (Shift: 10 frames) |
| P / A / S / R / T | Position / Anchor / Scale / Rotation / Opacity |
| U | Reveal animated properties |
| Shift+F3 | Toggle Graph Editor |
| F9 (graph focused) | Ease selected key's outgoing segment |
| J / K | Previous / Next key on the selected layer |
| B / N | Work area start / end |
| + / − | Timeline zoom |
| Delete | Delete selected timeline keys, otherwise selected layers |

Help → Keyboard shortcuts lists the controls in the app. Text fields keep typing
isolated from editor shortcuts; Save, New, Open and Alt+F4 commit the field before
continuing. Buttons support Tab and Enter/Space.

The eight selection handles scale a layer around its anchor; Shift preserves the
existing scale ratio. W rotates the clicked layer around its anchor, with Shift
snapping to 15° increments. Y moves the anchor and compensates Position so the
content stays in place at the current frame. These tools account for transformed
parents, preview while dragging, commit as one undo step, and cancel with Escape.
Scale, rotation and anchor gestures edit the clicked layer; Selection-tool movement
moves the selected group. Existing animation tracks receive keys at the playhead.
The W/Y shortcuts and Shift rotation increments follow Adobe's
[current keyboard reference](https://helpx.adobe.com/sg/after-effects/desktop/get-started/keyboard-shortcuts/keyboard-shortcuts-reference.html).

Edit → Split layers divides each selected layer at the playhead, retaining its
animation tracks and remapping parent links within the new group. Every selected
layer must contain the split frame; invalid or locked selections leave the project
unchanged. Group duplication also preserves parent links within the duplicated
group, while links to unselected parents keep their original targets.

## Masks, effects and rendering

Properties provides a rectangular layer-space mask with inversion. The Effect menu
or the right dock's Effects & Presets search adds an effect and opens the left
Effect Controls tab. The stack supports rename, duplicate, reorder, bypass, reset
and removal, with up to 64 instances per layer. Available effects are Gaussian
Blur, Brightness, Grayscale, Fill, Tint, Hue/Saturation, Levels, Drop Shadow, Glow,
Curves, Linear Gradient and Radial Gradient.

Use a parameter's stopwatch to enable animation, its diamond to add/remove a key,
and its arrow buttons to visit keys. Editing an animated value inserts a key at
the playhead. The interpolation control cycles Linear/Hold/Smoothstep/Bezier for
the outgoing segment. Disabling animation retains the sampled value; Reset removes
that effect's keys. Undo restores each operation. Parameter ranges also clamp
Bezier overshoot during sampling. Effect keys move, split, copy and convert FPS with
their layer, and shortening a composition cannot discard them.

Effects run top to bottom in layer coordinates, after the rectangular mask and
before layer opacity, transforms and composition blending. New effects process
sRGB channels; existing fixed effects retain their original linear-RGB evaluation.
Effect Controls > Edit as ordered effects converts the old Blur → Grayscale →
Brightness chain without changing its color space or adding intermediate rounding.
Until converted, those existing effects run before newly added effects.

Fill replaces RGB while preserving source alpha, multiplied by its opacity.
Tint maps luminance between dark/light RGB values with an adjustable amount.
Levels uses a 257-sample input-black/input-white/gamma table; equal endpoints make
a threshold and reversed endpoints invert the range. Shadow is placed behind the
source; Glow adds a blurred copy to source RGBA. Blur uses a four-radius padding
budget. Cumulative effect regions over 32 megapixels fail with an error. Point text
uses shaped glyph bounds so applying effects does not crop long or multiline text.

Open `examples/effect-study.lfe.json` for animated blur, shadow and glow on point text.
Regenerate it with `cargo run -p libre-effects-core --example make_effect_study -- examples/effect-study.lfe.json`.

Mask values remain static. Effect parameters also appear under their effect names
in the timeline and participate in animated-property filtering, marquee selection,
key dragging, Copy/Cut/Paste, Delete and previous/next key navigation. Click an
Effect Controls parameter label to open its value graph. Graph key time/value and
outgoing Bezier handles use the same validated commands as transform properties.
Copying effect keys to another layer requires the same effect instance ID and kind;
a missing or mismatched destination is rejected atomically. Effect IDs remain stable
when the stack is reordered. Removing an effect clears stale key/graph selections.
Preset saving and additional effect families remain in the backlog.
The same
resvg compositor renders both the composition preview and exported frames,
including text, images, parenting, interpolation, layer timing and alpha.

Curves has five fixed input points (0/25/50/75/100%) for the master RGB curve and
each red, green and blue channel. Select a channel above the graph, drag vertically,
or use Left/Right to select a point and Up/Down to change its output (Shift: 10).
Escape cancels a drag; releasing commits one Undo step and updates the composition.
Numeric output values range from 0 to 255; each point supports the usual keyframes.
The master is applied before the individual channel curve, using shape-preserving
cubic interpolation and a rounded 256-entry map in the 8-bit sRGB renderer.
Alpha is preserved. Arbitrary point placement, pencil curves and ACV/AMP import
are not implemented.

Gradients use layer-space start/end positions, start/end RGB and Blend with original
(0–100%), all animatable. Linear uses the start-to-end vector; Radial uses the start
as center and its distance to the end as radius. Outside endpoints the colors hold;
coincident endpoints produce the end color. The preceding effect stage's alpha
limits the gradient, including blur or shadow extents. Reset initializes endpoints
from the current source dimensions; resizing a source keeps existing endpoints.
Curve/gradient projects use version 19. Open `examples/tonal-color-study.lfe.json`
for animated contrast and a moving radial center. Gradient scatter/dithering and
on-canvas endpoint handles remain in the backlog.

File → Export current frame (PNG, alpha) writes a full-resolution RGBA PNG. Render work area
writes a PNG sequence to a new subfolder of the chosen directory, with frame rate,
dimensions, frame range and completion status in `sequence.json`. Cancel render
stops after the current frame and keeps completed files. Export uses a project
snapshot, so editing during a render does not change its output. The preview is
limited to 1280 pixels on its longest side; exports support up to 32 megapixels.
Click Full / Half / Quarter below the composition to reduce preview resolution
for faster interaction. This changes only the preview; every output uses the full
composition resolution.

File → Render work area — MP4 exports H.264 (CRF 18, yuv420p, fast-start). Transparent
pixels are composited over the composition background color (black by default).
Odd dimensions are padded using the same color by one pixel on the
right/bottom for H.264 compatibility. Render work area — MOV with alpha exports
ProRes 4444 with transparency and the exact composition dimensions. Both use the
composition frame rate and the B/N work area; the first output frame is the work
area's first frame. These presets currently export silent video.
Exact fractional clocks are passed directly to FFmpeg. When a nonzero start
timecode is set, MP4/MOV include its NDF timecode plus the work-area offset.
PNG sequence manifests record the exact rate, first-frame timecode and NDF format.

### Composition background and transparent output

Ctrl+K → Background (RGB) accepts six-digit hex colors, such as `#26384A`. The
color is saved in the project, supports undo/redo, and changes neither the layers
nor their alpha. Existing projects default to black.

- With the transparency grid off, the composition viewer displays the background
  behind transparent and partly transparent pixels. The grid reveals alpha and is
  only a viewing aid; it is never exported.
- MP4 always composites against the background color captured when rendering
  starts. Translucent text/shape edges are blended before alpha is removed. The
  render strip names the background color, and any H.264 padding uses that color.
- PNG and PNG sequence menus offer **alpha** and **background** variants. Alpha
  preserves transparency; background writes opaque RGBA pixels matching the matte
  shown in the viewer. Sequence manifests record the chosen alpha/background policy.
- MOV with alpha retains transparency regardless of the composition background.
  To make the background part of every format, choose **Layer → New background
  solid**. It adds an editable, independent solid beneath existing layers
  using the current background color, in one undo step. Its size and color are
  copied at creation; later composition changes do not resize or recolor it.

Video export requires FFmpeg with `libx264` and `prores_ks` on PATH. Alternatively,
set `LIBRE_EFFECTS_FFMPEG` to the full executable path before launching the app.
FFmpeg is not bundled or downloaded automatically. A missing executable/encoder
is reported in the render status. Use a `.mp4` or `.mov` filename matching the preset.

The render strip shows the preset, frame range, progress and destination. Editing
can continue while a snapshot renders. Cancel interrupts the encoder, removes
temporary output and preserves an existing destination. A completed video replaces
the destination only after successful encoding; an encoder stalled for 120 seconds
is stopped. Wait for completion or cancel before closing the application.

### A complete 2D motion-graphics delivery

1. Open `examples/content-study.lfe.json`, or create a composition with Ctrl+K.
2. Add text/rectangles from Layer, or import a PNG/JPEG with Ctrl+I.
3. Enable a transform stopwatch, move the playhead and change the value to animate.
   Use P/S/R/T and the Graph Editor to refine motion; save with Ctrl+S.
4. Set B/N for the output range. Alt+[ and Alt+] trim layers without moving their
   animation; the Out trim includes the frame under the playhead. Arrow keys nudge
   in composition pixels, even under transformed parents, without moving a selected
   child twice when its parent is also selected.
5. Choose the MP4 preset for viewing/sharing, or MOV with alpha for another compositor.
   Dismiss the completed render strip to recover the full editing workspace.

This workflow supports short 2D titles, animated graphics, linked video footage
and transparent overlays. Audio mixing and the full AE workflow remain pending.

`examples/lower-third.lfe.json` is a 1920×1080, 30 fps, five-second transparent
name/title overlay with entry/exit animation. Edit the Name and Role layers, then
render MOV with alpha to place it over footage in another editor. Its first and
last frames are transparent; frame 30 is useful for editing the visible title.

![Completed ProRes 4444 alpha render from the Windows release application](screenshots/video-export.png)

Regenerate the template with:

~~~sh
cargo run -p libre-effects-core --example make_lower_third -- examples/lower-third.lfe.json
~~~

Open `examples/content-study.lfe.json` for a text/mask/effects sample.
Regenerate it with `cargo run -p libre-effects-core --example make_content_study -- examples/content-study.lfe.json`.

## Video footage

![Linked video and an animated title rendered to 1080p MP4](screenshots/footage-composite.png)

File → Import video (Ctrl+Shift+I) reads a local video through FFprobe and FFmpeg,
verifies its first frame, and adds a layer at the playhead. Its Out point is the
source duration or composition end, whichever comes first. Oversized footage is
scaled down to fit; smaller footage retains its native dimensions. Use layer order
controls to put footage below a title, then save the project and render MP4/MOV.

- Supports square-pixel, constant-frame-rate video at 1–240 fps, up to 4096 × 4096
  and 24 hours. MP4/MOV/MKV/WebM support depends on the installed FFmpeg codecs.
  Convert variable-frame-rate, anamorphic or HDR footage to SDR CFR first. Source
  frame-rate metadata is checked, but this is not a full VFR timestamp scan.
- Video stays linked while editing. Save records sources beneath the destination
  project folder as relative paths; sources outside it remain absolute. Reopening
  resolves relative paths against the project file, including offline sources.
  Save As does not change live source paths, Undo history or the original footage.
- Scrubbing samples the preceding source frame at composition time. Different
  source/composition rates duplicate or drop frames; there is no frame blending.
  Trim changes visibility without slipping the source. Moving a clip shifts its
  source origin and keys; splitting retains source continuity in both halves.
- Transforms, parenting, opacity, rectangular masks and effects apply to footage
  through the same compositor used for preview, PNG and video output.
- Missing sources produce an explicit preview error and fail output without
  replacing an existing destination. Select the video and use Relink source in
  Properties or File → Relink selected video. Relink is undoable and retains layer
  timing/framing; it updates all references to that source across compositions,
  including locked instances. Replacement dimensions must match every instance. A shorter replacement is
  transparent beyond its duration. File → Refresh footage retries after restoring
  or replacing a file at the same path.
- Preview decoding runs in the background with one request in flight and a bounded
  32 MiB / 24-frame PNG cache. Playback may skip preview frames; uncached frames are
  decoded on demand and real-time playback is not guaranteed. Output renders every
  frame. Frame decoding times out after 15 seconds; cancellation can wait for the
  current source-frame decode. Preview resolution does not reduce output quality.
- Audio is not imported, played or exported. Color processing is 8-bit RGBA and is
  not an HDR/color-managed workflow. Source files must stay unchanged during export;
  project snapshots preserve edits, not the external file bytes.

### Portable projects and missing media

File → Manage project media lists unique linked videos, their layer reference
counts and Online/Missing status across all compositions. Refresh rechecks disk.
Locate selects one replacement; Relink missing from folder searches a chosen
folder and its children, verifies the codec/first frame, and applies the resolved
sources as one undoable edit. Ambiguous filenames are left unresolved with a
report. Search skips symbolic links and is limited to 10,000 entries / 16 levels.
If a checked replacement has incompatible dimensions, the entire edit is rejected.

File → Collect project files creates a new `LibreEffects-collected-*` directory
inside the chosen parent, with `project.lfe.json` and a `Media` folder. Linked
videos are copied once per canonical source with unique numbered filenames;
embedded images and editor views stay in the JSON. The current project and source
files remain unchanged. Move the complete new folder to relocate the project.
Copy errors, changed source files and cancellation remove the incomplete new
collection. File → Cancel file collection stops copying between 1 MiB chunks.
The completion path or error also remains in the Project Media dialog.
Collected projects contain local file bytes, not external fonts, FFmpeg binaries
or a package installer. Existing embedded-image limits still apply.

### Video speed, reverse and freeze

![Reverse playback in the existing Properties panel](screenshots/video-retiming.png)

Select a video layer and use Properties → Content. The workspace layout
is unchanged; playback controls appear only for video layers.

- **Speed (%)** changes how fast the source advances: 50 is half speed, 200 is
  double speed, and 0 holds the source at the layer's In point. Negative values
  play backward. Supported nonzero magnitudes are 1–10000%.
- **Source In (s)** sets the source time shown at the layer's current In point,
  allowing a different section of the original video to occupy the same timeline
  slot. Speed changes preserve this source time, including after trimming.
- **Reverse** reverses the currently visible discrete frame interval. It starts
  with the frame previously shown at Out minus one and ends with the old In frame,
  including when source and composition frame rates differ. Trim to valid source
  frames first if the layer extends beyond the source. Reversing twice restores
  the original frame sequence.
- **Freeze at playhead** holds the sampled source frame for the entire layer.
  Put the playhead on a visible source frame first. Set Speed back to 100 to resume
  forward playback from the held frame at the layer's In point, or undo to restore
  the previous mapping.
- The current source time, or an outside-source message, appears under the controls.
  These edits preserve layer In/Out, transform keys, parenting, masks and effects.
  Faster or slipped footage can run out before Out; those frames are transparent
  (the composition background in MP4). Extend/trim the layer explicitly when changing its duration.
- Every operation supports undo/redo and survives saving, relinking, moving and
  splitting. Preview, PNG sequences and MP4/MOV use the same source-time mapping.
  These are the base constant-speed controls. Use Animated Time Remap below for
  variable playback. Automatic layer/keyframe stretching and audio retiming remain pending.

FFprobe must be on PATH alongside FFmpeg. `LIBRE_EFFECTS_FFPROBE` overrides its
executable; when `LIBRE_EFFECTS_FFMPEG` is absolute, FFprobe defaults to that same
directory. Neither tool is downloaded automatically.

### Animated Time Remap

Layer → Enable Time Remapping (Ctrl+Alt+T) adds a source-seconds track to video
and precomposition layers. The first and last visible frames become keys, keeping
the current trim, shifted origin and base speed unchanged on activation. Edit the
Time Remap row in the Timeline, the Properties source-time field, or its value
graph to accelerate, slow down, hold or reverse parts of the source. Keys support
the same selection, copy/paste, movement, interpolation and Undo/Redo as effect keys.
The stopwatch can remove animation while keeping the sampled source time constant.
Disabling Time Remapping removes the track and restores the preserved base timing.
Base video Speed/Source In controls are hidden while remapping is enabled.

Freeze frame with Time Remap replaces its keys with one Hold key at the current
displayed source frame. The playhead must be inside the layer and a valid source.
Source times before zero or at/after the source duration produce transparency.
Before the first and after the last key, the source time holds at the endpoint;
extend the layer Out point to display this hold. Trimming keeps keys in place,
moving a layer shifts its keys, and cross-FPS layer paste converts key positions
without changing their values in seconds. Layer transforms/effects keep using
composition time while the nested source evaluates at remapped time.

Projects with remapping use version 21. Preview, PNG, MP4 and alpha MOV use the
same preceding-source-frame sampling; frame blending, optical flow and audio
retiming are not implemented. `examples/time-remap-study.lfe.json` compares four
instances of one animated source: original, fast/slow, hold and reverse.

Validation covers rational FPS, shifted/trimmed/reversed/frozen sources, out-of-range
transparency, nested sampling, layer split/pre-compose, cross-FPS clipboard, invalid
edits, and project round trips. Preview/PNG pixels and MP4/alpha MOV source colors,
alpha, frame count and background composition are checked by regression tests.
Native verification includes source-time editing, graph dragging, freezing,
Undo/Redo, Ctrl+Alt+T and reopening the edited project with its source time intact.

## Project files and recovery

### Multiple compositions

Use Composition > New / Duplicate / Delete composition, or the Project panel's
plus button. Click a composition in Project or its viewer tab to activate it.
Each composition has independent dimensions, frame rate, duration, background,
layers and B/N work area. Ctrl+K edits the active composition's settings and name.
The project supports up to 100 compositions and 1,000 total layers. Duplicate
assigns new layer IDs and reconnects parent links within the copy. Create,
duplicate and delete support Undo/Redo; the last composition cannot be deleted.
Switching tabs does not create an edit or mark a saved document dirty. Key selections
reset; the playhead and view settings restore independently for each composition.
The active composition is captured at save/export time. All compositions and
their shared image assets are saved together.

### Pre-composing and nesting

Open `examples/precomposition-study.lfe.json` for a reusable animated title
inside a master composition with its own MP4 background.

Select consecutive unlocked layers and use Layer > Pre-compose selection
(Ctrl+Shift+C). This moves all attributes into a new source composition, keeping
the original canvas size, frame rate and timeline origin. Include the complete
parent hierarchy; selections with gaps or links across the boundary are rejected
to preserve stacking and animation. The replacement stays in the active timeline
and spans the selected layers' range. Undo restores the original layers in one step.
Use its Properties > Open source composition button or the viewer tabs to edit
the source. Project rows have a plus button to insert an existing composition at
the active playhead. Renaming uses Ctrl+K in the source composition.

Nested layers retain alpha, support transforms/masks/effects, and sample the
preceding source frame using exact rational FPS conversion. Trimming preserves the
source origin; moving shifts it. A source's background color is a viewer/output
matte, so it is not baked into its nested layers; use a background solid when needed.
Changing a source canvas retains the existing instance's layer dimensions.
Circular/missing references and deleting a referenced composition are rejected.
Nesting is limited to 16 levels, 4,096 rendered layer instances and a 64 MiB SVG
description per frame. Unsupported frames fail without replacing an export.
Animated Time Remap can retime each instance independently. Collapse-transform
controls and the alternative "leave attributes" pre-compose mode remain pending.
Clear Solo switches and disable Guide on selected layers before pre-composing;
otherwise moving the layers into a nested composition would change their visibility.

Versioned .lfe.json files contain compositions, layer ranges, transforms and
keyframes. Files from the initial rectangle editor remain readable. Bezier or
parenting edits upgrade the project to version 2; text, images, masks or effects
upgrade it to version 3; linked videos require version 4, and altered video playback
requires version 5. Nonblack composition backgrounds require version 6; shared embedded image assets require version 7, saved work areas require version 8, multiple compositions require version 9, nested layers require version 10, Null/Solo/Shy/Guide require version 11, ordered animated effects require version 12, timeline markers require version 13, rational FPS/nonzero start timecode require version 14, and portable source-path rewrites use version 15. Integer FPS files remain readable; fractional rates serialize as `{numerator, denominator}`. Older applications reject
unsupported versions. Save writes
the snapshot captured when clicked using a temporary file before replacement.
Files are limited to 256 MiB, with up to 128 MiB of unique base64 image data and
16 MiB of compact metadata when saving. Image payloads are stored once in the
version 7 asset table; duplicates and Undo snapshots share immutable image memory.
Older inline-image projects are upgraded when opened. Undo history is capped at
100 edits and is reset at a
New/Open document boundary. Failed saves leave the edited project intact.

An asterisk in the window title indicates unsaved changes. Ctrl+S saves to the
current file; Ctrl+Shift+S chooses another path. Closing, New and Open offer
Save / Discard / Cancel when there are changes. Save and continue proceeds only
after a successful save. Canceling a file picker does not discard the document.

Changed unsaved projects are checkpointed every five seconds in a uniquely named
slot under `%LOCALAPPDATA%/LibreEffects/recovery/`. Each running instance holds an
OS file lock; other instances neither offer nor remove its checkpoints. Each slot
keeps a current and previous checkpoint. After an interrupted session, startup
lets you browse abandoned slots and Restore, Discard, or keep them for later.
A damaged current checkpoint falls back to the previous one when it is readable.
Restore first copies the project into the current instance's slot and opens an
unsaved document; saving the original is an explicit later action. Closing or
switching documents cleans only the current slot, and queued older writes cannot
recreate it. Corrupt unreadable slots are preserved and reported. Small lock files
remain on disk. Old `recovery.lfe.json` files are preserved untouched and can be
restored using File > Open, including while an older application is running.

Render preflight rejects output destinations that identify a linked source file
(including hard links), and GUI/CLI exports also protect the open/input project.
Missing visible footage and compositions over the 32 megapixel output limit fail
before video encoding. New composition settings enforce that pixel limit.

SDR output assumes nonlinear sRGB working pixels, explicitly converts to a BT.709
YCbCr matrix at limited range, and records BT.709 primaries with the sRGB transfer
function in MP4/ProRes metadata. ProRes retains alpha. This defines the current
8-bit output policy; ICC-aware import, monitor transforms, HDR and linear-light
compositing remain pending. Playback applications may handle color tags differently.

## Render from the command line

Use `--render PROJECT --output FILE` to render a saved project without opening
the editor or changing its recovery slot. The output extension selects MP4/H.264,
MOV/ProRes with alpha, or a single PNG. MP4 uses the composition background; PNG
preserves transparency unless `--png-background` is supplied. The saved active
composition is used by default; `--composition ID` selects another stable ID
from the project JSON (`composition_id` or an `other_compositions` key).

`--start FRAME` is inclusive and `--end FRAME` is exclusive. Video defaults to
the whole composition; PNG defaults to one frame starting at frame zero.
Invalid ranges fail before replacing the output. Successful renders replace the
destination atomically. Exit status is zero on success and one on failure.

On Windows, wait explicitly for the GUI-subsystem executable and redirect its
messages when running in a script:

~~~powershell
$job = Start-Process -FilePath '.\target\release\libre-effects.exe' -ArgumentList '--render examples/lower-third.lfe.json --output title.mp4 --start 0 --end 150' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput render.log -RedirectStandardError render-error.log
$job.ExitCode
~~~

Quote paths containing spaces inside the argument string. For an opaque still,
use `--output title.png --start 30 --png-background`. Use `--help` for syntax.

## Remaining limitations

See [the development backlog](DEVELOPMENT_BACKLOG.md) for the current capability
audit, priorities, dependencies and proposed acceptance criteria.

Still pending: frame blending/optical flow, audio footage,
audio output, freeform/animated masks, additional effects and reusable presets,
rich text layout, 3D, JSX, ExtendScript and expressions. PNG sequences can be
assembled in an external video tool.
There is no claim of AEP or Adobe script compatibility.

## Development

Use proto use at the repository root for the versions pinned in .prototools.

~~~sh
moon run desktop:dev
moon run desktop:check
moon run desktop:build
cargo test --workspace
moon run desktop:test
moon run desktop:test-media
cargo fmt --all --check
~~~

Optional FFmpeg integration checks encode and decode MP4/ProRes, check work-area
timing and alpha, and cancel an active encoder while preserving the destination:

~~~sh
cargo test -p libre-effects-desktop -- --ignored
~~~

The dedicated Windows desktop CI installs FFmpeg and runs checks, formatting,
workspace tests, the explicit media suite and a release build. The Moon desktop
`test` task joins normal CI; `test-media` is explicit because it needs FFmpeg.

These six tests are explicitly ignored in the default suite when FFmpeg/FFprobe
are not declared test dependencies. They also check footage import, different
frame rates, the final source frame, missing media, and composited video output.
Retiming checks compare preview pixels and encoded frames for reverse, slow-motion,
freeze and slipped footage after a project-file round trip.
Background checks encode colored/white mattes with semitransparent layers, verify
odd-dimension padding, and ensure ProRes alpha remains transparent.
The MP4/ProRes round-trip test also places a full-frame Guide above the scene and
verifies that it is excluded from both encoded formats.

When Moon is unavailable, the corresponding local commands are:

~~~sh
cargo run -p libre-effects-desktop
cargo check -p libre-effects-desktop
cargo build -p libre-effects-desktop --release
~~~

The executable is target/debug/libre-effects.exe (or target/release/ for release).
The first GPUI build can take a while.
The Windows release executable opens the editor without an extra console window.

Windows uses Win32/DirectWrite. macOS requires Xcode command line tools. Linux
requires Vulkan, a C toolchain, cmake, libvulkan1, libwayland-dev, libx11-xcb-dev,
libxkbcommon-x11-dev and libfontconfig-dev. WSLg uses XWayland when available due
to the GPUI 0.2.2/xdg_wm_base version mismatch.

On Windows, GPUI also needs the Windows SDK shader compiler. If a clean build
reports `Failed to find fxc.exe`, set `GPUI_FXC_PATH` to the installed SDK's x64
compiler before running Cargo or Moon, for example in PowerShell:

~~~powershell
$env:GPUI_FXC_PATH = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\fxc.exe'
cargo build -p libre-effects-desktop --release
~~~

Use the SDK version installed on your machine; the compiler is only needed to
build the application.

## Solid and adjustment layers

Use **Layer → New solid** (Ctrl+Y) to create a white source matching the current
composition. **Properties → Solid settings** edits its color, source width and
height independently. **Make comp size** copies the current canvas dimensions.
Source resizing preserves the layer origin, anchor, transform animation and mask;
use the existing anchor/position controls to recenter it if desired. Duplicates
are independent sources. Existing Rectangle layers retain their old behavior.

**Layer → New adjustment layer** (Ctrl+Alt+Y) contributes no image of its own.
Its ordered effects process the visible, active composite below it, within the
same composition. Higher layers remain unaffected. Transform and parent settings
move its rectangular coverage and effect coordinate space. A rectangular mask
(including inversion) limits coverage after filtering. Opacity animates the blend
between original and filtered premultiplied RGBA without doubling transparency.
Guide/Solo/range switches follow the existing compositor rules. Pre-compose must
include all layers below a selected adjustment to retain its input.

Projects using these source types require version 16. Preview, PNG and video use
the same compositor; opaque export applies the composition background afterward.
Adjustment boundaries rasterize at the requested preview/export resolution in
8-bit sRGB; this is not a linear-light/HDR or continuously rasterized pipeline.
Intermediate frames retain the existing 32 MP and 64 MiB encoded-image limits.
`examples/adjustment-study.lfe.json` demonstrates animated, masked desaturation
of a translucent solid while an upper blue solid keeps its color.

## Layer blending

The timeline **Mode** column and **Properties → Blend mode** choose Normal,
Multiply, Screen, Add or Overlay. The Mode column appears when the timeline's
left pane is at least 540 logical pixels wide; the property control is always
available for renderable layers. Open with click/Enter, navigate with Up/Down,
confirm with Enter and close with Escape or an outside click. Locked layers and
Null objects cannot change mode. The choice supports Undo/Redo and independent
copy/duplicate, and is saved in project version 17.

RGB blending operates on straight sRGB channel values, followed by premultiplied
source-over with alpha `As + Ab * (1 - As)`. Normal, Multiply, Screen and Overlay
follow the [W3C separable blending equations](https://www.w3.org/TR/compositing-1/#blending).
Add saturates the sum of straight channels before the same source-over step;
it does not add alpha. Colors in fully transparent pixels do not contribute.
Effects, transforms, masks and layer opacity are evaluated before layer blending.
The composition background is applied only when a render format requests a matte.

On an Adjustment Layer the blend function mixes filtered and original colors in
place, weighted by the original alpha, while retaining the filtered alpha. Its
mask and opacity then interpolate that result against the original composite.
This avoids adding the same lower alpha twice. Adjustment with no active effects
remains a no-op. Nested compositions isolate their internal blend backdrop;
pre-composing a blended layer therefore requires including all lower layers.

Blended boundaries use the same 8-bit raster buffers and limits as adjustments.
`examples/blend-modes-study.lfe.json` compares the five modes with animated source
opacity on translucent backdrops. This does not claim Adobe project interchange
or full color-managed AE equivalence.


### Track mattes

Select a consumer layer and use **Properties → Track Matte** to choose its source,
then **Matte mode** for Alpha, Alpha inverted, Luma, or Luma inverted. The same
controls appear beside Mode in the timeline when its layer columns are widened
to at least 750 logical pixels. Menus support arrows, Enter/Space and Escape.
A source can be reused by multiple consumers anywhere in the layer stack.
Selecting an unlocked source hides its independent composite in the same undoable
edit; a locked source retains its visibility. Switching modes keeps visibility.
Use the source's eye switch to render it independently as well. A selected hidden
matte source still has transform handles and can be moved in the Composition viewer.

Matte inputs include their own source content, rectangular mask, ordered effects,
opacity, parent/world transform and recursively assigned matte. Their independent
blend mode, eye, Solo and Guide switches do not suppress the input. In/out points
still apply. Source pixels outside their time or spatial bounds are transparent;
inverted modes therefore retain the consumer there. A Null or Adjustment cannot
serve as a matte source; pre-compose a desired adjustment result into a pixel source.
Adjustment consumers are supported: matte coverage limits the adjusted region,
without compositing the underlying alpha a second time.

The renderer multiplies premultiplied consumer RGBA by matte coverage after its
own effects and before blending. Alpha uses source alpha. Luma is the weighted
sum of premultiplied sRGB channels (0.2126 R + 0.7152 G + 0.0722 B), including source
alpha. Inversion uses one minus that coverage. This is an explicit 8-bit sRGB
policy, not linear-light or HDR luminance. Preview and every output format use the
same path; MP4 flattens the resulting alpha over the composition background.
Raster boundaries use the requested output resolution and existing 32 MP limits.

Projects with mattes use version 18. Cycles, absent references and paths longer
than 16 matte links are rejected atomically. A frame remains limited to 4,096
evaluated layer/matte instances. Duplicate and paste remap copied references;
pasting into another composition requires the matte source to be copied as well.
Pre-compose requires all sources and consumers together, and splitting a source
requires all its consumers so both halves retain the correct references. Deleting
through the editor clears affected references in one Undo transaction; locked
consumers prevent that edit. Direct core deletion must remove or detach all
consumers in the same transaction. Reordering never changes the selected source.

`examples/track-matte-study.lfe.json` demonstrates animated traveling mattes in
all four modes over Wanted Sans text. Regression tests cover reference alpha and
luma pixels, inversion, masks, effects, animation, parent transforms, nested/reused
mattes, history, copy/split/pre-compose, invalid references and hidden offline
footage. Explicit FFmpeg tests compare MP4 and alpha MOV frames to PNG results.
