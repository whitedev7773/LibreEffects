use super::*;

fn path() -> ContentsNode {
    ContentsNode::with_defaults(ContentsKind::Path {
        path: VectorPath {
            vertices: [[-10., -20.], [180., 0.], [40., 140.]]
                .map(PathVertex::corner)
                .to_vec(),
            closed: true,
        },
        animation: PathAnimation::default(),
    })
}

fn tree() -> ShapeContents {
    let mut fill = ContentsNode::with_defaults(ContentsKind::Fill { even_odd: true });
    fill.set_static_value(ContentsParam::Shape(ShapeParam::FillRed), 23.)
        .unwrap();
    fill.set_static_value(ContentsParam::Shape(ShapeParam::FillOpacity), 67.)
        .unwrap();
    let mut group = ContentsNode::with_defaults(ContentsKind::Group(vec![path(), fill]));
    group.name = "Imported group".into();
    ShapeContents::from_nodes(vec![group]).unwrap()
}

fn import(contents: ShapeContents) -> Command {
    Command::ImportSvg {
        contents,
        width: 160.,
        height: 120.,
        name: "Drawing.svg".into(),
    }
}

fn pending_redo() -> Editor {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Redo survives".into(),
        })
        .unwrap();
    editor.undo();
    assert!(editor.can_redo());
    editor
}

fn rejected(editor: &mut Editor, command: Command) -> String {
    let before = (
        editor.current.clone(),
        editor.undo.clone(),
        editor.redo.clone(),
    );
    let error = editor.execute(command).unwrap_err();
    assert_eq!(
        (
            editor.current.clone(),
            editor.undo.clone(),
            editor.redo.clone()
        ),
        before
    );
    error
}

#[test]
fn svg_import_builder_assigns_fresh_ids_defaults_and_preserves_authored_values() {
    let contents = tree();
    assert_eq!(
        contents
            .rows()
            .iter()
            .map(|(_, _, node)| node.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(contents.next_id, 4);
    assert_eq!(contents.items[0].name, "Imported group");
    assert_eq!(
        contents
            .node(3)
            .unwrap()
            .value_at(ContentsParam::Shape(ShapeParam::FillRed), 0),
        23.
    );
    assert_eq!(contents.node(1).unwrap().transform(0), Affine::default());
    let mut nodes = contents.items.clone();
    nodes[0].id = u64::MAX;
    let rebuilt = ShapeContents::from_nodes(nodes).unwrap();
    assert_eq!(rebuilt, contents);
    let mut fill = contents.node(3).unwrap().clone();
    for (parameter, value) in [
        (ContentsParam::Width, 12.),
        (ContentsParam::Shape(ShapeParam::FillRed), -1.),
        (ContentsParam::Shape(ShapeParam::FillRed), f64::NAN),
        (ContentsParam::Shape(ShapeParam::FillRed), f64::INFINITY),
    ] {
        let before = fill.clone();
        assert!(fill.set_static_value(parameter, value).is_err());
        assert_eq!(fill, before);
    }
    fill.parameters
        .get_mut(&ContentsParam::Shape(ShapeParam::FillRed))
        .unwrap()
        .keys
        .insert(
            0,
            Keyframe {
                value: 10.,
                interpolation: Interpolation::Linear,
                temporal: Default::default(),
            },
        );
    let before = fill.clone();
    assert!(
        fill.set_static_value(ContentsParam::Shape(ShapeParam::FillRed), 20.)
            .is_err()
    );
    assert_eq!(fill, before);
}

#[test]
fn svg_import_builder_enforces_existing_tree_and_geometry_limits() {
    let fill = || ContentsNode::with_defaults(ContentsKind::Fill { even_odd: false });
    assert!(ShapeContents::from_nodes((0..256).map(|_| fill()).collect()).is_ok());
    assert!(ShapeContents::from_nodes((0..257).map(|_| fill()).collect()).is_err());
    let mut nested = fill();
    for _ in 0..8 {
        nested = ContentsNode::with_defaults(ContentsKind::Group(vec![nested]));
    }
    assert!(ShapeContents::from_nodes(vec![nested.clone()]).is_ok());
    assert!(
        ShapeContents::from_nodes(vec![ContentsNode::with_defaults(ContentsKind::Group(
            vec![nested]
        ))])
        .is_err()
    );
    let mut invalid_path = path();
    if let ContentsKind::Path { path, .. } = &mut invalid_path.kind {
        path.vertices[0].position[0] = f64::NAN;
    }
    assert!(ShapeContents::from_nodes(vec![invalid_path]).is_err());
    let mut bad_name = fill();
    bad_name.name = "x".repeat(129);
    assert!(ShapeContents::from_nodes(vec![bad_name]).is_err());
    let mut missing_default = fill();
    missing_default.parameters.clear();
    assert!(ShapeContents::from_nodes(vec![missing_default]).is_err());
}

#[test]
fn svg_import_is_one_undo_at_origin_with_viewport_clip_and_fresh_local_ids() {
    let mut editor = pending_redo();
    editor.current.project.composition.layers[0].locked = true;
    let before = editor.current.clone();
    let undo_len = editor.undo.len();
    let mut input = tree();
    fn rekey(nodes: &mut [ContentsNode]) {
        for node in nodes {
            node.id += 40;
            if let ContentsKind::Group(children) = &mut node.kind {
                rekey(children);
            }
        }
    }
    rekey(&mut input.items);
    input.next_id += 40;
    let source_input = input.clone();
    editor.execute(import(input.clone())).unwrap();
    assert_eq!(input, source_input);
    assert_eq!(editor.undo.len(), undo_len + 1);
    assert!(!editor.can_redo());
    assert_eq!(editor.selected(), Some(2));
    let layer = editor.selected_layer().unwrap();
    assert_eq!(layer.id, 2);
    assert_eq!((layer.width, layer.height), (160., 120.));
    assert_eq!(layer.name, "Drawing.svg");
    assert_eq!(layer.in_frame, 0);
    assert_eq!(layer.out_frame, None);
    assert_eq!(layer.asset, None);
    assert_eq!(layer.parent, None);
    assert_eq!(
        editor.project().composition.world_transform(2, 0),
        Some(Affine::default())
    );
    assert_eq!(
        editor.project().composition.layers[1],
        before.project.composition.layers[0]
    );
    let Content::ShapeContents(contents) = &layer.content else {
        panic!()
    };
    assert_eq!(contents, &tree());
    assert_eq!(layer.path_masks.len(), 1);
    let mask = &layer.path_masks[0];
    assert_eq!(mask.id, 1);
    assert_eq!(layer.next_mask_id, 2);
    assert_eq!(mask.mode, PathMaskMode::Add);
    assert!(!mask.inverted);
    assert_eq!(
        mask.path
            .vertices
            .iter()
            .map(|v| v.position)
            .collect::<Vec<_>>(),
        vec![[0., 0.], [160., 0.], [160., 120.], [0., 120.]]
    );
    assert!(mask.default_parameters());
    assert_eq!(editor.project().version, 44);
    let imported = editor.current.clone();
    editor.undo();
    assert_eq!(editor.current, before);
    editor.redo();
    assert_eq!(editor.current, imported);
    editor.execute(import(tree())).unwrap();
    assert_eq!(editor.selected(), Some(3));
    assert_eq!(editor.project().composition.layers[1].id, 2);
    assert_eq!(
        editor.selected_layer().unwrap().content(),
        &Content::ShapeContents(tree())
    );
}

#[test]
fn svg_import_rejects_invalid_empty_animated_or_unpainted_payloads_without_history() {
    let mut editor = pending_redo();
    for contents in [
        ShapeContents::default(),
        ShapeContents::from_nodes(vec![ContentsNode::with_defaults(ContentsKind::Group(
            vec![],
        ))])
        .unwrap(),
        ShapeContents::from_nodes(vec![path()]).unwrap(),
        ShapeContents::from_nodes(vec![ContentsNode::with_defaults(ContentsKind::Fill {
            even_odd: false,
        })])
        .unwrap(),
        ShapeContents::from_nodes(vec![
            ContentsNode::with_defaults(ContentsKind::Fill { even_odd: false }),
            path(),
        ])
        .unwrap(),
    ] {
        rejected(&mut editor, import(contents));
    }
    for kind in [
        ContentsKind::TrimPaths,
        ContentsKind::Parametric(ShapeKind::Rectangle),
    ] {
        let mut contents = tree();
        contents.items.push(ContentsNode::with_defaults(kind));
        let contents = ShapeContents::from_nodes(contents.items).unwrap();
        rejected(&mut editor, import(contents));
    }
    let mut contents = tree();
    contents
        .node_mut(3)
        .unwrap()
        .parameters
        .get_mut(&ContentsParam::Shape(ShapeParam::FillRed))
        .unwrap()
        .keys
        .insert(
            0,
            Keyframe {
                value: 10.,
                interpolation: Interpolation::Linear,
                temporal: Default::default(),
            },
        );
    rejected(&mut editor, import(contents));
    let mut contents = tree();
    contents.items[0].enabled = false;
    rejected(&mut editor, import(contents));
    let mut contents = tree();
    contents.items[0]
        .set_static_value(ContentsParam::Transform(Property::Opacity), 0.)
        .unwrap();
    rejected(&mut editor, import(contents));
    let mut contents = tree();
    contents
        .node_mut(3)
        .unwrap()
        .set_static_value(ContentsParam::Shape(ShapeParam::FillOpacity), 0.)
        .unwrap();
    rejected(&mut editor, import(contents));
    let mut contents = tree();
    contents.items[0].id = 2;
    rejected(&mut editor, import(contents));
    let mut contents = tree();
    contents.next_id = 3;
    rejected(&mut editor, import(contents));
    let mut contents = tree();
    contents.node_mut(3).unwrap().blend = PaintBlend::Multiply;
    rejected(&mut editor, import(contents));
    editor.redo();
    assert_eq!(editor.selected_layer().unwrap().name(), "Redo survives");
}

#[test]
fn svg_import_rejects_bad_dimensions_names_and_batch_escape_routes_atomically() {
    let mut editor = pending_redo();
    for (width, height) in [
        (0., 30.),
        (-1., 30.),
        (0.5, 30.),
        (16385., 30.),
        (160.5, 120.),
        (160., 120.25),
        (1.000_000_000_000_000_2, 30.),
        (30., 16383.999_999_999_998),
        (f64::NAN, 30.),
        (30., f64::INFINITY),
        (30., 0.),
    ] {
        rejected(
            &mut editor,
            Command::ImportSvg {
                contents: tree(),
                width,
                height,
                name: "Import".into(),
            },
        );
    }
    for name in [String::new(), "  ".into(), "x".repeat(1025)] {
        rejected(
            &mut editor,
            Command::ImportSvg {
                contents: tree(),
                width: 100.,
                height: 100.,
                name,
            },
        );
    }
    rejected(&mut editor, Command::Batch(vec![import(tree())]));
    rejected(
        &mut editor,
        Command::Batch(vec![Command::AddRectangle, import(tree())]),
    );
    let mut nested = import(tree());
    for _ in 0..128 {
        nested = Command::Batch(vec![nested]);
    }
    rejected(&mut editor, nested);
}

#[test]
fn svg_import_rejects_incompatible_legacy_assets_and_preserves_registered_sources() {
    let mut editor = pending_redo();
    editor.current.project.version = 9;
    let source = &mut editor.current.project.composition.layers[0];
    source.content = Content::Image { png: "YWJj".into() };
    let mut inactive = editor.project().composition.clone();
    inactive.layers[0].id = 8;
    editor
        .current
        .project
        .other_compositions
        .insert(2, inactive.clone());
    editor.current.project.next_composition_id = 3;
    editor.current.project.next_layer_id = 9;
    editor.project().validate().unwrap();
    // Registering old media would change authored source identity. Reject the
    // incompatible promotion, then exercise the same import on a modern source.
    assert!(rejected(&mut editor, import(tree())).contains("missing its asset ID"));
    editor.current.project.sync_assets().unwrap();
    let before = editor.current.clone();
    editor.execute(import(tree())).unwrap();
    assert_eq!(editor.selected(), Some(9));
    assert_eq!(
        editor.project().composition.layers[1],
        before.project.composition.layers[0]
    );
    assert_eq!(
        editor.project().other_compositions,
        before.project.other_compositions
    );
    assert_eq!(editor.project().asset_library, before.project.asset_library);
    assert!(editor.project().composition.layers[1].asset.is_some());
    assert_eq!(editor.project().version, 44);
    editor.undo();
    assert_eq!(editor.current, before);
}

#[test]
fn svg_import_preserves_legacy_group_sources_or_rejects_incompatible_promotion() {
    let mut editor = Editor::default();
    editor.execute(import(tree())).unwrap();
    editor.current.project.version = 43;
    let Content::ShapeContents(contents) =
        &mut editor.current.project.composition.layers[0].content
    else {
        panic!()
    };
    contents.items[0].parameters.remove(&ContentsParam::Skew);
    contents.items[0]
        .parameters
        .remove(&ContentsParam::SkewAxis);
    editor.project().validate().unwrap();
    let source = editor.project().composition.layers[0].clone();
    editor.execute(import(tree())).unwrap();
    assert_eq!(editor.project().version, 43);
    assert_eq!(editor.project().composition.layers[1], source);
    let Content::ShapeContents(contents) = editor.selected_layer().unwrap().content() else {
        panic!()
    };
    assert!(
        !contents.items[0]
            .parameters
            .contains_key(&ContentsParam::Skew)
    );
    editor.undo();
    let mut skewed = tree();
    skewed.items[0]
        .set_static_value(ContentsParam::Skew, 10.)
        .unwrap();
    rejected(&mut editor, import(skewed));
}

#[test]
fn svg_import_preserves_high_schema_and_both_codecs_with_exact_view() {
    for version in [1, 44, 54, 55, 56, 57] {
        let mut editor = Editor::default();
        editor.current.project.version = version;
        editor.execute(import(tree())).unwrap();
        assert_eq!(editor.project().version, version.max(44));
        let source = editor.project().clone();
        let json = source.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), source);
        let view = br#"{"timeline":{"zoom":3.5},"selected_layer":1}"#;
        let bytes = project_file::encode(&source, Some(view)).unwrap();
        let decoded = project_file::decode(&bytes).unwrap();
        assert_eq!(decoded.project, source);
        assert_eq!(decoded.view, Some(view.as_slice()));
        assert_eq!(
            project_file::encode(&decoded.project, decoded.view).unwrap(),
            bytes
        );
    }
}

#[test]
fn svg_import_checks_source_validity_and_global_layer_limit_before_commit() {
    let mut editor = pending_redo();
    editor.current.project.version = 0;
    rejected(&mut editor, import(tree()));
    editor.current.project.version = 1;
    let layer = editor.project().composition.layers[0].clone();
    editor.current.project.composition.layers = (1..=999)
        .map(|id| {
            let mut l = layer.clone();
            l.id = id;
            l
        })
        .collect();
    let mut inactive = editor.project().composition.clone();
    inactive.layers = vec![{
        let mut l = layer;
        l.id = 1000;
        l
    }];
    editor
        .current
        .project
        .other_compositions
        .insert(2, inactive);
    editor.current.project.version = 9;
    editor.current.project.next_composition_id = 3;
    editor.current.project.next_layer_id = 1001;
    editor.project().validate().unwrap();
    assert!(rejected(&mut editor, import(tree())).contains("1000 layers"));
}

#[test]
fn svg_import_static_linear_paints_require_only_existing_schema45() {
    for count in [2, 3, 32] {
        let gradient = ShapeGradient::with_paired_stops(count).unwrap();
        assert_eq!(gradient.colors.len(), count);
        assert_eq!(gradient.opacities.len(), count);
        assert!(gradient.valid());
        let contents = ShapeContents::from_nodes(vec![
            path(),
            ContentsNode::with_defaults(ContentsKind::GradientFill {
                even_odd: false,
                gradient,
            }),
        ])
        .unwrap();
        for version in [1, 44, 45, 60] {
            let mut editor = Editor::default();
            editor.current.project.version = version;
            let before = editor.current.clone();
            editor.execute(import(contents.clone())).unwrap();
            assert_eq!(editor.project().version, version.max(45));
            let expected = editor.current.clone();
            let bytes = project_file::encode(editor.project(), None).unwrap();
            assert_eq!(
                project_file::decode(&bytes).unwrap().project,
                expected.project
            );
            editor.undo();
            assert_eq!(editor.current, before);
            editor.redo();
            assert_eq!(editor.current, expected);
        }
    }
    for count in [0, 1, 33, usize::MAX] {
        assert!(ShapeGradient::with_paired_stops(count).is_err());
    }
}

#[test]
fn svg_import_linear_paint_does_not_migrate_existing_legacy_groups() {
    let mut editor = Editor::default();
    editor.execute(import(tree())).unwrap();
    editor.current.project.version = 43;
    let Content::ShapeContents(contents) =
        &mut editor.current.project.composition.layers[0].content
    else {
        panic!()
    };
    contents.items[0].parameters.remove(&ContentsParam::Skew);
    contents.items[0]
        .parameters
        .remove(&ContentsParam::SkewAxis);
    editor.project().validate().unwrap();
    let contents = ShapeContents::from_nodes(vec![
        path(),
        ContentsNode::with_defaults(ContentsKind::GradientFill {
            even_odd: false,
            gradient: ShapeGradient::default(),
        }),
    ])
    .unwrap();
    rejected(&mut editor, import(contents));
}

#[test]
fn svg_import_transparent_or_animated_gradient_rejects_atomically() {
    for animated in [false, true] {
        let gradient = ShapeGradient::default();
        let mut fill = ContentsNode::with_defaults(ContentsKind::GradientFill {
            even_odd: false,
            gradient: gradient.clone(),
        });
        if animated {
            let mut colors = gradient.colors_at(&fill, 0);
            colors.colors[0].red = 100.;
            fill.kind.gradient_mut().unwrap().colors_animation =
                Some(serde_json::from_value(serde_json::json!({"keys": {"0": colors}})).unwrap());
        } else {
            for &id in &gradient.opacities {
                fill.set_static_value(ContentsParam::Gradient(GradientParam::Opacity(id)), 0.)
                    .unwrap();
            }
        }
        let contents = ShapeContents::from_nodes(vec![path(), fill]).unwrap();
        rejected(&mut pending_redo(), import(contents));
    }
}

#[test]
fn svg_import_static_radial_fill_and_stroke_use_existing_schema_and_history() {
    for stroke in [false, true] {
        let gradient = ShapeGradient {
            radial: true,
            ..ShapeGradient::default()
        };
        let mut paint = ContentsNode::with_defaults(if stroke {
            ContentsKind::GradientStroke {
                style: ShapeStroke::default(),
                gradient,
            }
        } else {
            ContentsKind::GradientFill {
                even_odd: true,
                gradient,
            }
        });
        paint
            .set_static_value(ContentsParam::Gradient(GradientParam::HighlightLength), 50.)
            .unwrap();
        paint
            .set_static_value(ContentsParam::Gradient(GradientParam::HighlightAngle), -90.)
            .unwrap();
        let contents = ShapeContents::from_nodes(vec![path(), paint]).unwrap();
        for version in [1, 44, 45, 60] {
            let mut editor = Editor::default();
            editor.current.project.version = version;
            let before = editor.current.clone();
            editor.execute(import(contents.clone())).unwrap();
            assert_eq!(editor.project().version, version.max(45));
            let after = editor.current.clone();
            let bytes = project_file::encode(editor.project(), None).unwrap();
            assert_eq!(project_file::decode(&bytes).unwrap().project, after.project);
            editor.undo();
            assert_eq!(editor.current, before);
            editor.redo();
            assert_eq!(editor.current, after);
        }
    }
}
