use super::*;
use libre_effects_core::{ContentsParam, KeyRef, Property, PropertyPath, ShapePaint, TrackEdit};

fn event(chord: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: gpui::Keystroke::parse(chord).unwrap(),
        is_held: false,
    }
}
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
fn geometry(s: &EditorState, target: Target) -> (VectorPath, Affine) {
    let (_, path, world) = paths(s).into_iter().find(|(t, _, _)| *t == target).unwrap();
    (path, world)
}
fn scene(
    kind: usize,
    closed: bool,
    animated: bool,
    transformed: bool,
) -> (EditorState, Target, PathTarget) {
    let mut s = EditorState::default();
    s.tool = Tool::Pen;
    s.composition_started = true;
    let comp = s.editor.project().composition();
    s.editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(polygon(closed)),
                fill: closed,
                ..Default::default()
            }),
            width: comp.width() as f64,
            height: comp.height() as f64,
            name: "Marquee test".into(),
        })
        .unwrap();
    let (target, core) = match kind {
        0 => (Target::Shape(1), PathTarget::Shape),
        1 => {
            contents(&mut s, ContentsEdit::Promote);
            if transformed {
                contents(
                    &mut s,
                    ContentsEdit::Add {
                        parent: 0,
                        kind: ContentsKind::Group(vec![]),
                    },
                );
                contents(
                    &mut s,
                    ContentsEdit::Move {
                        item: 1,
                        parent: 5,
                        index: 0,
                    },
                );
                contents(
                    &mut s,
                    ContentsEdit::Track {
                        item: 5,
                        parameter: ContentsParam::Transform(Property::Rotation),
                        edit: TrackEdit::Value {
                            frame: 0,
                            value: 13.,
                        },
                    },
                );
                if animated {
                    for edit in [
                        TrackEdit::ToggleAnimation { frame: 0 },
                        TrackEdit::Value {
                            frame: 20,
                            value: 37.,
                        },
                    ] {
                        contents(
                            &mut s,
                            ContentsEdit::Track {
                                item: 5,
                                parameter: ContentsParam::Transform(Property::Rotation),
                                edit,
                            },
                        );
                    }
                }
                for (parameter, value) in [
                    (ContentsParam::Skew, 24.),
                    (ContentsParam::SkewAxis, 39.),
                    (ContentsParam::Transform(Property::Rotation), -11.),
                ] {
                    contents(
                        &mut s,
                        ContentsEdit::Track {
                            item: 1,
                            parameter,
                            edit: TrackEdit::Value { frame: 0, value },
                        },
                    );
                }
            }
            (Target::Contents(1, 2), PathTarget::Contents(2))
        }
        _ => {
            let mut source = polygon(true);
            for v in &mut source.vertices {
                v.position[0] += 700.;
            }
            s.editor
                .execute(Command::EditPath {
                    id: 1,
                    target: PathTarget::Shape,
                    frame: 0,
                    path: source,
                })
                .unwrap();
            s.editor
                .execute(Command::SetPathMasks {
                    id: 1,
                    masks: vec![
                        PathMask {
                            path: polygon(true),
                            ..Default::default()
                        },
                        PathMask {
                            path: polygon(true),
                            ..Default::default()
                        },
                    ],
                })
                .unwrap();
            // A non-index stable mask ID catches accidental index-based edits.
            let masks = s.editor.selected_layer().unwrap().path_masks()[1..].to_vec();
            s.editor
                .execute(Command::SetPathMasks { id: 1, masks })
                .unwrap();
            (Target::Mask(1, 0), PathTarget::Mask(2))
        }
    };
    if transformed {
        s.editor.execute(Command::AddNull).unwrap();
        s.editor
            .execute(Command::SetParent {
                id: 1,
                parent: Some(2),
                frame: 0,
            })
            .unwrap();
        value(&mut s, 2, Property::Rotation, 31.);
        if animated {
            s.editor
                .execute(Command::ToggleAnimation {
                    id: 2,
                    property: Property::Rotation,
                    frame: 0,
                })
                .unwrap();
            s.editor
                .execute(Command::SetValue {
                    id: 2,
                    property: Property::Rotation,
                    frame: 20,
                    value: 55.,
                })
                .unwrap();
        }
        value(&mut s, 2, Property::ScaleX, -125.);
        value(&mut s, 1, Property::ScaleY, 80.);
        s.editor.select(1);
    }
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
            v.position = add(v.position, [40., 30.]);
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
fn overlay(pen: &Pen, s: &EditorState, target: Target) -> BTreeSet<usize> {
    let (path, world) = geometry(s, target);
    pen.overlay(s)
        .into_iter()
        .find(|(p, w, _, _)| *p == path && *w == world)
        .unwrap()
        .3
}
fn begin(pen: &mut Pen, s: &EditorState, p: [f64; 2], zoom: f64) {
    assert!(pen.down(s, p, zoom, false, true, false).is_none());
    assert!(pen.marquee.is_some());
    assert!(pen.drag.is_none());
    assert!(pen.draft.is_none());
    assert!(pen.pending(s).is_none());
}
fn box_vertex(pen: &mut Pen, s: &EditorState, target: Target, index: usize) {
    let (path, world) = geometry(s, target);
    let p = world.point(path.vertices[index].position);
    begin(pen, s, add(p, [-12., -12.]), 1.);
    assert!(pen.release(s, add(p, [12., 12.]), false, true).is_none());
}
fn redo(s: &mut EditorState) {
    value(s, 1, Property::Opacity, 40.);
    s.editor.undo();
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
}
struct Unchanged {
    project: Project,
    json: String,
    autosave_json: String,
    dirty: bool,
    history: (bool, bool),
    keys: BTreeSet<KeyRef>,
    revisions: (u64, u64),
}
impl Unchanged {
    fn new(s: &EditorState) -> Self {
        Self {
            project: s.editor.project().clone(),
            json: s.editor.project().to_json().unwrap(),
            // This is the source snapshot used by the autosave loop.
            autosave_json: s.text_project().to_json().unwrap(),
            dirty: s.dirty(),
            history: (s.editor.can_undo(), s.editor.can_redo()),
            keys: s.selected_keys.clone(),
            revisions: (s.document_revision, s.preview_revision),
        }
    }
    fn check(&self, s: &EditorState) {
        assert_eq!(s.editor.project(), &self.project);
        assert_eq!(s.editor.project().to_json().unwrap(), self.json);
        assert_eq!(s.text_project().to_json().unwrap(), self.autosave_json);
        assert_eq!(s.dirty(), self.dirty);
        assert_eq!((s.editor.can_undo(), s.editor.can_redo()), self.history);
        assert_eq!(s.selected_keys, self.keys);
        assert_eq!((s.document_revision, s.preview_revision), self.revisions);
    }
}

#[test]
fn marquee_and_select_all_are_transient_for_every_static_animated_and_transformed_target() {
    for kind in 0..3 {
        for animated in [false, true] {
            for transformed in [false, true] {
                let (mut s, target, core) = scene(kind, true, animated, transformed);
                redo(&mut s);
                s.selected_keys = [KeyRef {
                    id: 1,
                    property: PropertyPath::Path(core),
                    frame: 0,
                }]
                .into();
                let before = Unchanged::new(&s);
                let mut pen = Pen::default();
                pick(&mut pen, &s, target, 0, false);
                let (path, world) = geometry(&s, target);
                let p = world.point(path.vertices[3].position);
                begin(&mut pen, &s, add(p, [-12., -12.]), 1.);
                pen.moving(add(p, [12., 12.]), false, true);
                assert_eq!(selected(&pen), [0]);
                assert_eq!(overlay(&pen, &s, target), [0, 3].into());
                assert!(pen.pending(&s).is_none());
                before.check(&s);
                assert!(pen.up(&s).is_none());
                assert_eq!(selected(&pen), [0, 3]);
                assert!(pen.marquee.is_none());
                before.check(&s);
                assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
                assert_eq!(selected(&pen), [0, 1, 2, 3, 4, 5]);
                before.check(&s);
            }
        }
    }
}

#[test]
fn additive_boxes_are_repeatable_and_empty_boxes_preserve_selection() {
    let (s, target, _) = scene(0, true, false, false);
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 0, false);
    for (index, expected) in [(3, vec![0, 3]), (3, vec![0, 3]), (4, vec![0, 3, 4])] {
        box_vertex(&mut pen, &s, target, index);
        assert_eq!(selected(&pen), expected);
    }
    begin(&mut pen, &s, [400., 400.], 1.);
    assert!(pen.release(&s, [600., 600.], false, true).is_none());
    assert_eq!(selected(&pen), [0, 3, 4]);
}

#[test]
fn normalized_rectangles_work_in_all_four_pointer_directions() {
    for (start, end) in [
        ([0., 0.], [140., 40.]),
        ([140., 0.], [0., 40.]),
        ([0., 40.], [140., 0.]),
        ([140., 40.], [0., 0.]),
    ] {
        let (s, target, _) = scene(0, true, false, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 5, false);
        begin(&mut pen, &s, start, 1.);
        assert!(pen.release(&s, end, false, true).is_none());
        assert_eq!(selected(&pen), [0, 1, 5]);
    }
}

#[test]
fn four_screen_pixel_max_axis_threshold_is_zoom_scaled_and_latches() {
    for zoom in [0.25, 1., 2., 8.] {
        let (s, target, _) = scene(0, true, false, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 0, false);
        let start = [700., 600.];
        begin(&mut pen, &s, start, zoom);
        // Diagonal distance exceeds four; neither screen axis has reached four.
        pen.moving(add(start, [3.9 / zoom, -3.9 / zoom]), false, true);
        assert!(!pen.marquee.as_ref().unwrap().moved);
        assert_eq!(overlay(&pen, &s, target), [0].into());
        pen.moving(add(start, [4. / zoom, 0.]), false, true);
        assert!(pen.marquee.as_ref().unwrap().moved);
        pen.moving(start, false, true);
        assert!(pen.marquee.as_ref().unwrap().moved);
        assert_eq!(pen.marquee.as_ref().unwrap().bounds(), [start, start]);
        assert!(pen.up(&s).is_none());
        assert_eq!(selected(&pen), [0]);
    }
}

#[test]
fn inclusive_edges_and_zero_height_or_zero_width_boxes_select_anchor_centers() {
    for (start, end, expected) in [
        ([0., 20.], [120., 20.], vec![0, 1, 4]),
        ([20., 0.], [20., 220.], vec![0, 4, 5]),
        ([0., 0.], [120., 20.], vec![0, 1, 4]),
        ([120., 40.], [0., 20.], vec![0, 1, 4]),
    ] {
        let (s, target, _) = scene(0, true, false, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 4, false);
        begin(&mut pen, &s, start, 1.);
        assert!(pen.release(&s, end, false, true).is_none());
        assert_eq!(selected(&pen), expected);
    }
}

#[test]
fn rectangle_hits_only_anchors_and_recomputes_from_current_box_without_accumulation() {
    let (mut s, target, _) = scene(0, true, false, false);
    let mut path = geometry(&s, target).0;
    path.vertices[0].outgoing = [80., 80.];
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target: PathTarget::Shape,
            frame: 0,
            path,
        })
        .unwrap();
    s.editor.clear_history();
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 5, false);
    begin(&mut pen, &s, [80., 80.], 1.);
    pen.moving([110., 110.], false, true);
    assert_eq!(overlay(&pen, &s, target), [5].into()); // tangent endpoint only
    assert!(pen.up(&s).is_none());
    begin(&mut pen, &s, [50., 10.], 1.);
    pen.moving([90., 50.], false, true);
    assert_eq!(overlay(&pen, &s, target), [5].into()); // curve without anchors
    pen.moving([240., 240.], false, true);
    assert_eq!(overlay(&pen, &s, target), [1, 2, 3, 4, 5].into());
    pen.moving([90., 50.], false, true);
    assert_eq!(overlay(&pen, &s, target), [5].into());
    assert_eq!(selected(&pen), [5]);
    assert!(pen.up(&s).is_none());
    assert_eq!(selected(&pen), [5]);
}

#[test]
fn known_empty_target_can_marquee_and_select_all_but_no_target_cannot() {
    let (s, target, _) = scene(0, true, false, false);
    let mut pen = Pen::default();
    assert!(pen.down(&s, [0., 0.], 1., false, true, false).is_none());
    pen.moving([240., 240.], false, true);
    assert!(pen.up(&s).is_none());
    assert!(pen.marquee.is_none() && pen.selected.is_none() && pen.draft.is_none());
    assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
    assert!(pen.selected.is_none());
    pick(&mut pen, &s, target, 0, false);
    pick(&mut pen, &s, target, 0, true);
    assert!(selected(&pen).is_empty());
    box_vertex(&mut pen, &s, target, 3);
    assert_eq!(selected(&pen), [3]);
    pick(&mut pen, &s, target, 3, true);
    assert!(selected(&pen).is_empty());
    assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
    assert_eq!(selected(&pen), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn final_release_coordinates_commit_without_a_final_move_event() {
    let (s, target, _) = scene(0, true, false, false);
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 5, false);
    begin(&mut pen, &s, [0., 0.], 1.);
    assert!(pen.release(&s, [140., 40.], false, false).is_none());
    assert_eq!(selected(&pen), [0, 1, 5]);
    begin(&mut pen, &s, [200., 200.], 1.);
    pen.moving([240., 240.], false, true);
    assert_eq!(overlay(&pen, &s, target), [0, 1, 3, 5].into());
    assert!(pen.release(&s, [201., 201.], true, false).is_none());
    assert_eq!(selected(&pen), [0, 1, 5]);
}

#[test]
fn second_down_discards_provisional_selection_and_begins_from_committed_set() {
    let (s, target, _) = scene(0, true, false, false);
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 5, false);
    begin(&mut pen, &s, [0., 0.], 1.);
    pen.moving([140., 40.], false, true);
    assert_eq!(overlay(&pen, &s, target), [0, 1, 5].into());
    begin(&mut pen, &s, [200., 200.], 1.);
    assert!(pen.release(&s, [240., 240.], false, true).is_none());
    assert_eq!(selected(&pen), [3, 5]);
    begin(&mut pen, &s, [0., 0.], 1.);
    pen.moving([140., 40.], false, true);
    pick(&mut pen, &s, target, 2, false);
    assert!(pen.marquee.is_none());
    assert_eq!(selected(&pen), [2]);
}

#[test]
fn marquee_and_select_all_span_contents_domain_but_keep_masks_on_known_path() {
    for kind in [1, 2] {
        let (mut s, target, _) = scene(kind, true, false, false);
        if kind == 1 {
            let mut other = polygon(true);
            for v in &mut other.vertices {
                v.position[0] += 500.;
            }
            contents(
                &mut s,
                ContentsEdit::Add {
                    parent: 1,
                    kind: ContentsKind::Path {
                        path: other,
                        animation: Default::default(),
                    },
                },
            );
        }
        s.editor.clear_history();
        let before = Unchanged::new(&s);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 0, false);
        begin(&mut pen, &s, [-100., -100.], 1.);
        pen.moving([1500., 1500.], false, true);
        assert_eq!(
            pen.overlay(&s)
                .iter()
                .filter(|(_, _, _, indices)| !indices.is_empty())
                .count(),
            if kind == 1 { 2 } else { 1 }
        );
        assert!(pen.up(&s).is_none());
        assert!(pen.selected.as_ref().unwrap().target == target);
        assert_eq!(selected(&pen), [0, 1, 2, 3, 4, 5]);
        pick(&mut pen, &s, target, 0, true);
        assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
        assert!(pen.selected.as_ref().unwrap().target == target);
        assert_eq!(
            pen.overlay(&s)
                .iter()
                .filter(|(_, _, _, indices)| !indices.is_empty())
                .count(),
            if kind == 1 { 2 } else { 1 }
        );
        before.check(&s);
    }
}

#[test]
fn stale_marquee_context_cancels_before_release_and_never_publishes_indices() {
    for mode in 0..9 {
        let (mut s, target, _) = scene(1, true, false, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 5, false);
        begin(&mut pen, &s, [0., 0.], 1.);
        pen.moving([140., 40.], false, true);
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
            7 => value(&mut s, 1, Property::Opacity, 30.),
            _ => {
                s.editor
                    .replace_project(s.editor.project().clone())
                    .unwrap();
                s.document_revision += 1;
            }
        }
        let before = Unchanged::new(&s);
        assert!(pen.pending(&s).is_none());
        assert!(
            pen.overlay(&s)
                .iter()
                .all(|(_, _, _, vertices)| vertices.is_empty())
        );
        assert!(pen.release(&s, [240., 240.], false, true).is_none());
        assert!(pen.marquee.is_none() && pen.selected.is_none());
        assert!(!pen.held);
        before.check(&s);
    }
}

#[test]
fn locked_disabled_and_singular_paths_never_start_box_or_select_all() {
    for mode in 0..5 {
        let (mut s, target, _) = scene(1, true, false, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 2, false);
        match mode {
            0 => {
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
            }
            1 => contents(
                &mut s,
                ContentsEdit::Enabled {
                    item: 1,
                    enabled: false,
                },
            ),
            2 => contents(
                &mut s,
                ContentsEdit::Enabled {
                    item: 2,
                    enabled: false,
                },
            ),
            3 => value(&mut s, 1, Property::ScaleX, 0.),
            _ => contents(
                &mut s,
                ContentsEdit::Track {
                    item: 1,
                    parameter: ContentsParam::Transform(Property::ScaleY),
                    edit: TrackEdit::Value {
                        frame: 0,
                        value: 0.,
                    },
                },
            ),
        }
        // Even a current context cannot make an unavailable path editable.
        pen.selected_context = Some(Context::capture(&s));
        let before = Unchanged::new(&s);
        assert!(
            pen.down(&s, [-100., -100.], 1., false, true, false)
                .is_none()
        );
        assert!(pen.marquee.is_none());
        assert!(pen.release(&s, [500., 500.], false, true).is_none());
        assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
        assert_eq!(selected(&pen), [2]);
        before.check(&s);
    }
}

#[test]
fn escape_and_focus_loss_discard_committed_and_provisional_selection() {
    for escape in [false, true] {
        let (mut s, target, _) = scene(0, true, false, false);
        redo(&mut s);
        let before = Unchanged::new(&s);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 5, false);
        begin(&mut pen, &s, [0., 0.], 1.);
        pen.moving([140., 40.], false, true);
        if escape {
            assert!(matches!(pen.key("escape", &s), (true, None)));
        } else {
            // Preview invokes cancel on blur/window deactivation.
            pen.cancel();
        }
        assert!(pen.marquee.is_none() && pen.selected.is_none());
        assert!(pen.pending(&s).is_none());
        assert!(pen.release(&s, [240., 240.], false, true).is_none());
        before.check(&s);
    }
}

#[test]
fn enter_and_order_shortcuts_are_consumed_while_marquee_is_provisional() {
    let (mut s, target, _) = scene(0, true, false, false);
    redo(&mut s);
    let before = Unchanged::new(&s);
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 5, false);
    begin(&mut pen, &s, [0., 0.], 1.);
    pen.moving([140., 40.], false, true);
    assert!(matches!(pen.key("enter", &s), (true, None)));
    for chord in ["shift-f", "shift-r"] {
        assert!(matches!(
            pen.order_key(&event(chord), true, false, &s),
            (true, None)
        ));
    }
    assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
    assert!(pen.marquee.is_some());
    assert_eq!(selected(&pen), [5]);
    assert_eq!(overlay(&pen, &s, target), [0, 1, 5].into());
    assert!(pen.pending(&s).is_none());
    assert!(pen.up(&s).is_none());
    assert_eq!(selected(&pen), [0, 1, 5]);
    before.check(&s);
}

#[test]
fn delete_and_backspace_cancel_box_without_deleting_committed_or_provisional_vertices() {
    for key in ["delete", "backspace"] {
        let (mut s, target, _) = scene(0, true, false, false);
        redo(&mut s);
        let before = Unchanged::new(&s);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 5, false);
        begin(&mut pen, &s, [0., 0.], 1.);
        pen.moving([140., 40.], false, true);
        assert!(matches!(pen.key(key, &s), (true, None)));
        assert!(pen.marquee.is_none());
        assert_eq!(selected(&pen), [5]);
        assert_eq!(overlay(&pen, &s, target), [5].into());
        assert!(pen.release(&s, [240., 240.], false, true).is_none());
        before.check(&s);
        // The cancellation consumed only that keypress; the next explicit edit works.
        assert!(matches!(
            pen.key(key, &s),
            (true, Some(Command::EditPath { .. }))
        ));
    }
}

#[test]
fn select_all_requires_exact_control_a_canvas_focus_and_no_ime() {
    let (s, target, _) = scene(0, true, false, false);
    for mode in 0..9 {
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 2, false);
        let mut key = event("ctrl-a");
        let mut focused = true;
        let mut composing = false;
        match mode {
            0 => key.keystroke.modifiers.control = false,
            1 => key.keystroke.modifiers.shift = true,
            2 => key.keystroke.modifiers.alt = true,
            3 => key.keystroke.modifiers.platform = true,
            4 => key.keystroke.modifiers.function = true,
            5 => focused = false,
            6 => composing = true,
            7 => key.keystroke.key = "b".into(),
            _ => {
                key.keystroke.modifiers.control = false;
                key.keystroke.modifiers.platform = true;
            }
        }
        assert!(!pen.select_all_key(&key, focused, composing, &s));
        assert_eq!(selected(&pen), [2]);
    }
    for key in ["a", "A"] {
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 2, false);
        let mut e = event("ctrl-a");
        e.keystroke.key = key.into();
        assert!(pen.select_all_key(&e, true, false, &s));
        assert_eq!(selected(&pen), [0, 1, 2, 3, 4, 5]);
    }
}

#[test]
fn select_all_repeats_held_pointers_and_unpublished_drafts_are_consumed_without_changes() {
    for mode in 0..4 {
        let (s, target, _) = scene(0, true, false, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 2, false);
        let mut e = event("ctrl-a");
        match mode {
            0 => e.is_held = true,
            1 => pen.held = true,
            2 => {
                pen.down(&s, [220., 20.], 1., false, false, false);
                pen.moving([250., 50.], false, false);
            }
            _ => {
                for p in [[700., 600.], [800., 600.], [800., 700.]] {
                    pen.down(&s, p, 1., false, false, false);
                    pen.up(&s);
                }
            }
        }
        let selection = pen.selected.as_ref().map(|v| v.vertices.clone());
        let draft = pen.draft.as_ref().map(|v| v.path.clone());
        let drag = pen.drag.as_ref().map(|v| v.session.path.clone());
        assert!(pen.select_all_key(&e, true, false, &s));
        assert_eq!(pen.selected.as_ref().map(|v| v.vertices.clone()), selection);
        assert_eq!(pen.draft.as_ref().map(|v| v.path.clone()), draft);
        assert_eq!(pen.drag.as_ref().map(|v| v.session.path.clone()), drag);
        assert!(!s.editor.can_undo());
    }
}

#[test]
fn select_all_leaves_text_color_gradient_and_other_tools_to_their_own_editors() {
    for mode in 0..4 {
        let (mut s, target, _) = scene(if mode == 3 { 1 } else { 0 }, true, false, false);
        if mode == 3 {
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
            s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 5));
        }
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 2, false);
        match mode {
            0 => s.tool = Tool::Select,
            1 => {
                s.text_session = Some(
                    crate::text_edit::Session::new(
                        s.editor.project(),
                        s.document_revision,
                        s.frame,
                        None,
                        [0., 0.],
                    )
                    .unwrap(),
                )
            }
            2 => {
                s.colors.session = Some(
                    crate::color_edit::Session::new(
                        crate::color_edit::Target::Shape(1, ShapePaint::Fill),
                        s.editor.project(),
                        s.document_revision,
                        s.frame,
                    )
                    .unwrap(),
                )
            }
            _ => {
                s.gradient_editor =
                    Some(crate::panels::gradient_editor::Session::new(&s, 5).unwrap())
            }
        }
        let before = Unchanged::new(&s);
        assert!(!pen.select_all_key(&event("ctrl-a"), true, false, &s));
        assert_eq!(selected(&pen), [2]);
        before.check(&s);
    }
}

#[test]
fn marquee_selection_moves_the_same_evaluated_vertices_and_creates_only_one_edit() {
    for kind in 0..3 {
        for animated in [false, true] {
            let (mut s, target, core) = scene(kind, true, animated, true);
            let original_project = s.editor.project().clone();
            let (original, world) = geometry(&s, target);
            let mut pen = Pen::default();
            pick(&mut pen, &s, target, 0, false);
            box_vertex(&mut pen, &s, target, 3);
            let pointer = original.vertices[3].position;
            assert!(
                pen.down(&s, world.point(pointer), 1., false, false, false)
                    .is_none()
            );
            let command = pen
                .release(&s, world.point(add(pointer, [15., 27.])), false, false)
                .unwrap();
            assert!(matches!(command, Command::EditPath { target, .. } if target == core));
            s.editor.execute(command).unwrap();
            let changed = geometry(&s, target).0;
            for (i, v) in changed.vertices.iter().enumerate() {
                let expected = if [0, 3].contains(&i) {
                    add(original.vertices[i].position, [15., 27.])
                } else {
                    original.vertices[i].position
                };
                assert!(distance(v.position, expected) < 1e-8);
                assert_eq!(v.incoming, original.vertices[i].incoming);
                assert_eq!(v.outgoing, original.vertices[i].outgoing);
            }
            pen.reset_if_stale(&s);
            assert_eq!(selected(&pen), [0, 3]);
            s.editor.undo();
            assert_eq!(s.editor.project(), &original_project);
            assert!(!s.editor.can_undo());
        }
    }
}

#[test]
fn marquee_delete_is_atomic_and_animated_topology_rejection_preserves_selection_and_redo() {
    for kind in 0..3 {
        for animated in [false, true] {
            let (mut s, target, core) = scene(kind, true, animated, false);
            redo(&mut s);
            let before = s.editor.project().clone();
            let original = geometry(&s, target).0;
            let mut pen = Pen::default();
            pick(&mut pen, &s, target, 0, false);
            box_vertex(&mut pen, &s, target, 3);
            let (handled, command) = pen.key("delete", &s);
            assert!(handled);
            let command = command.unwrap();
            assert!(matches!(command, Command::EditPath { target, .. } if target == core));
            if animated {
                assert!(s.editor.execute(command).unwrap_err().contains("topology"));
                assert_eq!(selected(&pen), [0, 3]);
                assert_eq!(s.editor.project(), &before);
                assert!(!s.editor.can_undo() && s.editor.can_redo());
            } else {
                s.editor.execute(command).unwrap();
                let expected: Vec<_> = original
                    .vertices
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| ![0, 3].contains(i))
                    .map(|(_, v)| v.clone())
                    .collect();
                assert_eq!(geometry(&s, target).0.vertices, expected);
                assert!(selected(&pen).is_empty());
                s.editor.undo();
                assert_eq!(s.editor.project(), &before);
                assert!(!s.editor.can_undo());
            }
        }
    }
}

#[test]
fn marquee_then_reverse_remaps_geometric_selection_for_all_target_kinds() {
    for kind in 0..3 {
        for animated in [false, true] {
            let (mut s, target, _) = scene(kind, true, animated, true);
            let original = geometry(&s, target).0;
            let mut pen = Pen::default();
            pick(&mut pen, &s, target, 1, false);
            box_vertex(&mut pen, &s, target, 4);
            let (handled, command) = pen.order_key(&event("shift-r"), true, false, &s);
            assert!(handled);
            s.editor.execute(command.unwrap()).unwrap();
            pen.reset_if_stale(&s);
            assert_eq!(selected(&pen), [2, 5]);
            let reversed = geometry(&s, target).0;
            assert_eq!(reversed.vertices[2].position, original.vertices[4].position);
            assert_eq!(reversed.vertices[5].position, original.vertices[1].position);
        }
    }
}

#[test]
fn select_all_preserves_open_path_minimum_and_never_falls_through_to_layer_deletion() {
    for closed in [false, true] {
        let (s, target, _) = scene(0, closed, false, false);
        let before = Unchanged::new(&s);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 0, false);
        assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
        assert_eq!(selected(&pen), [0, 1, 2, 3, 4, 5]);
        for key in ["delete", "backspace"] {
            assert!(matches!(pen.key(key, &s), (true, None)));
        }
        before.check(&s);
    }
}

#[test]
fn anchor_toggle_and_handle_drag_take_priority_over_starting_marquee() {
    let (mut s, target, _) = scene(0, true, false, false);
    let mut path = geometry(&s, target).0;
    path.vertices[0].outgoing = [40., 30.];
    path.vertices[0].incoming = [-40., -30.];
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target: PathTarget::Shape,
            frame: 0,
            path,
        })
        .unwrap();
    s.editor.clear_history();
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 0, false);
    assert!(pen.down(&s, [120., 20.], 1., false, true, false).is_none());
    pen.moving([200., 100.], false, true);
    assert!(pen.marquee.is_none() && pen.drag.is_none());
    assert!(pen.up(&s).is_none());
    assert_eq!(selected(&pen), [0, 1]);
    assert!(pen.down(&s, [60., 50.], 1., false, true, false).is_none());
    assert!(pen.marquee.is_none() && pen.drag.is_some());
    let command = pen.release(&s, [90., 50.], true, false).unwrap();
    s.editor.execute(command).unwrap();
    let changed = geometry(&s, target).0;
    assert_eq!(changed.vertices[0].outgoing, [70., 30.]);
    assert_eq!(changed.vertices[0].incoming, [-40., -30.]);
    assert_eq!(changed.vertices[1].position, [120., 20.]);
}

#[test]
fn creation_force_mask_and_curve_insertion_remain_available_outside_marquee() {
    for mask in [false, true] {
        let (s, target, _) = scene(0, true, false, false);
        let mut pen = Pen::default();
        pick(&mut pen, &s, target, 0, false);
        for (i, p) in [[700., 600.], [800., 600.], [800., 700.]]
            .into_iter()
            .enumerate()
        {
            assert!(pen.down(&s, p, 1., false, i == 1, mask).is_none());
            assert!(pen.up(&s).is_none());
            assert!(pen.marquee.is_none());
        }
        assert_eq!(pen.draft.as_ref().unwrap().path.vertices.len(), 3);
        let command = pen.key("enter", &s).1.unwrap();
        if mask {
            assert!(matches!(command, Command::SetPathMasks { id: 1, .. }));
        } else {
            assert!(matches!(command, Command::AddContent { .. }));
        }
    }
    let (s, target, _) = scene(0, true, false, false);
    let mut pen = Pen::default();
    pick(&mut pen, &s, target, 0, false);
    begin(&mut pen, &s, [70., 20.], 1.);
    assert!(pen.up(&s).is_none());
    assert_eq!(geometry(&s, target).0.vertices.len(), 6);
    assert!(pen.down(&s, [70., 20.], 1., false, false, false).is_none());
    assert!(pen.marquee.is_none() && pen.drag.is_some());
    assert!(matches!(pen.up(&s), Some(Command::EditPath { .. })));
}

#[test]
fn transformed_rectangle_membership_is_axis_aligned_in_composition_space() {
    for kind in 0..3 {
        let (s, target, _) = scene(kind, true, true, true);
        let (path, world) = geometry(&s, target);
        let points: Vec<_> = path
            .vertices
            .iter()
            .map(|v| world.point(v.position))
            .collect();
        let min = [0, 1].map(|axis| points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min));
        let max = [0, 1].map(|axis| {
            points
                .iter()
                .map(|p| p[axis])
                .fold(f64::NEG_INFINITY, f64::max)
        });
        for axis in 0..2 {
            let start = add(min, [-25., -25.]);
            let mut end = add(max, [25., 25.]);
            end[axis] = (min[axis] + max[axis]) / 2.;
            let expected: BTreeSet<_> = [0]
                .into_iter()
                .chain(points.iter().enumerate().filter_map(|(i, p)| {
                    (p[0] >= start[0] && p[0] <= end[0] && p[1] >= start[1] && p[1] <= end[1])
                        .then_some(i)
                }))
                .collect();
            let mut pen = Pen::default();
            pick(&mut pen, &s, target, 0, false);
            begin(&mut pen, &s, start, 1.);
            pen.moving(end, false, true);
            assert_eq!(overlay(&pen, &s, target), expected);
            assert!(pen.up(&s).is_none());
            assert_eq!(pen.selected.as_ref().unwrap().vertices, expected);
        }
    }
}

#[test]
fn no_target_selection_does_not_dirty_an_empty_document() {
    let mut s = EditorState::default();
    s.tool = Tool::Pen;
    assert!(!s.dirty());
    let before = Unchanged::new(&s);
    let mut pen = Pen::default();
    assert!(pen.select_all_key(&event("ctrl-a"), true, false, &s));
    assert!(pen.down(&s, [0., 0.], 1., false, true, false).is_none());
    assert!(pen.release(&s, [200., 200.], false, true).is_none());
    assert!(pen.selected.is_none() && pen.draft.is_none());
    before.check(&s);
}
