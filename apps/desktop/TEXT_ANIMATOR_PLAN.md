# Text Animator foundation

Contract, 2026-10-04. One hard Range Selector with Position X/Y and Opacity
continues F03. This is not After Effects compatibility or a complete text animator
stack. Multiple animators/selectors, softness, random order, word/line
units, rotation/scale, per-character styles and path text remain separate work.

## Source and time

Five appended `TextParam` values live in the existing sparse `text_parameters`
map: `AnimatorStart`, `AnimatorEnd`, `AnimatorPositionX`, `AnimatorPositionY`,
`AnimatorOpacity`. Missing values mean 0%, 100%, 0 px, 0 px and 100%. Any stored
entry, including a static default or dormant track, requires schema 56. Old
omitted storage, LEP 1, VIEW 1/2 and address 1 remain unchanged.

Start/End/Opacity are finite 0–100. Position is finite ±1,000,000 source pixels.
Existing scalar sampling, interpolation, 10,000 keys/track, composition frame
and 16 MiB metadata bounds apply. Sampling clamps easing overshoot to property
bounds. Source Text is sampled first; selector indexing is recomputed from that
complete current string. Animator-only editing validates original and candidate
source atomically without historical asset migration, promotes only the needed
schema, and preserves unrelated source, key metadata and exact no-op Redo.

## Graphemes and shaped units

Index Unicode extended grapheme clusters in logical source order, using the
pinned `unicode-segmentation` implementation. Spaces, tabs, LF and other source
graphemes count even when no ink is painted. CRLF is one extended grapheme.
For N graphemes, grapheme i has center `100 * (i + 0.5) / N`. It is selected when
`Start <= center < End`. Empty text or Start >= End selects nothing. End 100
includes the final center. This is a discrete hard range, not fractional coverage.

Graphemes and glyph clusters are different. The authoritative compositor retains
shaping, bidirectional order, fallback faces and actual glyph positions. Original
shaping cluster source intervals expand to complete source graphemes; overlapping
expanded intervals merge transitively into protected units. Selecting any
grapheme of a protected unit selects all its glyphs. Thus an `ffi` ligature can
move as one unit when only one logical letter center is selected. Combining
marks, Hangul sequences and emoji components cannot receive different effects
inside one extended grapheme, including across fallback glyphs. No Unicode-scalar
split, proportional glyph guess or heuristic glyph-ID match is permitted here.

The implementation must retain actual source cluster ranges through usvg's
shaping/flattening rather than infer them from repeated glyph identities. A
minimal pinned vendor extension may carry those ranges and apply protected-unit
effects while preserving outline, COLR, embedded-SVG and bitmap fallback routes.
Missing font coverage remains the renderer's existing limitation; selection
safety does not imply every script/emoji/font renders correctly.

An attenuated protected unit must form one contiguous visual paint run within
one span. An unsupported interleaved/spanning unit fails explicitly rather than
being split into several opacity operations or reordered. Resource serialization
uses object-identity-derived unique IDs so independent color/SVG glyph paint
servers cannot collide by their original names. Preflight and final output stay
within the existing 64 MiB expanded-SVG ceiling; the preflight is conservative.

## Paint and geometry

Layout and paragraph wrapping finish before animation. Selected units receive
the full X/Y translation in text-layer source axes; unselected units stay fixed.
Animated ink does not reflow, alter alignment, change Fit, move the source caret
or modify text-edit hit testing. Paragraph clipping stays at the original box
and clips transformed ink. Layer/parent transforms, masks, effects and blend
continue afterward through existing composition routes.
Existing path-mask domains remain fixed at the layer's original source rectangle;
moving point-text ink outside that domain can clip it even without paragraph mode.

Selected-unit opacity multiplies its paint within each complete fill/stroke pass.
The existing whole-layer fill/stroke order and independent pass opacity remain
authoritative; this is not per-character alternating fill/stroke. Glyphs in one
protected unit attenuate together. Separate protected units retain normal source
overlap behavior. Unattenuated transformed geometry supplies existing effect
bounds even when selected opacity is zero. Empty selection and effective identity
(X=Y=0 and Opacity=100) use the exact legacy geometry path.

## Editing and acceptance

Text Properties exposes five precise typed values with guarded animation/key
actions, and materialized tracks use the existing Timeline/Graph operations.
Disabling animation explicitly removes all keys after sampling the current
value. Source/frame/selection/action/transport bindings and one-use successful
pending-field receipts reject stale, marked-IME and interrupted callbacks.

Verify literal selector boundaries and protected closure, source/history/codec
preservation, independently constructed pixels, paragraph/clipping/paint/effects,
animated Source Text and typography, fallback/RTL/ligature/combining/emoji cases,
all aggregate gates, old/new legacy CLI equivalence and a bounded native editing
and save/reopen sequence. Record exact source/build pins and unexecuted native
script/platform/IME/adversarial matrices. No push, PR, deployment or user-desktop
work is authorized in this slice.

## Range Offset continuation — 2026-10-05

The appended `AnimatorOffset` scalar translates both independently sampled
Start/End values by −100…100 percentage points. Its omitted default is 0.
Effective endpoints are each clipped to 0…100, without wrapping, reordering,
or changing authored Start/End tracks. An off-range, empty or reversed interval
selects nothing. Offset uses the existing hard grapheme-center selector and
protected shaping units, with no renderer or layout changes.

Any materialized Offset, including a static default or dormant track, requires
schema 58. Omitted storage and all existing animator channels keep their previous
schema minima. No unrelated migration is performed. LEP, VIEW and property
address versions stay unchanged. Inspector, Timeline and Graph expose the same
independent scalar/key channel and existing guarded editing/history operations.
