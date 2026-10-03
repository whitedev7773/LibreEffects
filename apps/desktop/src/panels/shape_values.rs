use crate::{
    editor::{Action, EditorState},
    ui,
};
use gpui::{Entity, IntoElement, SharedString};
use libre_effects_core::{Command, Frame, LayerId, PropertyPath, Shape, ShapeParam, TrackEdit};

pub(super) fn watch(
    state: &Entity<EditorState>,
    shape: &Shape,
    id: LayerId,
    parameter: ShapeParam,
    frame: Frame,
) -> impl IntoElement {
    ui::action_tool(
        SharedString::from(format!("shape-{parameter:?}-watch")),
        "stopwatch",
        "Toggle shape property animation",
        state,
        Action::Edit(Command::EditTrack {
            id,
            property: PropertyPath::Shape(parameter),
            edit: TrackEdit::ToggleAnimation { frame },
        }),
        shape
            .parameters
            .get(&parameter)
            .is_some_and(|t| !t.keys().is_empty()),
    )
}
