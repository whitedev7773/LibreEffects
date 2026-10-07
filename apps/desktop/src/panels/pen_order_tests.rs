use super::*;
use libre_effects_core::{ContentsParam, Property, TrackEdit};

fn event(chord: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: gpui::Keystroke::parse(chord).unwrap(),
        is_held: false,
    }
}
fn path(closed: bool) -> VectorPath {
    VectorPath {
        closed,
        vertices: [
            [30., 40.],
            [170., 40.],
            [230., 160.],
            [150., 240.],
            [30., 180.],
        ]
        .into_iter()
        .map(PathVertex::corner)
        .collect(),
    }
}
fn scene(kind: usize, closed: bool, animated: bool) -> (EditorState, Target, PathTarget) {
    let mut s = EditorState::default();
    s.tool = Tool::Pen;
    s.composition_started = true;
    let comp = s.editor.project().composition();
    s.editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(path(closed)),
                fill: closed,
                ..Default::default()
            }),
            width: comp.width() as f64,
            height: comp.height() as f64,
            name: "Path order".into(),
        })
        .unwrap();
    let (target, core) = match kind {
        0 => (Target::Shape(1), PathTarget::Shape),
        1 => {
            s.editor
                .execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::Promote,
                })
                .unwrap();
            s.editor
                .execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::Track {
                        item: 1,
                        parameter: ContentsParam::Skew,
                        edit: TrackEdit::Value {
                            frame: 0,
                            value: 24.,
                        },
                    },
                })
                .unwrap();
            s.editor
                .execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::Track {
                        item: 1,
                        parameter: ContentsParam::SkewAxis,
                        edit: TrackEdit::Value {
                            frame: 0,
                            value: 39.,
                        },
                    },
                })
                .unwrap();
            (Target::Contents(1, 2), PathTarget::Contents(2))
        }
        _ => {
            let mut shifted = path(true);
            for v in &mut shifted.vertices {
                v.position[0] += 500.;
            }
            s.editor
                .execute(Command::EditPath {
                    id: 1,
                    target: PathTarget::Shape,
                    frame: 0,
                    path: shifted,
                })
                .unwrap();
            s.editor
                .execute(Command::SetPathMasks {
                    id: 1,
                    masks: vec![
                        PathMask {
                            path: path(true),
                            ..Default::default()
                        },
                        PathMask {
                            path: path(true),
                            ..Default::default()
                        },
                    ],
                })
                .unwrap();
            let masks = s.editor.selected_layer().unwrap().path_masks()[1..].to_vec();
            s.editor
                .execute(Command::SetPathMasks { id: 1, masks })
                .unwrap();
            (Target::Mask(1, 0), PathTarget::Mask(2))
        }
    };
    s.editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::Rotation,
            frame: 0,
            value: 13.,
        })
        .unwrap();
    if animated {
        s.editor
            .execute(Command::AnimatePath {
                id: 1,
                target: core,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
        let mut next = geometry(&s, target).0;
        for v in &mut next.vertices {
            v.position = add(v.position, [20., 35.]);
        }
        s.editor
            .execute(Command::EditPath {
                id: 1,
                target: core,
                frame: 20,
                path: next,
            })
            .unwrap();
        s.frame = 10;
    }
    s.editor.clear_history();
    (s, target, core)
}
fn geometry(s: &EditorState, target: Target) -> (VectorPath, Affine) {
    let (_, path, world) = paths(s).into_iter().find(|(t, _, _)| *t == target).unwrap();
    (path, world)
}
fn pick(pen: &mut Pen, s: &EditorState, target: Target, index: usize, shift: bool) {
    let (path, world) = geometry(s, target);
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
fn apply(pen: &mut Pen, s: &mut EditorState, chord: &str) -> Command {
    let (handled, command) = pen.order_key(&event(chord), true, false, s);
    assert!(handled);
    let command = command.expect("accepted reorder");
    s.editor.execute(command.clone()).unwrap();
    pen.reset_if_stale(s);
    command
}
fn redo(s: &mut EditorState) {
    s.editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 40.,
        })
        .unwrap();
    s.editor.undo();
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
}

#[test]
fn first_and_reverse_remap_geometric_selection_for_every_static_and_animated_target() {
    for kind in 0..3 {
        for animated in [false, true] {
            let (mut s, target, core) = scene(kind, true, animated);
            let original = s.editor.project().clone();
            let (before, world) = geometry(&s, target);
            let mut pen = Pen::default();
            pick(&mut pen, &s, target, 3, false);
            let command = apply(&mut pen, &mut s, "shift-f");
            assert!(
                matches!(command, Command::ReorderPath { id: 1, target, order: PathOrder::FirstVertex(3) } if target == core)
            );
            assert_eq!(selected(&pen), [0]);
            assert_eq!(geometry(&s, target).0.vertices[0], before.vertices[3]);
            assert_eq!(geometry(&s, target).1, world);
            assert_eq!(
                pen.overlay(&s)
                    .iter()
                    .find(|(p, _, _, _)| p.vertices[0] == before.vertices[3])
                    .unwrap()
                    .3,
                [0].into()
            );
            let after_first = s.editor.project().clone();
            apply(&mut pen, &mut s, "shift-r");
            assert_eq!(selected(&pen), [0]);
            apply(&mut pen, &mut s, "shift-r");
            assert_eq!(s.editor.project(), &after_first);
            s.editor.undo();
            s.editor.undo();
            s.editor.undo();
            assert_eq!(s.editor.project(), &original);
            assert!(!s.editor.can_undo());
            s.editor.redo();
            assert_eq!(s.editor.project(), &after_first);
        }
    }
}

#[test]
fn reverse_multiselection_then_drag_moves_the_same_vertices_in_each_target() {
    for kind in 0..3 {
        let (mut s, target, _) = scene(kind, true, true);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 1, false);
        pick(&mut pen, &s, target, 3, true);
        let before = geometry(&s, target).0;
        apply(&mut pen, &mut s, "shift-r");
        assert_eq!(selected(&pen), [2, 4]);
        let (reversed, world) = geometry(&s, target);
        assert_eq!(reversed.vertices[2].position, before.vertices[3].position);
        assert_eq!(reversed.vertices[4].position, before.vertices[1].position);
        let pointer = reversed.vertices[2].position;
        assert!(
            pen.down(&s, world.point(pointer), 1., false, false, false)
                .is_none()
        );
        let command = pen
            .release(&s, world.point(add(pointer, [12., 27.])), false, false)
            .unwrap();
        s.editor.execute(command).unwrap();
        let moved = geometry(&s, target).0;
        for (i, v) in moved.vertices.iter().enumerate() {
            let expected = if [2, 4].contains(&i) {
                add(reversed.vertices[i].position, [12., 27.])
            } else {
                reversed.vertices[i].position
            };
            assert!(distance(v.position, expected) < 1e-8);
            assert_eq!(v.incoming, reversed.vertices[i].incoming);
            assert_eq!(v.outgoing, reversed.vertices[i].outgoing);
        }
    }
}

#[test]
fn every_closed_first_index_maps_the_selected_vertex_to_zero() {
    for index in 0..5 {
        let (mut s, target, _) = scene(0, true, false);
        let original = geometry(&s, target).0;
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, index, false);
        apply(&mut pen, &mut s, "shift-f");
        assert_eq!(geometry(&s, target).0.vertices[0], original.vertices[index]);
        assert_eq!(selected(&pen), [0]);
        assert_eq!(s.editor.can_undo(), index != 0);
    }
}

#[test]
fn open_paths_reject_first_without_wrapping_and_reverse_selected_endpoints() {
    for kind in 0..2 {
        let (mut s, target, _) = scene(kind, false, true);
        redo(&mut s);
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 1, false);
        let (handled, command) = pen.order_key(&event("shift-f"), true, false, &s);
        assert!(handled && command.is_none());
        assert_eq!(selected(&pen), [1]);
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo() && s.editor.can_redo());
        assert!(pen.order_help(&s).contains("switches endpoints"));
        pick(&mut pen, &s, target, 0, false);
        pick(&mut pen, &s, target, 4, true);
        apply(&mut pen, &mut s, "shift-r");
        assert_eq!(selected(&pen), [0, 4]);
        assert!(!geometry(&s, target).0.closed);
    }
}

#[test]
fn first_zero_is_exact_noop_preserving_redo_and_selection_context() {
    for kind in 0..3 {
        let (mut s, target, _) = scene(kind, true, true);
        redo(&mut s);
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 0, false);
        apply(&mut pen, &mut s, "shift-f");
        assert_eq!(selected(&pen), [0]);
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo() && s.editor.can_redo());
        assert!(pen.selected_context.as_ref().unwrap().valid(&s));
    }
}

#[test]
fn first_requires_single_selection_and_order_never_chooses_an_arbitrary_path() {
    let (s, target, _) = scene(0, true, false);
    let mut pen = Pen::default();
    for chord in ["shift-f", "shift-r"] {
        let (handled, command) = pen.order_key(&event(chord), true, false, &s);
        assert!(handled && command.is_none());
    }
    pick(&mut pen, &s, target, 1, false);
    pick(&mut pen, &s, target, 2, true);
    assert!(
        pen.order_key(&event("shift-f"), true, false, &s)
            .1
            .is_none()
    );
    assert_eq!(selected(&pen), [1, 2]);
    for vertices in [BTreeSet::new(), [5].into(), [0, 999].into()] {
        pen.selected.as_mut().unwrap().vertices = vertices.clone();
        for chord in ["shift-f", "shift-r"] {
            assert!(pen.order_key(&event(chord), true, false, &s).1.is_none());
            assert_eq!(pen.selected.as_ref().unwrap().vertices, vertices);
        }
    }
    pen.selected.as_mut().unwrap().vertices = [1].into();
    for target in [
        Target::Shape(999),
        Target::Contents(1, 2),
        Target::Mask(1, 0),
        Target::NewShape,
        Target::NewContents(1, 0),
    ] {
        pen.selected.as_mut().unwrap().target = target;
        assert!(
            pen.order_key(&event("shift-r"), true, false, &s)
                .1
                .is_none()
        );
        assert_eq!(selected(&pen), [1]);
    }
    assert!(!s.editor.can_undo());
}

#[test]
fn shortcut_modifiers_focus_ime_and_key_repeat_are_safe() {
    let (s, target, _) = scene(0, true, false);
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 2, false);
    for chord in [
        "f",
        "r",
        "ctrl-r",
        "ctrl-shift-r",
        "alt-shift-f",
        "cmd-shift-r",
        "shift-g",
        "ctrl-shift-f",
        "alt-shift-r",
    ] {
        let (handled, command) = pen.order_key(&event(chord), true, false, &s);
        assert!(!handled && command.is_none(), "{chord}");
    }
    let mut function = event("shift-r");
    function.keystroke.modifiers.function = true;
    assert!(!pen.order_key(&function, true, false, &s).0);
    for (focus, composing) in [(false, false), (true, true), (false, true)] {
        let (handled, command) = pen.order_key(&event("shift-f"), focus, composing, &s);
        assert!(!handled && command.is_none());
    }
    let mut held = event("shift-r");
    held.is_held = true;
    let (handled, command) = pen.order_key(&held, true, false, &s);
    assert!(handled && command.is_none());
    assert_eq!(selected(&pen), [2]);
    assert!(!s.editor.can_undo());
}

#[test]
fn stale_context_rejects_order_without_publishing_or_remapping_old_indices() {
    for mode in 0..8 {
        let (mut s, target, _) = scene(1, true, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 2, false);
        match mode {
            0 => s.frame += 1,
            1 => s.document_revision += 1,
            2 => s.tool = Tool::Select,
            3 => s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 1)),
            4 => {
                s.selected_layers.insert(1);
            }
            5 => {
                s.editor.execute(Command::AddNull).unwrap();
            }
            6 => {
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
            }
            _ => {
                s.editor
                    .execute(Command::SetValue {
                        id: 1,
                        property: Property::Opacity,
                        frame: 0,
                        value: 30.,
                    })
                    .unwrap();
            }
        }
        let before = s.editor.project().clone();
        let undo = s.editor.can_undo();
        let redo = s.editor.can_redo();
        assert!(
            pen.order_key(&event("shift-r"), true, false, &s)
                .1
                .is_none()
        );
        assert_eq!(selected(&pen), [2]);
        assert_eq!(s.editor.project(), &before);
        assert_eq!((s.editor.can_undo(), s.editor.can_redo()), (undo, redo));
    }
}

#[test]
fn locked_disabled_and_singular_paths_reject_even_with_a_current_selection_context() {
    for mode in 0..3 {
        let (mut s, target, _) = scene(1, true, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 2, false);
        match mode {
            0 => {
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
            }
            1 => {
                s.editor
                    .execute(Command::Contents {
                        id: 1,
                        edit: ContentsEdit::Enabled {
                            item: 1,
                            enabled: false,
                        },
                    })
                    .unwrap();
            }
            _ => {
                s.editor
                    .execute(Command::SetValue {
                        id: 1,
                        property: Property::ScaleX,
                        frame: 0,
                        value: 0.,
                    })
                    .unwrap();
            }
        }
        pen.selected_context = Some(Context::capture(&s));
        let before = s.editor.project().clone();
        assert!(
            pen.order_key(&event("shift-f"), true, false, &s)
                .1
                .is_none()
        );
        assert!(
            pen.order_key(&event("shift-r"), true, false, &s)
                .1
                .is_none()
        );
        assert_eq!(selected(&pen), [2]);
        assert_eq!(s.editor.project(), &before);
    }
}

#[test]
fn drag_and_pending_insertion_consume_order_without_committing_unpublished_indices() {
    for insertion in [false, true] {
        let (mut s, target, _) = scene(0, true, false);
        redo(&mut s);
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        let (path, world) = geometry(&s, target);
        let pointer = if insertion {
            [100., 40.]
        } else {
            path.vertices[2].position
        };
        assert!(
            pen.down(&s, world.point(pointer), 1., false, false, false)
                .is_none()
        );
        let pending = pen.pending(&s);
        let count = pen.drag.as_ref().unwrap().session.path.vertices.len();
        let selection = selected(&pen);
        for chord in ["shift-f", "shift-r"] {
            let (handled, command) = pen.order_key(&event(chord), true, false, &s);
            assert!(handled && command.is_none());
        }
        assert_eq!(
            pen.drag.as_ref().unwrap().session.path.vertices.len(),
            count
        );
        assert_eq!(selected(&pen), selection);
        assert_eq!(pen.pending(&s).is_some(), pending.is_some());
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo() && s.editor.can_redo());
        pen.cancel();
        assert!(pen.up(&s).is_none());
    }
}

#[test]
fn unfinished_draft_and_held_pointer_never_become_an_order_command() {
    let (s, target, _) = scene(0, true, false);
    let mut pen = Pen::default();
    for p in [[700., 600.], [800., 600.], [800., 700.]] {
        assert!(pen.down(&s, p, 1., false, false, false).is_none());
        assert!(pen.up(&s).is_none());
    }
    let draft = pen.draft.as_ref().unwrap().path.clone();
    for chord in ["shift-f", "shift-r"] {
        let (handled, command) = pen.order_key(&event(chord), true, false, &s);
        assert!(handled && command.is_none());
        assert_eq!(pen.draft.as_ref().unwrap().path, draft);
    }
    assert!(pen.order_help(&s).contains("finish or cancel"));
    pen.cancel();
    pick(&mut pen, &s, target, 1, false);
    pen.held = true;
    assert!(
        pen.order_key(&event("shift-r"), true, false, &s)
            .1
            .is_none()
    );
    assert_eq!(selected(&pen), [1]);
    assert!(!s.editor.can_undo());
}

#[test]
fn focus_cancellation_and_canvas_text_session_cannot_reorder() {
    let (mut s, target, _) = scene(0, true, false);
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 1, false);
    // Preview calls this on blur and window deactivation.
    pen.cancel();
    assert!(
        pen.order_key(&event("shift-f"), true, false, &s)
            .1
            .is_none()
    );
    pick(&mut pen, &s, target, 1, false);
    s.text_session = Some(
        crate::text_edit::Session::new(
            s.editor.project(),
            s.document_revision,
            s.frame,
            None,
            [0., 0.],
        )
        .unwrap(),
    );
    let (handled, command) = pen.order_key(&event("shift-r"), true, false, &s);
    assert!(!handled && command.is_none());
    assert_eq!(selected(&pen), [1]);
    assert!(!s.editor.can_undo());
}

#[test]
fn masks_use_stable_ids_and_stale_reordered_mask_lists_never_fall_back() {
    let (mut s, target, _) = scene(2, true, false);
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 2, false);
    let command = apply(&mut pen, &mut s, "shift-f");
    assert!(matches!(
        command,
        Command::ReorderPath {
            target: PathTarget::Mask(2),
            ..
        }
    ));
    let mut masks = s.editor.selected_layer().unwrap().path_masks().to_vec();
    masks.insert(
        0,
        PathMask {
            id: 0,
            path: path(true),
            ..Default::default()
        },
    );
    s.editor
        .execute(Command::SetPathMasks { id: 1, masks })
        .unwrap();
    let before = s.editor.project().clone();
    assert!(
        pen.order_key(&event("shift-r"), true, false, &s)
            .1
            .is_none()
    );
    assert_eq!(s.editor.project(), &before);
    assert_eq!(selected(&pen), [0]);
}

#[test]
fn ordering_under_nested_skew_and_parent_reflection_keeps_world_vertices_and_transforms() {
    let (mut s, target, _) = scene(1, true, true);
    s.editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add {
                parent: 0,
                kind: ContentsKind::Group(vec![]),
            },
        })
        .unwrap();
    s.editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Move {
                item: 1,
                parent: 5,
                index: 0,
            },
        })
        .unwrap();
    s.editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Transform(Property::Rotation),
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 31.,
                },
            },
        })
        .unwrap();
    s.editor.execute(Command::AddNull).unwrap();
    s.editor
        .execute(Command::SetParent {
            id: 1,
            parent: Some(2),
            frame: 0,
        })
        .unwrap();
    for (property, value) in [
        (Property::ScaleX, -140.),
        (Property::ScaleY, 72.),
        (Property::Rotation, -26.),
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
    s.editor.clear_history();
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 3, false);
    let (before, world) = geometry(&s, target);
    apply(&mut pen, &mut s, "shift-f");
    let (after, transformed) = geometry(&s, target);
    assert_eq!(world, transformed);
    for (index, vertex) in after.vertices.iter().enumerate() {
        assert_eq!(
            transformed.point(vertex.position),
            world.point(before.vertices[(index + 3) % 5].position)
        );
    }
    apply(&mut pen, &mut s, "shift-r");
    assert_eq!(selected(&pen), [0]);
    assert_eq!(geometry(&s, target).1, world);
}
