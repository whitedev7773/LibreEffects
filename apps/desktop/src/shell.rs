use gpui::{Context, Entity, Window, div, prelude::*};

use crate::editor::{Action, EditorState, action_button};
use crate::panels::{Browser, Inspector, Preview, Timeline};
use crate::theme::ActiveTheme;

// Panels are entities created once in `new`, not via `cx.new` inline in
// `render`, so each keeps its own state across renders instead of being
// torn down and rebuilt on every frame.
pub(crate) struct Shell {
    state: Entity<EditorState>,
    browser: Entity<Browser>,
    preview: Entity<Preview>,
    inspector: Entity<Inspector>,
    timeline: Entity<Timeline>,
}

impl Shell {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| EditorState::default());
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            browser: cx.new(|cx| Browser::new(state.clone(), cx)),
            preview: cx.new(|cx| Preview::new(state.clone(), cx)),
            inspector: cx.new(|cx| Inspector::new(state.clone(), cx)),
            timeline: cx.new(|cx| Timeline::new(state.clone(), cx)),
            state,
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = window.theme().colors;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(
                div()
                    .flex()
                    .h_10()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(div().text_sm().mr_4().child("Libre Effects"))
                    .child(action_button(
                        "new-project",
                        "New",
                        &self.state,
                        Action::New,
                    ))
                    .child(action_button(
                        "open-project",
                        "Open",
                        &self.state,
                        Action::Open,
                    ))
                    .child(action_button(
                        "save-project",
                        "Save as",
                        &self.state,
                        Action::SaveAs,
                    ))
                    .child(
                        action_button("undo", "Undo", &self.state, Action::Undo)
                            .disabled(!self.state.read(cx).editor.can_undo()),
                    )
                    .child(
                        action_button("redo", "Redo", &self.state, Action::Redo)
                            .disabled(!self.state.read(cx).editor.can_redo()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(self.browser.clone())
                    .child(self.preview.clone())
                    .child(self.inspector.clone()),
            )
            .child(self.timeline.clone())
            .child(
                div()
                    .h_7()
                    .flex_none()
                    .px_3()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .overflow_hidden()
                    .child(self.state.read(cx).status.clone()),
            )
    }
}
