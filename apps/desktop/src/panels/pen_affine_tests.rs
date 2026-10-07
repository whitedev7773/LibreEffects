use super::*;
use crate::editor::Action;
use libre_effects_core::{ContentsParam, Property, ShapeContents, TrackEdit};

fn event(chord: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: gpui::Keystroke::parse(chord).unwrap(),
        is_held: false,
    }
}
fn edit(s: &mut EditorState, edit: ContentsEdit) {
    s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn group_value(s: &mut EditorState, item: u64, parameter: ContentsParam, value: f64) {
    edit(
        s,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value { frame: 0, value },
        },
    );
}
fn polygon() -> VectorPath {
    VectorPath {
        closed: true,
        vertices: vec![
            PathVertex {
                position: [20., 20.],
                incoming: [-8., 3.],
                outgoing: [12., -5.],
            },
            PathVertex::corner([100., 20.]),
            PathVertex::corner([100., 100.]),
            PathVertex::corner([20., 100.]),
        ],
    }
}
fn scene(transformed: bool) -> EditorState {
    let mut s = EditorState::default();
    s.tool = Tool::Pen;
    s.composition_started = true;
    let comp = s.editor.project().composition();
    s.editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents::default()),
            width: comp.width() as f64,
            height: comp.height() as f64,
            name: "Affine Contents".into(),
        })
        .unwrap();
    for _ in 0..2 {
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 0,
                kind: ContentsKind::Group(vec![]),
            },
        );
    }
    for parent in [1, 2] {
        edit(
            &mut s,
            ContentsEdit::Add {
                parent,
                kind: ContentsKind::Path {
                    path: polygon(),
                    animation: Default::default(),
                },
            },
        );
    }
    edit(
        &mut s,
        ContentsEdit::Add {
            parent: 2,
            kind: ContentsKind::Group(vec![]),
        },
    ); //5 nested
    edit(
        &mut s,
        ContentsEdit::Move {
            item: 4,
            parent: 5,
            index: 0,
        },
    );
    group_value(
        &mut s,
        1,
        ContentsParam::Transform(Property::PositionX),
        200.,
    );
    group_value(
        &mut s,
        2,
        ContentsParam::Transform(Property::PositionX),
        500.,
    );
    if transformed {
        group_value(&mut s, 1, ContentsParam::Transform(Property::ScaleX), -100.);
        group_value(&mut s, 1, ContentsParam::Skew, 23.);
        group_value(&mut s, 5, ContentsParam::Transform(Property::Rotation), 31.);
        group_value(&mut s, 5, ContentsParam::Transform(Property::ScaleY), 70.);
    }
    s.selected_layers = [1].into();
    s.frame = 30;
    s.editor.clear_history();
    s
}
fn path(s: &EditorState, item: u64) -> (VectorPath, Affine) {
    let (_, path, world) = paths(s)
        .into_iter()
        .find(|(target, _, _)| *target == Target::Contents(1, item))
        .unwrap();
    (path, world)
}
fn anchor(s: &EditorState, item: u64, index: usize) -> [f64; 2] {
    let (path, world) = path(s, item);
    world.point(path.vertices[index].position)
}
fn pick(pen: &mut Pen, s: &EditorState, item: u64, index: usize, shift: bool) {
    let p = anchor(s, item, index);
    assert!(pen.down(s, p, 1., false, shift, false).is_none());
    assert!(pen.release(s, p, false, shift).is_none());
}
fn pair(pen: &mut Pen, s: &EditorState) {
    pick(pen, s, 3, 0, false);
    pick(pen, s, 4, 0, true);
    assert!(pen.selected.as_ref().unwrap().cross_path());
}
fn near(actual: [f64; 2], expected: [f64; 2]) {
    assert!(
        distance(actual, expected) < 1e-8,
        "{actual:?} != {expected:?}"
    );
}
fn dispatch(pen: &mut Pen, s: &mut EditorState, command: Command) {
    s.bulk_test_action(&Action::Edit(command));
    assert_eq!(s.status, "Edited");
    pen.did_commit(s);
    assert!(pen.selected_context.as_ref().unwrap().valid(s));
}

#[test]
fn affine_cross_path_move_is_world_aligned_nonaccumulating_and_one_current_frame_command() {
    let mut s = scene(true);
    s.editor
        .execute(Command::AnimatePath {
            id: 1,
            target: PathTarget::Contents(4),
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    let before_path = path(&s, 4).0;
    let mut last = before_path.clone();
    for vertex in &mut last.vertices {
        vertex.position[1] += 50.;
    }
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target: PathTarget::Contents(4),
            frame: 60,
            path: last,
        })
        .unwrap();
    s.editor.clear_history();
    let before = s.editor.project().clone();
    let source = [path(&s, 3), path(&s, 4)];
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let start = anchor(&s, 4, 0);
    pen.down(&s, start, 1., false, false, false);
    pen.moving(add(start, [2., 3.]), false, false);
    pen.moving(add(start, [8., 9.]), false, false);
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
    let command = pen
        .release(&s, add(start, [40., -15.]), false, true)
        .unwrap();
    match &command {
        Command::TransformContentsPoints {
            id,
            frame,
            selections,
            transform,
        } => {
            assert_eq!((*id, *frame), (1, 30));
            assert_eq!(selections.len(), 2);
            near(transform.translation, [40., 0.]);
        }
        _ => panic!("Expected one cross-path command"),
    }
    dispatch(&mut pen, &mut s, command);
    for (item, (original, world)) in [3, 4].into_iter().zip(source) {
        let updated = path(&s, item).0;
        near(
            world.point(updated.vertices[0].position),
            add(world.point(original.vertices[0].position), [40., 0.]),
        );
        assert_eq!(&updated.vertices[1..], &original.vertices[1..]);
        near(
            world.vector(updated.vertices[0].incoming),
            world.vector(original.vertices[0].incoming),
        );
        near(
            world.vector(updated.vertices[0].outgoing),
            world.vector(original.vertices[0].outgoing),
        );
    }
    let after = s.editor.project().clone();
    s.bulk_test_action(&Action::Undo);
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
    s.bulk_test_action(&Action::Redo);
    assert_eq!(s.editor.project(), &after);
}

#[test]
fn affine_box_signed_nonuniform_scale_transforms_tangents_around_frozen_center() {
    let mut s = scene(true);
    let mut pen = Pen::default();
    assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
    pen.toggle_transform(&s);
    assert!(pen.transform_enabled());
    let bounds = pen.transform_overlay(&s, 1.).unwrap();
    let original = [path(&s, 3), path(&s, 4)];
    let before = s.editor.project().clone();
    let start = bounds.corners[2];
    let delta = sub(start, bounds.pivot);
    pen.down(&s, start, 1., false, false, false);
    assert!(pen.affine.is_some());
    let finish = add(bounds.pivot, [-2. * delta[0], 3. * delta[1]]);
    let command = pen.release(&s, finish, false, false).unwrap();
    assert_eq!(s.editor.project(), &before);
    dispatch(&mut pen, &mut s, command);
    for (item, (source, world)) in [3, 4].into_iter().zip(original) {
        let result = path(&s, item).0;
        for (old, new) in source.vertices.iter().zip(result.vertices) {
            let p = world.point(old.position);
            near(
                world.point(new.position),
                add(
                    bounds.pivot,
                    [
                        -2. * (p[0] - bounds.pivot[0]),
                        3. * (p[1] - bounds.pivot[1]),
                    ],
                ),
            );
            for (old, new) in [(old.incoming, new.incoming), (old.outgoing, new.outgoing)] {
                let v = world.vector(old);
                near(world.vector(new), [-2. * v[0], 3. * v[1]]);
            }
        }
    }
}

#[test]
fn affine_rotate_uses_final_pointer_and_shift_snaps_fifteen_degrees() {
    for shift in [false, true] {
        let mut s = scene(true);
        let mut pen = Pen::default();
        pen.select_all_key(&event("ctrl-a"), true, false, &s);
        pen.toggle_transform(&s);
        let bounds = pen.transform_overlay(&s, 1.).unwrap();
        let originals = [path(&s, 3), path(&s, 4)];
        let start = bounds.rotate;
        let delta = sub(start, bounds.pivot);
        let angle: f64 = if shift { 83. } else { 90. };
        let (sin, cos) = angle.to_radians().sin_cos();
        let finish = add(
            bounds.pivot,
            [
                cos * delta[0] - sin * delta[1],
                sin * delta[0] + cos * delta[1],
            ],
        );
        pen.down(&s, start, 1., false, false, false);
        let command = pen.release(&s, finish, false, shift).unwrap();
        dispatch(&mut pen, &mut s, command);
        for (item, (source, world)) in [3, 4].into_iter().zip(originals) {
            let result = path(&s, item).0;
            for (old, new) in source.vertices.iter().zip(result.vertices) {
                let d = sub(world.point(old.position), bounds.pivot);
                near(world.point(new.position), add(bounds.pivot, [-d[1], d[0]]));
                let v = world.vector(old.outgoing);
                near(world.vector(new.outgoing), [-v[1], v[0]]);
            }
        }
    }
}

#[test]
fn affine_uniform_scale_and_return_to_start_preserve_exact_source_and_history() {
    for handle in 0..3 {
        let s = scene(true);
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        pair(&mut pen, &s);
        if handle > 0 {
            pen.toggle_transform(&s);
        }
        let bounds = pen.transform_overlay(&s, 1.);
        let start = match handle {
            0 => anchor(&s, 3, 0),
            1 => bounds.unwrap().corners[2],
            _ => bounds.unwrap().rotate,
        };
        pen.down(&s, start, 1., false, false, false);
        pen.moving(add(start, [50., 20.]), false, true);
        assert!(pen.pending(&s).is_some());
        assert!(pen.release(&s, start, false, true).is_none());
        assert!(pen.pending(&s).is_none());
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
    let s = scene(false);
    let mut pen = Pen::default();
    pen.select_all_key(&event("ctrl-a"), true, false, &s);
    pen.toggle_transform(&s);
    let bounds = pen.transform_overlay(&s, 1.).unwrap();
    let start = bounds.corners[2];
    let d = sub(start, bounds.pivot);
    pen.down(&s, start, 1., false, false, false);
    let command = pen
        .release(
            &s,
            add(bounds.pivot, [d[0] * 1.5, d[1] * 1.25]),
            false,
            true,
        )
        .unwrap();
    let Command::TransformContentsPoints { transform, .. } = command else {
        panic!()
    };
    assert_eq!(transform.scale_percent, [150., 150.]);
}

#[test]
fn affine_selection_initializes_domain_and_excludes_disabled_singular_and_masks() {
    let mut s = scene(false);
    for _ in 0..2 {
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 0,
                kind: ContentsKind::Group(vec![]),
            },
        );
    } //6,7
    for parent in [6, 7] {
        edit(
            &mut s,
            ContentsEdit::Add {
                parent,
                kind: ContentsKind::Path {
                    path: polygon(),
                    animation: Default::default(),
                },
            },
        );
    } //8,9
    edit(
        &mut s,
        ContentsEdit::Enabled {
            item: 6,
            enabled: false,
        },
    );
    group_value(&mut s, 7, ContentsParam::Transform(Property::ScaleX), 0.);
    s.editor
        .execute(Command::SetPathMasks {
            id: 1,
            masks: vec![PathMask {
                path: polygon(),
                ..Default::default()
            }],
        })
        .unwrap();
    s.editor.clear_history();
    let before = s.editor.project().clone();
    for marquee in [false, true] {
        let mut pen = Pen::default();
        if marquee {
            pen.down(&s, [-500., -500.], 1., false, true, false);
            assert!(pen.marquee.is_some());
            assert!(pen.selected.as_ref().unwrap().vertices.is_empty());
            pen.moving([1500., 1500.], false, true);
            assert!(pen.selected.as_ref().unwrap().vertices.is_empty());
            assert!(pen.release(&s, [1500., 1500.], false, true).is_none());
        } else {
            assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
        }
        let (_, selected) = pen.selected.as_ref().unwrap().contents().unwrap();
        assert_eq!(selected.keys().copied().collect::<Vec<_>>(), [3, 4]);
        assert!(
            selected
                .values()
                .all(|indices| indices == &[0, 1, 2, 3].into())
        );
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
}

#[test]
fn affine_unavailable_cross_path_numeric_topology_and_modifiers_do_not_fall_through() {
    let s = scene(false);
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    assert!(!pen.numeric_vertex_available(&s));
    assert!(pen.numeric_vertex_request(&s).is_none());
    for chord in ["shift-r", "shift-f"] {
        let (handled, command) = pen.order_key(&event(chord), true, false, &s);
        assert!(handled && command.is_none());
    }
    for key in ["delete", "backspace"] {
        let (handled, command) = pen.key(key, &s);
        assert!(handled && command.is_none());
    }
    assert!(pen.numeric_vertex_key(&event("shift-v"), true, false, &s).0);
    let selection = pen.selected.clone();
    pen.down(&s, [260., 20.], 1., false, false, false); // curve insertion is blocked for cross selection
    assert!(pen.draft.is_none() && pen.drag.is_none());
    assert!(pen.up(&s).is_none());
    assert!(pen.selected == selection);
    pen.toggle_transform(&s);
    let mut held = event("shift-t");
    held.is_held = true;
    assert!(pen.transform_key(&held, true, false, &s));
    assert!(pen.transform_enabled());
    for (chord, focused, composing) in [
        ("shift-t", false, false),
        ("shift-t", true, true),
        ("ctrl-shift-t", true, false),
    ] {
        assert!(!pen.transform_key(&event(chord), focused, composing, &s));
        assert!(pen.transform_enabled());
    }
}

#[test]
fn affine_singleton_and_flat_boxes_are_visible_and_do_not_invent_geometry() {
    let s = scene(false);
    let mut pen = Pen::default();
    pick(&mut pen, &s, 3, 0, false);
    pen.toggle_transform(&s);
    let bounds = pen.transform_overlay(&s, 2.).unwrap();
    let selected = anchor(&s, 3, 0);
    assert_eq!(bounds.pivot, selected);
    assert_eq!(bounds.corners[0], sub(selected, [6., 6.]));
    pen.down(&s, bounds.corners[2], 2., false, false, false);
    assert!(
        pen.release(&s, add(selected, [12., 12.]), false, true)
            .is_some()
    ); // tangents scale; anchor stays fixed
    let overlay = affine::BoxOverlay::new([[10., 20.], [50., 20.]].into_iter(), 1.).unwrap();
    assert_eq!(overlay.pivot, [30., 20.]);
    assert_eq!(overlay.corners[0], [10., 8.]);
}

#[test]
fn affine_render_generation_is_unique_and_retires_with_interruption() {
    let s = scene(false);
    let mut pen = Pen::default();
    pair(&mut pen, &s);
    let start = anchor(&s, 3, 0);
    pen.down(&s, start, 1., false, false, false);
    let first = pen.render_generation(&s).unwrap();
    pen.moving(add(start, [20., 10.]), false, false);
    assert_eq!(pen.render_generation(&s), Some(first));
    pen.abandon_pointer();
    assert_eq!(pen.render_generation(&s), None);
    pen.down(&s, start, 1., false, false, false);
    assert_ne!(pen.render_generation(&s), Some(first));
    assert!(pen.release(&s, start, false, false).is_none());
    assert_eq!(pen.render_generation(&s), None);
}

#[test]
fn affine_idle_view_actions_preserve_selection_and_between_point_creation_but_retire_held() {
    for action in [
        Action::ZoomPreview(2.),
        Action::FitPreview,
        Action::Checkerboard,
        Action::ViewerOption(crate::viewer_tools::ViewOption::Rulers),
        Action::PreviewChannel(crate::viewer_tools::Channel::Alpha),
    ] {
        let mut s = scene(false);
        let mut pen = Pen::default();
        pair(&mut pen, &s);
        let selection = pen.selected.clone();
        let input = s.input_context_generation();
        s.bulk_test_action(&action);
        assert!(s.input_context_generation() > input);
        pen.reset_if_stale(&s);
        assert!(pen.selected == selection);
        let start = anchor(&s, 3, 0);
        pen.down(&s, start, 1., false, false, false);
        pen.moving(add(start, [15., 10.]), false, false);
        assert!(pen.pending(&s).is_some());
        s.bulk_test_action(&action);
        assert!(pen.pending(&s).is_none());
        assert!(
            pen.release(&s, add(start, [20., 10.]), false, false)
                .is_none()
        );
        let mut pen = Pen::default();
        pen.down(&s, [1100., 900.], 1., false, false, false);
        pen.up(&s);
        assert!(pen.draft.is_some());
        s.bulk_test_action(&action);
        pen.reset_if_stale(&s);
        assert!(pen.draft.is_some());
    }
}

#[test]
fn affine_held_single_path_delete_abandons_without_publishing_topology() {
    let s = scene(false);
    let before = s.editor.project().clone();
    let mut pen = Pen::default();
    pick(&mut pen, &s, 3, 0, false);
    pen.toggle_transform(&s);
    let start = pen.transform_overlay(&s, 1.).unwrap().rotate;
    pen.down(&s, start, 1., false, false, false);
    pen.moving(add(start, [30., 20.]), false, false);
    assert!(pen.pending(&s).is_some());
    let (handled, command) = pen.key("delete", &s);
    assert!(handled && command.is_none());
    assert!(pen.affine.is_none());
    assert!(
        pen.release(&s, add(start, [30., 20.]), false, false)
            .is_none()
    );
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
}
