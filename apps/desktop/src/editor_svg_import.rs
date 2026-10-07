//! Bounded, source-preserving SVG import. Chooser and parse results are receipts,
//! never permission to add a layer to whichever document happens to be current.
use super::*;
use crate::svg_import::ImportedSvg;

struct Receipt {
    operation: u64,
    document: u64,
    core_context: u64,
    input: u64,
    transport: u64,
    modal: u64,
    source: Project,
    selected: Option<LayerId>,
    layers: BTreeSet<LayerId>,
    keys: BTreeSet<KeyRef>,
    contents: Option<(CompositionId, LayerId, u64)>,
    graph_key: Option<KeyRef>,
    graph_property: PropertyPath,
    project_item: Option<libre_effects_core::ProjectItem>,
    colors_owned: bool,
    frame: Frame,
    tool: Tool,
}

impl Receipt {
    fn capture(state: &EditorState, operation: u64) -> Self {
        Self {
            operation,
            document: state.document_revision,
            core_context: state.editor.context_generation(),
            input: state.input_context_generation(),
            transport: state.transport_generation(),
            modal: state.colors_clipboard_generation(),
            source: state.editor.project().clone(),
            selected: state.editor.selected(),
            layers: state.selected_layers.clone(),
            keys: state.selected_keys.clone(),
            contents: state.contents_selection,
            graph_key: state.graph_key,
            graph_property: state.graph_property,
            project_item: state.project_item,
            colors_owned: state.colors_key_owned.get(),
            frame: state.frame,
            tool: state.tool,
        }
    }
    fn owns(&self, state: &EditorState) -> bool {
        state.pending_svg_import == Some(self.operation) && state.file_operation == self.operation
    }
    fn current(&self, state: &EditorState, pending_input: bool) -> bool {
        self.owns(state)
            && !pending_input
            && self.document == state.document_revision
            && self.core_context == state.editor.context_generation()
            && self.input == state.input_context_generation()
            && self.transport == state.transport_generation()
            && self.modal == state.colors_clipboard_generation()
            && self.source == *state.editor.project()
            && self.selected == state.editor.selected()
            && self.layers == state.selected_layers
            && self.keys == state.selected_keys
            && self.contents == state.contents_selection
            && self.graph_key == state.graph_key
            && self.graph_property == state.graph_property
            && self.project_item == state.project_item
            && self.colors_owned == state.colors_key_owned.get()
            && self.frame == state.frame
            && self.tool == state.tool
            && !state.playing
            && !state.preview_caching
            && state.svg_import_context_ready()
    }
}

impl EditorState {
    fn svg_import_context_ready(&self) -> bool {
        self.ae_import.is_none()
            && !self.saving
            && !self.collecting
            && !self.importing_video
            && !self.exporting
            && !self.new_composition_requested
            && !self.close_after_save
            && !self.media_open
            && !self.fonts_open
            && !self.queue_open
            && self.recovery.is_none()
            && self.text_session.is_none()
            && self.vertex_editor.is_none()
            && self.expression_editor.is_none()
            && self.gradient_editor.is_none()
            && self.gradient_preview.is_none()
            && self.colors.session.is_none()
    }
    pub(crate) fn svg_import_available(&self) -> bool {
        self.pending_svg_import.is_none() && self.svg_import_context_ready()
    }
    /// Workspace capture runs before transient panel selections or field blur.
    /// Equal-state selection ABA cannot revive an async import after new input.
    pub(crate) fn retire_pending_svg_import(&mut self) {
        if self.pending_svg_import.is_some() {
            self.begin_input_action();
        }
    }
    fn begin_svg_import(&mut self) -> Receipt {
        self.stop();
        let operation = self.begin_file_operation();
        self.pending_svg_import = Some(operation);
        Receipt::capture(self, operation)
    }
    pub(super) fn import_svg(&mut self, cx: &mut Context<Self>) {
        if !self.svg_import_available() {
            return;
        }
        let receipt = self.begin_svg_import();
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                "Import static SVG as editable cubic paths (integer-pixel viewport, up to 1 MiB)"
                    .into(),
            ),
        });
        self.status = "Choose a static SVG with an integer-pixel viewport · Editable paths/gradients may have slight edge or channel-rounding differences".into();
        cx.spawn(async move |entity, cx| {
            let selected = match prompt.await {
                Ok(Ok(paths)) => one_svg_path(paths),
                Ok(Err(error)) => Err(format!("SVG file chooser failed: {error}")),
                Err(error) => Err(format!("SVG file chooser failed: {error}")),
            };
            let path = match selected {
                Ok(Some(path)) => path,
                result => {
                    let _ = entity.update(cx, |state, cx| {
                        state.finish_svg_import(&receipt, result.map(|_| None), false);
                        cx.notify();
                    });
                    return;
                }
            };
            // Check once before expensive work, then again before committing.
            let current = entity.update(cx, |state, cx| {
                let pending = crate::components::TextField::active_has_pending_source_input(cx);
                if receipt.current(state, pending) {
                    state.status = "Reading static SVG…".into();
                    cx.notify();
                    true
                } else {
                    state.finish_svg_import(&receipt, Ok(None), pending);
                    cx.notify();
                    false
                }
            });
            if !matches!(current, Ok(true)) {
                return;
            }
            let result = cx
                .background_executor()
                .spawn(async move {
                    let imported = crate::svg_import::read_svg_file(&path)?;
                    Ok(Some((imported, svg_layer_name(&path))))
                })
                .await;
            let _ = entity.update(cx, |state, cx| {
                let pending = crate::components::TextField::active_has_pending_source_input(cx);
                state.finish_svg_import(&receipt, result, pending);
                cx.notify();
            });
        })
        .detach();
    }
    fn finish_svg_import(
        &mut self,
        receipt: &Receipt,
        result: Result<Option<(ImportedSvg, String)>, String>,
        pending_input: bool,
    ) {
        // Superseded completions cannot clear a newer chooser or replace status.
        if !receipt.owns(self) {
            return;
        }
        let current = receipt.current(self, pending_input);
        self.pending_svg_import = None;
        if !current {
            self.status = "SVG import canceled: editor context changed; import again".into();
            return;
        }
        let result = result.and_then(|selected| {
            let Some((imported, name)) = selected else {
                return Ok(false);
            };
            self.editor.execute(Command::ImportSvg {
                contents: imported.contents,
                width: imported.width,
                height: imported.height,
                name,
            })?;
            // Only a successful atomic core transaction may change selection,
            // transient ownership, transport, views or the dirty document.
            self.begin_input_action();
            self.retire_colors_clipboard();
            self.colors_key_owned.set(false);
            self.stop();
            self.selected_layers = self.editor.selected().into_iter().collect();
            self.selected_keys.clear();
            self.contents_selection = None;
            self.gradient_controls = None;
            self.graph_key = None;
            self.composition_started = true;
            self.preview_revision = self.preview_revision.wrapping_add(1);
            self.normalize();
            Ok(true)
        });
        self.status = match result {
            Ok(true) => {
                "Imported SVG as editable cubic paths · Edges/gradient rounding may differ slightly · One Undo"
                    .into()
            }
            Ok(false) => "SVG import canceled".into(),
            Err(error) => format!("SVG import failed; no shapes added: {error}"),
        };
    }
}

fn one_svg_path(paths: Option<Vec<PathBuf>>) -> Result<Option<PathBuf>, String> {
    let mut paths = paths.unwrap_or_default();
    if paths.len() > 1 {
        return Err("Choose exactly one SVG file".into());
    }
    Ok(paths.pop())
}

fn svg_layer_name(path: &Path) -> String {
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    let name: String = name
        .chars()
        .filter(|character| !character.is_control())
        .take(128)
        .collect();
    if name.trim().is_empty() {
        "Imported SVG".into()
    } else {
        name
    }
}

#[cfg(test)]
#[path = "editor_svg_import_tests.rs"]
mod tests;
