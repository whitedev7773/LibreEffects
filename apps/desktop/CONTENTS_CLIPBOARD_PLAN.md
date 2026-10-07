# E04 contract: session-local Contents sibling clipboard

Design checkpoint: 2026-10-04, starting at clean `9af50ba` on
`codex/ae-workspace`. This is a bounded continuation of the editing-first roadmap.
Implementation and executed acceptance are recorded separately; a planned case
is not native evidence. Read [HANDOFF.md](HANDOFF.md) for the current checkpoint.

## Scope and destination

- Copy, Cut and Paste one or more immediate siblings, including complete nested
  Groups. Snapshot roots follow source tree order, never click order or numeric ID.
- Paste into an existing Contents layer in the same composition at the same FPS.
  No implicit conversion of a legacy Shape, cross-composition/FPS conversion,
  OS/project clipboard interchange or paste-at-playhead is included.
- A selected singleton Group appends within that Group. A selected leaf or sibling
  block inserts after the last selected sibling. With no selection, append at root.
  The UI states the destination and retains the local-value/paint-scope warning.
- Preserve names, enabled flags, geometry, transforms, paints, Trim order, complete
  scalar keys/handles, dormant tracks, every path pose (including unused poses),
  and complete Hold Colors snapshots. Preserve original key frames.
- Allocate fresh layer-local Contents node IDs recursively on every paste. Numeric
  node IDs may coincide across different layers; their complete addresses do not.
  Gradient-local stop
  IDs/allocator and path-pose indices stay intact because they are separate identity
  domains. Local transforms and destination paint scope may change appearance;
  there is no coordinate compensation or promise of pixel-preserving relocation.

## Source and history contract

Copy captures immutable payloads after full source validation and makes no history
entry. Later source edits or deletion do not change the snapshot. Cut deletes the
exact sibling set in one validated transaction, replacing the clipboard only after
success. Paste inserts in one transaction and selects/reveals the new root siblings.
Cut retains the parent even when emptied. Invalid or stale operations preserve
source, allocator, selection, clipboard, Undo and Redo.

Core accepts explicit exact source parent/IDs or destination parent/index. Empty,
duplicate, mixed-parent, missing, non-Contents, locked, out-of-range, exhausted-ID,
invalid-original, depth/node/key/metadata, changed-FPS and key-duration-overflow
cases reject atomically. A duration edit alone does not invalidate a snapshot
whose stored data still fits the current duration. Original and final projects and metadata budgets are validated. No
unrelated source migration or asset synchronization belongs to the clipboard path.
Cut preserves declared schema; Paste may raise only the existing minimum needed
by its payload, notably Colors schema54 after Copy then Undo of its introduction.
No new project, LEP, VIEW or numeric-address version is introduced.

## Desktop ownership and pending input

Contents, layer and key clipboards are mutually exclusive and cleared on New,
Open and recovery. Contents Paste cannot fall through to layer/key Paste. Tree-only
Ctrl+C/X/V and explicit guarded buttons are supported. Text fields retain OS text
clipboard ownership and marked input. Shell menu ownership is explicit or safely
unavailable while Contents owns focus; menu clicks must never copy/cut an old
Timeline selection. Held/extra-modifier chords are consumed without mutations.

Bindings include immutable source, composition/layer/selection, revision, frame,
transport and editor-action generation, and a panel binding serial. A successful
pending field flush may grant one action receipt only after semantic destination
is replanned. Invalid, stale, duplicate, marked or failed-core drafts cannot do so.
An accepted pending field edit retains its own existing Undo transaction; the
clipboard action is a separate transaction. A rejected clipboard action does not
roll back that already accepted field edit. Guarded non-left presses reject before
field blur. Playback, conflicting modal
sessions, external selection changes and equal-return context changes retire old
callbacks. Successful actions normalize Graph/Pen/gradient targets through normal
EditorState policy without transferring source Graph pins to cloned node IDs.

## Independent automated acceptance

1. Literal complete expected trees for singleton/noncontiguous selections,
   scrambled IDs, disabled/nested Groups, root/group/cross-layer placement and
   repeated fresh recursive IDs.
2. Exact preservation of dormant tracks, easing/temporal handles, unused path
   poses, legacy gradients, variable-topology Hold Colors and local stop identity.
3. Atomic source/clipboard/history behavior for failures, Copy after source change,
   Cut/Undo/Redo/Paste, no-op Redo, schema preservation/promotion and budgets.
4. UI/session ownership, destination/selection/focus, pending receipts, lock,
   playback, stale context, modal/IME policy, held/modified keys and menu domains.
5. Independent multiframe preview/output rendering, official LEP full source/VIEW
   round trips and actual EditorState normalization. Expected documents must not
   be produced by the clipboard operation under test.
6. Serialized format, all-target check, workspace/media/vendor gates, then a normal
   optimized release with frozen watched-source manifest and CLI comparisons.

## Bounded native qualification

Use the final attributed release and preserve the prior saved editor until it is
ready. The dot cloud desktop is the selected machine; the requested GitHub browser
tab remains intact. The original 47 cases and prior Colors gaps stay separate.

- Copy/Paste singleton and noncontiguous siblings in visual order.
- Nested Group/root and same-composition cross-layer paste; repeated independent IDs.
- Cut, one Undo/Redo and later Paste, with source snapshot retained after deletion.
- Animated scalar/gradient/Colors samples at representative frames.
- Pending valid/invalid field, text clipboard, Escape and menu ownership.
- Locked/playback/modal/held-key refusal without layer/key leakage.
- Actual native Save/Open/resave and strict complete source/VIEW comparisons
  against independent expectations, plus pinned CLI render checks.

Record each executed, partial or unrun case. Generated fixtures are never native
saves; clipboard-pasted text is not direct typing or real marked Korean IME.
Windows/macOS/DPI/device cases remain unrun unless independently exercised.

## Exclusions and next scope

Cross-parent source selections, drag-copy, expanded multi-item Duplicate/Delete,
property-only clipboard, cross-paint bulk Colors, compound interpolation/Timeline
lanes and E02 cross-path/affine editing are separate work. E04 remains Partial.
No push, PR, merge, deployment or user-desktop action is part of this milestone.
