//! Target checks for commands originating in a filtered Timeline.
//!
//! This module never changes shared selection. Graph, Inspector and canvas
//! edits retain their own ownership; callers opt in only for Timeline actions.

use crate::editor::Action;
use gpui::KeyDownEvent;

pub(super) use crate::timeline_filter::{TargetScope, blocks_hidden_targets};

/// Classify selection-based commands, not explicit commands from other panels.
/// Apply this only after establishing that the Timeline owns the invocation.
pub(super) fn action_scope(action: &Action) -> Option<TargetScope> {
    match action {
        Action::CopySelection | Action::CutSelection | Action::DeleteSelection => {
            Some(TargetScope::Selection)
        }
        Action::CopyKeys => Some(TargetScope::Keys),
        Action::PasteSelection | Action::PasteKeys => Some(TargetScope::Paste),
        Action::CopyLayers
        | Action::DuplicateSelection
        | Action::SplitSelection
        | Action::TrimSelection(_)
        | Action::NudgeSelection(..)
        | Action::TransformLayers(_)
        | Action::PrecomposeSelection
        | Action::ToggleSelectedSwitch(_)
        | Action::ToggleTimeRemap
        | Action::FreezeTimeRemap => Some(TargetScope::Layers),
        // PasteLayers creates new layers, and explicit Edit commands retain the
        // owning panel's validated targets rather than using shared selection.
        _ => None,
    }
}

/// Mirror Timeline-owned editing keys and the selection shortcuts that bubble
/// to Shell::key. This deliberately leaves transport, tools and view keys alone.
pub(super) fn shortcut_scope(event: &KeyDownEvent) -> Option<TargetScope> {
    let key = event.keystroke.key.as_str();
    let m = event.keystroke.modifiers;
    if key == "delete" {
        // Timeline currently handles Delete regardless of modifier state.
        return Some(TargetScope::Selection);
    }
    if key == "f9" && !event.is_held && !m.alt && !m.platform && (!m.control || m.shift) {
        return Some(TargetScope::Keys);
    }
    if m.control {
        match key {
            "c" if m.shift => Some(TargetScope::Layers),
            "c" | "x" => Some(TargetScope::Selection),
            "v" => Some(TargetScope::Paste),
            "d" => Some(TargetScope::Layers),
            "t" if m.alt => Some(TargetScope::Layers),
            _ => None,
        }
    } else if m.alt {
        matches!(key, "[" | "]").then_some(TargetScope::Layers)
    } else {
        match key {
            "left" | "right" | "up" | "down" => Some(TargetScope::Layers),
            "backspace" => Some(TargetScope::Selection),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, KeyRef, LayerId, LayerTransformOp, Property};

    fn event(key: &str) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: gpui::Keystroke::parse(key).unwrap(),
            is_held: false,
        }
    }

    fn key(id: LayerId) -> KeyRef {
        KeyRef {
            id,
            property: Property::PositionX.into(),
            frame: 10,
        }
    }

    #[test]
    fn shortcut_scope_matches_layer_edits_even_when_scalar_keys_are_selected() {
        for shortcut in [
            "left",
            "shift-right",
            "up",
            "shift-down",
            "ctrl-d",
            "ctrl-shift-d",
            "ctrl-shift-c",
            "alt-[",
            "alt-]",
            "ctrl-alt-t",
        ] {
            assert_eq!(shortcut_scope(&event(shortcut)), Some(TargetScope::Layers));
        }
        for shortcut in ["delete", "backspace", "ctrl-c", "ctrl-x"] {
            assert_eq!(
                shortcut_scope(&event(shortcut)),
                Some(TargetScope::Selection)
            );
        }
        for shortcut in ["f9", "shift-f9", "ctrl-shift-f9"] {
            assert_eq!(shortcut_scope(&event(shortcut)), Some(TargetScope::Keys));
        }
        assert_eq!(shortcut_scope(&event("ctrl-v")), Some(TargetScope::Paste));
    }

    #[test]
    fn navigation_creation_and_view_shortcuts_do_not_become_selection_edits() {
        for shortcut in [
            "ctrl-a",
            "ctrl-f",
            "ctrl-s",
            "ctrl-z",
            "ctrl-shift-z",
            "ctrl-y",
            "ctrl-alt-y",
            "ctrl-t",
            "ctrl-f9",
            "alt-f9",
            "home",
            "end",
            "pageup",
            "pagedown",
            "space",
            "escape",
            "shift-f3",
            "p",
            "u",
            "j",
            "k",
        ] {
            assert_eq!(shortcut_scope(&event(shortcut)), None, "{shortcut}");
        }
        let mut held_ease = event("f9");
        held_ease.is_held = true;
        assert_eq!(shortcut_scope(&held_ease), None);
    }

    #[test]
    fn action_scope_does_not_intercept_explicit_graph_inspector_or_canvas_edits() {
        assert_eq!(
            action_scope(&Action::DeleteSelection),
            Some(TargetScope::Selection)
        );
        assert_eq!(action_scope(&Action::CopyKeys), Some(TargetScope::Keys));
        assert_eq!(action_scope(&Action::PasteKeys), Some(TargetScope::Paste));
        for action in [
            Action::DuplicateSelection,
            Action::SplitSelection,
            Action::TrimSelection(true),
            Action::NudgeSelection(1.0, 0.0),
            Action::TransformLayers(LayerTransformOp::FlipHorizontal),
            Action::PrecomposeSelection,
            Action::ToggleTimeRemap,
            Action::FreezeTimeRemap,
        ] {
            assert_eq!(action_scope(&action), Some(TargetScope::Layers));
        }
        for action in [
            Action::Edit(Command::RemoveLayer(2)),
            Action::Edit(Command::MoveKeys {
                keys: vec![key(2)],
                delta: 1,
            }),
            Action::Select(2),
            Action::GraphProperty(2, Property::PositionX.into()),
            Action::PasteLayers,
            Action::PreviousKey,
            Action::NextKey,
            Action::Undo,
            Action::Redo,
        ] {
            assert_eq!(action_scope(&action), None);
        }
    }
}
