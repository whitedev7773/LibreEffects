# Contents sibling clipboard acceptance — 2026-10-04

This records the bounded session-local sibling clipboard milestone. Read
[the contract](CONTENTS_CLIPBOARD_PLAN.md) for semantics. Earlier Colors/shared
field native gaps and the older 47 native cases remain separate evidence.

## Implementation

- `a70d6b8`: immutable exact sibling snapshots and atomic source-preserving Cut/Paste.
- `851657d`: independent complete-source/render/official-LEP/VIEW acceptance.
- `a0c3fa0`: guarded tree/button Copy/Cut/Paste, pending-field receipts, clipboard
  domains, selection/reveal, and shell menu/search ownership.

Copy has no history entry. Cut replaces the clipboard only after its one deletion
transaction succeeds; Paste is one transaction with fresh recursive layer-local
node IDs. Gradient-local stop IDs/allocator, path-pose indices and original frames
remain intact. Destination transforms and paint scope apply. A preceding accepted
field edit keeps its own existing history step, separate from the clipboard action.
No new project, LEP, VIEW or address version is introduced.

## Final automated gate on a0c3fa0

- `cargo fmt --all --check` and locked all-target workspace check passed.
- **1,510 default tests passed**: 514 core + 990 desktop + six build helpers.
  Default desktop run intentionally ignored 34 tests: 32 media cases and two
  actual-native-file verifiers requiring separately supplied inputs.
- **32 explicit media tests passed**; physical device-clock and both file-input
  verifiers were excluded from this generic media command.
- Vendor grid debug/release each passed 215 unit + 43 doc tests (two ignored doc
  examples); no-default-features check passed.
- Focused core: 16 passed, including two exact metadata-budget boundary tests.
  Focused UI: 16 clipboard/session, nine menu/search and 11 pointer-policy cases;
  the broader 144-test focused pass overlaps the aggregate and is not added again.
- Six independent acceptance tests passed. Thirteen nine-frame scenarios at
  400×240 yielded **351 exact RGBA pairs /33,696,000 pixels**, comparing actual
  preview/output, independent literal expected source and official-codec results.
  Complete source/history/Redo, recursive IDs, source order, dormant tracks/handles,
  unused path poses, disabled/reflected/skewed Groups, variable-topology Fill/Stroke
  Colors, schema 44/53/54, source edit/deletion, and Graph/VIEW were covered.
- Nineteen immutable generated project fixtures were exported as JSON+LEP
  (38 files), with SHA-256 manifest rechecked after native work. They are not
  actual native saves.
- Existing pinned Rust1.97/Linux libraries were reused. Direct Cargo equivalents
  were used; Moon was not run. Desktop check/test reduced debug metadata only;
  assertions, features, optimization and normal release settings stayed unchanged.
  Existing non-fatal Linux/vendor warnings remain.

Independent review found and repaired field-identity/action/transport receipt
holes, wrong-field callbacks mutating before refusal, canceled-press arms blocking
later inputs, GPUI mouse-down focus ownership, stale menu receipts, header switching
and menu→Find command/cancellation cleanup. Final source review was clear. Initial
new-test fixture errors (gradient units, legacy setup, expected schema promotion
and offscreen native geometry) were corrected without weakening assertions;
initial logs/fixtures remain retained. No executed app/test defect remains known.

## Frozen release and CLI

Normal optimized release passed in 102 seconds. Build:
`20261004.150318-c0b487e1e7a2ed85`; embedded source:
`Git a0c3fa068aa8 (clean sources)`; UTC: `2026-10-04 15:03:18 UTC`;
target/profile: `x86_64-unknown-linux-gnu / release`.
All 357 watched inputs match fingerprint `c0b487e1e7a2ed85`.
Binary size: 66,341,792 bytes. SHA-256:
`bf0a1fed28cf93c85ff380b344da6e894350c6b73a7092a8ca6e8f32ccbd9651`.
Actual About matched the displayed identity fields.

Pinned CLI passed **90 generated-fixture renders /45 exact pairs /4,320,000 pixels**.
The initial helper used the prepended Debian Python without Pillow; using the
already installed full runtime Python resolved that QA-script import error. No
package install or application-source change was needed.

## Bounded native qualification

Executed through supported full-desktop CUA on the dot cloud Linux computer.
The previous clean saved editor was closed normally only after the replacement
release was ready. The requested GitHub browser tab remained intact.

| Area | Actual observed outcome |
| --- | --- |
| Group Copy/Paste | Tree Ctrl+C/V copied the complete animated Group and appended into Destination; new root selected/revealed; actual save matched full source/VIEW |
| Repeated Paste/history | Reselected Destination, pasted fresh recursive IDs 15/16/17; one Undo returned clean saved source, Redo restored repeat; strict save pass |
| Cut/history/snapshot | Tree Ctrl+X, one Undo restored clean original, Redo restored Cut; guarded Paste button reused snapshot after removal; both strict save passes |
| Root append | Ctrl-click cleared selection; UI stated 0 selected/tree focused/end of Contents; Paste button appended root; strict save pass |
| Cross-layer paste | Same-composition destination Group accepted original local payload; visible parent rotation/placement applied; strict source/VIEW save pass |
| Mouse menu/search ownership | Tree→mouse Edit disabled generic Copy/Cut/Paste; disabled Cut did not mutate; header switch→Help→Find command retained explicit unavailable states; Escape/outside dismissal then Timeline focus restored Copy/Cut availability |
| Invalid pending field | OS-pasted bad numeric text followed by Copy restored 100, showed finite-value error, kept clean source and empty clipboard; Paste remained unavailable |
| Valid pending field | OS-pasted 110 without Enter followed by one Copy click accepted the field and copied its resulting snapshot; field-only save and subsequent Paste save matched independent X110 source/schema 44/VIEW |
| Text clipboard | Focused PositionX Ctrl+C/X affected only text draft; Ctrl+V restored 110 and Escape canceled, with clean document and unchanged artwork/layers |
| Actual Open/resave | Opened actual 08, refocused workspace, saved 09; all 6,435 bytes including VIEW were identical at frame 30 |

Nine accepted actual files (01-group-default-view and02–09) passed the strict
complete-source+VIEW official decoder and nine-frame preview/output/reference/
codec verifier. Pinned native-file CLI passed **54 renders /27 exact pairs /
2,592,000 pixels**. The app remains clean on `09-reopened.lep`.

Initial01-group.lep correctly failed strict VIEW comparison because the operator
collapsed the timeline (`expanded=false`) to expose the source layer. Its source
was already exact. Restoring normal disclosure then saving a separate file passed
the unchanged verifier. The first file/failure log remain as setup evidence.
Properties was temporarily enlarged for simultaneous Copy/field access, then
Reset Workspace restored the independent default before accepted saves.
After actual Open, the first Save As chord without workspace refocus did not open
a chooser; clicking the Timeline then using Save As worked. This is recorded as
focus/transport behavior, not evidence of broader platform shortcut qualification.

Input used ordinary GTK-entry Copy then actual Ctrl+V in GPUI. This is not direct
typing or real marked Korean IME coverage. Screenshots were inspected through CUA;
no locally saved screenshot files were supplied by that tool in this pass.

## Explicit native gaps and remaining scope

Unrun natively: noncontiguous/disabled/compound-Colors clipboard fixtures, source
layer deletion after Copy, shared-field pending→clipboard, lock/playback/modal/
held-key and adversarial stale/canceled-pointer races. These have automated or
source-review evidence only. Real marked Korean IME, Windows/macOS/other DPI and
physical audio remain unrun. The older 47 native cases and earlier Colors gaps
remain independently pending.

Cross-composition/FPS conversion, cross-project/OS clipboard, implicit legacy Shape
conversion, cross-parent source selection, coordinate compensation and time-shift
paste are excluded. Incompatible old-schema payload combinations reject rather
than silently migrate their defaults. Compound interpolation/Timeline lanes and
cross-paint bulk Colors remain separate. E04 remains Partial.

Machine-local evidence: `/workspace/shared/libreeffects-qa/contents-clipboard-20261004`
contains exact logs/commands, source and release manifests, generated/native files,
strict verifier reports and CLI comparisons. No push, PR, merge or deployment was
performed. Next candidate is a separately designed E02 cross-path selection and
local/world affine canvas-editing milestone; it has not started here.
