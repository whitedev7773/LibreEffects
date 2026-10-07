//! Bounded static SVG insertion, independent of generic schema/asset migration.
use super::*;

#[cfg(test)]
#[path = "svg_import_tests.rs"]
mod tests;

/// A standalone import cannot be hidden in a mixed batch and accidentally run
/// the generic migrations. Inspect iteratively before candidate/command cloning.
pub(super) fn route(command: &Command) -> Result<bool, String> {
    if let Command::ImportSvg { contents, .. } = command {
        contents.validate(u32::MAX)?;
        return Ok(true);
    }
    let mut stack = vec![command];
    while let Some(command) = stack.pop() {
        match command {
            Command::ImportSvg { .. } => {
                return Err("SVG import must be a standalone transaction".into());
            }
            Command::Batch(commands) => stack.extend(commands),
            _ => {}
        }
    }
    Ok(false)
}

fn validate_static(nodes: &[ContentsNode]) -> Result<(), String> {
    for node in nodes {
        if node.parameters.values().any(|track| !track.keys.is_empty()) {
            return Err("SVG import supports static Contents only".into());
        }
        if node.blend != PaintBlend::Normal {
            return Err("SVG import does not support paint blending".into());
        }
        match &node.kind {
            ContentsKind::Group(children) => validate_static(children)?,
            ContentsKind::Path { animation, .. } if animation.is_default() => {}
            ContentsKind::Fill { .. } | ContentsKind::Stroke(_) => {}
            ContentsKind::GradientFill { gradient, .. }
            | ContentsKind::GradientStroke { gradient, .. }
                if gradient.colors_animation().is_none() => {}
            _ => {
                return Err(
                    "SVG import supports static paths, groups, solid paints and gradients only"
                        .into(),
                );
            }
        }
    }
    Ok(())
}

// Geometry exports through groups even when their own paint opacity is zero,
// matching Contents rendering. A paint consumes only preceding geometry.
fn paintable(nodes: &[ContentsNode]) -> (bool, bool) {
    let (mut paths, mut painted) = (false, false);
    for node in nodes.iter().filter(|node| node.enabled) {
        let visible_stops = node.kind.gradient().is_none_or(|gradient| {
            gradient.opacities.iter().any(|&id| {
                node.value_at(ContentsParam::Gradient(GradientParam::Opacity(id)), 0) > 0.
            })
        });
        match &node.kind {
            ContentsKind::Path { .. } => paths = true,
            ContentsKind::Group(children) => {
                let (child_paths, child_painted) = paintable(children);
                let has_area = node.value_at(ContentsParam::Transform(Property::ScaleX), 0) != 0.
                    && node.value_at(ContentsParam::Transform(Property::ScaleY), 0) != 0.;
                paths |= child_paths && has_area;
                painted |= child_painted
                    && has_area
                    && node.value_at(ContentsParam::Transform(Property::Opacity), 0) > 0.;
            }
            ContentsKind::Fill { .. } | ContentsKind::GradientFill { .. } => {
                painted |= paths
                    && visible_stops
                    && node.value_at(ContentsParam::Shape(ShapeParam::FillOpacity), 0) > 0.;
            }
            ContentsKind::Stroke(_) | ContentsKind::GradientStroke { .. } => {
                painted |= paths
                    && visible_stops
                    && node.value_at(ContentsParam::Shape(ShapeParam::StrokeOpacity), 0) > 0.
                    && node.value_at(ContentsParam::Shape(ShapeParam::StrokeWidth), 0) > 0.;
            }
            _ => {}
        }
    }
    (paths, painted)
}

pub(super) fn apply(state: &mut Snapshot, command: Command) -> Result<(), String> {
    let Command::ImportSvg {
        contents,
        width,
        height,
        name,
    } = command
    else {
        unreachable!("the dedicated route admits only ImportSvg")
    };
    if ![width, height]
        .into_iter()
        .all(|v| v.is_finite() && v.fract() == 0.0 && (1.0..=16384.0).contains(&v))
    {
        return Err("SVG viewport dimensions must be whole pixels from 1–16384; fractional viewport clipping is unsupported".into());
    }
    if name.trim().is_empty() || name.len() > 1024 {
        return Err("SVG layer name is empty or too long".into());
    }
    validate_static(&contents.items)?;
    if !paintable(&contents.items).1 {
        return Err("SVG import has no painted paths".into());
    }
    let mut contents = ShapeContents::from_nodes(contents.items)?;
    // Admit the actual bounded evaluated representation before recording history.
    contents.svg_at(0).map_err(|error| error.to_string())?;
    // 44 is the stable current base Contents schema; loading 43 upgrades it.
    let mut required = if contents
        .rows()
        .iter()
        .any(|(_, _, node)| node.kind.gradient().is_some())
    {
        45
    } else {
        44
    };
    if contents
        .rows()
        .iter()
        .any(|(_, _, node)| node.composite != PaintComposite::BelowPrevious)
    {
        required = required.max(46);
    }
    let legacy_groups = state.project.version == 43
        && state.project.compositions().into_iter().flat_map(|(_, comp)| &comp.layers).any(|layer| {
            matches!(&layer.content, Content::ShapeContents(source)
                if source.rows().iter().any(|(_, _, node)| matches!(node.kind, ContentsKind::Group(_))))
        });
    if required == 44
        && legacy_groups
        && contents.rows().iter().all(|(_, _, node)| {
            !matches!(node.kind, ContentsKind::Group(_))
                || (node.value_at(ContentsParam::Skew, 0) == 0.
                    && node.value_at(ContentsParam::SkewAxis, 0) == 0.)
        })
    {
        // Only fresh imported defaults are omitted. Existing v43 trees and all
        // authored source data remain byte-for-byte unchanged.
        fn omit_skew(nodes: &mut [ContentsNode]) {
            for node in nodes {
                if let ContentsKind::Group(children) = &mut node.kind {
                    node.parameters.remove(&ContentsParam::Skew);
                    node.parameters.remove(&ContentsParam::SkewAxis);
                    omit_skew(children);
                }
            }
        }
        omit_skew(&mut contents.items);
        required = 43;
    }
    let version = state.project.version.max(required);
    contents.validate_version(state.project.composition.duration, version)?;
    super::apply(state, Command::AddRectangle)?;
    let layer = state.project.composition.layers.first_mut().unwrap();
    layer.content = Content::ShapeContents(contents);
    layer.width = width;
    layer.height = height;
    layer.name = name;
    layer.color = 0xffffff;
    for property in [
        Property::PositionX,
        Property::PositionY,
        Property::AnchorX,
        Property::AnchorY,
    ] {
        layer.properties.get_mut(&property).unwrap().value = 0.;
    }
    // SVG's outer viewport clips overflow. A regular editable mask preserves
    // that boundary without baking pixels or adding a special schema field.
    mask_animation::set(
        layer,
        &[PathMask {
            path: VectorPath {
                vertices: [[0., 0.], [width, 0.], [width, height], [0., height]]
                    .map(PathVertex::corner)
                    .to_vec(),
                closed: true,
            },
            ..Default::default()
        }],
    )?;
    state.project.version = version;
    Ok(())
}
