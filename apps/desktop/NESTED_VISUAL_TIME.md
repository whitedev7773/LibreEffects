# Continuous nested visual time (schema 77)

This bounded implementation keeps authored keys on integer composition frames,
while nested compositions may be rendered at continuous source time. The optional
composition field `preserve_nested_frame_rate` is absent in older documents.
Absent retains the legacy preceding-frame behavior. Explicit `true` retains the
source frame grid; explicit `false` preserves the containing sample time through
nested frame-rate changes. The authored FPS, duration, keys, expressions and audio
switches do not change. Setting this metadata requires project schema 77.

## Clock and detached views

Frame-derived samples keep checked reduced rational seconds. Remapped arbitrary
seconds retain their finite f64 identity. Source offsets use the nested instance's
source start frame, independently of trim and layer startTime. Explicit quantization
has no epsilon. Negative/out-of-duration samples are transparent; overflow rejects.
A 60 → 23 → 60 chain with both sources opted in therefore retains the original
sample instead of flooring the intermediate 23 fps coordinate.

The renderer retains one immutable authored project per output frame. Every sampled
composition view is derived from that source, and caches include exact sample time
and guide mode. Hidden matte providers and transform parents share that composition
sample. Recursion restores the containing time before rendering siblings, including
on errors. The root preview receipt uses the exact root clock. Sampled views cannot
be serialized, saved or committed as editor source. At most 128 sampled views and
128 expression evaluations are admitted per rendered frame.

The fractional adapter freezes scalar transforms, joined planar Position, per-side
Opacity, Slider Control and discrete authored Source Text before applying validated
numeric, string or path expression results. Expression `time` and pre-expression
`value` use that same sample. Existing expression CPU, wall, subprocess, memory,
read and result-size limits remain unchanged. Whole-frame rendering keeps its
existing path and old native files keep their original sampling contract.

Fractional sampling rejects spatial/camera compositions, video/image sequences,
animated typography/text selectors, authored mask animation, animated non-Slider
effects, animated shape geometry/paint and Contents gradients. Unsupported varying
families must not silently use floor-frame values. Static image, shape, Fill, matte
and rich point-text content remain usable. Audio already follows continuous seconds;
this visual change does not mute, merge or normalize duplicate audio routes.

## Actual reconstruction boundary

The generic `reference_player_project` example accepts bounded, hash-verified
reviewed input outside Git and appends Art plus two players to a supplied native
project. The private derivative contains six compositions and 109 layers. It
preserves both players' 23 fps metadata, five supplied embedded images, rounded
matte providers, two black Fill effects and two enabled synchronized audio paths
per standalone player. All six explicit nested-frame flags are false according to
the source field evidence. The existing 94 layers are otherwise preserved.

Absent source properties require documented native materialization. The supplied
album image is a role-based replacement, not proven identical to the old footage.
Full render themes, Spectrum, missing video, unresolved dormant mask-space details
and AE pixel parity remain outside this partial deliverable.

## Current validation (2026-10-06)

- Six clock/adapter cases and five integration tests pass. Integration covers exact
  history/schema admission, fractional authored and expression values, unsavable
  views, unsupported-family rejection, remap and freeze behavior.
- Canonical workspace all-target check passes in 1m25s; workspace fmt/diff checks
  pass. No full model-suite replay or relaxed expression budget was used.
- The current-source, GPUI-free production renderer probe passes eight actual
  samples: Black 1671/6044, White 1671/1, Art 4359, Lyric 4360, title 4359 and
  Playbar 4359. Preview/export pixels match and authored source is unchanged in
  every case. Both assembled player images were inspected: album artwork, rounded
  matte, title/artist, control/menu icons, time labels and moving dot are present.
- Five independently authored synthetic nested scenes match flat static references
  and literal full-image pixels (111,616 pixels): continuous 60→23→60, explicit
  preserved grid, absent legacy policy, distinct same-floor remaps, and a hidden
  animated parent/matte. Nine unique fixture/reference renders pass.
- Prior actual Lyric/title/Playbar samples match 1,945,920 pixels. The first raw
  Lyric comparison rejected because the old CLI image was opaque black whereas
  the probe retains alpha. Explicit black compositing reproduces the prior Lyric
  and title bytes; Playbar matches raw RGBA. This mode distinction is retained in
  the receipt; no source correction or expected-pixel adjustment was needed.

Evidence is outside Git at `../reference-player-private-20261006`: the constructor
`independent-audit/readback-receipt.json`, `integration-checks/all-target-check.log`,
`render-probe/actual-render-receipts.json`, `render-probe/prior-composition-regressions.json`,
and `nested-oracles/{run-plan,pixel-oracle-results,render-receipts}.json`.
The compiler receipt pins current workspace artifacts; the probe imports production
Renderer/worker code and excludes external video decoding/media relocation only.
Static references share the production rasterizer, while literal solid pixel
expectations are independently written. There is no AE visual oracle.

## Final release/native qualification

Qualified source `bb70fad53f028f7fb3df668dd6a6a703ca360229`, build
`20261006.200726-974d4822eef51633`, 766 verified inputs. One canonical release
passed in839.957s. Pinned binary SHA-256:
`175ecacfae5c9621d159d78385d555cc778b6b9448e0f0d03b1b0c3c54d75e39`.
Native About matched. The final serial CLI batch passed all17 invocations once,
with empty stderr and no input mutation. Five synthetic cases match111,616 literal
pixels; eight actual samples match the frozen helper references, with explicit
opaque/alpha alignment. The prior three composition PNGs match byte-for-byte.
Total comparison scope is3,900,736 pixels; no independent AE image is available.

The native app opens the portable six-composition109-layer project and renders
both players at1671, Black6044, Art4359 and the three preserved compositions.
Artwork, rounded matte, title/artist, icons and timecodes are visible. Project
Media reports one online Audio.mp3, zero missing and three authored layer references.
Both nested audio routes per player remain enabled; audibility was not tested.

Existing Dot Position fx Cancel preserves source. One synthetic comp4/layer95
rename traverses the supervised worker and changes only its name. Five shared
PNG chunks/aliases, all six sampling flags and every other source field remain
exact. Undo, Redo, final Undo and Save→Open→Save match complete expected native
bytes, including VIEW. The final restored file SHA-256 is
`c7012c035e1d9f915245fe064da773fd882e33d197bc7a82eb228ff2e943a21e`.
Initial constructor object-order normalization/VIEW defaults are separately
accounted for; no scalar spelling or image-byte relaxation was used. The app
closed normally at20:23UTC, exit0, without an unexpected expression/render error.
This bounded batch does not prove all-frame/playback or auditory equivalence.

Detailed release resources, input identities, native observations and complete
source comparisons are at
`../libreeffects-qa/reference-players-release-20261006/QUALIFICATION.md` relative
to the repository root. Source tests and earlier debug failures retain their own
scope; no full-suite replay, changed runtime budget or AE pixel-parity claim.

## Completed bounded release/native protocol

The clean integration production commit is `6eaaaa1`. Perform one normal serial
release after the source backup, then qualify the exact final binary. Preserve
all earlier packages and qualification evidence. Use a fresh extraction of both
portable ZIPs into their shared `Lyric-Title-Playbar-Players-partial` directory,
with process-local font configuration pointing at its `fonts` directory.

Final CLI scope: the five synthetic pairs above, Black/White 1671, Black 6044,
White 1 and Art 4359, plus previous actual Lyric 4360/title 4359/Playbar 4359 regression
samples. Use RGBA for synthetic cases and align the old opaque CLI mode explicitly
for Lyric/title; no repeated exploratory rendering is required.

Native scope: open the copied actual six-composition project, verify About/source,
Black and White at 1671 and the late Black 6044 dot/timecode boundary, then navigate
through Art and the three preserved compositions. The Project Media state should
retain one linked audio file; both nested audio paths per player remain enabled.
Do not claim auditory equivalence from file linkage. Save, one harmless rename,
Undo/Redo/final Undo and Save→Open→Save must preserve complete PROJ/image payloads
and alias identities, with intentional active-composition storage swap and VIEW
state reported separately. Native repeated-save bytes should be identical after
the established constructor object-order normalization. A short fx Cancel verifies
computed source is preserved; new editing functionality is not this milestone.

The private input programs/media remain outside Git. The partial derivative is
not the full four-theme reconstruction, and no AEP opening, full playback reliability,
Spectrum or AE pixel parity is claimed.
