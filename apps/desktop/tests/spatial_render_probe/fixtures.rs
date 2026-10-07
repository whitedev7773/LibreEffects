//! Independently authored native fixtures and literal projection references.
//! References never sample production geometry, animation, or the Renderer.
use libre_effects_core::*;
use std::path::Path;

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "Independent native spatial planes".into(),
            width: 480,
            height: 270,
            fps: 30.into(),
            duration: 90,
            display_start: 0,
        })
        .unwrap();
    editor
}
fn camera(editor: &mut Editor) {
    editor
        .execute(Command::SetCamera {
            camera: Some(Camera3 {
                position: [240., 135., -500.],
                focal_distance: 500.,
                principal_point: [240., 135.],
                near_clip: 1.,
            }),
        })
        .unwrap();
}
fn rectangle(editor: &mut Editor, name: &str, color: u32) -> LayerId {
    editor
        .execute(Command::AddContent {
            content: Content::Rectangle,
            width: 80.,
            height: 50.,
            name: name.into(),
        })
        .unwrap();
    let id = editor.selected().unwrap();
    editor.execute(Command::SetColor { id, color }).unwrap();
    id
}
fn spatial(editor: &mut Editor, id: LayerId, value: [f64; 3]) {
    editor
        .execute(Command::SetThreeD { id, enabled: true })
        .unwrap();
    editor
        .execute(Command::SetSpatialPosition {
            id,
            edit: SpatialEdit::Value(value),
        })
        .unwrap();
}
fn native(editor: &Editor, output: &Path, name: &str) {
    std::fs::write(
        output.join(format!("{name}.lep")),
        project_file::encode(editor.project(), None).unwrap(),
    )
    .unwrap();
}
fn reference(output: &Path, name: &str, body: &str) {
    std::fs::write(
        output.join(format!("{name}.svg")),
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='480' height='270'>{body}</svg>"),
    )
    .unwrap();
}
pub(crate) fn generate(output: &Path) {
    std::fs::create_dir_all(output).unwrap();
    let mut legacy = scene();
    rectangle(&mut legacy, "Legacy lower", 0xee3322);
    let top = rectangle(&mut legacy, "Legacy upper", 0x2266ee);
    legacy
        .execute(Command::SetPosition {
            id: top,
            frame: 0,
            x: 280.,
            y: 135.,
        })
        .unwrap();
    native(&legacy, output, "legacy-2d");
    reference(
        output,
        "legacy-2d",
        "<rect x='200' y='110' width='80' height='50' fill='#ee3322'/><rect x='240' y='110' width='80' height='50' fill='#2266ee'/>",
    );
    // Camera's optical center is composition center. At Z=0 projection is 1:1;
    // at Z=500 it is 1/2. The earlier near layer must cover the later far layer.
    let mut depth = scene();
    camera(&mut depth);
    let near = rectangle(&mut depth, "Near first", 0xee3322);
    spatial(&mut depth, near, [250., 135., 0.]);
    let far = rectangle(&mut depth, "Far later", 0x2266ee);
    spatial(&mut depth, far, [340., 135., 500.]);
    native(&depth, output, "depth");
    let mut overlay = Editor::default();
    overlay.replace_project(depth.project().clone()).unwrap();
    overlay.execute(Command::AddNull).unwrap();
    let null = overlay.selected().unwrap();
    overlay
        .execute(Command::SetPosition {
            id: null,
            frame: 0,
            x: 20.,
            y: 20.,
        })
        .unwrap();
    native(&overlay, output, "depth-with-2d-null");
    let mut empty = scene();
    empty.execute(Command::AddNull).unwrap();
    native(&empty, output, "null-only-2d");
    let null = empty.selected().unwrap();
    empty
        .execute(Command::SetThreeD {
            id: null,
            enabled: true,
        })
        .unwrap();
    native(&empty, output, "null-only-spatial");

    reference(
        output,
        "depth",
        "<rect x='270' y='122.5' width='40' height='25' fill='#2266ee'/><rect x='210' y='110' width='80' height='50' fill='#ee3322'/>",
    );
    depth.execute(Command::ToggleLocked(near)).unwrap();
    native(&depth, output, "depth-locked");
    depth.execute(Command::ToggleLocked(near)).unwrap();
    depth
        .execute(Command::SetValue {
            id: near,
            property: Property::Opacity,
            frame: 0,
            value: 50.,
        })
        .unwrap();
    native(&depth, output, "depth-opacity");
    reference(
        output,
        "depth-opacity",
        "<rect x='270' y='122.5' width='40' height='25' fill='#2266ee'/><rect x='210' y='110' width='80' height='50' fill='#ee3322' opacity='.5'/>",
    );
    depth
        .execute(Command::SetValue {
            id: near,
            property: Property::Opacity,
            frame: 0,
            value: 100.,
        })
        .unwrap();
    depth
        .execute(Command::SetSpatialPosition {
            id: near,
            edit: SpatialEdit::Key {
                frame: 0,
                value: [250., 135., 0.],
            },
        })
        .unwrap();
    depth
        .execute(Command::SetSpatialPosition {
            id: near,
            edit: SpatialEdit::Key {
                frame: 30,
                value: [350., 135., 1000.],
            },
        })
        .unwrap();
    native(&depth, output, "animated-depth");
    // Linear XYZ midpoint is [300,135,500]. Both planes now tie in depth, so
    // the later blue layer is above the first red layer.
    reference(
        output,
        "animated-depth-15",
        "<rect x='250' y='122.5' width='40' height='25' fill='#ee3322'/><rect x='270' y='122.5' width='40' height='25' fill='#2266ee'/>",
    );
    depth.execute(Command::SetCamera { camera: None }).unwrap();
    native(&depth, output, "missing-camera");

    let mut tie = scene();
    camera(&mut tie);
    let a = rectangle(&mut tie, "First equal depth", 0xee3322);
    spatial(&mut tie, a, [250., 135., 0.]);
    let b = rectangle(&mut tie, "Later equal depth", 0x2266ee);
    spatial(&mut tie, b, [290., 135., 0.]);
    native(&tie, output, "tie");
    reference(
        output,
        "tie",
        "<rect x='210' y='110' width='80' height='50' fill='#ee3322'/><rect x='250' y='110' width='80' height='50' fill='#2266ee'/>",
    );
    tie.execute(Command::SetSpatialPosition {
        id: b,
        edit: SpatialEdit::Value([290., 135., -499.5]),
    })
    .unwrap();
    native(&tie, output, "near-plane");

    let mut mixed = scene();
    camera(&mut mixed);
    let a = rectangle(&mut mixed, "Spatial", 0xee3322);
    spatial(&mut mixed, a, [250., 135., 0.]);
    rectangle(&mut mixed, "Visible 2D", 0x2266ee);
    native(&mixed, output, "mixed");

    let mut parent = scene();
    camera(&mut parent);
    parent.execute(Command::AddNull).unwrap();
    let p = parent.selected().unwrap();
    spatial(&mut parent, p, [220., 125., 500.]);
    parent
        .execute(Command::SetValue {
            id: p,
            property: Property::ScaleX,
            frame: 0,
            value: 200.,
        })
        .unwrap();
    let child = rectangle(&mut parent, "Parented spatial plane", 0x22aa55);
    spatial(&mut parent, child, [20., 10., 0.]);
    parent
        .execute(Command::SetSpatialParent {
            id: child,
            parent: Some(p),
        })
        .unwrap();
    native(&parent, output, "parented");
    reference(
        output,
        "parented",
        "<rect x='210' y='122.5' width='80' height='25' fill='#22aa55'/>",
    );

    let mut text = scene();
    camera(&mut text);
    text.execute(Command::AddContent {
        content: Content::Text {
            text: "XYZ\nNative".into(),
            font_size: 24.,
        },
        width: 160.,
        height: 80.,
        name: "Spatial text".into(),
    })
    .unwrap();
    let id = text.selected().unwrap();
    text.execute(Command::SetColor {
        id,
        color: 0xffffff,
    })
    .unwrap();
    spatial(&mut text, id, [240., 135., 500.]);
    native(&text, output, "text");
    // Font resolution is shared with production; source, line placement,
    // projection and SVG structure are independently literal.
    let family = crate::fonts::svg_family(&TextStyle::default());
    reference(
        output,
        "text",
        &format!(
            "<g transform='matrix(.5 0 0 .5 200 115)'><text x='0' y='24' font-family='{family}' font-weight='400' font-size='24' fill='white'>XYZ</text><text x='0' y='52.8' font-family='{family}' font-weight='400' font-size='24' fill='white'>Native</text></g>"
        ),
    );
    println!(
        "Wrote independently authored native spatial fixtures and literal SVG references to {}",
        output.display()
    );
}
