use super::{tree_selection::*, *};
use gpui::{KeyDownEvent, MouseDownEvent, Point};
use libre_effects_core::{CompositionId, Project};
use std::collections::{BTreeMap, BTreeSet};

/// Registrations are rebuilt for every paint. A layout epoch changes only when
/// actual geometry changes, so an identical repaint preserves a gesture.
#[derive(Default)]
pub(super) struct TreeLayout {
    generation: u64,
    pub epoch: u64,
    owner: Option<(CompositionId, u64)>,
    visible: Vec<(usize, u64, u64)>,
    rows: BTreeMap<
        u64,
        (
            Option<(Bounds<Pixels>, Bounds<Pixels>)>,
            Option<Bounds<Pixels>>,
        ),
    >,
    root: Option<super::tree_drop::RootGeometry>,
    latest: Option<super::tree_drop::Geometry>,
    previous: Option<super::tree_drop::Geometry>,
}
impl TreeLayout {
    pub fn begin(&mut self, owner: (CompositionId, u64), visible: Vec<(usize, u64, u64)>) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.owner = Some(owner);
        self.visible = visible;
        self.rows.clear();
        self.root = None;
        self.latest = None;
        self.generation
    }
    pub fn row(&mut self, item: u64, bounds: Bounds<Pixels>, clip: Bounds<Pixels>) {
        self.rows.entry(item).or_default().0 = Some((bounds, clip));
    }
    pub fn label(&mut self, item: u64, bounds: Bounds<Pixels>) {
        self.rows.entry(item).or_default().1 = Some(bounds);
    }
    pub fn root(&mut self, bounds: Bounds<Pixels>, clip: Bounds<Pixels>) {
        self.root = Some(super::tree_drop::RootGeometry { bounds, clip });
    }
    pub fn finish(&mut self, groups: &BTreeSet<u64>) {
        let Some(owner) = self.owner else {
            return;
        };
        let rows = self
            .visible
            .iter()
            .filter_map(|&(depth, parent, item)| {
                let &(Some((bounds, clip)), Some(label)) = self.rows.get(&item)? else {
                    return None;
                };
                Some(super::tree_drop::RowGeometry {
                    item,
                    parent,
                    depth,
                    group: groups.contains(&item),
                    bounds,
                    label,
                    clip,
                })
            })
            .collect();
        let geometry = super::tree_drop::Geometry {
            owner,
            visible: self.visible.clone(),
            rows,
            root: self.root.clone(),
        };
        if self.previous.as_ref() != Some(&geometry) {
            self.epoch = self.epoch.wrapping_add(1);
        }
        self.previous = Some(geometry.clone());
        self.latest = Some(geometry);
    }
    pub fn current(&self) -> Option<&super::tree_drop::Geometry> {
        self.latest.as_ref()
    }
    pub fn invalidate(&mut self) {
        // Native bounds/scroll can change before paint refreshes hitboxes. A
        // new press must wait for the next completed generation as well.
        self.latest = None;
    }
    fn invalidate_outside_label(&mut self, position: Point<Pixels>) -> bool {
        let on_label = self.current().is_some_and(|geometry| {
            geometry.rows.iter().any(|row| {
                let visible = row.label.intersect(&row.clip);
                visible.size.width > px(0.)
                    && visible.size.height > px(0.)
                    && position.x >= visible.left()
                    && position.x < visible.right()
                    && position.y >= visible.top()
                    && position.y < visible.bottom()
            })
        });
        if !on_label {
            self.invalidate();
        }
        !on_label
    }
}

/// Rendered tree identity is checked before any pending field is allowed to
/// commit. Unlike MoveContext this also permits empty and locked selections.
#[derive(Clone)]
pub(super) struct PressContext {
    project: std::sync::Arc<Project>,
    revision: u64,
    transport_generation: u64,
    composition: CompositionId,
    layer: u64,
    parent: u64,
    item: u64,
    locked: bool,
    frame: u32,
    tool: crate::editor::Tool,
    gradient_controls: Option<crate::color_edit::GradientTarget>,
    selected_layers: BTreeSet<u64>,
    singleton: Option<(CompositionId, u64, u64)>,
    selection: Selection,
    collapsed: BTreeSet<u64>,
}
impl PressContext {
    pub fn new(
        this: &ContentsControls,
        parent: u64,
        item: u64,
        project: std::sync::Arc<Project>,
        cx: &Context<ContentsControls>,
    ) -> Self {
        Self::capture(
            this.state.read(cx),
            &this.selection,
            &this.collapsed,
            parent,
            item,
            project,
        )
    }
    pub(super) fn capture(
        s: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        parent: u64,
        item: u64,
        project: std::sync::Arc<Project>,
    ) -> Self {
        Self {
            project,
            revision: s.document_revision,
            transport_generation: s.transport_generation(),
            composition: s.editor.project().active_composition_id(),
            layer: s.editor.selected().unwrap(),
            parent,
            item,
            locked: s.editor.selected_layer().unwrap().locked(),
            frame: s.frame,
            tool: s.tool,
            gradient_controls: s.gradient_controls,
            selected_layers: s.selected_layers.clone(),
            singleton: s.contents_selection,
            selection: selection.clone(),
            collapsed: collapsed.clone(),
        }
    }
    pub(super) fn same_context(
        &self,
        s: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        after_flush: bool,
    ) -> bool {
        !blocked(s)
            && self.revision == s.document_revision
            && self.composition == s.editor.project().active_composition_id() && s.editor.selected() == Some(self.layer)
            && self.frame == s.frame && self.tool == s.tool && self.gradient_controls == s.gradient_controls
            && self.selected_layers == s.selected_layers && self.singleton == s.contents_selection
            && self.selection == *selection && self.collapsed == *collapsed
            && (after_flush && !self.locked || (s.editor.project() == self.project.as_ref() && self.transport_generation == s.transport_generation()))
            && s.editor.selected_layer().is_some_and(|l| l.locked() == self.locked && matches!(l.content(), Content::ShapeContents(c) if sibling_order(c, self.parent).is_some_and(|ids| ids.contains(&self.item))))
    }
    pub fn current(&self, this: &ContentsControls, s: &EditorState, after_flush: bool) -> bool {
        self.same_context(s, &this.selection, &this.collapsed, after_flush)
            && this.owner == Some((self.composition, self.layer)) && this.owner_revision == self.revision
            && this.tree_layout.borrow().current().is_some_and(|g| {
                g.owner == (self.composition, self.layer) && g.visible.iter().any(|&(_, p, id)| p == self.parent && id == self.item)
                    && s.editor.selected_layer().is_some_and(|l| matches!(l.content(), Content::ShapeContents(c) if g.matches_visible(&visible_rows(c, &this.collapsed))))
            })
    }
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
    transport_generation: u64,
    pub(super) pointer_generation: u64,
    pub(super) geometry: Option<(u64, super::tree_drop::Geometry)>,
    pressed: u64,
    origin: Point<Pixels>,
    plain: bool,
    pub moved: bool,
    pub preview: Option<super::tree_drop::DropPreview>,
}
pub(super) fn blocked(s: &EditorState) -> bool {
    s.colors.session.is_some()
        || s.gradient_editor.is_some()
        || s.vertex_editor.is_some()
        || s.expression_editor.is_some()
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
/// Tree-owned transient context, separate from the full source snapshot in
/// InputTarget. A pointer receipt may survive its synchronous field commit but
/// may never adopt another selection, tool, layer set, disclosure state or time.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct MoveContext {
    composition: CompositionId,
    layer: u64,
    revision: u64,
    frame: u32,
    tool: crate::editor::Tool,
    gradient_controls: Option<crate::color_edit::GradientTarget>,
    selected_layers: BTreeSet<u64>,
    selection: Selection,
    collapsed: BTreeSet<u64>,
}
impl MoveContext {
    fn new(s: &EditorState, selection: &Selection, collapsed: &BTreeSet<u64>) -> Option<Self> {
        if blocked(s) || selection.items.is_empty() {
            return None;
        }
        let layer = s.editor.selected_layer().filter(|l| !l.locked())?;
        let Content::ShapeContents(contents) = layer.content() else {
            return None;
        };
        let composition = s.editor.project().active_composition_id();
        if s.contents_selection
            != selection
                .singleton()
                .map(|item| (composition, layer.id(), item))
        {
            return None;
        }
        let order = sibling_order(contents, selection.parent?)?;
        let visible = visible_rows(contents, collapsed);
        if !selection
            .items
            .iter()
            .all(|id| order.contains(id) && visible.iter().any(|(_, _, n)| n == id))
        {
            return None;
        }
        Some(Self {
            composition,
            layer: layer.id(),
            revision: s.document_revision,
            frame: s.frame,
            tool: s.tool,
            gradient_controls: s.gradient_controls,
            selected_layers: s.selected_layers.clone(),
            selection: selection.clone(),
            collapsed: collapsed.clone(),
        })
    }
    fn current(&self, s: &EditorState, selection: &Selection, collapsed: &BTreeSet<u64>) -> bool {
        Self::new(s, selection, collapsed).as_ref() == Some(self)
    }
    fn plan(
        &self,
        s: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        direction: MoveDirection,
    ) -> Option<MovePlan> {
        self.current(s, selection, collapsed).then_some(())?;
        let Content::ShapeContents(contents) = s.editor.selected_layer()?.content() else {
            return None;
        };
        plan_move(contents, selection, direction)
    }
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
        || s.expression_editor.is_some()
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
            transport_generation: s.transport_generation(),
            pointer_generation: 0,
            geometry: None,
            pressed,
            origin,
            plain,
            moved: false,
            preview: None,
        })
    }
    pub fn current(
        &self,
        s: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
    ) -> bool {
        !blocked(s)
            && self.transport_generation == s.transport_generation()
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
    pub(super) fn geometry_current(&self, layout: &TreeLayout) -> bool {
        self.geometry.as_ref().is_some_and(|(epoch, geometry)| {
            *epoch == layout.epoch && layout.current() == Some(geometry)
        })
    }
    fn update(&mut self, position: Point<Pixels>, geometry: &super::tree_drop::Geometry) {
        let delta = position - self.origin;
        self.moved |= f32::from(delta.x).hypot(f32::from(delta.y)) >= 4.;
        let contents = self.project.composition().layer(self.layer).and_then(|l| {
            if let Content::ShapeContents(c) = l.content() {
                Some(c)
            } else {
                None
            }
        });
        self.preview = self
            .moved
            .then(|| contents.and_then(|c| geometry.resolve(c, &self.selection, position)))
            .flatten();
    }
    fn release(
        &mut self,
        position: Point<Pixels>,
        geometry: &super::tree_drop::Geometry,
    ) -> Option<Command> {
        // Re-resolve all three steps at actual release, never dispatch hover state.
        self.update(position, geometry);
        Some(Command::Contents {
            id: self.layer,
            edit: self.preview.as_ref()?.plan.edit()?,
        })
    }
}
fn supported_modifiers(modifiers: gpui::Modifiers) -> bool {
    !modifiers.alt && !modifiers.platform && !modifiers.function
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
            self.selection
                .reconcile_reparented(contents, &mut self.collapsed);
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
        self.refresh_bulk_context(cx);
        self.refresh_clipboard_context(cx);
    }
    pub(super) fn publish_tree_selection(&mut self, cx: &mut Context<Self>) {
        // Even a source-equal A → B → A selection cycle retires old fields.
        self.bulk_session.borrow_mut().invalidate();
        self.clipboard_session.borrow_mut().invalidate();
        self.colors_serial.set(
            self.colors_serial
                .get()
                .checked_add(1)
                .expect("Colors input serial exhausted"),
        );
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
            s.retire_colors_clipboard();
            s.colors_key_owned.set(false);
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
    pub(super) fn hierarchy_context(&self, cx: &Context<Self>) -> Option<MoveContext> {
        let s = self.state.read(cx);
        let context = MoveContext::new(s, &self.selection, &self.collapsed)?;
        (self.owner == Some((context.composition, context.layer))
            && self.owner_revision == context.revision)
            .then_some(context)
    }

    /// Native buttons remain in the tree focus domain, including Tab/Enter.
    /// Register with the workspace's pointer-before-blur mechanism; its receipt
    /// validates the entire project before and after flushing pending input.
    pub(super) fn hierarchy_actions(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let context = self.hierarchy_context(cx);
        if context != self.move_context {
            self.move_serial = self.move_serial.wrapping_add(1);
            self.move_context = context.clone();
        }
        let mut actions = div().flex().flex_col().gap_1();
        let Some(context) = context else {
            self.move_input = None;
            return actions;
        };
        let target = crate::color_edit::InputTarget::new(self.state.read(cx));
        self.move_input = target.clone();
        for (direction, label) in [
            (MoveDirection::Into, "Move Into · Ctrl+Right"),
            (MoveDirection::Out, "Move Out · Ctrl+Left"),
        ] {
            if context
                .plan(
                    self.state.read(cx),
                    &self.selection,
                    &self.collapsed,
                    direction,
                )
                .is_none()
            {
                continue;
            }
            let control = format!("contents-hierarchy-{}-{direction:?}", self.move_serial);
            let context = context.clone();
            let target = target.clone();
            let button = ui::text_button(gpui::SharedString::from(control.clone()), label)
                .tooltip(|_, cx| cx.new(|_| ui::Tip(super::MOVE_HELP.into())).into());
            actions = actions.child(
                crate::color_edit::input_pointer_button_preserving_ime(button, control.clone(), target.clone())
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, w, cx| {
                        cx.stop_propagation();
                        if event.modifiers().modified()
                            || matches!(event, gpui::ClickEvent::Mouse(click) if click.down.modifiers.modified())
                            || TextField::is_composing(w, cx)
                            || !context.current(
                                this.state.read(cx),
                                &this.selection,
                                &this.collapsed,
                            )
                        {
                            return;
                        }
                        let Some(input) = crate::color_edit::input_click_target(
                            &control,
                            event,
                            &target,
                            &this.state,
                            w,
                            cx,
                        ) else {
                            return;
                        };
                        TextField::commit_active(w, cx);
                        if input.same_context(this.state.read(cx)) {
                            this.move_hierarchy(direction, &context, w, cx);
                        }
                    })),
            );
        }
        actions
    }

    fn move_hierarchy(
        &mut self,
        direction: MoveDirection,
        context: &MoveContext,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !w.is_window_active()
            || TextField::is_composing(w, cx)
            || self.hierarchy_context(cx).as_ref() != Some(context)
        {
            return;
        }
        let Some(plan) = context.plan(
            self.state.read(cx),
            &self.selection,
            &self.collapsed,
            direction,
        ) else {
            return;
        };
        self.apply_tree_move(context.layer, plan.parent, &plan.items, plan.edit(), w, cx);
    }

    /// Explicit moves and cross-parent drops share the same success-only reveal,
    /// stable selection, pin reconciliation and status policy.
    fn apply_tree_move(
        &mut self,
        layer: u64,
        parent: u64,
        items: &[u64],
        edit: ContentsEdit,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.state.read(cx).editor.project().clone();
        self.state.update(cx, |s, cx| {
            s.dispatch(&Action::Edit(Command::Contents { id: layer, edit }), w, cx)
        });
        let current = self.state.read(cx).editor.project();
        if current == &before {
            return;
        }
        // A failed transaction must not expand a group or discard its selection.
        let Some(contents) = self.state.read(cx).editor.selected_layer().and_then(|l| {
            if l.id() != layer {
                return None;
            }
            let Content::ShapeContents(contents) = l.content() else {
                return None;
            };
            let destination = sibling_order(contents, parent)?;
            (items.iter().all(|id| destination.contains(id))).then(|| contents.clone())
        }) else {
            return;
        };
        self.tree_drag = None;
        self.cancel_ramp(cx);
        self.paint_menu = None;
        self.selection
            .reconcile_reparented(&contents, &mut self.collapsed);
        self.publish_tree_selection(cx);
        self.state.update(cx, |s, cx| {
            s.status = super::MOVE_HELP.into();
            cx.notify();
        });
        w.focus(&self.tree_focus);
        cx.notify();
    }

    pub(super) fn tree_down(
        &mut self,
        control: &str,
        context: &PressContext,
        e: &MouseDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tree_drag = None;
        if TextField::is_composing(w, cx) || !context.current(self, self.state.read(cx), true) {
            return;
        }
        let Some(receipt) =
            crate::color_edit::input_tree_down_target(control, e, &self.state, w, cx)
        else {
            return;
        };
        let Some((epoch, geometry)) = self
            .tree_layout
            .borrow()
            .current()
            .map(|g| (self.tree_layout.borrow().epoch, g.clone()))
        else {
            return;
        };
        let Some(order) = self.state.read(cx).editor.selected_layer().and_then(|l| {
            let Content::ShapeContents(contents) = l.content() else {
                return None;
            };
            sibling_order(contents, context.parent)
        }) else {
            return;
        };
        let m = e.modifiers;
        let editable = matches!(receipt, crate::color_edit::TreeDownTarget::Editable(_));
        // A locked label keeps ordinary selection/focus behavior but never acquires
        // an editable receipt or a drag. Source is rechecked after outside-down.
        if editable {
            self.selection
                .press(context.parent, &order, context.item, m.control, m.shift);
        } else {
            self.selection
                .click(context.parent, &order, context.item, m.control, m.shift);
        }
        w.focus(&self.tree_focus);
        self.publish_tree_selection(cx);
        if editable {
            self.tree_drag = Drag::new(
                self.state.read(cx),
                &self.selection,
                &self.collapsed,
                context.item,
                e.position,
                !m.control && !m.shift,
            );
            if let Some(drag) = &mut self.tree_drag {
                drag.geometry = Some((epoch, geometry));
                drag.pointer_generation = crate::color_edit::input_pointer_generation(w, cx);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn tree_pointer_down_capture(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let canceled = self.tree_drag.take().is_some();
        // Scrollbar and splitter drags can move layout without a wheel event.
        // Only label presses may use this completed generation to start a drag.
        let invalidated = self
            .tree_layout
            .borrow_mut()
            .invalidate_outside_label(position);
        if canceled || invalidated {
            cx.notify();
        }
    }
    pub(crate) fn invalidate_tree_layout(&mut self, cx: &mut Context<Self>) {
        self.tree_layout.borrow_mut().invalidate();
        self.tree_drag = None;
        // Even a zero-delta/header scroll must refresh registrations; otherwise
        // an idle tree could stay ineligible until an unrelated repaint.
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
        if drag.pointer_generation != crate::color_edit::input_pointer_generation(w, cx)
            || e.pressed_button != Some(MouseButton::Left)
            || !supported_modifiers(e.modifiers)
            || !w.is_window_active()
            || !self.tree_focus.is_focused(w)
            || TextField::is_composing(w, cx)
            || !drag.current(self.state.read(cx), &self.selection, &self.collapsed)
            || !drag.geometry_current(&self.tree_layout.borrow())
        {
            self.tree_drag = None;
        } else if let Some(geometry) = self.tree_layout.borrow().current() {
            drag.update(e.position, geometry);
        }
        cx.notify();
    }
    pub(super) fn tree_up(
        &mut self,
        e: &gpui::MouseUpEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Consume first: wrong-button, stale, duplicate and canceled releases
        // cannot resurrect a receipt or publish a second command.
        let Some(mut drag) = self.tree_drag.take() else {
            return;
        };
        if drag.pointer_generation == crate::color_edit::input_pointer_generation(w, cx)
            && e.button == MouseButton::Left
            && e.click_count == 1
            && supported_modifiers(e.modifiers)
            && w.is_window_active()
            && self.tree_focus.is_focused(w)
            && !TextField::is_composing(w, cx)
            && drag.current(self.state.read(cx), &self.selection, &self.collapsed)
            && drag.geometry_current(&self.tree_layout.borrow())
        {
            let command = {
                let layout = self.tree_layout.borrow();
                drag.release(e.position, layout.current().unwrap())
            };
            if !drag.moved && drag.plain {
                self.selection
                    .one(drag.selection.parent.unwrap(), drag.pressed);
                self.publish_tree_selection(cx);
            } else if let Some(command) = command {
                let plan = &drag.preview.as_ref().unwrap().plan;
                if plan.source_parent != plan.parent {
                    self.apply_tree_move(
                        drag.layer,
                        plan.parent,
                        &plan.items,
                        plan.edit().unwrap(),
                        w,
                        cx,
                    );
                } else {
                    self.state
                        .update(cx, |s, cx| s.dispatch(&Action::Edit(command), w, cx));
                }
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
        if let Some(operation) = match key {
            TreeKey::Copy => Some(super::clipboard::Operation::Copy),
            TreeKey::Cut => Some(super::clipboard::Operation::Cut),
            TreeKey::Paste => Some(super::clipboard::Operation::Paste),
            _ => None,
        } {
            self.clipboard_key(operation, e.is_held, w, cx);
            return;
        }
        if matches!(key, TreeKey::MoveInto | TreeKey::MoveOut) {
            // Never reconcile a stale tree into a newly active layer before an edit.
            // Held chords and extra modifiers are consumed, never layer shortcuts.
            if let Some(direction) = key.move_direction(e.is_held)
                && self
                    .move_input
                    .as_ref()
                    .is_some_and(|input| input.current(self.state.read(cx)))
                && let Some(context) = self.move_context.clone()
            {
                self.move_hierarchy(direction, &context, w, cx);
            }
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
    fn geometry(s: &EditorState, collapsed: &BTreeSet<u64>) -> super::super::tree_drop::Geometry {
        use super::super::tree_drop::*;
        let visible = visible_rows(contents(s), collapsed);
        let clip = Bounds::new(point(px(0.), px(0.)), size(px(300.), px(2000.)));
        let rows = visible
            .iter()
            .enumerate()
            .map(|(i, &(depth, parent, item))| {
                let bounds =
                    Bounds::new(point(px(0.), px(i as f32 * 26.)), size(px(250.), px(26.)));
                RowGeometry {
                    item,
                    parent,
                    depth,
                    group: matches!(contents(s).node(item).unwrap().kind, ContentsKind::Group(_)),
                    bounds,
                    label: Bounds::new(point(px(50.), bounds.top()), size(px(200.), px(26.))),
                    clip,
                }
            })
            .collect();
        Geometry {
            owner: (s.editor.project().active_composition_id(), 1),
            root: Some(RootGeometry {
                bounds: Bounds::new(
                    point(px(0.), px(visible.len() as f32 * 26.)),
                    size(px(250.), px(26.)),
                ),
                clip,
            }),
            visible,
            rows,
        }
    }
    fn at(geometry: &super::super::tree_drop::Geometry, item: u64, dy: f32) -> Point<Pixels> {
        let row = geometry.rows.iter().find(|row| row.item == item).unwrap();
        point(px(80.), row.bounds.top() + px(dy))
    }
    fn hierarchy_scene() -> EditorState {
        let mut s = scene();
        // Add places non-paint items at the front. This fixture deliberately puts
        // the unselected destination immediately before the selected root block.
        edit(
            &mut s,
            ContentsEdit::Reorder {
                parent: 0,
                order: vec![1, 5, 6],
            },
        );
        s.editor.clear_history();
        s
    }
    #[test]
    fn rendered_press_checks_full_source_before_flush_and_exact_tree_context_afterward() {
        let mut s = scene();
        let selected = selection(&mut s, 1, &[2]);
        let collapsed = BTreeSet::new();
        let context = PressContext::capture(
            &s,
            &selected,
            &collapsed,
            1,
            2,
            std::sync::Arc::new(s.editor.project().clone()),
        );
        assert!(context.same_context(&s, &selected, &collapsed, false));
        edit(
            &mut s,
            ContentsEdit::Rename {
                item: 2,
                name: "Authorized pending name".into(),
            },
        );
        assert!(!context.same_context(&s, &selected, &collapsed, false));
        assert!(context.same_context(&s, &selected, &collapsed, true));
        let mut changed_selection = selected.clone();
        changed_selection.one(1, 3);
        assert!(!context.same_context(&s, &changed_selection, &collapsed, true));
        assert!(!context.same_context(&s, &selected, &[1].into(), true));
        s.frame += 1;
        assert!(!context.same_context(&s, &selected, &collapsed, true));
        s.frame -= 1;
        s.selected_layers.insert(999);
        assert!(!context.same_context(&s, &selected, &collapsed, true));
        s.selected_layers.clear();
        s.queue_open = true;
        assert!(!context.same_context(&s, &selected, &collapsed, true));
        s.queue_open = false;
        edit(
            &mut s,
            ContentsEdit::MoveSiblings {
                source_parent: 1,
                items: vec![2],
                parent: 0,
                index: 0,
            },
        );
        assert!(!context.same_context(&s, &selected, &collapsed, true));
    }
    #[test]
    fn locked_press_snapshot_never_rebases_after_a_source_change_or_becomes_editable() {
        let mut s = scene();
        s.editor.execute(Command::ToggleLocked(1)).unwrap();
        let selected = selection(&mut s, 1, &[2]);
        let collapsed = BTreeSet::new();
        let context = PressContext::capture(
            &s,
            &selected,
            &collapsed,
            1,
            2,
            std::sync::Arc::new(s.editor.project().clone()),
        );
        assert!(context.same_context(&s, &selected, &collapsed, false));
        assert!(context.same_context(&s, &selected, &collapsed, true));
        assert!(crate::color_edit::InputTarget::new(&s).is_none());
        assert!(Drag::new(&s, &selected, &collapsed, 2, point(px(0.), px(0.)), true).is_none());
        s.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(!context.same_context(&s, &selected, &collapsed, true));
        edit(
            &mut s,
            ContentsEdit::Rename {
                item: 2,
                name: "Other source".into(),
            },
        );
        s.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(!context.same_context(&s, &selected, &collapsed, true));
    }
    #[test]
    fn hierarchy_context_rejects_stale_owner_selection_focus_domains_and_blocked_states() {
        let mut s = hierarchy_scene();
        let selected = selection(&mut s, 0, &[5, 6]);
        let collapsed = [1].into();
        let context = MoveContext::new(&s, &selected, &collapsed).unwrap();
        let input = crate::color_edit::InputTarget::new(&s).unwrap();
        assert!(
            context
                .plan(&s, &selected, &collapsed, MoveDirection::Into)
                .is_some()
        );
        let gates: &[fn(&mut EditorState)] = &[
            |s| s.playing = true,
            |s| s.queue_open = true,
            |s| s.media_open = true,
            |s| s.fonts_open = true,
            |s| s.new_composition_requested = true,
            |s| s.close_after_save = true,
            |s| s.frame += 1,
            |s| s.document_revision += 1,
            |s| s.tool = crate::editor::Tool::Pen,
            |s| {
                s.selected_layers.insert(77);
            },
            |s| s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 5)),
            |s| s.gradient_controls = Some(crate::color_edit::GradientTarget::Contents(1, 1, 5)),
            |s| {
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
            },
            |s| s.editor.clear_selection(),
            |s| {
                s.editor.execute(Command::NewComposition).unwrap();
            },
        ];
        for mutate in gates {
            let mut candidate = hierarchy_scene();
            let selection = selection(&mut candidate, 0, &[5, 6]);
            mutate(&mut candidate);
            assert!(!context.current(&candidate, &selection, &collapsed));
            assert!(
                context
                    .plan(&candidate, &selection, &collapsed, MoveDirection::Into)
                    .is_none()
            );
        }
        let mut other = selected.clone();
        other.one(0, 5);
        assert!(!context.current(&s, &other, &collapsed));
        assert!(!context.current(&s, &selected, &BTreeSet::new()));
        // Unrelated source mutations also invalidate the full pointer/key snapshot.
        edit(
            &mut s,
            ContentsEdit::Rename {
                item: 2,
                name: "Changed while pressed".into(),
            },
        );
        assert!(!input.current(&s));
        assert!(context.current(&s, &selected, &collapsed));
        s.editor.undo();
        assert!(input.current(&s));
    }
    #[test]
    fn hierarchy_replans_after_authorized_pending_field_flush_without_captured_indices() {
        let mut s = hierarchy_scene();
        let selected = selection(&mut s, 0, &[5, 6]);
        let collapsed = [1].into();
        let context = MoveContext::new(&s, &selected, &collapsed).unwrap();
        let input = crate::color_edit::InputTarget::new(&s).unwrap();
        let old = context
            .plan(&s, &selected, &collapsed, MoveDirection::Into)
            .unwrap();
        assert_eq!(old.index, 5);
        assert!(input.current(&s)); // Workspace receipt checks this before flushing.
        edit(
            &mut s,
            ContentsEdit::Rename {
                item: 5,
                name: "Pending new name".into(),
            },
        );
        assert!(!input.current(&s));
        assert!(input.same_context(&s));
        assert!(context.current(&s, &selected, &collapsed));
        let flushed = crate::color_edit::InputTarget::new(&s).unwrap();
        let plan = context
            .plan(&s, &selected, &collapsed, MoveDirection::Into)
            .unwrap();
        assert_eq!(plan, old);
        let source = s.editor.project().clone();
        edit(&mut s, plan.edit());
        assert_eq!(contents(&s).node(5).unwrap().name, "Pending new name");
        assert_eq!(sibling_order(contents(&s), 1).unwrap()[5..], [5, 6]);
        assert!(!flushed.current(&s));
        let mut retained = selected.clone();
        let mut disclosure = collapsed.clone();
        retained.reconcile_reparented(contents(&s), &mut disclosure);
        assert_eq!(retained.items, selected.items);
        assert_eq!(retained.parent, Some(1));
        assert_eq!(retained.singleton(), None);
        assert!(!disclosure.contains(&1));
        s.editor.undo();
        assert_eq!(s.editor.project(), &source);
        retained.reconcile_reparented(contents(&s), &mut disclosure);
        assert_eq!(retained.parent, Some(0));
        assert_eq!(retained.items, selected.items);
        // Planning again reads the actual destination size; no old index is reused.
        edit(
            &mut s,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::Group(vec![]),
            },
        );
        assert_eq!(
            context
                .plan(&s, &selected, &collapsed, MoveDirection::Into)
                .unwrap()
                .index,
            6
        );
    }
    #[test]
    fn hierarchy_commands_never_reuse_same_parent_drag_geometry_or_mutate_on_planning() {
        let mut s = hierarchy_scene();
        let selected = selection(&mut s, 0, &[5, 6]);
        let collapsed = [1].into();
        let context = MoveContext::new(&s, &selected, &collapsed).unwrap();
        let before = s.editor.project().clone();
        let plan = context
            .plan(&s, &selected, &collapsed, MoveDirection::Into)
            .unwrap();
        assert!(matches!(
            plan.edit(),
            ContentsEdit::MoveSiblings {
                source_parent: 0,
                parent: 1,
                ..
            }
        ));
        assert_eq!(s.editor.project(), &before);
        assert_eq!(collapsed, [1].into());
        assert!(!s.editor.can_undo());
        let map = geometry(&s, &collapsed);
        let mut drag = Drag::new(&s, &selected, &collapsed, 5, at(&map, 5, 13.), true).unwrap();
        assert!(matches!(
            drag.release(at(&map, 1, 1.), &map).unwrap(),
            Command::Contents {
                edit: ContentsEdit::Reorder { parent: 0, .. },
                ..
            }
        ));
        edit(&mut s, plan.edit());
        assert!(!drag.current(&s, &selected, &collapsed));
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
            let selected = selection(&mut s, parent, &[order[0], order[order.len() - 2]]);
            let before = s.editor.project().clone();
            let map = geometry(&s, &BTreeSet::new());
            let mut drag = Drag::new(
                &s,
                &selected,
                &BTreeSet::new(),
                order[0],
                at(&map, order[0], 13.),
                true,
            )
            .unwrap();
            for &item in &order {
                drag.update(at(&map, item, 1.), &map);
            }
            assert_eq!(s.editor.project(), &before);
            assert!(!s.editor.can_undo());
            let command = drag
                .release(at(&map, *order.last().unwrap(), 25.), &map)
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
        let map = geometry(&s, &BTreeSet::new());
        let origin = at(&map, order[1], 13.);
        let mut drag = Drag::new(&s, &selected, &BTreeSet::new(), order[1], origin, true).unwrap();
        assert!(drag.release(at(&map, order[1], 25.), &map).is_none());
        assert!(drag.moved);
        let mut canceled =
            Drag::new(&s, &selected, &BTreeSet::new(), order[1], origin, true).unwrap();
        canceled.update(at(&map, *order.last().unwrap(), 25.), &map);
        drop(canceled);
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        s.editor.redo();
        assert_eq!(s.editor.project(), &redo);
        s.editor.undo();
        let mut fast = Drag::new(&s, &selected, &BTreeSet::new(), order[1], origin, true).unwrap();
        assert!(
            fast.release(at(&map, *order.last().unwrap(), 25.), &map)
                .is_some()
        );
    }
    #[test]
    fn cross_parent_final_release_replans_and_below_threshold_is_plain_click() {
        let mut s = scene();
        let selected = selection(&mut s, 1, &[2, 3]);
        let collapsed = [5].into();
        let map = geometry(&s, &collapsed);
        let origin = at(&map, 2, 13.);
        let mut drag = Drag::new(&s, &selected, &collapsed, 2, origin, true).unwrap();
        assert!(drag.release(origin + point(px(3.), px(0.)), &map).is_none());
        assert!(!drag.moved);
        drag.update(at(&map, 5, 13.), &map);
        assert_eq!(
            drag.preview.as_ref().unwrap().plan.target,
            DropTarget::Into(5)
        );
        let command = drag
            .release(map.root.as_ref().unwrap().bounds.center(), &map)
            .unwrap();
        assert!(matches!(
            command,
            Command::Contents {
                edit: ContentsEdit::MoveSiblings {
                    source_parent: 1,
                    parent: 0,
                    ..
                },
                ..
            }
        ));
        assert_eq!(collapsed, [5].into());
        assert!(!s.editor.can_undo());
        assert!(drag.release(point(px(999.), px(999.)), &map).is_none());
        assert!(drag.moved); // Returning outside never becomes a plain click.
    }
    #[test]
    fn invalid_controls_leaf_centers_and_clipped_rows_have_no_drop() {
        let mut s = scene();
        let selected = selection(&mut s, 1, &[3]);
        let mut map = geometry(&s, &BTreeSet::new());
        assert!(
            map.resolve(contents(&s), &selected, at(&map, 2, 13.))
                .is_none()
        );
        assert!(
            map.resolve(contents(&s), &selected, point(px(20.), px(1.)))
                .is_none()
        );
        assert!(
            map.resolve(contents(&s), &selected, point(px(999.), px(1.)))
                .is_none()
        );
        let row = map.rows.iter_mut().find(|r| r.item == 2).unwrap();
        row.clip.origin.y = row.bounds.top() + px(8.);
        assert!(
            map.resolve(contents(&s), &selected, at(&map, 2, 1.))
                .is_none()
        );
    }
    #[test]
    fn generations_drop_old_bounds_and_geometry_changes_cancel_stickily() {
        let mut s = scene();
        let selected = selection(&mut s, 1, &[2]);
        let map = geometry(&s, &BTreeSet::new());
        let groups: BTreeSet<_> = map
            .rows
            .iter()
            .filter(|r| r.group)
            .map(|r| r.item)
            .collect();
        let paint = |layout: &mut TreeLayout, geometry: &super::super::tree_drop::Geometry| {
            layout.begin(geometry.owner, geometry.visible.clone());
            assert!(layout.current().is_none());
            for row in &geometry.rows {
                layout.row(row.item, row.bounds, row.clip);
                layout.label(row.item, row.label);
            }
            if let Some(root) = &geometry.root {
                layout.root(root.bounds, root.clip);
            }
            layout.finish(&groups);
        };
        let mut layout = TreeLayout::default();
        paint(&mut layout, &map);
        let mut drag =
            Drag::new(&s, &selected, &BTreeSet::new(), 2, at(&map, 2, 13.), true).unwrap();
        drag.geometry = Some((layout.epoch, map.clone()));
        assert!(drag.geometry_current(&layout));
        paint(&mut layout, &map);
        assert!(drag.geometry_current(&layout));
        layout.invalidate();
        assert!(layout.current().is_none());
        assert!(!drag.geometry_current(&layout));
        // Source rows staying equal does not authorize a press between native
        // scroll/resize and the next completed prepaint generation.
        paint(&mut layout, &map);
        assert!(layout.current().is_some());
        let mut moved = map.clone();
        moved.rows[0].clip.size.width -= px(1.);
        paint(&mut layout, &moved);
        assert!(!drag.geometry_current(&layout));
        paint(&mut layout, &map);
        assert!(!drag.geometry_current(&layout));
        layout.begin(map.owner, map.visible.clone());
        layout.finish(&groups);
        assert!(layout.current().unwrap().rows.is_empty());
        assert!(layout.current().unwrap().root.is_none());
        assert!(!drag.geometry_current(&layout));
    }
    #[test]
    fn outside_label_down_invalidates_scrollbar_and_splitter_geometry_until_next_paint() {
        let s = scene();
        let map = geometry(&s, &BTreeSet::new());
        let mut layout = TreeLayout::default();
        layout.latest = Some(map.clone());
        layout.previous = Some(map.clone());
        assert!(!layout.invalidate_outside_label(at(&map, 2, 13.)));
        assert_eq!(layout.current(), Some(&map));
        // Eye/disclosure, root landing, scrollbar and panel chrome are not labels.
        for position in [
            point(px(20.), px(1.)),
            map.root.as_ref().unwrap().bounds.center(),
            point(px(299.), px(100.)),
        ] {
            layout.latest = Some(map.clone());
            assert!(layout.invalidate_outside_label(position));
            assert!(layout.current().is_none());
            assert!(layout.invalidate_outside_label(at(&map, 2, 13.)));
            assert_eq!(layout.previous.as_ref(), Some(&map));
        }
        let groups = map
            .rows
            .iter()
            .filter(|r| r.group)
            .map(|r| r.item)
            .collect();
        layout.begin(map.owner, map.visible.clone());
        for row in &map.rows {
            layout.row(row.item, row.bounds, row.clip);
            layout.label(row.item, row.label);
        }
        if let Some(root) = &map.root {
            layout.root(root.bounds, root.clip);
        }
        layout.finish(&groups);
        assert!(!layout.invalidate_outside_label(at(&map, 2, 13.)));
    }
    #[test]
    fn drag_pointer_modifiers_allow_selection_chords_but_reject_alt_platform_and_function() {
        for control in [false, true] {
            for shift in [false, true] {
                let m = gpui::Modifiers {
                    control,
                    shift,
                    ..Default::default()
                };
                assert!(supported_modifiers(m));
                assert!(!supported_modifiers(gpui::Modifiers { alt: true, ..m }));
                assert!(!supported_modifiers(gpui::Modifiers {
                    platform: true,
                    ..m
                }));
                assert!(!supported_modifiers(gpui::Modifiers {
                    function: true,
                    ..m
                }));
            }
        }
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
                        let expected = if !shift && !alt && !other && matches!(key, "c" | "x" | "v")
                        {
                            KeyRoute::Handle
                        } else {
                            KeyRoute::Consume
                        };
                        assert_eq!(key_route(owned, &s), expected, "{key}");
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
            ("left", true),
            ("right", true),
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
            TreeKey::MoveInto,
            TreeKey::MoveOut,
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
