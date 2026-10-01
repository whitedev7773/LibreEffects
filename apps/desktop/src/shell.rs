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
            fields: (0..5)
                .map(|_| cx.new(|cx| TextField::new(cx, |_, _, _| {})))
                .collect(),
            help: false,
        }
    }
    fn dispatch(&self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
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
        let numbers: Result<Vec<u32>, _> = self.fields[1..]
            .iter()
            .map(|field| field.read(cx).value().parse::<u32>())
            .collect();
        let Ok(n) = numbers else {
            self.settings_error = "Enter whole numbers for size, frame rate and duration.".into();
            cx.notify();
            return;
        };
        self.dispatch(
            Action::Edit(Command::ConfigureComposition {
                name,
                width: n[0],
                height: n[1],
                fps: n[2],
                duration: n[3],
            }),
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
            self.menu = None;
            self.settings = false;
            self.help = false;
            window.focus(&self.focus);
            cx.notify();
            return;
        }
        if self.settings || self.help {
            return;
        }
        let selected = self.state.read(cx).editor.selected();
        let action = if m.control {
            match key {
                "n" => Some(Action::New),
                "o" => Some(Action::Open),
                "s" => Some(Action::SaveAs),
                "z" => Some(if m.shift { Action::Redo } else { Action::Undo }),
                "y" => Some(Action::Edit(Command::AddRectangle)),
                "d" => selected.map(|id| Action::Edit(Command::DuplicateLayer(id))),
                "k" => {
                    self.open_settings(window, cx);
                    None
                }
                _ => None,
            }
        } else if m.shift && key == "f3" {
            Some(Action::ToggleGraph)
        } else {
            match key {
                "space" => Some(Action::Play),
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
                "delete" | "backspace" => selected.map(|id| Action::Edit(Command::RemoveLayer(id))),
                "v" => Some(Action::SetTool(Tool::Select)),
                "h" => Some(Action::SetTool(Tool::Hand)),
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
            window.focus(&self.focus);
            cx.on_focus_lost(window, |this, window, _| window.focus(&this.focus))
                .detach();
            self.initialized = true;
        }
        let state = self.state.read(cx);
        let selected = state.editor.selected();
        let tool = state.tool;
        let status = state.status.clone();
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
                        "Save project as (Ctrl+S)",
                        &self.state,
                        Action::SaveAs,
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
                    ("New project", "Ctrl+N", Some(Action::New)),
                    ("Open project…", "Ctrl+O", Some(Action::Open)),
                    ("Save project as…", "Ctrl+S", Some(Action::SaveAs)),
                ],
                "Edit" => vec![
                    ("Undo", "Ctrl+Z", Some(Action::Undo)),
                    ("Redo", "Ctrl+Shift+Z", Some(Action::Redo)),
                    (
                        "Duplicate layer",
                        "Ctrl+D",
                        selected.map(|id| Action::Edit(Command::DuplicateLayer(id))),
                    ),
                    (
                        "Delete layer",
                        "Delete",
                        selected.map(|id| Action::Edit(Command::RemoveLayer(id))),
                    ),
                ],
                "Layer" => vec![
                    (
                        "New rectangle",
                        "Ctrl+Y",
                        Some(Action::Edit(Command::AddRectangle)),
                    ),
                    (
                        "Duplicate layer",
                        "Ctrl+D",
                        selected.map(|id| Action::Edit(Command::DuplicateLayer(id))),
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
                    "Animation" => 224.0,
                    "View" => 302.0,
                    "Window" => 344.0,
                    _ => 413.0,
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
                    "V / H — Selection / Hand tool",
                    "Ctrl+Y — New rectangle    Ctrl+D — Duplicate",
                    "Ctrl+Z / Ctrl+Shift+Z — Undo / Redo",
                    "Ctrl+K — Composition settings",
                    "Space — Play / Pause    Home / End — Seek",
                    "Page Up / Down — Step frame (Shift: 10 frames)",
                    "P / A / S / R / T — Reveal transform property",
                    "U — Animated properties    J / K — Previous / Next key",
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
        root
    }
}
