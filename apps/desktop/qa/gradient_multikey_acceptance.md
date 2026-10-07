# Compound Gradient Colors multi-key acceptance inputs

`gradient_multikey_acceptance_tests.rs` provides independent literal source,
legacy-static rendering, complete JSON/LEP/VIEW, one-step history, rejection,
local stop identity and budget checks. Generated files are inputs and independent
references, **not evidence of native UI interactions**.

Run Cargo serially with the other desktop gates. The existing dev environment is
`/workspace/shared/libreeffects-dev-env.sh`. Debug metadata overrides below apply
only to the desktop test package, not to the optimized release or core assertions.

```sh
. /workspace/shared/libreeffects-dev-env.sh
cargo --config 'profile.test.package.libre-effects-desktop.debug=0' \
  --config 'profile.test.package.libre-effects-desktop.strip="debuginfo"' \
  test -p libre-effects-desktop --locked gradient_multikey_acceptance_tests -- --nocapture
```

## Immutable generated inputs

Use a new explicit absolute directory. Existing equal bytes are checked rather
than rewritten; differing bytes fail. Keep these files separate from actual GUI
saves and do not reuse the previous interpolation qualification root.

```sh
LIBREEFFECTS_EXPORT_GRADIENT_MULTIKEY_FIXTURES=/workspace/shared/libreeffects-qa/gradient-multikey-20261004/generated \
  cargo --config 'profile.test.package.libre-effects-desktop.debug=0' \
  --config 'profile.test.package.libre-effects-desktop.strip="debuginfo"' \
  test -p libre-effects-desktop --locked export_gradient_multikey_acceptance_fixtures \
  -- --ignored --exact editor::gradient_multikey_acceptance_tests::export_gradient_multikey_acceptance_fixtures --nocapture
```

`cases.json` lists explicit frame/sample references and bounded planned native
actions. Native sources have 120 frames, keys at 0/15/45, three visibly distinct
complete snapshots, outgoing Hold/Linear/Smoothstep modes and an enlarged Timeline.
The Animated filter is transient and must be selected in the UI. A native frame
override below changes only the declared expected frame, never source or other
VIEW fields. Selection and internal clipboard are deliberately not persisted.

The current/previous CLI helper requires Pillow. Its output directory must not
already exist. The previous binary is the frozen schema57 interpolation release:
all new edited sources must render identically because this editing milestone
changes no schema or renderer. Each new source is also compared to an independently
materialized legacy-static oracle and its JSON version.

```sh
/opt/codex/runtimes/codex-primary-runtime/dependencies/python/bin/python3 \
  apps/desktop/qa/gradient_multikey_cli.py \
  --fixtures /workspace/shared/libreeffects-qa/gradient-multikey-20261004/generated \
  --output /workspace/shared/libreeffects-qa/gradient-multikey-20261004/cli-generated \
  --binary /workspace/shared/libreeffects-qa/gradient-multikey-20261004/libre-effects-gradient-multikey-release \
  --previous-binary /workspace/shared/libreeffects-qa/gradient-interpolation-20261004/focus-final/libre-effects-gradient-interpolation-release
```

## Actual native saves

Use an actual GUI save, an independently declared generated expected source, and
an explicit expected frame. Record source/build pins alongside each actual save.
Saving invalidates the internal key clipboard, so complete Copy → Seek → Paste
(and optional repeated Seek → Paste) before saving. Save repeated-paste evidence
against `native-pasted90-repeat-expected.generated.lep`.

```sh
LIBREEFFECTS_GRADIENT_MULTIKEY_NATIVE_SAVE=/absolute/actual-save.lep \
LIBREEFFECTS_GRADIENT_MULTIKEY_EXPECTED=/absolute/generated/native-pasted60-expected.generated.lep \
LIBREEFFECTS_GRADIENT_MULTIKEY_EXPECTED_FRAME=60 \
  cargo --config 'profile.test.package.libre-effects-desktop.debug=0' \
  --config 'profile.test.package.libre-effects-desktop.strip="debuginfo"' \
  test -p libre-effects-desktop --locked verify_recorded_native_gradient_multikey_save \
  -- --ignored --nocapture
```

The native verifier checks complete source, complete VIEW, exact container version
and thirteen full-RGBA frames on five routes against an independent static oracle.
It applies no masks or tolerance. Use separate references for any intentionally
changed workspace settings; do not weaken the verifier to hide a mismatch.

Generic ignored-media gates must exclude these two explicit-input tests:

- `export_gradient_multikey_acceptance_fixtures`
- `verify_recorded_native_gradient_multikey_save`

For a supplementary frozen-binary check of actual saves, run
`gradient_multikey_native_cli.py --qa-root /absolute/qa-root --output /absolute/new-directory --binary /absolute/frozen-release`
with optional `--previous-binary /absolute/interpolation-release`. It reads the
same thirteen frames from generated/cases.json. The root's native-cases.json is
an explicit array of `{name, reference, frame, recorded_source, recorded_release}`
records, with actual saves at `native/<name>.lep` and planned expected sources at
`generated/<reference>.generated.lep`. This CLI comparison supplements, and does
not replace, the complete source/VIEW and independent-static native verifier.
