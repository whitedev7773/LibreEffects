# Native joined XYZ Position

This bounded native feature continues JSX/ScriptUI compatibility. AEP opening and
import are outside the user's current acceptance scope. Source schema **72** adds
an optional joined Position and an explicit fixed-axis camera. Existing versions
1–65 and reconstructed rich-text version 71 remain supported; unavailable legacy
contracts 66–70 remain rejected. No AE numerical/visual parity is claimed without
the separate AE oracle. Source d7aa0f9 now has the bounded release/CLI/native
qualification below; prior 8729c0f results retain their earlier binary attribution.

## Authored source and host

A 3D layer has exactly one `spatial_position` object and **no scalar PositionX/Y
tracks**. The value is XYZ; joined keys have independent incoming/outgoing
interpolation and temporal ease, relative incoming/outgoing XYZ tangents, and
explicit continuity/automatic flags. Times are native composition frames. Speeds
are nonnegative spatial distance units per second, influence is a percentage.
Tiny finite speeds, inactive endpoint handles and authored defaults persist.

`threeDLayer`, `ThreeD_SPATIAL`, joined `value`/`setValue`, `setValueAtTime`,
1-based nearest/time/value/remove key access and the original Position metadata
setter families operate on this source. Same-time value replacement retains the
key's existing metadata. `dimensionsSeparated=false` is an exact no-op; separated
tracks are rejected. Conversion from 2D requires static X/Y, no expressions and a
detached hierarchy. Conversion back requires an unkeyed zero-Z value. Removing
the last key or setting a static value on an animated source requires an explicit
future collapse contract and currently rejects. 3D Scale scripting is not
advertised as supported.

Manual spatial continuity retains zero handles and antiparallel active interior
handles. Automatic tangent/ease generation and temporal-continuous mode reject.
Metadata restoration may temporarily produce an unsampleable native curve;
native files preserve such structurally valid authored data and preview/export
show an explicit diagnostic. Automation validates the full resulting curve at
its final transaction boundary and rejects the entire draft on error, even when
JSX catches an earlier unsupported host call. No partial edit or Undo entry is
accepted. Source load/save do not flatten or repair unsupported authored curves.

## Sampling and timing

The mathematical crate samples the cubic spatial path by arc distance, then
applies independent temporal sides in physical units. An equal-endpoint segment
can make a real excursion; it is not collapsed by chord length. Certified chord
and control-polygon bounds, global largest-uncertainty refinement and bounded
inversion retain explicit precision errors. Default relative error is 1e-7 of the
initial control-polygon length, with 65,536 splits, depth 40 and 64 inversion steps.
No exhausted path becomes a straight line or a zero-speed approximation.

Whole-layer startTime/shift moves all XYZ key times together, preserving values,
relative handles, modes, flags and speeds. Trimming preserves keys. Duplicate and
same-FPS layer paste retain the joined track. Moves beyond representable native
ranges, composition FPS changes with spatial data, and cross-FPS paste reject.
Legacy scalar nudge/anchor/transform, graph and mixed-key clipboard edits cannot
silently edit one axis. The prior clipboard survives a rejected Copy operation.

## Explicit projection and native controls

`Camera3` stores XYZ position, focal distance, principal point and near clip. It
looks along positive Z. Native layer planes remain front-parallel; Z affects
perspective scale and stable far-to-near paint order. Equal-depth ties retain
ordinary layer-stack order. Same-dimensional native parenting preserves local
XY transforms and sums Z. No guessed AE default camera is introduced.

Preview, export, projected bounds and picking share the same geometry and depth
order. Unsupported mixed visible 2D/3D planes, missing camera, near-plane crossings,
3D expressions, tilted planes, non-identity parenting compensation, rendering
effects, masks, mattes and non-Normal blends fail explicitly. The initial content
subset is Rectangle, Solid, Text and Null. Non-painting Audio/Null sources do not
participate in pixel admission; projectable Null controls are separate overlays.
An unprojectable Null control is omitted without hiding a paint-layer error.

Inspector/Timeline show true read-only XYZ and joined-key count. Projected layer
selection, modifiers, locked-layer skipping and Hand/Zoom remain available.
Unsupported preview geometry/text gestures are guarded. This milestone exposes
native typed setup and fixtures; a general interactive 3D camera/rotation editor
is a separate feature.

## Verification entry points

Use the pinned workspace toolchain with one compiler and incremental off:

- `cargo test -p libre-effects-spatial --locked --offline -- --test-threads=1`
- `bash apps/desktop/scripts/verify.sh models --offline -- --test-threads=1`
- `cargo run -p libre-effects-editor-model --example automation_process_harness --locked --offline -- --spatial-roundtrip`
- `bash apps/desktop/scripts/verify.sh check --offline`

The standalone production-renderer probe entry is
`tests/spatial_render_probe/probe.rs`; it uses independently authored native fixtures and
literal SVG references, plus depth/tie/locked picking and explicit failures. Its
helper execution is separate from final-binary CLI/native qualification. No
original JSX/AEP data is copied into fixtures or executed by these checks.

Remaining original-workflow dependencies include faithful scalar Opacity
incoming/outgoing endpoint ease and signed tiny speeds, needed expression API
contracts, native template setup, and AE TextDocument replacement semantics.
Passing this bounded native feature does not mean the full supplied JSX works.

Fresh source gates: 20 pure math tests; 234 model cases (233-pass full run plus
one corrected test expectation in focused replay); real child-process source
roundtrip; 671 core cases plus one corrected future-version assertion in focused
replay; 8 literal render pairs (1,036,800 pixels), 4 picker tests and canonical
workspace all-target check in 42.402s. Formatting and whitespace checks pass.
The core/host and desktop changes received independent read-only review. These
source gates are distinct from the final-binary qualification below.

## Final release and native qualification

Source `d7aa0f9cd6059a8bfede4ce70df90c55ed991911`, build
`20261006.062623-1dee42429ce66b8a`, 722 inputs, passes one canonical measured
foreground release (596.105s). Native About matches. Eight final CLI literal
reference comparisons cover 1,036,800 RGBA pixels exactly; two Null-only outputs
are transparent; missing-camera, near-plane and mixed-plane failures preserve
existing PNG outputs. No font override or additional renderer build was needed.

Native projected depth/tie picking, post-seek geometry, Shift/Ctrl, locked skip,
Hand/Zoom, genuine read-only XYZ and scalar geometry/Graph guards pass. The public
ScriptUI sample's complete source matches an independent two-key metadata oracle,
including tiny dormant endpoint speeds and tangents. A caught unsupported automatic
mode rolls back the preceding rename and adds no Undo entry. Cancel, one-step
Undo/Redo, saved frame navigation and native reopen preserve full expected source;
VIEW changes are checked separately, and unchanged-view paths are byte-identical.
The fixture contains no embedded images, so image-alias coverage is not implied.

All three native preview diagnostics show explicitly; the valid saved scene
recovers. Short ScriptUI held Return/Escape and typing/cancel continuity passes.
The app closes normally with exit 0. Exact evidence and remaining visual/runtime
limitations are in `../libreeffects-qa/native-spatial72-release-20261006/` and the
latest [STATUS](STATUS.md). Generic script-dialog labels still mention 2D; that
nonblocking wording is a follow-up, not an expanded compatibility claim. No new
AEP/import, original JSX, Adobe parity or other-platform qualification is included.
