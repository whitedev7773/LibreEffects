//! Independent E02 cross-path source, history, render and native-save oracles.
//! Expectations never call the production path/world transform helpers. Cardinal
//! fixtures use literal algebra; the oblique fixture uses separate 3x3 matrices.
//! Generated files are fixtures, never evidence of native Save/Open interaction.
use crate::{
    editor::{Action, EditorState, Tool},
    rendering::Renderer,
    view_state::{GraphChannel, GraphRanges, ProjectViews},
};
use libre_effects_core::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 240;
const FRAMES: [Frame; 9] = [0, 15, 29, 30, 31, 45, 59, 60, 89];
const EDIT_FRAME: Frame = 30;

fn curve(half_width: f64, half_height: f64) -> VectorPath {
    VectorPath {
        closed: true,
        vertices: [
            ([-half_width, -half_height], [0., 9.], [12., 0.]),
            ([half_width, -half_height], [-10., 0.], [0., 11.]),
            ([half_width, half_height], [0., -8.], [-13., 0.]),
            ([-half_width, half_height], [14., 0.], [0., -10.]),
        ]
        .map(|(position, incoming, outgoing)| PathVertex {
            position,
            incoming,
            outgoing,
        })
        .to_vec(),
    }
}
fn shifted(path: &VectorPath, x: f64, y: f64) -> VectorPath {
    let mut result = path.clone();
    for vertex in &mut result.vertices {
        vertex.position[0] += x;
        vertex.position[1] += y;
    }
    result
}
fn node(project: &Project, id: u64) -> ContentsNode {
    let Content::ShapeContents(contents) = project.composition().layer(1).unwrap().content() else {
        panic!()
    };
    contents.node(id).unwrap().clone()
}
fn scalar(node: &mut ContentsNode, parameter: ContentsParam, value: f64) {
    assert!(node.parameters.contains_key(&parameter));
    node.parameters.insert(
        parameter,
        serde_json::from_value(json!({"value":value,"keys":{}})).unwrap(),
    );
}
fn layer_wire(wire: &mut Value, id: u64) -> &mut Value {
    wire["composition"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|layer| layer["id"] == id)
        .unwrap()
}
fn tree(project: &Project, items: Vec<ContentsNode>) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    layer_wire(&mut wire, 1)["content"]["ShapeContents"] = json!({"items":items, "next_id":14});
    // Retain a deliberately higher, valid existing schema. This command must not
    // migrate unrelated source simply because the selection changed.
    wire["version"] = json!(54);
    Project::from_json(&wire.to_string()).unwrap()
}
fn fixture() -> Project {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Cross-path independent acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(Default::default()),
            width: WIDTH as f64,
            height: HEIGHT as f64,
            name: "Two paths, one layer".into(),
        })
        .unwrap();
    for kind in [
        ContentsKind::Group(vec![]),
        ContentsKind::Path {
            path: curve(40., 30.),
            animation: Default::default(),
        },
        ContentsKind::Fill { even_odd: false },
        ContentsKind::Group(vec![]),
        ContentsKind::Path {
            path: curve(35., 25.),
            animation: Default::default(),
        },
        ContentsKind::Fill { even_odd: false },
        ContentsKind::Group(vec![]),
        ContentsKind::Path {
            path: curve(12., 9.),
            animation: Default::default(),
        },
        ContentsKind::Fill { even_odd: true },
        ContentsKind::Group(vec![]),
        ContentsKind::Path {
            path: curve(15., 10.),
            animation: Default::default(),
        },
        ContentsKind::Fill { even_odd: false },
        ContentsKind::Group(vec![]),
    ] {
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add { parent: 0, kind },
            })
            .unwrap();
    }
    let project = editor.project();
    let mut roots = Vec::new();
    for (group, path, fill, position, color, name) in [
        (1, 2, 3, [90., 100.], [230., 110., 45.], "Left copper path"),
        (4, 5, 6, [280., 100.], [40., 150., 235.], "Right blue path"),
        (
            7,
            8,
            9,
            [190., 200.],
            [225., 60., 190.],
            "Disabled preserved group",
        ),
        (
            10,
            11,
            12,
            [350., 205.],
            [80., 200., 95.],
            "Unselected green path",
        ),
    ] {
        let mut parent = node(project, group);
        parent.name = name.into();
        parent.enabled = group != 7;
        scalar(
            &mut parent,
            ContentsParam::Transform(Property::PositionX),
            position[0],
        );
        scalar(
            &mut parent,
            ContentsParam::Transform(Property::PositionY),
            position[1],
        );
        if group == 7 {
            // Scalar temporal tangents are supported; opaque Path timing keys
            // support easing but deliberately prohibit scalar slope handles.
            parent.parameters.insert(ContentsParam::Transform(Property::PositionX), serde_json::from_value(json!({
                "value": 190., "keys": {
                    "0": {"value": 180., "interpolation": "Linear"},
                    "30": {"value": 190., "interpolation": {"Bezier": {"x1": 0.2, "y1": -0.1, "x2": 0.8, "y2": 1.2}}, "temporal": {"incoming": {"slope": 0.5, "influence": 0.3}, "outgoing": {"slope": -0.25, "influence": 0.4}}},
                    "89": {"value": 210., "interpolation": "Smooth"}
                }
            })).unwrap());
        }
        let mut paint = node(project, fill);
        for (parameter, value) in [
            ShapeParam::FillRed,
            ShapeParam::FillGreen,
            ShapeParam::FillBlue,
        ]
        .into_iter()
        .zip(color)
        {
            scalar(&mut paint, ContentsParam::Shape(parameter), value);
        }
        parent.kind = ContentsKind::Group(vec![node(project, path), paint]);
        roots.push(parent);
    }
    let mut outer = node(project, 13);
    outer.name = "Nested right parent".into();
    outer.kind = ContentsKind::Group(vec![roots.remove(1)]);
    roots.insert(1, outer);
    tree(project, roots)
}
fn edit_node(project: &Project, id: u64, f: impl FnOnce(&mut ContentsNode)) -> Project {
    fn find(nodes: &mut [ContentsNode], id: u64) -> Option<&mut ContentsNode> {
        for node in nodes {
            if node.id == id {
                return Some(node);
            }
            if let ContentsKind::Group(children) = &mut node.kind {
                if let Some(found) = find(children, id) {
                    return Some(found);
                }
            }
        }
        None
    }
    let Content::ShapeContents(contents) = project.composition().layer(1).unwrap().content() else {
        panic!()
    };
    let mut wire = serde_json::to_value(contents).unwrap();
    let mut roots: Vec<ContentsNode> = serde_json::from_value(wire["items"].take()).unwrap();
    f(find(&mut roots, id).unwrap());
    tree(project, roots)
}
fn selections(all: bool) -> BTreeMap<u64, BTreeSet<usize>> {
    if all {
        [(2, [0, 1, 2, 3].into()), (5, [0, 1, 2, 3].into())].into()
    } else {
        [(2, [0, 2].into()), (5, [1, 3].into())].into()
    }
}
fn command(transform: PathTransformSpec, all: bool) -> Command {
    Command::TransformContentsPoints {
        id: 1,
        frame: EDIT_FRAME,
        selections: selections(all),
        transform,
    }
}
fn exact(actual: &Project, expected: &Project, context: &str) {
    assert_eq!(actual, expected, "{context}");
    assert_eq!(
        serde_json::to_vec(actual).unwrap(),
        serde_json::to_vec(expected).unwrap(),
        "{context}: complete serialized source"
    );
}
fn channel() -> GraphChannel {
    GraphChannel {
        id: 1,
        property: PropertyPath::Contents {
            item: 1,
            parameter: ContentsParam::Transform(Property::PositionX),
        },
    }
}
fn state(project: &Project, native: bool) -> EditorState {
    let mut state = EditorState::default();
    state.editor.replace_project(project.clone()).unwrap();
    state.editor.select(1);
    state.editor.clear_history();
    state.frame = EDIT_FRAME;
    state.selected_layers.insert(1);
    state.tool = Tool::Pen;
    if native {
        state.preview_zoom = Some(1.0);
        state.preview_pan = [0., 0.];
    } else {
        state.timeline_zoom = 2.;
        state.preview_zoom = Some(1.5);
        state.preview_pan = [13., -7.];
        state.graph_channels.pin(channel()).unwrap();
        state.graph_channels.ranges.insert(
            channel(),
            GraphRanges {
                value: Some([-100., 400.]),
                speed: Some([-20., 20.]),
            },
        );
        state.graph_channels.activate(channel());
    }
    state.normalize();
    state
}
fn codec(state: &mut EditorState, expected: &Project) -> ProjectViews {
    let views = state.capture_views();
    let view = views.encode_native(expected).unwrap();
    let bytes = project_file::encode(state.editor.project(), Some(&view)).unwrap();
    let decoded = project_file::decode(&bytes).unwrap();
    exact(&decoded.project, expected, "official LEP full source");
    exact(
        &Project::from_json(&expected.to_json().unwrap()).unwrap(),
        expected,
        "official JSON full source",
    );
    assert_eq!(decoded.view, Some(view.as_slice()));
    let loaded = ProjectViews::read_native(decoded.view.unwrap(), &decoded.project).unwrap();
    assert_eq!(loaded, views);
    let mut reopened = EditorState::default();
    reopened.editor.replace_project(decoded.project).unwrap();
    reopened.load_views(loaded);
    assert_eq!(
        reopened.capture_views().encode_native(expected).unwrap(),
        view
    );
    views
}
fn renders(actual: &Project, expected: &Project, before: &Project, changes: bool) {
    let renderer = Renderer::new();
    let decoded = project_file::decode(&project_file::encode(actual, None).unwrap())
        .unwrap()
        .project;
    let mut changed = false;
    let mut visible = false;
    for frame in FRAMES {
        let output = renderer
            .render_output(actual, frame, WIDTH, HEIGHT)
            .unwrap();
        visible |= output
            .chunks_exact(4)
            .any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2]);
        assert_eq!(
            output,
            renderer.render(actual, frame, WIDTH).unwrap(),
            "preview/output frame{frame}"
        );
        assert_eq!(
            output,
            renderer
                .render_output(expected, frame, WIDTH, HEIGHT)
                .unwrap(),
            "independent reference frame{frame}"
        );
        assert_eq!(
            output,
            renderer
                .render_output(&decoded, frame, WIDTH, HEIGHT)
                .unwrap(),
            "LEP frame{frame}"
        );
        changed |= output
            != renderer
                .render_output(before, frame, WIDTH, HEIGHT)
                .unwrap();
    }
    assert!(visible, "fixture must render meaningful colored pixels");
    assert_eq!(changed, changes, "visual-change contract");
}
fn export(label: &str, project: &Project, views: &ProjectViews) {
    let Some(directory) = std::env::var_os("LIBREEFFECTS_EXPORT_CROSS_PATH_FIXTURES") else {
        return;
    };
    let root = Path::new(&directory);
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let view = views.encode_native(project).unwrap();
    for (suffix, bytes) in [
        ("lfe.json", project.to_json().unwrap().into_bytes()),
        (
            "generated.lep",
            project_file::encode(project, Some(&view)).unwrap(),
        ),
    ] {
        use std::io::Write;
        let path = root.join(format!("{label}.{suffix}"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => file.write_all(&bytes).unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => assert_eq!(
                std::fs::read(&path).unwrap(),
                bytes,
                "immutable fixture {}",
                path.display()
            ),
            Err(error) => panic!("cannot export {}: {error}", path.display()),
        }
    }
}

#[derive(Clone, Copy)]
enum LiteralEdit {
    Move,
    Scale,
    Rotate,
    Collapse,
}
fn spec(edit: LiteralEdit) -> PathTransformSpec {
    match edit {
        LiteralEdit::Move => [20., 10., 0., 100., 100., 182.5, 100.].into(),
        LiteralEdit::Scale => [0., 0., 0., -50., 150., 182.5, 100.].into(),
        LiteralEdit::Rotate => [0., 0., 90., 100., 100., 182.5, 100.].into(),
        LiteralEdit::Collapse => [0., 0., 0., 0., 0., 182.5, 100.].into(),
    }
}
/// Independently expanded cardinal algebra. No production transform, inverse,
/// sampling or command is used to construct the expected path.
fn literal_path(
    source: &VectorPath,
    id: u64,
    selected: &BTreeSet<usize>,
    edit: LiteralEdit,
) -> VectorPath {
    let center_x = match id {
        2 => 90.,
        5 => 280.,
        _ => panic!(),
    };
    let mut path = source.clone();
    for &index in selected {
        let source = source.vertices[index];
        let [x, y] = source.position;
        let position = match edit {
            LiteralEdit::Move => [x + 20., y + 10.],
            LiteralEdit::Scale => [182.5 - 0.5 * (x + center_x - 182.5) - center_x, 1.5 * y],
            LiteralEdit::Rotate => [182.5 - y - center_x, x + center_x - 182.5],
            LiteralEdit::Collapse => [182.5 - center_x, 0.],
        };
        let tangent = |[x, y]: [f64; 2]| match edit {
            LiteralEdit::Move => [x, y],
            LiteralEdit::Scale => [-0.5 * x, 1.5 * y],
            LiteralEdit::Rotate => [-y, x],
            LiteralEdit::Collapse => [0., 0.],
        };
        let preserve_equal = |values: [f64; 2], original: [f64; 2]| {
            std::array::from_fn(|axis| {
                if values[axis] == original[axis] {
                    original[axis]
                } else {
                    values[axis]
                }
            })
        };
        path.vertices[index] = PathVertex {
            position: preserve_equal(position, source.position),
            incoming: preserve_equal(tangent(source.incoming), source.incoming),
            outgoing: preserve_equal(tangent(source.outgoing), source.outgoing),
        };
    }
    path
}
fn static_expected(before: &Project, edit: LiteralEdit, all: bool) -> Project {
    let mut expected = before.clone();
    for (id, selected) in selections(all) {
        expected = edit_node(&expected, id, |node| {
            let ContentsKind::Path { path, .. } = &mut node.kind else {
                panic!()
            };
            *path = literal_path(path, id, &selected, edit);
        });
    }
    expected
}
fn assert_history(
    before: &Project,
    expected: &Project,
    transform: PathTransformSpec,
    all: bool,
) -> ProjectViews {
    let mut state = state(before, false);
    let key = KeyRef {
        id: 1,
        property: PropertyPath::Path(PathTarget::Contents(2)),
        frame: 0,
    };
    if before
        .composition()
        .layer(1)
        .unwrap()
        .track(key.property)
        .is_some_and(|track| track.keys().contains_key(&0))
    {
        state.selected_keys.insert(key);
    }
    let selected_keys = state.selected_keys.clone();
    let graph = state.graph_channels.clone();
    let views = state.capture_views();
    state.bulk_test_action(&Action::Edit(command(transform, all)));
    exact(
        state.editor.project(),
        expected,
        "cross-path literal source",
    );
    assert_eq!(state.selected_layers, [1].into());
    assert_eq!(state.editor.selected(), Some(1));
    assert_eq!(state.graph_channels, graph);
    assert_eq!(
        state.selected_keys, selected_keys,
        "valid Timeline key selection survives the edit"
    );
    assert_eq!(state.capture_views(), views);
    codec(&mut state, expected);
    renders(state.editor.project(), expected, before, true);
    state.bulk_test_action(&Action::Undo);
    exact(
        state.editor.project(),
        before,
        "one Undo restores all paths/poses",
    );
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
    state.bulk_test_action(&Action::Redo);
    exact(
        state.editor.project(),
        expected,
        "one Redo restores all paths/poses",
    );
    assert!(!state.editor.can_redo());
    views
}

#[test]
fn cross_path_literal_partial_selection_move_scale_rotate_and_zero_scale() {
    let before = fixture();
    let views = state(&before, false).capture_views();
    export("static-before", &before, &views);
    for (edit, label) in [
        (LiteralEdit::Move, "static-move"),
        (LiteralEdit::Scale, "static-scale"),
        (LiteralEdit::Rotate, "static-rotate"),
        (LiteralEdit::Collapse, "static-collapse"),
    ] {
        let expected = static_expected(&before, edit, false);
        let views = assert_history(&before, &expected, spec(edit), false);
        export(&format!("{label}-expected"), &expected, &views);
    }
}

fn animated_fixture(existing: bool, dormant: bool) -> Project {
    let mut project = fixture();
    for id in [2, 5] {
        project = edit_node(&project, id, |node| {
            let ContentsKind::Path { path, animation } = &mut node.kind else {
                panic!()
            };
            let poses = [
                shifted(path, -8., 4.),
                shifted(path, 16., 12.),
                shifted(path, 71., -23.),
                shifted(path, 71., -23.),
            ];
            let keys = if dormant {
                json!({})
            } else if existing {
                json!({"0":{"value":0.,"interpolation":"Linear"},
                    "30":{"value":1.,"interpolation":{"Bezier":{"x1":0.2,"y1":-0.1,"x2":0.8,"y2":1.2}}},
                    "60":{"value":0.,"interpolation":"Smooth"},"89":{"value":1.,"interpolation":"Hold"}})
            } else {
                json!({"0":{"value":0.,"interpolation":"Smooth"},"60":{"value":1.,"interpolation":"Hold"}})
            };
            *animation =
                serde_json::from_value(json!({"poses":poses,"timing":{"value":2.,"keys":keys}}))
                    .unwrap();
        });
    }
    project
}
fn animated_expected(before: &Project, existing: bool, dormant: bool) -> Project {
    let mut expected = before.clone();
    for (id, selected) in selections(false) {
        expected = edit_node(&expected, id, |node| {
            let ContentsKind::Path { path, animation } = &mut node.kind else {
                panic!()
            };
            // Frame30 is either an exact authored key, the midpoint of Smooth,
            // or the static dormant reference. Literal sample positions retain
            // all untouched tangents and nonselected anchors.
            let sampled = if dormant {
                shifted(path, 71., -23.)
            } else if existing {
                shifted(path, 16., 12.)
            } else {
                shifted(path, 4., 8.)
            };
            let changed = literal_path(&sampled, id, &selected, LiteralEdit::Move);
            let mut wire = serde_json::to_value(&animation).unwrap();
            wire["poses"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::to_value(changed).unwrap());
            if dormant {
                wire["timing"]["value"] = json!(4.);
            } else if existing {
                wire["timing"]["keys"]["30"]["value"] = json!(4.);
            } else {
                wire["timing"]["keys"]["30"] = json!({"value":4.,"interpolation":"Linear"});
            }
            *animation = serde_json::from_value(wire).unwrap();
        });
    }
    expected
}
#[test]
fn cross_path_current_frame_preserves_eased_keys_duplicate_unused_poses_and_base() {
    for (existing, dormant, label) in [
        (false, false, "sampled"),
        (true, false, "existing-key"),
        (false, true, "dormant-static"),
    ] {
        let before = animated_fixture(existing, dormant);
        let expected = animated_expected(&before, existing, dormant);
        let views = assert_history(&before, &expected, spec(LiteralEdit::Move), false);
        export(&format!("{label}-before"), &before, &views);
        export(&format!("{label}-expected"), &expected, &views);
    }
}

#[test]
fn cross_path_noops_and_atomic_rejections_preserve_complete_source_and_redo() {
    let before = animated_fixture(true, false);
    let mut editor = Editor::default();
    editor.replace_project(before.clone()).unwrap();
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo sentinel".into(),
        })
        .unwrap();
    editor.undo();
    editor.clear_history();
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo sentinel".into(),
        })
        .unwrap();
    let redo = editor.project().clone();
    editor.undo();
    editor
        .execute(command(PathTransformSpec::default(), false))
        .unwrap();
    exact(
        editor.project(),
        &before,
        "identity is exact, including pose pool",
    );
    assert!(!editor.can_undo());
    assert!(editor.can_redo());
    for selections in [
        BTreeMap::new(),
        [(2, BTreeSet::new())].into(),
        [(2, [0].into()), (5, [99].into())].into(),
        [(2, [0].into()), (8, [0].into())].into(),
        [(2, [0].into()), (3, [0].into())].into(),
        [(2, [0].into()), (999, [0].into())].into(),
    ] {
        assert!(
            editor
                .execute(Command::TransformContentsPoints {
                    id: 1,
                    frame: EDIT_FRAME,
                    selections,
                    transform: spec(LiteralEdit::Move)
                })
                .is_err()
        );
        exact(
            editor.project(),
            &before,
            "invalid member must not partially edit valid path",
        );
        assert!(!editor.can_undo());
        assert!(editor.can_redo());
    }
    for transform in [
        PathTransformSpec {
            translation: [f64::NAN, 0.],
            ..Default::default()
        },
        PathTransformSpec {
            translation: [2_000_001., 0.],
            ..Default::default()
        },
    ] {
        assert!(editor.execute(command(transform, false)).is_err());
        exact(editor.project(), &before, "invalid transform atomicity");
        assert!(!editor.can_undo());
        assert!(editor.can_redo());
    }
    editor.redo();
    exact(
        editor.project(),
        &redo,
        "noops and failures retain original Redo",
    );
}

#[test]
fn cross_path_existing_equal_pose_is_reused_without_overwriting_any_slot() {
    let before = animated_fixture(true, false);
    let mut expected = before.clone();
    for id in [2, 5] {
        expected = edit_node(&expected, id, |node| {
            let ContentsKind::Path { animation, .. } = &mut node.kind else {
                panic!()
            };
            let mut wire = serde_json::to_value(&animation).unwrap();
            wire["timing"]["keys"]["30"]["value"] = json!(0.);
            *animation = serde_json::from_value(wire).unwrap();
        });
    }
    let transform: PathTransformSpec = [-24., -8., 0., 100., 100., 0., 0.].into();
    let views = assert_history(&before, &expected, transform, true);
    export("reused-pose-before", &before, &views);
    export("reused-pose-expected", &expected, &views);
}

#[test]
fn cross_path_singular_group_locked_layer_and_wrong_frame_are_atomic() {
    let ordinary = fixture();
    let singular = edit_node(&ordinary, 4, |node| {
        scalar(node, ContentsParam::Transform(Property::ScaleX), 0.)
    });
    let near_singular = edit_node(&ordinary, 4, |node| {
        scalar(node, ContentsParam::Transform(Property::ScaleX), 1e-10)
    });
    let mut wire = serde_json::to_value(&ordinary).unwrap();
    layer_wire(&mut wire, 1)["locked"] = json!(true);
    let locked = Project::from_json(&wire.to_string()).unwrap();
    for before in [singular, near_singular, locked] {
        let mut editor = Editor::default();
        editor.replace_project(before.clone()).unwrap();
        editor.clear_history();
        for transform in [spec(LiteralEdit::Move), PathTransformSpec::default()] {
            assert!(editor.execute(command(transform, false)).is_err());
            exact(
                editor.project(),
                &before,
                "singular or locked selection rejects atomically even identity",
            );
            assert!(!editor.can_undo());
        }
    }
    let mut editor = Editor::default();
    editor.replace_project(ordinary.clone()).unwrap();
    editor.clear_history();
    assert!(
        editor
            .execute(Command::TransformContentsPoints {
                id: 1,
                frame: 90,
                selections: selections(false),
                transform: spec(LiteralEdit::Move)
            })
            .is_err()
    );
    exact(
        editor.project(),
        &ordinary,
        "out-of-composition frame rejects atomically",
    );
    assert!(!editor.can_undo());
}

// Deliberately separate homogeneous-matrix oracle, with no Affine calls.
type Matrix = [[f64; 3]; 3];
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|row| {
        std::array::from_fn(|col| (0..3).map(|k| a[row][k] * b[k][col]).sum())
    })
}
fn translation(x: f64, y: f64) -> Matrix {
    [[1., 0., x], [0., 1., y], [0., 0., 1.]]
}
fn scale(x: f64, y: f64) -> Matrix {
    [[x, 0., 0.], [0., y, 0.], [0., 0., 1.]]
}
fn rotation(degrees: f64) -> Matrix {
    let a = degrees * std::f64::consts::PI / 180.;
    [
        [a.cos(), -a.sin(), 0.],
        [a.sin(), a.cos(), 0.],
        [0., 0., 1.],
    ]
}
fn inverse(a: Matrix) -> Matrix {
    let determinant = a[0][0] * a[1][1] - a[0][1] * a[1][0];
    let linear = [
        [a[1][1] / determinant, -a[0][1] / determinant, 0.],
        [-a[1][0] / determinant, a[0][0] / determinant, 0.],
        [0., 0., 1.],
    ];
    multiply(linear, translation(-a[0][2], -a[1][2]))
}
fn group(x: f64, y: f64, sx: f64, sy: f64, angle: f64, skew: f64, axis: f64) -> Matrix {
    let shear = [
        [1., -(skew * std::f64::consts::PI / 180.).tan(), 0.],
        [0., 1., 0.],
        [0., 0., 1.],
    ];
    [
        translation(x, y),
        rotation(angle),
        rotation(-axis),
        shear,
        rotation(axis),
        scale(sx / 100., sy / 100.),
    ]
    .into_iter()
    .reduce(multiply)
    .unwrap()
}
fn matrix_path(source: &VectorPath, selected: &BTreeSet<usize>, matrix: Matrix) -> VectorPath {
    let mut result = source.clone();
    for &index in selected {
        let apply = |point: [f64; 2], w: f64| {
            [
                matrix[0][0] * point[0] + matrix[0][1] * point[1] + matrix[0][2] * w,
                matrix[1][0] * point[0] + matrix[1][1] * point[1] + matrix[1][2] * w,
            ]
        };
        let source = source.vertices[index];
        result.vertices[index] = PathVertex {
            position: apply(source.position, 1.),
            incoming: apply(source.incoming, 0.),
            outgoing: apply(source.outgoing, 0.),
        };
    }
    result
}
fn oblique_fixture() -> Project {
    let mut project = fixture();
    for (id, sx, sy, angle, skew, axis) in [
        (1, -90., 110., 17., 13., 23.),
        (4, 80., -120., -19., -11., 31.),
        (13, 105., 95., 8., 7., -15.),
    ] {
        project = edit_node(&project, id, |node| {
            for (p, value) in [
                (ContentsParam::Transform(Property::ScaleX), sx),
                (ContentsParam::Transform(Property::ScaleY), sy),
                (ContentsParam::Transform(Property::Rotation), angle),
                (ContentsParam::Skew, skew),
                (ContentsParam::SkewAxis, axis),
            ] {
                scalar(node, p, value)
            }
            if id == 13 {
                node.parameters.insert(ContentsParam::Transform(Property::PositionX),serde_json::from_value(json!({"value":99.,"keys":{"0":{"value":-8.,"interpolation":"Linear"},"60":{"value":8.,"interpolation":"Smooth"}}})).unwrap());
            }
        });
    }
    // Independently authored parent-layer and residual affine offset exercise
    // the complete path-to-composition chain in addition to nested Groups.
    let mut editor = Editor::default();
    editor.replace_project(project).unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::Null,
            width: 20.,
            height: 20.,
            name: "Layer parent".into(),
        })
        .unwrap();
    let mut wire = serde_json::to_value(editor.project()).unwrap();
    let child = layer_wire(&mut wire, 1);
    child["parent"] = json!(2);
    child["transform_offset"] = json!([1., 0., 0.125, 1., -3., 6.]);
    let parent = layer_wire(&mut wire, 2);
    for (name, value) in [
        ("PositionX", 15.),
        ("PositionY", -8.),
        ("AnchorX", 0.),
        ("AnchorY", 0.),
        ("ScaleX", 95.),
        ("ScaleY", 105.),
        ("Rotation", -7.),
    ] {
        parent["properties"][name] = json!({"value":value,"keys":{}});
    }
    wire["version"] = json!(54);
    Project::from_json(&wire.to_string()).unwrap()
}
#[test]
fn cross_path_oblique_nested_reflections_skew_and_sampled_parent_have_independent_world_math() {
    let before = oblique_fixture();
    let transform: PathTransformSpec = [9., -7., 27., -85., 130., 200., 120.].into();
    let world_edit = [
        translation(9., -7.),
        translation(200., 120.),
        rotation(27.),
        scale(-0.85, 1.3),
        translation(-200., -120.),
    ]
    .into_iter()
    .reduce(multiply)
    .unwrap();
    let mut expected = before.clone();
    for (id, selected) in selections(false) {
        let world = if id == 2 {
            group(90., 100., -90., 110., 17., 13., 23.)
        } else {
            multiply(
                group(0., 0., 105., 95., 8., 7., -15.),
                group(280., 100., 80., -120., -19., -11., 31.),
            )
        };
        let layer = multiply(
            group(15., -8., 95., 105., -7., 0., 0.),
            [[1., 0.125, -3.], [0., 1., 6.], [0., 0., 1.]],
        );
        let world = multiply(layer, world);
        let local = multiply(inverse(world), multiply(world_edit, world));
        expected = edit_node(&expected, id, |node| {
            let ContentsKind::Path { path, .. } = &mut node.kind else {
                panic!()
            };
            *path = matrix_path(path, &selected, local);
        });
    }
    let mut state = state(&before, false);
    state.bulk_test_action(&Action::Edit(command(transform, false)));
    // Floating-point order differs between independent 3x3 and optimized affine
    // math. Bound only selected geometry; all other serialized source is exact.
    let actual = state.editor.project();
    let mut metadata = actual.clone();
    for (id, selected) in selections(false) {
        let ContentsKind::Path { path: a, .. } = node(actual, id).kind else {
            panic!()
        };
        let ContentsKind::Path { path: e, .. } = node(&expected, id).kind else {
            panic!()
        };
        for index in 0..a.vertices.len() {
            if !selected.contains(&index) {
                assert_eq!(a.vertices[index], e.vertices[index]);
                continue;
            }
            for (actual, expected) in [
                a.vertices[index].position,
                a.vertices[index].incoming,
                a.vertices[index].outgoing,
            ]
            .into_iter()
            .flatten()
            .zip(
                [
                    e.vertices[index].position,
                    e.vertices[index].incoming,
                    e.vertices[index].outgoing,
                ]
                .into_iter()
                .flatten(),
            ) {
                assert!(
                    (actual - expected).abs() <= 1e-10,
                    "independent geometry id{id} vertex{index}: {actual} vs{expected}"
                );
            }
        }
        metadata = edit_node(&metadata, id, |node| {
            let ContentsKind::Path { path, .. } = &mut node.kind else {
                panic!()
            };
            *path = e;
        });
    }
    exact(
        &metadata,
        &expected,
        "all non-geometry source remains exact",
    );
    renders(actual, &expected, &before, true);
    let actual = actual.clone();
    let views = codec(&mut state, &actual);
    state.bulk_test_action(&Action::Undo);
    exact(state.editor.project(), &before, "oblique one Undo");
    assert!(!state.editor.can_undo());
    state.bulk_test_action(&Action::Redo);
    exact(state.editor.project(), &actual, "oblique one Redo");
    export("oblique-before", &before, &views);
    export("oblique-expected", &expected, &views);
}

fn native_fixture() -> Project {
    let fixture = edit_node(&fixture(), 10, |node| node.enabled = false);
    let mut project = edit_node(&fixture, 4, |node| {
        scalar(node, ContentsParam::Transform(Property::PositionX), 270.);
    });
    project = edit_node(&project, 5, |node| {
        let path = curve(40., 25.);
        let animation = serde_json::from_value(json!({
            "poses": [shifted(&path, -8., 0.), path, shifted(&path, 8., 0.), shifted(&path, 47., 19.)],
            "timing": {"value": 3., "keys": {
                "0": {"value": 0., "interpolation": "Linear"},
                "30": {"value": 1., "interpolation": "Smooth"},
                "60": {"value": 2., "interpolation": "Hold"},
                "89": {"value": 2., "interpolation": "Linear"}
            }}
        })).unwrap();
        node.kind = ContentsKind::Path { path, animation };
    });
    project
}
fn native_spec(edit: LiteralEdit) -> PathTransformSpec {
    match edit {
        LiteralEdit::Move => [20., 10., 0., 100., 100., 180., 100.].into(),
        LiteralEdit::Scale => [0., 0., 0., 150., 150., 180., 100.].into(),
        LiteralEdit::Rotate => [0., 0., 90., 100., 100., 180., 100.].into(),
        LiteralEdit::Collapse => unreachable!(),
    }
}
fn native_expected(before: &Project, edit: LiteralEdit) -> Project {
    let mut expected = before.clone();
    for id in [2, 5] {
        expected = edit_node(&expected, id, |node| {
            let ContentsKind::Path { path, animation } = &mut node.kind else {
                panic!()
            };
            let center = if id == 2 { 90. } else { 270. };
            let mut changed = path.clone();
            for vertex in &mut changed.vertices {
                let [x, y] = vertex.position;
                vertex.position = match edit {
                    LiteralEdit::Move => [x + 20., y + 10.],
                    LiteralEdit::Scale => [180. + 1.5 * (x + center - 180.) - center, 1.5 * y],
                    LiteralEdit::Rotate => [180. - y - center, x + center - 180.],
                    LiteralEdit::Collapse => unreachable!(),
                };
                for tangent in [&mut vertex.incoming, &mut vertex.outgoing] {
                    let [x, y] = *tangent;
                    *tangent = match edit {
                        LiteralEdit::Move => [x, y],
                        LiteralEdit::Scale => [1.5 * x, 1.5 * y],
                        LiteralEdit::Rotate => [-y, x],
                        LiteralEdit::Collapse => unreachable!(),
                    };
                }
            }
            if id == 2 {
                *path = changed;
            } else {
                let mut wire = serde_json::to_value(&animation).unwrap();
                wire["poses"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::to_value(changed).unwrap());
                wire["timing"]["keys"]["30"]["value"] = json!(4.);
                *animation = serde_json::from_value(wire).unwrap();
            }
        });
    }
    expected
}
#[test]
fn cross_path_native_plan_has_literal_full_selection_expectations() {
    let before = native_fixture();
    let views = state(&before, true).capture_views();
    export("native-before", &before, &views);
    for (edit, label) in [
        (LiteralEdit::Move, "native-move"),
        (LiteralEdit::Scale, "native-scale"),
        (LiteralEdit::Rotate, "native-rotate"),
    ] {
        let expected = native_expected(&before, edit);
        let mut editor = Editor::default();
        editor.replace_project(before.clone()).unwrap();
        editor.clear_history();
        editor.execute(command(native_spec(edit), true)).unwrap();
        exact(editor.project(), &expected, label);
        renders(editor.project(), &expected, &before, true);
        export(&format!("{label}-expected"), &expected, &views);
    }
}

/// Requires a separately observed native save. Never manufactures native evidence.
#[test]
#[ignore = "requires separately recorded native save and independent expected fixture"]
fn verify_recorded_native_cross_path_save() {
    let actual_path = std::env::var_os("LIBREEFFECTS_CROSS_PATH_NATIVE_SAVE")
        .expect("set actual native save path");
    let expected_path = std::env::var_os("LIBREEFFECTS_CROSS_PATH_EXPECTED")
        .expect("set independent expected generated LEP path");
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_CROSS_PATH_EXPECTED_FRAME") {
        let frame: u32 = frame
            .parse()
            .expect("expected frame must be unsigned integer");
        assert!(frame < expected.project.composition().duration());
        expected
            .views
            .compositions
            .entry(expected.project.active_composition_id())
            .or_default()
            .frame = frame;
    }
    assert_eq!(actual.format, crate::project_io::ProjectFormat::Lep);
    exact(
        &actual.project,
        &expected.project,
        "native Save/Open complete source",
    );
    assert_eq!(
        actual.views, expected.views,
        "native Save/Open complete VIEW"
    );
    renders(&actual.project, &expected.project, &expected.project, false);
    println!(
        "Official decoder: exact complete source, VIEW and {} preview/output/reference/codec frame sets. Actual native file: {}",
        FRAMES.len(),
        Path::new(&actual_path).display()
    );
}
