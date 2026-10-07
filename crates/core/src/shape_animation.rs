use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ShapeParam {
    StrokeWidth,
    Roundness,
    InnerRadius,
    Points,
    MiterLimit,
    DashOffset,
    DashLength(u8),
    FillOpacity,
    StrokeOpacity,
    FillRed,
    FillGreen,
    FillBlue,
    StrokeRed,
    StrokeGreen,
    StrokeBlue,
}
impl From<ShapeParam> for String {
    fn from(p: ShapeParam) -> Self {
        match p {
            ShapeParam::DashLength(index) => format!("DashLength{index}"),
            _ => format!("{p:?}"),
        }
    }
}
impl TryFrom<String> for ShapeParam {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(match value.as_str() {
            "StrokeWidth" => Self::StrokeWidth,
            "Roundness" => Self::Roundness,
            "InnerRadius" => Self::InnerRadius,
            "Points" => Self::Points,
            "MiterLimit" => Self::MiterLimit,
            "DashOffset" => Self::DashOffset,
            "FillOpacity" => Self::FillOpacity,
            "StrokeOpacity" => Self::StrokeOpacity,
            "FillRed" => Self::FillRed,
            "FillGreen" => Self::FillGreen,
            "FillBlue" => Self::FillBlue,
            "StrokeRed" => Self::StrokeRed,
            "StrokeGreen" => Self::StrokeGreen,
            "StrokeBlue" => Self::StrokeBlue,
            _ => {
                let index = value
                    .strip_prefix("DashLength")
                    .and_then(|v| v.parse::<u8>().ok())
                    .filter(|i| {
                        (*i as usize) < ShapeStroke::MAX_DASHES && value == format!("DashLength{i}")
                    })
                    .ok_or("Unknown shape parameter")?;
                Self::DashLength(index)
            }
        })
    }
}
impl ShapeParam {
    pub fn label(self) -> String {
        match self {
            Self::StrokeWidth => "Stroke Width",
            Self::Roundness => "Roundness",
            Self::InnerRadius => "Inner Radius %",
            Self::Points => "Points",
            Self::MiterLimit => "Miter Limit",
            Self::DashOffset => "Dash Offset",
            Self::FillOpacity => "Fill Opacity",
            Self::StrokeOpacity => "Stroke Opacity",
            Self::FillRed => "Fill Color · Red",
            Self::FillGreen => "Fill Color · Green",
            Self::FillBlue => "Fill Color · Blue",
            Self::StrokeRed => "Stroke Color · Red",
            Self::StrokeGreen => "Stroke Color · Green",
            Self::StrokeBlue => "Stroke Color · Blue",
            Self::DashLength(index) => {
                return format!(
                    "{} {}",
                    if index % 2 == 0 { "Dash" } else { "Gap" },
                    index / 2 + 1
                );
            }
        }
        .into()
    }
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::StrokeWidth => (0., 1024.),
            Self::Roundness => (0., 8192.),
            Self::InnerRadius => (0., 100.),
            Self::Points => (3., 128.),
            Self::FillOpacity | Self::StrokeOpacity => (0., 100.),
            Self::MiterLimit => (1., 1024.),
            Self::DashOffset => (-32768., 32768.),
            Self::DashLength(_) => (0., 8192.),
            Self::FillRed
            | Self::FillGreen
            | Self::FillBlue
            | Self::StrokeRed
            | Self::StrokeGreen
            | Self::StrokeBlue => (0., 255.),
        }
    }
    fn accepts(self, value: f64) -> bool {
        value.is_finite() && (self.bounds().0..=self.bounds().1).contains(&value)
    }
    fn base(self, shape: &Shape, fill_color: u32) -> f64 {
        match self {
            Self::StrokeWidth => shape.stroke_width,
            Self::FillOpacity => shape.fill_opacity,
            Self::StrokeOpacity => shape.stroke_opacity,
            Self::FillRed => ((fill_color >> 16) & 255) as f64,
            Self::FillGreen => ((fill_color >> 8) & 255) as f64,
            Self::FillBlue => (fill_color & 255) as f64,
            Self::StrokeRed => ((shape.stroke_color >> 16) & 255) as f64,
            Self::StrokeGreen => ((shape.stroke_color >> 8) & 255) as f64,
            Self::StrokeBlue => (shape.stroke_color & 255) as f64,
            Self::Roundness => shape.roundness,
            Self::InnerRadius => shape.inner_radius,
            Self::Points => shape.points as f64,
            Self::MiterLimit => shape.stroke_style.miter_limit,
            Self::DashOffset => shape.stroke_style.dash_offset,
            Self::DashLength(index) => shape
                .stroke_style
                .dashes
                .get(index as usize)
                .copied()
                .unwrap_or(0.),
        }
    }
}
impl Shape {
    pub(super) fn has_paint_opacity(&self) -> bool {
        self.fill_opacity != 100.
            || self.stroke_opacity != 100.
            || self.parameters.contains_key(&ShapeParam::FillOpacity)
            || self.parameters.contains_key(&ShapeParam::StrokeOpacity)
    }
    pub fn has_parameter(&self, parameter: ShapeParam) -> bool {
        match parameter {
            ShapeParam::Points => {
                self.path.is_none() && matches!(self.kind, ShapeKind::Polygon | ShapeKind::Star)
            }
            ShapeParam::DashLength(index) => (index as usize) < self.stroke_style.dashes.len(),
            _ => true,
        }
    }
    pub fn value_at(&self, parameter: ShapeParam, frame: Frame, fill_color: u32) -> f64 {
        self.parameters.get(&parameter).map_or_else(
            || parameter.base(self, fill_color),
            |track| {
                track
                    .value_at(frame)
                    .clamp(parameter.bounds().0, parameter.bounds().1)
            },
        )
    }
    pub(super) fn shape_track_mut(
        &mut self,
        parameter: ShapeParam,
        fill_color: u32,
    ) -> &mut AnimatedProperty {
        let base = parameter.base(self, fill_color);
        self.parameters
            .entry(parameter)
            .or_insert_with(|| AnimatedProperty::new(base))
    }
    pub(super) fn evaluated(&self, frame: Frame, fill_color: u32) -> Self {
        let mut shape = self.clone();
        shape.stroke_width = self.value_at(ShapeParam::StrokeWidth, frame, fill_color);
        shape.fill_opacity = self.value_at(ShapeParam::FillOpacity, frame, fill_color);
        shape.stroke_opacity = self.value_at(ShapeParam::StrokeOpacity, frame, fill_color);
        shape.stroke_color = self.paint_color_at(ShapePaint::Stroke, fill_color, frame);
        shape.roundness = self.value_at(ShapeParam::Roundness, frame, fill_color);
        shape.inner_radius = self.value_at(ShapeParam::InnerRadius, frame, fill_color);
        shape.stroke_style.miter_limit = self.value_at(ShapeParam::MiterLimit, frame, fill_color);
        shape.stroke_style.dash_offset = self.value_at(ShapeParam::DashOffset, frame, fill_color);
        for (index, length) in shape.stroke_style.dashes.iter_mut().enumerate() {
            *length = self.value_at(ShapeParam::DashLength(index as u8), frame, fill_color);
        }
        shape
    }
}
pub(super) fn validate(layer: &Layer, duration: Frame, version: u32) -> Result<(), String> {
    let Content::Shape(shape) = &layer.content else {
        return Ok(());
    };
    if !shape.parameters.is_empty() && version < 38 {
        return Err("Shape property animation requires project version 38".into());
    }
    if version < 40 && shape.has_paint_opacity() {
        return Err("Shape fill/stroke opacity requires project version 40".into());
    }
    if version < 41 && shape.has_paint_color_tracks() {
        return Err("Shape color animation requires project version 41".into());
    }
    if version < 42 && shape.parameters.contains_key(&ShapeParam::Points) {
        return Err("Animated/fractional points require project version 42".into());
    }
    for (&parameter, track) in &shape.parameters {
        if !shape.has_parameter(parameter)
            || (version < 39 && matches!(parameter, ShapeParam::DashLength(_)))
            || !parameter.accepts(track.value)
            || track.keys.len() > 10000
            || track.keys.iter().any(|(f, k)| {
                *f >= duration || !parameter.accepts(k.value) || !k.interpolation.valid()
            })
        {
            return Err("Invalid shape property or keyframe".into());
        }
    }
    Ok(())
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::EditShape {
        id,
        parameter,
        edit,
    } = command
    else {
        return None;
    };
    Some((|| {
        let duration = state.project.composition.duration;
        let layer = editing::editable(state, *id)?;
        let fill_color = layer.color;
        let Content::Shape(shape) = &mut layer.content else {
            return Err("Select a shape layer".into());
        };
        if !shape.has_parameter(*parameter) {
            return Err("Shape parameter is not available on this path".into());
        }
        let track = shape.shape_track_mut(*parameter, fill_color);
        time_remap::edit_track(track, duration, edit, |v| parameter.accepts(v))?;
        // Sampling a curve must retain the bounded value visible in the viewer.
        // Explicit numeric/keyframe edits still use the strict validator above.
        if let TrackEdit::ToggleAnimation { frame } | TrackEdit::ToggleKey { frame } = edit {
            let (min, max) = parameter.bounds();
            track.value = track.value.clamp(min, max);
            if let Some(key) = track.keys.get_mut(frame) {
                key.value = key.value.clamp(min, max);
            }
        }
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape {
                stroke_width: 12.,
                ..Default::default()
            }),
            width: 200.,
            height: 200.,
            name: "Shape".into(),
        })
        .unwrap();
        e
    }
    fn edit(e: &mut Editor, p: ShapeParam, edit: TrackEdit) {
        e.execute(Command::EditTrack {
            id: 1,
            property: PropertyPath::Shape(p),
            edit,
        })
        .unwrap();
    }
    #[test]
    fn sampling_overshooting_shape_curves_keeps_the_visible_bounded_value() {
        for parameter in [
            ShapeParam::StrokeWidth,
            ShapeParam::MiterLimit,
            ShapeParam::DashOffset,
            ShapeParam::FillOpacity,
            ShapeParam::StrokeOpacity,
            ShapeParam::FillRed,
            ShapeParam::StrokeBlue,
        ] {
            for direction in [-2., 3.] {
                let mut e = scene();
                let (min, max) = parameter.bounds();
                let path = PropertyPath::Shape(parameter);
                edit(
                    &mut e,
                    parameter,
                    TrackEdit::Value {
                        frame: 0,
                        value: min + (max - min) / 3.,
                    },
                );
                edit(&mut e, parameter, TrackEdit::ToggleAnimation { frame: 0 });
                edit(
                    &mut e,
                    parameter,
                    TrackEdit::Value {
                        frame: 40,
                        value: min + 2. * (max - min) / 3.,
                    },
                );
                edit(
                    &mut e,
                    parameter,
                    TrackEdit::Interpolate {
                        frame: 0,
                        interpolation: Interpolation::Bezier(Bezier {
                            x1: 1. / 3.,
                            y1: direction,
                            x2: 2. / 3.,
                            y2: direction,
                        }),
                    },
                );
                let before = e.project().clone();
                let expected = if direction < 0. { min } else { max };
                let layer = e.selected_layer().unwrap();
                assert_eq!(layer.track_value(path, 20), Some(expected));
                assert!(!parameter.accepts(layer.track(path).unwrap().value_at(20)));
                for operation in [
                    TrackEdit::ToggleKey { frame: 20 },
                    TrackEdit::ToggleAnimation { frame: 20 },
                ] {
                    edit(&mut e, parameter, operation.clone());
                    let after = e.project().clone();
                    let track = e.selected_layer().unwrap().track(path).unwrap();
                    assert_eq!(track.value_at(20), expected);
                    if matches!(operation, TrackEdit::ToggleKey { .. }) {
                        let original = before.composition().layer(1).unwrap().track(path).unwrap();
                        assert_eq!(track.keys[&0], original.keys[&0]);
                        assert_eq!(track.keys[&40], original.keys[&40]);
                        assert_eq!(track.keys.len(), 3);
                    } else {
                        assert!(track.keys.is_empty());
                        assert_eq!(track.value_at(99), expected);
                    }
                    assert_eq!(
                        Project::from_json(&after.to_json().unwrap()).unwrap(),
                        after
                    );
                    e.undo();
                    assert_eq!(e.project(), &before);
                    e.redo();
                    assert_eq!(e.project(), &after);
                    e.undo();
                }
                assert!(
                    e.execute(Command::EditTrack {
                        id: 1,
                        property: path,
                        edit: TrackEdit::ToggleAnimation { frame: u32::MAX },
                    })
                    .is_err()
                );
                assert_eq!(e.project(), &before);
            }
        }
    }
    #[test]
    fn paint_opacity_is_independent_versioned_and_preserves_history() {
        let mut e = scene();
        let original = e.project().clone();
        let old_json = original.to_json().unwrap();
        assert!(!old_json.contains("fill_opacity"));
        assert!(!old_json.contains("stroke_opacity"));
        assert_eq!(Project::from_json(&old_json).unwrap(), original);
        for (p, end) in [
            (ShapeParam::FillOpacity, 0.),
            (ShapeParam::StrokeOpacity, 40.),
        ] {
            edit(&mut e, p, TrackEdit::ToggleAnimation { frame: 0 });
            let before = e.project().clone();
            edit(
                &mut e,
                p,
                TrackEdit::Value {
                    frame: 40,
                    value: end,
                },
            );
            let after = e.project().clone();
            assert_eq!(
                e.selected_layer()
                    .unwrap()
                    .track_value(PropertyPath::Shape(p), 20),
                Some((100. + end) / 2.)
            );
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &after);
            let mut json: serde_json::Value =
                serde_json::from_str(&after.to_json().unwrap()).unwrap();
            assert_eq!(json["version"], 40);
            assert_eq!(Project::from_json(&json.to_string()).unwrap(), after);
            json["version"] = 39.into();
            assert!(Project::from_json(&json.to_string()).is_err());
        }
        let Content::Shape(shape) = e.selected_layer().unwrap().content() else {
            unreachable!()
        };
        assert_eq!(shape.evaluated(20, 0).fill_opacity, 50.);
        assert_eq!(shape.evaluated(20, 0).stroke_opacity, 70.);
        assert_eq!(shape.stroke_width, 12.);
        assert_eq!(shape.fill_opacity, 100.);
        for p in [ShapeParam::FillOpacity, ShapeParam::StrokeOpacity] {
            let before = e.project().clone();
            for value in [-1., 101., f64::NAN, f64::INFINITY] {
                assert!(
                    e.execute(Command::EditShape {
                        id: 1,
                        parameter: p,
                        edit: TrackEdit::Value { frame: 20, value }
                    })
                    .is_err()
                );
                assert_eq!(e.project(), &before);
            }
            edit(&mut e, p, TrackEdit::ToggleAnimation { frame: 20 });
            assert_eq!(
                e.selected_layer()
                    .unwrap()
                    .track_value(PropertyPath::Shape(p), 99),
                Some(if p == ShapeParam::FillOpacity {
                    50.
                } else {
                    70.
                })
            );
        }
        e.execute(Command::ToggleLocked(1)).unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::EditShape {
                id: 1,
                parameter: ShapeParam::FillOpacity,
                edit: TrackEdit::ToggleAnimation { frame: 0 }
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }

    #[test]
    fn static_paint_opacity_validates_versions_and_default_compatibility() {
        let mut e = scene();
        let Content::Shape(mut shape) = e.selected_layer().unwrap().content().clone() else {
            unreachable!()
        };
        shape.fill_opacity = 25.;
        shape.stroke_opacity = 0.;
        e.execute(Command::SetContent {
            id: 1,
            content: Content::Shape(shape.clone()),
        })
        .unwrap();
        let saved = e.project().clone();
        let mut json: serde_json::Value = serde_json::from_str(&saved.to_json().unwrap()).unwrap();
        assert_eq!(json["version"], 40);
        assert_eq!(Project::from_json(&json.to_string()).unwrap(), saved);
        json["version"] = 39.into();
        assert!(Project::from_json(&json.to_string()).is_err());
        for value in [-1., 101., f64::INFINITY] {
            shape.stroke_opacity = value;
            assert!(
                e.execute(Command::SetContent {
                    id: 1,
                    content: Content::Shape(shape.clone())
                })
                .is_err()
            );
            assert_eq!(e.project(), &saved);
        }
    }

    #[test]
    fn shape_tracks_interpolate_preserve_geometry_and_roundtrip_history() {
        let mut e = scene();
        let before = e.project().clone();
        for (p, value) in [
            (ShapeParam::StrokeWidth, 52.),
            (ShapeParam::Roundness, 80.),
            (ShapeParam::InnerRadius, 20.),
            (ShapeParam::MiterLimit, 8.),
            (ShapeParam::DashOffset, -100.),
        ] {
            let base = e
                .selected_layer()
                .unwrap()
                .track_value(PropertyPath::Shape(p), 0)
                .unwrap();
            edit(&mut e, p, TrackEdit::ToggleAnimation { frame: 0 });
            let old = e.project().clone();
            edit(&mut e, p, TrackEdit::Value { frame: 40, value });
            let saved = e.project().clone();
            assert_eq!(
                e.selected_layer()
                    .unwrap()
                    .track_value(PropertyPath::Shape(p), 20),
                Some((base + value) / 2.)
            );
            e.undo();
            assert_eq!(e.project(), &old);
            e.redo();
            assert_eq!(e.project(), &saved);
            assert_eq!(
                Project::from_json(&saved.to_json().unwrap()).unwrap(),
                saved
            );
        }
        let Content::Shape(shape) = e.selected_layer().unwrap().content() else {
            unreachable!()
        };
        assert_eq!(shape.stroke_width, 12.);
        assert_eq!(shape.path, None);
        assert_eq!(
            e.selected_layer().unwrap().properties,
            before.composition().layer(1).unwrap().properties
        );
        let mut json: serde_json::Value =
            serde_json::from_str(&e.project().to_json().unwrap()).unwrap();
        assert_eq!(json["version"], 38);
        json["version"] = 37.into();
        assert!(Project::from_json(&json.to_string()).is_err());
        edit(
            &mut e,
            ShapeParam::StrokeWidth,
            TrackEdit::ToggleAnimation { frame: 20 },
        );
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track_value(PropertyPath::Shape(ShapeParam::StrokeWidth), 99),
            Some(32.)
        );
    }
    #[test]
    fn shape_keys_shift_copy_scale_and_delete_through_common_tracks() {
        let mut e = scene();
        let p = ShapeParam::StrokeWidth;
        let path = PropertyPath::Shape(p);
        edit(&mut e, p, TrackEdit::ToggleAnimation { frame: 0 });
        edit(
            &mut e,
            p,
            TrackEdit::Value {
                frame: 20,
                value: 32.,
            },
        );
        edit(
            &mut e,
            p,
            TrackEdit::Interpolate {
                frame: 0,
                interpolation: Interpolation::Hold,
            },
        );
        assert_eq!(e.selected_layer().unwrap().track_value(path, 10), Some(12.));
        let copy = e.selected_layer().unwrap().copy_key(path, 20).unwrap();
        e.execute(Command::PasteKeys {
            keys: vec![copy],
            frame: 40,
            target: None,
        })
        .unwrap();
        e.execute(Command::SetLayerRange {
            id: 1,
            start: 0,
            end: 100,
        })
        .unwrap();
        e.execute(Command::ShiftLayer { id: 1, delta: 5 }).unwrap();
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track(path)
                .unwrap()
                .keys()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![5, 25, 45]
        );
        e.execute(Command::ScaleKeys {
            keys: vec![KeyRef {
                id: 1,
                property: path,
                frame: 25,
            }],
            scale: KeyScale {
                time_origin: 5.,
                time_scale: 1.5,
                value_origin: 12.,
                value_scale: 2.,
            },
        })
        .unwrap();
        assert_eq!(e.selected_layer().unwrap().track_value(path, 35), Some(52.));
        for frame in [5, 45, 35] {
            e.execute(Command::DeleteKeys(vec![KeyRef {
                id: 1,
                property: path,
                frame,
            }]))
            .unwrap();
        }
        assert_eq!(e.selected_layer().unwrap().track_value(path, 0), Some(52.));
        assert!(
            e.selected_layer()
                .unwrap()
                .track(path)
                .unwrap()
                .keys()
                .is_empty()
        );
        e.execute(Command::AddContent {
            content: Content::Shape(Shape::default()),
            width: 100.,
            height: 100.,
            name: "Target".into(),
        })
        .unwrap();
        let mut source = scene();
        edit(&mut source, p, TrackEdit::ToggleKey { frame: 0 });
        e.execute(Command::PasteKeys {
            keys: vec![source.selected_layer().unwrap().copy_key(path, 0).unwrap()],
            frame: 10,
            target: Some(2),
        })
        .unwrap();
        assert_eq!(
            e.project()
                .composition()
                .layer(2)
                .unwrap()
                .track_value(path, 10),
            Some(12.)
        );
    }
    #[test]
    fn shape_animation_rejects_bad_values_times_and_locks_atomically() {
        let mut e = scene();
        let before = e.project().clone();
        for (p, value) in [
            (ShapeParam::StrokeWidth, -1.),
            (ShapeParam::Roundness, 8193.),
            (ShapeParam::InnerRadius, 101.),
            (ShapeParam::MiterLimit, 0.),
            (ShapeParam::DashOffset, 32769.),
            (ShapeParam::StrokeWidth, f64::NAN),
        ] {
            assert!(
                e.execute(Command::EditShape {
                    id: 1,
                    parameter: p,
                    edit: TrackEdit::Value { frame: 0, value }
                })
                .is_err()
            );
            assert_eq!(e.project(), &before);
        }
        assert!(
            e.execute(Command::EditShape {
                id: 1,
                parameter: ShapeParam::StrokeWidth,
                edit: TrackEdit::ToggleKey {
                    frame: e.project().composition().duration
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
        e.execute(Command::ToggleLocked(1)).unwrap();
        assert!(
            e.execute(Command::EditShape {
                id: 1,
                parameter: ShapeParam::StrokeWidth,
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 20.
                }
            })
            .is_err()
        );
    }

    #[test]
    fn dash_track_addresses_roundtrip_and_reject_missing_or_old_version_tracks() {
        for p in [
            ShapeParam::StrokeWidth,
            ShapeParam::Roundness,
            ShapeParam::InnerRadius,
            ShapeParam::MiterLimit,
            ShapeParam::DashOffset,
        ] {
            let expected = format!("\"{p:?}\"");
            assert_eq!(serde_json::to_string(&p).unwrap(), expected);
            assert_eq!(serde_json::from_str::<ShapeParam>(&expected).unwrap(), p);
        }
        for index in 0..16 {
            let p = ShapeParam::DashLength(index);
            assert_eq!(
                serde_json::from_str::<ShapeParam>(&serde_json::to_string(&p).unwrap()).unwrap(),
                p
            );
        }
        for invalid in [
            "DashLength16",
            "DashLength255",
            "DashLength-1",
            "DashLength01",
            "DashLength",
            "DashLength1.0",
        ] {
            assert!(serde_json::from_str::<ShapeParam>(&format!("\"{invalid}\"")).is_err());
        }
        let mut e = scene();
        let original = e.project().clone();
        let path = PropertyPath::Shape(ShapeParam::DashLength(0));
        assert_eq!(e.selected_layer().unwrap().track_value(path, 0), None);
        assert!(
            e.execute(Command::EditTrack {
                id: 1,
                property: path,
                edit: TrackEdit::ToggleAnimation { frame: 0 }
            })
            .is_err()
        );
        assert_eq!(e.project(), &original);
        let Content::Shape(mut shape) = e.selected_layer().unwrap().content().clone() else {
            unreachable!()
        };
        shape.stroke_style.dashes = vec![20., 40.];
        e.execute(Command::SetContent {
            id: 1,
            content: Content::Shape(shape),
        })
        .unwrap();
        edit(
            &mut e,
            ShapeParam::DashLength(0),
            TrackEdit::ToggleAnimation { frame: 0 },
        );
        edit(
            &mut e,
            ShapeParam::DashLength(0),
            TrackEdit::Value {
                frame: 20,
                value: 60.,
            },
        );
        assert_eq!(e.selected_layer().unwrap().track_value(path, 10), Some(40.));
        let json = e.project().to_json().unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), *e.project());
        let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(old["version"], 39);
        old["version"] = 38.into();
        assert!(Project::from_json(&old.to_string()).is_err());
        let saved = e.project().clone();
        let Content::Shape(mut shape) = e.selected_layer().unwrap().content().clone() else {
            unreachable!()
        };
        shape.stroke_style.dashes.clear();
        assert!(
            e.execute(Command::SetContent {
                id: 1,
                content: Content::Shape(shape)
            })
            .is_err()
        );
        assert_eq!(e.project(), &saved);
        let copy = e.selected_layer().unwrap().copy_key(path, 20).unwrap();
        e.execute(Command::AddContent {
            content: Content::Shape(Shape::default()),
            width: 100.,
            height: 100.,
            name: "No dashes".into(),
        })
        .unwrap();
        let before = e.project().clone();
        assert!(
            e.execute(Command::PasteKeys {
                keys: vec![copy],
                frame: 40,
                target: Some(2)
            })
            .is_err()
        );
        assert_eq!(e.project(), &before);
    }
}
