use crate::editor::EditorState;
use gpui::{Pixels, Point};
use libre_effects_core::{Command, Content, Editor, Project, Shape, ShapeKind};

pub(super) struct ShapeGesture {
    kind: ShapeKind,
    start: [f64; 2],
    end: [f64; 2],
    origin: Point<Pixels>,
    zoom: f32,
    shift: bool,
    alt: bool,
    project: Project,
    revision: u64,
    frame: u32,
}
impl ShapeGesture {
    pub fn new(
        kind: ShapeKind,
        start: [f64; 2],
        origin: Point<Pixels>,
        zoom: f32,
        state: &EditorState,
    ) -> Self {
        Self {
            kind,
            start,
            end: start,
            origin,
            zoom,
            shift: false,
            alt: false,
            project: state.editor.project().clone(),
            revision: state.document_revision,
            frame: state.frame,
        }
    }
    pub fn update(&mut self, point: Point<Pixels>, shift: bool, alt: bool) {
        self.end = [
            f32::from(point.x - self.origin.x) as f64 / self.zoom as f64,
            f32::from(point.y - self.origin.y) as f64 / self.zoom as f64,
        ];
        self.shift = shift;
        self.alt = alt;
    }
    pub fn command(&self, state: &EditorState) -> Option<Command> {
        if state.document_revision != self.revision
            || state.frame != self.frame
            || state.editor.project() != &self.project
        {
            return None;
        }
        draw_command(
            &self.project,
            self.kind,
            self.start,
            self.end,
            self.shift,
            self.alt,
            self.frame,
        )
    }
}
fn draw_command(
    project: &Project,
    kind: ShapeKind,
    start: [f64; 2],
    end: [f64; 2],
    shift: bool,
    alt: bool,
    frame: u32,
) -> Option<Command> {
    let mut delta = [end[0] - start[0], end[1] - start[1]];
    if shift {
        let side = delta[0].abs().max(delta[1].abs());
        delta = [
            side * if delta[0] < 0.0 { -1.0 } else { 1.0 },
            side * if delta[1] < 0.0 { -1.0 } else { 1.0 },
        ];
    }
    let multiplier = if alt { 2.0 } else { 1.0 };
    let width = delta[0].abs() * multiplier;
    let height = delta[1].abs() * multiplier;
    if width < 1.0 || height < 1.0 || !width.is_finite() || !height.is_finite() {
        return None;
    }
    let add = Command::AddContent {
        content: Content::Shape(Shape {
            kind,
            ..Default::default()
        }),
        width,
        height,
        name: kind.label().into(),
    };
    let mut temporary = Editor::default();
    temporary.replace_project(project.clone()).ok()?;
    temporary.execute(add.clone()).ok()?;
    let id = temporary.selected()?;
    Some(Command::Batch(vec![
        add,
        Command::SetPosition {
            id,
            frame,
            x: start[0] + if alt { 0.0 } else { delta[0] / 2.0 },
            y: start[1] + if alt { 0.0 } else { delta[1] / 2.0 },
        },
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::Property;
    #[test]
    fn reverse_and_centered_constrained_drags_are_one_history_entry() {
        for (alt, shift, expected) in [
            (false, false, [40.0, 20.0, 80.0, 90.0]),
            (true, true, [80.0, 80.0, 100.0, 100.0]),
        ] {
            let mut e = Editor::default();
            let before = e.project().clone();
            let command = draw_command(
                &before,
                ShapeKind::Ellipse,
                [100.0, 100.0],
                [60.0, 80.0],
                shift,
                alt,
                0,
            )
            .unwrap();
            e.execute(command).unwrap();
            let l = e.selected_layer().unwrap();
            assert_eq!(
                [
                    l.width(),
                    l.height(),
                    l.property(Property::PositionX).value_at(0),
                    l.property(Property::PositionY).value_at(0)
                ],
                expected
            );
            e.undo();
            assert_eq!(*e.project(), before);
            e.redo();
            assert_eq!(e.project().composition().layers().len(), 1);
        }
        assert!(
            draw_command(
                &Project::default(),
                ShapeKind::Star,
                [0.0, 0.0],
                [0.0, 0.0],
                false,
                false,
                0
            )
            .is_none()
        );
    }
}
