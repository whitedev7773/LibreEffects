//! Independent paragraph-style acceptance. Literal line plans and raw SVG text
//! nodes bypass production wrapping, metrics, text_svg and paragraph geometry.
//! The shared font database/rasterizer remains authoritative for glyph shaping.
//! Exported fixtures are generated input, never evidence of native interaction.
use crate::{
    editor::{Action, EditorState, Tool},
    rendering::Renderer,
    view_state::ProjectViews,
};
use libre_effects_core::*;
use serde_json::json;
use std::path::Path;

const WIDTH: u32 = 384;
const HEIGHT: u32 = 320;
const ORIGIN: [f64; 2] = [36., 24.];
const FRAMES: [Frame; 9] = [0, 15, 29, 30, 31, 45, 59, 60, 89];
const FIELDS: [(TextParagraphField, &str); 5] = [
    (TextParagraphField::LeftIndent, "paragraph_left_indent"),
    (TextParagraphField::RightIndent, "paragraph_right_indent"),
    (
        TextParagraphField::FirstLineIndent,
        "paragraph_first_line_indent",
    ),
    (TextParagraphField::SpaceBefore, "paragraph_space_before"),
    (TextParagraphField::SpaceAfter, "paragraph_space_after"),
];

#[derive(Clone, Debug)]
struct LiteralLine<'a> {
    text: &'a str,
    x: f64,
    width: f64,
    y: f64,
    after: f64,
}
fn line(text: &str, x: f64, width: f64, y: f64, after: f64) -> LiteralLine<'_> {
    LiteralLine {
        text,
        x,
        width,
        y,
        after,
    }
}
fn styled() -> TextStyle {
    TextStyle {
        paragraph: true,
        leading: 1.25,
        paragraph_left_indent: 30.,
        paragraph_right_indent: 24.,
        paragraph_first_line_indent: 60.,
        paragraph_space_before: 11.,
        paragraph_space_after: 9.,
        ..Default::default()
    }
}
fn scene(text: &str, size: f64, bounds: [f64; 2], style: TextStyle) -> Editor {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Paragraph independent acceptance".into(),
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
        name: "Paragraph style".into(),
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
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}
/// Independent glyph-only SVG; no production paragraph or text-SVG helper.
fn raw_text(text: &str, size: f64, width: f64, style: &TextStyle) -> String {
    let (x, anchor) = match style.align {
        TextAlign::Left => (0., "start"),
        TextAlign::Center => (width / 2., "middle"),
        TextAlign::Right => (width, "end"),
    };
    format!(
        "<text x='{x}' y='{size}' text-anchor='{anchor}' letter-spacing='{}' font-family='Wanted Sans' font-weight='400' font-style='normal' font-size='{size}' fill='#3876c8' xml:space='preserve'>{}</text>",
        style.tracking * size / 1000.,
        escape(text)
    )
}
fn raw_document(body: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'>{body}</svg>"
    )
}
fn literal_svg(size: f64, bounds: [f64; 2], style: &TextStyle, plan: &[LiteralLine<'_>]) -> String {
    let mut body = format!("<g transform='translate({} {})'>", ORIGIN[0], ORIGIN[1]);
    if style.paragraph {
        body.push_str(&format!(
            "<svg width='{}' height='{}' overflow='hidden'>",
            bounds[0], bounds[1]
        ));
    }
    for row in plan {
        if !row.text.is_empty() {
            body.push_str(&format!(
                "<g transform='translate({} {})'>{}</g>",
                row.x,
                row.y,
                raw_text(row.text, size, row.width, style)
            ));
        }
    }
    if style.paragraph {
        body.push_str("</svg>");
    }
    body.push_str("</g>");
    raw_document(&body)
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
    assert!(
        changed.is_empty(),
        "{context}: first RGBA differences {changed:?}"
    );
}
fn raw_bottom(text: &str, size: f64, width: f64, style: &TextStyle) -> f64 {
    fn find(group: &resvg::usvg::Group) -> Option<&resvg::usvg::Text> {
        group.children().iter().find_map(|node| match node {
            resvg::usvg::Node::Text(text) => Some(text.as_ref()),
            resvg::usvg::Node::Group(group) => find(group),
            _ => None,
        })
    }
    let body = raw_text(if text.is_empty() { " " } else { text }, size, width, style);
    let tree =
        resvg::usvg::Tree::from_str(&raw_document(&body), &crate::fonts::render_options()).unwrap();
    find(tree.root()).map_or(size * 1.2, |t| f64::from(t.bounding_box().bottom()))
}
fn compare_paths(project: &Project, frame: Frame, expected: &image::RgbaImage) {
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
    }
    assert_eq!(
        project.to_json().unwrap(),
        source,
        "render must not rewrite source"
    );
    assert_eq!(project_file::encode(project, None).unwrap(), native);
}
fn compare_plan(text: &str, size: f64, width: f64, style: &TextStyle, plan: &[LiteralLine<'_>]) {
    let actual = crate::text_flow::lines(text, size, width, style);
    assert_eq!(actual.len(), plan.len(), "literal wrap count for {text:?}");
    for (index, (actual, expected)) in actual.iter().zip(plan).enumerate() {
        assert_eq!(
            &text[actual.range.start..actual.visible_end],
            expected.text,
            "line {index}"
        );
        assert_eq!(
            (actual.x, actual.width, actual.y, actual.after),
            (expected.x, expected.width, expected.y, expected.after),
            "line {index}"
        );
        assert!(actual.fits_width, "literal fitting line {index}");
        let bottom = expected.y + raw_bottom(expected.text, size, expected.width, style);
        assert!(
            (actual.bottom - bottom).abs() < 1e-6,
            "independent glyph metric line {index}"
        );
    }
    let joined = actual
        .iter()
        .map(|l| &text[l.range.start..l.terminator.end])
        .collect::<String>();
    assert_eq!(
        joined, text,
        "source ranges and complete terminators retain every original byte"
    );
}

#[test]
fn paragraph_literal_wrap_hard_crlf_blank_and_trailing_paragraphs_match_svg() {
    let text = "AAAA BBBB CCCC\r\nD\n\nE\n";
    for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        let style = TextStyle { align, ..styled() };
        let plan = [
            line("AAAA", 90., 86., 11., 0.),
            line("BBBB CCCC", 30., 146., 41., 9.),
            line("D", 90., 86., 91., 9.),
            line("", 90., 86., 141., 9.),
            line("E", 90., 86., 191., 9.),
            line("", 90., 86., 241., 9.),
        ];
        compare_plan(text, 24., 200., &style, &plan);
        let e = scene(text, 24., [200., 285.], style.clone());
        let expected = raw_pixels(&literal_svg(24., [200., 285.], &style, &plan));
        assert!(expected.pixels().any(|p| p[3] == 255));
        compare_paths(e.project(), 30, &expected);
        let fit = plan
            .iter()
            .map(|row| row.y + raw_bottom(row.text, 24., row.width, &style) + row.after)
            .fold(1., f64::max);
        assert!(
            (crate::text_flow::fit_height(e.selected_layer().unwrap(), 30).unwrap() - fit).abs()
                < 1e-6
        );
    }
}

#[test]
fn paragraph_unicode_mandatory_breaks_do_not_restart_indent_or_paragraph_spacing() {
    for separator in ['\u{2028}', '\u{2029}'] {
        let text = format!("A{separator}B\nC");
        let style = styled();
        let plan = [
            line("A", 90., 86., 11., 0.),
            line("B", 30., 146., 41., 9.),
            line("C", 90., 86., 91., 9.),
        ];
        compare_plan(&text, 24., 200., &style, &plan);
        let e = scene(&text, 24., [200., 160.], style.clone());
        compare_paths(
            e.project(),
            0,
            &raw_pixels(&literal_svg(24., [200., 160.], &style, &plan)),
        );
    }
}

#[test]
fn paragraph_negative_hanging_indents_align_in_inner_width_and_clip_the_outer_box() {
    for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        let style = TextStyle {
            align,
            paragraph_first_line_indent: -45.,
            paragraph_left_indent: 5.,
            paragraph_right_indent: 12.,
            paragraph_space_before: 0.,
            paragraph_space_after: 0.,
            ..styled()
        };
        let plan = [
            line("AAAA", -40., 108., 0., 0.),
            line("B", -40., 108., 30., 0.),
        ];
        let e = scene("AAAA\nB", 24., [80., 90.], style.clone());
        compare_plan("AAAA\nB", 24., 80., &style, &plan);
        let expected = raw_pixels(&literal_svg(24., [80., 90.], &style, &plan));
        assert!(expected.pixels().any(|p| p[3] > 0));
        for (x, y, pixel) in expected.enumerate_pixels() {
            if !(36..116).contains(&x) || !(24..114).contains(&y) {
                assert_eq!(pixel[3], 0);
            }
        }
        compare_paths(e.project(), 15, &expected);
    }
}

#[test]
fn paragraph_nonpositive_width_is_overflow_and_final_after_space_does_not_hide_ink() {
    for (left, first, right) in [(100., 0., 0.), (30., 40., 30.), (30., 41., 30.)] {
        let style = TextStyle {
            paragraph_left_indent: left,
            paragraph_first_line_indent: first,
            paragraph_right_indent: right,
            ..styled()
        };
        for text in ["A\nB", "\nB"] {
            let e = scene(text, 24., [100., 150.], style.clone());
            let flow = crate::text_flow::lines(text, 24., 100., &style);
            assert_eq!(crate::text_flow::composed_count(&flow, 150.), 0);
            assert!(!flow[0].fits_width);
            assert_eq!(
                &text[flow[0].range.clone()],
                text.split('\n').next().unwrap()
            );
            compare_paths(e.project(), 0, &image::RgbaImage::new(WIDTH, HEIGHT));
        }
    }
    let style = TextStyle {
        paragraph_left_indent: 0.,
        paragraph_first_line_indent: 0.,
        paragraph_right_indent: 0.,
        paragraph_space_before: 7.,
        paragraph_space_after: 100.,
        ..styled()
    };
    let bottom = 7. + raw_bottom("A", 24., 100., &style);
    let height = bottom.ceil();
    let e = scene("A", 24., [100., height], style.clone());
    let plan = [line("A", 0., 100., 7., 100.)];
    compare_plan("A", 24., 100., &style, &plan);
    assert_eq!(
        crate::text_flow::composed_count(&crate::text_flow::lines("A", 24., 100., &style), height),
        1
    );
    assert!(
        (crate::text_flow::fit_height(e.selected_layer().unwrap(), 0).unwrap() - bottom - 100.)
            .abs()
            < 1e-6
    );
    let expected = raw_pixels(&literal_svg(24., [100., height], &style, &plan));
    assert!(expected.pixels().any(|p| p[3] > 0));
    compare_paths(e.project(), 0, &expected);
    let too_short = scene("A", 24., [100., bottom - 1.], style);
    compare_paths(
        too_short.project(),
        0,
        &image::RgbaImage::new(WIDTH, HEIGHT),
    );
}

#[test]
fn paragraph_zero_defaults_and_point_mode_dormancy_preserve_old_literal_pixels() {
    for paragraph in [false, true] {
        for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
            let style = TextStyle {
                paragraph,
                align,
                leading: 1.25,
                ..Default::default()
            };
            let plan = [
                line("A<&\"", 0., 200., 0., 0.),
                line("B", 0., 200., 30., 0.),
            ];
            let e = scene("A<&\"\nB", 24., [200., 150.], style.clone());
            let source = e.project().to_json().unwrap();
            for (_, name) in FIELDS {
                assert!(
                    !source.contains(name),
                    "zero field must remain absent: {name}"
                );
            }
            assert!(
                serde_json::to_value(e.project()).unwrap()["version"]
                    .as_u64()
                    .unwrap()
                    < 55
            );
            let expected = raw_pixels(&literal_svg(24., [200., 150.], &style, &plan));
            compare_paths(e.project(), 0, &expected);
            if !paragraph {
                let dormant = TextStyle {
                    paragraph: false,
                    align,
                    ..styled()
                };
                let e = scene("A<&\"\nB", 24., [200., 150.], dormant);
                assert_eq!(serde_json::to_value(e.project()).unwrap()["version"], 55);
                assert_eq!(
                    crate::text_flow::fit_height(e.selected_layer().unwrap(), 0),
                    None
                );
                compare_paths(e.project(), 0, &expected);
            }
        }
    }
}

#[test]
fn paragraph_fit_and_point_conversion_keep_literal_visible_source_and_dormant_styles() {
    let mut e = scene("AAAA BBBB CCCC\r\nD\n\nE", 24., [200., 130.], styled());
    let before = e.project().clone();
    let mut expected = serde_json::to_value(&before).unwrap();
    expected["composition"]["layers"][0]["content"]["Text"]["text"] = json!("AAAA\nBBBB CCCC\nD");
    expected["composition"]["layers"][0]["text_style"]["paragraph"] = json!(false);
    let expected = Project::from_json(&expected.to_string()).unwrap();
    e.execute(crate::text_flow::convert(
        e.selected_layer().unwrap(),
        false,
        30,
    ))
    .unwrap();
    exact(
        e.project(),
        &expected,
        "literal current-frame Point conversion",
    );
    assert_eq!(serde_json::to_value(e.project()).unwrap()["version"], 55);
    e.undo();
    exact(
        e.project(),
        &before,
        "conversion one Undo restores hidden source",
    );
    assert!(!e.can_undo());
    e.redo();
    exact(e.project(), &expected, "conversion one Redo");
    let mut style = styled();
    style.paragraph = false;
    let plan = [
        line("AAAA", 0., 200., 0., 0.),
        line("BBBB CCCC", 0., 200., 30., 0.),
        line("D", 0., 200., 60., 0.),
    ];
    compare_paths(
        e.project(),
        30,
        &raw_pixels(&literal_svg(24., [200., 130.], &style, &plan)),
    );
}

#[test]
fn paragraph_all_five_fields_invalidate_flow_and_caret_geometry_cache_without_source_mutation() {
    let text = "A\nB";
    let style = styled();
    let base = crate::text_flow::lines(text, 24., 200., &style);
    assert!(std::sync::Arc::ptr_eq(
        &base,
        &crate::text_flow::lines(text, 24., 200., &style)
    ));
    for (field, _) in FIELDS {
        let mut changed = style.clone();
        match field {
            TextParagraphField::LeftIndent => changed.paragraph_left_indent += 1.,
            TextParagraphField::RightIndent => changed.paragraph_right_indent += 1.,
            TextParagraphField::FirstLineIndent => changed.paragraph_first_line_indent += 1.,
            TextParagraphField::SpaceBefore => changed.paragraph_space_before += 1.,
            TextParagraphField::SpaceAfter => changed.paragraph_space_after += 1.,
        }
        let flow = crate::text_flow::lines(text, 24., 200., &changed);
        assert!(!std::sync::Arc::ptr_eq(&base, &flow), "{field:?}");
        let layout = crate::text_edit::layout::Layout::shape(text, 24., 200., &changed);
        for (index, row) in flow.iter().enumerate() {
            let caret = layout.caret(row.range.start);
            assert_eq!(caret[1], row.y, "line {index} {field:?}");
            // Left-aligned A/B have an initial logical origin of zero.
            assert!(
                (caret[0] - row.x).abs() < 0.001,
                "line {index} {field:?}: {caret:?}"
            );
            assert_eq!(layout.hit([caret[0], caret[1] + 12.]), row.range.start);
        }
    }
}

fn animated_scene() -> Editor {
    let mut e = scene(
        "AA\nBB",
        24.,
        [220., 280.],
        TextStyle {
            paragraph_left_indent: 16.,
            paragraph_right_indent: 12.,
            paragraph_first_line_indent: 20.,
            paragraph_space_before: 8.,
            paragraph_space_after: 12.,
            ..styled()
        },
    );
    for (parameter, value) in [
        (TextParam::FontSize, 40.),
        (TextParam::Tracking, 120.),
        (TextParam::Leading, 1.75),
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
    for (frame, text) in [(30, "CC\n\nDD"), (60, "")] {
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
fn paragraph_animated_source_and_typography_match_literal_sampled_svg_at_boundaries() {
    let e = animated_scene();
    let original = e.project().clone();
    for frame in FRAMES {
        let t = frame.min(60) as f64 / 60.;
        let size = 24. + 16. * t;
        let style = TextStyle {
            tracking: 120. * t,
            leading: 1.25 + 0.5 * t,
            ..e.selected_layer().unwrap().text_style()
        };
        let step = size * style.leading + 20.;
        let plan = if frame < 30 {
            vec![
                line("AA", 36., 172., 8., 12.),
                line("BB", 36., 172., 8. + step, 12.),
            ]
        } else if frame < 60 {
            vec![
                line("CC", 36., 172., 8., 12.),
                line("", 36., 172., 8. + step, 12.),
                line("DD", 36., 172., 8. + 2. * step, 12.),
            ]
        } else {
            vec![line("", 36., 172., 8., 12.)]
        };
        // These sampled values are literal interpolation arithmetic, not the
        // layer's typography or Source Text sampler.
        let expected = raw_pixels(&literal_svg(size, [220., 280.], &style, &plan));
        assert_eq!(expected.pixels().any(|p| p[3] > 0), frame < 60);
        compare_paths(e.project(), frame, &expected);
    }
    exact(
        e.project(),
        &original,
        "animation render is source-preserving",
    );
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
fn codec(state: &mut EditorState, expected: &Project) -> ProjectViews {
    let views = state.capture_views();
    let view = views.encode_native(expected).unwrap();
    let bytes = project_file::encode(state.editor.project(), Some(&view)).unwrap();
    assert_eq!(&bytes[8..10], &[1, 0], "LEP container version remains 1");
    let decoded = project_file::decode(&bytes).unwrap();
    exact(&decoded.project, expected, "official LEP full source");
    exact(
        &Project::from_json(&expected.to_json().unwrap()).unwrap(),
        expected,
        "official JSON full source",
    );
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
    views
}
fn expected_field(project: &Project, name: &str, value: f64) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    wire["version"] = json!(55);
    if value == 0. {
        wire["composition"]["layers"][0]["text_style"]
            .as_object_mut()
            .unwrap()
            .remove(name);
    } else {
        wire["composition"]["layers"][0]["text_style"][name] = json!(value);
    }
    Project::from_json(&wire.to_string()).unwrap()
}
#[test]
fn paragraph_field_edits_are_literal_source_exact_and_preserve_history_tracks_and_view() {
    let before = animated_scene().project().clone();
    for ((field, name), value) in FIELDS.into_iter().zip([24., 18., -8., 14., 16.]) {
        let mut s = state(&before, false);
        let view = s.capture_views();
        let expected = expected_field(&before, name, value);
        s.bulk_test_action(&Action::Edit(Command::SetTextParagraphValue {
            id: 1,
            field,
            value,
        }));
        exact(s.editor.project(), &expected, name);
        assert_eq!(
            s.capture_views(),
            view,
            "editing a static style preserves VIEW"
        );
        codec(&mut s, &expected);
        s.bulk_test_action(&Action::Undo);
        exact(s.editor.project(), &before, "one Undo");
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        let original = field.value(&before.composition().layer(1).unwrap().text_style());
        assert!(
            s.editor
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .text_paragraph_value_command(field, original)
                .unwrap()
                .is_none()
        );
        s.bulk_test_action(&Action::Edit(Command::SetTextParagraphValue {
            id: 1,
            field,
            value: original,
        }));
        exact(
            s.editor.project(),
            &before,
            "equal explicit command preserves source and Redo",
        );
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        s.bulk_test_action(&Action::Redo);
        exact(s.editor.project(), &expected, "one Redo");
        assert!(!s.editor.can_redo());
        assert_eq!(s.capture_views(), view);
    }
}

#[test]
fn paragraph_unsupported_source_versions_nonfinite_bounds_and_nontargets_reject_atomically() {
    let e = animated_scene();
    let source = e.project().clone();
    for version in [3, 48, 49, 50, 54, u32::MAX] {
        let mut wire = serde_json::to_value(&source).unwrap();
        wire["version"] = json!(version);
        assert!(
            Project::from_json(&wire.to_string()).is_err(),
            "unsupported schema {version}"
        );
        let unchecked: Project = serde_json::from_value(wire).unwrap();
        assert!(project_file::encode(&unchecked, None).is_err());
    }
    for (field, name) in FIELDS {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            16384.001,
            -16384.001,
        ] {
            let mut e = animated_scene();
            e.execute(Command::SetTextParagraphValue {
                id: 1,
                field,
                value: 42.,
            })
            .unwrap();
            e.undo();
            let before = e.project().clone();
            assert!(e.can_redo());
            assert!(
                e.execute(Command::SetTextParagraphValue {
                    id: 1,
                    field,
                    value
                })
                .is_err(),
                "{name}: {value}"
            );
            exact(e.project(), &before, "invalid field source rollback");
            assert!(!e.can_undo());
            assert!(e.can_redo());
        }
        if field != TextParagraphField::FirstLineIndent {
            let mut wire = serde_json::to_value(&source).unwrap();
            wire["composition"]["layers"][0]["text_style"][name] = json!(-0.001);
            assert!(Project::from_json(&wire.to_string()).is_err());
        }
    }
    let mut e = animated_scene();
    e.execute(Command::AddContent {
        content: Content::Solid,
        width: 50.,
        height: 50.,
        name: "Unrelated".into(),
    })
    .unwrap();
    e.clear_history();
    let before = e.project().clone();
    for id in [2, 999] {
        assert!(
            e.execute(Command::SetTextParagraphValue {
                id,
                field: TextParagraphField::LeftIndent,
                value: 10.
            })
            .is_err()
        );
        exact(e.project(), &before, "nontext/missing source rollback");
        assert!(!e.can_undo());
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
/// Generated fixtures are immutable input and independent expectations only.
#[test]
#[ignore = "requires an explicit absolute fixture output directory"]
fn export_paragraph_style_acceptance_fixtures() {
    let root =
        std::env::var_os("LIBREEFFECTS_EXPORT_PARAGRAPH_FIXTURES").expect("set fixture directory");
    let root = Path::new(&root);
    let before = scene(
        "AAAA BBBB CCCC\nD\n\nE",
        24.,
        [200., 260.],
        TextStyle {
            paragraph: true,
            leading: 1.25,
            ..Default::default()
        },
    )
    .project()
    .clone();
    let views = state(&before, true).capture_views();
    export(root, "native-before", &before, &views);
    // These retain their actual pre-55 schema and omit every new field. They
    // can be rendered with the frozen previous release for real compatibility
    // comparisons rather than an inferred old-renderer result.
    for paragraph in [false, true] {
        for (align, label) in [
            (TextAlign::Left, "left"),
            (TextAlign::Center, "center"),
            (TextAlign::Right, "right"),
        ] {
            let old = scene(
                "AAAA BBBB CCCC\r\nD\n\nE\n",
                24.,
                [200., 260.],
                TextStyle {
                    paragraph,
                    align,
                    leading: 1.25,
                    ..Default::default()
                },
            )
            .project()
            .clone();
            assert!(
                serde_json::to_value(&old).unwrap()["version"]
                    .as_u64()
                    .unwrap()
                    < 55
            );
            for (_, name) in FIELDS {
                assert!(!old.to_json().unwrap().contains(name));
            }
            export(
                root,
                &format!(
                    "legacy-{}-{label}",
                    if paragraph { "paragraph" } else { "point" }
                ),
                &old,
                &views,
            );
        }
    }
    let mut expected = before;
    for (((field, name), value), label) in FIELDS
        .into_iter()
        .zip([24., 16., 12., 10., 14.])
        .zip(["left", "right", "first", "before", "after"])
    {
        let next = expected_field(&expected, name, value);
        let mut e = Editor::default();
        e.replace_project(expected).unwrap();
        e.execute(Command::SetTextParagraphValue {
            id: 1,
            field,
            value,
        })
        .unwrap();
        exact(e.project(), &next, "literal generated expectation");
        export(root, &format!("native-{label}-expected"), &next, &views);
        expected = next;
    }
    export(root, "animated-source", animated_scene().project(), &views);
    let mut dormant = serde_json::to_value(&expected).unwrap();
    dormant["composition"]["layers"][0]["text_style"]["paragraph"] = json!(false);
    export(
        root,
        "dormant-point",
        &Project::from_json(&dormant.to_string()).unwrap(),
        &views,
    );
}
/// Requires separately recorded native Save/Open evidence; it never creates it.
#[test]
#[ignore = "requires separately recorded native save and independent expected fixture"]
fn verify_recorded_native_paragraph_style_save() {
    let actual_path = std::env::var_os("LIBREEFFECTS_PARAGRAPH_NATIVE_SAVE")
        .expect("set actual native save path");
    let expected_path = std::env::var_os("LIBREEFFECTS_PARAGRAPH_EXPECTED")
        .expect("set independent generated LEP path");
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_PARAGRAPH_EXPECTED_FRAME") {
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
        let reference = renderer.render(&expected.project, frame, WIDTH).unwrap();
        compare_paths(&actual.project, frame, &reference);
    }
    println!(
        "Exact full source, VIEW and {} native preview/output/JSON/LEP frame sets: {}",
        FRAMES.len(),
        Path::new(&actual_path).display()
    );
}
