//! Whole-gradient Colors lanes deliberately live outside scalar PropertyPath.
//! All controls reject pending fields before blur; retiming preserves complete keys.
#[path = "compound_colors_drag.rs"]
mod pointer_drag;
#[path = "compound_colors_marquee.rs"]
mod pointer_marquee;
#[path = "compound_colors_scale.rs"]
mod time_scale;
use crate::{
    color_edit::InputTarget,
    components::TextField,
    editor::{Action, EditorState, PropertyFilter},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, relative, rgb};
use libre_effects_core::{
    Command, CompositionId, Content, ContentsEdit, ContentsNode, Frame, GradientColorsAnimation,
    GradientColorsEdit, GradientColorsHoldReason, GradientColorsInterpolation,
    GradientColorsKeyCopy, Layer,
};
use pointer_drag::PointerDrag;
pub(crate) use pointer_drag::PointerGeometry;
use pointer_marquee::PointerMarquee;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

pub(crate) fn mode_label(mode: GradientColorsInterpolation) -> &'static str {
    match mode {
        GradientColorsInterpolation::Hold => "Hold",
        GradientColorsInterpolation::Linear => "Linear",
        GradientColorsInterpolation::Smooth => "Smoothstep",
    }
}

pub(crate) fn segment_label(animation: &GradientColorsAnimation, frame: Frame) -> String {
    let Some(status) = animation.segment_at(frame) else {
        return "Static".into();
    };
    match status.hold_reason {
        Some(GradientColorsHoldReason::IncompatibleTopology) => format!(
            "{} requested · Hold fallback: stop IDs/order differ ({} → {})",
            mode_label(status.interpolation),
            status.frame,
            status.next_frame.unwrap()
        ),
        Some(GradientColorsHoldReason::NoNextKey) => format!(
            "{} outgoing · last key holds (no next key)",
            mode_label(status.interpolation)
        ),
        Some(GradientColorsHoldReason::BeforeFirstKey) => {
            format!("Hold before first key ({})", status.frame)
        }
        None => format!(
            "{} · {} → {}",
            mode_label(status.effective),
            status.frame,
            status.next_frame.unwrap_or(status.frame)
        ),
    }
}

fn blocked(state: &EditorState) -> bool {
    state.playing
        || state.text_session.is_some()
        || state.colors.session.is_some()
        || state.gradient_editor.is_some()
        || state.vertex_editor.is_some()
        || state.expression_editor.is_some()
        || state.gradient_preview.is_some()
        || state.media_open
        || state.fonts_open
        || state.queue_open
        || state.recovery.is_some()
        || state.new_composition_requested
        || state.close_after_save
        || state.selected_layers.len() != 1
}

#[derive(Clone)]
struct Owner {
    input: InputTarget,
    generation: u64,
    continuity: u64,
    transport: u64,
    layer: u64,
    composition: CompositionId,
    frame: Frame,
    tool: crate::editor::Tool,
    contents_selection: Option<(CompositionId, u64, u64)>,
    gradient_controls: Option<crate::color_edit::GradientTarget>,
    selected_keys: BTreeSet<libre_effects_core::KeyRef>,
    expanded: bool,
    filter: Option<PropertyFilter>,
    graph_open: bool,
}
impl Owner {
    fn capture(state: &EditorState) -> Option<Self> {
        if blocked(state) {
            return None;
        }
        let layer = state.editor.selected_layer().filter(|l| !l.locked())?;
        if !matches!(layer.content(), Content::ShapeContents(_)) {
            return None;
        }
        Some(Self {
            input: InputTarget::new(state)?,
            generation: state.input_context_generation(),
            continuity: state.colors_clipboard_generation(),
            transport: state.transport_generation(),
            layer: layer.id(),
            composition: state.editor.project().active_composition_id(),
            frame: state.frame,
            tool: state.tool,
            contents_selection: state.contents_selection,
            gradient_controls: state.gradient_controls,
            selected_keys: state.selected_keys.clone(),
            expanded: state.expanded,
            filter: state.property_filter,
            graph_open: state.graph_open,
        })
    }
    fn same_domain(&self, state: &EditorState) -> bool {
        !blocked(state)
            && state.selected_layers == [self.layer].into()
            && self.tool == state.tool
            && self.contents_selection == state.contents_selection
            && self.gradient_controls == state.gradient_controls
            && self.selected_keys == state.selected_keys
            && self.expanded == state.expanded
            && self.filter == state.property_filter
            && self.graph_open == state.graph_open
    }
    fn current(&self, state: &EditorState) -> bool {
        self.same_domain(state)
            && self.input.current(state)
            && self.generation == state.input_context_generation()
            && self.continuity == state.colors_clipboard_generation()
            && self.transport == state.transport_generation()
    }
    fn key(&self) -> String {
        format!(
            "{}-{}-{}",
            self.input.binding(),
            self.generation,
            self.transport
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Selection {
    composition: CompositionId,
    layer: u64,
    item: u64,
    frames: BTreeSet<Frame>,
}
impl Selection {
    fn anchor(&self) -> Frame {
        *self.frames.first().expect("nonempty Colors selection")
    }
    fn owns(&self, target: &Target) -> bool {
        self.composition == target.owner.composition
            && self.layer == target.owner.layer
            && self.item == target.item
            && !self.frames.is_empty()
    }
}

/// No OS clipboard and no correspondence guesses: these snapshots can be pasted
/// only back into the exact, uninterrupted paint/source from which they came.
#[derive(Clone)]
struct Clipboard {
    owner: Owner,
    epoch: u64,
    ownership: Rc<Cell<bool>>,
    item: u64,
    keys: Vec<GradientColorsKeyCopy>,
}
impl Clipboard {
    fn current(&self, state: &EditorState) -> bool {
        self.epoch == state.colors_clipboard_generation()
            && Rc::ptr_eq(&self.ownership, &state.colors_key_owned)
            && state.colors_key_owned.get()
            && self.owner.same_domain(state)
            && self.owner.input.source_current(state)
    }
    fn refresh(&mut self, state: &EditorState) -> Option<()> {
        self.owner = Owner::capture(state)?;
        self.epoch = state.colors_clipboard_generation();
        self.ownership = state.colors_key_owned.clone();
        Some(())
    }
}

#[derive(Clone)]
pub(crate) struct Target {
    owner: Owner,
    item: u64,
    serial: u64,
    selection: Option<Selection>,
}
impl Target {
    fn key(&self) -> String {
        format!(
            "colors-timeline-{}-{}-{}-{:?}",
            self.owner.key(),
            self.item,
            self.serial,
            self.selection
        )
    }
    fn node<'a>(&self, state: &'a EditorState) -> Option<&'a ContentsNode> {
        let layer = state
            .editor
            .project()
            .composition()
            .layer(self.owner.layer)?;
        let Content::ShapeContents(contents) = layer.content() else {
            return None;
        };
        contents
            .node(self.item)
            .filter(|n| n.kind.gradient().is_some())
    }
    fn command(&self, state: &EditorState, control: Control) -> Option<Action> {
        let node = self.node(state)?;
        let gradient = node.kind.gradient()?;
        let animation = gradient.colors_animation();
        let frame = self.owner.frame;
        let selected = self.selection.as_ref().filter(|s| s.owns(self));
        if matches!(control, Control::Remove | Control::Mode(_))
            && !selected.is_some_and(|s| {
                animation.is_some_and(|a| s.frames.iter().all(|f| a.keys().contains_key(f)))
            })
        {
            return None;
        }
        let edit = match control {
            Control::Enable
                if animation.is_none() && gradient.colors_animation_compatible(node) =>
            {
                GradientColorsEdit::SetAnimation {
                    frame,
                    enabled: true,
                }
            }
            Control::Disable if animation.is_some() => GradientColorsEdit::SetAnimation {
                frame,
                enabled: false,
            },
            Control::Add if animation.is_some_and(|a| !a.keys().contains_key(&frame)) => {
                GradientColorsEdit::ToggleKey { frame }
            }
            Control::Remove => GradientColorsEdit::DeleteKeys {
                frames: selected?.frames.clone(),
                frame,
            },
            Control::Mode(interpolation) => GradientColorsEdit::SetInterpolations {
                frames: selected?.frames.clone(),
                interpolation,
            },
            Control::Select(key) if animation?.keys().contains_key(&key) => {
                return Some(Action::Seek(key));
            }
            Control::Previous => {
                return Some(Action::Seek(
                    *animation?.keys().range(..frame).next_back()?.0,
                ));
            }
            Control::Next => {
                return Some(Action::Seek(
                    *animation?
                        .keys()
                        .range((std::ops::Bound::Excluded(frame), std::ops::Bound::Unbounded))
                        .next()?
                        .0,
                ));
            }
            Control::Edit
                if state.contents_selection
                    == Some((self.owner.composition, self.owner.layer, self.item)) =>
            {
                return Some(Action::OpenGradient(self.item));
            }
            _ => return None,
        };
        Some(Action::Edit(Command::Contents {
            id: self.owner.layer,
            edit: ContentsEdit::GradientColors {
                item: self.item,
                edit,
            },
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Enable,
    Disable,
    Add,
    Remove,
    Copy,
    Paste,
    Previous,
    Next,
    Edit,
    Select(Frame),
    Mode(GradientColorsInterpolation),
}

#[derive(Default)]
pub(crate) struct Input {
    pointer: Option<PointerDrag>,
    marquee: Option<PointerMarquee>,
    ownership: Option<Rc<Cell<bool>>>,
    owner: Option<Owner>,
    selected: Option<Selection>,
    clipboard: Option<Clipboard>,
    domain_owned: bool,
    serial: u64,
}
impl Input {
    pub(crate) fn observe(&mut self, state: &EditorState) {
        if self
            .pointer
            .as_ref()
            .is_some_and(|p| !p.current(self, state, p.geometry))
        {
            self.cancel_pointer();
        }
        if self
            .marquee
            .as_ref()
            .is_some_and(|p| !p.current(self, state, p.geometry))
        {
            self.cancel_pointer();
        }
        if self.clipboard.as_ref().is_some_and(|c| !c.current(state)) {
            self.clipboard = None;
        }
        if self.domain_owned
            && (!state.colors_key_owned.get()
                || self
                    .ownership
                    .as_ref()
                    .is_some_and(|owner| !Rc::ptr_eq(owner, &state.colors_key_owned)))
        {
            self.select(None);
        }
        self.ownership = Some(state.colors_key_owned.clone());
        if self.owner.as_ref().is_some_and(|o| o.current(state)) {
            return;
        }
        if self.selected.is_some()
            && self.owner.as_ref().is_some_and(|owner| {
                owner.continuity != state.colors_clipboard_generation()
                    || !owner.same_domain(state)
                    || !owner.input.source_current(state)
            })
        {
            self.retire_selection();
        }
        self.owner = Owner::capture(state);
        // Seeks preserve the selected key set; no key is inferred from playhead.
        // Removed keys or a changed paint/domain retire the complete selection.
        if self.selected.as_ref().is_some_and(|selected| {
            !self
                .owner
                .as_ref()
                .is_some_and(|o| o.composition == selected.composition && o.layer == selected.layer)
                || !self.target(selected.item).is_some_and(|t| {
                    t.node(state)
                        .and_then(|n| n.kind.gradient())
                        .and_then(|g| g.colors_animation())
                        .is_some_and(|a| selected.frames.iter().all(|f| a.keys().contains_key(f)))
                })
        }) {
            self.retire_selection();
        }
    }
    /// Retired/deleted keys keep an opaque domain tombstone. A held Delete or
    /// global clipboard shortcut must never fall through to whole-layer edits.
    fn retire_selection(&mut self) {
        self.cancel_pointer();
        self.selected = None;
        self.clipboard = None;
        self.serial = self.serial.wrapping_add(1);
    }
    fn select(&mut self, selected: Option<Selection>) {
        self.cancel_pointer();
        if selected.as_ref().is_none_or(|next| {
            self.selected.as_ref().is_none_or(|old| {
                next.composition != old.composition
                    || next.layer != old.layer
                    || next.item != old.item
            })
        }) {
            self.clipboard = None;
        }
        self.selected = selected;
        self.domain_owned = self.selected.is_some();
        if let Some(ownership) = &self.ownership {
            ownership.set(self.selected.is_some());
        }
        self.serial = self.serial.wrapping_add(1);
    }
    pub(crate) fn target(&self, item: u64) -> Option<Target> {
        Some(Target {
            owner: self.owner.clone()?,
            item,
            serial: self.serial,
            selection: self.selected.clone(),
        })
    }
    fn current(&self, target: &Target, state: &EditorState) -> bool {
        state.colors_key_owned.get() == self.domain_owned
            && self
                .ownership
                .as_ref()
                .is_some_and(|o| Rc::ptr_eq(o, &state.colors_key_owned))
            && self.serial == target.serial
            && self.selected == target.selection
            && self
                .owner
                .as_ref()
                .is_some_and(|o| o.key() == target.owner.key())
            && target.owner.current(state)
            && target.node(state).is_some()
    }
    fn prepare(&self, target: &Target, state: &EditorState, pending: Option<String>) -> bool {
        pending.is_none() && !self.pointer_active() && self.current(target, state)
    }
    pub(crate) fn select_frames(
        &mut self,
        state: &EditorState,
        item: u64,
        frames: BTreeSet<Frame>,
    ) {
        self.ownership = Some(state.colors_key_owned.clone());
        self.owner = Owner::capture(state);
        let selected = self
            .owner
            .as_ref()
            .filter(|_| !frames.is_empty())
            .map(|o| Selection {
                composition: o.composition,
                layer: o.layer,
                item,
                frames,
            });
        self.select(selected);
    }
    #[cfg(test)]
    fn select_frame(&mut self, state: &EditorState, item: u64, frame: Frame) {
        self.select_frames(state, item, [frame].into());
    }
    fn select_key(&mut self, state: &EditorState, item: u64, frame: Frame, toggle: bool) {
        let mut frames = if toggle {
            self.selected
                .as_ref()
                .filter(|s| {
                    s.composition == state.editor.project().active_composition_id()
                        && Some(s.layer) == state.editor.selected()
                        && s.item == item
                })
                .map(|s| s.frames.clone())
                .unwrap_or_default()
        } else {
            BTreeSet::new()
        };
        if !toggle || !frames.remove(&frame) {
            frames.insert(frame);
        }
        self.select_frames(state, item, frames);
    }
    fn copy_selected(&mut self, target: &Target, state: &EditorState) -> bool {
        if !self.current(target, state) {
            return false;
        }
        let Some(selected) = target.selection.as_ref().filter(|s| s.owns(target)) else {
            return false;
        };
        let Some(animation) = target
            .node(state)
            .and_then(|n| n.kind.gradient())
            .and_then(|g| g.colors_animation())
        else {
            return false;
        };
        let keys = selected
            .frames
            .iter()
            .map(|&frame| {
                Some(GradientColorsKeyCopy {
                    offset: frame - selected.anchor(),
                    colors: animation.keys().get(&frame)?.clone(),
                    interpolation: animation.interpolation(frame)?,
                })
            })
            .collect::<Option<Vec<_>>>();
        let Some(keys) = keys else {
            return false;
        };
        self.clipboard = Some(Clipboard {
            owner: target.owner.clone(),
            epoch: state.colors_clipboard_generation(),
            ownership: state.colors_key_owned.clone(),
            item: target.item,
            keys,
        });
        self.serial = self.serial.wrapping_add(1);
        true
    }
    fn paste_command(&self, target: &Target, state: &EditorState) -> Option<Action> {
        let clipboard = self.clipboard.as_ref()?;
        (self.current(target, state)
            && clipboard.current(state)
            && clipboard.item == target.item
            && target.selection.as_ref().is_some_and(|s| s.owns(target)))
        .then_some(())?;
        target.node(state)?.kind.gradient()?.colors_animation()?;
        Some(Action::Edit(Command::Contents {
            id: target.owner.layer,
            edit: ContentsEdit::GradientColors {
                item: target.item,
                edit: GradientColorsEdit::PasteKeys {
                    keys: clipboard.keys.clone(),
                    frame: state.frame,
                },
            },
        }))
    }
    fn finish_paste(&mut self, state: &EditorState, mut clipboard: Clipboard, frame: Frame) {
        self.observe(state);
        let frames = clipboard
            .keys
            .iter()
            .map(|k| frame.checked_add(k.offset))
            .collect::<Option<BTreeSet<_>>>();
        if let Some(frames) = frames {
            self.select_frames(state, clipboard.item, frames);
            if self.selected.is_some() && clipboard.refresh(state).is_some() {
                self.clipboard = Some(clipboard);
            }
        }
    }
    fn finish_mode(&mut self, state: &EditorState, selection: Selection) {
        self.observe(state);
        self.select_frames(state, selection.item, selection.frames);
    }
    fn enabled(&self, target: &Target, state: &EditorState, control: Control) -> bool {
        match control {
            Control::Copy => {
                self.current(target, state)
                    && target.selection.as_ref().is_some_and(|s| s.owns(target))
            }
            Control::Paste => self.paste_command(target, state).is_some(),
            _ => target.command(state, control).is_some(),
        }
    }
}

fn modifiers_allowed(modifiers: gpui::Modifiers, control: Control) -> bool {
    !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
        && !modifiers.function
        && (!modifiers.shift || matches!(control, Control::Select(_)))
}

fn button(
    state: &Entity<EditorState>,
    input: &Rc<RefCell<Input>>,
    target: Option<Target>,
    control: Control,
    label: impl Into<SharedString>,
    active: bool,
    disabled_input: Option<InputTarget>,
    lane_identity: (u64, u64),
) -> gpui::Stateful<gpui::Div> {
    let name = format!(
        "{}-{control:?}",
        target
            .as_ref()
            .map(Target::key)
            .unwrap_or_else(|| format!("disabled-colors-{}-{}", lane_identity.0, lane_identity.1))
    );
    let widget = ui::text_button(SharedString::from(name.clone()), label)
        .text_size(px(10.))
        .when(active, |d| d.text_color(rgb(ui::BLUE)))
        // The workspace capture guard already validated the press. Do not let
        // Timeline's ancestor start a scalar marquee behind this dedicated key.
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
    let Some(target) = target else {
        return crate::color_edit::input_pointer_button_guarded(
            widget.opacity(0.4),
            name,
            disabled_input,
            |_, _, _| false,
        );
    };
    let state = state.clone();
    let guard_target = target.clone();
    let guard_input = input.clone();
    let input = input.clone();
    let source = Some(target.owner.input.clone());
    let guard = move |state: &EditorState, cx: &gpui::App, _| {
        guard_input
            .borrow()
            .prepare(&guard_target, state, TextField::active_pending_binding(cx))
    };
    let widget = if matches!(control, Control::Select(_)) {
        crate::color_edit::input_pointer_key_button_guarded(
            widget,
            name.clone(),
            source.clone(),
            guard,
        )
    } else {
        crate::color_edit::input_pointer_button_guarded(widget, name.clone(), source.clone(), guard)
    };
    widget.on_click(move |event, window, cx| {
        cx.stop_propagation();
        if !modifiers_allowed(event.modifiers(), control)
            || matches!(event, gpui::ClickEvent::Mouse(click) if !modifiers_allowed(click.down.modifiers, control)
                || click.down.modifiers != click.up.modifiers)
            || TextField::is_composing(window, cx)
            || (matches!(event, gpui::ClickEvent::Keyboard(_)) && TextField::active_has_focus(window, cx))
            || !input.borrow().prepare(&target, state.read(cx), TextField::active_pending_binding(cx))
            || crate::color_edit::input_click_target(&name, event, &source, &state, window, cx).is_none()
        { return; }
        state.update(cx, |state, cx| {
            if !input.borrow().current(&target, state) { return; }
            if control == Control::Copy {
                if input.borrow_mut().copy_selected(&target, state) {
                    state.status = "Copied Colors keys; seek, then Paste at playhead on this paint".into();
                    cx.notify();
                }
                return;
            }
            let action = if control == Control::Paste { input.borrow().paste_command(&target, state) }
                else { target.command(state, control) };
            let Some(action) = action else { return; };
            let pasted = (control == Control::Paste).then(|| input.borrow().clipboard.clone()).flatten();
            let paste_frame = state.frame;
            let mode_selection = matches!(control, Control::Mode(_)).then(|| target.selection.clone()).flatten();
            let select = match action { Action::Seek(frame) => Some(frame), _ => None };
            // Colors selection is separate from scalar/Graph selection.
            state.selected_keys.clear();
            state.graph_key = None;
            state.dispatch(&action, window, cx);
            let mut input = input.borrow_mut();
            if let Some(frame) = select {
                input.select_key(state, target.item, frame, matches!(control, Control::Select(_)) && event.modifiers().shift);
            } else if let Some(clipboard) = pasted.filter(|_| state.status == "Edited") {
                input.finish_paste(state, clipboard, paste_frame);
            } else if let Some(selection) = mode_selection.filter(|_| state.status == "Edited") {
                input.finish_mode(state, selection);
            } else { input.observe(state); }
            cx.notify();
        });
    })
}

fn move_command(
    target: &Target,
    state: &EditorState,
    text: &str,
) -> Result<Option<(Command, Frame)>, String> {
    let selected = target
        .selection
        .as_ref()
        .filter(|s| s.owns(target))
        .ok_or("Select Colors keys first")?;
    let animation = target
        .node(state)
        .and_then(|n| n.kind.gradient())
        .and_then(|g| g.colors_animation())
        .ok_or("Colors keys no longer exist")?;
    if !selected
        .frames
        .iter()
        .all(|f| animation.keys().contains_key(f))
    {
        return Err("Colors keys no longer exist".into());
    }
    let to = text
        .trim()
        .parse::<Frame>()
        .map_err(|_| "Enter a whole frame number")?;
    let duration = state.editor.project().composition().duration();
    let destinations = selected
        .frames
        .iter()
        .map(|f| to.checked_add(f - selected.anchor()))
        .collect::<Option<BTreeSet<_>>>()
        .ok_or("Frame must be inside the composition")?;
    if destinations.iter().any(|f| *f >= duration) {
        return Err("Frame must be inside the composition".into());
    }
    if to == selected.anchor() {
        return Ok(None);
    }
    if destinations
        .iter()
        .any(|f| animation.keys().contains_key(f) && !selected.frames.contains(f))
    {
        return Err("An unselected Colors key already exists at a destination frame".into());
    }
    Ok(Some((
        Command::Contents {
            id: selected.layer,
            edit: ContentsEdit::GradientColors {
                item: selected.item,
                edit: GradientColorsEdit::MoveKeys {
                    frames: selected.frames.clone(),
                    to,
                },
            },
        },
        to,
    )))
}

fn submit_move(
    input: &Rc<RefCell<Input>>,
    target: &Target,
    state: &mut EditorState,
    text: &str,
    active: bool,
    apply: impl FnOnce(&mut EditorState, Command, Frame) -> bool,
) -> String {
    let anchor = target
        .selection
        .as_ref()
        .map_or(target.owner.frame, Selection::anchor);
    if !active || !input.borrow().current(target, state) {
        state.status = "Colors key editing context changed; frame was not applied".into();
        return anchor.to_string();
    }
    match move_command(target, state, text) {
        Ok(None) => {}
        Ok(Some((command, frame))) => {
            if apply(state, command, frame) {
                let frames = target
                    .selection
                    .as_ref()
                    .unwrap()
                    .frames
                    .iter()
                    .map(|f| frame + (f - anchor))
                    .collect();
                let mut input = input.borrow_mut();
                input.observe(state);
                input.select_frames(state, target.item, frames);
                return frame.to_string();
            }
        }
        Err(error) => state.status = error,
    }
    anchor.to_string()
}

fn visible_nodes(layer: &Layer, filter: Option<PropertyFilter>) -> Vec<&ContentsNode> {
    let Content::ShapeContents(contents) = layer.content() else {
        return vec![];
    };
    contents
        .rows()
        .into_iter()
        .filter_map(|(_, _, node)| {
            let gradient = node.kind.gradient()?;
            (filter.is_none()
                || filter == Some(PropertyFilter::Animated)
                    && gradient.colors_animation().is_some())
            .then_some(node)
        })
        .collect()
}

/// Text and wrapped controls must use the viewport width, never contribute an
/// intrinsic minimum that stretches every Colors lane away from the ruler.
fn bounded_row() -> gpui::Div {
    div().w_full().max_w_full().min_w_0()
}

fn note(text: impl Into<SharedString>) -> gpui::Div {
    bounded_row()
        .pl(px(24.))
        .pr_2()
        .text_size(px(10.))
        .text_color(rgb(ui::MUTED))
        .whitespace_normal()
        .overflow_x_hidden()
        .child(text.into())
}

#[derive(Default)]
pub(super) struct TimelineColors {
    input: Rc<RefCell<Input>>,
    move_field: Option<Entity<TextField>>,
    scale_field: Option<Entity<TextField>>,
    lane_bounds: BTreeMap<(u64, u64), Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>>,
    pointer_generation: Option<u64>,
    pressed_bounds: Option<gpui::Bounds<gpui::Pixels>>,
}
impl TimelineColors {
    pub(super) fn observe(&self, state: &EditorState) {
        self.input.borrow_mut().observe(state);
    }
    pub(super) fn clear_selection(&self) {
        self.input.borrow_mut().select(None);
    }
    /// A Timeline-focused Delete must never fall through to layer deletion
    /// while a dedicated Colors key owns selection, even if that receipt retired.
    pub(super) fn key_down(
        &self,
        event: &gpui::KeyDownEvent,
        state: &Entity<EditorState>,
        window: &mut Window,
        cx: &mut Context<super::Timeline>,
    ) -> bool {
        if !self.input.borrow().domain_owned {
            return false;
        }
        if event.keystroke.key == "escape" && self.input.borrow_mut().cancel_pointer() {
            cx.notify();
            return true;
        }
        if event.keystroke.key == "escape" {
            self.clear_selection();
            cx.notify();
            return false;
        }
        if self.input.borrow().pointer_active() && event.keystroke.key != "alt" {
            self.input.borrow_mut().cancel_pointer();
            cx.notify();
        }
        // Explicit compound buttons own clipboard and group operations. Do not
        // let Timeline shortcuts interpret the opaque selection as layer keys.
        if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
            && matches!(event.keystroke.key.as_str(), "a" | "c" | "x" | "v" | "d")
        {
            return true;
        }
        if event.keystroke.key != "delete" {
            return false;
        }
        if event.is_held
            || event.keystroke.modifiers.modified()
            || TextField::is_composing(window, cx)
            || TextField::active_has_focus(window, cx)
            || TextField::active_pending_binding(cx).is_some()
        {
            return true;
        }
        let Some(selected) = self.input.borrow().selected.clone() else {
            return true;
        };
        let Some(target) = self.input.borrow().target(selected.item) else {
            return true;
        };
        if !self.input.borrow().current(&target, state.read(cx)) {
            return true;
        }
        if let Some(action) = target.command(state.read(cx), Control::Remove) {
            state.update(cx, |state, cx| {
                state.dispatch(&action, window, cx);
                self.input.borrow_mut().observe(state);
            });
        }
        true
    }
    pub(super) fn render_rows(
        &mut self,
        state: &Entity<EditorState>,
        layer: &Layer,
        left: f32,
        start: Frame,
        visible: Frame,
        frame: Frame,
        filter: Option<PropertyFilter>,
        graph_open: bool,
        fallback_input: Option<InputTarget>,
        return_focus: gpui::FocusHandle,
        window: &mut Window,
        cx: &mut Context<super::Timeline>,
    ) -> gpui::Div {
        let mut rows = bounded_row().flex().flex_col();
        let disabled = self
            .input
            .borrow()
            .owner
            .as_ref()
            .map(|o| o.input.clone())
            .or(fallback_input);
        for node in visible_nodes(layer, filter) {
            let gradient = node.kind.gradient().unwrap();
            let animation = gradient.colors_animation();
            let target = self
                .input
                .borrow()
                .target(node.id)
                .filter(|t| t.owner.layer == layer.id());
            let enabled = [
                Control::Enable,
                Control::Disable,
                Control::Add,
                Control::Remove,
                Control::Copy,
                Control::Paste,
                Control::Previous,
                Control::Next,
                Control::Edit,
                Control::Mode(GradientColorsInterpolation::Hold),
                Control::Mode(GradientColorsInterpolation::Linear),
                Control::Mode(GradientColorsInterpolation::Smooth),
            ]
            .into_iter()
            .filter(|kind| {
                target
                    .as_ref()
                    .is_some_and(|t| self.input.borrow().enabled(t, state.read(cx), *kind))
            })
            .collect::<Vec<_>>();
            let control = |kind, label, active| {
                let valid = target
                    .clone()
                    .filter(|_| matches!(kind, Control::Select(_)) || enabled.contains(&kind));
                button(
                    state,
                    &self.input,
                    valid,
                    kind,
                    label,
                    active,
                    disabled.clone(),
                    (layer.id(), node.id),
                )
            };
            let selected = self
                .input
                .borrow()
                .selected
                .clone()
                .filter(|s| s.layer == layer.id() && s.item == node.id);
            let mut controls = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .w(px(left))
                .flex_none()
                .pl(px(24.))
                .child(
                    div()
                        .text_size(px(11.))
                        .child(format!("{} · Colors", node.name)),
                )
                .child(control(
                    if animation.is_some() {
                        Control::Disable
                    } else {
                        Control::Enable
                    },
                    if animation.is_some() {
                        "Disable (clear keys)"
                    } else {
                        "Animate (Hold)"
                    },
                    animation.is_some(),
                ));
            if animation.is_some() {
                controls = controls
                    .child(control(Control::Previous, "‹", false))
                    .child(control(Control::Next, "›", false))
                    .child(control(Control::Add, "Add at playhead", false))
                    .child(
                        control(Control::Edit, "Edit Colors…", false).tooltip(|_, cx| {
                            cx.new(|_| {
                                ui::Tip("Select this paint in Contents to edit its stops".into())
                            })
                            .into()
                        }),
                    );
            }
            let lane_bounds = self
                .lane_bounds
                .entry((layer.id(), node.id))
                .or_default()
                .clone();
            let measure = lane_bounds.clone();
            let input = self.input.clone();
            let lane_item = node.id;
            let layout_owner = cx.entity().entity_id();
            let lane = div()
                .relative()
                .flex_1()
                .min_w_0()
                .min_h(px(29.))
                .overflow_hidden()
                .child(super::grid(start, visible, frame))
                .child(
                    gpui::canvas(
                        move |bounds, _, cx| {
                            measure.set(Some(bounds));
                            let mut input = input.borrow_mut();
                            if input.pointer.as_ref().is_some_and(|p| {
                                p.target.item == lane_item && p.geometry.bounds != bounds
                            }) || input.marquee.as_ref().is_some_and(|p| {
                                p.target.item == lane_item && p.geometry.bounds != bounds
                            }) {
                                input.cancel_pointer();
                                cx.notify(layout_owner);
                            }
                        },
                        |_, _, _, _| (),
                    )
                    .absolute()
                    .size_full(),
                );
            let mut lane = self.marquee_lane(
                lane,
                target.clone().filter(|_| animation.is_some()),
                state,
                lane_bounds.clone(),
                disabled.clone(),
                (layer.id(), node.id),
                cx,
            );
            let marquee_preview = self.input.borrow().marquee_preview(node.id, state.read(cx));
            let preview = self
                .input
                .borrow()
                .pointer
                .as_ref()
                .filter(|p| p.target.item == node.id && p.crossed && !p.shift)
                .map(|p| (p.delta, p.invalid, p.snapped));
            if let Some(animation) = animation {
                for (&key_frame, _) in animation.keys() {
                    let active = marquee_preview.as_ref().map_or_else(
                        || {
                            selected
                                .as_ref()
                                .is_some_and(|s| s.frames.contains(&key_frame))
                        },
                        |frames| frames.contains(&key_frame),
                    );
                    let label = match animation.interpolation(key_frame).unwrap() {
                        GradientColorsInterpolation::Hold => "■",
                        GradientColorsInterpolation::Linear => "◆",
                        GradientColorsInterpolation::Smooth => "●",
                    };
                    let tooltip = format!(
                        "Colors key {key_frame} · {} · drag selected keys; Shift-click toggles; Alt bypasses snapping",
                        segment_label(animation, key_frame)
                    );
                    if key_frame >= start && key_frame <= start.saturating_add(visible) {
                        lane = lane.child(
                            self.key_button(
                                target.clone(),
                                key_frame,
                                label,
                                active,
                                state,
                                lane_bounds.clone(),
                                disabled.clone(),
                                (layer.id(), node.id),
                                cx,
                            )
                            .absolute()
                            .left(relative((key_frame - start) as f32 / visible.max(1) as f32))
                            .ml(px(-7.))
                            .top(px(2.))
                            .w(px(15.))
                            .px_0()
                            .when(active && preview.is_some(), |d| d.opacity(0.35))
                            .tooltip(move |_, cx| {
                                cx.new(|_| ui::Tip(tooltip.clone().into())).into()
                            }),
                        );
                    }
                    if active && let Some((delta, invalid, _)) = preview {
                        let destination = (i64::from(key_frame) + delta) as Frame;
                        if destination >= start && destination <= start.saturating_add(visible) {
                            lane = lane.child(
                                div()
                                    .absolute()
                                    .left(relative(
                                        (destination - start) as f32 / visible.max(1) as f32,
                                    ))
                                    .ml(px(-7.))
                                    .top(px(2.))
                                    .w(px(15.))
                                    .text_center()
                                    .text_size(px(10.))
                                    .text_color(rgb(if invalid { 0xee7777 } else { ui::BLUE }))
                                    .child(label),
                            );
                        }
                    }
                }
                if let Some((_, _, Some(at))) = preview
                    && at >= start
                    && at <= start.saturating_add(visible)
                {
                    lane = lane.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(relative((at - start) as f32 / visible.max(1) as f32))
                            .w(px(1.))
                            .bg(rgb(0xffc75f)),
                    );
                }
            }
            let lane = self.marquee_overlay(lane, node.id);
            rows = rows.child(
                bounded_row()
                    .flex()
                    .min_h(px(29.))
                    .flex_none()
                    .child(controls)
                    .when(!graph_open, |d| d.child(lane)),
            );
            let summary = animation.map(|a| format!("{} keys · {}", a.keys().len(), segment_label(a, frame)))
                .unwrap_or_else(|| if gradient.colors_animation_compatible(node) { "Static Colors".into() }
                    else { "Scalar stop animation preserved; disable stop channels before enabling Colors".into() });
            rows = rows.child(note(summary));
            let detail_selection = self
                .input
                .borrow()
                .pointer
                .as_ref()
                .map(|p| p.previous_selection.clone())
                .unwrap_or(selected)
                .filter(|s| s.layer == layer.id() && s.item == node.id);
            if let (Some(selected), Some(target), Some(animation)) =
                (detail_selection, target.clone(), animation)
            {
                if !selected
                    .frames
                    .iter()
                    .all(|f| animation.keys().contains_key(f))
                {
                    continue;
                }
                let scale_control = time_scale::scale_control(
                    &mut self.scale_field,
                    &self.input,
                    &target,
                    &selected,
                    state,
                    return_focus.clone(),
                    window,
                    cx,
                );
                let input = self.input.clone();
                let edit_state = state.clone();
                let field = self
                    .move_field
                    .get_or_insert_with(|| {
                        cx.new(|cx| {
                            TextField::new(cx, |_, _, _| {}).return_focus(return_focus.clone())
                        })
                    })
                    .clone();
                field.update(cx, |field, _| {
                    field.sync_guarded(
                        format!("{}-move", target.key()),
                        selected.anchor().to_string(),
                        window,
                        move |text, window, cx| {
                            edit_state.update(cx, |state, cx| {
                                let display = submit_move(
                                    &input,
                                    &target,
                                    state,
                                    text,
                                    window.is_window_active(),
                                    |state, command, frame| {
                                        state.dispatch(&Action::Edit(command), window, cx);
                                        if state.status != "Edited" {
                                            return false;
                                        }
                                        state.dispatch(&Action::Seek(frame), window, cx);
                                        true
                                    },
                                );
                                cx.notify();
                                display
                            })
                        },
                    );
                });
                let mut detail = bounded_row()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .pl(px(24.))
                    .text_size(px(10.))
                    .child(format!(
                        "{} selected · Earliest {} → frame",
                        selected.frames.len(),
                        selected.anchor()
                    ))
                    .child(div().w(px(80.)).child(field))
                    .when_some(scale_control, |d, scale| d.child(scale))
                    .child(control(Control::Remove, "Delete selected", false))
                    .child(control(Control::Copy, "Copy selected", false))
                    .child(control(Control::Paste, "Paste at playhead", false));
                for mode in [
                    GradientColorsInterpolation::Hold,
                    GradientColorsInterpolation::Linear,
                    GradientColorsInterpolation::Smooth,
                ] {
                    detail = detail.child(control(
                        Control::Mode(mode),
                        mode_label(mode),
                        selected
                            .frames
                            .iter()
                            .all(|f| animation.interpolation(*f) == Some(mode)),
                    ));
                }
                rows = rows.child(detail).child(note(
                    "Drag moves selected keys together; Alt bypasses snapping. Red ghosts reject collisions. Move anchors the earliest selected key. Scale keeps the earliest fixed and fits the last to the entered end frame; rounded collisions reject. Complete snapshots and outgoing modes stay together. Copy/Paste works only on this unchanged paint. Copy lasts through seeks and its own pastes; other commands or paint changes clear it. Finish pending input with Enter before using buttons.",
                ));
            }
        }
        rows
    }
}

#[cfg(test)]
#[path = "compound_colors_tests.rs"]
mod tests;
