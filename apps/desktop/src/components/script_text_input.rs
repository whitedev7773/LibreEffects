//! Native, local-only text entry for ScriptUI controls. Script edits never enter
//! the composition editor's transaction or undo history.
use crate::{text_edit::Buffer, ui};
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, ElementInputHandler, EntityInputHandler,
    FocusHandle, KeyDownEvent, MouseButton, Pixels, Point, ShapedLine, TextRun, UTF16Selection,
    Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use std::{ops::Range, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

const MAX_BYTES: usize = 16 * 1024;
const LINE_HEIGHT: f32 = 18.0;
type Change = Rc<dyn Fn(&str, &mut Window, &mut App)>;
type EditActivity = Rc<dyn Fn(&mut Window, &mut App)>;

struct ActiveInput(gpui::WeakEntity<ScriptTextInput>);
impl gpui::Global for ActiveInput {}

struct InputLine {
    range: Range<usize>,
    shaped: ShapedLine,
}

pub(crate) struct ScriptTextInput {
    focus: FocusHandle,
    buffer: Buffer,
    binding: String,
    revision: Option<u64>,
    notified: String,
    multiline: bool,
    height: f32,
    on_change: Change,
    on_edit_activity: Option<EditActivity>,
    lines: Vec<InputLine>,
    bounds: Option<Bounds<Pixels>>,
    scroll: Point<Pixels>,
    reveal_caret: bool,
    preferred_x: Option<Pixels>,
    dragging: bool,
    cancel_composition: bool,
}

impl ScriptTextInput {
    pub fn new(
        cx: &mut Context<Self>,
        multiline: bool,
        on_change: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            focus: cx.focus_handle().tab_index(0).tab_stop(true),
            buffer: Buffer::new(String::new()),
            binding: String::new(),
            revision: None,
            notified: String::new(),
            multiline,
            height: if multiline { 112.0 } else { 24.0 },
            on_change: Rc::new(on_change),
            on_edit_activity: None,
            lines: vec![],
            bounds: None,
            scroll: point(px(0.0), px(0.0)),
            reveal_caret: true,
            preferred_x: None,
            dragging: false,
            cancel_composition: false,
        }
    }

    pub fn value(&self) -> &str {
        &self.buffer.text
    }

    pub fn multiline(&self) -> bool {
        self.multiline
    }

    /// Optional local ownership hook, including marked/unchanged native input.
    /// ScriptUI keeps its existing committed-text-only callback behavior.
    pub fn set_edit_activity(&mut self, callback: impl Fn(&mut Window, &mut App) + 'static) {
        self.on_edit_activity = Some(Rc::new(callback));
    }

    fn edit_activity(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(callback) = &self.on_edit_activity {
            callback(window, cx);
        }
    }

    /// ScriptUI layout supplies a viewport-bounded height. Other native inputs
    /// keep their existing single-line/multiline defaults.
    pub fn set_height(&mut self, height: f32) {
        if self.height != height {
            self.height = height;
            self.reveal_caret = true;
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub fn has_focus(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub fn is_composing(&self) -> bool {
        self.buffer.marked.is_some()
    }

    pub fn has_pending_edit(&self) -> bool {
        self.is_composing() || self.buffer.text != self.notified
    }

    pub fn active_has_focus(window: &Window, cx: &App) -> bool {
        cx.try_global::<ActiveInput>()
            .and_then(|active| active.0.upgrade())
            .is_some_and(|input| input.read(cx).has_focus(window))
    }

    pub fn active_is_composing(window: &Window, cx: &App) -> bool {
        cx.try_global::<ActiveInput>()
            .and_then(|active| active.0.upgrade())
            .is_some_and(|input| {
                let input = input.read(cx);
                input.has_focus(window) && input.is_composing()
            })
    }

    pub fn focus_input(&self, window: &mut Window) {
        window.focus(&self.focus);
    }

    pub fn focus_select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A panel must not consume an unfinished native composition to select
        // another control. Keep the marked selection under the IME's control.
        if !self.is_composing() {
            self.buffer.all();
            self.reveal_caret = true;
        }
        window.focus(&self.focus);
        cx.set_global(ActiveInput(cx.entity().downgrade()));
        cx.notify();
    }

    /// Ordinary redraws cannot replace a focused draft. Values loaded from a
    /// script are bounded at a UTF-8 boundary, like native edits in Buffer.
    pub fn sync(&mut self, binding: String, value: String, window: &Window) {
        let rebound = binding != self.binding;
        if rebound || !self.has_focus(window) {
            self.install(binding, value, rebound);
        }
    }

    /// A changed revision is an authoritative script assignment, including
    /// assignments while focused. Local edit acknowledgements must retain the
    /// revision; callers must not advance it for each ordinary redraw.
    pub fn sync_revision(
        &mut self,
        binding: String,
        value: String,
        revision: u64,
        window: &mut Window,
    ) {
        let rebound = binding != self.binding;
        let authoritative = self.revision != Some(revision);
        // A queued acknowledgement may arrive after native composition has
        // begun. Its pre-composition value must not erase the marked draft.
        let value = bounded_value(&value, self.multiline);
        let keep_marked = keep_composition(rebound, self.is_composing(), &value, &self.notified);
        if !keep_marked && should_sync(rebound, self.has_focus(window), authoritative) {
            // An acknowledgement of the current value is not a new edit: keep
            // its selection, local undo history, and any native marked range.
            self.install(binding, value, rebound);
        }
        self.revision = Some(revision);
        if self.cancel_composition && self.has_focus(window) {
            window.blur();
        }
    }

    fn install(&mut self, binding: String, value: String, reset: bool) {
        let value = bounded_value(&value, self.multiline);
        if reset || value != self.buffer.text {
            self.cancel_composition |= self.is_composing();
            self.buffer = Buffer::new(value.clone());
            self.notified = value;
            self.lines.clear();
            self.preferred_x = None;
            self.scroll = point(px(0.0), px(0.0));
            self.reveal_caret = true;
            self.dragging = false;
        }
        self.binding = binding;
    }

    fn edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.lines.clear();
        self.preferred_x = None;
        self.reveal_caret = true;
        // Intermediate marked text remains entirely local: it must not fire
        // ScriptUI handlers or activate a default/cancel button.
        if !self.is_composing() && self.buffer.text != self.notified {
            self.notified = self.buffer.text.clone();
            (self.on_change)(&self.notified, window, cx);
        }
        cx.notify();
    }

    fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        mark: bool,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cancel_composition || !self.has_focus(window) {
            return;
        }
        // A pending async authoring check must lose ownership before even an
        // empty/identical marked replacement can alter the native draft.
        self.edit_activity(window, cx);
        let text = normalize_text(text, self.multiline);
        if self.buffer.replace(range, &text, mark, selected).is_ok() {
            self.edited(window, cx);
        }
    }

    fn vertical(&mut self, down: bool, extend: bool) {
        let rows = line_ranges(&self.buffer.text);
        let row = row_for_index(&rows, self.buffer.caret);
        let next = if down {
            (row + 1).min(rows.len() - 1)
        } else {
            row.saturating_sub(1)
        };
        if let (Some(current), Some(target)) = (self.lines.get(row), self.lines.get(next)) {
            let x = *self.preferred_x.get_or_insert_with(|| {
                current
                    .shaped
                    .x_for_index(self.buffer.caret - current.range.start)
            });
            let local = target.shaped.closest_index_for_x(x);
            let at = target.range.start
                + grapheme_boundary(&self.buffer.text[target.range.clone()], local);
            self.buffer.select(at, extend);
        } else {
            // Input can arrive before the first paint. Preserve a grapheme
            // column until measured visual lines are available.
            let at = vertical_index(&self.buffer.text, self.buffer.caret, down);
            self.buffer.select(at, extend);
        }
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let command = modifiers.control || modifiers.platform;
        // GPUI sends printable and dead keys to the platform input handler only
        // when propagation remains enabled. Consuming them here would prevent
        // ordinary typing and starting native IME composition. The containing
        // ScriptUI panel and shell must ignore these bubbled text keys while
        // this field is focused. Do not synthesize key_char insertion instead.
        let native = native_text_key(key, modifiers.control, modifiers.platform, modifiers.alt);
        // Linux XKB composed characters and single-byte Wayland IME commits
        // arrive as native KeyDown text while Buffer still has a marked range.
        // Let that replacement through. Platform candidate selection receives
        // Enter/Escape before this listener; any forwarded editing command
        // must remain inert until the marked composition is committed.
        if suppress_key(self.cancel_composition, self.is_composing(), native) {
            cx.stop_propagation();
            return;
        }
        if native {
            // Dead keys can start native composition before a replacement is
            // delivered. Retire async ownership at the first editing intent.
            self.edit_activity(window, cx);
            return;
        }
        cx.stop_propagation();
        if edit_activity_key(key, command, self.multiline) {
            self.edit_activity(window, cx);
        }
        let shift = modifiers.shift;
        let old_text = self.buffer.text.clone();
        if !matches!(key, "up" | "down") {
            self.preferred_x = None;
        }
        if command {
            match key {
                "a" => self.buffer.all(),
                "z" => self.buffer.history(shift),
                "y" => self.buffer.history(true),
                "c" | "x" => {
                    let range = self.buffer.selection();
                    if !range.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            self.buffer.text[range].to_string(),
                        ));
                        if key == "x" {
                            let _ = self.buffer.replace(None, "", false, None);
                        }
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        let text = normalize_text(&text, self.multiline);
                        let _ = self.buffer.replace(None, &text, false, None);
                    }
                }
                "home" | "end" => self.buffer.line_edge(key == "end", true, shift),
                "left" | "right" => {
                    // Command-arrow is a line edge on macOS; Control-arrow is
                    // word movement on the other native platforms.
                    if modifiers.platform {
                        self.buffer.line_edge(key == "right", false, shift);
                    } else {
                        self.buffer.word(key == "right", shift);
                    }
                }
                "up" | "down" => self.buffer.line_edge(key == "down", true, shift),
                "backspace" | "delete" => {
                    if self.buffer.selection().is_empty() {
                        if modifiers.platform {
                            self.buffer.line_edge(key == "delete", false, true);
                        } else {
                            self.buffer.word(key == "delete", true);
                        }
                    }
                    let _ = self.buffer.replace(None, "", false, None);
                }
                _ => {}
            }
        } else {
            match key {
                "enter" if self.multiline => {
                    let _ = self.buffer.replace(None, "\n", false, None);
                }
                "backspace" | "delete" => {
                    if modifiers.alt && self.buffer.selection().is_empty() {
                        self.buffer.word(key == "delete", true);
                    }
                    let _ = self.buffer.delete(key == "delete");
                }
                "left" | "right" => {
                    if modifiers.alt {
                        self.buffer.word(key == "right", shift);
                    } else {
                        self.buffer.step(key == "right", shift);
                    }
                }
                "home" | "end" => self.buffer.line_edge(key == "end", false, shift),
                "up" | "down" => self.vertical(key == "down", shift),
                _ => {}
            }
        }
        self.reveal_caret = true;
        if self.buffer.text != old_text {
            self.edited(window, cx);
        } else {
            cx.notify();
        }
    }

    fn hit(&self, position: Point<Pixels>) -> Option<usize> {
        let bounds = self.bounds?;
        let y = f32::from(position.y - bounds.top() + self.scroll.y).max(0.0);
        let row = (y / LINE_HEIGHT) as usize;
        let line = self.lines.get(row.min(self.lines.len().checked_sub(1)?))?;
        let local = line
            .shaped
            .closest_index_for_x(position.x - bounds.left() + self.scroll.x);
        Some(line.range.start + grapheme_boundary(&self.buffer.text[line.range.clone()], local))
    }

    fn shape(&mut self, window: &mut Window) {
        if !self.lines.is_empty() {
            return;
        }
        let font = window.text_style().font();
        self.lines = line_ranges(&self.buffer.text)
            .into_iter()
            .map(|range| {
                let text = self.buffer.text[range.clone()].to_string();
                let shaped = window.text_system().shape_line(
                    text.clone().into(),
                    px(12.0),
                    &[TextRun {
                        len: text.len(),
                        font: font.clone(),
                        color: rgb(0xdedede).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                InputLine { range, shaped }
            })
            .collect();
    }

    fn clamp_scroll(&mut self, bounds: Bounds<Pixels>) {
        let width = self
            .lines
            .iter()
            .map(|line| f32::from(line.shaped.width))
            .fold(0.0_f32, f32::max);
        let max_x = (width + 3.0 - f32::from(bounds.size.width)).max(0.0);
        let max_y = if self.multiline {
            (self.lines.len() as f32 * LINE_HEIGHT - f32::from(bounds.size.height)).max(0.0)
        } else {
            0.0
        };
        self.scroll.x = px(f32::from(self.scroll.x).clamp(0.0, max_x));
        self.scroll.y = px(f32::from(self.scroll.y).clamp(0.0, max_y));
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.shape(window);
        if self.bounds.is_some_and(|old| old.size != bounds.size) {
            self.reveal_caret = true;
        }
        self.bounds = Some(bounds);
        let caret_row = self
            .lines
            .iter()
            .rposition(|line| line.range.start <= self.buffer.caret)
            .unwrap_or(0);
        if self.reveal_caret {
            let line = &self.lines[caret_row];
            let x = line
                .shaped
                .x_for_index(self.buffer.caret - line.range.start);
            let y = px(caret_row as f32 * LINE_HEIGHT);
            if x < self.scroll.x {
                self.scroll.x = x;
            } else if x + px(2.0) > self.scroll.x + bounds.size.width {
                self.scroll.x = x + px(2.0) - bounds.size.width;
            }
            if y < self.scroll.y {
                self.scroll.y = y;
            } else if y + px(LINE_HEIGHT) > self.scroll.y + bounds.size.height {
                self.scroll.y = y + px(LINE_HEIGHT) - bounds.size.height;
            }
            self.reveal_caret = false;
        }
        self.clamp_scroll(bounds);
        window.handle_input(
            &self.focus,
            ElementInputHandler::new(bounds, cx.entity()),
            cx,
        );
        let focused = self.has_focus(window);
        let selection = self.buffer.selection();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (row, line) in self.lines.iter().enumerate() {
                let origin = bounds.origin
                    + point(-self.scroll.x, px(row as f32 * LINE_HEIGHT) - self.scroll.y);
                if origin.y + px(LINE_HEIGHT) < bounds.top() || origin.y > bounds.bottom() {
                    continue;
                }
                let local_start = selection.start.max(line.range.start).min(line.range.end);
                let local_end = selection.end.min(line.range.end).max(line.range.start);
                let newline_selected = selection.start <= line.range.end
                    && selection.end > line.range.end
                    && line.range.end < self.buffer.text.len();
                if focused && (local_start < local_end || newline_selected) {
                    let x1 = line.shaped.x_for_index(local_start - line.range.start);
                    let x2 = line.shaped.x_for_index(local_end - line.range.start)
                        + if newline_selected { px(5.0) } else { px(0.0) };
                    window.paint_quad(fill(
                        Bounds::new(origin + point(x1, px(0.0)), size(x2 - x1, px(LINE_HEIGHT))),
                        rgb(0x204d76),
                    ));
                }
                let _ = line.shaped.paint(origin, px(LINE_HEIGHT), window, cx);
                if let Some(marked) = &self.buffer.marked {
                    let start = marked.start.max(line.range.start);
                    let end = marked.end.min(line.range.end);
                    if start < end {
                        let x1 = line.shaped.x_for_index(start - line.range.start);
                        let x2 = line.shaped.x_for_index(end - line.range.start);
                        window.paint_quad(fill(
                            Bounds::new(
                                origin + point(x1, px(LINE_HEIGHT - 2.0)),
                                size((x2 - x1).max(px(1.0)), px(1.0)),
                            ),
                            rgb(ui::BLUE),
                        ));
                    }
                }
                if focused && selection.is_empty() && row == caret_row {
                    let x = line
                        .shaped
                        .x_for_index(self.buffer.caret - line.range.start);
                    window.paint_quad(fill(
                        Bounds::new(origin + point(x, px(0.0)), size(px(1.0), px(LINE_HEIGHT))),
                        rgb(ui::BLUE),
                    ));
                }
            }
        });
    }
}

impl EntityInputHandler for ScriptTextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.buffer.byte(range.start)..self.buffer.byte(range.end);
        if range.start > range.end {
            return None;
        }
        *actual = Some(self.buffer.utf16(range.clone()));
        Some(self.buffer.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.utf16(self.buffer.selection()),
            reversed: self.buffer.caret < self.buffer.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer
            .marked
            .clone()
            .map(|range| self.buffer.utf16(range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cancel_composition {
            self.edit_activity(window, cx);
        }
        self.buffer.marked = None;
        if !self.cancel_composition {
            self.edited(window, cx);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range, text, false, None, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range, text, true, selected, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds?;
        let start = self.buffer.byte(range.start);
        let end = self.buffer.byte(range.end).max(start);
        let row = self
            .lines
            .iter()
            .rposition(|line| line.range.start <= start)?;
        let line = &self.lines[row];
        let x1 = line
            .shaped
            .x_for_index(start.min(line.range.end) - line.range.start);
        let x2 = line
            .shaped
            .x_for_index(end.min(line.range.end) - line.range.start);
        let origin = bounds.origin
            + point(
                x1 - self.scroll.x,
                px(row as f32 * LINE_HEIGHT) - self.scroll.y,
            );
        Some(Bounds::new(
            origin,
            size((x2 - x1).max(px(1.0)), px(LINE_HEIGHT)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let at = self.hit(position)?;
        Some(self.buffer.utf16(at..at).start)
    }
}

impl Render for ScriptTextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.cancel_composition {
            if self.has_focus(window) {
                window.blur();
            }
            self.cancel_composition = false;
        }
        if self.has_focus(window) {
            cx.set_global(ActiveInput(cx.entity().downgrade()));
        }
        let input = cx.entity();
        div()
            .id("script-text-input")
            .track_focus(&self.focus)
            .tab_index(0)
            .key_context("ScriptTextInput")
            .cursor_text()
            .h(px(self.height))
            .flex_none()
            .w_full()
            .px_1()
            .py(px(2.0))
            .overflow_hidden()
            .bg(rgb(0x181818))
            .border_1()
            .border_color(rgb(0x373737))
            .focus(|style| style.border_color(rgb(ui::BLUE)))
            .on_key_down(cx.listener(Self::key))
            .on_key_up(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if this.is_composing() {
                        return;
                    }
                    window.focus(&this.focus);
                    cx.set_global(ActiveInput(cx.entity().downgrade()));
                    if let Some(at) = this.hit(event.position) {
                        if event.click_count >= 3 {
                            this.buffer.select_line(at);
                        } else if event.click_count == 2 {
                            this.buffer.select_word(at);
                        } else {
                            this.buffer.select(at, event.modifiers.shift);
                        }
                    }
                    this.preferred_x = None;
                    this.dragging = true;
                    this.reveal_caret = true;
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                cx.stop_propagation();
                let delta = event.delta.pixel_delta(px(LINE_HEIGHT));
                if event.modifiers.shift || !this.multiline {
                    this.scroll.x -= if delta.x == px(0.0) { delta.y } else { delta.x };
                } else {
                    this.scroll.x -= delta.x;
                    this.scroll.y -= delta.y;
                }
                if let Some(bounds) = this.bounds {
                    this.clamp_scroll(bounds);
                }
                this.reveal_caret = false;
                cx.notify();
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        input.update(cx, |this, cx| this.paint(bounds, window, cx));
                        // Register drag listeners every paint so a quick gesture is
                        // handled even before its first mouse-down repaint.
                        let moving = input.clone();
                        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                            if !phase.bubble() || event.pressed_button != Some(MouseButton::Left) {
                                return;
                            }
                            moving.update(cx, |this, cx| {
                                if this.dragging && !this.is_composing() {
                                    if let Some(at) = this.hit(event.position) {
                                        this.buffer.select(at, true);
                                        this.reveal_caret = true;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }
                            });
                        });
                        let ending = input.clone();
                        window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                            if phase.bubble() && event.button == MouseButton::Left {
                                ending.update(cx, |this, cx| {
                                    if this.dragging {
                                        this.dragging = false;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                });
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}

fn suppress_key(cancelled: bool, composing: bool, native_text: bool) -> bool {
    cancelled || (composing && !native_text)
}

fn native_text_key(key: &str, control: bool, platform: bool, alt: bool) -> bool {
    !platform
        && (!control || alt) // AltGr may be reported as Control+Alt.
        && !matches!(
            key,
            "enter" | "tab" | "escape" | "backspace" | "delete" | "left" | "right"
                | "home" | "end" | "up" | "down"
        )
}

fn should_sync(rebound: bool, focused: bool, authoritative: bool) -> bool {
    rebound || !focused || authoritative
}

fn edit_activity_key(key: &str, command: bool, multiline: bool) -> bool {
    matches!(key, "backspace" | "delete")
        || (command && matches!(key, "z" | "y" | "x" | "v"))
        || (!command && multiline && key == "enter")
}

fn keep_composition(rebound: bool, composing: bool, value: &str, notified: &str) -> bool {
    !rebound && composing && value == notified
}

fn normalize_text(text: &str, multiline: bool) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !c.is_control() || (multiline && matches!(c, '\n' | '\t')))
        .collect()
}

fn bounded_value(text: &str, multiline: bool) -> String {
    let mut value = normalize_text(text, multiline);
    if value.len() > MAX_BYTES {
        let mut end = MAX_BYTES;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
    value
}

/// Include the empty trailing line so Enter at EOF has a real caret location.
fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut offset = 0;
    text.split('\n')
        .map(|line| {
            let range = offset..offset + line.len();
            offset += line.len() + 1;
            range
        })
        .collect()
}

fn row_for_index(rows: &[Range<usize>], at: usize) -> usize {
    rows.iter()
        .rposition(|range| range.start <= at)
        .unwrap_or(0)
}

fn grapheme_boundary(text: &str, at: usize) -> usize {
    if at >= text.len() {
        return text.len();
    }
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .take_while(|index| *index <= at)
        .last()
        .unwrap_or(0)
}

fn vertical_index(text: &str, caret: usize, down: bool) -> usize {
    let rows = line_ranges(text);
    let row = row_for_index(&rows, caret);
    let next = if down {
        (row + 1).min(rows.len() - 1)
    } else {
        row.saturating_sub(1)
    };
    let column = text[rows[row].start..caret].graphemes(true).count();
    let target = &rows[next];
    target.start
        + text[target.clone()]
            .grapheme_indices(true)
            .nth(column)
            .map_or(target.len(), |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_activity_covers_local_mutation_attempts_without_treating_copy_as_an_edit() {
        for key in ["z", "y", "x", "v", "backspace", "delete"] {
            assert!(edit_activity_key(key, true, true), "{key}");
        }
        assert!(edit_activity_key("enter", false, true));
        assert!(!edit_activity_key("enter", true, true));
        assert!(!edit_activity_key("enter", false, false));
        for key in ["c", "a", "left", "right", "home", "end"] {
            assert!(!edit_activity_key(key, true, true), "{key}");
        }
    }

    #[test]
    fn native_typing_and_dead_keys_reach_ime_but_editing_and_shortcuts_stay_local() {
        for key in ["a", "space", "é", "dead_acute"] {
            assert!(native_text_key(key, false, false, false));
        }
        assert!(native_text_key("q", true, false, true)); // AltGr
        assert!(!native_text_key("z", true, false, false));
        assert!(!native_text_key("s", false, true, false));
        for key in ["enter", "tab", "escape", "left", "delete"] {
            assert!(!native_text_key(key, false, false, false));
        }
    }

    #[test]
    fn marked_linux_native_completion_can_replace_the_preedit() {
        let mut buffer = Buffer::new(String::new());
        buffer.replace(None, "´", true, None).unwrap();
        let native = native_text_key("eacute", false, false, false);
        assert!(!suppress_key(false, buffer.marked.is_some(), native));
        buffer.replace(None, "é", false, None).unwrap();
        assert_eq!(buffer.text, "é");
        assert!(buffer.marked.is_none());
        assert!(suppress_key(false, true, false)); // Marked Enter or shortcut.
        assert!(suppress_key(true, true, true)); // Retired binding's late input.
        assert!(!suppress_key(false, false, false)); // Ordinary local editing.
    }

    #[test]
    fn multiline_paste_keeps_real_lines_and_singleline_filters_them() {
        assert_eq!(normalize_text("a\r\nb\rc\n\tδ\0", true), "a\nb\nc\n\tδ");
        assert_eq!(normalize_text("a\r\nb\rc\n\tδ\0", false), "abcδ");
        assert_eq!(line_ranges("a\n\nβ\n"), vec![0..1, 2..2, 3..5, 6..6]);
        assert_eq!(line_ranges(""), vec![0..0]);
    }

    #[test]
    fn native_limits_never_split_utf8_and_reject_oversize_edits() {
        let large = "é".repeat(MAX_BYTES);
        let text = bounded_value(&large, true);
        assert_eq!(text.len(), MAX_BYTES);
        let mut buffer = Buffer::new(text.clone());
        assert!(buffer.replace(None, "x", false, None).is_err());
        assert_eq!(buffer.text, text);
        let odd = format!("x{}", "😀".repeat(MAX_BYTES));
        assert_eq!(bounded_value(&odd, true).len(), MAX_BYTES - 3);
    }

    #[test]
    fn revisions_preserve_live_edits_until_authoritative_update() {
        assert!(!should_sync(false, true, false));
        assert!(should_sync(true, true, false));
        assert!(should_sync(false, true, true));
        assert!(should_sync(false, false, false));
        assert!(keep_composition(false, true, "prior", "prior"));
        assert!(!keep_composition(true, true, "prior", "prior"));
        assert!(!keep_composition(false, false, "prior", "prior"));
        assert!(!keep_composition(false, true, "script assignment", "prior"));
    }

    #[test]
    fn vertical_navigation_uses_unicode_graphemes_and_empty_lines() {
        let text = "e\u{301}😀z\naβ\n\n末尾";
        assert_eq!(
            vertical_index(text, "e\u{301}😀".len(), true),
            "e\u{301}😀z\naβ".len()
        );
        let empty = "e\u{301}😀z\naβ\n".len();
        assert_eq!(vertical_index(text, "e\u{301}😀z\naβ".len(), true), empty);
        assert_eq!(vertical_index(text, empty, true), empty + 1);
        assert_eq!(grapheme_boundary("e\u{301}😀", 2), 0);
        assert_eq!(grapheme_boundary("e\u{301}😀", 5), 3);
    }

    #[test]
    fn composition_and_undo_stay_in_the_local_buffer() {
        let mut buffer = Buffer::new("a\n".into());
        buffer.replace(None, "に", true, Some(1..1)).unwrap();
        assert!(buffer.marked.is_some());
        buffer.replace(None, "日本", false, None).unwrap();
        assert_eq!(buffer.text, "a\n日本");
        assert!(buffer.marked.is_none());
        buffer.history(false);
        assert_eq!(buffer.text, "a\n");
        buffer.history(true);
        assert_eq!(buffer.text, "a\n日本");
    }
}
