//! Independent compound Gradient Colors multi-key acceptance. Literal source
//! snapshots and a legacy-static render route form the oracle. No command output
//! or production compound sampler supplies expected source or expected pixels.
//! Generated files are immutable inputs/references, never native UI evidence.
use crate::{
    editor::{Action, EditorState, Tool},
    rendering::Renderer,
    view_state::{GraphChannel, GraphRanges, ProjectViews},
};
use libre_effects_core::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 240;
const FRAMES: [Frame; 13] = [0, 7, 15, 22, 30, 44, 45, 52, 60, 67, 75, 90, 119];
const FIRST: Frame = 0;
const PAINT: u64 = 2;
thread_local! {
    static PIXEL_PAIRS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
struct PixelEvidence(&'static str);
impl PixelEvidence {
    fn new(label: &'static str) -> Self {
        PIXEL_PAIRS.with(|n| n.set(0));
        Self(label)
    }
}
impl Drop for PixelEvidence {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let pairs = PIXEL_PAIRS.with(std::cell::Cell::get);
            println!(
                "GRADIENT_MULTIKEY_PIXEL_EVIDENCE {}: {pairs} exact RGBA pairs / {} pixels",
                self.0,
                pairs * WIDTH as usize * HEIGHT as usize
            );
        }
    }
}
fn colors(value: Value) -> GradientColors {
    serde_json::from_value(value).unwrap()
}
fn first() -> GradientColors {
    colors(json!({
        "colors": [
            {"id":1,"position":-0.0,"midpoint":1.,"red":240.,"green":16.,"blue":32.},
            {"id":5,"position":20.,"midpoint":99.,"red":32.,"green":224.,"blue":64.},
            {"id":2,"position":100.,"midpoint":25.,"red":16.,"green":64.,"blue":240.}
        ],
        "opacities": [
            {"id":3,"position":0.,"midpoint":99.,"opacity":100.},
            {"id":6,"position":40.,"midpoint":1.,"opacity":20.},
            {"id":4,"position":100.,"midpoint":25.,"opacity":60.}
        ]
    }))
}
fn second() -> GradientColors {
    colors(json!({
        "colors": [
            {"id":1,"position":100.,"midpoint":99.,"red":16.,"green":224.,"blue":192.},
            {"id":5,"position":80.,"midpoint":1.,"red":224.,"green":32.,"blue":240.},
            {"id":2,"position":0.,"midpoint":75.,"red":240.,"green":192.,"blue":16.}
        ],
        "opacities": [
            {"id":3,"position":100.,"midpoint":1.,"opacity":20.},
            {"id":6,"position":60.,"midpoint":99.,"opacity":100.},
            {"id":4,"position":0.,"midpoint":75.,"opacity":80.}
        ]
    }))
}
fn third() -> GradientColors {
    colors(json!({
        "colors": [
            {"id":1,"position":10.,"midpoint":30.,"red":240.,"green":160.,"blue":16.},
            {"id":5,"position":60.,"midpoint":70.,"red":32.,"green":176.,"blue":240.},
            {"id":2,"position":90.,"midpoint":50.,"red":160.,"green":32.,"blue":208.}
        ],
        "opacities": [
            {"id":3,"position":20.,"midpoint":35.,"opacity":100.},
            {"id":6,"position":80.,"midpoint":60.,"opacity":60.},
            {"id":4,"position":100.,"midpoint":75.,"opacity":40.}
        ]
    }))
}
fn base() -> GradientColors {
    // Deliberately different dormant base. Interpolation/metadata commands must
    // not rewrite it from either endpoint or an arbitrary sampled frame.
    let mut c = first();
    c.colors[1].position = 37.;
    c.colors[1].red = 99.;
    c.opacities[1].opacity = 73.;
    c
}
/// Independent explicit field arithmetic. No production sampling/interpolation
/// or commands are used. Dyadic checkpoints also have literal scalar assertions.
fn expected_sample(
    a: &GradientColors,
    b: &GradientColors,
    mode: GradientColorsInterpolation,
    frame: Frame,
    start: Frame,
    end: Frame,
) -> GradientColors {
    if frame <= start || mode == GradientColorsInterpolation::Hold && frame < end {
        return a.clone();
    }
    if frame >= end {
        return b.clone();
    }
    let t = f64::from(frame - start) / f64::from(end - start);
    let t = if mode == GradientColorsInterpolation::Smooth {
        t * t * (3. - 2. * t)
    } else {
        t
    };
    let mix = |left: f64, right: f64| {
        if left.to_bits() == right.to_bits() {
            left
        } else {
            left + (right - left) * t
        }
    };
    let mut out = a.clone();
    for ((out, a), b) in out.colors.iter_mut().zip(&a.colors).zip(&b.colors) {
        out.position = mix(a.position, b.position);
        out.midpoint = mix(a.midpoint, b.midpoint);
        out.red = mix(a.red, b.red);
        out.green = mix(a.green, b.green);
        out.blue = mix(a.blue, b.blue);
    }
    for ((out, a), b) in out.opacities.iter_mut().zip(&a.opacities).zip(&b.opacities) {
        out.position = mix(a.position, b.position);
        out.midpoint = mix(a.midpoint, b.midpoint);
        out.opacity = mix(a.opacity, b.opacity);
    }
    out
}
fn contents(project: &Project) -> &ShapeContents {
    let Content::ShapeContents(c) = project.composition().layer(1).unwrap().content() else {
        panic!("Contents fixture")
    };
    c
}
fn paint(project: &Project) -> &ContentsNode {
    contents(project).node(PAINT).unwrap()
}
fn animation(project: &Project) -> &GradientColorsAnimation {
    paint(project)
        .kind
        .gradient()
        .unwrap()
        .colors_animation()
        .unwrap()
}
fn sample(project: &Project, frame: Frame) -> GradientColors {
    let n = paint(project);
    n.kind.gradient().unwrap().colors_at(n, frame)
}
fn paint_wire(project: &mut Value) -> &mut Value {
    &mut project["composition"]["layers"][0]["content"]["ShapeContents"]["items"][1]
}
fn gradient_wire(node: &mut Value) -> &mut Value {
    let name = if node["kind"].get("GradientFill").is_some() {
        "GradientFill"
    } else {
        "GradientStroke"
    };
    &mut node["kind"][name]["gradient"]
}
fn scalar(n: &mut ContentsNode, p: ContentsParam, value: f64) {
    n.parameters.insert(
        p,
        serde_json::from_value(json!({"value":value,"keys":{}})).unwrap(),
    );
}
/// Literal legacy static materialization bypasses the compound render path.
fn materialized_node(node: &ContentsNode, c: &GradientColors) -> ContentsNode {
    let mut wire = serde_json::to_value(node).unwrap();
    let gradient = gradient_wire(&mut wire);
    gradient["colors"] = json!(c.colors.iter().map(|s| s.id).collect::<Vec<_>>());
    gradient["opacities"] = json!(c.opacities.iter().map(|s| s.id).collect::<Vec<_>>());
    gradient.as_object_mut().unwrap().remove("colors_animation");
    let mut node: ContentsNode = serde_json::from_value(wire).unwrap();
    node.parameters
        .retain(|p, _| !matches!(p, ContentsParam::Gradient(p) if p.stop().is_some()));
    for s in &c.colors {
        for (p, v) in [
            (GradientParam::ColorPosition(s.id), s.position),
            (GradientParam::ColorMidpoint(s.id), s.midpoint),
            (GradientParam::Red(s.id), s.red),
            (GradientParam::Green(s.id), s.green),
            (GradientParam::Blue(s.id), s.blue),
        ] {
            scalar(&mut node, ContentsParam::Gradient(p), v);
        }
    }
    for s in &c.opacities {
        for (p, v) in [
            (GradientParam::OpacityPosition(s.id), s.position),
            (GradientParam::OpacityMidpoint(s.id), s.midpoint),
            (GradientParam::Opacity(s.id), s.opacity),
        ] {
            scalar(&mut node, ContentsParam::Gradient(p), v);
        }
    }
    node
}
fn materialized(project: &Project, c: &GradientColors) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    *paint_wire(&mut wire) = serde_json::to_value(materialized_node(paint(project), c)).unwrap();
    Project::from_json(&wire.to_string()).unwrap()
}
fn scene(stroke: bool, radial: bool) -> Project {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Gradient multikey acceptance".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
        duration: 120,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::ShapeContents(Default::default()),
        width: WIDTH as f64,
        height: HEIGHT as f64,
        name: "Gradient Colors multikey".into(),
    })
    .unwrap();
    let gradient: ShapeGradient = serde_json::from_value(
        json!({"radial":radial,"colors":[1,2],"opacities":[3,4],"next_stop":9}),
    )
    .unwrap();
    for kind in [
        ContentsKind::Parametric(ShapeKind::Rectangle),
        if stroke {
            ContentsKind::GradientStroke {
                style: Default::default(),
                gradient,
            }
        } else {
            ContentsKind::GradientFill {
                even_odd: false,
                gradient,
            }
        },
    ] {
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add { parent: 0, kind },
        })
        .unwrap();
    }
    let mut wire = serde_json::to_value(e.project()).unwrap();
    // No new feature is materialized: preserve an explicit pre-compound schema
    // so the prior frozen release can independently render this legacy input.
    wire["version"] = json!(53);
    let items = &mut wire["composition"]["layers"][0]["content"]["ShapeContents"]["items"];
    let mut shape: ContentsNode = serde_json::from_value(items[0].clone()).unwrap();
    shape.name = "Gradient rectangle".into();
    scalar(&mut shape, ContentsParam::Width, 300.);
    scalar(&mut shape, ContentsParam::Height, 160.);
    items[0] = serde_json::to_value(shape).unwrap();
    let mut p = materialized_node(paint(e.project()), &base());
    p.name = if stroke {
        "Gradient Stroke"
    } else {
        "Gradient Fill"
    }
    .into();
    for (parameter, value) in [
        (GradientParam::StartX, -145.),
        (GradientParam::StartY, -55.),
        (GradientParam::EndX, 145.),
        (GradientParam::EndY, 65.),
        (GradientParam::HighlightLength, 35.),
        (GradientParam::HighlightAngle, 25.),
    ] {
        scalar(&mut p, ContentsParam::Gradient(parameter), value);
    }
    scalar(
        &mut p,
        ContentsParam::Shape(if stroke {
            ShapeParam::StrokeOpacity
        } else {
            ShapeParam::FillOpacity
        }),
        85.,
    );
    if stroke {
        scalar(&mut p, ContentsParam::Shape(ShapeParam::StrokeWidth), 20.);
    }
    items[1] = serde_json::to_value(p).unwrap();
    Project::from_json(&wire.to_string()).unwrap()
}
fn keyed(
    project: &Project,
    keys: &[(Frame, GradientColors)],
    modes: &[(Frame, GradientColorsInterpolation)],
    version: u32,
) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    wire["version"] = json!(version);
    let mut animation = json!({"keys": keys.iter().cloned().collect::<BTreeMap<_,_>>()});
    if !modes.is_empty() {
        animation["outgoing_interpolation"] =
            json!(modes.iter().copied().collect::<BTreeMap<_, _>>());
    }
    gradient_wire(paint_wire(&mut wire))["colors_animation"] = animation;
    Project::from_json(&wire.to_string()).unwrap()
}
fn fixture(stroke: bool, radial: bool) -> Project {
    keyed(
        &scene(stroke, radial),
        &[(0, first()), (15, second()), (45, third())],
        &[
            (15, GradientColorsInterpolation::Linear),
            (45, GradientColorsInterpolation::Smooth),
        ],
        57,
    )
}
fn reference_sample(project: &Project, frame: Frame) -> GradientColors {
    let Some(animation) = paint(project).kind.gradient().unwrap().colors_animation() else {
        // Static expected documents are materialized independently by the caller.
        panic!("reference sample requires literal keyed expected fixture")
    };
    let keys = animation.keys();
    let (&start, a) = keys
        .range(..=frame)
        .next_back()
        .or_else(|| keys.first_key_value())
        .unwrap();
    let Some((&end, b)) = keys
        .range((std::ops::Bound::Excluded(start), std::ops::Bound::Unbounded))
        .next()
    else {
        return a.clone();
    };
    let raw = serde_json::to_value(animation).unwrap();
    let mode = match raw["outgoing_interpolation"][start.to_string()].as_str() {
        Some("Linear") => GradientColorsInterpolation::Linear,
        Some("Smooth") => GradientColorsInterpolation::Smooth,
        _ => GradientColorsInterpolation::Hold,
    };
    let topology = a
        .colors
        .iter()
        .map(|x| x.id)
        .eq(b.colors.iter().map(|x| x.id))
        && a.opacities
            .iter()
            .map(|x| x.id)
            .eq(b.opacities.iter().map(|x| x.id));
    expected_sample(
        a,
        b,
        if topology {
            mode
        } else {
            GradientColorsInterpolation::Hold
        },
        frame,
        start,
        end,
    )
}
fn assert_render(project: &Project) {
    for frame in FRAMES {
        let expected = reference_sample(project, frame);
        assert_eq!(
            sample(project, frame),
            expected,
            "all fields and local identities at {frame}"
        );
        compare_paths(project, frame, &materialized(project, &expected));
    }
}
fn exact(actual: &Project, expected: &Project, why: &str) {
    assert_eq!(actual, expected, "{why}");
    assert_eq!(
        actual.to_json().unwrap(),
        expected.to_json().unwrap(),
        "{why}: complete serialized source"
    );
}
fn compare_paths(project: &Project, frame: Frame, expected: &Project) {
    let renderer = Renderer::new();
    let source = project.to_json().unwrap();
    let json = Project::from_json(&source).unwrap();
    let native = project_file::encode(project, None).unwrap();
    let lep = project_file::decode(&native).unwrap().project;
    exact(&json, project, "JSON roundtrip");
    exact(&lep, project, "LEP roundtrip");
    let reference = renderer
        .render_output(expected, frame, WIDTH, HEIGHT)
        .unwrap();
    for (route, output) in [
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
        assert_eq!(output.dimensions(), reference.dimensions());
        let differences: Vec<_> = output
            .enumerate_pixels()
            .filter_map(|(x, y, p)| {
                let expected = reference.get_pixel(x, y);
                (p != expected).then_some((x, y, p.0, expected.0))
            })
            .take(8)
            .collect();
        assert!(
            differences.is_empty(),
            "exact unmasked RGBA {route} frame {frame}: first differences {differences:?}"
        );
        PIXEL_PAIRS.with(|n| n.set(n.get() + 1));
    }
    assert_eq!(
        source,
        project.to_json().unwrap(),
        "sampling is source-preserving"
    );
    assert_eq!(native, project_file::encode(project, None).unwrap());
}
fn state(project: &Project, native: bool) -> EditorState {
    let mut s = EditorState::default();
    s.editor.replace_project(project.clone()).unwrap();
    s.editor.select(1);
    s.selected_layers.insert(1);
    s.contents_selection = Some((project.active_composition_id(), 1, PAINT));
    s.editor.clear_history();
    s.frame = FIRST;
    s.tool = Tool::Select;
    s.preview_zoom = Some(if native { 1. } else { 1.5 });
    s.preview_pan = if native { [0., 0.] } else { [13., -7.] };
    s.timeline_zoom = if native { 1. } else { 2. };
    if native {
        s.workspace.fractions[2] = 0.52;
    }
    if !native {
        let channel = GraphChannel {
            id: 1,
            property: PropertyPath::Contents {
                item: PAINT,
                parameter: ContentsParam::Gradient(GradientParam::EndX),
            },
        };
        s.graph_channels.pin(channel).unwrap();
        s.graph_channels.ranges.insert(
            channel,
            GraphRanges {
                value: Some([-200., 300.]),
                speed: Some([-30., 30.]),
            },
        );
    }
    s.normalize();
    s
}
fn codec(s: &mut EditorState, expected: &Project) {
    let views = s.capture_views();
    let view = views.encode_native(expected).unwrap();
    let bytes = project_file::encode(s.editor.project(), Some(&view)).unwrap();
    assert_eq!(&bytes[8..10], &[1, 0], "unchanged native container version");
    let decoded = project_file::decode(&bytes).unwrap();
    exact(&decoded.project, expected, "official LEP full source");
    assert_eq!(decoded.view, Some(view.as_slice()));
    let loaded = ProjectViews::read_native(decoded.view.unwrap(), &decoded.project).unwrap();
    assert_eq!(loaded, views);
    let mut reopened = EditorState::default();
    reopened.editor.replace_project(decoded.project).unwrap();
    reopened.load_views(loaded);
    assert_eq!(
        reopened.capture_views().encode_native(expected).unwrap(),
        view
    );
}
fn command(edit: GradientColorsEdit) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::GradientColors { item: PAINT, edit },
    }
}
fn apply_history(before: &Project, expected: &Project, edit: GradientColorsEdit) {
    let mut s = state(before, false);
    let views = s.capture_views();
    s.bulk_test_action(&Action::Edit(command(edit)));
    assert_eq!(s.status, "Edited");
    exact(s.editor.project(), expected, "literal command result");
    assert_eq!(s.capture_views(), views);
    codec(&mut s, expected);
    s.bulk_test_action(&Action::Undo);
    exact(s.editor.project(), before, "one Undo");
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
    s.bulk_test_action(&Action::Redo);
    exact(s.editor.project(), expected, "one Redo");
    assert_eq!(s.capture_views(), views);
}

fn selection(frames: &[Frame]) -> BTreeSet<Frame> {
    frames.iter().copied().collect()
}
fn copy_pair() -> Vec<GradientColorsKeyCopy> {
    vec![
        GradientColorsKeyCopy {
            offset: 0,
            colors: first(),
            interpolation: GradientColorsInterpolation::Hold,
        },
        GradientColorsKeyCopy {
            offset: 15,
            colors: second(),
            interpolation: GradientColorsInterpolation::Linear,
        },
    ]
}
fn moved_overlap(stroke: bool, radial: bool) -> Project {
    keyed(
        &scene(stroke, radial),
        &[(15, first()), (30, second()), (45, third())],
        &[
            (30, GradientColorsInterpolation::Linear),
            (45, GradientColorsInterpolation::Smooth),
        ],
        57,
    )
}
fn moved_noncontiguous(stroke: bool, radial: bool) -> Project {
    keyed(
        &scene(stroke, radial),
        &[(0, first()), (30, second()), (60, third())],
        &[
            (30, GradientColorsInterpolation::Linear),
            (60, GradientColorsInterpolation::Smooth),
        ],
        57,
    )
}
fn pasted(stroke: bool, radial: bool) -> Project {
    keyed(
        &scene(stroke, radial),
        &[
            (0, first()),
            (15, second()),
            (45, third()),
            (60, first()),
            (75, second()),
        ],
        &[
            (15, GradientColorsInterpolation::Linear),
            (45, GradientColorsInterpolation::Smooth),
            (75, GradientColorsInterpolation::Linear),
        ],
        57,
    )
}
fn repeated_paste() -> Project {
    keyed(
        &scene(false, false),
        &[
            (0, first()),
            (15, second()),
            (45, third()),
            (60, first()),
            (75, second()),
            (90, first()),
            (105, second()),
        ],
        &[
            (15, GradientColorsInterpolation::Linear),
            (45, GradientColorsInterpolation::Smooth),
            (75, GradientColorsInterpolation::Linear),
            (105, GradientColorsInterpolation::Linear),
        ],
        57,
    )
}
fn group_smooth(stroke: bool, radial: bool) -> Project {
    keyed(
        &scene(stroke, radial),
        &[(0, first()), (15, second()), (45, third())],
        &[
            (0, GradientColorsInterpolation::Smooth),
            (15, GradientColorsInterpolation::Smooth),
            (45, GradientColorsInterpolation::Smooth),
        ],
        57,
    )
}
fn deleted_pair(stroke: bool, radial: bool) -> Project {
    keyed(
        &scene(stroke, radial),
        &[(45, third())],
        &[(45, GradientColorsInterpolation::Smooth)],
        57,
    )
}
fn baked30() -> GradientColors {
    // Deliberately literal expected midpoint, independent of both the production
    // sampler and the arithmetic reference helper. It is not any selected key.
    colors(json!({
        "colors":[
            {"id":1,"position":55.,"midpoint":64.5,"red":128.,"green":192.,"blue":104.},
            {"id":5,"position":70.,"midpoint":35.5,"red":128.,"green":104.,"blue":240.},
            {"id":2,"position":45.,"midpoint":62.5,"red":200.,"green":112.,"blue":112.}
        ],
        "opacities":[
            {"id":3,"position":60.,"midpoint":18.,"opacity":60.},
            {"id":6,"position":70.,"midpoint":79.5,"opacity":80.},
            {"id":4,"position":50.,"midpoint":75.,"opacity":60.}
        ]
    }))
}

#[test]
fn multikey_group_retime_preserves_gaps_selected_overlap_local_ids_and_sparse_modes() {
    let _evidence = PixelEvidence::new("group_move_all_four_paints");
    for stroke in [false, true] {
        for radial in [false, true] {
            let before = fixture(stroke, radial);
            let moved = moved_overlap(stroke, radial);
            apply_history(
                &before,
                &moved,
                GradientColorsEdit::MoveKeys {
                    frames: selection(&[0, 15]),
                    to: 15,
                },
            );
            assert_render(&moved);
            let gaps = moved_noncontiguous(stroke, radial);
            apply_history(
                &before,
                &gaps,
                GradientColorsEdit::MoveKeys {
                    frames: selection(&[15, 45]),
                    to: 30,
                },
            );
            assert_render(&gaps);
            assert_eq!(
                animation(&gaps).keys().keys().copied().collect::<Vec<_>>(),
                vec![0, 30, 60]
            );
            assert_eq!(
                animation(&gaps).interpolation(60),
                Some(GradientColorsInterpolation::Smooth)
            );
            assert_eq!(
                animation(&gaps).segment_status(60).unwrap().hold_reason,
                Some(GradientColorsHoldReason::NoNextKey)
            );
        }
    }
}

#[test]
fn multikey_paste_keeps_entire_snapshots_modes_timing_and_no_stop_remapping() {
    let _evidence = PixelEvidence::new("paste_all_four_paints");
    for stroke in [false, true] {
        for radial in [false, true] {
            let before = fixture(stroke, radial);
            let expected = pasted(stroke, radial);
            apply_history(
                &before,
                &expected,
                GradientColorsEdit::PasteKeys {
                    keys: copy_pair(),
                    frame: 60,
                },
            );
            assert_render(&expected);
            let before_wire = serde_json::to_value(paint(&before)).unwrap();
            let after_wire = serde_json::to_value(paint(&expected)).unwrap();
            let mut a = before_wire;
            let mut b = after_wire;
            assert_eq!(gradient_wire(&mut a)["next_stop"], json!(9));
            assert_eq!(gradient_wire(&mut b)["next_stop"], json!(9));
            assert_eq!(animation(&expected).keys()[&60], first());
            assert_eq!(animation(&expected).keys()[&75], second());
            assert_eq!(sample(&expected, 30), baked30());
        }
    }
    let again = repeated_paste();
    apply_history(
        &pasted(false, false),
        &again,
        GradientColorsEdit::PasteKeys {
            keys: copy_pair(),
            frame: 90,
        },
    );
    assert_render(&again);
    // A noncontiguous copied selection keeps its 45-frame gap and dormant mode.
    let before = fixture(false, false);
    let expected = keyed(
        &scene(false, false),
        &[
            (0, first()),
            (15, second()),
            (45, third()),
            (60, first()),
            (105, third()),
        ],
        &[
            (15, GradientColorsInterpolation::Linear),
            (45, GradientColorsInterpolation::Smooth),
            (105, GradientColorsInterpolation::Smooth),
        ],
        57,
    );
    apply_history(
        &before,
        &expected,
        GradientColorsEdit::PasteKeys {
            keys: vec![
                GradientColorsKeyCopy {
                    offset: 45,
                    colors: third(),
                    interpolation: GradientColorsInterpolation::Smooth,
                },
                GradientColorsKeyCopy {
                    offset: 0,
                    colors: first(),
                    interpolation: GradientColorsInterpolation::Hold,
                },
            ],
            frame: 60,
        },
    );
    assert_render(&expected);
}

#[test]
fn multikey_modes_are_atomic_sparse_and_render_from_literal_static_oracles() {
    let _evidence = PixelEvidence::new("group_modes");
    for stroke in [false, true] {
        for radial in [false, true] {
            let before = fixture(stroke, radial);
            let smooth = group_smooth(stroke, radial);
            apply_history(
                &before,
                &smooth,
                GradientColorsEdit::SetInterpolations {
                    frames: selection(&[0, 15]),
                    interpolation: GradientColorsInterpolation::Smooth,
                },
            );
            assert_render(&smooth);
            let hold = keyed(
                &scene(stroke, radial),
                &[(0, first()), (15, second()), (45, third())],
                &[],
                57,
            );
            apply_history(
                &smooth,
                &hold,
                GradientColorsEdit::SetInterpolations {
                    frames: selection(&[0, 15, 45]),
                    interpolation: GradientColorsInterpolation::Hold,
                },
            );
            assert!(!hold.to_json().unwrap().contains("outgoing_interpolation"));
            assert_render(&hold);
        }
    }
}

#[test]
fn multikey_delete_selection_retains_dormant_mode_or_bakes_explicit_playhead_sample() {
    let _evidence = PixelEvidence::new("group_delete_and_explicit_bake");
    for stroke in [false, true] {
        for radial in [false, true] {
            let before = fixture(stroke, radial);
            let retained = deleted_pair(stroke, radial);
            apply_history(
                &before,
                &retained,
                GradientColorsEdit::DeleteKeys {
                    frames: selection(&[0, 15]),
                    frame: 30,
                },
            );
            assert_render(&retained);
            let expected = materialized(&before, &baked30());
            assert_ne!(baked30(), first());
            assert_ne!(baked30(), second());
            assert_ne!(baked30(), third());
            apply_history(
                &before,
                &expected,
                GradientColorsEdit::DeleteKeys {
                    frames: selection(&[0, 15, 45]),
                    frame: 30,
                },
            );
            assert!(!expected.to_json().unwrap().contains("colors_animation"));
            for frame in FRAMES {
                compare_paths(&expected, frame, &materialized(&before, &baked30()));
            }
        }
    }
}

#[test]
fn multikey_oracles_have_distinct_pixels_and_preserve_full_unrelated_source() {
    let renderer = Renderer::new();
    let seed = fixture(false, false);
    let a = renderer
        .render(&materialized(&seed, &first()), 0, WIDTH)
        .unwrap();
    let b = renderer
        .render(&materialized(&seed, &second()), 0, WIDTH)
        .unwrap();
    let c = renderer
        .render(&materialized(&seed, &third()), 0, WIDTH)
        .unwrap();
    assert_ne!(a, b, "first/second oracle must be visually distinguishable");
    assert_ne!(a, c, "first/third oracle must be visually distinguishable");
    assert_ne!(b, c, "second/third oracle must be visually distinguishable");
    let bake = renderer
        .render(&materialized(&seed, &baked30()), 0, WIDTH)
        .unwrap();
    assert_ne!(bake, a);
    assert_ne!(bake, b);
    assert_ne!(bake, c);
    let moved = renderer
        .render(&moved_overlap(false, false), 15, WIDTH)
        .unwrap();
    assert_ne!(
        moved,
        renderer.render(&seed, 15, WIDTH).unwrap(),
        "retiming a real snapshot must alter the chosen frame"
    );

    // Preserve a dormant animated endpoint, an unrelated layer, its transform
    // key metadata and selection/Graph VIEW. Expected source edits only the maps.
    let mut e = Editor::default();
    e.replace_project(seed.clone()).unwrap();
    e.execute(Command::AddNull).unwrap();
    e.execute(Command::ImportAsset {
        content: Content::Image {png:"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==".into()},
        width:1.,height:1.,name:"Unused persistent asset sentinel".into(),folder:None,frame:None,
    }).unwrap();
    let mut raw = serde_json::to_value(e.project()).unwrap();
    let layers = raw["composition"]["layers"].as_array_mut().unwrap();
    if layers[0]["id"] != json!(1) {
        layers.swap(0, 1);
    }
    let mut node: ContentsNode = serde_json::from_value(paint_wire(&mut raw).clone()).unwrap();
    node.parameters.insert(ContentsParam::Gradient(GradientParam::EndX),serde_json::from_value(json!({"value":145.,"keys":{"0":{"value":145.,"interpolation":"Hold"},"119":{"value":175.,"interpolation":"Hold"}}})).unwrap());
    *paint_wire(&mut raw) = serde_json::to_value(node).unwrap();
    let before = Project::from_json(&raw.to_string()).unwrap();
    assert_eq!(before.asset_library().assets().len(), 1);
    let expected = keyed(
        &before,
        &[(15, first()), (30, second()), (45, third())],
        &[
            (30, GradientColorsInterpolation::Linear),
            (45, GradientColorsInterpolation::Smooth),
        ],
        57,
    );
    apply_history(
        &before,
        &expected,
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: 15,
        },
    );
}

fn history_sentinel(project: &Project) -> (EditorState, Project) {
    let mut s = state(project, false);
    s.bulk_test_action(&Action::Edit(Command::RenameLayer {
        id: 1,
        name: "Redo sentinel".into(),
    }));
    let redo = s.editor.project().clone();
    s.bulk_test_action(&Action::Undo);
    assert!(s.editor.can_redo());
    assert!(!s.editor.can_undo());
    (s, redo)
}
fn rejected(s: &mut EditorState, edit: GradientColorsEdit) {
    let before = s.editor.project().clone();
    let view = s.capture_views();
    let undo = s.editor.can_undo();
    let redo = s.editor.can_redo();
    assert!(s.editor.execute(command(edit)).is_err());
    exact(
        s.editor.project(),
        &before,
        "rejected group command remains atomic",
    );
    assert_eq!(s.capture_views(), view);
    assert_eq!(s.editor.can_undo(), undo);
    assert_eq!(s.editor.can_redo(), redo);
}
#[test]
fn multikey_equal_actions_and_returning_pure_batch_preserve_redo_and_legacy_schema() {
    let before = fixture(false, false);
    let (mut s, redo) = history_sentinel(&before);
    let view = s.capture_views();
    for edit in [
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: 0,
        },
        GradientColorsEdit::SetInterpolations {
            frames: selection(&[15]),
            interpolation: GradientColorsInterpolation::Linear,
        },
        GradientColorsEdit::SetInterpolations {
            frames: selection(&[0]),
            interpolation: GradientColorsInterpolation::Hold,
        },
        GradientColorsEdit::PasteKeys {
            keys: copy_pair(),
            frame: 0,
        },
    ] {
        s.bulk_test_action(&Action::Edit(command(edit)));
        exact(
            s.editor.project(),
            &before,
            "equal complete-payload group no-op",
        );
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        assert_eq!(s.capture_views(), view);
    }
    s.bulk_test_action(&Action::Edit(Command::Batch(vec![
        command(GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: 15,
        }),
        command(GradientColorsEdit::MoveKeys {
            frames: selection(&[15, 30]),
            to: 0,
        }),
    ])));
    exact(s.editor.project(), &before, "returning pure batch");
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
    s.bulk_test_action(&Action::Redo);
    exact(s.editor.project(), &redo, "complete Redo survives no-ops");

    let old = keyed(
        &scene(false, false),
        &[(0, first()), (15, second()), (45, third())],
        &[],
        54,
    );
    let moved = keyed(
        &scene(false, false),
        &[(15, first()), (30, second()), (45, third())],
        &[],
        54,
    );
    apply_history(
        &old,
        &moved,
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: 15,
        },
    );
    let smooth = keyed(
        &scene(false, false),
        &[(0, first()), (15, second()), (45, third())],
        &[
            (0, GradientColorsInterpolation::Smooth),
            (15, GradientColorsInterpolation::Smooth),
        ],
        57,
    );
    apply_history(
        &old,
        &smooth,
        GradientColorsEdit::SetInterpolations {
            frames: selection(&[0, 15]),
            interpolation: GradientColorsInterpolation::Smooth,
        },
    );
}

#[test]
fn multikey_collision_missing_empty_overflow_lock_and_malformed_payloads_preserve_redo() {
    let before = fixture(false, false);
    let (mut s, redo) = history_sentinel(&before);
    let mut invalid = copy_pair();
    invalid[1].offset = 0;
    let mut wrong_id = copy_pair();
    wrong_id[0].colors.colors[0].id = 3;
    let mut nan = copy_pair();
    nan[0].colors.colors[0].position = f64::NAN;
    let mut wrong_anchor = copy_pair();
    for k in &mut wrong_anchor {
        k.offset += 1;
    }
    let mut partial_collision = copy_pair();
    partial_collision[1].offset = 30;
    for edit in [
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: 30,
        },
        GradientColorsEdit::MoveKeys {
            frames: selection(&[15, 45]),
            to: 110,
        },
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: u32::MAX,
        },
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 14]),
            to: 15,
        },
        GradientColorsEdit::MoveKeys {
            frames: selection(&[]),
            to: 15,
        },
        GradientColorsEdit::DeleteKeys {
            frames: selection(&[0, 14]),
            frame: 30,
        },
        GradientColorsEdit::DeleteKeys {
            frames: selection(&[]),
            frame: 30,
        },
        GradientColorsEdit::DeleteKeys {
            frames: selection(&[0, 15]),
            frame: 120,
        },
        GradientColorsEdit::SetInterpolations {
            frames: selection(&[0, 14]),
            interpolation: GradientColorsInterpolation::Linear,
        },
        GradientColorsEdit::SetInterpolations {
            frames: selection(&[]),
            interpolation: GradientColorsInterpolation::Linear,
        },
        GradientColorsEdit::PasteKeys {
            keys: vec![],
            frame: 60,
        },
        GradientColorsEdit::PasteKeys {
            keys: copy_pair(),
            frame: 110,
        },
        GradientColorsEdit::PasteKeys {
            keys: copy_pair(),
            frame: u32::MAX,
        },
        GradientColorsEdit::PasteKeys {
            keys: copy_pair(),
            frame: 15,
        },
        GradientColorsEdit::PasteKeys {
            keys: partial_collision,
            frame: 0,
        },
        GradientColorsEdit::PasteKeys {
            keys: invalid,
            frame: 60,
        },
        GradientColorsEdit::PasteKeys {
            keys: wrong_id,
            frame: 60,
        },
        GradientColorsEdit::PasteKeys {
            keys: nan,
            frame: 60,
        },
        GradientColorsEdit::PasteKeys {
            keys: wrong_anchor,
            frame: 60,
        },
    ] {
        rejected(&mut s, edit);
    }
    s.bulk_test_action(&Action::Redo);
    exact(
        s.editor.project(),
        &redo,
        "redo survives every rejected payload",
    );
    let mut locked = serde_json::to_value(&before).unwrap();
    locked["composition"]["layers"][0]["locked"] = json!(true);
    let locked = Project::from_json(&locked.to_string()).unwrap();
    let mut s = state(&locked, false);
    for edit in [
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: 15,
        },
        GradientColorsEdit::DeleteKeys {
            frames: selection(&[0, 15]),
            frame: 30,
        },
        GradientColorsEdit::SetInterpolations {
            frames: selection(&[0, 15]),
            interpolation: GradientColorsInterpolation::Smooth,
        },
        GradientColorsEdit::PasteKeys {
            keys: copy_pair(),
            frame: 60,
        },
    ] {
        rejected(&mut s, edit);
    }
}

#[test]
fn multikey_budgets_and_mixed_batches_reject_without_partial_history_or_source_changes() {
    let before = fixture(false, false);
    let (mut s, redo) = history_sentinel(&before);
    for edit in [
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: 15,
        },
        GradientColorsEdit::DeleteKeys {
            frames: selection(&[0, 15]),
            frame: 30,
        },
        GradientColorsEdit::SetInterpolations {
            frames: selection(&[0, 15]),
            interpolation: GradientColorsInterpolation::Smooth,
        },
        GradientColorsEdit::PasteKeys {
            keys: copy_pair(),
            frame: 60,
        },
    ] {
        assert!(
            s.editor
                .execute(Command::Batch(vec![command(edit), Command::AddNull]))
                .is_err()
        );
        exact(s.editor.project(), &before, "mixed batch");
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
    }
    s.bulk_test_action(&Action::Redo);
    exact(
        s.editor.project(),
        &redo,
        "mixed failures keep complete Redo",
    );
    for full_stops in [false, true] {
        let mut wire = serde_json::to_value(&before).unwrap();
        wire["composition"]["duration"] = json!(2000);
        wire["composition"]["layers"][0]["out_frame"] = json!(2000);
        let mut c = first();
        if full_stops {
            for id in 7..=35 {
                let mut stop = c.colors[0].clone();
                stop.id = id;
                c.colors.push(stop);
            }
            for id in 36..=64 {
                let mut stop = c.opacities[0].clone();
                stop.id = id;
                c.opacities.push(stop);
            }
            gradient_wire(paint_wire(&mut wire))["next_stop"] = json!(65);
        }
        let count = if full_stops { 512 } else { 1000 };
        gradient_wire(paint_wire(&mut wire))["colors_animation"] =
            json!({"keys":(0..count).map(|i|(i,c.clone())).collect::<BTreeMap<_,_>>()});
        let full = Project::from_json(&wire.to_string()).unwrap();
        let (mut s, redo) = history_sentinel(&full);
        rejected(
            &mut s,
            GradientColorsEdit::PasteKeys {
                keys: vec![GradientColorsKeyCopy {
                    offset: 0,
                    colors: c.clone(),
                    interpolation: GradientColorsInterpolation::Hold,
                }],
                frame: 1500,
            },
        );
        // At the budget an exact complete-payload no-op still succeeds.
        s.bulk_test_action(&Action::Edit(command(GradientColorsEdit::PasteKeys {
            keys: vec![GradientColorsKeyCopy {
                offset: 0,
                colors: c,
                interpolation: GradientColorsInterpolation::Hold,
            }],
            frame: 0,
        })));
        exact(s.editor.project(), &full, "at-budget no-op");
        assert!(s.editor.can_redo());
        assert!(!s.editor.can_undo());
        s.bulk_test_action(&Action::Redo);
        exact(s.editor.project(), &redo, "at-budget history retained");
    }
}

#[test]
fn multikey_malformed_source_and_codec_versions_fail_without_replacing_existing_document() {
    let before = fixture(false, false);
    let (mut s, redo) = history_sentinel(&before);
    for kind in 0..6 {
        let mut raw = serde_json::to_value(&before).unwrap();
        match kind {
            0 => raw["version"] = json!(56),
            1 => {
                gradient_wire(paint_wire(&mut raw))["colors_animation"]["outgoing_interpolation"] =
                    json!({"0":"Hold"})
            }
            2 => {
                gradient_wire(paint_wire(&mut raw))["colors_animation"]["outgoing_interpolation"] =
                    json!({"1":"Linear"})
            }
            3 => gradient_wire(paint_wire(&mut raw))["next_stop"] = json!(4),
            4 => {
                gradient_wire(paint_wire(&mut raw))["colors_animation"]["keys"]["0"]["colors"][0]
                    ["position"] = json!(101.)
            }
            _ => {
                gradient_wire(paint_wire(&mut raw))["colors_animation"]["keys"]["0"]["opacities"]
                    [0]["id"] = json!(1)
            }
        }
        assert!(Project::from_json(&raw.to_string()).is_err());
        let invalid: Project = serde_json::from_value(raw).unwrap();
        assert!(project_file::encode(&invalid, None).is_err());
        assert!(s.editor.replace_project(invalid).is_err());
        exact(
            s.editor.project(),
            &before,
            "invalid source failed before replacement",
        );
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
    }
    s.bulk_test_action(&Action::Redo);
    exact(
        s.editor.project(),
        &redo,
        "malformed loading preserved history",
    );
}

#[test]
fn multikey_copy_payload_restores_dormant_local_ids_and_modes_without_remapping() {
    let before = keyed(
        &scene(false, false),
        &[(0, first()), (15, second())],
        &[],
        54,
    );
    let mut retained = third();
    retained.colors[1].id = 777;
    let mut raw = serde_json::to_value(&before).unwrap();
    raw["version"] = json!(57);
    gradient_wire(paint_wire(&mut raw))["next_stop"] = json!(778);
    gradient_wire(paint_wire(&mut raw))["colors_animation"] = json!({
        "keys":{"0":first(),"15":second(),"60":retained},
        "outgoing_interpolation":{"60":"Smooth"}
    });
    let expected = Project::from_json(&raw.to_string()).unwrap();
    apply_history(
        &before,
        &expected,
        GradientColorsEdit::PasteKeys {
            keys: vec![GradientColorsKeyCopy {
                offset: 0,
                colors: retained,
                interpolation: GradientColorsInterpolation::Smooth,
            }],
            frame: 60,
        },
    );
    assert_eq!(animation(&expected).keys()[&60].colors[1].id, 777);
    assert_eq!(
        animation(&expected).interpolation(60),
        Some(GradientColorsInterpolation::Smooth)
    );
}

#[test]
fn multikey_retime_and_copy_preserve_ordered_topology_fallback_without_matching_stops() {
    let _evidence = PixelEvidence::new("topology_fallback_preserved");
    let mut different = second();
    different.opacities.swap(0, 1);
    let before = keyed(
        &scene(false, false),
        &[(0, first()), (15, different.clone()), (45, third())],
        &[
            (0, GradientColorsInterpolation::Linear),
            (15, GradientColorsInterpolation::Smooth),
            (45, GradientColorsInterpolation::Linear),
        ],
        57,
    );
    let moved = keyed(
        &scene(false, false),
        &[(15, first()), (30, different.clone()), (45, third())],
        &[
            (15, GradientColorsInterpolation::Linear),
            (30, GradientColorsInterpolation::Smooth),
            (45, GradientColorsInterpolation::Linear),
        ],
        57,
    );
    apply_history(
        &before,
        &moved,
        GradientColorsEdit::MoveKeys {
            frames: selection(&[0, 15]),
            to: 15,
        },
    );
    for frame in [15, 30] {
        let status = animation(&moved).segment_status(frame).unwrap();
        assert_eq!(status.effective, GradientColorsInterpolation::Hold);
        assert_eq!(
            status.hold_reason,
            Some(GradientColorsHoldReason::IncompatibleTopology)
        );
    }
    assert_render(&moved);
    let pasted = keyed(
        &scene(false, false),
        &[
            (0, first()),
            (15, different.clone()),
            (45, third()),
            (60, first()),
            (75, different.clone()),
        ],
        &[
            (0, GradientColorsInterpolation::Linear),
            (15, GradientColorsInterpolation::Smooth),
            (45, GradientColorsInterpolation::Linear),
            (60, GradientColorsInterpolation::Linear),
            (75, GradientColorsInterpolation::Smooth),
        ],
        57,
    );
    apply_history(
        &before,
        &pasted,
        GradientColorsEdit::PasteKeys {
            keys: vec![
                GradientColorsKeyCopy {
                    offset: 0,
                    colors: first(),
                    interpolation: GradientColorsInterpolation::Linear,
                },
                GradientColorsKeyCopy {
                    offset: 15,
                    colors: different,
                    interpolation: GradientColorsInterpolation::Smooth,
                },
            ],
            frame: 60,
        },
    );
    assert_eq!(
        animation(&pasted).keys()[&75]
            .opacities
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![6, 3, 4]
    );
    assert_render(&pasted);
}

fn write_immutable(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => file.write_all(bytes).unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => assert_eq!(
            std::fs::read(path).unwrap(),
            bytes,
            "immutable fixture {}",
            path.display()
        ),
        Err(e) => panic!("cannot export {}: {e}", path.display()),
    }
}
fn export(root: &Path, label: &str, project: &Project, views: &ProjectViews) {
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let view = views.encode_native(project).unwrap();
    write_immutable(
        &root.join(format!("{label}.lfe.json")),
        project.to_json().unwrap().as_bytes(),
    );
    write_immutable(
        &root.join(format!("{label}.generated.lep")),
        &project_file::encode(project, Some(&view)).unwrap(),
    );
}
#[test]
#[ignore = "requires an explicit absolute fixture output directory"]
fn export_gradient_multikey_acceptance_fixtures() {
    let root = std::env::var_os("LIBREEFFECTS_EXPORT_GRADIENT_MULTIKEY_FIXTURES")
        .expect("set fixture directory");
    let root = Path::new(&root);
    let before = fixture(false, false);
    let views = state(&before, true).capture_views();
    for (label, project) in [
        ("native-before", before.clone()),
        ("native-moved-overlap-expected", moved_overlap(false, false)),
        (
            "native-moved-noncontiguous-expected",
            moved_noncontiguous(false, false),
        ),
        ("native-group-smooth-expected", group_smooth(false, false)),
        ("native-pasted60-expected", pasted(false, false)),
        ("native-pasted90-repeat-expected", repeated_paste()),
        ("native-deleted-pair-expected", deleted_pair(false, false)),
        (
            "native-deleted-all-at30-expected",
            materialized(&before, &baked30()),
        ),
    ] {
        export(root, label, &project, &views);
    }
    let mut render_cases = vec![];
    for stroke in [false, true] {
        for radial in [false, true] {
            let paint = format!(
                "{}-{}",
                if stroke { "stroke" } else { "fill" },
                if radial { "radial" } else { "linear" }
            );
            for (action, project) in [
                ("seed", fixture(stroke, radial)),
                ("moved-overlap", moved_overlap(stroke, radial)),
                ("pasted60", pasted(stroke, radial)),
            ] {
                let name = format!("{paint}-{action}");
                export(root, &name, &project, &views);
                for frame in FRAMES {
                    let expected = materialized(&project, &reference_sample(&project, frame));
                    let reference = format!("{name}-static-frame-{frame:03}");
                    export(root, &reference, &expected, &views);
                    render_cases.push(json!({"input":format!("{name}.generated.lep"),"expected":format!("{reference}.generated.lep"),"frame":frame,"width":WIDTH,"height":HEIGHT,"comparison":"exact full RGBA, no masks"}));
                }
            }
        }
    }
    let manifest = json!({
        "fixture_kind":"generated immutable inputs and independent expected sources; not native evidence",
        "dimensions":[WIDTH,HEIGHT],"frames":FRAMES,"duration":120,"native_initial_frame":0,"paint_item":PAINT,
        "schema":"Existing 54/57 only; prior interpolation release must render every new generated source identically",
        "native_cases":[
            {"label":"move-selected-overlap","start":"native-before","action":"Select keys 0 and 15; Move earliest selected frame to 15; confirm keys 15/30/45 and one Undo/Redo", "expected":"native-moved-overlap-expected"},
            {"label":"move-noncontiguous","start":"native-before","action":"Select keys 15 and 45; Move earliest selected frame to 30; preserve 30-frame gap", "expected":"native-moved-noncontiguous-expected"},
            {"label":"modes","start":"native-before","action":"Select keys 0 and 15; set Smoothstep together; terminal 45 already Smoothstep", "expected":"native-group-smooth-expected"},
            {"label":"copy-paste","start":"native-before","action":"Select keys 0 and 15; Copy; go to playhead 60 and Paste on the same paint; keys become 0/15/45/60/75", "expected":"native-pasted60-expected"},
            {"label":"paste-repeat","start":"native-pasted60-expected","action":"Without saving or changing source independently, move playhead to90 and Paste the same internal copied keys again; new keys90/105","expected":"native-pasted90-repeat-expected"},
            {"label":"collision","start":"native-before","action":"Select keys 0 and 15; attempt Move earliest to 30; unselected occupied 45 rejects without history", "expected":"native-before"},
            {"label":"delete-pair","start":"native-before","action":"Select keys 0 and 15; Delete; retain key 45 and its dormant Smoothstep; one Undo/Redo", "expected":"native-deleted-pair-expected"},
            {"label":"delete-all","start":"native-before","action":"Select all three keys, set playhead 30, Delete; bake independently literal interpolated sample", "expected":"native-deleted-all-at30-expected"},
            {"label":"no-op","start":"native-before","action":"Create Redo via Move and Undo, then select 0/15 and Move earliest to unchanged 0; Redo must remain available", "expected":"native-before"},
            {"label":"reopen","start":"an actual native save","action":"New, Open the actual saved file and Save As separately; compare complete source and VIEW", "expected":"the same independently declared source and VIEW"}
        ],
        "native_verifier":{"test":"verify_recorded_native_gradient_multikey_save","actual_env":"LIBREEFFECTS_GRADIENT_MULTIKEY_NATIVE_SAVE","expected_env":"LIBREEFFECTS_GRADIENT_MULTIKEY_EXPECTED","optional_frame_env":"LIBREEFFECTS_GRADIENT_MULTIKEY_EXPECTED_FRAME","rules":"Full source and full VIEW equality. Only a declared expected frame override is allowed. Actual saves are separate from immutable generated references."},
        "render_cases":render_cases
    });
    write_immutable(
        &root.join("cases.json"),
        serde_json::to_string_pretty(&manifest).unwrap().as_bytes(),
    );
}
#[test]
#[ignore = "requires separately recorded native save and independent expected fixture"]
fn verify_recorded_native_gradient_multikey_save() {
    let _evidence = PixelEvidence::new("native_full_source_view_literal_static_oracle");
    let actual_path = std::env::var_os("LIBREEFFECTS_GRADIENT_MULTIKEY_NATIVE_SAVE")
        .expect("set actual native path");
    let expected_path = std::env::var_os("LIBREEFFECTS_GRADIENT_MULTIKEY_EXPECTED")
        .expect("set independent expected LEP");
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_GRADIENT_MULTIKEY_EXPECTED_FRAME") {
        let frame: Frame = frame.parse().expect("unsigned frame");
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
        "actual native complete source",
    );
    assert_eq!(
        actual.views, expected.views,
        "actual native full VIEW; no masks"
    );
    for frame in FRAMES {
        let reference = if paint(&expected.project)
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .is_some()
        {
            materialized(
                &expected.project,
                &reference_sample(&expected.project, frame),
            )
        } else {
            expected.project.clone()
        };
        compare_paths(&actual.project, frame, &reference);
    }
    println!(
        "Verified complete native source, full VIEW and {} five-route independent-static RGBA frame sets: {}",
        FRAMES.len(),
        Path::new(&actual_path).display()
    );
}
