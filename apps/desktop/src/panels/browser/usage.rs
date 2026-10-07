//! Direct source references, with explicit guarded navigation only.
use super::*;
use crate::editor::project_usage::Target;
use libre_effects_editor_model::project_usage;

pub(super) struct Cache {
    key: (u64, u64, ProjectItem),
    uses: Vec<project_usage::Usage>,
}

fn usage_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    ui::text_button(id, label)
        // The workspace's earlier capture protects pending source fields. Keep
        // focus until the click, including a press/release outside cancellation.
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_key_down(|event: &gpui::KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
            }
        })
}

impl Browser {
    fn usage_input_available(&self, window: &Window, cx: &gpui::App) -> bool {
        !cx.has_active_drag()
            && !TextField::is_composing(window, cx)
            && !TextField::active_has_pending_source_input(cx)
            && self.state.read(cx).project_usage_available()
    }

    pub(super) fn usage_details(
        &mut self,
        project: &Project,
        item: ProjectItem,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if matches!(item, ProjectItem::Folder(_)) {
            return div().into_any_element();
        }
        let expanded = self.usage_open == Some(item);
        let state = self.state.read(cx);
        let document_revision = state.document_revision;
        let editor_generation = state.editor.context_generation();
        let input_generation = state.input_context_generation();
        let key = (document_revision, editor_generation, item);
        if self
            .usage_cache
            .as_ref()
            .is_none_or(|cache| cache.key != key)
        {
            // Playhead/transport redraws reuse the scan. Editor generations catch
            // edits, history and selection without cloning the Project here.
            self.usage_cache = Some(Cache {
                key,
                uses: project_usage::direct_uses(project, item),
            });
        }
        let uses = &self.usage_cache.as_ref().unwrap().uses;
        let mut details = div()
            .id("project-usage-details")
            .flex()
            .flex_col()
            // Shrink the scrollable list in short panes, retaining Show/Hide.
            .min_h(px(25.0))
            .min_w_0()
            .overflow_hidden()
            .px_2()
            .text_size(px(11.0))
            .child(
                usage_button(
                    "project-direct-uses",
                    format!("{} direct uses · {}", uses.len(), if expanded { "Hide" } else { "Show" }),
                )
                .w_full()
                .flex_none()
                .justify_start()
                .gap_1()
                .child(ui::icon(if expanded { "chevron-down" } else { "chevron-right" }))
                .tooltip(|_, cx| cx.new(|_| ui::Tip("Layers that directly reference this item. Nested indirect uses are not counted.".into())).into())
                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    if !event.standard_click() || !this.usage_input_available(window, cx) {
                        return;
                    }
                    let state = this.state.read(cx);
                    if state.project_item != Some(item) || state.document_revision != document_revision {
                        return;
                    }
                    this.usage_open = if this.usage_open == Some(item) { None } else { Some(item) };
                    window.focus(&this.focus);
                    cx.notify();
                })),
            );
        if expanded {
            let mut list = div()
                .id("project-direct-use-list")
                .flex()
                .flex_col()
                .min_w_0()
                .min_h_0()
                .max_h(px(115.0))
                .overflow_y_scroll();
            if uses.is_empty() {
                list = list.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_color(rgb(ui::MUTED))
                        .child("No direct uses in this project."),
                );
            }
            for usage in uses {
                let target = Target {
                    item,
                    composition: usage.composition,
                    layer: usage.layer,
                    document_revision,
                    editor_generation,
                    input_generation,
                };
                let label = format!("{} › {}", usage.composition_name, usage.layer_name);
                let tip = format!(
                    "Open {} and select {} · composition {}, layer {}",
                    usage.composition_name, usage.layer_name, usage.composition, usage.layer
                );
                list = list.child(
                    usage_button(
                        SharedString::from(format!(
                            "project-use-{}-{}",
                            usage.composition, usage.layer
                        )),
                        "",
                    )
                    .w_full()
                    .min_w_0()
                    .flex_none()
                    .justify_start()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .text_ellipsis()
                            .line_clamp(1)
                            .child(label),
                    )
                    .tooltip(move |_, cx| cx.new(|_| ui::Tip(tip.clone().into())).into())
                    .on_click(cx.listener(
                        move |this, event: &gpui::ClickEvent, window, cx| {
                            if !event.standard_click() || !this.usage_input_available(window, cx) {
                                return;
                            }
                            let shown = this.state.update(cx, |s, cx| {
                                let shown = s.show_project_usage(target);
                                if shown {
                                    cx.notify();
                                }
                                shown
                            });
                            if shown {
                                // Timeline consumes the one-shot scroll/focus request.
                                window.focus(&this.focus);
                                cx.notify();
                            }
                        },
                    )),
                );
            }
            details = details.child(list);
        }
        crate::color_edit::input_pointer_navigation_guarded(details, move |state, cx| {
            !cx.has_active_drag()
                && state.project_usage_available()
                && state.project_item == Some(item)
                && state.document_revision == document_revision
                && state.editor.context_generation() == editor_generation
                && state.input_context_generation() == input_generation
        })
        .into_any_element()
    }
}
