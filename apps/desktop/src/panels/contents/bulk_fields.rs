//! Source-bound sibling numeric fields. This session is owned by Contents, not
//! the public singleton Contents target used by Pen and gradient overlays.
use super::{ContentsControls, TextField, tree, tree_selection::Selection, *};
use libre_effects_core::{CompositionId, ContentsAnimationAction, Frame, Project, ShapeContents};
use std::{collections::BTreeSet, sync::Arc};

const HELP: &str = "Sets this value on all selected items. Static properties stay static; animated properties update at the playhead.";
const ANIMATION_HELP: &str = "Enable animation and Add key keep each item's own value. Disable animation removes ALL keys and keeps each item's value at the playhead. Remove key affects only the playhead; a final key becomes that item's static value.";
const STALE: &str = "Contents selection or editing context changed; value was not applied";

#[derive(Clone)]
struct Binding {
    serial: u64,
    project: Arc<Project>,
    revision: u64,
    transport: u64,
    input_generation: u64,
    composition: CompositionId,
    layer: u64,
    frame: Frame,
    tool: crate::editor::Tool,
    gradient_controls: Option<crate::color_edit::GradientTarget>,
    selected_layers: BTreeSet<u64>,
    selection: Selection,
    collapsed: BTreeSet<u64>,
}
impl Binding {
    fn capture(
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        serial: u64,
    ) -> Option<Self> {
        if tree::blocked(state) || selection.items.len() < 2 || state.contents_selection.is_some() {
            return None;
        }
        let layer = state.editor.selected_layer().filter(|l| !l.locked())?;
        let Content::ShapeContents(contents) = layer.content() else {
            return None;
        };
        let items: Vec<_> = selection.items.iter().copied().collect();
        contents.shared_parameters(selection.parent?, &items).ok()?;
        let visible = super::tree_selection::visible_rows(contents, collapsed);
        if !items
            .iter()
            .all(|id| visible.iter().any(|(_, _, item)| item == id))
        {
            return None;
        }
        Some(Self {
            serial,
            project: Arc::new(state.editor.project().clone()),
            revision: state.document_revision,
            transport: state.transport_generation(),
            input_generation: state.input_context_generation(),
            composition: state.editor.project().active_composition_id(),
            layer: layer.id(),
            frame: state.frame,
            tool: state.tool,
            gradient_controls: state.gradient_controls,
            selected_layers: state.selected_layers.clone(),
            selection: selection.clone(),
            collapsed: collapsed.clone(),
        })
    }
    fn current(
        &self,
        state: &EditorState,
        selection: &Selection,
        collapsed: &BTreeSet<u64>,
        serial: u64,
    ) -> bool {
        self.serial == serial
            && !tree::blocked(state)
            && state.contents_selection.is_none()
            && self.revision == state.document_revision
            && self.transport == state.transport_generation()
            && self.input_generation == state.input_context_generation()
            && self.composition == state.editor.project().active_composition_id()
            && state.editor.selected() == Some(self.layer)
            && state.editor.selected_layer().is_some_and(|l| !l.locked())
            && self.frame == state.frame
            && self.tool == state.tool
            && self.gradient_controls == state.gradient_controls
            && self.selected_layers == state.selected_layers
            && self.selection == *selection
            && self.collapsed == *collapsed
            && self.project.as_ref() == state.editor.project()
    }
    fn display(&self, project: &Project, parameter: ContentsParam) -> Option<String> {
        if project.active_composition_id() != self.composition {
            return None;
        }
        let Content::ShapeContents(contents) = project.composition().layer(self.layer)?.content()
        else {
            return None;
        };
        display(contents, &self.selection, parameter, self.frame)
    }
    /// Only the synchronous guarded field submission may change source and its
    /// action generations. Everything identifying the visible owner stays fixed.
    fn same_owner(&self, state: &EditorState) -> bool {
        !tree::blocked(state)
            && state.contents_selection.is_none()
            && self.revision == state.document_revision
            && self.composition == state.editor.project().active_composition_id()
            && state.editor.selected() == Some(self.layer)
            && state.editor.selected_layer().is_some_and(|l| !l.locked())
            && self.frame == state.frame
            && self.tool == state.tool
            && self.gradient_controls == state.gradient_controls
            && self.selected_layers == state.selected_layers
    }
}

/// All rendered callbacks capture a serial. Source/context changes retire it;
/// publishing any selection also retires it, including A → B → A transitions.
#[derive(Default)]
pub(super) struct Session {
    serial: u64,
    action_serial: u64,
    binding: Option<Binding>,
    armed: Option<u64>,
    flushed: Option<(u64, Binding)>,
}
impl Session {
    pub(super) fn invalidate(&mut self) {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("Contents input serial exhausted");
        self.binding = None;
        self.action_serial = self.serial;
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
    fn target(&self, parameter: ContentsParam) -> Option<FieldTarget> {
        let binding = self.binding.clone()?;
        binding.display(&binding.project, parameter)?;
        Some(FieldTarget { binding, parameter })
    }
    fn current(&self, target: &FieldTarget, state: &EditorState) -> bool {
        self.binding.as_ref().is_some_and(|current| {
            target
                .binding
                .current(state, &current.selection, &current.collapsed, self.serial)
        })
    }
    fn prepare_action(&mut self, target: &FieldTarget, state: &EditorState) -> bool {
        self.armed = None;
        self.flushed = None;
        if !self.current(target, state) {
            return false;
        }
        self.armed = Some(target.binding.serial);
        true
    }
    fn action_target(&self, target: &FieldTarget, state: &EditorState) -> Option<FieldTarget> {
        if self.current(target, state) {
            return Some(target.clone());
        }
        let (origin, binding) = self.flushed.as_ref()?;
        if *origin != target.binding.serial {
            return None;
        }
        let next = FieldTarget {
            binding: binding.clone(),
            parameter: target.parameter,
        };
        self.current(&next, state).then_some(next)
    }
}

#[derive(Clone)]
struct FieldTarget {
    binding: Binding,
    parameter: ContentsParam,
}
impl FieldTarget {
    fn binding_key(&self) -> String {
        format!(
            "contents-shared-{}-{:?}",
            self.binding.serial, self.parameter
        )
    }
    fn command(&self, text: &str) -> Result<Command, String> {
        let value = text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| "Enter a finite absolute Contents value".to_string())?;
        let (min, max) = self.parameter.bounds();
        if !(min..=max).contains(&value) {
            return Err(format!(
                "{} must be between {min} and {max}",
                self.parameter.label()
            ));
        }
        Ok(Command::Contents {
            id: self.binding.layer,
            edit: ContentsEdit::SetSharedValue {
                parent: self.binding.selection.parent.expect("Bound sibling parent"),
                items: self.binding.selection.items.iter().copied().collect(),
                parameter: self.parameter,
                frame: self.binding.frame,
                value,
            },
        })
    }
    fn animation_command(&self, action: ContentsAnimationAction) -> Command {
        Command::Contents {
            id: self.binding.layer,
            edit: ContentsEdit::SharedAnimation {
                parent: self.binding.selection.parent.expect("Bound sibling parent"),
                items: self.binding.selection.items.iter().copied().collect(),
                parameter: self.parameter,
                frame: self.binding.frame,
                action,
            },
        }
    }
}

/// The production TextField callback uses this same synchronous session path.
/// `apply` is supplied by the UI so the model is testable without native focus or
/// a window. Rejection and acceptance both return an authoritative baseline.
fn submit(
    session: &Rc<RefCell<Session>>,
    target: &FieldTarget,
    state: &mut EditorState,
    text: &str,
    apply: impl FnOnce(&mut EditorState, Command) -> bool,
) -> String {
    let current = session.borrow().current(target, state);
    let armed = session.borrow().armed == Some(target.binding.serial);
    let action_serial = session.borrow().action_serial;
    let mut valid = false;
    if !current {
        state.status = STALE.into();
    } else {
        match target.command(text) {
            Ok(command) => {
                valid = apply(state, command);
            }
            Err(error) => state.status = error,
        }
    }
    // Consume before any later blur/Enter receipt. Never alter a newer session
    // when an already-retired callback happens to arrive late.
    if session.borrow().serial == target.binding.serial {
        session.borrow_mut().invalidate();
        if armed && valid && target.binding.same_owner(state) {
            let mut session = session.borrow_mut();
            if let Some(binding) = Binding::capture(
                state,
                &target.binding.selection,
                &target.binding.collapsed,
                session.serial,
            ) {
                session.binding = Some(binding.clone());
                session.flushed = Some((target.binding.serial, binding));
                // Preserve pointer button identity through this one authorized
                // flush/repaint, but not through any independent invalidation.
                session.action_serial = action_serial;
            }
        }
    }
    target
        .binding
        .display(state.editor.project(), target.parameter)
        .or_else(|| {
            target
                .binding
                .display(&target.binding.project, target.parameter)
        })
        .unwrap_or_else(|| "Mixed".into())
}

fn submit_clipboard_field(
    session: &Rc<RefCell<Session>>,
    clipboard: &Rc<RefCell<super::clipboard::Session>>,
    target: &FieldTarget,
    state: &mut EditorState,
    text: &str,
    apply: impl FnOnce(&mut EditorState, Command) -> bool,
) -> String {
    if !clipboard
        .borrow_mut()
        .allow_field(state, &target.binding_key())
    {
        state.status = STALE.into();
        return target
            .binding
            .display(state.editor.project(), target.parameter)
            .or_else(|| {
                target
                    .binding
                    .display(&target.binding.project, target.parameter)
            })
            .unwrap_or_else(|| "Mixed".into());
    }
    let generation = state.input_context_generation();
    let transport = state.transport_generation();
    let mut valid = false;
    let display = submit(session, target, state, text, |state, command| {
        valid = apply(state, command);
        valid
    });
    clipboard.borrow_mut().field_submitted(
        state,
        valid,
        &target.binding_key(),
        generation,
        transport,
    );
    display
}
#[cfg(test)]
pub(super) fn submit_clipboard_test(
    clipboard: &Rc<RefCell<super::clipboard::Session>>,
    state: &mut EditorState,
    selection: &Selection,
    parameter: ContentsParam,
    text: &str,
) -> String {
    let session = Rc::new(RefCell::new(Session::default()));
    session
        .borrow_mut()
        .observe(state, selection, &BTreeSet::new());
    let target = session.borrow().target(parameter).unwrap();
    submit_clipboard_field(
        &session,
        clipboard,
        &target,
        state,
        text,
        |state, command| {
            state.bulk_test_action(&Action::Edit(command));
            state.status == "Edited"
        },
    )
}

fn submit_animation(
    session: &Rc<RefCell<Session>>,
    rendered: &FieldTarget,
    state: &mut EditorState,
    after_flush: bool,
    action: ContentsAnimationAction,
    apply: impl FnOnce(&mut EditorState, Command) -> bool,
) -> bool {
    let target = if after_flush {
        session.borrow().action_target(rendered, state)
    } else {
        session
            .borrow()
            .current(rendered, state)
            .then(|| rendered.clone())
    };
    let Some(target) = target else {
        state.status = STALE.into();
        return false;
    };
    // Consume before dispatch: a duplicate click/Enter cannot replay this intent.
    session.borrow_mut().invalidate();
    apply(state, target.animation_command(action))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AnimationState {
    total: usize,
    animated: usize,
    keyed: usize,
}
impl AnimationState {
    fn actions(self) -> Vec<(ContentsAnimationAction, &'static str)> {
        let mut actions = Vec::new();
        if self.animated < self.total {
            actions.push((ContentsAnimationAction::Enable, "Enable animation"));
        }
        if self.animated > 0 {
            actions.push((ContentsAnimationAction::Disable, "Disable animation"));
        }
        if self.keyed < self.total {
            actions.push((ContentsAnimationAction::AddKey, "Add key"));
        }
        if self.keyed > 0 {
            actions.push((ContentsAnimationAction::RemoveKey, "Remove key"));
        }
        actions
    }
    fn label(self) -> String {
        let animation = if self.animated == 0 {
            "Static"
        } else if self.animated == self.total {
            "Animated"
        } else {
            "Mixed animation"
        };
        let keys = if self.keyed == 0 {
            "No key"
        } else if self.keyed == self.total {
            "Key at playhead"
        } else {
            "Mixed keys"
        };
        format!("{animation} · {keys}")
    }
}
fn animation_state(contents: &ShapeContents, target: &FieldTarget) -> Option<AnimationState> {
    let mut state = AnimationState {
        total: target.binding.selection.items.len(),
        animated: 0,
        keyed: 0,
    };
    for item in &target.binding.selection.items {
        let track = contents.node(*item)?.parameters.get(&target.parameter)?;
        state.animated += usize::from(!track.keys().is_empty());
        state.keyed += usize::from(track.keys().contains_key(&target.binding.frame));
    }
    Some(state)
}

fn display(
    contents: &ShapeContents,
    selection: &Selection,
    parameter: ContentsParam,
    frame: Frame,
) -> Option<String> {
    let items: Vec<_> = selection.items.iter().copied().collect();
    if !contents
        .shared_parameters(selection.parent?, &items)
        .ok()?
        .contains(&parameter)
    {
        return None;
    }
    // Source order, not click order or ID order, determines the uniform sample's
    // representation (including signed zero). Equality is exact, never rounded.
    let order = super::tree_selection::sibling_order(contents, selection.parent?)?;
    let mut values = order
        .into_iter()
        .filter(|id| selection.items.contains(id))
        .map(|id| contents.node(id).unwrap().value_at(parameter, frame));
    let first = values.next()?;
    Some(if values.all(|value| value == first) {
        first.to_string()
    } else {
        "Mixed".into()
    })
}

impl ContentsControls {
    fn shared_animation_buttons(
        &self,
        target: &FieldTarget,
        summary: AnimationState,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let mut buttons = div().flex().flex_wrap().items_center().gap_1();
        let rendered_input = crate::color_edit::InputTarget::new(self.state.read(cx));
        for (action, label) in summary.actions() {
            let control = format!(
                "contents-shared-animation-{}-{:?}-{action:?}",
                self.bulk_session.borrow().action_serial,
                target.parameter,
            );
            let owner = cx.entity();
            let guarded_target = target.clone();
            let target = target.clone();
            let input = rendered_input.clone();
            let button = ui::text_button(gpui::SharedString::from(control.clone()), label)
                .tooltip(|_, cx| cx.new(|_| ui::Tip(ANIMATION_HELP.into())).into());
            buttons = buttons.child(
                crate::color_edit::input_pointer_button_guarded(
                    button, control.clone(), input.clone(),
                    move |state, cx, after_flush| {
                        let owner = owner.read(cx);
                        let mut session = owner.bulk_session.borrow_mut();
                        let bound = if after_flush {
                            session.action_target(&guarded_target, state)
                        } else {
                            session.current(&guarded_target, state).then(|| guarded_target.clone())
                        };
                        let Some(bound) = bound else { return false; };
                        if !bound.binding.current(state, &owner.selection, &owner.collapsed, session.serial) {
                            return false;
                        }
                        after_flush || session.prepare_action(&guarded_target, state)
                    },
                )
                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, w, cx| {
                    cx.stop_propagation();
                    if event.modifiers().modified()
                        || matches!(event, gpui::ClickEvent::Mouse(click) if click.down.modifiers.modified())
                        || TextField::is_composing(w, cx)
                    { return; }
                    let after_flush = matches!(event, gpui::ClickEvent::Mouse(_));
                    let bound = if after_flush {
                        this.bulk_session.borrow().action_target(&target, this.state.read(cx))
                    } else {
                        this.bulk_session.borrow().current(&target, this.state.read(cx)).then(|| target.clone())
                    };
                    let Some(bound) = bound else { return; };
                    if !bound.binding.current(this.state.read(cx), &this.selection, &this.collapsed, this.bulk_session.borrow().serial)
                        || crate::color_edit::input_click_target(&control, event, &input, &this.state, w, cx).is_none()
                    { return; }
                    let session = this.bulk_session.clone();
                    this.state.update(cx, |state, cx| {
                        submit_animation(&session, &target, state, after_flush, action, |state, command| {
                            state.dispatch(&Action::Edit(command), w, cx);
                            state.status == "Edited"
                        });
                        cx.notify();
                    });
                    cx.notify();
                })),
            );
        }
        buttons
    }

    pub(super) fn refresh_bulk_context(&mut self, cx: &Context<Self>) {
        self.bulk_session.borrow_mut().observe(
            self.state.read(cx),
            &self.selection,
            &self.collapsed,
        );
    }
    pub(super) fn shared_numeric_fields(
        &mut self,
        contents: &ShapeContents,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.refresh_bulk_context(cx);
        let mut root =
            div()
                .flex()
                .flex_col()
                .gap_1()
                .mt_2()
                .child(div().text_size(px(11.)).child(format!(
                    "{} items · Shared numeric properties",
                    self.selection.items.len()
                )));
        let items: Vec<_> = self.selection.items.iter().copied().collect();
        let parameters = self
            .selection
            .parent
            .and_then(|parent| contents.shared_parameters(parent, &items).ok())
            .unwrap_or_default();
        if parameters.is_empty() {
            self.bulk_fields.clear();
            return root.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(ui::MUTED))
                    .child("No shared numeric properties"),
            );
        }
        if !self
            .bulk_fields
            .iter()
            .map(|(p, _)| *p)
            .eq(parameters.iter().copied())
        {
            self.bulk_fields = parameters
                .iter()
                .map(|&parameter| (parameter, cx.new(|cx| TextField::new(cx, |_, _, _| {}))))
                .collect();
        }
        let frame = self.state.read(cx).frame;
        for (parameter, field) in &self.bulk_fields {
            let value = display(contents, &self.selection, *parameter, frame)
                .unwrap_or_else(|| "Mixed".into());
            let target = self.bulk_session.borrow().target(*parameter);
            if let Some(target) = target.as_ref() {
                let target = target.clone();
                let state = self.state.clone();
                let session = self.bulk_session.clone();
                let clipboard_session = self.clipboard_session.clone();
                field.update(cx, |field, _| {
                    field.sync_guarded(
                        target.binding_key(),
                        value.clone(),
                        w,
                        move |text, w, cx| {
                            state.update(cx, |state, cx| {
                                let display = submit_clipboard_field(
                                    &session,
                                    &clipboard_session,
                                    &target,
                                    state,
                                    text,
                                    |state, command| {
                                        state.dispatch(&Action::Edit(command), w, cx);
                                        state.status == "Edited"
                                    },
                                );
                                cx.notify();
                                display
                            })
                        },
                    );
                });
            }
            let mut row = div().flex().flex_col().gap_1().pb_1().child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_h(px(27.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(11.))
                            .child(parameter.label()),
                    )
                    .child(
                        div()
                            .w(px(110.))
                            .when(target.is_some(), |d| d.child(field.clone()))
                            .when(target.is_none(), |d| d.child(value)),
                    ),
            );
            if let Some(target) = &target
                && let Some(summary) = animation_state(contents, target)
            {
                row = row
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(ui::MUTED))
                            .child(summary.label()),
                    )
                    .child(self.shared_animation_buttons(target, summary, cx));
            }
            root = root.child(row);
        }
        root.child(
            div()
                .text_size(px(10.))
                .text_color(rgb(ui::MUTED))
                .child(HELP),
        )
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(ui::MUTED))
                .child(ANIMATION_HELP),
        )
    }
}

#[cfg(test)]
mod tests;
