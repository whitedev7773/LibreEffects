//! Project-wide font inventory and transactional replacement planning.
use libre_effects_core::{Command, Content, Project, TextFont};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(crate) struct Usage {
    pub composition: String,
    pub layer: String,
    pub locked: bool,
}
#[derive(Clone)]
pub(crate) struct Group {
    pub font: TextFont,
    pub actual: TextFont,
    pub warning: Option<String>,
    pub usages: Vec<Usage>,
}
impl Group {
    pub fn editable(&self) -> usize {
        self.usages.iter().filter(|u| !u.locked).count()
    }
}
pub(crate) fn inventory(project: &Project) -> Vec<Group> {
    let mut groups = BTreeMap::<TextFont, Vec<Usage>>::new();
    for (_, comp) in project.compositions() {
        for layer in comp.layers() {
            if matches!(layer.content(), Content::Text { .. }) {
                groups
                    .entry(TextFont::of(&layer.text_style()))
                    .or_default()
                    .push(Usage {
                        composition: comp.name().into(),
                        layer: layer.name().into(),
                        locked: layer.locked(),
                    });
            }
        }
    }
    groups
        .into_iter()
        .map(|(font, usages)| {
            let style = font.style();
            Group {
                actual: TextFont::of(&crate::fonts::resolved(&style)),
                warning: crate::fonts::warning(&style),
                font,
                usages,
            }
        })
        .collect()
}
pub(crate) fn missing_count(project: &Project) -> usize {
    inventory(project)
        .iter()
        .filter(|g| g.warning.is_some())
        .map(|g| g.usages.len())
        .sum()
}
pub(crate) struct Replacement {
    origin: Project,
    revision: u64,
    pub from: TextFont,
    pub to: TextFont,
    pub count: usize,
    pub locked: usize,
}
impl Replacement {
    pub fn new(
        project: &Project,
        revision: u64,
        from: TextFont,
        to: TextFont,
    ) -> Result<Self, String> {
        if from == to {
            return Err("Choose a different font or style".into());
        }
        if crate::fonts::warning(&to.style()).is_some() {
            return Err("Choose an installed replacement face".into());
        }
        let group = inventory(project)
            .into_iter()
            .find(|g| g.font == from)
            .ok_or("This font is no longer used")?;
        let count = group.editable();
        if count == 0 {
            return Err("All matching text layers are locked".into());
        }
        Ok(Self {
            origin: project.clone(),
            revision,
            from,
            to,
            count,
            locked: group.usages.len() - count,
        })
    }
    pub fn command(&self, project: &Project, revision: u64) -> Result<Command, String> {
        if project != &self.origin || revision != self.revision {
            return Err("The project changed. Choose the replacement again.".into());
        }
        Ok(Command::ReplaceTextFont {
            from: self.from.clone(),
            to: self.to.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, TextStyle};
    #[test]
    fn project_font_report_replacement_history_and_rendering() {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Fonts QA".into(),
            width: 480,
            height: 200,
            fps: 30,
            duration: 90,
        })
        .unwrap();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Font 한글".into(),
                font_size: 64.0,
            },
            width: 400.0,
            height: 100.0,
            name: "Title".into(),
        })
        .unwrap();
        let style = TextStyle {
            font_family: "LibreEffects missing font QA".into(),
            weight: 700,
            ..Default::default()
        };
        e.execute(Command::SetTextStyle {
            id: 1,
            style: style.clone(),
        })
        .unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        e.execute(Command::ToggleLocked(2)).unwrap();
        let before = e.project().clone();
        let groups = inventory(&before);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].editable(), 1);
        assert_eq!(groups[0].usages.len(), 2);
        assert!(groups[0].warning.as_ref().unwrap().contains("Missing font"));
        assert_eq!(groups[0].actual.family, "Wanted Sans");
        assert_eq!(missing_count(&before), 2);
        let plan =
            Replacement::new(&before, 7, groups[0].font.clone(), groups[0].actual.clone()).unwrap();
        assert_eq!((plan.count, plan.locked), (1, 1));
        assert!(plan.command(&before, 8).is_err());
        let mut changed = before.clone();
        changed.activate_composition(1).unwrap();
        assert!(plan.command(&changed, 7).is_err());
        let renderer = crate::rendering::Renderer::new();
        let expected = renderer.render(&changed, 0, 480).unwrap();
        e.execute(plan.command(&before, 7).unwrap()).unwrap();
        let after = e.project().clone();
        assert_eq!(missing_count(&after), 1);
        let mut saved = Project::from_json(&after.to_json().unwrap()).unwrap();
        saved.activate_composition(1).unwrap();
        assert_eq!(renderer.render_preview(&saved, 0, 480).unwrap(), expected);
        assert_eq!(
            renderer.render_output(&saved, 0, 480, 200).unwrap(),
            expected
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        assert!(
            Replacement::new(&after, 8, groups[0].font.clone(), groups[0].actual.clone()).is_err()
        );
        assert!(
            Replacement::new(&before, 7, groups[0].font.clone(), groups[0].font.clone()).is_err()
        );
        let unavailable = TextFont {
            family: "Missing replacement QA".into(),
            ..groups[0].actual.clone()
        };
        assert!(Replacement::new(&before, 7, groups[0].font.clone(), unavailable).is_err());
    }
    #[test]
    fn unavailable_face_and_slant_are_reported_separately_from_installed_fonts() {
        let mut e = Editor::default();
        for (i, style) in [
            TextStyle::default(),
            TextStyle {
                font_face: "WantedSans-Missing".into(),
                ..Default::default()
            },
            TextStyle {
                italic: true,
                ..Default::default()
            },
        ]
        .into_iter()
        .enumerate()
        {
            e.execute(Command::AddContent {
                content: Content::Text {
                    text: "Title".into(),
                    font_size: 48.0,
                },
                width: 200.0,
                height: 100.0,
                name: format!("Title {i}"),
            })
            .unwrap();
            e.execute(Command::SetTextStyle {
                id: e.selected().unwrap(),
                style,
            })
            .unwrap();
        }
        let groups = inventory(e.project());
        assert_eq!(groups.len(), 3);
        assert_eq!(missing_count(e.project()), 2);
        assert_eq!(groups.iter().filter(|g| g.warning.is_none()).count(), 1);
        assert!(groups.iter().all(|g| g.actual.family == "Wanted Sans"));
    }
}
