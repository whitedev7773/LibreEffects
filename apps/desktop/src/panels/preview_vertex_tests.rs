use super::*;
use libre_effects_core::{
    Content, ContentsEdit, ContentsKind, ContentsParam, PathMask, PathTarget, PathVertex, Shape,
    TrackEdit, VectorPath,
};

fn opened() -> EditorState {
    opened_selection(0, &[1])
}

fn opened_selection(kind: usize, indices: &[usize]) -> EditorState {
    let mut state = EditorState::default();
    state.tool = Tool::Pen;
    state.composition_started = true;
    let path = VectorPath {
        closed: true,
        vertices: [[20., 20.], [140., 20.], [140., 140.], [20., 140.]]
            .into_iter()
            .map(PathVertex::corner)
            .collect(),
    };
    state
        .editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(path.clone()),
                ..Default::default()
            }),
            width: 200.,
            height: 200.,
            name: "Preview vertex".into(),
        })
        .unwrap();
    state
        .editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::Rotation,
            frame: 0,
            value: 37.,
        })
        .unwrap();
    state
        .editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::ScaleX,
            frame: 0,
            value: -125.,
        })
        .unwrap();
    let target = match kind {
        0 => PathTarget::Shape,
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
                ContentsEdit::Track {
                    item: 1,
                    parameter: ContentsParam::Skew,
                    edit: TrackEdit::Value {
                        frame: 0,
                        value: 24.,
                    },
                },
                ContentsEdit::Track {
                    item: 5,
                    parameter: ContentsParam::Transform(Property::Rotation),
                    edit: TrackEdit::Value {
                        frame: 0,
                        value: 31.,
                    },
                },
                ContentsEdit::Track {
                    item: 5,
                    parameter: ContentsParam::Transform(Property::ScaleX),
                    edit: TrackEdit::Value {
                        frame: 0,
                        value: -125.,
                    },
                },
            ] {
                state
                    .editor
                    .execute(Command::Contents { id: 1, edit })
                    .unwrap();
            }
            PathTarget::Contents(2)
        }
        _ => {
            state
                .editor
                .execute(Command::SetPathMasks {
                    id: 1,
                    masks: vec![
                        PathMask {
                            path: path.clone(),
                            ..Default::default()
                        },
                        PathMask {
                            path: path.clone(),
                            ..Default::default()
                        },
                    ],
                })
                .unwrap();
            let masks = state.editor.selected_layer().unwrap().path_masks()[1..].to_vec();
            state
                .editor
                .execute(Command::SetPathMasks { id: 1, masks })
                .unwrap();
            PathTarget::Mask(2)
        }
    };
    state.editor.clear_history();
    let mut world = state
        .editor
        .project()
        .composition()
        .world_transform(1, 0)
        .unwrap();
    if let PathTarget::Contents(item) = target {
        let Content::ShapeContents(contents) = state.editor.selected_layer().unwrap().content()
        else {
            panic!("expected Shape Contents");
        };
        let (_, _, local) = contents
            .editable_paths(0)
            .into_iter()
            .find(|(id, _, _)| *id == item)
            .unwrap();
        world = world.compose(local);
    }
    let request = super::super::vertex_editor::Request::for_selection(
        &state,
        1,
        target,
        indices.iter().copied().collect(),
        path,
        world,
    )
    .unwrap();
    state.vertex_editor = Some(super::super::vertex_editor::Session::new(&state, request).unwrap());
    state
}

#[test]
fn numeric_vertex_preview_uses_isolated_path_world_and_selected_marker() {
    let mut state = opened();
    let source = state.editor.project().clone();
    let session = state.vertex_editor.as_mut().unwrap();
    let id = session.id;
    let world = session.request().world;
    session.input(0, "215.125").unwrap();
    session.input(4, "19.75").unwrap();
    assert!(preview_modal_active(&state));
    let draft = vertex_session(&state).unwrap();
    assert_eq!(draft.id, id);
    assert_ne!(draft.project(), &source);
    assert_eq!(state.editor.project(), &source);
    let overlay = vertex_overlay(&state);
    assert_eq!(overlay.len(), 1);
    assert_eq!(overlay[0].0.vertices[1].position, [215.125, 20.]);
    assert_eq!(overlay[0].0.vertices[1].outgoing, [19.75, 0.]);
    assert_eq!(overlay[0].1, world);
    assert!(!overlay[0].2);
    assert_eq!(overlay[0].3, [1].into());
    assert_eq!(
        world.point(overlay[0].0.vertices[1].position),
        draft.request().world.point([215.125, 20.])
    );
    state.frame += 1;
    assert!(vertex_session(&state).is_none());
    assert!(vertex_overlay(&state).is_empty());
    assert!(preview_modal_active(&state));
}

#[test]
fn numeric_vertex_return_is_one_shot_and_cancel_preserves_exact_source() {
    let mut state = opened();
    let source = state.editor.project().clone();
    state
        .vertex_editor
        .as_mut()
        .unwrap()
        .input(0, "700")
        .unwrap();
    state.cancel_vertex_editor();
    assert_eq!(state.editor.project(), &source);
    assert!(!preview_modal_active(&state));
    let mut pen = super::super::pen::Pen::default();
    assert!(restore_vertex_return(&mut pen, &mut state, true));
    assert!(state.vertex_return.is_none());
    assert!(pen.single_vertex_request(&state).is_some());
    assert!(!restore_vertex_return(&mut pen, &mut state, true));
}

#[test]
fn numeric_vertex_return_is_discarded_on_inactive_or_stale_context() {
    for changed in 0..3 {
        let mut state = opened();
        state.cancel_vertex_editor();
        assert!(state.vertex_return.is_some());
        let mut pen = super::super::pen::Pen::default();
        match changed {
            1 => state.frame += 1,
            2 => state.editor.execute(Command::ToggleLocked(1)).unwrap(),
            _ => (),
        }
        assert!(!restore_vertex_return(&mut pen, &mut state, changed != 0));
        assert!(state.vertex_return.is_none());
        assert!(!restore_vertex_return(&mut pen, &mut state, true));
        assert!(pen.single_vertex_request(&state).is_none());
    }
}

#[test]
fn numeric_vertex_stale_or_abandoned_modal_never_issues_return() {
    let mut state = opened();
    state.frame += 1;
    state.cancel_vertex_editor();
    assert!(state.vertex_return.is_none());
    assert!(state.vertex_editor.is_none());
    let mut state = opened();
    state.vertex_editor = None; // Shell window deactivation/closure abandonment.
    assert!(state.vertex_return.is_none());
    let mut pen = super::super::pen::Pen::default();
    assert!(!restore_vertex_return(&mut pen, &mut state, true));
}

#[test]
fn numeric_vertex_modal_release_leaves_capture_dispatch_and_source_untouched() {
    let mut state = opened();
    let source = state.editor.project().clone();
    let serial = state.vertex_editor.as_ref().unwrap().id;
    let mut propagates = true;
    let mut canvas_release_count = 0;
    route_preview_release(preview_modal_active(&state), || {
        // Model a canvas handler with consequential release work and propagation
        // changes. Neither is allowed before the modal receives its mouse-up.
        canvas_release_count += 1;
        propagates = false;
        state.cancel_vertex_editor();
    });
    assert!(propagates);
    assert_eq!(canvas_release_count, 0);
    assert_eq!(state.vertex_editor.as_ref().unwrap().id, serial);
    assert_eq!(state.editor.project(), &source);
    assert!(state.vertex_return.is_none());
    // The untouched event may now reach the modal button's normal click handler.
    state.cancel_vertex_editor();
    assert!(state.vertex_editor.is_none());
    assert_eq!(state.editor.project(), &source);
}

#[test]
fn numeric_vertex_release_without_modal_routes_canvas_handler_once() {
    let mut state = opened();
    state.cancel_vertex_editor();
    let mut canvas_release_count = 0;
    route_preview_release(preview_modal_active(&state), || canvas_release_count += 1);
    assert_eq!(canvas_release_count, 1);
}

#[test]
fn transform_overlay_uses_isolated_geometry_frozen_world_and_all_selected_markers() {
    for kind in 0..3 {
        for indices in [&[0, 2][..], &[0, 1, 2, 3][..]] {
            let mut state = opened_selection(kind, indices);
            let source = state.editor.project().clone();
            let session = state.vertex_editor.as_mut().unwrap();
            let opening_path = session.path().clone();
            let world = session.request().world;
            for (field, value) in [
                (0, "12.5"),
                (1, "-7.25"),
                (2, "90"),
                (3, "-50"),
                (4, "125"),
                (5, "20"),
                (6, "30"),
            ] {
                session.input(field, value).unwrap();
            }
            let expected_path = session.path().clone();
            assert_ne!(session.project(), &source);
            assert_eq!(state.editor.project(), &source);
            let overlay = vertex_overlay(&state);
            assert_eq!(overlay.len(), 1);
            assert_eq!(overlay[0].0, expected_path);
            assert_eq!(overlay[0].1, world);
            assert_eq!(overlay[0].2, kind == 2);
            assert_eq!(overlay[0].3, indices.iter().copied().collect());
            for (index, vertex) in overlay[0].0.vertices.iter().enumerate() {
                if indices.contains(&index) {
                    assert_ne!(vertex.position, opening_path.vertices[index].position);
                    assert_eq!(
                        world.point(vertex.position),
                        world.point(expected_path.vertices[index].position)
                    );
                } else {
                    assert_eq!(vertex, &opening_path.vertices[index]);
                }
            }
            state.frame += 1;
            assert!(vertex_overlay(&state).is_empty());
            assert!(preview_modal_active(&state));
        }
    }
}

#[test]
fn transform_return_restores_all_indices_once_after_both_normal_exits() {
    for kind in 0..3 {
        for indices in [&[0, 2][..], &[0, 1, 2, 3][..]] {
            for accept in [false, true] {
                let mut state = opened_selection(kind, indices);
                let source = state.editor.project().clone();
                let original = state.vertex_editor.as_ref().unwrap().request().clone();
                state
                    .vertex_editor
                    .as_mut()
                    .unwrap()
                    .input(0, "25.125")
                    .unwrap();
                if accept {
                    state.accept_vertex_editor();
                } else {
                    state.cancel_vertex_editor();
                }
                assert!(!preview_modal_active(&state));
                let mut pen = super::super::pen::Pen::default();
                assert!(restore_vertex_return(&mut pen, &mut state, true));
                assert!(state.vertex_return.is_none());
                let restored = pen.numeric_vertex_request(&state).unwrap();
                assert_eq!(restored.indices, indices.iter().copied().collect());
                assert!(restored.target == original.target);
                assert_eq!(restored.world, original.world);
                if accept {
                    assert_ne!(state.editor.project(), &source);
                } else {
                    assert_eq!(state.editor.project(), &source);
                }
                pen.cancel();
                assert!(!restore_vertex_return(&mut pen, &mut state, true));
                assert!(pen.numeric_vertex_request(&state).is_none());
            }
        }
    }
}

#[test]
fn transform_invalid_or_inactive_return_is_consumed_without_later_resurrection() {
    for kind in 0..3 {
        for changed in 0..6 {
            let mut state = opened_selection(kind, &[0, 2]);
            state.cancel_vertex_editor();
            let old_frame = state.frame;
            let old_layers = state.selected_layers.clone();
            let old_contents = state.contents_selection;
            match changed {
                1 => state.frame += 1,
                2 => state.editor.execute(Command::ToggleLocked(1)).unwrap(),
                3 => state.tool = Tool::Select,
                4 => {
                    state.selected_layers.insert(999);
                }
                5 => {
                    state.contents_selection =
                        Some((state.editor.project().active_composition_id(), 1, 5))
                }
                _ => (),
            }
            let mut pen = super::super::pen::Pen::default();
            assert!(!restore_vertex_return(&mut pen, &mut state, changed != 0));
            assert!(state.vertex_return.is_none());
            state.frame = old_frame;
            state.tool = Tool::Pen;
            state.selected_layers = old_layers;
            state.contents_selection = old_contents;
            if changed == 2 {
                state.editor.undo();
            }
            assert!(!restore_vertex_return(&mut pen, &mut state, true));
            assert!(pen.numeric_vertex_request(&state).is_none());
        }
    }
}

#[test]
fn transform_forged_return_set_is_consumed_without_partial_selection_restore() {
    for indices in [std::collections::BTreeSet::new(), [0, 99].into()] {
        let mut state = opened_selection(2, &[0, 2]);
        state.cancel_vertex_editor();
        state.vertex_return.as_mut().unwrap().indices = indices;
        let mut pen = super::super::pen::Pen::default();
        assert!(!restore_vertex_return(&mut pen, &mut state, true));
        assert!(state.vertex_return.is_none());
        assert!(pen.numeric_vertex_request(&state).is_none());
        assert!(!restore_vertex_return(&mut pen, &mut state, true));
    }
}

#[test]
fn transform_stale_or_abandoned_modal_never_restores_selection() {
    for kind in 0..3 {
        for abandon in [false, true] {
            let mut state = opened_selection(kind, &[0, 2]);
            if abandon {
                state.vertex_editor = None;
            } else {
                state.frame += 1;
                state.cancel_vertex_editor();
                state.frame -= 1;
            }
            assert!(state.vertex_return.is_none());
            assert!(state.vertex_editor.is_none());
            let mut pen = super::super::pen::Pen::default();
            assert!(!restore_vertex_return(&mut pen, &mut state, true));
            assert!(pen.numeric_vertex_request(&state).is_none());
        }
    }
}

#[test]
fn transform_modal_release_keeps_button_dispatch_intact_for_ok_and_cancel() {
    for kind in 0..3 {
        for accept in [false, true] {
            let mut state = opened_selection(kind, &[0, 2]);
            let source = state.editor.project().clone();
            let serial = state.vertex_editor.as_ref().unwrap().id;
            state
                .vertex_editor
                .as_mut()
                .unwrap()
                .input(0, "25.125")
                .unwrap();
            let mut propagates = true;
            let mut releases = 0;
            route_preview_release(preview_modal_active(&state), || {
                propagates = false;
                releases += 1;
                state.cancel_vertex_editor();
            });
            assert!(propagates);
            assert_eq!(releases, 0);
            assert_eq!(state.vertex_editor.as_ref().unwrap().id, serial);
            assert_eq!(state.editor.project(), &source);
            assert!(state.vertex_return.is_none());
            if accept {
                state.accept_vertex_editor();
            } else {
                state.cancel_vertex_editor();
            }
            assert!(state.vertex_editor.is_none());
            assert_eq!(state.vertex_return.as_ref().unwrap().indices, [0, 2].into());
            route_preview_release(preview_modal_active(&state), || releases += 1);
            assert_eq!(releases, 1);
        }
    }
}

#[test]
fn whole_pose_overlay_samples_completed_draft_at_frozen_frame_with_same_world_and_selection() {
    use super::super::vertex_editor::{Request, Session, TransformScope};
    for kind in 0..3 {
        let mut state = opened_selection(kind, &[0, 2]);
        let original = state.vertex_editor.take().unwrap().request().clone();
        let target = original.target;
        state
            .editor
            .execute(Command::AnimatePath {
                id: 1,
                target,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
        let mut next = original.path.clone();
        for (index, vertex) in next.vertices.iter_mut().enumerate() {
            vertex.position[0] += 17.123456789012345 + index as f64 * 0.125;
            vertex.position[1] -= 31.987654321098765;
            vertex.incoming = [3.123456789012345, -8.5];
            vertex.outgoing = [-7.25, 4.987654321098765];
        }
        state
            .editor
            .execute(Command::EditPath {
                id: 1,
                target,
                frame: 20,
                path: next,
            })
            .unwrap();
        state.frame = 7;
        state.editor.clear_history();
        let source = state.editor.project().clone();
        let (base, animation) = source
            .composition()
            .layer(1)
            .unwrap()
            .path_animation(target)
            .unwrap();
        let opening = animation.at(base, state.frame);
        let request =
            Request::for_selection(&state, 1, target, [0, 2].into(), opening, original.world)
                .unwrap();
        state.vertex_editor = Some(Session::new(&state, request).unwrap());
        let serial = state.vertex_editor.as_ref().unwrap().id;
        assert!(state.switch_vertex_scope(serial, TransformScope::AllPoses, None, false));
        let session = state.vertex_editor.as_mut().unwrap();
        for (index, value) in [
            (0, "0.123456789012345"),
            (1, "-3.987654321098765"),
            (2, "33.75"),
            (3, "-73.125"),
            (4, "127.5"),
        ] {
            session.input(index, value).unwrap();
        }
        let (base, animation) = session
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .path_animation(target)
            .unwrap();
        let rendered_sample = animation.at(base, state.frame);
        assert_eq!(session.path(), &rendered_sample);
        let overlay = vertex_overlay(&state);
        assert_eq!(overlay[0].0, rendered_sample);
        assert_eq!(overlay[0].1, original.world);
        assert_eq!(overlay[0].2, kind == 2);
        assert_eq!(overlay[0].3, [0, 2].into());
        assert_eq!(state.editor.project(), &source);
        assert!(!state.editor.can_undo());
        state.frame += 1;
        assert!(vertex_overlay(&state).is_empty());
    }
}

#[test]
fn whole_pose_modal_release_and_both_exits_preserve_one_shot_selection_return() {
    use super::super::vertex_editor::TransformScope;
    for kind in 0..3 {
        for accept in [false, true] {
            let mut state = opened_selection(kind, &[0, 2]);
            let source = state.editor.project().clone();
            let first = state.vertex_editor.as_ref().unwrap().id;
            assert!(state.switch_vertex_scope(
                first,
                TransformScope::AllPoses,
                Some((0, "25.125")),
                false
            ));
            let expected = state.vertex_editor.as_ref().unwrap().project().clone();
            let mut canvas_releases = 0;
            route_preview_release(preview_modal_active(&state), || canvas_releases += 1);
            assert_eq!(canvas_releases, 0);
            if accept {
                state.accept_vertex_editor();
            } else {
                state.cancel_vertex_editor();
            }
            assert!(state.vertex_editor.is_none());
            assert_eq!(
                state.editor.project(),
                if accept { &expected } else { &source }
            );
            let mut pen = super::super::pen::Pen::default();
            assert!(restore_vertex_return(&mut pen, &mut state, true));
            assert!(state.vertex_return.is_none());
            let returned = pen.numeric_vertex_request(&state).unwrap();
            assert_eq!(returned.indices, [0, 2].into());
            let (base, animation) = state
                .editor
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .path_animation(returned.target)
                .unwrap();
            assert_eq!(returned.path, animation.at(base, state.frame));
            assert!(!restore_vertex_return(&mut pen, &mut state, true));
            state.vertex_input(first, 0, "999");
            assert_eq!(
                state.editor.project(),
                if accept { &expected } else { &source }
            );
            let reopened = super::super::vertex_editor::Session::new(&state, returned).unwrap();
            assert_eq!(reopened.scope(), TransformScope::ThisFrame);
        }
    }
}
