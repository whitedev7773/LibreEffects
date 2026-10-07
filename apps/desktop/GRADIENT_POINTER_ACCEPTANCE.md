# Compound Gradient Colors pointer acceptance — 2026-10-05

Bounded E04 continuation from `d629a52`, resumed after the explicitly unqualified
`c9402a0` checkpoint. Read the [pointer contract](GRADIENT_POINTER_PLAN.md),
[multi-key contract](GRADIENT_MULTIKEY_PLAN.md) and
[repeatable qualification instructions](qa/gradient_pointer_acceptance.md).
Same dot-cloud checkout and `codex/ae-workspace`; local commits only.

## Implemented boundary

The dedicated Colors lane admits keys on one paint. Plain press on an already
selected key preserves the group until release; a click collapses to that key,
while a press on an unselected key selects it. Shift-click toggles membership
and is selection-only. Four horizontal pixels start dragging; vertical-only
movement cannot retime keys. The initial pointer offset inside the glyph is
preserved. Complete snapshots, ordered stop IDs, relative timing and outgoing
modes move together through the existing source-preserving MoveKeys command.

Only local ghost glyphs change while dragging. Source, assets, VIEW, playhead and
history remain unchanged until one release command. Zero-frame and return-to-
start gestures preserve Redo. The common delta clamps to composition bounds;
selected-old-frame overlap is allowed, unselected-key collision rejects the
whole drop. Eight-pixel snapping considers only the frozen playhead and
unselected same-paint keys; equal distances prefer ascending selected anchor,
then target. Alt bypasses snapping. No auto-scroll, time scaling or marquee.

Window-level capture consumes release once, including over another panel.
Frozen lane geometry/source/selection/action/input/transport receipts reject
stale callbacks and ABA. Zoom/scroll/layout, native bounds, focus/activation,
modifier, modal, lock and pending/composing input changes cancel safely. Pending
input is refused before blur. All admitted presses, including Shift with no
selection, own the compound shortcut domain. Retired/deleted selections retain
the existing tombstone so repeated Delete/Cut/Copy/Duplicate cannot reach a layer.
Keyboard activation remains available without replaying mouse clicks.

No new core operation, project schema, LEP/VIEW or property-address version is
introduced. Scalar Graph channels and other paints retain their existing paths.

## Source, review and initial failure

- `779369a` / `2b399ea`: bounded contract and snapping/click semantics
- `948b2d0`: usage guidance
- `cea4428`: independent controller/source/render/codec/history acceptance
- `4f630eb`: guarded UI, pointer controller and focused regressions
- `78144c9`: repaint canceled ghosts
- `c9402a0`: honest uncompiled checkpoint while disk recovery awaited approval
- `eb0bbe1`: test-only aliases through the private Timeline module
- **`4010607`**: recovered-environment qualification guidance; final qualified source

Read-only review identified native resize-before-repaint invalidation, missing
shortcut ownership for an initial Shift press, and keyboard activation lost when
removing mouse-click callbacks. These were fixed before compilation. The first
real all-target check then found two E0603 private-module test imports; narrow
`cfg(test)` aliases fixed them without exposing the production Timeline module.
The initial failure is retained in `first-check.log`; it is not a passing gate.
No application source change was needed after actual native testing began.

## Environment recovery and historical evidence

On Oct 4, large builds paused with about 918 MiB free. On Oct 5, after the user
approved narrow stale incremental-cache cleanup, the entire previous
`/workspace/shared` directory was already absent before this task could remove
anything. Its cache, toolchain, old QA archives and saved files were unavailable;
the repository and full Git history survived. **This task deleted nothing and
cannot attribute the freed space to its own cleanup.** Do not infer that missing
historical local evidence is currently available from its older documentation.

The still-running preceding editor provided two recoverable artifacts:

- Native Save As recovered the prior SVG `08-reopened.lep`, 8,489 bytes, exact
  historical SHA-256 `c3a82ab4b3e164d97012308a9578ec281606593cb2059ca6988394c16fdfb8ab`.
- Its running executable was copied through the cloud desktop before normal
  close: 67,276,256 bytes, exact historical SHA-256
  `bb86a3b335ccedbbb257e5e362a81e76ce6f6bfc945d1cd2daa40b33bc2239f0`.
  This is the **fb8b03a SVG** release, build
  `20261004.224701-4242695670ebf006`, used as the compatible previous comparator.

Official pinned Rust/Cargo 1.97.0 and signature/hash-verified Debian native
packages were restored under the persistent task workspace. Vulkan llvmpipe,
LLVM 19, FFmpeg/FFprobe and DejaVu font checks passed. New builds use
`CARGO_INCREMENTAL=0` and two jobs. Only desktop dev/test debug metadata is
reduced/stripped; release settings, assertions and dependencies are unchanged.
Direct Cargo equivalents were used, not Moon. New evidence is independent of the
missing old directories; no historical files or results were recreated as originals.

## Final automated gates

All 12 gates in `final/final-gates.tsv` passed on **4010607**:

- Workspace formatting and locked all-target check.
- **1,818 default tests:** 597 core, 1,215 desktop and six build helpers.
  Desktop's 50 explicit-input/media/device tests remain ignored in this gate.
- **32 explicit media tests**, with device, exporter/native verifiers and the
  separately run font test excluded from generic ignored-media execution.
- Vendor grid debug/release: each 215 unit +43 doc tests, two doc tests ignored;
  no-default check. usvg's eight tests, formatting and no-text check. Explicit
  DejaVu ffi ligature test. Normal optimized desktop release.

Focused 45 compound Timeline, one shared Alt policy and six independent tests
also pass and overlap this aggregate; they are not added to its total.

The six independent tests drive the actual pointer controller and compare
literal source/VIEW, JSON/LEP, history and independent static-gradient render
oracles across all Fill/Stroke × Linear/Radial paints. The successful translation
and topology matrices make **1,300 exact full-RGBA pairs /124,800,000 pixels**.
They also cover provisional neutrality, no-op/Redo, locks, collisions, bounds,
noncontiguous sets, modes, unrelated source and ordered-topology Hold.

Immutable export and repeat verification pass for **467 files /208 render
cases**. Frozen generated CLI qualification makes **1,040 renders /832 exact
RGBA pairs /79,872,000 pixels**: 208 independent-static, 208 JSON/LEP and 416
previous/current comparisons. All generated inputs and both binaries remain
unchanged. There are no pixel masks or relaxed production tolerances.

## Exact final release

- Source: `4010607c88e74f90deb511a4af73007ad00bb43c`
- Build: **20261005.015007-b0495381d51ed0b2**
- Displayed source: `Git 4010607c88e7 (clean sources)`
- Fingerprint: `b0495381d51ed0b2`; 467 watched inputs
- Target/profile: `x86_64-unknown-linux-gnu / release`
- Binary: 67,413,896 bytes
- SHA-256: `89728dcb484db42a4f95ae7e74740d8e7166e5441afd96c6dfbfb32fff06866e`

Actual native About matched these fields. Later documentation-only commits do
not change this qualified binary's identity.

## Bounded actual native qualification

The prior clean editor was closed normally after exact recovery. No discard,
forced quit or lock-file removal occurred. Candidate first paint required one
maximize/restore expose; its cause is unproven. Bound-window synthetic clicks
were ineffective, so the accepted workflows use direct cloud-desktop input.
Keyboard menu access verified About. These startup/input observations remain
separate from the successful editing flows.

All eight actual saves below use the final build. Each passes complete source
and complete VIEW against a separately declared literal reference; only the
explicit expected playhead frame is changed in the reference.

| Save | Actual observed sequence and final state |
| --- | --- |
| 01-selection | Click 0, Shift-click 15; two selected, source unchanged, frame 15 |
| 02-overlap | Drag selected 0/15 from x707 to x761; keys become 15/30/45, playhead 15 unchanged |
| 03-undo | One global Ctrl+Z restores 0/15/45; frame 15 |
| 04-redo-collision | Vertical-only and away/back group drags leave source unchanged; one Ctrl+Shift+Z restores the move. Select 15/30 at frame 30; +15 drop onto occupied 45 rejects with a diagnostic |
| 05-guards-undo | Pending `bad` Move blocks a drag before blur; Escape restores the field/focus. Delete the pair, then two more Delete presses and Backspace leave key 45 and one layer. One Undo restores the full moved source at frame 30 |
| 06-outside-release | Fresh 0/45 selection; plain drag from (815,711) to (851,400), released over Preview, produces 10/15/55 and retains playhead 45 |
| 07-locked | Locked fixture rejects key click and +54-pixel drag; full source/VIEW unchanged at frame 0 |
| 08-reopened | Actual New reaches empty Untitled; Open actual 06, then Save As 08 preserves its full source/VIEW at frame 45 |

The no-op/Redo/collision and pending/Delete/Undo intermediate states were
visually observed; their final states are the separately saved evidence. Discrete
repeated keys do not establish held-auto-repeat. Eight native verifiers make
**520 exact RGBA pairs /49,920,000 pixels** across 13 frames and five routes.
Files 01/03, 04/05 and 06/08 are byte-identical; 06/08 are 4,955 bytes each.

Supplementary native CLI makes **312 renders /208 exact RGBA pairs /19,968,000
pixels**, including 104 previous/current actual-save comparisons. Together,
generated and native CLI make **1,352 renders /1,040 pairs /99,840,000 pixels**.
The final audit checks 479 input hashes, including eight native saves, all 467
generated files, the manifest and three binaries. No inputs changed.

Both successful drag destinations exactly matched their intended integer
frames; this does not establish arbitrary coordinate/DPI precision. An attempted
Alt+drag was intercepted by Xfce and moved the window without editing keys;
Alt+F10 restored its original bounds before the successful plain outside-release
drag. **Alt bypass is headless-qualified only.** Screenshots were inspected,
not archived. Atomic drag input cannot establish held-preview observation or
Escape/focus/deactivation/modifier interruption while the button remains held.

## Handoff and remaining scope

QA root: `/workspace/scratch/99b390616904/libreeffects-qa/gradient-pointer-20261005`.
`final/` is authoritative for source/gates/release/generated/native qualification;
`recovered/` preserves the prior project/binary. Setup provenance is under
`/workspace/scratch/99b390616904/libreeffects-dev`. Source the latter's
`dev-env.sh`; serialize Cargo and preserve normal release configuration.

The app is clean on `final/native/08-reopened.lep`, frame 45, keys 10/15/55.
GitHub remains open. Recovered prior project/binary hashes remain exact. No
push, PR, deployment or user-desktop work occurred. A verified complete-history
bundle follows the documentation checkpoint before another feature.

E04 and the 77-ID inventory remain **Partial**. Same-paint marquee, scaling,
compound Graph, Bezier/velocity, topology reconciliation, cross-paint dragging
and cross-project interchange stay separate. Native Alt/held-event/IME/stale/
zoom-scroll-clamp combinations, direct radial/Stroke interaction, other OS/DPI/
devices and earlier E02/E04/text gaps remain. A defensible next editing slice is
same-paint marquee selection under its own contract, or one bounded advanced
Text Animator selector. Keep cache/GPU/output, 3D/tracking and Adobe interchange
behind editing-first work. No next feature has started.
