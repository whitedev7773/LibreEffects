//! Session-only Timeline row filtering. Source layers and their order are unchanged.

use libre_effects_core::{Composition, Content, KeyRef, LayerId};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayerTypeFilter {
    #[default]
    All,
    Shape,
    Text,
    Solid,
    Image,
    Sequence,
    Video,
    Audio,
    Precomp,
    Adjustment,
    Null,
}

impl LayerTypeFilter {
    pub const ALL: [Self; 11] = [
        Self::All,
        Self::Shape,
        Self::Text,
        Self::Solid,
        Self::Image,
        Self::Sequence,
        Self::Video,
        Self::Audio,
        Self::Precomp,
        Self::Adjustment,
        Self::Null,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All types",
            Self::Shape => "Shape",
            Self::Text => "Text",
            Self::Solid => "Solid",
            Self::Image => "Image",
            Self::Sequence => "Sequence",
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Precomp => "Precomp",
            Self::Adjustment => "Adjustment",
            Self::Null => "Null",
        }
    }

    pub fn matches(self, content: &Content) -> bool {
        let kind = match content {
            Content::Rectangle | Content::Shape(_) | Content::ShapeContents(_) => Self::Shape,
            Content::Text { .. } => Self::Text,
            Content::Solid => Self::Solid,
            Content::Image { .. } => Self::Image,
            Content::ImageSequence { .. } => Self::Sequence,
            Content::Video { .. } => Self::Video,
            Content::Audio { .. } => Self::Audio,
            Content::Composition { .. } => Self::Precomp,
            Content::Adjustment => Self::Adjustment,
            Content::Null => Self::Null,
        };
        self == Self::All || self == kind
    }
}

/// Return membership, not display order. Render by enumerating `comp.layers()`
/// and checking these stable IDs, retaining the original stack indices.
///
/// Name, type and selection criteria are combined with AND. Hidden shy layers
/// never match; nonmatching parents are not added implicitly.
pub fn matching_layer_ids(
    comp: &Composition,
    query: &str,
    layer_type: LayerTypeFilter,
    selected_only: bool,
    selected: &BTreeSet<LayerId>,
) -> BTreeSet<LayerId> {
    let query = query.trim().to_lowercase();
    comp.layers()
        .iter()
        .filter(|layer| {
            !(comp.hide_shy() && layer.shy())
                && layer_type.matches(layer.content())
                && (!selected_only || selected.contains(&layer.id()))
                && (query.is_empty() || layer.name().to_lowercase().contains(&query))
        })
        .map(|layer| layer.id())
        .collect()
}

/// Select a range in the displayed stack, skipping filtered rows. A hidden or
/// missing anchor starts a new selection at the target; a hidden/missing target
/// cannot select anything. IDs absent from the current composition are ignored.
pub fn selected_visible_range(
    comp: &Composition,
    visible: &BTreeSet<LayerId>,
    anchor: Option<LayerId>,
    target: LayerId,
) -> BTreeSet<LayerId> {
    let rows: Vec<_> = comp
        .layers()
        .iter()
        .filter(|layer| visible.contains(&layer.id()) && !(comp.hide_shy() && layer.shy()))
        .map(|layer| layer.id())
        .collect();
    let Some(target_index) = rows.iter().position(|id| *id == target) else {
        return BTreeSet::new();
    };
    let anchor_index = anchor
        .and_then(|anchor| rows.iter().position(|id| *id == anchor))
        .unwrap_or(target_index);
    rows[anchor_index.min(target_index)..=anchor_index.max(target_index)]
        .iter()
        .copied()
        .collect()
}

/// Resolve a Timeline row click without retaining hidden or stale selections.
/// Shift extends the visible selection by a visible range; Control toggles one
/// visible row; an ordinary click replaces the selection. A hidden target only
/// reconciles the existing selection and never adds a hidden row.
pub fn selected_visible_click(
    comp: &Composition,
    visible: &BTreeSet<LayerId>,
    selected: &BTreeSet<LayerId>,
    anchor: Option<LayerId>,
    target: LayerId,
    toggle: bool,
    range: bool,
) -> BTreeSet<LayerId> {
    let eligible: BTreeSet<_> = comp
        .layers()
        .iter()
        .filter(|layer| visible.contains(&layer.id()) && !(comp.hide_shy() && layer.shy()))
        .map(|layer| layer.id())
        .collect();
    let mut selection: BTreeSet<_> = selected.intersection(&eligible).copied().collect();
    if !eligible.contains(&target) {
        return selection;
    }
    if range {
        selection.extend(selected_visible_range(comp, &eligible, anchor, target));
    } else if toggle {
        if !selection.remove(&target) {
            selection.insert(target);
        }
    } else {
        selection = BTreeSet::from([target]);
    }
    selection
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetScope {
    Layers,
    Keys,
    /// Delete, cut and copy prefer keys whenever any scalar keys are selected.
    Selection,
    /// Multi-layer key clipboards can target IDs absent from current selection.
    Paste,
}

/// An atomic refusal avoids silently editing a subset of a shared selection.
/// Until clipboard destinations are resolved, pasting through the Timeline is
/// conservative whenever any rows are hidden, including with an empty selection.
pub fn blocks_hidden_targets(
    scope: TargetScope,
    visible: &BTreeSet<LayerId>,
    selected_layers: &BTreeSet<LayerId>,
    selected_keys: &BTreeSet<KeyRef>,
    rows_hidden: bool,
) -> bool {
    match scope {
        TargetScope::Layers => selected_layers.iter().any(|id| !visible.contains(id)),
        TargetScope::Keys => selected_keys.iter().any(|key| !visible.contains(&key.id)),
        TargetScope::Selection if !selected_keys.is_empty() => {
            selected_keys.iter().any(|key| !visible.contains(&key.id))
        }
        TargetScope::Selection => selected_layers.iter().any(|id| !visible.contains(id)),
        TargetScope::Paste => rows_hidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{AudioMetadata, Command, Editor, LayerSwitch};

    fn add_layer(editor: &mut Editor, name: &str, content: Content) -> LayerId {
        editor.execute(Command::AddRectangle).unwrap();
        let id = editor.selected().unwrap();
        editor.execute(Command::SetContent { id, content }).unwrap();
        editor
            .execute(Command::RenameLayer {
                id,
                name: name.into(),
            })
            .unwrap();
        id
    }

    fn text() -> Content {
        Content::Text {
            text: "Source text is not the layer name".into(),
            font_size: 32.0,
        }
    }

    fn audio() -> AudioMetadata {
        AudioMetadata {
            stream_index: 0,
            sample_rate: 48_000,
            channels: 2,
            channel_layout: "stereo".into(),
            duration: 5.0,
            start_time: 0.0,
            file_offset: 0.0,
        }
    }

    #[test]
    fn every_content_variant_has_exactly_one_specific_type() {
        use LayerTypeFilter as Type;
        let examples = [
            (Content::Rectangle, Type::Shape),
            (Content::Shape(Default::default()), Type::Shape),
            (Content::ShapeContents(Default::default()), Type::Shape),
            (text(), Type::Text),
            (Content::Solid, Type::Solid),
            (Content::Image { png: "YWJj".into() }, Type::Image),
            (
                Content::ImageSequence {
                    frames: vec!["frame.png".into()].into(),
                    fps: 30.into(),
                    missing: Default::default(),
                    start_frame: 0,
                    playback: Default::default(),
                },
                Type::Sequence,
            ),
            (
                Content::Video {
                    path: "clip.mp4".into(),
                    audio: None,
                    duration: 5.0,
                    source_fps: 30.0,
                    start_frame: 0,
                    playback: Default::default(),
                },
                Type::Video,
            ),
            (
                Content::Video {
                    path: "clip-with-audio.mp4".into(),
                    audio: Some(audio()),
                    duration: 5.0,
                    source_fps: 30.0,
                    start_frame: 0,
                    playback: Default::default(),
                },
                Type::Video,
            ),
            (
                Content::Audio {
                    path: "sound.wav".into(),
                    audio: audio(),
                    start_frame: 0,
                    playback: Default::default(),
                },
                Type::Audio,
            ),
            (
                Content::Composition {
                    composition: 2,
                    start_frame: 0,
                },
                Type::Precomp,
            ),
            (Content::Adjustment, Type::Adjustment),
            (Content::Null, Type::Null),
        ];
        for (content, expected) in examples {
            for filter in Type::ALL {
                assert_eq!(
                    filter.matches(&content),
                    filter == Type::All || filter == expected,
                    "{} must classify {content:?} as {}",
                    filter.label(),
                    expected.label(),
                );
            }
        }
        assert_eq!(Type::default(), Type::All);
        assert_eq!(
            Type::ALL.map(Type::label),
            [
                "All types",
                "Shape",
                "Text",
                "Solid",
                "Image",
                "Sequence",
                "Video",
                "Audio",
                "Precomp",
                "Adjustment",
                "Null",
            ]
        );
    }

    #[test]
    fn name_search_is_trimmed_case_insensitive_unicode_substring() {
        let mut editor = Editor::default();
        let hero = add_layer(&mut editor, "Main HERO title", text());
        let unicode = add_layer(&mut editor, "CAFÉ 東京 ✨", Content::Rectangle);
        let ordinary = add_layer(&mut editor, "Background", Content::Solid);
        let comp = editor.project().composition();
        for (query, expected) in [
            ("  hErO  ", BTreeSet::from([hero])),
            ("FÉ 東", BTreeSet::from([unicode])),
            ("京 ✨", BTreeSet::from([unicode])),
            ("source text", BTreeSet::new()),
            ("missing", BTreeSet::new()),
            (" \t\n", BTreeSet::from([hero, unicode, ordinary])),
        ] {
            assert_eq!(
                matching_layer_ids(comp, query, LayerTypeFilter::All, false, &BTreeSet::new()),
                expected,
                "query {query:?}"
            );
        }
    }

    #[test]
    fn name_type_and_selected_filters_are_all_required() {
        let mut editor = Editor::default();
        let wanted = add_layer(&mut editor, "Hero title", text());
        let unselected = add_layer(&mut editor, "Hero subtitle", text());
        let wrong_name = add_layer(&mut editor, "Credits", text());
        let wrong_type = add_layer(&mut editor, "Hero shape", Content::Rectangle);
        let selected = BTreeSet::from([wanted, wrong_name, wrong_type, 999]);
        let comp = editor.project().composition();
        assert_eq!(
            matching_layer_ids(comp, " HERO ", LayerTypeFilter::Text, true, &selected),
            BTreeSet::from([wanted])
        );
        assert_eq!(
            matching_layer_ids(comp, "hero", LayerTypeFilter::Text, false, &selected),
            BTreeSet::from([wanted, unselected])
        );
        assert!(
            matching_layer_ids(comp, "", LayerTypeFilter::All, true, &BTreeSet::new()).is_empty()
        );
        assert_eq!(
            matching_layer_ids(comp, "", LayerTypeFilter::All, true, &selected),
            BTreeSet::from([wanted, wrong_name, wrong_type])
        );
    }

    #[test]
    fn hide_shy_is_always_respected_even_for_selected_matches() {
        let mut editor = Editor::default();
        let shy = add_layer(&mut editor, "Hero shy", text());
        let ordinary = add_layer(&mut editor, "Hero visible", text());
        editor
            .execute(Command::SetLayerSwitch {
                id: shy,
                switch: LayerSwitch::Shy,
                enabled: true,
            })
            .unwrap();
        let selected = BTreeSet::from([shy, ordinary]);
        for hide_shy in [true, false] {
            editor.execute(Command::SetHideShy(hide_shy)).unwrap();
            let expected = if hide_shy {
                BTreeSet::from([ordinary])
            } else {
                selected.clone()
            };
            assert_eq!(
                matching_layer_ids(
                    editor.project().composition(),
                    "hero",
                    LayerTypeFilter::Text,
                    true,
                    &selected,
                ),
                expected
            );
        }
    }

    #[test]
    fn all_layers_remain_searchable_when_disabled_locked_or_outside_the_playhead() {
        let mut editor = Editor::default();
        let id = add_layer(&mut editor, "Hero", Content::Rectangle);
        editor.execute(Command::ToggleVisible(id)).unwrap();
        editor
            .execute(Command::SetLayerRange {
                id,
                start: 30,
                end: 60,
            })
            .unwrap();
        editor.execute(Command::ToggleLocked(id)).unwrap();
        assert_eq!(
            matching_layer_ids(
                editor.project().composition(),
                "hero",
                LayerTypeFilter::All,
                false,
                &BTreeSet::new(),
            ),
            BTreeSet::from([id])
        );
    }

    #[test]
    fn current_composition_bounds_results_even_with_stale_selection() {
        let mut editor = Editor::default();
        let old = add_layer(&mut editor, "Hero old comp", text());
        editor.execute(Command::NewComposition).unwrap();
        assert!(
            matching_layer_ids(
                editor.project().composition(),
                "",
                LayerTypeFilter::All,
                false,
                &BTreeSet::from([old]),
            )
            .is_empty()
        );
        let current = add_layer(&mut editor, "Hero current comp", text());
        assert_eq!(
            matching_layer_ids(
                editor.project().composition(),
                "hero",
                LayerTypeFilter::Text,
                true,
                &BTreeSet::from([old, current]),
            ),
            BTreeSet::from([current])
        );
    }

    #[test]
    fn view_filters_preserve_stack_indices_source_selection_and_history() {
        let mut editor = Editor::default();
        let first = add_layer(&mut editor, "Hero bottom", Content::Rectangle);
        let middle = add_layer(&mut editor, "Other middle", Content::Rectangle);
        let last = add_layer(&mut editor, "Hero top", Content::Rectangle);
        editor
            .execute(Command::RenameLayer {
                id: last,
                name: "Temporary".into(),
            })
            .unwrap();
        editor.undo();
        let original = editor.project().clone();
        let json = original.to_json().unwrap();
        let selection = editor.selected();
        let generation = editor.context_generation();
        let history = (editor.can_undo(), editor.can_redo());
        let comp = editor.project().composition();
        let matches = matching_layer_ids(
            comp,
            "hero",
            LayerTypeFilter::Shape,
            false,
            &BTreeSet::new(),
        );
        let rows: Vec<_> = comp
            .layers()
            .iter()
            .enumerate()
            .filter(|(_, layer)| matches.contains(&layer.id()))
            .map(|(index, layer)| (index, layer.id()))
            .collect();
        assert_eq!(rows, [(0, last), (2, first)]);
        assert!(!matches.contains(&middle));
        let selected = BTreeSet::from([last, middle]);
        assert_eq!(
            selected_visible_click(comp, &matches, &selected, Some(last), first, false, true),
            BTreeSet::from([first, last])
        );
        assert_eq!(
            selected_visible_click(comp, &matches, &selected, Some(last), last, true, false),
            BTreeSet::new()
        );
        assert_eq!(selected, BTreeSet::from([last, middle]));
        assert_eq!(editor.project(), &original);
        assert_eq!(editor.project().to_json().unwrap(), json);
        assert_eq!(editor.selected(), selection);
        assert_eq!(editor.context_generation(), generation);
        assert_eq!((editor.can_undo(), editor.can_redo()), history);
        assert!(editor.can_redo());
    }

    #[test]
    fn matching_children_keep_parent_links_without_including_ancestors() {
        let mut editor = Editor::default();
        let parent = add_layer(&mut editor, "Rig", Content::Null);
        let child = add_layer(&mut editor, "Hero child", text());
        editor
            .execute(Command::SetParent {
                id: child,
                parent: Some(parent),
                frame: 0,
            })
            .unwrap();
        let comp = editor.project().composition();
        assert_eq!(
            matching_layer_ids(comp, "hero", LayerTypeFilter::Text, false, &BTreeSet::new(),),
            BTreeSet::from([child])
        );
        assert_eq!(comp.layer(child).unwrap().parent(), Some(parent));
        assert_eq!(comp.layer(parent).unwrap().name(), "Rig");
    }

    #[test]
    fn range_selection_uses_visible_stack_order_in_both_directions() {
        let mut editor = Editor::default();
        let bottom = add_layer(&mut editor, "Hero bottom", Content::Rectangle);
        let hidden = add_layer(&mut editor, "Other", Content::Rectangle);
        let middle = add_layer(&mut editor, "Hero middle", Content::Rectangle);
        let top = add_layer(&mut editor, "Hero top", Content::Rectangle);
        let comp = editor.project().composition();
        let before = comp.clone();
        let visible =
            matching_layer_ids(comp, "hero", LayerTypeFilter::All, false, &BTreeSet::new());
        for (anchor, target) in [(top, bottom), (bottom, top)] {
            assert_eq!(
                selected_visible_range(comp, &visible, Some(anchor), target),
                BTreeSet::from([top, middle, bottom])
            );
        }
        assert_eq!(
            selected_visible_range(comp, &visible, Some(top), middle),
            BTreeSet::from([top, middle])
        );
        assert_eq!(
            selected_visible_range(comp, &visible, Some(middle), middle),
            BTreeSet::from([middle])
        );
        assert!(!visible.contains(&hidden));
        assert_eq!(comp, &before);
    }

    #[test]
    fn range_selection_handles_hidden_or_stale_endpoints() {
        let mut editor = Editor::default();
        let hidden = add_layer(&mut editor, "Other", Content::Rectangle);
        let target = add_layer(&mut editor, "Hero", Content::Rectangle);
        let comp = editor.project().composition();
        let visible = BTreeSet::from([target, 999]);
        for anchor in [None, Some(hidden), Some(999)] {
            assert_eq!(
                selected_visible_range(comp, &visible, anchor, target),
                BTreeSet::from([target])
            );
        }
        for missing_target in [hidden, 999] {
            assert!(
                selected_visible_range(comp, &visible, Some(target), missing_target).is_empty()
            );
        }
        assert!(selected_visible_range(comp, &BTreeSet::new(), None, target).is_empty());
    }

    #[test]
    fn range_selection_rechecks_hide_shy_against_stale_visible_ids() {
        let mut editor = Editor::default();
        let shy = add_layer(&mut editor, "Shy", Content::Rectangle);
        let target = add_layer(&mut editor, "Target", Content::Rectangle);
        let visible = BTreeSet::from([shy, target]);
        editor
            .execute(Command::SetLayerSwitch {
                id: shy,
                switch: LayerSwitch::Shy,
                enabled: true,
            })
            .unwrap();
        editor.execute(Command::SetHideShy(true)).unwrap();
        let comp = editor.project().composition();
        assert_eq!(
            selected_visible_range(comp, &visible, Some(shy), target),
            BTreeSet::from([target])
        );
        assert!(selected_visible_range(comp, &visible, Some(target), shy).is_empty());
    }

    #[test]
    fn control_click_removes_last_visible_selection_without_retaining_hidden_owner() {
        let mut editor = Editor::default();
        let hidden = add_layer(&mut editor, "Other", Content::Rectangle);
        let visible = add_layer(&mut editor, "Hero", Content::Rectangle);
        let comp = editor.project().composition();
        assert!(
            selected_visible_click(
                comp,
                &BTreeSet::from([visible]),
                &BTreeSet::from([hidden, visible]),
                Some(hidden),
                visible,
                true,
                false,
            )
            .is_empty()
        );
    }

    #[test]
    fn control_click_adds_only_visible_rows_and_plain_click_replaces_selection() {
        let mut editor = Editor::default();
        let hidden = add_layer(&mut editor, "Other", Content::Rectangle);
        let first = add_layer(&mut editor, "First", Content::Rectangle);
        let target = add_layer(&mut editor, "Target", Content::Rectangle);
        let comp = editor.project().composition();
        let visible = BTreeSet::from([first, target]);
        let previous = BTreeSet::from([hidden, first, 999]);
        assert_eq!(
            selected_visible_click(comp, &visible, &previous, Some(hidden), target, true, false),
            BTreeSet::from([first, target])
        );
        assert_eq!(
            selected_visible_click(
                comp,
                &visible,
                &previous,
                Some(hidden),
                target,
                false,
                false
            ),
            BTreeSet::from([target])
        );
    }

    #[test]
    fn shift_click_extends_only_visible_previous_selection() {
        let mut editor = Editor::default();
        let bottom = add_layer(&mut editor, "Bottom", Content::Rectangle);
        let middle = add_layer(&mut editor, "Middle", Content::Rectangle);
        let hidden = add_layer(&mut editor, "Other", Content::Rectangle);
        let top = add_layer(&mut editor, "Top", Content::Rectangle);
        let comp = editor.project().composition();
        let visible = BTreeSet::from([bottom, middle, top]);
        let previous = BTreeSet::from([bottom, hidden, 999]);
        for toggle in [false, true] {
            assert_eq!(
                selected_visible_click(comp, &visible, &previous, Some(top), middle, toggle, true),
                BTreeSet::from([bottom, middle, top])
            );
        }
        assert_eq!(
            selected_visible_click(comp, &visible, &previous, Some(hidden), top, false, true),
            BTreeSet::from([bottom, top])
        );
    }

    #[test]
    fn hidden_click_target_reconciles_selection_without_adding_hidden_or_stale_ids() {
        let mut editor = Editor::default();
        let hidden = add_layer(&mut editor, "Other", Content::Rectangle);
        let shy = add_layer(&mut editor, "Shy", Content::Rectangle);
        let shown = add_layer(&mut editor, "Shown", Content::Rectangle);
        editor
            .execute(Command::SetLayerSwitch {
                id: shy,
                switch: LayerSwitch::Shy,
                enabled: true,
            })
            .unwrap();
        editor.execute(Command::SetHideShy(true)).unwrap();
        let comp = editor.project().composition();
        let stale_visible = BTreeSet::from([shy, shown, 999]);
        let previous = BTreeSet::from([hidden, shy, shown, 999]);
        for target in [hidden, shy, 999] {
            for (toggle, range) in [(false, false), (true, false), (false, true)] {
                assert_eq!(
                    selected_visible_click(
                        comp,
                        &stale_visible,
                        &previous,
                        Some(shown),
                        target,
                        toggle,
                        range,
                    ),
                    BTreeSet::from([shown])
                );
            }
        }
    }
    fn key(id: LayerId) -> KeyRef {
        KeyRef {
            id,
            property: libre_effects_core::Property::PositionX.into(),
            frame: 10,
        }
    }
    #[test]
    fn delete_targets_keys_without_falling_back_to_visible_layers() {
        let visible = [1].into();
        let layers = [1, 2].into();
        assert!(!blocks_hidden_targets(
            TargetScope::Selection,
            &visible,
            &layers,
            &[key(1)].into(),
            true,
        ));
        assert!(blocks_hidden_targets(
            TargetScope::Selection,
            &visible,
            &[1].into(),
            &[key(2)].into(),
            true,
        ));
        assert!(blocks_hidden_targets(
            TargetScope::Selection,
            &visible,
            &layers,
            &BTreeSet::new(),
            true,
        ));
    }

    #[test]
    fn layer_and_key_scopes_validate_their_actual_target_sets() {
        let visible = [1].into();
        let layers = [1, 2].into();
        let keys = [key(1)].into();
        assert!(blocks_hidden_targets(
            TargetScope::Layers,
            &visible,
            &layers,
            &keys,
            true,
        ));
        assert!(!blocks_hidden_targets(
            TargetScope::Keys,
            &visible,
            &layers,
            &keys,
            true,
        ));
        assert!(blocks_hidden_targets(
            TargetScope::Keys,
            &visible,
            &[1].into(),
            &[key(1), key(2)].into(),
            true,
        ));
    }

    #[test]
    fn paste_checks_hidden_rows_even_without_selected_hidden_targets() {
        for layers in [BTreeSet::new(), [1].into()] {
            for keys in [BTreeSet::new(), [key(1)].into()] {
                assert!(blocks_hidden_targets(
                    TargetScope::Paste,
                    &[1].into(),
                    &layers,
                    &keys,
                    true,
                ));
                assert!(!blocks_hidden_targets(
                    TargetScope::Paste,
                    &[1, 2].into(),
                    &layers,
                    &keys,
                    false,
                ));
            }
        }
    }
}
