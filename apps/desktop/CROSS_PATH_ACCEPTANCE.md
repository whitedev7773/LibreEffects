# Cross-path canvas editing acceptance — 2026-10-04

This records the bounded E02 current-frame Contents milestone. Read
[the contract](CROSS_PATH_PLAN.md) for semantics. Earlier all-stored-pose native
gaps, the older 47 cases and other platform/IME gaps remain separate evidence.

## Implementation and independent review

- `b09ad54`: separately committed bounded contract before implementation.
- `ba54bc3`: source-preserving world-affine core command and geometric helper.
- `14973a1`: nine independent interruption/receipt guard regressions.
- `b5edb9e`: independent complete-source/render/official-codec/VIEW acceptance.
- `7c3e946`: guarded cross-path selection and affine canvas UI.
- `e5bad8c`: one-line help caption fix found in preliminary native qualification.
- `8f3cc72`: final Transform ON caption and related Pen tooltip correction;
  exactly two string-literal changes, with no behavioral code change.

The new route is limited to enabled Bezier Contents paths on one selected layer
in the active composition. It edits sampled current-frame poses through actual
nested/layer world transforms without conversion. Static/dormant/animated data,
unselected vertices, original pose slots, other keys and existing key metadata
are preserved. Interpolated neighboring frames can change after key editing.
Existing supported inverse and storage/geometry/metadata limits apply. All paths
commit atomically; exact no-op/return-to-start preserves history and Redo.

Independent review identified and repaired primary-selection normalization,
irrelevant huge-pivot translation arithmetic, strict one-use post-dispatch
receipts, color-sampler priority, outside-down retirement, cancellation of queued
transient previews, held-Delete topology fallthrough and idle viewport-action
selection retention. New held affine
and marquee receipts retain full action/transport/source/view validation. No
remaining concrete source defect was found in that review; final gates and native
execution are recorded below rather than inferred from source inspection.

## Automated validation

Final automated gates on `8f3cc72` passed:

- **1,550 default tests**: 528 core + 1,016 desktop + six build helpers.
  The default desktop suite ignored 35 tests: 32 media cases and three
  actual-native-file verifiers that need separately supplied input files.
- **32 explicit media tests** passed. Physical device clock and all three file
  verifiers were excluded from the generic media command.
- Format, locked all-target workspace check and vendor debug/release/no-default-
  features gates passed. Vendor debug/release each passed 215 unit + 43 doc tests,
  with two intentional ignored doc examples.
- Fourteen new core regressions cover literal world geometry, nested animated
  transforms, source/history/schema/assets, signed-zero pose reuse, geometry/
  pool/key bounds and real original/final metadata-cap boundaries.
- All **128 focused Pen tests** passed, including ten new affine/domain and nine
  independent guard tests. These overlap the aggregate and are not counted twice.
- Seven independent tests cover 12 nine-frame scenarios at 400×240:
  **324 exact RGBA pairs / 31,104,000 pixels**. Complete source/history/VIEW,
  sampled/existing/eased/dormant paths, reflection/skew/animated ancestry, no-op
  and invalid atomicity, official JSON/LEP round trips and semantic differences
  are checked. Only selected geometry in the separately composed oblique oracle
  uses a 1e-10 absolute bound; all other source and its rendered pixels are exact.
- Nineteen immutable generated documents / 38 JSON+LEP files are retained with
  SHA-256 manifests. They are not actual native saves.

The pinned Rust1.97/Linux helpers were reused. Cargo equivalents were used,
not Moon; only desktop package check/test debug metadata was reduced. Assertions,
features, optimization and the normal release profile were unchanged. Existing
non-fatal Linux/vendor warnings remain.

Initial test preparation errors were corrected without weakening production
contracts: a legacy-image fixture needed its required asset ID; an independent
path fixture incorrectly used scalar temporal handles on opaque pose-reference
keys; new UI test code used obsolete enum names and a mathematically equal but
not bit-identical transformed-coordinate assertion. The existing same-path-only
Contents marquee expectation was updated to the explicitly expanded domain.
An initial broad-debug compilation configuration was not accepted as a gate;
subsequent runs use the established package-specific overrides. First logs and
generated fixtures remain retained separately.

## Frozen release and CLI

Final normal release on clean `8f3cc72`: build
`20261004.164023-bc0e287e4faa3aab`, UTC `2026-10-04 16:40:23`,
`x86_64-unknown-linux-gnu / release`, 363 watched inputs, 66,650,072 bytes.
SHA-256: `faa475443464cf77240ad8053ee9870d820f58b8dd7f165f8c19462caa5d747a`.
The running About matched every displayed field. Generated CLI passed
**93 renders / 36 exact pairs / 3,456,000 pixels** with 12 changed scenarios.
Native-file CLI passed **36 renders / 18 exact pairs / 1,728,000 pixels**,
using the two separately qualified precision references where explicitly required.

The complete geometry qualification pin is `e5bad8c`, build
`20261004.161843-f1d51f5bef78a227`, clean sources, 363 watched inputs,
66,649,960 bytes, SHA-256
`4a6f10cfe1f8c4acb2f30bb3a61e4bc457c8f4f0132c242eb651ede4289dbd4d`.
Its About matched; CLI passed 93 renders / 36 exact pairs / 3,456,000 pixels.
The earlier `7c3e946` preliminary pin is retained separately and is not relabeled.

## Bounded native qualification

Executed using supported full-desktop CUA on the dot cloud Linux computer.
The previous clean `09-reopened.lep` editor was preserved until a qualified
replacement existed, then closed normally. Its saved bytes remain unchanged;
the requested GitHub tab remains open. Terminal text entry was unsupported, so
the desktop Alt+F2 launcher ran the qualified launch script. Screenshots were
inspected through CUA; no local screenshot files were supplied by that tool.

The geometry matrix below belongs to **e5bad8c**. The final text-only diff is
retained in `final-text-only-diff.patch`; it contains exactly two help literals
and no behavior change. Final **8f3cc72** smoke confirmed readable ON and OFF
captions in the default 1180px window, the corrected Pen tooltip, keyboard and
mouse-toolbar toggles, matching About, actual Open of 05 and Save As 06. All 9,064
bytes of 05 and 06 are identical; the complete literal source/VIEW and nine-frame
verifier passed. The app is left clean on `06-final-smoke.lep`.

| Area | Actual observed outcome |
| --- | --- |
| Selection | Initializing Shift marquee selected eight anchors across two enabled paths; separate Shift-click toggled anchors across paths; Ctrl+A selected the eligible layer domain, excluding disabled subtrees |
| Move | Cross-path anchor drag moved both paths together; one Undo restored baseline and one Redo restored move; actual 01 save passed the narrow common-translation qualification below |
| Scale | Shift+T exposed corners and fixed center; horizontal Shift corner drag uniformly scaled both paths about [180,100]; one Undo/Redo worked; actual 02 passed the narrow one-factor qualification below |
| Rotation | Top-handle drag with Shift snapped 90°; one Undo/Redo worked; actual 03 matched the complete literal source/VIEW and nine-frame reference exactly |
| Guarded no-op | Cross-path Delete, Shift+V and Shift+R did not edit or open an unrelated dialog; return-to-start movement and Escape from transform mode preserved Redo; Redo restored rotation, Undo restored baseline; actual 04 matched the complete original source/VIEW and nine-frame reference |
| Actual reopen | Opened actual 03 and saved 05 at frame 30; complete source/VIEW and nine-frame checks passed; all 9,064 bytes were identical |

### Native pointer precision distinction

The original exact target-coordinate checks for 01/02 **failed and remain failed**
against the immutable literal 20×10 move and 1.5 scale expectations. No production
code, original expected file or strict official verifier was loosened.

For 01, a separate QA-only check infers two translation components from one anchor,
requires an error below 1/1024 composition pixel per axis, then predicts every
other selected anchor exactly and keeps all 32 tangent components and every
remaining source/VIEW value exact. The common delivered delta was
`[20.00030517578125, 10.000152587890625]`, consistent with `(1 + 2^-16)` scaling.
That pattern is consistent with input normalization; its cause was not proven
without delivered-event logging.

For 02, one factor inferred from one tangent is
`1.499965963991311`, with fixed pivot [180,100] and zero translation. All 47 other
selected anchor/tangent components are predicted bit-exact; all remaining source
and VIEW are exact. Its difference from intended 1.5 is 0.000034036, with maximum
anchor discrepancy 0.004425 pixel. A separately disclosed ≤1/64-pixel error
assumption for each absolute endpoint derives the narrow factor interval
`[1.499699555341906, 1.500300516889049]`. This is a bounded qualification assumption,
not an independently proven OS/app attribution or exact 150% native-input claim.

These independent algebraic constructions produce separate precision-reference
LEPs; the unchanged official decoder/source/VIEW and nine-frame renderer verifier
passes against them. Eight negative controls reject stray anchor/tangent edits,
dormant-pose or unrelated metadata edits, easing/VIEW changes and transforms
outside the stated bounds. The literal failures and distinct precision reports
are retained under the QA root. The six actual files yield 162 exact RGBA pairs
/15,552,000 pixels after the explicitly scoped reference qualification.

Native unrun: true held-pointer Escape, adversarial lock/playback/modal/source-
return races, pending/marked text interactions, singular/reflected/skewed native
fixtures, nonuniform/reflected/zero scale, arbitrary unsnapped rotation, other
zoom/DPI/platform/device configurations and real marked Korean IME. Automated
evidence does not replace these native gaps. The prior 47-case and all-stored-pose
native backlogs remain separate. The bounded matrix was not expanded further.

## Remaining scope and evidence

Cross-layer/composition/mask sets, custom pivots, affine canvas skew, world-space
all-stored-pose editing, topology animation and parametric conversion remain
outside this slice. Linked tangent controls and broader editing/accessibility
remain separate. E02 and the full 77-ID inventory remain Partial.

Machine-local evidence is under
`/workspace/shared/libreeffects-qa/cross-path-20261004`. Work is local on the dot
cloud computer; no push, PR, merge, deploy or user-desktop action is included.
