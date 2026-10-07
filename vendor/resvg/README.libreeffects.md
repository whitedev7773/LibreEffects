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

- `cargo test --offline --manifest-path vendor/resvg/Cargo.toml --lib`
- `cargo check --offline --manifest-path vendor/resvg/Cargo.toml --no-default-features --lib`
- `cargo test --offline --manifest-path vendor/usvg/Cargo.toml --lib gaussian_blur_edge_mode_tests`

The vendor test workspace patches the same local usvg and pins dependency
versions already resolved by the application's lockfile. It does not bring in
a second implementation. Literal five-box and FIR boundary oracles, constant
premultiplied corners, zero sigma, checked domain failures and memory caps are
covered. This is a bounded native rendering contract, not a claim of complete
SVG edgeMode or all AE GPU/kernel parity.
