//! Independent per-unit transform checks. Expectations use literal SVG affine
//! matrices and declared baselines, never the production animator or selector.
//! Only font shaping and rasterization are shared with the implementation.
use crate::rendering::{FrameRenderBudget, Renderer};
use libre_effects_core::*;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 224;
const SIZE: f64 = 24.;
const BOUNDS: [f64; 2] = [180., 160.];
const IDENTITY: [f64; 6] = [1., 0., 0., 1., 0., 0.];

fn scene(text: &str, paragraph: bool, bounds: [f64; 2]) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Text unit transform acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: text.into(),
                font_size: SIZE,
            },
            width: bounds[0],
            height: bounds[1],
            name: "Unit transforms".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                paragraph,
                leading: 1.25,
                ..Default::default()
            },
        })
        .unwrap();
    editor
        .execute(Command::SetColor {
            id: 1,
            color: 0x3876c8,
        })
        .unwrap();
    for (property, value) in [
        (Property::AnchorX, 0.),
        (Property::AnchorY, 0.),
        (Property::PositionX, 48.),
        (Property::PositionY, 40.),
    ] {
        editor
            .execute(Command::SetValue {
                id: 1,
                property,
                frame: 0,
                value,
            })
            .unwrap();
    }
    editor
}

fn set(editor: &mut Editor, parameter: TextParam, value: f64) {
    editor
        .execute(Command::EditText {
            id: 1,
            parameter,
            edit: TrackEdit::Value { frame: 0, value },
        })
        .unwrap();
}

fn select(editor: &mut Editor, units: TextSelectorUnits, shape: TextSelectorShape) {
    editor
        .execute(Command::SetTextSelector {
            id: 1,
            selector: TextSelector { units, shape },
        })
        .unwrap();
}

fn transform(editor: &mut Editor, scale: [f64; 2], rotation: f64, position: [f64; 2]) {
    for (parameter, value) in [
        (TextParam::AnimatorScaleX, scale[0]),
        (TextParam::AnimatorScaleY, scale[1]),
        (TextParam::AnimatorRotation, rotation),
        (TextParam::AnimatorPositionX, position[0]),
        (TextParam::AnimatorPositionY, position[1]),
    ] {
        set(editor, parameter, value);
    }
}

#[derive(Clone, Copy)]
struct Row<'a> {
    text: &'a str,
    y: f64,
    matrix: [f64; 6],
    opacity: f64,
}

fn row(text: &str, y: f64, matrix: [f64; 6]) -> Row<'_> {
    Row {
        text,
        y,
        matrix,
        opacity: 1.,
    }
}

fn literal_svg(paragraph: bool, bounds: [f64; 2], rows: &[Row<'_>]) -> String {
    let mut body = String::new();
    for row in rows {
        let baseline = if paragraph { SIZE } else { SIZE + row.y };
        let text = row
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let mut ink = format!(
            "<text x='0' y='{baseline}' text-anchor='start' letter-spacing='0' font-family='Wanted Sans' font-weight='400' font-style='normal' font-size='{SIZE}' fill='#3876c8' xml:space='preserve'>{text}</text>"
        );
        if paragraph {
            ink = format!("<g transform='translate(0 {})'>{ink}</g>", row.y);
        }
        if row.matrix != IDENTITY {
            let [a, b, c, d, e, f] = row.matrix;
            ink = format!("<g transform='matrix({a} {b} {c} {d} {e} {f})'>{ink}</g>");
        }
        if row.opacity != 1. {
            ink = format!("<g opacity='{}'>{ink}</g>", row.opacity);
        }
        body.push_str(&ink);
    }
    if paragraph {
        body = format!(
            "<svg width='{}' height='{}' overflow='hidden'>{body}</svg>",
            bounds[0], bounds[1]
        );
    }
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'><g transform='translate(48 40)'>{body}</g></svg>"
    )
}

fn literal(paragraph: bool, bounds: [f64; 2], rows: &[Row<'_>]) -> image::RgbaImage {
    raw_pixels(&literal_svg(paragraph, bounds, rows))
}

fn raw_pixels(svg: &str) -> image::RgbaImage {
    let tree = resvg::usvg::Tree::from_str(svg, &crate::fonts::render_options()).unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(WIDTH, HEIGHT).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    image::RgbaImage::from_raw(
        WIDTH,
        HEIGHT,
        pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let pixel = pixel.demultiply();
                [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
            })
            .collect(),
    )
    .unwrap()
}

fn assert_pixels(actual: &image::RgbaImage, expected: &image::RgbaImage, context: &str) {
    assert_eq!(actual.dimensions(), expected.dimensions(), "{context}");
    let differences: Vec<_> = actual
        .enumerate_pixels()
        .filter_map(|(x, y, pixel)| {
            let wanted = expected.get_pixel(x, y);
            (pixel != wanted).then_some((x, y, pixel.0, wanted.0))
        })
        .take(4)
        .collect();
    assert!(differences.is_empty(), "{context}: {differences:?}");
}

fn assert_paths(project: &Project, frame: Frame, expected: &image::RgbaImage) {
    let renderer = Renderer::new();
    for (name, image) in [
        ("shared", renderer.render(project, frame, WIDTH).unwrap()),
        (
            "preview",
            renderer.render_preview(project, frame, WIDTH).unwrap(),
        ),
        (
            "output",
            renderer
                .render_output(project, frame, WIDTH, HEIGHT)
                .unwrap(),
        ),
    ] {
        assert_pixels(&image, expected, &format!("{name}, frame {frame}"));
    }
}

fn isolated(project: &Project) -> String {
    Renderer::new()
        .isolated_layer_svg(
            project,
            project.active_composition_id(),
            1,
            0,
            WIDTH,
            "unit-transform-identity",
            &mut FrameRenderBudget::default(),
        )
        .unwrap()
}

#[test]
fn unit_transform_identity_and_zero_amount_keep_exact_legacy_svg() {
    for paragraph in [false, true] {
        let baseline = scene("AB CD\nEF", paragraph, [42., 160.]);
        let svg = isolated(baseline.project());
        let expected = Renderer::new()
            .render(baseline.project(), 0, WIDTH)
            .unwrap();
        for units in [
            TextSelectorUnits::Graphemes,
            TextSelectorUnits::Words,
            TextSelectorUnits::Lines,
        ] {
            let mut editor = Editor::default();
            editor.replace_project(baseline.project().clone()).unwrap();
            select(&mut editor, units, TextSelectorShape::Triangle);
            transform(&mut editor, [100., 100.], 0., [0., 0.]);
            assert_eq!(isolated(editor.project()), svg);
            assert_paths(editor.project(), 0, &expected);
            transform(&mut editor, [0., 230.], 137., [32., 16.]);
            set(&mut editor, TextParam::AnimatorOpacity, 20.);
            set(&mut editor, TextParam::AnimatorAmount, 0.);
            assert_eq!(isolated(editor.project()), svg);
            assert_paths(editor.project(), 0, &expected);
        }
    }
}

#[test]
fn unit_transform_words_use_independent_baseline_pivots_instead_of_layer_origin() {
    for paragraph in [false, true] {
        let mut editor = scene("AB\nCD", paragraph, BOUNDS);
        select(
            &mut editor,
            TextSelectorUnits::Words,
            TextSelectorShape::Square,
        );
        transform(&mut editor, [150., 50.], 90., [0., 0.]);
        // R(90) * S(1.5, .5), about (0, 24) and (0, 54).
        let expected = literal(
            paragraph,
            BOUNDS,
            &[
                row("AB", 0., [0., 1.5, -0.5, 0., 12., 24.]),
                row("CD", 30., [0., 1.5, -0.5, 0., 27., 54.]),
            ],
        );
        let layer_origin = literal(
            paragraph,
            BOUNDS,
            &[
                row("AB", 0., [0., 1.5, -0.5, 0., 0., 0.]),
                row("CD", 30., [0., 1.5, -0.5, 0., 0., 0.]),
            ],
        );
        assert_ne!(
            expected, layer_origin,
            "the oracle distinguishes layer motion"
        );
        assert_paths(editor.project(), 0, &expected);
    }
}

#[test]
fn unit_transform_characters_protect_graphemes_and_bundled_multigrapheme_ligature() {
    // Wanted Sans has a shipped liga mapping all 11 ASCII source graphemes
    // of wanted_logo to one glyph. Verify the independent raw shaping metadata.
    fn ranges(group: &resvg::usvg::Group, out: &mut Vec<std::ops::Range<usize>>) {
        for node in group.children() {
            match node {
                resvg::usvg::Node::Text(text) => {
                    out.extend(text.layouted().iter().flat_map(|span| {
                        span.positioned_glyphs
                            .iter()
                            .map(|glyph| glyph.source_range.clone())
                    }))
                }
                resvg::usvg::Node::Group(group) => ranges(group, out),
                _ => {}
            }
        }
    }
    let svg = literal_svg(false, BOUNDS, &[row("wanted_logo", 0., IDENTITY)]);
    let tree = resvg::usvg::Tree::from_str(&svg, &crate::fonts::render_options()).unwrap();
    let mut actual_ranges = vec![];
    ranges(tree.root(), &mut actual_ranges);
    assert_eq!(actual_ranges, [0..11]);
    for text in ["e\u{301}", "\u{1100}\u{1161}\u{11a8}", "wanted_logo"] {
        for paragraph in [false, true] {
            let mut editor = scene(text, paragraph, BOUNDS);
            // Select only the center source grapheme, not all 11 logo characters.
            set(&mut editor, TextParam::AnimatorStart, 45.);
            set(&mut editor, TextParam::AnimatorEnd, 55.);
            transform(&mut editor, [150., 50.], -90., [8., 4.]);
            set(&mut editor, TextParam::AnimatorOpacity, 60.);
            let mut expected = row(text, 0., [0., -1.5, 0.5, 0., -4., 28.]);
            expected.opacity = 0.6;
            assert_paths(
                editor.project(),
                0,
                &literal(paragraph, BOUNDS, &[expected]),
            );
        }
    }
}

#[test]
fn unit_transform_amount_weights_scale_rotation_and_position_in_that_order() {
    // A single unit's center is 50. RampUp over [40,80) gives 1/4,
    // multiplied by Amount50 to 1/8. Square with Amount50 gives 1/2.
    for (shape, scale, rotation, matrix, opacity) in [
        (
            TextSelectorShape::RampUp,
            [300., 500.],
            720.,
            [0., 1.25, -1.5, 0., 44., 28.],
            0.9,
        ),
        (
            TextSelectorShape::Square,
            [300., 0.],
            180.,
            [0., 2., -0.5, 0., 44., 40.],
            0.6,
        ),
    ] {
        for paragraph in [false, true] {
            let mut editor = scene("A", paragraph, BOUNDS);
            select(&mut editor, TextSelectorUnits::Graphemes, shape);
            transform(&mut editor, scale, rotation, [64., 32.]);
            set(&mut editor, TextParam::AnimatorStart, 40.);
            set(&mut editor, TextParam::AnimatorEnd, 80.);
            set(&mut editor, TextParam::AnimatorAmount, 50.);
            set(&mut editor, TextParam::AnimatorOpacity, 20.);
            let mut expected = row("A", 0., matrix);
            expected.opacity = opacity;
            assert_paths(
                editor.project(),
                0,
                &literal(paragraph, BOUNDS, &[expected]),
            );
        }
    }
}

#[test]
fn unit_transform_source_and_animated_channels_recompute_after_json_and_lep() {
    let mut editor = scene("AB\nCD", false, BOUNDS);
    select(
        &mut editor,
        TextSelectorUnits::Words,
        TextSelectorShape::Square,
    );
    set(&mut editor, TextParam::AnimatorStart, 40.);
    set(&mut editor, TextParam::AnimatorEnd, 60.);
    for (parameter, value) in [
        (TextParam::AnimatorScaleX, 300.),
        (TextParam::AnimatorScaleY, 0.),
        (TextParam::AnimatorRotation, 180.),
    ] {
        for edit in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value { frame: 60, value },
        ] {
            editor
                .execute(Command::EditText {
                    id: 1,
                    parameter,
                    edit,
                })
                .unwrap();
        }
    }
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    for (frame, text) in [(30, "AB\nCD\nEF"), (60, "")] {
        editor
            .execute(Command::EditSourceText {
                id: 1,
                frame,
                text: text.into(),
            })
            .unwrap();
    }
    let project = editor.project();
    let json = project.to_json().unwrap();
    let lep = project_file::encode(project, None).unwrap();
    let reopened = [
        Project::from_json(&json).unwrap(),
        project_file::decode(&lep).unwrap().project,
    ];
    for saved in &reopened {
        assert_eq!(saved, project);
    }
    for frame in [0, 29, 30, 60] {
        let rows = match frame {
            0 | 29 => vec![row("AB", 0., IDENTITY), row("CD", 30., IDENTITY)],
            30 => vec![
                row("AB", 0., IDENTITY),
                // At frame30: S(2,.5), R90 about CD's (0,54) baseline.
                row("CD", 30., [0., 2., -0.5, 0., 27., 54.]),
                row("EF", 60., IDENTITY),
            ],
            _ => vec![],
        };
        let expected = literal(false, BOUNDS, &rows);
        assert_paths(project, frame, &expected);
        for saved in &reopened {
            assert_paths(saved, frame, &expected);
        }
    }
    assert_eq!(project.to_json().unwrap(), json, "render remains read-only");
    assert_eq!(project_file::encode(project, None).unwrap(), lep);
}

#[test]
fn unit_transform_words_and_source_lines_share_one_pivot_across_visual_wraps() {
    let bounds = [20., 160.];
    for units in [TextSelectorUnits::Words, TextSelectorUnits::Lines] {
        let mut editor = scene("AAAA", true, bounds);
        select(&mut editor, units, TextSelectorShape::Square);
        transform(&mut editor, [100., 50.], 0., [0., 0.]);
        let layer = editor.project().composition().layer(1).unwrap();
        let flow = crate::text_flow::layer_lines(layer, 0).unwrap();
        assert_eq!(
            flow.len(),
            4,
            "the fixture must wrap one word into four rows"
        );
        assert_eq!(
            flow.iter()
                .map(|line| line.range.clone())
                .collect::<Vec<_>>(),
            [0..1, 1..2, 2..3, 3..4]
        );
        let expected = literal(
            true,
            bounds,
            &[
                row("A", 0., [1., 0., 0., 0.5, 0., 12.]),
                row("A", 30., [1., 0., 0., 0.5, 0., 12.]),
                row("A", 60., [1., 0., 0., 0.5, 0., 12.]),
                row("A", 90., [1., 0., 0., 0.5, 0., 12.]),
            ],
        );
        assert_paths(editor.project(), 0, &expected);
        // Character pivots stay on each source baseline instead. This catches
        // accidentally implementing every unit kind as independent glyph motion.
        select(
            &mut editor,
            TextSelectorUnits::Graphemes,
            TextSelectorShape::Square,
        );
        let characters = literal(
            true,
            bounds,
            &[
                row("A", 0., [1., 0., 0., 0.5, 0., 12.]),
                row("A", 30., [1., 0., 0., 0.5, 0., 27.]),
                row("A", 60., [1., 0., 0., 0.5, 0., 42.]),
                row("A", 90., [1., 0., 0., 0.5, 0., 57.]),
            ],
        );
        assert_ne!(expected, characters);
        assert_paths(editor.project(), 0, &characters);
    }
}

#[test]
fn unit_transform_keeps_paragraph_clip_overflow_and_caret_layout_unchanged() {
    let bounds = [80., 60.];
    let mut editor = scene("A\nB\nC", true, bounds);
    let before = editor.project().clone();
    let original = before.composition().layer(1).unwrap();
    let flow = crate::text_flow::layer_lines(original, 0).unwrap();
    let layout = crate::text_edit::layout::Layout::for_layer(original, 0).unwrap();
    let fit = crate::text_flow::fit_height(original, 0);
    assert_eq!(crate::text_flow::composed_count(&flow, bounds[1]), 2);
    transform(&mut editor, [200., 50.], 180., [30., -30.]);
    let changed = editor.project().composition().layer(1).unwrap();
    assert_eq!(crate::text_flow::fit_height(changed, 0), fit);
    assert_eq!(
        format!("{:?}", crate::text_flow::layer_lines(changed, 0).unwrap()),
        format!("{flow:?}")
    );
    assert_eq!(
        format!(
            "{:?}",
            crate::text_edit::layout::Layout::for_layer(changed, 0).unwrap()
        ),
        format!("{layout:?}"),
        "caret and hit cells use source layout, not transformed ink"
    );
    // Scale and rotate around each baseline, then move up30. C would return
    // into the viewport if post-animation geometry were wrongly recomposed.
    let rows = [
        row("A", 0., [-2., 0., 0., -0.5, 30., 6.]),
        row("B", 30., [-2., 0., 0., -0.5, 30., 51.]),
    ];
    let expected = literal(true, bounds, &rows);
    assert_ne!(
        expected,
        literal(false, bounds, &rows),
        "box clipping is exercised"
    );
    let mut resurrected = rows.to_vec();
    resurrected.push(row("C", 60., [-2., 0., 0., -0.5, 30., 96.]));
    assert_ne!(expected, literal(true, bounds, &resurrected));
    assert_paths(editor.project(), 0, &expected);
}

#[test]
fn unit_transform_zero_scale_collapses_selected_ink_without_reflection() {
    for paragraph in [false, true] {
        for scale in [[0., 100.], [100., 0.], [0., 0.]] {
            let mut editor = scene("A\nB", paragraph, BOUNDS);
            select(
                &mut editor,
                TextSelectorUnits::Words,
                TextSelectorShape::Square,
            );
            set(&mut editor, TextParam::AnimatorEnd, 50.);
            transform(&mut editor, scale, 90., [20., 10.]);
            // Only B survives. A must disappear without failing serialization,
            // being reflected, or collapsing the surrounding unselected pass.
            let expected = literal(paragraph, BOUNDS, &[row("B", 30., IDENTITY)]);
            assert_paths(editor.project(), 0, &expected);
        }
    }
}

#[test]
fn unit_transform_nonuniform_scale_transforms_authored_stroke_with_complete_ink() {
    for paragraph in [false, true] {
        let mut editor = scene("AB\nCD", paragraph, BOUNDS);
        editor
            .execute(Command::SetTextStyle {
                id: 1,
                style: TextStyle {
                    paragraph,
                    leading: 1.25,
                    fill_enabled: false,
                    stroke_enabled: true,
                    stroke_color: 0xb84090,
                    stroke_width: 6.,
                    stroke_join: TextStrokeJoin::Round,
                    ..Default::default()
                },
            })
            .unwrap();
        select(
            &mut editor,
            TextSelectorUnits::Words,
            TextSelectorShape::Square,
        );
        transform(&mut editor, [200., 50.], 90., [0., 0.]);
        // The explicit affine surrounds authored stroke, so its thickness is
        // scaled nonuniformly along with the outlines. Scaling just glyph
        // coordinates and repainting an unscaled stroke cannot match this.
        let svg = literal_svg(
            paragraph,
            BOUNDS,
            &[
                row("AB", 0., [0., 2., -0.5, 0., 12., 24.]),
                row("CD", 30., [0., 2., -0.5, 0., 27., 54.]),
            ],
        )
        .replace(
            "fill='#3876c8'",
            "fill='none' stroke='#b84090' stroke-width='6' stroke-linejoin='round' stroke-miterlimit='4'",
        );
        assert_paths(editor.project(), 0, &raw_pixels(&svg));
    }
}
