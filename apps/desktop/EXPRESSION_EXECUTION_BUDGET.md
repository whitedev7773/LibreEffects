# Expression execution budget correction

The prior actual Lyric preview showed a transient budget error during an
Undo/Save→Reopen transition. A later observation rendered correctly without
input. That incident remains evidence; its exact cause has not been reproduced.
This change addresses measured wrapper overhead and distinguishes active work
from descheduling. It does not claim the original incident is conclusively fixed.

## Policy and isolation

- Default 100 ms calling-thread CPU on Linux/macOS/Windows. Linux/macOS use
  `CLOCK_THREAD_CPUTIME_ID`; Windows uses checked `GetThreadTimes` kernel+user
  accounting. Native API errors, invalid timestamps, overflow and regression
  fail closed. The clock is neither Send nor Sync.
- Other targets explicitly use conservative monotonic wall accounting. This is
  bounded fallback, not permission to run without a clock.
- A separate 2-second monotonic evaluator wall ceiling cannot be disabled or
  raised. The existing parent supervisor still kills/reaps an isolated process
  at its independent 2-second deadline, including uninterruptible native work.
- Runtime/context setup, host compilation/execution and result validation are
  charged. Successful results get a final budget/cancellation check. Existing
  preparation/input validation precedes the timer as before; parent supervision
  bounds the whole child call. Waiting for the shared worker permit is excluded.
- VM heap/stack, interrupts, reads, evaluations, dependency depth, IPC limits and
  cancellation remain bounded as before. No persistent runtime or cross-frame
  value cache was introduced. Diagnostics report phase, charged/wall time and
  interrupt count without exposing source text.
- Each exact pooled source gets one frozen function wrapper per batch. Repeated
  invocation has fresh arguments/lexical scope; direct eval preserves statement
  completion and still parses source each time. Recursive dependencies and
  caught host rejection retain their existing semantics.

## Recorded measurements and checks

Evidence, relative to the repository:
`../libreeffects-qa/actual-expression-budget-20261006/`.
`comparison/COMPARISON.md` and `comparison/equivalence.json` retain full outcomes,
private instrumentation provenance, timing and restoration checks. No original
expression source or private project is committed here.

The fixed optimized baseline/candidate matrix passes exact full-result equality
for actual composition 1/frame 4360 three times, a 192-binding synthetic batch,
four whitespace/Unicode sources and a small JSX's complete project/output.
Every evaluated value, dependency, identity and counter agrees. Actual candidate
CPU was 22.665/12.407/9.154 ms. Synthetic CPU was 83.577 → 25.066 ms; wrappers
192 → 6, wrapper CPU 49.676 → 1.326 ms. These are bounded observations, not a
statistical latency guarantee; the first actual candidate sample was slower.

A single controlled 150 ms sleep inside the evaluation interval demonstrated
the policy difference: old wall accounting rejected, while the candidate returned
identical actual values at 159.817 ms wall / 9.755 ms CPU. This diagnostic sleep
does not establish that the native incident involved descheduling.

Both variants reject native BigInt work through a deliberately lowered 150 ms
production supervisor deadline (153.3/153.1 ms), cancel ongoing native work
(32.3/34.7 ms), and reject a 32 MiB allocation under the unchanged 16 MiB heap.
All workers/permits were released. No partially successful result was returned.

Clock-provider tests: six pass, including native Linux sampling and checked
Windows FILETIME arithmetic. A provider-only check with the genuine installed
`x86_64-pc-windows-msvc` standard library and Windows bindings passes. This is
typechecking, not a Windows runtime or full-desktop build claim.

Unoptimized crate gates are retained, in order:

1. `candidate/expression-tests.stdout`: 47/47.
2. `candidate/expression-tests-final.stdout`: 44/47 after final result/setup
   checks, with active-CPU Budget failures in the six-program, 192-binding and
   four-source cases.
3. `candidate/expression-tests-diagnostic.stdout`: 35/47 after adding bounded
   phase/time diagnostics. Reported failures charge 107–364 ms, mainly host and
   expressions, once host compilation. CPU/wall closely agree. One expected
   unsupported-operation assertion instead receives Budget. The tiny four-source
   case passes this time. No general debug-suite pass is claimed.

The exact command is `cargo test -p libre-effects-ae-expressions --offline
--locked --lib -- --test-threads=1`, after sourcing the shared development
environment, with jobs 1 and incremental off. The last compilation passed;
47 tests completed in 5.38 seconds, exit 101. Earlier errors are not replaced by
the successful optimized matrix. No further benchmark expansion or limit raise
is planned for this bounded correction. A read-only artifact inspection confirms
the final debug test binary embeds QuickJS C compiled at -O0; release uses
opt-level 3. This is a credible profile-cost difference, not an isolated causal
experiment. The same O0 engine was used across all three debug gates, so it does
not by itself explain their variability or the native release incident.

## Canonical and final native qualification

The single canonical all-target check passed on production commit `40a836a4`:
`bash apps/desktop/scripts/verify.sh check --offline`, jobs 1/incremental off,
1m29s, exit 0. Raw logs are `candidate/all-target.stdout` and `.stderr` in the
evidence root. Formatting and diff checks pass. No new full model replay or
desktop test executable was run. The optimized comparison precedes the final
diagnostic-only formatting; the final release gate below uses those final bytes.

Qualified source `f57e02d8`, build `20261006.171519-e6f3b2a85d4d3bbb`, passed one
canonical measured release in 772.069s. Native About and all 760 input hashes
matched. Final-binary Lyric4360 and title4359 preserve identical PNG bytes and
all 1,931,520 decoded RGBA pixels against their prior qualified native frames.
The original schema75 project and five exact supplied faces remain unchanged.

One heavy numeric draft canceled while visibly Checking. A separate attempt
rejected with `phase=host and expressions, charged=346.046 ms, wall=435.689 ms,
interrupts=5`. The active-CPU guard rejected before the 2s process watchdog, so
this UI test does not independently qualify watchdog expiry. The workload was
not increased. Both drafts left the authored project unchanged.

Exactly three predetermined synthetic-rename/history/reopen cycles then passed
15 complete-source comparisons. Every Undo/final Undo and Save/Open/Save restores
the complete native baseline, including VIEW; every renamed/Redo result matches
its exact expected file. All observed settled Lyric4360 frames render without an
unexpected budget error. These worker transactions also demonstrate recovery
after the draft cancellation and rejection. The saved app closed normally,
exit 0, at 17:37 UTC. No additional runtime attempts or debug retries were made.

The prior transient remains evidence, with its cause unproven. Three fixed cycles
cannot establish every frame, machine or arbitrary program meets 100 ms. The
recorded unoptimized failures, platform limits and partial-project gaps remain
explicit. Actual-frame regression is not independent AE pixel parity. Full
receipts and preservation checks are in the sibling
`libreeffects-qa/actual-expression-budget-release-20261006/QUALIFICATION.md`.
