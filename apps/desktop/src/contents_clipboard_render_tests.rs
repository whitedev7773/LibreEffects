//! Independent Contents clipboard source/render/codec acceptance.
//! Expected documents replace literal tree slots and IDs, never invoke Copy,
//! Cut, Paste or their allocation/selection helpers. Exported LEPs are generated
//! fixtures; only the ignored verifier accepts independently recorded native saves.
use super::{Action, EditorState};
use crate::{
    rendering::Renderer,
    view_state::{GraphChannel, GraphRanges, ProjectViews},
};
use libre_effects_core::*;
use serde_json::{Value, json};
use std::path::Path;

const WIDTH: u32 = 400;
const HEIGHT: u32 = 240;
const FRAMES: [u32; 9] = [0, 15, 29, 30, 31, 45, 59, 60, 89];

fn contents(project: &Project, layer: u64) -> &ShapeContents {
    let Content::ShapeContents(contents) = project.composition().layer(layer).unwrap().content()
    else {
        panic!("expected Contents layer {layer}")
    };
    contents
}
fn node(project: &Project, layer: u64, id: u64) -> ContentsNode {
    contents(project, layer).node(id).unwrap().clone()
}
fn track(wire: Value) -> AnimatedProperty {
    serde_json::from_value(wire).unwrap()
}
fn scalar(item: &mut ContentsNode, parameter: ContentsParam, value: f64) {
    assert!(item.parameters.contains_key(&parameter));
    item.parameters
        .insert(parameter, track(json!({"value": value, "keys": {}})));
}
fn layer_wire(project: &mut Value, id: u64) -> &mut Value {
    project["composition"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|layer| layer["id"] == id)
        .unwrap()
}
fn replace_tree(project: &Project, layer: u64, items: Vec<ContentsNode>, next_id: u64) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    layer_wire(&mut wire, layer)["content"]["ShapeContents"] =
        json!({"items": items, "next_id": next_id});
    Project::from_json(&wire.to_string()).unwrap()
}
fn version(project: &Project, value: u32) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    wire["version"] = json!(value);
    Project::from_json(&wire.to_string()).unwrap()
}
fn path(points: &[[f64; 2]]) -> VectorPath {
    VectorPath {
        vertices: points.iter().copied().map(PathVertex::corner).collect(),
        closed: true,
    }
}
fn compound(item: ContentsNode, stroke: bool) -> ContentsNode {
    let mut wire = serde_json::to_value(item).unwrap();
    let gradient = &mut wire["kind"][if stroke {
        "GradientStroke"
    } else {
        "GradientFill"
    }]["gradient"];
    gradient["next_stop"] = json!(7);
    // IDs are local to each paint. Fill and Stroke deliberately share 1..=6.
    gradient["colors_animation"] = json!({"keys": {
        "0": {"colors": [
            {"id": 1, "position": 0., "midpoint": 35., "red": 240.5, "green": 20., "blue": 10.},
            {"id": 2, "position": 100., "midpoint": 50., "red": 20., "green": 70., "blue": 250.}
        ], "opacities": [
            {"id": 3, "position": 0., "midpoint": 45., "opacity": 95.},
            {"id": 4, "position": 100., "midpoint": 50., "opacity": 55.}
        ]},
        "30": {"colors": [
            {"id": 2, "position": 0., "midpoint": 20., "red": 40., "green": 190., "blue": 30.},
            {"id": 5, "position": 55., "midpoint": 70., "red": 20., "green": 50., "blue": 245.},
            {"id": 1, "position": 100., "midpoint": 50., "red": 200., "green": 20., "blue": 170.}
        ], "opacities": [
            {"id": 4, "position": 0., "midpoint": 25., "opacity": 75.},
            {"id": 6, "position": 60., "midpoint": 65., "opacity": 30.},
            {"id": 3, "position": 100., "midpoint": 50., "opacity": 95.}
        ]},
        "60": {"colors": [
            {"id": 5, "position": 20., "midpoint": 75., "red": 250., "green": 180., "blue": 20.},
            {"id": 2, "position": 95., "midpoint": 50., "red": 40., "green": 20., "blue": 240.}
        ], "opacities": [
            {"id": 6, "position": 15., "midpoint": 30., "opacity": 85.},
            {"id": 4, "position": 85., "midpoint": 50., "opacity": 40.}
        ]}
    }});
    serde_json::from_value(wire).unwrap()
}
fn fixture() -> Project {
    let base = path(&[[-48., -34.], [52., -28.], [35., 40.], [-43., 29.]]);
    let pose = path(&[[-35., -40.], [63., -18.], [20., 49.], [-54., 17.]]);
    let unused = path(&[[-11., -9.], [33., -14.], [21., 38.], [-26., 28.]]);
    let animation: PathAnimation = serde_json::from_value(json!({
        "poses": [base, pose, unused], "timing": {"value": 2., "keys": {
            "0": {"value": 0., "interpolation": "Linear"},
            "60": {"value": 1., "interpolation": "Smooth"}
        }}
    }))
    .unwrap();
    let kinds = vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::GradientFill {
            even_odd: true,
            gradient: Default::default(),
        },
        ContentsKind::Path {
            path: base,
            animation,
        },
        ContentsKind::GradientStroke {
            style: Default::default(),
            gradient: Default::default(),
        },
        ContentsKind::Group(vec![]),
        ContentsKind::Parametric(ShapeKind::Ellipse),
        ContentsKind::Fill { even_odd: false },
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::Fill { even_odd: false },
    ];
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Contents clipboard acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    for name in ["Clipboard source", "Different local destination"] {
        editor
            .execute(Command::AddContent {
                content: Content::ShapeContents(Default::default()),
                width: WIDTH as f64,
                height: HEIGHT as f64,
                name: name.into(),
            })
            .unwrap();
    }
    for kind in kinds {
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add { parent: 0, kind },
            })
            .unwrap();
    }
    let project = editor.project();
    let mut rect = node(project, 1, 3);
    rect.name = "Animated rectangle".into();
    scalar(&mut rect, ContentsParam::Height, 57.);
    rect.parameters.insert(ContentsParam::Width, track(json!({"value": 999., "keys": {
        "0": {"value": 68., "interpolation": "Linear"},
        "30": {"value": 91., "interpolation": {"Bezier": {"x1": 0.2, "y1": -0.1, "x2": 0.8, "y2": 1.2}},
            "temporal": {"incoming": {"slope": 0.5, "influence": 0.3}, "outgoing": {"slope": -0.25, "influence": 0.4}}},
        "89": {"value": 43., "interpolation": "Hold"}
    }})));
    let mut fill = compound(node(project, 1, 4), false);
    fill.name = "Topology Fill".into();
    scalar(
        &mut fill,
        ContentsParam::Gradient(GradientParam::StartX),
        -50.,
    );
    scalar(&mut fill, ContentsParam::Gradient(GradientParam::EndX), 50.);
    let mut stroke = compound(node(project, 1, 6), true);
    stroke.name = "Topology Stroke".into();
    scalar(
        &mut stroke,
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        8.,
    );
    scalar(
        &mut stroke,
        ContentsParam::Gradient(GradientParam::StartX),
        -45.,
    );
    scalar(
        &mut stroke,
        ContentsParam::Gradient(GradientParam::EndX),
        45.,
    );
    let mut inner = node(project, 1, 2);
    inner.name = "Skewed animated subtree".into();
    scalar(&mut inner, ContentsParam::Skew, 11.);
    scalar(
        &mut inner,
        ContentsParam::Transform(Property::Rotation),
        -9.,
    );
    inner.kind = ContentsKind::Group(vec![rect, fill, node(project, 1, 5), stroke]);
    let mut dormant = node(project, 1, 8);
    dormant.parameters.insert(
        ContentsParam::Width,
        track(json!({"value": 27., "keys": {
            "15": {"value": 45., "interpolation": "Hold"},
            "89": {"value": 105., "interpolation": "Smooth", "temporal": {"mode": "Auto"}}
        }})),
    );
    let mut disabled = node(project, 1, 7);
    disabled.name = "Disabled authored subtree".into();
    disabled.enabled = false;
    scalar(
        &mut disabled,
        ContentsParam::Transform(Property::ScaleX),
        -120.,
    );
    scalar(&mut disabled, ContentsParam::Skew, -17.);
    disabled.kind = ContentsKind::Group(vec![dormant, node(project, 1, 9)]);
    let mut untouched = node(project, 1, 10);
    untouched.name = "Unselected gap".into();
    scalar(&mut untouched, ContentsParam::Width, 25.);
    scalar(&mut untouched, ContentsParam::Height, 40.);
    scalar(
        &mut untouched,
        ContentsParam::Transform(Property::PositionX),
        -95.,
    );
    let mut outer = node(project, 1, 1);
    outer.name = "Reflected source parent".into();
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::PositionX),
        115.,
    );
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::PositionY),
        117.,
    );
    scalar(&mut outer, ContentsParam::Transform(Property::ScaleX), -85.);
    scalar(&mut outer, ContentsParam::Transform(Property::ScaleY), 90.);
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::Rotation),
        13.,
    );
    outer.kind = ContentsKind::Group(vec![inner, untouched, disabled, node(project, 1, 11)]);
    let mut destination = node(project, 1, 1);
    destination.id = 20;
    destination.name = "Destination local space".into();
    scalar(
        &mut destination,
        ContentsParam::Transform(Property::PositionX),
        282.,
    );
    scalar(
        &mut destination,
        ContentsParam::Transform(Property::PositionY),
        125.,
    );
    scalar(
        &mut destination,
        ContentsParam::Transform(Property::ScaleX),
        105.,
    );
    scalar(
        &mut destination,
        ContentsParam::Transform(Property::ScaleY),
        85.,
    );
    scalar(
        &mut destination,
        ContentsParam::Transform(Property::Rotation),
        -17.,
    );
    scalar(&mut destination, ContentsParam::Skew, -8.);
    let mut background = node(project, 1, 8);
    background.id = 21;
    scalar(&mut background, ContentsParam::Width, 128.);
    scalar(&mut background, ContentsParam::Height, 95.);
    let mut background_fill = node(project, 1, 9);
    background_fill.id = 22;
    scalar(
        &mut background_fill,
        ContentsParam::Shape(ShapeParam::FillRed),
        30.,
    );
    scalar(
        &mut background_fill,
        ContentsParam::Shape(ShapeParam::FillGreen),
        45.,
    );
    scalar(
        &mut background_fill,
        ContentsParam::Shape(ShapeParam::FillBlue),
        65.,
    );
    destination.kind = ContentsKind::Group(vec![background, background_fill]);
    let p = version(project, 54);
    let p = replace_tree(&p, 1, vec![outer], 12);
    replace_tree(&p, 2, vec![destination], 23)
}
/// Explicit reference mapping, intentionally independent of recursive allocator.
fn copied(project: &Project, pairs: &[(u64, u64)]) -> Vec<ContentsNode> {
    fn map(mut item: ContentsNode, pairs: &[(u64, u64)]) -> ContentsNode {
        item.id = pairs.iter().find(|(old, _)| *old == item.id).unwrap().1;
        if let ContentsKind::Group(children) = &mut item.kind {
            *children = children
                .iter()
                .cloned()
                .map(|item| map(item, pairs))
                .collect();
        }
        item
    }
    vec![
        map(node(project, 1, 2), pairs),
        map(node(project, 1, 7), pairs),
    ]
}
fn expected_paste(before: &Project, snapshot: &Project) -> Project {
    let mut destination = node(before, 2, 20);
    let ContentsKind::Group(children) = &mut destination.kind else {
        unreachable!()
    };
    children.extend(copied(
        snapshot,
        &[
            (2, 23),
            (3, 24),
            (4, 25),
            (5, 26),
            (6, 27),
            (7, 28),
            (8, 29),
            (9, 30),
        ],
    ));
    replace_tree(before, 2, vec![destination], 31)
}
fn expected_cut(before: &Project) -> Project {
    let mut outer = node(before, 1, 1);
    outer.kind = ContentsKind::Group(vec![node(before, 1, 10), node(before, 1, 11)]);
    replace_tree(before, 1, vec![outer], 12)
}
fn exact(actual: &Project, expected: &Project, context: &str) {
    assert_eq!(actual, expected, "{context}");
    assert_eq!(
        serde_json::to_vec(actual).unwrap(),
        serde_json::to_vec(expected).unwrap(),
        "{context}: complete serialized source"
    );
}
fn channel(layer: u64, item: u64) -> GraphChannel {
    GraphChannel {
        id: layer,
        property: PropertyPath::Contents {
            item,
            parameter: ContentsParam::Width,
        },
    }
}
fn state(project: &Project) -> EditorState {
    let mut state = EditorState::default();
    state.editor.replace_project(project.clone()).unwrap();
    state.editor.select(1);
    state.editor.clear_history();
    state.selected_layers.insert(1);
    state.frame = 30;
    state.timeline_zoom = 2.;
    state.preview_zoom = Some(1.5);
    state.preview_pan = [13., -7.];
    state.graph_open = true;
    for channel in [channel(1, 3), channel(1, 10), channel(2, 21)] {
        if channel.available(project.composition()) {
            state.graph_channels.pin(channel).unwrap();
            state.graph_channels.ranges.insert(
                channel,
                GraphRanges {
                    value: Some([-100., 200.]),
                    speed: Some([-50., 50.]),
                },
            );
        }
    }
    state.graph_channels.activate(channel(2, 21));
    state.normalize();
    state
}
fn renders(actual: &Project, expected: &Project, before: &Project, changes: bool) {
    let renderer = Renderer::new();
    let encoded = project_file::encode(actual, None).unwrap();
    let decoded = project_file::decode(&encoded).unwrap().project;
    let mut changed = false;
    let mut visible = false;
    let mut distinct = false;
    let mut previous = None;
    for frame in FRAMES {
        let output = renderer
            .render_output(actual, frame, WIDTH, HEIGHT)
            .unwrap();
        visible |= output
            .chunks_exact(4)
            .any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2]);
        distinct |= previous.as_ref().is_some_and(|pixels| pixels != &output);
        previous = Some(output.clone());
        assert_eq!(
            output,
            renderer.render(actual, frame, WIDTH).unwrap(),
            "preview/output frame {frame}"
        );
        assert_eq!(
            output,
            renderer
                .render_output(expected, frame, WIDTH, HEIGHT)
                .unwrap(),
            "literal-source reference frame {frame}"
        );
        assert_eq!(
            output,
            renderer
                .render_output(&decoded, frame, WIDTH, HEIGHT)
                .unwrap(),
            "codec frame {frame}"
        );
        changed |= output
            != renderer
                .render_output(before, frame, WIDTH, HEIGHT)
                .unwrap();
    }
    assert!(visible, "fixture must render meaningful colored pixels");
    assert_eq!(changed, changes, "visual-change contract");
    if contents(actual, 2).node(23).is_some() {
        assert!(distinct, "animated payload must affect multiple frames");
    }
}
fn codec(state: &mut EditorState, expected: &Project) -> ProjectViews {
    let views = state.capture_views();
    let encoded_view = views.encode_native(expected).unwrap();
    let bytes = project_file::encode(state.editor.project(), Some(&encoded_view)).unwrap();
    let decoded = project_file::decode(&bytes).unwrap();
    exact(&decoded.project, expected, "official LEP full source");
    assert_eq!(decoded.view, Some(encoded_view.as_slice()));
    let loaded = ProjectViews::read_native(decoded.view.unwrap(), &decoded.project).unwrap();
    assert_eq!(loaded, views);
    let mut reopened = EditorState::default();
    reopened.editor.replace_project(decoded.project).unwrap();
    reopened.load_views(loaded);
    assert_eq!(
        reopened.capture_views().encode_native(expected).unwrap(),
        encoded_view
    );
    views
}
fn export(label: &str, project: &Project, views: &ProjectViews) {
    let Some(directory) = std::env::var_os("LIBREEFFECTS_EXPORT_CONTENTS_CLIPBOARD_FIXTURES")
    else {
        return;
    };
    let root = Path::new(&directory);
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let view = views.encode_native(project).unwrap();
    for (extension, bytes) in [
        ("lfe.json", project.to_json().unwrap().into_bytes()),
        (
            "generated.lep",
            project_file::encode(project, Some(&view)).unwrap(),
        ),
    ] {
        use std::io::Write;
        let path = root.join(format!("{label}.{extension}"));
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
fn paste_command(clipboard: ContentsClipboard) -> Command {
    Command::Contents {
        id: 2,
        edit: ContentsEdit::Paste {
            parent: 20,
            index: 2,
            clipboard,
        },
    }
}

#[test]
fn clipboard_copy_paste_literal_cross_layer_source_ids_view_history_and_pixels() {
    let before = fixture();
    let expected = expected_paste(&before, &before);
    let mut state = state(&before);
    state.bulk_test_action(&Action::Edit(Command::RenameLayer {
        id: 1,
        name: "Redo sentinel".into(),
    }));
    state.bulk_test_action(&Action::Undo);
    let selected_key = KeyRef {
        id: 1,
        property: channel(1, 3).property,
        frame: 30,
    };
    state.selected_keys.insert(selected_key);
    let graph = state.graph_channels.clone();
    let before_view = state.capture_views();
    let snapshot = state.editor.copy_contents(1, 1, &[7, 2]).unwrap();
    assert_eq!(snapshot.len(), 2);
    assert!(!snapshot.is_empty());
    state.set_contents_clipboard(snapshot.clone());
    assert!(state.contents_clipboard().is_some());
    exact(state.editor.project(), &before, "Copy does not edit source");
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo(), "Copy preserves Redo");
    state.bulk_test_action(&Action::Edit(paste_command(snapshot)));
    exact(
        state.editor.project(),
        &expected,
        "cross-layer literal Paste",
    );
    assert_eq!(
        state.graph_channels, graph,
        "pins must remain bound to original IDs"
    );
    assert_eq!(state.capture_views(), before_view);
    assert_eq!(state.selected_layers, [1].into());
    assert_eq!(
        state.selected_keys,
        [selected_key].into(),
        "Paste retains valid original key selection"
    );
    let views = codec(&mut state, &expected);
    renders(state.editor.project(), &expected, &before, true);
    state.bulk_test_action(&Action::Undo);
    exact(
        state.editor.project(),
        &before,
        "one Undo removes whole block",
    );
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
    assert!(
        state.selected_keys.is_empty(),
        "history keeps existing key-selection clearing policy"
    );
    assert_eq!(state.graph_key, None);
    let copy_again = state.editor.copy_contents(1, 1, &[2, 7]).unwrap();
    state.set_contents_clipboard(copy_again);
    assert!(state.editor.can_redo());
    state.bulk_test_action(&Action::Redo);
    exact(
        state.editor.project(),
        &expected,
        "Redo same recursive IDs/source",
    );
    assert!(!state.editor.can_redo());
    export("complex-before", &before, &before_view);
    export("complex-paste-expected", &expected, &views);
}

#[test]
fn clipboard_cut_snapshot_survives_source_edit_delete_and_restores_literal_payload() {
    let before = fixture();
    let cut = expected_cut(&before);
    let expected = expected_paste(&cut, &before);
    let mut state = state(&before);
    let original_graph = state.graph_channels.clone();
    state.selected_keys.insert(KeyRef {
        id: 1,
        property: channel(1, 3).property,
        frame: 30,
    });
    let snapshot = state.editor.copy_contents(1, 1, &[7, 2]).unwrap();
    state.set_contents_clipboard(snapshot.clone());
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::RemoveSiblings {
            parent: 1,
            items: vec![7, 2],
        },
    }));
    exact(
        state.editor.project(),
        &cut,
        "Cut removes selected subtrees only; allocator retained",
    );
    assert!(!state.graph_channels.is_available(channel(1, 3)));
    assert!(
        state.selected_keys.is_empty(),
        "Cut normalization drops removed keys"
    );
    assert_eq!(state.graph_channels.pinned, original_graph.pinned);
    assert_eq!(state.graph_channels.ranges, original_graph.ranges);
    let cut_view = codec(&mut state, &cut);
    renders(state.editor.project(), &cut, &before, true);
    state.bulk_test_action(&Action::Undo);
    exact(state.editor.project(), &before, "Cut Undo full source");
    assert_eq!(state.graph_channels, original_graph);
    state.bulk_test_action(&Action::Redo);
    exact(state.editor.project(), &cut, "Cut Redo");
    state.bulk_test_action(&Action::Edit(paste_command(snapshot)));
    exact(
        state.editor.project(),
        &expected,
        "captured deleted subtrees remain exact",
    );
    assert!(
        !state.graph_channels.is_available(channel(1, 3)),
        "new destination identities cannot revive old pins"
    );
    let views = codec(&mut state, &expected);
    renders(state.editor.project(), &expected, &cut, true);
    state.bulk_test_action(&Action::Undo);
    exact(
        state.editor.project(),
        &cut,
        "Paste Undo preserves Cut transaction",
    );
    state.bulk_test_action(&Action::Undo);
    exact(state.editor.project(), &before, "second Undo restores Cut");
    assert!(!state.editor.can_undo());
    state.bulk_test_action(&Action::Redo);
    state.bulk_test_action(&Action::Redo);
    exact(state.editor.project(), &expected, "both Redo transactions");
    export("complex-cut-expected", &cut, &cut_view);
    export("complex-cut-paste-expected", &expected, &views);

    // Capture once, edit the source, then delete its entire layer. The stored
    // snapshot must not alias either live descendants or current layer existence.
    let mut editor = Editor::default();
    editor.replace_project(before.clone()).unwrap();
    let clipboard = editor.copy_contents(1, 1, &[7, 2]).unwrap();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Rename {
                item: 2,
                name: "Changed after Copy".into(),
            },
        })
        .unwrap();
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::SetSharedValue {
                parent: 2,
                items: vec![3],
                parameter: ContentsParam::Width,
                frame: 30,
                value: 39.,
            },
        })
        .unwrap();
    editor.execute(Command::RemoveLayer(1)).unwrap();
    let deleted = editor.project().clone();
    // Ordinary layer deletion may lower the project schema after the last
    // compound paint disappears; Paste must reintroduce its existing v54 gate.
    let expected_deleted = expected_paste(&version(&deleted, 54), &before);
    editor.clear_history();
    assert_eq!(
        editor.paste_contents(2, 20, 2, &clipboard).unwrap(),
        vec![23, 28]
    );
    exact(
        editor.project(),
        &expected_deleted,
        "snapshot survives source edit and whole-layer deletion",
    );
    renders(editor.project(), &expected_deleted, &deleted, true);
    editor.undo();
    exact(editor.project(), &deleted, "convenience Paste is one Undo");
    assert!(!editor.can_undo());
    editor.redo();
    exact(
        editor.project(),
        &expected_deleted,
        "convenience Paste Redo",
    );
}

#[test]
fn clipboard_schema_preserves_legacy_and_promotes_only_materialized_features() {
    let source = fixture();
    // An independently seeded destination with the same composition identity and
    // FPS exercises the core token contract. UI document replacement clears its
    // clipboard, so this is not evidence of user-facing cross-document support.
    let mut target = replace_tree(&source, 1, vec![], 12);
    target = version(&target, 53);
    let mut source_editor = Editor::default();
    source_editor.replace_project(source.clone()).unwrap();
    let static_snapshot = source_editor.copy_contents(1, 1, &[10]).unwrap();
    let compound_snapshot = source_editor.copy_contents(1, 1, &[7, 2]).unwrap();
    let mut editor = Editor::default();
    editor.replace_project(target.clone()).unwrap();
    let mut item = node(&source, 1, 10);
    item.id = 23;
    let mut destination = node(&target, 2, 20);
    let ContentsKind::Group(children) = &mut destination.kind else {
        unreachable!()
    };
    children.push(item);
    let static_expected = replace_tree(&target, 2, vec![destination], 24);
    editor.paste_contents(2, 20, 2, &static_snapshot).unwrap();
    exact(
        editor.project(),
        &static_expected,
        "simple snapshot retains declared v53",
    );
    let mut cut_editor = Editor::default();
    cut_editor.replace_project(static_expected.clone()).unwrap();
    cut_editor.clear_history();
    let static_cut_expected = replace_tree(&target, 2, vec![node(&target, 2, 20)], 24);
    cut_editor
        .execute(Command::Contents {
            id: 2,
            edit: ContentsEdit::RemoveSiblings {
                parent: 20,
                items: vec![23],
            },
        })
        .unwrap();
    exact(
        cut_editor.project(),
        &static_cut_expected,
        "legacy Cut retains v53 and consumed allocator",
    );
    cut_editor.undo();
    exact(cut_editor.project(), &static_expected, "legacy Cut Undo");
    assert!(!cut_editor.can_undo());
    cut_editor.redo();
    exact(
        cut_editor.project(),
        &static_cut_expected,
        "legacy Cut Redo",
    );
    editor.undo();
    exact(
        editor.project(),
        &target,
        "legacy Undo retains source and schema",
    );
    let expected = version(&expected_paste(&version(&target, 54), &source), 54);
    editor.paste_contents(2, 20, 2, &compound_snapshot).unwrap();
    exact(
        editor.project(),
        &expected,
        "compound-only schema promotion v53 to v54",
    );
    renders(editor.project(), &expected, &target, true);
    editor.undo();
    exact(editor.project(), &target, "promotion Undo exact v53");
    editor.redo();
    exact(editor.project(), &expected, "promotion Redo exact v54");
    let views = ProjectViews::default();
    export("legacy-before", &target, &views);
    export("legacy-static-paste-expected", &static_expected, &views);
    export("legacy-compound-paste-expected", &expected, &views);
}

#[test]
fn clipboard_scrambled_visual_order_and_repeated_paste_have_independent_recursive_ids() {
    let catalog = fixture();
    let mut outer = node(&catalog, 1, 1);
    outer.kind = ContentsKind::Group(vec![
        node(&catalog, 1, 7),
        node(&catalog, 1, 10),
        node(&catalog, 1, 2),
        node(&catalog, 1, 11),
    ]);
    let before = replace_tree(&catalog, 1, vec![outer], 12);
    let mut destination = node(&before, 2, 20);
    let ContentsKind::Group(children) = &mut destination.kind else {
        unreachable!()
    };
    let mut first = copied(
        &before,
        &[
            (7, 23),
            (8, 24),
            (9, 25),
            (2, 26),
            (3, 27),
            (4, 28),
            (5, 29),
            (6, 30),
        ],
    );
    first.reverse();
    children.extend(first);
    let expected_once = replace_tree(&before, 2, vec![destination.clone()], 31);
    let ContentsKind::Group(children) = &mut destination.kind else {
        unreachable!()
    };
    let mut second = copied(
        &before,
        &[
            (7, 31),
            (8, 32),
            (9, 33),
            (2, 34),
            (3, 35),
            (4, 36),
            (5, 37),
            (6, 38),
        ],
    );
    second.reverse();
    children.extend(second);
    let expected_twice = replace_tree(&before, 2, vec![destination], 39);
    let mut editor = Editor::default();
    editor.replace_project(before.clone()).unwrap();
    editor.clear_history();
    let snapshot = editor.copy_contents(1, 1, &[2, 7]).unwrap();
    assert_eq!(
        editor.paste_contents(2, 20, 2, &snapshot).unwrap(),
        vec![23, 26]
    );
    exact(
        editor.project(),
        &expected_once,
        "visual order differs from both ID and click order",
    );
    assert_eq!(
        editor.paste_contents(2, 20, 4, &snapshot).unwrap(),
        vec![31, 34]
    );
    exact(
        editor.project(),
        &expected_twice,
        "second paste allocates independent descendant IDs",
    );
    renders(editor.project(), &expected_twice, &before, true);
    editor.undo();
    exact(
        editor.project(),
        &expected_once,
        "one Undo removes only second copy",
    );
    editor.undo();
    exact(editor.project(), &before, "second Undo removes first copy");
    assert!(!editor.can_undo());
    editor.redo();
    editor.redo();
    exact(
        editor.project(),
        &expected_twice,
        "repeated Paste Redo retains exact two ID blocks",
    );
    let views = ProjectViews::default();
    export("scrambled-before", &before, &views);
    export("scrambled-once-expected", &expected_once, &views);
    export("scrambled-twice-expected", &expected_twice, &views);
}

fn native_fixture() -> Project {
    let catalog = fixture();
    let mut source = node(&catalog, 1, 1);
    source.name = "Copy me".into();
    for (property, value) in [
        (Property::PositionX, 100.),
        (Property::PositionY, 90.),
        (Property::ScaleX, 100.),
        (Property::ScaleY, 100.),
        (Property::Rotation, 0.),
    ] {
        scalar(&mut source, ContentsParam::Transform(property), value);
    }
    let mut fill = node(&catalog, 1, 11);
    fill.name = "Copper Fill".into();
    scalar(&mut fill, ContentsParam::Shape(ShapeParam::FillRed), 225.);
    scalar(&mut fill, ContentsParam::Shape(ShapeParam::FillGreen), 100.);
    scalar(&mut fill, ContentsParam::Shape(ShapeParam::FillBlue), 40.);
    scalar(
        &mut fill,
        ContentsParam::Shape(ShapeParam::FillOpacity),
        70.,
    );
    source.kind = ContentsKind::Group(vec![node(&catalog, 1, 3), fill]);
    let mut destination = node(&catalog, 1, 1);
    destination.id = 7;
    destination.name = "Destination".into();
    for (property, value) in [
        (Property::PositionX, 140.),
        (Property::PositionY, 20.),
        (Property::ScaleX, 100.),
        (Property::ScaleY, 100.),
        (Property::Rotation, 0.),
    ] {
        scalar(&mut destination, ContentsParam::Transform(property), value);
    }
    destination.kind = ContentsKind::Group(vec![]);
    let project = replace_tree(&catalog, 1, vec![source, destination], 12);
    let mut cross_layer = node(&project, 2, 20);
    scalar(
        &mut cross_layer,
        ContentsParam::Transform(Property::PositionX),
        170.,
    );
    scalar(
        &mut cross_layer,
        ContentsParam::Transform(Property::PositionY),
        20.,
    );
    replace_tree(&project, 2, vec![cross_layer], 23)
}
fn native_clone(source: &Project, root: u64, rectangle: u64, fill: u64) -> ContentsNode {
    // Each role and ID is literal; neither traversal nor allocator is shared.
    let mut copied = node(source, 1, 1);
    copied.id = root;
    let mut copied_rectangle = node(source, 1, 3);
    copied_rectangle.id = rectangle;
    let mut copied_fill = node(source, 1, 11);
    copied_fill.id = fill;
    copied.kind = ContentsKind::Group(vec![copied_rectangle, copied_fill]);
    copied
}
fn native_expected(source: &Project, cut: bool, placement: &str) -> Project {
    let original = node(source, 1, 1);
    let mut destination = node(source, 1, 7);
    let mut roots = if cut { vec![] } else { vec![original] };
    let mut next = 12;
    if matches!(placement, "group" | "repeat") {
        let mut children = vec![native_clone(source, 12, 13, 14)];
        next = 15;
        if placement == "repeat" {
            children.push(native_clone(source, 15, 16, 17));
            next = 18;
        }
        destination.kind = ContentsKind::Group(children);
    }
    roots.push(destination);
    if placement == "root" {
        roots.push(native_clone(source, 12, 13, 14));
        next = 15;
    }
    let mut expected = replace_tree(source, 1, roots, next);
    if placement == "cross-layer" {
        let mut destination = node(source, 2, 20);
        let ContentsKind::Group(children) = &mut destination.kind else {
            unreachable!()
        };
        children.push(native_clone(source, 23, 24, 25));
        expected = replace_tree(&expected, 2, vec![destination], 26);
    }
    expected
}
#[test]
fn clipboard_native_plan_fixtures_have_literal_root_group_repeat_and_cut_expectations() {
    let before = native_fixture();
    let mut view_state = EditorState::default();
    view_state.editor.replace_project(before.clone()).unwrap();
    view_state.frame = 30;
    view_state.normalize();
    let views = view_state.capture_views();
    export("native-before", &before, &views);
    for (cut, placement, label) in [
        (true, "none", "native-cut-expected"),
        (false, "group", "native-group-paste-expected"),
        (false, "repeat", "native-repeat-paste-expected"),
        (true, "group", "native-cut-paste-expected"),
        (false, "root", "native-root-paste-expected"),
        (false, "cross-layer", "native-cross-layer-paste-expected"),
    ] {
        let expected = native_expected(&before, cut, placement);
        let mut editor = Editor::default();
        editor.replace_project(before.clone()).unwrap();
        editor.clear_history();
        let clipboard = editor.copy_contents(1, 0, &[1]).unwrap();
        if cut {
            editor
                .execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::RemoveSiblings {
                        parent: 0,
                        items: vec![1],
                    },
                })
                .unwrap();
        }
        match placement {
            "group" | "repeat" => {
                editor.paste_contents(1, 7, 0, &clipboard).unwrap();
            }
            "root" => {
                editor.paste_contents(1, 0, 2, &clipboard).unwrap();
            }
            "cross-layer" => {
                editor.paste_contents(2, 20, 2, &clipboard).unwrap();
            }
            "none" => {}
            _ => unreachable!(),
        }
        if placement == "repeat" {
            editor.paste_contents(1, 7, 1, &clipboard).unwrap();
        }
        exact(editor.project(), &expected, label);
        renders(editor.project(), &expected, &before, true);
        export(label, &expected, &views);
    }
}

#[test]
fn clipboard_pending_native_field_expectation_is_literal_before_copy_snapshot() {
    let before = native_fixture();
    let mut source = node(&before, 1, 1);
    scalar(
        &mut source,
        ContentsParam::Transform(Property::PositionX),
        110.,
    );
    // The existing ordinary singleton field edit uses the generic migration
    // route; its current minimal schema is44 after compound paints are absent.
    let edited = version(
        &replace_tree(&before, 1, vec![source, node(&before, 1, 7)], 12),
        44,
    );
    let expected = native_expected(&edited, false, "group");
    let mut state = EditorState::default();
    state.editor.replace_project(before.clone()).unwrap();
    state.editor.select(1);
    state.editor.clear_history();
    state.frame = 30;
    state.normalize();
    let views = state.capture_views();
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::Track {
            item: 1,
            parameter: ContentsParam::Transform(Property::PositionX),
            edit: TrackEdit::Value {
                frame: 30,
                value: 110.,
            },
        },
    }));
    exact(
        state.editor.project(),
        &edited,
        "literal pending field edit",
    );
    let clipboard = state.editor.copy_contents(1, 0, &[1]).unwrap();
    state.set_contents_clipboard(clipboard.clone());
    state.bulk_test_action(&Action::Edit(Command::Contents {
        id: 1,
        edit: ContentsEdit::Paste {
            parent: 7,
            index: 0,
            clipboard,
        },
    }));
    exact(
        state.editor.project(),
        &expected,
        "Copy snapshots committed110 before Group Paste",
    );
    codec(&mut state, &expected);
    renders(state.editor.project(), &expected, &before, true);
    state.bulk_test_action(&Action::Undo);
    exact(
        state.editor.project(),
        &edited,
        "Paste Undo retains separately committed field",
    );
    state.bulk_test_action(&Action::Undo);
    exact(
        state.editor.project(),
        &before,
        "second Undo restores field and original schema54",
    );
    assert!(!state.editor.can_undo());
    export("native-pending-field-expected", &edited, &views);
    export("native-pending-copy-paste-expected", &expected, &views);
}

/// This checker consumes files saved by a separately observed native application.
/// It never creates that evidence or infers native event delivery from fixtures.
#[test]
#[ignore = "requires separately recorded native save and independent expected fixture"]
fn verify_recorded_native_contents_clipboard_save() {
    let actual_path = std::env::var_os("LIBREEFFECTS_CONTENTS_CLIPBOARD_NATIVE_SAVE")
        .expect("set actual native save path");
    let expected_path = std::env::var_os("LIBREEFFECTS_CONTENTS_CLIPBOARD_EXPECTED")
        .expect("set independent expected generated LEP path");
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_CONTENTS_CLIPBOARD_EXPECTED_FRAME") {
        let frame: u32 = frame
            .parse()
            .expect("expected frame must be an unsigned integer");
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
