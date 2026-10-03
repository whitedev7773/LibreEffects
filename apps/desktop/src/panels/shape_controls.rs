use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px};
use libre_effects_core::{Command, Content, PropertyPath, ShapeKind, ShapeParam, TrackEdit};

fn parameter(index: usize) -> Option<ShapeParam> {
    match index {
        1 => Some(ShapeParam::StrokeWidth),
        2 => Some(ShapeParam::Roundness),
        4 => Some(ShapeParam::InnerRadius),
        _ => None,
    }
}

pub(crate) struct ShapeControls {
    stroke: Entity<super::shape_stroke::StrokeControls>,
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
}
impl ShapeControls {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let fields = (0..5).map(|index| {
            let edit = state.clone();
            cx.new(|cx| TextField::new(cx, move |text, window, cx| {
                edit.update(cx, |s, cx| {
                    let Some(layer) = s.editor.selected_layer() else {return;};
                    if layer.locked() {return;}
                    let Content::Shape(mut shape) = layer.content().clone() else {return;};
                    let id = layer.id();
                    if let Some(parameter) = parameter(index) {
                        match text.trim().parse::<f64>() {
                            Ok(value) => s.dispatch(&Action::Edit(Command::EditTrack {
                                id, property: PropertyPath::Shape(parameter), edit: TrackEdit::Value { frame: s.frame, value },
                            }), window, cx),
                            Err(_) => { s.status = "Enter a finite shape value".into(); cx.notify(); }
                        }
                        return;
                    }
                    let valid = match index {
                        0 => ui::parse_hex_color(text).map(|v| shape.stroke_color = v).is_ok(),
                        3 => text.trim().parse::<u32>().map(|v| shape.points = v).is_ok(),
                        _ => text.trim().parse::<f64>().map(|v| match index {
                            1 => shape.stroke_width = v,
                            2 => shape.roundness = v,
                            _ => shape.inner_radius = v,
                        }).is_ok(),
                    };
                    if valid && shape.valid() {
                        s.dispatch(&Action::Edit(Command::SetContent {id, content: Content::Shape(shape)}), window, cx);
                    } else {
                        s.status = "Invalid shape value: stroke 0–1024, roundness 0–8192, points 3–128, inner radius 0–100%.".into();
                        cx.notify();
                    }
                });
            }))
        }).collect();
        Self {
            stroke: cx.new(|cx| super::shape_stroke::StrokeControls::new(state.clone(), cx)),
            state,
            fields,
        }
    }
}
impl Render for ShapeControls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().flex().flex_col().gap_1().mt_2();
        let Some(layer) = self.state.read(cx).editor.selected_layer() else {
            return root;
        };
        let Content::Shape(shape) = layer.content().clone() else {
            return root;
        };
        let id = layer.id();
        let frame = self.state.read(cx).frame;
        let path_row = shape.path.as_ref().map(|_| {
            super::path_controls::row(
                &self.state,
                layer,
                libre_effects_core::PathTarget::Shape,
                frame,
            )
        });
        let locked = layer.locked();
        let mut next = shape.clone();
        next.fill = !next.fill;
        let state = self.state.clone();
        root = root.child(
            ui::text_button("shape-fill", if shape.fill { "☑ Fill" } else { "☐ Fill" })
                .justify_start()
                .when(!locked, |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::SetContent {
                                    id,
                                    content: Content::Shape(next.clone()),
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                }),
        );
        if let Some(row) = path_row {
            root = root.child(row);
        }
        if let Some(path) = &shape.path {
            let mut changed = shape.clone();
            changed.path.as_mut().unwrap().closed = !path.closed;
            if path.vertices.len() >= 3 && shape.path_animation.is_default() {
                let state = self.state.clone();
                root = root.child(
                    ui::text_button(
                        "path-closed",
                        if path.closed {
                            "☑ Closed path"
                        } else {
                            "☐ Closed path"
                        },
                    )
                    .when(!locked, |b| {
                        b.on_click(move |_, w, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::SetContent {
                                        id,
                                        content: Content::Shape(changed.clone()),
                                    }),
                                    w,
                                    cx,
                                )
                            });
                        })
                    }),
                );
            }
            root = root.child(div().child(format!(
                "{} vertices · Pen (G) to edit",
                path.vertices.len()
            )));
        }
        let values = [
            format!("{:06X}", shape.stroke_color),
            shape.value_at(ShapeParam::StrokeWidth, frame).to_string(),
            shape.value_at(ShapeParam::Roundness, frame).to_string(),
            shape.points.to_string(),
            shape.value_at(ShapeParam::InnerRadius, frame).to_string(),
        ];
        for (index, label) in [
            "Stroke color",
            "Stroke width",
            "Roundness",
            "Points",
            "Inner radius %",
        ]
        .into_iter()
        .enumerate()
        {
            if (shape.path.is_some() && index >= 2)
                || index == 2 && shape.kind != ShapeKind::RoundedRectangle
                || index == 3 && !matches!(shape.kind, ShapeKind::Polygon | ShapeKind::Star)
                || index == 4 && shape.kind != ShapeKind::Star
            {
                continue;
            }
            self.fields[index].update(cx, |f, _| {
                f.sync(
                    format!("{id}-{index}-{frame}"),
                    values[index].clone(),
                    window,
                )
            });
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(27.0))
                    .child(
                        div()
                            .w(px(105.0))
                            .flex()
                            .items_center()
                            .when_some(parameter(index), |d, p| {
                                d.child(super::shape_values::watch(
                                    &self.state,
                                    &shape,
                                    id,
                                    p,
                                    frame,
                                ))
                            })
                            .child(div().min_w_0().text_size(px(11.0)).child(label)),
                    )
                    .when(index == 0, |d| {
                        d.child(super::color_picker::swatch(
                            "shape-stroke-color",
                            shape.stroke_color,
                            crate::color_edit::Target::Stroke(id),
                            locked,
                            &self.state,
                        ))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(!locked, |d| d.child(self.fields[index].clone()))
                            .when(locked, |d| d.child(values[index].clone())),
                    ),
            );
        }
        root.child(self.stroke.clone())
    }
}
