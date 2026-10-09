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

pub(crate) fn parse_hex_color(text: &str) -> Result<u32, &'static str> {
    let value = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Enter a six-digit RGB color, for example #26384A");
    }
    u32::from_str_radix(value, 16).map_err(|_| "Invalid RGB color")
}

#[cfg(test)]
mod color_tests {
    use super::parse_hex_color;
    #[test]
    fn rgb_input_accepts_six_hex_digits_and_rejects_alpha_or_partial_values() {
        assert_eq!(parse_hex_color(" #aBcD09 "), Ok(0xabcd09));
        assert_eq!(parse_hex_color("000000"), Ok(0));
        for value in ["#fff", "#12345678", "", "0x1234", "+12345", "GG1122"] {
            assert!(parse_hex_color(value).is_err(), "{value}");
        }
    }
}

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        macro_rules! icons { ($($name:literal),* $(,)?) => { match path {
            $($name => Some(Cow::Borrowed(include_bytes!(concat!("../assets/icons/", $name, ".svg")) as &[u8])),)*
            _ => None,
        } }; }
        Ok(icons!(
            "text-align-left",
            "text-align-center",
            "text-align-right",
            "cursor",
            "pen",
            "circle",
            "star",
            "text",
            "triangle-up",
            "volume",
            "volume-xmark",
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
            "xmark",
            "object-align-left",
            "object-align-center-horizontal",
            "object-align-right",
            "object-align-top",
            "object-align-center-vertical",
            "object-align-bottom",
            "chart-line",
            "circle-link"
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

pub(crate) struct Tip(pub(crate) SharedString);
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
        .h(px(25.0))
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
                .border_color(rgb(TEXT))
                .child(title.into()),
        )
}

/// Both docks share visited composition tabs and the same navigation actions.
pub(crate) fn composition_tabs(
    state: &Entity<EditorState>,
    cx: &gpui::App,
    viewer: bool,
) -> Stateful<Div> {
    let s = state.read(cx);
    let active = s.editor.project().active_composition_id();
    let ids = s.composition_tab_ids();
    let tabs: Vec<_> = ids
        .into_iter()
        .filter_map(|id| {
            s.editor
                .project()
                .compositions()
                .into_iter()
                .find(|(candidate, _)| *candidate == id)
                .map(|(_, comp)| (id, comp.name().to_string()))
        })
        .collect();
    let count = tabs.len();
    let mut bar = div()
        .id(if viewer {
            "viewer-tabs"
        } else {
            "timeline-tabs"
        })
        .flex()
        .flex_none()
        .h(px(26.0))
        .overflow_x_scroll()
        .border_b_1()
        .border_color(rgb(BORDER))
        .bg(rgb(BG));
    for (index, (id, name)) in tabs.iter().enumerate() {
        let id = *id;
        let neighbor = tabs
            .get(if index > 0 { index - 1 } else { 1 })
            .map(|(id, _)| *id);
        let switch_state = state.clone();
        let mut tab = div()
            .flex()
            .items_center()
            .flex_none()
            .when(id == active && (viewer || !s.queue_open), |d| {
                d.border_b_2().border_color(rgb(TEXT))
            })
            .child(
                text_button(
                    SharedString::from(format!("composition-tab-{viewer}-{id}")),
                    if viewer {
                        format!("Composition  {name}")
                    } else {
                        name.clone()
                    },
                )
                .max_w(px(260.0))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_size(px(11.0))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    switch_state.update(cx, |s, cx| {
                        s.dispatch(&Action::ActivateComposition(id), window, cx);
                        if !viewer && s.editor.project().active_composition_id() == id {
                            s.dispatch(
                                &Action::Queue(crate::editor::queue::QueueAction::Show(false)),
                                window,
                                cx,
                            );
                        }
                    });
                }),
            );
        if count > 1 {
            let close_state = state.clone();
            tab = tab.child(
                tool(
                    SharedString::from(format!("close-comp-{viewer}-{id}")),
                    "xmark",
                    "Close viewer tab (keeps the composition in Project)",
                    false,
                )
                .size(px(18.0))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    close_state.update(cx, |s, cx| {
                        if s.editor.project().active_composition_id() == id {
                            if let Some(next) = neighbor {
                                s.dispatch(&Action::ActivateComposition(next), window, cx);
                            }
                        }
                        if s.editor.project().active_composition_id() != id {
                            s.composition_tabs.retain(|candidate| *candidate != id);
                            cx.notify();
                        }
                    });
                }),
            );
        }
        bar = bar.child(tab);
    }
    if !viewer {
        let queue_state = state.clone();
        bar = bar.child(
            text_button("render-queue-tab", "Render Queue")
                .text_size(px(11.0))
                .flex_none()
                .when(s.queue_open, |d| d.border_b_2().border_color(rgb(TEXT)))
                .on_click(move |_, window, cx| {
                    queue_state.update(cx, |s, cx| {
                        s.dispatch(
                            &Action::Queue(crate::editor::queue::QueueAction::Show(true)),
                            window,
                            cx,
                        )
                    })
                }),
        );
    }
    bar
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
