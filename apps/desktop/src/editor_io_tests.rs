use super::*;
use crate::recovery::Session;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

fn relative_sequence_project() -> Project {
    static PROJECT: OnceLock<Project> = OnceLock::new();
    PROJECT
        .get_or_init(|| {
            let mut editor = Editor::default();
            // Three separate manifests fit the compact 16 MiB budget using
            // relative paths, but expanding their 12,288 entries pushes them
            // over it. Each individual manifest remains below its 8 MiB limit.
            for label in ['a', 'b', 'c'] {
                editor
                    .execute(Command::ImportAsset {
                        content: Content::ImageSequence {
                            frames: Arc::new(vec![
                                format!(
                                    "{label}/{}frame.png",
                                    "nested/".repeat(190)
                                );
                                4096
                            ]),
                            fps: 30.into(),
                            missing: Default::default(),
                            start_frame: 0,
                            playback: Default::default(),
                        },
                        width: 10.0,
                        height: 10.0,
                        name: label.to_string(),
                        folder: None,
                        frame: None,
                    })
                    .unwrap();
            }
            editor.project().clone()
        })
        .clone()
}

fn long_directory(root: &Path) -> PathBuf {
    let path = root.join("long-project-directory-".repeat(5));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    std::fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

fn dirty_state(recovery_root: &Path) -> EditorState {
    let mut state = EditorState::default();
    state.editor.execute(Command::AddRectangle).unwrap();
    state.saved = state.editor.project().clone();
    state.editor.execute(Command::AddRectangle).unwrap();
    state
        .editor
        .execute(Command::RenameLayer {
            id: 2,
            name: "Unsaved layer".into(),
        })
        .unwrap();
    state.editor.execute(Command::ToggleVisible(2)).unwrap();
    state.editor.undo();
    state.editor.select(1);
    state.selected_layers.insert(1);
    state.path = Some(recovery_root.parent().unwrap().join("old.lfe.json"));
    state.document_revision = 7;
    state.frame = 19;
    state.work_start = 5;
    state.work_end = 100;
    state.preview_pan = [12.0, 34.0];
    let (session, candidates, warnings) = Session::in_directory(recovery_root).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    session.checkpoint(0, Some(&state.saved)).unwrap();
    session.checkpoint(0, Some(state.editor.project())).unwrap();
    state.recovery_session = Some(Arc::new(Mutex::new(session)));
    state.recovery_pending = candidates.into();
    state.recovery = state.recovery_pending.pop_front();
    state.recovery_ready = state.recovery.is_none();
    assert!(state.dirty());
    assert!(state.editor.can_undo());
    assert!(state.editor.can_redo());
    state
}

struct Before {
    project: Project,
    saved: Project,
    path: Option<PathBuf>,
    ready: bool,
    candidate: Option<String>,
    pending: usize,
    files: BTreeMap<PathBuf, Vec<u8>>,
    generation: u64,
}
impl Before {
    fn capture(state: &EditorState, recovery_root: &Path) -> Self {
        Self {
            project: state.editor.project().clone(),
            saved: state.saved.clone(),
            path: state.path.clone(),
            ready: state.recovery_ready,
            candidate: state
                .recovery
                .as_ref()
                .map(|candidate| candidate.label.clone()),
            pending: state.recovery_pending.len(),
            files: files(recovery_root),
            generation: state
                .recovery_session
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .generation,
        }
    }
    fn assert_unchanged(self, state: &mut EditorState, recovery_root: &Path) {
        assert_eq!(state.editor.project(), &self.project);
        assert_eq!(state.saved, self.saved);
        assert_eq!(state.path, self.path);
        assert_eq!(state.editor.selected(), Some(1));
        assert_eq!(state.selected_layers, BTreeSet::from([1]));
        assert_eq!(state.document_revision, 7);
        assert_eq!(
            (state.frame, state.work_start, state.work_end),
            (19, 5, 100)
        );
        assert_eq!(state.preview_pan, [12.0, 34.0]);
        assert!(state.dirty());
        assert_eq!(state.recovery_ready, self.ready);
        assert_eq!(
            state
                .recovery
                .as_ref()
                .map(|candidate| candidate.label.clone()),
            self.candidate
        );
        assert_eq!(state.recovery_pending.len(), self.pending);
        assert_eq!(files(recovery_root), self.files);
        assert_eq!(
            state
                .recovery_session
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .generation,
            self.generation
        );
        assert!(state.editor.can_undo());
        assert!(state.editor.can_redo());
        state.editor.redo();
        assert!(
            !state
                .editor
                .project()
                .composition()
                .layer(2)
                .unwrap()
                .visible()
        );
        state.editor.undo();
        assert_eq!(state.editor.project(), &self.project);
        state.editor.undo();
        assert_ne!(
            state
                .editor
                .project()
                .composition()
                .layer(2)
                .unwrap()
                .name(),
            "Unsaved layer"
        );
        state.editor.redo();
        assert_eq!(state.editor.project(), &self.project);
    }
}

#[test]
fn open_rejects_expanded_relative_paths_without_changing_document_or_recovery() {
    let root = tempfile::tempdir().unwrap();
    let source = long_directory(root.path()).join("large.lfe.json");
    let json = relative_sequence_project().to_json().unwrap();
    assert!(json.len() > 15 * 1024 * 1024);
    std::fs::write(&source, &json).unwrap();
    let recovery_root = root.path().join("recovery");
    let mut state = dirty_state(&recovery_root);
    let before = Before::capture(&state, &recovery_root);
    // Exercise the real reader: parsing succeeds, and path resolution returns
    // a structurally valid model whose serialized metadata is now too large.
    let (project, views) = crate::project_io::read_editor_project(&source).unwrap();
    assert!(project.to_json().unwrap_err().contains("metadata exceeds"));
    let error = state
        .install_opened_project(project, views, source.clone())
        .unwrap_err();
    assert!(error.contains("metadata exceeds"), "{error}");
    before.assert_unchanged(&mut state, &recovery_root);
    assert_eq!(std::fs::read_to_string(source).unwrap(), json);
}

fn abandon(root: &Path, project: &Project) {
    let (session, _, _) = Session::in_directory(root).unwrap();
    session.checkpoint(0, Some(project)).unwrap();
}

#[test]
fn recovery_rejects_expanded_paths_without_consuming_either_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let recovery_root = long_directory(root.path());
    abandon(&recovery_root, &relative_sequence_project());
    let mut state = dirty_state(&recovery_root);
    assert!(state.recovery.is_some());
    let before = Before::capture(&state, &recovery_root);
    let error = state.apply_recovery(true).unwrap_err();
    assert!(error.contains("metadata exceeds"), "{error}");
    before.assert_unchanged(&mut state, &recovery_root);
}

#[test]
fn recovery_validates_editor_replacement_before_copying_or_discarding_slots() {
    let root = tempfile::tempdir().unwrap();
    let recovery_root = root.path().join("recovery");
    abandon(&recovery_root, &Project::default());
    let mut state = dirty_state(&recovery_root);
    let candidate = state.recovery.as_mut().unwrap();
    // This serializes within budget, so the recovery writer alone cannot catch
    // the invalid editor replacement. Validation must precede all disk changes.
    let mut value = serde_json::to_value(&candidate.project).unwrap();
    value["version"] = u32::MAX.into();
    candidate.project = serde_json::from_value(value).unwrap();
    candidate.project.to_json().unwrap();
    let before = Before::capture(&state, &recovery_root);
    let error = state.apply_recovery(true).unwrap_err();
    assert!(error.contains("Unsupported project version"), "{error}");
    before.assert_unchanged(&mut state, &recovery_root);
}

#[test]
fn accepted_open_replaces_baseline_and_clears_old_history_and_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let recovery_root = root.path().join("recovery");
    let mut state = dirty_state(&recovery_root);
    let path = root.path().join("new.lfe.json");
    let project = Project::default();
    state
        .install_opened_project(project.clone(), Default::default(), path.clone())
        .unwrap();
    assert_eq!(state.editor.project(), &project);
    assert_eq!(state.saved, project);
    assert_eq!(state.path, Some(path));
    assert_eq!(state.document_revision, 8);
    assert!(!state.dirty());
    assert!(!state.editor.can_undo());
    assert!(!state.editor.can_redo());
    assert!(
        files(&recovery_root)
            .keys()
            .all(|path| path.extension().is_some_and(|e| e == "lock"))
    );
}
