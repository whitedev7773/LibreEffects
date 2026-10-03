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
