# Current desktop implementation status

Updated 2026-10-03 for the resumed-work, I01 output-preflight and
E04/E02 editing, D03 time-box and animated path-order checkpoints.
This is the current checkpoint inventory;
[DEVELOPMENT_BACKLOG.md](DEVELOPMENT_BACKLOG.md) retains the original audit and
its dated implementation/test history. Older “remaining” lists and test counts
there describe their own checkpoints, not the latest source.

Libre Effects is an **early 2D motion-graphics and compositing editor**. It is not
an After Effects replacement, an AE pixel-equivalent renderer, or an AEP/Adobe
script-compatible application. A working control or passing model test alone
does not establish a complete native editing workflow.

## Restored work and current verification

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
  Marquee/cross-path selection and topology-changing animation remain open.
- **Whole-track path order:** Reverse Direction and closed-path Set First Vertex
  reorder base geometry and every stored pose together without changing timing,
  pose references, transforms or paint settings. Canvas shortcuts remap the same
  selected geometric points; a separate outline marks the first point. Pure
  nonempty reorder batches preserve legacy version/assets and exact no-op history.
  Cubic geometry is preserved, but winding/Non-Zero holes and dash placement can
  intentionally change. Marquee/cross-path and numeric vertex editing remain open.
- **Selected-group Pen creation:** new open/closed paths can be drawn directly
  into the explicitly selected Contents Group, including empty/nested transformed
  groups. Existing applicable paints are inherited without paint additions; the
  Group remains selected for repeated drawing. Invalid targets/limits reject
  atomically. Existing hit precedence, Ctrl-mask and standalone fallback remain.
- **Speed Graph time box:** two horizontal handles scale selected key times while
  preserving values and temporal metadata through the existing core command.
  Finite signed endpoint speeds place the box; snapping is time-only and drafts
  freeze source/view/selection. Offscreen pivots, tool switches, final release,
  no-op history and glyph-clear boundary placement have regression coverage.
  Vertical velocity scaling and simultaneous multi-channel graphs remain open.
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
  legacy settings retain fallback behavior. Version 4 queue envelopes prevent
  old readers silently downgrading strict policy; versions 1–3 migrate without
  discarding explicit policies. Source scans also honor cancellation.
  This does not guarantee future free disk space or audit per-glyph font coverage.
- **Reproducible example:** [gradient-study.lfe.json](../../examples/gradient-study.lfe.json)
  is generated by [make_gradient_study.rs](../../crates/core/examples/make_gradient_study.rs).
  Generation and a 640×360 PNG render through the final release CLI succeeded.
  This generated fixture is distinct from a project created through native UI.

### Verification record for this checkpoint

The combined checkpoint includes all prior restoration/I01/Gradient/Pen fixes,
selected-group creation, Speed Graph time scaling and whole-track path ordering.
Formatting, type checking, all 579 default tests, the explicit 30-test media suite
and the optimized release build passed. Native checks are listed separately with
their tested build and coverage; automated tests do not imply native acceptance.

| Check | Result and boundary |
| --- | --- |
| Core tests | 210 passed, including save-budget guards, group spaces and 11 path-order/Batch regressions. |
| Desktop default tests on Linux | 369 passed, including 14 new Pen-order and 5 rendering regressions plus prior editing/focus/I01 tests; 30 FFmpeg tests ignored by default. |
| Explicit FFmpeg integration suite | All 30 ignored media tests were explicitly run and passed. They are not included in the default pass count. |
| Focused editing regressions | 16 modal-transaction tests, 55 Pen tests (including 14 group-creation and 14 path-order tests), 17 Speed-box tests, 5 path-order rendering tests, 3 shell-focus tests and the prior 20 gradient-draft/endpoint tests passed within the desktop suite; do not add these counts a second time. |
| Release build | `cargo build -p libre-effects-desktop --release --locked` succeeded after the final review fixes. The earlier interrupted-build blocker is closed. |
| Vendored grid | Debug and release each passed 215 unit tests and 43 doc tests with all features. |
| Web/API Moon CI targets | `web:build`, `web:test` and `api:build` passed locally; web has 3 passing tests. API validation is a deployment dry run, not a deployment. |
| Native Linux smoke check | Launched the app; created a composition, rectangle and Contents group; saved a 7,937-byte project through a real native Save dialog. The final release also replaced the earlier editor, leaving one work window. |
| Native gradient acceptance, release checkpoint | Actual endpoint drags, single-Undo restoration, color-picker Cancel restoration, OK apply and one Undo passed. Save/reopen preserved the changed endpoint in a 12,346-byte native project; a 1364×1024 screenshot records the result. Mid-mouse-held frame observation and Escape during an active drag were not verified because the input tool executes drags atomically. These checks do not establish complete stop/midpoint or keyboard coverage. CLI output from the native saved file matched an independent linear-gradient calculation at 2,244 interior pixels with maximum RGB error below 0.50/255 and opaque alpha. |
| Native I01 output preflight | Strict rejection preserved the existing destination; policy and diagnostic survived restart. Fallback produced H.264 320×180 at24fps with exactly two frames and an explicit substitution warning. A renamed output folder produced an actionable missing-parent diagnostic and no output. These UI checks used the initial I01 build; the subsequent v4 envelope guard and migrations passed the final fresh-target tests. |
| Native modal editor | Initial debug build passed stop/midpoint/RGB/opacity drafts, Cancel, HEX validation, field Escape, multi-change OK and exact one-Undo/Redo. Edit-away-and-back was a no-op preserving prior history. Final release reopened the saved result; its CLI output matched independent color/opacity midpoint math at 2,244 interior pixels with maximum RGB error 1.47/255. No actual AE comparison was performed. |
| Native Pen and overlay focus, final release | Same-path pair drag passed for legacy Shape, rotated/skewed nested Contents and vector mask; saved JSON confirmed identical local deltas, unchanged unselected positions and all tangent offsets. Selection-only was clean. Static six-to-four deletion, closed-path minimum-size rejection, animated topology rejection with both keys preserved, exact one-Undo/Redo and static save/reopen passed. Ctrl+K/Ctrl+N moved focus off the canvas, preventing Delete behind Settings; field input focus and field/dialog Escape passed. File actions such as Save intentionally blur and clear transient Pen selection. Atomic drag input still prevents intermediate mouse-held observation. |
| Native selected-group Pen / Speed time box | Optimized release passed nested closed/open curves, repeated group creation, Cancel, one-Undo/Redo, save/reopen and Ctrl-mask/standalone controls. Existing path/paint JSON remained exact; independent group-transform reconstruction matched actual click coordinates within 0.223 pixels per axis. Speed showed only two side handles; vertical-only and return-to-start drags were no-ops, horizontal scaling preserved values/influences with common nominal slope compensation, and one-Undo/Redo/reopen matched saved data. The inset frame-zero handle was visible and draggable without hiding the key glyph. 26 post-hoc assertions and 15 native screenshots record these flows. Alt-centered/differential snap, held-drag interruption, tiny plots and full layout/DPI coverage remain unverified natively. Extremely short plots may omit a handle if no glyph-clear position fits. |
| Native whole-track path order | Corrected optimized release is pinned; its native acceptance session is pending. Automated whole-track timing/selection/render guarantees above do not imply that native shortcuts, first-point marker or Properties Reverse have passed yet. |
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
| A04 | Partial | Instance-owned recovery slots, previous checkpoint, recovery/defer choice and stale-write protection. Longer backup history and a project recovery-management view remain. |
| A05 | Implemented | Shared embedded images and sequence manifests, larger envelope and precommit metadata/image budgets with atomic failure. External asset packaging and whole-process memory budgeting remain separate extensions. |
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
| D01 | Partial | Shared addresses/tracks/history for transforms, effects, audio, masks, paths, shapes and Contents paint, including RGB and scalar gradient channels. Rich text/string and general compound-property animation remain. |
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
| E02 | Partial | Open/closed Bezier Pen, insert/delete/convert points, handles, parametric conversion and same-path multi-vertex moves/static deletion through transforms, animated-safe Reverse/Set First and first-point marker. Marquee/cross-path selection, numeric vertex editing and topology-changing animation remain. |
| E03 | Partial | Animated scalar/RGB/opacity paint, cap/join/miter/dash controls, fractional Points and Contents linear/radial Gradient Fill/Stroke. Remaining parametric details, topology editing and complete native coverage are not done. |
| E04 | Partial | Nested Contents tree, paths/paints, animated group transforms/Skew, Composite ordering, 16 paint blend modes, live gradient ramp drafts, transformed canvas endpoints, transactional modal Gradient Editor and selected-group Pen creation. Compound Colors animation and tree drag/multi-selection remain. |
| E05 | Partial | Fixed-topology shape/mask path animation exists. Trim Paths, Repeater, Merge/Offset Paths and topology-changing interpolation are not implemented. |
| E06 | Not implemented | SVG import with editable element conversion and unsupported-element reporting. Internal SVG rendering is not an SVG importer. |

### F — Text

| ID | Status | Current scope and remaining boundary |
| --- | --- | --- |
| F01 | Partial | Direct canvas point/paragraph text, selection, paste, wrapping, resize, commit/cancel and Undo. Real Korean IME composition/candidate and full native gesture/focus coverage remain. |
| F02 | Partial | Installed family/real-style selection, shared preview/output font resolution, missing-font reporting and project-wide replacement. Glyph coverage/fallback diagnostics, variable axes and cross-machine portability testing remain. |
| F03 | Partial | Layer-wide alignment, spacing, wrapping, Fill/Stroke and paint order. Per-character rich text, full kerning/paragraph controls and text-paint animation remain. |
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
