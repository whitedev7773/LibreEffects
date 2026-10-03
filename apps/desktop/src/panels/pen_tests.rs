use super::*;
use libre_effects_core::{
    ContentsEdit, ContentsKind, ContentsParam, Property, PropertyPath, TrackEdit,
};

fn polygon(closed: bool) -> VectorPath {
    VectorPath {
        closed,
        vertices: [
            [20., 20.],
            [120., 20.],
            [220., 20.],
            [220., 220.],
            [120., 220.],
            [20., 220.],
        ]
        .into_iter()
        .map(PathVertex::corner)
        .collect(),
    }
}
fn scene(closed: bool) -> EditorState {
    let mut s = EditorState::default();
    s.tool = Tool::Pen;
    s.composition_started = true;
    let c = s.editor.project().composition();
    s.editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(polygon(closed)),
                fill: closed,
                ..Default::default()
            }),
            width: c.width() as f64,
            height: c.height() as f64,
            name: "Path test".into(),
        })
        .unwrap();
    s.editor.clear_history();
    s
}
fn current(s: &EditorState) -> (Target, VectorPath, Affine) {
    paths(s).remove(0)
}
fn pick(pen: &mut Pen, s: &EditorState, index: usize, shift: bool) {
    let (_, path, world) = current(s);
    assert!(
        pen.down(
            s,
            world.point(path.vertices[index].position),
            1.,
            false,
            shift,
            false
        )
        .is_none()
    );
    assert!(pen.up(s).is_none());
}
fn selected(pen: &Pen) -> Vec<usize> {
    pen.selected
        .as_ref()
        .unwrap()
        .vertices
        .iter()
        .copied()
        .collect()
}
fn pair(pen: &mut Pen, s: &EditorState) {
    pick(pen, s, 0, false);
    pick(pen, s, 1, true);
}
fn begin(pen: &mut Pen, s: &EditorState, offset: [f64; 2]) -> (VectorPath, Affine, [f64; 2]) {
    let (_, original, world) = current(s);
    let pointer = add(original.vertices[0].position, offset);
    assert!(
        pen.down(s, world.point(pointer), 1., false, false, false)
            .is_none()
    );
    (original, world, pointer)
}
fn near(a: [f64; 2], b: [f64; 2]) {
    assert!(distance(a, b) < 1e-8, "{a:?} != {b:?}");
}
fn assert_move(original: &VectorPath, changed: &VectorPath, indices: &[usize], delta: [f64; 2]) {
    assert_eq!(original.closed, changed.closed);
    assert_eq!(original.vertices.len(), changed.vertices.len());
    for (i, (before, after)) in original.vertices.iter().zip(&changed.vertices).enumerate() {
        near(
            after.position,
            if indices.contains(&i) {
                add(before.position, delta)
            } else {
                before.position
            },
        );
        assert_eq!(before.incoming, after.incoming);
        assert_eq!(before.outgoing, after.outgoing);
    }
}
fn value(s: &mut EditorState, id: LayerId, property: Property, value: f64) {
    s.editor
        .execute(Command::SetValue {
            id,
            property,
            frame: 0,
            value,
        })
        .unwrap();
}
fn contents(s: &mut EditorState, edit: ContentsEdit) {
    s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn target(s: &EditorState) -> PathTarget {
    match current(s).0 {
        Target::Shape(_) => PathTarget::Shape,
        Target::Contents(_, item) => PathTarget::Contents(item),
        Target::Mask(id, index) => PathTarget::Mask(
            s.editor
                .project()
                .composition()
                .layer(id)
                .unwrap()
                .path_masks()[index]
                .id,
        ),
        _ => unreachable!(),
    }
}
fn animate(s: &mut EditorState) {
    let target = target(s);
    s.editor
        .execute(Command::AnimatePath {
            id: 1,
            target,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    let mut path = current(s).1;
    for v in &mut path.vertices {
        v.position = add(v.position, [40., 20.]);
    }
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target,
            frame: 20,
            path,
        })
        .unwrap();
    s.frame = 10;
    s.editor.clear_history();
}
fn redo_available(s: &mut EditorState) {
    value(s, 1, Property::Opacity, 45.);
    s.editor.undo();
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
}

#[test]
fn shift_selection_toggles_without_geometry_history_or_preview_commands() {
    let mut s = scene(true);
    redo_available(&mut s);
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    assert_eq!(selected(&pen), [0, 1]);
    assert_eq!(pen.overlay(&s)[0].3, [0, 1].into());
    pick(&mut pen, &s, 0, false);
    assert_eq!(selected(&pen), [0, 1]);
    pick(&mut pen, &s, 2, false);
    assert_eq!(selected(&pen), [2]);
    pick(&mut pen, &s, 2, true);
    assert!(selected(&pen).is_empty());
    assert!(pen.pending(&s).is_none());
    assert!(pen.key("delete", &s).0);
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
}
#[test]
fn shift_click_and_pointer_motion_is_selection_only_and_empty_click_does_not_insert_or_create() {
    let s = scene(true);
    let mut pen = Pen::default();
    pick(&mut pen, &s, 0, false);
    let (_, path, world) = current(&s);
    pen.down(
        &s,
        world.point(path.vertices[1].position),
        1.,
        false,
        true,
        false,
    );
    pen.moving(world.point([170., 70.]), false, true);
    assert!(pen.pending(&s).is_none());
    assert!(pen.up(&s).is_none());
    assert_eq!(selected(&pen), [0, 1]);
    for p in [[70., 20.], [450., 450.]] {
        assert!(
            pen.down(&s, world.point(p), 1., false, true, false)
                .is_none()
        );
        assert!(pen.up(&s).is_none());
        assert!(pen.draft.is_none());
        assert_eq!(selected(&pen), [0, 1]);
    }
}
#[test]
fn selected_vertices_move_from_frozen_pose_without_jump_drift_or_tangent_changes() {
    let mut s = scene(true);
    let mut path = current(&s).1;
    path.vertices[0].incoming = [-15., -25.];
    path.vertices[0].outgoing = [25., 35.];
    path.vertices[1].incoming = [-18., -12.];
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target: PathTarget::Shape,
            frame: 0,
            path,
        })
        .unwrap();
    s.editor.clear_history();
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let (original, world, pointer) = begin(&mut pen, &s, [3., -2.]);
    pen.moving(world.point(pointer), false, false);
    assert!(pen.pending(&s).is_none());
    for delta in [[10., 15.], [30., 40.], [-10., 7.], [30., 40.]] {
        pen.moving(world.point(add(pointer, delta)), false, false);
        assert_move(
            &original,
            &pen.drag.as_ref().unwrap().session.path,
            &[0, 1],
            delta,
        );
        assert_eq!(s.editor.project(), &before);
    }
    let command = pen.up(&s).unwrap();
    assert!(matches!(command, Command::EditPath { .. }));
    s.editor.execute(command).unwrap();
    assert_move(&original, &current(&s).1, &[0, 1], [30., 40.]);
    assert_eq!(selected(&pen), [0, 1]);
    s.editor.undo();
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
}
#[test]
fn animated_selection_and_drag_returning_to_start_do_not_insert_keys_or_clear_redo() {
    let mut s = scene(true);
    animate(&mut s);
    redo_available(&mut s);
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    assert_eq!(
        s.editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::Path(target(&s)))
            .unwrap()
            .keys()
            .len(),
        2
    );
    let (_, world, pointer) = begin(&mut pen, &s, [2., 1.]);
    pen.moving(world.point(add(pointer, [45., 30.])), false, false);
    assert!(pen.pending(&s).is_some());
    pen.moving(world.point(pointer), false, false);
    assert!(pen.pending(&s).is_none());
    assert!(pen.up(&s).is_none());
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
}
#[test]
fn animated_multi_vertex_edit_creates_one_middle_pose_and_one_undo() {
    let mut s = scene(true);
    animate(&mut s);
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let (original, world, pointer) = begin(&mut pen, &s, [1., 2.]);
    pen.moving(world.point(add(pointer, [30., -8.])), false, false);
    s.editor.execute(pen.up(&s).unwrap()).unwrap();
    assert_move(&original, &current(&s).1, &[0, 1], [30., -8.]);
    assert_eq!(
        s.editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::Path(target(&s)))
            .unwrap()
            .keys()
            .len(),
        3
    );
    let after = s.editor.project().clone();
    s.editor.undo();
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
    s.editor.redo();
    assert_eq!(s.editor.project(), &after);
}
#[test]
fn transformed_parent_and_nested_contents_skew_use_shared_local_axis_delta() {
    let mut s = scene(true);
    contents(&mut s, ContentsEdit::Promote);
    contents(
        &mut s,
        ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::Group(vec![]),
        },
    );
    // Promotion creates group 1 with Path 2 / Fill 3 / Stroke 4; outer group is 5.
    contents(
        &mut s,
        ContentsEdit::Move {
            item: 1,
            parent: 5,
            index: 0,
        },
    );
    for (item, parameter, amount) in [
        (1, ContentsParam::Skew, 23.),
        (1, ContentsParam::SkewAxis, 49.),
        (5, ContentsParam::Transform(Property::Rotation), 31.),
        (5, ContentsParam::Transform(Property::ScaleY), 65.),
    ] {
        contents(
            &mut s,
            ContentsEdit::Track {
                item,
                parameter,
                edit: TrackEdit::Value {
                    frame: 0,
                    value: amount,
                },
            },
        );
    }
    s.editor.execute(Command::AddNull).unwrap();
    s.editor
        .execute(Command::SetParent {
            id: 1,
            parent: Some(2),
            frame: 0,
        })
        .unwrap();
    value(&mut s, 2, Property::Rotation, 27.);
    value(&mut s, 2, Property::ScaleX, -130.);
    value(&mut s, 2, Property::ScaleY, 80.);
    value(&mut s, 1, Property::Rotation, -19.);
    s.editor.select(1);
    s.editor.clear_history();
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let (original, world, pointer) = begin(&mut pen, &s, [1., 1.]);
    for (motion, expected) in [([12., 30.], [0., 30.]), ([35., 8.], [35., 0.])] {
        pen.moving(world.point(add(pointer, motion)), false, true);
        assert_move(
            &original,
            &pen.drag.as_ref().unwrap().session.path,
            &[0, 1],
            expected,
        );
    }
    s.editor.execute(pen.up(&s).unwrap()).unwrap();
    assert_move(&original, &current(&s).1, &[0, 1], [35., 0.]);
    s.editor.undo();
    assert_eq!(s.editor.project(), &before);
}
fn mask_scene() -> EditorState {
    let mut s = scene(true);
    // Move shape outside the mask so hit tests address the mask itself.
    let mut shape = current(&s).1;
    for v in &mut shape.vertices {
        v.position = add(v.position, [500., 0.]);
    }
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target: PathTarget::Shape,
            frame: 0,
            path: shape,
        })
        .unwrap();
    s.editor
        .execute(Command::SetPathMasks {
            id: 1,
            masks: vec![PathMask {
                path: polygon(true),
                ..Default::default()
            }],
        })
        .unwrap();
    s.editor.clear_history();
    s
}
#[test]
fn another_path_replaces_the_vertex_set_and_highlights_only_its_vertices() {
    let s = mask_scene();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    assert_eq!(selected(&pen), [0, 1]);
    pen.down(&s, [20., 20.], 1., false, true, false);
    assert!(pen.up(&s).is_none());
    assert!(matches!(
        pen.selected.as_ref().unwrap().target,
        Target::Mask(1, 0)
    ));
    assert_eq!(selected(&pen), [0]);
    let overlay = pen.overlay(&s);
    assert!(overlay[0].3.is_empty());
    assert_eq!(overlay[1].3, [0].into());
    pen.down(&s, [120., 20.], 1., false, true, false);
    assert!(pen.up(&s).is_none());
    assert_eq!(selected(&pen), [0, 1]);
    pick(&mut pen, &s, 3, false);
    assert!(matches!(
        pen.selected.as_ref().unwrap().target,
        Target::Shape(1)
    ));
    assert_eq!(selected(&pen), [3]);
}
#[test]
fn multiple_mask_vertices_move_in_parent_coordinates_and_animate_atomically() {
    let mut s = mask_scene();
    // Use the mask target directly because this scene also has a shape path.
    let mask_id = s.editor.selected_layer().unwrap().path_masks()[0].id;
    s.editor.execute(Command::AddNull).unwrap();
    s.editor
        .execute(Command::SetParent {
            id: 1,
            parent: Some(2),
            frame: 0,
        })
        .unwrap();
    value(&mut s, 2, Property::Rotation, 32.);
    value(&mut s, 2, Property::ScaleX, -120.);
    value(&mut s, 1, Property::ScaleY, 70.);
    s.editor.select(1);
    s.editor
        .execute(Command::AnimatePath {
            id: 1,
            target: PathTarget::Mask(mask_id),
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    s.frame = 10;
    s.editor.clear_history();
    let before = s.editor.project().clone();
    let (_, original, world) = paths(&s).remove(1);
    let mut pen = Pen::default();
    for i in [0, 1] {
        pen.down(
            &s,
            world.point(original.vertices[i].position),
            1.,
            false,
            i > 0,
            false,
        );
        assert!(pen.up(&s).is_none());
    }
    let pointer = add(original.vertices[0].position, [1., 1.]);
    pen.down(&s, world.point(pointer), 1., false, false, false);
    pen.moving(world.point(add(pointer, [15., 30.])), false, false);
    s.editor.execute(pen.up(&s).unwrap()).unwrap();
    assert_move(&original, &paths(&s)[1].1, &[0, 1], [15., 30.]);
    assert_eq!(
        s.editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::Path(PathTarget::Mask(mask_id)))
            .unwrap()
            .keys()
            .len(),
        2
    );
    s.editor.undo();
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
}
#[test]
fn multi_delete_is_one_static_edit_and_preserves_open_closed_minima() {
    for closed in [false, true] {
        let mut s = scene(closed);
        let before = s.editor.project().clone();
        let original = current(&s).1;
        let mut pen = Pen::default();
        pair(&mut pen, &s);
        let (handled, command) = pen.key("delete", &s);
        assert!(handled);
        s.editor.execute(command.unwrap()).unwrap();
        assert_eq!(current(&s).1.vertices, original.vertices[2..]);
        assert!(pen.key("delete", &s).0); // Never fall through to layer deletion.
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        pen.reset_if_stale(&s);
        let minimum = if closed { 3 } else { 2 };
        for i in 0..(6 - minimum + 1) {
            pick(&mut pen, &s, i, i > 0);
        }
        assert!(matches!(pen.key("delete", &s), (true, None)));
        assert_eq!(s.editor.project(), &before);
        // Removing exactly to the minimum remains legal.
        pick(&mut pen, &s, 6 - minimum, true);
        s.editor.execute(pen.key("delete", &s).1.unwrap()).unwrap();
        assert_eq!(current(&s).1.vertices.len(), minimum);
        pick(&mut pen, &s, 0, false);
        assert!(matches!(pen.key("delete", &s), (true, None)));
        assert_eq!(s.editor.project().composition().layers().len(), 1);
    }
}
#[test]
fn animated_topology_rejection_preserves_selection_document_and_history_on_repeated_delete() {
    let mut s = scene(true);
    animate(&mut s);
    redo_available(&mut s);
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    for _ in 0..2 {
        let (handled, command) = pen.key("delete", &s);
        assert!(handled);
        assert!(
            s.editor
                .execute(command.unwrap())
                .unwrap_err()
                .contains("topology")
        );
        assert_eq!(selected(&pen), [0, 1]);
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
    }
}
#[test]
fn handles_edit_only_the_hit_vertex_and_alt_conversion_and_insertion_still_commit() {
    let mut s = scene(true);
    let mut path = current(&s).1;
    path.vertices[0].incoming = [-30., -15.];
    path.vertices[0].outgoing = [30., 15.];
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target: PathTarget::Shape,
            frame: 0,
            path,
        })
        .unwrap();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let (_, original, world) = current(&s);
    pen.down(&s, world.point([50., 35.]), 1., false, false, false);
    pen.moving(world.point([75., 30.]), true, true);
    let changed = &pen.drag.as_ref().unwrap().session.path;
    assert_eq!(changed.vertices[0].outgoing, [55., 0.]);
    assert_eq!(changed.vertices[0].incoming, original.vertices[0].incoming);
    assert_eq!(changed.vertices[1..], original.vertices[1..]);
    s.editor.execute(pen.up(&s).unwrap()).unwrap();
    pen.down(
        &s,
        world.point(original.vertices[0].position),
        1.,
        true,
        false,
        false,
    );
    s.editor.execute(pen.up(&s).unwrap()).unwrap();
    assert_eq!(current(&s).1.vertices[0].incoming, [0., 0.]);
    assert_eq!(current(&s).1.vertices[0].outgoing, [0., 0.]);
    pen.down(&s, world.point([70., 20.]), 1., false, false, false);
    s.editor.execute(pen.up(&s).unwrap()).unwrap();
    assert_eq!(current(&s).1.vertices.len(), original.vertices.len() + 1);
}
#[test]
fn stale_context_and_escape_or_focus_cancel_drop_the_entire_pending_edit() {
    let mutations: &[fn(&mut EditorState)] = &[
        |s| s.frame += 1,
        |s| s.tool = Tool::Select,
        |s| s.document_revision += 1,
        |s| s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 1)),
        |s| {
            s.selected_layers.insert(1);
        },
        |s| s.editor.clear_selection(),
        |s| {
            value(s, 1, Property::Rotation, 5.);
        },
        |s| {
            s.editor.execute(Command::AddNull).unwrap();
        },
    ];
    for mutate in mutations {
        let mut s = scene(true);
        let mut pen = Pen::default();
        pair(&mut pen, &s);
        let (_, world, pointer) = begin(&mut pen, &s, [0., 0.]);
        pen.moving(world.point(add(pointer, [10., 10.])), false, false);
        mutate(&mut s);
        let after_external_change = s.editor.project().clone();
        assert!(pen.pending(&s).is_none());
        assert!(pen.up(&s).is_none());
        assert!(pen.selected.is_none());
        assert_eq!(s.editor.project(), &after_external_change);
    }
    for escape in [false, true] {
        let s = scene(true);
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        pair(&mut pen, &s);
        let (_, world, pointer) = begin(&mut pen, &s, [0., 0.]);
        pen.moving(world.point(add(pointer, [10., 10.])), false, false);
        if escape {
            assert!(pen.key("escape", &s).0);
        } else {
            pen.cancel();
        }
        assert!(pen.pending(&s).is_none());
        assert!(pen.up(&s).is_none());
        assert!(pen.selected.is_none());
        assert_eq!(s.editor.project(), &before);
    }
}
#[test]
fn rendered_draft_is_isolated_and_commit_roundtrips_with_identical_output_pixels() {
    let mut s = scene(true);
    let renderer = crate::rendering::Renderer::new();
    let before = s.editor.project().clone();
    let original_pixels = renderer.render(&before, 0, 480).unwrap();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let (_, world, pointer) = begin(&mut pen, &s, [0., 0.]);
    pen.moving(world.point(add(pointer, [70., 90.])), false, false);
    let mut draft = libre_effects_core::Editor::default();
    draft.replace_project(before.clone()).unwrap();
    draft.execute(pen.pending(&s).unwrap()).unwrap();
    let draft_pixels = renderer.render(draft.project(), 0, 480).unwrap();
    assert_ne!(draft_pixels, original_pixels);
    assert_eq!(s.editor.project(), &before);
    assert_eq!(
        renderer.render(s.editor.project(), 0, 480).unwrap(),
        original_pixels
    );
    s.editor.execute(pen.up(&s).unwrap()).unwrap();
    let restored = Project::from_json(&s.editor.project().to_json().unwrap()).unwrap();
    assert_eq!(&restored, s.editor.project());
    assert_eq!(renderer.render(&restored, 0, 480).unwrap(), draft_pixels);
    assert_eq!(
        renderer.render_preview(&restored, 0, 480).unwrap(),
        draft_pixels
    );
    s.editor.undo();
    assert_eq!(
        renderer.render(s.editor.project(), 0, 480).unwrap(),
        original_pixels
    );
}

fn single_path_scene(kind: usize) -> EditorState {
    let mut s = scene(true);
    if kind == 1 {
        contents(&mut s, ContentsEdit::Promote);
    } else if kind == 2 {
        s.editor = libre_effects_core::Editor::default();
        let c = s.editor.project().composition();
        s.editor
            .execute(Command::AddContent {
                content: Content::Solid,
                width: c.width() as f64,
                height: c.height() as f64,
                name: "Masked solid".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::SetPathMasks {
                id: 1,
                masks: vec![PathMask {
                    path: polygon(true),
                    ..Default::default()
                }],
            })
            .unwrap();
    }
    s.editor.clear_history();
    s
}
#[test]
fn shape_contents_and_masks_share_noop_animation_and_atomic_static_deletion_policy() {
    for kind in 0..3 {
        let mut s = single_path_scene(kind);
        let original = current(&s).1;
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        pair(&mut pen, &s);
        s.editor.execute(pen.key("delete", &s).1.unwrap()).unwrap();
        assert_eq!(current(&s).1.vertices, original.vertices[2..]);
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        s.editor.clear_history();
        animate(&mut s);
        redo_available(&mut s);
        let before = s.editor.project().clone();
        pen.reset_if_stale(&s);
        pair(&mut pen, &s);
        let (original, world, pointer) = begin(&mut pen, &s, [2., 1.]);
        pen.moving(world.point(add(pointer, [25., 15.])), false, false);
        pen.moving(world.point(pointer), false, false);
        assert!(pen.up(&s).is_none());
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        for _ in 0..2 {
            let (handled, command) = pen.key("delete", &s);
            assert!(handled);
            assert!(
                s.editor
                    .execute(command.unwrap())
                    .unwrap_err()
                    .contains("topology")
            );
            assert_eq!(selected(&pen), [0, 1]);
            assert_eq!(s.editor.project(), &before);
        }
        begin(&mut pen, &s, [2., 1.]);
        pen.moving(world.point(add(pointer, [25., 15.])), false, false);
        s.editor.execute(pen.up(&s).unwrap()).unwrap();
        assert_move(&original, &current(&s).1, &[0, 1], [25., 15.]);
        assert_eq!(
            s.editor
                .selected_layer()
                .unwrap()
                .track(PropertyPath::Path(target(&s)))
                .unwrap()
                .keys()
                .len(),
            3
        );
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
}
#[test]
fn rejected_animated_insertion_does_not_leave_an_out_of_range_selection_or_delete_the_layer() {
    let mut s = scene(true);
    animate(&mut s);
    let before = s.editor.project().clone();
    let (_, path, world) = current(&s);
    // Insertion in the closing segment adds a vertex at the old vertex count.
    let midpoint = curve(path.vertices.last().unwrap(), &path.vertices[0], 0.5);
    let mut pen = Pen::default();
    pen.down(&s, world.point(midpoint), 1., false, false, false);
    let command = pen.up(&s).unwrap();
    assert!(s.editor.execute(command).unwrap_err().contains("topology"));
    assert!(selected(&pen).is_empty());
    assert!(matches!(pen.key("delete", &s), (true, None)));
    assert_eq!(s.editor.project(), &before);
}
#[test]
fn opening_gradient_modal_cancels_pen_draft_and_vertex_drag_before_modal_preview() {
    for new_path in [false, true] {
        let mut s = single_path_scene(1);
        contents(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::GradientFill {
                    gradient: Default::default(),
                    even_odd: false,
                },
            },
        );
        let composition = s.editor.project().active_composition_id();
        s.contents_selection = Some((composition, 1, 5));
        let mut pen = Pen::default();
        if new_path {
            pen.down(&s, [500., 500.], 1., false, false, false);
            pen.up(&s);
            pen.down(&s, [600., 500.], 1., false, false, false);
            pen.up(&s);
        } else {
            pair(&mut pen, &s);
            let (_, world, pointer) = begin(&mut pen, &s, [0., 0.]);
            pen.moving(world.point(add(pointer, [25., 15.])), false, false);
        }
        assert!(pen.pending(&s).is_some());
        let before = s.editor.project().clone();
        s.gradient_editor = Some(crate::panels::gradient_editor::Session::new(&s, 5).unwrap());
        assert!(pen.pending(&s).is_none());
        assert!(pen.overlay(&s).is_empty());
        pen.reset_if_stale(&s);
        assert!(pen.drag.is_none());
        assert!(pen.draft.is_none());
        assert!(pen.selected.is_none());
        assert!(pen.up(&s).is_none());
        assert_eq!(s.editor.project(), &before);
    }
}
#[test]
fn switching_composition_or_replacing_same_document_cancels_pending_path_gestures() {
    for replace_same in [false, true] {
        let mut s = scene(true);
        if !replace_same {
            s.editor.execute(Command::NewComposition).unwrap();
            s.editor.activate_composition(1).unwrap();
            s.editor.select(1);
        }
        let mut pen = Pen::default();
        pair(&mut pen, &s);
        let (_, world, pointer) = begin(&mut pen, &s, [0., 0.]);
        pen.moving(world.point(add(pointer, [25., 15.])), false, false);
        if replace_same {
            // Open/recovery increments the document identity even for identical bytes.
            s.editor
                .replace_project(s.editor.project().clone())
                .unwrap();
            s.document_revision += 1;
        } else {
            s.editor.activate_composition(2).unwrap();
        }
        let before = s.editor.project().clone();
        assert!(pen.pending(&s).is_none());
        assert!(pen.up(&s).is_none());
        assert!(pen.selected.is_none());
        assert_eq!(s.editor.project(), &before);
    }
}

#[test]
fn repeated_pointer_down_or_rejected_delete_cannot_commit_an_abandoned_drag() {
    let s = scene(true);
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let (_, world, pointer) = begin(&mut pen, &s, [0., 0.]);
    pen.moving(world.point(add(pointer, [50., 20.])), false, false);
    assert!(pen.pending(&s).is_some());
    pen.down(&s, [500., 500.], 1., false, true, false);
    assert!(pen.up(&s).is_none());
    assert!(pen.pending(&s).is_none());
    assert_eq!(selected(&pen), [0, 1]);
    // An over-large deletion request also cancels the old pointer draft.
    pick(&mut pen, &s, 2, true);
    pick(&mut pen, &s, 3, true);
    begin(&mut pen, &s, [0., 0.]);
    pen.moving(world.point(add(pointer, [50., 20.])), false, false);
    assert!(matches!(pen.key("delete", &s), (true, None)));
    assert!(pen.pending(&s).is_none());
    assert!(pen.up(&s).is_none());
    assert_eq!(s.editor.project(), &before);
}

fn begin_insertion(pen: &mut Pen, s: &EditorState, closing: bool) {
    let (_, path, world) = current(s);
    let segment = if closing { path.vertices.len() - 1 } else { 0 };
    let point = curve(
        &path.vertices[segment],
        &path.vertices[(segment + 1) % path.vertices.len()],
        0.5,
    );
    assert!(
        pen.down(s, world.point(point), 1., false, false, false)
            .is_none()
    );
    assert_eq!(selected(pen), [segment + 1]);
    assert!(pen.pending(s).is_some());
}
#[test]
fn delete_during_insertion_cancels_transient_vertex_without_deleting_a_source_vertex() {
    for kind in 0..3 {
        for closing in [false, true] {
            for key in ["delete", "backspace"] {
                let mut s = single_path_scene(kind);
                redo_available(&mut s);
                let before = s.editor.project().clone();
                let mut pen = Pen::default();
                begin_insertion(&mut pen, &s, closing);
                assert!(matches!(pen.key(key, &s), (true, None)));
                assert!(selected(&pen).is_empty());
                assert!(pen.pending(&s).is_none());
                assert!(pen.up(&s).is_none());
                assert!(matches!(pen.key(key, &s), (true, None)));
                assert_eq!(s.editor.project(), &before);
                assert!(!s.editor.can_undo());
                assert!(s.editor.can_redo());
            }
        }
    }
}
#[test]
fn repeated_down_abandons_inserted_indices_before_empty_shift_click_and_delete() {
    for kind in 0..3 {
        for closing in [false, true] {
            let s = single_path_scene(kind);
            let before = s.editor.project().clone();
            let mut pen = Pen::default();
            begin_insertion(&mut pen, &s, closing);
            pen.down(&s, [500., 500.], 1., false, true, false);
            assert!(selected(&pen).is_empty());
            assert!(matches!(pen.key("delete", &s), (true, None)));
            assert!(pen.pending(&s).is_none());
            assert!(pen.up(&s).is_none());
            assert_eq!(s.editor.project(), &before);
        }
    }
}
#[test]
fn invalid_insertion_release_and_defensive_selection_bounds_keep_delete_safe() {
    let s = scene(true);
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    begin_insertion(&mut pen, &s, true);
    pen.moving([1e20, 1e20], false, false);
    assert!(pen.up(&s).is_none());
    assert!(selected(&pen).is_empty());
    assert!(matches!(pen.key("delete", &s), (true, None)));
    // Defense in depth: a malformed cached index must be rejected atomically.
    pen.selected.as_mut().unwrap().vertices = [0, 6].into();
    assert!(matches!(pen.key("delete", &s), (true, None)));
    assert!(selected(&pen).is_empty());
    assert_eq!(s.editor.project(), &before);
}

#[test]
fn release_position_overrides_last_move_and_returning_to_start_does_not_add_a_key() {
    let mut s = scene(true);
    animate(&mut s);
    redo_available(&mut s);
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let (_, world, pointer) = begin(&mut pen, &s, [2., 1.]);
    pen.moving(world.point(add(pointer, [50., 20.])), false, false);
    assert!(pen.pending(&s).is_some());
    assert!(
        pen.release(&s, world.point(pointer), false, false)
            .is_none()
    );
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
    assert_eq!(
        s.editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::Path(target(&s)))
            .unwrap()
            .keys()
            .len(),
        2
    );
}
#[test]
fn release_time_shift_and_alt_modifiers_are_applied_to_the_final_pose() {
    let mut s = scene(true);
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let (original, world, pointer) = begin(&mut pen, &s, [2., 1.]);
    pen.moving(world.point(add(pointer, [20., 30.])), false, false);
    let command = pen
        .release(&s, world.point(add(pointer, [40., 10.])), false, true)
        .unwrap();
    s.editor.execute(command).unwrap();
    assert_move(&original, &current(&s).1, &[0, 1], [40., 0.]);
    // Release-time Alt must also break a handle without changing the opposite one.
    let mut path = current(&s).1;
    path.vertices[0].incoming = [-30., -15.];
    path.vertices[0].outgoing = [30., 15.];
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target: target(&s),
            frame: 0,
            path,
        })
        .unwrap();
    let original = current(&s).1;
    let at = original.vertices[0].position;
    pen.down(
        &s,
        world.point(add(at, [30., 15.])),
        1.,
        false,
        false,
        false,
    );
    pen.moving(world.point(add(at, [50., 25.])), false, false);
    let command = pen
        .release(&s, world.point(add(at, [60., 30.])), true, false)
        .unwrap();
    s.editor.execute(command).unwrap();
    let changed = current(&s).1;
    assert_eq!(changed.vertices[0].outgoing, [60., 30.]);
    assert_eq!(changed.vertices[0].incoming, original.vertices[0].incoming);
    assert_eq!(changed.vertices[1..], original.vertices[1..]);
}
