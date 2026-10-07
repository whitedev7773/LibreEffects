use super::*;

fn ease(speed: f64, influence: f64) -> ScalarEase {
    ScalarEase { speed, influence }
}

fn bezier(
    out_speed: f64,
    out_influence: f64,
    in_speed: f64,
    in_influence: f64,
) -> (ScalarKeyTiming, ScalarKeyTiming) {
    let mut a = ScalarKeyTiming::new();
    a.out_interpolation = ScalarInterpolation::Bezier;
    a.out_ease = ease(out_speed, out_influence);
    let mut b = ScalarKeyTiming::new();
    b.in_interpolation = ScalarInterpolation::Bezier;
    b.in_ease = ease(in_speed, in_influence);
    (a, b)
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual:e} != {expected:e}; tolerance {tolerance:e}"
    );
}

// Independent Bernstein-basis oracle. Production uses de Casteljau values and
// a centered compensated time polynomial, so this does not mirror its solver.
fn bernstein([a, b, c, d]: [f64; 4], u: f64) -> f64 {
    let v = 1.0 - u;
    a * v.powi(3) + 3.0 * b * u * v.powi(2) + 3.0 * c * u.powi(2) * v + d * u.powi(3)
}

#[test]
fn explicit_native_defaults_and_exact_endpoint_source_values() {
    let a = ScalarKeyTiming::new();
    assert_eq!(a.in_interpolation, ScalarInterpolation::Linear);
    assert_eq!(a.out_interpolation, ScalarInterpolation::Linear);
    assert_eq!(a.in_ease, ease(0.0, 100.0 / 3.0));
    assert_eq!(a.out_ease, a.in_ease);
    assert!(!a.temporal_continuous && !a.temporal_auto_bezier);
    let b = a;
    for frame in [-1.0, 10.0] {
        assert_eq!(
            sample_segment(10, -0.0, &a, 50, 93.0, &b, frame, 0.04)
                .unwrap()
                .to_bits(),
            (-0.0_f64).to_bits()
        );
    }
    for frame in [50.0, 100.0] {
        assert_eq!(
            sample_segment(10, -0.0, &a, 50, 93.0, &b, frame, 0.04)
                .unwrap()
                .to_bits(),
            93.0_f64.to_bits()
        );
    }
}

#[test]
fn increasing_and_decreasing_linear_values_at_fractional_frames_and_two_rates() {
    let timing = ScalarKeyTiming::new();
    for (a, b) in [(10.0, 90.0), (90.0, 10.0), (-13.0, -13.0)] {
        for frame in [11.125, 22.5, 37.75, 49.125] {
            for spf in [1.0 / 24.0, 1.0 / 60.0] {
                let expected = a + (b - a) * ((frame - 10.0) / 40.0);
                close(
                    sample_segment(10, a, &timing, 50, b, &timing, frame, spf).unwrap(),
                    expected,
                    2e-13,
                );
            }
        }
    }
}

#[test]
fn zero_speed_bezier_uses_independent_smoothstep_oracle() {
    let (a, b) = bezier(0.0, 100.0 / 3.0, 0.0, 100.0 / 3.0);
    for (first, last) in [(0.0, 100.0), (100.0, 0.0), (-17.0, 83.0)] {
        for u in [0.01, 0.125, 0.375, 0.5, 0.825, 0.99] {
            let expected = first + (last - first) * (3.0 * u * u - 2.0 * u * u * u);
            close(
                sample_segment(0, first, &a, 100, last, &b, u * 100.0, 0.01).unwrap(),
                expected,
                3e-11,
            );
        }
    }
}

#[test]
fn signed_bezier_derivatives_are_physical_units_per_second_at_two_rates() {
    let (a, b) = bezier(30.0, 100.0 / 3.0, -18.0, 100.0 / 3.0);
    for spf in [1.0 / 24.0, 1.0 / 60.0] {
        let duration = 120.0 * spf;
        let controls = [
            10.0,
            10.0 + 30.0 * duration / 3.0,
            70.0 + 18.0 * duration / 3.0,
            70.0,
        ];
        for u in [0.125, 0.3125, 0.5, 0.875] {
            close(
                sample_segment(7, 10.0, &a, 127, 70.0, &b, 7.0 + 120.0 * u, spf).unwrap(),
                bernstein(controls, u),
                5e-11,
            );
        }
    }
}

#[test]
fn both_mixed_mode_orders_use_independent_secant_sides() {
    for outgoing_bezier in [true, false] {
        let (mut a, mut b) = bezier(-16.0, 20.0, 27.0, 65.0);
        let (x, y) = if outgoing_bezier {
            b.in_interpolation = ScalarInterpolation::Linear;
            b.in_ease = ease(-f64::MAX, 99.0); // Authored, but dormant.
            (
                [0.0, 0.2, 2.0 / 3.0, 1.0],
                [20.0, 20.0 - 16.0 * 2.0 * 0.2, 60.0, 80.0],
            )
        } else {
            a.out_interpolation = ScalarInterpolation::Linear;
            a.out_ease = ease(f64::MAX, 0.1);
            (
                [0.0, 1.0 / 3.0, 0.35, 1.0],
                [20.0, 40.0, 80.0 - 27.0 * 2.0 * 0.65, 80.0],
            )
        };
        for u in [0.07, 0.25, 0.43, 0.72, 0.97] {
            let frame = 100.0 * bernstein(x, u);
            close(
                sample_segment(0, 20.0, &a, 100, 80.0, &b, frame, 0.02).unwrap(),
                bernstein(y, u),
                4e-11,
            );
        }
    }
}

#[test]
fn increasing_decreasing_and_equal_endpoints_retain_raw_signed_overshoot() {
    let (a, b) = bezier(-20.0, 100.0, -20.0, 100.0);
    let low = sample_segment(0, 0.0, &a, 16, 1.0, &b, 7.0, 1.0 / 16.0).unwrap();
    close(low, bernstein([0.0, -20.0, 21.0, 1.0], 0.25), 1e-12);
    assert!(low < 0.0);
    let high = sample_segment(0, 0.0, &a, 16, 1.0, &b, 9.0, 1.0 / 16.0).unwrap();
    assert!(high > 1.0);
    let (a, b) = bezier(120.0, 100.0 / 3.0, 120.0, 100.0 / 3.0);
    let high = sample_segment(0, 100.0, &a, 100, 0.0, &b, 12.5, 0.01).unwrap();
    close(high, bernstein([100.0, 140.0, -40.0, 0.0], 0.125), 4e-11);
    assert!(high > 100.0);
    let (a, b) = bezier(120.0, 100.0 / 3.0, -120.0, 100.0 / 3.0);
    close(
        sample_segment(0, 50.0, &a, 100, 50.0, &b, 50.0, 0.01).unwrap(),
        80.0,
        4e-11,
    );
}

#[test]
fn both_maximum_influences_match_cube_root_inverse_at_stationary_time() {
    let (a, b) = bezier(-2.0, 100.0, 3.0, 100.0);
    let center_bits = 0.5_f64.to_bits();
    let times = [
        0.001,
        0.4375,
        f64::from_bits(center_bits - 1),
        0.5,
        f64::from_bits(center_bits + 1),
        0.500000000001,
        0.5625,
        0.999,
    ];
    for time in times {
        // x(u)=1/2+4(u-1/2)^3, independently inverted with the real cube root.
        let u = 0.5 + ((time - 0.5) / 4.0).cbrt();
        let expected = bernstein([7.0, 5.0, 8.0, 11.0], u);
        close(
            sample_segment(0, 7.0, &a, 1, 11.0, &b, time, 1.0).unwrap(),
            expected,
            8e-12,
        );
    }
}

#[test]
fn near_maximum_asymmetric_influences_match_independent_parameter_oracle() {
    for influences in [
        (100.0, 99.99999999999999),
        (99.99999999999999, 100.0),
        (99.9, 100.0),
        (0.1, 0.1),
    ] {
        let (a, b) = bezier(1.0, influences.0, -1.0, influences.1);
        let p = influences.0 / 100.0;
        let q = influences.1 / 100.0;
        for u in [0.1, 0.25, 0.75, 0.9] {
            let time = bernstein([0.0, p, 1.0 - q, 1.0], u);
            close(
                sample_segment(0, -3.0, &a, 1, 2.0, &b, time, 1.0).unwrap(),
                bernstein([-3.0, -3.0 + p, 2.0 + q, 2.0], u),
                4e-12,
            );
        }
    }
}

#[test]
fn tiny_negative_speed_is_never_inferred_to_be_zero() {
    let (a, b) = bezier(-1e-300, 100.0, 0.0, 100.0);
    let value = sample_segment(0, 0.0, &a, 16, 0.0, &b, 7.0, 1.0 / 16.0).unwrap();
    close(value / 1e-300, -27.0 / 64.0, 1e-13);
    assert!(value < 0.0);
    assert_eq!(a.out_ease.speed.to_bits(), (-1e-300_f64).to_bits());
}

#[test]
fn minimum_negative_subnormal_speed_can_make_an_exact_representable_excursion() {
    let speed = -f64::from_bits(1);
    assert_eq!(speed / 24.0, -0.0);
    let (a, b) = bezier(speed, 100.0, 0.0, 100.0);
    // Duration is exactly 1024 s at 24 FPS. At u=1/4, x=7/16,
    // and y=3*(-1024 minimum units)*(1/4)*(3/4)^2 = -432 units.
    let value = sample_segment(0, 0.0, &a, 24_576, 0.0, &b, 10_752.0, 1.0 / 24.0).unwrap();
    assert_eq!(value.to_bits(), (-f64::from_bits(432)).to_bits());
    assert_eq!(a.out_ease.speed.to_bits(), speed.to_bits());
}

#[test]
fn dormant_endpoint_and_linear_metadata_survives_mode_round_trips() {
    let mut a = ScalarKeyTiming::new();
    a.in_interpolation = ScalarInterpolation::Hold;
    a.in_ease = ease(-f64::MAX, 0.1);
    a.out_ease = ease(-f64::from_bits(1), 100.0);
    let mut b = ScalarKeyTiming::new();
    b.out_interpolation = ScalarInterpolation::Bezier;
    b.out_ease = ease(f64::MAX, 100.0);
    b.in_ease = ease(-17.0, 47.0);
    let before = (a, b);
    close(
        sample_segment(0, 20.0, &a, 48, 80.0, &b, 12.0, 1.0 / 24.0).unwrap(),
        35.0,
        1e-12,
    );
    a.out_interpolation = ScalarInterpolation::Bezier;
    b.in_interpolation = ScalarInterpolation::Bezier;
    let _ = sample_segment(0, 20.0, &a, 48, 80.0, &b, 12.0, 1.0 / 24.0).unwrap();
    a.out_interpolation = ScalarInterpolation::Linear;
    b.in_interpolation = ScalarInterpolation::Linear;
    assert_eq!((a, b), before);
    assert_eq!(
        a.out_ease.speed.to_bits(),
        before.0.out_ease.speed.to_bits()
    );
    assert_eq!(a.in_ease.speed.to_bits(), before.0.in_ease.speed.to_bits());
}

#[test]
fn outgoing_hold_bypasses_dormant_active_range_but_incoming_hold_alone_errors() {
    let (mut a, mut b) = bezier(f64::MAX, 100.0, -f64::MAX, 100.0);
    a.out_interpolation = ScalarInterpolation::Hold;
    b.in_interpolation = ScalarInterpolation::Hold;
    validate_segment(0, 4.0, &a, 100, 93.0, &b, f64::MAX).unwrap();
    assert_eq!(
        sample_segment(0, 4.0, &a, 100, 93.0, &b, 99.999, f64::MAX).unwrap(),
        4.0
    );
    assert_eq!(
        sample_segment(0, 4.0, &a, 100, 93.0, &b, 100.0, f64::MAX).unwrap(),
        93.0
    );
    a.out_interpolation = ScalarInterpolation::Linear;
    assert_eq!(
        sample_segment(0, 4.0, &a, 100, 93.0, &b, 50.0, 0.01),
        Err(ScalarError::IncomingHold)
    );
    assert_eq!(
        validate_segment(0, 4.0, &a, 100, 93.0, &b, 0.01),
        Err(ScalarError::IncomingHold)
    );
    // An exact key still returns its source value without its adjacent segment.
    assert_eq!(
        sample_segment(0, 4.0, &a, 100, 93.0, &b, 100.0, 0.01).unwrap(),
        93.0
    );
}

#[test]
fn serialization_preserves_signed_zero_subnormal_fields_and_rejects_unknown_fields() {
    let (mut a, _) = bezier(-f64::from_bits(1), 100.0 / 3.0, 0.0, 100.0);
    a.in_ease = ease(-0.0, 0.1);
    let encoded = serde_json::to_string(&a).unwrap();
    let decoded: ScalarKeyTiming = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, a);
    assert_eq!(decoded.in_ease.speed.to_bits(), a.in_ease.speed.to_bits());
    assert_eq!(decoded.out_ease.speed.to_bits(), a.out_ease.speed.to_bits());
    assert_eq!(
        decoded.out_ease.influence.to_bits(),
        a.out_ease.influence.to_bits()
    );
    let mut value = serde_json::to_value(a).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), true.into());
    assert!(serde_json::from_value::<ScalarKeyTiming>(value).is_err());
    let mut value = serde_json::to_value(a).unwrap();
    value["out_ease"]["unexpected"] = true.into();
    assert!(serde_json::from_value::<ScalarKeyTiming>(value).is_err());
    let mut value = serde_json::to_value(a).unwrap();
    value.as_object_mut().unwrap().remove("temporal_continuous");
    assert!(serde_json::from_value::<ScalarKeyTiming>(value).is_err());
    assert!(serde_json::from_str::<ScalarInterpolation>("\"Auto\"").is_err());
    assert!(serde_json::from_str::<SamplingOptions>(r#"{"absolute_tolerance":1e-9,"relative_tolerance":1e-12,"max_inversion_iterations":64,"unexpected":true}"#).is_err());
}

#[test]
fn invalid_metadata_and_unsupported_flags_fail_even_when_dormant() {
    for influence in [0.0, 0.099, 100.01, f64::INFINITY, f64::NAN] {
        assert!(ease(-1.0, influence).validate().is_err());
    }
    for speed in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        assert!(ease(speed, 10.0).validate().is_err());
    }
    for speed in [-f64::MAX, -f64::from_bits(1), -0.0, 0.0, f64::MAX] {
        ease(speed, 100.0).validate().unwrap();
    }
    for auto in [false, true] {
        let mut key = ScalarKeyTiming::new();
        key.temporal_auto_bezier = auto;
        key.temporal_continuous = !auto;
        assert!(matches!(
            key.validate(),
            Err(ScalarError::UnsupportedMode(_))
        ));
        assert!(matches!(
            sample_segment(0, 1.0, &key, 1, 2.0, &ScalarKeyTiming::new(), 0.0, 1.0),
            Err(ScalarError::UnsupportedMode(_))
        ));
    }
}

#[test]
fn invalid_frames_times_values_and_options_are_explicit_errors() {
    let key = ScalarKeyTiming::new();
    for spf in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            sample_segment(0, 0.0, &key, 100, 1.0, &key, 50.0, spf),
            Err(ScalarError::InvalidTime)
        );
    }
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            sample_segment(0, 0.0, &key, 100, 1.0, &key, frame, 0.01),
            Err(ScalarError::InvalidTime)
        );
    }
    for (first, last) in [(10, 10), (10, 9)] {
        assert_eq!(
            sample_segment(first, 0.0, &key, last, 1.0, &key, 10.0, 0.01),
            Err(ScalarError::InvalidFrameRange)
        );
    }
    assert_eq!(
        sample_segment(0, f64::NAN, &key, 100, 1.0, &key, 50.0, 0.01),
        Err(ScalarError::NonFinite)
    );
    for options in [
        SamplingOptions {
            absolute_tolerance: 0.0,
            ..SamplingOptions::default()
        },
        SamplingOptions {
            relative_tolerance: f64::INFINITY,
            ..SamplingOptions::default()
        },
        SamplingOptions {
            max_inversion_iterations: 0,
            ..SamplingOptions::default()
        },
        SamplingOptions {
            max_inversion_iterations: MAX_INVERSION_ITERATIONS + 1,
            ..SamplingOptions::default()
        },
    ] {
        assert_eq!(options.validate(), Err(ScalarError::InvalidSamplingOptions));
    }
}

#[test]
fn active_overflow_underflow_and_unattainable_precision_never_become_zero() {
    let (a, b) = bezier(f64::MAX, 100.0, 0.0, 100.0);
    assert_eq!(
        sample_segment(0, 0.0, &a, 100, 1.0, &b, 50.0, 1.0),
        Err(ScalarError::NumericRange)
    );
    let (a, b) = bezier(-f64::from_bits(1), 0.1, 0.0, 100.0);
    assert_eq!(
        sample_segment(0, 0.0, &a, 1, 0.0, &b, 0.5, 1.0),
        Err(ScalarError::PrecisionNotEstablished)
    );
    let (a, b) = bezier(1.0, 30.0, -1.0, 30.0);
    assert_eq!(
        sample_segment_with_options(
            0,
            0.0,
            &a,
            100,
            1.0,
            &b,
            37.0,
            0.01,
            SamplingOptions {
                relative_tolerance: 1e-30,
                ..SamplingOptions::default()
            }
        ),
        Err(ScalarError::PrecisionNotEstablished)
    );
    let (a, b) = bezier(1e10, 100.0, 1e10, 100.0);
    // Huge opposing controls do not license a large absolute uncertainty at a
    // midpoint whose mathematical value is only 50 units.
    assert_eq!(
        sample_segment(0, 0.0, &a, 100, 100.0, &b, 50.0, 0.01),
        Err(ScalarError::PrecisionNotEstablished)
    );
    let (a, b) = bezier(1e5, 100.0, 0.0, 100.0);
    assert_eq!(
        sample_segment(0, 1e30, &a, 100, 1e30, &b, 50.0, 0.01),
        Err(ScalarError::PrecisionNotEstablished)
    );
}

#[test]
fn bounded_inversion_returns_work_error_when_caller_ceiling_is_exhausted() {
    let (a, b) = bezier(-20.0, 20.0, 30.0, 70.0);
    assert_eq!(
        sample_segment_with_options(
            0,
            0.0,
            &a,
            100,
            100.0,
            &b,
            17.0,
            0.01,
            SamplingOptions {
                max_inversion_iterations: 1,
                ..SamplingOptions::default()
            }
        ),
        Err(ScalarError::WorkBudgetExceeded)
    );
    sample_segment_with_options(
        0,
        0.0,
        &a,
        100,
        100.0,
        &b,
        17.0,
        0.01,
        SamplingOptions {
            max_inversion_iterations: MAX_INVERSION_ITERATIONS,
            ..SamplingOptions::default()
        },
    )
    .unwrap();
}

#[test]
fn fractional_shifted_frames_keep_the_time_division_remainder_near_stationary_point() {
    let (a, b) = bezier(0.0, 100.0, 0.0, 100.0);
    let midpoint_bits = 70.0_f64.to_bits();
    for frame in [
        f64::from_bits(midpoint_bits - 1),
        70.0,
        f64::from_bits(midpoint_bits + 1),
    ] {
        // Shift before division to obtain the tiny signed distance from the
        // stationary time exactly. Normalizing frame first loses that detail.
        let u = 0.5 + ((frame - 70.0) / 480.0).cbrt();
        let expected = 100.0 * (3.0 * u * u - 2.0 * u * u * u);
        close(
            sample_segment(10, 0.0, &a, 130, 100.0, &b, frame, 1.0 / 24.0).unwrap(),
            expected,
            3e-11,
        );
    }
}

#[test]
fn nonzero_subnormal_products_do_not_hide_large_relative_rounding_errors() {
    let (a, b) = bezier(-f64::from_bits(1), 100.0, 0.0, 100.0);
    assert_eq!(
        sample_segment(0, 0.0, &a, 1, 0.0, &b, 0.5, 2.5),
        Err(ScalarError::PrecisionNotEstablished)
    );
    // A subnormal time handle can lose precision even when multiplication by
    // the finite speed brings the final value handle back into normal range.
    let (a, b) = bezier(1e300, 50.0, 0.0, 100.0);
    assert_eq!(
        sample_segment(0, 0.0, &a, 1, 0.0, &b, 0.5, f64::from_bits(3)),
        Err(ScalarError::PrecisionNotEstablished)
    );
}
