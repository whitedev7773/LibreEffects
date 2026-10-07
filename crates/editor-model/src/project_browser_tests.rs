use super::*;
use libre_effects_core::{AssetId, AudioMetadata, Command, Editor};

fn folder(editor: &mut Editor, name: &str, parent: Option<FolderId>) -> FolderId {
    editor
        .execute(Command::NewProjectFolder {
            name: name.into(),
            parent,
        })
        .unwrap();
    *editor
        .project()
        .asset_library()
        .folders()
        .keys()
        .last()
        .unwrap()
}

fn asset(editor: &mut Editor, name: &str, parent: Option<FolderId>, content: Content) -> AssetId {
    editor
        .execute(Command::ImportAsset {
            content,
            // Distinct dimensions keep equal-source fixture assets independent.
            width: 20.0 + editor.project().asset_library().assets().len() as f64,
            height: 10.0,
            name: name.into(),
            folder: parent,
            frame: None,
        })
        .unwrap();
    *editor
        .project()
        .asset_library()
        .assets()
        .keys()
        .last()
        .unwrap()
}

fn image(editor: &mut Editor, name: &str, parent: Option<FolderId>) -> AssetId {
    asset(editor, name, parent, Content::Image { png: "YWJj".into() })
}

fn rename(editor: &mut Editor, item: ProjectItem, name: &str) {
    editor
        .execute(Command::RenameProjectItem {
            item,
            name: name.into(),
        })
        .unwrap();
}

fn fixture() -> Editor {
    let mut editor = Editor::default();
    assert_eq!(folder(&mut editor, "Media", None), 1);
    assert_eq!(folder(&mut editor, "Nested", Some(1)), 2);
    assert_eq!(image(&mut editor, "Zulu", Some(2)), 3);
    assert_eq!(image(&mut editor, "Alpha", None), 4);
    editor.execute(Command::NewComposition).unwrap();
    rename(&mut editor, ProjectItem::Composition(2), "Nested comp");
    editor
        .execute(Command::MoveProjectItem {
            item: ProjectItem::Composition(2),
            folder: Some(2),
        })
        .unwrap();
    editor
}

fn ids(rows: &[Row]) -> Vec<ProjectItem> {
    rows.iter().map(|row| row.item).collect()
}

fn names(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|row| row.name.as_str()).collect()
}

#[test]
fn every_item_variant_has_exactly_one_specific_type() {
    assert_eq!(ItemType::default(), ItemType::All);
    assert_eq!(
        ItemType::ALL.map(ItemType::label),
        ["All types", "Compositions", "Footage", "Folders"]
    );
    for (item, expected) in [
        (ProjectItem::Asset(1), ItemType::Footage),
        (ProjectItem::Folder(1), ItemType::Folders),
        (ProjectItem::Composition(1), ItemType::Compositions),
    ] {
        for filter in ItemType::ALL {
            assert_eq!(
                filter.matches(item),
                filter == ItemType::All || filter == expected
            );
        }
    }
}

#[test]
fn unfiltered_rows_keep_sorted_hierarchy_collapse_and_stable_identity() {
    let editor = fixture();
    let project = editor.project();
    let visible = rows(project, "", ItemType::All, false, false, &BTreeSet::new());
    assert_eq!(
        names(&visible),
        [
            "Alpha",
            "Composition 01",
            "Media",
            "Nested",
            "Nested comp",
            "Zulu"
        ]
    );
    assert_eq!(
        visible.iter().map(|row| row.depth).collect::<Vec<_>>(),
        [0, 0, 0, 1, 2, 2]
    );
    assert_eq!(
        ids(&visible),
        [
            ProjectItem::Asset(4),
            ProjectItem::Composition(1),
            ProjectItem::Folder(1),
            ProjectItem::Folder(2),
            ProjectItem::Composition(2),
            ProjectItem::Asset(3)
        ]
    );
    assert_eq!(
        names(&rows(
            project,
            "",
            ItemType::All,
            false,
            true,
            &BTreeSet::new()
        )),
        [
            "Media",
            "Nested",
            "Zulu",
            "Nested comp",
            "Composition 01",
            "Alpha"
        ]
    );
    assert_eq!(
        names(&rows(
            project,
            "",
            ItemType::All,
            true,
            false,
            &BTreeSet::new()
        )),
        [
            "Composition 01",
            "Media",
            "Nested",
            "Nested comp",
            "Zulu",
            "Alpha"
        ]
    );
    for (collapsed, expected) in [
        (
            BTreeSet::from([1]),
            vec!["Alpha", "Composition 01", "Media"],
        ),
        (
            BTreeSet::from([2]),
            vec!["Alpha", "Composition 01", "Media", "Nested"],
        ),
    ] {
        assert_eq!(
            names(&rows(project, "", ItemType::All, false, false, &collapsed)),
            expected
        );
    }
    assert_eq!(
        visible,
        rows(
            project,
            " \t\n",
            ItemType::All,
            false,
            false,
            &BTreeSet::new()
        )
    );
}

#[test]
fn text_search_is_trimmed_case_insensitive_unicode_and_matches_name_or_kind() {
    let mut editor = fixture();
    rename(&mut editor, ProjectItem::Asset(3), "CAFÉ 東京 ✨");
    for query in [" café ", "CAFÉ", "\t東京\n", "✨"] {
        let visible = rows(
            editor.project(),
            query,
            ItemType::All,
            false,
            false,
            &BTreeSet::from([1]),
        );
        assert_eq!(ids(&visible), [ProjectItem::Asset(3)]);
        assert_eq!(visible[0].depth, 0);
    }
    let images = rows(
        editor.project(),
        " IMAGE ",
        ItemType::All,
        false,
        true,
        &BTreeSet::new(),
    );
    assert_eq!(ids(&images), [ProjectItem::Asset(3), ProjectItem::Asset(4)]);
    let folders = rows(
        editor.project(),
        "fOlDeR",
        ItemType::All,
        false,
        false,
        &BTreeSet::new(),
    );
    assert_eq!(
        ids(&folders),
        [ProjectItem::Folder(1), ProjectItem::Folder(2)]
    );
    assert_eq!(
        ids(&rows(
            editor.project(),
            "comp",
            ItemType::All,
            false,
            false,
            &BTreeSet::new()
        )),
        [ProjectItem::Composition(1), ProjectItem::Composition(2)]
    );
}

#[test]
fn type_filters_reveal_collapsed_matches_and_retain_parent_context() {
    let editor = fixture();
    let project = editor.project();
    let collapsed = BTreeSet::from([1, 2]);
    for (filter, expected) in [
        (
            ItemType::Footage,
            vec![ProjectItem::Asset(4), ProjectItem::Asset(3)],
        ),
        (
            ItemType::Compositions,
            vec![ProjectItem::Composition(1), ProjectItem::Composition(2)],
        ),
        (
            ItemType::Folders,
            vec![ProjectItem::Folder(1), ProjectItem::Folder(2)],
        ),
    ] {
        let visible = rows(project, "", filter, false, false, &collapsed);
        assert_eq!(ids(&visible), expected);
        assert!(visible.iter().all(|row| row.depth == 0));
        let nested = visible.last().unwrap();
        let expected_parent = if filter == ItemType::Folders {
            Some(1)
        } else {
            Some(2)
        };
        assert_eq!(nested.folder, expected_parent);
        assert_eq!(
            folder_path(project, nested.folder),
            if filter == ItemType::Folders {
                "Media"
            } else {
                "Media / Nested"
            }
        );
    }
    assert_eq!(folder_path(project, None), "Project");
    assert_eq!(folder_path(project, Some(999)), "Project");
    assert_eq!(collapsed, BTreeSet::from([1, 2]));
    assert_eq!(
        names(&rows(project, "", ItemType::All, false, false, &collapsed)),
        ["Alpha", "Composition 01", "Media"]
    );
}

#[test]
fn name_or_kind_search_combines_with_item_type_using_and() {
    let mut editor = fixture();
    rename(&mut editor, ProjectItem::Folder(1), "Image bin");
    rename(&mut editor, ProjectItem::Composition(2), "Alpha comp");
    for (query, filter, expected) in [
        (
            "image",
            ItemType::All,
            vec![
                ProjectItem::Asset(4),
                ProjectItem::Folder(1),
                ProjectItem::Asset(3),
            ],
        ),
        (
            "image",
            ItemType::Footage,
            vec![ProjectItem::Asset(4), ProjectItem::Asset(3)],
        ),
        ("image", ItemType::Folders, vec![ProjectItem::Folder(1)]),
        ("image", ItemType::Compositions, vec![]),
        (
            "alpha",
            ItemType::All,
            vec![ProjectItem::Asset(4), ProjectItem::Composition(2)],
        ),
        (
            "alpha",
            ItemType::Compositions,
            vec![ProjectItem::Composition(2)],
        ),
        ("alpha", ItemType::Footage, vec![ProjectItem::Asset(4)]),
        ("alpha", ItemType::Folders, vec![]),
        ("folder", ItemType::Footage, vec![]),
    ] {
        let visible = rows(
            editor.project(),
            query,
            filter,
            false,
            false,
            &BTreeSet::from([1]),
        );
        assert_eq!(ids(&visible), expected, "query {query}, filter {filter:?}");
        assert!(visible.iter().all(|row| row.depth == 0));
    }
}

#[test]
fn footage_includes_every_asset_kind_with_existing_kind_labels() {
    let mut editor = Editor::default();
    let audio = AudioMetadata {
        stream_index: 0,
        sample_rate: 48_000,
        channels: 2,
        channel_layout: "stereo".into(),
        duration: 5.0,
        start_time: 0.0,
        file_offset: 0.0,
    };
    let contents = [
        ("Image", Content::Image { png: "YWJj".into() }),
        (
            "Sequence",
            Content::ImageSequence {
                frames: vec!["frame.png".into()].into(),
                fps: 30.into(),
                missing: Default::default(),
                start_frame: 0,
                playback: Default::default(),
            },
        ),
        (
            "Video",
            Content::Video {
                path: "clip.mp4".into(),
                audio: None,
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
        ),
        (
            "Video + Audio",
            Content::Video {
                path: "sound-clip.mp4".into(),
                audio: Some(audio.clone()),
                duration: 5.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
        ),
        (
            "Audio",
            Content::Audio {
                path: "sound.wav".into(),
                audio,
                start_frame: 0,
                playback: Default::default(),
            },
        ),
    ];
    let expected: Vec<_> = contents
        .into_iter()
        .enumerate()
        .map(|(i, (kind, content))| {
            (
                ProjectItem::Asset(asset(&mut editor, &format!("Source {i}"), None, content)),
                kind,
            )
        })
        .collect();
    let visible = rows(
        editor.project(),
        "",
        ItemType::Footage,
        false,
        false,
        &BTreeSet::new(),
    );
    assert_eq!(
        visible
            .iter()
            .map(|row| (row.item, row.kind))
            .collect::<Vec<_>>(),
        expected
    );
    assert!(visible.iter().all(|row| row.folder.is_none()));
    assert_eq!(
        rows(
            editor.project(),
            "audio",
            ItemType::Footage,
            false,
            false,
            &BTreeSet::new()
        )
        .iter()
        .map(|row| row.kind)
        .collect::<Vec<_>>(),
        ["Video + Audio", "Audio"]
    );
}

#[test]
fn equal_labels_use_deterministic_identity_tiebreaks_for_every_sort_mode() {
    let mut editor = Editor::default();
    folder(&mut editor, "same", None);
    folder(&mut editor, "SAME", None);
    image(&mut editor, "SAME", None);
    image(&mut editor, "same", None);
    rename(&mut editor, ProjectItem::Composition(1), "same");
    editor.execute(Command::NewComposition).unwrap();
    rename(&mut editor, ProjectItem::Composition(2), "SAME");
    for (by_type, expected) in [
        (
            false,
            vec![
                ProjectItem::Asset(3),
                ProjectItem::Asset(4),
                ProjectItem::Folder(1),
                ProjectItem::Folder(2),
                ProjectItem::Composition(1),
                ProjectItem::Composition(2),
            ],
        ),
        (
            true,
            vec![
                ProjectItem::Composition(1),
                ProjectItem::Composition(2),
                ProjectItem::Folder(1),
                ProjectItem::Folder(2),
                ProjectItem::Asset(3),
                ProjectItem::Asset(4),
            ],
        ),
    ] {
        for query in ["", "same"] {
            assert_eq!(
                ids(&rows(
                    editor.project(),
                    query,
                    ItemType::All,
                    by_type,
                    false,
                    &BTreeSet::new()
                )),
                expected
            );
            assert_eq!(
                ids(&rows(
                    editor.project(),
                    query,
                    ItemType::All,
                    by_type,
                    true,
                    &BTreeSet::new()
                )),
                expected.iter().rev().copied().collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn navigation_follows_filtered_visible_sort_order_and_clamps_endpoints() {
    let editor = fixture();
    for descending in [false, true] {
        let visible = rows(
            editor.project(),
            "image",
            ItemType::Footage,
            true,
            descending,
            &BTreeSet::from([1]),
        );
        assert_eq!(visible.len(), 2);
        let first = Some(visible[0].item);
        let last = Some(visible[1].item);
        assert_eq!(select(&visible, first, Navigation::Next), last);
        assert_eq!(select(&visible, last, Navigation::Previous), first);
        assert_eq!(select(&visible, first, Navigation::Previous), first);
        assert_eq!(select(&visible, last, Navigation::Next), last);
        assert_eq!(select(&visible, last, Navigation::First), first);
        assert_eq!(select(&visible, first, Navigation::Last), last);
    }
    let one = rows(
        editor.project(),
        "zulu",
        ItemType::All,
        false,
        false,
        &BTreeSet::new(),
    );
    for direction in [
        Navigation::Previous,
        Navigation::Next,
        Navigation::First,
        Navigation::Last,
    ] {
        assert_eq!(select(&one, None, direction), Some(ProjectItem::Asset(3)));
        assert_eq!(
            select(&one, Some(ProjectItem::Asset(3)), direction),
            Some(ProjectItem::Asset(3))
        );
    }
}

#[test]
fn navigation_uses_displayed_tree_order_and_skips_collapsed_descendants() {
    let editor = fixture();
    for collapsed in [BTreeSet::new(), BTreeSet::from([1]), BTreeSet::from([2])] {
        let visible = rows(editor.project(), "", ItemType::All, true, true, &collapsed);
        for pair in visible.windows(2) {
            assert_eq!(
                select(&visible, Some(pair[0].item), Navigation::Next),
                Some(pair[1].item)
            );
            assert_eq!(
                select(&visible, Some(pair[1].item), Navigation::Previous),
                Some(pair[0].item)
            );
        }
        if !collapsed.is_empty() {
            assert!(!visible.iter().any(|row| row.item == ProjectItem::Asset(3)));
            assert_eq!(
                select(&visible, Some(ProjectItem::Asset(3)), Navigation::Next),
                Some(visible[0].item)
            );
        }
    }
}

#[test]
fn absent_or_hidden_selection_starts_at_directional_endpoint() {
    let editor = fixture();
    let visible = rows(
        editor.project(),
        "",
        ItemType::Footage,
        false,
        false,
        &BTreeSet::from([1]),
    );
    for current in [
        None,
        Some(ProjectItem::Asset(999)),
        Some(ProjectItem::Composition(2)),
        Some(ProjectItem::Folder(1)),
    ] {
        for direction in [Navigation::Next, Navigation::First] {
            assert_eq!(
                select(&visible, current, direction),
                Some(ProjectItem::Asset(4))
            );
        }
        for direction in [Navigation::Previous, Navigation::Last] {
            assert_eq!(
                select(&visible, current, direction),
                Some(ProjectItem::Asset(3))
            );
        }
    }
}

#[test]
fn empty_and_no_match_views_have_no_navigation_target() {
    let project = Project::default();
    for visible in [
        vec![],
        rows(
            &project,
            "",
            ItemType::Footage,
            false,
            false,
            &BTreeSet::new(),
        ),
        rows(
            &project,
            "missing",
            ItemType::All,
            false,
            false,
            &BTreeSet::new(),
        ),
    ] {
        assert!(visible.is_empty());
        for direction in [
            Navigation::Previous,
            Navigation::Next,
            Navigation::First,
            Navigation::Last,
        ] {
            for current in [None, Some(ProjectItem::Composition(1))] {
                assert_eq!(select(&visible, current, direction), None);
            }
        }
    }
}

#[test]
fn shortcuts_accept_only_unmodified_arrows_home_and_end() {
    for (key, direction) in [
        ("up", Navigation::Previous),
        ("down", Navigation::Next),
        ("home", Navigation::First),
        ("end", Navigation::Last),
    ] {
        assert_eq!(
            shortcut(key, false, false, false, false, false),
            Some(direction)
        );
        for mask in 1..32 {
            assert_eq!(
                shortcut(
                    key,
                    mask & 1 != 0,
                    mask & 2 != 0,
                    mask & 4 != 0,
                    mask & 8 != 0,
                    mask & 16 != 0
                ),
                None
            );
        }
    }
    for key in [
        "", "left", "right", "a", "enter", "delete", "pageup", "pagedown", "Up",
    ] {
        assert_eq!(shortcut(key, false, false, false, false, false), None);
    }
}

#[test]
fn tab_traversal_accepts_only_fresh_tab_or_shift_tab_without_owned_input() {
    for shift in [false, true] {
        for other_modifier in [false, true] {
            for held in [false, true] {
                for input_busy in [false, true] {
                    let expected = if other_modifier || held || input_busy {
                        None
                    } else if shift {
                        Some(TabDirection::Previous)
                    } else {
                        Some(TabDirection::Next)
                    };
                    assert_eq!(
                        tab_direction(shift, other_modifier, held, input_busy),
                        expected,
                        "shift={shift}, other_modifier={other_modifier}, held={held}, input_busy={input_busy}"
                    );
                }
            }
        }
    }
}

#[test]
fn browsing_preserves_source_selection_active_composition_and_history() {
    let mut editor = fixture();
    editor
        .execute(Command::AddAssetLayer { asset: 3, frame: 5 })
        .unwrap();
    rename(&mut editor, ProjectItem::Asset(3), "Temporary");
    let renamed = editor.project().clone();
    editor.undo();
    let before = editor.project().clone();
    let json = before.to_json().unwrap();
    let selected = editor.selected();
    let generation = editor.context_generation();
    let history = (editor.can_undo(), editor.can_redo());
    let collapsed = BTreeSet::from([1, 2]);
    for item_type in ItemType::ALL {
        for query in ["", " \t", "zulu", "image", "nested", "missing"] {
            for by_type in [false, true] {
                for descending in [false, true] {
                    let visible = rows(
                        editor.project(),
                        query,
                        item_type,
                        by_type,
                        descending,
                        &collapsed,
                    );
                    for direction in [
                        Navigation::Previous,
                        Navigation::Next,
                        Navigation::First,
                        Navigation::Last,
                    ] {
                        let current =
                            select(&visible, Some(ProjectItem::Composition(1)), direction);
                        let _ = select(&visible, current, direction);
                    }
                    for row in visible {
                        let _ = folder_path(editor.project(), row.folder);
                    }
                }
            }
        }
    }
    assert_eq!(editor.project(), &before);
    assert_eq!(editor.project().to_json().unwrap(), json);
    assert_eq!(editor.project().active_composition_id(), 2);
    assert_eq!(editor.selected(), selected);
    assert_eq!(editor.context_generation(), generation);
    assert_eq!((editor.can_undo(), editor.can_redo()), history);
    assert_eq!(history, (true, true));
    assert_eq!(collapsed, BTreeSet::from([1, 2]));
    editor.redo();
    assert_eq!(editor.project(), &renamed);
    editor.undo();
    assert_eq!(editor.project(), &before);
}

#[test]
fn project_focus_blocks_retained_timeline_editing_but_preserves_global_shortcuts() {
    for control in [false, true] {
        for alt in [false, true] {
            for key in ["left", "right", "delete", "backspace"] {
                assert!(blocks_layer_shortcut(key, control, alt));
            }
            for key in ["a", "c", "x", "v", "d"] {
                assert_eq!(blocks_layer_shortcut(key, control, alt), control);
            }
            assert_eq!(blocks_layer_shortcut("t", control, alt), control && alt);
            for key in ["[", "]"] {
                assert_eq!(blocks_layer_shortcut(key, control, alt), alt);
            }
            for key in [
                "s", "o", "n", "i", "z", "y", "f", "q", "up", "down", "home", "end", "space",
                "enter", "escape",
            ] {
                assert!(
                    !blocks_layer_shortcut(key, control, alt),
                    "key {key}, control {control}, alt {alt}"
                );
            }
        }
    }
}
