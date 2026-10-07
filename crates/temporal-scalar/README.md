# Native signed temporal scalar math

This intermediate crate is independently tested mathematical infrastructure.
It does not by itself enable Opacity timing metadata, schema73, host operations,
rendering or native controls. Those integration changes are being developed
separately. No After Effects numerical parity is claimed.

Each timing key retains independent incoming/outgoing modes, signed speed in
units per second, influence in percent and explicit flags. Values and frame keys
remain owned by the caller. Defaults are native Linear sides, zero speed,
100/3-percent influence and false flags. Automatic/continuous generation rejects;
dormant endpoint ease and tiny finite values remain authored data.

Bezier controls use speed multiplied by segment duration and influence, with no
value-delta normalization or speed/FPS storage conversion. Equal endpoints can
overshoot or reverse. Linear sides use independent secants. Outgoing Hold retains
the first value until the next key; incoming Hold alone is explicitly unsupported.
Exact keys return exact authored values.

Compensated frame normalization and centered time inversion preserve the
stationary 100%/100% case. Work is bounded. Absolute and relative value-error
ceilings apply simultaneously; neither hides excessive uncertainty in the other.
Unrepresentable active products, unmet precision and exhausted work return errors.
Source metadata is never zeroed or canonicalized to repair a sample.

Fresh validation: 19 pure tests pass, including independent analytic cubic and
cube-root references, shifted/fractional time, two frame rates, signed/equal-value
overshoot, exact subnormal excursions, dormant serialization, Hold and error
budgets. Crate-only formatting and whitespace checks pass. Integration and
release/native qualification remain pending.

Run with the pinned workspace toolchain, one compiler and incremental off:
`cargo test -p libre-effects-temporal-scalar --locked --offline`.
JSON consumers need serde_json's float_roundtrip parser feature for bit-exact
metadata; native core already enables it.
