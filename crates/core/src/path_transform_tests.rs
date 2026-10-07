use super::*;

fn path(closed: bool) -> VectorPath {
    VectorPath {
        closed,
        vertices: vec![
            PathVertex {
                position: [1., 2.],
                incoming: [2., 3.],
                outgoing: [-4., 5.],
            },
            PathVertex {
                position: [-3., 4.],
                incoming: [-1., 2.],
                outgoing: [6., -2.],
            },
            PathVertex::corner([9., 10.]),
        ],
    }
}

fn transformed(path: &VectorPath, values: [f64; 7]) -> VectorPath {
    transform_path(path, &[0, 1].into(), &values.into()).unwrap()
}

fn bits(path: &VectorPath) -> Vec<u64> {
    path.vertices
        .iter()
        .flat_map(|v| v.position.into_iter().chain(v.incoming).chain(v.outgoing))
        .map(f64::to_bits)
        .collect()
}

#[test]
fn defaults_and_ui_array_retain_percent_units_and_field_order() {
    assert_eq!(
        PathTransformSpec::default(),
        [0., 0., 0., 100., 100., 0., 0.].into()
    );
    let spec = PathTransformSpec::from([1., 2., 3., 4., 5., 6., 7.]);
    assert_eq!(spec.translation, [1., 2.]);
    assert_eq!(spec.rotation_degrees, 3.);
    assert_eq!(spec.scale_percent, [4., 5.]);
    assert_eq!(spec.pivot, [6., 7.]);
}

#[test]
fn cardinal_signed_scales_and_tangent_vectors_are_exact_on_open_and_closed_paths() {
    for closed in [false, true] {
        let source = path(closed);
        for (angle, sx, sy, anchor, incoming, outgoing) in [
            (90., 100., 100., [-2., 1.], [-3., 2.], [-5., -4.]),
            (-90., 100., 100., [2., -1.], [3., -2.], [5., 4.]),
            (180., 100., 100., [-1., -2.], [-2., -3.], [4., -5.]),
            (270., 100., 100., [2., -1.], [3., -2.], [5., 4.]),
            (0., -100., 100., [-1., 2.], [-2., 3.], [4., 5.]),
            (0., 0., 100., [0., 2.], [0., 3.], [0., 5.]),
            (0., 0., 0., [0., 0.], [0., 0.], [0., 0.]),
            (90., -200., 50., [-1., -2.], [-1.5, -4.], [-2.5, 8.]),
        ] {
            let output = transformed(&source, [7., -11., angle, sx, sy, 0., 0.]);
            assert_eq!(
                output.vertices[0].position,
                [anchor[0] + 7., anchor[1] - 11.]
            );
            assert_eq!(output.vertices[0].incoming, incoming);
            assert_eq!(output.vertices[0].outgoing, outgoing);
            assert_eq!(output.vertices[2], source.vertices[2]);
            assert_eq!(output.closed, closed);
        }
    }
}

#[test]
fn arbitrary_rotation_selected_indices_and_fixed_pivot_match_independent_math() {
    for closed in [false, true] {
        let source = path(closed);
        for indices in [BTreeSet::from([0]), [0, 2].into(), [0, 1, 2].into()] {
            let spec =
                PathTransformSpec::from([13.0625, -7.875, 32.25, -135.5, 62.25, -11.375, 41.0625]);
            let actual = transform_path(&source, &indices, &spec).unwrap();
            let (s, c) = spec.rotation_degrees.to_radians().sin_cos();
            let linear = |v: [f64; 2]| {
                [
                    v[0] * -1.355 * c - v[1] * 0.6225 * s,
                    v[0] * -1.355 * s + v[1] * 0.6225 * c,
                ]
            };
            for (index, after) in actual.vertices.iter().enumerate() {
                let before = source.vertices[index];
                if !indices.contains(&index) {
                    assert_eq!(*after, before);
                    continue;
                }
                let relative = linear([before.position[0] + 11.375, before.position[1] - 41.0625]);
                let expected = [
                    relative[0] - 11.375 + 13.0625,
                    relative[1] + 41.0625 - 7.875,
                ];
                for (actual, expected) in [
                    (after.position, expected),
                    (after.incoming, linear(before.incoming)),
                    (after.outgoing, linear(before.outgoing)),
                ] {
                    for axis in 0..2 {
                        assert!((actual[axis] - expected[axis]).abs() < 1e-12);
                    }
                }
            }
        }
    }
}

#[test]
fn exact_identity_full_turns_and_fixed_points_preserve_original_bits() {
    let mut source = path(false);
    source.vertices[0].incoming = [-0., 0.];
    source.vertices[1].outgoing = [0., -0.];
    for values in [
        [0., 0., 0., 100., 100., 0., 0.],
        [-0., 0., -1080., 100., 100., 1e308, -1e308],
        [0., 0., 180., -100., -100., -1e308, 1e308],
    ] {
        assert_eq!(bits(&transformed(&source, values)), bits(&source));
    }
    source.vertices = vec![PathVertex::corner([0.125, -0.0625]); 3];
    for values in [
        [0., 0., 42.25, 0., 0., 0.125, -0.0625],
        [0., 0., 180., 100., 100., 0.125, -0.0625],
        [0., 0., 0., -125., 350., 0.125, -0.0625],
    ] {
        assert_eq!(bits(&transformed(&source, values)), bits(&source));
    }
}

#[test]
fn large_pivots_tiny_turns_near_unit_scales_and_cardinal_swaps_keep_real_motion() {
    let mut source = path(false);
    source.vertices[0] = PathVertex {
        position: [0., 31.123456789012345],
        incoming: [3.123456789012345, 0.],
        outgoing: [0., 2.987654321098765],
    };
    source.vertices[1] = PathVertex::corner([0., 52.987654321098765]);
    let moved = transformed(&source, [1e-16, 0., 0., 100., 100., 1e300, -1e300]);
    assert_eq!(moved.vertices[0].position[0], 1e-16);
    assert_eq!(
        moved.vertices[0].position[1].to_bits(),
        source.vertices[0].position[1].to_bits()
    );
    assert_eq!(moved.vertices[0].incoming, source.vertices[0].incoming);
    let turned = transformed(&source, [0., 0., -1e-16, 100., 100., 0., 0.]);
    assert!(turned.vertices[0].position[0] > 0.);
    assert!(turned.vertices[0].incoming[1] < 0.);
    source.vertices[0].position = [1e-20, 2e-20];
    source.vertices[1].position = [3e-20, 4e-20];
    let swapped = transformed(&source, [0., 0., 90., 100., -100., 1e300, 1e300]);
    assert_eq!(swapped.vertices[0].position, [2e-20, 1e-20]);
    source.vertices[0].position = [1., 2.];
    let scaled = transformed(&source, [0., 0., 0., 100.00000000000003, 100., 1e12, 0.]);
    assert!(scaled.vertices[0].position[0] < 1.);
    assert!(scaled.vertices[0].position[0] > 0.999);
    let tiny = transformed(&source, [0., 0., 0., 1e-18, 100., 0., 0.]);
    assert_eq!(tiny.vertices[0].position[0], 1e-18_f64 / 100.);
}

#[test]
fn collapse_is_exact_at_nonbinary_pivots_and_disparate_cardinal_fixed_points() {
    let source = path(false);
    let output = transformed(&source, [0.17, -0.29, 37.1, 0., 0., 0.13, 0.37]);
    for vertex in &output.vertices[..2] {
        assert_eq!(vertex.position, [0.13 + 0.17, 0.37 - 0.29]);
        assert_eq!(vertex.incoming, [0., 0.]);
        assert_eq!(vertex.outgoing, [0., 0.]);
    }
    for angle in [90., -90., 180., 270.] {
        let mut source = source.clone();
        source.vertices = vec![PathVertex::corner([1e-20, 1e6]); 3];
        let result = transformed(&source, [0., 0., angle, 100., 100., 1e-20, 1e6]);
        assert_eq!(bits(&result), bits(&source));
    }
}

#[test]
fn helper_rejects_invalid_source_selection_and_all_nonfinite_parameters_before_identity() {
    let identity = PathTransformSpec::default();
    let source = path(false);
    for indices in [
        BTreeSet::new(),
        [3].into(),
        [usize::MAX].into(),
        [0, 4].into(),
    ] {
        assert!(transform_path(&source, &indices, &identity).is_err());
    }
    for coordinate in [f64::NAN, f64::INFINITY, -f64::INFINITY, 1_000_001.] {
        let mut invalid = source.clone();
        // Unselected coordinates are also part of the validated source.
        invalid.vertices[2].outgoing[1] = coordinate;
        assert!(transform_path(&invalid, &[0].into(), &identity).is_err());
    }
    for count in [0, 1, 1025] {
        let mut invalid = source.clone();
        invalid.vertices = vec![PathVertex::corner([0., 0.]); count];
        assert!(transform_path(&invalid, &[0].into(), &identity).is_err());
    }
    for index in 0..7 {
        for value in [f64::NAN, f64::INFINITY, -f64::INFINITY] {
            let mut values = [0., 0., 0., 100., 100., 0., 0.];
            values[index] = value;
            assert!(transform_path(&source, &[0].into(), &values.into()).is_err());
        }
    }
}

#[test]
fn valid_extents_allow_large_finite_parameters_but_never_clamp_outputs() {
    for (count, closed) in [(2, false), (3, true), (1024, false), (1024, true)] {
        let mut source = VectorPath {
            closed,
            vertices: vec![PathVertex::corner([-1_000_000., 0.]); count],
        };
        let selected = (0..count).collect();
        let output = transform_path(
            &source,
            &selected,
            &[2_000_000., 0., 0., 100., 100., 0., 0.].into(),
        )
        .unwrap();
        assert_eq!(output.vertices[0].position, [1_000_000., 0.]);
        for values in [
            [2_000_000.01, 0., 0., 100., 100., 0., 0.],
            [0., 0., 0., 1e308, 100., 0., 0.],
        ] {
            assert!(transform_path(&source, &selected, &values.into()).is_err());
        }
        for vertex in &mut source.vertices {
            vertex.position = [0., 0.];
        }
        assert_eq!(
            transform_path(
                &source,
                &selected,
                &[0., 0., 0., 1e308, 100., 0., 0.].into()
            )
            .unwrap(),
            source
        );
    }
}
