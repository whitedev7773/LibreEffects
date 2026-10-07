# JavaScript / JSX automation and ScriptUI preview

This is a **bounded, synchronous compatibility subset**, not an After Effects
runtime or AEP importer. Ordinary JavaScript executes in a separate headless QuickJS worker process.
Unsupported host operations reject the whole run, even if a script catches the
reported exception. The supplied AE lyric/part script and AEP remain outside the
repository and were not executed as test fixtures.

## Run a script

1. Open a Libre Effects composition and finish active text/property edits.
2. Choose **File → Run script (.jsx / .js)…** (also available in Find command).
3. Choose a UTF-8 `.jsx` or `.js` file up to 1 MiB.
4. Interact with its native dialog. The rest of the editor stays blocked while
   the script runs. **Cancel script** discards the entire candidate.
5. Successful edits appear together and take **one Undo**. A no-op preserves
   Undo/Redo. Uncaught exceptions, unsupported APIs, cancellation, runtime limits and
   stale editor receipts leave project source and history unchanged.

Try [the independently authored text-duplication example](../../examples/scripts/duplicate-text.jsx)
with **one static text layer selected**. It duplicates one layer per nonempty
input line, replaces each duplicate's text, adds a Focus marker, and keys opacity.
It uses no file/network access or external assets and does not modify its template.

Ordinary caught JavaScript exceptions retain normal JavaScript semantics; host
rejections and callback failures are latched independently and cannot be swallowed.

Scripts are not embedded into `.lep`, do not run on Open/Save, and cannot request
file/network access. Only the explicitly selected source file is read by the app.

## Current host surface

- `app.project.activeItem`, `numItems`, `item(i)` and `items`: 1-based project
  enumeration; read-only composition, footage and folder wrappers. Enumeration is
  deterministic native composition-ID order, then footage, then folders. Native
  IDs belong to per-type namespaces, not imported AE globally unique item IDs.
- `CompItem` type checks, name, size, duration, frame rate/duration, layer count,
  `layer(i)`/`layer(name)`, `layers`, and `selectedLayers`. The active composition's
  `time` is readable. Inactive composition time and animated `.value` sampling are
  rejected because a correct inactive playhead is not supplied to this host.
- Layers: stable references, name, enabled state, independent frame-aligned
  `startTime`, indexed `label` (0–16), in/out points, duplicate,
  remove and move-to-beginning. Duplication preserves native layer content,
  transforms, effects, parenting and animation; native names use ` copy` suffixes.
- Static text: `Source Text` / `ADBE Text Document`, `TextDocument.text`, and
  `setValue`. Existing whole-layer style is retained. Animated Source Text writes
  are rejected, rather than silently inserting or overwriting a sampled key.
- 2D Position/Scale and scalar Opacity: current values, static values, frame-aligned
  `setValueAtTime`, key count/time/value, nearest key and removal. Property and
  transform match-name aliases cover the supplied script's lookup pattern.
- Layer marker comments and frame-aligned duration; setting a marker at an
  existing time replaces it rather than creating duplicate timestamps.
- Legacy scalar tracks support LINEAR/HOLD interpolation, with matching incoming/outgoing types. Independent
  scalar temporal ease is supported where existing adjacent segments can express
  its speed/influence. Spatial/vector ease, incompatible endpoint cases, mixed
  in/out modes, BEZIER mode and ease adjacent to Hold segments explicitly reject.
- Native joined XY/fixed-plane XYZ Position and native per-side Opacity timing
  additionally support independent LINEAR/BEZIER/HOLD sides and exact stored ease.
  See [NATIVE_PLANAR_POSITION.md](NATIVE_PLANAR_POSITION.md),
  [NATIVE_SPATIAL_POSITION.md](NATIVE_SPATIAL_POSITION.md) and
  [NATIVE_OPACITY_TIMING.md](NATIVE_OPACITY_TIMING.md) for their admission limits.
- On those native key owners: `keyInInterpolationType`, `keyOutInterpolationType`,
  `keyInTemporalEase`, `keyOutTemporalEase`, `keyTemporalContinuous` and
  `keyTemporalAutoBezier` return authored metadata. Ease is a fresh one-element
  `KeyframeEase` array, retaining raw speed and influence, including dormant and
  tiny signed Opacity values. Native Position also exposes `keyInSpatialTangent`,
  `keyOutSpatialTangent`, `keySpatialContinuous` and `keySpatialAutoBezier`.
  Tangent arrays retain two or three axes and are detached copies. Indexes are
  1-based; missing keys, legacy timing and incompatible properties reject rather
  than infer metadata. Reads do not migrate schemas or create an edit.
- Numeric `expression` / `expressionEnabled` on Position, Scale and Opacity, plus
  evaluated `.value` reads. See [EXPRESSIONS.md](EXPRESSIONS.md) for persisted
  programs, named slider controls and evaluated preview/export semantics.
- `beginUndoGroup` / `endUndoGroup` validate balanced nesting. All successful
  script mutations still form one native history entry, including across comps.
- `alert`, `confirm`, `console.log`, `$.writeln` and normal JavaScript strings,
  regexes, loops, closures and Unicode. Last output appears in the status line.

Unknown APIs fail explicitly. In particular: out-of-composition stored layer
timing, custom AE label palettes, general 3D rendering/Scale/orientation, expression APIs outside the supported numeric
subset, rich character-style runs,
layer creation APIs, arbitrary effects/plugins, files/folders/sockets, module
loading, shell/processes, timers and asynchronous Promise work are unsupported.
Fractional-frame edits are rejected, rather than silently snapping to a new time.
The ordinary editor's legacy schema recalculation route is bypassed for these
restricted commands; untouched source and the existing schema floor are retained.

## Native ScriptUI subset

`Window("dialog")`, group, panel, statictext, edittext and button are supported.
Single-line and multiline input use native text input, selection, clipboard and
local text Undo. Both single-line and multiline buffers are limited to **16 KiB UTF-8**; oversize script
values reject before rendering. Newline entry uses the native normalized LF form.

`show()` really blocks the VM thread. The native event loop forwards input to the
original `onShow`, `onChanging` and `onClick` closures; it never restarts/replays the
script. `defaultElement`, `cancelElement`, `close`, active focus, enabled/visible
controls and Tab/Shift+Tab are handled. Holding an activation key cannot repeatedly
submit a button. Enter remains newline input inside multiline fields. Escape and
Cancel are distinct from successful Apply; canceled runs discard all mutations.
Alerts/confirms may temporarily cover a dialog without losing queued input.
If focus is lost while an activation key is held and its release is missed, the
first matching press after returning may be conservatively ignored; release and
press again. Native focus-loss/key-release behavior remains a qualification gap.

Layout is a bounded, scrollable native modal with row/column groups and panels.
AE size/alignment/font objects are accepted as advisory metadata, **not exact AE
layout/font rendering**. `center` and layout methods use the native modal layout.
For resizable dialogs, changing the containing app's viewport forwards resize
callbacks. There is no separate detachable ScriptUI window, palette, docking,
resource-string UI, absolute bounds, custom drawing, `onChange` (blur callback),
nested Window dialogs or re-show of the same Window instance.

## Runtime and lifecycle limits

- Source: 1 MiB; VM heap: 64 MiB; VM stack: 512 KiB
- Automation project: 16 MiB of raw serialized source, including images; larger
  otherwise-valid projects are rejected unchanged. Shared image references are
  currently repeated in this transport, so image-heavy projects can hit this
  additional execution limit before the native document limit
- IPC: 18 MiB per project/result frame, 1 MiB per UI frame, 256 MiB cumulative;
  serializers and frame readers enforce the limits before oversized allocations
- Execution time: 2-second uninterrupted execution slice, 30 seconds cumulative execution;
  time spent waiting for native user input is excluded
- Host: 10,000 calls, 10,000 project layers, bounded arguments and bridge output
- UI: 256 controls, 16 nesting levels, 10,000 exchanges, 64 queued native events
- Output: 64 KiB and 1,024 lines

The runtime is a capability-restricted embedded VM, not an OS security sandbox.
No QuickJS libc/system bindings or module loader are installed. A supervisor
checks cancellation and elapsed execution time independently of VM interrupt
hooks, including inside native array builtins. It kills and reaps a stalled or
canceled child, including when pipe I/O blocks. Only an actual complete native
UI request pauses the elapsed execution clock; sending its response resumes it.
Only one JavaScript worker may be active; it must be reaped before another begins.
A script selected while a preview expression batch is finishing waits for that
worker cancellably; the script execution budget begins after the permit is acquired.
Worker crashes, missing results, malformed/oversized messages and stale UI
responses discard the candidate. Returned projects are validated again in the
parent; the asset library must be unchanged and no new media source is allowed.
Linux workers additionally request a 1 GiB address-space limit. Unix workers
request a 35-second CPU ceiling and disable core dumps. macOS/Windows have no
verified equivalent hard process-memory limit; VM/stack/IPC budgets still apply.
These limits do not grant scripts access to operating-system APIs. The loader checks regular files
before and after opening and uses nonblocking Unix open to reject FIFOs/devices.
Ordinary filesystem stalls are not given a hard wall-clock I/O deadline.

Numeric expression batches use the same one-child supervisor in a separate
headless mode. Their immutable snapshot is capped at 4 MiB, the result frame at
16 MiB, and elapsed execution at 2 seconds independently of the evaluator's
100 ms cooperative budget. These batches never pause for UI. Background
render/export work waiting behind a script dialog can be canceled without
starting another child. Returned values must match the requested composition,
time, property identities and dimensions, and contain finite, acyclic, bounded
dependencies; authored tracks are never replaced by this transient result.

The app checks operation/document/core/input/transport/source/selection receipts
before installing a candidate. Closing or replacing the editor cancels pending
work. A late callback cannot install into a different document. Core validation
and normal project budgets run before a one-step transaction is accepted.

## Reference workflow gap

The attached AE project contains expression-driven lyric animation, styled text,
parented layers and effects. The numeric expression dependency now has a native
model and shared evaluated rendering path; full expression/API and styling
fidelity still exceeds this subset. The attached JSX additionally
uses layer timing, labels and 3D/spatial APIs. Independent frame-aligned origins
and indexed labels now have native models, with remaining timing bounds detailed
in [LAYER_TIMING.md](LAYER_TIMING.md). This preview
does **not** import that AEP or promise visual parity for the supplied workflow.
See [the assessment matrix](../../docs/ae-jsx-compatibility.md) for the historical
pre-implementation API inventory. The next fidelity work needs explicit native
models for these features; dropping unsupported data is not a migration strategy.

## Verification

Use the memory-light gates from the repository root:

- `bash apps/desktop/scripts/verify.sh models`
- `cargo run -p libre-effects-editor-model --example automation_process_harness --locked`
- `cargo test -p libre-effects-editor-model --example automation_process_harness --locked`
- `bash apps/desktop/scripts/verify.sh check`
- `cargo fmt --all --check`
- `git diff --check`

Runtime/host/modal-handshake regressions run in the GPUI-free editor-model crate.
Desktop tests are compiled by the all-target check; this does not execute the
monolithic desktop test binary. See the latest STATUS checkpoint for exact final
counts and native qualification. Windows/macOS/DPI and real marked-language IME
remain separate native verification requirements unless explicitly recorded there.

### Shared media across process boundaries

Private worker envelopes preserve explicit image and sequence alias partitions
using stable asset and composition/layer references. Exact payload and complete
partition validation precede restoration; surviving original handles are reused.
Equal payloads with independent identities stay independent. Contradictory source
merges/splits or malformed references reject the result without committing. This
keeps unchanged LEP image chunks and sequence manifests shared after scripting;
it does not change the saved schema or introduce content-based global deduplication.
