//! Independent E02 geometry/render acceptance. Expected documents replace only
//! literal base/pose geometry; they never invoke the production affine helper or
//! repeat EditPath per key. Headless codec tests are not native Save/Open evidence.
use crate::{
    editor::{EditorState, Tool},
    panels::vertex_editor::{Request, Session, TransformScope},
    rendering::Renderer,
};
use libre_effects_core::*;
use serde_json::{Value, json};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;
const SELECTED: [usize; 2] = [0, 2];
const FIELDS: [f64; 7] = [7.125, -3.5, 90., -150., 75., 41.25, 72.125];
const FRAMES: [Frame; 7] = [0, 8, 16, 32, 48, 64, 80];

fn curve(closed: bool) -> VectorPath {
    VectorPath {
        closed,
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

fn shifted(base: &VectorPath, dx: f64, dy: f64) -> VectorPath {
    let mut path = base.clone();
    for vertex in &mut path.vertices {
        vertex.position[0] += dx;
        vertex.position[1] += dy;
        vertex.incoming[0] *= 0.5;
        vertex.outgoing[1] *= 1.5;
    }
    path
}

fn animation(base: &VectorPath, dormant: bool) -> PathAnimation {
    // Descending, ascending and repeated references. Slots 3 and 4 are unused
    // duplicates and must retain their separate identities and order.
    let poses = vec![
        shifted(base, -8., 4.),
        shifted(base, 16., 8.),
        shifted(base, 0., -16.),
        shifted(base, 48., -20.),
        shifted(base, 48., -20.),
    ];
    let keys = if dormant {
        json!({})
    } else {
        json!({"0": {"value": 2., "interpolation": "Linear"},
               "32": {"value": 0., "interpolation": "Smooth"},
               "64": {"value": 1., "interpolation": {"Bezier": {"x1": 0.25, "y1": -0.5, "x2": 0.75, "y2": 1.5}}},
               "80": {"value": 1., "interpolation": "Hold"}})
    };
    serde_json::from_value(json!({"poses": poses, "timing": {"value": 1., "keys": keys}})).unwrap()
}

fn contents(s: &mut EditorState, edit: ContentsEdit) {
    s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
}

fn group_value(s: &mut EditorState, item: u64, parameter: ContentsParam, value: f64) {
    contents(
        s,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value { frame: 0, value },
        },
    );
}

fn scene(kind: u8, closed: bool, dormant: bool) -> (EditorState, PathTarget) {
    let mut s = EditorState::default();
    s.editor
        .execute(Command::ConfigureComposition {
            name: "Whole-pose independent render oracle".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 96,
        })
        .unwrap();
    let base = curve(closed);
    let content = if kind < 2 {
        Content::Shape(Shape {
            path: Some(base.clone()),
            path_animation: animation(&base, dormant),
            fill: closed,
            stroke_width: 3.,
            ..Default::default()
        })
    } else {
        Content::Solid
    };
    s.editor
        .execute(Command::AddContent {
            content,
            width: 200.,
            height: 160.,
            name: "Curved target".into(),
        })
        .unwrap();
    s.editor
        .execute(Command::SetColor {
            id: 1,
            color: 0x3988d8,
        })
        .unwrap();
    let target = match kind {
        0 => {
            s.editor
                .execute(Command::AddContent {
                    content: Content::Null,
                    width: 20.,
                    height: 20.,
                    name: "Reflected parent".into(),
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
            PathTarget::Shape
        }
        1 => {
            contents(&mut s, ContentsEdit::Promote);
            contents(
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
                .find(|(_, parent, n)| {
                    *parent == 0 && n.id != 1 && matches!(n.kind, ContentsKind::Group(_))
                })
                .unwrap()
                .2
                .id;
            contents(
                &mut s,
                ContentsEdit::Move {
                    item: 1,
                    parent: outer,
                    index: 0,
                },
            );
            for group in [1, outer] {
                for (p, value) in [
                    (Property::AnchorX, 100.),
                    (Property::AnchorY, 80.),
                    (Property::PositionX, 100.),
                    (Property::PositionY, 80.),
                ] {
                    group_value(&mut s, group, ContentsParam::Transform(p), value);
                }
            }
            for (group, p, value) in [
                (1, Property::Rotation, 18.),
                (1, Property::ScaleX, -105.),
                (1, Property::ScaleY, 90.),
                (outer, Property::Rotation, -12.),
                (outer, Property::ScaleX, 90.),
                (outer, Property::ScaleY, 110.),
            ] {
                group_value(&mut s, group, ContentsParam::Transform(p), value);
            }
            group_value(&mut s, 1, ContentsParam::Skew, 12.);
            group_value(&mut s, outer, ContentsParam::SkewAxis, 25.);
            PathTarget::Contents(2)
        }
        2 | 3 => {
            let right = if kind == 2 { 55. } else { 200. };
            s.editor
                .execute(Command::SetPathMasks {
                    id: 1,
                    masks: vec![
                        PathMask {
                            path: VectorPath {
                                closed: true,
                                vertices: [[0., 0.], [right, 0.], [right, 160.], [0., 160.]]
                                    .map(PathVertex::corner)
                                    .to_vec(),
                            },
                            ..Default::default()
                        },
                        PathMask {
                            path: base.clone(),
                            animation: animation(&base, dormant),
                            mode: if kind == 2 {
                                PathMaskMode::Add
                            } else {
                                PathMaskMode::Subtract
                            },
                            ..Default::default()
                        },
                    ],
                })
                .unwrap();
            PathTarget::Mask(
                s.editor
                    .project()
                    .composition()
                    .layer(1)
                    .unwrap()
                    .path_masks()[1]
                    .id,
            )
        }
        _ => unreachable!(),
    };
    s.editor.select(1);
    s.editor.clear_history();
    s.tool = Tool::Pen;
    (s, target)
}

fn contents_path(nodes: &mut Value, id: u64) -> Option<&mut Value> {
    for node in nodes.as_array_mut().unwrap() {
        if node["id"].as_u64() == Some(id) {
            return node["kind"].get_mut("Path");
        }
        if let Some(children) = node["kind"].get_mut("Group") {
            if let Some(path) = contents_path(children, id) {
                return Some(path);
            }
        }
    }
    None
}

fn target_json(project: &mut Value, target: PathTarget) -> &mut Value {
    let layer = project["composition"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|layer| layer["id"] == 1)
        .unwrap();
    match target {
        PathTarget::Shape => &mut layer["content"]["Shape"],
        PathTarget::Contents(id) => {
            contents_path(&mut layer["content"]["ShapeContents"]["items"], id).unwrap()
        }
        PathTarget::Mask(id) => layer["path_masks"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|mask| mask["id"] == id)
            .unwrap(),
    }
}

fn animation_key(target: PathTarget) -> &'static str {
    if target == PathTarget::Shape {
        "path_animation"
    } else {
        "animation"
    }
}

// R90 * S(-1.5, .75) has literal rows [0, -.75] and [-1.5, 0].
// Every source, coefficient and pivot is dyadic, so these bounded operations are exact.
fn quarter(path: &VectorPath) -> VectorPath {
    let mut result = path.clone();
    for i in SELECTED {
        let v = path.vertices[i];
        result.vertices[i] = PathVertex {
            position: [
                41.25 - 0.75 * (v.position[1] - 72.125) + 7.125,
                72.125 - 1.5 * (v.position[0] - 41.25) - 3.5,
            ],
            incoming: [-0.75 * v.incoming[1], -1.5 * v.incoming[0]],
            outgoing: [-0.75 * v.outgoing[1], -1.5 * v.outgoing[0]],
        };
    }
    result
}

fn reference(source: &Project, target: PathTarget) -> Project {
    let mut json = serde_json::to_value(source).unwrap();
    let target_value = target_json(&mut json, target);
    target_value["path"] = serde_json::to_value(quarter(
        &serde_json::from_value(target_value["path"].clone()).unwrap(),
    ))
    .unwrap();
    for pose in target_value[animation_key(target)]["poses"]
        .as_array_mut()
        .unwrap()
    {
        *pose =
            serde_json::to_value(quarter(&serde_json::from_value(pose.clone()).unwrap())).unwrap();
    }
    Project::from_json(&json.to_string()).unwrap()
}

fn evaluate(project: &Project, target: PathTarget, frame: Frame) -> (VectorPath, Affine) {
    let comp = project.composition();
    let layer = comp.layer(1).unwrap();
    let world = comp.world_transform(1, frame).unwrap();
    if let PathTarget::Contents(item) = target {
        let Content::ShapeContents(contents) = layer.content() else {
            panic!()
        };
        let (_, path, local) = contents
            .editable_paths(frame)
            .into_iter()
            .find(|(id, _, _)| *id == item)
            .unwrap();
        (path, world.compose(local))
    } else {
        let (base, animation) = layer.path_animation(target).unwrap();
        (animation.at(base, frame), world)
    }
}

fn open(s: &mut EditorState, target: PathTarget, frame: Frame) {
    s.frame = frame;
    let (path, world) = evaluate(s.editor.project(), target, frame);
    let request = Request::for_selection(s, 1, target, SELECTED.into(), path, world).unwrap();
    s.vertex_editor = Some(Session::new(s, request).unwrap());
}

fn switch(s: &mut EditorState, scope: TransformScope) {
    let serial = s.vertex_editor.as_ref().unwrap().id;
    assert!(s.switch_vertex_scope(serial, scope, None, false));
    assert_eq!(s.vertex_editor.as_ref().unwrap().scope(), scope);
}

fn fields(s: &mut EditorState, values: [f64; 7]) {
    for i in [5, 6, 3, 4, 2, 0, 1] {
        s.vertex_editor
            .as_mut()
            .unwrap()
            .input(i, &values[i].to_string())
            .unwrap();
    }
}

fn pixels(renderer: &Renderer, project: &Project, frame: Frame) -> image::RgbaImage {
    let image = renderer.render_preview(project, frame, WIDTH).unwrap();
    assert_eq!(
        image,
        renderer
            .render_output(project, frame, WIDTH, HEIGHT)
            .unwrap()
    );
    image
}

fn check_case(kind: u8, closed: bool, dormant: bool) {
    let (mut s, target) = scene(kind, closed, dormant);
    let source = s.editor.project().clone();
    let source_bytes = crate::project_io::encode_native_project(&source, None).unwrap();
    let expected = reference(&source, target);
    open(&mut s, target, 16);
    assert_eq!(
        s.vertex_editor.as_ref().unwrap().scope(),
        TransformScope::ThisFrame
    );
    switch(&mut s, TransformScope::AllPoses);
    assert_eq!(s.vertex_editor.as_ref().unwrap().project(), &source);
    fields(&mut s, FIELDS);
    let session = s.vertex_editor.as_ref().unwrap();
    assert_eq!(
        session.project(),
        &expected,
        "all unrelated geometry, timing, slots, transforms, paint and metadata stay exact"
    );
    assert_eq!(
        session.path(),
        &evaluate(&expected, target, 16).0,
        "overlay uses evaluated transformed draft"
    );
    assert_eq!(s.editor.project(), &source);
    assert_eq!(
        crate::project_io::encode_native_project(s.editor.project(), None).unwrap(),
        source_bytes
    );
    assert!(!s.editor.can_undo());
    let renderer = Renderer::new();
    let before_pixels = pixels(&renderer, &source, 16);
    let draft_pixels = pixels(&renderer, session.project(), 16);
    assert!(
        before_pixels.pixels().filter(|p| p[3] > 0).count() > 100,
        "fixture must paint"
    );
    assert_ne!(draft_pixels, before_pixels, "fixture must visibly change");
    for frame in FRAMES {
        assert_eq!(
            pixels(&renderer, session.project(), frame),
            pixels(&renderer, &expected, frame)
        );
    }
    let discarded = s.vertex_editor.take().unwrap();
    drop(discarded);
    assert_eq!(
        pixels(&renderer, s.editor.project(), 16),
        before_pixels,
        "discarding isolated draft leaves source pixels unchanged"
    );
    open(&mut s, target, 16);
    switch(&mut s, TransformScope::AllPoses);
    fields(&mut s, FIELDS);
    s.accept_vertex_editor();
    assert!(s.vertex_editor.is_none());
    assert_eq!(s.editor.project(), &expected);
    assert_eq!(s.vertex_return.as_ref().unwrap().indices, SELECTED.into());
    assert_eq!(pixels(&renderer, s.editor.project(), 16), draft_pixels);
    let encoded = crate::project_io::encode_native_project(s.editor.project(), None).unwrap();
    let reopened = crate::project_io::decode_project(&encoded).unwrap().project;
    assert_eq!(reopened, expected);
    for frame in FRAMES {
        assert_eq!(
            pixels(&renderer, &reopened, frame),
            pixels(&renderer, &expected, frame)
        );
    }
    s.editor.undo();
    assert_eq!(s.editor.project(), &source);
    assert_eq!(
        crate::project_io::encode_native_project(s.editor.project(), None).unwrap(),
        source_bytes
    );
    assert!(
        !s.editor.can_undo(),
        "one accepted transform is one history entry"
    );
    s.editor.redo();
    assert_eq!(s.editor.project(), &expected);
    assert_eq!(pixels(&renderer, s.editor.project(), 16), draft_pixels);
}

#[test]
fn whole_pose_parented_shape_and_nested_reflected_contents_match_exact_independent_rasters() {
    for kind in [0, 1] {
        for closed in [false, true] {
            check_case(kind, closed, false);
        }
    }
}

#[test]
fn whole_pose_add_and_subtract_masks_match_exact_independent_rasters() {
    for kind in [2, 3] {
        check_case(kind, true, false);
    }
}

#[test]
fn whole_pose_dormant_tracks_and_unused_duplicate_slots_survive_render_and_lep_roundtrip() {
    for kind in [0, 1, 2, 3] {
        check_case(kind, true, true);
    }
}

#[test]
fn whole_pose_scope_switch_distinguishes_existing_storage_from_current_frame_key_edit() {
    let (mut s, target) = scene(1, true, false);
    let source = s.editor.project().clone();
    let expected_all = reference(&source, target);
    let mut frame_only = Editor::default();
    frame_only.replace_project(source.clone()).unwrap();
    frame_only
        .execute(Command::EditPath {
            id: 1,
            target,
            frame: 16,
            path: quarter(&evaluate(&source, target, 16).0),
        })
        .unwrap();
    open(&mut s, target, 16);
    fields(&mut s, FIELDS);
    assert_eq!(
        s.vertex_editor.as_ref().unwrap().project(),
        frame_only.project()
    );
    assert_ne!(frame_only.project(), &expected_all);
    let renderer = Renderer::new();
    assert_eq!(
        pixels(&renderer, frame_only.project(), 16),
        pixels(&renderer, &expected_all, 16),
        "dyadic midpoint commutes exactly"
    );
    assert_ne!(
        pixels(&renderer, frame_only.project(), 0),
        pixels(&renderer, &expected_all, 0)
    );
    switch(&mut s, TransformScope::AllPoses);
    assert_eq!(s.vertex_editor.as_ref().unwrap().project(), &expected_all);
    switch(&mut s, TransformScope::ThisFrame);
    assert_eq!(
        s.vertex_editor.as_ref().unwrap().project(),
        frame_only.project()
    );
    assert_eq!(s.editor.project(), &source);
    assert!(!s.editor.can_undo());
    switch(&mut s, TransformScope::AllPoses);
    assert_eq!(
        s.vertex_editor.as_ref().unwrap().project(),
        &expected_all,
        "switches rebuild from frozen source without compounding"
    );
}

#[test]
fn whole_pose_visible_fixed_point_does_not_hide_a_changed_base_or_unused_pose() {
    let (mut s, target) = scene(1, true, false);
    let mut doc = serde_json::to_value(s.editor.project()).unwrap();
    let current = &mut target_json(&mut doc, target)[animation_key(target)]["poses"][2];
    let mut fixed: VectorPath = serde_json::from_value(current.clone()).unwrap();
    for index in SELECTED {
        fixed.vertices[index] = PathVertex::corner([64., 64.]);
    }
    *current = serde_json::to_value(fixed).unwrap();
    s.editor
        .replace_project(Project::from_json(&doc.to_string()).unwrap())
        .unwrap();
    s.editor.select(1);
    s.editor.clear_history();
    let source = s.editor.project().clone();
    open(&mut s, target, 0);
    switch(&mut s, TransformScope::AllPoses);
    fields(&mut s, [0., 0., 90., 100., 100., 64., 64.]);
    let session = s.vertex_editor.as_ref().unwrap();
    assert_eq!(session.path(), &evaluate(&source, target, 0).0);
    assert_ne!(session.project(), &source);
    assert!(session.command().unwrap().is_some());
    let renderer = Renderer::new();
    assert_eq!(
        pixels(&renderer, session.project(), 0),
        pixels(&renderer, &source, 0)
    );
    assert_ne!(
        pixels(&renderer, session.project(), 32),
        pixels(&renderer, &source, 32)
    );
    let draft = session.project().clone();
    s.accept_vertex_editor();
    assert_eq!(s.editor.project(), &draft);
    s.editor.undo();
    assert_eq!(s.editor.project(), &source);
    s.editor.redo();
    assert_eq!(s.editor.project(), &draft);
}

#[test]
fn whole_pose_arbitrary_angle_commutation_has_a_scale_derived_geometry_error_bound() {
    let (s, target) = scene(0, true, false);
    let mut source_json = serde_json::to_value(s.editor.project()).unwrap();
    target_json(&mut source_json, target)[animation_key(target)]["timing"]["keys"]["0"]["interpolation"] =
        json!({"Bezier": {"x1": 0.25, "y1": -0.5, "x2": 0.75, "y2": 1.5}});
    let source = Project::from_json(&source_json.to_string()).unwrap();
    let spec = PathTransformSpec::from([7.125, -3.5, 33.25, -150., 75., 41.25, 72.125]);
    let mut edited = Editor::default();
    edited.replace_project(source.clone()).unwrap();
    edited
        .execute(Command::TransformPathPoses {
            id: 1,
            target,
            indices: SELECTED.into(),
            transform: spec,
        })
        .unwrap();
    let (sin, cos) = 33.25_f64.to_radians().sin_cos();
    let matrix = [[-1.5 * cos, -0.75 * sin], [-1.5 * sin, 0.75 * cos]];
    let norm = matrix
        .into_iter()
        .map(|r| r[0].abs() + r[1].abs())
        .fold(0., f64::max);
    let (_, animation) = source
        .composition()
        .layer(1)
        .unwrap()
        .path_animation(target)
        .unwrap();
    let serialized = serde_json::to_value(animation).unwrap();
    let poses: Vec<VectorPath> = serde_json::from_value(serialized["poses"].clone()).unwrap();
    let magnitude = poses
        .iter()
        .flat_map(|p| &p.vertices)
        .flat_map(|v| v.position.into_iter().chain(v.incoming).chain(v.outgoing))
        .map(f64::abs)
        .fold(0., f64::max);
    let track = source
        .composition()
        .layer(1)
        .unwrap()
        .track(PropertyPath::Path(target))
        .unwrap();
    let mut saw_overshoot = false;
    for frame in [1, 7, 16, 23, 31, 33, 41, 63] {
        let (left_frame, left) = track.keys().range(..=frame).next_back().unwrap();
        let (right_frame, right) = track
            .keys()
            .range((std::ops::Bound::Excluded(frame), std::ops::Bound::Unbounded))
            .next()
            .unwrap();
        assert!(left_frame < right_frame && left.value != right.value);
        let t = (track.value_at(frame) - left.value) / (right.value - left.value);
        let weight = (1. - t).abs() + t.abs();
        saw_overshoot |= !(0. ..=1.).contains(&t);
        // Freeze the rounded sine/cosine coefficients in both expressions. No
        // trigonometric accuracy assumption is needed to test their commutation.
        // Each coordinate uses fewer than 32 rounded arithmetic operations per
        // arm (blend then affine, or affine then blend). gamma_64 safely covers
        // their expansion plus the separately rounded independent matrix. The
        // factor 2 sums the two arms' absolute-error bounds. Normal finite inputs
        // here exclude overflow/underflow; cancellation is covered by absolute
        // magnitudes. This is a local-coordinate bound, never a pixel tolerance.
        let u = f64::EPSILON / 2.;
        let gamma = 64. * u / (1. - 64. * u);
        let bound = 2. * gamma * weight.max(1.) * (norm * (magnitude + 72.125) + 72.125 + 7.125);
        assert!(
            bound < 1e-10,
            "fixture must retain a meaningful tight bound"
        );
        let opening = evaluate(&source, target, frame).0;
        let actual = evaluate(edited.project(), target, frame).0;
        for index in 0..opening.vertices.len() {
            if !SELECTED.contains(&index) {
                assert_eq!(actual.vertices[index], opening.vertices[index]);
                continue;
            }
            let a = opening.vertices[index];
            let b = actual.vertices[index];
            for axis in 0..2 {
                let row = matrix[axis];
                let expected = spec.pivot[axis]
                    + row[0] * (a.position[0] - spec.pivot[0])
                    + row[1] * (a.position[1] - spec.pivot[1])
                    + spec.translation[axis];
                assert!(
                    (b.position[axis] - expected).abs() <= bound,
                    "frame={frame} index={index} axis={axis}"
                );
                for (before, after) in [(a.incoming, b.incoming), (a.outgoing, b.outgoing)] {
                    let expected = row[0] * before[0] + row[1] * before[1];
                    assert!((after[axis] - expected).abs() <= bound);
                }
            }
        }
    }
    assert!(
        saw_overshoot,
        "the bound must cover an actual extrapolated sample"
    );
}
