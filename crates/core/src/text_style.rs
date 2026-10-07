use serde::{Deserialize, Serialize};

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// Only the font identity; replacing it preserves paint, paragraph and spacing.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextFont {
    pub family: String,
    pub face: String,
    pub weight: u16,
    pub italic: bool,
}
impl TextFont {
    pub fn of(style: &TextStyle) -> Self {
        Self {
            family: style.font_family.clone(),
            face: style.font_face.clone(),
            weight: style.weight,
            italic: style.italic,
        }
    }
    pub fn apply(&self, style: &mut TextStyle) {
        style.font_family = self.family.clone();
        style.font_face = self.face.clone();
        style.weight = self.weight;
        style.italic = self.italic;
    }
    pub fn style(&self) -> TextStyle {
        let mut style = TextStyle::default();
        self.apply(&mut style);
        style
    }
}
pub(crate) fn replace_font(
    state: &mut crate::Snapshot,
    from: TextFont,
    to: TextFont,
) -> Result<(), String> {
    if !from.style().valid() || !to.style().valid() || from == to {
        return Err("Choose a different valid replacement font".into());
    }
    let mut count = 0;
    for layer in state.project.compositions_mut().flat_map(|c| &mut c.layers) {
        if layer.locked || !matches!(layer.content, crate::Content::Text { .. }) {
            continue;
        }
        let mut changed = false;
        if TextFont::of(&layer.text_style) == from {
            to.apply(&mut layer.text_style);
            changed = true;
        }
        if let Some(rich) = &mut layer.rich_text {
            rich.for_each_style(|style| changed |= style.replace_font(&from, &to));
        }
        if changed {
            count += 1;
        }
    }
    if count == 0 {
        return Err("No unlocked text layers use this font".into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextStrokeJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub font_family: String,
    /// PostScript face identity; empty means match the requested weight and slant.
    pub font_face: String,
    pub weight: u16,
    pub italic: bool,
    /// Line spacing as a multiplier of font size.
    pub leading: f64,
    /// Tracking in thousandths of an em, as in the Character panel.
    pub tracking: f64,
    pub align: TextAlign,
    /// Wrap and clip source text inside the layer's width and height.
    pub paragraph: bool,
    /// Paragraph-only offsets in source pixels; retained while in point mode.
    #[serde(skip_serializing_if = "is_zero")]
    pub paragraph_left_indent: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub paragraph_right_indent: f64,
    /// Added to the left indent on the first visual line of each paragraph.
    #[serde(skip_serializing_if = "is_zero")]
    pub paragraph_first_line_indent: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub paragraph_space_before: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub paragraph_space_after: f64,
    pub fill_enabled: bool,
    pub stroke_enabled: bool,
    pub stroke_color: u32,
    /// Centered on glyph outlines, in source pixels; does not affect advances.
    pub stroke_width: f64,
    /// Whole-layer paint order, including overlaps between lines and characters.
    pub stroke_over_fill: bool,
    pub stroke_join: TextStrokeJoin,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "Wanted Sans".into(),
            font_face: String::new(),
            weight: 400,
            italic: false,
            leading: 1.2,
            tracking: 0.0,
            align: TextAlign::Left,
            paragraph: false,
            paragraph_left_indent: 0.0,
            paragraph_right_indent: 0.0,
            paragraph_first_line_indent: 0.0,
            paragraph_space_before: 0.0,
            paragraph_space_after: 0.0,
            fill_enabled: true,
            stroke_enabled: false,
            stroke_color: 0,
            stroke_width: 1.0,
            stroke_over_fill: false,
            stroke_join: TextStrokeJoin::Miter,
        }
    }
}
impl TextStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn valid(&self) -> bool {
        !self.font_family.trim().is_empty()
            && self.font_family.len() <= 256
            && !self.font_family.chars().any(char::is_control)
            && self.font_face.len() <= 256
            && !self.font_face.chars().any(char::is_control)
            && (1..=1000).contains(&self.weight)
            && self.leading.is_finite()
            && (0.1..=10.0).contains(&self.leading)
            && self.tracking.is_finite()
            && (-1000.0..=10000.0).contains(&self.tracking)
            && TextParagraphField::ALL
                .into_iter()
                .all(|field| field.accepts(field.value(self)))
            && self.stroke_color <= 0xffffff
            && self.stroke_width.is_finite()
            && (0.0..=1000.0).contains(&self.stroke_width)
    }
    /// Includes dormant values so point-mode documents retain their schema gate.
    pub fn has_paragraph_override(&self) -> bool {
        TextParagraphField::ALL
            .into_iter()
            .any(|field| field.value(self) != 0.0)
    }
    pub fn has_paint_override(&self) -> bool {
        !self.fill_enabled
            || self.stroke_enabled
            || self.stroke_color != 0
            || self.stroke_width != 1.0
            || self.stroke_over_fill
            || self.stroke_join != TextStrokeJoin::Miter
    }
    pub fn has_font_override(&self) -> bool {
        self.font_family != "Wanted Sans"
            || !self.font_face.is_empty()
            || self.weight != 400
            || self.italic
    }
}

/// Static whole-layer paragraph fields; these are not animation parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextParagraphField {
    LeftIndent,
    RightIndent,
    FirstLineIndent,
    SpaceBefore,
    SpaceAfter,
}
impl TextParagraphField {
    pub const ALL: [Self; 5] = [
        Self::LeftIndent,
        Self::RightIndent,
        Self::FirstLineIndent,
        Self::SpaceBefore,
        Self::SpaceAfter,
    ];

    pub const fn bounds(self) -> (f64, f64) {
        match self {
            Self::FirstLineIndent => (-16_384.0, 16_384.0),
            _ => (0.0, 16_384.0),
        }
    }

    pub fn value(self, style: &TextStyle) -> f64 {
        match self {
            Self::LeftIndent => style.paragraph_left_indent,
            Self::RightIndent => style.paragraph_right_indent,
            Self::FirstLineIndent => style.paragraph_first_line_indent,
            Self::SpaceBefore => style.paragraph_space_before,
            Self::SpaceAfter => style.paragraph_space_after,
        }
    }

    fn accepts(self, value: f64) -> bool {
        value.is_finite() && (self.bounds().0..=self.bounds().1).contains(&value)
    }

    fn set(self, style: &mut TextStyle, value: f64) {
        match self {
            Self::LeftIndent => style.paragraph_left_indent = value,
            Self::RightIndent => style.paragraph_right_indent = value,
            Self::FirstLineIndent => style.paragraph_first_line_indent = value,
            Self::SpaceBefore => style.paragraph_space_before = value,
            Self::SpaceAfter => style.paragraph_space_after = value,
        }
    }
}

impl crate::Layer {
    /// Plan a static, exact paragraph-field edit without sampling typography or
    /// Source Text. Equal finite values, including differently formatted zero,
    /// preserve the authored source and create no history entry.
    pub fn text_paragraph_value_command(
        &self,
        field: TextParagraphField,
        value: f64,
    ) -> Result<Option<crate::Command>, String> {
        if !matches!(self.content, crate::Content::Text { .. }) {
            return Err("Select a text layer".into());
        }
        if self.locked {
            return Err("Unlock the layer before changing its text".into());
        }
        if !self.text_style.valid() || !field.accepts(value) {
            return Err("Invalid paragraph style value".into());
        }
        Ok((field.value(&self.text_style) != value).then_some(
            crate::Command::SetTextParagraphValue {
                id: self.id,
                field,
                value,
            },
        ))
    }
}

pub(super) fn paragraph_edits_only(command: &crate::Command) -> bool {
    fn classify(command: &crate::Command) -> Option<bool> {
        match command {
            crate::Command::SetTextParagraphValue { .. } => Some(true),
            crate::Command::Batch(commands) => commands
                .iter()
                .try_fold(false, |found, command| Some(found | classify(command)?)),
            _ => None,
        }
    }
    classify(command) == Some(true)
}

pub(super) fn paragraph_materialized(project: &crate::Project) -> bool {
    project.compositions().into_iter().any(|(_, comp)| {
        comp.layers
            .iter()
            .any(|layer| layer.text_style.has_paragraph_override())
    })
}

pub(super) fn apply_paragraph_value(
    state: &mut crate::Snapshot,
    id: crate::LayerId,
    field: TextParagraphField,
    value: f64,
) -> Result<(), String> {
    let layer = crate::editing::editable(state, id)?;
    // Validate eligibility and input even for exact no-ops. Equal values do not
    // rewrite source bits, including dormant -0.0.
    if layer.text_paragraph_value_command(field, value)?.is_some() {
        field.set(&mut layer.text_style, value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Content, Editor, Project};
    #[test]
    fn replace_font_across_compositions_is_atomic_and_preserves_locked_layers() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Original 한글\nTitle".into(),
                font_size: 48.0,
            },
            width: 500.0,
            height: 200.0,
            name: "Text".into(),
        })
        .unwrap();
        let style = TextStyle {
            font_family: "Missing Font".into(),
            font_face: "Missing-Bold".into(),
            weight: 700,
            paragraph: true,
            leading: 1.5,
            tracking: 45.0,
            stroke_enabled: true,
            stroke_color: 0x123456,
            stroke_width: 8.0,
            ..Default::default()
        };
        e.execute(Command::SetTextStyle {
            id: 1,
            style: style.clone(),
        })
        .unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        e.execute(Command::DuplicateLayer(2)).unwrap();
        e.execute(Command::ToggleLocked(3)).unwrap();
        let before = e.project().clone();
        let selected = e.selected();
        let from = TextFont::of(&style);
        let to = TextFont {
            family: "Wanted Sans".into(),
            face: "WantedSans-Bold".into(),
            weight: 700,
            italic: false,
        };
        e.execute(Command::ReplaceTextFont {
            from: from.clone(),
            to: to.clone(),
        })
        .unwrap();
        let after = e.project().clone();
        assert_eq!(e.selected(), selected);
        assert_eq!(
            after.active_composition_id(),
            before.active_composition_id()
        );
        let mut changed = 0;
        for ((a_id, a), (b_id, b)) in after.compositions().into_iter().zip(before.compositions()) {
            assert_eq!(a_id, b_id);
            for (actual, old) in a.layers().iter().zip(b.layers()) {
                let mut expected = old.clone();
                if !old.locked() {
                    to.apply(&mut expected.text_style);
                    changed += 1;
                }
                assert_eq!(actual, &expected);
            }
        }
        assert_eq!(changed, 2);
        assert_eq!(
            Project::from_json(&after.to_json().unwrap()).unwrap(),
            after
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        assert!(
            e.execute(Command::ReplaceTextFont {
                from: from.clone(),
                to: to.clone()
            })
            .is_err()
        );
        assert_eq!(e.project(), &after);
        e.undo();
        let mut invalid = to.clone();
        invalid.family.clear();
        assert!(
            e.execute(Command::ReplaceTextFont {
                from: from.clone(),
                to: invalid
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        assert!(
            e.execute(Command::ReplaceTextFont {
                from: from.clone(),
                to: from
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn text_paint_roundtrip_version_history_and_validation() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Stroke 한글".into(),
                font_size: 48.0,
            },
            width: 500.0,
            height: 100.0,
            name: "Title".into(),
        })
        .unwrap();
        let id = e.selected().unwrap();
        let before = e.project().clone();
        let style = TextStyle {
            fill_enabled: false,
            stroke_enabled: true,
            stroke_width: 12.5,
            stroke_color: 0xabcdef,
            stroke_over_fill: true,
            stroke_join: TextStrokeJoin::Bevel,
            ..Default::default()
        };
        e.execute(Command::SetTextStyle {
            id,
            style: style.clone(),
        })
        .unwrap();
        let after = e.project().clone();
        let json = after.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), after);
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], 34);
        value["version"] = 33.into();
        assert!(Project::from_json(&value.to_string()).is_err());
        for bad in [
            TextStyle {
                stroke_width: -1.0,
                ..style.clone()
            },
            TextStyle {
                stroke_width: f64::NAN,
                ..style.clone()
            },
            TextStyle {
                stroke_width: 1000.01,
                ..style.clone()
            },
            TextStyle {
                stroke_color: 0x1000000,
                ..style.clone()
            },
        ] {
            assert!(e.execute(Command::SetTextStyle { id, style: bad }).is_err());
            assert_eq!(e.project(), &after);
        }
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        e.execute(Command::ToggleLocked(id)).unwrap();
        assert!(
            e.execute(Command::SetTextStyle {
                id,
                style: Default::default()
            })
            .is_err()
        );
        let legacy: TextStyle = serde_json::from_str("{}").unwrap();
        assert!(legacy.fill_enabled && !legacy.stroke_enabled && !legacy.has_paint_override());
    }
    #[test]
    fn paragraph_box_versions_history_and_resize_preserve_source_and_transform() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "one two 한글".into(),
                font_size: 48.0,
            },
            width: 640.0,
            height: 120.0,
            name: "Text".into(),
        })
        .unwrap();
        let id = e.selected().unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::SetTextBox {
                id,
                width: 200.0,
                height: 300.0
            })
            .is_err()
        );
        e.execute(Command::Batch(vec![
            Command::SetTextStyle {
                id,
                style: TextStyle {
                    paragraph: true,
                    ..Default::default()
                },
            },
            Command::SetTextBox {
                id,
                width: 200.0,
                height: 300.0,
            },
        ]))
        .unwrap();
        let after = e.project().clone();
        let layer = after.composition().layer(id).unwrap();
        assert_eq!((layer.width(), layer.height()), (200.0, 300.0));
        assert_eq!(
            layer.content(),
            before.composition().layer(id).unwrap().content()
        );
        assert_eq!(
            after.composition().world_transform(id, 0),
            before.composition().world_transform(id, 0)
        );
        let json = after.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), after);
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], 33);
        value["version"] = 32.into();
        assert!(Project::from_json(&value.to_string()).is_err());
        for width in [0.0, 16385.0, f64::NAN] {
            assert!(
                e.execute(Command::SetTextBox {
                    id,
                    width,
                    height: 100.0
                })
                .is_err()
            );
            assert_eq!(e.project(), &after);
        }
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        e.execute(Command::SetTextStyle {
            id,
            style: TextStyle::default(),
        })
        .unwrap();
        assert_eq!(e.selected_layer().unwrap().content(), layer.content());
        e.undo();
        assert_eq!(e.project(), &after);
        e.execute(Command::ToggleLocked(id)).unwrap();
        assert!(
            e.execute(Command::SetTextBox {
                id,
                width: 80.0,
                height: 80.0
            })
            .is_err()
        );
    }
    #[test]
    fn font_selection_versions_history_and_invalid_edits() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "한글 Title".into(),
                font_size: 48.0,
            },
            width: 500.0,
            height: 100.0,
            name: "Title".into(),
        })
        .unwrap();
        let id = e.selected().unwrap();
        let before = e.project().clone();
        let legacy = Project::from_json(&before.to_json().unwrap()).unwrap();
        assert_eq!(
            legacy.composition().layer(id).unwrap().text_style(),
            TextStyle::default()
        );
        let style = TextStyle {
            font_family: "Example Missing Font".into(),
            font_face: "ExampleMissing-BoldItalic".into(),
            weight: 700,
            italic: true,
            ..Default::default()
        };
        e.execute(Command::SetTextStyle {
            id,
            style: style.clone(),
        })
        .unwrap();
        let saved = e.project().clone();
        let json = saved.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), saved);
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], 32);
        value["version"] = 31.into();
        assert!(Project::from_json(&value.to_string()).is_err());
        e.undo();
        assert_eq!(*e.project(), before);
        e.redo();
        assert_eq!(*e.project(), saved);
        for bad in [
            TextStyle {
                weight: 0,
                ..style.clone()
            },
            TextStyle {
                weight: 1001,
                ..style.clone()
            },
            TextStyle {
                font_family: " ".into(),
                ..style.clone()
            },
            TextStyle {
                font_family: "Bad\nfont".into(),
                ..style.clone()
            },
            TextStyle {
                font_family: "x".repeat(257),
                ..style.clone()
            },
        ] {
            assert!(e.execute(Command::SetTextStyle { id, style: bad }).is_err());
            assert_eq!(*e.project(), saved);
        }
        e.execute(Command::ToggleLocked(id)).unwrap();
        assert!(
            e.execute(Command::SetTextStyle {
                id,
                style: TextStyle::default()
            })
            .is_err()
        );
    }
    #[test]
    fn typography_roundtrips_and_invalid_or_locked_edits_are_atomic() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Title\nSubtitle".into(),
                font_size: 48.0,
            },
            width: 600.0,
            height: 160.0,
            name: "Text".into(),
        })
        .unwrap();
        let id = e.selected().unwrap();
        let style = TextStyle {
            leading: 1.5,
            tracking: 125.0,
            align: TextAlign::Center,
            ..Default::default()
        };
        e.execute(Command::SetTextStyle {
            id,
            style: style.clone(),
        })
        .unwrap();
        let saved = e.project().clone();
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        e.undo();
        assert!(e.selected_layer().unwrap().text_style().is_default());
        e.redo();
        assert_eq!(*e.project(), saved);
        assert!(
            e.execute(Command::SetTextStyle {
                id,
                style: TextStyle {
                    tracking: f64::NAN,
                    ..style
                }
            })
            .is_err()
        );
        assert_eq!(*e.project(), saved);
        e.execute(Command::ToggleLocked(id)).unwrap();
        assert!(
            e.execute(Command::SetTextStyle {
                id,
                style: TextStyle::default()
            })
            .is_err()
        );
    }
}
