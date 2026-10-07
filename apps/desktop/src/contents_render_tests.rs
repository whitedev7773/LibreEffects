use libre_effects_core::*;
#[path = "contents_bulk_fields_render_tests.rs"]
mod bulk_fields;
#[path = "contents_cross_parent_render_tests.rs"]
mod cross_parent;
#[path = "contents_reorder_render_tests.rs"]
mod reorder;
fn edit(e: &mut Editor, edit: ContentsEdit) {
    e.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn value(e: &mut Editor, item: u64, p: ContentsParam, v: f64) {
    edit(
        e,
        ContentsEdit::Track {
            item,
            parameter: p,
            edit: TrackEdit::Value { frame: 0, value: v },
        },
    );
}
fn scene(kind: ShapeKind) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Contents".into(),
        width: 400,
        height: 240,
        fps: 30,
        duration: 120,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Shape(Shape {
            kind,
            stroke_width: 0.,
            ..Default::default()
        }),
        width: 160.,
        height: 100.,
        name: "Contents".into(),
    })
    .unwrap();
    e.execute(Command::SetColor {
        id: 1,
        color: 0x204080,
    })
    .unwrap();
    e
}

#[test]
fn paint_blend_modes_match_reference_colors_alpha_and_group_isolation() {
    // W3C blending equations, independently evaluated for Cb=(51,102,204)/255,
    // Cs=(230,77,26)/255. These are straight colors before alpha compositing.
    let reference = [
        [0.901960784314, 0.301960784314, 0.101960784314],
        [0.2, 0.301960784314, 0.101960784314],
        [0.180392156863, 0.120784313725, 0.081568627451],
        [0.113043478261, 0., 0.],
        [0.901960784314, 0.4, 0.8],
        [0.921568627451, 0.581176470588, 0.820392156863],
        [1., 0.573033707865, 0.890829694323],
        [0.360784313725, 0.241568627451, 0.640784313725],
        [0.399372549020, 0.304941176471, 0.672627450980],
        [0.843137254902, 0.241568627451, 0.163137254902],
        [0.701960784314, 0.098039215686, 0.698039215686],
        [0.741176470588, 0.460392156863, 0.738823529412],
        [0.7155, 0.2655, 0.1155],
        [0.138666666667, 0.405333333333, 0.938666666667],
        [0.826, 0.226, 0.026],
        [0.275960784314, 0.475960784314, 0.875960784314],
    ];
    let renderer = crate::rendering::Renderer::new();
    for (gradient, stroke) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut e = scene(ShapeKind::Rectangle);
        edit(&mut e, ContentsEdit::Promote);
        edit(&mut e, ContentsEdit::Remove(3));
        use ContentsParam::Shape as S;
        for (p, v) in [
            (ShapeParam::FillRed, 51.),
            (ShapeParam::FillGreen, 102.),
            (ShapeParam::FillBlue, 204.),
            (ShapeParam::FillOpacity, 60.),
        ] {
            value(&mut e, 4, S(p), v);
        }
        let kind = match (gradient, stroke) {
            (false, false) => ContentsKind::Fill { even_odd: false },
            (false, true) => ContentsKind::Stroke(ShapeStroke::default()),
            (true, false) => ContentsKind::GradientFill {
                even_odd: false,
                gradient: ShapeGradient::default(),
            },
            (true, true) => ContentsKind::GradientStroke {
                style: ShapeStroke::default(),
                gradient: ShapeGradient::default(),
            },
        };
        edit(&mut e, ContentsEdit::Add { parent: 1, kind });
        if stroke {
            value(&mut e, 5, S(ShapeParam::StrokeWidth), 20.);
        }
        if gradient {
            for stop in [1, 2] {
                for (p, v) in [
                    (GradientParam::Red(stop), 230.),
                    (GradientParam::Green(stop), 77.),
                    (GradientParam::Blue(stop), 26.),
                ] {
                    value(&mut e, 5, ContentsParam::Gradient(p), v);
                }
            }
        } else {
            let channels = if stroke {
                [
                    ShapeParam::StrokeRed,
                    ShapeParam::StrokeGreen,
                    ShapeParam::StrokeBlue,
                ]
            } else {
                [
                    ShapeParam::FillRed,
                    ShapeParam::FillGreen,
                    ShapeParam::FillBlue,
                ]
            };
            for (p, v) in channels.into_iter().zip([230., 77., 26.]) {
                value(&mut e, 5, S(p), v);
            }
        }
        let opacity = S(if stroke {
            ShapeParam::StrokeOpacity
        } else {
            ShapeParam::FillOpacity
        });
        value(&mut e, 5, opacity, 80.);
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 5,
                parameter: opacity,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        );
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 5,
                parameter: opacity,
                edit: TrackEdit::Value {
                    frame: 60,
                    value: 20.,
                },
            },
        );
        for composite in [PaintComposite::BelowPrevious, PaintComposite::AbovePrevious] {
            if composite == PaintComposite::AbovePrevious {
                edit(
                    &mut e,
                    ContentsEdit::Move {
                        item: 5,
                        parent: 1,
                        index: 2,
                    },
                );
                edit(
                    &mut e,
                    ContentsEdit::Composite {
                        item: 5,
                        mode: composite,
                    },
                );
            }
            for (mode, mixed) in PaintBlend::ALL.into_iter().zip(reference) {
                edit(&mut e, ContentsEdit::Blend { item: 5, mode });
                let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
                for frame in [0, 30, 60] {
                    let image = renderer.render(&saved, frame, 400).unwrap();
                    assert_eq!(
                        image,
                        renderer.render_output(&saved, frame, 400, 240).unwrap()
                    );
                    let actual = image.get_pixel(124, 120).0;
                    let a = 0.8 - frame as f64 / 100.;
                    let b = 0.6;
                    let alpha = a + b * (1. - a);
                    for i in 0..3 {
                        let cs = [230., 77., 26.][i] / 255.;
                        let cb = [51., 102., 204.][i] / 255.;
                        let expected = 255.
                            * (a * (1. - b) * cs + a * b * mixed[i] + (1. - a) * b * cb)
                            / alpha;
                        assert!(
                            (actual[i] as f64 - expected).abs() <= 2.5,
                            "{gradient} {stroke} {composite:?} {mode:?} {frame} {actual:?} channel {i} expected {expected}"
                        );
                    }
                    assert!((actual[3] as f64 - alpha * 255.).abs() <= 1.);
                    assert_eq!(image.get_pixel(100, 120).0, [0, 0, 0, 0]);
                }
            }
        }
        // A paint inside a group with no local backdrop keeps its own color;
        // it must not multiply with the parent group's green fill.
        edit(
            &mut e,
            ContentsEdit::Enabled {
                item: 4,
                enabled: false,
            },
        );
        edit(
            &mut e,
            ContentsEdit::Blend {
                item: 5,
                mode: PaintBlend::Multiply,
            },
        );
        value(&mut e, 1, ContentsParam::Transform(Property::Opacity), 50.);
        edit(
            &mut e,
            ContentsEdit::Add {
                parent: 0,
                kind: ContentsKind::Fill { even_odd: false },
            },
        );
        for (p, v) in [
            (ShapeParam::FillRed, 17.),
            (ShapeParam::FillGreen, 231.),
            (ShapeParam::FillBlue, 42.),
        ] {
            value(&mut e, 6, S(p), v);
        }
        for frame in [0, 60] {
            let image = renderer.render(e.project(), frame, 400).unwrap();
            let actual = image.get_pixel(124, 120).0;
            let a = (0.8 - frame as f64 / 100.) * 0.5;
            for i in 0..3 {
                let expected = [230., 77., 26.][i] * a + [17., 231., 42.][i] * (1. - a);
                assert!(
                    (actual[i] as f64 - expected).abs() <= 2.,
                    "isolated {gradient} {stroke} {frame}: {actual:?} expected {expected}"
                );
            }
            assert_eq!(actual[3], 255);
        }
    }
}

#[test]
fn paint_composite_changes_overlap_without_changing_path_order_or_group_scope() {
    let renderer = crate::rendering::Renderer::new();
    // Exercise all four paint kinds against an earlier opaque blue fill.
    for kind in [
        ContentsKind::Fill { even_odd: false },
        ContentsKind::Stroke(ShapeStroke::default()),
        ContentsKind::GradientFill {
            even_odd: false,
            gradient: ShapeGradient::default(),
        },
        ContentsKind::GradientStroke {
            style: ShapeStroke::default(),
            gradient: ShapeGradient::default(),
        },
    ] {
        let gradient = kind.gradient().is_some();
        let stroke = kind.stroke().is_some();
        let mut e = scene(ShapeKind::Rectangle);
        edit(&mut e, ContentsEdit::Promote);
        edit(&mut e, ContentsEdit::Remove(3));
        edit(&mut e, ContentsEdit::Add { parent: 1, kind }); // id 5, initially before Fill 4
        edit(
            &mut e,
            ContentsEdit::Move {
                item: 5,
                parent: 1,
                index: 2,
            },
        );
        use ContentsParam::Shape as S;
        if stroke {
            value(&mut e, 5, S(ShapeParam::StrokeWidth), 20.);
        }
        if gradient {
            for stop in [1, 2] {
                for (p, v) in [
                    (GradientParam::Red(stop), 255.),
                    (GradientParam::Green(stop), 0.),
                    (GradientParam::Blue(stop), 0.),
                ] {
                    value(&mut e, 5, ContentsParam::Gradient(p), v);
                }
            }
        } else {
            for (p, v) in if stroke {
                [
                    (ShapeParam::StrokeRed, 255.),
                    (ShapeParam::StrokeGreen, 0.),
                    (ShapeParam::StrokeBlue, 0.),
                ]
            } else {
                [
                    (ShapeParam::FillRed, 255.),
                    (ShapeParam::FillGreen, 0.),
                    (ShapeParam::FillBlue, 0.),
                ]
            } {
                value(&mut e, 5, S(p), v);
            }
        }
        let opacity = S(if stroke {
            ShapeParam::StrokeOpacity
        } else {
            ShapeParam::FillOpacity
        });
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 5,
                parameter: opacity,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        );
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 5,
                parameter: opacity,
                edit: TrackEdit::Value {
                    frame: 60,
                    value: 0.,
                },
            },
        );
        let original = e.project().clone();
        for mode in [PaintComposite::BelowPrevious, PaintComposite::AbovePrevious] {
            edit(&mut e, ContentsEdit::Composite { item: 5, mode });
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            for frame in [0, 30, 60] {
                let image = renderer.render(&saved, frame, 400).unwrap();
                assert_eq!(
                    image,
                    renderer.render_output(&saved, frame, 400, 240).unwrap()
                );
                let alpha = if mode == PaintComposite::AbovePrevious {
                    1. - frame as f64 / 60.
                } else {
                    0.
                };
                // Left edge interior is covered by both stroke/fill and the earlier fill.
                let actual = image.get_pixel(124, 120).0;
                for (i, bg) in [32., 64., 128.].into_iter().enumerate() {
                    let expected = bg * (1. - alpha) + if i == 0 { 255. * alpha } else { 0. };
                    assert!(
                        (actual[i] as f64 - expected).abs() <= 1.,
                        "{gradient} {stroke} {mode:?} frame {frame}: {actual:?}"
                    );
                }
                assert_eq!(actual[3], 255);
                assert_eq!(image.get_pixel(100, 120).0, [0, 0, 0, 0]);
            }
        }
        e.undo();
        assert_eq!(e.project(), &original);
        e.redo();
        // The nested group's local Above choice must not jump in front of an
        // earlier sibling group. Duplicate group 1 first, then hide its red paint.
        edit(&mut e, ContentsEdit::Duplicate(1));
        edit(
            &mut e,
            ContentsEdit::Move {
                item: 6,
                parent: 0,
                index: 0,
            },
        );
        edit(
            &mut e,
            ContentsEdit::Enabled {
                item: 9,
                enabled: false,
            },
        );
        let image = renderer.render(e.project(), 0, 400).unwrap();
        assert_eq!(image.get_pixel(124, 120).0, [32, 64, 128, 255]);
    }
}
fn gradient_scene(stroke: bool) -> Editor {
    let mut e = scene(ShapeKind::Rectangle);
    edit(&mut e, ContentsEdit::Promote);
    edit(&mut e, ContentsEdit::Remove(3));
    edit(&mut e, ContentsEdit::Remove(4));
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 1,
            kind: if stroke {
                ContentsKind::GradientStroke {
                    style: ShapeStroke::default(),
                    gradient: ShapeGradient::default(),
                }
            } else {
                ContentsKind::GradientFill {
                    even_odd: false,
                    gradient: ShapeGradient::default(),
                }
            },
        },
    );
    value(
        &mut e,
        5,
        ContentsParam::Gradient(GradientParam::EndX),
        160.,
    );
    e
}
#[test]
fn gradient_fill_midpoints_and_independent_alpha_animate_in_preview_and_output() {
    use GradientParam::*;
    let mut e = gradient_scene(false);
    let renderer = crate::rendering::Renderer::new();
    for (p, v) in [
        (Red(1), 255.),
        (Red(2), 0.),
        (Green(2), 0.),
        (ColorMidpoint(1), 25.),
        (Opacity(3), 0.),
    ] {
        value(&mut e, 5, ContentsParam::Gradient(p), v);
    }
    for change in [
        TrackEdit::ToggleAnimation { frame: 0 },
        TrackEdit::Value {
            frame: 60,
            value: 240.,
        },
    ] {
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Gradient(EndX),
                edit: change,
            },
        );
    }
    let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    for frame in [0, 30, 60] {
        let im = renderer.render(&saved, frame, 400).unwrap();
        assert_eq!(im, renderer.render_output(&saved, frame, 400, 240).unwrap());
        for x in 122..278 {
            let t = (x as f64 + 0.5 - 120.) / (160. + frame as f64 * 80. / 60.);
            let blue = t.sqrt(); // 25% color midpoint maps to 50%; opacity remains linear.
            let pixel = im.get_pixel(x, 120).0;
            assert!(
                (pixel[3] as f64 - 255. * t).abs() <= 1.1,
                "{frame}/{x}: {pixel:?}"
            );
            for (i, c) in [1. - blue, 0., blue].into_iter().enumerate() {
                let premult = pixel[i] as f64 * pixel[3] as f64 / 255.;
                assert!(
                    (premult - c * t * 255.).abs() <= 2.,
                    "{frame}/{x}: {pixel:?}"
                );
            }
        }
        assert_eq!(im.get_pixel(119, 120).0, [0; 4]);
    }
}
#[test]
fn gradient_definitions_are_scoped_per_layer_and_strokes_keep_dash_style() {
    use GradientParam::*;
    let mut e = gradient_scene(true);
    let renderer = crate::rendering::Renderer::new();
    value(
        &mut e,
        5,
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        12.,
    );
    edit(&mut e, ContentsEdit::AddDash(5));
    edit(&mut e, ContentsEdit::AddDash(5));
    edit(
        &mut e,
        ContentsEdit::StrokeCap {
            item: 5,
            cap: StrokeCap::Butt,
        },
    );
    for (p, v) in [(Red(1), 255.), (Green(2), 0.), (Blue(2), 0.)] {
        value(&mut e, 5, ContentsParam::Gradient(p), v);
    }
    let original = e.project().clone();
    let single = renderer.render(&original, 0, 400).unwrap();
    assert_eq!(single.get_pixel(125, 67).0, [255, 0, 0, 255]);
    assert_eq!(single.get_pixel(135, 67).0, [0; 4]);
    assert_eq!(single.get_pixel(200, 120).0, [0; 4]);
    e.execute(Command::DuplicateLayer(1)).unwrap();
    for (p, v) in [(Property::PositionX, 60.), (Property::PositionY, 40.)] {
        e.execute(Command::SetValue {
            id: 2,
            property: p,
            frame: 0,
            value: v,
        })
        .unwrap();
    }
    for (p, v) in [(Red(1), 0.), (Red(2), 0.), (Blue(1), 255.), (Blue(2), 255.)] {
        e.execute(Command::Contents {
            id: 2,
            edit: ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Gradient(p),
                edit: TrackEdit::Value { frame: 0, value: v },
            },
        })
        .unwrap();
    }
    let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    let both = renderer.render(&saved, 0, 400).unwrap();
    assert_eq!(both, renderer.render_output(&saved, 0, 400, 240).unwrap());
    assert_eq!(both.get_pixel(125, 67), single.get_pixel(125, 67));
    assert_eq!(both.get_pixel(15, 87).0, [0, 0, 255, 255]);
}
#[test]
fn radial_gradient_highlight_and_collapsed_endpoints_render_deterministically() {
    use GradientParam::*;
    let mut e = gradient_scene(false);
    let renderer = crate::rendering::Renderer::new();
    edit(
        &mut e,
        ContentsEdit::GradientType {
            item: 5,
            radial: true,
        },
    );
    for (p, v) in [
        (StartX, 80.),
        (StartY, 50.),
        (EndX, 120.),
        (EndY, 50.),
        (HighlightLength, 50.),
        (HighlightAngle, 90.),
    ] {
        value(&mut e, 5, ContentsParam::Gradient(p), v);
    }
    let im = renderer.render(e.project(), 0, 400).unwrap();
    // Center (200,120), focus (200,140). Distance along the vertical ray
    // to the near circumference is 20, to the far circumference is 60.
    for y in [90, 100, 110, 120, 130, 145, 150] {
        let px = 0.5f64;
        let py = y as f64 + 0.5 - 140.;
        let a = px * px + py * py;
        let b = 40. * py;
        let c = -1200.;
        let distance = (-b + (b * b - 4. * a * c).sqrt()) / (2. * a);
        let expected = (255. / distance).clamp(0., 255.);
        for channel in &im.get_pixel(200, y).0[..3] {
            assert!((*channel as f64 - expected).abs() <= 1.5);
        }
    }
    for radial in [false, true] {
        edit(&mut e, ContentsEdit::GradientType { item: 5, radial });
        value(&mut e, 5, ContentsParam::Gradient(EndX), 80.);
        let im = renderer.render(e.project(), 0, 400).unwrap();
        assert_eq!(
            im.get_pixel(200, 120).0,
            [255; 4],
            "collapsed radial {radial}"
        );
        assert_eq!(im.get_pixel(130, 80).0, [255; 4]);
    }
}
#[test]
fn animated_group_skew_matches_independent_inverse_geometry_and_output() {
    let renderer = crate::rendering::Renderer::new();
    let mut e = scene(ShapeKind::Rectangle);
    edit(&mut e, ContentsEdit::Promote);
    for (p, v) in [
        (Property::AnchorX, 80.),
        (Property::AnchorY, 50.),
        (Property::PositionX, 80.),
        (Property::PositionY, 50.),
    ] {
        value(&mut e, 1, ContentsParam::Transform(p), v);
    }
    for (parameter, end) in [(ContentsParam::Skew, 60.), (ContentsParam::SkewAxis, 90.)] {
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        );
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 1,
                parameter,
                edit: TrackEdit::Value {
                    frame: 60,
                    value: end,
                },
            },
        );
    }
    let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    for frame in [0, 15, 30, 45, 60] {
        let actual = renderer.render(&saved, frame, 400).unwrap();
        assert_eq!(
            actual,
            renderer.render_output(&saved, frame, 400, 240).unwrap()
        );
        let skew = (frame as f64).to_radians().tan();
        let (s, c) = (frame as f64 * 1.5).to_radians().sin_cos();
        let mut count = 0;
        for y in (0..240).step_by(3) {
            for x in (0..400).step_by(3) {
                // Invert the oriented shear using elementary rotations, not Affine.
                let (px, py) = (x as f64 + 0.5 - 200., y as f64 + 0.5 - 120.);
                let (u, v) = (c * px - s * py, s * px + c * py);
                let u = u + skew * v;
                let (lx, ly) = (c * u + s * v, -s * u + c * v);
                // Skip antialiased boundary samples, including amplified shear edges.
                if (lx.abs() - 80.).abs() < 4. || (ly.abs() - 50.).abs() < 4. {
                    continue;
                }
                let expected = if lx.abs() < 80. && ly.abs() < 50. {
                    [32, 64, 128, 255]
                } else {
                    [0, 0, 0, 0]
                };
                assert_eq!(
                    actual.get_pixel(x, y).0,
                    expected,
                    "frame {frame} at {x},{y}"
                );
                count += 1;
            }
        }
        assert!(count > 9000);
    }
}
#[test]
fn contents_stroke_cap_join_and_dash_edits_match_legacy_stroke_output() {
    let renderer = crate::rendering::Renderer::new();
    for cap in StrokeCap::ALL {
        for join in StrokeJoin::ALL {
            let mut e = scene(ShapeKind::Star);
            let shape = Shape {
                kind: ShapeKind::Star,
                fill: false,
                stroke_width: 12.,
                stroke_style: ShapeStroke {
                    cap,
                    join,
                    dashes: vec![10., 20.],
                    ..Default::default()
                },
                ..Default::default()
            };
            e.execute(Command::SetContent {
                id: 1,
                content: Content::Shape(shape),
            })
            .unwrap();
            // Hold path representation fixed so this checks paint edits rather
            // than polygon-versus-cubic dash rasterization differences.
            e.execute(Command::ConvertShapeToPath { id: 1, frame: 0 })
                .unwrap();
            let legacy = e.project().clone();
            edit(&mut e, ContentsEdit::Promote);
            edit(
                &mut e,
                ContentsEdit::StrokeCap {
                    item: 3,
                    cap: StrokeCap::Butt,
                },
            );
            edit(
                &mut e,
                ContentsEdit::StrokeJoin {
                    item: 3,
                    join: StrokeJoin::Round,
                },
            );
            edit(&mut e, ContentsEdit::RemoveDash(3));
            edit(&mut e, ContentsEdit::AddDash(3));
            value(
                &mut e,
                3,
                ContentsParam::Shape(ShapeParam::DashLength(1)),
                20.,
            );
            edit(&mut e, ContentsEdit::StrokeCap { item: 3, cap });
            edit(&mut e, ContentsEdit::StrokeJoin { item: 3, join });
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            let old = renderer.render(&legacy, 0, 400).unwrap();
            let actual = renderer.render(&saved, 0, 400).unwrap();
            assert_eq!(actual, renderer.render_output(&saved, 0, 400, 240).unwrap());
            assert_eq!(actual, old, "{cap:?} {join:?}");
        }
    }
}
#[test]
fn contents_compound_paths_paint_order_group_opacity_and_animation_render() {
    use ContentsParam::{Shape as S, Transform as T};
    use ShapeParam::*;
    let renderer = crate::rendering::Renderer::new();
    let mut e = scene(ShapeKind::Rectangle);
    edit(&mut e, ContentsEdit::Promote);
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Fill { even_odd: false },
        },
    );
    value(&mut e, 5, S(FillGreen), 0.);
    value(&mut e, 5, S(FillBlue), 0.);
    value(&mut e, 5, S(FillOpacity), 50.);
    let image = renderer.render(e.project(), 0, 400).unwrap();
    let p = image.get_pixel(200, 120).0;
    assert_eq!(p[3], 255);
    assert!(
        p[0].abs_diff(144) <= 1 && p[1].abs_diff(32) <= 1 && p[2].abs_diff(64) <= 1,
        "{p:?}"
    );
    let above = e.project().clone();
    edit(
        &mut e,
        ContentsEdit::Move {
            item: 5,
            parent: 1,
            index: 3,
        },
    );
    assert_eq!(
        renderer
            .render(e.project(), 0, 400)
            .unwrap()
            .get_pixel(200, 120)
            .0,
        [32, 64, 128, 255]
    );
    e.undo();
    assert_eq!(e.project(), &above);
    edit(
        &mut e,
        ContentsEdit::Enabled {
            item: 5,
            enabled: false,
        },
    );
    edit(&mut e, ContentsEdit::Duplicate(2));
    value(&mut e, 6, T(Property::PositionX), 60.);
    value(&mut e, 4, S(FillOpacity), 50.);
    let im = renderer.render(e.project(), 0, 400).unwrap();
    assert_eq!(im.get_pixel(200, 120)[3], 128);
    assert_eq!(im.get_pixel(300, 120)[3], 128);
    edit(
        &mut e,
        ContentsEdit::FillRule {
            item: 4,
            even_odd: true,
        },
    );
    let im = renderer.render(e.project(), 0, 400).unwrap();
    assert_eq!(im.get_pixel(200, 120)[3], 0);
    assert_eq!(im.get_pixel(140, 120)[3], 128);
    e.undo();
    value(&mut e, 1, T(Property::Opacity), 50.);
    assert_eq!(
        renderer
            .render(e.project(), 0, 400)
            .unwrap()
            .get_pixel(200, 120)[3],
        64
    );
    for edit_track in [
        TrackEdit::ToggleAnimation { frame: 0 },
        TrackEdit::Value {
            frame: 60,
            value: 40.,
        },
    ] {
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 1,
                parameter: T(Property::PositionX),
                edit: edit_track,
            },
        );
    }
    let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    for frame in [0, 30, 60] {
        let preview = renderer.render(&saved, frame, 400).unwrap();
        assert_eq!(
            preview,
            renderer.render_output(&saved, frame, 400, 240).unwrap()
        );
        let left = 120 + frame * 2 / 3;
        assert_eq!(preview.get_pixel(left + 5, 120)[3], 64);
        assert_eq!(preview.get_pixel(left - 5, 120)[3], 0);
    }
}
#[test]
fn contents_migration_keeps_animated_geometry_and_paints_in_preview_and_output() {
    let renderer = crate::rendering::Renderer::new();
    for kind in ShapeKind::ALL {
        let mut e = scene(kind);
        for p in [ShapeParam::FillOpacity, ShapeParam::StrokeWidth] {
            for edit in [
                TrackEdit::ToggleAnimation { frame: 0 },
                TrackEdit::Value {
                    frame: 60,
                    value: 20.,
                },
            ] {
                e.execute(Command::EditShape {
                    id: 1,
                    parameter: p,
                    edit,
                })
                .unwrap();
            }
        }
        let old = e.project().clone();
        edit(&mut e, ContentsEdit::Promote);
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        for f in [0, 30, 60] {
            let a = renderer.render(&old, f, 400).unwrap();
            let b = renderer.render(&saved, f, 400).unwrap();
            assert_eq!(b, renderer.render_output(&saved, f, 400, 240).unwrap());
            let error = a
                .pixels()
                .zip(b.pixels())
                .map(|(a, b)| a[3].abs_diff(b[3]) as f64 / 255.)
                .sum::<f64>();
            assert!(error < 30., "{kind:?} {f}: {error}");
            assert_eq!(a.get_pixel(200, 120), b.get_pixel(200, 120));
        }
    }
}

#[test]
fn nested_contents_groups_compose_transforms_and_isolated_opacity() {
    use ContentsParam::Transform as T;
    let renderer = crate::rendering::Renderer::new();
    let mut e = scene(ShapeKind::Rectangle);
    edit(&mut e, ContentsEdit::Promote);
    edit(
        &mut e,
        ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::Group(vec![]),
        },
    );
    edit(
        &mut e,
        ContentsEdit::Move {
            item: 1,
            parent: 5,
            index: 0,
        },
    );
    value(&mut e, 1, T(Property::PositionX), 20.);
    value(&mut e, 5, T(Property::PositionX), 30.);
    value(&mut e, 1, T(Property::Opacity), 50.);
    value(&mut e, 5, T(Property::Opacity), 50.);
    let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    let im = renderer.render(&saved, 0, 400).unwrap();
    assert_eq!(im, renderer.render_output(&saved, 0, 400, 240).unwrap());
    assert_eq!(im.get_pixel(175, 120)[3], 64);
    assert_eq!(im.get_pixel(165, 120)[3], 0);
    assert_eq!(im.get_pixel(335, 120)[3], 0);
    edit(&mut e, ContentsEdit::Remove(5));
    assert!(
        renderer
            .render(e.project(), 0, 400)
            .unwrap()
            .pixels()
            .all(|p| p[3] == 0)
    );
    e.undo();
    assert_eq!(e.project(), &saved);
}
