# Independent cubic arc-length fixtures

`trim-arc-oracles.json` is a test-only reference for Trim Paths. It contains 23
contours, 154 cut references, and a compact connected 1,024-segment stress case.
It does not depend on, call, or import the production Trim evaluator.

## Reproduction

Requirements: Python 3 and **mpmath 1.3.0**. No Rust or runtime dependency is added.
From the repository root:

```
python3 crates/core/tests/data/generate_trim_arc_oracles.py
python3 crates/core/tests/data/verify_trim_arc_oracles.py
```

Generation has no clock-dependent fields and uses the fixed random seed
`20261004`. The JSON records Python/mpmath versions and the generator's SHA-256.
A `--case CASE_ID` option produces only selected cases; `--output PATH` and
`--checkpoint-dir DIRECTORY` support generation outside the tracked dataset.
Do not replace the full tracked file with a selected-case run.

## Mathematical method and independence

1. Every control coordinate and requested fraction starts as a binary64 value.
   `as_integer_ratio()` imports its **exact rational value** into mpmath. The JSON
   includes both Python float-hex notation and numerator/denominator strings.
2. Integrate the norm of the quadratic cubic derivative with high-precision
   **tanh-sinh quadrature**. This is neither the production chord/control-polygon
   method nor its interval-AD Simpson/Boole intersection.
3. Isolate all real roots of each derivative component and of the derivative of
   squared speed. Polynomial critical points recursively bracket roots; bisection
   refines them. Split the quadrature there. At a small nonzero speed minimum,
   introduce a geometric mesh based on speed/acceleration, expanding by eight.
   The near-cusp fixture therefore has 43 integration panels. Simply repeating
   a poorly resolved cusp integral at higher precision would not be adequate.
4. Invert cumulative arc length with **safeguarded Newton/bisection**, evaluating
   independent speed integrals at candidates. Evaluate points using the cubic
   Bernstein polynomial, not production de Casteljau extraction.
5. Repeat all integrations and inversions at **80 and 120 decimal digits**.
   Require total/segment/prefix length and point changes below
   `1e-55 * max(1, total length, maximum absolute input coordinate)`. Keep 72
   significant digits in the artifact. All input/reference arithmetic for
   comparison is performed at high precision; there is no decimal-to-binary64
   intermediate in the mathematical reference.
6. The separate verifier uses **100-digit Gauss-Legendre quadrature** on the
   root/graded panel partition to crosscheck every total, segment length, and
   cumulative prefix at each recorded binary64 parameter. It also checks the
   exact hexadecimal/rational input encodings and stress-case additivity.

The recorded uncertainty is a deliberately conservative **empirical allowance**
from convergence and crosschecks, not a formally proved interval enclosure.
Analytic straight, rectangle, backtracking, and exact cusp cases provide separate
closed-form checks. Production's outward-rounded bounds must still supply their
own enclosure proof. Reference agreement cannot establish that proof by itself.

## Coverage

- A truly constant cubic, distinct from positive-length zero-chord curves.
- A 100-unit zero-handle straight segment. Its stored parameter is smoothstep;
  the quarter cut is `t = 0.326351822333069651...`, with `x = 25`.
- Unequal collinear segments with both non-dyadic near-boundary fractions and
  **exact** dyadic cumulative boundaries (lengths 16, 48, 192; total 256).
- A closed 100-by-40 rectangle, reversed direction, and shifted first vertex.
- The actual binary64 cubic `(0,0), (1/3,0), (2/3,1/3), (1,1)`. Its length differs
  from the ideal analytic parabola by approximately `-5.6666906280287e-18`.
  That difference is retained, not erased by using the ideal parabola as truth.
- Collinear backtracking with two interior stationary roots; a noncollinear
  zero-chord loop; an exact cusp; and a near-cusp perturbation.
- Existing KAPPA ellipse and rounded-rectangle segments, both individual corners
  and complete contours. The controls include the actual binary64 products and
  position-plus-tangent additions used by the existing primitive converter.
  Ideal circle/ellipse circumference is never substituted.
- Seeded tiny, normal, and source-limit curves; a tiny zero-chord loop; million-unit
  tangent handles; and a few-ULP positive-length cubic near coordinate 999999.
- A mixed four-segment connected block and a 256-repeat stress case, giving exactly
  1,024 segments without storing duplicate reference controls 256 times.

## JSON interface

`cases[]` fields:

- `id`, `closed`, `note`
- `segments`: numeric binary64 controls, indexed `[segment][control 0..3][x,y]`
- `input_hex`, `input_ratios`: the same controls encoded exactly
- `total_length`, `segment_lengths`: decimal-string references. Their corresponding
  `_f64_lower`/`_f64_upper` fields provide directed binary64 endpoints (arrays for
  `segment_lengths`).
- `quadrature_panels`: reference integration breakpoints, not output geometry
- `convergence`: observed 80/120-digit differences, inversion residuals, and the
  conservative empirical uncertainty allowance
- `cuts[]`: numeric `fraction`, exact fraction encodings, `segment_index`, decimal
  `t`, `point`, `prefix_length`, `target_length`, and `residual`
- Each cut also has `t_f64`, `t_f64_hex`, `t_f64_prefix_length`, `t_f64_point`, and
  `t_f64_arc_rounding_residual`. These references integrate at the **exact rounded
  binary64 parameter**, so tests do not falsely assume that parsing the long
  decimal parameter preserves its arbitrarily precise arc residual.
- Each cut additionally supplies `t_f64_lower`/`t_f64_upper`,
  `point_f64_lower`/`point_f64_upper`, `prefix_length_f64_lower`/`_upper`,
  `target_length_f64_lower`/`_upper`, and
  `t_f64_prefix_length_f64_lower`/`_upper`. These bound the associated stored
  decimal strings; `t_f64_lower`/`upper` bound `t`, while the existing `t_f64` is
  its nearest binary64 value.
- `analytic_crosscheck`, when available, distinguishes the analytic comparator
  from the actual input-cubic integral.

At an exact cumulative segment boundary, the reference uses the preceding
segment at `t = 1`. A production start boundary may correctly select the following
segment at `t = 0`; compare their same cumulative arc/point, and test the specified
start/end half-open topology separately. Non-dyadic fractions close to a boundary
are **not** snapped to that boundary.

`stress_cases[]` refers to `mixed_repeat_block` by ID and supplies `repeat_count`,
`segment_count`, multiplied `total_length`, and accumulated reference uncertainty.
Expand the block in traversal order; its final endpoint equals its first exactly.
Its total also supplies directed `total_length_f64_lower`/`upper` fields.

### Directed endpoints versus reference uncertainty

Every directed pair is the **minimal binary64 hull of the exact stored decimal
string**, computed with Python `Fraction` comparisons and `math.nextafter`. It
contains either one exactly representable value or two adjacent binary64 values.
The generator does not add the empirical reference allowance to these endpoints.
For example, analytic total lengths 0, 100, and 256 each have equal lower and
upper bounds, allowing valid zero-width production intervals to pass.

The verifier separately proves these representational hull properties by exact
rational comparison for every directed field. This proof concerns decimal
representation only: it does **not** prove the numerical integral is exact. The
existing empirical uncertainty describes the independent numerical reference,
including convergence and quadrature crosschecks. Even equal directed endpoints
do not turn finite-precision quadrature into a formal real-arithmetic proof.
Closed-form analytic cases provide their separately documented crosschecks.

## Using the references

- Check that production total-length and prefix enclosures contain **both**
  corresponding directed reference endpoints. This is stronger than widening a
  nearest-rounded expected value by one ULP and testing interval overlap. Keep
  the empirical numerical uncertainty separate and document it in the test; do
  not indiscriminately pad exact analytic central values. Reference containment
  still does not replace the production algorithm's mathematical enclosure proof.
- Check actual cuts using the documented operator-local budget:
  `min(1/1024, min(retained length, removed length)/8)`. A Euclidean endpoint
  comparison alone cannot establish arc residual on a backtracking curve; compare
  the selected segment, parameter/prefix arc, and traversal order as well.
- Preserve original cubic subcurves. These fixtures do not authorize flattening
  the path or exposing quadrature panels as rendered geometry.
- Full-span exact identity, equal-endpoint empty, source identity, seam adjacency,
  disconnected run order, scope, work caps, and explicit precision failures require
  separate exact/structural tests. The dataset is not a renderer or topology oracle.
- The tiny/few-ULP cases may validly trigger the published precision/work failure
  contract if their requested feature is unrepresentable. An inaccurate success,
  silent empty/full result, or best-effort truncation is not acceptable.

## Generation validation recorded for this artifact

The 80/120-digit generation completed for all 23 cases and 154 cuts. Maximum
observed discrepancies were approximately:

- Total/segment/rounded-parameter prefix length: `2.663e-75`
- Cut point coordinate: `6.610e-64`
- Inversion parameter: `3.005e-63`
- Inversion arc residual: `8.834e-64`

These are reference-generation checks, not a claim that production Rust tests,
renderer tests, or application checks have passed.

The separate 100-digit Gauss-Legendre verifier passed 226 checks across all 23
cases; its largest absolute discrepancy was `3.75192623886708e-66`. A second full
generation was byte-identical (`cmp` exit 0). The directed-bound enriched artifact
also passed 997 exact rational directed-hull checks and the same 226 quadrature
checks. Its second full generation was byte-identical. All earlier numeric
reference values were unchanged; only directed bounds and provenance were added.
The enriched JSON SHA-256 is
`1cdc374903274c7a3620851cf7ae3f00af7f777f5fe2e2d142e6d6cb156291f8`.
