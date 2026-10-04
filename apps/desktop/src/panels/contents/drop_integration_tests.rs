//! E04 stage2 integration oracles. Every expected hierarchy is written literally;
//! no expected Project calls Move, MoveSiblings, Reorder, or a production planner.
//! These headless tests do not establish native pointer, focus, IME, or Save/Open behavior.
use super::{
    tree_drop::{Geometry, RootGeometry, RowGeometry},
    tree_selection::{DropTarget, Selection, plan_drop, visible_rows},
};
use crate::{
    rendering::Renderer,
    view_state::{CompositionView, ProjectViews},
};
use gpui::{Bounds, point, px, size};
use libre_effects_core::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const WIDTH: u32 = 240;
const HEIGHT: u32 = 160;

fn contents(project: &Project) -> &ShapeContents {
    let Content::ShapeContents(c) = project.composition().layer(1).unwrap().content() else {
        panic!("Expected Contents")
    };
    c
}

fn path(dx: f64) -> VectorPath {
    VectorPath {
        closed: true,
        vertices: [
            [10. + dx, 10.],
            [70. + dx, 10.],
            [70. + dx, 50.],
            [10. + dx, 50.],
        ]
        .into_iter()
        .map(PathVertex::corner)
        .collect(),
    }
}

fn path_kind(dx: f64) -> ContentsKind {
    ContentsKind::Path {
        path: path(dx),
        animation: PathAnimation::default(),
    }
}

fn catalog(kinds: Vec<ContentsKind>) -> Project {
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Guarded drag independent integration".into(),
        width: WIDTH,
        height: HEIGHT,
        fps: 30,
        duration: 90,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::ShapeContents(ShapeContents::default()),
        width: WIDTH as f64,
        height: HEIGHT as f64,
        name: "Drag source".into(),
    })
    .unwrap();
    for kind in kinds {
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Add { parent: 0, kind },
        })
        .unwrap();
    }
    e.project().clone()
}

fn tree(catalog: &Project, items: Vec<ContentsNode>) -> Project {
    let mut value = serde_json::to_value(catalog).unwrap();
    // Keep the catalog's declared schema; hierarchy planning must not migrate it.
    value["composition"]["layers"][0]["content"]["ShapeContents"]["items"] =
        serde_json::to_value(items).unwrap();
    value["composition"]["layers"][0]["content"]["ShapeContents"]["next_id"] = 1000.into();
    Project::from_json(&value.to_string()).unwrap()
}

fn track(value: f64, end: f64) -> AnimatedProperty {
    serde_json::from_value(json!({"value": value, "keys": {
        "0": {"value": value, "interpolation": "Linear"},
        "60": {"value": end, "interpolation": "Hold"}
    }}))
    .unwrap()
}

fn set(node: &mut ContentsNode, parameter: ContentsParam, value: f64) {
    assert!(node.parameters.contains_key(&parameter));
    node.parameters.insert(
        parameter,
        serde_json::from_value(json!({"value": value, "keys": {}})).unwrap(),
    );
}

struct Fixture {
    catalog: Project,
    nodes: BTreeMap<u64, ContentsNode>,
}
impl Fixture {
    fn new() -> Self {
        let catalog = catalog(vec![
            ContentsKind::Group(vec![]),
            ContentsKind::Group(vec![]),
            ContentsKind::Group(vec![]),
            ContentsKind::Group(vec![]),
            path_kind(0.),
            path_kind(10.),
            path_kind(20.),
            ContentsKind::Fill { even_odd: false },
            ContentsKind::Fill { even_odd: true },
            path_kind(30.),
            path_kind(40.),
            ContentsKind::Fill { even_odd: false },
            path_kind(50.),
            ContentsKind::GradientFill {
                even_odd: false,
                gradient: ShapeGradient::default(),
            },
            ContentsKind::TrimPaths,
        ]);
        let remap = [70, 20, 90, 10, 83, 35, 6, 91, 61, 23, 12, 52, 7, 44, 32];
        let mut nodes = BTreeMap::new();
        for (offset, id) in remap.into_iter().enumerate() {
            let mut node = contents(&catalog).node(offset as u64 + 1).unwrap().clone();
            node.id = id;
            node.name = format!("Stable node {id}");
            nodes.insert(id, node);
        }
        nodes.get_mut(&10).unwrap().enabled = false;
        nodes.get_mut(&23).unwrap().enabled = false;
        nodes.get_mut(&35).unwrap().kind = ContentsKind::Path {
            path: path(-10.),
            animation: serde_json::from_value(json!({
                "poses": [path(0.), path(12.), path(42.), path(42.)],
                "timing": {"value": 2., "keys": {
                    "0": {"value": 0., "interpolation": "Linear"},
                    "60": {"value": 1., "interpolation": "Hold"}
                }}
            }))
            .unwrap(),
        };
        nodes
            .get_mut(&20)
            .unwrap()
            .parameters
            .insert(ContentsParam::Transform(Property::ScaleX), track(-100., 0.));
        nodes.get_mut(&70).unwrap().parameters.insert(
            ContentsParam::Transform(Property::PositionX),
            track(0., 80.),
        );
        Self { catalog, nodes }
    }
    fn n(&self, id: u64) -> ContentsNode {
        self.nodes[&id].clone()
    }
    fn g(&self, id: u64, children: Vec<ContentsNode>) -> ContentsNode {
        let mut node = self.n(id);
        assert!(matches!(node.kind, ContentsKind::Group(_)));
        node.kind = ContentsKind::Group(children);
        node
    }
    fn t(&self, items: Vec<ContentsNode>) -> Project {
        tree(&self.catalog, items)
    }
    fn child(&self) -> ContentsNode {
        self.g(90, vec![self.n(6), self.n(91)])
    }
    fn source(&self) -> ContentsNode {
        self.g(
            70,
            vec![self.n(35), self.child(), self.n(61), self.n(44), self.n(32)],
        )
    }
    fn destination(&self) -> ContentsNode {
        self.g(20, vec![self.n(12), self.g(10, vec![]), self.n(52)])
    }
    fn before(&self) -> Project {
        self.t(vec![
            self.n(83),
            self.source(),
            self.n(23),
            self.destination(),
            self.n(7),
        ])
    }
}

fn selection(parent: u64, click_order: &[u64]) -> Selection {
    Selection {
        parent: Some(parent),
        items: click_order.iter().copied().collect(),
        anchor: click_order.first().copied(),
        cursor: click_order.last().copied(),
    }
}

fn editor(project: &Project) -> Editor {
    let mut e = Editor::default();
    e.replace_project(project.clone()).unwrap();
    e.clear_history();
    e
}

fn records(project: &Project) -> BTreeMap<u64, ContentsNode> {
    contents(project)
        .rows()
        .into_iter()
        .map(|(_, _, node)| {
            let mut record = node.clone();
            if let ContentsKind::Group(children) = &mut record.kind {
                children.clear();
            }
            (record.id, record)
        })
        .collect()
}

fn assert_locals(before: &Project, after: &Project) {
    assert_eq!(
        records(before),
        records(after),
        "Every local payload/unused pose stays exact"
    );
    let before_json = serde_json::to_value(before).unwrap();
    let after_json = serde_json::to_value(after).unwrap();
    assert_eq!(before_json["version"], after_json["version"]);
    assert_eq!(
        serde_json::to_value(contents(before)).unwrap()["next_id"],
        serde_json::to_value(contents(after)).unwrap()["next_id"]
    );
    let before_layer = before.composition().layer(1).unwrap();
    let after_layer = after.composition().layer(1).unwrap();
    for address in before_layer.track_paths() {
        assert_eq!(before_layer.track(address), after_layer.track(address));
        assert_eq!(
            before_layer.track_label(address),
            after_layer.track_label(address)
        );
    }
}

fn assert_one_transaction(e: &mut Editor, before: &Project, after: &Project) {
    assert_eq!(e.project(), after);
    assert_eq!(e.selected(), Some(1));
    assert!(e.can_undo());
    assert!(!e.can_redo());
    e.undo();
    assert_eq!(e.project(), before);
    assert_eq!(e.selected(), Some(1));
    assert!(!e.can_undo(), "One drop is exactly one history record");
    e.redo();
    assert_eq!(e.project(), after);
    assert!(!e.can_redo());
}

fn assert_drop(before: &Project, expected: &Project, selected: &Selection, target: DropTarget) {
    let mut e = editor(before);
    let frozen_selection = selected.clone();
    let plan = plan_drop(contents(before), selected, target).unwrap();
    assert_eq!(plan.target, target);
    let edit = plan.edit().expect("Expected a nonidentity drop");
    if plan.source_parent == plan.parent {
        assert!(matches!(edit, ContentsEdit::Reorder { .. }));
    } else {
        assert!(matches!(&edit, ContentsEdit::MoveSiblings {
            source_parent, items, parent, index
        } if *source_parent == plan.source_parent && items == &plan.items
            && *parent == plan.parent && *index == plan.index));
    }
    assert_eq!(e.project(), before, "Planning must not mutate the source");
    assert_eq!(selected, &frozen_selection);
    assert!(!e.can_undo() && !e.can_redo());
    e.execute(Command::Contents { id: 1, edit }).unwrap();
    assert_eq!(
        e.project(),
        expected,
        "Complete independent document must match"
    );
    assert_eq!(e.project().to_json().unwrap(), expected.to_json().unwrap());
    assert_locals(before, e.project());
    assert_one_transaction(&mut e, before, expected);
}

#[test]
fn root_noncontiguous_targets_use_source_visual_order_for_real_editor_commands() {
    let f = Fixture::new();
    let before = f.before();
    let selected = selection(0, &[7, 23, 83]);
    let cases = [
        (
            DropTarget::Before(12),
            f.t(vec![
                f.source(),
                f.g(
                    20,
                    vec![f.n(83), f.n(23), f.n(7), f.n(12), f.g(10, vec![]), f.n(52)],
                ),
            ]),
        ),
        (
            DropTarget::After(12),
            f.t(vec![
                f.source(),
                f.g(
                    20,
                    vec![f.n(12), f.n(83), f.n(23), f.n(7), f.g(10, vec![]), f.n(52)],
                ),
            ]),
        ),
        (
            DropTarget::Into(10),
            f.t(vec![
                f.source(),
                f.g(
                    20,
                    vec![f.n(12), f.g(10, vec![f.n(83), f.n(23), f.n(7)]), f.n(52)],
                ),
            ]),
        ),
        (
            DropTarget::Into(20),
            f.t(vec![
                f.source(),
                f.g(
                    20,
                    vec![f.n(12), f.g(10, vec![]), f.n(52), f.n(83), f.n(23), f.n(7)],
                ),
            ]),
        ),
        (
            DropTarget::RootEnd,
            f.t(vec![f.source(), f.destination(), f.n(83), f.n(23), f.n(7)]),
        ),
    ];
    for (target, expected) in cases {
        assert_eq!(
            plan_drop(contents(&before), &selected, target)
                .unwrap()
                .items,
            [83, 23, 7]
        );
        assert_drop(&before, &expected, &selected, target);
    }
}

#[test]
fn nested_noncontiguous_and_complete_group_subtrees_have_literal_destination_trees() {
    let f = Fixture::new();
    let before = f.before();
    let selected = selection(70, &[61, 35]);
    let remainder = || f.g(70, vec![f.child(), f.n(44), f.n(32)]);
    for (target, expected) in [
        (
            DropTarget::RootEnd,
            f.t(vec![
                f.n(83),
                remainder(),
                f.n(23),
                f.destination(),
                f.n(7),
                f.n(35),
                f.n(61),
            ]),
        ),
        (
            DropTarget::Before(23),
            f.t(vec![
                f.n(83),
                remainder(),
                f.n(35),
                f.n(61),
                f.n(23),
                f.destination(),
                f.n(7),
            ]),
        ),
        (
            DropTarget::After(10),
            f.t(vec![
                f.n(83),
                remainder(),
                f.n(23),
                f.g(
                    20,
                    vec![f.n(12), f.g(10, vec![]), f.n(35), f.n(61), f.n(52)],
                ),
                f.n(7),
            ]),
        ),
        (
            DropTarget::Into(10),
            f.t(vec![
                f.n(83),
                remainder(),
                f.n(23),
                f.g(20, vec![f.n(12), f.g(10, vec![f.n(35), f.n(61)]), f.n(52)]),
                f.n(7),
            ]),
        ),
        (
            DropTarget::Into(70),
            f.t(vec![
                f.n(83),
                f.g(70, vec![f.child(), f.n(44), f.n(32), f.n(35), f.n(61)]),
                f.n(23),
                f.destination(),
                f.n(7),
            ]),
        ),
    ] {
        assert_drop(&before, &expected, &selected, target);
    }
    let selected = selection(70, &[32, 90]);
    let expected = f.t(vec![
        f.n(83),
        f.g(70, vec![f.n(35), f.n(61), f.n(44)]),
        f.n(23),
        f.g(
            20,
            vec![f.n(12), f.g(10, vec![]), f.n(52), f.child(), f.n(32)],
        ),
        f.n(7),
    ]);
    assert_drop(&before, &expected, &selected, DropTarget::Into(20));
    assert_eq!(contents(&before).node(90), contents(&expected).node(90));
}

// Seed two distinct snapshots, then park between them. Checking Undo and Redo all
// the way to their ends detects a hidden no-op transaction or a cleared Redo stack.
fn historical(project: &Project) -> (Editor, Project, Project) {
    let mut e = editor(project);
    for name in ["History first", "History second"] {
        e.execute(Command::Contents {
            id: 1,
            edit: ContentsEdit::Rename {
                item: 70,
                name: name.into(),
            },
        })
        .unwrap();
    }
    let future = e.project().clone();
    e.undo();
    let current = e.project().clone();
    (e, current, future)
}

fn assert_histories_untouched(e: &mut Editor, base: &Project, current: &Project, future: &Project) {
    assert_eq!(e.project(), current);
    assert!(e.can_undo() && e.can_redo());
    e.redo();
    assert_eq!(e.project(), future);
    assert!(!e.can_redo());
    e.undo();
    assert_eq!(e.project(), current);
    e.undo();
    assert_eq!(e.project(), base);
    assert!(!e.can_undo());
}

#[test]
fn selected_sibling_boundaries_and_append_identity_preserve_both_histories() {
    let f = Fixture::new();
    let base = f.before();
    for (selected, target) in [
        (selection(70, &[61, 90]), DropTarget::Before(90)),
        (selection(70, &[61, 90]), DropTarget::After(61)),
        (selection(70, &[32, 44]), DropTarget::Into(70)),
        (selection(0, &[7]), DropTarget::RootEnd),
        (selection(0, &[7]), DropTarget::After(7)),
    ] {
        let (mut e, current, future) = historical(&base);
        let collapsed: BTreeSet<_> = [10, 90].into();
        let disclosure_before = collapsed.clone();
        let selected_before = selected.clone();
        let plan = plan_drop(contents(e.project()), &selected, target).unwrap();
        assert!(
            plan.edit().is_none(),
            "Identity must not dispatch a command: {target:?}"
        );
        assert_eq!(selected, selected_before);
        assert_eq!(collapsed, disclosure_before);
        assert_histories_untouched(&mut e, &base, &current, &future);
    }
}

#[test]
fn invalid_semantic_targets_and_stale_selections_never_dispatch_or_disturb_redo() {
    let f = Fixture::new();
    let base = f.before();
    for (selected, target) in [
        (selection(0, &[70]), DropTarget::Into(70)),
        (selection(0, &[70]), DropTarget::Into(90)),
        (selection(0, &[70]), DropTarget::Before(6)),
        (selection(70, &[90]), DropTarget::After(91)),
        (selection(70, &[35]), DropTarget::Into(35)),
        (selection(70, &[35]), DropTarget::Before(999)),
        (selection(70, &[35]), DropTarget::Into(999)),
        (selection(70, &[35, 12]), DropTarget::RootEnd),
        (selection(70, &[35, 999]), DropTarget::RootEnd),
        (selection(0, &[0]), DropTarget::RootEnd),
        (selection(999, &[35]), DropTarget::RootEnd),
        (selection(70, &[]), DropTarget::RootEnd),
    ] {
        let (mut e, current, future) = historical(&base);
        assert!(
            plan_drop(contents(e.project()), &selected, target).is_none(),
            "Unexpected valid plan: {selected:?} {target:?}"
        );
        assert_histories_untouched(&mut e, &base, &current, &future);
    }
}

#[test]
fn real_planned_commands_reject_stale_indices_and_locked_layers_atomically() {
    let f = Fixture::new();
    let original = f.before();
    let selected = selection(70, &[35]);
    let planned = plan_drop(contents(&original), &selected, DropTarget::After(52)).unwrap();
    assert_eq!(planned.index, 3);
    // Removing destination node 52 independently makes the already planned index
    // stale. The UI separately rejects stale full-source snapshots; core must also
    // reject this particular stale command rather than clamp its insertion index.
    let shortened = f.t(vec![
        f.n(83),
        f.source(),
        f.n(23),
        f.g(20, vec![f.n(12), f.g(10, vec![])]),
        f.n(7),
    ]);
    let mut locked_json = serde_json::to_value(&original).unwrap();
    locked_json["composition"]["layers"][0]["locked"] = true.into();
    let locked = Project::from_json(&locked_json.to_string()).unwrap();
    for base in [&shortened, &locked] {
        // Rename the unlocked source to seed histories, then lock through an
        // independent loaded source where appropriate; history seeding itself
        // must not need a locked-layer edit.
        let mut e = editor(base);
        let mut first_json = serde_json::to_value(base).unwrap();
        first_json["composition"]["name"] = "First history composition".into();
        let first = Project::from_json(&first_json.to_string()).unwrap();
        e.replace_project(first.clone()).unwrap();
        let mut second_json = first_json;
        second_json["composition"]["name"] = "Second history composition".into();
        let second = Project::from_json(&second_json.to_string()).unwrap();
        e.replace_project(second.clone()).unwrap();
        e.undo();
        assert!(
            e.execute(Command::Contents {
                id: 1,
                edit: planned.edit().unwrap()
            })
            .is_err()
        );
        assert_histories_untouched(&mut e, base, &first, &second);
    }
}

fn ladder(f: &Fixture, last_children: Vec<ContentsNode>) -> ContentsNode {
    let mut current = f.g(10, last_children);
    current.id = 107;
    for id in (100..107).rev() {
        let mut parent = f.g(10, vec![current]);
        parent.id = id;
        current = parent;
    }
    current
}

#[test]
fn planned_commands_respect_actual_empty_group_depth_and_256_node_budget() {
    let f = Fixture::new();
    let before = f.t(vec![ladder(&f, vec![]), f.g(70, vec![]), f.n(83)]);
    let expected = f.t(vec![ladder(&f, vec![f.n(83)]), f.g(70, vec![])]);
    assert_drop(
        &before,
        &expected,
        &selection(0, &[83]),
        DropTarget::Into(107),
    );
    assert!(
        plan_drop(
            contents(&before),
            &selection(0, &[70]),
            DropTarget::Into(107)
        )
        .is_none()
    );
    let mut boundary = f.g(10, vec![]);
    boundary.id = 107;
    let mut parent = f.g(10, vec![boundary, f.g(70, vec![])]);
    parent.id = 106;
    for id in (100..106).rev() {
        let mut outer = f.g(10, vec![parent]);
        outer.id = id;
        parent = outer;
    }
    let expected = f.t(vec![parent, f.n(83)]);
    assert_drop(
        &before,
        &expected,
        &selection(0, &[70]),
        DropTarget::Into(106),
    );

    let leaves: Vec<_> = (200..455)
        .map(|id| {
            let mut node = f.n(83);
            node.id = id;
            node
        })
        .collect();
    let mut root = vec![f.g(70, vec![])];
    root.extend(leaves.clone());
    let full = f.t(root);
    let mut expected_root = vec![f.g(70, vec![leaves[0].clone(), leaves[254].clone()])];
    expected_root.extend_from_slice(&leaves[1..254]);
    let expected = f.t(expected_root);
    assert_drop(
        &full,
        &expected,
        &selection(0, &[454, 200]),
        DropTarget::Into(70),
    );
    let mut oversized = serde_json::to_value(contents(&full)).unwrap();
    let mut extra = f.n(83);
    extra.id = 900;
    oversized["items"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(extra).unwrap());
    let oversized: ShapeContents = serde_json::from_value(oversized).unwrap();
    assert!(plan_drop(&oversized, &selection(0, &[200]), DropTarget::Into(70)).is_none());
}

#[test]
fn planned_move_preserves_declared_schema_complete_json_lep_and_desktop_view() {
    let f = Fixture::new();
    let before = f.before();
    let expected = f.t(vec![
        f.n(83),
        f.g(70, vec![f.n(35), f.n(61), f.n(44), f.n(32)]),
        f.n(23),
        f.g(20, vec![f.n(12), f.g(10, vec![]), f.n(52), f.child()]),
        f.n(7),
    ]);
    let mut views = ProjectViews::default();
    let mut view = CompositionView::default();
    view.frame = 30;
    view.graph_view.height = Some([-40., 120.]);
    views
        .compositions
        .insert(before.active_composition_id(), view);
    views.normalize(&before);
    let original_view = views.encode_native(&before).unwrap();
    let mut e = editor(&before);
    let plan = plan_drop(
        contents(&before),
        &selection(70, &[90]),
        DropTarget::Into(20),
    )
    .unwrap();
    e.execute(Command::Contents {
        id: 1,
        edit: plan.edit().unwrap(),
    })
    .unwrap();
    assert_eq!(e.project(), &expected);
    assert_locals(&before, e.project());
    assert_eq!(views.encode_native(e.project()).unwrap(), original_view);
    let json = e.project().to_json().unwrap();
    assert_eq!(Project::from_json(&json).unwrap(), expected);
    let encoded = crate::project_io::encode_native_project(e.project(), Some(&views)).unwrap();
    assert_eq!(&encoded[..8], project_file::MAGIC);
    assert_eq!(&encoded[8..10], &[1, 0]);
    let decoded = crate::project_io::decode_project(&encoded).unwrap();
    assert_eq!(decoded.project, expected);
    assert_eq!(decoded.views, views);
    assert_eq!(
        crate::project_io::encode_native_project(&decoded.project, Some(&decoded.views)).unwrap(),
        encoded
    );
    assert_one_transaction(&mut e, &before, &expected);
    assert_eq!(views.encode_native(e.project()).unwrap(), original_view);
}

fn geometry(project: &Project) -> Geometry {
    let visible = visible_rows(contents(project), &BTreeSet::new());
    let clip = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(800.)));
    let rows = visible
        .iter()
        .enumerate()
        .map(|(index, &(depth, parent, item))| RowGeometry {
            item,
            parent,
            depth,
            group: matches!(
                contents(project).node(item).unwrap().kind,
                ContentsKind::Group(_)
            ),
            bounds: Bounds::new(
                point(px(10.), px(index as f32 * 26.)),
                size(px(300.), px(26.)),
            ),
            label: Bounds::new(
                point(px(40. + depth as f32 * 16.), px(index as f32 * 26.)),
                size(px(220. - depth as f32 * 16.), px(26.)),
            ),
            clip,
        })
        .collect();
    let root = Some(RootGeometry {
        bounds: Bounds::new(
            point(px(10.), px(visible.len() as f32 * 26.)),
            size(px(300.), px(26.)),
        ),
        clip,
    });
    Geometry {
        owner: (project.active_composition_id(), 1),
        visible,
        rows,
        root,
    }
}

fn export_case(before: &Project, after: &Project, references: &[(u32, Project)]) {
    use std::io::Write;
    let Some(directory) = std::env::var_os("LIBREEFFECTS_EXPORT_GUARDED_DROP_FIXTURES") else {
        return;
    };
    let root = std::path::PathBuf::from(directory);
    assert!(
        root.is_absolute(),
        "Fixture export directory must be absolute"
    );
    std::fs::create_dir_all(&root).unwrap();
    let write = |name: &str, bytes: &[u8]| {
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
    for (suffix, project) in [("before", before), ("planned", after)] {
        write(
            &format!("guarded-drag-{suffix}.lfe.json"),
            project.to_json().unwrap().as_bytes(),
        );
        write(
            &format!("guarded-drag-{suffix}-generated.lep"),
            &project_file::encode(project, None).unwrap(),
        );
    }
    for (frame, project) in references {
        write(
            &format!("guarded-drag-reference-{frame}.lfe.json"),
            project.to_json().unwrap().as_bytes(),
        );
    }
}

#[test]
fn geometry_target_plan_editor_and_exact_rgba_match_two_independent_output_references() {
    let base = catalog(vec![
        ContentsKind::Group(vec![]),
        ContentsKind::Group(vec![]),
        path_kind(0.),
        path_kind(80.),
        ContentsKind::Fill { even_odd: false },
    ]);
    let n = |id| contents(&base).node(id).unwrap().clone();
    let g = |id, children| {
        let mut node = n(id);
        node.kind = ContentsKind::Group(children);
        node
    };
    let mut disabled = n(4);
    disabled.enabled = false;
    let mut paint = n(5);
    for (channel, value) in [
        (ShapeParam::FillRed, 255.),
        (ShapeParam::FillGreen, 0.),
        (ShapeParam::FillBlue, 0.),
    ] {
        set(&mut paint, ContentsParam::Shape(channel), value);
    }
    let mut destination = g(2, vec![]);
    set(
        &mut destination,
        ContentsParam::Transform(Property::ScaleX),
        -100.,
    );
    set(
        &mut destination,
        ContentsParam::Transform(Property::PositionY),
        50.,
    );
    destination.parameters.insert(
        ContentsParam::Transform(Property::PositionX),
        track(180., 200.),
    );
    let before = tree(
        &base,
        vec![
            g(1, vec![n(3), disabled.clone(), paint.clone()]),
            destination.clone(),
        ],
    );
    destination.kind = ContentsKind::Group(vec![n(3), paint.clone()]);
    let expected = tree(&base, vec![g(1, vec![disabled]), destination]);
    let selected = selection(1, &[5, 3]);
    let geometry = geometry(&before);
    // Group 2 is the fifth visible row; its label center unambiguously means Into.
    let preview = geometry
        .resolve(contents(&before), &selected, point(px(100.), px(117.)))
        .unwrap();
    assert_eq!(preview.plan.target, DropTarget::Into(2));
    assert_eq!(preview.plan.items, [3, 5]);
    let mut e = editor(&before);
    e.execute(Command::Contents {
        id: 1,
        edit: preview.plan.edit().unwrap(),
    })
    .unwrap();
    assert_eq!(e.project(), &expected);
    assert_locals(&before, e.project());
    let renderer = Renderer::new();
    let mut references = Vec::new();
    for (frame, left) in [(0, 110u32), (60, 130u32)] {
        let mut baked = n(3);
        baked.kind = ContentsKind::Path {
            path: VectorPath {
                closed: true,
                vertices: [
                    [left as f64 + 60., 60.],
                    [left as f64, 60.],
                    [left as f64, 100.],
                    [left as f64 + 60., 100.],
                ]
                .into_iter()
                .map(PathVertex::corner)
                .collect(),
            },
            animation: PathAnimation::default(),
        };
        let reference = tree(&base, vec![baked, paint.clone()]);
        let actual = renderer
            .render_output(e.project(), frame, WIDTH, HEIGHT)
            .unwrap();
        let independent = renderer
            .render_output(&reference, frame, WIDTH, HEIGHT)
            .unwrap();
        assert_eq!(actual, independent);
        let analytic = image::RgbaImage::from_fn(WIDTH, HEIGHT, |x, y| {
            image::Rgba(
                if (left..left + 60).contains(&x) && (60..100).contains(&y) {
                    [255, 0, 0, 255]
                } else {
                    [0; 4]
                },
            )
        });
        assert_eq!(
            actual, analytic,
            "Exact independent rectangle RGBA at frame {frame}"
        );
        assert_eq!(
            actual,
            renderer.render_preview(e.project(), frame, WIDTH).unwrap()
        );
        assert_ne!(
            actual,
            renderer
                .render_output(&before, frame, WIDTH, HEIGHT)
                .unwrap()
        );
        references.push((frame, reference));
    }
    export_case(&before, e.project(), &references);
    assert_one_transaction(&mut e, &before, &expected);
}
