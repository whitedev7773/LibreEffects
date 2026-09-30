use std::{path::Path, time::Instant};

use gpui::{Context, ElementId, Entity, PathPromptOptions, SharedString, Window};
use libre_effects_core::{Command, Editor, Frame, LayerId, Project};

use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::project_io::{read_project, write_project};

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
}

pub(crate) struct EditorState {
    pub editor: Editor,
    pub frame: Frame,
    pub playing: bool,
    pub status: String,
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
            playback_origin: None,
            playback_generation: 0,
        }
    }
}

impl EditorState {
    fn stop(&mut self) {
        self.playing = false;
        self.playback_origin = None;
        self.playback_generation = self.playback_generation.wrapping_add(1);
    }

    pub fn dispatch(&mut self, action: &Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
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
                        self.status = "New composition. Undo restores the previous project.".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            Action::Open => self.open(cx),
            Action::SaveAs => self.save_as(cx),
        }
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
                state.frame = ((u64::from(first) + elapsed) % u64::from(comp.duration())) as Frame;
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
