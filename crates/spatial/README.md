# Native joined XYZ mathematical foundation

This is an intermediate, independently testable crate. It does not by itself
add native 3D layers, camera rendering, host integration or a new saved schema.
Those integration changes are being developed separately. The current milestone
has 20 passing pure Rust tests, cargo check, formatting and whitespace checks.
No AE numerical parity or native UI qualification is claimed. No supplied program
or AEP data is copied into the synthetic tests; AEP opening is outside scope.

`SpatialPosition3` owns one XYZ value and sorted joined keys. Each key retains
independent incoming/outgoing modes/ease, relative XYZ handles and explicit flags.
Native new-key defaults are linear, zero handles and independent/manual modes.
These are native defaults, not inferred After Effects defaults. Automatic modes
and temporal-continuous generation reject explicitly. Manual spatial continuity
allows antiparallel or zero handles on active interior sides; dormant endpoint
and hold-side tangents are retained. Structural validation permits intermediate
metadata restoration; sampling compatibility is checked separately.

The sampler combines temporal easing in distance units with bounded spatial cubic
arc-length inversion. A global uncertainty budget refines the widest unresolved
leaf first; final reconstructed bounds still enforce the requested tolerance.
Chord/control-polygon enclosures include roundoff guards;
equal endpoints can make a real excursion. Default relative distance tolerance is
1e-7 of initial control-polygon length (absolute tolerance 0), with 65,536 splits,
40 subdivision levels and 64 inversion steps. Public options have hard ceilings.
Exhausted precision/work limits are errors, never silent line/chord fallback.
Tiny finite values and exact endpoints are retained. Scalar signed Opacity ease
is a separate future integration; spatial speeds are nonnegative distance rates.

Tests include an independent analytic XYZ parabola, an equal-endpoint 8h/9 arc,
100% influence, zero derivatives, dormant metadata, staged restoration, strict
numeric-frame duplicate rejection, retiming and source serialization. JSON
consumers need serde_json's float_roundtrip parser feature to preserve tiny
floating-point values bit-exactly; the native core already enables it.

Run with the pinned workspace toolchain and one compiler:
`cargo test -p libre-effects-spatial --locked --offline -- --test-threads=1`.
