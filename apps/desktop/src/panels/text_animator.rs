//! Ordered source-unit selectors, with source-bound typed and discrete controls.
use crate::color_edit::InputTarget;
use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{
    Command, Content, Frame, Layer, MAX_TEXT_ANIMATORS, MAX_TEXT_RANGE_SELECTORS, PropertyPath,
    TextParam, TextRangeSelector, TextSelectorMode, TextSelectorParam, TextSelectorShape,
    TextSelectorUnits, TrackEdit,
};
use std::{cell::RefCell, rc::Rc};

pub(super) const ANIMATOR_PARAMETERS: [TextParam; 10] = [
    TextParam::AnimatorStart,
    TextParam::AnimatorEnd,
    TextParam::AnimatorPositionX,
    TextParam::AnimatorPositionY,
    TextParam::AnimatorOpacity,
    TextParam::AnimatorOffset,
    TextParam::AnimatorAmount,
    TextParam::AnimatorScaleX,
    TextParam::AnimatorScaleY,
    TextParam::AnimatorRotation,
];
const ANIMATOR_FIELDS: usize = ANIMATOR_PARAMETERS.len();
const ANIMATOR_LABELS: [&str; ANIMATOR_FIELDS] = [
    "Range Start (%)",
    "Range End (%)",
    "Position X (px)",
    "Position Y (px)",
    "Opacity (%)",
    "Range Offset (%)",
    "Amount (%)",
    "Scale X (%)",
    "Scale Y (%)",
    "Rotation (°)",
];
// Retain stable field bindings while grouping range and transform controls.
const ANIMATOR_DISPLAY_ORDER: [usize; ANIMATOR_FIELDS] = [0, 1, 5, 6, 7, 8, 9, 2, 3, 4];
const RANGE_FIELDS: [usize; 4] = [0, 1, 5, 6];
const ANIMATOR_STACK_HELP: &str = "Primary stays first. Each range supports independent animation of Start, End, Offset and Amount. Process top to bottom: Add = clamp(acc + weight), Subtract = clamp(acc − weight), Intersect = acc × weight; clamp to 0–1 after each step. The shared transform uses the final influence. Primary Units always define scale/rotation groups and pivots, even when secondary Units differ.";
const MULTIPLE_ANIMATORS_HELP: &str = "Primary stays first and keeps its ordered animated secondary selectors and shared transform. Add up to 3 independent animators after Primary; each extra animator has a single range with independent Units, Shape and all ten animated channels. Process animators top to bottom, each Scale → Rotation → Position around its original source-unit pivot. Later transforms also act on earlier Position; opacity multiplies. Extras do not have secondary selectors.";
const ANIMATOR_STALE: &str = "Text Animator editing context changed; value was not applied";
const ANIMATOR_HELP: &str = "Characters are extended Unicode graphemes, including spaces/newlines. Words use Unicode word boundaries and exclude separators, punctuation-only runs and emoji. Lines are hard LF/CRLF source lines, including empty/trailing lines, never visual wrapping. Units and Shape are static. Unit centers inside [Start + Offset, End + Offset) are selected; clip to 0–100 without wrapping. Offset is ±100 percentage points; Start ≥ End selects none. Across the clipped range, Square is 1, Ramp Up 0→1, Ramp Down 1→0, Triangle 0→1→0. Amount scales this selector’s weight; 0 contributes zero (Intersect clears accumulated influence). Connected shaping units stay together.";
const ANIMATOR_TRANSFORM_HELP: &str = "Primary Units: Characters transform protected shaping/grapheme clusters; Words and Lines transform whole source units. Each pivot is the first logical rendered glyph’s baseline origin, shared across automatic wrapping. Weighted scale is 1 + influence × (Scale / 100 − 1); weighted rotation is influence × Rotation. Apply Scale, then Rotation, then weighted Position after layout and before clipping and the layer transform. No reflow or caret movement. At full influence, Scale 0 collapses ink; negative scale is rejected, with no reflection.";
const ANIMATOR_ANIMATION_HELP: &str = "Type precise values. Disable animation deletes all keys for that property and keeps its current value. Remove key deletes only the current key; removing the last key keeps its value.";

fn animator_values(layer: &Layer, frame: Frame) -> [f64; ANIMATOR_FIELDS] {
    ANIMATOR_PARAMETERS.map(|parameter| layer.text_value_at(parameter, frame).unwrap())
}

fn selector_values(layer: &Layer, frame: Frame, selector: Option<u64>) -> [f64; ANIMATOR_FIELDS] {
    let mut values = animator_values(layer, frame);
    if let Some(selector) = selector.and_then(|id| range_selector(layer, id)) {
        for index in RANGE_FIELDS {
            values[index] = selector.value_at(selector_parameter(index).unwrap(), frame);
        }
    }
    values
}

fn extra_animator(layer: &Layer, id: u64) -> Option<&libre_effects_core::TextAnimator> {
    layer
        .text_animators()
        .iter()
        .find(|animator| animator.id == id)
}

fn scoped_values(
    layer: &Layer,
    frame: Frame,
    animator: Option<u64>,
    selector: Option<u64>,
) -> [f64; ANIMATOR_FIELDS] {
    if let Some(animator) = animator.and_then(|id| extra_animator(layer, id)) {
        ANIMATOR_PARAMETERS.map(|parameter| animator.value_at(parameter, frame).unwrap())
    } else {
        selector_values(layer, frame, selector)
    }
}

fn scoped_property(animator: Option<u64>, selector: Option<u64>, index: usize) -> PropertyPath {
    animator.map_or_else(
        || animator_property(selector, index),
        |animator| PropertyPath::TextAnimator {
            animator,
            parameter: ANIMATOR_PARAMETERS[index],
        },
    )
}

fn selector_parameter(index: usize) -> Option<TextSelectorParam> {
    match index {
        0 => Some(TextSelectorParam::Start),
        1 => Some(TextSelectorParam::End),
        5 => Some(TextSelectorParam::Offset),
        6 => Some(TextSelectorParam::Amount),
        _ => None,
    }
}

fn animator_property(selector: Option<u64>, index: usize) -> PropertyPath {
    match (selector, selector_parameter(index)) {
        (Some(selector), Some(parameter)) => PropertyPath::TextSelector {
            selector,
            parameter,
        },
        _ => PropertyPath::Text(ANIMATOR_PARAMETERS[index]),
    }
}

fn range_selector(layer: &Layer, id: u64) -> Option<&TextRangeSelector> {
    layer
        .text_range_selectors()
        .iter()
        .find(|selector| selector.id == id)
}

fn animator_field_command(
    layer: &Layer,
    frame: Frame,
    index: usize,
    text: &str,
) -> Result<Option<Command>, String> {
    selector_field_command(layer, frame, None, index, text)
}

fn selector_field_command(
    layer: &Layer,
    frame: Frame,
    selector: Option<u64>,
    index: usize,
    text: &str,
) -> Result<Option<Command>, String> {
    scoped_field_command(layer, frame, None, selector, index, text)
}

fn scoped_field_command(
    layer: &Layer,
    frame: Frame,
    animator: Option<u64>,
    selector: Option<u64>,
    index: usize,
    text: &str,
) -> Result<Option<Command>, String> {
    if layer.locked() || !matches!(layer.content(), Content::Text { .. }) {
        return Err("Select unlocked text".into());
    }
    let parameter = *ANIMATOR_PARAMETERS
        .get(index)
        .ok_or("Unknown Text Animator field")?;
    let value = text
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or("Enter a finite number")?;
    let (min, max) = parameter.bounds();
    if !(min..=max).contains(&value) {
        return Err(format!(
            "{} must be from {min} to {max}",
            ANIMATOR_LABELS[index]
        ));
    }
    if let Some(id) = animator {
        if selector.is_some() {
            return Err("Extra animators have a single range".into());
        }
        extra_animator(layer, id).ok_or("Text animator no longer exists")?;
        return layer.text_animator_value_command(id, parameter, value, frame);
    }
    if let Some(id) = selector {
        range_selector(layer, id).ok_or("Text selector no longer exists")?;
        if let Some(parameter) = selector_parameter(index) {
            return layer.text_selector_value_command(id, parameter, value, frame);
        }
    }
    layer.text_value_command(parameter, value, frame)
}

/// Explicit intent remains stable when a pending value creates a current key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnimatorEdit {
    Enable,
    Disable,
    AddKey,
    RemoveKey,
}

fn animator_command(
    layer: &Layer,
    parameter: TextParam,
    frame: Frame,
    edit: AnimatorEdit,
) -> Result<Option<Command>, String> {
    if layer.locked() || !matches!(layer.content(), Content::Text { .. }) {
        return Err("Select unlocked text".into());
    }
    if !ANIMATOR_PARAMETERS.contains(&parameter) {
        return Err("Unknown Text Animator field".into());
    }
    let track = layer.track(PropertyPath::Text(parameter));
    let animated = track.is_some_and(|track| !track.keys().is_empty());
    let keyed = track.is_some_and(|track| track.keys().contains_key(&frame));
    let track_edit = match edit {
        AnimatorEdit::Enable if !animated => TrackEdit::ToggleAnimation { frame },
        AnimatorEdit::Disable if animated => TrackEdit::ToggleAnimation { frame },
        AnimatorEdit::AddKey if !keyed => TrackEdit::ToggleKey { frame },
        AnimatorEdit::RemoveKey if keyed => TrackEdit::ToggleKey { frame },
        _ => return Ok(None),
    };
    Ok(Some(Command::EditText {
        id: layer.id(),
        parameter,
        edit: track_edit,
    }))
}

fn selector_animation_command(
    layer: &Layer,
    selector: u64,
    parameter: TextSelectorParam,
    frame: Frame,
    edit: AnimatorEdit,
) -> Result<Option<Command>, String> {
    if layer.locked() || !matches!(layer.content(), Content::Text { .. }) {
        return Err("Select unlocked text".into());
    }
    range_selector(layer, selector).ok_or("Text selector no longer exists")?;
    let property = PropertyPath::TextSelector {
        selector,
        parameter,
    };
    let track = layer.track(property);
    let animated = track.is_some_and(|track| !track.keys().is_empty());
    let keyed = track.is_some_and(|track| track.keys().contains_key(&frame));
    let edit = match edit {
        AnimatorEdit::Enable if !animated => TrackEdit::ToggleAnimation { frame },
        AnimatorEdit::Disable if animated => TrackEdit::ToggleAnimation { frame },
        AnimatorEdit::AddKey if !keyed => TrackEdit::ToggleKey { frame },
        AnimatorEdit::RemoveKey if keyed => TrackEdit::ToggleKey { frame },
        _ => return Ok(None),
    };
    Ok(Some(Command::EditTrack {
        id: layer.id(),
        property,
        edit,
    }))
}

fn extra_animation_command(
    layer: &Layer,
    animator: u64,
    parameter: TextParam,
    frame: Frame,
    edit: AnimatorEdit,
) -> Result<Option<Command>, String> {
    if layer.locked() || !matches!(layer.content(), Content::Text { .. }) {
        return Err("Select unlocked text".into());
    }
    extra_animator(layer, animator).ok_or("Text animator no longer exists")?;
    if !ANIMATOR_PARAMETERS.contains(&parameter) {
        return Err("Unknown Text Animator field".into());
    }
    let property = PropertyPath::TextAnimator {
        animator,
        parameter,
    };
    let track = layer.track(property);
    let animated = track.is_some_and(|track| !track.keys().is_empty());
    let keyed = track.is_some_and(|track| track.keys().contains_key(&frame));
    let edit = match edit {
        AnimatorEdit::Enable if !animated => TrackEdit::ToggleAnimation { frame },
        AnimatorEdit::Disable if animated => TrackEdit::ToggleAnimation { frame },
        AnimatorEdit::AddKey if !keyed => TrackEdit::ToggleKey { frame },
        AnimatorEdit::RemoveKey if keyed => TrackEdit::ToggleKey { frame },
        _ => return Ok(None),
    };
    Ok(Some(Command::EditTrack {
        id: layer.id(),
        property,
        edit,
    }))
}

/// Frozen, explicit intent; selector changes preserve the other live setting
/// after an authorized numeric-field flush and never implicitly toggle values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnimatorAction {
    ExtraAnimation {
        animator: u64,
        parameter: TextParam,
        edit: AnimatorEdit,
    },
    SelectAnimator(Option<u64>),
    AddAnimator,
    RemoveAnimator(u64),
    MoveAnimator {
        animator: u64,
        index: usize,
    },
    AnimatorUnits {
        animator: u64,
        units: TextSelectorUnits,
    },
    AnimatorShape {
        animator: u64,
        shape: TextSelectorShape,
    },
    Animation {
        parameter: TextParam,
        edit: AnimatorEdit,
    },
    SelectorAnimation {
        selector: u64,
        parameter: TextSelectorParam,
        edit: AnimatorEdit,
    },
    Units(TextSelectorUnits),
    Shape(TextSelectorShape),
    SelectSelector(Option<u64>),
    AddSelector,
    RemoveSelector(u64),
    MoveSelector {
        selector: u64,
        index: usize,
    },
    SelectorUnits {
        selector: u64,
        units: TextSelectorUnits,
    },
    SelectorShape {
        selector: u64,
        shape: TextSelectorShape,
    },
    SelectorMode {
        selector: u64,
        mode: TextSelectorMode,
    },
}

fn animator_action_command(
    layer: &Layer,
    frame: Frame,
    action: AnimatorAction,
) -> Result<Option<Command>, String> {
    if layer.locked() || !matches!(layer.content(), Content::Text { .. }) {
        return Err("Select unlocked text".into());
    }
    let mut selector = layer.text_selector();
    match action {
        AnimatorAction::ExtraAnimation {
            animator,
            parameter,
            edit,
        } => {
            return extra_animation_command(layer, animator, parameter, frame, edit);
        }
        AnimatorAction::SelectAnimator(_) => return Err("Selection is a panel action".into()),
        AnimatorAction::AddAnimator => {
            return Ok((layer.text_animators().len() < MAX_TEXT_ANIMATORS)
                .then_some(Command::AddTextAnimator { id: layer.id() }));
        }
        AnimatorAction::RemoveAnimator(animator) => {
            extra_animator(layer, animator).ok_or("Text animator no longer exists")?;
            return Ok(Some(Command::RemoveTextAnimator {
                id: layer.id(),
                animator,
            }));
        }
        AnimatorAction::MoveAnimator { animator, index } => {
            let animators = layer.text_animators();
            let current = animators
                .iter()
                .position(|item| item.id == animator)
                .ok_or("Text animator no longer exists")?;
            if index >= animators.len() {
                return Err("Text animator position is out of range".into());
            }
            return Ok((current != index).then_some(Command::MoveTextAnimator {
                id: layer.id(),
                animator,
                index,
            }));
        }
        AnimatorAction::AnimatorUnits { animator, .. }
        | AnimatorAction::AnimatorShape { animator, .. } => {
            let original =
                extra_animator(layer, animator).ok_or("Text animator no longer exists")?;
            let mut selector = original.selector;
            match action {
                AnimatorAction::AnimatorUnits { units, .. } => selector.units = units,
                AnimatorAction::AnimatorShape { shape, .. } => selector.shape = shape,
                _ => unreachable!(),
            }
            return Ok((selector != original.selector).then_some(
                Command::SetTextAnimatorSelector {
                    id: layer.id(),
                    animator,
                    selector,
                },
            ));
        }
        AnimatorAction::Animation { parameter, edit } => {
            return animator_command(layer, parameter, frame, edit);
        }
        AnimatorAction::SelectorAnimation {
            selector,
            parameter,
            edit,
        } => return selector_animation_command(layer, selector, parameter, frame, edit),
        AnimatorAction::Units(units) => selector.units = units,
        AnimatorAction::Shape(shape) => selector.shape = shape,
        AnimatorAction::SelectSelector(_) => return Err("Selection is a panel action".into()),
        AnimatorAction::AddSelector => {
            return Ok(
                (layer.text_range_selectors().len() < MAX_TEXT_RANGE_SELECTORS)
                    .then_some(Command::AddTextRangeSelector { id: layer.id() }),
            );
        }
        AnimatorAction::RemoveSelector(id) => {
            range_selector(layer, id).ok_or("Text selector no longer exists")?;
            return Ok(Some(Command::RemoveTextRangeSelector {
                id: layer.id(),
                selector: id,
            }));
        }
        AnimatorAction::MoveSelector {
            selector: id,
            index,
        } => {
            let selectors = layer.text_range_selectors();
            let current = selectors
                .iter()
                .position(|selector| selector.id == id)
                .ok_or("Text selector no longer exists")?;
            if index >= selectors.len() {
                return Err("Text selector position is out of range".into());
            }
            return Ok(
                (current != index).then_some(Command::MoveTextRangeSelector {
                    id: layer.id(),
                    selector: id,
                    index,
                }),
            );
        }
        AnimatorAction::SelectorUnits { selector: id, .. }
        | AnimatorAction::SelectorShape { selector: id, .. }
        | AnimatorAction::SelectorMode { selector: id, .. } => {
            let original = range_selector(layer, id).ok_or("Text selector no longer exists")?;
            let mut selector = original.clone();
            match action {
                AnimatorAction::SelectorUnits { units, .. } => selector.selector.units = units,
                AnimatorAction::SelectorShape { shape, .. } => selector.selector.shape = shape,
                AnimatorAction::SelectorMode { mode, .. } => selector.mode = mode,
                _ => unreachable!(),
            }
            return Ok(
                (selector != *original).then_some(Command::SetTextRangeSelector {
                    id: layer.id(),
                    selector,
                }),
            );
        }
    }
    Ok(
        (selector != layer.text_selector()).then_some(Command::SetTextSelector {
            id: layer.id(),
            selector,
        }),
    )
}

fn animator_blocked(state: &EditorState) -> bool {
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
}

/// Preserve the frozen source receipt while additionally retiring controls on
/// transport/action round trips and pending source-text or modal editing.
#[derive(Clone)]
struct AnimatorTarget {
    input: InputTarget,
    layer: u64,
    animator: Option<u64>,
    selector: Option<u64>,
    composition: u64,
    document_revision: u64,
    transport: u64,
    generation: u64,
    tool: crate::editor::Tool,
    selected_layers: std::collections::BTreeSet<u64>,
    values: [f64; ANIMATOR_FIELDS],
}
impl AnimatorTarget {
    fn capture(state: &EditorState) -> Option<Self> {
        Self::capture_selector(state, None)
    }
    fn capture_selector(state: &EditorState, selector: Option<u64>) -> Option<Self> {
        Self::capture_scope(state, None, selector)
    }
    fn capture_scope(
        state: &EditorState,
        animator: Option<u64>,
        selector: Option<u64>,
    ) -> Option<Self> {
        if animator_blocked(state) || (animator.is_some() && selector.is_some()) {
            return None;
        }
        let layer = state
            .editor
            .selected_layer()
            .filter(|l| matches!(l.content(), Content::Text { .. }))?;
        if selector.is_some_and(|id| range_selector(layer, id).is_none())
            || animator.is_some_and(|id| extra_animator(layer, id).is_none())
        {
            return None;
        }
        Some(Self {
            animator,
            selector,
            composition: state.editor.project().active_composition_id(),
            document_revision: state.document_revision,
            input: InputTarget::new(state)?,
            layer: layer.id(),
            transport: state.transport_generation(),
            generation: state.input_context_generation(),
            tool: state.tool,
            selected_layers: state.selected_layers.clone(),
            values: scoped_values(layer, state.frame, animator, selector),
        })
    }
    fn same_owner(&self, state: &EditorState) -> bool {
        !animator_blocked(state)
            && self.input.same_context(state)
            && self.tool == state.tool
            && self.selected_layers == state.selected_layers
            && state.editor.selected_layer().is_some_and(|l| {
                matches!(l.content(), Content::Text { .. })
                    && self
                        .animator
                        .is_none_or(|id| extra_animator(l, id).is_some())
                    && self
                        .selector
                        .is_none_or(|id| range_selector(l, id).is_some())
            })
    }
    fn current(&self, state: &EditorState) -> bool {
        self.same_owner(state)
            && self.input.current(state)
            && self.transport == state.transport_generation()
            && self.generation == state.input_context_generation()
    }
    fn key(&self) -> String {
        format!(
            "animator-{}-{}-{}-{:?}-{:?}",
            self.input.binding(),
            self.transport,
            self.generation,
            self.animator,
            self.selector
        )
    }
    fn field_key(&self, index: usize) -> String {
        format!("{}-{index}", self.key())
    }
    fn display(&self, state: &EditorState, index: usize) -> String {
        if self.same_owner(state) {
            scoped_values(
                state.editor.selected_layer().unwrap(),
                state.frame,
                self.animator,
                self.selector,
            )[index]
                .to_string()
        } else {
            self.values[index].to_string()
        }
    }
}

#[derive(Default)]
struct AnimatorInput {
    target: Option<AnimatorTarget>,
    // Exactly one pending animator field may grant a synchronous rebase.
    armed: Option<(String, Option<usize>)>,
    flushed: Option<(String, AnimatorTarget)>,
}
impl AnimatorInput {
    fn observe(&mut self, state: &EditorState) {
        if self.target.as_ref().is_some_and(|t| t.current(state)) {
            return;
        }
        let selected = self.target.as_ref().filter(|target| {
            target.composition == state.editor.project().active_composition_id()
                && target.document_revision == state.document_revision
                && state
                    .editor
                    .selected_layer()
                    .is_some_and(|layer| layer.id() == target.layer)
        });
        self.target = selected
            .and_then(|target| {
                AnimatorTarget::capture_scope(state, target.animator, target.selector)
            })
            .or_else(|| AnimatorTarget::capture(state));
        self.armed = None;
        self.flushed = None;
    }
    fn current(&self, target: &AnimatorTarget, state: &EditorState) -> bool {
        self.target
            .as_ref()
            .is_some_and(|current| current.key() == target.key())
            && target.current(state)
    }
    fn prepare(
        &mut self,
        target: &AnimatorTarget,
        state: &EditorState,
        pending: Option<String>,
    ) -> bool {
        if !self.current(target, state) {
            return false;
        }
        self.armed = None;
        self.flushed = None;
        let index = match pending {
            Some(pending) => {
                let Some(index) =
                    (0..ANIMATOR_FIELDS).find(|index| target.field_key(*index) == pending)
                else {
                    return false;
                };
                Some(index)
            }
            None => None,
        };
        self.armed = Some((target.key(), index));
        true
    }
    /// Read-only Timeline lanes cannot own a pending numeric draft. Reject
    /// foreign input before the workspace's outside-down commit can run.
    fn prepare_readonly(
        &mut self,
        target: &AnimatorTarget,
        state: &EditorState,
        pending: Option<String>,
    ) -> bool {
        if !self.current(target, state) {
            return false;
        }
        self.armed = None;
        self.flushed = None;
        pending.is_none() && self.prepare(target, state, None)
    }
    fn action_target(
        &self,
        target: &AnimatorTarget,
        state: &EditorState,
    ) -> Option<AnimatorTarget> {
        let (origin, next) = self.flushed.as_ref()?;
        ((*origin == target.key() || next.key() == target.key()) && self.current(next, state))
            .then(|| next.clone())
    }
    fn finish_flush(&mut self, target: &AnimatorTarget, state: &EditorState) -> bool {
        if !self.current(target, state) && self.action_target(target, state).is_none() {
            return false;
        }
        let next = if self.armed == Some((target.key(), None)) && self.current(target, state) {
            Some(target.clone())
        } else {
            self.action_target(target, state)
        };
        // A canceled pointer must never leave field permission armed.
        self.armed = None;
        self.flushed = next.map(|next| (target.key(), next));
        self.flushed.is_some()
    }
    fn take_action(
        &mut self,
        target: &AnimatorTarget,
        state: &EditorState,
        pointer: bool,
    ) -> Option<AnimatorTarget> {
        let next = if pointer {
            self.action_target(target, state)
        } else {
            self.current(target, state).then(|| target.clone())
        }?;
        self.armed = None;
        self.flushed = None;
        Some(next)
    }
}

/// Shared by the real guarded TextField callback and headless input tests.
/// Never finish a Source Text session from an animator numeric-field callback.
fn submit_animator_field(
    session: &Rc<RefCell<AnimatorInput>>,
    target: &AnimatorTarget,
    state: &mut EditorState,
    index: usize,
    text: &str,
    active: bool,
    apply: impl FnOnce(&mut EditorState, Command) -> bool,
) -> String {
    let armed = session.borrow().armed.clone();
    let current = session.borrow().current(target, state);
    let expected = armed
        .as_ref()
        .is_none_or(|(origin, field)| *origin == target.key() && *field == Some(index));
    let mut valid = false;
    if index >= ANIMATOR_FIELDS {
        return String::new();
    }
    if !active || !expected || !current {
        state.status = ANIMATOR_STALE.into();
    } else {
        let layer = state.editor.selected_layer().unwrap();
        let command = if target.animator.is_some() {
            scoped_field_command(layer, state.frame, target.animator, None, index, text)
        } else if target.selector.is_some() {
            selector_field_command(layer, state.frame, target.selector, index, text)
        } else {
            animator_field_command(layer, state.frame, index, text)
        };
        match command {
            Ok(None) => valid = true,
            Ok(Some(command)) => {
                valid = apply(state, command)
                    && target.same_owner(state)
                    && target.generation.checked_add(1) == Some(state.input_context_generation())
                    && target.transport.wrapping_add(1) == state.transport_generation();
            }
            Err(error) => state.status = error,
        }
    }
    let mut session = session.borrow_mut();
    // A late callback cannot retire a newer source-bound field or click receipt.
    if !session
        .target
        .as_ref()
        .is_some_and(|current| current.key() == target.key())
    {
        return target.display(state, index);
    }
    session.armed = None;
    session.flushed = None;
    if valid {
        let next = AnimatorTarget::capture_scope(state, target.animator, target.selector);
        if let (Some((origin, Some(_))), Some(next)) = (&armed, &next) {
            session.flushed = Some((origin.clone(), next.clone()));
        }
        session.target = next;
    }
    target.display(state, index)
}

/// The same receipt is consumed by static selectors and animation buttons.
/// A stale action must not finish or replace a pending Source Text session.
fn submit_animator_action(
    session: &Rc<RefCell<AnimatorInput>>,
    target: &AnimatorTarget,
    state: &mut EditorState,
    pointer: bool,
    action: AnimatorAction,
    apply: impl FnOnce(&mut EditorState, Command) -> bool,
) -> bool {
    let Some(target) = session.borrow_mut().take_action(target, state, pointer) else {
        return false;
    };
    if !target.current(state) {
        return false;
    }
    if let AnimatorAction::SelectAnimator(animator) = action {
        if animator == target.animator && target.selector.is_none() {
            return true;
        }
        let Some(next) = AnimatorTarget::capture_scope(state, animator, None) else {
            return false;
        };
        session.borrow_mut().target = Some(next);
        return true;
    }
    if target.animator.is_some()
        && matches!(
            action,
            AnimatorAction::Animation { .. }
                | AnimatorAction::SelectorAnimation { .. }
                | AnimatorAction::Units(_)
                | AnimatorAction::Shape(_)
                | AnimatorAction::SelectSelector(_)
                | AnimatorAction::AddSelector
                | AnimatorAction::RemoveSelector(_)
                | AnimatorAction::MoveSelector { .. }
                | AnimatorAction::SelectorUnits { .. }
                | AnimatorAction::SelectorShape { .. }
                | AnimatorAction::SelectorMode { .. }
        )
    {
        return false;
    }
    if let AnimatorAction::SelectSelector(selector) = action {
        if selector == target.selector {
            return true;
        }
        // InputTarget::new gives every actual row switch a fresh monotonic
        // identity, including A → B → A with an unchanged source/history.
        let Some(next) = AnimatorTarget::capture_selector(state, selector) else {
            return false;
        };
        session.borrow_mut().target = Some(next);
        return true;
    }
    // A secondary range can only edit its stable-ID channels. Shared
    // transforms intentionally keep the existing primary addresses.
    if target.selector.is_some()
        && matches!(action, AnimatorAction::Animation { parameter, .. }
        if RANGE_FIELDS.iter().any(|index| ANIMATOR_PARAMETERS[*index] == parameter))
    {
        return false;
    }
    match animator_action_command(state.editor.selected_layer().unwrap(), state.frame, action) {
        Ok(Some(command)) => {
            let applied = apply(state, command);
            if applied && matches!(action, AnimatorAction::AddAnimator) {
                let selected = state
                    .editor
                    .selected_layer()
                    .and_then(|layer| layer.text_animators().last())
                    .map(|animator| animator.id);
                session.borrow_mut().target = AnimatorTarget::capture_scope(state, selected, None);
            }
            if applied && matches!(action, AnimatorAction::AddSelector) {
                let selected = state
                    .editor
                    .selected_layer()
                    .and_then(|layer| layer.text_range_selectors().last())
                    .map(|selector| selector.id);
                session.borrow_mut().target = AnimatorTarget::capture_selector(state, selected);
            }
            applied
        }
        Ok(None) => true,
        Err(error) => {
            state.status = error;
            false
        }
    }
}

fn animator_activation_allowed(
    pointer: bool,
    active: bool,
    modified: bool,
    composing: bool,
    field_focused: bool,
    pending_input: bool,
) -> bool {
    active && !modified && !composing && (pointer || (!field_focused && !pending_input))
}

fn animator_button(
    button: gpui::Stateful<gpui::Div>,
    control: String,
    action: AnimatorAction,
    state: &Entity<EditorState>,
    session: &Rc<RefCell<AnimatorInput>>,
    target: Option<AnimatorTarget>,
    disabled_input: Option<InputTarget>,
    allow_pending_fields: bool,
) -> gpui::Stateful<gpui::Div> {
    let Some(target) = target else {
        // A pending Source Text session can have a valid generic source target
        // while animator editing is blocked. Reject its press before blur.
        return crate::color_edit::input_pointer_button_guarded(
            button,
            control,
            disabled_input,
            |_, _, _| false,
        );
    };
    let state = state.clone();
    let session = session.clone();
    let guard_session = session.clone();
    let guard_target = target.clone();
    let input = Some(target.input.clone());
    crate::color_edit::input_pointer_button_guarded(
        button,
        control.clone(),
        input.clone(),
        move |state, cx, after_flush| {
            let mut session = guard_session.borrow_mut();
            if after_flush {
                session.finish_flush(&guard_target, state)
            } else if allow_pending_fields {
                session.prepare(&guard_target, state, TextField::active_pending_binding(cx))
            } else {
                session.prepare_readonly(
                    &guard_target,
                    state,
                    TextField::active_pending_binding(cx),
                )
            }
        },
    )
    .on_click(move |event, w, cx| {
        cx.stop_propagation();
        let pointer = matches!(event, gpui::ClickEvent::Mouse(_));
        if !animator_activation_allowed(
            pointer,
            w.is_window_active(),
            event.modifiers().modified()
                || matches!(event, gpui::ClickEvent::Mouse(click) if click.down.modifiers.modified()),
            TextField::is_composing(w, cx),
            TextField::active_has_focus(w, cx),
            TextField::active_pending_binding(cx).is_some(),
        ) || crate::color_edit::input_click_target(&control, event, &input, &state, w, cx)
                .is_none()
        {
            return;
        }
        state.update(cx, |state, cx| {
            submit_animator_action(&session, &target, state, pointer, action, |state, command| {
                state.dispatch(&Action::Edit(command), w, cx);
                state.status == "Edited"
            });
            cx.notify();
        });
    })
}

/// Timeline owns no editable animator fields. Its session deliberately stays
/// separate from Properties: pending input must be committed with Enter first.
#[derive(Default)]
pub(super) struct TimelineAnimator {
    input: Rc<RefCell<AnimatorInput>>,
}
impl TimelineAnimator {
    pub(super) fn observe(&self, state: &EditorState) {
        self.input.borrow_mut().observe(state);
    }
    pub(super) fn control(
        &self,
        state: &Entity<EditorState>,
        layer: &Layer,
        parameter: TextParam,
        frame: Frame,
        animation: bool,
        disabled_input: Option<InputTarget>,
    ) -> gpui::Stateful<gpui::Div> {
        self.track_control(
            state,
            layer,
            PropertyPath::Text(parameter),
            frame,
            animation,
            disabled_input,
        )
    }
    pub(super) fn selector_control(
        &self,
        state: &Entity<EditorState>,
        layer: &Layer,
        selector: u64,
        parameter: TextSelectorParam,
        frame: Frame,
        animation: bool,
        disabled_input: Option<InputTarget>,
    ) -> gpui::Stateful<gpui::Div> {
        self.track_control(
            state,
            layer,
            PropertyPath::TextSelector {
                selector,
                parameter,
            },
            frame,
            animation,
            disabled_input,
        )
    }
    pub(super) fn animator_control(
        &self,
        state: &Entity<EditorState>,
        layer: &Layer,
        animator: u64,
        parameter: TextParam,
        frame: Frame,
        animation: bool,
        disabled_input: Option<InputTarget>,
    ) -> gpui::Stateful<gpui::Div> {
        self.track_control(
            state,
            layer,
            PropertyPath::TextAnimator {
                animator,
                parameter,
            },
            frame,
            animation,
            disabled_input,
        )
    }
    fn track_control(
        &self,
        state: &Entity<EditorState>,
        layer: &Layer,
        property: PropertyPath,
        frame: Frame,
        animation: bool,
        disabled_input: Option<InputTarget>,
    ) -> gpui::Stateful<gpui::Div> {
        let track = layer.track(property);
        let active = track.is_some_and(|track| {
            if animation {
                !track.keys().is_empty()
            } else {
                track.keys().contains_key(&frame)
            }
        });
        let (edit, title) = match (animation, active) {
            (true, true) => (
                AnimatorEdit::Disable,
                "Disable animation · delete all keys for this property",
            ),
            (true, false) => (AnimatorEdit::Enable, "Enable animation"),
            (false, true) => (AnimatorEdit::RemoveKey, "Remove current key"),
            (false, false) => (AnimatorEdit::AddKey, "Add current key"),
        };
        let control = format!("timeline-animator-{}-{property:?}-{edit:?}", layer.id());
        let action = match property {
            PropertyPath::Text(parameter) => AnimatorAction::Animation { parameter, edit },
            PropertyPath::TextAnimator {
                animator,
                parameter,
            } => AnimatorAction::ExtraAnimation {
                animator,
                parameter,
                edit,
            },
            PropertyPath::TextSelector {
                selector,
                parameter,
            } => AnimatorAction::SelectorAnimation {
                selector,
                parameter,
                edit,
            },
            _ => unreachable!("Timeline animator controls require text properties"),
        };
        let target = self
            .input
            .borrow()
            .target
            .clone()
            .filter(|target| target.layer == layer.id());
        animator_button(
            ui::tool(
                SharedString::from(control.clone()),
                if animation { "stopwatch" } else { "diamond" },
                format!("{title} · select this layer and finish pending input with Enter first"),
                active,
            ),
            control,
            action,
            state,
            &self.input,
            target,
            disabled_input,
            false,
        )
    }
}

pub(crate) struct TextAnimator {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
    input: Rc<RefCell<AnimatorInput>>,
}
impl TextAnimator {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            fields: (0..ANIMATOR_FIELDS)
                .map(|_| cx.new(|cx| TextField::new(cx, |_, _, _| {})))
                .collect(),
            input: Default::default(),
        }
    }
}
impl Render for TextAnimator {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        self.input.borrow_mut().observe(state);
        let target = self.input.borrow().target.clone();
        let disabled_input = InputTarget::new(state);
        let source_editing = state.text_session.is_some();
        let frame = state.frame;
        let layer = state
            .editor
            .selected_layer()
            .filter(|layer| matches!(layer.content(), Content::Text { .. }))
            .cloned();
        let mut panel = div()
            .mt_3()
            .pt_3()
            .border_t_1()
            .border_color(rgb(ui::BORDER))
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(11.0))
            .child(div().text_color(rgb(ui::MUTED)).child("Text Animator"));
        let Some(layer) = layer else {
            return panel;
        };
        if source_editing {
            panel = panel.child(
                div()
                    .text_color(rgb(ui::MUTED))
                    .child("Finish Source Text editing to change Text Animator settings."),
            );
        }
        let selected_animator = target.as_ref().and_then(|target| target.animator);
        let selected = target.as_ref().and_then(|target| target.selector);
        let primary_control = "animator-select-primary".to_string();
        let mut animators = div().flex().flex_col().gap_1().child(animator_button(
            ui::text_button(
                SharedString::from(primary_control.clone()),
                "1 · Primary · pinned first",
            )
            .when(selected_animator.is_none(), |button| {
                button.text_color(rgb(ui::BLUE))
            }),
            primary_control,
            AnimatorAction::SelectAnimator(None),
            &self.state,
            &self.input,
            target.clone(),
            disabled_input.clone(),
            true,
        ));
        for (index, animator) in layer.text_animators().iter().enumerate() {
            let control = format!("animator-select-extra-{}", animator.id);
            let mut row = div().flex().flex_wrap().gap_1().child(animator_button(
                ui::text_button(
                    SharedString::from(control.clone()),
                    format!("{} · Animator #{} · single range", index + 2, animator.id),
                )
                .when(selected_animator == Some(animator.id), |button| {
                    button.text_color(rgb(ui::BLUE))
                }),
                control,
                AnimatorAction::SelectAnimator(Some(animator.id)),
                &self.state,
                &self.input,
                target.clone(),
                disabled_input.clone(),
                true,
            ));
            let mut actions = Vec::new();
            if index > 0 {
                actions.push((
                    "up",
                    "Move up",
                    AnimatorAction::MoveAnimator {
                        animator: animator.id,
                        index: index - 1,
                    },
                ));
            }
            if index + 1 < layer.text_animators().len() {
                actions.push((
                    "down",
                    "Move down",
                    AnimatorAction::MoveAnimator {
                        animator: animator.id,
                        index: index + 1,
                    },
                ));
            }
            actions.push((
                "remove",
                "Remove",
                AnimatorAction::RemoveAnimator(animator.id),
            ));
            for (name, label, action) in actions {
                let control = format!("animator-extra-{}-{name}", animator.id);
                row = row.child(animator_button(
                    ui::text_button(SharedString::from(control.clone()), label).text_size(px(10.0)),
                    control,
                    action,
                    &self.state,
                    &self.input,
                    target.clone(),
                    disabled_input.clone(),
                    true,
                ));
            }
            animators = animators.child(row);
        }
        if layer.text_animators().len() < MAX_TEXT_ANIMATORS {
            let control = "animator-add-extra".to_string();
            animators = animators.child(animator_button(
                ui::text_button(
                    SharedString::from(control.clone()),
                    "Add animator · single range",
                ),
                control,
                AnimatorAction::AddAnimator,
                &self.state,
                &self.input,
                target.clone(),
                disabled_input.clone(),
                true,
            ));
        } else {
            animators = animators.child(
                div()
                    .text_color(rgb(ui::MUTED))
                    .child("3 additional animators maximum"),
            );
        }
        panel = panel.child(animators);
        if selected_animator.is_none() {
            panel = panel.child(
                div()
                    .mt_2()
                    .text_color(rgb(ui::MUTED))
                    .child("Primary · ordered ranges"),
            );
            let primary_control = "animator-select-primary-range".to_string();
            let mut stack = div().flex().flex_col().gap_1().child(animator_button(
                ui::text_button(
                    SharedString::from(primary_control.clone()),
                    "1 · Primary range · pinned first",
                )
                .when(selected.is_none(), |button| {
                    button.text_color(rgb(ui::BLUE))
                }),
                primary_control,
                AnimatorAction::SelectSelector(None),
                &self.state,
                &self.input,
                target.clone(),
                disabled_input.clone(),
                true,
            ));
            for (index, range) in layer.text_range_selectors().iter().enumerate() {
                let control = format!("animator-select-{}", range.id);
                let mut row = div().flex().flex_wrap().gap_1().child(animator_button(
                    ui::text_button(
                        SharedString::from(control.clone()),
                        format!(
                            "{} · Selector #{} · {}",
                            index + 2,
                            range.id,
                            range.mode.label()
                        ),
                    )
                    .when(selected == Some(range.id), |button| {
                        button.text_color(rgb(ui::BLUE))
                    }),
                    control,
                    AnimatorAction::SelectSelector(Some(range.id)),
                    &self.state,
                    &self.input,
                    target.clone(),
                    disabled_input.clone(),
                    true,
                ));
                let mut actions = Vec::new();
                if index > 0 {
                    actions.push((
                        "up",
                        "Move up",
                        AnimatorAction::MoveSelector {
                            selector: range.id,
                            index: index - 1,
                        },
                    ));
                }
                if index + 1 < layer.text_range_selectors().len() {
                    actions.push((
                        "down",
                        "Move down",
                        AnimatorAction::MoveSelector {
                            selector: range.id,
                            index: index + 1,
                        },
                    ));
                }
                actions.push(("remove", "Remove", AnimatorAction::RemoveSelector(range.id)));
                for (name, label, action) in actions {
                    let control = format!("animator-selector-{}-{name}", range.id);
                    row = row.child(animator_button(
                        ui::text_button(SharedString::from(control.clone()), label)
                            .text_size(px(10.0)),
                        control,
                        action,
                        &self.state,
                        &self.input,
                        target.clone(),
                        disabled_input.clone(),
                        true,
                    ));
                }
                stack = stack.child(row);
            }
            if layer.text_range_selectors().len() < MAX_TEXT_RANGE_SELECTORS {
                let control = "animator-add-selector".to_string();
                stack = stack.child(animator_button(
                    ui::text_button(SharedString::from(control.clone()), "Add selector"),
                    control,
                    AnimatorAction::AddSelector,
                    &self.state,
                    &self.input,
                    target.clone(),
                    disabled_input.clone(),
                    true,
                ));
            } else {
                stack = stack.child(
                    div()
                        .text_color(rgb(ui::MUTED))
                        .child("7 secondary selectors maximum"),
                );
            }
            panel = panel.child(stack);
        }
        panel = panel.child(div().text_color(rgb(ui::MUTED)).child(
            match (selected_animator, selected) {
                (Some(id), _) => format!("Animator #{id} · single range · all channels animate"),
                (_, Some(id)) => format!("Primary · Selector #{id} · animation available"),
                _ => "Primary range · animation available".into(),
            },
        ));
        let selector = selected_animator
            .and_then(|id| extra_animator(&layer, id))
            .map(|animator| animator.selector)
            .or_else(|| {
                selected
                    .and_then(|id| range_selector(&layer, id))
                    .map(|range| range.selector)
            })
            .unwrap_or_else(|| layer.text_selector());
        if let Some(range) = selected.and_then(|id| range_selector(&layer, id)) {
            let mut modes = div().flex().flex_wrap().gap_1();
            for mode in TextSelectorMode::ALL {
                let control = format!("animator-selector-{}-mode-{mode:?}", range.id);
                modes = modes.child(animator_button(
                    ui::text_button(SharedString::from(control.clone()), mode.label())
                        .when(range.mode == mode, |button| {
                            button.text_color(rgb(ui::BLUE))
                        }),
                    control,
                    AnimatorAction::SelectorMode {
                        selector: range.id,
                        mode,
                    },
                    &self.state,
                    &self.input,
                    target.clone(),
                    disabled_input.clone(),
                    true,
                ));
            }
            panel = panel
                .child(div().text_color(rgb(ui::MUTED)).child("Mode · static"))
                .child(modes);
        }
        let mut units = div().flex().flex_wrap().gap_1();
        for value in TextSelectorUnits::ALL {
            let control = format!("animator-units-{value:?}");
            units = units.child(animator_button(
                ui::text_button(SharedString::from(control.clone()), value.label())
                    .when(selector.units == value, |b| b.text_color(rgb(ui::BLUE))),
                control,
                selected_animator.map_or_else(
                    || {
                        selected.map_or(AnimatorAction::Units(value), |selector| {
                            AnimatorAction::SelectorUnits {
                                selector,
                                units: value,
                            }
                        })
                    },
                    |animator| AnimatorAction::AnimatorUnits {
                        animator,
                        units: value,
                    },
                ),
                &self.state,
                &self.input,
                target.clone(),
                disabled_input.clone(),
                true,
            ));
        }
        let mut shapes = div().flex().flex_wrap().gap_1();
        for value in TextSelectorShape::ALL {
            let control = format!("animator-shape-{value:?}");
            shapes = shapes.child(animator_button(
                ui::text_button(SharedString::from(control.clone()), value.label())
                    .when(selector.shape == value, |b| b.text_color(rgb(ui::BLUE))),
                control,
                selected_animator.map_or_else(
                    || {
                        selected.map_or(AnimatorAction::Shape(value), |selector| {
                            AnimatorAction::SelectorShape {
                                selector,
                                shape: value,
                            }
                        })
                    },
                    |animator| AnimatorAction::AnimatorShape {
                        animator,
                        shape: value,
                    },
                ),
                &self.state,
                &self.input,
                target.clone(),
                disabled_input.clone(),
                true,
            ));
        }
        panel = panel
            .child(div().text_color(rgb(ui::MUTED)).child("Units · static"))
            .child(units)
            .child(div().text_color(rgb(ui::MUTED)).child("Shape · static"))
            .child(shapes);
        let values = scoped_values(&layer, frame, selected_animator, selected);
        // Keep existing field bindings stable while adding transform controls after Amount.
        for index in ANIMATOR_DISPLAY_ORDER {
            let parameter = ANIMATOR_PARAMETERS[index];
            if index == 7 {
                panel = panel.child(div().mt_2().text_color(rgb(ui::MUTED)).child(
                    if selected_animator.is_some() {
                        "This animator’s transform"
                    } else {
                        "Primary · shared animator transform"
                    },
                ));
            }
            if let Some(target) = target.clone() {
                let state = self.state.clone();
                let session = self.input.clone();
                self.fields[index].update(cx, |field, _| {
                    field.sync_guarded(
                        target.field_key(index),
                        values[index].to_string(),
                        w,
                        move |text, w, cx| {
                            state.update(cx, |state, cx| {
                                let display = submit_animator_field(
                                    &session,
                                    &target,
                                    state,
                                    index,
                                    text,
                                    w.is_window_active(),
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
            let property = scoped_property(selected_animator, selected, index);
            let track = layer.track(property);
            let animated = track.is_some_and(|track| !track.keys().is_empty());
            let keyed = track.is_some_and(|track| track.keys().contains_key(&frame));
            let mut buttons = div().flex().flex_wrap().gap_1();
            for (edit, label) in [
                (
                    if animated {
                        AnimatorEdit::Disable
                    } else {
                        AnimatorEdit::Enable
                    },
                    if animated {
                        "Disable · delete keys"
                    } else {
                        "Enable animation"
                    },
                ),
                (
                    if keyed {
                        AnimatorEdit::RemoveKey
                    } else {
                        AnimatorEdit::AddKey
                    },
                    if keyed { "Remove key" } else { "Add key" },
                ),
            ] {
                let control = format!("animator-{property:?}-{edit:?}");
                let action = match property {
                    PropertyPath::TextAnimator {
                        animator,
                        parameter,
                    } => AnimatorAction::ExtraAnimation {
                        animator,
                        parameter,
                        edit,
                    },
                    PropertyPath::TextSelector {
                        selector,
                        parameter,
                    } => AnimatorAction::SelectorAnimation {
                        selector,
                        parameter,
                        edit,
                    },
                    _ => AnimatorAction::Animation { parameter, edit },
                };
                buttons = buttons.child(animator_button(
                    ui::text_button(SharedString::from(control.clone()), label).text_size(px(10.0)),
                    control,
                    action,
                    &self.state,
                    &self.input,
                    target.clone(),
                    disabled_input.clone(),
                    true,
                ));
            }
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(div().w(px(118.0)).child(ANIMATOR_LABELS[index]))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .when(target.is_none(), |d| d.child(values[index].to_string()))
                                    .when(target.is_some(), |d| {
                                        d.child(self.fields[index].clone())
                                    }),
                            ),
                    )
                    .child(buttons),
            );
        }
        panel
            .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(MULTIPLE_ANIMATORS_HELP))
            .when(selected_animator.is_none(), |panel| panel.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child(ANIMATOR_STACK_HELP),
            ))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child(ANIMATOR_HELP),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child(if selected_animator.is_some() {
                        "This animator’s Units defines protected Character clusters, whole Words or hard source Lines and their original first-logical-glyph baseline pivots, shared across automatic wrapping. Weighted Scale = 1 + influence × (Scale / 100 − 1), weighted Rotation = influence × Rotation, then weighted Position. No reflow or caret movement. Scale 0 can collapse ink; negative scale is rejected."
                    } else { ANIMATOR_TRANSFORM_HELP }),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child(ANIMATOR_ANIMATION_HELP),
            )
    }
}

#[cfg(test)]
#[path = "text_animator_controls_tests.rs"]
mod tests;
