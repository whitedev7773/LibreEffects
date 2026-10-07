# Compound Gradient Colors multi-key editing contract

Bounded E04 continuation, 2026-10-04. Starts from `82a1c22` in the existing
`codex/ae-workspace` dot-cloud clone. Local commits only; no push, PR, deployment
or user-desktop work. Preserve the prior saved native editor file and GitHub tab.
The [interpolation contract](GRADIENT_INTERPOLATION_PLAN.md) still defines complete
snapshots, ordered paint-local stop identities, interpolation and topology Hold.

## Selection and timing

The dedicated Timeline Colors lane owns a set of exact keys on **one paint**.
Ordinary click selects one key. Shift-click toggles membership on that paint;
selecting another paint replaces the selection. An empty selection releases
compound ownership. Selection is transient and is not persisted in source/VIEW.
No scalar PropertyPath or Graph curve is invented.

Precise group Move sets the earliest selected key to the entered whole frame.
All other selected keys retain their relative frame offsets, complete snapshots,
ordered stop IDs and outgoing interpolation. Selected destinations may overlap
their own old frames, but collisions with unselected keys reject the entire
operation. Empty/missing keys, overflow and out-of-composition destinations
reject. This is rigid translation, not duration scaling or pointer dragging.

Delete removes the exact selected set in one transaction. If no keys remain,
the sample at the explicit current playhead is baked before deleting the keys.
The mode controls set all selected keys' outgoing mode in one transaction.
Dormant final-key modes and the existing topology-mismatch Hold policy remain.

## Internal same-paint key clipboard

Explicit Copy selected and Paste at playhead controls use an internal clipboard,
separate from the system text, scalar-key, Contents and layer clipboards. The
earliest copied key has offset zero. Paste places it at the playhead and keeps
all copied relative offsets, exact ordered snapshots and outgoing modes.
Paste requires a nonempty key selection on that same paint; source-neutral
Seek/Step retains the compound selection. Clearing selection clears the copy.

The clipboard is valid only for its original paint and uninterrupted source/
ownership context. It survives source-neutral playhead navigation. Intervening
source/history/domain actions invalidate it, including changes later reversed
back to identical source. Copy again after editing. A successful paste may
refresh this receipt to permit another paste of the same payload. New/Open,
composition/paint changes, lock/modal transitions and stale callbacks may never
revive it. This deliberately conservative boundary avoids guessing whether a
reused paint-local stop ID still represents the copied stop.

Paste requires existing Colors animation and does not overwrite an occupied
destination. A fully identical payload already present at all destinations is
a no-op, including exact snapshot bits and outgoing modes. Partial overlap,
different snapshots/modes, duplicate offsets, a missing zero anchor, excessive
payloads and invalid frames reject atomically. Imported IDs are retained without
remapping and the paint's next-stop allocator remains above all admitted IDs.
No cross-paint/project, OS clipboard format, implicit conversion or Cut exists
in this slice.

## Source, history and UI safety

The new core operations use the dedicated compound source-preserving route:
validate original and candidate projects, metadata/key/storage budgets, lock,
composition bounds and interpolation schema. Preserve unrelated source/assets,
existing schema unless the payload requires promotion, and exact no-op Redo.
New operations cannot escape through a mixed generic-migration batch. Schema
54/57, LEP1, VIEW and property-address formats do not change.

Each callback is bound to rendered source, ownership, selection serial, input
action and transport. Pending fields are rejected before blur, not committed by
a stale button. Real composing state, hidden/blocked controls, newer selection,
lock/modal/playback changes and removed/recreated keys cancel safely. Enter and
Escape return the Move field's focus to Timeline. Generic selection commands
must continue to consume compound ownership rather than deleting/copying an
entire layer. Shift admission is scoped only to compound key selection.
Retired or deleted keys leave a temporary compound-domain owner until an
explicit domain exit, so repeated Delete cannot fall through to layer deletion.
Selection restoration after an edit is a one-use successful return from that
specific operation; arbitrary source/history ABA cannot recreate a selection.
Shell-only modal entry retires compound field/selection/clipboard receipts
before focus changes can blur pending Move input.

## Required evidence and exclusions

Use independent literal snapshots and static-gradient render references for
source, pixels, JSON/LEP/VIEW, history and no-op checks. Qualify selected-overlap
translation, collision/bounds/budget rejection, interpolation and topology
preservation, multi-delete, exact duplicate paste, sparse schema promotion and
clipboard source/ABA invalidation. Run the full pinned default/media/vendor/
format/check/release gates and attribute a bounded native session to its exact
source and binary. Generated inputs are immutable and separate from GUI saves.

E04 and the 77-ID inventory remain Partial. Pointer dragging/marquee, time
scaling, compound Graph curves, Bezier/velocity easing, topology reconciliation,
cross-paint bulk and cross-project Contents clipboard remain separate. Native
IME/adversarial input timing, other platforms/DPI/devices and earlier E02/E04
gaps must stay distinct from automated evidence. Finish this milestone report
and verified complete-history bundle before starting another feature.
