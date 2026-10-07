//! Independently authored native opacity fixtures. SVGs and numeric expectations
//! are literal or analytic inputs; no production sampler or Renderer authors them.
use libre_effects_core::*;
use std::path::Path;

pub(crate) const WIDTH: u32 = 96;
pub(crate) const HEIGHT: u32 = 64;
const BLUE: &str = "<rect width='96' height='64' fill='#0000ff'/>";
const RED: &str = "<rect x='28' y='20' width='40' height='24' fill='#ff0000'/>";

#[derive(Clone, Copy)]
pub(crate) struct RenderCase {
    pub name: &'static str,
    pub frame: Frame,
    pub reference: &'static str,
}

pub(crate) const CASES: &[RenderCase] = &[
    RenderCase {
        name: "normal-low",
        frame: 15,
        reference: "blue",
    },
    RenderCase {
        name: "normal-high",
        frame: 15,
        reference: "red",
    },
    RenderCase {
        name: "normal-high",
        frame: 0,
        reference: "red-50",
    },
    RenderCase {
        name: "normal-high",
        frame: 30,
        reference: "red-50",
    },
    RenderCase {
        name: "equal-endpoints",
        frame: 15,
        reference: "red-75",
    },
    RenderCase {
        name: "signed-tiny",
        frame: 15,
        reference: "blue",
    },
    RenderCase {
        name: "mixed-modes",
        frame: 15,
        reference: "red-20",
    },
    RenderCase {
        name: "mixed-modes",
        frame: 30,
        reference: "red-80",
    },
    RenderCase {
        name: "mixed-modes",
        frame: 45,
        reference: "red-62.5",
    },
    RenderCase {
        name: "mixed-modes",
        frame: 75,
        reference: "red-47.5",
    },
    RenderCase {
        name: "identity-low",
        frame: 15,
        reference: "blue",
    },
    RenderCase {
        name: "identity-high",
        frame: 15,
        reference: "red",
    },
    RenderCase {
        name: "arithmetic-low",
        frame: 15,
        reference: "red-50",
    },
    RenderCase {
        name: "arithmetic-high",
        frame: 15,
        reference: "red-50",
    },
    RenderCase {
        name: "adjustment-low",
        frame: 15,
        reference: "red",
    },
    RenderCase {
        name: "adjustment-high",
        frame: 15,
        reference: "black",
    },
    RenderCase {
        name: "adjustment-equal",
        frame: 15,
        reference: "adjustment-75",
    },
    RenderCase {
        name: "nested-low",
        frame: 15,
        reference: "blue",
    },
    RenderCase {
        name: "nested-high",
        frame: 15,
        reference: "red",
    },
    RenderCase {
        name: "nested-rate",
        frame: 15,
        reference: "red-75",
    },
    RenderCase {
        name: "nested-expression-rate",
        frame: 15,
        reference: "red-75",
    },
    RenderCase {
        name: "nested-adjustment-rate",
        frame: 15,
        reference: "adjustment-75",
    },
];

pub(crate) fn scene(fps: u32) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "Independent native opacity qualification".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: fps.into(),
            duration: fps * 4,
            display_start: 0,
        })
        .unwrap();
    editor
}

fn rectangle(editor: &mut Editor, name: &str, color: u32, width: f64, height: f64) -> LayerId {
    editor
        .execute(Command::AddContent {
            content: Content::Rectangle,
            width,
            height,
            name: name.into(),
        })
        .unwrap();
    let id = editor.selected().unwrap();
    editor.execute(Command::SetColor { id, color }).unwrap();
    id
}

fn plates(editor: &mut Editor) -> LayerId {
    rectangle(
        editor,
        "Opaque blue reference plate",
        0x0000ff,
        WIDTH.into(),
        HEIGHT.into(),
    );
    rectangle(editor, "Authored red sample", 0xff0000, 40., 24.)
}

fn edit(editor: &mut Editor, id: LayerId, edit: OpacityEdit) {
    editor
        .execute(Command::SetOpacityTiming { id, edit })
        .unwrap();
}

fn key(editor: &mut Editor, id: LayerId, frame: Frame, value: f64) {
    edit(editor, id, OpacityEdit::Key { frame, value });
}

fn modes(
    editor: &mut Editor,
    id: LayerId,
    frame: Frame,
    incoming: OpacityInterpolation,
    outgoing: OpacityInterpolation,
) {
    edit(
        editor,
        id,
        OpacityEdit::Interpolation {
            frame,
            incoming,
            outgoing,
        },
    );
}

fn speeds(editor: &mut Editor, id: LayerId, frame: Frame, incoming: f64, outgoing: f64) {
    edit(
        editor,
        id,
        OpacityEdit::TemporalEase {
            frame,
            incoming: OpacityEase {
                speed: incoming,
                influence: 100. / 3.,
            },
            outgoing: OpacityEase {
                speed: outgoing,
                influence: 100. / 3.,
            },
        },
    );
}

/// Uniform temporal controls at 1/3 and 2/3 make the cubic parameter equal time.
/// Endpoints v,v and signed speeds s,-s give v + s*t*(1-t) over one second.
/// Thus v=50,s=600 -> 200, s=-600 -> -100, and s=100 -> 75 at t=1/2.
pub(crate) fn equal_curve(editor: &mut Editor, id: LayerId, fps: u32, value: f64, speed: f64) {
    key(editor, id, 0, value);
    key(editor, id, fps, value);
    modes(
        editor,
        id,
        0,
        OpacityInterpolation::Bezier,
        OpacityInterpolation::Bezier,
    );
    modes(
        editor,
        id,
        fps,
        OpacityInterpolation::Bezier,
        OpacityInterpolation::Bezier,
    );
    speeds(editor, id, 0, -1e-300, speed);
    speeds(editor, id, fps, -speed, -0.0);
}

fn expression(editor: &mut Editor, id: LayerId, source: &str) {
    editor
        .execute(Command::SetExpression {
            id,
            target: ExpressionTarget::Opacity,
            source: source.into(),
            enabled: true,
        })
        .unwrap();
}

fn adjustment(editor: &mut Editor) -> LayerId {
    editor.execute(Command::AddAdjustment).unwrap();
    let id = editor.selected().unwrap();
    editor
        .execute(Command::SetEffects {
            id,
            effects: Effects {
                brightness: 0.,
                ..Effects::default()
            },
        })
        .unwrap();
    id
}

fn native(editor: &Editor, output: &Path, name: &str) {
    let bytes = project_file::encode(editor.project(), None).unwrap();
    let decoded = project_file::decode(&bytes).unwrap().project;
    assert!(
        editor.project().same_document(&decoded),
        "Native roundtrip changed {name}"
    );
    assert_eq!(
        project_file::encode(&decoded, None).unwrap(),
        bytes,
        "Native bytes changed for {name}"
    );
    std::fs::write(output.join(format!("{name}.lep")), bytes).unwrap();
}

fn svg(output: &Path, name: &str, body: &str) {
    std::fs::write(
        output.join(format!("{name}.svg")),
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='96' height='64'>{body}</svg>\n"),
    )
    .unwrap();
}

fn references(output: &Path) {
    svg(output, "blue", BLUE);
    svg(output, "red", &format!("{BLUE}{RED}"));
    svg(
        output,
        "black",
        "<rect width='96' height='64' fill='#000000'/>",
    );
    for (name, opacity) in [
        ("20", ".2"),
        ("50", ".5"),
        ("75", ".75"),
        ("80", ".8"),
        ("62.5", ".625"),
        ("47.5", ".475"),
    ] {
        svg(
            output,
            &format!("red-{name}"),
            &format!("{BLUE}<g opacity='{opacity}'>{RED}</g>"),
        );
    }
    // A 75% black adjustment leaves channel 64 after the independent 8-bit
    // coverage roundoff: (255 * (255 - 191) + 127) / 255 = 64.
    svg(
        output,
        "adjustment-75",
        "<rect width='96' height='64' fill='#000040'/><rect x='28' y='20' width='40' height='24' fill='#400000'/>",
    );
}

fn nested(output: &Path, name: &str, speed: f64, use_expression: bool, use_adjustment: bool) {
    let mut editor = scene(30);
    let parent = editor.project().active_composition_id();
    editor.execute(Command::NewComposition).unwrap();
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "60fps independent source".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 60.into(),
            duration: 240,
            display_start: 0,
        })
        .unwrap();
    let child = editor.project().active_composition_id();
    let red = plates(&mut editor);
    let target = if use_adjustment {
        adjustment(&mut editor)
    } else {
        red
    };
    equal_curve(&mut editor, target, 60, 50., speed);
    if use_expression {
        expression(&mut editor, target, "value;");
    }
    editor.activate_composition(parent).unwrap();
    editor
        .execute(Command::AddCompositionLayer {
            composition: child,
            frame: 0,
        })
        .unwrap();
    native(&editor, output, name);
}

fn unsupported_hold(output: &Path) {
    // A structurally valid but unsampleable native source is retained for repair.
    // The negative oracle is explicit sampling/render/automation rejection.
    let bytes = std::fs::read(output.join("normal-high.lep")).unwrap();
    let project = project_file::decode(&bytes).unwrap().project;
    let mut wire: serde_json::Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
    let layer = wire["composition"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|layer| layer.get("opacity_timing").is_some())
        .unwrap();
    layer["opacity_timing"]["keys"]["30"]["in_interpolation"] = "Hold".into();
    let project = Project::from_json(&serde_json::to_string(&wire).unwrap()).unwrap();
    assert!(project.validate_opacity_animation().is_err());
    let bytes = project_file::encode(&project, None).unwrap();
    assert_eq!(
        project_file::encode(&project_file::decode(&bytes).unwrap().project, None).unwrap(),
        bytes
    );
    std::fs::write(output.join("unmatched-incoming-hold.lep"), bytes).unwrap();
}

pub(crate) fn generate(output: &Path) {
    std::fs::create_dir_all(output).unwrap();
    references(output);
    for (name, speed, program) in [
        ("normal-low", -600., None),
        ("normal-high", 600., None),
        ("equal-endpoints", 100., None),
        ("identity-low", -600., Some("value;")),
        ("identity-high", 600., Some("value;")),
        ("arithmetic-low", -600., Some("value + 150;")),
        ("arithmetic-high", 600., Some("value - 150;")),
        (
            "expression-error",
            100.,
            Some("throw new Error('independent opacity failure');"),
        ),
        ("expression-nonfinite", 100., Some("1 / 0;")),
    ] {
        let mut editor = scene(30);
        let id = plates(&mut editor);
        equal_curve(&mut editor, id, 30, 50., speed);
        if let Some(program) = program {
            expression(&mut editor, id, program);
        }
        native(&editor, output, name);
    }
    unsupported_hold(output);
    let mut tiny = scene(30);
    let id = plates(&mut tiny);
    equal_curve(&mut tiny, id, 30, 0., 1e-300);
    native(&tiny, output, "signed-tiny");

    let mut mixed = scene(30);
    let id = plates(&mut mixed);
    for (frame, value) in [(0, 20.), (30, 80.), (60, 40.), (90, 20.)] {
        key(&mut mixed, id, frame, value);
    }
    modes(
        &mut mixed,
        id,
        0,
        OpacityInterpolation::Bezier,
        OpacityInterpolation::Hold,
    );
    modes(
        &mut mixed,
        id,
        30,
        OpacityInterpolation::Hold,
        OpacityInterpolation::Linear,
    );
    modes(
        &mut mixed,
        id,
        60,
        OpacityInterpolation::Bezier,
        OpacityInterpolation::Bezier,
    );
    speeds(&mut mixed, id, 0, -1e-300, -0.0);
    speeds(&mut mixed, id, 60, -60., 120.);
    speeds(&mut mixed, id, 90, 9e-300, -1e-300);
    native(&mixed, output, "mixed-modes");

    for (name, speed) in [
        ("adjustment-low", -600.),
        ("adjustment-high", 600.),
        ("adjustment-equal", 100.),
    ] {
        let mut editor = scene(30);
        plates(&mut editor);
        let id = adjustment(&mut editor);
        equal_curve(&mut editor, id, 30, 50., speed);
        native(&editor, output, name);
    }
    nested(output, "nested-low", -600., false, false);
    nested(output, "nested-high", 600., false, false);
    nested(output, "nested-rate", 100., false, false);
    nested(output, "nested-expression-rate", 100., true, false);
    nested(output, "nested-adjustment-rate", 100., false, true);

    let manifest = CASES
        .iter()
        .map(|case| format!("{}\t{}\t{}\n", case.name, case.frame, case.reference))
        .collect::<String>();
    std::fs::write(
        output.join("render-cases.tsv"),
        format!("project_stem\tframe\treference_stem\n{manifest}"),
    )
    .unwrap();
    std::fs::write(output.join("rejection-cases.tsv"), "project_stem\tframe\terror_fragment\nexpression-error\t15\tindependent opacity failure\nexpression-nonfinite\t15\tfinite\nunmatched-incoming-hold\t15\tincoming hold\n").unwrap();
    println!(
        "Wrote independently authored schema73 native fixtures and literal SVG references to {}",
        output.display()
    );
}
