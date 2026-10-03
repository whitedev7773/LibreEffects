//! Multi-selection transactions exercise the same source isolation as singleton edits.
use super::{
    tests::{TARGETS, animation, geometry, scene},
    *,
};
use libre_effects_core::{
    ContentsEdit, ContentsKind, ContentsParam, Interpolation, PathVertex, Property, PropertyPath,
    TrackEdit,
};

fn request(s: &EditorState, target: PathTarget, indices: &[usize]) -> Request {
    let (path, world) = evaluated(s, 1, target).unwrap();
    Request::for_selection(s, 1, target, indices.iter().copied().collect(), path, world).unwrap()
}
fn open(s: &mut EditorState, target: PathTarget, indices: &[usize]) -> u64 {
    let session = Session::new(s, request(s, target, indices)).unwrap();
    let id = session.id;
    s.vertex_editor = Some(session);
    id
}
fn set_path(s: &mut EditorState, target: PathTarget, path: VectorPath) {
    s.editor
        .execute(Command::EditPath {
            id: 1,
            target,
            frame: s.frame,
            path,
        })
        .unwrap();
    s.editor.clear_history();
}
fn parameters(session: &Session) -> [f64; 7] {
    std::array::from_fn(|index| session.value(index).unwrap())
}
fn apply(session: &mut Session, values: [f64; 7]) {
    for (index, value) in values.into_iter().enumerate() {
        session.input(index, &value.to_string()).unwrap();
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a:.17} != {b:.17}");
}

#[test]
fn selection_constructor_preserves_singleton_api_and_rejects_empty_or_invalid_sets() {
    for target in TARGETS {
        let s = scene(target, false, 0);
        let (path, world) = evaluated(&s, 1, target).unwrap();
        for indices in [BTreeSet::new(), [0, 4].into(), [usize::MAX].into()] {
            assert!(!Request::available_selection(
                &s, 1, target, &indices, &path, world
            ));
            assert!(Request::for_selection(&s, 1, target, indices, path.clone(), world).is_err());
        }
        for indices in [&[2][..], &[0, 2], &[0, 1, 2, 3]] {
            let req = request(&s, target, indices);
            assert_eq!(req.index, indices[0]);
            assert_eq!(req.indices, indices.iter().copied().collect());
            let mut session = Session::new(&s, req).unwrap();
            assert_eq!(session.is_transform(), indices.len() > 1);
            assert_eq!(session.field_count(), if indices.len() > 1 { 7 } else { 6 });
            assert!(session.value(session.field_count()).is_none());
            assert!(session.input(session.field_count(), "1").is_err());
            assert!(session.input(usize::MAX, "1").is_err());
        }
        for indices in [BTreeSet::new(), [0, 4].into(), [1, 2].into()] {
            let mut req = request(&s, target, &[0, 2]);
            req.indices = indices;
            assert!(!req.current(&s));
            assert!(Session::new(&s, req).is_err());
        }
    }
}

#[test]
fn opening_pivot_uses_selected_local_anchor_bounds_and_never_recenters() {
    for target in TARGETS {
        let s = scene(target, true, 10);
        let req = request(&s, target, &[0, 2]);
        let source = req.path.clone();
        let expected = [
            (source.vertices[0].position[0] + source.vertices[2].position[0]) / 2.,
            (source.vertices[0].position[1] + source.vertices[2].position[1]) / 2.,
        ];
        let mut session = Session::new(&s, req).unwrap();
        assert_eq!(
            parameters(&session),
            [0., 0., 0., 100., 100., expected[0], expected[1]]
        );
        session.input(0, "53.125").unwrap();
        session.input(2, "41.75").unwrap();
        assert_eq!(session.value(5), Some(expected[0]));
        assert_eq!(session.value(6), Some(expected[1]));
        session.input(5, "-12.0625").unwrap();
        session.input(6, "0.03125").unwrap();
        session.input(0, "-2.75").unwrap();
        assert_eq!(session.value(5), Some(-12.0625));
        assert_eq!(session.value(6), Some(0.03125));
        for index in 0..7 {
            assert_eq!(
                session
                    .field_value(index)
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    .to_bits(),
                session.value(index).unwrap().to_bits()
            );
        }
    }
}

#[test]
fn combined_local_transform_matches_independent_math_and_changes_only_selected_geometry() {
    for target in TARGETS {
        for indices in [&[0, 2][..], &[1, 2], &[0, 1, 2, 3]] {
            let mut s = scene(target, false, 0);
            let source = s.editor.project().clone();
            let req = request(&s, target, indices);
            let original = req.path.clone();
            let id = open(&mut s, target, indices);
            let values = [13.0625, -7.875, 32.25, -135.5, 62.25, -11.375, 41.0625];
            apply(s.vertex_editor.as_mut().unwrap(), values);
            let session = s.vertex_editor.as_ref().unwrap();
            assert_eq!(session.id, id);
            let [dx, dy, angle, sx, sy, px, py] = values;
            let (sin, cos) = angle.to_radians().sin_cos();
            let linear = |[x, y]: [f64; 2]| {
                let x = x * sx / 100.;
                let y = y * sy / 100.;
                [x * cos - y * sin, x * sin + y * cos]
            };
            for (index, vertex) in session.path().vertices.iter().enumerate() {
                let before = original.vertices[index];
                if !indices.contains(&index) {
                    assert_eq!(*vertex, before);
                    continue;
                }
                let relative = linear([before.position[0] - px, before.position[1] - py]);
                close(vertex.position[0], px + relative[0] + dx);
                close(vertex.position[1], py + relative[1] + dy);
                for (actual, expected) in [
                    (vertex.incoming, linear(before.incoming)),
                    (vertex.outgoing, linear(before.outgoing)),
                ] {
                    close(actual[0], expected[0]);
                    close(actual[1], expected[1]);
                }
            }
            assert_eq!(session.path().closed, original.closed);
            assert_eq!(session.path().vertices.len(), original.vertices.len());
            assert_eq!(s.editor.project(), &source);
            let preview = session.project().clone();
            let mut independent = Editor::default();
            independent.replace_project(source.clone()).unwrap();
            independent
                .execute(Command::EditPath {
                    id: 1,
                    target,
                    frame: 0,
                    path: session.path().clone(),
                })
                .unwrap();
            assert_eq!(&preview, independent.project());
            s.accept_vertex_editor();
            assert_eq!(s.editor.project(), &preview);
            assert_eq!(
                s.vertex_return.as_ref().unwrap().indices,
                indices.iter().copied().collect()
            );
            assert!(s.vertex_return.as_ref().unwrap().current(&s));
            assert!(s.editor.can_undo());
            s.editor.undo();
            assert_eq!(s.editor.project(), &source);
            assert!(!s.editor.can_undo());
            s.editor.redo();
            assert_eq!(s.editor.project(), &preview);
            let encoded = libre_effects_core::project_file::encode(&preview, None).unwrap();
            assert_eq!(
                libre_effects_core::project_file::decode(&encoded)
                    .unwrap()
                    .project,
                preview
            );
        }
    }
}

#[test]
fn cardinal_turns_reflections_zero_scales_and_tangent_vectors_are_exact() {
    let source = VectorPath {
        closed: false,
        vertices: vec![
            PathVertex {
                position: [1., 2.],
                incoming: [2., 3.],
                outgoing: [-4., 5.],
            },
            PathVertex {
                position: [-3., 4.],
                incoming: [-1., 2.],
                outgoing: [6., -2.],
            },
            PathVertex::corner([9., 10.]),
        ],
    };
    for (angle, sx, sy, expected, incoming) in [
        (90., 100., 100., [-2., 1.], [-3., 2.]),
        (-90., 100., 100., [2., -1.], [3., -2.]),
        (180., 100., 100., [-1., -2.], [-2., -3.]),
        (270., 100., 100., [2., -1.], [3., -2.]),
        (0., -100., 100., [-1., 2.], [-2., 3.]),
        (0., 0., 100., [0., 2.], [0., 3.]),
        (0., 0., 0., [0., 0.], [0., 0.]),
        (90., -200., 50., [-1., -2.], [-1.5, -4.]),
    ] {
        let path =
            transform_path(&source, &[0, 1].into(), &[7., -11., angle, sx, sy, 0., 0.]).unwrap();
        assert_eq!(
            path.vertices[0].position,
            [expected[0] + 7., expected[1] - 11.]
        );
        assert_eq!(path.vertices[0].incoming, incoming);
        assert_eq!(path.vertices[2], source.vertices[2]);
        assert!(path.valid());
    }
}

#[test]
fn unchanged_components_survive_enormous_pivots_and_tiny_negative_rotation_is_not_erased() {
    let source = VectorPath {
        closed: false,
        vertices: vec![
            PathVertex {
                position: [0., 31.123456789012345],
                incoming: [3.123456789012345, 0.],
                outgoing: [0., 2.987654321098765],
            },
            PathVertex::corner([0., 52.987654321098765]),
        ],
    };
    let moved = transform_path(
        &source,
        &[0, 1].into(),
        &[1e-16, 0., 0., 100., 100., 1e300, -1e300],
    )
    .unwrap();
    assert_eq!(moved.vertices[0].position[0], 1e-16);
    assert_eq!(
        moved.vertices[0].position[1].to_bits(),
        source.vertices[0].position[1].to_bits()
    );
    assert_eq!(moved.vertices[0].incoming, source.vertices[0].incoming);
    assert_eq!(moved.vertices[0].outgoing, source.vertices[0].outgoing);
    let turned = transform_path(
        &source,
        &[0, 1].into(),
        &[0., 0., -1e-16, 100., 100., 0., 0.],
    )
    .unwrap();
    assert!(turned.vertices[0].position[0] > 0.);
    assert!(turned.vertices[0].incoming[1] < 0.);
    assert_ne!(turned, source);
}

#[test]
fn exact_identities_pivot_edits_and_return_to_source_preserve_project_redo_and_schema() {
    for target in TARGETS {
        for (animated, frame) in [(false, 0), (true, 10), (true, 20)] {
            for mode in 0..5 {
                let mut s = scene(target, animated, frame);
                if target == PathTarget::Shape && !animated {
                    let mut value = serde_json::to_value(s.editor.project()).unwrap();
                    value["version"] = 30.into();
                    s.editor
                        .replace_project(Project::from_json(&value.to_string()).unwrap())
                        .unwrap();
                    s.editor.select(1);
                    // Project replacement is itself undoable; it is fixture setup,
                    // not part of the transaction history measured below.
                    s.editor.clear_history();
                }
                s.editor
                    .execute(Command::RenameLayer {
                        id: 2,
                        name: "Redo sentinel".into(),
                    })
                    .unwrap();
                let redo = s.editor.project().clone();
                s.editor.undo();
                let source = s.editor.project().clone();
                let bytes = libre_effects_core::project_file::encode(&source, None).unwrap();
                let id = open(&mut s, target, &[0, 2]);
                match mode {
                    0 => {
                        s.vertex_input(id, 2, "-1080");
                    }
                    1 => {
                        s.vertex_input(id, 5, "1e308");
                        s.vertex_input(id, 6, "-1e308");
                    }
                    2 => {
                        s.vertex_input(id, 3, "-100");
                        s.vertex_input(id, 4, "-100");
                        s.vertex_input(id, 2, "180");
                        s.vertex_input(id, 5, "1e308");
                    }
                    3 => {
                        s.vertex_input(id, 0, "11.123456789012345");
                        s.vertex_input(id, 0, "0");
                    }
                    _ => {
                        s.vertex_input(id, 2, "37.25");
                        s.vertex_input(id, 2, "0");
                    }
                }
                let session = s.vertex_editor.as_ref().unwrap();
                assert!(
                    session.command().unwrap().is_none(),
                    "{target:?} {frame} {mode}"
                );
                assert_eq!(session.project(), &source);
                assert_eq!(
                    libre_effects_core::project_file::encode(session.project(), None).unwrap(),
                    bytes
                );
                s.accept_vertex_editor();
                assert_eq!(s.editor.project(), &source);
                assert!(!s.editor.can_undo());
                assert!(s.editor.can_redo());
                s.editor.redo();
                assert_eq!(s.editor.project(), &redo);
            }
        }
    }
}

#[test]
fn degenerate_and_fixed_point_transforms_are_exact_effective_noops() {
    for target in TARGETS {
        for fixed_line in [false, true] {
            let mut s = scene(target, false, 0);
            let path = VectorPath {
                closed: target == PathTarget::Mask(1),
                vertices: (0..4)
                    .map(|index| {
                        if fixed_line {
                            PathVertex {
                                position: [index as f64 * 0.125, 0.],
                                incoming: [-0.03125, 0.],
                                outgoing: [0.0625, 0.],
                            }
                        } else {
                            PathVertex::corner([13.125, -7.0625])
                        }
                    })
                    .collect(),
            };
            set_path(&mut s, target, path);
            let source = s.editor.project().clone();
            let id = open(&mut s, target, &[0, 1, 2, 3]);
            if fixed_line {
                s.vertex_input(id, 4, "-350");
            } else {
                s.vertex_input(id, 3, "0");
                s.vertex_input(id, 4, "0");
                s.vertex_input(id, 2, "43.123456789012345");
            }
            let session = s.vertex_editor.as_ref().unwrap();
            assert!(session.command().unwrap().is_none());
            assert_eq!(session.project(), &source);
            s.accept_vertex_editor();
            assert_eq!(s.editor.project(), &source);
            assert!(!s.editor.can_undo());
        }
    }
}

#[test]
fn reset_rotates_serial_before_old_callbacks_and_clears_all_errors() {
    for target in TARGETS {
        let mut s = scene(target, true, 10);
        let source = s.editor.project().clone();
        let first = open(&mut s, target, &[0, 2]);
        let defaults = parameters(s.vertex_editor.as_ref().unwrap());
        s.vertex_input(first, 0, "41.125");
        s.vertex_input(first, 2, "27.5");
        s.vertex_input(first, 5, "-4.0625");
        s.vertex_input(first, 1, "bad");
        s.vertex_input(first, 6, "NaN");
        assert!(s.reset_vertex_editor(first));
        let reset = s.vertex_editor.as_ref().unwrap();
        let second = reset.id;
        assert_ne!(first, second);
        assert_eq!(parameters(reset), defaults);
        assert!(reset.command().unwrap().is_none());
        assert!(reset.error.is_empty());
        assert_eq!(reset.project(), &source);
        assert_eq!(reset.request().indices, [0, 2].into());
        s.vertex_input(first, 0, "777");
        s.vertex_input(first, 5, "bad");
        assert!(s.revert_vertex_field(first, 0).is_none());
        assert!(!s.reset_vertex_editor(first));
        assert_eq!(s.vertex_editor.as_ref().unwrap().project(), &source);
        s.vertex_input(second, 0, "7.125");
        s.cancel_vertex_editor();
        assert_eq!(s.editor.project(), &source);
        assert_eq!(s.vertex_return.as_ref().unwrap().indices, [0, 2].into());
        assert!(!s.editor.can_undo());
    }
}

#[test]
fn finite_parameters_are_not_geometry_capped_and_rejected_results_keep_last_valid_values() {
    let target = PathTarget::Shape;
    let mut s = scene(target, false, 0);
    let mut source_path = geometry(false, 0.);
    for index in [0, 2] {
        source_path.vertices[index].position[0] = -1_000_000.;
    }
    set_path(&mut s, target, source_path.clone());
    let source = s.editor.project().clone();
    let id = open(&mut s, target, &[0, 2]);
    s.vertex_input(id, 0, "2000000");
    let session = s.vertex_editor.as_ref().unwrap();
    assert_eq!(session.value(0), Some(2_000_000.));
    for index in [0, 2] {
        assert_eq!(session.path().vertices[index].position[0], 1_000_000.);
    }
    let valid = session.project().clone();
    for text in [
        "2000000.00001",
        "-0.00001",
        "1e308",
        "-1e308",
        "NaN",
        "inf",
        "-inf",
        "1e309",
        "",
        "invalid",
    ] {
        s.vertex_input(id, 0, text);
        let session = s.vertex_editor.as_ref().unwrap();
        assert!(session.has_input_error(0), "{text}");
        assert!(session.field_error(0).is_some());
        assert_eq!(session.value(0), Some(2_000_000.));
        assert_eq!(session.project(), &valid);
        assert_eq!(s.editor.project(), &source);
    }
    s.vertex_input(id, 3, "1e308");
    s.vertex_input(id, 4, "not a number");
    s.vertex_input(id, 1, "1.25");
    let session = s.vertex_editor.as_ref().unwrap();
    assert_eq!(session.input_error, Some(0));
    assert!(session.has_input_error(0) && session.has_input_error(3) && session.has_input_error(4));
    assert_eq!(session.value(3), Some(100.));
    assert_eq!(session.value(4), Some(100.));
    assert_eq!(session.value(1), Some(1.25));
    s.accept_vertex_editor();
    assert!(s.vertex_editor.is_some());
    assert_eq!(s.revert_vertex_field(id, 0).as_deref(), Some("2000000"));
    assert_eq!(s.vertex_editor.as_ref().unwrap().input_error, Some(3));
    s.vertex_input(id, 3, "100");
    assert_eq!(s.vertex_editor.as_ref().unwrap().input_error, Some(4));
    s.revert_vertex_field(id, 4).unwrap();
    assert!(
        s.vertex_editor
            .as_ref()
            .unwrap()
            .command()
            .unwrap()
            .is_some()
    );
}

#[test]
fn overflow_rejection_preserves_pivot_and_collapse_can_accept_unbounded_finite_scale() {
    let mut s = scene(PathTarget::Shape, false, 0);
    let id = open(&mut s, PathTarget::Shape, &[0, 2]);
    s.vertex_input(id, 5, "1e308");
    let source = s.vertex_editor.as_ref().unwrap().project().clone();
    s.vertex_input(id, 3, "1e308");
    let session = s.vertex_editor.as_ref().unwrap();
    assert!(session.has_input_error(3));
    assert_eq!(session.value(3), Some(100.));
    assert_eq!(session.value(5), Some(1e308));
    assert_eq!(session.project(), &source);
    s.cancel_vertex_editor();
    set_path(
        &mut s,
        PathTarget::Shape,
        VectorPath {
            closed: false,
            vertices: vec![PathVertex::corner([0., 0.]); 4],
        },
    );
    let id = open(&mut s, PathTarget::Shape, &[0, 2]);
    s.vertex_input(id, 3, "1e308");
    let session = s.vertex_editor.as_ref().unwrap();
    assert!(!session.has_input_error(3));
    assert_eq!(session.value(3), Some(1e308));
    assert!(session.command().unwrap().is_none());
}

#[test]
fn repeated_parameter_edits_rebuild_from_source_without_drift_or_pose_accumulation() {
    for target in TARGETS {
        for frame in [10, 20] {
            let s = scene(target, true, frame);
            let source = s.editor.project().clone();
            let count = animation(&source, target)["poses"]
                .as_array()
                .unwrap()
                .len();
            let mut session = Session::new(&s, request(&s, target, &[0, 2])).unwrap();
            let final_values = [17.0625, -6.125, 38.75, -77.5, 132.25, 6.375, -17.625];
            for step in 0..50 {
                session
                    .input(0, &(10. + step as f64 * 0.125).to_string())
                    .unwrap();
                session
                    .input(2, &(step as f64 * 1.375).to_string())
                    .unwrap();
                session
                    .input(3, &(-42. + step as f64 * 0.25).to_string())
                    .unwrap();
                assert_eq!(
                    animation(session.project(), target)["poses"]
                        .as_array()
                        .unwrap()
                        .len(),
                    count + 1
                );
                assert_eq!(s.editor.project(), &source);
            }
            apply(&mut session, final_values);
            let mut fresh = Session::new(&s, request(&s, target, &[0, 2])).unwrap();
            apply(&mut fresh, final_values);
            assert_eq!(session.path(), fresh.path());
            assert_eq!(session.project(), fresh.project());
        }
    }
}

#[test]
fn animated_transform_preserves_existing_easing_shared_poses_and_unedited_keys() {
    for target in TARGETS {
        for frame in [10, 20] {
            let mut s = scene(target, true, frame);
            let source = s.editor.project().clone();
            let previous = animation(&source, target);
            let opening = evaluated(&s, 1, target).unwrap().0;
            let id = open(&mut s, target, &[0, 2]);
            assert_eq!(s.vertex_editor.as_ref().unwrap().path(), &opening);
            s.vertex_input(id, 0, "7.0625");
            s.vertex_input(id, 2, "32.25");
            let preview = s.vertex_editor.as_ref().unwrap().project().clone();
            s.accept_vertex_editor();
            let layer = s.editor.project().composition().layer(1).unwrap();
            let keys = layer.track(PropertyPath::Path(target)).unwrap().keys();
            assert_eq!(keys.len(), if frame == 10 { 5 } else { 4 });
            if frame == 10 {
                assert_eq!(keys[&10].interpolation, Interpolation::Linear);
            }
            let after = animation(s.editor.project(), target);
            for key in [0, 20, 40, 60] {
                if key == frame {
                    continue;
                }
                let key = key.to_string();
                assert_eq!(
                    after["timing"]["keys"][&key],
                    previous["timing"]["keys"][&key]
                );
                let index = previous["timing"]["keys"][&key]["value"].as_f64().unwrap() as usize;
                assert_eq!(after["poses"][index], previous["poses"][index]);
            }
            if frame == 20 {
                assert_eq!(
                    after["timing"]["keys"]["20"]["interpolation"],
                    previous["timing"]["keys"]["20"]["interpolation"]
                );
                assert_eq!(
                    previous["timing"]["keys"]["20"]["value"],
                    previous["timing"]["keys"]["60"]["value"]
                );
            }
            assert_eq!(s.editor.project(), &preview);
            s.editor.undo();
            assert_eq!(s.editor.project(), &source);
            assert!(!s.editor.can_undo());
            s.editor.redo();
            assert_eq!(s.editor.project(), &preview);
        }
    }
}

#[test]
fn nested_contents_world_is_only_an_editability_guard_and_stable_mask_id_is_preserved() {
    let target = PathTarget::Contents(2);
    let mut s = scene(target, false, 0);
    s.editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add {
                parent: 0,
                kind: ContentsKind::Group(vec![]),
            },
        })
        .unwrap();
    let Content::ShapeContents(contents) =
        s.editor.project().composition().layer(1).unwrap().content()
    else {
        panic!()
    };
    let outer = contents
        .rows()
        .into_iter()
        .find(|(_, parent, node)| {
            *parent == 0 && node.id != 1 && matches!(node.kind, ContentsKind::Group(_))
        })
        .unwrap()
        .2
        .id;
    s.editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Move {
                item: 1,
                parent: outer,
                index: 0,
            },
        })
        .unwrap();
    for (parameter, value) in [
        (ContentsParam::Transform(Property::Rotation), -21.25),
        (ContentsParam::Skew, 17.5),
        (ContentsParam::Transform(Property::ScaleY), -125.),
    ] {
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Track {
                    item: outer,
                    parameter,
                    edit: TrackEdit::Value { frame: 0, value },
                },
            })
            .unwrap();
    }
    s.editor.clear_history();
    let opening = evaluated(&s, 1, target).unwrap().0;
    let id = open(&mut s, target, &[0, 2]);
    s.vertex_input(id, 0, "7.5");
    let mut expected = opening;
    for index in [0, 2] {
        expected.vertices[index].position[0] += 7.5;
    }
    assert_eq!(s.vertex_editor.as_ref().unwrap().path(), &expected);
    s.cancel_vertex_editor();
    s.editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: outer,
                parameter: ContentsParam::Transform(Property::ScaleX),
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 0.,
                },
            },
        })
        .unwrap();
    assert!(evaluated(&s, 1, target).is_none());

    let mut s = scene(PathTarget::Mask(1), false, 0);
    // Load an existing stable ID with its matching allocator watermark.
    // SetPathMasks deliberately cannot invent an arbitrary nonzero ID.
    let mut json = serde_json::to_value(s.editor.project()).unwrap();
    let layer = json["composition"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|layer| layer["id"] == 1)
        .unwrap();
    layer["path_masks"][0]["id"] = 73.into();
    layer["next_mask_id"] = 74.into();
    let loaded = Project::from_json(&json.to_string()).unwrap();
    s.editor.replace_project(loaded).unwrap();
    s.editor.select(1);
    s.editor.clear_history();
    let id = open(&mut s, PathTarget::Mask(73), &[0, 2]);
    s.vertex_input(id, 3, "0");
    s.vertex_input(id, 4, "-100");
    s.accept_vertex_editor();
    let returned = s.vertex_return.as_ref().unwrap();
    assert_eq!(returned.target, PathTarget::Mask(73));
    assert!(returned.path.closed);
    assert!(returned.current(&s));
}

#[test]
fn multi_selection_context_invalidation_and_late_reset_never_restore_an_old_target() {
    let changes: Vec<Box<dyn Fn(&mut EditorState)>> = vec![
        Box::new(|s| s.frame += 1),
        Box::new(|s| s.tool = Tool::Select),
        Box::new(|s| s.editor.select(2)),
        Box::new(|s| {
            s.selected_layers.insert(2);
        }),
        Box::new(|s| s.contents_selection = None),
        Box::new(|s| s.document_revision += 1),
        Box::new(|s| s.playing = true),
        Box::new(|s| s.preview_caching = true),
        Box::new(|s| s.close_after_save = true),
        Box::new(|s| {
            s.editor.execute(Command::ToggleLocked(1)).unwrap();
        }),
        Box::new(|s| {
            s.editor
                .execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::Enabled {
                        item: 1,
                        enabled: false,
                    },
                })
                .unwrap();
        }),
        Box::new(|s| {
            s.editor.execute(Command::NewComposition).unwrap();
        }),
    ];
    for change in changes {
        let mut s = scene(PathTarget::Contents(2), true, 10);
        let id = open(&mut s, PathTarget::Contents(2), &[0, 2]);
        s.vertex_input(id, 0, "11.25");
        change(&mut s);
        let changed = s.editor.project().clone();
        assert!(!s.reset_vertex_editor(id));
        assert!(s.vertex_editor.is_none());
        assert!(s.vertex_return.is_none());
        s.vertex_input(id, 0, "999");
        s.accept_vertex_editor();
        s.cancel_vertex_editor();
        assert_eq!(s.editor.project(), &changed);
        assert!(s.vertex_return.is_none());
    }
    let mut s = scene(PathTarget::Shape, false, 0);
    let first = open(&mut s, PathTarget::Shape, &[0, 2]);
    s.cancel_vertex_editor();
    let second = open(&mut s, PathTarget::Shape, &[1, 3]);
    assert_ne!(first, second);
    s.vertex_input(first, 0, "111");
    assert_eq!(s.vertex_editor.as_ref().unwrap().value(0), Some(0.));
    assert_eq!(
        s.vertex_editor.as_ref().unwrap().request().indices,
        [1, 3].into()
    );
}

#[test]
fn zero_scale_collapses_exactly_at_nonbinary_pivot_plus_delta() {
    let source = VectorPath {
        closed: false,
        vertices: vec![
            PathVertex {
                position: [1., 2.],
                incoming: [3., -4.],
                outgoing: [-5., 6.],
            },
            PathVertex {
                position: [1_000_000., -1_000_000.],
                incoming: [-7., 8.],
                outgoing: [9., -10.],
            },
            PathVertex::corner([0.1, -0.2]),
        ],
    };
    for (sx, sy) in [(0., 100.), (100., 0.), (0., 0.)] {
        let path =
            transform_path(&source, &[0, 1, 2].into(), &[0., 0., 0., sx, sy, 0.1, -0.2]).unwrap();
        for (before, after) in source.vertices.iter().zip(&path.vertices) {
            for (axis, collapsed, pivot) in [(0, sx == 0., 0.1_f64), (1, sy == 0., -0.2_f64)] {
                if collapsed {
                    assert_eq!(after.position[axis].to_bits(), pivot.to_bits());
                    assert_eq!(after.incoming[axis], 0.);
                    assert_eq!(after.outgoing[axis], 0.);
                } else {
                    assert_eq!(
                        after.position[axis].to_bits(),
                        before.position[axis].to_bits()
                    );
                    assert_eq!(
                        after.incoming[axis].to_bits(),
                        before.incoming[axis].to_bits()
                    );
                    assert_eq!(
                        after.outgoing[axis].to_bits(),
                        before.outgoing[axis].to_bits()
                    );
                }
            }
        }
    }
    let path = transform_path(
        &source,
        &[0, 1, 2].into(),
        &[0.3, -0.4, 37., 0., 0., 0.1, -0.2],
    )
    .unwrap();
    for vertex in &path.vertices {
        assert_eq!(vertex.position, [0.1 + 0.3, -0.2 - 0.4]);
        assert_eq!(vertex.incoming, [0., 0.]);
        assert_eq!(vertex.outgoing, [0., 0.]);
    }
}

#[test]
fn tiny_nonzero_scale_remains_nonzero_through_numeric_session() {
    let mut s = scene(PathTarget::Shape, false, 0);
    set_path(
        &mut s,
        PathTarget::Shape,
        VectorPath {
            closed: false,
            vertices: vec![
                PathVertex {
                    position: [1., 0.],
                    incoming: [1., 0.],
                    outgoing: [-1., 0.],
                },
                PathVertex::corner([0., 0.]),
            ],
        },
    );
    let id = open(&mut s, PathTarget::Shape, &[0, 1]);
    s.vertex_input(id, 5, "0");
    for scale in ["1e-20", "-1e-20"] {
        s.vertex_input(id, 3, scale);
        let session = s.vertex_editor.as_ref().unwrap();
        assert!(!session.has_input_error(3));
        let expected = scale.parse::<f64>().unwrap() / 100.;
        let vertex = session.path().vertices[0];
        assert_ne!(vertex.position[0], 0.);
        assert_eq!(vertex.position[0].to_bits(), expected.to_bits());
        assert_eq!(vertex.incoming[0].to_bits(), expected.to_bits());
        assert_eq!(vertex.outgoing[0].to_bits(), (-expected).to_bits());
        assert!(session.command().unwrap().is_some());
    }
}

#[test]
fn cardinal_swap_retains_tiny_component_through_numeric_session_and_equal_pivots() {
    let mut s = scene(PathTarget::Shape, false, 0);
    set_path(
        &mut s,
        PathTarget::Shape,
        VectorPath {
            closed: false,
            vertices: vec![
                PathVertex {
                    position: [1e-20, 1_000_000.],
                    incoming: [1e-20, 1.],
                    outgoing: [-1e-20, -1.],
                },
                PathVertex::corner([0., 0.]),
            ],
        },
    );
    let id = open(&mut s, PathTarget::Shape, &[0, 1]);
    s.vertex_input(id, 5, "0");
    s.vertex_input(id, 6, "0");
    s.vertex_input(id, 2, "90");
    let session = s.vertex_editor.as_ref().unwrap();
    assert!(!session.has_input_error(2));
    assert_eq!(session.path().vertices[0].position, [-1_000_000., 1e-20]);
    assert_eq!(session.path().vertices[0].incoming, [-1., 1e-20]);
    assert_eq!(session.path().vertices[0].outgoing, [1., -1e-20]);
    // One collapsed axis makes this equal-pivot swap a valid complete path,
    // despite its tiny surviving component being far below the pivot's ulp.
    let path = transform_path(
        &session.request().path,
        &[0, 1].into(),
        &[0., 0., 90., 100., 0., 1_000_000., 1_000_000.],
    )
    .unwrap();
    assert_eq!(path.vertices[0].position, [1_000_000., 1e-20]);
}

#[test]
fn nonidentity_near_unit_scale_retains_small_motion_about_large_pivot() {
    let mut s = scene(PathTarget::Shape, false, 0);
    set_path(
        &mut s,
        PathTarget::Shape,
        VectorPath {
            closed: false,
            vertices: vec![
                PathVertex::corner([0.125, 0.]),
                PathVertex::corner([0., 0.]),
            ],
        },
    );
    let id = open(&mut s, PathTarget::Shape, &[0, 1]);
    s.vertex_input(id, 5, "1000000000000000");
    s.vertex_input(id, 3, "100.00000000000003");
    let session = s.vertex_editor.as_ref().unwrap();
    assert!(!session.has_input_error(3));
    let factor = 100.00000000000003_f64 / 100.;
    let expected = 0.125 + (factor - 1.) * (0.125 - 1e15);
    assert_eq!(session.path().vertices[0].position[0], expected);
    assert_ne!(expected, 1e15 + factor * (0.125 - 1e15));
    assert!(session.command().unwrap().is_some());
}

#[test]
fn cardinal_fixed_points_with_disparate_components_preserve_exact_project_bytes_and_redo() {
    for target in TARGETS {
        for (animated, frame) in [(false, 0), (true, 10)] {
            for position in [[1e-20, 1_000_000.], [-1_000_000., -1e-20]] {
                for angle in ["90", "-90", "180", "270"] {
                    let mut s = scene(target, animated, frame);
                    set_path(
                        &mut s,
                        target,
                        VectorPath {
                            closed: matches!(target, PathTarget::Mask(_)),
                            vertices: vec![PathVertex::corner(position); 4],
                        },
                    );
                    s.editor
                        .execute(Command::RenameLayer {
                            id: 2,
                            name: "Cardinal fixed-point redo".into(),
                        })
                        .unwrap();
                    let redo = s.editor.project().clone();
                    s.editor.undo();
                    let source = s.editor.project().clone();
                    let bytes = libre_effects_core::project_file::encode(&source, None).unwrap();
                    let id = open(&mut s, target, &[0, 2]);
                    s.vertex_input(id, 2, angle);
                    let session = s.vertex_editor.as_ref().unwrap();
                    assert_eq!(session.path(), &session.request().path);
                    assert!(
                        session.command().unwrap().is_none(),
                        "{target:?} {angle} {position:?}"
                    );
                    assert_eq!(session.project(), &source);
                    assert_eq!(
                        libre_effects_core::project_file::encode(session.project(), None).unwrap(),
                        bytes
                    );
                    s.accept_vertex_editor();
                    assert_eq!(s.editor.project(), &source);
                    assert!(!s.editor.can_undo());
                    assert!(s.editor.can_redo());
                    s.editor.redo();
                    assert_eq!(s.editor.project(), &redo);
                }
            }
        }
    }
}
