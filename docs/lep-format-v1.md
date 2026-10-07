# Libre Effects Project (`.lep`) container v1

## Schema 73: independent native Opacity timing

Optional `Layer.opacity_timing.keys` maps composition frames to records containing
`in_interpolation`/`out_interpolation`, `in_ease`/`out_ease` (`speed` in signed
percentage units per second, `influence` in percent), `temporal_continuous` and
`temporal_auto_bezier`. Supported mode names are `Linear`, `Bezier`, `Hold`;
automatic/continuous true flags reject. The nonempty key set must exactly match
`properties.Opacity.keys`, which remains the only value store. Its legacy keys
must have Linear interpolation and empty temporal handles, preventing conflicting
timing authority. Absence retains the old scalar evaluation and wire format.

Any materialized timing requires schema 73, including dormant endpoints and
inactive compositions. Signed tiny finite speeds persist directly; no stored
FPS conversion occurs. Authored values remain0–100; raw interpolated/expression
results may overshoot and only painting clamps. Evaluated views are transient and
cannot be saved. LEP1, VIEW1/2 and existing scalar address encodings do not change;
annotated Opacity is unavailable through ambiguous scalar Graph operations.
See [the ownership contract](../apps/desktop/NATIVE_OPACITY_TIMING.md).

## Schema 72: joined XYZ and an explicit camera

Optional `Layer.spatial_position` contains a static XYZ `value` and `keys`, a map
of composition-frame strings to joined key objects. Each key stores `value`,
`in_interpolation`/`out_interpolation` (`Linear`, `Bezier`, `Hold`),
`in_ease`/`out_ease` (`speed` in nonnegative distance units per second, `influence`
in 0.1–100 percent), relative XYZ `in_tangent`/`out_tangent`, and the four boolean
`temporal_continuous`, `temporal_auto_bezier`, `spatial_continuous`,
`spatial_auto_bezier` fields. A materialized joined source removes the scalar
PositionX/Y entries from `properties`; readers reject coexisting scalar shadows.
Unknown fields, nonfinite or out-of-bounds coordinates, invalid flags and out-of-composition
key times reject. Dormant metadata and tiny finite values are retained exactly.

Optional `Composition.camera` stores `position` (XYZ), `focal_distance`,
`principal_point` (XY) and `near_clip`. It is an explicit positive-Z fixed-axis
camera for the supported front-parallel native planes. Materialized camera or
joined source, including in inactive compositions, requires schema 72. Absent
fields are omitted; native 2D documents do not acquire them. LEP1, VIEW1/2 and
legacy scalar addresses remain unchanged. Joined XYZ is not serialized as a
scalar Graph address. See [the native contract](../apps/desktop/NATIVE_SPATIAL_POSITION.md)
for supported rendering and explicit error/authoring behavior.

## Schema 71: reconstructed static character runs

Optional `Layer.rich_text` preserves a complete UTF-8 byte-range partition of
static Source Text with resolved per-character style, including exact font-face
identity. Existing layer-wide paragraph settings remain separate. Original
CR/LF/CRLF bytes are preserved. The reconstruction uses schema 71 intentionally;
unavailable earlier schema contracts 66–70 are rejected rather than reinterpreted.
See [the reconstruction contract](../apps/desktop/AE_RECONSTRUCTION.md).

## Schema 65: numeric programs and Slider Control

Layers may store sparse numeric programs with stable transform/slider targets,
exact source and an enabled flag. Nonempty program storage and Slider Control
instances require model schema 65 in every composition. Programs are limited to
16 KiB each; disabled source is retained and empty assignment removes a program.
Slider instances use existing stable effect IDs and animated Amount tracks.
Evaluated frame views are transient: core validation, JSON/native encoding,
serde serialization and editor commits reject them. Authored tracks and programs
are the only saved state. LEP1, VIEW1/2 and existing property-address formats do
not change. See [EXPRESSIONS.md](../apps/desktop/EXPRESSIONS.md).

## Schema 64: independent layer origin and indexed label

Sparse `Layer.start_frame` stores an independent signed composition-frame origin;
`Layer.label_index` stores a timeline-only integer 0–16. Either materialized field
requires model schema 64, including inactive compositions. Neither field changes
LEP container version 1, VIEW 1/2 or property-address versions. Absent legacy fields
remain absent on unchanged load/save, and assigning an existing logical value is
an exact no-op. Labels do not alter rendered fill colors. See
[the timing contract](../apps/desktop/LAYER_TIMING.md) for move/trim/source-clock
semantics and the remaining composition-bounded animation restriction.


`.lep` is the native Libre Effects Project file extension. It is a bounded binary
container for portable project metadata, optional editor view state, and embedded
images. The container version is **1**. The project's JSON schema version remains
independent (currently **73**); choosing this format does not upgrade that schema.
Schema 48 adds optional layer-wide text-paint tracks. Schema 49 adds sparse
`FontSize`, `Tracking` and `Leading` tracks in the same text-parameter map. Any
materialized typography track, including a keyless override or an inactive
composition's track, requires schema 49; RGB/Stroke Width-only maps still require only 48.
Documents without those tracks retain their otherwise required schema, including
legacy static text. Merely inspecting or focusing a sparse typography property
does not materialize a track or upgrade its project schema. Readers that only
support project schema 48 reject typography-track projects; the unchanged LEP
container version does not grant forward model compatibility.
Schema 50 adds the Contents `TrimPaths` operator with `Trim.Start`, `Trim.End`
and `Trim.Offset` scalar tracks. Its presence requires schema 50 even when disabled,
identity-valued, keyless, nested or in an inactive composition. No new chunk or
container version is introduced. Projects without Trim are not upgraded merely
by this executable, and unchanged Trim Value edits retain exact source/history.
Readers limited to project schema 49 reject Trim projects; existing typed Contents
property addresses retain address version 1 and VIEW uses its existing version 1/2
rules. The render-only fragments are derived at evaluation time, never stored as
replacement source vertices or path poses.
Schema 51 adds the ordered `LumaKey` effect, `LumaThreshold`/`LumaSoftness`
tracks and its required `luma_key_mode` (`KeepBrighter` or `KeepDarker`). Other
kinds omit that field and reject a non-null Luma mode. Presence requires schema 51
regardless of bypass, identity values, key count or active composition. Luma Key
supports sRGB pixel-bearing layers only. Unchanged validated scalar/mode edits
preserve exact source/history/assets rather than materializing a key or normalizing
an older declaration. No new LEP chunk, VIEW version or property-address shape is
needed; older readers reject the unsupported model. Luma-containing effect presets
separately use preset version 4; that is not a project/container version change.
Text `FillOpacity` and `StrokeOpacity` are independent 0–100 scalar tracks in the
existing sparse `text_parameters` map. Absence means 100% without changing
TextStyle storage. Any materialized entry requires project schema 52, including
keyless 100%, disabled paint or an inactive composition. Existing RGB/Stroke Width
tracks still require 48 and typography 49. An unchanged default-value edit does
not materialize a track or upgrade schema. LEP 1, VIEW 1/2 and typed address 1 remain
unchanged; saved opacity pins require their corresponding materialized track.

Schema 53 adds the default-skipped layer `source_text_animation` field, separate
from numeric `text_parameters` and TextStyle. It stores `strings` (UTF-8 pool) and
`timing` (opaque pool indices with Hold-only keys); `Content::Text.text` remains
the static baseline. Before the first key use the first string, then hold the
latest key. Empty strings are valid. Nondefault keyless storage, invalid indices,
non-Hold interpolation or temporal handles/modes are rejected. Off/last-key
operations bake the chosen fallback into the baseline and clear the pool/timing.
Limits are 16,384 bytes per string, 10,000 strings and keys, 1 MiB pool bytes per
layer, plus the existing 16 MiB escaped metadata bound. Live indices remain stable;
unreferenced pool storage can be reused. Every nondefault animation requires schema
53, including in inactive compositions. Static old text does not gain the field.
LEP 1 and VIEW 1/2 remain unchanged. Source Text is a nonnumeric property and cannot
be serialized as a numeric Graph address; existing pins/ranges retain address 1.

Schema 55 adds five individually default-skipped TextStyle numbers:
`paragraph_left_indent`, `paragraph_right_indent`, `paragraph_first_line_indent`,
`paragraph_space_before` and `paragraph_space_after`. Zero means the prior layout.
They are layer-wide static source-pixel values, retained but dormant in point
mode. Nonzero fields in any composition require 55 even when dormant. First-line
accepts finite −16,384–16,384; the other four accept finite 0–16,384. Only LF starts
a new paragraph; existing CRLF and Unicode line-break handling remain unchanged.
The dedicated field edit validates original/final source and metadata budgets,
preserves all unrelated source and only raises the version when necessary.
Incompatible old-schema payload combinations are rejected atomically rather than
implicitly migrating assets. LEP1, VIEW1/2 and property-address1 are unchanged.

Schema 56 appends `AnimatorStart`, `AnimatorEnd`, `AnimatorPositionX`,
`AnimatorPositionY` and `AnimatorOpacity` to the sparse `text_parameters` map.
Absence gives 0%, 100%, 0 source pixels, 0 source pixels and 100%, respectively.
Start/End/Opacity accept finite 0–100; Position X/Y accept finite ±1,000,000.
Every materialized entry requires 56, including keyless identity tracks, disabled
paint and inactive compositions. Existing scalar keyframes, interpolation,
temporal metadata, retiming and clipboard routes apply unchanged. The renderer
owns source grapheme selection and shaping-cluster mapping; core stores only
bounded scalar samples. Empty or reversed ranges and neutral effects are identity.
Animator-only scalar/key/temporal edit batches validate original/final source and metadata,
preserve all unrelated authored values/assets, and raise the schema only when
materialized data requires it. Equal sampled-value assignments do not add keys,
rewrite dormant track values or clear Redo. Incompatible historical media/schema
combinations reject atomically rather than migrating assets. LEP1, VIEW1/2 and
property-address1 remain unchanged.

Schema 58 adds `AnimatorOffset` (−100…100 percentage points, absent = 0).
It shifts both endpoints, clips each to 0…100 and does not wrap. Schema 59 adds
`AnimatorAmount` (0…100, absent = 100) and optional layer `text_selector`:
`units` is `Graphemes` (default), `Words` or `Lines`; `shape` is `Square`
(default), `RampUp`, `RampDown` or `Triangle`. Default selector storage is omitted.
Any nondefault selector or materialized Amount track requires 59, including
inactive compositions; all settings require text content. Unknown enum values
or selector fields reject. Units/shape are static; Amount supports scalar animation.

Selection samples logical unit centers in the shifted, clipped half-open range.
Characters are extended graphemes. Words use Unicode UAX #29 alphanumeric word
spans; separator/punctuation-only/emoji spans remain unaffected. Lines split only
on source LF (CRLF is one break), counting blank/trailing empty lines; visual
paragraph wraps do not add units. Within the effective range, normalized `t`
gives weights 1, `t`, `1-t` or `1-|2t-1|`, multiplied by Amount/100. Connected
shaping clusters and graphemes share their maximum weight; position is multiplied
by it and opacity interpolates from 100% toward Animator Opacity. With only
the primary selector, Amount 0 is identity. Animated Source Text is resegmented at each sampled frame; editing
geometry, shaping and LEP/VIEW/address versions remain unchanged.

Schema 60 adds `AnimatorScaleX`, `AnimatorScaleY` (finite 0–1000%, absent =
100) and `AnimatorRotation` (finite −3600…3600 degrees, absent = 0). Negative
scale/reflection is unsupported. Materialized tracks require 60, including
identity tracks and inactive compositions; absent defaults remain sparse.
They use the same scalar animation, guarded editing and history routes.

Scale then rotation is applied after shaping/layout, about the first logical
rendered glyph’s baseline origin in each protected character cluster or source
Word/Line. A wrapped source unit shares that pivot across its rendered lines.
Connected shaping clusters crossing unit boundaries merge transform units,
using their maximum influence. Scale interpolates from 1 to scale/100 and
rotation from 0 to the authored degrees. Existing per-cluster weighted Position
is applied afterward; opacity keeps its existing protected grouping/weight.
At full influence, zero scale collapses that unit’s ink. Paragraph clipping, layer transforms and
paint/layer opacity follow; paragraph layout and caret geometry do not change.
Identity channels preserve the preexisting rendering path and exact pixels.

Schema 61 adds a sparse `text_range_selectors` list and
`next_text_range_selector_id` allocator to text layers. The existing primary selector
and its scalar tracks remain unchanged and always run first. Up to seven
secondary selectors follow in list order. Each has a unique nonzero `id`,
`mode` (`Add`, `Subtract`, `Intersect`), `start`, `end`, `offset`, `amount`, and
`selector` containing the existing `units`/`shape`. These fields remain the static
baselines. Start/End/Amount accept finite
0–100; Offset accepts finite −100–100. Shift, clip and unit-center weighting are
the same as the primary. The allocator defaults to 1 and is omitted until used;
it stays above every stored ID, survives removal, and rejects overflow. Empty
lists are omitted. Nondefault allocator/list storage requires schema 61, including
inactive compositions; malformed IDs, lists, unknown fields and nontext storage
reject. Empty `parameters` maps are omitted, so all schema-61 static selectors
retain their exact serialized shape.

Schema 62 adds optional sparse `parameters` to each secondary selector, keyed by
`Start`, `End`, `Offset`, or `Amount`, with ordinary scalar `value`/`keys` tracks.
A missing channel samples its corresponding static baseline. A stored track,
even one with no keys, requires schema 62; absent tracks do not upgrade a static
project. Each track permits at most 10,000 keys, finite in-range values, valid
interpolation/temporal metadata, and key frames inside its composition. Both
active and inactive compositions validate before edits or persistence. Source
baselines are validated even when a track overrides them. Unknown parameters,
stale selector IDs and invalid source versions reject without repair.

The numeric property path is `TextSelector { selector: <stable ID>, parameter }`.
It is independent of selector list position. Generic value, key, interpolation,
move, paste, delete, scale and temporal-handle commands use this address; the
pinned primary retains its original `Text` addresses. Untracked value edits
change the static baseline; explicit key/animation commands materialize tracks.
Unchanged sampled values do not insert keys, change dormant source values or
clear Redo. Removing a selector also removes its tracks in the same transaction;
Undo restores the complete selector. LEP1 and VIEW1/2 remain unchanged. Numeric
graph-address version 2 adds the stable `TextSelector` property variant; version 1
continues to encode existing properties and rejects the new variant. Secondary
addresses reject zero and maximum-u64 IDs.

At each frame, evaluate and clamp each secondary scalar to its own bounds before
adding Offset and clipping Start/End to 0–100. Authored curve overshoot remains
stored. Disabling animation bakes the bounded visible scalar sample.
For each source grapheme, initialize influence from the sampled primary selector.
Then fold secondary weights in order: Add is `min(1, influence + weight)`, Subtract
is `max(0, influence - weight)`, and Intersect is `influence * weight`. An Add
selector can therefore affect text when the primary range is empty or Amount 0.
Only after composition do connected shaping/grapheme clusters receive their
maximum combined weight. Primary Units still determines Scale/Rotation groups
and pivots; secondary Units controls only that selector's contribution. Reordering
secondaries never rebinds the primary's keys or another selector's values.
Source Text is resegmented at the sampled frame. Add/remove/reorder/value edits
are atomic history operations; an unchanged edit does not clear Redo. Old files
without secondary storage keep their source version and exact single-selector
rendering path.

Schema 63 adds up to three independent additional text animators in an ordered
sparse `text_animators` list, plus a durable `next_text_animator_id` allocator.
The original primary animator remains pinned first in its existing fields,
including its ordered animated secondary selectors and unchanged `Text` and
`TextSelector` property addresses. Neither primary values nor selector IDs are
migrated, copied into the new list or renumbered. Each additional animator has a
nonzero layer-local stable `id`, an optional `selector` (Units and Shape), and
optional sparse `parameters` keyed by the ten existing animator `TextParam`
variants: Start, End, Offset, Amount, Position X/Y, Opacity, Scale X/Y and Rotation.
Other text parameters and nested secondary selectors are rejected. Missing
channels use neutral defaults (Start, Offset, Position and Rotation 0; End,
Amount, Opacity and Scale 100). Default selector configuration and empty maps
are omitted. Values use the existing channel bounds and animation semantics.

Additional animator storage or a noninitial allocator requires schema 63. Empty
new storage with allocator 1 remains absent in older sources; adding a neutral
animator advances only the schema and the necessary new fields. Removing the
last animator keeps the allocator and schema requirement, so stale addresses
cannot bind to a subsequently added animator after save/load. IDs cannot be zero
or maximum-u64, must be distinct and less than the allocator, and are preserved
when reordering or duplicating a layer. Duplicates have independent layer IDs.

The extra numeric path is `TextAnimator { animator: <stable ID>, parameter }`.
Desktop Graph address version 3 adds the `text_animator` variant with its stable
animator ID and animator-only parameter. Versions 1/2 reject that variant; v1
also continues to reject secondary-selector addresses. Writers select the
minimum address version from included pins, active channel and saved ranges.
LEP1 and VIEW1/2 stay unchanged; ordinary legacy addresses remain version 1.
All ten channels support generic value/key edits, interpolation, move, paste,
delete, time/value scaling and temporal handles. Paths reject non-animator
parameters and removed identities. Numeric edits materialize only changed
channels; unchanged bounded samples preserve sparse/dormant values, keys, source
bytes and Redo. Layer shifts and cross-frame-rate layer paste retime these tracks
and their temporal slopes with other layer tracks. Add/remove/reorder and selector
configuration changes use atomic history; invalid input cannot partially mutate
or silently repair invalid source data in active or inactive compositions.

Sampling returns the legacy primary first, then additional animators in stored
order. Each extra range independently evaluates Start, End, Offset and Amount,
clips the offset endpoints to 0–100, and uses its own Units and Shape. Its sample
has no secondary ranges. Authored curve overshoot is retained; displayed samples
are bounded and disabling animation bakes the visible bounded value. LEP1 and
VIEW1/2 framing stay unchanged.

The renderer applies Primary then extras in list order. Each animator independently
protects the original shaping/grapheme clusters and uses its own Units to choose
Scale/Rotation groups. Its pivot is that original source unit's first logical
rendered-cluster baseline, including across visual wrapping, in layer-local
coordinates. A step applies weighted Scale, then weighted Rotation around this
unchanged original pivot, then weighted Position. Each step left-multiplies the
accumulated matrix, so later transforms also transform earlier translations.
Glyph geometry is materialized once after the full affine composition. Opacity
multiplies the independent factors `1 + weight * (opacity/100 - 1)`; retained
geometry bounds do not depend on opacity. Identity or empty animators are skipped,
and zero or one active animator retains the exact legacy rendering route.

The base/all-stored-pose vertex transform changes only existing VectorPath data.
Its scope is transient editor state, not a serialized field or version change.
All pose slots/references/timing and the declared project version remain intact;
LEP, VIEW and property-address versions are unchanged. Unused/dormant poses remain
authored data and are validated/transformed rather than dropped or re-interned.

A numeric project schema newer than this reader's supported maximum is rejected
with both version numbers before model deserialization or asset resolution. This
also covers future model variants and large unsigned version values. Native
container/CRC/size/strict-JSON checks remain prior boundaries; missing, malformed
or older versions retain their own existing diagnostics.
The codec retains the project's declared version on unmodified load/save.
Existing generic edit commands may recalculate the version required by the
resulting features; a changed static edit can therefore lower an overdeclared
version without losing data. This is distinct from container conversion and from
the no-op paths that preserve the exact source and history.
Existing `.lfe.json` files and `Project::to_json` / `Project::from_json` stay supported.

All integer fields are unsigned and **little-endian**. Lengths count bytes, not
characters. MiB means 1,048,576 bytes. There is no alignment padding, compression,
offset table, trailer, archive extraction, or embedded directory structure.

## File header: 32 bytes

| Offset | Size | Field | v1 value |
| --- | --- | --- | --- |
| 0 | 8 | Magic | `89 4c 45 50 0d 0a 1a 0a` (`\x89LEP\r\n\x1a\n`) |
| 8 | 2 | Container version | `1` |
| 10 | 2 | Header size | `32` |
| 12 | 4 | Flags | `0` |
| 16 | 8 | Total file length | Exact length, including this header and all chunks |
| 24 | 4 | Chunk count | `1..=1002` |
| 28 | 4 | Header CRC32 | CRC32 of bytes `[0, 28)` |

Exactly the declared number of chunks follows. The final cursor must equal both
the declared file length and actual input length. A truncated header/payload,
trailing byte, extra chunk, missing chunk, or length/count mismatch is an error.

## Chunk header: 20 bytes

| Offset | Size | Field | v1 value |
| --- | --- | --- | --- |
| 0 | 4 | Tag | ASCII FourCC: `PROJ`, `VIEW`, or `IMAG` |
| 4 | 2 | Chunk version | `1` |
| 6 | 2 | Flags | `0` |
| 8 | 8 | Payload length | Exact following payload size |
| 16 | 4 | Chunk CRC32 | CRC32 of header bytes `[0, 16)` followed by the entire payload |

The checksum field itself is excluded from the chunk checksum. Unsupported
container versions, header sizes, flags, chunk tags, chunk versions, chunk flags,
or reserved values are rejected. There is no skip-unknown-chunk behavior.

### Checksums

All checksums use **CRC-32/ISO-HDLC**, as implemented by `crc32fast` 1.5.0:
polynomial `0x04c11db7` (reflected representation `0xedb88320`), reflected input and
output, initial value `0xffffffff`, and final XOR `0xffffffff`. The standard vector
`123456789` produces `0xcbf43926`. CRC32 detects accidental corruption; it does
**not** authenticate a file or prevent an intentional modification.

## Chunk types

### `PROJ`: exactly one

The payload is UTF-8 JSON using the existing project model/schema. Writers emit
compact JSON. Readers may accept JSON whitespace but reject duplicate object keys
at every depth, including equivalent escaped spellings of a key. The default
`serde_json` nesting limit remains enabled.

- Image sources contain `{"Image":{"asset":"image-1"}}` references instead of
  inline `png` strings. Each reference must resolve to an `IMAG` ID.
- An `image_assets` table anywhere in the top-level project object is forbidden,
  even an empty table. Inline `Image.png` fields in layer or asset-library sources
  are forbidden, including reference-plus-inline combinations.
- Sequence manifests use the existing `sequence_assets` table and `manifest`
  references. A manifest reference must resolve and must not coexist with inline
  `frames`. Unused declared sequence manifests are rejected.
- Missing image references and unused `IMAG` chunks are rejected. Identical image
  content under distinct IDs still counts separately against declared-image limits.
- Video/audio/sequence source paths retain their exact strings. The codec never
  opens those paths, reads media, or extracts files.

Shared metadata preparation removes image data before constructing JSON values or
serializing JSON. The native decoder resolves image references directly from a
bounded map of `Arc<str>` values; it never constructs a base64 `image_assets` JSON
table or calls the legacy JSON encoder as an intermediate step.

### `VIEW`: zero or one

The payload is independently bounded, well-formed UTF-8 JSON. Duplicate keys and
excessive nesting are rejected just as for `PROJ`. Core returns the **original
borrowed bytes**, including whitespace, without normalizing or interpreting the
view schema. The desktop application owns view-state schema validation and
normalization. Absent view state is distinct from any present JSON value.

The core's validation-only visitor does not build a value tree for array elements
or string values in `VIEW`; object keys are retained while needed for duplicate-key
checks. Neither view parsing nor its schema assumes a particular project version.

#### Desktop view metadata versions

The desktop currently reads view metadata **versions 1 and 2**. These are the
JSON root's `version`, not the LEP container or chunk version; both remain 1.
Version 1 retains the original composition/workspace fields, including one
`graph_view` with `speed` and `height`. Version 2 additionally permits optional
`graph_channels` in each composition view.

The channel object has its own address-schema `version: 1` and these fields:

- `pinned`: ordered channel addresses, at most 16.
- `active`: a channel address or `null`; one unpinned active address can add a
  seventeenth included lane.
- `ranges`: at most 17 unique entries, each with `channel` and nullable `value`
  and `speed` two-number ranges. Each range must belong to a pin or active channel.

A channel is `{ "id": <positive layer ID>, "property": <typed address> }`.
The property's `kind` is `transform`, `shape`, `text` or `audio` with a typed
`parameter`; `contents` also requires a positive `item` ID, `mask` a positive
`mask` ID, and `effect` a positive `effect` ID. `time_remap` has no additional
fields. Parameter values use the existing typed scalar enums. Geometry Path
timing is not a numeric Graph channel. Names are labels, not identities.

Unknown fields/variants, unsupported root/address versions, duplicate addresses
or ranges, malformed address/range shapes and over-limit counts are rejected.
Finite range bounds outside ±1e15, or ranges narrower than 1e-6, normalize to
automatic height. Addresses that do not resolve to an existing scalar track are
pruned against the loaded composition; reading views never creates source tracks.
Selected keys, active key, pending input, drag state and runtime-only unavailable
pin markers are not serialized.

Writers normalize a **copy** against the project. They emit exact version-1
metadata when no saved composition needs channel state, or version 2 when any
composition retains pins, an explicit active channel or per-channel ranges.
Inactive compositions participate in this choice. Save does not remove live
unavailable pins retained for Undo, and there is no sticky “once version 2” flag.
Typography channel addresses use the existing text-parameter address enum and
require an existing typography track, whose project schema is at least 49.
Runtime sparse property focus is not serialized; unavailable typography pins and
ranges are removed from the saved copy in active and inactive compositions. Thus
ordinary legacy project saves cannot emit the new typography addresses merely
because a user focused one of those rows. No new VIEW or LEP version is needed.

Older desktop readers that only understand view metadata v1 reject native files
containing v2 view metadata, even when their render-project data is unchanged.
The core container API still treats either payload as bounded opaque JSON. This
view extension does not change the render-project schema or the `.lep` extension.

### `IMAG`: zero to 1000

| Payload offset | Size | Field |
| --- | --- | --- |
| 0 | 2 | ID byte length, `1..=64` |
| 2 | 1 | Storage kind, `0` or `1` |
| 3 | 1 | Reserved, must be `0` |
| 4 | ID length | ID, ASCII `[A-Za-z0-9_-]` only |
| 4 + ID length | Remaining payload | Nonempty image data |

Image IDs are unique across the file. They are opaque reference identifiers, not
paths. IDs are case-sensitive. The remainder is the entire image; there is no
separate inner length or padding.

**Kind 0: original PNG bytes.** The data must start with the eight-byte PNG
signature `89 50 4e 47 0d 0a 1a 0a`. On load, the exact bytes are encoded using the
standard padded base64 alphabet to obtain the project's existing image string.
The encoded-byte charge is `4 * ceil(raw_length / 3)`, computed with checked
arithmetic. Raw images are at most 9 MiB, equivalent to at most 12 MiB encoded.

**Kind 1: original encoded ASCII.** The data is the exact project image string,
with no decoding, normalization, padding changes, or pixel transformation. Its
alphabet is the existing core-compatible `[A-Za-z0-9+/=]`; the core historically
permits strings that are not canonical base64 or are not PNGs, such as `YWJj`.
Its encoded-byte charge is the actual payload-string length, at most 12 MiB.

A writer chooses kind 0 **only** when all three checks succeed:

1. `base64::engine::general_purpose::STANDARD.decode(original)` succeeds.
2. The decoded bytes start with the PNG signature.
3. `STANDARD.encode(decoded) == original` exactly.

Otherwise it chooses kind 1 and stores the original string verbatim. Kind 1 is a
lossless compatibility representation, not a guarantee that the image can render.
No pixels are decoded or re-encoded by this codec. The desktop's `validate_images`
remains the final PNG-decoding, dimension, and allocation gate before use.

## Ordering, limits, and allocation discipline

Writers produce `PROJ`, optional `VIEW`, then `IMAG` chunks in lexicographic stable
ID order. Readers accept any chunk order. IDs are generated deterministically by
the shared metadata preparer; this is not a content-addressed format.

| Limit | Maximum |
| --- | --- |
| Total file | 256 MiB |
| `PROJ` payload | 16 MiB |
| `VIEW` payload | 16 MiB |
| Image chunks | 1000 |
| Reconstructed encoded bytes per image | 12 MiB |
| Sum of reconstructed encoded bytes for all declared images | 128 MiB |
| Chunk count | 1002, including `PROJ` and optional `VIEW` |
| Image ID | 64 ASCII bytes |

Lengths, counts, arithmetic, conversions, and ranges are checked before their
corresponding allocation or payload access. Chunk payload limits are checked
before hashing an advertised payload. All declared image sizes are charged before
image reconstruction or content interning; unused or duplicate-content chunks
cannot evade the budget. Chunk descriptors borrow the input until the complete
container framing and declared-image budget have been checked.

The writer reserves a single checked upper bound based on encoded image sizes,
writes metadata/view, and decodes at most **one image temporary at a time** while
appending image chunks. It then fills the actual total length and header CRC.
PNG raw data never exceeds its canonical base64 length, so this does not require
repeated growth or simultaneous raw buffers for all images.

The native JSON visitor is bounded by the 16 MiB input cap and by a value/key
budget equal to input bytes (each value/key necessarily consumes at least one
byte). It does not trust collection size hints and leaves serde's nesting limit
in place. Native duplicate-key rejection is intentionally separate from the
legacy JSON parser, preserving that parser's historical behavior.

Both formats finish loading through the same sequence: model validation, mask
migration, shape-content migration, asset synchronization, model revalidation,
and resident metadata/image budget validation. This preserves image and sequence
`Arc` sharing and ensures migrations cannot bypass final document budgets.

## Rust API

The UI-independent `libre_effects_core::project_file` module exports:

```rust
pub const MAGIC: &[u8; 8];
pub const MAX_FILE_BYTES: usize;

pub struct DecodedProject<'a> {
    pub project: Project,
    pub view: Option<&'a [u8]>,
}

pub fn encode(project: &Project, view: Option<&[u8]>) -> Result<Vec<u8>, String>;
pub fn decode(input: &[u8]) -> Result<DecodedProject<'_>, String>;
```

The codec is pure bytes. File dialogs, atomic writes, recovery selection, legacy
file detection, view semantics, and media decode validation belong to callers.

## Regression coverage

The core suite checks deterministic layout and the CRC standard vector; rich
multi-composition projects and external paths; shared image/sequence references;
exact PNG bytes and legacy-string fallback; absent/present/borrowed view state;
arbitrary chunk order; every truncation prefix and every single-byte mutation of
a small complete fixture; validly checksummed malformed headers, references,
IDs, storage kinds, duplicate chunks, JSON keys, and unsupported versions; exact
and over-limit JSON/image/count/aggregate budgets; trailing bytes; and unchanged
legacy migration and metadata-budget behavior.
