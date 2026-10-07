//! Luma fields and clicks keep the displayed document, composition and frame.
//! Existing effect controls keep their own legacy behavior.
use super::*;
use crate::color_edit::InputTarget;
use libre_effects_core::{EffectInstance, Frame, Layer, LumaKeyMode, PropertyPath};
use std::{cell::RefCell, rc::Rc};

pub(super) fn eligible(layer: &Layer) -> bool {
    !layer.locked() && !matches!(layer.content(), Content::Audio { .. } | Content::Null)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Name,
    Scalar(EffectParam),
}

/// Stored with each TextField, never a reference to a mutable panel-wide target.
#[derive(Clone)]
struct FieldTarget {
    context: InputTarget,
    layer: LayerId,
    effect: EffectId,
    field: Field,
    frame: Frame,
}
impl FieldTarget {
    fn binding(&self) -> String {
        format!(
            "{}-{}-{:?}",
            self.context.binding(),
            self.effect,
            self.field
        )
    }
    fn command(&self, state: &EditorState, text: &str) -> Result<Option<Command>, String> {
        if !self.context.same_context(state) {
            return Ok(None);
        }
        let layer = state
            .editor
            .selected_layer()
            .filter(|l| l.id() == self.layer)
            .ok_or("Select the original Luma Key layer")?;
        field_command(layer, self.effect, self.field, self.frame, text)
    }
}

fn instance(layer: &Layer, effect: EffectId) -> Option<&EffectInstance> {
    eligible(layer).then_some(())?;
    layer
        .effect_stack()
        .iter()
        .find(|e| e.id() == effect && e.kind() == EffectKind::LumaKey)
}

fn field_command(
    layer: &Layer,
    effect: EffectId,
    field: Field,
    frame: Frame,
    text: &str,
) -> Result<Option<Command>, String> {
    let source = instance(layer, effect).ok_or("Select an unlocked Luma Key effect")?;
    let edit = match field {
        Field::Name => {
            if source.name() == text.trim() {
                return Ok(None);
            }
            EffectEdit::Rename {
                effect,
                name: text.into(),
            }
        }
        Field::Scalar(parameter) => {
            if !matches!(
                parameter,
                EffectParam::LumaThreshold | EffectParam::LumaSoftness
            ) {
                return Err("Select a Luma Key parameter".into());
            }
            let value = text
                .trim()
                .parse::<f64>()
                .map_err(|_| "Enter a finite value from 0 to 255")?;
            if !value.is_finite() || !(0.0..=255.0).contains(&value) {
                return Err("Enter a finite value from 0 to 255".into());
            }
            // Let core validate frame/lock/range before its exact no-op path.
            EffectEdit::SetValue {
                effect,
                parameter,
                frame,
                value,
            }
        }
    };
    Ok(Some(Command::Effect {
        id: layer.id(),
        edit,
    }))
}

#[derive(Clone, Copy)]
pub(super) enum Intent {
    Add,
    Mode(LumaKeyMode),
    Watch(EffectParam),
    Key(EffectParam),
    Graph(EffectParam),
    Previous(EffectParam),
    Next(EffectParam),
    Interpolate(EffectParam),
    Bypass,
    Up,
    Down,
    Duplicate,
    Reset,
    Remove,
    Save,
}

/// Replan only after a validated synchronous input flush. Derived choices such
/// as bypass, key state, key interpolation and order must use that resulting state.
fn action(layer: &Layer, effect: EffectId, frame: Frame, intent: Intent) -> Option<Action> {
    if !eligible(layer) {
        return None;
    }
    if matches!(intent, Intent::Add) {
        return Some(Action::Edit(Command::Effect {
            id: layer.id(),
            edit: EffectEdit::Add(EffectKind::LumaKey),
        }));
    }
    let source = instance(layer, effect)?;
    let parameter = match intent {
        Intent::Watch(p)
        | Intent::Key(p)
        | Intent::Graph(p)
        | Intent::Previous(p)
        | Intent::Next(p)
        | Intent::Interpolate(p) => Some(p),
        _ => None,
    };
    if let Some(parameter) = parameter {
        source.parameter(parameter)?;
    }
    let edit = match intent {
        Intent::Add => unreachable!(),
        Intent::Mode(mode) => EffectEdit::SetLumaKeyMode { effect, mode },
        Intent::Watch(parameter) => EffectEdit::ToggleAnimation {
            effect,
            parameter,
            frame,
        },
        Intent::Key(parameter) => EffectEdit::ToggleKey {
            effect,
            parameter,
            frame,
        },
        Intent::Graph(parameter) => {
            return Some(Action::GraphProperty(
                layer.id(),
                PropertyPath::Effect { effect, parameter },
            ));
        }
        Intent::Previous(parameter) => {
            return source
                .parameter(parameter)?
                .keys()
                .range(..frame)
                .next_back()
                .map(|(&at, _)| Action::Seek(at));
        }
        Intent::Next(parameter) => {
            return source
                .parameter(parameter)?
                .keys()
                .range(frame.checked_add(1)?..)
                .next()
                .map(|(&at, _)| Action::Seek(at));
        }
        Intent::Interpolate(parameter) => EffectEdit::Interpolate {
            effect,
            parameter,
            frame,
            interpolation: source
                .parameter(parameter)?
                .keys()
                .get(&frame)?
                .interpolation
                .next(),
        },
        Intent::Bypass => EffectEdit::Bypass {
            effect,
            bypassed: !source.bypassed(),
        },
        Intent::Up | Intent::Down => {
            let index = layer.effect_stack().iter().position(|e| e.id() == effect)?;
            EffectEdit::Move {
                effect,
                index: if matches!(intent, Intent::Up) {
                    index.saturating_sub(1)
                } else {
                    (index + 1).min(layer.effect_stack().len() - 1)
                },
            }
        }
        Intent::Duplicate => EffectEdit::Duplicate(effect),
        Intent::Reset => EffectEdit::Reset(effect),
        Intent::Remove => EffectEdit::Remove(effect),
        Intent::Save => return Some(Action::Preset(PresetAction::Save(Some(effect)))),
    };
    Some(Action::Edit(Command::Effect {
        id: layer.id(),
        edit,
    }))
}

pub(super) fn button(
    button: gpui::Stateful<gpui::Div>,
    control: String,
    state: &Entity<EditorState>,
    target: Option<InputTarget>,
    id: LayerId,
    effect: EffectId,
    intent: Intent,
) -> gpui::Stateful<gpui::Div> {
    let state = state.clone();
    crate::color_edit::input_pointer_button(button, control.clone(), target.clone()).on_click(
        move |event, window, cx| {
            cx.stop_propagation();
            let Some(target) =
                crate::color_edit::input_click_target(&control, event, &target, &state, window, cx)
            else {
                return;
            };
            if state.read(cx).editor.selected() != Some(id) {
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
                let Some(command) = s
                    .editor
                    .selected_layer()
                    .filter(|l| l.id() == id)
                    .and_then(|l| action(l, effect, s.frame, intent))
                else {
                    return;
                };
                s.dispatch(&command, window, cx);
                if matches!(intent, Intent::Add) {
                    s.effect_controls_open = true;
                    cx.notify();
                }
            });
        },
    )
}

struct BoundField {
    field: Entity<TextField>,
    target: Rc<RefCell<Option<FieldTarget>>>,
}
#[derive(Default)]
pub(super) struct Controls {
    source: Option<InputTarget>,
    fields: BTreeMap<(LayerId, EffectId, Field), BoundField>,
}
impl Controls {
    pub(super) fn refresh(&mut self, state: &EditorState) {
        InputTarget::refresh(&mut self.source, state);
        self.fields.retain(|(id, effect, _), _| {
            state
                .editor
                .selected_layer()
                .is_some_and(|l| l.id() == *id && instance(l, *effect).is_some())
        });
    }
    fn field(
        &mut self,
        state: &Entity<EditorState>,
        layer: &Layer,
        effect: &EffectInstance,
        field: Field,
        frame: Frame,
        value: String,
        window: &Window,
        cx: &mut Context<EffectControls>,
    ) -> Entity<TextField> {
        let entry = self
            .fields
            .entry((layer.id(), effect.id(), field))
            .or_insert_with(|| {
                let state = state.clone();
                let target: Rc<RefCell<Option<FieldTarget>>> = Default::default();
                let commit_target = target.clone();
                let input = cx.new(|cx| {
                    let input = TextField::new(cx, move |text, window, cx| {
                        state.update(cx, |s, cx| {
                            let Some(target) = commit_target
                                .borrow()
                                .clone()
                                .filter(|t| t.context.current(s))
                            else {
                                return;
                            };
                            s.finish_text(true, cx);
                            match target.command(s, text) {
                                Ok(Some(command)) => s.dispatch(&Action::Edit(command), window, cx),
                                Ok(None) => {}
                                Err(message) => {
                                    s.status = message;
                                    cx.notify();
                                }
                            }
                        });
                    });
                    if matches!(field, Field::Scalar(_)) {
                        input.numeric()
                    } else {
                        input
                    }
                });
                BoundField {
                    field: input,
                    target,
                }
            });
        let target = self.source.clone().map(|context| FieldTarget {
            context,
            layer: layer.id(),
            effect: effect.id(),
            field,
            frame,
        });
        let binding = target
            .as_ref()
            .map(FieldTarget::binding)
            .unwrap_or_default();
        // Both changes occur together while the row is visible. sync discards
        // a draft when the binding changes; hidden rows keep their old snapshot.
        *entry.target.borrow_mut() = target;
        entry
            .field
            .update(cx, |f, _| f.sync(binding, value, window));
        entry.field.clone()
    }
    pub(super) fn render(
        &mut self,
        state: &Entity<EditorState>,
        layer: &Layer,
        effect: &EffectInstance,
        window: &mut Window,
        cx: &mut Context<EffectControls>,
    ) -> gpui::Div {
        let id = layer.id();
        let effect_id = effect.id();
        let frame = state.read(cx).frame;
        let locked = layer.locked();
        let prefix = format!("effect-{id}-{effect_id}");
        let target = self.source.clone();
        let guarded = |button, control, intent| {
            button_fn(
                button,
                control,
                state,
                target.clone(),
                id,
                effect_id,
                intent,
            )
        };
        let tool = |suffix: &str, icon: &'static str, label: &'static str, intent, active| {
            let control = format!("{prefix}-{suffix}");
            guarded(
                ui::tool(SharedString::from(control.clone()), icon, label, active)
                    .w(px(22.0))
                    .h(px(22.0))
                    .when(locked, |s| s.opacity(0.4)),
                control,
                intent,
            )
        };
        let name = self.field(
            state,
            layer,
            effect,
            Field::Name,
            frame,
            effect.name().into(),
            window,
            cx,
        );
        let mut section = div()
            .border_t_1()
            .border_color(rgb(ui::BORDER))
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(tool(
                        "enabled",
                        "eye",
                        "Enable / bypass effect",
                        Intent::Bypass,
                        !effect.bypassed(),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(!locked, |d| d.child(name))
                            .when(locked, |d| d.child(effect.name().to_string())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(ui::MUTED))
                            .child(effect.kind().label()),
                    )
                    .child(tool("up", "arrow-up", "Move effect up", Intent::Up, false))
                    .child(tool(
                        "down",
                        "arrow-down",
                        "Move effect down",
                        Intent::Down,
                        false,
                    ))
                    .child(tool(
                        "copy",
                        "copy",
                        "Duplicate effect",
                        Intent::Duplicate,
                        false,
                    ))
                    .child(tool(
                        "reset",
                        "arrow-rotate-left",
                        "Reset effect and remove its keys",
                        Intent::Reset,
                        false,
                    ))
                    .child(tool(
                        "remove",
                        "trash-bin",
                        "Remove effect",
                        Intent::Remove,
                        false,
                    )),
            );
        let control = format!("{prefix}-save-preset");
        section = section.child(guarded(
            ui::text_button(SharedString::from(control.clone()), "Save this effect…")
                .when(locked, |d| d.opacity(0.4)),
            control,
            Intent::Save,
        ));
        let mut modes = div()
            .flex()
            .items_center()
            .gap_1()
            .child(div().w(px(38.0)).child("Mode"));
        for mode in LumaKeyMode::ALL {
            let control = format!("{prefix}-mode-{mode:?}");
            modes = modes.child(guarded(
                ui::text_button(SharedString::from(control.clone()), mode.label())
                    .flex_1()
                    .when(effect.luma_key_mode() == Some(mode), |d| {
                        d.bg(rgb(0x164a7b))
                    })
                    .when(locked, |d| d.opacity(0.4)),
                control,
                Intent::Mode(mode),
            ));
        }
        section = section.child(modes);
        for spec in EffectKind::LumaKey.parameters() {
            let parameter = spec.parameter;
            let value = effect.value_at(parameter, frame).to_string();
            let field = self.field(
                state,
                layer,
                effect,
                Field::Scalar(parameter),
                frame,
                value.clone(),
                window,
                cx,
            );
            let track = effect.parameter(parameter).unwrap();
            let param_prefix = format!("{prefix}-{parameter:?}");
            let control = format!("{param_prefix}-graph");
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(tool(
                        &format!("{parameter:?}-watch"),
                        "stopwatch",
                        "Enable animation / remove all parameter keys",
                        Intent::Watch(parameter),
                        !track.keys().is_empty(),
                    ))
                    .child(guarded(
                        ui::text_button(SharedString::from(control.clone()), spec.label)
                            .flex_1()
                            .min_w_0()
                            .justify_start(),
                        control,
                        Intent::Graph(parameter),
                    ))
                    .child(
                        div()
                            .w(px(95.0))
                            .when(!locked, |d| d.child(field))
                            .when(locked, |d| d.child(value)),
                    )
                    .child(tool(
                        &format!("{parameter:?}-key"),
                        "diamond",
                        "Add / remove key at playhead",
                        Intent::Key(parameter),
                        track.keys().contains_key(&frame),
                    )),
            );
            if !track.keys().is_empty() {
                let mut keys = div().flex().items_center().gap_1().pl(px(24.0));
                for (suffix, icon, label, intent) in [
                    (
                        "previous",
                        "arrow-left",
                        "Previous parameter key",
                        Intent::Previous(parameter),
                    ),
                    (
                        "next",
                        "arrow-right",
                        "Next parameter key",
                        Intent::Next(parameter),
                    ),
                ] {
                    if action(layer, effect_id, frame, intent).is_some() {
                        keys = keys.child(tool(
                            &format!("{parameter:?}-{suffix}"),
                            icon,
                            label,
                            intent,
                            false,
                        ));
                    }
                }
                keys = keys.child(
                    div()
                        .flex_1()
                        .text_color(rgb(ui::MUTED))
                        .child(format!("{} keys", track.keys().len())),
                );
                if let Some(key) = track.keys().get(&frame) {
                    let control = format!("{param_prefix}-interpolation");
                    keys = keys.child(guarded(
                        ui::text_button(
                            SharedString::from(control.clone()),
                            key.interpolation.label(),
                        ),
                        control,
                        Intent::Interpolate(parameter),
                    ));
                }
                section = section.child(keys);
            }
        }
        section
    }
}

// Keep the local render closure unambiguous alongside its `button` argument.
use button as button_fn;

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> EditorState {
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state
            .editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::LumaKey),
            })
            .unwrap();
        state
    }
    fn target(state: &EditorState, parameter: EffectParam) -> FieldTarget {
        FieldTarget {
            context: InputTarget::new(state).unwrap(),
            layer: 1,
            effect: 1,
            field: Field::Scalar(parameter),
            frame: state.frame,
        }
    }
    fn accept(state: &mut EditorState, target: &FieldTarget, text: &str) -> bool {
        if !target.context.current(state) {
            return false;
        }
        if let Some(command) = target.command(state, text).unwrap() {
            state.editor.execute(command).unwrap();
        }
        true
    }
    fn run(state: &mut EditorState, effect: EffectId, intent: Intent) {
        let Some(Action::Edit(command)) = action(
            state.editor.selected_layer().unwrap(),
            effect,
            state.frame,
            intent,
        ) else {
            panic!("expected source edit");
        };
        state.editor.execute(command).unwrap();
    }
    fn effect(state: &EditorState, id: EffectId) -> &EffectInstance {
        state
            .editor
            .selected_layer()
            .unwrap()
            .effect_stack()
            .iter()
            .find(|e| e.id() == id)
            .unwrap()
    }

    #[test]
    fn luma_display_is_roundtrip_precise_and_format_only_input_keeps_source_and_redo() {
        for value in [
            0.000_000_000_001_234_567,
            123.456_789_123_456_78,
            254.999_999_999_999_97,
        ] {
            let mut state = scene();
            let input = target(&state, EffectParam::LumaThreshold);
            assert!(accept(&mut state, &input, &value.to_string()));
            let displayed = effect(&state, 1)
                .value_at(EffectParam::LumaThreshold, 0)
                .to_string();
            assert_eq!(displayed.parse::<f64>().unwrap(), value);
            state
                .editor
                .execute(Command::RenameLayer {
                    id: 1,
                    name: "Temporary".into(),
                })
                .unwrap();
            state.editor.undo();
            let before = state.editor.project().to_json().unwrap();
            let input = target(&state, EffectParam::LumaThreshold);
            assert!(accept(&mut state, &input, &format!("  {displayed}  ")));
            assert_eq!(state.editor.project().to_json().unwrap(), before);
            assert!(state.editor.can_redo());
        }
    }

    #[test]
    fn luma_name_format_only_edit_is_elided_before_core_normalization() {
        let state = scene();
        let layer = state.editor.selected_layer().unwrap();
        assert!(
            field_command(layer, 1, Field::Name, 0, "  Luma Key  ")
                .unwrap()
                .is_none()
        );
        assert!(
            field_command(layer, 1, Field::Name, 0, "My Luma")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn luma_field_validation_rejects_nonfinite_range_wrong_effect_parameter_and_locks() {
        let mut state = scene();
        for text in ["", "nan", "NaN", "inf", "-1", "255.0001", "1e100"] {
            assert!(
                target(&state, EffectParam::LumaThreshold)
                    .command(&state, text)
                    .is_err(),
                "{text}"
            );
        }
        let mut input = target(&state, EffectParam::LumaThreshold);
        input.field = Field::Scalar(EffectParam::Radius);
        assert!(input.command(&state, "1").is_err());
        input.effect = 999;
        assert!(input.command(&state, "1").is_err());
        let input = target(&state, EffectParam::LumaThreshold);
        state.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(!accept(&mut state, &input, "64"));
        assert!(InputTarget::new(&state).is_none());
        for intent in [
            Intent::Add,
            Intent::Mode(LumaKeyMode::KeepDarker),
            Intent::Watch(EffectParam::LumaThreshold),
            Intent::Key(EffectParam::LumaSoftness),
            Intent::Duplicate,
            Intent::Reset,
            Intent::Remove,
            Intent::Bypass,
            Intent::Up,
            Intent::Down,
        ] {
            assert!(action(state.editor.selected_layer().unwrap(), 1, 0, intent).is_none());
        }
    }

    #[test]
    fn field_targets_freeze_each_parameter_and_reject_stale_frame_source_document_and_selection() {
        let mut state = scene();
        let threshold = target(&state, EffectParam::LumaThreshold);
        let softness = target(&state, EffectParam::LumaSoftness);
        assert_ne!(threshold.binding(), softness.binding());
        let binding = threshold.binding();
        state.frame = 10;
        assert!(!accept(&mut state, &threshold, "64"));
        let mut panel_source = Some(threshold.context.clone());
        InputTarget::refresh(&mut panel_source, &state);
        assert_eq!(threshold.binding(), binding);
        assert!(panel_source.unwrap().current(&state));
        state.frame = 0;
        state.playing = true;
        assert!(!accept(&mut state, &threshold, "64"));
        state.playing = false;
        state.document_revision += 1;
        assert!(!accept(&mut state, &threshold, "64"));
        state.document_revision -= 1;
        assert!(accept(&mut state, &threshold, "64"));
        assert!(!accept(&mut state, &softness, "20"));
        assert_eq!(
            effect(&state, 1).value_at(EffectParam::LumaSoftness, 0),
            0.0
        );
        let current = target(&state, EffectParam::LumaThreshold);
        state.editor.execute(Command::AddSolid).unwrap();
        assert!(!accept(&mut state, &current, "100"));
        state.editor.select(1);
        assert!(!accept(&mut state, &current, "100"));
        // The shared InputTarget contract intentionally permits an exact source
        // restored by Undo. It never permits a different source with reused IDs.
        state.editor.undo();
        state.editor.select(1);
        assert!(current.context.current(&state));
        state.editor.execute(Command::NewComposition).unwrap();
        state.editor.execute(Command::AddRectangle).unwrap();
        let id = state.editor.selected().unwrap();
        state
            .editor
            .execute(Command::Effect {
                id,
                edit: EffectEdit::Add(EffectKind::LumaKey),
            })
            .unwrap();
        assert!(!accept(&mut state, &current, "100"));
    }

    #[test]
    fn pending_numeric_then_mode_or_watch_commits_old_value_first_and_replans_independent_tracks() {
        for intent in [
            Intent::Mode(LumaKeyMode::KeepDarker),
            Intent::Watch(EffectParam::LumaThreshold),
            Intent::Watch(EffectParam::LumaSoftness),
        ] {
            let mut state = scene();
            let before = state.editor.project().clone();
            let input = target(&state, EffectParam::LumaThreshold);
            assert!(input.context.current(&state));
            assert!(accept(&mut state, &input, "73.123456789"));
            let numeric = state.editor.project().clone();
            assert!(!input.context.current(&state));
            assert!(input.context.same_context(&state));
            // Mirrors input_pointer_root's pre-flush validity and after-flush
            // receipt; keyboard controls use the same synchronous replan.
            run(&mut state, 1, intent);
            assert_eq!(
                effect(&state, 1).value_at(EffectParam::LumaThreshold, 0),
                73.123456789
            );
            assert_eq!(
                effect(&state, 1).value_at(EffectParam::LumaSoftness, 0),
                0.0
            );
            match intent {
                Intent::Mode(mode) => {
                    assert_eq!(effect(&state, 1).luma_key_mode(), Some(mode));
                    assert!(
                        effect(&state, 1)
                            .parameter(EffectParam::LumaThreshold)
                            .unwrap()
                            .keys()
                            .is_empty()
                    );
                }
                Intent::Watch(parameter) => {
                    assert!(
                        effect(&state, 1)
                            .parameter(parameter)
                            .unwrap()
                            .keys()
                            .contains_key(&0)
                    );
                    let other = if parameter == EffectParam::LumaThreshold {
                        EffectParam::LumaSoftness
                    } else {
                        EffectParam::LumaThreshold
                    };
                    assert!(
                        effect(&state, 1)
                            .parameter(other)
                            .unwrap()
                            .keys()
                            .is_empty()
                    );
                }
                _ => unreachable!(),
            }
            state.editor.undo();
            assert_eq!(state.editor.project(), &numeric);
            state.editor.undo();
            assert_eq!(state.editor.project(), &before);
        }
    }

    #[test]
    fn planning_or_discarding_pending_input_does_not_edit_and_valid_blur_belongs_to_old_layer() {
        let mut state = scene();
        let before = state.editor.project().clone();
        let input = target(&state, EffectParam::LumaThreshold);
        let planned = input.command(&state, "64.123456789").unwrap();
        drop(planned); // Cancel/Escape never executes the field callback's plan.
        assert_eq!(state.editor.project(), &before);
        assert!(accept(&mut state, &input, "64.123456789"));
        state.editor.execute(Command::AddSolid).unwrap();
        let old = state.editor.project().composition().layer(1).unwrap();
        assert_eq!(
            old.effect_stack()[0].value_at(EffectParam::LumaThreshold, 0),
            64.123456789
        );
        assert!(
            state
                .editor
                .selected_layer()
                .unwrap()
                .effect_stack()
                .is_empty()
        );
        assert!(!accept(&mut state, &input, "200"));
    }

    #[test]
    fn luma_intents_keep_stable_effect_ids_for_duplicate_reset_bypass_order_and_remove() {
        let mut state = scene();
        state
            .editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::Brightness),
            })
            .unwrap();
        run(&mut state, 1, Intent::Mode(LumaKeyMode::KeepDarker));
        run(&mut state, 1, Intent::Watch(EffectParam::LumaThreshold));
        run(&mut state, 1, Intent::Duplicate);
        let duplicate = state
            .editor
            .selected_layer()
            .unwrap()
            .effect_stack()
            .iter()
            .find(|e| e.kind() == EffectKind::LumaKey && e.id() != 1)
            .unwrap()
            .id();
        assert_eq!(
            effect(&state, duplicate).luma_key_mode(),
            Some(LumaKeyMode::KeepDarker)
        );
        run(&mut state, 1, Intent::Bypass);
        assert!(effect(&state, 1).bypassed());
        assert!(!effect(&state, duplicate).bypassed());
        run(&mut state, 1, Intent::Down);
        assert_eq!(
            state.editor.selected_layer().unwrap().effect_stack()[1].id(),
            1
        );
        run(&mut state, 1, Intent::Up);
        assert_eq!(
            state.editor.selected_layer().unwrap().effect_stack()[0].id(),
            1
        );
        let old = target(&state, EffectParam::LumaThreshold);
        run(&mut state, 1, Intent::Reset);
        assert_eq!(
            effect(&state, 1).luma_key_mode(),
            Some(LumaKeyMode::KeepBrighter)
        );
        assert!(
            effect(&state, 1)
                .parameter(EffectParam::LumaThreshold)
                .unwrap()
                .keys()
                .is_empty()
        );
        assert!(!accept(&mut state, &old, "20"));
        run(&mut state, 1, Intent::Remove);
        assert!(
            action(
                state.editor.selected_layer().unwrap(),
                1,
                0,
                Intent::Key(EffectParam::LumaThreshold)
            )
            .is_none()
        );
        assert_eq!(
            effect(&state, duplicate).luma_key_mode(),
            Some(LumaKeyMode::KeepDarker)
        );
    }

    #[test]
    fn luma_key_graph_and_interpolation_intents_use_two_independent_typed_addresses() {
        let mut state = scene();
        for (parameter, frame) in [
            (EffectParam::LumaThreshold, 7),
            (EffectParam::LumaSoftness, 13),
        ] {
            state.frame = frame;
            run(&mut state, 1, Intent::Key(parameter));
            run(&mut state, 1, Intent::Interpolate(parameter));
            let layer = state.editor.selected_layer().unwrap();
            let Some(Action::GraphProperty(id, property)) =
                action(layer, 1, frame, Intent::Graph(parameter))
            else {
                panic!("missing graph action");
            };
            assert_eq!(id, 1);
            assert_eq!(
                property,
                PropertyPath::Effect {
                    effect: 1,
                    parameter
                }
            );
            assert!(layer.track_paths().contains(&property)); // Timeline discovers these same paths.
            assert!(
                matches!(action(layer, 1, frame + 1, Intent::Previous(parameter)), Some(Action::Seek(at)) if at == frame)
            );
            assert!(
                matches!(action(layer, 1, frame - 1, Intent::Next(parameter)), Some(Action::Seek(at)) if at == frame)
            );
        }
        assert_eq!(
            effect(&state, 1)
                .parameter(EffectParam::LumaThreshold)
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![7]
        );
        assert_eq!(
            effect(&state, 1)
                .parameter(EffectParam::LumaSoftness)
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![13]
        );
    }

    #[test]
    fn new_catalog_luma_eligibility_rejects_audio_null_and_locked_and_allows_adjustments() {
        let audio = Content::Audio {
            path: "fixture.wav".into(),
            audio: libre_effects_core::AudioMetadata {
                stream_index: 0,
                sample_rate: 48000,
                channels: 2,
                channel_layout: "stereo".into(),
                duration: 5.0,
                start_time: 0.0,
                file_offset: 0.0,
            },
            start_frame: 0,
            playback: Default::default(),
        };
        for (content, allowed) in [
            (Content::Rectangle, true),
            (Content::Solid, true),
            (Content::Adjustment, true),
            (Content::Null, false),
            (audio, false),
        ] {
            let mut state = EditorState::default();
            state
                .editor
                .execute(Command::AddContent {
                    content,
                    width: 200.0,
                    height: 120.0,
                    name: "Catalog fixture".into(),
                })
                .unwrap();
            let layer = state.editor.selected_layer().unwrap();
            assert_eq!(eligible(layer), allowed);
            assert_eq!(action(layer, 0, 0, Intent::Add).is_some(), allowed);
            state.editor.execute(Command::ToggleLocked(1)).unwrap();
            assert!(!eligible(state.editor.selected_layer().unwrap()));
        }
    }
}
