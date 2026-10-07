//! Selected path-local affine transforms shared by the editor and all-pose commands.
use super::*;

/// UI units: translation/pivot in the command's coordinate space, clockwise rotation in degrees,
/// and signed scale percentages (100 is identity). Zero scale is supported.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathTransformSpec {
    pub translation: [f64; 2],
    pub rotation_degrees: f64,
    pub scale_percent: [f64; 2],
    pub pivot: [f64; 2],
}

impl Default for PathTransformSpec {
    fn default() -> Self {
        Self {
            translation: [0.; 2],
            rotation_degrees: 0.,
            scale_percent: [100.; 2],
            pivot: [0.; 2],
        }
    }
}

impl From<[f64; 7]> for PathTransformSpec {
    fn from([dx, dy, rotation_degrees, sx, sy, px, py]: [f64; 7]) -> Self {
        Self {
            translation: [dx, dy],
            rotation_degrees,
            scale_percent: [sx, sy],
            pivot: [px, py],
        }
    }
}

/// Scale local axes, then rotate clockwise in the normal downward-Y canvas,
/// then translate anchors. Tangent offsets receive only the linear transform.
/// Source geometry, a nonempty selection and all parameters are validated even
/// for an exact identity. No epsilon, snapping, or inverse is involved.
pub fn transform_path(
    source: &VectorPath,
    indices: &BTreeSet<usize>,
    transform: &PathTransformSpec,
) -> Result<VectorPath, String> {
    if !source.valid() {
        return Err("Invalid source path: anchors and tangents must be finite and within -1000000 to 1000000, with 2–1024 open or 3–1024 closed vertices".into());
    }
    if indices.is_empty() {
        return Err("Select at least one path vertex".into());
    }
    if indices.iter().any(|index| *index >= source.vertices.len()) {
        return Err("Selected path vertex no longer exists".into());
    }
    let matrix = linear_matrix(transform)?;
    let PathTransformSpec {
        translation: [dx, dy],
        pivot: [px, py],
        ..
    } = *transform;
    if matrix == [[1., 0.], [0., 1.]] && dx == 0. && dy == 0. {
        return Ok(source.clone());
    }
    let mut path = source.clone();
    for &index in indices {
        let original = source.vertices[index];
        let vertex = &mut path.vertices[index];
        for axis in 0..2 {
            let row = matrix[axis];
            let position =
                transform_anchor_component(row, axis, original.position, [px, py], [dx, dy][axis]);
            if position != original.position[axis] {
                vertex.position[axis] = position;
            }
            for (before, after) in [
                (original.incoming, &mut vertex.incoming),
                (original.outgoing, &mut vertex.outgoing),
            ] {
                let value = linear_component(row, before);
                if value != before[axis] {
                    after[axis] = value;
                }
            }
        }
    }
    if !path.valid() {
        return Err("Resulting anchor and tangent coordinates must be finite and within -1000000 to 1000000".into());
    }
    Ok(path)
}

pub(super) fn linear_matrix(transform: &PathTransformSpec) -> Result<[[f64; 2]; 2], String> {
    let PathTransformSpec {
        translation: [dx, dy],
        rotation_degrees: rotation,
        scale_percent: [sx, sy],
        pivot: [px, py],
    } = *transform;
    if [dx, dy, rotation, sx, sy, px, py]
        .iter()
        .any(|value| !value.is_finite())
    {
        return Err("Enter finite numeric values".into());
    }
    // Signed remainder preserves tiny negative turns that adding 360 would round
    // away. Exact cardinal coefficients avoid trigonometric no-op drift.
    let angle = rotation % 360.;
    let (sin, cos) = match angle {
        0. => (0., 1.),
        90. | -270. => (1., 0.),
        180. | -180. => (0., -1.),
        270. | -90. => (-1., 0.),
        _ => angle.to_radians().sin_cos(),
    };
    let matrix = [
        [cos * (sx / 100.), -sin * (sy / 100.)],
        [sin * (sx / 100.), cos * (sy / 100.)],
    ];
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        return Err("Transform coefficients must be finite".into());
    }
    Ok(matrix)
}

pub(super) fn transform_anchor_component(
    row: [f64; 2],
    axis: usize,
    source: [f64; 2],
    pivot: [f64; 2],
    translation: f64,
) -> f64 {
    if row == [0., 0.] {
        // Exact collapse must not leave cancellation residue per vertex.
        return pivot[axis] + translation;
    }
    let signed_axis = match row {
        [1., 0.] => Some((0, 1.)),
        [-1., 0.] => Some((0, -1.)),
        [0., 1.] => Some((1, 1.)),
        [0., -1.] => Some((1, -1.)),
        _ => None,
    };
    if let Some((component, sign)) = signed_axis {
        if source[component] == pivot[component] {
            // At this row's exact fixed component, avoid subtracting large pivot
            // terms that could erase the other, much smaller pivot component.
            return pivot[axis] + translation;
        }
        // Identity rows and cardinal swaps retain tiny source components even
        // with equal, enormous pivots. No source component is canceled back out.
        return sign * source[component] + (pivot[axis] - sign * pivot[component]) + translation;
    }
    let relative = [source[0] - pivot[0], source[1] - pivot[1]];
    let correction = [
        row[0] - if axis == 0 { 1. } else { 0. },
        row[1] - if axis == 1 { 1. } else { 0. },
    ];
    if correction.iter().all(|value| value.abs() <= 0.5) {
        // Near identity, a correction retains small motion about a large pivot.
        // This condition only chooses arithmetic: no coefficient or result is
        // treated as zero. Away from identity, subtraction of the original
        // component could erase a genuine tiny scale or swapped coordinate.
        source[axis] + (linear_component(correction, relative) + translation)
    } else {
        pivot[axis] + linear_component(row, relative) + translation
    }
}

pub(super) fn linear_component(row: [f64; 2], point: [f64; 2]) -> f64 {
    // Zero coefficients also avoid unnecessary 0 × overflow intermediates.
    match row {
        [0., 0.] => 0.,
        [1., 0.] => point[0],
        [0., 1.] => point[1],
        [a, 0.] => a * point[0],
        [0., b] => b * point[1],
        [a, b] => a * point[0] + b * point[1],
    }
}

// A mixed batch, or any empty nested batch, retains the existing migration path.
pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::TransformPathPoses { .. } | Command::TransformContentsPoints { .. } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}

#[cfg(test)]
#[path = "path_transform_tests.rs"]
mod tests;
