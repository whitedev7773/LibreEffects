use super::*;
use libre_effects_core::{
    ContentsParam, PathAnimation, Property, ShapeContents, ShapeParam, TrackEdit,
};

fn edit(s: &mut EditorState, edit: ContentsEdit) {
    s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
}
fn scene(paint: bool) -> EditorState {
    let mut s = EditorState::default();
    s.tool = Tool::Pen;
    s.composition_started = true;
    let c = s.editor.project().composition();
    s.editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents::default()),
            width: c.width() as f64,
            height: c.height() as f64,
            name: "Pen Contents".into(),
        })
        .unwrap();
    edit(
        &mut s,
        ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::Group(vec![]),
        },
    );
    if paint {
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::Fill { even_odd: false },
            },
        );
    }
    s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 1));
    s.editor.clear_history();
    s
}
fn contents(s: &EditorState) -> &ShapeContents {
    let Content::ShapeContents(c) = s.editor.project().composition().layer(1).unwrap().content()
    else {
        panic!()
    };
    c
}
fn children(s: &EditorState, id: u64) -> &Vec<libre_effects_core::ContentsNode> {
    let ContentsKind::Group(children) = &contents(s).node(id).unwrap().kind else {
        panic!()
    };
    children
}
fn select(s: &mut EditorState, id: u64) {
    s.contents_selection = Some((s.editor.project().active_composition_id(), 1, id));
}
fn click(pen: &mut Pen, s: &EditorState, p: [f64; 2]) -> Option<Command> {
    let command = pen.down(s, p, 1., false, false, false);
    assert!(pen.up(s).is_none());
    command
}
fn square(pen: &mut Pen, s: &EditorState) {
    for p in [[80., 80.], [240., 80.], [240., 240.], [80., 240.]] {
        assert!(click(pen, s, p).is_none());
    }
}
fn finish(pen: &mut Pen, s: &EditorState) -> Command {
    let (handled, command) = pen.key("enter", s);
    assert!(handled);
    command.unwrap()
}
fn path(s: &EditorState, group: u64) -> (&VectorPath, &PathAnimation) {
    let ContentsKind::Path { path, animation } = &children(s, group)[0].kind else {
        panic!()
    };
    (path, animation)
}
fn value(s: &mut EditorState, item: u64, parameter: ContentsParam, value: f64) {
    edit(
        s,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value { frame: 0, value },
        },
    );
}
fn layer_value(s: &mut EditorState, id: LayerId, property: Property, value: f64) {
    s.editor
        .execute(Command::SetValue {
            id,
            property,
            frame: 0,
            value,
        })
        .unwrap();
}
fn near(actual: [f64; 2], expected: [f64; 2]) {
    assert!(
        distance(actual, expected) < 1e-8,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn open_closed_and_curved_paths_use_fresh_ids_keep_paints_and_one_undo() {
    for closed in [false, true] {
        let mut s = scene(true);
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::Stroke(Default::default()),
            },
        );
        let paints = children(&s, 1).clone();
        let selection = s.contents_selection;
        s.editor.clear_history();
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        assert!(pen.down(&s, [80., 80.], 1., false, false, false).is_none());
        assert!(pen.release(&s, [100., 110.], false, false).is_none());
        for p in [[240., 80.], [240., 240.], [80., 240.]] {
            click(&mut pen, &s, p);
        }
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        let command = if closed {
            click(&mut pen, &s, [80., 80.]).unwrap()
        } else {
            finish(&mut pen, &s)
        };
        assert!(matches!(
            command,
            Command::Contents {
                id: 1,
                edit: ContentsEdit::Add { parent: 1, .. }
            }
        ));
        s.editor.execute(command).unwrap();
        assert_eq!(s.editor.project().composition().layers().len(), 1);
        assert_eq!(s.contents_selection, selection);
        assert_eq!(&children(&s, 1)[1..], &paints);
        assert_eq!(children(&s, 1)[0].id, 4);
        let (p, animation) = path(&s, 1);
        assert_eq!(p.closed, closed);
        assert_eq!(
            p.vertices[0],
            PathVertex {
                position: [80., 80.],
                incoming: [-20., -30.],
                outgoing: [20., 30.]
            }
        );
        assert_eq!(animation, &PathAnimation::default());
        let after = s.editor.project().clone();
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        s.editor.redo();
        assert_eq!(s.editor.project(), &after);
        let restored = Project::from_json(&after.to_json().unwrap()).unwrap();
        assert_eq!(restored, after);
    }
}

#[test]
fn repeated_creation_keeps_selected_group_and_existing_geometry() {
    let mut s = scene(false);
    let mut pen = Pen::default();
    for y in [80., 240.] {
        click(&mut pen, &s, [80., y]);
        click(&mut pen, &s, [240., y]);
        s.editor.execute(finish(&mut pen, &s)).unwrap();
    }
    assert_eq!(
        children(&s, 1).iter().map(|n| n.id).collect::<Vec<_>>(),
        [3, 2]
    );
    assert_eq!(path(&s, 1).0.vertices[0].position, [80., 240.]);
    assert_eq!(
        contents(&s).node(2).unwrap().path_at(0).unwrap().vertices[0].position,
        [80., 80.]
    );
    assert_eq!(s.contents_selection.unwrap().2, 1);
    assert_eq!(s.editor.selected(), Some(1));
    assert_eq!(s.editor.project().composition().layers().len(), 1);
}

#[test]
fn draft_and_commit_inherit_paint_without_mutating_source_and_roundtrip_pixels() {
    let mut s = scene(true);
    let renderer = crate::rendering::Renderer::new();
    let before = s.editor.project().clone();
    let original = renderer.render(&before, 0, 480).unwrap();
    assert!(original.pixels().all(|p| p[3] == 0));
    let mut pen = Pen::default();
    square(&mut pen, &s);
    let mut preview = libre_effects_core::Editor::default();
    preview.replace_project(before.clone()).unwrap();
    preview.execute(pen.pending(&s).unwrap()).unwrap();
    let draft = renderer.render_preview(preview.project(), 0, 480).unwrap();
    // Default composition is 1920 wide; these independent points are well
    // inside/outside the requested 80..240 square at quarter scale.
    assert_eq!(draft.get_pixel(40, 40).0, [255; 4]);
    assert_eq!(draft.get_pixel(15, 40)[3], 0);
    assert_eq!(s.editor.project(), &before);
    assert!(!s.editor.can_undo());
    assert_eq!(
        renderer.render(s.editor.project(), 0, 480).unwrap(),
        original
    );
    s.editor
        .execute(click(&mut pen, &s, [80., 80.]).unwrap())
        .unwrap();
    let restored = Project::from_json(&s.editor.project().to_json().unwrap()).unwrap();
    assert_eq!(&restored, s.editor.project());
    assert_eq!(renderer.render(&restored, 0, 480).unwrap(), draft);
    assert_eq!(renderer.render_preview(&restored, 0, 480).unwrap(), draft);
    s.editor.undo();
    assert_eq!(
        renderer.render(s.editor.project(), 0, 480).unwrap(),
        original
    );
}

#[test]
fn ancestor_paints_follow_existing_order_and_unpainted_paths_remain_editable() {
    let renderer = crate::rendering::Renderer::new();
    for applicable in [false, true] {
        let mut s = scene(true);
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::Group(vec![]),
            },
        );
        if !applicable {
            edit(
                &mut s,
                ContentsEdit::Move {
                    item: 3,
                    parent: 1,
                    index: 1,
                },
            );
        }
        select(&mut s, 3);
        let paints = contents(&s).node(2).unwrap().clone();
        let mut pen = Pen::default();
        square(&mut pen, &s);
        s.editor
            .execute(click(&mut pen, &s, [80., 80.]).unwrap())
            .unwrap();
        assert_eq!(contents(&s).node(2).unwrap(), &paints);
        assert_eq!(pen.overlay(&s).len(), 1);
        assert_eq!(path(&s, 3).0.vertices.len(), 4);
        let pixels = renderer.render(s.editor.project(), 0, 480).unwrap();
        assert_eq!(
            pixels.get_pixel(40, 40)[3],
            if applicable { 255 } else { 0 }
        );
        if !applicable {
            assert!(pixels.pixels().all(|p| p[3] == 0));
        }
    }
    for paint in [false, true] {
        let mut s = scene(paint);
        if paint {
            edit(
                &mut s,
                ContentsEdit::Enabled {
                    item: 2,
                    enabled: false,
                },
            );
        }
        let mut pen = Pen::default();
        square(&mut pen, &s);
        s.editor.execute(finish(&mut pen, &s)).unwrap();
        assert_eq!(pen.overlay(&s).len(), 1);
        assert!(
            renderer
                .render(s.editor.project(), 0, 480)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
    }
}

// Deliberately independent of Affine, ContentsNode::transform and layer
// world_transform: scalar operations supply the expected canvas coordinates.
fn scalar_transform(p: [f64; 2], v: [f64; 9]) -> [f64; 2] {
    let [x, y, ax, ay, sx, sy, rotation, skew, axis] = v;
    let rotate = |p: [f64; 2], degrees: f64| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos]
    };
    let mut p = rotate([(p[0] - ax) * sx, (p[1] - ay) * sy], axis);
    p[0] -= skew.to_radians().tan() * p[1];
    p = rotate(rotate(p, -axis), rotation);
    [p[0] + x, p[1] + y]
}
fn animate_contents(s: &mut EditorState, item: u64, parameter: ContentsParam, from: f64, to: f64) {
    value(s, item, parameter, from);
    edit(
        s,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        },
    );
    edit(
        s,
        ContentsEdit::Track {
            item,
            parameter,
            edit: TrackEdit::Value {
                frame: 20,
                value: to,
            },
        },
    );
}
fn animate_layer(s: &mut EditorState, id: LayerId, property: Property, from: f64, to: f64) {
    layer_value(s, id, property, from);
    s.editor
        .execute(Command::ToggleAnimation {
            id,
            property,
            frame: 0,
        })
        .unwrap();
    s.editor
        .execute(Command::SetValue {
            id,
            property,
            frame: 20,
            value: to,
        })
        .unwrap();
}
#[test]
fn nested_empty_group_and_animated_layer_parent_spaces_map_points_and_handles() {
    for frame in [0, 5, 10, 20] {
        let mut s = scene(true);
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::Group(vec![]),
            },
        );
        for (item, parameter, amount) in [
            (3, ContentsParam::Transform(Property::AnchorX), 3.),
            (3, ContentsParam::Transform(Property::AnchorY), 4.),
            (3, ContentsParam::Transform(Property::PositionY), 20.),
            (3, ContentsParam::Transform(Property::ScaleX), -120.),
            (3, ContentsParam::Transform(Property::ScaleY), 80.),
            (3, ContentsParam::Transform(Property::Rotation), 17.),
            (3, ContentsParam::SkewAxis, 37.),
            (1, ContentsParam::Transform(Property::PositionX), 80.),
            (1, ContentsParam::Transform(Property::PositionY), 60.),
            (1, ContentsParam::Transform(Property::ScaleY), 150.),
        ] {
            value(&mut s, item, parameter, amount);
        }
        animate_contents(
            &mut s,
            3,
            ContentsParam::Transform(Property::PositionX),
            10.,
            30.,
        );
        animate_contents(&mut s, 3, ContentsParam::Skew, 10., 30.);
        animate_contents(
            &mut s,
            1,
            ContentsParam::Transform(Property::Rotation),
            0.,
            60.,
        );
        for property in [
            Property::AnchorX,
            Property::AnchorY,
            Property::PositionX,
            Property::PositionY,
        ] {
            layer_value(&mut s, 1, property, 0.);
        }
        s.editor.execute(Command::AddNull).unwrap();
        layer_value(&mut s, 2, Property::PositionX, 0.);
        layer_value(&mut s, 2, Property::PositionY, 0.);
        s.editor
            .execute(Command::SetParent {
                id: 1,
                parent: Some(2),
                frame: 0,
            })
            .unwrap();
        layer_value(&mut s, 1, Property::PositionX, 40.);
        layer_value(&mut s, 1, Property::PositionY, 30.);
        animate_layer(&mut s, 1, Property::Rotation, 0., 20.);
        animate_layer(&mut s, 2, Property::ScaleX, 100., -200.);
        animate_layer(&mut s, 2, Property::PositionX, 160., 200.);
        layer_value(&mut s, 2, Property::PositionY, 120.);
        layer_value(&mut s, 2, Property::Rotation, -23.);
        layer_value(&mut s, 2, Property::ScaleY, 75.);
        s.editor.select(1);
        select(&mut s, 3);
        s.frame = frame;
        s.editor.clear_history();
        let f = f64::from(frame);
        let expected = |p| {
            let p = scalar_transform(p, [10. + f, 20., 3., 4., -1.2, 0.8, 17., 10. + f, 37.]);
            let p = scalar_transform(p, [80., 60., 0., 0., 1., 1.5, 3. * f, 0., 0.]);
            let p = scalar_transform(p, [40., 30., 0., 0., 1., 1., f, 0., 0.]);
            scalar_transform(
                p,
                [
                    160. + 2. * f,
                    120.,
                    0.,
                    0.,
                    1. - 0.15 * f,
                    0.75,
                    -23.,
                    0.,
                    0.,
                ],
            )
        };
        let mut pen = Pen::default();
        pen.down(&s, expected([20., 30.]), 1., false, false, false);
        pen.release(&s, expected([32., 48.]), false, false);
        click(&mut pen, &s, expected([120., 130.]));
        let command = finish(&mut pen, &s);
        s.editor.execute(command).unwrap();
        let (p, animation) = path(&s, 3);
        near(p.vertices[0].position, [20., 30.]);
        near(p.vertices[0].outgoing, [12., 18.]);
        near(p.vertices[0].incoming, [-12., -18.]);
        near(p.vertices[1].position, [120., 130.]);
        assert_eq!(animation, &PathAnimation::default());
        let overlay = pen.overlay(&s);
        near(
            overlay[0].1.point(p.vertices[0].position),
            expected([20., 30.]),
        );
        assert_eq!(s.editor.project().composition().layers().len(), 2);
    }
}

#[test]
fn ctrl_empty_canvas_still_creates_mask_and_non_group_selections_still_create_layers() {
    let mut s = scene(true);
    let mut pen = Pen::default();
    assert!(pen.down(&s, [80., 80.], 1., false, false, true).is_none());
    pen.up(&s);
    click(&mut pen, &s, [240., 80.]);
    click(&mut pen, &s, [240., 240.]);
    s.editor.execute(finish(&mut pen, &s)).unwrap();
    assert_eq!(s.editor.selected_layer().unwrap().path_masks().len(), 1);
    assert_eq!(children(&s, 1).len(), 1);
    assert_eq!(s.editor.project().composition().layers().len(), 1);
    for selection in [None, Some((1, 1, 2)), Some((2, 1, 1)), Some((1, 999, 1))] {
        let mut s = scene(true);
        s.contents_selection = selection;
        let mut pen = Pen::default();
        click(&mut pen, &s, [80., 80.]);
        click(&mut pen, &s, [240., 80.]);
        let command = finish(&mut pen, &s);
        assert!(matches!(command, Command::AddContent { .. }));
        s.editor.execute(command).unwrap();
        assert_eq!(s.editor.project().composition().layers().len(), 2);
        assert_eq!(children(&s, 1).len(), 1);
    }
}

#[test]
fn selected_group_keeps_vertex_handle_curve_and_multiselect_hit_precedence() {
    let mut s = scene(true);
    let mut p = VectorPath {
        vertices: [[80., 80.], [240., 80.], [240., 240.], [80., 240.]]
            .into_iter()
            .map(PathVertex::corner)
            .collect(),
        closed: true,
    };
    p.vertices[0].outgoing = [40., 0.];
    edit(
        &mut s,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Path {
                path: p.clone(),
                animation: Default::default(),
            },
        },
    );
    s.editor.clear_history();
    let mut pen = Pen::default();
    click(&mut pen, &s, p.vertices[0].position);
    pen.down(&s, p.vertices[1].position, 1., false, true, false);
    pen.up(&s);
    assert_eq!(pen.selected.as_ref().unwrap().vertices, [0, 1].into());
    assert!(pen.draft.is_none());
    pen.down(&s, p.vertices[0].position, 1., false, false, true);
    assert!(pen.draft.is_none());
    assert!(matches!(
        pen.drag.as_ref().unwrap().session.target,
        Target::Contents(1, 3)
    ));
    pen.release(&s, [90., 100.], false, false);
    pen.cancel();
    pen.down(&s, [120., 80.], 1., false, false, false);
    assert!(matches!(pen.drag.as_ref().unwrap().part, Part::Outgoing));
    assert!(pen.draft.is_none());
    pen.cancel();
    pen.down(&s, [240., 160.], 1., false, false, false);
    assert!(matches!(
        pen.drag.as_ref().unwrap().session.target,
        Target::Contents(1, 3)
    ));
    s.editor.execute(pen.up(&s).unwrap()).unwrap();
    assert_eq!(path(&s, 1).0.vertices.len(), 5);
    assert_eq!(children(&s, 1).len(), 2);
    assert_eq!(s.editor.project().composition().layers().len(), 1);
}

#[test]
fn selected_group_draft_cancels_on_escape_focus_and_every_captured_context_change() {
    let mutations: &[fn(&mut EditorState)] = &[
        |s| s.frame += 1,
        |s| s.tool = Tool::Select,
        |s| s.document_revision += 1,
        |s| s.contents_selection = None,
        |s| select(s, 2),
        |s| {
            s.selected_layers.insert(1);
        },
        |s| s.editor.clear_selection(),
        |s| layer_value(s, 1, Property::Rotation, 5.),
        |s| {
            edit(s, ContentsEdit::Remove(1));
        },
        |s| {
            s.editor.execute(Command::ToggleLocked(1)).unwrap();
        },
        |s| {
            s.editor.execute(Command::NewComposition).unwrap();
        },
    ];
    for mutate in mutations {
        let mut s = scene(true);
        let mut pen = Pen::default();
        square(&mut pen, &s);
        assert!(pen.pending(&s).is_some());
        mutate(&mut s);
        let after = s.editor.project().clone();
        assert!(pen.pending(&s).is_none());
        assert!(pen.key("enter", &s).1.is_none());
        assert!(pen.draft.is_none());
        assert_eq!(s.editor.project(), &after);
    }
    for escape in [false, true] {
        let s = scene(true);
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        square(&mut pen, &s);
        if escape {
            assert!(pen.key("escape", &s).0);
        } else {
            pen.cancel();
        }
        assert!(pen.pending(&s).is_none());
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
}

#[test]
fn disabled_missing_locked_and_singular_groups_never_create_fallback_layers() {
    let mutations: &[fn(&mut EditorState)] = &[
        |s| {
            edit(
                s,
                ContentsEdit::Enabled {
                    item: 3,
                    enabled: false,
                },
            )
        },
        |s| {
            edit(
                s,
                ContentsEdit::Enabled {
                    item: 1,
                    enabled: false,
                },
            )
        },
        |s| select(s, 999),
        |s| value(s, 3, ContentsParam::Transform(Property::ScaleX), 0.),
        |s| value(s, 1, ContentsParam::Transform(Property::ScaleY), 0.),
        |s| layer_value(s, 1, Property::ScaleX, 0.),
        |s| {
            s.editor.execute(Command::AddNull).unwrap();
            s.editor
                .execute(Command::SetParent {
                    id: 1,
                    parent: Some(2),
                    frame: 0,
                })
                .unwrap();
            layer_value(s, 2, Property::ScaleX, 0.);
            s.editor.select(1);
        },
        |s| {
            s.editor.execute(Command::ToggleLocked(1)).unwrap();
        },
    ];
    for mutate in mutations {
        let mut s = scene(true);
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::Group(vec![]),
            },
        );
        select(&mut s, 3);
        mutate(&mut s);
        s.editor.clear_history();
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        square(&mut pen, &s);
        assert!(pen.draft.is_none());
        assert!(pen.pending(&s).is_none());
        assert!(pen.key("enter", &s).1.is_none());
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
}

#[test]
fn contents_item_limit_and_invalid_coordinates_preserve_source_selection_and_redo() {
    let mut s = scene(false);
    for _ in 0..255 {
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::Fill { even_odd: false },
            },
        );
    }
    s.editor.clear_history();
    layer_value(&mut s, 1, Property::Rotation, 1.);
    s.editor.undo();
    let before = s.editor.project().clone();
    let selection = s.contents_selection;
    let mut pen = Pen::default();
    square(&mut pen, &s);
    assert!(s.editor.execute(finish(&mut pen, &s)).is_err());
    assert_eq!(s.editor.project(), &before);
    assert_eq!(s.contents_selection, selection);
    assert_eq!(s.editor.selected(), Some(1));
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
    for invalid in [f64::NAN, f64::INFINITY, 1_000_001.] {
        let s = scene(false);
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        click(&mut pen, &s, [80., 80.]);
        click(&mut pen, &s, [invalid, 100.]);
        assert!(pen.pending(&s).is_none());
        assert!(pen.key("enter", &s).1.is_none());
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        pen.cancel();
    }
}

#[test]
fn creation_preserves_existing_layer_visibility_and_time_range_policy() {
    for hidden in [false, true] {
        let mut s = scene(true);
        if hidden {
            s.editor.execute(Command::ToggleVisible(1)).unwrap();
        } else {
            s.editor
                .execute(Command::SetLayerRange {
                    id: 1,
                    start: 10,
                    end: 30,
                })
                .unwrap();
        }
        s.editor.clear_history();
        let mut pen = Pen::default();
        square(&mut pen, &s);
        s.editor.execute(finish(&mut pen, &s)).unwrap();
        assert_eq!(path(&s, 1).0.vertices.len(), 4);
        assert!(
            crate::rendering::Renderer::new()
                .render(s.editor.project(), 0, 480)
                .unwrap()
                .pixels()
                .all(|p| p[3] == 0)
        );
    }
}

#[test]
fn open_path_inherits_existing_stroke_and_renders_independent_center_samples() {
    let mut s = scene(false);
    edit(
        &mut s,
        ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::Stroke(Default::default()),
        },
    );
    value(
        &mut s,
        2,
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        16.,
    );
    let mut pen = Pen::default();
    click(&mut pen, &s, [80., 160.]);
    click(&mut pen, &s, [240., 160.]);
    s.editor.execute(finish(&mut pen, &s)).unwrap();
    assert!(!path(&s, 1).0.closed);
    let pixels = crate::rendering::Renderer::new()
        .render(s.editor.project(), 0, 480)
        .unwrap();
    assert_eq!(pixels.get_pixel(40, 40).0, [255; 4]);
    assert_eq!(pixels.get_pixel(40, 35)[3], 0);
    assert_eq!(pixels.get_pixel(15, 40)[3], 0);
}

#[test]
fn exhausted_contents_ids_reject_atomically_without_losing_redo() {
    let mut s = scene(false);
    let mut json = serde_json::to_value(contents(&s)).unwrap();
    json["next_id"] = serde_json::json!(u64::MAX);
    s.editor
        .execute(Command::SetContent {
            id: 1,
            content: Content::ShapeContents(serde_json::from_value(json).unwrap()),
        })
        .unwrap();
    s.editor.clear_history();
    layer_value(&mut s, 1, Property::Rotation, 1.);
    s.editor.undo();
    let before = s.editor.project().clone();
    let selection = s.contents_selection;
    let mut pen = Pen::default();
    square(&mut pen, &s);
    let command = finish(&mut pen, &s);
    assert!(
        s.editor
            .execute(command)
            .unwrap_err()
            .contains("ID exhausted")
    );
    assert_eq!(s.editor.project(), &before);
    assert_eq!(s.contents_selection, selection);
    assert!(s.editor.can_redo());
    assert!(!s.editor.can_undo());
}

#[test]
fn vertex_limit_is_bounded_and_invalid_handle_cannot_commit() {
    let mut s = scene(false);
    let mut pen = Pen::default();
    for i in 0..1025 {
        click(&mut pen, &s, [20. + f64::from(i) * 8., 100.]);
    }
    assert_eq!(pen.draft.as_ref().unwrap().path.vertices.len(), 1024);
    s.editor.execute(finish(&mut pen, &s)).unwrap();
    assert_eq!(path(&s, 1).0.vertices.len(), 1024);
    for invalid in [f64::NAN, f64::INFINITY, 1_000_001.] {
        let s = scene(false);
        let before = s.editor.project().clone();
        let mut pen = Pen::default();
        click(&mut pen, &s, [80., 80.]);
        pen.down(&s, [240., 80.], 1., false, false, false);
        pen.release(&s, [240. + invalid, 80.], false, false);
        assert!(pen.pending(&s).is_none());
        assert!(pen.key("enter", &s).1.is_none());
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
}
