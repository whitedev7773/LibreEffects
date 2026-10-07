//! Standalone GPUI-free production Renderer qualification binary. Never compile
//! as `--test` and never rename to main.rs (Cargo discovers test directories).
//! Every expression runs through the production child-process dispatcher.
#![allow(dead_code)]

#[path = "../../src/adjustment_render.rs"]
mod adjustment_render;
#[path = "../../src/automation_process.rs"]
mod automation_process;
#[path = "../../src/blend_render.rs"]
mod blend_render;
#[path = "../../src/effect_render.rs"]
mod effect_render;
#[path = "../../src/fonts.rs"]
mod fonts;
#[path = "../../src/graph_channels.rs"]
mod graph_channels;
#[path = "../../src/image_sequence.rs"]
mod image_sequence;
#[path = "../../src/matte_render.rs"]
mod matte_render;
#[path = "../../src/path_mask_render.rs"]
mod path_mask_render;
#[path = "../../src/rendering.rs"]
mod rendering;
#[path = "../../src/rich_text_render.rs"]
mod rich_text_render;
#[path = "../../src/source_render.rs"]
mod source_render;
#[path = "../../src/text_animator.rs"]
mod text_animator;
#[path = "../../src/text_animator_render.rs"]
mod text_animator_render;
#[path = "../../src/text_edit.rs"]
mod text_edit;
#[path = "../../src/text_flow.rs"]
mod text_flow;

// These fixtures have no external media. Reject, rather than invent decoded
// content, if a future fixture inadvertently enters one of these seams.
mod video_decoder {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    #[derive(Default)]
    pub(crate) struct Pool;
    impl Pool {
        pub fn clear(&mut self) {}
        pub fn frame_png(
            &mut self,
            _: &str,
            _: f64,
            _: f64,
            _: u32,
            _: u32,
            _: u32,
            _: &AtomicBool,
        ) -> Result<Arc<str>, String> {
            Err("Opacity probe excludes external video decoding".into())
        }
    }
    pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            Err("Opacity probe canceled".into())
        } else {
            Ok(())
        }
    }
}
mod media_io {
    pub(crate) fn path_string(_: &std::path::Path) -> Result<String, String> {
        Err("Opacity probe excludes external media import and relocation".into())
    }
}

mod fixtures;

use libre_effects_core::{expression_runtime as ae, *};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn load(path: &Path) -> Project {
    project_file::decode(&std::fs::read(path).unwrap())
        .unwrap()
        .project
}

fn source(project: &Project) -> Vec<u8> {
    project_file::encode(project, None).unwrap()
}

fn reference(svg: &str, width: u32, height: u32) -> image::RgbaImage {
    let tree = resvg::usvg::Tree::from_str(svg, &fonts::render_options()).unwrap();
    assert_eq!(tree.size().width(), width as f32);
    assert_eq!(tree.size().height(), height as f32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let pixels = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let pixel = pixel.demultiply();
            [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
        })
        .collect();
    image::RgbaImage::from_raw(width, height, pixels).unwrap()
}

fn render(project: &Project, frame: Frame, reference_path: &Path, output: &Path) {
    let before = source(project);
    let renderer = rendering::Renderer::new();
    let comp = project.composition();
    let expected = reference(
        &std::fs::read_to_string(reference_path).unwrap(),
        comp.width(),
        comp.height(),
    );
    let actual = renderer.render(project, frame, u32::MAX).unwrap();
    assert_eq!(
        actual,
        renderer.render_preview(project, frame, u32::MAX).unwrap(),
        "Preview differs from production render"
    );
    assert_eq!(
        actual,
        renderer
            .render_output(project, frame, comp.width(), comp.height())
            .unwrap(),
        "Explicit output differs from production render"
    );
    std::fs::create_dir_all(output).unwrap();
    actual.save(output.join("actual.png")).unwrap();
    expected.save(output.join("reference.png")).unwrap();
    let different = actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        expected.pixels().any(|pixel| pixel[3] != 0),
        "Independent reference is blank"
    );
    assert_eq!(
        different,
        0,
        "Literal SVG mismatch at frame {frame}; see {}",
        output.display()
    );
    assert_eq!(
        source(project),
        before,
        "Rendering changed authored native bytes"
    );
    assert!(
        !automation_process::worker_active(),
        "Expression worker permit leaked"
    );
    println!(
        "PASS render/preview/output frame {frame}: {} pixels, zero literal SVG differences",
        actual.width() * actual.height()
    );
}

fn reject_render(project: &Project, frame: Frame, fragment: &str) {
    let before = source(project);
    let renderer = rendering::Renderer::new();
    let comp = project.composition();
    for result in [
        renderer.render(project, frame, u32::MAX),
        renderer.render_preview(project, frame, u32::MAX),
        renderer.render_output(project, frame, comp.width(), comp.height()),
    ] {
        let error = result.expect_err("Unsupported source silently rendered authored opacity");
        assert!(
            error.contains(fragment),
            "Unexpected explicit failure: {error}"
        );
        println!("PASS explicit render rejection: {error}");
    }
    assert_eq!(source(project), before);
    assert!(!automation_process::worker_active());
}

fn close(actual: f64, expected: f64) {
    let tolerance = if expected != 0. && expected.abs() < 1e-200 {
        expected.abs() * 1e-11
    } else {
        1e-8
    };
    assert!(
        (actual - expected).abs() <= tolerance,
        "Expected {expected:.17e}, got {actual:.17e}"
    );
}

fn annotated(project: &Project) -> &Layer {
    project
        .composition()
        .layers()
        .iter()
        .find(|layer| layer.has_opacity_timing())
        .unwrap()
}

fn helpers(inputs: &Path) {
    for (name, center, speed) in [
        ("normal-low", 50., -600.),
        ("normal-high", 50., 600.),
        ("equal-endpoints", 50., 100.),
        ("signed-tiny", 0., 1e-300),
    ] {
        let project = load(&inputs.join(format!("{name}.lep")));
        let before = source(&project);
        let layer = annotated(&project);
        assert_eq!(layer.opacity_key_count(), 2);
        assert!(layer.property(Property::Opacity).is_none());
        assert!(layer.track(Property::Opacity.into()).is_none());
        assert!(
            !graph_channels::GraphChannel {
                id: layer.id(),
                property: Property::Opacity.into()
            }
            .available(project.composition())
        );
        for frame in [0, 1, 7, 15, 23, 29, 30, 90] {
            let t = (f64::from(frame) / 30.).min(1.);
            // Independently reduced polynomial, with fixed authored controls.
            close(
                layer.opacity_at(frame, 1. / 30.).unwrap(),
                center + speed * t * (1. - t),
            );
        }
        for seconds in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(
                layer.opacity_at(15, seconds).is_err(),
                "Invalid seconds/frame accepted: {seconds}"
            );
        }
        assert_eq!(source(&project), before);
    }
    let mixed = load(&inputs.join("mixed-modes.lep"));
    let layer = annotated(&mixed);
    assert_eq!(layer.opacity_key_count(), 4);
    // Midpoint controls for [80,40] are [80,200/3,60,40]: 62.5.
    // Midpoint controls for [40,20] are [40,80,80/3,20]: 47.5.
    for (frame, expected) in [
        (0, 20.),
        (15, 20.),
        (29, 20.),
        (30, 80.),
        (45, 62.5),
        (60, 40.),
        (75, 47.5),
        (90, 20.),
    ] {
        close(layer.opacity_at(frame, 1. / 30.).unwrap(), expected);
    }
    let raw: serde_json::Value = serde_json::from_str(&mixed.to_json().unwrap()).unwrap();
    let wire = raw["composition"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["id"] == layer.id())
        .unwrap();
    let timing = &wire["opacity_timing"]["keys"];
    assert_eq!(timing["0"]["in_ease"]["speed"].as_f64().unwrap(), -1e-300);
    assert_eq!(
        timing["0"]["out_ease"]["speed"].as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(timing["90"]["out_ease"]["speed"].as_f64().unwrap(), -1e-300);
    assert_eq!(timing["90"]["in_ease"]["speed"].as_f64().unwrap(), 9e-300);
    assert_eq!(timing["30"]["in_interpolation"], "Hold");
    println!(
        "PASS analytic raw opacity samples, exact endpoints, signed tiny/end metadata, mixed modes, read-only graph admission"
    );
}

fn expressions(inputs: &Path) {
    for (name, authored, expected) in [
        ("identity-low", -100., -100.),
        ("identity-high", 200., 200.),
        ("arithmetic-low", -100., 50.),
        ("arithmetic-high", 200., 50.),
    ] {
        let project = load(&inputs.join(format!("{name}.lep")));
        let before = source(&project);
        let composition = project.active_composition_id();
        let layer = annotated(&project);
        let snapshot = project.expression_snapshot(composition, 15).unwrap();
        let property = &snapshot
            .layers
            .iter()
            .find(|candidate| candidate.id.0 == layer.id())
            .unwrap()
            .opacity;
        let ae::PropertyValue::Scalar(value) = &property.authored_value else {
            panic!("Expected scalar snapshot")
        };
        close(*value, authored);
        let roots = project.expression_roots(composition, 15, false).unwrap();
        let evaluated = automation_process::evaluate_expressions(
            &snapshot,
            &roots,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(
            evaluated.expression_evaluations, 1,
            "Identity expression did not execute"
        );
        let address = ae::PropertyAddress {
            composition: ae::CompositionId(composition),
            layer: ae::LayerId(layer.id()),
            property: ae::ExpressionProperty::Opacity,
        };
        let ae::PropertyValue::Scalar(value) = evaluated.get(&address).unwrap() else {
            panic!("Expected scalar result")
        };
        close(*value, expected);
        let view = project
            .with_evaluated_properties(composition, 15, false, &evaluated)
            .unwrap();
        close(
            view.composition()
                .layer(layer.id())
                .unwrap()
                .opacity_at(15, 1. / 30.)
                .unwrap(),
            expected,
        );
        assert!(
            project_file::encode(&view, None).is_err(),
            "Detached render view saved as source"
        );
        assert_eq!(
            source(&project),
            before,
            "Expression changed authored metadata or values"
        );
        assert!(!automation_process::worker_active());
    }
    println!(
        "PASS real expression child IPC: raw snapshot values, identity/arithmetic results, detached raw values, native-byte preservation"
    );
}

fn text_graph() {
    let mut editor = fixtures::scene(30);
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "Native".into(),
                font_size: 20.,
            },
            width: 80.,
            height: 30.,
            name: "Independent text lanes".into(),
        })
        .unwrap();
    let id = editor.selected().unwrap();
    for property in [
        PropertyPath::Text(TextParam::FillOpacity),
        PropertyPath::Text(TextParam::StrokeOpacity),
    ] {
        assert!(
            !graph_channels::GraphChannel { id, property }
                .available(editor.project().composition())
        );
    }
    // Graph admission concerns authored channels. Default text paint values are
    // lazy and have no scalar track until an actual edit materializes one.
    for (parameter, value) in [
        (TextParam::FillOpacity, 75.),
        (TextParam::StrokeOpacity, 50.),
    ] {
        editor
            .execute(Command::EditText {
                id,
                parameter,
                edit: TrackEdit::Value { frame: 0, value },
            })
            .unwrap();
    }
    fixtures::equal_curve(&mut editor, id, 30, 50., 100.);
    for property in [
        PropertyPath::Text(TextParam::FillOpacity),
        PropertyPath::Text(TextParam::StrokeOpacity),
    ] {
        assert!(
            graph_channels::GraphChannel { id, property }.available(editor.project().composition())
        );
    }
    assert!(
        !graph_channels::GraphChannel {
            id,
            property: Property::Opacity.into()
        }
        .available(editor.project().composition())
    );
    println!(
        "PASS transform opacity guard leaves independent text fill/stroke opacity graph lanes available"
    );
}

fn rejection_contract(inputs: &Path) {
    let project = load(&inputs.join("normal-high.lep"));
    let id = annotated(&project).id();
    for edit in [
        OpacityEdit::TemporalContinuous {
            frame: 0,
            value: true,
        },
        OpacityEdit::TemporalAutoBezier {
            frame: 0,
            value: true,
        },
        OpacityEdit::TemporalEase {
            frame: 0,
            incoming: OpacityEase {
                speed: 0.,
                influence: 0.,
            },
            outgoing: OpacityEase {
                speed: 0.,
                influence: 33.,
            },
        },
        OpacityEdit::Value(25.),
    ] {
        let mut editor = Editor::default();
        editor.replace_project(project.clone()).unwrap();
        let before = source(editor.project());
        assert!(
            editor
                .execute(Command::SetOpacityTiming { id, edit })
                .is_err(),
            "Unsupported edit was silently normalized"
        );
        assert_eq!(
            source(editor.project()),
            before,
            "Rejected edit partially changed source"
        );
    }
    let original: serde_json::Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
    for mutation in 0..3 {
        let mut wire = original.clone();
        let layer = wire["composition"]["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|layer| layer["id"] == id)
            .unwrap();
        match mutation {
            0 => {
                layer["opacity_timing"]["keys"]
                    .as_object_mut()
                    .unwrap()
                    .remove("30");
            }
            1 => {
                layer["opacity_timing"]["keys"]["99"] =
                    layer["opacity_timing"]["keys"]["0"].clone();
            }
            _ => {
                layer["opacity_timing"]["keys"]["0"]["temporal_auto_bezier"] = true.into();
            }
        }
        assert!(
            Project::from_json(&serde_json::to_string(&wire).unwrap()).is_err(),
            "Malformed annotation {mutation} accepted"
        );
    }
    let unsupported = load(&inputs.join("unmatched-incoming-hold.lep"));
    let before = source(&unsupported);
    assert!(
        unsupported
            .validate_opacity_animation()
            .unwrap_err()
            .contains("incoming hold")
    );
    assert!(
        annotated(&unsupported)
            .opacity_at(15, 1. / 30.)
            .unwrap_err()
            .contains("incoming hold")
    );
    let host =
        libre_effects_editor_model::automation_host::AutomationHost::new(&unsupported, &[id], 15)
            .unwrap();
    assert!(host.finish().unwrap_err().contains("incoming hold"));
    let mut editor = Editor::default();
    editor.replace_project(project.clone()).unwrap();
    let editor_before = source(editor.project());
    assert!(
        editor
            .commit_automation_project(unsupported.clone())
            .unwrap_err()
            .contains("incoming hold")
    );
    assert_eq!(source(editor.project()), editor_before);
    assert_eq!(source(&unsupported), before);
    println!(
        "PASS explicit invalid time/mode/ease/value/coverage rejection, without partial writes"
    );
}

fn switch_preserves_metadata(inputs: &Path) {
    let mut editor = Editor::default();
    editor
        .replace_project(load(&inputs.join("normal-high.lep")))
        .unwrap();
    let id = annotated(editor.project()).id();
    let before = source(editor.project());
    for (frame, mode) in [
        (0, OpacityInterpolation::Linear),
        (30, OpacityInterpolation::Linear),
        (0, OpacityInterpolation::Bezier),
        (30, OpacityInterpolation::Bezier),
    ] {
        editor
            .execute(Command::SetOpacityTiming {
                id,
                edit: OpacityEdit::Interpolation {
                    frame,
                    incoming: mode,
                    outgoing: mode,
                },
            })
            .unwrap();
    }
    assert_eq!(
        source(editor.project()),
        before,
        "Mode switches changed signed ease or dormant endpoint metadata"
    );
    println!(
        "PASS native bytes after Bezier/Linear/Bezier mode switches, including dormant endpoint sides"
    );
}

fn suite(output: &Path) {
    let inputs = output.join("fixtures");
    fixtures::generate(&inputs);
    helpers(&inputs);
    expressions(&inputs);
    text_graph();
    rejection_contract(&inputs);
    switch_preserves_metadata(&inputs);
    for case in fixtures::CASES {
        println!("CASE {} frame {}", case.name, case.frame);
        let project = load(&inputs.join(format!("{}.lep", case.name)));
        render(
            &project,
            case.frame,
            &inputs.join(format!("{}.svg", case.reference)),
            &output.join(format!("pixels-{}-{}", case.name, case.frame)),
        );
    }
    reject_render(
        &load(&inputs.join("expression-error.lep")),
        15,
        "independent opacity failure",
    );
    reject_render(
        &load(&inputs.join("expression-nonfinite.lep")),
        15,
        "finite",
    );
    reject_render(
        &load(&inputs.join("unmatched-incoming-hold.lep")),
        15,
        "incoming hold",
    );
    println!(
        "PASS opacity73 helper qualification: {} independent RGBA pairs ({} pixels), native persistence, analytic sampling, real expression IPC, graph guards and explicit errors",
        fixtures::CASES.len(),
        fixtures::CASES.len() * (fixtures::WIDTH * fixtures::HEIGHT) as usize
    );
}

fn main() {
    if let Some(result) = automation_process::dispatch_worker() {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("suite") if args.len() == 3 => suite(Path::new(&args[2])),
        Some("generate") if args.len() == 3 => fixtures::generate(Path::new(&args[2])),
        Some("render") if args.len() == 6 => render(
            &load(Path::new(&args[2])),
            args[3].parse().unwrap(),
            Path::new(&args[4]),
            Path::new(&args[5]),
        ),
        Some("reject") if args.len() == 5 => reject_render(
            &load(Path::new(&args[2])),
            args[3].parse().unwrap(),
            &args[4],
        ),
        _ => {
            eprintln!(
                "Usage: opacity-probe suite OUTDIR | generate OUTDIR | render INPUT.lep FRAME REFERENCE.svg OUTDIR | reject INPUT.lep FRAME ERROR"
            );
            std::process::exit(2);
        }
    }
}
