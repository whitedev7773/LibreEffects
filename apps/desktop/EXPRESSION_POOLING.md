# Exact source pooling for native JSX workflows

The user clarified on 2026-10-06 that LibreEffects need not open AEP files. The
active goal is JSX/ScriptUI compatibility using native layers/compositions; the
supplied AEP remains a reference. No decoder or new importer work is included.
Existing importer code is retained, but its gaps are not acceptance blockers.

## Resulting contract

A frame-scoped `CompositionSnapshot.sources` table stores each exact UTF-8 source
once. `ExpressionSourceId(u32)` bindings travel unchanged through worker IPC and
QuickJS input. Native saved expression strings, schemas, source IDs for layers/
effects, history and source bytes remain unchanged. The table is transient.

The producer compares borrowed strings and clones each distinct source once.
Validation rejects duplicate table entries, invalid IDs including disabled
programs, more than16,384 entries, more than64KiB per source and more than512KiB
of all stored table bytes. Unused entries count fully. Native authored source
still has its existing16KiB per-program limit. Bridge JSON escaping uses a bounded
4MiB writer rather than allocating an unchecked oversized temporary.

There is no normalization, hash-only identity, source-keyed result cache or
compiled-function sharing. Every property retains strict lexical execution and
its own layer/time/value/dependency context. The existing property-index cache,
cycle checks, cooperative VM budgets and external hard process deadline remain.

## Independent native acceptance

`examples/support/pooling_fixture.rs` constructs64 native rectangles with origins,
trim, markers, Slider controls and three expressions each. Six independently
written3,176-byte programs repeat32times:609,792 logical source bytes become
19,056 table bytes without raising the existing budget. Every property is checked
against explicit arithmetic at frames120 and150, including evaluated Opacity
consumed by Position. No uploaded program, lyrics or private template text is used.

The model gate verifies source-table/ID roundtrip,192 separate evaluations,
CRLF/LF distinction, disabled source preservation, over-budget rejection, history
and byte-identical native save/reopen. The real-child example includes the exact
production supervisor module and verifies values across the actual IPC boundary,
invalid disabled IDs, duplicate tables, cancellation, worker-permit reuse and
unchanged native bytes.

Commands (one compiler at a time, jobs1/incremental off):

- `cargo test -p libre-effects-ae-expressions --locked -- --test-threads=1`
- `bash apps/desktop/scripts/verify.sh models -- --test-threads=1`
- `cargo run -p libre-effects-editor-model --example expression_process_pooling --locked`

Fresh checks pass:35 runtime tests (1.63s),220 model tests (10.50s), the actual
production-child fixture, formatting and independent read-only review. The first
aggregate-model compile found an older process example still on the old private
API; its source-table migration is now compiled by the passing model gate. The
existing production process harness passes 18 groups and the canonical all-target
check passes in 57.738s. The two interrupted XKB-only build sessions retain their
missing-terminal-result classification; neither is attributed to OOM/SIGKILL.

Combined source `8729c0fc8c884d3ff5ffa751ec457afb440e4264` is now release-qualified:
build `20261006.043817-e6237fd8ac02f80e`, 705 inputs, canonical release 571.514s.
Four final CLI exports at frames 120/150 match expression-free, independently
authored scalar references over **460,800 RGBA pixels with zero differences**.
All 640 reference scalars were separately checked using rational arithmetic;
references share the production raster backend. Native frame 120 renders, initial
Save preserves complete project data, and two ScriptUI name-only edits preserve
all 192 authored programs. Undo/Redo/cancel/reopen comparisons retain complete
native baseline bytes. See the top [STATUS](STATUS.md) and external QA evidence.
Original JSX workflow and AE visual parity are not yet established.

Next compatibility dependencies are native spatial Position/3D, faithful per-side
Opacity timing, and the script's required TextDocument/API behavior. AEP ingestion
is not part of this roadmap.
