use libre_effects_core::*;
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
