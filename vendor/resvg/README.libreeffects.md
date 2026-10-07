# Libre Effects resvg extension

Source: the official crates.io `resvg` 0.45.1 archive. The upstream source,
manifest, documentation, MIT license and Apache license are retained.

- Archive SHA-256: `a8928798c0a55e03c9ca6c4c6846f76377427d2c1e1f7e6de3c06ae57942df43`
- Upstream revision: `1b6c2fddbcbeffa8135df4323b02aaae84890907`
- Upstream path: `crates/resvg`

This package is vendored to implement an opt-in, checked Repeat Edge Pixels
Gaussian boundary mode without duplicating SVG rasterization or ordered filters
in the application. Legacy rendering remains the upstream path.

The local changes and focused validation are documented in subsequent commits.
Remove this patch when upstream provides equivalent typed nominal input domains,
checked failures and bounded original-input clamp extension.

## Local contract

`render_checked` accepts `CheckedRenderOptions` with a frame-local
`RepeatEdgeDomain` per executed `(filter_id, primitive_index)`. Unrelated entries
from already-baked or culled frame stages are ignored. Its rectangle is the
nominal input extent, including transparent pixels; its transform maps that
rectangle into filter user space. The actual renderer transform maps it into
the temporary filter buffer. Missing/duplicate domains, unsupported transforms,
linearRGB repeat, lost required source support, overflow and checked allocation
limits return `RenderError`. Callers must discard a failed destination.

The source domain D, primitive output O, and work halo H are distinct. H expands
O by the sum of all five box radii, is filled once from the original input using
clamp_D, then uses the upstream pass order, widths and rounding with preallocated
scratch. No pass reclamps its intermediate output. The cropped result covers O.

Upstream chooses five-box blur if either resolved sigma is at least 2. Repeat
mode follows that choice. If both sigmas are smaller, Repeat uses a normalized
separable Gaussian FIR, ceil(4*sigma) support (at most 8 pixels/17 taps), f64
accumulation and nearest-byte rounding per directional pass. This opt-in FIR is
not pixel-identical to upstream IIR. Sigma scaling and the <0.05 cutoff remain;
both zero is an identity. Legacy `render`/`render_node` retain upstream behavior.
Gaussian edgeMode=None omits the attribute in usvg output, preserving default
SVG serialization and rendering.

The native contract uses sRGB and nested one-filter stages. Arbitrary rotations,
shear and differently sized multi-filter buffers are explicit errors. Checked
state follows embedded SVG and feImage subtrees as well. Pixel/byte/live limits bound checked layers, retained
filter inputs and repeat output/halo/scratch. Existing non-repeat primitives and
asset decoders retain their upstream allocation behavior.

## Focused validation

Quality raster-image minification averages premultiplied pixel coverage before
the final bicubic/bilinear sampling. Original image bytes and image coordinates
are preserved; nearest-neighbor pixel-art modes retain their original behavior.
Synthetic rendered checkerboards and odd-sized transparent edges verify coverage
and alpha without relying on project artwork. This improves minification but does
not establish equivalence to AE's full image sampling pipeline.

- `cargo test --offline --manifest-path vendor/resvg/Cargo.toml --lib`
- `cargo check --offline --manifest-path vendor/resvg/Cargo.toml --no-default-features --lib`
- `cargo test --offline --manifest-path vendor/usvg/Cargo.toml --lib gaussian_blur_edge_mode_tests`

The vendor test workspace patches the same local usvg and pins dependency
versions already resolved by the application's lockfile. It does not bring in
a second implementation. Literal five-box and FIR boundary oracles, constant
premultiplied corners, zero sigma, checked domain failures and memory caps are
covered. This is a bounded native rendering contract, not a claim of complete
SVG edgeMode or all AE GPU/kernel parity.

## Explicit fractional-box mask feather

The private usvg profile selects three horizontal fractional-box passes followed
by three vertical passes, with transparent borders and floor-to-byte quantization
after each pass. Sliding integer channel sums make work linear in pixel count,
independent of radius. One bounded pixel scratch buffer is reused. Exact quarter
turns swap axes and directional order together; arbitrary shear/rotation rejects.
The profile never infers a radius from Gaussian stdDeviation and does not change
an ordinary Gaussian filter. Both SVG writers preserve the explicit radius.

The checked entry point is activated for this profile even without repeat-edge
domains. Input/output copies, scratch, and containing buffers share allocation
limits; errors identify the affected primitive. Literal impulse, independent
direct convolution, premultiplied channels, quarter turns, invalid radii and
memory/transform rejection are tested. Measured AE square-mask alpha remains
within two levels for the sampled Feather 20/50 probes and one for 677. Full
composition paint and AE's small-kernel behavior are separate qualifications.

## Explicit opaque layer-opacity interpolation

`data-libre-effects-compositing="opaque-opacity-byte257-v1"` on a normal group
opts into `D + trunc(((S-D)*A+128)*257/65536)` for opaque source and destination
pixels, where A is the rounded byte layer opacity. Zero retains D and 255 copies
S. Signed division truncates toward zero; a signed right shift changes the result.
Partial-alpha pairs are painted through the original tiny-skia implementation.
This is a limited mathematical profile, not a complete AE compositing contract.

The checked renderer activates for this profile without blur domains. A bounded
intersection buffer records opaque overrides before the unchanged native draw;
its bytes share the containing layer's live allocation cap. Placement intersects
in i64 before indexing, including negative and completely offscreen coordinates.
Conflicting blend modes and memory limits fail explicitly. Both usvg writers
retain the private attribute and malformed profiles reject during parsing.
An opacity-only checked tree retains the legacy ordinary-Gaussian group crop
pixels while still checking each buffer and its live bytes. Trees with explicit
box/repeat blur retain the stricter unclipped-support contract.
The fixture contains 405 literal independently captured AE byte cases; checked
SVG rendering, opacity endpoints, negative division boundaries and preservation
of unqualified partial-alpha pixels are tested. Diagnostic PNG and the official
render queue's TIFF output were independently compared for the seven gray ramps.
