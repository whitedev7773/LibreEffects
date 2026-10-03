use super::*;
use libre_effects_core::{ContentsParam, Property, TrackEdit};

fn event(chord: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: gpui::Keystroke::parse(chord).unwrap(),
        is_held: false,
    }
}
fn path() -> VectorPath {
    VectorPath {
        closed: true,
        vertices: [[20., 20.], [140., 20.], [140., 140.], [20., 140.]]
            .into_iter()
            .map(PathVertex::corner)
            .collect(),
    }
}
fn scene(kind: usize, animated: bool) -> (EditorState, Target, PathTarget) {
    let mut s = EditorState::default();
    s.tool = Tool::Pen;
    s.composition_started = true;
    s.editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(path()),
                ..Default::default()
            }),
            width: 300.,
            height: 300.,
            name: "Numeric vertex bridge".into(),
        })
        .unwrap();
    let (target, stable) = match kind {
        0 => (Target::Shape(1), PathTarget::Shape),
        1 => {
            for edit in [
                ContentsEdit::Promote,
                ContentsEdit::Add {
                    parent: 0,
                    kind: ContentsKind::Group(vec![]),
                },
                ContentsEdit::Move {
                    item: 1,
                    parent: 5,
                    index: 0,
                },
            ] {
                s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
            }
            for (item, parameter, value) in [
                (1, ContentsParam::Skew, 24.),
                (1, ContentsParam::SkewAxis, 39.),
                (1, ContentsParam::Transform(Property::Rotation), -11.),
                (5, ContentsParam::Transform(Property::Rotation), 31.),
                (5, ContentsParam::Transform(Property::ScaleX), -125.),
            ] {
                s.editor
                    .execute(Command::Contents {
                        id: 1,
                        edit: ContentsEdit::Track {
                            item,
                            parameter,
                            edit: TrackEdit::Value { frame: 0, value },
                        },
                    })
                    .unwrap();
            }
            (Target::Contents(1, 2), PathTarget::Contents(2))
        }
        _ => {
            s.editor
                .execute(Command::SetPathMasks {
                    id: 1,
                    masks: vec![
                        PathMask {
                            path: path(),
                            ..Default::default()
                        },
                        PathMask {
                            path: path(),
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
    for (property, value) in [(Property::Rotation, 17.), (Property::ScaleY, 80.)] {
        s.editor
            .execute(Command::SetValue {
                id: 1,
                property,
                frame: 0,
                value,
            })
            .unwrap();
    }
    if animated {
        s.editor
            .execute(Command::AnimatePath {
                id: 1,
                target: stable,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
        let mut next = path();
        for vertex in &mut next.vertices {
            vertex.position[0] += 40.;
        }
        s.editor
            .execute(Command::EditPath {
                id: 1,
                target: stable,
                frame: 20,
                path: next,
            })
            .unwrap();
        s.frame = 10;
    }
    s.selected_layers = [1].into();
    s.editor.clear_history();
    (s, target, stable)
}
fn selected(s: &EditorState, target: Target, index: usize) -> Pen {
    let mut pen = Pen::default();
    pen.select_vertex(target, index, false, s);
    pen
}

#[test]
fn numeric_request_captures_all_path_targets_evaluated_pose_and_exact_world() {
    for kind in 0..3 {
        for animated in [false, true] {
            let (s, target, stable) = scene(kind, animated);
            let pen = selected(&s, target, 2);
            assert!(pen.numeric_vertex_available(&s));
            let request = pen.single_vertex_request(&s).unwrap();
            let (_, path, world) = paths(&s)
                .into_iter()
                .find(|(t, _, _)| *t == target)
                .unwrap();
            assert_eq!(request.layer, 1);
            assert!(request.target == stable);
            assert_eq!(request.index, 2);
            assert_eq!(request.frame, s.frame);
            assert_eq!(request.path, path);
            assert_eq!(request.world, world);
            assert!(request.current(&s));
            if kind == 1 {
                assert_ne!(
                    world,
                    s.editor
                        .project()
                        .composition()
                        .world_transform(1, s.frame)
                        .unwrap()
                );
            }
            if kind == 2 {
                assert!(request.target == PathTarget::Mask(2));
            }
            if animated {
                assert_eq!(request.path.vertices[0].position[0], 40.);
            }
        }
    }
}

#[test]
fn numeric_request_never_infers_selection_from_layer_or_contents_row() {
    for kind in 0..3 {
        let (mut s, _, _) = scene(kind, false);
        s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 2));
        assert!(Pen::default().single_vertex_request(&s).is_none());
    }
}

#[test]
fn numeric_request_requires_exactly_one_existing_index_and_idle_gesture() {
    for guard in 0..10 {
        let (s, target, _) = scene(0, false);
        let mut pen = selected(&s, target, 1);
        let (path, world) = {
            let (_, p, w) = paths(&s).remove(0);
            (p, w)
        };
        match guard {
            0 => pen.selected.as_mut().unwrap().vertices.clear(),
            1 => {
                pen.selected.as_mut().unwrap().vertices.insert(2);
            }
            2 => pen.selected.as_mut().unwrap().vertices = [99].into(),
            3 => pen.held = true,
            4 => pen.selected_context = None,
            5 => {
                pen.draft = Some(Session {
                    target,
                    path,
                    world,
                    context: Context::capture(&s),
                })
            }
            6 => {
                pen.marquee = Some(Marquee {
                    session: Session {
                        target,
                        path,
                        world,
                        context: Context::capture(&s),
                    },
                    start: [0.; 2],
                    end: [200.; 2],
                    zoom: 1.,
                    moved: false,
                    vertices: [1].into(),
                })
            }
            7 => {
                pen.down(
                    &s,
                    world.point(path.vertices[1].position),
                    1.,
                    false,
                    false,
                    false,
                );
                assert!(pen.drag.is_some());
            }
            8 => {
                pen.down(&s, world.point([80., 20.]), 1., false, false, false);
                assert!(pen.drag.is_some());
                assert_eq!(pen.drag.as_ref().unwrap().start.vertices.len(), 5);
            }
            _ => pen.selected.as_mut().unwrap().target = Target::NewShape,
        }
        assert!(pen.single_vertex_request(&s).is_none(), "guard {guard}");
        assert!(
            !pen.numeric_vertex_available(&s),
            "availability guard {guard}"
        );
    }
}

#[test]
fn numeric_request_rejects_stale_source_context_and_competing_workflows() {
    for guard in 0..16 {
        let (mut s, target, _) = scene(0, false);
        let pen = selected(&s, target, 1);
        match guard {
            0 => s.frame += 1,
            1 => s.document_revision += 1,
            2 => s.selected_layers.clear(),
            3 => s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 2)),
            4 => s.tool = Tool::Select,
            5 => s.playing = true,
            6 => {
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
            }
            7 => {
                s.editor.execute(Command::AddNull).unwrap();
            }
            8 => s.new_composition_requested = true,
            9 => s.media_open = true,
            10 => s.fonts_open = true,
            11 => s.queue_open = true,
            12 => s.exporting = true,
            13 => s.preview_caching = true,
            14 => {
                s.editor
                    .execute(Command::SetValue {
                        id: 1,
                        property: Property::Opacity,
                        frame: 0,
                        value: 75.,
                    })
                    .unwrap();
            }
            _ => {
                let request = pen.single_vertex_request(&s).unwrap();
                s.vertex_editor =
                    Some(super::super::vertex_editor::Session::new(&s, request).unwrap());
            }
        }
        assert!(pen.single_vertex_request(&s).is_none(), "guard {guard}");
        assert!(
            !pen.numeric_vertex_available(&s),
            "availability guard {guard}"
        );
    }
}

#[test]
fn numeric_shift_v_is_exact_canvas_only_and_consumes_unavailable_or_repeated() {
    let (mut s, target, _) = scene(0, false);
    let pen = selected(&s, target, 1);
    assert!(
        pen.numeric_vertex_key(&event("shift-v"), true, false, &s)
            .1
            .is_some()
    );
    for chord in [
        "v",
        "ctrl-v",
        "ctrl-shift-v",
        "alt-shift-v",
        "cmd-shift-v",
        "shift-r",
    ] {
        assert!(
            !pen.numeric_vertex_key(&event(chord), true, false, &s).0,
            "{chord}"
        );
    }
    assert!(
        !pen.numeric_vertex_key(&event("shift-v"), false, false, &s)
            .0
    );
    assert!(!pen.numeric_vertex_key(&event("shift-v"), true, true, &s).0);
    let mut function_modified = event("shift-v");
    function_modified.keystroke.modifiers.function = true;
    assert!(
        !pen.numeric_vertex_key(&function_modified, true, false, &s)
            .0
    );
    let mut repeated = event("shift-v");
    repeated.is_held = true;
    let result = pen.numeric_vertex_key(&repeated, true, false, &s);
    assert!(result.0 && result.1.is_none());
    for unavailable in [false, true] {
        s.playing = unavailable;
        let result = Pen::default().numeric_vertex_key(&event("shift-v"), true, false, &s);
        assert!(result.0 && result.1.is_none());
    }
}

#[test]
fn numeric_request_is_captured_before_blur_but_validates_again_on_open() {
    let (mut s, target, _) = scene(2, false);
    let mut pen = selected(&s, target, 1);
    let request = pen.single_vertex_request(&s).unwrap();
    pen.cancel();
    assert!(pen.single_vertex_request(&s).is_none());
    assert!(super::super::vertex_editor::Session::new(&s, request.clone()).is_ok());
    s.frame += 1;
    assert!(super::super::vertex_editor::Session::new(&s, request).is_err());
}

#[test]
fn numeric_restore_checks_expected_source_stable_target_and_resulting_pose() {
    for kind in 0..3 {
        let (mut s, target, _) = scene(kind, true);
        let mut pen = selected(&s, target, 1);
        let original = pen.single_vertex_request(&s).unwrap();
        let before = s.editor.project().clone();
        let mut draft = super::super::vertex_editor::Session::new(&s, original.clone()).unwrap();
        draft.input(0, "217.125").unwrap();
        s.vertex_editor = Some(draft);
        pen.cancel();
        assert!(!pen.restore_vertex(&original, &s));
        s.accept_vertex_editor();
        assert!(!pen.restore_vertex(&original, &s));
        let request = s.vertex_return.take().unwrap();
        assert!(pen.restore_vertex(&request, &s));
        let restored = pen.single_vertex_request(&s).unwrap();
        assert_eq!(restored.path.vertices[1].position[0], 217.125);
        assert!(restored.target == request.target);
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        assert!(!pen.restore_vertex(&request, &s));
    }
}

#[test]
fn numeric_vertex_request_uses_final_committed_release_not_last_move() {
    let (mut s, target, _) = scene(0, false);
    let mut pen = selected(&s, target, 1);
    let (_, path, world) = paths(&s)
        .into_iter()
        .find(|(t, _, _)| *t == target)
        .unwrap();
    pen.down(
        &s,
        world.point(path.vertices[1].position),
        1.,
        false,
        false,
        false,
    );
    pen.moving(world.point([150., 25.]), false, false);
    assert!(pen.single_vertex_request(&s).is_none());
    let command = pen
        .release(&s, world.point([175., 45.]), false, false)
        .unwrap();
    s.editor.execute(command).unwrap();
    let request = pen.single_vertex_request(&s).unwrap();
    assert!((request.path.vertices[1].position[0] - 175.).abs() < 1e-9);
    assert!((request.path.vertices[1].position[1] - 45.).abs() < 1e-9);
}
