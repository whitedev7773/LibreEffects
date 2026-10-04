//! Transient Contents sibling selection and bounded hierarchy/permutation planning.
//! Nothing in this module changes source nodes, IDs, transforms, paint scope or history.
use libre_effects_core::{ContentsEdit, ContentsKind, ContentsNode, ShapeContents};
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
    /// A hierarchy edit or history step may move stable IDs beneath a collapsed
    /// ancestor. Reveal only a complete block whose common parent changed; ordinary
    /// disclosure still clears hidden descendants and never edits source data.
    pub fn reconcile_reparented(
        &mut self,
        contents: &ShapeContents,
        collapsed: &mut BTreeSet<u64>,
    ) {
        if let Some(rows) = bounded_rows(contents) {
            let parents: BTreeSet<_> = rows
                .iter()
                .filter(|(_, _, node)| self.items.contains(&node.id))
                .map(|(_, parent, _)| *parent)
                .collect();
            let found = rows
                .iter()
                .filter(|(_, _, n)| self.items.contains(&n.id))
                .count();
            if found == self.items.len() && found > 0 && parents.len() == 1 {
                let parent = *parents.first().unwrap();
                if self.parent != Some(parent) {
                    let mut ancestor = parent;
                    while ancestor != 0 {
                        collapsed.remove(&ancestor);
                        ancestor = rows
                            .iter()
                            .find(|(_, _, n)| n.id == ancestor)
                            .map(|(_, parent, _)| *parent)
                            .unwrap_or(0);
                    }
                    self.parent = Some(parent);
                }
            }
        }
        self.reconcile(contents, collapsed);
    }
    /// Never retain a multi-selection spanning different parents or descendants
    /// hidden by an explicit disclosure action.
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
pub(super) enum MoveDirection {
    Into,
    Out,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MovePlan {
    pub source_parent: u64,
    pub items: Vec<u64>,
    pub parent: u64,
    pub index: usize,
}
impl MovePlan {
    pub fn edit(&self) -> ContentsEdit {
        ContentsEdit::MoveSiblings {
            source_parent: self.source_parent,
            items: self.items.clone(),
            parent: self.parent,
            index: self.index,
        }
    }
}

/// Traverse only the actual Contents limits, including empty eighth-level groups.
/// Source schema, tracks and metadata are still validated by the atomic core edit.
fn bounded_rows(contents: &ShapeContents) -> Option<Vec<(usize, u64, &ContentsNode)>> {
    fn walk<'a>(
        nodes: &'a [ContentsNode],
        depth: usize,
        parent: u64,
        rows: &mut Vec<(usize, u64, &'a ContentsNode)>,
        ids: &mut BTreeSet<u64>,
    ) -> Option<()> {
        if depth > 8 {
            return None;
        }
        for node in nodes {
            if rows.len() >= 256 || node.id == 0 || !ids.insert(node.id) {
                return None;
            }
            rows.push((depth, parent, node));
            if let ContentsKind::Group(children) = &node.kind {
                walk(children, depth + 1, node.id, rows, ids)?;
            }
        }
        Some(())
    }
    let mut rows = Vec::new();
    walk(&contents.items, 0, 0, &mut rows, &mut BTreeSet::new())?;
    Some(rows)
}

/// The same plan serves singleton/multi buttons and Ctrl+arrows. No indices are
/// retained by the UI: execute against the current source after pending input.
pub(super) fn plan_move(
    contents: &ShapeContents,
    selection: &Selection,
    direction: MoveDirection,
) -> Option<MovePlan> {
    let rows = bounded_rows(contents)?;
    let source_parent = selection.parent?;
    let order = sibling_order(contents, source_parent)?;
    if selection.items.is_empty() || !selection.items.iter().all(|id| order.contains(id)) {
        return None;
    }
    let items: Vec<_> = order
        .iter()
        .copied()
        .filter(|id| selection.items.contains(id))
        .collect();
    let (parent, index) = match direction {
        MoveDirection::Into => {
            let first = order.iter().position(|id| selection.items.contains(id))?;
            let parent = *order.get(first.checked_sub(1)?)?;
            if selection.items.contains(&parent) {
                return None;
            }
            (parent, sibling_order(contents, parent)?.len())
        }
        MoveDirection::Out => {
            if source_parent == 0 {
                return None;
            }
            let parent = rows.iter().find(|(_, _, n)| n.id == source_parent)?.1;
            let destination = sibling_order(contents, parent)?;
            let index = destination.iter().position(|id| *id == source_parent)? + 1;
            (parent, index)
        }
    };
    let target_depth = if parent == 0 {
        0
    } else {
        rows.iter().find(|(_, _, n)| n.id == parent)?.0 + 1
    };
    for item in &items {
        let start = rows.iter().position(|(_, _, n)| n.id == *item)?;
        let source_depth = rows[start].0;
        for &(depth, _, node) in rows[start..].iter().take(1).chain(
            rows[start + 1..]
                .iter()
                .take_while(|(depth, _, _)| *depth > source_depth),
        ) {
            let moved_depth = target_depth + depth - source_depth;
            if node.id == parent
                || moved_depth > 8
                || (moved_depth == 8 && matches!(node.kind, ContentsKind::Group(_)))
            {
                return None;
            }
        }
    }
    Some(MovePlan {
        source_parent,
        items,
        parent,
        index,
    })
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
    MoveInto,
    MoveOut,
    Escape,
    CancelAndBubble,
    Consume,
}

impl TreeKey {
    pub fn move_direction(self, held: bool) -> Option<MoveDirection> {
        if held {
            return None;
        }
        match self {
            Self::MoveInto => Some(MoveDirection::Into),
            Self::MoveOut => Some(MoveDirection::Out),
            _ => None,
        }
    }
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
            if control && !shift && !alternate {
                TreeKey::MoveOut
            } else if plain && !shift {
                TreeKey::Collapse
            } else {
                TreeKey::Consume
            }
        }
        "right" => {
            if control && !shift && !alternate {
                TreeKey::MoveInto
            } else if plain && !shift {
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
    fn node(id: u64, children: Option<Vec<ContentsNode>>) -> ContentsNode {
        ContentsNode {
            id,
            name: format!("Item {id}"),
            enabled: true,
            kind: children
                .map(ContentsKind::Group)
                .unwrap_or(ContentsKind::Fill { even_odd: false }),
            composite: Default::default(),
            blend: Default::default(),
            parameters: Default::default(),
        }
    }
    fn hierarchy() -> ShapeContents {
        let mut c = ShapeContents::default();
        // Deliberately non-monotonic sibling IDs; a disabled empty destination is legal.
        c.items = vec![
            node(90, Some(vec![node(60, None)])),
            node(7, None),
            node(81, None),
            node(3, Some(vec![])),
            node(94, None),
        ];
        c.items[0].enabled = false;
        c
    }
    #[test]
    fn hierarchy_plans_singleton_and_noncontiguous_blocks_in_source_order() {
        let c = hierarchy();
        let mut selected = Selection::default();
        selected.all(0, &[94, 7, 3]);
        assert_eq!(
            plan_move(&c, &selected, MoveDirection::Into),
            Some(MovePlan {
                source_parent: 0,
                items: vec![7, 3, 94],
                parent: 90,
                index: 1,
            })
        );
        assert!(plan_move(&c, &selected, MoveDirection::Out).is_none());
        selected.one(0, 94);
        assert_eq!(
            plan_move(&c, &selected, MoveDirection::Into),
            Some(MovePlan {
                source_parent: 0,
                items: vec![94],
                parent: 3,
                index: 0,
            })
        );
        selected.one(0, 81); // The immediate preceding row is not a group.
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_none());
        selected.one(0, 90);
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_none());
        selected.all(0, &[90, 7]); // A selected group cannot be its own destination.
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_none());
        selected.all(90, &[60]);
        assert_eq!(
            plan_move(&c, &selected, MoveDirection::Out),
            Some(MovePlan {
                source_parent: 90,
                items: vec![60],
                parent: 0,
                index: 1,
            })
        );
    }
    #[test]
    fn nested_out_uses_grandparent_and_missing_or_mixed_siblings_reject() {
        let mut c = hierarchy();
        c.items.insert(
            1,
            node(
                50,
                Some(vec![
                    node(40, Some(vec![node(10, None), node(20, None)])),
                    node(30, None),
                ]),
            ),
        );
        let mut selected = Selection::default();
        selected.all(40, &[20, 10]);
        assert_eq!(
            plan_move(&c, &selected, MoveDirection::Out),
            Some(MovePlan {
                source_parent: 40,
                items: vec![10, 20],
                parent: 50,
                index: 1,
            })
        );
        for (parent, items) in [
            (40, vec![]),
            (40, vec![10, 30]),
            (99, vec![10]),
            (40, vec![99]),
            (7, vec![81]),
        ] {
            selected.all(parent, &items);
            for direction in [MoveDirection::Into, MoveDirection::Out] {
                assert!(plan_move(&c, &selected, direction).is_none());
            }
        }
    }
    #[test]
    fn hierarchy_planning_is_bounded_by_actual_node_and_empty_group_depth_limits() {
        fn chain(depth: usize) -> ContentsNode {
            let mut n = node(100, None);
            for i in (0..depth).rev() {
                n = node(101 + i as u64, Some(vec![n]));
            }
            n
        }
        let mut c = ShapeContents::default();
        c.items = vec![node(1, Some(vec![])), chain(8)];
        let mut selected = Selection::default();
        selected.one(0, 101);
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_none());
        c.items[1] = chain(7);
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_some());
        // A group at depth 8 is invalid even if it has no children.
        c.items[1] = node(100, Some(vec![]));
        for i in 0..8 {
            c.items[1] = node(101 + i, Some(vec![c.items[1].clone()]));
        }
        selected.one(0, 108);
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_none());
        c.items = vec![node(1, Some(vec![]))];
        c.items.extend((2..=256).map(|id| node(id, None)));
        selected.all(0, &[2, 256]);
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_some());
        c.items.push(node(257, None));
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_none());
        c.items.pop();
        c.items[2].id = 2;
        assert!(plan_move(&c, &selected, MoveDirection::Into).is_none());
    }
    #[test]
    fn reparented_selection_reveals_only_destination_ancestors_and_keeps_anchor_cursor() {
        let mut c = hierarchy();
        let mut selected = Selection::default();
        selected.all(0, &[7, 81]);
        selected.anchor = Some(81);
        selected.cursor = Some(7);
        let before = selected.clone();
        let mut collapsed = [90, 3].into();
        let plan = plan_move(&c, &selected, MoveDirection::Into).unwrap();
        // Planning/hover/failure never changes collapsed state or selection.
        assert_eq!(collapsed, [90, 3].into());
        assert_eq!(selected, before);
        let moved: Vec<_> = c
            .items
            .iter()
            .filter(|n| plan.items.contains(&n.id))
            .cloned()
            .collect();
        c.items.retain(|n| !plan.items.contains(&n.id));
        let ContentsKind::Group(children) = &mut c.items[0].kind else {
            panic!()
        };
        children.extend(moved);
        selected.reconcile_reparented(&c, &mut collapsed);
        assert_eq!(selected.parent, Some(90));
        assert_eq!(selected.items, before.items);
        assert_eq!(selected.anchor, before.anchor);
        assert_eq!(selected.cursor, before.cursor);
        assert_eq!(selected.singleton(), None);
        assert_eq!(collapsed, [3].into());
        collapsed.insert(90); // Explicit collapse still clears hidden descendants.
        selected.reconcile_reparented(&c, &mut collapsed);
        assert!(selected.items.is_empty());
        assert!(collapsed.contains(&90));
    }
    #[test]
    fn history_reparents_stable_ids_and_expands_an_old_source_without_selection_history() {
        let before = hierarchy();
        let mut moved = before.clone();
        let item = moved.items.remove(1);
        let ContentsKind::Group(children) = &mut moved.items[0].kind else {
            panic!()
        };
        children.push(item);
        let mut selected = Selection::default();
        selected.one(0, 7);
        let mut collapsed = [90, 3].into();
        selected.reconcile_reparented(&moved, &mut collapsed);
        assert_eq!(selected.parent, Some(90));
        assert_eq!(selected.singleton(), Some(7));
        selected.reconcile_reparented(&before, &mut collapsed); // Undo.
        assert_eq!(selected.parent, Some(0));
        assert_eq!(selected.singleton(), Some(7));
        collapsed.insert(90);
        selected.reconcile_reparented(&moved, &mut collapsed); // Redo.
        assert_eq!(selected.parent, Some(90));
        assert!(!collapsed.contains(&90));
        assert!(collapsed.contains(&3));
        assert_eq!(selected.anchor, Some(7));
        assert_eq!(selected.cursor, Some(7));
    }
    #[test]
    fn hierarchy_chords_are_exact_and_plain_arrows_keep_disclosure_ownership() {
        for (key, hierarchy, plain) in [
            ("right", TreeKey::MoveInto, TreeKey::Expand),
            ("left", TreeKey::MoveOut, TreeKey::Collapse),
        ] {
            assert_eq!(
                tree_key(key, true, false, false, false, true, false),
                Some(hierarchy)
            );
            assert!(hierarchy.move_direction(false).is_some());
            assert!(hierarchy.move_direction(true).is_none());
            assert!(plain.move_direction(false).is_none());
            assert_eq!(
                tree_key(key, false, false, false, false, true, false),
                Some(plain)
            );
            for (shift, alt, other) in [
                (true, false, false),
                (false, true, false),
                (false, false, true),
                (true, true, true),
            ] {
                assert_eq!(
                    tree_key(key, true, shift, alt, other, true, false),
                    Some(TreeKey::Consume)
                );
            }
            assert_eq!(tree_key(key, true, false, false, false, false, false), None);
            assert_eq!(tree_key(key, true, false, false, false, true, true), None);
        }
    }

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
