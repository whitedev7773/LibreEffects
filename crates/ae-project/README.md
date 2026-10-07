# Bounded AE project ingestion foundation

This crate was independently reauthored from recovered source baseline `e7bb28d`.
It is not a recovery of later lost implementation bytes or their verification.
All fixtures are synthetic, authored for this contract; no original user project,
script, binary payload, extracted string, or name-based reconstruction is included.

## Two deliberately separate inputs

- `parse_json(bytes, &Limits)` reads **Libre Effects interchange schema 1** and
  returns an immutable `ValidatedProject`. This JSON is not an Adobe AEP/AEPX
  schema and is not automatically produced from an AEP by this crate.
- `inspect_rifx(bytes, &Limits)` inventories big-endian container boundaries only.
  Every successful inventory has `BinarySchemaUnverified` and
  `is_convertible() == false`. It never returns a project. All non-container
  payloads remain opaque. There is no string scanning, project-name lookup,
  fingerprint reconstruction, JavaScript execution, asset loading or I/O.

A structurally valid original binary file still requires verified field schemas
and independent reference/oracle evidence before actual typed decoding can be
implemented. A valid container is not evidence that any composition was decoded.

## Schema and conversion boundary

The public types in `src/model.rs` define schema 1. `synthetic-project.json` is a
complete example with parented rich text, an explicit null source, a nested
composition and a blue solid. Its coordinates are explicitly normalized to the
native top-left convention, not inferred AE baseline coordinates.

- Items and globally unique layer IDs are nonzero `u64`; Slider IDs are nonzero
  and unique within a layer. Names need not be unique and never establish identity.
- Layer array order is front-to-back. Parent links stay within their composition;
  composition/footage links use stable item IDs. Cycles and dangling links reject.
- Time is signed `i64/u32` seconds; frame rate and pixel aspect are positive
  `u32/u32`. Rational representation is retained exactly. Equal key/marker times
  are compared with `i128` cross products, never rounded through floating point.
  Independent start origins and in/out points are retained, including negative
  times; native conversion may need to reject them rather than clamp or shift.
- Every source has explicit geometry or an item reference. Solid colors are
  finite normalized RGB. No missing geometry is inferred from source names.
- Numeric properties have an explicit dimension, optional authored value, sorted
  keys, independent incoming/outgoing interpolation, optional easing and optional
  expression source/enabled state. Missing base/ease is preserved as absence, not
  zero or guessed easing. Expressions are never parsed or run here. Slider paths
  must be `ADBE Slider Control` / `ADBE Slider Control-0001` and scalar.
- Text retains exact UTF-8 bytes/newlines, PostScript identity/family/style/weight,
  point coordinate convention, paragraph data and complete character styles.
  Nonempty run arrays must completely cover the text without gaps or overlaps;
  UTF-16 offsets cannot split surrogate pairs. Empty arrays mean the default style
  applies throughout. Native conversion must additionally reject unsupported
  shaping/grapheme boundaries, unsupported coordinate conventions, style fields and
  animation. The native converter now maps explicit point-text baseline origins to
  its schema74 baseline representation while preserving transform tracks.
- Markers preserve all declared metadata and unique times. Unsupported native
  metadata must block conversion; dropping it would violate this contract.
- Provenance is retained but is descriptive data, never a conversion selector.
  Source hashes, if supplied, are lowercase SHA-256 hex. They are not verified
  without separately supplied source bytes and never imply source trust.
- Unknown JSON fields/tags, duplicate fields/IDs, missing required fields, trailing
  JSON, invalid geometry/time/ranges, nonfinite numbers and invalid links reject.
  All semantic nullable fields must appear explicitly, even when their value is
  null: parent ID, authored numeric value, expression, incoming/outgoing ease,
  fill/stroke paint, paragraph box size, and footage frame rate/duration. Null
  retains the documented absence/unavailable semantics; omission is an error so a
  forgotten producer field cannot silently remove parenting, programs, paint or
  paragraph geometry. Only provenance `source_sha256` may be omitted. Structs
  have no numeric/style defaults. Deserialization keeps serde_json's
  built-in recursion limit and rejects duplicate object members directly, without
  first converting the input to a map that could discard them.

`composition_closure(root_id)` returns dependency-first IDs and structured
unsupported diagnostics. Unknown data outside that closure does not block it;
project-wide unknowns block every root. 3D and external footage always block this
slice. Other explicit unsupported annotations in selected text/properties/layers
also block. Empty diagnostics mean only that there are no declared blockers:
**the native converter must positively recognize and faithfully map every field**
before treating the closure as usable. Disabled layers and disabled expressions
are retained and validated too. This crate is not a completeness attestation for
an untrusted producer that omits undeclared source semantics.

## Bounds

Default limits: 16 MiB input; 4,096 items; 20,000 layers; 100,000 numeric properties;
100,000 item dependency links; 250,000 total text/numeric keys; 100,000 markers;
100,000 text runs; 1 MiB per string and 8 MiB aggregate string bytes; 4,096 programs,
16 KiB per program and 1 MiB aggregate program bytes; 10,000 unsupported features;
32,768 pixels per dimension; one year absolute time; 64 item/parent dependency
levels; 100,000 RIFX chunks and 64 chunk levels. Program strings also count against
string limits. Match-name paths have 1–16 segments and numeric vectors 1–4
components. Each marker has at most 1,024 uniquely named parameters.

`Limits` can lower budgets for individual callers. Positive frame rates are at
most 1,000 fps and pixel aspects at most 100. Style range checks are interchange
validation, not claims that the native renderer supports their entire ranges.
The file byte limit applies before JSON allocation; model validation also applies
to directly constructed Rust values through `validate_project`.

## Container framing sources

The framing checks follow the primary IBM/Microsoft *Multimedia Programming
Interface and Data Specifications 1.0* (August 1991), chapter 2, pages 2-1–2-3
([archived original PDF](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf)):
RIFX uses big-endian integers; chunk size excludes its header and even-byte padding;
odd payloads require a zero padding byte; LIST and RIFX containers begin with a
four-byte type. Checked child boundaries, exact root length and iterative depth
limits additionally narrow accepted inputs. Mixed-endian RIFF nesting rejects.

Microsoft's [RIFF overview](https://learn.microsoft.com/en-us/windows/win32/xaudio2/resource-interchange-file-format--riff-)
and [container services](https://learn.microsoft.com/en-us/windows/win32/multimedia/resource-interchange-file-format-services)
corroborate the framing. Adobe's [project documentation](https://helpx.adobe.com/il_en/after-effects/desktop/work-with-projects/after-effects-projects/projects.html)
identifies binary AEP and partially binary XML AEPX, but provides no usable payload
schema here. None of those sources establishes our interchange JSON as Adobe data.

## Tests

Run `cargo test -p libre-effects-ae-project`. The fixture tests cover lossless
roundtrips, IDs/order/times/rich text/expressions, exact closure ordering, isolated
unsupported data, malformed/unknown JSON, all budgets, parent/item cycles, a shared
dependency depth regression, nonfinite numbers, surrogate boundaries and explicitly
absent numeric bases. An omission matrix rejects every semantic nullable field, while
an explicit-null matrix accepts and preserves each documented absence. RIFX tests
check independently calculated offsets, all truncated prefixes, framing/padding/
size/depth failures and bounded mutation smoke tests.
Those mutation checks are not a replacement for a full fuzzing campaign.

## Live reference diagnostics

`apps/desktop/scripts/ae-reference-snapshot.jsx` captures the open project through
AE's scripting API. Its diagnostic schema is separate from interchange schema 1.
It preserves match-name property trees, keys, expression bytes, per-character font
styles and exposed baseline locations. Captures belong in ignored local QA output;
private media, source text and original programs are not committed as fixtures.

`cargo run -p libre-effects-ae-project --example reference_inventory -- SNAPSHOT.json [ROOT_ID]`
reports a bounded dependency inventory without opening captured media paths or
running captured expressions. `native_conversion_verified` remains false. This
report is evidence for implementation planning, not a native import result.
