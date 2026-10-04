use super::*;
use std::sync::OnceLock;

fn metadata_size(project: &Project) -> usize {
    let prepared = Prepared::new(project);
    count_json(
        &prepared.metadata(),
        false,
        prepared.image_references * 2,
        usize::MAX,
        METADATA_LIMIT_ERROR,
    )
    .unwrap()
}

/// A structurally valid project right at the real budget, with small in-memory
/// text that expands when JSON-escaped. Avoid hundreds of incremental edits.
fn budget_project(spare: usize) -> Project {
    static FULL: OnceLock<Project> = OnceLock::new();
    let mut project = FULL
        .get_or_init(|| {
            let mut editor = Editor::default();
            editor
                .execute(Command::AddContent {
                    content: Content::Text {
                        text: "\u{0001}".repeat(16384),
                        font_size: 24.0,
                    },
                    width: 10.0,
                    height: 10.0,
                    name: "Text".into(),
                })
                .unwrap();
            let mut project = editor.project().clone();
            let template = project.composition.layers[0].clone();
            project.composition.layers = (1..=180)
                .map(|id| {
                    let mut layer = template.clone();
                    layer.id = id;
                    layer
                })
                .collect();
            project.next_layer_id = 181;
            let mut remaining = (metadata_size(&project) - MAX_METADATA_BYTES).div_ceil(5);
            for layer in &mut project.composition.layers {
                let Content::Text { text, .. } = &mut layer.content else {
                    unreachable!()
                };
                let count = remaining.min(text.len());
                *text = "x".repeat(count) + &"\u{0001}".repeat(text.len() - count);
                remaining -= count;
            }
            assert_eq!(remaining, 0);
            let padding = MAX_METADATA_BYTES - metadata_size(&project);
            assert!(padding < 5);
            project.composition.name.push_str(&"x".repeat(padding));
            project.validate().unwrap();
            assert_eq!(metadata_size(&project), MAX_METADATA_BYTES);
            project
        })
        .clone();
    let Content::Text { text, .. } = &mut project.composition.layers[0].content else {
        unreachable!()
    };
    assert!(text.bytes().all(|byte| byte == b'x'));
    text.truncate(text.len() - spare);
    project
}

fn editor_near_budget(spare: usize) -> Editor {
    let mut editor = Editor::default();
    editor.replace_project(budget_project(spare)).unwrap();
    editor.execute(Command::ToggleVisible(1)).unwrap();
    editor.undo();
    editor.select(2);
    assert!(editor.can_undo());
    assert!(editor.can_redo());
    editor
}

fn assert_rejected(editor: &mut Editor, command: Command) {
    let current = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    let error = editor.execute(command).unwrap_err();
    assert!(error.contains(METADATA_LIMIT_ERROR), "{error}");
    assert_eq!(editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
}

#[test]
fn ordinary_edits_preserve_last_good_document_selection_and_history() {
    let mut editor = editor_near_budget(8);
    assert_rejected(
        &mut editor,
        Command::RenameLayer {
            id: 1,
            name: "a".repeat(1024),
        },
    );
    assert_rejected(
        &mut editor,
        Command::SetValue {
            id: 1,
            property: Property::Rotation,
            frame: 0,
            value: 123456.123456789,
        },
    );
    let json = editor.project().to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
    editor.redo();
    assert!(!editor.project().composition.layers[0].visible);
    editor.undo();
    assert_eq!(editor.selected(), Some(2));
}

#[test]
fn batches_are_budgeted_as_one_atomic_candidate() {
    let mut editor = editor_near_budget(10);
    let rename = |id| Command::RenameLayer {
        id,
        name: "Textabcdef".into(),
    };
    assert_rejected(
        &mut editor,
        Command::Batch(vec![rename(1), Command::Batch(vec![rename(2)])]),
    );
    // Each individual edit fits; only their combined result exceeds the budget.
    editor.execute(rename(1)).unwrap();
    assert_rejected(&mut editor, rename(2));
    // Temporary growth is allowed when the final atomic result still fits.
    editor
        .execute(Command::Batch(vec![
            Command::RenameLayer {
                id: 2,
                name: "a".repeat(1024),
            },
            Command::RenameLayer {
                id: 2,
                name: "Text".into(),
            },
        ]))
        .unwrap();
}

#[test]
fn duplication_and_inactive_compositions_share_the_same_budget() {
    let mut editor = editor_near_budget(64);
    assert_rejected(&mut editor, Command::DuplicateLayer(1));
    assert_rejected(&mut editor, Command::DuplicateLayers(vec![1, 2]));
    assert_rejected(&mut editor, Command::DuplicateComposition);
    // A small active composition cannot hide an oversized inactive one.
    let mut project = budget_project(0);
    project.composition.layers[0].name.push('x');
    project
        .other_compositions
        .insert(2, project.composition.clone());
    project
        .other_compositions
        .get_mut(&2)
        .unwrap()
        .layers
        .clear();
    std::mem::swap(
        &mut project.composition,
        project.other_compositions.get_mut(&2).unwrap(),
    );
    project.next_composition_id = 3;
    project.version = 9;
    project.validate().unwrap();
    let before = editor.current.clone();
    assert!(
        editor
            .replace_project(project)
            .unwrap_err()
            .contains(METADATA_LIMIT_ERROR)
    );
    assert_eq!(editor.current, before);
}

#[test]
fn exact_budget_is_saveable_but_oversized_loads_and_replacements_are_rejected() {
    let project = budget_project(0);
    let json = project.to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), project);
    let mut oversized = project;
    oversized.composition.layers[0].name.push('x');
    oversized.validate().unwrap();
    assert!(
        oversized
            .to_json()
            .unwrap_err()
            .contains(METADATA_LIMIT_ERROR)
    );
    let raw = serde_json::to_string(&oversized).unwrap();
    assert!(
        Project::from_json(&raw)
            .unwrap_err()
            .contains(METADATA_LIMIT_ERROR)
    );
    let mut editor = editor_near_budget(16);
    let current = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    assert!(
        editor
            .replace_project(oversized)
            .unwrap_err()
            .contains(METADATA_LIMIT_ERROR)
    );
    assert_eq!(editor.current, current);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
}

fn sequence(paths: Arc<Vec<String>>, name: &str) -> Command {
    Command::ImportAsset {
        content: Content::ImageSequence {
            frames: paths,
            fps: 30.into(),
            missing: MissingFramePolicy::Error,
            start_frame: 0,
            playback: Default::default(),
        },
        width: 10.0,
        height: 10.0,
        name: name.into(),
        folder: None,
        frame: Some(0),
    }
}

#[test]
fn shared_manifests_count_once_and_sequence_growth_is_rejected_atomically() {
    let mut editor = Editor::default();
    let large = Arc::new(vec!["a".repeat(8192); 1024]);
    editor.execute(sequence(large.clone(), "First")).unwrap();
    editor.execute(Command::DuplicateComposition).unwrap();
    editor.execute(Command::DuplicateLayers(vec![2])).unwrap();
    assert!(metadata_size(editor.project()) < 9 * 1024 * 1024);
    editor
        .execute(sequence(Arc::new(vec!["b".repeat(1024); 1024]), "Second"))
        .unwrap();
    editor.execute(Command::ToggleVisible(3)).unwrap();
    editor.undo();
    let larger = Arc::new(vec!["b".repeat(8192); 1024]);
    assert_rejected(
        &mut editor,
        Command::RelinkSequence {
            asset: 2,
            frames: larger.clone(),
        },
    );
    assert_rejected(&mut editor, sequence(larger, "Third"));
    let json = editor.project().to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
}

#[test]
fn budget_count_matches_saved_metadata_with_escaping_and_asset_references() {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Image { png: "YWJj".into() },
            width: 10.0,
            height: 10.0,
            name: "\\\"\n雪".into(),
        })
        .unwrap();
    let frames = Arc::new(vec!["C:\\雪\\\"shot\"\t.png".into(), "next.png".into()]);
    editor.execute(sequence(frames, "Sequence")).unwrap();
    editor.execute(Command::DuplicateComposition).unwrap();
    let json = editor.project().to_json().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value.as_object_mut().unwrap().remove("image_assets");
    assert_eq!(
        metadata_size(editor.project()),
        serde_json::to_vec(&value).unwrap().len()
    );
    assert_eq!(Project::from_json(&json).unwrap(), *editor.project());
    // Equal manifests still deduplicate even when they do not share an Arc;
    // the pointer fast path must not change the document's content identity.
    let mut independent = editor.project().clone();
    for asset in independent.asset_library.assets.values_mut() {
        if let Content::ImageSequence { frames, .. } = &mut asset.content {
            *frames = Arc::new((**frames).clone());
        }
    }
    for layer in independent.compositions_mut().flat_map(|c| &mut c.layers) {
        if let Content::ImageSequence { frames, .. } = &mut layer.content {
            *frames = Arc::new((**frames).clone());
        }
    }
    assert_eq!(Prepared::new(&independent).sequences.len(), 1);
    assert_eq!(independent.to_json().unwrap(), json);
}

#[test]
fn bounded_counter_and_compact_fallback_preserve_exact_json() {
    let value = serde_json::json!({"nested": [[[[["\\\"雪\n", 1, 2, 3]]]]]});
    let compact = serde_json::to_string(&value).unwrap();
    let pretty = serde_json::to_string_pretty(&value).unwrap();
    assert_eq!(
        count_json(&value, false, 0, compact.len(), "limit").unwrap(),
        compact.len()
    );
    assert!(count_json(&value, false, 0, compact.len() - 1, "limit").is_err());
    assert!(count_json(&value, false, usize::MAX, usize::MAX, "limit").is_err());
    assert_eq!(encode_value(&value, pretty.len()).unwrap(), pretty);
    assert_eq!(encode_value(&value, pretty.len() - 1).unwrap(), compact);
}

#[test]
fn metadata_counter_stops_before_visiting_an_oversized_payload() {
    use serde::ser::SerializeSeq;
    use std::cell::Cell;
    struct ManyItems(Cell<usize>);
    impl Serialize for ManyItems {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut sequence = serializer.serialize_seq(Some(100_000))?;
            for _ in 0..100_000 {
                self.0.set(self.0.get() + 1);
                sequence.serialize_element("a long metadata item")?;
            }
            sequence.end()
        }
    }
    let value = ManyItems(Cell::new(0));
    assert!(count_json(&value, false, 0, 64, METADATA_LIMIT_ERROR).is_err());
    assert!(value.0.get() < 10);
}

#[test]
fn native_metadata_uses_the_same_exact_budget_as_legacy_json() {
    let project = budget_project(0);
    let bytes = crate::project_file::encode(&project, None).unwrap();
    assert_eq!(
        crate::project_file::decode(&bytes).unwrap().project,
        project
    );
    let mut oversized = project;
    oversized.composition.layers[0].name.push('x');
    assert!(
        crate::project_file::encode(&oversized, None)
            .unwrap_err()
            .contains(METADATA_LIMIT_ERROR)
    );
}

fn whole_pose_budget_editor() -> Editor {
    let mut project = budget_project(4096);
    project.version = PROJECT_VERSION;
    project.composition.layers[0].path_masks = vec![PathMask {
        id: 1,
        path: VectorPath {
            closed: true,
            vertices: [[0., 0.], [10., 0.], [0., 10.]]
                .map(PathVertex::corner)
                .to_vec(),
        },
        ..Default::default()
    }];
    project.composition.layers[0].next_mask_id = 2;
    let padding = MAX_METADATA_BYTES - metadata_size(&project);
    let Content::Text { text, .. } = &mut project.composition.layers[0].content else {
        unreachable!()
    };
    text.push_str(&"x".repeat(padding));
    assert!(text.len() <= 16384);
    assert_eq!(metadata_size(&project), MAX_METADATA_BYTES);
    let mut editor = Editor::default();
    editor.replace_project(project).unwrap();
    editor
        .execute(Command::SetValue {
            id: 2,
            property: Property::PositionX,
            frame: 0,
            value: 0.,
        })
        .unwrap();
    editor.undo();
    editor.select(1);
    assert!(editor.can_undo() && editor.can_redo());
    editor
}

fn whole_pose_shift(dx: f64) -> Command {
    Command::TransformPathPoses {
        id: 1,
        target: PathTarget::Mask(1),
        indices: [0].into(),
        transform: PathTransformSpec {
            translation: [dx, 0.],
            ..Default::default()
        },
    }
}

#[test]
fn whole_pose_output_metadata_growth_rejects_atomically_and_exact_noop_is_saveable() {
    let mut editor = whole_pose_budget_editor();
    let before = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    editor.execute(whole_pose_shift(0.)).unwrap();
    assert_eq!(editor.current, before);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    assert_rejected(&mut editor, whole_pose_shift(0.123456789012345));
    assert_rejected(
        &mut editor,
        Command::Batch(vec![Command::Batch(vec![whole_pose_shift(
            0.123456789012345,
        )])]),
    );
    // A pure batch that returns exactly to the source is still one atomic no-op.
    editor
        .execute(Command::Batch(vec![
            whole_pose_shift(0.125),
            whole_pose_shift(-0.125),
        ]))
        .unwrap();
    assert_eq!(editor.current, before);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    let bytes = project_file::encode(editor.project(), None).unwrap();
    assert_eq!(
        project_file::decode(&bytes).unwrap().project,
        *editor.project()
    );
}

#[test]
fn whole_pose_identity_rejects_oversized_input_instead_of_skipping_budget_validation() {
    let mut editor = whole_pose_budget_editor();
    editor.current.project.composition.layers[0].name.push('x');
    assert_rejected(&mut editor, whole_pose_shift(0.));
    assert_rejected(
        &mut editor,
        Command::Batch(vec![Command::Batch(vec![whole_pose_shift(0.)])]),
    );
}

fn contents_move_budget_editor(spare: usize) -> Editor {
    let mut shape = Editor::default();
    shape
        .execute(Command::AddContent {
            content: Content::Shape(Shape::default()),
            width: 10.,
            height: 10.,
            name: "Contents budget".into(),
        })
        .unwrap();
    for edit in [
        ContentsEdit::Promote,
        ContentsEdit::Add {
            parent: 0,
            kind: ContentsKind::Group(vec![]),
        },
    ] {
        shape.execute(Command::Contents { id: 1, edit }).unwrap();
    }
    let mut project = budget_project(8192);
    project.version = PROJECT_VERSION;
    let mut layer = shape.project().composition.layer(1).unwrap().clone();
    layer.id = 181;
    project.composition.layers.push(layer);
    project.next_layer_id = 182;
    let padding = MAX_METADATA_BYTES - metadata_size(&project) - spare;
    let Content::Text { text, .. } = &mut project.composition.layers[0].content else {
        unreachable!()
    };
    text.push_str(&"x".repeat(padding));
    assert!(text.len() <= 16384);
    assert_eq!(metadata_size(&project), MAX_METADATA_BYTES - spare);
    let mut editor = Editor::default();
    editor.replace_project(project).unwrap();
    // Seed Redo with a shrinking edit: toggling true to false would itself
    // consume an extra byte and fail before the move boundary is exercised.
    editor
        .execute(Command::RenameLayer {
            id: 2,
            name: "T".into(),
        })
        .unwrap();
    editor.undo();
    editor.select(181);
    assert!(editor.can_undo() && editor.can_redo());
    editor
}

fn contents_budget_move(source_parent: u64, items: &[u64], parent: u64, index: usize) -> Command {
    Command::Contents {
        id: 181,
        edit: ContentsEdit::MoveSiblings {
            source_parent,
            items: items.to_vec(),
            parent,
            index,
        },
    }
}

#[test]
fn contents_move_siblings_exact_metadata_boundary_and_candidate_growth_are_atomic() {
    // Emptying a populated source into an already populated destination adds
    // exactly one serialized comma, even though every node payload is unchanged.
    let command = contents_budget_move(1, &[4, 2, 3], 0, 0);
    let mut boundary = contents_move_budget_editor(1);
    let before = boundary.current.clone();
    let undo = boundary.undo.clone();
    boundary.execute(command.clone()).unwrap();
    assert_eq!(metadata_size(boundary.project()), MAX_METADATA_BYTES);
    assert_eq!(boundary.undo.len(), undo.len() + 1);
    assert!(boundary.redo.is_empty());
    let after = boundary.current.clone();
    boundary.undo();
    assert_eq!(boundary.current, before);
    assert_eq!(boundary.undo, undo);
    boundary.redo();
    assert_eq!(boundary.current, after);
    let bytes = project_file::encode(boundary.project(), None).unwrap();
    assert_eq!(
        project_file::decode(&bytes).unwrap().project,
        *boundary.project()
    );

    let mut full = contents_move_budget_editor(0);
    assert_rejected(&mut full, command.clone());
    assert_rejected(
        &mut full,
        Command::Batch(vec![Command::Batch(vec![command])]),
    );
}

#[test]
fn contents_move_siblings_exact_roundtrip_at_metadata_limit_preserves_both_histories() {
    let mut editor = contents_move_budget_editor(0);
    let before = editor.current.clone();
    let undo = editor.undo.clone();
    let redo = editor.redo.clone();
    // Intermediate metadata can exceed the budget; the transaction validates
    // its original source and final candidate, with no partial publication.
    editor
        .execute(Command::Batch(vec![
            contents_budget_move(1, &[4, 2, 3], 0, 0),
            Command::Batch(vec![contents_budget_move(0, &[4, 3, 2], 1, 0)]),
        ]))
        .unwrap();
    assert_eq!(editor.current, before);
    assert_eq!(editor.undo, undo);
    assert_eq!(editor.redo, redo);
    assert_eq!(metadata_size(editor.project()), MAX_METADATA_BYTES);
}

#[test]
fn contents_move_siblings_rejects_oversized_source_even_when_move_would_repair_its_budget() {
    let mut editor = contents_move_budget_editor(0);
    // Moving one of several children into an empty group removes one comma.
    let shrinking = contents_budget_move(1, &[2], 5, 0);
    let mut candidate = editor.current.clone();
    apply(&mut candidate, shrinking.clone()).unwrap();
    assert_eq!(metadata_size(&candidate.project), MAX_METADATA_BYTES - 1);
    editor.current.project.composition.name.push('x');
    assert_eq!(metadata_size(editor.project()), MAX_METADATA_BYTES + 1);
    assert_rejected(&mut editor, shrinking.clone());
    assert_rejected(
        &mut editor,
        Command::Batch(vec![Command::Batch(vec![shrinking])]),
    );
    assert_rejected(
        &mut editor,
        Command::Batch(vec![
            contents_budget_move(1, &[2], 5, 0),
            contents_budget_move(5, &[2], 1, 0),
        ]),
    );
}
