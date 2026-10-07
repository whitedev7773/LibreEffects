//! Bounded static SVG import acceptance. The pixel oracle is the literal source
//! SVG and separately authored mathematical cubic references, never an
//! importer-generated tree. Raw-source edge deltas are explicit, unmasked and
//! fixture-specific; every native rendering route must equal its reference exactly.
//! Generated files are immutable QA inputs; they are not native interaction.
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
    reference: &'static str,
    raw_delta: (usize, u8),
    width: f64,
    height: f64,
}
macro_rules! fixture {
    ($name:literal) => {
        Fixture {
            name: $name,
            svg: include_str!(concat!("../fixtures/svg_import/", $name, ".svg")),
            reference: include_str!(concat!(
                "../fixtures/svg_import/cubic-references/",
                $name,
                ".svg"
            )),
            raw_delta: (0, 0),
            width: WIDTH as f64,
            height: HEIGHT as f64,
        }
    };
    ($name:literal, $width:expr, $height:expr) => {
        Fixture {
            width: $width,
            height: $height,
            ..fixture!($name)
        }
    };
}
const FIXTURES: [Fixture; 11] = [
    Fixture {
        raw_delta: (4, 16),
        ..fixture!("primitives")
    },
    Fixture {
        raw_delta: (6, 1),
        ..fixture!("curves")
    },
    Fixture {
        raw_delta: (5, 16),
        ..fixture!("compound-winding")
    },
    fixture!("paint-order-opacity"),
    Fixture {
        raw_delta: (4, 16),
        ..fixture!("nested-affine")
    },
    fixture!("inherited-paint"),
    fixture!("viewbox-meet"),
    fixture!("viewbox-none"),
    fixture!("viewport-clipping", 160., 100.),
    Fixture {
        raw_delta: (1, 9),
        ..fixture!("viewbox-slice")
    },
    fixture!("stroke-styles"),
];

fn blank() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "SVG import independent acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor.clear_history();
    editor
}
fn import(editor: &mut Editor, fixture: Fixture) {
    let parsed = crate::svg_import::parse(fixture.svg.as_bytes())
        .unwrap_or_else(|error| panic!("{}: {error}", fixture.name));
    assert_eq!(
        (parsed.width, parsed.height),
        (fixture.width, fixture.height)
    );
    parsed.contents.validate(90).unwrap();
    editor
        .execute(Command::ImportSvg {
            contents: parsed.contents,
            width: parsed.width,
            height: parsed.height,
            name: fixture.name.into(),
        })
        .unwrap_or_else(|error| panic!("{} insertion: {error}", fixture.name));
}
fn exact(actual: &Project, expected: &Project, context: &str) {
    assert_eq!(actual, expected, "{context}");
    assert_eq!(
        actual.to_json().unwrap(),
        expected.to_json().unwrap(),
        "{context}"
    );
}
fn raw_pixels(source: &str, _width: f64, _height: f64) -> image::RgbaImage {
    // Import places the viewport at composition origin. A nested literal
    // viewport independently clips overflow before the composition boundary.
    let document = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'>{source}</svg>"
    );
    let tree = resvg::usvg::Tree::from_str(&document, &resvg::usvg::Options::default())
        .expect("literal source must render independently");
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
fn pixels_equal(actual: &image::RgbaImage, expected: &image::RgbaImage, context: &str) {
    assert_eq!(actual.dimensions(), expected.dimensions(), "{context}");
    let differences: Vec<_> = actual
        .enumerate_pixels()
        .filter_map(|(x, y, pixel)| {
            let expected = expected.get_pixel(x, y);
            (pixel != expected).then_some((x, y, pixel.0, expected.0))
        })
        .take(8)
        .collect();
    let changed = actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, b)| a != b)
        .count();
    let max_delta = actual
        .as_raw()
        .iter()
        .zip(expected.as_raw())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(
        differences.is_empty(),
        "{context}: {changed} changed pixels, max channel delta {max_delta}; first RGBA differences {differences:?}"
    );
}
/// These counts characterize only pinned literal fixtures, not a blanket visual
/// tolerance. Nothing is masked: every RGBA byte participates. A changed sample
/// must also lie on a nonuniform 3x3 neighborhood in the raw source raster.
fn raw_normalization_delta(
    actual: &image::RgbaImage,
    raw: &image::RgbaImage,
    expected: (usize, u8),
    label: &str,
) {
    assert_eq!(actual.dimensions(), raw.dimensions());
    let mut changed = 0;
    let mut maximum = 0;
    for (x, y, pixel) in actual.enumerate_pixels() {
        let before = raw.get_pixel(x, y);
        if pixel != before {
            changed += 1;
            maximum = maximum.max(
                pixel
                    .0
                    .iter()
                    .zip(before.0)
                    .map(|(a, b)| a.abs_diff(b))
                    .max()
                    .unwrap(),
            );
            let edge = (y.saturating_sub(1)..=(y + 1).min(HEIGHT - 1)).any(|ny| {
                (x.saturating_sub(1)..=(x + 1).min(WIDTH - 1))
                    .any(|nx| raw.get_pixel(nx, ny) != before)
            });
            assert!(
                edge,
                "{label}: raw-source difference inside a uniform region at {x},{y}"
            );
        }
    }
    assert_eq!(
        (changed, maximum),
        expected,
        "{label}: exact declared unmasked raw-source cubic-normalization delta"
    );
}

fn compare_routes(project: &Project, expected: &image::RgbaImage, label: &str) {
    let renderer = Renderer::new();
    let source = project.to_json().unwrap();
    let json = Project::from_json(&source).unwrap();
    let bytes = project_file::encode(project, None).unwrap();
    let lep = project_file::decode(&bytes).unwrap().project;
    exact(&json, project, "JSON exact source");
    exact(&lep, project, "LEP exact source");
    for frame in FRAMES {
        for (route, pixels) in [
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
            pixels_equal(
                &pixels,
                expected,
                &format!("{label}: {route} frame {frame}"),
            );
        }
    }
    assert_eq!(
        project.to_json().unwrap(),
        source,
        "render is source-neutral"
    );
    assert_eq!(project_file::encode(project, None).unwrap(), bytes);
}

fn literal_acceptance(fixture: Fixture) {
    let mut editor = blank();
    import(&mut editor, fixture);
    let raw = raw_pixels(fixture.svg, fixture.width, fixture.height);
    let expected = raw_pixels(fixture.reference, fixture.width, fixture.height);
    raw_normalization_delta(&expected, &raw, fixture.raw_delta, fixture.name);
    assert!(
        expected.pixels().any(|p| p[3] > 0),
        "nonempty {} oracle",
        fixture.name
    );
    compare_routes(editor.project(), &expected, fixture.name);
}
macro_rules! literal_case {
    ($name:ident, $index:expr) => {
        #[test]
        fn $name() {
            literal_acceptance(FIXTURES[$index]);
        }
    };
}
literal_case!(svg_import_literal_primitives, 0);
literal_case!(svg_import_literal_curves_and_arcs, 1);
literal_case!(svg_import_literal_compound_winding, 2);
literal_case!(svg_import_literal_paint_order_opacity, 3);
literal_case!(svg_import_literal_nested_affine_strokes, 4);
literal_case!(svg_import_literal_inherited_paint, 5);
literal_case!(svg_import_literal_viewbox_meet, 6);
literal_case!(svg_import_literal_viewbox_none, 7);
literal_case!(svg_import_literal_viewport_clipping, 8);
literal_case!(svg_import_literal_viewbox_slice, 9);
literal_case!(svg_import_literal_stroke_styles_and_dashes, 10);

#[test]
fn svg_import_literal_oracles_distinguish_winding_paint_order_and_isolated_opacity() {
    let winding = FIXTURES[2];
    let pixels = raw_pixels(winding.svg, winding.width, winding.height);
    assert_eq!(pixels.get_pixel(39, 43).0, [206, 72, 57, 255]);
    assert_eq!(pixels.get_pixel(119, 43).0, [0, 0, 0, 0]);
    assert_eq!(pixels.get_pixel(199, 43).0, [0, 0, 0, 0]);
    let paint = FIXTURES[3];
    let pixels = raw_pixels(paint.svg, paint.width, paint.height);
    // The overlap is a single group at 0.5 opacity, not two translucent paints.
    assert_eq!(pixels.get_pixel(190, 48)[3], pixels.get_pixel(170, 25)[3]);
    assert_ne!(
        pixels.get_pixel(190, 48).0[..3],
        pixels.get_pixel(170, 25).0[..3]
    );
    assert_eq!(pixels.get_pixel(194, 134).0, [99, 167, 70, 255]);
    assert_ne!(
        pixels,
        raw_pixels(
            &paint.svg.replace("<g opacity=\"0.5\">", "<g>"),
            paint.width,
            paint.height
        ),
        "group opacity oracle is visible"
    );
    let affine = FIXTURES[4];
    assert_ne!(
        raw_pixels(affine.svg, affine.width, affine.height),
        raw_pixels(
            &affine.svg.replace("scale(1.8 0.65)", "scale(1 1)"),
            affine.width,
            affine.height
        ),
        "anisotropic stroke/geometry fixture is nontrivial"
    );
}

#[test]
fn svg_import_each_insert_is_one_history_step_with_deterministic_editable_contents() {
    for fixture in FIXTURES {
        let parsed = crate::svg_import::parse(fixture.svg.as_bytes()).unwrap();
        let again = crate::svg_import::parse(fixture.svg.as_bytes()).unwrap();
        assert_eq!(
            parsed.contents, again.contents,
            "deterministic {}",
            fixture.name
        );
        let rows = parsed.contents.rows();
        assert!(!rows.is_empty());
        let ids: std::collections::BTreeSet<_> = rows.iter().map(|(_, _, node)| node.id).collect();
        assert_eq!(ids.len(), rows.len(), "unique native IDs");
        for (_, _, node) in rows {
            assert!(node.id > 0);
            assert!(
                matches!(
                    node.kind,
                    ContentsKind::Group(_)
                        | ContentsKind::Path { .. }
                        | ContentsKind::Fill { .. }
                        | ContentsKind::Stroke(_)
                ),
                "editable static native Contents only"
            );
        }
        let mut editor = blank();
        let before = editor.project().clone();
        import(&mut editor, fixture);
        assert!(editor.can_undo());
        assert!(!editor.can_redo());
        let after = editor.project().clone();
        assert_eq!(after.composition().layers().len(), 1);
        let layer = &after.composition().layers()[0];
        assert_eq!(layer.name(), fixture.name);
        assert_eq!(
            (layer.width(), layer.height()),
            (fixture.width, fixture.height)
        );
        assert!(matches!(layer.content(), Content::ShapeContents(_)));
        editor.undo();
        exact(editor.project(), &before, "one Undo restores full source");
        assert!(!editor.can_undo());
        editor.redo();
        exact(
            editor.project(),
            &after,
            "one Redo restores full native content",
        );
        assert!(!editor.can_redo());
    }
}

#[test]
fn svg_import_literal_viewbox_alignment_and_safe_xml_metadata() {
    for alignment in [
        "xMinYMin", "xMidYMin", "xMaxYMin", "xMinYMid", "xMidYMid", "xMaxYMid", "xMinYMax",
        "xMidYMax", "xMaxYMax",
    ] {
        for mode in ["meet", "slice"] {
            let source = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='180' height='120' viewBox='10 20 100 100' preserveAspectRatio='{alignment} {mode}'><rect x='-8' y='6' width='49' height='85' fill='#5b893d'/><circle cx='85' cy='96' r='31' fill='#b4588c' stroke='#4f2840' stroke-width='7'/></svg>"
            );
            let parsed = crate::svg_import::parse(source.as_bytes()).unwrap();
            let mut editor = blank();
            editor
                .execute(Command::ImportSvg {
                    contents: parsed.contents,
                    width: parsed.width,
                    height: parsed.height,
                    name: alignment.into(),
                })
                .unwrap();
            // Authored affine equivalents for this fixed literal viewBox. No
            // production viewport helper or floating transform is reused.
            let matrix = if mode == "meet" {
                let x = if alignment.starts_with("xMin") {
                    -12
                } else if alignment.starts_with("xMid") {
                    18
                } else {
                    48
                };
                format!("1.2 0 0 1.2 {x} -24")
            } else {
                let y = if alignment.ends_with("YMin") {
                    -36
                } else if alignment.ends_with("YMid") {
                    -66
                } else {
                    -96
                };
                format!("1.8 0 0 1.8 -18 {y}")
            };
            let reference = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='180' height='120'><g transform='matrix({matrix})'><rect x='-8' y='6' width='49' height='85' fill='#5b893d'/><circle cx='85' cy='96' r='31' fill='#b4588c' stroke='#4f2840' stroke-width='7'/></g></svg>"
            );
            let expected = raw_pixels(&reference, 180., 120.);
            let raw = raw_pixels(&source, 180., 120.);
            let delta = if mode == "meet" && matches!(alignment, "xMinYMax" | "xMidYMax") {
                (1, 16)
            } else {
                (0, 0)
            };
            raw_normalization_delta(
                &expected,
                &raw,
                delta,
                &format!("alignment {alignment} {mode}"),
            );
            let actual = Renderer::new().render(editor.project(), 30, WIDTH).unwrap();
            pixels_equal(&actual, &expected, &format!("alignment {alignment} {mode}"));
        }
    }
    let source = "<?xml version='1.0' encoding='UTF-8'?><svg viewBox='0 0 240 160'><title>Safe &amp; inert</title><!-- ignored --><desc>Literal description</desc><g id='group'><rect id='box' x='12' y='16' width='70' height='50' fill='rgb(12, 80, 144)'/><path d='M100 20 L200 90' fill='none' stroke='purple' stroke-width='5'/></g></svg>";
    let parsed = crate::svg_import::parse(source.as_bytes()).unwrap();
    assert_eq!((parsed.width, parsed.height), (240., 160.));
    assert!(
        parsed
            .contents
            .rows()
            .iter()
            .any(|(_, _, n)| n.name == "box")
    );
    let mut editor = blank();
    editor
        .execute(Command::ImportSvg {
            contents: parsed.contents,
            width: parsed.width,
            height: parsed.height,
            name: "Metadata".into(),
        })
        .unwrap();
    // Strip only the XML declaration when nesting this standalone literal XML.
    let source = source.split_once("?>").unwrap().1;
    let reference = source.replace(
        "M100 20 L200 90",
        "M100 20 C133.33333333333334 43.33333333333333 166.66666666666666 66.66666666666667 200 90",
    );
    let expected = raw_pixels(&reference, 240., 160.);
    raw_normalization_delta(
        &expected,
        &raw_pixels(source, 240., 160.),
        (0, 0),
        "metadata",
    );
    compare_routes(
        editor.project(),
        &expected,
        "safe metadata and dimensions from viewBox",
    );
}

#[test]
fn svg_import_local_reader_matches_parser_and_rejects_nonregular_or_oversized_files() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("literal.svg");
    std::fs::write(&good, FIXTURES[0].svg).unwrap();
    let parsed = crate::svg_import::parse(FIXTURES[0].svg.as_bytes()).unwrap();
    let read = crate::svg_import::read_svg_file(&good).unwrap();
    assert_eq!(read.contents, parsed.contents);
    assert_eq!((read.width, read.height), (parsed.width, parsed.height));
    assert!(crate::svg_import::read_svg_file(dir.path()).is_err());
    assert!(crate::svg_import::read_svg_file(&dir.path().join("missing.svg")).is_err());
    let oversized = dir.path().join("oversized.svg");
    std::fs::write(&oversized, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(crate::svg_import::read_svg_file(&oversized).is_err());
    #[cfg(unix)]
    {
        let link = dir.path().join("link.svg");
        std::os::unix::fs::symlink(&good, &link).unwrap();
        assert!(crate::svg_import::read_svg_file(&link).is_err());
    }
}

fn rejected(source: &[u8], label: &str) {
    match crate::svg_import::parse(source) {
        Ok(_) => panic!("unsafe/unsupported SVG accepted: {label}"),
        Err(error) => assert!(!error.trim().is_empty(), "actionable rejection: {label}"),
    }
}
fn wrapped(body: &str) -> String {
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='240' height='160'>{body}</svg>")
}
#[test]
fn svg_import_rejects_malformed_xml_entities_processing_and_foreign_namespaces() {
    for (label, source) in [
        ("empty", ""),
        (
            "invalid XML version",
            "<?xml version='garbage'?><svg width='240' height='160'><rect width='20' height='20'/></svg>",
        ),
        (
            "UTF16 declaration",
            "<?xml version='1.0' encoding='UTF-16'?><svg width='240' height='160'><rect width='20' height='20'/></svg>",
        ),
        (
            "invalid standalone",
            "<?xml version='1.0' standalone='maybe'?><svg width='240' height='160'><rect width='20' height='20'/></svg>",
        ),
        ("truncated", "<svg"),
        ("mismatched", "<svg><g></svg>"),
        ("multiple roots", "<svg/><svg/>"),
        ("junk suffix", "<svg/>tail"),
        (
            "duplicate attribute",
            "<svg width='240' width='120' height='160'/>",
        ),
        (
            "wrong namespace",
            "<svg xmlns='https://invalid.example/svg' width='240' height='160'/>",
        ),
        ("non-svg root", "<html><svg/></html>"),
        (
            "unknown entity",
            "<svg width='240' height='160'>&missing;</svg>",
        ),
        ("DTD", "<!DOCTYPE svg><svg width='240' height='160'/>"),
        (
            "external DTD",
            "<!DOCTYPE svg SYSTEM 'https://invalid.example/svg.dtd'><svg width='240' height='160'/>",
        ),
        (
            "entity declaration",
            "<!DOCTYPE svg [<!ENTITY color '#fff'>]><svg width='240' height='160'><rect width='10' height='10' fill='&color;'/></svg>",
        ),
        (
            "external entity",
            "<!DOCTYPE svg [<!ENTITY secret SYSTEM 'file:///etc/passwd'>]><svg width='240' height='160'>&secret;</svg>",
        ),
        (
            "processing instruction",
            "<?xml-stylesheet href='https://invalid.example/x.css'?><svg width='240' height='160'/>",
        ),
        (
            "inner processing instruction",
            "<svg width='240' height='160'><?work anything?></svg>",
        ),
        (
            "foreign child",
            "<svg xmlns='http://www.w3.org/2000/svg' xmlns:q='https://invalid.example'><q:rect width='20' height='20'/></svg>",
        ),
    ] {
        rejected(source.as_bytes(), label);
    }
    rejected(&[0xff, 0xfe, 0, 0], "invalid UTF-8");
    rejected(b"<svg width='240' height='160'>\0</svg>", "NUL");
}
#[test]
fn svg_import_rejects_unsupported_nodes_css_resource_references_and_events() {
    for element in [
        "script",
        "style",
        "text",
        "image",
        "use",
        "foreignObject",
        "defs",
        "symbol",
        "linearGradient",
        "radialGradient",
        "pattern",
        "clipPath",
        "mask",
        "filter",
        "animate",
        "animateTransform",
        "set",
        "a",
        "marker",
        "switch",
        "metadata",
    ] {
        rejected(wrapped(&format!("<{element}/>")).as_bytes(), element);
    }
    for attribute in [
        "style='filter:none'",
        "class='paint'",
        "onclick='run()'",
        "onload='run()'",
        "href='#thing'",
        "href='https://invalid.example/resource.svg'",
        "fill='url(#paint)'",
        "fill='url(https://invalid.example/paint.svg#p)'",
        "stroke='url(data:image/svg+xml;base64,PHN2Zz4=)'",
        "filter='url(#f)'",
        "clip-path='url(#c)'",
        "mask='url(#m)'",
        "marker-end='url(#m)'",
        "vector-effect='non-scaling-stroke'",
        "paint-order='stroke fill'",
        "display='none'",
        "visibility='hidden'",
        "mix-blend-mode='multiply'",
        "unknown='value'",
        "font-family='sans-serif'",
    ] {
        let source = wrapped(&format!(
            "<rect x='10' y='10' width='20' height='20' {attribute}/>"
        ));
        rejected(source.as_bytes(), attribute);
    }
    rejected(wrapped("<g xmlns:q='https://invalid.example' q:opacity='0.5'><rect width='20' height='20'/></g>").as_bytes(), "foreign attr");
    rejected(wrapped("<g xmlns:xlink='http://www.w3.org/1999/xlink'><rect width='20' height='20' xlink:href='#x'/></g>").as_bytes(), "namespaced href");
    rejected(
        wrapped("<svg width='20' height='20'><rect width='20' height='20'/></svg>").as_bytes(),
        "nested SVG outside bounded contract",
    );
    rejected(
        wrapped("<![CDATA[unsupported text]]>").as_bytes(),
        "CDATA text",
    );
}
#[test]
fn svg_import_rejects_invalid_or_unrepresentable_values_without_partial_document() {
    for body in [
        "<path d='M0 0 L20'/>",
        "<path d='L0 0 L20 20'/>",
        "<path d='M0 0 C10 10 20 20'/>",
        "<path d='M0 0 L10 10 garbage'/>",
        "<path d='M0 0 A10 10 0 2 0 20 20'/>",
        "<path d='M0 0 LNaN 20'/>",
        "<path d='M90 90 M10 10 L30 30'/>",
        "<path d='M10 10 L30 30 M90 90'/>",
        "<path d='M0 0 A1 1 0 0 1 1e-300 0'/>",
        "<path d='M0 0 L20 20,'/>",
        "<polyline points='0 0 10 10,'/>",
        "<polyline points='0 0,,10 10'/>",
        "<path d='M0 0 L1000001 20'/>",
        "<rect width='-10' height='20'/>",
        "<rect width='20%' height='20'/>",
        "<rect width='2em' height='20'/>",
        "<rect width='20' height='20' fill='not-a-color'/>",
        "<rect width='20' height='20' fill='rgb(1,2,3,)'/>",
        "<rect width='20' height='20' fill='rgba(1,2,3,0.9999)'/>",
        "<rect width='20' height='20' fill='#112233ff'/>",
        "<rect width='20' height='20' fill='#123f'/>",
        "<rect width='20' height='20' fill='hsl(0,50%,50%)'/>",
        "<rect width='20' height='20' fill='rgb(10%,20,30%)'/>",
        "<rect width='20' height='20' fill='rgb(1+2+3)'/>",
        "<rect width='20' height='20' fill='rgb(10%20%30%)'/>",
        "<rect width='20' height='20' opacity='NaN'/>",
        "<rect width='20' height='20' stroke='red' stroke-width='1025'/>",
        "<rect width='20' height='20' stroke='red' stroke-miterlimit='1025'/>",
        "<rect width='20' height='20' stroke='red' stroke-miterlimit='4px'/>",
        "<rect width='20' height='20' stroke='red' stroke-dasharray=''/>",
        "<rect width='20' height='20' stroke='red' stroke-dasharray='1 2,'/>",
        "<rect width='20' height='20' stroke='red' stroke-dasharray='-1 2'/>",
        "<g transform='translate(10 20) trailing'><rect width='20' height='20'/></g>",
        "<g transform='matrix(1 0 0 1 0)'><rect width='20' height='20'/></g>",
        "<g transform='scale(0)'><rect width='20' height='20'/></g>",
        "<g transform='scale(2,)'><rect width='20' height='20'/></g>",
        "<g transform='translate(1 2),'><rect width='20' height='20'/></g>",
        "<g transform=''><rect width='20' height='20'/></g>",
        "<g transform='scale(1000)'><rect width='20' height='20'/></g>",
        "<g transform='translate(1000001 0)'><rect width='20' height='20'/></g>",
    ] {
        rejected(wrapped(body).as_bytes(), body);
    }
    for source in [
        "<svg width='0' height='160'/>",
        "<svg width='240' height='-1'/>",
        "<svg width='NaN' height='160'/>",
        "<svg width='1e309' height='160'/>",
        "<svg width='32769' height='160'/>",
        "<svg width='240' height='160' viewBox='0 0 0 10'/>",
        "<svg width='240' height='160' viewBox='0 0 10'/>",
        "<svg width='240' height='160' preserveAspectRatio='invented'><rect width='20' height='20'/></svg>",
        "<svg width='240' height='160' viewBox='0 0 40 40,'><rect width='20' height='20'/></svg>",
        "<svg width='240' height='160' viewBox='0 0 40+40'><rect width='20' height='20'/></svg>",
        "<svg width='240' height='160' viewBox='0 0 40 40' preserveAspectRatio='xMidYMid!'><rect width='20' height='20'/></svg>",
        "<svg width='240' height='160' viewBox='0 0 40 40' preserveAspectRatio='none!'><rect width='20' height='20'/></svg>",
    ] {
        rejected(source.as_bytes(), source);
    }
}
#[test]
fn svg_import_fractional_viewport_rejects_instead_of_compounding_edge_coverage() {
    rejected(
        include_bytes!("../fixtures/svg_import/fractional-viewport.svg"),
        "fractional viewport requires unsupported fractional mask coverage",
    );
}

#[test]
fn svg_import_rejects_each_bounded_resource_overflow() {
    rejected(&vec![b' '; 1024 * 1024 + 1], "1 MiB input cap");
    let repeated_close = format!("<path d='M0 0 L10 0 L10 10 Z{}'/>", "Z".repeat(8000));
    rejected(
        wrapped(&repeated_close).as_bytes(),
        "repeat Close rejected before recursive simplifier",
    );
    let comments = "<!--small-->".repeat(2049);
    rejected(wrapped(&comments).as_bytes(), "2048 total XML nodes");
    let elements = "<g/>".repeat(513);
    rejected(wrapped(&elements).as_bytes(), "512 elements");
    let shapes = "<rect width='1' height='1'/>".repeat(100);
    rejected(wrapped(&shapes).as_bytes(), "256 generated Contents nodes");
    let deep = format!(
        "{}<rect width='1' height='1'/>{}",
        "<g>".repeat(20),
        "</g>".repeat(20)
    );
    rejected(wrapped(&deep).as_bytes(), "bounded nesting");
    let contour = format!(
        "<path d='M0 0 {}' fill='none' stroke='black'/>",
        "L1 1 ".repeat(1025)
    );
    rejected(wrapped(&contour).as_bytes(), "1024 contour vertices");
    let segments = format!(
        "<path d='{}' fill='none' stroke='black'/>",
        "M0 0 L1 1 ".repeat(4097)
    );
    rejected(wrapped(&segments).as_bytes(), "8192 path segments");
    let dashes = (0..17).map(|_| "1").collect::<Vec<_>>().join(" ");
    rejected(
        wrapped(&format!(
            "<path d='M0 0 L20 20' stroke='black' stroke-dasharray='{dashes}'/>"
        ))
        .as_bytes(),
        "16 dashes",
    );
}

fn immutable(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => file.write_all(bytes).unwrap(),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            assert_eq!(
                std::fs::read(path).unwrap(),
                bytes,
                "immutable {}",
                path.display()
            );
        }
        Err(error) => panic!("write {}: {error}", path.display()),
    }
}
fn export_project(root: &Path, label: &str, project: &Project, views: &ProjectViews) {
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
#[ignore = "requires an explicit absolute immutable fixture output directory"]
fn export_svg_import_acceptance_fixtures() {
    let root = std::env::var_os("LIBREEFFECTS_EXPORT_SVG_IMPORT_FIXTURES")
        .expect("set SVG fixture output directory");
    let root = Path::new(&root);
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let mut views = ProjectViews::default();
    let blank = blank();
    views.normalize(blank.project());
    export_project(root, "before-import", blank.project(), &views);
    let mut cases = Vec::new();
    for fixture in FIXTURES {
        let mut editor = self::blank();
        import(&mut editor, fixture);
        views.normalize(editor.project());
        export_project(root, fixture.name, editor.project(), &views);
        immutable(
            &root.join(format!("{}.svg", fixture.name)),
            fixture.svg.as_bytes(),
        );
        let raw = raw_pixels(fixture.svg, fixture.width, fixture.height);
        let expected = raw_pixels(fixture.reference, fixture.width, fixture.height);
        raw_normalization_delta(&expected, &raw, fixture.raw_delta, fixture.name);
        immutable(
            &root.join(format!("{}.cubic.svg", fixture.name)),
            fixture.reference.as_bytes(),
        );
        let mut raw_png = Cursor::new(Vec::new());
        raw.write_to(&mut raw_png, image::ImageFormat::Png).unwrap();
        immutable(
            &root.join(format!("{}.raw.literal.png", fixture.name)),
            &raw_png.into_inner(),
        );
        compare_routes(editor.project(), &expected, fixture.name);
        let mut png = Cursor::new(Vec::new());
        expected
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        immutable(
            &root.join(format!("{}.literal.png", fixture.name)),
            &png.into_inner(),
        );
        for frame in FRAMES {
            cases.push(json!({"label": fixture.name, "frame": frame,
                "input": format!("{}.generated.lep", fixture.name),
                "json": format!("{}.lfe.json", fixture.name),
                "source_svg": format!("{}.svg", fixture.name),
                "expected_png": format!("{}.literal.png", fixture.name),
                "raw_source_expected_png": format!("{}.raw.literal.png", fixture.name),
                "cubic_reference_svg": format!("{}.cubic.svg", fixture.name),
                "raw_source_changed_pixels": fixture.raw_delta.0,
                "raw_source_max_channel_delta": fixture.raw_delta.1}));
        }
    }
    immutable(
        &root.join("cases.json"),
        &serde_json::to_vec_pretty(&json!({
            "dimensions": [WIDTH, HEIGHT], "frames": FRAMES,
            "oracle": "independently authored cubic SVG reference; exact full unmasked RGBA. Original source SVG edge deltas separately characterized.",
            "render_cases": cases
        }))
        .unwrap(),
    );
    println!(
        "Exported {} independent cubic SVG references and original source delta records; {} exact five-route frame sets. Generated inputs are not native interaction evidence.",
        FIXTURES.len(),
        FIXTURES.len() * FRAMES.len()
    );
}
/// Reads actual separately recorded native Save/Open; never fabricates a save.
#[test]
#[ignore = "requires separately recorded native save and expected generated LEP; SVG_SOURCE for nonblank"]
fn verify_recorded_native_svg_import_save() {
    let actual_path =
        std::env::var_os("LIBREEFFECTS_SVG_IMPORT_NATIVE_SAVE").expect("set native save");
    let expected_path =
        std::env::var_os("LIBREEFFECTS_SVG_IMPORT_EXPECTED").expect("set generated expected LEP");
    let source_path = std::env::var_os("LIBREEFFECTS_SVG_IMPORT_SOURCE");
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_SVG_IMPORT_EXPECTED_FRAME") {
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
    exact(
        &actual.project,
        &expected.project,
        "recorded native full source",
    );
    assert_eq!(actual.views, expected.views, "recorded native full VIEW");
    let pixels = if let Some(source_path) = source_path {
        assert_eq!(
            expected.project.composition().layers().len(),
            1,
            "single imported layer fixture"
        );
        let source = std::fs::read_to_string(source_path).unwrap();
        let layer = &expected.project.composition().layers()[0];
        let fixture = FIXTURES
            .into_iter()
            .find(|fixture| fixture.svg == source)
            .expect("native source must match an unchanged literal acceptance fixture");
        let raw = raw_pixels(&source, layer.width(), layer.height());
        let pixels = raw_pixels(fixture.reference, layer.width(), layer.height());
        raw_normalization_delta(&pixels, &raw, fixture.raw_delta, fixture.name);
        pixels
    } else {
        assert!(
            expected.project.composition().layers().is_empty(),
            "SVG_SOURCE is required for a nonempty native expectation"
        );
        image::RgbaImage::new(WIDTH, HEIGHT)
    };
    compare_routes(&actual.project, &pixels, "actual native SVG import");
    println!(
        "Exact full source, VIEW and {} five-route frames against independent cubic SVG reference (or literal blank), with declared raw-source edge delta: {}",
        FRAMES.len(),
        Path::new(&actual_path).display()
    );
}
