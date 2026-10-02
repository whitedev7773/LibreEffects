use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
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
    }
    pub fn has_font_override(&self) -> bool {
        self.font_family != "Wanted Sans"
            || !self.font_face.is_empty()
            || self.weight != 400
            || self.italic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Content, Editor, Project};
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
