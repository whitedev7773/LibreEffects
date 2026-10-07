# Bounded SVG linear-gradient import

This extends the existing static SVG and strict inline-style contracts. Imported
paints become ordinary editable Gradient Fill/Stroke nodes; no new schema or
raster baking. Existing gradient schema 45 is used only when a gradient is present.
All existing source/history/chooser receipts remain in force.

## Accepted subset

- Root-level `defs` containing named `linearGradient` definitions and inert
  title/desc. Literal `url(#ID)` in Fill/Stroke attributes or strict inline style;
  forward references work. IDs are case-sensitive ASCII letters/underscore,
  followed by letters/digits/underscore/hyphen/dot, at most 128 bytes. All XML IDs
  remain unique; inline keyword folding never folds the fragment ID.
- Local gradient templates support literal `href="#ID"` and namespace-correct
  `xlink:href="#ID"`, including forward chains and reuse. The namespace URI is
  `http://www.w3.org/1999/xlink`; an equivalent prefix is accepted. When both
  attributes are supplied they must name the same ID. Conflicting destinations
  reject deliberately, rather than adopting SVG2's href-over-xlink precedence.
- Missing x1/y1/x2/y2, gradientUnits and gradientTransform take the nearest
  explicitly supplied template value; defaults apply only after the whole chain.
  Coordinates retain their units until the final gradientUnits is known. A local
  transform replaces the inherited transform, including explicit identity; it
  does not concatenate. Any local stops replace the entire inherited list.
  With no local stops, the nearest inherited list is used. Pad remains the only
  supported spread value. CSS properties such as color-interpolation are not
  copied through href; every accepted definition uses explicit/default sRGB.
- Each resolved gradient has 2–32 stops. Explicit offsets are ordered numbers 0–1 or
  percentages 0–100%, including coincident offsets in source order. Literal
  named/RGB/three/six-digit hex stop-color; stop-opacity 0–1. Default black/opaque.
  Paired editable color and opacity rows retain every stop and its ordering.
- Strict stop style admits only stop-color/stop-opacity; the existing paint style
  additionally admits local paint references. Inline wins over attributes; last
  duplicate wins; invalid shadowed values and missing/wrong-type refs reject.
- objectBoundingBox (default): unitless/percent coordinates in the unstroked
  geometry's tight bounding box. Zero-width/height boxes reject. userSpaceOnUse:
  unitless/px/percent coordinates; percentages use root viewBox dimensions, or
  viewport dimensions without viewBox. Defaults 0%,0%→100%,0%.
- gradientTransform uses the existing complete finite nonsingular affine grammar.
  Bbox transform precedes the gradient transform. The inverse-transpose field is
  converted to equivalent endpoints; simply transforming endpoints would be
  incorrect under nonuniform scale or shear. Normal element/group transforms
  remain on editable groups.
- Only pad spread and sRGB interpolation, explicit or default. At most 64 gradient
  definitions, 256 source stops, 256 resolved stops summed across all definitions,
  and 16 reference edges per chain (17 definitions). Memoized whole-table
  resolution bounds repeated work and counts reused stops to prevent amplification.
  Existing bytes/XML/elements/nesting/style/path/Contents budgets still apply.
  Every definition and supplied value is validated, even unused or shadowed,
  including references that local attributes/stops make visually unnecessary.
  Each definition must independently resolve to 2–32 valid stops and a valid
  field; an empty attribute-only template still rejects even if a child supplies
  its own stops. User-space effective fields validate globally; bbox-dependent
  effective fields validate for each geometry that actually uses them.

Radial gradients are covered by the [bounded radial extension](SVG_RADIAL_GRADIENT_PLAN.md).

## Explicit rejections and precision

No pattern/use/image/external URL/fallback paint, external href, missing or
wrong-type targets, reference cycles, conflicting href/xlink destinations,
whitespace/escaped/encoded/nonliteral fragment references, gradient CSS/general
stylesheet, stop functions,
repeat/reflect/linearRGB, animation, zero-length ramp or out-of-range/clamped
stop values. Existing DTD/entity/script/event/foreign-content restrictions stay.
No definition XML, reference or CSS reaches the geometry-only normalizer.

Source and flattened f32 endpoint lengths must exceed the pinned shader's
1/32768 degeneracy threshold. Transform inversion, relative field error and
normalized offset shift are checked at renderer precision (maximum 1e-6); a
materially distorted field rejects instead of becoming a solid/wrong ramp.
This is not a promise of raw-SVG byte-identical pixels. In addition to the
existing cubic geometry/AA distinction, mathematically equivalent transformed
versus flattened gradients can cross 8-bit channel rounding boundaries. The
independent shear fixture pins exactly 93 changed pixels with maximum channel
change 1 and identical fully opaque coverage, without a mask or general tolerance.
Editable rendering must still equal the independently authored flattened cubic
reference exactly across shared/preview/output/JSON/LEP routes.

Reference semantics:
https://www.w3.org/TR/SVG11/pservers.html#LinearGradients and
https://www.w3.org/TR/SVG2/pservers.html#PaintServerTemplates
