//! Model-only acceptance. Native pointer, Tab/Enter and IME delivery remain deferred.
use super::{gradient_gesture, *};
use crate::{color_edit::GradientTarget, panels::pen};
use libre_effects_core::{
    Content, ContentsEdit, ContentsKind, ContentsParam, GradientParam, PathTarget, PathVertex,
    ShapeContents, TrackEdit, VectorPath,
};

fn edit(s: &mut EditorState, edit: ContentsEdit) {
    s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn scene() -> EditorState {
    let mut s = EditorState::default();
    s.composition_started = true;
    s.editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents::default()),
            width: 200.,
            height: 120.,
            name: "Cross-parent gestures".into(),
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
    edit(
        &mut s,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Path {
                path: VectorPath {
                    vertices: [[10., 20.], [50., 20.], [50., 60.], [10., 60.]]
                        .map(PathVertex::corner)
                        .into(),
                    closed: true,
                },
                animation: Default::default(),
            },
        },
    );
    edit(
        &mut s,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::GradientFill {
                even_odd: false,
                gradient: Default::default(),
            },
        },
    );
    for (property, value) in [
        (Property::PositionX, 45.),
        (Property::PositionY, 20.),
        (Property::ScaleX, -150.),
        (Property::ScaleY, 80.),
    ] {
        edit(
            &mut s,
            ContentsEdit::Track {
                item: 2,
                parameter: ContentsParam::Transform(property),
                edit: TrackEdit::Value { frame: 0, value },
            },
        );
    }
    s.selected_layers.insert(1);
    s.editor.clear_history();
    s
}
fn contents(s: &EditorState) -> &ShapeContents {
    let Content::ShapeContents(c) = s.editor.selected_layer().unwrap().content() else {
        panic!()
    };
    c
}
fn select(s: &mut EditorState, item: u64) {
    s.contents_selection = Some((s.editor.project().active_composition_id(), 1, item));
}
fn move_items(s: &mut EditorState, items: Vec<u64>) {
    edit(
        s,
        ContentsEdit::MoveSiblings {
            source_parent: 1,
            items,
            parent: 2,
            index: 0,
        },
    );
}
fn bounds() -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(0.)), size(px(1000.), px(1000.)))
}
fn pointer(p: [f64; 2]) -> Point<Pixels> {
    point(px(p[0] as f32), px(p[1] as f32))
}
fn view(s: &EditorState) -> pen::View {
    pen::View::new(bounds(), point(px(0.), px(0.)), 1., s)
}

#[test]
fn moved_pen_target_discards_pending_gesture_and_new_overlay_uses_new_transform_chain() {
    let mut s = scene();
    s.tool = Tool::Pen;
    select(&mut s, 3);
    let identity = s.contents_selection;
    let mut pen = pen::Pen::default();
    let before_overlay = pen.overlay(&s);
    let (path, old_world, _, _) = before_overlay.first().unwrap();
    let local = path.vertices[0].position;
    let origin = old_world.point(local);
    assert!(
        pen.pointer_down(&s, pointer(origin), Some(view(&s)), Default::default())
            .is_none()
    );
    pen.pointer_move(
        &s,
        pointer([origin[0] + 12., origin[1] + 8.]),
        Some(view(&s)),
        Default::default(),
    );
    assert!(pen.pending(&s).is_some());
    let original_node = contents(&s).node(3).unwrap().clone();
    move_items(&mut s, vec![3]);
    assert_eq!(contents(&s).node(3), Some(&original_node));
    assert_eq!(s.contents_selection, identity);
    assert!(pen.pending(&s).is_none());
    pen.reset_if_stale(&s);
    let overlay = pen.overlay(&s);
    let (new_path, new_world, _, vertices) = overlay.first().unwrap();
    assert_eq!(new_path, path);
    assert!(vertices.is_empty()); // No transient vertex selection is fabricated from a tree row.
    let expected_world = s
        .editor
        .project()
        .composition()
        .world_transform(1, 0)
        .unwrap()
        .compose(contents(&s).group_transform(2, 0).unwrap());
    assert_eq!(*new_world, expected_world);
    assert_ne!(*new_world, *old_world);
    assert!(
        pen.pointer_up(&s, pointer(origin), Some(view(&s)), Default::default())
            .is_none()
    );
    let origin = new_world.point(local);
    pen.pointer_down(&s, pointer(origin), Some(view(&s)), Default::default());
    pen.pointer_move(
        &s,
        pointer([origin[0] + 12., origin[1] + 8.]),
        Some(view(&s)),
        Default::default(),
    );
    assert!(matches!(
        pen.pending(&s),
        Some(Command::EditPath {
            id: 1,
            target: PathTarget::Contents(3),
            ..
        })
    ));
    assert_eq!(s.contents_selection, identity);
}

#[test]
fn moved_gradient_keeps_identity_rejects_old_drag_and_rebuilds_local_space_overlay() {
    let mut s = scene();
    s.tool = Tool::Select;
    select(&mut s, 4);
    let target = GradientTarget::Contents(s.editor.project().active_composition_id(), 1, 4);
    s.gradient_controls = Some(target);
    let overlay = gradient_gesture::Overlay::current(&s, s.editor.project()).unwrap();
    let old_world = s
        .editor
        .project()
        .composition()
        .world_transform(1, 0)
        .unwrap()
        .compose(contents(&s).group_transform(1, 0).unwrap());
    let origin = old_world.point(overlay.points[0]);
    let mut gesture = gradient_gesture::Gesture::new(overlay.clone(), 0, origin, bounds(), &s);
    gesture.update([origin[0] + 18., origin[1] + 12.], false, false);
    assert!(gesture.command(&s).is_some());
    let original = contents(&s).node(4).unwrap().clone();
    move_items(&mut s, vec![4]);
    assert_eq!(contents(&s).node(4), Some(&original));
    assert_eq!(s.gradient_controls, Some(target));
    assert_eq!(s.contents_selection, Some((target.composition(), 1, 4)));
    assert!(!gesture.valid(&s));
    assert!(gesture.command(&s).is_none());
    let fresh = gradient_gesture::Overlay::current(&s, s.editor.project()).unwrap();
    assert_eq!(fresh.points, overlay.points);
    let world = s
        .editor
        .project()
        .composition()
        .world_transform(1, 0)
        .unwrap()
        .compose(contents(&s).group_transform(2, 0).unwrap());
    assert_ne!(world, old_world);
    let new_origin = world.point(fresh.points[0]);
    assert_eq!(fresh.hit(new_origin, 1., 0), Some(0));
    let mut new_gesture =
        gradient_gesture::Gesture::new(fresh.clone(), 0, new_origin, bounds(), &s);
    let local = [fresh.points[0][0] + 20., fresh.points[0][1] + 15.];
    new_gesture.update(world.point(local), false, false);
    assert!((new_gesture.points[0][0] - local[0]).abs() < 1e-9);
    assert!((new_gesture.points[0][1] - local[1]).abs() < 1e-9);
    let command = new_gesture.command(&s).unwrap();
    let Command::Batch(commands) = command else {
        panic!()
    };
    assert!(commands.iter().all(|command| matches!(
        command,
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 4,
                parameter: ContentsParam::Gradient(GradientParam::StartX | GradientParam::StartY),
                ..
            }
        }
    )));
    // History changes invalidate in-flight gestures without changing stable target IDs.
    s.editor.undo();
    assert!(!new_gesture.valid(&s));
    assert_eq!(s.gradient_controls, Some(target));
    assert_eq!(
        gradient_gesture::Overlay::current(&s, s.editor.project())
            .unwrap()
            .hit(origin, 1., 0),
        Some(0)
    );
    s.editor.redo();
    assert_eq!(
        gradient_gesture::Overlay::current(&s, s.editor.project())
            .unwrap()
            .hit(new_origin, 1., 0),
        Some(0)
    );
}

#[test]
fn singular_destination_moves_local_values_but_does_not_invent_editable_overlays() {
    let mut s = scene();
    edit(
        &mut s,
        ContentsEdit::Track {
            item: 2,
            parameter: ContentsParam::Transform(Property::ScaleX),
            edit: TrackEdit::Value {
                frame: 0,
                value: 0.,
            },
        },
    );
    let path = contents(&s).node(3).unwrap().clone();
    let gradient = contents(&s).node(4).unwrap().clone();
    move_items(&mut s, vec![3, 4]);
    assert_eq!(contents(&s).node(3), Some(&path));
    assert_eq!(contents(&s).node(4), Some(&gradient));
    s.tool = Tool::Pen;
    select(&mut s, 3);
    assert!(pen::Pen::default().overlay(&s).is_empty());
    s.tool = Tool::Select;
    select(&mut s, 4);
    s.gradient_controls = Some(GradientTarget::Contents(
        s.editor.project().active_composition_id(),
        1,
        4,
    ));
    assert!(gradient_gesture::Overlay::current(&s, s.editor.project()).is_none());
}
