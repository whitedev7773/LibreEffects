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
    state.path = Some(recovery_root.parent().unwrap().join("old.lep"));
    state.source_format = Some(ProjectFormat::Lep);
    state.imported_original = Some(recovery_root.parent().unwrap().join("original.lfe.json"));
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
    format: Option<ProjectFormat>,
    original: Option<PathBuf>,
    views: BTreeMap<CompositionId, crate::view_state::CompositionView>,
    workspace: crate::view_state::WorkspaceView,
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
            format: state.source_format,
            original: state.imported_original.clone(),
            views: state.composition_views.clone(),
            workspace: state.workspace.clone(),
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
        assert_eq!(state.source_format, self.format);
        assert_eq!(state.imported_original, self.original);
        assert_eq!(state.composition_views, self.views);
        assert_eq!(state.workspace, self.workspace);
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
    let opened = crate::project_io::read_editor_project(&source).unwrap();
    assert!(
        opened
            .project
            .to_json()
            .unwrap_err()
            .contains("metadata exceeds")
    );
    let error = state
        .install_opened_project(opened, source.clone())
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
    let path = root.path().join("new.lep");
    let project = Project::default();
    state
        .install_opened_project(
            OpenedProject {
                project: project.clone(),
                views: Default::default(),
                format: ProjectFormat::Lep,
            },
            path.clone(),
        )
        .unwrap();
    assert_eq!(state.editor.project(), &project);
    assert_eq!(state.saved, project);
    assert_eq!(state.path, Some(path));
    assert_eq!(state.source_format, Some(ProjectFormat::Lep));
    assert_eq!(state.imported_original, None);
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

fn open_test_project(state: &mut EditorState, path: &Path) -> Result<(), String> {
    let opened = crate::project_io::read_editor_project(path)?;
    state.install_opened_project(opened, path.to_path_buf())
}

fn save_test_project(state: &mut EditorState, path: &Path) -> Result<(), String> {
    let snapshot = state.editor.project().clone();
    let views = state.capture_views();
    let result = selected_native_destination(path).and_then(|destination| {
        crate::media_io::save_protected(
            &snapshot,
            &views,
            &destination,
            state.imported_original.as_deref(),
        )?;
        Ok(destination)
    });
    match result {
        Ok(destination) => {
            state.finish_save(state.file_operation, snapshot, destination, Ok(()));
            Ok(())
        }
        Err(error) => {
            state.finish_save(
                state.file_operation,
                snapshot,
                path.to_path_buf(),
                Err(error.clone()),
            );
            Err(error)
        }
    }
}

#[test]
fn file_action_blur_uses_actual_save_routing() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = EditorState::default();
    assert!(state.should_blur_for_file_action(&Action::Save));
    assert!(state.should_blur_for_file_action(&Action::SaveAs));
    for (name, format, blur) in [
        ("native.lep", ProjectFormat::Lep, false),
        ("native.LEP", ProjectFormat::Lep, false),
        ("native.png", ProjectFormat::Lep, true),
        ("legacy.json", ProjectFormat::LegacyJson, true),
        ("legacy.lep", ProjectFormat::LegacyJson, true),
    ] {
        let source = directory.path().join(name);
        let project = Project::default();
        if format == ProjectFormat::Lep {
            crate::project_io::write_native_project(&source, &project, None).unwrap();
        } else {
            std::fs::write(&source, project.to_json().unwrap()).unwrap();
        }
        open_test_project(&mut state, &source).unwrap();
        assert_eq!(
            state.should_blur_for_file_action(&Action::Save),
            blur,
            "{name}"
        );
        assert!(state.should_blur_for_file_action(&Action::SaveAs), "{name}");
    }
}

#[test]
fn file_action_blur_skips_in_progress_save_noops() {
    let mut state = EditorState::default();
    state.saving = true;
    assert!(!state.should_blur_for_file_action(&Action::Save));
    assert!(!state.should_blur_for_file_action(&Action::SaveAs));
    state.path = Some(PathBuf::from("native.lep"));
    state.source_format = Some(ProjectFormat::Lep);
    assert!(!state.should_blur_for_file_action(&Action::Save));
    assert!(!state.should_blur_for_file_action(&Action::SaveAs));
    // Saving does not suppress the other file-dialog safeguards.
    assert!(state.should_blur_for_file_action(&Action::Open));
}

#[test]
fn file_action_blur_preserves_other_chooser_guards() {
    let state = EditorState::default();
    for action in [
        Action::Open,
        Action::CollectFiles,
        Action::RelinkSource("source.png".into()),
        Action::RelinkMissing,
        Action::ImportImageSequence,
        Action::RelinkSequence(1),
        Action::ImportImage,
        Action::ImportVideo,
        Action::RelinkVideo,
        Action::ExportFrame,
        Action::ExportFrameBackground,
        Action::ExportSequence,
        Action::ExportSequenceBackground,
        Action::ExportVideo(crate::video_export::VideoPreset::H264),
    ] {
        assert!(state.should_blur_for_file_action(&action));
    }
    for action in [Action::Undo, Action::Redo, Action::DeleteSelection] {
        assert!(!state.should_blur_for_file_action(&action));
    }
}

#[test]
fn native_save_chooser_names_and_normalization_never_imply_an_unconfirmed_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = EditorState::default();
    assert_eq!(state.save_path(false), None);
    assert_eq!(state.suggested_save_name(), "Untitled.lep");
    assert_eq!(state.save_directory(), Path::new("."));
    for (source, expected) in [
        ("Legacy.lfe.json", "Legacy.lep"),
        ("Legacy.LFE.JSON", "Legacy.lep"),
        ("Legacy.json", "Legacy.lep"),
        ("Legacy.png", "Legacy.png.lep"),
        ("Legacy.lep", "Legacy-copy.lep"),
    ] {
        let source = directory.path().join(source);
        std::fs::write(&source, Project::default().to_json().unwrap()).unwrap();
        open_test_project(&mut state, &source).unwrap();
        assert_eq!(state.path, Some(source.clone()));
        assert_eq!(state.source_format, Some(ProjectFormat::LegacyJson));
        assert_eq!(state.imported_original, Some(source));
        assert_eq!(state.save_path(false), None);
        assert_eq!(state.save_path(true), None);
        assert_eq!(state.suggested_save_name(), expected);
        assert_eq!(state.save_directory(), directory.path());
    }
    let selected = directory.path().join("copy");
    let normalized = directory.path().join("copy.lep");
    assert_eq!(selected_native_destination(&selected).unwrap(), normalized);
    std::fs::write(&normalized, b"existing native destination").unwrap();
    assert!(
        selected_native_destination(&selected)
            .unwrap_err()
            .contains("explicitly")
    );
    assert_eq!(
        selected_native_destination(&normalized).unwrap(),
        normalized
    );
    assert_eq!(
        std::fs::read(&normalized).unwrap(),
        b"existing native destination"
    );
}

#[test]
fn imported_original_survives_native_save_copy_and_repeated_save_without_changing_bytes() {
    let directory = tempfile::tempdir().unwrap();
    // The source's suffix is not evidence of its file format.
    let source = directory.path().join("original.lep");
    let original = Project::default().to_json().unwrap();
    std::fs::write(&source, &original).unwrap();
    let mut state = EditorState::default();
    open_test_project(&mut state, &source).unwrap();
    state.editor.execute(Command::AddRectangle).unwrap();
    state.preview_pan = [11.0, 13.0];
    assert!(save_test_project(&mut state, &source).is_err());
    assert!(state.dirty());
    assert_eq!(state.source_format, Some(ProjectFormat::LegacyJson));
    let copy = directory.path().join("copy.lep");
    save_test_project(&mut state, &copy).unwrap();
    assert!(!state.dirty());
    assert_eq!(state.path, Some(copy.clone()));
    assert_eq!(state.save_path(false), Some(copy.clone()));
    assert_eq!(state.save_path(true), None);
    assert!(!state.should_blur_for_file_action(&Action::Save));
    assert!(state.should_blur_for_file_action(&Action::SaveAs));
    assert_eq!(state.source_format, Some(ProjectFormat::Lep));
    assert_eq!(state.imported_original, Some(source.clone()));
    assert!(state.editor.can_undo());
    let opened = crate::project_io::read_editor_project(&copy).unwrap();
    assert_eq!(opened.format, ProjectFormat::Lep);
    assert_eq!(&opened.project, state.editor.project());
    assert_eq!(opened.views.compositions[&1].preview_pan, [11.0, 13.0]);
    let first = std::fs::read(&copy).unwrap();
    save_test_project(&mut state, &copy).unwrap();
    assert_eq!(std::fs::read(&copy).unwrap(), first);
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
    let hardlink = directory.path().join("original-alias.lep");
    std::fs::hard_link(&source, &hardlink).unwrap();
    assert!(save_test_project(&mut state, &hardlink).is_err());
    assert_eq!(state.path, Some(copy));
    assert_eq!(state.imported_original, Some(source.clone()));
    assert_eq!(std::fs::read_to_string(&hardlink).unwrap(), original);
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
}

#[test]
fn misleading_native_suffix_requires_copy_and_cancellation_or_failed_save_keeps_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("native.png");
    let bytes = crate::project_io::encode_native_project(&Project::default(), None).unwrap();
    std::fs::write(&source, &bytes).unwrap();
    let mut state = EditorState::default();
    open_test_project(&mut state, &source).unwrap();
    assert_eq!(state.source_format, Some(ProjectFormat::Lep));
    assert_eq!(state.save_path(false), None);
    assert_eq!(state.suggested_save_name(), "native.png.lep");
    state.editor.execute(Command::AddRectangle).unwrap();
    let saved = state.saved.clone();
    state.saving = true;
    state.close_after_save = true;
    state.cancel_save(state.file_operation);
    assert!(!state.saving);
    assert!(!state.close_after_save);
    assert_eq!(state.path, Some(source.clone()));
    assert_eq!(state.imported_original, Some(source.clone()));
    assert_eq!(state.saved, saved);
    assert!(state.dirty());
    let blocked = directory.path().join("absent/copy.lep");
    state.saving = true;
    state.close_after_save = true;
    assert!(save_test_project(&mut state, &blocked).is_err());
    assert!(!state.saving);
    assert!(!state.close_after_save);
    assert_eq!(state.path, Some(source.clone()));
    assert_eq!(state.imported_original, Some(source.clone()));
    assert_eq!(state.saved, saved);
    assert!(state.editor.can_undo());
    assert!(state.dirty());
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
}

#[test]
fn failed_native_or_future_open_preserves_document_history_views_provenance_and_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let native = crate::project_io::encode_native_project(&Project::default(), None).unwrap();
    let mut future = native.clone();
    future[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    let mut corrupt = native.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    let mut future_model = serde_json::to_value(Project::default()).unwrap();
    future_model["version"] = u32::MAX.into();
    for (name, bytes) in [
        ("future.lep", future),
        ("corrupt.lep", corrupt),
        ("truncated.lep", native[..20].to_vec()),
        (
            "future-json.lep",
            serde_json::to_vec(&future_model).unwrap(),
        ),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        let recovery_root = directory.path().join(format!("recovery-{name}"));
        let mut state = dirty_state(&recovery_root);
        let before = Before::capture(&state, &recovery_root);
        assert!(open_test_project(&mut state, &path).is_err());
        before.assert_unchanged(&mut state, &recovery_root);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn new_recovery_and_subsequent_open_replace_source_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let recovery_root = directory.path().join("recovery");
    let mut state = dirty_state(&recovery_root);
    state.install_new_project().unwrap();
    assert_eq!(state.path, None);
    assert_eq!(state.source_format, None);
    assert_eq!(state.imported_original, None);
    assert_eq!(state.suggested_save_name(), "Untitled.lep");
    assert!(!state.dirty());
    assert!(!state.editor.can_undo());
    drop(state);
    abandon(&recovery_root, &Project::default());
    let mut state = dirty_state(&recovery_root);
    assert!(state.recovery.is_some());
    state.apply_recovery(true).unwrap();
    assert_eq!(state.path, None);
    assert_eq!(state.source_format, None);
    assert_eq!(state.imported_original, None);
    let source = directory.path().join("legacy.lfe.json");
    std::fs::write(&source, Project::default().to_json().unwrap()).unwrap();
    open_test_project(&mut state, &source).unwrap();
    assert_eq!(state.imported_original, Some(source));
    let native = directory.path().join("new.lep");
    crate::project_io::write_native_project(&native, &Project::default(), None).unwrap();
    open_test_project(&mut state, &native).unwrap();
    assert_eq!(state.source_format, Some(ProjectFormat::Lep));
    assert_eq!(state.imported_original, None);
    assert_eq!(state.save_path(false), Some(native));
}

#[cfg(unix)]
#[test]
fn original_symlink_and_parent_alias_saves_are_rejected_after_native_copy() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.lep");
    let bytes = Project::default().to_json().unwrap();
    std::fs::write(&source, &bytes).unwrap();
    let mut state = EditorState::default();
    open_test_project(&mut state, &source).unwrap();
    let copy = directory.path().join("copy.lep");
    save_test_project(&mut state, &copy).unwrap();
    let alias = directory.path().join("alias.lep");
    symlink(&source, &alias).unwrap();
    assert!(save_test_project(&mut state, &alias).is_err());
    let parent_alias = directory.path().join("folder-alias");
    symlink(directory.path(), &parent_alias).unwrap();
    assert!(save_test_project(&mut state, &parent_alias.join("original.lep")).is_err());
    assert_eq!(state.path, Some(copy));
    assert_eq!(std::fs::read_to_string(&source).unwrap(), bytes);
    let dangling_target = directory.path().join("unconfirmed.lep");
    symlink(directory.path().join("missing"), &dangling_target).unwrap();
    assert!(selected_native_destination(&directory.path().join("unconfirmed")).is_err());
}

#[test]
fn stale_open_and_save_completions_cannot_replace_newer_document_or_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("original.json");
    std::fs::write(&legacy, Project::default().to_json().unwrap()).unwrap();
    let native = directory.path().join("other.lep");
    crate::project_io::write_native_project(&native, &Project::default(), None).unwrap();
    let mut state = EditorState::default();
    open_test_project(&mut state, &legacy).unwrap();
    let previous = state.editor.project().clone();
    let revision = state.document_revision;
    let pending_open = state.begin_file_operation();
    let pending_save = state.begin_file_operation();
    state.saving = true;
    state
        .finish_open(
            pending_open,
            revision,
            &previous,
            crate::project_io::read_editor_project(&native),
            native.clone(),
        )
        .unwrap();
    assert_eq!(state.path, Some(legacy.clone()));
    assert_eq!(state.source_format, Some(ProjectFormat::LegacyJson));
    assert!(state.saving);
    let copy = directory.path().join("copy.lep");
    state.finish_save(pending_save, previous.clone(), copy.clone(), Ok(()));
    assert_eq!(state.path, Some(copy));
    assert_eq!(state.imported_original, Some(legacy.clone()));
    // A newer document boundary invalidates both successful and failed old saves.
    let stale_save = state.begin_file_operation();
    state.install_new_project().unwrap();
    for result in [Ok(()), Err("late failure".into())] {
        state.finish_save(stale_save, previous.clone(), legacy.clone(), result);
        assert_eq!(state.path, None);
        assert_eq!(state.source_format, None);
        assert_eq!(state.imported_original, None);
        assert_eq!(state.status, "New composition");
        assert!(!state.saving);
    }
    // A canceled newer Open still supersedes an older in-flight read.
    let canceled_previous = state.editor.project().clone();
    let canceled_revision = state.document_revision;
    let old_open = state.begin_file_operation();
    state.begin_file_operation(); // Newer chooser is canceled without a read.
    state
        .finish_open(
            old_open,
            canceled_revision,
            &canceled_previous,
            crate::project_io::read_editor_project(&native),
            native.clone(),
        )
        .unwrap();
    assert_eq!(state.path, None);
    assert_eq!(state.status, "New composition");
    // Late callbacks may not clear a newer save's busy/close flags or status.
    let old_save = state.begin_file_operation();
    let new_save = state.begin_file_operation();
    state.saving = true;
    state.close_after_save = true;
    state.status = "Newer save pending".into();
    state.finish_save(
        old_save,
        previous.clone(),
        legacy.clone(),
        Err("stale failure".into()),
    );
    state.finish_save(old_save, previous.clone(), legacy.clone(), Ok(()));
    state.cancel_save(old_save);
    assert!(state.saving);
    assert!(state.close_after_save);
    assert_eq!(state.status, "Newer save pending");
    assert_eq!(state.path, None);
    state.cancel_save(new_save);
    // A second Open wins even if the first read fails after it completed.
    let first_open = state.begin_file_operation();
    let old_revision = state.document_revision;
    open_test_project(&mut state, &native).unwrap();
    state
        .finish_open(
            first_open,
            old_revision,
            &previous,
            Err("stale read failure".into()),
            legacy,
        )
        .unwrap();
    assert_eq!(state.path, Some(native));
    assert_eq!(state.status, "Project opened");
}

#[test]
fn editing_while_open_reads_prevents_replacement_and_a_save_keeps_newer_edits_dirty() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("incoming.lep");
    crate::project_io::write_native_project(&path, &Project::default(), None).unwrap();
    let mut state = EditorState::default();
    let previous = state.editor.project().clone();
    let revision = state.document_revision;
    let operation = state.begin_file_operation();
    state.editor.execute(Command::AddRectangle).unwrap();
    let changed = state.editor.project().clone();
    assert!(
        state
            .finish_open(
                operation,
                revision,
                &previous,
                crate::project_io::read_editor_project(&path),
                path.clone()
            )
            .unwrap_err()
            .contains("changed while opening")
    );
    assert_eq!(state.editor.project(), &changed);
    assert_eq!(state.path, None);
    assert!(state.editor.can_undo());
    let save = state.begin_file_operation();
    state.editor.execute(Command::AddRectangle).unwrap();
    state.finish_save(save, changed.clone(), path.clone(), Ok(()));
    assert_eq!(state.saved, changed);
    assert_eq!(state.path, Some(path));
    assert!(state.dirty());
    assert_eq!(state.editor.project().composition().layers().len(), 2);
}

#[test]
fn recovery_cleanup_warning_still_installs_the_durable_restored_project() {
    let directory = tempfile::tempdir().unwrap();
    let recovery_root = directory.path().join("recovery");
    std::fs::create_dir(&recovery_root).unwrap();
    let abandoned = recovery_root.join("session-abandoned.json");
    let recovered = Project::default();
    std::fs::write(&abandoned, recovered.to_json().unwrap()).unwrap();
    let mut state = dirty_state(&recovery_root);
    assert_eq!(state.recovery.as_ref().unwrap().project, recovered);
    // Simulate cleanup failure after the owned native copy was made durable.
    std::fs::create_dir(abandoned.with_extension("previous")).unwrap();
    state.apply_recovery(true).unwrap();
    assert_eq!(state.editor.project(), &recovered);
    assert_eq!(state.path, None);
    assert_eq!(state.source_format, None);
    assert_eq!(state.imported_original, None);
    assert!(state.recovery.is_none());
    assert!(state.recovery_ready);
    assert!(state.status.starts_with("Recovered checkpoint."));
    assert!(state.status.len() > "Recovered checkpoint. Save as to keep it. Other backups remain available at next startup.".len());
    assert_eq!(
        state
            .recovery_session
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .generation,
        1
    );
    let owned = std::fs::read_dir(&recovery_root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "lep"))
        .unwrap();
    assert_eq!(crate::project_io::read_project(&owned).unwrap(), recovered);
}

fn numeric_vertex_io_state() -> EditorState {
    use crate::panels::vertex_editor::{Request, Session as VertexSession};
    use libre_effects_core::{PathTarget, PathVertex, Shape, VectorPath};
    let path = VectorPath {
        closed: true,
        vertices: [[20., 20.], [160., 20.], [160., 140.], [20., 140.]]
            .map(PathVertex::corner)
            .to_vec(),
    };
    let mut state = EditorState::default();
    state
        .editor
        .execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(path.clone()),
                ..Default::default()
            }),
            width: 200.,
            height: 160.,
            name: "Numeric vertex I/O".into(),
        })
        .unwrap();
    state.editor.clear_history();
    state.saved = state.editor.project().clone();
    state.tool = crate::editor::Tool::Pen;
    let world = state
        .editor
        .project()
        .composition()
        .world_transform(1, 0)
        .unwrap();
    let request = Request::new(&state, 1, PathTarget::Shape, 0, path, world).unwrap();
    let mut session = VertexSession::new(&state, request).unwrap();
    session.input(0, "45.125").unwrap();
    state.vertex_editor = Some(session);
    state
}

#[test]
fn numeric_vertex_late_open_and_invalid_replacement_preserve_isolated_draft() {
    let mut state = numeric_vertex_io_state();
    let source = state.editor.project().clone();
    let draft = state.vertex_editor.as_ref().unwrap().project().clone();
    let serial = state.vertex_editor.as_ref().unwrap().id;
    let operation = state.begin_file_operation();
    let root = tempfile::tempdir().unwrap();
    let error = state
        .finish_open(
            operation,
            state.document_revision,
            &source,
            Ok(OpenedProject {
                project: Project::default(),
                views: Default::default(),
                format: ProjectFormat::Lep,
            }),
            root.path().join("late.lep"),
        )
        .unwrap_err();
    assert!(error.contains("changed while opening"));
    assert_eq!(state.editor.project(), &source);
    assert_eq!(state.vertex_editor.as_ref().unwrap().id, serial);
    assert_eq!(state.vertex_editor.as_ref().unwrap().project(), &draft);
    assert!(!state.editor.can_undo());
    let mut invalid = serde_json::to_value(Project::default()).unwrap();
    invalid["version"] = u32::MAX.into();
    let error = state
        .install_opened_project(
            OpenedProject {
                project: serde_json::from_value(invalid).unwrap(),
                views: Default::default(),
                format: ProjectFormat::Lep,
            },
            root.path().join("invalid.lep"),
        )
        .unwrap_err();
    assert!(error.contains("Unsupported project version"));
    assert_eq!(state.editor.project(), &source);
    assert_eq!(state.vertex_editor.as_ref().unwrap().id, serial);
    assert_eq!(state.vertex_editor.as_ref().unwrap().project(), &draft);
}

#[test]
fn numeric_vertex_valid_replacements_drop_sessions_return_tokens_and_late_callbacks() {
    let root = tempfile::tempdir().unwrap();
    for new_document in [false, true] {
        for return_only in [false, true] {
            let mut state = numeric_vertex_io_state();
            let serial = state.vertex_editor.as_ref().unwrap().id;
            if return_only {
                state.cancel_vertex_editor();
                assert!(state.vertex_editor.is_none());
                assert!(state.vertex_return.is_some());
            }
            if new_document {
                state.install_new_project().unwrap();
            } else {
                state
                    .install_opened_project(
                        OpenedProject {
                            project: Project::default(),
                            views: Default::default(),
                            format: ProjectFormat::Lep,
                        },
                        root.path().join("opened.lep"),
                    )
                    .unwrap();
            }
            assert!(state.vertex_editor.is_none());
            assert!(state.vertex_return.is_none());
            let fresh = state.editor.project().clone();
            state.vertex_input(serial, 0, "777");
            assert_eq!(state.editor.project(), &fresh);
            assert!(!state.editor.can_undo());
        }
    }
}

#[test]
fn numeric_vertex_direct_save_serializes_source_and_never_modal_geometry() {
    let root = tempfile::tempdir().unwrap();
    let mut state = numeric_vertex_io_state();
    let source = state.editor.project().clone();
    let draft = state.vertex_editor.as_ref().unwrap().project().clone();
    assert_ne!(source, draft);
    let path = root.path().join("source-only.lep");
    save_test_project(&mut state, &path).unwrap();
    let saved = crate::project_io::read_editor_project(&path).unwrap();
    assert_eq!(saved.project, source);
    assert_ne!(saved.project, draft);
    assert_eq!(state.editor.project(), &source);
    assert_eq!(state.vertex_editor.as_ref().unwrap().project(), &draft);
    assert!(!state.editor.can_undo());
}

#[test]
fn numeric_vertex_accepted_edit_stays_dirty_after_an_older_save_completes() {
    use crate::panels::vertex_editor::Session as VertexSession;
    let root = tempfile::tempdir().unwrap();
    let mut state = numeric_vertex_io_state();
    state.cancel_vertex_editor();
    let request = state.vertex_return.take().unwrap();
    let snapshot = state.editor.project().clone();
    let operation = state.begin_file_operation();
    state.saving = true;
    // A normal save is in flight before the modal opens. Its old snapshot must
    // become the saved baseline, never replace a later accepted geometry edit.
    let mut session = VertexSession::new(&state, request).unwrap();
    session.input(0, "75.123456789").unwrap();
    state.vertex_editor = Some(session);
    state.accept_vertex_editor();
    let edited = state.editor.project().clone();
    assert_ne!(edited, snapshot);
    assert!(state.dirty());
    state.finish_save(
        operation,
        snapshot.clone(),
        root.path().join("earlier.lep"),
        Ok(()),
    );
    assert!(!state.saving);
    assert_eq!(state.saved, snapshot);
    assert_eq!(state.editor.project(), &edited);
    assert!(state.dirty());
    state.editor.undo();
    assert_eq!(state.editor.project(), &snapshot);
    assert!(!state.dirty());
    assert!(!state.editor.can_undo());
    state.editor.redo();
    assert_eq!(state.editor.project(), &edited);
    assert!(state.dirty());
}

#[test]
fn malformed_v2_open_preserves_explicit_graph_channels_and_full_editor_boundary() {
    use crate::view_state::GraphChannel;
    let directory = tempfile::tempdir().unwrap();
    let address =
        serde_json::json!({"id":1,"property":{"kind":"transform","parameter":"PositionX"}});
    for (name, metadata) in [
        (
            "duplicate-v2.lep",
            serde_json::json!({
                "version":2,"compositions":{"1":{"graph_channels":{
                    "version":1,"pinned":[address.clone(),address.clone()],"active":address.clone(),"ranges":[]
                }}},"workspace":{}
            }),
        ),
        (
            "future-address-v2.lep",
            serde_json::json!({
                "version":2,"compositions":{"1":{"graph_channels":{
                    "version":2,"pinned":[address.clone()],"active":address.clone(),"ranges":[]
                }}},"workspace":{}
            }),
        ),
        (
            "unknown-time-remap-v2.lep",
            serde_json::json!({
                "version":2,"compositions":{"1":{"graph_channels":{
                    "version":1,"pinned":[],"active":{"id":1,"property":{"kind":"time_remap","future":true}},"ranges":[]
                }}},"workspace":{}
            }),
        ),
    ] {
        let metadata = serde_json::to_vec(&metadata).unwrap();
        let bytes =
            libre_effects_core::project_file::encode(&Project::default(), Some(&metadata)).unwrap();
        let path = directory.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        let recovery_root = directory.path().join(format!("recovery-{name}"));
        let mut state = dirty_state(&recovery_root);
        let x = GraphChannel {
            id: 1,
            property: Property::PositionX.into(),
        };
        let a = GraphChannel {
            id: 2,
            property: Property::Opacity.into(),
        };
        state.graph_pin_channel(x).unwrap();
        state.graph_pin_channel(a).unwrap();
        state.graph_set_channel_height(x, false, Some([-300., 900.]));
        state.graph_set_channel_height(x, true, Some([-40., 40.]));
        state.graph_set_channel_height(a, false, Some([0., 100.]));
        state.graph_view.speed = true;
        state.graph_open = true;
        state.remember_view();
        let channels = state.graph_channels.clone();
        let graph_view = state.graph_view.clone();
        let graph_key = state.graph_key;
        let graph_property = state.graph_property;
        let selected_keys = state.selected_keys.clone();
        let before = Before::capture(&state, &recovery_root);
        assert!(open_test_project(&mut state, &path).is_err());
        assert_eq!(state.graph_channels, channels);
        assert_eq!(state.graph_view, graph_view);
        assert_eq!(state.graph_key, graph_key);
        assert_eq!(state.graph_property, graph_property);
        assert_eq!(state.selected_keys, selected_keys);
        assert!(state.graph_open);
        before.assert_unchanged(&mut state, &recovery_root);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
