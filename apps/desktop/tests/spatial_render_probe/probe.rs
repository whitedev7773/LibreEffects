//! Standalone GPUI-free qualification probe. Compile this file as a binary,
//! never with `--test`: it imports the production Renderer and source helpers.
//! No original JSX or external template is loaded. Native inputs and literal
//! independent SVG references are supplied by the qualification fixture writer.
#![allow(dead_code)]

#[path = "../../src/adjustment_render.rs"]
mod adjustment_render;
#[path = "../../src/blend_render.rs"]
mod blend_render;
#[path = "../../src/effect_render.rs"]
mod effect_render;
#[path = "../../src/fonts.rs"]
mod fonts;
#[path = "../../src/image_sequence.rs"]
mod image_sequence;
#[path = "../../src/matte_render.rs"]
mod matte_render;
#[path = "../../src/path_mask_render.rs"]
mod path_mask_render;
#[path = "../../src/projected_selection.rs"]
mod projected_selection;
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
#[path = "../../src/panels/transform_gesture.rs"]
mod transform_gesture;

// Only external media decoding, media-import path rewriting, and expression
// workers are excluded. Fail immediately if a fixture unexpectedly invokes one.
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
            Err("Spatial probe excludes video decoding".into())
        }
    }
    pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            Err("Spatial probe canceled".into())
        } else {
            Ok(())
        }
    }
}
mod media_io {
    pub(crate) fn path_string(_: &std::path::Path) -> Result<String, String> {
        Err("Spatial probe excludes media import and relocation".into())
    }
}
mod automation_process {
    use libre_effects_ae_expressions as ae;
    pub(crate) fn evaluate_expressions(
        _: &ae::CompositionSnapshot,
        _: &[ae::PropertyAddress],
        _: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<ae::EvaluatedProperties, ae::EvaluationError> {
        panic!("Spatial probe must not invoke an expression worker")
    }
}
// The production text module also exposes a UI selection-target check. Its
// unused state type needs no GPUI shell for this standalone renderer probe.
mod editor {
    pub(crate) struct EditorState {
        pub text_session: Option<crate::text_edit::Session>,
        pub editor: libre_effects_core::Editor,
        pub document_revision: u64,
        pub frame: libre_effects_core::Frame,
    }
}

mod fixtures;
mod planar;

use libre_effects_core::{Project, project_file};
use std::{collections::BTreeSet, path::Path};

fn load(path: &str) -> Project {
    let bytes = std::fs::read(path).unwrap();
    project_file::decode(&bytes).unwrap().project
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
fn render(project: &Project, frame: u32, reference_path: &str, output: &str) {
    let before = source(project);
    let renderer = rendering::Renderer::new();
    let comp = project.composition();
    let svg = std::fs::read_to_string(reference_path).unwrap();
    let expected = reference(&svg, comp.width(), comp.height());
    let actual = renderer.render(project, frame, u32::MAX).unwrap();
    let preview = renderer.render_preview(project, frame, u32::MAX).unwrap();
    let export = renderer
        .render_output(project, frame, comp.width(), comp.height())
        .unwrap();
    assert_eq!(actual, preview, "Preview/export geometry diverged");
    assert_eq!(actual, export, "Explicit output geometry diverged");
    let differences = actual
        .pixels()
        .zip(expected.pixels())
        .filter(|(a, b)| a != b)
        .count();
    let painted = expected.pixels().filter(|pixel| pixel[3] != 0).count();
    assert!(painted > 0, "Independent reference was blank");
    let out = Path::new(output);
    std::fs::create_dir_all(out).unwrap();
    actual.save(out.join("actual.png")).unwrap();
    expected.save(out.join("reference.png")).unwrap();
    assert_eq!(differences, 0, "Independent literal SVG pixel mismatch");
    assert_eq!(source(project), before, "Renderer mutated source");
    println!(
        "PASS renderer/preview/export: {} pixels, zero differences, {painted} nontransparent reference pixels",
        actual.width() as u64 * actual.height() as u64
    );
}
fn reject(project: &Project, frame: u32, fragment: &str) {
    let before = source(project);
    let renderer = rendering::Renderer::new();
    for result in [
        renderer.render(project, frame, u32::MAX),
        renderer.render_preview(project, frame, u32::MAX),
    ] {
        let error = result.expect_err("Unsupported spatial state rendered successfully");
        assert!(error.contains(fragment), "Unexpected error: {error}");
        println!("PASS explicit rejection: {error}");
    }
    assert_eq!(source(project), before);
}
fn pick(project: &Project, frame: u32, point: [f64; 2], expected: Option<u64>) {
    let before = source(project);
    let hit = projected_selection::projected_layer_hit(
        project.composition(),
        frame,
        point,
        &BTreeSet::new(),
        |layer| transform_gesture::layer_bounds(layer, frame),
    )
    .unwrap();
    assert_eq!(hit, expected);
    assert_eq!(source(project), before);
    println!("PASS projected selection: {point:?} -> {hit:?}");
}
fn empty(project: &Project) {
    let before = source(project);
    let renderer = rendering::Renderer::new();
    let comp = project.composition();
    let actual = renderer.render(project, 0, u32::MAX).unwrap();
    assert!(actual.pixels().all(|pixel| pixel[3] == 0));
    assert_eq!(
        actual,
        renderer.render_preview(project, 0, u32::MAX).unwrap()
    );
    assert_eq!(
        actual,
        renderer
            .render_output(project, 0, comp.width(), comp.height())
            .unwrap()
    );
    assert_eq!(source(project), before);
    println!("PASS Null-only scene: transparent preview/export without a camera");
}

fn suite(output: &str) {
    let output = Path::new(output);
    let inputs = output.join("fixtures");
    fixtures::generate(&inputs);
    let pairs = [
        ("legacy-2d", 0, "legacy-2d"),
        ("depth", 0, "depth"),
        ("depth-with-2d-null", 0, "depth"),
        ("depth-opacity", 0, "depth-opacity"),
        ("tie", 0, "tie"),
        ("animated-depth", 15, "animated-depth-15"),
        ("parented", 0, "parented"),
        ("text", 0, "text"),
    ];
    for (name, frame, expected) in pairs {
        let project = load(inputs.join(format!("{name}.lep")).to_str().unwrap());
        println!("CASE {name} frame {frame}");
        render(
            &project,
            frame,
            inputs.join(format!("{expected}.svg")).to_str().unwrap(),
            output.join(format!("pixels-{name}")).to_str().unwrap(),
        );
    }
    for name in ["null-only-2d", "null-only-spatial"] {
        empty(&load(inputs.join(format!("{name}.lep")).to_str().unwrap()));
    }
    for (name, fragment) in [
        ("missing-camera", "camera"),
        ("near-plane", "near plane"),
        ("mixed", "Mixed visible"),
    ] {
        let project = load(inputs.join(format!("{name}.lep")).to_str().unwrap());
        reject(&project, 0, fragment);
    }
    for (name, frame, point, expected) in [
        ("depth", 0, [280., 135.], Some(1)),
        ("depth-with-2d-null", 0, [280., 135.], Some(1)),
        ("depth-with-2d-null", 0, [30., 30.], Some(3)),
        ("depth", 0, [300., 135.], Some(2)),
        ("depth", 0, [350., 135.], None),
        ("depth-locked", 0, [280., 135.], Some(2)),
        ("tie", 0, [280., 135.], Some(2)),
        ("animated-depth", 15, [280., 135.], Some(2)),
    ] {
        let project = load(inputs.join(format!("{name}.lep")).to_str().unwrap());
        pick(&project, frame, point, expected);
    }
    println!(
        "PASS spatial probe suite: 8 literal RGBA pairs (1036800 pixels), 2 Null-only transparent scenes, preview/output agreement, 3 negative scenes, 8 depth/tie/locked/Null picks"
    );
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args[1] == "planar-suite" {
        planar::suite(Path::new(&args[2]));
        return;
    }
    if args[1] == "suite" {
        suite(&args[2]);
        return;
    }
    if args[1] == "generate" {
        fixtures::generate(Path::new(&args[2]));
        return;
    }
    let project = load(&args[2]);
    let frame = args[3].parse().unwrap();
    match args[1].as_str() {
        "render" => render(&project, frame, &args[4], &args[5]),
        "reject" => reject(&project, frame, &args[4]),
        "pick" => pick(
            &project,
            frame,
            [args[4].parse().unwrap(), args[5].parse().unwrap()],
            if args[6] == "none" {
                None
            } else {
                Some(args[6].parse().unwrap())
            },
        ),
        _ => panic!(
            "Usage: probe render INPUT.lep FRAME REFERENCE.svg OUTDIR | reject INPUT.lep FRAME ERROR | pick INPUT.lep FRAME X Y ID|none"
        ),
    }
}
