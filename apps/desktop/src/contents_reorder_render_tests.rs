//! A sibling permutation preserves data, but deliberately changes paint scope/order.
use super::{edit, scene, value};
use libre_effects_core::*;
use std::collections::BTreeMap;

fn contents(project: &Project) -> &ShapeContents {
    match project.composition().layer(1).unwrap().content() {
        Content::ShapeContents(contents) => contents,
        _ => panic!("Expected promoted fixture"),
    }
}

fn children(project: &Project, parent: u64) -> Vec<u64> {
    contents(project)
        .rows()
        .into_iter()
        .filter_map(|(_, owner, node)| (owner == parent).then_some(node.id))
        .collect()
}

fn nodes(project: &Project) -> BTreeMap<u64, ContentsNode> {
    contents(project)
        .rows()
        .into_iter()
        .map(|(_, _, node)| (node.id, node.clone()))
        .collect()
}

fn animate(editor: &mut Editor, item: u64, parameter: ContentsParam, a: f64, b: f64) {
    value(editor, item, parameter, a);
    edit(
        editor,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
    edit(
        editor,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value {
                frame: 60,
                value: b,
            },
        },
    );
}

fn color(editor: &mut Editor, item: u64, rgb: [f64; 3]) {
    for (parameter, channel) in [
        ShapeParam::FillRed,
        ShapeParam::FillGreen,
        ShapeParam::FillBlue,
    ]
    .into_iter()
    .zip(rgb)
    {
        value(editor, item, ContentsParam::Shape(parameter), channel);
    }
}

fn fill(project: &Project, parent: u64) -> u64 {
    contents(project)
        .rows()
        .into_iter()
        .find(|(_, owner, node)| *owner == parent && matches!(node.kind, ContentsKind::Fill { .. }))
        .unwrap()
        .2
        .id
}

fn native_roundtrip(project: &Project) -> Project {
    let bytes = project_file::encode(project, None).unwrap();
    let reopened = project_file::decode(&bytes).unwrap().project;
    assert_eq!(&reopened, project);
    reopened
}

fn render(
    renderer: &crate::rendering::Renderer,
    project: &Project,
    frame: u32,
) -> image::RgbaImage {
    let preview = renderer.render(project, frame, 400).unwrap();
    assert_eq!(
        preview,
        renderer.render_output(project, frame, 400, 240).unwrap()
    );
    preview
}

#[test]
fn sibling_paint_block_before_geometry_changes_scope_without_editing_payloads() {
    let mut editor = scene(ShapeKind::Rectangle);
    edit(&mut editor, ContentsEdit::Promote);
    edit(&mut editor, ContentsEdit::Remove(3));
    edit(
        &mut editor,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Fill { even_odd: false },
        },
    );
    assert_eq!(children(editor.project(), 1), vec![2, 5, 4]);
    color(&mut editor, 5, [255., 0., 0.]);
    animate(
        &mut editor,
        5,
        ContentsParam::Shape(ShapeParam::FillOpacity),
        90.,
        30.,
    );
    value(
        &mut editor,
        1,
        ContentsParam::Transform(Property::Rotation),
        16.,
    );
    value(&mut editor, 1, ContentsParam::Skew, 12.);
    let before = editor.project().clone();
    let original_nodes = nodes(&before);
    let renderer = crate::rendering::Renderer::new();
    for frame in [0, 30, 60] {
        assert_eq!(
            render(&renderer, &before, frame).get_pixel(200, 120)[3],
            255
        );
    }

    // Both paints move as a block before the geometry. They now consume no paths.
    edit(
        &mut editor,
        ContentsEdit::Reorder {
            parent: 1,
            order: vec![5, 4, 2],
        },
    );
    let after = editor.project().clone();
    assert_eq!(children(&after, 1), vec![5, 4, 2]);
    for id in [2, 4, 5] {
        assert_eq!(contents(&after).node(id).unwrap(), &original_nodes[&id]);
    }
    assert_eq!(
        contents(&after).node(1).unwrap().parameters,
        original_nodes[&1].parameters
    );
    let reopened = native_roundtrip(&after);
    for frame in [0, 30, 60] {
        let actual = render(&renderer, &after, frame);
        assert!(actual.pixels().all(|pixel| pixel[3] == 0));
        assert_eq!(actual, render(&renderer, &reopened, frame));
    }
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
}

#[test]
fn noncontiguous_root_group_block_changes_overlap_preserving_animated_subtrees() {
    let mut editor = scene(ShapeKind::Rectangle);
    edit(&mut editor, ContentsEdit::Promote);
    edit(&mut editor, ContentsEdit::Remove(3));
    edit(&mut editor, ContentsEdit::Duplicate(1));
    let red = children(editor.project(), 0)[0];
    edit(&mut editor, ContentsEdit::Duplicate(1));
    let green = children(editor.project(), 0)
        .into_iter()
        .find(|id| ![1, red].contains(id))
        .unwrap();
    // Duplicate inserts before its source. Establish the intended initial order
    // with the existing singleton command, independent of the new permutation.
    edit(
        &mut editor,
        ContentsEdit::Move {
            item: 1,
            parent: 0,
            index: 0,
        },
    );
    edit(
        &mut editor,
        ContentsEdit::Move {
            item: green,
            parent: 0,
            index: 1,
        },
    );
    assert_eq!(children(editor.project(), 0), vec![1, green, red]);
    let red_fill = fill(editor.project(), red);
    let green_fill = fill(editor.project(), green);
    color(&mut editor, red_fill, [255., 0., 0.]);
    color(&mut editor, green_fill, [0., 255., 0.]);
    animate(
        &mut editor,
        1,
        ContentsParam::Transform(Property::Rotation),
        0.,
        18.,
    );
    animate(
        &mut editor,
        green,
        ContentsParam::Transform(Property::PositionX),
        0.,
        24.,
    );
    value(
        &mut editor,
        red,
        ContentsParam::Transform(Property::PositionX),
        -40.,
    );
    let before = editor.project().clone();
    let original_nodes = nodes(&before);

    // Selected noncontiguous siblings [blue, red] move together after green.
    edit(
        &mut editor,
        ContentsEdit::Reorder {
            parent: 0,
            order: vec![green, 1, red],
        },
    );
    let after = editor.project().clone();
    assert_eq!(nodes(&after), original_nodes);
    let reopened = native_roundtrip(&after);
    let renderer = crate::rendering::Renderer::new();
    for frame in [0, 30, 60] {
        let old = render(&renderer, &before, frame);
        let actual = render(&renderer, &after, frame);
        assert_eq!(old.get_pixel(200, 120).0, [32, 64, 128, 255]);
        assert_eq!(actual.get_pixel(200, 120).0, [0, 255, 0, 255]);
        assert_ne!(actual, old);
        assert_eq!(actual, render(&renderer, &reopened, frame));
    }
    assert_ne!(render(&renderer, &after, 0), render(&renderer, &after, 60));
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
}
