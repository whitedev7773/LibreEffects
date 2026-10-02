use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    /// Line spacing as a multiplier of font size.
    pub leading: f64,
    /// Tracking in thousandths of an em, as in the Character panel.
    pub tracking: f64,
    pub align: TextAlign,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
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
        self.leading.is_finite()
            && (0.1..=10.0).contains(&self.leading)
            && self.tracking.is_finite()
            && (-1000.0..=10000.0).contains(&self.tracking)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Content, Editor, Project};
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
        };
        e.execute(Command::SetTextStyle { id, style }).unwrap();
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
