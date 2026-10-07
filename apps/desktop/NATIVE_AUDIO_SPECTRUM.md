# Spectrum renderer and controls — bounded release/native qualification passed

Updated 2026-10-07 UTC. Production source **caf7554** connects selected-source
analysis to the shared renderer from the authored project and exact local sample.
Following filters consume an explicit generator result. Static controls expose
source binding, numeric settings, display/side and Composite Original with current
context receipts and key-down ownership. Footage refresh retires the PCM cache.

The shared frame caps are64 requests,2,000,000 PCM scheduling units,16,000,000 DSP
units and65,536 retained output bands. These are deterministic work/admission
limits, not elapsed-time guarantees. Geometry is clipped to the nominal layer
rectangle and bounded to8 MiB per fragment under the existing64 MiB frame limit.

## Final release/native result

Clean `17d0287845e2`, build `20261007.003312-7839c4a0ca7cd80e`, is qualified by
one canonical release (639.448s), ten final CLI cases and the bounded Linux native
workflow. All 811 source inputs and native About match. The CLI compares
5,591,040 pixels exactly, verifies 12,288 zero RGBA pixels for None, and preserves
an existing output on the expected missing-media error. Raw execution and
independent comparison remain separate; no extra helper render or test replay ran.

Native actual frame4360 renders using extracted relative audio and exact private
fonts. Bands960/history, None/rebind, held Enter Composite activation, locked
rejection, selected-source mute/Undo, preset version6/source:null and final
Open→Save preserve the complete expected source, five IMAG chunks and ten aliases.
Twenty-one snapshots retain the first strict order failure and its separately
predeclared object-order-only explanation. Initial VIEW exactly matches the
source-derived 3,310-byte default expansion. Later history uses exact native bytes.

Audio On Play is explicitly blocked on Linux by “Audio preview currently requires
Windows WASAPI.” An initial Play→Composite attempt therefore had no established
playing precondition and was an ordinary paused edit, subsequently undone. It is
not counted as a playback-guard result. With output Audio Off, visual playback is
observed before the rejected click, and held Space stops without editing. Output
mute leaves selected-source analysis active; muting source110 instead produces
analyzed silence, with its possible baseline stroke rather than unassigned-source
transparency. Audible preview and audio equivalence remain unqualified.

After interruptions, the existing app/state was inspected and continued without
rebuild or restart. A second disposable copy of the verified baseline restored
frame4360 while preserving the first copy's recorded playback state. Final
Save→Open→Save is byte-identical to baseline `fb25dab5…763d77`. The app exits normally with code 0 at01:31UTC; all three app/portal PIDs
are absent. Full hashes, scope and receipts are in STATUS and
`../libreeffects-qa/reference-spectrum-release-20261007`.

## Completed source and raster checks

Canonical all-target checks passed in1m05s for renderer integration and30.63s after
controls. Formatting/diff checks pass. Focused evidence includes13 DSP,6 core,
18 selected-PCM,16 geometry,2 extracted control-planning and3 Difference cases.
The control helper excludes GPUI activation/render callbacks. No full workspace
or monolithic desktop test binary was run under the serialized resource plan.

Six fixed90ms windows from the supplied MP3 were compared with a full sequential
FFmpeg decode. Exact source length is12,614,447 stereo frames. Five windows are
bit-identical; one right-channel sample differs by1.4551915228366852e-11 due to the
existing tiny-interpolation cutoff. All51,840 channel samples meet the predeclared
2e-6 tolerance, including boundary padding. The optional ptrace attempt was denied
before execution; the successful gate records helper-observed process arguments
without claiming OS tracing. This qualifies seek/window consistency, not AE audio.

A separate optimized, GPUI-free helper imports the complete production rendering,
selected PCM, DSP and supervised expression paths. Its initial dev build passed
in33.69s, the optimized build in5m38s, and two oracle helper bins in4.69s. External
video decoding, media import/path rewriting and an unused output-settings type are
explicit exclusions. This is not a desktop release or native-window qualification.

Five predeclared128x96 literal-SVG cases compare61,440 exact RGBA pixels: one-band
Line/Above, clipped Bars/Both, tilted Points/Below, overlapping Points/Both with
original/alpha, and Difference over an opaque backdrop. Expected coordinates use
an independent1500Hz half-amplitude signal calculation; Difference uses an integer
formula. The actual f32 waveform's independently measured amplitude is
0.49999999082226493; no geometry or tolerance was adjusted after output. Literal
edge coverage shares pinned resvg, so this does not independently qualify that
rasterizer. An unassigned source succeeds with zero RGBA; a missing selected file
returns a contextual error. The initial harness expected the words "selected
source" while the error identifies "source Some(1)"; that harness failure is kept,
and the existing result was qualified without rerunning or changing production.

The actual partial Black root rendered at4360 and6379 in0.942s and0.941s respectively,
with preview/export equality, exact PostScript-face checks across all compositions,
empty stderr and unchanged inputs. The prior no-Spectrum draft matches all
1,843,200 earlier RGBA pixels. At4360 the new line changes8,902 pixels only inside
x0..1919/y900..959. Both full-canvas images were inspected. No AE pixel oracle exists.

Independent complete-source audit passes175 checks and eight auditor sensitivity
cases. All115 prior layers, assets/media/fonts, image aliases, VIEW and five IMAG
chunks are preserved. Root7 gains layer116 with the agreed source110/native settings.
Its source-derived relative stack is Lyric, Player, Spectrum, Background. The first
private constructor placed Spectrum on top; that superseded artifact and audit are
retained. Correcting the stack changes8 pixels at4360 and130 at6379, with no other
source-value change or production edit. The extended auditor rejects the old order.
The only extra storage change is comp8 work_area:null becoming omitted, with the
same effective range. The project has8 compositions/116 layers and schema79.

## Frozen inputs and reproducible bounded protocol

Source tree: `LibreEffects-native-spectrum79`, production `caf7554`. Private inputs
and expectations: `../reference-spectrum-private-20261006`:

- `checks/`: raw source/model/PCM/DSP/control gate receipts.
- `render-probe/execution-provenance.json`: imported-source/helper/binary hashes,
  build logs, actual receipts and explicit exclusions.
- `pixel-oracle/`: immutable prepared WAV/literal SVGs/manifest, typed native
  fixtures, independent exact comparator and separate source-state diagnostics.
- `black-root-draft/stack-corrected/Black-theme-NativeSpectrum-PARTIAL.lep`: actual schema79 input,
  SHA256 00ffdf0d25bdcddb770da8ac4dad126dd1a6d349754acdc97c05665d97ff8774.
- `black-root-draft/stack-corrected/frame4360/opaque.png`: actual frame proof,
  SHA256 a51c9436c2d7123405d6480a775c07194e43eaee428ab135b32932794385d236.
- `spectrum-project-audit/`: independent source oracle and declared normalization; `ordered-v2` holds the final order-aware audit.
- `final-cli-plan.json`: ten frozen final-binary cases with exact inputs/expectations.

The staged corrected LEP is a construction input; the helper resolved its relative
audio against the original draft media directory. Native/CLI qualification must
use the separately assembled portable package containing sibling media/fonts.

The final canonical release repeated the five exact literal comparisons,
source-None/missing-media cases, actual4360/6379 and the prior Black regression.
The reproducible native scope is: open the copied portable actual root, observe named source
and1920 bands, edit/revert one numeric setting and source None/rebind with exact
history, verify preset binding is cleared, locked/playing input guards and fresh
keyboard ownership, then Save/Open/Save source/image-alias preservation. Include
the earlier playing-click/Space ownership regression for a Spectrum control.
Native stale-event and cancellation claims must be limited to gestures actually
observed; no synthetic internal event injection is implied.

The canonical release and bounded native input/source workflow are qualified
above. Audible Linux preview, full-frame reliability, Windows native behavior,
encoded export, AE DSP/popup/softness/hue calibration and full theme reproduction
remain unqualified. Private original inputs, fonts/media and program payloads are
excluded from source commits and source archives.

# Foundation checkpoint cc768e1 (historical)

That earlier intermediate checkpoint added the typed model, selected-source PCM
path, Difference blend and deterministic NativeV1 analysis. Generation and desktop
authoring were pending at that checkpoint. The current integration status is
recorded above; neither checkpoint establishes AE-calibrated behavior.

## Explicit model and analysis contract

Audio Spectrum has static typed settings and a stable source-layer reference in
its owning composition. Unassigned is a distinct state. Dangling, cross-composition
and non-audio references fail validation, including bypassed instances. New settings
and Difference require project version79; old projects without either retain their
storage version and omitted defaults. Portable Spectrum presets require version6
and intentionally clear the project-specific source binding for explicit rebinding.

Duplicate/clipboard/composition operations remap copied source identities.
Precompose and source splitting require the complete dependency closure. Removing
a referenced source fails atomically unless its consumers are removed too. Rename,
reorder and mute preserve identity. Settings no-ops preserve pending Redo.

The first generator target is a2D Rectangle or Solid. One enabled Spectrum must be
the first enabled pixel effect, with no legacy effects, masks or track matte.
Following ordinary effects can consume the generated result. The filter builder
requires an explicit generator receipt; it cannot silently ignore an unprocessed
Spectrum effect.

SelectedLayerOutput samples the selected instance and its authored audio subtree,
including continuous trim/origin/remap/audio matrices. Ancestors outside that
subtree and unrelated output voices do not contribute. Analysis receives unclipped
48kHz stereo PCM and does not update master playback meters. The shared decoder
retains the existing FFmpeg command/history/local-protocol contract. The cache pins
source metadata/stamps and checks dependencies even on coefficient-cache hits.
Missing/changed media, cancellation and nonfinite samples remain errors.

NativeV1 is an explicitly chosen native algorithm: a centered1–1000ms window,
periodic Hann, zero-padding to a power-of-two FFT, coherent-gain-normalized one-sided
magnitudes, RMS combination of channel magnitudes and linear frequency interpolation.
It has no hidden master clipping, normalization, dB floor or temporal history.
Bands1–4096 are supported independently of FFT size. Analysis preserves amplitudes
above one; later geometry owns height clipping. The pure crate documents bounds,
work estimates, cancellation and numeric qualification in
[crates/audio-spectrum](../../crates/audio-spectrum/README.md).

Difference uses the W3C separable absolute channel difference with the existing
premultiplied source-over equations. This does not establish AE color-space parity.

## Focused intermediate checks

- The std-only DSP crate passes13 synthetic tests, its crate check and formatting.
  A separate direct-DFT oracle checks the zero-padded interpolation result.
- Six core cases pass for schema/bounds/unknown settings/no-op history, source
  admission and atomic deletion, duplication/clipboard/composition remapping,
  split/precompose closure, preset rebinding and Difference version preservation.
  The first test compile used an incorrect existing audio-switch command name;
  the corrected test uses SetAudioEnabled and the initial receipt is retained.
- The selected PCM helper passes17 tests: five existing mixer cases, three cache
  cases and nine selected-source cases. It compiles complete production audio
  modules against actual core with panic-only decoder/process stubs. No user
  audio was decoded or played; ignored FFmpeg/export integrations were excluded.
- Three pure Difference helper cases pass, retaining the existing five modes'
  literal expectations. The helper is not a full renderer qualification.

Raw receipts are under `../reference-spectrum-private-20261006/checks`.
Desktop all-target, actual-media comparison, integrated raster and native
application qualification had not run at that foundation checkpoint. Current
completed and pending stages are recorded above.

## Actual-project limits

The selected source identity and authored1920-band/20–800Hz/90ms values can be
retained. AE Display/Side/Composite defaults, softness/hue interpretation and its
spectral transfer function remain unverified. Native defaults cannot be labeled
recovered AE settings. Missing movie pixels and unresolved source-less lyric-box
mask coordinates also remain explicit limits on final-theme reproduction.

A separately saved whole-canvas partial draft uses only already-supported layers
and explicitly omits Spectrum and the other unresolved pieces. Its private data,
fonts, media and source programs are not part of this repository.
