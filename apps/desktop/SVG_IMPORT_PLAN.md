# Static editable SVG import contract

The bounded inline-presentation continuation is specified separately in
[SVG_INLINE_STYLE_PLAN.md](SVG_INLINE_STYLE_PLAN.md). Its literal `style` subset
is the only exception to the general CSS exclusion below. Geometry, viewport,
normalization and transaction limits in this contract remain unchanged.

## Scope and placement

E06's first bounded slice imports one local UTF-8 SVG file into one new editable
Shape Contents layer at composition origin. The SVG viewport becomes the layer
size and an editable rectangular viewport mask. Existing composition settings,
layers, assets and source metadata stay unchanged. One successful import is one
Undo transaction; cancel, rejection, stale completion and empty input change no
source/history. Every layer/Contents/mask identity is fresh in its own domain.

Supported source elements: outer `svg`, `g`, `path`, `rect` (including rounded
corners), `circle`, `ellipse`, `line`, `polyline`, `polygon`, and inert `title` /
`desc`. Geometry becomes editable cubic paths. Pinned svgtypes/usvg conversion
uses the renderer's existing f32 geometry and elliptical-arc approximation.
Named/3-or-6-digit hex/three-channel RGB colors are supported; embedded alpha
color syntax and HSL are rejected in favor of separate paint opacity.
Nested transforms and group opacity, inherited solid Fill/Stroke, fill rule,
cap/join/miter/dashes and SVG painter order are preserved. Viewport dimensions
use integer numbers/px; missing dimensions may use integer viewBox dimensions.
Fractional viewport sizes reject explicitly: current native masks multiply edge
coverage and do not faithfully preserve a fractional SVG viewport clip. Root viewBox and
preserveAspectRatio are converted to editable group transforms.

Unsupported elements/attributes and malformed data reject the entire file with
an explicit diagnostic. This includes general CSS/stylesheets/class, gradients/patterns,
resources/references/use/images, text, masks/clip paths, filters, markers,
animation, scripts/events, hrefs, nested SVG and foreign namespaces. Titles and
descriptions are inert metadata, never executable. There is no fallback raster,
partial import, external resource retrieval or script execution. Singular or
unrepresentable transforms and degenerate/unrepresentable contours reject.

## Safety and limits

The file reader checks a regular local file and reads at most 1 MiB + one byte;
there are no network or external-resource resolvers in the import route. XML is
parsed with DTD/entity declarations disabled, a 2,048-node bound, a 512-element
bound and explicit depth checks. Processing instructions (other than the XML
declaration) reject. Non-UTF8/SVGZ/HTML and unsupported namespace data reject.
Known attributes use complete parsers; no malformed suffix or unknown property
is ignored. Raw and expanded paths share a bounded 8,192-segment work budget;
coordinates and tangents must remain within ±1,000,000. Contents retains its
256-node, 8-group and 1,024-vertex-per-contour budgets. Current stroke/transform
numeric bounds apply. Excess bounds are errors, never clamps. Only safe numeric
geometry is handed to the pinned normalizer.

## Transaction and UI

The File menu and command search expose “Import SVG as editable shapes…”. The
native single-file chooser freezes a source/document/selection/input/transport
receipt. File I/O and parsing happen off the UI thread. Cancellation and stale
receipts retire the request. Pending invalid/draft input rejects before chooser
blur and again on completion. General menu/search opening retains its established
draft-commit behavior; an SVG-only pre-entry latch prevents that same pending
entry from starting an import. Source/history preservation begins at the chooser
transaction, not before an independent general menu-open commit. A dedicated core transaction validates original
and candidate budgets, uses only the necessary existing schema version, and
never invokes unrelated generic migrations. No new LEP/VIEW/address version.

## Required evidence

Separate design, core, UI, independent fixture/test and final acceptance commits.
Independent literal source SVG is the semantic/pixel baseline, never reserialized
imported geometry. Editable paths are cubic-only: lines use linear-parameter
collinear controls, quadratics use exact degree elevation, and arcs retain the
pinned renderer's existing cubic approximation. The pinned rasterizer uses
different line/quadratic/cubic stroke and antialias algorithms, so mathematically
equivalent conversion can change a few edge pixels. UI/help disclose this
normalization. Independently authored cubic-equivalent SVG is the exact pixel
oracle across renderer/codec/CLI routes; raw-source differences are separately
measured with tight per-fixture complete-image bounds, no masked pixels and no
generic visual-equivalence claim. Dashed strokes must have separate witnesses.
The initial zero-handle-line candidate and fractional-viewport candidate failed
raw-source fidelity and are retained as evidence, not relabeled as passing. Test group opacity/ordering, winding, compound paths, transforms and
strokes, viewport clipping, complete JSON/LEP round trips, Undo/Redo, fresh IDs,
source preservation, no-op/error preservation and XML/resource/size/work limits.
Run final Rust/workspace/media/vendor/check/format/release gates using the pinned
Linux environment. Pin final source, build identity, binary hash and evidence.
Perform bounded actual native import/reject/cancel/Undo/Redo/save/reopen checks
without modifying the previous saved project or user desktop. Report exact
native boundaries and remaining E06 scope rather than claiming full SVG support.
