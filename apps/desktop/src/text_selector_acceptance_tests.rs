//! Focused selector integration checks. Literal glyph transforms are independent
//! expectations; only font shaping and rasterization are shared with production.
use crate::rendering::{FrameRenderBudget, Renderer};
use libre_effects_core::*;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 192;
const SIZE: f64 = 24.;
const BOUNDS: [f64; 2] = [180., 150.];

fn scene(text: &str, paragraph: bool, width: f64) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Text selector acceptance".into(),
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
            width,
            height: BOUNDS[1],
            name: "Selector".into(),
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
        (Property::PositionX, 24.),
        (Property::PositionY, 20.),
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

fn select(
    editor: &mut Editor,
    units: TextSelectorUnits,
    shape: TextSelectorShape,
    range: [f64; 2],
) {
    editor
        .execute(Command::SetTextSelector {
            id: 1,
            selector: TextSelector { units, shape },
        })
        .unwrap();
    for (parameter, value) in [
        (TextParam::AnimatorStart, range[0]),
        (TextParam::AnimatorEnd, range[1]),
        (TextParam::AnimatorPositionX, 32.),
        (TextParam::AnimatorPositionY, 16.),
        (TextParam::AnimatorOpacity, 20.),
    ] {
        set(editor, parameter, value);
    }
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
            "selector-neutral",
            &mut FrameRenderBudget::default(),
        )
        .unwrap()
}

// Rows explicitly supply baseline offsets, motion and opacity. No production
// selector, source segmentation or paragraph wrapping computes this oracle.
fn literal(paragraph: bool, rows: &[(&str, f64, [f64; 2], f64)]) -> image::RgbaImage {
    let mut body = String::new();
    for (text, y, delta, opacity) in rows {
        let baseline = if paragraph { SIZE } else { SIZE + y };
        let mut row = format!(
            "<text x='0' y='{baseline}' text-anchor='start' letter-spacing='0' font-family='Wanted Sans' font-weight='400' font-style='normal' font-size='{SIZE}' fill='#3876c8' xml:space='preserve'>{text}</text>"
        );
        if paragraph {
            row = format!("<g transform='translate(0 {y})'>{row}</g>");
        }
        if *delta != [0., 0.] {
            row = format!(
                "<g transform='translate({} {})'>{row}</g>",
                delta[0], delta[1]
            );
        }
        if *opacity != 100. {
            row = format!("<g opacity='{}'>{row}</g>", opacity / 100.);
        }
        body.push_str(&row);
    }
    if paragraph {
        body = format!(
            "<svg width='{}' height='{}' overflow='hidden'>{body}</svg>",
            BOUNDS[0], BOUNDS[1]
        );
    }
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'><g transform='translate(24 20)'>{body}</g></svg>"
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &crate::fonts::render_options()).unwrap();
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

#[test]
fn selector_zero_amount_preserves_exact_legacy_svg_and_pixels() {
    for paragraph in [false, true] {
        let baseline = scene("AB CD\nEF", paragraph, 42.);
        let svg = isolated(baseline.project());
        let expected = Renderer::new()
            .render(baseline.project(), 0, WIDTH)
            .unwrap();
        for units in [
            TextSelectorUnits::Graphemes,
            TextSelectorUnits::Words,
            TextSelectorUnits::Lines,
        ] {
            for shape in [
                TextSelectorShape::Square,
                TextSelectorShape::RampUp,
                TextSelectorShape::RampDown,
                TextSelectorShape::Triangle,
            ] {
                let mut editor = scene("AB CD\nEF", paragraph, 42.);
                select(&mut editor, units, shape, [0., 100.]);
                set(&mut editor, TextParam::AnimatorAmount, 0.);
                assert_eq!(isolated(editor.project()), svg, "{units:?}, {shape:?}");
                assert_paths(editor.project(), 0, &expected);
            }
        }
    }
}

#[test]
fn selector_shapes_and_amount_match_literal_single_glyph_motion_and_opacity() {
    // One source unit has center 50. In [40, 80), t is exactly 1/4.
    // At Amount 50, the four weights are 1/2, 1/8, 3/8 and 1/4.
    for paragraph in [false, true] {
        for (shape, position, opacity) in [
            (TextSelectorShape::Square, [16., 8.], 60.),
            (TextSelectorShape::RampUp, [4., 2.], 90.),
            (TextSelectorShape::RampDown, [12., 6.], 70.),
            (TextSelectorShape::Triangle, [8., 4.], 80.),
        ] {
            let mut editor = scene("A", paragraph, BOUNDS[0]);
            select(&mut editor, TextSelectorUnits::Graphemes, shape, [40., 80.]);
            set(&mut editor, TextParam::AnimatorAmount, 50.);
            assert_paths(
                editor.project(),
                0,
                &literal(paragraph, &[("A", 0., position, opacity)]),
            );
        }
    }
}

#[test]
fn selector_words_leave_punctuation_unselected_and_move_the_whole_word() {
    for paragraph in [false, true] {
        let mut editor = scene("AB\n!\nCD", paragraph, BOUNDS[0]);
        select(
            &mut editor,
            TextSelectorUnits::Words,
            TextSelectorShape::Square,
            [50., 100.],
        );
        assert_paths(
            editor.project(),
            0,
            &literal(
                paragraph,
                &[
                    ("AB", 0., [0., 0.], 100.),
                    ("!", 30., [0., 0.], 100.),
                    ("CD", 60., [32., 16.], 20.),
                ],
            ),
        );
    }
}

#[test]
fn selector_words_and_source_lines_do_not_resegment_at_visual_wraps() {
    // Literal source indices in "AB CD\nEF": CD is graphemes 3..5;
    // the first LF-delimited line is graphemes 0..5. Soft wrapping must
    // not change either unit selection. The existing hard selector is the
    // comparison path, so expected positioning never estimates glyph advances.
    for (paragraph, width) in [(false, BOUNDS[0]), (true, BOUNDS[0]), (true, 42.)] {
        for (units, range, grapheme_range) in [
            (TextSelectorUnits::Words, [45., 55.], [37.5, 62.5]),
            (TextSelectorUnits::Lines, [0., 50.], [0., 62.5]),
        ] {
            let mut editor = scene("AB CD\nEF", paragraph, width);
            let before = editor.project().clone();
            select(&mut editor, units, TextSelectorShape::Square, range);
            let mut expected = scene("AB CD\nEF", paragraph, width);
            select(
                &mut expected,
                TextSelectorUnits::Graphemes,
                TextSelectorShape::Square,
                grapheme_range,
            );
            let image = Renderer::new()
                .render(expected.project(), 0, WIDTH)
                .unwrap();
            assert_ne!(
                image,
                Renderer::new().render(&before, 0, WIDTH).unwrap(),
                "the test must exercise a visible selection"
            );
            assert_paths(editor.project(), 0, &image);
            if paragraph && width == 42. {
                let layer = editor.project().composition().layer(1).unwrap();
                let lines = crate::text_flow::layer_lines(layer, 0).unwrap();
                assert_eq!(lines.len(), 3, "one hard line wraps into two visual lines");
            }
        }
    }
}

#[test]
fn selector_lines_count_crlf_blank_and_trailing_empty_source_lines() {
    // Four source lines have centers 12.5, 37.5, 62.5 and 87.5.
    for (range, selected) in [([62.5, 87.5], true), ([87.5, 100.], false)] {
        let mut editor = scene("A\r\n\r\nB\n", true, BOUNDS[0]);
        select(
            &mut editor,
            TextSelectorUnits::Lines,
            TextSelectorShape::Square,
            range,
        );
        let (position, opacity) = if selected {
            ([32., 16.], 20.)
        } else {
            ([0., 0.], 100.)
        };
        assert_paths(
            editor.project(),
            0,
            &literal(
                true,
                &[("A", 0., [0., 0.], 100.), ("B", 60., position, opacity)],
            ),
        );
    }
}

#[test]
fn selector_animated_source_and_amount_resegment_after_json_and_lep_roundtrips() {
    for units in [TextSelectorUnits::Words, TextSelectorUnits::Lines] {
        let mut editor = scene("AB\nCD", false, BOUNDS[0]);
        select(&mut editor, units, TextSelectorShape::Triangle, [40., 60.]);
        set(&mut editor, TextParam::AnimatorAmount, 0.);
        for edit in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value {
                frame: 60,
                value: 100.,
            },
        ] {
            editor
                .execute(Command::EditText {
                    id: 1,
                    parameter: TextParam::AnimatorAmount,
                    edit,
                })
                .unwrap();
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
            assert_eq!(
                saved.composition().layer(1).unwrap().text_selector(),
                TextSelector {
                    units,
                    shape: TextSelectorShape::Triangle,
                }
            );
        }
        for frame in [0, 29, 30, 45, 60] {
            let rows = if frame < 30 {
                vec![("AB", 0., [0., 0.], 100.), ("CD", 30., [0., 0.], 100.)]
            } else if frame < 60 {
                let (position, opacity) = if frame == 30 {
                    ([16., 8.], 60.)
                } else {
                    ([24., 12.], 40.)
                };
                vec![
                    ("AB", 0., [0., 0.], 100.),
                    ("CD", 30., position, opacity),
                    ("EF", 60., [0., 0.], 100.),
                ]
            } else {
                vec![]
            };
            let expected = literal(false, &rows);
            assert_paths(project, frame, &expected);
            for saved in &reopened {
                assert_paths(saved, frame, &expected);
            }
        }
        assert_eq!(project.to_json().unwrap(), json, "render is read-only");
        assert_eq!(project_file::encode(project, None).unwrap(), lep);
    }
}
