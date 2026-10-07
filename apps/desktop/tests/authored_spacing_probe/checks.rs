//! Synthetic source positions and literal SVG oracles; no private source fonts,
//! lyrics, reference frames or renderer-derived expected positions.
use crate::{fonts, rendering, rich_text_render, text_edit};
use libre_effects_core::*;
use resvg::usvg;

const TEXT: &str = "AV éA\r\nBY";
const HASH: &str = "849b51938f5779e7526fcf09b3c737e9d401261d111f626280af16423bf19f40";

fn glyphs(text: &str, positions: &[f64]) -> Vec<AuthoredTextGlyph> {
    let face = rustybuzz::ttf_parser::Face::parse(
        include_bytes!("../../assets/fonts/WantedSans-Regular.ttf"),
        0,
    )
    .unwrap();
    assert_eq!(text.chars().count(), positions.len());
    text.char_indices()
        .zip(positions)
        .map(|((start, ch), &x)| AuthoredTextGlyph {
            start,
            end: start + ch.len_utf8(),
            glyph_id: face.glyph_index(ch).unwrap().0,
            x,
        })
        .collect()
}

fn fixture() -> (RichText, TextStyle) {
    let style = TextStyle {
        align: TextAlign::Center,
        ..Default::default()
    };
    let mut red = TextCharacterStyle::from_style(&style, 32.0, 0xff3322);
    red.font_face = "WantedSans-Regular".into();
    red.tracking = 125.0;
    red.leading = Some(TextLeading::Fixed(48.0));
    let mut blue = red.clone();
    blue.fill_color = 0x2266ee;
    let mut rich = RichText::new(
        TEXT,
        red.clone(),
        vec![
            TextStyleRun {
                start: 0,
                end: 3,
                style: red.clone(),
            },
            TextStyleRun {
                start: 3,
                end: TEXT.len(),
                style: blue,
            },
        ],
    )
    .unwrap();
    rich.point_origin = true;
    rich.positioning = Some(AuthoredTextPositions {
        text: TEXT.into(),
        align: style.align,
        lines: vec![AuthoredTextLine {
            start: 0,
            end: 6,
            font_sha256: HASH.into(),
            font_index: 0,
            style: red,
            glyphs: glyphs("AV éA", &[-40.0, -7.0, 18.0, 44.0, 76.0]),
            end_x: 101.0,
        }],
    });
    (rich, style)
}

fn literal(authored: bool, stroke: bool, anchor: &str) -> String {
    let family = fonts::svg_family(&TextStyle::default());
    let (x, first_anchor, tracking) = if authored {
        ("-40 -7 18 44 76", "start", "0")
    } else {
        ("0", anchor, "4")
    };
    let (red, blue) = if stroke {
        (
            "fill='none' stroke='#11cc44' stroke-width='2' stroke-linejoin='round'",
            "fill='none' stroke='#11cc44' stroke-width='2' stroke-linejoin='round'",
        )
    } else {
        ("fill='#ff3322'", "fill='#2266ee'")
    };
    format!(
        "<text x='{x}' y='0' text-anchor='{first_anchor}' font-family='{family}' font-size='32' letter-spacing='{tracking}' xml:space='preserve'><tspan {red}>AV </tspan><tspan {blue}>éA</tspan></text><text x='0' y='48' text-anchor='{anchor}' font-family='{family}' font-size='32' letter-spacing='4' xml:space='preserve' {blue}>BY</text>"
    )
}

fn raster(body: &str) -> image::RgbaImage {
    let body = format!("<g transform='translate(120 90)'>{body}</g>");
    let svg = rendering::svg_document(&body, 320.0, 200.0).unwrap();
    let tree = usvg::Tree::from_str(&svg, &fonts::render_options()).unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(320, 200).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let bytes = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let p = pixel.demultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        })
        .collect();
    image::RgbaImage::from_raw(320, 200, bytes).unwrap()
}

fn same_pixels(actual: &image::RgbaImage, expected: &image::RgbaImage) {
    assert_eq!(actual.dimensions(), expected.dimensions());
    let different = actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        different, 0,
        "RGBA pixels differ from the independent oracle"
    );
}

pub(crate) fn literal_pixels_and_carets() {
    for (align, anchor) in [
        (TextAlign::Left, "start"),
        (TextAlign::Center, "middle"),
        (TextAlign::Right, "end"),
    ] {
        let (mut rich, mut style) = fixture();
        style.align = align;
        rich.positioning.as_mut().unwrap().align = align;
        let body = rich_text_render::layer_svg(TEXT, &rich, 320.0, &style, [100.0; 2], false)
            .unwrap()
            .0;
        let pixels = raster(&body);
        same_pixels(&pixels, &raster(&literal(true, false, anchor)));
        assert!(pixels.pixels().any(|p| p[3] > 0));
        // Authored paint has been flattened; only the unannotated line is text.
        assert_eq!(body.matches("<text ").count(), 1);
        let layout = text_edit::layout::Layout::shape_rich(TEXT, &rich, 320.0, &style).unwrap();
        let wider = text_edit::layout::Layout::shape_rich(TEXT, &rich, 640.0, &style).unwrap();
        assert_eq!(layout.carets, wider.carets);
        for (at, x) in [
            (0, -40.0),
            (1, -7.0),
            (2, 18.0),
            (3, 44.0),
            (5, 76.0),
            (6, 101.0),
        ] {
            assert_eq!(layout.caret(at), [x, -32.0]);
            assert_eq!(layout.hit([x, -20.0]), at);
        }
        assert_eq!(layout.caret(6), layout.caret(7));
        assert!(!layout.carets.iter().any(|(byte, _)| *byte == 7));
        for cell in layout.cells.iter().filter(|cell| cell.range.start < 6) {
            let point = [(cell.x1 + cell.x2) * 0.5, -20.0];
            assert!(layout.contains(point));
            assert_eq!(layout.hit_character(point), cell.range.start);
        }
        assert!(layout.bounds()[0] <= -40.0);
        let old = rich.positioning.clone();
        for run in &mut rich.runs {
            run.style.stroke_enabled = true;
            run.style.stroke_color = 0x11cc44;
            run.style.stroke_width = 2.0;
            run.style.stroke_join = TextStrokeJoin::Round;
        }
        let painted = rich_text_render::layer_svg(TEXT, &rich, 320.0, &style, [100.0; 2], false)
            .unwrap()
            .0;
        same_pixels(
            &raster(&painted),
            &raster(&format!(
                "{}{}",
                literal(true, true, anchor),
                literal(true, false, anchor)
            )),
        );
        assert_eq!(rich.positioning, old);
        let painted_layout =
            text_edit::layout::Layout::shape_rich(TEXT, &rich, 320.0, &style).unwrap();
        assert_eq!(painted_layout.carets, layout.carets);
        let faded =
            rich_text_render::layer_svg(TEXT, &rich, 320.0, &style, [0.0, 50.0], true).unwrap();
        assert_eq!(faded.1.as_deref(), Some(painted.as_str()));
        assert_ne!(raster(&faded.0), raster(&painted));
    }
    println!(
        "PASS literal positioned SVG pixels, paint-only retention, CRLF caret/hit geometry, independent end caret, unannotated line and all alignments"
    );
}

pub(crate) fn legacy_and_rejections() {
    let (rich, style) = fixture();
    let mut uppercase = rich.clone();
    uppercase.positioning.as_mut().unwrap().lines[0].font_sha256 = HASH.to_uppercase();
    assert!(rich_text_render::compose(TEXT, &uppercase, 320.0, &style).is_ok());
    let mut native = rich.clone();
    native.positioning = None;
    let body = rich_text_render::layer_svg(TEXT, &native, 320.0, &style, [100.0; 2], false)
        .unwrap()
        .0;
    same_pixels(&raster(&body), &raster(&literal(false, false, "middle")));
    assert_eq!(body.matches("<text ").count(), 2);
    for (kind, expected) in [
        (0, "SHA-256"),
        (1, "face identity"),
        (2, "glyph ID"),
        (3, "glyph count"),
    ] {
        let mut invalid = rich.clone();
        let line = &mut invalid.positioning.as_mut().unwrap().lines[0];
        match kind {
            0 => line.font_sha256 = "0".repeat(64),
            1 => line.font_index = 1,
            2 => line.glyphs[1].glyph_id = line.glyphs[0].glyph_id,
            _ => {
                let text = "wanted_logo";
                let mut run_style = line.style.clone();
                run_style.tracking = 0.0;
                invalid = RichText::new(
                    text,
                    run_style.clone(),
                    vec![TextStyleRun {
                        start: 0,
                        end: text.len(),
                        style: run_style.clone(),
                    }],
                )
                .unwrap();
                invalid.point_origin = true;
                invalid.positioning = Some(AuthoredTextPositions {
                    text: text.into(),
                    align: style.align,
                    lines: vec![AuthoredTextLine {
                        start: 0,
                        end: text.len(),
                        font_sha256: HASH.into(),
                        font_index: 0,
                        style: run_style,
                        glyphs: glyphs(
                            text,
                            &(0..text.len()).map(|i| i as f64 * 30.0).collect::<Vec<_>>(),
                        ),
                        end_x: 400.0,
                    }],
                });
            }
        }
        let source = invalid.positioning.as_ref().unwrap().text.clone();
        let before = invalid.clone();
        let error = rich_text_render::compose(&source, &invalid, 320.0, &style).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("Reset authored spacing"), "{error}");
        assert_eq!(invalid, before);
    }
    let pinned = fonts::authored_font(&style).unwrap();
    assert_eq!(pinned.sha256, HASH);
    assert_eq!(pinned.post_script_name, "WantedSans-Regular");
    assert_eq!(pinned.index, 0);
    assert!(std::ptr::eq(pinned, fonts::authored_font(&style).unwrap()));
    let empty = usvg::Options {
        fontdb: std::sync::Arc::new(usvg::fontdb::Database::new()),
        ..Default::default()
    };
    let first_line = RichText {
        runs: vec![TextStyleRun {
            start: 0,
            end: 6,
            style: rich.runs[0].style.clone(),
        }],
        ..rich.clone()
    };
    let mut first_line = first_line;
    first_line.positioning.as_mut().unwrap().text = "AV éA".into();
    let paths = rich_text_render::layer_svg("AV éA", &first_line, 320.0, &style, [100.0; 2], false)
        .unwrap()
        .0;
    let svg = rendering::svg_document(
        &format!("<g transform='translate(120 90)'>{paths}</g>"),
        320.0,
        200.0,
    )
    .unwrap();
    let tree = usvg::Tree::from_str(&svg, &empty).unwrap();
    let mut pixels = resvg::tiny_skia::Pixmap::new(320, 200).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
    );
    assert!(
        pixels.pixels().iter().any(|p| p.alpha() > 0),
        "Pinned glyph paths reopened system fonts"
    );
    println!(
        "PASS legacy SVG pixels, exact bundled identity, cached pinned face, fontless final paint, wrong hash/index/glyph ID and ligature rejection without source mutation"
    );
}

pub(crate) fn production_roundtrip(output: Option<&std::path::Path>) {
    let (rich, style) = fixture();
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Synthetic authored spacing".into(),
            width: 320,
            height: 200,
            fps: 30,
            duration: 60,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: TEXT.into(),
                font_size: 32.0,
            },
            width: 320.0,
            height: 120.0,
            name: "Positioned synthetic text".into(),
        })
        .unwrap();
    let id = editor.selected().unwrap();
    editor.execute(Command::SetTextStyle { id, style }).unwrap();
    editor
        .execute(Command::SetRichText {
            id,
            rich_text: Some(rich),
        })
        .unwrap();
    editor
        .execute(Command::SetAnchor {
            id,
            frame: 0,
            x: 0.0,
            y: 0.0,
        })
        .unwrap();
    editor
        .execute(Command::SetPosition {
            id,
            frame: 0,
            x: 120.0,
            y: 90.0,
        })
        .unwrap();
    let native = project_file::encode(editor.project(), None).unwrap();
    let project = project_file::decode(&native).unwrap().project;
    let renderer = rendering::Renderer::new();
    let actual = renderer.render(&project, 0, 320).unwrap();
    let reference = raster(&literal(true, false, "middle"));
    same_pixels(&actual, &reference);
    same_pixels(&actual, &renderer.render_preview(&project, 0, 320).unwrap());
    same_pixels(
        &actual,
        &renderer.render_output(&project, 0, 320, 200).unwrap(),
    );
    assert_eq!(native, project_file::encode(&project, None).unwrap());
    let mut wrong: serde_json::Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
    wrong["composition"]["layers"][0]["rich_text"]["positioning"]["lines"][0]["font_sha256"] =
        "0".repeat(64).into();
    let wrong = Project::from_json(&serde_json::to_string(&wrong).unwrap()).unwrap();
    for result in [
        renderer.render(&wrong, 0, 320),
        renderer.render_preview(&wrong, 0, 320),
        renderer.render_output(&wrong, 0, 320, 200),
    ] {
        assert!(result.unwrap_err().contains("SHA-256"));
    }
    if let Some(output) = output {
        std::fs::create_dir_all(output).unwrap();
        std::fs::write(output.join("synthetic.lep"), native).unwrap();
        std::fs::write(
            output.join("reference.svg"),
            rendering::svg_document(
                &format!(
                    "<g transform='translate(120 90)'>{}</g>",
                    literal(true, false, "middle")
                ),
                320.0,
                200.0,
            )
            .unwrap(),
        )
        .unwrap();
        actual.save(output.join("actual.png")).unwrap();
        reference.save(output.join("reference.png")).unwrap();
    }
    println!(
        "PASS native save/reopen and production render/preview/export equal independent literal SVG; wrong font rejected on all three paths"
    );
}

pub(crate) fn paint_overhang_keeps_independent_caret() {
    let style = TextStyle::default();
    let mut character = TextCharacterStyle::from_style(&style, 32.0, 0xff3322);
    character.font_face = "WantedSans-Regular".into();
    character.leading = Some(TextLeading::Fixed(48.0));
    character.stroke_enabled = true;
    character.stroke_color = 0x11cc44;
    character.stroke_width = 20.0;
    character.stroke_join = TextStrokeJoin::Round;
    let mut rich = RichText::new(
        "W",
        character.clone(),
        vec![TextStyleRun {
            start: 0,
            end: 1,
            style: character.clone(),
        }],
    )
    .unwrap();
    rich.point_origin = true;
    rich.positioning = Some(AuthoredTextPositions {
        text: "W".into(),
        align: style.align,
        lines: vec![AuthoredTextLine {
            start: 0,
            end: 1,
            font_sha256: HASH.into(),
            font_index: 0,
            style: character,
            glyphs: glyphs("W", &[0.0]),
            end_x: 1.0,
        }],
    });
    let svg = rich_text_render::layer_svg("W", &rich, 320.0, &style, [100.0; 2], false)
        .unwrap()
        .0;
    let family = fonts::svg_family(&style);
    let expected = format!(
        "<text x='0' y='0' font-family='{family}' font-size='32' fill='none' stroke='#11cc44' stroke-width='20' stroke-linejoin='round'>W</text><text x='0' y='0' font-family='{family}' font-size='32' fill='#ff3322'>W</text>"
    );
    let pixels = raster(&svg);
    same_pixels(&pixels, &raster(&expected));
    let layout = text_edit::layout::Layout::shape_rich("W", &rich, 320.0, &style).unwrap();
    assert_eq!(layout.caret(1), [1.0, -32.0]);
    assert!(layout.bounds()[0] < 0.0);
    assert!(layout.bounds()[0] + layout.bounds()[2] > 20.0);
    let (x, y, _) = pixels
        .enumerate_pixels()
        .find(|(x, _, p)| *x > 140 && p[3] == 255)
        .unwrap();
    let point = [f64::from(x) - 120.0 + 0.5, f64::from(y) - 90.0 + 0.5];
    assert!(
        layout.contains(point),
        "Visible authored overhang cannot be picked"
    );
    assert_eq!(layout.hit_character(point), 0);
    assert_eq!(layout.hit(point), 1);
    println!(
        "PASS actual stroked ink extends selection bounds and remains pickable while the independent terminal caret stays at x=1"
    );
}

#[cfg(test)]
#[test]
fn authored_spacing_literal_pixels_and_carets() {
    literal_pixels_and_carets();
}
#[cfg(test)]
#[test]
fn authored_spacing_legacy_and_rejections() {
    legacy_and_rejections();
}
#[cfg(test)]
#[test]
fn authored_spacing_production_roundtrip() {
    production_roundtrip(None);
}

#[cfg(test)]
#[test]
fn authored_spacing_paint_overhang() {
    paint_overhang_keeps_independent_caret();
}
