//! Gradient points use the same layer space as the effect renderer.
use crate::editor::{EditorState, Tool};
use gpui::{Bounds, PathBuilder, Pixels, Point, Window, fill, point, px, rgb, size};
use libre_effects_core::{
    Affine, Command, CompositionId, EffectEdit, EffectId, EffectKind, EffectParam, LayerId, Project,
};

type Target = (CompositionId, LayerId, EffectId);
#[derive(Clone)]
pub(super) struct Overlay {
    target: Target,
    pub points: [[f64; 2]; 2],
    world: Affine,
}
impl Overlay {
    pub fn current(s: &EditorState, project: &Project) -> Option<Self> {
        let target = s.gradient_controls?;
        if s.tool != Tool::Select
            || s.playing
            || s.text_session.is_some()
            || s.colors.session.is_some()
            || project.active_composition_id() != target.0
            || s.editor.selected() != Some(target.1)
        {
            return None;
        }
        let c = project.composition();
        let l = c.layer(target.1)?;
        let e = l.effect_stack().iter().find(|e| e.id() == target.2)?;
        if l.locked()
            || e.bypassed()
            || !super::controls_active(c, l, s.frame, true)
            || !matches!(
                e.kind(),
                EffectKind::LinearGradient | EffectKind::RadialGradient
            )
        {
            return None;
        }
        let world = c.world_transform(target.1, s.frame)?;
        world.inverse()?;
        Some(Self {
            target,
            world,
            points: [
                [
                    e.value_at(EffectParam::StartX, s.frame),
                    e.value_at(EffectParam::StartY, s.frame),
                ],
                [
                    e.value_at(EffectParam::EndX, s.frame),
                    e.value_at(EffectParam::EndY, s.frame),
                ],
            ],
        })
    }
    pub fn hit(&self, p: [f64; 2], zoom: f32, preferred: usize) -> Option<usize> {
        [preferred, 1 - preferred].into_iter().find(|&i| {
            let q = self.world.point(self.points[i]);
            (q[0] - p[0]).hypot(q[1] - p[1]) * f64::from(zoom) <= 8.0
        })
    }
    pub fn command(&self, points: [[f64; 2]; 2], frame: u32) -> Option<Command> {
        if points == self.points
            || points
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(-32768.0..=32768.0).contains(v))
        {
            return None;
        }
        Some(Command::Batch(
            [
                EffectParam::StartX,
                EffectParam::StartY,
                EffectParam::EndX,
                EffectParam::EndY,
            ]
            .into_iter()
            .zip(points.into_iter().flatten())
            .zip(self.points.into_iter().flatten())
            .filter(|((_, v), old)| v != old)
            .map(|((parameter, value), _)| Command::Effect {
                id: self.target.1,
                edit: EffectEdit::SetValue {
                    effect: self.target.2,
                    parameter,
                    frame,
                    value,
                },
            })
            .collect(),
        ))
    }
    pub fn nudge(&self, index: usize, delta: [f64; 2], both: bool, frame: u32) -> Option<Command> {
        let mut points = self.points;
        for (i, p) in points.iter_mut().enumerate() {
            if both || i == index {
                p[0] += delta[0];
                p[1] += delta[1];
            }
        }
        self.command(points, frame)
    }
}

pub(super) struct Gesture {
    overlay: Overlay,
    pointer: [f64; 2],
    pub points: [[f64; 2]; 2],
    pub index: usize,
    project: Project,
    frame: u32,
    revision: u64,
    preview_zoom: Option<f32>,
    preview_pan: [f32; 2],
    pub bounds: Bounds<Pixels>,
    rulers: bool,
}
impl Gesture {
    pub fn new(
        overlay: Overlay,
        index: usize,
        pointer: [f64; 2],
        bounds: Bounds<Pixels>,
        s: &EditorState,
    ) -> Self {
        Self {
            points: overlay.points,
            overlay,
            index,
            pointer,
            bounds,
            project: s.editor.project().clone(),
            frame: s.frame,
            revision: s.document_revision,
            preview_zoom: s.preview_zoom,
            preview_pan: s.preview_pan,
            rulers: s.viewer.rulers,
        }
    }
    pub fn valid(&self, s: &EditorState) -> bool {
        self.frame == s.frame
            && self.revision == s.document_revision
            && &self.project == s.editor.project()
            && self.preview_zoom == s.preview_zoom
            && self.preview_pan == s.preview_pan
            && self.rulers == s.viewer.rulers
            && Overlay::current(s, s.editor.project())
                .is_some_and(|o| o.target == self.overlay.target)
    }
    pub fn update(&mut self, pointer: [f64; 2], shift: bool, both: bool) {
        let mut d = self
            .overlay
            .world
            .inverse()
            .unwrap()
            .vector([pointer[0] - self.pointer[0], pointer[1] - self.pointer[1]]);
        // Inverting rotated transforms can introduce tiny movement on a fixed axis.
        for value in &mut d {
            if value.abs() < 1e-9 {
                *value = 0.0;
            }
        }
        if shift {
            if d[0].abs() >= d[1].abs() {
                d[1] = 0.0;
            } else {
                d[0] = 0.0;
            }
        }
        self.points = self.overlay.points;
        for (i, p) in self.points.iter_mut().enumerate() {
            if both || i == self.index {
                p[0] += d[0];
                p[1] += d[1];
            }
        }
    }
    pub fn command(&self, s: &EditorState) -> Option<Command> {
        self.valid(s)
            .then(|| self.overlay.command(self.points, self.frame))
            .flatten()
    }
}

pub(super) fn paint(
    overlay: &Overlay,
    active: usize,
    origin: Point<Pixels>,
    zoom: f32,
    window: &mut Window,
) {
    let positions = overlay.points.map(|p| {
        let p = overlay.world.point(p);
        origin + point(px(p[0] as f32 * zoom), px(p[1] as f32 * zoom))
    });
    let mut line = PathBuilder::stroke(px(1.0));
    line.move_to(positions[0]);
    line.line_to(positions[1]);
    if let Ok(path) = line.build() {
        window.paint_path(path, rgb(crate::ui::BLUE));
    }
    for (i, p) in positions.into_iter().enumerate() {
        let bounds = Bounds::new(p - point(px(5.0), px(5.0)), size(px(10.0), px(10.0)));
        window.paint_quad(fill(
            bounds,
            rgb(if i == active {
                crate::ui::BLUE
            } else {
                0x202020
            }),
        ));
        window.paint_quad(gpui::outline(
            bounds,
            rgb(0xffffff),
            gpui::BorderStyle::Solid,
        ));
        // Start has a cross; the end remains a square, even when colors match.
        if i == 0 {
            window.paint_quad(fill(
                Bounds::new(p - point(px(3.0), px(0.5)), size(px(6.0), px(1.0))),
                rgb(0xffffff),
            ));
            window.paint_quad(fill(
                Bounds::new(p - point(px(0.5), px(3.0)), size(px(1.0), px(6.0))),
                rgb(0xffffff),
            ));
        }
    }
}

impl super::Preview {
    pub(super) fn update_gradient(
        &mut self,
        position: Point<Pixels>,
        shift: bool,
        both: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let valid = self
            .gradient_drag
            .as_ref()
            .is_some_and(|g| g.valid(self.state.read(cx)) && self.bounds.get() == Some(g.bounds));
        if !valid {
            self.gradient_drag = None;
            cx.notify();
            return;
        }
        if let Some(p) = self.pen_pointer(position, cx)
            && let Some(g) = &mut self.gradient_drag
        {
            g.update(p, shift, both);
        }
        cx.notify();
    }
    pub(super) fn gradient_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.focus.is_focused(window) {
            return false;
        }
        if event.keystroke.key == "escape" && self.gradient_drag.take().is_some() {
            cx.notify();
            return true;
        }
        let s = self.state.read(cx);
        let Some(overlay) = Overlay::current(s, s.editor.project()) else {
            return false;
        };
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform {
            return false;
        }
        let step = if modifiers.shift { 10.0 } else { 1.0 };
        let delta = match event.keystroke.key.as_str() {
            "escape" => {
                self.state.update(cx, |s, cx| {
                    s.gradient_controls = None;
                    cx.notify();
                });
                return true;
            }
            "tab" => {
                if !event.is_held && self.gradient_drag.is_none() {
                    self.gradient_point = 1 - self.gradient_point;
                }
                cx.notify();
                return true;
            }
            "left" => [-step, 0.0],
            "right" => [step, 0.0],
            "up" => [0.0, -step],
            "down" => [0.0, step],
            _ => return false,
        };
        if self.gradient_drag.is_none()
            && let Some(command) = overlay.nudge(self.gradient_point, delta, modifiers.alt, s.frame)
        {
            self.state.update(cx, |s, cx| {
                s.dispatch(&crate::editor::Action::Edit(command), window, cx)
            });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, Property, PropertyPath, TemporalHandle};
    fn scene(kind: EffectKind) -> EditorState {
        let mut s = EditorState::default();
        s.editor.execute(Command::AddRectangle).unwrap();
        s.editor
            .execute(Command::Effect {
                id: 1,
                edit: EffectEdit::Add(kind),
            })
            .unwrap();
        s.selected_layers.insert(1);
        s.gradient_controls = Some((s.editor.project().active_composition_id(), 1, 1));
        s
    }
    fn drag(s: &EditorState, index: usize) -> Gesture {
        let overlay = Overlay::current(s, s.editor.project()).unwrap();
        let p = overlay.world.point(overlay.points[index]);
        Gesture::new(
            overlay,
            index,
            p,
            Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0))),
            s,
        )
    }
    fn near(a: [f64; 2], b: [f64; 2]) {
        assert!(
            (a[0] - b[0]).abs() < 1e-8 && (a[1] - b[1]).abs() < 1e-8,
            "{a:?} != {b:?}"
        );
    }
    fn set(s: &mut EditorState, id: LayerId, p: Property, value: f64) {
        s.editor
            .execute(Command::SetValue {
                id,
                property: p,
                frame: 0,
                value,
            })
            .unwrap();
    }
    #[test]
    fn parent_rotation_mirroring_and_hit_radius_use_layer_space_without_drift() {
        let mut s = scene(EffectKind::LinearGradient);
        s.editor.execute(Command::AddNull).unwrap();
        s.editor
            .execute(Command::SetParent {
                id: 1,
                parent: Some(2),
                frame: 0,
            })
            .unwrap();
        set(&mut s, 2, Property::Rotation, 37.0);
        set(&mut s, 2, Property::ScaleX, -140.0);
        set(&mut s, 2, Property::ScaleY, 65.0);
        set(&mut s, 1, Property::Rotation, -22.0);
        s.editor.select(1);
        let overlay = Overlay::current(&s, s.editor.project()).unwrap();
        let p = overlay.world.point(overlay.points[1]);
        assert_eq!(overlay.hit([p[0] + 15.0, p[1]], 0.5, 1), Some(1));
        assert_eq!(overlay.hit([p[0] + 17.0, p[1]], 0.5, 1), None);
        let offset = [p[0] + 3.0, p[1] - 2.0];
        let mut g = Gesture::new(overlay.clone(), 1, offset, drag(&s, 1).bounds, &s);
        let d = overlay.world.vector([25.0, -14.0]);
        let end = [offset[0] + d[0], offset[1] + d[1]];
        g.update(end, false, false);
        g.update(end, false, false);
        near(
            g.points[1],
            [overlay.points[1][0] + 25.0, overlay.points[1][1] - 14.0],
        );
        assert_eq!(g.points[0], overlay.points[0]);
        assert!(g.command(&s).is_some());
    }
    #[test]
    fn shift_constrains_local_axis_and_alt_translates_both_endpoints() {
        let mut s = scene(EffectKind::RadialGradient);
        set(&mut s, 1, Property::Rotation, 45.0);
        let mut g = drag(&s, 0);
        let original = g.points;
        let d = g.overlay.world.vector([12.0, 30.0]);
        g.update([g.pointer[0] + d[0], g.pointer[1] + d[1]], true, true);
        for i in 0..2 {
            near(g.points[i], [original[i][0], original[i][1] + 30.0]);
        }
        g.update(g.pointer, false, false);
        assert!(g.command(&s).is_none());
        assert_eq!(g.points, original);
    }
    #[test]
    fn animated_drag_is_one_undo_preserves_handles_and_only_keys_changed_coordinates() {
        let mut s = scene(EffectKind::LinearGradient);
        set(&mut s, 1, Property::Rotation, 37.0);
        for parameter in [EffectParam::EndX, EffectParam::EndY] {
            s.editor
                .execute(Command::Effect {
                    id: 1,
                    edit: EffectEdit::ToggleAnimation {
                        effect: 1,
                        parameter,
                        frame: 0,
                    },
                })
                .unwrap();
            s.editor
                .execute(Command::Effect {
                    id: 1,
                    edit: EffectEdit::ToggleKey {
                        effect: 1,
                        parameter,
                        frame: 60,
                    },
                })
                .unwrap();
        }
        s.editor
            .execute(Command::SetTemporalHandle {
                id: 1,
                property: PropertyPath::Effect {
                    effect: 1,
                    parameter: EffectParam::EndX,
                },
                frame: 0,
                incoming: false,
                handle: TemporalHandle {
                    slope: 12.0,
                    influence: 0.4,
                },
            })
            .unwrap();
        s.frame = 30;
        let before = s.editor.project().clone();
        let mut g = drag(&s, 1);
        let d = g.overlay.world.vector([40.0, 0.0]);
        g.update([g.pointer[0] + d[0], g.pointer[1] + d[1]], false, false);
        s.editor.execute(g.command(&s).unwrap()).unwrap();
        let after = s.editor.project().clone();
        let e = &after.composition().layer(1).unwrap().effect_stack()[0];
        assert_eq!(
            e.parameter(EffectParam::EndX)
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![0, 30, 60]
        );
        assert_eq!(
            e.parameter(EffectParam::EndY)
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![0, 60]
        );
        assert_eq!(
            e.parameter(EffectParam::EndX).unwrap().keys()[&0],
            before.composition().layer(1).unwrap().effect_stack()[0]
                .parameter(EffectParam::EndX)
                .unwrap()
                .keys()[&0]
        );
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        s.editor.redo();
        assert_eq!(s.editor.project(), &after);
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
    }
    #[test]
    fn stale_gestures_cancel_for_document_time_tool_selection_view_and_playback_changes() {
        for mutate in [
            |s: &mut EditorState| s.frame += 1,
            |s: &mut EditorState| s.document_revision += 1,
            |s: &mut EditorState| s.preview_zoom = Some(2.0),
            |s: &mut EditorState| s.preview_pan = [20.0, 10.0],
            |s: &mut EditorState| s.viewer.rulers = !s.viewer.rulers,
            |s: &mut EditorState| s.tool = Tool::Hand,
            |s: &mut EditorState| s.playing = true,
            |s: &mut EditorState| s.gradient_controls = None,
            |s: &mut EditorState| {
                s.editor.execute(Command::AddRectangle).unwrap();
            },
            |s: &mut EditorState| {
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
            },
            |s: &mut EditorState| {
                s.editor
                    .execute(Command::Effect {
                        id: 1,
                        edit: EffectEdit::Bypass {
                            effect: 1,
                            bypassed: true,
                        },
                    })
                    .unwrap();
            },
            |s: &mut EditorState| {
                s.editor
                    .execute(Command::Effect {
                        id: 1,
                        edit: EffectEdit::Remove(1),
                    })
                    .unwrap();
            },
        ] {
            let mut s = scene(EffectKind::LinearGradient);
            let mut g = drag(&s, 1);
            g.update([g.pointer[0] + 10.0, g.pointer[1]], false, false);
            mutate(&mut s);
            assert!(!g.valid(&s));
            assert!(g.command(&s).is_none());
        }
        let mut s = scene(EffectKind::LinearGradient);
        set(&mut s, 1, Property::ScaleX, 0.0);
        assert!(Overlay::current(&s, s.editor.project()).is_none());
    }
    #[test]
    fn point_limits_noop_and_coincident_points_are_safe_and_cancel_keeps_project() {
        let s = scene(EffectKind::LinearGradient);
        let before = s.editor.project().clone();
        let mut overlay = Overlay::current(&s, &before).unwrap();
        assert!(overlay.command(overlay.points, 0).is_none());
        assert!(overlay.command([[0.0, 0.0], [32769.0, 2.0]], 0).is_none());
        assert!(overlay.command([[f64::NAN, 0.0], [1.0, 2.0]], 0).is_none());
        assert!(overlay.nudge(0, [65536.0, 0.0], true, 0).is_none());
        overlay.points[1] = overlay.points[0];
        let p = overlay.world.point(overlay.points[0]);
        assert_eq!(overlay.hit(p, 1.0, 0), Some(0));
        assert_eq!(overlay.hit(p, 1.0, 1), Some(1));
        let mut g = drag(&s, 0);
        g.update([g.pointer[0] + 40.0, g.pointer[1] + 20.0], false, true);
        drop(g);
        assert_eq!(s.editor.project(), &before);
    }
    #[test]
    fn saved_gradient_points_match_independent_pixel_formula_and_preview_output() {
        let renderer = crate::rendering::Renderer::new();
        for kind in [EffectKind::LinearGradient, EffectKind::RadialGradient] {
            let s = scene(kind);
            let overlay = Overlay::current(&s, s.editor.project()).unwrap();
            let mut e = Editor::default();
            e.replace_project(s.editor.project().clone()).unwrap();
            e.execute(overlay.command([[0.0, 0.0], [320.0, 0.0]], 0).unwrap())
                .unwrap();
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            let image = renderer.render(&saved, 0, 960).unwrap();
            assert_eq!(image, renderer.render_output(&saved, 0, 960, 540).unwrap());
            let world = saved.composition().world_transform(1, 0).unwrap();
            for local in [[80.0, 50.0], [160.0, 100.0], [240.0, 120.0]] {
                let p = world.point(local);
                let (x, y) = ((p[0] / 2.0).floor() as u32, (p[1] / 2.0).floor() as u32);
                let local = world
                    .inverse()
                    .unwrap()
                    .point([f64::from(x) * 2.0 + 1.0, f64::from(y) * 2.0 + 1.0]);
                let t = if kind == EffectKind::LinearGradient {
                    local[0] / 320.0
                } else {
                    local[0].hypot(local[1]) / 320.0
                };
                let expected = (t.clamp(0.0, 1.0) * 255.0).round() as i32;
                let pixel = image.get_pixel(x, y).0;
                assert_eq!(pixel[3], 255);
                for v in &pixel[..3] {
                    assert!(
                        (i32::from(*v) - expected).abs() <= 2,
                        "{kind:?} {pixel:?} expected {expected}"
                    );
                }
            }
        }
    }
}
