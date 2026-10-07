> Implementation follow-up: [current scripting preview](../apps/desktop/SCRIPTING.md).
> The assessment below remains pinned to the earlier baseline; it is not a claim
> that the new runtime or native ScriptUI work is still absent.

# AE JSX and ScriptUI compatibility target

Assessment date: 2026-10-05. Inspected source baseline: `53e9cba`.
This is the pre-implementation compatibility contract, not a claim that later
implementation checkpoints have passed it. See the newest desktop
[STATUS](../apps/desktop/STATUS.md) entry for delivered scope and verification.

## Reference and immediate conclusion

The owner supplied `Automation.jsx`, whose header identifies it as Flat Lyric &
Part Maker, and requested that its workflow guide automation and ScriptUI
support. The complete 873-line, 25,723-byte file was inspected without executing
it. Its original remains outside the repository; do not publish or commit it.
Use independently authored synthetic tests for the API behaviors below.

**At the assessed baseline, this file cannot run in Libre Effects.** There is no
JSX loader, JavaScript/ExtendScript runtime, ScriptUI host or AE object-model
bridge in the Rust desktop application. Existing Rust editing primitives are
useful building blocks, not callable AE APIs. React `.jsx` is unrelated to this
After Effects `.jsx` script format.

Even with a runtime and dialog bridge, the parts/all modes require genuine
three-dimensional spatial Position behavior that the current 2D document model
does not represent. Do not advertise the complete reference script as supported
merely because its dialog opens or a test stub finishes.

## What the reference actually does

1. Opens a resizable dialog with one multiline input, live timestamp/part counts,
   sample/clear/validate/cancel controls, and lyrics/parts/all creation buttons.
2. Parses `HH:MM:SS:CC`, where `CC` means hundredths of a second, **not frames**.
   The optional braced suffix supplies a part name. Ordinary lyric entries have
   three nonempty lines; a part-only timestamp may have no lyric lines.
3. Resolves existing compositions and templates by name. It does not create or
   import them: `Lyric` / `LyricLayer`, and `Name & Artist` / `Partname` plus `Song`.
   It checks Source Text and, for parts/all, an unseparated 3D Song Position.
4. Duplicates the lyric template, moves each copy to the stack beginning, changes
   its name and multiline Source Text, enables it, and assigns its start time.
   The first entry starts at its own timestamp; later entries start at the
   previous input entry's timestamp. Each destination composition snaps times
   independently to its own FPS.
5. Writes Focus/Hide/End markers at its own, next and following entry times.
   Missing future entries use two-second fallbacks. The out point is one second
   after End. The final two calculated lyric copies receive label index 4.
6. Keeps empty entries while calculating timing, then removes their duplicate
   layers. A timestamp suffixed by `...`, or all three lyric tokens being `.` or
   `...`, denotes an empty entry. Individual blank tokens become line breaks.
7. Duplicates the part template at each part timestamp, replaces its text, and
   writes four Position and four Opacity keys onto the shared Song layer at
   offset times. It restores interpolation, temporal ease and spatial handles.
   Existing keys are not cleared first.
8. Uses one undo group for the chosen operation. A caught processing error can
   leave generated content, with one Undo offered; empty calculation layers are
   cleaned up where possible. Success/error alerts are displayed.

The template project itself was not supplied. Its fonts, styles, animations,
parenting, effects and any expressions consuming markers are unknown. The JSX
neither authors nor evaluates expressions. Marker data alone cannot establish
visual equivalence with the original AE project.

## Inspection and execution boundaries

No file, folder, network, shell, dynamic evaluation, include, import, project
save or export operation was found in this particular script. Its external
host actions are dialogs and in-memory project changes. This observation does
not authorize executing arbitrary scripts or trusting future files.

A script host needs explicit source-size, execution-time/instruction, memory,
recursion, object/control, string, key and project-mutation budgets. Do not expose
filesystem, processes, network, `File`, `Folder`, `Socket`, `system.callSystem`,
`$.evalFile`, module loading or native callbacks by default. Unsupported APIs
must produce an actionable filename/line/API error instead of a silent no-op.
Catchable unsupported-property exceptions must not silently turn into a success
claim, especially where this reference intentionally catches ease errors.

ScriptUI callbacks may outlive initial source evaluation. Cancellation, Close,
repeated clicks, focus loss, exceptions and project replacement must retire the
correct session. A synchronous `Window.show()` continuation must see the selected
result and its lexical callback state. A `show()` stub that immediately returns
causes this reference to return `null` without doing anything.

## Baseline API matrix

`Ready primitive` means a corresponding native editor operation exists.
`Partial primitive` means a semantic/model gap must be resolved before binding.
`Missing` means the host API or required model is absent. **Every AE/ScriptUI
binding is missing at baseline**, including rows with ready native primitives.

| Reference API or behavior | Native baseline | Mapping and remaining contract |
| --- | --- | --- |
| `#target aftereffects`; ordinary `var`, functions, regex, arrays, math, exceptions, `finally`, closures, `instanceof` | Missing | Choose a bounded JS runtime and strip only supported directives; preserve source line numbers. Do not translate JS with ad hoc textual substitutions. This file does not require general ExtendScript extensions or E4X. |
| `new Window("dialog", ..., {resizeable:true})`; `add("group"/"panel"/"statictext"/"edittext"/"button")` | Missing | Retained control tree and modal desktop UI; nested layout and multiline/scrolling input, with `wantReturn`, are required. |
| `orientation`, `alignChildren`, `spacing`, `margins`, `minimumSize`, `alignment`, `preferredSize`, `helpTip`; `graphics.font`, `ScriptUI.newFont` | Missing | Define the supported layout/font subset and visible fallbacks. Do not equate merely storing these fields with rendering them. |
| `text`, `active`, `onChanging`, `onClick`, `onShow`, `onResize`, `onResizing`, `layout.resize()` | Missing | Bidirectional control state, callable closures and event dispatch. Programmatic text changes in this file explicitly invoke the counter update. |
| `defaultElement`, `cancelElement`, `center()`, `show()`, `close(result)`, `alert`, `confirm` | Missing | Modal result/continuation, Enter/Escape routing, nested confirmations and cancellation. Enter in the multiline input must honor `wantReturn`. |
| `app.project.numItems`, `item(i)`, `item instanceof CompItem`, `item.name` | Partial primitive | `Project::compositions()` / `composition_by_id()` and asset/folder model exist. AE item indices are 1-based and include non-compositions; core composition IDs are stable identifiers, not indices. Specify item enumeration and stable wrapper lifetime. |
| `comp.numLayers`, `comp.layer(i)`, `layer.name` | Ready primitive | `Composition::layers()` / `layer(id)` and `RenameLayer`; bridge 1-based stack indices to stable IDs and preserve first matching name semantics. Core names are bounded/trimmed. |
| `comp.frameDuration` and per-comp time snapping | Ready primitive | `FrameRate` stores exact numerator/denominator. Expose seconds per frame; centiseconds need their own parser, not `FrameRate::parse_timecode`. Keys and markers store integer composition frames. |
| `layer.duplicate()`, `moveToBeginning()`, `remove()` | Ready primitive | `DuplicateLayer`, `MoveLayer {index:0}`, `RemoveLayer`. Return the new stable handle, not the selected row index. Preserve template content/keys/styles/markers. Existing lock, child and reference checks still apply. |
| `layer.enabled = true` | Ready primitive | Stored `visible` plus `ToggleVisible`; setter must inspect state rather than toggle unconditionally. |
| `layer.startTime`; `layer.outPoint` | Partial primitive | `ShiftLayer` shifts ranges, keys and markers, but no general text-layer start origin is stored. `SetLayerRange` only trims. Do not alias startTime to inPoint. Existing ranges/keys must remain inside the composition, unlike an unrestricted AE timeline. |
| `layer.label = 4` | Partial primitive | Native layer color is RGB, not an indexed AE label palette. Define and disclose a label mapping or model it explicitly. |
| `property("ADBE Text Properties").property("ADBE Text Document")`, `.value`, `.text`, `.setValue(document)` | Partial primitive | UTF-8 Source Text and layer-wide styles exist. `EditSourceText` preserves style but adds a key when animated; that is not a general static `Property.setValue` contract. A TextDocument wrapper and explicit keyed-property behavior are needed. Preserve CR/CRLF/LF and blank-line semantics in tests. |
| `property("Marker")`, `new MarkerValue(comment)`, `setValueAtTime` | Partial primitive | Layer markers and `Command::Marker` exist in composition frames. Add then Update supplies the name, but Add does not replace a same-frame marker. Implement deterministic upsert; preserve unrelated template markers and obey unique-time/range budgets. |
| `property("ADBE Transform Group")`, `property("ADBE Position")`, `property("ADBE Opacity")` | Partial primitive | Typed `PropertyPath::Transform` and scalar X/Y/Opacity tracks exist; match-name property groups and vector Position wrappers do not. |
| `position.dimensionsSeparated`, `position.propertyValueType`, `PropertyValueType.ThreeD_SPATIAL` | Missing | Only 2D scalar X/Y Position exists. Do not report ThreeD_SPATIAL for it. All supplied key Z values are zero, but the reference explicitly rejects non-3D properties; a declared 2D conversion is a different workflow, not unchanged-script support. |
| `setValueAtTime`, `nearestKeyIndex`, `keyTime` | Partial primitive | Sorted `AnimatedProperty::keys()` and key edit commands are available. Specify forced key creation/upsert, 1-based sorted indices, tie behavior and same-time collision handling; ordinary SetValue alone may only edit the static value. |
| `KeyframeInterpolationType.BEZIER/LINEAR/HOLD`; `setInterpolationTypeAtKey(in,out)` | Partial primitive | Native Linear/Hold/Bezier and outgoing-segment interpolation exist. Independent AE in/out interpolation representation is not present. Smooth is explicitly Smoothstep, not an AE temporal Bezier substitute. |
| `new KeyframeEase(speed,influence)`; `setTemporalEaseAtKey`; temporal Continuous/AutoBezier setters | Partial primitive | `TemporalHandle`, `SetTemporalHandle` and `SetTemporalMode` exist for scalar channels. Convert units/second to units/frame using exact FPS and percent to fraction. Native mode is one enum, endpoint setters require adjacent segments, and spatial speed is not the same as independent component slopes. |
| `setSpatialTangentsAtKey`, `setSpatialContinuousAtKey`, `setSpatialAutoBezierAtKey` | Missing | No spatial Position path or three-dimensional tangents. Temporal scalar curves cannot substitute for these without changing the motion. |
| `app.beginUndoGroup`, `app.endUndoGroup` across two compositions | Partial primitive | `Editor::execute(Command::Batch)` prepares a candidate and records one atomic change. Commands generally target the active composition. Add a composition-addressed transaction/session that preserves active view and stable handles, with explicitly documented rollback/error semantics. |

## Concrete staged priorities

The owner's 2026-10-05 automation/ScriptUI request promotes this bounded K06
track to active development. It does not implicitly require AEP, MOGRT, third-party
plugins, arbitrary Adobe APIs or the full 3D renderer in the first increment.

1. **Bounded runtime and interactive ScriptUI vertical slice.** A selected `.jsx`
   file can evaluate supported language/directive syntax, present the reference's
   control kinds, retain callbacks and return from a real modal session. An
   independent tiny dialog fixture edits text, updates a count, confirms an
   action and cancels. Unsupported OS/network/3D APIs diagnose explicitly.
2. **Read-only AE model plus lyric primitives.** Stable project/composition/layer
   wrappers, property match names, Source Text and MarkerValue, then candidate
   transactions for duplication, naming, stack order, enabling, timing and marker
   upsert. Resolve layer-time and animated Source Text rules before claiming the
   lyric mode runs. A supported synthetic template is the first useful editing
   acceptance target; the original template has not been inspected.
3. **Reference lyrics-only workflow.** Preserve the three-line/blank-entry rules,
   centisecond rounding, shifted-template animation, Focus/Hide/End timing,
   last-two labels, cleanup and one Undo. Add snapshot-based preflight and an
   explicit failure policy for unsupported properties/times. Review outcomes
   against an independently calculated synthetic project, not a self-derived
   expected result.
4. **Parts/all fidelity.** Add actual vector/spatial/3D Position and its time/ease
   semantics, separate interpolation endpoints, cross-comp transaction support,
   and collision tests for closely spaced parts. Until then, parts/all must
   report the unsupported 3D dependency. An optional 2D adaptation must be
   labeled as such and must not silently change the supplied script.
5. **Wider compatibility only after evidence.** Expand the named API coverage and
   regressions from additional real scripts. File/system access, expressions,
   external plugins and native Adobe project formats are separate explicit
   design/security/product decisions.

### Suggested ownership boundaries

- Runtime/control model: isolated JS engine integration, directives, resource
  budgets, diagnostics, native-call/event protocol and retained callback lifetime.
- Document host: stable wrappers, property types, candidate project mutation,
  composition targets, time conversion, transaction/undo and serialization tests.
- Desktop host: chooser/menu entry, GPUI ScriptUI rendering, multiline text/focus,
  modal lifecycle, user messages and stale-session guards.
- Qualification: synthetic API fixtures and independently authored expected
  documents; build/runtime/native evidence must be reported separately.

## Acceptance cases required by this reference

- Dialog: sample insert/replace confirmation, clear confirmation, live count,
  validation without mutation, each mode button, Enter/Escape versus multiline
  Return, resize, close, repeated buttons, cancel and handler exception.
- Input: LF/CRLF/CR; Unicode text; skipped blank lines; part-only entries;
  incomplete triplets; invalid minutes/seconds/centiseconds; empty/unclosed
  braces; all blank versus individual blank tokens; no recognized timestamps.
- Timing: 24, 25, 30 and 30000/1001 FPS; centiseconds not frame labels; half-frame
  rounding; distinct target comp rates; negative/nonfinite/out-of-range API
  inputs; duplicate/snap-colliding/nonmonotonic timestamps; fallback End/outPoint
  past composition duration. The script itself does not reject all such inputs,
  so any safer narrowing must be visible rather than presented as AE parity.
- Project: absent/duplicate comp or layer names, non-text template, locked target,
  changed/deleted template during dialog, preserved unselected layers, existing
  markers/keys, copied styles/animation, repeated runs and bounded large input.
- Result: stack order, names, text/newlines, enabled state, time origin versus
  trim, marker names/frames/upsert, label mapping, empty-layer cleanup and no
  mutations during validation/cancel. A second run intentionally creates more
  copies; do not silently deduplicate the user's requested operation.
- History/storage: one intended undo entry; Undo/Redo across compositions;
  failed/canceled runs preserve document/history; save/reopen retains the exact
  supported source data, without unsupported metadata being quietly dropped.
- Song: four Position and four Opacity keys per part, sorted 1-based key lookup,
  same-time overwrites, independent in/out interpolation/ease, endpoints and
  spatial tangent preservation. Lack of 3D must fail explicitly before mutation.
- Security/resources: disallowed APIs, infinite loops, deep recursion, excessive
  controls/text/keys/layers, nested or abandoned undo groups, cancellation and
  synchronous modal callbacks without unbounded UI blocking.

## Code evidence at the assessed baseline

- `crates/core/src/lib.rs`: `Property`, `Layer`, `Composition`, `Command`,
  `Editor::execute`, `DuplicateLayer`, `SetLayerRange`, name/visibility/order.
- `crates/core/src/compositions.rs`: ID lookup/enumeration and active-comp swap.
- `crates/core/src/editing.rs`: `ShiftLayer`, batch edits, duplicate and bounds.
- `crates/core/src/markers.rs`: composition-frame markers, unique-time limits.
- `crates/core/src/source_text_animation.rs`: static/animated string editing.
- `crates/core/src/tracks.rs`, `temporal.rs`, `time.rs`: scalar track addresses,
  ease handles/modes and rational frame clocks.
- `apps/desktop/src/main.rs`, `editor.rs`, `shell.rs` and Cargo manifests: native
  host/menu entry points; no scripting engine or ScriptUI module at baseline.
- Desktop `STATUS.md` K01/K06 and README Remaining limitations: prior explicit
  3D/JSX/ExtendScript/expressions exclusions remain accurate for this baseline.

This assessment used source inspection only. No original JSX execution, desktop
build, application interaction, runtime compatibility test, push or deployment
was performed. Documentation verification: link/path review and `git diff --check`.
