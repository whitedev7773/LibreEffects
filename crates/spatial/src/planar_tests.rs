use super::*;

fn track(a: [f64; 2], b: [f64; 2]) -> SpatialPosition2 {
    SpatialPosition2 {
        value: None,
        keys: BTreeMap::from([(0, SpatialKey2::new(a)), (100, SpatialKey2::new(b))]),
    }
}

fn close(actual: [f64; 2], expected: [f64; 2], tolerance: f64) {
    for axis in 0..2 {
        assert!(
            (actual[axis] - expected[axis]).abs() <= tolerance,
            "{actual:?} != {expected:?}; tolerance {tolerance}"
        );
    }
}

fn same_bits(actual: [f64; 2], expected: [f64; 2]) {
    assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
}

#[test]
fn planar_absent_base_roundtrip_does_not_invent_authored_values() {
    let mut t = track([-0.0, 1e-300], [1e-200, -1e-250]);
    let first = t.keys.get_mut(&0).unwrap();
    first.in_tangent = [-0.0, -1e-300];
    first.in_ease.speed = -0.0;
    let encoded = serde_json::to_string(&t).unwrap();
    let shape: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert!(shape.get("value").is_none());
    let decoded: SpatialPosition2 = serde_json::from_reader(encoded.as_bytes()).unwrap();
    assert_eq!(decoded, t);
    assert!(decoded.value.is_none());
    same_bits(decoded.keys[&0].value, [-0.0, 1e-300]);
    same_bits(decoded.keys[&0].in_tangent, [-0.0, -1e-300]);
    assert_eq!(
        decoded.keys[&0].in_ease.speed.to_bits(),
        (-0.0_f64).to_bits()
    );
    for frame in [-1.0, 0.0] {
        same_bits(decoded.sample(frame, 1.0 / 24.0).unwrap(), [-0.0, 1e-300]);
    }
    same_bits(
        decoded.sample(200.0, 1.0 / 24.0).unwrap(),
        [1e-200, -1e-250],
    );
    assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);

    let mut with_base = t;
    with_base.value = Some([-0.0, f64::from_bits(1)]);
    let encoded = serde_json::to_string(&with_base).unwrap();
    let decoded: SpatialPosition2 = serde_json::from_str(&encoded).unwrap();
    same_bits(decoded.value.unwrap(), with_base.value.unwrap());
}

#[test]
fn planar_static_value_is_required_and_samples_exactly() {
    let empty: SpatialPosition2 = serde_json::from_str(r#"{"keys":{}}"#).unwrap();
    assert!(matches!(
        empty.validate(),
        Err(SpatialError::InvalidMetadata(_))
    ));
    assert!(empty.validate_sampling_metadata().is_err());
    assert!(empty.validate_sampling(0.01).is_err());
    assert!(empty.sample(0.0, 0.01).is_err());
    assert!(empty.retimed(0.0, 1.0, 0.0).is_err());

    let t = SpatialPosition2::new([-0.0, f64::from_bits(1)]);
    t.validate_sampling(1.0 / 60.0).unwrap();
    same_bits(t.sample(-25.5, 1.0 / 60.0).unwrap(), t.value.unwrap());
    assert_eq!(
        serde_json::from_str::<SpatialPosition2>(&serde_json::to_string(&t).unwrap()).unwrap(),
        t
    );
}

#[test]
fn planar_equal_endpoints_with_nonzero_tangent_make_real_excursion() {
    let mut t = track([0.0, 0.0], [0.0, 0.0]);
    t.keys.get_mut(&100).unwrap().in_tangent = [9.0, 0.0];
    // Px(u)=27*u^2*(1-u). The independent total variation is 8,
    // reaching x=4 halfway through arc distance despite identical endpoints.
    t.validate_sampling(0.01).unwrap();
    for (frame, x) in [
        (0.0, 0.0),
        (25.0, 2.0),
        (50.0, 4.0),
        (75.0, 2.0),
        (100.0, 0.0),
    ] {
        close(t.sample(frame, 0.01).unwrap(), [x, 0.0], 3e-6);
    }
    assert!(t.value.is_none());
}

#[test]
fn planar_temporal_ease_uses_explicit_frame_duration() {
    let mut t = track([0.0, 0.0], [12.0, 0.0]);
    let a = t.keys.get_mut(&0).unwrap();
    a.out_interpolation = SpatialInterpolation::Bezier;
    a.out_ease = SpatialEase {
        speed: 3.0,
        influence: 100.0 / 3.0,
    };
    // Mixed Bezier/linear sides have distance controls [0, 2, 8, 12]
    // at 50 fps and [0, 1, 8, 12] at 100 fps. x(u)=u in time.
    close(t.sample(50.0, 0.02).unwrap(), [5.25, 0.0], 3e-6);
    close(t.sample(50.0, 0.01).unwrap(), [4.875, 0.0], 3e-6);
    for seconds_per_frame in [0.0, -0.1, f64::NAN, f64::INFINITY] {
        assert_eq!(
            t.sample(0.0, seconds_per_frame),
            Err(SpatialError::InvalidTime)
        );
        assert_eq!(
            t.validate_sampling(seconds_per_frame),
            Err(SpatialError::InvalidTime)
        );
    }
    assert_eq!(t.sample(f64::NAN, 0.01), Err(SpatialError::InvalidTime));
}

#[test]
fn planar_retime_preserves_optional_base_and_signed_tiny_metadata() {
    let mut t = track([-0.0, 0.0], [12.0, 0.0]);
    let a = t.keys.get_mut(&0).unwrap();
    a.in_ease = SpatialEase {
        speed: -0.0,
        influence: 0.1,
    };
    a.out_interpolation = SpatialInterpolation::Bezier;
    a.out_ease = SpatialEase {
        speed: 3.0,
        influence: 100.0 / 3.0,
    };
    a.in_tangent = [-0.0, -1e-300];
    a.spatial_continuous = true;
    let b = t.keys.get_mut(&100).unwrap();
    b.out_ease = SpatialEase {
        speed: 1e-300,
        influence: 100.0,
    };
    b.out_tangent = [1e-250, -0.0];
    let encoded = serde_json::to_string(&t).unwrap();
    let retimed = t.retimed(0.0, 2.0, 10.0).unwrap();
    assert!(retimed.value.is_none());
    assert_eq!(retimed.keys.keys().copied().collect::<Vec<_>>(), [10, 210]);
    same_bits(retimed.keys[&10].value, t.keys[&0].value);
    same_bits(retimed.keys[&10].in_tangent, t.keys[&0].in_tangent);
    same_bits(retimed.keys[&210].out_tangent, t.keys[&100].out_tangent);
    assert!(retimed.keys[&10].spatial_continuous);
    assert_eq!(
        retimed.keys[&10].in_ease.speed.to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(retimed.keys[&10].out_ease.speed, 1.5);
    assert_eq!(
        retimed.keys[&210].out_ease.speed.to_bits(),
        (1e-300_f64 / 2.0).to_bits()
    );
    assert_eq!(retimed.keys[&210].out_ease.influence, 100.0);
    close(
        retimed.sample(110.0, 0.01).unwrap(),
        t.sample(50.0, 0.01).unwrap(),
        3e-6,
    );
    assert_eq!(serde_json::to_string(&t).unwrap(), encoded);

    t.value = Some([-0.0, 1e-300]);
    same_bits(
        t.retimed(0.0, 2.0, 10.0).unwrap().value.unwrap(),
        t.value.unwrap(),
    );
    for (origin, scale, offset) in [
        (0.0, -1.0, 0.0),
        (0.0, 1.0, 0.5),
        (0.0, 1.0, -1.0),
        (0.0, 1.0, f64::from(u32::MAX)),
    ] {
        assert_eq!(
            t.retimed(origin, scale, offset),
            Err(SpatialError::InvalidRetime)
        );
    }
    assert_eq!(
        t.retimed(1e30, 1.0, 0.0),
        Err(SpatialError::RetimeCollision)
    );
    // A dormant nonzero speed may not be silently rounded down to zero.
    t.keys.get_mut(&100).unwrap().out_ease.speed = f64::from_bits(1);
    assert_eq!(t.retimed(0.0, 2.0, 0.0), Err(SpatialError::InvalidRetime));
}

#[test]
fn planar_strict_wire_rejects_wrong_arity_fields_and_numeric_duplicates() {
    let t = track([0.0, 0.0], [1.0, 2.0]);
    let text = serde_json::to_string(&t).unwrap();
    let key = serde_json::to_string(&t.keys[&0]).unwrap();
    for keys in [
        format!(r#""1":{key},"1":{key}"#),
        format!(r#""1":{key},"01":{key}"#),
    ] {
        assert!(
            serde_json::from_str::<SpatialPosition2>(&format!(r#"{{"keys":{{{keys}}}}}"#)).is_err()
        );
    }
    for changed in [
        text.replacen("{", "{\"extra\":1,", 1),
        text.replacen("\"value\":[0.0,0.0]", "\"value\":[0.0,0.0,0.0]", 1),
        text.replacen("\"in_tangent\":[0.0,0.0]", "\"in_tangent\":[0.0]", 1),
        text.replacen("\"speed\":0.0", "\"speed\":0.0,\"extra\":0", 1),
        text.replacen("\"temporal_continuous\":false,", "", 1),
        text.replacen(
            "\"spatial_auto_bezier\":false",
            "\"spatial_auto_bezier\":false,\"extra\":0",
            1,
        ),
        text.replacen("\"Linear\"", "\"AutoBezier\"", 1),
        r#"{"value":[0,0]}"#.to_owned(),
        r#"{"value":[0,0,0],"keys":{}}"#.to_owned(),
    ] {
        assert!(
            serde_json::from_str::<SpatialPosition2>(&changed).is_err(),
            "{changed}"
        );
    }
}

#[test]
fn planar_unsupported_modes_and_nonfinite_metadata_reject_without_repair() {
    for mode in 0..3 {
        let mut t = track([0.0, 0.0], [1.0, 0.0]);
        let key = t.keys.get_mut(&0).unwrap();
        match mode {
            0 => key.temporal_continuous = true,
            1 => key.temporal_auto_bezier = true,
            _ => key.spatial_auto_bezier = true,
        }
        let before = t.clone();
        assert!(matches!(
            t.validate(),
            Err(SpatialError::UnsupportedMode(_))
        ));
        assert!(matches!(
            t.sample(0.0, 0.01),
            Err(SpatialError::UnsupportedMode(_))
        ));
        assert_eq!(t, before);
    }
    let mut t = track([0.0, 0.0], [1.0, 0.0]);
    t.keys.get_mut(&0).unwrap().in_tangent[0] = f64::INFINITY;
    assert_eq!(t.validate(), Err(SpatialError::NonFinite));
    assert_eq!(
        SpatialPosition2::new([f64::NAN, 0.0]).validate(),
        Err(SpatialError::NonFinite)
    );
}

#[test]
fn planar_active_and_dormant_continuity_use_existing_spatial_rules() {
    let mut t = track([0.0, 0.0], [1.0, 1.0]);
    let mut middle = SpatialKey2::new([0.5, 0.5]);
    middle.spatial_continuous = true;
    middle.in_tangent = [-1e-200, -2e-200];
    middle.out_tangent = [1e-100, 2e-100];
    t.keys.insert(50, middle);
    t.validate_sampling_metadata().unwrap();
    t.keys.get_mut(&50).unwrap().out_tangent = [2e-100, 1e-100];
    t.validate().unwrap();
    assert!(matches!(
        t.validate_sampling_metadata(),
        Err(SpatialError::InvalidMetadata(_))
    ));
    t.keys.get_mut(&0).unwrap().out_interpolation = SpatialInterpolation::Hold;
    t.keys.get_mut(&50).unwrap().in_interpolation = SpatialInterpolation::Hold;
    t.validate_sampling(0.01).unwrap();
    close(t.sample(25.0, 0.01).unwrap(), [0.0, 0.0], 0.0);
    t.keys.get_mut(&50).unwrap().spatial_continuous = false;
    t.keys.get_mut(&0).unwrap().out_interpolation = SpatialInterpolation::Linear;
    assert_eq!(t.sample(25.0, 0.01), Err(SpatialError::IncomingHold));
}

#[test]
fn planar_sampler_propagates_work_numeric_and_key_budgets() {
    let mut t = track([0.0, 0.0], [1.0, 1.0]);
    t.keys.get_mut(&0).unwrap().out_tangent = [0.0, 2.0];
    t.keys.get_mut(&100).unwrap().in_tangent = [1.0, -1.0];
    let limited = SamplingOptions {
        max_subdivisions: 1,
        ..SamplingOptions::default()
    };
    assert_eq!(
        t.sample_with_options(50.0, 0.01, limited),
        Err(SpatialError::WorkBudgetExceeded)
    );
    assert_eq!(
        t.validate_sampling_with_options(0.01, limited),
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
    assert_eq!(
        track([-f64::MAX, 0.0], [f64::MAX, 0.0]).sample(50.0, 0.01),
        Err(SpatialError::NumericRange)
    );
    let mut too_many = SpatialPosition2::new([0.0, 0.0]);
    for frame in 0..=MAX_SPATIAL_KEYS as u32 {
        too_many.keys.insert(frame, SpatialKey2::new([0.0, 0.0]));
    }
    assert_eq!(too_many.validate(), Err(SpatialError::KeyBudgetExceeded));
    assert_eq!(
        too_many.sample(0.0, 0.01),
        Err(SpatialError::KeyBudgetExceeded)
    );
}

#[test]
fn planar_addition_keeps_legacy_xyz_wire_and_z_motion() {
    assert_eq!(
        serde_json::to_string(&SpatialPosition3::new([1.0, 2.0, 3.0])).unwrap(),
        r#"{"value":[1.0,2.0,3.0],"keys":{}}"#
    );
    assert!(serde_json::from_str::<SpatialPosition3>(r#"{"keys":{}}"#).is_err());
    assert!(serde_json::from_str::<SpatialPosition3>(r#"{"value":[1,2],"keys":{}}"#).is_err());
    let mut xyz = SpatialPosition3::new([7.0, 8.0, 9.0]);
    xyz.keys = BTreeMap::from([
        (0, SpatialKey3::new([0.0; 3])),
        (100, SpatialKey3::new([0.0; 3])),
    ]);
    xyz.keys.get_mut(&100).unwrap().in_tangent = [0.0, 0.0, 9.0];
    let result = xyz.sample(50.0, 0.01).unwrap();
    assert_eq!(result[0], 0.0);
    assert_eq!(result[1], 0.0);
    assert!((result[2] - 4.0).abs() < 3e-6);
    let encoded = serde_json::to_string(&xyz).unwrap();
    assert_eq!(
        serde_json::from_str::<SpatialPosition3>(&encoded).unwrap(),
        xyz
    );
}
