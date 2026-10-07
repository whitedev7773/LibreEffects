//! C09 rendering references use explicit source-space geometry and scalar edits,
//! never the production layer-transform planner or bounds helpers.
use crate::rendering::Renderer;
use libre_effects_core::*;
use serde_json::Value;

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;
const OPS: [LayerTransformOp; 5] = [
    LayerTransformOp::ResetScaleRotation,
    LayerTransformOp::FlipHorizontal,
    LayerTransformOp::FlipVertical,
    LayerTransformOp::FitInsideComposition,
    LayerTransformOp::CenterAnchorInSourceBounds,
];
type Change = (LayerId, Property, f64);
type Matrix = [f64; 6];
const IDENTITY: Matrix = [1., 0., 0., 1., 0., 0.];

fn value(e: &mut Editor, id: LayerId, property: Property, frame: Frame, value: f64) {
    e.execute(Command::SetValue {
        id,
        property,
        frame,
        value,
    })
    .unwrap();
}
fn artwork(width: f64, height: f64) -> Content {
    // Deliberately asymmetric in both axes, with painted bounds smaller than
    // the declared source rectangle. Fitting artwork or a mask is incorrect.
    Content::Shape(Shape {
        path: Some(VectorPath {
            closed: true,
            vertices: [
                [0.08, 0.10],
                [0.94, 0.22],
                [0.53, 0.40],
                [0.78, 0.89],
                [0.12, 0.65],
            ]
            .map(|[x, y]| PathVertex::corner([width * x, height * y]))
            .to_vec(),
        }),
        stroke_width: 2.,
        ..Default::default()
    })
}
fn scene(contents: bool, decorated: bool) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Layer transform rendering references".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
        duration: 121,
    })
    .unwrap();
    for (name, width, height, color) in [
        ("Asymmetric child", 160., 100., 0x286acf),
        ("Reflected parent", 220., 140., 0xdf7835),
        ("Independent root", 90., 60., 0x38a476),
    ] {
        e.execute(Command::AddContent {
            content: artwork(width, height),
            width,
            height,
            name: name.into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: e.selected().unwrap(),
            color,
        })
        .unwrap();
    }
    for (id, property, v) in [
        (1, Property::PositionX, 185.25),
        (1, Property::PositionY, 125.5),
        (1, Property::AnchorX, 23.25),
        (1, Property::AnchorY, 11.75),
        (1, Property::ScaleX, -75.),
        (1, Property::ScaleY, 135.),
        (1, Property::Rotation, -19.),
        (1, Property::Opacity, 85.),
        (2, Property::PositionX, 180.),
        (2, Property::PositionY, 135.),
        (2, Property::AnchorX, 75.),
        (2, Property::AnchorY, 42.),
        (2, Property::ScaleX, -125.),
        (2, Property::ScaleY, 80.),
        (2, Property::Rotation, 17.),
        (2, Property::Opacity, 30.),
        (3, Property::PositionX, 315.),
        (3, Property::PositionY, 230.),
        (3, Property::AnchorX, 9.),
        (3, Property::AnchorY, 7.),
        (3, Property::ScaleX, 70.),
        (3, Property::ScaleY, -90.),
        (3, Property::Rotation, 23.),
    ] {
        value(&mut e, id, property, 0, v);
    }
    // Reparenting stores an inverse compensation matrix. Subsequent changes to
    // the rotated, nonuniform, reflected parent make that matrix consequential.
    e.execute(Command::SetParent {
        id: 1,
        parent: Some(2),
        frame: 0,
    })
    .unwrap();
    for (p, v) in [
        (Property::Rotation, 31.),
        (Property::ScaleX, -90.),
        (Property::ScaleY, 135.),
    ] {
        value(&mut e, 2, p, 0, v);
    }
    assert_ne!(offset(e.project(), 1), IDENTITY);
    if contents {
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Promote,
        })
        .unwrap();
        for (parameter, v) in [
            (ContentsParam::Transform(Property::Rotation), 13.),
            (ContentsParam::Transform(Property::ScaleX), -85.),
            (ContentsParam::Skew, 11.),
        ] {
            e.execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Track {
                    item: 1,
                    parameter,
                    edit: TrackEdit::Value { frame: 0, value: v },
                },
            })
            .unwrap();
        }
    }
    if decorated {
        e.execute(Command::SetPathMasks {
            id: 1,
            masks: vec![PathMask {
                path: VectorPath {
                    closed: true,
                    vertices: [[5., 5.], [115., 5.], [115., 90.], [5., 90.]]
                        .map(PathVertex::corner)
                        .to_vec(),
                },
                ..Default::default()
            }],
        })
        .unwrap();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::DropShadow),
        })
        .unwrap();
        for (parameter, v) in [
            (EffectParam::Radius, 8.),
            (EffectParam::OffsetX, 35.),
            (EffectParam::OffsetY, 14.),
        ] {
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::SetValue {
                    effect: 1,
                    parameter,
                    frame: 0,
                    value: v,
                },
            })
            .unwrap();
        }
    }
    e.select(1);
    e.clear_history();
    e
}

fn layer(p: &Project, id: LayerId) -> &Layer {
    p.composition().layer(id).unwrap()
}
fn offset(p: &Project, id: LayerId) -> Matrix {
    serde_json::from_value(serde_json::to_value(layer(p, id)).unwrap()["transform_offset"].clone())
        .unwrap()
}
fn multiply(l: Matrix, r: Matrix) -> Matrix {
    let [a, b, c, d, x, y] = l;
    let [e, f, g, h, u, v] = r;
    [
        a * e + c * f,
        b * e + d * f,
        a * g + c * h,
        b * g + d * h,
        a * u + c * v + x,
        b * u + d * v + y,
    ]
}
fn point(m: Matrix, p: [f64; 2]) -> [f64; 2] {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}
fn local(p: &Project, id: LayerId, frame: Frame, scale_factor: f64) -> Matrix {
    let l = layer(p, id);
    let v = |p| l.property(p).expect("2D fixture property").value_at(frame);
    let angle = v(Property::Rotation) * std::f64::consts::PI / 180.;
    let sx = v(Property::ScaleX) * scale_factor / 100.;
    let sy = v(Property::ScaleY) * scale_factor / 100.;
    let [a, b, c, d] = [
        angle.cos() * sx,
        angle.sin() * sx,
        -angle.sin() * sy,
        angle.cos() * sy,
    ];
    [
        a,
        b,
        c,
        d,
        v(Property::PositionX) - a * v(Property::AnchorX) - c * v(Property::AnchorY),
        v(Property::PositionY) - b * v(Property::AnchorX) - d * v(Property::AnchorY),
    ]
}
fn space(p: &Project, id: LayerId, frame: Frame) -> Matrix {
    let parent = layer(p, id)
        .parent()
        .map_or(IDENTITY, |id| world(p, id, frame));
    multiply(parent, offset(p, id))
}
fn world(p: &Project, id: LayerId, frame: Frame) -> Matrix {
    multiply(space(p, id, frame), local(p, id, frame, 1.))
}
fn corners(p: &Project, id: LayerId, frame: Frame) -> [[f64; 2]; 4] {
    let l = layer(p, id);
    let m = world(p, id, frame);
    [
        [0., 0.],
        [l.width(), 0.],
        [l.width(), l.height()],
        [0., l.height()],
    ]
    .map(|v| point(m, v))
}
fn bounds(points: [[f64; 2]; 4]) -> [f64; 4] {
    points.into_iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        },
    )
}
fn close(a: f64, b: f64) {
    // Independent matrix/corner arithmetic has a different operation order.
    // This is a floating-point scalar oracle budget, never a pixel tolerance.
    let budget = 128. * f64::EPSILON * a.abs().max(b.abs()).max(1.);
    assert!(
        (a - b).abs() <= budget,
        "{a:.17} != {b:.17}, arithmetic budget {budget}"
    );
}
fn reference_changes(
    p: &Project,
    ids: &[LayerId],
    frame: Frame,
    op: LayerTransformOp,
) -> Vec<Change> {
    let mut changes = Vec::new();
    for &id in ids {
        let l = layer(p, id);
        let v = |p| l.property(p).expect("2D fixture property").value_at(frame);
        let edits = match op {
            LayerTransformOp::ResetScaleRotation => vec![
                (Property::ScaleX, 100.),
                (Property::ScaleY, 100.),
                (Property::Rotation, 0.),
            ],
            LayerTransformOp::FlipHorizontal => vec![(Property::ScaleX, -v(Property::ScaleX))],
            LayerTransformOp::FlipVertical => vec![(Property::ScaleY, -v(Property::ScaleY))],
            LayerTransformOp::CenterAnchorInSourceBounds => {
                let m = local(p, id, frame, 1.);
                let dx = l.width() / 2. - v(Property::AnchorX);
                let dy = l.height() / 2. - v(Property::AnchorY);
                vec![
                    (Property::AnchorX, l.width() / 2.),
                    (Property::AnchorY, l.height() / 2.),
                    (
                        Property::PositionX,
                        v(Property::PositionX) + m[0] * dx + m[2] * dy,
                    ),
                    (
                        Property::PositionY,
                        v(Property::PositionY) + m[1] * dx + m[3] * dy,
                    ),
                ]
            }
            LayerTransformOp::FitInsideComposition => {
                // Use translated corners, independently of production's linear
                // extents shortcut. Source bounds ignore masks and effects.
                let b = bounds(corners(p, id, frame));
                let factor = (WIDTH as f64 / (b[2] - b[0])).min(HEIGHT as f64 / (b[3] - b[1]));
                let s = space(p, id, frame);
                let det = s[0] * s[3] - s[1] * s[2];
                let target = [WIDTH as f64 / 2. - s[4], HEIGHT as f64 / 2. - s[5]];
                let local_center = [
                    (s[3] * target[0] - s[2] * target[1]) / det,
                    (-s[1] * target[0] + s[0] * target[1]) / det,
                ];
                let m = local(p, id, frame, factor);
                let dx = l.width() / 2. - v(Property::AnchorX);
                let dy = l.height() / 2. - v(Property::AnchorY);
                vec![
                    (Property::ScaleX, v(Property::ScaleX) * factor),
                    (Property::ScaleY, v(Property::ScaleY) * factor),
                    (Property::PositionX, local_center[0] - m[0] * dx - m[2] * dy),
                    (Property::PositionY, local_center[1] - m[1] * dx - m[3] * dy),
                ]
            }
        };
        changes.extend(
            edits
                .into_iter()
                .filter(|(property, n)| v(*property) != *n)
                .map(|(property, n)| (id, property, n)),
        );
    }
    changes
}
fn reference(p: &Project, frame: Frame, changes: &[Change]) -> Project {
    let mut e = Editor::default();
    e.replace_project(p.clone()).unwrap();
    for &(id, property, v) in changes {
        value(&mut e, id, property, frame, v);
    }
    e.project().clone()
}
fn track_value_slot(p: &mut Value, id: LayerId, property: Property, frame: Frame) -> &mut Value {
    let l = p["composition"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|l| l["id"].as_u64() == Some(id))
        .unwrap();
    let track = &mut l["properties"][format!("{property:?}")];
    if track["keys"].as_object().unwrap().is_empty() {
        &mut track["value"]
    } else {
        &mut track["keys"][frame.to_string()]["value"]
    }
}
fn assert_reference(actual: &Project, expected: &Project, frame: Frame, changes: &[Change]) {
    let mut a = serde_json::to_value(actual).unwrap();
    let mut b = serde_json::to_value(expected).unwrap();
    for &(id, property, _) in changes {
        let av = track_value_slot(&mut a, id, property, frame);
        let bv = track_value_slot(&mut b, id, property, frame);
        close(av.as_f64().unwrap(), bv.as_f64().unwrap());
        *av = bv.clone();
    }
    // Exact equality everywhere else: other frames, static bases, interpolation,
    // handles, geometry, Contents, masks, effects, offsets, IDs and schema.
    assert_eq!(a, b);
}
fn pixels(r: &Renderer, p: &Project, frame: Frame) -> image::RgbaImage {
    let image = r.render_preview(p, frame, WIDTH).unwrap();
    assert_eq!(image, r.render_output(p, frame, WIDTH, HEIGHT).unwrap());
    image
}
fn verify_commit(
    e: &mut Editor,
    ids: Vec<LayerId>,
    roots: &[LayerId],
    frame: Frame,
    op: LayerTransformOp,
    samples: &[Frame],
) {
    let before = e.project().clone();
    let changes = reference_changes(&before, roots, frame, op);
    let expected = reference(&before, frame, &changes);
    let r = Renderer::new();
    let opening = pixels(&r, &before, frame);
    assert!(opening.pixels().filter(|p| p[3] != 0).count() > 100);
    e.execute(Command::TransformLayers {
        ids,
        frame,
        operation: op,
    })
    .unwrap();
    let committed = e.project().clone();
    assert_reference(&committed, &expected, frame, &changes);
    let actual = pixels(&r, &committed, frame);
    assert_eq!(actual, pixels(&r, &expected, frame), "{op:?} frame {frame}");
    if op == LayerTransformOp::CenterAnchorInSourceBounds {
        assert_eq!(
            actual, opening,
            "anchor centering must preserve current pixels exactly"
        );
    } else {
        assert_ne!(actual, opening, "fixture must exercise a visible {op:?}");
    }
    let encoded = crate::project_io::encode_native_project(&committed, None).unwrap();
    let reopened = crate::project_io::decode_project(&encoded).unwrap().project;
    assert_eq!(reopened, committed);
    for &sample in samples {
        assert_eq!(
            pixels(&r, &reopened, sample),
            pixels(&r, &expected, sample),
            "{op:?}, sample {sample}"
        );
    }
    assert!(e.can_undo());
    e.undo();
    assert_eq!(e.project(), &before);
    assert!(!e.can_undo(), "one operation must consume exactly one Undo");
    assert_eq!(pixels(&r, e.project(), frame), opening);
    e.redo();
    assert_eq!(e.project(), &committed);
}

#[test]
fn reset_and_both_flips_render_independent_local_values_and_selected_roots() {
    for op in OPS[..3].iter().copied() {
        for (ids, roots) in [(vec![1], vec![1]), (vec![2, 1, 3, 1], vec![2, 3])] {
            let mut e = scene(false, true);
            verify_commit(&mut e, ids, &roots, 0, op, &[0]);
        }
    }
}

#[test]
fn fit_uses_each_root_source_rectangle_under_parent_offset_and_ignores_painted_bounds() {
    for contents in [false, true] {
        let mut e = scene(contents, true);
        let before = e.project().clone();
        let ids = if contents { vec![2, 1, 3] } else { vec![1, 3] };
        let roots = if contents { vec![2, 3] } else { vec![1, 3] };
        verify_commit(
            &mut e,
            ids,
            &roots,
            0,
            LayerTransformOp::FitInsideComposition,
            &[0],
        );
        for &id in &roots {
            let b = bounds(corners(e.project(), id, 0));
            close((b[0] + b[2]) / 2., WIDTH as f64 / 2.);
            close((b[1] + b[3]) / 2., HEIGHT as f64 / 2.);
            assert!(
                b[0] >= -1e-10
                    && b[1] >= -1e-10
                    && b[2] <= WIDTH as f64 + 1e-10
                    && b[3] <= HEIGHT as f64 + 1e-10
            );
            let fills_width = (b[2] - b[0] - WIDTH as f64).abs() < 1e-10;
            let fills_height = (b[3] - b[1] - HEIGHT as f64).abs() < 1e-10;
            assert!(fills_width || fills_height);
            let old = layer(&before, id);
            let new = layer(e.project(), id);
            let v = |l: &Layer, p| l.property(p).expect("2D fixture property").value_at(0);
            close(
                v(old, Property::ScaleX) / v(old, Property::ScaleY),
                v(new, Property::ScaleX) / v(new, Property::ScaleY),
            );
            assert_eq!(
                old.property(Property::Rotation)
                    .expect("2D fixture property"),
                new.property(Property::Rotation)
                    .expect("2D fixture property")
            );
            assert_eq!(
                v(old, Property::ScaleX).is_sign_negative(),
                v(new, Property::ScaleX).is_sign_negative()
            );
            assert_eq!(
                v(old, Property::ScaleY).is_sign_negative(),
                v(new, Property::ScaleY).is_sign_negative()
            );
        }
    }
}

#[test]
fn anchor_centering_preserves_pixels_and_world_pose_for_selected_parent_and_child() {
    for (singular, ids) in [(false, vec![1, 2, 3]), (true, vec![1]), (true, vec![1, 2])] {
        let mut e = scene(true, true);
        if singular {
            value(&mut e, 2, Property::ScaleX, 0, 0.);
            e.clear_history();
        }
        let before = e.project().clone();
        verify_commit(
            &mut e,
            ids.clone(),
            &ids,
            0,
            LayerTransformOp::CenterAnchorInSourceBounds,
            &[0],
        );
        for id in [1, 2, 3] {
            for (a, b) in world(&before, id, 0)
                .into_iter()
                .zip(world(e.project(), id, 0))
            {
                close(a, b);
            }
        }
        for id in ids {
            let l = layer(e.project(), id);
            assert_eq!(
                l.property(Property::AnchorX)
                    .expect("2D fixture property")
                    .value_at(0),
                l.width() / 2.
            );
            assert_eq!(
                l.property(Property::AnchorY)
                    .expect("2D fixture property")
                    .value_at(0),
                l.height() / 2.
            );
        }
    }
}

fn animate(e: &mut Editor) {
    for (id, property, delta) in [
        (1, Property::PositionX, 30.),
        (1, Property::PositionY, -20.),
        (1, Property::AnchorX, 12.5),
        (1, Property::AnchorY, 16.25),
        (1, Property::ScaleX, 20.),
        (1, Property::ScaleY, -20.),
        (1, Property::Rotation, 27.),
        (1, Property::Opacity, -15.),
        (2, Property::Rotation, 12.),
        (2, Property::ScaleX, -12.),
    ] {
        let base = layer(e.project(), id)
            .property(property)
            .expect("2D fixture property")
            .value_at(0);
        e.execute(Command::ToggleAnimation {
            id,
            property,
            frame: 0,
        })
        .unwrap();
        value(e, id, property, 60, base + delta);
        value(e, id, property, 90, base + delta * 0.5);
        for frame in [0, 60] {
            e.execute(Command::SetInterpolation {
                id,
                property,
                frame,
                interpolation: Interpolation::Bezier(Bezier {
                    x1: 0.2,
                    y1: 0.1,
                    x2: 0.75,
                    y2: 0.9,
                }),
            })
            .unwrap();
        }
        for (incoming, slope, influence) in [
            (true, delta / 60. * 0.3, 0.28),
            (false, -delta / 30. * 0.4, 0.18),
        ] {
            e.execute(Command::SetTemporalHandle {
                id,
                property: property.into(),
                frame: 60,
                incoming,
                handle: TemporalHandle { slope, influence },
            })
            .unwrap();
        }
    }
    e.clear_history();
}

#[test]
fn animated_current_frame_edits_preserve_other_keys_handles_geometry_and_reference_neighbor_pixels()
{
    for op in OPS {
        for frame in [30, 60] {
            let mut e = scene(false, true);
            animate(&mut e);
            let before = e.project().clone();
            verify_commit(
                &mut e,
                vec![1],
                &[1],
                frame,
                op,
                &[0, 15, 30, 45, 60, 75, 90],
            );
            for property in Property::ALL {
                let old = layer(&before, 1)
                    .property(property)
                    .expect("2D fixture property");
                let new = layer(e.project(), 1)
                    .property(property)
                    .expect("2D fixture property");
                for (&at, key) in old.keys() {
                    let changed = new.keys().get(&at).unwrap();
                    assert_eq!(key.interpolation, changed.interpolation);
                    assert_eq!(key.temporal, changed.temporal);
                    if at != frame {
                        assert_eq!(key, changed);
                    }
                }
                assert!(
                    new.keys()
                        .keys()
                        .all(|at| old.keys().contains_key(at) || *at == frame)
                );
            }
            // Only the sampled pose is compensated for anchor centering. Neighbor
            // frames intentionally follow the independent edited-track reference.
        }
    }
}

fn assert_rejected(mut e: Editor, ids: Vec<LayerId>, op: LayerTransformOp) {
    e.clear_history();
    e.execute(Command::SetColor {
        id: 3,
        color: 0xf12266,
    })
    .unwrap();
    e.undo();
    let before = e.project().clone();
    let encoded = crate::project_io::encode_native_project(&before, None).unwrap();
    let r = Renderer::new();
    let opening = pixels(&r, &before, 0);
    assert!(
        e.execute(Command::TransformLayers {
            ids,
            frame: 0,
            operation: op
        })
        .is_err()
    );
    assert_eq!(e.project(), &before);
    assert_eq!(
        crate::project_io::encode_native_project(e.project(), None).unwrap(),
        encoded
    );
    assert!(!e.can_undo());
    assert!(e.can_redo());
    assert_eq!(pixels(&r, e.project(), 0), opening);
}

#[test]
fn rejected_selections_leave_rendered_document_and_redo_exactly_unchanged() {
    for op in OPS {
        let mut locked = scene(false, false);
        locked.execute(Command::ToggleLocked(1)).unwrap();
        // A locked selected descendant must be validated even if its parent
        // would otherwise carry it as a single selection root.
        assert_rejected(locked, vec![2, 1, 3], op);
        let mut audio = scene(false, false);
        audio
            .execute(Command::AddContent {
                content: Content::Audio {
                    path: "layer-transform-reference.wav".into(),
                    audio: AudioMetadata {
                        stream_index: 0,
                        sample_rate: 48000,
                        channels: 2,
                        channel_layout: "stereo".into(),
                        duration: 4.,
                        start_time: 0.,
                        file_offset: 0.,
                    },
                    start_frame: 0,
                    playback: VideoPlayback::default(),
                },
                width: 100.,
                height: 50.,
                name: "Unsupported audio".into(),
            })
            .unwrap();
        assert_rejected(audio, vec![1, 4], op);
    }
    for parent in [false, true] {
        let mut e = scene(false, false);
        value(&mut e, if parent { 2 } else { 1 }, Property::ScaleX, 0, 0.);
        assert_rejected(e, vec![3, 1], LayerTransformOp::FitInsideComposition);
    }
    for op in [
        LayerTransformOp::FitInsideComposition,
        LayerTransformOp::CenterAnchorInSourceBounds,
    ] {
        let mut e = scene(false, false);
        e.execute(Command::AddContent {
            content: Content::Null,
            width: 20.,
            height: 20.,
            name: "Unsupported bounds".into(),
        })
        .unwrap();
        assert_rejected(e, vec![1, 4], op);
    }
}

#[test]
fn exact_noops_and_repeated_fit_preserve_pixels_keys_and_redo_without_tolerance_drift() {
    for op in OPS {
        let mut e = scene(false, false);
        if op == LayerTransformOp::FitInsideComposition {
            e.execute(Command::TransformLayers {
                ids: vec![1, 3],
                frame: 0,
                operation: op,
            })
            .unwrap();
        } else {
            let edits = match op {
                LayerTransformOp::ResetScaleRotation => vec![
                    (Property::ScaleX, 100.),
                    (Property::ScaleY, 100.),
                    (Property::Rotation, 0.),
                ],
                LayerTransformOp::FlipHorizontal => vec![(Property::ScaleX, 0.)],
                LayerTransformOp::FlipVertical => vec![(Property::ScaleY, 0.)],
                LayerTransformOp::CenterAnchorInSourceBounds => {
                    vec![(Property::AnchorX, 80.), (Property::AnchorY, 50.)]
                }
                LayerTransformOp::FitInsideComposition => unreachable!(),
            };
            for (property, v) in edits {
                value(&mut e, 1, property, 0, v);
                e.execute(Command::ToggleAnimation {
                    id: 1,
                    property,
                    frame: 0,
                })
                .unwrap();
                value(&mut e, 1, property, 60, v);
            }
        }
        e.clear_history();
        e.execute(Command::SetColor {
            id: 3,
            color: 0xf12266,
        })
        .unwrap();
        e.undo();
        let before = e.project().clone();
        let r = Renderer::new();
        let opening = pixels(&r, &before, 30);
        for _ in 0..3 {
            e.execute(Command::TransformLayers {
                ids: if op == LayerTransformOp::FitInsideComposition {
                    vec![1, 3]
                } else {
                    vec![1]
                },
                frame: 30,
                operation: op,
            })
            .unwrap();
            assert_eq!(e.project(), &before, "{op:?} must remain an exact no-op");
            assert!(!e.can_undo());
            assert!(e.can_redo());
            assert_eq!(pixels(&r, e.project(), 30), opening);
        }
    }
}
