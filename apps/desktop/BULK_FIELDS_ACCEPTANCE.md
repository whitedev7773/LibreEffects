# Shared Contents numeric fields: acceptance record

2026-10-04 UTC. Implementation source: `56ef417b56ad9c641ba0258f20b3b9d861b91ae4`.
This is a bounded continuation, not completion of E04 or native qualification of
all earlier features. The [contract](CONTENTS_BULK_FIELDS_PLAN.md) and
[STATUS](STATUS.md) describe the behavior and remaining work.

## Code and automated gates

| Area | Commit / result |
| --- | --- |
| Trim oracle portability | `4c38913`; test-only two-ULP bound on path data, exact other bytes and same-runtime identity |
| Atomic core assignment | `e1004ce`; 19 new core tests |
| Literal render/codec integration | `8ea6de4`; three tests, five frames each |
| Guarded desktop fields | `56ef417`; 21 session/UI + two TextField policy tests |
| Formatting / type check | Passed locked all-target workspace check and `cargo fmt --all --check` |
| Default workspace | 1,407 passed: 472 core, 929 desktop, six build helpers; 32 media cases ignored here |
| Explicit FFmpeg | 32 passed separately; Windows physical audio device was not exercised |
| Vendor grid | Debug and release: 215 unit +43 doc tests each, two ignored doc examples each; no-std check passed |
| Normal release | Passed, without test debug/strip overrides |

Independent literal documents specify complete static/eased/new-key tracks.
Fifteen fixture frames compare preview, output, literal expected and official-codec
reopened projects: **45 exact RGBA pairs /4,320,000 compared pixels**. Core tests
also cover invalid original/inactive projects, unused path poses, legacy schema
and nonempty assets, signed zero/tiny changes, 10,000 keys, 16 MiB metadata and
exact-return pure batches. Actual EditorState apply/normalize/history preserves
Graph pins/ranges/availability and the established selected-key policy.

The first new-test run found two fixture assumptions (source order/signed-zero
setup) and offscreen reflected geometry; the fixtures were corrected while keeping
all assertions. Independent review found a real second-field select-all hazard;
the final source preserves an unchanged, unmarked, untouched focused field's
selection across rebinding. No legacy test failed in the initial run. The final
aggregate passed after these corrections.

## Pinned executable and CLI

- Build: **`20261004.124444-7f1ec73f65e6e1c8`**
- Built at: `2026-10-04 12:44:44 UTC`
- Embedded source: `Git 56ef417b56ad (clean sources)`
- Target/profile: `x86_64-unknown-linux-gnu / release`
- Watched source inputs: 347; fingerprint `7f1ec73f65e6e1c8`
- Executable: 65,995,840 bytes
- SHA-256: `8247fa2a3eff8fcf61c09db874a13b9d2de1697198372da0897081c5b153f933`

All watched SHA-256 inputs matched the final source before pinning the binary.
The running native About dialog independently showed this same build, source,
fingerprint and release target. Documentation changes after this pin do not alter
those watched inputs or retrospectively change the executable's embedded Git ID.

Pinned CLI checks passed:

1. Three before/expected fixtures, legacy JSON versus generated native LEP, at
   frames0/15/30/45/60: 60 renders, **30 exact RGBA pairs /2,880,000 pixels**.
2. Three actual native saved/reopened files versus independent expected JSON at
   the same five frames: 30 renders, **15 exact pairs /1,440,000 pixels**.

These are source/codec/renderer consistency checks. They do not establish AE
pixel equivalence or native keyboard/IME behavior.

## Bounded native Linux result

The fresh cloud had no Vulkan ICD or file-dialog portal binaries. Official Debian
Mesa25.0.7 llvmpipe and xdg-desktop-portal/GTK were installed workspace-locally;
services started in the existing graphical session only when absent. No security
policy, credentials, host restart or application source was changed to recover
native access. The pinned app then ran in the supported cloud UI with normal
native Open/Save dialogs. The requested GitHub browser tab remained intact.

| Behavior | Actual native result |
| --- | --- |
| Three sibling selection / intersection | Passed: Width Mixed, Height70, PositionX Mixed, PositionY0 at frame30 |
| Mixed value acceptance | Passed using native clipboard paste + Enter: Width100 on Rectangle/Ellipse/Star |
| Track/history preservation | Native SaveAs `01-width100.lep` matched complete independent expected project through the official decoder; the already-equal animated Rectangle gained no key, Ellipse stayed static, Star's existing eased key retained metadata |
| Ordinary blur / consecutive edits | Passed: Width80 blur to Height kept Height70 select-all; paste80 produced Height80, not7080, with three-item selection retained |
| Separate history steps | Passed: one Undo restored only Height70; the next restored Width100; Redo restored both; actual `02-width80-height80.lep` matched independent expected metadata |
| Empty / Escape / unchanged input | Passed: empty Enter reported finite-value error and restored source; Escape discarded a cleared draft; unchanged and rejected input preserved Redo |
| Lock / playback | Passed: locked-layer controls rendered read-only; active playback advanced to frame67 with read-only sampled shared values; stopped afterward |
| Native reopen/resave | Passed: reopened `02-width80-height80.lep`, restored frame30 and saved `03-reopened.lep`; complete LEP bytes were identical, including PROJ and VIEW |
| About identity | Passed: visually matched the pinned automatic build metadata above |

Tool limitations are explicit: direct `type_text` on the GPUI window returned an
AT-SPI-provider-unavailable error, and synthetic digit key presses did not insert
text. Normal native Ctrl+C/Ctrl+V input, Enter, Escape, Backspace, mouse selection,
Undo/Redo and file dialogs were exercised instead. This run does **not** claim
native direct character typing or real marked Korean IME acceptance.

Still unrun natively for this new slice: paste-before-tree-selection ownership,
hierarchy movement after bulk edits, nonfinite/out-of-bounds submissions,
solid/gradient opacity and endpoint-specific UI fixtures, adversarial stale/ABA
callback timing, broader modal transitions, real IME and Windows/macOS/DPI cases.
Their applicable model/render tests are green; they are not substitutes for
native event delivery. The original eight-case plan is therefore **partially
qualified**, not eight native passes. The earlier 47 native cases remain unrun in
this iteration and must not be credited from these checks. No source-review or
executed-check defect remained when this iteration ended.

## Reproducible local evidence

Machine-local QA root: `/workspace/shared/libreeffects-qa/resume-20261004`.
It holds command/exit logs (`final-gates.tsv`), source manifest, pinned executable
and `release.json`, immutable generated fixtures, exact CLI results, actual native
LEP files, the independent expected repeated-edit JSON and official-core verifier.
Native visuals were observed through the supported cloud screen tool; no
standalone screenshot files were exported. These local paths are not remote
GitHub artifacts. New commits were not pushed and fresh Windows CI remains unrun.
