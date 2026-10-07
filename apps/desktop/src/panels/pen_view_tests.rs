use super::*;

fn scene() -> EditorState {
    let mut s = EditorState::default();
    s.tool = Tool::Pen;
    s.composition_started = true;
    let mut vertices = [
        [20., 20.],
        [120., 20.],
        [220., 20.],
        [220., 220.],
        [120., 220.],
        [20., 220.],
    ]
    .into_iter()
    .map(PathVertex::corner)
    .collect::<Vec<_>>();
    vertices[0].outgoing = [30., 50.];
    vertices[0].incoming = [-30., -50.];
    let c = s.editor.project().composition();
    s.editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(VectorPath {
                    vertices,
                    closed: true,
                }),
                ..Default::default()
            }),
            width: c.width() as f64,
            height: c.height() as f64,
            name: "View safety".into(),
        })
        .unwrap();
    s.editor.clear_history();
    s
}
fn view(s: &EditorState) -> View {
    View::new(
        Bounds::new(point(px(100.), px(100.)), size(px(800.), px(600.))),
        point(px(137.), px(169.)),
        0.5,
        s,
    )
}
fn screen(view: View, p: [f64; 2]) -> Point<Pixels> {
    view.origin + point(px(p[0] as f32 * view.zoom), px(p[1] as f32 * view.zoom))
}
fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
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
fn pick(pen: &mut Pen, s: &EditorState, view: View, index: usize) {
    let (_, path, world) = paths(s).remove(0);
    let p = screen(view, world.point(path.vertices[index].position));
    assert!(
        pen.pointer_down(s, p, Some(view), Modifiers::default())
            .is_none()
    );
    assert!(
        pen.pointer_up(s, p, Some(view), Modifiers::default())
            .is_none()
    );
}
fn held(pen: &mut Pen, s: &EditorState, view: View, kind: usize) {
    pick(pen, s, view, 0);
    let (_, path, world) = paths(s).remove(0);
    let local = match kind {
        0 => [0., 0.], // Additive selection box.
        1 => path.vertices[1].position,
        2 => add(path.vertices[0].position, path.vertices[0].outgoing),
        3 => [220., 120.], // Straight segment insertion.
        _ => [500., 500.], // New shape creation.
    };
    let m = if kind == 0 {
        shift()
    } else {
        Modifiers::default()
    };
    assert!(
        pen.pointer_down(s, screen(view, world.point(local)), Some(view), m)
            .is_none()
    );
    assert!(pen.held && pen.pointer_view.is_some());
    pen.pointer_move(
        s,
        screen(view, world.point(add(local, [40., 70.]))),
        Some(view),
        m,
    );
}
fn changed(mut view: View, mode: usize) -> Option<View> {
    match mode {
        0 => view.zoom_setting = Some(2.),
        1 => view.pan = [70., -20.],
        2 => view.rulers = !view.rulers,
        3 => view.bounds.origin.x += px(1.),
        4 => view.bounds.size.width += px(1.),
        5 => view.zoom *= 1.2,
        6 => view.origin.y += px(1.),
        7 => return None,
        _ => view.origin.x = px(f32::NAN),
    }
    Some(view)
}
#[test]
fn held_box_geometry_handle_insertion_and_creation_cancel_on_every_view_change_before_mapping() {
    for kind in 0..5 {
        for mode in 0..9 {
            for release_only in [false, true] {
                let s = scene();
                let v = view(&s);
                let mut pen = Pen::default();
                held(&mut pen, &s, v, kind);
                let source = s.editor.project().clone();
                let selection = pen.selected.as_ref().map(|s| s.vertices.clone());
                let altered = changed(v, mode);
                let position = point(px(900.), px(650.));
                if !release_only {
                    pen.pointer_move(&s, position, altered, Modifiers::default());
                    assert!(!pen.held && pen.pointer_view.is_none());
                }
                assert!(
                    pen.pointer_up(&s, position, altered, Modifiers::default())
                        .is_none()
                );
                assert!(pen.marquee.is_none() && pen.drag.is_none() && pen.draft.is_none());
                assert!(pen.pending(&s).is_none());
                if kind == 3 {
                    assert!(selected(&pen).is_empty());
                } else {
                    assert_eq!(pen.selected.as_ref().map(|s| s.vertices.clone()), selection);
                }
                // Returning to the old mapping never resurrects canceled input.
                pen.pointer_move(&s, position, Some(v), Modifiers::default());
                assert!(
                    pen.pointer_up(&s, position, Some(v), Modifiers::default())
                        .is_none()
                );
                assert_eq!(s.editor.project(), &source);
                assert!(!s.editor.can_undo());
            }
        }
    }
}
#[test]
fn idle_view_changes_preserve_target_and_creation_between_points() {
    let s = scene();
    let v = view(&s);
    let mut pen = Pen::default();
    pick(&mut pen, &s, v, 0);
    for mode in 0..8 {
        pen.validate_view(changed(v, mode));
        assert_eq!(selected(&pen), [0]);
        assert!(pen.selected_context.as_ref().unwrap().valid(&s));
    }
    let start = [500., 500.];
    pen.pointer_down(&s, screen(v, start), Some(v), Modifiers::default());
    pen.pointer_up(
        &s,
        screen(v, add(start, [20., 10.])),
        Some(v),
        Modifiers::default(),
    );
    let before = pen.draft.as_ref().unwrap().path.clone();
    let mut next = v;
    next.zoom = 1.5;
    next.origin = point(px(-300.), px(50.));
    next.zoom_setting = Some(1.5);
    assert!(pen.validate_view(Some(next)));
    assert_eq!(pen.draft.as_ref().unwrap().path, before);
    let end = [600., 500.];
    pen.pointer_down(&s, screen(next, end), Some(next), Modifiers::default());
    pen.pointer_up(&s, screen(next, end), Some(next), Modifiers::default());
    let draft = &pen.draft.as_ref().unwrap().path;
    assert_eq!(draft.vertices.len(), 2);
    assert_eq!(draft.vertices[0], before.vertices[0]);
    assert_eq!(draft.vertices[1].position, end);
}
#[test]
fn exact_shift_at_down_latches_box_despite_release_modifiers() {
    let s = scene();
    let v = view(&s);
    for excluded in 0..4 {
        let mut pen = Pen::default();
        pick(&mut pen, &s, v, 0);
        let mut m = shift();
        match excluded {
            0 => m.control = true,
            1 => m.alt = true,
            2 => m.platform = true,
            _ => m.function = true,
        }
        pen.pointer_down(&s, screen(v, [100., 0.]), Some(v), m);
        assert!(pen.marquee.is_none() && pen.draft.is_none());
        assert!(!pen.held);
        assert!(
            pen.pointer_up(&s, screen(v, [240., 240.]), Some(v), m)
                .is_none()
        );
        assert_eq!(selected(&pen), [0]);
    }
    let mut pen = Pen::default();
    pick(&mut pen, &s, v, 0);
    pen.pointer_down(&s, screen(v, [100., 0.]), Some(v), shift());
    assert!(pen.marquee.is_some());
    let all = Modifiers {
        shift: false,
        control: true,
        alt: true,
        platform: true,
        function: true,
    };
    assert!(
        pen.pointer_up(&s, screen(v, [240., 240.]), Some(v), all)
            .is_none()
    );
    assert_eq!(selected(&pen), [0, 1, 2, 3, 4]);
    assert!(pen.pending(&s).is_none() && !s.editor.can_undo());
}
#[test]
fn screen_threshold_and_outside_release_use_final_frozen_coordinates() {
    let s = scene();
    for zoom in [0.25, 0.5, 1., 2., 4.] {
        let mut v = view(&s);
        v.zoom = zoom;
        let mut pen = Pen::default();
        pick(&mut pen, &s, v, 0);
        let start = screen(v, [100., 0.]);
        pen.pointer_down(&s, start, Some(v), shift());
        pen.pointer_move(&s, start + point(px(3.99), px(0.)), Some(v), shift());
        assert!(pen.marquee_overlay(&s).is_none());
        pen.pointer_move(&s, start + point(px(4.), px(0.)), Some(v), shift());
        assert!(pen.marquee_overlay(&s).is_some());
        // The same release route handles canvas-out events. It does not clip
        // the logical box to the canvas or require one final move event.
        let outside = screen(v, [3000., 3000.]);
        assert!(!v.bounds.contains(&outside));
        assert!(
            pen.pointer_up(&s, outside, Some(v), Modifiers::default())
                .is_none()
        );
        assert_eq!(selected(&pen), [0, 1, 2, 3, 4]);
    }
}
#[test]
fn second_down_and_missing_bounds_abandon_only_unpublished_box() {
    let s = scene();
    let v = view(&s);
    let mut pen = Pen::default();
    held(&mut pen, &s, v, 0);
    pen.pointer_move(&s, screen(v, [300., 300.]), Some(v), shift());
    assert_eq!(selected(&pen), [0]);
    pen.pointer_down(&s, screen(v, [100., 0.]), None, shift());
    assert!(pen.marquee.is_none() && !pen.held);
    assert_eq!(selected(&pen), [0]);
    pen.pointer_down(&s, screen(v, [100., 0.]), Some(v), shift());
    assert!(pen.marquee.is_some());
    assert!(
        pen.pointer_up(&s, screen(v, [240., 50.]), Some(v), shift())
            .is_none()
    );
    assert_eq!(selected(&pen), [0, 1, 2]);
}
#[test]
fn ordinary_release_still_commits_one_geometry_edit_from_final_position() {
    let mut s = scene();
    let v = view(&s);
    let mut pen = Pen::default();
    pick(&mut pen, &s, v, 1);
    let before = s.editor.project().clone();
    let (_, path, world) = paths(&s).remove(0);
    let start = world.point(path.vertices[1].position);
    pen.pointer_down(&s, screen(v, start), Some(v), Modifiers::default());
    let end = add(start, [35., 50.]);
    let command = pen
        .pointer_up(&s, screen(v, end), Some(v), Modifiers::default())
        .unwrap();
    s.editor.execute(command).unwrap();
    assert_eq!(
        paths(&s)[0].1.vertices[1].position,
        add(path.vertices[1].position, [35., 50.])
    );
    assert_eq!(selected(&pen), [1]);
    assert!(pen.pointer_view.is_none());
    s.editor.undo();
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
}
#[test]
fn routed_second_down_abandons_box_or_inserted_indices_and_ignores_late_release() {
    let s = scene();
    let v = view(&s);
    for kind in [0, 1, 2, 3] {
        let mut pen = Pen::default();
        held(&mut pen, &s, v, kind);
        let baseline = selected(&pen);
        // Preview does this before a ruler/gradient/text handler can return.
        pen.reset_if_stale(&s);
        pen.validate_view(Some(v));
        pen.abandon_pointer();
        assert_eq!(selected(&pen), if kind == 3 { vec![] } else { baseline });
        assert!(pen.marquee.is_none() && pen.drag.is_none() && pen.pointer_view.is_none());
        assert!(!pen.held && pen.pending(&s).is_none());
        assert!(
            pen.pointer_up(&s, screen(v, [300., 300.]), Some(v), Modifiers::default())
                .is_none()
        );
        assert!(!s.editor.can_undo());
    }
}
#[test]
fn select_all_ignores_canvas_clipping_but_is_consumed_during_every_held_pointer_kind() {
    let s = scene();
    let mut v = view(&s);
    v.bounds = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.)));
    v.origin = point(px(0.), px(0.));
    v.zoom = 1.;
    let event = KeyDownEvent {
        keystroke: gpui::Keystroke::parse("ctrl-a").unwrap(),
        is_held: false,
    };
    let mut pen = Pen::default();
    pick(&mut pen, &s, v, 0);
    let (_, path, world) = paths(&s).remove(0);
    assert!(
        !v.bounds
            .contains(&screen(v, world.point(path.vertices[3].position)))
    );
    assert!(pen.select_all_key(&event, true, false, &s));
    assert_eq!(selected(&pen), [0, 1, 2, 3, 4, 5]);
    for kind in 0..5 {
        let mut pen = Pen::default();
        held(&mut pen, &s, v, kind);
        let selection = pen.selected.as_ref().map(|s| s.vertices.clone());
        let pending = pen.pending(&s).is_some();
        assert!(pen.select_all_key(&event, true, false, &s));
        assert_eq!(pen.selected.as_ref().map(|s| s.vertices.clone()), selection);
        assert_eq!(pen.pending(&s).is_some(), pending);
        assert!(pen.held);
    }
    assert!(!s.editor.can_undo());
}
