# Native XYZ study

Open `native-spatial-study.lep` in LibreEffects. It is an independently authored
480×270, 30fps native project with two 80×50 rectangles and an explicit camera.
No AEP import or external media is needed. The adjacent SVG is an independent
literal reference for frame0; the spatial renderer probe compares every pixel.

The earlier red plane is at [250,135,0]. The later blue plane is at [340,135,500].
The camera is at [240,135,-500], focal distance500, principal point[240,135],
near clip1. The blue plane has half scale and paints behind red despite its layer
stack position. Inspector/Timeline show read-only native XYZ; preview selection,
Hand and Zoom use the same projection. Interactive 3D transforms are guarded.

Run `scripts/spatial-position.jsx` through File → Run Automation Script. Its
Apply button writes two joined keys on the unkeyed top blue layer, at seconds0/1,
with independent endpoint ease, dormant tangents, and tiny dormant speeds. Cancel
leaves the project unchanged. Apply is one Undo step. At frame30 the blue plane
has [300,145,0], full scale, and the ordinary equal-depth stack tie puts it above
red. Save to a new native `.lep` path to retain the bundled baseline.

This is a native fixed-axis/front-parallel study. It does not establish After
Effects camera defaults or full original-script compatibility. See
`apps/desktop/NATIVE_SPATIAL_POSITION.md` for supported and rejected operations.
