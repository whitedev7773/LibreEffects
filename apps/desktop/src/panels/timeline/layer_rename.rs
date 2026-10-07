//! Inline name editing owns one visible row and never retargets a stale draft.
use super::*;
use libre_effects_editor_model::timeline_rename::{LayerRename, RenameContext, RenameFocus};

pub(super) struct RenameInput {
    pub target: LayerRename,
    pub field: Entity<TextField>,
    live: Rc<Cell<bool>>,
    focus: RenameFocus,
}

fn context<'a>(state: &'a EditorState, visible: &'a BTreeSet<LayerId>) -> RenameContext<'a> {
    RenameContext {
        editor: &state.editor,
        selected: &state.selected_layers,
        visible,
        document_revision: state.document_revision,
        input_generation: state.input_context_generation(),
        frame: state.frame,
        playing: state.playing,
    }
}

impl Timeline {
    fn rename_domain_available(&self, cx: &gpui::App) -> bool {
        let state = self.state.read(cx);
        state.selected_keys.is_empty()
            && !state.colors_key_owned.get()
            && state.text_session.is_none()
            && state.colors.session.is_none()
            && state.gradient_editor.is_none()
            && state.vertex_editor.is_none()
            && state.expression_editor.is_none()
            && self.drag.is_none()
            && self.bar_drag.is_none()
            && self.marquee.is_none()
            && !self.scrubbing
            && !self.resizing
            && self.parent_open.is_none()
            && self.key_menu.is_none()
            && !self.type_open
    }

    pub(super) fn prepare_layer_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = &self.rename else { return };
        let visible = self.visible_layer_ids(cx);
        if input.live.get()
            && self.rename_domain_available(cx)
            && input
                .target
                .current(&context(self.state.read(cx), &visible))
        {
            let focused = input.field.read(cx).has_focus(window);
            let input = self.rename.as_mut().unwrap();
            if input.focus.keep_for_blur(focused) {
                // GPUI delivers focus listeners after drawing roots. Retain a
                // current field for that first unfocused draw so its own blur
                // handler can commit. The next draw also closes no-op/Escape.
                if !focused {
                    cx.defer_in(window, |_, _, cx| cx.notify());
                }
                return;
            }
        }
        let input = self.rename.take().unwrap();
        // Invalidate before blur: its deferred callback cannot mutate a hidden,
        // replaced, newly selected or source-changed layer, including IME text.
        input.live.set(false);
        if input.field.read(cx).has_focus(window) {
            window.focus(&self.focus);
        }
    }

    pub(super) fn begin_layer_rename(
        &mut self,
        id: LayerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rename.is_some()
            || !self.rename_domain_available(cx)
            || TextField::active_has_focus(window, cx)
            || TextField::is_composing(window, cx)
            || TextField::active_has_pending_source_input(cx)
        {
            return;
        }
        let visible = self.visible_layer_ids(cx);
        let state = self.state.read(cx);
        let Some(target) =
            LayerRename::begin(&context(state, &visible)).filter(|s| s.layer() == id)
        else {
            self.state.update(cx, |s, cx| {
                s.status =
                    "Select one visible, unlocked layer and pause playback to rename it.".into();
                cx.notify();
            });
            return;
        };
        let binding = format!("timeline-rename-{id}-{}", state.input_context_generation());
        let field = cx.new(|cx| TextField::new(cx, |_, _, _| {}).return_focus(self.focus.clone()));
        let owner = cx.entity().downgrade();
        let edit = self.state.clone();
        let receipt = target.clone();
        let live = Rc::new(Cell::new(true));
        let pending = live.clone();
        field.update(cx, |field, cx| {
            field.sync_guarded(
                binding,
                target.name().into(),
                window,
                move |name, window, cx| {
                    let command = if pending.replace(false) {
                        owner.upgrade().and_then(|owner| {
                            let timeline = owner.read(cx);
                            timeline.rename_domain_available(cx).then(|| {
                                let visible = timeline.visible_layer_ids(cx);
                                receipt.command(&context(edit.read(cx), &visible), name)
                            })
                        })
                    } else {
                        None
                    };
                    edit.update(cx, |state, cx| {
                        match command {
                            Some(Ok(Some(command))) => {
                                state.dispatch(&Action::Edit(command), window, cx)
                            }
                            Some(Err(error)) => {
                                state.status = error.into();
                                cx.notify();
                            }
                            _ => {}
                        }
                        state
                            .editor
                            .project()
                            .composition()
                            .layer(id)
                            .map_or_else(String::new, |layer| layer.name().into())
                    })
                },
            );
            field.focus_select_all(window, cx);
        });
        cx.observe(&field, |_, _, cx| cx.notify()).detach();
        self.layer_navigation.reset();
        self.reveal_layer = Some(id);
        self.rename = Some(RenameInput {
            target,
            field,
            live,
            focus: Default::default(),
        });
        cx.notify();
    }

    pub(super) fn layer_rename_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus.is_focused(window) || event.keystroke.key != "f2" {
            return false;
        }
        if !event.is_held
            && !event.keystroke.modifiers.modified()
            && let Some(id) = self.state.read(cx).editor.selected()
        {
            self.begin_layer_rename(id, window, cx);
        }
        true
    }
}
