# Independent layer origins and indexed labels

This compatibility slice follows the bounded JSX/ScriptUI foundation. It adds
real persisted data for two previously rejected AE attributes, without treating
trim points as source origins or using rendered fill colors as label metadata.

## Data and API

Project schema **64** introduces two sparse layer fields:

- `start_frame: Option<i64>`: independent signed origin in composition frames
- `label_index: Option<u8>`: timeline label index, 0 through 16

Absent fields retain exact legacy serialization. Static legacy layers have origin
zero; timed legacy layers use their existing content-source origin. That fallback
is never inferred from the trim range. Native edits pin the independent origin
before a playback/source clock rebase could change it. Existing unrelated source,
IDs, assets and composition selection are retained by the timing transaction.

`Layer.startTime` is exposed in seconds through the active layer's own rational
composition frame rate. Assignments must align to a frame, fit the AE API's
±10,800-second range and be representable by the native timing model. They shift
origin, in/out range, source mapping, every existing scalar/opaque timing track,
gradient-color pose timing and layer markers by the same delta. Spatial/parent
transforms, text styles, effect values and rendered fill colors are unchanged.

Setting `inPoint` or `outPoint` only trims; it does not move the independent origin
or existing keys/markers. Native layer moves share the same timing implementation.
Duplicating/splitting keeps the origin; cross-FPS layer paste converts it with the
existing rational clock conversion. Playback speed/source-in/reverse/freeze may
rebase an internal content clock while retaining the layer's logical start time.
New static image assets placed at the playhead receive that placement origin.

`Layer.label` reads/writes an integer index. Timeline swatches and duration bars
use a fixed native display palette when a label is stored. This palette does not
import a user's customized AE application preferences. Index 0 is neutral gray;
unmodified legacy layers retain their previous swatch. Label edits never change
composited pixels by changing the layer fill. Assigning the existing logical
origin or label is a true no-op, including history/Redo and sparse storage.

## Bounded compatibility and next dependency

The current native key/range clock is still unsigned and composition-bounded.
A move that would put a stored range, key, marker or gradient pose outside the
composition **rejects atomically**. It never clips data, rounds subframes or
quietly treats startTime as inPoint. Thus a reference workflow may still reject
an intermediate layer move near the composition boundary, even if a later script
statement would shorten it. Signed/out-of-composition animation storage remains
an explicit next compatibility dependency.

Directly deserialized in-memory media projects that bypass normal loading may
still lack the asset table required by schema 64. Source-preserving commands
reject a schema-raising edit in that case. Normal JSON/LEP loading already
canonicalizes legacy assets through the existing loader, so ordinary old-file
opens do not gain a new restriction. Future AEP conversion must likewise supply
a complete canonical asset table.

The broader supplied-template goal still needs AEP ingestion, the required
expression/effect properties, rich text and authentic spatial/3D semantics.
These attributes do not make the supplied JSX/AEP fully compatible by themselves.

## Acceptance and evidence

Independently authored fixtures cover a 60 fps text template whose origin and
in-point are one frame, with an 11.65-second out-point. A synthetic lyric-like
sequence duplicates layers, assigns preceding timestamps as origins, sets final
out-points, writes Focus/Hide/End markers, and labels the last two layers with
index 4. No original JSX, expressions or lyrics are copied into the fixtures.

Final verification on 2026-10-05: **133/133 GPUI-free model tests pass** (2.17 s;
18 timing-model regressions and 5 new JSX cases beyond the 110-test foundation).
The canonical workspace/all-target check passes in **47.72 s**. Rustfmt and
`git diff --check` pass. One-job/incremental-off settings and the existing cache
were retained; no desktop test executable or aggregate suite was run.

Tests cover trim/origin independence, signed representable origins, all 17 label
indices, animation timing families, source-time evaluation, parent/duplicate/split/
precompose/paste preservation, playback clock rebases, invalid-value rollback,
no-op/history behavior, schema rejection and complete LEP roundtrip. See STATUS
for historical checkpoint context. No release/native session is part of this
slice; the prior ScriptUI native batch is qualified separately on its frozen head.

References: [Adobe layer movement and keyframes](https://helpx.adobe.com/after-effects/desktop/work-with-layers/select-and-arrange-layers/selecting-arranging-layers.html),
[Adobe scripting Layer attributes](https://ae-scripting.docsforadobe.dev/layer/layer/).
