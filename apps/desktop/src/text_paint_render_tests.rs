//! Animated paint is compared against separately constructed static documents.
use libre_effects_core::*;

const FRAMES: [u32; 5] = [0, 15, 30, 45, 60];
const FILLS: [u32; 5] = [0x204060, 0x506070, 0x808080, 0xb0a090, 0xe0c0a0];
const STROKES: [u32; 5] = [0xf02040, 0xc03468, 0x904890, 0x605cb8, 0x3070e0];
const WIDTHS: [f64; 5] = [0., 12., 24., 36., 48.];

fn scene(
    paragraph: bool,
    fill: bool,
    stroke: bool,
    over: bool,
    rgb: u32,
    outline: u32,
    width: f64,
) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Text paint reference".into(),
        width: 480,
        height: 320,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: "AVAVA\nH H".into(),
            font_size: 72.,
        },
        width: 180.,
        height: 160.,
        name: "Text".into(),
    })
    .unwrap();
    e.execute(Command::SetColor { id: 1, color: rgb }).unwrap();
    e.execute(Command::SetTextStyle {
        id: 1,
        style: TextStyle {
            paragraph,
            fill_enabled: fill,
            stroke_enabled: stroke,
            stroke_color: outline,
            stroke_width: width,
            stroke_over_fill: over,
            leading: 0.6,
            tracking: -150.,
            ..Default::default()
        },
    })
    .unwrap();
    for (property, value) in [
        (Property::AnchorX, 0.),
        (Property::AnchorY, 0.),
        (Property::PositionX, 70.),
        (Property::PositionY, 65.),
    ] {
        e.execute(Command::SetValue {
            id: 1,
            property,
            frame: 0,
            value,
        })
        .unwrap();
    }
    assert!(TextParam::ALL.into_iter().all(|parameter| {
        e.selected_layer()
            .unwrap()
            .track(PropertyPath::Text(parameter))
            .is_none()
    }));
    e
}

fn animate(e: &mut Editor) {
    for (parameter, value) in TextParam::ALL
        .into_iter()
        .zip([224., 192., 160., 48., 112., 224., 48.])
    {
        e.execute(Command::EditText {
            id: 1,
            parameter,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        e.execute(Command::EditText {
            id: 1,
            parameter,
            edit: TrackEdit::Value { frame: 60, value },
        })
        .unwrap();
    }
}

#[test]
fn animated_fill_matches_manual_static_reference_and_preserves_source() {
    let mut e = scene(false, true, false, false, FILLS[0], STROKES[0], WIDTHS[0]);
    animate(&mut e);
    let before = e.project().clone();
    let json = before.to_json().unwrap();
    let native = crate::project_io::encode_native_project(&before, None).unwrap();
    let saved = crate::project_io::decode_project(&native).unwrap().project;
    assert_eq!(saved, before);
    assert_eq!(Project::from_json(&json).unwrap(), before);
    let renderer = crate::rendering::Renderer::new();
    for (i, frame) in FRAMES.into_iter().enumerate() {
        let reference = scene(false, true, false, false, FILLS[i], STROKES[i], WIDTHS[i]);
        let expected = renderer.render(reference.project(), frame, 480).unwrap();
        let actual = renderer.render(&saved, frame, 480).unwrap();
        assert_eq!(actual, expected, "manual fill reference at {frame}");
        let channels = [
            (FILLS[i] >> 16) as u8,
            (FILLS[i] >> 8) as u8,
            FILLS[i] as u8,
        ];
        let opaque: Vec<_> = actual.pixels().filter(|p| p[3] == 255).collect();
        assert!(opaque.len() > 100);
        assert!(opaque.iter().all(|p| p.0[..3] == channels));
        assert_eq!(renderer.render_preview(&saved, frame, 480).unwrap(), actual);
        assert_eq!(
            renderer.render_output(&saved, frame, 480, 320).unwrap(),
            actual
        );
    }
    assert_eq!(e.project(), &before);
    assert_eq!(e.project().to_json().unwrap(), json);
    assert_eq!(
        crate::project_io::encode_native_project(e.project(), None).unwrap(),
        native
    );
}

#[test]
fn animated_point_and_paragraph_paint_orders_switches_and_clipping_match_static() {
    let renderer = crate::rendering::Renderer::new();
    for paragraph in [false, true] {
        for (fill, stroke) in [(true, false), (false, true), (true, true), (false, false)] {
            for over in [false, true] {
                let mut e = scene(
                    paragraph, fill, stroke, over, FILLS[0], STROKES[0], WIDTHS[0],
                );
                animate(&mut e);
                let before = e.project().clone();
                for (i, frame) in FRAMES.into_iter().enumerate() {
                    let reference = scene(
                        paragraph, fill, stroke, over, FILLS[i], STROKES[i], WIDTHS[i],
                    );
                    let actual = renderer.render(e.project(), frame, 480).unwrap();
                    assert_eq!(
                        actual,
                        renderer.render(reference.project(), frame, 480).unwrap(),
                        "paragraph={paragraph} fill={fill} stroke={stroke} over={over} frame={frame}"
                    );
                    if !fill && (!stroke || i == 0) {
                        assert!(actual.pixels().all(|p| p[3] == 0));
                    } else {
                        assert!(actual.pixels().any(|p| p[3] == 255));
                    }
                    if paragraph {
                        for (x, y, pixel) in actual.enumerate_pixels() {
                            if !(70..250).contains(&x) || !(65..225).contains(&y) {
                                assert_eq!(pixel[3], 0, "Paragraph paint must stay inside its box");
                            }
                        }
                    }
                }
                assert_eq!(e.project(), &before);
            }
        }
    }
}

#[test]
fn animated_fill_and_stroke_keep_interior_rgb_and_layer_alpha() {
    let renderer = crate::rendering::Renderer::new();
    for stroke in [false, true] {
        let mut e = scene(
            false, !stroke, stroke, false, FILLS[0], STROKES[0], WIDTHS[0],
        );
        animate(&mut e);
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 60.,
        })
        .unwrap();
        for (i, frame) in FRAMES.into_iter().enumerate() {
            let mut reference = scene(
                false, !stroke, stroke, false, FILLS[i], STROKES[i], WIDTHS[i],
            );
            reference
                .execute(Command::SetValue {
                    id: 1,
                    property: Property::Opacity,
                    frame: 0,
                    value: 60.,
                })
                .unwrap();
            let actual = renderer.render(e.project(), frame, 480).unwrap();
            assert_eq!(
                actual,
                renderer.render(reference.project(), frame, 480).unwrap()
            );
            if stroke && i == 0 {
                continue;
            }
            let rgb = if stroke { STROKES[i] } else { FILLS[i] };
            let channels = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
            let interior: Vec<_> = actual.pixels().filter(|p| p[3] == 153).collect();
            assert!(
                interior.len() > 100,
                "straight-alpha interiors at frame {frame}"
            );
            assert!(actual.pixels().all(|p| p[3] <= 153));
            assert!(interior.iter().all(|p| {
                p.0[..3]
                    .iter()
                    .zip(channels)
                    .all(|(a, b)| a.abs_diff(b) <= 1)
            }));
        }
    }
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

fn effect(e: &mut Editor, kind: EffectKind) {
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Add(kind),
    })
    .unwrap();
}

#[test]
fn animated_wide_point_stroke_effect_bounds_include_overflowing_glyphs() {
    let renderer = crate::rendering::Renderer::new();
    let mut animated = scene(false, true, true, true, FILLS[0], STROKES[0], 0.);
    narrow_point(&mut animated);
    animate(&mut animated);
    effect(&mut animated, EffectKind::Brightness); // Identity effect still owns a filter region.
    let before = animated.project().clone();
    let zero_width = renderer.render(&before, 0, 480).unwrap();
    for (i, frame) in FRAMES.into_iter().enumerate() {
        let mut reference = scene(false, true, true, true, FILLS[i], STROKES[i], WIDTHS[i]);
        narrow_point(&mut reference);
        let unfiltered = renderer.render(reference.project(), frame, 480).unwrap();
        effect(&mut reference, EffectKind::Brightness);
        let expected = renderer.render(reference.project(), frame, 480).unwrap();
        assert!(
            expected == unfiltered,
            "identity effect must not crop static overflow at {frame}"
        );
        let actual = renderer.render(animated.project(), frame, 480).unwrap();
        assert_eq!(
            actual, expected,
            "effect bounds must use animated width at {frame}"
        );
        assert!(
            actual
                .enumerate_pixels()
                .any(|(x, _, p)| x > 180 && p[3] > 0)
        );
        if frame > 0 {
            assert!(
                actual
                    .pixels()
                    .zip(zero_width.pixels())
                    .any(|(painted, zero)| painted[3] > 0 && zero[3] == 0),
                "animated stroke must extend beyond zero-width glyph support at {frame}"
            );
        }
    }
    assert_eq!(animated.project(), &before);
    // The supported maximum remains bounded and is sampled without falling back
    // to the base width, even when its outline covers the output canvas.
    animated
        .execute(Command::EditText {
            id: 1,
            parameter: TextParam::StrokeWidth,
            edit: TrackEdit::Value {
                frame: 60,
                value: 1000.,
            },
        })
        .unwrap();
    let mut reference = scene(false, true, true, true, FILLS[4], STROKES[4], 1000.);
    narrow_point(&mut reference);
    effect(&mut reference, EffectKind::Brightness);
    assert_eq!(
        renderer.render(animated.project(), 60, 480).unwrap(),
        renderer.render(reference.project(), 60, 480).unwrap()
    );
}

fn masked_nested(e: &mut Editor, nested: bool) {
    e.execute(Command::SetMask {
        id: 1,
        mask: Some(Mask {
            x: 20.,
            y: 5.,
            width: 130.,
            height: 115.,
            inverted: false,
        }),
    })
    .unwrap();
    effect(e, EffectKind::GaussianBlur);
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::Radius,
            frame: 0,
            value: 2.,
        },
    })
    .unwrap();
    e.execute(Command::SetValue {
        id: 1,
        property: Property::Opacity,
        frame: 0,
        value: 60.,
    })
    .unwrap();
    if nested {
        e.execute(Command::Precompose {
            layers: vec![1],
            name: "Animated text source".into(),
        })
        .unwrap();
        let id = e.selected().unwrap();
        e.execute(Command::Precompose {
            layers: vec![id],
            name: "Nested text source".into(),
        })
        .unwrap();
    }
}

#[test]
fn animated_paint_matches_static_through_masks_effects_and_nested_compositions() {
    let renderer = crate::rendering::Renderer::new();
    for paragraph in [false, true] {
        for nested in [false, true] {
            let mut e = scene(
                paragraph, true, true, false, FILLS[0], STROKES[0], WIDTHS[0],
            );
            animate(&mut e);
            masked_nested(&mut e, nested);
            let original = e.project().clone();
            let native = crate::project_io::encode_native_project(&original, None).unwrap();
            let saved = crate::project_io::decode_project(&native).unwrap().project;
            assert_eq!(saved, original);
            assert_eq!(
                Project::from_json(&original.to_json().unwrap()).unwrap(),
                original
            );
            for (i, frame) in FRAMES.into_iter().enumerate() {
                let mut reference = scene(
                    paragraph, true, true, false, FILLS[i], STROKES[i], WIDTHS[i],
                );
                masked_nested(&mut reference, nested);
                let actual = renderer.render(&saved, frame, 480).unwrap();
                assert_eq!(
                    actual,
                    renderer.render(reference.project(), frame, 480).unwrap(),
                    "masked paragraph={paragraph} nested={nested} frame={frame}"
                );
                assert!(actual.pixels().any(|p| p[3] > 0));
                assert_eq!(actual.get_pixel(0, 0).0, [0; 4]);
                assert_eq!(renderer.render_preview(&saved, frame, 480).unwrap(), actual);
                assert_eq!(
                    renderer.render_output(&saved, frame, 480, 320).unwrap(),
                    actual
                );
            }
            assert_eq!(e.project(), &original);
        }
    }
}

#[test]
fn animated_paint_png_sequence_matches_preview_stills_and_manual_references() {
    let mut e = scene(false, true, true, false, FILLS[0], STROKES[0], WIDTHS[0]);
    animate(&mut e);
    let before = e.project().clone();
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
        e.project(),
        None,
        0..61,
        &output,
        Default::default(),
        progress.clone(),
    )
    .unwrap();
    assert_eq!(progress.load(std::sync::atomic::Ordering::Relaxed), 5);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.path.join("sequence.json")).unwrap()).unwrap();
    assert_eq!(manifest["rendered_frames"], 5);
    let renderer = crate::rendering::Renderer::new();
    for (i, frame) in FRAMES.into_iter().enumerate() {
        let pixels = image::open(output.path.join(format!("frame-{i:06}.png")))
            .unwrap()
            .to_rgba8();
        let reference = scene(false, true, true, false, FILLS[i], STROKES[i], WIDTHS[i]);
        assert_eq!(
            pixels,
            renderer.render(reference.project(), frame, 480).unwrap()
        );
        assert_eq!(
            pixels,
            renderer.render_preview(e.project(), frame, 480).unwrap()
        );
        assert_eq!(
            pixels,
            renderer
                .render_output(e.project(), frame, 480, 320)
                .unwrap()
        );
    }
    assert_eq!(e.project(), &before);
}
