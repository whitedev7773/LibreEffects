# Native numeric expression and Null control authoring

The user clarified that a generated template plus successful JSX transactions
does not replace the reference project. This bounded milestone closes two actual
authoring gaps: creating/editing Slider Controls on Nulls, and entering/toggling
the existing numeric expressions through the native editor. AEP opening/import
remains outside scope. Reference media/fonts/programs stay private and out of Git.

## User-visible contract

Effect Controls, the catalog and Effect menu now permit Slider Control on Null
layers. A shared core capability matches Add admission; unsupported pixel effects
and preset application are not offered for Nulls. Existing rename/value/key,
locked-layer and Undo behavior is reused.

Properties exposes Position, Scale and Opacity expression entrypoints; Slider
Amount has an entrypoint in Effect Controls. The modal is a source-local draft
with Enabled, Apply, explicit Remove and Cancel. It pauses playback before
capture, preserves untouched authored source including CRLF, and supports local
text Undo and Ctrl/Cmd+Enter. Ordinary Enter belongs to the multiline field.
See [the from-blank guide](../../docs/numeric-expressions.md).

Enabled Apply prepares a detached candidate and requests the current visible/
guide roots plus the edited property, including hidden Null/Slider targets. The
existing supervised evaluator runs outside the UI process with unchanged limits.
Core's frame/result/dependency admission checks the answer; the evaluated view is
discarded. Only the authored expression command can become one document edit.
Disabled nonempty drafts save without evaluation. Empty source requires the
explicit Remove action rather than silently deleting a program.

The model captures core generation, source, selection, composition and frame;
desktop receipts additionally capture session, document, input and transport
generations. Text/toggle activity retires pending checks. Each attempt retains
its own cancellation token, so an old result cannot resolve a same-revision retry.
Cancel, stale receipts and failed validation preserve source and history.
Dirty detection and render refresh use the existing source/generation observer.

## Input integrity

Review found and corrected two loss paths before qualification: expression entry
could interfere with an unfinished base/name field, and marked input could begin
without changing the authored draft revision during an asynchronous check. The
existing workspace capture guard now runs before outside-down field submission,
with a dispatch preflight. An optional ScriptTextInput activity hook cancels
checks before marked/native/local edit intent. Existing ScriptUI consumers have
no hook, retaining their earlier callback behavior. Exact attempt identity also
prevents canceled-check ABA. Focused desktop regression sources cover these cases;
their execution/qualification status is stated in STATUS/HANDOFF.

## Boundaries and qualification

Only existing 2D numeric targets are exposed. Spatial compositions, text/path
targets, a full property expression API and original effects/visual equivalence
are not added. Duplicate Slider-name shadowing rejects explicitly. Applying an
enabled draft checks one current frame, not every possible future frame. No
source schema, runtime deadline or expression language is changed.

Required native acceptance starts with an empty project: create a named Null
Slider, create a visible layer, enter and apply a program, change/key the control
and observe the preview. Exercise disabled save, syntax/missing-reference/cycle
errors and recovery, Cancel, stale work, one Undo/Redo and Save/Reopen. Record the
actual controls and preview; a pre-authored project is not a substitute.

Source `91463d1e362519d0e931312eeb2b56f63b662298` is qualified by one canonical
release and a bounded Linux native batch. Build
`20261006.110446-7128516095e76842` contains 746 verified inputs. Actual UI creation
from blank, named Null/Slider, entered Position and Slider programs, live/keyed
preview, syntax/reference/cycle rejection and recovery, disabled invalid save,
explicit Remove, Cancel, one-step history and native Save/Reopen pass. Whole-source
oracles preserve underlying authored tracks. A native-created frame-30 output
matches an independent +60 X static reference across 2,073,600 pixels; the pair
shares the native raster/font backend. STATUS/HANDOFF retain the exact identity,
input/focus observations and coverage limits.

The final canonical all-target check passes in 42.226s. Ten new draft cases and
four Null Slider cases pass. The prior full model run was 258/260 with two
unchanged 100ms budget failures, including a failed isolated replay. A single
serial foreground replay now passes all 260 cases in 12.68s without relaxing
limits; prior failures remain evidence of wall-clock sensitivity. Desktop
session/input regressions compile only. No Windows, marked-IME or exhaustive
asynchronous-race qualification is claimed. Supplied Noto/A2Z/Paperlogy typography,
selected-range styling, the reference's remaining path/text programs, spatial
authoring and Audio Spectrum remain separate product gaps.
