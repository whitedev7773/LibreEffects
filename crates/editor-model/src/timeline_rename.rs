//! One visible layer's source-bound, session-only Timeline name edit.

use libre_effects_core::{Command, CompositionId, Editor, Frame, LayerId};
use std::collections::BTreeSet;

/// The UI supplies live membership and its document/action generations. Core's
/// own generation also catches direct source, selection and history changes.
pub struct RenameContext<'a> {
    pub editor: &'a Editor,
    pub selected: &'a BTreeSet<LayerId>,
    pub visible: &'a BTreeSet<LayerId>,
    pub document_revision: u64,
    pub input_generation: u64,
    pub frame: Frame,
    pub playing: bool,
}

/// Retain a current field for its first unfocused draw: GPUI delivers its blur
/// commit after drawing roots. A later draw closes unchanged/canceled fields.
/// Source/ownership invalidation bypasses this grace and cancels immediately.
#[derive(Default)]
pub struct RenameFocus {
    blur_pending: bool,
}

impl RenameFocus {
    pub fn keep_for_blur(&mut self, focused: bool) -> bool {
        if focused {
            self.blur_pending = false;
            true
        } else {
            !std::mem::replace(&mut self.blur_pending, true)
        }
    }
}

#[derive(Clone, Debug)]
pub struct LayerRename {
    layer: LayerId,
    name: String,
    composition: CompositionId,
    editor_generation: u64,
    document_revision: u64,
    input_generation: u64,
    frame: Frame,
}

impl LayerRename {
    pub fn begin(context: &RenameContext<'_>) -> Option<Self> {
        let layer = context.editor.selected_layer()?;
        let comp = context.editor.project().composition();
        if context.playing
            || layer.locked()
            || context.selected.len() != 1
            || !context.selected.contains(&layer.id())
            || !context.visible.contains(&layer.id())
            || (comp.hide_shy() && layer.shy())
        {
            return None;
        }
        Some(Self {
            layer: layer.id(),
            name: layer.name().into(),
            composition: context.editor.project().active_composition_id(),
            editor_generation: context.editor.context_generation(),
            document_revision: context.document_revision,
            input_generation: context.input_generation,
            frame: context.frame,
        })
    }

    pub fn layer(&self) -> LayerId {
        self.layer
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn current(&self, context: &RenameContext<'_>) -> bool {
        self.document_revision == context.document_revision
            && self.input_generation == context.input_generation
            && self.editor_generation == context.editor.context_generation()
            && self.composition == context.editor.project().active_composition_id()
            && self.frame == context.frame
            && !context.playing
            && context.editor.selected() == Some(self.layer)
            && context.selected.len() == 1
            && context.selected.contains(&self.layer)
            && context.visible.contains(&self.layer)
            && context
                .editor
                .project()
                .composition()
                .layer(self.layer)
                .is_some_and(|layer| {
                    !layer.locked()
                        && layer.name() == self.name
                        && !(context.editor.project().composition().hide_shy() && layer.shy())
                })
    }

    pub fn command(
        &self,
        context: &RenameContext<'_>,
        name: &str,
    ) -> Result<Option<Command>, &'static str> {
        if !self.current(context) {
            return Err("Layer rename expired. Select one visible, unlocked layer and try again.");
        }
        // Match RenameLayer's byte budget before trimming. Keep Unicode and
        // internal whitespace; an unchanged normalized name creates no history.
        if name.trim().is_empty() || name.len() > 1024 {
            return Err("Enter a layer name (1–1024 bytes)");
        }
        Ok((name.trim() != self.name).then(|| Command::RenameLayer {
            id: self.layer,
            name: name.trim().into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{LayerSwitch, Property};

    fn fixture() -> (Editor, BTreeSet<LayerId>, BTreeSet<LayerId>) {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor.execute(Command::AddRectangle).unwrap();
        let selected = BTreeSet::from([editor.selected().unwrap()]);
        let visible = editor
            .project()
            .composition()
            .layers()
            .iter()
            .map(|l| l.id())
            .collect();
        editor.clear_history();
        (editor, selected, visible)
    }

    fn context<'a>(
        editor: &'a Editor,
        selected: &'a BTreeSet<LayerId>,
        visible: &'a BTreeSet<LayerId>,
    ) -> RenameContext<'a> {
        RenameContext {
            editor,
            selected,
            visible,
            document_revision: 3,
            input_generation: 7,
            frame: 12,
            playing: false,
        }
    }

    #[test]
    fn one_visible_unlocked_selected_layer_is_required() {
        let (mut editor, selected, visible) = fixture();
        assert!(LayerRename::begin(&context(&editor, &selected, &visible)).is_some());
        for selection in [BTreeSet::new(), visible.clone(), BTreeSet::from([999])] {
            assert!(LayerRename::begin(&context(&editor, &selection, &visible)).is_none());
        }
        assert!(LayerRename::begin(&context(&editor, &selected, &BTreeSet::new())).is_none());
        editor
            .execute(Command::ToggleLocked(editor.selected().unwrap()))
            .unwrap();
        assert!(LayerRename::begin(&context(&editor, &selected, &visible)).is_none());
    }

    #[test]
    fn hide_shy_is_checked_even_when_visible_membership_is_stale() {
        let (mut editor, selected, visible) = fixture();
        editor
            .execute(Command::SetLayerSwitch {
                id: editor.selected().unwrap(),
                switch: LayerSwitch::Shy,
                enabled: true,
            })
            .unwrap();
        assert!(LayerRename::begin(&context(&editor, &selected, &visible)).is_some());
        editor.execute(Command::SetHideShy(true)).unwrap();
        assert!(LayerRename::begin(&context(&editor, &selected, &visible)).is_none());
    }

    #[test]
    fn rename_is_exactly_one_undoable_name_change() {
        let (mut editor, selected, visible) = fixture();
        let id = editor.selected().unwrap();
        editor
            .execute(Command::SetValue {
                id,
                property: Property::PositionX,
                frame: 0,
                value: 48.0,
            })
            .unwrap();
        editor.clear_history();
        let before = editor.project().clone();
        let session = LayerRename::begin(&context(&editor, &selected, &visible)).unwrap();
        let command = session
            .command(&context(&editor, &selected, &visible), "  Title 제목 🎬  ")
            .unwrap()
            .unwrap();
        let mut expected = Editor::default();
        expected.replace_project(editor.project().clone()).unwrap();
        expected
            .execute(Command::RenameLayer {
                id,
                name: "Title 제목 🎬".into(),
            })
            .unwrap();
        editor.execute(command).unwrap();
        assert_eq!(editor.project(), expected.project());
        assert_eq!(editor.selected(), Some(id));
        assert!(editor.can_undo());
        editor.undo();
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
        editor.redo();
        assert_eq!(editor.project(), expected.project());
    }

    #[test]
    fn unchanged_normalized_name_does_not_create_history() {
        let (editor, selected, visible) = fixture();
        let context = context(&editor, &selected, &visible);
        let session = LayerRename::begin(&context).unwrap();
        assert!(session.command(&context, session.name()).unwrap().is_none());
        assert!(
            session
                .command(&context, &format!("  {}  ", session.name()))
                .unwrap()
                .is_none()
        );
        assert!(!editor.can_undo());
    }

    #[test]
    fn names_follow_core_byte_budget_and_preserve_internal_whitespace() {
        let (mut editor, selected, visible) = fixture();
        let context = context(&editor, &selected, &visible);
        let session = LayerRename::begin(&context).unwrap();
        for value in [
            "".into(),
            " \t\n ".into(),
            "é".repeat(513),
            format!(" {}", "x".repeat(1024)),
        ] {
            assert!(session.command(&context, &value).is_err());
        }
        for value in ["é".repeat(512), "  First  second  ".into()] {
            let command = session.command(&context, &value).unwrap().unwrap();
            let mut copy = Editor::default();
            copy.replace_project(editor.project().clone()).unwrap();
            copy.select(session.layer());
            copy.execute(command).unwrap();
            assert_eq!(copy.selected_layer().unwrap().name(), value.trim());
        }
        editor.clear_history();
        assert!(!editor.can_undo());
    }

    #[test]
    fn document_action_transport_and_hidden_selection_changes_expire_drafts() {
        let (editor, selected, visible) = fixture();
        let session = LayerRename::begin(&context(&editor, &selected, &visible)).unwrap();
        for change in 0..5 {
            let mut ctx = context(&editor, &selected, &visible);
            match change {
                0 => ctx.document_revision += 1,
                1 => ctx.input_generation += 1,
                2 => ctx.frame += 1,
                3 => ctx.playing = true,
                _ => ctx.selected = &visible,
            }
            assert!(!session.current(&ctx));
            assert!(session.command(&ctx, "Do not rename").is_err());
        }
        assert!(
            session
                .command(&context(&editor, &selected, &BTreeSet::new()), "Hidden")
                .is_err()
        );
    }

    #[test]
    fn source_edit_and_undo_cannot_revive_equal_source_drafts() {
        let (mut editor, selected, visible) = fixture();
        let before = editor.project().clone();
        let session = LayerRename::begin(&context(&editor, &selected, &visible)).unwrap();
        let other = *visible.difference(&selected).next().unwrap();
        editor
            .execute(Command::RenameLayer {
                id: other,
                name: "Elsewhere".into(),
            })
            .unwrap();
        assert!(!session.current(&context(&editor, &selected, &visible)));
        editor.undo();
        assert_eq!(editor.project(), &before);
        assert!(
            session
                .command(&context(&editor, &selected, &visible), "Stale")
                .is_err()
        );
    }

    #[test]
    fn selection_roundtrip_and_removed_targets_expire_drafts() {
        let (mut editor, selected, visible) = fixture();
        let session = LayerRename::begin(&context(&editor, &selected, &visible)).unwrap();
        let other = *visible.difference(&selected).next().unwrap();
        editor.select(other);
        editor.select(session.layer());
        assert!(!session.current(&context(&editor, &selected, &visible)));
        let session = LayerRename::begin(&context(&editor, &selected, &visible)).unwrap();
        editor
            .execute(Command::RemoveLayer(session.layer()))
            .unwrap();
        assert!(
            session
                .command(&context(&editor, &selected, &visible), "Deleted")
                .is_err()
        );
    }

    #[test]
    fn current_field_survives_one_blur_draw_and_refocus_rearms_it() {
        let mut focus = RenameFocus::default();
        assert!(focus.keep_for_blur(true));
        assert!(focus.keep_for_blur(true));
        assert!(focus.keep_for_blur(false));
        assert!(!focus.keep_for_blur(false));
        assert!(focus.keep_for_blur(true));
        assert!(focus.keep_for_blur(false));
        assert!(focus.keep_for_blur(true));
        assert!(focus.keep_for_blur(false));
        assert!(!focus.keep_for_blur(false));
    }
}
