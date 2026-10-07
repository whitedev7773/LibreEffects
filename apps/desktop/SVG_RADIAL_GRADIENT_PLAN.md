# Bounded SVG radial-gradient import

This extends the [linear/reference contract](SVG_LINEAR_GRADIENT_PLAN.md),
without a schema change or new paint type. Imported radial Fill/Stroke uses
native Gradient center/end, Highlight Length/Angle and paired editable stops.

## Accepted fields and templates

- Named root-defs `radialGradient`, literal local paint references, and same-kind
  href / namespace-correct xlink:href. Cross-kind inheritance deliberately rejects.
  Existing whole-table validation, local-stop replacement, nearest explicit
  attribute and replacement-transform rules apply to both gradient kinds.
- cx/cy/r default to 50%; fx/fy default to the final effective cx/cy only if there
  is no explicit corresponding focus value in the reference chain. A parent's
  implicit focus is never frozen before a child overrides its center or units.
- Unitless/percent coordinates; px only with userSpaceOnUse. User-space x/y
  percentages use viewport/viewBox width/height; radius percentages use the
  normalized diagonal sqrt((width²+height²)/2). `fr` may be omitted or zero only.
- The combined object-bbox and gradient transform must preserve a circle in the
  native paint coordinate system: translation, rotation, uniform scale and
  reflection are accepted. Non-square boxes are accepted when the gradient
  transform compensates their anisotropy. Unequal axes/shear reject (relative
  similarity check 1e-10); no ellipse approximation or geometry rebaking.
- Ordinary element/group/viewBox affine transforms remain editable groups and
  are not subject to the paint-only circle restriction. Their scaling/shear may
  faithfully transform the complete painted geometry into an ellipse.
- Native Start is center; End is center+(radius,0). Highlight Length is positive
  focus displacement/radius ×100; Angle is the relative focus direction, zero for
  a centered focus. Displacement beyond the native 99.9% limit rejects, including
  SVG focus-on-circle/outside-circle behavior. No clamping is substituted.
- Existing 2–32 effective stops, pad/sRGB, strict stop declarations and stable
  stop identities remain. 64 definitions, 256 source and resolved stops, and
  16 href edges are shared across both gradient kinds. No definition/reference
  XML reaches the geometry normalizer; no network or external resources.

## Precision and limitations

Positive source and flattened radii must exceed the pinned radial shader's
1/4096 inclusive near-zero cutoff. The standards-resolved field’s f32 length conversion, bbox /
gradient composition and stored shader coefficients are compared with the
native emitted center/radius/focus shader. Precision uses the nested radial
circle family, conditioned near the focus boundary; material local field error
above 1e-6 rejects. Identical source/native shader fields stay accepted, including
existing f32 rounding. The shader's small focus-distance branch (1/32768) uses
focus as its effective center and is included in the check. This is a local
paint-field guard; extreme ancestor-transform rounding is not newly qualified.

Nonzero focal radius, gradient-only ellipses/shear, zero/negative radius,
nonrepresentable focus/precision and existing unsafe/unsupported constructs
reject explicitly. Original-versus-flattened floating-point shaders and existing
cubic geometry/AA distinctions do not promise raw-SVG byte-identical pixels.
The pinned SVG renderer does not apply ancestor-template gradientTransform
consistently; standards-correct inheritance is therefore checked against explicit
flattened fixtures rather than using that raw-renderer quirk as the oracle.
Independent literal flattened cubic fixtures must match native shared, preview,
output, JSON and LEP routes exactly. E06 remains Partial; general CSS, arbitrary
radial fields, full SVG round trips and wider native/platform coverage remain.

Reference semantics: [W3C radial gradients](https://www.w3.org/TR/SVG2/pservers.html#RadialGradients),
particularly the explicit fx/fy template/default distinction; local pinned
usvg length conversion and tiny-skia radial shader determine precision guards.
