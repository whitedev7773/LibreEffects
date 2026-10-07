use super::*;
use crate::{PathOrder, PathVertex};

fn path(points: &[[f64; 2]], closed: bool) -> VectorPath {
    VectorPath {
        vertices: points.iter().copied().map(PathVertex::corner).collect(),
        closed,
    }
}
fn contour(points: &[[f64; 2]], closed: bool) -> RenderContour {
    RenderContour::new(17, 0, path(points, closed))
}
fn cubics(segments: &[Cubic], closed: bool) -> RenderContour {
    let n = segments.len();
    RenderContour {
        source_id: 17,
        contour_ordinal: 0,
        next_boundary: n as u64 + 1,
        geometry: Geometry::Pieces(
            segments
                .iter()
                .enumerate()
                .map(|(i, &cubic)| Piece {
                    cubic,
                    start: i as u64,
                    end: if closed && i + 1 == n {
                        0
                    } else {
                        i as u64 + 1
                    },
                })
                .collect(),
        ),
    }
}
fn trim(c: &mut RenderContour, s: f64, e: f64, o: f64) -> ContentsRenderBudget {
    let mut budget = ContentsRenderBudget::default();
    c.trim(s, e, o, 41, 8, &mut budget, None).unwrap();
    budget
}
fn pieces(c: &RenderContour) -> &[Piece] {
    match &c.geometry {
        Geometry::Pieces(p) => p,
        _ => panic!("expected derived geometry"),
    }
}
fn ends(c: &RenderContour) -> (Point, Point) {
    let p = pieces(c);
    (p[0].cubic[0], p.last().unwrap().cubic[3])
}
fn near(a: f64, b: f64, t: f64) {
    assert!(
        (a - b).abs() <= t,
        "{a:.17} != {b:.17}; error {} > {t}",
        (a - b).abs()
    );
}
fn measured(segments: &[Cubic], tolerance: f64) -> (I, ContentsRenderBudget) {
    let c = cubics(segments, false);
    let p = pieces(&c);
    let mut budget = ContentsRenderBudget::default();
    let cx = Context {
        frame: 8,
        operator: 41,
        source: 17,
    };
    let mut table = Table::new(p, cx, &mut budget, None).unwrap();
    while table.total().width() > tolerance {
        table.refine_largest(&mut budget, None).unwrap();
    }
    (table.total(), budget)
}

#[test]
fn identity_preserves_source_data_and_skips_all_measurement() {
    let mut source = path(&[[0.1, -0.2], [70.3, 12.4], [4.5, 8.6]], true);
    source.vertices[0].outgoing = [3.7, 8.2];
    source.vertices[1].incoming = [-9.1, 2.3];
    for (start, end) in [(0.0, 100.0), (100.0, 0.0)] {
        for offset in [0.0, 360.0, -360.0, 999999.0] {
            let mut c = RenderContour::new(9, 2, source.clone());
            let original = c.clone();
            let mut budget = ContentsRenderBudget {
                source_node_limit: 0,
                frame_work_limit: 0,
                ..Default::default()
            };
            c.trim(start, end, offset, 41, 8, &mut budget, None)
                .unwrap();
            assert_eq!(c, original);
            assert_eq!(c.svg_data(), source.svg_data());
            assert_eq!(budget.frame_work, 0);
            assert_eq!(c.source_id(), 9);
            assert_eq!(c.contour_ordinal, 2);
        }
    }
    let mut partial = RenderContour::new(9, 0, source);
    trim(&mut partial, 10.0, 80.0, 0.0);
    let before = partial.clone();
    trim(&mut partial, 0.0, 100.0, 88.0);
    assert_eq!(partial, before);
}

#[test]
fn equality_empty_and_full_constant_are_exact() {
    let mut c = contour(&[[1.0, 2.0], [1.0, 2.0]], false);
    let original = c.clone();
    trim(&mut c, 0.0, 100.0, 90.0);
    assert_eq!(c, original);
    trim(&mut c, 20.0, 80.0, 90.0);
    assert!(c.is_empty());
    let mut c = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    trim(&mut c, 50.0, 50.0, 180.0);
    assert!(c.is_empty());
    assert_eq!(c.svg_data(), "");
}

#[test]
fn straight_zero_handles_inverts_arc_length_not_parameter() {
    let mut c = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    let budget = trim(&mut c, 25.0, 75.0, 0.0);
    let (a, b) = ends(&c);
    near(a[0], 25.0, ABS_CUT_BUDGET * 0.5);
    near(b[0], 75.0, ABS_CUT_BUDGET * 0.5);
    assert!(a[0] > 20.0);
    assert!(b[0] < 80.0);
    assert_eq!(pieces(&c).len(), 1);
    assert!(!c.svg_data().contains('Z'));
    eprintln!(
        "trim line work={} subdivisions={} inversions={} nodes={}",
        budget.frame_work,
        budget.subdivision_count,
        budget.inversion_count,
        budget.peak_source_nodes
    );
    let mut swapped = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    trim(&mut swapped, 75.0, 25.0, 0.0);
    assert_eq!(c, swapped);
}

#[test]
fn unequal_segments_use_cumulative_length_and_preserve_interior_controls() {
    let mut c = contour(&[[0.0, 0.0], [10.0, 0.0], [40.0, 0.0], [100.0, 0.0]], false);
    trim(&mut c, 5.0, 70.0, 0.0);
    let p = pieces(&c);
    assert_eq!(p.len(), 3);
    near(p[0].cubic[0][0], 5.0, ABS_CUT_BUDGET * 0.5);
    near(p[2].cubic[3][0], 70.0, ABS_CUT_BUDGET * 0.5);
    assert_eq!(
        p[1].cubic,
        [[10.0, 0.0], [10.0, 0.0], [40.0, 0.0], [40.0, 0.0]]
    );
}

#[test]
fn open_wrap_is_disconnected_even_with_coincident_source_endpoints() {
    let mut c = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    trim(&mut c, 25.0, 75.0, 180.0);
    assert_eq!(c.svg_data().matches('M').count(), 2);
    let p = pieces(&c);
    assert_eq!(p.len(), 2);
    near(p[0].cubic[0][0], 75.0, ABS_CUT_BUDGET * 0.5);
    near(p[1].cubic[3][0], 25.0, ABS_CUT_BUDGET * 0.5);
    let mut c = contour(&[[0.0, 0.0], [100.0, 0.0], [0.0, 0.0]], false);
    trim(&mut c, 25.0, 75.0, 180.0);
    assert_eq!(c.svg_data().matches('M').count(), 2);
}

#[test]
fn closed_original_seam_joins_but_removed_gap_never_does() {
    let square = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let mut c = contour(&square, true);
    trim(&mut c, 25.0, 75.0, 180.0);
    assert_eq!(c.svg_data().matches('M').count(), 1);
    assert!(!c.svg_data().contains('Z'));
    assert_eq!(ends(&c), ([0.0, 100.0], [100.0, 0.0]));
    let mut c = contour(&square, true);
    trim(&mut c, 0.0, 75.0, 0.0);
    trim(&mut c, 25.0, 75.0, 180.0);
    assert_eq!(c.svg_data().matches('M').count(), 2);
    assert!(!c.svg_data().contains('Z'));
}

#[test]
fn successive_trim_uses_combined_survivors_and_boundary_half_open_rules() {
    let mut c = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    trim(&mut c, 25.0, 75.0, 180.0);
    trim(&mut c, 25.0, 75.0, 0.0);
    let p = pieces(&c);
    assert_eq!(p.len(), 2);
    near(p[0].cubic[0][0], 87.5, ABS_CUT_BUDGET);
    near(p[0].cubic[3][0], 100.0, 0.0);
    near(p[1].cubic[0][0], 0.0, 0.0);
    near(p[1].cubic[3][0], 12.5, ABS_CUT_BUDGET);
    // Exact equal-length axis-aligned runs: start selects following, end previous.
    let mut c = cubics(
        &[
            [[75.0, 0.0], [75.0, 0.0], [100.0, 0.0], [100.0, 0.0]],
            [[0.0, 0.0], [0.0, 0.0], [25.0, 0.0], [25.0, 0.0]],
        ],
        false,
    );
    if let Geometry::Pieces(p) = &mut c.geometry {
        p[1].start = 99;
    }
    let mut tail = c.clone();
    trim(&mut tail, 50.0, 100.0, 0.0);
    assert_eq!(pieces(&tail).len(), 1);
    assert_eq!(ends(&tail), ([0.0, 0.0], [25.0, 0.0]));
    trim(&mut c, 0.0, 50.0, 0.0);
    assert_eq!(pieces(&c).len(), 1);
    assert_eq!(ends(&c), ([75.0, 0.0], [100.0, 0.0]));
}

#[test]
fn rectangle_direction_first_vertex_and_anisotropic_space() {
    let source = path(
        &[[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]],
        true,
    );
    let mut c = RenderContour::new(17, 0, source.clone());
    trim(&mut c, 0.0, 50.0, 0.0);
    assert_eq!(ends(&c), ([0.0, 0.0], [100.0, 50.0]));
    let mut reverse = RenderContour::new(17, 0, source.reordered(PathOrder::Reverse).unwrap());
    trim(&mut reverse, 0.0, 50.0, 0.0);
    assert_eq!(ends(&reverse), ([0.0, 0.0], [100.0, 50.0]));
    assert_ne!(c.svg_data(), reverse.svg_data());
    let mut shifted =
        RenderContour::new(17, 0, source.reordered(PathOrder::FirstVertex(1)).unwrap());
    trim(&mut shifted, 0.0, 50.0, 0.0);
    assert_eq!(ends(&shifted), ([100.0, 0.0], [0.0, 50.0]));
    let square = contour(
        &[[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
        true,
    );
    let transform = Affine([2.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let mut parent = square.clone().transformed(transform, 8).unwrap();
    trim(&mut parent, 0.0, 25.0, 0.0);
    near(ends(&parent).1[0], 150.0, ABS_CUT_BUDGET * 0.5);
    let mut child = square;
    trim(&mut child, 0.0, 25.0, 0.0);
    let child = child.transformed(transform, 8).unwrap();
    assert_eq!(ends(&child).1, [200.0, 0.0]);
}

#[test]
fn coincident_endpoint_loops_and_backtracking_are_not_zero() {
    let backtrack = [[0.0, 0.0], [100.0, 0.0], [-100.0, 0.0], [0.0, 0.0]];
    let (enclosure, budget) = measured(&[backtrack], 1e-5);
    let exact = 200.0 / 3.0_f64.sqrt();
    assert!(
        enclosure.lo <= exact && exact <= enclosure.hi,
        "{enclosure:?} expected {exact}"
    );
    let mut c = cubics(&[backtrack], false);
    trim(&mut c, 10.0, 90.0, 0.0);
    assert!(!c.is_empty());
    let looped = [[0.0, 0.0], [100.0, 100.0], [-100.0, 100.0], [0.0, 0.0]];
    let mut c = cubics(&[looped], false);
    trim(&mut c, 25.0, 75.0, 0.0);
    assert!(!c.is_empty());
    assert!(ends(&c).0[1] > 0.0);
    eprintln!(
        "trim cusp fallback work={} nodes={}",
        budget.frame_work, budget.peak_source_nodes
    );
}

#[test]
fn parabola_certified_length_matches_independent_analytic_formula() {
    let c = [
        [0.0, 0.0],
        [1.0 / 3.0, 0.0],
        [2.0 / 3.0, 1.0 / 3.0],
        [1.0, 1.0],
    ];
    let (bound, budget) = measured(&[c], 1e-8);
    let exact = 5.0_f64.sqrt() * 0.5 + 2.0_f64.asinh() * 0.25;
    // Binary64 thirds differ from the ideal parabola by < 2^-54 per control;
    // integrating the derivative difference bounds length perturbation by 2^-50.
    assert!(bound.lo <= exact + 2.0_f64.powi(-50) && bound.hi >= exact - 2.0_f64.powi(-50));
    assert!(bound.width() <= 1e-8);
    assert!(budget.peak_source_nodes < 2048);
}

#[test]
fn tiny_features_are_resolved_or_fail_explicitly_never_silently_empty() {
    let mut c = contour(&[[0.0, 0.0], [1e-8, 0.0]], false);
    trim(&mut c, 49.0, 51.0, 0.0);
    near(ends(&c).0[0], 4.9e-9, 2e-10 / 16.0);
    near(ends(&c).1[0], 5.1e-9, 2e-10 / 16.0);
    let mut c = contour(&[[1e6, 0.0], [1e6 + 1.0, 0.0]], false);
    let before = c.clone();
    let mut b = ContentsRenderBudget::default();
    let result = c.trim(50.0, 50.0 + 1e-12, 0.0, 41, 8, &mut b, None);
    if let Err(error) = result {
        assert!(matches!(
            error.kind,
            ContentsRenderErrorKind::Precision | ContentsRenderErrorKind::WorkLimit
        ));
        assert_eq!(c, before);
    } else {
        assert!(!c.is_empty());
        assert_ne!(ends(&c).0, ends(&c).1);
    }
}

#[test]
fn repeated_operators_have_linear_piece_growth_and_deterministic_work() {
    let initial = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    let run = || {
        let mut c = initial.clone();
        let mut budget = ContentsRenderBudget::default();
        for i in 0..128 {
            c.trim(
                0.0,
                99.0,
                (i * 37 % 360) as f64,
                41 + i,
                8,
                &mut budget,
                None,
            )
            .unwrap();
            assert!(pieces(&c).len() <= 1 + 2 * (i as usize + 1));
        }
        (c, budget)
    };
    let (a, ba) = run();
    let (b, bb) = run();
    assert_eq!(a, b);
    assert_eq!(ba.frame_work, bb.frame_work);
    assert!(pieces(&a).len() <= 257);
    eprintln!(
        "trim128 pieces={} work={} nodes={} subdivisions={} inversions={}",
        pieces(&a).len(),
        ba.frame_work,
        ba.peak_source_nodes,
        ba.subdivision_count,
        ba.inversion_count
    );
}

#[test]
fn ordinary_1024_mixed_cubics_fit_published_work_guards() {
    let curves: Vec<Cubic> = (0..1024)
        .map(|i| {
            let x = i as f64;
            [[x, 0.0], [x + 0.2, 0.5], [x + 0.8, -0.5], [x + 1.0, 0.0]]
        })
        .collect();
    let mut c = cubics(&curves, false);
    let budget = trim(&mut c, 17.0, 83.0, 37.0);
    assert!(!c.is_empty());
    assert!(pieces(&c).len() <= 1026);
    assert!(budget.peak_source_nodes <= TRIM_SOURCE_NODE_LIMIT);
    assert!(budget.frame_work <= TRIM_LAYER_WORK_LIMIT);
    eprintln!(
        "trim1024 work={} nodes={} subdivisions={} inversions={} pieces={}",
        budget.frame_work,
        budget.peak_source_nodes,
        budget.subdivision_count,
        budget.inversion_count,
        pieces(&c).len()
    );
}

#[test]
fn hard_work_output_cancellation_and_nonfinite_errors_retain_context() {
    let c = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    for (source, layer, frame) in [
        (0, usize::MAX, usize::MAX),
        (usize::MAX, 0, usize::MAX),
        (usize::MAX, usize::MAX, 0),
    ] {
        let mut c = c.clone();
        let before = c.clone();
        let mut b = ContentsRenderBudget {
            source_node_limit: source,
            layer_work_limit: layer,
            frame_work_limit: frame,
            ..Default::default()
        };
        let e = c.trim(25.0, 75.0, 0.0, 41, 8, &mut b, None).unwrap_err();
        assert_eq!(e.kind, ContentsRenderErrorKind::WorkLimit);
        assert_eq!(
            (e.operator_id, e.source_id, e.frame),
            (Some(41), Some(17), 8)
        );
        assert_eq!(c, before);
    }
    let mut b = ContentsRenderBudget {
        output_byte_limit: 8,
        ..Default::default()
    };
    assert_eq!(
        c.svg_data_checked(8, &mut b).unwrap_err().kind,
        ContentsRenderErrorKind::OutputLimit
    );
    assert!(b.output_bytes <= 8);
    let mut cancelled = c.clone();
    let e = cancelled
        .trim(
            25.0,
            75.0,
            0.0,
            41,
            8,
            &mut ContentsRenderBudget::default(),
            Some(&|| true),
        )
        .unwrap_err();
    assert_eq!(e.kind, ContentsRenderErrorKind::Cancelled);
    assert_eq!(cancelled, c);
    let e = c
        .transformed(Affine([f64::INFINITY, 0.0, 0.0, 1.0, 0.0, 0.0]), 8)
        .unwrap_err();
    assert_eq!(e.kind, ContentsRenderErrorKind::NonFinite);
}

#[test]
fn begin_layer_preserves_frame_and_output_budgets() {
    let mut c = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    let mut b = trim(&mut c, 25.0, 75.0, 0.0);
    c.svg_data_checked(8, &mut b).unwrap();
    let work = b.frame_work;
    let bytes = b.output_bytes;
    b.begin_layer();
    assert_eq!(b.layer_work, 0);
    assert_eq!(b.frame_work, work);
    assert_eq!(b.output_bytes, bytes);
    c.trim(25.0, 75.0, 0.0, 42, 8, &mut b, None).unwrap();
    assert!(b.frame_work > work);
    assert!(b.layer_work > 0);
}

#[test]
fn identity_transform_uses_legacy_vertex_and_offset_arithmetic() {
    let mut p = path(&[[0.1, 0.2], [10.3, 20.4]], false);
    p.vertices[0].outgoing = [2.7, 4.1];
    p.vertices[1].incoming = [-1.2, 8.3];
    let t = Affine([1.13, 0.07, -0.23, 2.11, 99.3, -88.1]);
    let c = RenderContour::new(17, 0, p.clone())
        .transformed(t, 8)
        .unwrap();
    for v in &mut p.vertices {
        v.position = t.point(v.position);
        v.incoming = t.vector(v.incoming);
        v.outgoing = t.vector(v.outgoing);
    }
    assert_eq!(c.svg_data(), p.svg_data());
}

#[test]
fn interval_arithmetic_contains_exact_representable_and_inexact_examples() {
    assert_eq!(I::point(25.0).div_positive(I::point(100.0)).lo, 0.25);
    assert_eq!(I::point(25.0).div_positive(I::point(100.0)).hi, 0.25);
    let third = I::point(1.0).div_positive(I::point(3.0));
    assert!(third.lo < 1.0 / 3.0 && third.hi > 1.0 / 3.0);
    let s = I::point(2.0).sqrt();
    assert!(s.lo < 2.0_f64.sqrt() && s.hi > 2.0_f64.sqrt());
    let subnormal = I::point(f64::from_bits(1)).scale(0.5);
    assert_eq!(subnormal.lo, 0.0_f64.next_down());
    assert_eq!(subnormal.hi, f64::from_bits(1));
    let big = I::point(1e16).add(I::point(1.0));
    assert!(big.lo <= 1e16 && big.hi >= 1e16 + 2.0);
}

#[test]
fn exact_shared_modulo_numerator_keeps_ordinary_decimal_boundaries() {
    for (start, end, offset) in [
        (10.0, 60.0, 324.0),
        (12.5, 62.5, 315.0),
        (37.5, 87.5, -135.0),
    ] {
        let mut c = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
        trim(&mut c, start, end, offset);
        assert_eq!(ends(&c).0, [0.0, 0.0]);
        near(ends(&c).1[0], 50.0, ABS_CUT_BUDGET * 0.5);
    }
}

#[test]
fn common_diagonal_and_translated_cubic_lengths_resolve_exact_gap_equality() {
    for segments in [
        vec![
            [[0.0, 0.0], [0.0, 0.0], [25.0, 25.0], [25.0, 25.0]],
            [
                [1000.0, 1000.0],
                [1000.0, 1000.0],
                [1025.0, 1025.0],
                [1025.0, 1025.0],
            ],
        ],
        vec![
            [[0.0, 0.0], [8.0, 20.0], [24.0, -20.0], [32.0, 0.0]],
            [
                [1024.0, 512.0],
                [1032.0, 532.0],
                [1048.0, 492.0],
                [1056.0, 512.0],
            ],
        ],
    ] {
        let mut c = cubics(&segments, false);
        if let Geometry::Pieces(p) = &mut c.geometry {
            p[1].start = 99;
        }
        let mut following = c.clone();
        trim(&mut following, 50.0, 100.0, 0.0);
        assert_eq!(pieces(&following).len(), 1);
        assert_eq!(pieces(&following)[0].cubic, segments[1]);
        trim(&mut c, 0.0, 50.0, 0.0);
        assert_eq!(pieces(&c).len(), 1);
        assert_eq!(pieces(&c)[0].cubic, segments[0]);
    }
}

#[test]
fn near_disconnected_gap_cuts_choose_certified_geometric_side() {
    let diagonal = [
        [[0.0, 0.0], [0.0, 0.0], [25.0, 25.0], [25.0, 25.0]],
        [
            [100000.0, 100000.0],
            [100000.0, 100000.0],
            [100025.0, 100025.0],
            [100025.0, 100025.0],
        ],
    ];
    for fraction in [49.999999, 50.000001] {
        let mut c = cubics(&diagonal, false);
        if let Geometry::Pieces(p) = &mut c.geometry {
            p[1].start = 99;
        }
        trim(&mut c, fraction, 75.0, 0.0);
        let x = ends(&c).0[0];
        if fraction < 50.0 {
            assert!(x < 100.0, "wrong side of gap: {x}");
        } else {
            assert!(x > 99999.0, "wrong side of gap: {x}");
        }
    }
}

#[test]
fn output_copy_accounting_does_not_reduce_the_final_svg_ceiling() {
    let c = contour(&[[0.0, 0.0], [100.0, 0.0]], false);
    let size = c.svg_data().len();
    let mut b = ContentsRenderBudget {
        output_byte_limit: size,
        ..Default::default()
    };
    for _ in 0..20 {
        assert_eq!(c.svg_data_checked(8, &mut b).unwrap().len(), size);
    }
    assert_eq!(b.output_bytes, size * 20);
    assert!(b.check_output_size(size + 1, 8).is_err());
}

fn oracle_number(value: &serde_json::Value) -> f64 {
    value.as_str().unwrap().parse().unwrap()
}
fn oracle_segments(case: &serde_json::Value) -> Vec<Cubic> {
    case["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|segment| {
            std::array::from_fn(|i| std::array::from_fn(|axis| segment[i][axis].as_f64().unwrap()))
        })
        .collect()
}
fn local_speed_upper(c: Cubic, t: I) -> f64 {
    let d = [
        ip_scale(ip_sub(ip(c[1]), ip(c[0])), 3.0),
        ip_scale(ip_sub(ip(c[2]), ip(c[1])), 3.0),
        ip_scale(ip_sub(ip(c[3]), ip(c[2])), 3.0),
    ];
    let u = I::point(1.0).sub(t);
    norm(std::array::from_fn(|axis| {
        d[0][axis]
            .mul(u.square())
            .add(d[1][axis].mul(u).mul(t).scale(2.0))
            .add(d[2][axis].mul(t.square()))
    }))
    .hi
}

#[test]
fn independent_high_precision_oracles_enclose_lengths_and_bound_cut_arc_residuals() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/trim-arc-oracles.json")).unwrap();
    let mut checked_cuts = 0;
    let mut checked_half_allocations = 0;
    let mut explicit_failures = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["id"].as_str().unwrap();
        let curves = oracle_segments(case);
        let c = cubics(&curves, case["closed"].as_bool().unwrap());
        let p = pieces(&c);
        let reference = oracle_number(&case["total_length"]);
        let uncertainty = oracle_number(&case["convergence"]["empirical_reference_uncertainty"]);
        let reference_interval = I {
            lo: case["total_length_f64_lower"].as_f64().unwrap(),
            hi: case["total_length_f64_upper"].as_f64().unwrap(),
        };
        let cx = Context {
            frame: 8,
            operator: 41,
            source: 17,
        };
        let mut budget = ContentsRenderBudget::default();
        let mut table = Table::new(p, cx, &mut budget, None).unwrap();
        assert!(
            table.total().lo <= reference_interval.lo && table.total().hi >= reference_interval.hi,
            "initial {name}: {:?} excludes {reference}",
            table.total()
        );
        if reference == 0.0 {
            assert_eq!(table.total().hi, 0.0);
            continue;
        }
        let mut failed = false;
        while table.total().lo <= 0.0 {
            if let Err(e) = table.refine_largest(&mut budget, None) {
                explicit_failures.push((name, e.kind));
                failed = true;
                break;
            }
        }
        if failed {
            continue;
        }
        let minimum = case["cuts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["fraction"].as_f64().unwrap())
            .filter(|q| *q > 0.0 && *q < 1.0)
            .map(|q| q.min(1.0 - q))
            .fold(1.0, f64::min);
        let cut_budget = ABS_CUT_BUDGET.min(mul_down(mul_down(table.total().lo, minimum), 0.125));
        while table.total().width() > cut_budget * 0.125 {
            if let Err(e) = table.refine_largest(&mut budget, None) {
                explicit_failures.push((name, e.kind));
                failed = true;
                break;
            }
            assert!(
                table.total().lo <= reference_interval.lo
                    && table.total().hi >= reference_interval.hi,
                "refined {name}: {:?} excludes {reference}",
                table.total()
            );
        }
        if failed {
            continue;
        }
        for reference_cut in case["cuts"].as_array().unwrap() {
            let q = reference_cut["fraction"].as_f64().unwrap();
            let cut = match table.invert(
                I::point(q),
                I::point(q).scale(1800.0),
                cut_budget,
                p,
                &mut budget,
                None,
            ) {
                Ok(c) => c,
                Err(e) => {
                    explicit_failures.push((name, e.kind));
                    break;
                }
            };
            let reference_piece = reference_cut["segment_index"].as_u64().unwrap() as usize;
            let reference_t_lo = reference_cut["t_f64_lower"].as_f64().unwrap();
            let reference_t_hi = reference_cut["t_f64_upper"].as_f64().unwrap();
            // The oracle independently integrated the exact input derivative at
            // 80 and 120 decimal digits. Its root interval is outward rounded by
            // exact rational comparison of the stored decimal and binary64.
            // Bounding speed only between that root and our dyadic
            // parameter bounds actual arc residual independently of production's
            // chord/quadrature length table, without a loose coordinate tolerance.
            let residual_upper = if cut.piece == reference_piece {
                let bracket = I {
                    lo: cut.t.min(reference_t_lo).max(0.0),
                    hi: cut.t.max(reference_t_hi).min(1.0),
                };
                add_up(
                    mul_up(
                        local_speed_upper(curves[cut.piece], bracket),
                        bracket.width(),
                    ),
                    uncertainty,
                )
            } else {
                assert!(
                    cut.t == 0.0 || cut.t == 1.0,
                    "{name}: unexpected distant piece {cut:?}"
                );
                let boundary = cut.piece + usize::from(cut.t == 1.0);
                let mut prefix = I::point(0.0);
                for index in 0..boundary {
                    prefix = prefix.add(I {
                        lo: case["segment_lengths_f64_lower"][index].as_f64().unwrap(),
                        hi: case["segment_lengths_f64_upper"][index].as_f64().unwrap(),
                    });
                }
                add_up(
                    prefix
                        .sub(I {
                            lo: reference_cut["target_length_f64_lower"].as_f64().unwrap(),
                            hi: reference_cut["target_length_f64_upper"].as_f64().unwrap(),
                        })
                        .max_abs(),
                    uncertainty,
                )
            };
            assert!(
                residual_upper <= cut_budget,
                "{name} q={q} cut={cut:?}: independently bounded arc residual {residual_upper} > cut budget {cut_budget}"
            );
            let endpoint_rounding = point_rounding(curves[cut.piece], cut.t);
            assert!(
                endpoint_rounding <= cut_budget * 0.5,
                "{name} q={q}: endpoint rounding {endpoint_rounding} exceeds its half-budget allocation {}",
                cut_budget * 0.5
            );
            assert!(
                add_up(residual_upper, endpoint_rounding) <= cut_budget,
                "{name} q={q}: independent arc residual plus endpoint rounding exceeds cut budget {cut_budget}"
            );
            // On these fixed regular fixtures the local maximum-speed bound is
            // sharp enough to test the inversion's separate half allocation.
            // Cusps/backtracking/tiny features retain the combined assertion:
            // maximum speed times parameter width can overbound their arc.
            if matches!(
                name,
                "zero_handle_straight_100"
                    | "unequal_straight_chain"
                    | "unequal_dyadic_boundary_chain"
                    | "unequal_rectangle"
                    | "unequal_rectangle_reversed"
                    | "unequal_rectangle_shifted_first"
                    | "actual_binary64_parabola"
                    | "kappa_ellipse_quarter_actual_controls"
                    | "kappa_ellipse_closed_actual_controls"
                    | "kappa_rounded_corner_actual_controls"
                    | "kappa_rounded_rectangle_actual_controls"
                    | "seeded_normal"
            ) {
                assert!(
                    residual_upper <= cut_budget * 0.5,
                    "{name} q={q}: independent arc residual {residual_upper} exceeds inversion half-budget {}",
                    cut_budget * 0.5
                );
                checked_half_allocations += 1;
            }
            let got = if cut.t == 0.0 {
                curves[cut.piece][0]
            } else if cut.t == 1.0 {
                curves[cut.piece][3]
            } else {
                split(curves[cut.piece], cut.t).0[3]
            };
            for axis in 0..2 {
                let expected = oracle_number(&reference_cut["point"][axis]);
                let parse = reference_cut["point_f64_upper"][axis].as_f64().unwrap()
                    - reference_cut["point_f64_lower"][axis].as_f64().unwrap();
                near(got[axis], expected, cut_budget + parse);
            }
            checked_cuts += 1;
        }
        eprintln!(
            "oracle {name}: work={} nodes={} width={}",
            budget.frame_work,
            budget.peak_source_nodes,
            table.total().width()
        );
    }
    for (name, kind) in &explicit_failures {
        assert_eq!(
            *name, "source_limit_close_controls",
            "unexpected guarded oracle case {name}: {kind:?}"
        );
        assert!(matches!(
            kind,
            ContentsRenderErrorKind::Precision | ContentsRenderErrorKind::WorkLimit
        ));
    }
    assert!(
        checked_cuts >= 120,
        "only {checked_cuts} independently checked cuts"
    );
    assert_eq!(
        checked_half_allocations, 84,
        "regular allocation coverage changed"
    );
    eprintln!(
        "independent oracle cuts={checked_cuts}; half-allocation cuts={checked_half_allocations}; explicit resolution/work failures={explicit_failures:?}"
    );
}

#[test]
fn independent_mixed_1024_segment_stress_enclosure_and_work() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/trim-arc-oracles.json")).unwrap();
    let stress = &fixture["stress_cases"][0];
    let id = stress["block_case_id"].as_str().unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    let block = oracle_segments(case);
    let curves: Vec<_> = (0..stress["repeat_count"].as_u64().unwrap())
        .flat_map(|_| block.iter().copied())
        .collect();
    assert_eq!(curves.len(), 1024);
    let (bound, budget) = measured(&curves, ABS_CUT_BUDGET * 0.125);
    let reference = oracle_number(&stress["total_length"]);
    assert!(bound.lo <= down(reference) && bound.hi >= up(reference));
    let mut c = cubics(&curves, true);
    let trim_budget = trim(&mut c, 17.0, 83.0, 37.0);
    eprintln!(
        "oracle mixed1024 measurement work={} nodes={}; trim work={} nodes={} subdivisions={} inversions={}",
        budget.frame_work,
        budget.peak_source_nodes,
        trim_budget.frame_work,
        trim_budget.peak_source_nodes,
        trim_budget.subdivision_count,
        trim_budget.inversion_count
    );
}

#[test]
fn valid_document_subnormal_feature_is_a_precision_error() {
    let mut c = contour(&[[20.0, 60.0], [180.0, 60.0]], false);
    let before = c.clone();
    let mut budget = ContentsRenderBudget::default();
    let error = c
        .trim(0.0, 1e-300, 0.0, 41, 8, &mut budget, None)
        .unwrap_err();
    assert_eq!(error.kind, ContentsRenderErrorKind::Precision);
    assert_eq!(c, before);
    assert!(budget.peak_source_nodes < 1024);
}

#[test]
fn two_boundary_de_casteljau_preserves_exact_polynomial_controls() {
    // Reference cubic is exactly (3t,3t²); substitute t=1/4+u/2 and
    // convert its power polynomial analytically to Bernstein controls.
    let original = [[0.0, 0.0], [1.0, 0.0], [2.0, 1.0], [3.0, 3.0]];
    assert_eq!(
        subcubic(original, 0.25, 0.75),
        [
            [0.75, 0.1875],
            [1.25, 0.4375],
            [1.75, 0.9375],
            [2.25, 1.6875]
        ]
    );
    assert_eq!(subcubic(original, 0.0, 1.0), original);
}

#[test]
fn nearly_closed_long_curve_does_not_freeze_a_tiny_chord_budget() {
    let curve = [[0.0, 0.0], [100.0, 100.0], [-100.0, 100.0], [1e-12, 0.0]];
    let mut c = cubics(&[curve], false);
    let budget = trim(&mut c, 25.0, 75.0, 0.0);
    assert!(!c.is_empty());
    assert!(budget.peak_source_nodes < 4096);
}

#[test]
fn cancellation_is_checked_within_bounded_work_batches() {
    let calls = std::cell::Cell::new(0usize);
    let cancel = || {
        let n = calls.get() + 1;
        calls.set(n);
        n >= 3
    };
    let curves: Vec<Cubic> = (0..1024)
        .map(|i| {
            let x = i as f64;
            [[x, 0.0], [x + 0.2, 0.5], [x + 0.8, -0.5], [x + 1.0, 0.0]]
        })
        .collect();
    let mut c = cubics(&curves, false);
    let before = c.clone();
    let mut budget = ContentsRenderBudget::default();
    let error = c
        .trim(25.0, 75.0, 0.0, 41, 8, &mut budget, Some(&cancel))
        .unwrap_err();
    assert_eq!(error.kind, ContentsRenderErrorKind::Cancelled);
    assert!(budget.frame_work <= 128);
    assert_eq!(calls.get(), 3);
    assert_eq!(c, before);
}

#[test]
fn independent_oracles_cover_full_trim_normalization_and_extraction() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/trim-arc-oracles.json")).unwrap();
    // Expected fractional boundaries are explicit oracle references, independent
    // of the production percentage/offset normalization arithmetic.
    let operations = [
        (0.0, 25.0, 0.0, 0.0, 0.25),
        (0.0, 50.0, 0.0, 0.0, 0.5),
        (0.0, 75.0, 0.0, 0.0, 0.75),
        (25.0, 75.0, 0.0, 0.25, 0.75),
        (75.0, 25.0, 0.0, 0.25, 0.75),
        (25.0, 75.0, 180.0, 0.75, 0.25),
    ];
    let mut attempts = 0;
    let mut successful = 0;
    let mut zero_length = 0;
    let mut guarded = 0;
    let mut swapped = 0;
    let mut wrapped = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["id"].as_str().unwrap();
        let curves = oracle_segments(case);
        let closed = case["closed"].as_bool().unwrap();
        let source = cubics(&curves, closed);
        let total = case["total_length_f64_upper"].as_f64().unwrap();
        let uncertainty = oracle_number(&case["convergence"]["empirical_reference_uncertainty"]);
        let mut canonical_middle = None;
        for (start, end, offset, expected_start, expected_end) in operations {
            let reference_at = |fraction| {
                case["cuts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|cut| cut["fraction"].as_f64() == Some(fraction))
            };
            let references = if total == 0.0 {
                None
            } else {
                // Some seeded fixtures deliberately omit quarter references.
                // Only existing independent cuts are used; no expected point is
                // generated by the implementation under test.
                let (Some(a), Some(b)) = (reference_at(expected_start), reference_at(expected_end))
                else {
                    continue;
                };
                Some((a, b))
            };
            attempts += 1;
            let mut actual = source.clone();
            let mut budget = ContentsRenderBudget::default();
            match actual.trim(start, end, offset, 41, 8, &mut budget, None) {
                Ok(()) => {}
                Err(error) => {
                    assert_eq!(
                        name, "source_limit_close_controls",
                        "unexpected full-pipeline error: {error}"
                    );
                    assert!(matches!(
                        error.kind,
                        ContentsRenderErrorKind::Precision | ContentsRenderErrorKind::WorkLimit
                    ));
                    assert_eq!(actual, source, "failed extraction changed its input");
                    guarded += 1;
                    continue;
                }
            }
            if total == 0.0 {
                assert!(actual.is_empty());
                zero_length += 1;
                continue;
            }
            assert!(!actual.is_empty(), "{name}: nonempty interval became empty");
            let span = (end - start).abs() / 100.0;
            let cut_budget = ABS_CUT_BUDGET.min(mul_up(mul_up(total, span.min(1.0 - span)), 0.125));
            let (a, b) = references.unwrap();
            let (first, last) = ends(&actual);
            for (point, reference) in [(first, a), (last, b)] {
                let difference = std::array::from_fn(|axis| {
                    I::point(point[axis]).sub(
                        I {
                            lo: reference["point_f64_lower"][axis].as_f64().unwrap(),
                            hi: reference["point_f64_upper"][axis].as_f64().unwrap(),
                        }
                        .add(I {
                            lo: -uncertainty,
                            hi: uncertainty,
                        }),
                    )
                });
                let distance = norm(difference).hi;
                assert!(
                    distance <= cut_budget,
                    "{name} Trim({start},{end},{offset}): emitted endpoint distance {distance} exceeds documented cut budget {cut_budget}"
                );
            }
            let svg = actual.svg_data();
            assert!(!svg.contains('Z'), "partial extraction closed its stroke");
            let expected_runs = if offset == 180.0 && !closed { 2 } else { 1 };
            assert_eq!(
                svg.matches('M').count(),
                expected_runs,
                "{name}: incorrect source seam/gap topology"
            );
            assert!(pieces(&actual).len() <= curves.len() + 2);
            if (start, end, offset) == (25.0, 75.0, 0.0) {
                canonical_middle = Some(actual.clone());
            } else if (start, end, offset) == (75.0, 25.0, 0.0) {
                assert_eq!(
                    Some(&actual),
                    canonical_middle.as_ref(),
                    "{name}: swapped endpoints changed geometry"
                );
                swapped += 1;
            }
            if offset == 180.0 {
                wrapped += 1;
            }
            successful += 1;
        }
    }
    assert_eq!(
        attempts, 119,
        "existing dyadic-oracle pipeline coverage changed"
    );
    assert_eq!(zero_length, 6);
    assert_eq!(successful + zero_length + guarded, attempts);
    assert!(successful >= 107);
    assert!(swapped >= 17 && wrapped >= 17);
    eprintln!(
        "full Trim oracle attempts={attempts}; successful={successful}; zero={zero_length}; guarded={guarded}; swapped={swapped}; wrapped={wrapped}"
    );
}
