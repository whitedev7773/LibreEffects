# Supplied AE project compatibility

The acceptance goal is analysing the original AEP in After Effects, converting
its editable compositions, media, text, animation, expressions and effects into
a native LEP, and matching operation and output against After Effects. Direct
AEP importing is outside the owner's requested scope. A flattened reference
movie or a successful expression test alone does not meet this goal.

## Observed reference

On 2026-10-07, the supplied archive's AEP was compared with the adjacent extracted
AEP using SHA-256; the bytes matched. The project was opened in After Effects 2026
and its final Black-White composition was played. The original file was not saved
or overwritten, and its original automation JSX was not executed.

The read-only scripting capture contains 50 original items, 14 compositions,
174 layers, 30,917 property nodes and 354 keys. There are 195 enabled expression
bindings using nine distinct exact source programs. No captured layer uses 3D.
The project also contains 23 fps player precompositions inside 60 fps compositions,
negative source origins and an audio out point between composition frames.

The captured effects use these match names: `ADBE Slider Control`, `ADBE Fill`,
`ADBE Drop Shadow`, `ADBE Gaussian Blur 2`, and `ADBE AudSpect`. Native effect
availability does not establish that every AE parameter or sampling rule matches.

Character capture succeeded for all 93 static TextDocuments. It retains exact
PostScript names, per-character styles and CR line breaks. Exposed lyric baseline
locations confirm a zero first baseline and subsequent baselines at 62 and 124
pixels for the observed uniform 46-pixel, fixed-leading text. Font shaping,
kerning, paint, paragraph behavior and raster fidelity still need comparison.

Private captures, original expressions and full-resolution AE reference PNGs are
kept under ignored `target/ae-reference/`. They are local diagnostic evidence and
are not redistributed as synthetic unit fixtures.

## Implemented and measured

| Area | Evidence and current boundary |
| --- | --- |
| Reference capture | Property trees, source/parent/matte IDs, keys/easing/tangents, text runs, exposed baseline locations and media metadata can be captured from AE. Missing values remain explicit. This is a diagnostic schema, not a native project export. |
| Native development LEP | The offline adapter converted all 14 compositions and 174 layers, retaining all 195 exact expression bindings, original IDs, folders, linked media and original embedded PNG bytes. Native binary save/reopen equality is verified. The LEP is explicitly marked as a development conversion in its separate receipt. |
| Exact-source evaluation | Production evaluator comparison covers seven lyric times and five playbar times: all 1,359 property results match within the explicit `1e-7` numeric tolerance or exact text bytes, including five Mask Path results. Snapshots come from the saved/reopened native LEP. The 2D transform comparison explicitly projects the scripting API's dormant Z axis to two active axes. This is sampled expression evidence, not complete render parity. |
| Evaluation performance | Immutable marker/Slider views and exact layer-name indexes are reused within one batch. Lookups still charge the read budget. The embedded C VM is optimized in debug/test builds without increasing CPU, wall, memory or read limits. |
| Expression RAM preview | A cached expression scene now reuses its shared RGBA buffer and reevaluates only the displayed root geometry. The same isolated evaluator, cancellation and current-frame receipts apply. Native Lyric and Playbar cases verify complete equality with freshly rendered views and unchanged authored bytes. This does not remove UI upload or audio playback costs. |
| Point-text coordinates | Typed interchange baseline origins now map to native schema74 point origin while preserving source text, rich styles and transform tracks. The source producer still must declare supported typography correctly. |
| Rich text rendering | A multi-span combining-character crash was repaired by merging glyphs through source cluster intervals rather than interpreting UTF-8 byte lengths as glyph counts. Pixel parity with AE remains a separate check. |
| Layer timing | Schema81 retains negative, fractional and beyond-composition authored trim endpoints. Integer timeline ranges are clipped projections; expression, JSX and audio clocks retain source endpoints. JSX assignments preserve the other exact endpoint and no-op assignments preserve source bytes. Moving, splitting, resizing composition duration and undo/reopen preserve this distinction. |
| Shape geometry and masks | Schema81 supports centered parametric geometry that stays centered when resized. AE point text and centered contents use source bounds including negative coordinates. Legacy text masks retain their fixed layer domain when text animators move ink. Native renders retain the lyric panel, rounded player and album-art matte. Feather up to 2048 is bounded and persists natively. |
| Font metrics | Independent shaping of the exact installed font shows that OpenType `palt` advances and offsets reproduce the observed AE baseline positions. Schema81 rich text retains proportional metrics across source/style edits; the SVG compositor supports proportional-width shaping. This is a measured font rule, not proof of every AE text feature. |
| Raster-image minification | Quality sampling averages premultiplied source coverage before the final bicubic/bilinear footprint. Synthetic rendered checkerboards and odd transparent edges verify coverage. Original embedded image bytes and pixel-art modes are preserved. |
| Effect unit calibration | Temporary AE rectangle probes measured every blur/shadow amount used by this project. Explicit resource mappings can convert source amounts to native Gaussian sigma; missing entries reject once a mapping is supplied. The fit is approximate: even ideal Gaussian edge predictions differ from AE by up to three alpha levels. The receipt retains the mapping, evidence and unresolved kernel requirements. |
| Polygon storage | Independent AE probes establish float32 input conversion followed by biased float32 signed 16.16 storage for zero-handle polygons. `createPath` reproduces the observed rounding and overflow. Authored path coordinates stay exact. Nonzero handles use a different AE pipeline and remain outside this qualification. |
| Audio Spectrum | Actual AE sine probes identify display option 1 as digital bars. The adapter retains display, side, composite and Gaussian boundary-repeat options. Opt-in schema82 HammingV1 uses a periodic Hamming window, uncorrected gain, an exclusive end frequency and denser bounded interpolation. Production coefficients predict 1,031/1,032 observed integer bar heights exactly, with one-pixel maximum error. Separate antiphase, one-channel and unequal-channel captures identify arithmetic-mean stereo before magnitude. NativeV1 remains unchanged; complete AE DSP and paint equivalence remain unverified. |

The mask probe separates JavaScript arithmetic from AE Shape storage: direct
`linear()` values retain double precision. The zero-handle Shape storage rule
removed the four original mismatches without changing source programs, captured
authored coordinates or comparison tolerance. Curved paths are not qualified.

## Remaining acceptance requirements

| Requirement | Remaining work |
| --- | --- |
| Offline LEP conversion | A native development file is generated and opens in the desktop editor. Qualify output and editing behavior before treating it as a faithful final conversion. The `reference_snapshot_project` example does not add AEP importing to the editor. |
| Complete source semantics | Native adapters cover the actual graph. The private capture also preserves metadata outside current native contracts. Qualify effects, interpretation, dormant stroke/marker metadata and text semantics rather than equating successful graph conversion with complete source fidelity. |
| Timing and keys | Native schema81 preserves signed and fractional layer-range endpoints and extended authored scalar Position values. Expression snapshots and continuous audio use the precise endpoints; the timeline retains an integer projection. Source keys still require frame-grid times. Mixed frame rates, per-side easing, spatial handles and dormant metadata require native and AE comparisons. |
| Media and fonts | Actual source files and exact installed faces are resolved with resource checksums and font evidence. Qualify audio clocks, broader frame sampling, alpha/color interpretation and text rasterization. |
| Native operation | The actual editor opened the development LEP, committed a layer-name edit, undid it and saved a separate LEP. Independent production decoding confirms complete Project equality with the conversion source. A subsequent conversion also opened and rendered at a user-interface-selected timeline position. Playback performance, broader edits and the original automation script remain to qualify. |
| Render comparison | Full-resolution AE and native frames at 5 seconds and 72+40/60 seconds were inspected. Native proportional Japanese spacing now agrees visually at both times. Shape origins and matte placement are corrected. Image minification, approximate shadow/blur calibration and Audio Spectrum require further comparisons across frames and transitions. |

There is no verified 100% compatibility result yet. Empty capture errors or passing
synthetic regressions must not be presented as completion of the original project.

## Validation checkpoint

On 2026-10-08, Rust 1.97.0 workspace tests including every target passed 2,793
tests in 33 suites, with zero failures and 55 conditional media/manual tests
ignored. Separate doc-test suites contain no cases. The installed exact-CJK-font
qualification was explicitly run and passed one otherwise ignored test. The
modified vendored `usvg` and `resvg` libraries passed 16 and 18 tests respectively.
`moon run desktop:check`, the desktop build, `cargo fmt --all --check`, and
`git diff --check` passed. These checks validate the implementation checkpoint;
they do not establish original-project parity.

The latest 12-case native comparison evaluated all 1,359 requested values without
execution failures or mismatches, using expected values measured in live AE.
Earlier comparisons correctly returned failure for four Mask Path mismatches.
An empty reference input is rejected rather than reported as passing. Temporary
probes clean up their own composition and solid items; the graph returns to 50
items, and the original AEP file hash remains unchanged.

The comparison uses actual expression snapshots of the saved/reopened native
LEP, rather than a separately reconstructed expression-only graph.
Reference capture v7 corrects MarkerValue classification
using the property's match name; 190 marker keys now retain their metadata. A
field comparison with v6 found no changes to shared non-marker source values.

Full-frame pixel measurements of the pre-Hamming development render at 5 seconds
and 72+40/60 seconds report RGB mean absolute differences of 2.174 and 2.082 on
the 0..255 scale. Maximum channel differences are 69 and 72. Exact RGBA pixels
are approximately 11.3% and 11.2%; visual similarity is not pixel identity.

The updated HammingV1 development LEP also rendered both 1920x960 frames with
strict font admission. Full-frame RGB mean absolute differences improve to
2.103 and 2.041. The bottom spectrum strip's RGB RMSE improves from 9.205 to
6.591 at 5 seconds and from 4.644 to 2.581 at 72+40/60 seconds. Maximum full-frame
channel differences remain 69 and 72. This is measured progress, not 100% parity.
All 14 compositions, 174 layers and 195 bindings were retained in the then-delivered
2,436,490-byte schema82 development LEP and its separate conversion receipt.

## Preview performance follow-up

On 2026-10-08, optimizing only the `resvg` and `tiny-skia` development/test
dependencies reduced fresh 1280x640 root preview renders from 36.77/41.05 seconds
to 7.65/11.45 seconds at frames 300/4360 on this machine. These are individual
observations after renderer initialization, not a general FPS guarantee. Editor
debug code, assertions, expression limits and release profiles are unchanged.

The new expression-cache path was separately qualified on composition 151
(Lyric) at the same frames: fresh renders took 0.861/0.828 seconds and cache-hit
geometry refreshes took 0.270/0.262 seconds. Composition 1044 (Playbar), at frames
115/1671, took 0.302/0.286 seconds fresh and 0.253/0.250 seconds cached. All four
expression views equal the corresponding complete freshly rendered view, and
pixel buffers are shared rather than copied. The final root has no root
expressions and already used immediate RAM playback; its cache benefit must not
be attributed to the newly added expression path.

Strict-font 1920x960 exports at frames 300/4360 contain exactly the same RGBA
pixels before and after rasterizer optimization: 1,843,200 pixels per frame,
zero changed pixels. PNG encoding bytes differ, so qualification compares decoded
pixels, not file hashes. This preserves the existing AE differences documented
above. The new binary's additional native-UI check could not proceed because the
Computer Use helper repeatedly returned `foreground window did not report a
process id`; previous editor-operation evidence remains the prior checkpoint.

The follow-up all-target workspace run passed 2,795 tests in 33 suites, with no
failures and 55 conditional tests ignored. The additional regressions verify
that a RAM hit shares its pixel buffer, skips exhausted raster work, still
rejects cancellation/out-of-range frames, and admits only bounded explicit
benchmark requests.
The desktop build, `moon run desktop:check`, `cargo fmt --all --check` and
`git diff --check` passed for this follow-up. Workspace doc-test suites contain
zero cases and completed without errors.

Independent temporary AE mask captures at Feather 0/20/40/80 were reproduced
through the public synthetic probe with complete RGBA equality and 50 original
items before/after. Feather 20's best ideal Gaussian edge fit is sigma 8.12,
with up to two alpha levels of remaining error; the current native half-amount
rule differs by up to 14 alpha levels at that edge. Feather 40/80 fits also retain
up to three alpha levels of error. Native mask units have not been changed based
on this approximation; an explicit compatible kernel still needs development.

## Native Opacity editing follow-up

The converted project contains 23 layers with independent per-side Opacity
timing, covering 116 keys. Properties now edits their raw base values, adds or
removes keys, seeks to adjacent keys, and edits each side's speed, influence and
Linear/Bezier/Hold mode. Changing one side preserves the opposite side and
dormant endpoint values. Unchanged round-trip input preserves the original
unkeyed overshoot. Authored values remain bounded to 0–100%; influence is 0.1–100%.
An explicit static-collapse command preserves expressions and retains all
original records in Undo. Generic Timeline/Graph edits remain unavailable for
this timing representation.

The Computer Use connection recovered for this checkpoint. In actual AE, the
root fade layer's next-key controls seek to frames 15655 and 15747 and show 0%
and 100%. The new native Properties controls seek to the same frames and values.
In the native editor, frame 15655 was edited to 25% and its outgoing influence
to 45%, saved separately, then collapsed to a static 25%. Undo and Redo restored
the keyed/static states; undoing all three edits restored the original metadata.
Separate edited, static and restored LEP files have complete metadata equality
to their expected projects and unchanged embedded-media hashes. JSON object
ordering is canonicalized for this comparison; no numeric tolerance is used.
Reopening the edited LEP in the actual editor retained 25%, 45% and the other
side's original values. Original AEP bytes remain unchanged. This qualifies these
editing operations, not full AE image or animation parity.

The final all-target workspace run passed 2,799 tests in 33 suites, with zero
failures and 55 conditional tests ignored. Four new regressions cover exact
round-trip no-ops, independent side edits and history, key navigation/add/remove
and Hold constraints, and explicit static collapse with native reopening and
exact Undo/Redo. The desktop build, `moon run desktop:check`, rustfmt and diff
checks passed. Workspace doc-test suites again contained zero cases and completed
without errors.

## Exact frame qualification follow-up

The native `--compare-reference` command renders explicitly selected saved LEP
frames through the production output renderer, at full composition size with
strict font preflight. It records encoded-input and decoded-RGBA SHA-256 hashes,
per-channel errors, mismatch counts, bounds and the first differing pixel. A
one-level difference in any channel fails, including RGB beneath transparent
alpha. No resampling, tolerance or background compositing is applied. All cases
are validated before rendering; inputs are bounded and the new JSON receipt is
published atomically without replacing an existing destination. A completed
mismatch still writes its full receipt and exits with failure. Audio and frames
outside the requested finite set are not qualified by this command.

Actual qualification of the complete development LEP returned failure and a
complete receipt for these four complex frames at 1920×960:

| Frame | RGB mean absolute error | Maximum channel error | Exact RGBA pixels |
| --- | ---: | ---: | ---: |
| 300 | 2.103164 | 69 | 11.279894% |
| 4360 | 2.041445 | 72 | 11.194065% |
| 8880 | 2.422296 | 85 | 8.617459% |
| 11465 | 2.381749 | 94 | 8.622179% |

Frames 0 and 15747 returned success with complete RGBA equality; both are fully
opaque black and do not qualify the complex effects. Native and AE editor views
were also observed at the same frame 11465, including the animated chorus title
and lyric text. Independent Pillow/NumPy verification reproduces the first two
frames' statistics and complete decoded pixel hashes, including exact equality
to the preceding native v8 renders. A repeated command targeting the existing
receipt rejects and retains its checksum. Both jobs verify unchanged authored
native bytes. Original AEP and delivered development LEP file hashes remain
unchanged. This confirms the comparison path while leaving full parity open.

Three new regressions cover all-channel statistics and transparent RGB,
explicit/bounded command options and implicit color conversion rejection, and
a complete native job with exact match, one-level alpha mismatch, UTF-8 BOM,
invalid later case and preserved input/receipt files. The all-target workspace
run passed 2,802 tests in 33 suites, with zero failures and 55 conditional cases
ignored. Desktop build, Moon check, rustfmt and diff checks passed. Workspace
doc-test suites contain zero cases and completed without errors.

The public AE frame probe now validates all requests before the first capture
and waits for the PNG end chunk, since file creation and even nonzero length can
precede a completed write. Actual AE captures at frames 0 and 15747 completed
through this path. Six live rejection cases cover empty requests, null/negative/
out-of-range times, duplicate destinations and an existing second destination.
Each rejected before creating the first requested PNG, retained the preview
resolution, and retained all 50 original project items.

The expanded mask probe captured 16 Feather amounts from 0 through 677 in a wide
edge geometry and seven amounts in the default square geometry; both retained
50 original project items. A fitted three-pass fractional box model with byte
quantization gives at most one alpha level of error on the measured straight
edge for amounts 3–677. At square corners, three horizontal passes followed by
three vertical passes are closer than interleaved passes, but still differ by
up to two alpha levels at 20/50 and one at 128/677. These are measured hypotheses,
not a production kernel. Amounts 1/2 follow a different small-kernel behavior.
An independently captured mask covering far beyond the entire 512×512 source
retains alpha 253–255 at Feather 677, leaving additional numerical behavior to
explain. These observations do not justify clipping the mask to source bounds
or changing native Feather units. At that checkpoint production mask filtering
remained unchanged; the subsequent opt-in implementation is described below.

### Fractional-box mask and video-color checkpoint

Schema83 retains an explicit FractionalBox3V1 profile on each converted nonzero
mask without changing its authored Feather, path, animation, ID or opacity.
Default masks omit the field and keep the existing Gaussian renderer. The core
profile command preserves unrelated native data, rejects invalid/locked edits,
and supports no-op history, Undo/Redo and reopen. The offline adapter requires
an explicit radius for every source amount once calibration is supplied and
rejects uncaptured Feather/Opacity/Expansion animation rather than dropping it.
The source values 20, 50 and 677 map to measured radii 7.871, 18.93 and 250.323.
Small Feather amounts 1/2 remain outside this calibration.

The production SVG filter uses bounded linear-time sliding sums with three
horizontal then three vertical passes and byte quantization per pass. Checked
allocation, transform and primitive errors remain explicit. Vendor regressions
cover independent direct convolution, premultiplied channels, literal impulse,
quarter turns and failures; native regression sampling checks actual AE alpha
levels and exact native reopen/default-render restoration.

A complete source-metadata comparison of development v7 and v8 found only the
schema increment and six explicit profiles. All other values and embedded PNG
chunks remained unchanged; the saved/reopened v8 still matches all 1,359 sampled
expression results. The updated editor opened v8 through its native chooser
and rendered frame 11465 with the same visible lyrics/player header as AE.
Captures of the two original Lyric Box precompositions show
alpha MAE falling from 8.2704 to 0.1950 and 4.7788 to 0.000764 respectively.
Remaining maximum alpha differences are 27 and 14, concentrated at shape edges;
square-mask kernel samples alone do not qualify original shape paint. The AE
PNG RGB encoding is premultiplied/matted while native output is straight; a raw
RGBA receipt records a mismatch but cannot qualify transparent color parity.
The alpha comparisons above do not depend on that encoding difference.

The first complete v8 frame comparison exposed larger overall RGB errors:
2.7140, 2.6635, 3.0314 and 2.9907 at frames 300, 4360, 8880 and 11465.
The isolated alpha improvement does not excuse that regression. The delivered
development file was preserved while investigating background color separately.

An original-footage AE probe retains all 50 original project items and confirms
8-bit, nonlinear blending with no working color space. Its full-resolution frame
comparison isolates FFmpeg's default YUV-to-RGB byte bias. Both accurate rounding
and full chroma interpolation are necessary: bicubic conversion with both flags
reduces mean RGB error from 1.1669 to 0.1559, with 1,474,209 of 2,073,600 exact
pixels. The flags individually do not achieve that result. Both persistent and
still-frame decoding now share that conversion. Chroma edges and broader frames
remain unqualified. A separate real-AE neutral solid/20-percent black overlay
probe still shows a compositor-rounding difference, leaving further paint work.

With the corrected decoder and v8 masks together, the four full-frame RGB MAEs
are 2.1266, 2.0810, 2.4462 and 2.4187. They improve over the first box-profile
render but remain slightly above the delivered v7 checkpoint (2.1032, 2.0414,
2.4223 and 2.3817). The delivered file therefore remained v7 at that checkpoint; v8 was a separate
development candidate. Both black endpoints still match all RGBA bytes. The
original AEP and delivered LEP hashes were independently checked unchanged.

The delivered v7 itself benefits from the decoder fix without rewriting its
source. The updated production renderer measures RGB MAEs of 1.7755, 1.7196,
2.0403 and 1.9860 at the same four frames, about 16 percent lower on average
than the previous checkpoint. Maximum channel errors are 68, 71, 84 and 94.
These frames still fail exact comparison. A subsequent independent 256-level
AE solid ramp at seven black-overlay opacities retains all 50 original items
and records deterministic compositing differences for further qualification;
neither a guessed brightness offset nor an unqualified blend rule was applied.

The final all-target workspace checkpoint passed 2,806 tests in 33 suites with
zero failures and 56 ignored conditional cases. All six video-decoder tests
were explicitly run with ignored cases enabled and passed, covering the neutral
oracle, CFR fractional rates, random seeks, loops, alpha, resize, cache changes,
cancellation and interleaved sources. Vendored usvg/resvg passed 17/23 tests.
Desktop build, Moon check, workspace/vendor rustfmt and diff checks passed;
separate workspace doc-test suites contain zero cases. These implementation
checks do not establish 100-percent original-project compatibility.

## Opaque layer-opacity qualification

Owned AE 2026 probes measured 256 grayscale destinations and 256 opacity steps
with black/white foregrounds, plus the three source channels 32/128/224. For an
opaque source and opaque destination, all 327,680 measured channel combinations
match `D + trunc(((S-D)*A+128)*257/65536)` with endpoint source replacement.
Another 28 fractional opacity percentages (including 49.999/50/50.001 and
99.6/99.9/99.99) match all 21,504 checked RGB values using rounded byte opacity.
Signed division truncates toward zero; ordinary symmetric rounding differs for
dark-over-light values. Seven black-opacity ramps were also rendered through
AE's Best Settings / TIFF Sequence with Alpha queue and matched the diagnostic
PNG bytes exactly. Probe compositions and separately created sources were
removed, retaining all 50 original items and zero queue entries. No AEP was saved.

Schema84 `CompositingProfile::OpaqueOpacityByte257V1` is explicitly opt-in for
each composition. `NativeV1` remains the default for existing LEPs. Only opaque
source/destination pixel pairs use this arithmetic; partial-alpha pairs keep the
original native implementation because their AE measurement still has one-level
differences. Its normal SVG group attribute survives both writers. Checked
rendering accounts for the opaque-override intersection buffer's live bytes;
unsupported blend combinations or budgets fail explicitly.
Opacity-only checked rendering retains ordinary-Gaussian crop pixels, with a
512 MiB live-buffer cap (32 megapixels /128 MiB per image); explicit box/repeat
trees retain their 256 MiB live cap and unclipped-support requirements.
Literal captured byte fixtures, native reopen, preview/output equality and the partial-alpha
contract are covered. The offline converter accepts `compositing_profile` and
copies `compositing_profile_evidence` into the checksum-qualified receipt.
Unknown profiles reject. Selecting it does not mark project parity as verified.

The current delivered development LEP is schema 84 v10 (2,437,148 bytes). It keeps
the v7 Gaussian masks and changes only the schema and the explicit opacity
profile in 14 compositions; all other project metadata and all 1,445,366 bytes
of non-PROJ chunks match v7 exactly. Reopened expression cases are byte-identical
to v7/v8 and the current production evaluator again matches all 1,359 results.
An older cached release comparison executable exceeded its 100 ms CPU allowance
on two cases; the current cached evaluator passed all 12 cases with no errors.
The fixture/source programs, tolerance and runtime budgets were not changed.

The v10 full-frame RGB MAEs are 1.6233, 1.5663, 1.7932 and 1.7032 at frames 300,
4360, 8880 and 11465, versus 1.7755, 1.7196, 2.0403 and 1.9860 for v7 on the same
corrected decoder: an 11.1-percent additional average reduction. Maximum channel
errors remain equal or improve in each scene. The box-mask/opacity v9 candidate
has a slightly smaller average across the four scenes, but raises early-scene
maximum errors; it remains experimental. Both black endpoints still match every
RGBA byte. The updated GUI reopened v10 and rendered the matching lyric/player
scene at 00:03:11:05 without a warning or a dirty authored project. The original
AEP hash is unchanged; the delivered v7 bytes remain in the private diagnostic
backup. Full original-project pixel, audio and editing parity is still unproven.

Final checks passed 2,810 all-target workspace tests in 33 suites, zero failures
and 56 ignored conditional cases. Both vendor suites passed (usvg 18/resvg 28).
Desktop build, Moon check, workspace/vendor formatting and diff checks passed.
All seven workspace doc-test suites contain zero cases. These checks qualify
implementation behavior; they do not establish 100-percent AE compatibility.

The synthetic transparent strip established that the diagnostic PNG can contain
premultiplied RGB: a white foreground at byte opacity51 gives RGBA[51,51,51,51].
The queue's inspected TIFF template is Premultiplied (Matted). Therefore raw
straight-RGBA equality is meaningful for the original project's opaque root
frames, while transparent-component RGB requires a separately qualified output
encoding. Mask alpha comparisons remain valid. Do not reinterpret these probes
as proof of straight-RGB parity for transparent nested compositions.

`scripts/ae-opacity-compositing-reference.jsx` accepts
`LIBRE_EFFECTS_OPACITY_REFERENCE` with `directory` (existing, empty), `foreground`
(three byte channels) and `opacity` (1..256 finite percentages in0..100).
It captures 1028×16 frames with 256 opaque grayscale tiles and a transparent
four-pixel strip, records a receipt, and restores the owned frame-helper global.
The source project must already be 8 bpc; it never changes project bit depth.

## Straight alpha output qualification

`scripts/ae-render-queue-reference.jsx` now captures full-resolution Straight
RGBA8 PNGs through the installed AE render queue. An explicit installed template
is required; the script checks its actual Format, Color, Channels, Depth, size,
crop, resize, region of interest, audio and post-render settings before starting
any output. It does not infer an encoding from a template's name. The installed
AE26 `_HIDDEN X-Factor 8` template was independently inspected and qualified as
PNG Sequence / Straight (Unmatted) / RGB + Alpha / Millions of Colors+. Its
presence is version-dependent and is not assumed by the reusable script.

The normal ExtendScript `setSettings` call rejected changing Color as read-only
in this installed AE version. That rejected probe preserved the original 50
items and empty queue. The native output dialog was also inspected directly.
The capture script uses a verified existing template without saving project or
preference changes.

An owned 28×80 scene covers seven grayscale destinations, five destination alpha
levels and nine foreground opacity bytes. Actual queue TIFFs in Premultiplied
(Matted) mode match all diagnostic RGBA pixels. Those TIFFs label their fourth
sample as unspecified (`ExtraSamples=0`); Pillow's RGBX decoder drops it. The
measurement therefore validates the uncompressed interleaved four-byte strips
directly rather than inventing opaque alpha with an RGB-to-RGBA conversion.

The matching Straight queue PNGs preserve alpha exactly. Associating their RGB
with nearest-byte `RGB * alpha / 255` reproduces all 20,160 diagnostic pixels in
this owned fixture. The reverse rounded-byte operation fails on 6,912 pixels by
up to one RGB byte. This is evidence that the diagnostic PNG cannot reconstruct
the original Straight bytes exactly; it does not establish a general compositor
formula. Independent queue captures are now used for transparent references.

All four original opaque root frames (300, 4360, 8880, 11465) were rendered again
through the verified Straight queue template. Their decoded RGBA hashes are
identical to the earlier diagnostic references, with zero differing pixels out
of 7,372,800. Thus the published v10 scene error measurements remain applicable
to these actual queue renders. This AE-to-AE equality is not Libre Effects
parity; the native comparisons still fail exact equality.

Original mask compositions541 and567 also have new Straight queue references.
Their alpha planes equal the earlier diagnostics exactly. Against those new
references, v10 Gaussian alpha MAE remains8.270371 /4.778785; the v8 Box3
experiment remains0.195021 /0.000764. RGB is exact for the black mask, while the
white mask still has edge differences (RGB MAE0.009971, maximum255). No hidden
RGB, alpha errors or edge pixels are suppressed. Box3 remains experimental and
these results do not change the delivered v10 project.

Two actual rejection probes cover an invalid late frame and an installed TIFF
template with unsuitable encoding. Both produced zero files, restored the empty
queue, and retained all50 original project items. Successful mask/root captures
also retained all50 items and the empty queue. Original AEP saving was never
invoked. Private evidence includes `straight-reference-output-qualification-v1.json`,
`partial-alpha-dialog-v1/comparison.json`, `partial-alpha-dialog-v1/roundtrips-v1.json`,
`straight-reference-rejections-v1.json`, and the exact native comparison reports.

## Reproduction tools

The owner subsequently changed the stopping criterion to a usable compatible
development LEP and executable, rather than complete AE equality. The delivered
project remains v10:14 compositions,174 layers and195 expression bindings.
The mixed small-mask v11 experiment renders successfully after the temporary
support correction, but its decoded pixels are identical to v10 at all four
qualified root frames. It is therefore not substituted for the delivered LEP.
Pixel, audio and editing parity remain unverified; visible text and blur
differences remain in the measured scenes. Development stops after validating
the current executable and reopening the delivered LEP in the native editor.

Run the capture scripts through an installed AE scripting entry point, with an
explicit new output path. Adobe documents CLI script execution using
[`AfterFX.exe -r`](https://helpx.adobe.com/after-effects/desktop/automate-in-after-effects/automate-animation/scripts.html).
The scripts never save the original project. Precision probes create temporary
synthetic items and remove their composition and separately created solid source.

- `scripts/ae-reference-snapshot.jsx`: `LIBRE_EFFECTS_REFERENCE_OUTPUT` path.
- `scripts/ae-expression-reference.jsx`: `LIBRE_EFFECTS_EXPRESSION_REFERENCE`
  with explicit output, composition/time cases and any explicit lexical bindings.
- `scripts/ae-frame-reference.jsx`: `LIBRE_EFFECTS_FRAME_REFERENCE` array of
  composition/time/new PNG paths. The diagnostic PNG method is capability-checked
  and missing output is an error; check each produced file independently.
- `scripts/ae-render-queue-reference.jsx`: `LIBRE_EFFECTS_RENDER_QUEUE_REFERENCE`
  with `directory` (existing, empty), `template` (explicit installed output
  template), and `cases` (1..64 `{compositionId, frame}` objects). The project
  must already be8bpc and its queue empty/stopped. Actual full-resolution Straight
  RGBA8 output settings are checked before rendering. `receipt.json` records
  actual PNG paths, settings, dimensions and original item/queue counts. Output
  files still require independent static RGBA8 decoding. Only owned temporary
  queue items are removed; authored compositions and original AEP are not saved.
- `scripts/ae-linear-precision-reference.jsx`: `LIBRE_EFFECTS_LINEAR_REFERENCE`
  with output and sample times. This isolates arithmetic from Shape storage.
- `scripts/ae-path-storage-reference.jsx`: `LIBRE_EFFECTS_PATH_STORAGE_REFERENCE`
  with a new output and 1..128 finite numeric values. This independently samples
  zero-handle polygon coordinate storage without executing a supplied program.
- `scripts/ae-mask-feather-reference.jsx`: `LIBRE_EFFECTS_MASK_FEATHER_REFERENCE`
  with a new JSON output in an existing directory and 1..16 amounts in 0..2048.
  Geometry is `square` (default, 512×512) or `wide-edge` (8192×64, sampling the
  middle of an 8192-pixel-tall source). Captures indexed sibling PNGs and records
  geometry, dimensions, bit depth and item counts. It never changes the original
  project's masks. The wide probe isolates the original project's Feather 677
  as well as the smaller amounts. Measurement models still require qualification.

```text
cargo run -p libre-effects-ae-project --example reference_inventory -- SNAPSHOT.json [ROOT_ID]
cargo run --release -p libre-effects-ae-expressions --example compare_reference -- CASES.json
cargo run -p libre-effects-editor-model --example reference_snapshot_project -- SNAPSHOT.json RESOURCES.json ROOT_ID NEW.lep --all
cargo run -p libre-effects-editor-model --example reference_native_cases -- PROJECT.lep AE_CASES.json NEW_CASES.json
libre-effects --compare-reference PROJECT.lep --cases FRAME_CASES.json --output NEW_REPORT.json
```

The first command reports dependencies, mixed rates, features and capture gaps,
without loading media or executing captured programs. The second runs captured
programs inside the bounded production expression evaluator and reports every
mismatch. It returns failure when any compared property does not match.

The converter requires explicit resource SHA-256 values, probed audio metadata
and any source-program lexical bindings in `RESOURCES.json`. It verifies source
bytes and fonts, validates native data, encodes and reopens the LEP, and writes a
separate conversion receipt with unresolved requirements. Outputs must be new
files. `--all` includes all captured compositions; omission includes only the
selected root's dependency closure. The native-cases command pairs real saved
LEP snapshots with the captured AE results for the production evaluator.

Optional `effect_sigma` resource maps use source parameter match names and exact
authored numeric amounts as keys. Values are explicit nonnegative native sigma
measurements; zero must map to zero. The accompanying `effect_sigma_evidence`
is copied into the receipt with the resource checksum. This development mapping
does not change the native effect's existing sigma contract or imply AE parity.

The optional `audio_spectrum_profile` resource field accepts NativeV1 (default)
or HammingV1. Unknown values reject. HammingV1 requires schema82; the resource
checksum and `audio_spectrum_profile_evidence` are retained in the conversion
receipt. Selecting it does not mark pixel or audio parity as verified.

Optional `mask_feather_box3_radii` resources map exact source Feather amounts to
explicit measured radii. Zero must map to zero; nonzero calibration requires
Feather >=3 and a finite radius >=0.5 within the native coefficient bounds.
Missing amounts reject. The coefficient retains source Feather units when
editing; interpolation outside the measured amounts is a mathematical profile,
not further AE qualification. `mask_feather_box3_evidence` and the resources
checksum are retained in the receipt. The editor does not import AEP files.

Alternatively, `mask_feather_profiles` maps each exact source Feather amount to
`"GaussianV1"` or `{"box3_radius": MEASURED_RADIUS}`. It cannot be combined with
`mask_feather_box3_radii`; missing amounts, unknown profiles and extra profile
keys reject. This lets the converter retain Gaussian for one source mask while
applying an explicit calibrated Box3 kernel to another. The receipt retains
`mask_feather_profiles_evidence`. Source values, paths, keyframes and expression
programs stay unchanged. Choosing a profile does not establish AE parity.

Checked mixed Box3/Gaussian rendering retains the complete declared filter
support instead of cropping to the upstream five-viewport allocation shortcut.
Ordinary Gaussian input copies and kernel scratch share the checked live-buffer
budget. Allocation errors identify temporary group bounds when available.
Opacity-only checked rendering preserves its historical ordinary-SVG bounds.
The source project's measured mixed mask needs a 6875×5077 temporary Gaussian
buffer, above the former 32-megapixel temporary limit. Mixed Box3/opacity trees
without repeat-edge domains allow 64 megapixels /256 MiB per temporary image and
1 GiB of live checked buffers. Other profiles retain their existing limits;
final frames remain limited to32 megapixels. This is a bounded support policy,
not a claim that every upstream SVG allocation is fallible or accounted for.

AE's diagnostic PNG method can return before its worker commits the file. The
frame capture waits for the PNG end chunk with a bounded 30-second deadline;
each resulting PNG still needs independent decoding and pixel inspection.
