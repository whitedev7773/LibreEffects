use crate::{
    components::TextField,
    editor::{Action, EditorState, queue::QueueAction},
    output_settings::{Field, Spec},
    render_queue::Format,
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};
use std::{collections::BTreeMap, sync::atomic::Ordering};

/// Occupies the existing timeline dock; composition/project/right panels stay put.
pub(crate) struct RenderDock {
    state: Entity<EditorState>,
    timeline: Entity<super::Timeline>,
    fields: BTreeMap<(u64, bool), Entity<TextField>>,
    output_fields: BTreeMap<(u64, usize, Field), Entity<TextField>>,
    expanded: Option<(u64, usize)>,
    preset_name: Entity<TextField>,
    preset_open: bool,
    preset_cursor: usize,
    preset_focus: gpui::FocusHandle,
}
impl RenderDock {
    pub fn new(
        state: Entity<EditorState>,
        timeline: Entity<super::Timeline>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            timeline,
            fields: BTreeMap::new(),
            output_fields: BTreeMap::new(),
            expanded: None,
            preset_open: false,
            preset_cursor: 0,
            preset_focus: cx.focus_handle(),
            preset_name: cx.new(|cx| TextField::new(cx, |_, _, _| {})),
        }
    }
    fn button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<gpui::SharedString>,
        action: QueueAction,
    ) -> gpui::Stateful<gpui::Div> {
        let state = self.state.clone();
        ui::text_button(id, label).on_click(move |_, window, cx| {
            state.update(cx, |s, cx| {
                s.dispatch(&Action::Queue(action.clone()), window, cx)
            })
        })
    }
}
impl Render for RenderDock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.state.read(cx).queue_open {
            return div().size_full().child(self.timeline.clone());
        }
        let queue = self.state.read(cx).queue.clone();
        let Some(queue) = queue else {
            return div()
                .p_3()
                .child(self.state.read(cx).queue_message.clone())
                .child(self.button("queue-return", "Timeline", QueueAction::Show(false)));
        };
        let Ok(q) = queue.try_lock() else {
            return div().p_3().child("Updating render queue…");
        };
        let data = q.data.clone();
        let active = q.active;
        let progress = q.progress.load(Ordering::Relaxed);
        let running = q.running;
        drop(q);
        let busy = running || self.state.read(cx).queue_busy;
        let formats = self.state.read(cx).queue_formats.clone();
        let mut choices: Vec<(String, Vec<Spec>)> = Format::ALL
            .iter()
            .map(|f| (f.label().into(), vec![(*f).into()]))
            .collect();
        choices.extend(
            data.presets
                .iter()
                .map(|p| (p.name.clone(), p.specs.clone())),
        );
        let chosen = choices.iter().position(|(_, f)| *f == formats).unwrap_or(0);
        let label = choices
            .iter()
            .find(|(_, f)| *f == formats)
            .map(|(label, _)| label.clone())
            .unwrap_or_else(|| "Custom".into());
        let keyboard_choices = choices.clone();
        let choice_count = choices.len();
        self.preset_cursor = self.preset_cursor.min(choice_count - 1);
        let mut root=div().relative().size_full().flex().flex_col().bg(rgb(ui::PANEL))
            .child(ui::composition_tabs(&self.state, cx, false))
            .child(div().flex().items_center().gap_2().h(px(27.0)).border_b_1().border_color(rgb(ui::BORDER))
                .child(div().px_2().text_color(rgb(ui::BLUE)).child("Render Queue"))
                .child(self.button("queue-timeline","Timeline",QueueAction::Show(false)))
                .child(div().flex_1())
                .child(self.button("queue-undo","Undo",QueueAction::Undo).when(busy,|d|d.opacity(0.4)))
                .child(self.button("queue-redo","Redo",QueueAction::Redo).when(busy,|d|d.opacity(0.4))))
            .child(div().flex().items_center().gap_2().px_2().py_1()
                .child(self.button("queue-add","Add composition (Ctrl+M)",QueueAction::Add).when(busy,|d|d.opacity(0.4)))
                .child(ui::text_button("queue-preset",format!("Preset: {label} ▾")).track_focus(&self.preset_focus).when(busy,|d|d.opacity(0.4))
                    .on_click(cx.listener(move|this,event,window,cx|{
                        if busy {return;}
                        window.focus(&this.preset_focus);
                        if this.preset_open && matches!(event,gpui::ClickEvent::Keyboard(_)) {
                            let formats=keyboard_choices[this.preset_cursor].1.clone();
                            this.state.update(cx,|s,cx|{s.queue_formats=formats;cx.notify();});this.preset_open=false;
                        }else{this.preset_open=!this.preset_open;this.preset_cursor=chosen;}
                        cx.stop_propagation();cx.notify();
                    }))
                    .on_key_down(cx.listener(move|this,event:&gpui::KeyDownEvent,_,cx|{
                        if busy || event.keystroke.modifiers.modified(){return;}
                        match event.keystroke.key.as_str(){
                            "down"=>{this.preset_open=true;this.preset_cursor=(this.preset_cursor+1)%choice_count;},
                            "up"=>{this.preset_open=true;this.preset_cursor=(this.preset_cursor+choice_count-1)%choice_count;},
                            "escape"=>this.preset_open=false,
                            "enter"|"space"=>{},_=>return,
                        }cx.stop_propagation();cx.notify();
                    })))
                .child(self.button("queue-policy",if data.stop_on_error {"On error: Stop"}else{"On error: Continue"},QueueAction::Policy).when(busy,|d|d.opacity(0.4)))
                .child(div().flex_1())
                .child(self.button("queue-run",if running{"Stop"}else{"Render"},if running{QueueAction::Stop}else{QueueAction::Start}).when(self.state.read(cx).queue_busy,|d|d.opacity(0.4))))
            .child(div().flex().items_center().gap_2().px_2().py_1().text_color(rgb(ui::MUTED))
                .child("Preset name") .child(div().w(px(180.0)).child(self.preset_name.clone()))
                .child("Range uses composition frames; End is exclusive. Jobs keep the captured composition."));
        if !self.state.read(cx).queue_message.is_empty() {
            root = root.child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(rgb(ui::MUTED))
                    .child(self.state.read(cx).queue_message.clone()),
            );
        }
        let mut list = div()
            .id("render-queue-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        if data.jobs.is_empty() {
            list=list.child(div().p_4().text_color(rgb(ui::MUTED)).child("Add a composition to queue a snapshot of its work area. Jobs and output results restore when the editor restarts."));
        }
        self.output_fields.retain(|(id, index, _), _| {
            data.jobs
                .iter()
                .any(|j| j.id == *id && *index < j.outputs.len())
        });
        self.fields
            .retain(|(id, _), _| data.jobs.iter().any(|j| j.id == *id));
        for (position, job) in data.jobs.iter().enumerate() {
            let id = job.id;
            for start in [true, false] {
                let s = self.state.clone();
                let field = self.fields.entry((id, start)).or_insert_with(|| {
                    cx.new(|cx| {
                        TextField::new(cx, move |text, window, cx| {
                            s.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Queue(QueueAction::Range(id, start, text.into())),
                                    window,
                                    cx,
                                )
                            })
                        })
                        .numeric()
                    })
                });
                field.update(cx, |f, _| {
                    f.sync(
                        format!("queue-{id}-{start}"),
                        if start {
                            job.range.start
                        } else {
                            job.range.end
                        }
                        .to_string(),
                        window,
                    )
                });
            }
            let s = self.state.clone();
            let preset_name = self.preset_name.clone();
            let mut row = div()
                .border_b_1()
                .border_color(rgb(ui::BORDER))
                .px_2()
                .py_2()
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(
                            self.button(
                                ("queue-enabled", id),
                                if job.enabled { "Enabled" } else { "Disabled" },
                                QueueAction::Enable(id),
                            )
                            .when(busy, |d| d.opacity(0.4)),
                        )
                        .child(format!("{}  {}", position + 1, job.name))
                        .child(
                            div()
                                .text_color(rgb(ui::MUTED))
                                .child(job.description.clone()),
                        )
                        .child(div().flex_1())
                        .child(
                            self.button(("queue-up", id), "Up", QueueAction::Move(id, -1))
                                .when(busy || position == 0, |d| d.opacity(0.4)),
                        )
                        .child(
                            self.button(("queue-down", id), "Down", QueueAction::Move(id, 1))
                                .when(busy || position + 1 == data.jobs.len(), |d| d.opacity(0.4)),
                        )
                        .child(
                            self.button(("queue-retry", id), "Retry", QueueAction::Retry(id))
                                .when(busy, |d| d.opacity(0.4)),
                        )
                        .child(
                            self.button(("queue-remove", id), "Remove", QueueAction::Remove(id))
                                .when(busy, |d| d.opacity(0.4)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pl_6()
                        .child("Start")
                        .child(div().w(px(70.0)).child(self.fields[&(id, true)].clone()))
                        .child("End")
                        .child(div().w(px(70.0)).child(self.fields[&(id, false)].clone()))
                        .child(
                            self.button(
                                ("queue-output", id),
                                "+ Output",
                                QueueAction::AddOutput(id, formats[0].clone()),
                            )
                            .when(busy, |d| d.opacity(0.4)),
                        )
                        .child(
                            ui::text_button(("queue-save-preset", id), "Save preset")
                                .when(busy, |d| d.opacity(0.4))
                                .on_click(move |_, window, cx| {
                                    let name = preset_name.read(cx).value().to_owned();
                                    s.update(cx, |s, cx| {
                                        s.dispatch(
                                            &Action::Queue(QueueAction::SavePreset(id, name)),
                                            window,
                                            cx,
                                        )
                                    });
                                }),
                        ),
                );
            for (index, output) in job.outputs.iter().enumerate() {
                let message = if active == Some((id, index)) {
                    crate::audio_mix::progress_label(progress)
                        .unwrap_or_else(|| format!("{progress} output frames rendered"))
                } else {
                    output.message.clone()
                };
                row = row
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .pl_6()
                            .py_1()
                            .child(
                                div()
                                    .w(px(80.0))
                                    .text_color(rgb(ui::BLUE))
                                    .child(output.status.label()),
                            )
                            .child(
                                ui::text_button(
                                    gpui::SharedString::from(format!(
                                        "queue-settings-{id}-{index}"
                                    )),
                                    format!("{} ▾", output.spec.format.label()),
                                )
                                .w(px(190.0))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.expanded = if this.expanded == Some((id, index)) {
                                            None
                                        } else {
                                            Some((id, index))
                                        };
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                self.button(
                                    gpui::SharedString::from(format!("queue-path-{id}-{index}")),
                                    crate::media_io::path_string(&output.path)
                                        .unwrap_or_else(|_| output.path.display().to_string()),
                                    QueueAction::Path(id, index),
                                )
                                .when(busy, |d| d.opacity(0.4))
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden(),
                            )
                            .child(
                                self.button(
                                    gpui::SharedString::from(format!(
                                        "queue-delete-output-{id}-{index}"
                                    )),
                                    "Remove",
                                    QueueAction::RemoveOutput(id, index),
                                )
                                .when(busy || job.outputs.len() == 1, |d| d.opacity(0.4)),
                            ),
                    )
                    .when(!message.is_empty(), |row| {
                        row.child(div().pl_6().text_color(rgb(ui::MUTED)).child(message))
                    });
                let settings = &output.spec.settings;
                row = row.child(
                    div()
                        .pl_6()
                        .text_color(rgb(ui::MUTED))
                        .child(settings.summary(output.spec.format)),
                );
                if self.expanded == Some((id, index)) {
                    let mut controls = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .pl_6()
                        .py_1();
                    for field in Field::ALL {
                        let state = self.state.clone();
                        let input =
                            self.output_fields
                                .entry((id, index, field))
                                .or_insert_with(|| {
                                    cx.new(|cx| {
                                        TextField::new(cx, move |text, window, cx| {
                                            state.update(cx, |s, cx| {
                                                s.dispatch(
                                                    &Action::Queue(QueueAction::Settings(
                                                        id,
                                                        index,
                                                        field,
                                                        text.into(),
                                                    )),
                                                    window,
                                                    cx,
                                                )
                                            })
                                        })
                                    })
                                });
                        input.update(cx, |f, _| {
                            f.sync(
                                format!("output-{id}-{index}-{field:?}"),
                                settings.value(field),
                                window,
                            )
                        });
                        controls = controls
                            .child(field.label())
                            .child(div().w(px(112.0)).child(input.clone()));
                    }
                    controls = controls.child(self.button(
                        gpui::SharedString::from(format!("queue-reset-{id}-{index}")),
                        "Reset settings",
                        QueueAction::ResetSettings(id, index),
                    ));
                    row=row.child(controls).child(div().pl_6().text_color(rgb(ui::MUTED)).child("Size: comp / 1920x1080 · FPS: comp / 29.97 · Channels: auto / rgb / rgba / alpha · Quality: auto / crf:18 / kbps:8000 · Encoder: auto / fast / medium / slow · Audio: auto / off · Fonts: fallback (warn) / strict"));
                }
            }
            list = list.child(row);
        }
        for (index, preset) in data.presets.iter().enumerate() {
            list = list.child(
                div()
                    .flex()
                    .gap_2()
                    .px_2()
                    .child(format!("Preset: {}", preset.name))
                    .child(self.button(
                        ("queue-preset-delete", index),
                        "Remove preset",
                        QueueAction::DeletePreset(index),
                    )),
            );
        }
        root = root.child(list);
        if self.preset_open && !busy {
            let mut menu = div()
                .id("queue-presets-menu")
                .absolute()
                .left(px(172.0))
                .top(px(54.0))
                .w(px(270.0))
                .max_h(px(190.0))
                .overflow_y_scroll()
                .bg(rgb(0x2b2b2b))
                .border_1()
                .border_color(rgb(ui::BORDER))
                .shadow_lg()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.preset_open = false;
                    cx.notify();
                }));
            for (index, (label, formats)) in choices.into_iter().enumerate() {
                menu = menu.child(
                    ui::text_button(("queue-preset-option", index), label)
                        .justify_start()
                        .when(index == self.preset_cursor, |d| d.bg(rgb(0x164a7b)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.update(cx, |s, cx| {
                                s.queue_formats = formats.clone();
                                cx.notify();
                            });
                            this.preset_open = false;
                            cx.notify();
                        })),
                );
            }
            root = root.child(gpui::deferred(menu).with_priority(2));
        }
        root
    }
}
