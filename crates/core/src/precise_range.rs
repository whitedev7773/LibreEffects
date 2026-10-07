//! Source endpoints can precede frame zero or fall between composition frames.
//! The integer range is their clipped projection for timeline interaction only.
use super::*;

impl Layer {
    pub fn in_frame_sample(&self) -> f64 {
        self.precise_range
            .map_or(f64::from(self.in_frame), |r| r[0])
    }
    pub fn out_frame_sample(&self, duration: Frame) -> f64 {
        self.precise_range
            .map_or(f64::from(self.out_frame(duration)), |r| r[1])
    }
    pub fn active_at_sample(&self, frame: f64, duration: Frame) -> bool {
        self.visible
            && frame.is_finite()
            && frame >= 0.0
            && frame < f64::from(duration)
            && frame >= self.in_frame_sample()
            && frame < self.out_frame_sample(duration)
    }
    pub(super) fn rescale_precise_range(&mut self, ratio: f64, duration: Frame) {
        if let Some(range) = &mut self.precise_range {
            for endpoint in range {
                *endpoint *= ratio;
            }
            self.project_precise_range(duration);
        }
    }
    pub(super) fn project_precise_range(&mut self, duration: Frame) {
        if let Some([start, end]) = self.precise_range {
            self.in_frame = start.max(0.0).ceil() as Frame;
            self.out_frame = Some(end.min(f64::from(duration)).ceil() as Frame);
        }
    }
}

fn wide_position(layer: &Layer) -> bool {
    [Property::PositionX, Property::PositionY]
        .into_iter()
        .any(|p| {
            layer.properties.get(&p).is_some_and(|track| {
                track.value.abs() > 1_000_000.0
                    || track.keys.values().any(|key| key.value.abs() > 1_000_000.0)
            })
        })
}
pub(super) fn materialized(project: &Project) -> bool {
    project.compositions().iter().any(|(_, comp)| {
        comp.layers.iter().any(|l| {
            l.precise_range.is_some()
                || wide_position(l)
                || matches!(&l.content, Content::ShapeContents(contents) if contents.has_centered_parametrics())
                || l.path_masks.iter().any(|m| {
                    m.parameters.get(&MaskParam::Feather).is_some_and(|t| {
                        t.value > 256.0 || t.keys.values().any(|k| k.value > 256.0)
                    })
                })
        })
    })
}
pub(super) fn validate(layer: &Layer, version: u32, duration: Frame) -> Result<(), String> {
    if version < 81 && (layer.precise_range.is_some() || wide_position(layer)) {
        return Err(
            "Precise layer ranges and extended authored Position require project version 81".into(),
        );
    }
    if let Some([start, end]) = layer.precise_range {
        if !start.is_finite()
            || !end.is_finite()
            || start.abs() > 100_000_000.0
            || start >= end
            || end <= 0.0
            || end > 100_000_000.0
            || layer.in_frame != start.max(0.0).ceil() as Frame
            || layer.out_frame != Some(end.min(f64::from(duration)).ceil() as Frame)
        {
            return Err("Precise layer range needs finite ordered endpoints and its exact clipped integer projection".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let mut project = editor.project().clone();
        project.version = 81;
        let layer = &mut project.composition.layers[0];
        layer.precise_range = Some([-30.0, 100.25]);
        layer.project_precise_range(project.composition.duration);
        project
    }

    #[test]
    fn sample_range_edits_validate_atomically_and_preserve_history() {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let before = editor.project().clone();
        editor
            .execute(Command::SetLayerRangeSamples {
                id: 1,
                start: -30.25,
                end: 600.5,
            })
            .unwrap();
        let saved = editor.project().clone();
        assert_eq!(saved.version, 81);
        assert_eq!(saved.composition.layers[0].in_frame_sample(), -30.25);
        assert_eq!(saved.composition.layers[0].out_frame_sample(150), 600.5);
        for (start, end) in [
            (f64::NAN, 100.0),
            (0.0, f64::INFINITY),
            (-100_000_001.0, 100.0),
            (150.0, 200.0),
            (30.0, 30.0),
            (-30.0, -1.0),
        ] {
            assert!(
                editor
                    .execute(Command::SetLayerRangeSamples { id: 1, start, end })
                    .is_err()
            );
            assert_eq!(editor.project(), &saved);
        }
        editor.undo();
        assert_eq!(editor.project(), &before);
        editor.redo();
        assert_eq!(editor.project(), &saved);
    }

    #[test]
    fn source_out_point_beyond_composition_is_preserved_without_extending_output() {
        let mut p = project();
        p.composition.layers[0].precise_range = Some([0.0, 600.5]);
        p.composition.layers[0].project_precise_range(p.composition.duration);
        p.validate().unwrap();
        let layer = &p.composition.layers[0];
        assert_eq!(
            layer.out_frame(p.composition.duration),
            p.composition.duration
        );
        assert_eq!(layer.out_frame_sample(300), 600.5);
        assert!(!layer.active_at_sample(300.0, 300));
        assert_eq!(
            p.expression_snapshot(p.active_composition_id(), 0)
                .unwrap()
                .layers[0]
                .out_point,
            600.5 / 30.0
        );
        assert_eq!(
            project_file::decode(&project_file::encode(&p, None).unwrap())
                .unwrap()
                .project,
            p
        );
        let mut editor = Editor::default();
        editor.replace_project(p.clone()).unwrap();
        editor
            .execute(Command::ShiftLayer { id: 1, delta: -30 })
            .unwrap();
        assert_eq!(editor.selected_layer().unwrap().in_frame_sample(), -30.0);
        assert_eq!(
            editor.selected_layer().unwrap().out_frame_sample(300),
            570.5
        );
        editor.undo();
        assert_eq!(editor.project(), &p);
        editor
            .execute(Command::ConfigureComposition {
                name: "Extended canvas time".into(),
                width: 1920,
                height: 1080,
                fps: 30,
                duration: 900,
            })
            .unwrap();
        assert_eq!(editor.selected_layer().unwrap().out_frame(900), 601);
        assert_eq!(
            editor.selected_layer().unwrap().out_frame_sample(900),
            600.5
        );
        editor.undo();
        assert_eq!(editor.project(), &p);
    }

    #[test]
    fn proportional_text_metrics_survive_source_edits_history_and_native_reopening() {
        let text = "ABC";
        let style = TextCharacterStyle::from_style(&TextStyle::default(), 46.0, 0xffffff);
        let mut rich = RichText::new(
            text,
            style.clone(),
            vec![TextStyleRun {
                start: 0,
                end: text.len(),
                style,
            }],
        )
        .unwrap();
        rich.proportional_metrics = true;
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: text.into(),
                    font_size: 46.0,
                },
                width: 100.0,
                height: 100.0,
                name: "Proportional metrics".into(),
            })
            .unwrap();
        editor
            .execute(Command::SetRichText {
                id: 1,
                rich_text: Some(rich.clone()),
            })
            .unwrap();
        let original = editor.project().clone();
        assert_eq!(original.version, 81);
        let mut old = original.clone();
        old.version = 80;
        assert!(old.validate().is_err());
        let (new_source, edited) = rich.replace_range(text, 1..2, "XY").unwrap();
        assert!(edited.proportional_metrics);
        editor
            .execute(Command::SetStyledText {
                id: 1,
                text: new_source,
                rich_text: edited,
            })
            .unwrap();
        let saved = editor.project().clone();
        assert_eq!(
            project_file::decode(&project_file::encode(&saved, None).unwrap())
                .unwrap()
                .project,
            saved
        );
        editor.undo();
        assert_eq!(editor.project(), &original);
        editor.redo();
        assert_eq!(editor.project(), &saved);
        assert!(
            !RichText::new("", rich.default_style.clone(), vec![])
                .unwrap()
                .proportional_metrics
        );
    }

    #[test]
    fn signed_fractional_endpoints_roundtrip_and_sample_without_rounding() {
        let p = project();
        p.validate().unwrap();
        let l = &p.composition.layers[0];
        assert_eq!((l.in_frame(), l.out_frame(300)), (0, 101));
        assert_eq!(
            (l.in_frame_sample(), l.out_frame_sample(300)),
            (-30.0, 100.25)
        );
        assert!(l.active_at(100, 300));
        assert!(l.active_at_sample(100.249, 300));
        assert!(!l.active_at_sample(100.25, 300));
        assert!(!l.active_at_sample(-0.1, 300));
        let snapshot = p.expression_snapshot(p.active_composition_id(), 0).unwrap();
        assert_eq!(snapshot.layers[0].in_point, -1.0);
        let bytes = project_file::encode(&p, None).unwrap();
        assert_eq!(project_file::decode(&bytes).unwrap().project, p);
        let mut invalid = p.clone();
        invalid.version = 80;
        assert!(invalid.validate().unwrap_err().contains("version 81"));
        invalid.version = 81;
        invalid.composition.layers[0].out_frame = Some(100);
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn extended_authored_position_remains_exact_and_requires_new_schema() {
        let mut p = project();
        p.composition.layers[0]
            .properties
            .get_mut(&Property::PositionY)
            .unwrap()
            .value = 1_250_000_000_000.25;
        p.validate().unwrap();
        assert_eq!(
            p.expression_snapshot(p.active_composition_id(), 0)
                .unwrap()
                .layers[0]
                .position
                .authored_value,
            expression_runtime::PropertyValue::Vector2([960.0, 1_250_000_000_000.25])
        );
        assert_eq!(Project::from_json(&p.to_json().unwrap()).unwrap(), p);
        p.version = 80;
        assert!(p.validate().is_err());
        assert!(!Property::PositionY.accepts(f64::INFINITY));
        assert!(!Property::PositionY.accepts(2e15 + 1.0));
        assert!(!Property::AnchorY.accepts(1_000_001.0));
    }

    #[test]
    fn move_split_trim_and_history_preserve_precise_endpoints() {
        let original = project();
        let mut e = Editor::default();
        e.replace_project(original.clone()).unwrap();
        e.execute(Command::ShiftLayer { id: 1, delta: 30 }).unwrap();
        let moved = e.project().composition().layer(1).unwrap();
        assert_eq!(
            (moved.in_frame_sample(), moved.out_frame_sample(300)),
            (0.0, 130.25)
        );
        e.undo();
        assert_eq!(e.project(), &original);
        e.redo();
        e.execute(Command::SplitLayers {
            ids: vec![1],
            frame: 60,
        })
        .unwrap();
        let left = e.project().composition().layer(1).unwrap();
        let right = e.project().composition().layer(2).unwrap();
        assert_eq!(
            (left.in_frame_sample(), left.out_frame_sample(300)),
            (0.0, 60.0)
        );
        assert_eq!(
            (right.in_frame_sample(), right.out_frame_sample(300)),
            (60.0, 130.25)
        );
        e.execute(Command::SetLayerRange {
            id: 2,
            start: 70,
            end: 120,
        })
        .unwrap();
        assert!(
            e.project()
                .composition()
                .layer(2)
                .unwrap()
                .precise_range
                .is_none()
        );
        e.undo();
        assert_eq!(
            e.project()
                .composition()
                .layer(2)
                .unwrap()
                .out_frame_sample(300),
            130.25
        );
    }

    #[test]
    fn extended_feather_is_bounded_and_preserved_in_history_and_native_file() {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        let mask = PathMask {
            path: VectorPath {
                closed: true,
                vertices: vec![
                    PathVertex::corner([0.0, 0.0]),
                    PathVertex::corner([100.0, 0.0]),
                    PathVertex::corner([0.0, 100.0]),
                ],
            },
            ..Default::default()
        };
        e.execute(Command::SetPathMasks {
            id: 1,
            masks: vec![mask],
        })
        .unwrap();
        let before = e.project().clone();
        e.execute(Command::EditMask {
            id: 1,
            mask: 1,
            parameter: MaskParam::Feather,
            edit: TrackEdit::Value {
                frame: 0,
                value: 700.0,
            },
        })
        .unwrap();
        assert_eq!(e.project().version, 81);
        let saved = e.project().clone();
        let bytes = project_file::encode(&saved, None).unwrap();
        assert_eq!(project_file::decode(&bytes).unwrap().project, saved);
        let mut old = saved.clone();
        old.version = 80;
        assert!(old.validate().is_err());
        assert!(
            e.execute(Command::EditMask {
                id: 1,
                mask: 1,
                parameter: MaskParam::Feather,
                edit: TrackEdit::Value {
                    frame: 0,
                    value: 2049.0
                }
            })
            .is_err()
        );
        assert_eq!(e.project(), &saved);
        e.undo();
        assert_eq!(e.project(), &before);
        e.redo();
        assert_eq!(e.project(), &saved);
    }
}
