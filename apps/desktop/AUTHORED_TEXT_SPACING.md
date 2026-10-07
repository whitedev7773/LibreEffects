# Saved point-text glyph spacing

Schema 80 optionally stores exact horizontal glyph origins for an ordered subset
of complete single-font point-text hard lines. Unlisted lines keep normal shaping.
Original text, font size, tracking, alignment and leading are preserved. Positions
are a native authored-data contract, not an optical-kerning algorithm for new text.

## Admission and rendering

The payload binds exact text bytes, valid UTF8 single-scalar grapheme spans, the
shaping style, alignment, exact PostScript face and collection index, font-file
SHA256, glyph IDs, increasing finite horizontal origins and a separate logical
end boundary. It does not normalize Unicode/whitespace or reinterpret CR/LF as
painted glyphs. Complex/reordered shaping, mixed-font positioned lines and a
changed/missing font version fail closed. Existing baselines remain authoritative.

Font snapshots load lazily only for positioned lines;64MiB per file and 256 MiB
retained per process are explicit limits. Verified bytes are used for both
shaping and flattened paint, so final rendering/export never reopens another
version of that font. Fill/stroke and caret/selection/picking geometry share the
same positioned compositor output. Paint bounds include visible overhang while
the caret retains its independent logical terminal boundary. Ordinary lines and
legacy documents preserve the old path.

## Editing and history

Text, font, size, tracking, alignment, leading or point/paragraph changes reset
saved positions atomically. The Character panel shows this behavior before an
edit and reports a draft reset afterward. Explicit Reset saved spacing is one
undoable action with document/time/selection/IME/lock/transport guards; key-up
cannot reactivate an action owned by transport or a previous press. Text and
script commits report an actual reset. Undo restores the exact old payload;
typing the old text again does not silently revive it.

No-op edits, paint-only formatting and downstream transforms retain positions.
A SourceText expression changing text invalidates the evaluated copy only.
Core rejects malformed source/style/range geometry; the renderer independently
checks actual font bytes and glyph/cluster identity. Public APIs and fixtures are
generic and contain no private source project/text/expressions/fonts.

## Current verification

- Nine focused new model cases passed; the final 56-case scoped editor-model run
  includes those nine plus affected rich/selected/point/Playbar cases.
- 168 core text/source regressions passed.
- The independent synthetic production probe passed literal SVG pixels for all
  alignments, paint/stroke/opacity retention, CRLF caret/hit behavior, legacy pixels,
  wrong hash/index/glyph/ligature rejection, fontless final paint and native
  save/reopen/render/preview/export equality. A single stroked W with end_x = 1
  confirms pickable overhang with the caret still at 1.
- Formatting and final offline desktop all-target check passed. The earlier
  intermediate 93d3eeb checkpoint was model only; the final release and bounded
  native checks below now pass. No full workspace test pass is claimed.

The private first admission is 57 audited single-font 46 px lines. Four mixed-font
lines and 53 px variants remain unconverted. An isolated, unfitted source-position
proof improves one reference line's bright-mask overlap from 0.0942 to 0.8209;
that measurement does not establish exact raster parity, general cache semantics
for other applications, or external caret behavior. All 93 source text records
were separately audited for source/font/glyph mapping before selecting this subset.

## Bounded native qualification — 2026-10-07

Clean source `88b436b97c0e0449e0c0f955e8c220322397ad64` passed one canonical release
(build `20261007.044734-a00edb0dae48f504`) and three predetermined CLI calls. The
public synthetic literal reference is exact; the prior schema79 actual frame is
unchanged. The new actual frame changes only the two source-predicted Japanese
regions. A fixed opaque-frame bright-mask diagnostic narrows the focused line
from 492 px to 454 px against the supplied reference's 455 px; this is not an AE
raster-equivalence claim or the same metric as the earlier transparent-text proof.

Actual-layer native Character checks establish byte-exact no-op retention,
paint-only retention, atomic size47/cache removal, one document Undo/Redo, final
restoration and Save/Open/Save preservation of all 57 positioned lines. A public
synthetic selected-glyph draft separately establishes visible reset, local
Undo/Redo and source-preserving Cancel. The actual Lyric canvas retains its
existing expression-driven selection-only guard. The app closed normally.

The native session did not replay explicit Reset-button held-key, playing/locked,
or wrong-font recovery. Their source/model/probe coverage remains distinct;
marked IME and Windows runtime remain unqualified. Default-width whole-layer Character value fields are
narrow, but the tested numeric and paint edits were usable. Full-desktop capture
was used after window-bound screenshots returned black despite normal visible UI.
This capture behavior did not require an application restart.

Initial actual serialization admits only object ordering and the five specifically
identified legacy default omissions. Exact raw source tokens, all positioning,
images and aliases stay strict; subsequent history and final saves are byte-exact.
See STATUS/HANDOFF and the external `authored-spacing80-release-20261007` evidence
for binary identity, fixed pixel regions, source plans, retained limits and audit.
