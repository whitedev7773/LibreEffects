# Typed Playbar expressions and actual six-layer increment

This milestone supports the three programs used by the actual reference Playbar:
clamped linear Position, createPath mask geometry, and a formatted Source Text
string reading another text layer. It extends the same supervised evaluated view
used by preview/export; it does not recognize source fingerprints or bake sampled
text/path animation into authored data. General AEP opening remains out of scope.

## Stored and runtime contract

- Schema76 adds SourceText and MaskPath(stable ID) targets to the existing sparse
  expression list, plus optional local_bindings metadata. Existing numeric JSON
  is byte-compatible when new fields are absent; older schema numbers reject new
  fields/targets rather than silently discard them.
- SetExpressionLocalBindings validates at most64 distinct ASCII identifiers,
  max64bytes each. Reserved words, eval/arguments, host/global names and injected
  punctuation reject. Source editing preserves existing bindings. The wrapper
  declares fresh lexical locals and direct-evals unchanged source under strict
  semantics. Cache identity includes exact source ID and bindings. No shared
  implicit globals, proxy scope or regex source rewrite is used.
- Source Text reads are primitive strings with lazy typed dependencies, including
  cycle and caught-host-failure guards. Output is bounded to16KiB UTF-8/no NUL.
  Enabled output initially requires uniform character styling; mixed-run AE
  TextDocument replacement is not inferred. The transient view preserves the
  uniform face/style and point origin, and cannot be serialized or committed.
- createPath returns a branded immutable value with layer-local points and relative
  incoming/outgoing handles. Empty handle arrays expand to zero handles. Paths
  require2 open/3 closed points, max1024, finite coordinates within±1e6; coincident
  vertices are permitted. Native MaskPath targets require closed paths and an
  existing stable mask ID. Arbitrary forged objects are rejected.
- linear supports finite scalars or same-dimension2/3-vectors, with finite strictly
  increasing time endpoints. Values clamp outside the range. Other AE overloads
  or degenerate/reversed ranges are not claimed.
- IPC independently validates text/path type and bounds; core validates ownership,
  source equality and dependency closure before applying a transient result.
  Existing memory, CPU, wall, read, dependency and process limits remain intact.
- Source Text and mask rows expose the existing expression editor. Explicit local
  names are displayed; source edits preserve them. Local-binding metadata is set
  through the core command/constructor. Text-tool and Pen vertex editing of an
  enabled computed target are blocked until disabled, avoiding geometry guesses.

## Actual private project construction

The generic reviewed-data append example now accepts ShapeContents, path masks,
markers and exact expression metadata in addition to its previous text/audio
contract. It verifies the input SHA, exact original project/resources, every
existing composition and view, new source mappings and a deterministic roundtrip.
It neither parses AEP files nor executes expressions. No originals, text, fonts,
media or private source programs are included in the repository.

The private output has three compositions/94 layers. New Playbar is600×24,
60fps/360s, with three shape layers, two point-text timecodes and shared audio.
Its three original programs plus the preserved192 bindings account for all195
bindings/nine unique sources in the current expression closure. This does not
mean all14 source compositions or174 layers have been recreated.

The two timecodes use exact supplied Paperlogy-2ExtraLight at13px, white,50%
layer opacity; Current Time is left-aligned and End Time right-aligned. Shape
center-based coordinates are converted into native top-left parametric geometry
by explicit half-size translation, preserving source group positions/anchors.
Current Time clamps at262s, Dot/mask markers end at262.8s, and decoded audio ends
at12614447/48000s. Those boundaries are intentionally distinct.

Remaining explicit adaptations: absent bar layer Position is materialized at
composition center; authored source-less mask coordinate units are unresolved,
so a manually reviewed t0 path is stored as its dormant native base while the
unchanged enabled expression computes the visible path. Disabling it is not
claimed equivalent to the AEP. The source audio raw295.72s out-point is retained
in provenance; native trim clamps to15769frames and exact source duration remains.
Original Lyric adaptations and AE shaping/interpolation/pixel unknowns remain.

## Gates and qualification

- Crate-only runtime checkpoint4ad49b6: all15 new tests pass; single aggregate
  debug run61/62. The inherited192-binding test exceeded unchanged100ms CPU at
  106.892ms (wall113.149ms). No retry or limit change. Crate fmt/diff pass.
- Core/editor-model library typecheck passes. Three focused public-API tests pass:
  shared typed results, exact uniform style/source/Undo/Redo, schema and malformed
  local rejection, stable mask ownership and mixed-style atomic rejection.
- Generic constructor builds and appends all six layers; original88 layers/assets/
  programs/views remain exact and native roundtrip is deterministic.
- Canonical workspace all-target check passes in1m24s, jobs1/incremental off,
  locked/offline. Raw logs are outside the repository under
  `../reference-playbar-private-20261006/all-target.{stdout,stderr}`.
- Independent source audit passes379/379: old88-layer/source/view preservation,
  exact raw AEP property/program evidence, geometry/style/marker/asset mapping and
  disclosed adaptations. Receipt is in the private increment's independent-audit.
- Six actual frames0/4359/15719/15720/15768/21599 pass the GPUI-free production
  renderer with the real supervised expression process. Preview/export pixels and
  source preservation match. Independent arithmetic checks every resulting text,
  dot coordinate and mask vertex/relative handle at the three distinct endpoints.
- Static reference LEPs authored from that arithmetic, with all three Playbar
  programs removed, match86,400 RGBA pixels exactly across the six pairs. This
  shares the production raster backend; it is not an AE pixel oracle. An initial
  reference-writer header-CRC mistake was rejected before rendering and retained;
  corrected v2 references include the proper header CRC. Product source unchanged.
- Exact supplied-font availability is checked before the helper renders. The
  observed frame4359 shows01:12, the dot/reveal and04:22; no visible clipping was
  observed. Actual renderer evidence is in the private increment's render-probe
  directory, with source/dependency build receipts. Runtime crate and focused
  model test counts are terminal-attributed, not redirected raw log claims.
- Final release/native qualification below passes within its bounded scope. No
  full model replay is claimed, and the known61/62 debug result is retained.

The next actual-project increment is assembled players. Current native nesting
floors to a child frame; the source disables preserveNestedFrameRate, so retaining
23fps metadata alone is insufficient for correct60fps parent sampling. That
continuous clock change belongs to the following coherent increment.

## Final release and native result

Qualified source `7af802778b900ea9f91812929229c34153419799`, build
`20261006.182714-887e11e34c600272`, 762 verified inputs. One canonical release
passes in807.597s. Pinned binary SHA-256:
`ae51315b7d4295f1c40490ae95b022509bf46ef69fe9a4f9223b7e10cc3ed561`.
Native About matches. All14 final CLI invocations pass once: the six static pairs
match86,400 pixels and exact PNG bytes, while prior Lyric4360/title4359 match
1,931,520 pixels and exact PNG bytes. No renderer stderr or changed protected input.

The isolated native app renders all three actual compositions and the six Playbar
boundary states. The Source Text editor shows all ten explicit locals. A `42`
draft rejects as non-string and Cancel retains exact authored source. Disabling
only its enabled flag reveals authored00:00; one Undo restores computed04:22 and
all original source bytes. The unchanged Mask Path editor opens and cancels.
Text/Pen entry is refused by the outer expression-scene guard, with no computed
mask handles; deeper per-target guard execution is not claimed.

The initial desktop Save reorders constructor JSON objects and materializes VIEW
without changing raw scalar tokens. A retained strict check rejects only the
intentional timeline-collapse VIEW change; exact expected PROJ and all other
chunks match. The separate VIEW-aware report passes. Mask Cancel and final
Save→Open→Save match complete native bytes including VIEW. No unexpected runtime
error appeared in this batch; the app closed normally at18:53UTC, exit0.
This is bounded native evidence, not an all-frame reliability or AE parity claim.

Detailed identity, resources, source comparisons and limitations:
`../libreeffects-qa/reference-playbar-release-20261006/QUALIFICATION.md` relative
to the repository root. All original packages and prior evidence remain unchanged.

## Completed bounded native protocol

Freeze the qualified source/package before one canonical release. Re-run the six
actual/static CLI pairs on that exact binary, plus the previously qualified
Lyric4360/title4359 regressions. Native open the fresh portable extraction, switch
to Playbar4359, inspect the current/end text and dot/reveal, then the262s and262.8s
boundaries. Open/Cancel the Source Text and mask expression editors; the original
source and explicit locals remain visible and unchanged. One target disable/Undo
can verify authored fallback and single-step history, followed by Save/Open/Save
with complete source preservation. A wrong-type text return should reject without
source/history mutation. Preserve all previous package/evidence bytes. No new
player, AEP import, Spectrum or broad UI regression scope in this batch.
