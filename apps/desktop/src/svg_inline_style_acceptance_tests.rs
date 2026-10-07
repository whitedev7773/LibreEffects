//! Independent bounded inline-CSS acceptance, separate from the immutable first
//! SVG corpus. Literal style, attribute-only and cubic files are authored inputs;
//! no imported tree is used to derive a semantic or pixel reference.
use crate::{rendering::Renderer, view_state::ProjectViews};
use libre_effects_core::*;
use serde_json::json;
use std::{io::Cursor, path::Path};

const WIDTH: u32 = 240;
const HEIGHT: u32 = 160;
const FRAMES: [Frame; 3] = [0, 30, 60];

#[derive(Clone, Copy)]
struct Fixture {
    name: &'static str,
    svg: &'static str,
    attributes: &'static str,
    cubic: &'static str,
}
macro_rules! fixture {
    ($name:literal) => {
        Fixture {
            name: $name,
            svg: include_str!(concat!("../fixtures/svg_inline_styles/", $name, ".svg")),
            attributes: include_str!(concat!(
                "../fixtures/svg_inline_styles/attributes/",
                $name,
                ".svg"
            )),
            cubic: include_str!(concat!(
                "../fixtures/svg_inline_styles/cubic-references/",
                $name,
                ".svg"
            )),
        }
    };
}
const FIXTURES: [Fixture; 6] = [
    fixture!("precedence"),
    fixture!("inheritance"),
    fixture!("opacity-isolation"),
    fixture!("stroke-dashes"),
    fixture!("case-rgb"),
    fixture!("empty-whitespace"),
];

fn blank() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "SVG inline style independent acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor.clear_history();
    editor
}
fn insert(editor: &mut Editor, source: &str, name: &str) {
    let parsed = crate::svg_import::parse(source.as_bytes())
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!((parsed.width, parsed.height), (WIDTH as f64, HEIGHT as f64));
    parsed.contents.validate(90).unwrap();
    editor
        .execute(Command::ImportSvg {
            contents: parsed.contents,
            width: parsed.width,
            height: parsed.height,
            name: name.into(),
        })
        .unwrap();
}
fn source_equal(actual: &Project, expected: &Project, label: &str) {
    assert_eq!(actual, expected, "{label}");
    assert_eq!(
        actual.to_json().unwrap(),
        expected.to_json().unwrap(),
        "{label}"
    );
}
fn literal_pixels(source: &str) -> image::RgbaImage {
    // All fixtures have the full 240x160 viewport. A nested <svg> wrapper
    // would exercise the pinned rasterizer's separate nested-root semantics.
    let tree = resvg::usvg::Tree::from_str(source, &resvg::usvg::Options::default())
        .expect("literal reference must render independently");
    let mut pixels = resvg::tiny_skia::Pixmap::new(WIDTH, HEIGHT).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
    );
    image::RgbaImage::from_raw(
        WIDTH,
        HEIGHT,
        pixels
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
fn pixels_equal(actual: &image::RgbaImage, expected: &image::RgbaImage, label: &str) {
    assert_eq!(actual.dimensions(), expected.dimensions(), "{label}");
    let changed = actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, b)| a != b)
        .count();
    let maximum = actual
        .as_raw()
        .iter()
        .zip(expected.as_raw())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert_eq!(
        changed, 0,
        "{label}: {changed} changed pixels; max RGBA channel delta {maximum}"
    );
}
fn compare_routes(project: &Project, pixels: &image::RgbaImage, label: &str) {
    let renderer = Renderer::new();
    let source = project.to_json().unwrap();
    let encoded = project_file::encode(project, None).unwrap();
    let json = Project::from_json(&source).unwrap();
    let lep = project_file::decode(&encoded).unwrap().project;
    source_equal(&json, project, "JSON preserves complete source");
    source_equal(&lep, project, "LEP preserves complete source");
    for frame in FRAMES {
        for (route, actual) in [
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
            ("JSON", renderer.render(&json, frame, WIDTH).unwrap()),
            ("LEP", renderer.render(&lep, frame, WIDTH).unwrap()),
        ] {
            pixels_equal(&actual, pixels, &format!("{label} {route} frame {frame}"));
        }
    }
    assert_eq!(
        project.to_json().unwrap(),
        source,
        "rendering preserves source"
    );
    assert_eq!(project_file::encode(project, None).unwrap(), encoded);
}
fn accepted_pair(style: &str, attributes: &str, label: &str) {
    let actual =
        crate::svg_import::parse(style.as_bytes()).unwrap_or_else(|e| panic!("{label}: {e}"));
    let expected = crate::svg_import::parse(attributes.as_bytes())
        .unwrap_or_else(|e| panic!("{label} attribute reference: {e}"));
    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height),
        "{label}"
    );
    assert_eq!(
        actual.contents, expected.contents,
        "{label}: exact editable source tree"
    );
}
fn acceptance(fixture: Fixture) {
    accepted_pair(fixture.svg, fixture.attributes, fixture.name);
    let pixels = literal_pixels(fixture.cubic);
    assert!(pixels.pixels().any(|p| p[3] != 0), "nonempty oracle");
    pixels_equal(
        &literal_pixels(fixture.attributes),
        &pixels,
        &format!("{} attribute/cubic literals", fixture.name),
    );
    pixels_equal(
        &literal_pixels(fixture.svg),
        &pixels,
        &format!("{} original CSS/cubic literals", fixture.name),
    );
    let mut actual = blank();
    let mut expected = blank();
    insert(&mut actual, fixture.svg, fixture.name);
    insert(&mut expected, fixture.attributes, fixture.name);
    source_equal(actual.project(), expected.project(), fixture.name);
    compare_routes(actual.project(), &pixels, fixture.name);
}
macro_rules! literal_case {
    ($name:ident, $index:expr) => {
        #[test]
        fn $name() {
            acceptance(FIXTURES[$index]);
        }
    };
}
literal_case!(inline_svg_literal_attribute_precedence_both_xml_orders, 0);
literal_case!(inline_svg_literal_paint_and_stroke_inheritance, 1);
literal_case!(inline_svg_literal_local_opacity_and_group_isolation, 2);
literal_case!(inline_svg_literal_dash_geometry_and_paint_order, 3);
literal_case!(inline_svg_literal_rgb_and_last_valid_declaration, 4);
literal_case!(
    inline_svg_literal_empty_declarations_and_ascii_whitespace,
    5
);

#[test]
fn inline_svg_independent_oracles_have_distinct_visible_semantic_witnesses() {
    let source = FIXTURES[2].attributes;
    let pixels = literal_pixels(source);
    // Root opacity 0.8 times one isolated group opacity 0.5. The nested g and
    // both children default to opacity 1; opacity is not an inherited paint.
    assert_eq!(pixels.get_pixel(24, 30)[3], 102);
    assert_eq!(pixels.get_pixel(54, 66)[3], 102);
    // fill-opacity IS inherited: separately translucent overlapping paints
    // produce higher alpha than either paint alone, before root opacity.
    assert_eq!(pixels.get_pixel(144, 30)[3], 102);
    // The two 0.5 paints quantize to alpha 128; their overlap quantizes
    // to 192, then the 0.8 root opacity yields 154 in the pinned rasterizer.
    assert_eq!(pixels.get_pixel(174, 66)[3], 154);
    assert_ne!(
        pixels,
        literal_pixels(&source.replace("opacity=\"0.5\"", "opacity=\"1\""))
    );
    let stroke = literal_pixels(FIXTURES[3].attributes);
    assert_ne!(
        stroke,
        literal_pixels(
            &FIXTURES[3]
                .attributes
                .replace("stroke-dasharray=\"18 9\"", "stroke-dasharray=\"none\"")
        )
    );
    assert_ne!(
        stroke,
        literal_pixels(
            &FIXTURES[3]
                .attributes
                .replace("stroke-dashoffset=\"6\"", "stroke-dashoffset=\"0\"")
        )
    );
    assert_ne!(
        stroke,
        literal_pixels(&FIXTURES[3].attributes.replace(
            "id=\"paint-order\"",
            "id=\"paint-order\" paint-order=\"stroke fill\""
        ))
    );
    assert_ne!(
        literal_pixels(FIXTURES[1].attributes),
        literal_pixels(
            &FIXTURES[1]
                .attributes
                .replace("fill-rule=\"evenodd\"", "fill-rule=\"nonzero\"")
        )
    );
}

/// The pinned usvg attribute-name lookup is case-sensitive, and simplecss
/// DeclarationTokenizer stops on a leading empty declaration. Retain both
/// original literal witnesses byte-for-byte. They are valid importer inputs,
/// but their raw CSS raster cannot serve as its semantic oracle. This test uses
/// independently authored attribute/cubic references and requires exact native
/// output; the separate raw deltas characterize the dependency, not a tolerance.
#[test]
fn inline_svg_original_case_and_leading_empty_declarations_have_independent_oracles() {
    for (source, fixture, expected_delta, reason) in [
        (
            include_str!("../fixtures/svg_inline_styles/renderer-limitations/case-rgb.svg"),
            FIXTURES[4],
            (6553, 255),
            "pinned usvg case-sensitive presentation lookup and values",
        ),
        (
            include_str!("../fixtures/svg_inline_styles/renderer-limitations/empty-whitespace.svg"),
            FIXTURES[5],
            (6084, 191),
            "pinned simplecss stops on leading semicolon",
        ),
    ] {
        accepted_pair(source, fixture.attributes, reason);
        accepted_pair(source, fixture.svg, "canonical inline source equivalence");
        let expected = literal_pixels(fixture.cubic);
        let raw = literal_pixels(source);
        let changed = raw
            .pixels()
            .zip(expected.pixels())
            .filter(|(a, b)| a != b)
            .count();
        let maximum = raw
            .as_raw()
            .iter()
            .zip(expected.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert_eq!(
            (changed, maximum),
            expected_delta,
            "exact full-image pinned CSS limitation: {reason}"
        );
        let mut editor = blank();
        insert(&mut editor, source, fixture.name);
        compare_routes(editor.project(), &expected, reason);
    }
}

#[test]
fn inline_svg_import_is_one_undo_step_with_source_and_redo_preservation() {
    for fixture in FIXTURES {
        let mut editor = blank();
        // Existing painted source must survive appending a separate import.
        insert(
            &mut editor,
            FIXTURES[0].attributes,
            "Existing attribute-only layer",
        );
        editor.clear_history();
        let before = editor.project().clone();
        insert(&mut editor, fixture.svg, fixture.name);
        let after = editor.project().clone();
        assert_eq!(
            &after.composition().layers()[1],
            &before.composition().layers()[0]
        );
        for property in [
            Property::PositionX,
            Property::PositionY,
            Property::AnchorX,
            Property::AnchorY,
        ] {
            assert_eq!(
                after.composition().layers()[0]
                    .property(property)
                    .expect("2D fixture property")
                    .value_at(0),
                0.
            );
        }
        assert_eq!(after.composition().layers().len(), 2);
        assert_ne!(
            after.composition().layers()[0].id(),
            after.composition().layers()[1].id()
        );
        assert!(editor.can_undo());
        editor.undo();
        source_equal(
            editor.project(),
            &before,
            "one Undo restores existing source",
        );
        assert!(!editor.can_undo());
        assert!(editor.can_redo());
        let malformed =
            wrapped("<path d='M1 1 L40 40' style='fill:red;fill:url(#missing);fill:blue'/>");
        rejected(&malformed, "later good declaration cannot hide resource");
        source_equal(
            editor.project(),
            &before,
            "rejection leaves source unchanged",
        );
        assert!(!editor.can_undo());
        assert!(editor.can_redo());
        editor.redo();
        source_equal(
            editor.project(),
            &after,
            "one Redo restores exact imported source",
        );
        assert!(!editor.can_redo());
        let parsed = crate::svg_import::parse(fixture.svg.as_bytes()).unwrap();
        assert_eq!(
            parsed.contents,
            crate::svg_import::parse(fixture.svg.as_bytes())
                .unwrap()
                .contents
        );
        let rows = parsed.contents.rows();
        let ids: std::collections::BTreeSet<_> = rows.iter().map(|(_, _, n)| n.id).collect();
        assert_eq!(ids.len(), rows.len());
        assert!(rows.iter().all(|(_, _, n)| n.id > 0
            && matches!(
                n.kind,
                ContentsKind::Group(_)
                    | ContentsKind::Path { .. }
                    | ContentsKind::Fill { .. }
                    | ContentsKind::Stroke(_)
            )));
    }
}
fn wrapped(body: &str) -> String {
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='240' height='160'>{body}</svg>")
}
fn shape(style: &str) -> String {
    wrapped(&format!("<rect width='24' height='24' style='{style}'/>"))
}
fn rejected(source: &str, label: &str) {
    match crate::svg_import::parse(source.as_bytes()) {
        Ok(_) => panic!("strict SVG accepted unsupported source: {label}"),
        Err(error) => assert!(
            !error.trim().is_empty(),
            "missing rejection diagnostic: {label}"
        ),
    }
}

#[test]
fn inline_svg_styles_work_on_every_supported_drawing_element_and_root_and_group() {
    for geometry in [
        "path d='M12 12 L72 12 L72 72 Z'",
        "rect x='12' y='12' width='60' height='60'",
        "circle cx='42' cy='42' r='30'",
        "ellipse cx='42' cy='42' rx='30' ry='24'",
        "line x1='12' y1='12' x2='72' y2='72'",
        "polyline points='12 12 72 12 72 72'",
        "polygon points='12 12 72 12 72 72'",
    ] {
        accepted_pair(
            &wrapped(&format!(
                "<{geometry} style='fill:red;stroke:blue;stroke-width:3'/>"
            )),
            &wrapped(&format!(
                "<{geometry} fill='red' stroke='blue' stroke-width='3'/>"
            )),
            geometry,
        );
    }
    for body in [
        "<title style=''>text</title>",
        "<desc style='fill:red'>text</desc>",
    ] {
        rejected(
            &wrapped(&format!("{body}<rect width='24' height='24'/>")),
            body,
        );
    }
}

#[test]
fn inline_svg_all_twelve_properties_obey_xml_order_and_repeated_declaration_rules() {
    for (property, before, after) in [
        ("fill", "red", "blue"),
        ("stroke", "red", "blue"),
        ("fill-rule", "nonzero", "evenodd"),
        ("fill-opacity", "0.3", "0.7"),
        ("stroke-opacity", "0.3", "0.7"),
        ("stroke-width", "3", "6"),
        ("stroke-linecap", "butt", "square"),
        ("stroke-linejoin", "miter", "round"),
        ("stroke-miterlimit", "2", "4"),
        ("stroke-dasharray", "3 6", "12 3"),
        ("stroke-dashoffset", "-3", "6"),
        ("opacity", "0.3", "0.7"),
    ] {
        let paint = if property == "stroke" {
            ""
        } else {
            "stroke='black'"
        };
        let expected = wrapped(&format!(
            "<rect width='24' height='24' {paint} {property}='{after}'/>"
        ));
        for attributes in [
            format!("{property}='{before}' style='{property}:{after}'"),
            format!("style='{property}:{after}' {property}='{before}'"),
            format!(
                "{property}='{before}' style='{property}:{before};{}:{after}'",
                property.to_ascii_uppercase()
            ),
        ] {
            accepted_pair(
                &wrapped(&format!(
                    "<rect width='24' height='24' {paint} {attributes}/>"
                )),
                &expected,
                property,
            );
        }
    }
}

#[test]
fn inline_svg_never_hides_bad_presentation_attributes_or_earlier_bad_declarations() {
    for (property, bad, good) in [
        ("fill", "url(#paint)", "red"),
        ("stroke", "currentColor", "blue"),
        ("fill-rule", "invented", "evenodd"),
        ("fill-opacity", "-1", "0.5"),
        ("stroke-opacity", "NaN", "0.5"),
        ("stroke-width", "-1", "3"),
        ("stroke-linecap", "invented", "butt"),
        ("stroke-linejoin", "invented", "miter"),
        ("stroke-miterlimit", "0", "4"),
        ("stroke-dasharray", "-1 2", "3 6"),
        ("stroke-dashoffset", "1000001", "0"),
        ("opacity", "1.1", "0.5"),
    ] {
        for attributes in [
            format!("{property}='{bad}' style='{property}:{good}'"),
            format!("style='{property}:{good}' {property}='{bad}'"),
            format!("style='{property}:{bad};{property}:{good}'"),
            format!("style='{property}:{good};{property}:{bad}'"),
        ] {
            rejected(
                &wrapped(&format!("<rect width='24' height='24' {attributes}/>")),
                &attributes,
            );
        }
    }
    for style in [
        "stroke-width:1025;stroke-width:3",
        "stroke-miterlimit:1025;stroke-miterlimit:4",
        "stroke-dasharray:8193 2;stroke-dasharray:none",
        "fill:rgba(1,2,3,1);fill:red",
        "fill:#1234;fill:red",
        "fill:rgb(1,2,3,);fill:red",
        "fill:rgb(10%,20,30%);fill:red",
    ] {
        rejected(&shape(style), style);
    }
}

#[test]
fn inline_svg_rejects_nonliteral_css_grammar_selectors_and_resource_mechanisms() {
    for style in [
        "fill",
        ":red",
        "fill:",
        "fill:red:blue",
        "fill:red stroke:blue",
        "fill:red;;unknown:value",
        "fill:red !important",
        "fill:red!important",
        "fill:red !IMPORTANT",
        "/* comment */fill:red",
        "fill:/* comment */red",
        "fill:red/* comment */",
        "f\\69ll:red",
        "fill:r\\65d",
        "fill:&#34;red&#34;",
        "fill:&apos;red&apos;",
        "--paint:red;fill:red",
        "fill:var(--paint)",
        "stroke-width:calc(1 + 2)",
        "fill:url(#paint)",
        "fill:URL(https://invalid.example/paint.svg#x)",
        "stroke:url(data:image/svg+xml;base64,PHN2Zz4=)",
        "fill:rgb(1,2,3)url(#x)",
        "fill:rgba(1,2,3,1)",
        "fill:hsl(0,100%,50%)",
        "fill:rgb(1 2 3 / 1)",
        "fill:rgb(1+2+3)",
        "fill:rgb(1,2,3,)",
        "fill:rgb(10%20%30%)",
        "fill:{red}",
        "rect { fill:red }",
        "@import url(https://invalid.example/x.css)",
        "transform:translate(1,2)",
        "d:M0 0L20 20",
        "width:20px",
        "x:1",
        "r:12",
        "color:red",
        "display:none",
        "visibility:hidden",
        "paint-order:stroke fill",
        "clip-path:url(#x)",
        "mask:url(#x)",
        "filter:url(#x)",
        "marker-end:url(#x)",
        "font-family:serif",
        "vector-effect:non-scaling-stroke",
        "mix-blend-mode:multiply",
        "ﬁll:red",
        "fill：red",
        "fill:red\u{a0}",
    ] {
        rejected(&shape(style), style);
    }
    for property in [
        "fill",
        "stroke",
        "fill-rule",
        "fill-opacity",
        "stroke-opacity",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "stroke-miterlimit",
        "stroke-dasharray",
        "stroke-dashoffset",
        "opacity",
    ] {
        for value in [
            "inherit",
            "currentColor",
            "initial",
            "unset",
            "revert",
            "revert-layer",
        ] {
            let style = format!("{property}:{value}");
            rejected(&shape(&style), &style);
        }
    }
    for body in [
        "<style>rect { fill:red }</style><rect width='24' height='24'/>",
        "<rect width='24' height='24' class='paint' style='fill:red'/>",
        "<rect width='24' height='24' style='fill:red' href='#missing'/>",
        "<rect width='24' height='24' style='fill:red' onclick='run()'/>",
        "<g xmlns:q='https://invalid.example' q:style='fill:red'><rect width='24' height='24'/></g>",
    ] {
        rejected(&wrapped(body), body);
    }
}

#[test]
fn inline_svg_numeric_tokens_are_complete_bounded_css_numbers() {
    for style in [
        "opacity:1.",
        "fill-opacity:.5.",
        "stroke-width:1.px",
        "stroke-width:1..0",
        "stroke-dasharray:1. 2",
        "fill:rgb(1.,2,3)",
        "fill:rgb(1,2 3)",
        "fill:rgb(1 2,3)",
        "opacity:1e-999",
        "stroke-width:1e-999",
        "stroke-width:1e-46",
        "stroke-dashoffset:1e-999",
        "stroke-dasharray:1e-999 2",
        "fill:rgb(1e-999,2,3)",
        "opacity:1e999",
        "opacity:NaN",
        "stroke-width:Infinity",
        "stroke-width:1e",
        "stroke-width:1e+",
        "stroke-width:+",
        "stroke-width:.",
        "stroke-width:3 px",
        "stroke-width:3em",
        "stroke-width:3%",
        "stroke-miterlimit:4px",
    ] {
        rejected(&shape(style), style);
    }
    accepted_pair(
        &shape(
            "opacity:+1;fill-opacity:.5;stroke:blue;stroke-opacity:5e-1;stroke-width:3E+0px;stroke-miterlimit:+4;stroke-dasharray:3e0 6e0;stroke-dashoffset:-3e0;fill:rgb(12, 96, 168)",
        ),
        &wrapped(
            "<rect width='24' height='24' opacity='1' fill-opacity='0.5' stroke='blue' stroke-opacity='0.5' stroke-width='3px' stroke-miterlimit='4' stroke-dasharray='3 6' stroke-dashoffset='-3' fill='#0c60a8'/>",
        ),
        "sign decimal and scientific CSS numeric tokens",
    );
    accepted_pair(
        &shape("fill:rgb(12 96 168)"),
        &wrapped("<rect width='24' height='24' fill='#0c60a8'/>"),
        "three whitespace-separated RGB channels",
    );
    accepted_pair(
        &shape("stroke:black;stroke-dasharray:0 0"),
        &wrapped("<rect width='24' height='24' stroke='black' stroke-dasharray='0 0'/>"),
        "existing zero-sum dash arrays remain valid solid strokes",
    );
}

#[test]
fn inline_svg_exact_style_byte_declaration_and_document_work_boundaries() {
    let prefix = "fill:red;";
    let exact_bytes = format!("{prefix}{}", " ".repeat(16 * 1024 - prefix.len()));
    accepted_pair(
        &shape(&exact_bytes),
        &wrapped("<rect width='24' height='24' fill='red'/>"),
        "16 KiB style exactly",
    );
    rejected(&shape(&(exact_bytes + " ")), "16 KiB plus one style byte");
    let declarations = "fill:red;".repeat(128);
    accepted_pair(
        &shape(&declarations),
        &wrapped("<rect width='24' height='24' fill='red'/>"),
        "128 declarations exactly",
    );
    rejected(
        &shape(&(declarations.clone() + "fill:red")),
        "129 declarations",
    );
    // Empty semicolon fields are no declarations, but still cost style bytes.
    accepted_pair(
        &shape(&format!("{}{}", ";".repeat(256), declarations)),
        &wrapped("<rect width='24' height='24' fill='red'/>"),
        "empty declarations do not consume declaration quota",
    );
    let element = format!(
        "<rect width='24' height='24' style='{}'/>",
        "fill:red;".repeat(128)
    );
    let body = element.repeat(16);
    let attrs = "<rect width='24' height='24' fill='red'/>".repeat(16);
    accepted_pair(
        &wrapped(&body),
        &wrapped(&attrs),
        "2048 document declarations exactly",
    );
    rejected(
        &wrapped(&(body + "<rect width='24' height='24' style='fill:red'/>")),
        "2049 document declarations",
    );
}

#[test]
fn inline_svg_regular_file_reader_and_parser_agree_and_rejections_are_atomic() {
    let root = tempfile::tempdir().unwrap();
    for fixture in FIXTURES {
        let path = root.path().join(format!("{}.svg", fixture.name));
        std::fs::write(&path, fixture.svg).unwrap();
        let read = crate::svg_import::read_svg_file(&path).unwrap();
        let parsed = crate::svg_import::parse(fixture.svg.as_bytes()).unwrap();
        assert_eq!(read.contents, parsed.contents);
        assert_eq!((read.width, read.height), (parsed.width, parsed.height));
    }
    let source = wrapped(
        "<rect width='24' height='24' style='fill:green'/><rect width='24' height='24' style='fill:red;fill:url(#x);fill:blue'/>",
    );
    let path = root.path().join("later-bad-element.svg");
    std::fs::write(&path, &source).unwrap();
    rejected(&source, "later bad style rejects whole document");
    assert!(crate::svg_import::read_svg_file(&path).is_err());
}

fn immutable(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => file.write_all(bytes).unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => assert_eq!(
            std::fs::read(path).unwrap(),
            bytes,
            "immutable {}",
            path.display()
        ),
        Err(e) => panic!("{}: {e}", path.display()),
    }
}
fn export_project(root: &Path, label: &str, project: &Project) {
    let mut views = ProjectViews::default();
    // The app explicitly saves the active composition's frame-zero default
    // view. normalize() only validates existing entries; it never adds one.
    // Author this expectation before any native recording is inspected.
    views
        .compositions
        .insert(project.active_composition_id(), Default::default());
    views.normalize(project);
    immutable(
        &root.join(format!("{label}.lfe.json")),
        project.to_json().unwrap().as_bytes(),
    );
    immutable(
        &root.join(format!("{label}.generated.lep")),
        &project_file::encode(project, Some(&views.encode_native(project).unwrap())).unwrap(),
    );
}
#[test]
fn inline_svg_export_authors_exact_active_composition_default_view() {
    let root = tempfile::tempdir().unwrap();
    let mut cases = vec![("before-import", blank())];
    for fixture in FIXTURES {
        let mut editor = blank();
        insert(&mut editor, fixture.svg, fixture.name);
        cases.push((fixture.name, editor));
    }
    for (label, editor) in cases {
        let project = editor.project();
        let before = project.to_json().unwrap();
        export_project(root.path(), label, project);
        let bytes = std::fs::read(root.path().join(format!("{label}.generated.lep"))).unwrap();
        let decoded = project_file::decode(&bytes).unwrap();
        source_equal(&decoded.project, project, "exported full source");
        // Independent, complete wire expectation: no normalizing actual VIEW,
        // dropping defaults, or reading a previously recorded native save.
        let expected = json!({
            "version": 1,
            "compositions": {
                (project.active_composition_id().to_string()): {
                    "frame": 0,
                    "timeline_start": 0,
                    "timeline_zoom": 1.0,
                    "preview_zoom": null,
                    "preview_pan": [0.0, 0.0],
                    "preview_resolution": 1,
                    "checkerboard": false,
                    "viewer": {
                        "rulers": false, "grid": false, "guides": true,
                        "safe": false, "snap_guides": true, "snap_grid": false,
                        "lock_guides": false, "grid_size": 100.0, "channel": "Rgb"
                    },
                    "graph_open": false,
                    "graph_view": {"speed": false, "height": null},
                    "expanded": true
                }
            },
            "workspace": {
                "fractions": [0.84, 0.2, 0.615, 0.615],
                "timeline_left": 560.0,
                "sidebar_expanded": [true, false, false, false],
                "extra_sidebar_expanded": [false, false, false],
                "effect_controls_open": false,
                "snapping": true,
                "align_to_selection": false
            }
        });
        let actual: serde_json::Value =
            serde_json::from_slice(decoded.view.expect("export must include VIEW")).unwrap();
        assert_eq!(actual, expected, "{label}: exact complete exported VIEW");
        assert_eq!(
            project.to_json().unwrap(),
            before,
            "export is source-neutral"
        );
        // Repeating an export must verify the frozen bytes, never rewrite them.
        export_project(root.path(), label, project);
        assert_eq!(
            std::fs::read(root.path().join(format!("{label}.generated.lep"))).unwrap(),
            bytes
        );
    }
}

fn export_png(path: &Path, pixels: &image::RgbaImage) {
    let mut bytes = Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    immutable(path, &bytes.into_inner());
}

/// Compatible with svg_import/verify_cli.py; all outputs use create_new and
/// byte comparison on an existing file. These are generated, not native saves.
#[test]
#[ignore = "requires LIBREEFFECTS_EXPORT_INLINE_SVG_FIXTURES absolute output directory"]
fn export_inline_svg_acceptance_fixtures() {
    let root =
        std::env::var_os("LIBREEFFECTS_EXPORT_INLINE_SVG_FIXTURES").expect("set output directory");
    let root = Path::new(&root);
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    export_project(root, "before-import", blank().project());
    let mut cases = Vec::new();
    for fixture in FIXTURES {
        acceptance(fixture);
        let mut editor = blank();
        insert(&mut editor, fixture.svg, fixture.name);
        export_project(root, fixture.name, editor.project());
        immutable(
            &root.join(format!("{}.svg", fixture.name)),
            fixture.svg.as_bytes(),
        );
        immutable(
            &root.join(format!("{}.attributes.svg", fixture.name)),
            fixture.attributes.as_bytes(),
        );
        immutable(
            &root.join(format!("{}.cubic.svg", fixture.name)),
            fixture.cubic.as_bytes(),
        );
        export_png(
            &root.join(format!("{}.raw.literal.png", fixture.name)),
            &literal_pixels(fixture.svg),
        );
        export_png(
            &root.join(format!("{}.literal.png", fixture.name)),
            &literal_pixels(fixture.cubic),
        );
        for frame in FRAMES {
            cases.push(json!({"label": fixture.name, "frame": frame,
                "input": format!("{}.generated.lep", fixture.name),
                "json": format!("{}.lfe.json", fixture.name),
                "source_svg": format!("{}.svg", fixture.name),
                "attribute_reference_svg": format!("{}.attributes.svg", fixture.name),
                "cubic_reference_svg": format!("{}.cubic.svg", fixture.name),
                "expected_png": format!("{}.literal.png", fixture.name),
                "raw_source_expected_png": format!("{}.raw.literal.png", fixture.name),
                "raw_source_changed_pixels": 0, "raw_source_max_channel_delta": 0}));
        }
    }
    immutable(&root.join("cases.json"), &serde_json::to_vec_pretty(&json!({
        "dimensions": [WIDTH, HEIGHT], "frames": FRAMES,
        "oracle": "independently authored literal inline, attribute-only and cubic SVG; exact full unmasked RGBA; complete imported source equality",
        "render_cases": cases
    })).unwrap());
    println!(
        "Exported {} independent inline SVG families, exact original/attribute/cubic pixels, and {} exact five-route frame sets. Generated files are not native-interaction evidence.",
        FIXTURES.len(),
        FIXTURES.len() * FRAMES.len()
    );
}

/// Reads actual separately recorded native Save/Open output. It does not create
/// actual saves, normalize VIEW, rename layers or patch expected source.
#[test]
#[ignore = "requires actual native save and generated expectation; SOURCE required for nonblank"]
fn verify_recorded_native_inline_svg_save() {
    let actual =
        std::env::var_os("LIBREEFFECTS_INLINE_SVG_NATIVE_SAVE").expect("set actual native save");
    let expected =
        std::env::var_os("LIBREEFFECTS_INLINE_SVG_EXPECTED").expect("set generated expected LEP");
    let actual = crate::project_io::read_editor_project(Path::new(&actual)).unwrap();
    let expected = crate::project_io::read_editor_project(Path::new(&expected)).unwrap();
    assert_eq!(actual.format, crate::project_io::ProjectFormat::Lep);
    source_equal(
        &actual.project,
        &expected.project,
        "actual native full source",
    );
    assert_eq!(actual.views, expected.views, "actual native full VIEW");
    let pixels = if let Some(path) = std::env::var_os("LIBREEFFECTS_INLINE_SVG_SOURCE") {
        let source = std::fs::read_to_string(path).unwrap();
        let fixture = FIXTURES
            .into_iter()
            .find(|f| f.svg == source)
            .expect("native source must be an unchanged inline literal fixture");
        assert_eq!(expected.project.composition().layers().len(), 1);
        acceptance(fixture);
        literal_pixels(fixture.cubic)
    } else {
        assert!(
            expected.project.composition().layers().is_empty(),
            "SOURCE is required for a nonempty native expectation"
        );
        image::RgbaImage::new(WIDTH, HEIGHT)
    };
    compare_routes(&actual.project, &pixels, "actual native inline SVG");
    println!(
        "Exact full native source and VIEW, {} five-route frame sets, independent literal/cubic oracle; no patched expectations.",
        FRAMES.len()
    );
}
