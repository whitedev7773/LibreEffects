//! Focused ordered-selector acceptance. Literal rows declare expected weights,
//! baselines, motion and opacity without production selection or source mapping.
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
            name: "Ordered text selector acceptance".into(),
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

fn add(editor: &mut Editor, mut selector: TextRangeSelector) -> u64 {
    editor
        .execute(Command::AddTextRangeSelector { id: 1 })
        .unwrap();
    let id = editor
        .selected_layer()
        .unwrap()
        .text_range_selectors()
        .last()
        .unwrap()
        .id;
    selector.id = id;
    editor
        .execute(Command::SetTextRangeSelector { id: 1, selector })
        .unwrap();
    id
}

fn secondary(mode: TextSelectorMode, amount: f64) -> TextRangeSelector {
    TextRangeSelector {
        mode,
        amount,
        ..TextRangeSelector::new(1)
    }
}

#[test]
fn selector_stack_order_and_per_step_clamping_match_literal_pixels() {
    use TextSelectorMode::{Add, Intersect, Subtract};
    for paragraph in [false, true] {
        for (amount, first, second, position, opacity) in [
            (
                75.,
                secondary(Add, 75.),
                secondary(Subtract, 50.),
                [16., 8.],
                60.,
            ),
            (
                75.,
                secondary(Subtract, 50.),
                secondary(Add, 75.),
                [32., 16.],
                20.,
            ),
            (
                50.,
                secondary(Add, 50.),
                secondary(Intersect, 50.),
                [16., 8.],
                60.,
            ),
            (
                50.,
                secondary(Intersect, 50.),
                secondary(Add, 50.),
                [24., 12.],
                40.,
            ),
        ] {
            let mut editor = scene("A", paragraph, BOUNDS[0]);
            select(
                &mut editor,
                TextSelectorUnits::Graphemes,
                TextSelectorShape::Square,
                [0., 100.],
            );
            set(&mut editor, TextParam::AnimatorAmount, amount);
            add(&mut editor, first);
            add(&mut editor, second);
            assert_paths(
                editor.project(),
                0,
                &literal(paragraph, &[("A", 0., position, opacity)]),
            );
        }
    }
}

#[test]
fn selector_stack_empty_primary_add_offset_shapes_match_literal_pixels() {
    // The authored [20,60] plus Offset 20 becomes [40,80]. The sole source
    // unit has t=1/4; each listed weight already includes secondary Amount 50.
    for paragraph in [false, true] {
        for (shape, position, opacity) in [
            (TextSelectorShape::Square, [16., 8.], 60.),
            (TextSelectorShape::RampUp, [4., 2.], 90.),
            (TextSelectorShape::RampDown, [12., 6.], 70.),
            (TextSelectorShape::Triangle, [8., 4.], 80.),
        ] {
            let mut editor = scene("A", paragraph, BOUNDS[0]);
            select(
                &mut editor,
                TextSelectorUnits::Graphemes,
                TextSelectorShape::Square,
                [80., 20.],
            );
            add(
                &mut editor,
                TextRangeSelector {
                    start: 20.,
                    end: 60.,
                    offset: 20.,
                    amount: 50.,
                    selector: TextSelector {
                        units: TextSelectorUnits::Graphemes,
                        shape,
                    },
                    ..TextRangeSelector::new(1)
                },
            );
            assert_paths(
                editor.project(),
                0,
                &literal(paragraph, &[("A", 0., position, opacity)]),
            );
        }
    }
}

#[test]
fn selector_stack_mixed_word_line_and_grapheme_units_match_literal_pixels() {
    for paragraph in [false, true] {
        let mut editor = scene("AB\n!\nCD", paragraph, BOUNDS[0]);
        select(
            &mut editor,
            TextSelectorUnits::Graphemes,
            TextSelectorShape::Square,
            [0., 100.],
        );
        set(&mut editor, TextParam::AnimatorAmount, 25.);
        add(
            &mut editor,
            TextRangeSelector {
                amount: 50.,
                selector: TextSelector {
                    units: TextSelectorUnits::Words,
                    shape: TextSelectorShape::Triangle,
                },
                ..TextRangeSelector::new(1)
            },
        );
        add(
            &mut editor,
            TextRangeSelector {
                mode: TextSelectorMode::Intersect,
                end: 66.,
                selector: TextSelector {
                    units: TextSelectorUnits::Lines,
                    shape: TextSelectorShape::Square,
                },
                ..TextRangeSelector::new(1)
            },
        );
        // Word ramp contributes 1/4 to AB and CD, but not ! or LF. The final
        // hard-line intersection keeps AB and ! and removes the last line.
        assert_paths(
            editor.project(),
            0,
            &literal(
                paragraph,
                &[
                    ("AB", 0., [16., 8.], 60.),
                    ("!", 30., [8., 4.], 80.),
                    ("CD", 60., [0., 0.], 100.),
                ],
            ),
        );
    }
}

#[test]
fn selector_stack_preserves_graphemes_ligatures_rtl_and_source_only_identity() {
    for grapheme in ["e\u{301}", "한", "👩‍💻", "🇰🇷"] {
        let source = format!("A\n{grapheme}\nB");
        let mut editor = scene(&source, false, BOUNDS[0]);
        select(
            &mut editor,
            TextSelectorUnits::Graphemes,
            TextSelectorShape::Square,
            [0., 100.],
        );
        set(&mut editor, TextParam::AnimatorAmount, 0.);
        add(
            &mut editor,
            TextRangeSelector {
                start: 40.,
                end: 60.,
                amount: 50.,
                ..TextRangeSelector::new(1)
            },
        );
        assert_paths(
            editor.project(),
            0,
            &literal(
                false,
                &[
                    ("A", 0., [0., 0.], 100.),
                    (grapheme, 30., [16., 8.], 60.),
                    ("B", 60., [0., 0.], 100.),
                ],
            ),
        );
    }
    // The primary f and secondary i touch the same ffi ligature. Combining
    // before protection gives an empty intersection, not a selected ligature.
    let baseline = scene("office", false, BOUNDS[0]);
    let mut editor = scene("office", false, BOUNDS[0]);
    select(
        &mut editor,
        TextSelectorUnits::Graphemes,
        TextSelectorShape::Square,
        [16., 34.],
    );
    add(
        &mut editor,
        TextRangeSelector {
            mode: TextSelectorMode::Intersect,
            start: 50.,
            end: 67.,
            ..TextRangeSelector::new(1)
        },
    );
    assert_eq!(isolated(editor.project()), isolated(baseline.project()));
    assert_paths(
        editor.project(),
        0,
        &Renderer::new()
            .render(baseline.project(), 0, WIDTH)
            .unwrap(),
    );

    // Declared source range matches the legacy hard-selector path for actual
    // ligature and RTL shaping; neither expectation uses secondary evaluation.
    for (text, range) in [("office", [16., 34.]), ("אבג", [0., 34.])] {
        let mut expected = scene(text, false, BOUNDS[0]);
        select(
            &mut expected,
            TextSelectorUnits::Graphemes,
            TextSelectorShape::Square,
            range,
        );
        set(&mut expected, TextParam::AnimatorAmount, 50.);
        let mut actual = scene(text, false, BOUNDS[0]);
        select(
            &mut actual,
            TextSelectorUnits::Graphemes,
            TextSelectorShape::Square,
            [0., 100.],
        );
        set(&mut actual, TextParam::AnimatorAmount, 0.);
        add(
            &mut actual,
            TextRangeSelector {
                start: range[0],
                end: range[1],
                amount: 50.,
                ..TextRangeSelector::new(1)
            },
        );
        assert_paths(
            actual.project(),
            0,
            &Renderer::new()
                .render(expected.project(), 0, WIDTH)
                .unwrap(),
        );
    }
    let baseline = scene("A\r\nB\n", true, BOUNDS[0]);
    let mut editor = scene("A\r\nB\n", true, BOUNDS[0]);
    select(
        &mut editor,
        TextSelectorUnits::Graphemes,
        TextSelectorShape::Square,
        [0., 0.],
    );
    add(
        &mut editor,
        TextRangeSelector {
            start: 25.,
            end: 50.,
            ..TextRangeSelector::new(1)
        },
    );
    assert_eq!(
        isolated(editor.project()),
        isolated(baseline.project()),
        "unpainted CRLF affects no neighboring glyph"
    );
}

#[test]
fn selector_stack_secondary_units_keep_primary_pivots_across_visual_wraps() {
    // Source CD is the later visual wrap of the first hard line. Selecting
    // only C must transform the whole primary line around its original pivot.
    for (paragraph, width) in [(false, BOUNDS[0]), (true, 42.)] {
        for primary_units in [TextSelectorUnits::Words, TextSelectorUnits::Lines] {
            let text = if primary_units == TextSelectorUnits::Words {
                "ABCD\nEF"
            } else {
                "AB CD\nEF"
            };
            let range = if primary_units == TextSelectorUnits::Words {
                [29., 43.]
            } else {
                [37.5, 50.]
            };
            let mut actual = scene(text, paragraph, width);
            let original_layer = actual.selected_layer().unwrap();
            let original_flow = format!("{:?}", crate::text_flow::layer_lines(original_layer, 0));
            let original_fit = crate::text_flow::fit_height(original_layer, 0);
            let original_layout = format!(
                "{:?}",
                crate::text_edit::layout::Layout::for_layer(original_layer, 0)
            );
            select(
                &mut actual,
                primary_units,
                TextSelectorShape::Square,
                [0., 0.],
            );
            add(
                &mut actual,
                TextRangeSelector {
                    start: range[0],
                    end: range[1],
                    ..TextRangeSelector::new(1)
                },
            );
            let mut expected = scene(text, paragraph, width);
            select(
                &mut expected,
                primary_units,
                TextSelectorShape::Square,
                [0., 50.],
            );
            for editor in [&mut actual, &mut expected] {
                set(editor, TextParam::AnimatorPositionX, 0.);
                set(editor, TextParam::AnimatorPositionY, 0.);
                set(editor, TextParam::AnimatorOpacity, 100.);
                set(editor, TextParam::AnimatorScaleX, 140.);
                set(editor, TextParam::AnimatorScaleY, 60.);
                set(editor, TextParam::AnimatorRotation, 20.);
            }
            let image = Renderer::new()
                .render(expected.project(), 0, WIDTH)
                .unwrap();
            assert_paths(actual.project(), 0, &image);
            let layer = actual.selected_layer().unwrap();
            assert_eq!(layer.source_text_at(0), Some(text));
            assert_eq!(
                format!("{:?}", crate::text_flow::layer_lines(layer, 0)),
                original_flow
            );
            assert_eq!(crate::text_flow::fit_height(layer, 0), original_fit);
            assert_eq!(
                format!(
                    "{:?}",
                    crate::text_edit::layout::Layout::for_layer(layer, 0)
                ),
                original_layout,
                "selection leaves caret and hit-test layout unchanged"
            );
            if paragraph {
                let layer = actual.selected_layer().unwrap();
                assert!(
                    crate::text_flow::layer_lines(layer, 0).unwrap().len() >= 3,
                    "first authored unit wraps"
                );
            }
        }
    }
}

#[test]
fn selector_stack_roundtrips_resegment_animated_source_without_mutation() {
    let mut editor = scene("A\nB", false, BOUNDS[0]);
    select(
        &mut editor,
        TextSelectorUnits::Graphemes,
        TextSelectorShape::Square,
        [0., 0.],
    );
    add(
        &mut editor,
        TextRangeSelector {
            amount: 50.,
            start: 40.,
            end: 60.,
            selector: TextSelector {
                units: TextSelectorUnits::Words,
                shape: TextSelectorShape::Square,
            },
            ..TextRangeSelector::new(1)
        },
    );
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 30,
            text: "A\nB\nC".into(),
        })
        .unwrap();
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 60,
            text: "".into(),
        })
        .unwrap();
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
    for frame in [0, 29, 30, 59, 60] {
        let rows = if frame < 30 {
            vec![("A", 0., [0., 0.], 100.), ("B", 30., [0., 0.], 100.)]
        } else if frame < 60 {
            vec![
                ("A", 0., [0., 0.], 100.),
                ("B", 30., [16., 8.], 60.),
                ("C", 60., [0., 0.], 100.),
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
    assert_eq!(
        project.to_json().unwrap(),
        json,
        "render leaves authored source unchanged"
    );
    assert_eq!(project_file::encode(project, None).unwrap(), lep);
}

fn animate_secondary(
    editor: &mut Editor,
    selector: u64,
    parameter: TextSelectorParam,
    from: f64,
    to: f64,
) {
    let property = PropertyPath::TextSelector {
        selector,
        parameter,
    };
    for edit in [
        TrackEdit::Value {
            frame: 0,
            value: from,
        },
        TrackEdit::ToggleAnimation { frame: 0 },
        TrackEdit::Value {
            frame: 60,
            value: to,
        },
    ] {
        editor
            .execute(Command::EditTrack {
                id: 1,
                property,
                edit,
            })
            .unwrap();
    }
}

#[test]
fn animated_secondary_channels_match_literal_current_frame_pixels() {
    for paragraph in [false, true] {
        for (parameter, from, to, weights) in [
            (TextSelectorParam::Start, 0., 100., [1., 1., 1., 0., 0.]),
            (TextSelectorParam::End, 0., 100., [0., 0., 0., 1., 1.]),
            (TextSelectorParam::Offset, -100., 100., [0., 0., 1., 1., 0.]),
            (
                TextSelectorParam::Amount,
                0.,
                100.,
                [0., 0.25, 0.5, 0.75, 1.],
            ),
        ] {
            let mut editor = scene("A", paragraph, BOUNDS[0]);
            select(
                &mut editor,
                TextSelectorUnits::Graphemes,
                TextSelectorShape::Square,
                [0., 0.],
            );
            let selector = add(&mut editor, TextRangeSelector::new(1));
            animate_secondary(&mut editor, selector, parameter, from, to);
            let source = editor.project().to_json().unwrap();
            for (frame, weight) in [0, 15, 30, 45, 60].into_iter().zip(weights) {
                assert_paths(
                    editor.project(),
                    frame,
                    &literal(
                        paragraph,
                        &[("A", 0., [32. * weight, 16. * weight], 100. - 80. * weight)],
                    ),
                );
            }
            assert_eq!(editor.project().to_json().unwrap(), source);
        }
    }
}

#[test]
fn animated_secondary_reorder_remove_undo_keeps_track_identity_and_pixels() {
    let mut editor = scene("A", false, BOUNDS[0]);
    select(
        &mut editor,
        TextSelectorUnits::Graphemes,
        TextSelectorShape::Square,
        [0., 100.],
    );
    set(&mut editor, TextParam::AnimatorAmount, 75.);
    let animated = add(&mut editor, secondary(TextSelectorMode::Add, 0.));
    let subtract = add(&mut editor, secondary(TextSelectorMode::Subtract, 50.));
    animate_secondary(&mut editor, animated, TextSelectorParam::Amount, 0., 100.);
    let original = editor.project().clone();
    assert_paths(
        editor.project(),
        30,
        &literal(false, &[("A", 0., [16., 8.], 60.)]),
    );
    editor
        .execute(Command::MoveTextRangeSelector {
            id: 1,
            selector: subtract,
            index: 0,
        })
        .unwrap();
    let reordered = editor.project().clone();
    assert_paths(
        editor.project(),
        30,
        &literal(false, &[("A", 0., [24., 12.], 40.)]),
    );
    editor.undo();
    assert_eq!(editor.project(), &original);
    editor.redo();
    assert_eq!(editor.project(), &reordered);
    editor
        .execute(Command::RemoveTextRangeSelector {
            id: 1,
            selector: animated,
        })
        .unwrap();
    assert_paths(
        editor.project(),
        30,
        &literal(false, &[("A", 0., [8., 4.], 80.)]),
    );
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .track(PropertyPath::TextSelector {
                selector: animated,
                parameter: TextSelectorParam::Amount
            })
            .is_none()
    );
    editor.undo();
    assert_eq!(editor.project(), &reordered);
    assert_paths(
        editor.project(),
        30,
        &literal(false, &[("A", 0., [24., 12.], 40.)]),
    );
    let json = editor.project().to_json().unwrap();
    let lep = project_file::encode(editor.project(), None).unwrap();
    for reopened in [
        Project::from_json(&json).unwrap(),
        project_file::decode(&lep).unwrap().project,
    ] {
        assert_eq!(&reopened, editor.project());
        assert_paths(
            &reopened,
            30,
            &literal(false, &[("A", 0., [24., 12.], 40.)]),
        );
    }
}

#[test]
fn animated_secondary_and_source_text_resample_unicode_without_layout_changes() {
    let mut editor = scene("A\ne\u{301}\nB", false, BOUNDS[0]);
    select(
        &mut editor,
        TextSelectorUnits::Graphemes,
        TextSelectorShape::Square,
        [0., 0.],
    );
    let selector = add(
        &mut editor,
        TextRangeSelector {
            start: 40.,
            end: 60.,
            ..TextRangeSelector::new(1)
        },
    );
    animate_secondary(&mut editor, selector, TextSelectorParam::Amount, 0., 100.);
    editor
        .execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::SourceText,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 30,
            text: "".into(),
        })
        .unwrap();
    editor
        .execute(Command::EditSourceText {
            id: 1,
            frame: 60,
            text: "👩‍💻".into(),
        })
        .unwrap();
    let source = editor.project().to_json().unwrap();
    assert_paths(
        editor.project(),
        15,
        &literal(
            false,
            &[
                ("A", 0., [0., 0.], 100.),
                ("e\u{301}", 30., [8., 4.], 80.),
                ("B", 60., [0., 0.], 100.),
            ],
        ),
    );
    assert_paths(editor.project(), 30, &literal(false, &[]));
    assert_paths(
        editor.project(),
        60,
        &literal(false, &[("👩‍💻", 0., [32., 16.], 20.)]),
    );
    for frame in [15, 30, 60] {
        let layer = editor.selected_layer().unwrap();
        let text = layer.source_text_at(frame).unwrap();
        let baseline = scene(text, false, BOUNDS[0]);
        let original = baseline.selected_layer().unwrap();
        assert_eq!(
            format!("{:?}", crate::text_flow::layer_lines(layer, frame)),
            format!("{:?}", crate::text_flow::layer_lines(original, 0))
        );
        assert_eq!(
            format!(
                "{:?}",
                crate::text_edit::layout::Layout::for_layer(layer, frame)
            ),
            format!(
                "{:?}",
                crate::text_edit::layout::Layout::for_layer(original, 0)
            )
        );
    }
    assert_eq!(editor.project().to_json().unwrap(), source);
}

#[test]
fn animated_secondary_intersection_happens_before_ligature_protection() {
    let baseline = scene("office", false, BOUNDS[0]);
    let mut editor = scene("office", false, BOUNDS[0]);
    select(
        &mut editor,
        TextSelectorUnits::Graphemes,
        TextSelectorShape::Square,
        [16., 34.],
    );
    let selector = add(
        &mut editor,
        TextRangeSelector {
            mode: TextSelectorMode::Intersect,
            start: 50.,
            end: 67.,
            ..TextRangeSelector::new(1)
        },
    );
    animate_secondary(&mut editor, selector, TextSelectorParam::Amount, 0., 100.);
    for frame in [0, 15, 30, 60] {
        assert_paths(
            editor.project(),
            frame,
            &Renderer::new()
                .render(baseline.project(), frame, WIDTH)
                .unwrap(),
        );
    }
}
