# Libre Effects usvg extension

Source: official crates.io `usvg` 0.45.1 package, retained with its MIT and Apache
licenses. The root Cargo patch makes resvg and the app use this one copy.

The local extension exposes `PositionedGlyph::source_range`, captured from the
final fallback-resolved shaping clusters before tracking hides any cluster.
It also exposes `PositionedGlyph::baseline_origin`, calculated from the span
and cluster transforms without the font/mark outline offset. The consuming
`Tree::with_text_glyph_effects` API supports post-layout affine transforms,
translation and protected-unit opacity. Existing no-effect parsing, shaping,
fallback, flattening and painting remain unchanged. No independent shaper or
font-outline approximation is introduced. All upstream outline/COLR/SVG/bitmap
flatten branches remain authoritative.

Libre Effects can additionally install a renderer-owned `PathCache` for a
synchronous parse/render scope. It reuses immutable path geometry only when
the entire `d` string matches; paints, CSS, transforms, clipping and filters
remain current. The retained-payload limit includes source keys, conservative
point/verb capacity and entry overhead, with 1024 entries and an 8-MiB maximum
key. Cache allocation failure simply declines insertion. Scopes restore their
caller on drop and cannot move between threads. The default parser has no cache.
Pixel regressions cover changed paint/transforms/clipping, changed curves and
the upstream partial-path behavior on malformed suffixes.

The callback addresses ordinary scene text by its ID and receives its original
layout. It must return one validated effect per positioned glyph, or None.
Identity-affine effects retain the existing translation/opacity path and its
upstream path merging. Nonidentity affine effects wrap complete original paint
pieces, including authored strokes and all glyph formats. Adjacent identical
opaque affines can share a piece; attenuated contiguous protected clusters
receive one opacity operation each. Affines do not define opacity-unit IDs. Underline/overline/strikethrough
are preserved once and remain outside the editor's bounded animator contract.

## Provenance and removal

- crates.io archive SHA-256:
  `80be9b06fbae3b8b303400ab20778c80bbaf338f563afe567cf3c9eea17b47ef`
- Upstream repository revision: `1b6c2fddbcbeffa8135df4323b02aaae84890907`
- Upstream source, manifest, documentation and both licenses are retained.
- Remove this patch only after a tested upstream API exposes equivalent source
  cluster metadata and post-layout effects for all supported glyph formats.
- Local regression tests use the application's shipped Wanted Sans font; run
  `cargo test --manifest-path vendor/usvg/Cargo.toml --lib` from the repository root, plus a no-default-features
  check and the desktop renderer/source compatibility suites.

## Opt-in materialization and limits

`Tree::to_string_with_unique_resource_ids` is used only by the new animated
text path. It assigns deterministic unique IDs by resource/node identity, so
independently imported glyph gradients, patterns, clips, masks, filters and
`feImage` references cannot alias each other merely because their original IDs
match. Embedded SVG image documents receive the same treatment within their
own reference scope. Ordinary `Tree::to_string` keeps upstream behavior.

The opt-in writer also emits round-tripping `f32` decimals for paths and
transforms. Upstream's precision multiply/round/divide can change a coordinate
by an ULP even at high requested precision. That affected antialiased pixels;
the local exact-coordinate and glyph-path bit-pattern regressions cover it.

Effects fail explicitly if metadata is invalid, if glyphs in the same protected
unit disagree about an effect, or if an attenuated unit occupies disjoint paint
runs/spans. The editor's uniform text style normally has one span. This error
boundary avoids splitting one intended opacity operation or silently changing
paint order. Whole-node uniform affines and one complete protected unit retain
the original flattened paint subtree. `GlyphRenderEffect::default()` is an
identity affine, zero translation and full opacity. Translation is applied
after the affine. Nonfinite matrices fail explicitly; a singular affine drops
collapsed ink, including stroke, and returns finite empty bounds.

The application checks source normalization and UTF-8 boundaries, retains
original source indexing across CRLF and trimmed whitespace, preserves the
legacy output when no glyph is selected, and applies paragraph clipping after
motion. A full shaped-pass prepass joins authored Words/Lines across visual
wraps and chooses their first logical rendered glyph's baseline origin, in
layer-local coordinates. Connected shaping/grapheme protection merges any
crossing source units and gives Scale/Rotation their maximum influence. Position
and opacity retain their prior per-cluster weights; opacity protection IDs stay
independent of transform groups. Characters use the protected cluster itself.
The prepass uses existing layout only, never another shape or layout operation.
It uses a conservative 64 MiB expanded-geometry serialization budget
before serializing and checks the final SVG byte limit afterward. An otherwise
valid long source string can exceed the expanded budget.

Regression evidence includes synthetic resource collisions, exact-coordinate
round trips, source clusters, negative tracking, protected opacity units and
same-font desktop pixel oracles. Reusing upstream COLR/SVG/bitmap branches does
not by itself qualify every platform, font format, script, DPI or native device.
That broader color-font/platform matrix remains unqualified.

## Gaussian blur edge mode

`GaussianBlur::edge_mode` retains an authored `feGaussianBlur edgeMode` of
`none`, `duplicate` or `wrap` for the renderer. Missing and unknown values use
`None`, as do CSS `blur()` functions, preserving the existing transparent-edge
behavior. Both SVG writers emit `duplicate` and `wrap`; they omit `None` so
existing default blur serialization remains unchanged. Focused regressions
cover parsing, both writer round trips, invalid/default values and CSS blur.
The editor's Repeat Edge Pixels option uses `duplicate`; retaining `wrap`
in the tree and writer does not by itself establish renderer support for it.

## Multi-span shaping repair

Font overlays replace complete source intervals whose boundaries occur in both
shaping results. UTF-8 cluster lengths are never used as glyph counts. This
prevents a panic when a repeated span contains combining marks and preserves
all glyphs when fonts produce different ligature/mark decompositions. The
desktop selected-text source-join regression exercises this path with `e`, a
space and a combining acute accent before a transactional edit.

## Proportional font metrics

`font-variant="proportional-width"` requests OpenType `palt` while shaping the
whole text chunk, including fallback fonts. Small caps and proportional widths
can be combined; both SVG writers retain the requested variant. Missing variants
keep the existing shaping behavior. Libre Effects uses this for explicitly
converted proportional rich-text metrics, rather than altering legacy documents.

## Explicit fractional-box blur

The private `data-libre-effects-box3-radius` attribute on `feGaussianBlur`
retains one or two finite f64 radii in 0..8192, independently of stdDeviation.
Primitive-unit conversion and both writers preserve those radii. CSS blur and
ordinary Gaussian filters have no profile and keep their existing behavior.
Malformed values, use on another element, or duplicate/wrap edge modes reject
before tree conversion. The private parser table leaves generated SVG names
unchanged. This is a mathematical rendering contract, not an AE compatibility
declaration; the resvg patch specifies its passes, quantization and limits.
## Private opaque layer-opacity profile

Normal `<g>` elements can explicitly carry
`data-libre-effects-compositing="opaque-opacity-byte257-v1"`. The parsed group
retains a typed flag separate from generated SVG attribute names. Both writers
preserve it; unknown values or use on another element reject. The tree exposes
whether any group or resource subroot requests this profile so resvg can select
checked rendering. This flag does not change ordinary SVGs or force isolation
for a fully opaque group. Its limited pixel arithmetic is defined by resvg.
