# Reliable modal activation on X11

Source correction after the mixed `510244c` native batch. That binary produced
one saved unintended rename when Return was held across a nested ScriptUI confirm;
a bounded repeat stayed pending. The successful repeat does not erase the failure.
Initial proof remains in `../libreeffects-qa/rebuild-rich-native-20261006/`.

## Cause boundary and correction

GPUI 0.2.2 reports X11 repeat downs with `is_held=false`. Its polling code filters
a synthetic repeat release only when a corresponding press occurs in the same
batch, with no intervening event and within 20 ms. A leaked release can unlock the
application's otherwise-correct modal latch. This is a source-established risk
consistent with the native failure; the original native event stream was not
instrumented, so no exact event ordering is claimed.

The exact locked GPUI package is vendored with its Apache-2.0 license and original
file hashes. Its small capability API defaults false. The X11 override requests
XKB DetectableAutorepeat on the existing retained connection and validates both
supported and enabled reply bits. Only that per-client bit changes. No repeat
rates, global controls, system settings, extra connections, raw pointers, new
unsafe code or timer/debounce assumptions are introduced.

Audited Wayland/macOS/Windows adapters explicitly opt in to their existing native
release semantics; no new native qualification for those platforms is claimed.
Unknown/TestWindow implementations retain false. The app negotiates once while
opening its main window, before constructing modal controllers. The inherited
20 ms filter can still conservatively miss an unusually fast genuine re-press.
Injected events are not authenticated by this capability.

## Fail-closed fallback

Unsupported, disabled, missing-initialization and request/reply-error cases keep
ScriptUI, AE root selection and its save-confirmation keyboard activation disabled.
Every latch still tracks down/up state. A visible explanation asks for pointer
buttons; pointer Apply/Cancel, Tab/navigation and native text insertion remain.
Space in text and multiline Enter keep their text meaning. No arbitrary timer
promotes an unreliable release to an independent activation.

`LIBREEFFECTS_MODAL_POINTER_ONLY=1` selects the same safe fallback for accessibility
workarounds and deterministic QA. It only removes capability; no environment flag
can force unsupported activation on. The startup log records the selected policy.

This is a bounded policy for those modal controllers, not a claim that all legacy
editor dialogs have been independently qualified. Project format and rendering
code are unchanged.

## Verification and corrective handoff

Run serially with the canonical workspace-local environment:

- `bash apps/desktop/scripts/verify-xkb-reply.sh` tests actual reply interpretation
  without GPUI linkage, including missing/disabled/unrelated bits.
- `bash apps/desktop/scripts/verify.sh models -- --test-threads=1` tests nested UI
  queues, reliable repeats, synthetic release pairs, request errors, unsupported
  servers, new-session clearing and exit barriers.
- The fingerprint helper includes vendored source/resources/build inputs.
- `bash apps/desktop/scripts/verify.sh check`, formatting and staged diff checks.

Fresh gates: 217 model tests pass on unchanged replay (6.45s), two reply-mask
tests and six fingerprint tests pass, and canonical all-target check passes in
59.047s. First full model run had two unchanged 100ms expression budget failures;
all 15 expression tests and the full unchanged replay passed. No production
budgets changed. Formatting and local-change whitespace checks pass. The all-files whitespace
check reports seven mixed-indent lines in the unchanged upstream Metal shader;
its exact archive hash is verified, and those renderer bytes are preserved. The first attempt
to invoke the reply file directly required correction to a parent-module harness;
the tracked script now supplies it. No monolithic desktop test binary was built.

## Combined native result

The correction is natively qualified within the synthetic ScriptUI scope on source
`8729c0fc8c884d3ff5ffa751ec457afb440e4264`, build
`20261006.043817-e6237fd8ac02f80e` (705 source inputs). The normal X11 startup
logs verified capability. Return held 1,200 ms twice and Tab-focused Apply's Space
held 1,200 ms leave nested confirmation pending; Escape held 1,200 ms twice returns
only to the parent. A fresh Return then executes exactly one Unicode rename.
Ordinary text, held printable repeat, Tab and pointer cancel remain usable.
Complete source comparisons prove the intended name-only change, byte-identical
one-step Undo/Redo and native reopen/resave.

A separate `LIBREEFFECTS_MODAL_POINTER_ONLY=1` launch logs the safe fallback and
shows its visible explanation. Fresh and held 1,200 ms Return/Space/Escape cannot
activate either parent or nested ScriptUI. Direct text/Tab and pointer No/Yes/Cancel
work; the accepted rename changes only its name, and Undo/cancel preserve the
whole baseline. Both isolated app sessions close normally with exit 0. No global
keyboard setting changed. CUA visual observations, startup logs and full-source
receipts are in `../libreeffects-qa/rebuild-pooling-native-20261006/`.

The user removed AEP/import from scope before qualification; no new importer or
AE save-confirmation coverage is claimed. The initial `510244c` intermittent failure
and earlier rich/import CLI proof retain their original source attribution. The
combined binary separately passes two pooled CLI pixel pairs. No original JSX/AEP
execution, exhaustive repeat timing, marked IME or new other-platform qualification
is implied. Both new launches required a maximize/restore to paint the initial
client surface; that observation is recorded without attributing its cause.
