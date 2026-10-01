//! Separable sRGB blend functions followed by premultiplied source-over.
//! Reference: https://www.w3.org/TR/compositing-1/#blending
use crate::{adjustment_render::embedded, rendering::Renderer};
use libre_effects_core::BlendMode;

fn channel(b: f64, s: f64, mode: BlendMode) -> f64 {
    match mode {
        BlendMode::Normal => s,
        BlendMode::Multiply => b * s,
        BlendMode::Screen => b + s - b * s,
        BlendMode::Add => (b + s).min(1.0),
        BlendMode::Overlay => {
            if b <= 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
    }
}
fn byte(value: f64) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Premultiplied input/output. No color is contributed by fully clear pixels.
fn source_over(b: &[u8], s: &[u8], mode: BlendMode) -> [u8; 4] {
    if s[3] == 0 {
        return [b[0], b[1], b[2], b[3]];
    }
    if b[3] == 0 {
        return [s[0], s[1], s[2], s[3]];
    }
    let ab = f64::from(b[3]) / 255.0;
    let asrc = f64::from(s[3]) / 255.0;
    let mut output = [0; 4];
    for i in 0..3 {
        let cb = f64::from(b[i]) / f64::from(b[3]);
        let cs = f64::from(s[i]) / f64::from(s[3]);
        output[i] = byte(
            asrc * (1.0 - ab) * cs + asrc * ab * channel(cb, cs, mode) + (1.0 - asrc) * ab * cb,
        );
    }
    output[3] = byte(asrc + ab * (1.0 - asrc));
    output
}

/// An adjustment replaces a filtered lower composite; it must not add its alpha
/// back over the same lower pixels. Blend colors in place, then apply coverage.
pub(crate) fn adjusted_pixel(b: &[u8], s: &[u8], mode: BlendMode) -> [u8; 4] {
    let mut output = [s[0], s[1], s[2], s[3]];
    if mode == BlendMode::Normal || b[3] == 0 || s[3] == 0 {
        return output;
    }
    let ab = f64::from(b[3]) / 255.0;
    let asrc = f64::from(s[3]) / 255.0;
    for i in 0..3 {
        let cb = f64::from(b[i]) / f64::from(b[3]);
        let cs = f64::from(s[i]) / f64::from(s[3]);
        output[i] = byte(asrc * ((1.0 - ab) * cs + ab * channel(cb, cs, mode)));
    }
    output
}
impl Renderer {
    pub(crate) fn blend_composite(
        &self,
        lower: &str,
        source: &str,
        mode: BlendMode,
        width: u32,
        height: u32,
        max_dimension: u32,
    ) -> Result<String, String> {
        if lower.is_empty() {
            return Ok(source.into());
        }
        let mut backdrop = self.raster_canvas(lower, width, height, max_dimension)?;
        let pixels = self.raster_canvas(source, width, height, max_dimension)?;
        for (b, s) in backdrop
            .data_mut()
            .chunks_exact_mut(4)
            .zip(pixels.data().chunks_exact(4))
        {
            b.copy_from_slice(&source_over(b, s, mode));
        }
        embedded(&backdrop, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{
        Command, Content, Editor, EffectEdit, EffectKind, Mask, Project, Property,
    };
    #[test]
    fn reference_opaque_and_translucent_pixels_obey_blend_and_alpha_equations() {
        let opaque = [
            [192, 64, 128, 255],
            [48, 32, 96, 255],
            [208, 160, 224, 255],
            [255, 192, 255, 255],
            [96, 65, 192, 255],
        ];
        let partial = [
            [112, 64, 112, 192],
            [76, 56, 104, 192],
            [116, 88, 136, 192],
            [128, 96, 144, 192],
            [88, 64, 128, 192],
        ];
        for (index, mode) in BlendMode::ALL.into_iter().enumerate() {
            assert_eq!(
                source_over(&[64, 128, 192, 255], &[192, 64, 128, 255], mode),
                opaque[index],
                "{mode:?}"
            );
            assert_eq!(
                source_over(&[32, 64, 96, 128], &[96, 32, 64, 128], mode),
                partial[index],
                "{mode:?}"
            );
            assert_eq!(
                source_over(&[0, 0, 0, 0], &[96, 32, 64, 128], mode),
                [96, 32, 64, 128]
            );
            assert_eq!(
                source_over(&[32, 64, 96, 128], &[0, 0, 0, 0], mode),
                [32, 64, 96, 128]
            );
            assert_eq!(
                adjusted_pixel(&[32, 64, 96, 128], &[96, 32, 64, 128], mode)[3],
                128
            );
        }
    }
    fn scene(mode: BlendMode) -> Editor {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Blend QA".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 10,
        })
        .unwrap();
        for (id, color, x) in [(1, 0x4080c0, 40.0), (2, 0xc04080, 60.0)] {
            e.execute(Command::AddContent {
                content: Content::Solid,
                width: 60.0,
                height: 60.0,
                name: format!("Color {id}"),
            })
            .unwrap();
            e.execute(Command::SetColor { id, color }).unwrap();
            e.execute(Command::SetPosition {
                id,
                frame: 0,
                x,
                y: 50.0,
            })
            .unwrap();
            e.execute(Command::SetValue {
                id,
                property: Property::Opacity,
                frame: 0,
                value: 50.0,
            })
            .unwrap();
        }
        e.execute(Command::SetBlendMode { id: 2, mode }).unwrap();
        e
    }
    #[test]
    fn blend_render_retains_non_overlap_masks_timing_and_precomposition_pixels() {
        let r = Renderer::new();
        for mode in BlendMode::ALL {
            let mut e = scene(mode);
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            let before = r.render(&saved, 0, 100).unwrap();
            assert_eq!(before.get_pixel(20, 50).0, [64, 128, 191, 128]);
            assert_eq!(before.get_pixel(80, 50).0, [191, 64, 128, 128]);
            assert_eq!(before.get_pixel(50, 50)[3], 192);
            assert_eq!(before.get_pixel(0, 0)[3], 0);
            assert_eq!(r.render_preview(&saved, 0, 100).unwrap(), before);
            e.execute(Command::Precompose {
                layers: vec![1, 2],
                name: "Nested blend".into(),
            })
            .unwrap();
            assert_eq!(r.render(e.project(), 0, 100).unwrap(), before);
            e.undo();
            e.execute(Command::SetMask {
                id: 2,
                mask: Some(Mask {
                    x: 0.0,
                    y: 0.0,
                    width: 20.0,
                    height: 60.0,
                    inverted: false,
                }),
            })
            .unwrap();
            assert_eq!(
                r.render(e.project(), 0, 100).unwrap().get_pixel(60, 50),
                before.get_pixel(20, 50)
            );
            e.execute(Command::SetLayerRange {
                id: 2,
                start: 5,
                end: 10,
            })
            .unwrap();
            assert_eq!(
                r.render(e.project(), 0, 100).unwrap().get_pixel(40, 50),
                before.get_pixel(20, 50)
            );
        }
    }
    #[test]
    fn adjustment_blend_changes_colors_without_recompositing_existing_alpha() {
        let mut e = scene(BlendMode::Normal);
        e.execute(Command::RemoveLayer(2)).unwrap();
        e.execute(Command::AddAdjustment).unwrap();
        e.execute(Command::Effect {
            id: 3,
            edit: EffectEdit::Add(EffectKind::Grayscale),
        })
        .unwrap();
        let r = Renderer::new();
        let normal = r.render(e.project(), 0, 100).unwrap();
        for mode in [
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Add,
            BlendMode::Overlay,
        ] {
            e.execute(Command::SetBlendMode { id: 3, mode }).unwrap();
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            let image = r.render(&saved, 0, 100).unwrap();
            assert_eq!(image.get_pixel(40, 50)[3], 128);
            assert_ne!(image.get_pixel(40, 50), normal.get_pixel(40, 50));
            assert_eq!(image.get_pixel(0, 0)[3], 0);
        }
    }
    #[test]
    #[ignore = "requires FFmpeg; compares every blend mode against PNG with alpha and matte export"]
    fn blend_modes_roundtrip_through_mp4_and_alpha_mov() {
        use crate::video_export::{VideoPreset, export_video, ffmpeg_path};
        let dir = tempfile::tempdir().unwrap();
        for mode in BlendMode::ALL {
            let mut e = scene(mode);
            e.execute(Command::SetCompositionBackground(0x183048))
                .unwrap();
            let preview = Renderer::new().render_preview(e.project(), 0, 100).unwrap();
            let mut flattened = preview.clone();
            crate::rendering::composite_background(&mut flattened, 0x183048);
            for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
                let output =
                    dir.path()
                        .join(format!("blend-{}.{}", mode.label(), preset.extension()));
                export_video(
                    e.project(),
                    0..3,
                    preset,
                    &output,
                    Default::default(),
                    Default::default(),
                )
                .unwrap();
                let mut cmd = std::process::Command::new(ffmpeg_path());
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000);
                }
                let decoded = cmd
                    .args(["-v", "error", "-i"])
                    .arg(&output)
                    .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(decoded.status.success());
                assert_eq!(decoded.stdout.len(), 100 * 100 * 4 * 3);
                let reference = if preset == VideoPreset::H264 {
                    &flattened
                } else {
                    &preview
                };
                for (x, y) in [(0, 0), (20, 50), (50, 50), (80, 50)] {
                    let pixel = &decoded.stdout[(y * 100 + x) * 4..][..4];
                    let expected = reference.get_pixel(x as u32, y as u32).0;
                    assert!(
                        (i32::from(pixel[3]) - i32::from(expected[3])).abs() <= 2,
                        "{mode:?}: {pixel:?} != {expected:?}"
                    );
                    if expected[3] > 0 {
                        for c in 0..3 {
                            assert!(
                                (i32::from(pixel[c]) - i32::from(expected[c])).abs() <= 8,
                                "{mode:?}: {pixel:?} != {expected:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
