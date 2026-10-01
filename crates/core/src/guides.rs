//! Composition-space layout references. Guides never contribute rendered pixels.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub axis: GuideAxis,
    pub position: f64,
}
impl Composition {
    pub fn guides(&self) -> &[Guide] {
        &self.guides
    }
}
pub(super) fn validate(guides: &[Guide]) -> Result<(), String> {
    if guides.len() > 256 {
        return Err("A composition supports at most 256 guides".into());
    }
    if guides
        .iter()
        .any(|g| !g.position.is_finite() || g.position.abs() > 32768.0)
    {
        return Err("Guide positions must be between -32768 and 32768 pixels".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guide_edits_are_atomic_undoable_and_persist_per_composition() {
        let mut e = Editor::default();
        let before = e.project().clone();
        let guides = vec![
            Guide {
                axis: GuideAxis::Vertical,
                position: 220.5,
            },
            Guide {
                axis: GuideAxis::Horizontal,
                position: -8.0,
            },
        ];
        e.execute(Command::SetGuides(guides.clone())).unwrap();
        let saved = e.project().clone();
        assert_eq!(saved.composition().guides(), guides);
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &saved);
        let json = saved.to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), saved);
        assert!(Project::from_json(&json.replace("\"version\": 20", "\"version\": 19")).is_err());
        for guides in [
            vec![Guide {
                axis: GuideAxis::Vertical,
                position: f64::NAN,
            }],
            vec![Guide {
                axis: GuideAxis::Vertical,
                position: 32769.0,
            }],
            vec![guides[0]; 257],
        ] {
            assert!(e.execute(Command::SetGuides(guides)).is_err());
            assert_eq!(e.project(), &saved);
        }
        e.execute(Command::DuplicateComposition).unwrap();
        assert_eq!(e.project().composition().guides(), guides);
        e.execute(Command::NewComposition).unwrap();
        assert!(e.project().composition().guides().is_empty());
        e.activate_composition(1).unwrap();
        e.execute(Command::SetGuides(Vec::new())).unwrap();
        assert!(e.project().composition().guides().is_empty());
        e.undo();
        assert_eq!(e.project().composition().guides(), guides);
    }
}
