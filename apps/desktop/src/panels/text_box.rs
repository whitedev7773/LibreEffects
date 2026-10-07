use super::*;
pub(super) struct TextBoxDrag {
    start: [f64; 2],
    end: [f64; 2],
    origin: Point<Pixels>,
    zoom: f32,
    bounds: Bounds<Pixels>,
    centered: bool,
    revision: u64,
    frame: u32,
    project: libre_effects_core::Project,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gesture(zoom: f32) -> (EditorState, TextBoxDrag) {
        let mut state = EditorState::default();
        state.tool = Tool::Text;
        state.preview_zoom = Some(zoom);
        let bounds = Bounds::new(point(px(25.0), px(50.0)), size(px(800.0), px(600.0)));
        let comp = state.editor.project().composition();
        let (zoom, origin) = geometry(
            bounds,
            comp.width(),
            comp.height(),
            state.preview_zoom,
            point(px(0.0), px(0.0)),
            state.viewer.rulers,
        );
        let drag = TextBoxDrag::new([100.0, 100.0], origin, zoom, bounds, &state);
        (state, drag)
    }
    #[test]
    fn click_and_drag_threshold_is_screen_space_even_with_zoom_and_alt() {
        for zoom in [0.0625, 0.5, 1.0, 8.0] {
            for centered in [false, true] {
                let (_, mut drag) = gesture(zoom);
                assert!(matches!(
                    drag.action(),
                    Action::BeginText(None, [100.0, 100.0])
                ));
                let down = drag.origin + point(px(100.0 * zoom), px(100.0 * zoom));
                drag.update(down + point(px(2.0), px(2.0)), centered);
                assert!(matches!(
                    drag.action(),
                    Action::BeginText(None, [100.0, 100.0])
                ));
                drag.update(down + point(px(4.0), px(0.0)), centered);
                let Action::BeginParagraph(rect) = drag.action() else {
                    panic!("four-pixel drag must create paragraph")
                };
                assert_eq!(
                    rect[2],
                    (4.0 / zoom as f64 * if centered { 2.0 } else { 1.0 }).max(1.0)
                );
                assert_eq!(rect[3], 1.0);
                drag.update(down + point(px(-12.0), px(-8.0)), centered);
                assert!(matches!(drag.action(), Action::BeginParagraph(_)));
            }
        }
    }
    #[test]
    fn held_text_insertion_rejects_changed_document_frame_tool_and_view() {
        let (mut state, drag) = gesture(0.5);
        assert!(drag.valid(&state, Some(drag.bounds)));
        assert!(!drag.valid(&state, None));
        let mut resized = drag.bounds;
        resized.size.width += px(1.0);
        assert!(!drag.valid(&state, Some(resized)));
        state.preview_pan[0] = 1.0;
        assert!(!drag.valid(&state, Some(drag.bounds)));
        state.preview_pan[0] = 0.0;
        state.preview_zoom = Some(1.0);
        assert!(!drag.valid(&state, Some(drag.bounds)));
        state.preview_zoom = Some(0.5);
        state.viewer.rulers = !state.viewer.rulers;
        assert!(!drag.valid(&state, Some(drag.bounds)));
        state.viewer.rulers = !state.viewer.rulers;
        state.frame = 1;
        assert!(!drag.valid(&state, Some(drag.bounds)));
        state.frame = 0;
        state.document_revision += 1;
        assert!(!drag.valid(&state, Some(drag.bounds)));
        state.document_revision -= 1;
        state.tool = Tool::Select;
        assert!(!drag.valid(&state, Some(drag.bounds)));
        state.tool = Tool::Text;
        state.editor.execute(Command::AddRectangle).unwrap();
        assert!(!drag.valid(&state, Some(drag.bounds)));
    }
    #[test]
    fn paragraph_drag_handles_reverse_direction_and_click_tolerance() {
        let mut drag = TextBoxDrag {
            start: [100.0, 100.0],
            end: [20.0, 40.0],
            origin: point(px(10.0), px(20.0)),
            zoom: 0.5,
            bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0))),
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
    pub fn new(
        p: [f64; 2],
        origin: Point<Pixels>,
        zoom: f32,
        bounds: Bounds<Pixels>,
        s: &EditorState,
    ) -> Self {
        Self {
            start: p,
            end: p,
            origin,
            zoom,
            bounds,
            centered: false,
            revision: s.document_revision,
            frame: s.frame,
            project: s.editor.project().clone(),
        }
    }
    pub fn valid(&self, s: &EditorState, bounds: Option<Bounds<Pixels>>) -> bool {
        let comp = s.editor.project().composition();
        let (zoom, origin) = geometry(
            self.bounds,
            comp.width(),
            comp.height(),
            s.preview_zoom,
            point(px(s.preview_pan[0]), px(s.preview_pan[1])),
            s.viewer.rulers,
        );
        bounds == Some(self.bounds)
            && zoom == self.zoom
            && origin == self.origin
            && s.text_session.is_none()
            && !s.playing
            && s.tool == Tool::Text
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
        // Screen-space motion, independent of zoom or Alt's centered box size.
        let dx = (self.end[0] - self.start[0]) * self.zoom as f64;
        let dy = (self.end[1] - self.start[1]) * self.zoom as f64;
        if dx.hypot(dy) >= 4.0 {
            let [x, y, width, height] = self.rect();
            Action::BeginParagraph([x, y, width.max(1.0), height.max(1.0)])
        } else {
            Action::BeginText(None, self.start)
        }
    }
}
