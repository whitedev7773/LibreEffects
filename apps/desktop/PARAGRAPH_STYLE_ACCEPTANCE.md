# Paragraph-style acceptance — 2026-10-04

## Scope and source pins

The [contract](PARAGRAPH_STYLE_PLAN.md) implements five layer-wide, static
paragraph geometry values: left/right and first-line indents, space before/after.
Shared flow supplies wrapping, alignment, paint/export, caret/hit/selection/IME
rectangles, Fit, overflow and current-frame Point conversion. It reuses existing
font shaping; no per-character style, justification or Text Animator is claimed.

Started at clean `9152683` on `codex/ae-workspace` in the existing dot cloud clone.
Local milestones: contract `5657a52`; shared geometry `65acd07`; core `25b7b41`;
independent acceptance `966e279`/`2975dfb`; UI `3ed4de2`; independent caret review
`c8ee004`; future-version test maintenance `64aedd2`; signed overflow-caret fix
and test wiring `f9e6e1b`. Core dedicated edits avoid generic schema/assets
recalculation. Zero fields are omitted; nonzero values require schema 55, including
dormant point-mode values and inactive compositions. LEP/VIEW/address versions stay.

Full source pin **f9e6e1bbb132f98f54d4b9730c51e2ab13cdeb4c** was clean before
final gates. Later acceptance/handoff edits are documentation only.

## Final gates and release

All passed using `/workspace/shared/libreeffects-dev-env.sh`, pinned Rust/Cargo
1.97.0 and serialized direct Cargo equivalents. Moon itself was not run.

- Formatting and workspace/all-target locked check
- **1,590 default tests:** 540 core + 1,044 desktop + six build helpers
- **32 explicit media tests**, excluding the device-clock and native verifier/
  explicit fixture-generator tests
- Vendored grid debug/release all-features and no-default-features check
- Normal optimized release build; only desktop check/test debug metadata was
  reduced, with test desktop debuginfo stripping. Release settings were unchanged.

Release: **20261004.171144-31d2c5616f09a6ec**, UTC 17:11:44, clean
`f9e6e1bbb132`, 367 watched source inputs, x86_64 Linux/release.
Size: **66,642,320 bytes**. SHA-256:
`a5885252889e147e8f0ff51639d8b15fd91b56143e1a0204e4c7e4a4a1ec37f7`.
Native About matched all displayed identity fields. Frozen release/test binaries,
source manifest and exact command logs are retained.

## Independent coverage

Forty new default tests comprise 12 core, 11 guarded UI/planner, ten independent
source/render/codec, five independent caret/overflow review and two direct flow/
navigation tests. The 56-test focused run also includes relevant older tests;
its count is not added again to the default total.

Independent literal positioned SVG text bypasses production wrapping, metrics
and text_svg, sharing the bundled Wanted Sans face and authoritative rasterizer.
**175 exact RGBA pairs / 21,504,000 pixels** cover hard/soft breaks, CRLF/blank/
trailing paragraphs, all alignments, hanging/clipped and exhausted intervals,
zero defaults/dormant point values, Fit/conversion and nine animation-boundary
frames. Complete source, schema, tracks, history, JSON, LEP and VIEW assertions
cover source preservation, failure atomicity and no-op Redo. Additional UI tests
exercise source/action/transport receipts, invalid/wrong/foreign fields, marked
source sessions, locks, modal states and one-use flush retirement.

Initial test compilation exposed private API access in the new test module,
recorded in `paragraph-focused.log`; editor-child placement and public JSON schema
reads fixed it. Review found caller-side width clamping could misalign overflow
carets. The final fix retains the true signed logical interval, while the metric
helper alone bounds its temporary SVG canvas; its independent regression passed.

Fourteen generated projects/28 immutable JSON+LEP fixtures are input and expected
source, never native evidence. Final CLI did **102 renders / 60 exact RGBA pairs /
7,372,800 pixels**, including 18 previous-release-versus-final legacy comparisons.
Those compare six zero-default point/paragraph/alignment fixtures at 0/30/60 with
the earlier frozen cross-path release, in addition to final JSON/LEP equivalence.

## Bounded native acceptance

All observations use the final frozen Linux release, one 1x view at frame 30 and
Latin source. Text values were transferred via GTK clipboard Copy and GPUI Ctrl+V;
this is not a real IME/keyboard-layout qualification. Screenshots were inspected
through supported CUA but were not archived as local image files.

1. Opened the independent baseline; all seven numeric rows and full help were
   readable after scrolling. Applied left 24, right 16, first-line 12, before 10
   and after 14. Five sequential actual saves match independent full source.
2. One Undo removed only after spacing. Typing `10.000` over before 10 made no
   history entry; Redo restored after 14. A pending right −1 followed by Center
   was rejected, restored authoritative 16, retained Left and left source clean.
   `06-noop-invalid.lep` is byte-identical to `05-after.lep`.
3. Pending before 20 followed by Center applied both. `07-pending-center.lep`
   matches a predeclared literal two-field expectation. Native overflow hid E;
   Fit changed displayed box height 260→286 and revealed E. Fit's intermediate
   state was visually observed, not separately saved or independently height-
   oracled. One Undo removed Fit, another Center, another before 20. Thus the
   pending field and following action had separate history transactions.
4. `08-restored.lep` is byte-identical to 05/06. Actual Open of 08 and Save As
   `09-reopened.lep` preserved all **2,042 bytes**. App remains clean on 09.

The first strict save check correctly failed only because the operator closed
Properties and opened Paragraph. `native-01-verify.log` preserves that failure.
Separate immutable `native-references/` explicitly change exactly those two
workspace booleans before subsequent checks. The builder reads only generated
expectations, never native output, retains every PROJ byte and all other VIEW
values, and does not mask comparisons. A separate literal pending-field reference
changes only before=20 and alignment=Center. Official codec verifiers decode all
references and compare entire source and VIEW.

All **nine actual saves** pass those strict full-source/VIEW verifiers and nine-
frame shared/preview/output/JSON/LEP checks: **405 exact RGBA pairs / 49,766,400
pixels**. Native final CLI adds **54 renders / 27 exact pairs / 3,317,760 pixels**.
Previous `cross-path-20261004/native/06-final-smoke.lep` remains SHA-exact; the
requested GitHub tab stays open. No push, PR, deploy or user-desktop work occurred.

## Remaining boundaries

Native unrun: hanging/exhausted-width edits, point-mode conversion/dormant restore,
animated text at different frames, caret/selection editing, locked targets, real
marked IME, stale/held/canceled pointer races, adversarial file/modal/playback
ordering, other fonts/scripts, platforms, DPI and devices. Headless tests are
separate evidence and do not close these native gaps or older milestones' gaps.

All seven Paragraph numbers now use guarded precise typed entry; drag scrubbing
was intentionally retired for the existing dimension fields too. Per-paragraph
or per-character styles, kerning/leading modes, justification and Text Animator
remain. F03 and the 77-ID backlog remain Partial. Next is a separate bounded
Text Animator/selector design with explicit grapheme/cluster semantics, not a
claim of complete shaping or complex-script per-character control.

QA root: `/workspace/shared/libreeffects-qa/paragraph-style-20261004`.
