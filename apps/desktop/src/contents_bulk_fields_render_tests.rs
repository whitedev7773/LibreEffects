//! Shared-value integration acceptance uses literal track replacements, never
//! ordinary Track commands to manufacture its expected result. Generated LEP
//! files exercise the codec; they are not evidence of native Save/Open.
use crate::{
    rendering::Renderer,
    view_state::{CompositionView, GraphChannel, GraphRanges, ProjectViews},
};
use libre_effects_core::*;
use serde_json::json;

const WIDTH: u32 = 400;
const HEIGHT: u32 = 240;
const FRAMES: [u32; 5] = [0, 15, 30, 45, 60];

fn contents(project: &Project) -> &ShapeContents {
    let Content::ShapeContents(contents) = project.composition().layer(1).unwrap().content() else {
        panic!("Expected Contents fixture")
    };
    contents
}
fn catalog(kinds: Vec<ContentsKind>) -> Project {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Shared numeric properties acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(ShapeContents::default()),
            width: WIDTH as f64,
            height: HEIGHT as f64,
            name: "Shared local scalars".into(),
        })
        .unwrap();
    for kind in kinds {
        editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add { parent: 0, kind },
            })
            .unwrap();
    }
    editor.project().clone()
}
fn node(project: &Project, id: u64) -> ContentsNode {
    contents(project).node(id).unwrap().clone()
}
fn tree(project: &Project, items: Vec<ContentsNode>) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    wire["composition"]["layers"][0]["content"]["ShapeContents"]["items"] =
        serde_json::to_value(items).unwrap();
    Project::from_json(&wire.to_string()).unwrap()
}
fn static_track(value: f64) -> AnimatedProperty {
    serde_json::from_value(json!({"value": value, "keys": {}})).unwrap()
}
fn linear_track(a: f64, b: f64, middle: Option<f64>) -> AnimatedProperty {
    let mut wire = json!({"value": a + 1., "keys": {
        "0": {"value": a, "interpolation": "Linear"},
        "60": {"value": b, "interpolation": "Hold"}
    }});
    if let Some(value) = middle {
        wire["keys"]["30"] = json!({"value": value, "interpolation": "Linear"});
    }
    serde_json::from_value(wire).unwrap()
}
fn eased_track(middle: f64) -> AnimatedProperty {
    serde_json::from_value(json!({"value": 53., "keys": {
        "0": {"value": 40., "interpolation": "Linear"},
        "30": {"value": middle, "interpolation": {"Bezier": {"x1": 0.2, "y1": -0.1, "x2": 0.8, "y2": 1.2}},
            "temporal": {"incoming": {"slope": 0.5, "influence": 0.3}, "outgoing": {"slope": -0.25, "influence": 0.4}}},
        "60": {"value": 80., "interpolation": "Hold"}
    }})).unwrap()
}
fn set(node: &mut ContentsNode, parameter: ContentsParam, track: AnimatedProperty) {
    assert!(node.parameters.contains_key(&parameter));
    node.parameters.insert(parameter, track);
}
fn scalar(node: &mut ContentsNode, parameter: ContentsParam, value: f64) {
    set(node, parameter, static_track(value));
}
fn group(project: &Project, id: u64, children: Vec<ContentsNode>) -> ContentsNode {
    let mut result = node(project, id);
    result.kind = ContentsKind::Group(children);
    result
}
fn transformed_tree(catalog: &Project, children: Vec<ContentsNode>) -> Project {
    let mut inner = group(catalog, 2, children);
    scalar(&mut inner, ContentsParam::Skew, 11.);
    scalar(
        &mut inner,
        ContentsParam::Transform(Property::Rotation),
        -9.,
    );
    let mut outer = group(catalog, 1, vec![inner]);
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::PositionX),
        220.,
    );
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::PositionY),
        80.,
    );
    scalar(&mut outer, ContentsParam::Transform(Property::ScaleX), -90.);
    scalar(&mut outer, ContentsParam::Transform(Property::ScaleY), 85.);
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::Rotation),
        13.,
    );
    tree(catalog, vec![outer])
}
fn geometry(project: &Project, id: u64, x: f64) -> ContentsNode {
    let mut result = node(project, id);
    scalar(
        &mut result,
        ContentsParam::Transform(Property::PositionX),
        x,
    );
    scalar(&mut result, ContentsParam::Height, 70.);
    result
}
fn command(items: &[u64], parameter: ContentsParam, value: f64) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::SetSharedValue {
            parent: 2,
            items: items.to_vec(),
            parameter,
            frame: 30,
            value,
        },
    }
}
fn export(label: &str, before: &Project, expected: &Project) {
    use std::io::Write;
    let Some(directory) = std::env::var_os("LIBREEFFECTS_EXPORT_BULK_FIELDS_FIXTURES") else {
        return;
    };
    let root = std::path::PathBuf::from(directory);
    assert!(root.is_absolute());
    std::fs::create_dir_all(&root).unwrap();
    for (suffix, project) in [("before", before), ("expected", expected)] {
        let mut views = ProjectViews::default();
        views.compositions.insert(
            project.active_composition_id(),
            CompositionView {
                frame: 30,
                ..Default::default()
            },
        );
        let view = views.encode_native(project).unwrap();
        for (extension, bytes) in [
            ("lfe.json", project.to_json().unwrap().into_bytes()),
            (
                "generated.lep",
                project_file::encode(project, Some(&view)).unwrap(),
            ),
        ] {
            let path = root.join(format!("{label}-{suffix}.{extension}"));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => file.write_all(&bytes).unwrap(),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    assert_eq!(
                        std::fs::read(&path).unwrap(),
                        bytes,
                        "Refusing to replace differing fixture {}",
                        path.display()
                    );
                }
                Err(error) => panic!("Cannot export fixture {}: {error}", path.display()),
            }
        }
    }
}
fn acceptance(
    label: &str,
    before: Project,
    expected: Project,
    items: &[u64],
    parameter: ContentsParam,
    value: f64,
) {
    assert_ne!(before, expected);
    let mut editor = Editor::default();
    editor.replace_project(before.clone()).unwrap();
    editor.clear_history();
    let mut views = ProjectViews::default();
    let mut view = CompositionView {
        frame: 30,
        ..Default::default()
    };
    for &item in items {
        let channel = GraphChannel {
            id: 1,
            property: PropertyPath::Contents { item, parameter },
        };
        view.graph_channels.pin(channel).unwrap();
        view.graph_channels.ranges.insert(
            channel,
            GraphRanges {
                value: Some([-100., 200.]),
                speed: Some([-50., 50.]),
            },
        );
    }
    views.compositions.insert(1, view);
    views.normalize(&before);
    let view_bytes = views.encode_native(&before).unwrap();
    editor.execute(command(items, parameter, value)).unwrap();
    assert_eq!(
        editor.project(),
        &expected,
        "complete literal expected project"
    );
    assert_eq!(views.encode_native(editor.project()).unwrap(), view_bytes);
    let native = project_file::encode(editor.project(), Some(&view_bytes)).unwrap();
    let decoded = project_file::decode(&native).unwrap();
    assert_eq!(decoded.project, expected);
    assert_eq!(decoded.view, Some(view_bytes.as_slice()));
    let renderer = Renderer::new();
    let mut changed_frame = false;
    for frame in FRAMES {
        let preview = renderer.render(editor.project(), frame, WIDTH).unwrap();
        let output = renderer
            .render_output(editor.project(), frame, WIDTH, HEIGHT)
            .unwrap();
        let literal = renderer
            .render_output(&expected, frame, WIDTH, HEIGHT)
            .unwrap();
        let reopened = renderer
            .render_output(&decoded.project, frame, WIDTH, HEIGHT)
            .unwrap();
        assert_eq!(preview, output, "preview/output frame {frame}");
        assert_eq!(output, literal, "literal reference frame {frame}");
        assert_eq!(output, reopened, "codec frame {frame}");
        changed_frame |= output
            != renderer
                .render_output(&before, frame, WIDTH, HEIGHT)
                .unwrap();
    }
    assert!(
        changed_frame,
        "fixture must expose the edit in rendered pixels"
    );
    editor.undo();
    assert_eq!(editor.project(), &before);
    editor.undo();
    assert_eq!(editor.project(), &before, "exactly one Undo step");
    editor.redo();
    assert_eq!(editor.project(), &expected);
    let exact = serde_json::to_vec(editor.project()).unwrap();
    editor.execute(command(items, parameter, value)).unwrap();
    assert_eq!(serde_json::to_vec(editor.project()).unwrap(), exact);
    editor.undo();
    assert_eq!(
        editor.project(),
        &before,
        "sampled no-op creates no extra history"
    );
    export(label, &before, &expected);
}

#[test]
fn shared_width_matches_literal_static_and_eased_tracks_in_reflected_nested_groups() {
    let catalog = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::Parametric(ShapeKind::Ellipse),
        ContentsKind::Parametric(ShapeKind::Star),
        ContentsKind::Fill { even_odd: false },
    ]);
    let mut rectangle = geometry(&catalog, 3, -110.);
    set(
        &mut rectangle,
        ContentsParam::Width,
        linear_track(60., 140., None),
    );
    let mut ellipse = geometry(&catalog, 4, 0.);
    scalar(&mut ellipse, ContentsParam::Width, 45.);
    let mut star = geometry(&catalog, 5, 110.);
    set(&mut star, ContentsParam::Width, eased_track(70.));
    let mut paint = node(&catalog, 6);
    scalar(&mut paint, ContentsParam::Shape(ShapeParam::FillGreen), 80.);
    let before = transformed_tree(
        &catalog,
        vec![
            rectangle.clone(),
            ellipse.clone(),
            star.clone(),
            paint.clone(),
        ],
    );
    scalar(&mut ellipse, ContentsParam::Width, 100.);
    set(&mut star, ContentsParam::Width, eased_track(100.));
    let expected = transformed_tree(&catalog, vec![rectangle, ellipse, star, paint]);
    acceptance(
        "heterogeneous-width",
        before,
        expected,
        &[5, 3, 4],
        ContentsParam::Width,
        100.,
    );
}

#[test]
fn shared_paint_opacity_matches_literal_static_and_inserted_key_with_stop_tracks_unchanged() {
    let catalog = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::Fill { even_odd: false },
        ContentsKind::GradientFill {
            even_odd: true,
            gradient: ShapeGradient::default(),
        },
    ]);
    let shape = geometry(&catalog, 3, 0.);
    let parameter = ContentsParam::Shape(ShapeParam::FillOpacity);
    let mut solid = node(&catalog, 4);
    scalar(&mut solid, parameter, 85.);
    scalar(&mut solid, ContentsParam::Shape(ShapeParam::FillRed), 170.);
    let mut gradient = node(&catalog, 5);
    set(&mut gradient, parameter, linear_track(20., 60., None));
    set(
        &mut gradient,
        ContentsParam::Gradient(GradientParam::ColorPosition(1)),
        linear_track(0., 25., None),
    );
    let before = transformed_tree(
        &catalog,
        vec![shape.clone(), solid.clone(), gradient.clone()],
    );
    scalar(&mut solid, parameter, 70.);
    set(&mut gradient, parameter, linear_track(20., 60., Some(70.)));
    let expected = transformed_tree(&catalog, vec![shape, solid, gradient]);
    acceptance(
        "mixed-paint-opacity",
        before,
        expected,
        &[4, 5],
        parameter,
        70.,
    );
}

#[test]
fn shared_gradient_endpoint_matches_literal_fill_stroke_tracks_and_preserves_stop_identity() {
    let catalog = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        ContentsKind::Parametric(ShapeKind::Ellipse),
        ContentsKind::GradientFill {
            even_odd: false,
            gradient: ShapeGradient::default(),
        },
        ContentsKind::GradientStroke {
            style: ShapeStroke::default(),
            gradient: ShapeGradient::default(),
        },
    ]);
    let shape = geometry(&catalog, 3, 0.);
    let parameter = ContentsParam::Gradient(GradientParam::EndX);
    let mut fill = node(&catalog, 4);
    scalar(&mut fill, parameter, 120.);
    let mut stroke = node(&catalog, 5);
    scalar(
        &mut stroke,
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        12.,
    );
    set(&mut stroke, parameter, linear_track(50., 130., None));
    let before = transformed_tree(&catalog, vec![shape.clone(), stroke.clone(), fill.clone()]);
    scalar(&mut fill, parameter, 40.);
    set(&mut stroke, parameter, linear_track(50., 130., Some(40.)));
    let expected = transformed_tree(&catalog, vec![shape, stroke, fill]);
    acceptance(
        "gradient-endpoint",
        before,
        expected,
        &[5, 4],
        parameter,
        40.,
    );
}
