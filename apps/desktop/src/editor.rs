use std::{path::Path, time::Instant};

use gpui::{Context, ElementId, Entity, PathPromptOptions, SharedString, Window};
use libre_effects_core::{Command, Editor, Frame, LayerId, Project, Property};

use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::project_io::{read_project, write_project};

#[derive(Clone)]
pub(crate) enum Action {
    Edit(Command),
    Select(LayerId),
    Seek(Frame),
    Step(i32),
    Play,
    Undo,
    Redo,
    New,
    Open,
    SaveAs,
    ZoomTimeline(f32),
    PanTimeline(i32),
    ZoomPreview(f32),
    FitPreview,
    Checkerboard,
    SetTool(Tool),
    WorkStart,
    WorkEnd,
    PreviousKey,
    NextKey,
    ToggleExpanded,
    Filter(Option<PropertyFilter>),
    ToggleGraph,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    Select,
    Hand,
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
    pub editor: Editor,
    pub frame: Frame,
    pub playing: bool,
    pub status: String,
    pub timeline_zoom: f32,
    pub timeline_start: Frame,
    pub preview_zoom: Option<f32>,
    pub checkerboard: bool,
    pub tool: Tool,
    pub work_start: Frame,
    pub work_end: Frame,
    pub expanded: bool,
    pub property_filter: Option<PropertyFilter>,
    pub graph_open: bool,
    pub graph_property: Property,
    pub graph_key: Option<(LayerId, Frame)>,
    playback_origin: Option<(Instant, Frame)>,
    playback_generation: u64,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            editor: Editor::default(),
            frame: 0,
            playing: false,
            status: "Add a rectangle to start. Projects are saved as .lfe.json.".into(),
            timeline_zoom: 1.0,
            timeline_start: 0,
            preview_zoom: None,
            checkerboard: false,
            tool: Tool::Select,
            work_start: 0,
            work_end: 150,
            expanded: true,
            property_filter: None,
            graph_open: false,
            graph_property: Property::PositionX,
            graph_key: None,
            playback_origin: None,
            playback_generation: 0,
        }
    }
}

impl EditorState {
    pub fn visible_frames(&self) -> Frame {
        ((self.editor.project().composition().duration() as f32 / self.timeline_zoom).ceil()
            as Frame)
            .max(2)
    }
    fn normalize(&mut self) {
        if self.graph_key.is_some_and(|(id, frame)| {
            self.editor.selected() != Some(id)
                || !self
                    .editor
                    .project()
                    .composition()
                    .layer(id)
                    .is_some_and(|l| l.property(self.graph_property).keys().contains_key(&frame))
        }) {
            self.graph_key = None;
        }
        let duration = self.editor.project().composition().duration();
        self.frame = self.frame.min(duration - 1);
        self.work_start = self.work_start.min(duration - 1);
        self.work_end = self.work_end.min(duration).max(self.work_start + 1);
        self.timeline_start = self
            .timeline_start
            .min(duration.saturating_sub(self.visible_frames()));
    }
    fn stop(&mut self) {
        self.playing = false;
        self.playback_origin = None;
        self.playback_generation = self.playback_generation.wrapping_add(1);
    }

    pub fn dispatch(&mut self, action: &Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::ToggleGraph => self.graph_open = !self.graph_open,
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
            Action::FitPreview => self.preview_zoom = None,
            Action::Checkerboard => self.checkerboard = !self.checkerboard,
            Action::SetTool(tool) => self.tool = *tool,
            Action::WorkStart => {
                self.work_start = self.frame;
                self.work_end = self.work_end.max(self.frame + 1);
            }
            Action::WorkEnd => {
                self.work_end = self.frame + 1;
                self.work_start = self.work_start.min(self.frame);
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
                        libre_effects_core::Property::ALL
                            .into_iter()
                            .flat_map(|property| layer.property(property).keys().keys().copied())
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
            Action::Edit(command) => {
                self.stop();
                self.status = match self.editor.execute(command.clone()) {
                    Ok(()) => "Edited — use Save as to keep your changes.".into(),
                    Err(error) => error,
                };
            }
            Action::Select(id) => self.editor.select(*id),
            Action::Seek(frame) => {
                self.stop();
                self.frame = (*frame).min(self.editor.project().composition().duration() - 1);
            }
            Action::Step(delta) => {
                self.stop();
                self.frame = (i64::from(self.frame) + i64::from(*delta)).clamp(
                    0,
                    i64::from(self.editor.project().composition().duration() - 1),
                ) as Frame;
            }
            Action::Play => {
                if self.playing {
                    self.stop();
                } else {
                    self.playing = true;
                    if self.frame < self.work_start || self.frame >= self.work_end {
                        self.frame = self.work_start;
                    }
                    self.playback_origin = Some((Instant::now(), self.frame));
                    self.playback_generation = self.playback_generation.wrapping_add(1);
                    self.schedule_frame(self.playback_generation, window, cx);
                }
            }
            Action::Undo | Action::Redo => {
                self.stop();
                if matches!(action, Action::Undo) {
                    self.editor.undo();
                } else {
                    self.editor.redo();
                }
                self.frame = self
                    .frame
                    .min(self.editor.project().composition().duration() - 1);
                self.status = "History updated — use Save as to keep your changes.".into();
            }
            Action::New => {
                self.stop();
                match self.editor.replace_project(Project::default()) {
                    Ok(()) => {
                        self.frame = 0;
                        self.work_start = 0;
                        self.work_end = 150;
                        self.timeline_start = 0;
                        self.status = "New composition. Undo restores the previous project.".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            Action::Open => self.open(cx),
            Action::SaveAs => self.save_as(cx),
        }
        self.normalize();
        cx.notify();
    }

    fn schedule_frame(&self, generation: u64, window: &mut Window, cx: &mut Context<Self>) {
        cx.on_next_frame(window, move |state, window, cx| {
            if !state.playing || generation != state.playback_generation {
                return;
            }
            if let Some((start, first)) = state.playback_origin {
                let comp = state.editor.project().composition();
                let elapsed = (start.elapsed().as_secs_f64() * f64::from(comp.fps())) as u64;
                let end = state.work_end.min(comp.duration());
                let start = state.work_start.min(end - 1);
                state.frame = start
                    + ((u64::from(first.saturating_sub(start)) + elapsed) % u64::from(end - start))
                        as Frame;
                cx.notify();
                state.schedule_frame(generation, window, cx);
            }
        });
    }

    fn open(&mut self, cx: &mut Context<Self>) {
        self.stop();
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Libre Effects project (.lfe.json)".into()),
        });
        cx.spawn(async move |entity, cx| {
            let path = match prompt.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => return,
                error => {
                    let _ = entity.update(cx, |state, cx| {
                        state.status = format!("Could not open file picker: {error:?}");
                        cx.notify();
                    });
                    return;
                }
            };
            let Some(path) = path else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { read_project(&path) })
                .await;
            let _ = entity.update(cx, |state, cx| {
                match result.and_then(|project| state.editor.replace_project(project)) {
                    Ok(()) => {
                        state.stop();
                        state.frame = 0;
                        state.work_start = 0;
                        state.work_end = state.editor.project().composition().duration();
                        state.timeline_start = 0;
                        state.status = "Project opened. Undo restores the previous project.".into();
                    }
                    Err(error) => state.status = format!("Open failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_as(&mut self, cx: &mut Context<Self>) {
        self.stop();
        let json = match self.editor.project().to_json() {
            Ok(json) => json,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let directory = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        let prompt = cx.prompt_for_new_path(&directory, Some("Untitled.lfe.json"));
        cx.spawn(async move |entity, cx| {
            let path = match prompt.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => return,
                error => {
                    let _ = entity.update(cx, |state, cx| {
                        state.status = format!("Could not open save dialog: {error:?}");
                        cx.notify();
                    });
                    return;
                }
            };
            let result = cx
                .background_executor()
                .spawn(async move { write_project(&path, &json) })
                .await;
            let _ = entity.update(cx, |state, cx| {
                state.status = match result {
                    Ok(()) => "Saved the project snapshot from when Save as was clicked.".into(),
                    Err(error) => format!("Save failed: {error}"),
                };
                cx.notify();
            });
        })
        .detach();
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

pub(crate) fn timecode(frame: Frame, fps: u32) -> String {
    let seconds = frame / fps;
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60,
        frame % fps
    )
}
