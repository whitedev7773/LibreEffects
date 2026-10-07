//! Atomic selection edits in composition bounds or each root's local transform space.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignTarget {
    Selection,
    Composition,
}

impl Composition {
    pub(super) fn require_two_d_transform(&self, id: LayerId) -> Result<(), String> {
        if self
            .layer(id)
            .is_some_and(|layer| layer.planar_position().is_some())
        {
            return Err("Joined XY Position requires explicit vector transform edits".into());
        }
        let mut current = Some(id);
        for _ in 0..=self.layers.len() {
            let Some(id) = current else {
                return Ok(());
            };
            let layer = self.layer(id).ok_or("Layer not found")?;
            if layer.is_three_d() {
                return Err("3D layers and their descendants do not support 2D transforms".into());
            }
            current = layer.parent;
        }
        Err("Invalid parent hierarchy".into())
    }
    /// Selected ancestors carry their selected descendants. Validate the whole
    /// selection before changing anything, including locked descendants.
    pub fn selection_roots(&self, ids: &[LayerId]) -> Result<Vec<LayerId>, String> {
        let ids: BTreeSet<_> = ids.iter().copied().collect();
        if ids.is_empty() {
            return Err("Select at least one layer".into());
        }
        for id in &ids {
            let layer = self.layer(*id).ok_or("Selected layer no longer exists")?;
            if layer.locked {
                return Err("Unlock all selected layers before transforming".into());
            }
        }
        let parents: BTreeMap<_, _> = self.layers.iter().map(|l| (l.id, l.parent)).collect();
        Ok(self
            .layers
            .iter()
            .filter(|l| {
                if !ids.contains(&l.id) {
                    return false;
                }
                let mut parent = l.parent;
                for _ in 0..self.layers.len() {
                    let Some(id) = parent else {
                        return true;
                    };
                    if ids.contains(&id) {
                        return false;
                    }
                    parent = parents.get(&id).copied().flatten();
                }
                false
            })
            .map(|l| l.id)
            .collect())
    }
    /// Transformed source bounds, excluding masks/effect expansion.
    pub fn layer_bounds(&self, id: LayerId, frame: Frame) -> Option<[f64; 4]> {
        let points = self.corners_at(id, frame)?;
        Some([
            points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
            points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
            points
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max),
            points
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max),
        ])
    }
}

fn coordinate(bounds: [f64; 4], alignment: Alignment) -> (usize, f64) {
    let [left, top, right, bottom] = bounds;
    match alignment {
        Alignment::Left => (0, left),
        Alignment::HorizontalCenter => (0, (left + right) * 0.5),
        Alignment::Right => (0, right),
        Alignment::Top => (1, top),
        Alignment::VerticalCenter => (1, (top + bottom) * 0.5),
        Alignment::Bottom => (1, bottom),
    }
}

fn translation(
    comp: &Composition,
    id: LayerId,
    frame: Frame,
    delta: [f64; 2],
) -> Result<Option<Command>, String> {
    if delta.iter().all(|d| d.abs() < 1e-9) {
        return Ok(None);
    }
    let layer = comp.layer(id).ok_or("Layer not found")?;
    let local = comp
        .position_space(id, frame)
        .and_then(Affine::inverse)
        .ok_or("Cannot align through a zero-scale parent")?
        .vector(delta);
    Ok(Some(Command::SetPosition {
        id,
        frame,
        x: layer
            .property(Property::PositionX)
            .ok_or("Scalar transform property is unavailable")?
            .value_at(frame)
            + local[0],
        y: layer
            .property(Property::PositionY)
            .ok_or("Scalar transform property is unavailable")?
            .value_at(frame)
            + local[1],
    }))
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let (ids, frame) = match command {
        Command::AlignLayers { ids, frame, .. }
        | Command::DistributeLayers { ids, frame, .. }
        | Command::RotateLayers { ids, frame, .. }
        | Command::ScaleLayers { ids, frame, .. } => (ids, *frame),
        _ => return None,
    };
    Some((|| {
        let comp = &state.project.composition;
        if frame >= comp.duration {
            return Err("Transform frame must be inside the composition".into());
        }
        let roots = comp.selection_roots(ids)?;
        for id in ids {
            comp.require_two_d_transform(*id)?;
        }
        let mut commands = Vec::new();
        match command {
            Command::AlignLayers { alignment, .. }
            | Command::DistributeLayers { alignment, .. } => {
                let mut bounds = roots
                    .iter()
                    .map(|id| {
                        comp.layer_bounds(*id, frame)
                            .map(|bounds| (*id, bounds))
                            .ok_or_else(|| "Invalid layer transform".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if let Command::AlignLayers { target, .. } = command {
                    let target = match target {
                        AlignTarget::Composition => {
                            [0.0, 0.0, comp.width as f64, comp.height as f64]
                        }
                        AlignTarget::Selection => {
                            if roots.len() < 2 {
                                return Err(
                                    "Select at least two independent layers to align to selection"
                                        .into(),
                                );
                            }
                            bounds.iter().fold(
                                [
                                    f64::INFINITY,
                                    f64::INFINITY,
                                    f64::NEG_INFINITY,
                                    f64::NEG_INFINITY,
                                ],
                                |a, (_, b)| {
                                    [
                                        a[0].min(b[0]),
                                        a[1].min(b[1]),
                                        a[2].max(b[2]),
                                        a[3].max(b[3]),
                                    ]
                                },
                            )
                        }
                    };
                    let (axis, goal) = coordinate(target, *alignment);
                    for (id, bounds) in bounds {
                        let mut delta = [0.0; 2];
                        delta[axis] = goal - coordinate(bounds, *alignment).1;
                        if let Some(c) = translation(comp, id, frame, delta)? {
                            commands.push(c);
                        }
                    }
                } else {
                    if roots.len() < 3 {
                        return Err("Select at least three independent layers to distribute".into());
                    }
                    bounds.sort_by(|a, b| {
                        coordinate(a.1, *alignment)
                            .1
                            .total_cmp(&coordinate(b.1, *alignment).1)
                            .then(a.0.cmp(&b.0))
                    });
                    let (axis, start) = coordinate(bounds[0].1, *alignment);
                    let end = coordinate(bounds.last().unwrap().1, *alignment).1;
                    let step = (end - start) / (bounds.len() - 1) as f64;
                    for (i, (id, bounds)) in bounds.into_iter().enumerate() {
                        let mut delta = [0.0; 2];
                        delta[axis] = start + step * i as f64 - coordinate(bounds, *alignment).1;
                        if let Some(c) = translation(comp, id, frame, delta)? {
                            commands.push(c);
                        }
                    }
                }
            }
            Command::RotateLayers { degrees, .. } => {
                if !degrees.is_finite() {
                    return Err("Rotation delta must be finite".into());
                }
                if degrees.abs() < 1e-12 {
                    return Ok(());
                }
                for id in roots {
                    commands.push(Command::SetValue {
                        id,
                        property: Property::Rotation,
                        frame,
                        value: comp
                            .layer(id)
                            .unwrap()
                            .property(Property::Rotation)
                            .ok_or("Scalar transform property is unavailable")?
                            .value_at(frame)
                            + degrees,
                    });
                }
            }
            Command::ScaleLayers { factor, offset, .. } => {
                if factor.iter().chain(offset).any(|v| !v.is_finite()) {
                    return Err("Scale changes must be finite".into());
                }
                for id in roots {
                    for (axis, property) in
                        [Property::ScaleX, Property::ScaleY].into_iter().enumerate()
                    {
                        if (factor[axis] - 1.0).abs() < 1e-12 && offset[axis].abs() < 1e-12 {
                            continue;
                        }
                        commands.push(Command::SetValue {
                            id,
                            property,
                            frame,
                            value: comp
                                .layer(id)
                                .unwrap()
                                .property(property)
                                .ok_or("Scalar transform property is unavailable")?
                                .value_at(frame)
                                * factor[axis]
                                + offset[axis],
                        });
                    }
                }
            }
            _ => unreachable!(),
        }
        for c in commands {
            super::apply(state, c)?;
        }
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Editor {
        let mut e = Editor::default();
        for (width, x, y) in [
            (20.0, 100.0, 100.0),
            (40.0, 260.0, 170.0),
            (60.0, 500.0, 300.0),
        ] {
            e.execute(Command::AddContent {
                content: Content::Rectangle,
                width,
                height: 40.0,
                name: "Box".into(),
            })
            .unwrap();
            e.execute(Command::SetPosition {
                id: e.selected().unwrap(),
                frame: 0,
                x,
                y,
            })
            .unwrap();
        }
        e
    }
    fn value(e: &Editor, id: LayerId, p: Property, f: Frame) -> f64 {
        e.project()
            .composition()
            .layer(id)
            .unwrap()
            .property(p)
            .unwrap()
            .value_at(f)
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-7, "{a} != {b}");
    }
    #[test]
    fn alignment_and_six_distributions_use_transformed_bounds_and_fixed_extremes() {
        for alignment in [
            Alignment::Left,
            Alignment::HorizontalCenter,
            Alignment::Right,
            Alignment::Top,
            Alignment::VerticalCenter,
            Alignment::Bottom,
        ] {
            let mut e = scene();
            for (id, p, v) in [
                (1, Property::Rotation, 31.0),
                (2, Property::ScaleX, -175.0),
                (3, Property::Rotation, -48.0),
            ] {
                e.execute(Command::SetValue {
                    id,
                    property: p,
                    frame: 0,
                    value: v,
                })
                .unwrap();
            }
            let before = e.project().clone();
            let mut coordinates: Vec<_> = (1..=3)
                .map(|id| {
                    (
                        id,
                        coordinate(before.composition().layer_bounds(id, 0).unwrap(), alignment).1,
                    )
                })
                .collect();
            coordinates.sort_by(|a, b| a.1.total_cmp(&b.1));
            e.execute(Command::DistributeLayers {
                ids: vec![3, 1, 2, 1],
                frame: 0,
                alignment,
            })
            .unwrap();
            for (i, (id, _)) in coordinates.iter().enumerate() {
                close(
                    coordinate(
                        e.project().composition().layer_bounds(*id, 0).unwrap(),
                        alignment,
                    )
                    .1,
                    coordinates[0].1 + (coordinates[2].1 - coordinates[0].1) * i as f64 / 2.0,
                );
            }
            let distributed = e.project().clone();
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &distributed);
            e.execute(Command::AlignLayers {
                ids: vec![1, 2, 3],
                frame: 0,
                alignment,
                target: AlignTarget::Selection,
            })
            .unwrap();
            let first = coordinate(
                e.project().composition().layer_bounds(1, 0).unwrap(),
                alignment,
            )
            .1;
            for id in [2, 3] {
                close(
                    coordinate(
                        e.project().composition().layer_bounds(id, 0).unwrap(),
                        alignment,
                    )
                    .1,
                    first,
                );
            }
            e.execute(Command::AlignLayers {
                ids: vec![1, 2, 3],
                frame: 0,
                alignment,
                target: AlignTarget::Composition,
            })
            .unwrap();
            let comp = e.project().composition();
            let goal = coordinate(
                [0.0, 0.0, comp.width() as f64, comp.height() as f64],
                alignment,
            )
            .1;
            for id in [1, 2, 3] {
                close(
                    coordinate(comp.layer_bounds(id, 0).unwrap(), alignment).1,
                    goal,
                );
            }
            assert_eq!(
                Project::from_json(&e.project().to_json().unwrap()).unwrap(),
                *e.project()
            );
        }
    }
    #[test]
    fn parent_child_selection_moves_rotates_and_scales_once_including_negative_scale() {
        let mut e = scene();
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        let before = e.project().clone();
        assert_eq!(
            before.composition().selection_roots(&[1, 2, 3]).unwrap(),
            vec![3, 1]
        );
        e.execute(Command::AlignLayers {
            ids: vec![1, 2, 3],
            frame: 0,
            alignment: Alignment::Left,
            target: AlignTarget::Composition,
        })
        .unwrap();
        let comp = e.project().composition();
        close(
            comp.layer_bounds(2, 0).unwrap()[0]
                - before.composition().layer_bounds(2, 0).unwrap()[0],
            comp.layer_bounds(1, 0).unwrap()[0]
                - before.composition().layer_bounds(1, 0).unwrap()[0],
        );
        assert_eq!(
            comp.layer(2).unwrap().properties,
            before.composition().layer(2).unwrap().properties
        );
        let anchors: Vec<_> = [1, 3]
            .map(|id| {
                let l = comp.layer(id).unwrap();
                comp.world_transform(id, 0).unwrap().point([
                    l.property(Property::AnchorX).unwrap().value_at(0),
                    l.property(Property::AnchorY).unwrap().value_at(0),
                ])
            })
            .into();
        e.execute(Command::RotateLayers {
            ids: vec![1, 2, 3],
            frame: 0,
            degrees: 33.0,
        })
        .unwrap();
        e.execute(Command::ScaleLayers {
            ids: vec![1, 2, 3],
            frame: 0,
            factor: [-1.5, 0.5],
            offset: [0.0; 2],
        })
        .unwrap();
        for (i, id) in [1, 3].into_iter().enumerate() {
            close(value(&e, id, Property::Rotation, 0), 33.0);
            close(value(&e, id, Property::ScaleX, 0), -150.0);
            close(value(&e, id, Property::ScaleY, 0), 50.0);
            let l = e.project().composition().layer(id).unwrap();
            let anchor = e
                .project()
                .composition()
                .world_transform(id, 0)
                .unwrap()
                .point([
                    l.property(Property::AnchorX).unwrap().value_at(0),
                    l.property(Property::AnchorY).unwrap().value_at(0),
                ]);
            close(anchor[0], anchors[i][0]);
            close(anchor[1], anchors[i][1]);
        }
        close(value(&e, 2, Property::Rotation, 0), 0.0);
        close(value(&e, 2, Property::ScaleX, 0), 100.0);
    }
    #[test]
    fn animated_selection_edits_add_current_keys_and_reject_invalid_selections_atomically() {
        let mut e = scene();
        for id in [1, 2, 3] {
            for property in [Property::PositionX, Property::Rotation, Property::ScaleX] {
                e.execute(Command::ToggleAnimation {
                    id,
                    property,
                    frame: 0,
                })
                .unwrap();
            }
        }
        let before = e.project().clone();
        e.execute(Command::AlignLayers {
            ids: vec![1, 2, 3],
            frame: 20,
            alignment: Alignment::Right,
            target: AlignTarget::Composition,
        })
        .unwrap();
        e.execute(Command::RotateLayers {
            ids: vec![1, 2, 3],
            frame: 20,
            degrees: 90.0,
        })
        .unwrap();
        e.execute(Command::ScaleLayers {
            ids: vec![1, 2, 3],
            frame: 20,
            factor: [2.0, 1.0],
            offset: [0.0; 2],
        })
        .unwrap();
        for id in [1, 2, 3] {
            close(value(&e, id, Property::Rotation, 10), 45.0);
            close(value(&e, id, Property::ScaleX, 10), 150.0);
            close(
                value(&e, id, Property::PositionX, 0),
                before
                    .composition()
                    .layer(id)
                    .unwrap()
                    .property(Property::PositionX)
                    .unwrap()
                    .value_at(0),
            );
        }
        let saved = e.project().clone();
        e.undo();
        e.redo();
        assert_eq!(e.project(), &saved);
        assert_eq!(
            Project::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        e.execute(Command::ToggleLocked(2)).unwrap();
        let locked = e.project().clone();
        for command in [
            Command::AlignLayers {
                ids: vec![1, 2, 3],
                frame: 20,
                alignment: Alignment::Left,
                target: AlignTarget::Selection,
            },
            Command::DistributeLayers {
                ids: vec![1, 2, 3],
                frame: 20,
                alignment: Alignment::Left,
            },
            Command::RotateLayers {
                ids: vec![1, 2, 3],
                frame: 20,
                degrees: 10.0,
            },
            Command::ScaleLayers {
                ids: vec![1, 2, 3],
                frame: 20,
                factor: [2.0; 2],
                offset: [0.0; 2],
            },
            Command::RotateLayers {
                ids: vec![1, 99],
                frame: 20,
                degrees: 10.0,
            },
            Command::ScaleLayers {
                ids: vec![1, 3],
                frame: 20,
                factor: [f64::NAN, 2.0],
                offset: [0.0; 2],
            },
            Command::DistributeLayers {
                ids: vec![1, 3],
                frame: 20,
                alignment: Alignment::Left,
            },
            Command::RotateLayers {
                ids: vec![1, 3],
                frame: 150,
                degrees: 10.0,
            },
        ] {
            assert!(e.execute(command).is_err());
            assert_eq!(e.project(), &locked);
        }
    }
    #[test]
    fn alignment_respects_transformed_parent_space_and_refuses_singular_parent() {
        let mut e = scene();
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        for (p, v) in [
            (Property::Rotation, 38.0),
            (Property::ScaleX, -140.0),
            (Property::ScaleY, 65.0),
        ] {
            e.execute(Command::SetValue {
                id: 1,
                property: p,
                frame: 0,
                value: v,
            })
            .unwrap();
        }
        e.execute(Command::AlignLayers {
            ids: vec![2, 3],
            frame: 0,
            alignment: Alignment::HorizontalCenter,
            target: AlignTarget::Composition,
        })
        .unwrap();
        for id in [2, 3] {
            let b = e.project().composition().layer_bounds(id, 0).unwrap();
            close((b[0] + b[2]) / 2.0, 960.0);
        }
        e.execute(Command::SetValue {
            id: 1,
            property: Property::ScaleY,
            frame: 0,
            value: 0.0,
        })
        .unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::AlignLayers {
                ids: vec![2, 3],
                frame: 0,
                alignment: Alignment::Top,
                target: AlignTarget::Composition
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }
}
