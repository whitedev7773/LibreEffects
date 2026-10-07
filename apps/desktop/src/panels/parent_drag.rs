use crate::{editor::EditorState, ui};
use gpui::{Context, Window, div, prelude::*, px, rgb};
use libre_effects_core::{Command, CompositionId, LayerId, Project};

#[derive(Clone)]
pub(crate) struct ParentDrag {
    child: LayerId,
    composition: CompositionId,
    revision: u64,
    name: String,
}
impl ParentDrag {
    pub fn new(state: &EditorState, child: LayerId, name: String) -> Self {
        Self {
            child,
            name,
            composition: state.editor.project().active_composition_id(),
            revision: state.document_revision,
        }
    }
    pub fn command(&self, state: &EditorState, parent: LayerId) -> Option<Command> {
        self.resolve(
            state.editor.project(),
            state.frame,
            state.document_revision,
            parent,
        )
    }
    fn resolve(
        &self,
        project: &Project,
        frame: u32,
        revision: u64,
        parent: LayerId,
    ) -> Option<Command> {
        let comp = project.composition();
        (self.revision == revision
            && self.composition == project.active_composition_id()
            && comp.layer(self.child).is_some_and(|l| !l.locked())
            && comp.can_parent(self.child, Some(parent))
            && comp
                .world_transform(parent, frame)
                .and_then(|t| t.inverse())
                .is_some())
        .then_some(Command::SetParent {
            id: self.child,
            parent: Some(parent),
            frame,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, Property};
    #[test]
    fn pick_whip_preserves_pose_and_rejects_stale_documents_cycles_and_locked_sources() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::Rotation,
            frame: 0,
            value: 47.0,
        })
        .unwrap();
        let drag = ParentDrag {
            child: 2,
            composition: e.project().active_composition_id(),
            revision: 5,
            name: "Child".into(),
        };
        let before = e.project().clone();
        assert!(drag.resolve(e.project(), 0, 6, 1).is_none());
        assert!(drag.resolve(e.project(), 0, 5, 2).is_none());
        assert!(drag.resolve(e.project(), 0, 5, 99).is_none());
        e.execute(drag.resolve(e.project(), 0, 5, 1).unwrap())
            .unwrap();
        let linked = e.project().clone();
        for (a, b) in before
            .composition()
            .corners_at(2, 0)
            .unwrap()
            .into_iter()
            .flatten()
            .zip(
                linked
                    .composition()
                    .corners_at(2, 0)
                    .unwrap()
                    .into_iter()
                    .flatten(),
            )
        {
            assert!((a - b).abs() < 1e-8);
        }
        let reverse = ParentDrag {
            child: 1,
            ..drag.clone()
        };
        assert!(reverse.resolve(e.project(), 0, 5, 2).is_none());
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &linked);
        let loaded = Project::from_json(&linked.to_json().unwrap()).unwrap();
        assert_eq!(loaded.composition().layer(2).unwrap().parent(), Some(1));
        e.execute(Command::ToggleLocked(2)).unwrap();
        assert!(drag.resolve(e.project(), 0, 5, 1).is_none());
        e.undo();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::ScaleY,
            frame: 0,
            value: 0.0,
        })
        .unwrap();
        assert!(drag.resolve(e.project(), 0, 5, 1).is_none());
        e.undo();
        e.execute(Command::NewComposition).unwrap();
        assert!(drag.resolve(e.project(), 0, 5, 1).is_none());
    }
}
impl Render for ParentDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap_2()
            .items_center()
            .px_2()
            .py_1()
            .bg(rgb(ui::PANEL))
            .border_1()
            .border_color(rgb(ui::BLUE))
            .text_color(rgb(ui::TEXT))
            .text_size(px(11.0))
            .child(ui::icon("circle-link"))
            .child(format!(
                "Parent {} → drop on a layer · Esc cancels",
                self.name
            ))
    }
}
