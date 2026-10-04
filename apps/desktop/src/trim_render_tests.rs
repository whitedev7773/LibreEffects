//! Independent compositor references: every expected fragment below is analytic.
//! No reference calls Trim's production geometry or SVG evaluator.
use super::*;
use libre_effects_core::*;

pub(crate) fn scene() -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Trim compositor".into(),
        width: 200,
        height: 120,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::ShapeContents(ShapeContents::default()),
        width: 200.,
        height: 120.,
        name: "Trim line".into(),
    })
    .unwrap();
    e
}
fn edit(e: &mut Editor, edit: ContentsEdit) {
    e.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn add(e: &mut Editor, parent: u64, kind: ContentsKind) -> u64 {
    edit(e, ContentsEdit::Add { parent, kind });
    let Content::ShapeContents(c) = e.project().composition().layer(1).unwrap().content() else {
        unreachable!()
    };
    c.rows().into_iter().map(|(_, _, n)| n.id).max().unwrap()
}
fn track(e: &mut Editor, item: u64, parameter: ContentsParam, frame: u32, value: f64) {
    edit(
        e,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value { frame, value },
        },
    );
}
fn path(e: &mut Editor, parent: u64, points: &[[f64; 2]], closed: bool) -> u64 {
    add(
        e,
        parent,
        ContentsKind::Path {
            path: VectorPath {
                closed,
                vertices: points.iter().copied().map(PathVertex::corner).collect(),
            },
            animation: Default::default(),
        },
    )
}
fn stroke(e: &mut Editor, parent: u64, cap: StrokeCap) -> u64 {
    let item = add(
        e,
        parent,
        ContentsKind::Stroke(ShapeStroke {
            cap,
            join: StrokeJoin::Miter,
            ..Default::default()
        }),
    );
    for (p, v) in [
        (ShapeParam::StrokeWidth, 8.),
        (ShapeParam::StrokeRed, 255.),
        (ShapeParam::StrokeGreen, 255.),
        (ShapeParam::StrokeBlue, 255.),
    ] {
        track(e, item, ContentsParam::Shape(p), 0, v);
    }
    item
}
fn trim(e: &mut Editor, parent: u64, start: f64, end: f64, offset: f64) -> u64 {
    let item = add(e, parent, ContentsKind::TrimPaths);
    for (p, value) in [
        (TrimParam::Start, start),
        (TrimParam::End, end),
        (TrimParam::Offset, offset),
    ] {
        track(e, item, ContentsParam::Trim(p), 0, value);
    }
    item
}
pub(crate) fn partial_scene() -> Editor {
    let mut e = scene();
    path(&mut e, 0, &[[20., 60.], [180., 60.]], false);
    stroke(&mut e, 0, StrokeCap::Butt);
    trim(&mut e, 0, 25., 75., 0.);
    e
}
pub(crate) fn failure_cases() -> Vec<(Editor, ContentsRenderBudget, &'static str)> {
    let mut precision = partial_scene();
    track(
        &mut precision,
        3,
        ContentsParam::Trim(TrimParam::Start),
        0,
        0.,
    );
    track(
        &mut precision,
        3,
        ContentsParam::Trim(TrimParam::End),
        0,
        1e-300,
    );
    vec![
        (
            partial_scene(),
            ContentsRenderBudget {
                frame_work_limit: 0,
                ..Default::default()
            },
            "WorkLimit",
        ),
        (
            partial_scene(),
            ContentsRenderBudget {
                output_byte_limit: 0,
                ..Default::default()
            },
            "OutputLimit",
        ),
        (precision, ContentsRenderBudget::default(), "Precision"),
    ]
}
fn reference(renderer: &Renderer, body: &str) -> image::RgbaImage {
    let pixmap = renderer.raster_canvas(body, 200, 120, 200).unwrap();
    image::RgbaImage::from_raw(
        200,
        120,
        pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let p = p.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            })
            .collect(),
    )
    .unwrap()
}
fn stroke_svg(data: &str, cap: &str) -> String {
    format!(
        "<path d='{data}' fill='none' stroke='white' stroke-width='8' stroke-linecap='{cap}' stroke-linejoin='miter'/>"
    )
}
fn save_difference(
    actual: &image::RgbaImage,
    expected: &image::RgbaImage,
    label: &str,
) -> std::path::PathBuf {
    let name = std::thread::current()
        .name()
        .unwrap_or("trim")
        .replace("::", "-");
    let directory =
        std::env::temp_dir().join(format!("libreeffects-trim-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    actual
        .save(directory.join(format!("{label}-actual.png")))
        .unwrap();
    expected
        .save(directory.join(format!("{label}-reference.png")))
        .unwrap();
    directory
}
fn assert_pixels(actual: &image::RgbaImage, expected: &image::RgbaImage, label: &str) {
    assert_eq!(actual.dimensions(), expected.dimensions());
    let differences: Vec<_> = actual
        .enumerate_pixels()
        .filter_map(|(x, y, p)| {
            let q = expected.get_pixel(x, y);
            (p != q).then_some((x, y, p.0, q.0))
        })
        .collect();
    if !differences.is_empty() {
        let directory = save_difference(actual, expected, label);
        panic!(
            "{label}: {} different pixels; first {:?}; images in {}",
            differences.len(),
            &differences[..differences.len().min(8)],
            directory.display()
        );
    }
}
fn assert_shared_routes(e: &Editor, frame: u32, actual: &image::RgbaImage) {
    let renderer = Renderer::new();
    assert_pixels(
        actual,
        &renderer
            .render_output(e.project(), frame, 200, 120)
            .unwrap(),
        "preview-output",
    );
    let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    assert_pixels(
        actual,
        &renderer.render(&saved, frame, 200).unwrap(),
        "preview-reopened",
    );
}
fn assert_reference(e: &Editor, frame: u32, body: &str) {
    let renderer = Renderer::new();
    let original = e.project().clone();
    let expected = reference(&renderer, body);
    let actual = renderer.render_preview(e.project(), frame, 200).unwrap();
    assert_pixels(&actual, &expected, "analytic-reference");
    assert_shared_routes(e, frame, &actual);
    assert_eq!(
        e.project(),
        &original,
        "rendering must not bake trimmed paths"
    );
}

/// Every pixel is enclosed by independent analytic capsules. Shrinking/expanding
/// each interval by the published 1/1024 source-unit cut budget brackets both
/// endpoint errors. This handles the rasterizer's discrete round-cap sampling
/// without allowing an arbitrary RGBA tolerance or ignoring boundary pixels.
fn assert_round_caps(e: &Editor, runs: &[(f64, f64)], operations: usize) {
    // For these straight runs, later interval boundaries are convex combinations
    // of prior endpoints. Their earlier uncertainty cannot grow; each additional
    // operator contributes at most one further absolute cut budget.
    let cut = operations as f64 / 1024.;
    let original = e.project().clone();
    let Content::ShapeContents(contents) = e.project().composition().layer(1).unwrap().content()
    else {
        unreachable!()
    };
    let sampled = contents.svg_at(0).unwrap();
    let data = sampled
        .split_once("<path d='")
        .unwrap()
        .1
        .split('\'')
        .next()
        .unwrap();
    let measured: Vec<Vec<f64>> = data
        .split('M')
        .skip(1)
        .map(|run| {
            run.split(|c: char| c.is_ascii_whitespace() || c == 'C')
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect()
        })
        .collect();
    assert_eq!(
        measured.len(),
        runs.len(),
        "wraps must retain exactly the independent runs"
    );
    for (coordinates, &(a, b)) in measured.iter().zip(runs) {
        assert_eq!(coordinates.len(), 8, "one retained cubic per straight run");
        assert!((coordinates[0] - a).abs() <= cut);
        assert!((coordinates[6] - b).abs() <= cut);
        if a == 20. {
            assert_eq!(coordinates[0], a);
        }
        if b == 180. {
            assert_eq!(coordinates[6], b);
        }
        assert!(coordinates.iter().skip(1).step_by(2).all(|y| *y == 60.));
        assert!(
            coordinates
                .iter()
                .step_by(2)
                .copied()
                .collect::<Vec<_>>()
                .windows(2)
                .all(|x| x[0] <= x[1])
        );
    }
    let renderer = Renderer::new();
    let body = |delta: f64| {
        stroke_svg(
            &runs
                .iter()
                .map(|(a, b)| {
                    format!(
                        "M{} 60H{}",
                        if *a == 20. { *a } else { a - delta },
                        if *b == 180. { *b } else { b + delta }
                    )
                })
                .collect::<String>(),
            "round",
        )
    };
    let inner = reference(&renderer, &body(-cut));
    let outer = reference(&renderer, &body(cut));
    let actual = renderer.render_preview(e.project(), 0, 200).unwrap();
    let mut differences = Vec::new();
    for (x, y, pixel) in actual.enumerate_pixels() {
        let lo = inner.get_pixel(x, y)[3];
        let hi = outer.get_pixel(x, y)[3];
        assert!(
            lo <= hi,
            "analytic capsule coverage is not monotone at {x},{y}"
        );
        if pixel[3] < lo || pixel[3] > hi || (pixel[3] > 0 && pixel.0[..3] != [255; 3]) {
            differences.push((x, y, lo, pixel.0, hi));
        }
    }
    if !differences.is_empty() {
        let directory = save_difference(&actual, &inner, "capsule-inner");
        save_difference(&actual, &outer, "capsule-outer");
        panic!(
            "{} pixels outside analytic cut-budget capsules; first {:?}; images in {}",
            differences.len(),
            &differences[..differences.len().min(8)],
            directory.display()
        );
    }
    assert_shared_routes(e, 0, &actual);
    assert_eq!(e.project(), &original);
}

#[test]
fn analytic_open_line_uses_arc_length_and_swapped_endpoints() {
    let mut e = partial_scene();
    assert_reference(&e, 0, &stroke_svg("M60 60H140", "butt"));
    track(&mut e, 3, ContentsParam::Trim(TrimParam::Start), 0, 75.);
    track(&mut e, 3, ContentsParam::Trim(TrimParam::End), 0, 25.);
    assert_reference(&e, 0, &stroke_svg("M60 60H140", "butt"));
}

#[test]
fn open_wrap_keeps_separate_caps_and_successive_trim_uses_combined_length() {
    let mut e = scene();
    path(&mut e, 0, &[[20., 60.], [180., 60.]], false);
    stroke(&mut e, 0, StrokeCap::Round);
    trim(&mut e, 0, 0., 50., 270.);
    assert_round_caps(&e, &[(140., 180.), (20., 60.)], 1);
    trim(&mut e, 0, 25., 75., 0.);
    // Each cut remains within the same source-space absolute accuracy contract.
    assert_round_caps(&e, &[(160., 180.), (20., 40.)], 2);
}

#[test]
fn closed_rectangle_wrap_crosses_original_seam_without_extra_cap_or_closure() {
    let mut e = scene();
    path(
        &mut e,
        0,
        &[[40., 40.], [160., 40.], [160., 80.], [40., 80.]],
        true,
    );
    stroke(&mut e, 0, StrokeCap::Round);
    trim(&mut e, 0, 0., 50., 315.);
    assert_reference(&e, 0, &stroke_svg("M40 80V40H160", "round"));
    // Once a gap exists, a later cyclic interval cannot reconnect that gap.
    trim(&mut e, 0, 0., 50., 270.);
    assert_reference(&e, 0, &stroke_svg("M120 40H160M40 80V40", "round"));
}

#[test]
fn trimmed_fill_uses_implicit_subpath_closure_and_even_odd_compound_rule() {
    for even_odd in [false, true] {
        let mut e = scene();
        path(
            &mut e,
            0,
            &[[40., 40.], [160., 40.], [160., 80.], [40., 80.]],
            true,
        );
        path(
            &mut e,
            0,
            &[[40., 40.], [160., 40.], [160., 80.], [40., 80.]],
            true,
        );
        let fill = add(&mut e, 0, ContentsKind::Fill { even_odd });
        for p in [
            ShapeParam::FillRed,
            ShapeParam::FillGreen,
            ShapeParam::FillBlue,
        ] {
            track(&mut e, fill, ContentsParam::Shape(p), 0, 255.);
        }
        trim(&mut e, 0, 0., 50., 0.);
        assert_reference(
            &e,
            0,
            &format!(
                "<path d='M40 40H160V80M40 40H160V80' fill='white' fill-rule='{}'/>",
                if even_odd { "evenodd" } else { "nonzero" }
            ),
        );
    }
}

#[test]
fn frozen_paint_membership_and_operator_scope_preserve_later_source() {
    let mut e = scene();
    let a = path(&mut e, 0, &[[20., 40.], [180., 40.]], false);
    let paint = stroke(&mut e, 0, StrokeCap::Butt);
    let operator = trim(&mut e, 0, 25., 75., 0.);
    let b = path(&mut e, 0, &[[20., 80.], [180., 80.]], false);
    // Path additions retain the legacy front insertion rule. Move B after Trim.
    edit(
        &mut e,
        ContentsEdit::Move {
            item: b,
            parent: 0,
            index: 3,
        },
    );
    assert_reference(&e, 0, &stroke_svg("M60 40H140", "butt"));
    let second = stroke(&mut e, 0, StrokeCap::Butt);
    edit(
        &mut e,
        ContentsEdit::Move {
            item: second,
            parent: 0,
            index: 4,
        },
    );
    assert_reference(&e, 0, &stroke_svg("M60 40H140M20 80H180", "butt"));
    assert_eq!((a, paint, operator, b, second), (1, 2, 3, 4, 5));
}

#[test]
fn parent_trim_measures_descendant_transform_but_does_not_repaint_child() {
    for child_trim in [false, true] {
        let mut e = scene();
        let group = add(&mut e, 0, ContentsKind::Group(vec![]));
        path(
            &mut e,
            group,
            &[[20., 20.], [60., 20.], [60., 60.], [20., 60.]],
            true,
        );
        track(
            &mut e,
            group,
            ContentsParam::Transform(Property::ScaleX),
            0,
            200.,
        );
        if child_trim {
            trim(&mut e, group, 0., 25., 0.);
        } else {
            trim(&mut e, 0, 0., 25., 0.);
        }
        stroke(&mut e, 0, StrokeCap::Butt);
        // Local perimeter 160 => 40 before export; parent perimeter 240 => 60.
        assert_reference(
            &e,
            0,
            &stroke_svg(
                if child_trim {
                    "M40 20H120"
                } else {
                    "M40 20H100"
                },
                "butt",
            ),
        );
    }
    let mut e = scene();
    let group = add(&mut e, 0, ContentsKind::Group(vec![]));
    path(&mut e, group, &[[20., 60.], [180., 60.]], false);
    stroke(&mut e, group, StrokeCap::Butt);
    trim(&mut e, 0, 25., 75., 0.);
    // Parent Trim only changes exported geometry. The child's own paint stays full.
    assert_reference(&e, 0, &stroke_svg("M20 60H180", "butt"));
}

#[test]
fn animated_trim_matches_saved_preview_output_and_nested_source_timing() {
    let mut e = partial_scene();
    track(&mut e, 3, ContentsParam::Trim(TrimParam::Start), 0, 0.);
    track(&mut e, 3, ContentsParam::Trim(TrimParam::End), 0, 0.);
    edit(
        &mut e,
        ContentsEdit::Track {
            item: 3,
            parameter: ContentsParam::Trim(TrimParam::End),
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
    track(&mut e, 3, ContentsParam::Trim(TrimParam::End), 60, 100.);
    for (frame, end) in [(15, 60), (30, 100), (45, 140)] {
        assert_reference(&e, frame, &stroke_svg(&format!("M20 60H{end}"), "butt"));
    }
    let expected = Renderer::new().render(e.project(), 30, 200).unwrap();
    e.execute(Command::Precompose {
        layers: vec![1],
        name: "Trim nested".into(),
    })
    .unwrap();
    assert_pixels(
        &Renderer::new().render(e.project(), 30, 200).unwrap(),
        &expected,
        "nested-timing",
    );
    let source = e.project().active_composition_id();
    e.execute(Command::NewComposition).unwrap();
    e.execute(Command::ConfigureComposition {
        name: "Retimed Trim parent".into(),
        width: 200,
        height: 120,
        fps: 60,
        duration: 180,
    })
    .unwrap();
    e.execute(Command::AddCompositionLayer {
        composition: source,
        frame: 20,
    })
    .unwrap();
    // Parent frame 50 is source frame (50 - 20) * 30 / 60 = 15.
    assert_reference(&e, 50, &stroke_svg("M20 60H60", "butt"));
}

#[test]
fn effects_measure_the_trimmed_svg_once_and_keep_final_layer_prefix() {
    let mut e = scene();
    path(&mut e, 0, &[[-100., 60.], [300., 60.]], false);
    let paint = add(
        &mut e,
        0,
        ContentsKind::GradientStroke {
            style: ShapeStroke::default(),
            gradient: ShapeGradient::default(),
        },
    );
    track(
        &mut e,
        paint,
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        0,
        8.,
    );
    trim(&mut e, 0, 25., 75., 0.);
    let renderer = Renderer::new();
    let mut plain_budget = FrameRenderBudget::default();
    renderer
        .isolated_layer_svg(e.project(), 1, 1, 0, 200, "proof", &mut plain_budget)
        .unwrap();
    assert!(plain_budget.contents.frame_work > 0);
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Add(EffectKind::Brightness),
    })
    .unwrap();
    let mut effect_budget = FrameRenderBudget::default();
    let svg = renderer
        .isolated_layer_svg(e.project(), 1, 1, 0, 200, "proof", &mut effect_budget)
        .unwrap();
    assert_eq!(
        effect_budget.contents.frame_work, plain_budget.contents.frame_work,
        "effect measurement must reuse the single evaluation"
    );
    let filter = svg
        .split_once("<filter id='proof-1-effect-1'")
        .unwrap()
        .1
        .split('>')
        .next()
        .unwrap();
    let number = |name: &str| -> f64 {
        filter
            .split_once(&format!("{name}='"))
            .unwrap()
            .1
            .split('\'')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    // One bounded cut error per endpoint, plus the root bounds' f64->f32
    // conversion (one ULP at 200); bounds union retains the nominal layer size.
    let rounding = 2f64.powi(-16);
    let cut = 1. / 1024.;
    assert!((-cut - rounding..=0.).contains(&number("x")), "{filter}");
    assert!(
        (200. ..=200. + 2. * (cut + rounding)).contains(&number("width")),
        "{filter}"
    );
    assert_eq!(number("y"), 0.);
    assert_eq!(number("height"), 120.);
    assert!(
        svg.contains("id='g70726f6f662d31-2'"),
        "gradient must use exact final layer prefix"
    );
    assert!(svg.contains("stroke='url(#g70726f6f662d31-2)'"));
    let pixels = renderer.render(e.project(), 0, 200).unwrap();
    assert!(pixels.get_pixel(100, 60)[3] > 0);
}

#[test]
fn one_frame_budget_accumulates_across_layers_nested_instances_and_mattes() {
    let renderer = Renderer::new();
    let original = partial_scene();
    let mut measured = FrameRenderBudget::default();
    renderer
        .isolated_layer_svg(original.project(), 1, 1, 0, 200, "count", &mut measured)
        .unwrap();
    let one = measured.contents.frame_work;
    assert!(one > 0);
    for nested in [false, true] {
        let mut e = Editor::default();
        e.replace_project(original.project().clone()).unwrap();
        if nested {
            e.execute(Command::Precompose {
                layers: vec![1],
                name: "Budget source".into(),
            })
            .unwrap();
        }
        let first = e.selected().unwrap();
        e.execute(Command::DuplicateLayer(first)).unwrap();
        let budget = ContentsRenderBudget {
            frame_work_limit: one,
            ..Default::default()
        };
        let error =
            with_test_contents_budget(budget, || Renderer::new().render(e.project(), 0, 200))
                .unwrap_err();
        assert!(
            error.contains("WorkLimit")
                && error.contains("Composition")
                && error.contains("layer")
                && error.contains("Trim 3")
                && error.contains("source 1"),
            "{error}"
        );
    }
    let mut e = original;
    e.execute(Command::AddSolid).unwrap();
    e.execute(Command::SetTrackMatte {
        id: 2,
        matte: Some(TrackMatte {
            source: 1,
            mode: MatteMode::Alpha,
        }),
    })
    .unwrap();
    assert!(!e.project().composition().layer(1).unwrap().visible());
    e.execute(Command::DuplicateLayer(2)).unwrap();
    let budget = ContentsRenderBudget {
        frame_work_limit: one,
        ..Default::default()
    };
    let error = with_test_contents_budget(budget, || Renderer::new().render(e.project(), 0, 200))
        .unwrap_err();
    assert!(
        error.contains("WorkLimit") && error.contains("Trim line"),
        "{error}"
    );
    // The original 4096 instance guard remains independent of Trim's work cap.
    let mut budget = FrameRenderBudget {
        layer_instances: 4096,
        ..Default::default()
    };
    assert!(
        renderer
            .isolated_layer_svg(e.project(), 1, 2, 0, 200, "count", &mut budget)
            .unwrap_err()
            .contains("4096")
    );
}

#[test]
fn partial_trim_is_a_real_matte_and_matches_analytic_alpha() {
    let mut e = partial_scene();
    e.execute(Command::AddSolid).unwrap();
    e.execute(Command::SetColor {
        id: 2,
        color: 0xffffff,
    })
    .unwrap();
    e.execute(Command::SetTrackMatte {
        id: 2,
        matte: Some(TrackMatte {
            source: 1,
            mode: MatteMode::Alpha,
        }),
    })
    .unwrap();
    assert!(!e.project().composition().layer(1).unwrap().visible());
    assert_reference(&e, 0, &stroke_svg("M60 60H140", "butt"));
}

#[test]
fn work_output_and_precision_errors_reach_all_renderer_entrypoints_without_a_success_image() {
    for (e, budget, kind) in failure_cases() {
        with_test_contents_budget(budget, || {
            let renderer = Renderer::new();
            for result in [
                renderer.render(e.project(), 0, 200),
                renderer.render_preview(e.project(), 0, 200),
                renderer.render_output(e.project(), 0, 200, 120),
            ] {
                let error = result.unwrap_err();
                assert!(
                    error.contains(kind)
                        && error.contains("Trim compositor")
                        && error.contains("Trim line")
                        && error.contains("frame 0"),
                    "{error}"
                );
            }
        });
    }
    let e = partial_scene();
    let renderer = Renderer::with_cancel(Arc::new(std::sync::atomic::AtomicBool::new(true)));
    assert!(
        renderer
            .render_preview(e.project(), 0, 200)
            .unwrap_err()
            .to_lowercase()
            .contains("cancel")
    );
    assert!(
        renderer
            .render_output(e.project(), 0, 200, 120)
            .unwrap_err()
            .to_lowercase()
            .contains("cancel")
    );
}

#[test]
fn checked_svg_append_counts_wrappers_and_does_not_mutate_on_failure() {
    let payload = "x".repeat(SVG_LIMIT - 1);
    let mut output = String::from("a");
    append_svg(&mut output, format_args!("{payload}")).unwrap();
    assert_eq!(output.len(), SVG_LIMIT);
    let capacity = output.capacity();
    assert!(append_svg(&mut output, format_args!("b")).is_err());
    assert_eq!(output.len(), SVG_LIMIT);
    assert_eq!(
        output.capacity(),
        capacity,
        "rejected append must not reserve first"
    );
    assert!(svg_document(&output, 200., 120.).is_err());
    let mut tiny = String::from("keep");
    assert!(append_svg(&mut tiny, format_args!("{}", output)).is_err());
    assert_eq!(tiny, "keep");
}

#[test]
fn all_primitives_disabled_and_full_trim_preserve_exact_svg_and_pixels() {
    let renderer = Renderer::new();
    for kind in ShapeKind::ALL {
        let mut e = scene();
        let group = add(&mut e, 0, ContentsKind::Group(vec![]));
        let primitive = add(&mut e, group, ContentsKind::Parametric(kind));
        for (parameter, value) in [
            (ContentsParam::Width, 100.),
            (ContentsParam::Height, 70.),
            (ContentsParam::Transform(Property::PositionX), 30.),
            (ContentsParam::Transform(Property::PositionY), 20.),
        ] {
            track(&mut e, primitive, parameter, 0, value);
        }
        let paint = add(
            &mut e,
            group,
            ContentsKind::GradientStroke {
                style: ShapeStroke {
                    cap: StrokeCap::Round,
                    join: StrokeJoin::Bevel,
                    ..Default::default()
                },
                gradient: ShapeGradient::default(),
            },
        );
        track(
            &mut e,
            paint,
            ContentsParam::Shape(ShapeParam::StrokeWidth),
            0,
            6.,
        );
        edit(&mut e, ContentsEdit::AddDash(paint));
        let fill = add(&mut e, group, ContentsKind::Fill { even_odd: true });
        edit(
            &mut e,
            ContentsEdit::Composite {
                item: fill,
                mode: PaintComposite::AbovePrevious,
            },
        );
        edit(
            &mut e,
            ContentsEdit::Blend {
                item: fill,
                mode: PaintBlend::Multiply,
            },
        );
        track(
            &mut e,
            group,
            ContentsParam::Transform(Property::Opacity),
            0,
            70.,
        );
        track(&mut e, group, ContentsParam::Skew, 0, 10.);
        let mut before_budget = FrameRenderBudget::default();
        let before_svg = renderer
            .isolated_layer_svg(e.project(), 1, 1, 0, 200, "identity", &mut before_budget)
            .unwrap();
        let before_pixels = renderer.render(e.project(), 0, 200).unwrap();
        assert!(before_pixels.pixels().any(|p| p[3] > 0));
        let operator = trim(&mut e, group, 0., 100., 0.);
        for (start, end, offset, enabled) in [
            (0., 100., 0., true),
            (100., 0., 720., true),
            (17., 29., -359., false),
        ] {
            for (parameter, value) in [
                (TrimParam::Start, start),
                (TrimParam::End, end),
                (TrimParam::Offset, offset),
            ] {
                track(&mut e, operator, ContentsParam::Trim(parameter), 0, value);
            }
            edit(
                &mut e,
                ContentsEdit::Enabled {
                    item: operator,
                    enabled,
                },
            );
            let mut budget = FrameRenderBudget::default();
            let svg = renderer
                .isolated_layer_svg(e.project(), 1, 1, 0, 200, "identity", &mut budget)
                .unwrap();
            assert_eq!(
                svg, before_svg,
                "{kind:?}, identity/disabled Trim changed SVG bytes"
            );
            assert_pixels(
                &renderer.render_preview(e.project(), 0, 200).unwrap(),
                &before_pixels,
                "identity",
            );
            assert_eq!(
                budget.contents.frame_work, 0,
                "identity must skip measurement"
            );
        }
    }
}

#[test]
fn pre_trim_fixture_svg_keeps_exact_rgba_for_legacy_full_and_disabled_operators() {
    // These SVG strings were generated by the immutable pre-Trim core runner,
    // rather than by the implementation under test. Core pins the exact strings;
    // this test independently pins their compositor output through layer wrappers.
    let contents: ShapeContents = serde_json::from_str(include_str!(
        "../../../crates/core/src/trim_fixtures/legacy_contents.json"
    ))
    .unwrap();
    let samples: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../crates/core/src/trim_fixtures/legacy_svg.json"
    ))
    .unwrap();
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Legacy Trim pixel pin".into(),
        width: 1000,
        height: 700,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::ShapeContents(contents),
        width: 1000.,
        height: 700.,
        name: "Legacy Contents".into(),
    })
    .unwrap();
    let renderer = Renderer::new();
    let references: Vec<_> = samples
        .iter()
        .map(|sample| {
            let frame = sample["frame"].as_u64().unwrap() as u32;
            let pixmap = renderer
                .raster_canvas(sample["svg"].as_str().unwrap(), 1000, 700, 1000)
                .unwrap();
            let image = image::RgbaImage::from_raw(
                1000,
                700,
                pixmap
                    .pixels()
                    .iter()
                    .flat_map(|p| {
                        let p = p.demultiply();
                        [p.red(), p.green(), p.blue(), p.alpha()]
                    })
                    .collect(),
            )
            .unwrap();
            (frame, image)
        })
        .collect();
    for (frame, expected) in &references {
        assert_pixels(
            &renderer.render(e.project(), *frame, 1000).unwrap(),
            expected,
            "pre-trim-legacy",
        );
    }
    let operator = trim(&mut e, 0, 100., 0., 360.);
    for enabled in [true, false] {
        edit(
            &mut e,
            ContentsEdit::Enabled {
                item: operator,
                enabled,
            },
        );
        for (frame, expected) in &references {
            assert_pixels(
                &renderer.render_preview(e.project(), *frame, 1000).unwrap(),
                expected,
                "pre-trim-identity",
            );
        }
    }
}
