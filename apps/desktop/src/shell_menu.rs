//! Shared menu commands and keyboard navigation. Mouse and keyboard use the same entries.
use crate::editor::{Action, EditorState, PropertyFilter, Tool};
use libre_effects_core::Command;

/// Mouse activation must carry ownership captured before a menu button stole
/// focus. Existing open-menu navigation retains that owner; keyboard activation
/// uses its current focus. A missing/stale first mouse receipt fails closed.
pub(super) fn clipboard_menu_origin_allowed(pointer: bool, menu_open: bool, receipt: bool) -> bool {
    !pointer || menu_open || receipt
}

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
    Menu(&'static str),
    NewComposition,
    Settings,
    ResetWorkspace,
    Help,
    About,
    Search,
}
/// Import-only entry receipt. General menus keep their existing commit-on-open
/// behavior, but those commits cannot turn a pending source draft into implicit
/// permission to begin SVG import. A fresh clean entry clears the receipt.
#[derive(Default)]
pub(super) struct SvgImportEntry {
    pending_source: bool,
}
impl SvgImportEntry {
    pub(super) fn begin(&mut self, continuing_menu: bool, pending_source: bool) {
        if !continuing_menu {
            self.pending_source = pending_source;
        }
    }
    pub(super) fn finish(&mut self, target: &Target) -> bool {
        let allowed = !self.pending_source
            || !matches!(
                target,
                Target::Action(Action::ImportSvg | Action::RunScript | Action::ImportAeProject)
            );
        self.clear();
        allowed
    }
    pub(super) fn clear(&mut self) {
        self.pending_source = false;
    }
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
    if menu == "Open Recent" {
        let mut result = vec![Item::special("‹ File", "Left", Target::Menu("File"))];
        // Static keys keep command-search identities unchanged. Actual filenames
        // and full-path tooltips are derived only for this bounded submenu.
        for (number, path) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
            .into_iter()
            .zip(state.recent_projects.paths())
        {
            result.push(Item::special(
                number,
                "",
                Target::Action(Action::OpenRecent(path.clone())),
            ));
        }
        if state.recent_projects.paths().is_empty() {
            result.push(Item {
                label: "No recent projects",
                shortcut: "",
                target: None,
            });
        }
        result.push(Item {
            label: "Clear recent list",
            shortcut: "",
            target: (!state.recent_projects.paths().is_empty()
                || state.recent_history_needs_save())
            .then_some(Target::Action(Action::ClearRecentProjects(
                state.recent_projects.revision(),
            ))),
        });
        return result;
    }
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
            (
                "Run script (.jsx / .js)…",
                "",
                state.automation_available().then_some(Action::RunScript),
            ),
            ("Import footage…", "Ctrl+I", Some(Action::ImportImage)),
            (
                "Import AE project data…",
                "",
                state
                    .ae_import_available()
                    .then_some(Action::ImportAeProject),
            ),
            (
                "Import SVG as editable shapes…",
                "",
                state.svg_import_available().then_some(Action::ImportSvg),
            ),
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
            (
                "Copy selection",
                "Ctrl+C",
                (!state.shell_clipboard_blocked).then_some(Action::CopySelection),
            ),
            (
                "Cut selection",
                "Ctrl+X",
                (!state.shell_clipboard_blocked).then_some(Action::CutSelection),
            ),
            (
                "Paste",
                "Ctrl+V",
                (!state.shell_clipboard_blocked && state.contents_clipboard().is_none())
                    .then_some(Action::PasteSelection),
            ),
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
            .filter(|kind| {
                state
                    .editor
                    .selected_layer()
                    .is_none_or(|layer| layer.supports_effect_kind(*kind))
            })
            .map(|kind| {
                (
                    kind.label(),
                    "",
                    state
                        .editor
                        .selected_layer()
                        .filter(|l| !l.locked() && l.supports_effect_kind(kind))
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
        "Tools" => [
            ("Selection tool", "V", Tool::Select),
            ("Hand tool", "H", Tool::Hand),
            ("Zoom tool", "Z", Tool::Zoom),
            ("Rotation tool", "W", Tool::Rotate),
            ("Anchor Point tool", "Y", Tool::Anchor),
            ("Pen tool", "G", Tool::Pen),
            ("Text tool", "Ctrl+T", Tool::Text),
        ]
        .into_iter()
        .map(|(label, shortcut, tool)| (label, shortcut, Some(Action::SetTool(tool))))
        .collect(),
        "Preview" => vec![
            ("Play / Pause", "Space", Some(Action::Play)),
            ("Previous frame", "Page Up", Some(Action::Step(-1))),
            ("Next frame", "Page Down", Some(Action::Step(1))),
            ("Set work area start", "B", Some(Action::WorkStart)),
            ("Set work area end", "N", Some(Action::WorkEnd)),
            ("Toggle preview audio", "", Some(Action::PreviewAudio)),
            ("Toggle preview loop", "", Some(Action::PreviewLoop)),
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
        "File" => result.insert(
            2,
            Item::special("Open recent", "›", Target::Menu("Open Recent")),
        ),
        "Layer" => {
            let availability = state.layer_transform_availability();
            let transforms =
                crate::editor::layer_transform::ENTRIES
                    .into_iter()
                    .map(|(label, operation)| Item {
                        label,
                        shortcut: "",
                        target: availability
                            .allows(operation)
                            .then_some(Target::Action(Action::TransformLayers(operation))),
                    });
            result.splice(0..0, transforms);
        }
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
        "Help" => result.extend([
            Item::special("Find command…", "Ctrl+Shift+P", Target::Search),
            Item::special("Keyboard shortcuts", "", Target::Help),
            Item::special("About Libre Effects", "", Target::About),
        ]),
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
    let Some(i) = MENUS.iter().position(|m| *m == menu) else {
        return if forward { "File" } else { "Help" };
    };
    MENUS[(i + if forward { 1 } else { MENUS.len() - 1 }) % MENUS.len()]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub category: &'static str,
    pub label: &'static str,
}
pub struct Match {
    pub key: Key,
    pub item: Item,
}
pub fn search(query: &str, state: &EditorState) -> Vec<Match> {
    let query = query.trim().to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    let mut matches = vec![];
    for category in MENUS.into_iter().chain(["Tools", "Shape", "Preview"]) {
        for item in items(category, state) {
            if matches!(item.target, Some(Target::Search | Target::Menu(_))) {
                continue;
            }
            let label = item.label.to_lowercase();
            let haystack = format!("{category} {} {}", item.label, item.shortcut).to_lowercase();
            if words.iter().all(|word| haystack.contains(word)) {
                let rank = if label == query {
                    0
                } else if label.starts_with(&query) {
                    1
                } else if label.contains(&query) {
                    2
                } else {
                    3
                };
                matches.push((
                    rank,
                    Match {
                        key: Key {
                            category,
                            label: item.label,
                        },
                        item,
                    },
                ));
            }
        }
    }
    matches.sort_by_key(|(rank, entry)| (entry.item.target.is_none(), *rank));
    matches.into_iter().map(|(_, entry)| entry).collect()
}
pub fn search_step(results: &[Match], current: Option<Key>, forward: bool) -> Option<Key> {
    let count = results.len();
    if count == 0 {
        return None;
    }
    let start = results
        .iter()
        .position(|r| Some(r.key) == current)
        .unwrap_or(if forward { count - 1 } else { 0 });
    (1..=count)
        .map(|n| {
            if forward {
                (start + n) % count
            } else {
                (start + count - n) % count
            }
        })
        .find(|i| results[*i].item.target.is_some())
        .map(|i| results[i].key)
}
/// Resolve against current state, never dispatch an Action cached in a result row.
pub fn resolve(key: Key, state: &EditorState) -> Option<Target> {
    items(key.category, state)
        .into_iter()
        .find(|item| item.label == key.label)?
        .target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_menu_is_bounded_explicit_and_uses_exact_paths() {
        let root = tempfile::tempdir().unwrap();
        let mut state = EditorState::default();
        state.recent_projects = Default::default();
        let empty = items("Open Recent", &state);
        assert_eq!(empty.len(), 3);
        assert!(empty[1].target.is_none());
        assert!(empty[2].target.is_none());
        assert!(matches!(
            resolve(
                Key {
                    category: "File",
                    label: "Open recent"
                },
                &state
            ),
            Some(Target::Menu("Open Recent"))
        ));
        for i in 0..12 {
            state
                .recent_projects
                .remember(&root.path().join(format!("{i}/same.lep")));
        }
        let entries = items("Open Recent", &state);
        assert_eq!(entries.len(), 12);
        assert!(
            matches!(&entries[1].target, Some(Target::Action(Action::OpenRecent(path))) if *path == root.path().join("11/same.lep"))
        );
        assert!(
            matches!(&entries[10].target, Some(Target::Action(Action::OpenRecent(path))) if *path == root.path().join("2/same.lep"))
        );
        assert!(
            matches!(&entries[11].target, Some(Target::Action(Action::ClearRecentProjects(revision))) if *revision == state.recent_projects.revision())
        );
        // Navigation-only submenus never enter Find command or transfer its
        // focus/input receipts. Stored paths are not indexed by command search.
        assert!(search("Open recent", &state).is_empty());
        assert!(search("same.lep", &state).is_empty());
    }

    #[test]
    fn clipboard_menu_rejects_missing_mouse_owner_and_blocks_generic_targets() {
        assert!(!clipboard_menu_origin_allowed(true, false, false));
        assert!(clipboard_menu_origin_allowed(true, false, true));
        assert!(clipboard_menu_origin_allowed(true, true, false));
        assert!(clipboard_menu_origin_allowed(false, false, false));
        let mut state = EditorState::default();
        for blocked in [false, true, false] {
            state.shell_clipboard_blocked = blocked;
            for label in ["Copy selection", "Cut selection", "Paste"] {
                assert_eq!(
                    resolve(
                        Key {
                            category: "Edit",
                            label
                        },
                        &state
                    )
                    .is_some(),
                    !blocked
                );
                assert_eq!(
                    search(label, &state)
                        .iter()
                        .find(|entry| entry.key.label == label)
                        .unwrap()
                        .item
                        .target
                        .is_some(),
                    !blocked
                );
            }
            assert!(
                resolve(
                    Key {
                        category: "Edit",
                        label: "Copy layers"
                    },
                    &state
                )
                .is_some()
            );
        }
    }

    #[test]
    fn svg_import_entry_guard_covers_pointer_keyboard_search_and_reopen() {
        for route in ["pointer menu", "keyboard menu", "command search"] {
            let mut entry = SvgImportEntry::default();
            // Capture precedes the existing commit/blur for every entry route.
            entry.begin(false, true);
            assert!(!entry.finish(&Target::Action(Action::ImportSvg)), "{route}");
            entry.begin(false, false);
            assert!(entry.finish(&Target::Action(Action::ImportSvg)), "{route}");
        }
        let mut entry = SvgImportEntry::default();
        entry.begin(false, true);
        entry.begin(true, false); // Menu → command search retains the original receipt.
        assert!(!entry.finish(&Target::Action(Action::ImportSvg)));
        entry.begin(false, true);
        assert!(entry.finish(&Target::Action(Action::ImportImage))); // Other actions unchanged.
        assert!(entry.finish(&Target::Action(Action::ImportSvg)));
        entry.begin(false, true);
        entry.clear(); // Esc/Close must not leave import disabled.
        entry.begin(false, false);
        assert!(entry.finish(&Target::Action(Action::ImportSvg)));
    }

    #[test]
    fn svg_import_is_explicit_file_menu_and_search_command() {
        let mut state = EditorState::default();
        for enabled in [true, false] {
            state.saving = !enabled;
            let item = items("File", &state)
                .into_iter()
                .find(|item| item.label == "Import SVG as editable shapes…")
                .unwrap();
            assert!(item.shortcut.is_empty());
            assert_eq!(item.target.is_some(), enabled);
            let results = search("svg editable", &state);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].key.category, "File");
            assert_eq!(results[0].item.target.is_some(), enabled);
            if enabled {
                assert!(matches!(
                    resolve(results[0].key, &state),
                    Some(Target::Action(Action::ImportSvg))
                ));
            }
        }
    }

    #[test]
    fn about_is_a_dedicated_help_command_and_search_result() {
        let state = EditorState::default();
        let help = items("Help", &state);
        assert!(
            help.iter().any(|item| item.label == "Keyboard shortcuts"
                && matches!(item.target, Some(Target::Help)))
        );
        assert!(help.iter().any(|item| item.label == "About Libre Effects"
            && matches!(item.target, Some(Target::About))));
        let about = search("about", &state);
        assert_eq!(about.len(), 1);
        assert_eq!(about[0].key.category, "Help");
        assert!(matches!(resolve(about[0].key, &state), Some(Target::About)));
    }

    #[test]
    fn layer_transform_menu_and_search_share_availability_without_shortcuts() {
        let mut state = EditorState::default();
        for enabled in [false, true] {
            if enabled {
                state.editor.execute(Command::AddRectangle).unwrap();
                state.editor.execute(Command::AddRectangle).unwrap();
                state.selected_layers = [1, 2].into();
            }
            for (label, operation) in crate::editor::layer_transform::ENTRIES {
                let menu = items("Layer", &state)
                    .into_iter()
                    .find(|item| item.label == label)
                    .unwrap();
                let results = search(label, &state);
                let found = results
                    .iter()
                    .find(|entry| entry.key.label == label)
                    .unwrap();
                assert_eq!(menu.target.is_some(), enabled);
                assert_eq!(found.item.target.is_some(), enabled);
                assert_eq!(found.key.category, "Layer");
                assert!(menu.shortcut.is_empty());
                assert!(found.item.shortcut.is_empty());
                if enabled {
                    assert!(matches!(menu.target,
                        Some(Target::Action(Action::TransformLayers(actual))) if actual == operation));
                    assert!(matches!(resolve(found.key, &state),
                        Some(Target::Action(Action::TransformLayers(actual))) if actual == operation));
                } else {
                    assert!(resolve(found.key, &state).is_none());
                }
            }
        }
        state.editor.execute(Command::ToggleLocked(1)).unwrap();
        for (label, _) in crate::editor::layer_transform::ENTRIES {
            assert!(
                resolve(
                    Key {
                        category: "Layer",
                        label
                    },
                    &state
                )
                .is_none()
            );
        }
    }

    #[test]
    fn cached_layer_menu_and_search_targets_plan_fresh_ids_and_time() {
        use libre_effects_core::LayerTransformOp;
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state.editor.execute(Command::AddRectangle).unwrap();
        state.selected_layers = [1].into();
        let cached_menu = items("Layer", &state)
            .into_iter()
            .find(|item| item.label == "Flip Horizontal")
            .unwrap()
            .target
            .unwrap();
        let cached_search = search("layer flip horizontal", &state).remove(0);
        state.selected_layers = [2].into();
        state.frame = 27;
        for target in [
            cached_menu,
            cached_search.item.target.unwrap(),
            resolve(cached_search.key, &state).unwrap(),
        ] {
            let Target::Action(action) = target else {
                panic!("expected action")
            };
            assert!(matches!(action.layer_transform_command(&state),
                Some(Command::TransformLayers {
                    ids, frame: 27, operation: LayerTransformOp::FlipHorizontal
                }) if ids == vec![2]));
        }
        state.editor.execute(Command::ToggleLocked(2)).unwrap();
        assert!(resolve(cached_search.key, &state).is_none());
        state.editor.execute(Command::AddNull).unwrap();
        state.selected_layers = [1, 3].into();
        assert!(resolve(cached_search.key, &state).is_some());
        assert!(
            resolve(
                Key {
                    category: "Layer",
                    label: "Fit Layer Inside Composition"
                },
                &state
            )
            .is_none()
        );
        assert!(
            resolve(
                Key {
                    category: "Layer",
                    label: "Center Anchor in Source Bounds"
                },
                &state
            )
            .is_none()
        );
    }

    #[test]
    fn command_search_matches_words_categories_shortcuts_and_disabled_entries() {
        let state = EditorState::default();
        let results = search("  PROJECT   fonts ", &state);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key.label, "Manage project fonts…");
        assert_eq!(
            search("Ctrl+K", &state)[0].key.label,
            "Composition settings…"
        );
        assert!(matches!(
            search("tools pen", &state)[0].item.target,
            Some(Target::Action(Action::SetTool(Tool::Pen)))
        ));
        assert!(search("no-such-command 한글", &state).is_empty());
        let effects = search("effect blur", &state);
        assert_eq!(effects.len(), 1);
        assert!(effects[0].item.target.is_none());
        assert_eq!(search_step(&effects, None, true), None);
        let all = search("", &state);
        assert!(
            !all.iter()
                .any(|r| matches!(r.item.target, Some(Target::Search)))
        );
        let mut keys = std::collections::BTreeSet::new();
        for entry in &all {
            assert!(keys.insert((entry.key.category, entry.key.label)));
        }
        let first = search_step(&all, None, true);
        let last = search_step(&all, first, false);
        assert_eq!(search_step(&all, last, true), first);
        assert_eq!(adjacent("Shape", true), "File");
        assert_eq!(adjacent("Shape", false), "Help");
    }

    #[test]
    fn search_resolves_fresh_selection_and_effect_roundtrips_through_history_and_pixels() {
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        let key = search("effect gaussian", &state)[0].key;
        assert!(matches!(
            resolve(key, &state),
            Some(Target::Action(Action::Edit(Command::Effect { id: 1, .. })))
        ));
        state.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(resolve(key, &state).is_none());
        state.editor.execute(Command::AddRectangle).unwrap();
        assert!(matches!(
            resolve(key, &state),
            Some(Target::Action(Action::Edit(Command::Effect { id: 2, .. })))
        ));
        let before = state.editor.project().clone();
        let renderer = crate::rendering::Renderer::new();
        let original = renderer.render_preview(&before, 0, 480).unwrap();
        let Some(Target::Action(Action::Edit(command))) = resolve(key, &state) else {
            panic!("missing command");
        };
        state.editor.execute(command).unwrap();
        let after = state.editor.project().clone();
        let reopened = libre_effects_core::Project::from_json(&after.to_json().unwrap()).unwrap();
        assert_eq!(after, reopened);
        let preview = renderer.render_preview(&after, 0, 480).unwrap();
        assert_ne!(preview, original);
        assert_eq!(
            renderer
                .render_output(&reopened, 0, preview.width(), preview.height())
                .unwrap(),
            preview
        );
        state.editor.undo();
        assert_eq!(state.editor.project(), &before);
        state.editor.redo();
        assert_eq!(state.editor.project(), &after);
    }

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

#[cfg(test)]
mod luma_menu_tests {
    use super::*;
    use libre_effects_core::{AudioMetadata, Content, EffectEdit, EffectKind};

    #[test]
    fn luma_menu_requires_pixel_target_without_changing_existing_audio_effect_entries() {
        let audio = Content::Audio {
            path: "menu-fixture.wav".into(),
            audio: AudioMetadata {
                stream_index: 0,
                sample_rate: 48000,
                channels: 2,
                channel_layout: "stereo".into(),
                duration: 5.0,
                start_time: 0.0,
                file_offset: 0.0,
            },
            start_frame: 0,
            playback: Default::default(),
        };
        for (content, luma_allowed, legacy_allowed) in [
            (Content::Rectangle, true, true),
            (Content::Adjustment, true, true),
            (Content::Null, false, false),
            (audio, false, true),
        ] {
            let mut state = EditorState::default();
            state
                .editor
                .execute(Command::AddContent {
                    content,
                    width: 200.0,
                    height: 120.0,
                    name: "Effect menu fixture".into(),
                })
                .unwrap();
            let entries = items("Effect", &state);
            let luma = entries.iter().find(|i| i.label == "Luma Key").unwrap();
            assert_eq!(luma.target.is_some(), luma_allowed);
            assert!(
                entries
                    .iter()
                    .filter(|i| i.label != "Luma Key")
                    .all(|i| i.target.is_some() == legacy_allowed)
            );
            if luma_allowed {
                assert!(matches!(
                    &luma.target,
                    Some(Target::Action(Action::Edit(Command::Effect {
                        id: 1,
                        edit: EffectEdit::Add(EffectKind::LumaKey)
                    })))
                ));
            }
            state.editor.execute(Command::ToggleLocked(1)).unwrap();
            assert!(
                items("Effect", &state)
                    .into_iter()
                    .all(|i| i.target.is_none())
            );
        }
    }
}
