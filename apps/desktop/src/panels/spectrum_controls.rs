//! Static native spectrum settings retain the displayed input receipt. Each
//! accepted edit patches current settings after any synchronous field flush.
use super::*;
use crate::color_edit::InputTarget;
use libre_effects_core::{
    AudioSpectrumSettings, EffectInstance, Layer, SpectrumDisplay, SpectrumSide, SpectrumSource,
};
use libre_effects_editor_model::automation_ui::FocusedToggleAction;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Bands,
    StartHz,
    EndHz,
    Duration,
    Offset,
    Height,
    Thickness,
    StartX,
    StartY,
    EndX,
    EndY,
}
impl Field {
    const ALL: [Self; 11] = [
        Self::Bands,
        Self::StartHz,
        Self::EndHz,
        Self::Duration,
        Self::Offset,
        Self::Height,
        Self::Thickness,
        Self::StartX,
        Self::StartY,
        Self::EndX,
        Self::EndY,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Bands => "Bands",
            Self::StartHz => "Start frequency (Hz)",
            Self::EndHz => "End frequency (Hz)",
            Self::Duration => "Duration (ms)",
            Self::Offset => "Offset (ms)",
            Self::Height => "Maximum height",
            Self::Thickness => "Thickness",
            Self::StartX => "Start X",
            Self::StartY => "Start Y",
            Self::EndX => "End X",
            Self::EndY => "End Y",
        }
    }
    fn value(self, settings: &AudioSpectrumSettings) -> String {
        match self {
            Self::Bands => return settings.bands.to_string(),
            Self::StartHz => settings.start_hz,
            Self::EndHz => settings.end_hz,
            Self::Duration => settings.duration_ms,
            Self::Offset => settings.offset_ms,
            Self::Height => settings.maximum_height,
            Self::Thickness => settings.thickness,
            Self::StartX => settings.start[0],
            Self::StartY => settings.start[1],
            Self::EndX => settings.end[0],
            Self::EndY => settings.end[1],
        }
        .to_string()
    }
    fn patch(self, settings: &mut AudioSpectrumSettings, text: &str) -> Result<(), String> {
        let (minimum, maximum) = match self {
            Self::Bands => (1.0, 4096.0),
            Self::StartHz => (0.0, settings.end_hz),
            Self::EndHz => (settings.start_hz, 24_000.0),
            Self::Duration => (1.0, 1000.0),
            Self::Offset => (-86_400_000.0, 86_400_000.0),
            Self::Height => (0.0, 32_768.0),
            Self::Thickness => (0.1, 256.0),
            Self::StartX | Self::StartY | Self::EndX | Self::EndY => (-32_768.0, 32_768.0),
        };
        let error = || {
            format!(
                "{}: enter {} from {minimum} to {maximum}",
                self.label(),
                if self == Self::Bands {
                    "an integer"
                } else {
                    "a finite value"
                }
            )
        };
        let value = text.trim().parse::<f64>().map_err(|_| error())?;
        if !value.is_finite()
            || !(minimum..=maximum).contains(&value)
            || (self == Self::Bands && value.fract() != 0.0)
        {
            return Err(error());
        }
        match self {
            Self::Bands => settings.bands = value as u16,
            Self::StartHz => settings.start_hz = value,
            Self::EndHz => settings.end_hz = value,
            Self::Duration => settings.duration_ms = value,
            Self::Offset => settings.offset_ms = value,
            Self::Height => settings.maximum_height = value,
            Self::Thickness => settings.thickness = value,
            Self::StartX => settings.start[0] = value,
            Self::StartY => settings.start[1] = value,
            Self::EndX => settings.end[0] = value,
            Self::EndY => settings.end[1] = value,
        }
        if settings.start == settings.end {
            return Err("Audio Spectrum start and end points must differ".into());
        }
        Ok(())
    }
}

fn settings(layer: &Layer, effect: EffectId) -> Option<&AudioSpectrumSettings> {
    (!layer.locked()).then_some(())?;
    layer
        .effect_stack()
        .iter()
        .find(|source| source.id() == effect && source.kind() == EffectKind::AudioSpectrum)?
        .audio_spectrum()
}

fn patch_command(
    layer: &Layer,
    effect: EffectId,
    patch: impl FnOnce(&mut AudioSpectrumSettings) -> Result<(), String>,
) -> Result<Option<Command>, String> {
    let current = settings(layer, effect).ok_or("Select an unlocked Audio Spectrum effect")?;
    let mut next = current.clone();
    patch(&mut next)?;
    next.validate()?;
    if next == *current {
        return Ok(None);
    }
    Ok(Some(Command::Effect {
        id: layer.id(),
        edit: EffectEdit::SetAudioSpectrum {
            effect,
            settings: next,
        },
    }))
}

#[derive(Clone)]
struct FieldTarget {
    context: InputTarget,
    layer: LayerId,
    effect: EffectId,
    field: Field,
}
impl FieldTarget {
    fn binding(&self) -> String {
        format!(
            "spectrum-{}-{}-{:?}",
            self.context.binding(),
            self.effect,
            self.field
        )
    }
    /// `current` is checked immediately before flushing; only then may the
    /// current document supply settings to patch under this context receipt.
    fn command(&self, state: &EditorState, text: &str) -> Result<Option<Command>, String> {
        if !self.context.same_context(state) {
            return Ok(None);
        }
        let layer = state
            .editor
            .selected_layer()
            .filter(|layer| layer.id() == self.layer)
            .ok_or("Select the original Audio Spectrum layer")?;
        patch_command(layer, self.effect, |settings| {
            self.field.patch(settings, text)
        })
    }
}

#[derive(Clone, Copy)]
enum Intent {
    Source(Option<LayerId>),
    Display(SpectrumDisplay),
    Side(SpectrumSide),
    Composite,
}
fn action(
    state: &EditorState,
    id: LayerId,
    effect: EffectId,
    intent: Intent,
) -> Result<Option<Command>, String> {
    if state.playing {
        return Ok(None);
    }
    let layer = state
        .editor
        .selected_layer()
        .filter(|layer| layer.id() == id)
        .ok_or("Select the original Audio Spectrum layer")?;
    if let Intent::Source(Some(source)) = intent {
        let project = state.editor.project();
        if !project
            .spectrum_source_layers(project.active_composition_id())?
            .contains(&source)
        {
            return Err("Audio source is unavailable; choose a source layer again".into());
        }
    }
    patch_command(layer, effect, |settings| {
        match intent {
            Intent::Source(source) => {
                settings.source = source.map(|layer| SpectrumSource { layer })
            }
            Intent::Display(display) => settings.display = display,
            Intent::Side(side) => settings.side = side,
            Intent::Composite => settings.composite_original = !settings.composite_original,
        }
        Ok(())
    })
}

fn activate(
    target: InputTarget,
    id: LayerId,
    effect: EffectId,
    intent: Intent,
    state: &Entity<EditorState>,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    if !target.current(state.read(cx)) {
        return;
    }
    TextField::commit_active(window, cx);
    state.update(cx, |state, cx| {
        if !target.same_context(state) {
            return;
        }
        state.finish_text(true, cx);
        if !target.same_context(state) {
            return;
        }
        // The button captures only intent, never the pre-flush settings value.
        match action(state, id, effect, intent) {
            Ok(Some(command)) => state.dispatch(&Action::Edit(command), window, cx),
            Ok(None) => {}
            Err(message) => {
                state.status = message;
                cx.notify();
            }
        }
    });
}

fn button(
    button: gpui::Stateful<gpui::Div>,
    control: String,
    state: &Entity<EditorState>,
    target: Option<InputTarget>,
    id: LayerId,
    effect: EffectId,
    intent: Intent,
) -> gpui::Stateful<gpui::Div> {
    let key_state = state.clone();
    let key_target = target.clone();
    let button = button.capture_key_down(move |event, window, cx| {
        if !matches!(event.keystroke.key.as_str(), "enter" | "space") {
            return;
        }
        let target = key_target
            .as_ref()
            .filter(|target| target.current(key_state.read(cx)))
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
            return; // The first Space during playback still stops transport.
        }
        cx.stop_propagation();
        window.prevent_default();
        if action == FocusedToggleAction::Activate
            && let Some(target) = target
        {
            activate(target, id, effect, intent, &key_state, window, cx);
        }
    });
    let state = state.clone();
    crate::color_edit::input_pointer_button(button, control.clone(), target.clone()).on_click(
        move |event, window, cx| {
            // A key-up synthetic click cannot recover a denied or held press.
            if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                return;
            }
            cx.stop_propagation();
            let Some(target) =
                crate::color_edit::input_click_target(&control, event, &target, &state, window, cx)
            else {
                return;
            };
            activate(target, id, effect, intent, &state, window, cx);
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
                .is_some_and(|layer| layer.id() == *id && settings(layer, *effect).is_some())
        });
    }
    fn field(
        &mut self,
        state: &Entity<EditorState>,
        layer: &Layer,
        effect: &EffectInstance,
        field: Field,
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
                        state.update(cx, |state, cx| {
                            let Some(target) = commit_target
                                .borrow()
                                .clone()
                                .filter(|target| target.context.current(state))
                            else {
                                return;
                            };
                            state.finish_text(true, cx);
                            match target.command(state, text) {
                                Ok(Some(command)) => {
                                    state.dispatch(&Action::Edit(command), window, cx)
                                }
                                Ok(None) => {}
                                Err(message) => {
                                    state.status = message;
                                    cx.notify();
                                }
                            }
                        });
                    });
                    if field == Field::Bands {
                        input.integer()
                    } else {
                        input.numeric()
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
        });
        let binding = target
            .as_ref()
            .map(FieldTarget::binding)
            .unwrap_or_default();
        *entry.target.borrow_mut() = target;
        entry
            .field
            .update(cx, |field, _| field.sync(binding, value, window));
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
        let Some(settings) = effect.audio_spectrum() else {
            return div();
        };
        let id = layer.id();
        let effect_id = effect.id();
        let prefix = format!("spectrum-{id}-{effect_id}");
        let target = self.source.clone();
        let disabled = target.is_none();
        let choice = |suffix: String, label: String, intent, selected| {
            let control = format!("{prefix}-{suffix}");
            button(
                ui::text_button(SharedString::from(control.clone()), label)
                    .flex_none()
                    .when(selected, |button| button.bg(rgb(0x164a7b)))
                    .when(disabled, |button| button.opacity(0.4)),
                control,
                state,
                target.clone(),
                id,
                effect_id,
                intent,
            )
        };
        let project = state.read(cx).editor.project();
        let sources = project.spectrum_source_layers(project.active_composition_id());
        let selected_name = settings.source.and_then(|source| {
            sources
                .as_ref()
                .ok()
                .filter(|sources| sources.contains(&source.layer))?;
            project
                .composition()
                .layer(source.layer)
                .map(|layer| layer.name().to_string())
        });
        let source_label = match (settings.source, selected_name) {
            (None, _) => "Source: None · choose a source below".to_string(),
            (Some(_), Some(name)) => format!("Source: {name}"),
            (Some(_), None) => "Source unavailable · rebind below".to_string(),
        };
        let mut source_list = div()
            .id(SharedString::from(format!("{prefix}-sources")))
            .max_h(px(104.0))
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .child(
                choice(
                    "source-none".into(),
                    "None".into(),
                    Intent::Source(None),
                    settings.source.is_none(),
                )
                .justify_start(),
            );
        match sources {
            Ok(sources) => {
                for source in sources {
                    let Some(layer) = project.composition().layer(source) else {
                        continue;
                    };
                    source_list = source_list.child(
                        choice(
                            format!("source-{source}"),
                            format!("{} · #{source}", layer.name()),
                            Intent::Source(Some(source)),
                            settings
                                .source
                                .is_some_and(|selected| selected.layer == source),
                        )
                        .justify_start()
                        .overflow_hidden(),
                    );
                }
            }
            Err(message) => {
                source_list = source_list.child(div().text_color(rgb(ui::MUTED)).child(message));
            }
        }
        let mut section = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_color(rgb(ui::MUTED))
                    .child("NativeV1 · native, not AE-calibrated"),
            )
            .child(
                div()
                    .text_color(rgb(ui::MUTED))
                    .child("Input: selected layer output"),
            )
            .child(div().child(source_label))
            .child(source_list);
        for field in Field::ALL {
            let value = field.value(settings);
            let input = self.field(state, layer, effect, field, value.clone(), window, cx);
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(field.label()))
                    .child(
                        div()
                            .w(px(104.0))
                            .when(!disabled, |row| row.child(input))
                            .when(disabled, |row| row.text_color(rgb(ui::MUTED)).child(value)),
                    ),
            );
        }
        let mut displays = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_1()
            .child("Display");
        for (display, label) in [
            (SpectrumDisplay::Line, "Line"),
            (SpectrumDisplay::Bars, "Bars"),
            (SpectrumDisplay::Points, "Points"),
        ] {
            displays = displays.child(choice(
                format!("display-{display:?}"),
                label.into(),
                Intent::Display(display),
                settings.display == display,
            ));
        }
        let mut sides = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_1()
            .child("Side");
        for (side, label) in [
            (SpectrumSide::Above, "Above"),
            (SpectrumSide::Below, "Below"),
            (SpectrumSide::Both, "Both"),
        ] {
            sides = sides.child(choice(
                format!("side-{side:?}"),
                label.into(),
                Intent::Side(side),
                settings.side == side,
            ));
        }
        section.child(displays).child(sides).child(
            choice(
                "composite".into(),
                if settings.composite_original {
                    "☑ Composite Original"
                } else {
                    "☐ Composite Original"
                }
                .into(),
                Intent::Composite,
                false,
            )
            .justify_start(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrum_numeric_fields_enforce_integer_finite_and_cross_field_bounds() {
        let initial = AudioSpectrumSettings::default();
        for field in Field::ALL {
            for text in ["", "NaN", "inf", "-inf", "1e100"] {
                assert!(
                    field.patch(&mut initial.clone(), text).is_err(),
                    "{field:?}: {text}"
                );
            }
        }
        for text in ["0", "4097", "1.5", "65536"] {
            assert!(
                Field::Bands.patch(&mut initial.clone(), text).is_err(),
                "{text}"
            );
        }
        for text in ["1", "4096", " 64.0 "] {
            assert!(
                Field::Bands.patch(&mut initial.clone(), text).is_ok(),
                "{text}"
            );
        }
        for (field, accepted, rejected) in [
            (Field::StartHz, "0", "800.1"),
            (Field::EndHz, "24000", "19.9"),
            (Field::Duration, "1000", "0.9"),
            (Field::Offset, "-86400000", "86400001"),
            (Field::Height, "32768", "32769"),
            (Field::Thickness, "0.1", "256.1"),
            (Field::StartX, "-32768", "-32769"),
            (Field::StartY, "32768", "32769"),
            (Field::EndX, "32768", "32769"),
            (Field::EndY, "-32768", "-32769"),
        ] {
            assert!(
                field.patch(&mut initial.clone(), accepted).is_ok(),
                "{field:?}"
            );
            assert!(
                field.patch(&mut initial.clone(), rejected).is_err(),
                "{field:?}"
            );
        }
        assert!(Field::StartX.patch(&mut initial.clone(), "640").is_err());
        for field in Field::ALL {
            let mut next = initial.clone();
            field.patch(&mut next, &field.value(&initial)).unwrap();
            assert_eq!(next, initial);
        }
    }

    #[test]
    fn spectrum_field_and_button_replan_preserve_an_intervening_setting_edit() {
        let mut state = EditorState::default();
        state.editor.execute(Command::AddRectangle).unwrap();
        state
            .editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(EffectKind::AudioSpectrum),
            })
            .unwrap();
        let target = FieldTarget {
            context: InputTarget::new(&state).unwrap(),
            layer: 1,
            effect: 1,
            field: Field::Bands,
        };
        assert!(target.context.current(&state));
        // Simulates the synchronous pending field flush before the button or
        // other field replans. Only same_context may survive that exact flush.
        let command = patch_command(state.editor.selected_layer().unwrap(), 1, |settings| {
            Field::EndHz.patch(settings, "12345.678901234567")
        })
        .unwrap()
        .unwrap();
        state.editor.execute(command).unwrap();
        assert!(!target.context.current(&state));
        assert!(target.context.same_context(&state));
        let expected = settings(state.editor.selected_layer().unwrap(), 1)
            .unwrap()
            .clone();
        let command = target.command(&state, "128").unwrap().unwrap();
        state.editor.execute(command).unwrap();
        let mut expected = AudioSpectrumSettings {
            bands: 128,
            ..expected
        };
        assert_eq!(
            settings(state.editor.selected_layer().unwrap(), 1),
            Some(&expected)
        );
        for intent in [
            Intent::Display(SpectrumDisplay::Points),
            Intent::Side(SpectrumSide::Both),
            Intent::Composite,
        ] {
            let command = action(&state, 1, 1, intent).unwrap().unwrap();
            state.editor.execute(command).unwrap();
            match intent {
                Intent::Display(display) => expected.display = display,
                Intent::Side(side) => expected.side = side,
                Intent::Composite => expected.composite_original = !expected.composite_original,
                Intent::Source(_) => unreachable!(),
            }
            assert_eq!(
                settings(state.editor.selected_layer().unwrap(), 1),
                Some(&expected)
            );
        }
    }
}
