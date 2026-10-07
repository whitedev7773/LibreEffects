# Native selected-character formatting

Scope: author mixed font/size/fill runs from an ordinary text draft. AEP import is
not required. Existing rich text wire schema 71 is reused; versions 72/73 remain
preserved. No new format schema or expression runtime is introduced.

Implementation:
- Core patches only one chosen attribute over a nonempty grapheme-aligned range.
  CRLF stays atomic, untouched source/default style/attributes remain exact, and
  adjacent equal runs merge before enforcing the 4096-run budget.
- Buffer promotes plain text only for an effective style change. Text, styles and
  directional selection share local history and monotonic ownership generations.
- Character controls edit the Session draft. Field/picker receipts include the
  session identity, generation, range, document and frame context. Stale events
  cannot retarget the current selection; marked input is preserved.
- The workspace pointer capture preserves a draft when opening Character and
  using its controls. Outgoing clicks submit only its focused, unmarked numeric
  or color field before Preview closes the draft. Unhandled Character/header keys
  cannot become document layer shortcuts.
- Style-only commit uses SetRichText. Combined edits use SetStyledText to install
  the exact final source and runs atomically, avoiding an invalid intermediate
  diff against the old styles. Accepting the draft is one document Undo.
- Font inventory includes root, empty-text insertion default and run identities,
  deduplicated per font/layer. Existing incomplete rich glyph analysis remains
  explicit; no new shaping analyzer is claimed.

Validation:
- Pure model checkpoint 46c03e6: 42 focused tests passed. The final same-family
  no-op correction adds two regressions; its selected-text filter passes 10/10.
- The first integrated aggregate passed 273 library and 4 integration tests.
  The final aggregate passed 274/275 library tests: the unchanged 192-binding
  expression-pooling case exceeded its existing 100 ms execution budget. No
  budget was changed and this result had not been retried at that checkpoint.
  All selected-text
  cases passed. Preserve both logs; this is not a fully green final model gate.
- Final canonical all-target check passed in 1m 00s; formatting and diff checks passed.
  Session/field/font diagnostic regression sources compile there; no monolithic
  desktop test executable is built or run.
- Independent source review found keyboard-ownership and outgoing-field ordering
  issues. Both were corrected and the bounded correction review found no remaining
  blocker. Normal native text propagation remains intact by source inspection.
- Real shared font catalog probe resolved all five supplied faces/weights exactly
  using a private process-local FONTCONFIG_FILE. Input hashes remained unchanged.
- At the source-only checkpoint, native interaction was still pending. The bounded
  Linux qualification below now covers ordinary interaction; actual marked IME
  and Windows remain unqualified. No full reference-project replacement is claimed.

Native acceptance must begin from blank: type Japanese/Korean/other-text lines,
select each range, choose Noto Light/A2Z Bold/an explicit Paperlogy weight, and
change size/fill. Inspect the actual mixed preview. Save full source and verify
exact run ranges/faces/values, local history, one document Undo/Redo, Cancel and
Save/Reopen. Check mixed-field no-op, click-away pending field, protected header
focus and typing/shortcut isolation. Marked IME and Windows require direct native
coverage before claiming those platforms/flows qualified. An application-local
private font collection is permitted for cloud QA; never bundle user font bytes.

## Bounded native qualification, 2026-10-06

Source c3858645 was built once with the canonical release profile and a verified
749-input fingerprint, ba7b66f8f08e29e4. From blank, native point text was authored
and its three nonempty language ranges formatted through Character. Exact saved
faces/weights, size, fill and untouched source passed an independently written
whole-project/CRC/numeric oracle. The native baseline is an identity snapshot;
its creation and action counts are established by the CUA observations.

The actual clipboard path yielded LF. Explicit 37-byte LF expectations use
ranges 0..15, 16..31 and 32..37, preserving inherited newline styles. This does
not establish native CRLF or marked-IME coverage. Japanese/Korean/Latin glyphs
were visibly inspected. Font Manager lists run-only faces and reports no missing
references; its detailed rich-run glyph analyzer remains incomplete.

Same-value size and reselected-family settings preserve exact face identity and
history. Local Undo/Redo, pending size/fill click-away, field Escape, selected
range preservation across Character collapse/reopen, Mixed summaries and visible
scrollable Done/Cancel controls pass. The tested header-focused shortcut sequence
did not delete/nudge layers or enter document Undo/Redo. Cancel discards style and text
changes while preserving an existing Redo branch. One document Undo restores
baseline PROJ bytes; Redo and Save/Reopen restore the complete applied file bytes.
Only the explicitly opened Character panel differs in VIEW; the initial strict
VIEW failure and the separate-view successful comparison are both retained.

Two strict-font CLI renders match a reference independently authored as three
ordinary text layers: 640×360, zero RGBA differences, 17,756 visible pixels and
all expected line colors. This tests rich-versus-ordinary composition using
shared font resolution/shaping/rasterization/compositing, not independent backend
correctness or After Effects parity. All private font files remain outside Git.

The single unchanged-budget model replay again passes 274/275, retaining the
existing 192-binding/100ms failure. No further retry, relaxed limit or full aggregate pass
is claimed. The inherited command-palette printable-input defect was observed
and classified by unchanged routing source; use the ordinary File menu for font
management. No additional production change or corrective release was needed.

Full evidence, relative to the repository root, is in
`../libreeffects-qa/native-selected-text-release-20261006/`.
