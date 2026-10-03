use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{Context, Entity, Window, div, prelude::*, px};
use libre_effects_core::{
    Command, Content, PropertyPath, ShapeKind, ShapePaint, ShapeParam, TrackEdit,
};

fn parameter(index: usize) -> Option<ShapeParam> {
    match index {
        1 => Some(ShapeParam::StrokeWidth),
        2 => Some(ShapeParam::Roundness),
        3 => Some(ShapeParam::Points),
        4 => Some(ShapeParam::InnerRadius),
        5 => Some(ShapeParam::FillOpacity),
        6 => Some(ShapeParam::StrokeOpacity),
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
        let fields = (0..7)
            .map(|index| {
                let edit = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, window, cx| {
                        edit.update(cx, |s, cx| {
                            let Some(layer) = s.editor.selected_layer() else {
                                return;
                            };
                            if layer.locked() {
                                return;
                            }
                            if !matches!(layer.content(), Content::Shape(_)) {
                                return;
                            }
                            let id = layer.id();
                            if index == 0 {
                                let command = ui::parse_hex_color(text)
                                    .map_err(str::to_owned)
                                    .and_then(|color| {
                                        layer.shape_color_command(
                                            ShapePaint::Stroke,
                                            color,
                                            s.frame,
                                        )
                                    });
                                match command {
                                    Ok(command) => s.dispatch(&Action::Edit(command), window, cx),
                                    Err(error) => {
                                        s.status = error;
                                        cx.notify();
                                    }
                                }
                                return;
                            }
                            if let Some(parameter) = parameter(index) {
                                match text.trim().parse::<f64>() {
                                    Ok(value) => s.dispatch(
                                        &Action::Edit(Command::EditTrack {
                                            id,
                                            property: PropertyPath::Shape(parameter),
                                            edit: TrackEdit::Value {
                                                frame: s.frame,
                                                value,
                                            },
                                        }),
                                        window,
                                        cx,
                                    ),
                                    Err(_) => {
                                        s.status = "Enter a finite shape value".into();
                                        cx.notify();
                                    }
                                }
                                return;
                            }
                        });
                    })
                })
            })
            .collect();
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
        let fill_color = layer.color();
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
        if shape.path.is_none() {
            let state = self.state.clone();
            root = root.child(
                ui::text_button("shape-to-path", "Convert To Bezier Path")
                    .tooltip(|_, cx| cx.new(|_| ui::Tip("Freeze geometry at current time; replaces Points/Radius/Roundness animation. Paint animation is retained. Undo restores the original.".into())).into())
                    .when(!locked, |b| b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(&Action::Edit(Command::ConvertShapeToPath { id, frame: s.frame }), w, cx);
                        });
                    })),
            );
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
            format!(
                "{:06X}",
                shape.paint_color_at(ShapePaint::Stroke, fill_color, frame)
            ),
            shape
                .value_at(ShapeParam::StrokeWidth, frame, fill_color)
                .to_string(),
            shape
                .value_at(ShapeParam::Roundness, frame, fill_color)
                .to_string(),
            shape
                .value_at(ShapeParam::Points, frame, fill_color)
                .to_string(),
            shape
                .value_at(ShapeParam::InnerRadius, frame, fill_color)
                .to_string(),
            shape
                .value_at(ShapeParam::FillOpacity, frame, fill_color)
                .to_string(),
            shape
                .value_at(ShapeParam::StrokeOpacity, frame, fill_color)
                .to_string(),
        ];
        for (index, label) in [
            (5, "Fill opacity"),
            (0, "Stroke color"),
            (1, "Stroke width"),
            (6, "Stroke opacity"),
            (2, "Roundness"),
            (3, "Points"),
            (4, "Inner radius %"),
        ] {
            if (shape.path.is_some() && (2..=4).contains(&index))
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
                            .when(index == 0, |d| {
                                d.child(super::shape_values::color_watch(
                                    &self.state,
                                    &shape,
                                    id,
                                    ShapePaint::Stroke,
                                    frame,
                                ))
                            })
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
                            shape.paint_color_at(ShapePaint::Stroke, fill_color, frame),
                            crate::color_edit::Target::Shape(id, ShapePaint::Stroke),
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

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, Project, Property, Shape};

    #[test]
    fn converted_shapes_preserve_preview_output_and_keep_paint_animation() {
        let renderer = crate::rendering::Renderer::new();
        for kind in ShapeKind::ALL {
            for stroke in [0., 8.] {
                let mut e = Editor::default();
                e.execute(Command::ConfigureComposition {
                    name: "Conversion".into(),
                    width: 300,
                    height: 200,
                    fps: 30,
                    duration: 90,
                })
                .unwrap();
                e.execute(Command::AddContent {
                    content: Content::Shape(Shape {
                        kind,
                        points: 7,
                        roundness: 24.,
                        stroke_width: stroke,
                        ..Default::default()
                    }),
                    width: 200.,
                    height: 120.,
                    name: "Shape".into(),
                })
                .unwrap();
                e.execute(Command::SetColor {
                    id: 1,
                    color: 0x204080,
                })
                .unwrap();
                for edit in [
                    TrackEdit::ToggleAnimation { frame: 0 },
                    TrackEdit::Value {
                        frame: 60,
                        value: 30.,
                    },
                ] {
                    e.execute(Command::EditShape {
                        id: 1,
                        parameter: ShapeParam::FillOpacity,
                        edit,
                    })
                    .unwrap();
                }
                if matches!(kind, ShapeKind::Star | ShapeKind::Polygon) {
                    e.execute(Command::EditShape {
                        id: 1,
                        parameter: ShapeParam::Points,
                        edit: TrackEdit::Value {
                            frame: 0,
                            value: 7.25,
                        },
                    })
                    .unwrap();
                }
                let before = e.project().clone();
                e.execute(Command::ConvertShapeToPath { id: 1, frame: 30 })
                    .unwrap();
                let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
                for frame in [0, 30, 60] {
                    let original = renderer.render(&before, frame, 300).unwrap();
                    let converted = renderer.render(&saved, frame, 300).unwrap();
                    assert_eq!(
                        converted,
                        renderer.render_output(&saved, frame, 300, 200).unwrap()
                    );
                    // Cubic circle arcs approximate SVG arcs; allow only a small
                    // antialiased boundary difference, including the stroked edge.
                    let alpha_error = original
                        .pixels()
                        .zip(converted.pixels())
                        .map(|(a, b)| (a[3] as f64 - b[3] as f64).abs() / 255.)
                        .sum::<f64>();
                    assert!(
                        alpha_error < 30.,
                        "{kind:?} stroke {stroke} frame {frame}: {alpha_error}"
                    );
                    assert_eq!(original.get_pixel(150, 100), converted.get_pixel(150, 100));
                }
                e.undo();
                assert_eq!(e.project(), &before);
            }
        }
    }

    #[test]
    fn points_animation_matches_closed_form_area_and_preview_output() {
        use std::f64::consts::{PI, TAU};
        let renderer = crate::rendering::Renderer::new();
        for kind in [ShapeKind::Polygon, ShapeKind::Star] {
            let mut e = Editor::default();
            e.execute(Command::ConfigureComposition {
                name: "Polystar".into(),
                width: 200,
                height: 200,
                fps: 30,
                duration: 90,
            })
            .unwrap();
            e.execute(Command::AddContent {
                content: Content::Shape(Shape {
                    kind,
                    points: 3,
                    ..Default::default()
                }),
                width: 160.,
                height: 120.,
                name: "Shape".into(),
            })
            .unwrap();
            e.execute(Command::SetColor {
                id: 1,
                color: 0x204080,
            })
            .unwrap();
            for edit in [
                TrackEdit::ToggleAnimation { frame: 0 },
                TrackEdit::Value {
                    frame: 60,
                    value: 4.,
                },
            ] {
                e.execute(Command::EditShape {
                    id: 1,
                    parameter: ShapeParam::Points,
                    edit,
                })
                .unwrap();
            }
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            for frame in [0, 15, 30, 45, 60] {
                let im = renderer.render(&saved, frame, 200).unwrap();
                assert_eq!(im, renderer.render_output(&saved, frame, 200, 200).unwrap());
                let points = 3. + frame as f64 / 60.;
                let expected = if kind == ShapeKind::Polygon {
                    let n = points.floor();
                    n / 2. * 80. * 60. * (TAU / n).sin()
                } else if points.fract() == 0. {
                    points * 80. * 60. * 0.5 * (PI / points).sin()
                } else {
                    let f = points.fract();
                    let inner = 0.5;
                    let tip = inner + f * (1. - inner);
                    80. * 60. / 2.
                        * ((2. * points.ceil() - 2.) * inner * (PI / points).sin()
                            + 2. * inner * tip * (f * PI / points).sin())
                };
                let area = im.pixels().map(|p| p[3] as f64 / 255.).sum::<f64>();
                assert!(
                    (area - expected).abs() < 12.,
                    "{kind:?} {frame}: area {area}, expected {expected}"
                );
                assert_eq!(im.get_pixel(100, 100).0, [32, 64, 128, 255]);
                assert_eq!(im.get_pixel(10, 10).0, [0, 0, 0, 0]);
            }
            e.execute(Command::EditShape {
                id: 1,
                parameter: ShapeParam::Points,
                edit: TrackEdit::ToggleAnimation { frame: 30 },
            })
            .unwrap();
            let frozen = renderer.render(e.project(), 30, 200).unwrap();
            assert_eq!(frozen, renderer.render(&saved, 30, 200).unwrap());
            assert_eq!(frozen, renderer.render(e.project(), 60, 200).unwrap());
            e.undo();
            assert_eq!(e.project(), &saved);
        }
    }

    #[test]
    fn independent_paint_alpha_composes_before_layer_opacity_and_matches_preview() {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Paint alpha".into(),
            width: 200,
            height: 200,
            fps: 30,
            duration: 60,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                stroke_width: 20.,
                stroke_color: 0xffffff,
                ..Default::default()
            }),
            width: 100.,
            height: 100.,
            name: "Rectangle".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0xffffff,
        })
        .unwrap();
        for (p, a, b) in [
            (ShapeParam::FillOpacity, 100., 0.),
            (ShapeParam::StrokeOpacity, 0., 100.),
        ] {
            for edit in [
                TrackEdit::Value { frame: 0, value: a },
                TrackEdit::ToggleAnimation { frame: 0 },
                TrackEdit::Value {
                    frame: 40,
                    value: b,
                },
            ] {
                e.execute(Command::EditTrack {
                    id: 1,
                    property: PropertyPath::Shape(p),
                    edit,
                })
                .unwrap();
            }
        }
        let renderer = crate::rendering::Renderer::new();
        for layer_opacity in [100., 50.] {
            e.execute(Command::SetValue {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                value: layer_opacity,
            })
            .unwrap();
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            for frame in [0, 10, 20, 30, 40] {
                let im = renderer.render(&saved, frame, 200).unwrap();
                assert_eq!(im, renderer.render_output(&saved, frame, 200, 200).unwrap());
                let stroke = frame as f64 / 40.;
                let fill = 1. - stroke;
                // Center: fill only; outside edge: stroke only; inside edge: stroke over fill.
                for ((x, y), alpha) in [
                    ((100, 100), fill),
                    ((45, 100), stroke),
                    ((55, 100), stroke + fill * (1. - stroke)),
                    ((30, 100), 0.),
                ] {
                    let expected = (alpha * layer_opacity / 100. * 255.).round() as i32;
                    assert!(
                        (i32::from(im.get_pixel(x, y)[3]) - expected).abs() <= 1,
                        "frame {frame} at ({x},{y}): {:?}, expected {expected}",
                        im.get_pixel(x, y)
                    );
                }
            }
        }
    }
}
