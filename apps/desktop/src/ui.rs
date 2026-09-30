use crate::editor::{Action, EditorState};
use gpui::{
    AssetSource, Context, Div, Entity, SharedString, Stateful, Window, div, prelude::*, px, rgb,
    svg,
};
use std::borrow::Cow;

pub const BG: u32 = 0x1d1d1d;
pub const PANEL: u32 = 0x232323;
pub const BORDER: u32 = 0x111111;
pub const TEXT: u32 = 0xc9c9c9;
pub const MUTED: u32 = 0x969696;
pub const BLUE: u32 = 0x4da6ff;

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        macro_rules! icons { ($($name:literal),* $(,)?) => { match path {
            $($name => Some(Cow::Borrowed(include_bytes!(concat!("../assets/icons/", $name, ".svg")) as &[u8])),)*
            _ => None,
        } }; }
        Ok(icons!(
            "hand",
            "square",
            "plus",
            "play",
            "pause",
            "eye",
            "eye-slash",
            "lock",
            "lock-open",
            "chevron-down",
            "chevron-right",
            "arrow-rotate-left",
            "arrow-rotate-right",
            "magnifier",
            "folder-open",
            "floppy-disk",
            "trash-bin",
            "copy",
            "gear",
            "minus",
            "diamond",
            "stopwatch",
            "arrow-left",
            "arrow-right",
            "arrow-up",
            "arrow-down",
            "filmstrip",
            "target",
            "square-dashed",
            "xmark"
        ))
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

pub fn icon(name: &'static str) -> gpui::Svg {
    svg()
        .path(name)
        .size(px(14.0))
        .flex_none()
        .text_color(rgb(TEXT))
}

struct Tip(SharedString);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .bg(rgb(0x101010))
            .text_color(rgb(TEXT))
            .text_size(px(11.0))
            .rounded_sm()
            .child(self.0.clone())
    }
}

pub fn tool(
    id: impl Into<gpui::ElementId>,
    name: &'static str,
    label: impl Into<SharedString>,
    active: bool,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(26.0))
        .flex_none()
        .rounded(px(3.0))
        .cursor_pointer()
        .tab_index(0)
        .bg(rgb(if active { 0x164a7b } else { PANEL }))
        .text_color(rgb(if active { BLUE } else { TEXT }))
        .hover(|s| s.bg(rgb(0x373737)))
        .focus(|s| s.border_1().border_color(rgb(BLUE)))
        .tooltip(move |_, cx| cx.new(|_| Tip(label.clone())).into())
        .child(icon(name).text_color(rgb(if active { BLUE } else { TEXT })))
}

pub fn action_tool(
    id: impl Into<gpui::ElementId>,
    name: &'static str,
    label: impl Into<SharedString>,
    state: &Entity<EditorState>,
    action: Action,
    active: bool,
) -> Stateful<Div> {
    let state = state.clone();
    tool(id, name, label, active).on_click(move |_, window, cx| {
        cx.stop_propagation();
        state.update(cx, |state, cx| state.dispatch(&action, window, cx));
    })
}

pub fn panel_header(title: impl Into<SharedString>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h(px(31.0))
        .flex_none()
        .px_3()
        .gap_2()
        .bg(rgb(BG))
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .border_b_2()
                .border_color(rgb(BLUE))
                .child(title.into()),
        )
}

pub fn text_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .h(px(25.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .tab_index(0)
        .hover(|s| s.bg(rgb(0x353535)))
        .focus(|s| s.bg(rgb(0x164a7b)))
        .child(label.into())
}
