# Authored point-text spacing qualification

`probe.rs` is a standalone GPUI-free binary, deliberately not `main.rs` and not
a Cargo integration test. Build it with the workspace's pinned toolchain and
one matching set of fresh debug dependency artifacts after rebuilding the core
and editor model. Use one compiler, incremental off, and no `--test`. It imports
the production renderer, rich-text compositor, font catalog and text layout.
Only unused external media and expression-worker seams fail closed. The
`checks.rs` cases are also included in the desktop unit suite.

Direct dependencies are `libre_effects_core`, `libre_effects_editor_model`,
`libre_effects_ae_expressions`, `libre_effects_audio_spectrum`, `image`, `resvg`,
`base64`, `serde`, `serde_json`, `unicode_segmentation`, `rustybuzz`,
`unicode_bidi`, `unicode_script`, `unicode_linebreak`, and `sha2` 0.10.9. Resolve
these from a single Cargo artifact receipt rather than choosing arbitrary old
rlibs when multiple versions are present.

Run `authored-spacing-probe OUTDIR` to run the suite and save the synthetic
native project, independent literal SVG, actual PNG and reference PNG. With no
argument it runs the same assertions without writing fixtures.

The only font fixture is the repository's shipped Wanted Sans Regular, with a
fixed file SHA-256 and glyph IDs read independently from its cmap. Fixed
positions include a negative first origin, a repeated letter, a space, and the
multibyte scalar é across paint spans. A CRLF separates an unannotated second
line. Every pixel expectation comes from a literal SVG, not output recovered
from the authored-position renderer.

The suite checks exact pixels for all alignments, separate terminal caret
geometry, width independence, CRLF carets, picking, retained stroke/fill and
opacity bounds, ordinary-line layout and legacy pixels. It rejects wrong font
hash/index/glyph identity and ordinary whole-line ligatures without mutating
the payload. Uppercase hex digests remain valid. Final outlined paint is also
rasterized with an empty font database to prove that export does not reopen a
font after verification. A native save/reopen fixture requires identical
production render, preview and export pixels and explicit hash mismatch errors
on all three paths.

A deliberately short terminal (`x=1`) beneath a 32 px W with a 20 px stroke
checks the distinction between paint and caret geometry. Actual flattened ink
extends bounds and remains pickable while the terminal caret stays at x=1.
Only authored lines acquire this ink union; legacy bounds remain unchanged.

Font snapshots are loaded only when authored positions use that face, with a
64 MiB per-file limit and a 256 MiB process total. Concurrent reservations are
made before copying, failed loads release them, and successful immutable
snapshots are retained with their verified SHA-256. Exhaustion fails explicitly.

This verifies native authored origins. It does not qualify unrestricted complex
shaping, mixed-font authored lines, external-app caret parity, a general optical
kerning algorithm, or private reference-frame parity. No private source text,
font, project or program is stored in these fixtures.
