//! Independent G06 reference for the pinned, staged 8-bit compositor contract.
//!
//! The oracle intentionally does not use production Luma Key SVG/LUT functions.
//! Arithmetic is derived from resvg 0.45.1 filter/{mod,color_matrix}.rs and
//! tiny-skia 0.11.4's byte SourceIn stage. The previous stage supplies actual
//! premultiplied bytes, not idealized original image colors. Export demultiply
//! (f64) is a separate final step from filter demultiply (f32).
use crate::rendering::{FrameRenderBudget, Renderer};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::ImageEncoder;
use libre_effects_core::*;
use resvg::tiny_skia::{Pixmap, Transform};

fn luma_byte(p: [u8; 4]) -> u8 {
    if p[3] == 0 {
        return 0;
    }
    let alpha = p[3] as f32 / 255.0;
    let straight = |c: u8| (c as f32 / alpha + 0.5) as u8;
    let r = straight(p[0]) as f32 / 255.0;
    let g = straight(p[1]) as f32 / 255.0;
    let b = straight(p[2]) as f32 / 255.0;
    // Keep the explicit matrix's operation order and f32 normalization. In
    // particular this is neither nearest luma nor integer-weighted source luma.
    let luma = r * 0.2126_f32 + g * 0.7152_f32 + b * 0.0722_f32;
    (luma.clamp(0.0, 1.0) * 255.0) as u8
}

fn coverage(q: u8, threshold: f64, softness: f64, mode: LumaKeyMode) -> u8 {
    let bright = if softness == 0.0 {
        if f64::from(q) >= threshold { 255 } else { 0 }
    } else {
        (255.0 * (0.5 + (f64::from(q) - threshold) / softness).clamp(0.0, 1.0)).round() as u8
    };
    match mode {
        LumaKeyMode::KeepBrighter => bright,
        LumaKeyMode::KeepDarker => 255 - bright,
    }
}

fn scaled(p: [u8; 4], weight: u8) -> [u8; 4] {
    p.map(|c| ((u32::from(c) * u32::from(weight) + 127) / 255) as u8)
}

// Layer opacity is not an 8-bit matte. resvg 0.45.1 render.rs passes the
// group opacity straight to PixmapPaint. tiny-skia 0.11.4's Pattern uses the
// highp load_8888 -> Scale1Float -> unnorm pipeline, with nearest-even storage.
// Keep its f32 normalization order: byte1 at opacity0.5 becomes0, whereas
// SourceIn with matte128 correctly makes byte1 remain1.
fn group_opacity(p: [u8; 4], opacity: f32) -> [u8; 4] {
    const BYTE_TO_UNIT: f32 = 1.0 / 255.0;
    p.map(|c| {
        let normalized = c as f32 * BYTE_TO_UNIT;
        let attenuated = normalized * opacity;
        (attenuated.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8
    })
}

fn keyed(p: [u8; 4], threshold: f64, softness: f64, mode: LumaKeyMode) -> [u8; 4] {
    scaled(p, coverage(luma_byte(p), threshold, softness, mode))
}

fn straight_export(p: [u8; 4]) -> [u8; 4] {
    if p[3] == 0 {
        return [0; 4];
    }
    let a = f64::from(p[3]) / 255.0;
    [
        (f64::from(p[0]) / a + 0.5) as u8,
        (f64::from(p[1]) / a + 0.5) as u8,
        (f64::from(p[2]) / a + 0.5) as u8,
        p[3],
    ]
}

fn assert_keyed(
    actual: &Pixmap,
    source: &Pixmap,
    threshold: f64,
    softness: f64,
    mode: LumaKeyMode,
    context: &str,
) {
    assert_eq!(actual.width(), source.width());
    assert_eq!(actual.height(), source.height());
    let width = source.width();
    for (i, (actual, source)) in actual
        .data()
        .chunks_exact(4)
        .zip(source.data().chunks_exact(4))
        .enumerate()
    {
        let source: [u8; 4] = source.try_into().unwrap();
        let expected = keyed(source, threshold, softness, mode);
        assert_eq!(
            actual,
            expected,
            "{context}: pixel ({}, {}), source {source:?}, q={}, T={threshold}, S={softness}, {mode:?}",
            i as u32 % width,
            i as u32 / width,
            luma_byte(source),
        );
        if actual[3] == 0 {
            assert_eq!(actual, [0; 4], "hidden RGB at pixel {i}: {context}");
        }
    }
}

fn scene(width: u32, height: u32, content: Content) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Luma oracle".into(),
        width,
        height,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content,
        width: f64::from(width),
        height: f64::from(height),
        name: "Source".into(),
    })
    .unwrap();
    e
}
fn edit(e: &mut Editor, layer: u64, edit: EffectEdit) {
    e.execute(Command::Effect { id: layer, edit }).unwrap();
}
fn value(e: &mut Editor, layer: u64, effect: u64, parameter: EffectParam, frame: u32, value: f64) {
    edit(
        e,
        layer,
        EffectEdit::SetValue {
            effect,
            parameter,
            frame,
            value,
        },
    );
}
fn add_key(e: &mut Editor, layer: u64, threshold: f64, softness: f64, mode: LumaKeyMode) -> u64 {
    edit(e, layer, EffectEdit::Add(EffectKind::LumaKey));
    let effect = e
        .project()
        .composition()
        .layer(layer)
        .unwrap()
        .effect_stack()
        .last()
        .unwrap()
        .id();
    value(e, layer, effect, EffectParam::LumaThreshold, 0, threshold);
    value(e, layer, effect, EffectParam::LumaSoftness, 0, softness);
    edit(e, layer, EffectEdit::SetLumaKeyMode { effect, mode });
    effect
}
fn property(e: &mut Editor, layer: u64, property: Property, value: f64) {
    e.execute(Command::SetValue {
        id: layer,
        property,
        frame: 0,
        value,
    })
    .unwrap();
}
fn raster(body: &str, width: u32, height: u32) -> Pixmap {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='{width}' height='{height}'>{body}</svg>"
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
    let mut p = Pixmap::new(width, height).unwrap();
    resvg::render(&tree, Transform::identity(), &mut p.as_mut());
    p
}
fn stack_pixels(e: &Editor, body: &str, width: u32, height: u32, bounds: [f64; 4]) -> Pixmap {
    let layer = e.project().composition().layer(1).unwrap();
    let (defs, open, close) = super::stack(layer, 0, "oracle", bounds).unwrap();
    raster(
        &format!("<defs>{defs}</defs>{open}{body}{close}"),
        width,
        height,
    )
}
fn layer_pixels(r: &Renderer, project: &Project, frame: u32, layer: u64) -> Pixmap {
    let comp = project.composition();
    let svg = r
        .isolated_layer_svg(
            project,
            project.active_composition_id(),
            layer,
            frame,
            comp.width().max(comp.height()),
            "luma-oracle",
            &mut FrameRenderBudget::default(),
        )
        .unwrap();
    r.raster_canvas(
        &svg,
        comp.width(),
        comp.height(),
        comp.width().max(comp.height()),
    )
    .unwrap()
}
fn encoded(pixels: &image::RgbaImage) -> String {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            pixels.as_raw(),
            pixels.width(),
            pixels.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    STANDARD.encode(bytes)
}
fn reference_image(
    source: &Pixmap,
    threshold: f64,
    softness: f64,
    mode: LumaKeyMode,
) -> image::RgbaImage {
    image::RgbaImage::from_raw(
        source.width(),
        source.height(),
        source
            .data()
            .chunks_exact(4)
            .flat_map(|p| straight_export(keyed(p.try_into().unwrap(), threshold, softness, mode)))
            .collect(),
    )
    .unwrap()
}

#[test]
fn luma_oracle_anchors_pin_truncation_centered_width_and_single_alpha_multiplication() {
    use LumaKeyMode::{KeepBrighter as Bright, KeepDarker as Dark};
    assert_eq!(luma_byte([128, 0, 0, 128]), 54);
    assert_eq!(luma_byte([0, 64, 0, 64]), 182);
    assert_eq!(luma_byte([0, 0, 255, 255]), 18);
    assert_eq!(keyed([128, 0, 0, 128], 54.0, 100.0, Bright), [64, 0, 0, 64]);
    assert_eq!(keyed([0, 64, 0, 64], 182.0, 100.0, Bright), [0, 32, 0, 32]);
    assert_eq!(coverage(54, 54.0, 0.0, Bright), 255);
    assert_eq!(coverage(54, 54.000001, 0.0, Bright), 0);
    assert_eq!(coverage(0, 0.0, 255.0, Bright), 128);
    assert_eq!(coverage(255, 255.0, 255.0, Bright), 128);
    assert_eq!(coverage(0, 0.0, 255.0, Dark), 127);
    assert_eq!(coverage(255, 255.0, 255.0, Dark), 127);
    for q in 0..=255 {
        for (t, s) in [(0.0, 255.0), (127.5, 0.0), (182.0, 100.0), (255.0, 255.0)] {
            assert_eq!(
                u16::from(coverage(q, t, s, Bright)) + u16::from(coverage(q, t, s, Dark)),
                255
            );
        }
    }
    for (body, source, threshold, expected) in [
        (
            "<rect width='4' height='4' fill='red' fill-opacity='0.5019607843137255'/>",
            [128, 0, 0, 128],
            54.0,
            [64, 0, 0, 64],
        ),
        (
            "<rect width='4' height='4' fill='#00ff00' fill-opacity='0.25098039215686274'/>",
            [0, 64, 0, 64],
            182.0,
            [0, 32, 0, 32],
        ),
    ] {
        let mut e = scene(4, 4, Content::Solid);
        let baseline = raster(body, 4, 4);
        assert_eq!(&baseline.data()[..4], source);
        add_key(&mut e, 1, threshold, 100.0, Bright);
        let actual = stack_pixels(&e, body, 4, 4, [0.0, 0.0, 4.0, 4.0]);
        assert_eq!(&actual.data()[..4], expected);
        assert_keyed(
            &actual,
            &baseline,
            threshold,
            100.0,
            Bright,
            "anti-alpha-square anchor",
        );
    }
}

#[test]
fn luma_every_opaque_gray_matches_the_staged_byte_oracle_in_both_modes() {
    let body = (0..=255)
        .map(|v| format!("<rect x='{v}' width='1' height='8' fill='rgb({v},{v},{v})'/>"))
        .collect::<String>();
    let original = raster(&body, 256, 8);
    for q in 0..=255u32 {
        assert_eq!(original.pixel(q, 4).unwrap().alpha(), 255);
    }
    let cases = [
        (0.0, 0.0),
        (0.0, 255.0),
        (0.25, 0.0),
        (1.0, 0.000000001),
        (54.0, 0.0),
        (54.25, 0.5),
        (127.5, 0.0),
        (128.0, 0.0),
        (128.0, 1.0),
        (128.0, 100.0),
        (128.25, 2.5),
        (182.0, 255.0),
        (254.75, 0.5),
        (255.0, 0.0),
        (255.0, 255.0),
    ];
    for mode in [LumaKeyMode::KeepBrighter, LumaKeyMode::KeepDarker] {
        for (t, s) in cases {
            let mut e = scene(256, 8, Content::Solid);
            add_key(&mut e, 1, t, s, mode);
            let actual = stack_pixels(&e, &body, 256, 8, [0.0, 0.0, 256.0, 8.0]);
            assert_keyed(&actual, &original, t, s, mode, "all 256 opaque grays");
        }
    }
}

#[test]
fn luma_asymmetric_colors_and_all_selected_alphas_use_reconstructed_stage_colors() {
    let colors = [
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [17, 93, 241],
        [219, 41, 7],
        [37, 193, 111],
        [1, 128, 254],
        [63, 127, 191],
        [254, 1, 128],
        [123, 45, 67],
    ];
    let alphas = [0, 1, 64, 128, 254, 255];
    let mut body = String::new();
    for (x, [r, g, b]) in colors.into_iter().enumerate() {
        for (y, a) in alphas.into_iter().enumerate() {
            body.push_str(&format!("<rect x='{}' y='{}' width='4' height='4' fill='rgb({r},{g},{b})' fill-opacity='{}'/>", x * 4, y * 4, f64::from(a) / 255.0));
        }
    }
    let original = raster(&body, 48, 24);
    let cases = [
        (0.0, 0.0),
        (0.0, 255.0),
        (18.0, 0.0),
        (54.0, 100.0),
        (54.00001, 0.000001),
        (91.5, 63.25),
        (128.0, 255.0),
        (182.0, 100.0),
        (182.25, 0.0),
        (254.5, 1.0),
        (255.0, 255.0),
    ];
    for mode in [LumaKeyMode::KeepBrighter, LumaKeyMode::KeepDarker] {
        for (t, s) in cases {
            let mut e = scene(48, 24, Content::Solid);
            add_key(&mut e, 1, t, s, mode);
            let actual = stack_pixels(&e, &body, 48, 24, [0.0, 0.0, 48.0, 24.0]);
            assert_keyed(
                &actual,
                &original,
                t,
                s,
                mode,
                "asymmetric colors and alpha edge cases",
            );
        }
    }
}

#[test]
fn luma_identity_and_bypass_preserve_exact_svg_pixels_and_legacy_grouping() {
    let body = "<rect width='16' height='16' fill='#37c16f' fill-opacity='0.25098039215686274'/>";
    let mut e = scene(16, 16, Content::Solid);
    let before = super::stack(
        e.project().composition().layer(1).unwrap(),
        0,
        "exact",
        [0., 0., 16., 16.],
    )
    .unwrap();
    let identity = add_key(&mut e, 1, 0.0, 0.0, LumaKeyMode::KeepBrighter);
    assert_eq!(
        super::stack(
            e.project().composition().layer(1).unwrap(),
            0,
            "exact",
            [0., 0., 16., 16.]
        )
        .unwrap(),
        before
    );
    assert_eq!(
        stack_pixels(&e, body, 16, 16, [0., 0., 16., 16.]),
        raster(body, 16, 16)
    );
    value(&mut e, 1, identity, EffectParam::LumaThreshold, 0, 150.0);
    edit(
        &mut e,
        1,
        EffectEdit::Bypass {
            effect: identity,
            bypassed: true,
        },
    );
    assert_eq!(
        super::stack(
            e.project().composition().layer(1).unwrap(),
            0,
            "exact",
            [0., 0., 16., 16.]
        )
        .unwrap(),
        before
    );
    assert_eq!(
        stack_pixels(&e, body, 16, 16, [0., 0., 16., 16.]),
        raster(body, 16, 16)
    );

    let mut legacy = scene(16, 16, Content::Solid);
    legacy
        .execute(Command::SetEffects {
            id: 1,
            effects: Effects {
                blur: 1.5,
                brightness: 0.7,
                grayscale: true,
            },
        })
        .unwrap();
    edit(&mut legacy, 1, EffectEdit::ConvertLegacy);
    let before = super::stack(
        legacy.project().composition().layer(1).unwrap(),
        0,
        "exact",
        [0., 0., 16., 16.],
    )
    .unwrap();
    assert!(before.0.contains("linearRGB"));
    let pixels = stack_pixels(&legacy, body, 16, 16, [0., 0., 16., 16.]);
    let key = add_key(&mut legacy, 1, 0.0, 0.0, LumaKeyMode::KeepBrighter);
    edit(
        &mut legacy,
        1,
        EffectEdit::Move {
            effect: key,
            index: 1,
        },
    );
    assert_eq!(
        super::stack(
            legacy.project().composition().layer(1).unwrap(),
            0,
            "exact",
            [0., 0., 16., 16.]
        )
        .unwrap(),
        before
    );
    assert_eq!(
        stack_pixels(&legacy, body, 16, 16, [0., 0., 16., 16.]),
        pixels
    );
}

#[test]
fn luma_order_and_expanded_blur_shadow_regions_follow_the_preceding_stage() {
    let body =
        "<rect x='24' y='24' width='16' height='16' fill='red' fill-opacity='0.5019607843137255'/>";
    let bounds = [24., 24., 16., 16.];
    for kind in [EffectKind::GaussianBlur, EffectKind::DropShadow] {
        let mut e = scene(64, 64, Content::Solid);
        edit(&mut e, 1, EffectEdit::Add(kind));
        value(&mut e, 1, 1, EffectParam::Radius, 0, 2.0);
        if kind == EffectKind::DropShadow {
            for (p, v) in [
                (EffectParam::OffsetX, 7.),
                (EffectParam::OffsetY, 3.),
                (EffectParam::Red, 0.),
                (EffectParam::Green, 255.),
                (EffectParam::Blue, 0.),
                (EffectParam::Opacity, 100.),
            ] {
                value(&mut e, 1, 1, p, 0, v);
            }
        }
        let original = stack_pixels(&e, body, 64, 64, bounds);
        assert!((0..64).any(|y| {
            (0..64).any(|x| !(24..40).contains(&x) && original.pixel(x, y).unwrap().alpha() > 0)
        }));
        add_key(&mut e, 1, 54.0, 255.0, LumaKeyMode::KeepBrighter);
        let actual = stack_pixels(&e, body, 64, 64, bounds);
        assert_keyed(
            &actual,
            &original,
            54.0,
            255.0,
            LumaKeyMode::KeepBrighter,
            "expanded preceding filter",
        );
        assert!((0..64).any(|y| {
            (0..64).any(|x| !(24..40).contains(&x) && actual.pixel(x, y).unwrap().alpha() > 0)
        }));
    }
    let mut e = scene(64, 64, Content::Solid);
    edit(&mut e, 1, EffectEdit::Add(EffectKind::Fill));
    for (p, v) in [
        (EffectParam::Red, 0.),
        (EffectParam::Green, 255.),
        (EffectParam::Blue, 0.),
    ] {
        value(&mut e, 1, 1, p, 0, v);
    }
    let key = add_key(&mut e, 1, 128.0, 0.0, LumaKeyMode::KeepBrighter);
    assert_eq!(
        stack_pixels(&e, body, 64, 64, bounds)
            .pixel(32, 32)
            .unwrap()
            .alpha(),
        128
    );
    edit(
        &mut e,
        1,
        EffectEdit::Move {
            effect: key,
            index: 0,
        },
    );
    assert!(
        stack_pixels(&e, body, 64, 64, bounds)
            .data()
            .iter()
            .all(|c| *c == 0)
    );
}

#[test]
fn luma_masks_feather_and_overhanging_text_are_keyed_before_layer_opacity() {
    let r = Renderer::new();
    let mut masked = scene(128, 96, Content::Solid);
    masked
        .execute(Command::SetColor {
            id: 1,
            color: 0x5da9e7,
        })
        .unwrap();
    masked
        .execute(Command::SetMask {
            id: 1,
            mask: Some(Mask {
                x: 8.,
                y: 4.,
                width: 104.,
                height: 88.,
                inverted: false,
            }),
        })
        .unwrap();
    masked
        .execute(Command::SetPathMasks {
            id: 1,
            masks: vec![PathMask {
                path: VectorPath {
                    closed: true,
                    vertices: [[24., 12.], [100., 12.], [100., 84.], [24., 84.]]
                        .map(PathVertex::corner)
                        .to_vec(),
                },
                ..Default::default()
            }],
        })
        .unwrap();
    for (parameter, value) in [(MaskParam::Feather, 9.), (MaskParam::Opacity, 67.)] {
        masked
            .execute(Command::EditTrack {
                id: 1,
                property: PropertyPath::Mask { mask: 1, parameter },
                edit: TrackEdit::Value { frame: 0, value },
            })
            .unwrap();
    }
    let mut text = Editor::default();
    text.execute(Command::ConfigureComposition {
        name: "Luma text bounds".into(),
        width: 256,
        height: 96,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    text.execute(Command::AddContent {
        content: Content::Text {
            text: "WIDE WIDE".into(),
            font_size: 36.,
        },
        width: 48.,
        height: 64.,
        name: "Overhanging text".into(),
    })
    .unwrap();
    text.execute(Command::SetColor {
        id: 1,
        color: 0x5da9e7,
    })
    .unwrap();
    text.execute(Command::SetTextStyle {
        id: 1,
        style: TextStyle {
            stroke_enabled: true,
            stroke_color: 0xf0b023,
            stroke_width: 2.,
            ..Default::default()
        },
    })
    .unwrap();
    for (p, v) in [
        (Property::AnchorX, 0.),
        (Property::AnchorY, 0.),
        (Property::PositionX, 8.),
        (Property::PositionY, 12.),
    ] {
        property(&mut text, 1, p, v);
    }
    let text_original = layer_pixels(&r, text.project(), 0, 1);
    assert!(
        (56..256).any(|x| (0..96).any(|y| text_original.pixel(x, y).unwrap().alpha() > 0)),
        "fixture must extend beyond nominal text bounds"
    );
    for (name, mut e) in [("feathered mask", masked), ("overhanging text", text)] {
        let source = layer_pixels(&r, e.project(), 0, 1);
        assert!(
            source
                .pixels()
                .iter()
                .any(|p| p.alpha() > 0 && p.alpha() < 255)
        );
        add_key(&mut e, 1, 125.0, 128.0, LumaKeyMode::KeepBrighter);
        let keyed_pixels = layer_pixels(&r, e.project(), 0, 1);
        assert_keyed(
            &keyed_pixels,
            &source,
            125.0,
            128.0,
            LumaKeyMode::KeepBrighter,
            name,
        );
        property(&mut e, 1, Property::Opacity, 50.0);
        let downstream = layer_pixels(&r, e.project(), 0, 1);
        for (actual, original) in downstream
            .data()
            .chunks_exact(4)
            .zip(keyed_pixels.data().chunks_exact(4))
        {
            assert_eq!(
                actual,
                group_opacity(original.try_into().unwrap(), 0.5),
                "downstream opacity: {name}"
            );
        }
        let saved = crate::project_io::decode_project(
            &crate::project_io::encode_native_project(e.project(), None).unwrap(),
        )
        .unwrap()
        .project;
        let expected = r
            .render(e.project(), 0, e.project().composition().width())
            .unwrap();
        assert_eq!(
            r.render_preview(&saved, 0, saved.composition().width())
                .unwrap(),
            expected
        );
    }
}

#[test]
fn luma_imported_alpha_interpretation_precedes_the_key_and_ignores_hidden_rgb() {
    let pixels = image::RgbaImage::from_fn(64, 32, |x, _| {
        image::Rgba(match x / 16 {
            0 => [128, 0, 0, 128],
            1 => [0, 64, 0, 64],
            2 => [17, 93, 241, 0],
            _ => [45, 80, 120, 128],
        })
    });
    let r = Renderer::new();
    for alpha in [
        AlphaInterpretation::Straight,
        AlphaInterpretation::Premultiplied { matte: 0 },
        AlphaInterpretation::Ignore,
    ] {
        let mut e = Editor::default();
        e.execute(Command::ImportAsset {
            content: Content::Image {
                png: encoded(&pixels).into(),
            },
            width: 64.,
            height: 32.,
            name: "Interpret before key".into(),
            folder: None,
            frame: None,
        })
        .unwrap();
        e.execute(Command::InterpretAsset {
            asset: 1,
            interpretation: FootageInterpretation {
                alpha,
                invert_alpha: false,
                fps: None,
            },
        })
        .unwrap();
        e.execute(Command::CompositionFromAsset(1)).unwrap();
        let source = layer_pixels(&r, e.project(), 0, 1);
        if alpha != AlphaInterpretation::Ignore {
            assert_eq!(source.pixel(40, 16).unwrap().alpha(), 0);
        }
        add_key(&mut e, 1, 54., 100., LumaKeyMode::KeepBrighter);
        let output = layer_pixels(&r, e.project(), 0, 1);
        assert_keyed(
            &output,
            &source,
            54.,
            100.,
            LumaKeyMode::KeepBrighter,
            "interpreted imported PNG",
        );
        assert_eq!(
            r.render(e.project(), 0, 64).unwrap(),
            reference_image(&source, 54., 100., LumaKeyMode::KeepBrighter)
        );
    }
}

fn animated_scene(threshold: f64, softness: f64) -> Editor {
    let pixels = image::RgbaImage::from_fn(64, 32, |x, y| {
        let gray = (x * 4 + y % 4) as u8;
        image::Rgba([gray, gray, gray, if y < 16 { 255 } else { 128 }])
    });
    let mut e = scene(
        64,
        32,
        Content::Image {
            png: encoded(&pixels).into(),
        },
    );
    add_key(&mut e, 1, threshold, softness, LumaKeyMode::KeepDarker);
    e
}
fn animate_key(e: &mut Editor) {
    for (parameter, end) in [
        (EffectParam::LumaThreshold, 224.),
        (EffectParam::LumaSoftness, 80.),
    ] {
        edit(
            e,
            1,
            EffectEdit::ToggleAnimation {
                effect: 1,
                parameter,
                frame: 0,
            },
        );
        value(e, 1, 1, parameter, 60, end);
    }
}

#[test]
fn luma_animation_static_oracles_native_png_preview_and_nested_remap_agree() {
    let r = Renderer::new();
    let mut e = animated_scene(32., 16.);
    animate_key(&mut e);
    let json = e.project().to_json().unwrap();
    let native = crate::project_io::encode_native_project(e.project(), None).unwrap();
    let saved = crate::project_io::decode_project(&native).unwrap().project;
    let dir = tempfile::tempdir().unwrap();
    let mut references = Vec::new();
    for (frame, threshold, softness) in [
        (0, 32., 16.),
        (15, 80., 32.),
        (30, 128., 48.),
        (45, 176., 64.),
        (60, 224., 80.),
    ] {
        let mut reference = animated_scene(threshold, softness);
        let expected = r.render(reference.project(), frame, 64).unwrap();
        edit(
            &mut reference,
            1,
            EffectEdit::Bypass {
                effect: 1,
                bypassed: true,
            },
        );
        let source = layer_pixels(&r, reference.project(), frame, 1);
        assert_eq!(
            expected,
            reference_image(&source, threshold, softness, LumaKeyMode::KeepDarker),
            "independent animated frame {frame}"
        );
        assert_eq!(
            r.render(&saved, frame, 64).unwrap(),
            expected,
            "manual static frame {frame}"
        );
        assert_eq!(r.render_preview(&saved, frame, 64).unwrap(), expected);
        assert_eq!(r.render_output(&saved, frame, 64, 32).unwrap(), expected);
        let path = dir.path().join(format!("luma-{frame}.png"));
        expected.save(&path).unwrap();
        assert_eq!(image::open(path).unwrap().to_rgba8(), expected);
        references.push(expected);
    }
    assert_ne!(references[0], references[4]);
    assert_eq!(
        e.project().to_json().unwrap(),
        json,
        "rendering must not normalize the source"
    );
    assert_eq!(
        crate::project_io::encode_native_project(e.project(), None).unwrap(),
        native
    );
    for name in ["Luma source", "Luma nested source"] {
        let id = e.selected().unwrap();
        e.execute(Command::Precompose {
            layers: vec![id],
            name: name.into(),
        })
        .unwrap();
    }
    let id = e.selected().unwrap();
    e.execute(Command::SetTimeRemap { id, enabled: true })
        .unwrap();
    for (frame, seconds) in [(0, 2.), (60, 0.)] {
        e.execute(Command::EditTimeRemap {
            id,
            edit: TrackEdit::Value {
                frame,
                value: seconds,
            },
        })
        .unwrap();
    }
    let nested = Project::from_json(&e.project().to_json().unwrap()).unwrap();
    for (i, frame) in [0, 15, 30, 45, 60].into_iter().enumerate() {
        assert_eq!(
            r.render(&nested, frame, 64).unwrap(),
            references[4 - i],
            "nested reverse remap frame {frame}"
        );
        assert_eq!(
            r.render_preview(&nested, frame, 64).unwrap(),
            references[4 - i]
        );
    }
}

#[test]
fn luma_track_matte_multiplies_the_already_keyed_and_opacity_scaled_source() {
    let r = Renderer::new();
    for (mode, matte_color, matte_weight) in [
        (MatteMode::Alpha, 0xffffff, 128u8),
        (MatteMode::Luma, 0x00ff00, 92u8),
    ] {
        let mut e = scene(64, 64, Content::Solid);
        e.execute(Command::SetColor {
            id: 1,
            color: 0xff0000,
        })
        .unwrap();
        add_key(&mut e, 1, 54., 100., LumaKeyMode::KeepBrighter);
        property(&mut e, 1, Property::Opacity, 50.);
        e.execute(Command::AddContent {
            content: Content::Solid,
            width: 32.,
            height: 64.,
            name: "Downstream matte".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 2,
            color: matte_color,
        })
        .unwrap();
        property(&mut e, 2, Property::Opacity, 50.);
        e.execute(Command::SetTrackMatte {
            id: 1,
            matte: Some(TrackMatte { source: 2, mode }),
        })
        .unwrap();
        let output = r.render(e.project(), 0, 64).unwrap();
        let expected = straight_export(scaled(
            scaled(
                keyed([255, 0, 0, 255], 54., 100., LumaKeyMode::KeepBrighter),
                128,
            ),
            matte_weight,
        ));
        assert_eq!(expected[3], if mode == MatteMode::Alpha { 32 } else { 23 });
        for y in 0..64 {
            for x in 0..64 {
                assert_eq!(
                    output.get_pixel(x, y).0,
                    if (16..48).contains(&x) {
                        expected
                    } else {
                        [0; 4]
                    },
                    "{mode:?} at ({x},{y})"
                );
            }
        }
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(r.render_preview(&saved, 0, 64).unwrap(), output);
    }
}

#[test]
fn luma_adjustment_keys_accumulated_alpha_then_interpolates_its_masked_region() {
    let r = Renderer::new();
    let mut e = scene(64, 64, Content::Solid);
    e.execute(Command::SetColor {
        id: 1,
        color: 0xff0000,
    })
    .unwrap();
    property(&mut e, 1, Property::Opacity, 50.);
    e.execute(Command::AddAdjustment).unwrap();
    add_key(&mut e, 2, 54., 100., LumaKeyMode::KeepBrighter);
    let full = r.render(e.project(), 0, 64).unwrap();
    assert!(
        full.pixels().all(|p| p.0 == [255, 0, 0, 64]),
        "full adjustment must not square source alpha"
    );
    e.execute(Command::SetMask {
        id: 2,
        mask: Some(Mask {
            x: 0.,
            y: 0.,
            width: 32.,
            height: 64.,
            inverted: false,
        }),
    })
    .unwrap();
    property(&mut e, 2, Property::Opacity, 50.);
    let partial = r.render(e.project(), 0, 64).unwrap();
    // Original alpha128, filtered64, region opacity128/255. Interpolate the
    // existing lower composite rather than drawing the keyed result source-over.
    let mixed = ((128u32 * 127 + 64 * 128 + 127) / 255) as u8;
    assert_eq!(mixed, 96);
    for y in 0..64 {
        for x in 0..64 {
            assert_eq!(
                partial.get_pixel(x, y).0,
                [255, 0, 0, if x < 32 { mixed } else { 128 }]
            );
        }
    }
    let before = e.project().to_json().unwrap();
    let saved = Project::from_json(&before).unwrap();
    assert_eq!(r.render_preview(&saved, 0, 64).unwrap(), partial);
    assert_eq!(e.project().to_json().unwrap(), before);
}

#[test]
fn luma_reference_pins_existing_no_key_f32_group_opacity_for_every_alpha() {
    // This separately pins the pre-existing downstream stage. It must not be
    // confused with Luma Key's exact integer-byte SourceIn matte arithmetic.
    let pixels = image::RgbaImage::from_fn(256, 4, |x, _| image::Rgba([255, 255, 255, x as u8]));
    let mut e = scene(
        256,
        4,
        Content::Image {
            png: encoded(&pixels).into(),
        },
    );
    let r = Renderer::new();
    assert!(
        e.project()
            .composition()
            .layer(1)
            .unwrap()
            .effect_stack()
            .is_empty()
    );
    let source = layer_pixels(&r, e.project(), 0, 1);
    assert_eq!(source.pixel(1, 1).unwrap().alpha(), 1);
    property(&mut e, 1, Property::Opacity, 50.0);
    let output = layer_pixels(&r, e.project(), 0, 1);
    assert_eq!(output.pixel(1, 1).unwrap().alpha(), 0);
    assert_eq!(group_opacity([1; 4], 0.5), [0; 4]);
    assert_eq!(scaled([1; 4], 128), [1; 4]);
    for (actual, original) in output
        .data()
        .chunks_exact(4)
        .zip(source.data().chunks_exact(4))
    {
        assert_eq!(
            actual,
            group_opacity(original.try_into().unwrap(), 0.5),
            "existing opacity without any Luma Key"
        );
    }
}
