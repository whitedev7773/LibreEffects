use std::collections::BTreeSet;

use crate::{
    editor::{EditorState, Tool},
    ui,
};
use gpui::{
    Bounds, KeyDownEvent, Modifiers, PathBuilder, Pixels, Point, Window, fill, point, px, rgb, size,
};
use libre_effects_core::{
    Affine, Command, CompositionId, Content, ContentsEdit, ContentsKind, LayerId, PathMask,
    PathMaskMode, PathOrder, PathTarget, PathVertex, Project, Shape, VectorPath,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    NewShape,
    NewContents(LayerId, u64),
    Shape(LayerId),
    Contents(LayerId, u64),
    Mask(LayerId, usize),
}
/// Transient editor context; vertex selection never enters the project or history.
#[derive(Clone)]
struct Context {
    project: Project,
    revision: u64,
    frame: u32,
    selection: Option<LayerId>,
    selected_layers: BTreeSet<LayerId>,
    contents_selection: Option<(CompositionId, LayerId, u64)>,
}
impl Context {
    fn capture(s: &EditorState) -> Self {
        Self {
            project: s.editor.project().clone(),
            revision: s.document_revision,
            frame: s.frame,
            selection: s.editor.selected(),
            selected_layers: s.selected_layers.clone(),
            contents_selection: s.contents_selection,
        }
    }
    fn valid(&self, s: &EditorState) -> bool {
        s.tool == Tool::Pen
            && s.gradient_editor.is_none()
            && s.vertex_editor.is_none()
            && s.document_revision == self.revision
            && s.frame == self.frame
            && s.editor.selected() == self.selection
            && s.selected_layers == self.selected_layers
            && s.contents_selection == self.contents_selection
            && s.editor.project() == &self.project
    }
}
#[derive(Clone)]
struct Session {
    target: Target,
    path: VectorPath,
    world: Affine,
    context: Context,
}
impl Session {
    fn valid(&self, s: &EditorState) -> bool {
        self.context.valid(s)
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
                width: self.context.project.composition().width() as f64,
                height: self.context.project.composition().height() as f64,
                name: "Shape Path".into(),
            }),
            Target::NewContents(id, parent) => Some(Command::Contents {
                id,
                edit: ContentsEdit::Add {
                    parent,
                    kind: ContentsKind::Path {
                        path: self.path.clone(),
                        animation: Default::default(),
                    },
                },
            }),
            Target::Shape(id) => Some(Command::EditPath {
                id,
                target: PathTarget::Shape,
                frame: self.context.frame,
                path: self.path.clone(),
            }),
            Target::Contents(id, item) => Some(Command::EditPath {
                id,
                target: PathTarget::Contents(item),
                frame: self.context.frame,
                path: self.path.clone(),
            }),
            Target::Mask(id, index) => {
                if !self.path.closed {
                    return None;
                }
                let mut masks = self
                    .context
                    .project
                    .composition()
                    .layer(id)?
                    .path_masks()
                    .to_vec();
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
                        frame: self.context.frame,
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
    /// The evaluated pose before conversion/insertion, for no-op detection.
    original: VectorPath,
    /// The pose at pointer-down; movement is never accumulated between events.
    start: VectorPath,
    pointer: [f64; 2],
    vertices: BTreeSet<usize>,
    vertex: usize,
    part: Part,
}
impl Drag {
    fn command(&self) -> Option<Command> {
        (self.session.path != self.original)
            .then(|| self.session.command())
            .flatten()
    }
}
struct Selection {
    target: Target,
    vertices: BTreeSet<usize>,
}
/// Freeze every input to screen/composition mapping for a held pointer. This is
/// deliberately separate from selection validity: idle zoom/pan keeps the target.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct View {
    bounds: Bounds<Pixels>,
    origin: Point<Pixels>,
    zoom: f32,
    zoom_setting: Option<f32>,
    pan: [f32; 2],
    rulers: bool,
}
impl View {
    pub fn new(bounds: Bounds<Pixels>, origin: Point<Pixels>, zoom: f32, s: &EditorState) -> Self {
        Self {
            bounds,
            origin,
            zoom,
            zoom_setting: s.preview_zoom,
            pan: s.preview_pan,
            rulers: s.viewer.rulers,
        }
    }
    fn pointer(self, position: Point<Pixels>) -> Option<[f64; 2]> {
        let p = [
            f32::from(position.x - self.origin.x) as f64 / self.zoom as f64,
            f32::from(position.y - self.origin.y) as f64 / self.zoom as f64,
        ];
        (self.zoom.is_finite() && self.zoom > 0.0 && p.iter().all(|v| v.is_finite())).then_some(p)
    }
}
struct Marquee {
    session: Session,
    start: [f64; 2],
    end: [f64; 2],
    zoom: f64,
    moved: bool,
    vertices: BTreeSet<usize>,
}
impl Marquee {
    fn bounds(&self) -> [[f64; 2]; 2] {
        [
            [
                self.start[0].min(self.end[0]),
                self.start[1].min(self.end[1]),
            ],
            [
                self.start[0].max(self.end[0]),
                self.start[1].max(self.end[1]),
            ],
        ]
    }
    fn candidates(&self) -> BTreeSet<usize> {
        let mut selected = self.vertices.clone();
        if self.moved {
            let [min, max] = self.bounds();
            selected.extend(self.session.path.vertices.iter().enumerate().filter_map(
                |(index, vertex)| {
                    // Compare transformed anchor centers to the composition-space
                    // box. An inverse-mapped local AABB is wrong under skew/rotation.
                    let p = self.session.world.point(vertex.position);
                    (p[0] >= min[0] && p[0] <= max[0] && p[1] >= min[1] && p[1] <= max[1])
                        .then_some(index)
                },
            ));
        }
        selected
    }
}
#[derive(Default)]
pub(super) struct Pen {
    draft: Option<Session>,
    drag: Option<Drag>,
    marquee: Option<Marquee>,
    pointer_view: Option<View>,
    selected: Option<Selection>,
    selected_context: Option<Context>,
    held: bool,
}
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn constrain(delta: &mut [f64; 2]) {
    if delta[0].abs() > delta[1].abs() {
        delta[1] = 0.0;
    } else {
        delta[0] = 0.0;
    }
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
    if let Content::ShapeContents(contents) = l.content() {
        paths.extend(
            contents
                .editable_paths(s.frame)
                .into_iter()
                .filter_map(|(item, path, t)| {
                    let t = world.compose(t);
                    t.inverse()
                        .map(|_| (Target::Contents(l.id(), item), path, t))
                }),
        );
    }
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
impl Target {
    /// Mask indices are local Pen hit-test details. Persist only their stable ID
    /// across the modal focus change, and never infer a path from a layer row.
    fn stable(self, s: &EditorState) -> Option<(LayerId, PathTarget)> {
        match self {
            Self::Shape(layer) => Some((layer, PathTarget::Shape)),
            Self::Contents(layer, item) => Some((layer, PathTarget::Contents(item))),
            Self::Mask(layer, index) => Some((
                layer,
                PathTarget::Mask(
                    s.editor
                        .project()
                        .composition()
                        .layer(layer)?
                        .path_masks()
                        .get(index)?
                        .id,
                ),
            )),
            Self::NewShape | Self::NewContents(..) => None,
        }
    }
}
impl Pen {
    /// Capture only a committed, single selected vertex on its exact evaluated
    /// path. Call at pointer-down/key-down, before opening the modal blurs canvas.
    pub fn single_vertex_request(&self, s: &EditorState) -> Option<super::vertex_editor::Request> {
        let (layer, target, index, path, world) = self.single_vertex_candidate(s)?;
        super::vertex_editor::Request::new(s, layer, target, index, path, world).ok()
    }
    pub fn numeric_vertex_available(&self, s: &EditorState) -> bool {
        self.single_vertex_candidate(s)
            .is_some_and(|(layer, target, index, path, world)| {
                super::vertex_editor::Request::available(s, layer, target, index, &path, world)
            })
    }
    fn single_vertex_candidate(
        &self,
        s: &EditorState,
    ) -> Option<(LayerId, PathTarget, usize, VectorPath, Affine)> {
        if self.held
            || self.pointer_view.is_some()
            || self.drag.is_some()
            || self.draft.is_some()
            || self.marquee.is_some()
            || s.vertex_editor.is_some()
            || s.playing
        {
            return None;
        }
        self.selected_context.as_ref().filter(|c| c.valid(s))?;
        let selection = self
            .selected
            .as_ref()
            .filter(|selection| selection.vertices.len() == 1)?;
        let index = *selection.vertices.first()?;
        let (_, path, world) = paths(s)
            .into_iter()
            .find(|(target, _, _)| *target == selection.target)?;
        let (layer, target) = selection.target.stable(s)?;
        Some((layer, target, index, path, world))
    }
    /// Only the one-shot token supplied by ordinary modal OK/Cancel may recreate
    /// selection. Both the frozen context and re-evaluated target must still match.
    pub fn restore_vertex(
        &mut self,
        request: &super::vertex_editor::Request,
        s: &EditorState,
    ) -> bool {
        if s.vertex_editor.is_some() || !request.current(s) {
            return false;
        }
        let Some((target, _, _)) = paths(s).into_iter().find(|(target, path, world)| {
            target.stable(s) == Some((request.layer, request.target))
                && path == &request.path
                && world == &request.world
                && request.index < path.vertices.len()
        }) else {
            return false;
        };
        self.cancel();
        self.selected = Some(Selection {
            target,
            vertices: [request.index].into(),
        });
        self.selected_context = Some(Context::capture(s));
        true
    }
    /// An exact canvas Shift+V belongs to Pen even when no vertex can be edited.
    /// Consume repeats/unavailable targets so V cannot leak into the Select tool.
    pub fn numeric_vertex_key(
        &self,
        event: &KeyDownEvent,
        focused: bool,
        composing: bool,
        s: &EditorState,
    ) -> (bool, Option<super::vertex_editor::Request>) {
        let m = event.keystroke.modifiers;
        if !focused
            || composing
            || s.tool != Tool::Pen
            || s.text_session.is_some()
            || s.colors.session.is_some()
            || s.gradient_editor.is_some()
            || s.vertex_editor.is_some()
            || !m.shift
            || m.control
            || m.alt
            || m.platform
            || m.function
            || !matches!(event.keystroke.key.as_str(), "v" | "V")
        {
            return (false, None);
        }
        (
            true,
            (!event.is_held)
                .then(|| self.single_vertex_request(s))
                .flatten(),
        )
    }
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    pub fn abandon_pointer(&mut self) {
        if let Some(drag) = self.drag.take() {
            self.clear_transient_selection(&drag);
        }
        self.marquee = None;
        self.pointer_view = None;
        self.held = false;
    }
    /// A changed/missing view invalidates only an active pointer, not idle
    /// selection or a creation draft between points. During a held creation
    /// drag, conservatively discard that whole unpublished draft.
    pub fn validate_view(&mut self, view: Option<View>) -> bool {
        if self.pointer_view.is_some() && self.pointer_view != view {
            if self.held {
                self.draft = None;
            }
            self.abandon_pointer();
            return false;
        }
        view.is_some()
    }
    pub fn pointer_down(
        &mut self,
        s: &EditorState,
        position: Point<Pixels>,
        view: Option<View>,
        modifiers: Modifiers,
    ) -> Option<Command> {
        self.reset_if_stale(s);
        self.validate_view(view);
        // A second down always supersedes provisional state, even if canvas
        // bounds disappeared before the event could be mapped.
        self.abandon_pointer();
        let view = view?;
        let p = view.pointer(position)?;
        let exact_shift = modifiers.shift
            && !modifiers.alt
            && !modifiers.control
            && !modifiers.platform
            && !modifiers.function;
        let command = self.down_impl(
            s,
            p,
            view.zoom as f64,
            modifiers.alt,
            modifiers.shift,
            modifiers.control,
            exact_shift,
        );
        if self.held {
            self.pointer_view = Some(view);
        }
        command
    }
    pub fn pointer_move(
        &mut self,
        s: &EditorState,
        position: Point<Pixels>,
        view: Option<View>,
        modifiers: Modifiers,
    ) {
        self.reset_if_stale(s);
        // Validate before mapping, including fit resize and missing bounds.
        if self.validate_view(view)
            && self.pointer_view.is_some()
            && let Some(p) = view.and_then(|v| v.pointer(position))
        {
            self.moving(p, modifiers.alt, modifiers.shift);
        }
    }
    pub fn pointer_up(
        &mut self,
        s: &EditorState,
        position: Point<Pixels>,
        view: Option<View>,
        modifiers: Modifiers,
    ) -> Option<Command> {
        self.reset_if_stale(s);
        if !self.validate_view(view) || self.pointer_view.is_none() {
            return None;
        }
        let Some(p) = view.and_then(|v| v.pointer(position)) else {
            self.validate_view(None);
            return None;
        };
        self.release(s, p, modifiers.alt, modifiers.shift)
    }
    fn clear_transient_selection(&mut self, drag: &Drag) {
        if drag.start.vertices.len() != drag.original.vertices.len()
            && let Some(selection) = &mut self.selected
            && selection.target == drag.session.target
        {
            // Inserted indices address the draft, not the unchanged source path.
            // Keep the target so Delete remains consumed after cancellation.
            selection.vertices.clear();
        }
    }
    pub fn reset_if_stale(&mut self, s: &EditorState) {
        if self.selected_context.as_ref().is_some_and(|c| !c.valid(s))
            || self.draft.as_ref().is_some_and(|d| !d.valid(s))
            || self.drag.as_ref().is_some_and(|d| !d.session.valid(s))
            || self.marquee.as_ref().is_some_and(|d| !d.session.valid(s))
            || s.tool != Tool::Pen
            || s.gradient_editor.is_some()
            || s.vertex_editor.is_some()
        {
            self.cancel();
        }
    }
    pub fn pending(&self, s: &EditorState) -> Option<Command> {
        if let Some(d) = self.drag.as_ref().filter(|d| d.session.valid(s)) {
            d.command()
        } else {
            self.draft.as_ref().filter(|d| d.valid(s))?.command()
        }
    }
    fn select_vertex(&mut self, target: Target, index: usize, toggle: bool, s: &EditorState) {
        if let Some(selection) = &mut self.selected
            && selection.target == target
        {
            if toggle {
                if !selection.vertices.insert(index) {
                    selection.vertices.remove(&index);
                }
            } else if !selection.vertices.contains(&index) {
                selection.vertices = [index].into();
            }
        } else {
            self.selected = Some(Selection {
                target,
                vertices: [index].into(),
            });
        }
        self.selected_context = Some(Context::capture(s));
    }
    // Predict the committed context without touching the live document/history.
    // A rejected edit retains the current selection (and continues to consume Delete).
    fn remember_command(&mut self, command: &Command, s: &EditorState) -> bool {
        let mut next = libre_effects_core::Editor::default();
        if next.replace_project(s.editor.project().clone()).is_ok()
            && next.execute(command.clone()).is_ok()
        {
            let mut context = Context::capture(s);
            context.project = next.project().clone();
            self.selected_context = Some(context);
            true
        } else {
            false
        }
    }
    #[cfg(test)]
    fn down(
        &mut self,
        s: &EditorState,
        p: [f64; 2],
        zoom: f64,
        alt: bool,
        shift: bool,
        force_mask: bool,
    ) -> Option<Command> {
        self.down_impl(
            s,
            p,
            zoom,
            alt,
            shift,
            force_mask,
            shift && !alt && !force_mask,
        )
    }
    fn down_impl(
        &mut self,
        s: &EditorState,
        p: [f64; 2],
        zoom: f64,
        alt: bool,
        shift: bool,
        force_mask: bool,
        exact_shift: bool,
    ) -> Option<Command> {
        self.reset_if_stale(s);
        if s.tool != Tool::Pen || s.gradient_editor.is_some() || s.vertex_editor.is_some() {
            return None;
        }
        // A second pointer-down supersedes any gesture whose release was lost.
        self.abandon_pointer();
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
            context: Context::capture(s),
        };
        let existing = paths(s);
        // Handles precede curve insertion, vertices precede overlapping handles.
        for part in [Part::Vertex, Part::Incoming, Part::Outgoing] {
            for (target, path, world) in &existing {
                for (index, v) in path.vertices.iter().enumerate() {
                    let at = match part {
                        Part::Vertex => v.position,
                        Part::Incoming => add(v.position, v.incoming),
                        Part::Outgoing => add(v.position, v.outgoing),
                    };
                    if distance(world.point(at), p) <= radius {
                        self.select_vertex(
                            *target,
                            index,
                            shift && matches!(part, Part::Vertex),
                            s,
                        );
                        if shift && matches!(part, Part::Vertex) {
                            // Shift-click toggles selection only, even if the pointer moves.
                            self.held = false;
                            self.drag = None;
                            return None;
                        }
                        let mut session = make(*target, path.clone(), *world);
                        if alt && matches!(part, Part::Vertex) {
                            session.path.vertices[index].incoming = [0.0; 2];
                            session.path.vertices[index].outgoing = [0.0; 2];
                        }
                        self.drag = Some(Drag {
                            original: path.clone(),
                            start: session.path.clone(),
                            pointer: world.inverse()?.point(p),
                            vertices: self.selected.as_ref()?.vertices.clone(),
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
        if shift {
            if exact_shift
                && let Some(selection) = &self.selected
                && self.selected_context.as_ref().is_some_and(|c| c.valid(s))
                && let Some((target, path, world)) =
                    existing.iter().find(|(t, _, _)| *t == selection.target)
                && selection.vertices.iter().all(|&i| i < path.vertices.len())
            {
                self.marquee = Some(Marquee {
                    session: make(*target, path.clone(), *world),
                    start: p,
                    end: p,
                    zoom,
                    moved: false,
                    vertices: selection.vertices.clone(),
                });
                return None;
            }
            self.held = false;
            return None;
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
                    self.selected = Some(Selection {
                        target: *target,
                        vertices: [best.1 + 1].into(),
                    });
                    self.selected_context = Some(Context::capture(s));
                    self.drag = Some(Drag {
                        original: path.clone(),
                        start: session.path.clone(),
                        pointer: world.inverse()?.point(p),
                        vertices: [best.1 + 1].into(),
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
            if force_mask || !matches!(l.content(), Content::Shape(_) | Content::ShapeContents(_)) {
                if l.path_masks().len() >= 64 {
                    return None;
                }
                (
                    Target::Mask(l.id(), l.path_masks().len()),
                    comp.world_transform(l.id(), s.frame)?,
                )
            } else if let Content::ShapeContents(contents) = l.content()
                && let Some((composition, layer, item)) = s.contents_selection
                && composition == s.editor.project().active_composition_id()
                && layer == l.id()
            {
                // Only an explicitly selected Group receives new geometry. A
                // stale matching item or disabled ancestor must not fall back
                // to silently creating a separate layer. Keep the existing Pen
                // layer visibility/in-out policy; enabled Contents ancestry is
                // the additional boundary for this target.
                let node = contents.node(item)?;
                if matches!(node.kind, ContentsKind::Group(_)) {
                    (
                        Target::NewContents(l.id(), item),
                        comp.world_transform(l.id(), s.frame)?
                            .compose(contents.group_transform(item, s.frame)?),
                    )
                } else {
                    (Target::NewShape, Affine::default())
                }
            } else {
                (Target::NewShape, Affine::default())
            }
        } else {
            (Target::NewShape, Affine::default())
        };
        let local = world.inverse()?.point(p);
        self.selected = None;
        self.selected_context = None;
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
        if let Some(marquee) = &mut self.marquee {
            marquee.end = p;
            let delta = sub(p, marquee.start);
            marquee.moved |= delta[0].abs().max(delta[1].abs()) * marquee.zoom >= 4.0;
        } else if let Some(d) = &mut self.draft {
            let Some(v) = d.path.vertices.last_mut() else {
                return;
            };
            let Some(inverse) = d.world.inverse() else {
                return;
            };
            let mut delta = sub(inverse.point(p), v.position);
            if shift {
                constrain(&mut delta);
            }
            v.outgoing = delta;
            v.incoming = [-delta[0], -delta[1]];
        } else if let Some(d) = &mut self.drag {
            let Some(inverse) = d.session.world.inverse() else {
                return;
            };
            let p = inverse.point(p);
            d.session.path = d.start.clone();
            match d.part {
                Part::Vertex => {
                    let mut delta = sub(p, d.pointer);
                    if shift {
                        constrain(&mut delta);
                    }
                    for &index in &d.vertices {
                        d.session.path.vertices[index].position =
                            add(d.start.vertices[index].position, delta);
                    }
                }
                Part::Incoming | Part::Outgoing => {
                    if p == d.pointer {
                        return;
                    }
                    let v = &mut d.session.path.vertices[d.vertex];
                    let mut delta = sub(p, v.position);
                    if shift {
                        constrain(&mut delta);
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
    pub fn release(
        &mut self,
        s: &EditorState,
        p: [f64; 2],
        alt: bool,
        shift: bool,
    ) -> Option<Command> {
        self.reset_if_stale(s);
        self.moving(p, alt, shift);
        self.up(s)
    }
    pub fn up(&mut self, s: &EditorState) -> Option<Command> {
        self.reset_if_stale(s);
        self.held = false;
        self.pointer_view = None;
        if let Some(marquee) = self.marquee.take() {
            self.selected = Some(Selection {
                target: marquee.session.target,
                vertices: marquee.candidates(),
            });
            return None;
        }
        let drag = self.drag.take()?;
        let command = drag.command();
        if !command
            .as_ref()
            .is_some_and(|command| self.remember_command(command, s))
        {
            self.clear_transient_selection(&drag);
        }
        command
    }
    fn finish(&mut self) -> Option<Command> {
        let d = self.draft.as_ref()?;
        let result = d.command()?;
        self.draft = None;
        self.held = false;
        self.pointer_view = None;
        Some(result)
    }
    /// Selection-only Ctrl+A follows the app's exact Control convention. Once
    /// recognized, unavailable/repeated chords are consumed inside the canvas.
    pub fn select_all_key(
        &mut self,
        event: &KeyDownEvent,
        focused: bool,
        composing: bool,
        s: &EditorState,
    ) -> bool {
        let m = event.keystroke.modifiers;
        if !focused
            || composing
            || s.tool != Tool::Pen
            || s.text_session.is_some()
            || s.colors.session.is_some()
            || s.gradient_editor.is_some()
            || s.vertex_editor.is_some()
            || !m.control
            || m.shift
            || m.alt
            || m.platform
            || m.function
            || !matches!(event.keystroke.key.as_str(), "a" | "A")
        {
            return false;
        }
        self.reset_if_stale(s);
        if event.is_held || self.held || self.drag.is_some() || self.draft.is_some() {
            return true;
        }
        if let Some(selection) = &mut self.selected
            && self.selected_context.as_ref().is_some_and(|c| c.valid(s))
            && let Some((_, path, _)) = paths(s)
                .into_iter()
                .find(|(t, _, _)| *t == selection.target)
        {
            selection.vertices = (0..path.vertices.len()).collect();
        }
        true
    }
    /// Canvas-only shortcuts. Exact modifiers and focus keep shell shortcuts,
    /// fields and IME input out of this geometry-editing route.
    pub fn order_key(
        &mut self,
        event: &KeyDownEvent,
        focused: bool,
        composing: bool,
        s: &EditorState,
    ) -> (bool, Option<Command>) {
        let m = event.keystroke.modifiers;
        if !focused
            || composing
            || s.tool != Tool::Pen
            || s.text_session.is_some()
            || s.colors.session.is_some()
            || s.gradient_editor.is_some()
            || s.vertex_editor.is_some()
            || !m.shift
            || m.control
            || m.alt
            || m.platform
            || m.function
        {
            return (false, None);
        }
        let first = match event.keystroke.key.as_str() {
            "f" | "F" => true,
            "r" | "R" => false,
            _ => return (false, None),
        };
        // Consume repeats/unavailable edits instead of leaking Shift+R to the
        // shell's Rotation filter. Never order unpublished gesture indices.
        if event.is_held || self.held || self.drag.is_some() || self.draft.is_some() {
            return (true, None);
        }
        (true, self.reorder(first, s))
    }
    fn reorder(&mut self, first: bool, s: &EditorState) -> Option<Command> {
        let context = self.selected_context.as_ref().filter(|c| c.valid(s))?;
        let selection = self.selected.as_ref()?;
        let (_, path, _) = paths(s)
            .into_iter()
            .find(|(target, _, _)| *target == selection.target)?;
        let count = path.vertices.len();
        if selection.vertices.is_empty() || selection.vertices.iter().any(|&i| i >= count) {
            return None;
        }
        let order = if first {
            if !path.closed || selection.vertices.len() != 1 {
                return None;
            }
            PathOrder::FirstVertex(*selection.vertices.first()?)
        } else {
            PathOrder::Reverse
        };
        let (id, target) = match selection.target {
            Target::Shape(id) => (id, PathTarget::Shape),
            Target::Contents(id, item) => (id, PathTarget::Contents(item)),
            Target::Mask(id, index) => (
                id,
                PathTarget::Mask(
                    context
                        .project
                        .composition()
                        .layer(id)?
                        .path_masks()
                        .get(index)?
                        .id,
                ),
            ),
            Target::NewShape | Target::NewContents(..) => return None,
        };
        let command = Command::ReorderPath { id, target, order };
        if !self.remember_command(&command, s) {
            return None;
        }
        let selection = self.selected.as_mut()?;
        selection.vertices = selection
            .vertices
            .iter()
            .map(|&old| match order {
                PathOrder::Reverse if path.closed => (count - old) % count,
                PathOrder::Reverse => count - 1 - old,
                PathOrder::FirstVertex(index) => (old + count - index) % count,
            })
            .collect();
        Some(command)
    }
    pub fn order_help(&self, s: &EditorState) -> &'static str {
        if self.held || self.drag.is_some() || self.draft.is_some() {
            "Path order: finish or cancel the Pen gesture first"
        } else if self
            .selected
            .as_ref()
            .filter(|_| self.selected_context.as_ref().is_some_and(|c| c.valid(s)))
            .and_then(|selection| {
                paths(s)
                    .into_iter()
                    .find(|(t, _, _)| *t == selection.target)
            })
            .is_some_and(|(_, path, _)| !path.closed)
        {
            "Open path: Shift+R switches endpoints · Set First requires a closed path"
        } else {
            "Path: Shift+R reverse · Shift+F set first (one closed vertex) · outline = first"
        }
    }
    pub fn key(&mut self, key: &str, s: &EditorState) -> (bool, Option<Command>) {
        self.reset_if_stale(s);
        if self.marquee.is_some() {
            match key {
                "backspace" | "delete" => {
                    self.abandon_pointer();
                    return (true, None);
                }
                "enter" => return (true, None),
                _ => {}
            }
        }
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
                self.abandon_pointer();
                if let Some(d) = &mut self.draft {
                    d.path.vertices.pop();
                    if d.path.vertices.is_empty() {
                        self.draft = None;
                    }
                    return (true, None);
                }
                if let Some(selection) = &self.selected {
                    let target = selection.target;
                    if let Some((_, mut path, world)) =
                        paths(s).into_iter().find(|(t, _, _)| *t == target)
                    {
                        if selection
                            .vertices
                            .iter()
                            .any(|&index| index >= path.vertices.len())
                        {
                            self.selected.as_mut().unwrap().vertices.clear();
                            return (true, None);
                        }
                        let count = selection.vertices.len();
                        if count == 0
                            || path.vertices.len().saturating_sub(count)
                                < if path.closed { 3 } else { 2 }
                        {
                            return (true, None);
                        }
                        for &index in selection.vertices.iter().rev() {
                            path.vertices.remove(index);
                        }
                        let command = Session {
                            target,
                            path,
                            world,
                            context: Context::capture(s),
                        }
                        .command();
                        if let Some(command) = &command
                            && self.remember_command(command, s)
                        {
                            self.selected.as_mut().unwrap().vertices.clear();
                        }
                        return (true, command);
                    }
                    return (true, None);
                }
                (false, None)
            }
            _ => (false, None),
        }
    }
    pub fn overlay(&self, s: &EditorState) -> Vec<(VectorPath, Affine, bool, BTreeSet<usize>)> {
        if s.tool != Tool::Pen || s.gradient_editor.is_some() || s.vertex_editor.is_some() {
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
                    .as_ref()
                    .filter(|selection| selection.target == t)
                    .filter(|_| self.selected_context.as_ref().is_some_and(|c| c.valid(s)))
                    .map(|selection| selection.vertices.clone())
                    .unwrap_or_default();
                let selected = self
                    .marquee
                    .as_ref()
                    .filter(|m| m.session.target == t && m.session.valid(s))
                    .map(Marquee::candidates)
                    .unwrap_or(selected);
                (p, w, matches!(t, Target::Mask(..)), selected)
            })
            .collect()
    }
    pub fn marquee_overlay(&self, s: &EditorState) -> Option<[[f64; 2]; 2]> {
        self.marquee
            .as_ref()
            .filter(|m| m.moved && m.session.valid(s))
            .map(Marquee::bounds)
    }
}
pub(super) fn paint(
    paths: &[(VectorPath, Affine, bool, BTreeSet<usize>)],
    marquee: Option<[[f64; 2]; 2]>,
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
            // First-point identity is independent of the white selection fill.
            if index == 0 {
                let mut outline = PathBuilder::stroke(px(1.0));
                for (i, offset) in [[-5., -5.], [5., -5.], [5., 5.], [-5., 5.], [-5., -5.]]
                    .into_iter()
                    .enumerate()
                {
                    let at = p + point(px(offset[0]), px(offset[1]));
                    if i == 0 {
                        outline.move_to(at);
                    } else {
                        outline.line_to(at);
                    }
                }
                if let Ok(outline) = outline.build() {
                    window.paint_path(outline, color);
                }
            }
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
                if selected.contains(&index) {
                    rgb(0xffffff)
                } else {
                    color
                },
            ));
        }
    }
    if let Some([min, max]) = marquee {
        let mut line = PathBuilder::stroke(px(1.0));
        for (index, p) in [min, [max[0], min[1]], max, [min[0], max[1]], min]
            .into_iter()
            .enumerate()
        {
            let p = origin + point(px(p[0] as f32 * zoom), px(p[1] as f32 * zoom));
            if index == 0 {
                line.move_to(p);
            } else {
                line.line_to(p);
            }
        }
        if let Ok(line) = line.build() {
            window.paint_path(line, rgb(ui::BLUE));
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
        if let Some(c) = pen.down(s, p, 1.0, false, false, false) {
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
        pen.down(&s, [20.0, 20.0], 1.0, false, false, false);
        pen.moving([60.0, 40.0], false, false);
        s.editor.execute(pen.up(&s).unwrap()).unwrap();
        s.frame = 10;
        assert_eq!(paths(&s)[0].1.vertices[0].position, [40.0, 30.0]);
        let before = s.editor.project().clone();
        pen.down(&s, [40.0, 30.0], 1.0, false, false, false);
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
        pen.down(&s, [10.0, 10.0], 1.0, false, false, false);
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
        pen.down(&s, [400.0, 400.0], 1.0, false, false, false);
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
        pen.down(&s, [20.0, 20.0], 1.0, false, false, false);
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
        pen.down(&s, [20.0, 20.0], 1.0, true, false, false);
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
        pen.down(&s, world.point([30.0, 30.0]), 1.0, false, false, false);
        pen.moving(world.point([50.0, 50.0]), false, false);
        s.frame = 1;
        assert!(pen.up(&s).is_none());
        assert_eq!(*s.editor.project(), saved);
        s.frame = 0;
        s.editor.execute(Command::ToggleLocked(2)).unwrap();
        pen.down(&s, [0.0, 0.0], 1.0, false, false, false);
        assert!(pen.draft.is_none());
    }
}

#[cfg(test)]
#[path = "pen_tests.rs"]
mod multiselect_tests;

#[cfg(test)]
#[path = "pen_contents_tests.rs"]
mod contents_tests;

#[cfg(test)]
#[path = "pen_order_tests.rs"]
mod order_tests;

#[cfg(test)]
#[path = "pen_marquee_tests.rs"]
mod marquee_tests;

#[cfg(test)]
#[path = "pen_view_tests.rs"]
mod view_tests;

#[cfg(test)]
#[path = "pen_vertex_tests.rs"]
mod numeric_vertex_tests;
