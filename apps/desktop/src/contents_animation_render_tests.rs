//! Independent Contents-animation acceptance. Expected documents are literal
//! data replacements, never commands used as an oracle. Generated LEP fixtures
//! exercise the official codec and are explicitly not native Save/Open evidence.
use super::{Action, EditorState};
use crate::{
    rendering::Renderer,
    view_state::{GraphChannel, GraphRanges, ProjectViews},
};
use libre_effects_core::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 240;
const FRAMES: [u32; 9] = [0, 15, 29, 30, 31, 45, 59, 60, 89];

fn contents(project: &Project) -> &ShapeContents {
    let Content::ShapeContents(value) = project.composition().layer(1).unwrap().content() else {
        panic!("Expected Contents fixture")
    };
    value
}
fn catalog(kinds: Vec<ContentsKind>) -> Project {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Contents animation acceptance".into(),
            width: WIDTH,
            height: HEIGHT,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::ShapeContents(Default::default()),
            width: WIDTH as f64,
            height: HEIGHT as f64,
            name: "Nested Contents animation".into(),
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
fn track(value: Value) -> AnimatedProperty {
    serde_json::from_value(value).unwrap()
}
fn scalar(node: &mut ContentsNode, parameter: ContentsParam, value: f64) {
    set(node, parameter, track(json!({"value": value, "keys": {}})));
}
fn set(node: &mut ContentsNode, parameter: ContentsParam, value: AnimatedProperty) {
    assert!(node.parameters.contains_key(&parameter));
    node.parameters.insert(parameter, value);
}
fn nested(project: &Project, children: Vec<ContentsNode>) -> Project {
    let mut inner = node(project, 2);
    inner.name = "Inner skewed".into();
    inner.kind = ContentsKind::Group(children);
    scalar(&mut inner, ContentsParam::Skew, 11.);
    scalar(
        &mut inner,
        ContentsParam::Transform(Property::Rotation),
        -9.,
    );
    let mut outer = node(project, 1);
    outer.name = "Outer reflected".into();
    outer.kind = ContentsKind::Group(vec![inner]);
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::PositionX),
        205.,
    );
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::PositionY),
        95.,
    );
    scalar(&mut outer, ContentsParam::Transform(Property::ScaleX), -85.);
    scalar(&mut outer, ContentsParam::Transform(Property::ScaleY), 90.);
    scalar(
        &mut outer,
        ContentsParam::Transform(Property::Rotation),
        13.,
    );
    tree(project, vec![outer])
}
fn linear(middle: Option<f64>) -> AnimatedProperty {
    let mut wire = json!({"value": 17., "keys": {
        "0": {"value": 40., "interpolation": "Linear"},
        "60": {"value": 100., "interpolation": "Hold"}
    }});
    if let Some(value) = middle {
        wire["keys"]["30"] = json!({"value": value, "interpolation": "Linear"});
    }
    track(wire)
}
fn eased(with_middle: bool) -> AnimatedProperty {
    let mut wire = json!({"value": 53., "keys": {
        "0": {"value": 40., "interpolation": "Linear"},
        "60": {"value": 80., "interpolation": "Hold"}
    }});
    if with_middle {
        wire["keys"]["30"] = json!({"value": 90.,
            "interpolation": {"Bezier": {"x1": 0.2, "y1": -0.1, "x2": 0.8, "y2": 1.2}},
            "temporal": {"incoming": {"slope": 0.5, "influence": 0.3}, "outgoing": {"slope": -0.25, "influence": 0.4}}
        });
    }
    track(wire)
}
fn numeric_fixture() -> (Project, Vec<ContentsNode>) {
    let project = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::Parametric(ShapeKind::Ellipse),
        ContentsKind::Parametric(ShapeKind::Star),
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::Fill { even_odd: false },
    ]);
    let mut children: Vec<_> = (3..=7).map(|id| node(&project, id)).collect();
    for (item, name) in children.iter_mut().zip([
        "Static rectangle",
        "Between-key ellipse",
        "Eased star",
        "Only-key rectangle",
        "Fill",
    ]) {
        item.name = name.into();
    }
    for (index, item) in children[..4].iter_mut().enumerate() {
        scalar(
            item,
            ContentsParam::Transform(Property::PositionX),
            -120. + index as f64 * 80.,
        );
        scalar(item, ContentsParam::Height, 65.);
    }
    scalar(&mut children[0], ContentsParam::Width, 60.);
    set(&mut children[1], ContentsParam::Width, linear(None));
    set(&mut children[2], ContentsParam::Width, eased(true));
    set(
        &mut children[3],
        ContentsParam::Width,
        track(json!({"value": 999., "keys": {
            "30": {"value": 75., "interpolation": "Hold", "temporal": {"mode": "Auto"}}
        }})),
    );
    scalar(
        &mut children[4],
        ContentsParam::Shape(ShapeParam::FillGreen),
        80.,
    );
    (project, children)
}
fn numeric_command(action: ContentsAnimationAction, frame: u32) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::SharedAnimation {
            parent: 2,
            items: vec![6, 3, 5, 4],
            parameter: ContentsParam::Width,
            frame,
            action,
        },
    }
}
fn exact(actual: &Project, expected: &Project, message: &str) {
    assert_eq!(actual, expected, "{message}");
    assert_eq!(
        serde_json::to_vec(actual).unwrap(),
        serde_json::to_vec(expected).unwrap(),
        "{message}: complete serialized source"
    );
}
fn export(label: &str, project: &Project, views: &ProjectViews) {
    let Some(directory) = std::env::var_os("LIBREEFFECTS_EXPORT_CONTENTS_ANIMATION_FIXTURES")
    else {
        return;
    };
    let root = Path::new(&directory);
    assert!(root.is_absolute());
    std::fs::create_dir_all(root).unwrap();
    let view = views.encode_native(project).unwrap();
    for (extension, bytes) in [
        ("lfe.json", project.to_json().unwrap().into_bytes()),
        (
            "generated.lep",
            project_file::encode(project, Some(&view)).unwrap(),
        ),
    ] {
        use std::io::Write;
        let path = root.join(format!("{label}.{extension}"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => file.write_all(&bytes).unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => assert_eq!(
                std::fs::read(&path).unwrap(),
                bytes,
                "Refusing to replace differing fixture {}",
                path.display()
            ),
            Err(error) => panic!("Cannot export {}: {error}", path.display()),
        }
    }
}
fn fixture_state(project: &Project, channels: &[(u64, ContentsParam)]) -> EditorState {
    let mut state = EditorState::default();
    state.editor.replace_project(project.clone()).unwrap();
    state.editor.select(1);
    state.editor.clear_history();
    state.frame = 30;
    state.selected_layers.insert(1);
    state.timeline_zoom = 2.;
    state.preview_zoom = Some(1.5);
    state.preview_pan = [13., -7.];
    state.graph_open = true;
    for &(item, parameter) in channels {
        let channel = GraphChannel {
            id: 1,
            property: PropertyPath::Contents { item, parameter },
        };
        state.graph_channels.pin(channel).unwrap();
        state.graph_channels.ranges.insert(
            channel,
            GraphRanges {
                value: Some([-100., 200.]),
                speed: Some([-50., 50.]),
            },
        );
    }
    state.normalize();
    state
}
fn render_pair(actual: &Project, expected: &Project, before: &Project, require_change: bool) {
    let renderer = Renderer::new();
    let native = project_file::encode(actual, None).unwrap();
    let reopened = project_file::decode(&native).unwrap().project;
    let mut changed = false;
    for frame in FRAMES {
        let output = renderer
            .render_output(actual, frame, WIDTH, HEIGHT)
            .unwrap();
        assert_eq!(
            output,
            renderer.render(actual, frame, WIDTH).unwrap(),
            "preview/output frame {frame}"
        );
        assert_eq!(
            output,
            renderer
                .render_output(expected, frame, WIDTH, HEIGHT)
                .unwrap(),
            "literal source render frame {frame}"
        );
        assert_eq!(
            output,
            renderer
                .render_output(&reopened, frame, WIDTH, HEIGHT)
                .unwrap(),
            "codec render frame {frame}"
        );
        changed |= output
            != renderer
                .render_output(before, frame, WIDTH, HEIGHT)
                .unwrap();
    }
    assert_eq!(changed, require_change, "fixture visual-change contract");
}
fn numeric_acceptance(
    label: &str,
    action: ContentsAnimationAction,
    mutate: impl FnOnce(&mut Vec<ContentsNode>),
    changes_pixels: bool,
) {
    let (catalog, mut children) = numeric_fixture();
    let before = nested(&catalog, children.clone());
    mutate(&mut children);
    let expected = nested(&catalog, children);
    assert_ne!(before, expected);
    let channels: Vec<_> = (3..=6).map(|id| (id, ContentsParam::Width)).collect();
    let mut state = fixture_state(&before, &channels);
    let key = KeyRef {
        id: 1,
        property: PropertyPath::Contents {
            item: 5,
            parameter: ContentsParam::Width,
        },
        frame: 30,
    };
    state.selected_keys.insert(key);
    state.graph_key = Some(key);
    state.graph_channels.activate(key.into());
    state.normalize();
    let graph = state.graph_channels.clone();
    let views = state.capture_views();
    let view_bytes = views.encode_native(&before).unwrap();
    let command = numeric_command(action, 30);
    state.bulk_test_action(&Action::Edit(command.clone()));
    assert_eq!(state.status, "Edited");
    exact(state.editor.project(), &expected, "explicit shared action");
    assert_eq!(state.graph_channels, graph);
    assert_eq!(state.selected_layers, [1].into());
    assert_eq!(
        state.capture_views().encode_native(&expected).unwrap(),
        view_bytes
    );
    let native = project_file::encode(state.editor.project(), Some(&view_bytes)).unwrap();
    let decoded = project_file::decode(&native).unwrap();
    exact(&decoded.project, &expected, "official native codec");
    assert_eq!(decoded.view, Some(view_bytes.as_slice()));
    let loaded_views = ProjectViews::read_native(decoded.view.unwrap(), &decoded.project).unwrap();
    let mut reopened = fixture_state(&decoded.project, &[]);
    reopened.load_views(loaded_views);
    assert_eq!(
        reopened.capture_views().encode_native(&expected).unwrap(),
        view_bytes
    );
    assert_eq!(reopened.graph_channels, graph);
    assert_eq!(reopened.frame, 30);
    render_pair(state.editor.project(), &expected, &before, changes_pixels);
    // Repeating the explicit action is idempotent, unlike a toggle.
    state.bulk_test_action(&Action::Edit(command));
    exact(
        state.editor.project(),
        &expected,
        "repeat is source-identical",
    );
    state.bulk_test_action(&Action::Undo);
    exact(state.editor.project(), &before, "one atomic Undo");
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
    assert!(state.selected_keys.is_empty());
    assert_eq!(state.graph_key, None);
    state.bulk_test_action(&Action::Edit(numeric_command(
        ContentsAnimationAction::RemoveKey,
        89,
    )));
    exact(state.editor.project(), &before, "missing-key no-op");
    assert!(state.editor.can_redo(), "no-op preserves Redo");
    state.bulk_test_action(&Action::Redo);
    exact(state.editor.project(), &expected, "Redo");
    assert_eq!(state.graph_channels, graph);
    export(&format!("{label}-before"), &before, &views);
    export(&format!("{label}-expected"), &expected, &views);
}

#[test]
fn shared_enable_preserves_animated_members_and_full_source_view_and_pixels() {
    numeric_acceptance(
        "shared-enable",
        ContentsAnimationAction::Enable,
        |children| {
            set(
                &mut children[0],
                ContentsParam::Width,
                track(
                    json!({"value": 60., "keys": {"30": {"value": 60., "interpolation": "Linear"}}}),
                ),
            );
        },
        false,
    );
}
#[test]
fn shared_add_key_preserves_existing_metadata_and_adds_each_own_sample() {
    numeric_acceptance(
        "shared-add-key",
        ContentsAnimationAction::AddKey,
        |children| {
            set(
                &mut children[0],
                ContentsParam::Width,
                track(
                    json!({"value": 60., "keys": {"30": {"value": 60., "interpolation": "Linear"}}}),
                ),
            );
            set(&mut children[1], ContentsParam::Width, linear(Some(70.)));
        },
        false,
    );
}
#[test]
fn shared_disable_bakes_each_sample_and_removes_all_keys_atomically() {
    numeric_acceptance(
        "shared-disable",
        ContentsAnimationAction::Disable,
        |children| {
            for (index, value) in [(1, 70.), (2, 90.), (3, 75.)] {
                scalar(&mut children[index], ContentsParam::Width, value);
            }
        },
        true,
    );
}
#[test]
fn shared_remove_key_preserves_adjacent_metadata_and_bakes_only_final_key() {
    numeric_acceptance(
        "shared-remove-key",
        ContentsAnimationAction::RemoveKey,
        |children| {
            set(&mut children[2], ContentsParam::Width, eased(false));
            scalar(&mut children[3], ContentsParam::Width, 75.);
        },
        true,
    );
}

fn gradient_colors(colors: Value, opacities: Value) -> GradientColors {
    serde_json::from_value(json!({"colors": colors, "opacities": opacities})).unwrap()
}
fn base_colors() -> GradientColors {
    gradient_colors(
        json!([
            {"id": 1, "position": 0., "midpoint": 35., "red": 240., "green": 20., "blue": 10.},
            {"id": 5, "position": 45., "midpoint": 60., "red": 30., "green": 210., "blue": 80.},
            {"id": 2, "position": 100., "midpoint": 50., "red": 20., "green": 70., "blue": 250.}
        ]),
        json!([
            {"id": 3, "position": 0., "midpoint": 45., "opacity": 100.},
            {"id": 6, "position": 70., "midpoint": 40., "opacity": 25.},
            {"id": 4, "position": 100., "midpoint": 50., "opacity": 70.}
        ]),
    )
}
fn middle_colors() -> GradientColors {
    gradient_colors(
        json!([
            {"id": 5, "position": 20., "midpoint": 65., "red": 200., "green": 220., "blue": 30.},
            {"id": 2, "position": 100., "midpoint": 50., "red": 180., "green": 30., "blue": 220.}
        ]),
        json!([
            {"id": 6, "position": 15., "midpoint": 30., "opacity": 85.},
            {"id": 4, "position": 85., "midpoint": 50., "opacity": 40.}
        ]),
    )
}
fn last_colors() -> GradientColors {
    gradient_colors(
        json!([
            {"id": 2, "position": 0., "midpoint": 50., "red": 10., "green": 30., "blue": 250.},
            {"id": 1, "position": 55., "midpoint": 50., "red": 240., "green": 20., "blue": 10.},
            {"id": 5, "position": 55., "midpoint": 75., "red": 30., "green": 230., "blue": 160.}
        ]),
        json!([
            {"id": 4, "position": 0., "midpoint": 50., "opacity": 50.},
            {"id": 3, "position": 80., "midpoint": 65., "opacity": 100.},
            {"id": 6, "position": 100., "midpoint": 50., "opacity": 15.}
        ]),
    )
}
/// Literal legacy-static oracle. No compound sampling or editing APIs are used.
fn materialize_legacy(item: &ContentsNode, colors: &GradientColors) -> ContentsNode {
    let mut item = item.clone();
    let mut wire = serde_json::to_value(&item).unwrap();
    let kind = if wire["kind"].get("GradientFill").is_some() {
        "GradientFill"
    } else {
        "GradientStroke"
    };
    let gradient = &mut wire["kind"][kind]["gradient"];
    gradient["colors"] = json!(colors.colors.iter().map(|stop| stop.id).collect::<Vec<_>>());
    gradient["opacities"] = json!(
        colors
            .opacities
            .iter()
            .map(|stop| stop.id)
            .collect::<Vec<_>>()
    );
    gradient.as_object_mut().unwrap().remove("colors_animation");
    item = serde_json::from_value(wire).unwrap();
    item.parameters.retain(
        |parameter, _| !matches!(parameter, ContentsParam::Gradient(p) if p.stop().is_some()),
    );
    for stop in &colors.colors {
        for (parameter, value) in [
            (GradientParam::ColorPosition(stop.id), stop.position),
            (GradientParam::ColorMidpoint(stop.id), stop.midpoint),
            (GradientParam::Red(stop.id), stop.red),
            (GradientParam::Green(stop.id), stop.green),
            (GradientParam::Blue(stop.id), stop.blue),
        ] {
            item.parameters.insert(
                ContentsParam::Gradient(parameter),
                track(json!({"value": value, "keys": {}})),
            );
        }
    }
    for stop in &colors.opacities {
        for (parameter, value) in [
            (GradientParam::OpacityPosition(stop.id), stop.position),
            (GradientParam::OpacityMidpoint(stop.id), stop.midpoint),
            (GradientParam::Opacity(stop.id), stop.opacity),
        ] {
            item.parameters.insert(
                ContentsParam::Gradient(parameter),
                track(json!({"value": value, "keys": {}})),
            );
        }
    }
    item
}
fn with_animation(item: &ContentsNode, keys: &[(u32, GradientColors)]) -> ContentsNode {
    let mut wire = serde_json::to_value(item).unwrap();
    let kind = if wire["kind"].get("GradientFill").is_some() {
        "GradientFill"
    } else {
        "GradientStroke"
    };
    wire["kind"][kind]["gradient"]["colors_animation"] =
        json!({"keys": keys.iter().cloned().collect::<BTreeMap<_, _>>()});
    serde_json::from_value(wire).unwrap()
}
fn schema(project: &Project, version: u32) -> Project {
    let mut wire = serde_json::to_value(project).unwrap();
    wire["version"] = json!(version);
    Project::from_json(&wire.to_string()).unwrap()
}
fn gradient_fixture(stroke: bool) -> (Project, ContentsNode, ContentsNode) {
    let mut gradient: Value = serde_json::to_value(ShapeGradient::default()).unwrap();
    gradient["next_stop"] = json!(9);
    let gradient: ShapeGradient = serde_json::from_value(gradient).unwrap();
    let project = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
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
    ]);
    let mut shape = node(&project, 3);
    scalar(&mut shape, ContentsParam::Width, 175.);
    scalar(&mut shape, ContentsParam::Height, 115.);
    let mut paint = materialize_legacy(&node(&project, 4), &base_colors());
    scalar(
        &mut paint,
        ContentsParam::Gradient(GradientParam::StartX),
        -80.,
    );
    scalar(
        &mut paint,
        ContentsParam::Gradient(GradientParam::EndX),
        80.,
    );
    scalar(
        &mut paint,
        ContentsParam::Gradient(GradientParam::StartY),
        -15.,
    );
    scalar(
        &mut paint,
        ContentsParam::Gradient(GradientParam::EndY),
        10.,
    );
    let opacity = if stroke {
        ShapeParam::StrokeOpacity
    } else {
        ShapeParam::FillOpacity
    };
    set(&mut paint, ContentsParam::Shape(opacity), linear(None));
    if stroke {
        scalar(
            &mut paint,
            ContentsParam::Shape(ShapeParam::StrokeWidth),
            14.,
        );
    }
    (project, shape, paint)
}
fn gradient_command(edit: GradientColorsEdit) -> Command {
    Command::Contents {
        id: 1,
        edit: ContentsEdit::GradientColors { item: 4, edit },
    }
}
fn gradient_views(project: &Project) -> ProjectViews {
    let mut state = fixture_state(
        project,
        &[
            (4, ContentsParam::Gradient(GradientParam::StartX)),
            (4, ContentsParam::Gradient(GradientParam::EndX)),
        ],
    );
    state.capture_views()
}
fn compound_acceptance(label: &str, before: &Project, expected: &Project, command: Command) {
    let channels = [
        (4, ContentsParam::Gradient(GradientParam::StartX)),
        (4, ContentsParam::Gradient(GradientParam::EndX)),
    ];
    let mut state = fixture_state(before, &channels);
    let views = state.capture_views();
    let view_bytes = views.encode_native(before).unwrap();
    let graph = state.graph_channels.clone();
    state.bulk_test_action(&Action::Edit(command));
    assert_eq!(state.status, "Edited");
    exact(state.editor.project(), expected, "compound source contract");
    assert_eq!(state.graph_channels, graph);
    assert_eq!(
        state.capture_views().encode_native(expected).unwrap(),
        view_bytes
    );
    let native = project_file::encode(state.editor.project(), Some(&view_bytes)).unwrap();
    let decoded = project_file::decode(&native).unwrap();
    exact(&decoded.project, expected, "compound official codec");
    assert_eq!(decoded.view, Some(view_bytes.as_slice()));
    let loaded = ProjectViews::read_native(decoded.view.unwrap(), &decoded.project).unwrap();
    let mut reopened = fixture_state(&decoded.project, &[]);
    reopened.load_views(loaded);
    assert_eq!(
        reopened.capture_views().encode_native(expected).unwrap(),
        view_bytes
    );
    state.bulk_test_action(&Action::Undo);
    exact(state.editor.project(), before, "compound one Undo");
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
    state.bulk_test_action(&Action::Redo);
    exact(state.editor.project(), expected, "compound Redo");
    assert_eq!(state.graph_channels, graph);
    export(&format!("{label}-before"), before, &views);
    export(&format!("{label}-expected"), expected, &views);
}

#[test]
fn compound_enable_is_literal_hold_snapshot_and_legacy_pixels_remain_exact() {
    for stroke in [false, true] {
        let (catalog, shape, paint) = gradient_fixture(stroke);
        let before = nested(&catalog, vec![shape.clone(), paint.clone()]);
        let expected = nested(
            &schema(&catalog, 54),
            vec![shape, with_animation(&paint, &[(30, base_colors())])],
        );
        compound_acceptance(
            if stroke {
                "colors-stroke-enable"
            } else {
                "colors-fill-enable"
            },
            &before,
            &expected,
            gradient_command(GradientColorsEdit::SetAnimation {
                frame: 30,
                enabled: true,
            }),
        );
        render_pair(&expected, &before, &before, false);
        let json = before.to_json().unwrap();
        assert!(
            !json.contains("colors_animation"),
            "legacy serialization has no new field"
        );
        let reopened = Project::from_json(&json).unwrap();
        exact(&reopened, &before, "legacy JSON remains literal source");
    }
}

#[test]
fn compound_topology_holds_match_independent_legacy_oracles_on_adjacent_frames() {
    for stroke in [false, true] {
        let (catalog, shape, paint) = gradient_fixture(stroke);
        let catalog = schema(&catalog, 54);
        let before = nested(
            &catalog,
            vec![
                shape.clone(),
                with_animation(&paint, &[(15, base_colors()), (60, last_colors())]),
            ],
        );
        let expected = nested(
            &catalog,
            vec![
                shape.clone(),
                with_animation(
                    &paint,
                    &[
                        (15, base_colors()),
                        (30, middle_colors()),
                        (60, last_colors()),
                    ],
                ),
            ],
        );
        let label = if stroke {
            "colors-stroke-topology"
        } else {
            "colors-fill-topology"
        };
        compound_acceptance(
            label,
            &before,
            &expected,
            gradient_command(GradientColorsEdit::Set {
                frame: 30,
                colors: middle_colors(),
            }),
        );
        render_pair(&expected, &expected, &before, true);
        let renderer = Renderer::new();
        for frame in FRAMES {
            // The interval is an independently chosen literal, not colors_at().
            let snapshot = if frame < 30 {
                base_colors()
            } else if frame < 60 {
                middle_colors()
            } else {
                last_colors()
            };
            let reference = nested(
                &catalog,
                vec![shape.clone(), materialize_legacy(&paint, &snapshot)],
            );
            assert_eq!(
                renderer
                    .render_output(&expected, frame, WIDTH, HEIGHT)
                    .unwrap(),
                renderer
                    .render_output(&reference, frame, WIDTH, HEIGHT)
                    .unwrap(),
                "independent Hold topology frame {frame}, stroke={stroke}"
            );
            export(
                &format!("{label}-legacy-frame-{frame:03}"),
                &reference,
                &gradient_views(&reference),
            );
        }
        let mut editor = Editor::default();
        editor.replace_project(expected.clone()).unwrap();
        editor.clear_history();
        editor
            .execute(gradient_command(GradientColorsEdit::Set {
                frame: 30,
                colors: middle_colors(),
            }))
            .unwrap();
        exact(
            editor.project(),
            &expected,
            "same compound snapshot is source-identical",
        );
        assert!(!editor.can_undo(), "compound no-op has no history");
    }
}

#[test]
fn compound_disable_bakes_current_topology_and_retains_unrelated_animated_scalars() {
    for stroke in [false, true] {
        let (catalog, shape, paint) = gradient_fixture(stroke);
        let catalog = schema(&catalog, 54);
        let before = nested(
            &catalog,
            vec![
                shape.clone(),
                with_animation(
                    &paint,
                    &[
                        (15, base_colors()),
                        (30, middle_colors()),
                        (60, last_colors()),
                    ],
                ),
            ],
        );
        let expected = nested(
            &catalog,
            vec![shape, materialize_legacy(&paint, &middle_colors())],
        );
        compound_acceptance(
            if stroke {
                "colors-stroke-disable"
            } else {
                "colors-fill-disable"
            },
            &before,
            &expected,
            gradient_command(GradientColorsEdit::SetAnimation {
                frame: 30,
                enabled: false,
            }),
        );
        render_pair(&expected, &expected, &before, true);
        assert!(!expected.to_json().unwrap().contains("colors_animation"));
    }
}

#[test]
fn legacy_animated_stop_promotion_rejects_without_source_view_or_redo_loss() {
    let (catalog, shape, mut paint) = gradient_fixture(false);
    set(
        &mut paint,
        ContentsParam::Gradient(GradientParam::Red(1)),
        linear(None),
    );
    let before = nested(&catalog, vec![shape, paint]);
    let mut state = fixture_state(
        &before,
        &[(4, ContentsParam::Gradient(GradientParam::Red(1)))],
    );
    let views = state.capture_views().encode_native(&before).unwrap();
    state.bulk_test_action(&Action::Edit(Command::RenameLayer {
        id: 1,
        name: "Redo survives refusal".into(),
    }));
    let renamed = state.editor.project().clone();
    state.bulk_test_action(&Action::Undo);
    assert!(state.editor.can_redo());
    state.bulk_test_action(&Action::Edit(gradient_command(
        GradientColorsEdit::SetAnimation {
            frame: 30,
            enabled: true,
        },
    )));
    assert_ne!(state.status, "Edited");
    exact(
        state.editor.project(),
        &before,
        "reject animated legacy promotion",
    );
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
    assert_eq!(state.capture_views().encode_native(&before).unwrap(), views);
    state.bulk_test_action(&Action::Redo);
    exact(state.editor.project(), &renamed, "refusal retains Redo");
    export(
        "legacy-animated-stop-rejection",
        &before,
        &gradient_views(&before),
    );
}

#[test]
fn compound_last_key_removal_bakes_literal_topology_at_every_frame() {
    let (catalog, shape, paint) = gradient_fixture(false);
    let catalog = schema(&catalog, 54);
    let before = nested(
        &catalog,
        vec![
            shape.clone(),
            with_animation(&paint, &[(30, middle_colors())]),
        ],
    );
    let expected = nested(
        &catalog,
        vec![shape, materialize_legacy(&paint, &middle_colors())],
    );
    compound_acceptance(
        "colors-remove-final-key",
        &before,
        &expected,
        gradient_command(GradientColorsEdit::ToggleKey { frame: 30 }),
    );
    render_pair(&expected, &expected, &before, false);
}

#[test]
fn compound_move_key_changes_hold_boundary_without_touching_snapshots_or_view() {
    let (catalog, shape, paint) = gradient_fixture(false);
    let catalog = schema(&catalog, 54);
    let before = nested(
        &catalog,
        vec![
            shape.clone(),
            with_animation(
                &paint,
                &[
                    (15, base_colors()),
                    (30, middle_colors()),
                    (60, last_colors()),
                ],
            ),
        ],
    );
    let expected = nested(
        &catalog,
        vec![
            shape.clone(),
            with_animation(
                &paint,
                &[
                    (15, base_colors()),
                    (31, middle_colors()),
                    (60, last_colors()),
                ],
            ),
        ],
    );
    compound_acceptance(
        "colors-move-key",
        &before,
        &expected,
        gradient_command(GradientColorsEdit::MoveKey { from: 30, to: 31 }),
    );
    render_pair(&expected, &expected, &before, true);
    let renderer = Renderer::new();
    for (frame, snapshot) in [
        (29, base_colors()),
        (30, base_colors()),
        (31, middle_colors()),
        (59, middle_colors()),
        (60, last_colors()),
    ] {
        let reference = nested(
            &catalog,
            vec![shape.clone(), materialize_legacy(&paint, &snapshot)],
        );
        assert_eq!(
            renderer
                .render_output(&expected, frame, WIDTH, HEIGHT)
                .unwrap(),
            renderer
                .render_output(&reference, frame, WIDTH, HEIGHT)
                .unwrap(),
            "moved Hold boundary frame {frame}"
        );
    }
}

#[test]
fn compound_dormant_stop_graph_pins_are_session_only_and_revive_on_undo() {
    let (catalog, shape, paint) = gradient_fixture(false);
    let before = nested(&catalog, vec![shape, paint]);
    let parameter = ContentsParam::Gradient(GradientParam::Red(1));
    let endpoint = ContentsParam::Gradient(GradientParam::EndX);
    let stop_channel = GraphChannel {
        id: 1,
        property: PropertyPath::Contents { item: 4, parameter },
    };
    let endpoint_channel = GraphChannel {
        id: 1,
        property: PropertyPath::Contents {
            item: 4,
            parameter: endpoint,
        },
    };
    let mut state = fixture_state(&before, &[(4, parameter), (4, endpoint)]);
    state.graph_channels.activate(endpoint_channel);
    let before_channels = state.graph_channels.clone();
    let before_view = state.capture_views().encode_native(&before).unwrap();
    state.bulk_test_action(&Action::Edit(gradient_command(
        GradientColorsEdit::SetAnimation {
            frame: 30,
            enabled: true,
        },
    )));
    assert_eq!(state.status, "Edited");
    let enabled = state.editor.project().clone();
    assert!(state.graph_channels.is_pinned(stop_channel));
    assert!(!state.graph_channels.is_available(stop_channel));
    assert_eq!(
        state.graph_channels.ranges[&stop_channel],
        before_channels.ranges[&stop_channel]
    );
    assert!(state.graph_channels.is_available(endpoint_channel));
    assert!(!state.graph_included_channels().contains(&stop_channel));
    assert!(
        enabled
            .composition()
            .layer(1)
            .unwrap()
            .track(stop_channel.property)
            .is_none()
    );
    let saved = state.capture_views();
    assert!(
        !saved.compositions[&1]
            .graph_channels
            .is_pinned(stop_channel)
    );
    assert!(
        saved.compositions[&1]
            .graph_channels
            .is_pinned(endpoint_channel)
    );
    // Capturing/pruning VIEW must not prune the live history-bearing session.
    assert!(state.graph_channels.is_pinned(stop_channel));
    let view_bytes = saved.encode_native(&enabled).unwrap();
    let native = project_file::encode(&enabled, Some(&view_bytes)).unwrap();
    let opened = crate::project_io::decode_project(&native).unwrap();
    assert!(
        !opened.views.compositions[&1]
            .graph_channels
            .is_pinned(stop_channel)
    );
    exact(&opened.project, &enabled, "saved hidden-stop project");
    // Every legacy mutation route must reject instead of editing dormant source.
    for command in [
        Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 4,
                parameter,
                edit: TrackEdit::Value {
                    frame: 30,
                    value: 100.,
                },
            },
        },
        Command::EditTrack {
            id: 1,
            property: stop_channel.property,
            edit: TrackEdit::ToggleKey { frame: 30 },
        },
        Command::Contents {
            id: 1,
            edit: ContentsEdit::AddGradientStop {
                item: 4,
                opacity: false,
                position: 50.,
                frame: 30,
            },
        },
        Command::Contents {
            id: 1,
            edit: ContentsEdit::RemoveGradientStop { item: 4, stop: 1 },
        },
    ] {
        state.bulk_test_action(&Action::Edit(command));
        assert_ne!(state.status, "Edited");
        exact(
            state.editor.project(),
            &enabled,
            "dormant scalar edits reject atomically",
        );
    }
    state.bulk_test_action(&Action::Undo);
    exact(state.editor.project(), &before, "Undo restores legacy mode");
    assert_eq!(state.graph_channels, before_channels);
    assert_eq!(
        state.capture_views().encode_native(&before).unwrap(),
        before_view
    );
    state.bulk_test_action(&Action::Redo);
    exact(
        state.editor.project(),
        &enabled,
        "Redo retains compound mode",
    );
    assert!(!state.graph_channels.is_available(stop_channel));
}

/// Opt-in checker for a file independently saved by the native application.
/// This does not create a native save or prove how either supplied file arose.
#[test]
#[ignore = "requires separately recorded native Save/Open files and a literal expected fixture"]
fn verify_recorded_native_contents_animation_save() {
    let actual_path = std::env::var_os("LIBREEFFECTS_CONTENTS_ANIMATION_NATIVE_SAVE")
        .expect("set actual native save path");
    let expected_path = std::env::var_os("LIBREEFFECTS_CONTENTS_ANIMATION_EXPECTED")
        .expect("set independent expected generated LEP path");
    let actual = crate::project_io::read_editor_project(Path::new(&actual_path)).unwrap();
    let mut expected = crate::project_io::read_editor_project(Path::new(&expected_path)).unwrap();
    if let Ok(frame) = std::env::var("LIBREEFFECTS_CONTENTS_ANIMATION_EXPECTED_FRAME") {
        let frame: u32 = frame
            .parse()
            .expect("expected frame must be an unsigned integer");
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
        "recorded native Save/Open complete source",
    );
    assert_eq!(
        actual.views, expected.views,
        "recorded native Save/Open VIEW"
    );
    render_pair(&actual.project, &expected.project, &expected.project, false);
    println!(
        "Official desktop decoder: exact complete source, VIEW and {} preview/output/reference/codec frame sets. Actual native file: {}",
        FRAMES.len(),
        Path::new(&actual_path).display()
    );
}

#[test]
fn shared_endpoint_animation_keeps_compound_and_legacy_gradient_storage_independent() {
    let catalog = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::GradientFill {
            even_odd: false,
            gradient: Default::default(),
        },
        ContentsKind::GradientStroke {
            style: Default::default(),
            gradient: Default::default(),
        },
    ]);
    let catalog = schema(&catalog, 54);
    let mut shape = node(&catalog, 3);
    scalar(&mut shape, ContentsParam::Width, 175.);
    scalar(&mut shape, ContentsParam::Height, 115.);
    let mut fill = node(&catalog, 4);
    let mut wire = serde_json::to_value(&fill).unwrap();
    wire["kind"]["GradientFill"]["gradient"]["next_stop"] = json!(9);
    fill = serde_json::from_value(wire).unwrap();
    fill = materialize_legacy(&fill, &base_colors());
    fill = with_animation(
        &fill,
        &[
            (15, base_colors()),
            (30, middle_colors()),
            (60, last_colors()),
        ],
    );
    let parameter = ContentsParam::Gradient(GradientParam::EndX);
    set(&mut fill, parameter, linear(None));
    scalar(
        &mut fill,
        ContentsParam::Gradient(GradientParam::StartX),
        -80.,
    );
    let mut stroke = node(&catalog, 5);
    scalar(&mut stroke, parameter, 80.);
    scalar(
        &mut stroke,
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        14.,
    );
    set(
        &mut stroke,
        ContentsParam::Gradient(GradientParam::Blue(1)),
        linear(None),
    );
    let before = nested(&catalog, vec![shape.clone(), stroke.clone(), fill.clone()]);
    scalar(&mut fill, parameter, 70.);
    let expected = nested(&catalog, vec![shape, stroke, fill]);
    compound_acceptance(
        "shared-gradient-endpoint-disable",
        &before,
        &expected,
        Command::Contents {
            id: 1,
            edit: ContentsEdit::SharedAnimation {
                parent: 2,
                items: vec![5, 4],
                parameter,
                frame: 30,
                action: ContentsAnimationAction::Disable,
            },
        },
    );
    render_pair(&expected, &expected, &before, true);
}

#[test]
fn compound_modal_fractional_rgb_away_back_preserves_complete_snapshot_and_redo() {
    let (catalog, shape, paint) = gradient_fixture(false);
    let mut colors = base_colors();
    colors.colors[0].red = 127.5;
    colors.colors[0].green = 32.125;
    colors.colors[0].blue = 99.875;
    let before = nested(
        &schema(&catalog, 54),
        vec![
            shape,
            with_animation(&paint, &[(15, colors), (60, last_colors())]),
        ],
    );
    let mut state = fixture_state(&before, &[]);
    state.contents_selection = Some((1, 1, 4));
    state.bulk_test_action(&Action::Edit(Command::RenameLayer {
        id: 1,
        name: "Preserve modal Redo".into(),
    }));
    state.bulk_test_action(&Action::Undo);
    assert!(state.editor.can_redo());
    let mut draft = crate::panels::gradient_editor::Session::new(&state, 4).unwrap();
    assert_eq!(draft.field_value(4).as_deref(), Some("128"));
    draft.input(4, "12").unwrap();
    draft.input(4, "128").unwrap();
    assert!(
        draft.command().unwrap().is_none(),
        "returning to displayed RGB must restore original fractional source"
    );
    exact(
        &draft.preview(&state).unwrap(),
        &before,
        "fractional color no-op preview",
    );
    state.gradient_editor = Some(draft);
    state.accept_gradient_editor();
    exact(
        state.editor.project(),
        &before,
        "fractional color no-op apply",
    );
    assert!(!state.editor.can_undo());
    assert!(state.editor.can_redo());
}

#[test]
fn compound_signed_zero_stop_order_matches_literal_legacy_spatial_sorting() {
    let (catalog, shape, paint) = gradient_fixture(false);
    let mut colors = gradient_colors(
        json!([
            {"id": 1, "position": 0.0, "midpoint": 50., "red": 240., "green": 20., "blue": 10.},
            {"id": 2, "position": -0.0, "midpoint": 50., "red": 20., "green": 70., "blue": 250.}
        ]),
        json!([
            {"id": 3, "position": 0., "midpoint": 50., "opacity": 100.},
            {"id": 4, "position": 100., "midpoint": 50., "opacity": 100.}
        ]),
    );
    assert_eq!(colors.colors[1].position.to_bits(), (-0.0_f64).to_bits());
    let old = colors.clone();
    let catalog = schema(&catalog, 54);
    let before = nested(
        &catalog,
        vec![shape.clone(), with_animation(&paint, &[(15, old.clone())])],
    );
    colors.colors[1].position = 0.0;
    let expected = nested(
        &catalog,
        vec![
            shape.clone(),
            with_animation(&paint, &[(15, old.clone()), (30, colors.clone())]),
        ],
    );
    compound_acceptance(
        "colors-signed-zero",
        &before,
        &expected,
        gradient_command(GradientColorsEdit::Value {
            frame: 30,
            parameter: GradientParam::ColorPosition(2),
            value: 0.0,
        }),
    );
    render_pair(&expected, &expected, &before, true);
    let renderer = Renderer::new();
    for frame in FRAMES {
        let reference = nested(
            &catalog,
            vec![
                shape.clone(),
                materialize_legacy(&paint, if frame < 30 { &old } else { &colors }),
            ],
        );
        assert_eq!(
            renderer
                .render_output(&expected, frame, WIDTH, HEIGHT)
                .unwrap(),
            renderer
                .render_output(&reference, frame, WIDTH, HEIGHT)
                .unwrap(),
            "signed-zero literal legacy oracle frame {frame}"
        );
    }
}
