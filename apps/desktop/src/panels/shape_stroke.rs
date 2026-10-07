use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, Pixels, Window, anchored, canvas, deferred, div, point,
    prelude::*, px, rgb,
};
use libre_effects_core::{
    Command, Content, Frame, LayerId, Project, PropertyPath, ShapeParam, ShapeStroke, StrokeCap,
    StrokeJoin, TrackEdit,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy)]
enum Edit {
    Cap(StrokeCap),
    Join(StrokeJoin),
    Number(usize, f64),
    Add,
    Remove,
}
fn number_parameter(index: usize) -> Option<ShapeParam> {
    match index {
        0 => Some(ShapeParam::MiterLimit),
        1 => Some(ShapeParam::DashOffset),
        2..=17 => Some(ShapeParam::DashLength((index - 2) as u8)),
        _ => None,
    }
}
fn command_at(project: &Project, id: LayerId, edit: Edit, frame: Frame) -> Result<Command, String> {
    let l = project
        .composition()
        .layer(id)
        .ok_or("Shape no longer exists")?;
    if l.locked() {
        return Err("Unlock the layer before editing its stroke".into());
    }
    let Content::Shape(mut shape) = l.content().clone() else {
        return Err("Select a shape layer".into());
    };
    if let Edit::Number(index, value) = edit {
        let parameter = number_parameter(index)
            .filter(|p| shape.has_parameter(*p))
            .ok_or("Dash no longer exists")?;
        if !value.is_finite() || !(parameter.bounds().0..=parameter.bounds().1).contains(&value) {
            return Err("Miter: 1–1024; dash/gap: 0–8192 px; offset: ±32768 px.".into());
        }
        return Ok(Command::EditTrack {
            id,
            property: PropertyPath::Shape(parameter),
            edit: TrackEdit::Value { frame, value },
        });
    }
    let style = &mut shape.stroke_style;
    match edit {
        Edit::Cap(v) => style.cap = v,
        Edit::Join(v) => style.join = v,
        Edit::Add if style.dashes.len() < ShapeStroke::MAX_DASHES => style.dashes.push(10.0),
        Edit::Remove if !style.dashes.is_empty() => {
            style.dashes.pop();
            shape
                .parameters
                .remove(&ShapeParam::DashLength(style.dashes.len() as u8));
        }
        _ => return Err("Stroke supports up to 16 dash/gap lengths".into()),
    }
    if !shape.valid() {
        return Err("Miter: 1–1024; dash/gap: 0–8192 px; offset: ±32768 px.".into());
    }
    Ok(Command::SetContent {
        id,
        content: Content::Shape(shape),
    })
}
fn apply(
    s: &mut EditorState,
    id: LayerId,
    edit: Edit,
    w: &mut Window,
    cx: &mut Context<EditorState>,
) {
    match command_at(s.editor.project(), id, edit, s.frame) {
        Ok(c) => s.dispatch(&Action::Edit(c), w, cx),
        Err(e) => {
            s.status = e;
            cx.notify();
        }
    }
}
struct Menu {
    cap: bool,
    cursor: usize,
    id: LayerId,
    revision: u64,
}
pub(super) struct StrokeControls {
    state: Entity<EditorState>,
    fields: Vec<Entity<TextField>>,
    focus: [FocusHandle; 2],
    bounds: [Rc<Cell<Option<Bounds<Pixels>>>>; 2],
    menu: Option<Menu>,
    watches: Option<Vec<gpui::Subscription>>,
}
impl StrokeControls {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            let s = this.state.read(cx);
            if this.menu.as_ref().is_some_and(|m| {
                m.revision != s.document_revision || s.editor.selected() != Some(m.id)
            }) {
                this.menu = None;
            }
            cx.notify();
        })
        .detach();
        let fields = (0..ShapeStroke::MAX_DASHES + 2)
            .map(|index| {
                let state = state.clone();
                cx.new(|cx| {
                    TextField::new(cx, move |text, w, cx| {
                        state.update(cx, |s, cx| {
                            let Some(id) = s.editor.selected() else {
                                return;
                            };
                            match text.trim().parse::<f64>() {
                                Ok(value) => apply(s, id, Edit::Number(index, value), w, cx),
                                Err(_) => {
                                    s.status = "Enter a finite stroke value".into();
                                    cx.notify();
                                }
                            }
                        })
                    })
                    .numeric()
                })
            })
            .collect();
        Self {
            state,
            fields,
            focus: [cx.focus_handle(), cx.focus_handle()],
            bounds: Default::default(),
            menu: None,
            watches: None,
        }
    }
    fn choose(&mut self, index: usize, w: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.menu.take() else {
            return;
        };
        let s = self.state.read(cx);
        if s.document_revision != m.revision || s.editor.selected() != Some(m.id) {
            cx.notify();
            return;
        }
        let edit = if m.cap {
            Edit::Cap(StrokeCap::ALL[index])
        } else {
            Edit::Join(StrokeJoin::ALL[index])
        };
        self.state.update(cx, |s, cx| apply(s, m.id, edit, w, cx));
        cx.notify();
    }
}
impl Render for StrokeControls {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.watches.is_none() {
            let mut watches: Vec<_> = self
                .focus
                .iter()
                .map(|f| {
                    cx.on_blur(f, w, |this, _, cx| {
                        this.menu = None;
                        cx.notify();
                    })
                })
                .collect();
            watches.push(cx.observe_window_activation(w, |this, w, cx| {
                if !w.is_window_active() {
                    this.menu = None;
                    cx.notify();
                }
            }));
            self.watches = Some(watches);
        }
        let mut root = div().flex().flex_col().gap_1();
        let s = self.state.read(cx);
        let Some(l) = s.editor.selected_layer() else {
            return root;
        };
        let Content::Shape(shape) = l.content() else {
            return root;
        };
        let id = l.id();
        let fill_color = l.color();
        let locked = l.locked();
        let frame = s.frame;
        let shape = shape.clone();
        let style = shape.stroke_style.clone();
        for cap in [true, false] {
            let at = usize::from(!cap);
            let label = if cap {
                style.cap.label()
            } else {
                style.join.label()
            };
            let current = if cap {
                StrokeCap::ALL.iter().position(|v| *v == style.cap).unwrap()
            } else {
                StrokeJoin::ALL
                    .iter()
                    .position(|v| *v == style.join)
                    .unwrap()
            };
            let bounds = self.bounds[at].clone();
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(27.0))
                    .child(
                        div()
                            .w(px(105.0))
                            .child(if cap { "Line Cap" } else { "Line Join" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .relative()
                            .child(
                                ui::text_button(("shape-stroke-picker", at), format!("{label} ▾"))
                                    .track_focus(&self.focus[at])
                                    .w_full()
                                    .justify_start()
                                    .when(locked, |b| b.opacity(0.4))
                                    .on_click(cx.listener(move |this, e, w, cx| {
                                        if !locked {
                                            TextField::commit_active(w, cx);
                                            w.focus(&this.focus[at]);
                                            if this.menu.as_ref().is_some_and(|m| m.cap == cap) {
                                                if matches!(e, gpui::ClickEvent::Keyboard(_)) {
                                                    let i = this.menu.as_ref().unwrap().cursor;
                                                    this.choose(i, w, cx);
                                                } else {
                                                    this.menu = None;
                                                }
                                            } else {
                                                this.menu = Some(Menu {
                                                    cap,
                                                    cursor: current,
                                                    id,
                                                    revision: this.state.read(cx).document_revision,
                                                });
                                            }
                                            cx.notify();
                                        }
                                        cx.stop_propagation();
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, e: &gpui::KeyDownEvent, _, cx| {
                                            if locked || e.keystroke.modifiers.modified() {
                                                return;
                                            }
                                            match e.keystroke.key.as_str() {
                                                "escape" => this.menu = None,
                                                "enter" | "space" => {}
                                                "up" | "down" => {
                                                    if let Some(m) = &mut this.menu {
                                                        m.cursor = (m.cursor
                                                            + if e.keystroke.key == "up" {
                                                                2
                                                            } else {
                                                                1
                                                            })
                                                            % 3;
                                                    } else {
                                                        this.menu = Some(Menu {
                                                            cap,
                                                            cursor: current,
                                                            id,
                                                            revision: this
                                                                .state
                                                                .read(cx)
                                                                .document_revision,
                                                        });
                                                    }
                                                }
                                                _ => return,
                                            }
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    )),
                            )
                            .child(
                                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| ())
                                    .absolute()
                                    .size_full(),
                            ),
                    ),
            );
        }
        for index in 0..style.dashes.len() + 2 {
            if (index == 0 && style.join != StrokeJoin::Miter)
                || (index == 1 && style.dashes.is_empty())
            {
                continue;
            }
            let (label, value) = match index {
                0 => (
                    "Miter Limit".to_owned(),
                    shape.value_at(ShapeParam::MiterLimit, frame, fill_color),
                ),
                1 => (
                    "Dash Offset".to_owned(),
                    shape.value_at(ShapeParam::DashOffset, frame, fill_color),
                ),
                _ => (
                    format!(
                        "{} {}",
                        if index % 2 == 0 { "Dash" } else { "Gap" },
                        (index - 2) / 2 + 1
                    ),
                    shape.value_at(number_parameter(index).unwrap(), frame, fill_color),
                ),
            };
            self.fields[index].update(cx, |f, _| {
                f.sync(format!("{id}-{index}-{frame}"), value.to_string(), w)
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
                            .child(super::shape_values::watch(
                                &self.state,
                                &shape,
                                id,
                                number_parameter(index).unwrap(),
                                frame,
                            ))
                            .child(div().min_w_0().text_size(px(11.0)).child(label)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(!locked, |d| d.child(self.fields[index].clone()))
                            .when(locked, |d| d.child(value.to_string())),
                    ),
            );
        }
        let state = self.state.clone();
        let remove_state = self.state.clone();
        let add_disabled = locked || style.dashes.len() == ShapeStroke::MAX_DASHES;
        let remove_disabled = locked || style.dashes.is_empty();
        root = root.child(
            div()
                .flex()
                .items_center()
                .child(div().flex_1().child(if style.dashes.is_empty() {
                    "Dashes: solid"
                } else {
                    "Dashes"
                }))
                .child(
                    ui::tool("stroke-add-dash", "plus", "Add dash or gap", false)
                        .when(add_disabled, |b| b.opacity(0.4))
                        .on_click(move |_, w, cx| {
                            if !add_disabled {
                                TextField::commit_active(w, cx);
                                state.update(cx, |s, cx| apply(s, id, Edit::Add, w, cx));
                            }
                        }),
                )
                .child(
                    ui::tool(
                        "stroke-remove-dash",
                        "minus",
                        "Remove last dash or gap",
                        false,
                    )
                    .when(remove_disabled, |b| b.opacity(0.4))
                    .on_click(move |_, w, cx| {
                        if !remove_disabled {
                            TextField::commit_active(w, cx);
                            remove_state.update(cx, |s, cx| apply(s, id, Edit::Remove, w, cx));
                        }
                    }),
                ),
        );
        if let Some(m) = &self.menu {
            let at = usize::from(!m.cap);
            let position = self.bounds[at]
                .get()
                .map_or(point(px(0.), px(0.)), |b| point(b.left(), b.bottom()));
            let mut menu = div()
                .id("shape-stroke-menu")
                .w(px(145.0))
                .py_1()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(ui::BLUE))
                .occlude()
                .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.menu = None;
                    cx.notify();
                }));
            for index in 0..3 {
                let label = if m.cap {
                    StrokeCap::ALL[index].label()
                } else {
                    StrokeJoin::ALL[index].label()
                };
                menu = menu.child(
                    div()
                        .id(("stroke-option", index))
                        .h(px(25.0))
                        .px_2()
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .child(label)
                        .w_full()
                        .justify_start()
                        .when(index == m.cursor, |b| b.bg(rgb(0x344455)))
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.choose(index, w, cx);
                            cx.stop_propagation();
                        })),
                );
            }
            root = root.child(
                deferred(
                    anchored()
                        .position(position)
                        .snap_to_window_with_margin(px(8.0))
                        .child(menu),
                )
                .with_priority(5),
            );
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, PathVertex, Property, Shape, VectorPath};
    fn command(project: &Project, id: LayerId, edit: Edit) -> Result<Command, String> {
        command_at(project, id, edit, 0)
    }
    fn scene(points: &[[f64; 2]], closed: bool) -> Editor {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Stroke QA".into(),
            width: 200,
            height: 200,
            fps: 30,
            duration: 60,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                fill: false,
                stroke_width: 20.0,
                path: Some(VectorPath {
                    vertices: points.iter().copied().map(PathVertex::corner).collect(),
                    closed,
                }),
                ..Default::default()
            }),
            width: 200.,
            height: 200.,
            name: "Path".into(),
        })
        .unwrap();
        e
    }
    fn line() -> Editor {
        scene(&[[40., 100.], [160., 100.]], false)
    }
    fn change(e: &mut Editor, edit: Edit) {
        e.execute(command(e.project(), 1, edit).unwrap()).unwrap();
    }
    fn image(e: &Editor) -> image::RgbaImage {
        crate::rendering::Renderer::new()
            .render(e.project(), 0, 200)
            .unwrap()
    }
    #[test]
    fn caps_and_joins_have_distinct_geometry_and_miter_limit_bevels() {
        let mut e = line();
        let butt = image(&e);
        assert_eq!(butt.get_pixel(31, 100)[3], 0);
        change(&mut e, Edit::Cap(StrokeCap::Round));
        let round = image(&e);
        assert_eq!(round.get_pixel(31, 100)[3], 255);
        assert_eq!(round.get_pixel(31, 91)[3], 0);
        change(&mut e, Edit::Cap(StrokeCap::Square));
        let square = image(&e);
        assert_eq!(square.get_pixel(31, 91)[3], 255);
        let mut e = scene(&[[40., 140.], [100., 40.], [160., 140.]], false);
        let top = |im: &image::RgbaImage| {
            im.enumerate_pixels()
                .filter(|(_, _, p)| p[3] > 128)
                .map(|(_, y, _)| y)
                .min()
                .unwrap()
        };
        change(&mut e, Edit::Join(StrokeJoin::Miter));
        let miter = image(&e);
        change(&mut e, Edit::Join(StrokeJoin::Round));
        let round = image(&e);
        change(&mut e, Edit::Join(StrokeJoin::Bevel));
        let bevel = image(&e);
        assert!(top(&miter) < top(&round) && top(&round) < top(&bevel));
        change(&mut e, Edit::Join(StrokeJoin::Miter));
        change(&mut e, Edit::Number(0, 1.));
        assert_eq!(image(&e), bevel);
    }
    #[test]
    fn dashes_offset_odd_cycles_and_zero_length_round_dots_render() {
        let mut e = line();
        let solid = image(&e);
        change(&mut e, Edit::Add);
        change(&mut e, Edit::Number(2, 0.));
        assert_eq!(image(&e), solid);
        change(&mut e, Edit::Number(2, 20.));
        let dash = image(&e);
        assert_eq!(dash.get_pixel(50, 100)[3], 255);
        assert_eq!(dash.get_pixel(70, 100)[3], 0);
        change(&mut e, Edit::Number(1, 10.));
        let offset = image(&e);
        assert_eq!(offset.get_pixel(55, 100)[3], 0);
        assert_eq!(offset.get_pixel(75, 100)[3], 255);
        change(&mut e, Edit::Add);
        change(&mut e, Edit::Number(2, 0.));
        change(&mut e, Edit::Number(3, 20.));
        change(&mut e, Edit::Number(1, 0.));
        change(&mut e, Edit::Cap(StrokeCap::Round));
        let dots = image(&e);
        assert_eq!(dots.get_pixel(40, 100)[3], 255);
        // Use a wider gap than the cap diameter to leave fully transparent pixels.
        change(&mut e, Edit::Number(3, 30.));
        let dots = image(&e);
        assert_eq!(dots.get_pixel(55, 100)[3], 0);
        let mut e = line();
        for value in [15., 5., 10.] {
            let index = match e.selected_layer().unwrap().content() {
                Content::Shape(s) => s.stroke_style.dashes.len() + 2,
                _ => unreachable!(),
            };
            change(&mut e, Edit::Add);
            change(&mut e, Edit::Number(index, value));
        }
        let odd = image(&e);
        for (index, value) in [(5, 15.), (6, 5.), (7, 10.)] {
            change(&mut e, Edit::Add);
            change(&mut e, Edit::Number(index, value));
        }
        assert_eq!(image(&e), odd);
    }
    #[test]
    fn stroke_edits_preserve_path_and_transform_history_save_preview_and_output() {
        let mut e = line();
        let before = e.project().clone();
        for edit in [
            Edit::Cap(StrokeCap::Round),
            Edit::Join(StrokeJoin::Bevel),
            Edit::Add,
            Edit::Add,
            Edit::Number(2, 30.),
            Edit::Number(3, 15.),
            Edit::Number(1, -5.),
        ] {
            let old = e.project().clone();
            change(&mut e, edit);
            let after = e.project().clone();
            e.undo();
            assert_eq!(e.project(), &old);
            e.redo();
            assert_eq!(e.project(), &after);
        }
        if let (Content::Shape(a), Content::Shape(b)) = (
            before.composition().layer(1).unwrap().content(),
            e.selected_layer().unwrap().content(),
        ) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.path_animation, b.path_animation);
        }
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 50.,
        })
        .unwrap();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let r = crate::rendering::Renderer::new();
        let preview = r.render(&saved, 0, 200).unwrap();
        assert_eq!(preview, r.render_output(&saved, 0, 200, 200).unwrap());
        assert!(preview.pixels().any(|p| p[3] == 128));
        assert!(preview.pixels().all(|p| p[3] <= 128));
        for _ in 0..2 {
            change(&mut e, Edit::Remove);
        }
        assert!(
            matches!(e.selected_layer().unwrap().content(),Content::Shape(s) if s.stroke_style.dashes.is_empty())
        );
    }
    #[test]
    fn stroke_ui_rejects_stale_fields_invalid_limits_locked_and_nonshape_layers() {
        let mut e = line();
        for edit in [
            Edit::Number(2, 5.),
            Edit::Remove,
            Edit::Number(0, 0.),
            Edit::Number(1, f64::INFINITY),
        ] {
            assert!(command(e.project(), 1, edit).is_err());
        }
        for _ in 0..ShapeStroke::MAX_DASHES {
            change(&mut e, Edit::Add);
        }
        assert!(command(e.project(), 1, Edit::Add).is_err());
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(command(e.project(), 1, Edit::Cap(StrokeCap::Round)).is_err());
        e.execute(Command::AddRectangle).unwrap();
        assert!(command(e.project(), 2, Edit::Add).is_err());
        assert!(command(e.project(), 999, Edit::Add).is_err());
    }

    #[test]
    fn animated_stroke_width_and_offset_render_exact_interior_pixels() {
        let mut e = line();
        change(&mut e, Edit::Add);
        change(&mut e, Edit::Number(2, 20.));
        for (parameter, value) in [
            (ShapeParam::StrokeWidth, 40.),
            (ShapeParam::DashOffset, 20.),
        ] {
            e.execute(Command::EditShape {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
            e.execute(Command::EditShape {
                id: 1,
                parameter,
                edit: TrackEdit::Value { frame: 40, value },
            })
            .unwrap();
        }
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let r = crate::rendering::Renderer::new();
        for frame in [0, 10, 20, 30, 40] {
            let im = r.render(&saved, frame, 200).unwrap();
            assert_eq!(im, r.render_output(&saved, frame, 200, 200).unwrap());
            let width = 20. + frame as f64 / 2.;
            let offset = frame as f64 / 2.;
            for x in 42..158 {
                let phase = (x as f64 + 0.5 - 40. + offset) % 40.;
                if (phase - 20.).abs() < 2. || phase < 2. || phase > 38. {
                    continue;
                }
                for y in 75..125 {
                    let distance = (y as f64 + 0.5 - 100.).abs();
                    if (distance - width / 2.).abs() < 2. {
                        continue;
                    }
                    let expected = if phase < 20. && distance < width / 2. {
                        255
                    } else {
                        0
                    };
                    assert_eq!(im.get_pixel(x, y)[3], expected, "frame {frame}, ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn removing_animated_gap_is_one_undo_and_readding_does_not_revive_keys() {
        let mut e = line();
        change(&mut e, Edit::Add);
        change(&mut e, Edit::Add);
        let dash = ShapeParam::DashLength(0);
        let gap = ShapeParam::DashLength(1);
        for p in [dash, gap] {
            e.execute(Command::EditShape {
                id: 1,
                parameter: p,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
            e.execute(Command::EditShape {
                id: 1,
                parameter: p,
                edit: TrackEdit::Value {
                    frame: 20,
                    value: 50.,
                },
            })
            .unwrap();
        }
        let before = e.project().clone();
        change(&mut e, Edit::Remove);
        assert!(
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Shape(gap))
                .is_none()
        );
        assert!(
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Shape(dash))
                .is_some()
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        change(&mut e, Edit::Add);
        assert!(
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Shape(gap))
                .is_none()
        );
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track_value(PropertyPath::Shape(gap), 20),
            Some(10.)
        );
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(saved, *e.project());
        assert!(command_at(&saved, 1, Edit::Number(usize::MAX, 5.), 0).is_err());
        change(&mut e, Edit::Cap(StrokeCap::Round));
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Shape(dash))
                .unwrap()
                .keys()
                .len(),
            2
        );
    }

    #[test]
    fn animated_dash_and_gap_lengths_follow_independent_pattern_calculation() {
        let mut e = line();
        change(&mut e, Edit::Add);
        change(&mut e, Edit::Add);
        for (p, end) in [
            (ShapeParam::DashLength(0), 40.),
            (ShapeParam::DashLength(1), 30.),
        ] {
            e.execute(Command::EditShape {
                id: 1,
                parameter: p,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
            e.execute(Command::EditShape {
                id: 1,
                parameter: p,
                edit: TrackEdit::Value {
                    frame: 40,
                    value: end,
                },
            })
            .unwrap();
        }
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let r = crate::rendering::Renderer::new();
        for frame in [0, 10, 20, 30, 40] {
            let im = r.render(&saved, frame, 200).unwrap();
            assert_eq!(im, r.render_output(&saved, frame, 200, 200).unwrap());
            let dash = 10. + 30. * frame as f64 / 40.;
            let gap = 10. + 20. * frame as f64 / 40.;
            let mut count = 0;
            for x in 42..158 {
                let phase = (x as f64 + 0.5 - 40.) % (dash + gap);
                if phase < 2. || (phase - dash).abs() < 2. || phase > dash + gap - 2. {
                    continue;
                }
                assert_eq!(
                    im.get_pixel(x, 100)[3],
                    if phase < dash { 255 } else { 0 },
                    "frame {frame} at x={x}"
                );
                count += 1;
            }
            assert!(count > 55);
        }
    }
}
