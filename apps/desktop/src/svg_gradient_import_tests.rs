//! Independent bounded linear-gradient import acceptance. References below are
//! literal cubic paths and hand-derived gradient covectors, never importer output.
//! Every RGBA byte is compared, without masking or a blanket AA tolerance. The
//! shear fixture separately records its exact original/flattened shader rounding
//! delta; every native route still equals the cubic reference byte-for-byte. These
//! axis-aligned fixtures do not expand the existing general cubic/arc AA contract.
use crate::rendering::Renderer;
use libre_effects_core::*;

const WIDTH: u32 = 240;
const HEIGHT: u32 = 160;
const STOPS: &str = "<stop offset='0' stop-color='red'/><stop offset='50%' stop-color='#0f0' stop-opacity='0.5'/><stop offset='.5' stop-color='rgb(0,0,255)' stop-opacity='.5'/><stop offset='1' stop-color='white' stop-opacity='.75'/>";
const BW: &str = "<stop offset='0' stop-color='black'/><stop offset='1' stop-color='white'/>";
const RECT: &str = "<rect x='24' y='24' width='120' height='60' fill='url(#ramp)'/>";
// Independently authored exact thirds for the four straight cubic segments.
const CUBIC: &str =
    "M24 24 C64 24 104 24 144 24 C144 44 144 64 144 84 C104 84 64 84 24 84 C24 64 24 44 24 24 Z";

fn svg(body: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}'>{body}</svg>"
    )
}
fn gradient(attributes: &str, stops: &str, geometry: &str) -> String {
    svg(&format!(
        "<defs><linearGradient id='ramp' {attributes}>{stops}</linearGradient></defs>{geometry}"
    ))
}
fn basic_attributes() -> String {
    gradient(
        "spreadMethod='pad' color-interpolation='sRGB'",
        STOPS,
        "<rect x='24' y='24' width='120' height='60' fill='url(#ramp)' fill-opacity='.8' stroke='url(#ramp)' stroke-opacity='.6' stroke-width='6'/>",
    )
}
fn basic_inline() -> String {
    gradient(
        "spreadMethod='pad' color-interpolation='sRGB'",
        "<stop offset='0' stop-color='black' style='stop-color:red'/><stop offset='50%' stop-opacity='.2' style='stop-color:#0f0; stop-opacity:.5'/><stop offset='.5' style='stop-color:rgb(0,0,255);stop-opacity:.5'/><stop offset='1' stop-color='blue' style='stop-color:red;stop-color:white;stop-opacity:.75'/>",
        "<rect x='24' y='24' width='120' height='60' fill='black' stroke='red' stroke-width='1' style='fill:url(#ramp);fill-opacity:.8;stroke:url(#ramp);stroke-opacity:.6;stroke-width:6'/>",
    )
}
fn basic_reference() -> String {
    gradient(
        "gradientUnits='userSpaceOnUse' x1='24' y1='24' x2='144' y2='24'",
        STOPS,
        &format!(
            "<path d='{CUBIC}' fill='url(#ramp)' fill-opacity='.8' stroke='url(#ramp)' stroke-opacity='.6' stroke-width='6'/>"
        ),
    )
}
fn blank() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "SVG gradient independent acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor.clear_history();
    editor
}
fn insert(editor: &mut Editor, source: &str) -> Result<(), String> {
    let parsed = crate::svg_import::parse(source.as_bytes())?;
    parsed.contents.validate(90)?;
    editor.execute(Command::ImportSvg {
        contents: parsed.contents,
        width: parsed.width,
        height: parsed.height,
        name: "Imported linear gradients".into(),
    })?;
    Ok(())
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
    let tree = resvg::usvg::Tree::from_str(source, &resvg::usvg::Options::default())
        .expect("independent literal SVG reference must render");
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
    let first: Vec<_> = actual
        .enumerate_pixels()
        .filter_map(|(x, y, p)| {
            let e = expected.get_pixel(x, y);
            (p != e).then_some((x, y, p.0, e.0))
        })
        .take(4)
        .collect();
    assert_eq!(
        changed, 0,
        "{label}: {changed} differing pixels, max RGBA delta {maximum}; first {first:?}"
    );
}
fn compare_routes(project: &Project, expected: &image::RgbaImage, label: &str) {
    let source = project.to_json().unwrap();
    let bytes = project_file::encode(project, None).unwrap();
    let json = Project::from_json(&source).unwrap();
    let lep = project_file::decode(&bytes).unwrap().project;
    source_equal(&json, project, "JSON preserves complete gradient source");
    source_equal(&lep, project, "LEP preserves complete gradient source");
    let renderer = Renderer::new();
    for frame in [0, 30, 60] {
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
            pixels_equal(
                &actual,
                expected,
                &format!("{label}: {route} frame {frame}"),
            );
        }
    }
    assert_eq!(
        project.to_json().unwrap(),
        source,
        "rendering is source-neutral"
    );
    assert_eq!(project_file::encode(project, None).unwrap(), bytes);
}
fn imported_against_literal(
    source: &str,
    reference: &str,
    label: &str,
) -> (Editor, image::RgbaImage) {
    let expected = literal_pixels(reference);
    assert!(
        expected.pixels().any(|p| p[3] != 0),
        "nonempty {label} oracle"
    );
    let mut editor = blank();
    insert(&mut editor, source).unwrap_or_else(|e| panic!("{label}: {e}"));
    compare_routes(editor.project(), &expected, label);
    (editor, expected)
}
fn acceptance(source: &str, reference: &str, label: &str) -> Editor {
    let (editor, expected) = imported_against_literal(source, reference, label);
    pixels_equal(
        &literal_pixels(source),
        &expected,
        &format!("{label} original/cubic literals"),
    );
    editor
}
fn contents(project: &Project) -> &ShapeContents {
    let Content::ShapeContents(contents) = project.composition().layers()[0].content() else {
        panic!("SVG must remain editable native Contents");
    };
    contents
}
fn value(node: &ContentsNode, parameter: GradientParam) -> f64 {
    node.value_at(ContentsParam::Gradient(parameter), 0)
}
fn assert_endpoints(node: &ContentsNode, expected: [f64; 4]) {
    use GradientParam::*;
    for (parameter, expected) in [StartX, StartY, EndX, EndY].into_iter().zip(expected) {
        assert!(
            (value(node, parameter) - expected).abs() < 1e-8,
            "{parameter:?}: expected {expected}, got {}",
            value(node, parameter)
        );
    }
}

#[test]
fn svg_gradient_literal_inline_attributes_coincident_stops_fill_and_stroke() {
    let attributes = basic_attributes();
    let inline = basic_inline();
    let a = crate::svg_import::parse(attributes.as_bytes()).unwrap();
    let b = crate::svg_import::parse(inline.as_bytes()).unwrap();
    assert_eq!((a.width, a.height), (240., 160.));
    assert_eq!((a.width, a.height), (b.width, b.height));
    assert_eq!(
        a.contents, b.contents,
        "inline overrides produce the exact attribute-only source tree"
    );
    let actual = acceptance(
        &inline,
        &basic_reference(),
        "inline gradients with hard stops",
    );
    let expected = acceptance(
        &attributes,
        &basic_reference(),
        "attribute gradients with hard stops",
    );
    source_equal(
        actual.project(),
        expected.project(),
        "full inline/attribute project equality",
    );
    let pixels = literal_pixels(&basic_reference());
    assert_ne!(
        pixels.get_pixel(83, 50),
        pixels.get_pixel(84, 50),
        "coincident color stop remains a visible hard boundary"
    );
}

#[test]
fn svg_gradient_nonsquare_bbox_uses_covector_not_transformed_endpoints() {
    let source = gradient("x2='1' y2='1'", BW, RECT);
    // A=diag(120,60), d=(1,1). The covector is (1/240,1/120),
    // hence d'=(48,96). Simply transforming d would incorrectly give (120,60).
    let reference = gradient(
        "gradientUnits='userSpaceOnUse' x1='24' y1='24' x2='72' y2='120'",
        BW,
        &format!("<path d='{CUBIC}' fill='url(#ramp)'/>"),
    );
    let editor = acceptance(&source, &reference, "nonsquare diagonal bounding box");
    let nodes = contents(editor.project()).rows();
    let node = nodes
        .iter()
        .find(|(_, _, n)| n.kind.gradient().is_some())
        .unwrap()
        .2;
    assert_endpoints(node, [24., 24., 72., 120.]);
    let wrong = reference.replace("x2='72' y2='120'", "x2='144' y2='84'");
    assert_ne!(
        literal_pixels(&wrong),
        literal_pixels(&reference),
        "oracle distinguishes naive endpoint transform"
    );
}

#[test]
fn svg_gradient_sheared_user_space_uses_inverse_transpose() {
    let source = gradient(
        "gradientUnits='userSpaceOnUse' x1='0' y1='0' x2='100' y2='0' gradientTransform='matrix(1 0 1 1 20 100)'",
        BW,
        RECT,
    );
    // t=(x-y+80)/100, so a Euclidean gradient is (20,100)->(70,50).
    let reference = gradient(
        "gradientUnits='userSpaceOnUse' x1='20' y1='100' x2='70' y2='50'",
        BW,
        &format!("<path d='{CUBIC}' fill='url(#ramp)'/>"),
    );
    let (editor, expected) =
        imported_against_literal(&source, &reference, "sheared user-space gradient");
    // The two mathematically identical shader constructions round coordinates
    // differently in f32 before 8-bit quantization. Record this ONE literal pair,
    // not a tolerance for imported/native pixels. All native routes above remain
    // exact. No pixels are excluded, and no changed pixel has AA/alpha coverage.
    let original = literal_pixels(&source);
    let mut changed = 0;
    let mut maximum = 0;
    for (x, y, pixel) in original.enumerate_pixels() {
        let equivalent = expected.get_pixel(x, y);
        if pixel == equivalent {
            continue;
        }
        changed += 1;
        assert!(
            (24..144).contains(&x) && (24..84).contains(&y),
            "changed sample must be inside the literal rectangle: {x},{y}"
        );
        assert_eq!(
            (pixel[3], equivalent[3]),
            (255, 255),
            "coordinate rounding never changes coverage"
        );
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
        assert_eq!(equivalent[0], equivalent[1]);
        assert_eq!(equivalent[1], equivalent[2]);
        maximum = maximum.max(pixel[0].abs_diff(equivalent[0]));
    }
    assert_eq!(
        (changed, maximum),
        (93, 1),
        "exact unmasked transformed-versus-flattened gradient coordinate quantization"
    );
    let nodes = contents(editor.project()).rows();
    let node = nodes
        .iter()
        .find(|(_, _, n)| n.kind.gradient().is_some())
        .unwrap()
        .2;
    assert_endpoints(node, [20., 100., 70., 50.]);
    let wrong = reference.replace("x2='70' y2='50'", "x2='120' y2='100'");
    assert_ne!(
        literal_pixels(&wrong),
        literal_pixels(&reference),
        "shear oracle distinguishes naive endpoint transform"
    );
}

#[test]
fn svg_gradient_user_space_percentages_use_viewbox_size_and_match_px() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='240' height='160' viewBox='10 20 120 80'><defs><linearGradient id='ramp' gradientUnits='userSpaceOnUse' x1='10%' y1='25%' x2='90%' y2='75%'><stop offset='0' stop-color='black'/><stop offset='1' stop-color='white'/></linearGradient></defs><rect x='12' y='24' width='96' height='48' fill='url(#ramp)'/></svg>";
    let reference = svg(
        "<defs><linearGradient id='ramp' gradientUnits='userSpaceOnUse' x1='12' y1='20' x2='108' y2='60'><stop offset='0' stop-color='black'/><stop offset='1' stop-color='white'/></linearGradient></defs><g transform='matrix(2 0 0 2 -20 -40)'><path d='M12 24 C44 24 76 24 108 24 C108 40 108 56 108 72 C76 72 44 72 12 72 C12 56 12 40 12 24 Z' fill='url(#ramp)'/></g>",
    );
    let editor = acceptance(source, &reference, "viewBox percentage lengths");
    let pixels = source.replace(
        "x1='10%' y1='25%' x2='90%' y2='75%'",
        "x1='12px' y1='20px' x2='108px' y2='60px'",
    );
    let mut equivalent = blank();
    insert(&mut equivalent, &pixels).unwrap();
    source_equal(
        editor.project(),
        equivalent.project(),
        "percent and px preserve exact native source",
    );
    let nodes = contents(editor.project()).rows();
    let node = nodes
        .iter()
        .find(|(_, _, n)| n.kind.gradient().is_some())
        .unwrap()
        .2;
    assert_endpoints(node, [12., 20., 108., 60.]);
}

#[test]
fn svg_gradient_import_exposes_editable_stops_with_full_history_and_lep_roundtrip() {
    let mut editor = blank();
    let before = editor.project().clone();
    insert(&mut editor, &basic_attributes()).unwrap();
    let imported = editor.project().clone();
    assert!(editor.can_undo());
    assert!(!editor.can_redo());
    assert_eq!(imported.composition().layers().len(), 1);
    let layer = &imported.composition().layers()[0];
    assert_eq!(layer.name(), "Imported linear gradients");
    assert_eq!((layer.width(), layer.height()), (240., 160.));
    let rows = contents(&imported).rows();
    let ids: std::collections::BTreeSet<_> = rows.iter().map(|(_, _, n)| n.id).collect();
    assert_eq!(ids.len(), rows.len());
    assert!(!ids.contains(&0));
    let paints: Vec<_> = rows
        .iter()
        .filter_map(|(_, _, n)| n.kind.gradient().map(|g| (*n, g)))
        .collect();
    assert_eq!(paints.len(), 2);
    assert!(
        paints
            .iter()
            .any(|(n, _)| matches!(n.kind, ContentsKind::GradientFill { .. }))
    );
    assert!(
        paints
            .iter()
            .any(|(n, _)| matches!(n.kind, ContentsKind::GradientStroke { .. }))
    );
    for (node, gradient) in &paints {
        assert!(!gradient.radial);
        assert_eq!(gradient.colors.len(), 4);
        assert_eq!(gradient.opacities.len(), 4);
        assert_endpoints(node, [24., 24., 144., 24.]);
        for (&id, (position, rgb)) in gradient.colors.iter().zip([
            (0., 0xff0000),
            (50., 0x00ff00),
            (50., 0x0000ff),
            (100., 0xffffff),
        ]) {
            assert_eq!(value(node, GradientParam::ColorPosition(id)), position);
            assert_eq!(gradient.color_at(node, id, 0), Some(rgb));
            assert_eq!(value(node, GradientParam::ColorMidpoint(id)), 50.);
        }
        for (&id, (position, opacity)) in
            gradient
                .opacities
                .iter()
                .zip([(0., 100.), (50., 50.), (50., 50.), (100., 75.)])
        {
            assert_eq!(value(node, GradientParam::OpacityPosition(id)), position);
            assert_eq!(value(node, GradientParam::Opacity(id)), opacity);
            assert_eq!(value(node, GradientParam::OpacityMidpoint(id)), 50.);
        }
    }
    let item = paints[0].0.id;
    let stop = paints[0].1.colors[0];
    let layer_id = layer.id();
    editor.undo();
    source_equal(
        editor.project(),
        &before,
        "one undo removes all imported gradient source",
    );
    assert!(!editor.can_undo());
    editor.redo();
    source_equal(
        editor.project(),
        &imported,
        "one redo restores every node, stop and ID",
    );
    assert!(!editor.can_redo());
    editor
        .execute(Command::Contents {
            id: layer_id,
            edit: ContentsEdit::Track {
                item,
                parameter: ContentsParam::Gradient(GradientParam::Red(stop)),
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 128.,
                },
            },
        })
        .unwrap();
    let edited = editor.project().clone();
    let node = contents(&edited).node(item).unwrap();
    assert_eq!(value(node, GradientParam::Red(stop)), 128.);
    assert_ne!(
        Renderer::new().render(&edited, 0, WIDTH).unwrap(),
        literal_pixels(&basic_reference()),
        "editing imported paint visibly changes output"
    );
    let json = Project::from_json(&edited.to_json().unwrap()).unwrap();
    let encoded = project_file::encode(&edited, None).unwrap();
    let decoded = project_file::decode(&encoded).unwrap().project;
    source_equal(&json, &edited, "edited gradient JSON source");
    source_equal(&decoded, &edited, "edited gradient LEP source");
    editor.undo();
    source_equal(
        editor.project(),
        &imported,
        "one undo reverses only gradient edit",
    );
    editor.redo();
    source_equal(editor.project(), &edited, "one redo restores gradient edit");
}

fn rejected(editor: &mut Editor, source: &str, label: &str) {
    let before = editor.project().clone();
    let history = (editor.can_undo(), editor.can_redo());
    match insert(editor, source) {
        Ok(()) => panic!("unsupported gradient accepted: {label}"),
        Err(error) => assert!(!error.trim().is_empty(), "actionable rejection: {label}"),
    }
    source_equal(editor.project(), &before, label);
    assert_eq!(
        (editor.can_undo(), editor.can_redo()),
        history,
        "rejection preserves both history branches: {label}"
    );
}
fn with_pending_redo() -> Editor {
    let mut editor = blank();
    insert(&mut editor, &basic_attributes()).unwrap();
    insert(&mut editor, &basic_attributes()).unwrap();
    editor.undo();
    assert!(editor.can_undo() && editor.can_redo());
    editor
}

#[test]
fn svg_gradient_rejects_resource_inheritance_and_unsupported_semantics_atomically() {
    let mut editor = with_pending_redo();
    for attributes in [
        "href='#other'",
        "href='#ramp'",
        "spreadMethod='reflect'",
        "spreadMethod='repeat'",
        "color-interpolation='linearRGB'",
        "gradientUnits='invented'",
        "x1='NaN'",
        "x2='1e309'",
        "x1='2em'",
        "x1='12px'",
        "gradientTransform='scale(0)'",
        "gradientTransform='matrix(1 0 0 1 0)'",
        "onclick='run()'",
        "class='paint'",
        "style='filter:none'",
    ] {
        rejected(&mut editor, &gradient(attributes, BW, RECT), attributes);
    }
    for body in [
        format!("<defs><radialGradient id='ramp'>{BW}</radialGradient></defs>{RECT}"),
        format!("<g><defs><linearGradient id='ramp'>{BW}</linearGradient></defs>{RECT}</g>"),
        format!("<linearGradient id='ramp'>{BW}</linearGradient>{RECT}"),
        format!(
            "<defs><rect width='10' height='10'/><linearGradient id='ramp'>{BW}</linearGradient></defs>{RECT}"
        ),
        format!(
            "<defs><linearGradient id='ramp' href='#other'>{BW}</linearGradient><linearGradient id='other' href='#ramp'>{BW}</linearGradient></defs>{RECT}"
        ),
        format!(
            "<defs><linearGradient xmlns:xlink='http://www.w3.org/1999/xlink' id='ramp' xlink:href='#other'>{BW}</linearGradient></defs>{RECT}"
        ),
        format!(
            "<defs><linearGradient id='ramp'>{BW}</linearGradient><linearGradient id='ramp'>{BW}</linearGradient></defs>{RECT}"
        ),
        format!("<defs><linearGradient>{BW}</linearGradient></defs>{RECT}"),
        format!("<defs><linearGradient id='râmp'>{BW}</linearGradient></defs>{RECT}"),
        format!(
            "<defs><linearGradient id='ramp'>{BW}</linearGradient></defs><rect id='ramp' width='20' height='20'/>{RECT}"
        ),
    ] {
        rejected(&mut editor, &svg(&body), &body);
    }
    for paint in [
        "url(#missing)",
        "url(https://invalid.example/a.svg#ramp)",
        "url(data:image/svg+xml;base64,PHN2Zz4=)",
        "url(#ramp) red",
        "var(--ramp)",
        "currentColor",
    ] {
        for attr in [format!("fill='{paint}'"), format!("style='fill:{paint}'")] {
            let geometry = format!("<rect width='20' height='20' {attr}/>");
            rejected(&mut editor, &gradient("", BW, &geometry), &attr);
        }
    }
}

#[test]
fn svg_gradient_rejects_invalid_shadowed_stop_and_paint_values_atomically() {
    let mut editor = with_pending_redo();
    for stop in [
        "<stop stop-color='red'/>",
        "<stop offset='0' stop-color='none'/>",
        "<stop offset='0' stop-color='rgba(1,2,3,.5)'/>",
        "<stop offset='0' stop-color='#112233ff'/>",
        "<stop offset='0' stop-color='currentColor'/>",
        "<stop offset='0' stop-color='url(#ramp)'/>",
        "<stop offset='0' stop-opacity='NaN'/>",
        "<stop offset='0' stop-opacity='1.1'/>",
        "<stop offset='-1%' stop-color='red'/>",
        "<stop offset='101%' stop-color='red'/>",
        "<stop offset='NaN' stop-color='red'/>",
        "<stop offset='0px' stop-color='red'/>",
        "<stop offset='0' stop-color='bad' style='stop-color:red'/>",
        "<stop offset='0' stop-opacity='2' style='stop-opacity:1'/>",
        "<stop offset='0' style='stop-color:bad;stop-color:red'/>",
        "<stop offset='0' style='stop-color:red!important'/>",
        "<stop offset='0' style='stop-color:var(--red)'/>",
        "<stop offset='0' style='offset:0;stop-color:red'/>",
        "<stop offset='0'><animate/></stop>",
    ] {
        rejected(
            &mut editor,
            &gradient(
                "",
                &format!("{stop}<stop offset='1' stop-color='blue'/>"),
                RECT,
            ),
            stop,
        );
    }
    for geometry in [
        "<rect width='20' height='20' fill='url(#missing)' style='fill:url(#ramp)'/>",
        "<rect width='20' height='20' style='fill:url(#missing);fill:url(#ramp)'/>",
        "<rect width='20' height='20' stroke='url(https://invalid.example/a)' style='stroke:url(#ramp)'/>",
    ] {
        rejected(&mut editor, &gradient("", BW, geometry), geometry);
    }
}

#[test]
fn svg_gradient_stop_count_order_and_local_file_bounds_are_enforced() {
    let mut editor = with_pending_redo();
    for stops in [
        "",
        "<stop offset='0' stop-color='red'/>",
        "<stop offset='.75' stop-color='red'/><stop offset='.25' stop-color='blue'/>",
    ] {
        rejected(
            &mut editor,
            &gradient("", stops, RECT),
            "2..32 ordered stops",
        );
    }
    let stops = (0..32)
        .map(|i| format!("<stop offset='{}' stop-color='red'/>", i as f64 / 31.))
        .collect::<String>();
    let source = gradient("", &stops, RECT);
    let parsed = crate::svg_import::parse(source.as_bytes()).unwrap();
    let rows = parsed.contents.rows();
    let g = rows.iter().find_map(|(_, _, n)| n.kind.gradient()).unwrap();
    assert_eq!((g.colors.len(), g.opacities.len()), (32, 32));
    rejected(
        &mut editor,
        &gradient(
            "",
            &format!("{stops}<stop offset='1' stop-color='blue'/>"),
            RECT,
        ),
        "33 stops",
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gradient.svg");
    std::fs::write(&path, &source).unwrap();
    let read = crate::svg_import::read_svg_file(&path).unwrap();
    assert_eq!(
        read.contents, parsed.contents,
        "regular-file route preserves all 32 stops"
    );
    assert_eq!((read.width, read.height), (parsed.width, parsed.height));
    std::fs::write(&path, gradient("spreadMethod='repeat'", BW, RECT)).unwrap();
    assert!(crate::svg_import::read_svg_file(&path).is_err());
}

#[test]
fn svg_gradient_case_sensitive_forward_references_keep_attribute_inline_source_equal() {
    let attributes = basic_attributes().replace("ramp", "Paint_1");
    let inline = basic_inline().replace("ramp", "Paint_1");
    let parsed = crate::svg_import::parse(attributes.as_bytes()).unwrap();
    assert_eq!(
        parsed.contents,
        crate::svg_import::parse(inline.as_bytes())
            .unwrap()
            .contents,
        "inline URL values preserve local ID case"
    );
    let (_, after_start) = inline.split_once("<defs>").unwrap();
    let (definitions, after_defs) = after_start.split_once("</defs>").unwrap();
    let geometry = after_defs.strip_suffix("</svg>").unwrap();
    let forward = svg(&format!("{geometry}<defs>{definitions}</defs>"));
    assert_eq!(
        parsed.contents,
        crate::svg_import::parse(forward.as_bytes())
            .unwrap()
            .contents,
        "definition position does not change editable source"
    );
    acceptance(
        &forward,
        &basic_reference(),
        "case-sensitive inline forward references",
    );
    let wrong_case = forward.replace("url(#Paint_1)", "url(#paint_1)");
    rejected(
        &mut with_pending_redo(),
        &wrong_case,
        "local gradient IDs remain case sensitive",
    );
}

#[test]
fn svg_gradient_definition_and_total_stop_work_limits_accept_exact_boundaries() {
    let definitions = |counts: &[usize]| {
        let mut body = String::from("<defs>");
        for (index, &count) in counts.iter().enumerate() {
            body.push_str(&format!("<linearGradient id='g{index}'>"));
            for stop in 0..count {
                body.push_str(&format!(
                    "<stop offset='{}' stop-color='red'/>",
                    stop as f64 / (count - 1) as f64
                ));
            }
            body.push_str("</linearGradient>");
        }
        body.push_str("</defs><rect x='24' y='24' width='120' height='60' fill='url(#g0)'/>");
        svg(&body)
    };
    let accepted = |counts: &[usize], label: &str| {
        let actual = crate::svg_import::parse(definitions(counts).as_bytes())
            .unwrap_or_else(|error| panic!("{label}: {error}"));
        let single = crate::svg_import::parse(definitions(&counts[..1]).as_bytes()).unwrap();
        assert_eq!(
            actual.contents, single.contents,
            "{label}: unused definitions do not leak into native source"
        );
        assert_eq!((actual.width, actual.height), (single.width, single.height));
    };
    accepted(&[2; 64], "64 definitions exactly");
    accepted(&[32; 8], "256 total stops exactly");
    let mut editor = with_pending_redo();
    for (counts, expected_error) in [
        (vec![2; 65], "64 gradients"),
        (vec![32, 32, 32, 32, 32, 32, 32, 31, 2], "256 gradient-stop"),
    ] {
        let source = definitions(&counts);
        let error = crate::svg_import::parse(source.as_bytes()).unwrap_err();
        assert!(
            error.contains(expected_error),
            "must reject for targeted work limit: {error}"
        );
        rejected(&mut editor, &source, expected_error);
    }
}

/// Small explicit native QA packet. Generated inputs are not evidence of native
/// interaction; an existing native-final.lep is read but never created/rewritten.
#[test]
#[ignore = "requires LIBREEFFECTS_SVG_GRADIENT_QA absolute output directory"]
fn svg_gradient_native_qa_packet_and_exact_recorded_save() {
    use std::{
        io::{Cursor, Write},
        path::Path,
    };
    fn immutable(path: &Path, bytes: &[u8]) {
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
            Err(error) => panic!("{}: {error}", path.display()),
        }
    }
    let root = std::env::var_os("LIBREEFFECTS_SVG_GRADIENT_QA")
        .expect("set gradient native QA output directory");
    let root = Path::new(&root);
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let source = basic_attributes();
    immutable(
        &root.join("Imported linear gradients.svg"),
        source.as_bytes(),
    );
    let mut expected = blank();
    immutable(
        &root.join("native-start.lfe.json"),
        expected.project().to_json().unwrap().as_bytes(),
    );
    insert(&mut expected, &source).unwrap();
    let reference = basic_reference();
    let pixels = literal_pixels(&reference);
    pixels_equal(
        &literal_pixels(&source),
        &pixels,
        "native QA independent cubic reference",
    );
    compare_routes(expected.project(), &pixels, "native QA expected source");
    immutable(
        &root.join("native-expected.lfe.json"),
        expected.project().to_json().unwrap().as_bytes(),
    );
    let mut views = crate::view_state::ProjectViews::default();
    views.compositions.insert(
        expected.project().active_composition_id(),
        Default::default(),
    );
    immutable(
        &root.join("native-expected.generated.lep"),
        &project_file::encode(
            expected.project(),
            Some(&views.encode_native(expected.project()).unwrap()),
        )
        .unwrap(),
    );
    let mut png = Cursor::new(Vec::new());
    pixels.write_to(&mut png, image::ImageFormat::Png).unwrap();
    immutable(&root.join("literal.png"), &png.into_inner());
    let native_path = root.join("native-final.lep");
    if native_path.exists() {
        let native_bytes = std::fs::read(&native_path).unwrap();
        let raw = project_file::decode(&native_bytes).unwrap();
        let wire_view: serde_json::Value =
            serde_json::from_slice(raw.view.expect("actual native save must contain VIEW"))
                .unwrap();
        let composition = expected.project().active_composition_id().to_string();
        assert_eq!(
            wire_view["compositions"][&composition]["frame"], 0,
            "recorded raw VIEW must explicitly preserve frame zero"
        );
        let actual = crate::project_io::read_editor_project(&native_path).unwrap();
        assert_eq!(actual.format, crate::project_io::ProjectFormat::Lep);
        source_equal(
            &raw.project,
            expected.project(),
            "recorded raw native full Project",
        );
        source_equal(
            &actual.project,
            expected.project(),
            "recorded native full Project",
        );
        assert_eq!(
            actual
                .views
                .compositions
                .get(&expected.project().active_composition_id())
                .unwrap()
                .frame,
            0
        );
        compare_routes(&actual.project, &pixels, "recorded native gradient import");
        println!(
            "Verified recorded native full source, explicit frame-zero VIEW and exact five-route literal pixels at frames 0/30/60."
        );
    } else {
        println!(
            "Generated immutable native QA inputs and independent literal PNG; no recorded native save exists yet."
        );
    }
}
