//! Headless sampled-source regressions. References are separately constructed
//! static documents with literal strings and independently calculated values.
//! These tests do not constitute native UI or system-font parity evidence.
use super::Renderer;
use libre_effects_core::*;

pub(crate) const BASE: &str = "Static baseline";
pub(crate) const FIRST: &str = "FIRST";
pub(crate) const UNICODE: &str = "한글 e\u{301} 👩‍💻\r\nLong wrapped title";
pub(crate) const LAST: &str = "A<&\"\nLAST";
const FRAMES: [u32; 10] = [0, 9, 10, 19, 20, 29, 30, 39, 40, 60];
const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

pub(crate) fn expected_source(frame: Frame) -> &'static str {
    match frame {
        0..20 => FIRST,
        20..30 => UNICODE,
        30..40 => "",
        _ => LAST,
    }
}

fn values(t: f64) -> [(TextParam, f64); 8] {
    [
        (TextParam::FontSize, 24. + 24. * t),
        (TextParam::Tracking, 120. * t),
        (TextParam::Leading, 1. + t),
        (TextParam::FillRed, 64. + 128. * t),
        (TextParam::StrokeBlue, 32. + 192. * t),
        (TextParam::StrokeWidth, 2. + 6. * t),
        (TextParam::FillOpacity, 100. - 80. * t),
        (TextParam::StrokeOpacity, 100. - 40. * t),
    ]
}

fn scene(paragraph: bool, text: &str, t: f64) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Source Text oracle".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: text.into(),
            font_size: 24.,
        },
        width: 175.,
        height: 125.,
        name: "Source Text".into(),
    })
    .unwrap();
    e.execute(Command::SetColor {
        id: 1,
        color: 0x40a0c0,
    })
    .unwrap();
    e.execute(Command::SetTextStyle {
        id: 1,
        style: TextStyle {
            paragraph,
            leading: 1.,
            stroke_enabled: true,
            stroke_color: 0xc06020,
            stroke_width: 2.,
            stroke_over_fill: true,
            ..Default::default()
        },
    })
    .unwrap();
    for (parameter, value) in values(t) {
        e.execute(Command::EditText {
            id: 1,
            parameter,
            edit: TrackEdit::Value { frame: 0, value },
        })
        .unwrap();
    }
    for (property, value) in [
        (Property::AnchorX, 0.),
        (Property::AnchorY, 0.),
        (Property::PositionX, 16.),
        (Property::PositionY, 18.),
        (Property::Opacity, 80.),
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

pub(crate) fn animated_scene(paragraph: bool) -> Editor {
    let mut e = scene(paragraph, BASE, 0.);
    for (parameter, value) in values(1.) {
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
    e.execute(Command::EditTrack {
        id: 1,
        property: PropertyPath::SourceText,
        edit: TrackEdit::ToggleAnimation { frame: 10 },
    })
    .unwrap();
    for (frame, text) in [(10, FIRST), (20, UNICODE), (30, ""), (40, LAST)] {
        e.execute(Command::EditSourceText {
            id: 1,
            frame,
            text: text.into(),
        })
        .unwrap();
    }
    e
}

pub(crate) fn baked_scene(paragraph: bool, frame: Frame) -> Editor {
    scene(
        paragraph,
        expected_source(frame),
        frame.min(60) as f64 / 60.,
    )
}

#[test]
fn source_text_hold_boundaries_unicode_empty_and_xml_match_baked_static_pixels() {
    let renderer = Renderer::new();
    for paragraph in [false, true] {
        let e = animated_scene(paragraph);
        let before = e.project().clone();
        let json = before.to_json().unwrap();
        let native = crate::project_io::encode_native_project(&before, None).unwrap();
        let saved = crate::project_io::decode_project(&native).unwrap().project;
        assert_eq!(saved, before);
        assert_eq!(Project::from_json(&json).unwrap(), before);
        for frame in FRAMES {
            let static_e = baked_scene(paragraph, frame);
            let expected = renderer.render(static_e.project(), frame, WIDTH).unwrap();
            let actual = renderer.render(&saved, frame, WIDTH).unwrap();
            assert_eq!(actual, expected, "paragraph={paragraph} frame={frame}");
            assert_eq!(
                renderer.render_preview(&saved, frame, WIDTH).unwrap(),
                expected
            );
            assert_eq!(
                renderer
                    .render_output(&saved, frame, WIDTH, HEIGHT)
                    .unwrap(),
                expected
            );
            assert_eq!(
                actual.pixels().all(|pixel| pixel[3] == 0),
                expected_source(frame).is_empty(),
                "nonempty fixtures must actually paint: paragraph={paragraph} frame={frame}"
            );
        }
        assert_eq!(e.project().to_json().unwrap(), json);
        assert_eq!(
            crate::project_io::encode_native_project(e.project(), None).unwrap(),
            native
        );
    }
}

fn effects_and_masks(e: &mut Editor) {
    for kind in [EffectKind::GaussianBlur, EffectKind::DropShadow] {
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(kind),
        })
        .unwrap();
    }
    e.execute(Command::SetMask {
        id: 1,
        mask: Some(Mask {
            x: 0.,
            y: 0.,
            width: 165.,
            height: 120.,
            inverted: false,
        }),
    })
    .unwrap();
    e.execute(Command::SetPathMasks {
        id: 1,
        masks: vec![PathMask {
            path: VectorPath {
                closed: true,
                vertices: [[5., 0.], [170., 0.], [150., 125.], [0., 125.]]
                    .map(PathVertex::corner)
                    .to_vec(),
            },
            ..Default::default()
        }],
    })
    .unwrap();
}

fn as_matte(e: &mut Editor, mode: MatteMode) {
    // Source Text is the matte source, so changing its sampled glyph geometry
    // must affect the target even when the source is not independently visible.
    e.execute(Command::AddContent {
        content: Content::Solid,
        width: WIDTH.into(),
        height: HEIGHT.into(),
        name: "Matte target".into(),
    })
    .unwrap();
    e.execute(Command::SetColor {
        id: 2,
        color: 0x70c090,
    })
    .unwrap();
    e.execute(Command::SetTrackMatte {
        id: 2,
        matte: Some(TrackMatte { source: 1, mode }),
    })
    .unwrap();
    e.execute(Command::ToggleVisible(1)).unwrap();
}

#[test]
fn source_text_effect_bounds_masks_and_text_matte_sources_match_static_frames() {
    let renderer = Renderer::new();
    for mode in [
        None,
        Some(MatteMode::Alpha),
        Some(MatteMode::Luma),
        Some(MatteMode::LumaInverted),
    ] {
        let mut e = animated_scene(false);
        effects_and_masks(&mut e);
        if let Some(mode) = mode {
            as_matte(&mut e, mode);
        }
        let before = e.project().clone();
        for frame in [9, 20, 29, 30, 40, 60] {
            let mut static_e = baked_scene(false, frame);
            effects_and_masks(&mut static_e);
            if let Some(mode) = mode {
                as_matte(&mut static_e, mode);
            }
            assert_eq!(
                renderer.render(e.project(), frame, WIDTH).unwrap(),
                renderer.render(static_e.project(), frame, WIDTH).unwrap(),
                "effects/masks/matte {mode:?} frame={frame}"
            );
        }
        assert_eq!(e.project(), &before);
    }
}

#[test]
fn source_text_nested_reverse_remap_uses_resolved_child_frame() {
    let renderer = Renderer::new();
    let mut e = animated_scene(true);
    effects_and_masks(&mut e);
    for name in ["Source Text child", "Source Text nested"] {
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
    let before = e.project().clone();
    let saved = crate::project_io::decode_project(
        &crate::project_io::encode_native_project(&before, None).unwrap(),
    )
    .unwrap()
    .project;
    for frame in [0, 20, 21, 30, 31, 40, 41, 50, 51, 60] {
        let child_frame = 60 - frame;
        let mut static_e = baked_scene(true, child_frame);
        effects_and_masks(&mut static_e);
        assert_eq!(
            renderer.render(&saved, frame, WIDTH).unwrap(),
            renderer
                .render(static_e.project(), child_frame, WIDTH)
                .unwrap(),
            "parent={frame} child={child_frame}"
        );
    }
    assert_eq!(e.project(), &before);
}
