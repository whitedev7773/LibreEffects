//! F03 pixel oracles replace persisted static typography and remove only its
//! tracks. Expected geometry never uses the production typography/track sampler.
use crate::rendering::Renderer;
use libre_effects_core::*;
use serde_json::Value;

const WIDTH: u32 = 384;
const HEIGHT: u32 = 256;
const FRAMES: [Frame; 5] = [0, 15, 30, 45, 60];
const PARAMETERS: [TextParam; 3] = [TextParam::FontSize, TextParam::Tracking, TextParam::Leading];
const BASE: [f64; 3] = [32., -200., 0.5];
const LINEAR: [[f64; 3]; 5] = [
    [32., -200., 0.5],
    [44., -50., 0.75],
    [56., 100., 1.],
    [68., 250., 1.25],
    [80., 400., 1.5],
];
const TEXT: &str = "AVATAR WIDE\nHH OO";

fn value(e: &mut Editor, id: LayerId, property: Property, value: f64) {
    e.execute(Command::SetValue {
        id,
        property,
        frame: 0,
        value,
    })
    .unwrap();
}
fn scene(paragraph: bool, text: &str, size: [f64; 2]) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Independent typography reference".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
        duration: 91,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: text.into(),
            font_size: BASE[0],
        },
        width: size[0],
        height: size[1],
        name: "Typography".into(),
    })
    .unwrap();
    e.execute(Command::SetColor {
        id: 1,
        color: 0x3876c8,
    })
    .unwrap();
    e.execute(Command::SetTextStyle {
        id: 1,
        style: TextStyle {
            paragraph,
            tracking: BASE[1],
            leading: BASE[2],
            ..Default::default()
        },
    })
    .unwrap();
    for (p, v) in [
        (Property::AnchorX, 0.),
        (Property::AnchorY, 0.),
        (Property::PositionX, 28.),
        (Property::PositionY, 36.),
    ] {
        value(&mut e, 1, p, v);
    }
    assert!(PARAMETERS.into_iter().all(|p| {
        e.selected_layer()
            .unwrap()
            .track(PropertyPath::Text(p))
            .is_none()
    }));
    e
}
fn animate_parameter(e: &mut Editor, parameter: TextParam, end: f64) {
    for edit in [
        TrackEdit::ToggleAnimation { frame: 0 },
        TrackEdit::Value {
            frame: 60,
            value: end,
        },
    ] {
        e.execute(Command::EditText {
            id: 1,
            parameter,
            edit,
        })
        .unwrap();
    }
}
fn animate(e: &mut Editor) {
    for (p, end) in PARAMETERS.into_iter().zip(LINEAR[4]) {
        animate_parameter(e, p, end);
    }
}
fn paint(e: &mut Editor) {
    let mut style = e.project().composition().layer(1).unwrap().text_style();
    style.stroke_enabled = true;
    style.stroke_width = 2.;
    style.stroke_color = 0xf08030;
    e.execute(Command::SetTextStyle { id: 1, style }).unwrap();
    // A geometry reference must leave independent animated paint intact.
    animate_parameter(e, TextParam::FillRed, 200.);
    animate_parameter(e, TextParam::StrokeWidth, 8.);
}
fn samples(values: [[f64; 3]; 5]) -> [(Frame, [f64; 3]); 5] {
    std::array::from_fn(|i| (FRAMES[i], values[i]))
}
fn static_reference(project: &Project, typography: [f64; 3]) -> Project {
    fn freeze(composition: &mut Value, typography: [f64; 3]) -> usize {
        let mut count = 0;
        for layer in composition["layers"].as_array_mut().unwrap() {
            if layer["content"].get("Text").is_none() {
                continue;
            }
            layer["content"]["Text"]["font_size"] = typography[0].into();
            if layer.get("text_style").is_none() {
                layer["text_style"] = serde_json::to_value(TextStyle::default()).unwrap();
            }
            layer["text_style"]["tracking"] = typography[1].into();
            layer["text_style"]["leading"] = typography[2].into();
            if let Some(tracks) = layer
                .get_mut("text_parameters")
                .and_then(Value::as_object_mut)
            {
                for key in ["FontSize", "Tracking", "Leading"] {
                    tracks.remove(key);
                }
            }
            count += 1;
        }
        count
    }
    let mut json: Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
    let mut count = freeze(&mut json["composition"], typography);
    if let Some(compositions) = json
        .get_mut("other_compositions")
        .and_then(Value::as_object_mut)
    {
        for composition in compositions.values_mut() {
            count += freeze(composition, typography);
        }
    }
    assert_eq!(
        count, 1,
        "Every fixture deliberately has exactly one text source"
    );
    // Source size, alignment, font identity, paint, effects, masks, transforms and
    // nested timing are copied verbatim; no production geometry helper is called.
    Project::from_json(&json.to_string()).unwrap()
}
fn pixels_equal(
    actual: &image::RgbaImage,
    expected: &image::RgbaImage,
    frame: Frame,
    context: &str,
) {
    assert_eq!(
        actual.dimensions(),
        expected.dimensions(),
        "{context} at {frame}"
    );
    if actual != expected {
        let mut changed = 0;
        let mut alpha_changed = 0;
        let mut support_changed = 0;
        let mut maximum_delta = [0u8; 4];
        let mut first = Vec::new();
        for (x, y, pixel) in actual.enumerate_pixels() {
            let reference = expected.get_pixel(x, y);
            if pixel == reference {
                continue;
            }
            changed += 1;
            alpha_changed += usize::from(pixel[3] != reference[3]);
            support_changed += usize::from((pixel[3] == 0) != (reference[3] == 0));
            for channel in 0..4 {
                maximum_delta[channel] =
                    maximum_delta[channel].max(pixel[channel].abs_diff(reference[channel]));
            }
            if first.len() < 8 {
                first.push((x, y, pixel.0, reference.0));
            }
        }
        panic!(
            "{context} at frame {frame}: {changed} changed pixels, {alpha_changed} changed alpha, {support_changed} changed alpha support, max RGBA delta {maximum_delta:?}; first (x, y, actual, expected): {first:?}"
        );
    }
}
fn verify_samples(e: &Editor, samples: &[(Frame, [f64; 3])]) -> Vec<image::RgbaImage> {
    let original = e.project().clone();
    let json = original.to_json().unwrap();
    assert_eq!(serde_json::from_str::<Value>(&json).unwrap()["version"], 49);
    let json_saved = Project::from_json(&json).unwrap();
    let native = crate::project_io::encode_native_project(&original, None).unwrap();
    let saved = crate::project_io::decode_project(&native).unwrap().project;
    assert_eq!(saved, original);
    assert_eq!(json_saved, original);
    let renderer = Renderer::new();
    let mut rendered = Vec::new();
    for &(frame, typography) in samples {
        let reference = static_reference(&original, typography);
        let expected = renderer.render(&reference, frame, WIDTH).unwrap();
        assert!(
            expected.pixels().any(|p| p[3] > 0),
            "Nonempty fixture at {frame}"
        );
        let actual = renderer.render(e.project(), frame, WIDTH).unwrap();
        pixels_equal(&actual, &expected, frame, "static geometry reference");
        pixels_equal(
            &renderer.render_preview(e.project(), frame, WIDTH).unwrap(),
            &expected,
            frame,
            "preview",
        );
        pixels_equal(
            &renderer
                .render_output(e.project(), frame, WIDTH, HEIGHT)
                .unwrap(),
            &expected,
            frame,
            "output",
        );
        pixels_equal(
            &renderer.render(&saved, frame, WIDTH).unwrap(),
            &expected,
            frame,
            "native reload",
        );
        pixels_equal(
            &renderer.render(&json_saved, frame, WIDTH).unwrap(),
            &expected,
            frame,
            "JSON reload",
        );
        rendered.push(actual);
    }
    assert_eq!(
        e.project(),
        &original,
        "Rendering must not bake typography into source data"
    );
    assert_eq!(e.project().to_json().unwrap(), json);
    assert_eq!(
        crate::project_io::encode_native_project(e.project(), None).unwrap(),
        native
    );
    rendered
}
fn scalar_case(parameter: TextParam, index: usize) {
    for paragraph in [false, true] {
        let mut e = scene(paragraph, TEXT, [188., 140.]);
        animate_parameter(&mut e, parameter, LINEAR[4][index]);
        let expected = LINEAR.map(|v| {
            let mut t = BASE;
            t[index] = v[index];
            t
        });
        let rendered = verify_samples(&e, &samples(expected));
        assert_ne!(
            rendered[0], rendered[4],
            "The animated scalar must visibly affect geometry"
        );
        assert_eq!(
            PARAMETERS
                .into_iter()
                .filter(|p| e
                    .selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(*p))
                    .is_some())
                .count(),
            1
        );
    }
}

#[test]
fn typography_font_size_only_matches_static_point_and_paragraph() {
    scalar_case(TextParam::FontSize, 0);
}
#[test]
fn typography_tracking_only_matches_static_point_and_paragraph() {
    scalar_case(TextParam::Tracking, 1);
}
#[test]
fn typography_leading_only_matches_static_point_and_paragraph() {
    scalar_case(TextParam::Leading, 2);
}
#[test]
fn typography_combined_wrap_overlap_overflow_and_paint_match_static() {
    for paragraph in [false, true] {
        let mut e = scene(paragraph, TEXT, [188., 140.]);
        animate(&mut e);
        paint(&mut e);
        let rendered = verify_samples(&e, &samples(LINEAR));
        assert_ne!(rendered[0], rendered[4]);
        for image in rendered {
            if paragraph {
                assert!(
                    image
                        .enumerate_pixels()
                        .all(|(x, y, p)| p[3] == 0
                            || ((28..216).contains(&x) && (36..176).contains(&y))),
                    "Paragraph paint must remain inside its declared box"
                );
            }
        }
        if !paragraph {
            let frame = Renderer::new().render(e.project(), 60, WIDTH).unwrap();
            assert!(
                frame
                    .enumerate_pixels()
                    .any(|(x, _, p)| x >= 216 && p[3] > 0),
                "Point text must overflow the nominal source box"
            );
        }
    }
}
#[test]
fn typography_hold_and_smoothstep_use_independent_intermediate_geometry() {
    let smooth = [
        [32., -200., 0.5],
        [39.5, -106.25, 0.65625],
        [56., 100., 1.],
        [72.5, 306.25, 1.34375],
        [80., 400., 1.5],
    ]; // t*t*(3-2*t) at t = 0, 1/4, 1/2, 3/4, 1.
    for (interpolation, expected) in [
        (Interpolation::Hold, [BASE, BASE, BASE, BASE, LINEAR[4]]),
        (Interpolation::Smooth, smooth),
    ] {
        let mut e = scene(false, TEXT, [188., 140.]);
        animate(&mut e);
        for parameter in PARAMETERS {
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
        let rendered = verify_samples(&e, &samples(expected));
        if interpolation == Interpolation::Hold {
            assert_eq!(rendered[0], rendered[3]);
        } else {
            assert_ne!(rendered[1], rendered[3]);
        }
    }
}
#[test]
fn typography_unicode_graphemes_bidi_fallback_and_alignment_match_static() {
    let text = "AV e\u{301} ffi\nאבג नमस्ते\n한글 世界 👩‍💻";
    let expected = [
        [32., -200., 0.5],
        [36., -100., 0.75],
        [40., 0., 1.],
        [44., 100., 1.25],
        [48., 200., 1.5],
    ];
    for (paragraph, align, family) in [
        (false, TextAlign::Center, "Wanted Sans"),
        (true, TextAlign::Right, "Missing F03 Typography QA Font"),
    ] {
        let mut e = scene(paragraph, text, [328., 196.]);
        let mut style = e.selected_layer().unwrap().text_style();
        style.align = align;
        style.font_family = family.into();
        e.execute(Command::SetTextStyle { id: 1, style }).unwrap();
        for (parameter, end) in PARAMETERS.into_iter().zip(expected[4]) {
            animate_parameter(&mut e, parameter, end);
        }
        let rendered = verify_samples(&e, &samples(expected));
        assert_ne!(rendered[0], rendered[4]);
        assert!(
            matches!(e.selected_layer().unwrap().content(), Content::Text { text: source, .. } if source == text)
        );
    }
}
fn effect(e: &mut Editor, kind: EffectKind) {
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Add(kind),
    })
    .unwrap();
}
#[test]
fn typography_filter_bounds_include_animated_point_overflow_and_outlines() {
    let mut e = scene(false, "OOO\nIII", [28., 28.]);
    let mut style = e.selected_layer().unwrap().text_style();
    style.stroke_enabled = true;
    style.stroke_width = 10.;
    style.stroke_color = 0xf08030;
    e.execute(Command::SetTextStyle { id: 1, style }).unwrap();
    animate(&mut e);
    let unfiltered = e.project().clone();
    effect(&mut e, EffectKind::Brightness); // Identity, but allocates a filter region.
    let actual = verify_samples(&e, &samples(LINEAR));
    let renderer = Renderer::new();
    // This point source is left-aligned with zero anchors. Enlarging only its
    // declared size leaves glyph geometry fixed and gives the same filter an
    // independently oversized safety region, retaining its left/top raster origin.
    let mut oversized: Value = serde_json::from_str(&e.project().to_json().unwrap()).unwrap();
    oversized["composition"]["layers"][0]["width"] = 768.into();
    oversized["composition"]["layers"][0]["height"] = 512.into();
    let oversized = Project::from_json(&oversized.to_string()).unwrap();
    for (i, frame) in FRAMES.into_iter().enumerate() {
        let safe_reference = renderer
            .render(&static_reference(&oversized, LINEAR[i]), frame, WIDTH)
            .unwrap();
        pixels_equal(
            &actual[i],
            &safe_reference,
            frame,
            "Exact oversized-filter static reference",
        );
        let expected = renderer
            .render(&static_reference(&unfiltered, LINEAR[i]), frame, WIDTH)
            .unwrap();
        // The pinned C09 renderer already changes one interior pixel when
        // this static fixture is rasterized into a filter's offscreen surface.
        // Zero-offset filters and extra padding reproduce the same difference;
        // neither alpha support nor extents change. Check cropping exactly here.
        // verify_samples above still requires full RGBA equality between every
        // animated filtered frame and its independent static filtered reference.
        let support_difference = actual[i]
            .enumerate_pixels()
            .find(|(x, y, pixel)| (pixel[3] > 0) != (expected.get_pixel(*x, *y)[3] > 0));
        assert!(
            support_difference.is_none(),
            "Identity filter changed glyph/stroke support at frame {frame}: {support_difference:?}"
        );
        let extent = |image: &image::RgbaImage| {
            image.enumerate_pixels().filter(|(_, _, p)| p[3] > 0).fold(
                [image.width(), image.height(), 0, 0],
                |[left, top, right, bottom], (x, y, _)| {
                    [left.min(x), top.min(y), right.max(x), bottom.max(y)]
                },
            )
        };
        assert_eq!(
            extent(&actual[i]),
            extent(&expected),
            "Identity filter clipped extents at frame {frame}"
        );
        assert!(
            actual[i]
                .enumerate_pixels()
                .any(|(x, _, p)| x > 56 && p[3] > 0)
        );
        assert!(
            actual[i]
                .enumerate_pixels()
                .any(|(_, y, p)| y > 64 && p[3] > 0)
        );
    }
}
fn decorate(e: &mut Editor, matte: bool) {
    e.execute(Command::SetMask {
        id: 1,
        mask: Some(Mask {
            x: 0.,
            y: 0.,
            width: 175.,
            height: 140.,
            inverted: false,
        }),
    })
    .unwrap();
    e.execute(Command::SetPathMasks {
        id: 1,
        masks: vec![PathMask {
            path: VectorPath {
                closed: true,
                vertices: [[3., 2.], [172., 14.], [156., 138.], [0., 120.]]
                    .map(PathVertex::corner)
                    .to_vec(),
            },
            ..Default::default()
        }],
    })
    .unwrap();
    effect(e, EffectKind::DropShadow);
    for (parameter, v) in [
        (EffectParam::Radius, 3.),
        (EffectParam::OffsetX, 9.),
        (EffectParam::OffsetY, 5.),
    ] {
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::SetValue {
                effect: 1,
                parameter,
                frame: 0,
                value: v,
            },
        })
        .unwrap();
    }
    value(e, 1, Property::Opacity, 70.);
    if matte {
        e.execute(Command::ToggleVisible(1)).unwrap();
        e.execute(Command::AddContent {
            content: Content::Rectangle,
            width: f64::from(WIDTH),
            height: f64::from(HEIGHT),
            name: "Matte consumer".into(),
        })
        .unwrap();
        for p in [
            Property::AnchorX,
            Property::AnchorY,
            Property::PositionX,
            Property::PositionY,
        ] {
            value(e, 2, p, 0.);
        }
        e.execute(Command::SetColor {
            id: 2,
            color: 0x38b78c,
        })
        .unwrap();
        e.execute(Command::SetTrackMatte {
            id: 2,
            matte: Some(TrackMatte {
                source: 1,
                mode: MatteMode::Alpha,
            }),
        })
        .unwrap();
    }
}
#[test]
fn typography_masks_effects_opacity_and_hidden_text_matte_match_static() {
    for matte in [false, true] {
        let mut e = scene(true, TEXT, [188., 140.]);
        animate(&mut e);
        paint(&mut e);
        decorate(&mut e, matte);
        let rendered = verify_samples(&e, &samples(LINEAR));
        assert_ne!(rendered[0], rendered[4]);
        assert!(
            rendered
                .iter()
                .all(|image| image.get_pixel(0, 0).0 == [0; 4])
        );
        assert!(
            rendered
                .iter()
                .all(|image| image.pixels().any(|p| p[3] > 0 && p[3] < 255))
        );
    }
}
#[test]
fn typography_nested_remap_samples_source_frames_and_preserves_animated_paint() {
    let mut e = scene(false, TEXT, [188., 140.]);
    animate(&mut e);
    paint(&mut e);
    decorate(&mut e, false);
    for name in ["Typography source", "Typography nested source"] {
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
    // At 30 fps, the outer 2 -> 0 second ramp reaches source frames
    // 60, 45, 30, 15, 0. No production source-time or typography sampler is an oracle.
    let expected = [LINEAR[4], LINEAR[3], LINEAR[2], LINEAR[1], LINEAR[0]];
    let rendered = verify_samples(&e, &samples(expected));
    assert_ne!(rendered[0], rendered[4]);
}
#[test]
fn typography_native_view_legacy_pixels_and_png_sequence_remain_compatible() {
    let mut legacy = scene(false, TEXT, [188., 140.]);
    let legacy_json = legacy.project().to_json().unwrap();
    assert!(!legacy_json.contains("text_parameters"));
    assert!(
        serde_json::from_str::<Value>(&legacy_json).unwrap()["version"]
            .as_u64()
            .unwrap()
            < 48
    );
    let views = crate::view_state::ProjectViews::default();
    let legacy_native =
        crate::project_io::encode_native_project(legacy.project(), Some(&views)).unwrap();
    let renderer = Renderer::new();
    let native_saved = crate::project_io::decode_project(&legacy_native).unwrap();
    let json_saved = Project::from_json(&legacy_json).unwrap();
    for frame in FRAMES {
        let expected = renderer.render(legacy.project(), frame, WIDTH).unwrap();
        pixels_equal(
            &renderer
                .render(&native_saved.project, frame, WIDTH)
                .unwrap(),
            &expected,
            frame,
            "legacy native",
        );
        pixels_equal(
            &renderer.render(&json_saved, frame, WIDTH).unwrap(),
            &expected,
            frame,
            "legacy JSON",
        );
    }
    assert_eq!(native_saved.views, views);
    assert_eq!(legacy.project().to_json().unwrap(), legacy_json);
    animate(&mut legacy);
    paint(&mut legacy);
    let original = legacy.project().clone();
    let native = crate::project_io::encode_native_project(&original, Some(&views)).unwrap();
    assert_eq!(&native[..8], project_file::MAGIC);
    assert_eq!(
        &native[8..10],
        &1u16.to_le_bytes(),
        "Model v49 must keep LEP container v1"
    );
    let decoded = project_file::decode(&native).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(decoded.view.unwrap()).unwrap()["version"],
        1,
        "Typography must not upgrade VIEW"
    );
    assert_eq!(decoded.project, original);
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
        &original,
        None,
        0..61,
        &output,
        Default::default(),
        progress.clone(),
    )
    .unwrap();
    assert_eq!(progress.load(std::sync::atomic::Ordering::Relaxed), 5);
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(output.path.join("sequence.json")).unwrap()).unwrap();
    assert_eq!(manifest["rendered_frames"], 5);
    for (i, frame) in FRAMES.into_iter().enumerate() {
        let pixels = image::open(output.path.join(format!("frame-{i:06}.png")))
            .unwrap()
            .to_rgba8();
        let reference = static_reference(&original, LINEAR[i]);
        let expected = renderer.render(&reference, frame, WIDTH).unwrap();
        pixels_equal(&pixels, &expected, frame, "PNG sequence");
        pixels_equal(
            &renderer.render_preview(&original, frame, WIDTH).unwrap(),
            &expected,
            frame,
            "PNG/preview",
        );
        pixels_equal(
            &renderer
                .render_output(&original, frame, WIDTH, HEIGHT)
                .unwrap(),
            &expected,
            frame,
            "PNG/output",
        );
    }
    assert_eq!(legacy.project(), &original);
    assert_eq!(
        crate::project_io::encode_native_project(legacy.project(), Some(&views)).unwrap(),
        native
    );
}
