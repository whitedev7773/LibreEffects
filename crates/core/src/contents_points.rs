//! Current-frame, source-preserving point edits across a layer's Contents paths.
use super::*;
use path_transform::{linear_component, linear_matrix, transform_anchor_component};

type Matrix = [[f64; 2]; 2];

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| linear_component(a[row], [b[0][column], b[1][column]]))
    })
}

fn vector(matrix: Matrix, point: [f64; 2]) -> [f64; 2] {
    matrix.map(|row| linear_component(row, point))
}

// Match the editor's existing eligibility: Affine::inverse rejects singular or
// near-singular determinants (abs < 1e-10), nonfinite coefficients and inverse
// coefficients outside the supported +/-1e12 range.
fn inverse_linear(world: Affine) -> Result<Matrix, String> {
    if world.0.iter().any(|value| !value.is_finite()) {
        return Err("Path world transform must be finite".into());
    }
    let inverse = world
        .inverse()
        .ok_or("Cannot edit through a singular, near-singular or unsupported path transform")?;
    let [a, b, c, d, _, _] = inverse.0;
    Ok([[a, c], [b, d]])
}

/// Transform selected local vertices in composition/world axes. `world` maps
/// this path's coordinates to composition coordinates, including the layer and
/// all ancestor groups. Scale about the world pivot, rotate, then translate;
/// tangent offsets receive only the conjugated linear transform.
///
/// Pure preview/command geometry: validates source, selection, parameters and
/// supported invertibility even for identity. No geometry snapping or world round-trip is
/// used, so deliberate identity preserves authored floating-point bits.
pub fn transform_path_in_world(
    source: &VectorPath,
    indices: &BTreeSet<usize>,
    transform: &PathTransformSpec,
    world: Affine,
) -> Result<VectorPath, String> {
    if !source.valid() {
        return Err("Invalid source path geometry".into());
    }
    if indices.is_empty() || indices.iter().any(|index| *index >= source.vertices.len()) {
        return Err("Choose existing path vertices".into());
    }
    let matrix = linear_matrix(transform)?;
    let inverse = inverse_linear(world)?;
    let identity = [[1., 0.], [0., 1.]];
    if matrix == identity && transform.translation == [0., 0.] {
        return Ok(source.clone());
    }
    let translation = vector(inverse, transform.translation);
    let [a, b, c, d, tx, ty] = world.0;
    let parent = [[a, c], [b, d]];
    // Translation is pivot-independent; avoid irrelevant overflow for finite
    // enormous pivots while still validating all input fields above.
    let pivot = if matrix == identity {
        [0., 0.]
    } else {
        vector(inverse, [transform.pivot[0] - tx, transform.pivot[1] - ty])
    };
    let correction = [
        [matrix[0][0] - 1., matrix[0][1]],
        [matrix[1][0], matrix[1][1] - 1.],
    ];
    let near_identity = correction.iter().flatten().all(|value| value.abs() <= 0.5);
    // Form the small correction directly instead of subtracting one from a
    // composed coefficient. Tiny real rotations/scales must not become no-ops.
    let local = multiply(
        inverse,
        multiply(if near_identity { correction } else { matrix }, parent),
    );
    if translation
        .iter()
        .chain(&pivot)
        .chain(local.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err("Transformed path coordinates must be finite".into());
    }
    let mut result = source.clone();
    for &index in indices {
        let before = source.vertices[index];
        let after = &mut result.vertices[index];
        for axis in 0..2 {
            let value = if matrix == identity {
                before.position[axis] + translation[axis]
            } else if near_identity {
                before.position[axis]
                    + (linear_component(
                        local[axis],
                        [before.position[0] - pivot[0], before.position[1] - pivot[1]],
                    ) + translation[axis])
            } else {
                transform_anchor_component(
                    local[axis],
                    axis,
                    before.position,
                    pivot,
                    translation[axis],
                )
            };
            if value != before.position[axis] {
                after.position[axis] = value;
            }
            for (original, edited) in [
                (before.incoming, &mut after.incoming),
                (before.outgoing, &mut after.outgoing),
            ] {
                let value = if matrix == identity {
                    original[axis]
                } else if near_identity {
                    original[axis] + linear_component(local[axis], original)
                } else {
                    linear_component(local[axis], original)
                };
                if value != original[axis] {
                    edited[axis] = value;
                }
            }
        }
    }
    if !result.valid() {
        return Err("Resulting anchor and tangent coordinates must be finite and within -1000000 to 1000000".into());
    }
    Ok(result)
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::TransformContentsPoints {
        id,
        frame,
        selections,
        transform,
    } = command
    else {
        return None;
    };
    Some((|| {
        // Validate before traversal: duplicate IDs or malformed historical
        // pools cannot be repaired by selecting a convenient sampled pose.
        // Document bytes are budgeted once at the outer atomic acceptance.
        state.project.validate()?;
        let composition = &state.project.composition;
        if *frame >= composition.duration {
            return Err("Frame is outside the active composition".into());
        }
        if selections.is_empty() {
            return Err("Select at least one Contents path vertex".into());
        }
        let layer = composition
            .layer(*id)
            .ok_or("Layer not found in the active composition")?;
        if layer.locked {
            return Err("Unlock the layer before editing".into());
        }
        let Content::ShapeContents(contents) = &layer.content else {
            return Err("Select editable Shape Contents paths on one layer".into());
        };
        let world = composition
            .world_transform(*id, *frame)
            .ok_or("Invalid layer world transform")?;
        let paths: BTreeMap<_, _> = contents
            .editable_paths(*frame)
            .into_iter()
            .map(|(item, path, space)| (item, (path, world.compose(space))))
            .collect();
        // Prepare every output before writing, including unchanged members.
        // Enabled traversal excludes disabled ancestors and parametric shapes.
        let mut outputs = Vec::with_capacity(selections.len());
        for (item, indices) in selections {
            let (source, space) = paths
                .get(item)
                .ok_or("Selected Contents path is missing, disabled or not editable")?;
            let output = transform_path_in_world(source, indices, transform, *space)
                .map_err(|error| format!("Contents path {item}: {error}"))?;
            if output != *source {
                outputs.push((*item, output));
            }
        }
        let layer = editing::editable(state, *id)?;
        for (item, output) in outputs {
            let (base, animation) = layer
                .path_animation_mut(PathTarget::Contents(item))
                .ok_or("Selected Contents path no longer exists")?;
            animation.edit_sample_preserving_source(base, *frame, &output)?;
        }
        Ok(())
    })())
}

#[cfg(test)]
#[path = "contents_points_tests.rs"]
mod tests;
