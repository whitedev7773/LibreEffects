//! Layer-space SVG filters. Each stage takes the preceding stage's complete RGBA result.
use libre_effects_core::{EffectColorSpace, EffectInstance, EffectKind, EffectParam as P, Layer};
use std::fmt::Write;

pub(crate) fn stack(
    layer: &Layer,
    frame: u32,
    prefix: &str,
    bounds: [f64; 4],
) -> Result<(String, String, String), String> {
    let [left, top, source_width, source_height] = bounds;
    let mut definitions = String::new();
    let mut filters = Vec::new();
    let (mut px, mut py) = if layer.effects().blur > 0.0 {
        (source_width, source_height)
    } else {
        (0.0, 0.0)
    };
    let mut enabled = layer
        .effect_stack()
        .iter()
        .filter(|e| !e.bypassed())
        .peekable();
    while let Some(effect) = enabled.next() {
        if effect.color_space() == EffectColorSpace::LinearRgb {
            let id = format!("{prefix}-legacy-{}", effect.id());
            let mut operations = primitives(effect, frame, bounds);
            while enabled
                .peek()
                .is_some_and(|e| e.color_space() == EffectColorSpace::LinearRgb)
            {
                operations.push_str(&primitives(enabled.next().unwrap(), frame, bounds));
            }
            // Keep migrated legacy primitives in a single linear-light filter,
            // avoiding extra color conversions and 8-bit rounding between stages.
            write!(definitions,"<filter id='{id}' x='-100%' y='-100%' width='300%' height='300%' color-interpolation-filters='linearRGB'>{operations}</filter>").unwrap();
            px = px.max(source_width);
            py = py.max(source_height);
            filters.push(id);
            continue;
        }
        let v = |p| effect.value_at(p, frame);
        match effect.kind() {
            EffectKind::GaussianBlur | EffectKind::Glow => {
                px += v(P::Radius) * 4.0;
                py += v(P::Radius) * 4.0;
            }
            EffectKind::DropShadow => {
                px += v(P::Radius) * 4.0 + v(P::OffsetX).abs();
                py += v(P::Radius) * 4.0 + v(P::OffsetY).abs();
            }
            _ => {}
        }
        let (width, height) = (source_width + 2.0 * px, source_height + 2.0 * py);
        if width * height > 33_554_432.0 {
            return Err(
                "Effect region exceeds 32 megapixels; reduce blur, shadow offsets or effect count"
                    .into(),
            );
        }
        let id = format!("{prefix}-effect-{}", effect.id());
        write!(definitions,"<filter id='{id}' filterUnits='userSpaceOnUse' x='{}' y='{}' width='{width}' height='{height}' color-interpolation-filters='sRGB'>{}</filter>",left-px,top-py,primitives(effect,frame,[left-px,top-py,width,height])).unwrap();
        filters.push(id);
    }
    // Outer filters are evaluated after inner filters.
    let open = filters
        .iter()
        .rev()
        .map(|id| format!("<g filter='url(#{id})'>"))
        .collect();
    let close = "</g>".repeat(filters.len());
    Ok((definitions, open, close))
}

fn primitives(effect: &EffectInstance, frame: u32, bounds: [f64; 4]) -> String {
    let v = |p| effect.value_at(p, frame);
    let color = || {
        format!(
            "rgb({},{},{})",
            v(P::Red).round(),
            v(P::Green).round(),
            v(P::Blue).round()
        )
    };
    match effect.kind() {
        EffectKind::Curves => {
            use libre_effects_core::{CurveChannel, sample_color_curve};
            let master = effect.curve_values(CurveChannel::Rgb, frame);
            let channels = [
                ("R", CurveChannel::Red),
                ("G", CurveChannel::Green),
                ("B", CurveChannel::Blue),
            ]
            .into_iter()
            .map(|(name, channel)| {
                let values = effect.curve_values(channel, frame);
                // This renderer operates on 8-bit unpremultiplied channels.
                // Sample each possible input exactly and round once. resvg's
                // component transfer truncates floats, so place integer outputs
                // safely inside their quantization bin (including float error).
                let table = (0..=255)
                    .map(|i| {
                        let output = sample_color_curve(
                            values,
                            sample_color_curve(master, i as f64 / 255.0),
                        );
                        (((output * 255.0).round() + 0.25) / 255.0)
                            .min(1.0)
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("<feFunc{name} type='discrete' tableValues='{table}'/>")
            })
            .collect::<String>();
            format!("<feComponentTransfer>{channels}</feComponentTransfer>")
        }
        EffectKind::LinearGradient | EffectKind::RadialGradient => gradient(effect, frame, bounds),
        EffectKind::GaussianBlur => format!("<feGaussianBlur stdDeviation='{}'/>", v(P::Radius)),
        EffectKind::Brightness => {
            let b = v(P::Amount);
            format!("<feColorMatrix values='{b} 0 0 0 0 0 {b} 0 0 0 0 0 {b} 0 0 0 0 0 1 0'/>")
        }
        EffectKind::Grayscale => "<feColorMatrix type='saturate' values='0'/>".into(),
        EffectKind::Fill => format!(
            "<feFlood flood-color='{}' flood-opacity='{}'/><feComposite in2='SourceGraphic' operator='in'/>",
            color(),
            v(P::Opacity) / 100.0
        ),
        EffectKind::Tint => {
            let amount = v(P::Amount) / 100.0;
            let mut matrix = Vec::new();
            for (row, (dark, light)) in [
                (P::DarkRed, P::Red),
                (P::DarkGreen, P::Green),
                (P::DarkBlue, P::Blue),
            ]
            .into_iter()
            .enumerate()
            {
                let black = v(dark) / 255.0;
                let white = v(light) / 255.0;
                for (column, weight) in [0.2126, 0.7152, 0.0722].into_iter().enumerate() {
                    matrix.push(
                        amount * (white - black) * weight
                            + if row == column { 1.0 - amount } else { 0.0 },
                    );
                }
                matrix.extend([0.0, black * amount]);
            }
            matrix.extend([0.0, 0.0, 0.0, 1.0, 0.0]);
            format!(
                "<feColorMatrix values='{}'/>",
                matrix
                    .iter()
                    .map(f64::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        }
        EffectKind::HueSaturation => format!(
            "<feColorMatrix type='hueRotate' values='{}'/><feColorMatrix type='saturate' values='{}'/>",
            v(P::Hue),
            v(P::Amount)
        ),
        EffectKind::Levels => {
            let (black, white, gamma) = (v(P::Black), v(P::White), v(P::Gamma));
            let curve = (0..=256)
                .map(|i| {
                    let x = i as f64 / 256.0;
                    let y = if (white - black).abs() < 1e-9 {
                        if x > black { 1.0 } else { 0.0 }
                    } else {
                        ((x - black) / (white - black)).clamp(0.0, 1.0)
                    };
                    y.powf(1.0 / gamma).to_string()
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "<feComponentTransfer><feFuncR type='table' tableValues='{curve}'/><feFuncG type='table' tableValues='{curve}'/><feFuncB type='table' tableValues='{curve}'/></feComponentTransfer>"
            )
        }
        EffectKind::DropShadow => format!(
            "<feGaussianBlur in='SourceAlpha' stdDeviation='{}'/><feOffset dx='{}' dy='{}' result='offset'/><feFlood flood-color='{}' flood-opacity='{}'/><feComposite in2='offset' operator='in'/><feMerge><feMergeNode/><feMergeNode in='SourceGraphic'/></feMerge>",
            v(P::Radius),
            v(P::OffsetX),
            v(P::OffsetY),
            color(),
            v(P::Opacity) / 100.0
        ),
        EffectKind::Glow => format!(
            "<feGaussianBlur stdDeviation='{}'/><feComposite in2='SourceGraphic' operator='arithmetic' k1='0' k2='{}' k3='1' k4='0'/>",
            v(P::Radius),
            v(P::Amount)
        ),
    }
}

fn gradient(effect: &EffectInstance, frame: u32, bounds: [f64; 4]) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let v = |p| effect.value_at(p, frame);
    let [x, y, width, height] = bounds;
    let (sx, sy, ex, ey) = (v(P::StartX), v(P::StartY), v(P::EndX), v(P::EndY));
    let start = format!(
        "rgb({},{},{})",
        v(P::DarkRed),
        v(P::DarkGreen),
        v(P::DarkBlue)
    );
    let end = format!("rgb({},{},{})", v(P::Red), v(P::Green), v(P::Blue));
    let radius = (ex - sx).hypot(ey - sy);
    let stops =
        format!("<stop offset='0' stop-color='{start}'/><stop offset='1' stop-color='{end}'/>");
    let paint = if radius < 1e-9 {
        format!("<rect x='{x}' y='{y}' width='{width}' height='{height}' fill='{end}'/>")
    } else {
        let tag = if effect.kind() == EffectKind::RadialGradient {
            format!(
                "<radialGradient id='g' gradientUnits='userSpaceOnUse' cx='{sx}' cy='{sy}' r='{radius}'>{stops}</radialGradient>"
            )
        } else {
            format!(
                "<linearGradient id='g' gradientUnits='userSpaceOnUse' x1='{sx}' y1='{sy}' x2='{ex}' y2='{ey}'>{stops}</linearGradient>"
            )
        };
        format!(
            "<defs>{tag}</defs><rect x='{x}' y='{y}' width='{width}' height='{height}' fill='url(#g)'/>"
        )
    };
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='{x} {y} {width} {height}'>{paint}</svg>"
    );
    let image = STANDARD.encode(svg.as_bytes());
    let original = v(P::BlendOriginal) / 100.0;
    format!(
        "<feImage x='{x}' y='{y}' width='{width}' height='{height}' preserveAspectRatio='none' xlink:href='data:image/svg+xml;base64,{image}'/><feComposite in2='SourceGraphic' operator='in'/><feComposite in2='SourceGraphic' operator='arithmetic' k1='0' k2='{}' k3='{original}' k4='0'/>",
        1.0 - original
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::Renderer;
    use libre_effects_core::{Command, Content, Editor, EffectEdit, Effects, Project, Property};
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Effects".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 30,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: 20.0,
            height: 20.0,
            name: "Square".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 1,
            color: 0xff0000,
        })
        .unwrap();
        e
    }
    fn edit(e: &mut Editor, edit: EffectEdit) {
        e.execute(Command::Effect { id: 1, edit }).unwrap();
    }
    fn value(e: &mut Editor, id: u64, p: P, v: f64) {
        edit(
            e,
            EffectEdit::SetValue {
                effect: id,
                parameter: p,
                frame: 0,
                value: v,
            },
        );
    }

    #[test]
    fn curves_preserve_identity_alpha_and_apply_master_before_individual_channels() {
        let mut e = scene();
        let r = Renderer::new();
        e.execute(Command::SetColor {
            id: 1,
            color: 0x4080c0,
        })
        .unwrap();
        let before = r.render(e.project(), 0, 100).unwrap();
        edit(&mut e, EffectEdit::Add(EffectKind::Curves));
        assert_eq!(r.render(e.project(), 0, 100).unwrap(), before);
        for (p, v) in libre_effects_core::CurveChannel::Rgb
            .parameters()
            .into_iter()
            .zip([255.0, 191.25, 127.5, 63.75, 0.0])
        {
            value(&mut e, 1, p, v);
        }
        let image = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(image.get_pixel(50, 50).0, [191, 127, 63, 255]);
        for p in libre_effects_core::CurveChannel::Red.parameters() {
            value(&mut e, 1, p, 0.0);
        }
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 50.0,
        })
        .unwrap();
        let image = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(image.get_pixel(50, 50).0, [0, 128, 64, 128]);
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(r.render(&saved, 0, 100).unwrap(), image);
    }
    #[test]
    fn curves_map_every_8bit_input_without_systematic_rounding_loss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ramp.png");
        let input = image::RgbaImage::from_fn(256, 16, |x, _| {
            image::Rgba([x as u8, x as u8, x as u8, 255])
        });
        input.save(&path).unwrap();
        let (content, width, height) = crate::rendering::import_image(&path).unwrap();
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Ramp".into(),
            width,
            height,
            fps: 30,
            duration: 30,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content,
            width: width as f64,
            height: height as f64,
            name: "Ramp".into(),
        })
        .unwrap();
        let r = Renderer::new();
        edit(&mut e, EffectEdit::Add(EffectKind::Curves));
        assert_eq!(r.render(e.project(), 0, 256).unwrap(), input);
        for (p, v) in libre_effects_core::CurveChannel::Rgb
            .parameters()
            .into_iter()
            .zip([255.0, 191.25, 127.5, 63.75, 0.0])
        {
            value(&mut e, 1, p, v);
        }
        let output = r.render(e.project(), 0, 256).unwrap();
        for x in 0..256 {
            let inverse = 255 - x as u8;
            assert_eq!(
                output.get_pixel(x, 8).0,
                [inverse, inverse, inverse, 255],
                "input {x}"
            );
        }
    }
    #[test]
    fn tonal_animation_order_masks_adjustment_and_precompose_share_pixels() {
        let r = Renderer::new();
        let mut e = scene();
        edit(&mut e, EffectEdit::Add(EffectKind::LinearGradient));
        edit(&mut e, EffectEdit::Add(EffectKind::Curves));
        edit(
            &mut e,
            EffectEdit::ToggleAnimation {
                effect: 2,
                parameter: P::Curve50,
                frame: 0,
            },
        );
        edit(
            &mut e,
            EffectEdit::SetValue {
                effect: 2,
                parameter: P::Curve50,
                frame: 20,
                value: 240.0,
            },
        );
        edit(
            &mut e,
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter: P::EndY,
                frame: 0,
            },
        );
        edit(
            &mut e,
            EffectEdit::SetValue {
                effect: 1,
                parameter: P::EndY,
                frame: 20,
                value: 40.0,
            },
        );
        e.execute(Command::SetMask {
            id: 1,
            mask: Some(libre_effects_core::Mask {
                x: 0.0,
                y: 0.0,
                width: 12.0,
                height: 20.0,
                inverted: false,
            }),
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 60.0,
        })
        .unwrap();
        let frames: Vec<_> = [0, 10, 20]
            .into_iter()
            .map(|f| r.render(e.project(), f, 100).unwrap())
            .collect();
        assert_ne!(frames[0], frames[1]);
        assert_ne!(frames[1], frames[2]);
        for frame in &frames {
            assert_eq!(frame.get_pixel(55, 50)[3], 0);
            assert_eq!(frame.get_pixel(45, 50)[3], 153);
        }
        edit(
            &mut e,
            EffectEdit::Move {
                effect: 2,
                index: 0,
            },
        );
        assert_ne!(r.render(e.project(), 10, 100).unwrap(), frames[1]);
        e.undo();
        e.execute(Command::Precompose {
            layers: vec![1],
            name: "Tonal source".into(),
        })
        .unwrap();
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        for (f, expected) in [0, 10, 20].into_iter().zip(frames) {
            assert_eq!(r.render_preview(&saved, f, 100).unwrap(), expected);
            assert_eq!(r.render(&saved, f, 100).unwrap(), expected);
        }
        let mut adjustment = scene();
        adjustment
            .execute(Command::SetColor {
                id: 1,
                color: 0x4080c0,
            })
            .unwrap();
        adjustment
            .execute(Command::SetValue {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                value: 50.0,
            })
            .unwrap();
        adjustment.execute(Command::AddAdjustment).unwrap();
        adjustment
            .execute(Command::Effect {
                id: 2,
                edit: EffectEdit::Add(EffectKind::Curves),
            })
            .unwrap();
        for p in libre_effects_core::CurveChannel::Red.parameters() {
            adjustment
                .execute(Command::Effect {
                    id: 2,
                    edit: EffectEdit::SetValue {
                        effect: 1,
                        parameter: p,
                        frame: 0,
                        value: 0.0,
                    },
                })
                .unwrap();
        }
        let adjusted = r.render(adjustment.project(), 0, 100).unwrap();
        assert_eq!(adjusted.get_pixel(50, 50).0, [0, 128, 191, 128]);
        assert_eq!(adjusted.get_pixel(0, 0)[3], 0);
    }
    #[test]
    #[ignore = "requires FFmpeg; checks animated curves and gradients in MP4 and alpha MOV"]
    fn tonal_effects_roundtrip_through_mp4_and_alpha_mov() {
        use crate::video_export::{VideoPreset, export_video, ffmpeg_path};
        let dir = tempfile::tempdir().unwrap();
        for kind in [EffectKind::LinearGradient, EffectKind::RadialGradient] {
            let mut e = scene();
            // Spread the gradient across 80 pixels so 4:2:0 chroma subsampling
            // is tested away from high-frequency color transitions and edges.
            for property in [Property::ScaleX, Property::ScaleY] {
                e.execute(Command::SetValue {
                    id: 1,
                    property,
                    frame: 0,
                    value: 400.0,
                })
                .unwrap();
            }
            edit(&mut e, EffectEdit::Add(kind));
            value(&mut e, 1, P::DarkBlue, 200.0);
            value(&mut e, 1, P::Red, 200.0);
            value(&mut e, 1, P::Blue, 40.0);
            edit(&mut e, EffectEdit::Add(EffectKind::Curves));
            edit(
                &mut e,
                EffectEdit::ToggleAnimation {
                    effect: 2,
                    parameter: P::Curve50,
                    frame: 0,
                },
            );
            edit(
                &mut e,
                EffectEdit::SetValue {
                    effect: 2,
                    parameter: P::Curve50,
                    frame: 2,
                    value: 220.0,
                },
            );
            e.execute(Command::SetValue {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                value: 75.0,
            })
            .unwrap();
            e.execute(Command::SetCompositionBackground(0x183048))
                .unwrap();
            let project = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
                let output =
                    dir.path()
                        .join(format!("{}-tonal.{}", kind.label(), preset.extension()));
                export_video(
                    &project,
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
                for f in 0..3 {
                    let mut reference = Renderer::new().render_preview(&project, f, 100).unwrap();
                    let png = dir.path().join("tonal.png");
                    reference.save(&png).unwrap();
                    assert_eq!(image::open(png).unwrap().to_rgba8(), reference);
                    if preset == VideoPreset::H264 {
                        crate::rendering::composite_background(&mut reference, 0x183048);
                    }
                    for (x, y) in [(0, 0), (48, 46), (50, 50), (52, 54)] {
                        let offset = ((f as usize * 100 + y) * 100 + x) * 4;
                        let pixel = &decoded.stdout[offset..offset + 4];
                        let expected = reference.get_pixel(x as u32, y as u32).0;
                        assert!(
                            (pixel[3] as i32 - expected[3] as i32).abs() <= 2,
                            "{kind:?} frame {f}: {pixel:?} != {expected:?}"
                        );
                        if expected[3] > 0 {
                            for c in 0..3 {
                                assert!(
                                    (pixel[c] as i32 - expected[c] as i32).abs() <= 10,
                                    "{kind:?} frame {f}: {pixel:?} != {expected:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn gradients_render_layer_space_endpoints_radial_distance_and_original_mix() {
        let r = Renderer::new();
        for kind in [EffectKind::LinearGradient, EffectKind::RadialGradient] {
            let mut e = scene();
            let original = r.render(e.project(), 0, 100).unwrap();
            edit(&mut e, EffectEdit::Add(kind));
            let image = r.render(e.project(), 0, 100).unwrap();
            assert_eq!(image.get_pixel(0, 0)[3], 0);
            let center = image.get_pixel(50, 50).0;
            assert_eq!(center[3], 255);
            if kind == EffectKind::LinearGradient {
                assert!((center[0] as i32 - 134).abs() <= 2, "{center:?}");
            } else {
                assert!((center[0] as i32 - 18).abs() <= 2, "{center:?}");
            }
            assert_eq!(center[0], center[1]);
            assert_eq!(center[1], center[2]);
            value(&mut e, 1, P::BlendOriginal, 100.0);
            assert_eq!(r.render(e.project(), 0, 100).unwrap(), original);
            value(&mut e, 1, P::BlendOriginal, 0.0);
            value(&mut e, 1, P::StartX, 0.0);
            value(&mut e, 1, P::StartY, 0.0);
            value(&mut e, 1, P::EndX, 0.0);
            value(&mut e, 1, P::EndY, 0.0);
            value(&mut e, 1, P::Red, 0.0);
            value(&mut e, 1, P::Blue, 0.0);
            e.execute(Command::SetValue {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                value: 50.0,
            })
            .unwrap();
            assert_eq!(
                r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0,
                [0, 255, 0, 128]
            );
            value(&mut e, 1, P::BlendOriginal, 50.0);
            let mixed = r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0;
            assert_eq!(mixed, [128, 128, 0, 128]);
            let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            assert_eq!(r.render(&saved, 0, 100).unwrap().get_pixel(50, 50).0, mixed);
        }
    }
    #[test]
    fn order_bypass_animated_fill_and_alpha_are_visible_in_saved_and_nested_output() {
        let mut e = scene();
        let r = Renderer::new();
        edit(&mut e, EffectEdit::Add(EffectKind::Fill));
        value(&mut e, 1, P::Red, 0.0);
        value(&mut e, 1, P::Blue, 0.0);
        edit(&mut e, EffectEdit::Add(EffectKind::Brightness));
        value(&mut e, 2, P::Amount, 0.5);
        let half = r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0;
        assert!(
            half[0] == 0 && (half[1] as i32 - 128).abs() <= 1 && half[2] == 0 && half[3] == 255
        );
        edit(
            &mut e,
            EffectEdit::Move {
                effect: 2,
                index: 0,
            },
        );
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0,
            [0, 255, 0, 255]
        );
        edit(
            &mut e,
            EffectEdit::Bypass {
                effect: 1,
                bypassed: true,
            },
        );
        let half = r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0;
        assert!(
            (half[0] as i32 - 128).abs() <= 1 && half[1] == 0 && half[2] == 0 && half[3] == 255
        );
        edit(
            &mut e,
            EffectEdit::Bypass {
                effect: 1,
                bypassed: false,
            },
        );
        edit(
            &mut e,
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter: P::Opacity,
                frame: 0,
            },
        );
        edit(
            &mut e,
            EffectEdit::SetValue {
                effect: 1,
                parameter: P::Opacity,
                frame: 20,
                value: 0.0,
            },
        );
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(
            r.render(&saved, 10, 100).unwrap().get_pixel(50, 50).0,
            [0, 255, 0, 128]
        );
        assert_eq!(r.render(&saved, 20, 100).unwrap().get_pixel(50, 50)[3], 0);
        e.execute(Command::Precompose {
            layers: vec![1],
            name: "Source".into(),
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 10, 100).unwrap().get_pixel(50, 50).0,
            [0, 255, 0, 128]
        );
    }
    #[test]
    fn legacy_conversion_keeps_exact_pixels_including_masks_and_wide_blurs() {
        let r = Renderer::new();
        for radius in [0.0, 2.0, 30.0, 100.0] {
            let mut e = scene();
            e.execute(Command::SetMask {
                id: 1,
                mask: Some(libre_effects_core::Mask {
                    x: 3.0,
                    y: 2.0,
                    width: 12.0,
                    height: 14.0,
                    inverted: false,
                }),
            })
            .unwrap();
            e.execute(Command::SetEffects {
                id: 1,
                effects: Effects {
                    blur: radius,
                    brightness: 1.8,
                    grayscale: true,
                },
            })
            .unwrap();
            e.execute(Command::SetValue {
                id: 1,
                property: Property::Opacity,
                frame: 0,
                value: 63.0,
            })
            .unwrap();
            let before = r.render(e.project(), 0, 100).unwrap();
            edit(&mut e, EffectEdit::ConvertLegacy);
            let after = r.render(e.project(), 0, 100).unwrap();
            assert_eq!(
                before
                    .pixels()
                    .zip(after.pixels())
                    .filter(|(a, b)| a != b)
                    .count(),
                0,
                "Changed pixels after converting radius {radius}"
            );
        }
    }
    #[test]
    fn shadow_glow_and_blur_extend_alpha_without_painting_the_entire_filter_region() {
        let r = Renderer::new();
        let mut e = scene();
        edit(&mut e, EffectEdit::Add(EffectKind::DropShadow));
        value(&mut e, 1, P::Radius, 0.0);
        value(&mut e, 1, P::OffsetX, 0.0);
        value(&mut e, 1, P::OffsetY, 20.0);
        let pixels = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(pixels.get_pixel(50, 50).0, [255, 0, 0, 255]);
        assert_eq!(pixels.get_pixel(50, 75).0, [0, 0, 0, 255]);
        assert_eq!(pixels.get_pixel(5, 5)[3], 0);
        edit(&mut e, EffectEdit::Remove(1));
        edit(&mut e, EffectEdit::Add(EffectKind::Glow));
        value(&mut e, 2, P::Radius, 4.0);
        let pixels = r.render(e.project(), 0, 100).unwrap();
        assert!(pixels.get_pixel(36, 50)[3] > 0);
        assert_eq!(pixels.get_pixel(5, 5)[3], 0);
        edit(&mut e, EffectEdit::Remove(2));
        edit(&mut e, EffectEdit::Add(EffectKind::GaussianBlur));
        value(&mut e, 3, P::Radius, 4.0);
        let pixels = r.render(e.project(), 0, 100).unwrap();
        assert!(pixels.get_pixel(36, 50)[3] > 0);
        assert!(pixels.get_pixel(40, 50)[3] < 255);
    }
    #[test]
    fn tint_hue_levels_and_grayscale_keep_alpha_and_have_defined_color_results() {
        let r = Renderer::new();
        let mut e = scene();
        edit(&mut e, EffectEdit::Add(EffectKind::Tint));
        value(&mut e, 1, P::Red, 0.0);
        value(&mut e, 1, P::Green, 0.0);
        let p = r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0;
        assert!(
            p[0] == 0 && p[1] == 0 && (p[2] as i32 - 54).abs() <= 1 && p[3] == 255,
            "{p:?}"
        );
        edit(&mut e, EffectEdit::Remove(1));
        edit(&mut e, EffectEdit::Add(EffectKind::HueSaturation));
        value(&mut e, 2, P::Amount, 0.0);
        let p = r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0;
        assert_eq!(p[0], p[1]);
        assert_eq!(p[1], p[2]);
        assert_eq!(p[3], 255);
        edit(&mut e, EffectEdit::Remove(2));
        edit(&mut e, EffectEdit::Add(EffectKind::Levels));
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0,
            [255, 0, 0, 255]
        );
        e.execute(Command::SetColor {
            id: 1,
            color: 0x808080,
        })
        .unwrap();
        value(&mut e, 3, P::Black, 0.6);
        value(&mut e, 3, P::White, 1.0);
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0,
            [0, 0, 0, 255]
        );
        edit(&mut e, EffectEdit::Remove(3));
        edit(&mut e, EffectEdit::Add(EffectKind::Grayscale));
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50).0,
            [128, 128, 128, 255]
        );
    }
    #[test]
    fn effects_do_not_crop_point_text_that_exceeds_its_nominal_layer_size() {
        let mut e = scene();
        e.execute(Command::RemoveLayer(1)).unwrap();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Wide 한글 text".into(),
                font_size: 20.0,
            },
            width: 10.0,
            height: 5.0,
            name: "Point text".into(),
        })
        .unwrap();
        e.execute(Command::SetPosition {
            id: 2,
            frame: 0,
            x: 10.0,
            y: 30.0,
        })
        .unwrap();
        let r = Renderer::new();
        let before = r.render(e.project(), 0, 100).unwrap();
        e.execute(Command::Effect {
            id: 2,
            edit: EffectEdit::Add(EffectKind::Brightness),
        })
        .unwrap();
        let after = r.render(e.project(), 0, 100).unwrap();
        assert!(before.pixels().filter(|p| p[3] > 0).count() > 200);
        assert_eq!(
            before
                .pixels()
                .zip(after.pixels())
                .filter(|(a, b)| a != b)
                .count(),
            0
        );
    }
}
