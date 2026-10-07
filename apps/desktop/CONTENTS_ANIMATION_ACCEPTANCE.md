# Contents animation acceptance — 2026-10-04

This records the bounded Hold-only compound Gradient Colors and explicit shared
numeric animation milestone. Read [GRADIENT_COLORS_PLAN.md](GRADIENT_COLORS_PLAN.md)
and [CONTENTS_BULK_FIELDS_PLAN.md](CONTENTS_BULK_FIELDS_PLAN.md) for source semantics.
The prior shared-field and 47 older native cases remain separately qualified.

## Final automated gate on 3b7e20f

- `cargo fmt --all --check` and locked all-target workspace check passed.
- **1, 470 default tests passed**: 498 core + 966 desktop + 6 build helpers.
  The default run ignored 33 tests: 32 media cases plus the file-input native verifier.
- **32 explicit media tests passed**; device-clock and native-save verifier were
  deliberately excluded from this generic media run.
- Vendor grid debug/release each passed 215 unit + 43 doc tests (two ignored doc
  examples); no-default-features check passed.
- Focused subsets: 16 compound core, 10 shared animation core, 30 bulk UI/session,
  11 pointer-policy, 8 compound modal, 4 compound controls and 14 independent
  render/codec acceptance tests. These overlap the aggregate; do not add twice.
- Independent 14 scenarios used 126 frame sets at 400×240, yielding 338 meaningful
  exact RGBA pairs /32, 448, 000 compared pixels. Of these, 86 pairs compare literal
  independently constructed tracks or explicitly materialized legacy gradients.
  Deterministic same-document references are excluded from that accounting.
- Full project/history/Redo, official LEP source/VIEW, inactive compositions,
  dormant tracks/poses, easing metadata, signed-zero tie ordering and bounds were
  covered. 47 distinct generated documents were exported in JSON+LEP (94 files).
- Actual previous schema 53 release rejected new schema 54 JSON and LEP with its
  explicit unsupported-version diagnostic, leaving existing output byte-exact.
- Direct Cargo equivalents used the existing workspace-local Rust 1.97 and Linux
  libraries. Only desktop check/test debug metadata was reduced; assertions,
  features, optimization and normal release configuration remain unchanged.
  Moon was not run. Existing non-fatal Linux/vendor warnings remain.

Review found and repaired fractional-RGB return-to-display creating a needless
key, non-left guarded clicks bypassing pre-blur rejection, and a two-step private
signed-zero draft bridge that needed atomic adoption. Independent regression and
source review passed after those repairs. Initial new-test fixture setup errors
and two borrow-order compiler errors were corrected without weakening assertions.
No legacy regression remains known in the executed gate.

## Frozen optimized release and CLI

Normal release passed in 95 seconds. Build number:
`20261004.140046-f635623df359cd3b`; embedded source:
`Git 3b7e20f12226 (clean sources)`; target/profile:
`x86_64-unknown-linux-gnu / release`. The 352 watched input hashes match fingerprint
`f635623df359cd3b`. Binary size 66, 292, 616 bytes; SHA-256:
`1115ca4ca699ecfed6c616ee0c0189e767bd700dbdcdca1abee29a0def513746`.
The running About dialog matched all of those displayed identity fields.

Pinned-release CLI passed 78 generated-fixture renders /48 exact RGBA pairs /
4, 608, 000 pixels, including compound frames against explicit legacy-static
references. The old schema 53 binary separately refused both new formats before
modifying existing output. Original generated fixtures remained immutable.

## Bounded native qualification

Executed on the supported dot cloud Linux desktop and the frozen release above.
The previous saved editor was closed normally; the requested GitHub tab remained
untouched. This is partial qualification of the prepared 14-case menu.

| Area | Actual result |
| --- | --- |
| Shared selection and summaries | Four nested heterogeneous siblings, exact Mixed value/animation/key summaries, retained selection across consecutive actions |
| Explicit Enable/Add/Disable/Remove | All four actions performed; actual saves 02–05 matched full independent expected source and VIEW; existing easing/dormant data/final-key samples were retained |
| Shared history | One Undo/Redo around Enable and one Undo around Disable restored the prior states; in-app summaries/clean-state evidence matched |
| Compound Fill enable | Hold key at 30, explicit all-key clearing disclosure, legacy stop controls unavailable, independent endpoint Graph pins; native save 06 matched full schema 54 expected source/VIEW |
| Compound modal cancellation/error | Add-opacity draft then Cancel retained clean source; invalid HEX reported rejection and Escape restored/canceled without a source edit |
| Compound modal Apply/history | Native clipboard HEX 000033, new opacity stop 9 and location/midpoint/opacity 33 committed together; one Undo restored the clean saved source, Redo restored the edited snapshot; save 07 matched a separately specified literal expected project/VIEW |
| Native Open/resave | Open 07 then Save As 08 preserved all 5, 677 bytes including VIEW, at frame 30; final app remained clean |
| Official native-file checks | Seven actual files 02–08 passed complete source+VIEW and nine-frame preview/output/reference/codec checks each |
| Pinned-release native CLI |42 renders /21 exact RGBA pairs /2, 016, 000 pixels from those seven actual saves and their independent expectations |

The first strict check on file 01 correctly reported a VIEW difference after the
operator deliberately resized the Properties panel (fraction 0.6889936 instead of
fixture 0.84). Source and composition VIEW were already exact. Reset Workspace
restored the known layout; file 02 passed the unchanged full-source/VIEW verifier.
The initial failure log is retained as setup evidence, not silently discarded.

Native input used ordinary copy in a GTK text entry plus actual Ctrl+V/Enter in
GPUI. Direct GPUI text injection and bound-window paste reported unavailable
AT-SPI support. Do not label this direct typing or real marked-Korean-IME coverage.
Screenshots were inspected in the supported UI session; local saved-file evidence
and CLI PNGs are distinct from screenshot files not supplied by that tool.

Explicitly unrun native subcases: Gradient Stroke enable/disable, multikey Hold
boundary scrubbing, Previous/Next, compound final-key removal, new-control
lock/playback/stale/modal races, pending shared field→animation receipts,
non-left guarded presses, unchanged/away-back modal OK and stop-removal Apply,
real marked IME, Windows/macOS/other DPI and physical audio. Native MoveKey UI is
not implemented. The prior 47 native cases remain independently pending.

QA evidence is machine-local under
`/workspace/shared/libreeffects-qa/contents-animation-20261004`: exact gate logs,
source/release manifests, generated fixtures, native interaction report and saves,
strict verifier logs, compatibility checks and CLI reports. No push, PR, merge or
deployment was performed. E04 remains Partial.

## Scope and limitations

Hold Colors keys own complete independent stop rows, including topology and tie
order. Explicit shared numeric actions preserve per-member values and existing
metadata. No cross-gradient stop correspondence is inferred. Native Colors timing
navigation exists; retiming is core-only. No Timeline/Graph compound lane, smooth
compound interpolation, cross-paint bulk Colors or Contents clipboard is added.

Existing `.lfe.json` and LEP1 sources remain supported. Compound animation requires
schema 54; omitted legacy fields remain omitted. Declared older schemas with new
representation and future schemas reject. VIEW1/2 and numeric-address1 stay.
No legacy source fixture or production float tolerance is relaxed.

Native evidence must identify the frozen release and distinguish clipboard input
from direct typing/marked Korean IME. Generated fixtures are not native saves.
The older 47 unrun cases and Windows/macOS/DPI/device-audio gaps remain open.
