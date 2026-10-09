use crate::ui;
use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, FocusHandle,
    KeyDownEvent, MouseButton, Pixels, Point, ShapedLine, Subscription, TextRun, UTF16Selection,
    Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use std::{ops::Range, rc::Rc};

// Guarded fields return their authoritative source display after validation.
// Ordinary fields retain their existing optimistic acceptance policy.
type Commit = Rc<dyn Fn(&str, &mut Window, &mut App) -> Option<String>>;

struct ActiveField(gpui::WeakEntity<TextField>);
impl gpui::Global for ActiveField {}

/// Single-line Unicode input. Changes are committed together on Enter/blur;
/// Escape restores the original value without an editing command.
pub(crate) struct TextField {
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    content: String,
    original: String,
    binding: String,
    selection: Range<usize>,
    marked: Option<Range<usize>>,
    line: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    blur: Option<Subscription>,
    commit: Commit,
    numeric: bool,
    activate: Option<Rc<dyn Fn(&mut Window, &mut gpui::App) -> bool>>,
    dense: bool,
    integer: bool,
    guarded: bool,
    scrub: Option<(Pixels, f64)>,
    scrubbed: bool,
}

impl TextField {
    pub fn has_focus(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
    pub fn has_pending_edit(&self) -> bool {
        self.content != self.original
    }
    pub fn focus_input(&self, window: &mut Window) {
        window.focus(&self.focus);
    }
    pub fn focus_select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selection = 0..self.content.len();
        window.focus(&self.focus);
        cx.set_global(ActiveField(cx.entity().downgrade()));
        cx.notify();
    }
    pub fn is_composing(window: &Window, cx: &App) -> bool {
        cx.try_global::<ActiveField>()
            .and_then(|active| active.0.upgrade())
            .is_some_and(|field| {
                let field = field.read(cx);
                field.focus.is_focused(window) && field.marked.is_some()
            })
    }
    /// Read before a guarded pointer flush; a pending draft requires an explicit
    /// successful receipt from its owning field callback.
    pub(crate) fn active_pending_binding(cx: &App) -> Option<String> {
        cx.try_global::<ActiveField>()
            .and_then(|active| active.0.upgrade())
            .and_then(|field| {
                let field = field.read(cx);
                field.has_pending_edit().then(|| field.binding.clone())
            })
    }
    /// Async source additions cannot consume a pending draft or IME composition.
    /// Command-search queries are not source drafts. This deliberately does
    /// not submit or blur the owning field.
    pub(crate) fn active_has_pending_source_input(cx: &App) -> bool {
        cx.try_global::<ActiveField>()
            .and_then(|active| active.0.upgrade())
            .is_some_and(|field| {
                let field = field.read(cx);
                source_input_pending(
                    &field.binding,
                    field.has_pending_edit(),
                    field.marked.is_some(),
                )
            })
    }
    pub(crate) fn active_has_focus(window: &Window, cx: &App) -> bool {
        cx.try_global::<ActiveField>()
            .and_then(|active| active.0.upgrade())
            .is_some_and(|field| field.read(cx).has_focus(window))
    }
    pub fn commit_active(window: &mut Window, cx: &mut App) {
        let field = cx.try_global::<ActiveField>().map(|f| f.0.clone());
        if let Some(field) = field {
            let _ = field.update(cx, |field, cx| field.submit(window, cx));
        }
    }
    /// Preview's outside capture may finish a text draft before the Character
    /// field receives its own outside event. Flush only its focused, unmarked
    /// selection-bound field; unrelated source fields retain their own route.
    pub(crate) fn commit_text_selection_active(window: &mut Window, cx: &mut App) {
        let field = cx
            .try_global::<ActiveField>()
            .and_then(|active| active.0.upgrade())
            .filter(|field| {
                let field = field.read(cx);
                field.has_focus(window)
                    && field.marked.is_none()
                    && field.binding.starts_with("text-selection:")
            });
        if let Some(field) = field {
            field.update(cx, |field, cx| field.submit(window, cx));
        }
    }
    pub fn new(
        cx: &mut Context<Self>,
        commit: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            focus: cx.focus_handle(),
            return_focus: None,
            content: String::new(),
            original: String::new(),
            binding: String::new(),
            selection: 0..0,
            marked: None,
            line: None,
            bounds: None,
            blur: None,
            commit: Rc::new(move |value, w, cx| {
                commit(value, w, cx);
                None
            }),
            numeric: false,
            activate: None,
            dense: false,
            integer: false,
            guarded: false,
            scrub: None,
            scrubbed: false,
        }
    }
    pub fn on_activate(
        mut self,
        activate: impl Fn(&mut Window, &mut gpui::App) -> bool + 'static,
    ) -> Self {
        self.activate = Some(Rc::new(activate));
        self
    }
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }
    pub fn numeric(mut self) -> Self {
        self.numeric = true;
        self
    }
    pub fn integer(mut self) -> Self {
        self.numeric = true;
        self.integer = true;
        self
    }
    /// Opt in only where the owning panel handles safe Tab traversal. GPUI's
    /// element tab_index does not update an explicitly tracked focus handle.
    pub fn tab_stop(mut self) -> Self {
        self.focus = self.focus.tab_index(0).tab_stop(true);
        self
    }
    /// Keep keyboard editing in the owning panel after Enter or Escape.
    pub fn return_focus(mut self, focus: FocusHandle) -> Self {
        self.return_focus = Some(focus);
        self
    }
    fn finish_input(&self, window: &mut Window) {
        if let Some(focus) = &self.return_focus {
            window.focus(focus);
        } else {
            window.blur();
        }
    }
    pub fn set_numeric(&mut self) {
        self.numeric = true;
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
    /// Bulk-only source-bound text entry. The callback must return the current
    /// source display after either acceptance or rejection. Binding changes
    /// discard pending text before installing a fresh callback, even in focus.
    /// These fields deliberately have no numeric scrub behavior.
    pub fn sync_guarded(
        &mut self,
        binding: String,
        value: String,
        window: &mut Window,
        commit: impl Fn(&str, &mut Window, &mut App) -> String + 'static,
    ) {
        let cancel_composition = binding != self.binding && self.marked.is_some();
        let keep_selection = preserve_guarded_selection(
            binding != self.binding,
            self.focus.is_focused(window),
            &self.content,
            &self.original,
            &value,
            self.marked.is_some(),
        );
        let selection = self.selection.clone();
        self.sync(binding, value, window);
        if keep_selection {
            // Outside-down may commit a different bulk field before this one
            // receives mouse-down/select-all. A source rebind must not turn the
            // next keystroke into append-to-old-value by losing that selection.
            self.selection = selection;
        }
        self.guarded = true;
        self.numeric = false;
        self.scrub = None;
        self.scrubbed = false;
        self.commit = Rc::new(move |value, w, cx| Some(commit(value, w, cx)));
        if cancel_composition && self.focus.is_focused(window) {
            // An external context change must not let a delayed native IME
            // commit land in the freshly rebound selection/frame.
            window.blur();
        }
    }
    pub fn value(&self) -> &str {
        &self.content
    }
    fn scrub_to(&mut self, x: Pixels, shift: bool, alt: bool) {
        if let Some((origin, value)) = self.scrub {
            let delta = f32::from(x - origin) as f64;
            if delta.abs() > 3.0 || self.scrubbed {
                self.scrubbed = true;
                let step = if shift {
                    10.0
                } else if alt {
                    0.1
                } else {
                    1.0
                };
                self.content = scrub_text(value + delta * step, self.integer);
                self.selection = 0..self.content.len();
            }
        }
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if pending_submission(
            &self.content,
            &self.original,
            self.guarded,
            self.marked.is_some(),
        ) {
            let value = self.content.clone();
            self.original = value.clone();
            if let Some(source) = (self.commit)(&value, window, cx) {
                self.original = source.clone();
                self.content = source;
                self.selection = self.content.len()..self.content.len();
                self.marked = None;
            }
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
        if key == "escape" {
            self.scrub = None;
            self.scrubbed = false;
        }
        let control = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        // Do not synthesize printable insertion here: the platform delivers
        // exactly one composed/IME commit through EntityInputHandler.
        if libre_effects_editor_model::input_routing::native_text_key(
            key,
            event.keystroke.modifiers.control,
            event.keystroke.modifiers.platform,
            event.keystroke.modifiers.alt,
        ) {
            return;
        }
        if self.guarded && self.marked.is_some() && key != "escape" {
            // The platform IME owns marked text. In particular, Enter and file
            // shortcuts must not submit or blur an unfinished composition.
            cx.stop_propagation();
            return;
        }
        if (control && matches!(key, "s" | "o" | "n"))
            || (event.keystroke.modifiers.alt && key == "f4")
        {
            self.submit(window, cx);
            window.blur();
            return;
        }
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
                self.finish_input(window);
            }
            "escape" => {
                self.content = self.original.clone();
                self.selection = 0..0;
                self.marked = None;
                self.finish_input(window);
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
        if self.focus.is_focused(window) {
            cx.set_global(ActiveField(cx.entity().downgrade()));
        }
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
            .h(px(if self.dense { 20.0 } else { 24.0 }))
            .w_full()
            .px_1()
            .overflow_hidden()
            .bg(rgb(0x181818))
            .border_1()
            .border_color(rgb(0x373737))
            .focus(|s| s.border_color(rgb(ui::BLUE)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    if !this.focus.is_focused(window)
                        && this
                            .activate
                            .as_ref()
                            .is_some_and(|activate| !activate(window, cx))
                    {
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                    window.focus(&this.focus);
                    cx.set_global(ActiveField(cx.entity().downgrade()));
                    if this.numeric {
                        this.scrub = this
                            .content
                            .parse::<f64>()
                            .ok()
                            .map(|v| (event.position.x, v));
                        this.scrubbed = false;
                    }
                    this.selection = 0..this.content.len();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down_out(cx.listener(|this, _: &gpui::MouseDownEvent, window, cx| {
                // Commit during capture, before another control changes the bound layer.
                if this.focus.is_focused(window) {
                    if this.guarded && this.marked.is_some() {
                        window.prevent_default();
                        cx.stop_propagation();
                        return;
                    }
                    this.submit(window, cx);
                }
            }))
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
                        // Register even before a gesture starts: a fast drag can deliver
                        // move/up before the next paint after mouse-down.
                        {
                            let moving = input.clone();
                            window.on_mouse_event(
                                move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                                    if !phase.bubble()
                                        || event.pressed_button != Some(MouseButton::Left)
                                    {
                                        return;
                                    }
                                    moving.update(cx, |this, cx| {
                                        if this.scrub.is_some() {
                                            this.scrub_to(
                                                event.position.x,
                                                event.modifiers.shift,
                                                event.modifiers.alt,
                                            );
                                            cx.notify();
                                        }
                                    });
                                },
                            );
                            let ending = input.clone();
                            window.on_mouse_event(
                                move |event: &gpui::MouseUpEvent, phase, window, cx| {
                                    if !phase.bubble() || event.button != MouseButton::Left {
                                        return;
                                    }
                                    ending.update(cx, |this, cx| {
                                        if this.scrub.is_none() {
                                            return;
                                        }
                                        this.scrub_to(
                                            event.position.x,
                                            event.modifiers.shift,
                                            event.modifiers.alt,
                                        );
                                        this.scrub = None;
                                        if this.scrubbed {
                                            this.scrubbed = false;
                                            this.submit(window, cx);
                                        }
                                        cx.notify();
                                    });
                                },
                            );
                        }
                    },
                )
                .size_full(),
            )
    }
}

fn preserve_guarded_selection(
    binding_changed: bool,
    focused: bool,
    content: &str,
    original: &str,
    source: &str,
    marked: bool,
) -> bool {
    binding_changed && focused && !marked && content == original && content == source
}

fn source_input_pending(binding: &str, changed: bool, marked: bool) -> bool {
    !matches!(binding, "command-search")
        && !binding.starts_with("timeline-search:")
        && !binding.starts_with("project-search:")
        && (changed || marked)
}

fn pending_submission(content: &str, original: &str, guarded: bool, marked: bool) -> bool {
    content != original && !(guarded && marked)
}

fn scrub_text(value: f64, integer: bool) -> String {
    if integer {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn guarded_rebind_preserves_fresh_focus_selection_but_never_stale_or_marked_text() {
        assert!(super::preserve_guarded_selection(
            true, true, "70", "70", "70", false
        ));
        assert!(super::preserve_guarded_selection(
            true, true, "Mixed", "Mixed", "Mixed", false
        ));
        for (changed, focused, content, original, source, marked) in [
            (true, true, "80", "70", "70", false),
            (true, true, "70", "70", "80", false),
            (true, true, "70", "70", "70", true),
            (true, false, "70", "70", "70", false),
            (false, true, "70", "70", "70", false),
        ] {
            assert!(!super::preserve_guarded_selection(
                changed, focused, content, original, source, marked
            ));
        }
        // The production rebind decision retains mouse-down's 0..2 selection,
        // so the next insertion replaces "70" rather than producing "7080".
        let mut text = "70".to_string();
        let selected = if super::preserve_guarded_selection(true, true, &text, &text, &text, false)
        {
            0..2
        } else {
            2..2
        };
        text.replace_range(selected, "80");
        assert_eq!(text, "80");
    }
    #[test]
    fn svg_import_pending_input_guard_distinguishes_source_drafts_from_search_queries() {
        for binding in ["layer-position", "gradient-colors", "composition-settings"] {
            assert!(!super::source_input_pending(binding, false, false));
            for (changed, marked) in [(true, false), (false, true), (true, true)] {
                assert!(super::source_input_pending(binding, changed, marked));
                for search in ["command-search", "timeline-search:0:1", "project-search:0"] {
                    assert!(!super::source_input_pending(search, changed, marked));
                }
            }
        }
    }

    #[test]
    fn guarded_pending_text_keeps_mixed_unchanged_and_marked_ime_unsubmitted() {
        for (content, original, marked, pending) in [
            ("Mixed", "Mixed", false, false),
            ("123.45678901234567", "123.45678901234567", false, false),
            ("", "Mixed", false, true),
            ("NaN", "Mixed", false, true),
            ("125", "Mixed", false, true),
            ("125", "Mixed", true, false),
            ("", "Mixed", true, false),
        ] {
            assert_eq!(
                super::pending_submission(content, original, true, marked),
                pending
            );
        }
        // Legacy fields deliberately retain their existing submission policy.
        assert!(super::pending_submission("125", "100", false, true));
    }
    #[test]
    fn gradient_integer_scrubs_round_fractional_pointer_and_alt_deltas() {
        for (value, text) in [(12.0, "12"), (12.25, "12"), (12.75, "13"), (0.4, "0")] {
            assert_eq!(super::scrub_text(value, true), text);
        }
        assert_eq!(super::scrub_text(12.25, false), "12.25");
    }
}
