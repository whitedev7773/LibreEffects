# Bounded inline SVG presentation qualification

## Contract and scope

E06 now accepts a deliberately closed literal `style` attribute for the existing
12 paint/stroke/opacity properties. Read [the grammar](SVG_INLINE_STYLE_PLAN.md),
[base SVG contract](SVG_IMPORT_PLAN.md) and [user guidance](README.md#import-svg-as-editable-shapes).
Inline declarations override valid presentation attributes irrespective of XML
order; the last valid repeated declaration wins. Every supplied value remains
validated, including shadowed values. Paint/stroke state inherits; element
opacity is local subtree compositing with a default of one.

Property names and supported inline keywords/units are ASCII case-insensitive.
Empty styles/declarations are no-ops. Comments, escapes, strings, priorities,
CSS-wide/currentColor values, custom properties, unsupported properties/functions,
selectors, stylesheets and resources reject the entire file. RGB requires three
complete uniformly separated channels; CSS numeric tokens reject trailing dots,
nonfinite/out-of-range values and nonzero underflow. The decoded-style limit is
16 KiB /128 declarations per element and 2,048 declarations per document, in
addition to the existing XML/file/tree/geometry budgets. No dependency, schema,
LEP/VIEW/address version, renderer or resource resolver was added.

Work started clean at `06069d2` in the same dot-cloud clone and branch
`codex/ae-workspace`. All commits are local; no push/PR/deploy/user desktop.
Design `41ca505`; parser `417dcd3` /strict CSS numeric correction `4fda1e2`;
UI/help/history `1b8bdad`; contract link `f5d4a94`; independent fixtures
`06d20cf`; explicit exported native VIEW regression **fb8b03a**.

## Independent semantics and retained failures

Twenty literal files contain six source/attribute-only/cubic families and two
retained original renderer-limitation witnesses. Complete imported editable
source equals independently authored presentation-attribute equivalents.
Canonical source/attribute/cubic images and native shared/preview/output/JSON/LEP
routes compare every RGBA value exactly at frames 0/30/60: **132 pairs /
5,068,800 pixels**. Separate visible witnesses distinguish isolated opacity,
inherited fill-opacity, winding, dashes/offset and paint ordering. All 12 fields,
XML order, repeated declarations, whitespace/case, numeric grammar, work limits,
whole-file rejection, history, fresh IDs and codec behavior are covered.

Pinned raw-SVG rendering has two separately characterized limitations:
case-sensitive property lookup changes 6,553 pixels (max channel delta 255) for
the retained mixed-case source; the simplecss tokenizer stops on leading empty
declarations, changing 6,084 pixels (max 191). These original files stay unchanged.
Both imports still require exact editable source and all native renderer routes
against independent attribute/cubic references. These discrepancies are not
native-output tolerances and no pixels are masked. Six canonical inline fixtures
also match raw-source rendering exactly. General raw-SVG equivalence remains
unclaimed, as do the existing arbitrary-scale cubic/AA and fractional-viewport gaps.

Review found and closed mixed RGB separator acceptance, CSS-invalid trailing-dot
numbers and nonzero underflow before aggregate qualification. Preliminary fixture
runs also exposed a test-only nested SVG wrapper applying root opacity twice;
all new fixtures have full 240×160 viewports, so their oracle now renders each
literal directly. The overlap alpha witness is 154 from the pinned 8-bit alpha
composition, not an unquantized 153. Failed logs remain retained. The second focused
run used default desktop debug metadata; its reconstructible 686,566,256-byte test
binary was hashed and removed to recover space. It is not final-profile evidence.

Initial **06d20cf** passed 1,798 defaults and all 12 gates. Its first actual native
save matched complete source but failed full VIEW comparison: the generated
headless expectation omitted the active composition default view, which the app
correctly saves. **fb8b03a** explicitly authors that entry before final recording,
with a regression comparing all seven complete literal exported VIEW payloads,
source preservation and immutable repeat exports. The native verifier was not
weakened and actual data was not normalized. Initial logs, 45 generated files,
release and actual save remain under `final/`; final qualification is `view-final/`.

## Final gates and frozen release

Final code **fb8b03ae828db80b7d4f118c4f747c839957c857** passes all 12 gates:
**1,799 default tests** (597 core +1,196 desktop +6 build-identity), **32 media**,
workspace/all-target checks, rustfmt, grid debug/release/no-default variants,
usvg formatting/default/no-text, explicit DejaVu ffi and normal optimized release.
Direct Cargo equivalents ran, not Moon. Only desktop debug/test metadata was
reduced/stripped; normal release settings were unchanged.

Build **20261004.224701-4242695670ebf006**, 465 watched inputs,
`x86_64-unknown-linux-gnu`, 67,276,256 bytes; SHA-256
`bb86a3b335ccedbbb257e5e362a81e76ce6f6bfc945d1cd2daa40b33bc2239f0`.
Native About matched the exact clean-source commit, fingerprint, time and profile.
The final source manifest and hashed test/release binaries are preserved.

Final new generated CLI: **72 renders /90 exact pairs /3,456,000 pixels**;
all 45 files unchanged. Prior SVG corpus: **132 renders /165 exact pairs /
6,336,000 pixels**, all 69 files unchanged. Both compare the final release with the
previous `e078b4f` SVG release, which reads the same schema 44 imports. Original
SVG corpus files in the repository were not changed.

## Actual native recording on the final build

All eight final saves use **fb8b03a**, the frozen build above, frame 0 and the
240×160/30fps/90-frame independent composition. The working directory is
`/workspace/shared/sv2`; immutable byte-identical archive copies are in
`view-final/native/` under the QA root. No actual saves were fabricated.

1. `01-import.lep`: actual Open blank → keyboard File-menu SVG chooser → inline
   precedence fixture → Save As. Both XML orders and inherited root paint visible.
2. `02-undo.lep`: exactly one closed-menu keyboard Ctrl+Z removed the entire
   imported layer; actual blank Save As.
3. `03-reject.lep`: actual `!important` input rejected while Redo existed; explicit
   whole-file diagnostic, unchanged blank source, then Save As.
4. `04-cancel.lep`: File-menu SVG chooser → Escape; explicit canceled status and
   unchanged source, then Save As.
5. `05-redo.lep`: exactly one Ctrl+Shift+Z restored the import after rejection,
   cancellation and intervening saves; actual Save As.
6. `06-shadowed.lep`: negative stroke width followed by a valid duplicate rejected
   with bounds diagnostic; existing import unchanged, then Save As.
7. `07-strokes.lep`: Open blank → Ctrl+Shift+P → click SVG in unfiltered results →
   chooser → inline stroke/dash fixture → Save As.
8. `08-reopened.lep`: actual New (fresh Untitled observed) → Open actual 07 →
   Save As 08. Final app is clean on 08, frame 0, with stroke/dash geometry visible.

All eight pass **exact full source and VIEW** and **120 native renderer/codec
pairs /4,608,000 pixels**. Native CLI adds **48 renders /48 exact pairs /
1,843,200 pixels**, including old/current comparisons. Combined new/legacy/native
CLI totals: **252 renders /303 exact pairs /11,635,200 pixels**.

Byte-identical groups: 01/05/06 are 7,832 bytes, SHA-256
`7f020e593f31793b204f9379e63f22c0ce29b969e7a9831229fb885bfd19de69`;
02/03/04 are 950 bytes,
`dc390a8962b0f07fc6937a22a205378e00264e6e38d93aaf0caa3f776edcf41a`;
07/08 are 8,489 bytes,
`c3a82ab4b3e164d97012308a9578ec281606593cb2059ca6988394c16fdfb8ab`.
Previous SVG `08-final-reopened.lep` working/archive copies, compound Colors
`10-final-reopened90.lep`, interpolation `12-final-reopened.lep` and animator
`14-final-reopened.lep` remain hash-exact. GitHub remains open; no lock/runtime
profile modification. Screenshots were inspected, not archived.

The supported global desktop `type_text` worked for GTK chooser inputs. The
bound-app Paste action and GPUI text-input provider were unavailable; those
preliminary probes were canceled. Native command search was unfiltered, not typed.
These ordinary frame-zero flows do not qualify direct imported Contents edits,
pending-draft/stale-result races, held input/real IME, every transform/style/viewport
combination, other OS/DPI/devices or arbitrary-scale source fidelity.

**E06 remains Partial.** General CSS/exporter metadata, explicit inheritance and
currentColor, priorities, gradients/resources/text/images/use/defs, filters/clips/
masks, fractional viewports, SVG animation and source round trips remain outside
this slice. Keep earlier roadmap/native gaps distinct. Finish the verified
complete-history bundle before the next feature; no next feature has started.

QA root: `/workspace/shared/libreeffects-qa/svg-inline-styles-20261004`.
`view-final/` is authoritative; `final/` is the earlier VIEW-expectation candidate.
