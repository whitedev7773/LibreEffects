use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Instant,
};

use gpui::{Context, ElementId, Entity, PathPromptOptions, SharedString, Window};
use libre_effects_core::{
    Command, CompositionId, Content, Editor, Frame, KeyCopy, KeyRef, LayerId, Project, Property,
    PropertyPath,
};

use crate::components::{Button, ButtonSize, ButtonVariant};
#[path = "editor_assets.rs"]
pub(crate) mod assets;
#[path = "editor_footage.rs"]
mod footage;
#[path = "editor_io.rs"]
mod io;
#[path = "editor_layer_transform.rs"]
pub(crate) mod layer_transform;
#[path = "editor_media.rs"]
mod media;
#[path = "editor_playback.rs"]
mod playback;
#[path = "editor_presets.rs"]
pub(crate) mod presets;
#[path = "editor_queue.rs"]
pub(crate) mod queue;
#[path = "editor_video.rs"]
mod video;
#[path = "editor_view.rs"]
mod view;
#[derive(Clone)]
pub(crate) struct VideoJob {
    pub label: String,
    pub progress: u32,
    pub total: u32,
    pub message: String,
}

#[derive(Clone)]
pub(crate) enum Action {
    BeginText(Option<LayerId>, [f64; 2]),
    BeginParagraph([f64; 4]),
    CommitText,
    CancelText,
    Preset(presets::PresetAction),
    OpenGradient(u64),
    ApplyGradient,
    CancelGradient,
    OpenVertex(crate::panels::vertex_editor::Request),
    ApplyVertex,
    CancelVertex,
    OpenColor(crate::color_edit::Target),
    ApplyColor,
    CancelColor,
    PickColor,
    SampleColor([u8; 4]),
    Queue(queue::QueueAction),
    AddMarker(libre_effects_core::MarkerTarget),
    ShowMarker(
        libre_effects_core::MarkerTarget,
        libre_effects_core::MarkerId,
    ),
    NavigateMarker(bool),
    Edit(Command),
    ActivateComposition(CompositionId),
    AddComposition(CompositionId),
    PrecomposeSelection,
    Select(LayerId),
    Seek(Frame),
    Step(i32),
    Play,
    PreviewAudio,
    PreviewScrub,
    PreviewLoop,
    CacheWorkArea,
    CycleCacheBudget,
    PurgePreviewCache,
    Undo,
    Redo,
    New,
    Open,
    SaveAs,
    Save,
    CollectFiles,
    CancelCollection,
    ManageMedia,
    ManageFonts,
    RefreshMedia,
    RelinkSource(String),
    RelinkMissing,
    ImportImage,
    ImportImageSequence,
    RelinkSequence(u64),
    ImportVideo,
    RelinkVideo,
    RefreshFootage,
    AddText,
    CompositionFromFootage,
    ExportFrame,
    ExportFrameBackground,
    ExportSequence,
    ExportSequenceBackground,
    ExportVideo(crate::video_export::VideoPreset),
    DismissRender,
    CancelExport,
    CopyKeys,
    PasteKeys,
    CopySelection,
    CutSelection,
    PasteSelection,
    CopyLayers,
    PasteLayers,
    ToggleSelectedSwitch(libre_effects_core::LayerSwitch),
    DeleteSelection,
    DuplicateSelection,
    SplitSelection,
    TrimSelection(bool),
    NudgeSelection(f64, f64),
    /// Resolve selected layer IDs and playhead only after dispatch preparation.
    TransformLayers(libre_effects_core::LayerTransformOp),
    SelectMany(LayerId, bool, bool),
    ZoomTimeline(f32),
    PanTimeline(i32),
    ZoomPreview(f32),
    FitPreview,
    CyclePreviewResolution,
    Checkerboard,
    ViewerOption(crate::viewer_tools::ViewOption),
    PreviewChannel(crate::viewer_tools::Channel),
    ClearGuides,
    SetTool(Tool),
    WorkStart,
    WorkEnd,
    PreviousKey,
    NextKey,
    ToggleExpanded,
    Filter(Option<PropertyFilter>),
    ToggleGraph,
    ToggleTimeRemap,
    FreezeTimeRemap,
    GraphProperty(LayerId, PropertyPath),
}

impl Action {
    /// A vertex draft owns every editor command until it is accepted or canceled.
    /// In particular, Undo/Redo must never reach the source behind the modal.
    fn allowed_in_vertex_editor(&self) -> bool {
        matches!(self, Self::ApplyVertex | Self::CancelVertex)
    }
    fn allowed_in_gradient_editor(&self) -> bool {
        matches!(self, Self::ApplyGradient | Self::CancelGradient)
    }
    fn allowed_in_color_editor(&self) -> bool {
        matches!(
            self,
            Self::ApplyColor | Self::CancelColor | Self::PickColor | Self::SampleColor(_)
        )
    }
    fn commits_text_before_dispatch(&self) -> bool {
        !matches!(self, Self::CancelText | Self::CommitText)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    Text,
    Select,
    Hand,
    Zoom,
    Shape(libre_effects_core::ShapeKind),
    Pen,
    Rotate,
    Anchor,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyFilter {
    Position,
    Anchor,
    Scale,
    Rotation,
    Opacity,
    Animated,
}
impl PropertyFilter {
    pub fn includes(self, property: libre_effects_core::Property) -> bool {
        use libre_effects_core::Property::*;
        match self {
            Self::Position => matches!(property, PositionX | PositionY),
            Self::Anchor => matches!(property, AnchorX | AnchorY),
            Self::Scale => matches!(property, ScaleX | ScaleY),
            Self::Rotation => property == Rotation,
            Self::Opacity => property == Opacity,
            Self::Animated => true,
        }
    }
}

pub(crate) struct EditorState {
    pub fonts_open: bool,
    pub text_session: Option<crate::text_edit::Session>,
    pub presets: crate::effect_presets::Library,
    pub colors: crate::color_edit::Workflow,
    pub queue: Option<std::sync::Arc<std::sync::Mutex<crate::render_queue::Queue>>>,
    pub queue_open: bool,
    pub queue_busy: bool,
    pub queue_message: String,
    pub queue_formats: Vec<crate::output_settings::Spec>,
    pub project_item: Option<libre_effects_core::ProjectItem>,
    composition_views:
        std::collections::BTreeMap<CompositionId, crate::view_state::CompositionView>,
    pub workspace: crate::view_state::WorkspaceView,
    pub preview_pan: [f32; 2],
    pub snapping: bool,
    pub marker_selection: Option<(
        CompositionId,
        libre_effects_core::MarkerTarget,
        libre_effects_core::MarkerId,
    )>,
    pub selected_layers: BTreeSet<LayerId>,
    pub selected_keys: BTreeSet<KeyRef>,
    clipboard: Vec<KeyCopy>,
    layer_clipboard: Option<libre_effects_core::LayerClipboard>,
    pub path: Option<PathBuf>,
    source_format: Option<crate::project_io::ProjectFormat>,
    // Retain imported-file protection after saving a native copy.
    imported_original: Option<PathBuf>,
    file_operation: u64,
    pub composition_started: bool,
    pub new_composition_requested: bool,
    saved: Project,
    pub saving: bool,
    pub collecting: bool,
    pub media_open: bool,
    pub scanning_media: bool,
    pub media_entries: Vec<crate::media_io::MediaEntry>,
    pub media_message: String,
    collection_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub close_after_save: bool,
    pub recovery: Option<crate::recovery::Candidate>,
    pub recovery_pending: std::collections::VecDeque<crate::recovery::Candidate>,
    recovery_session: Option<std::sync::Arc<std::sync::Mutex<crate::recovery::Session>>>,
    recovery_ready: bool,
    pub exporting: bool,
    pub video_job: Option<VideoJob>,
    export_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub editor: Editor,
    pub frame: Frame,
    pub playing: bool,
    pub preview_audio: bool,
    pub preview_scrub: bool,
    pub preview_loop: bool,
    pub preview_caching: bool,
    pub preview_cache_limit: usize,
    pub preview_cache: crate::preview_cache::Summary,
    pub audio_status: crate::audio_playback::Status,
    audio_session: Option<crate::audio_playback::Session>,
    pub status: String,
    pub timeline_zoom: f32,
    pub timeline_start: Frame,
    pub preview_zoom: Option<f32>,
    pub preview_resolution: u32,
    pub preview_revision: u64,
    pub document_revision: u64,
    pub importing_video: bool,
    pub checkerboard: bool,
    pub viewer: crate::viewer_tools::ViewerOptions,
    pub pixel_info: Option<(u64, CompositionId, crate::viewer_tools::PixelInfo)>,
    pub tool: Tool,
    pub work_start: Frame,
    pub work_end: Frame,
    pub expanded: bool,
    pub property_filter: Option<PropertyFilter>,
    pub graph_open: bool,
    pub graph_view: crate::view_state::GraphView,
    pub effect_controls_open: bool,
    pub gradient_controls: Option<crate::color_edit::GradientTarget>,
    pub contents_selection: Option<(CompositionId, LayerId, u64)>,
    pub gradient_preview: Option<crate::color_edit::GradientDraft>,
    pub gradient_editor: Option<crate::panels::gradient_editor::Session>,
    pub vertex_editor: Option<crate::panels::vertex_editor::Session>,
    pub vertex_return: Option<crate::panels::vertex_editor::Request>,
    pub graph_property: PropertyPath,
    pub graph_key: Option<KeyRef>,
    pub graph_channels: crate::view_state::GraphChannels,
    playback_origin: Option<(Instant, Frame)>,
    playback_generation: u64,
}

impl Default for EditorState {
    fn default() -> Self {
        let mut colors = crate::color_edit::Workflow::default();
        if let Some(path) = crate::color_edit::Workflow::path() {
            colors.load(&path);
        }
        Self {
            text_session: None,
            presets: Default::default(),
            fonts_open: false,
            colors,
            queue: None,
            queue_open: false,
            queue_busy: false,
            queue_message: "Loading render queue…".into(),
            queue_formats: vec![crate::render_queue::Format::Mp4.into()],
            project_item: None,
            composition_views: Default::default(),
            workspace: Default::default(),
            preview_pan: [0.0; 2],
            snapping: true,
            marker_selection: None,
            selected_layers: BTreeSet::new(),
            selected_keys: BTreeSet::new(),
            clipboard: Vec::new(),
            layer_clipboard: None,
            path: None,
            source_format: None,
            imported_original: None,
            file_operation: 0,
            composition_started: false,
            new_composition_requested: false,
            saved: Project::default(),
            saving: false,
            collecting: false,
            media_open: false,
            scanning_media: false,
            media_entries: Vec::new(),
            media_message: String::new(),
            collection_cancel: Default::default(),
            close_after_save: false,
            recovery: None,
            recovery_ready: false,
            recovery_pending: Default::default(),
            recovery_session: None,
            exporting: false,
            video_job: None,
            export_cancel: Default::default(),
            editor: Editor::default(),
            frame: 0,
            playing: false,
            preview_audio: true,
            preview_scrub: false,
            preview_loop: true,
            preview_caching: false,
            preview_cache_limit: 256 * crate::preview_cache::MIB,
            preview_cache: Default::default(),
            audio_status: crate::audio_playback::Status {
                phase: crate::audio_playback::Phase::Ended,
                ..Default::default()
            },
            audio_session: None,
            status: "Create a composition or import footage to begin.".into(),
            timeline_zoom: 1.0,
            timeline_start: 0,
            preview_zoom: None,
            preview_resolution: 1,
            preview_revision: 0,
            document_revision: 0,
            importing_video: false,
            checkerboard: false,
            viewer: Default::default(),
            pixel_info: None,
            tool: Tool::Select,
            work_start: 0,
            work_end: 150,
            expanded: true,
            property_filter: None,
            graph_open: false,
            graph_view: Default::default(),
            effect_controls_open: false,
            gradient_controls: None,
            contents_selection: None,
            gradient_preview: None,
            gradient_editor: None,
            vertex_editor: None,
            vertex_return: None,
            graph_property: Property::PositionX.into(),
            graph_key: None,
            graph_channels: Default::default(),
            playback_origin: None,
            playback_generation: 0,
        }
    }
}

impl EditorState {
    pub(crate) fn finish_text(&mut self, commit: bool, cx: &mut Context<Self>) {
        let Some(session) = self.text_session.take() else {
            return;
        };
        if commit && session.changed() {
            if !session.valid(self.editor.project(), self.document_revision, self.frame) {
                self.status = "Text edit canceled because its document or time changed".into();
            } else {
                self.status = match self.editor.execute(session.command()) {
                    Ok(()) => {
                        self.editor.select(session.id);
                        self.selected_layers = [session.id].into();
                        self.composition_started = true;
                        "Text edited".into()
                    }
                    Err(e) => e,
                };
            }
        } else {
            self.status = "Text edit finished".into();
        }
        self.normalize();
        cx.notify();
    }
    pub(crate) fn text_project(&self) -> Project {
        self.text_session
            .as_ref()
            .filter(|s| s.valid(self.editor.project(), self.document_revision, self.frame))
            .and_then(|s| s.project().ok())
            .unwrap_or_else(|| self.editor.project().clone())
    }
    pub fn welcome(&self) -> bool {
        self.text_session.is_none()
            && self.tool != Tool::Text
            && !self.composition_started
            && self.path.is_none()
            && !self.dirty()
            && self.editor.project().composition().layers().is_empty()
            && self.editor.project().compositions().len() == 1
    }

    pub(crate) fn transport_generation(&self) -> u64 {
        self.playback_generation
    }
    pub fn selected_marker(
        &self,
    ) -> Option<(
        libre_effects_core::MarkerTarget,
        &libre_effects_core::Marker,
    )> {
        let (composition, target, id) = self.marker_selection?;
        if composition != self.editor.project().active_composition_id() {
            return None;
        }
        Some((
            target,
            self.editor
                .project()
                .composition()
                .marker_track(target)?
                .iter()
                .find(|m| m.id() == id)?,
        ))
    }
    /// Graph paste planning reads key data without falling back to layer paste.
    pub(crate) fn graph_clipboard(&self) -> &[KeyCopy] {
        &self.clipboard
    }

    fn clear_clipboard(&mut self) {
        self.media_open = false;
        self.media_entries.clear();
        self.media_message.clear();
        self.marker_selection = None;
        self.clipboard.clear();
        self.layer_clipboard = None;
    }
    fn copy_layers(&mut self) {
        match self
            .editor
            .copy_layers(&self.selected_layers.iter().copied().collect::<Vec<_>>())
        {
            Ok(clipboard) => {
                self.status = format!("Copied {} layers", clipboard.len());
                self.clipboard.clear();
                self.layer_clipboard = Some(clipboard);
            }
            Err(error) => self.status = error,
        }
    }
    pub fn visible_frames(&self) -> Frame {
        ((self.editor.project().composition().duration() as f32 / self.timeline_zoom).ceil()
            as Frame)
            .max(2)
    }
    fn normalize(&mut self) {
        if self.selected_marker().is_none() {
            self.marker_selection = None;
        }
        let comp = self.editor.project().composition();
        self.selected_layers.retain(|id| comp.layer(*id).is_some());
        self.selected_keys.retain(|k| {
            comp.layer(k.id).is_some_and(|l| {
                l.track(k.property)
                    .is_some_and(|t| t.keys().contains_key(&k.frame))
            })
        });
        if self.selected_layers.is_empty()
            && let Some(id) = self.editor.selected()
        {
            self.selected_layers.insert(id);
        }
        self.normalize_graph_channels(false);
        if !self.graph_channels.explicit
            && self
                .editor
                .selected_layer()
                .is_some_and(|l| l.track(self.graph_property).is_none())
        {
            self.graph_property = Property::PositionX.into();
            self.graph_key = None;
        }
        if self.graph_key.is_some_and(|key| {
            self.graph_active_channel() != Some(key.into())
                || !self
                    .editor
                    .project()
                    .composition()
                    .layer(key.id)
                    .is_some_and(|l| {
                        l.track(key.property)
                            .is_some_and(|t| t.keys().contains_key(&key.frame))
                    })
        }) {
            self.graph_key = None;
        }
        let duration = self.editor.project().composition().duration();
        self.frame = self.frame.min(duration - 1);
        let work_area = self.editor.project().composition().work_area();
        self.work_start = work_area.start;
        self.work_end = work_area.end;
        self.timeline_start = self
            .timeline_start
            .min(duration.saturating_sub(self.visible_frames()));
    }
    fn stop(&mut self) {
        self.preview_caching = false;
        self.audio_session = None;
        self.audio_status.phase = crate::audio_playback::Phase::Ended;
        self.audio_status.levels = Default::default();
        self.playing = false;
        self.playback_origin = None;
        self.playback_generation = self.playback_generation.wrapping_add(1);
    }

    fn step_history(&mut self, redo: bool) {
        self.stop();
        self.remember_view();
        let previous = self.editor.project().active_composition_id();
        if redo {
            self.editor.redo();
        } else {
            self.editor.undo();
        }
        // Reconcile cached views before restoring a composition, so Undo may
        // revive its unavailable pins without restoring a key selection or focus.
        self.normalize_graph_channels(true);
        if previous != self.editor.project().active_composition_id() {
            self.composition_changed();
        }
        // Core history restores its primary selection. Keep every panel in sync
        // instead of retaining a now-inactive head after redoing a layer split.
        self.selected_layers = self.editor.selected().into_iter().collect();
        self.selected_keys.clear();
        self.graph_key = None;
        self.normalize();
        self.status = "History updated".into();
    }

    /// Interruptions must discard the draft before any field blur can submit.
    /// Only ordinary Apply/Cancel may issue a validated one-shot Pen return.
    pub(crate) fn discard_vertex_editor(&mut self) {
        self.vertex_editor = None;
        self.vertex_return = None;
    }

    fn composition_changed(&mut self) {
        self.discard_vertex_editor();
        self.gradient_controls = None;
        self.contents_selection = None;
        self.gradient_preview = None;
        self.gradient_editor = None;
        self.stop();
        self.restore_composition_view();
        self.selected_layers.clear();
        self.selected_keys.clear();
        self.graph_key = None;
        self.document_revision = self.document_revision.wrapping_add(1);
        self.preview_revision = self.preview_revision.wrapping_add(1);
    }

    fn apply_edit(&mut self, command: &Command) {
        self.stop();
        self.remember_view();
        let before = self.editor.selected();
        let composition = self.editor.project().active_composition_id();
        self.status = match self.editor.execute(command.clone()) {
            Ok(()) => "Edited".into(),
            Err(error) => error,
        };
        if self.status == "Edited"
            && matches!(
                command,
                Command::Effect {
                    edit: libre_effects_core::EffectEdit::Add(_),
                    ..
                }
            )
        {
            self.effect_controls_open = true;
        }
        if before != self.editor.selected() {
            self.selected_layers.clear();
        }
        if composition != self.editor.project().active_composition_id() {
            self.composition_changed();
        }
    }

    pub fn dispatch(&mut self, action: &Action, window: &mut Window, cx: &mut Context<Self>) {
        let vertex_was_open = self.vertex_editor.is_some();
        let vertex_invalidated = self.invalidate_vertex_editor();
        if vertex_was_open && !action.allowed_in_vertex_editor() {
            // A stale draft is discarded, but the event aimed at that modal
            // must not suddenly become a source Undo/file/edit command.
            if vertex_invalidated {
                cx.notify();
            }
            return;
        }
        if !action.allowed_in_vertex_editor() {
            // A new editor action supersedes a still-unconsumed modal return.
            self.vertex_return = None;
        }
        self.invalidate_gradient_editor();
        if self.gradient_editor.is_some() && !action.allowed_in_gradient_editor() {
            return;
        }
        if action.commits_text_before_dispatch() {
            self.finish_text(true, cx);
        }
        if self.colors.session.is_some() && !action.allowed_in_color_editor() {
            return;
        }

        if !matches!(
            action,
            Action::Select(_)
                | Action::SelectMany(..)
                | Action::Seek(_)
                | Action::Step(_)
                | Action::Play
                | Action::ViewerOption(_)
                | Action::PreviewChannel(_)
                | Action::Checkerboard
                | Action::FitPreview
                | Action::ZoomPreview(_)
        ) {
            self.pixel_info = None;
        }
        if self.should_blur_for_file_action(action) {
            // GPUI activates focused buttons on Enter key-up. A native file dialog
            // may close on key-down, sending its key-up back to the editor button.
            window.blur();
        }
        match action {
            Action::BeginParagraph(rect) => {
                self.stop();
                match crate::text_edit::Session::new_box(
                    self.editor.project(),
                    self.document_revision,
                    self.frame,
                    *rect,
                ) {
                    Ok(session) => {
                        self.text_session = Some(session);
                        self.status =
                            "Edit paragraph text · Ctrl+Enter finishes · Esc cancels".into();
                    }
                    Err(e) => self.status = e,
                }
            }
            Action::BeginText(id, position) => {
                self.stop();
                match crate::text_edit::Session::new(
                    self.editor.project(),
                    self.document_revision,
                    self.frame,
                    *id,
                    *position,
                ) {
                    Ok(mut session) => {
                        if let Some(id) = id {
                            self.editor.select(*id);
                            self.selected_layers = [*id].into();
                            session.buffer.all();
                        }
                        self.text_session = Some(session);
                        self.status = "Edit text · Ctrl+Enter finishes · Esc cancels".into();
                    }
                    Err(e) => self.status = e,
                }
            }
            Action::CommitText => self.finish_text(true, cx),
            Action::CancelText => self.finish_text(false, cx),
            Action::Preset(action) => self.preset_action(action, window, cx),
            Action::ManageFonts => {
                self.stop();
                self.fonts_open = true;
            }
            Action::OpenVertex(request) => {
                // The idle Pen captured this context before focus left the canvas.
                // Do not call stop(): that would invalidate its transport generation.
                match crate::panels::vertex_editor::Session::new(self, request.clone()) {
                    Ok(session) => {
                        self.vertex_editor = Some(session);
                        self.status = "Vertex draft · OK applies one edit · Cancel discards".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            Action::ApplyVertex => self.accept_vertex_editor(),
            Action::CancelVertex => self.cancel_vertex_editor(),
            Action::OpenGradient(item) => {
                self.stop();
                self.gradient_preview = None;
                self.gradient_controls = None;
                match crate::panels::gradient_editor::Session::new(self, *item) {
                    Ok(session) => {
                        self.gradient_editor = Some(session);
                        self.status =
                            "Gradient draft · OK applies one edit · Cancel discards".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            Action::ApplyGradient => self.accept_gradient_editor(),
            Action::CancelGradient => {
                self.gradient_editor = None;
                self.status = "Gradient edit canceled".into();
            }
            Action::OpenColor(target) => {
                // A stale Text swatch must not become valid merely because
                // opening a generic picker normally stops playback.
                if !color_open_context_matches(*target, self.editor.selected(), self.playing) {
                    self.status =
                        "Select the original text layer and stop playback before editing its paint"
                            .into();
                    cx.notify();
                    return;
                }
                self.stop();
                match crate::color_edit::Session::new(
                    *target,
                    self.editor.project(),
                    self.document_revision,
                    self.frame,
                ) {
                    Ok(session) => {
                        self.colors.session = Some(session);
                        self.colors.serial = self.colors.serial.wrapping_add(1);
                    }
                    Err(error) => self.status = error,
                }
            }
            Action::CancelColor => {
                if self.colors.picking() {
                    self.colors.session.as_mut().unwrap().picking = false;
                    self.status = "Color sample canceled".into();
                } else {
                    self.colors.session = None;
                    self.status = "Color edit canceled".into();
                }
            }
            Action::PickColor => {
                if let Some(session) = &mut self.colors.session {
                    session.picking = true;
                    session.error.clear();
                }
                self.status = "Click inside the Composition to sample its RGBA pixels. Esc returns to the color dialog.".into();
            }
            Action::SampleColor(rgba) => {
                if let Some(session) = &mut self.colors.session {
                    match session.validate_context(
                        self.editor.project(),
                        self.document_revision,
                        self.frame,
                        self.editor.selected(),
                        self.playing,
                    ) {
                        Ok(()) => {
                            self.status = format!(
                                "Sampled RGBA {}, {}, {}, {}",
                                rgba[0], rgba[1], rgba[2], rgba[3]
                            );
                            session.set_color(crate::color_edit::Color::rgba(*rgba));
                            session.picking = false;
                        }
                        Err(error) => {
                            session.error = error;
                            session.picking = false;
                        }
                    }
                }
            }
            Action::ApplyColor => {
                if let Some(mut session) = self.colors.session.take() {
                    let result = if !session.error.is_empty() {
                        Err(session.error.clone())
                    } else {
                        session.validate_context(
                            self.editor.project(),
                            self.document_revision,
                            self.frame,
                            self.editor.selected(),
                            self.playing,
                        )
                    };
                    if let Err(error) = result {
                        session.error = error;
                        self.colors.session = Some(session);
                    } else {
                        let color = session.color;
                        if let Some(command) = session.command() {
                            self.dispatch(&Action::Edit(command), window, cx);
                        } else {
                            self.status = "Color accepted".into();
                        }
                        if self.status == "Edited" || self.status == "Color accepted" {
                            if matches!(
                                session.target,
                                crate::color_edit::Target::BackgroundDraft(_)
                            ) {
                                self.colors.background_result = Some(color.rgb);
                            }
                            self.colors.remember(color);
                            if let Some(path) = crate::color_edit::Workflow::path()
                                && let Err(error) = self.colors.save(&path)
                            {
                                self.status = format!(
                                    "Color applied; recent colors could not be saved: {error}"
                                );
                            }
                        } else {
                            session.error = self.status.clone();
                            self.colors.session = Some(session);
                        }
                    }
                }
            }
            Action::Queue(action) => self.queue_action(action, window, cx),
            Action::AddMarker(target) => {
                let existing = self
                    .editor
                    .project()
                    .composition()
                    .marker_track(*target)
                    .and_then(|t| t.iter().find(|m| m.frame() == self.frame))
                    .map(|m| m.id());
                if existing.is_none() {
                    self.dispatch(
                        &Action::Edit(Command::Marker {
                            target: *target,
                            edit: libre_effects_core::MarkerEdit::Add { frame: self.frame },
                        }),
                        window,
                        cx,
                    );
                }
                if let Some(id) = existing.or_else(|| {
                    self.editor
                        .project()
                        .composition()
                        .marker_track(*target)
                        .and_then(|t| t.iter().find(|m| m.frame() == self.frame))
                        .map(|m| m.id())
                }) {
                    self.dispatch(&Action::ShowMarker(*target, id), window, cx);
                }
            }
            Action::ShowMarker(target, id) => {
                if let Some(frame) = self
                    .editor
                    .project()
                    .composition()
                    .marker_track(*target)
                    .and_then(|t| t.iter().find(|m| m.id() == *id))
                    .map(|m| m.frame())
                {
                    self.stop();
                    self.frame = frame;
                    let visible = self.visible_frames();
                    if frame < self.timeline_start
                        || frame >= self.timeline_start.saturating_add(visible)
                    {
                        self.timeline_start = frame.saturating_sub(visible / 2);
                    }
                    if let libre_effects_core::MarkerTarget::Layer(id) = target {
                        self.editor.select(*id);
                        self.selected_layers = [*id].into();
                        self.selected_keys.clear();
                    }
                    self.marker_selection =
                        Some((self.editor.project().active_composition_id(), *target, *id));
                }
            }
            Action::NavigateMarker(next) => {
                let comp = self.editor.project().composition();
                let mut markers: Vec<_> = comp
                    .markers()
                    .iter()
                    .map(|m| {
                        (
                            m.frame(),
                            libre_effects_core::MarkerTarget::Composition,
                            m.id(),
                        )
                    })
                    .chain(self.editor.selected_layer().into_iter().flat_map(|l| {
                        l.markers().iter().map(move |m| {
                            (
                                m.frame(),
                                libre_effects_core::MarkerTarget::Layer(l.id()),
                                m.id(),
                            )
                        })
                    }))
                    .filter(|(f, _, _)| {
                        if *next {
                            *f > self.frame
                        } else {
                            *f < self.frame
                        }
                    })
                    .collect();
                markers.sort_by_key(|m| m.0);
                if let Some((_, target, id)) = if *next {
                    markers.first()
                } else {
                    markers.last()
                } {
                    self.dispatch(&Action::ShowMarker(*target, *id), window, cx);
                }
            }
            Action::GraphProperty(id, property) => {
                if self
                    .editor
                    .project()
                    .composition()
                    .layer(*id)
                    .is_some_and(|l| l.track(*property).is_some())
                {
                    self.editor.select(*id);
                    self.selected_layers = [*id].into();
                    self.selected_keys.clear();
                    self.graph_key = None;
                    if *property == PropertyPath::SourceText {
                        // Source Text selection is Timeline-only. Keep numeric
                        // Graph lane identity, pins and ranges unchanged.
                        self.graph_activate_property(
                            crate::view_state::GraphChannel {
                                id: *id,
                                property: *property,
                            },
                            false,
                        );
                        self.graph_open = false;
                        self.status =
                            "Source Text · Hold only · Edit in Properties or on canvas".into();
                    } else {
                        self.graph_property = *property;
                        self.graph_external_activation(*id, *property);
                        self.graph_open = !matches!(property, PropertyPath::Path(_));
                        if matches!(property, PropertyPath::Path(_)) {
                            self.tool = Tool::Pen;
                        }
                    }
                    self.expanded = true;
                }
            }
            Action::ToggleSelectedSwitch(switch) => {
                let layers: Vec<_> = self
                    .editor
                    .project()
                    .composition()
                    .layers()
                    .iter()
                    .filter(|l| self.selected_layers.contains(&l.id()))
                    .collect();
                let enabled = !layers.iter().all(|l| match switch {
                    libre_effects_core::LayerSwitch::Solo => l.solo(),
                    libre_effects_core::LayerSwitch::Shy => l.shy(),
                    libre_effects_core::LayerSwitch::Guide => l.guide(),
                });
                let command = Command::Batch(
                    layers
                        .iter()
                        .map(|l| Command::SetLayerSwitch {
                            id: l.id(),
                            switch: *switch,
                            enabled,
                        })
                        .collect(),
                );
                self.dispatch(&Action::Edit(command), window, cx);
            }
            Action::SelectMany(id, toggle, range) => {
                let comp = self.editor.project().composition();
                if *range {
                    let a = comp
                        .layers()
                        .iter()
                        .position(|l| Some(l.id()) == self.editor.selected());
                    let b = comp.layers().iter().position(|l| l.id() == *id);
                    if let (Some(a), Some(b)) = (a, b) {
                        self.selected_layers.extend(
                            comp.layers()[a.min(b)..=a.max(b)]
                                .iter()
                                .filter(|l| !(comp.hide_shy() && l.shy()))
                                .map(|l| l.id()),
                        );
                    }
                } else if *toggle {
                    if !self.selected_layers.remove(id) {
                        self.selected_layers.insert(*id);
                    }
                } else {
                    self.selected_layers.clear();
                    self.selected_layers.insert(*id);
                }
                if self.selected_layers.contains(id) {
                    self.editor.select(*id);
                } else if let Some(id) = self.selected_layers.first() {
                    self.editor.select(*id);
                } else {
                    self.editor.clear_selection();
                }
                self.selected_keys.clear();
                self.graph_external_layer_selection();
            }
            Action::CopySelection => {
                if self.selected_keys.is_empty() {
                    self.copy_layers();
                } else {
                    self.dispatch(&Action::CopyKeys, window, cx);
                }
            }
            Action::CopyLayers => self.copy_layers(),
            Action::CutSelection => {
                let previous = (self.clipboard.clone(), self.layer_clipboard.clone());
                self.dispatch(&Action::CopySelection, window, cx);
                if self.status.starts_with("Copied") {
                    self.dispatch(&Action::DeleteSelection, window, cx);
                    if !self.status.starts_with("Edited") {
                        (self.clipboard, self.layer_clipboard) = previous;
                    } else {
                        self.status = "Cut selection".into();
                    }
                }
            }
            Action::PasteSelection => {
                let action = if self.layer_clipboard.is_some() {
                    Action::PasteLayers
                } else {
                    Action::PasteKeys
                };
                self.dispatch(&action, window, cx);
            }
            Action::PasteLayers => {
                if let Some(clipboard) = &self.layer_clipboard {
                    let command = Command::PasteLayers(clipboard.clone());
                    let original: BTreeSet<_> = self
                        .editor
                        .project()
                        .composition()
                        .layers()
                        .iter()
                        .map(|l| l.id())
                        .collect();
                    self.dispatch(&Action::Edit(command), window, cx);
                    if self.status.starts_with("Edited") {
                        self.selected_keys.clear();
                        self.selected_layers = self
                            .editor
                            .project()
                            .composition()
                            .layers()
                            .iter()
                            .map(|l| l.id())
                            .filter(|id| !original.contains(id))
                            .collect();
                        self.status = format!("Pasted {} layers", self.selected_layers.len());
                    }
                } else {
                    self.status = "Copy layers first".into();
                }
            }
            Action::CopyKeys => {
                self.layer_clipboard = None;
                self.clipboard = self
                    .selected_keys
                    .iter()
                    .filter_map(|k| {
                        self.editor
                            .project()
                            .composition()
                            .layer(k.id)?
                            .copy_key(k.property, k.frame)
                    })
                    .collect();
                self.status = format!("Copied {} keyframes", self.clipboard.len());
            }
            Action::PasteKeys => {
                let single = self
                    .clipboard
                    .iter()
                    .map(|k| k.key.id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 1;
                let target = if single { self.editor.selected() } else { None };
                let first = self.clipboard.iter().map(|k| k.key.frame).min();
                let command = Command::PasteKeys {
                    keys: self.clipboard.clone(),
                    frame: self.frame,
                    target,
                };
                self.dispatch(&Action::Edit(command), window, cx);
                if self.status.starts_with("Edited")
                    && let Some(first) = first
                {
                    self.selected_keys = self
                        .clipboard
                        .iter()
                        .map(|k| KeyRef {
                            id: target.unwrap_or(k.key.id),
                            property: k.key.property,
                            frame: self.frame + (k.key.frame - first),
                        })
                        .collect();
                    self.selected_layers = self.selected_keys.iter().map(|k| k.id).collect();
                }
            }
            Action::DeleteSelection => {
                let command = if !self.selected_keys.is_empty() {
                    Command::DeleteKeys(self.selected_keys.iter().copied().collect())
                } else {
                    // Detach children first, including selected ones, to keep deletion atomic.
                    let mut commands = Vec::new();
                    for l in self.editor.project().composition().layers() {
                        if l.track_matte()
                            .is_some_and(|m| self.selected_layers.contains(&m.source))
                        {
                            commands.push(Command::SetTrackMatte {
                                id: l.id(),
                                matte: None,
                            });
                        }
                        if l.parent()
                            .is_some_and(|id| self.selected_layers.contains(&id))
                        {
                            commands.push(Command::SetParent {
                                id: l.id(),
                                parent: None,
                                frame: self.frame,
                            });
                        }
                    }
                    commands.extend(
                        self.selected_layers
                            .iter()
                            .copied()
                            .map(Command::RemoveLayer),
                    );
                    Command::Batch(commands)
                };
                self.dispatch(&Action::Edit(command), window, cx);
            }
            Action::PrecomposeSelection => {
                let command = Command::Precompose {
                    layers: self.selected_layers.iter().copied().collect(),
                    name: format!("Pre-comp {}", self.editor.project().compositions().len()),
                };
                self.dispatch(&Action::Edit(command), window, cx);
            }
            Action::AddComposition(composition) => {
                self.dispatch(
                    &Action::Edit(Command::AddCompositionLayer {
                        composition: *composition,
                        frame: self.frame,
                    }),
                    window,
                    cx,
                );
            }
            Action::DuplicateSelection | Action::SplitSelection => {
                let original: BTreeSet<_> = self
                    .editor
                    .project()
                    .composition()
                    .layers()
                    .iter()
                    .map(|l| l.id())
                    .collect();
                self.dispatch(
                    &Action::Edit(if matches!(action, Action::SplitSelection) {
                        Command::SplitLayers {
                            ids: self.selected_layers.iter().copied().collect(),
                            frame: self.frame,
                        }
                    } else {
                        Command::DuplicateLayers(self.selected_layers.iter().copied().collect())
                    }),
                    window,
                    cx,
                );
                if self.status.starts_with("Edited") {
                    self.selected_layers = self
                        .editor
                        .project()
                        .composition()
                        .layers()
                        .iter()
                        .map(|l| l.id())
                        .filter(|id| !original.contains(id))
                        .collect();
                    self.selected_keys.clear();
                }
            }
            Action::ToggleTimeRemap | Action::FreezeTimeRemap => {
                let ids: Vec<_> = self.selected_layers.iter().copied().collect();
                if ids.is_empty() {
                    self.status = "Select a video or precomposition layer".into();
                } else {
                    let enabled = ids.iter().any(|id| {
                        self.editor
                            .project()
                            .composition()
                            .layer(*id)
                            .is_some_and(|l| l.time_remap().is_none())
                    });
                    let freeze = matches!(action, Action::FreezeTimeRemap);
                    self.dispatch(
                        &Action::Edit(Command::Batch(
                            ids.iter()
                                .map(|id| {
                                    if freeze {
                                        Command::FreezeTimeRemap {
                                            id: *id,
                                            frame: self.frame,
                                        }
                                    } else {
                                        Command::SetTimeRemap { id: *id, enabled }
                                    }
                                })
                                .collect(),
                        )),
                        window,
                        cx,
                    );
                    if self.status.starts_with("Edited") && (enabled || freeze) {
                        self.graph_property = PropertyPath::TimeRemap;
                        if let Some(id) = self.editor.selected() {
                            self.graph_external_activation(id, PropertyPath::TimeRemap);
                        }
                        self.graph_key = None;
                        self.expanded = true;
                        self.property_filter = None;
                    }
                }
            }
            Action::AddText => {
                let comp = self.editor.project().composition();
                self.dispatch(
                    &Action::BeginText(
                        None,
                        [comp.width() as f64 / 2.0, comp.height() as f64 / 2.0],
                    ),
                    window,
                    cx,
                );
            }
            Action::ImportImage => self.import_assets(false, false, cx),
            Action::CompositionFromFootage => self.import_assets(false, true, cx),
            Action::ImportImageSequence => self.import_assets(true, false, cx),
            Action::RelinkSequence(id) => self.relink_sequence(*id, cx),
            Action::ImportVideo => self.import_video(false, cx),
            Action::RelinkVideo => self.import_video(true, cx),
            Action::ManageMedia => {
                self.media_open = true;
                self.refresh_media(cx);
            }
            Action::RefreshMedia => self.refresh_media(cx),
            Action::RelinkSource(path) => self.relink_media(Some(path.clone()), cx),
            Action::RelinkMissing => self.relink_media(None, cx),
            Action::RefreshFootage => {
                crate::footage::clear_cache();
                self.preview_revision = self.preview_revision.wrapping_add(1);
            }
            Action::ExportFrame => self.export(false, false, cx),
            Action::ExportFrameBackground => self.export(false, true, cx),
            Action::TrimSelection(start) => self.dispatch(
                &Action::Edit(Command::TrimLayers {
                    ids: self.selected_layers.iter().copied().collect(),
                    frame: self.frame,
                    start: *start,
                }),
                window,
                cx,
            ),
            Action::NudgeSelection(x, y) => self.dispatch(
                &Action::Edit(Command::NudgeLayers {
                    ids: self.selected_layers.iter().copied().collect(),
                    frame: self.frame,
                    delta: [*x, *y],
                }),
                window,
                cx,
            ),
            Action::ExportSequence => self.export(true, false, cx),
            Action::ExportSequenceBackground => self.export(true, true, cx),
            Action::ExportVideo(preset) => self.export_video(*preset, cx),
            Action::DismissRender => {
                if !self.exporting {
                    self.video_job = None;
                }
            }
            Action::CancelExport => self
                .export_cancel
                .store(true, std::sync::atomic::Ordering::Relaxed),
            Action::ToggleGraph => {
                self.graph_open = !self.graph_open;
                if self.graph_open {
                    self.graph_prune_excluded_keys();
                }
            }
            Action::ZoomTimeline(factor) => {
                self.timeline_zoom = (self.timeline_zoom * factor).clamp(1.0, 64.0);
                self.timeline_start = self.frame.saturating_sub(self.visible_frames() / 2);
            }
            Action::PanTimeline(delta) => {
                self.timeline_start =
                    (i64::from(self.timeline_start) + i64::from(*delta)).max(0) as Frame
            }
            Action::ZoomPreview(factor) => {
                self.preview_zoom =
                    Some((self.preview_zoom.unwrap_or(0.5) * factor).clamp(0.0625, 8.0))
            }
            Action::FitPreview => {
                self.preview_zoom = None;
                self.preview_pan = [0.0; 2];
            }
            Action::CyclePreviewResolution => {
                self.preview_resolution = if self.preview_resolution == 4 {
                    1
                } else {
                    self.preview_resolution * 2
                }
            }
            Action::Checkerboard => self.checkerboard = !self.checkerboard,
            Action::ViewerOption(option) => self.viewer.toggle(*option),
            Action::PreviewChannel(channel) => self.viewer.channel = *channel,
            Action::ClearGuides => {
                if self.viewer.lock_guides {
                    self.status = "Unlock guides before clearing them".into();
                } else {
                    self.dispatch(&Action::Edit(Command::SetGuides(Vec::new())), window, cx);
                }
            }
            Action::SetTool(tool) => self.tool = *tool,
            Action::WorkStart => {
                self.stop();
                if let Err(error) = self.editor.execute(Command::SetWorkArea {
                    start: self.frame,
                    end: self.work_end.max(self.frame + 1),
                }) {
                    self.status = error;
                }
            }
            Action::WorkEnd => {
                self.stop();
                if let Err(error) = self.editor.execute(Command::SetWorkArea {
                    start: self.work_start.min(self.frame),
                    end: self.frame + 1,
                }) {
                    self.status = error;
                }
            }
            Action::ToggleExpanded => self.expanded = !self.expanded,
            Action::Filter(filter) => {
                self.property_filter = *filter;
                self.expanded = true;
            }
            Action::PreviousKey | Action::NextKey => {
                let next = matches!(action, Action::NextKey);
                let mut frames: Vec<_> = self
                    .editor
                    .selected_layer()
                    .into_iter()
                    .flat_map(|layer| {
                        layer.track_paths().into_iter().flat_map(|p| {
                            layer
                                .track(p)
                                .into_iter()
                                .flat_map(|t| t.keys().keys().copied())
                        })
                    })
                    .filter(|frame| {
                        if next {
                            *frame > self.frame
                        } else {
                            *frame < self.frame
                        }
                    })
                    .collect();
                frames.sort_unstable();
                if let Some(frame) = if next { frames.first() } else { frames.last() } {
                    let frame = *frame;
                    self.stop();
                    self.frame = frame;
                }
            }
            Action::Edit(command) => self.apply_edit(command),
            Action::TransformLayers(_) => {
                // Unlike menu render, this runs after modal gates and text commit.
                let command = action.layer_transform_command(self).unwrap();
                self.apply_edit(&command);
            }
            Action::ActivateComposition(id) => {
                if *id != self.editor.project().active_composition_id() {
                    self.remember_view();
                    match self.editor.activate_composition(*id) {
                        Ok(()) => {
                            self.composition_changed();
                            self.status = format!(
                                "Composition: {}",
                                self.editor.project().composition().name()
                            );
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
            Action::Select(id) => {
                self.editor.select(*id);
                self.selected_layers.clear();
                self.selected_layers.insert(*id);
                self.selected_keys.clear();
                self.graph_external_layer_selection();
            }
            Action::Seek(frame) => {
                self.stop();
                self.frame = (*frame).min(self.editor.project().composition().duration() - 1);
                self.queue_scrub(window, cx);
            }
            Action::Step(delta) => {
                self.stop();
                self.frame = (i64::from(self.frame) + i64::from(*delta)).clamp(
                    0,
                    i64::from(self.editor.project().composition().duration() - 1),
                ) as Frame;
                self.queue_scrub(window, cx);
            }
            Action::CacheWorkArea => {
                let was_caching = self.preview_caching;
                self.stop();
                self.preview_caching = !was_caching && self.preview_cache_limit > 0;
                self.status = if self.preview_caching {
                    "Caching work area…"
                } else {
                    "RAM caching stopped"
                }
                .into();
            }
            Action::CycleCacheBudget => {
                self.stop();
                let mib = crate::preview_cache::MIB;
                self.preview_cache_limit = match self.preview_cache_limit / mib {
                    0 => 64 * mib,
                    64 => 256 * mib,
                    256 => 512 * mib,
                    _ => 0,
                };
            }
            Action::PurgePreviewCache => {
                self.stop();
                self.preview_revision = self.preview_revision.wrapping_add(1);
                self.preview_cache = Default::default();
                self.status = "RAM preview cache cleared".into();
            }
            Action::PreviewAudio => {
                self.stop();
                self.preview_audio = !self.preview_audio;
            }
            Action::PreviewScrub => {
                self.stop();
                self.preview_scrub = !self.preview_scrub;
            }
            Action::PreviewLoop => {
                self.stop();
                self.preview_loop = !self.preview_loop;
            }
            Action::Play => {
                if self.playing {
                    self.stop();
                } else {
                    self.start_playback(window, cx);
                }
            }
            Action::Undo | Action::Redo => {
                self.step_history(matches!(action, Action::Redo));
            }
            Action::New => {
                if let Err(error) = self.install_new_project() {
                    self.status = error;
                }
            }
            Action::Open => self.open(cx),
            Action::SaveAs => self.save_as(cx),
            Action::Save => self.save(cx),
            Action::CollectFiles => self.collect_files(cx),
            Action::CancelCollection => self
                .collection_cancel
                .store(true, std::sync::atomic::Ordering::Relaxed),
        }
        // These actions preserve selection, time and topology. Normalizing an
        // empty auxiliary layer set here would invalidate the explicit Pen
        // context just captured for opening or the one-shot return on closing.
        if !matches!(
            action,
            Action::OpenVertex(_) | Action::ApplyVertex | Action::CancelVertex
        ) {
            self.normalize();
        }
        cx.notify();
    }

    fn schedule_frame(&self, generation: u64, window: &mut Window, cx: &mut Context<Self>) {
        cx.on_next_frame(window, move |state, window, cx| {
            if (!state.playing && state.audio_session.is_none())
                || generation != state.playback_generation
            {
                return;
            }
            if state.audio_session.is_some() {
                state.poll_audio();
                cx.notify();
                if state.playing || state.audio_session.is_some() {
                    state.schedule_frame(generation, window, cx);
                }
            } else if let Some((start, first)) = state.playback_origin {
                let comp = state.editor.project().composition();
                let elapsed = (start.elapsed().as_secs_f64() * comp.fps().as_f64()) as u64;
                let end = state.work_end.min(comp.duration());
                let start = state.work_start.min(end - 1);
                let position = u64::from(first.saturating_sub(start)) + elapsed;
                if !state.preview_loop && position >= u64::from(end - start) {
                    state.frame = end - 1;
                    state.stop();
                    cx.notify();
                    return;
                }
                state.frame = start + (position % u64::from(end - start)) as Frame;
                cx.notify();
                state.schedule_frame(generation, window, cx);
            }
        });
    }
}

fn color_open_context_matches(
    target: crate::color_edit::Target,
    selected: Option<LayerId>,
    playing: bool,
) -> bool {
    match target {
        crate::color_edit::Target::Text(id, _) => !playing && selected == Some(id),
        _ => true,
    }
}

pub(crate) fn action_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    state: &Entity<EditorState>,
    action: Action,
) -> Button {
    let state = state.clone();
    Button::new(id, label)
        .size(ButtonSize::Small)
        .variant(ButtonVariant::Ghost)
        .on_click(move |_, window, cx| {
            state.update(cx, |state, cx| state.dispatch(&action, window, cx));
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contents_cross_parent_move_retains_keys_until_history_and_keeps_graph_identity() {
        use crate::view_state::{GraphChannel, GraphRanges};
        use libre_effects_core::{ContentsEdit, ContentsKind, ContentsParam, TrackEdit};
        let mut state = EditorState::default();
        state
            .editor
            .execute(Command::AddContent {
                content: Content::ShapeContents(Default::default()),
                width: 200.,
                height: 120.,
                name: "Contents key identity".into(),
            })
            .unwrap();
        for parent in [0, 0, 1] {
            state
                .editor
                .execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::Add {
                        parent,
                        kind: ContentsKind::Group(vec![]),
                    },
                })
                .unwrap();
        }
        let keys = [Property::PositionX, Property::Rotation].map(|property| KeyRef {
            id: 1,
            property: PropertyPath::Contents {
                item: 3,
                parameter: ContentsParam::Transform(property),
            },
            frame: 0,
        });
        for key in keys {
            state
                .editor
                .execute(Command::EditTrack {
                    id: key.id,
                    property: key.property,
                    edit: TrackEdit::ToggleKey { frame: key.frame },
                })
                .unwrap();
            let channel = GraphChannel::from(key);
            state.graph_channels.pin(channel).unwrap();
            state.graph_channels.ranges.insert(
                channel,
                GraphRanges {
                    value: Some([-75., 125.]),
                    speed: Some([-300., 300.]),
                },
            );
        }
        state.graph_channels.activate(keys[1].into());
        state.graph_key = Some(keys[1]);
        state.selected_keys = keys.into();
        state.selected_layers.insert(1);
        state.normalize();
        let before = state.editor.project().clone();
        let pins = state.graph_channels.clone();
        state.apply_edit(&Command::Contents {
            id: 1,
            edit: ContentsEdit::MoveSiblings {
                source_parent: 1,
                items: vec![3],
                parent: 2,
                index: 0,
            },
        });
        assert_eq!(state.status, "Edited");
        state.normalize();
        let after = state.editor.project().clone();
        assert_ne!(before, after);
        assert_eq!(state.selected_keys, keys.into());
        assert_eq!(state.graph_key, Some(keys[1]));
        assert_eq!(state.graph_channels, pins);
        for redo in [false, true] {
            state.selected_keys = keys.into();
            state.graph_key = Some(keys[1]);
            state.step_history(redo);
            assert_eq!(state.editor.project(), if redo { &after } else { &before });
            assert!(state.selected_keys.is_empty());
            assert_eq!(state.graph_key, None);
            assert_eq!(state.graph_channels, pins);
            assert_eq!(state.selected_layers, [1].into());
        }
    }
    #[test]
    fn text_opacity_picker_open_rejects_stale_context_before_stopping_playback() {
        use crate::color_edit::Target;
        use libre_effects_core::TextPaint;
        for paint in [TextPaint::Fill, TextPaint::Stroke] {
            let target = Target::Text(7, paint);
            assert!(color_open_context_matches(target, Some(7), false));
            for selected in [None, Some(8)] {
                assert!(!color_open_context_matches(target, selected, false));
            }
            assert!(!color_open_context_matches(target, Some(7), true));
        }
        // Generic pickers retain the prior stop-on-open behavior.
        assert!(color_open_context_matches(Target::Fill(7), None, true));
        assert!(color_open_context_matches(Target::Stroke(7), Some(8), true));
    }

    #[test]
    fn vertex_modal_only_routes_its_own_accept_and_cancel() {
        assert!(Action::ApplyVertex.allowed_in_vertex_editor());
        assert!(Action::CancelVertex.allowed_in_vertex_editor());
        for action in [
            Action::Undo,
            Action::Redo,
            Action::Edit(Command::AddRectangle),
            Action::New,
            Action::Open,
            Action::Save,
            Action::SaveAs,
            Action::CollectFiles,
            Action::ImportImage,
            Action::ExportFrame,
            Action::ManageFonts,
            Action::ManageMedia,
            Action::ActivateComposition(2),
            Action::Select(2),
            Action::SelectMany(2, true, false),
            Action::Seek(12),
            Action::Step(1),
            Action::Play,
            Action::SetTool(Tool::Select),
            Action::OpenGradient(2),
            Action::ApplyGradient,
            Action::CancelGradient,
            Action::ApplyColor,
            Action::CancelColor,
            Action::DeleteSelection,
            Action::CopySelection,
            Action::CutSelection,
            Action::PasteSelection,
            Action::ZoomPreview(2.),
            Action::ZoomTimeline(2.),
            Action::Queue(queue::QueueAction::Undo),
        ] {
            assert!(!action.allowed_in_vertex_editor());
        }
    }

    #[test]
    fn removed_effect_keys_do_not_leave_stale_graph_or_selection_addresses() {
        use libre_effects_core::{EffectEdit, EffectKind, EffectParam, TrackEdit};
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state
            .editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::GaussianBlur),
            })
            .unwrap();
        let property = PropertyPath::Effect {
            effect: 1,
            parameter: EffectParam::Radius,
        };
        state
            .editor
            .execute(Command::EditTrack {
                id: 1,
                property,
                edit: TrackEdit::ToggleKey { frame: 0 },
            })
            .unwrap();
        state.graph_property = property;
        state.graph_key = Some(KeyRef {
            id: 1,
            property,
            frame: 0,
        });
        state.selected_keys = [KeyRef {
            id: 1,
            property,
            frame: 0,
        }]
        .into();
        state
            .editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Remove(1),
            })
            .unwrap();
        state.normalize();
        assert!(state.selected_keys.is_empty());
        assert_eq!(state.graph_key, None);
        assert_eq!(state.graph_property, Property::PositionX.into());
    }
    #[test]
    fn composition_navigation_is_not_dirty_and_history_resets_timeline() {
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state
            .editor
            .execute(Command::SetWorkArea { start: 10, end: 50 })
            .unwrap();
        state.editor.execute(Command::NewComposition).unwrap();
        state.saved = state.editor.project().clone();
        state.editor.activate_composition(1).unwrap();
        assert!(!state.dirty());
        state.normalize();
        assert_eq!((state.work_start, state.work_end), (10, 50));
        state.editor.execute(Command::DeleteComposition).unwrap();
        state.frame = 100;
        state.step_history(false);
        assert_eq!(state.frame, 0);
        assert_eq!(state.editor.project().active_composition_id(), 1);
        assert_eq!(state.selected_layers, [1].into());
        assert!(!state.dirty());
        state.step_history(true);
        assert_eq!(state.editor.project().active_composition_id(), 2);
        assert!(state.selected_layers.is_empty());
        assert_eq!((state.work_start, state.work_end), (0, 150));
    }
    #[test]
    fn split_undo_redo_keeps_timeline_and_preview_selection_on_restored_layer() {
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state.selected_layers.insert(1);
        state.frame = 75;
        state
            .editor
            .execute(Command::SplitLayers {
                ids: vec![1],
                frame: 75,
            })
            .unwrap();
        state.selected_layers = [2].into();
        state.step_history(false);
        assert_eq!(state.selected_layers, [1].into());
        state.step_history(true);
        assert_eq!(state.selected_layers, [2].into());
        assert_eq!(state.editor.selected(), Some(2));
        assert!(
            state
                .editor
                .selected_layer()
                .unwrap()
                .active_at(state.frame, 150)
        );
    }
}
