use crate::{
    editor::{Action, EditorState},
    ui,
};
use gpui::{Div, Entity, SharedString, div, prelude::*, px};
use libre_effects_core::{Command, Layer, PathTarget, PropertyPath, TrackEdit};

pub(super) fn row(
    state: &Entity<EditorState>,
    layer: &Layer,
    target: PathTarget,
    frame: u32,
) -> Div {
    let property = PropertyPath::Path(target);
    let track = layer.track(property).expect("path control target");
    let id = layer.id();
    let key = |suffix: &str| SharedString::from(format!("path-{id}-{target:?}-{suffix}"));
    div()
        .flex()
        .items_center()
        .h(px(27.0))
        .child(ui::action_tool(
            key("watch"),
            "stopwatch",
            "Toggle Path animation",
            state,
            Action::Edit(Command::EditTrack {
                id,
                property,
                edit: TrackEdit::ToggleAnimation { frame },
            }),
            !track.keys().is_empty(),
        ))
        .child(ui::action_tool(
            key("key"),
            "diamond",
            "Add or remove Path keyframe",
            state,
            Action::Edit(Command::EditTrack {
                id,
                property,
                edit: TrackEdit::ToggleKey { frame },
            }),
            track.keys().contains_key(&frame),
        ))
        .child(div().flex_1().child("Path"))
        .child(ui::action_tool(
            key("pen"),
            "pen",
            "Edit path at current time",
            state,
            Action::GraphProperty(id, property),
            false,
        ))
}
