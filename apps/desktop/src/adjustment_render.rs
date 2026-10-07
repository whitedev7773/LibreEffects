//! Adjustment filters evaluate the accumulated lower composite in layer space.
//! Interpolation uses premultiplied RGBA: source-over would incorrectly increase alpha.
use crate::rendering::{
    FrameRenderBudget, Renderer, SVG_LIMIT, append_svg, frame_pixmap, paint_svg_tree, svg_document,
};
use base64::{Engine, engine::general_purpose::STANDARD};
#[cfg(test)]
use libre_effects_core::Property;
use libre_effects_core::{Affine, Effects, Layer};
use resvg::tiny_skia::Pixmap;

fn transform(matrix: Affine) -> String {
    let [a, b, c, d, x, y] = matrix.0;
    format!("matrix({a} {b} {c} {d} {x} {y})")
}

pub(crate) fn embedded(pixels: &Pixmap, width: u32, height: u32) -> Result<String, String> {
    let bytes = pixels.encode_png().map_err(|e| e.to_string())?;
    // Base64's checked expansion is known before allocating its output.
    let encoded_len = bytes
        .len()
        .checked_add(2)
        .and_then(|n| n.checked_div(3))
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= SVG_LIMIT)
        .ok_or("Adjustment frame exceeds the 64 MiB image limit")?;
    let prefix = format!(
        "<image width='{width}' height='{height}' preserveAspectRatio='none' xlink:href='data:image/png;base64,"
    );
    let total = prefix
        .len()
        .checked_add(encoded_len)
        .and_then(|n| n.checked_add(3))
        .filter(|n| *n <= SVG_LIMIT)
        .ok_or("Adjustment frame SVG exceeds the 64 MiB image limit")?;
    let mut output = String::new();
    output
        .try_reserve_exact(total)
        .map_err(|_| "Could not allocate bounded adjustment image")?;
    output.push_str(&prefix);
    STANDARD.encode_string(bytes, &mut output);
    output.push_str("'/>");
    Ok(output)
}
impl Renderer {
    pub(crate) fn raster_canvas(
        &self,
        svg: &str,
        width: u32,
        height: u32,
        max_dimension: u32,
    ) -> Result<Pixmap, String> {
        self.raster_canvas_with_domains(svg, width, height, max_dimension, &[])
    }
    pub(crate) fn raster_canvas_with_domains(
        &self,
        svg: &str,
        width: u32,
        height: u32,
        max_dimension: u32,
        domains: &[resvg::RepeatEdgeDomain],
    ) -> Result<Pixmap, String> {
        let scale = (f64::from(max_dimension) / f64::from(width.max(height))).min(1.0);
        let pw = (f64::from(width) * scale).round().max(1.0) as u32;
        let ph = (f64::from(height) * scale).round().max(1.0) as u32;
        if u64::from(pw) * u64::from(ph) > 33_554_432 {
            return Err("Adjustment rendering supports up to 32 megapixels".into());
        }
        let source = svg_document(svg, f64::from(width), f64::from(height))?;
        let tree =
            resvg::usvg::Tree::from_str(&source, &self.options).map_err(|e| e.to_string())?;
        let mut pixels = frame_pixmap(pw, ph, !domains.is_empty())?;
        paint_svg_tree(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                pw as f32 / width as f32,
                ph as f32 / height as f32,
            ),
            &mut pixels.as_mut(),
            domains,
        )?;
        Ok(pixels)
    }

    pub(crate) fn adjust_composite(
        &self,
        lower: &str,
        layer: &Layer,
        frame: u32,
        seconds_per_frame: f64,
        matrix: Affine,
        width: u32,
        height: u32,
        max_dimension: u32,
        id: &str,
        matte: Option<(&Pixmap, libre_effects_core::MatteMode)>,
        budget: &mut FrameRenderBudget,
    ) -> Result<String, String> {
        let opacity = layer
            .opacity_at(frame, seconds_per_frame)?
            .clamp(0.0, 100.0)
            / 100.0;
        if lower.is_empty()
            || opacity == 0.0
            || (layer.effects() == Effects::default()
                && layer.effect_stack().iter().all(|e| {
                    e.bypassed() || e.kind() == libre_effects_core::EffectKind::SliderControl
                }))
        {
            return Ok(lower.into());
        }
        let Some(inverse) = matrix.inverse() else {
            return Ok(lower.into());
        };
        let mut original = self.raster_canvas_with_domains(
            lower,
            width,
            height,
            max_dimension,
            &budget.repeat_domains,
        )?;
        let input = embedded(&original, width, height)?;
        let points = [
            [0.0, 0.0],
            [width as f64, 0.0],
            [0.0, height as f64],
            [width as f64, height as f64],
        ]
        .map(|p| inverse.point(p));
        let left = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let top = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let right = points
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let bottom = points
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        let stack = crate::effect_render::stack_with_domain(
            layer,
            frame,
            id,
            [left, top, right - left, bottom - top],
            crate::effect_render::InputDomain {
                rect: [0.0, 0.0, f64::from(width), f64::from(height)],
                transform: inverse,
            },
        )?;
        budget.register_repeat_domains(stack.repeat_domains)?;
        let (defs, open, close) = (stack.definitions, stack.open, stack.close);
        let forward = transform(matrix);
        let back = transform(inverse);
        let e = layer.effects();
        let mut legacy = String::new();
        if e.blur > 0.0 {
            legacy.push_str(&format!("<feGaussianBlur stdDeviation='{}'/>", e.blur));
        }
        if e.grayscale {
            legacy.push_str("<feColorMatrix type='saturate' values='0'/>");
        }
        if e.brightness != 1.0 {
            let b = e.brightness;
            legacy.push_str(&format!(
                "<feColorMatrix values='{b} 0 0 0 0 0 {b} 0 0 0 0 0 {b} 0 0 0 0 0 1 0'/>"
            ));
        }
        let legacy_group = if legacy.is_empty() {
            String::new()
        } else {
            format!("filter='url(#{id}-legacy-adjustment)'")
        };
        let mut filtered_svg = String::new();
        append_svg(
            &mut filtered_svg,
            format_args!(
                "<defs>{defs}<filter id='{id}-legacy-adjustment' x='-100%' y='-100%' width='300%' height='300%'>{legacy}</filter></defs><g transform='{forward}'>{open}<g {legacy_group}><g transform='{back}'>{input}</g></g>{close}</g>"
            ),
        )?;
        let filtered = self.raster_canvas_with_domains(
            &filtered_svg,
            width,
            height,
            max_dimension,
            &budget.repeat_domains,
        )?;
        let mut region = String::new();
        let clip = if let Some(m) = layer.mask() {
            let outer = if m.inverted {
                format!(
                    "M0 0h{}v{}h{}z ",
                    layer.width(),
                    layer.height(),
                    -layer.width()
                )
            } else {
                String::new()
            };
            region.push_str(&format!("<defs><clipPath id='{id}-area'><path clip-rule='evenodd' d='{outer}M{} {}h{}v{}h{}z'/></clipPath></defs>",m.x,m.y,m.width,m.height,-m.width));
            format!("clip-path='url(#{id}-area)'")
        } else {
            String::new()
        };
        let (path_defs, path_mask) = crate::path_mask_render::mask(layer, id, frame);
        region.push_str(&path_defs);
        region.push_str(&format!("<g transform='{forward}'><rect width='{}' height='{}' fill='white' opacity='{opacity}' {clip} {path_mask}/></g>",layer.width(),layer.height()));
        let mut coverage = self.raster_canvas(&region, width, height, max_dimension)?;
        if let Some((pixels, mode)) = matte {
            crate::matte_render::apply_matte(&mut coverage, pixels, mode);
        }
        for ((a, b), mask) in original
            .data_mut()
            .chunks_exact_mut(4)
            .zip(filtered.data().chunks_exact(4))
            .zip(coverage.data().chunks_exact(4))
        {
            let weight = u32::from(mask[3]);
            let adjusted = crate::blend_render::adjusted_pixel(a, b, layer.blend_mode());
            for channel in 0..4 {
                a[channel] = ((u32::from(a[channel]) * (255 - weight)
                    + u32::from(adjusted[channel]) * weight
                    + 127)
                    / 255) as u8;
            }
        }
        embedded(&original, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{
        Command, Content, Editor, EffectEdit, EffectKind, EffectParam, Mask, Project,
    };
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Adjustment QA".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 30,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Solid,
            width: 60.0,
            height: 60.0,
            name: "Red".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0xff0000,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 50.0,
        })
        .unwrap();
        e
    }
    #[test]
    #[ignore = "requires FFmpeg; validates adjustment composites in PNG, H.264 and ProRes alpha"]
    fn adjustment_exports_match_preview_alpha_and_composition_matte() {
        use crate::video_export::{VideoPreset, export_video, ffmpeg_path};
        let mut e = scene();
        e.execute(Command::SetCompositionBackground(0x0000ff))
            .unwrap();
        e.execute(Command::AddAdjustment).unwrap();
        fill(&mut e, 2, [0.0, 255.0, 0.0]);
        let project = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let preview = Renderer::new().render_preview(&project, 2, 100).unwrap();
        let png = dir.path().join("adjusted.png");
        preview.save(&png).unwrap();
        assert_eq!(image::open(png).unwrap().to_rgba8(), preview);
        for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
            let path = dir.path().join(format!("adjusted.{}", preset.extension()));
            export_video(
                &project,
                2..5,
                preset,
                &path,
                Default::default(),
                Default::default(),
            )
            .unwrap();
            let mut decoder = std::process::Command::new(ffmpeg_path());
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                decoder.creation_flags(0x08000000);
            }
            let decoded = decoder
                .args(["-v", "error", "-i"])
                .arg(&path)
                .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                decoded.status.success(),
                "{}",
                String::from_utf8_lossy(&decoded.stderr)
            );
            assert_eq!(decoded.stdout.len(), 100 * 100 * 4 * 3);
            let pixel = &decoded.stdout[(50 * 100 + 50) * 4..][..4];
            let corner = &decoded.stdout[..4];
            if preset == VideoPreset::H264 {
                assert!(
                    pixel[0] < 8
                        && (pixel[1] as i32 - 128).abs() < 8
                        && (pixel[2] as i32 - 127).abs() < 8,
                    "{pixel:?}"
                );
                assert_eq!(pixel[3], 255);
                assert!(
                    corner[0] < 8 && corner[1] < 8 && corner[2] > 247,
                    "{corner:?}"
                );
            } else {
                assert!(
                    pixel[0] < 8
                        && pixel[1] > 247
                        && pixel[2] < 8
                        && (pixel[3] as i32 - 128).abs() < 3,
                    "{pixel:?}"
                );
                assert_eq!(corner[3], 0);
            }
        }
    }
    fn fill(e: &mut Editor, id: u64, color: [f64; 3]) {
        e.execute(Command::Effect {
            id,
            edit: EffectEdit::Add(EffectKind::Fill),
        })
        .unwrap();
        let effect = e
            .project()
            .composition()
            .layers()
            .iter()
            .find(|l| l.id() == id)
            .unwrap()
            .effect_stack()
            .last()
            .unwrap()
            .id();
        for (parameter, value) in [EffectParam::Red, EffectParam::Green, EffectParam::Blue]
            .into_iter()
            .zip(color)
        {
            e.execute(Command::Effect {
                id,
                edit: EffectEdit::SetValue {
                    effect,
                    parameter,
                    frame: 0,
                    value,
                },
            })
            .unwrap();
        }
    }
    #[test]
    fn path_masks_and_legacy_masks_intersect_on_adjustments_without_changing_alpha() {
        use libre_effects_core::{Effects, PathMask, PathMaskMode, PathVertex, VectorPath};
        let mut e = scene();
        e.execute(Command::AddAdjustment).unwrap();
        e.execute(Command::SetEffects {
            id: 2,
            effects: Effects {
                brightness: 0.0,
                ..Default::default()
            },
        })
        .unwrap();
        e.execute(Command::SetPathMasks {
            id: 2,
            masks: vec![PathMask {
                path: VectorPath {
                    closed: true,
                    vertices: [[0.0, 0.0], [50.0, 0.0], [50.0, 100.0], [0.0, 100.0]]
                        .map(PathVertex::corner)
                        .to_vec(),
                },
                mode: PathMaskMode::Add,
                inverted: false,
                ..Default::default()
            }],
        })
        .unwrap();
        e.execute(Command::SetMask {
            id: 2,
            mask: Some(Mask {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
                inverted: false,
            }),
        })
        .unwrap();
        let pixels = Renderer::new().render_preview(e.project(), 0, 100).unwrap();
        assert_eq!(pixels.get_pixel(30, 30).0, [0, 0, 0, 128]);
        assert_eq!(pixels.get_pixel(70, 30).0, [255, 0, 0, 128]);
        assert_eq!(pixels.get_pixel(30, 70).0, [255, 0, 0, 128]);
    }
    #[test]
    fn adjustment_interpolates_premultiplied_alpha_masks_and_only_lower_layers() {
        let mut e = scene();
        let r = Renderer::new();
        let original = r.render(e.project(), 0, 100).unwrap();
        e.execute(Command::AddAdjustment).unwrap();
        assert_eq!(r.render(e.project(), 0, 100).unwrap(), original);
        fill(&mut e, 2, [0.0, 255.0, 0.0]);
        e.execute(Command::SetValue {
            id: 2,
            property: Property::Opacity,
            frame: 0,
            value: 50.0,
        })
        .unwrap();
        e.execute(Command::SetMask {
            id: 2,
            mask: Some(Mask {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 100.0,
                inverted: false,
            }),
        })
        .unwrap();
        let mixed = r.render(e.project(), 0, 100).unwrap();
        let pixel = mixed.get_pixel(30, 50).0;
        assert!(
            (pixel[0] as i32 - 128).abs() <= 3 && (pixel[1] as i32 - 128).abs() <= 3,
            "{pixel:?}"
        );
        assert_eq!(pixel[3], 128);
        assert_eq!(mixed.get_pixel(70, 50), original.get_pixel(70, 50));
        assert_eq!(mixed.get_pixel(0, 0)[3], 0);
        e.execute(Command::AddContent {
            content: Content::Solid,
            width: 20.0,
            height: 20.0,
            name: "Upper".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 3,
            color: 0x0000ff,
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0,
            [0, 0, 255, 255]
        );
        e.execute(Command::Effect {
            id: 2,
            edit: EffectEdit::Bypass {
                effect: 1,
                bypassed: true,
            },
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(30, 50),
            original.get_pixel(30, 50)
        );
        e.undo();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(
            r.render_preview(&saved, 0, 100).unwrap(),
            r.render(&saved, 0, 100).unwrap()
        );
        let before = r.render(&saved, 0, 100).unwrap();
        e.execute(Command::Precompose {
            layers: vec![1, 2, 3],
            name: "Adjusted".into(),
        })
        .unwrap();
        assert_eq!(r.render(e.project(), 0, 100).unwrap(), before);
    }
    #[test]
    fn adjustment_transforms_inverted_mask_timing_and_order_are_evaluated() {
        let mut e = scene();
        e.execute(Command::AddAdjustment).unwrap();
        fill(&mut e, 2, [0.0, 255.0, 0.0]);
        e.execute(Command::SetPosition {
            id: 2,
            frame: 0,
            x: 75.0,
            y: 50.0,
        })
        .unwrap();
        e.execute(Command::SetMask {
            id: 2,
            mask: Some(Mask {
                x: 0.0,
                y: 0.0,
                width: 25.0,
                height: 100.0,
                inverted: true,
            }),
        })
        .unwrap();
        let r = Renderer::new();
        let a = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(a.get_pixel(30, 50).0, [255, 0, 0, 128]);
        assert_eq!(a.get_pixel(60, 50).0, [0, 255, 0, 128]);
        e.execute(Command::SetLayerRange {
            id: 2,
            start: 5,
            end: 15,
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(60, 50).0,
            [255, 0, 0, 128]
        );
        assert_eq!(
            r.render(e.project(), 10, 100).unwrap().get_pixel(60, 50).0,
            [0, 255, 0, 128]
        );
        e.execute(Command::AddAdjustment).unwrap();
        fill(&mut e, 3, [0.0, 0.0, 255.0]);
        assert_eq!(
            r.render(e.project(), 10, 100).unwrap().get_pixel(60, 50).0,
            [0, 0, 255, 128]
        );
        e.execute(Command::MoveLayer { id: 3, index: 1 }).unwrap();
        assert_eq!(
            r.render(e.project(), 10, 100).unwrap().get_pixel(60, 50).0,
            [0, 255, 0, 128]
        );
    }
    #[test]
    fn adjustment_blur_expands_lower_alpha_and_animation_survives_reload() {
        let mut e = scene();
        e.execute(Command::AddAdjustment).unwrap();
        e.execute(Command::Effect {
            id: 2,
            edit: EffectEdit::Add(EffectKind::GaussianBlur),
        })
        .unwrap();
        e.execute(Command::Effect {
            id: 2,
            edit: EffectEdit::SetValue {
                effect: 1,
                parameter: EffectParam::Radius,
                frame: 0,
                value: 8.0,
            },
        })
        .unwrap();
        e.execute(Command::ToggleKeyframe {
            id: 2,
            property: Property::Opacity,
            frame: 0,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 2,
            property: Property::Opacity,
            frame: 20,
            value: 0.0,
        })
        .unwrap();
        let r = Renderer::new();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        let blurred = r.render(&saved, 0, 100).unwrap();
        assert!(blurred.get_pixel(16, 50)[3] > 0);
        let clear = r.render(&saved, 20, 100).unwrap();
        assert_eq!(clear.get_pixel(16, 50)[3], 0);
        assert_eq!(clear.get_pixel(50, 50).0, [255, 0, 0, 128]);
        assert!(
            r.render(&saved, 10, 100).unwrap().get_pixel(16, 50)[3] < blurred.get_pixel(16, 50)[3]
        );
    }
}
