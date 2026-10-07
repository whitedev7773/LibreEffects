# Compound Gradient Colors interpolation acceptance — 2026-10-04

Bounded E04 continuation from `643cda8`; read
[the source/UI contract](GRADIENT_INTERPOLATION_PLAN.md). The existing Hold-only
checkpoint remains separately documented in
[CONTENTS_ANIMATION_ACCEPTANCE.md](CONTENTS_ANIMATION_ACCEPTANCE.md).

## Implemented boundary

Complete snapshots retain paint-local ordered stop IDs. Hold is the unchanged
sparse default. Linear and Smoothstep interpolate every stop number only when
both independent rows have the same ordered IDs. Added, removed or reordered
IDs explicitly hold the left snapshot until the next key, with a visible reason;
requested modes remain stored. Position crossings are compatible. Exact key
snapshots and bit-identical signed-zero fields retain their bits and tie order.

Outgoing non-Hold metadata requires schema57, including a dormant final-key mode.
Hold-only compound source remains schema54 with unchanged serialization. JSON,
LEP1, VIEW and numeric-address contracts are unchanged otherwise. Moving keys or
converting frame rate moves their metadata; collisions reject. Delete, Disable,
mode and sample edits preserve dedicated source/history/no-op/budget contracts.
New mode/deletion commands cannot escape those contracts through mixed batches.

Properties adds exact-key modes/status. Dedicated Timeline lanes support key
selection/navigation, exact Move frame, Delete and mode controls, also under the
Animated filter. Pending input is rejected before blur. Frozen source/input/
transport/key receipts retire stale actions. A transient selection-domain latch
prevents generic Delete/Cut/Copy/Duplicate and similar commands acting on an
entire layer. Escape/explicit domain navigation releases it. The optional modal
shortcut requires that paint already selected in Contents. No scalar Graph
address is invented.

## Local commits and review

- `bbd9b8a`: explicit interpolation/topology/UI contract
- `c4f1ea4`: core sampling, schema, commands, metadata and source/history tests
- `523a0c8`: guarded Timeline/Properties controls and global selection isolation
- `76a164e`: user-facing semantics and bounded editing guidance
- `b0f6851`: independent source/render/codec/history acceptance and CLI helper
- `0bad9d1`: final narrow Move-field Timeline focus return and regression

Independent review found and repaired signed-zero interpolation changing
coincident-stop order, rejected pending input allowing a button, repeated whole-
project clones/duplicate disabled IDs, and compound selection falling through to
whole-layer actions. Shell Escape releases ownership. Review of the final focus
correction found no remaining confirmed defect. The focused first UI test had
one fixture-order error: Contents Add prepends nodes; expected order was corrected
without changing production order or weakening an assertion. Original logs remain.

## Final automated gates on 0bad9d1

- Format and locked all-target workspace check passed.
- **1,690 default tests passed:** 569 core, 1,115 desktop, six build helpers.
  The default desktop run ignored 42 explicit-input/media/device tests.
- **32 explicit media tests passed.** Generic media excludes device clock, all
  existing native-file verifiers/exporters, these two new fixture tests and the
  separately executed system-font ligature case.
- Vendor grid debug/release each passed 215 unit  + 43 doc tests; no-default check
  passed. usvg tests, formatting, no-text check and explicit DejaVu ffi case passed.
- Normal optimized release passed. Direct Cargo equivalents were used, not Moon.
  Final check/test overrides reduce only desktop package debug metadata; release
  settings, assertions and feature configuration stay unchanged.
- Focused final coverage: 27 compound core, 13 Timeline, 5 Properties, 3 global
  selection-domain and 11 independent acceptance tests. These overlap the full
  suite and are not added to the aggregate count.

The initial UI check and supplemental independent run used a broader debug-
metadata override; their logs are retained. The UI check was stopped and both
qualifications were repeated with the exact pinned package-only configuration.
Final gates above are the corrected commands. No release-profile change occurred.

## Independent source and pixels

The 11 independent tests make **1,330 exact full-RGBA comparisons /
127,680,000 pixels** at 400×240. Literal ordered stop snapshots and independent
arithmetic create legacy-static render oracles; production compound sampling is
not its own expected result. Coverage includes all four Fill/Stroke × Linear/
Radial paints, Linear/Smoothstep, unequal color/opacity counts, six independent
topology mismatches, crossing/tie ordering, signed zeros, dormant terminal mode,
retiming, deletion, sample insertion, Disable, history, no-op Redo and official
JSON/LEP/VIEW round trips. A forced signed-zero change demonstrably changes pixels,
so that oracle is nonvacuous. No comparison masks or production tolerance changes.

The immutable exporter produces 135 JSON+LEP document pairs plus `cases.json`:
271 files total, 108 explicit render cases and 12 planned native scenarios.
Original generated inputs remain separate from actual GUI saves. Repeating the
exporter verifies byte identity rather than overwriting different data.

Final frozen-release CLI qualification makes **432 renders /288 exact RGBA
pairs /27,648,000 pixels**. This includes 108 independent static-oracle pairs,
108 JSON/LEP pairs and 72 comparisons with the prior schema56 Text Animator release
across four schema53 static and four schema54 Hold paints. That prior release
also explicitly rejected both new schema57 JSON and LEP before altering existing
output sentinels.

## Final release identity

Final code: `0bad9d1725bdcdd44c434e411f3ab3fe6ba45428`.
Build: **20261004.194304-fb65ebd14f0ff145**.
Source: `Git 0bad9d1725bd (clean sources)`; 410 watched input hashes.
Target/profile: `x86_64-unknown-linux-gnu / release`.
Binary: 67,071,632 bytes; SHA-256:
`0546ce35e3e8e6dd3a77559cbc1fdde802b146b42a8859e8f69a65433e1b4cc5`.
The actual native About dialog matched all displayed identity fields.

The earlier native recording used `b0f6851` and build
`20261004.192651-6fb3e1bf7ecb074a` (67,068,648 bytes;
SHA-256 `04a4e73cfc3f6508c9556defec91838f522eb92b8e36a76e5bcf9dc1cade5534`).
Its identity and evidence are retained, not relabeled as final-build interactions.

## Bounded native qualification

On the supported dot cloud Linux desktop, the prior saved Text Animator editor
was closed normally. Its `14-final-reopened.lep` stays hash-exact and the GitHub
tab remains open. The current app is clean on **12-final-reopened.lep**, frame 30.

| Recording | Actual native result |
| --- | --- |
| b0f6851, 01–02 | Animated-filter lane, key 15 selection, Linear/Smoothstep, one-step mode Undo/Redo; unchanged Linear preserved Redo |
| b0f6851, 02 | Selected-key Backspace/Cut/Duplicate consumed with explicit guidance; whole layer/source unchanged |
| b0f6851, 03–04 | Typed Move 15→30 follows key; occupied 75 rejects; invalid pending text blocks Hold and stays focused; Escape reverts draft |
| b0f6851, 05–06 | Explicit Delete removes one snapshot and retains layer; native menu Undo restores complete source |
| b0f6851, 07–08 | Exact keyboard stepping to 45, Smoothstep sample, Disable bakes it; menu Undo restores both keys |
| b0f6851, 09–10 | Reordered-ID fixture visibly explains requested Linear / Hold fallback; frame 45 stays held, Next reaches exact 75/new snapshot |
| b0f6851, visual only | Narrow Properties sidebar exposes all wrapped mode labels and last-key explanation through ordinary scrolling |
| 0bad9d1, 11 | Actual Open of saved 06; Move Enter → keyboard Undo/Redo works without another focus click; invalid draft still blocks Hold; Escape → keyboard Undo/Redo works |
| 0bad9d1, 12 | Actual New → Open 11 → Save As 12 restores source, frame and workspace; complete file is byte-identical |

The initial keyboard-Undo symptom occurred after a typed Move/save/click sequence;
menu Undo worked. Inspection found the new Move field had no return-focus target.
The narrow correction gives Enter/Escape the owning Timeline focus, then the two
immediate keyboard sequences above were actually rerun. Earlier interactions are
not claimed as a full native rerun, and file-dialog focus behavior is not broadly
redefined by this fix.

All 12 actual saves pass final-code complete source + complete VIEW + nine-frame,
five-route verification: **540 exact RGBA pairs /51,840,000 pixels**. The only
reference override is each explicitly declared expected frame. Final native CLI
makes **216 renders /108 exact pairs /10,368,000 pixels**. Files 03/04/11/12 each
preserve all 4,514 bytes exactly; 02/06 are also byte-identical.

Native typing used GTK text entry Copy and GPUI Ctrl+V, not direct GPUI text
injection or real marked IME. Screenshots were inspected through CUA; screenshot
image files were not archived. Generated images/files are not native interactions.

## Explicit remaining scope

E04 and the 77-ID inventory remain Partial. This slice does not add compound key
pointer dragging, marquee/multi-key edits, key clipboard, Graph curves, Bezier/
velocity easing, automatic topology reconciliation, cross-gradient bulk Colors,
or cross-composition/project Contents clipboard. Legacy scalar stops remain a
separate alternative with no lossy implicit conversion.

Native Add-key, final-key removal/dormant mode activation, Properties mode edits,
Gradient Stroke/radial direct editing, modal draft editing under interpolation,
held/canceled/stale/adversarial input order, real marked IME, all supported fonts,
Windows/macOS/other DPI and physical audio remain separately unqualified. Existing
47-case and prior E02/E04 native gaps are not closed by these tests. Pointer key
retiming and compound key clipboard are not implemented here.

QA root: `/workspace/shared/libreeffects-qa/gradient-interpolation-20261004`.
Root artifacts pin the first release; **focus-final/** pins final gates, binary,
source manifest and final-code verification. `native-cases.json` records each
actual source/release/frame/reference. Gate scripts, raw failures, hashes,
immutable fixtures and command-level CLI logs remain available. No push, PR,
merge, deployment or user-desktop work occurred.
