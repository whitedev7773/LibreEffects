# Joined planar Position (schema 75)

Joined XY Position now has an authoritative two-coordinate track separate from
XYZ/3D mode. It retains manual per-side interpolation/ease, relative spatial
handles, continuity flags, and an absent static base for keyed source data.
Equal endpoint values do not imply a stationary path. The existing bounded
spatial sampler handles distance and time; the internal zero-Z adapter never
becomes an authored 3D layer or camera.

Composition FPS reaches world transforms, parenting, preview/export and picking.
Local planar parenting retains source coordinates; ordinary world-preserving
parenting keeps its existing contract. Hidden or zero-opacity parents still
contribute transforms to visible children. Scalar X/Y edits, generic key/graph
operations, ambiguous dimension changes, final-key deletion and incompatible
FPS clipboard operations reject explicitly. Native Position readouts identify
joined XY rather than XYZ. Geometry editing uses the explicit vector commands
or the existing bounded scripting methods; the canvas remains selectable.

The scripting property reports `TwoD_SPATIAL`, two coordinates, one ease per
side and `threeDLayer=false`. Manual metadata, inactive-comp metadata queries,
expression value reads and complete transaction rollback use the native source.
Automatic temporal/spatial modes remain unsupported. Legacy scalar XY and
schema-72 XYZ storage are unchanged when the new field is absent.

The generic `reference_title_project` example appends a reviewed runtime data
contract to an existing native LEP. It verifies file hashes in-process and does
not contain user text, source programs, media, fonts or AEP decoding. The private
actual-project increment preserves the prior Lyric source and adds the original
16 title/artist layers with 13 parent links, 100 Opacity keys and 48 planar keys.
No original expression program is evaluated by that constructor. Missing
authored properties, inactive raw fields, the unused legacy Opacity placeholder
and unverified AE shaping/interpolation details remain explicit project evidence.
This is still a partial reconstruction; player graphics, other compositions,
Spectrum and unavailable footage remain outside this artifact.

## Source checks

- Spatial library: 30 tests passed, including 20 existing XYZ cases.
- Focused planar model/host gate: 15 tests passed.
- Canonical workspace all-target check: passed in 1m 44s.
- One final serialized editor-model gate: **296/298**, with unchanged 100ms
  expression deadlines. `jsx_numeric_expression_assignment_and_value_reads_use_evaluated_results`
  and `native_192_bindings_share_six_sources_and_keep_independent_contexts` failed
  with execution-budget errors. No retry or limit increase is claimed.
- Independent raw-source audit of the private increment: 4,557 checks passed;
  all 148 new key records were compared with original bytes, and original
  composition/assets/programs/view remained unchanged.
- Standalone production-renderer proof: four literal SVG pairs match all
  144,000 RGBA pixels; four projected picks pass. This covers zero-opacity
  parenting, ordinary 2D stack order, explicit nested FPS and agreement of
  render/preview/output with unchanged source. The probe excludes expression
  execution, video decoding and interactive text selection.

## Final release/native qualification

Production f1cdddc4, build 20261006.154142-0bf61076f038e663, passed one canonical
measured release in 756.814s. Final-binary title frames 4359/60 visibly render with
exact supplied fonts; Lyric 4360 retains all 1,701,120 previous RGBA pixels. Four
planar reference pairs retain all 144,000 RGBA pixels. Native Song properties
identify joined XY/40 keys/read-only, and the actual title frames render.

A guarded synthetic rename survives complete schema 75 IPC with every other
source value/key/parent/chunk preserved. One Undo/Redo and Save/Open/Save restore
complete native files exactly. Constructor-to-first-Save object order and VIEW
materialization are explicitly diagnosed rather than claimed byte-identical.

A transient Lyric expression-budget error occurred during Undo/Save→Reopen and
recovered on the next observation without input. Its exact render attribution
within that transition is uncertain; the known 100ms reliability issue and the
296/298 model result remain open. No runtime budget was changed. The saved app
closed normally at 15:59 UTC. Full evidence is in the sibling
`libreeffects-qa/reference-name-artist-release-20261006/QUALIFICATION.md`.
No external AE reference image exists, so actual rendering does not establish
AE pixel parity or full-project equivalence.
