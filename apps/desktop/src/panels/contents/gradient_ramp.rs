use super::*;
use gpui::{MouseButton, MouseDownEvent, PathBuilder, Point, fill, size};
use libre_effects_core::{ContentsNode, Frame};

#[derive(Clone, Copy, Debug)]
struct Handle {
    parameter: GradientParam,
    position: f64,
    opacity: bool,
    span: Option<(f64, f64)>,
    color: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, Project};
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Default::default()),
            width: 160.,
            height: 100.,
            name: "Gradient".into(),
        })
        .unwrap();
        edit(&mut e, ContentsEdit::Promote);
        edit(
            &mut e,
            ContentsEdit::Add {
                parent: 1,
                kind: ContentsKind::GradientFill {
                    even_odd: false,
                    gradient: Default::default(),
                },
            },
        );
        e.clear_history();
        e
    }
    fn edit(e: &mut Editor, edit: ContentsEdit) {
        e.execute(Command::Contents { id: 1, edit }).unwrap();
    }
    fn node(e: &Editor) -> &ContentsNode {
        let Content::ShapeContents(c) = e.selected_layer().unwrap().content() else {
            panic!()
        };
        c.node(5).unwrap()
    }
    fn value(e: &mut Editor, p: GradientParam, v: f64) {
        edit(
            e,
            ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Gradient(p),
                edit: TrackEdit::Value { frame: 0, value: v },
            },
        );
    }
    #[test]
    fn ramp_hit_testing_uses_independent_rows_stable_ids_and_relative_midpoints() {
        let mut e = scene();
        value(&mut e, GradientParam::ColorPosition(1), 20.);
        value(&mut e, GradientParam::ColorPosition(2), 80.);
        value(&mut e, GradientParam::ColorMidpoint(1), 25.);
        let all = handles(node(&e), 0, None);
        let b = Bounds::new(point(px(100.), px(200.)), size(px(216.), px(64.)));
        let mid = all
            .iter()
            .find(|h| h.parameter == GradientParam::ColorMidpoint(1))
            .unwrap();
        assert_eq!(mid.position, 35.);
        assert_eq!(mid.value(50.), 50.);
        assert_eq!(mid.value(-100.), 1.);
        assert_eq!(mid.value(150.), 99.);
        assert_eq!(
            hit(&all, b, point(px(178.), px(256.))).unwrap().parameter,
            GradientParam::ColorMidpoint(1)
        );
        assert_eq!(
            hit(&all, b, point(px(108.), px(208.))).unwrap().parameter,
            GradientParam::OpacityPosition(3)
        );
        assert!(hit(&all, b, point(px(178.), px(232.))).is_none());
        // A narrow panel has the same percentage mapping and hit radius in pixels.
        for width in [120., 216., 300.] {
            let b = Bounds::new(point(px(40.), px(10.)), size(px(width), px(64.)));
            for h in &all {
                assert!((position(b, at(b, *h).x) - h.position).abs() < 0.0001);
            }
        }
        value(&mut e, GradientParam::ColorPosition(1), 80.);
        let overlap = handles(node(&e), 0, None);
        assert!(!overlap.iter().any(|h| !h.opacity && h.span.is_some()));
        assert_eq!(
            hit(&overlap, b, point(px(268.), px(256.)))
                .unwrap()
                .parameter,
            GradientParam::ColorPosition(2)
        );
        let crossed = handles(node(&e), 0, Some((GradientParam::ColorPosition(1), 90.)));
        let mid = crossed
            .iter()
            .find(|h| !h.opacity && h.span.is_some())
            .unwrap();
        assert_eq!(mid.parameter, GradientParam::ColorMidpoint(2));
        assert_eq!(mid.position, 85.);
    }
    #[test]
    fn ramp_gesture_commits_one_key_and_cancels_stale_context_without_mutating_source() {
        let mut e = scene();
        let p = GradientParam::ColorPosition(1);
        edit(
            &mut e,
            ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Gradient(p),
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            },
        );
        let before = e.project().clone();
        let h = handles(node(&e), 30, None)
            .into_iter()
            .find(|h| h.parameter == p)
            .unwrap();
        let mut d = RampDrag {
            layer: 1,
            item: 5,
            revision: 7,
            frame: 30,
            handle: h,
            grab_offset: 0.,
            original: 0.,
            value: 0.,
        };
        assert!(d.command().is_none());
        for v in [10., 20., 30., 60.] {
            d.value = d.handle.value(v);
        }
        assert_eq!(e.project(), &before);
        e.execute(d.command().unwrap()).unwrap();
        let saved = e.project().clone();
        assert_eq!(node(&e).value_at(ContentsParam::Gradient(p), 15), 30.);
        assert_eq!(
            node(&e).parameters[&ContentsParam::Gradient(p)]
                .keys()
                .len(),
            2
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &saved);
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        let mut state = EditorState::default();
        state.editor = e;
        state.document_revision = 7;
        state.frame = 30;
        assert!(d.current(&state, Some(5)));
        state.frame = 31;
        assert!(!d.current(&state, Some(5)));
        state.frame = 30;
        state.document_revision = 8;
        assert!(!d.current(&state, Some(5)));
        state.document_revision = 7;
        assert!(!d.current(&state, Some(4)));
        state.editor.clear_selection();
        assert!(!d.current(&state, Some(5)));
    }
}
impl Handle {
    fn value(self, position: f64) -> f64 {
        let (lo, hi) = self.parameter.bounds();
        match self.span {
            Some((a, b)) => ((position - a) * 100. / (b - a)).clamp(lo, hi),
            None => position.clamp(lo, hi),
        }
    }
}
fn handles(node: &ContentsNode, frame: Frame, draft: Option<(GradientParam, f64)>) -> Vec<Handle> {
    let Some(g) = node.kind.gradient() else {
        return vec![];
    };
    let value = |p| {
        draft.filter(|(q, _)| *q == p).map_or_else(
            || node.value_at(ContentsParam::Gradient(p), frame),
            |(_, v)| v,
        )
    };
    let mut result = vec![];
    for (opacity, ids) in [(true, &g.opacities), (false, &g.colors)] {
        let mut stops = ids
            .iter()
            .map(|&id| {
                let parameter = if opacity {
                    GradientParam::OpacityPosition(id)
                } else {
                    GradientParam::ColorPosition(id)
                };
                let color = if opacity {
                    let v = (value(GradientParam::Opacity(id)) * 2.55).round() as u32;
                    v * 0x010101
                } else {
                    g.color_at(node, id, frame).unwrap_or(0)
                };
                Handle {
                    parameter,
                    position: value(parameter),
                    opacity,
                    span: None,
                    color,
                }
            })
            .collect::<Vec<_>>();
        stops.sort_by(|a, b| a.position.total_cmp(&b.position));
        for pair in stops.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if b.position - a.position <= 1e-6 {
                continue;
            }
            let id = a.parameter.stop().unwrap();
            let parameter = if opacity {
                GradientParam::OpacityMidpoint(id)
            } else {
                GradientParam::ColorMidpoint(id)
            };
            result.push(Handle {
                parameter,
                position: a.position + (b.position - a.position) * value(parameter) / 100.,
                opacity,
                span: Some((a.position, b.position)),
                color: ui::TEXT,
            });
        }
        result.extend(stops);
    }
    result
}
fn position(bounds: Bounds<Pixels>, x: Pixels) -> f64 {
    (f32::from(x - bounds.left() - px(8.)) / (f32::from(bounds.size.width) - 16.).max(1.)) as f64
        * 100.
}
fn at(bounds: Bounds<Pixels>, h: Handle) -> Point<Pixels> {
    point(
        bounds.left() + px(8.) + (bounds.size.width - px(16.)) * h.position as f32 / 100.,
        bounds.top() + px(if h.opacity { 8. } else { 56. }),
    )
}
fn hit(handles: &[Handle], bounds: Bounds<Pixels>, p: Point<Pixels>) -> Option<Handle> {
    // Stops win over nearby diamonds; reverse order matches the paint stack for coincident stops.
    [false, true].into_iter().find_map(|midpoint| {
        handles.iter().rev().copied().find(|h| {
            let center = at(bounds, *h);
            h.span.is_some() == midpoint
                && f32::from(center.x - p.x).abs() <= if midpoint { 5. } else { 7. }
                && f32::from(center.y - p.y).abs() <= 8.
        })
    })
}
pub(super) struct RampDrag {
    layer: u64,
    item: u64,
    revision: u64,
    frame: Frame,
    handle: Handle,
    grab_offset: f64,
    original: f64,
    value: f64,
}
impl RampDrag {
    pub(super) fn current(&self, state: &EditorState, item: Option<u64>) -> bool {
        self.revision == state.document_revision
            && self.frame == state.frame
            && state.editor.selected() == Some(self.layer)
            && item == Some(self.item)
    }
    fn command(&self) -> Option<Command> {
        ((self.value - self.original).abs() > 1e-8).then(|| Command::Contents {
            id: self.layer,
            edit: ContentsEdit::Track {
                item: self.item,
                parameter: ContentsParam::Gradient(self.handle.parameter),
                edit: TrackEdit::Value {
                    frame: self.frame,
                    value: self.value,
                },
            },
        })
    }
}
impl ContentsControls {
    fn ramp_node(&self, cx: &Context<Self>) -> Option<(u64, ContentsNode, Frame)> {
        let s = self.state.read(cx);
        let layer = s.editor.selected_layer()?;
        if layer.locked() {
            return None;
        }
        let Content::ShapeContents(c) = layer.content() else {
            return None;
        };
        let node = c.node(self.selected?)?;
        node.kind.gradient()?;
        Some((layer.id(), node.clone(), s.frame))
    }
    fn ramp_apply(&mut self, id: u64, edit: ContentsEdit, w: &mut Window, cx: &mut Context<Self>) {
        self.ramp_drag = None;
        self.state.update(cx, |s, cx| {
            s.dispatch(&Action::Edit(Command::Contents { id, edit }), w, cx)
        });
        cx.notify();
    }
    fn ramp_down(&mut self, e: &MouseDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        TextField::commit_active(w, cx);
        let Some((id, node, frame)) = self.ramp_node(cx) else {
            return;
        };
        let Some(bounds) = self.ramp_bounds.get() else {
            return;
        };
        w.focus(&self.ramp_focus);
        if let Some(h) = hit(&handles(&node, frame, None), bounds, e.position) {
            self.gradient_stop = h.parameter.stop();
            self.ramp_selected = Some(h.parameter);
            let original = node.value_at(ContentsParam::Gradient(h.parameter), frame);
            self.ramp_drag = Some(RampDrag {
                layer: id,
                item: node.id,
                revision: self.state.read(cx).document_revision,
                frame,
                handle: h,
                grab_offset: position(bounds, e.position.x) - h.position,
                original,
                value: original,
            });
        } else {
            let y = f32::from(e.position.y - bounds.top());
            if !(0. ..=16.).contains(&y) && !(48. ..=64.).contains(&y) {
                return;
            }
            let opacity = y < 20.;
            let g = node.kind.gradient().unwrap();
            if (if opacity {
                g.opacities.len()
            } else {
                g.colors.len()
            }) >= ShapeGradient::MAX_STOPS
            {
                return;
            }
            self.ramp_apply(
                id,
                ContentsEdit::AddGradientStop {
                    item: node.id,
                    opacity,
                    position: position(bounds, e.position.x).clamp(0., 100.),
                    frame,
                },
                w,
                cx,
            );
            if let Some((_, node, _)) = self.ramp_node(cx) {
                let g = node.kind.gradient().unwrap();
                self.gradient_stop = if opacity {
                    g.opacities.last()
                } else {
                    g.colors.last()
                }
                .copied();
                self.ramp_selected = self.gradient_stop.map(|id| {
                    if opacity {
                        GradientParam::OpacityPosition(id)
                    } else {
                        GradientParam::ColorPosition(id)
                    }
                });
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn ramp_move(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(bounds) = self.ramp_bounds.get() else {
            return;
        };
        if let Some(d) = &mut self.ramp_drag {
            d.value = d.handle.value(position(bounds, x) - d.grab_offset);
            cx.notify();
        }
    }
    fn ramp_up(&mut self, x: Pixels, w: &mut Window, cx: &mut Context<Self>) {
        self.ramp_move(x, cx);
        if let Some(d) = self.ramp_drag.take() {
            if d.current(self.state.read(cx), self.selected)
                && let Some(command) = d.command()
            {
                self.state
                    .update(cx, |s, cx| s.dispatch(&Action::Edit(command), w, cx));
            }
            cx.notify();
            cx.stop_propagation();
        }
    }
    fn ramp_key(&mut self, e: &gpui::KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if e.keystroke.modifiers.control
            || e.keystroke.modifiers.alt
            || e.keystroke.modifiers.platform
        {
            return;
        }
        if e.keystroke.key == "escape" {
            self.ramp_drag = None;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        let Some((id, node, frame)) = self.ramp_node(cx) else {
            return;
        };
        let all = handles(&node, frame, None);
        self.ramp_drag = None;
        let selected = self
            .ramp_selected
            .filter(|p| p.stop() == self.gradient_stop)
            .or_else(|| {
                all.iter()
                    .find(|h| h.span.is_none() && h.parameter.stop() == self.gradient_stop)
                    .map(|h| h.parameter)
            });
        let index = all
            .iter()
            .position(|h| Some(h.parameter) == selected)
            .unwrap_or(0);
        let Some(h) = all.get(index).copied() else {
            return;
        };
        let current = node.value_at(ContentsParam::Gradient(h.parameter), frame);
        let step = if e.keystroke.modifiers.shift { 10. } else { 1. };
        let key = e.keystroke.key.as_str();
        match key {
            "up" | "down" => {
                let index = if key == "up" {
                    (index + all.len() - 1) % all.len()
                } else {
                    (index + 1) % all.len()
                };
                self.ramp_selected = Some(all[index].parameter);
                self.gradient_stop = all[index].parameter.stop();
            }
            "left" | "right" | "home" | "end" => {
                let (lo, hi) = h.parameter.bounds();
                let value = match key {
                    "home" => lo,
                    "end" => hi,
                    "left" => (current - step).clamp(lo, hi),
                    _ => (current + step).clamp(lo, hi),
                };
                if value != current {
                    self.ramp_apply(
                        id,
                        ContentsEdit::Track {
                            item: node.id,
                            parameter: ContentsParam::Gradient(h.parameter),
                            edit: TrackEdit::Value { frame, value },
                        },
                        w,
                        cx,
                    );
                }
            }
            "delete" | "backspace" if h.span.is_none() => {
                let g = node.kind.gradient().unwrap();
                if (if h.opacity {
                    g.opacities.len()
                } else {
                    g.colors.len()
                }) > 2
                {
                    self.ramp_apply(
                        id,
                        ContentsEdit::RemoveGradientStop {
                            item: node.id,
                            stop: h.parameter.stop().unwrap(),
                        },
                        w,
                        cx,
                    );
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn gradient_ramp(
        &mut self,
        node: &ContentsNode,
        frame: Frame,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let draft = self
            .ramp_drag
            .as_ref()
            .map(|d| (d.handle.parameter, d.value));
        let g = node.kind.gradient().unwrap();
        let samples = if let Some((p, v)) = draft {
            g.preview_edit(node, frame, 256, p, v)
        } else {
            g.preview(node, frame, 256)
        };
        let all = handles(node, frame, draft);
        let selected = self
            .ramp_selected
            .filter(|p| p.stop() == self.gradient_stop)
            .or_else(|| {
                all.iter()
                    .find(|h| h.span.is_none() && h.parameter.stop() == self.gradient_stop)
                    .map(|h| h.parameter)
            });
        let bounds = self.ramp_bounds.clone();
        let owner = cx.entity();
        let caption = if let Some((p, v)) = draft {
            format!("{}: {v:.1}", p.label())
        } else {
            "Above: opacity · Below: color · ◆ midpoint".into()
        };
        div().flex().flex_col().gap_1()
            .child(div().text_color(rgb(ui::MUTED)).text_size(px(10.)).child(caption))
            .child(div().id("contents-gradient-ramp").track_focus(&self.ramp_focus).tab_index(0)
                .when(locked,|d|d.opacity(0.4)).cursor_crosshair()
                .on_mouse_down(MouseButton::Left,cx.listener(Self::ramp_down))
                .on_key_down(cx.listener(Self::ramp_key))
                .tooltip(|_,cx|cx.new(|_|ui::Tip("Drag a stop or diamond; click an empty stop row to add. Up/Down selects; Left/Right adjusts (Shift: 10); Home/End; Delete removes a stop; Escape cancels a drag. Release applies one edit.".into())).into())
                .child(canvas(move |b,_,_|bounds.set(Some(b)),move |b,_,w,_| {
                    let width=(f32::from(b.size.width)-16.).ceil().max(1.) as usize;
                    for x in 0..width {
                        let c=samples[x*255/(width-1).max(1)];
                        for row in 0..3 {
                            let bg=if (x/8+row)%2==0 {85.} else {153.};
                            let color=c[..3].iter().fold(0u32,|acc,v|(acc<<8)|(v*255.*c[3]+bg*(1.-c[3])).round() as u32);
                            w.paint_quad(fill(Bounds::new(point(b.left()+px(8.+x as f32),b.top()+px(20.+row as f32*8.)),size(px(1.5),px(8.5))),rgb(color)));
                        }
                    }
                    for h in &all {
                        let p=at(b,*h);
                        let color=if selected==Some(h.parameter) {ui::BLUE} else {0x999999};
                        if h.span.is_some() {
                            let mut path=PathBuilder::fill();
                            path.move_to(p+point(px(0.),px(-4.)));path.line_to(p+point(px(4.),px(0.)));
                            path.line_to(p+point(px(0.),px(4.)));path.line_to(p+point(px(-4.),px(0.)));path.close();
                            if let Ok(path)=path.build(){w.paint_path(path,rgb(color));}
                        } else {
                            w.paint_quad(fill(Bounds::new(p-point(px(6.),px(6.)),size(px(12.),px(12.))),rgb(color)));
                            w.paint_quad(fill(Bounds::new(p-point(px(4.),px(4.)),size(px(8.),px(8.))),rgb(h.color)));
                        }
                    }
                    let moving=owner.clone();
                    w.on_mouse_event(move |e:&gpui::MouseMoveEvent,phase,_,cx| {
                        if phase.bubble() && e.pressed_button==Some(MouseButton::Left) {
                            moving.update(cx,|this,cx|this.ramp_move(e.position.x,cx));
                        }
                    });
                    let ending=owner.clone();
                    w.on_mouse_event(move |e:&gpui::MouseUpEvent,phase,w,cx| {
                        if phase.bubble() && e.button==MouseButton::Left {
                            ending.update(cx,|this,cx|this.ramp_up(e.position.x,w,cx));
                        }
                    });
                }).w_full().h(px(64.))))
    }
}
