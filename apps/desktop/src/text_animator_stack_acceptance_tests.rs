//! Ordered-animator pixels against literal transforms/weights. Expectations do
//! not use production selectors, animator composition, wrapping or pivot helpers.
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
            name: "Ordered animator acceptance".into(),
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
            name: "Text".into(),
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
fn set(editor: &mut Editor, animator: u64, parameter: TextParam, value: f64) {
    let property = if animator == 0 {
        PropertyPath::Text(parameter)
    } else {
        PropertyPath::TextAnimator {
            animator,
            parameter,
        }
    };
    editor
        .execute(Command::EditTrack {
            id: 1,
            property,
            edit: TrackEdit::Value { frame: 0, value },
        })
        .unwrap();
}
fn add(editor: &mut Editor) -> u64 {
    editor.execute(Command::AddTextAnimator { id: 1 }).unwrap();
    editor
        .selected_layer()
        .unwrap()
        .text_animators()
        .last()
        .unwrap()
        .id
}
fn selector(
    editor: &mut Editor,
    animator: u64,
    units: TextSelectorUnits,
    shape: TextSelectorShape,
) {
    let selector = TextSelector { units, shape };
    let command = if animator == 0 {
        Command::SetTextSelector { id: 1, selector }
    } else {
        Command::SetTextAnimatorSelector {
            id: 1,
            animator,
            selector,
        }
    };
    editor.execute(command).unwrap();
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
    for (name, actual) in [
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
        assert_pixels(&actual, expected, &format!("{name}, frame {frame}"));
    }
}
fn isolated(project: &Project, frame: Frame) -> String {
    Renderer::new()
        .isolated_layer_svg(
            project,
            project.active_composition_id(),
            1,
            frame,
            WIDTH,
            "ordered-animator",
            &mut FrameRenderBudget::default(),
        )
        .unwrap()
}
fn raster(svg: &str) -> image::RgbaImage {
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
                let p = pixel.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            })
            .collect(),
    )
    .unwrap()
}
fn literal(paragraph: bool, width: f64, rows: &[(&str, f64, [f64; 6], f64)]) -> image::RgbaImage {
    let mut body = String::new();
    for (text, y, matrix, opacity) in rows {
        let baseline = if paragraph { SIZE } else { SIZE + y };
        let mut row = format!(
            "<text x='0' y='{baseline}' text-anchor='start' letter-spacing='0' font-family='Wanted Sans' font-weight='400' font-style='normal' font-size='{SIZE}' fill='#3876c8' xml:space='preserve'>{text}</text>"
        );
        if paragraph {
            row = format!("<g transform='translate(0 {y})'>{row}</g>");
        }
        let [a, b, c, d, x, y] = matrix;
        body.push_str(&format!(
            "<g transform='matrix({a} {b} {c} {d} {x} {y})' opacity='{opacity}'>{row}</g>"
        ));
    }
    if paragraph {
        body = format!(
            "<svg width='{width}' height='{}' overflow='hidden'>{body}</svg>",
            BOUNDS[1]
        );
    }
    raster(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'><g transform='translate(48 40)'>{body}</g></svg>"
    ))
}
fn translation(x: f64, y: f64) -> [f64; 6] {
    [1., 0., 0., 1., x, y]
}

#[test]
fn animator_stack_noncommuting_order_matches_literal_matrices_and_undo() {
    for paragraph in [false, true] {
        let mut editor = scene("A", paragraph, BOUNDS[0]);
        set(&mut editor, 0, TextParam::AnimatorOpacity, 80.);
        let translate = add(&mut editor);
        set(&mut editor, translate, TextParam::AnimatorPositionX, 8.);
        set(&mut editor, translate, TextParam::AnimatorPositionY, 4.);
        let affine = add(&mut editor);
        set(&mut editor, affine, TextParam::AnimatorScaleX, 200.);
        set(&mut editor, affine, TextParam::AnimatorScaleY, 50.);
        set(&mut editor, affine, TextParam::AnimatorRotation, 90.);
        let before = editor.project().clone();
        let first = literal(
            paragraph,
            BOUNDS[0],
            &[("A", 0., [0., 2., -0.5, 0., 10., 40.], 0.8)],
        );
        assert_paths(editor.project(), 0, &first);
        editor
            .execute(Command::MoveTextAnimator {
                id: 1,
                animator: affine,
                index: 0,
            })
            .unwrap();
        let second = literal(
            paragraph,
            BOUNDS[0],
            &[("A", 0., [0., 2., -0.5, 0., 20., 28.], 0.8)],
        );
        assert_ne!(first, second, "order must be visibly noncommutative");
        assert_paths(editor.project(), 0, &second);
        editor.undo();
        assert_eq!(editor.project(), &before);
        assert_paths(editor.project(), 0, &first);
    }
}

#[test]
fn animator_stack_mixed_units_multiply_independent_opacity_weights() {
    for paragraph in [false, true] {
        let mut editor = scene("AB\n!\nCD", paragraph, BOUNDS[0]);
        set(&mut editor, 0, TextParam::AnimatorPositionX, 8.);
        set(&mut editor, 0, TextParam::AnimatorPositionY, 4.);
        set(&mut editor, 0, TextParam::AnimatorAmount, 50.);
        set(&mut editor, 0, TextParam::AnimatorOpacity, 50.);
        let words = add(&mut editor);
        selector(
            &mut editor,
            words,
            TextSelectorUnits::Words,
            TextSelectorShape::RampUp,
        );
        set(&mut editor, words, TextParam::AnimatorPositionX, 16.);
        set(&mut editor, words, TextParam::AnimatorOpacity, 0.);
        assert_paths(
            editor.project(),
            0,
            &literal(
                paragraph,
                BOUNDS[0],
                &[
                    ("AB", 0., translation(8., 2.), 0.5625),
                    ("!", 30., translation(4., 2.), 0.75),
                    ("CD", 60., translation(16., 2.), 0.1875),
                ],
            ),
        );
    }
}

#[test]
fn animator_stack_wraps_share_original_line_pivots_and_keep_layout() {
    let mut editor = scene("AB CD\nEF", true, 42.);
    let layer = editor.selected_layer().unwrap();
    let flow = format!("{:?}", crate::text_flow::layer_lines(layer, 0));
    let layout = format!(
        "{:?}",
        crate::text_edit::layout::Layout::for_layer(layer, 0)
    );
    set(&mut editor, 0, TextParam::AnimatorPositionX, 4.);
    let lines = add(&mut editor);
    selector(
        &mut editor,
        lines,
        TextSelectorUnits::Lines,
        TextSelectorShape::Square,
    );
    set(&mut editor, lines, TextParam::AnimatorScaleX, 50.);
    set(&mut editor, lines, TextParam::AnimatorScaleY, 50.);
    // First hard line spans rows at y=0 and y=30; both scale about (0,24).
    // Second hard line starts at y=60 and scales about its own (0,84).
    assert_paths(
        editor.project(),
        0,
        &literal(
            true,
            42.,
            &[
                ("AB", 0., [0.5, 0., 0., 0.5, 2., 12.], 1.),
                ("CD", 30., [0.5, 0., 0., 0.5, 2., 12.], 1.),
                ("EF", 60., [0.5, 0., 0., 0.5, 2., 42.], 1.),
            ],
        ),
    );
    let layer = editor.selected_layer().unwrap();
    assert_eq!(
        format!("{:?}", crate::text_flow::layer_lines(layer, 0)),
        flow
    );
    assert_eq!(
        format!(
            "{:?}",
            crate::text_edit::layout::Layout::for_layer(layer, 0)
        ),
        layout
    );
}

#[test]
fn animator_stack_protects_unicode_clusters_and_preserves_identity_extras_exactly() {
    for source in ["e\u{301}", "한", "👩‍💻", "🇰🇷", "ffi", "אבג"] {
        let mut editor = scene(source, false, BOUNDS[0]);
        set(&mut editor, 0, TextParam::AnimatorPositionX, 4.);
        let legacy = isolated(editor.project(), 0);
        let extra = add(&mut editor);
        assert_eq!(
            isolated(editor.project(), 0),
            legacy,
            "identity extra: {source}"
        );
        set(&mut editor, extra, TextParam::AnimatorPositionY, 8.);
        assert_paths(
            editor.project(),
            0,
            &literal(false, BOUNDS[0], &[(source, 0., translation(4., 8.), 1.)]),
        );
    }
    // Select separate source letters of the shipped logo ligature in
    // independent animators: both contribute to its motion after protection.
    let mut editor = scene("wanted_logo", false, BOUNDS[0]);
    set(&mut editor, 0, TextParam::AnimatorEnd, 5.);
    set(&mut editor, 0, TextParam::AnimatorPositionX, 4.);
    let extra = add(&mut editor);
    set(&mut editor, extra, TextParam::AnimatorStart, 95.);
    set(&mut editor, extra, TextParam::AnimatorPositionY, 8.);
    assert!(
        crate::text_animator_render::source_clusters(
            "wanted_logo",
            SIZE,
            BOUNDS[0],
            &TextStyle::default()
        )
        .unwrap()
        .iter()
        .any(|range| *range == (0..11))
    );
    assert_paths(
        editor.project(),
        0,
        &literal(
            false,
            BOUNDS[0],
            &[("wanted_logo", 0., translation(4., 8.), 1.)],
        ),
    );
}

#[test]
fn animator_stack_animated_samples_resegment_source_and_roundtrip_without_mutation() {
    let mut editor = scene("A\nB", false, BOUNDS[0]);
    set(&mut editor, 0, TextParam::AnimatorPositionY, 4.);
    let words = add(&mut editor);
    selector(
        &mut editor,
        words,
        TextSelectorUnits::Words,
        TextSelectorShape::Square,
    );
    set(&mut editor, words, TextParam::AnimatorStart, 40.);
    set(&mut editor, words, TextParam::AnimatorEnd, 60.);
    let property = PropertyPath::TextAnimator {
        animator: words,
        parameter: TextParam::AnimatorPositionX,
    };
    for edit in [
        TrackEdit::ToggleAnimation { frame: 0 },
        TrackEdit::Value {
            frame: 60,
            value: 24.,
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
    for frame in [0, 15, 29, 30, 45, 59, 60] {
        let rows = if frame < 30 {
            vec![
                ("A", 0., translation(0., 4.), 1.),
                ("B", 30., translation(0., 4.), 1.),
            ]
        } else if frame < 60 {
            vec![
                ("A", 0., translation(0., 4.), 1.),
                ("B", 30., translation(f64::from(frame) * 0.4, 4.), 1.),
                ("C", 60., translation(0., 4.), 1.),
            ]
        } else {
            vec![]
        };
        let expected = literal(false, BOUNDS[0], &rows);
        assert_paths(project, frame, &expected);
        for saved in &reopened {
            assert_eq!(saved, project);
            assert_paths(saved, frame, &expected);
        }
    }
    assert_eq!(project.to_json().unwrap(), json);
    assert_eq!(project_file::encode(project, None).unwrap(), lep);
}

#[test]
fn animator_stack_bounds_retain_composed_geometry_before_every_opacity() {
    use crate::text_animator::Selection;
    let source = "A";
    let pass =
        "<text id='le-animator-0' x='0' y='24' font-family='Wanted Sans' font-size='24'>A</text>";
    let samples = [
        TextAnimatorSample {
            position: [8., 4.],
            opacity: 0.,
            ..Default::default()
        },
        TextAnimatorSample {
            scale: [200., 50.],
            rotation: 90.,
            opacity: 50.,
            ..Default::default()
        },
    ];
    let active: Vec<_> = samples
        .iter()
        .map(|sample| (Selection::new(source, sample), sample))
        .collect();
    let (paint, bounds) = crate::text_animator_render::animate_stack_pass(
        pass, source, &active, 180., 150., "bounds-", true,
    )
    .unwrap();
    let opaque: Vec<_> = samples
        .iter()
        .map(|sample| TextAnimatorSample {
            opacity: 100.,
            ..sample.clone()
        })
        .collect();
    let active: Vec<_> = opaque
        .iter()
        .map(|sample| (Selection::new(source, sample), sample))
        .collect();
    let (expected, no_bounds) = crate::text_animator_render::animate_stack_pass(
        pass, source, &active, 180., 150., "bounds-", true,
    )
    .unwrap();
    assert!(no_bounds.is_none());
    assert_eq!(bounds.as_deref(), Some(expected.as_str()));
    let wrap = |body: &str| {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'><g transform='translate(48 40)'>{body}</g></svg>"
        )
    };
    assert!(raster(&wrap(&paint)).pixels().all(|pixel| pixel.0[3] == 0));
    let literal_svg = "<svg xmlns='http://www.w3.org/2000/svg' width='256' height='192'><g transform='translate(48 40)'><g transform='matrix(0 2 -0.5 0 10 40)'><text x='0' y='24' font-family='Wanted Sans' font-size='24'>A</text></g></g></svg>";
    assert_pixels(
        &raster(&wrap(bounds.as_ref().unwrap())),
        &raster(literal_svg),
        "unattenuated affine bounds geometry",
    );
}
