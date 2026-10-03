# Current desktop implementation status

Updated 2026-10-03 for the resumed-work, I01 output-preflight and
E04/E02 editing, D03 time-box, animated path-order, native LEP, Pen marquee and
Contents sibling-tree, layer-wide text-paint, numeric-vertex and font-diagnostic checkpoints.
This is the current checkpoint inventory;
[DEVELOPMENT_BACKLOG.md](DEVELOPMENT_BACKLOG.md) retains the original audit and
its dated implementation/test history. Older “remaining” lists and test counts
there describe their own checkpoints, not the latest source.

Libre Effects is an **early 2D motion-graphics and compositing editor**. It is not
an After Effects replacement, an AE pixel-equivalent renderer, or an AEP/Adobe
script-compatible application. A working control or passing model test alone
does not establish a complete native editing workflow.

## Restored work and current verification

- **Native project format:** `.lep` (Libre Effects Project) uses a documented
  version-1 binary container with a signature, bounded typed chunks and CRC32.
  Project metadata, optional editor views and shared embedded PNG bytes are
  separate chunks; linked videos, audio and sequences remain external files.
  Save, Save As, Collect Files, CLI and current/previous recovery use the same
  codec. Content-based legacy `.lfe.json` import remains available; Save creates
  a native copy, retaining imported-source protection through Save As and queued
  exports. Unsupported versions, malformed or corrupt files fail before changing
  the live document or replacing outputs. Async Open/Save callbacks cannot install
  stale documents or provenance. Recovery first durably saves the restored
  candidate, then reports cleanup failures without discarding that restore.
  CRC32 detects corruption, not malicious tampering. See the
  [format specification](../../docs/lep-format-v1.md).
- **Gradient editing:** Properties color/opacity stops and midpoint drags now
  preview their draft in both the ramp and Composition. The source document,
  autosave, export and history remain unchanged until release; release commits
  one Undo step. Preview requests coalesce and preserve the last valid frame;
  cancellation and generation checks reject stale results. Selected Contents
  Gradient Fill/Stroke Start/End handles work through nested group and layer
  transforms, with axis-constrained or paired endpoint movement. A modal Gradient
  Editor now isolates multi-control color/opacity-stop edits until one-Undo OK or
  Cancel; invalid fields block acceptance, field Escape reverts only that field,
  and no-op transactions preserve animation and history. Compound Colors/topology
  animation and AE UI/pixel equivalence remain open.
- **Pen multi-vertex editing:** Shift-click selects vertices on one legacy shape,
  mask or Contents path. Group moves preserve relative positions and tangents;
  static multi-delete is atomic and enforces minimum path sizes. Selection-only
  and no-net-change gestures preserve keys/history. Pending insertion interruption,
  final mouse-up coordinates/modifiers and overlay focus have regression coverage.
  Additive Shift blank-drag now boxes anchor centers on the known path, and exact
  canvas Ctrl+A selects all of that path's vertices. Empty retained targets remain
  usable, but no path is guessed from a layer or Group selection. Held view/layout
  changes cancel unpublished geometry/box edits before converting coordinates;
  idle zoom between creation points remains supported. Cross-path selection,
  bulk numeric transforms and topology-changing animation remain open.
- **Whole-track path order:** Reverse Direction and closed-path Set First Vertex
  reorder base geometry and every stored pose together without changing timing,
  pose references, transforms or paint settings. Canvas shortcuts remap the same
  selected geometric points; a separate outline marks the first point. Pure
  nonempty reorder batches preserve legacy version/assets and exact no-op history.
  Cubic geometry is preserved, but winding/Non-Zero holes and dash placement can
  intentionally change. Cross-path and bulk numeric vertex editing remain open.
- **Numeric vertex transaction:** an explicit idle single Pen vertex opens six
  full-precision local-coordinate fields through Edit Vertex or canvas Shift+V.
  Anchor coordinates and independent tangent offsets work on Shape, enabled
  Contents paths and vector Masks, including evaluated animation. Draft geometry
  and transformed overlays remain isolated from source/history/normal Save;
  acceptance executes one EditPath. Exact no-op/Cancel preserves keys and Redo.
  Existing-key interpolation survives; new keys use Linear. Frozen contexts,
  serial callbacks, per-field errors, replacement/late-I/O guards and validated
  one-shot selection return bound the modal. Mouse-release interception found in
  native v1 was fixed in Preview; the corrected-release mouse checks passed.
  No schema/container change or core command was needed. Multi-vertex numeric
  transforms, linked tangents and topology changes remain outside this slice.
- **Selected-group Pen creation:** new open/closed paths can be drawn directly
  into the explicitly selected Contents Group, including empty/nested transformed
  groups. Existing applicable paints are inherited without paint additions; the
  Group remains selected for repeated drawing. Invalid targets/limits reject
  atomically. Existing hit precedence, Ctrl-mask and standalone fallback remain.
- **Contents sibling tree:** plain/Ctrl/Shift selection, sibling Ctrl+A/range
  navigation and same-parent block dragging preserve selected/unselected relative
  order. Groups move as whole subtrees; no hover edit or implicit reparent occurs.
  Complete permutations preserve IDs, parameters, keys, poses, references and
  historical schema/assets. Stale Project/context, hidden rows and invalid drops
  cancel safely; unchanged gaps preserve history. Singleton controls and Pen/
  gradient targets remain separate from multi-selection. Tree-owned and
  unsupported selection-edit shortcuts cannot fall through to layers, including
  during playback. Direct native Save retains focus; genuine focus loss has a
  muted retained-selection cue. Paint scope/overlap can intentionally change with
  order. Cross-parent block moves, bulk fields and compound Colors remain open.
- **Speed Graph time box:** two horizontal handles scale selected key times while
  preserving values and temporal metadata through the existing core command.
  Finite signed endpoint speeds place the box; snapping is time-only and drafts
  freeze source/view/selection. Offscreen pivots, tool switches, final release,
  no-op history and glyph-clear boundary placement have regression coverage.
  Vertical velocity scaling and simultaneous multi-channel graphs remain open.
- **Animated text paint:** seven sparse tracks animate layer-wide Fill RGB,
  Stroke RGB and Stroke Width through Character/Properties, Timeline and the
  shared Value/Speed Graph. Grouped color edits are one Undo; unchanged HEX/picker
  input and Cancel preserve keys/history. Static typography and Source Text edits
  retain the separate paint tracks. Per-field source snapshots reject stale
  drafts without cloning the project on every unchanged render. Rendering uses
  the same sampled paint for text and effect bounds; outward-rounded text filter
  extents prevent a fractional antialiased edge from being clipped. Project
  schema 48 is required only when text tracks exist; LEP remains container v1.
  Typography/source/per-character animation and animated switches/order remain
  outside this slice.
- **Actual text-font diagnostics:** Manage project fonts has an explicit
  read-only background glyph check. It separates requested/primary identity from
  final positioned-glyph faces using the parsed tree's font database, including
  whole-run fallback. Glyph ID 0, bounded cluster samples, paragraph overflow,
  empty/incomplete states and unexamined layers are explicit. Checks include
  hidden/locked/paint-disabled layers, report at most 256 layers and bound each
  layer's source/lines/glyphs/faces/samples. Source snapshots, serials and a single
  retained worker slot reject stale results and overlapping checks; cancellation
  is cooperative between layers. Existing replacement locks/one-Undo semantics,
  layout, rendering, schema and strict-export policy are unchanged. These are
  shaping diagnostics, not semantic emoji/color-font assurance or a total
  process-memory bound. Variable axes and cross-machine qualification remain.
- **Project save budget:** a bounded streaming JSON counter checks each final
  atomic edit candidate before it enters history. It accounts for escaped
  metadata and shared image/sequence references without constructing a second
  JSON buffer. Over-budget edits leave the last good document, selection and
  Undo/Redo intact. The limits remain 16 MiB compact metadata, 128 MiB embedded
  image payloads and a 256 MiB project/recovery envelope. Exact-limit saves,
  atomic batches, inactive compositions, duplication, replacement/load and
  escaped/reference accounting have regression coverage. This closes the
  previously missing ordinary-edit metadata preflight; external asset packages
  and a total process/Undo-memory budget are separate work. Desktop Open/Recover
  now validates the replacement before modifying the live document, save path,
  history or recovery files. Four direct state-preservation regressions cover
  relative-path expansion and invalid recovery candidates.
- **Output path protection:** canonical-parent alias comparison now covers
  destinations that do not yet exist, with alias and offline-source regression
  cases. The previous Windows CI failure at `52ab32d` was addressed locally.
  This checkpoint is local-commit-only: no push or PR was requested, so fresh
  remote Windows CI has not run and is not scheduled by this checkpoint.
- **Dependency maintenance:** the vendored `grid` 0.18 compatibility backport
  uses upstream checked dimension arithmetic. The application still has the
  GPUI/Taffy-compatible API. See [backport provenance and removal criteria](../../vendor/grid/README.libreeffects.md).
  Version-only advisory scanners may continue to flag the version; the advisory
  is not globally suppressed. This is a tested source backport, not a claim that
  upstream released a patched 0.18 version or that every dependency is audited.
- **Output preflight (I01):** CLI, native PNG/video exports and the render queue
  share structured source/settings/destination diagnostics. Sibling-file write
  probing preserves existing outputs. One bounded, cancelable FFmpeg capability
  encode checks the selected video/audio configuration before rendering frames.
  Persisted Fonts fallback/strict policy reports substitutions or blocks them;
  legacy settings retain fallback behavior. Version 5 queue envelopes preserve
  both font policy and imported-source protection; versions 1–4 migrate without
  discarding explicit policies. Queue snapshots remain JSON. Source scans also
  honor cancellation.
  This does not guarantee future free disk space or audit per-glyph font coverage.
- **Reproducible example:** [gradient-study.lfe.json](../../examples/gradient-study.lfe.json)
  is generated by [make_gradient_study.rs](../../crates/core/examples/make_gradient_study.rs).
  Generation and a 640×360 PNG render through the final release CLI succeeded.
  This generated fixture is distinct from a project created through native UI.
  [native-project-study.lep](../../examples/native-project-study.lep), generated
  by [make_native_study.rs](../../crates/core/examples/make_native_study.rs), adds
  one embedded PNG shared by two layers, animated rotation and a frame-30 view.

### Verification record for this checkpoint

The combined checkpoint includes all prior restoration/I01/Gradient/Pen fixes,
selected-group creation, Speed Graph time scaling, whole-track path ordering and
native `.lep` storage integration, same-path marquee/Ctrl+A, the sibling tree
seven-channel text paint, the numeric vertex transaction and actual font diagnostics.
Formatting, type checking, all 792 default
tests, the explicit 30-test media suite
and the optimized release build passed. Native checks are listed separately with
their tested build and coverage; automated tests do not imply native acceptance.

| Check | Result and boundary |
| --- | --- |
| Core tests | 251 passed, including 13 text-paint track/lifecycle/legacy/no-op/budget regressions, 11 Contents permutation/preservation regressions, 16 codec and one native-budget regression plus prior save-budget, group-space and path-order tests. |
| Desktop default tests on Linux | 541 passed. Font diagnostics adds 25 (9 final-glyph/fixture tests and 16 lifecycle/snapshot tests). The numeric milestone adds 41: 14 session, 14 Pen/Preview including the two pointer-release regressions, 4 UI/routing, 4 file-I/O, 1 tree-modal and 4 independent render tests. Earlier text/tree/native-file/marquee/editing/focus/I01 tests remain included; 30 FFmpeg tests are ignored by default. |
| Explicit FFmpeg integration suite | All 30 ignored media tests were explicitly run and passed. They are not included in the default pass count. |
| Focused editing regressions | 16 modal-transaction tests, 89 Pen tests (including 26 marquee, 8 frozen-view, 14 group-creation and 14 path-order tests), 3 Preview tests (one new view snapshot), 17 Speed-box tests, 5 path-order rendering tests, 3 shell-focus tests and the prior 20 gradient-draft/endpoint tests passed within the desktop suite; do not add these counts a second time. |
| Release build | `cargo build -p libre-effects-desktop --release --locked` succeeded after the final review fixes. The earlier interrupted-build blocker is closed. |
| Task runner boundary | Direct pinned `cargo check -p libre-effects-desktop --locked` and `cargo fmt --all --check` pass. This checkpoint's Moon invocation could not set up its proto toolchain; the no-actions retry failed in its plugin with `entity not found`. These wrapper attempts are not recorded as passes. No application dependency or toolchain version was changed to work around them. |
| Native/legacy CLI equivalence | The pinned LEP release rendered both formats at frames 0, 30 and 60 with exact equality across 691,200 RGBA pixels. Future-version, corrupt-image and truncated inputs exited 1 while preserving existing output bytes. Linked-media native rendering also passed. |
| Text/diagnostic release pixels | The final font-diagnostic pin, as well as the earlier text and numeric pins, preserves all 27 legacy text-example frame outputs (9 projects × 0/30/60) from the pre-change release. Five new animated native frames (0/15/30/45/60) exactly match independently constructed static documents rendered by both releases. All 28,440,800 RGBA pixels match; original fixture/source hashes remain unchanged. The deliberately restored fractional filter-edge pixel is covered separately by the exact identity-effect unit regression. |
| Native LEP workflow | Pinned release passed legacy import/native-copy Save with unchanged originals, default .lep names, shared images/view state, edited Save/reopen, rejected Open with state/history preserved, normalized-path collision rejection and imported-original PNG protection. Actual five-second autosave, legacy JSON recovery and corrupt-current/valid-previous fallback each passed Restore/Save/reopen with exact project/image data. Collect Files produced project.lep plus Media; after moving the entire new folder, native Open retained frame 1/100% view and Manage project media showed the moved video Online with 0 missing. CLI rendering matched all 57,600 RGBA pixels before/after collection. Native queue-restart/video-export protection, Windows, OS association, IME and DPI were not exercised in this LEP session; queue/video guard logic is regression-tested. |
| Vendored grid | Debug and release each passed 215 unit tests and 43 doc tests with all features. |
| Web/API Moon CI targets | `web:build`, `web:test` and `api:build` passed locally; web has 3 passing tests. API validation is a deployment dry run, not a deployment. |
| Native Linux smoke check | Launched the app; created a composition, rectangle and Contents group; saved a 7,937-byte project through a real native Save dialog. The final release also replaced the earlier editor, leaving one work window. |
| Native gradient acceptance, release checkpoint | Actual endpoint drags, single-Undo restoration, color-picker Cancel restoration, OK apply and one Undo passed. Save/reopen preserved the changed endpoint in a 12,346-byte native project; a 1364×1024 screenshot records the result. Mid-mouse-held frame observation and Escape during an active drag were not verified because the input tool executes drags atomically. These checks do not establish complete stop/midpoint or keyboard coverage. CLI output from the native saved file matched an independent linear-gradient calculation at 2,244 interior pixels with maximum RGB error below 0.50/255 and opaque alpha. |
| Native I01 output preflight | Strict rejection preserved the existing destination; policy and diagnostic survived restart. Fallback produced H.264 320×180 at24fps with exactly two frames and an explicit substitution warning. A renamed output folder produced an actionable missing-parent diagnostic and no output. These UI checks used the initial I01 build; the subsequent v4 envelope guard and migrations passed the final fresh-target tests. |
| Native modal editor | Initial debug build passed stop/midpoint/RGB/opacity drafts, Cancel, HEX validation, field Escape, multi-change OK and exact one-Undo/Redo. Edit-away-and-back was a no-op preserving prior history. Final release reopened the saved result; its CLI output matched independent color/opacity midpoint math at 2,244 interior pixels with maximum RGB error 1.47/255. No actual AE comparison was performed. |
| Native Pen and overlay focus, final release | Same-path pair drag passed for legacy Shape, rotated/skewed nested Contents and vector mask; saved JSON confirmed identical local deltas, unchanged unselected positions and all tangent offsets. Selection-only was clean. Static six-to-four deletion, closed-path minimum-size rejection, animated topology rejection with both keys preserved, exact one-Undo/Redo and static save/reopen passed. Ctrl+K/Ctrl+N moved focus off the canvas, preventing Delete behind Settings; field input focus and field/dialog Escape passed. Focus-changing file actions cleared transient Pen selection in those checks. Atomic drag input still prevents intermediate mouse-held observation. |
| Native selected-group Pen / Speed time box | Optimized release passed nested closed/open curves, repeated group creation, Cancel, one-Undo/Redo, save/reopen and Ctrl-mask/standalone controls. Existing path/paint JSON remained exact; independent group-transform reconstruction matched actual click coordinates within 0.223 pixels per axis. Speed showed only two side handles; vertical-only and return-to-start drags were no-ops, horizontal scaling preserved values/influences with common nominal slope compensation, and one-Undo/Redo/reopen matched saved data. The inset frame-zero handle was visible and draggable without hiding the key glyph. 26 post-hoc assertions and 15 native screenshots record these flows. Alt-centered/differential snap, held-drag interruption, tiny plots and full layout/DPI coverage remain unverified natively. Extremely short plots may omit a handle if no glyph-clear position fits. |
| Native whole-track path order | Corrected release passed First/Reverse geometry and tangent permutations, independent first-point marker, index-zero no-op history, exact Undo/Redo, remapped pair drag, whole animated base/pose/timing preservation, Properties Reverse, save/reopen at frame 60, and nested/mask routes. Multiple-selection First and unfinished-draft shortcuts were safely rejected. 25 post-hoc assertions and 16 original screenshots record the tested flows; four original fixture hashes were unchanged. Native open-path, locked/stale/modifier/IME and held-input combinations remain unverified after the requested format-priority change. |
| Native Pen marquee/Ctrl+A | Separately pinned release passed static additive box selection, Ctrl+A whole-path movement, nested rotated/skewed Contents and mask targeting, including box selection from an empty retained mask target. Only intended anchor positions changed by equal local deltas; other vertices, tangents and metadata stayed exact. One-step Undo/Redo matched baseline/results. No-target and selection-only gestures changed no source; animated box/Ctrl+A at frame 30 preserved every pose and timing key without adding keys. Static .lep geometry was saved and actually reopened. A first tightly batched selection/save/drag attempt moved one point; it was undone and the separately observed gesture passed. No general Save-focus guarantee is inferred from that input-timing observation. Held-drag observation/interruption, broader focus/IME, Windows and DPI combinations remain unverified natively. |
| Native Contents sibling tree | Candidate v2 passed root/nested block moves, nonnumeric visual order, expanded/collapsed subtree targets, rejected/no-op drops, one Undo/Redo, singleton/multi controls, clipboard/trim routing, text-field input, Pen targeting and native save/reopen. 17 recorded cases (including the discovered focus issue) and 26 file assertions preserve build attribution. QA found direct Ctrl+S could blur an apparently active tree before a subsequent Delete; the final tested v3 corrects direct-save focus and distinguishes inactive selection. Final v3 passed the exact row→duplicate→direct Save→Undo→Delete regression with one layer retained, blue active focus through direct Save, gray inactive focus after Save As and explicit refocus. During actual five-minute-fixture playback, owned Delete/arrows/Ctrl+D stayed consumed while frames advanced; stopped/save/reopen PROJ and saved frame 2466 remained exact. V2 functional cases and v3 focus/playback checks retain their separate build attribution. Held-drag intermediate observation, real IME, Windows/DPI and the wider interruption/gradient-control matrix remain unverified natively. |
| Native text-paint editing | Final pinned release passed all 10 bounded cases: exact 0/30/60 Character samples; current-frame Character/Inspector RGB and Stroke picker edits; unchanged OK/changed Cancel followed by one Undo; grouped Fill animation off retaining its frame-30 sample and one Undo; seven Timeline channels; RGB/width Value and Speed units and representative endpoint drags; source editing at frame 30 preserving all paint tracks; native Save/reopen with exact PROJ and VIEW. All 25 post-hoc assertions passed, 13 native files passed CRC and official codec roundtrips, and 24 settled CUA screenshots were archived. Original fixture and pinned binary hashes stayed exact. Hidden stale-draft retargeting, real IME, every temporal/lock/modal combination, held-drag intermediate states, Windows and DPI remain unverified natively; model guards and exact release pixels are separate evidence. |
| Numeric vertex render/legacy checks | Independent expected geometry matches isolated draft, committed preview/output and native roundtrip for parented/reflected Shape, rotated/skewed/reflected Contents and Add/Subtract masks, static/between-key/existing-eased-key cases. Exact return-to-opening creates no middle key. The final v2 release also retains the 32 legacy/text CLI frame cases and all 28,440,800 RGBA pixels from the earlier baseline. |
| Native numeric vertex, preliminary v1 | Keyboard acceptance passed six independent fields/live preview, unchanged and away-back transactions, Cancel/invalid-field Escape and one Undo/Redo, transformed local coordinates, mask tangent editing, between-key insertion and existing eased-key replacement, and exact native save/reopen. 29 assertions, 22 native files and 37 CUA screenshots retain v1 attribution. Actual mouse OK/Cancel only focused buttons: Preview's occluded outside mouse-up capture handler intercepted the release. V2 removes that interception while safely abandoning canvas gestures; 50 focused vertex and 28 Preview tests pass. Final native v2 passed standalone mouse Cancel/OK, drag-out cancellation, invalid OK/field Escape, exact Undo/Redo and reopen, deactivation preserving Redo, and shared Gradient Add/Cancel/OK with one Undo. V2 has 17 exact assertions, 12 codec-validated native files and 20 CUA screenshots. Its narrower correction gate stays separate from the broader v1 target/animation cases. Real IME, direct digit text entry, held intermediate states, Windows/DPI and exhaustive stale/locked/modal permutations remain unverified natively. |
| Native font diagnostics | Final pinned release starts unchecked and reports actual DejaVu Sans whole-run fallback to installed Noto Sans CJK JP separately from the primary face. Wanted cases expose U+0378 glyph-ID-0 samples, empty/oversized layers and paragraph overflow. Check/Close/Save preserves exact original PROJ; replacement changes only two unlocked font references while preserving the locked layer, paint and animation. A subsequent check creates no history: one Undo restores the exact original, Redo and native reopen restore the exact replacement. A 258-layer fixture visibly cancels with reported/unexamined counts; restarting and switching groups clears the old result, and a fresh new-group check contains no stale reports. All 22 posthoc assertions pass, including exact reopened PROJ/VIEW and 8 CRC-validated official codec roundtrips; 31 actual CUA screenshots preserve the session. Windows/DPI, real IME and every asynchronous scheduling permutation remain outside native coverage. |
| Windows CI and device behavior | Fresh remote CI has not run: the requested checkpoint is local commits only, with no push or PR. Historical Windows native/device results remain in the backlog; the Windows-only audio-device test was not run on Linux. |

Linux native startup needs a working Vulkan driver, and Open/Save needs a running
D-Bus desktop session with `xdg-desktop-portal` plus a compatible file-chooser
backend. The smoke check used Mesa Vulkan and the GTK portal backend. Missing
portal services can make Open/Save appear to cancel without a dialog. See
[development prerequisites](README.md#development) for distro-level setup.

## Complete backlog inventory

Every original A01–L05 ID appears once below. The labels are deliberately bounded:

- **Implemented**: the stated baseline is present and has recorded automated or
  native evidence. It does not mean unlimited formats, every platform or AE parity.
- **Partial**: meaningful functionality exists, but listed scope or acceptance
  checks remain.
- **Not implemented**: the requested capability is not available; related lower-level
  tools do not count as that complete workflow.
- **Separate advanced scope**: unimplemented work requiring its own design and
  acceptance plan beyond the current 2D editor milestone.

### A — Projects and assets

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| A01 | Implemented | Stable composition IDs, create/duplicate/delete/switch, tabs, save, independent editing and CLI selection; referenced compositions cannot be deleted. Limits: 100 compositions and 1,000 layers total. |
| A02 | Partial | Shared assets, folders, search/sort, metadata, thumbnails, reuse and shared relinking. Tree multi-selection, drag movement and full keyboard navigation remain. |
| A03 | Partial | Nested rendering, rational-FPS conversion and all-attributes Pre-compose for supported contiguous selections. Noncontiguous/external-parent cases, leave-attributes mode and collapse transformations remain. |
| A04 | Partial | Native LEP instance-owned recovery slots, legacy JSON discovery, previous checkpoint, durable restore, recovery/defer choice and stale-write protection. Longer backup history and a project recovery-management view remain. |
| A05 | Implemented | Versioned LEP container, shared embedded images and sequence manifests, bounded integrity checks and precommit metadata/image budgets with atomic failure. Linked media remain external; whole-process memory budgeting is a separate extension. |
| A06 | Implemented | Relative paths inside the project tree, Save As rebasing, Collect Files, missing-media lists and individual/folder relinking across compositions. Outside-tree paths remain absolute; ambiguous matches require selection. |
| A07 | Implemented | Rational FPS, NTSC input, NDF start display, duration units/presets, bounded composition sizes and shared time conversion. Drop-frame numbering is unsupported. |
| A08 | Implemented | Atomic multi-file import, PNG/JPEG numbered sequences, gap policies, FPS/alpha interpretation and composition-from-source. TIFF/EXR, field/PAR/ICC interpretation and watched sequence growth are unsupported. |
| A09 | Implemented | Work area and per-composition playhead/view metadata, panel ratios, columns and tabs persist on explicit Save. Recovery uses default views; view changes do not enter document Undo. |

### B — Workspace and interaction

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| B01 | Partial | AE-inspired fixed workspace and recorded native size comparisons. A maintained cross-DPI visual regression baseline remains. |
| B02 | Not implemented | Arbitrary docking, detached panels/windows and named workspaces. Current split sizes and view-state persistence are covered by A09. |
| B03 | Partial | Functional Effect Controls, categorized effect search, saved/imported user presets and application. Preset drag-to-apply and full list keyboard UX remain. |
| B04 | Not implemented | Dedicated Footage/Layer viewers, locked composition tabs and viewer navigation history. |
| B05 | Partial | Keyboard menus, command search, graph/gradient keyboard editing and input-focus protections. Custom shortcuts, comprehensive context-command coverage and all native focus/IME regressions remain. |
| B06 | Implemented | Shared RGB/HEX/HSV picker, relevant opacity controls, recent colors and Composition pixel sampling with cancellable drafts. OS-wide eyedropper and color-managed/HDR sampling are unsupported. |
| B07 | Partial | Focus/keyboard handling and selected native checks exist. Full screen-reader semantics, real IME composition/candidate testing and 100–200% DPI coverage remain. |
| B08 | Not implemented | An integrated recent-project/settings/problems/history workspace. Existing status messages and individual settings are not that workflow. |
| B09 | Implemented | Rulers, guides, grid, snapping, title/action safe, RGB/alpha channel views and pixel information; overlays are excluded from output. Custom ruler origin and guide-preset exchange remain extensions. |

### C — Layers and timeline

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| C01 | Implemented | Null, independent Solid settings and Adjustment compositing, including masks/effects/opacity and history. |
| C02 | Partial | Solo, Shy, Guide and Hide Shy, with defined preview/output behavior. Labels and additional quality/switch columns remain. |
| C03 | Implemented | Atomic layer Copy/Cut/Paste across compositions in the same project, fresh IDs, parent remapping and FPS conversion. Cross-project/OS clipboard and playhead-relative layer paste remain extensions. |
| C04 | Implemented | Composition/layer marker names, colors, durations, navigation and time conversion through split/copy/nesting; collision cases are rejected atomically. |
| C05 | Implemented | Playhead, work area, layer/key/marker snapping, group-spacing preservation, zoom-aware tolerance and Alt override. |
| C06 | Partial | Constant source speed/reverse/freeze/slip and positive selected-key time scaling. Automatic layer sequencing/stretch and key-time reversal remain. |
| C07 | Implemented | Multi-layer rotation/scale, alignment/distribution and parenting Pick Whip with current-pose preservation and double-transform avoidance. Bounds exclude effect/mask expansion. |
| C08 | Partial | Property hierarchy and animated-property filtering exist. Modified-property filtering, compound search, row virtualization and measured large-timeline latency remain. |
| C09 | Not implemented | The planned Reset/Fit/flip/center-anchor/Auto Orient/layer-Skew command set. Existing anchor gestures and Contents-group Skew are narrower tools, not completion of this item. |

### D — Properties and animation

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| D01 | Partial | Shared addresses/tracks/history for transforms, effects, audio, masks, paths, shapes, text paint and Contents paint, including RGB and scalar gradient channels. Rich text/string and general compound-property animation remain. |
| D02 | Partial | Independent incoming/outgoing scalar speed/influence, Auto/Continuous modes, pointer handles and directional Easy Ease. Path-time/multidimensional semantics and full AE interpolation equivalence are not established. |
| D03 | Partial | Single-channel Value/Speed Graphs, multi-key editing, navigation/snapping, Value Graph selection scaling and horizontal Speed Graph time scaling. Vertical Speed/corner transforms and simultaneous multi-channel graphs remain. |
| D04 | Not implemented | Spatial position Bezier motion paths, spatial tangents and roving keys. Scalar X/Y animation is available. |
| D05 | Implemented | Animated source-time remapping for footage, sequences and precompositions, including reverse/hold, history and defined audio handling. Optical flow/frame blending remains G07. |
| D06 | Partial | Versioned effect/stack animation presets with search/import and FPS-aware multi-layer application. Arbitrary transform/mask/text property presets and selective paste remain. |
| D07 | Separate advanced scope | Expressions, references/controllers, loops/randomness and bounded deterministic evaluation; no AE expression-language compatibility claim. |

### E — Shapes and paths

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| E01 | Partial | Drag-created rectangle, rounded rectangle, ellipse, polygon and star; open lines can be drawn with Pen. Dedicated parametric line/tool-default controls and wider native regression remain. |
| E02 | Partial | Open/closed Bezier Pen, insert/delete/convert points, handles, parametric conversion, same-path marquee/Ctrl+A, multi-vertex moves/static deletion, animated-safe Reverse/Set First and single-vertex numeric transactions. Cross-path selection, bulk numeric transforms and topology-changing animation remain; corrected native mouse acceptance is recorded separately. |
| E03 | Partial | Animated scalar/RGB/opacity paint, cap/join/miter/dash controls, fractional Points and Contents linear/radial Gradient Fill/Stroke. Remaining parametric details, topology editing and complete native coverage are not done. |
| E04 | Partial | Nested Contents tree, paths/paints, animated group transforms/Skew, Composite ordering, 16 paint blend modes, gradient drafts/endpoints/modal editor, selected-group Pen and same-parent sibling multi-selection/block ordering. Compound Colors animation, cross-parent block dragging and bulk field editing remain; bounded native acceptance is recorded above. |
| E05 | Partial | Fixed-topology shape/mask path animation exists. Trim Paths, Repeater, Merge/Offset Paths and topology-changing interpolation are not implemented. |
| E06 | Not implemented | SVG import with editable element conversion and unsupported-element reporting. Internal SVG rendering is not an SVG importer. |

### F — Text

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| F01 | Partial | Direct canvas point/paragraph text, selection, paste, wrapping, resize, commit/cancel and Undo. Real Korean IME composition/candidate and full native gesture/focus coverage remain. |
| F02 | Partial | Installed family/real-style selection, shared preview/output resolution, missing-font reporting, project-wide replacement and explicit bounded final-glyph/fallback diagnostics. Variable axes, runtime catalog refresh, semantic/color-font assurance and cross-machine portability testing remain. |
| F03 | Partial | Layer-wide alignment, spacing, wrapping, Fill/Stroke and paint order, plus seven scalar Fill/Stroke RGB and Stroke Width animation tracks. Per-character rich text, typography/source animation and full kerning/paragraph controls remain. |
| F04 | Not implemented | Text Animator and character/word/line Range Selectors. |
| F05 | Not implemented | Text on paths, Source Text animation and reusable per-instance text controls. |

### G — Masks, compositing and effects

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| G01 | Partial | Ordered vector masks, modes/invert and animated opacity/uniform feather/expansion/path. Same-path multi-vertex moves/static deletion are implemented; variable feather, RotoBezier and topology-changing animation remain. |
| G02 | Implemented | Alpha/Luma and inverted track mattes, independent reusable source references and reference validation/remapping. Null, Adjustment and Audio cannot be matte sources. |
| G03 | Implemented | Normal/Multiply/Screen/Add/Overlay layer modes with alpha and Adjustment rules in the 8-bit sRGB compositor. The separate 16 Contents paint modes do not extend this layer list. |
| G04 | Implemented | Ordered, named effect instances with add/delete/duplicate/reorder/bypass/reset, common animated parameters and legacy migration. |
| G05 | Implemented | Basic Fill/Tint, Levels/Curves, Hue/Saturation, Glow, Drop Shadow and linear/radial Gradient effects, including effect endpoint handles. Free-form Curves, more advanced options and AE pixel equivalence remain outside this baseline. |
| G06 | Not implemented | The planned keying/matte-cleanup, displacement/distortion, noise/transition and layer-style families. |
| G07 | Not implemented | Motion blur/shutter sampling, subframe temporal compositing, frame blending and optical flow. |

### H — Audio

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| H01 | Implemented | Independent audio and the first video audio stream, metadata, bounded waveforms and shared source/time editing. Stream selection and full-length waveform overview remain extensions. |
| H02 | Partial | Windows default WASAPI output, scrub, device-clock playhead and block meters. Other platform backends, device selection/recovery and measured display/acoustic A/V latency remain. |
| H03 | Implemented | Audio switches/Solo, animated left/right level, pan/fade, nested mixing and Peak/RMS/clipping meters. True peak, peak hold and audio effects are unsupported. |
| H04 | Implemented | Shared 48 kHz stereo mixer, AAC MP4/PCM MOV, work area/nesting/remap and auto/off settings. Linear resampling changes pitch with speed; pitch preservation and broader rate/channel options remain. |

### I — Rendering and color

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| I01 | Implemented | Shared structured settings/source/path/font/encoder/destination preflight covers CLI, native exports and queue. Bounded actual-codec probe, safe sibling write/sync and persisted fallback/strict font policy pass regression tests. Native Linux strict rejection/output preservation, restart, fallback video and missing-parent diagnostics passed. Filesystem changes after preflight, disk-space guarantees, per-glyph font coverage and Windows-native acceptance are outside this baseline. |
| I02 | Implemented | Snapshot-based persisted render queue, ordering/retry, multiple outputs/presets, failure policy and cancellation. External source bytes are not embedded in queue snapshots. |
| I03 | Partial | Size/rational FPS/channel options, H.264 CRF/ABR/speed, ProRes/PNG and audio auto/off. Additional codec/audio profiles and rate/channel choices remain. |
| I04 | Implemented | Explicit bounded SDR sRGB/BT.709 transfer/matrix/range/tag and alpha policy, with round-trip checks. This is not ICC display management, linear-light compositing or HDR. |
| I05 | Separate advanced scope | 16/32-bit buffers, linear compositing, ICC/OCIO, HDR and EXR pipeline. |
| I06 | Partial | CLI composition/range/format/settings rendering with success/failure exit status. Structured job specs/progress, richer cancellation/error codes and resumable sequences remain. |

### J — Preview and performance

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| J01 | Implemented | Persistent CFR decoder, bounded prefetch/LRU, seek reuse, asynchronous compositor and stale-request cancellation. VFR and interrupting an individual SVG raster pass remain unsupported. |
| J02 | Partial | Budgeted RGBA RAM cache, work-area pre-cache, resident-range display, invalidation and source watching. Disk cache, persisted cache preferences and long-run display/FPS measurement remain. |
| J03 | Partial | Shared media allocations, save-budget checks and selected decoder/cache measurements. Incremental evaluation, a total Undo-memory budget and comprehensive benchmark projects remain. |
| J04 | Separate advanced scope | Proxy/ROI, adaptive preview quality, multi-frame rendering and GPU compositing, to be guided by measured bottlenecks. |

### K — Advanced production and compatibility

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| K01 | Separate advanced scope | 3D transforms, cameras/lights/views, shadows and depth of field. |
| K02 | Separate advanced scope | 3D models/materials/environment lighting and text/shape extrusion. |
| K03 | Separate advanced scope | Motion/planar/camera tracking, stabilization and mask tracking. |
| K04 | Separate advanced scope | Rotoscoping, paint/clone tools, Puppet, content removal and simulation. |
| K05 | Not implemented | Reusable templates, exposed per-instance controls and CSV/JSON-driven substitutions/batch rendering. |
| K06 | Separate advanced scope | Script API/plugin design and AEP/PSD/AI/MOGRT exchange research. JSX/ExtendScript and Adobe-format compatibility are not implemented. |

### L — Productization and validation

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| L01 | Partial | Moon test/media/format tasks, dependency lockfiles, explicit non-deploying web/API CI and Windows desktop/media/release workflow. Final local model/media/check/release checkpoints pass as recorded above. Fresh remote Windows CI is unrun under the local-commit-only scope. |
| L02 | Partial | Single-editor ownership/recovery, CLI tool discovery/overrides and startup prerequisites. Installer, updater, distribution and clean-machine end-to-end qualification remain. |
| L03 | Partial | Reproducible examples, extensive model/render/media tests and recorded native sessions. Automated UI state-transition/DPI regression and maintained performance thresholds remain. |
| L04 | Partial | User/developer guides, in-app shortcut help, examples and recorded format migrations. Korean UI localization, broader error/help polish and release packaging remain. |
| L05 | Separate advanced scope | Web editor/API product definition and desktop model-sharing plan. Web `/editor` remains a placeholder; build/test success does not make it a motion editor. |

## Next milestones

**User priority, 2026-10-03:** implement the dedicated `.lep` (Libre Effects
Project) container end-to-end before any further editing features. Keep legacy
`.lfe.json` import and source originals intact. Codec, bounded integrity checks,
Save/Open/Save As, CLI, recovery and Collect Files implementation, automated checks
and the bounded native acceptance above passed. The dedicated-format milestone
is complete. The preserved same-path marquee/Ctrl+A slice also passes
focused/aggregate/media/release checks and the bounded native acceptance above.
The E04 Contents sibling tree also passes aggregate/model/render checks, its
bounded native workflows and the corrected direct-save focus/playback gate.
The bounded F03/D01 text-paint slice passes aggregate/model/render checks and
the final native acceptance above. The E02 single-vertex numeric transaction
passes the aggregate/render gates and the corrected native mouse-button gate.
F02 final-glyph diagnostics passes automated/render checks and the bounded
native gate above. The next P1 slice is D03 vertical
endpoint-velocity scaling in the single-channel Speed Graph: keep key values and
times fixed, edit selected finite endpoint tangents atomically, reject Hold or
undefined sides and preserve the horizontal time route. It does not imply uniform
scaling of the whole derivative curve. Multi-channel axes/selection and corner
transforms remain separate milestones.
Cross-parent tree moves need their own coordinate/paint-scope design.
This does not close the broader A04 backup-management or other backlog extensions.

1. Extend native editing acceptance and retained regression evidence, especially
   animation, transformed/mask paths, focus/IME and DPI. Mid-drag preview observation
   remains tool-limited. Remote Windows CI remains unrun while delivery is local
   commits only.
2. Continue E02 selection/numeric vertex tools and E04 tree selection/dragging plus
   a deliberately versioned compound Colors/topology model. The current modal
   editor does not require or imply that later animation model.
3. Extend D02/D03 to multi-channel graphs and vertical Speed Graph transforms;
   specify spatial path/time semantics before adding roving/spatial interpolation.
4. Finish rich text and input quality: per-character F03 styling, text-property
   animation, real IME sessions, accessible focus and DPI regression. Keep F04/F05
   as explicit later text milestones.
5. Return to J02/J03 cache/incremental evaluation and productization after the
   prioritized editing workflow. Treat 3D, tracking/roto, script/plugin API and
   Adobe-format research as separately scoped projects with their own acceptance
   data, not unfinished checkboxes that can be declared complete by this recovery.
