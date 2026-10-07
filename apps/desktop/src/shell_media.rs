use super::*;

impl Shell {
    pub(super) fn media_dialog(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.state.read(cx);
        let entries = state.media_entries.clone();
        let scanning = state.scanning_media;
        let busy = state.importing_video || scanning;
        let collecting = state.collecting;
        let message = state.media_message.clone();
        let missing = entries.iter().filter(|e| e.offline).count();
        let mut controls = div().flex().gap_2();
        for (label, action, enabled) in [
            ("Refresh", Action::RefreshMedia, !busy),
            (
                "Relink missing from folder…",
                Action::RelinkMissing,
                !busy && missing > 0,
            ),
            ("Collect files…", Action::CollectFiles, !collecting),
            ("Cancel collection", Action::CancelCollection, collecting),
        ] {
            controls = controls.child(
                ui::text_button(label, label)
                    .when(!enabled, |s| s.opacity(0.4))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if enabled {
                            this.dispatch(action.clone(), window, cx);
                        }
                    })),
            );
        }
        let mut list = div()
            .id("media-sources-list")
            .max_h(px(330.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        if entries.is_empty() {
            list = list.child(if scanning {
                "Checking project footage…"
            } else {
                "No linked videos. Images are embedded in the project."
            });
        }
        for (index, entry) in entries.into_iter().enumerate() {
            let name = std::path::Path::new(&entry.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            list = list.child(
                div()
                    .flex_none()
                    .p_2()
                    .bg(rgb(ui::BG))
                    .flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_color(rgb(if entry.offline {
                                        0xffaa77
                                    } else {
                                        ui::TEXT
                                    }))
                                    .child(format!(
                                        "{} · {name} · {} layer reference(s)",
                                        if entry.offline { "Missing" } else { "Online" },
                                        entry.references
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(ui::MUTED))
                                    .child(entry.path.clone()),
                            ),
                    )
                    .child(
                        ui::text_button(("media-locate", index), "Locate…")
                            .when(busy, |s| s.opacity(0.4))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if !busy {
                                    this.dispatch(
                                        Action::RelinkSource(entry.path.clone()),
                                        window,
                                        cx,
                                    );
                                }
                            })),
                    ),
            );
        }
        div().w(px(820.0)).max_h(px(650.0)).p_5().flex().flex_col().gap_3()
            .bg(rgb(ui::PANEL)).border_1().border_color(rgb(ui::BORDER))
            .child(div().text_size(px(16.0)).child("Project Media"))
            .child(format!("{} linked source(s) · {missing} missing", state.media_entries.len()))
            .child(controls)
            .child(list)
            .child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED))
                .child("Relink updates every reference, including locked layers, while preserving transforms and timing. Folder search leaves ambiguous names unresolved. Collect creates a separate portable project with all linked footage and embedded images."))
            .child(div().id("media-operation-message").max_h(px(100.0)).overflow_y_scroll().text_size(px(11.0)).child(message))
            .child(div().flex().justify_end().child(ui::text_button("media-close", "Close").on_click(cx.listener(|this, _, window, cx| {
                this.state.update(cx, |s, cx| { s.media_open = false; cx.notify(); });
                window.focus(&this.focus);
                cx.notify();
            }))))
    }
}
