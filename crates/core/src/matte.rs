use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatteMode {
    Alpha,
    AlphaInverted,
    Luma,
    LumaInverted,
}
impl MatteMode {
    pub const ALL: [Self; 4] = [
        Self::Alpha,
        Self::AlphaInverted,
        Self::Luma,
        Self::LumaInverted,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Alpha => "Alpha",
            Self::AlphaInverted => "Alpha inverted",
            Self::Luma => "Luma",
            Self::LumaInverted => "Luma inverted",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMatte {
    pub source: LayerId,
    pub mode: MatteMode,
}

impl Layer {
    pub fn track_matte(&self) -> Option<TrackMatte> {
        self.track_matte
    }
    pub(super) fn remap_matte(&mut self, mapping: &BTreeMap<LayerId, LayerId>) {
        if let Some(matte) = &mut self.track_matte {
            matte.source = mapping.get(&matte.source).copied().unwrap_or(matte.source);
        }
    }
}
impl Composition {
    /// Matte dependencies are independent of parent transforms and stacking order.
    pub fn can_track_matte(&self, target: LayerId, source: LayerId) -> bool {
        if self
            .layer(target)
            .is_none_or(|l| matches!(l.content, Content::Null))
        {
            return false;
        }
        let mut current = Some(source);
        let mut visited = BTreeSet::from([target]);
        while let Some(id) = current {
            if !visited.insert(id) || visited.len() > 17 {
                return false;
            }
            let Some(layer) = self.layer(id) else {
                return false;
            };
            if matches!(layer.content, Content::Null | Content::Adjustment) {
                return false;
            }
            current = layer.track_matte.map(|m| m.source);
        }
        true
    }
}
pub(super) fn validate(comp: &Composition, version: u32) -> Result<(), String> {
    for layer in &comp.layers {
        if let Some(matte) = layer.track_matte {
            if version < 18 {
                return Err("Track mattes require project version 18".into());
            }
            if !comp.can_track_matte(layer.id, matte.source) {
                return Err("Invalid track matte: missing source, circular reference, unsupported source, or more than 16 links".into());
            }
        }
    }
    Ok(())
}
pub(super) fn set(
    state: &mut Snapshot,
    id: LayerId,
    matte: Option<TrackMatte>,
) -> Result<(), String> {
    let comp = &mut state.project.composition;
    let target = comp.layer(id).ok_or("Layer not found")?;
    if target.locked {
        return Err("Unlock the layer before editing".into());
    }
    if let Some(matte) = matte {
        if !comp.can_track_matte(id, matte.source) {
            return Err("Choose a non-circular pixel source (up to 16 matte links); Null and Adjustment cannot be matte sources".into());
        }
        // On first assignment hide an editable source, as in AE. A locked source
        // can still be referenced but its visibility must not be changed.
        if target
            .track_matte
            .is_none_or(|old| old.source != matte.source)
        {
            let source = comp
                .layers
                .iter_mut()
                .find(|l| l.id == matte.source)
                .unwrap();
            if !source.locked {
                source.visible = false;
            }
        }
    }
    comp.layers
        .iter_mut()
        .find(|l| l.id == id)
        .unwrap()
        .track_matte = matte;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn set_matte(e: &mut Editor, id: LayerId, source: LayerId, mode: MatteMode) {
        e.execute(Command::SetTrackMatte {
            id,
            matte: Some(TrackMatte { source, mode }),
        })
        .unwrap();
    }
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        e
    }
    #[test]
    fn matte_modes_roundtrip_hide_source_atomically_and_preserve_locked_sources() {
        let mut e = scene();
        let before = e.project().clone();
        set_matte(&mut e, 1, 2, MatteMode::Alpha);
        let after = e.project().clone();
        assert!(!after.composition().layer(2).unwrap().visible());
        assert_eq!(after.version, 18);
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &after);
        for mode in MatteMode::ALL {
            set_matte(&mut e, 1, 2, mode);
            let p = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            assert_eq!(&p, e.project());
            assert_eq!(
                p.composition().layer(1).unwrap().track_matte(),
                Some(TrackMatte { source: 2, mode })
            );
        }
        let mut old = e.project().clone();
        old.version = 17;
        assert!(
            Project::from_json(&serde_json::to_string(&old).unwrap())
                .unwrap_err()
                .contains("version 18")
        );
        // Switching mode must not hide a source the user re-enabled.
        e.execute(Command::ToggleVisible(2)).unwrap();
        set_matte(&mut e, 1, 2, MatteMode::Luma);
        assert!(e.project().composition().layer(2).unwrap().visible());
        e.execute(Command::SetTrackMatte { id: 1, matte: None })
            .unwrap();
        e.execute(Command::ToggleLocked(2)).unwrap();
        set_matte(&mut e, 1, 2, MatteMode::AlphaInverted);
        assert!(e.project().composition().layer(2).unwrap().visible());
        e.execute(Command::ToggleLocked(1)).unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::SetTrackMatte { id: 1, matte: None })
                .is_err()
        );
        assert_eq!(e.project(), &before);
        let legacy = Project::from_json(&Project::default().to_json().unwrap()).unwrap();
        assert!(
            legacy
                .composition()
                .layers()
                .iter()
                .all(|l| l.track_matte().is_none())
        );
    }
    #[test]
    fn matte_cycles_missing_sources_depth_and_null_adjustment_sources_are_rejected() {
        let mut e = scene();
        set_matte(&mut e, 1, 2, MatteMode::Alpha);
        e.execute(Command::AddNull).unwrap();
        e.execute(Command::AddAdjustment).unwrap();
        for (id, source) in [(1, 1), (2, 1), (1, 999), (1, 3), (1, 4), (3, 1)] {
            let before = e.project().clone();
            assert!(
                e.execute(Command::SetTrackMatte {
                    id,
                    matte: Some(TrackMatte {
                        source,
                        mode: MatteMode::Alpha
                    })
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
        }
        // A matte can follow its consumer through the independent parent graph.
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        let mut invalid = e.project().clone();
        invalid
            .composition
            .layers
            .iter_mut()
            .find(|l| l.id == 2)
            .unwrap()
            .track_matte = Some(TrackMatte {
            source: 1,
            mode: MatteMode::Alpha,
        });
        assert!(Project::from_json(&serde_json::to_string(&invalid).unwrap()).is_err());
        let mut chain = Editor::default();
        for _ in 0..18 {
            chain.execute(Command::AddRectangle).unwrap();
        }
        for id in 1..17 {
            set_matte(&mut chain, id, id + 1, MatteMode::Alpha);
        }
        let before = chain.project().clone();
        // A new downstream link also validates existing upstream paths.
        assert!(
            chain
                .execute(Command::SetTrackMatte {
                    id: 17,
                    matte: Some(TrackMatte {
                        source: 18,
                        mode: MatteMode::Alpha
                    })
                })
                .is_err()
        );
        assert_eq!(chain.project(), &before);
    }
    #[test]
    fn matte_copy_duplicate_and_composition_clone_remap_references() {
        let mut e = scene();
        set_matte(&mut e, 1, 2, MatteMode::LumaInverted);
        e.execute(Command::DuplicateLayer(1)).unwrap();
        assert_eq!(e.selected_layer().unwrap().track_matte().unwrap().source, 2);
        e.execute(Command::DuplicateLayers(vec![1, 2])).unwrap();
        let copies: Vec<_> = e
            .project()
            .composition()
            .layers()
            .iter()
            .filter(|l| l.id() >= 4)
            .collect();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[1].track_matte().unwrap().source, copies[0].id());
        e.execute(Command::DuplicateComposition).unwrap();
        let comp = e.project().composition();
        assert!(comp.layers().iter().all(|l| {
            l.track_matte()
                .is_none_or(|m| comp.layer(m.source).is_some())
        }));
        e.activate_composition(1).unwrap();
        let one = e.copy_layers(&[1]).unwrap();
        let both = e.copy_layers(&[1, 2]).unwrap();
        e.execute(Command::PasteLayers(one.clone())).unwrap();
        assert_eq!(e.selected_layer().unwrap().track_matte().unwrap().source, 2);
        e.execute(Command::NewComposition).unwrap();
        let before = e.project().clone();
        assert!(e.execute(Command::PasteLayers(one)).is_err());
        assert_eq!(e.project(), &before);
        e.execute(Command::PasteLayers(both)).unwrap();
        let comp = e.project().composition();
        assert_eq!(
            comp.layers()[1].track_matte().unwrap().source,
            comp.layers()[0].id()
        );
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
    }
    #[test]
    fn matte_split_precompose_delete_and_reorder_keep_dependencies_consistent() {
        let mut e = scene();
        set_matte(&mut e, 1, 2, MatteMode::Alpha);
        e.execute(Command::MoveLayer { id: 2, index: 1 }).unwrap();
        assert_eq!(
            e.project()
                .composition()
                .layer(1)
                .unwrap()
                .track_matte()
                .unwrap()
                .source,
            2
        );
        for command in [
            Command::RemoveLayer(2),
            Command::Precompose {
                layers: vec![1],
                name: "Lost matte".into(),
            },
            Command::Precompose {
                layers: vec![2],
                name: "Lost consumer".into(),
            },
            Command::SplitLayers {
                ids: vec![2],
                frame: 30,
            },
        ] {
            let before = e.project().clone();
            assert!(e.execute(command).is_err());
            assert_eq!(e.project(), &before);
        }
        let before = e.project().clone();
        e.execute(Command::SplitLayers {
            ids: vec![1, 2],
            frame: 30,
        })
        .unwrap();
        let comp = e.project().composition();
        let new = comp
            .layers()
            .iter()
            .find(|l| l.id() > 2 && l.track_matte().is_some())
            .unwrap();
        let source = comp.layer(new.track_matte().unwrap().source).unwrap();
        assert_eq!((new.in_frame(), source.in_frame()), (30, 30));
        assert_eq!(
            (
                comp.layer(1).unwrap().out_frame(150),
                comp.layer(2).unwrap().out_frame(150)
            ),
            (30, 30)
        );
        e.undo();
        assert_eq!(e.project(), &before);
        e.execute(Command::Precompose {
            layers: vec![1, 2],
            name: "Matted".into(),
        })
        .unwrap();
        assert_eq!(
            e.project()
                .composition_by_id(2)
                .unwrap()
                .layer(1)
                .unwrap()
                .track_matte()
                .unwrap()
                .source,
            2
        );
        e.undo();
        e.execute(Command::Batch(vec![
            Command::RemoveLayer(2),
            Command::RemoveLayer(1),
        ]))
        .unwrap();
        assert!(e.project().composition().layers().is_empty());
        e.undo();
        assert_eq!(e.project(), &before);
    }
}
