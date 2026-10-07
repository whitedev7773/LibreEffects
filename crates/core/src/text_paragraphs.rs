//! Source-preserving hard paragraphs for After Effects and native text.
//!
//! CR, LF and CRLF are paragraph breaks. CRLF is one break; empty source,
//! consecutive breaks and a final break all retain their empty paragraphs.
//! Offsets always address the original UTF-8 bytes, never a normalized copy.

use std::{iter::FusedIterator, ops::Range};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paragraph<'a> {
    /// Paragraph content, excluding its hard break.
    pub text: &'a str,
    /// Byte range of `text` in the original source.
    pub range: Range<usize>,
    /// Original CR, LF or CRLF bytes, or an empty range at the source end.
    pub terminator: Range<usize>,
}

impl Paragraph<'_> {
    /// Original content and its complete terminator, for source-unit selectors.
    pub fn source_range(&self) -> Range<usize> {
        self.range.start..self.terminator.end
    }
}

#[derive(Clone, Debug)]
pub struct Paragraphs<'a> {
    source: &'a str,
    next_start: Option<usize>,
}

/// Iterate without allocating or changing source text. At most `text.len() + 1`
/// paragraphs are produced, and every source byte is scanned at most once.
pub fn paragraphs(text: &str) -> Paragraphs<'_> {
    Paragraphs {
        source: text,
        next_start: Some(0),
    }
}

impl<'a> Iterator for Paragraphs<'a> {
    type Item = Paragraph<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.next_start?;
        let bytes = self.source.as_bytes();
        let end = bytes[start..]
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
            .map_or(bytes.len(), |offset| start + offset);
        let terminator_end = if end == bytes.len() {
            self.next_start = None;
            end
        } else {
            let end = end
                + if bytes[end] == b'\r' && bytes.get(end + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
            self.next_start = Some(end);
            end
        };
        Some(Paragraph {
            text: &self.source[start..end],
            range: start..end,
            terminator: end..terminator_end,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.next_start.map_or((0, Some(0)), |start| {
            (1, Some(self.source.len() - start + 1))
        })
    }
}

impl FusedIterator for Paragraphs<'_> {}

/// Find the hard paragraph containing an original byte offset. A position
/// inside a CRLF pair belongs to the preceding paragraph; a position after the
/// complete break belongs to the next one. Offsets past the source are clamped.
pub fn paragraph_at(text: &str, at: usize) -> Paragraph<'_> {
    let at = at.min(text.len());
    paragraphs(text)
        .find(|paragraph| at < paragraph.terminator.end || paragraph.terminator.is_empty())
        .expect("every source has a final paragraph")
}
