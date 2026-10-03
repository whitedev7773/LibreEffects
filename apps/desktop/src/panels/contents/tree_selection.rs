//! Transient, same-parent Contents selection and block-permutation planning.
//! Nothing in this module changes source nodes, IDs, transforms, paint scope or history.
use libre_effects_core::{ContentsKind, ShapeContents};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Selection {
    pub parent: Option<u64>,
    pub items: BTreeSet<u64>,
    pub anchor: Option<u64>,
    pub cursor: Option<u64>,
}
impl Selection {
    pub fn singleton(&self) -> Option<u64> {
        (self.items.len() == 1).then(|| *self.items.first().unwrap())
    }
    pub fn one(&mut self, parent: u64, item: u64) {
        self.parent = Some(parent);
        self.items = [item].into();
        self.anchor = Some(item);
        self.cursor = Some(item);
    }
    pub fn click(&mut self, parent: u64, order: &[u64], item: u64, toggle: bool, range: bool) {
        if !order.contains(&item) {
            return;
        }
        self.cursor = Some(item);
        if self.parent != Some(parent) {
            self.one(parent, item);
            return;
        }
        if range
            && let Some(a) = self
                .anchor
                .and_then(|a| order.iter().position(|id| *id == a))
        {
            let b = order.iter().position(|id| *id == item).unwrap();
            if !toggle {
                self.items.clear();
            }
            self.items.extend(&order[a.min(b)..=a.max(b)]);
        } else if toggle {
            if !self.items.remove(&item) {
                self.items.insert(item);
            }
            self.anchor = Some(item);
        } else {
            self.one(parent, item);
        }
    }
    /// A selected row keeps its whole block until release establishes a plain click.
    pub fn press(&mut self, parent: u64, order: &[u64], item: u64, toggle: bool, range: bool) {
        if toggle || range || self.parent != Some(parent) || !self.items.contains(&item) {
            self.click(parent, order, item, toggle, range);
        }
    }
    pub fn all(&mut self, parent: u64, order: &[u64]) {
        self.parent = Some(parent);
        self.cursor = order.first().copied();
        self.items = order.iter().copied().collect();
        if self.anchor.is_none_or(|id| !self.items.contains(&id)) {
            self.anchor = order.first().copied();
        }
    }
    /// Keep singleton controls working after an explicit indent/outdent; never keep a
    /// multi-selection spanning newly different parents or hidden descendants.
    pub fn reconcile(&mut self, contents: &ShapeContents, collapsed: &BTreeSet<u64>) {
        let visible = visible_rows(contents, collapsed);
        let had_items = !self.items.is_empty();
        self.items
            .retain(|id| visible.iter().any(|(_, _, n)| n == id));
        if (had_items && self.items.is_empty())
            || self.parent.is_some_and(|p| {
                p != 0 && (collapsed.contains(&p) || !visible.iter().any(|(_, _, id)| *id == p))
            })
        {
            *self = Self::default();
            return;
        }
        let parents: BTreeSet<_> = visible
            .iter()
            .filter(|(_, _, id)| self.items.contains(id))
            .map(|(_, parent, _)| *parent)
            .collect();
        if parents.len() > 1 {
            *self = Self::default();
            return;
        }
        if let Some(parent) = parents.first() {
            self.parent = Some(*parent);
        }
        if self.cursor.is_some_and(|id| !self.items.contains(&id)) {
            self.cursor = self.items.first().copied();
        }
        if self.anchor.is_some_and(|a| {
            !sibling_order(contents, self.parent.unwrap_or(0)).is_some_and(|v| v.contains(&a))
        }) {
            self.anchor = self.items.first().copied();
        }
    }
}

pub(super) fn sibling_order(contents: &ShapeContents, parent: u64) -> Option<Vec<u64>> {
    let nodes = if parent == 0 {
        &contents.items
    } else {
        let ContentsKind::Group(nodes) = &contents.node(parent)?.kind else {
            return None;
        };
        nodes
    };
    Some(nodes.iter().map(|n| n.id).collect())
}

pub(super) fn visible_rows(
    contents: &ShapeContents,
    collapsed: &BTreeSet<u64>,
) -> Vec<(usize, u64, u64)> {
    let mut hidden_depth = None;
    contents
        .rows()
        .into_iter()
        .filter_map(|(depth, parent, node)| {
            if hidden_depth.is_some_and(|d| depth > d) {
                return None;
            }
            hidden_depth = None;
            if collapsed.contains(&node.id) && matches!(node.kind, ContentsKind::Group(_)) {
                hidden_depth = Some(depth);
            }
            Some((depth, parent, node.id))
        })
        .collect()
}

/// `gap` is a boundary in the original immediate-child order, including 0 and len.
/// Remove selected siblings first, then insert their original-order block at that gap.
/// Identity permutations intentionally produce no command and preserve Redo.
pub(super) fn plan_order(order: &[u64], selected: &BTreeSet<u64>, gap: usize) -> Option<Vec<u64>> {
    if selected.is_empty() || gap > order.len() {
        return None;
    }
    let unique: BTreeSet<_> = order.iter().copied().collect();
    if unique.len() != order.len() || !selected.is_subset(&unique) {
        return None;
    }
    let insertion = order[..gap]
        .iter()
        .filter(|id| !selected.contains(id))
        .count();
    let block = order.iter().copied().filter(|id| selected.contains(id));
    let mut result: Vec<_> = order
        .iter()
        .copied()
        .filter(|id| !selected.contains(id))
        .collect();
    result.splice(insertion..insertion, block);
    (result != order).then_some(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TreeKey {
    SelectAll,
    Duplicate,
    Delete,
    Previous,
    Next,
    Collapse,
    Expand,
    Escape,
    CancelAndBubble,
    Consume,
}

pub(super) fn tree_key(
    key: &str,
    control: bool,
    shift: bool,
    alt: bool,
    other_modifier: bool,
    focused: bool,
    composing: bool,
) -> Option<TreeKey> {
    if !focused || composing {
        return None;
    }
    if control && key == "z" {
        return Some(TreeKey::CancelAndBubble);
    }
    // Reserve the finite selection-sensitive shell domains that the Contents
    // tree does not implement. In particular, a tree Cut must never cut a layer.
    if (control && matches!(key, "c" | "x" | "v"))
        || (control && alt && key == "t")
        || (!control && alt && matches!(key, "[" | "]"))
    {
        return Some(TreeKey::Consume);
    }
    let alternate = alt || other_modifier;
    let plain = !control && !alternate;
    Some(match key {
        "escape" => TreeKey::Escape,
        "a" if control => {
            if !shift && !alternate {
                TreeKey::SelectAll
            } else {
                TreeKey::Consume
            }
        }
        "d" if control => {
            if !shift && !alternate {
                TreeKey::Duplicate
            } else {
                TreeKey::Consume
            }
        }
        "delete" | "backspace" => {
            if plain && !shift {
                TreeKey::Delete
            } else {
                TreeKey::Consume
            }
        }
        "up" => {
            if plain {
                TreeKey::Previous
            } else {
                TreeKey::Consume
            }
        }
        "down" => {
            if plain {
                TreeKey::Next
            } else {
                TreeKey::Consume
            }
        }
        "left" => {
            if plain && !shift {
                TreeKey::Collapse
            } else {
                TreeKey::Consume
            }
        }
        "right" => {
            if plain && !shift {
                TreeKey::Expand
            } else {
                TreeKey::Consume
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Content, ContentsEdit, Editor};
    #[test]
    fn modifiers_ranges_follow_actual_siblings_not_ids_or_descendants() {
        let order = [81, 7, 62, 3, 94];
        let mut s = Selection::default();
        s.click(17, &order, 7, false, false);
        assert_eq!(s.singleton(), Some(7));
        s.click(17, &order, 3, true, false);
        assert_eq!(s.items, [7, 3].into());
        assert_eq!(s.singleton(), None);
        s.click(17, &order, 81, false, true);
        assert_eq!(s.items, [81, 7, 62, 3].into());
        s.click(17, &order, 94, true, true);
        assert_eq!(s.items, order.into());
        s.click(90, &[45, 2, 16], 2, true, true);
        assert_eq!(s.singleton(), Some(2));
        assert_eq!(s.parent, Some(90));
        s.click(90, &[45, 2, 16], 2, true, false);
        assert!(s.items.is_empty());
        assert_eq!(s.singleton(), None);
        s.all(90, &[45, 2, 16]);
        assert_eq!(s.items, [45, 2, 16].into());
        s.all(90, &[]);
        assert_eq!(s.singleton(), None);
    }
    #[test]
    fn selected_press_preserves_block_unselected_press_replaces_it() {
        let mut s = Selection::default();
        s.all(0, &[81, 7]);
        s.press(0, &[81, 7, 62], 7, false, false);
        assert_eq!(s.items, [81, 7].into());
        s.press(0, &[81, 7, 62], 62, false, false);
        assert_eq!(s.singleton(), Some(62));
        s.click(0, &[81, 7, 62], 7, false, false);
        assert_eq!(s.singleton(), Some(7));
    }
    #[test]
    fn block_plan_handles_original_gaps_boundaries_and_arbitrary_ids() {
        let order = [81, 7, 62, 3, 94];
        assert_eq!(
            plan_order(&order, &[7, 3].into(), 5),
            Some(vec![81, 62, 94, 7, 3])
        );
        assert_eq!(
            plan_order(&order, &[7, 3].into(), 0),
            Some(vec![7, 3, 81, 62, 94])
        );
        for gap in [1, 2, 3] {
            assert_eq!(plan_order(&order, &[7, 62].into(), gap), None);
        }
        assert_eq!(plan_order(&order, &order.into(), 5), None);
        assert_eq!(plan_order(&order, &BTreeSet::new(), 0), None);
        assert_eq!(plan_order(&order, &[99].into(), 0), None);
        assert_eq!(plan_order(&order, &[7].into(), 6), None);
        assert_eq!(plan_order(&[7, 7], &[7].into(), 0), None);
    }
    #[test]
    fn collapsed_groups_are_one_visible_item_and_selection_never_changes_source() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Default::default()),
            width: 100.,
            height: 100.,
            name: "Test".into(),
        })
        .unwrap();
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Promote,
        })
        .unwrap();
        let before = e.project().clone();
        let Content::ShapeContents(c) = e.selected_layer().unwrap().content() else {
            panic!()
        };
        let group = c.items[0].id;
        let order = sibling_order(c, group).unwrap();
        let mut s = Selection::default();
        s.all(group, &order);
        s.reconcile(c, &BTreeSet::new());
        assert_eq!(s.items.len(), order.len());
        let collapsed = [group].into();
        assert_eq!(visible_rows(c, &collapsed), vec![(0, 0, group)]);
        s.reconcile(c, &collapsed);
        assert!(s.items.is_empty());
        assert_eq!(s.parent, None); // Ctrl+A cannot select a now-hidden child list.
        s.one(0, group);
        s.reconcile(c, &collapsed);
        assert_eq!(s.singleton(), Some(group));
        s.one(group, order[0]);
        s.click(group, &order, order[0], true, false);
        assert!(s.items.is_empty());
        s.reconcile(c, &collapsed);
        assert_eq!(s.parent, None);
        assert_eq!(s.anchor, None);
        assert_eq!(e.project(), &before);
        assert_eq!(e.project().to_json().unwrap(), before.to_json().unwrap());
    }
    #[test]
    fn tree_shortcuts_require_exact_focus_and_do_not_steal_ime() {
        for key in [
            "delete",
            "backspace",
            "up",
            "down",
            "left",
            "right",
            "a",
            "d",
            "z",
            "escape",
        ] {
            assert!(tree_key(key, true, false, false, false, false, false).is_none());
            assert!(tree_key(key, true, false, false, false, true, true).is_none());
        }
        assert_eq!(
            tree_key("a", true, false, false, false, true, false),
            Some(TreeKey::SelectAll)
        );
        assert_eq!(
            tree_key("a", true, true, false, false, true, false),
            Some(TreeKey::Consume)
        );
        assert_eq!(
            tree_key("d", true, true, false, false, true, false),
            Some(TreeKey::Consume)
        );
        assert_eq!(
            tree_key("delete", false, false, true, false, true, false),
            Some(TreeKey::Consume)
        );
        assert_eq!(
            tree_key("up", true, false, false, false, true, false),
            Some(TreeKey::Consume)
        );
        assert_eq!(
            tree_key("z", true, true, false, false, true, false),
            Some(TreeKey::CancelAndBubble)
        );
        assert_eq!(tree_key("a", false, false, false, false, true, false), None);
    }
}
