//! Transient keyboard selection in displayed Timeline stack order.

use libre_effects_core::{Composition, LayerId};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    Previous,
    Next,
    First,
    Last,
    All,
}

/// Only plain/Shift row navigation and plain Control+A belong to this domain.
/// Focus, modal, gesture and key-selection ownership are checked by the UI.
pub fn shortcut(
    key: &str,
    control: bool,
    shift: bool,
    alt: bool,
    platform: bool,
    function: bool,
) -> Option<Navigation> {
    if alt || platform || function {
        return None;
    }
    if control {
        return (key == "a" && !shift).then_some(Navigation::All);
    }
    match key {
        "up" => Some(Navigation::Previous),
        "down" => Some(Navigation::Next),
        "home" => Some(Navigation::First),
        "end" => Some(Navigation::Last),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub active: Option<LayerId>,
    pub layers: BTreeSet<LayerId>,
}

/// Remembers an anchor only while selection and displayed order stay current.
/// A mouse/other-panel selection, filter or stack change starts a fresh range.
#[derive(Default)]
pub struct LayerNavigation {
    anchor: Option<LayerId>,
    previous: Option<Selection>,
    rows: Vec<LayerId>,
}

impl LayerNavigation {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn select(
        &mut self,
        comp: &Composition,
        visible: &BTreeSet<LayerId>,
        current: Selection,
        navigation: Navigation,
        extend: bool,
        selected_only: bool,
    ) -> Selection {
        let rows: Vec<_> = comp
            .layers()
            .iter()
            .filter(|layer| visible.contains(&layer.id()) && !(comp.hide_shy() && layer.shy()))
            .map(|layer| layer.id())
            .collect();
        let same_selection = self.previous.as_ref() == Some(&current);
        // Selected-only visibility can shrink as a direct consequence of our
        // own range selection. Preserve its anchor while it remains visible,
        // but never restore a row that the live selection filter now hides.
        let selection_shrank_rows = selected_only
            && same_selection
            && self
                .rows
                .iter()
                .filter(|id| current.layers.contains(id))
                .copied()
                .eq(rows.iter().copied());
        if !same_selection || (self.rows != rows && !selection_shrank_rows) {
            self.anchor = None;
        }
        let active_index = current
            .active
            .filter(|id| current.layers.contains(id))
            .and_then(|id| rows.iter().position(|row| *row == id));
        let target_index = match navigation {
            Navigation::Previous => {
                active_index.map_or(rows.len().saturating_sub(1), |i| i.saturating_sub(1))
            }
            Navigation::Next => {
                active_index.map_or(0, |i| (i + 1).min(rows.len().saturating_sub(1)))
            }
            Navigation::First => 0,
            Navigation::Last => rows.len().saturating_sub(1),
            Navigation::All => active_index.unwrap_or(0),
        };
        let active = rows.get(target_index).copied();
        let layers = if navigation == Navigation::All {
            self.anchor = active;
            rows.iter().copied().collect()
        } else if let Some(target) = active {
            let anchor = if extend {
                self.anchor
                    .filter(|id| rows.contains(id))
                    .or_else(|| active_index.map(|i| rows[i]))
                    .unwrap_or(target)
            } else {
                target
            };
            self.anchor = Some(anchor);
            let anchor_index = rows.iter().position(|id| *id == anchor).unwrap();
            rows[anchor_index.min(target_index)..=anchor_index.max(target_index)]
                .iter()
                .copied()
                .collect()
        } else {
            self.anchor = None;
            BTreeSet::new()
        };
        let selection = Selection { active, layers };
        self.previous = Some(selection.clone());
        self.rows = rows;
        selection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor, LayerSwitch};

    fn fixture() -> (Editor, Vec<LayerId>, BTreeSet<LayerId>) {
        let mut editor = Editor::default();
        for _ in 0..5 {
            editor.execute(Command::AddRectangle).unwrap();
        }
        let last = editor.selected().unwrap();
        editor
            .execute(Command::MoveLayer { id: last, index: 0 })
            .unwrap();
        let rows: Vec<_> = editor
            .project()
            .composition()
            .layers()
            .iter()
            .map(|l| l.id())
            .collect();
        let visible = [rows[0], rows[2], rows[4]].into();
        (editor, rows, visible)
    }

    fn selected(ids: &[LayerId], active: LayerId) -> Selection {
        Selection {
            active: Some(active),
            layers: ids.iter().copied().collect(),
        }
    }

    #[test]
    fn shortcuts_respect_modifiers_and_leave_other_domains_alone() {
        for shift in [false, true] {
            for (key, command) in [
                ("up", Navigation::Previous),
                ("down", Navigation::Next),
                ("home", Navigation::First),
                ("end", Navigation::Last),
            ] {
                assert_eq!(
                    shortcut(key, false, shift, false, false, false),
                    Some(command)
                );
                for modifiers in [
                    (true, false, false, false),
                    (false, true, false, false),
                    (false, false, true, false),
                    (false, false, false, true),
                ] {
                    assert_eq!(
                        shortcut(
                            key,
                            modifiers.0,
                            shift,
                            modifiers.1,
                            modifiers.2,
                            modifiers.3
                        ),
                        None
                    );
                }
            }
        }
        assert_eq!(
            shortcut("a", true, false, false, false, false),
            Some(Navigation::All)
        );
        for (control, shift, alt, platform, function) in [
            (false, false, false, false, false),
            (true, true, false, false, false),
            (true, false, true, false, false),
            (true, false, false, true, false),
            (true, false, false, false, true),
        ] {
            assert_eq!(shortcut("a", control, shift, alt, platform, function), None);
        }
        for key in [
            "left", "right", "pageup", "pagedown", "delete", "space", "tab",
        ] {
            assert_eq!(shortcut(key, false, false, false, false, false), None);
        }
    }

    #[test]
    fn arrows_follow_visible_stack_order_and_clamp_at_edges() {
        let (editor, rows, visible) = fixture();
        let mut nav = LayerNavigation::default();
        let comp = editor.project().composition();
        let mut current = selected(&[rows[0]], rows[0]);
        for expected in [rows[2], rows[4], rows[4]] {
            current = nav.select(comp, &visible, current, Navigation::Next, false, false);
            assert_eq!(current, selected(&[expected], expected));
        }
        for expected in [rows[2], rows[0], rows[0]] {
            current = nav.select(comp, &visible, current, Navigation::Previous, false, false);
            assert_eq!(current, selected(&[expected], expected));
        }
    }

    #[test]
    fn shift_range_grows_shrinks_and_reverses_around_one_anchor() {
        let (editor, rows, visible) = fixture();
        let mut nav = LayerNavigation::default();
        let comp = editor.project().composition();
        let mut current = selected(&[rows[2]], rows[2]);
        current = nav.select(comp, &visible, current, Navigation::Next, true, false);
        assert_eq!(current, selected(&[rows[2], rows[4]], rows[4]));
        current = nav.select(comp, &visible, current, Navigation::Previous, true, false);
        assert_eq!(current, selected(&[rows[2]], rows[2]));
        current = nav.select(comp, &visible, current, Navigation::Previous, true, false);
        assert_eq!(current, selected(&[rows[0], rows[2]], rows[0]));
        current = nav.select(comp, &visible, current, Navigation::Last, true, false);
        assert_eq!(current, selected(&[rows[2], rows[4]], rows[4]));
        current = nav.select(comp, &visible, current, Navigation::First, false, false);
        assert_eq!(current, selected(&[rows[0]], rows[0]));
    }

    #[test]
    fn absent_hidden_or_unselected_active_starts_at_directional_edge() {
        let (editor, rows, visible) = fixture();
        let comp = editor.project().composition();
        for current in [
            Selection::default(),
            selected(&[rows[1], 999], rows[1]),
            selected(&[rows[2]], rows[0]),
        ] {
            for (command, expected) in [
                (Navigation::Next, rows[0]),
                (Navigation::First, rows[0]),
                (Navigation::Previous, rows[4]),
                (Navigation::Last, rows[4]),
            ] {
                let result = LayerNavigation::default().select(
                    comp,
                    &visible,
                    current.clone(),
                    command,
                    true,
                    false,
                );
                assert_eq!(result, selected(&[expected], expected));
            }
        }
    }

    #[test]
    fn select_all_preserves_visible_active_and_uses_stack_first_as_fallback() {
        let (editor, rows, visible) = fixture();
        let comp = editor.project().composition();
        let mut nav = LayerNavigation::default();
        let result = nav.select(
            comp,
            &visible,
            selected(&[rows[2]], rows[2]),
            Navigation::All,
            false,
            false,
        );
        assert_eq!(
            result,
            Selection {
                active: Some(rows[2]),
                layers: visible.clone()
            }
        );
        let result = nav.select(
            comp,
            &visible,
            selected(&[rows[1], 999], rows[1]),
            Navigation::All,
            false,
            false,
        );
        assert_eq!(
            result,
            Selection {
                active: Some(rows[0]),
                layers: visible
            }
        );
    }

    #[test]
    fn external_selection_or_filter_change_reanchors_the_next_range() {
        let (editor, rows, visible) = fixture();
        let comp = editor.project().composition();
        let mut nav = LayerNavigation::default();
        let original = selected(&[rows[0]], rows[0]);
        nav.select(comp, &visible, original, Navigation::Last, true, false);
        let result = nav.select(
            comp,
            &visible,
            selected(&[rows[2]], rows[2]),
            Navigation::Last,
            true,
            false,
        );
        assert_eq!(result, selected(&[rows[2], rows[4]], rows[4]));
        let filtered = [rows[0], rows[4]].into();
        let result = nav.select(comp, &filtered, result, Navigation::First, true, false);
        assert_eq!(result, selected(&[rows[0], rows[4]], rows[0]));
        nav.reset();
        let result = nav.select(comp, &visible, result, Navigation::Next, true, false);
        assert_eq!(result, selected(&[rows[0], rows[2]], rows[2]));
    }

    #[test]
    fn empty_stale_and_shy_membership_never_selects_hidden_rows() {
        let (mut editor, rows, _) = fixture();
        editor
            .execute(Command::SetLayerSwitch {
                id: rows[0],
                switch: LayerSwitch::Shy,
                enabled: true,
            })
            .unwrap();
        editor.execute(Command::SetHideShy(true)).unwrap();
        let comp = editor.project().composition();
        let mut nav = LayerNavigation::default();
        for command in [
            Navigation::Previous,
            Navigation::Next,
            Navigation::First,
            Navigation::Last,
            Navigation::All,
        ] {
            assert_eq!(
                nav.select(
                    comp,
                    &BTreeSet::from([rows[0], 999]),
                    selected(&[rows[0]], rows[0]),
                    command,
                    true,
                    false
                ),
                Selection::default()
            );
        }
        let visible = rows.iter().copied().chain([999]).collect();
        let result = nav.select(
            comp,
            &visible,
            Selection::default(),
            Navigation::All,
            false,
            false,
        );
        assert!(!result.layers.contains(&rows[0]));
        assert!(!result.layers.contains(&999));
        assert_eq!(result.active, Some(rows[1]));
    }

    #[test]
    fn selected_only_self_contraction_keeps_anchor_without_restoring_hidden_rows() {
        let (editor, rows, visible) = fixture();
        let comp = editor.project().composition();
        let mut nav = LayerNavigation::default();
        let current = Selection {
            active: Some(rows[0]),
            layers: visible.clone(),
        };
        let partial = nav.select(comp, &visible, current, Navigation::Next, true, true);
        assert_eq!(partial, selected(&[rows[0], rows[2]], rows[2]));
        let shrunk = partial.layers.clone();
        let result = nav.select(
            comp,
            &shrunk,
            partial.clone(),
            Navigation::Previous,
            true,
            true,
        );
        assert_eq!(result, selected(&[rows[0]], rows[0]));
        let only_anchor = result.layers.clone();
        assert_eq!(
            nav.select(comp, &only_anchor, result, Navigation::Next, true, true),
            selected(&[rows[0]], rows[0])
        );
        // An explicit filter change resets even if its membership happens to
        // equal the selection-filter result.
        nav.reset();
        let result = nav.select(comp, &shrunk, partial, Navigation::Previous, true, true);
        assert_eq!(result, selected(&[rows[0], rows[2]], rows[0]));
    }

    #[test]
    fn stack_reorder_reanchors_even_with_identical_membership() {
        let (mut editor, rows, visible) = fixture();
        let mut nav = LayerNavigation::default();
        let current = nav.select(
            editor.project().composition(),
            &visible,
            selected(&[rows[0]], rows[0]),
            Navigation::Next,
            true,
            false,
        );
        editor
            .execute(Command::MoveLayer {
                id: rows[0],
                index: 4,
            })
            .unwrap();
        let result = nav.select(
            editor.project().composition(),
            &visible,
            current,
            Navigation::Next,
            true,
            false,
        );
        assert_eq!(result, selected(&[rows[2], rows[4]], rows[4]));
    }

    #[test]
    fn navigation_and_selection_leave_source_and_history_unchanged() {
        let (mut editor, rows, visible) = fixture();
        editor.clear_history();
        let source = editor.project().clone();
        let mut nav = LayerNavigation::default();
        let mut current = selected(&[rows[0]], rows[0]);
        for command in [
            Navigation::Last,
            Navigation::First,
            Navigation::Next,
            Navigation::Previous,
            Navigation::All,
        ] {
            current = nav.select(
                editor.project().composition(),
                &visible,
                current,
                command,
                true,
                false,
            );
            if let Some(id) = current.active {
                editor.select(id);
            }
            assert_eq!(editor.project(), &source);
            assert!(!editor.can_undo());
            assert!(!editor.can_redo());
        }
    }
}
