# Compound Gradient Colors pointer qualification

`gradient_pointer_acceptance_tests.rs` independently authors complete snapshots,
ordered paint-local stop identities and Hold/Linear/Smoothstep modes. Expected
retimed source is built from literal fixture data; neither production MoveKeys
nor the production sampler supplies an oracle. Static legacy gradients provide
full, unmasked RGBA references. Tests cover all Fill/Stroke × Linear/Radial paints,
selected-old overlap, true noncontiguous selection, topology Hold, bounds,
collision/lock rejection, complete source/VIEW/JSON/LEP and atomic history.
Controller gesture tests additionally qualify source-neutral provisional states,
return-to-origin, zero-frame motion, cancellation and Redo retention. Native
observations are a separate evidence class.

Run Cargo serially with the other desktop gates. These test-package-only metadata
overrides do not alter assertions, core features or the normal release profile.

```sh
. /workspace/scratch/99b390616904/libreeffects-dev/dev-env.sh
cargo --config 'profile.test.package.libre-effects-desktop.debug=0' \
  --config 'profile.test.package.libre-effects-desktop.strip="debuginfo"' \
  test -p libre-effects-desktop --locked gradient_pointer_acceptance_tests -- --nocapture
```

## Immutable input and reference export

Use a new explicit absolute output root. Equal existing bytes are verified;
changed bytes fail, never overwrite. Generated files are input and expected
source, not evidence that a native interaction occurred.

```sh
LIBREEFFECTS_EXPORT_GRADIENT_POINTER_FIXTURES=/absolute/qa-root/generated \
  cargo --config 'profile.test.package.libre-effects-desktop.debug=0' \
  --config 'profile.test.package.libre-effects-desktop.strip="debuginfo"' \
  test -p libre-effects-desktop --locked export_gradient_pointer_acceptance_fixtures \
  -- --ignored --nocapture
```

Default native source is 400×240, duration120, frames0/15/45, three distinct
complete snapshots, and outgoing Hold/Linear/Smoothstep modes. The native VIEW
contains explicit active composition defaults, Select tool, frame0, 1× preview
and Timeline, and Timeline workspace fraction0.52. Choose the transient Animated
filter in the actual editor. Standard expected states include overlap+15,
noncontiguous0/45+10, backward15/45−10, all+10 and all clamped at74/89/119.
Locked and ordered-topology fixtures are separate.

`cases.json` lists 208 render cases (four paints × four states × thirteen sample
frames) and nine planned native scenarios. The current/previous CLI helper uses
Pillow and a new output directory. The resumed qualification uses the recovered
prior SVG-inline-style release `fb8b03a` as the compatible comparator; it is not
the preceding multi-key release binary. Its existing source schema and rendering
support these fixtures. Pin the recovered binary's verified hash in the report.

```sh
python3 apps/desktop/qa/gradient_pointer_cli.py \
  --fixtures /absolute/qa-root/generated --output /absolute/qa-root/cli-generated \
  --binary /absolute/frozen-pointer-release --previous-binary /absolute/recovered-svg-release
```

The helper checks independent-static pixels, JSON/LEP pixels and previous/current
source/reference pixels. Input and frozen binary hashes are checked before/after.

## Measured coordinates and explicit additional expectations

Native pointer coordinates can resolve to a nearby whole frame. Record the
intended gesture, measured frame result and coordinate limitation honestly.
Before reading/verifying the actual source, independently declare the selected
original fixture frames and target earliest frame, for example:

```json
[{"name":"measured-pair13","stroke":false,"radial":false,"selected_frames":[0,15],"to":13}]
```

Set `LIBREEFFECTS_GRADIENT_POINTER_EXTRA_CASES=/absolute/extra-cases.json` and rerun
the exporter with a NEW `LIBREEFFECTS_EXPORT_GRADIENT_POINTER_FIXTURES` directory,
for example `/absolute/qa-root/generated-measured`. This adds
`native-measured-pair13-expected.generated.lep`. The original generated root
remains immutable. Inputs reject missing/duplicate/unknown source frames,
collisions and out-of-range expected destinations. The declaration must come
from the recorded chosen scenario, never extraction of source or modes from the
actual file. A nearby target qualifies the measured translation, not exact
intended-target accuracy.

## Actual native source, VIEW and pixels

Save actual GUI results separately under `native/`. Verify against an explicitly
chosen independent expected source. Only an explicitly expected playhead frame
may differ from the generated VIEW; no other fields are omitted or masked.

```sh
LIBREEFFECTS_GRADIENT_POINTER_NATIVE_SAVE=/absolute/qa-root/native/01-overlap.lep \
LIBREEFFECTS_GRADIENT_POINTER_EXPECTED=/absolute/qa-root/generated/native-overlap15-expected.generated.lep \
LIBREEFFECTS_GRADIENT_POINTER_EXPECTED_FRAME=15 \
  cargo --config 'profile.test.package.libre-effects-desktop.debug=0' \
  --config 'profile.test.package.libre-effects-desktop.strip="debuginfo"' \
  test -p libre-effects-desktop --locked verify_recorded_native_gradient_pointer_save \
  -- --ignored --nocapture
```

This checks complete source, complete VIEW, actual LEP container version and
thirteen five-route exact RGBA frame sets using an independent static oracle.
Native source is read only after the reference and declared VIEW are complete.
The verifier checks the actual file is unchanged afterward.

Supplementary frozen-binary native pixels use an explicit `native-cases.json`
array. Every record names `{name, reference, frame, recorded_source,
recorded_release}`; optional `reference_directory` is a single child directory
of the QA root, default `generated`. Native paths are `native/<name>.lep` and
references `<reference_directory>/<reference>.generated.lep`.

```sh
python3 apps/desktop/qa/gradient_pointer_native_cli.py \
  --qa-root /absolute/qa-root --output /absolute/qa-root/cli-native \
  --binary /absolute/frozen-pointer-release --previous-binary /absolute/recovered-svg-release
```

This supplementary CLI comparison does not replace full source/VIEW verification
or establish that native gestures occurred. Record build/source identity for
every actual save, and distinguish bounded complete drag observations from
held-pointer, IME, deactivation, stale-callback and adversarial timing tests.

Exclude both explicit-input tests from generic ignored-media runs:

- `export_gradient_pointer_acceptance_fixtures`
- `verify_recorded_native_gradient_pointer_save`
