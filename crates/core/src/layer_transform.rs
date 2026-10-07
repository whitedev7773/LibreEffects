//! Current-frame layer essentials, with changed-channel-only scalar edits.
use super::*;

/// Spatial operations on source rectangles, not effect, mask, ink or descendant bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerTransformOp {
    ResetScaleRotation,
    FlipHorizontal,
    FlipVertical,
    FitInsideComposition,
    CenterAnchorInSourceBounds,
}

// These operations only edit existing transform channels. Empty nested batches
// are harmless, but a wholly empty batch retains the historical migration path.
pub(super) fn edits_only(command: &Command) -> bool {
    fn classify(command: &Command) -> Option<bool> {
        match command {
            Command::TransformLayers { .. } => Some(true),
            Command::Batch(commands) => {
                let mut has_transform = false;
                for command in commands {
                    has_transform |= classify(command)?;
                }
                Some(has_transform)
            }
            _ => None,
        }
    }
    classify(command) == Some(true)
}

fn changed_value(
    commands: &mut Vec<Command>,
    layer: &Layer,
    frame: Frame,
    property: Property,
    value: f64,
) -> Result<(), String> {
    // Sampled animation may legitimately overshoot an input field's range.
    // An unchanged finite channel is not a new value and must keep its keys.
    // This also preserves the original bits of +0/-0 for a zero-scale flip.
    if value.is_finite()
        && layer
            .property(property)
            .ok_or("Scalar transform property is unavailable")?
            .value_at(frame)
            == value
    {
        return Ok(());
    }
    if !property.accepts(value) {
        return Err(format!(
            "{} would be outside the supported range; adjust the layer or its parent first",
            property.label()
        ));
    }
    commands.push(Command::SetValue {
        id: layer.id,
        property,
        frame,
        value,
    });
    Ok(())
}

// Normalize before testing rank so tiny uniform transforms remain recoverable:
// their raw determinant can underflow even though their source has two axes.
// No determinant epsilon or inverse is needed for this source-space test.
fn noncollapsed(transform: Affine) -> bool {
    let [a, b, c, d, _, _] = transform.0;
    let magnitude = a.abs().max(b.abs()).max(c.abs()).max(d.abs());
    if !magnitude.is_finite() || magnitude == 0.0 {
        return false;
    }
    let determinant = (a / magnitude) * (d / magnitude) - (b / magnitude) * (c / magnitude);
    determinant.is_finite() && determinant != 0.0
}

fn fit_space(comp: &Composition, layer: &Layer, frame: Frame) -> Result<(Affine, Affine), String> {
    if layer
        .property(Property::ScaleX)
        .ok_or("Scalar transform property is unavailable")?
        .value_at(frame)
        == 0.0
        || layer
            .property(Property::ScaleY)
            .ok_or("Scalar transform property is unavailable")?
            .value_at(frame)
            == 0.0
    {
        return Err("Cannot fit a collapsed layer; restore nonzero X and Y scale first".into());
    }
    let inverse = comp
        .position_space(layer.id, frame)
        .and_then(Affine::inverse)
        .ok_or("Cannot fit through a singular or unsupported parent transform; adjust the parent scale first")?;
    let world = comp.world_transform(layer.id, frame).ok_or(
        "Cannot fit an unsupported layer transform; reduce the layer or parent transform first",
    )?;
    if !noncollapsed(world) {
        return Err(
            "Cannot fit a collapsed source transform; adjust the layer or parent scales first"
                .into(),
        );
    }
    Ok((world, inverse))
}

// A forward-roundoff budget, not a visual/pixel snap threshold. A matrix
// component traverses at most two affine products (6 scalar operations each)
// per hierarchy level; 32 operations per level plus 32 for local trig/scale,
// source extents, ratio and centering conservatively covers this calculation.
// gamma(n) = n*eps/(1-n*eps). The maximum hierarchy is 1000 layers, so even the
// maximum relative budget remains below 8e-12. Ordinary channel comparisons
// stay exact; this budget only recognizes a numerically solved fit.
fn fit_roundoff(comp: &Composition, layer: &Layer) -> f64 {
    let mut levels = 1usize;
    let mut parent = layer.parent;
    while let Some(id) = parent {
        levels += 1;
        if levels > comp.layers.len() {
            break;
        }
        parent = comp.layer(id).and_then(|l| l.parent);
    }
    let error = (32 * (levels + 1)) as f64 * f64::EPSILON;
    error / (1.0 - error)
}

fn fit(
    comp: &Composition,
    layer: &Layer,
    frame: Frame,
    commands: &mut Vec<Command>,
) -> Result<(), String> {
    let (world, inverse) = fit_space(comp, layer, frame)?;
    let [a, b, c, d, _, _] = world.0;
    // Linear extents avoid loss of source width when translated far from zero.
    let size = [
        a.abs() * layer.width + c.abs() * layer.height,
        b.abs() * layer.width + d.abs() * layer.height,
    ];
    if size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err("Cannot fit a collapsed or nonfinite source rectangle".into());
    }
    let dimensions = [comp.width as f64, comp.height as f64];
    let mut factor = (dimensions[0] / size[0]).min(dimensions[1] / size[1]);
    if !factor.is_finite() || factor <= 0.0 {
        return Err("Fitted scale is outside the supported range".into());
    }
    let tolerance = fit_roundoff(comp, layer);
    let scale_solved = (factor - 1.0).abs() <= tolerance;
    let center = world.point([layer.width * 0.5, layer.height * 0.5]);
    let goal = [dimensions[0] * 0.5, dimensions[1] * 0.5];
    if scale_solved {
        // Do not turn floating-point noise into more animated scale keys.
        factor = 1.0;
        if (0..2).all(|axis| (center[axis] - goal[axis]).abs() <= dimensions[axis] * tolerance) {
            return Ok(());
        }
    }
    let sx = layer
        .property(Property::ScaleX)
        .ok_or("Scalar transform property is unavailable")?
        .value_at(frame)
        * factor;
    let sy = layer
        .property(Property::ScaleY)
        .ok_or("Scalar transform property is unavailable")?
        .value_at(frame)
        * factor;
    changed_value(commands, layer, frame, Property::ScaleX, sx)?;
    changed_value(commands, layer, frame, Property::ScaleY, sy)?;
    // The center must land on inverse(position-space) * composition-center.
    // Rebuild the local matrix from the new scale values using the same order
    // as the renderer, rather than multiplying an already-rounded matrix.
    let mut preview = layer.clone();
    preview
        .properties
        .insert(Property::ScaleX, AnimatedProperty::new(sx));
    preview
        .properties
        .insert(Property::ScaleY, AnimatedProperty::new(sy));
    let local_center = preview
        .local_transform(frame)
        .ok_or("Cannot fit a 3D layer with a 2D transform")?
        .vector([
            layer.width * 0.5
                - layer
                    .property(Property::AnchorX)
                    .ok_or("Scalar transform property is unavailable")?
                    .value_at(frame),
            layer.height * 0.5
                - layer
                    .property(Property::AnchorY)
                    .ok_or("Scalar transform property is unavailable")?
                    .value_at(frame),
        ]);
    let target = inverse.point(goal);
    let position = [target[0] - local_center[0], target[1] - local_center[1]];
    for (property, value) in [
        (Property::PositionX, position[0]),
        (Property::PositionY, position[1]),
    ] {
        changed_value(commands, layer, frame, property, value)?;
        preview
            .properties
            .insert(property, AnimatedProperty::new(value));
    }
    // Match the renderer's composition order, including each offset. A valid
    // inverse alone cannot guarantee accuracy through cancelling translations.
    let mut result = Affine::default();
    let mut current = Some(layer.id);
    while let Some(id) = current {
        let l = if id == layer.id {
            &preview
        } else {
            comp.layer(id).unwrap()
        };
        result = l
            .transform_offset
            .compose(
                l.local_transform(frame)
                    .ok_or("Cannot fit through a 3D parent transform")?,
            )
            .compose(result);
        current = l.parent;
    }
    let center = result.point([layer.width * 0.5, layer.height * 0.5]);
    let [a, b, c, d, _, _] = result.0;
    let saturation = ((a.abs() * layer.width + c.abs() * layer.height) / dimensions[0])
        .max((b.abs() * layer.width + d.abs() * layer.height) / dimensions[1]);
    if !result.valid()
        || !noncollapsed(result)
        || !saturation.is_finite()
        || (saturation - 1.0).abs() > tolerance
        || (0..2).any(|axis| {
            !center[axis].is_finite()
                || (center[axis] - goal[axis]).abs() > dimensions[axis] * tolerance
        })
    {
        return Err("Cannot fit accurately in this parent transform; reduce large or cancelling transforms first".into());
    }
    Ok(())
}

pub(super) fn apply(
    state: &mut Snapshot,
    ids: &[LayerId],
    frame: Frame,
    operation: LayerTransformOp,
) -> Result<(), String> {
    let comp = &state.project.composition;
    if frame >= comp.duration {
        return Err("Transform frame must be inside the composition".into());
    }
    let roots = comp.selection_roots(ids)?;
    // Validate every explicit member, even selected descendants carried by a root.
    for id in ids.iter().copied().collect::<BTreeSet<_>>() {
        let layer = comp.layer(id).ok_or("Selected layer no longer exists")?;
        comp.require_two_d_transform(id)?;
        if matches!(layer.content, Content::Audio { .. }) {
            return Err(
                "Audio layers do not support spatial transforms; select visual layers".into(),
            );
        }
        if matches!(
            operation,
            LayerTransformOp::FitInsideComposition | LayerTransformOp::CenterAnchorInSourceBounds
        ) && layer.content == Content::Null
        {
            return Err("Null layers have no source bounds; select visual layers".into());
        }
        if operation == LayerTransformOp::FitInsideComposition {
            fit_space(comp, layer, frame)?;
        }
    }
    let targets = if operation == LayerTransformOp::CenterAnchorInSourceBounds {
        ids.iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        roots
    };
    let mut commands = Vec::new();
    for id in targets {
        let layer = comp.layer(id).ok_or("Selected layer no longer exists")?;
        match operation {
            LayerTransformOp::ResetScaleRotation => {
                for (property, value) in [
                    (Property::ScaleX, 100.0),
                    (Property::ScaleY, 100.0),
                    (Property::Rotation, 0.0),
                ] {
                    changed_value(&mut commands, layer, frame, property, value)?;
                }
            }
            LayerTransformOp::FlipHorizontal | LayerTransformOp::FlipVertical => {
                let property = if operation == LayerTransformOp::FlipHorizontal {
                    Property::ScaleX
                } else {
                    Property::ScaleY
                };
                changed_value(
                    &mut commands,
                    layer,
                    frame,
                    property,
                    -layer
                        .property(property)
                        .ok_or("Scalar transform property is unavailable")?
                        .value_at(frame),
                )?;
            }
            LayerTransformOp::FitInsideComposition => fit(comp, layer, frame, &mut commands)?,
            LayerTransformOp::CenterAnchorInSourceBounds => {
                let center = [layer.width * 0.5, layer.height * 0.5];
                // As in SetAnchor, compensate in local position space. No inverse
                // parent is needed, even for a zero-scale ancestor.
                let delta = layer
                    .local_transform(frame)
                    .ok_or("Cannot center a 3D layer with a 2D transform")?
                    .vector([
                        center[0]
                            - layer
                                .property(Property::AnchorX)
                                .ok_or("Scalar transform property is unavailable")?
                                .value_at(frame),
                        center[1]
                            - layer
                                .property(Property::AnchorY)
                                .ok_or("Scalar transform property is unavailable")?
                                .value_at(frame),
                    ]);
                for (property, value) in [
                    (Property::AnchorX, center[0]),
                    (Property::AnchorY, center[1]),
                    (
                        Property::PositionX,
                        layer
                            .property(Property::PositionX)
                            .ok_or("Scalar transform property is unavailable")?
                            .value_at(frame)
                            + delta[0],
                    ),
                    (
                        Property::PositionY,
                        layer
                            .property(Property::PositionY)
                            .ok_or("Scalar transform property is unavailable")?
                            .value_at(frame)
                            + delta[1],
                    ),
                ] {
                    changed_value(&mut commands, layer, frame, property, value)?;
                }
            }
        }
    }
    for command in commands {
        super::apply(state, command)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
