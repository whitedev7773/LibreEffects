use super::*;
pub(super) struct TextBoxDrag {
    start: [f64; 2],
    end: [f64; 2],
    origin: Point<Pixels>,
    zoom: f32,
    centered: bool,
    revision: u64,
    frame: u32,
    project: libre_effects_core::Project,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paragraph_drag_handles_reverse_direction_and_click_tolerance() {
        let mut drag = TextBoxDrag {
            start: [100.0, 100.0],
            end: [20.0, 40.0],
            origin: point(px(10.0), px(20.0)),
            zoom: 0.5,
            centered: false,
            revision: 0,
            frame: 0,
            project: Default::default(),
        };
        assert_eq!(drag.rect(), [20.0, 40.0, 80.0, 60.0]);
        assert!(matches!(
            drag.action(),
            Action::BeginParagraph([20.0, 40.0, 80.0, 60.0])
        ));
        drag.centered = true;
        assert_eq!(drag.rect(), [20.0, 40.0, 160.0, 120.0]);
        drag.update(point(px(61.0), px(71.0)), false);
        assert!(matches!(
            drag.action(),
            Action::BeginText(None, [100.0, 100.0])
        ));
    }
}
impl TextBoxDrag {
    pub fn new(p: [f64; 2], origin: Point<Pixels>, zoom: f32, s: &EditorState) -> Self {
        Self {
            start: p,
            end: p,
            origin,
            zoom,
            centered: false,
            revision: s.document_revision,
            frame: s.frame,
            project: s.editor.project().clone(),
        }
    }
    pub fn valid(&self, s: &EditorState) -> bool {
        s.tool == Tool::Text
            && s.document_revision == self.revision
            && s.frame == self.frame
            && s.editor.project() == &self.project
    }
    pub fn update(&mut self, p: Point<Pixels>, centered: bool) {
        self.centered = centered;
        self.end = [
            f32::from(p.x - self.origin.x) as f64 / self.zoom as f64,
            f32::from(p.y - self.origin.y) as f64 / self.zoom as f64,
        ];
    }
    pub fn rect(&self) -> [f64; 4] {
        if self.centered {
            let dx = (self.start[0] - self.end[0]).abs();
            let dy = (self.start[1] - self.end[1]).abs();
            return [self.start[0] - dx, self.start[1] - dy, dx * 2.0, dy * 2.0];
        }
        [
            self.start[0].min(self.end[0]),
            self.start[1].min(self.end[1]),
            (self.start[0] - self.end[0]).abs(),
            (self.start[1] - self.end[1]).abs(),
        ]
    }
    pub fn action(&self) -> Action {
        let r = self.rect();
        if r[2] * self.zoom as f64 >= 4.0 && r[3] * self.zoom as f64 >= 4.0 {
            Action::BeginParagraph(r)
        } else {
            Action::BeginText(None, self.start)
        }
    }
}
