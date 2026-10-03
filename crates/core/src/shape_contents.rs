//! Ordered shape contents. Paints consume paths above them in their group;
//! earlier paints/groups composite in front of later paints/groups.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ContentsParam {
    Width,
    Height,
    Transform(Property),
    Shape(ShapeParam),
}
impl From<ContentsParam> for String {
    fn from(p: ContentsParam) -> String {
        match p {
            ContentsParam::Width => "Width".into(),
            ContentsParam::Height => "Height".into(),
            ContentsParam::Transform(p) => format!("Transform.{p:?}"),
            ContentsParam::Shape(p) => format!("Shape.{}", String::from(p)),
        }
    }
}
impl TryFrom<String> for ContentsParam {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        if s == "Width" {
            return Ok(Self::Width);
        }
        if s == "Height" {
            return Ok(Self::Height);
        }
        if let Some(p) = s.strip_prefix("Shape.") {
            return Ok(Self::Shape(ShapeParam::try_from(p.to_string())?));
        }
        Property::ALL
            .into_iter()
            .find(|p| s == format!("Transform.{p:?}"))
            .map(Self::Transform)
            .ok_or_else(|| "Unknown Contents property".into())
    }
}
impl ContentsParam {
    pub fn label(self) -> String {
        match self {
            Self::Width => "Width".into(),
            Self::Height => "Height".into(),
            Self::Transform(p) => p.label().into(),
            Self::Shape(p) => p.label(),
        }
    }
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::Width | Self::Height => (0.001, 32768.),
            Self::Shape(p) => p.bounds(),
            Self::Transform(Property::Opacity) => (0., 100.),
            Self::Transform(Property::ScaleX | Property::ScaleY) => (-10000., 10000.),
            Self::Transform(_) => (-1000000., 1000000.),
        }
    }
    fn accepts(self, v: f64) -> bool {
        v.is_finite() && (self.bounds().0..=self.bounds().1).contains(&v)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ContentsKind {
    Group(Vec<ContentsNode>),
    Parametric(ShapeKind),
    Path {
        path: VectorPath,
        animation: PathAnimation,
    },
    Fill {
        even_odd: bool,
    },
    Stroke(ShapeStroke),
}
impl ContentsKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Group(_) => "Group",
            Self::Parametric(k) => k.label(),
            Self::Path { .. } => "Path",
            Self::Fill { .. } => "Fill",
            Self::Stroke(_) => "Stroke",
        }
    }
    fn defaults(&self) -> BTreeMap<ContentsParam, AnimatedProperty> {
        use ContentsParam::{Height, Shape as S, Transform as T, Width};
        use ShapeParam::*;
        let mut values = match self {
            Self::Group(_) => Property::ALL
                .into_iter()
                .map(|p| {
                    (
                        T(p),
                        if matches!(p, Property::ScaleX | Property::ScaleY | Property::Opacity) {
                            100.
                        } else {
                            0.
                        },
                    )
                })
                .collect::<Vec<_>>(),
            Self::Parametric(k) => {
                let mut v = vec![
                    (Width, 100.),
                    (Height, 100.),
                    (T(Property::PositionX), 0.),
                    (T(Property::PositionY), 0.),
                ];
                if *k == ShapeKind::RoundedRectangle {
                    v.push((S(Roundness), 20.));
                }
                if matches!(k, ShapeKind::Polygon | ShapeKind::Star) {
                    v.push((S(Points), 5.));
                }
                if *k == ShapeKind::Star {
                    v.push((S(InnerRadius), 50.));
                }
                v
            }
            Self::Path { .. } => vec![],
            Self::Fill { .. } => vec![
                (S(FillRed), 255.),
                (S(FillGreen), 255.),
                (S(FillBlue), 255.),
                (S(FillOpacity), 100.),
            ],
            Self::Stroke(s) => {
                let mut v = vec![
                    (S(StrokeRed), 255.),
                    (S(StrokeGreen), 255.),
                    (S(StrokeBlue), 255.),
                    (S(StrokeOpacity), 100.),
                    (S(StrokeWidth), 4.),
                    (S(MiterLimit), s.miter_limit),
                    (S(DashOffset), s.dash_offset),
                ];
                v.extend(
                    s.dashes
                        .iter()
                        .enumerate()
                        .map(|(i, v)| (S(DashLength(i as u8)), *v)),
                );
                v
            }
        };
        values
            .drain(..)
            .map(|(p, v)| (p, AnimatedProperty::new(v)))
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContentsNode {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub kind: ContentsKind,
    pub parameters: BTreeMap<ContentsParam, AnimatedProperty>,
}
impl ContentsNode {
    pub fn paint(&self) -> Option<ShapePaint> {
        match self.kind {
            ContentsKind::Fill { .. } => Some(ShapePaint::Fill),
            ContentsKind::Stroke(_) => Some(ShapePaint::Stroke),
            _ => None,
        }
    }
    pub fn paint_color_at(&self, frame: Frame) -> Option<u32> {
        Some(self.paint()?.channels().into_iter().fold(0, |rgb, p| {
            (rgb << 8) | self.value_at(ContentsParam::Shape(p), frame).round() as u32
        }))
    }
    fn new(id: u64, kind: ContentsKind) -> Self {
        Self {
            id,
            name: format!("{} {id}", kind.label()),
            enabled: true,
            parameters: kind.defaults(),
            kind,
        }
    }
    pub fn value_at(&self, p: ContentsParam, f: Frame) -> f64 {
        self.parameters
            .get(&p)
            .map_or(0., |t| t.value_at(f).clamp(p.bounds().0, p.bounds().1))
    }
    pub fn transform(&self, f: Frame) -> Affine {
        if !matches!(self.kind, ContentsKind::Group(_)) {
            return Affine::default();
        }
        let v = |p| self.value_at(ContentsParam::Transform(p), f);
        let (s, c) = v(Property::Rotation).to_radians().sin_cos();
        let (sx, sy) = (v(Property::ScaleX) / 100., v(Property::ScaleY) / 100.);
        let (a, b, c, d) = (c * sx, s * sx, -s * sy, c * sy);
        Affine([
            a,
            b,
            c,
            d,
            v(Property::PositionX) - a * v(Property::AnchorX) - c * v(Property::AnchorY),
            v(Property::PositionY) - b * v(Property::AnchorX) - d * v(Property::AnchorY),
        ])
    }
    pub fn path_at(&self, f: Frame) -> Option<VectorPath> {
        match &self.kind {
            ContentsKind::Path { path, animation } => Some(animation.at(path, f)),
            ContentsKind::Parametric(kind) => {
                let mut shape = Shape {
                    kind: *kind,
                    ..Default::default()
                };
                for (p, t) in &self.parameters {
                    if let ContentsParam::Shape(p) = p {
                        shape.parameters.insert(*p, t.clone());
                    }
                }
                let p = shape_conversion::geometry(
                    &shape,
                    self.value_at(ContentsParam::Width, f),
                    self.value_at(ContentsParam::Height, f),
                    f,
                );
                Some(transformed(
                    p,
                    Affine([
                        1.,
                        0.,
                        0.,
                        1.,
                        self.value_at(ContentsParam::Transform(Property::PositionX), f),
                        self.value_at(ContentsParam::Transform(Property::PositionY), f),
                    ]),
                ))
            }
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeContents {
    pub items: Vec<ContentsNode>,
    next_id: u64,
}
impl Default for ShapeContents {
    fn default() -> Self {
        Self {
            items: vec![],
            next_id: 1,
        }
    }
}
impl ShapeContents {
    pub fn node(&self, id: u64) -> Option<&ContentsNode> {
        find(&self.items, id)
    }
    pub(super) fn node_mut(&mut self, id: u64) -> Option<&mut ContentsNode> {
        find_mut(&mut self.items, id)
    }
    pub fn rows(&self) -> Vec<(usize, u64, &ContentsNode)> {
        fn walk<'a>(
            nodes: &'a [ContentsNode],
            depth: usize,
            parent: u64,
            out: &mut Vec<(usize, u64, &'a ContentsNode)>,
        ) {
            for n in nodes {
                out.push((depth, parent, n));
                if let ContentsKind::Group(v) = &n.kind {
                    walk(v, depth + 1, n.id, out);
                }
            }
        }
        let mut out = vec![];
        walk(&self.items, 0, 0, &mut out);
        out
    }
    fn group_mut(&mut self, id: u64) -> Result<&mut Vec<ContentsNode>, String> {
        if id == 0 {
            return Ok(&mut self.items);
        }
        match &mut self
            .node_mut(id)
            .ok_or("Contents group no longer exists")?
            .kind
        {
            ContentsKind::Group(v) => Ok(v),
            _ => Err("Choose a Contents group".into()),
        }
    }
    fn allocate(&mut self) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or("Contents ID exhausted")?;
        Ok(id)
    }
    pub fn validate(&self, duration: Frame) -> Result<(), String> {
        fn walk(
            nodes: &[ContentsNode],
            depth: usize,
            ids: &mut BTreeSet<u64>,
            duration: Frame,
        ) -> Result<(), String> {
            if depth > 8 {
                return Err("Contents nesting exceeds 8 groups".into());
            }
            for n in nodes {
                if n.id == 0
                    || !ids.insert(n.id)
                    || ids.len() > 256
                    || n.name.trim().is_empty()
                    || n.name.chars().count() > 128
                {
                    return Err("Invalid Contents identity, name or item count".into());
                }
                if n.parameters.keys().copied().collect::<Vec<_>>()
                    != n.kind.defaults().keys().copied().collect::<Vec<_>>()
                {
                    return Err("Contents properties do not match the item type".into());
                }
                for (&p, t) in &n.parameters {
                    if !p.accepts(t.value)
                        || t.keys.len() > 10000
                        || t.keys.iter().any(|(f, k)| {
                            *f >= duration
                                || !p.accepts(k.value)
                                || !k.interpolation.valid()
                                || !k.temporal.valid()
                        })
                    {
                        return Err("Invalid Contents property or keyframe".into());
                    }
                }
                match &n.kind {
                    ContentsKind::Group(v) => walk(v, depth + 1, ids, duration)?,
                    ContentsKind::Path { path, animation } => {
                        if !path.valid() {
                            return Err("Invalid Contents path".into());
                        }
                        animation.validate(path, duration, 43)?;
                    }
                    ContentsKind::Stroke(s) if !s.valid() => {
                        return Err("Invalid Contents stroke".into());
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        let mut ids = BTreeSet::new();
        walk(&self.items, 0, &mut ids, duration)?;
        if self.next_id == 0 || ids.last().is_some_and(|id| *id >= self.next_id) {
            return Err("Invalid next Contents ID".into());
        }
        Ok(())
    }
    pub(super) fn tracks_mut(&mut self) -> Vec<&mut AnimatedProperty> {
        fn walk<'a>(nodes: &'a mut [ContentsNode], out: &mut Vec<&'a mut AnimatedProperty>) {
            for n in nodes {
                out.extend(n.parameters.values_mut());
                match &mut n.kind {
                    ContentsKind::Group(v) => walk(v, out),
                    ContentsKind::Path { animation, .. } => out.push(&mut animation.timing),
                    _ => {}
                }
            }
        }
        let mut out = vec![];
        walk(&mut self.items, &mut out);
        out
    }
    pub fn editable_paths(&self, f: Frame) -> Vec<(u64, VectorPath, Affine)> {
        fn walk(
            nodes: &[ContentsNode],
            f: Frame,
            world: Affine,
            out: &mut Vec<(u64, VectorPath, Affine)>,
        ) {
            for n in nodes.iter().filter(|n| n.enabled) {
                match &n.kind {
                    ContentsKind::Group(v) => walk(v, f, world.compose(n.transform(f)), out),
                    ContentsKind::Path { .. } => out.push((n.id, n.path_at(f).unwrap(), world)),
                    _ => {}
                }
            }
        }
        let mut out = vec![];
        walk(&self.items, f, Affine::default(), &mut out);
        out
    }
    pub fn svg_at(&self, f: Frame) -> String {
        render(&self.items, f).0
    }
}
fn find(nodes: &[ContentsNode], id: u64) -> Option<&ContentsNode> {
    for n in nodes {
        if n.id == id {
            return Some(n);
        }
        if let ContentsKind::Group(v) = &n.kind {
            if let Some(n) = find(v, id) {
                return Some(n);
            }
        }
    }
    None
}
fn find_mut(nodes: &mut [ContentsNode], id: u64) -> Option<&mut ContentsNode> {
    for n in nodes {
        if n.id == id {
            return Some(n);
        }
        if let ContentsKind::Group(v) = &mut n.kind {
            if let Some(n) = find_mut(v, id) {
                return Some(n);
            }
        }
    }
    None
}
fn take(nodes: &mut Vec<ContentsNode>, id: u64) -> Option<ContentsNode> {
    if let Some(i) = nodes.iter().position(|n| n.id == id) {
        return Some(nodes.remove(i));
    }
    for n in nodes {
        if let ContentsKind::Group(v) = &mut n.kind {
            if let Some(n) = take(v, id) {
                return Some(n);
            }
        }
    }
    None
}
fn transformed(mut path: VectorPath, t: Affine) -> VectorPath {
    for v in &mut path.vertices {
        v.position = t.point(v.position);
        v.incoming = t.vector(v.incoming);
        v.outgoing = t.vector(v.outgoing);
    }
    path
}
fn render(nodes: &[ContentsNode], f: Frame) -> (String, Vec<VectorPath>) {
    let mut paths = Vec::<VectorPath>::new();
    let mut paints = vec![];
    for n in nodes.iter().filter(|n| n.enabled) {
        if let Some(p) = n.path_at(f) {
            paths.push(p);
            continue;
        }
        let v = |p| n.value_at(ContentsParam::Shape(p), f);
        let color = |r, g, b| {
            format!(
                "#{:02x}{:02x}{:02x}",
                v(r).round() as u8,
                v(g).round() as u8,
                v(b).round() as u8
            )
        };
        use ShapeParam::*;
        match &n.kind {
            ContentsKind::Group(children) => {
                let (svg, child_paths) = render(children, f);
                let t = n.transform(f);
                let [a, b, c, d, x, y] = t.0;
                paints.push(format!(
                    "<g transform='matrix({a} {b} {c} {d} {x} {y})' opacity='{}'>{svg}</g>",
                    n.value_at(ContentsParam::Transform(Property::Opacity), f) / 100.
                ));
                paths.extend(child_paths.into_iter().map(|p| transformed(p, t)));
            }
            ContentsKind::Fill { even_odd } => paints.push(format!(
                "<path d='{}' fill='{}' fill-opacity='{}' fill-rule='{}'/>",
                paths
                    .iter()
                    .map(VectorPath::svg_data)
                    .collect::<Vec<_>>()
                    .join(" "),
                color(FillRed, FillGreen, FillBlue),
                v(FillOpacity) / 100.,
                if *even_odd { "evenodd" } else { "nonzero" }
            )),
            ContentsKind::Stroke(style) => {
                let mut style = style.clone();
                style.miter_limit = v(MiterLimit);
                style.dash_offset = v(DashOffset);
                for (i, x) in style.dashes.iter_mut().enumerate() {
                    *x = v(DashLength(i as u8));
                }
                paints.push(format!("<path d='{}' fill='none' stroke='{}' stroke-opacity='{}' stroke-width='{}' {}/>",paths.iter().map(VectorPath::svg_data).collect::<Vec<_>>().join(" "),color(StrokeRed,StrokeGreen,StrokeBlue),v(StrokeOpacity)/100.,v(StrokeWidth),style.svg()));
            }
            _ => {}
        }
    }
    (paints.into_iter().rev().collect(), paths)
}

#[derive(Clone, Debug)]
pub enum ContentsEdit {
    Promote,
    Add {
        parent: u64,
        kind: ContentsKind,
    },
    Remove(u64),
    Duplicate(u64),
    Move {
        item: u64,
        parent: u64,
        index: usize,
    },
    Rename {
        item: u64,
        name: String,
    },
    Enabled {
        item: u64,
        enabled: bool,
    },
    Track {
        item: u64,
        parameter: ContentsParam,
        edit: TrackEdit,
    },
    ConvertPath {
        item: u64,
        frame: Frame,
    },
    FillRule {
        item: u64,
        even_odd: bool,
    },
    StrokeCap {
        item: u64,
        cap: StrokeCap,
    },
    StrokeJoin {
        item: u64,
        join: StrokeJoin,
    },
    AddDash(u64),
    RemoveDash(u64),
}
fn promote(shape: &Shape, width: f64, height: f64, color: u32) -> ShapeContents {
    let kind = shape
        .path
        .as_ref()
        .map_or(ContentsKind::Parametric(shape.kind), |path| {
            ContentsKind::Path {
                path: path.clone(),
                animation: shape.path_animation.clone(),
            }
        });
    let mut path = ContentsNode::new(2, kind);
    for (&p, t) in &mut path.parameters {
        t.value = match p {
            ContentsParam::Width => width,
            ContentsParam::Height => height,
            ContentsParam::Shape(p) => shape.value_at(p, 0, color),
            _ => 0.,
        };
        if let ContentsParam::Shape(p) = p {
            if let Some(old) = shape.parameters.get(&p) {
                *t = old.clone();
            }
        }
    }
    let mut fill = ContentsNode::new(4, ContentsKind::Fill { even_odd: false });
    fill.enabled = shape.fill;
    let mut stroke = ContentsNode::new(3, ContentsKind::Stroke(shape.stroke_style.clone()));
    for n in [&mut fill, &mut stroke] {
        for (&p, t) in &mut n.parameters {
            if let ContentsParam::Shape(p) = p {
                t.value = shape.value_at(p, 0, color);
                if let Some(old) = shape.parameters.get(&p) {
                    *t = old.clone();
                }
            }
        }
    }
    let group = ContentsNode::new(1, ContentsKind::Group(vec![path, stroke, fill]));
    ShapeContents {
        items: vec![group],
        next_id: 5,
    }
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::Contents { id, edit } = command else {
        return None;
    };
    Some((|| {
        let duration = state.project.composition.duration;
        let layer = editing::editable(state, *id)?;
        if matches!(edit, ContentsEdit::Promote) {
            let Content::Shape(shape) = &layer.content else {
                return Err("Select a shape to organize into Contents".into());
            };
            layer.content =
                Content::ShapeContents(promote(shape, layer.width, layer.height, layer.color));
            return Ok(());
        }
        let Content::ShapeContents(contents) = &mut layer.content else {
            return Err("Select a Contents shape layer".into());
        };
        match edit {
            ContentsEdit::Add { parent, kind } => {
                let id = contents.allocate()?;
                let n = ContentsNode::new(id, kind.clone());
                let group = contents.group_mut(*parent)?;
                let index = if matches!(kind, ContentsKind::Fill { .. } | ContentsKind::Stroke(_)) {
                    group
                        .iter()
                        .position(|n| {
                            matches!(n.kind, ContentsKind::Fill { .. } | ContentsKind::Stroke(_))
                        })
                        .unwrap_or(group.len())
                } else {
                    0
                };
                group.insert(index, n);
            }
            ContentsEdit::Remove(id) => {
                take(&mut contents.items, *id).ok_or("Contents item no longer exists")?;
            }
            ContentsEdit::Duplicate(id) => {
                let (parent, index) = contents
                    .rows()
                    .into_iter()
                    .find(|(_, _, n)| n.id == *id)
                    .map(|(_, p, _)| p)
                    .and_then(|p| {
                        contents
                            .group_mut(p)
                            .ok()
                            .map(|v| (p, v.iter().position(|n| n.id == *id).unwrap()))
                    })
                    .ok_or("Contents item no longer exists")?;
                let mut n = contents.node(*id).unwrap().clone();
                fn rekey(n: &mut ContentsNode, c: &mut ShapeContents) -> Result<(), String> {
                    n.id = c.allocate()?;
                    if let ContentsKind::Group(v) = &mut n.kind {
                        for n in v {
                            rekey(n, c)?;
                        }
                    }
                    Ok(())
                }
                rekey(&mut n, contents)?;
                n.name = format!("{} copy", n.name).chars().take(128).collect();
                contents.group_mut(parent)?.insert(index, n);
            }
            ContentsEdit::Move {
                item,
                parent,
                index,
            } => {
                let n = take(&mut contents.items, *item).ok_or("Contents item no longer exists")?;
                let dest = contents.group_mut(*parent)?;
                if *index > dest.len() {
                    return Err("Contents order is outside the group".into());
                }
                dest.insert(*index, n);
            }
            ContentsEdit::Rename { item, name } => {
                contents
                    .node_mut(*item)
                    .ok_or("Contents item no longer exists")?
                    .name = name.trim().into()
            }
            ContentsEdit::Enabled { item, enabled } => {
                contents
                    .node_mut(*item)
                    .ok_or("Contents item no longer exists")?
                    .enabled = *enabled
            }
            ContentsEdit::Track {
                item,
                parameter,
                edit,
            } => {
                let t = contents
                    .node_mut(*item)
                    .and_then(|n| n.parameters.get_mut(parameter))
                    .ok_or("Contents property no longer exists")?;
                time_remap::edit_track(t, duration, edit, |v| parameter.accepts(v))?;
                if let TrackEdit::ToggleAnimation { frame } | TrackEdit::ToggleKey { frame } = edit
                {
                    let (a, b) = parameter.bounds();
                    t.value = t.value.clamp(a, b);
                    if let Some(k) = t.keys.get_mut(frame) {
                        k.value = k.value.clamp(a, b);
                    }
                }
            }
            ContentsEdit::ConvertPath { item, frame } => {
                if *frame >= duration {
                    return Err("Frame outside composition".into());
                }
                let n = contents
                    .node_mut(*item)
                    .ok_or("Contents path no longer exists")?;
                if !matches!(n.kind, ContentsKind::Parametric(_)) {
                    return Err("Select a parametric path".into());
                }
                n.kind = ContentsKind::Path {
                    path: n.path_at(*frame).unwrap(),
                    animation: Default::default(),
                };
                n.parameters.clear();
            }
            ContentsEdit::FillRule { item, even_odd } => {
                let n = contents
                    .node_mut(*item)
                    .ok_or("Contents fill no longer exists")?;
                if !matches!(n.kind, ContentsKind::Fill { .. }) {
                    return Err("Select a Fill".into());
                }
                n.kind = ContentsKind::Fill {
                    even_odd: *even_odd,
                };
            }
            ContentsEdit::StrokeCap { item, .. }
            | ContentsEdit::StrokeJoin { item, .. }
            | ContentsEdit::AddDash(item)
            | ContentsEdit::RemoveDash(item) => {
                let n = contents
                    .node_mut(*item)
                    .ok_or("Contents stroke no longer exists")?;
                let ContentsKind::Stroke(style) = &mut n.kind else {
                    return Err("Select a Stroke".into());
                };
                match edit {
                    ContentsEdit::StrokeCap { cap, .. } => style.cap = *cap,
                    ContentsEdit::StrokeJoin { join, .. } => style.join = *join,
                    ContentsEdit::AddDash(_) if style.dashes.len() < ShapeStroke::MAX_DASHES => {
                        let p =
                            ContentsParam::Shape(ShapeParam::DashLength(style.dashes.len() as u8));
                        style.dashes.push(10.);
                        n.parameters.insert(p, AnimatedProperty::new(10.));
                    }
                    ContentsEdit::RemoveDash(_) if !style.dashes.is_empty() => {
                        style.dashes.pop();
                        n.parameters
                            .remove(&ContentsParam::Shape(ShapeParam::DashLength(
                                style.dashes.len() as u8,
                            )));
                    }
                    _ => return Err("Stroke supports up to 16 dash/gap lengths".into()),
                }
            }
            ContentsEdit::Promote => unreachable!(),
        }
        contents.validate(duration)
    })())
}
