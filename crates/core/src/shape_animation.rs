use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ShapeParam {
    StrokeWidth,
    Roundness,
    InnerRadius,
    MiterLimit,
    DashOffset,
    DashLength(u8),
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
            "MiterLimit" => Self::MiterLimit,
            "DashOffset" => Self::DashOffset,
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
            Self::MiterLimit => "Miter Limit",
            Self::DashOffset => "Dash Offset",
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
            Self::MiterLimit => (1., 1024.),
            Self::DashOffset => (-32768., 32768.),
            Self::DashLength(_) => (0., 8192.),
        }
    }
    fn accepts(self, value: f64) -> bool {
        value.is_finite() && (self.bounds().0..=self.bounds().1).contains(&value)
    }
    fn base(self, shape: &Shape) -> f64 {
        match self {
            Self::StrokeWidth => shape.stroke_width,
            Self::Roundness => shape.roundness,
            Self::InnerRadius => shape.inner_radius,
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
    pub fn has_parameter(&self, parameter: ShapeParam) -> bool {
        match parameter {
            ShapeParam::DashLength(index) => (index as usize) < self.stroke_style.dashes.len(),
            _ => true,
        }
    }
    pub fn value_at(&self, parameter: ShapeParam, frame: Frame) -> f64 {
        self.parameters.get(&parameter).map_or_else(
            || parameter.base(self),
            |track| {
                track
                    .value_at(frame)
                    .clamp(parameter.bounds().0, parameter.bounds().1)
            },
        )
    }
    pub(super) fn shape_track_mut(&mut self, parameter: ShapeParam) -> &mut AnimatedProperty {
        let base = parameter.base(self);
        self.parameters
            .entry(parameter)
            .or_insert_with(|| AnimatedProperty::new(base))
    }
    pub(super) fn evaluated(&self, frame: Frame) -> Self {
        let mut shape = self.clone();
        shape.stroke_width = self.value_at(ShapeParam::StrokeWidth, frame);
        shape.roundness = self.value_at(ShapeParam::Roundness, frame);
        shape.inner_radius = self.value_at(ShapeParam::InnerRadius, frame);
        shape.stroke_style.miter_limit = self.value_at(ShapeParam::MiterLimit, frame);
        shape.stroke_style.dash_offset = self.value_at(ShapeParam::DashOffset, frame);
        for (index, length) in shape.stroke_style.dashes.iter_mut().enumerate() {
            *length = self.value_at(ShapeParam::DashLength(index as u8), frame);
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
        let Content::Shape(shape) = &mut layer.content else {
            return Err("Select a shape layer".into());
        };
        if !shape.has_parameter(*parameter) {
            return Err("Dash or gap no longer exists".into());
        }
        time_remap::edit_track(shape.shape_track_mut(*parameter), duration, edit, |v| {
            parameter.accepts(v)
        })
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
