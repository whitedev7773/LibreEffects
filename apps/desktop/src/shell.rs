use crate::{
    components::{Orientation, ResizablePanelGroup, TextField},
    editor::{Action, EditorState, PropertyFilter, Tool},
    panels::{Align, Browser, Preview, Sidebar, Timeline},
    ui,
};
use gpui::{Context, Entity, FocusHandle, KeyDownEvent, Window, div, prelude::*, px, rgb};
use libre_effects_core::Command;

pub(crate) struct Shell {
    state: Entity<EditorState>,
    layout: Entity<ResizablePanelGroup>,
    middle: Entity<ResizablePanelGroup>,
    upper: Entity<ResizablePanelGroup>,
    right: Entity<ResizablePanelGroup>,
    focus: FocusHandle,
    initialized: bool,
    menu: Option<&'static str>,
    settings: bool,
    settings_error: String,
    fields: Vec<Entity<TextField>>,
    help: bool,
    closing: bool,
    pending_document: Option<Action>,
    pending_save: bool,
    modal_active: bool,
}

impl Shell {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| EditorState::default());
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let browser = cx.new(|cx| Browser::new(state.clone(), cx));
        let preview = cx.new(|cx| Preview::new(state.clone(), cx));
        let sidebar = cx.new(|cx| Sidebar::new(state.clone(), cx));
        let align = cx.new(|cx| Align::new(state.clone(), cx));
        let timeline = cx.new(|cx| Timeline::new(state.clone(), cx));
        let upper = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Horizontal, browser, preview)
                .initial_fraction(0.20)
                .minimum_fraction(0.12)
        });
        let middle = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Vertical, upper.clone(), timeline)
                .initial_fraction(0.615)
                .minimum_fraction(0.22)
        });
        let right = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Vertical, sidebar, align)
                .initial_fraction(0.615)
                .minimum_fraction(0.2)
        });
        let layout = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Horizontal, middle.clone(), right.clone())
                .initial_fraction(0.84)
                .minimum_fraction(0.12)
        });
        let fields: Vec<_> = (0..6)
            .map(|_| cx.new(|cx| TextField::new(cx, |_, _, _| {})))
            .collect();
        cx.observe(&fields[5], |_, _, cx| cx.notify()).detach();
        Self {
            state,
            layout,
            middle,
            upper,
            right,
            focus: cx.focus_handle(),
            initialized: false,
            menu: None,
            settings: false,
            settings_error: String::new(),
            fields,
            help: false,
            closing: false,
            pending_document: None,
            pending_save: false,
            modal_active: false,
        }
    }
    fn dispatch(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(action, Action::New | Action::Open) {
            if self.state.read(cx).saving {
                self.state.update(cx, |s, cx| {
                    s.status = "Wait for the current save to finish.".into();
                    cx.notify();
                });
                return;
            }
            if self.state.read(cx).dirty() {
                self.pending_document = Some(action);
                self.pending_save = false;
                self.menu = None;
                cx.notify();
                return;
            }
        }
        self.state
            .update(cx, |state, cx| state.dispatch(&action, window, cx));
    }
    fn reset_layout(&mut self, cx: &mut Context<Self>) {
        self.layout.update(cx, |p, cx| p.reset(cx));
        self.upper.update(cx, |p, cx| p.reset(cx));
        self.middle.update(cx, |p, cx| p.reset(cx));
        self.right.update(cx, |p, cx| p.reset(cx));
    }
    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let comp = self.state.read(cx).editor.project().composition();
        let values = [
            comp.name().to_string(),
            comp.width().to_string(),
            comp.height().to_string(),
            comp.fps().to_string(),
            comp.duration().to_string(),
            format!("#{:06X}", comp.background_color()),
        ];
        for (field, value) in self.fields.iter().zip(values) {
            field.update(cx, |field, _| {
                field.sync("composition-settings".into(), value, window)
            });
        }
        self.settings_error.clear();
        self.settings = true;
        self.menu = None;
        cx.notify();
    }
    fn apply_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.fields[0].read(cx).value().to_string();
        let numbers: Result<Vec<u32>, _> = self.fields[1..5]
            .iter()
            .map(|field| field.read(cx).value().parse::<u32>())
            .collect();
        let Ok(n) = numbers else {
            self.settings_error = "Enter whole numbers for size, frame rate and duration.".into();
            cx.notify();
            return;
        };
        let background = match ui::parse_hex_color(self.fields[5].read(cx).value()) {
            Ok(color) => color,
            Err(error) => {
                self.settings_error = error.into();
                cx.notify();
                return;
            }
        };
        self.dispatch(
            Action::Edit(Command::Batch(vec![
                Command::ConfigureComposition {
                    name,
                    width: n[0],
                    height: n[1],
                    fps: n[2],
                    duration: n[3],
                },
                Command::SetCompositionBackground(background),
            ])),
            window,
            cx,
        );
        let status = self.state.read(cx).status.clone();
        if status.starts_with("Edited") {
            self.settings = false;
            window.focus(&self.focus);
        } else {
            self.settings_error = status;
        }
        cx.notify();
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if key == "escape" {
            self.closing = false;
            self.pending_document = None;
            self.pending_save = false;
            self.state.update(cx, |s, _| {
                s.close_after_save = false;
                s.marker_selection = None;
            });
            self.menu = None;
            self.settings = false;
            self.help = false;
            window.focus(&self.focus);
            cx.notify();
            return;
        }
        if self.settings
            || self.help
            || self.closing
            || self.pending_document.is_some()
            || self.state.read(cx).recovery.is_some()
        {
            return;
        }
        let action = if m.control {
            match key {
                "n" => Some(Action::New),
                "o" => Some(Action::Open),
                "s" => Some(if m.shift {
                    Action::SaveAs
                } else {
                    Action::Save
                }),
                "i" => Some(if m.shift {
                    Action::ImportVideo
                } else {
                    Action::ImportImage
                }),
                "c" => Some(if m.shift {
                    Action::PrecomposeSelection
                } else {
                    Action::CopySelection
                }),
                "x" => Some(Action::CutSelection),
                "v" => Some(Action::PasteSelection),
                "z" => Some(if m.shift { Action::Redo } else { Action::Undo }),
                "y" => Some(Action::Edit(Command::AddRectangle)),
                "d" => Some(if m.shift {
                    Action::SplitSelection
                } else {
                    Action::DuplicateSelection
                }),
                "k" => {
                    self.open_settings(window, cx);
                    None
                }
                _ => None,
            }
        } else if m.alt {
            match key {
                "[" => Some(Action::TrimSelection(true)),
                "]" => Some(Action::TrimSelection(false)),
                _ => None,
            }
        } else if m.shift && key == "f3" {
            Some(Action::ToggleGraph)
        } else {
            match key {
                "space" => Some(Action::Play),
                "left" => Some(Action::NudgeSelection(
                    if m.shift { -10.0 } else { -1.0 },
                    0.0,
                )),
                "right" => Some(Action::NudgeSelection(
                    if m.shift { 10.0 } else { 1.0 },
                    0.0,
                )),
                "up" => Some(Action::NudgeSelection(
                    0.0,
                    if m.shift { -10.0 } else { -1.0 },
                )),
                "down" => Some(Action::NudgeSelection(
                    0.0,
                    if m.shift { 10.0 } else { 1.0 },
                )),
                "home" => Some(Action::Seek(0)),
                "end" => Some(Action::Seek(
                    self.state
                        .read(cx)
                        .editor
                        .project()
                        .composition()
                        .duration()
                        - 1,
                )),
                "pageup" => Some(Action::Step(if m.shift { -10 } else { -1 })),
                "pagedown" => Some(Action::Step(if m.shift { 10 } else { 1 })),
                "delete" | "backspace" => Some(Action::DeleteSelection),
                "v" => Some(Action::SetTool(Tool::Select)),
                "h" => Some(Action::SetTool(Tool::Hand)),
                "w" => Some(Action::SetTool(Tool::Rotate)),
                "y" => Some(Action::SetTool(Tool::Anchor)),
                "p" => Some(Action::Filter(Some(PropertyFilter::Position))),
                "a" => Some(Action::Filter(Some(PropertyFilter::Anchor))),
                "s" => Some(Action::Filter(Some(PropertyFilter::Scale))),
                "r" => Some(Action::Filter(Some(PropertyFilter::Rotation))),
                "t" => Some(Action::Filter(Some(PropertyFilter::Opacity))),
                "u" => Some(Action::Filter(Some(PropertyFilter::Animated))),
                "j" => Some(Action::PreviousKey),
                "k" => Some(Action::NextKey),
                "b" => Some(Action::WorkStart),
                "n" => Some(Action::WorkEnd),
                "=" | "+" => Some(Action::ZoomTimeline(2.0)),
                "-" => Some(Action::ZoomTimeline(0.5)),
                _ => None,
            }
        };
        if let Some(action) = action {
            cx.stop_propagation();
            self.dispatch(action, window, cx);
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.initialized {
            self.state.update(cx, |s, cx| s.start_recovery(cx));
            let weak = cx.entity().downgrade();
            window.on_window_should_close(cx, move |window, cx| {
                TextField::commit_active(window, cx);
                // Commit an active field on blur before deciding whether the document
                // can close. Otherwise an uncommitted typed value could be lost.
                if weak.update(cx, |s, _| window.focus(&s.focus)).is_err() {
                    return true;
                }
                let weak = weak.clone();
                window.defer(cx, move |window, cx| {
                    let _ = weak.update(cx, |s, cx| {
                        if s.state.read(cx).exporting {
                            s.state.update(cx, |s, cx| { s.status = "A render is running. Cancel it or wait for completion before closing.".into(); cx.notify(); });
                        } else if s.state.read(cx).dirty() || s.state.read(cx).saving {
                            s.closing = true;
                            cx.notify();
                        } else {
                            s.state.read(cx).clear_recovery();
                            window.remove_window();
                        }
                    });
                });
                false
            });
            window.focus(&self.focus);
            cx.on_focus_lost(window, |this, window, _| window.focus(&this.focus))
                .detach();
            self.initialized = true;
        }
        if self.state.read(cx).request_open {
            self.state.update(cx, |s, _| s.request_open = false);
            self.dispatch(Action::Open, window, cx);
        }
        if self.pending_save && !self.state.read(cx).saving {
            self.pending_save = false;
            if !self.state.read(cx).dirty() {
                if let Some(action) = self.pending_document.take() {
                    self.state
                        .update(cx, |s, cx| s.dispatch(&action, window, cx));
                }
            }
        }
        let state = self.state.read(cx);
        let title = format!(
            "{}{} — Libre Effects",
            if state.dirty() { "* " } else { "" },
            state
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or("Untitled".into())
        );
        window.set_window_title(&title);
        if state.close_after_save && !state.saving && !state.dirty() && !state.exporting {
            state.clear_recovery();
            window.remove_window();
        }
        let selected = state.editor.selected();
        let tool = state.tool;
        let status = state.status.clone();
        let video_job = state.video_job.clone();
        let exporting = state.exporting;
        let mut root = div()
            .id("editor-workspace")
            .track_focus(&self.focus)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .font_family("Wanted Sans")
            .text_size(px(12.0))
            .text_color(rgb(ui::TEXT))
            .bg(rgb(ui::BG))
            .on_key_down(cx.listener(Self::key))
            .child(
                div()
                    .flex()
                    .h(px(27.0))
                    .flex_none()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(ui::BORDER))
                    .children(
                        [
                            "File",
                            "Edit",
                            "Composition",
                            "Layer",
                            "Effect",
                            "Animation",
                            "View",
                            "Window",
                            "Help",
                        ]
                        .into_iter()
                        .map(|name| {
                            ui::text_button(name, name)
                                .when(self.menu == Some(name), |s| s.bg(rgb(0x353535)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    window.focus(&this.focus);
                                    this.menu = if this.menu == Some(name) {
                                        None
                                    } else {
                                        Some(name)
                                    };
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .h(px(36.0))
                    .px_2()
                    .gap_1()
                    .flex_none()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(ui::BORDER))
                    .bg(rgb(ui::PANEL))
                    .child(ui::action_tool(
                        "select",
                        "square-dashed",
                        "Selection tool (V)",
                        &self.state,
                        Action::SetTool(Tool::Select),
                        tool == Tool::Select,
                    ))
                    .child(ui::action_tool(
                        "hand",
                        "hand",
                        "Hand tool (H)",
                        &self.state,
                        Action::SetTool(Tool::Hand),
                        tool == Tool::Hand,
                    ))
                    .child(ui::action_tool(
                        "rotate",
                        "arrow-rotate-right",
                        "Rotation tool (W)",
                        &self.state,
                        Action::SetTool(Tool::Rotate),
                        tool == Tool::Rotate,
                    ))
                    .child(ui::action_tool(
                        "anchor",
                        "target",
                        "Pan Behind / Anchor Point tool (Y)",
                        &self.state,
                        Action::SetTool(Tool::Anchor),
                        tool == Tool::Anchor,
                    ))
                    .child(div().mx_2().w(px(1.0)).h(px(20.0)).bg(rgb(0x414141)))
                    .child(ui::action_tool(
                        "rectangle",
                        "square",
                        "New rectangle layer (Ctrl+Y)",
                        &self.state,
                        Action::Edit(Command::AddRectangle),
                        false,
                    ))
                    .child(ui::action_tool(
                        "undo",
                        "arrow-rotate-left",
                        "Undo (Ctrl+Z)",
                        &self.state,
                        Action::Undo,
                        false,
                    ))
                    .child(ui::action_tool(
                        "redo",
                        "arrow-rotate-right",
                        "Redo (Ctrl+Shift+Z)",
                        &self.state,
                        Action::Redo,
                        false,
                    ))
                    .child(ui::action_tool(
                        "save",
                        "floppy-disk",
                        "Save project (Ctrl+S)",
                        &self.state,
                        Action::Save,
                        false,
                    ))
                    .child(div().flex_1())
                    .child(div().text_color(rgb(ui::BLUE)).mr_4().child("Default"))
                    .child(
                        ui::text_button("reset-workspace", "Reset workspace")
                            .on_click(cx.listener(|this, _, _, cx| this.reset_layout(cx))),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.layout.clone()))
            .when_some(video_job, |root, job| {
                root.child(
                    div()
                        .flex_none()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(rgb(ui::BORDER))
                        .bg(rgb(ui::PANEL))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(div().flex_1().child(format!("Render · {}", job.label)))
                                .child(format!(
                                    "{} / {} frames{}",
                                    job.progress,
                                    job.total,
                                    if exporting && job.progress == job.total {
                                        " · Finalizing…"
                                    } else {
                                        ""
                                    }
                                ))
                                .child(
                                    ui::text_button(
                                        "render-job-action",
                                        if exporting {
                                            "Cancel render"
                                        } else {
                                            "Dismiss"
                                        },
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.dispatch(
                                                if exporting {
                                                    Action::CancelExport
                                                } else {
                                                    Action::DismissRender
                                                },
                                                window,
                                                cx,
                                            )
                                        },
                                    )),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(ui::MUTED))
                                .max_h(px(32.0))
                                .overflow_hidden()
                                .child(job.message),
                        ),
                )
            })
            .child(
                div()
                    .h(px(23.0))
                    .flex_none()
                    .px_3()
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(rgb(ui::BORDER))
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child(div().flex_1().overflow_hidden().child(status))
                    .child("Libre Effects  •  2D compositor"),
            );

        if let Some(menu) = self.menu {
            let items: Vec<(&str, &str, Option<Action>)> = match menu {
                "File" => vec![
                    (
                        "Render work area — MP4…",
                        "",
                        Some(Action::ExportVideo(crate::video_export::VideoPreset::H264)),
                    ),
                    (
                        "Render work area — MOV with alpha…",
                        "",
                        Some(Action::ExportVideo(
                            crate::video_export::VideoPreset::ProResAlpha,
                        )),
                    ),
                    ("New project", "Ctrl+N", Some(Action::New)),
                    ("Open project…", "Ctrl+O", Some(Action::Open)),
                    ("Save", "Ctrl+S", Some(Action::Save)),
                    ("Save as…", "Ctrl+Shift+S", Some(Action::SaveAs)),
                    ("Import image…", "Ctrl+I", Some(Action::ImportImage)),
                    ("Import video…", "Ctrl+Shift+I", Some(Action::ImportVideo)),
                    ("Relink selected video…", "", Some(Action::RelinkVideo)),
                    ("Refresh footage", "", Some(Action::RefreshFootage)),
                    (
                        "Export current frame (PNG, alpha)…",
                        "",
                        Some(Action::ExportFrame),
                    ),
                    (
                        "Export current frame (PNG, background)…",
                        "",
                        Some(Action::ExportFrameBackground),
                    ),
                    (
                        "Render work area (PNG sequence, alpha)…",
                        "",
                        Some(Action::ExportSequence),
                    ),
                    (
                        "Render work area (PNG sequence, background)…",
                        "",
                        Some(Action::ExportSequenceBackground),
                    ),
                    ("Cancel render", "", Some(Action::CancelExport)),
                ],
                "Edit" => vec![
                    ("Copy selection", "Ctrl+C", Some(Action::CopySelection)),
                    ("Cut selection", "Ctrl+X", Some(Action::CutSelection)),
                    ("Paste", "Ctrl+V", Some(Action::PasteSelection)),
                    ("Copy layers", "", Some(Action::CopyLayers)),
                    ("Paste layers", "", Some(Action::PasteLayers)),
                    ("Undo", "Ctrl+Z", Some(Action::Undo)),
                    ("Redo", "Ctrl+Shift+Z", Some(Action::Redo)),
                    (
                        "Duplicate layer",
                        "Ctrl+D",
                        selected.map(|_| Action::DuplicateSelection),
                    ),
                    ("Delete selection", "Delete", Some(Action::DeleteSelection)),
                    (
                        "Split layers",
                        "Ctrl+Shift+D",
                        selected.map(|_| Action::SplitSelection),
                    ),
                ],
                "Layer" => vec![
                    (
                        "Pre-compose selection",
                        "Ctrl+Shift+C",
                        selected.map(|_| Action::PrecomposeSelection),
                    ),
                    (
                        "Trim In to playhead",
                        "Alt+[",
                        selected.map(|_| Action::TrimSelection(true)),
                    ),
                    (
                        "Trim Out to playhead",
                        "Alt+]",
                        selected.map(|_| Action::TrimSelection(false)),
                    ),
                    ("New text", "", Some(Action::AddText)),
                    ("New null object", "", Some(Action::Edit(Command::AddNull))),
                    (
                        "Solo selected layers",
                        "",
                        selected.map(|_| {
                            Action::ToggleSelectedSwitch(libre_effects_core::LayerSwitch::Solo)
                        }),
                    ),
                    (
                        "Shy selected layers",
                        "",
                        selected.map(|_| {
                            Action::ToggleSelectedSwitch(libre_effects_core::LayerSwitch::Shy)
                        }),
                    ),
                    (
                        "Guide selected layers",
                        "",
                        selected.map(|_| {
                            Action::ToggleSelectedSwitch(libre_effects_core::LayerSwitch::Guide)
                        }),
                    ),
                    (
                        "New background solid",
                        "",
                        Some(Action::Edit(Command::AddBackgroundSolid)),
                    ),
                    (
                        "New rectangle",
                        "Ctrl+Y",
                        Some(Action::Edit(Command::AddRectangle)),
                    ),
                    (
                        "Duplicate layer",
                        "Ctrl+D",
                        selected.map(|_| Action::DuplicateSelection),
                    ),
                    (
                        "Toggle visibility",
                        "",
                        selected.map(|id| Action::Edit(Command::ToggleVisible(id))),
                    ),
                    (
                        "Toggle lock",
                        "",
                        selected.map(|id| Action::Edit(Command::ToggleLocked(id))),
                    ),
                ],
                "Effect" => libre_effects_core::EffectKind::ALL
                    .into_iter()
                    .map(|kind| {
                        (
                            kind.label(),
                            "",
                            state
                                .editor
                                .selected_layer()
                                .filter(|l| {
                                    !l.locked()
                                        && !matches!(l.content(), libre_effects_core::Content::Null)
                                })
                                .map(|l| {
                                    Action::Edit(Command::Effect {
                                        id: l.id(),
                                        edit: libre_effects_core::EffectEdit::Add(kind),
                                    })
                                }),
                        )
                    })
                    .collect(),
                "Animation" => vec![
                    ("Toggle Graph Editor", "Shift+F3", Some(Action::ToggleGraph)),
                    ("Previous keyframe", "J", Some(Action::PreviousKey)),
                    ("Next keyframe", "K", Some(Action::NextKey)),
                    (
                        "Reveal animated properties",
                        "U",
                        Some(Action::Filter(Some(PropertyFilter::Animated))),
                    ),
                    ("Reveal all properties", "", Some(Action::Filter(None))),
                ],
                "View" => vec![
                    ("Fit composition", "", Some(Action::FitPreview)),
                    ("Zoom in", "", Some(Action::ZoomPreview(2.0))),
                    ("Zoom out", "", Some(Action::ZoomPreview(0.5))),
                    ("Transparency grid", "", Some(Action::Checkerboard)),
                ],
                _ => Vec::new(),
            };
            let mut dropdown = div()
                .id("main-menu")
                .absolute()
                .top(px(27.0))
                .left(px(match menu {
                    "File" => 0.0,
                    "Edit" => 40.0,
                    "Composition" => 78.0,
                    "Layer" => 180.0,
                    "Effect" => 224.0,
                    "Animation" => 271.0,
                    "View" => 349.0,
                    "Window" => 391.0,
                    _ => 460.0,
                }))
                .w(px(285.0))
                .p_1()
                .bg(rgb(0x2b2b2b))
                .border_1()
                .border_color(rgb(0x4a4a4a))
                .shadow_lg()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.menu = None;
                    cx.notify();
                }));
            for (index, (label, shortcut, action)) in items.into_iter().enumerate() {
                dropdown = dropdown.child(
                    ui::text_button(("menu-action", index), "")
                        .w_full()
                        .justify_between()
                        .when(action.is_none(), |s| s.opacity(0.4))
                        .child(label.to_string())
                        .child(div().text_color(rgb(ui::MUTED)).child(shortcut.to_string()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.menu = None;
                            if let Some(action) = &action {
                                this.dispatch(action.clone(), window, cx);
                            }
                            cx.notify();
                        })),
                );
            }
            if menu == "Composition" {
                for (id, label, command) in [
                    (
                        "composition-new",
                        "New composition",
                        Command::NewComposition,
                    ),
                    (
                        "composition-duplicate",
                        "Duplicate composition",
                        Command::DuplicateComposition,
                    ),
                    (
                        "composition-delete",
                        "Delete composition",
                        Command::DeleteComposition,
                    ),
                ] {
                    dropdown = dropdown.child(ui::text_button(id, label).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.menu = None;
                            this.dispatch(Action::Edit(command.clone()), window, cx);
                            cx.notify();
                        },
                    )));
                }
                dropdown = dropdown.child(
                    ui::text_button("composition-settings", "Composition settings…    Ctrl+K")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_settings(window, cx)),
                        ),
                );
            }
            if menu == "Window" {
                dropdown = dropdown.child(
                    ui::text_button("workspace-reset-menu", "Reset default workspace").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.menu = None;
                            this.reset_layout(cx);
                            cx.notify();
                        }),
                    ),
                );
            }
            if menu == "Help" {
                dropdown = dropdown.child(
                    ui::text_button("keyboard-help", "Keyboard shortcuts").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.menu = None;
                            this.help = true;
                            cx.notify();
                        },
                    )),
                );
            }
            root = root.child(gpui::deferred(dropdown).with_priority(2));
        }
        if self.settings || self.help {
            let mut dialog = div()
                .id("settings-help-dialog")
                .max_h(px(640.0))
                .overflow_y_scroll()
                .w(px(460.0))
                .p_5()
                .flex()
                .flex_col()
                .gap_3()
                .bg(rgb(0x262626))
                .border_1()
                .border_color(rgb(0x555555))
                .shadow_lg()
                .child(div().text_size(px(15.0)).text_color(rgb(0xffffff)).child(
                    if self.settings {
                        "Composition Settings"
                    } else {
                        "Keyboard shortcuts"
                    },
                ));
            if self.settings {
                for (index, label) in [
                    "Composition name",
                    "Width (px)",
                    "Height (px)",
                    "Frame rate (fps)",
                    "Duration (frames)",
                    "Background (RGB)",
                ]
                .into_iter()
                .enumerate()
                {
                    dialog = dialog.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().w(px(150.0)).child(label))
                            .child(div().flex_1().child(self.fields[index].clone())),
                    );
                }
                let color = ui::parse_hex_color(self.fields[5].read(cx).value()).ok();
                let mut palette = div().flex().items_center().gap_2().child(
                    div()
                        .w(px(24.0))
                        .h(px(24.0))
                        .border_1()
                        .border_color(rgb(ui::MUTED))
                        .bg(rgb(color.unwrap_or(0)))
                        .child(if color.is_some() { "" } else { "?" }),
                );
                for (label, color) in [
                    ("Black", 0x000000),
                    ("White", 0xffffff),
                    ("Slate", 0x26384a),
                    ("Navy", 0x102040),
                ] {
                    palette = palette.child(
                        ui::text_button(
                            gpui::SharedString::from(format!("background-{label}")),
                            label,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                window.focus(&this.focus);
                                this.fields[5].update(cx, |field, _| {
                                    field.sync(
                                        "composition-settings".into(),
                                        format!("#{color:06X}"),
                                        window,
                                    )
                                });
                                this.settings_error.clear();
                                cx.notify();
                            },
                        )),
                    );
                }
                dialog = dialog.child(palette).child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED))
                    .child("Used in the preview and MP4. Transparency grid and alpha exports keep transparency."));
                dialog = dialog
                    .child(
                        div()
                            .text_color(rgb(0xffa080))
                            .child(self.settings_error.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                ui::text_button("apply-settings", "OK")
                                    .bg(rgb(0x175c99))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.apply_settings(window, cx)
                                    })),
                            )
                            .child(ui::text_button("cancel-settings", "Cancel").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.settings = false;
                                    window.focus(&this.focus);
                                    cx.notify();
                                }),
                            )),
                    );
            } else {
                for line in [
                    "V / H / W / Y — Selection / Hand / Rotation / Anchor",
                    "Ctrl+Y — New rectangle    Ctrl+D — Duplicate",
                    "Ctrl+Shift+D — Split layers at playhead",
                    "Alt+[ / Alt+] — Trim In / Out to playhead",
                    "Arrow keys — Move selected layers 1 px (Shift: 10 px)",
                    "Drag handles — Scale    Shift — Proportional scale / 15° rotation",
                    "Esc — Cancel canvas drag",
                    "Ctrl+Z / Ctrl+Shift+Z — Undo / Redo",
                    "Ctrl+S / Ctrl+Shift+S — Save / Save as",
                    "Ctrl+I — Import image    Ctrl+C / Ctrl+X / Ctrl+V — Copy / Cut / Paste selection",
                    "Ctrl / Shift click — Toggle / Range select layers",
                    "Drag empty time area — Box select keys or layers",
                    "Drag a number — Scrub value (Shift: faster, Alt: finer)",
                    "Ctrl+K — Composition settings",
                    "Space — Play / Pause    Home / End — Seek",
                    "Page Up / Down — Step frame (Shift: 10 frames)",
                    "P / A / S / R / T — Reveal transform property",
                    "U — Animated properties    J / K — Previous / Next key",
                    "Ctrl+Shift+C — Pre-compose selected layers",
                    "Shift+F3 — Graph Editor    F9 — Ease selected graph segment",
                    "B / N — Work area start / end    + / − — Timeline zoom",
                    "Enter — Commit field    Escape — Cancel field",
                    "Drag time ruler to scrub; drag diamonds to move keys.",
                    "Double-click panel dividers to restore their size.",
                ] {
                    dialog = dialog.child(line);
                }
                dialog = dialog.child(ui::text_button("close-help", "Close").on_click(
                    cx.listener(|this, _, window, cx| {
                        this.help = false;
                        window.focus(&this.focus);
                        cx.notify();
                    }),
                ));
            }
            root = root.child(
                gpui::deferred(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(gpui::rgba(0x00000080))
                        .occlude()
                        .child(dialog),
                )
                .with_priority(3),
            );
        }
        let recovering = self.state.read(cx).recovery.is_some();
        let modal = self.closing || self.pending_document.is_some() || recovering;
        if modal && !self.modal_active {
            window.focus(&self.focus);
        }
        self.modal_active = modal;
        if modal {
            let mut dialog = div()
                .w(px(450.0))
                .p_5()
                .flex()
                .flex_col()
                .gap_3()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(ui::BLUE))
                .child(if recovering {
                    "An autosaved project is available."
                } else if self.closing {
                    "Save changes before closing?"
                } else {
                    "Save changes before switching projects?"
                });
            if recovering {
                let state = self.state.read(cx);
                dialog = dialog
                    .child(state.recovery.as_ref().unwrap().label.clone())
                    .child(format!(
                        "{} recoverable checkpoint(s)",
                        state.recovery_pending.len() + 1
                    ));
                for (label, restore) in [("Restore autosave", true), ("Discard autosave", false)] {
                    dialog = dialog.child(ui::text_button(label, label).on_click(cx.listener(
                        move |this, _, _, cx| this.state.update(cx, |s, cx| s.recover(restore, cx)),
                    )));
                }
                dialog = dialog
                    .child(
                        ui::text_button("next-recovery", "Next checkpoint").on_click(cx.listener(
                            |this, _, _, cx| this.state.update(cx, |s, cx| s.next_recovery(cx)),
                        )),
                    )
                    .child(
                        ui::text_button("keep-recovery", "Keep backups and start new").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |s, cx| s.keep_recoveries(cx))
                            }),
                        ),
                    );
            } else {
                dialog = dialog
                    .child(ui::text_button("close-save", "Save and continue").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.pending_save = this.pending_document.is_some();
                            this.state.update(cx, |s, cx| {
                                s.close_after_save = this.closing;
                                s.dispatch(&Action::Save, window, cx);
                            });
                        }),
                    ))
                    .child(
                        ui::text_button("close-discard", "Discard changes").on_click(cx.listener(
                            |this, _, window, cx| {
                                if this.state.read(cx).saving {
                                    return;
                                }
                                if this.closing {
                                    this.state.read(cx).clear_recovery();
                                    window.remove_window();
                                } else if let Some(action) = this.pending_document.take() {
                                    this.pending_save = false;
                                    this.state
                                        .update(cx, |s, cx| s.dispatch(&action, window, cx));
                                }
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        ui::text_button("close-cancel", "Cancel").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.closing = false;
                                this.pending_document = None;
                                this.pending_save = false;
                                this.state.update(cx, |s, _| s.close_after_save = false);
                                cx.notify();
                            },
                        )),
                    );
            }
            root = root.child(
                gpui::deferred(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(gpui::rgba(0x00000090))
                        .occlude()
                        .child(dialog),
                )
                .with_priority(5),
            );
        }
        root
    }
}
