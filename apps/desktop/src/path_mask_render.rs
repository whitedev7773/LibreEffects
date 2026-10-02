use libre_effects_core::{Layer, PathMaskMode};

/// Ordered mask coverage, shared by ordinary and adjustment layers.
pub(crate) fn mask(layer: &Layer, id: &str) -> (String, String) {
    let masks: Vec<_> = layer
        .path_masks()
        .iter()
        .filter(|m| m.mode != PathMaskMode::None)
        .collect();
    if masks.is_empty() {
        return (String::new(), String::new());
    }
    let width = layer.width();
    let height = layer.height();
    let mut coverage = if masks[0].mode == PathMaskMode::Add {
        String::new()
    } else {
        format!("<rect width='{width}' height='{height}' fill='white'/>")
    };
    let mut defs = String::new();
    for (index, mask) in masks.into_iter().enumerate() {
        let data = format!(
            "{}{}",
            if mask.inverted {
                format!("M0 0 H{width} V{height} H0 Z ")
            } else {
                String::new()
            },
            mask.path.svg_data()
        );
        match mask.mode {
            PathMaskMode::Add | PathMaskMode::Subtract => coverage.push_str(&format!(
                "<path d='{data}' fill-rule='evenodd' fill='{}'/>",
                if mask.mode == PathMaskMode::Add {
                    "white"
                } else {
                    "black"
                }
            )),
            PathMaskMode::Intersect => {
                defs.push_str(&format!("<clipPath id='{id}-intersection-{index}'><path d='{data}' clip-rule='evenodd'/></clipPath>"));
                coverage = format!("<g clip-path='url(#{id}-intersection-{index})'>{coverage}</g>");
            }
            PathMaskMode::None => {}
        }
    }
    (
        format!(
            "<defs>{defs}<mask id='{id}-paths' maskUnits='userSpaceOnUse' x='0' y='0' width='{width}' height='{height}' mask-type='luminance'>{coverage}</mask></defs>"
        ),
        format!("mask='url(#{id}-paths)'"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::Renderer;
    use libre_effects_core::{Command, Content, Editor, PathMask, PathVertex, VectorPath};
    fn box_mask(x: f64, right: f64, mode: PathMaskMode) -> PathMask {
        PathMask {
            path: VectorPath {
                closed: true,
                vertices: [[x, 0.0], [right, 0.0], [right, 100.0], [x, 100.0]]
                    .map(PathVertex::corner)
                    .to_vec(),
            },
            mode,
            inverted: false,
        }
    }
    #[test]
    fn ordered_mask_coverage_add_subtract_intersect_invert_and_none() {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Masks".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 30,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Solid,
            width: 100.0,
            height: 100.0,
            name: "Solid".into(),
        })
        .unwrap();
        for (masks, expected) in [
            (
                vec![
                    box_mask(0.0, 50.0, PathMaskMode::Add),
                    box_mask(20.0, 40.0, PathMaskMode::Subtract),
                ],
                [255, 0, 255, 0],
            ),
            (
                vec![
                    box_mask(0.0, 50.0, PathMaskMode::Add),
                    box_mask(20.0, 100.0, PathMaskMode::Intersect),
                ],
                [0, 255, 255, 0],
            ),
            (
                vec![box_mask(20.0, 40.0, PathMaskMode::Subtract)],
                [255, 0, 255, 255],
            ),
            (
                vec![PathMask {
                    inverted: true,
                    ..box_mask(20.0, 40.0, PathMaskMode::Add)
                }],
                [255, 0, 255, 255],
            ),
            (vec![box_mask(20.0, 40.0, PathMaskMode::None)], [255; 4]),
        ] {
            e.execute(Command::SetPathMasks { id: 1, masks }).unwrap();
            let r = Renderer::new();
            let svg = r
                .isolated_layer_svg(
                    e.project(),
                    e.project().active_composition_id(),
                    1,
                    0,
                    100,
                    "qa",
                    &mut 0,
                )
                .unwrap();
            let pixels = r.raster_canvas(&svg, 100, 100, 100).unwrap();
            for (x, alpha) in [10, 30, 45, 75].into_iter().zip(expected) {
                assert_eq!(pixels.pixel(x, 50).unwrap().alpha(), alpha, "x={x}");
            }
        }
    }
}
