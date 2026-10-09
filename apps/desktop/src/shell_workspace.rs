use super::*;

impl Shell {
    pub(super) fn open_workspaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings
            || self.about
            || self.help
            || self.pending_document.is_some()
            || self.state.read(cx).automation.is_some()
        {
            return;
        }
        self.workspace_dialog = true;
        let name = self
            .workspace_selected
            .clone()
            .unwrap_or_else(|| "My workspace".into());
        self.workspace_name.update(cx, |f, cx| {
            f.sync("workspace-name".into(), name, window);
            f.focus_select_all(window, cx);
        });
        cx.notify();
    }
    fn workspace_layout(&self, cx: &gpui::App) -> crate::view_state::WorkspaceView {
        let s = self.state.read(cx);
        let mut view = s.workspace.clone();
        view.effect_controls_open = s.effect_controls_open;
        view.snapping = s.snapping;
        view
    }
    pub(super) fn workspace_matches(
        &self,
        preset: crate::view_state::WorkspacePreset,
        cx: &gpui::App,
    ) -> bool {
        if self.workspace_selected.is_some() {
            return false;
        }
        let current = self.workspace_layout(cx);
        let mut expected = crate::view_state::WorkspaceView::preset(preset);
        expected.snapping = current.snapping;
        expected.align_to_selection = current.align_to_selection;
        expected == current
    }
    fn install_workspace(
        &mut self,
        view: crate::view_state::WorkspaceView,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |s, cx| {
            s.workspace = view;
            s.snapping = s.workspace.snapping;
            s.effect_controls_open = s.workspace.effect_controls_open;
            cx.notify();
        });
        cx.notify();
    }
    fn write_workspace(&mut self, rename: bool, cx: &mut Context<Self>) {
        let name = self.workspace_name.read(cx).value().to_string();
        let mut next = self.workspace_library.clone();
        let result = if rename {
            self.workspace_selected
                .as_ref()
                .ok_or_else(|| "Select a saved workspace first".to_string())
                .and_then(|old| next.rename(old, &name))
        } else {
            next.save(&name, self.workspace_layout(cx))
        };
        let result = result.and_then(|name| {
            next.persist()?;
            Ok(name)
        });
        match result {
            Ok(name) => {
                self.workspace_library = next;
                self.workspace_selected = Some(name);
                self.workspace_error = "Saved to local profile".into();
            }
            Err(error) => self.workspace_error = error,
        }
        cx.notify();
    }
    pub(super) fn workspaces_view(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut list = div()
            .id("saved-workspaces")
            .max_h(px(230.0))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for name in self.workspace_library.layouts.keys() {
            let name = name.clone();
            list = list.child(
                ui::text_button(
                    gpui::SharedString::from(format!("saved-workspace-{name}")),
                    name.clone(),
                )
                .w_full()
                .justify_start()
                .when(self.workspace_selected.as_ref() == Some(&name), |d| {
                    d.text_color(rgb(ui::BLUE))
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    let Some(view) = this.workspace_library.layouts.get(&name).cloned() else {
                        return;
                    };
                    this.workspace_selected = Some(name.clone());
                    this.workspace_name.update(cx, |field, _| {
                        field.sync("workspace-name".into(), name.clone(), window)
                    });
                    this.install_workspace(view, cx);
                    this.workspace_error.clear();
                    cx.stop_propagation();
                })),
            );
        }
        div().absolute().inset_0().flex().items_center().justify_center().bg(gpui::rgba(0x00000090)).occlude()
            .on_key_down(|event, _, cx| { if event.keystroke.key != "escape" { cx.stop_propagation(); } })
            .child(div().w(px(360.0)).p_3().flex().flex_col().gap_2().bg(rgb(ui::PANEL)).border_1().border_color(rgb(ui::BORDER))
                .child("User workspaces")
                .child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child("Save panel proportions and visibility. Select a saved layout to restore it."))
                .child(list)
                .child(div().text_size(px(11.0)).child("Workspace name"))
                .child(self.workspace_name.clone())
                .child(div().flex().gap_1()
                    .child(ui::text_button("save-workspace", "Save current layout").on_click(cx.listener(|this, _, _, cx| this.write_workspace(false, cx))))
                    .child(ui::text_button("rename-workspace", "Rename selected").when(self.workspace_selected.is_none(), |d| d.opacity(0.4))
                        .on_click(cx.listener(|this, _, _, cx| this.write_workspace(true, cx)))))
                .child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(self.workspace_error.clone()))
                .child(ui::text_button("close-workspaces", "Close").on_click(cx.listener(|this, _, window, cx| {
                    this.workspace_dialog = false; window.focus(&this.focus); cx.notify(); cx.stop_propagation();
                }))))
            .into_any_element()
    }
}
