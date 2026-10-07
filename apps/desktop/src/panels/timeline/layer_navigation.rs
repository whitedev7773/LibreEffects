//! Keyboard row selection stays separate from source edits and Graph addresses.
use super::*;
use libre_effects_editor_model::timeline_navigation::{Selection, shortcut};

impl Timeline {
    pub(super) fn layer_navigation_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus.is_focused(window) {
            return false;
        }
        let m = event.keystroke.modifiers;
        let state = self.state.read(cx);
        if !state.selected_keys.is_empty() || state.colors_key_owned.get() {
            // Scalar and compound keys keep their established keyboard domains.
            self.layer_navigation.reset();
            return false;
        }
        let Some(navigation) = shortcut(
            &event.keystroke.key,
            m.control,
            m.shift,
            m.alt,
            m.platform,
            m.function,
        ) else {
            // An unsupported row-navigation chord must not turn into a Shell
            // source nudge or seek (notably platform/function modified keys).
            return matches!(event.keystroke.key.as_str(), "up" | "down" | "home" | "end");
        };
        if TextField::active_has_focus(window, cx)
            || TextField::is_composing(window, cx)
            || TextField::active_has_pending_source_input(cx)
            || state.text_session.is_some()
            || state.colors.session.is_some()
            || state.gradient_editor.is_some()
            || state.vertex_editor.is_some()
            || state.expression_editor.is_some()
            || self.drag.is_some()
            || self.bar_drag.is_some()
            || self.marquee.is_some()
            || self.scrubbing
            || self.resizing
            || self.parent_open.is_some()
        {
            // Never let an unavailable row command become a Shell nudge/seek.
            self.layer_navigation.reset();
            return true;
        }
        let visible = self.visible_layer_ids(cx);
        let selection = self.layer_navigation.select(
            state.editor.project().composition(),
            &visible,
            Selection {
                active: state.editor.selected(),
                layers: state.selected_layers.clone(),
            },
            navigation,
            m.shift,
            self.selected_only,
        );
        self.reveal_layer = selection.active;
        self.selected_key = None;
        self.state.update(cx, |s, cx| {
            s.select_timeline_rows(selection);
            cx.notify();
        });
        cx.notify();
        true
    }
}
