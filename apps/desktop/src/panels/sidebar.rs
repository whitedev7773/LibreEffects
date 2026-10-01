use super::Inspector;
use crate::{
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::{AlignTarget, Alignment, Command};

pub(crate) struct Sidebar {
    state: Entity<EditorState>,
    inspector: Entity<Inspector>,
    catalog: Entity<super::effects::EffectCatalog>,
}
impl Sidebar {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            inspector: cx.new(|cx| Inspector::new(state.clone(), cx)),
            catalog: cx.new(|cx| super::effects::EffectCatalog::new(state.clone(), cx)),
            state,
        }
    }
}
impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let comp = state.editor.project().composition();
        let mut info = format!(
            "{}\n{} × {} · {} fps\nTime: {}\n{} layers",
            comp.name(),
            comp.width(),
            comp.height(),
            comp.fps().label(),
            comp.timecode(state.frame),
            comp.layers().len()
        );
        if let Some((revision, composition, pixel)) = &state.pixel_info
            && *revision == state.document_revision
            && *composition == state.editor.project().active_composition_id()
            && pixel.frame == state.frame
        {
            let [r, g, b, a] = pixel.rgba;
            info.push_str(&format!("\nX: {}   Y: {}\nR: {r}  G: {g}  B: {b}  A: {a}\n#{r:02X}{g:02X}{b:02X} · Alpha {:.1}%\nPreview sample: {} × {}",pixel.position[0],pixel.position[1],f64::from(a)/255.0*100.0,pixel.resolution[0],pixel.resolution[1]));
        } else {
            info.push_str("\nMove over the composition to sample RGBA");
        }
        let playing = state.playing;
        let expanded = state.workspace.sidebar_expanded;
        let work = format!("Work area: {}–{}f", state.work_start, state.work_end);
        let mut panel = div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .bg(rgb(ui::BG));
        for (index, label) in ["Properties", "Info", "Preview", "Effects & Presets"]
            .into_iter()
            .enumerate()
        {
            let active = expanded[index];
            panel = panel.child(
                ui::text_button(gpui::SharedString::from(format!("dock-{label}")), label)
                    .h(px(27.0))
                    .flex_none()
                    .justify_start()
                    .border_b_1()
                    .border_color(rgb(ui::BORDER))
                    .when(active, |s| s.text_color(rgb(ui::BLUE)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            s.workspace.sidebar_expanded[index] =
                                !s.workspace.sidebar_expanded[index];
                            cx.notify();
                        });
                    })),
            );
            if active {
                panel = panel.child(match index {
                    0 => div()
                        .flex_1()
                        .min_h_0()
                        .child(self.inspector.clone())
                        .into_any_element(),
                    1 => div()
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_size(px(11.0))
                        .children(info.lines().map(|line| div().child(line.to_string())))
                        .into_any_element(),
                    3 => div()
                        .flex_none()
                        .child(self.catalog.clone())
                        .into_any_element(),
                    _ => div()
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .gap_1()
                                .child(ui::action_tool(
                                    "preview-first",
                                    "arrow-left",
                                    "First frame (Home)",
                                    &self.state,
                                    Action::Seek(0),
                                    false,
                                ))
                                .child(ui::action_tool(
                                    "preview-back",
                                    "arrow-left",
                                    "Previous frame",
                                    &self.state,
                                    Action::Step(-1),
                                    false,
                                ))
                                .child(ui::action_tool(
                                    "preview-play",
                                    if playing { "pause" } else { "play" },
                                    "Play / Pause (Space)",
                                    &self.state,
                                    Action::Play,
                                    playing,
                                ))
                                .child(ui::action_tool(
                                    "preview-next",
                                    "arrow-right",
                                    "Next frame",
                                    &self.state,
                                    Action::Step(1),
                                    false,
                                )),
                        )
                        .child("Shortcut: Space")
                        .child(work.clone())
                        .child(preview_audio_controls(&self.state, cx))
                        .into_any_element(),
                });
            }
        }
        panel
    }
}
pub(crate) struct Align {
    state: Entity<EditorState>,
}
impl Align {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}
impl Render for Align {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let ids: Vec<_> = state.selected_layers.iter().copied().collect();
        let frame = state.frame;
        let roots = state.editor.project().composition().selection_roots(&ids);
        let count = roots.as_ref().map_or(0, |ids| ids.len());
        let selection = state.workspace.align_to_selection;
        let enabled = count >= if selection { 2 } else { 1 };
        let distribute_enabled = count >= 3;
        let mut target_controls = div().flex().gap_1();
        for (key, label, target) in [
            ("align-composition", "Composition", false),
            ("align-selection", "Selection", true),
        ] {
            target_controls = target_controls.child(
                ui::text_button(key, label)
                    .when(selection == target, |b| {
                        b.bg(rgb(0x164a7b)).text_color(rgb(ui::BLUE))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            s.workspace.align_to_selection = target;
                            cx.notify();
                        });
                    })),
            );
        }
        let mut distributions = div().flex().gap_2();
        let mut controls = div().flex().gap_2();
        for (index, (icon, label, alignment)) in [
            ("object-align-left", "Align left", Alignment::Left),
            (
                "object-align-center-horizontal",
                "Center horizontally",
                Alignment::HorizontalCenter,
            ),
            ("object-align-right", "Align right", Alignment::Right),
            ("object-align-top", "Align top", Alignment::Top),
            (
                "object-align-center-vertical",
                "Center vertically",
                Alignment::VerticalCenter,
            ),
            ("object-align-bottom", "Align bottom", Alignment::Bottom),
        ]
        .into_iter()
        .enumerate()
        {
            controls = controls.child(
                ui::tool(
                    gpui::SharedString::from(format!("align-{index}")),
                    icon,
                    label,
                    false,
                )
                .when(!enabled, |s| s.opacity(0.35))
                .on_click({
                    let state = self.state.clone();
                    let ids = ids.clone();
                    move |_, window, cx| {
                        if enabled {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::AlignLayers {
                                        ids: ids.clone(),
                                        target: if selection {
                                            AlignTarget::Selection
                                        } else {
                                            AlignTarget::Composition
                                        },
                                        frame,
                                        alignment,
                                    }),
                                    window,
                                    cx,
                                )
                            });
                        }
                    }
                }),
            );
            distributions = distributions.child(
                ui::tool(
                    gpui::SharedString::from(format!("distribute-{index}")),
                    icon,
                    match alignment {
                        Alignment::Left => "Distribute left edges",
                        Alignment::HorizontalCenter => "Distribute horizontal centers",
                        Alignment::Right => "Distribute right edges",
                        Alignment::Top => "Distribute top edges",
                        Alignment::VerticalCenter => "Distribute vertical centers",
                        Alignment::Bottom => "Distribute bottom edges",
                    },
                    false,
                )
                .when(!distribute_enabled, |b| b.opacity(0.35))
                .on_click({
                    let state = self.state.clone();
                    let ids = ids.clone();
                    move |_, w, cx| {
                        if distribute_enabled {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::DistributeLayers {
                                        ids: ids.clone(),
                                        frame,
                                        alignment,
                                    }),
                                    w,
                                    cx,
                                )
                            });
                        }
                    }
                }),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(ui::BG))
            .child(ui::panel_header("Align"))
            .child(
                div()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child("Align Layers to:")
                    .child(target_controls)
                    .child(controls)
                    .child("Distribute Layers:")
                    .child(distributions)
                    .child(div().text_size(px(10.0)).text_color(rgb(ui::MUTED)).child(
                        if roots.is_err() {
                            "Select unlocked layers"
                        } else if count < 3 {
                            "Distribution needs 3 independent layers"
                        } else {
                            "Selected parents carry their children"
                        },
                    )),
            )
    }
}

fn preview_audio_controls(state: &Entity<EditorState>, cx: &gpui::App) -> gpui::Div {
    use crate::audio_playback::Phase;
    let s = state.read(cx);
    let mut panel = div().flex().flex_col().gap_1().text_size(px(11.0));
    for (id, label, enabled, action) in [
        (
            "preview-audio",
            "Audio",
            s.preview_audio,
            Action::PreviewAudio,
        ),
        (
            "preview-scrub",
            "Scrub",
            s.preview_scrub,
            Action::PreviewScrub,
        ),
        (
            "preview-loop",
            "Loop work area",
            s.preview_loop,
            Action::PreviewLoop,
        ),
    ] {
        let state = state.clone();
        panel = panel.child(
            ui::text_button(
                id,
                format!("{label}: {}", if enabled { "On" } else { "Off" }),
            )
            .justify_start()
            .on_click(move |_, window, cx| {
                state.update(cx, |s, cx| s.dispatch(&action, window, cx))
            }),
        );
    }
    let status = &s.audio_status;
    let label = match &status.phase {
        Phase::Buffering => "Buffering audio…".to_string(),
        Phase::Playing => format!("Playing · {} buffer underruns", status.underruns),
        Phase::Ended if s.playing => if s.preview_audio {
            "No active audio · visual preview"
        } else {
            "Audio off · visual preview"
        }
        .to_string(),
        Phase::Ended => "Stopped".to_string(),
        Phase::Failed(error) => format!("Audio error: {error}"),
    };
    panel = panel
        .child("Default Windows output · 48 kHz stereo")
        .child(label);
    if matches!(status.phase, Phase::Playing) {
        for channel in 0..2 {
            let peak = f64::from(status.levels.peak[channel]);
            let rms =
                (status.levels.sum_squares[channel] / status.levels.frames.max(1) as f64).sqrt();
            let db = |v: f64| {
                if v == 0.0 {
                    "−∞".into()
                } else {
                    format!("{:.1}", 20.0 * v.log10())
                }
            };
            panel = panel.child(
                div()
                    .text_color(rgb(if peak > 1.0 { 0xef6666 } else { ui::MUTED }))
                    .child(format!(
                        "{} Peak {} / RMS {} dBFS",
                        if channel == 0 { "L" } else { "R" },
                        db(peak),
                        db(rms)
                    )),
            );
            let fraction = if peak > 0.0 {
                ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
            } else {
                0.0
            };
            panel = panel.child(
                div().h(px(5.0)).bg(rgb(0x303030)).child(
                    div()
                        .h_full()
                        .w(gpui::relative(fraction as f32))
                        .bg(rgb(if peak > 1.0 { 0xef6666 } else { 0x58bf96 })),
                ),
            );
        }
        panel = panel.child(format!(
            "{} clipped stereo samples / block",
            status.levels.clipped_frames
        ));
    }
    panel.child(
        div()
            .text_color(rgb(ui::MUTED))
            .child("Scrub previews 100 ms after seeking."),
    )
}
