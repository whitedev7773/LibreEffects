//! Independent Text Animator acceptance. Literal positioned text and explicitly
//! selected protected units bypass production selection, layout and animator SVG.
//! Font shaping/rasterization is shared and remains authoritative. Generated
//! files are immutable input/expectations, never evidence of native interaction.
use crate::{
    editor::{Action, EditorState, Tool},
    rendering::{FrameRenderBudget, Renderer},
    view_state::ProjectViews,
};
use libre_effects_core::*;
use serde_json::json;
use std::path::Path;

thread_local! {
    static PIXEL_PAIRS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PIXEL_CASE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static LITERAL_SVG: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static PIXEL_PROJECT: std::cell::RefCell<Option<Project>> = const { std::cell::RefCell::new(None) };
}
fn pixel_case(context: impl Into<String>) {
    PIXEL_CASE.with(|c| *c.borrow_mut() = context.into());
}
struct PixelEvidence(&'static str);
impl PixelEvidence {
    fn new(label: &'static str) -> Self {
        PIXEL_PAIRS.with(|n| n.set(0));
        PIXEL_CASE.with(|c| c.borrow_mut().clear());
        LITERAL_SVG.with(|c| c.borrow_mut().clear());
        Self(label)
    }
}
impl Drop for PixelEvidence {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let pairs = PIXEL_PAIRS.with(std::cell::Cell::get);
            if pairs > 0 {
                println!(
                    "ANIMATOR_PIXEL_EVIDENCE {}: {pairs} exact RGBA pairs / {} pixels",
                    self.0,
                    pairs * WIDTH as usize * HEIGHT as usize
                );
            }
        }
    }
}

const WIDTH: u32 = 384;
const HEIGHT: u32 = 320;
const ORIGIN: [f64; 2] = [36., 24.];
const FRAMES: [Frame; 9] = [0, 15, 29, 30, 31, 45, 59, 60, 89];
const PARAMETERS: [(TextParam, &str, f64); 5] = [
    (TextParam::AnimatorStart, "AnimatorStart", 0.),
    (TextParam::AnimatorEnd, "AnimatorEnd", 100.),
    (TextParam::AnimatorPositionX, "AnimatorPositionX", 0.),
    (TextParam::AnimatorPositionY, "AnimatorPositionY", 0.),
    (TextParam::AnimatorOpacity, "AnimatorOpacity", 100.),
];

fn scene(text: &str, size: f64, bounds: [f64; 2], style: TextStyle) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Text Animator independent acceptance".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: text.into(),
            font_size: size,
        },
        width: bounds[0],
        height: bounds[1],
        name: "Text Animator".into(),
    })
    .unwrap();
    e.execute(Command::SetColor {
        id: 1,
        color: 0x3876c8,
    })
    .unwrap();
    e.execute(Command::SetTextStyle { id: 1, style }).unwrap();
    for (property, value) in [
        (Property::AnchorX, 0.),
        (Property::AnchorY, 0.),
        (Property::PositionX, ORIGIN[0]),
        (Property::PositionY, ORIGIN[1]),
    ] {
        e.execute(Command::SetValue {
            id: 1,
            property,
            frame: 0,
            value,
        })
        .unwrap();
    }
    e.clear_history();
    e
}
fn style(paragraph: bool) -> TextStyle {
    TextStyle {
        paragraph,
        leading: 1.25,
        ..Default::default()
    }
}
fn set(e: &mut Editor, parameter: TextParam, value: f64) {
    e.execute(Command::EditText {
        id: 1,
        parameter,
        edit: TrackEdit::Value { frame: 0, value },
    })
    .unwrap();
}
fn animator(e: &mut Editor, range: [f64; 2], position: [f64; 2], opacity: f64) {
    for ((parameter, _, _), value) in
        PARAMETERS
            .into_iter()
            .zip([range[0], range[1], position[0], position[1], opacity])
    {
        set(e, parameter, value);
    }
}
fn exact(actual: &Project, expected: &Project, context: &str) {
    assert_eq!(actual, expected, "{context}");
    assert_eq!(
        actual.to_json().unwrap(),
        expected.to_json().unwrap(),
        "{context}"
    );
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
#[derive(Clone, Copy)]
struct LiteralUnit<'a> {
    text: &'a str,
    x: f64,
    y: f64,
    width: f64,
    selected: bool,
}
fn unit(text: &str, y: f64, selected: bool) -> LiteralUnit<'_> {
    LiteralUnit {
        text,
        x: 0.,
        y,
        width: 220.,
        selected,
    }
}
fn group_opacity(body: String, percent: f64) -> String {
    if percent == 100. {
        body
    } else {
        format!("<g opacity='{}'>{body}</g>", percent / 100.)
    }
}
/// Every row is an explicitly declared whole shaping/grapheme protected unit.
/// We never split source text to calculate expected glyph origins or advances.
fn literal_svg(
    size: f64,
    bounds: [f64; 2],
    style: &TextStyle,
    units: &[LiteralUnit<'_>],
    position: [f64; 2],
    unit_opacity: f64,
    paint_opacity: [f64; 2],
    layer_opacity: f64,
) -> String {
    let pass = |stroke: bool| {
        let mut body = String::new();
        for unit in units {
            let (x, anchor) = match style.align {
                TextAlign::Left => (0., "start"),
                TextAlign::Center => (unit.width / 2., "middle"),
                TextAlign::Right => (unit.width, "end"),
            };
            // Preserve the existing point-text SVG's absolute baseline. Moving
            // a baseline into an extra group is algebraically equal but changes
            // f32 outline/transform association in the authoritative rasterizer.
            let (x, y) = if style.paragraph {
                (x, size)
            } else {
                (x + unit.x, size + unit.y)
            };
            let text = format!(
                "<text x='{x}' y='{y}' text-anchor='{anchor}' letter-spacing='{}' font-family='{}' font-weight='{}' font-style='{}' font-size='{size}' fill='{}' xml:space='preserve'>{}</text>",
                style.tracking * size / 1000.,
                escape(&style.font_family),
                style.weight,
                if style.italic { "italic" } else { "normal" },
                if stroke { "none" } else { "#3876c8" },
                escape(unit.text),
            );
            let mut local = if style.paragraph {
                format!("<g transform='translate({} {})'>{text}</g>", unit.x, unit.y)
            } else {
                text
            };
            if unit.selected {
                local = format!(
                    "<g transform='translate({} {})'>{local}</g>",
                    position[0], position[1]
                );
                local = group_opacity(local, unit_opacity);
            }
            body.push_str(&local);
        }
        if style.paragraph {
            body = format!(
                "<svg width='{}' height='{}' overflow='hidden'>{body}</svg>",
                bounds[0], bounds[1]
            );
        }
        if stroke {
            let join = match style.stroke_join {
                TextStrokeJoin::Miter => "miter",
                TextStrokeJoin::Round => "round",
                TextStrokeJoin::Bevel => "bevel",
            };
            body = format!(
                "<g stroke='#{:06x}' stroke-width='{}' stroke-linejoin='{join}' stroke-miterlimit='4'>{body}</g>",
                style.stroke_color, style.stroke_width
            );
        }
        group_opacity(body, paint_opacity[usize::from(stroke)])
    };
    let fill = if style.fill_enabled {
        pass(false)
    } else {
        String::new()
    };
    let stroke = if style.stroke_enabled && style.stroke_width != 0. {
        pass(true)
    } else {
        String::new()
    };
    let body = if style.stroke_over_fill {
        format!("{fill}{stroke}")
    } else {
        format!("{stroke}{fill}")
    };
    let body = group_opacity(body, layer_opacity);
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'><g transform='translate({} {})'>{body}</g></svg>",
        ORIGIN[0], ORIGIN[1]
    );
    LITERAL_SVG.with(|value| *value.borrow_mut() = svg.clone());
    svg
}
fn raw_pixels(svg: &str) -> image::RgbaImage {
    let tree = resvg::usvg::Tree::from_str(svg, &crate::fonts::render_options()).unwrap();
    let mut map = resvg::tiny_skia::Pixmap::new(WIDTH, HEIGHT).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut map.as_mut(),
    );
    image::RgbaImage::from_raw(
        WIDTH,
        HEIGHT,
        map.pixels()
            .iter()
            .flat_map(|p| {
                let p = p.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            })
            .collect(),
    )
    .unwrap()
}
fn pixels_equal(actual: &image::RgbaImage, expected: &image::RgbaImage, context: &str) {
    assert_eq!(actual.dimensions(), expected.dimensions());
    let changed: Vec<_> = actual
        .enumerate_pixels()
        .filter_map(|(x, y, p)| {
            let e = expected.get_pixel(x, y);
            (p != e).then_some((x, y, p.0, e.0))
        })
        .take(8)
        .collect();
    if !changed.is_empty() {
        dump_pixel_mismatch(actual, expected, context);
    }
    assert!(
        changed.is_empty(),
        "{context}, {}: first RGBA differences {changed:?}",
        PIXEL_CASE.with(|c| c.borrow().clone())
    );
}
fn compare_paths(project: &Project, frame: Frame, expected: &image::RgbaImage) {
    PIXEL_PROJECT.with(|value| *value.borrow_mut() = Some(project.clone()));
    let renderer = Renderer::new();
    let source = project.to_json().unwrap();
    let saved = Project::from_json(&source).unwrap();
    let native = project_file::encode(project, None).unwrap();
    let lep = project_file::decode(&native).unwrap().project;
    exact(&saved, project, "JSON source");
    exact(&lep, project, "LEP source");
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
        ("JSON", renderer.render(&saved, frame, WIDTH).unwrap()),
        ("LEP", renderer.render(&lep, frame, WIDTH).unwrap()),
    ] {
        pixels_equal(&image, expected, &format!("{name} frame {frame}"));
        PIXEL_PAIRS.with(|n| n.set(n.get() + 1));
    }
    assert_eq!(
        project.to_json().unwrap(),
        source,
        "render does not rewrite source"
    );
    assert_eq!(project_file::encode(project, None).unwrap(), native);
}
fn isolated(project: &Project, frame: Frame) -> String {
    Renderer::new()
        .isolated_layer_svg(
            project,
            project.active_composition_id(),
            1,
            frame,
            WIDTH,
            "independent-animator",
            &mut FrameRenderBudget::default(),
        )
        .unwrap()
}

#[test]
fn animator_literal_half_open_centers_count_newlines_and_do_not_reorder_reversed_ranges() {
    let _evidence = PixelEvidence::new(
        "animator_literal_half_open_centers_count_newlines_and_do_not_reorder_reversed_ranges",
    );
    // A, LF, B, LF, C have centers 10, 30, 50, 70, 90. Explicit booleans
    // are the oracle; production grapheme enumeration is deliberately not used.
    for (range, selected) in [
        ([0., 100.], [true, true, true]),
        ([10., 50.], [true, false, false]),
        ([50., 90.], [false, true, false]),
        ([40., 60.], [false, true, false]),
        ([30., 40.], [false, false, false]),
        ([50., 50.], [false, false, false]),
        ([90., 10.], [false, false, false]),
    ] {
        pixel_case(format!("range={range:?}, selected={selected:?}"));
        let style = style(false);
        let mut e = scene("A\nB\nC", 24., [220., 160.], style.clone());
        animator(&mut e, range, [35., 9.], 55.);
        let units = [
            unit("A", 0., selected[0]),
            unit("B", 30., selected[1]),
            unit("C", 60., selected[2]),
        ];
        compare_paths(
            e.project(),
            30,
            &raw_pixels(&literal_svg(
                24.,
                [220., 160.],
                &style,
                &units,
                [35., 9.],
                55.,
                [100.; 2],
                100.,
            )),
        );
    }
}

#[test]
fn animator_literal_crlf_spaces_tabs_and_trailing_newlines_use_source_grapheme_denominator() {
    let _evidence = PixelEvidence::new(
        "animator_literal_crlf_spaces_tabs_and_trailing_newlines_use_source_grapheme_denominator",
    );
    for (text, range, selected) in [
        ("A\r\nB", [70., 90.], [false, true]), // CRLF is one, not two.
        ("A \nB", [80., 90.], [false, true]),  // The trailing space counts.
        ("A\t\nB", [80., 90.], [false, true]),
        ("A\nB\n", [55., 70.], [false, true]), // The final LF counts too.
        ("A\r\nB", [30., 70.], [false, false]),
    ] {
        // Paragraph layout trims end whitespace without deleting it from source.
        let style = style(true);
        let mut e = scene(text, 24., [220., 160.], style.clone());
        animator(&mut e, range, [27., -4.], 65.);
        let units = [unit("A", 0., selected[0]), unit("B", 30., selected[1])];
        compare_paths(
            e.project(),
            15,
            &raw_pixels(&literal_svg(
                24.,
                [220., 160.],
                &style,
                &units,
                [27., -4.],
                65.,
                [100.; 2],
                100.,
            )),
        );
    }
}

#[test]
fn animator_identity_empty_and_no_selection_retain_exact_legacy_svg() {
    let _evidence =
        PixelEvidence::new("animator_identity_empty_and_no_selection_retain_exact_legacy_svg");
    for paragraph in [false, true] {
        for text in ["A<&\"\nB", "", "A"] {
            let before = scene(text, 24., [220., 160.], style(paragraph))
                .project()
                .clone();
            let legacy = isolated(&before, 30);
            for (range, delta, opacity) in [
                ([0., 100.], [0., 0.], 100.),
                ([60., 60.], [52., -18.], 0.),
                ([90., 10.], [52., -18.], 0.),
                ([0., 1.], [52., -18.], 0.),
            ] {
                let mut e = Editor::default();
                e.replace_project(before.clone()).unwrap();
                animator(&mut e, range, delta, opacity);
                assert_eq!(
                    isolated(e.project(), 30),
                    legacy,
                    "exact old SVG {text:?}, {range:?}"
                );
                compare_paths(
                    e.project(),
                    30,
                    &Renderer::new().render(&before, 30, WIDTH).unwrap(),
                );
            }
        }
    }
}

#[test]
fn animator_literal_combining_jamo_emoji_zwj_and_flags_remain_whole_graphemes() {
    let _evidence = PixelEvidence::new(
        "animator_literal_combining_jamo_emoji_zwj_and_flags_remain_whole_graphemes",
    );
    // Each complete middle line is one EGC, even when the installed fallback
    // creates several glyphs. The middle unit center remains 50 in all cases.
    // This is headless same-font evidence, not a claim of platform-font parity.
    for grapheme in ["e\u{301}", "\u{1100}\u{1161}\u{11a8}", "👩‍💻", "🇰🇷", "👍🏽"]
    {
        pixel_case(format!("grapheme={grapheme:?}"));
        let text = format!("A\n{grapheme}\nB");
        let style = style(false);
        let mut e = scene(&text, 32., [220., 180.], style.clone());
        animator(&mut e, [40., 60.], [34., 6.], 45.);
        let units = [
            unit("A", 0., false),
            unit(grapheme, 40., true),
            unit("B", 80., false),
        ];
        let expected = raw_pixels(&literal_svg(
            32.,
            [220., 180.],
            &style,
            &units,
            [34., 6.],
            45.,
            [100.; 2],
            100.,
        ));
        compare_paths(e.project(), 30, &expected);
    }
}

#[test]
fn animator_unit_opacity_precedes_complete_fill_stroke_pass_opacity_and_layer_opacity() {
    let _evidence = PixelEvidence::new(
        "animator_unit_opacity_precedes_complete_fill_stroke_pass_opacity_and_layer_opacity",
    );
    for over in [false, true] {
        let style = TextStyle {
            leading: 0.1,
            stroke_enabled: true,
            stroke_color: 0xb84090,
            stroke_width: 15.,
            stroke_over_fill: over,
            ..style(false)
        };
        let mut e = scene("A\nB", 48., [220., 180.], style.clone());
        animator(&mut e, [0., 100.], [16., 8.], 45.);
        set(&mut e, TextParam::FillOpacity, 55.);
        set(&mut e, TextParam::StrokeOpacity, 70.);
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Opacity,
            frame: 0,
            value: 80.,
        })
        .unwrap();
        let units = [unit("A", 0., true), unit("B", 4.8, true)];
        compare_paths(
            e.project(),
            30,
            &raw_pixels(&literal_svg(
                48.,
                [220., 180.],
                &style,
                &units,
                [16., 8.],
                45.,
                [55., 70.],
                80.,
            )),
        );
    }
}

#[test]
fn animator_motion_is_after_layout_before_box_clip_and_does_not_resurrect_overflow() {
    let _evidence = PixelEvidence::new(
        "animator_motion_is_after_layout_before_box_clip_and_does_not_resurrect_overflow",
    );
    let style = style(true);
    let before = scene("A\nB\nC", 24., [80., 60.], style.clone())
        .project()
        .clone();
    let initial = before.composition().layer(1).unwrap();
    let initial_flow = crate::text_flow::layer_lines(initial, 30).unwrap();
    let initial_fit = crate::text_flow::fit_height(initial, 30);
    let initial_layout = crate::text_edit::layout::Layout::for_layer(initial, 30).unwrap();
    for delta in [[0., -30.], [90., 0.], [-40., 8.]] {
        let mut e = Editor::default();
        e.replace_project(before.clone()).unwrap();
        animator(&mut e, [0., 100.], delta, 100.);
        let layer = e.project().composition().layer(1).unwrap();
        let flow = crate::text_flow::layer_lines(layer, 30).unwrap();
        assert_eq!(crate::text_flow::fit_height(layer, 30), initial_fit);
        let layout = crate::text_edit::layout::Layout::for_layer(layer, 30).unwrap();
        assert_eq!(
            format!("{layout:?}"),
            format!("{initial_layout:?}"),
            "source caret/hit cells stay fixed"
        );
        assert_eq!(format!("{flow:?}"), format!("{initial_flow:?}"));
        assert_eq!(crate::text_flow::composed_count(&flow, 60.), 2);
        // C originally overflows and remains uncomposed even when moved upward.
        let units = [unit("A", 0., true), unit("B", 30., true)];
        compare_paths(
            e.project(),
            30,
            &raw_pixels(&literal_svg(
                24.,
                [80., 60.],
                &style,
                &units,
                delta,
                100.,
                [100.; 2],
                100.,
            )),
        );
    }
}

fn animated_scene() -> Editor {
    let mut e = scene("A\nB", 24., [220., 180.], style(false));
    animator(&mut e, [45., 60.], [0., 0.], 100.);
    for (parameter, value) in [
        (TextParam::AnimatorPositionX, 24.),
        (TextParam::AnimatorPositionY, 36.),
        (TextParam::AnimatorOpacity, 25.),
    ] {
        for edit in [
            TrackEdit::ToggleAnimation { frame: 0 },
            TrackEdit::Value { frame: 60, value },
        ] {
            e.execute(Command::EditText {
                id: 1,
                parameter,
                edit,
            })
            .unwrap();
        }
    }
    e.execute(Command::EditTrack {
        id: 1,
        property: PropertyPath::SourceText,
        edit: TrackEdit::ToggleAnimation { frame: 0 },
    })
    .unwrap();
    for (frame, text) in [(30, "C\nD\nE"), (60, "")] {
        e.execute(Command::EditSourceText {
            id: 1,
            frame,
            text: text.into(),
        })
        .unwrap();
    }
    e.clear_history();
    e
}
#[test]
fn animator_sampled_source_precedes_selection_and_linear_tracks_match_literal_boundary_frames() {
    let _evidence = PixelEvidence::new(
        "animator_sampled_source_precedes_selection_and_linear_tracks_match_literal_boundary_frames",
    );
    let e = animated_scene();
    for frame in FRAMES {
        let t = frame.min(60) as f64 / 60.;
        let units = if frame < 30 {
            vec![unit("A", 0., false), unit("B", 30., false)] // Selected LF.
        } else if frame < 60 {
            vec![
                unit("C", 0., false),
                unit("D", 30., true),
                unit("E", 60., false),
            ]
        } else {
            vec![]
        };
        compare_paths(
            e.project(),
            frame,
            &raw_pixels(&literal_svg(
                24.,
                [220., 180.],
                &style(false),
                &units,
                [24. * t, 36. * t],
                100. - 75. * t,
                [100.; 2],
                100.,
            )),
        );
    }
}

fn state(project: &Project, native: bool) -> EditorState {
    let mut state = EditorState::default();
    state.editor.replace_project(project.clone()).unwrap();
    state.editor.select(1);
    state.selected_layers.insert(1);
    state.editor.clear_history();
    state.frame = 30;
    state.tool = Tool::Select;
    state.preview_zoom = Some(if native { 1. } else { 1.5 });
    state.preview_pan = if native { [0., 0.] } else { [13., -7.] };
    state.timeline_zoom = if native { 1. } else { 2. };
    state.normalize();
    state
}
fn codec(state: &mut EditorState, expected: &Project) {
    let views = state.capture_views();
    let view = views.encode_native(expected).unwrap();
    let bytes = project_file::encode(state.editor.project(), Some(&view)).unwrap();
    assert_eq!(&bytes[8..10], &[1, 0], "LEP container version remains 1");
    let decoded = project_file::decode(&bytes).unwrap();
    exact(&decoded.project, expected, "official LEP complete source");
    assert_eq!(decoded.view, Some(view.as_slice()));
    let loaded = ProjectViews::read_native(decoded.view.unwrap(), &decoded.project).unwrap();
    assert_eq!(loaded, views);
    let mut reopened = EditorState::default();
    reopened.editor.replace_project(decoded.project).unwrap();
    reopened.load_views(loaded);
    assert_eq!(
        reopened.capture_views().encode_native(expected).unwrap(),
        view
    );
}
fn literal_static_parameter(project: &Project, name: &str, value: f64) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    wire["version"] = json!(56);
    let layer = &mut wire["composition"]["layers"][0];
    if layer.get("text_parameters").is_none() {
        layer["text_parameters"] = json!({});
    }
    layer["text_parameters"][name] = json!({ "value": value, "keys": {} });
    Project::from_json(&wire.to_string()).unwrap()
}
#[test]
fn animator_edits_preserve_literal_complete_source_history_and_native_view() {
    let _evidence = PixelEvidence::new(
        "animator_edits_preserve_literal_complete_source_history_and_native_view",
    );
    let before = scene("A\nB\nC", 24., [220., 180.], style(true))
        .project()
        .clone();
    for ((parameter, name, default), value) in
        PARAMETERS.into_iter().zip([20., 80., 24., -12., 40.])
    {
        let mut s = state(&before, false);
        let views = s.capture_views();
        let expected = literal_static_parameter(&before, name, value);
        s.bulk_test_action(&Action::Edit(Command::EditText {
            id: 1,
            parameter,
            edit: TrackEdit::Value { frame: 30, value },
        }));
        exact(s.editor.project(), &expected, name);
        assert_eq!(s.capture_views(), views);
        codec(&mut s, &expected);
        s.bulk_test_action(&Action::Undo);
        exact(s.editor.project(), &before, "one Undo");
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        s.bulk_test_action(&Action::Edit(Command::EditText {
            id: 1,
            parameter,
            edit: TrackEdit::Value {
                frame: 30,
                value: default,
            },
        }));
        exact(
            s.editor.project(),
            &before,
            "absent equal default preserves source and Redo",
        );
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        s.bulk_test_action(&Action::Redo);
        exact(s.editor.project(), &expected, "one Redo");
        assert_eq!(s.capture_views(), views);
    }
}
#[test]
fn animator_materialized_defaults_require_schema_56_but_preserve_legacy_pixels_and_view() {
    let _evidence = PixelEvidence::new("materialized_default_identity");
    let before = scene("A\nB", 24., [220., 180.], style(false))
        .project()
        .clone();
    let expected_pixels = Renderer::new().render(&before, 30, WIDTH).unwrap();
    for (_, name, default) in PARAMETERS {
        let project = literal_static_parameter(&before, name, default);
        let mut s = state(&project, false);
        codec(&mut s, &project);
        assert_eq!(isolated(&project, 30), isolated(&before, 30));
        compare_paths(&project, 30, &expected_pixels);
        for version in [33, 48, 49, 52, 54, 55] {
            let mut wire = serde_json::to_value(&project).unwrap();
            wire["version"] = json!(version);
            assert!(
                Project::from_json(&wire.to_string()).is_err(),
                "{name} schema {version}"
            );
            let unchecked: Project = serde_json::from_value(wire).unwrap();
            assert!(project_file::encode(&unchecked, None).is_err());
        }
    }
}

fn export(root: &Path, label: &str, project: &Project, views: &ProjectViews) {
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let view = views.encode_native(project).unwrap();
    for (suffix, bytes) in [
        ("lfe.json", project.to_json().unwrap().into_bytes()),
        (
            "generated.lep",
            project_file::encode(project, Some(&view)).unwrap(),
        ),
    ] {
        use std::io::Write;
        let path = root.join(format!("{label}.{suffix}"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => file.write_all(&bytes).unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => assert_eq!(
                std::fs::read(&path).unwrap(),
                bytes,
                "immutable fixture {}",
                path.display()
            ),
            Err(error) => panic!("cannot export {}: {error}", path.display()),
        }
    }
}
#[test]
#[ignore = "requires an explicit absolute fixture output directory"]
fn export_text_animator_acceptance_fixtures() {
    let root = std::env::var_os("LIBREEFFECTS_EXPORT_TEXT_ANIMATOR_FIXTURES")
        .expect("set fixture directory");
    let root = Path::new(&root);
    let before = scene("A\nB\nC", 32., [220., 180.], style(false))
        .project()
        .clone();
    let views = state(&before, true).capture_views();
    export(root, "native-before", &before, &views);
    let mut expected = before;
    for (((_, name, _), value), label) in PARAMETERS
        .into_iter()
        .zip([40., 60., 35., 9., 55.])
        .zip(["start", "end", "x", "y", "opacity"])
    {
        expected = literal_static_parameter(&expected, name, value);
        export(root, &format!("native-{label}-expected"), &expected, &views);
    }
    let mut wire = serde_json::to_value(&expected).unwrap();
    wire["composition"]["layers"][0]["text_parameters"]["AnimatorOpacity"]["keys"] = json!({
        "30": { "value": 55.0, "interpolation": "Linear" }
    });
    let keyed = Project::from_json(&wire.to_string()).unwrap();
    export(root, "native-opacity-key30-expected", &keyed, &views);
    wire["composition"]["layers"][0]["text_parameters"]["AnimatorOpacity"]["keys"]["60"] = json!({
        "value": 0.0, "interpolation": "Linear"
    });
    let animated = Project::from_json(&wire.to_string()).unwrap();
    export(root, "native-opacity-key60-expected", &animated, &views);
    let disabled = literal_static_parameter(&animated, "AnimatorOpacity", 27.5);
    export(
        root,
        "native-opacity-disabled45-expected",
        &disabled,
        &views,
    );
    export(root, "animated-source", animated_scene().project(), &views);
    for paragraph in [false, true] {
        for (align, label) in [
            (TextAlign::Left, "left"),
            (TextAlign::Center, "center"),
            (TextAlign::Right, "right"),
        ] {
            let legacy = scene(
                "A<&\"\r\nB\nC",
                24.,
                [220., 180.],
                TextStyle {
                    align,
                    ..style(paragraph)
                },
            )
            .project()
            .clone();
            for (_, name, _) in PARAMETERS {
                assert!(!legacy.to_json().unwrap().contains(&format!("\"{name}\"")));
            }
            export(
                root,
                &format!(
                    "legacy-{}-{label}",
                    if paragraph { "paragraph" } else { "point" }
                ),
                &legacy,
                &views,
            );
        }
    }
}
/// This verifier reads actual native output and a separately declared reference;
/// it never derives expected source or VIEW values from the actual save.
#[test]
#[ignore = "requires separately recorded native save and independent expected fixture"]
fn verify_recorded_native_text_animator_save() {
    let _evidence = PixelEvidence::new("actual_native_source_reference");
    let actual_path =
        std::env::var_os("LIBREEFFECTS_TEXT_ANIMATOR_NATIVE_SAVE").expect("set native save path");
    let expected_path = std::env::var_os("LIBREEFFECTS_TEXT_ANIMATOR_EXPECTED")
        .expect("set independent LEP reference");
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_TEXT_ANIMATOR_EXPECTED_FRAME") {
        let frame: u32 = frame.parse().expect("unsigned frame");
        assert!(frame < expected.project.composition().duration());
        expected
            .views
            .compositions
            .entry(expected.project.active_composition_id())
            .or_default()
            .frame = frame;
    }
    assert_eq!(actual.format, crate::project_io::ProjectFormat::Lep);
    exact(&actual.project, &expected.project, "native complete source");
    assert_eq!(actual.views, expected.views, "native complete VIEW");
    let renderer = Renderer::new();
    for frame in FRAMES {
        compare_paths(
            &actual.project,
            frame,
            &renderer.render(&expected.project, frame, WIDTH).unwrap(),
        );
    }
    println!(
        "Exact full source, VIEW and {} native shared/preview/output/JSON/LEP frame sets: {}",
        FRAMES.len(),
        Path::new(&actual_path).display()
    );
}

fn raw_source_ranges(text: &str, style: &TextStyle) -> Vec<std::ops::Range<usize>> {
    fn visit(group: &resvg::usvg::Group, out: &mut Vec<std::ops::Range<usize>>) {
        for node in group.children() {
            match node {
                resvg::usvg::Node::Text(text) => {
                    out.extend(text.layouted().iter().flat_map(|span| {
                        span.positioned_glyphs
                            .iter()
                            .map(|glyph| glyph.source_range.clone())
                    }))
                }
                resvg::usvg::Node::Group(group) => visit(group, out),
                _ => {}
            }
        }
    }
    let svg = literal_svg(
        32.,
        [220., 180.],
        style,
        &[unit(text, 0., false)],
        [0.; 2],
        100.,
        [100.; 2],
        100.,
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &crate::fonts::render_options()).unwrap();
    let mut ranges = Vec::new();
    visit(tree.root(), &mut ranges);
    ranges
}

#[test]
fn animator_bundled_multigrapheme_ligature_uses_actual_cluster_closure() {
    let _evidence = PixelEvidence::new("bundled_actual_ligature_closure");
    // Wanted Sans' shipped ordinary liga lookup maps these 11 characters to
    // one logo glyph. Pin actual compositor metadata before using that oracle.
    let text = "wanted_logo";
    assert_eq!(raw_source_ranges(text, &style(false)), [0..11]);
    for (range, opacity) in [([45., 55.], 40.), ([0., 5.], 100.), ([95., 100.], 60.)] {
        let mut e = scene(text, 32., [220., 180.], style(false));
        animator(&mut e, range, [34., 6.], opacity);
        compare_paths(
            e.project(),
            30,
            &raw_pixels(&literal_svg(
                32.,
                [220., 180.],
                &style(false),
                &[unit(text, 0., true)],
                [34., 6.],
                opacity,
                [100.; 2],
                100.,
            )),
        );
    }
}

#[test]
fn animator_authoritative_utf8_ranges_protect_whole_fallback_graphemes() {
    for grapheme in ["e\u{301}", "\u{1100}\u{1161}\u{11a8}", "👩‍💻", "🇰🇷", "👍🏽"]
    {
        let text = format!("A{grapheme}B");
        let ranges = raw_source_ranges(&text, &style(false));
        assert!(!ranges.is_empty(), "actual shaped glyph metadata {text:?}");
        for range in &ranges {
            assert!(
                range.start < range.end && range.end <= text.len(),
                "{text:?}: {range:?}"
            );
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }
        let selected = crate::text_animator::Selection::new(
            &text,
            &TextAnimatorSample {
                start: 40.,
                end: 60.,
                position: [34., 6.],
                opacity: 45.,
                ..Default::default()
            },
        )
        .protect(&ranges)
        .unwrap();
        let middle = 1..1 + grapheme.len();
        let mut middle_units = std::collections::BTreeSet::new();
        for (range, protected) in ranges.iter().zip(&selected) {
            let in_middle = range.start < middle.end && range.end > middle.start;
            assert_eq!(protected.selected, in_middle, "{text:?}: {range:?}");
            if in_middle {
                middle_units.insert(protected.unit);
            }
        }
        assert_eq!(
            middle_units.len(),
            1,
            "whole EGC protects every fallback glyph: {text:?}"
        );
    }
}

#[test]
#[ignore = "explicit Linux/system-font qualification: requires installed DejaVu Sans"]
fn animator_system_font_ffi_ligature_protects_partial_source_selection() {
    let _evidence = PixelEvidence::new("explicit_DejaVu_ffi_ligature");
    assert!(
        crate::fonts::family("DejaVu Sans").is_some(),
        "install the explicit fixture font before this qualification"
    );
    let style = TextStyle {
        font_family: "DejaVu Sans".into(),
        ..style(false)
    };
    assert_eq!(raw_source_ranges("ffi", &style), [0..3]);
    let repeated = raw_source_ranges("ffi ffi", &style);
    assert!(
        repeated.contains(&(0..3)) && repeated.contains(&(4..7)),
        "repeated same glyph IDs retain distinct source spans: {repeated:?}"
    );
    for range in [[0., 34.], [40., 60.], [70., 100.]] {
        let mut e = scene("ffi", 32., [220., 180.], style.clone());
        animator(&mut e, range, [34., 6.], 45.);
        compare_paths(
            e.project(),
            30,
            &raw_pixels(&literal_svg(
                32.,
                [220., 180.],
                &style,
                &[unit("ffi", 0., true)],
                [34., 6.],
                45.,
                [100.; 2],
                100.,
            )),
        );
    }
    let mut e = scene("ffi\nffi", 32., [220., 180.], style.clone());
    animator(&mut e, [20., 25.], [34., 6.], 45.);
    compare_paths(
        e.project(),
        30,
        &raw_pixels(&literal_svg(
            32.,
            [220., 180.],
            &style,
            &[unit("ffi", 0., true), unit("ffi", 40., false)],
            [34., 6.],
            45.,
            [100.; 2],
            100.,
        )),
    );
}

#[test]
fn animator_new_text_addresses_round_trip_graph_view_without_source_changes() {
    use crate::view_state::{GraphChannel, GraphRanges};
    let mut e = scene("A\nB", 24., [220., 180.], style(false));
    for (parameter, _, _) in PARAMETERS {
        e.execute(Command::EditText {
            id: 1,
            parameter,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
    }
    let source = e.project().clone();
    let mut s = state(&source, false);
    s.graph_open = true;
    for (parameter, _, _) in PARAMETERS {
        let channel = GraphChannel {
            id: 1,
            property: PropertyPath::Text(parameter),
        };
        s.graph_channels.pin(channel).unwrap();
        s.graph_channels.activate(channel);
        s.graph_channels.ranges.insert(
            channel,
            GraphRanges {
                value: Some([-50., 200.]),
                speed: Some([-25., 25.]),
            },
        );
    }
    s.normalize();
    let view = s.capture_views().encode_native(&source).unwrap();
    let wire = String::from_utf8(view.clone()).unwrap();
    for (_, name, _) in PARAMETERS {
        assert!(wire.contains(name), "new Text address {name}");
    }
    codec(&mut s, &source);
    exact(
        s.editor.project(),
        &source,
        "Graph VIEW cannot materialize or change tracks",
    );
    assert!(!s.editor.can_undo());
}

fn add_effect(e: &mut Editor, kind: EffectKind) {
    e.execute(Command::Effect {
        id: 1,
        edit: EffectEdit::Add(kind),
    })
    .unwrap();
}
fn set_property(e: &mut Editor, property: Property, value: f64) {
    e.execute(Command::SetValue {
        id: 1,
        property,
        frame: 0,
        value,
    })
    .unwrap();
}
fn clipped_mask(e: &mut Editor, offset: [f64; 2]) {
    e.execute(Command::SetMask {
        id: 1,
        mask: Some(Mask {
            x: 130. - offset[0],
            y: 35. - offset[1],
            width: 22.,
            height: 60.,
            inverted: false,
        }),
    })
    .unwrap();
    e.execute(Command::SetPathMasks {
        id: 1,
        masks: vec![PathMask {
            path: VectorPath {
                closed: true,
                vertices: [[118., 30.], [160., 30.], [150., 100.], [118., 100.]]
                    .map(|[x, y]| PathVertex::corner([x - offset[0], y - offset[1]]))
                    .to_vec(),
            },
            ..Default::default()
        }],
    })
    .unwrap();
}
fn text_matte(e: &mut Editor, mode: MatteMode) {
    e.execute(Command::AddContent {
        content: Content::Solid,
        width: WIDTH.into(),
        height: HEIGHT.into(),
        name: "Independent matte target".into(),
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
fn animator_opaque_local_motion_matches_static_source_with_fixed_masks_and_mattes() {
    let _evidence = PixelEvidence::new("static_motion_masks_mattes");
    for masked in [false, true] {
        for matte in [
            None,
            Some(MatteMode::Alpha),
            Some(MatteMode::Luma),
            Some(MatteMode::LumaInverted),
        ] {
            pixel_case(format!("masked={masked}, matte={matte:?}"));
            let mut actual = scene("A\nB", 32., [220., 180.], style(false));
            let mut expected = scene("A\nB", 32., [220., 180.], style(false));
            animator(&mut actual, [0., 100.], [120., 30.], 100.);
            set_property(&mut expected, Property::PositionX, ORIGIN[0] + 120.);
            set_property(&mut expected, Property::PositionY, ORIGIN[1] + 30.);
            for e in [&mut actual, &mut expected] {
                set_property(e, Property::Opacity, 80.);
            }
            if masked {
                clipped_mask(&mut actual, [0., 0.]);
                // Independent compensating mask coordinates keep the same
                // world-space clip after moving the reference whole layer.
                clipped_mask(&mut expected, [120., 30.]);
            }
            if let Some(mode) = matte {
                text_matte(&mut actual, mode);
                text_matte(&mut expected, mode);
            }
            let pixels = Renderer::new()
                .render(expected.project(), 30, WIDTH)
                .unwrap();
            assert!(pixels.pixels().any(|p| p[3] != 0), "reference must paint");
            compare_paths(actual.project(), 30, &pixels);
        }
    }
}

#[test]
fn animator_local_motion_precedes_layer_rotation_and_nonuniform_scale() {
    let _evidence = PixelEvidence::new("literal_local_axes_transform");
    let mut actual = scene("A\nB", 32., [40., 90.], style(false));
    let mut expected = scene("A\nB", 32., [40., 90.], style(false));
    for e in [&mut actual, &mut expected] {
        set_property(e, Property::PositionX, 180.);
        set_property(e, Property::PositionY, 60.);
        set_property(e, Property::Rotation, 90.);
        set_property(e, Property::ScaleX, 125.);
        set_property(e, Property::ScaleY, 75.);
    }
    animator(&mut actual, [0., 100.], [24., 8.], 100.);
    // R90 * diag(1.25,0.75) * [24,8] = [-6,30]. No production
    // transform/inversion helper is used to derive the static reference.
    set_property(&mut expected, Property::PositionX, 174.);
    set_property(&mut expected, Property::PositionY, 90.);
    compare_paths(
        actual.project(),
        30,
        &Renderer::new()
            .render(expected.project(), 30, WIDTH)
            .unwrap(),
    );
}

fn filter_rectangles(project: &Project) -> Vec<(String, [f32; 4])> {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'>{}</svg>",
        isolated(project, 30)
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &crate::fonts::render_options()).unwrap();
    tree.filters()
        .iter()
        .map(|filter| {
            let r = filter.rect();
            (
                filter.id().to_owned(),
                [r.x(), r.y(), r.width(), r.height()],
            )
        })
        .collect()
}
#[test]
fn animator_zero_opacity_retains_translated_unattenuated_effect_allocation() {
    let _evidence = PixelEvidence::new("zero_opacity_effect_geometry");
    for kind in [
        EffectKind::Fill,
        EffectKind::GaussianBlur,
        EffectKind::DropShadow,
        EffectKind::Glow,
    ] {
        let mut e = scene("A\nB", 32., [40., 90.], style(false));
        add_effect(&mut e, kind);
        let before = filter_rectangles(e.project());
        animator(&mut e, [0., 100.], [160., 30.], 100.);
        let translated = filter_rectangles(e.project());
        assert_ne!(
            translated, before,
            "translated ink expands the allocation for {kind:?}"
        );
        for opacity in [45., 0.] {
            set(&mut e, TextParam::AnimatorOpacity, opacity);
            assert_eq!(
                filter_rectangles(e.project()),
                translated,
                "opacity={opacity} must not shrink transformed {kind:?} geometry"
            );
        }
        compare_paths(e.project(), 30, &image::RgbaImage::new(WIDTH, HEIGHT));
    }
}

#[test]
fn animator_literal_oracle_pins_legacy_absolute_point_baselines_before_animation() {
    let _evidence = PixelEvidence::new("legacy_literal_oracle_self_check");
    for paragraph in [false, true] {
        for middle in ["B", "e\u{301}", "\u{1100}\u{1161}\u{11a8}", "👩‍💻", "🇰🇷"] {
            pixel_case(format!("legacy paragraph={paragraph}, middle={middle:?}"));
            let text = format!("A\n{middle}\nB");
            let e = scene(&text, 32., [220., 180.], style(paragraph));
            let units = [
                unit("A", 0., false),
                unit(middle, 40., false),
                unit("B", 80., false),
            ];
            compare_paths(
                e.project(),
                30,
                &raw_pixels(&literal_svg(
                    32.,
                    [220., 180.],
                    &style(paragraph),
                    &units,
                    [0.; 2],
                    100.,
                    [100.; 2],
                    100.,
                )),
            );
        }
    }
}

#[test]
fn animator_rtl_selection_uses_logical_source_not_visual_glyph_order() {
    let text = "אבג";
    let ranges = raw_source_ranges(text, &style(false));
    assert_eq!(ranges, [4..6, 2..4, 0..2], "authoritative visual RTL order");
    let protected = crate::text_animator::Selection::new(
        text,
        &TextAnimatorSample {
            start: 0.,
            end: 34.,
            position: [20., 0.],
            opacity: 50.,
            ..Default::default()
        },
    )
    .protect(&ranges)
    .unwrap();
    assert_eq!(
        protected.iter().map(|g| g.selected).collect::<Vec<_>>(),
        [false, false, true]
    );
    assert_eq!(
        protected.iter().map(|g| g.unit).collect::<Vec<_>>(),
        [2, 1, 0]
    );
}

#[test]
fn animator_path_mask_preserves_original_fixed_layer_domain_after_ink_motion() {
    let _evidence = PixelEvidence::new("fixed_path_mask_domain");
    // Path masks retain the established 0..layer-width/height domain. Moving
    // point ink beyond it does not enlarge or move that domain with the ink.
    let mut narrow = scene("A\nB", 32., [40., 90.], style(false));
    animator(&mut narrow, [0., 100.], [120., 30.], 100.);
    clipped_mask(&mut narrow, [0.; 2]);
    compare_paths(narrow.project(), 30, &image::RgbaImage::new(WIDTH, HEIGHT));
    let mut wide = scene("A\nB", 32., [220., 180.], style(false));
    animator(&mut wide, [0., 100.], [120., 30.], 100.);
    clipped_mask(&mut wide, [0.; 2]);
    assert!(
        Renderer::new()
            .render(wide.project(), 30, WIDTH)
            .unwrap()
            .pixels()
            .any(|p| p[3] > 0),
        "the same moved ink and masks paint when the fixed domain contains them"
    );
}

/// Explicit diagnostic export only. Ordinary default tests do not write files.
/// Every run gets a fresh directory; a failed comparison is never overwritten.
fn dump_pixel_mismatch(actual: &image::RgbaImage, expected: &image::RgbaImage, context: &str) {
    let Some(root) = std::env::var_os("LIBREEFFECTS_TEXT_ANIMATOR_DIAGNOSTICS") else {
        return;
    };
    let root = Path::new(&root);
    if !root.is_absolute() {
        eprintln!("Animator diagnostic directory must be absolute");
        return;
    }
    let case = PIXEL_CASE.with(|value| value.borrow().clone());
    let thread = std::thread::current();
    let label = format!("{}-{context}-{case}", thread.name().unwrap_or("animator"));
    let slug: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(180)
        .collect();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root.join(format!("{slug}-{nonce}"));
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&directory)?;
        actual.save(directory.join("actual.png"))?;
        expected.save(directory.join("expected.png"))?;
        std::fs::write(directory.join("context.txt"), label)?;
        let literal = LITERAL_SVG.with(|value| value.borrow().clone());
        if !literal.is_empty() {
            std::fs::write(directory.join("expected-literal.svg"), literal)?;
        }
        if let Some(project) = PIXEL_PROJECT.with(|value| value.borrow().clone()) {
            std::fs::write(
                directory.join("actual-source.lfe.json"),
                project.to_json().unwrap(),
            )?;
            // Test fixtures use frame30 unless the context explicitly says otherwise.
            let frame = context
                .rsplit_once("frame ")
                .and_then(|(_, n)| n.parse::<u32>().ok())
                .unwrap_or(30);
            let body = isolated(&project, frame);
            std::fs::write(
                directory.join("actual-isolated.svg"),
                format!(
                    "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='{WIDTH}' height='{HEIGHT}'>{body}</svg>"
                ),
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => eprintln!(
            "Animator exact-pixel failure artifacts: {}",
            directory.display()
        ),
        Err(error) => eprintln!("Could not write Animator diagnostic artifacts: {error}"),
    }
}

#[test]
fn animator_offset_animated_range_matches_literal_grapheme_units_without_wrapping() {
    let _evidence = PixelEvidence::new("animator_offset_literal_range");
    // Five graphemes with centers 10, 30, 50, 70, 90. The combining
    // sequence is one unit; literal flags avoid using production selection.
    for paragraph in [false, true] {
        let style = style(paragraph);
        let mut editor = scene("A\ne\u{301}\nB", 24., [220., 160.], style.clone());
        animator(&mut editor, [10., 50.], [23., 7.], 55.);
        set(&mut editor, TextParam::AnimatorOffset, -40.);
        editor
            .execute(Command::EditText {
                id: 1,
                parameter: TextParam::AnimatorOffset,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
        editor
            .execute(Command::EditText {
                id: 1,
                parameter: TextParam::AnimatorOffset,
                edit: TrackEdit::Value {
                    frame: 60,
                    value: 40.,
                },
            })
            .unwrap();
        for (frame, selected) in [
            (0, [false, false, false]),
            (30, [true, false, false]),
            (60, [false, true, false]),
        ] {
            pixel_case(format!("offset paragraph={paragraph}, frame={frame}"));
            let units = [
                unit("A", 0., selected[0]),
                unit("e\u{301}", 30., selected[1]),
                unit("B", 60., selected[2]),
            ];
            compare_paths(
                editor.project(),
                frame,
                &raw_pixels(&literal_svg(
                    24.,
                    [220., 160.],
                    &style,
                    &units,
                    [23., 7.],
                    55.,
                    [100.; 2],
                    100.,
                )),
            );
        }
    }
}

#[test]
fn animator_offset_ligature_render_matches_explicit_effective_range() {
    let mut offset = scene("office e\u{301} 👩‍💻", 32., [300., 120.], style(false));
    animator(&mut offset, [0., 10.], [17., -4.], 60.);
    let mut explicit = Editor::default();
    explicit.replace_project(offset.project().clone()).unwrap();
    animator(&mut explicit, [20., 30.], [17., -4.], 60.);
    set(&mut offset, TextParam::AnimatorOffset, 20.);
    let renderer = Renderer::new();
    let expected = renderer.render(explicit.project(), 0, WIDTH).unwrap();
    let actual = renderer.render(offset.project(), 0, WIDTH).unwrap();
    pixels_equal(
        &actual,
        &expected,
        "offset retains protected ffi/Unicode layout",
    );
}
