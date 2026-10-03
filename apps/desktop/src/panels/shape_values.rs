use crate::{
    editor::{Action, EditorState},
    ui,
};
use gpui::{Entity, IntoElement, SharedString};
use libre_effects_core::{
    Command, Frame, LayerId, PropertyPath, Shape, ShapePaint, ShapeParam, TrackEdit,
};

pub(super) fn color_watch(
    state: &Entity<EditorState>,
    shape: &Shape,
    id: LayerId,
    paint: ShapePaint,
    frame: Frame,
) -> impl IntoElement {
    let animated = shape.paint_color_animated(paint);
    let command = shape.paint_color_animation_command(id, paint, frame);
    ui::action_tool(
        SharedString::from(format!("shape-{paint:?}-color-watch")),
        "stopwatch",
        "Toggle color animation",
        state,
        Action::Edit(command),
        animated,
    )
}

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
