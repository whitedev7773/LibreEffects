use super::{tree_selection::*, *};
use gpui::{KeyDownEvent, MouseDownEvent, Point};
use libre_effects_core::{CompositionId, Project};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
pub(super) struct RowBounds {
    pub parent: u64,
    pub bounds: Bounds<Pixels>,
    pub visible: Bounds<Pixels>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Gap {
    pub item: u64,
    pub after: bool,
    pub index: usize,
}

pub(super) struct Drag {
    project: Project,
    revision: u64,
    composition: CompositionId,
    layer: u64,
    frame: u32,
    tool: crate::editor::Tool,
    gradient_controls: Option<crate::color_edit::GradientTarget>,
    selected_layers: BTreeSet<u64>,
    selection: Selection,
    collapsed: BTreeSet<u64>,
    order: Vec<u64>,
    pressed: u64,
    origin: Point<Pixels>,
    plain: bool,
    pub moved: bool,
    pub gap: Option<Gap>,
}
fn blocked(s: &EditorState) -> bool {
    s.colors.session.is_some()
        || s.gradient_editor.is_some()
        || s.vertex_editor.is_some()
        || s.gradient_preview.is_some()
        || s.text_session.is_some()
        || s.media_open
        || s.fonts_open
        || s.queue_open
        || s.recovery.is_some()
        || s.new_composition_requested
        || s.close_after_save
        || s.playing
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyRoute {
    Bubble,
    Consume,
    Handle,
}
fn key_route(key: TreeKey, s: &EditorState) -> KeyRoute {
    let modal = s.colors.session.is_some()
        || s.gradient_editor.is_some()
        || s.vertex_editor.is_some()
        || s.media_open
        || s.fonts_open
        || s.recovery.is_some()
        || s.close_after_save; // Shell Escape also cancels a confirmed pending close.
    // Queue display, pending composition requests, playback, live gradient
    // previews and canvas text sessions are not shell modal shortcut owners.
    if key == TreeKey::CancelAndBubble || (key == TreeKey::Escape && modal) {
        KeyRoute::Bubble
    } else if blocked(s) || key == TreeKey::Consume {
        // Inactive editing (especially playback) must never turn a Contents key
        // into a whole-layer action in the shell. Modal fields have separate focus.
        KeyRoute::Consume
    } else {
        KeyRoute::Handle
    }
}
impl Drag {
    fn new(
        s: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        pressed: u64,
        origin: Point<Pixels>,
        plain: bool,
    ) -> Option<Self> {
        if blocked(s) || !selection.items.contains(&pressed) {
            return None;
        }
        let layer = s.editor.selected_layer()?;
        if layer.locked() {
            return None;
        }
        let Content::ShapeContents(contents) = layer.content() else {
            return None;
        };
        let order = sibling_order(contents, selection.parent?)?;
        if !selection.items.iter().all(|id| order.contains(id)) {
            return None;
        }
        Some(Self {
            project: s.editor.project().clone(),
            revision: s.document_revision,
            composition: s.editor.project().active_composition_id(),
            layer: layer.id(),
            frame: s.frame,
            tool: s.tool,
            gradient_controls: s.gradient_controls,
            selected_layers: s.selected_layers.clone(),
            selection: selection.clone(),
            collapsed: collapsed.clone(),
            order,
            pressed,
            origin,
            plain,
            moved: false,
            gap: None,
        })
    }
    pub fn current(
        &self,
        s: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
    ) -> bool {
        !blocked(s)
            && self.revision == s.document_revision
            && self.composition == s.editor.project().active_composition_id()
            && s.editor.selected() == Some(self.layer)
            && s.frame == self.frame
            && s.tool == self.tool
            && s.gradient_controls == self.gradient_controls
            && s.selected_layers == self.selected_layers
            && s.contents_selection == selection.singleton().map(|item| (self.composition, self.layer, item))
            && self.selection == *selection
            && self.collapsed == *collapsed
            // Ordinary edits and Undo do not increment document_revision.
            && s.editor.project() == &self.project
            && s.editor.selected_layer().is_some_and(|l| !l.locked())
    }
    fn update(&mut self, position: Point<Pixels>, rows: &BTreeMap<u64, RowBounds>) {
        let delta = position - self.origin;
        self.moved |= f32::from(delta.x).hypot(f32::from(delta.y)) >= 4.;
        self.gap = self
            .moved
            .then(|| gap_at(position, self.selection.parent.unwrap(), &self.order, rows))
            .flatten();
    }
    fn release(
        &mut self,
        position: Point<Pixels>,
        rows: &BTreeMap<u64, RowBounds>,
    ) -> Option<Command> {
        // Always use the release coordinates, including a fast down/up without a move event.
        self.update(position, rows);
        let gap = self.gap?;
        let order = plan_order(&self.order, &self.selection.items, gap.index)?;
        Some(Command::Contents {
            id: self.layer,
            edit: ContentsEdit::Reorder {
                parent: self.selection.parent?,
                order,
            },
        })
    }
}
fn gap_at(
    position: Point<Pixels>,
    parent: u64,
    order: &[u64],
    rows: &BTreeMap<u64, RowBounds>,
) -> Option<Gap> {
    let (&item, row) = rows
        .iter()
        .find(|(_, row)| row.visible.contains(&position))?;
    if row.parent != parent {
        return None;
    }
    let after = position.y >= row.bounds.top() + row.bounds.size.height / 2.;
    let index = order.iter().position(|id| *id == item)? + usize::from(after);
    Some(Gap { item, after, index })
}
/// An expanded group travels with its subtree: the after marker belongs below its last visible descendant.
pub(super) fn marker_row(gap: Gap, visible: &[(usize, u64, u64)]) -> Option<(u64, bool)> {
    let index = visible.iter().position(|(_, _, item)| *item == gap.item)?;
    let depth = visible[index].0;
    let last = if gap.after {
        visible[index + 1..]
            .iter()
            .take_while(|(d, _, _)| *d > depth)
            .last()
            .map(|(_, _, id)| *id)
            .unwrap_or(gap.item)
    } else {
        gap.item
    };
    Some((last, gap.after))
}

impl ContentsControls {
    pub(super) fn reconcile_tree(&mut self, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        let revision = s.document_revision;
        let composition = s.editor.project().active_composition_id();
        let current = s.editor.selected_layer().and_then(|layer| {
            let Content::ShapeContents(contents) = layer.content() else {
                return None;
            };
            Some((layer.id(), contents.clone()))
        });
        let owner = current.as_ref().map(|(id, _)| (composition, *id));
        let reset = self.owner != owner || self.owner_revision != revision;
        let previous = self.selection.clone();
        if reset {
            self.tree_drag = None;
            self.selection = Selection::default();
            self.collapsed.clear();
            self.owner = owner;
            self.owner_revision = revision;
            self.add_open = false;
            self.paint_menu = None;
            self.gradient_stop = None;
        } else if let Some((_, contents)) = &current {
            self.selection.reconcile(contents, &self.collapsed);
        }
        let expected = owner.and_then(|(composition, layer)| {
            self.selection
                .singleton()
                .map(|item| (composition, layer, item))
        });
        let publish = reset || previous != self.selection || s.contents_selection != expected;
        if publish {
            self.publish_tree_selection(cx);
        }
    }
    pub(super) fn publish_tree_selection(&mut self, cx: &mut Context<Self>) {
        self.tree_drag = None;
        if let Some(item) = self.selection.singleton()
            && let Some((_, layer)) = self.owner
        {
            self.select(layer, item, cx);
            return;
        }
        self.cancel_ramp(cx);
        self.paint_menu = None;
        self.ramp_selected = None;
        self.gradient_stop = None;
        self.selected = None;
        self.fields.clear();
        self.name = None;
        self.add_open = false;
        self.state.update(cx, |s, cx| {
            s.contents_selection = None;
            if matches!(
                s.gradient_controls,
                Some(crate::color_edit::GradientTarget::Contents(..))
            ) {
                s.gradient_controls = None;
            }
            cx.notify();
        });
        cx.notify();
    }
    pub(super) fn tree_down(
        &mut self,
        layer: u64,
        parent: u64,
        item: u64,
        e: &MouseDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let m = e.modifiers;
        if m.alt || m.platform || m.function || blocked(self.state.read(cx)) {
            return;
        }
        TextField::commit_active(w, cx);
        self.reconcile_tree(cx);
        let Some(order) = self.state.read(cx).editor.selected_layer().and_then(|l| {
            if l.id() != layer {
                return None;
            }
            let Content::ShapeContents(contents) = l.content() else {
                return None;
            };
            sibling_order(contents, parent)
        }) else {
            return;
        };
        w.focus(&self.tree_focus);
        if self
            .state
            .read(cx)
            .editor
            .selected_layer()
            .is_some_and(|l| l.locked())
        {
            self.selection
                .click(parent, &order, item, m.control, m.shift);
        } else {
            self.selection
                .press(parent, &order, item, m.control, m.shift);
        }
        self.publish_tree_selection(cx);
        self.tree_drag = Drag::new(
            self.state.read(cx),
            &self.selection,
            &self.collapsed,
            item,
            e.position,
            !m.control && !m.shift,
        );
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn tree_move(
        &mut self,
        e: &gpui::MouseMoveEvent,
        w: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = &mut self.tree_drag else {
            return;
        };
        if !e.dragging()
            || !w.is_window_active()
            || !self.tree_focus.is_focused(w)
            || !drag.current(self.state.read(cx), &self.selection, &self.collapsed)
        {
            self.tree_drag = None;
        } else {
            drag.update(e.position, &self.tree_rows.borrow());
        }
        cx.notify();
    }
    pub(super) fn tree_up(
        &mut self,
        position: Point<Pixels>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.tree_drag.take() else {
            return;
        };
        if w.is_window_active()
            && self.tree_focus.is_focused(w)
            && drag.current(self.state.read(cx), &self.selection, &self.collapsed)
        {
            let command = drag.release(position, &self.tree_rows.borrow());
            if !drag.moved && drag.plain {
                self.selection
                    .one(drag.selection.parent.unwrap(), drag.pressed);
                self.publish_tree_selection(cx);
            } else if let Some(command) = command {
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::Edit(command), w, cx));
            }
        }
        cx.notify();
    }
    pub(super) fn tree_collapse(&mut self, item: u64, cx: &mut Context<Self>) {
        self.tree_drag = None;
        if !self.collapsed.remove(&item) {
            self.collapsed.insert(item);
        }
        self.reconcile_tree(cx);
        cx.notify();
    }
    pub(super) fn tree_key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let m = e.keystroke.modifiers;
        let key = e.keystroke.key.to_lowercase();
        let Some(key) = tree_key(
            &key,
            m.control,
            m.shift,
            m.alt,
            m.platform || m.function,
            // Disclosure/visibility buttons belong to this tree; editor TextFields
            // are deliberately outside it and retain their own shortcuts and IME.
            self.tree_focus.contains_focused(w, cx),
            TextField::is_composing(w, cx),
        ) else {
            return;
        };
        self.tree_drag = None;
        let route = key_route(key, self.state.read(cx));
        cx.notify();
        if route == KeyRoute::Bubble {
            return;
        }
        cx.stop_propagation();
        if route == KeyRoute::Consume {
            return;
        }
        self.reconcile_tree(cx);
        let Some(layer) = self.state.read(cx).editor.selected_layer().cloned() else {
            return;
        };
        let Content::ShapeContents(contents) = layer.content() else {
            return;
        };
        let visible = visible_rows(contents, &self.collapsed);
        let parent = self.selection.parent.unwrap_or(0);
        let order = sibling_order(contents, parent).unwrap_or_default();
        let mut edit = None;
        match key {
            TreeKey::SelectAll => {
                self.selection.all(parent, &order);
                self.publish_tree_selection(cx);
            }
            TreeKey::Delete | TreeKey::Duplicate if !layer.locked() && !e.is_held => {
                if let Some(item) = self.selection.singleton() {
                    edit = Some(if key == TreeKey::Delete {
                        ContentsEdit::Remove(item)
                    } else {
                        ContentsEdit::Duplicate(item)
                    });
                }
            }
            TreeKey::Previous | TreeKey::Next => {
                // Shift navigation stays among siblings, just like Shift-click.
                let ids: Vec<_> = if m.shift {
                    order
                } else {
                    visible.iter().map(|(_, _, id)| *id).collect()
                };
                if !ids.is_empty() {
                    let index = self
                        .selection
                        .cursor
                        .and_then(|id| ids.iter().position(|n| *n == id));
                    let index = match (key, index) {
                        (TreeKey::Previous, Some(i)) => i.saturating_sub(1),
                        (TreeKey::Next, Some(i)) => (i + 1).min(ids.len() - 1),
                        _ => 0,
                    };
                    let item = ids[index];
                    let parent = visible
                        .iter()
                        .find(|(_, _, id)| *id == item)
                        .map(|(_, p, _)| *p)
                        .unwrap_or(parent);
                    let order = sibling_order(contents, parent).unwrap_or_default();
                    self.selection.click(parent, &order, item, false, m.shift);
                    self.publish_tree_selection(cx);
                }
            }
            TreeKey::Collapse | TreeKey::Expand => {
                if let Some(item) = self.selection.singleton()
                    && contents
                        .node(item)
                        .is_some_and(|n| matches!(n.kind, ContentsKind::Group(_)))
                    && self.collapsed.contains(&item) == (key == TreeKey::Expand)
                {
                    self.tree_collapse(item, cx);
                }
            }
            TreeKey::Escape => {
                self.add_open = false;
            }
            _ => {}
        }
        if let Some(edit) = edit {
            self.state.update(cx, |s, cx| {
                s.dispatch(
                    &Action::Edit(Command::Contents {
                        id: layer.id(),
                        edit,
                    }),
                    w,
                    cx,
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;
    fn scene() -> EditorState {
        let mut s = EditorState::default();
        s.editor
            .execute(Command::AddContent {
                content: Content::Shape(Default::default()),
                width: 100.,
                height: 100.,
                name: "Tree".into(),
            })
            .unwrap();
        edit(&mut s, ContentsEdit::Promote);
        for parent in [0, 0, 1, 1] {
            edit(
                &mut s,
                ContentsEdit::Add {
                    parent,
                    kind: ContentsKind::Group(vec![]),
                },
            );
        }
        s.editor.clear_history();
        s
    }
    fn edit(s: &mut EditorState, edit: ContentsEdit) {
        s.editor.execute(Command::Contents { id: 1, edit }).unwrap();
    }
    fn contents(s: &EditorState) -> &libre_effects_core::ShapeContents {
        let Content::ShapeContents(c) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        c
    }
    fn selection(s: &mut EditorState, parent: u64, items: &[u64]) -> Selection {
        let mut selection = Selection::default();
        selection.all(parent, items);
        s.contents_selection = selection
            .singleton()
            .map(|id| (s.editor.project().active_composition_id(), 1, id));
        selection
    }
    fn rows(order: &[u64], parent: u64) -> BTreeMap<u64, RowBounds> {
        order
            .iter()
            .enumerate()
            .map(|(i, &item)| {
                let bounds =
                    Bounds::new(point(px(10.), px(i as f32 * 30.)), size(px(200.), px(30.)));
                (
                    item,
                    RowBounds {
                        parent,
                        bounds,
                        visible: bounds,
                    },
                )
            })
            .collect()
    }
    #[test]
    fn root_and_nested_drags_only_commit_final_release_gap_once() {
        for parent in [0, 1] {
            let mut s = scene();
            let mut order = sibling_order(contents(&s), parent).unwrap();
            order.reverse();
            edit(
                &mut s,
                ContentsEdit::Reorder {
                    parent,
                    order: order.clone(),
                },
            );
            s.editor.clear_history();
            let moved = [order[0], order[order.len() - 2]];
            let selected = selection(&mut s, parent, &moved);
            let before = s.editor.project().clone();
            let map = rows(&order, parent);
            let origin = point(px(50.), px(10.));
            let mut drag =
                Drag::new(&s, &selected, &BTreeSet::new(), order[0], origin, true).unwrap();
            // Hover can move through several gaps but is wholly transient.
            for y in [10., 45., 75.] {
                drag.update(point(px(50.), px(y)), &map);
            }
            assert_eq!(s.editor.project(), &before);
            assert!(!s.editor.can_undo());
            let command = drag
                .release(point(px(50.), px(order.len() as f32 * 30. - 2.)), &map)
                .unwrap();
            s.editor.execute(command).unwrap();
            let after = s.editor.project().clone();
            let mut expected: Vec<_> = order
                .iter()
                .copied()
                .filter(|id| !selected.items.contains(id))
                .collect();
            expected.extend(
                order
                    .iter()
                    .copied()
                    .filter(|id| selected.items.contains(id)),
            );
            assert_eq!(sibling_order(contents(&s), parent), Some(expected));
            s.editor.undo();
            assert_eq!(s.editor.project(), &before);
            assert!(!s.editor.can_undo());
            s.editor.redo();
            assert_eq!(s.editor.project(), &after);
        }
    }
    #[test]
    fn fast_release_uses_current_pointer_and_cancel_noop_preserve_redo() {
        let mut s = scene();
        let order = sibling_order(contents(&s), 1).unwrap();
        edit(
            &mut s,
            ContentsEdit::Rename {
                item: order[0],
                name: "Redo me".into(),
            },
        );
        let redo = s.editor.project().clone();
        s.editor.undo();
        let selected = selection(&mut s, 1, &[order[1]]);
        let before = s.editor.project().clone();
        let map = rows(&order, 1);
        let origin = point(px(50.), px(40.));
        let mut drag = Drag::new(&s, &selected, &BTreeSet::new(), order[1], origin, true).unwrap();
        assert!(drag.release(point(px(50.), px(58.)), &map).is_none()); // Original after gap.
        assert!(drag.moved);
        let mut canceled =
            Drag::new(&s, &selected, &BTreeSet::new(), order[1], origin, true).unwrap();
        canceled.update(point(px(50.), px(148.)), &map);
        drop(canceled);
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        s.editor.redo();
        assert_eq!(s.editor.project(), &redo);
        s.editor.undo();
        let mut fast = Drag::new(&s, &selected, &BTreeSet::new(), order[1], origin, true).unwrap();
        // No MouseMove was delivered; release must still plan the requested last gap.
        assert!(fast.release(point(px(50.), px(148.)), &map).is_some());
    }
    #[test]
    fn invalid_parent_outside_and_clipped_rows_have_no_drop() {
        let order = [91, 7, 42];
        let mut map = rows(&order, 15);
        let mut other = rows(&[8], 91);
        let row = other.get_mut(&8).unwrap();
        row.bounds.origin.y = px(90.);
        row.visible = row.bounds;
        map.extend(other);
        assert!(gap_at(point(px(40.), px(95.)), 15, &order, &map).is_none());
        assert!(gap_at(point(px(400.), px(30.)), 15, &order, &map).is_none());
        assert_eq!(
            gap_at(point(px(40.), px(1.)), 15, &order, &map)
                .unwrap()
                .index,
            0
        );
        assert_eq!(
            gap_at(point(px(40.), px(89.)), 15, &order, &map)
                .unwrap()
                .index,
            3
        );
        map.get_mut(&91).unwrap().visible =
            Bounds::new(point(px(10.), px(20.)), size(px(200.), px(10.)));
        assert!(gap_at(point(px(40.), px(5.)), 15, &order, &map).is_none());
    }
    #[test]
    fn after_markers_follow_expanded_group_subtrees_or_collapsed_header() {
        let gap = Gap {
            item: 91,
            after: true,
            index: 1,
        };
        assert_eq!(
            marker_row(gap, &[(0, 0, 91), (1, 91, 7), (2, 7, 6), (0, 0, 42)]),
            Some((6, true))
        );
        assert_eq!(marker_row(gap, &[(0, 0, 91), (0, 0, 42)]), Some((91, true)));
        assert_eq!(
            marker_row(
                Gap {
                    after: false,
                    ..gap
                },
                &[(0, 0, 91), (1, 91, 7)]
            ),
            Some((91, false))
        );
    }
    #[test]
    fn unsupported_selection_chords_are_consumed_without_stealing_fields_or_global_shortcuts() {
        let mut s = scene();
        s.selected_layers.insert(1);
        let before = s.editor.project().clone();
        let layers = s.selected_layers.clone();
        for shift in [false, true] {
            for alt in [false, true] {
                for other in [false, true] {
                    let mut chords = vec![("c", true), ("x", true), ("v", true)];
                    if alt {
                        chords.extend([("t", true), ("[", false), ("]", false)]);
                    }
                    for (key, control) in chords {
                        let owned = tree_key(key, control, shift, alt, other, true, false).unwrap();
                        assert_eq!(key_route(owned, &s), KeyRoute::Consume, "{key}");
                        // An editor TextField is outside the tree focus domain. Its
                        // clipboard shortcuts and IME candidates retain all input.
                        assert!(tree_key(key, control, shift, alt, other, false, false).is_none());
                        assert!(tree_key(key, control, shift, alt, other, true, true).is_none());
                    }
                }
            }
        }
        // Project, composition, import, creation, render, tool and view shortcuts
        // keep their shell mapping. Actual Alt stays separate from platform/Fn.
        for key in ["s", "o", "n", "i", "m", "y", "r", "t", "k", "[", "]"] {
            for shift in [false, true] {
                for other in [false, true] {
                    assert!(
                        tree_key(key, true, shift, false, other, true, false).is_none(),
                        "Control {key}"
                    );
                }
            }
        }
        for key in ["n", "y", "s", "o", "i"] {
            assert!(
                tree_key(key, true, false, true, false, true, false).is_none(),
                "Control Alt {key}"
            );
        }
        for key in [
            "space", "home", "end", "pageup", "pagedown", "c", "x", "v", "h", "z", "q", "g", "w",
            "y", "p", "a", "s", "r", "t", "u", "j", "k", "b", "n", "=", "+", "-", "f3",
        ] {
            for shift in [false, true] {
                assert!(
                    tree_key(key, false, shift, false, false, true, false).is_none(),
                    "{key}"
                );
            }
        }
        for shift in [false, true] {
            let undo_redo = tree_key("z", true, shift, false, false, true, false).unwrap();
            assert_eq!(key_route(undo_redo, &s), KeyRoute::Bubble);
        }
        assert_eq!(s.editor.project(), &before);
        assert_eq!(s.selected_layers, layers);
        assert!(!s.editor.can_undo());
        assert!(!s.editor.can_redo());
    }
    #[test]
    fn playback_consumes_tree_editing_chords_without_leaking_to_whole_layers() {
        let mut s = scene();
        s.selected_layers.insert(1);
        s.playing = true;
        let before = s.editor.project().clone();
        let layers = s.selected_layers.clone();
        for (key, control) in [
            ("delete", false),
            ("backspace", false),
            ("up", false),
            ("down", false),
            ("left", false),
            ("right", false),
            ("d", true),
            ("a", true),
        ] {
            let owned = tree_key(key, control, false, false, false, true, false).unwrap();
            assert_eq!(key_route(owned, &s), KeyRoute::Consume, "{key}");
        }
        assert_eq!(key_route(TreeKey::Escape, &s), KeyRoute::Consume);
        assert_eq!(key_route(TreeKey::CancelAndBubble, &s), KeyRoute::Bubble);
        assert_eq!(s.editor.project(), &before);
        assert_eq!(s.selected_layers, layers);
        assert!(!s.editor.can_undo());
        assert!(!s.editor.can_redo());
        s.playing = false;
        assert_eq!(key_route(TreeKey::Delete, &s), KeyRoute::Handle);
        s.colors.session = Some(
            crate::color_edit::Session::new(
                crate::color_edit::Target::BackgroundDraft(0x222222),
                s.editor.project(),
                s.document_revision,
                s.frame,
            )
            .unwrap(),
        );
        assert_eq!(key_route(TreeKey::Escape, &s), KeyRoute::Bubble);
        assert_eq!(key_route(TreeKey::Delete, &s), KeyRoute::Consume);
        s.colors.session = None;
        s.media_open = true;
        assert_eq!(key_route(TreeKey::Escape, &s), KeyRoute::Bubble);
        assert_eq!(key_route(TreeKey::Duplicate, &s), KeyRoute::Consume);
        s.media_open = false;
        let gates: [(fn(&mut EditorState, bool), KeyRoute); 4] = [
            (|s: &mut EditorState, v| s.queue_open = v, KeyRoute::Consume),
            (
                |s: &mut EditorState, v| s.new_composition_requested = v,
                KeyRoute::Consume,
            ),
            (
                |s: &mut EditorState, v| s.close_after_save = v,
                KeyRoute::Bubble,
            ),
            (|s: &mut EditorState, v| s.fonts_open = v, KeyRoute::Bubble),
        ];
        for (set, escape) in gates {
            set(&mut s, true);
            assert_eq!(key_route(TreeKey::Delete, &s), KeyRoute::Consume);
            assert_eq!(key_route(TreeKey::Previous, &s), KeyRoute::Consume);
            assert_eq!(key_route(TreeKey::Escape, &s), escape);
            set(&mut s, false);
        }
    }
    #[test]
    fn numeric_vertex_modal_blocks_tree_drag_keys_and_bubbles_modal_escape() {
        use crate::panels::vertex_editor::{Request, Session};
        let mut s = scene();
        edit(&mut s, ContentsEdit::ConvertPath { item: 2, frame: 0 });
        s.tool = crate::editor::Tool::Pen;
        let selected = selection(&mut s, 1, &[2]);
        let collapsed = BTreeSet::new();
        let drag = Drag::new(&s, &selected, &collapsed, 2, point(px(20.), px(20.)), true).unwrap();
        let (item, path, local) = contents(&s)
            .editable_paths(0)
            .into_iter()
            .find(|(item, _, _)| *item == 2)
            .unwrap();
        let world = s
            .editor
            .project()
            .composition()
            .world_transform(1, 0)
            .unwrap()
            .compose(local);
        let request = Request::new(
            &s,
            1,
            libre_effects_core::PathTarget::Contents(item),
            0,
            path,
            world,
        )
        .unwrap();
        s.vertex_editor = Some(Session::new(&s, request).unwrap());
        assert!(!drag.current(&s, &selected, &collapsed));
        assert!(Drag::new(&s, &selected, &collapsed, 2, point(px(20.), px(20.)), true).is_none());
        for key in [
            TreeKey::Delete,
            TreeKey::Duplicate,
            TreeKey::Previous,
            TreeKey::Next,
            TreeKey::SelectAll,
        ] {
            assert_eq!(key_route(key, &s), KeyRoute::Consume);
        }
        assert_eq!(key_route(TreeKey::Escape, &s), KeyRoute::Bubble);
        assert_eq!(key_route(TreeKey::CancelAndBubble, &s), KeyRoute::Bubble);
    }

    #[test]
    fn color_gradient_and_modal_sessions_cancel_a_frozen_tree_drag() {
        let mut s = scene();
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::GradientFill {
                    even_odd: false,
                    gradient: Default::default(),
                },
            },
        );
        let item = contents(&s)
            .rows()
            .into_iter()
            .find(|(_, _, n)| n.kind.gradient().is_some())
            .unwrap()
            .2
            .id;
        let selected = selection(&mut s, 1, &[item]);
        let collapsed = BTreeSet::new();
        let d = Drag::new(
            &s,
            &selected,
            &collapsed,
            item,
            point(px(20.), px(20.)),
            true,
        )
        .unwrap();
        s.colors.session = Some(
            crate::color_edit::Session::new(
                crate::color_edit::Target::BackgroundDraft(0x222222),
                s.editor.project(),
                s.document_revision,
                s.frame,
            )
            .unwrap(),
        );
        assert!(!d.current(&s, &selected, &collapsed));
        s.colors.session = None;
        s.gradient_editor = Some(crate::panels::gradient_editor::Session::new(&s, item).unwrap());
        assert!(!d.current(&s, &selected, &collapsed));
        s.gradient_editor = None;
        s.gradient_preview =
            crate::color_edit::GradientDraft::new(&s, item, GradientParam::ColorPosition(1));
        assert!(s.gradient_preview.is_some());
        assert!(!d.current(&s, &selected, &collapsed));
        assert_eq!(key_route(TreeKey::Escape, &s), KeyRoute::Consume);
        assert_eq!(key_route(TreeKey::Delete, &s), KeyRoute::Consume);
        s.gradient_preview = None;
        s.queue_open = true;
        assert!(!d.current(&s, &selected, &collapsed));
        s.queue_open = false;
        s.fonts_open = true;
        assert!(!d.current(&s, &selected, &collapsed));
        s.fonts_open = false;
        s.new_composition_requested = true;
        assert!(!d.current(&s, &selected, &collapsed));
        s.new_composition_requested = false;
        s.playing = true;
        assert!(!d.current(&s, &selected, &collapsed));
        s.playing = false;
        assert!(d.current(&s, &selected, &collapsed));
    }
    #[test]
    fn project_snapshot_cancels_ordinary_edits_reused_ids_history_locks_and_context_changes() {
        let mut s = scene();
        let order = sibling_order(contents(&s), 1).unwrap();
        let selected = selection(&mut s, 1, &[order[1]]);
        let collapsed = BTreeSet::new();
        let d = Drag::new(
            &s,
            &selected,
            &collapsed,
            order[1],
            point(px(50.), px(40.)),
            true,
        )
        .unwrap();
        assert!(d.current(&s, &selected, &collapsed));
        // Reused IDs and byte-identical replacement still reset on revision.
        s.document_revision += 1;
        assert!(!d.current(&s, &selected, &collapsed));
        s.document_revision -= 1;
        edit(
            &mut s,
            ContentsEdit::Rename {
                item: order[0],
                name: "Ordinary edit".into(),
            },
        );
        assert_eq!(s.document_revision, 0);
        assert!(!d.current(&s, &selected, &collapsed));
        let after_edit = Drag::new(
            &s,
            &selected,
            &collapsed,
            order[1],
            point(px(50.), px(40.)),
            true,
        )
        .unwrap();
        s.editor.undo();
        assert!(!after_edit.current(&s, &selected, &collapsed));
        assert!(d.current(&s, &selected, &collapsed));
        s.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(!d.current(&s, &selected, &collapsed));
        assert!(
            Drag::new(
                &s,
                &selected,
                &collapsed,
                order[1],
                point(px(0.), px(0.)),
                true
            )
            .is_none()
        );
        s.editor.undo();
        s.media_open = true;
        assert!(!d.current(&s, &selected, &collapsed));
        s.media_open = false;
        s.frame += 1;
        assert!(!d.current(&s, &selected, &collapsed));
        s.frame -= 1;
        let tool = s.tool;
        s.tool = crate::editor::Tool::Pen;
        assert!(!d.current(&s, &selected, &collapsed));
        s.tool = tool;
        s.gradient_controls = Some(crate::color_edit::GradientTarget::Contents(1, 1, order[1]));
        assert!(!d.current(&s, &selected, &collapsed));
        s.gradient_controls = None;
        s.selected_layers.insert(999);
        assert!(!d.current(&s, &selected, &collapsed));
        s.selected_layers.clear();
        s.contents_selection = None;
        assert!(!d.current(&s, &selected, &collapsed));
        s.contents_selection = Some((s.editor.project().active_composition_id(), 1, order[1]));
        assert!(!d.current(&s, &selected, &[1].into()));
        let mut other = selected.clone();
        other.one(1, order[0]);
        assert!(!d.current(&s, &other, &collapsed));
        s.editor.clear_selection();
        assert!(!d.current(&s, &selected, &collapsed));
    }
}
