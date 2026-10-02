use crate::{
    editor::{Action, EditorState, Tool},
    ui,
};
use gpui::{Div, Entity, div, prelude::*, px, rgb};
use libre_effects_core::{Command, Layer};

pub(super) fn controls(state: &Entity<EditorState>, layer: &Layer) -> Div {
    let mut root = div().flex().flex_col().gap_1();
    if layer.path_masks().is_empty() {
        return root;
    }
    root = root.child(
        div()
            .mt_3()
            .py_2()
            .border_t_1()
            .border_color(rgb(ui::BORDER))
            .child("Masks · Pen (G) to edit points"),
    );
    for (index, mask) in layer.path_masks().iter().enumerate() {
        let id = layer.id();
        let mut row = div()
            .flex()
            .items_center()
            .gap_1()
            .child(div().w(px(48.0)).child(format!("Mask {}", index + 1)));
        for (name, label) in [
            ("mode", mask.mode.label()),
            (
                "invert",
                if mask.inverted {
                    "☑ Invert"
                } else {
                    "☐ Invert"
                },
            ),
            ("up", "↑"),
            ("down", "↓"),
            ("remove", "×"),
        ] {
            let mut masks = layer.path_masks().to_vec();
            let enabled = !layer.locked()
                && (name != "up" || index > 0)
                && (name != "down" || index + 1 < masks.len());
            if enabled {
                match name {
                    "mode" => masks[index].mode = mask.mode.next(),
                    "invert" => masks[index].inverted = !mask.inverted,
                    "up" => masks.swap(index, index - 1),
                    "down" => masks.swap(index, index + 1),
                    _ => {
                        masks.remove(index);
                    }
                }
            }
            let state = state.clone();
            row = row.child(
                ui::text_button(
                    gpui::SharedString::from(format!("path-mask-{id}-{index}-{name}")),
                    label,
                )
                .px_1()
                .when(!enabled, |b| b.opacity(0.4))
                .when(enabled, |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::SetPathMasks {
                                    id,
                                    masks: masks.clone(),
                                }),
                                w,
                                cx,
                            )
                        });
                    })
                }),
            );
        }
        root = root.child(row);
    }
    let state = state.clone();
    root.child(
        ui::text_button("edit-mask-paths", "Edit mask paths (G)").on_click(move |_, w, cx| {
            state.update(cx, |s, cx| s.dispatch(&Action::SetTool(Tool::Pen), w, cx));
        }),
    )
}
