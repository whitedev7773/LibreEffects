# Expressions and evaluated preview

Schema **65** adds sparse, enabled/disabled numeric expression programs and real
named **Slider Control** effects. The first supported targets are 2D Position,
2D Scale, Opacity and slider values. The implementation follows the read-only
[snapshot evaluator contract](../../docs/ae-expression-evaluator.md).

Schema **76** extends that path with primitive Source Text dependencies/results,
closed `createPath` mask targets, finite scalar/vector `linear` and validated
explicit local bindings. Uniform text styling and exact authored source remain
preserved; mixed-style replacement and arbitrary path objects reject. See the
[current typed contract and actual Playbar qualification](NATIVE_PLAYBAR.md).

## Authored source and evaluated results

Programs retain exact UTF-8 source up to 16 KiB each. Empty assignment removes a
program; disabling keeps its source. Stable effect IDs bind slider expressions;
expression lookups use exact instance names, with the first duplicate name winning.
Slider controls have an animated numeric value and are identity operations in the
pixel compositor. Their inclusion does not add a color-conversion/filter pass.

Existing numeric/keyframe edits change the authored base tracks. The expression
receives that sample as `value`. Inspector and expression-driven slider fields
are labeled **base**; Timeline/Graph also continue to represent authored animation.
Composition pixels and expression-aware geometry use evaluated values.

The JSX Position/Scale/Opacity facade exposes `expression` and `expressionEnabled`.
Its `.value` reads evaluate enabled numeric expressions inside the already isolated
JSX process. Key values remain authored. An expression error is latched and rejects
the whole automation transaction even if the script catches the error. Inactive
composition value sampling still rejects when its actual playhead is unavailable.

For example, a script can set an existing layer's position expression to
`[value[0] + time * 40, value[1]]`. Disable the program to return to the unchanged
base animation. Layer creation and importing AE project files are separate features.

## Math and time helpers

Expressions can use `add`, `sub`, `mul`, `div`, `dot`, `cross`, `length`,
`normalize`, `clamp`, `degreesToRadians`, `radiansToDegrees`, and `timeToFrames`.
For example, `add(value, mul(normalize([3, 4]), time * 100))` moves a 2D Position
100 pixels per second in a fixed direction. `linear(time, 0, 2, 0, 100)` animates
an Opacity from 0 to 100 over two seconds; `linear(time / 2, 0, 100)` also works.

Vectors contain one to four finite components, with missing add/subtract/dot/
distance axes treated as zero. `cross` requires two three-component vectors.
Clamp accepts three numbers or three vectors. Final results must match the
target property's dimensions. Invalid inputs, zero-vector normalization,
division by zero and nonfinite results fail the complete evaluation.

`timeToFrames()` uses the composition-local snapshot time and rational frame rate.
Explicit times round down, including negative values; pass `true` as its third
argument to round durations away from zero. Native snapshots have no Adobe
display-start offset. These helpers are read-only and retain the evaluator's
existing resource limits. See the [complete helper bounds](../../crates/ae-expressions/README.md#bounded-math-and-time-helpers).

## One evaluated scene per render context

The core constructs immutable snapshots with rational frame rate, independent
signed layer start time, trim, named sliders, markers and pre-expression values.
A dedicated process evaluates requested properties and their dependencies once
per batch. Core validates the complete result before constructing a detached
composition/frame-specific view. This view cannot be serialized, saved, imported
as an editor document or committed into history.

Preview, still/sequence/video output, opacity, parent transforms and mattes pass
through the same compositor path. Nested compositions sample the original authored
project at their own remapped frame, never an ancestor's already-frozen tracks.
Within one frame, composition/time results are reused; 128 expression contexts
per rendered frame is the current bound. There is no cross-frame compilation cache.

Only relevant pixel roots, matte sources and parent transforms are requested.
Preview additionally requests transforms for active visible Null controls.
Hidden, unreferenced templates and their programs remain lazy dependencies, so
an unused template cannot break the frame merely by containing an expression.

Pixels and the root composition's evaluated view return together from background
rendering. Source, composition, core/document generations, time, resolution,
refresh, transport and transient-gesture receipts must match before geometry can
be used. Stale or missing geometry is hidden. Expression scenes bypass pixel-only
RAM hits until matching geometry is available. Errors clear the failed frame and
appear through the preview/export diagnostic path; there is no silent base-value
fallback. Selected hidden expression-driven matte controls are conservatively
suppressed rather than shown in authored coordinates.

Canvas interaction is deliberately selection-only for expression-driven geometry;
Hand/Zoom remain available. Transform, text/Pen/shape and related geometric actions
are refused until programs are disabled or authored properties are edited directly.
This avoids mapping edits through the wrong coordinate system. Full expression-
aware interactive manipulation is not claimed by this milestone.

## Exact-source pooling

The transient snapshot and both worker transports retain each exact expression
source once and refer to it by typed ID. Saved native source strings are unchanged.
The 512KiB source budget now counts unique table bytes; duplicate table entries,
bad IDs even when disabled, and unreferenced budget excess reject explicitly.
Independent192-binding/six-program native fixtures exercise per-property context,
markers, sliders, dependencies and time changes. This shares source bytes only,
not evaluated values or compiled functions. See [qualification](EXPRESSION_POOLING.md).

## Runtime boundary

Both JSX and expressions run in a separate copy of the executable, in a
headless mode entered before GPUI initialization. A supervisor can kill and reap
the child even when a QuickJS native builtin does not visit interrupt callbacks.
One process permit is shared across scripts, preview, export and thumbnails;
waiting is cancellable and is not charged as execution time. ScriptUI pauses only
for a complete validated UI request and resumes before its response is forwarded.
Expression batches never pause for user input.

Expression snapshots are limited to 4 MiB and result frames to 16 MiB, with a
2-second independent elapsed deadline, a 100 ms cooperative calling-thread CPU
budget on Linux/macOS/Windows, a separate 2-second evaluator wall ceiling and bounded
VM heap/stack/dependency work. These remain resource/capability boundaries, not an
OS privilege sandbox. See [SCRIPTING.md](SCRIPTING.md) for project/IPC limits and
platform-memory/orphan/focus qualification gaps. The standalone in-process
expression crate retains its documented native-method exclusions; ordinary JSX
sort/reverse/join remain available behind the hard process deadline. Other evaluator
targets use a conservative wall fallback; native clock failures reject. See
[EXPRESSION_EXECUTION_BUDGET.md](EXPRESSION_EXECUTION_BUDGET.md) for the measured
change, retained unoptimized test failures and bounded actual-project qualification.

## Paragraph fidelity

A shared allocation-free paragraph iterator recognizes CR, LF and CRLF, including
blank and trailing paragraphs. Layout, shaping inputs, SVG text nodes, caret/line
navigation, Text Animator line units and font diagnostics use original byte and
terminator ranges. Authored strings are not normalized during rendering or loading.
The native text editor preserves untouched authored CR/CRLF bytes. Schema71
supports bounded native character-style runs and style-preserving editing; AE
TextDocument replacement parity remains a separate, evidence-dependent contract.

## Independent acceptance fixtures

Run `cargo run -p libre-effects-editor-model --example expression_fixture -- DIR`
to write synthetic LEPs and explicit authored references. No original script,
expressions, lyrics or AEP bytes are copied into these fixtures.

- `expressions.lep`: six numeric programs, a named slider, Show/Focus/Hide/End
  markers, three CR-separated language lines and label index 4
- Frames 0, 59, 60, 119, 120, 180 and 239 must match the corresponding
  `reference-FRAME.lep` render exactly. Reference positions/scales/opacities are
  explicitly authored constants, not captured evaluator results
- `nested.lep` at frame 90 must match `reference-60.lep` at frame 60
- `cycle.lep` must report an error without creating or replacing output

The lightweight model and subprocess gates exercise semantics and process
boundaries. Pixel/native qualification belongs to the combined release checkpoint;
compilation alone is not evidence that the above raster checks passed.

## Remaining supplied-template gaps

The current acceptance goal is analysing the supplied AEP in After Effects and
converting it to an editable LEP with matching operation and output. Direct AEP
importing is outside the requested scope. Faithful media, typography, expressions,
effects, timing and rendering are acceptance requirements.
The previous native-only JSX milestone does not satisfy this goal. Remaining
script/workflow dependencies include general 3D rendering, further expression
APIs and AE TextDocument behavior. Native joined XY/fixed-plane XYZ Position and
per-side Opacity timing have bounded scripting contracts; see [SCRIPTING.md](SCRIPTING.md).
Additional effects and signed/out-of-composition key/range storage require separately bounded
native contracts where the project needs them. The original AEP has been opened
and played in After Effects; captured property values and reference frames are
local QA evidence. Its original automation script has not been run. See
[AE_PROJECT_COMPATIBILITY.md](AE_PROJECT_COMPATIBILITY.md) for the verified scope
and remaining import/render blockers.
