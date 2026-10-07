use super::*;

fn track(a: [f64; 3], b: [f64; 3]) -> SpatialPosition3 {
    SpatialPosition3 {
        value: [123.0, 456.0, 789.0],
        keys: BTreeMap::from([(0, SpatialKey3::new(a)), (100, SpatialKey3::new(b))]),
    }
}
fn close(actual: [f64; 3], expected: [f64; 3], error: f64) {
    for i in 0..3 {
        assert!(
            (actual[i] - expected[i]).abs() <= error,
            "{actual:?} != {expected:?}; tolerance {error}"
        );
    }
}
fn ease(speed: f64, influence: f64) -> SpatialEase {
    SpatialEase { speed, influence }
}

#[test]
fn explicit_native_defaults_and_exact_endpoints() {
    let key = SpatialKey3::new([1.0, 2.0, 3.0]);
    assert_eq!(key.in_interpolation, SpatialInterpolation::Linear);
    assert_eq!(key.out_interpolation, SpatialInterpolation::Linear);
    assert_eq!(key.in_tangent, [0.0; 3]);
    assert!(
        !key.spatial_continuous
            && !key.temporal_continuous
            && !key.spatial_auto_bezier
            && !key.temporal_auto_bezier
    );
    let t = track([2.0, -5.0, 7.0], [14.0, 11.0, 27.0]);
    assert_eq!(t.sample(-100.0, 0.04).unwrap(), t.keys[&0].value);
    assert_eq!(t.sample(0.0, 0.04).unwrap(), t.keys[&0].value);
    assert_eq!(t.sample(100.0, 0.04).unwrap(), t.keys[&100].value);
    assert_eq!(t.sample(1000.0, 0.04).unwrap(), t.keys[&100].value);
}

#[test]
fn linear_temporal_distance_inverts_nonuniform_collinear_cubic() {
    let mut t = track([2.0, -5.0, 7.0], [14.0, 11.0, 27.0]);
    // Zero handles yield a nonuniform spatial cubic. Linear temporal motion
    // must still cover one quarter of the line by one quarter of the time.
    close(t.sample(25.0, 0.04).unwrap(), [5.0, -1.0, 12.0], 5e-6);
    t.keys.get_mut(&0).unwrap().out_tangent = [1.5, 2.0, 2.5];
    t.keys.get_mut(&100).unwrap().in_tangent = [-6.0, -8.0, -10.0];
    close(t.sample(75.0, 0.04).unwrap(), [11.0, 7.0, 22.0], 5e-6);
}

#[test]
fn analytic_parabola_in_xyz_uses_arc_distance() {
    // P(u) = (u, u^2, u). Its independently integrated speed is
    // sqrt(2 + 4u^2), so S(u) = u*sqrt(2+4u^2)/2 + asinh(sqrt(2)*u)/2.
    let mut t = track([0.0; 3], [1.0, 1.0, 1.0]);
    t.keys.get_mut(&0).unwrap().out_tangent = [1.0 / 3.0, 0.0, 1.0 / 3.0];
    t.keys.get_mut(&100).unwrap().in_tangent = [-1.0 / 3.0, -2.0 / 3.0, -1.0 / 3.0];
    let integral =
        |u: f64| u * (2.0 + 4.0 * u * u).sqrt() / 2.0 + (2.0_f64.sqrt() * u).asinh() / 2.0;
    let total = integral(1.0);
    for u in [0.1, 0.25, 0.5, 0.8, 0.95] {
        close(
            t.sample(100.0 * integral(u) / total, 0.01).unwrap(),
            [u, u * u, u],
            8e-7,
        );
    }
    let (low, high) = sampling::test_arc_length(&t.keys[&0], &t.keys[&100]).unwrap();
    assert!(low <= total && total <= high, "{low} <= {total} <= {high}");
}

#[test]
fn equal_endpoints_with_one_incoming_handle_make_full_excursion() {
    let h = 9.0;
    let mut t = track([0.0; 3], [0.0; 3]);
    t.keys.get_mut(&100).unwrap().in_tangent = [0.0, 0.0, h];
    // Pz(u)=3h*u^2*(1-u), maximum 4h/9 at u=2/3. Total
    // variation/arc length is exactly 8h/9 despite coincident endpoints.
    let (low, high) = sampling::test_arc_length(&t.keys[&0], &t.keys[&100]).unwrap();
    assert!(
        low <= 8.0 * h / 9.0 && high >= 8.0 * h / 9.0,
        "{low} {high}"
    );
    close(t.sample(25.0, 0.01).unwrap(), [0.0, 0.0, 2.0], 3e-6);
    close(t.sample(50.0, 0.01).unwrap(), [0.0, 0.0, 4.0], 3e-6);
    close(t.sample(75.0, 0.01).unwrap(), [0.0, 0.0, 2.0], 3e-6);
}

#[test]
fn temporal_bezier_is_physical_distance_per_second_and_sides_independent() {
    let mut t = track([0.0; 3], [0.0, 0.0, 12.0]);
    let a = t.keys.get_mut(&0).unwrap();
    a.out_interpolation = SpatialInterpolation::Bezier;
    a.out_ease = ease(0.0, 100.0 / 3.0);
    let b = t.keys.get_mut(&100).unwrap();
    b.in_interpolation = SpatialInterpolation::Bezier;
    b.in_ease = ease(0.0, 100.0 / 3.0);
    // x(u)=u, distance=12*(3u^2-2u^3) for zero endpoint speeds.
    close(t.sample(25.0, 0.02).unwrap(), [0.0, 0.0, 1.875], 3e-6);
    t.keys.get_mut(&100).unwrap().in_interpolation = SpatialInterpolation::Linear;
    // Mixed sides: distance controls [0,0,8,12].
    close(t.sample(25.0, 0.02).unwrap(), [0.0, 0.0, 1.3125], 3e-6);
    let a = t.keys.get_mut(&0).unwrap();
    a.out_ease = ease(3.0, 100.0 / 3.0);
    // T=2 seconds makes first distance handle 2; changing fps changes it.
    close(t.sample(50.0, 0.02).unwrap(), [0.0, 0.0, 5.25], 3e-6);
    close(t.sample(50.0, 0.01).unwrap(), [0.0, 0.0, 4.875], 3e-6);
}

#[test]
fn influence_100_zero_time_derivative_has_stable_inverse() {
    let mut t = track([0.0; 3], [0.0, 0.0, 1.0]);
    let a = t.keys.get_mut(&0).unwrap();
    a.out_interpolation = SpatialInterpolation::Bezier;
    a.out_ease = ease(0.0, 100.0);
    let b = t.keys.get_mut(&100).unwrap();
    b.in_interpolation = SpatialInterpolation::Bezier;
    b.in_ease = ease(0.0, 100.0);
    close(t.sample(50.0, 0.01).unwrap(), [0.0, 0.0, 0.5], 3e-7);
    for fraction in [0.499999999999, 0.500000000001] {
        // x(u)=1/2+4(u-1/2)^3 has an independent real cube-root inverse.
        let actual_fraction = (fraction * 100.0) / 100.0;
        let u = 0.5 + ((actual_fraction - 0.5) / 4.0_f64).cbrt();
        let expected = 3.0 * u * u - 2.0 * u * u * u;
        close(
            t.sample(fraction * 100.0, 0.01).unwrap(),
            [0.0, 0.0, expected],
            3e-7,
        );
    }
}

#[test]
fn valid_crossed_distance_controls_are_supported() {
    let mut t = track([0.0; 3], [1.0, 0.0, 0.0]);
    let a = t.keys.get_mut(&0).unwrap();
    a.out_interpolation = SpatialInterpolation::Bezier;
    a.out_ease = ease(0.9, 100.0);
    let b = t.keys.get_mut(&100).unwrap();
    b.in_interpolation = SpatialInterpolation::Bezier;
    b.in_ease = ease(0.9, 100.0);
    // [0,.9,.1,1] has a nonnegative quadratic derivative.
    t.validate_sampling(0.01).unwrap();
    close(t.sample(50.0, 0.01).unwrap(), [0.5, 0.0, 0.0], 3e-7);
    // Both time and distance derivatives vanish at the midpoint. The exact
    // unit axis length certifies the boundary case without flattening it.
    t.keys.get_mut(&0).unwrap().out_ease.speed = 1.0;
    t.keys.get_mut(&100).unwrap().in_ease.speed = 1.0;
    t.validate_sampling(0.01).unwrap();
    close(t.sample(43.75, 0.01).unwrap(), [0.4375, 0.0, 0.0], 3e-7);
}

#[test]
fn active_reversal_and_constant_speed_are_errors_without_repair() {
    let mut t = track([0.0; 3], [1.0, 0.0, 0.0]);
    for key in t.keys.values_mut() {
        key.in_interpolation = SpatialInterpolation::Bezier;
        key.out_interpolation = SpatialInterpolation::Bezier;
        key.in_ease = ease(10.0, 100.0);
        key.out_ease = ease(10.0, 100.0);
    }
    let before = t.clone();
    t.validate().unwrap();
    assert_eq!(
        t.validate_sampling(0.01),
        Err(SpatialError::TemporalDistanceReversal)
    );
    assert_eq!(
        t.sample(50.0, 0.01),
        Err(SpatialError::TemporalDistanceReversal)
    );
    assert_eq!(before, t);
    t.keys.get_mut(&100).unwrap().value = [0.0; 3];
    assert_eq!(
        t.sample(50.0, 0.01),
        Err(SpatialError::NonzeroSpeedOnConstantPath)
    );
    t.keys.get_mut(&0).unwrap().out_ease.speed = 0.0;
    t.keys.get_mut(&100).unwrap().in_ease.speed = 0.0;
    assert_eq!(t.sample(50.0, 0.01).unwrap(), [0.0; 3]);
}

#[test]
fn dormant_one_key_endpoint_and_hold_metadata_survives() {
    let mut key = SpatialKey3::new([1.0, 2.0, 3.0]);
    key.in_interpolation = SpatialInterpolation::Hold;
    key.out_interpolation = SpatialInterpolation::Bezier;
    key.in_ease = ease(f64::MAX, 100.0);
    key.out_ease = ease(1e-300, 0.1);
    key.in_tangent = [1e-300, -1e-200, 8.0];
    key.out_tangent = [-5.0, 1e-280, 0.0];
    let mut t = SpatialPosition3 {
        value: [0.0; 3],
        keys: BTreeMap::from([(0, key)]),
    };
    let before = t.clone();
    t.validate_sampling(1.0).unwrap();
    assert_eq!(t.sample(0.25, 1.0).unwrap(), [1.0, 2.0, 3.0]);
    assert_eq!(before, t);
    let mut last = SpatialKey3::new([9.0, 8.0, 7.0]);
    last.in_interpolation = SpatialInterpolation::Hold;
    last.in_ease = ease(f64::MAX, 100.0);
    last.out_ease = ease(f64::MAX, 100.0);
    last.out_interpolation = SpatialInterpolation::Bezier;
    t.keys.insert(100, last);
    t.keys.get_mut(&0).unwrap().out_interpolation = SpatialInterpolation::Hold;
    let before = t.clone();
    t.validate_sampling(1.0).unwrap();
    assert_eq!(t.sample(99.999, 1.0).unwrap(), [1.0, 2.0, 3.0]);
    assert_eq!(t.sample(100.0, 1.0).unwrap(), [9.0, 8.0, 7.0]);
    assert_eq!(before, t);
    t.keys.get_mut(&0).unwrap().out_interpolation = SpatialInterpolation::Linear;
    assert_eq!(t.sample(50.0, 1.0), Err(SpatialError::IncomingHold));
}

#[test]
fn tiny_finite_metadata_and_tiny_paths_are_preserved() {
    let mut t = track([0.0; 3], [1e-200, 0.0, 0.0]);
    t.keys.get_mut(&0).unwrap().out_tangent = [1e-201, 0.0, 0.0];
    t.keys.get_mut(&100).unwrap().in_tangent = [-1e-202, 0.0, 0.0];
    t.keys.get_mut(&0).unwrap().out_ease = ease(1e-300, 0.1);
    t.keys.get_mut(&0).unwrap().out_interpolation = SpatialInterpolation::Bezier;
    let before = t.clone();
    let value = t.sample(50.0, 0.01).unwrap();
    assert!(value[0] > 0.0 && value[0] < 1e-200);
    let encoded = serde_json::to_string(&t).unwrap();
    assert_eq!(
        serde_json::from_str::<SpatialPosition3>(&encoded).unwrap(),
        before
    );
    assert_eq!(t.keys[&0].out_ease.speed.to_bits(), 1e-300_f64.to_bits());
    assert_eq!(
        t.keys[&100].in_tangent[0].to_bits(),
        (-1e-202_f64).to_bits()
    );
}

#[test]
fn manual_continuity_checks_directions_and_accepts_different_magnitudes() {
    let mut key = SpatialKey3::new([0.0; 3]);
    key.spatial_continuous = true;
    key.in_tangent = [1e-200, 2e-200, -3e-200];
    key.out_tangent = [-2e-100, -4e-100, 6e-100];
    let mut t = track([-1.0; 3], [1.0; 3]);
    t.keys.insert(50, key);
    t.validate_sampling_metadata().unwrap();
    t.keys.get_mut(&50).unwrap().out_tangent = [0.0; 3];
    t.validate_sampling_metadata().unwrap();
    for tangent in [[1.0, 2.0, -3.0], [-2.0, -4.0, 7.0]] {
        t.keys.get_mut(&50).unwrap().out_tangent = tangent;
        t.validate().unwrap();
        assert!(matches!(
            t.validate_sampling_metadata(),
            Err(SpatialError::InvalidMetadata(_))
        ));
        assert!(matches!(
            t.validate_sampling(0.01),
            Err(SpatialError::InvalidMetadata(_))
        ));
        assert!(matches!(
            t.sample(25.0, 0.01),
            Err(SpatialError::InvalidMetadata(_))
        ));
    }
    t.keys.get_mut(&50).unwrap().spatial_continuous = false;
    t.validate_sampling_metadata().unwrap();
}

#[test]
fn continuous_dormant_endpoint_and_hold_handles_are_preserved() {
    let mut t = track([0.0; 3], [10.0; 3]);
    for key in t.keys.values_mut() {
        key.spatial_continuous = true;
        key.in_tangent = [-2.0, 1.0, 0.0];
        key.out_tangent = [0.0, 3.0, 1.0];
    }
    let before = t.clone();
    t.validate_sampling(0.01).unwrap();
    t.sample(25.0, 0.01).unwrap();
    assert_eq!(t, before);
    let encoded = serde_json::to_string(&t).unwrap();
    assert_eq!(
        serde_json::from_str::<SpatialPosition3>(&encoded).unwrap(),
        before
    );

    let mut middle = SpatialKey3::new([5.0; 3]);
    middle.spatial_continuous = true;
    middle.in_tangent = [-1.0, 0.0, 0.0];
    middle.out_tangent = [0.0, 2.0, 0.0];
    t.keys.insert(50, middle);
    t.keys.get_mut(&0).unwrap().out_interpolation = SpatialInterpolation::Hold;
    t.validate_sampling(0.01).unwrap();
    t.keys.get_mut(&0).unwrap().out_interpolation = SpatialInterpolation::Linear;
    t.keys.get_mut(&50).unwrap().out_interpolation = SpatialInterpolation::Hold;
    t.validate_sampling(0.01).unwrap();
    t.keys.get_mut(&50).unwrap().out_interpolation = SpatialInterpolation::Linear;
    assert!(matches!(
        t.validate_sampling_metadata(),
        Err(SpatialError::InvalidMetadata(_))
    ));
}

#[test]
fn structural_validation_allows_tangent_and_flag_restoration_order() {
    let mut t = track([0.0; 3], [10.0; 3]);
    let mut key = SpatialKey3::new([5.0; 3]);
    key.spatial_continuous = true;
    key.in_tangent = [-1.0, 0.0, 0.0];
    key.out_tangent = [2.0, 0.0, 0.0];
    t.keys.insert(50, key);
    t.validate_sampling_metadata().unwrap();

    // Restore a different continuous pair one side at a time, preserving the
    // authored true flag until the second tangent completes the operation.
    t.keys.get_mut(&50).unwrap().in_tangent = [0.0, -2.0, 0.0];
    t.validate().unwrap();
    assert!(t.validate_sampling_metadata().is_err());
    t.keys.get_mut(&50).unwrap().out_tangent = [0.0, 3.0, 0.0];
    t.validate().unwrap();
    t.validate_sampling(0.01).unwrap();

    // Restoring tangents before the flag is also supported. An invalid true
    // final flag is rejected by sampling rather than changing either tangent.
    t.keys.get_mut(&50).unwrap().spatial_continuous = false;
    t.keys.get_mut(&50).unwrap().out_tangent = [1.0, 0.0, 0.0];
    t.validate().unwrap();
    t.keys.get_mut(&50).unwrap().spatial_continuous = true;
    t.validate().unwrap();
    assert!(t.sample(50.0, 0.01).is_err());
    t.keys.get_mut(&50).unwrap().in_tangent = [-2.0, 0.0, 0.0];
    t.validate_sampling(0.01).unwrap();
}

#[test]
fn unsupported_flags_are_explicit_even_on_dormant_keys() {
    for mode in 0..3 {
        let mut key = SpatialKey3::new([0.0; 3]);
        match mode {
            0 => key.temporal_continuous = true,
            1 => key.temporal_auto_bezier = true,
            _ => key.spatial_auto_bezier = true,
        }
        assert!(matches!(
            key.validate(),
            Err(SpatialError::UnsupportedMode(_))
        ));
    }
}

#[test]
fn finite_metadata_and_influence_boundaries_are_checked() {
    for speed in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(ease(speed, 30.0).validate().is_err());
    }
    for influence in [0.0, 0.09999, 100.001, f64::NAN] {
        assert!(ease(0.0, influence).validate().is_err());
    }
    for influence in [0.1, 100.0] {
        ease(f64::MIN_POSITIVE, influence).validate().unwrap();
    }
    let mut key = SpatialKey3::new([0.0; 3]);
    key.in_tangent[2] = f64::INFINITY;
    assert_eq!(key.validate(), Err(SpatialError::NonFinite));
    let mut t = SpatialPosition3::new([0.0; 3]);
    t.value[1] = f64::NAN;
    assert_eq!(t.validate(), Err(SpatialError::NonFinite));
    let t = track([0.0; 3], [1.0; 3]);
    for time in [0.0, -0.1, f64::INFINITY, f64::NAN] {
        assert_eq!(t.sample(0.0, time), Err(SpatialError::InvalidTime));
    }
    assert_eq!(t.sample(f64::NAN, 1.0), Err(SpatialError::InvalidTime));
}

#[test]
fn strict_serde_rejects_unknown_missing_wrong_arity_and_duplicate_numeric_keys() {
    let t = track([0.0; 3], [1.0; 3]);
    let text = serde_json::to_string(&t).unwrap();
    assert_eq!(serde_json::from_str::<SpatialPosition3>(&text).unwrap(), t);
    let key = serde_json::to_string(&t.keys[&0]).unwrap();
    for keys in [
        format!(r#""1":{key},"1":{key}"#),
        format!(r#""1":{key},"01":{key}"#),
    ] {
        let duplicate = format!(r#"{{"value":[0,0,0],"keys":{{{keys}}}}}"#);
        assert!(
            serde_json::from_str::<SpatialPosition3>(&duplicate).is_err(),
            "{duplicate}"
        );
    }
    for changed in [
        text.replacen("\"value\":[0.0,0.0,0.0]", "\"value\":[0.0,0.0]", 1),
        text.replacen("{", "{\"unexpected\":true,", 1),
        text.replacen("\"speed\":0.0", "\"speed\":0.0,\"unknown\":7", 1),
        text.replacen("\"temporal_continuous\":false,", "", 1),
        text.replacen(
            "\"spatial_continuous\":false",
            "\"spatial_continuous\":false,\"extra\":1",
            1,
        ),
    ] {
        assert!(
            serde_json::from_str::<SpatialPosition3>(&changed).is_err(),
            "{changed}"
        );
    }
    // Exercise the streaming reader path used by enclosing native IPC structs.
    let roundtrip: SpatialPosition3 = serde_json::from_reader(text.as_bytes()).unwrap();
    assert_eq!(roundtrip, t);
}

#[test]
fn translation_and_positive_exact_retiming_preserve_motion() {
    let mut t = track([1.0, 2.0, 3.0], [21.0, -8.0, 43.0]);
    t.keys.get_mut(&0).unwrap().out_interpolation = SpatialInterpolation::Bezier;
    t.keys.get_mut(&0).unwrap().out_ease = ease(5.0, 30.0);
    t.keys.get_mut(&0).unwrap().in_ease = ease(1e-200, 100.0);
    let offset = [50.0, -10.0, 100.0];
    let shifted = t.translated(offset).unwrap();
    let value = t.sample(30.0, 0.01).unwrap();
    close(
        shifted.sample(30.0, 0.01).unwrap(),
        std::array::from_fn(|i| value[i] + offset[i]),
        1e-5,
    );
    assert_eq!(shifted.keys[&0].out_tangent, t.keys[&0].out_tangent);
    let slowed = t.retimed(0.0, 2.0, 10.0).unwrap();
    assert_eq!(slowed.keys[&10].out_ease.speed, 2.5);
    assert_eq!(slowed.keys[&10].in_ease.speed, 0.5e-200);
    close(slowed.sample(70.0, 0.01).unwrap(), value, 1e-5);
    let translated_time = t.retimed(0.0, 1.0, 10.0).unwrap();
    assert_eq!(translated_time.keys[&10].out_ease, t.keys[&0].out_ease);
    assert!(t.retimed(0.0, -1.0, 0.0).is_err());
    assert!(t.retimed(0.0, 1.0, -1.0).is_err());
    assert!(t.retimed(0.0, 1.0, 0.5).is_err());
    assert!(t.retimed(0.0, 1.0, f64::from(u32::MAX)).is_err());
    assert_eq!(
        t.retimed(1e30, 1.0, 0.0),
        Err(SpatialError::RetimeCollision)
    );
    assert!(t.translated([f64::INFINITY, 0.0, 0.0]).is_err());
}

#[test]
fn work_precision_numeric_and_allocation_ceilings_return_diagnostics() {
    let mut t = track([0.0; 3], [1.0, 1.0, 1.0]);
    t.keys.get_mut(&0).unwrap().out_tangent = [0.0, 2.0, 0.0];
    t.keys.get_mut(&100).unwrap().in_tangent = [1.0, -1.0, 2.0];
    let limited = SamplingOptions {
        max_subdivisions: 1,
        ..SamplingOptions::default()
    };
    assert_eq!(
        t.sample_with_options(50.0, 0.01, limited),
        Err(SpatialError::WorkBudgetExceeded)
    );
    let limited = SamplingOptions {
        max_depth: 1,
        ..SamplingOptions::default()
    };
    assert_eq!(
        t.sample_with_options(50.0, 0.01, limited),
        Err(SpatialError::WorkBudgetExceeded)
    );
    let limited = SamplingOptions {
        max_inversion_iterations: 1,
        ..SamplingOptions::default()
    };
    assert_eq!(
        track([0.0; 3], [1.0; 3]).sample_with_options(25.0, 1.0, limited),
        Err(SpatialError::WorkBudgetExceeded)
    );
    let invalid = SamplingOptions {
        max_subdivisions: MAX_SUBDIVISIONS + 1,
        ..SamplingOptions::default()
    };
    assert_eq!(
        t.sample_with_options(50.0, 0.01, invalid),
        Err(SpatialError::InvalidSamplingOptions)
    );
    let precision = SamplingOptions {
        relative_tolerance: 1e-30,
        max_subdivisions: 4,
        ..SamplingOptions::default()
    };
    assert!(t.sample_with_options(50.0, 0.01, precision).is_err());
    let overflow = track([-f64::MAX, 0.0, 0.0], [f64::MAX, 0.0, 0.0]);
    assert_eq!(overflow.sample(50.0, 1.0), Err(SpatialError::NumericRange));
    assert_eq!(t.sample(50.0, f64::MAX), Err(SpatialError::NumericRange));
    let mut unrepresentable = track([1e16, 0.0, 0.0], [1e16, 0.0, 0.0]);
    unrepresentable.keys.get_mut(&100).unwrap().in_tangent = [1.0, 0.0, 0.0];
    assert_eq!(
        unrepresentable.sample(25.0, 0.01),
        Err(SpatialError::PrecisionNotEstablished)
    );
    let mut too_many = SpatialPosition3::new([0.0; 3]);
    for frame in 0..=MAX_KEYS as u32 {
        too_many.keys.insert(frame, SpatialKey3::new([0.0; 3]));
    }
    assert_eq!(too_many.validate(), Err(SpatialError::KeyBudgetExceeded));
}

#[test]
fn four_key_xyz_curve_validates_all_segments_under_default_bounds() {
    let mut t = SpatialPosition3::new([0.0; 3]);
    for (frame, value) in [
        (0, [180.0, 100.0, 0.0]),
        (30, [160.0, 100.0, 60.0]),
        (120, [160.0, 100.0, 60.0]),
        (150, [180.0, 100.0, 0.0]),
    ] {
        let mut key = SpatialKey3::new(value);
        key.in_interpolation = SpatialInterpolation::Bezier;
        key.out_interpolation = SpatialInterpolation::Bezier;
        key.in_ease = ease(if frame == 30 { 1e-9 } else { 0.0 }, 65.0);
        key.out_ease = ease(0.0, 35.0);
        key.spatial_continuous = true;
        match frame {
            0 => key.in_tangent = [-3.0, 4.0, -5.0],
            30 => key.in_tangent = [0.5, 0.0, 0.0],
            120 => {
                key.in_tangent = [-0.5, 0.0, 0.0];
                key.out_tangent = [0.5, 0.0, 0.0];
            }
            150 => key.out_ease = ease(1e-199, 17.25),
            _ => unreachable!(),
        }
        t.keys.insert(frame, key);
    }
    let before = t.clone();
    t.validate_sampling(1.0 / 30.0).unwrap();
    for (a, b) in [(0, 30), (30, 120), (120, 150)] {
        let (splits, depth) = sampling::test_arc_work(&t.keys[&a], &t.keys[&b]).unwrap();
        eprintln!("four-key segment {a}->{b}: {splits} splits, depth {depth}");
        assert!(
            splits < 4096,
            "ordinary segment {a}->{b} consumed {splits} splits"
        );
        assert!(
            depth < 30,
            "ordinary segment {a}->{b} reached depth {depth}"
        );
    }
    for frame in [1.0, 15.0, 29.0, 31.0, 75.0, 119.0, 121.0, 135.0, 149.0] {
        let value = t.sample(frame, 1.0 / 30.0).unwrap();
        assert_eq!(value[1], 100.0);
        assert!(value.iter().all(|v| v.is_finite()));
    }
    assert_eq!(t, before);
}

#[test]
fn global_arc_budget_handles_varied_generic_xyz_curves() {
    for (outgoing, incoming) in [
        ([0.0, 0.0, 0.0], [0.5, 0.0, 0.0]),
        ([0.5, 0.0, 0.0], [0.0, 0.0, 0.0]),
        ([30.0, -25.0, 1.0], [-5.0, 40.0, -10.0]),
        ([-10.0, 2.0, 30.0], [15.0, -20.0, 5.0]),
        ([0.0, 100.0, 0.0], [0.0, -100.0, 0.0]),
        ([-40.0, 20.0, -60.0], [40.0, -20.0, 60.0]),
    ] {
        let mut t = track([30.0, -10.0, 15.0], [10.0, 0.0, 75.0]);
        t.keys.get_mut(&0).unwrap().out_tangent = outgoing;
        t.keys.get_mut(&100).unwrap().in_tangent = incoming;
        t.validate_sampling(0.01).unwrap();
        for frame in [10.0, 50.0, 90.0] {
            let first = t.sample(frame, 0.01).unwrap();
            assert_eq!(first, t.sample(frame, 0.01).unwrap());
            assert!(first.iter().all(|v| v.is_finite()));
        }
    }
}
