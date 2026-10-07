//! Scope/model regressions. These do not establish native pointer or IME behavior.
use super::{
    tests::{TARGETS, animation, scene},
    *,
};
use libre_effects_core::PathVertex;
use serde_json::{Value, json};

fn open(s: &mut EditorState, target: PathTarget, indices: &[usize]) -> u64 {
    let (path, world) = evaluated(s, 1, target).unwrap();
    let request =
        Request::for_selection(s, 1, target, indices.iter().copied().collect(), path, world)
            .unwrap();
    let session = Session::new(s, request).unwrap();
    let serial = session.id;
    s.vertex_editor = Some(session);
    serial
}
fn switch(s: &mut EditorState, scope: TransformScope) -> u64 {
    let serial = s.vertex_editor.as_ref().unwrap().id;
    assert!(s.switch_vertex_scope(serial, scope, None, false));
    let session = s.vertex_editor.as_ref().unwrap();
    assert_eq!(session.scope(), scope);
    assert_ne!(session.id, serial);
    session.id
}
fn values(session: &Session) -> [f64; 7] {
    std::array::from_fn(|index| session.value(index).unwrap())
}
fn target_base(project: &Project, target: PathTarget) -> &VectorPath {
    project
        .composition()
        .layer(1)
        .unwrap()
        .path_animation(target)
        .unwrap()
        .0
}
fn poses(project: &Project, target: PathTarget) -> Vec<VectorPath> {
    serde_json::from_value(animation(project, target)["poses"].clone()).unwrap()
}
fn translated(mut path: VectorPath, indices: &[usize], delta: [f64; 2]) -> VectorPath {
    for &index in indices {
        for axis in 0..2 {
            path.vertices[index].position[axis] += delta[axis];
        }
    }
    path
}

// Fixture-only editing locates the one non-default animation by complete value,
// then changes its sibling base. It never uses the production affine helper.
fn edit_fixture(s: &mut EditorState, target: PathTarget, edit: impl Fn(&mut Value, &mut Value)) {
    fn visit(value: &mut Value, original: &Value, edit: &impl Fn(&mut Value, &mut Value)) -> usize {
        match value {
            Value::Object(object) => {
                for key in ["path_animation", "animation"] {
                    if object.get(key) == Some(original) && object.contains_key("path") {
                        let mut base = object.remove("path").unwrap();
                        let mut animation = object.remove(key).unwrap();
                        edit(&mut base, &mut animation);
                        object.insert("path".into(), base);
                        object.insert(key.into(), animation);
                        return 1;
                    }
                }
                object
                    .values_mut()
                    .map(|value| visit(value, original, edit))
                    .sum()
            }
            Value::Array(values) => values
                .iter_mut()
                .map(|value| visit(value, original, edit))
                .sum(),
            _ => 0,
        }
    }
    let original = animation(s.editor.project(), target);
    assert!(!original["poses"].as_array().unwrap().is_empty());
    let mut value = serde_json::to_value(s.editor.project()).unwrap();
    assert_eq!(visit(&mut value, &original, &edit), 1);
    s.editor
        .replace_project(Project::from_json(&value.to_string()).unwrap())
        .unwrap();
    s.editor.select(1);
    s.selected_layers = [1].into();
    s.editor.clear_history();
}
fn with_unused(s: &mut EditorState, target: PathTarget, dormant: bool) {
    edit_fixture(s, target, |base, animation| {
        let mut unused: VectorPath = serde_json::from_value(base.clone()).unwrap();
        for vertex in &mut unused.vertices {
            vertex.position[0] += 100.;
            vertex.outgoing[1] -= 17.;
        }
        animation["poses"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(unused).unwrap());
        if dormant {
            animation["timing"]["keys"] = json!({});
            animation["timing"]["value"] = 1.into();
        }
    });
}

#[test]
fn all_pose_scope_transforms_base_and_every_slot_without_retiming_or_accumulation() {
    for target in TARGETS {
        for kind in 0..3 {
            let mut s = scene(target, kind != 0, if kind == 1 { 10 } else { 0 });
            if kind != 0 {
                with_unused(&mut s, target, kind == 2);
            }
            let source = s.editor.project().clone();
            let old_animation = animation(&source, target);
            let old_poses = poses(&source, target);
            let first = open(&mut s, target, &[0, 2]);
            assert_eq!(
                s.vertex_editor.as_ref().unwrap().scope(),
                TransformScope::ThisFrame
            );
            let defaults = values(s.vertex_editor.as_ref().unwrap());
            let serial = switch(&mut s, TransformScope::AllPoses);
            assert_ne!(serial, first);
            assert_eq!(values(s.vertex_editor.as_ref().unwrap()), defaults);
            assert_eq!(s.vertex_editor.as_ref().unwrap().project(), &source);
            assert!(
                s.vertex_editor
                    .as_ref()
                    .unwrap()
                    .command()
                    .unwrap()
                    .is_none()
            );
            for step in [1., 31., -17., 8.] {
                s.vertex_input(serial, 0, &step.to_string());
                s.vertex_input(serial, 1, "-4");
                let session = s.vertex_editor.as_ref().unwrap();
                assert!(!session.has_input_error(0));
                assert_eq!(
                    animation(session.project(), target)["timing"],
                    old_animation["timing"]
                );
                assert_eq!(poses(session.project(), target).len(), old_poses.len());
                assert_eq!(s.editor.project(), &source);
            }
            let session = s.vertex_editor.as_ref().unwrap();
            assert_eq!(
                target_base(session.project(), target),
                &translated(target_base(&source, target).clone(), &[0, 2], [8., -4.])
            );
            for (actual, old) in poses(session.project(), target).into_iter().zip(old_poses) {
                assert_eq!(actual, translated(old, &[0, 2], [8., -4.]));
            }
            assert_eq!(
                session.path(),
                &evaluated_at(session.project(), 1, target, s.frame)
                    .unwrap()
                    .0
            );
            assert_eq!(
                session.project().composition().layer(2),
                source.composition().layer(2)
            );
            assert!(
                matches!(session.command().unwrap(), Some(Command::TransformPathPoses { id: 1, target: t, indices, .. }) if t == target && indices == [0, 2].into())
            );
            let expected = session.project().clone();
            s.accept_vertex_editor();
            assert!(s.vertex_editor.is_none());
            assert_eq!(s.editor.project(), &expected);
            assert!(s.vertex_return.as_ref().unwrap().current(&s));
            assert_eq!(s.vertex_return.as_ref().unwrap().indices, [0, 2].into());
            s.editor.undo();
            assert_eq!(s.editor.project(), &source);
            assert!(!s.editor.can_undo());
            s.editor.redo();
            assert_eq!(s.editor.project(), &expected);
            assert!(!s.editor.can_redo());
        }
    }
}

#[test]
fn a_visible_fixed_point_still_commits_changed_base_unused_and_other_poses() {
    for target in TARGETS {
        for hidden in 0..3 {
            let mut s = scene(target, true, 0);
            edit_fixture(&mut s, target, |base, animation| {
                let fixed = VectorPath {
                    closed: target == PathTarget::Mask(1),
                    vertices: (0..4)
                        .map(|i| PathVertex::corner([i as f64 * 8., 0.]))
                        .collect(),
                };
                let moved = translated(fixed.clone(), &[0, 2], [0., 12.]);
                let fixed = serde_json::to_value(fixed).unwrap();
                let moved = serde_json::to_value(moved).unwrap();
                *base = if hidden == 0 {
                    moved.clone()
                } else {
                    fixed.clone()
                };
                animation["poses"] = json!([
                    fixed,
                    if hidden == 1 {
                        moved.clone()
                    } else {
                        fixed.clone()
                    },
                    if hidden == 2 { moved } else { fixed.clone() }
                ]);
                animation["timing"] = json!({"value": 0., "keys": {
                    "0": {"value": 0., "interpolation": "Linear"},
                    "20": {"value": 1., "interpolation": "Linear"}
                }});
            });
            let source = s.editor.project().clone();
            let first = open(&mut s, target, &[0, 2]);
            s.vertex_input(first, 4, "200");
            assert!(
                s.vertex_editor
                    .as_ref()
                    .unwrap()
                    .command()
                    .unwrap()
                    .is_none()
            );
            let serial = switch(&mut s, TransformScope::AllPoses);
            let session = s.vertex_editor.as_ref().unwrap();
            assert_eq!(session.value(4), Some(200.));
            assert_eq!(session.path(), &session.request.path);
            assert_ne!(session.project(), &source, "{target:?} hidden {hidden}");
            assert!(session.command().unwrap().is_some());
            let expected = session.project().clone();
            s.vertex_input(first, 0, "999");
            assert_eq!(s.vertex_editor.as_ref().unwrap().id, serial);
            s.accept_vertex_editor();
            assert_eq!(s.editor.project(), &expected);
            s.editor.undo();
            assert_eq!(s.editor.project(), &source);
            s.editor.redo();
            assert_eq!(s.editor.project(), &expected);
        }
    }
}

#[test]
fn all_pose_noops_reset_away_back_and_scope_alone_preserve_redo_and_source_bytes() {
    for target in TARGETS {
        for kind in 0..3 {
            let mut s = scene(target, kind != 0, 0);
            if kind != 0 {
                with_unused(&mut s, target, kind == 2);
            }
            if kind == 0 && target == PathTarget::Shape {
                let mut old = serde_json::to_value(s.editor.project()).unwrap();
                old["version"] = 30.into();
                s.editor
                    .replace_project(Project::from_json(&old.to_string()).unwrap())
                    .unwrap();
                s.editor.select(1);
                s.editor.clear_history();
            }
            s.editor
                .execute(Command::RenameLayer {
                    id: 2,
                    name: "Whole-pose redo sentinel".into(),
                })
                .unwrap();
            let redo = s.editor.project().clone();
            s.editor.undo();
            let source = s.editor.project().clone();
            let bytes = libre_effects_core::project_file::encode(&source, None).unwrap();
            open(&mut s, target, &[0, 2]);
            let defaults = values(s.vertex_editor.as_ref().unwrap());
            let mut serial = switch(&mut s, TransformScope::AllPoses);
            for (index, text) in [(2, "-1080"), (5, "1e300"), (6, "-1e300")] {
                s.vertex_input(serial, index, text);
            }
            assert_eq!(s.vertex_editor.as_ref().unwrap().project(), &source);
            assert!(s.reset_vertex_editor(serial));
            serial = s.vertex_editor.as_ref().unwrap().id;
            assert_eq!(
                s.vertex_editor.as_ref().unwrap().scope(),
                TransformScope::AllPoses
            );
            assert_eq!(values(s.vertex_editor.as_ref().unwrap()), defaults);
            for (index, text) in [(0, "18.25"), (0, "0"), (2, "37.5"), (2, "0")] {
                s.vertex_input(serial, index, text);
            }
            switch(&mut s, TransformScope::ThisFrame);
            switch(&mut s, TransformScope::AllPoses);
            let session = s.vertex_editor.as_ref().unwrap();
            assert!(session.command().unwrap().is_none());
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

#[test]
fn scope_switch_commits_pending_text_atomically_and_rejects_bad_text_ime_and_stale_serials() {
    for target in TARGETS {
        let mut s = scene(target, true, 10);
        with_unused(&mut s, target, false);
        let source = s.editor.project().clone();
        let first = open(&mut s, target, &[0, 2]);
        s.vertex_input(first, 5, "-11.25");
        let accepted = values(s.vertex_editor.as_ref().unwrap());
        for (text, composing) in [("bad", false), ("NaN", false), ("17.25", true)] {
            assert!(!s.switch_vertex_scope(
                first,
                TransformScope::AllPoses,
                Some((0, text)),
                composing
            ));
            let session = s.vertex_editor.as_ref().unwrap();
            assert_eq!(session.id, first);
            assert_eq!(session.scope(), TransformScope::ThisFrame);
            assert_eq!(values(session), accepted);
            assert_eq!(session.project(), &source);
        }
        assert!(s.switch_vertex_scope(first, TransformScope::AllPoses, Some((0, "17.25")), false));
        let session = s.vertex_editor.as_ref().unwrap();
        let next = session.id;
        assert_ne!(next, first);
        assert_eq!(session.value(0), Some(17.25));
        assert_eq!(session.value(5), Some(-11.25));
        assert!(session.error.is_empty());
        let all = session.project().clone();
        assert!(!s.switch_vertex_scope(first, TransformScope::ThisFrame, Some((0, "999")), false));
        s.vertex_input(first, 0, "888");
        assert!(!s.reset_vertex_editor(first));
        assert_eq!(s.vertex_editor.as_ref().unwrap().project(), &all);
        let current = switch(&mut s, TransformScope::ThisFrame);
        let session = s.vertex_editor.as_ref().unwrap();
        assert_eq!(session.value(0), Some(17.25));
        assert_eq!(session.value(5), Some(-11.25));
        assert!(matches!(
            session.command().unwrap(),
            Some(Command::EditPath { frame: 10, .. })
        ));
        assert_eq!(
            target_base(session.project(), target),
            target_base(&source, target)
        );
        assert_ne!(session.project(), &all);
        assert!(s.reset_vertex_editor(current));
        assert_eq!(
            s.vertex_editor.as_ref().unwrap().scope(),
            TransformScope::ThisFrame
        );
        assert_eq!(s.vertex_editor.as_ref().unwrap().project(), &source);
        s.cancel_vertex_editor();
        assert_eq!(s.editor.project(), &source);
        assert!(!s.editor.can_undo());
    }
}

#[test]
fn invalid_stored_output_rejects_switch_and_pending_value_without_partial_acceptance() {
    for target in TARGETS {
        let mut s = scene(target, true, 10);
        edit_fixture(&mut s, target, |base, animation| {
            let mut unused: VectorPath = serde_json::from_value(base.clone()).unwrap();
            for index in [0, 2] {
                unused.vertices[index].position[0] = 999_999.;
            }
            animation["poses"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::to_value(unused).unwrap());
        });
        let source = s.editor.project().clone();
        let serial = open(&mut s, target, &[0, 2]);
        s.vertex_input(serial, 1, "-7.5");
        let before = s.vertex_editor.as_ref().unwrap().project().clone();
        let accepted = values(s.vertex_editor.as_ref().unwrap());
        assert!(!s.switch_vertex_scope(serial, TransformScope::AllPoses, Some((0, "25")), false));
        let session = s.vertex_editor.as_ref().unwrap();
        assert_eq!(session.id, serial);
        assert_eq!(session.scope(), TransformScope::ThisFrame);
        assert_eq!(session.project(), &before);
        assert_eq!(values(session), accepted);
        assert!(session.has_input_error(0));
        assert!(session.error.contains("Stored pose"));
        assert_eq!(s.editor.project(), &source);
        assert_eq!(s.revert_vertex_field(serial, 0), Some("0".into()));
        assert!(s.switch_vertex_scope(serial, TransformScope::AllPoses, None, false));
        let next = s.vertex_editor.as_ref().unwrap().id;
        let valid = s.vertex_editor.as_ref().unwrap().project().clone();
        s.vertex_input(next, 0, "25");
        assert_eq!(s.vertex_editor.as_ref().unwrap().project(), &valid);
        assert!(s.vertex_editor.as_ref().unwrap().has_input_error(0));
        assert_eq!(s.vertex_editor.as_ref().unwrap().value(0), Some(0.));
    }
}

#[test]
fn valid_stored_poses_do_not_allow_invalid_opening_interpolated_sample() {
    let mut s = scene(PathTarget::Shape, true, 10);
    edit_fixture(&mut s, PathTarget::Shape, |base, animation| {
        let path = |x| VectorPath {
            closed: false,
            vertices: vec![PathVertex::corner([x, 0.]); 4],
        };
        *base = serde_json::to_value(path(0.)).unwrap();
        animation["poses"] = json!([path(0.), path(450_000.)]);
        animation["timing"] = json!({"value": 0., "keys": {
            "0": {"value": 0., "interpolation": {"Bezier": {"x1": 1. / 3., "y1": 0., "x2": 2. / 3., "y2": 3.}}},
            "20": {"value": 1., "interpolation": "Linear"}
        }});
    });
    open(&mut s, PathTarget::Shape, &[0, 2]);
    let serial = switch(&mut s, TransformScope::AllPoses);
    s.vertex_input(serial, 5, "0");
    let before = s.vertex_editor.as_ref().unwrap().project().clone();
    s.vertex_input(serial, 3, "200");
    let session = s.vertex_editor.as_ref().unwrap();
    assert!(session.error.contains("opening frame"), "{}", session.error);
    assert!(session.has_input_error(3));
    assert_eq!(session.value(3), Some(100.));
    assert_eq!(session.project(), &before);
    assert!(session.path().valid());
}

#[test]
fn singleton_scope_stays_absolute_and_context_changes_cancel_scope_callbacks() {
    let mut singleton = scene(PathTarget::Shape, false, 0);
    let first = open(&mut singleton, PathTarget::Shape, &[1]);
    let source = singleton.editor.project().clone();
    assert!(!singleton.switch_vertex_scope(first, TransformScope::AllPoses, None, false));
    let session = singleton.vertex_editor.as_ref().unwrap();
    assert_eq!(session.scope(), TransformScope::ThisFrame);
    assert!(!session.is_transform());
    assert_eq!(session.field_count(), 6);
    assert_eq!(session.project(), &source);
    singleton.vertex_input(first, 0, "48.5");
    assert_eq!(
        singleton.vertex_editor.as_ref().unwrap().path().vertices[1].position[0],
        48.5
    );
    for change in 0..5 {
        let mut s = scene(PathTarget::Contents(2), true, 10);
        open(&mut s, PathTarget::Contents(2), &[0, 2]);
        let serial = switch(&mut s, TransformScope::AllPoses);
        match change {
            0 => s.frame += 1,
            1 => s.document_revision += 1,
            2 => s.playing = true,
            3 => s.contents_selection = None,
            _ => s
                .editor
                .execute(Command::RenameLayer {
                    id: 2,
                    name: "Changed source".into(),
                })
                .unwrap(),
        }
        let changed = s.editor.project().clone();
        assert!(!s.switch_vertex_scope(serial, TransformScope::ThisFrame, Some((0, "14")), false));
        assert!(s.vertex_editor.is_none());
        assert!(s.vertex_return.is_none());
        assert_eq!(s.editor.project(), &changed);
    }
}
