# Bounded inline SVG presentation declarations

## Contract

This E06 continuation extends the existing static importer only. The `style`
attribute is accepted on the root SVG, groups and supported drawing elements,
not on inert title/desc. Supported properties are exactly `fill`, `stroke`,
`fill-rule`, `fill-opacity`, `stroke-opacity`, `stroke-width`, `stroke-linecap`,
`stroke-linejoin`, `stroke-miterlimit`, `stroke-dasharray`, `stroke-dashoffset`
and `opacity`. No project/container/view/address schema changes.

A deliberately closed ASCII grammar accepts `property : value` declarations,
separated by semicolons, with an optional trailing semicolon. Empty declarations
and empty/whitespace-only styles are no-ops. CSS ASCII whitespace is accepted.
Property names, color names, supported keywords, RGB function names and `px`
units are ASCII case-insensitive in inline declarations. XML attribute names
remain case-sensitive. Existing presentation-attribute parsing is retained.

All attributes are validated and applied before inline declarations regardless
of their order in XML. Valid inline declarations override the corresponding
attributes; the last valid declaration of a repeated property wins. Every
supplied declaration and presentation attribute must be valid, even when later
overridden. This fail-closed policy deliberately differs from CSS error recovery:
an unknown/malformed/unsupported declaration rejects the entire import, rather
than being dropped in favor of another declaration.

The existing solid color, number, length, stroke and opacity value parsers retain
their complete-value and editable numeric bounds. Inline numeric tokens also
follow CSS number syntax (digits must follow a decimal point); literal RGB uses
either three comma-separated or three whitespace-separated channels, never mixed
separators. Numeric overflow and nonzero underflow to f64/f32 zero reject.
The closed lexical scan rejects
comments, escapes, strings, braces/brackets, at-rules and priority markers before
interpreting delimiters. Functions are rejected except a complete, unnested
three-channel literal `rgb(...)` value for fill/stroke. No styles, strings or
resource-bearing source data enter the geometry normalizer. No network resolver
is added. This closed language is intentionally smaller than general CSS;
there is no forgiving CSS parser or naive unrestricted delimiter splitting.

Implicit inherited values retain the computed parent paint/stroke state. Local
attributes then local inline declarations override that state. Element `opacity`
is separately composited and defaults to 1 for each element; it never implicitly
inherits or multiplies into the inherited paint opacity. Explicit `inherit`,
`currentColor`, `initial`, `unset`, `revert` and `revert-layer` are unsupported,
including when a later declaration could hide them. `color` itself is unsupported.
`!important` is rejected, rather than silently losing its cascade semantics.

## Limits and exclusions

At most 16 KiB decoded inline-style bytes and 128 nonempty declarations per
element, and 2,048 nonempty declarations per document. These supplement existing
1 MiB source, XML/tree/geometry and numeric budgets. ASCII non-whitespace control
characters, non-ASCII tokens, malformed declaration separators and empty values
reject. Delimiters inside unsupported strings/escapes/comments/functions are
never interpreted as permitted declarations.

Selectors, stylesheets, class, custom properties, variables, calculations,
resources/URLs, gradients, transforms or geometry in CSS, fonts/text and every
previously unsupported element/attribute remain outside this slice. Existing
integer-pixel viewport clipping and cubic-normalization/AA limitations remain;
no general SVG/CSS compatibility or raw-source pixel equivalence is claimed.

## Evidence

Separate design, parser, independent fixtures/tests, UI guidance and final
acceptance commits. Literal inline inputs have independently authored equivalent
presentation-attribute references; imported source, renderer and JSON/LEP routes
must match exactly. Cover precedence/XML order, duplicates, case/whitespace,
implicit inheritance versus local opacity, stroke/dashes and paint ordering;
reject resources/escapes/functions/priorities/CSS-wide tokens/unknown properties,
malformed and over-budget input including bad shadowed declarations. Old corpus
and existing no-op/Undo/Redo/stale-input transaction guards remain covered.

Run all pinned workspace/media/vendor/font/check/format/release gates. Pin the
source/build/binary and use that release for bounded actual native import,
unsupported-style rejection preserving history, Undo/Redo and save/reopen. Keep
previous project/QA files immutable. Finish acceptance, next gaps and verified
complete-history bundle before another feature. No push/deploy/user desktop.
