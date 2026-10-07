//! Frozen current-frame Contents geometry. No live project is changed by a drag.
use super::*;
use libre_effects_core::{PathTransformSpec, transform_path_in_world};

#[derive(Clone)]
pub(super) struct Binding {
    context: Context,
    transport: u64,
    action: u64,
    selection: Selection,
}
impl Binding {
    pub fn new(s: &EditorState, selection: &Selection) -> Self {
        Self {
            context: Context::capture(s),
            transport: s.transport_generation(),
            action: s.input_context_generation(),
            selection: selection.clone(),
        }
    }
    pub fn valid(&self, s: &EditorState, selection: Option<&Selection>) -> bool {
        ready(s)
            && self.context.valid(s)
            && self.transport == s.transport_generation()
            && self.action == s.input_context_generation()
            && selection == Some(&self.selection)
    }
}

pub(super) fn ready(s: &EditorState) -> bool {
    s.tool == Tool::Pen
        && !s.playing
        && !s.preview_caching
        && !s.new_composition_requested
        && !s.media_open
        && !s.fonts_open
        && !s.queue_open
        && !s.exporting
        && !s.close_after_save
        && s.recovery.is_none()
        && s.colors.session.is_none()
        && s.text_session.is_none()
        && s.gradient_editor.is_none()
        && s.vertex_editor.is_none()
        && s.expression_editor.is_none()
        && s.gradient_preview.is_none()
}

pub(super) fn modifiers_allowed(m: Modifiers) -> bool {
    !m.alt && !m.control && !m.platform && !m.function
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Handle {
    Move,
    Scale(usize),
    Rotate,
}

#[derive(Clone, Copy)]
pub(crate) struct BoxOverlay {
    pub corners: [[f64; 2]; 4],
    pub pivot: [f64; 2],
    pub rotate: [f64; 2],
}
impl BoxOverlay {
    pub fn new(points: impl Iterator<Item = [f64; 2]>, zoom: f64) -> Option<Self> {
        if !zoom.is_finite() || zoom <= 0. {
            return None;
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for point in points {
            for axis in 0..2 {
                if !point[axis].is_finite() {
                    return None;
                }
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        if min.iter().chain(max.iter()).any(|v| !v.is_finite()) {
            return None;
        }
        let pivot = [0, 1].map(|axis| min[axis] + (max[axis] - min[axis]) / 2.);
        // A one-point/flat selection still has usable handles. This is visual
        // padding only; geometry and the actual center pivot remain unchanged.
        for axis in 0..2 {
            let extent = ((max[axis] - min[axis]) / 2.).max(12. / zoom);
            min[axis] = pivot[axis] - extent;
            max[axis] = pivot[axis] + extent;
        }
        Some(Self {
            corners: [min, [max[0], min[1]], max, [min[0], max[1]]],
            pivot,
            rotate: [pivot[0], min[1] - 24. / zoom],
        })
    }
    pub(super) fn hit(&self, p: [f64; 2], zoom: f64) -> Option<Handle> {
        if distance(self.rotate, p) * zoom <= 8. {
            return Some(Handle::Rotate);
        }
        self.corners
            .iter()
            .position(|corner| distance(*corner, p) * zoom <= 8.)
            .map(Handle::Scale)
    }
    fn transformed(self, spec: &PathTransformSpec) -> Self {
        let (sin, cos) = spec.rotation_degrees.to_radians().sin_cos();
        let map = |point: [f64; 2]| {
            let delta = [0, 1]
                .map(|axis| (point[axis] - spec.pivot[axis]) * spec.scale_percent[axis] / 100.);
            [
                spec.pivot[0] + cos * delta[0] - sin * delta[1] + spec.translation[0],
                spec.pivot[1] + sin * delta[0] + cos * delta[1] + spec.translation[1],
            ]
        };
        Self {
            corners: self.corners.map(map),
            pivot: map(self.pivot),
            rotate: map(self.rotate),
        }
    }
}

pub(super) struct Gesture {
    pub id: u64,
    pub binding: Binding,
    layer: LayerId,
    frame: u32,
    selections: BTreeMap<u64, BTreeSet<usize>>,
    source: BTreeMap<u64, (VectorPath, Affine)>,
    start: [f64; 2],
    handle: Handle,
    bounds: BoxOverlay,
    transform: PathTransformSpec,
    /// Invalid/out-of-range final input must not commit an earlier valid draft.
    preview: Option<BTreeMap<u64, VectorPath>>,
}
impl Gesture {
    pub fn new(
        s: &EditorState,
        selection: &Selection,
        start: [f64; 2],
        handle: Handle,
        zoom: f64,
    ) -> Option<Self> {
        if !ready(s) {
            return None;
        }
        let (layer, selections) = selection.contents()?;
        let source: BTreeMap<_, _> = paths(s)
            .into_iter()
            .filter_map(|(target, path, world)| match target {
                Target::Contents(id, item) if id == layer && selections.contains_key(&item) => {
                    Some((item, (path, world)))
                }
                _ => None,
            })
            .collect();
        if source.len() != selections.len() || selections.is_empty() {
            return None;
        }
        for (&item, indices) in &selections {
            let (path, world) = source.get(&item)?;
            transform_path_in_world(path, indices, &PathTransformSpec::default(), *world).ok()?;
        }
        let bounds = BoxOverlay::new(
            selections.iter().flat_map(|(item, indices)| {
                let (path, world) = &source[item];
                indices
                    .iter()
                    .map(move |&index| world.point(path.vertices[index].position))
            }),
            zoom,
        )?;
        let preview = Some(
            source
                .iter()
                .map(|(&item, (path, _))| (item, path.clone()))
                .collect(),
        );
        Some(Self {
            id: crate::color_edit::next_gradient_gesture(),
            binding: Binding::new(s, selection),
            layer,
            frame: s.frame,
            selections,
            source,
            start,
            handle,
            bounds,
            transform: PathTransformSpec {
                pivot: bounds.pivot,
                ..Default::default()
            },
            preview,
        })
    }
    pub fn update(&mut self, p: [f64; 2], shift: bool) {
        let mut spec = PathTransformSpec {
            pivot: self.bounds.pivot,
            ..Default::default()
        };
        if p.iter().any(|value| !value.is_finite()) {
            self.preview = None;
            return;
        }
        // Exact return-to-start skips all inverse/trig arithmetic and source
        // rewrites, including under reflected/skewed parents.
        if p != self.start {
            match self.handle {
                Handle::Move => {
                    spec.translation = sub(p, self.start);
                    if shift {
                        constrain(&mut spec.translation);
                    }
                }
                Handle::Scale(_) => {
                    let before = sub(self.start, self.bounds.pivot);
                    let after = sub(p, self.bounds.pivot);
                    spec.scale_percent = [0, 1].map(|axis| 100. * after[axis] / before[axis]);
                    if shift {
                        let axis = usize::from(
                            (spec.scale_percent[1] - 100.).abs()
                                > (spec.scale_percent[0] - 100.).abs(),
                        );
                        spec.scale_percent = [spec.scale_percent[axis]; 2];
                    }
                }
                Handle::Rotate => {
                    let before = sub(self.start, self.bounds.pivot);
                    let after = sub(p, self.bounds.pivot);
                    if after == [0.; 2] {
                        self.preview = None;
                        return;
                    }
                    let angle =
                        (after[1].atan2(after[0]) - before[1].atan2(before[0])).to_degrees();
                    spec.rotation_degrees = (angle + 180.).rem_euclid(360.) - 180.;
                    if shift {
                        spec.rotation_degrees = (spec.rotation_degrees / 15.).round() * 15.;
                    }
                }
            }
        }
        self.transform = spec;
        self.preview = self
            .source
            .iter()
            .map(|(&item, (path, world))| {
                transform_path_in_world(path, &self.selections[&item], &spec, *world)
                    .map(|path| (item, path))
            })
            .collect::<Result<_, _>>()
            .ok();
    }
    pub fn command(&self) -> Option<Command> {
        let preview = self.preview.as_ref()?;
        if preview
            .iter()
            .all(|(item, path)| *path == self.source[item].0)
        {
            return None;
        }
        Some(Command::TransformContentsPoints {
            id: self.layer,
            frame: self.frame,
            selections: self.selections.clone(),
            transform: self.transform,
        })
    }
    pub fn path(&self, item: u64) -> Option<&VectorPath> {
        self.preview.as_ref()?.get(&item)
    }
    pub fn overlay(&self) -> Option<BoxOverlay> {
        self.preview.as_ref()?;
        Some(self.bounds.transformed(&self.transform))
    }
}

pub(crate) fn paint(overlay: BoxOverlay, origin: Point<Pixels>, zoom: f32, window: &mut Window) {
    let screen = |p: [f64; 2]| origin + point(px(p[0] as f32 * zoom), px(p[1] as f32 * zoom));
    let mut line = PathBuilder::stroke(px(1.));
    line.move_to(screen(overlay.corners[0]));
    for corner in overlay.corners[1..]
        .iter()
        .chain(std::iter::once(&overlay.corners[0]))
    {
        line.line_to(screen(*corner));
    }
    line.move_to(screen([
        (overlay.corners[0][0] + overlay.corners[1][0]) / 2.,
        (overlay.corners[0][1] + overlay.corners[1][1]) / 2.,
    ]));
    line.line_to(screen(overlay.rotate));
    if let Ok(line) = line.build() {
        window.paint_path(line, rgb(ui::BLUE));
    }
    for p in overlay
        .corners
        .into_iter()
        .chain(std::iter::once(overlay.rotate))
    {
        window.paint_quad(fill(
            Bounds::new(screen(p) - point(px(4.), px(4.)), size(px(8.), px(8.))),
            rgb(ui::BLUE),
        ));
        window.paint_quad(fill(
            Bounds::new(screen(p) - point(px(2.), px(2.)), size(px(4.), px(4.))),
            rgb(0xffffff),
        ));
    }
    let center = screen(overlay.pivot);
    let mut pivot = PathBuilder::stroke(px(1.));
    pivot.move_to(center - point(px(4.), px(0.)));
    pivot.line_to(center + point(px(4.), px(0.)));
    pivot.move_to(center - point(px(0.), px(4.)));
    pivot.line_to(center + point(px(0.), px(4.)));
    if let Ok(pivot) = pivot.build() {
        window.paint_path(pivot, rgb(0xffffff));
    }
}
