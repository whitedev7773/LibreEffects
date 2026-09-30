use crate::ui;
use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, FocusHandle,
    KeyDownEvent, MouseButton, Pixels, Point, ShapedLine, Subscription, TextRun, UTF16Selection,
    Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use std::{ops::Range, rc::Rc};

type Commit = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// Single-line Unicode input. Changes are committed together on Enter/blur;
/// Escape restores the original value without an editing command.
pub(crate) struct TextField {
    focus: FocusHandle,
    content: String,
    original: String,
    binding: String,
    selection: Range<usize>,
    marked: Option<Range<usize>>,
    line: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    blur: Option<Subscription>,
    commit: Commit,
}

impl TextField {
    pub fn new(
        cx: &mut Context<Self>,
        commit: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            focus: cx.focus_handle(),
            content: String::new(),
            original: String::new(),
            binding: String::new(),
            selection: 0..0,
            marked: None,
            line: None,
            bounds: None,
            blur: None,
            commit: Rc::new(commit),
        }
    }
    pub fn sync(&mut self, binding: String, value: String, window: &Window) {
        if binding != self.binding || !self.focus.is_focused(window) {
            self.binding = binding;
            self.original = value.clone();
            self.content = value;
            self.selection = self.content.len()..self.content.len();
            self.marked = None;
        }
    }
    pub fn value(&self) -> &str {
        &self.content
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.content != self.original {
            let value = self.content.clone();
            self.original = value.clone();
            (self.commit)(&value, window, cx);
        }
        cx.notify();
    }
    fn byte_offset(&self, utf16: usize) -> usize {
        let mut units = 0;
        for (offset, ch) in self.content.char_indices() {
            if units >= utf16 {
                return offset;
            }
            units += ch.len_utf16();
        }
        self.content.len()
    }
    fn utf16_range(&self, range: Range<usize>) -> Range<usize> {
        self.content[..range.start].encode_utf16().count()
            ..self.content[..range.end].encode_utf16().count()
    }
    fn replace(&mut self, range: Option<Range<usize>>, text: &str, cx: &mut Context<Self>) {
        let range = range
            .map(|r| self.byte_offset(r.start)..self.byte_offset(r.end))
            .unwrap_or_else(|| self.marked.clone().unwrap_or(self.selection.clone()));
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if self.content.len() - (range.end - range.start) + text.len() > 1024 {
            return;
        }
        self.content.replace_range(range.clone(), &text);
        let end = range.start + text.len();
        self.selection = end..end;
        self.marked = None;
        cx.notify();
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let control = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        cx.stop_propagation();
        if control {
            match key {
                "a" => self.selection = 0..self.content.len(),
                "c" | "x" => {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.content[self.selection.clone()].to_string(),
                    ));
                    if key == "x" {
                        self.replace(None, "", cx);
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.replace(None, &text, cx);
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }
        match key {
            "enter" => {
                self.submit(window, cx);
                window.blur();
            }
            "escape" => {
                self.content = self.original.clone();
                self.selection = 0..0;
                self.marked = None;
                window.blur();
            }
            "backspace" => {
                if self.selection.is_empty() {
                    self.selection.start = self.content[..self.selection.start]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(i, _)| i);
                }
                self.replace(None, "", cx);
            }
            "delete" => {
                if self.selection.is_empty() {
                    self.selection.end += self.content[self.selection.end..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8);
                }
                self.replace(None, "", cx);
            }
            "left" => {
                let at = if self.selection.is_empty() {
                    self.content[..self.selection.start]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(i, _)| i)
                } else {
                    self.selection.start
                };
                self.selection = at..at;
            }
            "right" => {
                let at = if self.selection.is_empty() {
                    self.selection.end
                        + self.content[self.selection.end..]
                            .chars()
                            .next()
                            .map_or(0, char::len_utf8)
                } else {
                    self.selection.end
                };
                self.selection = at..at;
            }
            "home" => self.selection = 0..0,
            "end" => self.selection = self.content.len()..self.content.len(),
            _ => {}
        }
        cx.notify();
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.byte_offset(range.start)..self.byte_offset(range.end);
        *actual = Some(self.utf16_range(range.clone()));
        Some(self.content[range].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.utf16_range(self.selection.clone()),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|range| self.utf16_range(range))
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range, text, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = range
            .as_ref()
            .map(|r| self.byte_offset(r.start))
            .or_else(|| self.marked.as_ref().map(|r| r.start))
            .unwrap_or(self.selection.start);
        self.replace(range, text, cx);
        let end = self.selection.end;
        if end > start {
            self.marked = Some(start..end);
        }
        if let Some(selected) = selected {
            let base = self.content[..start].encode_utf16().count();
            self.selection =
                self.byte_offset(base + selected.start)..self.byte_offset(base + selected.end);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.line.as_ref()?;
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(self.byte_offset(range.start)),
                bounds.top(),
            ),
            point(
                bounds.left() + line.x_for_index(self.byte_offset(range.end)),
                bounds.bottom(),
            ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let i = self
            .line
            .as_ref()?
            .closest_index_for_x(p.x - self.bounds?.left());
        Some(
            self.content[..i.min(self.content.len())]
                .encode_utf16()
                .count(),
        )
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.blur.is_none() {
            self.blur = Some(cx.on_blur(&self.focus.clone(), window, |this, window, cx| {
                this.submit(window, cx)
            }));
        }
        let input = cx.entity();
        div()
            .id("text-field")
            .track_focus(&self.focus)
            .tab_index(0)
            .cursor_text()
            .h(px(24.0))
            .w_full()
            .px_1()
            .overflow_hidden()
            .bg(rgb(0x181818))
            .border_1()
            .border_color(rgb(0x373737))
            .focus(|s| s.border_color(rgb(ui::BLUE)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus);
                    this.selection = 0..this.content.len();
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(Self::key))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let field = input.read(cx);
                        let focus = field.focus.clone();
                        let content = field.content.clone();
                        let selection = field.selection.clone();
                        let style = window.text_style();
                        let line = window.text_system().shape_line(
                            content.clone().into(),
                            px(12.0),
                            &[TextRun {
                                len: content.len(),
                                font: style.font(),
                                color: rgb(ui::BLUE).into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        );
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, input.clone()),
                            cx,
                        );
                        if focus.is_focused(window) {
                            let x = bounds.left() + line.x_for_index(selection.start);
                            let width = if selection.is_empty() {
                                px(1.0)
                            } else {
                                line.x_for_index(selection.end) - line.x_for_index(selection.start)
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(x, bounds.top()),
                                    size(width, bounds.size.height),
                                ),
                                rgb(if selection.is_empty() {
                                    ui::BLUE
                                } else {
                                    0x204d76
                                }),
                            ));
                        }
                        let _ = line.paint(bounds.origin, px(22.0), window, cx);
                        input.update(cx, |field, _| {
                            field.line = Some(line);
                            field.bounds = Some(bounds);
                        });
                    },
                )
                .size_full(),
            )
    }
}
