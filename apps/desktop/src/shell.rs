use crate::{
    components::{Orientation, ResizablePanelGroup, TextField},
    editor::{Action, EditorState, PropertyFilter, Tool},
    panels::{Align, Browser, Preview, Sidebar, Timeline},
    ui,
};
use gpui::{Context, Entity, FocusHandle, KeyDownEvent, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, FrameRate};
#[path = "shell_media.rs"]
mod media;

pub(crate) struct Shell {
    state: Entity<EditorState>,
    color_picker: Entity<crate::panels::color_picker::ColorPicker>,
    layout: Entity<ResizablePanelGroup>,
    middle: Entity<ResizablePanelGroup>,
    upper: Entity<ResizablePanelGroup>,
    right: Entity<ResizablePanelGroup>,
    focus: FocusHandle,
    initialized: bool,
    menu: Option<&'static str>,
    settings: bool,
    settings_new: bool,
    settings_error: String,
    fields: Vec<Entity<TextField>>,
    help: bool,
    closing: bool,
    pending_document: Option<Action>,
    pending_save: bool,
    modal_active: bool,
    replacing: bool,
}

impl Shell {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| EditorState::default());
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let color_picker =
            cx.new(|cx| crate::panels::color_picker::ColorPicker::new(state.clone(), cx));
        let browser = cx.new(|cx| Browser::new(state.clone(), cx));
        let preview = cx.new(|cx| Preview::new(state.clone(), cx));
        let sidebar = cx.new(|cx| Sidebar::new(state.clone(), cx));
        let align = cx.new(|cx| Align::new(state.clone(), cx));
        let timeline = cx.new(|cx| Timeline::new(state.clone(), cx));
        let render_dock =
            cx.new(|cx| crate::panels::render_queue::RenderDock::new(state.clone(), timeline, cx));
        let upper = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Horizontal, browser, preview)
                .initial_fraction(0.20)
                .minimum_fraction(0.12)
        });
        let middle = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Vertical, upper.clone(), render_dock)
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
        let fields: Vec<_> = (0..7)
            .map(|_| cx.new(|cx| TextField::new(cx, |_, _, _| {})))
            .collect();
        for (index, panel) in [&layout, &upper, &middle, &right].into_iter().enumerate() {
            let state = state.clone();
            cx.observe(panel, move |_, panel, cx| {
                let fraction = panel.read(cx).fraction();
                if state.read(cx).workspace.fractions[index] != fraction {
                    state.update(cx, |s, cx| {
                        s.workspace.fractions[index] = fraction;
                        cx.notify();
                    });
                }
            })
            .detach();
        }
        cx.observe(&fields[5], |_, _, cx| cx.notify()).detach();
        Self {
            state,
            color_picker,
            layout,
            middle,
            upper,
            right,
            focus: cx.focus_handle(),
            initialized: false,
            menu: None,
            settings: false,
            settings_new: false,
            settings_error: String::new(),
            fields,
            help: false,
            closing: false,
            pending_document: None,
            pending_save: false,
            modal_active: false,
            replacing: false,
        }
    }
    pub(crate) fn replace_instance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        TextField::commit_active(window, cx);
        window.focus(&self.focus);
        self.replacing = true;
        self.state.update(cx, |s, cx| s.prepare_replacement(cx));
        cx.notify();
    }
    fn dispatch(&mut self, mut action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).colors.session.is_some() {
            return;
        }
        if self.state.read(cx).queue_open && matches!(action, Action::Undo | Action::Redo) {
            action = Action::Queue(if matches!(action, Action::Redo) {
                crate::editor::queue::QueueAction::Redo
            } else {
                crate::editor::queue::QueueAction::Undo
            });
        }
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
        self.state.update(cx, |s, cx| {
            s.workspace = Default::default();
            s.effect_controls_open = false;
            s.snapping = true;
            cx.notify();
        });
        self.layout.update(cx, |p, cx| p.reset(cx));
        self.upper.update(cx, |p, cx| p.reset(cx));
        self.middle.update(cx, |p, cx| p.reset(cx));
        self.right.update(cx, |p, cx| p.reset(cx));
    }
    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).colors.session.is_some() {
            return;
        }
        self.settings_new = false;
        let comp = self.state.read(cx).editor.project().composition();
        let values = [
            comp.name().to_string(),
            comp.width().to_string(),
            comp.height().to_string(),
            comp.fps().to_string(),
            comp.duration().to_string(),
            format!("#{:06X}", comp.background_color()),
            comp.fps().timecode(u64::from(comp.display_start())),
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
    fn new_composition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).colors.session.is_some() {
            return;
        }
        self.open_settings(window, cx);
        self.settings_new = true;
        let name = format!(
            "Composition {:02}",
            if self.state.read(cx).welcome() {
                1
            } else {
                self.state.read(cx).editor.project().compositions().len() + 1
            }
        );
        self.fields[0].update(cx, |f, _| f.sync("new-composition".into(), name, window));
    }
    fn apply_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.fields[0].read(cx).value().to_string();
        let parsed = (|| -> Result<_, String> {
            let width = self.fields[1]
                .read(cx)
                .value()
                .trim()
                .parse::<u32>()
                .map_err(|_| "Width must be a whole number")?;
            let height = self.fields[2]
                .read(cx)
                .value()
                .trim()
                .parse::<u32>()
                .map_err(|_| "Height must be a whole number")?;
            let fps: FrameRate = self.fields[3].read(cx).value().parse()?;
            let duration = fps.parse_duration(self.fields[4].read(cx).value())?;
            let display_start = fps.parse_timecode(self.fields[6].read(cx).value())?;
            if display_start >= fps.nominal() * 86_400 {
                return Err("Start timecode must be before 24:00:00:00".into());
            }
            if duration == 0 || duration > fps.max_duration() {
                return Err("Duration must be at least one frame and at most 24 hours".into());
            }
            Ok((width, height, fps, duration, display_start))
        })();
        let (width, height, fps, duration, display_start) = match parsed {
            Ok(values) => values,
            Err(error) => {
                self.settings_error = error;
                cx.notify();
                return;
            }
        };
        let background = match ui::parse_hex_color(self.fields[5].read(cx).value()) {
            Ok(color) => color,
            Err(error) => {
                self.settings_error = error.into();
                cx.notify();
                return;
            }
        };
        let mut commands = vec![
            Command::ConfigureCompositionRate {
                name,
                width,
                height,
                fps,
                duration,
                display_start,
            },
            Command::SetCompositionBackground(background),
        ];
        if self.settings_new && !self.state.read(cx).welcome() {
            commands.insert(0, Command::NewComposition);
        }
        self.dispatch(Action::Edit(Command::Batch(commands)), window, cx);
        let status = self.state.read(cx).status.clone();
        if status.starts_with("Edited") {
            self.state.update(cx, |s, cx| {
                s.composition_started = true;
                cx.notify();
            });
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
        if self.state.read(cx).colors.session.is_some() {
            if key == "escape" {
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::CancelColor, window, cx));
                window.focus(&self.focus);
            }
            cx.stop_propagation();
            return;
        }
        if key == "escape" {
            cx.stop_active_drag(window);
            self.closing = false;
            self.pending_document = None;
            self.pending_save = false;
            self.state.update(cx, |s, _| {
                s.close_after_save = false;
                s.marker_selection = None;
                s.media_open = false;
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
            || self.state.read(cx).media_open
            || self.closing
            || self.pending_document.is_some()
            || self.state.read(cx).recovery.is_some()
        {
            return;
        }
        let action = if m.control {
            match key {
                "n" if m.alt => Some(Action::New),
                "n" => {
                    self.new_composition(window, cx);
                    None
                }
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
                "m" => Some(Action::Queue(crate::editor::queue::QueueAction::Add)),
                "y" => Some(Action::Edit(if m.alt {
                    Command::AddAdjustment
                } else {
                    Command::AddSolid
                })),
                "d" => Some(if m.shift {
                    Action::SplitSelection
                } else {
                    Action::DuplicateSelection
                }),
                "r" => Some(Action::ViewerOption(
                    crate::viewer_tools::ViewOption::Rulers,
                )),
                "t" if m.alt => Some(Action::ToggleTimeRemap),
                "t" => Some(Action::SetTool(Tool::Text)),
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
                "z" => Some(Action::SetTool(Tool::Zoom)),
                "q" => {
                    let all = libre_effects_core::ShapeKind::ALL;
                    let next = match self.state.read(cx).tool {
                        Tool::Shape(kind) => {
                            all[(all.iter().position(|k| *k == kind).unwrap() + 1) % all.len()]
                        }
                        _ => all[0],
                    };
                    Some(Action::SetTool(Tool::Shape(next)))
                }
                "g" => Some(Action::SetTool(Tool::Pen)),
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
        if let Some(color) = self
            .state
            .update(cx, |s, _| s.colors.background_result.take())
        {
            self.fields[5].update(cx, |f, _| {
                f.sync(
                    "composition-settings".into(),
                    format!("#{color:06X}"),
                    window,
                )
            });
        }

        if self.state.read(cx).new_composition_requested {
            self.state
                .update(cx, |s, _| s.new_composition_requested = false);
            self.new_composition(window, cx);
        }
        let fractions = self.state.read(cx).workspace.fractions;
        for (index, panel) in [&self.layout, &self.upper, &self.middle, &self.right]
            .into_iter()
            .enumerate()
        {
            panel.update(cx, |p, cx| p.set_fraction(fractions[index], cx));
        }
        if !self.initialized {
            self.state.update(cx, |s, cx| {
                s.start_recovery(cx);
                s.load_queue(cx);
                s.load_presets(cx);
            });
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
                        s.state.update(cx, |state,cx|state.finish_text(true,cx));
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
        if self.replacing
            && !state.saving
            && !state.exporting
            && !state.collecting
            && !state.queue_busy
            && !state.importing_video
        {
            match state.preserve_replacement() {
                Ok(()) => window.remove_window(),
                Err(error) => {
                    self.replacing = false;
                    self.state.update(cx, |s, cx| {
                        s.status = format!("Cannot replace editor: {error}");
                        cx.notify();
                    });
                }
            }
        }
        let state = self.state.read(cx);
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
                        "cursor",
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
                    .child(ui::action_tool("zoom-tool", "magnifier", "Zoom tool (Z) · Alt to zoom out", &self.state, Action::SetTool(Tool::Zoom), tool == Tool::Zoom))
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
                    .child(ui::action_tool("shape-tool", match tool { Tool::Shape(libre_effects_core::ShapeKind::Ellipse) => "circle", Tool::Shape(libre_effects_core::ShapeKind::Star) => "star", Tool::Shape(libre_effects_core::ShapeKind::Polygon) => "triangle-up", _ => "square" }, "Shape tool (Q cycles shapes) · Drag to draw · Shift constrains · Alt draws from center", &self.state, Action::SetTool(match tool {Tool::Shape(_) => tool, _ => Tool::Shape(libre_effects_core::ShapeKind::Rectangle)}), matches!(tool, Tool::Shape(_))))
                    .child(ui::text_button("shape-menu", "▾").on_click(cx.listener(|this, _, window, cx| {window.focus(&this.focus); this.menu = if this.menu == Some("Shape") {None} else {Some("Shape")}; cx.notify();})))
                    .child(ui::action_tool("pen-tool", "pen", "Pen (G) · Click vertices, drag curves · Close at first point / Enter · Alt converts corners or breaks handles · Ctrl draws a mask on a shape", &self.state, Action::SetTool(Tool::Pen), tool == Tool::Pen))
                    .child(ui::action_tool("text-tool", "text", "Text tool (Ctrl+T) · Click point text · Drag a paragraph box", &self.state, Action::SetTool(Tool::Text), tool == Tool::Text))
                    .child(div().mx_2().w(px(1.0)).h(px(20.0)).bg(rgb(0x414141)))
                    .child(ui::text_button("toolbar-snapping", if self.state.read(cx).snapping {"☑ Snapping"} else {"☐ Snapping"}).on_click(cx.listener(|this,_,window,cx| {let _ = window; this.state.update(cx, |s,cx| {s.snapping = !s.snapping; cx.notify();});})))
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
                                .child(
                                    crate::audio_mix::progress_label(job.progress).unwrap_or_else(
                                        || {
                                            format!(
                                                "{} / {} frames{}",
                                                job.progress,
                                                job.total,
                                                if exporting && job.progress == job.total {
                                                    " · Finalizing…"
                                                } else {
                                                    ""
                                                }
                                            )
                                        },
                                    ),
                                )
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
                    ("New project", "Ctrl+Alt+N", Some(Action::New)),
                    ("Open project…", "Ctrl+O", Some(Action::Open)),
                    ("Save", "Ctrl+S", Some(Action::Save)),
                    ("Save as…", "Ctrl+Shift+S", Some(Action::SaveAs)),
                    ("Collect project files…", "", Some(Action::CollectFiles)),
                    (
                        "Cancel file collection",
                        "",
                        self.state
                            .read(cx)
                            .collecting
                            .then_some(Action::CancelCollection),
                    ),
                    ("Import footage…", "Ctrl+I", Some(Action::ImportImage)),
                    (
                        "Import image sequence…",
                        "",
                        Some(Action::ImportImageSequence),
                    ),
                    ("Import video…", "Ctrl+Shift+I", Some(Action::ImportVideo)),
                    ("Manage project media…", "", Some(Action::ManageMedia)),
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
                        if state
                            .editor
                            .selected_layer()
                            .is_some_and(|l| l.time_remap().is_some())
                        {
                            "Disable Time Remapping"
                        } else {
                            "Enable Time Remapping"
                        },
                        "Ctrl+Alt+T",
                        state
                            .editor
                            .selected_layer()
                            .filter(|l| l.can_time_remap() && !l.locked())
                            .map(|_| Action::ToggleTimeRemap),
                    ),
                    (
                        "Freeze frame with Time Remap",
                        "",
                        state
                            .editor
                            .selected_layer()
                            .filter(|l| l.can_time_remap() && !l.locked())
                            .map(|_| Action::FreezeTimeRemap),
                    ),
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
                    ("New solid", "Ctrl+Y", Some(Action::Edit(Command::AddSolid))),
                    (
                        "New adjustment layer",
                        "Ctrl+Alt+Y",
                        Some(Action::Edit(Command::AddAdjustment)),
                    ),
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
                        "",
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
                "Shape" => libre_effects_core::ShapeKind::ALL
                    .into_iter()
                    .map(|kind| (kind.label(), "Q", Some(Action::SetTool(Tool::Shape(kind)))))
                    .collect(),
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
                "Window" => vec![(
                    "Render Queue",
                    "",
                    Some(Action::Queue(crate::editor::queue::QueueAction::Show(true))),
                )],
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
                    (
                        "Rulers",
                        "Ctrl+R",
                        Some(Action::ViewerOption(
                            crate::viewer_tools::ViewOption::Rulers,
                        )),
                    ),
                    (
                        "Grid",
                        "",
                        Some(Action::ViewerOption(crate::viewer_tools::ViewOption::Grid)),
                    ),
                    (
                        "Guides",
                        "",
                        Some(Action::ViewerOption(
                            crate::viewer_tools::ViewOption::Guides,
                        )),
                    ),
                    (
                        "Title / Action Safe",
                        "",
                        Some(Action::ViewerOption(crate::viewer_tools::ViewOption::Safe)),
                    ),
                    (
                        "Snap to guides",
                        "",
                        Some(Action::ViewerOption(
                            crate::viewer_tools::ViewOption::SnapGuides,
                        )),
                    ),
                    (
                        "Snap to grid",
                        "",
                        Some(Action::ViewerOption(
                            crate::viewer_tools::ViewOption::SnapGrid,
                        )),
                    ),
                    (
                        "Lock guides",
                        "",
                        Some(Action::ViewerOption(
                            crate::viewer_tools::ViewOption::LockGuides,
                        )),
                    ),
                    ("Clear guides", "", Some(Action::ClearGuides)),
                ],
                _ => Vec::new(),
            };
            let mut dropdown = div()
                .id("main-menu")
                .absolute()
                .top(px(if menu == "Shape" { 63.0 } else { 27.0 }))
                .left(px(match menu {
                    "Shape" => 165.0,
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
                let label = if let Some(Action::ViewerOption(option)) = &action {
                    format!(
                        "{} {label}",
                        if self.state.read(cx).viewer.enabled(*option) {
                            "✓"
                        } else {
                            "  "
                        }
                    )
                } else {
                    label.to_string()
                };
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
                dropdown = dropdown.child(
                    ui::text_button("queue-comp", "Add to Render Queue    Ctrl+M").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.menu = None;
                            this.dispatch(
                                Action::Queue(crate::editor::queue::QueueAction::Add),
                                window,
                                cx,
                            )
                        }),
                    ),
                );
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
                            if matches!(command, Command::NewComposition) {
                                this.new_composition(window, cx);
                            } else {
                                this.dispatch(Action::Edit(command.clone()), window, cx);
                            }
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
        if (self.settings || self.help) && !self.state.read(cx).colors.picking() {
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
                        if self.settings_new {
                            "New Composition"
                        } else {
                            "Composition Settings"
                        }
                    } else {
                        "Keyboard shortcuts"
                    },
                ));
            if self.settings {
                let mut presets = div().flex().items_center().gap_2();
                for (label, width, height, fps) in [
                    ("HD 23.976", "1920", "1080", "24000/1001"),
                    ("HD 29.97", "1920", "1080", "30000/1001"),
                    ("UHD 25", "3840", "2160", "25"),
                ] {
                    presets = presets.child(
                        ui::text_button(
                            gpui::SharedString::from(format!("comp-preset-{label}")),
                            label,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                window.focus(&this.focus);
                                for (index, value) in [(1, width), (2, height), (3, fps)] {
                                    this.fields[index].update(cx, |field, _| {
                                        field.sync(
                                            "composition-settings".into(),
                                            value.into(),
                                            window,
                                        )
                                    });
                                }
                                this.settings_error.clear();
                                cx.notify();
                            },
                        )),
                    );
                }
                dialog = dialog.child(presets);
                for (index, label) in [
                    "Composition name",
                    "Width (px)",
                    "Height (px)",
                    "Frame rate (fps)",
                    "Duration",
                    "Background (RGB)",
                    "Start timecode (NDF)",
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
                let mut durations = div().flex().items_center().gap_2().child("Duration:");
                for value in ["5s", "10s", "30s", "60s"] {
                    durations = durations.child(
                        ui::text_button(
                            gpui::SharedString::from(format!("duration-{value}")),
                            value,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                window.focus(&this.focus);
                                this.fields[4].update(cx, |field, _| {
                                    field.sync("composition-settings".into(), value.into(), window)
                                });
                                this.settings_error.clear();
                                cx.notify();
                            },
                        )),
                    );
                }
                dialog = dialog.child(durations).child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED))
                    .child("FPS: 24, 23.976 or 24000/1001. Duration: 240f, 10s or HH:MM:SS:FF. Timecode is non-drop-frame; seconds use the exact rate. Changing FPS preserves keyframe numbers."));
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
                palette = palette.child(
                    ui::text_button("choose-background-color", "Choose…").on_click(cx.listener(
                        |this, _, w, cx| {
                            TextField::commit_active(w, cx);
                            if let Ok(color) = ui::parse_hex_color(this.fields[5].read(cx).value())
                            {
                                this.state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::OpenColor(
                                            crate::color_edit::Target::BackgroundDraft(color),
                                        ),
                                        w,
                                        cx,
                                    )
                                });
                            } else {
                                this.settings_error = "Enter a valid background color first".into();
                                cx.notify();
                            }
                        },
                    )),
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
                    "Ctrl+Y — New solid    Ctrl+Alt+Y — Adjustment    Ctrl+D — Duplicate",
                    "Ctrl+T — Text tool · Click text to edit · Ctrl+Enter finish · Esc cancel",
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
        if self.state.read(cx).media_open {
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
                        .child(self.media_dialog(cx)),
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
        if self.state.read(cx).colors.session.is_some() && !self.state.read(cx).colors.picking() {
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
                        .child(self.color_picker.clone()),
                )
                .with_priority(4),
            );
        }
        root
    }
}
