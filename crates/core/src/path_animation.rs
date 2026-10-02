use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathTarget {
    Shape,
    Mask(u64),
}

/// The timing track stores opaque pose references, never editable numeric values.
/// This lets path keys share time selection, clipboard and retiming with other tracks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathAnimation {
    poses: Vec<VectorPath>,
    pub(super) timing: AnimatedProperty,
}
impl Default for PathAnimation {
    fn default() -> Self {
        Self {
            poses: Vec::new(),
            timing: AnimatedProperty::new(0.0),
        }
    }
}
impl PathAnimation {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn animated(&self) -> bool {
        !self.timing.keys.is_empty()
    }
    pub fn at(&self, base: &VectorPath, frame: Frame) -> VectorPath {
        if self.poses.is_empty() {
            return base.clone();
        }
        let left = self.timing.keys.range(..=frame).next_back();
        let right = self
            .timing
            .keys
            .range((std::ops::Bound::Excluded(frame), std::ops::Bound::Unbounded))
            .next();
        let index = |v: f64| self.poses.get(v as usize).unwrap_or(base);
        match (left, right) {
            (Some((_, a)), Some((_, b))) if a.value != b.value => {
                let t = (self.timing.value_at(frame) - a.value) / (b.value - a.value);
                let a = index(a.value);
                let b = index(b.value);
                let lerp =
                    |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                VectorPath {
                    closed: a.closed,
                    vertices: a
                        .vertices
                        .iter()
                        .zip(&b.vertices)
                        .map(|(a, b)| PathVertex {
                            position: lerp(a.position, b.position),
                            incoming: lerp(a.incoming, b.incoming),
                            outgoing: lerp(a.outgoing, b.outgoing),
                        })
                        .collect(),
                }
            }
            (Some((_, k)), _) | (_, Some((_, k))) => index(k.value).clone(),
            _ => index(self.timing.value).clone(),
        }
    }
    fn intern(&mut self, base: &VectorPath, path: &VectorPath) -> Result<f64, String> {
        if !path.valid() || path.closed != base.closed || path.vertices.len() != base.vertices.len()
        {
            return Err("Animated paths require the same vertex count and open/closed state. Turn off Path animation before changing topology.".into());
        }
        if let Some(i) = self.poses.iter().position(|p| p == path) {
            return Ok(i as f64);
        }
        if let Some(i) = (0..self.poses.len()).find(|i| {
            self.timing.value != *i as f64
                && !self.timing.keys.values().any(|k| k.value == *i as f64)
        }) {
            self.poses[i] = path.clone();
            return Ok(i as f64);
        }
        if self.poses.len() >= 10000 || (self.poses.len() + 1) * path.vertices.len() > 200000 {
            return Err("Path animation geometry limit reached".into());
        }
        self.poses.push(path.clone());
        Ok((self.poses.len() - 1) as f64)
    }
    fn validate(&self, base: &VectorPath, duration: Frame, version: u32) -> Result<(), String> {
        if self.is_default() {
            return Ok(());
        }
        let valid_index = |v: f64| {
            v.is_finite() && v >= 0.0 && v.fract() == 0.0 && (v as usize) < self.poses.len()
        };
        if version < 31
            || self.poses.len() > 10000
            || self.poses.len() * base.vertices.len() > 200000
            || self.poses.iter().any(|p| {
                !p.valid() || p.closed != base.closed || p.vertices.len() != base.vertices.len()
            })
            || !valid_index(self.timing.value)
            || self.timing.keys.len() > 10000
            || self
                .timing
                .keys
                .iter()
                .any(|(f, k)| *f >= duration || !valid_index(k.value) || !k.interpolation.valid())
        {
            return Err("Invalid path animation or unsupported project version".into());
        }
        Ok(())
    }
}
impl Shape {
    pub fn path_at(&self, frame: Frame) -> Option<VectorPath> {
        self.path.as_ref().map(|p| self.path_animation.at(p, frame))
    }
    pub fn svg_at(&self, width: f64, height: f64, color: u32, frame: Frame) -> String {
        self.svg_with_path(width, height, color, self.path_at(frame).as_ref())
    }
}
impl PathMask {
    pub fn path_at(&self, frame: Frame) -> VectorPath {
        self.animation.at(&self.path, frame)
    }
}
impl Layer {
    pub fn path_animation(&self, target: PathTarget) -> Option<(&VectorPath, &PathAnimation)> {
        match target {
            PathTarget::Shape => match &self.content {
                Content::Shape(s) => Some((s.path.as_ref()?, &s.path_animation)),
                _ => None,
            },
            PathTarget::Mask(id) => self
                .path_masks
                .iter()
                .find(|m| m.id == id)
                .map(|m| (&m.path, &m.animation)),
        }
    }
    pub(super) fn path_animation_mut(
        &mut self,
        target: PathTarget,
    ) -> Option<(&mut VectorPath, &mut PathAnimation)> {
        match target {
            PathTarget::Shape => match &mut self.content {
                Content::Shape(s) => Some((s.path.as_mut()?, &mut s.path_animation)),
                _ => None,
            },
            PathTarget::Mask(id) => self
                .path_masks
                .iter_mut()
                .find(|m| m.id == id)
                .map(|m| (&mut m.path, &mut m.animation)),
        }
    }
    pub(super) fn copy_path_pose(&self, target: PathTarget, frame: Frame) -> Option<VectorPath> {
        let (base, animation) = self.path_animation(target)?;
        Some(animation.at(base, frame))
    }
    pub(super) fn paste_path_pose(
        &mut self,
        target: PathTarget,
        pose: &VectorPath,
    ) -> Result<f64, String> {
        let (base, animation) = self
            .path_animation_mut(target)
            .ok_or("Path no longer exists")?;
        if animation.is_default() {
            animation.intern(base, base)?;
        }
        animation.intern(base, pose)
    }
}
pub(super) fn validate(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    if let Content::Shape(shape) = &layer.content {
        if let Some(base) = &shape.path {
            shape.path_animation.validate(base, duration, version)?;
        } else if !shape.path_animation.is_default() {
            return Err("Path animation requires a vector path".into());
        }
    }
    for mask in &layer.path_masks {
        mask.animation.validate(&mask.path, duration, version)?;
    }
    Ok(())
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let (id, target) = match command {
        Command::EditPath { id, target, .. } | Command::AnimatePath { id, target, .. } => {
            (*id, *target)
        }
        _ => return None,
    };
    Some((|| {
        let duration = state.project.composition.duration;
        let layer = editing::editable(state, id)?;
        let (base, animation) = layer
            .path_animation_mut(target)
            .ok_or("Path no longer exists")?;
        if let Command::EditPath { frame, path, .. } = command {
            if *frame >= duration
                || !path.valid()
                || matches!(target, PathTarget::Mask(_)) && !path.closed
            {
                return Err("Invalid path or frame".into());
            }
            if animation.animated() {
                let value = animation.intern(base, path)?;
                let interpolation = animation
                    .timing
                    .keys
                    .get(frame)
                    .map_or(Interpolation::Linear, |k| k.interpolation);
                animation.timing.keys.insert(
                    *frame,
                    Keyframe {
                        value,
                        interpolation,
                    },
                );
            } else {
                *base = path.clone();
                *animation = PathAnimation::default();
            }
            return Ok(());
        }
        let Command::AnimatePath { edit, .. } = command else {
            unreachable!()
        };
        let frame = match *edit {
            TrackEdit::ToggleKey { frame }
            | TrackEdit::ToggleAnimation { frame }
            | TrackEdit::Interpolate { frame, .. } => frame,
            _ => {
                return Err(
                    "Edit path geometry with the Pen tool; move path keys in the timeline".into(),
                );
            }
        };
        if frame >= duration {
            return Err("Keyframe is outside the composition".into());
        }
        let pose = animation.at(base, frame);
        match *edit {
            TrackEdit::ToggleAnimation { .. } if animation.animated() => {
                *base = pose;
                *animation = PathAnimation::default();
            }
            TrackEdit::ToggleKey { .. } if animation.timing.keys.contains_key(&frame) => {
                animation.timing.keys.remove(&frame);
                if !animation.animated() {
                    *base = pose;
                    *animation = PathAnimation::default();
                }
            }
            TrackEdit::ToggleKey { .. } | TrackEdit::ToggleAnimation { .. } => {
                let value = animation.intern(base, &pose)?;
                animation.timing.value = value;
                animation.timing.keys.insert(
                    frame,
                    Keyframe {
                        value,
                        interpolation: Interpolation::Linear,
                    },
                );
            }
            TrackEdit::Interpolate { interpolation, .. } => {
                if !interpolation.valid() {
                    return Err("Invalid interpolation".into());
                }
                animation
                    .timing
                    .keys
                    .get_mut(&frame)
                    .ok_or("Add a path keyframe first")?
                    .interpolation = interpolation;
            }
            _ => unreachable!(),
        }
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(x: f64) -> VectorPath {
        VectorPath {
            closed: true,
            vertices: [[x, 0.0], [x + 20.0, 0.0], [x + 10.0, 20.0]]
                .map(PathVertex::corner)
                .to_vec(),
        }
    }
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(path(0.0)),
                ..Default::default()
            }),
            width: 100.0,
            height: 100.0,
            name: "Morph".into(),
        })
        .unwrap();
        e.execute(Command::AnimatePath {
            id: 1,
            target: PathTarget::Shape,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        e.execute(Command::EditPath {
            id: 1,
            target: PathTarget::Shape,
            frame: 20,
            path: path(40.0),
        })
        .unwrap();
        e
    }
    fn sampled(e: &Editor, id: LayerId, frame: Frame) -> VectorPath {
        e.project()
            .composition()
            .layer(id)
            .unwrap()
            .copy_path_pose(PathTarget::Shape, frame)
            .unwrap()
    }
    #[test]
    fn copied_path_and_mask_tracks_resample_time_without_resampling_pose_references() {
        let mut e = scene();
        e.execute(Command::SetPathMasks {
            id: 1,
            masks: vec![PathMask {
                path: path(0.0),
                ..Default::default()
            }],
        })
        .unwrap();
        e.execute(Command::AnimatePath {
            id: 1,
            target: PathTarget::Mask(1),
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        e.execute(Command::EditPath {
            id: 1,
            target: PathTarget::Mask(1),
            frame: 20,
            path: path(40.0),
        })
        .unwrap();
        let feather = PropertyPath::Mask {
            mask: 1,
            parameter: MaskParam::Feather,
        };
        e.execute(Command::EditTrack {
            id: 1,
            property: feather,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        e.execute(Command::EditTrack {
            id: 1,
            property: feather,
            edit: TrackEdit::Value {
                frame: 20,
                value: 40.0,
            },
        })
        .unwrap();
        let clip = e.copy_layers(&[1]).unwrap();
        e.execute(Command::NewComposition).unwrap();
        e.execute(Command::ConfigureComposition {
            name: "60fps".into(),
            width: 100,
            height: 100,
            fps: 60,
            duration: 300,
        })
        .unwrap();
        e.execute(Command::PasteLayers(clip)).unwrap();
        let layer = e.selected_layer().unwrap();
        assert_eq!(
            layer.copy_path_pose(PathTarget::Shape, 20),
            Some(path(20.0))
        );
        assert_eq!(layer.path_masks()[0].path_at(20), path(20.0));
        assert_eq!(layer.track_value(feather, 20), Some(20.0));
        assert!(
            layer
                .track(PropertyPath::Path(PathTarget::Shape))
                .unwrap()
                .keys()
                .contains_key(&40)
        );
        assert_eq!(
            Project::from_json(&e.project().to_json().unwrap()).unwrap(),
            *e.project()
        );
    }
    #[test]
    fn path_geometry_interpolates_handles_eases_and_bakes_without_exposing_pose_indices() {
        let mut e = scene();
        assert_eq!(sampled(&e, 1, 10), path(20.0));
        let mut curved = path(40.0);
        curved.vertices[0].outgoing = [10.0, 20.0];
        e.execute(Command::EditPath {
            id: 1,
            target: PathTarget::Shape,
            frame: 20,
            path: curved,
        })
        .unwrap();
        assert_eq!(sampled(&e, 1, 10).vertices[0].outgoing, [5.0, 10.0]);
        let prop = PropertyPath::Path(PathTarget::Shape);
        assert_eq!(e.selected_layer().unwrap().track_value(prop, 10), None);
        for (interpolation, x) in [
            (Interpolation::Hold, 0.0),
            (Interpolation::Smooth, 6.25),
            (Interpolation::Linear, 10.0),
        ] {
            e.execute(Command::EditTrack {
                id: 1,
                property: prop,
                edit: TrackEdit::Interpolate {
                    frame: 0,
                    interpolation,
                },
            })
            .unwrap();
            assert!((sampled(&e, 1, 5).vertices[0].position[0] - x).abs() < 1e-9);
        }
        let before = e.project().clone();
        let baked = sampled(&e, 1, 10);
        e.execute(Command::EditTrack {
            id: 1,
            property: prop,
            edit: TrackEdit::ToggleAnimation { frame: 10 },
        })
        .unwrap();
        assert_eq!(sampled(&e, 1, 90), baked);
        assert!(
            e.selected_layer()
                .unwrap()
                .track(prop)
                .unwrap()
                .keys()
                .is_empty()
        );
        e.undo();
        assert_eq!(*e.project(), before);
    }
    #[test]
    fn path_keys_move_copy_between_layers_shift_and_roundtrip_with_geometry() {
        let mut e = scene();
        let prop = PropertyPath::Path(PathTarget::Shape);
        let key = KeyRef {
            id: 1,
            property: prop,
            frame: 20,
        };
        e.execute(Command::MoveKeys {
            keys: vec![key],
            delta: 20,
        })
        .unwrap();
        assert_eq!(sampled(&e, 1, 20), path(20.0));
        let copied = e.selected_layer().unwrap().copy_key(prop, 40).unwrap();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                path: Some(path(80.0)),
                ..Default::default()
            }),
            width: 100.0,
            height: 100.0,
            name: "Destination".into(),
        })
        .unwrap();
        e.execute(Command::PasteKeys {
            keys: vec![copied],
            frame: 60,
            target: Some(2),
        })
        .unwrap();
        assert_eq!(sampled(&e, 2, 60), path(40.0));
        e.execute(Command::EditPath {
            id: 2,
            target: PathTarget::Shape,
            frame: 80,
            path: path(0.0),
        })
        .unwrap();
        // Pose references run backwards here; interpolation must still move 40 -> 0.
        assert_eq!(sampled(&e, 2, 70), path(20.0));
        e.execute(Command::SetLayerRange {
            id: 2,
            start: 0,
            end: 100,
        })
        .unwrap();
        e.execute(Command::ShiftLayer { id: 2, delta: 5 }).unwrap();
        assert_eq!(sampled(&e, 2, 75), path(20.0));
        let saved = e.project().clone();
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        e.execute(Command::DeleteKeys(vec![
            KeyRef {
                id: 2,
                property: prop,
                frame: 65,
            },
            KeyRef {
                id: 2,
                property: prop,
                frame: 85,
            },
        ]))
        .unwrap();
        assert_eq!(sampled(&e, 2, 100), path(0.0));
        e.undo();
        assert_eq!(*e.project(), saved);
    }
    #[test]
    fn mask_animation_tracks_identity_and_rejects_topology_numeric_edits_and_invalid_files() {
        let mut e = scene();
        e.execute(Command::SetPathMasks {
            id: 1,
            masks: vec![
                PathMask {
                    path: path(0.0),
                    ..Default::default()
                },
                PathMask {
                    path: path(10.0),
                    ..Default::default()
                },
            ],
        })
        .unwrap();
        let target = PathTarget::Mask(1);
        e.execute(Command::AnimatePath {
            id: 1,
            target,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        e.execute(Command::EditPath {
            id: 1,
            target,
            frame: 20,
            path: path(40.0),
        })
        .unwrap();
        let mut masks = e.selected_layer().unwrap().path_masks().to_vec();
        masks.reverse();
        e.execute(Command::SetPathMasks { id: 1, masks }).unwrap();
        assert_eq!(
            e.selected_layer().unwrap().path_masks()[1].path_at(10),
            path(20.0)
        );
        let before = e.project().clone();
        let mut changed = path(40.0);
        changed.insert(0, 0.5);
        for cmd in [
            Command::EditPath {
                id: 1,
                target,
                frame: 20,
                path: changed,
            },
            Command::EditPath {
                id: 1,
                target,
                frame: 150,
                path: path(0.0),
            },
            Command::EditTrack {
                id: 1,
                property: PropertyPath::Path(target),
                edit: TrackEdit::Value {
                    frame: 10,
                    value: 0.5,
                },
            },
        ] {
            assert!(e.execute(cmd).is_err());
            assert_eq!(*e.project(), before);
        }
        let mut bad = before.clone();
        bad.composition.layers[0].path_masks[1]
            .animation
            .timing
            .keys
            .get_mut(&0)
            .unwrap()
            .value = 99.5;
        assert!(Project::from_json(&bad.to_json().unwrap()).is_err());
        let json = before
            .to_json()
            .unwrap()
            .replace("\"version\": 31", "\"version\": 30");
        assert!(Project::from_json(&json).is_err());
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(
            e.execute(Command::EditPath {
                id: 1,
                target,
                frame: 20,
                path: path(20.0)
            })
            .is_err()
        );
    }
}
