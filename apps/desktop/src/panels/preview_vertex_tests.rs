use super::*;
use libre_effects_core::{Content, PathTarget, PathVertex, Shape, VectorPath};

fn opened() -> EditorState {
    let mut state = EditorState::default();
    state.tool = Tool::Pen;
    state.composition_started = true;
    let path = VectorPath {
        closed: true,
        vertices: [[20., 20.], [140., 20.], [140., 140.]]
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
    state.editor.clear_history();
    let world = state
        .editor
        .project()
        .composition()
        .world_transform(1, 0)
        .unwrap();
    let request =
        super::super::vertex_editor::Request::new(&state, 1, PathTarget::Shape, 1, path, world)
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
