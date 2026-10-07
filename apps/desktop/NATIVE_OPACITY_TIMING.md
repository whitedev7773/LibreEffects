# Native Opacity timing, schema 73

This source milestone adds independently authored native scalar timing needed by
the JSX workflow. AEP opening/import is outside acceptance. Full supplied-script
and After Effects numerical parity remain unqualified. Qualified source 9c76d88 / build 20261006.081441-31989ffc887ad91f has passed the
bounded final CLI and native gate described below. Earlier d7aa0f9 XYZ evidence
retains its own attribution.

## Source ownership

The existing `properties.Opacity` value/key map is the only stored value source.
Optional `Layer.opacity_timing.keys` must cover exactly the same nonempty frame
set. Each record owns incoming/outgoing interpolation, incoming/outgoing ease,
and explicit temporal-continuous/auto flags. Speed is stored directly in signed
percentage units per second; influence is stored in percent. No FPS division,
absolute-value conversion or epsilon-zero inference rewrites authored speeds.

When metadata exists, every underlying legacy key has Linear interpolation and
empty legacy temporal handles. Conflicting timing sources, malformed coverage,
unknown fields and unsupported flags reject. Materialization requires schema 73
across active/inactive compositions; LEP1 and VIEW1/2 do not change. Versions1–65,
71 and72 retain their existing contracts; unavailable66–70 remain rejected.

Initial promotion accepts canonical Linear keys with empty temporal handles.
Existing Smooth/custom Bezier/Auto/Continuous data stays untouched and rejects
unsupported promotion. Unchanged Linear/false assignments do not promote or
create history. New native timing defaults are explicit Linear sides, zero
speed, 100/3-percent influence and false flags; these are not inferred AE defaults.

## Sampling and scripting

`opacity_at(frame, seconds_per_frame)` is the authoritative raw source sample.
Legacy tracks delegate unchanged to their existing sampler. Annotated tracks
combine each side independently: Bezier value controls use signed speed times
segment seconds times influence; Linear sides use secants. No division by value
delta occurs, so equal endpoints may overshoot or reverse. Exact keys preserve
source values. Outgoing Hold holds; active incoming Hold alone is unsupported.
Dormant ease and endpoints remain stored even under Linear/Hold modes.

Compensated frame normalization and centered time inversion handle the stationary
100%/100% case. Default absolute1e-9 and relative1e-12 value-error ceilings both
apply, with64 iterations and an80-iteration hard cap. Numeric range, insufficient
precision and exhausted work are explicit errors. True automatic/continuous
modes reject. Native structural authoring may retain an unsampleable curve for
repair; automation validates resulting active segments before accepting its one
atomic transaction, even when JSX catches a rejected host operation.

The host supports independent interpolation sides, both endpoint eases, raw
signed/tiny speeds, false flags, exact-time replacement, key metadata and removal.
Existing-key value edits preserve timing. New/removal operations preserve exact
coverage. Annotated final-key removal and animated static replacement require an
explicit future collapse contract and reject. Legacy unannotated removal retains
its prior behavior. Explicit malformed outgoing types reject instead of defaulting.

Generic scalar sampling/Graph access is unavailable for annotated Opacity.
Ambiguous key move/edit/toggle, timing, scale and mixed-copy paths reject before
mutation. Explicit native value edits preserve metadata and can add documented
native keys. Whole-layer shift/startTime/duplicate and same-FPS paste retain all
metadata; FPS changes/cross-FPS paste reject rather than rescale stored speed.

## Rendering, expressions and color

Normal/matte, adjustment and nested rendering use the owning composition's time
base. Raw Opacity may be below0 or above100; only paint clamps it. Expression
snapshots consume the same raw sample. Finite evaluated Opacity can overshoot;
its transient view clears source timing only in that view and cannot be saved or
committed. Persisted authored values and key values remain bounded0–100.

Inspector/Timeline show truthful read-only native timing, raw value and key count.
The layer Fill Color dialog initializes original/current alpha from the clipped
presentation sample. Accepting or retyping unchanged0/100 and RGB-only edits do
not bake raw overshoot or insert opacity keys. A genuinely changed alpha uses
the atomic native value owner. Independent Text Fill/Stroke alpha is unchanged.

## Source verification before release

- 19 pure scalar math tests, crate all-target check, formatting and whitespace
- 249 editor-model tests, including exact source/metadata, raw expressions,
  inactive compositions, legacy preservation, retime/history and bypass guards
- Real separate child-process input/output with negative-subnormal and dormant
  metadata, full-source duplicate/startTime equality, one Undo/Redo and rollback
- Independent core/host and desktop source review; two findings were corrected
  with regressions: generic key-rotation batch rebinding and malformed outType

One initial existing model run had three old-contract assertion failures and a
100ms expression-budget fluctuation. Updated contract assertions and the full
249-case rerun pass; no execution limit was relaxed. The standalone production
Renderer probe at `tests/opacity_render_probe/probe.rs` passes22 literal pairs
(135,168 RGBA pixels, zero differences), identical preview/output, actual expression
workers, nested-FPS/adjustment discrimination, raw/tiny/endpoint metadata and nine
explicit renderer rejections. The helper corrected two fixture assumptions: Text
paint tracks must be explicitly authored before Graph admission, and native Hold
metadata can be retained for repair while active sampling/automation/rendering fail.
No production correction was needed from the probe.

The canonical workspace all-target check passes in69.492s, with formatting and
whitespace checks. Seven focused UI regression sources compile; a native desktop
test executable was not built or run. Low/high source studies are bundled as
`examples/native-opacity-{low,high}.lep`, with usage notes beside them. The exact
helper receipt and its failed-first-attempt records remain under
`../libreeffects-qa/native-opacity73-20261006/`. Final-binary/native qualification was pending at that source checkpoint;
the completed bounded gate is recorded below. No fullsuite, AEP/original-script run, push or deployment.

## Bounded final release and native qualification

On 2026-10-06, clean **9c76d88a12a3b8d72b3d6d61450b1b21a12a5196** produced build
**20261006.081441-31989ffc887ad91f** with **738** verified inputs. Its 72,892,696-byte
binary SHA-256 is `eebb044e3c692860b92cdf0d3e3d3c673b4118283ee43bd23c4c84c57bf7846a`.
The single canonical foreground release passed in **671.679s**. Profile/codegen
were unchanged; the recognized process-local allocator settings and measured
lifecycle are recorded in the QA receipt. Native About matched the exact build.

Final CLI passes **22 literal pairs / 135,168 RGBA pixels / zero differences**
and three explicit rejection cases preserving existing output files. Native low
and high studies verify truthful raw readouts, clipped painting and alpha,
read-only timing controls, direct color input, unchanged/retyped alpha preservation,
distinct RGB-only edits, exact one-step history and Save/Reopen. Full-source
comparison retains signed zero and all timing/value maps.

A synthetic ScriptUI transaction preserves independent dormant first-in/last-out
sides and exact signed −1e−199/−2e−199 ease speeds. Caught true-auto mode rejects
atomically with no Undo entry. The coupled XYZ-layer script stores four Opacity
keys and exact −5e−324/−7e−6/−3e−7/−1e−199 speeds, promotes schema 72 to 73, and
preserves all XYZ/camera and other authored fields. One Undo/Redo and Save/Reopen
are byte-exact. Seeking frame 15 changes only VIEW. Native expression/adjustment
previews, explicit errors, recovery and short held Return/Escape continuity pass.

The retained **28 native Save-state receipts** include **20 whole-file identity
comparisons**. Initial input saves preserve parsed source while adding VIEW and
rewriting JSON ordering. Independent artifact audit verifies hashes, exact float
bits, pixels and saved-state comparisons; CUA observations establish native action
chronology. No embedded-image coverage, full forced-pointer fallback rerun,
Adobe parity, AEP compatibility or full supplied-script compatibility is claimed.
The app closed normally with exit 0; prior evidence and original/copy inputs are
unchanged. Initial startup painting again needed maximize/restore, without an
attributed cause. No new functional defect was found.

Evidence: `../libreeffects-qa/native-opacity73-release-20261006/QUALIFICATION.md`,
`release-identity.json`, final CLI results/seal, `session/` source receipts and
`FINAL-PRESERVATION.json`. No further production build is needed for these docs.
