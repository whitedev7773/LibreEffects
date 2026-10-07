use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use libre_effects_core::{
    AlphaInterpretation, AssetId, Command, Content, FootageInterpretation, FrameRate,
};

pub(crate) struct Interpretation {
    state: Entity<EditorState>,
    asset: AssetId,
    fps: Entity<TextField>,
    matte: Entity<TextField>,
}
impl Interpretation {
    pub fn new(state: Entity<EditorState>, asset: AssetId, cx: &mut Context<Self>) -> Self {
        let edit = state.clone();
        let fps = cx.new(|cx| {
            TextField::new(cx, move |value, window, cx| {
                edit.update(cx, |s, cx| {
                    let Some(a) = s.editor.project().asset_library().assets().get(&asset) else {
                        return;
                    };
                    let mut interpretation = a.interpretation();
                    let value = value.trim();
                    let fps = if value.is_empty() || value.eq_ignore_ascii_case("source") {
                        Ok(None)
                    } else {
                        value.parse::<FrameRate>().map(Some)
                    };
                    match fps {
                        Ok(fps) => {
                            interpretation.fps = fps;
                            s.dispatch(
                                &Action::Edit(Command::InterpretAsset {
                                    asset,
                                    interpretation,
                                }),
                                window,
                                cx,
                            );
                        }
                        Err(e) => {
                            s.status = e;
                            cx.notify();
                        }
                    }
                });
            })
            .tab_stop()
        });
        let edit = state.clone();
        let matte = cx.new(|cx| {
            TextField::new(cx, move |value, window, cx| {
                edit.update(cx, |s, cx| {
                    let Some(a) = s.editor.project().asset_library().assets().get(&asset) else {
                        return;
                    };
                    let mut interpretation = a.interpretation();
                    let value = value.trim().trim_start_matches('#');
                    if value.len() == 6
                        && let Ok(matte) = u32::from_str_radix(value, 16)
                    {
                        interpretation.alpha = AlphaInterpretation::Premultiplied { matte };
                        s.dispatch(
                            &Action::Edit(Command::InterpretAsset {
                                asset,
                                interpretation,
                            }),
                            window,
                            cx,
                        );
                    } else {
                        s.status = "Matte color must be six hexadecimal digits".into();
                        cx.notify();
                    }
                });
            })
            .tab_stop()
        });
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            asset,
            fps,
            matte,
        }
    }
    fn change(
        &mut self,
        interpretation: FootageInterpretation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |s, cx| {
            s.dispatch(
                &Action::Edit(Command::InterpretAsset {
                    asset: self.asset,
                    interpretation,
                }),
                window,
                cx,
            )
        });
    }
}
impl Render for Interpretation {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(a) = self
            .state
            .read(cx)
            .editor
            .project()
            .asset_library()
            .assets()
            .get(&self.asset)
        else {
            return div();
        };
        let interpretation = a.interpretation();
        let video = a.content().footage_timing().is_some();
        let missing = if let Content::ImageSequence { missing, .. } = a.content() {
            Some(*missing)
        } else {
            None
        };
        let key = format!("{}-{}", self.state.read(cx).document_revision, self.asset);
        self.fps.update(cx, |f, _| {
            f.sync(
                key.clone(),
                interpretation
                    .fps
                    .map(|f| f.to_string())
                    .unwrap_or("Source".into()),
                window,
            )
        });
        let matte = match interpretation.alpha {
            AlphaInterpretation::Premultiplied { matte } => matte,
            _ => 0,
        };
        self.matte
            .update(cx, |f, _| f.sync(key, format!("{matte:06X}"), window));
        let mut panel = div()
            .px_2()
            .py_1()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(11.0));
        if video {
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().w(px(110.0)).child("Assume FPS"))
                    .child(div().flex_1().child(self.fps.clone())),
            );
        }
        if let Some(missing) = missing {
            let mut row = div().flex().gap_1();
            for (index, label, policy) in [
                (
                    0usize,
                    "Error",
                    libre_effects_core::MissingFramePolicy::Error,
                ),
                (1, "Hold", libre_effects_core::MissingFramePolicy::Hold),
                (
                    2,
                    "Transparent",
                    libre_effects_core::MissingFramePolicy::Transparent,
                ),
            ] {
                row = row.child(
                    ui::text_button(("sequence-missing", index), label)
                        .flex_1()
                        .when(missing == policy, |d| d.bg(rgb(ui::BLUE)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::SetSequenceMissing {
                                        asset: this.asset,
                                        missing: policy,
                                    }),
                                    window,
                                    cx,
                                )
                            });
                        })),
                );
            }
            panel = panel.child("Missing frames").child(row);
        }
        let mut alpha = div().flex().gap_1();
        for (index, label, mode) in [
            (0usize, "Straight", AlphaInterpretation::Straight),
            (1, "Ignore", AlphaInterpretation::Ignore),
            (
                2,
                "Premultiplied",
                AlphaInterpretation::Premultiplied { matte },
            ),
        ] {
            alpha = alpha.child(
                ui::text_button(("alpha-mode", index), label)
                    .flex_1()
                    .when(interpretation.alpha == mode, |d| d.bg(rgb(ui::BLUE)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.change(
                            FootageInterpretation {
                                alpha: mode,
                                ..interpretation
                            },
                            window,
                            cx,
                        )
                    })),
            );
        }
        panel = panel.child(alpha);
        if matches!(
            interpretation.alpha,
            AlphaInterpretation::Premultiplied { .. }
        ) {
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().w(px(110.0)).child("Matte color (hex)"))
                    .child(div().flex_1().child(self.matte.clone())),
            );
        }
        panel
            .child(
                div()
                    .flex()
                    .gap_1()
                    .when(interpretation.alpha != AlphaInterpretation::Ignore, |d| {
                        d.child(
                            ui::text_button(
                                "invert-source-alpha",
                                if interpretation.invert_alpha {
                                    "✓ Invert alpha"
                                } else {
                                    "Invert alpha"
                                },
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.change(
                                        FootageInterpretation {
                                            invert_alpha: !interpretation.invert_alpha,
                                            ..interpretation
                                        },
                                        window,
                                        cx,
                                    )
                                },
                            )),
                        )
                    })
                    .child(
                        ui::text_button("reset-interpretation", "Reset").on_click(cx.listener(
                            |this, _, window, cx| this.change(Default::default(), window, cx),
                        )),
                    ),
            )
            .child(
                div()
                    .text_color(rgb(ui::MUTED))
                    .text_size(px(10.0))
                    .child("All source instances · layer trims and keys stay fixed"),
            )
    }
}
