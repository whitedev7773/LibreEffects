# Gaussian Repeat Edge Pixels (schema 78)

This native mode clamps samples to an explicit finite input domain. It is a
static policy on an sRGB Gaussian Blur, with Transparent as the omitted legacy
default. Repeat requires schema 78; effect presets containing it require version 5.
Reset restores Transparent. Duplicate, copy, presets and Undo/Redo preserve it.
Unchanged Off actions preserve both source serialization and the pending Redo.

The renderer keeps three regions distinct: nominal input D, declared finite output
O, and working halo H. Pointwise effects preserve D; spreading effects pass their
finite declared output to the next stage. Hidden matte, blend and adjustment
rasterizations share a bounded frame-local domain registry. Adjustment inputs use
the complete lower composition and its inverse mapping into adjustment space;
the adjustment rectangle/matte/opacity remain coverage controls only.

Default-off rendering uses the existing backend path and SVG. Repeat uses the
checked pinned resvg 0.45.1 path. Its box branch retains all five widths, directional
order and rounding. Work storage extends O by the sum of all pass radii, samples
clamped original D, then crops after all passes. Re-clamping intermediate pass
edges is deliberately not the contract. Resolved sigma below 2 on both axes uses a
normalized finite Gaussian approximation with at most 17 taps per axis, instead of
the legacy recursive approximation. This opt-in difference is explicit; sigma
units, scaling and the small-value cutoff are retained. It is not AE calibration.

Axis-aligned raster domains, including supported reflection/quarter turns, are
admitted. Arbitrary rotated/sheared domains, invalid bounds, clipped support and
allocation limits report errors; no fallback disables Repeat. Migrated linear-color
or legacy blur combinations are rejected in this first native slice. New Gaussian
effects expose a guarded Repeat Edge Pixels toggle. It is not animatable.

Native limits cap each checked image at 32 megapixels / 128 MiB and checked live pixel
storage at 256 MiB. There are at most 4096 domain records per frame. New pixel, halo
and scratch allocations use fallible reservation. Existing asset decoders and
non-repeat primitives retain their upstream allocation behavior; this is not a
claim that every rendering allocation has become fallible.

The original reference has Repeat enabled by stored default on one White-theme
adjustment blur and explicitly disabled on seven others. This native feature does
not establish AE Blurriness-to-sigma, shadow softness/direction, popup defaults,
source-less mask coordinates, Audio Spectrum behavior or the missing movie's
pixels. The already-delivered six-composition player project is unchanged.

## Source and renderer validation (2026-10-06)

- Three model cases pass for schema/preset admission, exact no-op/Redo, reset,
  duplicate/preset retention, wrong-owner/color-space and locked rollback.
- Sixteen checked resvg cases and three usvg parser/writer cases pass, along with
  the no-default-feature backend check. Literal box and FIR kernels, cropped
  output, embedded context, domain transforms, clipping and allocation guards are
  exercised. Vendor files retain both licenses and exact official-source provenance.
- The canonical workspace all-target check passes in 1m30s. No full model-suite
  replay, monolithic desktop test binary, relaxed expression budget or release was
  used for this source checkpoint.
- The GPUI-free production renderer passes 15 predetermined invocations: 13 valid
  renders and two explicit UnsupportedTransform rejections. Valid renders match
  preview/export and preserve authored input. Independent literal checks cover
  constant opaque and pre-filter partial alpha, omitted/explicit Off, zero radius,
  bypass, full-lower-composition adjustment sampling with matte coverage, ordered
  shadow followed by Repeat, translation and half-scale domains. The large native
  sigma 70 diagnostic at 1920×960 also matches its constant-color expectation.
  It took 23.189s for both preview and export in the unoptimized helper; this is
  a bounded stress result, not a release-performance or AE calibration claim.
- The existing actual Black player 1671 and Lyric 4360 retain all 1,931,520 pixels
  with Repeat absent (explicit opaque-black mode alignment for the older Lyric
  CLI image). Their source is unchanged. The final binary must requalify these.

The initial probe command selected standalone resvg as a root workspace package,
which Cargo rejected before compilation. The corrected recorded helper build uses
its pinned vendor manifest plus current core/model artifact receipts. No production
code or expected pixels changed for that tooling error.

Synthetic source projects, generator, run plan and output-only checker are in
[fixtures/gaussian-repeat78](fixtures/gaussian-repeat78/README.md). They contain no
original project payload. Private checks are under
`../repeat-edge-private-20261006/{checks,render-probe,fixtures}`. The literal checker
was authored independently of output. Raster references still use the production
rasterizer; this is not an AE visual oracle.

## Preserved initial release/native result and keyboard defect

Tested source `36da3d36adfd764d36a88db8f3f4c4d8b1b20ea7`, build
`20261006.213300-57ac00290b38bd9e`, 800 verified inputs. One canonical release
passed in737.669s. The76,245,528-byte binary SHA-256 is
`71a09a84eb35beb02c44f08bb9cdcad40423f30e9f031fac84b041e607319b8f`.
Native About matched. All17 final CLI attempts meet expected outcomes:13 valid
synthetics match1,933,824 helper pixels and unchanged literal assertions; two
unsupported transforms reject without output; both actual regressions retain
1,931,520 pixels and exact prior PNGs. Native sigma70 completed in1.183s once.
The literal adapter preserves raw CLI receipts and makes no fresh final-binary
preview/export claim. Raster references remain shared-backend evidence.

The bounded native batch found a real key ownership defect: after a rejected
Repeat click during playback focuses the button, Space-down stops transport and
the synthetic Space-up click toggles Repeat. The saved artifact proves precisely
that mode mutation. This is not a held-key-repeat failure or model playback-guard
failure. The correction requires matching ownership of a fresh eligible key-down.
Initial native acceptance remains failed; the separate correction is qualified in
[REPEAT_EDGE_KEY_OWNERSHIP.md](REPEAT_EDGE_KEY_OWNERSHIP.md).

Unaffected mouse toggle/Undo/Redo, pending Radius3→mode ordering, locked rejection,
version5 preset, Duplicate/Undo, zero-radius/bypass views, adjustment source and
ordered-stage inspection pass. Explicit rotated-domain error recovers after Off.
Four first Saves match only predeclared typed normalization/object order/VIEW,
with strict-original failures retained. Later source/history comparisons are
byte-exact to their explicit expectations; final constant reopen/resave is entirely
identical, including VIEW. Native observations are operator/CUA-attributed. The
app closed normally at22:04UTC, exit0; frozen initial evidence remains unchanged.

Optional native Reset did not run after two automatic-review rejections about
modal/coordinate identification. It remains unqualified natively; model Reset
coverage is separate. No alternate route or further Reset attempt was used.
The checkbox cannot issue an unchanged-Off command, so that no-op/Redo contract
is model-only. Strong stale-receipt protection is not inferred from ordinary
commit-before-navigation behavior.

Raw vendor test logs were not retained. Counts are terminal-attributed:16 resvg
cases at7b8bc2f,3 usvg cases at9ce4330, and a preceding no-default-feature check at
5c7afd1. Later checked text-failure propagation/tests mean the latter is not a
final-source no-default-feature claim. The final integrated default-feature
all-target log remains preserved.

Full scope, source/VIEW distinctions and the failed native acceptance criterion:
`../libreeffects-qa/repeat-edge-release-20261006/QUALIFICATION.md` relative to the
repository root. No AE effect calibration, full theme or all-frame guarantee.

## Initial bounded release/native protocol

After the source backup, build once with the established canonical profile and
process-local allocator settings. Source implementation is `ad3bd23`; later changes
add only synthetic fixtures and this handoff. Preserve the qualified 109-layer
packages and prior QA evidence. No new actual-theme LEP is claimed by this feature.

Use the checked-in synthetic project files. The renderer-probe run plan selects
compositions; a final CLI wrapper may create exact active-composition copies and
must record those storage swaps. Run the same 15 cases once on the pinned binary,
retaining exact literal expectations and explicit errors. Include one final actual
Black player 1671 and Lyric 4360 regression. The half-scale source case is a source
transform test, not an export-resize claim.

Native scope: open a copied constant-boundaries project, select its Gaussian Blur
and toggle Repeat through Effects. Verify edge behavior visibly, then Save, one
Undo/Redo and reopen with a complete source comparison. Check the radius 0/bypass
identity, a copied interior adjustment/matte case, and the ordered shadow case.
A rotated/sheared Repeat scene must show the explicit error and recover when the
mode is disabled. Validate a pending numeric-field commit before the toggle,
selection/document/time changes canceling stale input, and disabled editing while
playing/locked. Observe the mode on saved preset/duplicate/reset if practical;
model tests already cover those data contracts. No actual missing-movie placeholder
or uncertain blur-unit mapping may be presented as a completed theme.

The narrow keyboard correction is natively qualified on `7de5cfabe868`, with
its own release identity and source/history evidence documented in
[REPEAT_EDGE_KEY_OWNERSHIP.md](REPEAT_EDGE_KEY_OWNERSHIP.md). This does not change
the initial failure or denied Reset attribution. Windows native behavior,
all-frame playback reliability, AE effect calibration and full reference parity
remain unverified.
