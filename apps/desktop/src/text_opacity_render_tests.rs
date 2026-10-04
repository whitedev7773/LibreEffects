//! Independent complete-pass opacity reference. Geometry comes from the established
//! opaque text path; attenuation and source-over arithmetic do not use production
//! opacity helpers. A 100% pass is drawn directly into the destination, preserving
//! the legacy raster stages instead of inventing an isolated intermediate.
use super::{FrameRenderBudget, Renderer};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::ImageEncoder;
use libre_effects_core::*;
use resvg::tiny_skia::{Pixmap, Transform};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
const SOURCE: &str = "AVAVA\nH H";

fn scene(paragraph: bool, fill: bool, stroke: bool, over: bool) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Text opacity oracle".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: SOURCE.into(),
            font_size: 64.,
        },
        width: 180.,
        height: 150.,
        name: "Text".into(),
    })
    .unwrap();
    e.execute(Command::SetColor {
        id: 1,
        color: 0x40c080,
    })
    .unwrap();
    e.execute(Command::SetTextStyle {
        id: 1,
        style: TextStyle {
            paragraph,
            fill_enabled: fill,
            stroke_enabled: stroke,
            stroke_color: 0xc040a0,
            stroke_width: 18.,
            stroke_over_fill: over,
            tracking: -220.,
            leading: 0.55,
            ..Default::default()
        },
    })
    .unwrap();
    for (property, value) in [
        (Property::AnchorX, 0.),
        (Property::AnchorY, 0.),
        (Property::PositionX, 20.),
        (Property::PositionY, 15.),
    ] {
        e.execute(Command::SetValue {
            id: 1,
            property,
            frame: 0,
            value,
        })
        .unwrap();
    }
    e
}

fn opacity(e: &mut Editor, parameter: TextParam, frame: u32, value: f64) {
    e.execute(Command::EditText {
        id: 1,
        parameter,
        edit: TrackEdit::Value { frame, value },
    })
    .unwrap();
}

fn raster_into(r: &Renderer, body: &str, p: &mut Pixmap) {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='{}' height='{}'>{body}</svg>",
        p.width(),
        p.height()
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &r.options).unwrap();
    resvg::render(&tree, Transform::identity(), &mut p.as_mut());
}
fn raster(r: &Renderer, body: &str) -> Pixmap {
    let mut p = Pixmap::new(WIDTH, HEIGHT).unwrap();
    raster_into(r, body, &mut p);
    p
}
fn layer_svg(r: &Renderer, project: &Project, frame: u32) -> String {
    r.isolated_layer_svg(
        project,
        project.active_composition_id(),
        1,
        frame,
        WIDTH,
        "text-opacity-oracle",
        &mut FrameRenderBudget::default(),
    )
    .unwrap()
}
fn layer_pixels(r: &Renderer, project: &Project, frame: u32) -> Pixmap {
    raster(r, &layer_svg(r, project, frame))
}

// Frozen pre-opacity pass assembly from 5ec7eb1. Only the unchanged opaque
// geometry builder is shared. There is deliberately no new opacity helper here.
fn legacy_passes(layer: &Layer, frame: u32) -> (String, String, bool) {
    let Content::Text { text, .. } = layer.content() else {
        panic!("text fixture")
    };
    let mut style = layer.text_style();
    let typography = layer.text_typography_at(frame).unwrap();
    typography.apply_to_style(&mut style);
    style.stroke_color = layer.text_color_at(TextPaint::Stroke, frame).unwrap();
    style.stroke_width = layer.text_value_at(TextParam::StrokeWidth, frame).unwrap();
    let fill = if style.fill_enabled {
        super::text_geometry_svg(
            text,
            typography.font_size,
            &format!(
                "#{:06x}",
                layer.text_color_at(TextPaint::Fill, frame).unwrap()
            ),
            layer.width(),
            layer.height(),
            &style,
        )
    } else {
        String::new()
    };
    let stroke = if style.stroke_enabled && style.stroke_width != 0. {
        let join = match style.stroke_join {
            TextStrokeJoin::Miter => "miter",
            TextStrokeJoin::Round => "round",
            TextStrokeJoin::Bevel => "bevel",
        };
        format!(
            "<g stroke='#{:06x}' stroke-width='{}' stroke-linejoin='{join}' stroke-miterlimit='4'>{}</g>",
            style.stroke_color,
            style.stroke_width,
            super::text_geometry_svg(
                text,
                typography.font_size,
                "none",
                layer.width(),
                layer.height(),
                &style
            )
        )
    } else {
        String::new()
    };
    (fill, stroke, style.stroke_over_fill)
}

fn attenuate(p: [u8; 4], opacity: f32) -> [u8; 4] {
    p.map(|c| {
        ((c as f32 * (1.0 / 255.0)) * opacity * 255.0)
            .clamp(0., 255.)
            .round_ties_even() as u8
    })
}
fn straight(p: [u8; 4]) -> [u8; 4] {
    if p[3] == 0 {
        return [0; 4];
    }
    let a = f64::from(p[3]) / 255.;
    [
        (f64::from(p[0]) / a + 0.5) as u8,
        (f64::from(p[1]) / a + 0.5) as u8,
        (f64::from(p[2]) / a + 0.5) as u8,
        p[3],
    ]
}
fn assert_pixels(actual: &Pixmap, expected: &Pixmap, context: &str) {
    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width(), expected.height())
    );
    for (i, (a, e)) in actual
        .data()
        .chunks_exact(4)
        .zip(expected.data().chunks_exact(4))
        .enumerate()
    {
        assert_eq!(
            a,
            e,
            "{context}: pixel ({}, {})",
            i as u32 % actual.width(),
            i as u32 / actual.width()
        );
    }
}

#[test]
fn text_opacity_oracle_pins_float_group_rounding_for_every_alpha() {
    let r = Renderer::new();
    let pixels = image::RgbaImage::from_fn(256, 4, |x, _| image::Rgba([255, 255, 255, x as u8]));
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(pixels.as_raw(), 256, 4, image::ExtendedColorType::Rgba8)
        .unwrap();
    let image = format!(
        "<image width='256' height='4' xlink:href='data:image/png;base64,{}'/>",
        STANDARD.encode(bytes)
    );
    let source = raster(&r, &image);
    assert_eq!(source.pixel(1, 1).unwrap().alpha(), 1);
    assert_eq!(attenuate([1; 4], 0.5), [0; 4]);
    assert_eq!(attenuate([3; 4], 0.5), [2; 4]);
    assert_eq!(attenuate([255; 4], 0.5), [128; 4]);
    for opacity in [0., 0.00125, 0.33333, 0.5, 1.] {
        let actual = raster(&r, &format!("<g opacity='{opacity}'>{image}</g>"));
        for (a, s) in actual
            .data()
            .chunks_exact(4)
            .zip(source.data().chunks_exact(4))
        {
            assert_eq!(
                a,
                attenuate(s.try_into().unwrap(), opacity),
                "opacity={opacity}"
            );
        }
    }
}

#[test]
fn text_opacity_legacy_absent_keyless_and_keyed_hundred_emit_exact_old_passes() {
    let r = Renderer::new();
    for paragraph in [false, true] {
        for over in [false, true] {
            let mut e = scene(paragraph, true, true, over);
            let original = e.project().clone();
            let (fill, stroke, _) = legacy_passes(e.selected_layer().unwrap(), 0);
            let old = if over {
                format!("{fill}{stroke}")
            } else {
                format!("{stroke}{fill}")
            };
            let expected = raster(&r, &format!("<g transform='translate(20 15)'>{old}</g>"));
            let baseline = layer_svg(&r, &original, 0);
            assert!(baseline.contains(&old));
            assert_pixels(&layer_pixels(&r, &original, 0), &expected, "legacy absent");
            for parameter in [TextParam::FillOpacity, TextParam::StrokeOpacity] {
                opacity(&mut e, parameter, 0, 99.);
                opacity(&mut e, parameter, 0, 100.);
            }
            assert_eq!(
                layer_svg(&r, e.project(), 0),
                baseline,
                "keyless100 must not add isolation"
            );
            for parameter in [TextParam::FillOpacity, TextParam::StrokeOpacity] {
                e.execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit: TrackEdit::ToggleAnimation { frame: 0 },
                })
                .unwrap();
            }
            for frame in [0, 30, 60] {
                assert_eq!(
                    layer_svg(&r, e.project(), frame),
                    baseline,
                    "animated100 must not add isolation"
                );
                assert_pixels(
                    &layer_pixels(&r, e.project(), frame),
                    &expected,
                    "legacy keyed100",
                );
            }
            assert_eq!(
                original.composition().layer(1).unwrap().text_style(),
                e.selected_layer().unwrap().text_style()
            );
        }
    }
}

#[test]
fn text_opacity_single_complete_paint_exactly_attenuates_aa_and_overlapping_glyphs() {
    let r = Renderer::new();
    for paragraph in [false, true] {
        for stroke in [false, true] {
            let mut e = scene(paragraph, !stroke, stroke, false);
            let (fill, outline, _) = legacy_passes(e.selected_layer().unwrap(), 0);
            let body = format!("<g transform='translate(20 15)'>{fill}{outline}</g>");
            let opaque = raster(&r, &body);
            assert!(
                opaque
                    .data()
                    .chunks_exact(4)
                    .any(|p| (1..255).contains(&p[3]))
            );
            assert!(opaque.data().chunks_exact(4).any(|p| p[3] == 255));
            for value in [0., 0.125, 33.333, 50., 100.] {
                opacity(
                    &mut e,
                    if stroke {
                        TextParam::StrokeOpacity
                    } else {
                        TextParam::FillOpacity
                    },
                    0,
                    value,
                );
                let mut expected = opaque.clone();
                if value != 100. {
                    for p in expected.data_mut().chunks_exact_mut(4) {
                        let source: [u8; 4] = (&*p).try_into().unwrap();
                        p.copy_from_slice(&attenuate(source, (value / 100.) as f32));
                    }
                }
                assert_pixels(
                    &layer_pixels(&r, e.project(), 0),
                    &expected,
                    &format!("paragraph={paragraph} stroke={stroke} opacity={value}"),
                );
                let rgba = image::RgbaImage::from_raw(
                    WIDTH,
                    HEIGHT,
                    expected
                        .data()
                        .chunks_exact(4)
                        .flat_map(|p| straight(p.try_into().unwrap()))
                        .collect(),
                )
                .unwrap();
                assert_eq!(r.render(e.project(), 0, WIDTH).unwrap(), rgba);
            }
        }
    }
}

// The high-precision pattern pipeline applies the floating weight and
// source-over before its one nearest-even byte store. Rounding attenuation to
// bytes first would create a fictitious extra stage at overlapping paint edges.
fn float_over(source: [u8; 4], destination: [u8; 4], opacity: f32) -> [u8; 4] {
    let s = source.map(|v| v as f32 * (1. / 255.) * opacity);
    let d = destination.map(|v| v as f32 * (1. / 255.));
    std::array::from_fn(|i| {
        ((d[i] * (1. - s[3]) + s[i]) * 255.)
            .clamp(0., 255.)
            .round_ties_even() as u8
    })
}
fn reference_pass(r: &Renderer, body: &str, opacity: f64, destination: &mut Pixmap) {
    let body = format!("<g transform='translate(20 15)'>{body}</g>");
    if opacity == 100. {
        raster_into(r, &body, destination);
    } else if opacity != 0. {
        let opaque = raster(r, &body);
        for (d, s) in destination
            .data_mut()
            .chunks_exact_mut(4)
            .zip(opaque.data().chunks_exact(4))
        {
            let result = float_over(
                s.try_into().unwrap(),
                (&*d).try_into().unwrap(),
                (opacity / 100.) as f32,
            );
            d.copy_from_slice(&result);
        }
    }
}
fn reference(
    r: &Renderer,
    layer: &Layer,
    frame: u32,
    fill_opacity: f64,
    stroke_opacity: f64,
    layer_opacity: f64,
) -> Pixmap {
    let (fill, stroke, over) = legacy_passes(layer, frame);
    let passes = if over {
        [(&fill, fill_opacity), (&stroke, stroke_opacity)]
    } else {
        [(&stroke, stroke_opacity), (&fill, fill_opacity)]
    };
    let mut result = Pixmap::new(WIDTH, HEIGHT).unwrap();
    for (body, opacity) in passes {
        reference_pass(r, body, opacity, &mut result);
    }
    if layer_opacity != 100. {
        for p in result.data_mut().chunks_exact_mut(4) {
            let attenuated = attenuate((&*p).try_into().unwrap(), (layer_opacity / 100.) as f32);
            p.copy_from_slice(&attenuated);
        }
    }
    result
}
fn exported(p: &Pixmap) -> image::RgbaImage {
    image::RgbaImage::from_raw(
        p.width(),
        p.height(),
        p.data()
            .chunks_exact(4)
            .flat_map(|p| straight(p.try_into().unwrap()))
            .collect(),
    )
    .unwrap()
}

#[test]
fn text_opacity_both_orders_preserve_direct_hundred_pass_and_separate_layer_opacity() {
    let r = Renderer::new();
    for paragraph in [false, true] {
        for over in [false, true] {
            let mut e = scene(paragraph, true, true, over);
            for (fill, stroke) in [
                (50., 25.),
                (33.333, 0.125),
                (100., 50.),
                (50., 100.),
                (0., 50.),
                (50., 0.),
                (0., 0.),
            ] {
                opacity(&mut e, TextParam::FillOpacity, 0, fill);
                opacity(&mut e, TextParam::StrokeOpacity, 0, stroke);
                for layer in [100., 50.] {
                    e.execute(Command::SetValue {
                        id: 1,
                        property: Property::Opacity,
                        frame: 0,
                        value: layer,
                    })
                    .unwrap();
                    let expected =
                        reference(&r, e.selected_layer().unwrap(), 0, fill, stroke, layer);
                    assert_pixels(
                        &layer_pixels(&r, e.project(), 0),
                        &expected,
                        &format!(
                            "paragraph={paragraph} over={over} fill={fill} stroke={stroke} layer={layer}"
                        ),
                    );
                    assert_eq!(
                        r.render(e.project(), 0, WIDTH).unwrap(),
                        exported(&expected)
                    );
                }
            }
        }
    }
}

#[test]
fn text_opacity_overlap_fixture_distinguishes_complete_pass_from_per_line_alpha() {
    let r = Renderer::new();
    let e = scene(false, true, false, false);
    let (fill, _, _) = legacy_passes(e.selected_layer().unwrap(), 0);
    let complete = reference(&r, e.selected_layer().unwrap(), 0, 50., 100., 100.);
    let per_line = raster(
        &r,
        &format!(
            "<g transform='translate(20 15)'>{}</g>",
            fill.replace("<text ", "<text opacity='0.5' ")
        ),
    );
    assert_ne!(
        complete.data(),
        per_line.data(),
        "tight-leading lines must overlap so inherited per-line alpha is observable"
    );
    assert!(
        per_line
            .data()
            .chunks_exact(4)
            .zip(complete.data().chunks_exact(4))
            .any(|(incorrect, correct)| incorrect[3] > correct[3])
    );
}

fn narrow_point(e: &mut Editor) {
    let mut style = e.selected_layer().unwrap().text_style();
    style.paragraph = true;
    e.execute(Command::SetTextStyle {
        id: 1,
        style: style.clone(),
    })
    .unwrap();
    e.execute(Command::SetTextBox {
        id: 1,
        width: 30.,
        height: 90.,
    })
    .unwrap();
    style.paragraph = false;
    e.execute(Command::SetTextStyle { id: 1, style }).unwrap();
}
fn parsed_filter_rectangles(r: &Renderer, svg: &str) -> Vec<(String, [f32; 4])> {
    let document = super::svg_document(svg, WIDTH.into(), HEIGHT.into()).unwrap();
    let tree = resvg::usvg::Tree::from_str(&document, &r.options).unwrap();
    tree.filters()
        .iter()
        .map(|filter| {
            let rect = filter.rect();
            (
                filter.id().to_owned(),
                [rect.x(), rect.y(), rect.width(), rect.height()],
            )
        })
        .collect()
}
fn add_effect(e: &mut Editor, kind: EffectKind) {
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Add(kind),
    })
    .unwrap();
}

#[test]
fn text_opacity_does_not_change_legacy_migrated_or_modern_filter_rectangles() {
    let r = Renderer::new();
    for pipeline in 0..3 {
        let mut e = scene(false, true, true, true);
        narrow_point(&mut e);
        if pipeline < 2 {
            e.execute(Command::SetEffects {
                id: 1,
                effects: Effects {
                    blur: 2.,
                    brightness: 1.4,
                    grayscale: true,
                },
            })
            .unwrap();
            if pipeline == 1 {
                e.execute(Command::Effect {
                    id: 1,
                    edit: EffectEdit::ConvertLegacy,
                })
                .unwrap();
            }
        } else {
            add_effect(&mut e, EffectKind::GaussianBlur);
            add_effect(&mut e, EffectKind::Brightness);
        }
        let baseline = parsed_filter_rectangles(&r, &layer_svg(&r, e.project(), 0));
        assert!(!baseline.is_empty());
        for (fill, stroke) in [
            (0., 100.),
            (100., 0.),
            (0., 0.),
            (0.125, 33.333),
            (50., 50.),
            (100., 100.),
        ] {
            opacity(&mut e, TextParam::FillOpacity, 0, fill);
            opacity(&mut e, TextParam::StrokeOpacity, 0, stroke);
            assert_eq!(
                parsed_filter_rectangles(&r, &layer_svg(&r, e.project(), 0)),
                baseline,
                "pipeline={pipeline} fill={fill} stroke={stroke}"
            );
        }
    }
}

#[test]
fn text_opacity_animated_width_preserves_filter_allocation_and_legacy_raster_stages() {
    let r = Renderer::new();
    for kind in [
        EffectKind::Brightness,
        EffectKind::GaussianBlur,
        EffectKind::DropShadow,
        EffectKind::Glow,
    ] {
        let mut e = scene(false, true, true, true);
        narrow_point(&mut e);
        opacity(&mut e, TextParam::StrokeWidth, 0, 0.);
        e.execute(Command::EditText {
            id: 1,
            parameter: TextParam::StrokeWidth,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        opacity(&mut e, TextParam::StrokeWidth, 60, 48.);
        add_effect(&mut e, kind);
        for frame in [0, 15, 30, 45, 60] {
            opacity(&mut e, TextParam::FillOpacity, frame, 100.);
            opacity(&mut e, TextParam::StrokeOpacity, frame, 100.);
            let opaque_svg = layer_svg(&r, e.project(), frame);
            let bounds = parsed_filter_rectangles(&r, &opaque_svg);
            let (fill_pass, stroke_pass, over) = legacy_passes(e.selected_layer().unwrap(), frame);
            let legacy = if over {
                format!("{fill_pass}{stroke_pass}")
            } else {
                format!("{stroke_pass}{fill_pass}")
            };
            assert_eq!(
                opaque_svg.matches(&legacy).count(),
                1,
                "exactly one complete frozen legacy paint body"
            );
            for (fill, stroke) in [(50., 25.), (0.125, 50.), (0., 0.)] {
                opacity(&mut e, TextParam::FillOpacity, frame, fill);
                opacity(&mut e, TextParam::StrokeOpacity, frame, stroke);
                let actual_svg = layer_svg(&r, e.project(), frame);
                assert_eq!(
                    parsed_filter_rectangles(&r, &actual_svg),
                    bounds,
                    "bounds kind={kind:?} frame={frame} fill={fill} stroke={stroke}"
                );
                // This integration reference preserves the opaque legacy filter
                // allocation and its raster stages exactly. Strict numerical
                // attenuation/order is proved separately above. Enlarging a tight
                // filter is not a pixel-equivalent operation in the pinned backend,
                // as the immutable negative control below demonstrates.
                let wrap = |body: &str, value: f64| {
                    if body.is_empty() || value == 100. {
                        body.to_owned()
                    } else {
                        format!("<g opacity='{}'>{body}</g>", value / 100.)
                    }
                };
                let fill_reference = wrap(&fill_pass, fill);
                let stroke_reference = wrap(&stroke_pass, stroke);
                let paint_reference = if over {
                    format!("{fill_reference}{stroke_reference}")
                } else {
                    format!("{stroke_reference}{fill_reference}")
                };
                let expected = raster(&r, &opaque_svg.replacen(&legacy, &paint_reference, 1));
                let layer = e.selected_layer().unwrap();
                let mut style = layer.text_style();
                style.stroke_width = layer.text_value_at(TextParam::StrokeWidth, frame).unwrap();
                let (_, retained_bounds) = super::layer_text_svg(
                    SOURCE,
                    64.,
                    "#40c080",
                    layer.width(),
                    layer.height(),
                    style,
                    [fill, stroke],
                    true,
                );
                assert_eq!(
                    retained_bounds.as_deref(),
                    Some(legacy.as_str()),
                    "unattenuated measured geometry kind={kind:?} frame={frame}"
                );
                assert_pixels(
                    &raster(&r, &actual_svg),
                    &expected,
                    &format!(
                        "legacy-stage filtered reference kind={kind:?} frame={frame} fill={fill} stroke={stroke}"
                    ),
                );
            }
        }
    }
}

fn animate_value(e: &mut Editor, parameter: TextParam, start: f64, end: f64) {
    opacity(e, parameter, 0, start);
    e.execute(Command::EditText {
        id: 1,
        parameter,
        edit: TrackEdit::ToggleAnimation { frame: 0 },
    })
    .unwrap();
    opacity(e, parameter, 60, end);
}

#[test]
fn text_opacity_manual_samples_survive_native_png_preview_and_nested_reverse_remap() {
    let r = Renderer::new();
    let mut e = scene(true, true, true, false);
    animate_value(&mut e, TextParam::FillOpacity, 100., 0.);
    animate_value(&mut e, TextParam::StrokeOpacity, 0., 100.);
    animate_value(&mut e, TextParam::StrokeWidth, 2., 26.);
    let before = e.project().clone();
    let json = before.to_json().unwrap();
    let native = crate::project_io::encode_native_project(&before, None).unwrap();
    let saved = crate::project_io::decode_project(&native).unwrap().project;
    assert_eq!(saved, before);
    assert_eq!(Project::from_json(&json).unwrap(), before);
    let frames = [0, 15, 30, 45, 60];
    let mut references = Vec::new();
    for (i, frame) in frames.into_iter().enumerate() {
        let mut manual = scene(true, true, true, false);
        opacity(
            &mut manual,
            TextParam::StrokeWidth,
            0,
            [2., 8., 14., 20., 26.][i],
        );
        let expected = exported(&reference(
            &r,
            manual.selected_layer().unwrap(),
            0,
            [100., 75., 50., 25., 0.][i],
            [0., 25., 50., 75., 100.][i],
            100.,
        ));
        assert_eq!(
            r.render(&saved, frame, WIDTH).unwrap(),
            expected,
            "manual sample {frame}"
        );
        assert_eq!(r.render_preview(&saved, frame, WIDTH).unwrap(), expected);
        assert_eq!(
            r.render_output(&saved, frame, WIDTH, HEIGHT).unwrap(),
            expected
        );
        references.push(expected);
    }
    let directory = tempfile::tempdir().unwrap();
    let mut output = crate::render_queue::Output::new(
        crate::render_queue::Format::PngAlpha,
        directory.path().join("sequence"),
    );
    output
        .spec
        .settings
        .change(crate::output_settings::Field::Fps, "2")
        .unwrap();
    let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    crate::render_queue::execute(
        &saved,
        None,
        0..61,
        &output,
        Default::default(),
        progress.clone(),
    )
    .unwrap();
    assert_eq!(progress.load(std::sync::atomic::Ordering::Relaxed), 5);
    for (i, expected) in references.iter().enumerate() {
        assert_eq!(
            image::open(output.path.join(format!("frame-{i:06}.png")))
                .unwrap()
                .to_rgba8(),
            *expected
        );
    }
    assert_eq!(e.project(), &before);
    assert_eq!(e.project().to_json().unwrap(), json);
    assert_eq!(
        crate::project_io::encode_native_project(e.project(), None).unwrap(),
        native
    );
    for name in ["Text opacity source", "Nested text opacity"] {
        e.execute(Command::Precompose {
            layers: vec![e.selected().unwrap()],
            name: name.into(),
        })
        .unwrap();
    }
    let id = e.selected().unwrap();
    e.execute(Command::SetTimeRemap { id, enabled: true })
        .unwrap();
    for (frame, value) in [(0, 2.), (60, 0.)] {
        e.execute(Command::EditTimeRemap {
            id,
            edit: TrackEdit::Value { frame, value },
        })
        .unwrap();
    }
    let nested_native = crate::project_io::encode_native_project(e.project(), None).unwrap();
    let nested = crate::project_io::decode_project(&nested_native)
        .unwrap()
        .project;
    for (i, frame) in frames.into_iter().enumerate() {
        assert_eq!(
            r.render(&nested, frame, WIDTH).unwrap(),
            references[4 - i],
            "reverse remap frame {frame}"
        );
    }
}

#[test]
fn text_opacity_hold_smooth_and_bezier_overshoot_use_independent_bounded_samples() {
    let r = Renderer::new();
    for (interpolation, samples) in [
        (Interpolation::Hold, [20., 20., 20., 20., 80.]),
        (Interpolation::Smooth, [20., 29.375, 50., 70.625, 80.]),
        (
            Interpolation::Bezier(Bezier {
                x1: 1. / 3.,
                y1: -2.,
                x2: 2. / 3.,
                y2: -2.,
            }),
            [20., 0., 0., 0., 80.],
        ),
        (
            Interpolation::Bezier(Bezier {
                x1: 1. / 3.,
                y1: 3.,
                x2: 2. / 3.,
                y2: 3.,
            }),
            [20., 100., 100., 100., 80.],
        ),
    ] {
        let mut e = scene(false, true, true, true);
        for parameter in [TextParam::FillOpacity, TextParam::StrokeOpacity] {
            animate_value(&mut e, parameter, 20., 80.);
            e.execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::Interpolate {
                    frame: 0,
                    interpolation,
                },
            })
            .unwrap();
        }
        let source = e.project().clone();
        for (frame, opacity) in [0, 15, 30, 45, 60].into_iter().zip(samples) {
            let expected = reference(
                &r,
                e.selected_layer().unwrap(),
                frame,
                opacity,
                opacity,
                100.,
            );
            assert_pixels(
                &layer_pixels(&r, e.project(), frame),
                &expected,
                &format!("interpolation={interpolation:?} frame={frame}"),
            );
        }
        assert_eq!(e.project(), &source);
    }
}

#[test]
fn text_opacity_mask_and_track_matte_apply_after_independent_paint_passes() {
    let r = Renderer::new();
    for (mode, weight) in [
        (MatteMode::Alpha, 128u32),
        (MatteMode::AlphaInverted, 127),
        (MatteMode::Luma, 92),
        (MatteMode::LumaInverted, 163),
    ] {
        let mut e = scene(false, true, true, false);
        opacity(&mut e, TextParam::FillOpacity, 0, 33.333);
        opacity(&mut e, TextParam::StrokeOpacity, 0, 50.);
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 50.,
        })
        .unwrap();
        let mut expected = reference(&r, e.selected_layer().unwrap(), 0, 33.333, 50., 50.);
        e.execute(Command::SetMask {
            id: 1,
            mask: Some(Mask {
                x: 10.,
                y: 5.,
                width: 100.,
                height: 100.,
                inverted: false,
            }),
        })
        .unwrap();
        for (i, p) in expected.data_mut().chunks_exact_mut(4).enumerate() {
            let x = i as u32 % WIDTH;
            let y = i as u32 / WIDTH;
            if !(30..130).contains(&x) || !(20..120).contains(&y) {
                p.fill(0);
            }
        }
        assert_pixels(
            &layer_pixels(&r, e.project(), 0),
            &expected,
            "integer clip keeps complete-pass alpha",
        );
        e.execute(Command::AddContent {
            content: Content::Solid,
            width: WIDTH.into(),
            height: HEIGHT.into(),
            name: "Half-opacity green matte".into(),
        })
        .unwrap();
        e.execute(Command::SetColor {
            id: 2,
            color: 0x00ff00,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 2,
            property: Property::Opacity,
            frame: 0,
            value: 50.,
        })
        .unwrap();
        e.execute(Command::SetTrackMatte {
            id: 1,
            matte: Some(TrackMatte { source: 2, mode }),
        })
        .unwrap();
        for p in expected.data_mut().chunks_exact_mut(4) {
            for c in p {
                *c = ((u32::from(*c) * weight + 127) / 255) as u8;
            }
        }
        assert_eq!(
            r.render(e.project(), 0, WIDTH).unwrap(),
            exported(&expected),
            "{mode:?}"
        );
    }
}

// Independent luma-filter byte contract: reconstruct straight f32 RGB for the
// truncated matrix, then multiply all original premultiplied channels by one
// rounded transfer-table coverage byte. This must see the attenuated text pass.
fn luma_keyed(source: [u8; 4]) -> [u8; 4] {
    if source[3] == 0 {
        return [0; 4];
    }
    let alpha = source[3] as f32 / 255.;
    let rgb =
        [source[0], source[1], source[2]].map(|c| ((c as f32 / alpha + 0.5) as u8) as f32 / 255.);
    let q = ((rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722).clamp(0., 1.) * 255.) as u8;
    let weight = (255. * (0.5 + (f64::from(q) - 120.) / 160.).clamp(0., 1.)).round() as u32;
    source.map(|c| ((u32::from(c) * weight + 127) / 255) as u8)
}

#[test]
fn text_opacity_alpha_sensitive_effect_receives_once_attenuated_complete_paints() {
    let r = Renderer::new();
    for over in [false, true] {
        let mut e = scene(false, true, true, over);
        add_effect(&mut e, EffectKind::LumaKey);
        for (parameter, value) in [
            (EffectParam::LumaThreshold, 120.),
            (EffectParam::LumaSoftness, 160.),
        ] {
            e.execute(Command::Effect {
                id: 1,
                edit: EffectEdit::SetValue {
                    effect: 1,
                    parameter,
                    frame: 0,
                    value,
                },
            })
            .unwrap();
        }
        for (fill, stroke) in [(50., 25.), (0.125, 33.333), (0., 0.)] {
            opacity(&mut e, TextParam::FillOpacity, 0, fill);
            opacity(&mut e, TextParam::StrokeOpacity, 0, stroke);
            let mut expected = reference(&r, e.selected_layer().unwrap(), 0, fill, stroke, 100.);
            for pixel in expected.data_mut().chunks_exact_mut(4) {
                let keyed = luma_keyed((&*pixel).try_into().unwrap());
                pixel.copy_from_slice(&keyed);
            }
            assert_pixels(
                &layer_pixels(&r, e.project(), 0),
                &expected,
                &format!("Luma Key over={over} fill={fill} stroke={stroke}"),
            );
        }
    }
}

#[test]
fn text_opacity_frozen_legacy_filter_edge_negative_control_is_exact() {
    // Immutable pre-change release SHA256:
    // 91d9343ce02f88809f58aa7bf83492abb3c5dfa386d68d97c951ac1ef0300602.
    // Frozen CLI inputs and all four padding controls are recorded beside
    // text-opacity/edge-diagnostic/report.json in the external QA evidence.
    // base.json SHA256: fce441f11bdaa3c2e218f88015ae3744dab98bad8de423e7ab4c6c60359ca6f3
    // oversized.json SHA256: 95267d15ab8da41bdaa86830967db3620ad9e11f56211681852bfefff2f661f8
    // Old straight export [191,64,159,32] reconstructs premultiplied [24,8,20,32]
    // by nearest-integer(c*32/255). Its right boundary is -25+192+20 =187.
    // This is inherited viewport-edge raster behavior, not opacity clipping.
    let r = Renderer::new();
    let mut e = scene(false, true, true, true);
    narrow_point(&mut e);
    opacity(&mut e, TextParam::StrokeWidth, 0, 36.);
    add_effect(&mut e, EffectKind::Brightness);
    let svg = layer_svg(&r, e.project(), 0);
    let source = raster(&r, &svg);
    assert_eq!(
        parsed_filter_rectangles(&r, &svg)[0].1,
        [-25., 0., 192., 118.]
    );
    let index = (96 * WIDTH as usize + 187) * 4;
    assert_eq!(&source.data()[index..index + 4], [24, 8, 20, 32]);
    assert_eq!(straight([24, 8, 20, 32]), [191, 64, 159, 32]);
    for padding in [1, 2, 16, 768] {
        let original = "filterUnits='userSpaceOnUse' x='-25' y='0' width='192' height='118'";
        assert!(svg.contains(original));
        let expanded = svg.replace(
            original,
            &format!(
                "filterUnits='userSpaceOnUse' x='-25' y='0' width='{}' height='118'",
                192 + padding
            ),
        );
        let expanded = raster(&r, &expanded);
        assert_eq!(&expanded.data()[index..index + 4], [0; 4]);
        let differences: Vec<_> = source
            .data()
            .chunks_exact(4)
            .zip(expanded.data().chunks_exact(4))
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            differences,
            [96 * WIDTH as usize + 187],
            "right padding{padding}"
        );
    }
}
