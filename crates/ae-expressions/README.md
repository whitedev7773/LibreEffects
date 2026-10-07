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
