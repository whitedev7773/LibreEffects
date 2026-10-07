# Native lyric and section template

Open `native-lyric-part-template.lep` in LibreEffects and save your work under a
new name. This is an independently authored, six-minute native setup for scripts
that find the exact composition and layer names below. Both compositions are
60fps. There is no external footage, embedded image, AEP payload or imported
expression source.

## Composition and source contract

- **Lyric**, 960×540: disabled, unlocked **LyricLayer** has static uniform text
  “Your lyric line”, native 48px Wanted Sans regular, and no markers. Its origin
  and inPoint are exactly frame 1 (1/60 second). Its explicit outPoint is frame
  721 (12 + 1/60 seconds). The source is only 720 frames long before a script
  duplicates and shifts it.
- **Lyric Motion** is a disabled 2D Null in Lyric, with an explicit range of
  frames [0,720). Its Position is [80,240] and its named **Transition Frames**
  slider starts at 18 frames. Hidden controls remain available to expressions
  after their own visible range.
- **Name & Artist**, 960×160: visible, unlocked **Song** has the placeholder
  “Song title / Artist” in native 36px Wanted Sans regular. Disabled, unlocked
  **Partname** has “Section name” in native 28px Wanted Sans regular. Both have
  origin/inPoint 0. Partname's explicit outPoint is frame 360 (6 seconds). Song's
  explicit outPoint is the composition end, frame 21600 (360 seconds), so its
  later Position and Opacity keys remain visible throughout the song.
- Song and Partname are true native 3D planes. Partname is parented to Song;
  both retain identity parenting compensation. Song's local XYZ is [64,36,0],
  and Partname's local XYZ is [0,64,0]. No layer in this composition has an
  enabled expression. No visible 2D plane is mixed into it.
- Name & Artist has an explicit fixed-axis camera at [480,80,-960], focal
  distance 960, principal point [480,80] and near clip 1. It looks along +Z;
  planes remain front-parallel. At Z=0 this setup gives one source pixel per
  composition pixel. It is a native choice, not an After Effects default.

Lyric opens as the active composition and is initially blank because its source
layer and control are disabled. Select Name & Artist and view frame 0 to see Song.
Every range is explicit. Only Song, the persistent visible parent that is not
shifted by the intended workflow, spans the composition. Both disabled text
templates have short ranges so later startTime changes have room inside the
composition. Moving a source beyond its composition's duration still rejects;
the template does not silently clip or extend it.

The optional LyricLayer expressions are small native Position, Scale and Opacity
programs written for this example. They scan marker comments for **Focus**,
**Hide** and **End**, requiring all three and Focus ≤ Hide < End. Missing or
unordered named markers return the authored property value. Focus starts a short
entrance: Position rises 24 pixels to Lyric Motion, Scale grows from 86% to 100%,
and Opacity fades to 100%. The entrance duration comes from Transition Frames and
the owning composition's frame rate. From Hide to End, Position drops 16 pixels,
Scale shrinks by eight percentage points and Opacity fades to zero. Other marker
comments have no effect. These marker roles belong only to this native example.

## Regeneration and test variants

The generator writes deterministic native LEP bytes, validates the project, and
checks a full decode/re-encode source roundtrip before writing. It refuses to
replace an existing output. From the repository root, using the pinned toolchain:

```sh
cargo run -p libre-effects-editor-model --example native_lyric_template --locked --offline -- /tmp/native-lyric-part-template.lep
```

Optional flags may be combined on a new output path:

```sh
cargo run -p libre-effects-editor-model --example native_lyric_template --locked --offline -- /tmp/native-lyric-short.lep --duration-seconds 40
cargo run -p libre-effects-editor-model --example native_lyric_template --locked --offline -- /tmp/native-lyric-fps-discriminator.lep --duration-seconds 40 --name-artist-fps 30
cargo run -p libre-effects-editor-model --example native_lyric_template --locked --offline -- /tmp/native-lyric-locked-song.lep --duration-seconds 40 --locked-song
```

The 30fps option changes only Name & Artist's time base and corresponding range
and composition frame counts; Lyric remains 60fps. The locked-Song option is a
negative transaction fixture and does not change the public baseline. Use
`--without-lyric-expressions` for a static control fixture. The reusable builder
is `build(TemplateOptions) -> Result<Editor, String>` in
`crates/editor-model/examples/support/native_lyric_fixture.rs`. Stable IDs are
Lyric composition 1, Name & Artist composition 2, LyricLayer 1, Lyric Motion 2,
Song 3 and Partname 4. The returned editor has no Undo/Redo history.

This fixture uses the application's native default available font contract,
static Source Text and uniform styles. It does not establish rich TextDocument
replacement, AEP import, original expression behavior, After Effects camera or
visual parity, or whole-script compatibility. Those claims need separate checks
against the exact script and supported native host. See
`../apps/desktop/NATIVE_SPATIAL_POSITION.md` and
`../apps/desktop/NATIVE_OPACITY_TIMING.md` for the bounded source contracts.
