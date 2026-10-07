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
