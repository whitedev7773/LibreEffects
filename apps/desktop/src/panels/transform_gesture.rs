use libre_effects_core::{Affine, Command, Composition, LayerId, Property};

#[derive(Clone)]
pub(super) enum TransformGesture {
    Selection {
        driver: Box<TransformGesture>,
        ids: Vec<LayerId>,
        rotation: f64,
        scale: [f64; 2],
    },
    Scale {
        id: LayerId,
        inverse: Affine,
        arms: [f64; 2],
        scale: [f64; 2],
        axes: [bool; 2],
    },
    Rotate {
        id: LayerId,
        inverse: Affine,
        center: [f64; 2],
        previous: [f64; 2],
        rotation: f64,
        angle: f64,
    },
    Anchor {
        id: LayerId,
        inverse: Affine,
        anchor: [f64; 2],
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::Editor;
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        for (id, property, value) in [
            (1, Property::Rotation, 35.0),
            (1, Property::ScaleX, 160.0),
            (1, Property::ScaleY, 60.0),
            (2, Property::Rotation, 28.0),
        ] {
            e.execute(Command::SetValue {
                id,
                property,
                frame: 0,
                value,
            })
            .unwrap();
        }
        e
    }
    fn value(e: &Editor, p: Property) -> f64 {
        e.project()
            .composition()
            .layer(2)
            .unwrap()
            .property(p)
            .value_at(0)
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-7, "{a} != {b}");
    }
    #[test]
    fn scale_handle_tracks_pointer_under_rotated_nonuniform_parent() {
        let mut e = scene();
        let comp = e.project().composition();
        let before = comp.corners_at(2, 0).unwrap()[2];
        let anchor = comp.world_transform(2, 0).unwrap().point([160.0, 100.0]);
        let g = TransformGesture::scale(comp, 2, 0, 4).unwrap();
        let delta = [63.0, -27.0];
        e.execute(g.command(0, delta, false)).unwrap();
        let comp = e.project().composition();
        let after = comp.corners_at(2, 0).unwrap()[2];
        for i in 0..2 {
            close(after[i], before[i] + delta[i]);
            close(
                comp.world_transform(2, 0).unwrap().point([160.0, 100.0])[i],
                anchor[i],
            );
        }
        e.undo();
        e.execute(g.command(0, delta, true)).unwrap();
        close(value(&e, Property::ScaleX), value(&e, Property::ScaleY));
    }
    #[test]
    fn side_handle_changes_one_axis_and_survives_zero_scale() {
        let mut e = scene();
        let g = TransformGesture::scale(e.project().composition(), 2, 0, 3).unwrap();
        e.execute(g.command(0, [30.0, 40.0], false)).unwrap();
        close(value(&e, Property::ScaleY), 100.0);
        e.execute(Command::SetValue {
            id: 2,
            property: Property::ScaleX,
            frame: 0,
            value: 0.0,
        })
        .unwrap();
        let g = TransformGesture::scale(e.project().composition(), 2, 0, 3).unwrap();
        e.execute(g.command(0, [30.0, 40.0], false)).unwrap();
        assert!(value(&e, Property::ScaleX).abs() > 1.0);
    }
    #[test]
    fn rotation_tracks_multiple_turns_and_snaps_in_parent_space() {
        let mut e = scene();
        let comp = e.project().composition();
        let l = comp.layer(2).unwrap();
        let center = [
            l.property(Property::PositionX).value_at(0),
            l.property(Property::PositionY).value_at(0),
        ];
        let space = comp.position_space(2, 0).unwrap();
        let pointer = |angle: f64| {
            let (sin, cos) = angle.to_radians().sin_cos();
            space.point([center[0] + 100.0 * cos, center[1] + 100.0 * sin])
        };
        let mut g = TransformGesture::rotate(comp, 2, 0, pointer(0.0)).unwrap();
        for a in [90.0, 180.0, 270.0, 360.0, 450.0, 467.0] {
            g.update(pointer(a));
        }
        e.execute(g.command(0, [0.0, 0.0], false)).unwrap();
        close(value(&e, Property::Rotation), 495.0);
        g.update(pointer(471.0));
        e.execute(g.command(0, [0.0, 0.0], true)).unwrap();
        close(value(&e, Property::Rotation), 495.0);
    }
    #[test]
    fn anchor_follows_pointer_without_moving_parented_content() {
        let mut e = scene();
        let comp = e.project().composition();
        let before = comp.corners_at(2, 0).unwrap();
        let anchor = comp.world_transform(2, 0).unwrap().point([160.0, 100.0]);
        let g = TransformGesture::anchor(comp, 2, 0).unwrap();
        e.execute(g.command(0, [40.0, -25.0], false)).unwrap();
        let comp = e.project().composition();
        for (a, b) in before
            .into_iter()
            .flatten()
            .zip(comp.corners_at(2, 0).unwrap().into_iter().flatten())
        {
            close(a, b);
        }
        let new_anchor = comp
            .world_transform(2, 0)
            .unwrap()
            .point([value(&e, Property::AnchorX), value(&e, Property::AnchorY)]);
        close(new_anchor[0], anchor[0] + 40.0);
        close(new_anchor[1], anchor[1] - 25.0);
    }
    #[test]
    fn selection_rotation_and_negative_scale_preserve_relative_values_and_child_local_pose() {
        let mut e = scene();
        e.execute(Command::AddRectangle).unwrap();
        for (property, value) in [
            (Property::ScaleX, -60.0),
            (Property::ScaleY, 140.0),
            (Property::Rotation, -10.0),
        ] {
            e.execute(Command::SetValue {
                id: 3,
                property,
                frame: 0,
                value,
            })
            .unwrap();
        }
        let comp = e.project().composition();
        let center = [960.0, 540.0];
        let mut rotate = TransformGesture::rotate(comp, 1, 0, [center[0] + 100.0, center[1]])
            .unwrap()
            .with_selection(comp, vec![1, 2, 3], 0)
            .unwrap();
        rotate.update([center[0], center[1] + 100.0]);
        e.execute(rotate.command(0, [0.0; 2], true)).unwrap();
        close(
            e.project()
                .composition()
                .layer(1)
                .unwrap()
                .property(Property::Rotation)
                .value_at(0),
            120.0,
        );
        close(
            e.project()
                .composition()
                .layer(3)
                .unwrap()
                .property(Property::Rotation)
                .value_at(0),
            75.0,
        );
        close(value(&e, Property::Rotation), 28.0);
        let comp = e.project().composition();
        let before = e.project().clone();
        let driver = TransformGesture::scale(comp, 1, 0, 4)
            .unwrap()
            .with_selection(comp, vec![1, 2, 3], 0)
            .unwrap();
        let corner = comp.corners_at(1, 0).unwrap()[2];
        e.execute(Command::ScaleLayers {
            ids: vec![1],
            frame: 0,
            factor: [-0.5, 2.0],
            offset: [0.0; 2],
        })
        .unwrap();
        let goal = e.project().composition().corners_at(1, 0).unwrap()[2];
        e.undo();
        e.execute(driver.command(0, [goal[0] - corner[0], goal[1] - corner[1]], false))
            .unwrap();
        close(
            e.project()
                .composition()
                .layer(1)
                .unwrap()
                .property(Property::ScaleX)
                .value_at(0),
            -80.0,
        );
        close(
            e.project()
                .composition()
                .layer(3)
                .unwrap()
                .property(Property::ScaleX)
                .value_at(0),
            30.0,
        );
        close(
            e.project()
                .composition()
                .layer(3)
                .unwrap()
                .property(Property::ScaleY)
                .value_at(0),
            280.0,
        );
        close(value(&e, Property::ScaleX), 100.0);
        let changed = e.project().clone();
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &changed);
        let saved = libre_effects_core::Project::from_json(&changed.to_json().unwrap()).unwrap();
        let renderer = crate::rendering::Renderer::new();
        assert_eq!(
            renderer.render(&saved, 0, 200).unwrap(),
            renderer.render(&changed, 0, 200).unwrap()
        );
    }
    #[test]
    fn selection_scale_can_leave_zero_without_nan_and_rejects_locked_members() {
        let mut e = libre_effects_core::Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::SetValue {
            id: 1,
            property: Property::ScaleX,
            frame: 0,
            value: 0.0,
        })
        .unwrap();
        let comp = e.project().composition();
        let gesture = TransformGesture::scale(comp, 1, 0, 3)
            .unwrap()
            .with_selection(comp, vec![1, 2], 0)
            .unwrap();
        e.execute(gesture.command(0, [16.0, 0.0], false)).unwrap();
        close(
            e.project()
                .composition()
                .layer(1)
                .unwrap()
                .property(Property::ScaleX)
                .value_at(0),
            10.0,
        );
        close(value(&e, Property::ScaleX), 110.0);
        close(value(&e, Property::ScaleY), 100.0);
        e.execute(Command::ToggleLocked(2)).unwrap();
        assert!(
            TransformGesture::scale(e.project().composition(), 1, 0, 3)
                .unwrap()
                .with_selection(e.project().composition(), vec![1, 2], 0)
                .is_err()
        );
    }
}
pub(super) fn handles(width: f64, height: f64) -> [[f64; 2]; 8] {
    [
        [0.0, 0.0],
        [width / 2.0, 0.0],
        [width, 0.0],
        [width, height / 2.0],
        [width, height],
        [width / 2.0, height],
        [0.0, height],
        [0.0, height / 2.0],
    ]
}
impl TransformGesture {
    pub fn with_selection(
        self,
        comp: &Composition,
        ids: Vec<LayerId>,
        frame: u32,
    ) -> Result<Self, String> {
        if ids.len() <= 1 || matches!(self, Self::Anchor { .. }) {
            return Ok(self);
        }
        let id = match &self {
            Self::Scale { id, .. } | Self::Rotate { id, .. } => *id,
            _ => return Ok(self),
        };
        comp.selection_roots(&ids)?;
        let layer = comp.layer(id).ok_or("Layer not found")?;
        Ok(Self::Selection {
            rotation: layer.property(Property::Rotation).value_at(frame),
            scale: [
                layer.property(Property::ScaleX).value_at(frame),
                layer.property(Property::ScaleY).value_at(frame),
            ],
            driver: Box::new(self),
            ids,
        })
    }
    pub fn scale(comp: &Composition, id: LayerId, frame: u32, handle: usize) -> Option<Self> {
        let l = comp.layer(id)?;
        let anchor = [
            l.property(Property::AnchorX).value_at(frame),
            l.property(Property::AnchorY).value_at(frame),
        ];
        let h = handles(l.width(), l.height())[handle];
        let angle = -l.property(Property::Rotation).value_at(frame).to_radians();
        let (sin, cos) = angle.sin_cos();
        let inverse = Affine([cos, sin, -sin, cos, 0.0, 0.0])
            .compose(comp.position_space(id, frame)?.inverse()?);
        Some(Self::Scale {
            id,
            inverse,
            arms: [h[0] - anchor[0], h[1] - anchor[1]],
            scale: [
                l.property(Property::ScaleX).value_at(frame),
                l.property(Property::ScaleY).value_at(frame),
            ],
            axes: [!matches!(handle, 1 | 5), !matches!(handle, 3 | 7)],
        })
    }
    pub fn rotate(comp: &Composition, id: LayerId, frame: u32, pointer: [f64; 2]) -> Option<Self> {
        let l = comp.layer(id)?;
        let inverse = comp.position_space(id, frame)?.inverse()?;
        let center = [
            l.property(Property::PositionX).value_at(frame),
            l.property(Property::PositionY).value_at(frame),
        ];
        let p = inverse.point(pointer);
        Some(Self::Rotate {
            id,
            inverse,
            center,
            previous: [p[0] - center[0], p[1] - center[1]],
            rotation: l.property(Property::Rotation).value_at(frame),
            angle: 0.0,
        })
    }
    pub fn anchor(comp: &Composition, id: LayerId, frame: u32) -> Option<Self> {
        let l = comp.layer(id)?;
        Some(Self::Anchor {
            id,
            inverse: comp.world_transform(id, frame)?.inverse()?,
            anchor: [
                l.property(Property::AnchorX).value_at(frame),
                l.property(Property::AnchorY).value_at(frame),
            ],
        })
    }
    pub fn update(&mut self, pointer: [f64; 2]) {
        if let Self::Selection { driver, .. } = self {
            driver.update(pointer);
            return;
        }
        if let Self::Rotate {
            inverse,
            center,
            previous,
            angle,
            ..
        } = self
        {
            let p = inverse.point(pointer);
            let next = [p[0] - center[0], p[1] - center[1]];
            if next[0].hypot(next[1]) > 0.001 && previous[0].hypot(previous[1]) > 0.001 {
                *angle += (previous[0] * next[1] - previous[1] * next[0])
                    .atan2(previous[0] * next[0] + previous[1] * next[1])
                    .to_degrees();
            }
            *previous = next;
        }
    }
    pub fn command(&self, frame: u32, delta: [f64; 2], constrained: bool) -> Command {
        match self {
            Self::Selection {
                driver,
                ids,
                rotation,
                scale,
            } => match driver.command(frame, delta, constrained) {
                Command::SetValue {
                    property: Property::Rotation,
                    value,
                    ..
                } => Command::RotateLayers {
                    ids: ids.clone(),
                    frame,
                    degrees: value - rotation,
                },
                Command::Batch(changes) => {
                    let mut factor = [1.0; 2];
                    let mut offset = [0.0; 2];
                    for (axis, change) in changes.into_iter().enumerate() {
                        if let Command::SetValue { value, .. } = change {
                            if scale[axis].abs() < 1e-12 {
                                offset[axis] = value;
                            } else {
                                factor[axis] = value / scale[axis];
                            }
                        }
                    }
                    Command::ScaleLayers {
                        ids: ids.clone(),
                        frame,
                        factor,
                        offset,
                    }
                }
                _ => unreachable!("Only rotation and scale use a selection driver"),
            },
            Self::Anchor {
                id,
                inverse,
                anchor,
            } => {
                let d = inverse.vector(delta);
                Command::SetAnchor {
                    id: *id,
                    frame,
                    x: anchor[0] + d[0],
                    y: anchor[1] + d[1],
                }
            }
            Self::Rotate {
                id,
                rotation,
                angle,
                ..
            } => {
                let value = rotation + angle;
                Command::SetValue {
                    id: *id,
                    property: Property::Rotation,
                    frame,
                    value: if constrained {
                        (value / 15.0).round() * 15.0
                    } else {
                        value
                    },
                }
            }
            Self::Scale {
                id,
                inverse,
                arms,
                scale,
                axes,
            } => {
                let d = inverse.vector(delta);
                let mut next = *scale;
                for axis in 0..2 {
                    if axes[axis] && arms[axis].abs() > 0.001 {
                        next[axis] += d[axis] * 100.0 / arms[axis];
                    }
                }
                if constrained {
                    let axis = (0..2)
                        .filter(|a| axes[*a] && scale[*a].abs() > 0.001 && arms[*a].abs() > 0.001)
                        .max_by(|a, b| {
                            ((next[*a] / scale[*a]) - 1.0)
                                .abs()
                                .total_cmp(&((next[*b] / scale[*b]) - 1.0).abs())
                        });
                    if let Some(a) = axis {
                        let ratio = next[a] / scale[a];
                        next = [scale[0] * ratio, scale[1] * ratio];
                    }
                }
                Command::Batch(
                    [Property::ScaleX, Property::ScaleY]
                        .into_iter()
                        .enumerate()
                        .map(|(i, property)| Command::SetValue {
                            id: *id,
                            property,
                            frame,
                            value: next[i],
                        })
                        .collect(),
                )
            }
        }
    }
}
