# Compound Gradient Colors multi-key acceptance — 2026-10-04

Bounded E04 continuation from `82a1c22` in the existing `codex/ae-workspace`
dot-cloud clone. Read the [multi-key contract](GRADIENT_MULTIKEY_PLAN.md) and
[prior interpolation acceptance](GRADIENT_INTERPOLATION_ACCEPTANCE.md).
This milestone adds editing operations on the existing compound representation;
it does not change schema 54/57, LEP1, VIEW or numeric property-address versions.

## Implemented boundary

The dedicated Timeline Colors lane owns an exact key set on one paint. Click
selects one key; Shift-click toggles membership. Selecting another paint replaces
the set. Precise Move sets the earliest selected frame and translates every other
selected key by the same integer offset, retaining gaps, complete ordered
snapshots and outgoing modes. Destinations may overlap selected keys' old frames;
collisions with unselected keys reject the whole operation. Empty/missing keys,
overflow, lock, composition bounds and storage/metadata budgets reject atomically.

Delete removes the selected set in one Undo. Deleting every key first bakes the
sample at the explicit playhead. Hold/Linear/Smoothstep controls set the selected
keys' outgoing modes together, including dormant terminal modes. The existing
ordered-ID compatibility and explicit topology-mismatch Hold policy remain.

Copy selected and Paste at playhead use a separate internal same-paint clipboard.
The earliest copied key has offset zero; all other offsets, ordered stop IDs,
complete snapshots and modes are retained without remapping. Paste requires an
existing Colors animation and selection on the original paint. A fully identical
payload already at all destinations is a no-op, including exact snapshot bits
and modes. Partial/different overlap, malformed offsets or invalid frames reject.
The paint's stop allocator stays above every admitted ID.

Source-neutral Seek/Step retains selection and the copied payload. Successful
paste refreshes its receipt for another paste. Other source/history/domain
changes retire the clipboard, including source ABA, New/Open, lock and modal
entry. Clearing selection clears the copy. Exact rendered-source/ownership/
selection/action/transport receipts reject stale callbacks and pending or marked
input before blur. Enter/Escape returns Move focus to Timeline. Retired/deleted
keys keep compound-domain ownership until an explicit exit, preventing repeated
Delete/Cut/Copy/Duplicate from falling through to whole-layer edits. A successful
operation alone can restore selection through a one-use receipt. Shell-only
modal entry retires all compound receipts before focus can blur a draft.

Dedicated core operations validate both original and candidate source and retain
unrelated assets/source, historical schema declarations and exact no-op Redo.
They cannot escape this route through a mixed generic-migration batch. No scalar
PropertyPath or Graph curve, lossy legacy-stop conversion or OS clipboard format
is invented.

## Local commits, review and native-driven correction

- `5594f64`: initial bounded multi-key contract
- `95b23b3` / `edfba00` / `5f2cf54`: controls, ownership and retirement guidance
- `999abf7`: core group move/delete/mode and same-paint paste operations
- `ad9c979`: independent source/render/codec/history acceptance and CLI helper
- `a4bc33f`: guarded Timeline group controls and shared ownership/modal guards
- **`e8a159a`**: constrain compound lane/details/help to the viewport

Independent review identified source-ABA selection reattachment, repeated Delete
falling through after deleting the selected set, and Shell-only modal continuity
allowing stale clipboard/Move receipts. The UI change fixes these with focused
regressions. Static core review found no confirmed additional overlap, duplicate
paste, identity/allocator, schema or budget/mixed-batch defect. An initial compile
failed because the independent test module was outside its editor-private
helpers; standard editor-child module wiring fixed the access errors. Original logs remain.

The first full-gated release `a4bc33f` passed 1,732 default tests. Actual native
About/selection exposed help-text intrinsic width stretching the Colors lane
while the ruler stayed fixed. No actual native saves were made on that build.
The three-file layout-only correction `e8a159a` adds bounded shrink/wrap behavior
and one regression. All final gates were rerun, and actual native selection then
kept key 15 at x706 and key 45 at x814 before/after selection with wrapped prose.
Initial evidence is retained under its original source and binary identity.

## Final automated gates on e8a159a

All 12 gates in `layout-final/final-gates.tsv` passed:

- Format and locked all-target workspace check.
- **1,733 default tests:** 584 core, 1,143 desktop and six build helpers.
  The default desktop run ignored 44 explicit-input/media/device tests.
- **32 explicit media tests.** Generic media excludes device clock, recorded
  native verifiers/fixture exporters and the separately run system-font case.
- Vendor grid debug/release each passed 215 unit + 43 doc tests, with two doc
  tests ignored; no-default check passed. usvg's eight tests, format and no-text
  check passed, as did the explicit DejaVu ffi ligature case.
- Normal optimized release. Direct Cargo equivalents were used, not Moon.
  Only desktop-package check/test debug metadata was reduced/stripped; release
  settings, assertions and feature configuration stayed unchanged.

Focused UI guards and the final 28 compound Timeline tests overlap the aggregate
suite and are not added to its total. No application build is required for the
later documentation-only checkpoint; its validation is `git diff --check`.

## Independent source and pixels

The 11 independent acceptance tests make **2,080 exact full-RGBA comparisons /
199,680,000 pixels** at 400×240 over 13 sample frames. Literal ordered snapshots
and independent interpolation arithmetic create legacy-static render oracles;
production compound sampling is not its own expected result. Coverage includes
all four Fill/Stroke × Linear/Radial paints; selected-overlap/noncontiguous moves;
complete paste and dormant-ID/mode preservation; group modes; subset/all-key
delete and explicit-playhead bake; ordered-topology Hold; signed-zero/exact bits;
full unrelated source; schema/JSON/LEP/VIEW; atomic rejection/budgets/mixed batches;
and history/no-op Redo. Pixel-distinct oracle checks avoid vacuous comparisons.
No masks or production tolerance changes were used.

The immutable exporter produces 176 JSON+LEP document pairs plus `cases.json`:
**353 files**, with 156 explicit render cases and 10 planned native scenarios.
Generated inputs and expected sources are distinct from actual GUI saves.
Repeating the exporter verifies byte identity rather than replacing changed data.

Final frozen-release CLI qualification makes **780 renders /624 exact RGBA
pairs /59,904,000 pixels**: 156 independent static-oracle pairs, 156 JSON/LEP
pairs, 156 previous/current edited-source pairs and 156 previous/current static-
oracle pairs. The previous binary is the frozen `0bad9d1` interpolation release,
so all 312 cross-release comparisons use the already-supported source format.
Generated hashes remain unchanged before/after qualification.

## Final release identity

Final code: `e8a159a0aae30c98afd8ea5ba2c9d4d14eb3b300`.
Build: **20261004.203455-f5679af4662cfe8a**.
Source: `Git e8a159a0aae3 (clean sources)`; 412 watched input hashes.
Target/profile: `x86_64-unknown-linux-gnu / release`.
Binary: 67,153,824 bytes; SHA-256:
`0aa28ac5ba948d51c4d3681f08b9d5d18dae6a63c0a6753cf948f224a6b7c5d7`.
The actual final native About dialog matched the displayed identity fields.

The first release was `a4bc33f7112d3f5b84c4ee85f86230b17e099087`, build
`20261004.202158-8f7fbc4d873d36b3`, 67,165,744 bytes; SHA-256:
`03d64e1535528dd0f44c539001da70e487dc4c24544bcf6dcb9bb12244572957`.
Its About/selection observations are not relabeled as final-build interactions.

## Bounded native qualification

On the dot cloud Linux desktop, the preceding interpolation editor was closed
normally. Its actual `12-final-reopened.lep` and the earlier Text Animator
`14-final-reopened.lep` remain hash-exact, and the GitHub tab stays open.
The current app is clean on **10-final-reopened90.lep**, frame 90,
with seven Colors keys. All ten actual saves below use final **e8a159a** / build
**20261004.203455-f5679af4662cfe8a**; initial a4bc33f had no native saves.

| Actual saves | Recorded native result |
| --- | --- |
| 01 | About identity and Animated filter; click/Shift-click selects 0/15; Move earliest 0→15 gives 15/30/45; Enter→keyboard Undo/Redo restores both complete states; markers stay ruler-aligned |
| 02–03 | Move earliest→30 rejects collision with unselected 45; invalid `bad` Move blocks Hold before blur and stays focused; Escape reverts the draft and returns Timeline focus |
| 04–05 | Copy 0/15 once, source-neutral navigation to 60→Paste and then 90→Paste yields seven keys; one keyboard Undo removes only 90/105, leaving the first paste; first paste is saved after that Undo |
| 06 | Set both selected keys Smoothstep→Undo, fresh Copy 0/15→exact duplicate Paste at 0 leaves three keys; keyboard Redo restores both Smoothstep modes, proving the no-op preserved Redo |
| 07 | Delete selected 0/15 leaves key 45 and one layer; two discrete Delete presses, Backspace, Cut and Duplicate are consumed without layer/source changes |
| 08 | One Undo restores the pair; Shift-select all 0/15/45, navigate to 30 and Delete selected bakes the independent interpolated frame-30 sample; an extra discrete Delete is consumed |
| 09 | Undo restores animation; Copy→About→Close clears clipboard, and reselect/clicking disabled Paste has no effect; valid pending Move 15 interrupted by Ctrl+Shift+P Search rejects before blur; Escape closes Search without source change |
| 10 | Actual File→New reaches empty Untitled; Open actual 04 restores seven keys/frame 90/workspace; Save As 10 preserves the entire file |

All ten saves pass complete source + complete VIEW and 13-frame, five-route
verification: **650 exact RGBA pairs /62,400,000 pixels**. The only reference
override is the explicitly declared expected frame. Files 04/10 preserve all
6,726 bytes exactly; 02/03/09 preserve all 4,956 bytes exactly. Native files stay
hash-exact during verification. Final native CLI makes **390 renders /260 exact
RGBA pairs /24,960,000 pixels**: 130 actual/reference pairs and 130 previous/
current actual-save pairs, across all ten files and the same 13 sample frames.

The first paste and the intermediate group-mode/Undo/no-op sequence were visually
observed rather than separately saved at each step; the later actual saves and
stated reference checks qualify the recorded final states. Discrete repeated
keys do not establish held-auto-repeat or adversarial event timing.
Native text entry used GTK Copy plus GPUI Ctrl+V, not real marked IME or direct
GPUI text injection. Screenshots were inspected through CUA, not archived.
Generated inputs and images are not native interactions.

## Explicit remaining scope

E04 and the 77-ID inventory remain **Partial**. Pointer group dragging, marquee/
selection gestures, time scaling, compound Graph curves, Bezier/velocity easing,
automatic topology reconciliation, cross-paint bulk and cross-project Contents
clipboard remain separate. This is same-paint internal Copy/Paste only; there is
no Cut, OS clipboard interchange or implicit scalar/compound conversion.

Native noncontiguous group Move, other-paint/lock/source-ABA transitions, direct
Properties/Add-key and Gradient Stroke/radial editing, real marked IME, held-
auto-repeat/adversarial/stale callback timing, other platforms/DPI/devices and
earlier E02/E04 validation gaps remain distinct from automated guards. The
four-paint headless pixel matrix does not establish those native workflows.
The bounded About/Search cases do not qualify every modal or event ordering.

A verified complete-history bundle will be produced after this documentation
checkpoint, before another feature. A defensible editing-first next slice is
bounded pointer group drag/selection gestures, or one advanced Text Animator
selector under a separate contract. No next feature has started. No push, PR, merge, deployment or
user-desktop work occurred.

QA root: `/workspace/shared/libreeffects-qa/gradient-multikey-20261004`.
Root artifacts pin the initial release; **layout-final/** pins final gates,
source manifest, binary and generated/native CLI. `native-cases.json` records
actual source/release/frame/reference attribution and per-save verification. Raw initial
failures, scripts, hashes, immutable inputs and command-level output remain.
