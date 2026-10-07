//! Raster regressions for path reindexing: geometry is preserved, but winding and
//! the starting point/direction of distance-along-path paints remain meaningful.
use crate::rendering::Renderer;
use image::RgbaImage;
use libre_effects_core::*;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 144;
const FRAMES: [u32; 6] = [0, 7, 15, 23, 30, 45];

fn scene(content: Content) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Path order rendering".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 60,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content,
            width: WIDTH as f64,
            height: HEIGHT as f64,
            name: "Path".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetColor {
            id: 1,
            color: 0x3269c7,
        })
        .unwrap();
    editor
}

fn curved_path() -> VectorPath {
    VectorPath {
        closed: true,
        vertices: vec![
            PathVertex {
                position: [25., 25.],
                incoming: [-13., 16.],
                outgoing: [21., -12.],
            },
            PathVertex {
                position: [120., 30.],
                incoming: [-19., -8.],
                outgoing: [17., 24.],
            },
            PathVertex {
                position: [132., 105.],
                incoming: [5., -16.],
                outgoing: [-27., 17.],
            },
            PathVertex {
                position: [30., 118.],
                incoming: [20., 8.],
                outgoing: [-12., -22.],
            },
        ],
    }
}

fn morphed(mut path: VectorPath) -> VectorPath {
    for (vertex, [dx, dy]) in
        path.vertices
            .iter_mut()
            .zip([[5., 2.], [-6., 9.], [-14., -8.], [12., -12.]])
    {
        vertex.position[0] += dx;
        vertex.position[1] += dy;
        vertex.incoming[0] *= 0.7;
        vertex.outgoing[1] *= 1.3;
    }
    path
}

fn rectangle(left: f64, top: f64, right: f64, bottom: f64) -> VectorPath {
    VectorPath {
        closed: true,
        vertices: [[left, top], [right, top], [right, bottom], [left, bottom]]
            .map(PathVertex::corner)
            .to_vec(),
    }
}

fn animate(editor: &mut Editor, target: PathTarget, pose: VectorPath) {
    editor
        .execute(Command::AnimatePath {
            id: 1,
            target,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    editor
        .execute(Command::EditPath {
            id: 1,
            target,
            frame: 30,
            path: pose,
        })
        .unwrap();
    editor
        .execute(Command::AnimatePath {
            id: 1,
            target,
            edit: TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Smooth,
            },
        })
        .unwrap();
}

fn promote(editor: &mut Editor, contents: bool) -> PathTarget {
    if contents {
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Promote,
            })
            .unwrap();
        PathTarget::Contents(2)
    } else {
        PathTarget::Shape
    }
}

fn alpha_error(a: &RgbaImage, b: &RgbaImage) -> f64 {
    a.pixels()
        .zip(b.pixels())
        .map(|(a, b)| f64::from(a[3].abs_diff(b[3])) / 255.)
        .sum()
}

fn assert_same_coverage(a: &RgbaImage, b: &RgbaImage, context: &str) {
    assert_eq!(a.dimensions(), b.dimensions());
    // Reversing cubic subdivision can round edge coverage differently. Permit
    // at most 8 fully covered pixels of total error in this 160 x 144 image,
    // and 16/255 at any sample; this cannot hide a displaced edge or a hole.
    // Compare premultiplied color too, so transparent RGB cannot dominate error.
    let mut error = [0.; 4];
    let mut peak: f64 = 0.;
    for (a, b) in a.pixels().zip(b.pixels()) {
        for channel in 0..4 {
            let premultiplied = |p: &image::Rgba<u8>| {
                f64::from(p[channel])
                    * if channel == 3 {
                        1.
                    } else {
                        f64::from(p[3]) / 255.
                    }
            };
            let difference = (premultiplied(a) - premultiplied(b)).abs();
            error[channel] += difference / 255.;
            peak = peak.max(difference);
        }
    }
    assert!(
        peak <= 16. && error.into_iter().all(|e| e <= 8.),
        "{context}: premultiplied pixel error {error:?}, peak {peak}"
    );
}

fn check_reorder(renderer: &Renderer, editor: &mut Editor, target: PathTarget, order: PathOrder) {
    let original = editor.project().clone();
    let property = PropertyPath::Path(target);
    let timing = editor.selected_layer().unwrap().track(property).cloned();
    editor
        .execute(Command::ReorderPath {
            id: 1,
            target,
            order,
        })
        .unwrap();
    assert_eq!(
        editor.selected_layer().unwrap().track(property),
        timing.as_ref()
    );
    let saved = Project::from_json(&editor.project().to_json().unwrap()).unwrap();
    assert_eq!(&saved, editor.project());
    for frame in FRAMES {
        let before = renderer.render_preview(&original, frame, WIDTH).unwrap();
        let after = renderer.render_preview(&saved, frame, WIDTH).unwrap();
        assert_same_coverage(
            &before,
            &after,
            &format!("{target:?} {order:?} frame {frame}"),
        );
        assert_eq!(
            after,
            renderer
                .render_output(&saved, frame, WIDTH, HEIGHT)
                .unwrap(),
            "preview/output differ at frame {frame}"
        );
    }
}

#[test]
fn single_contour_fill_keeps_static_and_animated_coverage_after_reindexing() {
    let renderer = Renderer::new();
    for contents in [false, true] {
        for animated in [false, true] {
            for order in [PathOrder::Reverse, PathOrder::FirstVertex(2)] {
                let path = curved_path();
                let mut editor = scene(Content::Shape(Shape {
                    path: Some(path.clone()),
                    ..Default::default()
                }));
                let target = promote(&mut editor, contents);
                if animated {
                    animate(&mut editor, target, morphed(path));
                    let first = renderer.render(editor.project(), 0, WIDTH).unwrap();
                    let last = renderer.render(editor.project(), 30, WIDTH).unwrap();
                    assert!(alpha_error(&first, &last) > 200.);
                }
                check_reorder(&renderer, &mut editor, target, order);
            }
        }
    }
}

#[test]
fn open_solid_cubic_stroke_keeps_animated_coverage_when_reversed() {
    let renderer = Renderer::new();
    for contents in [false, true] {
        let mut path = curved_path();
        path.closed = false;
        let mut editor = scene(Content::Shape(Shape {
            path: Some(path.clone()),
            fill: false,
            stroke_width: 6.,
            stroke_style: ShapeStroke {
                cap: StrokeCap::Round,
                ..Default::default()
            },
            ..Default::default()
        }));
        let target = promote(&mut editor, contents);
        animate(&mut editor, target, morphed(path));
        check_reorder(&renderer, &mut editor, target, PathOrder::Reverse);
    }
}

#[test]
fn even_odd_mask_reindexing_keeps_animated_coverage_and_mask_settings() {
    let renderer = Renderer::new();
    for mode in [
        PathMaskMode::Add,
        PathMaskMode::Subtract,
        PathMaskMode::Intersect,
        PathMaskMode::None,
    ] {
        for inverted in [false, true] {
            for order in [PathOrder::Reverse, PathOrder::FirstVertex(2)] {
                let path = curved_path();
                let mut editor = scene(Content::Solid);
                editor
                    .execute(Command::SetPathMasks {
                        id: 1,
                        masks: vec![
                            PathMask {
                                path: rectangle(4., 4., 80., 140.),
                                ..Default::default()
                            },
                            PathMask {
                                path: path.clone(),
                                mode,
                                inverted,
                                ..Default::default()
                            },
                        ],
                    })
                    .unwrap();
                let target = PathTarget::Mask(2);
                animate(&mut editor, target, morphed(path));
                for (parameter, start, end) in [
                    (MaskParam::Opacity, 80., 55.),
                    (MaskParam::Feather, 2., 5.),
                    (MaskParam::Expansion, -2., 3.),
                ] {
                    for edit in [
                        TrackEdit::Value {
                            frame: 0,
                            value: start,
                        },
                        TrackEdit::ToggleAnimation { frame: 0 },
                        TrackEdit::Value {
                            frame: 30,
                            value: end,
                        },
                    ] {
                        editor
                            .execute(Command::EditTrack {
                                id: 1,
                                property: PropertyPath::Mask { mask: 2, parameter },
                                edit,
                            })
                            .unwrap();
                    }
                }
                let masks = editor.selected_layer().unwrap().path_masks().to_vec();
                let (svg, _) = crate::path_mask_render::mask(
                    editor.selected_layer().unwrap(),
                    "order-test",
                    15,
                );
                assert!(svg.contains("fill-rule='evenodd'"));
                check_reorder(&renderer, &mut editor, target, order);
                let after = editor.selected_layer().unwrap().path_masks();
                assert_eq!(after[0], masks[0], "another mask was modified");
                assert_eq!(after[1].id, masks[1].id);
                assert_eq!(after[1].mode, masks[1].mode);
                assert_eq!(after[1].inverted, masks[1].inverted);
                assert_eq!(after[1].parameters, masks[1].parameters);
            }
        }
    }
}

#[test]
fn reversing_one_compound_contour_changes_nonzero_hole_but_not_evenodd() {
    let renderer = Renderer::new();
    for even_odd in [false, true] {
        let mut editor = scene(Content::Shape(Shape {
            path: Some(rectangle(20., 20., 140., 124.)),
            ..Default::default()
        }));
        promote(&mut editor, true);
        for edit in [
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::Path {
                    path: rectangle(55., 50., 105., 94.),
                    animation: Default::default(),
                },
            },
            ContentsEdit::FillRule { item: 4, even_odd },
        ] {
            editor.execute(Command::Contents { id: 1, edit }).unwrap();
        }
        animate(
            &mut editor,
            PathTarget::Contents(5),
            rectangle(48., 44., 112., 100.),
        );
        let before = editor.project().clone();
        editor
            .execute(Command::ReorderPath {
                id: 1,
                target: PathTarget::Contents(5),
                order: PathOrder::Reverse,
            })
            .unwrap();
        let saved = Project::from_json(&editor.project().to_json().unwrap()).unwrap();
        for frame in FRAMES {
            let a = renderer.render(&before, frame, WIDTH).unwrap();
            let b = renderer.render(&saved, frame, WIDTH).unwrap();
            assert_eq!(
                b,
                renderer
                    .render_output(&saved, frame, WIDTH, HEIGHT)
                    .unwrap()
            );
            assert_eq!(a.get_pixel(80, 72)[3], if even_odd { 0 } else { 255 });
            assert_eq!(
                b.get_pixel(80, 72)[3],
                0,
                "reversed inner contour is a hole"
            );
            assert_eq!(a.get_pixel(30, 72), b.get_pixel(30, 72));
            assert_eq!(b.get_pixel(30, 72)[3], 255, "outer contour remains filled");
            if even_odd {
                assert_same_coverage(&a, &b, "EvenOdd ignores contour winding");
            } else {
                assert!(alpha_error(&a, &b) > 2_000., "NonZero must expose the hole");
            }
        }
    }
}

#[test]
fn dash_direction_and_start_phase_change_without_changing_dash_data() {
    let renderer = Renderer::new();
    for contents in [false, true] {
        for (closed, order, changed_to_ink, changed_to_gap) in [
            (false, PathOrder::Reverse, 31, 40),
            (true, PathOrder::Reverse, 35, 28),
            (true, PathOrder::FirstVertex(1), 31, 24),
        ] {
            let mut path = rectangle(20., 30., 133., 108.);
            if !closed {
                path.closed = false;
                path.vertices.truncate(2);
            }
            let mut editor = scene(Content::Shape(Shape {
                path: Some(path.clone()),
                fill: false,
                stroke_width: 6.,
                stroke_style: ShapeStroke {
                    dashes: vec![13., 7.],
                    dash_offset: 3.,
                    ..Default::default()
                },
                ..Default::default()
            }));
            for edit in [
                TrackEdit::ToggleAnimation { frame: 0 },
                TrackEdit::Value {
                    frame: 30,
                    value: 8.,
                },
            ] {
                editor
                    .execute(Command::EditShape {
                        id: 1,
                        parameter: ShapeParam::DashOffset,
                        edit,
                    })
                    .unwrap();
            }
            let target = promote(&mut editor, contents);
            for vertex in &mut path.vertices {
                vertex.position[1] += 15.;
            }
            animate(&mut editor, target, path);
            let before = editor.project().clone();
            // Snapshot the complete paint parameters, including animated dash offset.
            let paint = |project: &Project| match project.composition().layer(1).unwrap().content()
            {
                Content::Shape(shape) => {
                    serde_json::to_value((&shape.stroke_style, &shape.parameters)).unwrap()
                }
                Content::ShapeContents(contents) => {
                    serde_json::to_value(contents.node(3).unwrap()).unwrap()
                }
                _ => unreachable!(),
            };
            let original_paint = paint(&before);
            editor
                .execute(Command::ReorderPath {
                    id: 1,
                    target,
                    order,
                })
                .unwrap();
            let saved = Project::from_json(&editor.project().to_json().unwrap()).unwrap();
            assert_eq!(paint(&saved), original_paint);
            for frame in [0, 7, 30] {
                let a = renderer.render(&before, frame, WIDTH).unwrap();
                let b = renderer.render(&saved, frame, WIDTH).unwrap();
                assert_eq!(
                    b,
                    renderer
                        .render_output(&saved, frame, WIDTH, HEIGHT)
                        .unwrap()
                );
                assert!(
                    alpha_error(&a, &b) > 20.,
                    "{target:?} {order:?} frame {frame}"
                );
                if frame == 0 {
                    // Interior samples on the top/only straight segment. Dash
                    // lengths and offset stay fixed while arc-length origin or
                    // direction changes; these differences are intentional.
                    assert_eq!(a.get_pixel(changed_to_ink, 30)[3], 0);
                    assert_eq!(b.get_pixel(changed_to_ink, 30)[3], 255);
                    assert_eq!(a.get_pixel(changed_to_gap, 30)[3], 255);
                    assert_eq!(b.get_pixel(changed_to_gap, 30)[3], 0);
                }
            }
        }
    }
}
