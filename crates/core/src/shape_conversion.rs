use super::*;

// Cubic approximation of a quarter circle. Tangents use layer coordinates.
const KAPPA: f64 = 0.5522847498307936;

fn geometry(shape: &Shape, width: f64, height: f64, frame: Frame) -> VectorPath {
    let corner = PathVertex::corner;
    let vertices = match shape.kind {
        ShapeKind::Polygon | ShapeKind::Star => polystar::vertices(
            shape.kind,
            shape.value_at(ShapeParam::Points, frame, 0),
            shape.value_at(ShapeParam::InnerRadius, frame, 0),
            width,
            height,
        )
        .into_iter()
        .map(corner)
        .collect(),
        ShapeKind::Ellipse => {
            let (rx, ry) = (width / 2., height / 2.);
            vec![
                PathVertex {
                    position: [width, ry],
                    incoming: [0., -KAPPA * ry],
                    outgoing: [0., KAPPA * ry],
                },
                PathVertex {
                    position: [rx, height],
                    incoming: [KAPPA * rx, 0.],
                    outgoing: [-KAPPA * rx, 0.],
                },
                PathVertex {
                    position: [0., ry],
                    incoming: [0., KAPPA * ry],
                    outgoing: [0., -KAPPA * ry],
                },
                PathVertex {
                    position: [rx, 0.],
                    incoming: [-KAPPA * rx, 0.],
                    outgoing: [KAPPA * rx, 0.],
                },
            ]
        }
        ShapeKind::Rectangle | ShapeKind::RoundedRectangle => {
            let r = if shape.kind == ShapeKind::RoundedRectangle {
                shape
                    .value_at(ShapeParam::Roundness, frame, 0)
                    .min(width / 2.)
                    .min(height / 2.)
            } else {
                0.
            };
            if r == 0. {
                vec![
                    corner([0., 0.]),
                    corner([width, 0.]),
                    corner([width, height]),
                    corner([0., height]),
                ]
            } else {
                let k = KAPPA * r;
                vec![
                    PathVertex {
                        position: [r, 0.],
                        incoming: [-k, 0.],
                        outgoing: [0., 0.],
                    },
                    PathVertex {
                        position: [width - r, 0.],
                        incoming: [0., 0.],
                        outgoing: [k, 0.],
                    },
                    PathVertex {
                        position: [width, r],
                        incoming: [0., -k],
                        outgoing: [0., 0.],
                    },
                    PathVertex {
                        position: [width, height - r],
                        incoming: [0., 0.],
                        outgoing: [0., k],
                    },
                    PathVertex {
                        position: [width - r, height],
                        incoming: [k, 0.],
                        outgoing: [0., 0.],
                    },
                    PathVertex {
                        position: [r, height],
                        incoming: [0., 0.],
                        outgoing: [-k, 0.],
                    },
                    PathVertex {
                        position: [0., height - r],
                        incoming: [0., k],
                        outgoing: [0., 0.],
                    },
                    PathVertex {
                        position: [0., r],
                        incoming: [0., 0.],
                        outgoing: [0., -k],
                    },
                ]
            }
        }
    };
    VectorPath {
        vertices,
        closed: true,
    }
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::ConvertShapeToPath { id, frame } = command else {
        return None;
    };
    Some((|| {
        if *frame >= state.project.composition.duration {
            return Err("Conversion frame is outside the composition".into());
        }
        let layer = editing::editable(state, *id)?;
        let Content::Shape(shape) = &mut layer.content else {
            return Err("Select a parametric shape layer".into());
        };
        if shape.path.is_some() {
            return Err("Shape already has a Bezier path".into());
        }
        let path = geometry(shape, layer.width, layer.height, *frame);
        if !path.valid() {
            return Err("Converted path is invalid".into());
        }
        shape.path = Some(path);
        shape.path_animation = Default::default();
        for parameter in [
            ShapeParam::Points,
            ShapeParam::Roundness,
            ShapeParam::InnerRadius,
        ] {
            shape.parameters.remove(&parameter);
        }
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(kind: ShapeKind) -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                kind,
                ..Default::default()
            }),
            width: 200.,
            height: 120.,
            name: "Convert".into(),
        })
        .unwrap();
        e
    }

    #[test]
    fn conversion_freezes_geometry_preserves_paint_and_supports_path_animation() {
        for kind in ShapeKind::ALL {
            let mut e = scene(kind);
            let geometry_parameter = match kind {
                ShapeKind::Polygon | ShapeKind::Star => Some(ShapeParam::Points),
                ShapeKind::RoundedRectangle => Some(ShapeParam::Roundness),
                _ => None,
            };
            for p in [Some(ShapeParam::StrokeWidth), geometry_parameter]
                .into_iter()
                .flatten()
            {
                for edit in [
                    TrackEdit::ToggleAnimation { frame: 0 },
                    TrackEdit::Value {
                        frame: 60,
                        value: 12.,
                    },
                ] {
                    e.execute(Command::EditShape {
                        id: 1,
                        parameter: p,
                        edit,
                    })
                    .unwrap();
                }
            }
            let before = e.project().clone();
            let Content::Shape(original) = before.composition().layer(1).unwrap().content() else {
                unreachable!()
            };
            let expected = geometry(original, 200., 120., 30);
            e.execute(Command::ConvertShapeToPath { id: 1, frame: 30 })
                .unwrap();
            let after = e.project().clone();
            let Content::Shape(shape) = e.selected_layer().unwrap().content() else {
                unreachable!()
            };
            assert_eq!(shape.path.as_ref(), Some(&expected));
            assert_eq!(
                shape.parameters.get(&ShapeParam::StrokeWidth),
                original.parameters.get(&ShapeParam::StrokeWidth)
            );
            assert!(!shape.parameters.contains_key(&ShapeParam::Points));
            assert!(!shape.parameters.contains_key(&ShapeParam::Roundness));
            let mut unchanged = before.composition().layer(1).unwrap().clone();
            unchanged.content = Content::Shape(shape.clone());
            assert_eq!(&unchanged, e.selected_layer().unwrap());
            assert_eq!(
                Project::from_json(&after.to_json().unwrap()).unwrap(),
                after
            );
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &after);
            e.execute(Command::AnimatePath {
                id: 1,
                target: PathTarget::Shape,
                edit: TrackEdit::ToggleAnimation { frame: 30 },
            })
            .unwrap();
            let mut moved = expected.clone();
            moved.vertices[0].position[0] += 40.;
            e.execute(Command::EditPath {
                id: 1,
                target: PathTarget::Shape,
                frame: 60,
                path: moved,
            })
            .unwrap();
            let Content::Shape(shape) = e.selected_layer().unwrap().content() else {
                unreachable!()
            };
            let middle = shape.path_animation.at(shape.path.as_ref().unwrap(), 45);
            assert_eq!(
                middle.vertices[0].position[0],
                expected.vertices[0].position[0] + 20.
            );
        }
    }

    #[test]
    fn conversion_rejects_locked_missing_nonshape_and_existing_path_atomically() {
        let mut e = scene(ShapeKind::Star);
        for command in [
            Command::ConvertShapeToPath { id: 100, frame: 0 },
            Command::ConvertShapeToPath {
                id: 1,
                frame: u32::MAX,
            },
        ] {
            let before = e.project().clone();
            assert!(e.execute(command).is_err());
            assert_eq!(e.project(), &before);
        }
        e.current.project.composition.layers[0].locked = true;
        let before = e.project().clone();
        assert!(
            e.execute(Command::ConvertShapeToPath { id: 1, frame: 0 })
                .is_err()
        );
        assert_eq!(e.project(), &before);
        e.current.project.composition.layers[0].locked = false;
        e.execute(Command::ConvertShapeToPath { id: 1, frame: 0 })
            .unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::ConvertShapeToPath { id: 1, frame: 0 })
                .is_err()
        );
        assert_eq!(e.project(), &before);
        e.current.project.composition.layers[0].content = Content::Solid;
        let before = e.project().clone();
        assert!(
            e.execute(Command::ConvertShapeToPath { id: 1, frame: 0 })
                .is_err()
        );
        assert_eq!(e.project(), &before);
    }
}
