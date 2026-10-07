# Read-only expression evaluator

The standalone `libre-effects-ae-expressions` crate evaluates a bounded typed
AE-expression subset with the existing pinned QuickJS 0.14 dependency. It is
independent of core, GPUI, the mutating JSX facade and any project file format.
The desktop integrates it through a killable worker with persisted native expression
programs and shared evaluated preview/export/geometry views; see
[the current integration contract](../apps/desktop/EXPRESSIONS.md).
AEP opening/import is not a product requirement under the user's 2026-10-06 scope
change. The supplied AEP is reference material; original-template parity is not
claimed.

## Implemented contract

`ExpressionEvaluator::evaluate(snapshot, requested)` accepts a detached, immutable
composition snapshot sampled at one time and returns `EvaluatedProperties`.
Composition and layer IDs have separate Rust types, and property addresses include
both IDs. IDs never pass through JavaScript numbers, preserving all 64 bits.

- Actual JavaScript execution with functions, arrays, loops, `parseInt`, Math,
  lexical variables and statement completion values, including `if/else`.
  The runtime does not recognize or substitute source fingerprints.
- `thisComp.layer(name/index)` uses exact names, first matching layer in stack
  order and one-based indices. Composition width, height, duration, time,
  frameDuration/frameRate and numLayers are readable.
- Layer `transform.position/scale/opacity` and direct aliases read evaluated
  dependencies. Positions use pixels; scale/opacity use AE percentages.
- `effect(name)(1)` reads numeric Slider controls; `Slider` and
  `ADBE Slider Control-0001` also identify that one supported parameter.
  Names are case-sensitive; the fixture's `Start Pont` spelling is intentional.
- `thisLayer.marker.numKeys` and `.key(i).comment/time/index` read strictly ordered
  composition-time markers with one-based indices.
- `thisLayer`, `transform`, `marker`, `effect`, `time`, `startTime`, `inPoint`,
  `outPoint`, `value`, and `framesToTime(frames[, fps])` are expression bindings.
  `value` is the pre-expression track sample. Signed `startTime` is independent
  of trim. The default clock uses the snapshot's rational frame rate.
- Custom JavaScript cubic interpolation functions execute normally. A synthetic
  six-program graph exercises three state-position programs and three five-state
  animated properties, a 32-frame duration, markers and cubic control values.
- Frozen function wrappers are reused once per exact source ID within a batch.
  Each call retains fresh parameters, lexical scope and direct-eval completion
  semantics. Source eval is still parsed per call; no VM/cache survives a batch.
- Dependencies are evaluated once per batch and cached by stable property
  address. Cycles include the dependency path in a diagnostic. Each call creates
  a fresh VM; changing time or document state cannot reuse stale cached values.
- Numeric outputs must exactly match the authored scalar/two-vector/three-vector type,
  with finite components. Strings, objects, undefined, NaN, Infinity and coercions
  do not become numeric property values. A three-vector is not a 3D renderer.

Snapshots, roots, results and errors implement `Serialize`/`Deserialize` for a
bounded process transport. `EvaluatedProperties.values` and `.dependencies` encode
as sequences of `[PropertyAddress, value]` pairs, preserving typed addresses and
full-width IDs without invalid JSON object keys. Duplicate addresses, oversized
result maps and unknown struct fields reject on decode. This transport encoding
is not a project-file schema and does not replace frame-size limits or semantic
validation at the process boundary.

Any error rejects the batch and returns no partially successful view. Callers
must explicitly implement and disclose any authored-value fallback. Host access
failures are latched outside guest exception handling, so `try/catch` cannot turn
missing controls, cycles or read-only writes into a successful evaluation.
Ordinary caught JavaScript exceptions retain standard language semantics.

## Isolation and budgets

The VM has no editor reference, UI, files, network, shell, module loader or native
host callback. Snapshot facades are read-only; returned vectors are immutable and
copied before crossing the VM boundary. Standard intrinsic objects are frozen to
prevent one property program from corrupting another. No asynchronous jobs run;
Promise creation (including async functions/import) invalidates the batch. Date,
Math.random and unimplemented random helpers are unavailable, maintaining a
deterministic snapshot dependency graph.

Default execution budgets: 16 MiB VM heap, 512 KiB VM stack, 100 ms calling-thread
CPU time on Linux/macOS/Windows, a separate 2-second monotonic wall ceiling,
10,000 QuickJS interrupt callbacks, 20,000 host reads, 2,048 expression evaluations
and dependency depth 32. Interrupt callbacks are a deterministic engine-work
budget, not a claim of an exact JavaScript instruction count. Cancellation is
polled at VM interrupts and setup/finalization boundaries. Other targets explicitly
use a conservative monotonic wall fallback; supported native clock failures fail
closed. No evaluator clock can move to another thread. The time limit is cooperative,
not real-time preemption; bounded native work can overshoot the nominal deadline.
Configurable limits have hard ceilings and cannot disable all bounds. Native work
such as large BigInt multiplication can substantially exceed the CPU budget
before returning. The desktop therefore retains its separate killable-process
2-second deadline; the standalone crate alone is not hard preemption. See
[the measured policy and qualification limits](../apps/desktop/EXPRESSION_EXECUTION_BUDGET.md).

Pinned QuickJS contains native sparse-array loops that do not poll interrupts.
To prevent those paths from defeating cancellation, the guest subset explicitly
rejects Array `concat`, `join`, `toLocaleString`, `shift`, `unshift`, `reverse`,
`sort`, `slice`, `splice`, `copyWithin`, `flat`, `flatMap`, `fill`, `toReversed`,
`toSorted`, `toSpliced` and `with`, plus Iterator helpers and concat/zip/zipKeyed.
The six target programs need none of these. Ordinary array indexing, construction,
map/filter/reduce/find and standard language loops remain available. The host
uses private original methods only on its small, validated internal arrays.
Do not restore excluded native entry points without an interrupt-safety fix.

Input limits: 2,048 layers, 16,384 properties, 16,384 markers, 16 KiB individual
metadata strings, 1 MiB combined metadata strings, 64 KiB per distinct expression,
16,384 exact source-table entries, 512 KiB total source-table bytes and 4 MiB
serialized bridge input/output. All table entries count, including unused and
disabled bindings; duplicate entries and invalid IDs reject before execution.
Exact UTF-8 byte identity is used, without whitespace or newline normalization. Oversized,
nonfinite, duplicate-identity, wrong-type and out-of-order snapshots reject before
execution. Slider names must currently be unique on each layer.

Call from a worker, not the UI or render thread. This is a capability-restricted
embedded VM, not an operating-system security sandbox or a full AE JavaScript
engine. Expressions run in isolated **strict** lexical scopes. Schema76 adds
validated explicit local bindings for retained assignment-based programs; it does
not enable shared implicit globals. The current bounded contract also includes
primitive Source Text dependencies/results, branded `createPath` values for closed
mask targets, and clamped finite scalar/vector `linear`. Exact bounds, style rules,
local validation and qualification are in
[the Playbar contract](../apps/desktop/NATIVE_PLAYBAR.md).
AE vector arithmetic extensions, other-composition reads, `valueAtTime`, keyframe
APIs, arbitrary effects and AE random helpers remain unsupported.

## Integrated authored storage and transient source pooling

Native schema 65 already persists sparse per-layer numeric expression records
`{target, source, enabled}` and actual Slider Control effects with stable IDs.
Layer duplication preserves those authored records and normal Undo/Redo semantics.
Loading a project does not execute source. Rich native text separately uses schema71;
pooling itself changes no saved source representation. Schema76 separately adds
SourceText/MaskPath targets and optional explicit local bindings as linked above.

`CompositionSnapshot.sources` is a required exact-byte table, and each
`ExpressionProgram` contains `{source_id, enabled}`. The core snapshot builder
borrows comparison keys and clones each distinct source once. The same table is
sent through child IPC and the Rust→QuickJS bridge; per-property wire records
carry only source IDs. Legacy private snapshots reject explicitly rather than
being guessed or silently migrated. Parent and child are the same executable.

Pooling changes neither function/lexical scope nor result-cache identity. Each
property executes in its original strict scope; dependency caching remains keyed
by property address. No source-keyed evaluated-value or compiled-function cache
is introduced. Time/source changes create a new snapshot/VM. Bridge JSON is
written through a bounded4MiB writer so escaping cannot allocate an unchecked
oversized temporary. Disabled bindings still require valid table references.

The core produces authored samples and rational time/origin/markers. The worker
returns one complete result with source/time receipt checks; all render consumers
use the same detached evaluated view, which cannot be saved. Failures remain
visible and never silently substitute authored tracks. The standalone crate's
cooperative limitations above still apply, and the desktop's external process
deadline supplies hard termination for native work.

See [pooling acceptance and current qualification](../apps/desktop/EXPRESSION_POOLING.md).

## Verification

Use one Cargo process and the shared cache, with `CARGO_BUILD_JOBS=1` and
`CARGO_INCREMENTAL=0`:

```
cargo test -p libre-effects-ae-expressions --locked -j1 -- --test-threads=1
cargo check -p libre-effects-ae-expressions --all-targets --locked -j1
cargo fmt --all --check
git diff --check
```

Tests are independently authored synthetic fixtures. Original attached scripts,
project bytes, expression text and lyric content are not committed or executed.
Passing these tests establishes the evaluator contract, not AE render equivalence
or end-to-end desktop expression support.
