# Static SVG import qualification

## Bounded feature and source contract

This report concerns E06's first static editable-import slice, not general SVG
support. Read [the contract](SVG_IMPORT_PLAN.md) and [user guidance](README.md#import-svg-as-editable-shapes).
The importer accepts common static geometry, groups/transforms, solid paint,
opacity, winding/stroke styles and root viewBox placement. Unsupported resources,
CSS, text, gradients, filters/masks, animation, scripts and foreign content reject
the entire import. Integer-pixel viewports only; no new project schema is needed.

The project is the dot-cloud clone on `codex/ae-workspace`, starting clean at
`c1b8ea3`. All changes are local commits. No push, PR, deployment or user-desktop
work is part of this milestone. Previous native project files are immutable.

## Independent evidence policy

Raw SVG fixtures are authored independently and retained unchanged. Native paths
are cubic-only, so exact raw-SVG pixel identity is not claimed. Each accepted
fixture also has a separately authored cubic-equivalent SVG reference, based on
literal endpoint/control plans without calling the importer or native path model.
All renderer/JSON/LEP/CLI routes must match that reference exactly. Raw-source
comparisons cover every RGBA pixel, record exact changed-pixel counts and maximum
channel differences, and require every changed pixel to lie at a locally varying
edge. No masks or ignored image regions are used.

The first candidate used zero-handle cubics for straight segments. Independent
raw-source comparison found 142 changed dashed-stroke pixels (maximum channel
change 138), a meaningful dash-placement error. Line controls now lie at one-third
and two-thirds of each segment, preserving linear parameterization. This fixed
that dashed witness exactly. Separate small edge-only changes remain because the
pinned rasterizer uses different line/quadratic/cubic stroke and antialias paths.

The fractional-viewport witness changed 399 pixels with maximum channel delta 191:
current native mask coverage was applied repeatedly at the fractional boundary.
Fractional dimensions now reject explicitly, including near-integer values; they
are never rounded. The original fixture remains a rejection regression.

Strict-input review additionally identified permissive pinned-parser token
handling, repeated-close recursion before the expanded-path budget, tiny arc
inputs reaching underflow arithmetic, and overwritten move-only subpaths.
Preflight now rejects those before the geometry-only normalizer. DTD/resources/
scripts never reach it. File reads are bounded and refuse symlinks/nonregular
files; Unix opening uses no-follow/nonblocking flags and handle validation.
Windows reparse guards are implemented but require platform qualification.

## Transaction and UI boundaries

Import is a dedicated source-preserving core transaction with fresh layer-local
Contents/mask IDs, a new layer at composition origin and one Undo. Original and
candidate budgets are validated; source assets/inactive compositions are not
silently migrated. Incompatible old schema/assets reject. A transient core epoch
and UI source/selection/input/transport/modal receipts reject stale or ABA results.
Cancel, failures and exact no-ops preserve Redo.

General File-menu/command-search opening retains its existing independent field
commit behavior. An SVG-only pre-entry latch prevents an entry that began with a
pending source draft from starting import. Thus the source/history preservation
promise starts at the import transaction, not before general menu opening.

## Final gates and native recording

Final code **e078b4fa7bcc611bbe24d72cc955684372bc0ca6** passes all 12 gates:
**1,781 default tests** (597 core + 1,178 desktop + six build-identity), **32 media**,
workspace/all-target checks, format, grid debug/release/no-default variants, usvg
format/default/no-text, explicit DejaVu ffi and normal optimized release. Direct
Cargo equivalents ran, not Moon. Only desktop debug/test metadata was reduced and
test debuginfo stripped; release settings remained normal. The independent
worker's two preliminary focused runs used a previously cached global debug=0
profile; those are separate from this final pinned-profile aggregate.

Build **20261004.214034-604994e339acebef**, 444 watched source inputs, target
`x86_64-unknown-linux-gnu`, 67,225,664 bytes. Frozen binary SHA-256:
`bef3ab68d8a7c0f871337215462da637f155e5f2d1ad4099a3f0f7961f24baea`.
The source snapshot, all gate logs, normal optimized binary and 119,827,496-byte
pinned test binary are under the external QA root's `final/` directory.

The first aggregate on `6f59c48` passed 597 core /1,177 desktop tests but failed
an existing compound-Colors source-order guard: the new rejected-import focus
return appeared before the shell modal-retirement block. Commit `e078b4f` moves
the existing retirement block ahead of that guard, retaining the established
covered-modal ordering. All final gates above reran. `first-aggregate/` retains
the actual failure/source snapshot; it is not counted as a final pass.

Independent acceptance makes **198 exact RGBA pairs /7,603,200 pixels** across
11 cubic-reference fixtures, 18 viewBox alignments and the safe-metadata witness.
Raw original fixtures remain byte-identical. Changed pixels/max-channel deltas
at 240×160 are: primitives 4/16, curves 6/1, compound winding 5/16, nested affine
4/16, slice 1/9; the other six fixtures are exact. Two of 18 alignment witnesses
have 1/16; the rest and safe metadata are exact. These are exact fixture-specific
characterizations, not general tolerances. All changed pixels pass the local-edge
witness and every RGBA sample participates.

Final generated CLI: **132 renders /165 exact RGBA pairs /6,336,000 pixels**,
including 66 prior/current comparisons using the preceding compound-Colors
release. The old release can read/render the same schema 44 imports. All **69
generated files** and both binary hashes remain unchanged. JSON/LEP/reference
checks use all three frames 0/30/60 and full RGBA, without masks. Source metadata
and literal/cubic references are distinguished explicitly in the manifest.

## Actual native qualification on the final build

About in the running dot-cloud editor matched **e078b4f** and build
**20261004.214034-604994e339acebef**. All eight actual saves use this same frozen
binary, frame 0, with 240×160 /30fps /90-frame composition. Input copies are
regular files whose bytes match the immutable generated/original references.
The native working directory is `/workspace/shared/svgqa`; verified byte-for-byte
copies are archived under the QA root's `final/native/`. The initial `/tmp` input
copy was not visible to the desktop file chooser, so shared working copies were
used. No saved result was fabricated from a generated fixture.

1. `01-primitives-import.lep`: actual Open blank → File-menu SVG chooser → import
   primitives → Save As. One layer at origin, geometry and normalization notice
   were visible.
2. `02-one-undo-blank.lep`: exactly one Edit-menu Undo removed that imported layer;
   the actual blank state was saved. A Ctrl+Z pressed while the menu was open was
   consumed by the menu; only the subsequent menu Undo is qualified here.
3. `03-rejected-blank.lep`: selected unsupported text SVG while Redo existed.
   Explicit `<text>` error, no added layer/dirty source, then actual Save As.
4. `04-cancelled-blank.lep`: F10/keyboard File navigation opened the SVG chooser;
   Escape canceled, with explicit canceled status and unchanged source, then save.
5. `05-redo-primitives.lep`: one keyboard Ctrl+Shift+Z after rejection/cancel
   restored the imported layer; actual Save As.
6. `06-fractional-rejected.lep`: original fractional SVG was rejected with the
   integer-viewport explanation; imported source stayed unchanged, then save.
7. `07-search-stroke-styles.lep`: fresh blank → Ctrl+Shift+P → click the SVG result
   in the unfiltered command list → chooser → stroke/dash fixture → actual save.
   Synthetic character-key input did not populate the GPUI query; typed-query
   filtering is not claimed by this native case.
8. `08-final-reopened.lep`: actual New (fresh Untitled observed) → Open actual 07
   → Save As 08. Final app is clean on 08, frame 0, with the dash fixture visible.

All eight actual files pass **exact full source and VIEW** against the independent
expected projects. Three frames ×five renderer/codec routes ×eight saves give
**120 exact RGBA pairs /4,608,000 pixels**. Native release CLI gives **48 renders /
48 exact RGBA pairs /1,843,200 pixels**, including previous/current comparisons.
Combined generated/native CLI totals are **180 renders /213 exact RGBA pairs /
8,179,200 pixels**. No ignored pixel regions or undocumented view overrides.

Byte identities: 01/05/06 each 16,482 bytes with SHA-256
`e38a6eaf8cb8e834155da83fb264877cda83516741f7470e1eae3a2cd0d36178`;
02/03/04 each 944 bytes with
`e71207dfab38afc77680a02c6fe66f4a77311db76230200657b0fc5c8c0d6e9d`;
07/08 each 9,157 bytes with
`9f7754d5d3d507e8551135d68c444f023e879e5527833b98d35e13e81652a468`.
The earlier compound-Colors `10-final-reopened90.lep`, interpolation
`12-final-reopened.lep` and animator `14-final-reopened.lep` remain hash-exact.
GitHub remains open; no user desktop, lock deletion or alternate runtime profile
was used. Screenshots were inspected, not archived.

Boundaries: the eight ordinary routes do not qualify direct path/paint editing
of imported geometry, pending-draft entry gestures, adversarial stale-result
races, real IME/held input, every transform/style/clip combination, other scales,
fonts/devices/platforms or source-SVG round trips. Headless receipts/negative
fixtures remain distinct evidence. The cloud input surface lacked a Paste
action, so native GTK paths used supported discrete keys; this is not evidence
of normal typing/IME behavior. General pre-menu draft commits retain their old
contract as explained above.

E06 remains Partial. CSS/exporter metadata compatibility, gradients, resources,
text, images, use/defs, filters/clips/masks, fractional viewports, animation,
round-trip source preservation and per-segment SVG rendering provenance remain
outside this slice. Broader IME, adversarial timing, held input, Windows/macOS,
DPI/device coverage and earlier roadmap/native gaps remain separate.
