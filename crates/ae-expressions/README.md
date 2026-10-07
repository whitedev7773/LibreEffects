# Bounded expression values

This crate evaluates exact expression source bytes over a detached, immutable
frame snapshot. It exposes no editor mutation, files, network, UI, or asynchronous
jobs. The calling application must retain its isolated-process watchdog.

Numeric scalar, 2D-vector, and 3D-vector wire values retain their existing JSON
representations. Two additional values are admitted:

- `Text(String)` is a primitive string, limited to 16,384 UTF-8 bytes. Expression
  strings containing unpaired UTF-16 surrogates reject; they are never repaired or
  coerced. `thisComp.layer(name).text.sourceText` samples the same lazy dependency
  graph as numeric properties. Non-text layers do not expose `text`. TextDocument
  objects and text styling are outside this facade.
- `Path(ExpressionPath)` contains layer-local vertices, relative incoming and
  outgoing handles, and a boolean `closed`. Every array has matching length,
  limited to 1,024 vertices; coordinates must be finite and within ±1,000,000.
  Closed paths require at least three vertices, open paths at least two.
  Coincident vertices are valid. Native mask owners may impose closed-only paths.

`createPath(points, inTangents, outTangents, isClosed)` requires four explicit
arguments. Empty handle arrays expand to zero relative handles. Dense array data
elements are copied immediately; accessors, coercion hooks, and inherited array
elements are rejected. Proxy descriptor traps remain under the VM budgets and
cannot suppress latched host failures. Returned paths carry a private identity
brand, so arbitrary objects and proxies around a path cannot forge a result.
The authored `value` of a path property has the same brand. Path inspection and
mask-reading methods are not exposed.

For zero-handle polygons, `createPath` reproduces the float32 to signed 16.16
coordinate storage measured in independent AE probes, including half-step
rounding and overflow to -32768. Authored path values retain their exact captured
coordinates. Curved paths retain the native relative-handle contract; their AE
storage pipeline is not qualified and can also change vertex coordinates.

`linear(t, tMin, tMax, first, last)` clamps to its endpoints and interpolates
finite scalars or matching two-/three-component vectors. Its bounded contract
requires an increasing time range with a finite span; it does not coerce values.

## Explicit lexical locals

`ExpressionProgram.local_bindings` is compatibility metadata, defaulting to an
empty list. Each name creates a fresh `let` binding in the strict wrapper around
the unchanged direct-eval source. Names start initialized to `undefined` on every
property invocation, including nested dependencies and repeated source IDs.
An empty list retains the original strict behavior: undeclared assignment fails.
The wrapper cache includes both exact source identity and the ordered local list.

At most 64 unique ASCII identifiers of at most 64 bytes each are accepted. Strict
reserved words, `eval`, `arguments`, host bindings, and global names are forbidden.
`validate_local_bindings` is shared with native document admission. The runtime
also checks the actual VM globals without invoking their getters. This is an
explicit lexical adaptation, not source rewriting or a shared sloppy global
environment. It neither discovers identifiers nor retries failed programs.

Direct eval preserves JavaScript statement completion values. Existing frozen
intrinsics, per-property cache/dependency identities, cycle detection, sticky
host-failure handling, and CPU/wall/memory/stack budgets remain active. Any failure
rejects the entire evaluated view. Only immutable primitive values or validated
private path copies reach the serialized result; authored snapshots stay intact.

The regression programs in this crate are independently authored synthetic
fixtures. Original project expression sources are not included.

Within one evaluation, immutable marker-key and Slider views are reused, and
exact layer-name lookup uses a first-name index. Every lookup and Slider sample
still charges the host-read budget, including cache hits. Invalid lookups and
attempted mutations remain sticky batch failures. Nothing is cached across
snapshots or frames. Development/test builds optimize the embedded C VM while
retaining the same expression execution limits.

For actual AE reference evidence, the desktop's `ae-expression-reference.jsx`
captures explicit composition/time cases and exact expression source. The
`compare_reference` example checks those local cases with production evaluator
limits and an explicit numerical tolerance, reporting every mismatched property.
These captures are separate from synthetic unit fixtures and do not attest to
full AEP import, text rasterization or effect/render parity.

## Bounded math and time helpers

`add`, `sub`, `mul`, `div`, `dot`, `cross`, `length`, `normalize`, `clamp`,
`degreesToRadians`, `radiansToDegrees` and `timeToFrames` are available to exact
expression source. `linear` accepts both its three- and five-argument forms.

Vector helpers admit one to four finite components. Add/subtract/dot/distance
pad missing axes with zero; multiply/divide use a finite scalar. `cross` admits
two three-component vectors. Clamp accepts three numbers or three vectors;
scalar/vector mixtures reject. Zero-vector normalization, division by zero,
nonfinite arithmetic, sparse arrays, accessors and implicit coercion reject the
whole batch. Normalization scales before measuring magnitude, preserving tiny
and very large finite directions. Final property dimensions remain unchanged.

`timeToFrames` defaults to the snapshot's composition-local time and frame rate;
native snapshots have no nonzero Adobe display-start origin. Absolute times round
down, including negative values; durations round away from zero. The duration
flag must be boolean, FPS positive, and the result an exact-range JS integer.

These helpers are frozen, count against the existing host-read budget, and are
reserved from explicit `local_bindings`. Helpers never mutate an authored value
or add random/asynchronous capabilities. The reference semantics are documented
in [Adobe's expression language reference](https://helpx.adobe.com/after-effects/desktop/work-with-expressions/expression-language-reference/expression-language-reference.html);
the stricter native input bounds above remain part of this subset.
