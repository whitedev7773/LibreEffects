# Native lyric/part JSX workflow

This milestone targets the user's supplied automation script on native LibreEffects
compositions. AEP opening/import is outside acceptance. The private 25,723-byte
original stays outside Git and is executed unchanged only against disposable,
independently authored native fixtures. Its verified SHA-256 is
`2f18aba2f1bcae471b92ca289396a6893cc22fbdaf8d4a6bb9274aa8a534863c`.

## Usable native setup

[The bundled template](../../examples/native-lyric-part-template.lep) contains
Lyric/LyricLayer and Name & Artist/Partname/Song, with explicit native origins,
short disabled template ranges, true joined XYZ Song/Partname, identity parenting
and an explicit camera. Song remains visible throughout the composition. Uniform
point text and three independently authored marker-driven lyric expressions make
the generated native content useful without relying on an imported project.
[The guide](../../examples/native-lyric-part-template.md) describes setup, motion,
duration/frame-rate variants and supported limits.

Open a copy, run the separately supplied JSX through File → Run script, enter
timestamps and three lyric lines, then choose lyrics, parts or all. The script's
last timestamp component represents hundredths of a second. Each composition
snaps its own times independently. Cancel or a rejected host operation discards
the complete draft; a successful run is one native Undo step.

## External-source acceptance

The new [external script harness](../../docs/external-jsx-harness.md) accepts a
native project, external script and explicit modal-event plan. It runs the real
production process supervisor with unchanged limits and matches controls from
actual ScriptUI trees. It never substitutes a parser or rewrites the script.
Private source, inputs, event transcripts and source-specific numeric oracles
remain outside the repository.

Initial execution/source acceptance passes 14 scenarios: lyrics-only, parts-only,
all, an all-mode 60/30fps discriminator, validation correction, empty validation,
sample/clear confirmations, Cancel, window close, external interruption, final
alert interruption and locked-Song failure. The locked variant fails after earlier
lyric/part candidate edits; the host rejection latch returns the diagnostic
immediately instead of showing the script's later error alert. No rejected draft
is installed. The first event plan expected that alert and was corrected; no
production behavior changed to satisfy it.

An independent container/JSON oracle compares the complete expected project and
all other chunks, retaining binary64 signs and tiny values. It checks generated
layer ordering/IDs, CR-separated text, removed blank layers, origin/range changes,
marker ownership, indexed labels, parents and every restored Song value, ease,
interpolation, tangent and flag. Literal key metadata is read as data from the
hash-verified original; no JavaScript is evaluated by the oracle. Successful
runs verify one Undo/Redo and byte-identical Save/Reopen. Failure/no-op paths also
prove the exact pre-existing Redo branch survives.

## ScriptUI behavior

Requested edit/button dimensions are applied within the available viewport,
with a scrolling modal body. Explicit `active=true` assignments issue ordered
focus requests consumed once by the native view. Ordinary callback revisions do
not repeatedly steal focus from Tab or buttons. Nested confirmation/alert focus
restoration and the existing held-key boundary remain separate concerns.
Focused controls are revealed within the scroll viewport. Other ScriptUI font,
alignment and general layout APIs remain a bounded subset.

## Qualification boundary

Source `f560992b7a9e81940b581613a14b8e0da651680d` is now qualified by one canonical
release and a bounded native batch. Build `20261006.093422-d821fd1fcb1f455d`
contains 744 verified inputs. The unchanged original all-mode dialog runs on the
public 360-second template, with complete independent source comparison, exact
one-step Undo/Redo and byte-identical Save/Reopen. Sample/clear confirmations,
one-shot edit refocus, native typing, held-Return isolation and Tab reveal at
1180×812 and 1100×700 pass. Cancel preserves the source. Native previews show
covered Japanese/Korean three-line lyrics and parented parts.

Four independent constant-state CLI reference pairs pass all 1,344,000 pixels.
Those CLI inputs are the separately proven 40-second process result; the native
GUI uses the delivered 360-second template. References have no expressions or
animation keys but share the native font/projection/raster backend. Their zero
differences establish selected constant states, not transition or Adobe parity.
Japanese fallback is visibly working, although its exact runtime face is unknown.
See STATUS/HANDOFF for the exact binary identity, evidence and retained caveats.

Fresh source gates pass 250 model cases, 14 harness/supervisor tests, three fixture
cases, 10 pure UI policy tests, formatting and the 70.846-second canonical
all-target check. The first full model run hit the unchanged 100ms pooling-budget
fluctuation; a serial full replay passes without changing any limit. Other original
modes and interrupted/failure paths retain their supervised-worker acceptance;
they were not all repeated through the native GUI.

The native fixture is deliberately uniform text. AE rich TextDocument replacement,
original template expressions/effects, Adobe camera/curve/visual parity, arbitrary
project/script compatibility, marked IME and other platforms remain unqualified.
No AEP conversion, user real-project mutation, push or deployment is required by
or included in this milestone.
