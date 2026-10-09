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
#[path = "shell_menu.rs"]
mod menu;
#[path = "shell_search.rs"]
mod search;
#[path = "shell_workspace.rs"]
mod workspace;

/// Shell-owned overlays that do not install their own initial focus handler.
/// Keep this separate from save/recovery rendering, and detect each newly opened
/// overlay so a later confirmation can safely cover an already-open Settings view.
#[derive(Clone, Copy, Default)]
struct FocuslessModals {
    settings: bool,
    help: bool,
    about: bool,
    media: bool,
    confirmation: bool,
}
impl FocuslessModals {
    fn opened_since(self, previous: Self) -> bool {
        (self.settings && !previous.settings)
            || (self.help && !previous.help)
            || (self.about && !previous.about)
            || (self.media && !previous.media)
            || (self.confirmation && !previous.confirmation)
    }
}

fn about_key_closes(key: &str, modified: bool) -> bool {
    !modified && matches!(key, "escape" | "enter" | "space")
}

/// An overlay keeps its explicit originating panel. Otherwise neutral Shell
/// focus uses Timeline safety, so closing a menu or pressing Escape cannot turn
/// a refused hidden-selection command into an unguarded keyboard command.
fn timeline_command_owned(
    captured_origin: Option<bool>,
    timeline_focused: bool,
    shell_focused: bool,
) -> bool {
    captured_origin.unwrap_or(timeline_focused || shell_focused)
}

pub(crate) struct Shell {
    state: Entity<EditorState>,
    timeline: Entity<Timeline>,
    script_ui: Entity<crate::panels::script_ui::ScriptUi>,
    ae_import: Entity<crate::panels::ae_import::AeImport>,
    ae_import_open: bool,
    ae_confirmation_active: bool,
    ae_exit_barrier: bool,
    confirmation_focus: [FocusHandle; 3],
    confirmation_keys: libre_effects_editor_model::automation_ui::ActivationKeys,
    color_picker: Entity<crate::panels::color_picker::ColorPicker>,
    gradient_editor: Entity<crate::panels::gradient_editor::GradientEditor>,
    vertex_editor: Entity<crate::panels::vertex_editor::VertexEditor>,
    expression_editor: Entity<crate::panels::expression_editor::ExpressionEditor>,
    expression_open: bool,
    expression_exit_barrier: bool,
    font_manager: Entity<crate::panels::font_manager::FontManager>,
    layout: Entity<ResizablePanelGroup>,
    middle: Entity<ResizablePanelGroup>,
    upper: Entity<ResizablePanelGroup>,
    right: Entity<ResizablePanelGroup>,
    focus: FocusHandle,
    initialized: bool,
    menu: Option<&'static str>,
    menu_cursor: Option<usize>,
    menu_return_focus: Option<FocusHandle>,
    menu_pointer_owner: Option<(
        gpui::MouseDownEvent,
        bool,
        Option<FocusHandle>,
        u64,
        bool,
        bool,
    )>,
    // Preserve the originating panel while a menu or Find command owns focus.
    // Some(false) is meaningful: explicit Graph/Inspector/canvas commands must
    // not inherit a Timeline filter merely because an overlay moved focus.
    timeline_command_origin: Option<bool>,
    menu_pointer_source_pending: bool,
    svg_import_entry: menu::SvgImportEntry,
    menu_scroll: gpui::ScrollHandle,
    search_open: bool,
    search_field: Entity<TextField>,
    search_cursor: Option<menu::Key>,
    search_query: String,
    search_scroll: gpui::ScrollHandle,
    search_return_focus: Option<FocusHandle>,
    workspace_library: crate::workspace_library::Library,
    workspace_selected: Option<String>,
    workspace_dialog: bool,
    workspace_name: Entity<TextField>,
    workspace_error: String,
    settings: bool,
    settings_new: bool,
    settings_error: String,
    fields: Vec<Entity<TextField>>,
    help: bool,
    about: bool,
    closing: bool,
    pending_document: Option<Action>,
    pending_save: bool,
    modal_active: FocuslessModals,
    replacing: bool,
    automation_open: bool,
}

impl Shell {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| EditorState::default());
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let script_ui = cx.new(|cx| crate::panels::script_ui::ScriptUi::new(state.clone(), cx));
        let ae_import = cx.new(|cx| crate::panels::ae_import::AeImport::new(state.clone(), cx));
        let color_picker =
            cx.new(|cx| crate::panels::color_picker::ColorPicker::new(state.clone(), cx));
        let gradient_editor =
            cx.new(|cx| crate::panels::gradient_editor::GradientEditor::new(state.clone(), cx));
        let vertex_editor =
            cx.new(|cx| crate::panels::vertex_editor::VertexEditor::new(state.clone(), cx));
        let expression_editor =
            cx.new(|cx| crate::panels::expression_editor::ExpressionEditor::new(state.clone(), cx));
        let browser = cx.new(|cx| Browser::new(state.clone(), cx));
        let font_manager =
            cx.new(|cx| crate::panels::font_manager::FontManager::new(state.clone(), cx));
        let preview = cx.new(|cx| Preview::new(state.clone(), cx));
        let sidebar = cx.new(|cx| Sidebar::new(state.clone(), cx));
        let align = cx.new(|cx| Align::new(state.clone(), cx));
        let timeline = cx.new(|cx| Timeline::new(state.clone(), cx));
        let render_dock = cx.new(|cx| {
            crate::panels::render_queue::RenderDock::new(state.clone(), timeline.clone(), cx)
        });
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
        let search_field = cx.new(|cx| TextField::new(cx, |_, _, _| {}));
        cx.observe(&search_field, |_, _, cx| cx.notify()).detach();
        cx.observe(&fields[5], |_, _, cx| cx.notify()).detach();
        Self {
            state,
            timeline,
            color_picker,
            gradient_editor,
            script_ui,
            ae_import,
            ae_import_open: false,
            ae_confirmation_active: false,
            ae_exit_barrier: false,
            confirmation_focus: std::array::from_fn(|_| cx.focus_handle()),
            confirmation_keys: crate::modal_keyboard::activation_keys(cx),
            vertex_editor,
            expression_editor,
            expression_open: false,
            expression_exit_barrier: false,
            font_manager,
            layout,
            middle,
            upper,
            right,
            focus: cx.focus_handle(),
            initialized: false,
            menu: None,
            menu_cursor: None,
            menu_return_focus: None,
            menu_pointer_owner: None,
            timeline_command_origin: None,
            menu_pointer_source_pending: false,
            svg_import_entry: Default::default(),
            menu_scroll: gpui::ScrollHandle::new(),
            search_open: false,
            search_field,
            search_cursor: None,
            search_query: String::new(),
            search_scroll: gpui::ScrollHandle::new(),
            search_return_focus: None,
            workspace_library: crate::workspace_library::Library::load(),
            workspace_selected: None,
            workspace_dialog: false,
            workspace_name: cx.new(|cx| TextField::new(cx, |_, _, _| {}).tab_stop()),
            workspace_error: String::new(),
            settings: false,
            settings_new: false,
            settings_error: String::new(),
            fields,
            help: false,
            about: false,
            closing: false,
            pending_document: None,
            pending_save: false,
            modal_active: FocuslessModals::default(),
            replacing: false,
            automation_open: false,
        }
    }
    pub(crate) fn replace_instance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.timeline_command_origin = None;
        self.state.update(cx, |s, _| {
            s.cancel_automation();
            s.cancel_expression_editor();
            s.cancel_ae_import();
            s.retire_colors_context();
            s.discard_vertex_editor();
        });
        TextField::commit_active(window, cx);
        window.focus(&self.focus);
        self.replacing = true;
        self.state.update(cx, |s, cx| s.prepare_replacement(cx));
        cx.notify();
    }
    fn dispatch(&mut self, mut action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).automation.is_some()
            || self.about
            || self.state.read(cx).colors.session.is_some()
            || self.state.read(cx).gradient_editor.is_some()
            || self.state.read(cx).vertex_editor.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
            return;
        }
        if let Action::OpenRecent(path) = &action
            && !self.state.read(cx).recent_projects.contains(path)
        {
            self.state.update(cx, |s, cx| {
                s.status =
                    "Recent project is no longer in the list; reopen File → Open recent".into();
                cx.notify();
            });
            return;
        }
        let timeline_owned = timeline_command_owned(
            self.timeline_command_origin,
            self.timeline.read(cx).owns_focus(window, cx),
            self.focus.is_focused(window),
        );
        if timeline_owned && self.timeline.read(cx).selection_action_blocked(&action, cx) {
            self.state.update(cx, |state, cx| {
                state.status = "Some targets are hidden in the Timeline. Clear the layer filters or select visible rows before editing.".into();
                cx.notify();
            });
            return;
        }
        if self.state.read(cx).queue_open && matches!(action, Action::Undo | Action::Redo) {
            action = Action::Queue(if matches!(action, Action::Redo) {
                crate::editor::queue::QueueAction::Redo
            } else {
                crate::editor::queue::QueueAction::Undo
            });
        }
        if matches!(
            action,
            Action::New | Action::Open | Action::OpenRecent(_) | Action::ApplyAeProject(_)
        ) {
            if self.state.read(cx).saving {
                self.state.update(cx, |s, cx| {
                    s.status = "Wait for the current save to finish.".into();
                    cx.notify();
                });
                return;
            }
            if self.state.read(cx).dirty() {
                self.state.update(cx, |s, _| s.retire_colors_context());
                self.pending_document = Some(action);
                self.pending_save = false;
                self.menu = None;
                self.timeline_command_origin = None;
                cx.notify();
                return;
            }
        }
        self.state
            .update(cx, |state, cx| state.dispatch(&action, window, cx));
    }
    fn reset_layout(&mut self, cx: &mut Context<Self>) {
        self.apply_workspace(crate::view_state::WorkspacePreset::Standard, cx);
    }
    fn apply_workspace(
        &mut self,
        preset: crate::view_state::WorkspacePreset,
        cx: &mut Context<Self>,
    ) {
        if self.state.read(cx).automation.is_some()
            || self.about
            || self.settings
            || self.help
            || self.closing
            || self.pending_document.is_some()
            || self.state.read(cx).colors.session.is_some()
            || self.state.read(cx).gradient_editor.is_some()
            || self.state.read(cx).vertex_editor.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
            return;
        }
        self.workspace_selected = None;
        self.state.update(cx, |s, cx| {
            let snapping = s.snapping;
            let align_to_selection = s.workspace.align_to_selection;
            s.workspace = crate::view_state::WorkspaceView::preset(preset);
            s.workspace.snapping = snapping;
            s.workspace.align_to_selection = align_to_selection;
            s.effect_controls_open = s.workspace.effect_controls_open;
            cx.notify();
        });
        cx.notify();
    }
    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).automation.is_some()
            || self.about
            || self.state.read(cx).colors.session.is_some()
            || self.state.read(cx).gradient_editor.is_some()
            || self.state.read(cx).vertex_editor.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
            return;
        }
        self.state.update(cx, |s, _| s.retire_colors_context());
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
        self.timeline_command_origin = None;
        self.state
            .update(cx, |state, _| state.shell_clipboard_blocked = false);
        // Ctrl+K/Ctrl+N can originate in the Composition. Remove its key target
        // immediately, before a queued Delete/Escape or the next render.
        cx.stop_active_drag(window);
        window.focus(&self.focus);
        cx.notify();
    }
    fn new_composition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).automation.is_some()
            || self.about
            || self.state.read(cx).colors.session.is_some()
            || self.state.read(cx).gradient_editor.is_some()
            || self.state.read(cx).vertex_editor.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
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
        if self.state.read(cx).automation.is_some()
            || self.about
            || self.state.read(cx).vertex_editor.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
            return;
        }
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
    fn clipboard_owner(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.state
            .read(cx)
            .contents_tree_focus
            .as_ref()
            .is_some_and(|focus| focus.contains_focused(window, cx))
            || TextField::active_has_focus(window, cx)
    }
    fn timeline_focus_origin(&self, window: &Window, cx: &Context<Self>) -> bool {
        timeline_command_owned(
            None,
            self.timeline.read(cx).owns_focus(window, cx),
            self.focus.is_focused(window),
        )
    }
    fn capture_clipboard_owner(&mut self, window: &Window, cx: &mut Context<Self>) {
        let blocked = self.clipboard_owner(window, cx);
        self.state
            .update(cx, |state, _| state.shell_clipboard_blocked = blocked);
    }
    fn menu_pointer_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_pointer_owner = None;
        if event.button == gpui::MouseButton::Left {
            let (blocked, focus, timeline_owned) = if self.menu.is_some() {
                (
                    self.state.read(cx).shell_clipboard_blocked,
                    self.menu_return_focus.clone(),
                    self.timeline_command_origin.unwrap_or(false),
                )
            } else {
                (
                    self.clipboard_owner(window, cx),
                    window.focused(cx),
                    self.timeline_focus_origin(window, cx),
                )
            };
            self.menu_pointer_owner = Some((
                event.clone(),
                blocked,
                focus,
                crate::color_edit::input_pointer_generation(window, cx),
                self.menu_pointer_source_pending,
                timeline_owned,
            ));
        }
    }
    fn click_menu(
        &mut self,
        name: &'static str,
        event: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receipt = self.menu_pointer_owner.take();
        if self.menu == Some(name) {
            self.close_menu(window, cx);
            return;
        }
        let origin = match (event, receipt) {
            (
                gpui::ClickEvent::Mouse(click),
                Some((down, blocked, focus, generation, pending, timeline_owned)),
            ) if generation == crate::color_edit::input_pointer_generation(window, cx)
                && down.position == click.down.position
                && down.button == click.down.button
                && down.modifiers == click.down.modifiers
                && down.click_count == click.down.click_count
                && down.first_mouse == click.down.first_mouse =>
            {
                Some((blocked, focus, pending, timeline_owned))
            }
            _ => None,
        };
        if !menu::clipboard_menu_origin_allowed(
            matches!(event, gpui::ClickEvent::Mouse(click) if !click.down.first_mouse),
            self.menu.is_some(),
            origin.is_some(),
        ) {
            return;
        }
        self.open_menu_owned(name, origin, window, cx);
    }
    fn open_menu(&mut self, name: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_pointer_owner = None;
        self.open_menu_owned(name, None, window, cx);
    }
    fn open_menu_owned(
        &mut self,
        name: &'static str,
        origin: Option<(bool, Option<FocusHandle>, bool, bool)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.read(cx).automation.is_some()
            || self.about
            || self.state.read(cx).vertex_editor.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
            return;
        }
        if self.menu.is_none() {
            if let Some((blocked, focus, pending, timeline_owned)) = origin {
                self.timeline_command_origin = Some(timeline_owned);
                self.svg_import_entry.begin(false, pending);
                self.state
                    .update(cx, |state, _| state.shell_clipboard_blocked = blocked);
                self.menu_return_focus = focus;
            } else {
                self.timeline_command_origin = Some(self.timeline_focus_origin(window, cx));
                self.svg_import_entry.begin(
                    false,
                    TextField::active_has_pending_source_input(cx)
                        || self.state.read(cx).text_session.is_some(),
                );
                self.capture_clipboard_owner(window, cx);
                self.menu_return_focus = window.focused(cx);
            }
        }
        TextField::commit_active(window, cx);
        self.state.update(cx, |s, cx| s.finish_text(true, cx));
        window.focus(&self.focus);
        self.menu = Some(name);
        self.menu_cursor = menu::initial(&menu::items(name, self.state.read(cx)), false);
        self.menu_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        cx.notify();
    }
    fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.timeline_command_origin = None;
        self.svg_import_entry.clear();
        self.menu_pointer_owner = None;
        self.state
            .update(cx, |state, _| state.shell_clipboard_blocked = false);
        self.menu = None;
        self.menu_cursor = None;
        if let Some(focus) = self.menu_return_focus.take() {
            window.focus(&focus);
        } else {
            window.focus(&self.focus);
        }
        cx.notify();
    }
    fn run_menu(&mut self, target: menu::Target, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_pointer_owner = None;
        if self.state.read(cx).automation.is_some()
            || self.about
            || self.state.read(cx).vertex_editor.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
            self.timeline_command_origin = None;
            return;
        }
        if let menu::Target::Menu(name) = target {
            self.open_menu(name, window, cx);
            return;
        }
        if matches!(target, menu::Target::Search) {
            self.open_search(window, cx);
            return;
        }
        self.menu = None;
        self.menu_cursor = None;
        self.menu_return_focus = None;
        if matches!(
            target,
            menu::Target::Settings
                | menu::Target::NewComposition
                | menu::Target::Help
                | menu::Target::About
                | menu::Target::Action(Action::New | Action::Open | Action::OpenRecent(_))
        ) {
            self.state.update(cx, |s, _| s.retire_colors_context());
        }
        if !self.svg_import_entry.finish(&target) {
            self.timeline_command_origin = None;
            self.state.update(cx, |s, cx| {
                s.shell_clipboard_blocked = false;
                s.status = "File operation not started: a source edit was pending when the menu opened; finish the edit and reopen File or Find command".into();
                cx.notify();
            });
            window.focus(&self.focus);
            cx.notify();
            return;
        }
        window.focus(&self.focus);
        match target {
            menu::Target::Action(action) => {
                self.dispatch(action, window, cx);
            }
            menu::Target::NewComposition => self.new_composition(window, cx),
            menu::Target::Settings => self.open_settings(window, cx),
            menu::Target::ResetWorkspace => self.reset_layout(cx),
            menu::Target::ManageWorkspaces => self.open_workspaces(window, cx),
            menu::Target::Workspace(preset) => self.apply_workspace(preset, cx),
            menu::Target::Help => self.help = true,
            menu::Target::About => self.open_about(window, cx),
            menu::Target::Search => self.open_search(window, cx),
            menu::Target::Menu(name) => self.open_menu(name, window, cx),
        }
        self.timeline_command_origin = None;
        self.state
            .update(cx, |state, _| state.shell_clipboard_blocked = false);
        cx.notify();
    }
    fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if state.automation.is_some()
            || self.settings
            || self.help
            || self.closing
            || self.pending_document.is_some()
            || state.media_open
            || state.fonts_open
            || state.recovery.is_some()
            || state.colors.session.is_some()
            || state.gradient_editor.is_some()
            || state.vertex_editor.is_some()
            || state.expression_editor.is_some()
        {
            return;
        }
        self.state.update(cx, |s, _| s.retire_colors_context());
        self.about = true;
        cx.stop_active_drag(window);
        window.focus(&self.focus);
        cx.notify();
    }
    fn close_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.about = false;
        window.focus(&self.focus);
        cx.notify();
    }
    fn ae_confirmation_choice(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.pending_document, Some(Action::ApplyAeProject(_))) {
            return;
        }
        match index {
            0 if !self.state.read(cx).saving => {
                self.pending_save = true;
                self.state
                    .update(cx, |state, cx| state.dispatch(&Action::Save, window, cx));
            }
            1 if !self.state.read(cx).saving => {
                self.pending_save = false;
                if let Some(action) = self.pending_document.take() {
                    self.state
                        .update(cx, |state, cx| state.dispatch(&action, window, cx));
                }
            }
            2 => {
                self.pending_document = None;
                self.pending_save = false;
                self.state
                    .update(cx, |state, _| state.cancel_ae_import_confirmation());
            }
            _ => {}
        }
        cx.notify();
    }
    fn menu_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.ae_import_open && self.state.read(cx).ae_import.is_none() {
            self.ae_exit_barrier = true;
        }
        let press = self
            .confirmation_keys
            .press_receipt(event.keystroke.key.as_str(), event.is_held);
        crate::modal_keyboard::record_press(event, press, window, cx);
        let activation = press.allowed();
        if libre_effects_editor_model::input_routing::suppress_modal_exit_key(
            self.ae_exit_barrier,
            self.state.read(cx).ae_import.is_some(),
            event.keystroke.key.as_str(),
            activation,
        ) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if self.state.read(cx).ae_import.is_none() {
            self.ae_import.update(cx, |panel, _| {
                panel.suppress_activation_key(event.keystroke.key.as_str(), event.is_held)
            });
        } else if matches!(self.pending_document, Some(Action::ApplyAeProject(_))) {
            self.ae_import.update(cx, |panel, _| {
                panel.suppress_activation_key(event.keystroke.key.as_str(), event.is_held)
            });
            let key = event.keystroke.key.as_str();
            let modifiers = event.keystroke.modifiers;
            if modifiers.alt && key == "f4" {
                return;
            }
            cx.stop_propagation();
            window.prevent_default();
            if modifiers.control || modifiers.alt || modifiers.platform {
                return;
            }
            if key == "tab" {
                let at = self
                    .confirmation_focus
                    .iter()
                    .position(|f| f.is_focused(window))
                    .unwrap_or(2);
                let next = if modifiers.shift {
                    (at + 2) % 3
                } else {
                    (at + 1) % 3
                };
                window.focus(&self.confirmation_focus[next]);
            } else if activation && !modifiers.shift {
                if key == "escape" {
                    self.ae_confirmation_choice(2, window, cx);
                } else if matches!(key, "enter" | "space") {
                    if let Some(at) = self
                        .confirmation_focus
                        .iter()
                        .position(|f| f.is_focused(window))
                    {
                        self.ae_confirmation_choice(at, window, cx);
                    }
                }
            }
            return;
        } else {
            if !self.ae_import.read(cx).contains_focus(window, cx) {
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        if self.expression_open && self.state.read(cx).expression_editor.is_none() {
            self.expression_exit_barrier = true;
        }
        if libre_effects_editor_model::input_routing::suppress_modal_exit_key(
            self.expression_exit_barrier,
            self.state.read(cx).expression_editor.is_some(),
            event.keystroke.key.as_str(),
            activation,
        ) {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if self.state.read(cx).expression_editor.is_some() {
            if !self.expression_editor.read(cx).contains_focus(window, cx) {
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        self.expression_editor.update(cx, |panel, _| {
            panel.suppress_activation_key(event.keystroke.key.as_str(), event.is_held)
        });
        if self.state.read(cx).automation.is_some() {
            if !self.script_ui.read(cx).contains_focus(window, cx) {
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        self.state.update(cx, |s, _| s.retire_pending_svg_import());
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        let state = self.state.read(cx);
        if self.about
            && !self.closing
            && self.pending_document.is_none()
            && state.recovery.is_none()
        {
            // Trap all editor keys and Tab before descendants can edit the canvas
            // or focus an occluded control. Keep the platform close shortcut.
            if m.alt && key == "f4" {
                return;
            }
            cx.stop_propagation();
            window.prevent_default();
            if about_key_closes(key, m.control || m.alt || m.shift || m.platform) {
                self.close_about(window, cx);
            }
            return;
        }
        if self.workspace_dialog
            || self.settings
            || self.help
            || self.about
            || self.closing
            || self.pending_document.is_some()
            || state.media_open
            || state.fonts_open
            || state.recovery.is_some()
            || state.colors.session.is_some()
            || state.gradient_editor.is_some()
            || state.vertex_editor.is_some()
            || state.expression_editor.is_some()
        {
            return;
        }
        if self.search_open {
            self.search_key(event, window, cx);
            return;
        }
        if key == "p"
            && m.control
            && m.shift
            && !m.alt
            && !m.platform
            && !TextField::is_composing(window, cx)
            && !state
                .text_session
                .as_ref()
                .is_some_and(|s| s.buffer.marked.is_some())
        {
            self.open_search(window, cx);
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if self.menu.is_none() {
            if key == "f10"
                && !m.control
                && !m.alt
                && !m.shift
                && !m.platform
                && !TextField::is_composing(window, cx)
                && !state
                    .text_session
                    .as_ref()
                    .is_some_and(|s| s.buffer.marked.is_some())
            {
                self.open_menu("File", window, cx);
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        if m.alt && key == "f4" {
            self.close_menu(window, cx);
            return;
        }
        // Capture before focused children or the editor can interpret arrows,
        // Space, Delete or shortcuts as document edits while a menu is open.
        cx.stop_propagation();
        window.prevent_default();
        let name = self.menu.unwrap();
        let items = menu::items(name, self.state.read(cx));
        match key {
            "escape" | "left" if name == "Open Recent" => self.open_menu("File", window, cx),
            "escape" | "f10" | "tab" => self.close_menu(window, cx),
            "right"
                if !event.is_held
                    && self
                        .menu_cursor
                        .and_then(|i| items.get(i))
                        .is_some_and(|i| matches!(i.target, Some(menu::Target::Menu(_)))) =>
            {
                if let Some(menu::Target::Menu(name)) = self
                    .menu_cursor
                    .and_then(|i| items.get(i))
                    .and_then(|i| i.target.clone())
                {
                    self.open_menu(name, window, cx);
                }
            }
            "left" | "right" if name != "Open Recent" => {
                self.open_menu(menu::adjacent(name, key == "right"), window, cx)
            }
            "up" | "down" => self.menu_cursor = menu::step(&items, self.menu_cursor, key == "down"),
            "home" | "end" => self.menu_cursor = menu::initial(&items, key == "end"),
            "enter" | "space" if !event.is_held => {
                if let Some(target) = self
                    .menu_cursor
                    .and_then(|i| items.get(i))
                    .and_then(|i| i.target.clone())
                {
                    self.run_menu(target, window, cx);
                }
            }
            _ if !m.control && !m.alt && !m.platform => {
                self.menu_cursor = menu::letter(&items, self.menu_cursor, key)
            }
            _ => {}
        }
        if let Some(index) = self.menu_cursor {
            self.menu_scroll.scroll_to_item(index);
        }
        cx.notify();
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).ae_import.is_some()
            && self.pending_document.is_none()
            && !self.closing
        {
            return;
        }
        if self.state.read(cx).automation.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
            return;
        }
        if self.search_open {
            cx.stop_propagation();
            return;
        }
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        // Focused text owns printable/dead/AltGr keys. Propagation must remain
        // enabled for the native input handler, without firing editor shortcuts.
        if (TextField::active_has_focus(window, cx) || self.state.read(cx).text_session.is_some())
            && libre_effects_editor_model::input_routing::native_text_key(
                key, m.control, m.platform, m.alt,
            )
        {
            return;
        }
        // Source and selection-formatting controls own a text draft. Unhandled
        // keys must not become layer deletion, nudging, creation or document
        // history merely because focus moved from the preview to Character.
        if self.state.read(cx).text_session.is_some()
            && !((m.control || m.platform) && matches!(key, "s" | "o" | "n"))
            && !(m.alt && key == "f4")
        {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if self.state.read(cx).colors.session.is_some()
            || self.state.read(cx).gradient_editor.is_some()
            || self.state.read(cx).vertex_editor.is_some()
            || self.state.read(cx).expression_editor.is_some()
        {
            if key == "escape" {
                self.state.update(cx, |s, cx| {
                    let action = if s.vertex_editor.is_some() {
                        Action::CancelVertex
                    } else if s.gradient_editor.is_some() {
                        Action::CancelGradient
                    } else {
                        Action::CancelColor
                    };
                    s.dispatch(&action, window, cx);
                });
                window.focus(&self.focus);
            }
            cx.stop_propagation();
            return;
        }
        if key == "escape" {
            self.menu_pointer_owner = None;
            self.timeline_command_origin = None;
            cx.stop_active_drag(window);
            self.closing = false;
            self.pending_document = None;
            self.pending_save = false;
            self.state.update(cx, |s, _| {
                s.cancel_ae_import_confirmation();
                s.close_after_save = false;
                s.marker_selection = None;
                s.media_open = false;
                s.fonts_open = false;
                s.shell_clipboard_blocked = false;
                s.retire_colors_clipboard();
                s.colors_key_owned.set(false);
            });
            self.menu = None;
            self.workspace_dialog = false;
            self.settings = false;
            self.help = false;
            self.about = false;
            window.focus(&self.focus);
            cx.notify();
            return;
        }
        if self.settings
            || self.help
            || self.about
            || self.state.read(cx).media_open
            || self.state.read(cx).fonts_open
            || self.closing
            || self.pending_document.is_some()
            || self.state.read(cx).recovery.is_some()
        {
            return;
        }
        if m.alt && m.shift && !m.control && key == "j" {
            self.timeline
                .update(cx, |timeline, cx| timeline.focus_time(window, cx));
            cx.stop_propagation();
            return;
        }
        if m.alt && !m.control && matches!(key, "left" | "right") {
            self.dispatch(
                Action::NavigateHistory(if key == "left" { -1 } else { 1 }),
                window,
                cx,
            );
            cx.stop_propagation();
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
        // Recovery may arrive after a background import has prepared its result.
        // Retire the lower-priority import instead of trapping focus in controls
        // hidden by the recovery prompt.
        if self.state.read(cx).recovery.is_some() && self.state.read(cx).ae_import.is_some() {
            if matches!(self.pending_document, Some(Action::ApplyAeProject(_))) {
                self.pending_document = None;
                self.pending_save = false;
            }
            self.state.update(cx, |state, _| state.cancel_ae_import());
        }
        self.state.update(cx, |state, _| {
            state.invalidate_expression_editor();
        });
        let expression_open = self.state.read(cx).expression_editor.is_some();
        if self.expression_open && !expression_open {
            self.expression_exit_barrier = self.confirmation_keys.has_pressed_activation();
            window.focus(&self.focus);
        }
        self.expression_open = expression_open;
        let ae_import_open = self.state.read(cx).ae_import.is_some();
        if self.ae_import_open && !ae_import_open {
            self.ae_exit_barrier = self.confirmation_keys.has_pressed_activation();
            window.focus(&self.focus);
        }
        self.ae_import_open = ae_import_open;
        if let Some(operation) = self
            .state
            .update(cx, |state, _| state.take_ae_import_apply_request())
        {
            self.dispatch(Action::ApplyAeProject(operation), window, cx);
        }
        let automation_open = self.state.read(cx).automation.is_some();
        if self.automation_open && !automation_open {
            window.focus(&self.focus);
        }
        self.automation_open = automation_open;
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
            let pointer_window = window.window_handle().window_id();
            cx.on_release(move |_, cx| {
                crate::color_edit::release_input_pointer(pointer_window, cx);
            })
            .detach();
            let weak = cx.entity().downgrade();
            window.on_window_should_close(cx, move |window, cx| {
                crate::color_edit::cancel_input_pointer(window, cx);
                let _ = weak.update(cx, |s, cx| {
                    s.state.update(cx, |state, cx| {
                        state.cancel_automation(); state.cancel_ae_import(); state.cancel_expression_editor();
                        state.retire_colors_context();
                        state.gradient_editor = None;
                        state.discard_vertex_editor();
                        cx.notify();
                    });
                });
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
                            s.state.read(cx).flush_recent_projects_on_close();
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
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    crate::color_edit::cancel_input_pointer(window, cx);
                    this.state.update(cx, |state, cx| {
                        state.discard_vertex_editor();
                        cx.notify();
                    });
                }
            })
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
                .unwrap_or("Untitled.lep".into())
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
                Ok(()) => {
                    state.flush_recent_projects_on_close();
                    window.remove_window();
                }
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
            state.flush_recent_projects_on_close();
            state.clear_recovery();
            window.remove_window();
        }
        let tool = state.tool;
        let status = state.status.clone();
        let video_job = state.video_job.clone();
        let exporting = state.exporting;
        let mut root = crate::color_edit::input_pointer_root(div().id("editor-workspace"), &self.state)
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                // Capture before any child outside-down/blur commits a field.
                this.menu_pointer_source_pending = TextField::active_has_pending_source_input(cx)
                    || this.state.read(cx).text_session.is_some();
                this.state.update(cx, |s, _| s.retire_pending_svg_import());
            }))
            .track_focus(&self.focus)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .font_family("Wanted Sans")
            .text_size(px(12.0))
            .text_color(rgb(ui::TEXT))
            .bg(rgb(ui::BG))
            .capture_key_down(cx.listener(Self::menu_key))
            .capture_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, window, cx| {
                crate::modal_keyboard::clear_press(window, cx);
                this.ae_import.update(cx, |panel, _| panel.release_key(event.keystroke.key.as_str()));
                this.expression_editor.update(cx, |panel, _| panel.release_key(event.keystroke.key.as_str()));
                this.confirmation_keys.release(event.keystroke.key.as_str());
                if !this.confirmation_keys.has_pressed_activation() { this.ae_exit_barrier = false; this.expression_exit_barrier = false; }
            }))
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
                        menu::MENUS
                        .into_iter()
                        .map(|name| {
                            ui::text_button(name, name)
                                .when(self.menu == Some(name), |s| s.bg(rgb(0x353535)))
                                .capture_any_mouse_down(cx.listener(Self::menu_pointer_down))
                                .on_click(cx.listener(move |this, event, window, cx| this.click_menu(name, event, window, cx)))
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
                    .child(ui::text_button("shape-menu", "▾").capture_any_mouse_down(cx.listener(Self::menu_pointer_down)).on_click(cx.listener(|this, event, window, cx| this.click_menu("Shape", event, window, cx))))
                    .child(ui::action_tool("pen-tool", "pen", "Pen (G) · Shift-click / Shift-drag select · Contents: same-layer cross-path selection, Ctrl+A all points · Cross-path drag / Shift+T: composition axes, current frame · Ordinary single-path drag: local axes · First point / Enter finishes · Alt converts corners or breaks handles · Ctrl draws a mask on a shape", &self.state, Action::SetTool(Tool::Pen), tool == Tool::Pen))
                    .child(ui::action_tool("text-tool", "text", "Text tool (Ctrl+T) · Click point text · Drag a paragraph box", &self.state, Action::SetTool(Tool::Text), tool == Tool::Text))
                    .child(div().mx_2().w(px(1.0)).h(px(20.0)).bg(rgb(0x414141)))
                    .child(ui::text_button("toolbar-snapping", if self.state.read(cx).snapping {"☑ Snapping"} else {"☐ Snapping"}).on_click(cx.listener(|this,_,window,cx| {let _ = window; this.state.update(cx, |s,cx| {s.snapping = !s.snapping; cx.notify();});})))
                    .child(div().flex_1())
                    .children(crate::view_state::WorkspacePreset::ALL.into_iter().map(|preset| {
                        ui::text_button(gpui::SharedString::from(format!("workspace-{}", preset.label())), preset.label())
                            .text_size(px(11.0))
                            .on_key_down(|e, _, cx| { if matches!(e.keystroke.key.as_str(), "enter" | "space") { cx.stop_propagation(); } })
                            .when(self.workspace_matches(preset, cx), |button| button.text_color(rgb(ui::BLUE)))
                            .on_click(cx.listener(move |this, _, _, cx| this.apply_workspace(preset, cx)))
                    }))
                    .child(ui::text_button("user-workspaces", self.workspace_selected.clone().unwrap_or_else(|| "Workspaces…".into()))
                        .on_key_down(|e, _, cx| { if matches!(e.keystroke.key.as_str(), "enter" | "space") { cx.stop_propagation(); } })
                        .on_click(cx.listener(|this, _, window, cx| this.open_workspaces(window, cx))))
                    .child(
                        ui::text_button("reset-workspace", "Reset workspace")
                            .on_key_down(|e, _, cx| { if matches!(e.keystroke.key.as_str(), "enter" | "space") { cx.stop_propagation(); } })
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

        if self.workspace_dialog {
            root = root.child(gpui::deferred(self.workspaces_view(cx)).with_priority(5));
        }
        if let Some(menu) = self.menu {
            let items = menu::items(menu, self.state.read(cx));
            let mut dropdown = div()
                .id("main-menu")
                .absolute()
                .top(px(if menu == "Shape" { 63.0 } else { 27.0 }))
                .left(px(match menu {
                    "Shape" => 165.0,
                    "File" | "Open Recent" => 0.0,
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
                .max_h((window.viewport_size().height - px(90.0)).max(px(100.0)))
                .overflow_y_scroll()
                .track_scroll(&self.menu_scroll)
                .p_1()
                .bg(rgb(0x2b2b2b))
                .border_1()
                .border_color(rgb(0x4a4a4a))
                .shadow_lg()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.menu = None;
                    this.menu_cursor = None;
                    this.menu_return_focus = None;
                    this.timeline_command_origin = None;
                    this.state
                        .update(cx, |state, _| state.shell_clipboard_blocked = false);
                    cx.notify();
                }));
            for (index, item) in items.into_iter().enumerate() {
                let label = if let Some(menu::Target::Action(Action::ViewerOption(option))) =
                    &item.target
                {
                    format!(
                        "{} {}",
                        if self.state.read(cx).viewer.enabled(*option) {
                            "✓"
                        } else {
                            "  "
                        },
                        item.label
                    )
                } else if let Some(menu::Target::Action(Action::OpenRecent(path))) = &item.target {
                    format!(
                        "{}. {}",
                        item.label,
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )
                } else {
                    item.label.to_string()
                };
                let recent_path = match &item.target {
                    Some(menu::Target::Action(Action::OpenRecent(path))) => {
                        Some(path.display().to_string())
                    }
                    _ => None,
                };
                let enabled = item.target.is_some();
                dropdown = dropdown.child(
                    ui::text_button(("menu-action", index), "")
                        .w_full()
                        .justify_between()
                        .flex_none()
                        .when(!enabled, |s| s.opacity(0.4))
                        .when(self.menu_cursor == Some(index), |s| s.bg(rgb(0x164a7b)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(label),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(ui::MUTED))
                                .child(item.shortcut),
                        )
                        .when_some(recent_path, |button, path| {
                            button.tooltip(move |_, cx| {
                                cx.new(|_| ui::Tip(path.clone().into())).into()
                            })
                        })
                        .on_hover(cx.listener(move |this, hovered, _, cx| {
                            if *hovered && enabled {
                                this.menu_cursor = Some(index);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(target) = item.target.clone() {
                                this.run_menu(target, window, cx);
                            }
                        })),
                );
            }
            root = root.child(gpui::deferred(dropdown).with_priority(2));
        }
        if self.search_open {
            root = root.child(gpui::deferred(self.render_search(window, cx)).with_priority(3));
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
                    "Preview arrow keys — Move selected layers 1 px (Shift: 10 px)",
                    "Drag handles — Scale    Shift — Proportional scale / 15° rotation",
                    "Esc — Cancel canvas drag",
                    "Ctrl+Z / Ctrl+Shift+Z — Undo / Redo",
                    "Ctrl+S / Ctrl+Shift+S — Save / Save as",
                    "Ctrl+I — Import image    Ctrl+C / Ctrl+X / Ctrl+V — Copy / Cut / Paste selection",
                    "File → Import SVG as editable shapes · Static paths, primitives, groups and solid paints",
                    "SVG: integer-pixel viewport · One layer at composition origin · One Undo",
                    "SVG inline style: literal paint/stroke/opacity only; overrides attributes",
                    "No stylesheets, resources, CSS-wide values or !important",
                    "Geometry becomes editable cubic paths; antialiased edges may differ slightly",
                    "Unsupported SVG features reject the entire import",
                    "Ctrl / Shift click — Toggle / Range select layers",
                    "Timeline rows: Up/Down, Home/End — Select layer · Shift extends · Ctrl+A selects visible",
                    "Timeline F2 / double-click layer name — Rename · Enter commits · Esc cancels",
                    "Drag empty time area — Box select keys or layers",
                    "Drag a number — Scrub value (Shift: faster, Alt: finer)",
                    "Ctrl+K — Composition settings",
                    "Space — Play / Pause    Home / End outside Timeline rows — Seek",
                    "Page Up / Down — Step frame (Shift: 10 frames)",
                    "P / A / S / R / T — Reveal transform property",
                    "U — Animated properties    J / K — Previous / Next key",
                    "F4 — Toggle Timeline Switches / Modes columns",
                    "Ctrl+Shift+C — Pre-compose selected layers",
                    "Shift+F3 — Graph Editor    F9 — Easy Ease selected keys",
                    "Shift+F9 — Ease In    Ctrl+Shift+F9 — Ease Out (timeline/graph)",
                    "B / N — Work area start / end    + / − — Timeline zoom",
                    "Ctrl+Shift+P — Find command · ↑/↓ select · Enter run · Esc close",
                    "F10 — Menus · ←/→ menu · ↑/↓ item · Enter run · Esc close",
                    "Menu Home/End — First/last · Letter — Next matching item",
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
        if self.about {
            let mut dialog = div()
                .id("about-libre-effects-dialog")
                .w(px(540.0))
                .p_5()
                .flex()
                .flex_col()
                .gap_3()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(0x555555))
                .shadow_lg()
                .child(
                    div()
                        .text_size(px(18.0))
                        .text_color(rgb(0xffffff))
                        .child("About Libre Effects"),
                )
                .child(
                    div()
                        .text_color(rgb(ui::MUTED))
                        .child("Motion graphics editor · Rust + GPUI"),
                );
            for (label, value) in crate::build_info::rows() {
                dialog = dialog.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(ui::MUTED))
                                .child(label),
                        )
                        .child(div().text_size(px(12.0)).child(value)),
                );
            }
            dialog = dialog
                .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED))
                    .child("Build information is embedded in this executable. The source fingerprint identifies changes; it is not a security signature."))
                .child(div().flex().justify_end().child(
                    ui::text_button("close-about", "Close (Esc)")
                        .bg(rgb(0x175c99))
                        .on_click(cx.listener(|this, _, window, cx| this.close_about(window, cx))),
                ));
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
        if self.state.read(cx).fonts_open {
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
                        .child(self.font_manager.clone()),
                )
                .with_priority(3),
            );
        }
        let recovering = self.state.read(cx).recovery.is_some();
        let confirmation = self.closing || self.pending_document.is_some() || recovering;
        let focus_modals = FocuslessModals {
            settings: self.settings,
            help: self.help,
            about: self.about,
            media: self.state.read(cx).media_open,
            confirmation,
        };
        if focus_modals.opened_since(self.modal_active) {
            self.state.update(cx, |s, _| s.retire_colors_context());
            cx.stop_active_drag(window);
            window.focus(&self.focus);
        }
        // Do not refocus on later renders: Settings/Media text fields own their
        // active editing focus. Color, Gradient, Vertex, Fonts and Search own their focus.
        self.modal_active = focus_modals;
        let ae_confirmation = matches!(self.pending_document, Some(Action::ApplyAeProject(_)));
        if ae_confirmation && !self.ae_confirmation_active {
            window.focus(&self.confirmation_focus[2]);
        }
        self.ae_confirmation_active = ae_confirmation;
        if confirmation {
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
            if ae_confirmation {
                dialog = dialog.children(crate::modal_keyboard::warning(cx).map(|message| {
                    div()
                        .text_color(rgb(ui::MUTED))
                        .text_size(px(11.0))
                        .child(message)
                }));
            }
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
                    .child(
                        ui::text_button("close-save", "Save and continue")
                            .when(ae_confirmation, |button| {
                                button.track_focus(&self.confirmation_focus[0])
                            })
                            .on_click(cx.listener(|this, event, window, cx| {
                                if matches!(this.pending_document, Some(Action::ApplyAeProject(_)))
                                {
                                    if matches!(event, gpui::ClickEvent::Mouse(click) if !click.down.first_mouse) {
                                        this.ae_confirmation_choice(0, window, cx);
                                    }
                                    return;
                                }
                                this.pending_save = this.pending_document.is_some();
                                this.state.update(cx, |s, cx| {
                                    s.close_after_save = this.closing;
                                    s.dispatch(&Action::Save, window, cx);
                                });
                            })),
                    )
                    .child(
                        ui::text_button("close-discard", "Discard changes")
                            .when(ae_confirmation, |button| {
                                button.track_focus(&self.confirmation_focus[1])
                            })
                            .on_click(cx.listener(|this, event, window, cx| {
                                if matches!(this.pending_document, Some(Action::ApplyAeProject(_)))
                                {
                                    if matches!(event, gpui::ClickEvent::Mouse(click) if !click.down.first_mouse) {
                                        this.ae_confirmation_choice(1, window, cx);
                                    }
                                    return;
                                }
                                if this.state.read(cx).saving {
                                    return;
                                }
                                if this.closing {
                                    this.state.read(cx).flush_recent_projects_on_close();
                                    this.state.read(cx).clear_recovery();
                                    window.remove_window();
                                } else if let Some(action) = this.pending_document.take() {
                                    this.pending_save = false;
                                    this.state
                                        .update(cx, |s, cx| s.dispatch(&action, window, cx));
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        ui::text_button("close-cancel", "Cancel")
                            .when(ae_confirmation, |button| {
                                button.track_focus(&self.confirmation_focus[2])
                            })
                            .on_click(cx.listener(|this, event, window, cx| {
                                if matches!(this.pending_document, Some(Action::ApplyAeProject(_)))
                                {
                                    if matches!(event, gpui::ClickEvent::Mouse(click) if !click.down.first_mouse) {
                                        this.ae_confirmation_choice(2, window, cx);
                                    }
                                    return;
                                }
                                this.closing = false;
                                this.pending_document = None;
                                this.pending_save = false;
                                this.state.update(cx, |s, _| {
                                    s.close_after_save = false;
                                    s.cancel_ae_import_confirmation();
                                });
                                cx.notify();
                            })),
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
        if self.state.read(cx).gradient_editor.is_some() {
            root = root.child(
                gpui::deferred(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(gpui::rgba(0x00000070))
                        .occlude()
                        .child(self.gradient_editor.clone()),
                )
                .with_priority(4),
            );
        }
        if self.state.read(cx).vertex_editor.is_some() {
            root = root.child(
                gpui::deferred(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(gpui::rgba(0x00000070))
                        .occlude()
                        .child(self.vertex_editor.clone()),
                )
                .with_priority(4),
            );
        }
        if self.state.read(cx).expression_editor.is_some() {
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
                        .child(self.expression_editor.clone()),
                )
                .with_priority(6),
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
        if self.state.read(cx).ae_import.is_some() {
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
                        .child(self.ae_import.clone()),
                )
                .with_priority(4),
            );
        }
        if self.state.read(cx).automation.is_some() {
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
                        .child(self.script_ui.clone()),
                )
                .with_priority(6),
            );
        }
        root
    }
}

#[cfg(test)]
mod modal_focus_tests {
    use super::{FocuslessModals, about_key_closes, timeline_command_owned};

    #[test]
    fn timeline_menu_origin_survives_overlay_focus_changes() {
        for timeline in [false, true] {
            for shell in [false, true] {
                assert!(timeline_command_owned(Some(true), timeline, shell));
                assert!(!timeline_command_owned(Some(false), timeline, shell));
            }
        }
    }

    #[test]
    fn neutral_shell_uses_timeline_safety_without_overriding_other_panels() {
        assert!(timeline_command_owned(None, true, false));
        assert!(timeline_command_owned(None, false, true));
        assert!(!timeline_command_owned(None, false, false));
        // A refused menu command leaves neutral Shell focus. Repeated Delete
        // remains guarded after the one-shot captured origin is cleared.
        assert!(timeline_command_owned(Some(true), false, true));
        assert!(timeline_command_owned(None, false, true));
        // A menu originating in Graph remains explicit for that invocation;
        // later Graph focus remains exempt from Timeline-only row filtering.
        assert!(!timeline_command_owned(Some(false), false, true));
        assert!(!timeline_command_owned(None, false, false));
    }

    #[test]
    fn about_keyboard_dismissal_does_not_enable_editor_shortcuts() {
        for key in ["escape", "enter", "space"] {
            assert!(about_key_closes(key, false));
            assert!(!about_key_closes(key, true));
        }
        for key in [
            "tab",
            "delete",
            "backspace",
            "n",
            "z",
            "p",
            "f10",
            "left",
            "right",
        ] {
            assert!(!about_key_closes(key, false));
            assert!(!about_key_closes(key, true));
        }
    }

    #[test]
    fn shell_modal_focus_captures_every_focusless_overlay_only_on_open() {
        for next in [
            FocuslessModals {
                settings: true,
                ..Default::default()
            },
            FocuslessModals {
                help: true,
                ..Default::default()
            },
            FocuslessModals {
                about: true,
                ..Default::default()
            },
            FocuslessModals {
                media: true,
                ..Default::default()
            },
            FocuslessModals {
                confirmation: true,
                ..Default::default()
            },
        ] {
            assert!(next.opened_since(FocuslessModals::default()));
            assert!(
                !next.opened_since(next),
                "Re-render must preserve descendant field focus"
            );
            assert!(
                !FocuslessModals::default().opened_since(next),
                "Closing does not recapture focus"
            );
        }
    }
    #[test]
    fn shell_modal_focus_handles_later_confirmation_and_dialog_switches() {
        let settings = FocuslessModals {
            settings: true,
            ..Default::default()
        };
        let covered = FocuslessModals {
            confirmation: true,
            ..settings
        };
        assert!(covered.opened_since(settings));
        assert!(!settings.opened_since(covered));
        let help = FocuslessModals {
            help: true,
            ..Default::default()
        };
        assert!(help.opened_since(settings));
        assert!(settings.opened_since(FocuslessModals::default()));
    }
    #[test]
    fn shell_modal_settings_help_media_do_not_enable_save_confirmation() {
        let focus_only = FocuslessModals {
            settings: true,
            help: true,
            about: true,
            media: true,
            confirmation: false,
        };
        assert!(focus_only.opened_since(FocuslessModals::default()));
        assert!(!focus_only.confirmation);
    }
}
