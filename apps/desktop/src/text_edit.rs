//! Transactional point-text editing. The live document changes only at commit.
use libre_effects_core::{Affine, Command, Content, Editor, Frame, LayerId, Project, Property};
#[cfg(test)]
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use unicode_segmentation::UnicodeSegmentation;
#[path = "text_layout.rs"]
pub(crate) mod layout;

pub(crate) use libre_effects_editor_model::text_buffer::Buffer;

static NEXT_TEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// A picker or field owns one exact draft selection, including its history epoch.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectionTarget {
    serial: u64,
    generation: u64,
    range: std::ops::Range<usize>,
    id: LayerId,
    frame: Frame,
}
impl SelectionTarget {
    pub fn binding(&self, field: &str) -> String {
        format!("text-selection:{}:{}:{field}", self.serial, self.generation)
    }
    pub fn current(&self, state: &crate::editor::EditorState) -> bool {
        state.text_session.as_ref().is_some_and(|session| {
            session.valid(state.editor.project(), state.document_revision, state.frame)
                && session.serial == self.serial
                && session.buffer.generation() == self.generation
                && session.buffer.selection() == self.range
                && session.buffer.marked.is_none()
                && session.id == self.id
                && session.frame == self.frame
        })
    }
}

#[derive(Clone)]
pub(crate) struct Session {
    pub buffer: Buffer,
    pub id: LayerId,
    pub frame: Frame,
    pub world: Affine,
    /// Frozen playhead geometry. Existing source commits never persist typography.
    pub font_size: f64,
    pub width: f64,
    pub height: f64,
    pub style: libre_effects_core::TextStyle,
    pub preferred_x: Option<f64>,
    pub caret_hint: Option<(usize, [f64; 2])>,
    base: Project,
    seed: Vec<Command>,
    revision: u64,
    serial: u64,
    pub baseline_style: libre_effects_core::TextCharacterStyle,
    format_error: Option<String>,
}
impl Session {
    pub fn authored_spacing_reset(&self) -> bool {
        self.base
            .composition()
            .layer(self.id)
            .is_some_and(|layer| layer.has_authored_text_positions())
            && self
                .buffer
                .rich_text
                .as_ref()
                .is_none_or(|rich| rich.positioning.is_none())
    }

    pub fn identity(&self) -> u64 {
        self.serial
    }
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
        // Use actual visual-line origins so paragraph spacing cannot trap Up/Down
        // on the current line. Retain the existing point-text navigation path.
        if self.style.paragraph || self.buffer.rich_text.is_some() {
            p[1] = layout
                .carets
                .iter()
                .map(|(_, q)| q[1])
                .filter(|y| {
                    if down {
                        *y > p[1] + 0.001
                    } else {
                        *y < p[1] - 0.001
                    }
                })
                .min_by(|a, b| (a - p[1]).abs().total_cmp(&(b - p[1]).abs()))
                .unwrap_or(p[1]);
            p[1] += if self.buffer.rich_text.is_some() {
                layout.height_at(p) / 1.2 * 0.5
            } else {
                self.font_size * 0.5
            };
        } else {
            p[1] += self.font_size * (0.5 + self.style.leading * if down { 1.0 } else { -1.0 });
        }
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
        if layer.has_enabled_expression(libre_effects_core::ExpressionTarget::SourceText) {
            return Err(
                "Disable the Source Text expression before editing its visible text".into(),
            );
        }
        if comp.layers().iter().any(|layer| layer.is_three_d()) {
            return Err("Spatial composition text geometry is edited through scripting".into());
        }
        let Some(text) = layer.source_text_at(frame) else {
            return Err("Select a text layer".into());
        };
        let world = comp
            .world_transform(id, frame)
            .filter(|m| m.inverse().is_some())
            .ok_or("Text transform cannot be edited at zero scale")?;
        let typography = layer.text_typography_at(frame).unwrap();
        let mut style = layer.text_style();
        typography.apply_to_style(&mut style);
        let mut buffer = Buffer::new(text.into());
        buffer.rich_text = layer.rich_text().cloned();
        let session = Self {
            buffer,
            id,
            frame,
            world,
            font_size: typography.font_size,
            width: layer.width(),
            height: layer.height(),
            style,
            preferred_x: None,
            caret_hint: None,
            base: project.clone(),
            seed,
            revision,
            serial: NEXT_TEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            baseline_style: layer.base_character_style().unwrap(),
            format_error: layer.rich_text_eligibility().err(),
        };
        if let Some(error) = &layout::Layout::new(&session).error {
            return Err(error.clone());
        }
        Ok(session)
    }
    pub fn valid(&self, project: &Project, revision: u64, frame: Frame) -> bool {
        self.revision == revision && self.frame == frame && &self.base == project
    }
    pub fn selection_target(&self) -> Result<SelectionTarget, String> {
        if self.style.paragraph {
            return Err("Character selections currently require point text".into());
        }
        if let Some(error) = &self.format_error {
            return Err(error.clone());
        }
        if self.buffer.marked.is_some() {
            return Err("Finish composing text before formatting a selection".into());
        }
        self.buffer.selection_style(&self.baseline_style)?;
        Ok(SelectionTarget {
            serial: self.serial,
            generation: self.buffer.generation(),
            range: self.buffer.selection(),
            id: self.id,
            frame: self.frame,
        })
    }
    pub fn format_selection(
        &mut self,
        patch: &libre_effects_core::TextCharacterPatch,
    ) -> Result<bool, String> {
        self.selection_target()?;
        let changed = self.buffer.format_selection(&self.baseline_style, patch)?;
        if changed {
            self.preferred_x = None;
            self.caret_hint = None;
        }
        Ok(changed)
    }
    pub fn changed(&self) -> bool {
        if !self.seed.is_empty() {
            !self.buffer.text.is_empty()
        } else {
            self.base.composition().layer(self.id).is_some_and(|l| {
                l.source_text_at(self.frame)
                    .is_some_and(|text| text != self.buffer.text)
                    || l.rich_text() != self.buffer.rich_text.as_ref()
                    || (self.style.paragraph
                        && (l.width() != self.width || l.height() != self.height))
            })
        }
    }
    pub fn command(&self) -> Command {
        let mut commands = self.seed.clone();
        if self.seed.is_empty() {
            // Edit from the immutable frame sample. Repeated previews operate on
            // fresh clones, so their interned drafts cannot grow the live pool.
            let source_changed = self
                .base
                .composition()
                .layer(self.id)
                .and_then(|layer| layer.source_text_at(self.frame))
                != Some(self.buffer.text.as_str());
            if let Some(rich_text) = &self.buffer.rich_text {
                if source_changed {
                    commands.push(Command::SetStyledText {
                        id: self.id,
                        text: self.buffer.text.clone(),
                        rich_text: rich_text.clone(),
                    });
                } else {
                    commands.push(Command::SetRichText {
                        id: self.id,
                        rich_text: Some(rich_text.clone()),
                    });
                }
            } else if source_changed {
                commands.push(Command::EditSourceText {
                    id: self.id,
                    frame: self.frame,
                    text: self.buffer.text.clone(),
                });
            }
        } else {
            commands.push(Command::SetContent {
                id: self.id,
                content: Content::Text {
                    text: self.buffer.text.clone(),
                    font_size: self.font_size,
                },
            });
        }
        if !self.seed.is_empty() && self.buffer.rich_text.is_some() {
            commands.push(Command::SetRichText {
                id: self.id,
                rich_text: self.buffer.rich_text.clone(),
            });
        }
        if self.style.paragraph
            && (!self.seed.is_empty()
                || self.base.composition().layer(self.id).is_some_and(|layer| {
                    layer.width() != self.width || layer.height() != self.height
                }))
        {
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
#[path = "source_text_session_tests.rs"]
mod source_text_tests;

#[cfg(test)]
#[path = "selected_text_session_tests.rs"]
mod selected_text_tests;

#[cfg(test)]
#[path = "text_paint_session_tests.rs"]
mod paint_tests;

#[cfg(test)]
#[path = "text_typography_session_tests.rs"]
mod typography_tests;

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
    fn paragraph_vertical_navigation_crosses_large_spacing_and_empty_paragraphs() {
        let mut s = Session::new_box(&Project::default(), 0, 0, [0.0, 0.0, 500.0, 5000.0]).unwrap();
        s.style.paragraph_space_before = 500.0;
        s.style.paragraph_space_after = 800.0;
        s.style.paragraph_left_indent = 30.0;
        s.style.paragraph_first_line_indent = -10.0;
        s.buffer.replace(None, "AB\n\nCD", false, None).unwrap();
        s.buffer.select(0, false);
        s.vertical(true, false);
        assert_eq!(s.buffer.caret, 3);
        s.vertical(true, false);
        assert_eq!(s.buffer.caret, 4);
        s.vertical(false, false);
        assert_eq!(s.buffer.caret, 3);
        s.vertical(false, false);
        assert_eq!(s.buffer.caret, 0);
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
    fn editing_existing_mixed_breaks_preserves_bytes_and_utf16_positions() {
        use libre_effects_core::text_paragraphs::paragraphs;
        let text = "English\r日本語👩‍💻\r\n한국어\n\r끝\r\n";
        let mut buffer = Buffer::new(text.into());
        for paragraph in paragraphs(text) {
            buffer.select(paragraph.range.start, false);
            buffer.line_edge(true, false, false);
            assert_eq!(buffer.caret, paragraph.range.end);
            buffer.line_edge(false, false, false);
            assert_eq!(buffer.caret, paragraph.range.start);
            buffer.select_line(paragraph.range.start);
            assert_eq!(buffer.selection(), paragraph.source_range());
            let utf16 = buffer.utf16(paragraph.source_range());
            assert_eq!(buffer.byte(utf16.start), paragraph.range.start);
            assert_eq!(buffer.byte(utf16.end), paragraph.terminator.end);
        }
        let crlf = text.find("\r\n").unwrap();
        let utf16 = buffer.utf16(crlf..crlf + 2);
        assert_eq!(utf16.end - utf16.start, 2);
        assert_eq!(buffer.byte(utf16.start + 1), crlf + 1);
        buffer.select(crlf + 1, false);
        buffer.line_edge(true, false, false);
        assert_eq!(buffer.caret, crlf);
        buffer.select(crlf + 2, false);
        buffer.step(false, true);
        assert_eq!(buffer.selection(), crlf..crlf + 2);
        buffer.delete(false).unwrap();
        assert_eq!(
            buffer.text,
            format!("{}{}", &text[..crlf], &text[crlf + 2..])
        );
        buffer.history(false);
        assert_eq!(buffer.text, text);
        // Native input replaces only the selected original byte range; all
        // authored break styles and the unaffected source keep their offsets.
        buffer.replace(Some(0..7), "Changed", false, None).unwrap();
        assert_eq!(&buffer.text[7..], &text[7..]);
        buffer.history(false);
        assert_eq!(buffer.text, text);
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
        assert_eq!(b.caret, 7);
        b.step(false, true);
        assert_eq!(&b.text[b.selection()], "\r\n");
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

#[cfg(test)]
#[path = "point_text_tests.rs"]
mod point_text_tests;
