//! Transactional point-text editing. The live document changes only at commit.
use libre_effects_core::{Affine, Command, Content, Editor, Frame, LayerId, Project, Property};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
#[path = "text_layout.rs"]
pub(crate) mod layout;

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    text: String,
    anchor: usize,
    caret: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Buffer {
    pub text: String,
    pub anchor: usize,
    pub caret: usize,
    pub marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl Buffer {
    pub fn new(text: String) -> Self {
        let end = text.len();
        Self {
            text,
            anchor: end,
            caret: end,
            marked: None,
            undo: vec![],
            redo: vec![],
        }
    }
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
    pub fn byte(&self, units: usize) -> usize {
        let mut count = 0;
        for (i, c) in self.text.char_indices() {
            if count + c.len_utf16() > units {
                return i;
            }
            count += c.len_utf16();
        }
        self.text.len()
    }
    pub fn utf16(&self, range: Range<usize>) -> Range<usize> {
        self.text[..range.start].encode_utf16().count()
            ..self.text[..range.end].encode_utf16().count()
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            caret: self.caret,
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.anchor = s.anchor;
        self.caret = s.caret;
        self.marked = None;
    }
    pub fn history(&mut self, redo: bool) {
        self.marked = None;
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
        if range.start > range.end {
            return Err("Invalid text selection".into());
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect();
        if self.text.len() - (range.end - range.start) + text.len() > 16384 {
            return Err("Text is limited to 16 KiB".into());
        }
        if self.marked.is_none() {
            if self.undo.len() == 100 {
                self.undo.remove(0);
            }
            self.undo.push(self.snapshot());
            self.redo.clear();
        }
        let start = range.start;
        let end = start + text.len();
        self.text.replace_range(range, &text);
        self.anchor = end;
        self.caret = end;
        self.marked = if mark && start < end {
            Some(start..end)
        } else {
            None
        };
        if mark {
            if let Some(s) = selected {
                let base = self.text[..start].encode_utf16().count();
                self.anchor = self.byte(base + s.start).min(end);
                self.caret = self.byte(base + s.end).min(end);
            }
        }
        Ok(())
    }
    pub fn select(&mut self, at: usize, extend: bool) {
        self.marked = None;
        self.caret = at.min(self.text.len());
        while !self.text.is_char_boundary(self.caret) {
            self.caret -= 1;
        }
        if !extend {
            self.anchor = self.caret;
        }
    }
    pub fn all(&mut self) {
        self.marked = None;
        self.anchor = 0;
        self.caret = self.text.len();
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
        self.line_edge(false, false, false);
        self.line_edge(true, false, true);
        if self.caret < self.text.len() {
            self.caret += 1;
        }
    }
    pub fn line_edge(&mut self, end: bool, document: bool, extend: bool) {
        let at = if document {
            if end { self.text.len() } else { 0 }
        } else if end {
            self.text[self.caret..]
                .find('\n')
                .map_or(self.text.len(), |i| self.caret + i)
        } else {
            self.text[..self.caret].rfind('\n').map_or(0, |i| i + 1)
        };
        self.select(at, extend);
    }
}
#[derive(Clone)]
pub(crate) struct Session {
    pub buffer: Buffer,
    pub id: LayerId,
    pub frame: Frame,
    pub world: Affine,
    pub font_size: f64,
    pub width: f64,
    pub height: f64,
    pub style: libre_effects_core::TextStyle,
    pub preferred_x: Option<f64>,
    pub caret_hint: Option<(usize, [f64; 2])>,
    base: Project,
    seed: Vec<Command>,
    revision: u64,
}
impl Session {
    pub fn line_edge(&mut self, end: bool, document: bool, extend: bool) {
        if document || !self.style.paragraph {
            self.buffer.line_edge(end, document, extend);
            self.caret_hint = None;
            return;
        }
        let layout = layout::Layout::new(self);
        let y = self.caret_position(&layout)[1];
        let candidates = layout
            .carets
            .iter()
            .filter(|(_, p)| (p[1] - y).abs() < 0.001);
        let target = if end {
            candidates.max_by_key(|(i, _)| *i)
        } else {
            candidates.min_by_key(|(i, _)| *i)
        };
        if let Some((at, p)) = target {
            self.buffer.select(*at, extend);
            self.caret_hint = Some((*at, *p));
        }
    }
    pub fn caret_position(&self, layout: &layout::Layout) -> [f64; 2] {
        let at = self.buffer.caret;
        if let Some((index, p)) = self.caret_hint.filter(|(i, _)| *i == at) {
            return layout
                .carets
                .iter()
                .filter(|(i, _)| *i == index)
                .min_by(|(_, a), (_, b)| {
                    let distance =
                        |q: &[f64; 2]| (q[0] - p[0]).abs() + (q[1] - p[1]).abs() * 10000.0;
                    distance(a).total_cmp(&distance(b))
                })
                .map_or_else(|| layout.caret(at), |(_, q)| *q);
        }
        layout.caret(at)
    }
    pub fn vertical(&mut self, down: bool, extend: bool) {
        let layout = layout::Layout::new(self);
        let mut p = self.caret_position(&layout);
        p[0] = *self.preferred_x.get_or_insert(p[0]);
        p[1] += self.font_size * (0.5 + self.style.leading * if down { 1.0 } else { -1.0 });
        let (at, point) = layout.hit_caret(p);
        self.buffer.select(at, extend);
        self.caret_hint = Some((at, point));
    }
    pub fn new_box(
        project: &Project,
        revision: u64,
        frame: Frame,
        rect: [f64; 4],
    ) -> Result<Self, String> {
        if !rect[2..]
            .iter()
            .all(|v| v.is_finite() && (1.0..=16384.0).contains(v))
        {
            return Err("Paragraph box dimensions must be 1–16384 pixels".into());
        }
        let mut session = Self::new(project, revision, frame, None, [rect[0], rect[1]])?;
        session.width = rect[2];
        session.height = rect[3];
        session.style.paragraph = true;
        if let Some(Command::AddContent { width, height, .. }) = session.seed.first_mut() {
            *width = rect[2];
            *height = rect[3];
        }
        session.seed.push(Command::SetTextStyle {
            id: session.id,
            style: session.style.clone(),
        });
        Ok(session)
    }
    pub fn new(
        project: &Project,
        revision: u64,
        frame: Frame,
        id: Option<LayerId>,
        position: [f64; 2],
    ) -> Result<Self, String> {
        let mut temporary = Editor::default();
        temporary.replace_project(project.clone())?;
        let mut seed = vec![];
        let id = if let Some(id) = id {
            id
        } else {
            let add = Command::AddContent {
                content: Content::Text {
                    text: String::new(),
                    font_size: 72.0,
                },
                width: 640.0,
                height: 120.0,
                name: "Text".into(),
            };
            temporary.execute(add.clone())?;
            seed.push(add);
            let id = temporary.selected().unwrap();
            for command in [
                Command::SetValue {
                    id,
                    property: Property::AnchorX,
                    frame,
                    value: 0.0,
                },
                Command::SetValue {
                    id,
                    property: Property::AnchorY,
                    frame,
                    value: 0.0,
                },
                Command::SetPosition {
                    id,
                    frame,
                    x: position[0],
                    y: position[1],
                },
            ] {
                temporary.execute(command.clone())?;
                seed.push(command);
            }
            id
        };
        let comp = temporary.project().composition();
        let layer = comp.layer(id).ok_or("Missing text layer")?;
        if layer.locked() {
            return Err("Unlock the text layer before editing".into());
        }
        let Content::Text { text, font_size } = layer.content() else {
            return Err("Select a text layer".into());
        };
        let world = comp
            .world_transform(id, frame)
            .filter(|m| m.inverse().is_some())
            .ok_or("Text transform cannot be edited at zero scale")?;
        Ok(Self {
            buffer: Buffer::new(text.clone()),
            id,
            frame,
            world,
            font_size: *font_size,
            width: layer.width(),
            height: layer.height(),
            style: layer.text_style(),
            preferred_x: None,
            caret_hint: None,
            base: project.clone(),
            seed,
            revision,
        })
    }
    pub fn valid(&self, project: &Project, revision: u64, frame: Frame) -> bool {
        self.revision == revision && self.frame == frame && &self.base == project
    }
    pub fn changed(&self) -> bool {
        if !self.seed.is_empty() {
            !self.buffer.text.is_empty()
        } else {
            self.base.composition().layer(self.id).is_some_and(|l| {
                matches!(l.content(),Content::Text{text,..} if text!=&self.buffer.text)
                    || (self.style.paragraph
                        && (l.width() != self.width || l.height() != self.height))
            })
        }
    }
    pub fn command(&self) -> Command {
        let mut commands = self.seed.clone();
        commands.push(Command::SetContent {
            id: self.id,
            content: Content::Text {
                text: self.buffer.text.clone(),
                font_size: self.font_size,
            },
        });
        if self.style.paragraph {
            commands.push(Command::SetTextBox {
                id: self.id,
                width: self.width,
                height: self.height,
            });
        }
        Command::Batch(commands)
    }
    pub fn project(&self) -> Result<Project, String> {
        let mut e = Editor::default();
        e.replace_project(self.base.clone())?;
        e.execute(self.command())?;
        Ok(e.project().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paragraph_draft_resize_and_visual_line_navigation_are_one_transaction() {
        let mut e = Editor::default();
        let base = e.project().clone();
        let mut s = Session::new_box(&base, 0, 0, [20.0, 30.0, 210.0, 160.0]).unwrap();
        assert!(!s.changed());
        s.buffer
            .replace(None, "one two three four five six", false, None)
            .unwrap();
        let layout = layout::Layout::new(&s);
        let lines = crate::text_flow::lines(&s.buffer.text, s.font_size, s.width, &s.style);
        assert!(lines.len() > 2);
        let start = lines[1].range.start;
        let point = layout
            .carets
            .iter()
            .find(|(i, p)| *i == start && p[1] > 0.0)
            .unwrap()
            .1;
        s.buffer.select(start, false);
        s.caret_hint = Some((start, point));
        assert_eq!(s.caret_position(&layout), point);
        s.line_edge(true, false, false);
        assert_eq!(s.buffer.caret, lines[1].range.end);
        assert_eq!(s.caret_position(&layout)[1], point[1]);
        s.line_edge(false, false, false);
        assert_eq!(s.buffer.caret, start);
        s.line_edge(false, true, false);
        assert_eq!(s.buffer.caret, 0);
        e.execute(s.command()).unwrap();
        let committed = e.project().clone();
        assert_eq!(committed, s.project().unwrap());
        e.undo();
        assert_eq!(e.project(), &base);
        e.redo();
        assert_eq!(e.project(), &committed);
        let mut edit = Session::new(e.project(), 1, 0, Some(s.id), [0.0; 2]).unwrap();
        edit.width = 420.0;
        edit.height = 240.0;
        assert!(edit.changed());
        assert!(
            crate::text_flow::lines(&edit.buffer.text, edit.font_size, edit.width, &edit.style)
                .len()
                < lines.len()
        );
        e.execute(edit.command()).unwrap();
        let resized = e.project().clone();
        assert_eq!(resized.composition().layer(s.id).unwrap().width(), 420.0);
        e.undo();
        assert_eq!(e.project(), &committed);
        e.redo();
        assert_eq!(e.project(), &resized);
        assert_eq!(
            Project::from_json(&resized.to_json().unwrap()).unwrap(),
            resized
        );
    }
    #[test]
    fn vertical_motion_remembers_the_original_column_across_short_lines() {
        let mut s = Session::new(&Project::default(), 0, 0, None, [0.0; 2]).unwrap();
        s.buffer
            .replace(None, "WWWWWW\nI\nWWWWWW", false, None)
            .unwrap();
        s.buffer.select(6, false);
        s.vertical(true, false);
        assert_eq!(s.buffer.caret, 8);
        s.vertical(true, false);
        assert_eq!(s.buffer.caret, 15);
        s.vertical(false, false);
        assert_eq!(s.buffer.caret, 8);
        s.vertical(false, true);
        assert_eq!(s.buffer.selection(), 6..8);
    }
    #[test]
    fn word_selection_and_grapheme_deletion_keep_unicode_intact() {
        let mut b = Buffer::new("one 한글 three".into());
        b.word(false, true);
        assert_eq!(&b.text[b.selection()], "three");
        b.replace(None, "끝", false, None).unwrap();
        assert_eq!(b.text, "one 한글 끝");
        b.line_edge(false, true, false);
        b.word(true, false);
        assert_eq!(b.caret, 4);
        b.word(true, true);
        assert_eq!(&b.text[b.selection()], "한글 ");
        b.all();
        b.replace(None, "👩‍💻🇰🇷e\u{301}", false, None).unwrap();
        for expected in ["👩‍💻🇰🇷", "👩‍💻", ""] {
            b.delete(false).unwrap();
            assert_eq!(b.text, expected);
        }
        b.history(false);
        assert_eq!(b.text, "👩‍💻");
        b.replace(Some(0..0), "한", true, Some(50..80)).unwrap();
        assert_eq!(b.caret, "한".len());
        assert_eq!(b.anchor, b.caret);
        b = Buffer::new("first 한글\nsecond line".into());
        b.select_word(7);
        assert_eq!(&b.text[b.selection()], "한글");
        b.select_line(7);
        assert_eq!(&b.text[b.selection()], "first 한글\n");
        b.select_line(b.text.len());
        assert_eq!(&b.text[b.selection()], "second line");
    }

    #[test]
    fn existing_text_preserves_animation_and_style_through_render_and_history() {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Text QA".into(),
            width: 480,
            height: 240,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        let mut first = Session::new(e.project(), 0, 0, None, [30.0, 30.0]).unwrap();
        first.buffer.replace(None, "Before", false, None).unwrap();
        e.execute(first.command()).unwrap();
        let id = first.id;
        let style = libre_effects_core::TextStyle {
            weight: 700,
            tracking: 20.0,
            ..Default::default()
        };
        e.execute(Command::SetTextStyle {
            id,
            style: style.clone(),
        })
        .unwrap();
        e.execute(Command::ToggleAnimation {
            id,
            property: Property::Rotation,
            frame: 0,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id,
            property: Property::Rotation,
            frame: 30,
            value: 12.0,
        })
        .unwrap();
        let before = e.project().clone();
        let mut session = Session::new(&before, 3, 15, Some(id), [0.0; 2]).unwrap();
        session.buffer.all();
        session
            .buffer
            .replace(None, "한글\nTitle", false, None)
            .unwrap();
        let draft = session.project().unwrap();
        assert_eq!(e.project(), &before);
        let renderer = crate::rendering::Renderer::new();
        let preview = renderer.render_preview(&draft, 15, 480).unwrap();
        assert!(preview.pixels().any(|p| p[3] > 0));
        assert_ne!(renderer.render(&before, 15, 480).unwrap(), preview);
        e.execute(session.command()).unwrap();
        let layer = e.project().composition().layer(id).unwrap();
        assert_eq!(layer.text_style(), style);
        assert_eq!(
            layer.property(Property::Rotation),
            before
                .composition()
                .layer(id)
                .unwrap()
                .property(Property::Rotation)
        );
        let saved = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(renderer.render(&saved, 15, 480).unwrap(), preview);
        let mut png = std::io::Cursor::new(Vec::new());
        preview.write_to(&mut png, image::ImageFormat::Png).unwrap();
        assert_eq!(
            image::load_from_memory(png.get_ref()).unwrap().to_rgba8(),
            preview
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &saved);
        e.execute(Command::SetValue {
            id,
            property: Property::ScaleX,
            frame: 15,
            value: 0.0,
        })
        .unwrap();
        assert!(Session::new(e.project(), 3, 15, Some(id), [0.0; 2]).is_err());
    }
    #[test]
    fn unicode_selection_ime_history_and_limits_are_atomic() {
        let mut b = Buffer::new("A😀e\u{301}\n한글".into());
        assert_eq!(b.byte(2), 1);
        assert_eq!(b.byte(3), 5);
        b.select(8, false);
        b.delete(false).unwrap();
        assert_eq!(b.text, "A😀\n한글");
        b.history(false);
        assert_eq!(b.text, "A😀e\u{301}\n한글");
        b.all();
        b.replace(None, "ㅎ", true, None).unwrap();
        b.replace(None, "하", true, None).unwrap();
        b.replace(None, "한", false, None).unwrap();
        assert_eq!(b.text, "한");
        b.history(false);
        assert_eq!(b.text, "A😀e\u{301}\n한글");
        b.history(true);
        assert_eq!(b.text, "한");
        let old = b.text.clone();
        assert!(b.replace(None, &"x".repeat(16385), false, None).is_err());
        assert_eq!(b.text, old);
        b.all();
        b.replace(None, "first\r\nsecond", false, None).unwrap();
        b.line_edge(false, false, false);
        assert_eq!(b.caret, 6);
        b.step(false, true);
        assert_eq!(&b.text[b.selection()], "\n");
    }
    #[test]
    fn session_preview_commit_cancel_and_stale_document_are_independent() {
        let mut e = Editor::default();
        let base = e.project().clone();
        let mut s = Session::new(&base, 4, 0, None, [100.0, 200.0]).unwrap();
        assert!(!s.changed());
        s.buffer.replace(None, "한글\nTitle", false, None).unwrap();
        assert!(s.changed());
        let draft = s.project().unwrap();
        assert_eq!(e.project(), &base);
        assert!(s.valid(&base, 4, 0));
        assert!(!s.valid(&base, 5, 0));
        assert!(!s.valid(&base, 4, 1));
        e.execute(s.command()).unwrap();
        assert_eq!(e.project(), &draft);
        assert_eq!(s.world.point([0.0, 0.0]), [100.0, 200.0]);
        let saved = Project::from_json(&draft.to_json().unwrap()).unwrap();
        assert_eq!(saved, draft);
        e.undo();
        assert_eq!(e.project(), &base);
        e.redo();
        assert_eq!(e.project(), &draft);
        let id = s.id;
        e.execute(Command::ToggleLocked(id)).unwrap();
        assert!(Session::new(e.project(), 4, 0, Some(id), [0.0; 2]).is_err());
    }
}
