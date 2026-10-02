//! Shared menu commands and keyboard navigation. Mouse and keyboard use the same entries.
use crate::editor::{Action, EditorState, PropertyFilter, Tool};
use libre_effects_core::Command;

pub const MENUS: [&str; 9] = [
    "File",
    "Edit",
    "Composition",
    "Layer",
    "Effect",
    "Animation",
    "View",
    "Window",
    "Help",
];
#[derive(Clone)]
pub enum Target {
    Action(Action),
    NewComposition,
    Settings,
    ResetWorkspace,
    Help,
}
pub struct Item {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub target: Option<Target>,
}
impl Item {
    fn special(label: &'static str, shortcut: &'static str, target: Target) -> Self {
        Self {
            label,
            shortcut,
            target: Some(target),
        }
    }
}
pub fn items(menu: &str, state: &EditorState) -> Vec<Item> {
    let selected = state.editor.selected();
    let items: Vec<(&str, &str, Option<Action>)> = match menu {
        "File" => vec![
            ("New project", "Ctrl+Alt+N", Some(Action::New)),
            ("Open project…", "Ctrl+O", Some(Action::Open)),
            ("Save", "Ctrl+S", Some(Action::Save)),
            ("Save as…", "Ctrl+Shift+S", Some(Action::SaveAs)),
            ("Collect project files…", "", Some(Action::CollectFiles)),
            (
                "Cancel file collection",
                "",
                state.collecting.then_some(Action::CancelCollection),
            ),
            ("Import footage…", "Ctrl+I", Some(Action::ImportImage)),
            (
                "Import image sequence…",
                "",
                Some(Action::ImportImageSequence),
            ),
            ("Import video…", "Ctrl+Shift+I", Some(Action::ImportVideo)),
            ("Manage project media…", "", Some(Action::ManageMedia)),
            ("Manage project fonts…", "", Some(Action::ManageFonts)),
            ("Relink selected video…", "", Some(Action::RelinkVideo)),
            ("Refresh footage", "", Some(Action::RefreshFootage)),
            (
                "Export current frame (PNG, alpha)…",
                "",
                Some(Action::ExportFrame),
            ),
            (
                "Export current frame (PNG, background)…",
                "",
                Some(Action::ExportFrameBackground),
            ),
            (
                "Render work area (PNG sequence, alpha)…",
                "",
                Some(Action::ExportSequence),
            ),
            (
                "Render work area (PNG sequence, background)…",
                "",
                Some(Action::ExportSequenceBackground),
            ),
            (
                "Render work area — MP4…",
                "",
                Some(Action::ExportVideo(crate::video_export::VideoPreset::H264)),
            ),
            (
                "Render work area — MOV with alpha…",
                "",
                Some(Action::ExportVideo(
                    crate::video_export::VideoPreset::ProResAlpha,
                )),
            ),
            ("Cancel render", "", Some(Action::CancelExport)),
        ],
        "Edit" => vec![
            ("Copy selection", "Ctrl+C", Some(Action::CopySelection)),
            ("Cut selection", "Ctrl+X", Some(Action::CutSelection)),
            ("Paste", "Ctrl+V", Some(Action::PasteSelection)),
            ("Copy layers", "", Some(Action::CopyLayers)),
            ("Paste layers", "", Some(Action::PasteLayers)),
            ("Undo", "Ctrl+Z", Some(Action::Undo)),
            ("Redo", "Ctrl+Shift+Z", Some(Action::Redo)),
            (
                "Duplicate layer",
                "Ctrl+D",
                selected.map(|_| Action::DuplicateSelection),
            ),
            ("Delete selection", "Delete", Some(Action::DeleteSelection)),
            (
                "Split layers",
                "Ctrl+Shift+D",
                selected.map(|_| Action::SplitSelection),
            ),
        ],
        "Layer" => vec![
            (
                if state
                    .editor
                    .selected_layer()
                    .is_some_and(|l| l.time_remap().is_some())
                {
                    "Disable Time Remapping"
                } else {
                    "Enable Time Remapping"
                },
                "Ctrl+Alt+T",
                state
                    .editor
                    .selected_layer()
                    .filter(|l| l.can_time_remap() && !l.locked())
                    .map(|_| Action::ToggleTimeRemap),
            ),
            (
                "Freeze frame with Time Remap",
                "",
                state
                    .editor
                    .selected_layer()
                    .filter(|l| l.can_time_remap() && !l.locked())
                    .map(|_| Action::FreezeTimeRemap),
            ),
            (
                "Pre-compose selection",
                "Ctrl+Shift+C",
                selected.map(|_| Action::PrecomposeSelection),
            ),
            (
                "Trim In to playhead",
                "Alt+[",
                selected.map(|_| Action::TrimSelection(true)),
            ),
            (
                "Trim Out to playhead",
                "Alt+]",
                selected.map(|_| Action::TrimSelection(false)),
            ),
            ("New text", "", Some(Action::AddText)),
            ("New null object", "", Some(Action::Edit(Command::AddNull))),
            ("New solid", "Ctrl+Y", Some(Action::Edit(Command::AddSolid))),
            (
                "New adjustment layer",
                "Ctrl+Alt+Y",
                Some(Action::Edit(Command::AddAdjustment)),
            ),
            (
                "Solo selected layers",
                "",
                selected
                    .map(|_| Action::ToggleSelectedSwitch(libre_effects_core::LayerSwitch::Solo)),
            ),
            (
                "Shy selected layers",
                "",
                selected
                    .map(|_| Action::ToggleSelectedSwitch(libre_effects_core::LayerSwitch::Shy)),
            ),
            (
                "Guide selected layers",
                "",
                selected
                    .map(|_| Action::ToggleSelectedSwitch(libre_effects_core::LayerSwitch::Guide)),
            ),
            (
                "New background solid",
                "",
                Some(Action::Edit(Command::AddBackgroundSolid)),
            ),
            (
                "New rectangle",
                "",
                Some(Action::Edit(Command::AddRectangle)),
            ),
            (
                "Duplicate layer",
                "Ctrl+D",
                selected.map(|_| Action::DuplicateSelection),
            ),
            (
                "Toggle visibility",
                "",
                selected.map(|id| Action::Edit(Command::ToggleVisible(id))),
            ),
            (
                "Toggle lock",
                "",
                selected.map(|id| Action::Edit(Command::ToggleLocked(id))),
            ),
        ],
        "Shape" => libre_effects_core::ShapeKind::ALL
            .into_iter()
            .map(|kind| (kind.label(), "Q", Some(Action::SetTool(Tool::Shape(kind)))))
            .collect(),
        "Effect" => libre_effects_core::EffectKind::ALL
            .into_iter()
            .map(|kind| {
                (
                    kind.label(),
                    "",
                    state
                        .editor
                        .selected_layer()
                        .filter(|l| {
                            !l.locked() && !matches!(l.content(), libre_effects_core::Content::Null)
                        })
                        .map(|l| {
                            Action::Edit(Command::Effect {
                                id: l.id(),
                                edit: libre_effects_core::EffectEdit::Add(kind),
                            })
                        }),
                )
            })
            .collect(),
        "Window" => vec![(
            "Render Queue",
            "",
            Some(Action::Queue(crate::editor::queue::QueueAction::Show(true))),
        )],
        "Animation" => vec![
            ("Toggle Graph Editor", "Shift+F3", Some(Action::ToggleGraph)),
            ("Previous keyframe", "J", Some(Action::PreviousKey)),
            ("Next keyframe", "K", Some(Action::NextKey)),
            (
                "Reveal animated properties",
                "U",
                Some(Action::Filter(Some(PropertyFilter::Animated))),
            ),
            ("Reveal all properties", "", Some(Action::Filter(None))),
        ],
        "View" => vec![
            ("Fit composition", "", Some(Action::FitPreview)),
            ("Zoom in", "", Some(Action::ZoomPreview(2.0))),
            ("Zoom out", "", Some(Action::ZoomPreview(0.5))),
            ("Transparency grid", "", Some(Action::Checkerboard)),
            (
                "Rulers",
                "Ctrl+R",
                Some(Action::ViewerOption(
                    crate::viewer_tools::ViewOption::Rulers,
                )),
            ),
            (
                "Grid",
                "",
                Some(Action::ViewerOption(crate::viewer_tools::ViewOption::Grid)),
            ),
            (
                "Guides",
                "",
                Some(Action::ViewerOption(
                    crate::viewer_tools::ViewOption::Guides,
                )),
            ),
            (
                "Title / Action Safe",
                "",
                Some(Action::ViewerOption(crate::viewer_tools::ViewOption::Safe)),
            ),
            (
                "Snap to guides",
                "",
                Some(Action::ViewerOption(
                    crate::viewer_tools::ViewOption::SnapGuides,
                )),
            ),
            (
                "Snap to grid",
                "",
                Some(Action::ViewerOption(
                    crate::viewer_tools::ViewOption::SnapGrid,
                )),
            ),
            (
                "Lock guides",
                "",
                Some(Action::ViewerOption(
                    crate::viewer_tools::ViewOption::LockGuides,
                )),
            ),
            ("Clear guides", "", Some(Action::ClearGuides)),
        ],
        _ => Vec::new(),
    };

    let mut result: Vec<_> = items
        .into_iter()
        .map(|(label, shortcut, action)| Item {
            label,
            shortcut,
            target: action.map(Target::Action),
        })
        .collect();
    match menu {
        "Composition" => result.extend([
            Item::special(
                "Add to Render Queue",
                "Ctrl+M",
                Target::Action(Action::Queue(crate::editor::queue::QueueAction::Add)),
            ),
            Item::special("New composition", "Ctrl+N", Target::NewComposition),
            Item::special(
                "Duplicate composition",
                "",
                Target::Action(Action::Edit(Command::DuplicateComposition)),
            ),
            Item::special(
                "Delete composition",
                "",
                Target::Action(Action::Edit(Command::DeleteComposition)),
            ),
            Item::special("Composition settings…", "Ctrl+K", Target::Settings),
        ]),
        "Window" => result.push(Item::special(
            "Reset default workspace",
            "",
            Target::ResetWorkspace,
        )),
        "Help" => result.push(Item::special("Keyboard shortcuts", "", Target::Help)),
        _ => {}
    }
    result
}

/// Wrap through enabled entries. Disabled items never become keyboard targets.
pub fn step(items: &[Item], current: Option<usize>, forward: bool) -> Option<usize> {
    let len = items.len();
    if len == 0 {
        return None;
    }
    let start = current
        .filter(|i| *i < len)
        .unwrap_or(if forward { len - 1 } else { 0 });
    (1..=len)
        .map(|n| {
            if forward {
                (start + n) % len
            } else {
                (start + len - n) % len
            }
        })
        .find(|i| items[*i].target.is_some())
}
pub fn initial(items: &[Item], last: bool) -> Option<usize> {
    step(items, None, !last)
}
pub fn letter(items: &[Item], current: Option<usize>, key: &str) -> Option<usize> {
    if key.chars().count() != 1 || !key.chars().all(char::is_alphanumeric) {
        return current;
    }
    let len = items.len();
    if len == 0 {
        return None;
    }
    let start = current.filter(|i| *i < len).unwrap_or(len - 1);
    (1..=len)
        .map(|n| (start + n) % len)
        .find(|i| {
            items[*i].target.is_some()
                && items[*i]
                    .label
                    .to_lowercase()
                    .starts_with(&key.to_lowercase())
        })
        .or(current)
}
pub fn adjacent(menu: &str, forward: bool) -> &'static str {
    let i = MENUS.iter().position(|m| *m == menu).unwrap_or(0);
    MENUS[(i + if forward { 1 } else { MENUS.len() - 1 }) % MENUS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_wraps_skips_disabled_and_cycles_matching_letters() {
        let entries = vec![
            Item {
                label: "Unavailable",
                shortcut: "",
                target: None,
            },
            Item::special("Save", "", Target::Help),
            Item {
                label: "Save disabled",
                shortcut: "",
                target: None,
            },
            Item::special("Save as", "", Target::Help),
            Item::special("Open", "", Target::Help),
        ];
        assert_eq!(initial(&entries, false), Some(1));
        assert_eq!(initial(&entries, true), Some(4));
        assert_eq!(step(&entries, Some(1), true), Some(3));
        assert_eq!(step(&entries, Some(1), false), Some(4));
        assert_eq!(step(&entries, Some(4), true), Some(1));
        assert_eq!(letter(&entries, Some(1), "S"), Some(3));
        assert_eq!(letter(&entries, Some(3), "s"), Some(1));
        assert_eq!(letter(&entries, Some(3), "x"), Some(3));
        assert_eq!(letter(&entries, Some(3), "delete"), Some(3));
        assert_eq!(step(&entries[..1], None, true), None);
        assert_eq!(initial(&[], false), None);
        assert_eq!(adjacent("Help", true), "File");
        assert_eq!(adjacent("File", false), "Help");
    }

    #[test]
    fn every_menu_has_shared_targets_and_availability_tracks_the_document() {
        let mut state = EditorState::default();
        assert_eq!(items("File", &state)[0].label, "New project");
        for menu in MENUS.into_iter().chain(["Shape"]) {
            let list = items(menu, &state);
            assert!(!list.is_empty(), "{menu}");
            assert!(list.iter().all(|i| !i.label.is_empty()));
        }
        assert!(items("Effect", &state).iter().all(|i| i.target.is_none()));
        state.editor.execute(Command::AddRectangle).unwrap();
        assert!(items("Effect", &state).iter().all(|i| i.target.is_some()));
        state.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(items("Effect", &state).iter().all(|i| i.target.is_none()));
        let composition = items("Composition", &state);
        assert!(matches!(
            composition[1].target,
            Some(Target::NewComposition)
        ));
        assert!(matches!(composition[4].target, Some(Target::Settings)));
        let cancel = |state: &EditorState| {
            items("File", state)
                .into_iter()
                .find(|i| i.label == "Cancel file collection")
                .unwrap()
                .target
        };
        assert!(cancel(&state).is_none());
        state.collecting = true;
        assert!(matches!(
            cancel(&state),
            Some(Target::Action(Action::CancelCollection))
        ));
    }

    #[test]
    fn menu_edit_commands_use_document_history_and_roundtrip() {
        let mut state = EditorState::default();
        let before = state.editor.project().clone();
        let add = items("Layer", &state)
            .into_iter()
            .find(|i| i.label == "New rectangle")
            .unwrap();
        let Some(Target::Action(Action::Edit(command))) = add.target else {
            panic!("missing edit target");
        };
        state.editor.execute(command).unwrap();
        let after = state.editor.project().clone();
        assert_eq!(after.composition().layers().len(), 1);
        assert_eq!(
            libre_effects_core::Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        state.editor.redo();
        assert_eq!(state.editor.project(), &after);
    }
}
