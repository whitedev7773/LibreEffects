//! Source-bound editing of the independent sides of imported Opacity keys.
use crate::{
    color_edit::InputTarget,
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, Frame, Layer, OpacityEdit, OpacityInterpolation};
use libre_effects_editor_model::automation_ui::FocusedToggleAction;
use std::{
    cell::RefCell,
    ops::Bound::{Excluded, Unbounded},
    rc::Rc,
};

#[derive(Clone, Copy)]
enum Field {
    Value,
    InSpeed,
    InInfluence,
    OutSpeed,
    OutInfluence,
}
impl Field {
    const ALL: [Self; 5] = [
        Self::Value,
        Self::InSpeed,
        Self::InInfluence,
        Self::OutSpeed,
        Self::OutInfluence,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Value => "Value (%)",
            Self::InSpeed => "In speed (%/s)",
            Self::InInfluence => "In influence (%)",
            Self::OutSpeed => "Out speed (%/s)",
            Self::OutInfluence => "Out influence (%)",
        }
    }
    fn value(self, layer: &Layer, frame: Frame, seconds_per_frame: f64) -> Result<f64, String> {
        if matches!(self, Self::Value) {
            return layer.opacity_at(frame, seconds_per_frame);
        }
        let key = layer
            .opacity_timing()
            .and_then(|t| t.keys().get(&frame))
            .ok_or("Move to an Opacity key to edit easing")?;
        Ok(match self {
            Self::InSpeed => key.in_ease.speed,
            Self::InInfluence => key.in_ease.influence,
            Self::OutSpeed => key.out_ease.speed,
            Self::OutInfluence => key.out_ease.influence,
            Self::Value => unreachable!(),
        })
    }
}

fn field_command(
    layer: &Layer,
    frame: Frame,
    seconds_per_frame: f64,
    field: Field,
    text: &str,
) -> Result<Option<Command>, String> {
    if layer.locked() || !layer.has_opacity_timing() {
        return Err("Select an unlocked layer with Opacity timing".into());
    }
    let value = text
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or("Enter a finite Opacity value")?;
    // Unchanged input must not add a key at an unkeyed overshoot sample. Display
    // roundtrip precision also preserves tiny and dormant endpoint-side values.
    if field.value(layer, frame, seconds_per_frame)?.to_bits() == value.to_bits() {
        return Ok(None);
    }
    let edit = if matches!(field, Field::Value) {
        if !(0.0..=100.0).contains(&value) {
            return Err("Authored Opacity key values must be between 0 and 100%".into());
        }
        OpacityEdit::Key { frame, value }
    } else {
        let key = layer
            .opacity_timing()
            .unwrap()
            .keys()
            .get(&frame)
            .ok_or("Opacity key no longer exists")?;
        let (mut incoming, mut outgoing) = (key.in_ease, key.out_ease);
        match field {
            Field::InSpeed => incoming.speed = value,
            Field::InInfluence => incoming.influence = value,
            Field::OutSpeed => outgoing.speed = value,
            Field::OutInfluence => outgoing.influence = value,
            Field::Value => unreachable!(),
        }
        incoming.validate().map_err(|e| e.to_string())?;
        outgoing.validate().map_err(|e| e.to_string())?;
        OpacityEdit::TemporalEase {
            frame,
            incoming,
            outgoing,
        }
    };
    Ok(Some(Command::SetOpacityTiming {
        id: layer.id(),
        edit,
    }))
}

#[derive(Clone, Copy)]
enum Operation {
    Previous,
    Next,
    Add,
    Remove,
    Collapse,
    Mode(bool),
}
fn operation(
    layer: &Layer,
    frame: Frame,
    seconds_per_frame: f64,
    op: Operation,
) -> Result<Option<Action>, String> {
    if layer.locked() {
        return Err("Unlock the layer before editing Opacity".into());
    }
    let keys = layer
        .opacity_timing()
        .ok_or("Opacity timing no longer exists")?
        .keys();
    let edit = match op {
        Operation::Previous => {
            return Ok(keys
                .range(..frame)
                .next_back()
                .map(|(&frame, _)| Action::Seek(frame)));
        }
        Operation::Next => {
            return Ok(keys
                .range((Excluded(frame), Unbounded))
                .next()
                .map(|(&frame, _)| Action::Seek(frame)));
        }
        Operation::Add => {
            if keys.contains_key(&frame) {
                return Ok(None);
            }
            let value = layer.opacity_at(frame, seconds_per_frame)?;
            if !(0.0..=100.0).contains(&value) {
                return Err("Enter a value between 0 and 100% to add a key at this time".into());
            }
            OpacityEdit::Key { frame, value }
        }
        Operation::Remove => {
            if !keys.contains_key(&frame) {
                return Ok(None);
            }
            if keys.len() == 1 {
                return Err("The final Opacity key cannot be deleted without converting the animation to a static value".into());
            }
            OpacityEdit::RemoveKey { frame }
        }
        Operation::Collapse => {
            let value = layer.opacity_at(frame, seconds_per_frame)?;
            if !(0.0..=100.0).contains(&value) {
                return Err(
                    "Enter a value between 0 and 100% before converting Opacity to a static value"
                        .into(),
                );
            }
            OpacityEdit::Collapse { value }
        }
        Operation::Mode(incoming) => {
            let key = keys
                .get(&frame)
                .ok_or("Move to an Opacity key to edit interpolation")?;
            let mode = match if incoming {
                key.in_interpolation
            } else {
                key.out_interpolation
            } {
                OpacityInterpolation::Linear => OpacityInterpolation::Bezier,
                OpacityInterpolation::Bezier => OpacityInterpolation::Hold,
                OpacityInterpolation::Hold => OpacityInterpolation::Linear,
            };
            if incoming
                && mode == OpacityInterpolation::Hold
                && keys
                    .range(..frame)
                    .next_back()
                    .is_some_and(|(_, key)| key.out_interpolation != OpacityInterpolation::Hold)
            {
                return Err("Set the previous key's outgoing side to Hold before holding this incoming segment".into());
            }
            if !incoming
                && mode != OpacityInterpolation::Hold
                && keys
                    .range((Excluded(frame), Unbounded))
                    .next()
                    .is_some_and(|(_, key)| key.in_interpolation == OpacityInterpolation::Hold)
            {
                return Err("Change the next key's incoming side from Hold before releasing this held segment".into());
            }
            OpacityEdit::Interpolation {
                frame,
                incoming: if incoming { mode } else { key.in_interpolation },
                outgoing: if incoming {
                    key.out_interpolation
                } else {
                    mode
                },
            }
        }
    };
    Ok(Some(Action::Edit(Command::SetOpacityTiming {
        id: layer.id(),
        edit,
    })))
}

pub(super) struct Controls {
    state: Entity<EditorState>,
    source: Option<InputTarget>,
    fields: Vec<Entity<TextField>>,
    targets: Vec<Rc<RefCell<Option<InputTarget>>>>,
}
impl Controls {
    pub(super) fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let targets: Vec<Rc<RefCell<Option<InputTarget>>>> =
            (0..5).map(|_| Default::default()).collect();
        let fields = Field::ALL
            .into_iter()
            .enumerate()
            .map(|(index, field)| {
                let state = state.clone();
                let target = targets[index].clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        state.update(cx, |s, cx| {
                            let Some(target) =
                                target.borrow().clone().filter(|target| target.current(s))
                            else {
                                return;
                            };
                            s.finish_text(true, cx);
                            if !target.same_context(s) {
                                return;
                            }
                            let Some(layer) = s.editor.selected_layer() else {
                                return;
                            };
                            match field_command(
                                layer,
                                s.frame,
                                s.editor.project().composition().fps().seconds(1),
                                field,
                                text,
                            ) {
                                Ok(Some(command)) => s.dispatch(&Action::Edit(command), window, cx),
                                Ok(None) => {}
                                Err(error) => {
                                    s.status = error;
                                    cx.notify();
                                }
                            }
                        });
                    })
                    .numeric()
                })
            })
            .collect();
        Self {
            state,
            source: None,
            fields,
            targets,
        }
    }
    fn button(
        &self,
        id: u64,
        suffix: &str,
        label: String,
        op: Operation,
        available: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let control = format!("native-opacity-{id}-{suffix}");
        let target = available.then(|| self.source.clone()).flatten();
        let button = ui::text_button(SharedString::from(control.clone()), label)
            .when(target.is_none(), |b| b.opacity(0.4));
        let key_state = self.state.clone();
        let key_target = target.clone();
        let button = button.capture_key_down(move |event, window, cx| {
            if !matches!(event.keystroke.key.as_str(), "enter" | "space") {
                return;
            }
            let target = key_target
                .as_ref()
                .filter(|t| t.current(key_state.read(cx)))
                .cloned();
            let action = crate::modal_keyboard::take_press(event, window, cx).map_or(
                FocusedToggleAction::Consume,
                |press| {
                    press.focused_toggle(
                        event.keystroke.modifiers.modified(),
                        window.is_window_active(),
                        key_state.read(cx).playing,
                        target.is_some(),
                    )
                },
            );
            if action == FocusedToggleAction::Pass {
                return;
            }
            cx.stop_propagation();
            window.prevent_default();
            if action == FocusedToggleAction::Activate
                && let Some(target) = target
            {
                perform(target, id, op, &key_state, window, cx);
            }
        });
        let state = self.state.clone();
        crate::color_edit::input_pointer_button(button, control.clone(), target.clone()).on_click(
            move |event, window, cx| {
                if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                    return;
                }
                cx.stop_propagation();
                if let Some(target) = crate::color_edit::input_click_target(
                    &control, event, &target, &state, window, cx,
                ) {
                    perform(target, id, op, &state, window, cx);
                }
            },
        )
    }
}
fn perform(
    target: InputTarget,
    id: u64,
    op: Operation,
    state: &Entity<EditorState>,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    if !target.current(state.read(cx)) {
        return;
    }
    TextField::commit_active(window, cx);
    state.update(cx, |s, cx| {
        if !target.same_context(s) {
            return;
        }
        s.finish_text(true, cx);
        if !target.same_context(s) {
            return;
        }
        let Some(layer) = s.editor.selected_layer().filter(|l| l.id() == id) else {
            return;
        };
        match operation(
            layer,
            s.frame,
            s.editor.project().composition().fps().seconds(1),
            op,
        ) {
            Ok(Some(action)) => s.dispatch(&action, window, cx),
            Ok(None) => {}
            Err(error) => {
                s.status = error;
                cx.notify();
            }
        }
    });
}
impl Render for Controls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let Some(layer) = s
            .editor
            .selected_layer()
            .filter(|l| l.has_opacity_timing())
            .cloned()
        else {
            self.source = None;
            for target in &self.targets {
                *target.borrow_mut() = None;
            }
            return div();
        };
        InputTarget::refresh(&mut self.source, s);
        let frame = s.frame;
        let seconds_per_frame = s.editor.project().composition().fps().seconds(1);
        let id = layer.id();
        let keys = layer.opacity_timing().unwrap().keys();
        let has_key = keys.contains_key(&frame);
        let mut root = div()
            .flex()
            .flex_col()
            .gap_1()
            .py_1()
            .text_size(px(11.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .child(format!("Opacity · {} keys", keys.len())),
                    )
                    .when(!layer.is_three_d(), |row| {
                        row.child(super::expression_editor::entry_button(
                            &self.state,
                            &layer,
                            libre_effects_core::ExpressionTarget::Opacity,
                        ))
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(self.button(
                        id,
                        "previous",
                        "← Key".into(),
                        Operation::Previous,
                        keys.range(..frame).next_back().is_some(),
                    ))
                    .child(self.button(
                        id,
                        "next",
                        "Key →".into(),
                        Operation::Next,
                        keys.range((Excluded(frame), Unbounded)).next().is_some(),
                    ))
                    .child(self.button(
                        id,
                        "key",
                        if has_key { "Delete key" } else { "Add key" }.into(),
                        if has_key {
                            Operation::Remove
                        } else {
                            Operation::Add
                        },
                        !has_key || keys.len() > 1,
                    )),
            );
        for (index, field) in Field::ALL.into_iter().enumerate() {
            if !has_key && !matches!(field, Field::Value) {
                *self.targets[index].borrow_mut() = None;
                continue;
            }
            match field.value(&layer, frame, seconds_per_frame) {
                Ok(value) => {
                    *self.targets[index].borrow_mut() = self.source.clone();
                    let binding = self
                        .source
                        .as_ref()
                        .map(InputTarget::binding)
                        .unwrap_or_default();
                    self.fields[index]
                        .update(cx, |f, _| f.sync(binding, value.to_string(), window));
                    root = root.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .h(px(25.0))
                            .child(div().w(px(110.0)).child(field.label()))
                            .child(
                                div()
                                    .flex_1()
                                    .when(self.source.is_some(), |d| {
                                        d.child(self.fields[index].clone())
                                    })
                                    .when(self.source.is_none(), |d| d.child(value.to_string())),
                            ),
                    );
                }
                Err(error) => {
                    *self.targets[index].borrow_mut() = None;
                    root = root.child(div().text_color(rgb(ui::MUTED)).child(error));
                }
            }
        }
        if let Some(key) = keys.get(&frame) {
            root = root.child(
                div()
                    .flex()
                    .gap_1()
                    .child(self.button(
                        id,
                        "in-mode",
                        format!("In: {:?}", key.in_interpolation),
                        Operation::Mode(true),
                        true,
                    ))
                    .child(self.button(
                        id,
                        "out-mode",
                        format!("Out: {:?}", key.out_interpolation),
                        Operation::Mode(false),
                        true,
                    )),
            );
        } else {
            root = root.child(
                div()
                    .text_color(rgb(ui::MUTED))
                    .child("Move to a key to edit its easing."),
            );
        }
        root = root.child(self.button(
            id,
            "static",
            "Make static at current value".into(),
            Operation::Collapse,
            true,
        ));
        root.child(div().text_color(rgb(ui::MUTED)).child(
            if layer.has_enabled_expression(libre_effects_core::ExpressionTarget::Opacity) {
                "Value edits the base. Expressions determine the displayed opacity."
            } else {
                "Raw sample shown. Key values: 0–100%; paint clamps overshoot."
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, project_file};
    fn bytes(editor: &Editor) -> Vec<u8> {
        project_file::encode(editor.project(), None).unwrap()
    }
    fn apply(editor: &mut Editor, action: Action) {
        let Action::Edit(command) = action else {
            panic!("expected edit")
        };
        editor.execute(command).unwrap();
    }

    #[test]
    fn unchanged_raw_overshoot_and_tiny_dormant_ease_never_materialize_keys() {
        let editor = crate::opacity_test_support::overshoot_editor(false);
        let before = bytes(&editor);
        let layer = editor.selected_layer().unwrap();
        let raw = Field::Value.value(layer, 15, 1.0 / 30.0).unwrap();
        assert!((raw + 100.0).abs() < 1e-12);
        assert!(
            field_command(layer, 15, 1.0 / 30.0, Field::Value, &raw.to_string())
                .unwrap()
                .is_none()
        );
        for frame in [0, 30] {
            for field in Field::ALL {
                let value = field.value(layer, frame, 1.0 / 30.0).unwrap();
                assert!(
                    field_command(layer, frame, 1.0 / 30.0, field, &value.to_string())
                        .unwrap()
                        .is_none()
                );
            }
        }
        assert_eq!(before, bytes(&editor));
        assert_eq!(layer.opacity_key_count(), 2);
        assert!(field_command(layer, 15, 1.0 / 30.0, Field::InSpeed, "0").is_err());
        for text in ["NaN", "inf", "-101", "101"] {
            assert!(field_command(layer, 15, 1.0 / 30.0, Field::Value, text).is_err());
        }
    }
    #[test]
    fn independent_side_edit_and_mode_roundtrip_keep_other_metadata_and_exact_history() {
        let mut editor = crate::opacity_test_support::overshoot_editor(false);
        let before = bytes(&editor);
        let original = editor
            .selected_layer()
            .unwrap()
            .opacity_timing()
            .unwrap()
            .clone();
        let command = field_command(
            editor.selected_layer().unwrap(),
            0,
            1.0 / 30.0,
            Field::OutInfluence,
            "45",
        )
        .unwrap()
        .unwrap();
        editor.execute(command).unwrap();
        let changed = editor
            .selected_layer()
            .unwrap()
            .opacity_timing()
            .unwrap()
            .keys();
        assert_eq!(changed[&0].in_ease, original.keys()[&0].in_ease);
        assert_eq!(
            changed[&0].out_ease.speed,
            original.keys()[&0].out_ease.speed
        );
        assert_eq!(changed[&0].out_ease.influence, 45.0);
        assert_eq!(changed[&30], original.keys()[&30]);
        let after = bytes(&editor);
        assert_eq!(
            project_file::decode(&after).unwrap().project,
            *editor.project()
        );
        editor.undo();
        assert_eq!(bytes(&editor), before);
        editor.redo();
        assert_eq!(bytes(&editor), after);
        for _ in 0..3 {
            let action = operation(
                editor.selected_layer().unwrap(),
                0,
                1.0 / 30.0,
                Operation::Mode(false),
            )
            .unwrap()
            .unwrap();
            apply(&mut editor, action);
        }
        assert_eq!(
            bytes(&editor),
            after,
            "Bezier/Hold/Linear/Bezier changed dormant ease"
        );
    }
    #[test]
    fn key_add_remove_and_navigation_preserve_existing_timing_and_reject_invalid_holds() {
        let mut editor = crate::opacity_test_support::overshoot_editor(true);
        assert!(
            operation(
                editor.selected_layer().unwrap(),
                15,
                1.0 / 30.0,
                Operation::Add
            )
            .is_err()
        );
        assert!(
            operation(
                editor.selected_layer().unwrap(),
                15,
                1.0 / 30.0,
                Operation::Collapse
            )
            .is_err()
        );
        // Equal endpoint values 50 and signed speeds +100/-100 give
        // 50 + 100*t*(1-t), hence an independently known midpoint of 75.
        for (frame, speed) in [(0, 100.0), (30, -100.0)] {
            let key = editor
                .selected_layer()
                .unwrap()
                .opacity_timing()
                .unwrap()
                .keys()[&frame];
            let (mut incoming, mut outgoing) = (key.in_ease, key.out_ease);
            if frame == 0 {
                outgoing.speed = speed;
            } else {
                incoming.speed = speed;
            }
            editor
                .execute(Command::SetOpacityTiming {
                    id: 1,
                    edit: OpacityEdit::TemporalEase {
                        frame,
                        incoming,
                        outgoing,
                    },
                })
                .unwrap();
        }
        let before = bytes(&editor);
        let original = editor
            .selected_layer()
            .unwrap()
            .opacity_timing()
            .unwrap()
            .clone();
        assert!(matches!(
            operation(
                editor.selected_layer().unwrap(),
                15,
                1.0 / 30.0,
                Operation::Previous
            )
            .unwrap(),
            Some(Action::Seek(0))
        ));
        assert!(matches!(
            operation(
                editor.selected_layer().unwrap(),
                15,
                1.0 / 30.0,
                Operation::Next
            )
            .unwrap(),
            Some(Action::Seek(30))
        ));
        let add = operation(
            editor.selected_layer().unwrap(),
            15,
            1.0 / 30.0,
            Operation::Add,
        )
        .unwrap()
        .unwrap();
        apply(&mut editor, add);
        assert_eq!(
            editor.selected_layer().unwrap().opacity_key_value(15),
            Some(75.0)
        );
        for frame in [0, 30] {
            assert_eq!(
                editor
                    .selected_layer()
                    .unwrap()
                    .opacity_timing()
                    .unwrap()
                    .keys()[&frame],
                original.keys()[&frame]
            );
        }
        let remove = operation(
            editor.selected_layer().unwrap(),
            15,
            1.0 / 30.0,
            Operation::Remove,
        )
        .unwrap()
        .unwrap();
        apply(&mut editor, remove);
        assert_eq!(bytes(&editor), before);
        let command = field_command(
            editor.selected_layer().unwrap(),
            0,
            1.0 / 30.0,
            Field::InSpeed,
            "-1e-300",
        )
        .unwrap()
        .unwrap();
        editor.execute(command).unwrap();
        assert_eq!(
            editor
                .selected_layer()
                .unwrap()
                .opacity_timing()
                .unwrap()
                .keys()[&0]
                .in_ease
                .speed
                .to_bits(),
            (-1e-300_f64).to_bits()
        );
        assert!(
            operation(
                editor.selected_layer().unwrap(),
                30,
                1.0 / 30.0,
                Operation::Mode(true)
            )
            .is_err()
        );
        let layer = editor.selected_layer().unwrap();
        assert!(field_command(layer, 0, 1.0 / 30.0, Field::InInfluence, "0").is_err());
        let hold = operation(
            editor.selected_layer().unwrap(),
            0,
            1.0 / 30.0,
            Operation::Mode(false),
        )
        .unwrap()
        .unwrap();
        apply(&mut editor, hold);
        let hold = operation(
            editor.selected_layer().unwrap(),
            30,
            1.0 / 30.0,
            Operation::Mode(true),
        )
        .unwrap()
        .unwrap();
        apply(&mut editor, hold);
        let held_bytes = bytes(&editor);
        assert!(
            operation(
                editor.selected_layer().unwrap(),
                0,
                1.0 / 30.0,
                Operation::Mode(false)
            )
            .is_err()
        );
        assert_eq!(bytes(&editor), held_bytes);
        let release = operation(
            editor.selected_layer().unwrap(),
            30,
            1.0 / 30.0,
            Operation::Mode(true),
        )
        .unwrap()
        .unwrap();
        apply(&mut editor, release);
        let release = operation(
            editor.selected_layer().unwrap(),
            0,
            1.0 / 30.0,
            Operation::Mode(false),
        )
        .unwrap()
        .unwrap();
        apply(&mut editor, release);
    }
}
