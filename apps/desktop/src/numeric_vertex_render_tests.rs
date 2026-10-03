//! Numeric draft pixels must match independently edited local geometry.
use crate::{
    editor::{EditorState, Tool},
    panels::vertex_editor::{Request, Session},
    rendering::Renderer,
};
use libre_effects_core::*;

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;

fn curved() -> VectorPath {
    VectorPath {
        closed: true,
        vertices: vec![
            PathVertex {
                position: [25.125, 25.0625],
                incoming: [-13.25, 16.125],
                outgoing: [21.5, -12.25],
            },
            PathVertex {
                position: [150., 30.],
                incoming: [-19., -8.],
                outgoing: [17., 24.],
            },
            PathVertex {
                position: [155., 125.],
                incoming: [5., -16.],
                outgoing: [-27., 17.],
            },
            PathVertex {
                position: [30., 135.],
                incoming: [20., 8.],
                outgoing: [-12., -22.],
            },
        ],
    }
}
fn rectangle(right: f64) -> VectorPath {
    VectorPath {
        closed: true,
        vertices: [[0., 0.], [right, 0.], [right, 160.], [0., 160.]]
            .map(PathVertex::corner)
            .to_vec(),
    }
}
fn scene(content: Content) -> EditorState {
    let mut s = EditorState::default();
    s.editor
        .execute(Command::ConfigureComposition {
            name: "Numeric vertex pixels".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 91,
        })
        .unwrap();
    s.editor
        .execute(Command::AddContent {
            content,
            width: 200.,
            height: 160.,
            name: "Path source".into(),
        })
        .unwrap();
    s.editor
        .execute(Command::SetColor {
            id: 1,
            color: 0x3988d8,
        })
        .unwrap();
    s.tool = Tool::Pen;
    s
}
fn contents_edit(s: &mut EditorState, edit: ContentsEdit) {
    s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn contents_value(s: &mut EditorState, item: u64, parameter: ContentsParam, value: f64) {
    contents_edit(
        s,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value { frame: 0, value },
        },
    );
}
fn shape_scene(contents: bool) -> (EditorState, PathTarget) {
    let mut s = scene(Content::Shape(Shape {
        path: Some(curved()),
        stroke_width: 3.,
        ..Default::default()
    }));
    let target = if contents {
        contents_edit(&mut s, ContentsEdit::Promote);
        contents_edit(
            &mut s,
            ContentsEdit::Add {
                parent: 0,
                kind: ContentsKind::Group(vec![]),
            },
        );
        let Content::ShapeContents(c) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        let outer = c
            .rows()
            .into_iter()
            .find(|(_, parent, node)| {
                *parent == 0 && node.id != 1 && matches!(node.kind, ContentsKind::Group(_))
            })
            .unwrap()
            .2
            .id;
        contents_edit(
            &mut s,
            ContentsEdit::Move {
                item: 1,
                parent: outer,
                index: 0,
            },
        );
        for group in [1, outer] {
            for (p, v) in [
                (Property::AnchorX, 100.),
                (Property::AnchorY, 80.),
                (Property::PositionX, 100.),
                (Property::PositionY, 80.),
            ] {
                contents_value(&mut s, group, ContentsParam::Transform(p), v);
            }
        }
        for (group, p, v) in [
            (1, Property::Rotation, 18.),
            (1, Property::ScaleX, -105.),
            (1, Property::ScaleY, 90.),
            (outer, Property::Rotation, -12.),
            (outer, Property::ScaleX, 90.),
            (outer, Property::ScaleY, 110.),
        ] {
            contents_value(&mut s, group, ContentsParam::Transform(p), v);
        }
        contents_value(&mut s, 1, ContentsParam::Skew, 12.);
        contents_value(&mut s, outer, ContentsParam::SkewAxis, 25.);
        PathTarget::Contents(2)
    } else {
        s.editor
            .execute(Command::AddContent {
                content: Content::Null,
                width: 20.,
                height: 20.,
                name: "Parent".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::SetParent {
                id: 1,
                parent: Some(2),
                frame: 0,
            })
            .unwrap();
        for (property, value) in [
            (Property::Rotation, 17.),
            (Property::ScaleX, -90.),
            (Property::ScaleY, 110.),
        ] {
            s.editor
                .execute(Command::SetValue {
                    id: 2,
                    property,
                    frame: 0,
                    value,
                })
                .unwrap();
        }
        s.editor.select(1);
        PathTarget::Shape
    };
    s.editor.clear_history();
    (s, target)
}
fn mask_scene(mode: PathMaskMode) -> (EditorState, PathTarget) {
    let mut s = scene(Content::Solid);
    s.editor
        .execute(Command::SetPathMasks {
            id: 1,
            masks: vec![
                PathMask {
                    path: rectangle(if mode == PathMaskMode::Add { 55. } else { 200. }),
                    ..Default::default()
                },
                PathMask {
                    path: curved(),
                    mode,
                    ..Default::default()
                },
            ],
        })
        .unwrap();
    let id = s.editor.selected_layer().unwrap().path_masks()[1].id;
    s.editor.clear_history();
    (s, PathTarget::Mask(id))
}
fn evaluated(s: &EditorState, target: PathTarget) -> (VectorPath, Affine) {
    let layer = s.editor.project().composition().layer(1).unwrap();
    let world = s
        .editor
        .project()
        .composition()
        .world_transform(1, s.frame)
        .unwrap();
    if let PathTarget::Contents(item) = target {
        let Content::ShapeContents(c) = layer.content() else {
            panic!()
        };
        let (_, path, local) = c
            .editable_paths(s.frame)
            .into_iter()
            .find(|(id, _, _)| *id == item)
            .unwrap();
        (path, world.compose(local))
    } else {
        let (base, animation) = layer.path_animation(target).unwrap();
        (animation.at(base, s.frame), world)
    }
}
fn animate(s: &mut EditorState, target: PathTarget) {
    let mut last = evaluated(s, target).0;
    for v in &mut last.vertices {
        v.position[0] += 8.;
        v.position[1] += 9.;
        v.incoming[0] *= 0.8;
        v.outgoing[1] *= 1.2;
    }
    s.editor
        .execute(Command::AnimatePath {
            id: 1,
            target,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target,
            frame: 60,
            path: last,
        })
        .unwrap();
    s.editor
        .execute(Command::AnimatePath {
            id: 1,
            target,
            edit: TrackEdit::Interpolate {
                frame: 60,
                interpolation: Interpolation::Bezier(Bezier {
                    x1: 0.2,
                    y1: 0.6,
                    x2: 0.8,
                    y2: 0.4,
                }),
            },
        })
        .unwrap();
    s.editor.clear_history();
}
fn pixels(r: &Renderer, p: &Project, f: u32) -> image::RgbaImage {
    let image = r.render_preview(p, f, WIDTH).unwrap();
    assert_eq!(image, r.render_output(p, f, WIDTH, HEIGHT).unwrap());
    image
}
fn edited_preview_case(mut s: EditorState, target: PathTarget, frame: u32) {
    s.frame = frame;
    let before = s.editor.project().clone();
    let before_native = crate::project_io::encode_native_project(&before, None).unwrap();
    let (opening, world) = evaluated(&s, target);
    let request = Request::new(&s, 1, target, 0, opening.clone(), world).unwrap();
    let mut session = Session::new(&s, request).unwrap();
    let mut expected_path = opening.clone();
    // Values are constructed independently of the field-input implementation.
    expected_path.vertices[0] = PathVertex {
        position: [
            opening.vertices[0].position[0] + 21.125,
            opening.vertices[0].position[1] - 8.0625,
        ],
        incoming: [-22.5, 8.125],
        outgoing: [30.875, -20.0625],
    };
    for (i, value) in expected_path.vertices[0]
        .position
        .into_iter()
        .chain(expected_path.vertices[0].incoming)
        .chain(expected_path.vertices[0].outgoing)
        .enumerate()
    {
        session.input(i, &value.to_string()).unwrap();
    }
    let mut expected = Editor::default();
    expected.replace_project(before.clone()).unwrap();
    expected
        .execute(Command::EditPath {
            id: 1,
            target,
            frame,
            path: expected_path,
        })
        .unwrap();
    assert_eq!(session.project(), expected.project());
    assert_eq!(s.editor.project(), &before);
    assert_eq!(
        crate::project_io::encode_native_project(s.editor.project(), None).unwrap(),
        before_native
    );
    assert!(!s.editor.can_undo());
    let renderer = Renderer::new();
    let baseline = pixels(&renderer, &before, frame);
    let preview = pixels(&renderer, session.project(), frame);
    assert!(baseline.pixels().filter(|p| p[3] > 0).count() > 100);
    assert_ne!(preview, baseline, "fixture must exercise visible geometry");
    assert_eq!(preview, pixels(&renderer, expected.project(), frame));
    let command = session.command().unwrap().unwrap();
    drop(session); // Cancellation still renders the unchanged source.
    assert_eq!(pixels(&renderer, s.editor.project(), frame), baseline);
    s.editor.execute(command).unwrap();
    let committed = s.editor.project().clone();
    assert_eq!(&committed, expected.project());
    assert_eq!(pixels(&renderer, &committed, frame), preview);
    let saved = crate::project_io::encode_native_project(&committed, None).unwrap();
    let reopened = crate::project_io::decode_project(&saved).unwrap().project;
    assert_eq!(reopened, committed);
    for f in [0, 30, 60] {
        assert_eq!(
            pixels(&renderer, &reopened, f),
            pixels(&renderer, expected.project(), f)
        );
    }
    s.editor.undo();
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
    s.editor.redo();
    assert_eq!(s.editor.project(), &committed);
}
#[test]
fn numeric_vertex_static_parented_shape_and_nested_reflected_skewed_contents_match_reference() {
    for contents in [false, true] {
        let (s, target) = shape_scene(contents);
        edited_preview_case(s, target, 0);
    }
}
#[test]
fn numeric_vertex_add_and_subtract_mask_drafts_match_independent_geometry() {
    for mode in [PathMaskMode::Add, PathMaskMode::Subtract] {
        let (s, target) = mask_scene(mode);
        edited_preview_case(s, target, 0);
    }
}
#[test]
fn numeric_vertex_between_keys_and_existing_eased_keys_match_reference_after_native_roundtrip() {
    for case in 0..3 {
        for frame in [30, 60] {
            let (mut s, target) = match case {
                0 => shape_scene(false),
                1 => shape_scene(true),
                _ => mask_scene(PathMaskMode::Subtract),
            };
            animate(&mut s, target);
            edited_preview_case(s, target, frame);
        }
    }
}
#[test]
fn numeric_vertex_return_to_opening_pose_is_exact_source_and_pixels_without_a_midframe_key() {
    let (mut s, target) = shape_scene(true);
    animate(&mut s, target);
    s.frame = 30;
    let before = s.editor.project().clone();
    let (path, world) = evaluated(&s, target);
    let request = Request::new(&s, 1, target, 0, path.clone(), world).unwrap();
    let mut session = Session::new(&s, request).unwrap();
    session.input(0, "99.123456789").unwrap();
    session
        .input(0, &path.vertices[0].position[0].to_string())
        .unwrap();
    assert!(session.command().unwrap().is_none());
    assert_eq!(session.project(), &before);
    assert_eq!(
        pixels(&Renderer::new(), session.project(), 30),
        pixels(&Renderer::new(), &before, 30)
    );
    assert!(
        !s.editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::Path(target))
            .unwrap()
            .keys()
            .contains_key(&30)
    );
}
