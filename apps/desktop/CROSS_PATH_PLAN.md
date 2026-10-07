# E02 contract: cross-Contents point selection and canvas affine editing

Design checkpoint: 2026-10-04, starting clean at `3257d79` on
`codex/ae-workspace`. This is a bounded editing-first continuation. Planned
acceptance is not executed native evidence; final results belong in
[CROSS_PATH_ACCEPTANCE.md](CROSS_PATH_ACCEPTANCE.md).

## Selection and current-frame scope

- Select anchors across enabled, editable Bezier Path items within one selected
  Shape Contents layer in the active composition, including nested Groups.
- Shift-click toggles individual anchors across that domain. Shift marquee and
  Ctrl+A operate on eligible Contents paths. Legacy Shape and mask selection
  remain separate; no cross-layer, cross-composition or mixed mask/Contents set.
- Direct dragging of a cross-path selected Contents anchor moves the selected
  set in composition coordinates. Ordinary single-path editing with the box off
  retains its existing local-axis behavior. An explicit **Canvas Transform · Shift+T** control
  exposes a composition-axis selection box with four scale corners and a rotation
  handle. The captured selection center is the fixed pivot.
- Current-frame editing starts from each path's sampled pose. Static paths stay
  static; animated paths insert/update only the bound frame. Other keys, key
  metadata, base geometry and stored/unused poses are retained. A dormant static
  pose reference remains static and retains its stored data. Changing/inserting
  a key may change interpolated frames between neighboring keys.
- No topology changes, implicit parametric conversion, automatic layer/group
  transforms, all-stored-pose canvas scope or source path replacement. Existing
  same-path numeric base/all-stored-pose editing remains a separate explicit route.
- Cross-path numeric, order and topology actions are safely unavailable/consumed.
  Existing singleton/same-path behavior remains available where supported.

## Coordinates and numeric contract

The affine acts in composition coordinates: signed X/Y scale about a frozen
pivot, then clockwise rotation in downward-Y coordinates, then translation.
Selected anchor positions and their tangent offsets are mapped through each
path's actual layer and nested Contents matrices, transformed, and mapped back
into that path's local coordinates. Unselected vertices are exact.

Reflected/skewed/rotated parents within the existing inverse limits are supported.
Singular/near-singular determinants below 1e-10, nonfinite mappings or inverse
coefficients beyond ±1e12 reject instead of approximating an inverse. Zero scale
and reflections are allowed when resulting local coordinates remain valid. Exact
identity and return-to-start gestures preserve source/history; fixed components
should retain precision. The operation does not change local transform tracks.

Shift constrains move, uses the scale factor with the larger change on both axes,
and snaps rotation to 15°. Flat/single-anchor sets receive visual padding for
usable handles, without moving the actual center pivot or changing source.
No snapping to unrelated geometry or custom pivot is included.

## Source, history and input ownership

One dedicated core command validates source and final project/metadata budgets
and applies all selected paths atomically. Missing/disabled/locked/non-Path,
invalid frame/indices/geometry or noninvertible transforms reject the entire
command. No unrelated schema migration, asset synchronization or new file format
is introduced. LEP/VIEW/address versions stay unchanged.

Pointer drafts are isolated from document, autosave, output and history. Release
recomputes the final pointer/modifiers and commits at most one undo transaction.
Invalid drafts cannot apply a last-valid substitute. No-op preserves Redo.

Frozen bindings include source, revision, active composition, layer and layer-set
selection, Contents selection, frame, transport/editor-action generations and
view mapping. Selection or context changes, equal-return context changes,
playback, modal/marked input, tool changes, focus loss, Escape, interrupted
pointers and changed pan/zoom/bounds retire the gesture. Non-left, held or
unrelated-modifier input must not commit or leak to another editor action.

## Independent qualification

1. Literal geometric/source expectations, including selected/unselected tangents,
   nested reflected/skewed transforms, static/dormant/animated samples and keys.
2. Atomic invalid/source-budget/final-budget and identity/Redo boundaries.
3. Pen cross-path selection, exact modifier ownership, handle mapping, final-up
   coordinates, repeated/interrupted input and stale/transport/view guards.
4. Independent complete-source and real EditorState/history comparisons,
   multiframe preview/output rendering and official JSON/LEP/VIEW round trips.
   Expected documents must not be built with the command/helper under test.
5. Serialized format/all-target/workspace/media/vendor gates and ordinary release,
   with source manifest, automatic About identity and pinned CLI comparisons.
6. Bounded native select/move/scale/rotate, Undo/Redo/cancel and actual Save/Open
   qualification using the final attributed release. Save/source verification is
   independent of screenshots. Stop after the bounded matrix; do not turn this
   milestone into the entire older native backlog.

The prior saved editor is retained until a replacement release is ready, and the
requested GitHub tab stays open. Work is on the dot cloud computer. No push,
PR, merge, deploy or user-desktop action is included. Native real IME, other
platforms/DPI/device behavior and prior unrun cases remain explicit gaps. E02
remains Partial after this slice.
