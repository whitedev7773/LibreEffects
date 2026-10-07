# Bounded paragraph geometry styles

Contract, 2026-10-04. This continues F03 without introducing Text Animator,
per-character formatting, justification, alternate shaping or AE equivalence.

## Stored model and edits

Five layer-wide static source-pixel values live in TextStyle: left/right indent,
first-line indent, and space before/after. All default to zero and are individually
omitted at zero; old documents and default renders retain their representation.
Left/right/before/after accept finite 0–16,384 values; first-line accepts finite
−16,384–16,384. First-line is an offset relative to left indent. Fields remain
stored but dormant in point mode. Any nonzero value requires project schema 55,
including dormant fields; LEP1 and existing VIEW/address formats do not change.

A dedicated atomic field command changes only its requested field and promotes
an existing schema only when necessary. It validates source and candidate,
retains assets, all source-text/typography/paint keys and unrelated compositions,
and preserves the complete source and Redo on exact no-op. UI numeric formatting
alone is a no-op. Locked/non-text/invalid requests fail without history changes.

## Shared geometry

Only existing LF hard-newline boundaries start paragraphs (CRLF retains its
existing stripped-CR rendering). Blank and trailing-empty paragraphs are real
paragraphs. Existing Unicode soft/mandatory line-breaking behavior remains;
other separator characters do not acquire new paragraph semantics.

First visual line x = left + first-line; subsequent wrapped line x = left.
Available width = box width − right − x. Alignment uses this line-local interval.
Nonpositive width is overflow, never a silently fitting clamped interval. Negative
hanging text can extend left and is clipped to the original box. No outline
stroke advances or fallback/shaping policy changes are introduced.

Every paragraph starts with space-before; between paragraph origins add the
ordinary font-size × leading advance, previous space-after and next space-before.
Wrapped lines retain ordinary leading. Space-after contributes to Fit height,
including after the last paragraph, but does not hide otherwise fitting ink.
Composition stops at the first line whose existing ink/width check fails.

Rendering, preview/export, caret/hit/selection/IME anchor geometry, overflow,
Fit and Point conversion use the same line origins and widths. Vertical caret
navigation follows adjacent actual visual lines despite large paragraph gaps.
Point conversion retains the established current-frame Source Text behavior;
paragraph styles remain dormant rather than being destructively discarded.

## UI and acceptance

Paragraph provides five labeled pixel fields beside existing dimensions.
Controls preserve exact source/action/transport ownership, reject invalid or
stale inputs and cannot mutate while blocked, locked or during marked input.
A committed field is one Undo step; a following action is a separate transaction.

Independent acceptance compares literal positioned SVG lines, complete source,
JSON/LEP/VIEW, multiple animated Source Text/typography samples and old zero-style
results. Cover wrapping, blank/CRLF paragraphs, hanging indents, all alignments,
nonpositive width, clipping, Fit, caret movement, history and no-op Redo. Final
whole gates, release identity and bounded native observations must be attributed
to exact source/build pins. Linux evidence does not qualify other platforms,
real IME, all Unicode scripts, bidi/ligature caret precision or the remaining
text roadmap. No push, deployment or user-desktop action is authorized here.
