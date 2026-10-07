//! Session-local sibling clipboard and one-use guarded input receipts.
use super::{tree_selection::*, *};
use libre_effects_core::{CompositionId, ContentsClipboard, Project, ShapeContents};
use std::{collections::BTreeSet, sync::Arc};

const HELP: &str =
    "Keeps original key frames and local values; placement and paint scope may change.";
const STALE: &str = "Contents clipboard action expired; select the destination again";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Copy,
    Cut,
    Paste,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Destination {
    parent: u64,
    index: usize,
}
fn destination(contents: &ShapeContents, selection: &Selection) -> Option<Destination> {
    bounded_rows(contents)?;
    if selection.items.is_empty() {
        return Some(Destination {
            parent: 0,
            index: contents.items.len(),
        });
    }
    let parent = selection.parent?;
    let order = sibling_order(contents, parent)?;
    if !selection.items.iter().all(|id| order.contains(id)) {
        return None;
    }
    if let Some(item) = selection.singleton()
        && let ContentsKind::Group(children) = &contents.node(item)?.kind
    {
        return Some(Destination {
            parent: item,
            index: children.len(),
        });
    }
    Some(Destination {
        parent,
        index: order.iter().rposition(|id| selection.items.contains(id))? + 1,
    })
}

#[derive(Clone)]
struct Binding {
    serial: u64,
    source: Arc<Project>,
    revision: u64,
    transport: u64,
    generation: u64,
    composition: CompositionId,
    layer: u64,
    frame: u32,
    tool: crate::editor::Tool,
    gradient: Option<crate::color_edit::GradientTarget>,
    selected_layers: BTreeSet<u64>,
    selection: Selection,
    collapsed: BTreeSet<u64>,
    clipboard: Option<ContentsClipboard>,
}
impl Binding {
    fn capture(
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        serial: u64,
    ) -> Option<Self> {
        if tree::blocked(state) {
            return None;
        }
        let layer = state.editor.selected_layer().filter(|l| !l.locked())?;
        let Content::ShapeContents(contents) = layer.content() else {
            return None;
        };
        let composition = state.editor.project().active_composition_id();
        if state.contents_selection
            != selection
                .singleton()
                .map(|id| (composition, layer.id(), id))
        {
            return None;
        }
        destination(contents, selection)?;
        let visible = visible_rows(contents, collapsed);
        if !selection
            .items
            .iter()
            .all(|id| visible.iter().any(|(_, _, found)| found == id))
        {
            return None;
        }
        Some(Self {
            serial,
            source: Arc::new(state.editor.project().clone()),
            revision: state.document_revision,
            transport: state.transport_generation(),
            generation: state.input_context_generation(),
            composition,
            layer: layer.id(),
            frame: state.frame,
            tool: state.tool,
            gradient: state.gradient_controls,
            selected_layers: state.selected_layers.clone(),
            selection: selection.clone(),
            collapsed: collapsed.clone(),
            clipboard: state.contents_clipboard().cloned(),
        })
    }
    fn same_owner(
        &self,
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
    ) -> bool {
        !tree::blocked(state)
            && self.revision == state.document_revision
            && self.composition == state.editor.project().active_composition_id()
            && state.editor.selected() == Some(self.layer)
            && state
                .editor
                .selected_layer()
                .is_some_and(|l| !l.locked() && matches!(l.content(), Content::ShapeContents(_)))
            && self.frame == state.frame
            && self.tool == state.tool
            && self.gradient == state.gradient_controls
            && self.selected_layers == state.selected_layers
            && self.selection == *selection
            && self.collapsed == *collapsed
            && state.contents_selection
                == selection
                    .singleton()
                    .map(|id| (self.composition, self.layer, id))
            && self.clipboard.as_ref() == state.contents_clipboard()
    }
    fn current(
        &self,
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        serial: u64,
    ) -> bool {
        self.serial == serial
            && self.same_owner(state, selection, collapsed)
            && self.source.as_ref() == state.editor.project()
            && self.transport == state.transport_generation()
            && self.generation == state.input_context_generation()
    }
    fn available(&self, operation: Operation) -> bool {
        match operation {
            Operation::Copy | Operation::Cut => !self.selection.items.is_empty(),
            Operation::Paste => self.clipboard.is_some(),
        }
    }
}

#[derive(Default)]
pub(super) struct Session {
    serial: u64,
    action_serial: u64,
    binding: Option<Binding>,
    armed: Option<(u64, Option<String>)>,
    flushed: Option<(u64, Binding)>,
}
impl Session {
    pub(super) fn invalidate(&mut self) {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("Contents clipboard serial exhausted");
        self.action_serial = self.serial;
        self.binding = None;
        self.armed = None;
        self.flushed = None;
    }
    fn observe(&mut self, state: &EditorState, selection: &Selection, collapsed: &BTreeSet<u64>) {
        if self
            .binding
            .as_ref()
            .is_some_and(|b| b.current(state, selection, collapsed, self.serial))
        {
            return;
        }
        if self.binding.is_some() {
            self.invalidate();
        }
        self.binding = Binding::capture(state, selection, collapsed, self.serial);
    }
    fn current(
        &self,
        target: &Binding,
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
    ) -> bool {
        self.binding.is_some() && target.current(state, selection, collapsed, self.serial)
    }
    fn prepare(
        &mut self,
        target: &Binding,
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        pending: Option<String>,
    ) -> bool {
        self.armed = None;
        self.flushed = None;
        if !self.current(target, state, selection, collapsed) {
            return false;
        }
        self.armed = Some((target.serial, pending));
        true
    }
    fn action_target(
        &self,
        target: &Binding,
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        pointer: bool,
    ) -> Option<Binding> {
        if !pointer {
            return self
                .current(target, state, selection, collapsed)
                .then(|| target.clone());
        }
        if (self.armed == Some((target.serial, None))
            || self
                .flushed
                .as_ref()
                .is_some_and(|(_, next)| next.serial == target.serial))
            && self.current(target, state, selection, collapsed)
        {
            return Some(target.clone());
        }
        let (origin, next) = self.flushed.as_ref()?;
        (*origin == target.serial && self.current(next, state, selection, collapsed))
            .then(|| next.clone())
    }
    /// End the synchronous flush window. Keep only a click receipt, never an
    /// armed field permission that could reject a later ordinary Enter/blur
    /// when the pointer is canceled by releasing outside the button.
    fn finish_flush(
        &mut self,
        target: &Binding,
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
    ) -> bool {
        let next = self.action_target(target, state, selection, collapsed, true);
        self.armed = None;
        if let Some(next) = next {
            self.flushed = Some((target.serial, next));
            true
        } else {
            self.invalidate();
            false
        }
    }
    /// A pointer press authorizes exactly the field that was pending at down.
    /// Reject another callback before it can mutate source or history.
    pub(super) fn allow_field(&mut self, state: &EditorState, field: &str) -> bool {
        let allowed = match &self.armed {
            None => true,
            Some((origin, Some(expected))) => {
                expected == field
                    && self.binding.as_ref().is_some_and(|binding| {
                        binding.serial == *origin
                            && binding.current(
                                state,
                                &binding.selection,
                                &binding.collapsed,
                                self.serial,
                            )
                    })
            }
            Some((_, None)) => false,
        };
        if !allowed {
            self.invalidate();
        }
        allowed
    }
    /// Only a Contents field's actual synchronous callback can grant a rebase.
    /// A parse/core/stale rejection retires the press even when source is equal.
    pub(super) fn field_submitted(
        &mut self,
        state: &EditorState,
        valid: bool,
        field: &str,
        generation: u64,
        transport: u64,
    ) {
        let Some((origin, Some(expected))) = self.armed.clone() else {
            return;
        };
        let Some(before) = self.binding.clone() else {
            return;
        };
        if origin != before.serial {
            return;
        }
        let action_serial = self.action_serial;
        self.invalidate();
        if valid
            && field == expected
            && generation == before.generation
            && transport == before.transport
            && generation.checked_add(1) == Some(state.input_context_generation())
            && transport.wrapping_add(1) == state.transport_generation()
            && before.same_owner(state, &before.selection, &before.collapsed)
            && let Some(next) =
                Binding::capture(state, &before.selection, &before.collapsed, self.serial)
        {
            self.binding = Some(next.clone());
            self.flushed = Some((origin, next));
            self.action_serial = action_serial;
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    parent: u64,
    items: Vec<u64>,
}
/// Mutation is injected so tests use EditorState's real apply/normalize path.
fn execute(
    state: &mut EditorState,
    target: &Binding,
    operation: Operation,
    apply: impl FnOnce(&mut EditorState, Command) -> bool,
) -> Option<Outcome> {
    if !target.available(operation)
        || !target.current(state, &target.selection, &target.collapsed, target.serial)
    {
        return None;
    }
    let items: Vec<_> = target.selection.items.iter().copied().collect();
    if operation != Operation::Paste {
        let clipboard =
            match state
                .editor
                .copy_contents(target.layer, target.selection.parent?, &items)
            {
                Ok(clipboard) => clipboard,
                Err(error) => {
                    state.status = error;
                    return None;
                }
            };
        let count = clipboard.len();
        if operation == Operation::Cut
            && !apply(
                state,
                Command::Contents {
                    id: target.layer,
                    edit: ContentsEdit::RemoveSiblings {
                        parent: target.selection.parent?,
                        items,
                    },
                },
            )
        {
            return None;
        }
        state.set_contents_clipboard(clipboard);
        state.status = format!(
            "{} {count} Contents items",
            if operation == Operation::Cut {
                "Cut"
            } else {
                "Copied"
            }
        );
        return Some(Outcome {
            parent: target.selection.parent?,
            items: if operation == Operation::Copy {
                target.selection.items.iter().copied().collect()
            } else {
                vec![]
            },
        });
    }
    let Content::ShapeContents(contents) = state.editor.selected_layer()?.content() else {
        return None;
    };
    let destination = destination(contents, &target.selection)?;
    let original: BTreeSet<_> = sibling_order(contents, destination.parent)?
        .into_iter()
        .collect();
    let clipboard = state.contents_clipboard()?.clone();
    if !apply(
        state,
        Command::Contents {
            id: target.layer,
            edit: ContentsEdit::Paste {
                parent: destination.parent,
                index: destination.index,
                clipboard,
            },
        },
    ) {
        return None;
    }
    let Content::ShapeContents(contents) = state.editor.selected_layer()?.content() else {
        return None;
    };
    let items: Vec<_> = sibling_order(contents, destination.parent)?
        .into_iter()
        .filter(|id| !original.contains(id))
        .collect();
    if items.is_empty() {
        return None;
    }
    state.status = format!("Pasted {} Contents items. {HELP}", items.len());
    Some(Outcome {
        parent: destination.parent,
        items,
    })
}

fn field_display(
    target: &Binding,
    parameter: Option<ContentsParam>,
    project: &Project,
) -> Option<String> {
    let Content::ShapeContents(contents) = project
        .composition_by_id(target.composition)?
        .layer(target.layer)?
        .content()
    else {
        return None;
    };
    let node = contents.node(target.selection.singleton()?)?;
    Some(
        parameter
            .map(|p| node.value_at(p, target.frame).to_string())
            .unwrap_or_else(|| node.name.clone()),
    )
}
fn submit_singleton(
    session: &Rc<RefCell<Session>>,
    target: &Binding,
    parameter: Option<ContentsParam>,
    state: &mut EditorState,
    text: &str,
    apply: impl FnOnce(&mut EditorState, Command) -> bool,
) -> String {
    let mut valid = false;
    let generation = state.input_context_generation();
    let transport = state.transport_generation();
    let field = format!("contents-single-{}-{parameter:?}", target.serial);
    if !session.borrow_mut().allow_field(state, &field)
        || !session
            .borrow()
            .current(target, state, &target.selection, &target.collapsed)
    {
        state.status = STALE.into();
    } else if let Some(item) = target.selection.singleton() {
        let edit = if let Some(parameter) = parameter {
            text.trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|value| ContentsEdit::Track {
                    item,
                    parameter,
                    edit: TrackEdit::Value {
                        frame: target.frame,
                        value,
                    },
                })
        } else {
            Some(ContentsEdit::Rename {
                item,
                name: text.to_string(),
            })
        };
        if let Some(edit) = edit {
            valid = apply(
                state,
                Command::Contents {
                    id: target.layer,
                    edit,
                },
            );
        } else {
            state.status = "Enter a finite Contents value".into();
        }
    }
    session.borrow_mut().field_submitted(
        state,
        valid,
        &format!("contents-single-{}-{parameter:?}", target.serial),
        generation,
        transport,
    );
    field_display(target, parameter, state.editor.project())
        .or_else(|| field_display(target, parameter, &target.source))
        .unwrap_or_default()
}

impl ContentsControls {
    pub(super) fn sync_clipboard_field(
        &self,
        field: &Entity<TextField>,
        parameter: Option<ContentsParam>,
        value: String,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.clipboard_session.borrow().binding.clone() else {
            return;
        };
        let session = self.clipboard_session.clone();
        let state = self.state.clone();
        field.update(cx, |field, _| {
            field.sync_guarded(
                format!("contents-single-{}-{parameter:?}", target.serial),
                value,
                w,
                move |text, w, cx| {
                    state.update(cx, |state, cx| {
                        let result = submit_singleton(
                            &session,
                            &target,
                            parameter,
                            state,
                            text,
                            |state, command| {
                                state.dispatch(&Action::Edit(command), w, cx);
                                state.status == "Edited"
                            },
                        );
                        cx.notify();
                        result
                    })
                },
            )
        });
    }
    pub(super) fn refresh_clipboard_context(&mut self, cx: &Context<Self>) {
        self.clipboard_session.borrow_mut().observe(
            self.state.read(cx),
            &self.selection,
            &self.collapsed,
        );
    }
    pub(super) fn clipboard_actions(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        self.refresh_clipboard_context(cx);
        let mut row = div().flex().flex_col().gap_1();
        let Some(binding) = self.clipboard_session.borrow().binding.clone() else {
            return row;
        };
        let destination =
            self.state
                .read(cx)
                .editor
                .selected_layer()
                .and_then(|l| match l.content() {
                    Content::ShapeContents(c) => destination(c, &self.selection),
                    _ => None,
                });
        let destination_label = match destination {
            Some(Destination { parent: 0, .. }) if self.selection.items.is_empty() => {
                "Paste: end of Contents".to_string()
            }
            Some(Destination { parent, .. }) if self.selection.singleton() == Some(parent) => {
                "Paste: inside selected Group".into()
            }
            _ => "Paste: after last selected sibling".into(),
        };
        let mut buttons = div().flex().flex_wrap().gap_1();
        for (operation, label) in [
            (Operation::Copy, "Copy"),
            (Operation::Cut, "Cut"),
            (Operation::Paste, "Paste"),
        ] {
            let available = binding.available(operation);
            let target = binding.clone();
            let guarded = binding.clone();
            let owner = cx.entity();
            let input = crate::color_edit::InputTarget::new(self.state.read(cx));
            let control = format!(
                "contents-clipboard-{}-{operation:?}",
                self.clipboard_session.borrow().action_serial
            );
            let button = ui::text_button(gpui::SharedString::from(control.clone()), label)
                .when(!available, |b| b.opacity(0.4));
            buttons = buttons.child(crate::color_edit::input_pointer_button_guarded(button, control.clone(), input.clone(), move |state, cx, after_flush| {
                let owner = owner.read(cx);
                let mut session = owner.clipboard_session.borrow_mut();
                if !available { return false; }
                if after_flush {
                    session.finish_flush(&guarded, state, &owner.selection, &owner.collapsed)
                } else {
                    session.prepare(&guarded, state, &owner.selection, &owner.collapsed, TextField::active_pending_binding(cx))
                }
            }).on_click(cx.listener(move |this, event: &gpui::ClickEvent, w, cx| {
                cx.stop_propagation();
                if !available || event.modifiers().modified()
                    || matches!(event, gpui::ClickEvent::Mouse(e) if e.down.modifiers.modified())
                    || TextField::is_composing(w, cx) { return; }
                let pointer = matches!(event, gpui::ClickEvent::Mouse(_));
                let bound = this.clipboard_session.borrow().action_target(&target, this.state.read(cx), &this.selection, &this.collapsed, pointer);
                let Some(bound) = bound else { return; };
                if crate::color_edit::input_click_target(&control, event, &input, &this.state, w, cx).is_none() { return; }
                this.run_clipboard(operation, &bound, w, cx);
            })));
        }
        row = row.child(buttons).child(
            div()
                .text_size(px(10.))
                .text_color(rgb(ui::MUTED))
                .child(destination_label),
        );
        row.child(
            div()
                .text_size(px(10.))
                .text_color(rgb(ui::MUTED))
                .child(HELP),
        )
    }
    pub(super) fn clipboard_key(
        &mut self,
        operation: Operation,
        held: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if held {
            return;
        }
        let Some(target) = self.clipboard_session.borrow().binding.clone() else {
            return;
        };
        self.run_clipboard(operation, &target, w, cx);
    }
    fn run_clipboard(
        &mut self,
        operation: Operation,
        target: &Binding,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !w.is_window_active()
            || TextField::is_composing(w, cx)
            || !self.clipboard_session.borrow().current(
                target,
                self.state.read(cx),
                &self.selection,
                &self.collapsed,
            )
            || self.owner != Some((target.composition, target.layer))
            || self.owner_revision != target.revision
        {
            return;
        }
        self.clipboard_session.borrow_mut().invalidate();
        let result = self.state.update(cx, |state, cx| {
            let result = execute(state, target, operation, |state, command| {
                state.dispatch(&Action::Edit(command), w, cx);
                state.status == "Edited"
            });
            cx.notify();
            result
        });
        let Some(outcome) = result else {
            return;
        };
        if operation != Operation::Copy {
            self.selection.all(outcome.parent, &outcome.items);
            if outcome.items.is_empty() {
                self.selection = Selection::default();
            }
            if let Some(Content::ShapeContents(contents)) = self
                .state
                .read(cx)
                .editor
                .selected_layer()
                .map(|l| l.content())
            {
                reveal(contents, outcome.parent, &mut self.collapsed);
            }
            self.publish_tree_selection(cx);
        }
        w.focus(&self.tree_focus);
        cx.notify();
    }
}
fn reveal(contents: &ShapeContents, mut parent: u64, collapsed: &mut BTreeSet<u64>) {
    let Some(rows) = bounded_rows(contents) else {
        return;
    };
    while parent != 0 {
        collapsed.remove(&parent);
        parent = rows
            .iter()
            .find(|(_, _, n)| n.id == parent)
            .map(|(_, p, _)| *p)
            .unwrap_or(0);
    }
}

#[cfg(test)]
mod tests;
