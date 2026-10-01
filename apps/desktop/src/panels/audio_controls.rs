use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px, relative, rgb};
use libre_effects_core::{AudioParam, Command, LayerSwitch, PropertyPath, TrackEdit};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct AudioControls {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
    meter: Option<Result<crate::audio_mix::Levels, String>>,
    pending: bool,
    generation: u64,
    cancel: Arc<AtomicBool>,
}
impl Drop for AudioControls {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl AudioControls {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            this.cancel.store(true, Ordering::Relaxed);
            this.generation = this.generation.wrapping_add(1);
            this.meter = None;
            cx.notify();
        })
        .detach();
        let fields = AudioParam::ALL
            .into_iter()
            .map(|parameter| {
                let state = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        state.update(cx, |s, cx| {
                            let Some(id) = s.editor.selected() else {
                                return;
                            };
                            match text.trim().parse::<f64>() {
                                Ok(value) => s.dispatch(
                                    &Action::Edit(Command::EditTrack {
                                        id,
                                        property: PropertyPath::Audio(parameter),
                                        edit: TrackEdit::Value {
                                            frame: s.frame,
                                            value,
                                        },
                                    }),
                                    window,
                                    cx,
                                ),
                                Err(_) => {
                                    s.status = "Enter a finite audio value".into();
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
            fields,
            meter: None,
            pending: false,
            generation: 0,
            cancel: Default::default(),
        }
    }
    fn measure(&mut self, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        let state = self.state.read(cx);
        let project = state.editor.project().clone();
        let frame = state.frame;
        let generation = self.generation;
        self.pending = true;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let comp = project.composition();
                    let origin = comp.fps().seconds(u64::from(frame));
                    let end = comp.fps().seconds(u64::from(comp.duration()));
                    let count = ((end - origin).max(0.0) * 48000.0).ceil().min(4800.0) as usize;
                    let mut mixer = crate::audio_mix::Mixer::new(&project, true)?;
                    mixer.render(origin, 0, count, end, &cancel)?;
                    Ok::<_, String>(mixer.levels)
                })
                .await;
            let _ = entity.update(cx, |this, cx| {
                this.pending = false;
                if this.generation == generation {
                    this.meter = Some(result);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
fn decibels(value: f64) -> String {
    if value <= 0.0 {
        "−∞".into()
    } else {
        format!("{:.1}", 20.0 * value.log10())
    }
}
impl Render for AudioControls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let Some(layer) = s.editor.selected_layer().filter(|l| l.can_audio()).cloned() else {
            return div();
        };
        let comp = s.editor.project().composition();
        let (frame, duration, fps) = (s.frame, comp.duration(), comp.fps());
        let id = layer.id();
        let locked = layer.locked();
        let audio_enabled = layer.audio_enabled();
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_1()
            .py_2()
            .border_b_1()
            .border_color(rgb(ui::BORDER))
            .child("Audio")
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        ui::text_button(
                            "audio-enable",
                            if layer.audio_enabled() {
                                "Audio: On"
                            } else {
                                "Audio: Off"
                            },
                        )
                        .when(!locked, |b| {
                            b.on_click({
                                let state = self.state.clone();
                                move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::Edit(Command::SetAudioEnabled {
                                                id,
                                                enabled: !audio_enabled,
                                            }),
                                            window,
                                            cx,
                                        )
                                    });
                                }
                            })
                        }),
                    )
                    .child(
                        ui::text_button(
                            "audio-solo",
                            if layer.solo() {
                                "Solo: On"
                            } else {
                                "Solo: Off"
                            },
                        )
                        .when(!locked, |b| {
                            b.on_click({
                                let state = self.state.clone();
                                let enabled = !layer.solo();
                                move |_, window, cx| {
                                    state.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::Edit(Command::SetLayerSwitch {
                                                id,
                                                switch: LayerSwitch::Solo,
                                                enabled,
                                            }),
                                            window,
                                            cx,
                                        )
                                    });
                                }
                            })
                        }),
                    ),
            );
        for (index, p) in AudioParam::ALL.into_iter().enumerate() {
            let path = PropertyPath::Audio(p);
            let track = layer.track(path).unwrap();
            let value = format!("{:.2}", layer.track_value(path, frame).unwrap());
            self.fields[index].update(cx, |f, _| {
                f.sync(format!("{id}-{frame}"), value.clone(), window)
            });
            let key = |suffix: &str| SharedString::from(format!("audio-{index}-{suffix}"));
            panel = panel.child(
                div()
                    .flex()
                    .h(px(27.0))
                    .items_center()
                    .child(ui::action_tool(
                        key("watch"),
                        "stopwatch",
                        "Toggle audio animation",
                        &self.state,
                        Action::Edit(Command::EditTrack {
                            id,
                            property: path,
                            edit: TrackEdit::ToggleAnimation { frame },
                        }),
                        !track.keys().is_empty(),
                    ))
                    .child(ui::action_tool(
                        key("key"),
                        "diamond",
                        "Add or remove audio key",
                        &self.state,
                        Action::Edit(Command::EditTrack {
                            id,
                            property: path,
                            edit: TrackEdit::ToggleKey { frame },
                        }),
                        track.keys().contains_key(&frame),
                    ))
                    .child(
                        div()
                            .w(px(107.0))
                            .text_size(px(11.0))
                            .child(p.label().trim_start_matches("Audio ")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(!locked, |d| d.child(self.fields[index].clone()))
                            .when(locked, |d| d.child(value)),
                    )
                    .child(ui::action_tool(
                        key("graph"),
                        "chart-line",
                        "Edit audio graph",
                        &self.state,
                        Action::GraphProperty(id, path),
                        false,
                    )),
            );
        }
        let first = layer.in_frame();
        let last = layer.out_frame(duration) - 1;
        let length = (fps.as_f64() * 0.5).round().max(1.0) as u32;
        let mut fades = div().flex().gap_1();
        for fade_in in [true, false] {
            let (start, end) = if fade_in {
                (first, (first + length).min(last))
            } else {
                (last.saturating_sub(length).max(first), last)
            };
            let state = self.state.clone();
            fades = fades.child(
                ui::text_button(
                    if fade_in {
                        "audio-fade-in"
                    } else {
                        "audio-fade-out"
                    },
                    if fade_in {
                        "Fade In 0.5s"
                    } else {
                        "Fade Out 0.5s"
                    },
                )
                .when(!locked && start < end, |b| {
                    b.on_click(move |_, window, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::FadeAudio {
                                    id,
                                    start,
                                    end,
                                    fade_in,
                                }),
                                window,
                                cx,
                            )
                        });
                    })
                }),
            );
        }
        panel = panel
            .child(fades)
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(ui::MUTED))
                    .child("Pan moves stereo channels · 0 is unchanged"),
            )
            .child(
                ui::text_button(
                    "audio-meter",
                    if self.pending {
                        "Measuring…"
                    } else {
                        "Measure mix · next 100 ms"
                    },
                )
                .when(!self.pending, |b| {
                    b.on_click(cx.listener(|this, _, _, cx| this.measure(cx)))
                }),
            );
        if let Some(result) = &self.meter {
            match result {
                Ok(levels) => {
                    for channel in 0..2 {
                        let peak = f64::from(levels.peak[channel]);
                        let rms =
                            (levels.sum_squares[channel] / levels.frames.max(1) as f64).sqrt();
                        let fraction = if peak > 0.0 {
                            ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        panel = panel
                            .child(div().text_size(px(10.0)).child(format!(
                                "{} peak {} / RMS {} dBFS",
                                if channel == 0 { "L" } else { "R" },
                                decibels(peak),
                                decibels(rms)
                            )))
                            .child(
                                div().h(px(5.0)).bg(rgb(0x303030)).child(
                                    div()
                                        .h_full()
                                        .w(relative(fraction as f32))
                                        .bg(rgb(if peak > 1.0 { 0xef6666 } else { 0x58bf96 })),
                                ),
                            );
                    }
                    panel = panel.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(if levels.clipped_frames > 0 {
                                0xef6666
                            } else {
                                ui::MUTED
                            }))
                            .child(format!(
                                "Composition mix · {} clipped stereo samples",
                                levels.clipped_frames
                            )),
                    );
                }
                Err(error) => {
                    panel = panel.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(0xef6666))
                            .child(error.clone()),
                    )
                }
            }
        }
        panel
    }
}
