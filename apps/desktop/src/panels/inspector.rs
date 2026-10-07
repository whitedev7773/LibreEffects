use crate::color_edit::InputTarget;
use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::{
    Command, Content, Frame, Layer, Mask, Property, PropertyPath, TextPaint, TextParam, TrackEdit,
};
use std::{cell::RefCell, rc::Rc};

/// Joined Position is one spatial track. Its displayed sample and key count
/// must never be reconstructed from independent scalar X/Y channels.
pub(super) fn joined_position_summary(
    layer: &Layer,
    frame: Frame,
    seconds_per_frame: f64,
) -> Option<(String, usize)> {
    if let Some(planar) = layer.planar_position() {
        let value = match layer.position2_at(frame, seconds_per_frame) {
            Ok([x, y]) => format!("X {x:.2}  Y {y:.2}"),
            Err(error) => format!("Position unavailable: {error}"),
        };
        return Some((value, planar.keys.len()));
    }
    let spatial = layer.spatial_position()?;
    let value = match layer.position3_at(frame, seconds_per_frame) {
        Ok([x, y, z]) => format!("X {x:.2}  Y {y:.2}  Z {z:.2}"),
        Err(error) => format!("Position unavailable: {error}"),
    };
    Some((value, spatial.keys.len()))
}

/// Native timing owns the scalar sample. Keep its raw authored value visible,
/// including overshoot, without exposing the neutral legacy graph underneath.
pub(super) fn native_opacity_summary(
    layer: &Layer,
    frame: Frame,
    seconds_per_frame: f64,
) -> Option<(String, usize)> {
    if !layer.has_opacity_timing() {
        return None;
    }
    let value = match layer.opacity_at(frame, seconds_per_frame) {
        Ok(value) => {
            let prefix =
                if layer.has_enabled_expression(libre_effects_core::ExpressionTarget::Opacity) {
                    "Base raw"
                } else {
                    "Raw"
                };
            let value = if value != 0.0 && (value.abs() < 0.01 || value.abs() >= 1e7) {
                format!("{value:e}")
            } else {
                value.to_string()
            };
            format!("{prefix} {value}%")
        }
        Err(error) => format!("Opacity unavailable: {error}"),
    };
    Some((value, layer.opacity_key_count()))
}

#[cfg(test)]
mod native_opacity_tests {
    use super::*;

    #[test]
    fn native_opacity_summary_reads_raw_source_fps_keys_and_explicit_errors() {
        let mut editor = crate::opacity_test_support::overshoot_editor(false);
        let before = editor.project().clone();
        let layer = editor.selected_layer().unwrap();
        for (seconds_per_frame, expected) in [(1.0 / 30.0, -100.0), (1.0 / 60.0, -25.0)] {
            let (value, keys) = native_opacity_summary(layer, 15, seconds_per_frame).unwrap();
            let raw: f64 = value
                .strip_prefix("Raw ")
                .unwrap()
                .strip_suffix('%')
                .unwrap()
                .parse()
                .unwrap();
            assert!((raw - expected).abs() < 1e-10);
            assert_eq!(keys, 2);
        }
        assert!(
            native_opacity_summary(layer, 15, 0.0)
                .unwrap()
                .0
                .starts_with("Opacity unavailable:")
        );
        assert!(layer.property(Property::Opacity).is_none());
        assert_eq!(editor.project(), &before);
        editor
            .execute(Command::SetExpression {
                id: 1,
                target: libre_effects_core::ExpressionTarget::Opacity,
                source: "value".into(),
                enabled: true,
            })
            .unwrap();
        assert!(
            native_opacity_summary(editor.selected_layer().unwrap(), 15, 1.0 / 30.0)
                .unwrap()
                .0
                .starts_with("Base raw -")
        );
        editor.execute(Command::AddRectangle).unwrap();
        assert!(native_opacity_summary(editor.selected_layer().unwrap(), 0, 1.0 / 30.0).is_none());
    }
}

#[cfg(test)]
mod joined_position_tests {
    use super::*;

    #[test]
    fn ordinary_2d_position_keeps_its_scalar_fields() {
        let mut editor = libre_effects_core::Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let layer = editor.selected_layer().unwrap();
        assert!(joined_position_summary(layer, 0, 1.0 / 30.0).is_none());
        assert!(layer.property(Property::PositionX).is_some());
        assert!(layer.property(Property::PositionY).is_some());
    }

    #[test]
    fn joined_position_displays_all_three_source_coordinates_without_scalar_tracks() {
        let mut editor = libre_effects_core::Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        for (property, value) in [(Property::PositionX, 12.5), (Property::PositionY, -34.25)] {
            editor
                .execute(Command::SetValue {
                    id: 1,
                    property,
                    frame: 0,
                    value,
                })
                .unwrap();
        }
        editor
            .execute(Command::SetThreeD {
                id: 1,
                enabled: true,
            })
            .unwrap();
        let before = editor.project().clone();
        let layer = editor.selected_layer().unwrap();
        assert_eq!(
            joined_position_summary(layer, 17, 1.0 / 30.0),
            Some(("X 12.50  Y -34.25  Z 0.00".into(), 0))
        );
        assert!(layer.property(Property::PositionX).is_none());
        assert!(layer.property(Property::PositionY).is_none());
        assert!(layer.property(Property::Opacity).is_some());
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn joined_position_summary_uses_spatial_keys_and_the_actual_seconds_per_frame() {
        use libre_effects_core::{SpatialEase, SpatialEdit, SpatialInterpolation};

        let mut editor = libre_effects_core::Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor
            .execute(Command::SetThreeD {
                id: 1,
                enabled: true,
            })
            .unwrap();
        for edit in [
            SpatialEdit::Key {
                frame: 0,
                value: [0.0; 3],
            },
            SpatialEdit::Key {
                frame: 30,
                value: [0.0, 0.0, 12.0],
            },
            SpatialEdit::Interpolation {
                frame: 0,
                incoming: SpatialInterpolation::Linear,
                outgoing: SpatialInterpolation::Bezier,
            },
            SpatialEdit::TemporalEase {
                frame: 0,
                incoming: SpatialEase::default(),
                outgoing: SpatialEase {
                    speed: 4.0,
                    influence: 100.0 / 3.0,
                },
            },
        ] {
            editor
                .execute(Command::SetSpatialPosition { id: 1, edit })
                .unwrap();
        }
        let layer = editor.selected_layer().unwrap();
        assert_eq!(
            joined_position_summary(layer, 15, 1.0 / 30.0),
            Some(("X 0.00  Y 0.00  Z 5.00".into(), 2))
        );
        assert_eq!(
            joined_position_summary(layer, 15, 1.0 / 60.0),
            Some(("X 0.00  Y 0.00  Z 4.75".into(), 2))
        );
        assert!(
            joined_position_summary(layer, 15, 0.0)
                .unwrap()
                .0
                .starts_with("Position unavailable:")
        );
    }
}

fn text_field_command(
    layer: &Layer,
    frame: Frame,
    index: usize,
    value: &str,
) -> Result<Option<Command>, String> {
    if layer.locked() {
        return Err("Unlock the text layer before editing".into());
    }
    let Some(text) = layer.source_text_at(frame) else {
        return Err("Select a text layer".into());
    };
    match index {
        // Compare the displayed sample before interning. Unchanged input at an
        // unkeyed frame must not materialize a key or change independent styling.
        0 => Ok((text != value).then(|| Command::EditSourceText {
            id: layer.id(),
            frame,
            text: value.into(),
        })),
        1 | 11 | 12 => layer.text_value_command(
            text_field_parameter(index).unwrap(),
            value
                .trim()
                .parse::<f64>()
                .map_err(|_| "Enter a finite number")?,
            frame,
        ),
        2 => crate::color_edit::text_hex_command(layer, TextPaint::Fill, frame, value),
        _ => Err("Unknown text field".into()),
    }
}

fn text_field_parameter(index: usize) -> Option<TextParam> {
    match index {
        1 => Some(TextParam::FontSize),
        11 => Some(TextParam::FillOpacity),
        12 => Some(TextParam::StrokeOpacity),
        _ => None,
    }
}

pub(crate) struct Inspector {
    matte: Option<(
        u64,
        Entity<super::matte::MattePicker>,
        Entity<super::matte::MattePicker>,
    )>,
    blend: Option<(u64, Entity<super::blend::BlendPicker>)>,
    state: Entity<EditorState>,
    name: Entity<TextField>,
    fields: Vec<Entity<TextField>>,
    range: Vec<Entity<TextField>>,
    parent_open: bool,
    parent_owner: Option<u64>,
    extra: Vec<Entity<TextField>>,
    extra_source: Option<InputTarget>,
    extra_targets: Vec<Rc<RefCell<Option<InputTarget>>>>,
    playback: Vec<Entity<TextField>>,
    audio_controls: Entity<super::audio_controls::AudioControls>,
    text_animator: Entity<super::text_animator::TextAnimator>,
    shape_controls: Entity<super::shape_controls::ShapeControls>,
    contents_controls: Entity<super::contents::ContentsControls>,
    mask_values: Entity<super::mask_values::MaskValues>,
    native_opacity: Entity<super::native_opacity::Controls>,
}
impl Inspector {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let edit = state.clone();
        let name = cx.new(|cx| {
            TextField::new(cx, move |text, window, cx| {
                edit.update(cx, |state, cx| {
                    if let Some(id) = state.editor.selected() {
                        state.dispatch(
                            &Action::Edit(Command::RenameLayer {
                                id,
                                name: text.into(),
                            }),
                            window,
                            cx,
                        );
                    }
                });
            })
        });
        let fields = Property::ALL
            .into_iter()
            .map(|property| {
                let edit = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |state, cx| {
                            if let Some(id) = state.editor.selected() {
                                if state
                                    .editor
                                    .selected_layer()
                                    .and_then(|layer| layer.property(property))
                                    .is_none()
                                {
                                    state.status = if property == Property::Opacity {
                                        "Native Opacity timing is read-only in Properties."
                                    } else {
                                        "Joined Position is read-only in Properties."
                                    }
                                    .into();
                                    cx.notify();
                                    return;
                                }
                                match text.trim().parse::<f64>() {
                                    Ok(value) => state.dispatch(
                                        &Action::Edit(Command::SetValue {
                                            id,
                                            property,
                                            frame: state.frame,
                                            value,
                                        }),
                                        window,
                                        cx,
                                    ),
                                    Err(_) => {
                                        state.status =
                                            "Enter a finite number for the property.".into();
                                        cx.notify();
                                    }
                                }
                            }
                        });
                    })
                })
            })
            .collect();
        let range = (0..2)
            .map(|index| {
                let edit = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |state, cx| {
                            if let Some(layer) = state.editor.selected_layer() {
                                let Ok(value) = text.trim().parse::<u32>() else {
                                    state.status = "Enter a whole frame number.".into();
                                    cx.notify();
                                    return;
                                };
                                let duration = state.editor.project().composition().duration();
                                let command = Command::SetLayerRange {
                                    id: layer.id(),
                                    start: if index == 0 { value } else { layer.in_frame() },
                                    end: if index == 1 {
                                        value
                                    } else {
                                        layer.out_frame(duration)
                                    },
                                };
                                state.dispatch(&Action::Edit(command), window, cx);
                            }
                        });
                    })
                })
            })
            .collect();
        let playback = (0..3)
            .map(|index| {
                let edit = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |s, cx| {
                            let Some(id) = s.editor.selected() else {
                                return;
                            };
                            let Ok(value) = text.trim().parse::<f64>() else {
                                s.status = "Enter a number for video playback".into();
                                cx.notify();
                                return;
                            };
                            let command = if index == 0 {
                                Command::SetVideoSpeed {
                                    id,
                                    speed: value / 100.0,
                                }
                            } else if index == 1 {
                                Command::SetVideoSourceIn { id, seconds: value }
                            } else {
                                Command::EditTrack {
                                    id,
                                    property: PropertyPath::TimeRemap,
                                    edit: TrackEdit::Value {
                                        frame: s.frame,
                                        value,
                                    },
                                }
                            };
                            s.dispatch(&Action::Edit(command), window, cx);
                        });
                    })
                    .numeric()
                })
            })
            .collect();
        let extra_targets: Vec<Rc<RefCell<Option<InputTarget>>>> =
            (0..13).map(|_| Default::default()).collect();
        let extra = (0..13)
            .map(|index| {
                let edit = state.clone();
                let target = extra_targets[index].clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |s, cx| {
                            if !target.borrow().as_ref().is_some_and(|t| t.current(s)) {
                                return;
                            }
                            s.finish_text(true, cx);
                            if !target.borrow().as_ref().is_some_and(|t| t.same_context(s)) {
                                return;
                            }
                            let Some(l) = s.editor.selected_layer() else {
                                return;
                            };
                            let id = l.id();
                            if (index <= 2 || text_field_parameter(index).is_some())
                                && matches!(l.content(), Content::Text { .. })
                            {
                                match text_field_command(l, s.frame, index, text) {
                                    Ok(Some(command)) => {
                                        s.dispatch(&Action::Edit(command), window, cx)
                                    }
                                    Ok(None) => {}
                                    Err(error) => {
                                        s.status = error;
                                        cx.notify();
                                    }
                                }
                                return;
                            }
                            let number = text.parse::<f64>().ok();
                            let command = match index {
                                0 | 1 => None,
                                2 => u32::from_str_radix(text.trim().trim_start_matches('#'), 16)
                                    .ok()
                                    .and_then(|color| {
                                        if matches!(l.content(), Content::Shape(_)) {
                                            l.shape_color_command(
                                                libre_effects_core::ShapePaint::Fill,
                                                color,
                                                s.frame,
                                            )
                                            .ok()
                                        } else {
                                            Some(Command::SetColor { id, color })
                                        }
                                    }),
                                3 | 4 => number.map(|v| {
                                    let mut effects = l.effects();
                                    if index == 3 {
                                        effects.blur = v;
                                    } else {
                                        effects.brightness = v;
                                    }
                                    Command::SetEffects { id, effects }
                                }),
                                9 | 10 => text.trim().parse::<u32>().ok().map(|value| {
                                    Command::ConfigureSolid {
                                        id,
                                        width: if index == 9 { value } else { l.width() as u32 },
                                        height: if index == 10 {
                                            value
                                        } else {
                                            l.height() as u32
                                        },
                                        color: l.color(),
                                    }
                                }),
                                _ => l.mask().zip(number).map(|(mut m, v)| {
                                    match index {
                                        5 => m.x = v,
                                        6 => m.y = v,
                                        7 => m.width = v,
                                        _ => m.height = v,
                                    };
                                    Command::SetMask { id, mask: Some(m) }
                                }),
                            };
                            if let Some(c) = command {
                                s.dispatch(&Action::Edit(c), window, cx);
                            } else {
                                s.status = "Invalid value".into();
                                cx.notify();
                            }
                        })
                    })
                })
            })
            .collect();
        let audio_controls =
            cx.new(|cx| super::audio_controls::AudioControls::new(state.clone(), cx));
        let text_animator = cx.new(|cx| super::text_animator::TextAnimator::new(state.clone(), cx));
        let mask_values = cx.new(|cx| super::mask_values::MaskValues::new(state.clone(), cx));
        let native_opacity = cx.new(|cx| super::native_opacity::Controls::new(state.clone(), cx));
        let shape_controls =
            cx.new(|cx| super::shape_controls::ShapeControls::new(state.clone(), cx));
        let contents_controls =
            cx.new(|cx| super::contents::ContentsControls::new(state.clone(), cx));
        Self {
            blend: None,
            matte: None,
            extra,
            extra_source: None,
            extra_targets,
            state,
            name,
            fields,
            range,
            parent_open: false,
            parent_owner: None,
            playback,
            audio_controls,
            text_animator,
            shape_controls,
            contents_controls,
            mask_values,
            native_opacity,
        }
    }
}
impl Render for Inspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        InputTarget::refresh(&mut self.extra_source, state);
        let extra_binding = self
            .extra_source
            .as_ref()
            .map(InputTarget::binding)
            .unwrap_or_default();
        let frame = state.frame;
        let duration = state.editor.project().composition().duration();
        let selected = state.editor.selected_layer().cloned();
        let comp = state.editor.project().composition().clone();
        let tree_scroll_owner = self.contents_controls.clone();
        let mut panel = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .bg(rgb(ui::BG))
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let tree = tree_scroll_owner.clone();
                        let visible = bounds.intersect(&window.content_mask().bounds);
                        window.on_mouse_event(
                            move |event: &gpui::ScrollWheelEvent, phase, _, cx| {
                                if phase.capture() && visible.contains(&event.position) {
                                    tree.update(cx, |this, cx| this.invalidate_tree_layout(cx));
                                }
                            },
                        );
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        let Some(layer) = selected else {
            return panel.child(
                div()
                    .p_4()
                    .text_color(rgb(ui::MUTED))
                    .child("Select a layer to view its properties."),
            );
        };
        let id = layer.id();
        if self.parent_owner != Some(id) {
            self.parent_open = false;
            self.parent_owner = Some(id);
        }
        let locked = layer.locked();
        let is_audio = matches!(layer.content(), Content::Audio { .. });
        self.name.update(cx, |field, _| {
            field.sync(id.to_string(), layer.name().to_string(), window)
        });
        let mut contents = div()
            .id("properties-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_3()
            .py_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_3()
                    .child(ui::icon("square"))
                    .child(
                        div()
                            .flex_1()
                            .when(!locked, |s| s.child(self.name.clone()))
                            .when(locked, |s| s.child(layer.name().to_string())),
                    ),
            )
            .when(layer.can_audio(), |d| d.child(self.audio_controls.clone()))
            .when(!is_audio, |d| {
                d.child(
                    div()
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_b_1()
                        .border_color(rgb(0x353535))
                        .child(ui::icon("chevron-down"))
                        .child("Transform"),
                )
            });
        for (label, indices) in [
            ("Anchor Point", vec![2, 3]),
            ("Position", vec![0, 1]),
            ("Scale", vec![4, 5]),
            ("Rotation", vec![6]),
            ("Opacity", vec![7]),
        ] {
            if is_audio {
                continue;
            }
            if label == "Position"
                && let Some((value, keys)) =
                    joined_position_summary(&layer, frame, comp.fps().seconds(1))
            {
                contents = contents.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .py_1()
                        .text_size(px(11.0))
                        .child(format!(
                            "Position {} · joined · {keys} keys · read-only",
                            if layer.is_three_d() { "XYZ" } else { "XY" }
                        ))
                        .child(div().text_color(rgb(ui::MUTED)).child(value))
                        .when(!layer.is_three_d(), |row| {
                            row.child(super::expression_editor::entry_button(
                                &self.state,
                                &layer,
                                libre_effects_core::ExpressionTarget::Position,
                            ))
                        })
                        .child(
                            div()
                                .text_color(rgb(ui::MUTED))
                                .child("Edit spatial geometry through scripting."),
                        ),
                );
                continue;
            }
            if label == "Opacity" && layer.has_opacity_timing() {
                contents = contents.child(self.native_opacity.clone());
                continue;
            }
            let properties: Vec<_> = indices.iter().map(|i| Property::ALL[*i]).collect();
            let expression_driven = properties.iter().any(|property| {
                libre_effects_core::ExpressionTarget::from_property(*property)
                    .is_some_and(|target| layer.has_enabled_expression(target))
            });
            let animated = properties.iter().any(|p| {
                layer
                    .property(*p)
                    .is_some_and(|track| !track.keys().is_empty())
            });
            let command = Command::Batch(
                properties
                    .iter()
                    .filter(|p| {
                        layer
                            .property(**p)
                            .is_some_and(|track| track.keys().is_empty() == !animated)
                    })
                    .map(|p| Command::ToggleAnimation {
                        id,
                        property: *p,
                        frame,
                    })
                    .collect(),
            );
            let mut row = div()
                .flex()
                .items_center()
                .h(px(30.0))
                .gap_1()
                .child(ui::action_tool(
                    gpui::SharedString::from(format!("watch-{label}")),
                    "stopwatch",
                    "Toggle animation",
                    &self.state,
                    Action::Edit(command),
                    animated,
                ))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.0))
                        .child(if expression_driven {
                            format!("{label} · base")
                        } else {
                            label.to_string()
                        }),
                );
            for index in indices {
                let property = Property::ALL[index];
                let Some(track) = layer.property(property) else {
                    continue;
                };
                let value = track.value_at(frame);
                self.fields[index].update(cx, |field, _| {
                    field.set_numeric();
                    field.sync(format!("{id}-{frame}"), format!("{value:.2}"), window);
                });
                row = row.child(
                    div()
                        .w(px(62.0))
                        .when(!locked, |s| s.child(self.fields[index].clone()))
                        .when(locked, |s| s.child(format!("{value:.2}"))),
                );
            }
            if !layer.is_three_d()
                && let Some(target) = properties
                    .first()
                    .copied()
                    .and_then(libre_effects_core::ExpressionTarget::from_property)
            {
                row = row.child(super::expression_editor::entry_button(
                    &self.state,
                    &layer,
                    target,
                ));
            }
            contents = contents.child(row);
        }
        if layer.expressions().iter().any(|program| program.enabled) {
            contents=contents.child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child("Expressions enabled · numeric fields edit authored base values; the Composition shows evaluated results."));
        }
        let is_null = matches!(layer.content(), Content::Null);
        if !is_null && !is_audio {
            if self.blend.as_ref().is_none_or(|(owner, _)| *owner != id) {
                self.blend = Some((
                    id,
                    cx.new(|cx| super::blend::BlendPicker::new(self.state.clone(), id, cx)),
                ));
            }
            contents = contents.child(
                div()
                    .flex()
                    .items_center()
                    .mt_1()
                    .child(div().w(px(105.0)).child("Blend mode"))
                    .child(div().flex_1().child(self.blend.as_ref().unwrap().1.clone())),
            );
        }
        if !is_null && !is_audio {
            if self.matte.as_ref().is_none_or(|(owner, _, _)| *owner != id) {
                self.matte = Some((
                    id,
                    cx.new(|cx| super::matte::MattePicker::new(self.state.clone(), id, true, cx)),
                    cx.new(|cx| super::matte::MattePicker::new(self.state.clone(), id, false, cx)),
                ));
            }
            let (_, source, mode) = self.matte.as_ref().unwrap();
            for (label, picker) in [
                ("Track Matte", source.clone()),
                ("Matte mode", mode.clone()),
            ] {
                contents = contents.child(
                    div()
                        .flex()
                        .items_center()
                        .mt_1()
                        .child(div().w(px(105.0)).child(label))
                        .child(div().flex_1().min_w_0().child(picker)),
                );
            }
        }
        let is_adjustment = matches!(layer.content(), Content::Adjustment);
        let fill_color = match layer.content() {
            Content::Shape(shape) => {
                shape.paint_color_at(libre_effects_core::ShapePaint::Fill, layer.color(), frame)
            }
            Content::Text { .. } => layer.text_color_at(TextPaint::Fill, frame).unwrap(),
            _ => layer.color(),
        };
        let mut entries = vec![(2, "Fill (hex)", format!("{fill_color:06X}"))];
        if let Some(text) = layer.source_text_at(frame) {
            entries.insert(0, (0, "Source Text", text.to_owned()));
            entries.insert(
                1,
                (
                    1,
                    "Font size (px)",
                    layer
                        .text_value_at(TextParam::FontSize, frame)
                        .unwrap()
                        .to_string(),
                ),
            );
        }
        if matches!(layer.content(), Content::Text { .. }) {
            entries.extend([
                (
                    11,
                    "Fill opacity (%)",
                    layer
                        .text_value_at(TextParam::FillOpacity, frame)
                        .unwrap()
                        .to_string(),
                ),
                (
                    12,
                    "Stroke opacity (%)",
                    layer
                        .text_value_at(TextParam::StrokeOpacity, frame)
                        .unwrap()
                        .to_string(),
                ),
            ]);
        }
        if matches!(
            layer.content(),
            Content::Image { .. }
                | Content::Audio { .. }
                | Content::Video { .. }
                | Content::ImageSequence { .. }
                | Content::Composition { .. }
                | Content::Adjustment
                | Content::ShapeContents(_)
        ) {
            entries.retain(|(index, _, _)| *index != 2);
        }
        if is_null {
            entries.clear();
        }
        if matches!(layer.content(), Content::Solid | Content::Adjustment) {
            entries.push((9, "Source width", layer.width().to_string()));
            entries.push((10, "Source height", layer.height().to_string()));
        }
        contents = contents.child(
            div()
                .mt_3()
                .py_2()
                .border_t_1()
                .border_color(rgb(ui::BORDER))
                .child(if is_null {
                    "Null object · transform controller"
                } else if is_adjustment {
                    "Adjustment · composite below"
                } else if matches!(layer.content(), Content::Solid) {
                    "Solid settings"
                } else {
                    "Content"
                }),
        );
        if layer.source_text_at(frame).is_some() {
            contents = contents.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child("Source Text expression")
                    .child(super::expression_editor::entry_button(
                        &self.state,
                        &layer,
                        libre_effects_core::ExpressionTarget::SourceText,
                    )),
            );
        }
        contents = contents.child(self.contents_controls.clone());
        if matches!(layer.content(), Content::Shape(_)) {
            contents = contents.child(self.shape_controls.clone());
        }
        contents = contents.child(self.mask_values.clone());
        if let Content::Composition { composition, .. } = layer.content() {
            let source = *composition;
            let state = self.state.clone();
            contents = contents.child(
                ui::text_button("open-source-composition", "Open source composition").on_click(
                    move |_, window, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(&Action::ActivateComposition(source), window, cx)
                        })
                    },
                ),
            );
        }
        if layer.can_time_remap() {
            let remapped = layer.time_remap().is_some();
            contents = contents.child(div().mt_2().child("Time")).child(
                ui::text_button(
                    "time-remap-enable",
                    if remapped {
                        "Disable Time Remapping"
                    } else {
                        "Enable Time Remapping"
                    },
                )
                .when(!locked, |b| {
                    b.on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(&Action::ToggleTimeRemap, window, cx)
                            })
                        }
                    })
                }),
            );
            if remapped {
                let value = format!("{:.12}", layer.source_time(frame, comp.fps()).unwrap());
                self.playback[2].update(cx, |f, _| {
                    f.sync(format!("{id}-{frame}"), value.clone(), window)
                });
                contents = contents
                    .child(
                        div()
                            .flex()
                            .h(px(29.0))
                            .items_center()
                            .child(ui::action_tool(
                                "time-remap-key",
                                "diamond",
                                "Add or remove source time key",
                                &self.state,
                                Action::Edit(Command::EditTrack {
                                    id,
                                    property: PropertyPath::TimeRemap,
                                    edit: TrackEdit::ToggleKey { frame },
                                }),
                                layer.time_remap().unwrap().keys().contains_key(&frame),
                            ))
                            .child(div().w(px(88.0)).child("Source time (s)"))
                            .child(
                                div()
                                    .flex_1()
                                    .when(!locked, |d| d.child(self.playback[2].clone()))
                                    .when(locked, |d| d.child(value)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(ui::text_button("time-remap-graph", "Edit graph").on_click({
                                let state = self.state.clone();
                                move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::GraphProperty(id, PropertyPath::TimeRemap),
                                            window,
                                            cx,
                                        )
                                    })
                                }
                            }))
                            .when(!locked, |d| {
                                d.child(
                                    ui::text_button("time-remap-freeze", "Freeze frame").on_click(
                                        {
                                            let state = self.state.clone();
                                            move |_, window, cx| {
                                                state.update(cx, |s, cx| {
                                                    s.dispatch(&Action::FreezeTimeRemap, window, cx)
                                                })
                                            }
                                        },
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child("Source seconds · out of range is transparent"),
                    );
            }
        }
        if let Content::Video { playback, .. }
        | Content::Audio { playback, .. }
        | Content::ImageSequence { playback, .. } = layer.content()
        {
            let path = layer.content().linked_paths()[0].clone();
            let relink = if matches!(layer.content(), Content::ImageSequence { .. }) {
                Action::RelinkSequence(layer.asset_id().unwrap())
            } else if matches!(layer.content(), Content::Audio { .. }) {
                Action::RelinkSource(path.clone())
            } else {
                Action::RelinkVideo
            };
            let duration = layer
                .footage_interpretation()
                .duration(layer.content())
                .unwrap();
            contents = contents
                .child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(
                    match layer.content() {
                        Content::ImageSequence { frames, .. } => {
                            format!("Image sequence · {} frames · {duration:.2}s", frames.len())
                        }
                        Content::Audio { audio, .. } => format!(
                            "Audio · {} Hz · {} ch · {duration:.2}s",
                            audio.sample_rate, audio.channels
                        ),
                        Content::Video {
                            audio: Some(audio), ..
                        } => format!(
                            "Video + audio · {} Hz · {} ch",
                            audio.sample_rate, audio.channels
                        ),
                        _ => format!("Linked video · {duration:.2}s · no audio"),
                    },
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .max_h(px(48.0))
                        .overflow_hidden()
                        .child(path.clone()),
                )
                .child(ui::text_button("relink-video", "Relink source…").on_click({
                    let state = self.state.clone();
                    move |_, window, cx| {
                        state.update(cx, |s, cx| s.dispatch(&relink, window, cx));
                    }
                }));
            if layer.time_remap().is_none() {
                for (index, label, value) in [
                    (0, "Speed (%)", format!("{:.2}", playback.speed * 100.0)),
                    (
                        1,
                        "Source In (s)",
                        format!(
                            "{:.6}",
                            layer
                                .content()
                                .video_source_time(layer.in_frame(), comp.fps())
                                .unwrap()
                        ),
                    ),
                ] {
                    self.playback[index].update(cx, |field, _| {
                        field.sync(format!("{id}-{}", layer.in_frame()), value.clone(), window);
                    });
                    contents = contents.child(
                        div()
                            .flex()
                            .h(px(29.0))
                            .items_center()
                            .child(div().w(px(105.0)).child(label))
                            .child(
                                div()
                                    .flex_1()
                                    .when(!locked, |s| s.child(self.playback[index].clone()))
                                    .when(locked, |s| s.child(value)),
                            ),
                    );
                }
                contents = contents
                    .child(div().flex().gap_1().when(!locked, |row| {
                        row.child(ui::text_button("reverse-video", "Reverse").on_click({
                            let state = self.state.clone();
                            move |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::ReverseVideo { id }),
                                        window,
                                        cx,
                                    )
                                })
                            }
                        }))
                        .child(
                            ui::text_button("freeze-video", "Freeze at playhead").on_click({
                                let state = self.state.clone();
                                move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::Edit(Command::FreezeVideo {
                                                id,
                                                frame: s.frame,
                                            }),
                                            window,
                                            cx,
                                        )
                                    })
                                }
                            }),
                        )
                    }))
                    .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
                        "0% freezes · negative speed reverses. Layer range and keys stay fixed.",
                    ));
            }
            if layer.content().audio().is_some() {
                contents =
                    contents.child(div().text_size(px(11.0)).text_color(rgb(ui::MUTED)).child(
                        "Audio exports as 48 kHz stereo · preview playback is not yet available",
                    ));
            }
            let source_status =
                if frame < layer.in_frame() || frame >= layer.out_frame(comp.duration()) {
                    "Playhead is outside the layer".to_string()
                } else {
                    layer
                        .video_time(frame, comp.fps())
                        .map(|seconds| format!("Source now: {seconds:.3}s"))
                        .unwrap_or_else(|| "Outside source · transparent frame".into())
                };
            contents = contents.child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(rgb(ui::MUTED))
                    .child(source_status),
            );
        }
        for (index, label, value) in entries {
            *self.extra_targets[index].borrow_mut() = self.extra_source.clone();
            self.extra[index].update(cx, |field, _| {
                if index != 0 && index != 2 {
                    field.set_numeric();
                }
                field.sync(extra_binding.clone(), value.clone(), window);
            });
            contents = contents.child(
                div()
                    .flex()
                    .h(px(29.0))
                    .items_center()
                    .child(
                        div()
                            .w(px(if index == 0 || index == 11 || index == 12 {
                                155.0
                            } else {
                                105.0
                            }))
                            .flex()
                            .items_center()
                            .when(index == 0 && !locked, |d| {
                                d.child(super::timeline::source_text_control(
                                    &self.state,
                                    &layer,
                                    frame,
                                    super::timeline::SourceTextControl::Animation,
                                    self.extra_source.clone(),
                                    "inspector",
                                ))
                            })
                            .when(
                                text_field_parameter(index).is_some()
                                    && !locked
                                    && matches!(layer.content(), Content::Text { .. }),
                                |d| {
                                    d.child(super::character::scalar_watch(
                                        &self.state,
                                        &layer,
                                        text_field_parameter(index).unwrap(),
                                        self.extra_source.clone(),
                                        "inspector",
                                    ))
                                },
                            )
                            .when(index == 2, |d| {
                                if let Content::Shape(shape) = layer.content() {
                                    d.child(super::shape_values::color_watch(
                                        &self.state,
                                        shape,
                                        id,
                                        libre_effects_core::ShapePaint::Fill,
                                        frame,
                                    ))
                                } else if matches!(layer.content(), Content::Text { .. }) && !locked
                                {
                                    d.child(super::character::color_watch(
                                        &self.state,
                                        &layer,
                                        TextPaint::Fill,
                                        self.extra_source.clone(),
                                        "inspector",
                                    ))
                                } else {
                                    d
                                }
                            })
                            .child(label),
                    )
                    .when(
                        index == 2 && matches!(layer.content(), Content::Text { .. }),
                        |d| {
                            d.child(super::color_picker::text_swatch(
                                "layer-text-fill-color",
                                fill_color,
                                id,
                                TextPaint::Fill,
                                self.extra_source.clone(),
                                &self.state,
                            ))
                        },
                    )
                    .when(
                        index == 2 && !matches!(layer.content(), Content::Text { .. }),
                        |d| {
                            d.child(super::color_picker::swatch(
                                "layer-fill-color",
                                fill_color,
                                if matches!(layer.content(), Content::Shape(_)) {
                                    crate::color_edit::Target::Shape(
                                        id,
                                        libre_effects_core::ShapePaint::Fill,
                                    )
                                } else {
                                    crate::color_edit::Target::Fill(id)
                                },
                                locked,
                                &self.state,
                            ))
                        },
                    )
                    .child(
                        div()
                            .flex_1()
                            .when(!locked, |s| s.child(self.extra[index].clone()))
                            .when(locked, |s| s.child(value)),
                    ),
            );
        }
        if matches!(layer.content(), Content::Text { .. }) {
            contents = contents.child(self.text_animator.clone());
        }
        if matches!(layer.content(), Content::Solid | Content::Adjustment) {
            let state = self.state.clone();
            let command = Command::ConfigureSolid {
                id,
                width: comp.width(),
                height: comp.height(),
                color: layer.color(),
            };
            contents = contents
                .child(
                    ui::text_button("solid-comp-size", "Make comp size").when(!locked, |s| {
                        s.on_click(move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(&Action::Edit(command.clone()), window, cx)
                            });
                        })
                    }),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(ui::MUTED))
                        .child("Size edits keep the layer origin and animation."),
                );
        }
        if !is_null && !is_audio {
            let state = self.state.clone();
            contents = contents.child(
                ui::text_button("open-effects", "Open Effect Controls").on_click(
                    move |_, _, cx| {
                        state.update(cx, |s, cx| {
                            s.effect_controls_open = true;
                            cx.notify();
                        });
                    },
                ),
            );
            let mask = if layer.mask().is_some() {
                None
            } else {
                Some(Mask {
                    x: layer.width() * 0.25,
                    y: layer.height() * 0.25,
                    width: layer.width() * 0.5,
                    height: layer.height() * 0.5,
                    inverted: false,
                })
            };
            contents = contents.child(
                div()
                    .mt_3()
                    .border_t_1()
                    .border_color(rgb(ui::BORDER))
                    .child(
                        ui::text_button(
                            "mask-toggle",
                            if layer.mask().is_some() {
                                "Remove rectangle mask"
                            } else {
                                "Add rectangle mask"
                            },
                        )
                        .on_click({
                            let state = self.state.clone();
                            move |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::SetMask { id, mask }),
                                        window,
                                        cx,
                                    )
                                })
                            }
                        }),
                    ),
            );
            if let Some(mut mask) = layer.mask() {
                for (index, label, value) in [
                    (5, "Mask X", mask.x),
                    (6, "Mask Y", mask.y),
                    (7, "Mask Width", mask.width),
                    (8, "Mask Height", mask.height),
                ] {
                    self.extra[index].update(cx, |f, _| {
                        f.set_numeric();
                        f.sync(id.to_string(), format!("{value:.2}"), window);
                    });
                    contents = contents.child(
                        div()
                            .flex()
                            .h(px(29.0))
                            .items_center()
                            .child(div().flex_1().child(label))
                            .child(
                                div()
                                    .w(px(90.0))
                                    .when(!locked, |s| s.child(self.extra[index].clone())),
                            ),
                    );
                }
                mask.inverted = !mask.inverted;
                contents = contents.child(
                    ui::text_button(
                        "invert-mask",
                        if mask.inverted {
                            "Mask: Add"
                        } else {
                            "Mask: Subtract"
                        },
                    )
                    .on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::SetMask {
                                        id,
                                        mask: Some(mask),
                                    }),
                                    window,
                                    cx,
                                )
                            })
                        }
                    }),
                );
            }
        }
        contents = contents.child(
            div()
                .mt_3()
                .py_2()
                .border_t_1()
                .border_color(rgb(0x353535))
                .child("Parent & Link"),
        );
        let parent_label = layer
            .parent()
            .and_then(|id| comp.layer(id))
            .map_or("None".to_string(), |p| format!("{} · {}", p.id(), p.name()));
        contents = contents.child(
            ui::text_button("parent-picker", format!("{parent_label}  ▾"))
                .justify_start()
                .when(locked, |s| s.opacity(0.4))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !locked {
                        this.parent_open = !this.parent_open;
                        cx.notify();
                    }
                })),
        );
        if self.parent_open && !locked {
            let choices = std::iter::once((None, "None".to_string())).chain(
                comp.layers()
                    .iter()
                    .filter(|l| comp.can_parent(id, Some(l.id())))
                    .map(|l| (Some(l.id()), format!("{} · {}", l.id(), l.name()))),
            );
            let mut menu = div()
                .id("parent-options")
                .max_h(px(150.0))
                .overflow_y_scroll()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(0x454545));
            for (parent, label) in choices {
                menu = menu.child(
                    ui::text_button(
                        gpui::SharedString::from(format!("parent-{}", parent.unwrap_or(0))),
                        label,
                    )
                    .justify_start()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.parent_open = false;
                        this.state.update(cx, |state, cx| {
                            state.dispatch(
                                &Action::Edit(Command::SetParent { id, parent, frame }),
                                window,
                                cx,
                            )
                        });
                        cx.notify();
                    })),
                );
            }
            contents = contents.child(menu);
        }
        contents = contents.child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(ui::MUTED))
                .child("Keeps current pose · opacity stays independent"),
        );
        contents = contents.child(
            div()
                .mt_3()
                .py_2()
                .border_t_1()
                .border_color(rgb(0x353535))
                .child("Layer timing"),
        );
        for (index, (label, value)) in [
            ("In (frame)", layer.in_frame()),
            ("Out (exclusive)", layer.out_frame(duration)),
        ]
        .into_iter()
        .enumerate()
        {
            self.range[index].update(cx, |field, _| {
                field.sync(id.to_string(), value.to_string(), window)
            });
            contents = contents.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(32.0))
                    .gap_2()
                    .child(div().flex_1().text_size(px(11.0)).child(label))
                    .child(
                        div()
                            .w(px(82.0))
                            .when(!locked, |s| s.child(self.range[index].clone()))
                            .when(locked, |s| s.child(value.to_string())),
                    ),
            );
        }
        contents = contents.child(div().mt_3().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(if locked { "Layer locked. Unlock it in the timeline to edit." } else { "Enter to apply · Escape to cancel\nStopwatch toggles animation; diamond toggles a key." }));
        panel = panel.child(contents);
        panel
    }
}

#[cfg(test)]
mod typography_tests {
    use super::*;
    use libre_effects_core::Editor;

    #[test]
    fn inspector_font_size_and_source_text_keep_the_base_and_animated_tracks_separate() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Before".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 120.,
            name: "Text".into(),
        })
        .unwrap();
        assert!(
            text_field_command(e.selected_layer().unwrap(), 0, 0, "Before")
                .unwrap()
                .is_none()
        );
        let command = text_field_command(e.selected_layer().unwrap(), 0, 1, " 52.25 ")
            .unwrap()
            .unwrap();
        e.execute(command).unwrap();
        assert!(
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Text(TextParam::FontSize))
                .is_none()
        );
        for parameter in [TextParam::FontSize, TextParam::Tracking, TextParam::Leading] {
            e.execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
        }
        let command = text_field_command(e.selected_layer().unwrap(), 60, 1, "120.123456789")
            .unwrap()
            .unwrap();
        e.execute(command).unwrap();
        let sample = e
            .selected_layer()
            .unwrap()
            .text_value_at(TextParam::FontSize, 17)
            .unwrap();
        assert!(
            text_field_command(e.selected_layer().unwrap(), 17, 1, &sample.to_string())
                .unwrap()
                .is_none()
        );
        let tracks = [TextParam::FontSize, TextParam::Tracking, TextParam::Leading].map(|p| {
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Text(p))
                .unwrap()
                .clone()
        });
        let command = text_field_command(e.selected_layer().unwrap(), 17, 0, "After")
            .unwrap()
            .unwrap();
        e.execute(command).unwrap();
        assert_eq!(
            e.selected_layer().unwrap().content(),
            &Content::Text {
                text: "After".into(),
                font_size: 52.25
            }
        );
        for (p, track) in [TextParam::FontSize, TextParam::Tracking, TextParam::Leading]
            .into_iter()
            .zip(tracks)
        {
            assert_eq!(
                e.selected_layer().unwrap().track(PropertyPath::Text(p)),
                Some(&track)
            );
        }
        for invalid in ["NaN", "inf", "0", "2049", "bad"] {
            assert!(text_field_command(e.selected_layer().unwrap(), 17, 1, invalid).is_err());
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(text_field_command(e.selected_layer().unwrap(), 17, 0, "Hidden").is_err());
        assert!(text_field_command(e.selected_layer().unwrap(), 17, 1, "90").is_err());
    }
    #[test]
    fn inspector_text_opacity_fields_preserve_source_rgb_and_transform_and_precise_noops() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Before".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 120.,
            name: "Text".into(),
        })
        .unwrap();
        let original_style = e.selected_layer().unwrap().text_style();
        let original_color = e.selected_layer().unwrap().color();
        e.clear_history();
        let before = e.project().clone();
        for index in [11, 12] {
            assert!(
                text_field_command(e.selected_layer().unwrap(), 17, index, "100.000")
                    .unwrap()
                    .is_none()
            );
        }
        assert_eq!(e.project(), &before);
        assert!(!e.can_undo());
        for (index, parameter, value) in [
            (11, TextParam::FillOpacity, 23.123456789012345),
            (12, TextParam::StrokeOpacity, 67.98765432109876),
        ] {
            assert_eq!(text_field_parameter(index), Some(parameter));
            let command =
                text_field_command(e.selected_layer().unwrap(), 17, index, &value.to_string())
                    .unwrap()
                    .unwrap();
            e.execute(command).unwrap();
            assert_eq!(
                e.selected_layer().unwrap().text_value_at(parameter, 17),
                Some(value)
            );
            assert!(
                e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(parameter))
                    .unwrap()
                    .keys()
                    .is_empty()
            );
            assert!(
                text_field_command(e.selected_layer().unwrap(), 17, index, &value.to_string())
                    .unwrap()
                    .is_none()
            );
            for bad in ["NaN", "inf", "-1", "100.0001", "bad"] {
                assert!(text_field_command(e.selected_layer().unwrap(), 17, index, bad).is_err());
            }
        }
        let tracks = [TextParam::FillOpacity, TextParam::StrokeOpacity].map(|p| {
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Text(p))
                .unwrap()
                .clone()
        });
        let command = text_field_command(e.selected_layer().unwrap(), 17, 0, "After")
            .unwrap()
            .unwrap();
        e.execute(command).unwrap();
        for (parameter, track) in [TextParam::FillOpacity, TextParam::StrokeOpacity]
            .into_iter()
            .zip(tracks)
        {
            assert_eq!(
                e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(parameter)),
                Some(&track)
            );
        }
        assert_eq!(e.selected_layer().unwrap().text_style(), original_style);
        assert_eq!(e.selected_layer().unwrap().color(), original_color);
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .property(Property::Opacity)
                .expect("every layer has a scalar Opacity track")
                .value_at(17),
            100.
        );
        e.execute(Command::ToggleLocked(1)).unwrap();
        for index in [11, 12] {
            assert!(text_field_command(e.selected_layer().unwrap(), 17, index, "50").is_err());
        }
        e.execute(Command::AddSolid).unwrap();
        for index in [11, 12] {
            assert!(text_field_command(e.selected_layer().unwrap(), 17, index, "50").is_err());
        }
    }
}

#[cfg(test)]
mod source_text_tests {
    use super::*;

    fn state() -> EditorState {
        let mut s = EditorState::default();
        s.editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Baseline".into(),
                    font_size: 48.0,
                },
                width: 400.0,
                height: 120.0,
                name: "Text".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::EditTrack {
                id: 1,
                property: PropertyPath::SourceText,
                edit: TrackEdit::ToggleAnimation { frame: 10 },
            })
            .unwrap();
        s.editor
            .execute(Command::EditSourceText {
                id: 1,
                frame: 40,
                text: "Changed 🦋\r\n世界".into(),
            })
            .unwrap();
        s.frame = 20;
        s
    }

    #[test]
    fn inspector_source_text_reads_hold_sample_and_edits_only_current_source() {
        let mut s = state();
        for parameter in [
            TextParam::FontSize,
            TextParam::FillOpacity,
            TextParam::StrokeOpacity,
        ] {
            s.editor
                .execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit: TrackEdit::ToggleAnimation { frame: 0 },
                })
                .unwrap();
        }
        let layer = s.editor.selected_layer().unwrap();
        let style = layer.text_style();
        let baseline = layer.content().clone();
        let numeric: Vec<_> = layer
            .track_paths()
            .into_iter()
            .filter(|p| *p != PropertyPath::SourceText)
            .map(|p| (p, layer.track(p).unwrap().clone()))
            .collect();
        for (frame, value) in [
            (0, "Baseline"),
            (20, "Baseline"),
            (40, "Changed 🦋\r\n世界"),
            (80, "Changed 🦋\r\n世界"),
        ] {
            assert!(
                text_field_command(layer, frame, 0, value)
                    .unwrap()
                    .is_none()
            );
        }
        s.editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Temporary".into(),
            })
            .unwrap();
        s.editor.undo();
        let origin = s.editor.project().clone();
        assert!(s.editor.can_redo());
        assert!(
            text_field_command(s.editor.selected_layer().unwrap(), 20, 0, "Baseline")
                .unwrap()
                .is_none()
        );
        assert_eq!(s.editor.project(), &origin);
        assert!(s.editor.can_redo());
        let text = "  🦋\r\nWorld  ";
        let command = text_field_command(s.editor.selected_layer().unwrap(), 20, 0, text)
            .unwrap()
            .unwrap();
        assert!(
            matches!(&command, Command::EditSourceText { id: 1, frame: 20, text: value } if value == text)
        );
        s.editor.execute(command).unwrap();
        let layer = s.editor.selected_layer().unwrap();
        assert_eq!(layer.source_text_at(20), Some(text));
        assert_eq!(layer.source_text_at(39), Some(text));
        assert_eq!(layer.source_text_at(40), Some("Changed 🦋\r\n世界"));
        assert_eq!(layer.content(), &baseline);
        assert_eq!(layer.text_style(), style);
        for (p, track) in numeric {
            assert_eq!(layer.track(p), Some(&track));
        }
        s.editor.undo();
        assert_eq!(s.editor.project(), &origin);
        s.editor.redo();
        let command = text_field_command(s.editor.selected_layer().unwrap(), 20, 0, "")
            .unwrap()
            .unwrap();
        s.editor.execute(command).unwrap();
        assert_eq!(
            s.editor.selected_layer().unwrap().source_text_at(20),
            Some("")
        );
    }

    #[test]
    fn source_text_input_binding_rejects_source_frame_revision_layer_and_playback_changes() {
        let mut s = state();
        let target = InputTarget::new(&s).unwrap();
        assert!(target.current(&s));
        s.frame += 1;
        assert!(!target.current(&s));
        s.frame -= 1;
        s.document_revision += 1;
        assert!(!target.current(&s));
        s.document_revision -= 1;
        s.playing = true;
        assert!(!target.current(&s));
        s.playing = false;
        s.editor
            .execute(Command::EditSourceText {
                id: 1,
                frame: 40,
                text: "Different future source".into(),
            })
            .unwrap();
        // Even an unchanged current Hold sample cannot accept a stale document.
        assert_eq!(
            s.editor.selected_layer().unwrap().source_text_at(s.frame),
            Some("Baseline")
        );
        assert!(!target.current(&s));
        s.editor.undo();
        assert!(target.current(&s));
        s.editor.execute(Command::ToggleLocked(1)).unwrap();
        assert!(!target.current(&s));
        s.editor.undo();
        s.editor.execute(Command::AddSolid).unwrap();
        assert!(!target.current(&s));
    }
}
