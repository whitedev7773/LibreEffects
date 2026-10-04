//! Ordered shape contents. Paints consume paths above them in their group;
//! earlier paints/groups composite in front of later paints/groups.
use super::*;

/// Animated Trim Paths controls. Offset stays unwrapped in the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrimParam {
    Start,
    End,
    Offset,
}
impl TrimParam {
    pub const ALL: [Self; 3] = [Self::Start, Self::End, Self::Offset];
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::End => "End",
            Self::Offset => "Offset",
        }
    }
    pub fn label(self) -> &'static str {
        self.name()
    }
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::Start | Self::End => (0., 100.),
            Self::Offset => (-1000000., 1000000.),
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ContentsParam {
    Width,
    Height,
    Transform(Property),
    Skew,
    SkewAxis,
    Shape(ShapeParam),
    Gradient(GradientParam),
    Trim(TrimParam),
}
impl From<ContentsParam> for String {
    fn from(p: ContentsParam) -> String {
        match p {
            ContentsParam::Width => "Width".into(),
            ContentsParam::Height => "Height".into(),
            ContentsParam::Transform(p) => format!("Transform.{p:?}"),
            ContentsParam::Skew => "Skew".into(),
            ContentsParam::SkewAxis => "SkewAxis".into(),
            ContentsParam::Shape(p) => format!("Shape.{}", String::from(p)),
            ContentsParam::Gradient(p) => format!("Gradient.{}", p.name()),
            ContentsParam::Trim(p) => format!("Trim.{}", p.name()),
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
        if s == "Skew" {
            return Ok(Self::Skew);
        }
        if s == "SkewAxis" {
            return Ok(Self::SkewAxis);
        }
        if let Some(p) = s.strip_prefix("Shape.") {
            return Ok(Self::Shape(ShapeParam::try_from(p.to_string())?));
        }
        if let Some(p) = s.strip_prefix("Gradient.") {
            return GradientParam::parse(p)
                .map(Self::Gradient)
                .ok_or_else(|| "Unknown gradient property".into());
        }
        if let Some(p) = s.strip_prefix("Trim.") {
            return TrimParam::parse(p)
                .map(Self::Trim)
                .ok_or_else(|| "Unknown Trim Paths property".into());
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
            Self::Skew => "Skew".into(),
            Self::SkewAxis => "Skew Axis".into(),
            Self::Shape(p) => p.label(),
            Self::Gradient(p) => p.label(),
            Self::Trim(p) => p.label().into(),
        }
    }
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::Width | Self::Height => (0.001, 32768.),
            Self::Skew => (-89., 89.),
            Self::SkewAxis => (-1000000., 1000000.),
            Self::Shape(p) => p.bounds(),
            Self::Gradient(p) => p.bounds(),
            Self::Trim(p) => p.bounds(),
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
    TrimPaths,
    Parametric(ShapeKind),
    Path {
        path: VectorPath,
        animation: PathAnimation,
    },
    Fill {
        even_odd: bool,
    },
    Stroke(ShapeStroke),
    GradientFill {
        even_odd: bool,
        gradient: ShapeGradient,
    },
    GradientStroke {
        style: ShapeStroke,
        gradient: ShapeGradient,
    },
}
impl ContentsKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Group(_) => "Group",
            Self::TrimPaths => "Trim Paths",
            Self::Parametric(k) => k.label(),
            Self::Path { .. } => "Path",
            Self::Fill { .. } => "Fill",
            Self::Stroke(_) => "Stroke",
            Self::GradientFill { .. } => "Gradient Fill",
            Self::GradientStroke { .. } => "Gradient Stroke",
        }
    }
    pub fn gradient(&self) -> Option<&ShapeGradient> {
        match self {
            Self::GradientFill { gradient, .. } | Self::GradientStroke { gradient, .. } => {
                Some(gradient)
            }
            _ => None,
        }
    }
    pub(crate) fn gradient_mut(&mut self) -> Option<&mut ShapeGradient> {
        match self {
            Self::GradientFill { gradient, .. } | Self::GradientStroke { gradient, .. } => {
                Some(gradient)
            }
            _ => None,
        }
    }
    pub fn stroke(&self) -> Option<&ShapeStroke> {
        match self {
            Self::Stroke(s) | Self::GradientStroke { style: s, .. } => Some(s),
            _ => None,
        }
    }
    pub fn is_paint(&self) -> bool {
        matches!(
            self,
            Self::Fill { .. }
                | Self::Stroke(_)
                | Self::GradientFill { .. }
                | Self::GradientStroke { .. }
        )
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
                .chain([(ContentsParam::Skew, 0.), (ContentsParam::SkewAxis, 0.)])
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
            Self::TrimPaths => vec![
                (ContentsParam::Trim(TrimParam::Start), 0.),
                (ContentsParam::Trim(TrimParam::End), 100.),
                (ContentsParam::Trim(TrimParam::Offset), 0.),
            ],
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
            Self::GradientFill { gradient, .. } => {
                let mut v = gradient.defaults();
                v.push((S(FillOpacity), 100.));
                v
            }
            Self::GradientStroke { style, gradient } => {
                let mut v = Self::Stroke(style.clone())
                    .defaults()
                    .into_iter()
                    .filter(|(p, _)| {
                        !matches!(
                            p,
                            ContentsParam::Shape(StrokeRed | StrokeGreen | StrokeBlue)
                        )
                    })
                    .map(|(p, t)| (p, t.value))
                    .collect::<Vec<_>>();
                v.extend(gradient.defaults());
                v
            }
        };
        values
            .drain(..)
            .map(|(p, v)| (p, AnimatedProperty::new(v)))
            .collect()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaintComposite {
    #[default]
    BelowPrevious,
    AbovePrevious,
}
impl PaintComposite {
    fn is_default(&self) -> bool {
        *self == Self::BelowPrevious
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContentsNode {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub kind: ContentsKind,
    #[serde(default, skip_serializing_if = "PaintComposite::is_default")]
    pub composite: PaintComposite,
    #[serde(default, skip_serializing_if = "PaintBlend::is_normal")]
    pub blend: PaintBlend,
    pub parameters: BTreeMap<ContentsParam, AnimatedProperty>,
}
impl ContentsNode {
    /// Display order follows the shape-group Transform controls.
    pub fn parameter_order(&self) -> Vec<ContentsParam> {
        use ContentsParam::{Skew, SkewAxis, Transform as T};
        use Property::*;
        if matches!(self.kind, ContentsKind::Group(_)) {
            [
                T(AnchorX),
                T(AnchorY),
                T(PositionX),
                T(PositionY),
                T(ScaleX),
                T(ScaleY),
                Skew,
                SkewAxis,
                T(Rotation),
                T(Opacity),
            ]
            .into_iter()
            .filter(|p| self.parameters.contains_key(p))
            .collect()
        } else if matches!(self.kind, ContentsKind::TrimPaths) {
            TrimParam::ALL
                .into_iter()
                .map(ContentsParam::Trim)
                .collect()
        } else {
            self.parameters.keys().copied().collect()
        }
    }
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
            composite: PaintComposite::default(),
            blend: PaintBlend::Normal,
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
        let rotation = Affine([c, s, -s, c, 0., 0.]);
        let scale = Affine([sx, 0., 0., sy, 0., 0.]);
        // Local scale, oriented horizontal shear, then group rotation.
        // Positive Skew with axis 0 shifts the upper edge to the right.
        let skew = self.value_at(ContentsParam::Skew, f);
        let linear = if skew == 0. {
            rotation.compose(scale)
        } else {
            let (sa, ca) = self
                .value_at(ContentsParam::SkewAxis, f)
                .to_radians()
                .sin_cos();
            let axis = Affine([ca, -sa, sa, ca, 0., 0.]);
            let unaxis = Affine([ca, sa, -sa, ca, 0., 0.]);
            let shear = Affine([1., 0., -skew.to_radians().tan(), 1., 0., 0.]);
            rotation
                .compose(axis)
                .compose(shear)
                .compose(unaxis)
                .compose(scale)
        };
        let [a, b, c, d, _, _] = linear.0;
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
        self.validate_version(duration, PROJECT_VERSION)
    }
    pub(super) fn validate_version(&self, duration: Frame, version: u32) -> Result<(), String> {
        fn walk(
            nodes: &[ContentsNode],
            depth: usize,
            ids: &mut BTreeSet<u64>,
            duration: Frame,
            version: u32,
        ) -> Result<(), String> {
            if depth > 8 {
                return Err("Contents nesting exceeds 8 groups".into());
            }
            for n in nodes {
                if matches!(n.kind, ContentsKind::TrimPaths) && version < 50 {
                    return Err("Trim Paths requires project version 50".into());
                }
                if n.blend != PaintBlend::Normal && (version < 47 || !n.kind.is_paint()) {
                    return Err("Paint blending requires a paint item and project v47".into());
                }
                if n.composite != PaintComposite::BelowPrevious
                    && (version < 46 || !n.kind.is_paint())
                {
                    return Err("Composite requires a paint item and project v46".into());
                }
                if let Some(g) = n.kind.gradient() {
                    if version < 45 || !g.valid() {
                        return Err("Invalid gradient or project version (requires v45)".into());
                    }
                }
                if n.id == 0
                    || !ids.insert(n.id)
                    || ids.len() > 256
                    || n.name.trim().is_empty()
                    || n.name.chars().count() > 128
                {
                    return Err("Invalid Contents identity, name or item count".into());
                }
                let mut expected = n.kind.defaults();
                if version < 44 {
                    expected.remove(&ContentsParam::Skew);
                    expected.remove(&ContentsParam::SkewAxis);
                }
                if n.parameters.keys().copied().collect::<Vec<_>>()
                    != expected.keys().copied().collect::<Vec<_>>()
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
                    ContentsKind::Group(v) => walk(v, depth + 1, ids, duration, version)?,
                    ContentsKind::Path { path, animation } => {
                        if !path.valid() {
                            return Err("Invalid Contents path".into());
                        }
                        animation.validate(path, duration, 43)?;
                    }
                    ContentsKind::Stroke(s) | ContentsKind::GradientStroke { style: s, .. }
                        if !s.valid() =>
                    {
                        return Err("Invalid Contents stroke".into());
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        let mut ids = BTreeSet::new();
        walk(&self.items, 0, &mut ids, duration, version)?;
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
    pub fn svg_at(&self, f: Frame) -> Result<String, ContentsRenderError> {
        self.svg_at_with_prefix(f, "contents")
    }
    /// Map a group's local geometry into layer space, including the group's own
    /// evaluated transform. Disabled ancestors exclude their whole subtree. An
    /// empty group still has a space; a missing or non-group item does not.
    pub fn group_transform(&self, item: u64, f: Frame) -> Option<Affine> {
        fn walk(nodes: &[ContentsNode], item: u64, f: Frame, world: Affine) -> Option<Affine> {
            for node in nodes.iter().filter(|node| node.enabled) {
                if let ContentsKind::Group(children) = &node.kind {
                    let world = world.compose(node.transform(f));
                    if node.id == item {
                        return Some(world);
                    }
                    if let Some(world) = walk(children, item, f, world) {
                        return Some(world);
                    }
                }
            }
            None
        }
        walk(&self.items, item, f, Affine::default())
    }
    pub fn svg_at_with_prefix(
        &self,
        f: Frame,
        prefix: &str,
    ) -> Result<String, ContentsRenderError> {
        self.svg_at_with_budget(f, prefix, &mut ContentsRenderBudget::default(), None)
    }
    /// Evaluate a layer once, sharing the frame's work/output limits and cancellation.
    /// Group recursion preserves the same layer budget; subsequent layer calls reset
    /// only per-layer work while keeping the frame totals.
    pub fn svg_at_with_budget(
        &self,
        f: Frame,
        prefix: &str,
        budget: &mut ContentsRenderBudget,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Result<String, ContentsRenderError> {
        budget.begin_layer();
        budget.check_cancel(f, cancel)?;
        let mut scope = String::new();
        let bytes = prefix.len().checked_mul(2).ok_or(ContentsRenderError {
            kind: ContentsRenderErrorKind::OutputLimit,
            frame: f,
            operator_id: None,
            source_id: None,
            message: "Contents prefix exceeds output limit",
        })?;
        budget.charge_output(bytes, f)?;
        scope.try_reserve(bytes).map_err(|_| ContentsRenderError {
            kind: ContentsRenderErrorKind::OutputLimit,
            frame: f,
            operator_id: None,
            source_id: None,
            message: "Unable to allocate Contents prefix",
        })?;
        use std::fmt::Write;
        for byte in prefix.as_bytes() {
            write!(&mut scope, "{byte:02x}").expect("writing to a String cannot fail");
        }
        Ok(render(&self.items, f, &scope, budget, cancel)?.svg)
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
/// Every source keeps its original contour identity through splitting and export.
struct EvaluatedGroup {
    svg: String,
    contours: Vec<trim_paths::RenderContour>,
}

enum PlannedPaint<'a> {
    Group(&'a ContentsNode, String),
    Paint(&'a ContentsNode, usize),
}

/// Charge before allocation or copying. In particular, repeated paint and nested
/// group copies cannot build an oversized temporary and reject it only afterward.
fn append_svg(
    out: &mut String,
    value: &str,
    frame: Frame,
    budget: &mut ContentsRenderBudget,
) -> Result<(), ContentsRenderError> {
    let size = out
        .len()
        .checked_add(value.len())
        .ok_or(ContentsRenderError {
            kind: ContentsRenderErrorKind::OutputLimit,
            frame,
            operator_id: None,
            source_id: None,
            message: "Contents SVG length overflow",
        })?;
    budget.check_output_size(size, frame)?;
    budget.charge_output(value.len(), frame)?;
    out.try_reserve(value.len())
        .map_err(|_| ContentsRenderError {
            kind: ContentsRenderErrorKind::OutputLimit,
            frame,
            operator_id: None,
            source_id: None,
            message: "Unable to allocate Contents SVG",
        })?;
    out.push_str(value);
    Ok(())
}

fn checked_paint_bytes(
    previous: usize,
    added: usize,
    frame: Frame,
    budget: &ContentsRenderBudget,
) -> Result<usize, ContentsRenderError> {
    let size = previous.checked_add(added).ok_or(ContentsRenderError {
        kind: ContentsRenderErrorKind::OutputLimit,
        frame,
        operator_id: None,
        source_id: None,
        message: "Contents staged paint length overflow",
    })?;
    budget.check_output_size(size, frame)?;
    Ok(size)
}

fn render(
    nodes: &[ContentsNode],
    f: Frame,
    scope: &str,
    budget: &mut ContentsRenderBudget,
    cancel: Option<&dyn Fn() -> bool>,
) -> Result<EvaluatedGroup, ContentsRenderError> {
    let mut contours = Vec::<trim_paths::RenderContour>::new();
    let mut planned = Vec::new();
    let mut operators = Vec::new();
    let mut staged_children_bytes = 0usize;
    // Capture paths-above memberships before any operator runs. Because source
    // records are appended in sibling order, a frozen prefix is their exact set.
    for n in nodes.iter().filter(|n| n.enabled) {
        budget.check_cancel(f, cancel)?;
        if let Some(path) = n.path_at(f) {
            contours.push(trim_paths::RenderContour::new(n.id, 0, path));
        } else if let ContentsKind::Group(children) = &n.kind {
            let child = render(children, f, scope, budget, cancel)?;
            let transform = n.transform(f);
            for contour in child.contours {
                contours.push(contour.transformed(transform, f)?);
            }
            staged_children_bytes =
                staged_children_bytes
                    .checked_add(child.svg.len())
                    .ok_or(ContentsRenderError {
                        kind: ContentsRenderErrorKind::OutputLimit,
                        frame: f,
                        operator_id: None,
                        source_id: None,
                        message: "Contents child SVG length overflow",
                    })?;
            budget.check_output_size(staged_children_bytes, f)?;
            // Opacity affects this painted group only. Its geometry still exports.
            planned.push(PlannedPaint::Group(n, child.svg));
        } else if matches!(n.kind, ContentsKind::TrimPaths) {
            operators.push((n, contours.len()));
        } else if n.kind.is_paint() {
            planned.push(PlannedPaint::Paint(n, contours.len()));
        }
    }
    for (operator, count) in operators {
        let value = |p| operator.value_at(ContentsParam::Trim(p), f);
        for contour in &mut contours[..count] {
            budget.check_cancel(f, cancel)?;
            contour.trim(
                value(TrimParam::Start),
                value(TrimParam::End),
                value(TrimParam::Offset),
                operator.id,
                f,
                budget,
                cancel,
            )?;
        }
    }
    // Back-to-front SVG order is unchanged, including isolated child paintings.
    let mut paints = std::collections::VecDeque::new();
    let mut painted_bytes = 0usize;
    for plan in planned {
        budget.check_cancel(f, cancel)?;
        let (n, count) = match plan {
            PlannedPaint::Group(n, child_svg) => {
                staged_children_bytes -= child_svg.len();
                let [a, b, c, d, x, y] = n.transform(f).0;
                let mut svg = String::new();
                append_svg(
                    &mut svg,
                    &format!(
                        "<g transform='matrix({a} {b} {c} {d} {x} {y})' opacity='{}'>",
                        n.value_at(ContentsParam::Transform(Property::Opacity), f) / 100.
                    ),
                    f,
                    budget,
                )?;
                append_svg(&mut svg, &child_svg, f, budget)?;
                append_svg(&mut svg, "</g>", f, budget)?;
                painted_bytes = checked_paint_bytes(painted_bytes, svg.len(), f, budget)?;
                checked_paint_bytes(painted_bytes, staged_children_bytes, f, budget)?;
                paints.push_front(svg);
                continue;
            }
            PlannedPaint::Paint(n, count) => (n, count),
        };
        let v = |p| n.value_at(ContentsParam::Shape(p), f);
        let color = |r, g, b| {
            format!(
                "#{:02x}{:02x}{:02x}",
                v(r).round() as u8,
                v(g).round() as u8,
                v(b).round() as u8
            )
        };
        let mut paint = String::new();
        if n.blend != PaintBlend::Normal {
            append_svg(
                &mut paint,
                &format!("<g style='mix-blend-mode:{}'>", n.blend.css()),
                f,
                budget,
            )?;
        }
        let gradient = if let Some(gradient) = n.kind.gradient() {
            // The identifier may include a caller-supplied prefix; check its size
            // before formatting either identifier or gradient markup.
            let mut id = String::new();
            append_svg(&mut id, "g", f, budget)?;
            append_svg(&mut id, scope, f, budget)?;
            append_svg(&mut id, &format!("-{}", n.id), f, budget)?;
            // Keep potentially large caller prefixes out of infallible formatting.
            // Gradient source/stop count is bounded; insert the ID with checked
            // copies into the exact markup emitted by the existing paint helper.
            let markup = gradient.svg(n, f, "");
            let (head, tail) = markup.split_once("id='").expect("gradient has an ID");
            append_svg(&mut paint, "<defs>", f, budget)?;
            append_svg(&mut paint, head, f, budget)?;
            append_svg(&mut paint, "id='", f, budget)?;
            append_svg(&mut paint, &id, f, budget)?;
            append_svg(&mut paint, tail, f, budget)?;
            append_svg(&mut paint, "</defs>", f, budget)?;
            let mut reference = String::new();
            append_svg(&mut reference, "url(#", f, budget)?;
            append_svg(&mut reference, &id, f, budget)?;
            append_svg(&mut reference, ")", f, budget)?;
            Some(reference)
        } else {
            None
        };
        append_svg(&mut paint, "<path d='", f, budget)?;
        let mut first = true;
        for contour in &contours[..count] {
            if contour.is_empty() {
                continue;
            }
            if !first {
                append_svg(&mut paint, " ", f, budget)?;
            }
            first = false;
            let data = contour.svg_data_checked(f, budget)?;
            append_svg(&mut paint, &data, f, budget).map_err(|mut error| {
                error.source_id = Some(contour.source_id());
                error
            })?;
        }
        use ShapeParam::*;
        match &n.kind {
            ContentsKind::Fill { even_odd } | ContentsKind::GradientFill { even_odd, .. } => {
                append_svg(&mut paint, "' fill='", f, budget)?;
                append_svg(
                    &mut paint,
                    &gradient.unwrap_or_else(|| color(FillRed, FillGreen, FillBlue)),
                    f,
                    budget,
                )?;
                append_svg(
                    &mut paint,
                    &format!(
                        "' fill-opacity='{}' fill-rule='{}'/>",
                        v(FillOpacity) / 100.,
                        if *even_odd { "evenodd" } else { "nonzero" }
                    ),
                    f,
                    budget,
                )?;
            }
            ContentsKind::Stroke(style) | ContentsKind::GradientStroke { style, .. } => {
                let mut style = style.clone();
                style.miter_limit = v(MiterLimit);
                style.dash_offset = v(DashOffset);
                for (i, x) in style.dashes.iter_mut().enumerate() {
                    *x = v(DashLength(i as u8));
                }
                append_svg(&mut paint, "' fill='none' stroke='", f, budget)?;
                append_svg(
                    &mut paint,
                    &gradient.unwrap_or_else(|| color(StrokeRed, StrokeGreen, StrokeBlue)),
                    f,
                    budget,
                )?;
                append_svg(
                    &mut paint,
                    &format!(
                        "' stroke-opacity='{}' stroke-width='{}' {}/>",
                        v(StrokeOpacity) / 100.,
                        v(StrokeWidth),
                        style.svg()
                    ),
                    f,
                    budget,
                )?;
            }
            _ => unreachable!("only paint nodes enter the paint plan"),
        }
        if n.blend != PaintBlend::Normal {
            append_svg(&mut paint, "</g>", f, budget)?;
        }
        painted_bytes = checked_paint_bytes(painted_bytes, paint.len(), f, budget)?;
        checked_paint_bytes(painted_bytes, staged_children_bytes, f, budget)?;
        if n.composite == PaintComposite::AbovePrevious {
            paints.push_back(paint);
        } else {
            paints.push_front(paint);
        }
    }
    let mut svg = String::new();
    let isolated = nodes
        .iter()
        .any(|n| n.enabled && n.blend != PaintBlend::Normal);
    if isolated {
        append_svg(&mut svg, "<g style='isolation:isolate'>", f, budget)?;
    }
    for paint in paints {
        append_svg(&mut svg, &paint, f, budget)?;
    }
    if isolated {
        append_svg(&mut svg, "</g>", f, budget)?;
    }
    Ok(EvaluatedGroup { svg, contours })
}

#[derive(Clone, Debug)]
pub enum ContentsEdit {
    Blend {
        item: u64,
        mode: PaintBlend,
    },
    Composite {
        item: u64,
        mode: PaintComposite,
    },
    GradientType {
        item: u64,
        radial: bool,
    },
    AddGradientStop {
        item: u64,
        opacity: bool,
        position: f64,
        frame: Frame,
    },
    RemoveGradientStop {
        item: u64,
        stop: u64,
    },
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
    /// Reorder complete immediate children without reparenting or changing payloads.
    /// A parent of zero selects the root; `order` must be a complete permutation.
    Reorder {
        parent: u64,
        order: Vec<u64>,
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
pub(super) fn migrate(project: &mut Project) {
    if project.version != 43 {
        return;
    }
    fn walk(nodes: &mut [ContentsNode]) {
        for n in nodes {
            if let ContentsKind::Group(children) = &mut n.kind {
                n.parameters
                    .insert(ContentsParam::Skew, AnimatedProperty::new(0.));
                n.parameters
                    .insert(ContentsParam::SkewAxis, AnimatedProperty::new(0.));
                walk(children);
            }
        }
    }
    let mut changed = false;
    for l in project.compositions_mut().flat_map(|c| c.layers.iter_mut()) {
        if let Content::ShapeContents(contents) = &mut l.content {
            walk(&mut contents.items);
            changed = true;
        }
    }
    if changed {
        project.version = 44;
    }
}
/// Restrict exact source-preserving no-op handling to Trim value transactions.
pub(super) fn trim_value_edits_only(command: &Command) -> bool {
    match command {
        Command::Contents {
            edit:
                ContentsEdit::Track {
                    parameter: ContentsParam::Trim(_),
                    edit: TrackEdit::Value { .. },
                    ..
                },
            ..
        }
        | Command::EditTrack {
            property:
                PropertyPath::Contents {
                    parameter: ContentsParam::Trim(_),
                    ..
                },
            edit: TrackEdit::Value { .. },
            ..
        } => true,
        Command::Batch(commands) => {
            !commands.is_empty() && commands.iter().all(trim_value_edits_only)
        }
        _ => false,
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
                let index = if matches!(kind, ContentsKind::TrimPaths) {
                    group.len()
                } else if kind.is_paint() {
                    group
                        .iter()
                        .position(|n| n.kind.is_paint())
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
            ContentsEdit::Reorder { parent, order } => {
                let children = contents.group_mut(*parent)?;
                let invalid_order = "Contents order must include each immediate child exactly once";
                if order.len() != children.len() {
                    return Err(invalid_order.into());
                }
                let positions = order
                    .iter()
                    .enumerate()
                    .map(|(index, id)| (*id, index))
                    .collect::<BTreeMap<_, _>>();
                if positions.len() != order.len()
                    || children
                        .iter()
                        .any(|node| !positions.contains_key(&node.id))
                {
                    return Err(invalid_order.into());
                }
                children.sort_by_key(|node| positions[&node.id]);
                // The editor validates the complete candidate against its existing
                // schema. A pure permutation must not require newer properties.
                return Ok(());
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
                let node = contents
                    .node_mut(*item)
                    .ok_or("Contents item no longer exists")?;
                if matches!(parameter, ContentsParam::Trim(_)) {
                    if !matches!(node.kind, ContentsKind::TrimPaths)
                        || !node.parameters.contains_key(parameter)
                    {
                        return Err("Select a Trim Paths property".into());
                    }
                    if let TrackEdit::Value { frame, value } = edit {
                        if *frame >= duration {
                            return Err("Key is outside the composition".into());
                        }
                        if !parameter.accepts(*value) {
                            return Err("Invalid animated property value".into());
                        }
                        if *value == node.value_at(*parameter, *frame) {
                            return Ok(());
                        }
                    }
                }
                let t = node
                    .parameters
                    .get_mut(parameter)
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
                match &mut n.kind {
                    ContentsKind::Fill { even_odd: value }
                    | ContentsKind::GradientFill {
                        even_odd: value, ..
                    } => *value = *even_odd,
                    _ => return Err("Select a Fill".into()),
                }
            }
            ContentsEdit::Blend { item, mode } => {
                let n = contents
                    .node_mut(*item)
                    .ok_or("Contents paint no longer exists")?;
                if !n.kind.is_paint() {
                    return Err("Select a Fill or Stroke".into());
                }
                n.blend = *mode;
            }
            ContentsEdit::Composite { item, mode } => {
                let n = contents
                    .node_mut(*item)
                    .ok_or("Contents paint no longer exists")?;
                if !n.kind.is_paint() {
                    return Err("Select a Fill or Stroke".into());
                }
                n.composite = *mode;
            }
            ContentsEdit::GradientType { item, radial } => {
                let n = contents
                    .node_mut(*item)
                    .ok_or("Contents paint no longer exists")?;
                n.kind
                    .gradient_mut()
                    .ok_or("Select a gradient paint")?
                    .radial = *radial;
            }
            ContentsEdit::AddGradientStop {
                item,
                opacity,
                position,
                frame,
            } => {
                if *frame >= duration {
                    return Err("Frame outside composition".into());
                }
                ShapeGradient::add_stop(
                    contents
                        .node_mut(*item)
                        .ok_or("Contents paint no longer exists")?,
                    *opacity,
                    *position,
                    *frame,
                )?;
            }
            ContentsEdit::RemoveGradientStop { item, stop } => {
                ShapeGradient::remove_stop(
                    contents
                        .node_mut(*item)
                        .ok_or("Contents paint no longer exists")?,
                    *stop,
                )?;
            }
            ContentsEdit::StrokeCap { item, .. }
            | ContentsEdit::StrokeJoin { item, .. }
            | ContentsEdit::AddDash(item)
            | ContentsEdit::RemoveDash(item) => {
                let n = contents
                    .node_mut(*item)
                    .ok_or("Contents stroke no longer exists")?;
                let (ContentsKind::Stroke(style) | ContentsKind::GradientStroke { style, .. }) =
                    &mut n.kind
                else {
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
