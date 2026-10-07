# Compound Gradient Colors interpolation and Timeline contract

Bounded E04 continuation, 2026-10-04. Starts from `643cda8` in the existing
`codex/ae-workspace` dot-cloud clone. Local commits only; no push, PR, deployment
or user-desktop work. Preserve the previous saved native editor and GitHub tab.

## Source and sampling

The existing complete `GradientColors` snapshots and paint-local stop IDs remain
authoritative. Existing `keys` storage stays unchanged. A sparse
`outgoing_interpolation` map records only non-Hold modes on existing keys. An
absent entry means **Hold**, exactly preserving legacy behavior and serialization.
New keys default to Hold; editing an existing snapshot retains its mode.

Supported outgoing modes are Hold, Linear and Smoothstep. Smoothstep uses the
same bounded `t*t*(3-2*t)` progress as the scalar interpolation mode; it is not
Bezier/velocity easing or After Effects compatibility. All stop numbers (color
position/midpoint/RGB and independent opacity position/midpoint/value) share that
progress. Exact keys, before-first and after-last frames retain exact snapshots.

Compatibility requires exactly the same ordered color-stop IDs and exactly the
same ordered opacity-stop IDs in adjacent keys. Matching is paint-local, never
by row index, location or color. Position crossings remain compatible: spatial
sorting and coincident-stop tie behavior stay in the existing renderer. Any
added, removed or reordered stop makes the span incompatible. A requested Linear
or Smoothstep mode is retained in source, but that span explicitly holds its left
snapshot until the next key. UI status must say why. No resampling, invented
stop identity, topology morphing or cross-paint correspondence is allowed.

Metadata on the final key is retained but has no outgoing segment. Moving keys
moves their modes, deleting keys removes their modes, and frame-rate mapping
moves both maps with the existing collision rules. Disabling animation or
removing its last key bakes the current sampled snapshot. Dedicated compound-only
edits and batches preserve unrelated source/assets and
exact no-op Redo, and validate original/candidate metadata budgets. New
SetInterpolation/DeleteKey operations reject mixed batches instead of falling
through to generic schema/asset migration. Existing broader layer operations
and historical mixed batches retain their older contracts. Invalid modes, explicit
Hold entries, orphan mode entries,
unsupported versions and existing malformed source fail atomically.

Nonempty outgoing metadata requires schema **57**. Hold-only compound animation
continues to require 54; absent metadata and legacy source remain unchanged.
LEP container, VIEW metadata, and numeric property-address versions do not change.
Contents clipboard feature promotion must account for mode metadata, including
copy then Undo then paste. Legacy scalar-stop animation still cannot coexist.

## Desktop boundary

Properties retains default Hold enabling and adds explicit mode controls at an
exact current key plus an effective-segment explanation. The modal gradient
editor continues to edit one complete current-frame sample.

Timeline gains dedicated whole-Colors lanes for gradient paints, with discrete
key buttons, selection/navigation, exact Move frame input, explicit deletion and
Hold/Linear/Smoothstep actions. The optional modal shortcut requires the same
paint already selected in Contents; Timeline does not take over tree selection.
These are not scalar Graph tracks or numeric PropertyPath addresses. This slice does not implement pointer key dragging,
marquee/multi-key compound edits, clipboard key duplication, Bezier handles,
Graph curves, cross-gradient bulk Colors, or topology reconciliation.

Every action is tied to the rendered source, selection, input generation,
transport and exact selected key. Pending text/IME fields must not cause stale
selection or an Add/Remove/Enable/Disable intent inversion. Timeline actions
require pending fields to be accepted first. A stale callback, newer source,
lock/modal/playback transition or removed key cancels safely. A precise Move
commit is one history transaction; duplicate destinations reject without loss.
A shared transient ownership latch consumes generic layer/scalar selection
commands while a Colors key is selected, avoiding whole-layer Delete/Cut/Copy/
Duplicate fallthrough. Escape or explicit domain navigation releases ownership.
The latch authorizes no mutation and is never stored in source or VIEW.

## Evidence required

Independent tests construct literal snapshots and legacy-static rendering
oracles, including compatible Linear/Smoothstep, both paint types/modes,
independent color/opacity rows, crossing/tie ordering, topology boundaries,
endpoints, source/history/no-op, key timing, official LEP/VIEW round trips and
old Hold/legacy equivalence. Production sampling is not its own expected oracle.

Run the pinned environment's format, all-target workspace check, full workspace
suite, explicit media suite, vendor grid/usvg variants and normal optimized
release. Attribute all native evidence to an immutable release/source manifest.
Native qualification should cover visible lane/mode state, exact frame movement,
Undo/Redo, pending-invalid rejection, incompatible topology explanation, saved
source and actual reopen. Generated fixtures remain separate from actual saves;
full expected source and VIEW comparisons use no undisclosed masks. Report any
unrun native/IME/platform/DPI/audio scope explicitly. E04 remains Partial.
