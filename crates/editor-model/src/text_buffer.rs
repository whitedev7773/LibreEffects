//! Source-preserving text draft and local history, independent of GPUI.
use libre_effects_core::{
    RichText, TextCharacterPatch, TextCharacterStyle, TextSelectionStyle, TextStyleRun,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    text: String,
    rich_text: Option<libre_effects_core::RichText>,
    anchor: usize,
    caret: usize,
}
#[derive(Clone, Debug)]
pub struct Buffer {
    pub text: String,
    pub rich_text: Option<libre_effects_core::RichText>,
    pub anchor: usize,
    pub caret: usize,
    pub marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    generation: u64,
}
impl Buffer {
    pub fn new(text: String) -> Self {
        let end = text.len();
        Self {
            text,
            rich_text: None,
            anchor: end,
            caret: end,
            marked: None,
            undo: vec![],
            redo: vec![],
            generation: 0,
        }
    }
    /// Monotonic revision for rejecting stale draft callbacks, including ABA
    /// selection, composition and history changes. Active edits must use methods;
    /// legacy public fields remain available for initial setup and fixtures.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    fn changed(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("Text draft generation overflow");
    }
    pub fn clear_mark(&mut self) {
        if self.marked.take().is_some() {
            self.changed();
        }
    }
    /// Explicitly return a draft to native spacing as one local undo step.
    /// Clearing an absent payload is an exact no-op, including redo history.
    pub fn reset_positioning(&mut self) -> bool {
        if self
            .rich_text
            .as_ref()
            .is_none_or(|rich| rich.positioning.is_none())
        {
            return false;
        }
        self.save_undo();
        self.rich_text.as_mut().unwrap().reset_positioning();
        self.changed();
        true
    }
    fn effective_rich_text(&self, baseline: &TextCharacterStyle) -> Result<RichText, String> {
        if let Some(rich) = &self.rich_text {
            return Ok(rich.clone());
        }
        let runs = if self.text.is_empty() {
            vec![]
        } else {
            vec![TextStyleRun {
                start: 0,
                end: self.text.len(),
                style: baseline.clone(),
            }]
        };
        RichText::new(&self.text, baseline.clone(), runs)
    }
    pub fn selection_style(
        &self,
        baseline: &TextCharacterStyle,
    ) -> Result<TextSelectionStyle, String> {
        self.effective_rich_text(baseline)?
            .selection_style(&self.text, &self.selection())
    }
    /// Style a nonempty, unmarked selection. Plain drafts promote only after an
    /// effective change; a no-op or failure preserves exact local history.
    pub fn format_selection(
        &mut self,
        baseline: &TextCharacterStyle,
        patch: &TextCharacterPatch,
    ) -> Result<bool, String> {
        if self.marked.is_some() {
            return Err("Finish composing text before formatting characters".into());
        }
        let current = self.effective_rich_text(baseline)?;
        let next = current.format_range(&self.text, &self.selection(), patch)?;
        if current == next {
            return Ok(false);
        }
        self.save_undo();
        self.rich_text = Some(next);
        self.changed();
        Ok(true)
    }
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
    pub fn byte(&self, units: usize) -> usize {
        byte_offset(&self.text, units)
    }
    pub fn utf16(&self, range: Range<usize>) -> Range<usize> {
        self.text[..range.start].encode_utf16().count()
            ..self.text[..range.end].encode_utf16().count()
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            rich_text: self.rich_text.clone(),
            anchor: self.anchor,
            caret: self.caret,
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.rich_text = s.rich_text;
        self.anchor = s.anchor;
        self.caret = s.caret;
        self.marked = None;
    }
    fn save_undo(&mut self) {
        if self.undo.len() == 100 {
            self.undo.remove(0);
        }
        self.undo.push(self.snapshot());
        self.redo.clear();
    }
    pub fn history(&mut self, redo: bool) {
        let marked = self.marked.take().is_some();
        if let Some(s) = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        } {
            let now = self.snapshot();
            if redo {
                self.undo.push(now);
            } else {
                self.redo.push(now);
            }
            self.restore(s);
            self.changed();
        } else if marked {
            self.changed();
        }
    }
    pub fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        mark: bool,
        selected: Option<Range<usize>>,
    ) -> Result<(), String> {
        let range = range
            .map(|r| self.byte(r.start)..self.byte(r.end))
            .unwrap_or_else(|| self.marked.clone().unwrap_or_else(|| self.selection()));
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return Err("Invalid text selection".into());
        }
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || matches!(*c, '\r' | '\n' | '\t'))
            .collect();
        if self.text.len() - (range.end - range.start) + text.len() > 16384 {
            return Err("Text is limited to 16 KiB".into());
        }
        // Validate and prepare both text and character styling before changing
        // buffer history, selection or the marked range.
        let rich_result = self
            .rich_text
            .as_ref()
            .map(|rich| rich.replace_range(&self.text, range.clone(), &text))
            .transpose()?;
        let start = range.start;
        let end = start + text.len();
        let (source, rich_text) = if let Some((source, rich)) = rich_result {
            (source, Some(rich))
        } else {
            let mut source = self.text.clone();
            source.replace_range(range, &text);
            (source, None)
        };
        let marked = if mark && start < end {
            Some(start..end)
        } else {
            None
        };
        let mut anchor = end;
        let mut caret = end;
        if mark {
            if let Some(s) = selected {
                let base = source[..start].encode_utf16().count();
                anchor = byte_offset(&source, base.saturating_add(s.start)).min(end);
                caret = byte_offset(&source, base.saturating_add(s.end)).min(end);
            }
        }
        if self.text == source
            && self.rich_text == rich_text
            && self.anchor == anchor
            && self.caret == caret
            && self.marked == marked
        {
            return Ok(());
        }
        if self.marked.is_none() {
            self.save_undo();
        }
        self.text = source;
        self.rich_text = rich_text;
        self.anchor = anchor;
        self.caret = caret;
        self.marked = marked;
        self.changed();
        Ok(())
    }
    pub fn select(&mut self, at: usize, extend: bool) {
        let before = (self.anchor, self.caret, self.marked.clone());
        self.marked = None;
        self.caret = at.min(self.text.len());
        while !self.text.is_char_boundary(self.caret) {
            self.caret -= 1;
        }
        if !extend {
            self.anchor = self.caret;
        }
        if before != (self.anchor, self.caret, self.marked.clone()) {
            self.changed();
        }
    }
    pub fn all(&mut self) {
        let before = (self.anchor, self.caret, self.marked.clone());
        self.marked = None;
        self.anchor = 0;
        self.caret = self.text.len();
        if before != (self.anchor, self.caret, self.marked.clone()) {
            self.changed();
        }
    }
    pub fn step(&mut self, right: bool, extend: bool) {
        let range = self.selection();
        let at = if !extend && !range.is_empty() {
            if right { range.end } else { range.start }
        } else if right {
            self.text[self.caret..]
                .graphemes(true)
                .next()
                .map_or(self.caret, |g| self.caret + g.len())
        } else {
            self.text[..self.caret]
                .grapheme_indices(true)
                .last()
                .map_or(0, |(i, _)| i)
        };
        self.select(at, extend);
    }
    pub fn delete(&mut self, forward: bool) -> Result<(), String> {
        if self.selection().is_empty() {
            self.step(forward, true);
        }
        self.replace(None, "", false, None)
    }
    pub fn word(&mut self, right: bool, extend: bool) {
        let at = if right {
            self.text
                .unicode_word_indices()
                .map(|(i, _)| i)
                .find(|i| *i > self.caret)
                .unwrap_or(self.text.len())
        } else {
            self.text
                .unicode_word_indices()
                .map(|(i, _)| i)
                .take_while(|i| *i < self.caret)
                .last()
                .unwrap_or(0)
        };
        self.select(at, extend);
    }
    pub fn select_word(&mut self, at: usize) {
        let at = at.min(self.text.len());
        let range = self
            .text
            .split_word_bound_indices()
            .map(|(i, s)| i..i + s.len())
            .find(|r| r.contains(&at))
            .unwrap_or(at..at);
        self.select(range.start, false);
        self.select(range.end, true);
    }
    pub fn select_line(&mut self, at: usize) {
        self.select(at, false);
        let paragraph = libre_effects_core::text_paragraphs::paragraph_at(&self.text, self.caret);
        let range = paragraph.source_range();
        self.select(range.start, false);
        self.select(range.end, true);
    }
    pub fn line_edge(&mut self, end: bool, document: bool, extend: bool) {
        let at = if document {
            if end { self.text.len() } else { 0 }
        } else {
            let paragraph =
                libre_effects_core::text_paragraphs::paragraph_at(&self.text, self.caret);
            if end {
                paragraph.range.end
            } else {
                paragraph.range.start
            }
        };
        self.select(at, extend);
    }
}

fn byte_offset(text: &str, units: usize) -> usize {
    let mut count = 0;
    for (i, c) in text.char_indices() {
        if count + c.len_utf16() > units {
            return i;
        }
        count += c.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{RichText, TextCharacterStyle, TextStyle, TextStyleRun};
    fn rich_buffer() -> Buffer {
        let source = "Aβ\r\nZ";
        let red = TextCharacterStyle::from_style(&TextStyle::default(), 24.0, 0xff0000);
        let mut blue = red.clone();
        blue.fill_color = 0x0000ff;
        blue.font_size = 36.;
        let mut result = Buffer::new(source.into());
        result.rich_text = Some(
            RichText::new(
                source,
                red.clone(),
                vec![
                    TextStyleRun {
                        start: 0,
                        end: 1,
                        style: red,
                    },
                    TextStyleRun {
                        start: 1,
                        end: source.len(),
                        style: blue,
                    },
                ],
            )
            .unwrap(),
        );
        result
    }
    #[test]
    fn native_rich_draft_preserves_source_breaks_and_local_history() {
        let mut b = rich_buffer();
        let original = b.snapshot();
        b.anchor = 3;
        b.caret = 3;
        b.replace(None, "x\rY", false, None).unwrap();
        assert_eq!(b.text, "Aβx\rY\r\nZ");
        assert_eq!(
            b.rich_text.as_ref().unwrap().style_at(3).fill_color,
            0x0000ff
        );
        let applied = b.snapshot();
        b.history(false);
        assert_eq!(b.text, original.text);
        assert_eq!(b.rich_text, original.rich_text);
        b.history(true);
        assert_eq!(b.snapshot(), applied);
    }
    #[test]
    fn marked_replacements_keep_one_undo_and_exact_styles() {
        let mut b = rich_buffer();
        let original = b.snapshot();
        b.anchor = 1;
        b.caret = 3;
        b.replace(None, "あ", true, None).unwrap();
        b.replace(None, "愛", true, None).unwrap();
        b.replace(None, "愛", false, None).unwrap();
        assert_eq!(b.text, "A愛\r\nZ");
        assert!(b.marked.is_none());
        assert_eq!(b.undo.len(), 1);
        b.history(false);
        assert_eq!(b.text, original.text);
        assert_eq!(b.rich_text, original.rich_text);
    }
    #[test]
    fn rejected_rich_edit_preserves_history_selection_and_source() {
        let mut b = rich_buffer();
        let before = b.snapshot();
        assert!(b.replace(Some(4..1), "x", false, None).is_err());
        assert_eq!(b.snapshot(), before);
        assert!(b.undo.is_empty());
        assert!(b.replace(None, &"x".repeat(16384), false, None).is_err());
        assert_eq!(b.snapshot(), before);
        assert!(b.undo.is_empty());
    }
    #[test]
    fn delete_everything_preserves_insertion_style_through_undo() {
        let mut b = rich_buffer();
        b.all();
        b.replace(None, "", false, None).unwrap();
        assert_eq!(b.text, "");
        assert!(b.rich_text.as_ref().unwrap().runs.is_empty());
        b.replace(None, "new", false, None).unwrap();
        assert!(b.rich_text.as_ref().unwrap().validate(&b.text).is_ok());
        b.history(false);
        assert_eq!(b.text, "");
        b.history(false);
        assert_eq!(b.text, "Aβ\r\nZ");
    }

    #[test]
    fn selected_format_lazily_promotes_plain_text_and_keeps_reversed_selection() {
        let base = TextCharacterStyle::from_style(&TextStyle::default(), 24.0, 0xff0000);
        let mut b = Buffer::new("Aβ\r\nZ".into());
        b.select(5, false);
        b.select(1, true);
        let original = b.snapshot();
        let generation = b.generation();
        assert_eq!(b.selection_style(&base).unwrap().font_size, Some(24.0));
        assert!(
            !b.format_selection(&base, &TextCharacterPatch::FontSize(24.0))
                .unwrap()
        );
        assert_eq!(b.snapshot(), original);
        assert_eq!(b.generation(), generation);
        assert!(b.undo.is_empty());
        assert!(
            b.format_selection(&base, &TextCharacterPatch::FontSize(36.0))
                .unwrap()
        );
        assert_eq!((b.anchor, b.caret), (5, 1));
        assert_eq!(b.text, original.text);
        let rich = b.rich_text.as_ref().unwrap();
        assert_eq!(rich.default_style, base);
        assert_eq!(rich.runs.len(), 3);
        assert_eq!(rich.style_at(0), &base);
        assert_eq!(rich.style_at(5), &base);
        assert_eq!(rich.style_at(1).font_size, 36.0);
        let changed = b.snapshot();
        b.history(false);
        assert_eq!(b.snapshot(), original);
        assert!(b.rich_text.is_none());
        let generation = b.generation();
        assert!(
            !b.format_selection(&base, &TextCharacterPatch::FontSize(24.0))
                .unwrap()
        );
        assert_eq!(b.redo.len(), 1);
        assert!(
            b.format_selection(&base, &TextCharacterPatch::FontSize(f64::NAN))
                .is_err()
        );
        assert_eq!(b.snapshot(), original);
        assert_eq!(b.generation(), generation);
        assert_eq!(b.redo.len(), 1);
        b.history(true);
        assert_eq!(b.snapshot(), changed);
    }

    #[test]
    fn selected_format_and_typing_share_one_local_history() {
        let mut b = rich_buffer();
        let base = b.rich_text.as_ref().unwrap().default_style.clone();
        b.select(1, false);
        b.select(3, true);
        let original = b.snapshot();
        b.format_selection(&base, &TextCharacterPatch::FillEnabled(false))
            .unwrap();
        let styled = b.snapshot();
        b.select(3, false);
        b.replace(None, "한", false, None).unwrap();
        let typed = b.snapshot();
        assert!(!b.rich_text.as_ref().unwrap().style_at(3).fill_enabled);
        b.history(false);
        assert_eq!(b.text, styled.text);
        assert_eq!(b.rich_text, styled.rich_text);
        b.history(false);
        assert_eq!(b.snapshot(), original);
        b.history(true);
        b.history(true);
        assert_eq!(b.snapshot(), typed);
        assert!(b.undo.len() == 2 && b.redo.is_empty());
    }

    #[test]
    fn selected_format_rejects_empty_marked_and_partial_grapheme_selections_atomically() {
        let base = TextCharacterStyle::from_style(&TextStyle::default(), 24.0, 0);
        for (text, anchor, caret, marked) in [
            ("e\u{301}", 0, 1, None),
            ("👩‍💻", 4, 11, None),
            ("\r\n", 0, 1, None),
            ("AB", 1, 1, None),
            ("AB", 0, 2, Some(0..2)),
            ("AB", 0, 3, None),
        ] {
            let mut b = Buffer::new(text.into());
            b.anchor = anchor;
            b.caret = caret;
            b.marked = marked.clone();
            let snapshot = b.snapshot();
            assert!(
                b.format_selection(&base, &TextCharacterPatch::FillColor(0))
                    .is_err(),
                "{text:?}"
            );
            assert_eq!(b.snapshot(), snapshot);
            assert_eq!(b.marked, marked);
            assert_eq!(b.generation(), 0);
            assert!(b.undo.is_empty() && b.redo.is_empty());
        }
    }

    #[test]
    fn draft_generation_detects_selection_style_text_mark_and_history_aba() {
        let base = TextCharacterStyle::from_style(&TextStyle::default(), 24.0, 0);
        let mut b = Buffer::new("AB".into());
        b.all();
        let selected = (b.anchor, b.caret);
        let generation = b.generation();
        b.select(0, false);
        b.all();
        assert_eq!((b.anchor, b.caret), selected);
        assert!(b.generation() > generation);
        let generation = b.generation();
        b.format_selection(&base, &TextCharacterPatch::FillEnabled(false))
            .unwrap();
        b.history(false);
        assert!(b.rich_text.is_none());
        assert!(b.generation() > generation);
        let generation = b.generation();
        b.history(true);
        assert!(b.generation() > generation);
        let generation = b.generation();
        b.replace(None, "XY", true, Some(0..2)).unwrap();
        assert!(b.generation() > generation);
        let generation = b.generation();
        b.clear_mark();
        assert!(b.generation() > generation);
        let generation = b.generation();
        b.clear_mark();
        assert_eq!(b.generation(), generation);
        b.history(false);
        b.history(false);
        let generation = b.generation();
        b.history(false);
        assert_eq!(b.generation(), generation);
        assert_eq!(b.text, "AB");
    }

    #[test]
    fn unchanged_replacement_preserves_redo_and_generation() {
        let mut b = Buffer::new("AB".into());
        b.replace(None, "C", false, None).unwrap();
        b.history(false);
        let before = b.snapshot();
        let generation = b.generation();
        b.replace(None, "", false, None).unwrap();
        assert_eq!(b.snapshot(), before);
        assert_eq!(b.generation(), generation);
        assert_eq!(b.redo.len(), 1);
        b.history(true);
        assert_eq!(b.text, "ABC");
    }
}
