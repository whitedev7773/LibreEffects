//! Independent local-gradient-reference acceptance. Native source expectations
//! are authored directly from literal geometry and stop values, never by parsing
//! a second SVG. Every rendered RGBA byte is checked against a separate flattened
//! cubic SVG, with no pixel masks, tolerances, or importer-generated oracle.
use crate::rendering::Renderer;
use libre_effects_core::*;

const WIDTH: u32 = 240;
const HEIGHT: u32 = 160;
const NAME: &str = "Imported gradient references";
const BW_XML: &str = "<stop offset='0' stop-color='black'/><stop offset='1' stop-color='white'/>";
const HARD_XML: &str = "<stop offset='0' stop-color='red'/><stop offset='.5' stop-color='lime' stop-opacity='.5'/><stop offset='.5' stop-color='blue' stop-opacity='.5'/><stop offset='1' stop-color='white' stop-opacity='.75'/>";
const HARD: [(f64, u32, f64); 4] = [
    (0., 0xff0000, 100.),
    (50., 0x00ff00, 50.),
    (50., 0x0000ff, 50.),
    (100., 0xffffff, 75.),
];
const BW: [(f64, u32, f64); 2] = [(0., 0, 100.), (100., 0xffffff, 100.)];
const RECT: &str = "<rect id='top' x='24' y='24' width='120' height='60' fill='url(#ramp)'/>";
const CUBIC: &str =
    "M24 24 C64 24 104 24 144 24 C144 44 144 64 144 84 C104 84 64 84 24 84 C24 64 24 44 24 24 Z";
const LOWER_CUBIC: &str = "M24 108 C84 108 144 108 204 108 C204 118 204 128 204 138 C144 138 84 138 24 138 C24 128 24 118 24 108 Z";

fn svg(body: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='{WIDTH}' height='{HEIGHT}'>{body}</svg>"
    )
}
fn single(definitions: &str) -> String {
    svg(&format!("<defs>{definitions}</defs>{RECT}"))
}
fn reference(attributes: &str, stops: &str) -> String {
    svg(&format!(
        "<defs><linearGradient id='flat' gradientUnits='userSpaceOnUse' {attributes}>{stops}</linearGradient></defs><path d='{CUBIC}' fill='url(#flat)'/>"
    ))
}
fn blank() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "SVG reference independent acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor.clear_history();
    editor
}
fn insert_contents(editor: &mut Editor, contents: ShapeContents) -> Result<(), String> {
    editor.execute(Command::ImportSvg {
        contents,
        width: WIDTH as f64,
        height: HEIGHT as f64,
        name: NAME.into(),
    })
}
fn insert(editor: &mut Editor, source: &str) -> Result<(), String> {
    let parsed = crate::svg_import::parse(source.as_bytes())?;
    assert_eq!((parsed.width, parsed.height), (240., 160.));
    insert_contents(editor, parsed.contents)
}
fn exact(actual: &Project, expected: &Project, label: &str) {
    assert_eq!(actual, expected, "{label}");
    assert_eq!(
        actual.to_json().unwrap(),
        expected.to_json().unwrap(),
        "{label}"
    );
}
fn group(name: &str, children: Vec<ContentsNode>) -> ContentsNode {
    let mut node = ContentsNode::with_defaults(ContentsKind::Group(children));
    node.name = name.into();
    // Identity decomposition stores negative zero for the skew parameter.
    node.set_static_value(ContentsParam::Skew, -0.).unwrap();
    node
}
fn paint(
    endpoints: [f64; 4],
    stops: &[(f64, u32, f64)],
    stroke: bool,
    opacity: f64,
) -> ContentsNode {
    let gradient = ShapeGradient::with_paired_stops(stops.len()).unwrap();
    let mut node = ContentsNode::with_defaults(if stroke {
        ContentsKind::GradientStroke {
            style: ShapeStroke {
                join: StrokeJoin::Miter,
                ..Default::default()
            },
            gradient,
        }
    } else {
        ContentsKind::GradientFill {
            even_odd: false,
            gradient,
        }
    });
    node.name = if stroke {
        "Gradient Stroke"
    } else {
        "Gradient Fill"
    }
    .into();
    node.set_static_value(
        ContentsParam::Shape(if stroke {
            ShapeParam::StrokeOpacity
        } else {
            ShapeParam::FillOpacity
        }),
        opacity,
    )
    .unwrap();
    if stroke {
        node.set_static_value(ContentsParam::Shape(ShapeParam::StrokeWidth), 6.)
            .unwrap();
    }
    use GradientParam::*;
    for (parameter, value) in [StartX, StartY, EndX, EndY].into_iter().zip(endpoints) {
        node.set_static_value(ContentsParam::Gradient(parameter), value)
            .unwrap();
    }
    let count = stops.len() as u64;
    for (index, &(position, rgb, alpha)) in stops.iter().enumerate() {
        let color = index as u64 + 1;
        let opacity = color + count;
        for (parameter, value) in [
            (ColorPosition(color), position),
            (Red(color), ((rgb >> 16) & 255) as f64),
            (Green(color), ((rgb >> 8) & 255) as f64),
            (Blue(color), (rgb & 255) as f64),
            (OpacityPosition(opacity), position),
            (Opacity(opacity), alpha),
        ] {
            node.set_static_value(ContentsParam::Gradient(parameter), value)
                .unwrap();
        }
    }
    node
}
fn rectangle(name: &str, lower: bool, paints: Vec<ContentsNode>) -> ContentsNode {
    // Independently specified corners and cubic control offsets. Negative zero
    // on incoming straight controls is part of the stable serialized source.
    let (top, bottom, right, dx, dy) = if lower {
        (108., 138., 204., 60., 10.)
    } else {
        (24., 84., 144., 40., 20.)
    };
    let mut path = ContentsNode::with_defaults(ContentsKind::Path {
        path: VectorPath {
            vertices: vec![
                PathVertex {
                    position: [24., top],
                    incoming: [-0., dy],
                    outgoing: [dx, 0.],
                },
                PathVertex {
                    position: [right, top],
                    incoming: [-dx, -0.],
                    outgoing: [0., dy],
                },
                PathVertex {
                    position: [right, bottom],
                    incoming: [-0., -dy],
                    outgoing: [-dx, 0.],
                },
                PathVertex {
                    position: [24., bottom],
                    incoming: [dx, -0.],
                    outgoing: [0., -dy],
                },
            ],
            closed: true,
        },
        animation: Default::default(),
    });
    path.name = "Path 1".into();
    let mut children = vec![path];
    children.extend(paints);
    group(name, children)
}
fn expected_contents(endpoints: [f64; 4], stops: &[(f64, u32, f64)]) -> ShapeContents {
    ShapeContents::from_nodes(vec![group(
        "SVG",
        vec![rectangle(
            "top",
            false,
            vec![paint(endpoints, stops, false, 100.)],
        )],
    )])
    .unwrap()
}
fn reuse_source() -> String {
    // Definitions follow both painted objects; references mix SVG2 and correctly
    // namespaced xlink, and the two simultaneous href forms agree exactly.
    svg(&format!(
        "<rect id='top' x='24' y='24' width='120' height='60' fill='url(#ramp)' fill-opacity='.8' stroke='url(#ramp)' stroke-opacity='.6' stroke-width='6'/><rect id='lower' x='24' y='108' width='180' height='30' fill='url(#ramp)'/><defs><linearGradient id='ramp' href='#Middle' xlink:href='#Middle'/><linearGradient id='Middle' xlink:href='#Base'/><linearGradient id='Base'>{HARD_XML}</linearGradient></defs>"
    ))
}
fn reuse_contents(edited: bool) -> ShapeContents {
    let mut lower_stops = HARD;
    if edited {
        lower_stops[0].1 = 0x800000;
    }
    ShapeContents::from_nodes(vec![group(
        "SVG",
        vec![
            rectangle(
                "lower",
                true,
                vec![paint([24., 108., 204., 108.], &lower_stops, false, 100.)],
            ),
            rectangle(
                "top",
                false,
                vec![
                    paint([24., 24., 144., 24.], &HARD, true, 60.),
                    paint([24., 24., 144., 24.], &HARD, false, 80.),
                ],
            ),
        ],
    )])
    .unwrap()
}
fn reuse_reference(edited: bool) -> String {
    let lower = if edited {
        HARD_XML.replacen("stop-color='red'", "stop-color='#800000'", 1)
    } else {
        HARD_XML.into()
    };
    svg(&format!(
        "<defs><linearGradient id='upper' gradientUnits='userSpaceOnUse' x1='24' y1='24' x2='144' y2='24'>{HARD_XML}</linearGradient><linearGradient id='lower' gradientUnits='userSpaceOnUse' x1='24' y1='108' x2='204' y2='108'>{lower}</linearGradient></defs><path d='{CUBIC}' fill='url(#upper)' fill-opacity='.8' stroke='url(#upper)' stroke-opacity='.6' stroke-width='6'/><path d='{LOWER_CUBIC}' fill='url(#lower)'/>"
    ))
}
fn pixels(source: &str) -> image::RgbaImage {
    let tree = resvg::usvg::Tree::from_str(source, &resvg::usvg::Options::default()).unwrap();
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
            .flat_map(|p| {
                let p = p.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            })
            .collect(),
    )
    .unwrap()
}
fn pixels_equal(actual: &image::RgbaImage, expected: &image::RgbaImage, label: &str) {
    assert_eq!(actual.dimensions(), expected.dimensions(), "{label}");
    let differences: Vec<_> = actual
        .enumerate_pixels()
        .filter_map(|(x, y, p)| {
            let expected = expected.get_pixel(x, y);
            (p != expected).then_some((x, y, p.0, expected.0))
        })
        .take(4)
        .collect();
    assert!(
        differences.is_empty(),
        "{label}: first exact RGBA differences {differences:?}"
    );
}
fn compare_routes(actual: &Project, expected: &Project, reference: &str, label: &str) {
    exact(actual, expected, label);
    let expected_pixels = pixels(reference);
    assert!(
        expected_pixels.pixels().any(|pixel| pixel[3] != 0),
        "nonempty oracle: {label}"
    );
    let source = actual.to_json().unwrap();
    let bytes = project_file::encode(actual, None).unwrap();
    let json = Project::from_json(&source).unwrap();
    let lep = project_file::decode(&bytes).unwrap().project;
    exact(
        &json,
        expected,
        "JSON preserves independently expected complete source",
    );
    exact(
        &lep,
        expected,
        "LEP preserves independently expected complete source",
    );
    let renderer = Renderer::new();
    for frame in [0, 30, 60] {
        for (route, image) in [
            ("shared", renderer.render(actual, frame, WIDTH).unwrap()),
            (
                "preview",
                renderer.render_preview(actual, frame, WIDTH).unwrap(),
            ),
            (
                "output",
                renderer
                    .render_output(actual, frame, WIDTH, HEIGHT)
                    .unwrap(),
            ),
            ("JSON", renderer.render(&json, frame, WIDTH).unwrap()),
            ("LEP", renderer.render(&lep, frame, WIDTH).unwrap()),
        ] {
            pixels_equal(
                &image,
                &expected_pixels,
                &format!("{label}: {route} frame {frame}"),
            );
        }
    }
    assert_eq!(
        actual.to_json().unwrap(),
        source,
        "rendering is source-neutral"
    );
    assert_eq!(project_file::encode(actual, None).unwrap(), bytes);
}
fn acceptance(source: &str, contents: ShapeContents, reference: &str, label: &str) -> Editor {
    let parsed =
        crate::svg_import::parse(source.as_bytes()).unwrap_or_else(|e| panic!("{label}: {e}"));
    assert_eq!(
        parsed.contents, contents,
        "{label}: independent complete Contents"
    );
    let mut actual = blank();
    insert(&mut actual, source).unwrap();
    let mut expected = blank();
    insert_contents(&mut expected, contents).unwrap();
    compare_routes(actual.project(), expected.project(), reference, label);
    actual
}

#[test]
fn svg_gradient_reference_forward_mixed_href_reuse_has_independent_source_and_pixels() {
    let source = reuse_source();
    let reference = reuse_reference(false);
    acceptance(
        &source,
        reuse_contents(false),
        &reference,
        "forward mixed href reuse",
    );
    pixels_equal(
        &pixels(&source),
        &pixels(&reference),
        "original versus flattened literal reuse",
    );
    let aliases = source
        .replace("xlink:", "other:")
        .replace("xmlns:xlink", "xmlns:other");
    acceptance(
        &aliases,
        reuse_contents(false),
        &reference,
        "namespace URI determines xlink semantics",
    );
    // The literal hard-stop boundary must not disappear under inheritance.
    let image = pixels(&reference);
    assert_ne!(image.get_pixel(113, 120), image.get_pixel(114, 120));
}

#[test]
fn svg_gradient_reference_final_units_apply_to_inherited_raw_coordinates_and_defaults() {
    for (definitions, endpoints, flattened, label) in [
        (
            format!(
                "<linearGradient id='ramp' href='#base' gradientUnits='userSpaceOnUse'/><linearGradient id='base'>{BW_XML}</linearGradient>"
            ),
            [0., 0., 240., 0.],
            "x1='0' y1='0' x2='240' y2='0'",
            "defaults follow effective user-space units",
        ),
        (
            format!(
                "<linearGradient id='ramp' href='#base' gradientUnits='userSpaceOnUse'/><linearGradient id='base' x1='10%' y1='25%' x2='60%' y2='25%'>{BW_XML}</linearGradient>"
            ),
            [24., 40., 144., 40.],
            "x1='24' y1='40' x2='144' y2='40'",
            "bbox parent percentages become user space",
        ),
        (
            format!(
                "<linearGradient id='ramp' href='#base' gradientUnits='objectBoundingBox'/><linearGradient id='base' gradientUnits='userSpaceOnUse' x1='10%' y1='25%' x2='60%' y2='25%'>{BW_XML}</linearGradient>"
            ),
            [36., 39., 96., 39.],
            "x1='36' y1='39' x2='96' y2='39'",
            "user-space parent percentages become bbox",
        ),
        (
            format!(
                "<linearGradient id='ramp' href='#base' x1='24px'/><linearGradient id='base' gradientUnits='userSpaceOnUse' x1='12px' y1='24px' x2='144px' y2='24px'>{BW_XML}</linearGradient>"
            ),
            [24., 24., 144., 24.],
            "x1='24' y1='24' x2='144' y2='24'",
            "local px uses inherited user-space units",
        ),
    ] {
        acceptance(
            &single(&definitions),
            expected_contents(endpoints, &BW),
            &reference(flattened, BW_XML),
            label,
        );
    }
}

#[test]
fn svg_gradient_reference_closest_attributes_and_transform_replace_instead_of_concatenate() {
    let source = single(&format!(
        "<linearGradient id='ramp' href='#middle' x1='0' gradientTransform='translate(24 24)'/><linearGradient id='middle' href='#base' x1='12' x2='120' gradientTransform='scale(3)'/><linearGradient id='base' gradientUnits='userSpaceOnUse' x1='4' y1='0' x2='60' y2='0' gradientTransform='translate(100 80)' spreadMethod='pad' color-interpolation='sRGB'>{BW_XML}</linearGradient>"
    ));
    let flattened = reference("x1='24' y1='24' x2='144' y2='24'", BW_XML);
    acceptance(
        &source,
        expected_contents([24., 24., 144., 24.], &BW),
        &flattened,
        "closest values and transform replacement",
    );
    pixels_equal(
        &pixels(&source),
        &pixels(&flattened),
        "literal transform replacement",
    );
    let identity = source.replace(
        "gradientTransform='translate(24 24)'",
        "gradientTransform='matrix(1 0 0 1 0 0)'",
    );
    acceptance(
        &identity,
        expected_contents([0., 0., 120., 0.], &BW),
        &reference("x1='0' y1='0' x2='120' y2='0'", BW_XML),
        "explicit identity replaces inherited transforms",
    );
    let defaults = single(&format!(
        "<linearGradient id='ramp' href='#base'/><linearGradient id='base' gradientUnits='userSpaceOnUse' gradientTransform='translate(24 24)'>{BW_XML}</linearGradient>"
    ));
    acceptance(
        &defaults,
        expected_contents([24., 24., 264., 24.], &BW),
        &reference("x1='24' y1='24' x2='264' y2='24'", BW_XML),
        "inherited user-space default percentages and transform",
    );
    let inherited = source.replace(" x1='0' gradientTransform='translate(24 24)'", " x1='0'");
    acceptance(
        &inherited,
        expected_contents([0., 0., 360., 0.], &BW),
        &reference("x1='0' y1='0' x2='360' y2='0'", BW_XML),
        "nearest inherited transform replaces earlier transform",
    );
}

#[test]
fn svg_gradient_reference_local_stops_wholly_replace_inherited_hard_stops() {
    let source = single(&format!(
        "<linearGradient id='ramp' href='#middle'/><linearGradient id='middle' href='#base'>{BW_XML}</linearGradient><linearGradient id='base'>{HARD_XML}</linearGradient>"
    ));
    acceptance(
        &source,
        expected_contents([24., 24., 144., 24.], &BW),
        &reference("x1='24' y1='24' x2='144' y2='24'", BW_XML),
        "nearest local stops wholly replace",
    );
    let local = single(&format!(
        "<linearGradient id='ramp' href='#base'>{HARD_XML}</linearGradient><linearGradient id='base'>{BW_XML}</linearGradient>"
    ));
    acceptance(
        &local,
        expected_contents([24., 24., 144., 24.], &HARD),
        &reference("x1='24' y1='24' x2='144' y2='24'", HARD_XML),
        "local coincident stops remain exact",
    );
}

#[test]
fn svg_gradient_reference_reuse_is_independently_editable_with_exact_history_and_codecs() {
    let mut editor = blank();
    let before = editor.project().clone();
    insert(&mut editor, &reuse_source()).unwrap();
    let mut expected = blank();
    insert_contents(&mut expected, reuse_contents(false)).unwrap();
    let imported = expected.project().clone();
    exact(editor.project(), &imported, "independent imported Project");
    editor.undo();
    exact(
        editor.project(),
        &before,
        "one undo removes complete import",
    );
    assert!(!editor.can_undo() && editor.can_redo());
    editor.redo();
    exact(
        editor.project(),
        &imported,
        "one redo preserves every native ID",
    );
    assert!(editor.can_undo() && !editor.can_redo());
    let layer = editor.project().composition().layers()[0].id();
    // Stable independently specified depth-first ID: root=1, lower=2,
    // lower path=3, lower fill=4, top=5, top path=6, top stroke=7, top fill=8.
    editor
        .execute(Command::Contents {
            id: layer,
            edit: ContentsEdit::Track {
                item: 4,
                parameter: ContentsParam::Gradient(GradientParam::Red(1)),
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 128.,
                },
            },
        })
        .unwrap();
    let mut edited = blank();
    insert_contents(&mut edited, reuse_contents(true)).unwrap();
    compare_routes(
        editor.project(),
        edited.project(),
        &reuse_reference(true),
        "editing one reused paint leaves the other paints exact",
    );
    editor.undo();
    exact(
        editor.project(),
        &imported,
        "undo one independent paint edit",
    );
    editor.redo();
    exact(
        editor.project(),
        edited.project(),
        "redo one independent paint edit",
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("references.svg");
    std::fs::write(&path, reuse_source()).unwrap();
    let read = crate::svg_import::read_svg_file(&path).unwrap();
    assert_eq!(
        read.contents,
        reuse_contents(false),
        "local file route matches static source oracle"
    );
}

fn pending_redo() -> Editor {
    let mut editor = blank();
    insert_contents(&mut editor, reuse_contents(false)).unwrap();
    insert_contents(&mut editor, reuse_contents(true)).unwrap();
    editor.undo();
    assert!(editor.can_undo() && editor.can_redo());
    editor
}
fn reject(editor: &mut Editor, source: &str, label: &str) {
    let before = editor.project().clone();
    let bytes = project_file::encode(&before, None).unwrap();
    let history = (editor.can_undo(), editor.can_redo());
    let error = insert(editor, source).expect_err(label);
    assert!(!error.trim().is_empty(), "actionable rejection: {label}");
    exact(editor.project(), &before, label);
    assert_eq!(
        project_file::encode(editor.project(), None).unwrap(),
        bytes,
        "{label}"
    );
    assert_eq!((editor.can_undo(), editor.can_redo()), history, "{label}");
    // A failed parse cannot merely retain the flags while changing redo data.
    editor.redo();
    editor.undo();
    exact(
        editor.project(),
        &before,
        "rejection preserves redo payload",
    );
}

#[test]
fn svg_gradient_reference_resource_namespace_and_conflict_rejections_are_atomic() {
    let mut editor = pending_redo();
    for href in [
        "",
        "#",
        "#missing",
        "#base#other",
        "#b%61se",
        "#BASE",
        "#1base",
        "#bâse",
        "base",
        "url(#base)",
        "https://invalid.example/g.svg#base",
        "//invalid.example/g.svg#base",
        "file:///tmp/gradient.svg#base",
        "data:image/svg+xml;base64,PHN2Zz4=",
        "javascript:alert(1)",
        " #base",
        "#base ",
    ] {
        let source = single(&format!(
            "<linearGradient id='ramp' href='{href}'>{HARD_XML}</linearGradient><linearGradient id='base'>{BW_XML}</linearGradient>"
        ));
        reject(&mut editor, &source, href);
    }
    for attributes in [
        "href='#base' xlink:href='#other'",
        "href='#base' xlink:href='https://invalid.example/a.svg#base'",
        "href='https://invalid.example/a.svg#base' xlink:href='#base'",
        "xmlns:wrong='https://invalid.example/ns' wrong:href='#base'",
        "xmlns:xlink='https://invalid.example/ns' xlink:href='#base'",
        "xmlns:s='http://www.w3.org/2000/svg' s:href='#base'",
        "href='#base' xlink:title='ignored'",
        "href='#base' style='x1:0'",
        "href='#base' class='paint'",
    ] {
        reject(
            &mut editor,
            &single(&format!(
                "<linearGradient id='ramp' {attributes}/><linearGradient id='base'>{BW_XML}</linearGradient><linearGradient id='other'>{BW_XML}</linearGradient>"
            )),
            attributes,
        );
    }
    for target in [
        "<title id='base'>Title</title>",
        "<desc id='base'>Description</desc>",
        "<radialGradient id='base'/>",
    ] {
        reject(
            &mut editor,
            &single(&format!(
                "<linearGradient id='ramp' href='#base'>{BW_XML}</linearGradient>{target}"
            )),
            "target must be a supported local linearGradient",
        );
    }
    reject(
        &mut editor,
        &svg(&format!(
            "<defs><linearGradient id='ramp' href='#top'>{BW_XML}</linearGradient></defs>{RECT}"
        )),
        "a geometry ID is not a paint server",
    );
}

#[test]
fn svg_gradient_reference_preflights_unused_and_shadowed_invalid_definitions() {
    let mut editor = pending_redo();
    for invalid in [
        "href='#missing'",
        "href='#base'",
        "href='file:///tmp/private.svg#paint'",
        "gradientUnits='bad'",
        "x1='NaN'",
        "x2='1e309'",
        "x1='1em'",
        "spreadMethod='repeat'",
        "color-interpolation='linearRGB'",
        "gradientTransform='scale(0)'",
        "gradientTransform='matrix(1 0 0 1 0)'",
    ] {
        let defs = format!(
            "<linearGradient id='ramp'>{BW_XML}</linearGradient><linearGradient id='base' {invalid}>{HARD_XML}</linearGradient>"
        );
        reject(&mut editor, &single(&defs), "unused invalid definition");
        let shadowed = format!(
            "<linearGradient id='ramp' href='#base' gradientUnits='userSpaceOnUse' x1='0' x2='240' spreadMethod='pad' color-interpolation='sRGB' gradientTransform='translate(0 0)'>{BW_XML}</linearGradient><linearGradient id='base' {invalid}>{HARD_XML}</linearGradient>"
        );
        reject(
            &mut editor,
            &single(&shadowed),
            "overridden base value is still validated",
        );
    }
    let inherited_px = single(&format!(
        "<linearGradient id='ramp' href='#base' gradientUnits='objectBoundingBox'/><linearGradient id='base' gradientUnits='userSpaceOnUse' x1='0px' x2='120px'>{BW_XML}</linearGradient>"
    ));
    reject(
        &mut editor,
        &inherited_px,
        "inherited px is invalid in effective objectBoundingBox units",
    );
    let empty_base = single(&format!(
        "<linearGradient id='ramp' href='#base'>{BW_XML}</linearGradient><linearGradient id='base' x2='1'/>"
    ));
    reject(
        &mut editor,
        &empty_base,
        "all definitions need effective stops, including a locally shadowed base",
    );
    for defs in [
        format!("<linearGradient id='ramp' href='#ramp'>{BW_XML}</linearGradient>"),
        format!(
            "<linearGradient id='ramp'>{BW_XML}</linearGradient><linearGradient id='a' href='#b'>{BW_XML}</linearGradient><linearGradient id='b' href='#a'>{BW_XML}</linearGradient>"
        ),
        format!(
            "<linearGradient id='ramp' href='#base'>{BW_XML}</linearGradient><linearGradient id='base'><stop offset='0' stop-opacity='2'/><stop offset='1'/></linearGradient>"
        ),
        format!(
            "<linearGradient id='ramp' href='#base'><stop offset='0'/></linearGradient><linearGradient id='base'>{BW_XML}</linearGradient>"
        ),
        format!("<linearGradient id='ramp' href='#base'/><linearGradient id='base'/>"),
        format!(
            "<linearGradient id='ramp' href='#base'/><linearGradient id='base'>{BW_XML}</linearGradient><linearGradient id='base'>{BW_XML}</linearGradient>"
        ),
    ] {
        reject(
            &mut editor,
            &single(&defs),
            "invalid chain or local stops cannot be skipped",
        );
    }
}

fn chain(edges: usize, reverse: bool) -> String {
    let mut definitions = (0..edges)
        .map(|i| format!("<linearGradient id='g{i}' href='#g{}'/>", i + 1))
        .collect::<Vec<_>>();
    definitions.push(format!(
        "<linearGradient id='g{edges}'>{BW_XML}</linearGradient>"
    ));
    if reverse {
        definitions.reverse();
    }
    single(&definitions.join("")).replace("url(#ramp)", "url(#g0)")
}
fn stops(count: usize) -> String {
    (0..count)
        .map(|i| {
            format!(
                "<stop offset='{}' stop-color='red'/>",
                i as f64 / (count - 1) as f64
            )
        })
        .collect()
}
fn fanout(count: usize, stop_count: usize) -> String {
    let mut definitions = format!(
        "<linearGradient id='base'>{}</linearGradient>",
        stops(stop_count)
    );
    for i in 1..count {
        definitions.push_str(&format!("<linearGradient id='g{i}' href='#base'/>"));
    }
    single(&definitions).replace("url(#ramp)", "url(#base)")
}

#[test]
fn svg_gradient_reference_chain_definition_source_and_resolved_stop_boundaries() {
    let expected = expected_contents([24., 24., 144., 24.], &BW);
    for reverse in [false, true] {
        let parsed = crate::svg_import::parse(chain(16, reverse).as_bytes())
            .expect("16 reference edges are allowed");
        assert_eq!(
            parsed.contents, expected,
            "definition order cannot change chain depth or native source"
        );
    }
    let red = [(0., 0xff0000, 100.), (100., 0xff0000, 100.)];
    assert_eq!(
        crate::svg_import::parse(fanout(64, 2).as_bytes())
            .unwrap()
            .contents,
        expected_contents([24., 24., 144., 24.], &red),
        "64 definitions are allowed"
    );
    let parsed =
        crate::svg_import::parse(fanout(8, 32).as_bytes()).expect("256 resolved stops exactly");
    let rows = parsed.contents.rows();
    let gradient = rows.iter().find_map(|(_, _, n)| n.kind.gradient()).unwrap();
    assert_eq!((gradient.colors.len(), gradient.opacities.len()), (32, 32));
    let direct = (0..8)
        .map(|i| format!("<linearGradient id='g{i}'>{}</linearGradient>", stops(32)))
        .collect::<String>();
    crate::svg_import::parse(single(&direct).replace("url(#ramp)", "url(#g0)").as_bytes())
        .expect("256 physical source stops exactly");
    let mut editor = pending_redo();
    for reverse in [false, true] {
        reject(
            &mut editor,
            &chain(17, reverse),
            "17 reference edges exceed the chain limit",
        );
    }
    reject(&mut editor, &fanout(65, 2), "65 definitions");
    reject(
        &mut editor,
        &fanout(9, 32),
        "32 physical stops amplified into 288 resolved stops, even when aliases are unused",
    );
    let over = [32, 32, 32, 32, 32, 32, 32, 31, 2]
        .into_iter()
        .enumerate()
        .map(|(i, count)| {
            format!(
                "<linearGradient id='g{i}'>{}</linearGradient>",
                stops(count)
            )
        })
        .collect::<String>();
    reject(
        &mut editor,
        &single(&over).replace("url(#ramp)", "url(#g0)"),
        "257 physical source stops",
    );
}

/// Immutable test inputs are distinct from a native save made through the UI.
/// This helper never creates or rewrites native-final.lep.
#[test]
#[ignore = "requires LIBREEFFECTS_SVG_REFERENCE_QA absolute output directory"]
fn svg_gradient_reference_native_qa_packet_and_exact_recorded_save() {
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
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => assert_eq!(
                std::fs::read(path).unwrap(),
                bytes,
                "immutable {}",
                path.display()
            ),
            Err(e) => panic!("{}: {e}", path.display()),
        }
    }
    let root = std::env::var_os("LIBREEFFECTS_SVG_REFERENCE_QA")
        .expect("set reference native QA directory");
    let root = Path::new(&root);
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let mut expected = blank();
    immutable(
        &root.join("native-start.lfe.json"),
        expected.project().to_json().unwrap().as_bytes(),
    );
    immutable(&root.join(format!("{NAME}.svg")), reuse_source().as_bytes());
    insert_contents(&mut expected, reuse_contents(false)).unwrap();
    let mut actual = blank();
    insert(&mut actual, &reuse_source()).unwrap();
    compare_routes(
        actual.project(),
        expected.project(),
        &reuse_reference(false),
        "native QA independent source and reference",
    );
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
    pixels(&reuse_reference(false))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    immutable(&root.join("literal.png"), &png.into_inner());
    let saved = root.join("native-final.lep");
    if saved.exists() {
        let bytes = std::fs::read(&saved).unwrap();
        let decoded = project_file::decode(&bytes).unwrap();
        let view: serde_json::Value =
            serde_json::from_slice(decoded.view.expect("recorded save contains VIEW")).unwrap();
        assert_eq!(
            view["compositions"][expected.project().active_composition_id().to_string()]["frame"],
            0
        );
        let loaded = crate::project_io::read_editor_project(&saved).unwrap();
        assert_eq!(loaded.format, crate::project_io::ProjectFormat::Lep);
        exact(
            &decoded.project,
            expected.project(),
            "raw native save preserves independently expected full source",
        );
        compare_routes(
            &loaded.project,
            expected.project(),
            &reuse_reference(false),
            "recorded native reference import",
        );
        println!(
            "Verified recorded native source, explicit frame-zero VIEW, and exact five-route literal pixels at frames 0/30/60."
        );
    } else {
        println!(
            "Created immutable QA inputs and independent source/pixel oracles. No recorded native save exists yet."
        );
    }
}
