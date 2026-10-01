use super::*;

impl Project {
    pub fn active_composition_id(&self) -> CompositionId {
        self.composition_id
    }
    pub fn compositions(&self) -> Vec<(CompositionId, &Composition)> {
        let mut result: Vec<_> = std::iter::once((self.composition_id, &self.composition))
            .chain(self.other_compositions.iter().map(|(id, comp)| (*id, comp)))
            .collect();
        result.sort_by_key(|(id, _)| *id);
        result
    }
    pub(super) fn compositions_mut(&mut self) -> impl Iterator<Item = &mut Composition> {
        std::iter::once(&mut self.composition).chain(self.other_compositions.values_mut())
    }
    pub fn same_document(&self, other: &Self) -> bool {
        self.version == other.version
            && self.next_layer_id == other.next_layer_id
            && self.next_composition_id == other.next_composition_id
            && self.compositions() == other.compositions()
    }
    pub fn activate_composition(&mut self, id: CompositionId) -> Result<(), String> {
        if id == self.composition_id {
            return Ok(());
        }
        let comp = self
            .other_compositions
            .remove(&id)
            .ok_or("Composition not found")?;
        let previous = std::mem::replace(&mut self.composition, comp);
        self.other_compositions
            .insert(self.composition_id, previous);
        self.composition_id = id;
        Ok(())
    }
}
impl Editor {
    pub fn activate_composition(&mut self, id: CompositionId) -> Result<(), String> {
        self.current.project.activate_composition(id)?;
        self.current.selected = self
            .current
            .project
            .composition
            .layers
            .first()
            .map(Layer::id);
        Ok(())
    }
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    if !matches!(
        command,
        Command::NewComposition | Command::DuplicateComposition | Command::DeleteComposition
    ) {
        return None;
    }
    Some((|| {
        let project = &mut state.project;
        if matches!(command, Command::DeleteComposition) {
            let id = *project
                .other_compositions
                .keys()
                .next()
                .ok_or("The last composition cannot be deleted")?;
            let old = project.composition_id;
            project.activate_composition(id)?;
            project.other_compositions.remove(&old);
        } else {
            if project.other_compositions.len() >= 99 || project.next_composition_id == u64::MAX {
                return Err("Composition limit reached".into());
            }
            let id = project.next_composition_id;
            project.next_composition_id += 1;
            let mut comp = project.composition.clone();
            if matches!(command, Command::DuplicateComposition) {
                let mut mapping = BTreeMap::new();
                for layer in &comp.layers {
                    if project.next_layer_id >= u64::MAX - 1 {
                        return Err("Layer ID limit reached".into());
                    }
                    mapping.insert(layer.id, project.next_layer_id);
                    project.next_layer_id += 1;
                }
                for layer in &mut comp.layers {
                    layer.id = mapping[&layer.id];
                    layer.parent = layer.parent.map(|id| mapping[&id]);
                }
                comp.name = format!("{} copy", comp.name);
            } else {
                comp.layers.clear();
                comp.work_area = None;
                comp.name = format!("Composition {id:02}");
            }
            project.other_compositions.insert(id, comp);
            project.activate_composition(id)?;
        }
        state.selected = project.composition.layers.first().map(Layer::id);
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_compositions_switch_without_editing_and_duplicate_remaps_parents() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        e.execute(Command::SetWorkArea { start: 10, end: 30 })
            .unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        assert_eq!(e.project().compositions().len(), 2);
        assert_eq!(e.project().composition().work_area(), 10..30);
        let layers = e.project().composition().layers();
        assert_ne!(layers[0].id(), 2);
        assert_eq!(layers[0].parent(), Some(layers[1].id()));
        let saved = e.project().clone();
        e.activate_composition(1).unwrap();
        assert!(e.project().same_document(&saved));
        e.undo();
        assert_eq!(e.project().compositions().len(), 1);
        e.redo();
        assert!(e.project().same_document(&saved));
        e.activate_composition(2).unwrap();
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        e.execute(Command::DeleteComposition).unwrap();
        assert_eq!(e.project().active_composition_id(), 1);
        assert!(e.execute(Command::DeleteComposition).is_err());
        e.undo();
        assert_eq!(e.project(), &saved);
    }
    #[test]
    fn images_are_shared_and_validated_in_inactive_compositions() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Image { png: "YWJj".into() },
            width: 2.0,
            height: 2.0,
            name: "Image".into(),
        })
        .unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        e.execute(Command::NewComposition).unwrap();
        assert!(e.project().composition().layers().is_empty());
        let json = e.project().to_json().unwrap();
        assert_eq!(json.matches("YWJj").count(), 1);
        assert_eq!(Project::from_json(&json).unwrap(), *e.project());
        let mut broken = serde_json::from_str::<serde_json::Value>(&json).unwrap();
        broken["other_compositions"]["1"]["fps"] = 0.into();
        assert!(Project::from_json(&broken.to_string()).is_err());
    }
    #[test]
    fn composition_limits_and_invalid_references_fail_without_changing_document() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::DuplicateComposition).unwrap();
        let saved = e.project().clone();
        assert!(e.activate_composition(99).is_err());
        assert_eq!(e.project(), &saved);
        let mut invalid = saved.clone();
        invalid.other_compositions.get_mut(&1).unwrap().layers[0].id =
            invalid.composition.layers[0].id;
        assert!(invalid.validate().is_err());
        let mut invalid = saved.clone();
        invalid.composition.layers[0].parent = Some(1);
        assert!(invalid.validate().is_err());
        let mut invalid = saved.clone();
        invalid.version = 8;
        assert!(invalid.validate().is_err());
        while e.project().compositions().len() < 100 {
            e.execute(Command::NewComposition).unwrap();
        }
        let saved = e.project().clone();
        assert!(e.execute(Command::NewComposition).is_err());
        assert_eq!(e.project(), &saved);
    }
}
