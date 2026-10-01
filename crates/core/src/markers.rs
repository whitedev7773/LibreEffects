//! Timeline annotations use composition frames, including markers attached to layers.
use super::*;
pub type MarkerId = u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarkerTarget {
    Composition,
    Layer(LayerId),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    id: MarkerId,
    frame: Frame,
    duration: Frame,
    name: String,
    color: u32,
}
impl Marker {
    pub fn id(&self) -> MarkerId {
        self.id
    }
    pub fn frame(&self) -> Frame {
        self.frame
    }
    pub fn duration(&self) -> Frame {
        self.duration
    }
    pub fn end(&self) -> Frame {
        self.frame.saturating_add(self.duration)
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn color(&self) -> u32 {
        self.color
    }
}
#[derive(Clone, Debug)]
pub enum MarkerEdit {
    Add {
        frame: Frame,
    },
    Update {
        id: MarkerId,
        frame: Frame,
        duration: Frame,
        name: String,
        color: u32,
    },
    Remove(MarkerId),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Markers {
    next_id: MarkerId,
    items: Vec<Marker>,
}
impl Default for Markers {
    fn default() -> Self {
        Self {
            next_id: 1,
            items: Vec::new(),
        }
    }
}
impl Markers {
    pub(super) fn is_default(&self) -> bool {
        self.next_id == 1 && self.items.is_empty()
    }
    pub(super) fn validate(&self, duration: Frame) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        let mut frames = BTreeSet::new();
        if self.next_id == 0
            || self.items.len() > 1000
            || self.items.iter().any(|m| {
                m.id == 0
                    || m.id >= self.next_id
                    || !ids.insert(m.id)
                    || !frames.insert(m.frame)
                    || m.frame >= duration
                    || m.duration > duration - m.frame
                    || m.name.len() > 1024
                    || m.color > 0xffffff
                    || m.name.contains('\0')
            })
        {
            return Err("Markers need unique IDs/times, valid names/colors and a range inside the composition".into());
        }
        Ok(())
    }
    pub(super) fn shift(&mut self, delta: i64, duration: Frame) -> Result<(), String> {
        for m in &mut self.items {
            m.frame = u32::try_from(i64::from(m.frame) + delta)
                .map_err(|_| "Moving would place a marker outside the composition")?;
        }
        self.validate(duration)
    }
    pub(super) fn resample(
        &mut self,
        from: FrameRate,
        to: FrameRate,
        duration: Frame,
    ) -> Result<(), String> {
        let convert = |f: Frame| -> Result<Frame, String> {
            u32::try_from(
                from.convert_frames(u64::from(f), to, FrameRounding::Nearest)
                    .ok_or("Marker time overflow")?,
            )
            .map_err(|_| "Marker time overflow".into())
        };
        for m in &mut self.items {
            let end = convert(m.end())?;
            m.frame = convert(m.frame)?;
            let span = end.saturating_sub(m.frame);
            if m.duration > 0 && span == 0 {
                return Err("Destination frame rate would erase a marker duration".into());
            }
            m.duration = span;
        }
        self.validate(duration)
    }
    /// A duration spanning the split becomes one clipped annotation on each half.
    pub(super) fn split(&mut self, right: &mut Self, frame: Frame) -> Result<(), String> {
        self.items.retain(|m| m.frame < frame);
        for m in &mut self.items {
            m.duration = m.duration.min(frame - m.frame);
        }
        right.items.retain(|m| m.frame >= frame || m.end() > frame);
        for m in &mut right.items {
            if m.frame < frame {
                m.duration = m.end() - frame;
                m.frame = frame;
            }
        }
        let mut frames = BTreeSet::new();
        if right.items.iter().any(|m| !frames.insert(m.frame)) {
            return Err(
                "Split would merge overlapping marker starts; adjust the marker ranges first"
                    .into(),
            );
        }
        Ok(())
    }
}
impl Layer {
    pub fn markers(&self) -> &[Marker] {
        &self.markers.items
    }
}
impl Composition {
    pub fn markers(&self) -> &[Marker] {
        &self.markers.items
    }
    pub fn marker_track(&self, target: MarkerTarget) -> Option<&[Marker]> {
        match target {
            MarkerTarget::Composition => Some(self.markers()),
            MarkerTarget::Layer(id) => Some(self.layer(id)?.markers()),
        }
    }
}
pub(super) fn apply(
    state: &mut Snapshot,
    target: MarkerTarget,
    edit: MarkerEdit,
) -> Result<(), String> {
    let comp = &mut state.project.composition;
    let duration = comp.duration;
    let markers = match target {
        MarkerTarget::Composition => &mut comp.markers,
        MarkerTarget::Layer(id) => {
            let layer = comp
                .layers
                .iter_mut()
                .find(|l| l.id == id)
                .ok_or("Layer not found")?;
            if layer.locked {
                return Err("Unlock the layer before editing markers".into());
            }
            &mut layer.markers
        }
    };
    match edit {
        MarkerEdit::Add { frame } => {
            if markers.next_id >= u64::MAX - 1 {
                return Err("Marker ID limit reached".into());
            }
            let id = markers.next_id;
            markers.next_id += 1;
            markers.items.push(Marker {
                id,
                frame,
                duration: 0,
                name: format!("Marker {id}"),
                color: 0xe7bc6a,
            });
        }
        MarkerEdit::Update {
            id,
            frame,
            duration,
            name,
            color,
        } => {
            let m = markers
                .items
                .iter_mut()
                .find(|m| m.id == id)
                .ok_or("Marker not found")?;
            *m = Marker {
                id,
                frame,
                duration,
                name,
                color,
            };
        }
        MarkerEdit::Remove(id) => {
            let at = markers
                .items
                .iter()
                .position(|m| m.id == id)
                .ok_or("Marker not found")?;
            markers.items.remove(at);
        }
    }
    markers.items.sort_by_key(|m| m.frame);
    markers.validate(duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn marker(e: &mut Editor, target: MarkerTarget, frame: Frame, duration: Frame) {
        e.execute(Command::Marker {
            target,
            edit: MarkerEdit::Add { frame },
        })
        .unwrap();
        let id = e
            .project()
            .composition()
            .marker_track(target)
            .unwrap()
            .iter()
            .map(Marker::id)
            .max()
            .unwrap();
        e.execute(Command::Marker {
            target,
            edit: MarkerEdit::Update {
                id,
                frame,
                duration,
                name: "장면 전환".into(),
                color: 0x53d8c4,
            },
        })
        .unwrap();
    }
    #[test]
    fn markers_follow_shift_split_undo_save_and_precompose_without_copying_composition_markers() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetLayerRange {
            id: 1,
            start: 0,
            end: 120,
        })
        .unwrap();
        marker(&mut e, MarkerTarget::Composition, 20, 30);
        marker(&mut e, MarkerTarget::Layer(1), 40, 50);
        e.execute(Command::ShiftLayer { id: 1, delta: 10 }).unwrap();
        assert_eq!(
            e.project().composition().layer(1).unwrap().markers()[0].frame(),
            50
        );
        let before = e.project().clone();
        e.execute(Command::SplitLayers {
            ids: vec![1],
            frame: 70,
        })
        .unwrap();
        let split = e.project().clone();
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &split);
        let comp = e.project().composition();
        assert_eq!(
            (
                comp.layer(1).unwrap().markers()[0].frame(),
                comp.layer(1).unwrap().markers()[0].duration()
            ),
            (50, 20)
        );
        assert_eq!(
            (
                comp.layer(2).unwrap().markers()[0].frame(),
                comp.layer(2).unwrap().markers()[0].duration()
            ),
            (70, 30)
        );
        e.execute(Command::Precompose {
            layers: vec![2],
            name: "Nested markers".into(),
        })
        .unwrap();
        assert_eq!(e.project().composition().markers().len(), 1);
        let source = e.project().composition_by_id(2).unwrap();
        assert!(source.markers().is_empty());
        assert_eq!(source.layer(2).unwrap().markers()[0].name(), "장면 전환");
        let restored = Project::from_json(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(restored, *e.project());
        assert_eq!(restored.version, 13);
    }
    #[test]
    fn clipboard_resamples_marker_start_and_end_and_rejects_time_collisions() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        marker(&mut e, MarkerTarget::Layer(1), 40, 50);
        let clipboard = e.copy_layers(&[1]).unwrap();
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::ConfigureComposition {
            name: "24 fps".into(),
            width: 1920,
            height: 1080,
            fps: 24,
            duration: 150,
        })
        .unwrap();
        e.execute(Command::PasteLayers(clipboard)).unwrap();
        let m = &e.selected_layer().unwrap().markers()[0];
        assert_eq!((m.frame(), m.duration()), (32, 40));
        e.activate_composition(1).unwrap();
        marker(&mut e, MarkerTarget::Layer(1), 41, 0);
        let clipboard = e.copy_layers(&[1]).unwrap();
        e.activate_composition(2).unwrap();
        e.execute(Command::ConfigureComposition {
            name: "1 fps".into(),
            width: 1920,
            height: 1080,
            fps: 1,
            duration: 150,
        })
        .unwrap();
        let before = e.project().clone();
        assert!(e.execute(Command::PasteLayers(clipboard)).is_err());
        assert_eq!(e.project(), &before);
    }
    #[test]
    fn invalid_marker_edits_locked_layers_and_overlapping_splits_are_atomic() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        marker(&mut e, MarkerTarget::Composition, 145, 0);
        let before = e.project().clone();
        for edit in [
            MarkerEdit::Add { frame: 145 },
            MarkerEdit::Update {
                id: 1,
                frame: 149,
                duration: 2,
                name: "Bad".into(),
                color: 0xffffff,
            },
            MarkerEdit::Update {
                id: 1,
                frame: 40,
                duration: 0,
                name: "Bad".into(),
                color: 0x1000000,
            },
        ] {
            assert!(
                e.execute(Command::Marker {
                    target: MarkerTarget::Composition,
                    edit
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
        }
        assert!(
            e.execute(Command::ConfigureComposition {
                name: "Short".into(),
                width: 1920,
                height: 1080,
                fps: 30,
                duration: 140
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(
            e.execute(Command::Marker {
                target: MarkerTarget::Layer(1),
                edit: MarkerEdit::Add { frame: 0 }
            })
            .is_err()
        );
        e.execute(Command::ToggleLocked(1)).unwrap();
        marker(&mut e, MarkerTarget::Layer(1), 10, 80);
        marker(&mut e, MarkerTarget::Layer(1), 20, 80);
        let before = e.project().clone();
        assert!(
            e.execute(Command::SplitLayers {
                ids: vec![1],
                frame: 50
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        let mut json: serde_json::Value =
            serde_json::from_str(&e.project().to_json().unwrap()).unwrap();
        json["version"] = 12.into();
        assert!(Project::from_json(&json.to_string()).is_err());
    }
}
