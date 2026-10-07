//! Small camera-free joined XY renderer qualification. Every SVG coordinate is
//! literal independent arithmetic, never a production sample or geometry result.
use libre_effects_core::*;
use std::{collections::BTreeMap, path::Path};

fn configure(editor: &mut Editor, fps: u32) {
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "Independent planar rectangles".into(),
            width: 240,
            height: 150,
            fps: fps.into(),
            duration: 180,
            display_start: 0,
        })
        .unwrap();
}

fn rectangle(editor: &mut Editor, name: &str, size: [f64; 2], color: u32) -> LayerId {
    editor
        .execute(Command::AddContent {
            content: Content::Rectangle,
            width: size[0],
            height: size[1],
            name: name.into(),
        })
        .unwrap();
    let id = editor.selected().unwrap();
    editor.execute(Command::SetColor { id, color }).unwrap();
    id
}

fn linear(a: [f64; 2], b: [f64; 2]) -> SpatialPosition2 {
    SpatialPosition2 {
        value: None,
        keys: BTreeMap::from([(0, SpatialKey2::new(a)), (60, SpatialKey2::new(b))]),
    }
}

fn joined(editor: &mut Editor, id: LayerId, position: SpatialPosition2) {
    editor
        .execute(Command::SetPlanarPosition { id, position })
        .unwrap();
}

fn camera_free(project: &Project) {
    for (_, comp) in project.compositions() {
        assert!(comp.camera().is_none(), "Planar source acquired a camera");
        assert!(comp.layers().iter().all(|layer| !layer.is_three_d()));
    }
}

fn native(editor: &Editor, output: &Path, name: &str) -> Project {
    let path = output.join(format!("{name}.lep"));
    let authored = crate::source(editor.project());
    std::fs::write(&path, &authored).unwrap();
    let project = crate::load(path.to_str().unwrap());
    assert_eq!(crate::source(&project), authored, "Native source roundtrip");
    camera_free(&project);
    project
}

fn pair(project: &Project, output: &Path, name: &str, frame: u32, body: &str) {
    let reference = output.join(format!("{name}.svg"));
    std::fs::write(
        &reference,
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='240' height='150'>{body}</svg>"),
    )
    .unwrap();
    println!("CASE {name} frame {frame}");
    crate::render(
        project,
        frame,
        reference.to_str().unwrap(),
        output.join(format!("pixels-{name}")).to_str().unwrap(),
    );
    camera_free(project);
}

pub(crate) fn suite(output: &Path) {
    std::fs::create_dir_all(output).unwrap();
    let mut editor = Editor::default();
    configure(&mut editor, 30);
    let red = rectangle(&mut editor, "Joined red lower", [40., 12.], 0xee3322);
    joined(&mut editor, red, linear([80., 64.], [140., 104.]));
    let parent = rectangle(&mut editor, "Invisible joined parent", [20., 12.], 0xffff00);
    joined(&mut editor, parent, linear([60., 50.], [120., 90.]));
    for (property, value) in [(Property::Opacity, 0.), (Property::ScaleX, 200.)] {
        editor
            .execute(Command::SetValue {
                id: parent,
                property,
                frame: 0,
                value,
            })
            .unwrap();
    }
    let child = rectangle(&mut editor, "Scalar green child", [20., 12.], 0x22aa55);
    editor
        .execute(Command::SetPosition {
            id: child,
            frame: 0,
            x: 30.,
            y: 20.,
        })
        .unwrap();
    editor
        .execute(Command::SetPlanarParent {
            id: child,
            parent: Some(parent),
        })
        .unwrap();
    let blue = rectangle(&mut editor, "Later blue upper", [30., 16.], 0x2266ee);
    joined(&mut editor, blue, SpatialPosition2::new([135., 84.]));
    let project = native(&editor, output, "sampled-parent-stack");
    assert!(
        project
            .composition()
            .layer(child)
            .unwrap()
            .planar_position()
            .is_none()
    );
    assert!(
        project
            .composition()
            .layer(parent)
            .unwrap()
            .planar_position()
            .unwrap()
            .value
            .is_none()
    );

    // Red center: [80,64], [110,84], [140,104]. Parent anchor is [10,6].
    // Child center = parent XY + [2*(30-10), 20-6] = parent XY + [40,14].
    // Its 20x12 source becomes 40x12. Parent opacity is not inherited.
    // The later blue layer covers both earlier rectangles at frame 30.
    for (frame, body) in [
        (
            0,
            "<rect x='60' y='58' width='40' height='12' fill='#ee3322'/><rect x='80' y='58' width='40' height='12' fill='#22aa55'/><rect x='120' y='76' width='30' height='16' fill='#2266ee'/>",
        ),
        (
            30,
            "<rect x='90' y='78' width='40' height='12' fill='#ee3322'/><rect x='110' y='78' width='40' height='12' fill='#22aa55'/><rect x='120' y='76' width='30' height='16' fill='#2266ee'/>",
        ),
        (
            60,
            "<rect x='120' y='98' width='40' height='12' fill='#ee3322'/><rect x='140' y='98' width='40' height='12' fill='#22aa55'/><rect x='120' y='76' width='30' height='16' fill='#2266ee'/>",
        ),
    ] {
        pair(
            &project,
            output,
            &format!("sampled-parent-stack-{frame}"),
            frame,
            body,
        );
    }
    // This point belongs only to the sampled child's frame-30 rectangle. It is
    // outside the parent's rectangle and every rectangle at frames 0 and 60.
    crate::pick(&project, 30, [115., 80.], Some(child));
    crate::pick(&project, 0, [115., 80.], None);
    crate::pick(&project, 60, [115., 80.], None);
    crate::pick(&project, 30, [125., 84.], Some(blue));

    let mut nested = Editor::default();
    configure(&mut nested, 30);
    let cyan = rectangle(&mut nested, "30fps eased source", [20., 12.], 0x22bbcc);
    let mut position = linear([40., 40.], [160., 40.]);
    let first = position.keys.get_mut(&0).unwrap();
    first.out_interpolation = SpatialInterpolation::Bezier;
    first.out_ease = SpatialEase {
        speed: 30.,
        influence: 100. / 3.,
    };
    joined(&mut nested, cyan, position);
    let source_comp = nested.project().active_composition_id();
    nested.execute(Command::NewComposition).unwrap();
    configure(&mut nested, 60);
    nested
        .execute(Command::AddCompositionLayer {
            composition: source_comp,
            frame: 0,
        })
        .unwrap();
    let project = native(&nested, output, "nested-owning-fps");
    // Outer frame 60 at 60fps maps to source frame 30 at 30fps. The 2-second
    // segment has temporal distance controls [0,20,80,120], so at t=.5 distance
    // is 52.5 and source center X is 92.5. A wrong 60fps timebase gives 88.75.
    pair(
        &project,
        output,
        "nested-owning-fps-60",
        60,
        "<rect x='82.5' y='34' width='20' height='12' fill='#22bbcc'/>",
    );
    println!(
        "PASS planar probe suite: 4 literal RGBA pairs (144000 pixels), sampled XY at 0/30/60, scalar child of zero-opacity planar parent, ordinary 2D stack, 4 projected picks, nested owning FPS, unchanged camera-free source, preview/output agreement"
    );
}
