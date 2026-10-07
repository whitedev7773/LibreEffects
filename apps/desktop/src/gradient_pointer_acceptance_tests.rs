//! Independent compound Gradient Colors pointer-drag acceptance. Literal source
//! snapshots and a legacy-static render route form the oracle. No command output
//! or production compound sampler supplies expected source or expected pixels.
//! Generated files are immutable inputs/references, never native UI evidence.
use crate::{
    editor::{Action, EditorState, Tool},
    panels::{CompoundColorsInput as Input, CompoundPointerGeometry as PointerGeometry},
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
                "GRADIENT_POINTER_PIXEL_EVIDENCE {}: {pairs} exact RGBA pairs / {} pixels",
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
        name: "Gradient pointer acceptance".into(),
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
        name: "Gradient Colors pointer".into(),
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
    s.expanded = true;
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
// These expectations are built solely from the three independently authored
// snapshots above. They never inspect a command result or drag candidate.
fn literal_retime(stroke: bool, radial: bool, selected: &[Frame], to: Frame) -> Project {
    assert!(!selected.is_empty());
    let frames: BTreeSet<_> = selected.iter().copied().collect();
    assert_eq!(selected.len(), frames.len());
    assert!(frames.iter().all(|f| [0, 15, 45].contains(f)));
    let delta = i64::from(to) - i64::from(*frames.first().unwrap());
    let mut keys = vec![];
    let mut modes = vec![];
    let mut occupied = BTreeSet::new();
    for (frame, snapshot, mode) in [
        (0, first(), GradientColorsInterpolation::Hold),
        (15, second(), GradientColorsInterpolation::Linear),
        (45, third(), GradientColorsInterpolation::Smooth),
    ] {
        let destination = i64::from(frame) + if frames.contains(&frame) { delta } else { 0 };
        assert!((0..120).contains(&destination));
        let destination = Frame::try_from(destination).unwrap();
        assert!(
            occupied.insert(destination),
            "literal expectation collision"
        );
        keys.push((destination, snapshot));
        if mode != GradientColorsInterpolation::Hold {
            modes.push((destination, mode));
        }
    }
    keyed(&scene(stroke, radial), &keys, &modes, 57)
}
fn locked_fixture(stroke: bool, radial: bool) -> Project {
    let mut wire = serde_json::to_value(fixture(stroke, radial)).unwrap();
    wire["composition"]["layers"][0]["locked"] = json!(true);
    Project::from_json(&wire.to_string()).unwrap()
}
fn topology_fixture(stroke: bool, radial: bool, moved: bool) -> Project {
    let mut different = second();
    different.opacities.swap(0, 1);
    keyed(
        &scene(stroke, radial),
        &if moved {
            [(15, first()), (30, different), (45, third())]
        } else {
            [(0, first()), (15, different), (45, third())]
        },
        &if moved {
            [
                (15, GradientColorsInterpolation::Linear),
                (30, GradientColorsInterpolation::Smooth),
                (45, GradientColorsInterpolation::Linear),
            ]
        } else {
            [
                (0, GradientColorsInterpolation::Linear),
                (15, GradientColorsInterpolation::Smooth),
                (45, GradientColorsInterpolation::Linear),
            ]
        },
        57,
    )
}
fn geometry(s: &EditorState, scale: f32) -> PointerGeometry {
    PointerGeometry {
        bounds: gpui::Bounds::new(
            gpui::point(gpui::px(100.), gpui::px(40.)),
            gpui::size(gpui::px(s.visible_frames() as f32 * scale), gpui::px(20.)),
        ),
        start: s.timeline_start,
        visible: s.visible_frames(),
    }
}
fn pointer_input(s: &EditorState, selected: &[Frame]) -> Input {
    let mut input = Input::default();
    input.observe(s);
    input.select_frames(s, PAINT, selected.iter().copied().collect());
    assert_eq!(input.selected_frames(), selected.iter().copied().collect());
    input
}
fn assert_neutral(s: &mut EditorState, source: &Project, views: &ProjectViews, redo: bool) {
    exact(
        s.editor.project(),
        source,
        "provisional pointer source neutrality",
    );
    assert_eq!(
        &s.capture_views(),
        views,
        "provisional pointer complete VIEW neutrality"
    );
    assert!(!s.editor.can_undo());
    assert_eq!(s.editor.can_redo(), redo);
    codec(s, source);
}
fn assert_history(before: &Project, expected: &Project, selected: &[Frame], to: Frame) {
    let mut s = state(before, false);
    let views = s.capture_views();
    let mut input = pointer_input(&s, selected);
    let geometry = geometry(&s, 8.);
    let target = input.target(PAINT).unwrap();
    let pressed = *selected.last().unwrap();
    // Deliberately press off the glyph center. Only displacement is relevant.
    let origin = 103. + pressed as f32 * 8.;
    assert!(input.begin_pointer(&target, &s, pressed, origin, false, geometry));
    assert_neutral(&mut s, before, &views, false);
    let delta = i64::from(to) - i64::from(*selected.iter().min().unwrap());
    for fraction in [0.25, 0.5, 0.75] {
        assert!(input.update_pointer(&s, geometry, origin + delta as f32 * 8. * fraction, true));
        assert_neutral(&mut s, before, &views, false);
    }
    let mut commits = 0;
    assert!(input.finish_pointer(
        &mut s,
        geometry,
        origin + delta as f32 * 8.,
        true,
        false,
        |state, action| {
            assert!(
                matches!(
                    &action,
                    Action::Edit(Command::Contents {
                        edit: ContentsEdit::GradientColors {
                            edit: GradientColorsEdit::MoveKeys { .. },
                            ..
                        },
                        ..
                    })
                ),
                "release dispatches only existing compound group move"
            );
            commits += 1;
            state.bulk_test_action(&action);
            state.status == "Edited"
        }
    ));
    assert_eq!(commits, 1, "release commits exactly one transaction");
    assert_eq!(s.status, "Edited");
    assert_eq!(
        input.selected_frames(),
        selected
            .iter()
            .map(|f| Frame::try_from(i64::from(*f) + delta).unwrap())
            .collect()
    );
    assert!(!input.finish_pointer(
        &mut s,
        geometry,
        origin + delta as f32 * 8.,
        true,
        false,
        |_, _| panic!("duplicate release")
    ));
    exact(
        s.editor.project(),
        expected,
        "independent translated snapshots and modes",
    );
    assert_eq!(
        s.capture_views(),
        views,
        "drag never seeks or changes other VIEW state"
    );
    codec(&mut s, expected);
    s.bulk_test_action(&Action::Undo);
    exact(
        s.editor.project(),
        before,
        "one atomic Undo restores every key and mode",
    );
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
    s.bulk_test_action(&Action::Redo);
    exact(
        s.editor.project(),
        expected,
        "one atomic Redo restores translated source",
    );
    assert_eq!(s.capture_views(), views);
}

#[test]
fn pointer_move_commit_preserves_exact_snapshots_gaps_modes_and_history_all_four_paints() {
    let _evidence = PixelEvidence::new("committed_translation_all_four_paints");
    for stroke in [false, true] {
        for radial in [false, true] {
            let before = fixture(stroke, radial);
            for (selected, to) in [
                (&[0, 15][..], 15),     // Destination overlaps the selected old frame.
                (&[0, 45][..], 10),     // Truly noncontiguous: unselected key 15 remains.
                (&[15, 45][..], 5),     // Backward translation, preserving the 30-frame gap.
                (&[0, 15, 45][..], 74), // Rightmost key reaches final legal frame119.
            ] {
                let expected = literal_retime(stroke, radial, selected, to);
                assert_history(&before, &expected, selected, to);
                assert_render(&expected);
                assert_eq!(animation(&expected).keys().len(), 3);
                let mut a = serde_json::to_value(paint(&before)).unwrap();
                let mut b = serde_json::to_value(paint(&expected)).unwrap();
                gradient_wire(&mut a)
                    .as_object_mut()
                    .unwrap()
                    .remove("colors_animation");
                gradient_wire(&mut b)
                    .as_object_mut()
                    .unwrap()
                    .remove("colors_animation");
                assert_eq!(
                    a, b,
                    "dormant base, stop allocator and all scalar parameters unchanged"
                );
            }
        }
    }
}

#[test]
fn pointer_move_topology_order_and_dormant_modes_are_preserved_all_four_paints() {
    let _evidence = PixelEvidence::new("ordered_topology_hold_all_four_paints");
    for stroke in [false, true] {
        for radial in [false, true] {
            let before = topology_fixture(stroke, radial, false);
            let expected = topology_fixture(stroke, radial, true);
            assert_history(&before, &expected, &[0, 15], 15);
            for frame in [15, 30] {
                let segment = animation(&expected).segment_status(frame).unwrap();
                assert_eq!(segment.effective, GradientColorsInterpolation::Hold);
                assert_eq!(
                    segment.hold_reason,
                    Some(GradientColorsHoldReason::IncompatibleTopology)
                );
            }
            assert_eq!(
                animation(&expected).keys()[&30]
                    .opacities
                    .iter()
                    .map(|s| s.id)
                    .collect::<Vec<_>>(),
                [6, 3, 4]
            );
            assert_eq!(
                animation(&expected).interpolation(45),
                Some(GradientColorsInterpolation::Linear)
            );
            assert_render(&expected);
        }
    }
}

fn redo_sentinel(before: &Project) -> (EditorState, Project) {
    let mut s = state(before, false);
    s.bulk_test_action(&Action::Edit(Command::RenameLayer {
        id: 1,
        name: "Pointer Redo sentinel".into(),
    }));
    let redo = s.editor.project().clone();
    s.bulk_test_action(&Action::Undo);
    assert!(!s.editor.can_undo());
    assert!(s.editor.can_redo());
    (s, redo)
}
#[test]
fn pointer_move_core_rejects_collisions_bounds_missing_keys_and_lock_without_history_loss() {
    for stroke in [false, true] {
        for radial in [false, true] {
            let before = fixture(stroke, radial);
            let (mut s, redo) = redo_sentinel(&before);
            let view = s.capture_views();
            for (frames, to) in [
                (&[0, 15][..], 30), // The second key would occupy unselected45.
                (&[0, 45][..], 15), // The first key would occupy unselected15.
                (&[0, 45][..], 75), // The last destination would equal duration120.
                (&[15, 45][..], 100),
                (&[0, 15][..], Frame::MAX),
                (&[0, 7][..], 5),
                (&[][..], 0),
            ] {
                assert!(
                    s.editor
                        .execute(command(GradientColorsEdit::MoveKeys {
                            frames: frames.iter().copied().collect(),
                            to
                        }))
                        .is_err()
                );
                exact(s.editor.project(), &before, "rejection source atomicity");
                assert_eq!(s.capture_views(), view);
                assert!(!s.editor.can_undo());
                assert!(s.editor.can_redo());
            }
            s.bulk_test_action(&Action::Edit(command(GradientColorsEdit::MoveKeys {
                frames: [0, 45].into(),
                to: 0,
            })));
            exact(s.editor.project(), &before, "zero translation exact no-op");
            assert_eq!(s.capture_views(), view);
            assert!(!s.editor.can_undo());
            assert!(s.editor.can_redo());
            s.bulk_test_action(&Action::Redo);
            exact(
                s.editor.project(),
                &redo,
                "rejections and no-op retain complete Redo",
            );
            let locked = locked_fixture(stroke, radial);
            let mut locked_state = state(&locked, false);
            let locked_views = locked_state.capture_views();
            assert!(
                locked_state
                    .editor
                    .execute(command(GradientColorsEdit::MoveKeys {
                        frames: [0, 15].into(),
                        to: 15
                    }))
                    .is_err()
            );
            exact(locked_state.editor.project(), &locked, "locked paint");
            assert_eq!(locked_state.capture_views(), locked_views);
            assert!(!locked_state.editor.can_undo());
        }
    }
}

#[test]
fn pointer_move_independent_static_oracles_are_pixel_distinct_and_keep_unrelated_source() {
    let renderer = Renderer::new();
    for stroke in [false, true] {
        for radial in [false, true] {
            let seed = fixture(stroke, radial);
            let images: Vec<_> = [first(), second(), third()]
                .iter()
                .map(|c| renderer.render(&materialized(&seed, c), 0, WIDTH).unwrap())
                .collect();
            assert_ne!(images[0], images[1]);
            assert_ne!(images[0], images[2]);
            assert_ne!(images[1], images[2]);
            assert_ne!(
                renderer.render(&seed, 15, WIDTH).unwrap(),
                renderer
                    .render(&literal_retime(stroke, radial, &[0, 15], 15), 15, WIDTH)
                    .unwrap()
            );
        }
    }
    let mut e = Editor::default();
    e.replace_project(fixture(false, false)).unwrap();
    e.execute(Command::AddNull).unwrap();
    e.execute(Command::ImportAsset {
        content: Content::Image { png:"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==".into() },
        width:1., height:1., name:"Unrelated pointer asset sentinel".into(), folder:None, frame:None,
    }).unwrap();
    let mut wire = serde_json::to_value(e.project()).unwrap();
    let layers = wire["composition"]["layers"].as_array_mut().unwrap();
    if layers[0]["id"] != json!(1) {
        layers.swap(0, 1);
    }
    let mut node: ContentsNode = serde_json::from_value(paint_wire(&mut wire).clone()).unwrap();
    node.parameters.insert(ContentsParam::Gradient(GradientParam::EndX),serde_json::from_value(json!({"value":145.,"keys":{"0":{"value":145.,"interpolation":"Hold"},"119":{"value":175.,"interpolation":"Hold"}}})).unwrap());
    *paint_wire(&mut wire) = serde_json::to_value(node).unwrap();
    let before = Project::from_json(&wire.to_string()).unwrap();
    let expected = keyed(
        &before,
        &[(10, first()), (15, second()), (55, third())],
        &[
            (15, GradientColorsInterpolation::Linear),
            (55, GradientColorsInterpolation::Smooth),
        ],
        57,
    );
    assert_history(&before, &expected, &[0, 45], 10);
    assert_eq!(expected.asset_library().assets().len(), 1);
}

#[test]
fn pointer_provisional_cancellation_zero_motion_and_return_to_start_preserve_redo_and_view() {
    for stroke in [false, true] {
        for radial in [false, true] {
            let before = fixture(stroke, radial);
            for path in [vec![1., 1.], vec![4., 4.], vec![160., 0.], vec![0., 0.]] {
                let (mut s, redo) = redo_sentinel(&before);
                let views = s.capture_views();
                let mut input = pointer_input(&s, &[0, 45]);
                // 20px/frame makes a threshold-crossing 4px drag a zero-frame no-op.
                let geometry = geometry(&s, 20.);
                let target = input.target(PAINT).unwrap();
                assert!(input.begin_pointer(&target, &s, 0, 103., false, geometry));
                for dx in &path {
                    input.update_pointer(&s, geometry, 103. + dx, true);
                    assert_neutral(&mut s, &before, &views, true);
                }
                input.finish_pointer(
                    &mut s,
                    geometry,
                    103. + path.last().unwrap(),
                    true,
                    false,
                    |_, _| panic!("no-op must never dispatch"),
                );
                assert_neutral(&mut s, &before, &views, true);
                s.bulk_test_action(&Action::Redo);
                exact(
                    s.editor.project(),
                    &redo,
                    "complete Redo survives pointer no-op",
                );
            }
            for retirement in 0..3 {
                let (mut s, redo) = redo_sentinel(&before);
                let views = s.capture_views();
                let mut input = pointer_input(&s, &[0, 45]);
                let geometry = geometry(&s, 8.);
                let target = input.target(PAINT).unwrap();
                assert!(input.begin_pointer(&target, &s, 0, 103., false, geometry));
                input.update_pointer(&s, geometry, 183., true);
                assert_neutral(&mut s, &before, &views, true);
                let mut release_geometry = geometry;
                match retirement {
                    0 => {
                        assert!(input.cancel_pointer());
                    }
                    1 => {
                        release_geometry.bounds.size.width += gpui::px(1.);
                    }
                    _ => {
                        input.select_frames(&s, PAINT, [15].into());
                    }
                }
                input.finish_pointer(&mut s, release_geometry, 183., true, false, |_, _| {
                    panic!("retired gesture cannot commit")
                });
                assert_neutral(&mut s, &before, &views, true);
                s.bulk_test_action(&Action::Redo);
                exact(
                    s.editor.project(),
                    &redo,
                    "complete Redo survives pointer cancellation",
                );
            }
        }
    }
}

#[test]
fn pointer_drop_collision_is_atomic_and_common_bounds_clamp_without_crushing_spacing() {
    let before = fixture(false, false);
    let (mut s, redo) = redo_sentinel(&before);
    let views = s.capture_views();
    let mut input = pointer_input(&s, &[0, 15]);
    let mapping = geometry(&s, 8.);
    let target = input.target(PAINT).unwrap();
    assert!(input.begin_pointer(&target, &s, 15, 223., false, mapping));
    input.update_pointer(&s, mapping, 463., true);
    assert_neutral(&mut s, &before, &views, true);
    input.finish_pointer(&mut s, mapping, 463., true, false, |_, _| {
        panic!("colliding drop cannot dispatch")
    });
    assert_neutral(&mut s, &before, &views, true);
    s.bulk_test_action(&Action::Redo);
    exact(
        s.editor.project(),
        &redo,
        "collision preserves complete Redo",
    );

    let mut s = state(&before, false);
    let views = s.capture_views();
    let mut input = pointer_input(&s, &[0, 15, 45]);
    let mapping = geometry(&s, 8.);
    let target = input.target(PAINT).unwrap();
    assert!(input.begin_pointer(&target, &s, 0, 103., false, mapping));
    input.update_pointer(&s, mapping, 99999., true);
    assert_neutral(&mut s, &before, &views, false);
    let mut commits = 0;
    input.finish_pointer(&mut s, mapping, 99999., true, false, |state, action| {
        commits += 1;
        state.bulk_test_action(&action);
        state.status == "Edited"
    });
    assert_eq!(commits, 1);
    exact(
        s.editor.project(),
        &literal_retime(false, false, &[0, 15, 45], 74),
        "one common right bound delta",
    );
    assert_eq!(input.selected_frames(), [74, 89, 119].into());
    assert_eq!(s.capture_views(), views);
    s.bulk_test_action(&Action::Undo);
    exact(
        s.editor.project(),
        &before,
        "one Undo restores bound-clamped group",
    );
    assert!(!s.editor.can_undo());
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
    assert!(
        !label.is_empty()
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    );
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
#[ignore = "requires an explicit absolute immutable fixture directory"]
fn export_gradient_pointer_acceptance_fixtures() {
    let root = std::env::var_os("LIBREEFFECTS_EXPORT_GRADIENT_POINTER_FIXTURES")
        .expect("set fixture directory");
    let root = Path::new(&root);
    let before = fixture(false, false);
    let views = state(&before, true).capture_views();
    assert!(
        views
            .compositions
            .contains_key(&before.active_composition_id()),
        "export includes explicit active VIEW defaults"
    );
    for (label, project) in [
        ("native-before", before.clone()),
        (
            "native-overlap15-expected",
            literal_retime(false, false, &[0, 15], 15),
        ),
        (
            "native-noncontiguous10-expected",
            literal_retime(false, false, &[0, 45], 10),
        ),
        (
            "native-backward5-expected",
            literal_retime(false, false, &[15, 45], 5),
        ),
        (
            "native-all10-expected",
            literal_retime(false, false, &[0, 15, 45], 10),
        ),
        (
            "native-bound119-expected",
            literal_retime(false, false, &[0, 15, 45], 74),
        ),
        ("native-locked", locked_fixture(false, false)),
        (
            "native-topology-before",
            topology_fixture(false, false, false),
        ),
        (
            "native-topology-overlap15-expected",
            topology_fixture(false, false, true),
        ),
    ] {
        export(root, label, &project, &views);
    }
    let mut extra_cases = vec![];
    // Native coordinates may resolve to another whole frame. Declare that frame
    // and selected source set explicitly, before the verifier reads the actual
    // save. A NEW output root avoids mutating any previous manifest/reference.
    if let Some(path) = std::env::var_os("LIBREEFFECTS_GRADIENT_POINTER_EXTRA_CASES") {
        let bytes = std::fs::read(Path::new(&path)).unwrap();
        let cases: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        for case in cases {
            let name = case["name"].as_str().expect("case name");
            let stroke = case["stroke"].as_bool().expect("explicit stroke boolean");
            let radial = case["radial"].as_bool().expect("explicit radial boolean");
            let selected: Vec<Frame> =
                serde_json::from_value(case["selected_frames"].clone()).unwrap();
            let to: Frame = serde_json::from_value(case["to"].clone()).unwrap();
            let expected = literal_retime(stroke, radial, &selected, to);
            export(root, &format!("native-{name}-expected"), &expected, &views);
            extra_cases.push(case);
        }
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
                ("overlap15", literal_retime(stroke, radial, &[0, 15], 15)),
                (
                    "noncontiguous10",
                    literal_retime(stroke, radial, &[0, 45], 10),
                ),
                ("topology-overlap15", topology_fixture(stroke, radial, true)),
            ] {
                let name = format!("{paint}-{action}");
                export(root, &name, &project, &views);
                for frame in FRAMES {
                    let reference = format!("{name}-static-frame-{frame:03}");
                    export(
                        root,
                        &reference,
                        &materialized(&project, &reference_sample(&project, frame)),
                        &views,
                    );
                    render_cases.push(json!({"input":format!("{name}.generated.lep"),"expected":format!("{reference}.generated.lep"),"frame":frame,"width":WIDTH,"height":HEIGHT,"comparison":"exact full RGBA, no masks"}));
                }
            }
        }
    }
    let manifest = json!({
        "fixture_kind":"generated immutable inputs and independent literal expectations; never native interaction evidence",
        "dimensions":[WIDTH,HEIGHT], "frames":FRAMES, "duration":120, "native_initial_frame":0, "paint_item":PAINT,
        "schema":"Existing schema57/LEP1/VIEW; no source format or render changes",
        "native_cases":[
            {"label":"selected-old-overlap","start":"native-before","action":"Select0/15; plain press selected15 and drag group+15 with Alt if needed; no seek on drag; save after one-step Undo/Redo","expected":"native-overlap15-expected"},
            {"label":"noncontiguous","start":"native-before","action":"Select0/45 with Shift; drag group+10 while unselected15 stays put; preserve45-frame gap","expected":"native-noncontiguous10-expected"},
            {"label":"collision","start":"native-before","action":"Select0/15; drag+30 exactly to collide with unselected45; full drop rejects","expected":"native-before"},
            {"label":"no-op-redo","start":"native-before","action":"Create Redo by successful drag then Undo; move away and back to zero horizontal delta or vertical-only; Redo remains","expected":"native-before"},
            {"label":"shift-click","start":"native-before","action":"Shift-click toggles membership, Shift-drag remains selection-only; ordinary click selects a single key on release","expected":"native-before"},
            {"label":"bounds","start":"native-before","action":"Select0/15/45; drag beyond composition right boundary; common delta clamps at74, keys74/89/119","expected":"native-bound119-expected"},
            {"label":"locked","start":"native-locked","action":"Attempt compound key drag on locked layer; source unchanged","expected":"native-locked"},
            {"label":"topology","start":"native-topology-before","action":"Select0/15 and drag+15; ordered mismatch Hold remains","expected":"native-topology-overlap15-expected"},
            {"label":"reopen","start":"actual saved native file","action":"New/Open actual save/Save As; match independent complete source and full VIEW","expected":"same explicit independent source"}
        ],
        "extra_explicit_native_cases":extra_cases,
        "native_verifier":{"test":"verify_recorded_native_gradient_pointer_save","actual_env":"LIBREEFFECTS_GRADIENT_POINTER_NATIVE_SAVE","expected_env":"LIBREEFFECTS_GRADIENT_POINTER_EXPECTED","optional_frame_env":"LIBREEFFECTS_GRADIENT_POINTER_EXPECTED_FRAME","rules":"Only explicit expected frame override. Full source and full VIEW equality, no masks or source sampling from actual."},
        "render_cases":render_cases
    });
    write_immutable(
        &root.join("cases.json"),
        serde_json::to_string_pretty(&manifest).unwrap().as_bytes(),
    );
}

#[test]
#[ignore = "requires separately recorded actual native save and independent expected fixture"]
fn verify_recorded_native_gradient_pointer_save() {
    let _evidence = PixelEvidence::new("native_full_source_view_five_route_static_oracle");
    let actual_path = std::env::var_os("LIBREEFFECTS_GRADIENT_POINTER_NATIVE_SAVE")
        .expect("set actual native path");
    let expected_path = std::env::var_os("LIBREEFFECTS_GRADIENT_POINTER_EXPECTED")
        .expect("set independent expected LEP");
    assert_ne!(
        Path::new(&actual_path),
        Path::new(&expected_path),
        "actual GUI saves and references must be separate"
    );
    let actual_bytes = std::fs::read(Path::new(&actual_path)).unwrap();
    assert_eq!(
        &actual_bytes[8..10],
        &[1, 0],
        "actual unchanged LEP container version"
    );
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_GRADIENT_POINTER_EXPECTED_FRAME") {
        let frame: Frame = frame.parse().expect("unsigned explicitly expected frame");
        assert!(frame < expected.project.composition().duration());
        expected
            .views
            .compositions
            .entry(expected.project.active_composition_id())
            .or_default()
            .frame = frame;
    }
    // Read actual source only after the independent expectation is complete.
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    assert_eq!(actual.format, crate::project_io::ProjectFormat::Lep);
    exact(
        &actual.project,
        &expected.project,
        "actual native complete source",
    );
    assert_eq!(
        actual.views, expected.views,
        "actual native complete VIEW including defaults; no masks"
    );
    for frame in FRAMES {
        compare_paths(
            &actual.project,
            frame,
            &materialized(
                &expected.project,
                &reference_sample(&expected.project, frame),
            ),
        );
    }
    assert_eq!(
        std::fs::read(Path::new(&actual_path)).unwrap(),
        actual_bytes,
        "verification never changes actual save"
    );
    println!(
        "Verified complete native source/VIEW and {} five-route exact RGBA frame sets: {}",
        FRAMES.len(),
        Path::new(&actual_path).display()
    );
}
