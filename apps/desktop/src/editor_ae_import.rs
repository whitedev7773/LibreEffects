//! Detached, bounded AE interchange import. Reading and conversion are receipts,
//! not permission to replace whichever document happens to be open later.
use super::*;
use libre_effects_editor_model::ae_import::{ImportDocument, RootSummary};
use libre_effects_editor_model::ae_import_ui::Receipt;
use std::{io::Read, sync::Arc};

const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Choosing,
    Reading,
    Ready,
    Converting,
    AwaitingConfirmation,
    Failed,
}
pub(crate) struct Session {
    pub operation: u64,
    pub name: String,
    pub roots: Vec<RootSummary>,
    pub selection: Option<usize>,
    pub stage: Stage,
    pub error: Option<String>,
    document: Option<Arc<ImportDocument>>,
    path: Option<PathBuf>,
    source: Project,
    receipt: Receipt,
    prepared: Option<Project>,
    apply_requested: bool,
}
impl Session {
    pub(crate) fn can_apply(&self) -> bool {
        self.stage == Stage::Ready
            && self
                .selection
                .and_then(|index| self.roots.get(index))
                .is_some_and(|root| root.blocker.is_none())
            && self.document.is_some()
            && self.error.is_none()
    }
    fn current(&self, state: &EditorState) -> bool {
        self.receipt.current(
            state.file_operation,
            state.document_revision,
            state.editor.context_generation(),
        ) && self.source == *state.editor.project()
            && state.text_session.is_none()
            && state.vertex_editor.is_none()
            && state.expression_editor.is_none()
            && state.gradient_editor.is_none()
            && state.colors.session.is_none()
            && !state.playing
            && !state.preview_caching
            && state.recovery.is_none()
            && !state.media_open
            && !state.fonts_open
            && !state.queue_open
            && !state.exporting
            && !state.collecting
    }
    fn fail(&mut self, message: String) {
        self.error = Some(display_message(&message));
        self.stage = Stage::Failed;
        self.prepared = None;
        self.document = None;
        self.apply_requested = false;
    }
}

impl EditorState {
    pub(crate) fn ae_import_available(&self) -> bool {
        self.ae_import.is_none()
            && self.automation.is_none()
            && self.svg_import_available()
            && !self.playing
            && !self.preview_caching
    }
    pub(crate) fn cancel_ae_import(&mut self) {
        if self.ae_import.take().is_some() {
            self.status = "AE import canceled; project unchanged".into();
        }
    }
    pub(crate) fn cancel_ae_import_confirmation(&mut self) {
        if let Some(session) = self.ae_import.as_mut()
            && session.stage == Stage::AwaitingConfirmation
        {
            session.stage = Stage::Ready;
            session.prepared = None;
            session.apply_requested = false;
        }
    }
    pub(crate) fn take_ae_import_apply_request(&mut self) -> Option<u64> {
        let session = self.ae_import.as_mut()?;
        if session.apply_requested {
            session.apply_requested = false;
            Some(session.operation)
        } else {
            None
        }
    }
    pub(crate) fn ae_import_allows_action(&self, action: &Action) -> bool {
        self.ae_import.as_ref().is_none_or(|session| {
            session.stage == Stage::AwaitingConfirmation
                && matches!(
                    action,
                    Action::ApplyAeProject(_) | Action::Save | Action::SaveAs
                )
        })
    }
    /// Called only by the existing Save path, never by a generic file operation.
    pub(super) fn advance_ae_import_save(&mut self, previous: u64, operation: u64) {
        if let Some(session) = self.ae_import.as_mut()
            && session.stage == Stage::AwaitingConfirmation
        {
            session.receipt.advance_save(previous, operation);
        }
    }
    pub(super) fn import_ae_project(&mut self, cx: &mut Context<Self>) {
        if !self.ae_import_available() {
            return;
        }
        self.stop();
        let operation = self.begin_file_operation();
        self.ae_import = Some(Session {
            operation,
            name: "Choose a .json or .aep file".into(),
            roots: vec![],
            selection: None,
            stage: Stage::Choosing,
            error: None,
            document: None,
            path: None,
            source: self.editor.project().clone(),
            receipt: Receipt::capture(
                operation,
                self.document_revision,
                self.editor.context_generation(),
            ),
            prepared: None,
            apply_requested: false,
        });
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import AE project data (.json / .aep, up to 16 MiB)".into()),
        });
        self.status =
            "Choose typed AE project data; the current project is unchanged until Apply".into();
        cx.spawn(async move |entity, cx| {
            let selected = match prompt.await {
                Ok(Ok(paths)) => one_path(paths),
                Ok(Err(error)) => Err(format!("File chooser failed: {error}")),
                Err(error) => Err(format!("File chooser failed: {error}")),
            };
            let path = match selected {
                Ok(Some(path)) => path,
                result => {
                    let _ = entity.update(cx, |state, cx| {
                        if state.owns_ae_import(operation) {
                            match result {
                                Ok(None) => state.cancel_ae_import(),
                                Err(error) => state.fail_ae_import(operation, error),
                                _ => unreachable!(),
                            }
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            let current = entity.update(cx, |state, cx| {
                if !state.check_ae_import(operation, cx) {
                    return false;
                }
                let session = state.ae_import.as_mut().unwrap();
                session.stage = Stage::Reading;
                session.name = file_label(&path);
                state.status = "Reading bounded AE project data…".into();
                cx.notify();
                true
            });
            if !matches!(current, Ok(true)) {
                return;
            }
            let result = cx
                .background_executor()
                .spawn(async move {
                    let path = crate::media_io::clean_absolute(&path)?;
                    let document = read_file(&path)?;
                    let mut roots = document.roots_with_preflight(native_text_preflight);
                    for root in &mut roots {
                        root.name = display_message(&root.name);
                        root.blocker = root.blocker.as_deref().map(display_message);
                    }
                    if roots.is_empty() {
                        return Err("No selectable root compositions were provided".into());
                    }
                    Ok((path, Arc::new(document), roots))
                })
                .await;
            let _ = entity.update(cx, |state, cx| {
                if !state.check_ae_import(operation, cx) {
                    return;
                }
                match result {
                    Ok((path, document, roots)) => {
                        let session = state.ae_import.as_mut().unwrap();
                        session.path = Some(path);
                        session.document = Some(document);
                        session.selection = roots
                            .iter()
                            .position(|root| root.blocker.is_none())
                            .or(Some(0));
                        session.roots = roots;
                        session.stage = Stage::Ready;
                        state.status = "Select a supported root composition, then Apply".into();
                    }
                    Err(error) => state.fail_ae_import(operation, error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn owns_ae_import(&self, operation: u64) -> bool {
        self.ae_import
            .as_ref()
            .is_some_and(|session| session.operation == operation)
    }
    fn check_ae_import(&mut self, operation: u64, cx: &mut Context<Self>) -> bool {
        if !self.owns_ae_import(operation) {
            return false;
        }
        if !self.ae_import.as_ref().unwrap().current(self)
            || crate::components::TextField::active_has_pending_source_input(cx)
        {
            self.fail_ae_import(
                operation,
                "Editor context changed. Cancel and import again; the project was not replaced."
                    .into(),
            );
            cx.notify();
            return false;
        }
        true
    }
    fn fail_ae_import(&mut self, operation: u64, error: String) {
        if let Some(session) = self
            .ae_import
            .as_mut()
            .filter(|session| session.operation == operation)
        {
            self.status = format!(
                "AE import failed; project unchanged: {}",
                display_message(&error)
            );
            session.fail(error);
        }
    }
    pub(crate) fn select_ae_import_root(&mut self, operation: u64, index: usize) {
        if let Some(session) = self
            .ae_import
            .as_mut()
            .filter(|session| session.operation == operation)
            && session.stage == Stage::Ready
            && index < session.roots.len()
        {
            session.selection = Some(index);
        }
    }
    pub(crate) fn request_ae_import_apply(&mut self, operation: u64, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        if !self.check_ae_import(operation, cx) {
            return;
        }
        let session = self.ae_import.as_mut().unwrap();
        if !session.can_apply() {
            return;
        }
        let root = session.roots[session.selection.unwrap()].id;
        let document = session.document.as_ref().unwrap().clone();
        session.stage = Stage::Converting;
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let project = document.convert(root)?;
                    native_text_preflight(&project)?;
                    Ok(project)
                })
                .await;
            let _ = entity.update(cx, |state, cx| {
                if !state.check_ae_import(operation, cx) {
                    return;
                }
                match result {
                    Ok(project) => {
                        let session = state.ae_import.as_mut().unwrap();
                        session.prepared = Some(project);
                        session.stage = Stage::AwaitingConfirmation;
                        session.apply_requested = true;
                    }
                    Err(error) => state.fail_ae_import(operation, error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    /// The shell routes this only through the existing unsaved-changes contract.
    pub(super) fn apply_ae_project(&mut self, operation: u64, cx: &mut Context<Self>) {
        if self.saving || !self.check_ae_import(operation, cx) {
            return;
        }
        let session = self.ae_import.as_mut().unwrap();
        if session.stage != Stage::AwaitingConfirmation {
            return;
        }
        let Some(project) = session.prepared.take() else {
            return;
        };
        let Some(path) = session.path.clone() else {
            return;
        };
        match self.install_ae_imported_project(project, path) {
            Ok(()) => {
                self.ae_import = None;
            }
            Err(error) => self.fail_ae_import(operation, error),
        }
        cx.notify();
    }
}

/// Only authored text and static run styles are shaped here. Source expressions
/// are never evaluated to decide whether an import may replace the document.
fn native_text_preflight(project: &Project) -> Result<(), String> {
    for (composition_id, composition) in project.compositions() {
        for layer in composition.layers() {
            let (Content::Text { text, .. }, Some(rich)) = (layer.content(), layer.rich_text())
            else {
                continue;
            };
            crate::rich_text_render::compose(text, rich, layer.width(), &layer.text_style())
                .map_err(|error| {
                    format!(
                        "Composition {composition_id}, layer {}: {error}",
                        layer.id()
                    )
                })?;
        }
    }
    Ok(())
}

fn one_path(paths: Option<Vec<PathBuf>>) -> Result<Option<PathBuf>, String> {
    let mut paths = paths.unwrap_or_default();
    if paths.len() > 1 {
        return Err("Choose exactly one .json or .aep file".into());
    }
    Ok(paths.pop())
}
fn file_label(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .filter(|character| !character.is_control())
        .take(160)
        .collect()
}
fn display_message(message: &str) -> String {
    let mut text: String = message
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(4096)
        .collect();
    if message.chars().count() > 4096 {
        text.push_str("…");
    }
    text
}
fn read_file(path: &Path) -> Result<ImportDocument, String> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("aep")
        })
    {
        return Err("Choose a typed interchange .json or an .aep file".into());
    }
    let before = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect file: {error}"))?;
    if !before.file_type().is_file() {
        return Err("AE import requires a regular local file (no symlinks)".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000).share_mode(0x00000001);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("Could not open file: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect file: {error}"))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x00000400 != 0 {
            return Err("AE import does not follow reparse points".into());
        }
    }
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("The selected path is not a regular file".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("AE import files must be at most 16 MiB".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read file: {error}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("AE import files must be at most 16 MiB".into());
    }
    libre_effects_editor_model::ae_import::read_document(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn begin(state: &mut EditorState) -> u64 {
        let operation = state.begin_file_operation();
        state.ae_import = Some(Session {
            operation,
            name: "Synthetic fixture".into(),
            roots: vec![],
            selection: None,
            stage: Stage::Ready,
            error: None,
            document: None,
            path: None,
            source: state.editor.project().clone(),
            receipt: Receipt::capture(
                operation,
                state.document_revision,
                state.editor.context_generation(),
            ),
            prepared: None,
            apply_requested: false,
        });
        operation
    }
    #[test]
    fn cancel_and_failure_preserve_source_history_and_provenance() {
        for cancel in [true, false] {
            let mut state = EditorState::default();
            state.editor.execute(Command::AddRectangle).unwrap();
            state.path = Some(PathBuf::from("source.lep"));
            let source = state.editor.project().clone();
            let saved = state.saved.clone();
            let recent = state.recent_projects.paths().to_vec();
            let operation = begin(&mut state);
            if cancel {
                state.cancel_ae_import();
            } else {
                state.fail_ae_import(operation, "unsupported payload".into());
            }
            assert_eq!(state.editor.project(), &source);
            assert_eq!(state.saved, saved);
            assert_eq!(state.path, Some(PathBuf::from("source.lep")));
            assert_eq!(state.recent_projects.paths(), recent.as_slice());
            assert!(state.editor.can_undo());
        }
    }
    #[test]
    fn only_save_and_continue_advances_the_live_receipt() {
        let mut state = EditorState::default();
        let operation = begin(&mut state);
        assert!(state.ae_import.as_ref().unwrap().current(&state));
        let next = state.begin_file_operation();
        state.advance_ae_import_save(operation, next);
        assert!(!state.ae_import.as_ref().unwrap().current(&state));
        state.cancel_ae_import();
        let operation = begin(&mut state);
        state.ae_import.as_mut().unwrap().stage = Stage::AwaitingConfirmation;
        let next = state.begin_file_operation();
        state.advance_ae_import_save(operation, next);
        assert!(state.ae_import.as_ref().unwrap().current(&state));
        state.editor.execute(Command::AddRectangle).unwrap();
        state.editor.undo();
        assert!(!state.ae_import.as_ref().unwrap().current(&state));
    }
    #[test]
    fn canceled_confirmation_never_retains_a_prepared_apply() {
        let mut state = EditorState::default();
        begin(&mut state);
        let session = state.ae_import.as_mut().unwrap();
        session.stage = Stage::AwaitingConfirmation;
        session.prepared = Some(Project::default());
        session.apply_requested = true;
        state.cancel_ae_import_confirmation();
        assert!(state.take_ae_import_apply_request().is_none());
        assert!(state.ae_import.as_ref().unwrap().prepared.is_none());
        assert!(state.ae_import.as_ref().unwrap().stage == Stage::Ready);
    }
    #[test]
    fn reader_rejects_oversized_nonregular_and_unsupported_files() {
        let directory = tempfile::tempdir().unwrap();
        let huge = directory.path().join("huge.json");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(read_file(&huge).err().unwrap().contains("16 MiB"));
        let invalid = directory.path().join("invalid.aep");
        std::fs::write(&invalid, b"RIFX\0\0\0\x04Egg!").unwrap();
        assert!(read_file(&invalid).is_err());
        assert!(one_path(Some(vec![huge.clone(), invalid.clone()])).is_err());
        assert!(one_path(None).unwrap().is_none());
        #[cfg(unix)]
        {
            let link = directory.path().join("link.aep");
            std::os::unix::fs::symlink(&invalid, &link).unwrap();
            assert!(read_file(&link).is_err());
            use std::os::unix::ffi::OsStrExt;
            let fifo = directory.path().join("fifo.json");
            let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
            // SAFETY: a valid NUL-terminated path in this isolated temporary directory.
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            assert!(read_file(&fifo).is_err());
        }
    }
    #[test]
    fn unavailable_rich_font_is_blocked_before_replacing_source() {
        use libre_effects_core::{RichText, TextCharacterStyle, TextStyle, TextStyleRun};
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        let source = state.editor.project().clone();
        let mut imported = Editor::default();
        imported
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "A".into(),
                    font_size: 36.0,
                },
                width: 160.0,
                height: 80.0,
                name: "Independent missing-font preflight".into(),
            })
            .unwrap();
        let mut style = TextCharacterStyle::from_style(&TextStyle::default(), 36.0, 0xffffff);
        style.font_family = "LibreEffects AE Import Deliberately Missing Font".into();
        style.font_face.clear();
        let rich = RichText::new(
            "A",
            style.clone(),
            vec![TextStyleRun {
                start: 0,
                end: 1,
                style,
            }],
        )
        .unwrap();
        imported
            .execute(Command::SetRichText {
                id: imported.selected().unwrap(),
                rich_text: Some(rich),
            })
            .unwrap();
        let error = native_text_preflight(imported.project()).unwrap_err();
        assert!(error.contains("font unavailable"), "{error}");
        assert_eq!(state.editor.project(), &source);
        assert!(state.editor.can_undo());
    }
}
