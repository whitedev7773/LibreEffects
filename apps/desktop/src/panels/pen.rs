use crate::{
    editor::{EditorState, Tool},
    ui,
};
use gpui::{Bounds, PathBuilder, Pixels, Point, Window, fill, point, px, rgb, size};
use libre_effects_core::{
    Affine, Command, Content, LayerId, PathMask, PathMaskMode, PathTarget, PathVertex, Project,
    Shape, VectorPath,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    NewShape,
    Shape(LayerId),
    Mask(LayerId, usize),
}
#[derive(Clone)]
struct Session {
    target: Target,
    path: VectorPath,
    world: Affine,
    project: Project,
    revision: u64,
    frame: u32,
    selection: Option<LayerId>,
}
impl Session {
    fn valid(&self, s: &EditorState) -> bool {
        s.tool == Tool::Pen
            && s.document_revision == self.revision
            && s.frame == self.frame
            && s.editor.selected() == self.selection
            && s.editor.project() == &self.project
    }
    fn command(&self) -> Option<Command> {
        if !self.path.valid() {
            return None;
        }
        match self.target {
            Target::NewShape => Some(Command::AddContent {
                content: Content::Shape(Shape {
                    path: Some(self.path.clone()),
                    fill: self.path.closed,
                    stroke_width: if self.path.closed { 0.0 } else { 3.0 },
                    ..Default::default()
                }),
                width: self.project.composition().width() as f64,
                height: self.project.composition().height() as f64,
                name: "Shape Path".into(),
            }),
            Target::Shape(id) => Some(Command::EditPath {
                id,
                target: PathTarget::Shape,
                frame: self.frame,
                path: self.path.clone(),
            }),
            Target::Mask(id, index) => {
                if !self.path.closed {
                    return None;
                }
                let mut masks = self.project.composition().layer(id)?.path_masks().to_vec();
                if index == masks.len() {
                    masks.push(PathMask {
                        path: self.path.clone(),
                        mode: PathMaskMode::Add,
                        inverted: false,
                        ..Default::default()
                    });
                } else {
                    return Some(Command::EditPath {
                        id,
                        target: PathTarget::Mask(masks.get(index)?.id),
                        frame: self.frame,
                        path: self.path.clone(),
                    });
                }
                Some(Command::SetPathMasks { id, masks })
            }
        }
    }
}
#[derive(Clone, Copy)]
enum Part {
    Vertex,
    Incoming,
    Outgoing,
}
struct Drag {
    session: Session,
    vertex: usize,
    part: Part,
}
#[derive(Default)]
pub(super) struct Pen {
    draft: Option<Session>,
    drag: Option<Drag>,
    selected: Option<(Target, usize)>,
    selected_context: Option<(Project, u32, Option<LayerId>)>,
    held: bool,
}
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn curve(a: &PathVertex, b: &PathVertex, t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let c1 = add(a.position, a.outgoing);
    let c2 = add(b.position, b.incoming);
    [0, 1].map(|i| {
        u * u * u * a.position[i]
            + 3.0 * u * u * t * c1[i]
            + 3.0 * u * t * t * c2[i]
            + t * t * t * b.position[i]
    })
}
fn paths(s: &EditorState) -> Vec<(Target, VectorPath, Affine)> {
    let Some(l) = s.editor.selected_layer().filter(|l| !l.locked()) else {
        return Vec::new();
    };
    let Some(world) = s
        .editor
        .project()
        .composition()
        .world_transform(l.id(), s.frame)
        .filter(|a| a.inverse().is_some())
    else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    if let Content::Shape(shape) = l.content()
        && let Some(path) = shape.path_at(s.frame)
    {
        paths.push((Target::Shape(l.id()), path.clone(), world));
    }
    paths.extend(
        l.path_masks()
            .iter()
            .enumerate()
            .map(|(i, m)| (Target::Mask(l.id(), i), m.path_at(s.frame), world)),
    );
    paths
}
impl Pen {
    pub fn reset_if_stale(&mut self, s: &EditorState) {
        if self
            .selected_context
            .as_ref()
            .is_some_and(|(p, f, selection)| {
                p != s.editor.project() || *f != s.frame || *selection != s.editor.selected()
            })
        {
            self.selected = None;
            self.selected_context = None;
        }
        if self.draft.as_ref().is_some_and(|d| !d.valid(s))
            || self.drag.as_ref().is_some_and(|d| !d.session.valid(s))
            || s.tool != Tool::Pen
        {
            *self = Self::default();
        }
    }
    pub fn pending(&self, s: &EditorState) -> Option<Command> {
        self.drag
            .as_ref()
            .map(|d| &d.session)
            .or(self.draft.as_ref())
            .filter(|d| d.valid(s))?
            .command()
    }
    pub fn down(
        &mut self,
        s: &EditorState,
        p: [f64; 2],
        zoom: f64,
        alt: bool,
        force_mask: bool,
    ) -> Option<Command> {
        self.reset_if_stale(s);
        self.held = true;
        let radius = 7.0 / zoom;
        if let Some(d) = &mut self.draft {
            if d.path.vertices.len() >= 3
                && distance(d.world.point(d.path.vertices[0].position), p) <= radius
            {
                d.path.closed = true;
                return self.finish();
            }
            if d.path.vertices.len() < 1024 {
                d.path
                    .vertices
                    .push(PathVertex::corner(d.world.inverse()?.point(p)));
            }
            return None;
        }
        let make = |target, path, world| Session {
            target,
            path,
            world,
            project: s.editor.project().clone(),
            revision: s.document_revision,
            frame: s.frame,
            selection: s.editor.selected(),
        };
        let existing = paths(s);
        // Handles precede curve insertion, vertices precede overlapping handles.
        for (target, path, world) in &existing {
            for part in [Part::Vertex, Part::Incoming, Part::Outgoing] {
                for (index, v) in path.vertices.iter().enumerate() {
                    let at = match part {
                        Part::Vertex => v.position,
                        Part::Incoming => add(v.position, v.incoming),
                        Part::Outgoing => add(v.position, v.outgoing),
                    };
                    if distance(world.point(at), p) <= radius {
                        let mut session = make(*target, path.clone(), *world);
                        if alt && matches!(part, Part::Vertex) {
                            session.path.vertices[index].incoming = [0.0; 2];
                            session.path.vertices[index].outgoing = [0.0; 2];
                        }
                        self.selected = Some((*target, index));
                        self.drag = Some(Drag {
                            session,
                            vertex: index,
                            part: if alt && matches!(part, Part::Vertex) {
                                Part::Outgoing
                            } else {
                                part
                            },
                        });
                        return None;
                    }
                }
            }
        }
        for (target, path, world) in &existing {
            let mut best = (radius, 0, 0.5);
            for segment in 0..path.vertices.len() - usize::from(!path.closed) {
                for step in 1..40 {
                    let t = step as f64 / 40.0;
                    let d = distance(
                        world.point(curve(
                            &path.vertices[segment],
                            &path.vertices[(segment + 1) % path.vertices.len()],
                            t,
                        )),
                        p,
                    );
                    if d < best.0 {
                        best = (d, segment, t);
                    }
                }
            }
            if best.0 < radius {
                let mut session = make(*target, path.clone(), *world);
                if session.path.insert(best.1, best.2) {
                    self.selected = Some((*target, best.1 + 1));
                    self.drag = Some(Drag {
                        session,
                        vertex: best.1 + 1,
                        part: Part::Vertex,
                    });
                }
                return None;
            }
        }
        let comp = s.editor.project().composition();
        let (target, world) = if let Some(l) = s.editor.selected_layer() {
            if l.locked() || matches!(l.content(), Content::Audio { .. } | Content::Null) {
                return None;
            }
            if force_mask || !matches!(l.content(), Content::Shape(_)) {
                if l.path_masks().len() >= 64 {
                    return None;
                }
                (
                    Target::Mask(l.id(), l.path_masks().len()),
                    comp.world_transform(l.id(), s.frame)?,
                )
            } else {
                (Target::NewShape, Affine::default())
            }
        } else {
            (Target::NewShape, Affine::default())
        };
        let local = world.inverse()?.point(p);
        self.selected = None;
        self.draft = Some(make(
            target,
            VectorPath {
                vertices: vec![PathVertex::corner(local)],
                closed: false,
            },
            world,
        ));
        None
    }
    pub fn moving(&mut self, p: [f64; 2], alt: bool, shift: bool) {
        if !self.held {
            return;
        }
        if let Some(d) = &mut self.draft {
            let Some(v) = d.path.vertices.last_mut() else {
                return;
            };
            let Some(inverse) = d.world.inverse() else {
                return;
            };
            let mut delta = sub(inverse.point(p), v.position);
            if shift {
                if delta[0].abs() > delta[1].abs() {
                    delta[1] = 0.0;
                } else {
                    delta[0] = 0.0;
                }
            }
            v.outgoing = delta;
            v.incoming = [-delta[0], -delta[1]];
        } else if let Some(d) = &mut self.drag {
            let Some(inverse) = d.session.world.inverse() else {
                return;
            };
            let v = &mut d.session.path.vertices[d.vertex];
            let p = inverse.point(p);
            match d.part {
                Part::Vertex => v.position = p,
                Part::Incoming | Part::Outgoing => {
                    let mut delta = sub(p, v.position);
                    if shift {
                        if delta[0].abs() > delta[1].abs() {
                            delta[1] = 0.0;
                        } else {
                            delta[0] = 0.0;
                        }
                    }
                    let opposite = [-delta[0], -delta[1]];
                    match d.part {
                        Part::Incoming => {
                            v.incoming = delta;
                            if !alt {
                                v.outgoing = opposite;
                            }
                        }
                        _ => {
                            v.outgoing = delta;
                            if !alt {
                                v.incoming = opposite;
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn up(&mut self, s: &EditorState) -> Option<Command> {
        self.held = false;
        let command = self
            .drag
            .take()
            .filter(|d| d.session.valid(s))?
            .session
            .command()?;
        let mut next = libre_effects_core::Editor::default();
        if next.replace_project(s.editor.project().clone()).is_ok()
            && next.execute(command.clone()).is_ok()
        {
            self.selected_context = Some((next.project().clone(), s.frame, s.editor.selected()));
        }
        Some(command)
    }
    fn finish(&mut self) -> Option<Command> {
        let d = self.draft.as_ref()?;
        let result = d.command()?;
        self.draft = None;
        self.held = false;
        Some(result)
    }
    pub fn key(&mut self, key: &str, s: &EditorState) -> (bool, Option<Command>) {
        self.reset_if_stale(s);
        match key {
            "escape" if self.draft.is_some() || self.drag.is_some() || self.selected.is_some() => {
                *self = Self::default();
                (true, None)
            }
            "enter" if self.draft.is_some() => {
                if let Some(d) = &mut self.draft
                    && matches!(d.target, Target::Mask(..))
                {
                    d.path.closed = true;
                }
                (true, self.finish())
            }
            "backspace" | "delete" => {
                if let Some(d) = &mut self.draft {
                    d.path.vertices.pop();
                    if d.path.vertices.is_empty() {
                        self.draft = None;
                    }
                    return (true, None);
                }
                if let Some((target, index)) = self.selected {
                    if let Some((_, mut path, world)) =
                        paths(s).into_iter().find(|(t, _, _)| *t == target)
                        && index < path.vertices.len()
                        && path.vertices.len() > if path.closed { 3 } else { 2 }
                    {
                        path.vertices.remove(index);
                        self.selected = None;
                        return (
                            true,
                            Session {
                                target,
                                path,
                                world,
                                project: s.editor.project().clone(),
                                revision: s.document_revision,
                                frame: s.frame,
                                selection: s.editor.selected(),
                            }
                            .command(),
                        );
                    }
                    return (true, None);
                }
                (false, None)
            }
            _ => (false, None),
        }
    }
    pub fn overlay(&self, s: &EditorState) -> Vec<(VectorPath, Affine, bool, Option<usize>)> {
        if s.tool != Tool::Pen {
            return Vec::new();
        }
        let mut all = paths(s);
        if let Some(d) = self
            .drag
            .as_ref()
            .map(|d| &d.session)
            .or(self.draft.as_ref())
            .filter(|d| d.valid(s))
        {
            all.retain(|(t, _, _)| *t != d.target);
            all.push((d.target, d.path.clone(), d.world));
        }
        all.into_iter()
            .map(|(t, p, w)| {
                let selected = self
                    .selected
                    .filter(|(target, _)| *target == t)
                    .map(|(_, i)| i);
                (p, w, matches!(t, Target::Mask(..)), selected)
            })
            .collect()
    }
}
pub(super) fn paint(
    paths: &[(VectorPath, Affine, bool, Option<usize>)],
    origin: Point<Pixels>,
    zoom: f32,
    window: &mut Window,
) {
    for (path, world, mask, selected) in paths {
        let screen = |p| {
            let p = world.point(p);
            origin + point(px(p[0] as f32 * zoom), px(p[1] as f32 * zoom))
        };
        let color = rgb(if *mask { 0xffcb66 } else { ui::BLUE });
        let mut line = PathBuilder::stroke(px(1.25));
        if let Some(first) = path.vertices.first() {
            line.move_to(screen(first.position));
            for segment in 0..path
                .vertices
                .len()
                .saturating_sub(usize::from(!path.closed))
            {
                for step in 1..=40 {
                    line.line_to(screen(curve(
                        &path.vertices[segment],
                        &path.vertices[(segment + 1) % path.vertices.len()],
                        step as f64 / 40.0,
                    )));
                }
            }
            if let Ok(line) = line.build() {
                window.paint_path(line, color);
            }
        }
        for (index, vertex) in path.vertices.iter().enumerate() {
            let p = screen(vertex.position);
            for tangent in [vertex.incoming, vertex.outgoing] {
                if tangent == [0.0; 2] {
                    continue;
                }
                let end = screen(add(vertex.position, tangent));
                let mut line = PathBuilder::stroke(px(1.0));
                line.move_to(p);
                line.line_to(end);
                if let Ok(line) = line.build() {
                    window.paint_path(line, color);
                }
                window.paint_quad(fill(
                    Bounds::new(end - point(px(2.0), px(2.0)), size(px(4.0), px(4.0))),
                    color,
                ));
            }
            window.paint_quad(fill(
                Bounds::new(p - point(px(3.0), px(3.0)), size(px(6.0), px(6.0))),
                if *selected == Some(index) {
                    rgb(0xffffff)
                } else {
                    color
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::Property;
    fn state() -> EditorState {
        let mut state = EditorState::default();
        state.tool = Tool::Pen;
        state.composition_started = true;
        state
    }
    fn click(pen: &mut Pen, s: &mut EditorState, p: [f64; 2]) {
        if let Some(c) = pen.down(s, p, 1.0, false, false) {
            s.editor.execute(c).unwrap();
        }
        if let Some(c) = pen.up(s) {
            s.editor.execute(c).unwrap();
        }
    }
    fn closed(pen: &mut Pen, s: &mut EditorState) {
        for p in [
            [20.0, 20.0],
            [200.0, 20.0],
            [200.0, 200.0],
            [20.0, 200.0],
            [20.0, 20.0],
        ] {
            click(pen, s, p);
        }
    }
    #[test]
    fn pen_uses_interpolated_geometry_and_commits_one_undoable_path_key() {
        use libre_effects_core::{PropertyPath, TrackEdit};
        let mut s = state();
        let mut pen = Pen::default();
        closed(&mut pen, &mut s);
        let target = PathTarget::Shape;
        s.editor
            .execute(Command::AnimatePath {
                id: 1,
                target,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
        s.frame = 20;
        pen.down(&s, [20.0, 20.0], 1.0, false, false);
        pen.moving([60.0, 40.0], false, false);
        s.editor.execute(pen.up(&s).unwrap()).unwrap();
        s.frame = 10;
        assert_eq!(paths(&s)[0].1.vertices[0].position, [40.0, 30.0]);
        let before = s.editor.project().clone();
        pen.down(&s, [40.0, 30.0], 1.0, false, false);
        pen.moving([45.0, 55.0], false, false);
        s.editor.execute(pen.up(&s).unwrap()).unwrap();
        assert_eq!(paths(&s)[0].1.vertices[0].position, [45.0, 55.0]);
        assert_eq!(
            s.editor
                .selected_layer()
                .unwrap()
                .track(PropertyPath::Path(target))
                .unwrap()
                .keys()
                .len(),
            3
        );
        s.editor.undo();
        assert_eq!(*s.editor.project(), before);
        assert_eq!(paths(&s)[0].1.vertices[0].position, [40.0, 30.0]);
    }
    #[test]
    fn draft_close_cancel_open_and_curve_creation_are_atomic() {
        let mut s = state();
        let mut pen = Pen::default();
        let initial = s.editor.project().clone();
        closed(&mut pen, &mut s);
        assert_eq!(s.editor.project().composition().layers().len(), 1);
        let Content::Shape(shape) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        assert!(shape.path.as_ref().unwrap().closed);
        s.editor.undo();
        assert_eq!(*s.editor.project(), initial);
        pen.down(&s, [10.0, 10.0], 1.0, false, false);
        pen.moving([50.0, 10.0], false, false);
        pen.up(&s);
        click(&mut pen, &mut s, [150.0, 100.0]);
        let command = pen.key("enter", &s).1.unwrap();
        s.editor.execute(command).unwrap();
        let Content::Shape(shape) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        let path = shape.path.as_ref().unwrap();
        assert!(!path.closed);
        assert!(!shape.fill);
        assert_eq!(shape.stroke_width, 3.0);
        assert_eq!(path.vertices[0].outgoing, [40.0, 0.0]);
        assert_eq!(path.vertices[0].incoming, [-40.0, 0.0]);
        let saved = s.editor.project().clone();
        pen.down(&s, [400.0, 400.0], 1.0, false, false);
        pen.key("escape", &s);
        assert!(pen.pending(&s).is_none());
        assert_eq!(*s.editor.project(), saved);
    }
    #[test]
    fn vertex_handle_insertion_deletion_and_undo_preserve_other_vertices() {
        let mut s = state();
        let mut pen = Pen::default();
        closed(&mut pen, &mut s);
        let before = s.editor.project().clone();
        pen.down(&s, [20.0, 20.0], 1.0, false, false);
        pen.moving([35.0, 45.0], false, false);
        s.editor.execute(pen.up(&s).unwrap()).unwrap();
        let Content::Shape(shape) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        assert_eq!(
            shape.path.as_ref().unwrap().vertices[0].position,
            [35.0, 45.0]
        );
        s.editor.undo();
        assert_eq!(*s.editor.project(), before);
        pen.reset_if_stale(&s);
        assert!(pen.selected.is_none());
        click(&mut pen, &mut s, [110.0, 20.0]);
        let Content::Shape(shape) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        assert_eq!(shape.path.as_ref().unwrap().vertices.len(), 5);
        assert!(
            distance(
                shape.path.as_ref().unwrap().vertices[1].position,
                [110.0, 20.0]
            ) < 0.01
        );
        s.editor.execute(pen.key("delete", &s).1.unwrap()).unwrap();
        let Content::Shape(shape) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        assert_eq!(shape.path.as_ref().unwrap().vertices.len(), 4);
        pen.down(&s, [20.0, 20.0], 1.0, true, false);
        pen.moving([40.0, 20.0], false, false);
        s.editor.execute(pen.up(&s).unwrap()).unwrap();
        let Content::Shape(shape) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        assert_eq!(
            shape.path.as_ref().unwrap().vertices[0].outgoing,
            [20.0, 0.0]
        );
        assert_eq!(
            shape.path.as_ref().unwrap().vertices[0].incoming,
            [-20.0, 0.0]
        );
    }
    #[test]
    fn masks_use_parented_layer_coordinates_and_locked_or_stale_gestures_do_not_commit() {
        let mut s = state();
        s.editor
            .execute(Command::AddContent {
                content: Content::Null,
                width: 100.0,
                height: 100.0,
                name: "Parent".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::AddContent {
                content: Content::Solid,
                width: 300.0,
                height: 300.0,
                name: "Source".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::SetParent {
                id: 2,
                parent: Some(1),
                frame: 0,
            })
            .unwrap();
        s.editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::Rotation,
                frame: 0,
                value: 35.0,
            })
            .unwrap();
        let world = s
            .editor
            .project()
            .composition()
            .world_transform(2, 0)
            .unwrap();
        let mut pen = Pen::default();
        for p in [[30.0, 30.0], [200.0, 30.0], [100.0, 240.0], [30.0, 30.0]] {
            click(&mut pen, &mut s, world.point(p));
        }
        let layer = s.editor.selected_layer().unwrap();
        assert_eq!(layer.path_masks().len(), 1);
        assert!(
            distance(
                layer.path_masks()[0].path.vertices[0].position,
                [30.0, 30.0]
            ) < 1e-8
        );
        let saved = s.editor.project().clone();
        pen.down(&s, world.point([30.0, 30.0]), 1.0, false, false);
        pen.moving(world.point([50.0, 50.0]), false, false);
        s.frame = 1;
        assert!(pen.up(&s).is_none());
        assert_eq!(*s.editor.project(), saved);
        s.frame = 0;
        s.editor.execute(Command::ToggleLocked(2)).unwrap();
        pen.down(&s, [0.0, 0.0], 1.0, false, false);
        assert!(pen.draft.is_none());
    }
}
