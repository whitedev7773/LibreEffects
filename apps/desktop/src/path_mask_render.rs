use libre_effects_core::{Layer, MaskParam, PathMaskMode};

/// Evaluate mask coverage before layer effects, in layer space at composition time.
pub(crate) fn mask(layer: &Layer, id: &str, frame: u32) -> (String, String) {
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
    let rect = |color: &str| format!("<rect width='{width}' height='{height}' fill='{color}'/>");
    let mut coverage = if masks[0].mode == PathMaskMode::Add {
        String::new()
    } else {
        rect("white")
    };
    let mut defs = String::new();
    for mask in masks {
        let key = format!("{id}-mask-{}", mask.id);
        let expansion = mask.value_at(MaskParam::Expansion, frame);
        let feather = mask.value_at(MaskParam::Feather, frame);
        let opacity = mask.value_at(MaskParam::Opacity, frame) / 100.0;
        let pad = expansion.abs() + feather * 3.0 + 2.0;
        let mut filter = String::new();
        if expansion != 0.0 {
            filter.push_str(&format!(
                "<feMorphology operator='{}' radius='{}'/>",
                if expansion > 0.0 { "dilate" } else { "erode" },
                expansion.abs()
            ));
        }
        if mask.inverted {
            filter.push_str("<feColorMatrix values='0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 0 0 0 -1 1'/>");
        }
        if feather > 0.0 {
            filter.push_str(&format!(
                "<feGaussianBlur stdDeviation='{}'/>",
                feather / 2.0
            ));
        }
        let filtered = if filter.is_empty() {
            String::new()
        } else {
            defs.push_str(&format!("<filter id='{key}-filter' filterUnits='userSpaceOnUse' x='{}' y='{}' width='{}' height='{}'>{filter}</filter>", -pad, -pad, width + 2.0*pad, height + 2.0*pad));
            format!("filter='url(#{key}-filter)'")
        };
        defs.push_str(&format!("<mask id='{key}' maskUnits='userSpaceOnUse' x='0' y='0' width='{width}' height='{height}' mask-type='alpha'><g opacity='{opacity}'><path d='{}' fill='white' fill-rule='evenodd' {filtered}/></g></mask>", mask.path_at(frame).svg_data()));
        coverage = match mask.mode {
            PathMaskMode::Add | PathMaskMode::Subtract => format!(
                "{coverage}<g mask='url(#{key})'>{}</g>",
                rect(if mask.mode == PathMaskMode::Add {
                    "white"
                } else {
                    "black"
                })
            ),
            PathMaskMode::Intersect => format!("<g mask='url(#{key})'>{coverage}</g>"),
            PathMaskMode::None => coverage,
        };
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
            ..Default::default()
        }
    }
    #[test]
    fn animated_shape_and_mask_paths_share_preview_and_saved_frame_geometry() {
        use libre_effects_core::{PathTarget, Project, Shape, TrackEdit};
        let r = Renderer::new();
        for is_mask in [false, true] {
            let mut e = Editor::default();
            e.execute(Command::ConfigureComposition {
                name: "Morph".into(),
                width: 100,
                height: 100,
                fps: 30,
                duration: 60,
            })
            .unwrap();
            let base = box_mask(10.0, 30.0, PathMaskMode::Add).path;
            e.execute(Command::AddContent {
                content: if is_mask {
                    Content::Solid
                } else {
                    Content::Shape(Shape {
                        path: Some(base.clone()),
                        ..Default::default()
                    })
                },
                width: 100.0,
                height: 100.0,
                name: "Morph".into(),
            })
            .unwrap();
            let target = if is_mask {
                e.execute(Command::SetPathMasks {
                    id: 1,
                    masks: vec![PathMask {
                        path: base,
                        ..Default::default()
                    }],
                })
                .unwrap();
                PathTarget::Mask(1)
            } else {
                PathTarget::Shape
            };
            e.execute(Command::AnimatePath {
                id: 1,
                target,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
            e.execute(Command::EditPath {
                id: 1,
                target,
                frame: 20,
                path: box_mask(50.0, 70.0, PathMaskMode::Add).path,
            })
            .unwrap();
            let restored = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            for (frame, x) in [(0, 20), (10, 40), (20, 60)] {
                let image = r.render_preview(e.project(), frame, 100).unwrap();
                assert_eq!(image.get_pixel(x, 50).0[3], 255);
                assert_eq!(image.get_pixel(x - 15, 50).0[3], 0);
                assert_eq!(image.get_pixel(x + 15, 50).0[3], 0);
                assert_eq!(image, r.render_preview(&restored, frame, 100).unwrap());
            }
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
                    &mut crate::rendering::FrameRenderBudget::default(),
                )
                .unwrap();
            let pixels = r.raster_canvas(&svg, 100, 100, 100).unwrap();
            for (x, alpha) in [10, 30, 45, 75].into_iter().zip(expected) {
                assert_eq!(pixels.pixel(x, 50).unwrap().alpha(), alpha, "x={x}");
            }
        }
    }
    #[test]
    fn animated_opacity_feather_and_expansion_preserve_alpha_and_soft_edges() {
        use libre_effects_core::{MaskParam, PropertyPath, TrackEdit};
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Soft mask".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 60,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Solid,
            width: 100.0,
            height: 100.0,
            name: "Solid".into(),
        })
        .unwrap();
        e.execute(Command::SetPathMasks {
            id: 1,
            masks: vec![box_mask(30.0, 70.0, PathMaskMode::Add)],
        })
        .unwrap();
        let change = |e: &mut Editor, p, edit| {
            e.execute(Command::EditTrack {
                id: 1,
                property: PropertyPath::Mask {
                    mask: 1,
                    parameter: p,
                },
                edit,
            })
            .unwrap()
        };
        let r = Renderer::new();
        change(
            &mut e,
            MaskParam::Expansion,
            TrackEdit::Value {
                frame: 0,
                value: 10.0,
            },
        );
        assert_eq!(
            r.render_preview(e.project(), 0, 100)
                .unwrap()
                .get_pixel(24, 50)
                .0[3],
            255
        );
        change(
            &mut e,
            MaskParam::Expansion,
            TrackEdit::Value {
                frame: 0,
                value: -10.0,
            },
        );
        assert_eq!(
            r.render_preview(e.project(), 0, 100)
                .unwrap()
                .get_pixel(35, 50)
                .0[3],
            0
        );
        change(
            &mut e,
            MaskParam::Expansion,
            TrackEdit::Value {
                frame: 0,
                value: 0.0,
            },
        );
        change(
            &mut e,
            MaskParam::Feather,
            TrackEdit::Value {
                frame: 0,
                value: 10.0,
            },
        );
        let pixels = r.render_preview(e.project(), 0, 100).unwrap();
        let a = pixels.get_pixel(29, 50).0[3];
        let b = pixels.get_pixel(34, 50).0[3];
        assert!(a > 0 && a < 128 && b > 128 && b < 255, "{a} {b}");
        change(
            &mut e,
            MaskParam::Opacity,
            TrackEdit::ToggleAnimation { frame: 0 },
        );
        change(
            &mut e,
            MaskParam::Opacity,
            TrackEdit::Value {
                frame: 20,
                value: 0.0,
            },
        );
        let middle = r
            .render_preview(e.project(), 10, 100)
            .unwrap()
            .get_pixel(50, 50)
            .0[3];
        assert!((i32::from(middle) - 128).abs() <= 2);
        assert_eq!(
            r.render_preview(e.project(), 20, 100)
                .unwrap()
                .get_pixel(50, 50)
                .0[3],
            0
        );
        let mut masks = e.selected_layer().unwrap().path_masks().to_vec();
        masks[0].inverted = true;
        e.execute(Command::SetPathMasks { id: 1, masks }).unwrap();
        let inverse = r.render_preview(e.project(), 0, 100).unwrap();
        assert!(inverse.get_pixel(5, 50).0[3] > 250);
        assert!(inverse.get_pixel(50, 50).0[3] < 5);
    }
}
