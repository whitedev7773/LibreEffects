//! Closed local linear and radial paint servers. Definitions never reach the SVG normalizer.
use super::*;
use libre_effects_core::{GradientParam, ShapeGradient};
use std::collections::BTreeMap;
#[path = "svg_import_radial.rs"]
mod radial;
use radial::{RadialCircle, validate_radial_circle, validate_radial_precision};

const MAX_GRADIENTS: usize = 64;
const MAX_STOPS: usize = 256;
const MAX_REFERENCE_DEPTH: usize = 16;
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const STOP_ATTRS: &[&str] = &["stop-color", "stop-opacity"];

#[derive(Clone)]
pub(super) enum Paint {
    None,
    Solid(svgtypes::Color),
    Gradient(String),
}
#[derive(Clone)]
struct Stop {
    offset: f64,
    color: svgtypes::Color,
    opacity: f64,
}
struct Gradient {
    object_bbox: bool,
    geometry: Geometry,
    transform: Affine,
    stops: Vec<Stop>,
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Kind {
    #[default]
    Linear,
    Radial,
}
#[derive(Clone, Copy)]
enum Geometry {
    Linear {
        start: [f64; 2],
        end: [f64; 2],
    },
    Radial {
        center: [f64; 2],
        radius: f64,
        focus: [f64; 2],
        shader_center: [f32; 2],
        shader_radius: f32,
        shader_focus: [f32; 2],
    },
}
// Keep coordinates uncomputed: an inheriting gradient may override units, and
// absent defaults must be applied only after the complete reference chain.
#[derive(Clone, Default)]
struct Template {
    kind: Kind,
    object_bbox: Option<bool>,
    coordinates: [Option<svgtypes::Length>; 6],
    transform: Option<Affine>,
    stops: Vec<Stop>,
    href: Option<String>,
    depth: usize,
}
pub(super) struct Gradients(BTreeMap<String, Gradient>);

fn identifier(id: &str) -> bool {
    let mut chars = id.bytes();
    id.len() <= 128
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}
pub(super) fn reference(source: &str) -> Result<&str, String> {
    source
        .trim()
        .strip_prefix("url(#")
        .and_then(|s| s.strip_suffix(')'))
        .filter(|id| identifier(id))
        .ok_or_else(|| "SVG paint references require literal url(#ID) with a local ASCII ID".into())
}
pub(super) fn validate_element(n: Node<'_, '_>, budget: &mut Budget) -> Result<(), String> {
    let tag = n.tag_name().name();
    let parent = n.parent_element().map(|n| n.tag_name().name());
    let allowed: &[&str] = match tag {
        "defs" if parent == Some("svg") => &["id"],
        "linearGradient" if parent == Some("defs") => &[
            "id",
            "href",
            "x1",
            "y1",
            "x2",
            "y2",
            "gradientUnits",
            "gradientTransform",
            "spreadMethod",
            "color-interpolation",
        ],
        "radialGradient" if parent == Some("defs") => &[
            "id",
            "href",
            "cx",
            "cy",
            "r",
            "fx",
            "fy",
            "fr",
            "gradientUnits",
            "gradientTransform",
            "spreadMethod",
            "color-interpolation",
        ],
        "stop" if matches!(parent, Some("linearGradient" | "radialGradient")) => {
            &["id", "offset", "stop-color", "stop-opacity", "style"]
        }
        _ => {
            return Err(format!(
                "<{tag}> requires root-level defs / gradient / stop nesting"
            ));
        }
    };
    for child in n.children().filter(Node::is_element) {
        let child = child.tag_name().name();
        if !match tag {
            "defs" => matches!(
                child,
                "linearGradient" | "radialGradient" | "title" | "desc"
            ),
            "linearGradient" | "radialGradient" => matches!(child, "stop" | "title" | "desc"),
            _ => false,
        } {
            return Err(format!("Unsupported <{child}> inside <{tag}>"));
        }
    }
    for attr in n.attributes() {
        let local_attribute = attr.namespace().is_none() && allowed.contains(&attr.name());
        let xlink_href = matches!(tag, "linearGradient" | "radialGradient")
            && attr.name() == "href"
            && attr.namespace() == Some(XLINK_NS);
        if !local_attribute && !xlink_href {
            return Err(format!(
                "Unsupported SVG attribute '{}' on <{tag}>",
                attr.name()
            ));
        }
        if attr.name() == "id"
            && (attr.value().trim().is_empty()
                || attr.value().chars().count() > 128
                || !budget.ids.insert(attr.value().to_owned()))
        {
            return Err("SVG IDs must be nonempty, unique and at most 128 characters".into());
        }
        if attr.name() == "style" {
            budget.style_declarations += declarations(attr.value(), STOP_ATTRS)?.len();
            if budget.style_declarations > MAX_DOCUMENT_DECLARATIONS {
                return Err("SVG exceeds the 2048 inline-declaration document limit".into());
            }
        }
    }
    Ok(())
}
impl Gradients {
    pub(super) fn parse(root: Node<'_, '_>, viewport: [f64; 2]) -> Result<Self, String> {
        let mut templates = BTreeMap::new();
        let mut count = 0;
        for n in root.descendants().filter(|n| {
            n.is_element() && matches!(n.tag_name().name(), "linearGradient" | "radialGradient")
        }) {
            if templates.len() == MAX_GRADIENTS {
                return Err("SVG exceeds 64 gradients".into());
            }
            let id = n.attribute("id").filter(|id| identifier(id)).ok_or(
                "SVG gradient needs an ASCII ID (letter/underscore, then letters/digits/_.-)",
            )?;
            let kind = if n.tag_name().name() == "radialGradient" {
                Kind::Radial
            } else {
                Kind::Linear
            };
            let object_bbox = match n.attribute("gradientUnits") {
                Some("objectBoundingBox") => Some(true),
                Some("userSpaceOnUse") => Some(false),
                None => None,
                _ => return Err("Unsupported SVG gradientUnits".into()),
            };
            let local_reference = |value: &str| -> Result<String, String> {
                value
                    .strip_prefix('#')
                    .filter(|id| identifier(id))
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        "SVG gradient href requires literal #ID with a local ASCII ID".into()
                    })
            };
            // Validate both supplied attributes, even when SVG2 would shadow xlink.
            // Our closed subset deliberately rejects conflicting destinations.
            let href = n.attribute("href").map(local_reference).transpose()?;
            let xlink = n
                .attribute((XLINK_NS, "href"))
                .map(local_reference)
                .transpose()?;
            if href
                .as_ref()
                .zip(xlink.as_ref())
                .is_some_and(|(a, b)| a != b)
            {
                return Err("Conflicting SVG href and xlink:href are unsupported".into());
            }
            let href = href.or(xlink);
            if n.attribute("spreadMethod").is_some_and(|s| s != "pad") {
                return Err("SVG gradients support only pad spreadMethod".into());
            }
            if n.attribute("color-interpolation")
                .is_some_and(|s| s != "sRGB")
            {
                return Err("SVG gradients support only sRGB color interpolation".into());
            }
            let mut coordinates = [None; 6];
            let names: &[&str] = match kind {
                Kind::Linear => &["x1", "y1", "x2", "y2"],
                Kind::Radial => &["cx", "cy", "r", "fx", "fy", "fr"],
            };
            for (coordinate, &name) in coordinates.iter_mut().zip(names) {
                if let Some(value) = n.attribute(name) {
                    let value = svgtypes::Length::from_str(value)
                        .map_err(|_| "Invalid SVG gradient coordinate")?;
                    finite(value.number)?;
                    if name == "r" && value.number <= 0. {
                        return Err("SVG radial gradient radius must be positive".into());
                    }
                    if name == "fr" && value.number != 0. {
                        return Err(
                            "SVG radial gradients support only zero focal radius (fr)".into()
                        );
                    }
                    if !matches!(
                        value.unit,
                        svgtypes::LengthUnit::None
                            | svgtypes::LengthUnit::Percent
                            | svgtypes::LengthUnit::Px
                    ) {
                        return Err("SVG gradient coordinates support numbers/percent; px only in userSpaceOnUse".into());
                    }
                    *coordinate = Some(value);
                }
            }
            let transform = n
                .attribute("gradientTransform")
                .map(|value| parse_transform(Some(value)))
                .transpose()?;
            let mut stops = Vec::new();
            for stop in n
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "stop")
            {
                count += 1;
                if count > MAX_STOPS {
                    return Err("SVG exceeds the 256 gradient-stop document limit".into());
                }
                if stops.len() == ShapeGradient::MAX_STOPS {
                    return Err("SVG gradients support 2–32 stops".into());
                }
                let raw = stop
                    .attribute("offset")
                    .ok_or("SVG gradient stops require offset")?;
                let offset = if let Some(s) = raw.trim().strip_suffix('%') {
                    number(s)? / 100.
                } else {
                    number(raw)?
                };
                if !(0. ..=1.).contains(&offset)
                    || stops.last().is_some_and(|s: &Stop| offset < s.offset)
                {
                    return Err(
                        "SVG stop offsets must be ordered in the 0..1 / 0..100% range".into(),
                    );
                }
                let mut value = Stop {
                    offset: if offset == 0. { 0. } else { offset },
                    color: svgtypes::Color::new_rgb(0, 0, 0),
                    opacity: 1.,
                };
                let mut apply = |name: &str, source: &str| -> Result<(), String> {
                    match name {
                        "stop-color" => {
                            value.color = color(source)?.ok_or("SVG stop-color cannot be none")?
                        }
                        "stop-opacity" => value.opacity = opacity(source)?,
                        _ => return Err("Unsupported SVG stop property".into()),
                    }
                    Ok(())
                };
                for attr in stop.attributes().filter(|a| STOP_ATTRS.contains(&a.name())) {
                    apply(attr.name(), attr.value())?;
                }
                if let Some(style) = stop.attribute("style") {
                    for (name, source) in declarations(style, STOP_ATTRS)? {
                        apply(&name, &source)?;
                    }
                }
                stops.push(value);
            }
            templates.insert(
                id.to_owned(),
                Template {
                    kind,
                    object_bbox,
                    coordinates,
                    transform,
                    stops,
                    href,
                    depth: 0,
                },
            );
        }
        let mut resolved = BTreeMap::new();
        let mut active = BTreeSet::new();
        let mut resolved_stops = 0;
        // Resolve the entire definition table, not merely visible paints. Each
        // definition is memoized once; budgets count both source and reused stops.
        for id in templates.keys() {
            resolve(
                id,
                &templates,
                &mut resolved,
                &mut active,
                &mut resolved_stops,
            )?;
        }
        let mut result = BTreeMap::new();
        for (id, template) in resolved {
            result.insert(id, template.gradient(viewport)?);
        }
        Ok(Self(result))
    }
    pub(super) fn paint(&self, source: &str) -> Result<Paint, String> {
        if source.trim().starts_with("url(") {
            let id = reference(source)?;
            if !self.0.contains_key(id) {
                return Err(format!(
                    "SVG paint #{id} does not reference a local gradient"
                ));
            }
            Ok(Paint::Gradient(id.to_owned()))
        } else {
            Ok(color(source)?.map(Paint::Solid).unwrap_or(Paint::None))
        }
    }
    pub(super) fn node(
        &self,
        paint: &Paint,
        stroke: bool,
        style: &Style,
        bounds: Option<tiny_skia::Rect>,
    ) -> Result<Option<ContentsNode>, String> {
        let kind = match paint {
            Paint::None => return Ok(None),
            Paint::Solid(_) if stroke => ContentsKind::Stroke(style.stroke_style.clone()),
            Paint::Solid(_) => ContentsKind::Fill {
                even_odd: style.even_odd,
            },
            Paint::Gradient(id) => {
                let mut gradient = ShapeGradient::with_paired_stops(self.0[id].stops.len())?;
                gradient.radial = matches!(self.0[id].geometry, Geometry::Radial { .. });
                if stroke {
                    ContentsKind::GradientStroke {
                        style: style.stroke_style.clone(),
                        gradient,
                    }
                } else {
                    ContentsKind::GradientFill {
                        even_odd: style.even_odd,
                        gradient,
                    }
                }
            }
        };
        let mut node = ContentsNode::with_defaults(kind);
        node.name = match paint {
            Paint::Gradient(_) if stroke => "Gradient Stroke",
            Paint::Gradient(_) => "Gradient Fill",
            _ if stroke => "Stroke",
            _ => "Fill",
        }
        .into();
        node.set_static_value(
            ContentsParam::Shape(if stroke {
                ShapeParam::StrokeOpacity
            } else {
                ShapeParam::FillOpacity
            }),
            if stroke {
                style.stroke_opacity
            } else {
                style.fill_opacity
            } * 100.,
        )?;
        if stroke {
            node.set_static_value(
                ContentsParam::Shape(ShapeParam::StrokeWidth),
                style.stroke_width,
            )?;
        }
        match paint {
            Paint::Solid(color) => {
                let parameters = if stroke {
                    [
                        ShapeParam::StrokeRed,
                        ShapeParam::StrokeGreen,
                        ShapeParam::StrokeBlue,
                    ]
                } else {
                    [
                        ShapeParam::FillRed,
                        ShapeParam::FillGreen,
                        ShapeParam::FillBlue,
                    ]
                };
                for (p, v) in parameters
                    .into_iter()
                    .zip([color.red, color.green, color.blue])
                {
                    node.set_static_value(ContentsParam::Shape(p), v as f64)?;
                }
            }
            Paint::Gradient(id) => {
                let source = &self.0[id];
                let (start, end, highlight_length, highlight_angle) = match source.geometry {
                    Geometry::Linear { .. } => {
                        let (start, end) = source.endpoints(bounds)?;
                        (start, end, 0., 0.)
                    }
                    Geometry::Radial { .. } => source.radial_parameters(bounds)?,
                };
                use GradientParam::*;
                for (p, v) in [
                    (StartX, start[0]),
                    (StartY, start[1]),
                    (EndX, end[0]),
                    (EndY, end[1]),
                    (HighlightLength, highlight_length),
                    (HighlightAngle, highlight_angle),
                ] {
                    node.set_static_value(ContentsParam::Gradient(p), v)?;
                }
                let ids = node.kind.gradient().unwrap().clone();
                for ((&color, &alpha), stop) in
                    ids.colors.iter().zip(&ids.opacities).zip(&source.stops)
                {
                    for (p, v) in [
                        (ColorPosition(color), stop.offset * 100.),
                        (Red(color), stop.color.red as f64),
                        (Green(color), stop.color.green as f64),
                        (Blue(color), stop.color.blue as f64),
                        (OpacityPosition(alpha), stop.offset * 100.),
                        (Opacity(alpha), stop.opacity * 100.),
                    ] {
                        node.set_static_value(ContentsParam::Gradient(p), v)?;
                    }
                }
            }
            Paint::None => unreachable!(),
        }
        Ok(Some(node))
    }
}
fn resolve(
    id: &str,
    templates: &BTreeMap<String, Template>,
    resolved: &mut BTreeMap<String, Template>,
    active: &mut BTreeSet<String>,
    resolved_stops: &mut usize,
) -> Result<Template, String> {
    if let Some(template) = resolved.get(id) {
        return Ok(template.clone());
    }
    if active.len() > MAX_REFERENCE_DEPTH {
        return Err("SVG gradient inheritance exceeds the 16-reference depth limit".into());
    }
    if !active.insert(id.to_owned()) {
        return Err("Cyclic SVG gradient href inheritance is unsupported".into());
    }
    let mut template = templates
        .get(id)
        .cloned()
        .ok_or_else(|| format!("SVG gradient href #{id} does not reference a local gradient"))?;
    if let Some(href) = template.href.take() {
        if templates
            .get(&href)
            .is_some_and(|parent| parent.kind != template.kind)
        {
            return Err("SVG gradient href must reference the same gradient kind".into());
        }
        let inherited = resolve(&href, templates, resolved, active, resolved_stops)?;
        template.depth = inherited.depth + 1;
        if template.depth > MAX_REFERENCE_DEPTH {
            return Err("SVG gradient inheritance exceeds the 16-reference depth limit".into());
        }
        template.object_bbox = template.object_bbox.or(inherited.object_bbox);
        for (coordinate, parent) in template.coordinates.iter_mut().zip(inherited.coordinates) {
            *coordinate = coordinate.or(parent);
        }
        template.transform = template.transform.or(inherited.transform);
        if template.stops.is_empty() {
            template.stops = inherited.stops;
        }
    }
    if !(2..=ShapeGradient::MAX_STOPS).contains(&template.stops.len()) {
        return Err("SVG gradients support 2–32 effective stops".into());
    }
    *resolved_stops += template.stops.len();
    if *resolved_stops > MAX_STOPS {
        return Err("SVG exceeds the 256 resolved gradient-stop document limit".into());
    }
    active.remove(id);
    resolved.insert(id.to_owned(), template.clone());
    Ok(template)
}
impl Template {
    fn gradient(self, viewport: [f64; 2]) -> Result<Gradient, String> {
        let object_bbox = self.object_bbox.unwrap_or(true);
        let coordinate = |index: usize, fallback: usize| -> Result<f64, String> {
            let value = self.coordinates[index]
                .or(self.coordinates[fallback])
                .unwrap_or(svgtypes::Length {
                    number: match self.kind {
                        Kind::Linear => {
                            if index == 2 {
                                100.
                            } else {
                                0.
                            }
                        }
                        Kind::Radial => {
                            if index == 5 {
                                0.
                            } else {
                                50.
                            }
                        }
                    },
                    unit: svgtypes::LengthUnit::Percent,
                });
            let base = if object_bbox {
                1.
            } else if self.kind == Kind::Radial {
                match index {
                    0 | 3 => viewport[0],
                    1 | 4 => viewport[1],
                    _ => viewport[0].hypot(viewport[1]) / 2f64.sqrt(),
                }
            } else {
                viewport[index % 2]
            };
            let value = match value.unit {
                svgtypes::LengthUnit::None => value.number,
                svgtypes::LengthUnit::Px if !object_bbox => value.number,
                svgtypes::LengthUnit::Percent => value.number / 100. * base,
                _ => return Err(
                    "SVG gradient coordinates support numbers/percent; px only in userSpaceOnUse"
                        .into(),
                ),
            };
            finite(value)?;
            Ok(value)
        };
        // Reproduce the original parser's f32 length conversion independently
        // from the mathematical coordinates, before flattening the paint field.
        let shader_coordinate = |index: usize, fallback: f32| -> f32 {
            let Some(value) = self.coordinates[index] else {
                return fallback;
            };
            let number = value.number as f32;
            if value.unit != svgtypes::LengthUnit::Percent {
                return number;
            }
            if object_bbox {
                return number / 100.;
            }
            let [width, height] = viewport.map(|v| v as f32);
            let base = match index {
                0 | 3 => width,
                1 | 4 => height,
                _ => ((width * width + height * height) / 2.).sqrt(),
            };
            base * number / 100.
        };
        let geometry = match self.kind {
            Kind::Linear => {
                let start = [coordinate(0, 0)?, coordinate(1, 1)?];
                let end = [coordinate(2, 2)?, coordinate(3, 3)?];
                representable_length(start, end)?;
                Geometry::Linear { start, end }
            }
            Kind::Radial => {
                let center = [coordinate(0, 0)?, coordinate(1, 1)?];
                let radius = coordinate(2, 2)?;
                // Preserve explicit inherited focus; otherwise default only now,
                // using this gradient's final center, not the parent's default.
                let focus = [coordinate(3, 0)?, coordinate(4, 1)?];
                if coordinate(5, 5)? != 0. {
                    return Err("SVG radial gradients support only zero focal radius (fr)".into());
                }
                validate_radial_circle(center, radius, focus)?;
                let [width, height] = viewport.map(|v| v as f32);
                let default_x = if object_bbox { 0.5 } else { width * 50. / 100. };
                let default_y = if object_bbox {
                    0.5
                } else {
                    height * 50. / 100.
                };
                let default_radius = if object_bbox {
                    0.5
                } else {
                    ((width * width + height * height) / 2.).sqrt() * 50. / 100.
                };
                let shader_center = [
                    shader_coordinate(0, default_x),
                    shader_coordinate(1, default_y),
                ];
                let shader_radius = shader_coordinate(2, default_radius);
                let shader_focus = [
                    shader_coordinate(3, shader_center[0]),
                    shader_coordinate(4, shader_center[1]),
                ];
                Geometry::Radial {
                    center,
                    radius,
                    focus,
                    shader_center,
                    shader_radius,
                    shader_focus,
                }
            }
        };
        let gradient = Gradient {
            object_bbox,
            geometry,
            transform: self.transform.unwrap_or_default(),
            stops: self.stops,
        };
        if !object_bbox {
            match gradient.geometry {
                Geometry::Linear { .. } => {
                    gradient.endpoints(None)?;
                }
                Geometry::Radial { .. } => {
                    gradient.radial_parameters(None)?;
                }
            }
        }
        Ok(gradient)
    }
}
impl Gradient {
    fn transform(&self, bounds: Option<tiny_skia::Rect>) -> Result<Affine, String> {
        Ok(if self.object_bbox {
            let b = bounds
                .filter(|b| b.width() > 0. && b.height() > 0.)
                .ok_or("SVG objectBoundingBox gradient needs nonzero geometry width and height")?;
            Affine([
                b.width() as f64,
                0.,
                0.,
                b.height() as f64,
                b.left() as f64,
                b.top() as f64,
            ])
            .compose(self.transform)
        } else {
            self.transform
        })
    }
    fn radial_parameters(
        &self,
        bounds: Option<tiny_skia::Rect>,
    ) -> Result<([f64; 2], [f64; 2], f64, f64), String> {
        let Geometry::Radial {
            center,
            radius,
            focus,
            shader_center,
            shader_radius,
            shader_focus,
        } = self.geometry
        else {
            unreachable!()
        };
        let transform = self.transform(bounds)?;
        let [a, b, c, d, _, _] = transform.0;
        let sx = a.hypot(b);
        let sy = c.hypot(d);
        // A radial paint is a circle in the native paint coordinate system.
        // Outer geometry/group transforms stay separate and may be affine.
        if !sx.is_finite()
            || !sy.is_finite()
            || sx <= 0.
            || sy <= 0.
            || (sx - sy).abs() / sx.max(sy) > 1e-10
            || (a * c + b * d).abs() / (sx * sy) > 1e-10
        {
            return Err(
                "SVG radial gradient transform produces an unsupported ellipse or shear".into(),
            );
        }
        let center = transform.point(center);
        let focus = transform.point(focus);
        let radius = radius * sx;
        validate_radial_circle(center, radius, focus)?;
        let end = [center[0] + radius, center[1]];
        for value in center.into_iter().chain(end).chain(focus) {
            finite(value)?;
        }
        let delta = [focus[0] - center[0], focus[1] - center[1]];
        let length = delta[0].hypot(delta[1]) * 100. / radius;
        let angle = if length == 0. {
            0.
        } else {
            delta[1].atan2(delta[0]).to_degrees()
        };
        // Match ShapeGradient::svg, including f64 endpoint cancellation and
        // highlight reconstruction, before the SVG parser rounds emitted values.
        let actual_radius = (end[0] - center[0]).hypot(end[1] - center[1]);
        let distance = actual_radius * length / 100.;
        let actual_focus = [
            center[0] + distance * angle.to_radians().cos(),
            center[1] + distance * angle.to_radians().sin(),
        ];
        let [a, b, c, d, e, f] = self.transform.0.map(|v| v as f32);
        let mut shader_transform = tiny_skia::Transform::from_row(a, b, c, d, e, f);
        // usvg replaces invalid supplied transforms with identity before bbox
        // composition. Reject that change instead of flattening a different field.
        if !shader_transform.is_valid() {
            return Err("SVG radial gradient transform is invalid at renderer precision".into());
        }
        if self.object_bbox {
            let bounds = bounds.expect("nonzero bounds validated above");
            shader_transform = shader_transform.post_concat(tiny_skia::Transform::from_row(
                bounds.width(),
                0.,
                0.,
                bounds.height(),
                bounds.left(),
                bounds.top(),
            ));
        }
        validate_radial_precision(
            RadialCircle {
                center,
                radius,
                focus,
            },
            RadialCircle {
                center: shader_center.map(f64::from),
                radius: shader_radius as f64,
                focus: shader_focus.map(f64::from),
            },
            shader_transform,
            RadialCircle {
                center: center.map(|v| v as f32 as f64),
                radius: actual_radius as f32 as f64,
                focus: actual_focus.map(|v| v as f32 as f64),
            },
        )?;
        Ok((center, end, length, angle))
    }
    fn endpoints(&self, bounds: Option<tiny_skia::Rect>) -> Result<([f64; 2], [f64; 2]), String> {
        let Geometry::Linear {
            start: source_start,
            end: source_end,
        } = self.geometry
        else {
            unreachable!()
        };
        let t = self.transform(bounds)?;
        // t(p) = dot(d, inverse(A)(p - A start)) / dot(d,d).
        // Merely transforming endpoints is wrong under anisotropic scale/shear.
        let [a, b, c, d, _, _] = t.0;
        let det = a * d - b * c;
        let vector = [
            source_end[0] - source_start[0],
            source_end[1] - source_start[1],
        ];
        let norm = vector[0] * vector[0] + vector[1] * vector[1];
        if !det.is_finite() || det.abs() <= 1e-12 || norm <= 1e-24 {
            return Err("Degenerate SVG gradient field is unsupported".into());
        }
        let covector = [
            (d * vector[0] - b * vector[1]) / det / norm,
            (-c * vector[0] + a * vector[1]) / det / norm,
        ];
        let size = covector[0] * covector[0] + covector[1] * covector[1];
        let start = t.point(source_start);
        let end = [start[0] + covector[0] / size, start[1] + covector[1] / size];
        for value in start.into_iter().chain(end) {
            finite(value)?;
        }
        representable_length(start, end)?;
        let actual_start = start.map(|v| (v as f32) as f64);
        let actual_end = end.map(|v| (v as f32) as f64);
        validate_field_precision(start, covector, actual_start, actual_end)?;
        // The original transformed shader also uses f32. Reject transforms whose
        // rounding would change the field materially before endpoint flattening.
        let [fa, fb, fc, fd, fe, ff] = t.0.map(|v| v as f32);
        let transform = tiny_skia::Transform::from_row(fa, fb, fc, fd, fe, ff);
        let inverse = transform
            .invert()
            .ok_or("SVG gradient transform collapses at renderer precision")?;
        let source_start = source_start.map(|v| v as f32);
        let source_end = source_end.map(|v| v as f32);
        let dv = [
            source_end[0] - source_start[0],
            source_end[1] - source_start[1],
        ];
        let norm = dv[0] * dv[0] + dv[1] * dv[1];
        let shader_covector = [
            ((inverse.sx * dv[0] + inverse.ky * dv[1]) / norm) as f64,
            ((inverse.kx * dv[0] + inverse.sy * dv[1]) / norm) as f64,
        ];
        let shader_start = [
            (fa * source_start[0] + fc * source_start[1] + fe) as f64,
            (fb * source_start[0] + fd * source_start[1] + ff) as f64,
        ];
        compare_field_precision(start, covector, shader_start, shader_covector)?;
        Ok((start, end))
    }
}

#[cfg(test)]
mod renderer_precision_tests {
    fn reject_degenerate_field(attributes: &str) {
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'><defs><linearGradient id='ramp' gradientUnits='userSpaceOnUse' x1='0' y1='0' y2='0' {attributes}><stop offset='0' stop-color='black'/><stop offset='1' stop-color='white'/></linearGradient></defs><rect width='100' height='100' fill='url(#ramp)'/></svg>"
        );
        let error = super::super::parse(source.as_bytes())
            .expect_err("a renderer-degenerate gradient must not import as a different paint");
        assert!(error.to_ascii_lowercase().contains("gradient"), "{error}");
    }

    #[test]
    fn effective_short_gradient_rejects_instead_of_losing_black_half_plane() {
        // The literal source has a unit gradient transformed into a sharp boundary
        // at x=24. Collapsing the transform into endpoints produces a length below
        // tiny-skia's 1/32768 cutoff and otherwise turns the whole rect white.
        reject_degenerate_field("x2='1' gradientTransform='matrix(.00001 0 0 1 24 0)'");
    }

    #[test]
    fn original_short_gradient_rejects_even_when_transform_enlarges_it() {
        // tiny-skia checks the original endpoint length before gradientTransform.
        // Expanding these endpoints would otherwise turn a solid white source
        // into a visible black/white transition from x=24 to x=25.
        reject_degenerate_field("x2='.00001' gradientTransform='matrix(100000 0 0 1 24 0)'");
    }

    #[test]
    fn material_f32_endpoint_distortion_rejects_before_import() {
        // The intended endpoints 100000 and 100000.02 remain distinct as f32,
        // but their f32 separation is 0.0234375. Nested transforms put the rect
        // inside the viewport and enlarge that 17% field error to a visible
        // 2px versus 2.34375px ramp, without relying on offscreen geometry.
        let source = "<svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'><defs><linearGradient id='ramp' gradientUnits='userSpaceOnUse' x1='0' y1='0' x2='1' y2='0' gradientTransform='matrix(.02 0 0 1 100000 0)'><stop offset='0' stop-color='black'/><stop offset='1' stop-color='white'/></linearGradient></defs><g transform='scale(100)'><g transform='translate(-100000 0)'><rect x='100000' width='1' height='1' fill='url(#ramp)'/></g></g></svg>";
        let error = super::super::parse(source.as_bytes())
            .expect_err("material f32 field distortion must not silently change a gradient");
        assert!(error.to_ascii_lowercase().contains("gradient"), "{error}");
    }
}

// Match the pinned tiny-skia shader's actual degenerate-gradient boundary.
// Below this threshold it paints only the last stop, losing the two half-planes.
fn representable_length(start: [f64; 2], end: [f64; 2]) -> Result<(), String> {
    let dx = end[0] as f32 - start[0] as f32;
    let dy = end[1] as f32 - start[1] as f32;
    let length = (dx * dx + dy * dy).sqrt();
    if !length.is_finite() || length <= 1. / 32768. {
        return Err("SVG gradient endpoints are degenerate at renderer precision".into());
    }
    Ok(())
}

fn validate_field_precision(
    start: [f64; 2],
    covector: [f64; 2],
    actual_start: [f64; 2],
    actual_end: [f64; 2],
) -> Result<(), String> {
    let delta = [
        actual_end[0] - actual_start[0],
        actual_end[1] - actual_start[1],
    ];
    let norm = delta[0] * delta[0] + delta[1] * delta[1];
    compare_field_precision(
        start,
        covector,
        actual_start,
        [delta[0] / norm, delta[1] / norm],
    )
}
fn compare_field_precision(
    start: [f64; 2],
    covector: [f64; 2],
    actual_start: [f64; 2],
    actual: [f64; 2],
) -> Result<(), String> {
    let relative =
        (actual[0] - covector[0]).hypot(actual[1] - covector[1]) / covector[0].hypot(covector[1]);
    let shift = actual[0] * (actual_start[0] - start[0]) + actual[1] * (actual_start[1] - start[1]);
    if !relative.is_finite() || relative > 1e-6 || !shift.is_finite() || shift.abs() > 1e-6 {
        return Err("SVG gradient field loses precision in the editable renderer".into());
    }
    Ok(())
}
