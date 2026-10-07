//! Session-only Project rows, type filters and displayed-order navigation.

use libre_effects_core::{Content, FolderId, Project, ProjectItem};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemType {
    #[default]
    All,
    Compositions,
    Footage,
    Folders,
}

impl ItemType {
    pub const ALL: [Self; 4] = [Self::All, Self::Compositions, Self::Footage, Self::Folders];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All types",
            Self::Compositions => "Compositions",
            Self::Footage => "Footage",
            Self::Folders => "Folders",
        }
    }

    pub fn matches(self, item: ProjectItem) -> bool {
        self == Self::All
            || matches!(
                (self, item),
                (Self::Compositions, ProjectItem::Composition(_))
                    | (Self::Footage, ProjectItem::Asset(_))
                    | (Self::Folders, ProjectItem::Folder(_))
            )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub item: ProjectItem,
    pub name: String,
    pub kind: &'static str,
    pub folder: Option<FolderId>,
    pub depth: usize,
}
/// Build the visible Project rows without changing source or session selection.
/// With no filters, sorted siblings retain hierarchy and folder collapse. A name
/// or type filter returns flat matches, including inside collapsed folders, and
/// preserves each match's parent folder for display context. Name/kind search is
/// OR; the text criterion and item type are AND.
pub fn rows(
    project: &Project,
    query: &str,
    item_type: ItemType,
    by_type: bool,
    descending: bool,
    collapsed: &BTreeSet<FolderId>,
) -> Vec<Row> {
    let library = project.asset_library();
    let mut items: Vec<_> = library
        .folders()
        .iter()
        .map(|(id, f)| Row {
            item: ProjectItem::Folder(*id),
            name: f.name().into(),
            kind: "Folder",
            folder: f.parent(),
            depth: 0,
        })
        .chain(library.assets().iter().map(|(id, a)| Row {
            item: ProjectItem::Asset(*id),
            name: a.name().into(),
            kind: match a.content() {
                Content::Video { audio: Some(_), .. } => "Video + Audio",
                Content::Video { .. } => "Video",
                Content::Audio { .. } => "Audio",
                Content::ImageSequence { .. } => "Sequence",
                _ => "Image",
            },
            folder: a.folder(),
            depth: 0,
        }))
        .chain(project.compositions().into_iter().map(|(id, c)| Row {
            item: ProjectItem::Composition(id),
            name: c.name().into(),
            kind: "Comp",
            folder: library.composition_folder(id),
            depth: 0,
        }))
        .collect();
    items.sort_by(|a, b| {
        let order = (if by_type {
            a.kind.cmp(b.kind)
        } else {
            std::cmp::Ordering::Equal
        })
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.item.cmp(&b.item));
        if descending { order.reverse() } else { order }
    });
    let query = query.trim().to_lowercase();
    if !query.is_empty() || item_type != ItemType::All {
        return items
            .into_iter()
            .filter(|r| {
                item_type.matches(r.item)
                    && (query.is_empty()
                        || r.name.to_lowercase().contains(&query)
                        || r.kind.to_lowercase().contains(&query))
            })
            .collect();
    }
    fn visit(
        items: &[Row],
        parent: Option<FolderId>,
        depth: usize,
        collapsed: &BTreeSet<FolderId>,
        output: &mut Vec<Row>,
    ) {
        for row in items.iter().filter(|r| r.folder == parent) {
            let mut row = row.clone();
            row.depth = depth;
            let item = row.item;
            output.push(row);
            if let ProjectItem::Folder(id) = item {
                if !collapsed.contains(&id) {
                    visit(items, Some(id), depth + 1, collapsed, output);
                }
            }
        }
    }
    let mut output = Vec::new();
    visit(&items, None, 0, collapsed, &mut output);
    output
}
pub fn folder_path(project: &Project, mut folder: Option<FolderId>) -> String {
    let mut parts = Vec::new();
    for _ in 0..32 {
        let Some(f) = folder.and_then(|id| project.asset_library().folders().get(&id)) else {
            break;
        };
        parts.push(f.name().to_string());
        folder = f.parent();
    }
    parts.reverse();
    if parts.is_empty() {
        "Project".into()
    } else {
        parts.join(" / ")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    Previous,
    Next,
    First,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabDirection {
    Previous,
    Next,
}

/// Project-local Tab accepts only a fresh Tab/Shift+Tab press. The desktop
/// supplies source-draft, IME and gesture ownership; a plain search query does
/// not own focus and may be left without changing its text.
pub fn tab_direction(
    shift: bool,
    other_modifier: bool,
    held: bool,
    input_busy: bool,
) -> Option<TabDirection> {
    if other_modifier || held || input_busy {
        None
    } else if shift {
        Some(TabDirection::Previous)
    } else {
        Some(TabDirection::Next)
    }
}

/// Only unmodified navigation belongs to the Project browser. The desktop
/// checks focus, drafts and gesture ownership before applying these shortcuts.
pub fn shortcut(
    key: &str,
    control: bool,
    shift: bool,
    alt: bool,
    platform: bool,
    function: bool,
) -> Option<Navigation> {
    if control || shift || alt || platform || function {
        return None;
    }
    match key {
        "up" => Some(Navigation::Previous),
        "down" => Some(Navigation::Next),
        "home" => Some(Navigation::First),
        "end" => Some(Navigation::Last),
        _ => None,
    }
}

/// Prevent Project-owned key events from editing a retained Timeline selection.
/// The desktop checks text/IME ownership first; global document shortcuts remain
/// available. Shift and other modifiers do not opt these editing keys back in.
pub fn blocks_layer_shortcut(key: &str, control: bool, alt: bool) -> bool {
    matches!(key, "left" | "right" | "delete" | "backspace")
        || (control && matches!(key, "a" | "c" | "x" | "v" | "d"))
        || (control && alt && key == "t")
        || (alt && matches!(key, "[" | "]"))
}

/// Select in the exact visible/sorted row order, clamping at both endpoints.
/// A missing or hidden current item starts at the first row for Next/First and
/// the last row for Previous/Last; no item is selected when the view is empty.
pub fn select(
    rows: &[Row],
    current: Option<ProjectItem>,
    navigation: Navigation,
) -> Option<ProjectItem> {
    let last = rows.len().checked_sub(1)?;
    let current = current.and_then(|item| rows.iter().position(|row| row.item == item));
    let target = match navigation {
        Navigation::Previous => current.map_or(last, |index| index.saturating_sub(1)),
        Navigation::Next => current.map_or(0, |index| (index + 1).min(last)),
        Navigation::First => 0,
        Navigation::Last => last,
    };
    Some(rows[target].item)
}

#[cfg(test)]
#[path = "project_browser_tests.rs"]
mod tests;
