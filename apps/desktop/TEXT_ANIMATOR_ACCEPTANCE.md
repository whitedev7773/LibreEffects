# Text Animator acceptance — 2026-10-04

## Scope and implementation

The [contract](TEXT_ANIMATOR_PLAN.md) adds one hard logical extended-grapheme
range with five scalar channels: Start/End, Position X/Y and Opacity. Source Text
is sampled before indexing, whitespace/newlines consume positions, and selection
expands transitively across actual shaping-cluster/grapheme overlaps. Shaping,
fallback and visual order remain authoritative. Animation changes rendered ink
after layout; caret, wrapping, alignment and Fit stay in source geometry.

Started at clean `96eafda` in the existing dot cloud clone on `codex/ae-workspace`.
Local commits: contract `c42f27b`; selector/build attribution `119efa7`; guarded UI
`68662de`; core `e789a6b`; Timeline correction `92b61b7`; source-preserving key
operations `720f291`; independent guards/wiring `731d967`; independent acceptance
`5f855a2`; renderer/vendor `7aa4fc5`; help `2c0365c`; retained upstream vendor
reference documents `c0e5d15`; older test expectation updates `00c0690`; final
narrow-sidebar action wrapping `284f8bc`.

Schema 56 is required for any materialized animator channel, including dormant
identity tracks. Old sparse source, LEP 1, VIEW 1/2 and address 1 remain. Dedicated
animator-only scalar/key/temporal edits validate original and candidate source,
retain assets and unrelated data, and preserve exact no-op Redo. Incompatible
old-schema media combinations reject atomically rather than migrating assets.

The pinned usvg 0.45.1 extension carries authoritative fallback-resolved source
cluster ranges and applies glyph effects through the existing outline, COLR,
embedded-SVG and bitmap paths. It does not use the editor's older heuristic
caret-mapping helper. The opt-in serializer preserves resource identity and exact
f32 coordinates; the ordinary serializer and no-effect renderer stay unchanged.
See [vendor provenance](../../vendor/usvg/README.libreeffects.md).

## Review findings and retained failures

Independent review found and fixed two integration defects before release:

- Generic Timeline ToggleKey could remove a current key just created by a pending
  Properties field. Animator Timeline now uses explicit guarded intents, rejects
  pending fields before blur, and shows read-only values with a Properties hint.
- Animator-only key/temporal commands originally used historical asset migration.
  Their dedicated route now validates original budgets and preserves source, with
  public-core regressions for old-schema media PasteKeys and exact key no-ops.

Pixel failures exposed a concrete serialization defect: upstream numeric output
multiplies/divides f32 values by decimal precision, changing some coordinates by
an ULP even at precision 12. Direct round-tripping f32 output in the new opt-in
writer removed stray pixels on an unchanged neighboring glyph and antialiasing
differences on a real ligature. Bit-pattern and complete glyph-coordinate
round-trip tests pin the correction; no pixel tolerance was introduced.

Two independent-reference corrections are separately retained. Point-text
literal baselines now use the original absolute SVG baseline instead of an
algebraically equivalent extra transform that changes floating-point association;
an old-path literal self-check includes the exact neighboring B case. A masked
translation reference originally moved the fixed path-mask domain with the layer.
Its corrected wide-domain comparison is paired with an explicit narrow-domain
transparent expectation, preserving production mask semantics.

The first aggregate run passed 558 core and 1,085 desktop tests but failed two
older test expectations: schema 56 was still labeled unsupported and Animator
units were treated as RGB. Test-only `00c0690` uses a genuinely future version and
an exhaustive unit match. The failed aggregate and all focused failures remain
under the QA root; the complete gates are rerun after that correction.

## Independent evidence

The independent default suite makes **435 exact RGBA comparisons / 53,452,800
pixels**: 220 literal SVG pairs, 145 legacy/static identity pairs, 45 independently
compensated pipeline references and 25 exact transparent outputs. These include
half-open center thresholds, empty/reversed/source-only selectors, CRLF/tabs,
combining sequences, decomposed Hangul, emoji ZWJ/flags/skin tone, actual bundled
multi-grapheme ligature closure, logical RTL order, paragraph clipping, Source Text
and numeric sampling boundaries, global paint order/opacity, transforms, masks,
mattes and unattenuated effect allocation. The suite compares shared, preview,
output, JSON and official LEP paths and complete source/history/VIEW.

Native inputs and independent expected source are immutable generated documents,
kept separately from actual UI saves. The native verifier accepts only explicit
frame overrides and compares full source and full VIEW without masking fields.
DejaVu Sans `ffi` has a separate opt-in Linux/system-font qualification; it is
excluded from generic cross-platform media gates. Eight vendor tests cover
authoritative ranges, exact identity, protected opacity, invalid units/effects,
duplicate resource IDs and exact coordinate serialization.

## Final gates, release and native qualification

Final source pin: `284f8bc56ed955a7649977f240d8eda23db77033`, 406 watched
inputs, source fingerprint `29eb0ac92839de08`. The complete repeated gate passed
**1,651 default tests** (558 core + 1,087 desktop + six build helpers), **32 explicit
media tests**, workspace/all-target and formatting checks, grid debug/release/
no-default-feature gates, eight usvg tests, usvg formatting and no-text-feature
check. The explicit DejaVu Sans `ffi` test passed another **20 exact RGBA pairs /
2,457,600 pixels**. Direct Cargo equivalents used pinned Rust/Cargo 1.97.0;
Moon was not run. Only desktop check/test debug metadata was reduced; normal
release settings were retained.

Final normal release: **20261004.184033-29eb0ac92839de08**, UTC 18:40:33,
clean `284f8bc56ed9`, x86_64 Linux/release, **66,901,800 bytes**, SHA-256
`a17d83d6425deed0d2a32bae4844d311e5a09484da52e1b69a09815b4b76d304`.
Native About matched. Frozen final release/test binaries, source manifests and all
gate commands/logs are in `wrap-final/` under the QA root. Later handoff changes
are documentation only.

Sixteen generated projects/32 immutable JSON+LEP files are independent inputs and
expectations, not native interaction evidence. Final CLI: **114 renders / 66
exact RGBA pairs / 8,110,080 pixels**, including 18 old/new legacy comparisons
against the frozen paragraph-style release. Final-source re-export and hashes
retain every generated input byte.

### Bounded native sequence and exact attribution

The first eleven actual saves were recorded on clean `00c0690`, release
**20261004.181317-7ad3423909dcc4c7**. At the supported 1180×812 window size,
the long Disable/delete-keys and Remove-key buttons slightly overflowed their
row. The only subsequent source change is one `.flex_wrap()` call in `284f8bc`;
the exact one-line diff is retained. Both pins separately passed the complete
gates and normal release. Earlier interactions are not relabeled as reruns.

1. Opened the generated point-text baseline at frame 30. Cumulative Start 40,
   End 60, X 35, Y 9 and Opacity 55 each received a separate actual save. Only B
   moved/faded; A and C stayed fixed. Saves 01–05 match complete independent source
   and the unchanged VIEW.
2. One Undo restored only Opacity 100. Typing unchanged Y `9.000` preserved Redo;
   Redo restored Opacity 55. Pending Opacity 101 followed by Enable was rejected,
   restored 55, and did not enable animation. Saves 05/06 are byte-identical.
3. Enable at frame 30 stored Opacity 55. Seeking frame 60 and typing 0 stored the
   second key. At frame 45 the field displayed 27.5. Unchanged `27.500` created
   no key. Disable removed all Opacity keys and retained 27.5; one Undo restored
   exactly the original frame-30/frame-60 keys. Saves 07–10 match declared source.
4. Actual Open of save 10 followed by Save As 11 preserved all 2,197 bytes.
5. Final `284f8bc` About matched. Opening 11 and saving 12 preserved those same
   bytes. Both wrapped actions were fully visible. At frame 30, Remove key made
   B disappear and displayed 0 from the remaining frame-60 key; one Undo restored
   55 and both original keys. Save 13 matches the original keyed source at frame
   30. The intermediate removed-key state was visually observed, not separately
   saved/source-oracled.
6. New project visibly cleared the workspace. Actual Open of 13 and Save As 14
   preserved all 2,197 bytes. The final app is clean on **14-final-reopened.lep**
   at frame 30, with Opacity 55 and the wrapped controls visible.

All **14 actual saves** pass the final frozen verifier's complete source/VIEW
comparison and nine-frame checks: **630 exact RGBA pairs / 77,414,400 pixels**.
Only predeclared frame overrides are used; no source or VIEW fields are masked.
Final native CLI: **84 renders / 42 exact pairs / 5,160,960 pixels**. 10/11/12
and 13/14 are respectively byte-identical. The prior paragraph `09-reopened.lep`
retains its SHA-256 and the requested GitHub tab remains open.

Native input used GTK Copy plus GPUI Ctrl+V, not real marked IME. The shell
executor lacked GUI/runtime context, so the frozen binary was launched through
the actual cloud desktop Terminal. Bound keyboard worked; full-desktop pointer
control was used after bound pointer actions had no effect. Window size was
briefly restored and returned before editing; all accepted VIEW comparisons stay
exact. Screenshots were inspected through supported CUA, not archived as files.

## Boundaries

This remains an F03 foundation, not complete Text Animator or Adobe parity.
Multiple animators/selectors, soft/offset/random/word/line selectors, rotation,
scale and per-character styles remain. Native Timeline/Graph key editing and
pending-field rejection, Add-key intent, negative/fractional position and reversed
ranges, source-text-animation editing, real marked IME, adversarial held/stale
event ordering and full script/font/color-font-format/platform/DPI/device
matrices remain unqualified beyond the exact sequence above. Retaining the
upstream glyph branches does not certify every font's output or coverage.

An attenuated protected unit split across visual spans/runs fails explicitly;
unsupported normalization/mapping also fails rather than guessing. A conservative
preflight and exact output check bound expanded SVG at 64 MiB, so a legal source
string can still be rejected if glyph geometry/bitmap expansion is too large.
Path-mask domains stay at the original source rectangle, including point text.

QA root: `/workspace/shared/libreeffects-qa/text-animator-20261004`.
No push, PR, deployment or user-desktop work belongs to this milestone.
