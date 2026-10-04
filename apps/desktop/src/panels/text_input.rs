use super::*;
use crate::text_edit::{Session, layout::Layout};
use gpui::{App, ClipboardItem, ElementInputHandler, EntityInputHandler, UTF16Selection};
use std::ops::Range;

impl Preview {
    pub(super) fn resize_text(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(p) = self.text_point(p, cx) else {
            return;
        };
        self.state.update(cx, |s, cx| {
            if let Some(session) = &mut s.text_session {
                if session.style.paragraph {
                    session.width = p[0].clamp(1.0, 16384.0);
                    session.height = p[1].clamp(1.0, 16384.0);
                    session.preferred_x = None;
                    session.caret_hint = None;
                    cx.notify();
                }
            }
        });
    }
    fn text_point(&self, p: Point<Pixels>, cx: &Context<Self>) -> Option<[f64; 2]> {
        let s = self.state.read(cx);
        let session = s.text_session.as_ref()?;
        let comp = s.editor.project().composition();
        let (zoom, origin) = geometry(
            self.bounds.get()?,
            comp.width(),
            comp.height(),
            s.preview_zoom,
            point(px(s.preview_pan[0]), px(s.preview_pan[1])),
            s.viewer.rulers,
        );
        Some(session.world.inverse()?.point([
            f32::from(p.x - origin.x) as f64 / zoom as f64,
            f32::from(p.y - origin.y) as f64 / zoom as f64,
        ]))
    }
    pub(super) fn text_pointer(&mut self, p: Point<Pixels>, extend: bool, cx: &mut Context<Self>) {
        let Some(p) = self.text_point(p, cx) else {
            return;
        };
        self.state.update(cx, |s, cx| {
            if let Some(session) = &mut s.text_session {
                let (at, point) = Layout::new(session).hit_caret(p);
                session.preferred_x = None;
                session.buffer.select(at, extend);
                session.caret_hint = Some((at, point));
                cx.notify();
            }
        });
    }
    pub(super) fn text_click(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        self.text_pointer(e.position, e.modifiers.shift, cx);
        if e.click_count >= 2 {
            let local = self.text_point(e.position, cx);
            self.state.update(cx, |s, cx| {
                if let Some(session) = &mut s.text_session {
                    let at = local.map_or(session.buffer.caret, |p| {
                        Layout::new(session).hit_character(p)
                    });
                    if e.click_count == 2 {
                        session.buffer.select_word(at);
                    } else {
                        session.buffer.select_line(at);
                    }
                    cx.notify();
                }
            });
        }
    }
    pub(super) fn text_key(
        &mut self,
        e: &gpui::KeyDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.state.read(cx).text_session.is_none() {
            return false;
        }
        let key = e.keystroke.key.as_str();
        let ctrl = e.keystroke.modifiers.control || e.keystroke.modifiers.platform;
        let shift = e.keystroke.modifiers.shift;
        if (ctrl && matches!(key, "s" | "o" | "n")) || (e.keystroke.modifiers.alt && key == "f4") {
            self.state.update(cx, |s, cx| s.finish_text(true, cx));
            return false;
        }
        cx.stop_propagation();
        if key == "escape" || (ctrl && key == "enter") {
            self.state.update(cx, |s, cx| {
                s.dispatch(
                    &if key == "escape" {
                        Action::CancelText
                    } else {
                        Action::CommitText
                    },
                    w,
                    cx,
                )
            });
            self.text_dragging = false;
            self.text_resizing = false;
            return true;
        }
        self.state.update(cx, |s, cx| {
            let Some(session) = &mut s.text_session else {
                return;
            };
            let mut error = None;
            if !matches!(key, "up" | "down") {
                session.preferred_x = None;
                if !matches!(key, "home" | "end") {
                    session.caret_hint = None;
                }
            }
            if ctrl {
                match key {
                    "a" => session.buffer.all(),
                    "z" => session.buffer.history(shift),
                    "y" => session.buffer.history(true),
                    "c" | "x" => {
                        let r = session.buffer.selection();
                        if !r.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                session.buffer.text[r].into(),
                            ));
                            if key == "x" {
                                error = session.buffer.replace(None, "", false, None).err();
                            }
                        }
                    }
                    "v" => {
                        if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                            error = session.buffer.replace(None, &text, false, None).err();
                        }
                    }
                    "home" | "end" => session.line_edge(key == "end", true, shift),
                    "left" | "right" => session.buffer.word(key == "right", shift),
                    "backspace" | "delete" => {
                        if session.buffer.selection().is_empty() {
                            session.buffer.word(key == "delete", true);
                        }
                        error = session.buffer.replace(None, "", false, None).err();
                    }
                    _ => {}
                }
            } else {
                match key {
                    "enter" if session.buffer.marked.is_none() => {
                        error = session
                            .buffer
                            .replace(
                                None,
                                if shift && session.style.paragraph {
                                    "\u{2028}"
                                } else {
                                    "\n"
                                },
                                false,
                                None,
                            )
                            .err()
                    }
                    "tab" => error = session.buffer.replace(None, "    ", false, None).err(),
                    "backspace" | "delete" => error = session.buffer.delete(key == "delete").err(),
                    "left" | "right" => session.buffer.step(key == "right", shift),
                    "home" | "end" => session.line_edge(key == "end", false, shift),
                    "up" | "down" => {
                        session.vertical(key == "down", shift);
                    }
                    _ => {}
                }
            }
            if let Some(error) = error {
                s.status = error;
            }
            cx.notify();
        });
        true
    }
    fn replace_text(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        mark: bool,
        selected: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |s, cx| {
            if let Some(session) = &mut s.text_session {
                session.preferred_x = None;
                session.caret_hint = None;
                if let Err(e) = session.buffer.replace(range, text, mark, selected) {
                    s.status = e;
                }
                cx.notify();
            }
        });
    }
}
impl EntityInputHandler for Preview {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let s = self.state.read(cx).text_session.as_ref()?;
        let r = s.buffer.byte(r.start)..s.buffer.byte(r.end);
        if r.start > r.end {
            return None;
        }
        *actual = Some(s.buffer.utf16(r.clone()));
        Some(s.buffer.text[r].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let s = self.state.read(cx).text_session.as_ref()?;
        Some(UTF16Selection {
            range: s.buffer.utf16(s.buffer.selection()),
            reversed: s.buffer.caret < s.buffer.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let s = self.state.read(cx).text_session.as_ref()?;
        s.buffer.marked.clone().map(|r| s.buffer.utf16(r))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            if let Some(s) = &mut s.text_session {
                s.buffer.marked = None;
            }
            cx.notify();
        });
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text(r, text, false, None, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text(r, text, true, selected, cx);
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let state = self.state.read(cx);
        let s = state.text_session.as_ref()?;
        let comp = state.editor.project().composition();
        let (zoom, origin) = geometry(
            self.bounds.get()?,
            comp.width(),
            comp.height(),
            state.preview_zoom,
            point(px(state.preview_pan[0]), px(state.preview_pan[1])),
            state.viewer.rulers,
        );
        let layout = Layout::new(s);
        let index = s.buffer.byte(r.start);
        let at = if index == s.buffer.caret {
            s.caret_position(&layout)
        } else {
            layout.caret(index)
        };
        let [x, y, width, height] = layout.caret_rect(at);
        let corners = [
            [x, y],
            [x + width, y],
            [x + width, y + height],
            [x, y + height],
        ]
        .map(|p| s.world.point(p));
        let min = corners
            .iter()
            .fold([f64::INFINITY; 2], |a, p| [a[0].min(p[0]), a[1].min(p[1])]);
        let max = corners.iter().fold([f64::NEG_INFINITY; 2], |a, p| {
            [a[0].max(p[0]), a[1].max(p[1])]
        });
        Some(Bounds::from_corners(
            origin
                + point(
                    px((min[0] * zoom as f64) as f32),
                    px((min[1] * zoom as f64) as f32),
                ),
            origin
                + point(
                    px((max[0] * zoom as f64) as f32),
                    px((max[1] * zoom as f64) as f32),
                ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let p = self.text_point(p, cx)?;
        let s = self.state.read(cx).text_session.as_ref()?;
        let at = Layout::new(s).hit(p);
        Some(s.buffer.utf16(at..at).start)
    }
}
pub(super) fn paint(
    session: &Session,
    origin: Point<Pixels>,
    zoom: f32,
    bounds: Bounds<Pixels>,
    focus: &FocusHandle,
    input: Entity<Preview>,
    resize_handle: &Rc<Cell<Option<Bounds<Pixels>>>>,
    w: &mut Window,
    cx: &mut App,
) {
    w.handle_input(focus, ElementInputHandler::new(bounds, input), cx);
    let layout = Layout::new(session);
    let selection = session.buffer.selection();
    let to_screen = |p| {
        let p = session.world.point(p);
        origin + point(px(p[0] as f32 * zoom), px(p[1] as f32 * zoom))
    };
    let mut quad = |x1: f64, x2: f64, y: f64, height: f64, color: gpui::Hsla| {
        let (x1, x2, y, height) = if session.style.paragraph {
            let top = y.clamp(0.0, session.height);
            (
                x1.clamp(0.0, session.width),
                x2.clamp(0.0, session.width),
                top,
                (y + height).clamp(0.0, session.height) - top,
            )
        } else {
            (x1, x2, y, height)
        };
        if height <= 0.0 || x1 == x2 {
            return;
        }
        let points = [[x1, y], [x2, y], [x2, y + height], [x1, y + height]].map(to_screen);
        let mut path = PathBuilder::fill();
        path.move_to(points[0]);
        for p in &points[1..] {
            path.line_to(*p);
        }
        path.close();
        if let Ok(path) = path.build() {
            w.paint_path(path, color);
        }
    };
    for cell in &layout.cells {
        if cell.range.start < selection.end && cell.range.end > selection.start {
            quad(
                cell.x1,
                cell.x2,
                cell.y,
                layout.line_height(),
                gpui::rgba(0x3388ee55).into(),
            );
        }
        if session
            .buffer
            .marked
            .as_ref()
            .is_some_and(|r| cell.range.start < r.end && cell.range.end > r.start)
        {
            quad(
                cell.x1,
                cell.x2,
                cell.y + layout.size * 1.12,
                1.5 / zoom as f64,
                rgb(ui::BLUE).into(),
            );
        }
    }
    let p = session.caret_position(&layout);
    quad(
        p[0],
        p[0] + 1.5 / zoom as f64,
        p[1],
        layout.line_height(),
        rgb(0xffffff).into(),
    );
    if session.style.paragraph {
        let corner = to_screen([session.width, session.height]);
        resize_handle.set(Some(Bounds::new(
            corner - point(px(7.0), px(7.0)),
            gpui::size(px(14.0), px(14.0)),
        )));
        w.paint_quad(gpui::fill(
            Bounds::new(
                corner - point(px(3.0), px(3.0)),
                gpui::size(px(6.0), px(6.0)),
            ),
            rgb(ui::BLUE),
        ));
        let corners = [
            [0.0, 0.0],
            [session.width, 0.0],
            [session.width, session.height],
            [0.0, session.height],
        ]
        .map(to_screen);
        let mut path = PathBuilder::stroke(px(1.0));
        path.move_to(corners[0]);
        for p in &corners[1..] {
            path.line_to(*p);
        }
        path.close();
        if let Ok(path) = path.build() {
            w.paint_path(path, rgb(ui::BLUE));
        }
        let flow = crate::text_flow::lines(
            &session.buffer.text,
            session.font_size,
            session.width,
            &session.style,
        );
        if crate::text_flow::composed_count(&flow, session.height) < flow.len() {
            let position = to_screen([session.width, session.height]);
            w.paint_quad(gpui::fill(
                Bounds::new(
                    position - point(px(4.0), px(4.0)),
                    gpui::size(px(8.0), px(8.0)),
                ),
                rgb(0xef7755),
            ));
        }
    }
}
