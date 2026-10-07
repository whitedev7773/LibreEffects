//! E04 stage1 acceptance. Literal tree layouts and baked geometry are independent
//! oracles: no expected document invokes ContentsEdit::Move/MoveSiblings.
//! LEP roundtrips below exercise codecs; generated fixtures are not native saves.
use crate::{
    rendering::Renderer,
    view_state::{CompositionView, GraphChannel, GraphChannels, GraphRanges, ProjectViews},
};
use libre_effects_core::*;
use serde_json::json;
use std::collections::BTreeMap;

const WIDTH: u32 = 240;
const HEIGHT: u32 = 160;
const FRAMES: [u32; 3] = [0, 30, 60];

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
            name: "Cross-parent independent acceptance".into(),
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
            name: "Local values stay local".into(),
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

fn group(project: &Project, id: u64, children: Vec<ContentsNode>) -> ContentsNode {
    let mut node = node(project, id);
    assert!(matches!(node.kind, ContentsKind::Group(_)));
    node.kind = ContentsKind::Group(children);
    node
}

fn tree(project: &Project, items: Vec<ContentsNode>) -> Project {
    let mut value = serde_json::to_value(project).unwrap();
    value["composition"]["layers"][0]["content"]["ShapeContents"]["items"] =
        serde_json::to_value(items).unwrap();
    Project::from_json(&value.to_string()).unwrap()
}

fn track(value: f64, end: Option<f64>) -> AnimatedProperty {
    serde_json::from_value(match end {
        None => json!({"value": value, "keys": {}}),
        Some(end) => json!({"value": value, "keys": {
            "0": {"value": value, "interpolation": "Linear"},
            "60": {"value": end, "interpolation": {"Bezier": {"x1": 0.2, "y1": -0.1, "x2": 0.8, "y2": 1.2}}, "temporal": {"outgoing": {"slope": 0.25, "influence": 0.4}}}
        }}),
    })
    .unwrap()
}

fn parameter(node: &mut ContentsNode, parameter: ContentsParam, value: f64, end: Option<f64>) {
    assert!(node.parameters.contains_key(&parameter));
    node.parameters.insert(parameter, track(value, end));
}

fn transform(node: &mut ContentsNode, property: Property, value: f64, end: Option<f64>) {
    parameter(node, ContentsParam::Transform(property), value, end);
}

fn rgb(node: &mut ContentsNode, color: [f64; 3], stroke: bool) {
    let keys = if stroke {
        [
            ShapeParam::StrokeRed,
            ShapeParam::StrokeGreen,
            ShapeParam::StrokeBlue,
        ]
    } else {
        [
            ShapeParam::FillRed,
            ShapeParam::FillGreen,
            ShapeParam::FillBlue,
        ]
    };
    for (key, value) in keys.into_iter().zip(color) {
        parameter(node, ContentsParam::Shape(key), value, None);
    }
}

fn path(points: &[[f64; 2]], closed: bool) -> VectorPath {
    VectorPath {
        closed,
        vertices: points.iter().copied().map(PathVertex::corner).collect(),
    }
}

fn rectangle(dx: f64, dy: f64) -> VectorPath {
    path(
        &[
            [10. + dx, 10. + dy],
            [70. + dx, 10. + dy],
            [70. + dx, 50. + dy],
            [10. + dx, 50. + dy],
        ],
        true,
    )
}

fn path_kind(path: VectorPath) -> ContentsKind {
    ContentsKind::Path {
        path,
        animation: PathAnimation::default(),
    }
}

fn move_block(editor: &mut Editor, source_parent: u64, items: Vec<u64>, parent: u64, index: usize) {
    editor
        .execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::MoveSiblings {
                source_parent,
                items,
                parent,
                index,
            },
        })
        .unwrap();
}

// Comparing shallow records preserves every node-local field (including all
// dormant pose slots and temporal metadata), while allowing only hierarchy edits.
fn records(project: &Project) -> BTreeMap<u64, ContentsNode> {
    contents(project)
        .rows()
        .into_iter()
        .map(|(_, _, node)| {
            let mut node = node.clone();
            if let ContentsKind::Group(children) = &mut node.kind {
                children.clear();
            }
            (node.id, node)
        })
        .collect()
}

fn assert_preserved(before: &Project, after: &Project) {
    assert_eq!(records(before), records(after));
    let a = serde_json::to_value(contents(before)).unwrap();
    let b = serde_json::to_value(contents(after)).unwrap();
    assert_eq!(a["next_id"], b["next_id"]);
    assert_eq!(
        serde_json::to_value(before).unwrap()["version"],
        serde_json::to_value(after).unwrap()["version"]
    );
}

fn assert_history(editor: &mut Editor, before: &Project, after: &Project) {
    assert!(editor.can_undo());
    editor.undo();
    assert_eq!(editor.project(), before);
    assert!(!editor.can_undo(), "one block must be one transaction");
    editor.redo();
    assert_eq!(editor.project(), after);
    assert!(!editor.can_redo());
}

fn assert_routes(renderer: &Renderer, project: &Project, frame: u32) -> image::RgbaImage {
    let original = project.to_json().unwrap();
    let preview = renderer.render_preview(project, frame, WIDTH).unwrap();
    assert_eq!(preview, renderer.render(project, frame, WIDTH).unwrap());
    assert_eq!(
        preview,
        renderer
            .render_output(project, frame, WIDTH, HEIGHT)
            .unwrap()
    );
    let json = Project::from_json(&original).unwrap();
    let encoded = project_file::encode(project, None).unwrap();
    let reopened = project_file::decode(&encoded).unwrap().project;
    assert_eq!(&reopened, project);
    assert_eq!(preview, renderer.render(&json, frame, WIDTH).unwrap());
    assert_eq!(preview, renderer.render(&reopened, frame, WIDTH).unwrap());
    assert_eq!(project.to_json().unwrap(), original);
    preview
}

fn svg_reference(renderer: &Renderer, body: &str) -> image::RgbaImage {
    let pixmap = renderer.raster_canvas(body, WIDTH, HEIGHT, WIDTH).unwrap();
    image::RgbaImage::from_raw(
        WIDTH,
        HEIGHT,
        pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let p = p.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            })
            .collect(),
    )
    .unwrap()
}

// Optional coordinator-owned CLI materialization, deliberately labeled generated.
// A caller supplies its own output directory. Existing differing fixtures are
// never replaced: reruns may only verify previously emitted identical bytes.
fn export_case(label: &str, before: &Project, after: &Project, references: &[(u32, Project)]) {
    use std::io::Write;

    let Some(directory) = std::env::var_os("LIBREEFFECTS_EXPORT_CROSS_PARENT_FIXTURES") else {
        return;
    };
    let root = std::path::PathBuf::from(directory);
    assert!(
        root.is_absolute(),
        "Fixture export directory must be absolute"
    );
    std::fs::create_dir_all(&root).unwrap();
    let write = |name: String, bytes: &[u8]| {
        let path = root.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => file.write_all(bytes).unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    bytes,
                    "Existing fixture differs; refusing to replace {}",
                    path.display()
                );
            }
            Err(error) => panic!("Cannot create fixture {}: {error}", path.display()),
        }
    };
    for (suffix, project) in [("before", before), ("moved", after)] {
        write(
            format!("{label}-{suffix}.lfe.json"),
            project.to_json().unwrap().as_bytes(),
        );
        write(
            format!("{label}-{suffix}-generated.lep"),
            &project_file::encode(project, None).unwrap(),
        );
    }
    for (frame, project) in references {
        write(
            format!("{label}-baked-{frame}.lfe.json"),
            project.to_json().unwrap().as_bytes(),
        );
    }
}

#[test]
fn reflected_and_singular_destinations_adopt_animated_local_chain_without_baking() {
    let renderer = Renderer::new();
    for scale_x in [-100., 0.] {
        let catalog = catalog(vec![
            ContentsKind::Group(vec![]),
            ContentsKind::Group(vec![]),
            path_kind(rectangle(0., 0.)),
            path_kind(rectangle(100., 0.)),
            ContentsKind::Fill { even_odd: false },
            ContentsKind::Group(vec![]),
        ]);
        let mut geometry = node(&catalog, 3);
        geometry.name = "Animated local geometry with unused duplicate poses".into();
        geometry.kind = ContentsKind::Path {
            path: rectangle(-5., -4.),
            animation: serde_json::from_value(json!({
                "poses": [rectangle(0., 0.), rectangle(12., 8.), rectangle(42., 32.), rectangle(42., 32.)],
                "timing": {"value": 2., "keys": {
                    "0": {"value": 0., "interpolation": "Linear"},
                    "60": {"value": 1., "interpolation": "Hold"}
                }}
            })).unwrap(),
        };
        let mut disabled = node(&catalog, 4);
        disabled.enabled = false;
        let mut paint = node(&catalog, 5);
        rgb(&mut paint, [224., 32., 64.], false);
        parameter(
            &mut paint,
            ContentsParam::Shape(ShapeParam::FillGreen),
            32.,
            Some(128.),
        );
        let mut source = group(
            &catalog,
            1,
            vec![geometry.clone(), disabled.clone(), paint.clone()],
        );
        transform(&mut source, Property::PositionX, 20., Some(40.));
        transform(&mut source, Property::PositionY, 20., Some(30.));
        let mut destination = group(&catalog, 2, vec![]);
        transform(&mut destination, Property::ScaleX, scale_x, None);
        transform(&mut destination, Property::PositionX, 170., Some(200.));
        transform(&mut destination, Property::PositionY, 40., Some(60.));
        let mut ancestor = group(&catalog, 6, vec![destination.clone()]);
        transform(&mut ancestor, Property::ScaleY, -100., None);
        transform(&mut ancestor, Property::PositionX, 10., Some(20.));
        transform(&mut ancestor, Property::PositionY, 140., Some(150.));
        let before = tree(&catalog, vec![source.clone(), ancestor.clone()]);
        source.kind = ContentsKind::Group(vec![disabled]);
        destination.kind = ContentsKind::Group(vec![geometry.clone(), paint.clone()]);
        ancestor.kind = ContentsKind::Group(vec![destination]);
        let expected = tree(&catalog, vec![source, ancestor]);
        let mut editor = Editor::default();
        editor.replace_project(before.clone()).unwrap();
        editor.clear_history();
        // Caller order is intentionally reversed and selected source nodes are
        // noncontiguous. The source's visual order must define the new block.
        move_block(&mut editor, 1, vec![5, 3], 2, 0);
        assert_eq!(editor.project(), &expected);
        assert_preserved(&before, &expected);
        let mut references = Vec::new();
        for frame in FRAMES {
            let t = frame as f64 / 60.;
            let mut baked = rectangle(12. * t, 8. * t);
            for vertex in &mut baked.vertices {
                vertex.position = [
                    180. + 40. * t + scale_x / 100. * vertex.position[0],
                    100. - 10. * t - vertex.position[1],
                ];
            }
            let mut baked_node = geometry.clone();
            baked_node.kind = path_kind(baked);
            let reference = tree(&catalog, vec![baked_node, paint.clone()]);
            let actual = assert_routes(&renderer, editor.project(), frame);
            assert_eq!(
                actual,
                assert_routes(&renderer, &reference, frame),
                "scale {scale_x}, frame {frame}"
            );
            assert_ne!(actual, assert_routes(&renderer, &before, frame));
            if scale_x == 0. {
                assert!(actual.pixels().all(|p| p[3] == 0));
            } else {
                assert!(actual.pixels().any(|p| p[3] == 255));
            }
            references.push((frame, reference));
        }
        export_case(
            if scale_x == 0. {
                "singular"
            } else {
                "reflected"
            },
            &before,
            &expected,
            &references,
        );
        assert_history(&mut editor, &before, &expected);
    }
}

#[test]
fn moving_only_a_paint_adopts_destination_paths_and_preserves_both_geometries() {
    let catalog = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        path_kind(rectangle(0., 0.)),
        ContentsKind::Fill { even_odd: false },
        path_kind(rectangle(90., 40.)),
        ContentsKind::Fill { even_odd: false },
    ]);
    let mut red = node(&catalog, 4);
    rgb(&mut red, [255., 0., 0.], false);
    let mut blue = node(&catalog, 6);
    rgb(&mut blue, [0., 0., 255.], false);
    let before = tree(
        &catalog,
        vec![
            group(&catalog, 1, vec![node(&catalog, 3), red.clone()]),
            group(&catalog, 2, vec![node(&catalog, 5), blue.clone()]),
        ],
    );
    let expected = tree(
        &catalog,
        vec![
            group(&catalog, 1, vec![node(&catalog, 3)]),
            group(&catalog, 2, vec![node(&catalog, 5), red.clone(), blue]),
        ],
    );
    let reference = tree(&catalog, vec![node(&catalog, 5), red]);
    let mut editor = Editor::default();
    editor.replace_project(before.clone()).unwrap();
    editor.clear_history();
    move_block(&mut editor, 1, vec![4], 2, 1);
    assert_eq!(editor.project(), &expected);
    assert_preserved(&before, &expected);
    let renderer = Renderer::new();
    let actual = assert_routes(&renderer, editor.project(), 0);
    assert_eq!(actual, assert_routes(&renderer, &reference, 0));
    assert_eq!(actual.get_pixel(20, 20).0, [0; 4]);
    assert_eq!(actual.get_pixel(120, 60).0, [255, 0, 0, 255]);
    let old = assert_routes(&renderer, &before, 0);
    assert_eq!(old.get_pixel(20, 20).0, [255, 0, 0, 255]);
    assert_eq!(old.get_pixel(120, 60).0, [0, 0, 255, 255]);
    export_case("paint-scope", &before, &expected, &[(0, reference)]);
    assert_history(&mut editor, &before, &expected);
}

#[test]
fn moving_child_painted_group_and_path_changes_parent_trim_but_not_child_paint() {
    let catalog = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        path_kind(path(&[[20., 40.], [180., 40.]], false)),
        ContentsKind::Stroke(ShapeStroke::default()),
        path_kind(path(&[[20., 80.], [180., 80.]], false)),
        ContentsKind::Stroke(ShapeStroke::default()),
        ContentsKind::TrimPaths,
    ]);
    let mut red = node(&catalog, 5);
    red.blend = PaintBlend::Multiply;
    rgb(&mut red, [255., 0., 0.], true);
    parameter(
        &mut red,
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        8.,
        None,
    );
    let mut blue = node(&catalog, 7);
    rgb(&mut blue, [0., 0., 255.], true);
    parameter(
        &mut blue,
        ContentsParam::Shape(ShapeParam::StrokeWidth),
        12.,
        None,
    );
    let mut trim = node(&catalog, 8);
    parameter(&mut trim, ContentsParam::Trim(TrimParam::Start), 25., None);
    parameter(&mut trim, ContentsParam::Trim(TrimParam::End), 75., None);
    let mut child = group(&catalog, 3, vec![node(&catalog, 4), red.clone()]);
    transform(&mut child, Property::Opacity, 50., None);
    let mut destination = group(&catalog, 2, vec![blue.clone(), trim]);
    transform(&mut destination, Property::Opacity, 80., None);
    let before = tree(
        &catalog,
        vec![
            group(&catalog, 1, vec![child.clone(), node(&catalog, 6)]),
            destination.clone(),
        ],
    );
    let ContentsKind::Group(children) = &mut destination.kind else {
        unreachable!()
    };
    children.splice(0..0, [child.clone(), node(&catalog, 6)]);
    let expected = tree(&catalog, vec![group(&catalog, 1, vec![]), destination]);
    let mut editor = Editor::default();
    editor.replace_project(before.clone()).unwrap();
    editor.clear_history();
    move_block(&mut editor, 1, vec![6, 3], 2, 0);
    assert_eq!(editor.project(), &expected);
    assert_preserved(&before, &expected);
    let renderer = Renderer::new();
    let actual = assert_routes(&renderer, editor.project(), 0);
    // Parent Trim modifies exported geometry, but the child's already-painted
    // red stroke remains the full line. Parent opacity isolates their composite.
    let analytic = svg_reference(
        &renderer,
        "<g opacity='0.8'><path d='M60 40H140 M60 80H140' fill='none' stroke='#0000ff' stroke-width='12' stroke-linecap='butt' stroke-linejoin='miter'/><g opacity='0.5'><path d='M20 40H180' fill='none' stroke='#ff0000' stroke-width='8' stroke-linecap='butt' stroke-linejoin='miter'/></g></g>",
    );
    assert_eq!(actual, analytic);
    assert_ne!(actual, assert_routes(&renderer, &before, 0));
    assert!(
        actual.get_pixel(30, 40)[3] > 0,
        "child painting must remain untrimmed"
    );
    assert_eq!(actual.get_pixel(30, 80).0, [0; 4]);
    assert_eq!(actual.get_pixel(100, 80).0, [0, 0, 255, 204]);
    let mut baked_top = node(&catalog, 4);
    baked_top.kind = path_kind(path(&[[60., 40.], [140., 40.]], false));
    let mut baked_bottom = node(&catalog, 6);
    baked_bottom.kind = path_kind(path(&[[60., 80.], [140., 80.]], false));
    // Paint an independent cut-path group behind the full isolated child.
    let mut cut_group = group(&catalog, 1, vec![]);
    cut_group.id = 8;
    cut_group.name = "Independently baked parent Trim".into();
    // Reuse the original path IDs only here: the child needs a distinct copy.
    baked_top.id = 1;
    cut_group.kind = ContentsKind::Group(vec![baked_top, baked_bottom, blue]);
    let mut root = group(&catalog, 2, vec![child, cut_group]);
    transform(&mut root, Property::Opacity, 80., None);
    let reference = tree(&catalog, vec![root]);
    assert_eq!(actual, assert_routes(&renderer, &reference, 0));
    export_case(
        "child-trim-isolation",
        &before,
        &expected,
        &[(0, reference)],
    );
    assert_history(&mut editor, &before, &expected);
}

#[test]
fn root_move_preserves_group_path_paint_gradient_trim_pins_ranges_and_view_versions() {
    let catalog = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        ContentsKind::Parametric(ShapeKind::Rectangle),
        ContentsKind::Fill { even_odd: false },
        ContentsKind::GradientFill {
            even_odd: true,
            gradient: ShapeGradient::default(),
        },
        ContentsKind::TrimPaths,
    ]);
    let channel_specs = [
        (2, ContentsParam::Transform(Property::PositionX)),
        (3, ContentsParam::Width),
        (4, ContentsParam::Shape(ShapeParam::FillOpacity)),
        (5, ContentsParam::Gradient(GradientParam::Red(1))),
        (5, ContentsParam::Gradient(GradientParam::Opacity(3))),
        (6, ContentsParam::Trim(TrimParam::End)),
    ];
    let mut by_id: BTreeMap<_, _> = contents(&catalog)
        .rows()
        .into_iter()
        .map(|(_, _, n)| (n.id, n.clone()))
        .collect();
    for (item, key) in channel_specs {
        let node = by_id.get_mut(&item).unwrap();
        let value = node.value_at(key, 0);
        parameter(node, key, value, Some(value));
    }
    let mut subtree = by_id[&2].clone();
    subtree.kind = ContentsKind::Group(vec![
        by_id[&3].clone(),
        by_id[&4].clone(),
        by_id[&5].clone(),
        by_id[&6].clone(),
    ]);
    let before = tree(&catalog, vec![group(&catalog, 1, vec![subtree.clone()])]);
    let expected = tree(&catalog, vec![group(&catalog, 1, vec![]), subtree]);
    let channels: Vec<_> = channel_specs
        .into_iter()
        .map(|(item, parameter)| GraphChannel {
            id: 1,
            property: PropertyPath::Contents { item, parameter },
        })
        .collect();
    let mut graph = GraphChannels::default();
    for (i, &channel) in channels.iter().enumerate() {
        assert!(channel.available(before.composition()));
        graph.pin(channel).unwrap();
        graph.ranges.insert(
            channel,
            GraphRanges {
                value: Some([-(i as f64) - 10., 400.]),
                speed: Some([-50., 50.]),
            },
        );
    }
    graph.activate(channels[4]);
    let wire_before = serde_json::to_vec(&graph).unwrap();
    let original_tracks: Vec<_> = channels
        .iter()
        .map(|c| {
            before
                .composition()
                .layer(c.id)
                .unwrap()
                .track(c.property)
                .unwrap()
                .clone()
        })
        .collect();
    for explicit in [false, true] {
        let mut view = CompositionView::default();
        view.frame = 30;
        view.graph_view.height = Some([-100., 200.]);
        if explicit {
            view.graph_channels = graph.clone();
        }
        let mut views = ProjectViews::default();
        views.compositions.insert(1, view);
        views.normalize(&before);
        let view_before = views.encode_native(&before).unwrap();
        let mut editor = Editor::default();
        editor.replace_project(before.clone()).unwrap();
        editor.clear_history();
        move_block(&mut editor, 1, vec![2], 0, 1);
        assert_eq!(editor.project(), &expected);
        assert_preserved(&before, editor.project());
        graph.reconcile(Some(editor.project().composition()), false);
        assert_eq!(serde_json::to_vec(&graph).unwrap(), wire_before);
        assert_eq!(graph.included(), channels);
        assert_eq!(views.encode_native(editor.project()).unwrap(), view_before);
        for (channel, original) in channels.iter().zip(&original_tracks) {
            assert_eq!(
                editor
                    .project()
                    .composition()
                    .layer(channel.id)
                    .unwrap()
                    .track(channel.property)
                    .unwrap(),
                original
            );
        }
        let value: serde_json::Value = serde_json::from_slice(&view_before).unwrap();
        assert_eq!(value["version"], if explicit { 2 } else { 1 });
        if explicit {
            let graph = &value["compositions"]["1"]["graph_channels"];
            assert_eq!(graph["version"], 1);
            assert_eq!(graph["pinned"].as_array().unwrap().len(), channels.len());
            for (i, (item, parameter)) in channel_specs.into_iter().enumerate() {
                assert_eq!(graph["pinned"][i]["property"]["kind"], "contents");
                assert_eq!(graph["pinned"][i]["property"]["item"], item);
                assert_eq!(
                    graph["pinned"][i]["property"]["parameter"],
                    String::from(parameter)
                );
            }
        }
        let encoded =
            crate::project_io::encode_native_project(editor.project(), Some(&views)).unwrap();
        assert_eq!(&encoded[8..10], &[1, 0]);
        let opened = crate::project_io::decode_project(&encoded).unwrap();
        assert_eq!(opened.project, expected);
        assert_eq!(opened.views, views);
        assert_eq!(
            crate::project_io::encode_native_project(&opened.project, Some(&opened.views)).unwrap(),
            encoded
        );
        editor.undo();
        graph.reconcile(Some(editor.project().composition()), true);
        assert_eq!(editor.project(), &before);
        assert_eq!(serde_json::to_vec(&graph).unwrap(), wire_before);
        assert_eq!(views.encode_native(editor.project()).unwrap(), view_before);
        editor.redo();
        graph.reconcile(Some(editor.project().composition()), true);
        assert_eq!(editor.project(), &expected);
        assert_eq!(serde_json::to_vec(&graph).unwrap(), wire_before);
        assert_eq!(views.encode_native(editor.project()).unwrap(), view_before);
    }
}
