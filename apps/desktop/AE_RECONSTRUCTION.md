## Scope update: native JSX/ScriptUI (2026-10-06)

The user clarified that AEP opening/import is unnecessary. The existing reader/
importer and its historical verification remain recorded below; new ingestion
work stops, and missing AEP conversion is not a workflow acceptance blocker.
Native template setup, JSX/ScriptUI APIs, spatial Position/Opacity and needed
TextDocument behavior are the active target. Exact-source pooling is documented
in [EXPRESSION_POOLING.md](EXPRESSION_POOLING.md); fresh lightweight checks pass,
while heavy/native qualification remains deferred after execution interruptions.

## Pending X11 corrective qualification (2026-10-06)

The source-checked per-client keyboard correction and fail-closed fallback are
specified in [XKB_REPEAT_BOUNDARY.md](XKB_REPEAT_BOUNDARY.md). It follows, and does
not relabel, the mixed initial native evidence recorded below. Fresh correction
checks: 217 models on unchanged replay, two reply-mask tests, six fingerprint
tests and 59.047s canonical all-target check. One earlier model run hit two
unchanged expression wall-clock deadlines; no runtime limits were loosened.
New native qualification remains pending.

# AE ingestion and rich-text reconstruction — 2026-10-06

Production source checkpoint: `0be291bad7eb8535a38c0e879c625412a81ecc76`.

## Source boundary

The execution workspace changed after the earlier Slider/Mask qualification.
The later Git objects, toolchain, release binaries and external QA evidence were
not available through either supported cloud route. A verified Library bundle
restored the complete 324-commit history at `e7bb28d` into a separate checkout.
The stale checkout was left intact. This document describes newly authored
reconstruction; earlier native/pixel passes do not qualify these new bytes.

The restored checkpoint includes process-isolated JSX/ScriptUI, independent layer
origins/indexed labels, numeric expressions and evaluated 2D rendering. Rich
character styles and typed project ingestion are rebuilt here. Spatial Position,
endpoint/per-side Opacity timing, exact-source expression pooling, typed binary
Slider admission and mask expressions remain later reconstruction work.

## Usable bounded contract

File → Import AE project data accepts a strict, independently authored typed JSON
contract. Root compositions are listed with their blockers. Ready Apply prepares
a detached project, then uses the existing unsaved-changes Save/Discard/Cancel
workflow. The imported source is protected from native Save overwrites. Stale
reads/conversions, Cancel, malformed files and blocked roots preserve the open
project and history. Source expressions are not executed during reading,
conversion or text-shaping preflight.

Supported closures contain square-pixel 2D compositions, explicit point-text or
Null dimensions, solids, nested compositions, local parent transforms, independent
origins/trim/labels, complete Anchor/Position/Scale/Rotation/Opacity properties,
Linear/Hold keys without unrepresented side metadata, named Slider controls,
bounded numeric expression source, and ordinary comment/duration markers.

Every admitted time must lie exactly on its composition frame grid. Colors must
be exact normalized 8-bit channel values. No missing transform/base, fractional
frame, unknown effect, external media, marker URL/cue metadata, true 3D, Bezier
side/ease data or unknown required property is silently replaced by a default.
Native admission caps the document at 100 items, 1,000 total layers, 10,000 numeric
properties, 50,000 keys and 16 MiB, in addition to the reader's aggregate budgets.

RIFX inspection validates container framing, padding, offsets, depth and count.
It does **not** decode or admit binary AEP payloads. An `.aep` selection currently
reports that explicit blocker. Source names and fingerprints never reconstruct
missing objects or imply compatibility.

## Rich text

Schema 71 adds an optional sparse payload with resolved character styles and
canonical full-coverage UTF-8 byte ranges. The typed reader retains UTF-16 ranges;
the import adapter rejects boundaries inside surrogate pairs. Extended grapheme
and CRLF boundaries are validated after merging equal adjacent styles. Legacy
versions 1–65 without rich payload retain their prior serialization. Versions
66–70 belong to unrecovered wire contracts and reject explicitly before asset/model
decoding; no guessed migration or silent unknown-field loss is allowed.

Runs retain family and PostScript face identity, weight/slant, size, tracking,
fill/stroke values, and one common paint order. Source CR, CRLF and LF bytes are
preserved. Empty text retains an insertion style. Native range replacement
inherits the preceding style, or the following style at the beginning; this is
an explicit native editing contract, **not verified AE TextDocument replacement
behavior**. Local draft Undo/Redo includes styles and IME state.

The bounded renderer handles static point text. Active paragraph boxes, animated
Source Text/typography/paint, nonidentity text animators and incompatible paint
orders are rejected. One continuous compositor text chunk per hard paragraph
provides actual shaping, bidi, kerning and glyph/source positions. Run boundaries
crossing a shaped cluster and unavailable requested fonts fail visibly. The same
composed metadata drives pixels, text picking, carets and selection. Native import
preflights font/shaping constraints before it can replace an open document.

The interchange explicitly distinguishes native-normalized point coordinates
from AE baseline coordinates. AE baseline normalization remains blocked pending
independent evidence. Font coverage reporting marks rich-run analysis incomplete
rather than reporting the layer-wide face as a complete check.

## Fresh verification recorded so far

- Restored baseline: 168 GPUI-free model tests passed.
- Typed reader: 25 JSON and 9 RIFX tests; crate all-target check passed.
- Reconstructed integration: 213 GPUI-free model tests passed, including rich
  style/UTF-16/grapheme validation, exact native reopen, source/parent/timing
  conversion, no-expression-execution preflight, draft history and input routing.
- Renderer helper at `b5f2324`: production-locked, direct production helper code,
  9 groups and 6 independent 480×240 RGBA comparisons, 691,200 pixels with zero
  differences. Its original evidence is retained separately. A fresh schema-71 replay after the central
  grapheme-validation correction also passes the same 9 groups and 691,200 pixels.
- Canonical all-target check passes in 42.75 seconds; all 6 source-fingerprint
  tests, the native fixture example, rustfmt and diff checks pass. The first cold
  check compiled production but caught three test-only imports after Buffer
  extraction; those were restored before the final pass. Desktop test modules
  compile but have not been executed as a monolithic test binary.
- Fresh release at clean `510244c`: build `20261006.021934-e80c43944064bd30`, 557 inputs,
  one canonical cold release in 1,753.423 seconds with unchanged profile and verified
  process-local allocator settings. Actual About matches. Four full Renderer/CLI
  frames match unchanged literal SVG references exactly: 921,600 pixels. Geometry,
  text and styles are independently authored; the locked raster backend is shared.
- Actual native roots 101/202 import matches entire independent source oracles.
  Eleven negative files plus blocked root 303 reject explicitly and preserve the
  baseline/Redo branch. Dirty Cancel/Save/Discard and import-modal held-key guards
  pass. Direct rich input produces exactly `AB\r\nGo tv\nZ`, retaining styles/CRLF
  and all unrelated fields; draft/document history and reopen/resave pass exact
  file comparisons. Native color typing/cancel/history/reopen also pass, with
  disclosed valid 65→64 schema-floor normalization on the isolated non-rich root.
- **Native qualification is incomplete:** one ScriptUI held Return 1,200 ms accepted
  nested confirmation without fresh Yes and completed a name-only edit. A bounded
  repeat stayed pending; pointer/single Return paths passed. Exact failure and
  Undo are retained. A narrow per-client repeat-release correction is pending.
  Narrow-window layout, marked IME and other platforms remain unqualified.
- Missing native link, Vulkan ICD and portal runtime were repaired using verified
  official workspace-local payloads and process-local environments. The final
  portal uses a private lifetime-bound bus and only the GTK FileChooser backend.
  App closed normally at 03:05:31 UTC. Production source and system settings were
  unchanged; the app used an isolated QA profile. Detailed fresh evidence is in
  `../libreeffects-qa/rebuild-rich-native-20261006/QUALIFICATION.md`.

No original JSX/AEP was executed, imported or committed. Original-template visual
parity, AE baseline/style replacement semantics and remaining unsupported source
features are not claimed. The user-facing AE diagnostic bundle is still awaiting
user-supplied results.

## Synthetic inputs and QA

`crates/ae-project/tests/fixtures/synthetic-project.json` tests lossless typed
reading, including deliberately non-native-aligned timing. It is not a universal
native-ready fixture.

`synthetic-native-ready.ae.json` is a separate, independently authored native
acceptance fixture: root 101 has styled point text and a nested solid; root 202
is isolated; root 303 is deliberately blocked by 3D semantics. The example
`ae_rich_import_fixture` emits this JSON and supported native fixtures. Those
emitted LEPs are fixtures, not independent renderer oracles.

Fresh external evidence is under
`../libreeffects-qa/rebuild-rich-import-20261006/`. Each coherent committed source
checkpoint is separately saved as a verified complete-history Library bundle.
