# Repeat Edge keyboard ownership correction

The first schema-78 native batch found one document-editing defect. A rejected
Repeat click during playback still focused its button. Space then stopped playback
on key-down, but GPUI generated a focused-button click on key-up. The now-paused
control accepted that click and changed the effect to Transparent. The saved
source showed exactly that policy edit; Undo restored the complete baseline.

The correction gives Repeat the receipt from the workspace's existing activation
latch for the current key-down and window. The latch sees presses begun in other
controls and releases after focus moves. A first unmodified Space during playback
passes to transport. An eligible paused Enter/Space activates Repeat synchronously
once. Repeats, modifiers, inactive windows and stale or unavailable targets cannot
edit. Unsupported key-release negotiation permits pointer activation only; a
first playback Space can still stop transport.

Every GPUI keyboard click is ignored by Repeat. Key-up only releases ownership
and clears the dispatch receipt. No document action is retained for a later key-up
or re-evaluated after playback changes. Mouse input retains its original press
receipt and pending-field commit/context/effect validation sequence. Other buttons,
transport routing, schemas, fixtures and rendering algorithms are unchanged.

## Checked source

- All 17 focused activation/session tests pass, including five new cases covering
  playback-to-paused ownership, one activation per press, rejected/elsewhere
  presses, unsupported negotiation and independent Enter/Space ownership.
- The existing three Gaussian model cases pass for schema/preset/no-op/history,
  duplicate/reset and invalid-owner/locked rollback.
- The canonical all-target check passes in 1m17s; formatting and diff checks pass.
  An independent read-only review found no concrete defect in the four source files.
- No full model-suite replay or extra renderer run was used for this input fix.
  Raw logs are under `../repeat-edge-key-fix-private-20261006/checks`.

Native correction qualification passed on clean `7de5cfabe868`, build
`20261006.222502-9efe150efb577365`. One canonical foreground release took
635.212s; all 800 source inputs and native About matched. The exact rejected-click
then Space-stop regression preserves the complete baseline. Fresh Enter and held
1.2-second Enter/Space make one policy edit, with exact Undo/Redo and no further
Undo entry. Pending Radius/pointer ordering and locked rejection pass. Forced
pointer-only mode rejects Enter/held Space, permits the pointer toggle, and saves,
reopens and resaves the edit byte-identically. Both isolated sessions close normally.

There are 23 independently checked source snapshots; the two initial saves retain
known typed/default encoding normalization separately from subsequent exact-byte
checks. Cross-focus held dispatch remains model-only. Prior rendering and
unaffected native checks remain attributed to the initial schema-78 binary; no
corrective CLI rerun occurred. The initial Space failure and denied optional Reset
remain preserved, and Reset was not retried. See STATUS for fingerprint, resource
measurements and evidence location.

## Reproducible corrective native scope

Use a copied public constant-boundaries scene and a fresh native-save baseline.
Keep the original batch's source, packages and evidence immutable.

1. Enable looped playback, click Repeat while playing and observe the rejected
   edit. Press Space once: playback must stop, Repeat must stay On, and the project
   must remain clean. Return to the baseline frame and compare the whole source.
2. While paused and Repeat is focused, verify fresh Enter and Space each toggle
   exactly once. Held keys and their releases must not add another edit or start
   playback. Check one Undo/Redo and complete saved-source equality.
3. Exercise a focus-transfer/held-key sequence only if the supported native input
   tools can perform it directly. Otherwise retain the precise model-only coverage
   for that sequence; do not claim native dispatch was exercised.
4. Confirm a pending Radius edit followed by a pointer toggle retains the intended
   commit order and that locked/playback rejection remains unchanged.
5. In the existing forced pointer-only mode, focused Enter/Space cannot edit Repeat;
   pointer activation still works. Save/reopen and verify exact authored source.

No automatic-review-denied Reset action should be retried through another route.
AE effect calibration, Windows native behavior and full reference-project parity
remain outside this bounded correction.
