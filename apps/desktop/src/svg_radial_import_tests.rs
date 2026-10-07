//! Independent bounded radial-import acceptance. Native source is hand-authored;
//! pixel references are literal cubic geometry and explicit user-space circles.
//! No importer-generated oracle, image masking, or blanket tolerance is used.
use crate::rendering::Renderer;
use libre_effects_core::*;

const WIDTH: u32 = 240;
const HEIGHT: u32 = 160;
const NAME: &str = "import";
const BW_XML: &str = "<stop offset='0' stop-color='black'/><stop offset='1' stop-color='white'/>";
const HARD_XML: &str = "<stop offset='0' stop-color='red'/><stop offset='.5' stop-color='lime' stop-opacity='.5'/><stop offset='.5' stop-color='blue' stop-opacity='.5'/><stop offset='1' stop-color='white' stop-opacity='.75'/>";
const BW: [(f64, u32, f64); 2] = [(0., 0, 100.), (100., 0xffffff, 100.)];
const HARD: [(f64, u32, f64); 4] = [
    (0., 0xff0000, 100.),
    (50., 0x00ff00, 50.),
    (50., 0x0000ff, 50.),
    (100., 0xffffff, 75.),
];
const RECT: &str = "<rect id='top' x='24' y='24' width='60' height='60' fill='url(#ramp)'/>";
const CUBIC: &str =
    "M24 24 C44 24 64 24 84 24 C84 44 84 64 84 84 C64 84 44 84 24 84 C24 64 24 44 24 24 Z";
const WIDE_CUBIC: &str =
    "M24 24 C64 24 104 24 144 24 C144 44 144 64 144 84 C104 84 64 84 24 84 C24 64 24 44 24 24 Z";
const LOWER_CUBIC: &str = "M132 72 C152 72 172 72 192 72 C192 92 192 112 192 132 C172 132 152 132 132 132 C132 112 132 92 132 72 Z";

fn svg(body: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='{WIDTH}' height='{HEIGHT}'>{body}</svg>"
    )
}
fn single(definitions: &str) -> String {
    svg(&format!("<defs>{definitions}</defs>{RECT}"))
}
fn radial(attributes: &str) -> String {
    single(&format!(
        "<radialGradient id='ramp' {attributes}>{BW_XML}</radialGradient>"
    ))
}
fn reference(attributes: &str, stops: &str, path: &str) -> String {
    svg(&format!(
        "<defs><radialGradient id='flat' gradientUnits='userSpaceOnUse' {attributes}>{stops}</radialGradient></defs><path d='{path}' fill='url(#flat)'/>"
    ))
}
fn blank() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "SVG radial independent acceptance".into(),
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
    node.set_static_value(ContentsParam::Skew, -0.).unwrap();
    node
}
// circle = center x/y, radius, positive highlight percent, highlight degrees.
fn paint(circle: [f64; 5], stops: &[(f64, u32, f64)], stroke: bool, opacity: f64) -> ContentsNode {
    let mut gradient = ShapeGradient::with_paired_stops(stops.len()).unwrap();
    gradient.radial = true;
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
    let [x, y, radius, length, angle] = circle;
    use GradientParam::*;
    for (parameter, value) in [
        (StartX, x),
        (StartY, y),
        (EndX, x + radius),
        (EndY, y),
        (HighlightLength, length),
        (HighlightAngle, angle),
    ] {
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
fn rectangle(name: &str, bounds: [f64; 4], paints: Vec<ContentsNode>) -> ContentsNode {
    let [x, y, w, h] = bounds;
    let (right, bottom, dx, dy) = (x + w, y + h, w / 3., h / 3.);
    let mut path = ContentsNode::with_defaults(ContentsKind::Path {
        path: VectorPath {
            vertices: vec![
                PathVertex {
                    position: [x, y],
                    incoming: [-0., dy],
                    outgoing: [dx, 0.],
                },
                PathVertex {
                    position: [right, y],
                    incoming: [-dx, -0.],
                    outgoing: [0., dy],
                },
                PathVertex {
                    position: [right, bottom],
                    incoming: [-0., -dy],
                    outgoing: [-dx, 0.],
                },
                PathVertex {
                    position: [x, bottom],
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
fn expected(circle: [f64; 5], width: f64, stops: &[(f64, u32, f64)]) -> ShapeContents {
    ShapeContents::from_nodes(vec![group(
        "SVG",
        vec![rectangle(
            "top",
            [24., 24., width, 60.],
            vec![paint(circle, stops, false, 100.)],
        )],
    )])
    .unwrap()
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
        expected_pixels.pixels().any(|p| p[3] != 0),
        "nonempty oracle: {label}"
    );
    let source = actual.to_json().unwrap();
    let bytes = project_file::encode(actual, None).unwrap();
    let json = Project::from_json(&source).unwrap();
    let lep = project_file::decode(&bytes).unwrap().project;
    exact(
        &json,
        expected,
        "JSON preserves independently authored complete source",
    );
    exact(
        &lep,
        expected,
        "LEP preserves independently authored complete source",
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
fn svg_radial_defaults_and_signed_focus_directions_have_independent_source_and_pixels() {
    for (attributes, circle, flat, label) in [
        (
            "",
            [54., 54., 30., 0., 0.],
            "cx='54' cy='54' r='30' fx='54' fy='54'",
            "default bbox circle",
        ),
        (
            "fr='0%' fx='75%'",
            [54., 54., 30., 50., 0.],
            "cx='54' cy='54' r='30' fx='69' fy='54'",
            "right focus",
        ),
        (
            "fy='75%'",
            [54., 54., 30., 50., 90.],
            "cx='54' cy='54' r='30' fx='54' fy='69'",
            "down focus",
        ),
        (
            "fx='25%'",
            [54., 54., 30., 50., 180.],
            "cx='54' cy='54' r='30' fx='39' fy='54'",
            "left focus",
        ),
        (
            "fy='25%'",
            [54., 54., 30., 50., -90.],
            "cx='54' cy='54' r='30' fx='54' fy='39'",
            "up focus",
        ),
        (
            "cx='25%' cy='75%' r='25%'",
            [39., 69., 15., 0., 0.],
            "cx='39' cy='69' r='15' fx='39' fy='69'",
            "omitted focus uses explicit center",
        ),
    ] {
        let source = radial(attributes);
        let flat = reference(flat, BW_XML, CUBIC);
        acceptance(&source, expected(circle, 60., &BW), &flat, label);
        pixels_equal(&pixels(&source), &pixels(&flat), label);
    }
}

#[test]
fn svg_radial_reference_chain_defers_focus_defaults_and_replaces_transforms_and_stops() {
    for (defs, circle, flat, label) in [
        (
            format!(
                "<radialGradient id='ramp' href='#middle' xlink:href='#middle' cx='60' cy='48'/><radialGradient id='middle' xlink:href='#base'/><radialGradient id='base' gradientUnits='userSpaceOnUse' cx='30' cy='32' r='30'>{BW_XML}</radialGradient>"
            ),
            [60., 48., 30., 0., 0.],
            "cx='60' cy='48' r='30' fx='60' fy='48'",
            "absent inherited focus follows final center",
        ),
        (
            format!(
                "<radialGradient id='ramp' href='#base' cx='60' cy='48'/><radialGradient id='base' gradientUnits='userSpaceOnUse' cx='30' cy='32' r='30' fx='45'>{BW_XML}</radialGradient>"
            ),
            [60., 48., 30., 50., 180.],
            "cx='60' cy='48' r='30' fx='45' fy='48'",
            "explicit inherited x focus survives center override",
        ),
        (
            format!(
                "<radialGradient id='ramp' href='#base' gradientUnits='userSpaceOnUse' r='40'/><radialGradient id='base' cx='25%' cy='25%' fx='25%' fy='37.5%'>{BW_XML}</radialGradient>"
            ),
            [60., 40., 40., 50., 90.],
            "cx='60' cy='40' r='40' fx='60' fy='60'",
            "effective units precede percent resolution",
        ),
        (
            format!(
                "<radialGradient id='ramp' href='#middle' gradientTransform='translate(24 24)'/><radialGradient id='middle' href='#base' gradientTransform='scale(3)'>{BW_XML}</radialGradient><radialGradient id='base' gradientUnits='userSpaceOnUse' cx='30' cy='30' r='30' fx='45' gradientTransform='translate(100 80)'>{HARD_XML}</radialGradient>"
            ),
            [54., 54., 30., 50., 0.],
            "cx='54' cy='54' r='30' fx='69' fy='54'",
            "nearest transform and local stop list replace inherited values",
        ),
        (
            format!(
                "<radialGradient id='ramp' href='#base' gradientUnits='objectBoundingBox'/><radialGradient id='base' gradientUnits='userSpaceOnUse' cx='50%' cy='50%' r='50%' fy='25%'>{BW_XML}</radialGradient>"
            ),
            [54., 54., 30., 50., -90.],
            "cx='54' cy='54' r='30' fx='54' fy='39'",
            "user-space template percentages become bbox",
        ),
    ] {
        let source = single(&defs);
        acceptance(
            &source,
            expected(circle, 60., &BW),
            &reference(flat, BW_XML, CUBIC),
            label,
        );
    }
    let aliases=single(&format!("<radialGradient id='ramp' xlink:href='#base'/><radialGradient id='base'>{BW_XML}</radialGradient>"))
        .replace("xlink:","other:").replace("xmlns:xlink","xmlns:other");
    acceptance(
        &aliases,
        expected([54., 54., 30., 0., 0.], 60., &BW),
        &reference("cx='54' cy='54' r='30'", BW_XML, CUBIC),
        "namespace URI controls xlink",
    );
}

#[test]
fn svg_radial_user_space_percent_radius_uses_normalized_viewport_and_viewbox_diagonal() {
    // Independent Euclidean normalized-diagonal calculation, not the average
    // viewport dimension, bbox size, or either coordinate axis.
    let diagonal = 240_f64.hypot(160.) / 2_f64.sqrt();
    for (attributes, circle, flat) in [
        (
            "gradientUnits='userSpaceOnUse'",
            [120., 80., diagonal / 2., 0., 0.],
            format!("cx='120' cy='80' r='{}' fx='120' fy='80'", diagonal / 2.),
        ),
        (
            "gradientUnits='userSpaceOnUse' cx='25%' cy='25%' r='25%'",
            [60., 40., diagonal / 4., 0., 0.],
            format!("cx='60' cy='40' r='{}' fx='60' fy='40'", diagonal / 4.),
        ),
    ] {
        acceptance(
            &radial(attributes),
            expected(circle, 60., &BW),
            &reference(&flat, BW_XML, CUBIC),
            "non-square viewport radius percentage",
        );
    }
    let radius = (120_f64.hypot(80.) / 2_f64.sqrt()) / 4.;
    let source = radial("gradientUnits='userSpaceOnUse' r='25%'")
        .replace("width='240'", "width='240' viewBox='10 20 120 80'");
    let mut viewbox = group(
        "ViewBox",
        vec![rectangle(
            "top",
            [24., 24., 60., 60.],
            vec![paint([60., 40., radius, 0., 0.], &BW, false, 100.)],
        )],
    );
    for (parameter, value) in [
        (Property::PositionX, -20.),
        (Property::PositionY, -40.),
        (Property::ScaleX, 200.),
        (Property::ScaleY, 200.),
    ] {
        viewbox
            .set_static_value(ContentsParam::Transform(parameter), value)
            .unwrap();
    }
    let contents = ShapeContents::from_nodes(vec![group("SVG", vec![viewbox])]).unwrap();
    let flat = reference(
        &format!("cx='100' cy='40' r='{}' fx='100' fy='40'", radius * 2.),
        BW_XML,
        "M28 8 C68 8 108 8 148 8 C148 48 148 88 148 128 C108 128 68 128 28 128 C28 88 28 48 28 8 Z",
    );
    acceptance(
        &source,
        contents,
        &flat,
        "viewBox dimensions determine percentage radius before placement",
    );
    pixels_equal(
        &pixels(&source),
        &pixels(&flat),
        "literal viewBox radial placement",
    );
    let angle = 20_f64.atan2(15.).to_degrees();
    acceptance(
        &radial("gradientUnits='userSpaceOnUse' cx='54' cy='54' r='50' fx='69' fy='74'"),
        expected([54., 54., 50., 50., angle], 60., &BW),
        &reference("cx='54' cy='54' r='50' fx='69' fy='74'", BW_XML, CUBIC),
        "independent 3-4-5 focus angle and positive length",
    );
}

#[test]
fn svg_radial_similarity_compensation_rotation_reflection_and_replacement_are_exact() {
    for (attributes, circle, width, path, flat, label) in [
        (
            "gradientTransform='matrix(.5 0 0 1 .25 0)' fx='.75'",
            [84., 54., 30., 50., 0.],
            120.,
            WIDE_CUBIC,
            "cx='84' cy='54' r='30' fx='99' fy='54'",
            "bbox anisotropy compensated by gradient transform",
        ),
        (
            "gradientUnits='userSpaceOnUse' cx='0' cy='0' r='15' fx='7.5' gradientTransform='matrix(0 2 -2 0 54 54)'",
            [54., 54., 30., 50., 90.],
            60.,
            CUBIC,
            "cx='54' cy='54' r='30' fx='54' fy='69'",
            "scaled quarter turn",
        ),
        (
            "gradientUnits='userSpaceOnUse' cx='0' cy='0' r='30' fx='15' gradientTransform='matrix(-1 0 0 1 54 54)'",
            [54., 54., 30., 50., 180.],
            60.,
            CUBIC,
            "cx='54' cy='54' r='30' fx='39' fy='54'",
            "reflected focus",
        ),
    ] {
        let source = radial(attributes).replace("width='60'", &format!("width='{width}'"));
        let flat = reference(flat, BW_XML, path);
        acceptance(&source, expected(circle, width, &BW), &flat, label);
        pixels_equal(&pixels(&source), &pixels(&flat), label);
    }
    let source = single(&format!(
        "<radialGradient id='ramp' href='#base' gradientTransform='matrix(1 0 0 1 0 0)'/><radialGradient id='base' gradientUnits='userSpaceOnUse' cx='54' cy='54' r='30' fx='69' gradientTransform='scale(2)'>{BW_XML}</radialGradient>"
    ));
    acceptance(
        &source,
        expected([54., 54., 30., 50., 0.], 60., &BW),
        &reference("cx='54' cy='54' r='30' fx='69'", BW_XML, CUBIC),
        "explicit identity replaces inherited similarity transform",
    );
}

#[test]
fn svg_radial_outer_nonuniform_and_shear_groups_remain_native_transforms() {
    for (matrix, scale, skew, label) in [
        (
            "2 0 0 1 0 0",
            [200., 100.],
            -0.,
            "nonuniform group remains editable",
        ),
        (
            "1 0 .5 1 0 0",
            [100., 100.],
            -0.5_f64.atan().to_degrees(),
            "shear group remains editable",
        ),
    ] {
        let source = svg(&format!(
            "<defs><radialGradient id='ramp' fx='.75'>{BW_XML}</radialGradient></defs><g id='outer' transform='matrix({matrix})'>{RECT}</g>"
        ));
        let mut outer = group(
            "outer",
            vec![rectangle(
                "top",
                [24., 24., 60., 60.],
                vec![paint([54., 54., 30., 50., 0.], &BW, false, 100.)],
            )],
        );
        outer
            .set_static_value(ContentsParam::Transform(Property::ScaleX), scale[0])
            .unwrap();
        outer
            .set_static_value(ContentsParam::Transform(Property::ScaleY), scale[1])
            .unwrap();
        outer.set_static_value(ContentsParam::Skew, skew).unwrap();
        let contents = ShapeContents::from_nodes(vec![group("SVG", vec![outer])]).unwrap();
        let flat = svg(&format!(
            "<defs><radialGradient id='flat' gradientUnits='userSpaceOnUse' cx='54' cy='54' r='30' fx='69' fy='54'>{BW_XML}</radialGradient></defs><g transform='matrix({matrix})'><path d='{CUBIC}' fill='url(#flat)'/></g>"
        ));
        acceptance(&source, contents, &flat, label);
    }
}

fn reuse_source() -> String {
    svg(&format!(
        "<rect id='top' x='24' y='24' width='60' height='60' fill='url(#ramp)' fill-opacity='.8' stroke='url(#ramp)' stroke-opacity='.6' stroke-width='6'/><rect id='lower' x='132' y='72' width='60' height='60' fill='url(#ramp)'/><defs><radialGradient id='ramp' href='#middle' xlink:href='#middle'/><radialGradient id='middle' xlink:href='#base'/><radialGradient id='base' fx='.75'>{HARD_XML}</radialGradient></defs>"
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
                [132., 72., 60., 60.],
                vec![paint(
                    [162., 102., 30., if edited { 25. } else { 50. }, 0.],
                    &lower_stops,
                    false,
                    100.,
                )],
            ),
            rectangle(
                "top",
                [24., 24., 60., 60.],
                vec![
                    paint([54., 54., 30., 50., 0.], &HARD, true, 60.),
                    paint([54., 54., 30., 50., 0.], &HARD, false, 80.),
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
    let focus = if edited { 169.5 } else { 177. };
    svg(&format!(
        "<defs><radialGradient id='upper' gradientUnits='userSpaceOnUse' cx='54' cy='54' r='30' fx='69' fy='54'>{HARD_XML}</radialGradient><radialGradient id='lower' gradientUnits='userSpaceOnUse' cx='162' cy='102' r='30' fx='{focus}' fy='102'>{lower}</radialGradient></defs><path d='{CUBIC}' fill='url(#upper)' fill-opacity='.8' stroke='url(#upper)' stroke-opacity='.6' stroke-width='6'/><path d='{LOWER_CUBIC}' fill='url(#lower)'/>"
    ))
}

#[test]
fn svg_radial_reuse_is_independently_editable_and_preserves_source_history_and_codecs() {
    let source = reuse_source();
    let mut editor = acceptance(
        &source,
        reuse_contents(false),
        &reuse_reference(false),
        "mixed href hard-stop fill and stroke reuse",
    );
    pixels_equal(
        &pixels(&source),
        &pixels(&reuse_reference(false)),
        "original versus literal reuse",
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&editor.project().to_json().unwrap()).unwrap()["version"],
        45,
        "static radial import needs only the existing gradient schema"
    );
    let imported = editor.project().clone();
    editor.undo();
    exact(
        editor.project(),
        blank().project(),
        "one undo removes the complete import",
    );
    assert!(!editor.can_undo() && editor.can_redo());
    editor.redo();
    exact(editor.project(), &imported, "redo restores all source IDs");
    let layer = editor.project().composition().layers()[0].id();
    // Stable independently specified IDs: root=1; lower=2,path=3,fill=4;
    // top=5,path=6,stroke=7,fill=8. Reuse must not alias any paint tracks.
    for (parameter, value) in [
        (GradientParam::Red(1), 128.),
        (GradientParam::HighlightLength, 25.),
    ] {
        editor
            .execute(Command::Contents {
                id: layer,
                edit: ContentsEdit::Track {
                    item: 4,
                    parameter: ContentsParam::Gradient(parameter),
                    edit: TrackEdit::Value { frame: 0, value },
                },
            })
            .unwrap();
    }
    let mut edited = blank();
    insert_contents(&mut edited, reuse_contents(true)).unwrap();
    compare_routes(
        editor.project(),
        edited.project(),
        &reuse_reference(true),
        "editing one reused radial leaves both top paints exact",
    );
    editor.undo();
    editor.undo();
    exact(
        editor.project(),
        &imported,
        "two independent edits undo without affecting import",
    );
    editor.redo();
    editor.redo();
    exact(editor.project(), edited.project(), "redo both scalar edits");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("import.svg");
    std::fs::write(&path, &source).unwrap();
    assert_eq!(
        crate::svg_import::read_svg_file(&path).unwrap().contents,
        reuse_contents(false)
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
    editor.redo();
    editor.undo();
    exact(
        editor.project(),
        &before,
        "rejection preserves actual redo payload",
    );
}

#[test]
fn svg_radial_nonrepresentable_fields_and_precision_loss_reject_atomically() {
    let mut editor = pending_redo();
    for attributes in [
        "r='0'",
        "r='-1'",
        "r='1e-12'",
        "gradientUnits='userSpaceOnUse' r='0.000244140625'",
        "fr='.000001'",
        "fr='1%'",
        "fr='-1'",
        "fx='1'",
        "fx='1.1'",
        "fx='-.1'",
        "fy='1'",
        "fx='.99951'",
        "gradientTransform='scale(2 1)'",
        "gradientTransform='matrix(1 0 .01 1 0 0)'",
        "gradientTransform='scale(0)'",
        "gradientTransform='scale(1e-10)'",
        "gradientUnits='userSpaceOnUse' cx='999999' cy='54' r='2'",
        "gradientUnits='userSpaceOnUse' cx='0' cy='0' r='1' fx='.5' gradientTransform='matrix(.02 0 0 .02 100000 0)'",
        "gradientUnits='userSpaceOnUse' cx='0' cy='0' r='1' fx='.00001' gradientTransform='scale(100)'",
        "gradientUnits='userSpaceOnUse' cx='0' cy='0' r='1' fx='.00001' gradientTransform='scale(1000)'",
    ] {
        reject(&mut editor, &radial(attributes), attributes);
    }
    reject(
        &mut editor,
        &radial("").replace("width='60'", "width='120'"),
        "uncompensated object-bbox ellipse",
    );
    // An outer anisotropic transform cannot make an unsupported local field
    // representable; the importer must not silently flatten shape transforms.
    let unsupported = radial("gradientTransform='scale(2 1)'")
        .replace(RECT, &format!("<g transform='scale(.5 1)'>{RECT}</g>"));
    reject(
        &mut editor,
        &unsupported,
        "outer transform cannot rescue local ellipse",
    );
    for radius in [".01", "1", "30"] {
        let source = radial(&format!(
            "gradientUnits='userSpaceOnUse' cx='0' cy='0' r='{radius}'"
        ));
        crate::svg_import::parse(source.as_bytes())
            .expect("ordinary small origin-centered circles remain representable");
    }
    for attributes in [
        "gradientUnits='userSpaceOnUse' cx='100000' cy='54' r='.02'",
        "gradientUnits='userSpaceOnUse' cx='100000' cy='54' r='.001'",
        "gradientUnits='userSpaceOnUse' cx='100000' cy='54' r='1' fx='100000.02'",
    ] {
        crate::svg_import::parse(radial(attributes).as_bytes()).expect(
            "identical source/native fields survive large-coordinate scalar radius and focus rounding");
    }
    let invalid_shader_transform =
        radial("r='5' gradientTransform='matrix(.00000001 0 0 .0001 0 0)'")
            .replace("width='60'", "width='10000'")
            .replace("height='60'", "height='1'");
    reject(
        &mut editor,
        &invalid_shader_transform,
        "bbox compensation cannot rescue a transform discarded by the source shader",
    );
    let near = radial("gradientUnits='userSpaceOnUse' cx='0' cy='0' r='1000' fx='999'");
    let parsed =
        crate::svg_import::parse(near.as_bytes()).expect("99.9-percent boundary is representable");
    let rows = parsed.contents.rows();
    let node = rows
        .iter()
        .find_map(|(_, _, n)| n.kind.gradient().map(|_| n))
        .unwrap();
    assert!(
        (node.value_at(ContentsParam::Gradient(GradientParam::HighlightLength), 0) - 99.9).abs()
            < 1e-10
    );
}

#[test]
fn svg_radial_resource_namespace_kind_and_invalid_unused_templates_reject_atomically() {
    let mut editor = pending_redo();
    for href in [
        "",
        "#",
        "#missing",
        "#base#other",
        "#b%61se",
        "#BASE",
        " #base",
        "#base ",
        "base",
        "url(#base)",
        "https://invalid.example/g.svg#base",
        "//invalid.example/g.svg#base",
        "file:///tmp/private.svg#base",
        "data:image/svg+xml;base64,PHN2Zz4=",
        "javascript:alert(1)",
    ] {
        reject(
            &mut editor,
            &single(&format!(
                "<radialGradient id='ramp' href='{href}'>{HARD_XML}</radialGradient><radialGradient id='base'>{BW_XML}</radialGradient>"
            )),
            href,
        );
    }
    for attributes in [
        "href='#base' xlink:href='#other'",
        "xmlns:xlink='https://invalid.example/ns' xlink:href='#base'",
        "xmlns:s='http://www.w3.org/2000/svg' s:href='#base'",
        "xlink:title='ignored'",
        "style='r:50%'",
        "class='paint'",
        "x1='0'",
        "onload='alert(1)'",
    ] {
        reject(
            &mut editor,
            &single(&format!(
                "<radialGradient id='ramp' {attributes}>{BW_XML}</radialGradient><radialGradient id='base'>{BW_XML}</radialGradient><radialGradient id='other'>{BW_XML}</radialGradient>"
            )),
            attributes,
        );
    }
    for (kind, target) in [
        ("radialGradient", "linearGradient"),
        ("linearGradient", "radialGradient"),
    ] {
        reject(
            &mut editor,
            &single(&format!(
                "<{kind} id='ramp' href='#base'>{BW_XML}</{kind}><{target} id='base'>{BW_XML}</{target}>"
            )),
            "cross-kind references are unsupported",
        );
    }
    for invalid in [
        "href='#missing'",
        "href='#base'",
        "cx='NaN'",
        "r='1e309'",
        "r='-1'",
        "fr='.01'",
        "fx='2em'",
        "gradientUnits='bad'",
        "spreadMethod='repeat'",
        "color-interpolation='linearRGB'",
        "gradientTransform='scale(0)'",
        "gradientUnits='userSpaceOnUse' cx='0' cy='0' r='30' gradientTransform='scale(2 3)'",
        "gradientUnits='userSpaceOnUse' cx='0' cy='0' r='30' fx='30'",
    ] {
        let source = single(&format!(
            "<radialGradient id='ramp'>{BW_XML}</radialGradient><radialGradient id='base' {invalid}>{HARD_XML}</radialGradient>"
        ));
        reject(
            &mut editor,
            &source,
            "invalid unused definition must still validate",
        );
        let shadowed = single(&format!(
            "<radialGradient id='ramp' href='#base' cx='.5' cy='.5' r='.5' fr='0' fx='.5' fy='.5' gradientUnits='objectBoundingBox' gradientTransform='translate(0 0)'>{BW_XML}</radialGradient><radialGradient id='base' {invalid}>{HARD_XML}</radialGradient>"
        ));
        reject(
            &mut editor,
            &shadowed,
            "shadowed attributes must still validate",
        );
    }
    for definitions in [
        format!("<radialGradient id='ramp' href='#ramp'>{BW_XML}</radialGradient>"),
        format!(
            "<radialGradient id='ramp'>{BW_XML}</radialGradient><radialGradient id='a' href='#b'/><radialGradient id='b' href='#a'/>"
        ),
        format!(
            "<radialGradient id='ramp' href='#base'>{BW_XML}</radialGradient><radialGradient id='base'/>"
        ),
        format!(
            "<radialGradient id='ramp' href='#base'/><radialGradient id='base'>{BW_XML}</radialGradient><radialGradient id='base'>{BW_XML}</radialGradient>"
        ),
        format!(
            "<radialGradient id='ramp' href='#base' gradientUnits='objectBoundingBox'/><radialGradient id='base' gradientUnits='userSpaceOnUse' r='30px'>{BW_XML}</radialGradient>"
        ),
    ] {
        reject(
            &mut editor,
            &single(&definitions),
            "invalid radial inheritance graph",
        );
    }
    for source in [
        svg(&format!(
            "<radialGradient id='ramp'>{BW_XML}</radialGradient>{RECT}"
        )),
        svg(&format!(
            "<g><defs><radialGradient id='ramp'>{BW_XML}</radialGradient></defs>{RECT}</g>"
        )),
        svg(&format!(
            "<defs><radialGradient id='ramp'>{BW_XML}<animate attributeName='r' to='1'/></radialGradient></defs>{RECT}"
        )),
        format!(
            "<!DOCTYPE svg [<!ENTITY leak SYSTEM 'file:///etc/passwd'>]>{}",
            radial("")
        ),
    ] {
        reject(&mut editor, &source, "closed resource and nesting subset");
    }
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
fn chain(edges: usize, reverse: bool) -> String {
    let mut definitions = (0..edges)
        .map(|i| format!("<radialGradient id='g{i}' href='#g{}'/>", i + 1))
        .collect::<Vec<_>>();
    definitions.push(format!(
        "<radialGradient id='g{edges}'>{BW_XML}</radialGradient>"
    ));
    if reverse {
        definitions.reverse();
    }
    single(&definitions.join("")).replace("url(#ramp)", "url(#g0)")
}
fn fanout(count: usize, stop_count: usize) -> String {
    let mut definitions = format!(
        "<radialGradient id='base'>{}</radialGradient>",
        stops(stop_count)
    );
    for i in 1..count {
        definitions.push_str(&format!("<radialGradient id='g{i}' href='#base'/>"));
    }
    single(&definitions).replace("url(#ramp)", "url(#base)")
}
#[test]
fn svg_radial_chain_and_combined_definition_source_and_resolved_stop_budgets() {
    let expected = expected([54., 54., 30., 0., 0.], 60., &BW);
    for reverse in [false, true] {
        assert_eq!(
            crate::svg_import::parse(chain(16, reverse).as_bytes())
                .unwrap()
                .contents,
            expected,
            "16 edges allowed in either source order"
        );
    }
    crate::svg_import::parse(fanout(64, 2).as_bytes()).expect("64 radial definitions allowed");
    crate::svg_import::parse(fanout(8, 32).as_bytes()).expect("256 resolved stops allowed");
    // Linear/radial definitions share one global resource budget.
    let mixed = (0..64)
        .map(|i| {
            let tag = if i % 2 == 0 {
                "radialGradient"
            } else {
                "linearGradient"
            };
            format!("<{tag} id='g{i}'>{BW_XML}</{tag}>")
        })
        .collect::<String>();
    let source = single(&mixed).replace("url(#ramp)", "url(#g0)");
    crate::svg_import::parse(source.as_bytes()).expect("64 mixed definitions allowed");
    let exact_stops = (0..8)
        .map(|i| {
            let tag = if i % 2 == 0 {
                "radialGradient"
            } else {
                "linearGradient"
            };
            format!("<{tag} id='g{i}'>{}</{tag}>", stops(32))
        })
        .collect::<String>();
    crate::svg_import::parse(
        single(&exact_stops)
            .replace("url(#ramp)", "url(#g0)")
            .as_bytes(),
    )
    .expect("256 mixed source stops allowed");
    let mut editor = pending_redo();
    for reverse in [false, true] {
        reject(
            &mut editor,
            &chain(17, reverse),
            "17 radial reference edges",
        );
    }
    reject(&mut editor, &fanout(65, 2), "65 radial definitions");
    reject(
        &mut editor,
        &fanout(9, 32),
        "288 resolved stops through unused aliases",
    );
    reject(
        &mut editor,
        &source.replace(
            "</defs>",
            &format!("<linearGradient id='extra'>{BW_XML}</linearGradient></defs>"),
        ),
        "65 mixed definitions",
    );
    reject(
        &mut editor,
        &single(&format!(
            "{exact_stops}<radialGradient id='extra'>{BW_XML}</radialGradient>"
        ))
        .replace("url(#ramp)", "url(#g0)"),
        "258 mixed source stops",
    );
    reject(
        &mut editor,
        &single(&format!(
            "<radialGradient id='ramp'>{}</radialGradient>",
            stops(33)
        )),
        "33 per-gradient stops",
    );
}

/// Immutable authored inputs are separate from actual native UI saves. This
/// helper intentionally never creates, overwrites or synthesizes native-final.lep.
#[test]
#[ignore = "requires LIBREEFFECTS_SVG_RADIAL_QA absolute output directory"]
fn svg_radial_native_qa_packet_and_exact_recorded_save() {
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
    let root =
        std::env::var_os("LIBREEFFECTS_SVG_RADIAL_QA").expect("set radial native QA directory");
    let root = Path::new(&root);
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let mut expected = blank();
    immutable(
        &root.join("native-start.lfe.json"),
        expected.project().to_json().unwrap().as_bytes(),
    );
    immutable(&root.join("import.svg"), reuse_source().as_bytes());
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
    immutable(&root.join("literal.svg"), reuse_reference(false).as_bytes());
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
            "actual native save matches complete independent source",
        );
        compare_routes(
            &loaded.project,
            expected.project(),
            &reuse_reference(false),
            "recorded native radial import",
        );
        println!(
            "Verified actual native full source, frame-zero VIEW and exact five-route literal pixels at frames 0/30/60."
        );
    } else {
        println!(
            "Created immutable radial QA inputs and independent source/pixel oracles. No recorded native save exists yet."
        );
    }
}
