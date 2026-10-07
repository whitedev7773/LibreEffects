//! Independent E04 interpolation acceptance. Expected samples use explicit
//! arithmetic over literal snapshots; expected pixels use legacy static stop
//! tracks, never the production compound sampler. Generated inputs are immutable
//! codec fixtures and are not evidence of native interaction.
use crate::{
    editor::{Action, EditorState, Tool},
    rendering::Renderer,
    view_state::{GraphChannel, GraphRanges, ProjectViews},
};
use libre_effects_core::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 240;
const FRAMES: [Frame; 9] = [0, 15, 16, 30, 45, 60, 74, 75, 89];
const FIRST: Frame = 15;
const LAST: Frame = 75;
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
                "GRADIENT_INTERPOLATION_PIXEL_EVIDENCE {}: {pairs} exact RGBA pairs / {} pixels",
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
fn last() -> GradientColors {
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
    let mix = |left: f64, right: f64| left + (right - left) * t;
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
        name: "Gradient interpolation acceptance".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::ShapeContents(Default::default()),
        width: WIDTH as f64,
        height: HEIGHT as f64,
        name: "Gradient Colors interpolation".into(),
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
fn fixture(stroke: bool, radial: bool, mode: GradientColorsInterpolation) -> Project {
    let modes = if mode == GradientColorsInterpolation::Hold {
        vec![]
    } else {
        vec![(FIRST, mode)]
    };
    keyed(
        &scene(stroke, radial),
        &[(FIRST, first()), (LAST, last())],
        &modes,
        if modes.is_empty() { 54 } else { 57 },
    )
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
fn assert_samples(
    project: &Project,
    a: &GradientColors,
    b: &GradientColors,
    mode: GradientColorsInterpolation,
    start: Frame,
    end: Frame,
) {
    for frame in FRAMES {
        let expected = expected_sample(a, b, mode, frame, start, end);
        assert_eq!(
            sample(project, frame),
            expected,
            "all stop fields/ordered identities frame {frame}"
        );
        compare_paths(project, frame, &materialized(project, &expected));
    }
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

#[test]
fn linear_and_smooth_all_stop_fields_match_literal_legacy_fill_stroke_linear_radial() {
    let _evidence = PixelEvidence::new("linear_smooth_all_paints");
    for mode in [
        GradientColorsInterpolation::Linear,
        GradientColorsInterpolation::Smooth,
    ] {
        for stroke in [false, true] {
            for radial in [false, true] {
                let p = fixture(stroke, radial, mode);
                assert_samples(&p, &first(), &last(), mode, FIRST, LAST);
                let quarter = sample(&p, 30);
                let (position, red, midpoint, opacity) =
                    if mode == GradientColorsInterpolation::Linear {
                        (25., 184., 25.5, 80.)
                    } else {
                        (15.625, 205., 16.3125, 87.5)
                    };
                assert_eq!(quarter.colors[0].position, position);
                assert_eq!(quarter.colors[0].red, red);
                assert_eq!(quarter.colors[0].midpoint, midpoint);
                assert_eq!(quarter.opacities[0].opacity, opacity);
                assert_eq!(
                    sample(&p, FIRST).colors[0].position.to_bits(),
                    (-0.0f64).to_bits()
                );
                let middle = sample(&p, 45);
                assert!(middle.colors.iter().all(|s| s.position == 50.));
                assert!(middle.opacities.iter().all(|s| s.position == 50.));
                assert_eq!(
                    middle.colors.iter().map(|s| s.id).collect::<Vec<_>>(),
                    vec![1, 5, 2]
                );
                assert_eq!(
                    middle.opacities.iter().map(|s| s.id).collect::<Vec<_>>(),
                    vec![3, 6, 4]
                );
                assert_eq!(
                    animation(&p).segment_at(0).unwrap().hold_reason,
                    Some(GradientColorsHoldReason::BeforeFirstKey)
                );
                assert_eq!(
                    animation(&p).segment_status(LAST).unwrap().hold_reason,
                    Some(GradientColorsHoldReason::NoNextKey)
                );
            }
        }
    }
}

#[test]
fn incompatible_color_or_opacity_topologies_explicitly_hold_without_matching_by_position() {
    let _evidence = PixelEvidence::new("six_topology_holds");
    let mut variants = vec![];
    let mut c = last();
    c.colors.remove(1);
    variants.push(("color-removed", c));
    let mut c = last();
    let mut stop = c.colors[1].clone();
    stop.id = 7;
    c.colors.push(stop);
    variants.push(("color-added", c));
    let mut c = last();
    c.colors.swap(0, 1);
    variants.push(("color-reordered", c));
    let mut c = last();
    c.opacities.remove(1);
    variants.push(("opacity-removed", c));
    let mut c = last();
    let mut stop = c.opacities[1].clone();
    stop.id = 8;
    c.opacities.push(stop);
    variants.push(("opacity-added", c));
    let mut c = last();
    c.opacities.swap(0, 1);
    variants.push(("opacity-reordered", c));
    for (label, endpoint) in variants {
        let p = keyed(
            &scene(false, false),
            &[(FIRST, first()), (LAST, endpoint.clone())],
            &[(FIRST, GradientColorsInterpolation::Smooth)],
            57,
        );
        let status = animation(&p).segment_status(FIRST).unwrap();
        assert_eq!(
            status.interpolation,
            GradientColorsInterpolation::Smooth,
            "{label}"
        );
        assert_eq!(
            status.effective,
            GradientColorsInterpolation::Hold,
            "{label}"
        );
        assert_eq!(
            status.hold_reason,
            Some(GradientColorsHoldReason::IncompatibleTopology),
            "{label}"
        );
        assert_samples(
            &p,
            &first(),
            &endpoint,
            GradientColorsInterpolation::Hold,
            FIRST,
            LAST,
        );
        assert!(
            p.to_json().unwrap().contains("Smooth"),
            "fallback does not erase requested mode"
        );
    }
}

#[test]
fn different_color_and_opacity_row_counts_interpolate_independently() {
    let _evidence = PixelEvidence::new("independent_unequal_rows");
    for remove_color in [false, true] {
        let mut a = first();
        let mut b = last();
        if remove_color {
            a.colors.remove(1);
            b.colors.remove(1);
        } else {
            a.opacities.remove(1);
            b.opacities.remove(1);
        }
        for mode in [
            GradientColorsInterpolation::Linear,
            GradientColorsInterpolation::Smooth,
        ] {
            let p = keyed(
                &scene(false, true),
                &[(FIRST, a.clone()), (LAST, b.clone())],
                &[(FIRST, mode)],
                57,
            );
            assert_ne!(a.colors.len(), a.opacities.len());
            assert_samples(&p, &a, &b, mode, FIRST, LAST);
        }
    }
}

#[test]
fn crossing_tie_order_and_signed_zero_endpoints_have_independent_static_oracles() {
    let _evidence = PixelEvidence::new("signed_zero_tie_order");
    let mut a = first();
    let mut b = last();
    for s in &mut a.colors {
        s.position = 0.;
    }
    a.colors[0].position = -0.;
    for s in &mut b.colors {
        s.position = 0.;
    }
    b.colors[2].position = -0.;
    for s in &mut a.opacities {
        s.position = 0.;
    }
    a.opacities[2].position = -0.;
    for s in &mut b.opacities {
        s.position = 0.;
    }
    b.opacities[0].position = -0.;
    let p = keyed(
        &scene(true, true),
        &[(FIRST, a.clone()), (LAST, b.clone())],
        &[(FIRST, GradientColorsInterpolation::Linear)],
        57,
    );
    assert_samples(&p, &a, &b, GradientColorsInterpolation::Linear, FIRST, LAST);
    assert_eq!(sample(&p, FIRST), a);
    assert_eq!(sample(&p, LAST), b);
    assert_ne!(
        sample(&p, FIRST).colors[0].position.to_bits(),
        sample(&p, LAST).colors[0].position.to_bits()
    );
}

#[test]
fn bit_identical_signed_zero_snapshots_stay_constant_for_linear_and_smooth() {
    let _evidence = PixelEvidence::new("identical_signed_zero_snapshots");
    let mut c = first();
    // Row storage deliberately differs from total_cmp spatial order. Losing the
    // negative zero changes which RGB/opacity value wins the coincident ramp.
    c.colors[0].position = 0.;
    c.colors[1].position = -0.;
    c.colors[2].position = 100.;
    c.opacities[0].position = 0.;
    c.opacities[1].position = -0.;
    c.opacities[2].position = 100.;
    for mode in [
        GradientColorsInterpolation::Linear,
        GradientColorsInterpolation::Smooth,
    ] {
        let p = keyed(
            &scene(false, false),
            &[(FIRST, c.clone()), (LAST, c.clone())],
            &[(FIRST, mode)],
            57,
        );
        let static_oracle = materialized(&p, &c);
        let mut lossy = c.clone();
        lossy.colors[1].position = 0.;
        lossy.opacities[1].position = 0.;
        assert_ne!(
            Renderer::new().render(&static_oracle, 30, WIDTH).unwrap(),
            Renderer::new()
                .render(&materialized(&p, &lossy), 30, WIDTH)
                .unwrap(),
            "negative control proves the signed-zero tie affects these pixels"
        );
        for frame in FRAMES {
            assert_eq!(
                sample(&p, frame),
                c,
                "constant whole snapshot frame {frame}"
            );
            compare_paths(&p, frame, &static_oracle);
        }
    }
}

#[test]
fn clipboard_retime_and_layer_shift_move_sparse_modes_with_exact_snapshots() {
    let _evidence = PixelEvidence::new("retime_and_shift");
    let source = fixture(false, false, GradientColorsInterpolation::Smooth);
    let mut from = Editor::default();
    from.replace_project(source.clone()).unwrap();
    let clipboard = from.copy_layers(&[1]).unwrap();
    let mut to = Editor::default();
    to.execute(Command::ConfigureComposition {
        name: "Retime destination".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 60,
        duration: 180,
    })
    .unwrap();
    to.clear_history();
    let empty = to.project().clone();
    // Full expected destination is literal source layer insertion plus explicit
    // 30 -> 60 fps frame mapping; production retiming is never an oracle.
    let mut expected = serde_json::to_value(&empty).unwrap();
    expected["version"] = json!(57);
    expected["next_layer_id"] = json!(2);
    let mut layer = serde_json::to_value(source.composition().layer(1).unwrap()).unwrap();
    layer["out_frame"] = json!(180);
    expected["composition"]["layers"] = json!([layer]);
    gradient_wire(paint_wire(&mut expected))["colors_animation"] =
        json!({"keys":{"30":first(),"150":last()},"outgoing_interpolation":{"30":"Smooth"}});
    let expected = Project::from_json(&expected.to_string()).unwrap();
    to.execute(Command::PasteLayers(clipboard)).unwrap();
    exact(to.project(), &expected, "retimed paste full source");
    to.undo();
    exact(to.project(), &empty, "paste one Undo");
    assert!(!to.can_undo());
    to.redo();
    exact(to.project(), &expected, "paste Redo");
    for frame in FRAMES {
        let c = expected_sample(
            &first(),
            &last(),
            GradientColorsInterpolation::Smooth,
            frame,
            FIRST,
            LAST,
        );
        assert_eq!(sample(&expected, frame * 2), c);
        compare_paths(&expected, frame * 2, &materialized(&expected, &c));
    }
    let mut wire = serde_json::to_value(&source).unwrap();
    wire["composition"]["layers"][0]["out_frame"] = json!(80);
    let before = Project::from_json(&wire.to_string()).unwrap();
    wire["composition"]["layers"][0]["in_frame"] = json!(5);
    wire["composition"]["layers"][0]["out_frame"] = json!(85);
    gradient_wire(paint_wire(&mut wire))["colors_animation"] =
        json!({"keys":{"20":first(),"80":last()},"outgoing_interpolation":{"20":"Smooth"}});
    let expected = Project::from_json(&wire.to_string()).unwrap();
    let mut e = Editor::default();
    e.replace_project(before.clone()).unwrap();
    e.clear_history();
    e.execute(Command::ShiftLayer { id: 1, delta: 5 }).unwrap();
    exact(e.project(), &expected, "layer shift full source");
    e.undo();
    exact(e.project(), &before, "shift Undo");
    e.redo();
    exact(e.project(), &expected, "shift Redo");
    assert_samples(
        &expected,
        &first(),
        &last(),
        GradientColorsInterpolation::Smooth,
        20,
        80,
    );
}

#[test]
fn interpolation_commands_preserve_full_dormant_source_view_and_one_step_history() {
    for stroke in [false, true] {
        let before = fixture(stroke, false, GradientColorsInterpolation::Hold);
        for mode in [
            GradientColorsInterpolation::Linear,
            GradientColorsInterpolation::Smooth,
        ] {
            let expected = fixture(stroke, false, mode);
            apply_history(
                &before,
                &expected,
                GradientColorsEdit::SetInterpolation {
                    frame: FIRST,
                    interpolation: mode,
                },
            );
            let hold57 = keyed(
                &scene(stroke, false),
                &[(FIRST, first()), (LAST, last())],
                &[],
                57,
            );
            apply_history(
                &expected,
                &hold57,
                GradientColorsEdit::SetInterpolation {
                    frame: FIRST,
                    interpolation: GradientColorsInterpolation::Hold,
                },
            );
            assert!(!hold57.to_json().unwrap().contains("outgoing_interpolation"));
        }
    }
}

#[test]
fn delete_move_disable_and_add_sample_have_literal_metadata_and_history() {
    let _evidence = PixelEvidence::new("edit_sample_render");
    let before = fixture(false, true, GradientColorsInterpolation::Smooth);
    let moved = keyed(
        &scene(false, true),
        &[(30, first()), (LAST, last())],
        &[(30, GradientColorsInterpolation::Smooth)],
        57,
    );
    apply_history(
        &before,
        &moved,
        GradientColorsEdit::MoveKey {
            from: FIRST,
            to: 30,
        },
    );
    assert_samples(
        &moved,
        &first(),
        &last(),
        GradientColorsInterpolation::Smooth,
        30,
        LAST,
    );
    let removed = keyed(&scene(false, true), &[(LAST, last())], &[], 57);
    apply_history(
        &before,
        &removed,
        GradientColorsEdit::DeleteKey { frame: FIRST },
    );
    let terminal = keyed(
        &scene(false, true),
        &[(FIRST, first())],
        &[(FIRST, GradientColorsInterpolation::Smooth)],
        57,
    );
    apply_history(
        &before,
        &terminal,
        GradientColorsEdit::DeleteKey { frame: LAST },
    );
    assert_eq!(
        animation(&terminal)
            .segment_status(FIRST)
            .unwrap()
            .hold_reason,
        Some(GradientColorsHoldReason::NoNextKey)
    );
    let baked = materialized(&terminal, &first());
    apply_history(
        &terminal,
        &baked,
        GradientColorsEdit::DeleteKey { frame: FIRST },
    );
    let middle = expected_sample(
        &first(),
        &last(),
        GradientColorsInterpolation::Smooth,
        45,
        FIRST,
        LAST,
    );
    let disabled = materialized(&before, &middle);
    apply_history(
        &before,
        &disabled,
        GradientColorsEdit::SetAnimation {
            frame: 45,
            enabled: false,
        },
    );
    for frame in FRAMES {
        compare_paths(&disabled, frame, &materialized(&before, &middle));
    }
    let quarter = expected_sample(
        &first(),
        &last(),
        GradientColorsInterpolation::Smooth,
        30,
        FIRST,
        LAST,
    );
    let added = keyed(
        &scene(false, true),
        &[(FIRST, first()), (30, quarter), (LAST, last())],
        &[(FIRST, GradientColorsInterpolation::Smooth)],
        57,
    );
    apply_history(&before, &added, GradientColorsEdit::ToggleKey { frame: 30 });
}

#[test]
fn terminal_mode_stays_dormant_until_a_later_key_is_added() {
    let _evidence = PixelEvidence::new("dormant_terminal_mode");
    let before = fixture(true, false, GradientColorsInterpolation::Smooth);
    let terminal = keyed(
        &scene(true, false),
        &[(FIRST, first()), (LAST, last())],
        &[
            (FIRST, GradientColorsInterpolation::Smooth),
            (LAST, GradientColorsInterpolation::Linear),
        ],
        57,
    );
    apply_history(
        &before,
        &terminal,
        GradientColorsEdit::SetInterpolation {
            frame: LAST,
            interpolation: GradientColorsInterpolation::Linear,
        },
    );
    assert_eq!(
        animation(&terminal).interpolation(LAST),
        Some(GradientColorsInterpolation::Linear)
    );
    assert_eq!(
        animation(&terminal).segment_status(LAST).unwrap().effective,
        GradientColorsInterpolation::Hold
    );
    for frame in [75, 76, 80, 89] {
        assert_eq!(sample(&terminal, frame), last());
        compare_paths(&terminal, frame, &materialized(&terminal, &last()));
    }
    let next = keyed(
        &scene(true, false),
        &[(FIRST, first()), (LAST, last()), (85, first())],
        &[
            (FIRST, GradientColorsInterpolation::Smooth),
            (LAST, GradientColorsInterpolation::Linear),
        ],
        57,
    );
    apply_history(
        &terminal,
        &next,
        GradientColorsEdit::Set {
            frame: 85,
            colors: first(),
        },
    );
    assert_eq!(
        animation(&next).segment_status(LAST).unwrap().effective,
        GradientColorsInterpolation::Linear
    );
    let expected = expected_sample(
        &last(),
        &first(),
        GradientColorsInterpolation::Linear,
        80,
        75,
        85,
    );
    assert_eq!(sample(&next, 80), expected);
    compare_paths(&next, 80, &materialized(&next, &expected));
}

#[test]
fn strict_stale_actions_and_equal_actions_preserve_redo_source_and_view() {
    let before = fixture(false, false, GradientColorsInterpolation::Smooth);
    let mut s = state(&before, false);
    let views = s.capture_views();
    s.bulk_test_action(&Action::Edit(Command::RenameLayer {
        id: 1,
        name: "Redo sentinel".into(),
    }));
    let renamed = s.editor.project().clone();
    s.bulk_test_action(&Action::Undo);
    for edit in [
        GradientColorsEdit::SetInterpolation {
            frame: FIRST,
            interpolation: GradientColorsInterpolation::Smooth,
        },
        GradientColorsEdit::MoveKey {
            from: FIRST,
            to: FIRST,
        },
        GradientColorsEdit::Set {
            frame: FIRST,
            colors: first(),
        },
    ] {
        s.bulk_test_action(&Action::Edit(command(edit)));
        exact(s.editor.project(), &before, "equal source action");
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        assert_eq!(s.capture_views(), views);
    }
    for edit in [
        GradientColorsEdit::DeleteKey { frame: 30 },
        GradientColorsEdit::SetInterpolation {
            frame: 30,
            interpolation: GradientColorsInterpolation::Linear,
        },
        GradientColorsEdit::MoveKey { from: 30, to: 45 },
        GradientColorsEdit::MoveKey {
            from: FIRST,
            to: LAST,
        },
    ] {
        assert!(s.editor.execute(command(edit)).is_err());
        exact(s.editor.project(), &before, "stale strict action");
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        assert_eq!(s.capture_views(), views);
    }
    s.bulk_test_action(&Action::Redo);
    exact(s.editor.project(), &renamed, "Redo retained");
}

#[test]
fn legacy_static_and_hold_documents_remain_sparse_exact_and_schema57_is_narrow() {
    let _evidence = PixelEvidence::new("legacy_sparse_unchanged");
    for stroke in [false, true] {
        for radial in [false, true] {
            let static_source = scene(stroke, radial);
            assert!(
                !static_source
                    .to_json()
                    .unwrap()
                    .contains("colors_animation")
            );
            assert!(
                !static_source
                    .to_json()
                    .unwrap()
                    .contains("outgoing_interpolation")
            );
            let hold = fixture(stroke, radial, GradientColorsInterpolation::Hold);
            assert_eq!(serde_json::to_value(&hold).unwrap()["version"], json!(54));
            assert!(!hold.to_json().unwrap().contains("outgoing_interpolation"));
            assert_samples(
                &hold,
                &first(),
                &last(),
                GradientColorsInterpolation::Hold,
                FIRST,
                LAST,
            );
            let mut s = state(&static_source, false);
            codec(&mut s, &static_source);
            let mut s = state(&hold, false);
            codec(&mut s, &hold);
            s.bulk_test_action(&Action::Edit(command(
                GradientColorsEdit::SetInterpolation {
                    frame: FIRST,
                    interpolation: GradientColorsInterpolation::Hold,
                },
            )));
            exact(
                s.editor.project(),
                &hold,
                "implicit Hold remains schema54 and sparse",
            );
            assert!(!s.editor.can_undo());
            for version in [54, 55, 56] {
                let mut wire = serde_json::to_value(fixture(
                    stroke,
                    radial,
                    GradientColorsInterpolation::Linear,
                ))
                .unwrap();
                wire["version"] = json!(version);
                assert!(Project::from_json(&wire.to_string()).is_err());
                let unchecked: Project = serde_json::from_value(wire).unwrap();
                assert!(project_file::encode(&unchecked, None).is_err());
            }
        }
    }
    for malformed in [json!({"15":"Hold"}), json!({"30":"Linear"})] {
        let mut wire =
            serde_json::to_value(fixture(false, false, GradientColorsInterpolation::Smooth))
                .unwrap();
        gradient_wire(paint_wire(&mut wire))["colors_animation"]["outgoing_interpolation"] =
            malformed;
        assert!(Project::from_json(&wire.to_string()).is_err());
    }
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
fn export_gradient_interpolation_acceptance_fixtures() {
    let root = std::env::var_os("LIBREEFFECTS_EXPORT_GRADIENT_INTERPOLATION_FIXTURES")
        .expect("set fixture directory");
    let root = Path::new(&root);
    let before = fixture(false, false, GradientColorsInterpolation::Hold);
    let views = state(&before, true).capture_views();
    export(root, "native-before", &before, &views);
    let linear = fixture(false, false, GradientColorsInterpolation::Linear);
    let smooth = fixture(false, false, GradientColorsInterpolation::Smooth);
    export(root, "native-linear-expected", &linear, &views);
    export(root, "native-smooth-expected", &smooth, &views);
    let mut incompatible_colors = last();
    incompatible_colors.colors.swap(0, 1);
    let incompatible = keyed(
        &scene(false, false),
        &[(FIRST, first()), (LAST, incompatible_colors)],
        &[(FIRST, GradientColorsInterpolation::Linear)],
        57,
    );
    export(root, "native-incompatible-before", &incompatible, &views);

    let hold57 = keyed(
        &scene(false, false),
        &[(FIRST, first()), (LAST, last())],
        &[],
        57,
    );
    export(root, "native-hold57-expected", &hold57, &views);
    let moved = keyed(
        &scene(false, false),
        &[(30, first()), (LAST, last())],
        &[(30, GradientColorsInterpolation::Smooth)],
        57,
    );
    export(root, "native-moved30-expected", &moved, &views);
    let deleted = keyed(&scene(false, false), &[(LAST, last())], &[], 57);
    export(root, "native-deleted15-expected", &deleted, &views);
    let terminal = keyed(
        &scene(false, false),
        &[(FIRST, first())],
        &[(FIRST, GradientColorsInterpolation::Smooth)],
        57,
    );
    export(root, "native-deleted75-expected", &terminal, &views);
    export(
        root,
        "native-deleted-final-expected",
        &materialized(&terminal, &first()),
        &views,
    );
    let sample45 = expected_sample(
        &first(),
        &last(),
        GradientColorsInterpolation::Smooth,
        45,
        FIRST,
        LAST,
    );
    export(
        root,
        "native-disabled45-expected",
        &materialized(&smooth, &sample45),
        &views,
    );
    let sample30 = expected_sample(
        &first(),
        &last(),
        GradientColorsInterpolation::Smooth,
        30,
        FIRST,
        LAST,
    );
    let added = keyed(
        &scene(false, false),
        &[(FIRST, first()), (30, sample30), (LAST, last())],
        &[(FIRST, GradientColorsInterpolation::Smooth)],
        57,
    );
    export(root, "native-added30-expected", &added, &views);
    let mut render_cases = vec![];
    for stroke in [false, true] {
        for radial in [false, true] {
            let label = format!(
                "{}-{}",
                if stroke { "stroke" } else { "fill" },
                if radial { "radial" } else { "linear" }
            );
            let legacy = scene(stroke, radial);
            export(root, &format!("legacy-{label}"), &legacy, &views);
            for (mode, mode_name) in [
                (GradientColorsInterpolation::Hold, "hold"),
                (GradientColorsInterpolation::Linear, "linear"),
                (GradientColorsInterpolation::Smooth, "smooth"),
            ] {
                let p = fixture(stroke, radial, mode);
                let name = format!("{label}-{mode_name}");
                export(root, &name, &p, &views);
                for frame in FRAMES {
                    let expected = expected_sample(&first(), &last(), mode, frame, FIRST, LAST);
                    let reference = materialized(&p, &expected);
                    let reference_name = format!("{name}-legacy-frame-{frame:03}");
                    export(root, &reference_name, &reference, &views);
                    render_cases.push(json!({"input":format!("{name}.generated.lep"),"expected":format!("{reference_name}.generated.lep"),"frame":frame,"width":WIDTH,"height":HEIGHT,"comparison":"exact full RGBA, no masks"}));
                }
            }
        }
    }
    let manifest = json!({"fixture_kind":"generated input and independent expected sources; not native evidence","dimensions":[WIDTH,HEIGHT],"frames":FRAMES,"native_initial_frame":FIRST,"paint_item":PAINT,"native_cases":[
        {"label":"linear","start":"native-before","action":"Set outgoing key 15 to Linear","expected":"native-linear-expected"},
        {"label":"smooth","start":"native-linear-expected","action":"Set outgoing key 15 to Smooth","expected":"native-smooth-expected"},
        {"label":"undo","start":"native-smooth-expected","action":"Undo the Smooth change","expected":"native-linear-expected"},
        {"label":"redo","start":"after Undo","action":"Redo the Smooth change","expected":"native-smooth-expected"},
        {"label":"hold","start":"native-smooth-expected","action":"Set outgoing key 15 to Hold","expected":"native-hold57-expected"},
        {"label":"move","start":"native-smooth-expected","action":"Move key 15 to frame 30","expected":"native-moved30-expected"},
        {"label":"delete-first","start":"native-smooth-expected","action":"Delete key 15","expected":"native-deleted15-expected"},
        {"label":"delete-last","start":"native-smooth-expected","action":"Delete key 75","expected":"native-deleted75-expected"},
        {"label":"delete-final","start":"native-deleted75-expected","action":"Delete final key 15","expected":"native-deleted-final-expected"},
        {"label":"disable","start":"native-smooth-expected","action":"At frame 45 disable Colors animation","expected":"native-disabled45-expected"},
        {"label":"add","start":"native-smooth-expected","action":"At frame 30 add a Colors key; new outgoing mode defaults Hold","expected":"native-added30-expected"},
        {"label":"reopen","start":"any recorded native save","action":"New, Open actual native save, resave separately","expected":"same independently declared source and VIEW"}
    ],"native_verifier":{"test":"verify_recorded_native_gradient_interpolation_save","actual_env":"LIBREEFFECTS_GRADIENT_INTERPOLATION_NATIVE_SAVE","expected_env":"LIBREEFFECTS_GRADIENT_INTERPOLATION_EXPECTED","optional_frame_env":"LIBREEFFECTS_GRADIENT_INTERPOLATION_EXPECTED_FRAME","rules":"Full source + full VIEW equality. Only explicitly specified expected frame may change. Originals immutable; actual saves in separate directory."},"render_cases":render_cases});
    write_immutable(
        &root.join("cases.json"),
        serde_json::to_string_pretty(&manifest).unwrap().as_bytes(),
    );
}
#[test]
#[ignore = "requires separately recorded native save and independent expected fixture"]
fn verify_recorded_native_gradient_interpolation_save() {
    let _evidence = PixelEvidence::new("actual_native_full_source_view");
    let actual_path = std::env::var_os("LIBREEFFECTS_GRADIENT_INTERPOLATION_NATIVE_SAVE")
        .expect("set actual native save path");
    let expected_path = std::env::var_os("LIBREEFFECTS_GRADIENT_INTERPOLATION_EXPECTED")
        .expect("set independent reference LEP");
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_GRADIENT_INTERPOLATION_EXPECTED_FRAME") {
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
    exact(&actual.project, &expected.project, "native full source");
    assert_eq!(actual.views, expected.views, "native full VIEW; no masks");
    for frame in FRAMES {
        compare_paths(&actual.project, frame, &expected.project);
    }
    println!(
        "Verified complete source, VIEW and {} five-route unmasked RGBA frame sets: {}",
        FRAMES.len(),
        Path::new(&actual_path).display()
    );
}
